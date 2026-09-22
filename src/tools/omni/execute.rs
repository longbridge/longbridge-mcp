//! `execute`: run one inner tool, or a validated multi-step pipeline.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use rmcp::ErrorData as McpError;
use rmcp::RoleServer;
use rmcp::model::{CallToolResult, Content, JsonObject};
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use rmcp::service::RequestContext;
use serde_json::Value;

use crate::tools::omni::dispatch::{Dispatcher, envelope, is_write_tool};
use crate::tools::omni::pipeline::{self, Outcome, PlanError, Runner, Status, Step};
use crate::tools::omni::truncate::{MAX_OUTPUT_TOKENS, truncate_result};
use crate::tools::{Longbridge, McpContext};

/// Hard bound on a whole pipeline. On expiry the `tokio::time::timeout`
/// future is dropped, which drops the `JoinSet` running the steps and aborts
/// every inner tool call still in flight — including a write step's HTTP
/// request, whose outcome on the upstream side is then unknown to us.
pub(crate) const PIPELINE_TIMEOUT: Duration = Duration::from_secs(30);

/// One step of a pipeline.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StepParam {
    /// Step id, unique within the call, [a-z0-9_]{1,32}. Other steps reference it via {"$from": id}.
    pub id: String,
    /// Tool name (see `search`).
    pub tool: String,
    /// Tool arguments. Any value may be {"$from": "<id>", "jq": "<expr>"} to take (a projection of) an earlier step's result.
    pub arguments: Option<JsonObject>,
    /// jq projection applied to this step's result; later references and the final return see the projected value.
    pub jq: Option<String>,
}

/// Parameters for `execute`. Give either `tool` (+ `arguments`) or `steps` (+ `return`).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteParam {
    /// Single-call form: tool name (see `search`; get its schema via `docs`).
    pub tool: Option<String>,
    /// Single-call form: arguments matching the tool's input schema. Put `_jq` at the top level of this call, not here.
    pub arguments: Option<JsonObject>,
    /// Pipeline form: up to 10 steps; independent steps run concurrently; at most one write step, which nothing may depend on.
    pub steps: Option<Vec<StepParam>>,
    /// Pipeline form: step ids whose results to return (default: steps nobody references). Others return status only.
    #[serde(rename = "return")]
    pub return_ids: Option<Vec<String>>,
}

fn fix_params(code: &str, message: String, hint: &str) -> CallToolResult {
    envelope(code, message, "fix_params", hint, Value::Null)
}

/// Structural checks that do not need the router: exactly one form, no inner `_jq`.
pub(crate) fn shape_error(p: &ExecuteParam) -> Option<CallToolResult> {
    match (&p.tool, &p.steps) {
        (Some(_), Some(_)) => {
            return Some(fix_params(
                "invalid_execute",
                "Give either `tool` or `steps`, not both.".into(),
                "Use `tool` + `arguments` for one call, or `steps` (+ `return`) for a pipeline.",
            ));
        }
        (None, None) => {
            return Some(fix_params(
                "invalid_execute",
                "`execute` needs `tool` or `steps`.".into(),
                "Call `search` to find a tool, then `execute` with `tool` and `arguments`.",
            ));
        }
        _ => {}
    }
    let has_inner_jq = p.arguments.as_ref().is_some_and(|a| a.contains_key("_jq"))
        || p.steps.as_ref().is_some_and(|s| {
            s.iter()
                .any(|st| st.arguments.as_ref().is_some_and(|a| a.contains_key("_jq")))
        });
    if has_inner_jq {
        return Some(fix_params(
            "invalid_execute",
            "`_jq` inside `arguments` is not applied.".into(),
            "Put `_jq` at the top level of the execute call, or use a step-level `jq` in a pipeline.",
        ));
    }
    None
}

/// `fix_params` envelope for a pipeline rejected by `pipeline::validate`.
pub(crate) fn plan_error_result(err: PlanError) -> CallToolResult {
    fix_params(
        "invalid_pipeline",
        err.to_string(),
        "Fix the `steps` definition; see docs topic `pipelines`.",
    )
}

/// Build the pipeline response: status for every step, data only for `return_ids`.
///
/// `is_error` is set iff no step the caller can see succeeded: when
/// `return_ids` is non-empty, that means none of the *returned* steps is
/// `Ok`; when it is empty (e.g. an explicit `"return": []` for a status-only
/// pipeline), it means no step at all is `Ok`.
pub(crate) fn assemble(
    outcomes: &BTreeMap<String, Outcome>,
    return_ids: &[String],
) -> CallToolResult {
    let mut steps = serde_json::Map::new();
    let mut any_returned_ok = false;
    let mut any_ok = false;
    for (id, outcome) in outcomes {
        any_ok |= outcome.status == Status::Ok;
        let mut entry = serde_json::json!({
            "status": outcome.status.as_str(),
            "elapsed_ms": outcome.elapsed_ms,
        });
        if return_ids.contains(id) {
            entry["result"] = outcome.value.clone();
            any_returned_ok |= outcome.status == Status::Ok;
        }
        steps.insert(id.clone(), entry);
    }
    let body = serde_json::json!({ "steps": steps });
    let mut result = CallToolResult::success(vec![Content::text(body.to_string())]);
    result.structured_content = Some(body);
    let is_error = if return_ids.is_empty() {
        !any_ok
    } else {
        !any_returned_ok
    };
    if is_error {
        result.is_error = Some(true);
    }
    result
}

/// The account's DC region, resolved only when at least one requested tool is
/// region-scoped (resolving it makes an HTTP call).
async fn region_for(mctx: &McpContext, tools: &[&str]) -> Option<longbridge::DcRegion> {
    if tools.iter().any(|t| crate::tools::is_region_scoped(t)) {
        Some(mctx.dc_region().await)
    } else {
        None
    }
}

/// Run `execute`: a single inner tool call, or a validated pipeline of steps.
pub(crate) async fn execute(
    server: Longbridge,
    ctx: RequestContext<RoleServer>,
    mctx: &McpContext,
    p: ExecuteParam,
) -> Result<CallToolResult, McpError> {
    if let Some(err) = shape_error(&p) {
        return Ok(err);
    }
    if let Some(tool) = p.tool {
        let region = region_for(mctx, &[tool.as_str()]).await;
        let dispatcher = Dispatcher::new(server, ctx, region);
        let result = dispatcher
            .call(&tool, p.arguments.unwrap_or_default())
            .await;
        crate::metrics::record_omni_step(
            &tool,
            if result.is_error == Some(true) {
                "error"
            } else {
                "ok"
            },
        );
        crate::metrics::record_omni_pipeline_size(1);
        return Ok(truncate_result(result, MAX_OUTPUT_TOKENS));
    }
    let steps: Vec<Step> = p
        .steps
        .unwrap_or_default()
        .into_iter()
        .map(|s| Step {
            id: s.id,
            tool: s.tool,
            arguments: s.arguments.unwrap_or_default(),
            jq: s.jq,
        })
        .collect();
    let plan = match pipeline::validate(steps, p.return_ids, &is_write_tool) {
        Ok(plan) => plan,
        Err(err) => return Ok(plan_error_result(err)),
    };
    let tools: Vec<&str> = plan.steps.iter().map(|s| s.tool.as_str()).collect();
    let region = region_for(mctx, &tools).await;
    let dispatcher = Arc::new(Dispatcher::new(server, ctx, region));
    let runner: Runner = Arc::new(move |tool: String, args: JsonObject| {
        let dispatcher = dispatcher.clone();
        Box::pin(async move { dispatcher.call(&tool, args).await })
    });
    crate::metrics::record_omni_pipeline_size(plan.steps.len());
    let outcomes = match tokio::time::timeout(PIPELINE_TIMEOUT, pipeline::run(&plan, runner)).await
    {
        Ok(outcomes) => outcomes,
        Err(_) => {
            return Ok(envelope(
                "pipeline_timeout",
                format!(
                    "pipeline did not finish within {}s",
                    PIPELINE_TIMEOUT.as_secs()
                ),
                "none",
                "In-flight steps were aborted; if the pipeline contained a write step, verify \
                 its outcome (today_orders / order_detail) before retrying. Split the pipeline \
                 into smaller execute calls.",
                Value::Null,
            ));
        }
    };
    for step in &plan.steps {
        let status = outcomes
            .get(&step.id)
            .map(|o| o.status.as_str())
            .unwrap_or("error");
        crate::metrics::record_omni_step(&step.tool, status);
    }
    Ok(truncate_result(
        assemble(&outcomes, &plan.return_ids),
        MAX_OUTPUT_TOKENS,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: serde_json::Value) -> JsonObject {
        v.as_object().expect("object").clone()
    }

    #[test]
    fn shape_rejects_both_or_neither_and_inner_jq() {
        let both = ExecuteParam {
            tool: Some("quote".into()),
            arguments: None,
            steps: Some(vec![]),
            return_ids: None,
        };
        let err = shape_error(&both).expect("error");
        assert_eq!(
            crate::tools::jq::result_value(&err)["recoverable"],
            "fix_params",
            "giving both `tool` and `steps` is a fixable shape error"
        );
        let neither = ExecuteParam {
            tool: None,
            arguments: None,
            steps: None,
            return_ids: None,
        };
        assert!(
            shape_error(&neither).is_some(),
            "neither tool nor steps must be rejected"
        );
        let inner_jq = ExecuteParam {
            tool: Some("quote".into()),
            arguments: Some(obj(json!({"symbols": [], "_jq": "."}))),
            steps: None,
            return_ids: None,
        };
        let err = shape_error(&inner_jq).expect("error");
        assert!(
            crate::tools::jq::result_value(&err)["message"]
                .as_str()
                .expect("msg")
                .contains("_jq"),
            "the error must explain that inner `_jq` is not applied"
        );
        let ok = ExecuteParam {
            tool: Some("quote".into()),
            arguments: None,
            steps: None,
            return_ids: None,
        };
        assert!(
            shape_error(&ok).is_none(),
            "a valid single-call shape passes"
        );
    }

    #[test]
    fn plan_error_maps_to_fix_params_envelope() {
        let r = plan_error_result(crate::tools::omni::pipeline::PlanError::Cycle);
        let v = crate::tools::jq::result_value(&r);
        assert_eq!(
            v["error_code"], "invalid_pipeline",
            "a rejected plan must carry the invalid_pipeline error code"
        );
        assert_eq!(
            v["recoverable"], "fix_params",
            "a plan error is always fixable by the caller"
        );
    }

    #[test]
    fn assemble_returns_only_requested_step_values() {
        use crate::tools::omni::pipeline::{Outcome, Status};
        let mut outcomes = std::collections::BTreeMap::new();
        outcomes.insert(
            "a".to_string(),
            Outcome {
                status: Status::Ok,
                elapsed_ms: 5,
                value: json!([1, 2]),
            },
        );
        outcomes.insert(
            "b".to_string(),
            Outcome {
                status: Status::Error,
                elapsed_ms: 7,
                value: json!({"error_code": "x"}),
            },
        );
        let r = assemble(&outcomes, &["b".to_string()]);
        let v = crate::tools::jq::result_value(&r);
        assert_eq!(
            v["steps"]["a"],
            json!({"status": "ok", "elapsed_ms": 5}),
            "a step not in `return_ids` reports status only, no `result`"
        );
        assert_eq!(
            v["steps"]["b"]["status"], "error",
            "a failed step's status must be reported as error"
        );
        assert_eq!(
            v["steps"]["b"]["result"]["error_code"], "x",
            "a step in `return_ids` must carry its outcome value as `result`"
        );
        assert_eq!(r.is_error, Some(true), "all returned steps failed");
        let r = assemble(&outcomes, &["a".to_string(), "b".to_string()]);
        assert_ne!(r.is_error, Some(true), "one returned step succeeded");
    }

    #[test]
    fn assemble_with_empty_return_ids_falls_back_to_overall_success() {
        use crate::tools::omni::pipeline::{Outcome, Status};
        let mut all_ok = std::collections::BTreeMap::new();
        all_ok.insert(
            "a".to_string(),
            Outcome {
                status: Status::Ok,
                elapsed_ms: 1,
                value: json!(null),
            },
        );
        let r = assemble(&all_ok, &[]);
        assert_ne!(
            r.is_error,
            Some(true),
            "an empty `return` with a successful step must not be an error"
        );

        let mut all_error = std::collections::BTreeMap::new();
        all_error.insert(
            "a".to_string(),
            Outcome {
                status: Status::Error,
                elapsed_ms: 1,
                value: json!({"error_code": "x"}),
            },
        );
        let r = assemble(&all_error, &[]);
        assert_eq!(
            r.is_error,
            Some(true),
            "an empty `return` where every step failed must still be an error"
        );
    }
}
