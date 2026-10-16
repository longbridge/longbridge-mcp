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

type Filter = jaq_core::Filter<data::JustLut<Val>>;
pub(super) const INSTRUCTIONS: &str = "All tools accept optional `_jq`, a filter expression using jq CLI syntax applied to the response; one output is returned as-is, several as a JSON array. Response shapes differ per tool (some are a top-level array, some an object; most have no `data` wrapper) and most tool descriptions say what they return. If unsure, probe cheaply with keys (object) or .[0] | keys (array) rather than fetching the full response. Omit it for the full JSON response.";

/// Per-tool schema description for the injected `_jq` property, so a client
/// inspecting a single tool's schema learns the contract without depending on
/// the server-level `instructions` (which many clients drop).
const JQ_PROPERTY_DESCRIPTION: &str = "Optional jq filter applied to this tool's JSON response; it never changes the request, and several outputs come back as an array. For field names see the tool description, or probe with `keys` / `.[0] | keys`.";
const MAX_RESULTS: usize = 10_000;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const FILTER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Cap on the jaq error text echoed back. jaq embeds the offending value in
/// its message (`cannot index [<whole response>] with "data"`), which can be
/// tens of KB of upstream data the caller never asked to see.
const MAX_ERROR_CHARS: usize = 200;
/// Depth/width limits for [`shape_of`], keeping the summary to a few hundred
/// bytes even for large nested responses.
const SHAPE_MAX_DEPTH: usize = 3;
const SHAPE_MAX_KEYS: usize = 25;

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
            .chain(jaq_std::defs().filter(|def| def.name != "debug"))
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
        // Server environment contains credentials. Logging would also allow
        // queries to write upstream account data into server logs.
        .filter(|(name, _, _)| !matches!(*name, "env" | "debug" | "debug_empty" | "stderr"));
    jaq_core::Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errors| {
            let details: Vec<String> = errors
                .iter()
                .flat_map(|(_, undefined)| undefined)
                .map(|(name, kind)| format!("undefined {} `{name}`", kind.as_str()))
                .collect();
            McpError::invalid_params(
                format!("Invalid _jq expression: {}", details.join("; ")),
                None,
            )
        })
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

pub(super) async fn call<F, Fut>(
    mut request: CallToolRequestParams,
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
    let filter = match compile(&code) {
        Ok(filter) => filter,
        Err(error) => {
            record_failure(&request.name, "compile", &code, &error.message);
            return Ok(syntax_error(&error.message));
        }
    };
    let name = request.name.clone();
    let result = invoke(request).await?;
    if result.is_error == Some(true) || is_error_envelope(&result) {
        return Ok(result);
    }
    // Bound the *caller-visible* latency: `MAX_RESULTS`/`MAX_OUTPUT_BYTES` only
    // fire between output values, so a filter that spins without emitting (e.g.
    // `while(true; .)`) or builds one huge/non-terminating aggregate (e.g.
    // `[range(0;1e9)]`, `[repeat(1)]`) would otherwise hang the request. The
    // timeout returns a bounded, actionable error instead. Caveat: a dropped
    // `spawn_blocking` handle does not abort the blocking task, so a hostile
    // aggregate keeps running (and can OOM) on a detached thread until it
    // finishes -- jaq exposes no fuel/step limit to cap that from here. The
    // input is the already-fetched (bounded) tool response, so the only
    // unbounded vector is generative builtins, not caller-supplied data.
    let worker = tokio::task::spawn_blocking(move || apply(filter, result));
    let filtered = tokio::time::timeout(FILTER_TIMEOUT, worker).await;
    let message = match filtered {
        Ok(Ok(Ok(result))) => return Ok(result),
        Ok(Ok(Err(message))) => message,
        Ok(Err(_)) => "filter worker failed".to_string(),
        Err(_) => "filter timed out after 5s; narrow the _jq expression (avoid unbounded ranges or repeats)".to_string(),
    };
    record_failure(&name, "runtime", &code, &message);
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

fn filter_error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(serde_json::json!({
        "error_code": "jq_filter_error",
        "message": format!("_jq response filtering failed: {message}. The tool has already executed."),
        "recoverable": "none",
        "hint": "Do not automatically retry a write operation; verify its outcome first. For read-only tools, correct _jq and retry.",
        "data": null
    }).to_string())])
}

fn apply(filter: Filter, mut result: CallToolResult) -> Result<CallToolResult, String> {
    let input = result.structured_content.take().unwrap_or_else(|| {
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
            values.pop().unwrap()
        } else {
            Value::Array(values)
        }
    });
    let shape = shape_of(&input);
    let input: Val = serde_json::from_value(input).map_err(|error| error.to_string())?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut values = Vec::new();
    let mut bytes = 0;
    for value in filter.id.run((ctx, input)).map(jaq_core::unwrap_valr) {
        let value = value
            .map_err(|error| {
                format!(
                    "{}; response shape: {shape}",
                    truncate(&error.to_string(), MAX_ERROR_CHARS)
                )
            })?
            .to_string();
        bytes += value.len();
        if values.len() >= MAX_RESULTS || bytes > MAX_OUTPUT_BYTES {
            return Err(
                "output limit exceeded (10,000 results or 8 MiB); narrow the _jq expression".into(),
            );
        }
        values.push(serde_json::from_str::<Value>(&value).map_err(|error| error.to_string())?);
    }
    let value = if values.len() == 1 {
        values.pop().unwrap()
    } else {
        Value::Array(values)
    };
    result.content = vec![Content::text(value.to_string())];
    result.structured_content = value.is_object().then_some(value);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
