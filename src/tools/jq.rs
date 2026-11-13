//! Common response projection for every MCP tool. Keep jq out of business
//! parameters: it controls only the response, never an upstream request.
use jaq_core::{
    Ctx, Vars, data,
    load::{Arena, File, Loader},
};
use jaq_json::Val;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolRequestParams, CallToolResult, Content, Tool},
};
use serde_json::Value;

use crate::tools::jq_sandbox::{self, Reply, SandboxError};

type Filter = jaq_core::Filter<data::JustLut<Val>>;
pub(super) const INSTRUCTIONS: &str = "All tools accept optional `_jq`, a filter expression using jq CLI syntax applied to the response; one output is returned as-is, several as a JSON array. Response shapes differ per tool (some are a top-level array, some an object; most have no `data` wrapper) and most tool descriptions say what they return. If unsure, probe cheaply with keys (object) or .[0] | keys (array) rather than fetching the full response. Omit it for the full JSON response.";

/// Per-tool schema description for the injected `_jq` property, so a client
/// inspecting a single tool's schema learns the contract without depending on
/// the server-level `instructions` (which many clients drop).
const JQ_PROPERTY_DESCRIPTION: &str = "Optional jq filter applied to this tool's JSON response; it never changes the request, and several outputs come back as an array. For field names see the tool description, or probe with `keys` / `.[0] | keys`.";
const MAX_RESULTS: usize = 10_000;
/// Longest accepted `_jq`. jaq's parser recurses once per nesting level, so
/// this bounds the recursion depth of the in-server validation compile (see
/// [`validate`]); real filters are a few hundred characters.
const MAX_CODE_CHARS: usize = 4096;
/// Stack for the validation compile. A 1,000-deep `((((…))))` overflowed a
/// 2 MiB tokio worker stack and aborted the server; within
/// [`MAX_CODE_CHARS`] this leaves a wide margin (checked by tests).
const VALIDATE_STACK_BYTES: usize = 64 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Cap on the jaq error text echoed back. jaq embeds the offending value in
/// its message (`cannot index [<whole response>] with "data"`), which can be
/// tens of KB of upstream data the caller never asked to see.
const MAX_ERROR_CHARS: usize = 200;
/// Depth/width limits for [`shape_of`], keeping the summary to a few hundred
/// bytes even for large nested responses.
const SHAPE_MAX_DEPTH: usize = 3;
const SHAPE_MAX_KEYS: usize = 25;
/// jq builtins (definitions and natives) that `_jq` filters must not reach.
///
/// - `env`: the server environment contains credentials.
/// - `debug`/`stderr` and their `*_empty` natives: they write the filter input
///   (upstream account data) into the server log at ERROR level.
/// - `halt`/`halt_error`: jaq turns a halt into `std::process::exit`, so a
///   single `_jq: "halt_error(1)"` terminated the whole server process.
const BLOCKED_BUILTINS: &[&str] = &[
    "env",
    "debug",
    "debug_empty",
    "stderr",
    "stderr_empty",
    "halt",
    "halt_error",
];

pub(super) fn describe(tool: &mut Tool) {
    let schema = std::sync::Arc::make_mut(&mut tool.input_schema);
    schema
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("tool properties must be an object")
        .insert(
            "_jq".into(),
            serde_json::json!({"type": "string", "description": JQ_PROPERTY_DESCRIPTION}),
        );
    // A jq projection may return any JSON value. A fixed object schema would
    // reject valid filtered results. Full unfiltered schemas remain resources.
    //
    // Not replaced by a shape summary derived from it: most hand-written
    // output schemas have drifted from the real responses, and advertising
    // their field names would steer `_jq` filters wrong. A failed filter
    // reports the real response's shape instead (see `shape_of`).
    tool.output_schema = None;
}

fn compile(code: &str) -> Result<Filter, McpError> {
    if code.trim().is_empty() {
        return Err(McpError::invalid_params(
            "_jq must be a non-empty expression",
            None,
        ));
    }
    let arena = Arena::default();
    // Loader::new rejects imports/includes; do not give filters filesystem access.
    let modules = Loader::new(
        jaq_core::defs()
            .chain(jaq_std::defs().filter(|def| !BLOCKED_BUILTINS.contains(&def.name)))
            .chain(jaq_json::defs()),
    )
    .load(&arena, File { code, path: () })
    .map_err(|errors| {
        let details: Vec<String> = errors
            .iter()
            .flat_map(|(_, error)| load_error_details(code, error))
            .collect();
        McpError::invalid_params(
            format!("Invalid _jq expression: {}", details.join("; ")),
            None,
        )
    })?;
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|(name, _, _)| !BLOCKED_BUILTINS.contains(name));
    jaq_core::Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errors| {
            let details: Vec<String> = errors
                .iter()
                .flat_map(|(_, undefined)| undefined)
                .map(|(name, kind)| undefined_message(name, kind.as_str()))
                .collect();
            McpError::invalid_params(
                format!("Invalid _jq expression: {}", details.join("; ")),
                None,
            )
        })
}

/// Check that `code` compiles, without running it. This is the only jaq code
/// that runs inside the server process, so it is bounded: a length cap, then
/// the compile on a thread whose stack fits the deepest nesting that length
/// allows.
fn validate(code: &str) -> Result<(), String> {
    if code.chars().count() > MAX_CODE_CHARS {
        return Err(format!(
            "_jq is longer than {MAX_CODE_CHARS} characters; use a shorter expression"
        ));
    }
    let code = code.to_owned();
    std::thread::Builder::new()
        .name("jq-validate".into())
        .stack_size(VALIDATE_STACK_BYTES)
        .spawn(move || {
            compile(&code)
                .map(|_| ())
                .map_err(|error| error.message.into_owned())
        })
        .map_err(|error| format!("_jq validation failed: {error}"))?
        .join()
        .unwrap_or_else(|_| Err("_jq validation failed".into()))
}

/// Compile error text for an undefined name. A blocked builtin gets an
/// explicit "disabled" message instead of the bare `undefined filter`, so an
/// agent stops trying variants (`halt`, `halt_error(1)`, `stderr`, ...) of a
/// capability that does not exist here.
fn undefined_message(name: &str, kind: &str) -> String {
    let bare = name.trim_start_matches('$');
    if bare == "ENV" || bare == "__prog_args" {
        return format!(
            "`{name}` is disabled in _jq: the server environment is not exposed to filters"
        );
    }
    if BLOCKED_BUILTINS.contains(&bare) {
        return format!(
            "`{name}` is disabled in _jq: filters may only transform the response, so env, \
             debug, stderr and halt are not available; put what you need in the output, or \
             use error(\"message\") to fail"
        );
    }
    format!("undefined {kind} `{name}`")
}

/// 1-based character column of `part` within `code`, when `part` is a slice of
/// it (jaq reports error positions as slices of the source).
fn column_of(code: &str, part: &str) -> Option<usize> {
    let offset = (part.as_ptr() as usize).checked_sub(code.as_ptr() as usize)?;
    (offset <= code.len()).then(|| code[..offset].chars().count() + 1)
}

/// Readable descriptions of a jaq load (lex/parse) error, instead of its
/// `Debug` dump (`[(File { code: …, path: () }, Lex([(Delim("("), "")]))]`).
fn load_error_details(code: &str, error: &jaq_core::load::Error<&str>) -> Vec<String> {
    use jaq_core::load::Error;
    let at = |found: &str| match (column_of(code, found), found.is_empty()) {
        (_, true) => "at end of expression".to_string(),
        (Some(column), false) => format!(
            "at column {column} (near `{}`)",
            found.chars().take(12).collect::<String>()
        ),
        (None, false) => format!("near `{found}`"),
    };
    match error {
        Error::Io(_) => vec!["imports and includes are not allowed".to_string()],
        Error::Lex(errors) => errors
            .iter()
            .map(|(expect, found)| format!("expected {} {}", expect.as_str(), at(found)))
            .collect(),
        Error::Parse(errors) => errors
            .iter()
            .map(|(expect, found)| format!("expected {} {}", expect.as_str(), at(found)))
            .collect(),
    }
}

/// Apply the request's `_jq` (if any) to the result of `invoke`.
///
/// `caller` keys the per-caller worker limit (see [`jq_sandbox::caller_key`]).
/// For a tool that is not `read_only`, the worker slot is reserved *before*
/// the tool runs: a saturated sandbox then rejects the call up front instead
/// of discarding the result of a write that already happened.
pub(super) async fn call<F, Fut>(
    mut request: CallToolRequestParams,
    caller: &str,
    read_only: bool,
    invoke: F,
) -> Result<CallToolResult, McpError>
where
    F: FnOnce(CallToolRequestParams) -> Fut,
    Fut: std::future::Future<Output = Result<CallToolResult, McpError>>,
{
    let code = request
        .arguments
        .as_mut()
        .and_then(|args| args.remove("_jq"));
    let code = match code {
        None | Some(Value::Null) => return invoke(request).await,
        Some(Value::String(code)) => code,
        Some(_) => {
            let message = "_jq must be a string";
            record_failure(&request.name, "compile", "", message);
            return Ok(syntax_error(message));
        }
    };
    // Compile before any tool execution, particularly before trade writes.
    // The worker compiles again; this pass only validates.
    if let Err(message) = validate(&code) {
        record_failure(&request.name, "compile", &code, &message);
        return Ok(syntax_error(&message));
    }
    let name = request.name.clone();
    let reserved = if read_only {
        None
    } else {
        match jq_sandbox::acquire(caller).await {
            Ok(slot) => Some(slot),
            Err(error) => {
                record_failure(&name, "busy", &code, &error.to_string());
                return Ok(sandbox_busy(false));
            }
        }
    };
    let mut result = invoke(request).await?;
    if result.is_error == Some(true) || is_error_envelope(&result) {
        return Ok(result);
    }
    let slot = match reserved {
        Some(slot) => slot,
        None => match jq_sandbox::acquire(caller).await {
            Ok(slot) => slot,
            Err(error) => {
                // Only read-only tools get here, so retrying is safe.
                record_failure(&name, "busy", &code, &error.to_string());
                return Ok(sandbox_busy(true));
            }
        },
    };
    let input = take_input(&mut result);
    let shape = shape_of(&input);
    // The filter runs in a separate, resource-limited worker process. jaq has
    // no step/memory/recursion limit, so in-process evaluation let a single
    // `_jq` abort the server (`def f: 1 + f; f` overflows the stack,
    // `[range(1e9)]` exhausts memory). See `jq_sandbox`.
    let (stage, message) = match jq_sandbox::run(slot, &code, input).await {
        Ok(Reply::Values(values)) => return Ok(finish(result, values)),
        Ok(Reply::FilterError(message)) => {
            ("runtime", format!("{message}; response shape: {shape}"))
        }
        Ok(Reply::Error(message)) => ("runtime", message),
        Err(error @ SandboxError::Timeout) => ("runtime", error.to_string()),
        Err(error) => {
            // Not a caller typo: the filter crashed or the sandbox is saturated
            // (often deliberate probing), so make it stand out from INFO noise.
            tracing::warn!(tool = %crate::tools::tool_label(&name), %error, "jq sandbox failure");
            ("sandbox", error.to_string())
        }
    };
    record_failure(&name, stage, &code, &message);
    Ok(filter_error(&message))
}

/// Count and log a failed `_jq` filter. These calls reach the client as
/// `isError:true`, but run outside `measured_tool_call`, so without this they
/// would be invisible in both `mcp_tool_call_errors_total` and the logs.
fn record_failure(tool: &str, stage: &str, code: &str, message: &str) {
    // The name comes from the client and the tool may not exist (a compile
    // failure is reported before routing): label unknown names as one bucket.
    let tool = super::tool_label(tool);
    crate::metrics::record_jq_error(tool, stage);
    let client = crate::metrics::CURRENT_CLIENT
        .try_with(|c| *c)
        .unwrap_or("unknown");
    // INFO, not WARN: a bad filter is a caller mistake, like INVALID_PARAMS.
    tracing::info!(tool, client, stage, "tool call jq filter failed");
    // A runtime error quotes the response it ran on (`cannot index [{…}]`),
    // which for account tools is the user's positions or orders, and the
    // expression may embed argument values. Both go to the payload-capped
    // target, off by default (see `logging.rs`).
    tracing::info!(
        target: "longbridge_mcp::tools::error_detail",
        tool,
        stage,
        jq = %truncate(code, MAX_ERROR_CHARS),
        error = %truncate(message, MAX_ERROR_CHARS),
        "tool call jq filter error detail"
    );
}

fn truncate(text: &str, max_chars: usize) -> std::borrow::Cow<'_, str> {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &text[..end]).into(),
        None => text.into(),
    }
}

/// A compact, value-free description of a JSON response's structure, e.g.
/// `array[20] of {id: string, title: string, published_at: string}`. Returned
/// with a runtime filter error so the caller can fix the path in one retry
/// without being sent the (potentially large) response itself.
fn shape_of(value: &Value) -> String {
    fn walk(value: &Value, depth: usize, out: &mut String) {
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(_) => out.push_str("boolean"),
            Value::Number(_) => out.push_str("number"),
            Value::String(_) => out.push_str("string"),
            Value::Array(items) => {
                out.push_str(&format!("array[{}]", items.len()));
                if let Some(first) = items.first() {
                    out.push_str(" of ");
                    if depth >= SHAPE_MAX_DEPTH {
                        out.push('…');
                    } else {
                        walk(first, depth + 1, out);
                    }
                }
            }
            Value::Object(map) if depth >= SHAPE_MAX_DEPTH => {
                out.push_str(&format!("object{{{} keys}}", map.len()));
            }
            Value::Object(map) => {
                out.push('{');
                for (i, (key, child)) in map.iter().enumerate() {
                    if i == SHAPE_MAX_KEYS {
                        out.push_str(&format!(", … {} more", map.len() - i));
                        break;
                    }
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(key);
                    out.push_str(": ");
                    walk(child, depth + 1, out);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    walk(value, 0, &mut out);
    out
}

fn is_error_envelope(result: &CallToolResult) -> bool {
    // Terminal permission/no-data errors deliberately use isError:false and
    // schema-shaped placeholder structured content. Preserve their explanation.
    result
        .content
        .iter()
        .filter_map(|content| content.as_text())
        .any(|text| {
            serde_json::from_str::<Value>(&text.text)
                .ok()
                .is_some_and(|value| {
                    value.get("error_code").is_some() && value.get("recoverable").is_some()
                })
        })
}

/// The `_jq` expression itself was rejected, before the tool ran.
fn syntax_error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(serde_json::json!({
        "error_code": "jq_syntax_error",
        "message": message,
        "recoverable": "fix_params",
        "hint": "The tool did not run. Fix the _jq expression (jq syntax) and call again, or omit _jq to get the full response.",
        "data": null
    }).to_string())])
}

/// No sandbox worker slot was free. Not the filter's fault, so the caller
/// should back off and retry rather than rewrite the filter.
fn sandbox_busy(tool_ran: bool) -> CallToolResult {
    let hint = if tool_ran {
        "This is a read-only tool, so retrying is safe: retry shortly, or omit _jq."
    } else {
        "The tool did not run. Retry shortly, or omit _jq."
    };
    CallToolResult::error(vec![Content::text(serde_json::json!({
        "error_code": "jq_sandbox_busy",
        "message": "Too many _jq filters are running right now; the _jq expression itself is fine.",
        "recoverable": "backoff",
        "hint": hint,
        "data": null
    }).to_string())])
}

fn filter_error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(serde_json::json!({
        "error_code": "jq_filter_error",
        "message": format!("_jq response filtering failed: {message}. The tool has already executed."),
        "recoverable": "none",
        "hint": "Do not automatically retry a write operation; verify its outcome first. For read-only tools, correct _jq and retry.",
        "data": null
    }).to_string())])
}

/// The JSON value a filter runs on: the structured content when present,
/// otherwise the text content parsed as JSON (or kept as a string).
fn take_input(result: &mut CallToolResult) -> Value {
    result.structured_content.take().unwrap_or_else(|| {
        let mut values: Vec<Value> = result
            .content
            .iter()
            .map(|content| {
                if let Some(text) = content.as_text() {
                    serde_json::from_str(&text.text)
                        .unwrap_or_else(|_| Value::String(text.text.clone()))
                } else {
                    serde_json::to_value(content).expect("MCP content must serialize")
                }
            })
            .collect();
        if values.len() == 1 {
            values.pop().expect("one value is present")
        } else {
            Value::Array(values)
        }
    })
}

/// Replace a tool result's content with the filter outputs: one output as-is,
/// several (or none) as an array.
fn finish(mut result: CallToolResult, mut values: Vec<Value>) -> CallToolResult {
    let value = if values.len() == 1 {
        values.pop().expect("one value is present")
    } else {
        Value::Array(values)
    };
    result.content = vec![Content::text(value.to_string())];
    result.structured_content = value.is_object().then_some(value);
    result
}

/// Compile and run `code` on `input`. Called inside the sandbox worker
/// process; the server itself only compiles (to reject bad filters before the
/// tool runs) and never evaluates.
pub(super) fn evaluate(code: &str, input: Value) -> Reply {
    match compile(code) {
        Ok(filter) => run_filter(filter, input),
        Err(error) => Reply::Error(error.message.to_string()),
    }
}

fn run_filter(filter: Filter, input: Value) -> Reply {
    let input: Val = match serde_json::from_value(input) {
        Ok(input) => input,
        Err(error) => return Reply::Error(error.to_string()),
    };
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut values = Vec::new();
    let mut bytes = 0;
    for value in filter.id.run((ctx, input)) {
        // Not `jaq_core::unwrap_valr`: it calls `std::process::exit` on a halt.
        // `halt` is blocked at compile time; this keeps any other halt path
        // from ending the worker without a reply.
        let value = match value.map_err(|exn| exn.get_err()) {
            Ok(value) => value.to_string(),
            Err(Ok(error)) => {
                return Reply::FilterError(
                    truncate(&error.to_string(), MAX_ERROR_CHARS).into_owned(),
                );
            }
            Err(Err(_)) => return Reply::Error("halt is not allowed in _jq".into()),
        };
        bytes += value.len();
        if values.len() >= MAX_RESULTS || bytes > MAX_OUTPUT_BYTES {
            return Reply::Error(
                "output limit exceeded (10,000 results or 8 MiB); narrow the _jq expression".into(),
            );
        }
        match serde_json::from_str::<Value>(&value) {
            Ok(value) => values.push(value),
            Err(error) => return Reply::Error(error.to_string()),
        }
    }
    Reply::Values(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `super::call` as a read-only tool of one test caller.
    async fn call<F, Fut>(
        request: CallToolRequestParams,
        invoke: F,
    ) -> Result<CallToolResult, McpError>
    where
        F: FnOnce(CallToolRequestParams) -> Fut,
        Fut: std::future::Future<Output = Result<CallToolResult, McpError>>,
    {
        super::call(request, "jq-test", true, invoke).await
    }

    fn request(jq: serde_json::Value) -> CallToolRequestParams {
        CallToolRequestParams::new("test").with_arguments(
            json!({"symbol":"AAPL.US", "jq":"business argument", "_jq":jq})
                .as_object()
                .unwrap()
                .clone(),
        )
    }

    async fn filtered(code: &str, value: serde_json::Value) -> CallToolResult {
        call(request(json!(code)), |request| async move {
            assert_eq!(
                request.arguments.unwrap(),
                json!({"symbol":"AAPL.US", "jq":"business argument"})
                    .as_object()
                    .unwrap()
                    .clone()
            );
            Ok(super::super::tool_result(value.to_string()))
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn projects_and_selects_from_the_complete_json_payload() {
        let result = filtered(
            ".data | map(select(.price > 10) | {symbol})",
            json!({"data":[{"symbol":"A", "price":5},{"symbol":"B", "price":20}], "total":2}),
        )
        .await;
        assert_eq!(
            result.content[0].as_text().unwrap().text,
            r#"[{"symbol":"B"}]"#
        );
        assert!(result.structured_content.is_none());
    }

    #[tokio::test]
    async fn normalizes_streams_and_keeps_structured_content_in_sync() {
        for (code, expected) in [
            ("{chosen: .a}", json!({"chosen":1})),
            (".a", json!(1)),
            (".a, .b", json!([1, 2])),
            ("empty", json!([])),
            (".missing", json!(null)),
            ("false", json!(false)),
            (r#""hello""#, json!("hello")),
        ] {
            let result = filtered(code, json!({"a":1,"b":2})).await;
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(
                    &result.content[0].as_text().unwrap().text
                )
                .unwrap(),
                expected,
                "{code}"
            );
            assert_eq!(
                result.structured_content,
                expected.is_object().then_some(expected)
            );
        }
    }

    #[tokio::test]
    async fn invalid_filters_never_execute_the_tool() {
        for code in [
            json!(42),
            json!(""),
            json!("   "),
            json!(".["),
            json!("unknown_function"),
            json!("env"),
            json!("$ENV"),
            json!("debug"),
            json!("debug_empty"),
            json!("debug(\"x\")"),
            json!("stderr"),
            json!("stderr_empty"),
            json!("halt"),
            json!("halt(1)"),
            json!("halt_error"),
            json!("halt_error(1)"),
            json!(". as $x | halt_error(1)"),
            json!("include \"/etc/passwd\"; ."),
        ] {
            let result = call(request(code.clone()), |_| async {
                panic!("tool must not execute for {code}")
            })
            .await
            .unwrap();
            assert_eq!(result.is_error, Some(true), "{code}");
            assert!(result.content[0].as_text().unwrap().text.contains("_jq"));
        }
    }

    #[test]
    fn a_halt_that_reaches_runtime_is_an_error_not_a_process_exit() {
        // Compile with the unrestricted builtins, bypassing BLOCKED_BUILTINS,
        // to exercise the runtime guard in `run_filter`.
        let arena = Arena::default();
        let modules = Loader::new(
            jaq_core::defs()
                .chain(jaq_std::defs())
                .chain(jaq_json::defs()),
        )
        .load(
            &arena,
            File {
                code: "halt(1)",
                path: (),
            },
        )
        .expect("halt(1) must parse");
        let filter = jaq_core::Compiler::default()
            .with_funs(
                jaq_core::funs()
                    .chain(jaq_std::funs())
                    .chain(jaq_json::funs()),
            )
            .compile(modules)
            .expect("halt(1) must compile with unrestricted builtins");
        assert_eq!(
            run_filter(filter, json!({"a": 1})),
            Reply::Error("halt is not allowed in _jq".into())
        );
    }

    /// Deeply nested filters at the length cap, one per recursive construct of
    /// the jq grammar. Run in-process, any stack overflow here aborts the test
    /// binary, exactly as it aborted the server before `validate`.
    #[test]
    fn validation_survives_maximal_nesting() {
        let n = MAX_CODE_CHARS / 2 - 2;
        let cases = [
            format!("{}.{}", "(".repeat(n), ")".repeat(n)),
            format!("{}.{}", "[".repeat(n), "]".repeat(n)),
            format!("{}1{}", "{a:".repeat(n / 3), "}".repeat(n / 3)),
            format!("{}1", "-".repeat(MAX_CODE_CHARS - 1)),
            format!("1{}", "+1".repeat(n - 1)),
            format!(".{}", "|.".repeat(n - 1)),
            format!(".{}", ".a".repeat(n - 1)),
            format!(".{}", "[0]".repeat(n / 2)),
            format!("{}1{}", "if 1 then ".repeat(n / 6), " end".repeat(n / 6)),
            format!("{}1", "try ".repeat(n / 2)),
            format!("{}1", "def f: ".repeat(n / 4)),
            format!("{}.{}", "\"\\(".repeat(n / 4), ")\"".repeat(n / 4)),
            format!(
                "{}1{}",
                "reduce .[] as $x (".repeat(n / 10),
                "; .)".repeat(n / 10)
            ),
            format!("{}1", ". as $x | ".repeat(n / 5)),
            format!("{}.{}", "not(".repeat(n / 3), ")".repeat(n / 3)),
        ];
        for code in cases {
            // Cut to the cap: an unbalanced prefix still drives the parser
            // as deep as it can go. Only surviving matters, not validity.
            let code: String = code.chars().take(MAX_CODE_CHARS).collect();
            let _ = validate(&code);
        }
        let too_long = ".".repeat(MAX_CODE_CHARS + 1);
        assert!(validate(&too_long).unwrap_err().contains("longer than"));
    }

    #[tokio::test]
    async fn a_saturated_sandbox_never_runs_a_write_and_lets_reads_back_off() {
        // Exhaust this caller's worker slots.
        let caller = "jq-busy-test";
        let _held = (
            jq_sandbox::acquire(caller).await.unwrap(),
            jq_sandbox::acquire(caller).await.unwrap(),
        );
        let ran = std::sync::atomic::AtomicBool::new(false);
        let (write, read) = tokio::join!(
            super::call(request(json!(".")), caller, false, |_| async {
                panic!("a write must not run when no worker slot is free")
            }),
            super::call(request(json!(".")), caller, true, |_| async {
                ran.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(super::super::tool_result("{}".to_string()))
            }),
        );
        for (result, hint) in [
            (write.unwrap(), "did not run"),
            (read.unwrap(), "read-only"),
        ] {
            let text = &result.content[0].as_text().unwrap().text;
            assert_eq!(result.is_error, Some(true), "{text}");
            assert!(text.contains("jq_sandbox_busy"), "{text}");
            assert!(text.contains("\"recoverable\":\"backoff\""), "{text}");
            assert!(text.contains(hint), "{text}");
        }
        assert!(ran.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn blocked_builtins_explain_that_they_are_disabled() {
        for (code, name) in [
            ("halt_error(1)", "`halt_error`"),
            ("stderr", "`stderr`"),
            ("env", "`env`"),
            ("debug(\"x\")", "`debug`"),
            ("$ENV | keys", "`$ENV`"),
        ] {
            let result = call(request(json!(code)), |_| async {
                panic!("tool must not execute for {code}")
            })
            .await
            .unwrap();
            let text = &result.content[0].as_text().unwrap().text;
            assert!(text.contains(name), "{code}: {text}");
            assert!(text.contains("disabled in _jq"), "{code}: {text}");
            assert!(!text.contains("undefined"), "{code}: {text}");
        }
    }

    /// Every native jaq function reachable from `_jq`, reviewed to be free of
    /// side effects (no process exit, logging, environment, files or network).
    /// A jaq upgrade that adds a native fails this test until it is reviewed
    /// here or added to `BLOCKED_BUILTINS`.
    const REVIEWED_NATIVES: &[&str] = &[
        "acos",
        "acosh",
        "ascii_downcase",
        "ascii_upcase",
        "asin",
        "asinh",
        "atan",
        "atan2",
        "atanh",
        "bsearch",
        "cbrt",
        "ceil",
        "contains",
        "copysign",
        "cos",
        "cosh",
        "decode_base64",
        "decode_uri",
        "encode_base64",
        "encode_uri",
        "endswith",
        "erf",
        "erfc",
        "error_empty",
        "escape_html",
        "escape_sh",
        "exp",
        "exp10",
        "exp2",
        "explode",
        "expm1",
        "fabs",
        "fdim",
        "first",
        "floor",
        "fma",
        "fmax",
        "fmin",
        "fmod",
        "frexp",
        "fromdateiso8601",
        "fromjson",
        "gmtime",
        "group_by",
        "has",
        "hypot",
        "ilogb",
        "implode",
        "indices",
        "j0",
        "j1",
        "jn",
        "key_values",
        "keys_unsorted",
        "last",
        "ldexp",
        "length",
        "lgamma",
        "limit",
        "localtime",
        "log",
        "log10",
        "log1p",
        "log2",
        "ltrim",
        "ltrimstr",
        "matches",
        "max_by_or_empty",
        "min_by_or_empty",
        "mktime",
        "modf",
        "nearbyint",
        "nextafter",
        "now",
        "path",
        "path_value",
        "pow",
        "range",
        "remainder",
        "reverse",
        "rint",
        "round",
        "rtrim",
        "rtrimstr",
        "scalbln",
        "sin",
        "sinh",
        "skip",
        "sort",
        "sort_by",
        "split_",
        "split_matches",
        "sqrt",
        "startswith",
        "strflocaltime",
        "strftime",
        "strptime",
        "tan",
        "tanh",
        "tgamma",
        "tobytes",
        "todateiso8601",
        "tojson",
        "trim",
        "trunc",
        "unescape_html",
        "utf8bytelength",
        "y0",
        "y1",
        "yn",
    ];

    #[test]
    fn every_reachable_native_has_been_reviewed() {
        let mut unreviewed: Vec<&str> = jaq_core::funs::<data::JustLut<Val>>()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs())
            .map(|(name, _, _)| name)
            .filter(|name| !BLOCKED_BUILTINS.contains(name) && !REVIEWED_NATIVES.contains(name))
            .collect();
        unreviewed.sort_unstable();
        unreviewed.dedup();
        assert!(
            unreviewed.is_empty(),
            "review these jaq natives for side effects, then add them to REVIEWED_NATIVES or BLOCKED_BUILTINS: {unreviewed:?}"
        );
    }

    #[tokio::test]
    async fn syntax_errors_are_readable_and_locate_the_problem() {
        for (code, expected) in [
            (".data | map(.x", "end of expression"),
            (".[0] | {a: .b", "end of expression"),
            ("map(.x) | nosuchfn", "undefined filter `nosuchfn`"),
            (".a | ]", "at column 6"),
        ] {
            let result = call(request(json!(code)), |_| async {
                panic!("tool must not execute for {code}")
            })
            .await
            .unwrap();
            let text = &result.content[0].as_text().unwrap().text;
            assert!(text.contains("jq_syntax_error"), "{code}: {text}");
            assert!(text.contains(expected), "{code}: {text}");
            assert!(
                !text.contains("File {"),
                "debug dump leaked for {code}: {text}"
            );
        }
    }

    #[tokio::test]
    async fn absent_and_null_filters_preserve_results() {
        let original = super::super::tool_result(r#"{ "a": 1 }"#.into());
        for arguments in [None, Some(json!({"_jq":null}).as_object().unwrap().clone())] {
            let mut request = CallToolRequestParams::new("test");
            request.arguments = arguments;
            let result = call(request, |_| async { Ok(original.clone()) })
                .await
                .unwrap();
            assert_eq!(result, original);
        }
    }

    #[tokio::test]
    async fn errors_and_permission_placeholders_are_not_filtered() {
        let error =
            super::super::tool_error("depth", &McpError::invalid_params("bad symbol", None));
        let terminal =
            super::super::tool_error("depth", &McpError::internal_error("no quote access", None));
        for original in [error, terminal] {
            let result = call(request(json!("empty")), |_| async { Ok(original.clone()) })
                .await
                .unwrap();
            assert_eq!(result, original);
        }
    }

    #[tokio::test]
    async fn runtime_failure_does_not_return_unfiltered_data_or_retry() {
        let result = filtered(".a[]", json!({"a":1,"secret":"not requested"})).await;
        assert_eq!(result.is_error, Some(true));
        let text = &result.content[0].as_text().unwrap().text;
        assert!(text.contains("_jq"));
        assert!(text.contains("already executed"));
        assert!(!text.contains("not requested"));
        assert!(result.structured_content.is_none());
    }

    #[tokio::test]
    async fn json_text_and_plain_text_are_supported() {
        let result = call(request(json!("ascii_upcase")), |_| async {
            Ok(CallToolResult::success(vec![Content::text("hello")]))
        })
        .await
        .unwrap();
        assert_eq!(result.content[0].as_text().unwrap().text, r#""HELLO""#);
    }
    #[tokio::test]
    async fn excessive_output_is_an_error_without_partial_results() {
        let result = filtered("range(0; 10001)", json!(null)).await;
        assert_eq!(result.is_error, Some(true));
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("output limit")
        );
        assert!(result.structured_content.is_none());
    }

    #[test]
    fn shape_of_summarizes_structure_without_values() {
        let shape =
            shape_of(&json!([{"id": "1", "title": "secret", "n": 2, "ok": true, "x": null}]));
        assert_eq!(
            shape,
            "array[1] of {id: string, title: string, n: number, ok: boolean, x: null}"
        );
        assert!(
            !shape.contains("secret"),
            "the shape summary must not leak values: {shape}"
        );
    }

    #[tokio::test]
    async fn runtime_failure_reports_shape_and_truncates_echoed_values() {
        let big: Vec<_> = (0..500)
            .map(|i| json!({"id": i, "title": "x".repeat(100)}))
            .collect();
        let result = filtered(".data | map({title})", json!(big)).await;
        assert_eq!(result.is_error, Some(true));
        let text = &result.content[0].as_text().unwrap().text;
        assert!(
            text.contains("response shape: array[500] of {id: number, title: string}"),
            "{text}"
        );
        assert!(
            text.len() < 1_000,
            "error must not echo the response: {} bytes",
            text.len()
        );
    }

    #[tokio::test]
    async fn real_mcp_dispatch_filters_a_tool_without_business_arguments() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let server_task = tokio::spawn(async move {
                rmcp::serve_server(super::super::Longbridge, server).await.unwrap().waiting().await.unwrap();
            });
            let (reader, mut writer) = tokio::io::split(client);
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"jq-test","version":"1"}
            }});
            writer.write_all(format!("{initialize}\n").as_bytes()).await.unwrap();
            reader.read_line(&mut line).await.unwrap();
            let init: Value = serde_json::from_str(&line).unwrap();
            let instructions = init["result"]["instructions"].as_str().unwrap();
            assert_eq!(instructions.matches("_jq").count(), 1);
            writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").await.unwrap();
            for (id, jq, expected) in [(2, ". | {kind: type}", json!({"kind":"string"})), (3, "empty", json!([]))] {
                let call = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"now","arguments":{"_jq":jq}}});
                writer.write_all(format!("{call}\n").as_bytes()).await.unwrap();
                line.clear();
                reader.read_line(&mut line).await.unwrap();
                let response: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(response["id"], id);
                let result = &response["result"];
                assert_eq!(result["isError"], false, "{response}");
                assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(), expected);
                // The text JSON is the whole result: no outputSchema is listed, so
                // `call_tool` drops the structured copy (see `finish_result`).
                assert!(result.get("structuredContent").is_none(), "{response}");
            }
            drop(writer);
            drop(reader);
            server_task.await.unwrap();
        }).await.expect("MCP test timed out");
    }
}
