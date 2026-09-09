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
pub(super) const INSTRUCTIONS: &str = "All tools accept optional `_jq`, a filter expression using jq CLI syntax, e.g. .data | map({symbol}). Omit it for the full JSON response.";
const MAX_RESULTS: usize = 10_000;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn describe(tool: &mut Tool) {
    let schema = std::sync::Arc::make_mut(&mut tool.input_schema);
    schema
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("tool properties must be an object")
        .insert("_jq".into(), serde_json::json!({"type": "string"}));
    // A jq projection may return any JSON value. A fixed object schema would
    // reject valid filtered results. Full unfiltered schemas remain resources.
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
        McpError::invalid_params(format!("Invalid _jq expression: {errors:?}"), None)
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
            McpError::invalid_params(format!("Invalid _jq expression: {errors:?}"), None)
        })
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
            return Ok(super::tool_error(
                &request.name,
                &McpError::invalid_params("_jq must be a string", None),
            ));
        }
    };
    // Compile before any tool execution, particularly before trade writes.
    let filter = match compile(&code) {
        Ok(filter) => filter,
        Err(error) => return Ok(super::tool_error(&request.name, &error)),
    };
    let result = invoke(request).await?;
    if result.is_error == Some(true) || is_error_envelope(&result) {
        return Ok(result);
    }
    let filtered = tokio::task::spawn_blocking(move || apply(filter, result)).await;
    Ok(match filtered {
        Ok(Ok(result)) => result,
        Ok(Err(message)) => filter_error(&message),
        Err(_) => filter_error("filter worker failed"),
    })
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
    let input: Val = serde_json::from_value(input).map_err(|error| error.to_string())?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut values = Vec::new();
    let mut bytes = 0;
    for value in filter.id.run((ctx, input)).map(jaq_core::unwrap_valr) {
        let value = value.map_err(|error| error.to_string())?.to_string();
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
                if expected.is_object() {
                    assert_eq!(result["structuredContent"], expected);
                } else {
                    assert!(result.get("structuredContent").is_none());
                }
            }
            drop(writer);
            drop(reader);
            server_task.await.unwrap();
        }).await.expect("MCP test timed out");
    }
}
