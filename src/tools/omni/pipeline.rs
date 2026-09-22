//! Multi-step `execute`: validate a DAG of tool calls, resolve `$from`
//! references, run independent steps concurrently, project each result with
//! jq so only the requested summary returns to the model.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use rmcp::model::{CallToolResult, JsonObject};
use serde_json::{Map, Value, json};
use tokio::task::{Id, JoinSet};

use crate::tools::jq::{project_bounded, result_value};

// Nothing in this module is called outside tests yet; wiring the `/omni`
// dispatcher to `execute` (a later task) is its first production caller.

/// Most steps one `execute` call may contain.
pub(crate) const MAX_STEPS: usize = 10;
/// Most steps run at the same time.
pub(crate) const MAX_CONCURRENCY: usize = 4;
/// Order-defining fields a write step may not take from an upstream step.
pub(crate) const FORBIDDEN_WRITE_REFS: &[&str] = &[
    "symbol",
    "side",
    "quantity",
    "submitted_quantity",
    "price",
    "submitted_price",
    "trigger_price",
];

/// One pipeline step as supplied by the caller.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    /// Caller-chosen identifier other steps reference with `$from`.
    pub id: String,
    /// Inner tool name to call.
    pub tool: String,
    /// Arguments, possibly containing `{"$from": ...}` reference objects.
    pub arguments: JsonObject,
    /// Optional jq filter applied to this step's result.
    pub jq: Option<String>,
}

/// A validated pipeline. `deps[i]` are indices `steps[i]` reads from.
#[derive(Debug)]
pub(crate) struct Plan {
    /// Steps in the order the caller supplied them.
    pub steps: Vec<Step>,
    /// Per-step dependency indices into `steps`.
    pub deps: Vec<Vec<usize>>,
    /// Ids whose outcomes the caller wants returned.
    pub return_ids: Vec<String>,
}

/// Why a pipeline was rejected before running.
#[derive(Debug, thiserror::Error, PartialEq)]
pub(crate) enum PlanError {
    /// No steps were supplied.
    #[error("steps must not be empty")]
    Empty,
    /// More than [`MAX_STEPS`] steps were supplied.
    #[error("too many steps: {0} (max {MAX_STEPS})")]
    TooMany(usize),
    /// A step id is not `[a-z0-9_]{1,32}`.
    #[error("invalid step id `{0}`: use [a-z0-9_]{{1,32}}")]
    BadId(String),
    /// Two steps share an id.
    #[error("duplicate step id `{0}`")]
    DuplicateId(String),
    /// An object carries `$from` but is not a well-formed reference.
    #[error("step `{step}` has a malformed `$from` reference: use {{\"$from\": id, \"jq\": expr}}")]
    MalformedRef {
        /// The step whose arguments carry the malformed reference.
        step: String,
    },
    /// A `$from` names a step that does not exist.
    #[error("step `{step}` references unknown step `{target}`")]
    UnknownRef {
        /// The referencing step.
        step: String,
        /// The missing target id.
        target: String,
    },
    /// A step references itself.
    #[error("step `{0}` references itself")]
    SelfRef(String),
    /// The dependency graph is not acyclic.
    #[error("steps form a cycle")]
    Cycle,
    /// More than one write step was supplied.
    #[error("a pipeline may contain at most one write step; found {0:?}")]
    MultipleWrites(Vec<String>),
    /// A step reads from a write step.
    #[error(
        "step `{step}` must not depend on write step `{write}`: a write's first call is a dry-run preview for the user"
    )]
    DependsOnWrite {
        /// The dependent step.
        step: String,
        /// The write step it depends on.
        write: String,
    },
    /// A write step takes an order-defining field from another step.
    #[error(
        "write step `{step}` must not take `{field}` from another step; write literal values the user can confirm"
    )]
    WriteRefForbidden {
        /// The write step.
        step: String,
        /// The offending field name.
        field: String,
    },
    /// `return` names a step that does not exist.
    #[error("`return` names unknown step `{0}`")]
    UnknownReturn(String),
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A `{"$from": id, "jq": expr}` reference object.
fn as_ref(value: &Value) -> Option<(&str, Option<&str>)> {
    let obj = value.as_object()?;
    let from = obj.get("$from")?.as_str()?;
    if obj.keys().any(|k| k != "$from" && k != "jq") {
        return None;
    }
    let jq = match obj.get("jq") {
        Some(value) => Some(value.as_str()?),
        None => None,
    };
    Some((from, jq))
}

/// Whether any object under `value` carries a `$from` key without being a
/// well-formed reference: a typo in the reference must not reach the tool as a
/// literal argument.
fn has_malformed_ref(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            if map.contains_key("$from") && as_ref(value).is_none() {
                return true;
            }
            map.values().any(has_malformed_ref)
        }
        Value::Array(items) => items.iter().any(has_malformed_ref),
        _ => false,
    }
}

fn collect_refs<'a>(value: &'a Value, out: &mut Vec<(&'a str, Option<&'a str>)>) {
    if let Some(r) = as_ref(value) {
        out.push(r);
        return;
    }
    match value {
        Value::Object(map) => map.values().for_each(|v| collect_refs(v, out)),
        Value::Array(items) => items.iter().for_each(|v| collect_refs(v, out)),
        _ => {}
    }
}

/// Validate structure, dependencies and write rules.
pub(crate) fn validate(
    steps: Vec<Step>,
    return_ids: Option<Vec<String>>,
    is_write: &dyn Fn(&str) -> bool,
) -> Result<Plan, PlanError> {
    if steps.is_empty() {
        return Err(PlanError::Empty);
    }
    if steps.len() > MAX_STEPS {
        return Err(PlanError::TooMany(steps.len()));
    }
    let mut index: HashMap<&str, usize> = HashMap::new();
    for (i, step) in steps.iter().enumerate() {
        if !valid_id(&step.id) {
            return Err(PlanError::BadId(step.id.clone()));
        }
        if index.insert(&step.id, i).is_some() {
            return Err(PlanError::DuplicateId(step.id.clone()));
        }
    }
    let writes: Vec<String> = steps
        .iter()
        .filter(|s| is_write(&s.tool))
        .map(|s| s.id.clone())
        .collect();
    if writes.len() > 1 {
        return Err(PlanError::MultipleWrites(writes));
    }
    let mut deps: Vec<Vec<usize>> = Vec::with_capacity(steps.len());
    for step in &steps {
        let args = Value::Object(step.arguments.clone());
        if has_malformed_ref(&args) {
            return Err(PlanError::MalformedRef {
                step: step.id.clone(),
            });
        }
        let mut refs = Vec::new();
        collect_refs(&args, &mut refs);
        let mut mine = Vec::new();
        for (target, _) in &refs {
            let Some(&t) = index.get(target) else {
                return Err(PlanError::UnknownRef {
                    step: step.id.clone(),
                    target: (*target).to_string(),
                });
            };
            if *target == step.id {
                return Err(PlanError::SelfRef(step.id.clone()));
            }
            if is_write(&steps[t].tool) {
                return Err(PlanError::DependsOnWrite {
                    step: step.id.clone(),
                    write: steps[t].id.clone(),
                });
            }
            if !mine.contains(&t) {
                mine.push(t);
            }
        }
        if is_write(&step.tool) {
            for field in FORBIDDEN_WRITE_REFS {
                if step
                    .arguments
                    .get(*field)
                    .is_some_and(|v| as_ref(v).is_some())
                {
                    return Err(PlanError::WriteRefForbidden {
                        step: step.id.clone(),
                        field: (*field).to_string(),
                    });
                }
            }
        }
        deps.push(mine);
    }
    // Kahn's algorithm for cycle detection.
    let mut indegree: Vec<usize> = deps.iter().map(Vec::len).collect();
    let mut ready: Vec<usize> = indegree
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == 0)
        .map(|(i, _)| i)
        .collect();
    let mut seen = 0;
    while let Some(i) = ready.pop() {
        seen += 1;
        for (j, d) in deps.iter().enumerate() {
            if d.contains(&i) {
                indegree[j] -= 1;
                if indegree[j] == 0 {
                    ready.push(j);
                }
            }
        }
    }
    if seen != steps.len() {
        return Err(PlanError::Cycle);
    }
    let referenced: HashSet<usize> = deps.iter().flatten().copied().collect();
    let return_ids = match return_ids {
        Some(ids) => {
            for id in &ids {
                if !index.contains_key(id.as_str()) {
                    return Err(PlanError::UnknownReturn(id.clone()));
                }
            }
            ids
        }
        None => steps
            .iter()
            .enumerate()
            .filter(|(i, _)| !referenced.contains(i))
            .map(|(_, s)| s.id.clone())
            .collect(),
    };
    Ok(Plan {
        steps,
        deps,
        return_ids,
    })
}

/// Terminal state of one step.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Status {
    /// The step ran and its projection succeeded.
    Ok,
    /// The step failed, or its jq projection did; `value` is an envelope.
    Error,
    /// A dependency failed, so the step never ran.
    Skipped,
}

/// Result of one step: `value` is the projected result, or an error envelope.
#[derive(Debug, Clone)]
pub(crate) struct Outcome {
    /// How the step ended.
    pub status: Status,
    /// Wall-clock time the inner call took.
    pub elapsed_ms: u64,
    /// Projected result, error envelope, or `{"skipped_because": id}`.
    pub value: Value,
}

/// Executes one inner tool call.
pub(crate) type Runner = Arc<
    dyn Fn(String, JsonObject) -> Pin<Box<dyn Future<Output = CallToolResult> + Send>>
        + Send
        + Sync,
>;

/// The `jq_filter_error` envelope. `already_ran` tells a step's own projection
/// (its tool has run) apart from a `$from` reference's projection, which fails
/// before the referencing step is called.
fn jq_error(message: &str, already_ran: bool) -> Value {
    let message = if already_ran {
        format!("step jq projection failed: {message}. The step's tool has already executed.")
    } else {
        format!(
            "`$from` reference projection failed: {message}. The step was not called, but earlier steps have already executed."
        )
    };
    json!({
        "error_code": "jq_filter_error",
        "message": message,
        "recoverable": "fix_params",
        "hint": "Correct the step's `jq` (or the `$from` reference's `jq`) and re-run; do not retry write steps automatically.",
        "data": null
    })
}

/// The envelope for a step the scheduler could not complete.
fn internal_error(message: String) -> Value {
    json!({
        "error_code": "internal",
        "message": message,
        "recoverable": "none",
        "hint": null,
        "data": null,
    })
}

/// A step whose whole `arguments` is a reference resolving to a non-object.
fn invalid_args_error(step_id: &str) -> Value {
    json!({
        "error_code": "invalid_params",
        "message": format!("step `{step_id}` resolved its `arguments` to a non-object value"),
        "recoverable": "fix_params",
        "hint": "Reference upstream steps inside individual argument fields, e.g. {\"symbols\": {\"$from\": \"picks\"}}.",
        "data": null
    })
}

/// Replace every reference object with the upstream step's projected value,
/// applying the reference's own `jq` when present.
async fn resolve(value: Value, outcomes: &BTreeMap<String, Outcome>) -> Result<Value, Value> {
    if let Some((from, jq)) = as_ref(&value) {
        let upstream = outcomes
            .get(from)
            .map(|o| o.value.clone())
            .unwrap_or(Value::Null);
        return match jq {
            Some(code) => project_bounded(code.to_string(), upstream)
                .await
                .map_err(|m| jq_error(&m, false)),
            None => Ok(upstream),
        };
    }
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k, Box::pin(resolve(v, outcomes)).await?);
            }
            Ok(Value::Object(out))
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for v in items {
                out.push(Box::pin(resolve(v, outcomes)).await?);
            }
            Ok(Value::Array(out))
        }
        other => Ok(other),
    }
}

/// Mark every step blocked by a failed or skipped dependency as
/// [`Status::Skipped`], repeating until nothing changes so that chains
/// propagate whatever order the caller listed the steps in.
fn propagate_skips(
    plan: &Plan,
    done: &mut [bool],
    running: &HashSet<usize>,
    outcomes: &mut BTreeMap<String, Outcome>,
) {
    loop {
        let mut changed = false;
        for i in 0..plan.steps.len() {
            if done[i] || running.contains(&i) {
                continue;
            }
            let blocked = plan.deps[i].iter().find(|&&d| {
                done[d]
                    && outcomes
                        .get(&plan.steps[d].id)
                        .expect("a step marked done has an outcome")
                        .status
                        != Status::Ok
            });
            if let Some(&d) = blocked {
                outcomes.insert(
                    plan.steps[i].id.clone(),
                    Outcome {
                        status: Status::Skipped,
                        elapsed_ms: 0,
                        value: json!({"skipped_because": plan.steps[d].id}),
                    },
                );
                done[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Run the plan: schedule steps whose dependencies succeeded, up to
/// [`MAX_CONCURRENCY`] at a time; dependents of a failed step are `Skipped`.
pub(crate) async fn run(plan: &Plan, runner: Runner) -> BTreeMap<String, Outcome> {
    let n = plan.steps.len();
    let mut outcomes: BTreeMap<String, Outcome> = BTreeMap::new();
    let mut done: Vec<bool> = vec![false; n];
    let mut running: HashSet<usize> = HashSet::new();
    let mut spawned: HashMap<Id, usize> = HashMap::new();
    let mut tasks: JoinSet<(usize, Outcome)> = JoinSet::new();
    loop {
        propagate_skips(plan, &mut done, &running, &mut outcomes);
        // Launch ready steps. A step that fails while resolving its arguments
        // restarts the loop so its dependents are skipped, never launched.
        let mut resolve_failed = false;
        for i in 0..n {
            if done[i] || running.contains(&i) || tasks.len() >= MAX_CONCURRENCY {
                continue;
            }
            if !plan.deps[i].iter().all(|&d| done[d]) {
                continue;
            }
            let step = plan.steps[i].clone();
            let resolved = match resolve(Value::Object(step.arguments.clone()), &outcomes).await {
                Ok(Value::Object(map)) => Ok(map),
                Ok(_) => Err(invalid_args_error(&step.id)),
                Err(envelope) => Err(envelope),
            };
            let args = match resolved {
                Ok(args) => args,
                Err(value) => {
                    outcomes.insert(
                        step.id.clone(),
                        Outcome {
                            status: Status::Error,
                            elapsed_ms: 0,
                            value,
                        },
                    );
                    done[i] = true;
                    resolve_failed = true;
                    break;
                }
            };
            let runner = runner.clone();
            running.insert(i);
            let handle = tasks.spawn(async move {
                let start = Instant::now();
                let result = runner(step.tool.clone(), args).await;
                let elapsed_ms = start.elapsed().as_millis() as u64;
                let value = result_value(&result);
                if result.is_error == Some(true) {
                    return (
                        i,
                        Outcome {
                            status: Status::Error,
                            elapsed_ms,
                            value,
                        },
                    );
                }
                match step.jq {
                    Some(code) => match project_bounded(code, value).await {
                        Ok(v) => (
                            i,
                            Outcome {
                                status: Status::Ok,
                                elapsed_ms,
                                value: v,
                            },
                        ),
                        Err(m) => (
                            i,
                            Outcome {
                                status: Status::Error,
                                elapsed_ms,
                                value: jq_error(&m, true),
                            },
                        ),
                    },
                    None => (
                        i,
                        Outcome {
                            status: Status::Ok,
                            elapsed_ms,
                            value,
                        },
                    ),
                }
            });
            spawned.insert(handle.id(), i);
        }
        if resolve_failed {
            continue;
        }
        if done.iter().all(|d| *d) {
            break;
        }
        match tasks.join_next().await {
            Some(Ok((i, outcome))) => {
                running.remove(&i);
                done[i] = true;
                outcomes.insert(plan.steps[i].id.clone(), outcome);
            }
            Some(Err(join_error)) => {
                // A panicked step must not hang the pipeline; report it and continue.
                let failed: Vec<usize> = match spawned.remove(&join_error.id()) {
                    Some(i) => vec![i],
                    // The error names no task we spawned, so the step it belongs
                    // to is unknown: fail everything in flight rather than wait
                    // for a result that can no longer arrive.
                    None => mem::take(&mut running).into_iter().collect(),
                };
                for i in failed {
                    running.remove(&i);
                    done[i] = true;
                    outcomes.insert(
                        plan.steps[i].id.clone(),
                        Outcome {
                            status: Status::Error,
                            elapsed_ms: 0,
                            value: internal_error(format!(
                                "step `{}` did not complete: {join_error}",
                                plan.steps[i].id
                            )),
                        },
                    );
                }
            }
            None => {
                // Nothing left to join while steps are still pending. `validate`
                // rules this out, but a hand-built `Plan` (say, with a cycle)
                // must fail the request's steps rather than panic it.
                for (i, step_done) in done.iter_mut().enumerate() {
                    if !*step_done {
                        *step_done = true;
                        outcomes.insert(
                            plan.steps[i].id.clone(),
                            Outcome {
                                status: Status::Error,
                                elapsed_ms: 0,
                                value: internal_error(format!(
                                    "step `{}` was never scheduled: its dependencies cannot complete",
                                    plan.steps[i].id
                                )),
                            },
                        );
                    }
                }
                break;
            }
        }
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;
    use rmcp::model::Content;
    use serde_json::json;

    fn step(id: &str, tool: &str, args: serde_json::Value, jq: Option<&str>) -> Step {
        Step {
            id: id.into(),
            tool: tool.into(),
            arguments: args
                .as_object()
                .expect("test step arguments are a JSON object")
                .clone(),
            jq: jq.map(String::from),
        }
    }
    fn no_write(_: &str) -> bool {
        false
    }
    fn write_is_submit(name: &str) -> bool {
        name == "submit_order"
    }

    #[test]
    fn validate_builds_dependency_graph_from_refs() {
        let plan = validate(
            vec![
                step(
                    "a",
                    "screener_search",
                    json!({}),
                    Some(".data | map(.symbol)"),
                ),
                step("b", "quote", json!({"symbols": {"$from": "a"}}), None),
            ],
            None,
            &no_write,
        )
        .expect("valid plan");
        assert_eq!(
            plan.deps[0],
            Vec::<usize>::new(),
            "a step without references depends on nothing"
        );
        assert_eq!(plan.deps[1], vec![0], "`$from: a` makes `b` depend on `a`");
        assert_eq!(
            plan.return_ids,
            vec!["b".to_string()],
            "default return = leaves"
        );
    }

    #[test]
    fn validate_rejects_structural_errors() {
        assert_eq!(
            validate(vec![], None, &no_write).unwrap_err(),
            PlanError::Empty,
            "an empty pipeline is rejected"
        );
        let many = (0..11)
            .map(|i| step(&format!("s{i}"), "quote", json!({}), None))
            .collect();
        assert_eq!(
            validate(many, None, &no_write).unwrap_err(),
            PlanError::TooMany(11),
            "more than MAX_STEPS steps are rejected"
        );
        assert_eq!(
            validate(
                vec![step("Bad-Id", "quote", json!({}), None)],
                None,
                &no_write
            )
            .unwrap_err(),
            PlanError::BadId("Bad-Id".into()),
            "step ids are restricted to [a-z0-9_]"
        );
        assert_eq!(
            validate(
                vec![
                    step("a", "quote", json!({}), None),
                    step("a", "quote", json!({}), None)
                ],
                None,
                &no_write
            )
            .unwrap_err(),
            PlanError::DuplicateId("a".into()),
            "two steps may not share an id"
        );
        assert_eq!(
            validate(
                vec![step("a", "quote", json!({"x": {"$from": "zz"}}), None)],
                None,
                &no_write
            )
            .unwrap_err(),
            PlanError::UnknownRef {
                step: "a".into(),
                target: "zz".into()
            },
            "`$from` must name an existing step"
        );
        assert_eq!(
            validate(
                vec![
                    step("a", "quote", json!({"x": {"$from": "b"}}), None),
                    step("b", "quote", json!({"x": {"$from": "a"}}), None),
                ],
                None,
                &no_write
            )
            .unwrap_err(),
            PlanError::Cycle,
            "mutually referencing steps are a cycle"
        );
        assert_eq!(
            validate(
                vec![step("a", "quote", json!({}), None)],
                Some(vec!["nope".into()]),
                &no_write
            )
            .unwrap_err(),
            PlanError::UnknownReturn("nope".into()),
            "`return` must name existing steps"
        );
    }

    #[test]
    fn validate_rejects_malformed_references() {
        let malformed = [
            json!({"x": {"$from": "a", "typo": ".x"}}),
            json!({"x": {"$from": 123}}),
            json!({"x": {"$from": "a", "jq": 5}}),
        ];
        for arguments in malformed {
            assert_eq!(
                validate(
                    vec![
                        step("a", "quote", json!({}), None),
                        step("b", "quote", arguments.clone(), None),
                    ],
                    None,
                    &no_write
                )
                .unwrap_err(),
                PlanError::MalformedRef { step: "b".into() },
                "a near-miss reference must be rejected, not passed through as a literal: {arguments}"
            );
        }
    }

    #[test]
    fn validate_enforces_write_rules() {
        let two = vec![
            step("a", "submit_order", json!({}), None),
            step("b", "submit_order", json!({}), None),
        ];
        assert_eq!(
            validate(two, None, &write_is_submit).unwrap_err(),
            PlanError::MultipleWrites(vec!["a".into(), "b".into()]),
            "at most one write step per pipeline, reported in step order"
        );
        let dep = vec![
            step("w", "submit_order", json!({}), None),
            step("b", "quote", json!({"symbols": {"$from": "w"}}), None),
        ];
        assert_eq!(
            validate(dep, None, &write_is_submit).unwrap_err(),
            PlanError::DependsOnWrite {
                step: "b".into(),
                write: "w".into()
            },
            "nothing may consume a write step's result"
        );
        let price_ref = vec![
            step("a", "quote", json!({}), None),
            step(
                "w",
                "submit_order",
                json!({"symbol": "700.HK", "submitted_price": {"$from": "a", "jq": ".[0].last_done"}}),
                None,
            ),
        ];
        assert_eq!(
            validate(price_ref, None, &write_is_submit).unwrap_err(),
            PlanError::WriteRefForbidden {
                step: "w".into(),
                field: "submitted_price".into()
            },
            "order-defining fields must be literals the user can confirm"
        );
        let ok = vec![
            step("a", "quote", json!({}), None),
            step(
                "w",
                "submit_order",
                json!({"symbol": "700.HK", "remark": {"$from": "a"}}),
                None,
            ),
        ];
        assert!(
            validate(ok, None, &write_is_submit).is_ok(),
            "a write step may reference a non order-defining field"
        );
    }

    fn fake_runner() -> Runner {
        Arc::new(|tool: String, args: JsonObject| {
            Box::pin(async move {
                match tool.as_str() {
                    "picks" => CallToolResult::success(vec![Content::text(
                        json!({"data": [{"symbol": "700.HK"}, {"symbol": "AAPL.US"}]}).to_string(),
                    )]),
                    "quote" => {
                        let symbols = args.get("symbols").cloned().unwrap_or(json!([]));
                        CallToolResult::success(vec![Content::text(
                            json!(
                                symbols
                                    .as_array()
                                    .expect("symbols is an array")
                                    .iter()
                                    .map(|s| json!({"symbol": s, "last_done": 1}))
                                    .collect::<Vec<_>>()
                            )
                            .to_string(),
                        )])
                    }
                    "boom" => CallToolResult::error(vec![Content::text(
                        json!({"error_code": "x", "recoverable": "none"}).to_string(),
                    )]),
                    _ => CallToolResult::error(vec![Content::text("unknown")]),
                }
            })
        })
    }

    #[tokio::test]
    async fn run_resolves_refs_with_two_level_projection() {
        let plan = validate(
            vec![
                step("picks", "picks", json!({}), Some(".data | map(.symbol)")),
                step(
                    "quotes",
                    "quote",
                    json!({"symbols": {"$from": "picks", "jq": ".[:1]"}}),
                    Some("map(.symbol)"),
                ),
            ],
            None,
            &no_write,
        )
        .expect("valid plan");
        let out = run(&plan, fake_runner()).await;
        assert_eq!(out["picks"].status, Status::Ok, "the source step succeeds");
        assert_eq!(
            out["picks"].value,
            json!(["700.HK", "AAPL.US"]),
            "the step's own jq projects its result"
        );
        assert_eq!(
            out["quotes"].value,
            json!(["700.HK"]),
            "the reference's jq narrows the upstream value before the call"
        );
    }

    #[tokio::test]
    async fn run_marks_dependents_skipped_and_keeps_independent_steps() {
        let plan = validate(
            vec![
                step("bad", "boom", json!({}), None),
                step("dep", "quote", json!({"symbols": {"$from": "bad"}}), None),
                step("ok", "quote", json!({"symbols": ["700.HK"]}), None),
            ],
            Some(vec!["dep".into(), "ok".into()]),
            &no_write,
        )
        .expect("valid plan");
        let out = run(&plan, fake_runner()).await;
        assert_eq!(
            out["bad"].status,
            Status::Error,
            "an inner error result is an error outcome"
        );
        assert_eq!(
            out["bad"].value["error_code"], "x",
            "the inner envelope is kept as the outcome value"
        );
        assert_eq!(
            out["dep"].status,
            Status::Skipped,
            "a dependent of a failed step is skipped"
        );
        assert_eq!(
            out["ok"].status,
            Status::Ok,
            "an independent step still runs"
        );
    }

    #[tokio::test]
    async fn run_rejects_arguments_that_resolve_to_a_non_object() {
        let plan = validate(
            vec![
                step("picks", "picks", json!({}), Some(".data | map(.symbol)")),
                step("quotes", "quote", json!({"$from": "picks"}), None),
            ],
            None,
            &no_write,
        )
        .expect("valid plan");
        let out = run(&plan, fake_runner()).await;
        assert_eq!(
            out["quotes"].status,
            Status::Error,
            "arguments resolving to a non-object fail the step"
        );
        assert_eq!(
            out["quotes"].value["error_code"], "invalid_params",
            "the step reports an invalid_params envelope instead of panicking"
        );
    }

    #[tokio::test]
    async fn run_propagates_skips_along_chains_listed_out_of_order() {
        let plan = validate(
            vec![
                step("last", "quote", json!({"symbols": {"$from": "mid"}}), None),
                step("mid", "quote", json!({"symbols": {"$from": "bad"}}), None),
                step("bad", "boom", json!({}), None),
            ],
            Some(vec!["last".into()]),
            &no_write,
        )
        .expect("valid plan");
        let out = run(&plan, fake_runner()).await;
        assert_eq!(
            out["mid"].status,
            Status::Skipped,
            "the direct dependent of the failed step is skipped"
        );
        assert_eq!(
            out["mid"].value,
            json!({"skipped_because": "bad"}),
            "a skip names the step that blocked it"
        );
        assert_eq!(
            out["last"].status,
            Status::Skipped,
            "the skip propagates through the chain, not only one level"
        );
        assert_eq!(
            out["last"].value,
            json!({"skipped_because": "mid"}),
            "each skip names its immediate blocker"
        );
    }

    #[tokio::test]
    async fn run_reports_step_jq_failure_as_error() {
        let plan = validate(
            vec![step(
                "a",
                "quote",
                json!({"symbols": ["700.HK"]}),
                Some(".[0] | .x | ."),
            )],
            None,
            &no_write,
        )
        .expect("valid plan");
        let out = run(&plan, fake_runner()).await;
        assert_eq!(
            out["a"].status,
            Status::Ok,
            "null projection is not an error"
        );
        let plan = validate(
            vec![step(
                "a",
                "quote",
                json!({"symbols": ["700.HK"]}),
                Some("map("),
            )],
            None,
            &no_write,
        )
        .expect("valid plan");
        let out = run(&plan, fake_runner()).await;
        assert_eq!(
            out["a"].status,
            Status::Error,
            "an uncompilable jq filter fails the step"
        );
        assert_eq!(
            out["a"].value["error_code"], "jq_filter_error",
            "the step reports the jq_filter_error envelope"
        );
    }

    #[tokio::test]
    async fn run_never_exceeds_max_concurrency() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let watermark = Arc::new(AtomicUsize::new(0));
        let runner: Runner = {
            let in_flight = Arc::clone(&in_flight);
            let watermark = Arc::clone(&watermark);
            Arc::new(move |_tool: String, _args: JsonObject| {
                let in_flight = Arc::clone(&in_flight);
                let watermark = Arc::clone(&watermark);
                Box::pin(async move {
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    watermark.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                    CallToolResult::success(vec![Content::text("{}")])
                })
            })
        };
        let steps = (0..6)
            .map(|i| step(&format!("s{i}"), "quote", json!({}), None))
            .collect();
        let plan = validate(steps, None, &no_write).expect("valid plan");
        let out = run(&plan, runner).await;
        for i in 0..6 {
            assert_eq!(
                out[&format!("s{i}")].status,
                Status::Ok,
                "every independent step runs to completion"
            );
        }
        let peak = watermark.load(Ordering::SeqCst);
        assert!(
            peak <= MAX_CONCURRENCY,
            "at most MAX_CONCURRENCY steps may be in flight, saw {peak}"
        );
        assert_eq!(
            peak, MAX_CONCURRENCY,
            "the scheduler keeps all MAX_CONCURRENCY slots busy"
        );
    }

    #[tokio::test]
    async fn run_reports_a_panicking_step_and_keeps_going() {
        let runner: Runner = Arc::new(|tool: String, _args: JsonObject| {
            Box::pin(async move {
                assert!(tool != "boom_panic", "the fake tool panics on purpose");
                CallToolResult::success(vec![Content::text("{}")])
            })
        });
        let plan = validate(
            vec![
                step("bad", "boom_panic", json!({}), None),
                step("ok", "quote", json!({}), None),
            ],
            None,
            &no_write,
        )
        .expect("valid plan");
        let out = run(&plan, runner).await;
        assert_eq!(
            out["bad"].status,
            Status::Error,
            "a panicking step becomes an error outcome, not a hang"
        );
        assert_eq!(
            out["bad"].value["error_code"], "internal",
            "a panicked step reports the internal envelope"
        );
        assert_eq!(
            out["ok"].status,
            Status::Ok,
            "an independent step still completes after a panic"
        );
    }
}
