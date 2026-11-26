//! Who confirms a money-moving call: the user, through the client, when the
//! client can ask them; the model quoting back a confirmation code otherwise.
//!
//! [`dry_run`](super::dry_run) gates every order write behind a code the model
//! must quote back. That catches acting on an unpreviewed or different order,
//! but it cannot prove a *person* approved anything: the model reads the code
//! and could quote it in the same turn.
//!
//! MCP elicitation lets the server ask the user directly, through a form the
//! client renders, so approval no longer passes through the model. This module
//! picks one of three gates per call:
//!
//! - [`Gate::InputRequired`]: the client negotiated protocol `2026-07-28` or
//!   newer and declared form elicitation. The tool answers with an
//!   `input_required` result carrying the confirmation form; the client shows
//!   it, then retries the same call with the answer. This works on the
//!   stateless HTTP endpoints, because nothing is remembered between the two
//!   calls: `requestState` is derived from the order itself, so a retry for a
//!   different order does not match.
//! - [`Gate::Code`]: everything else. The existing two-step confirmation code,
//!   unchanged. That includes older clients that declare elicitation: before
//!   `2026-07-28` elicitation is a server-to-client request sent mid-call, and
//!   the stateless HTTP endpoints have no session to route the answer back to.
//!
//! When the user is asked directly, `execute` is ignored and no code is issued:
//! the only way to send the order is the user's own approval of this exact
//! order.

use super::dry_run::{Scope, current_window, live_windows};
use rmcp::ErrorData as McpError;
use rmcp::RoleServer;
use rmcp::model::{
    CallToolResponse, CallToolResult, ElicitRequest, ElicitRequestParams, ElicitResult,
    ElicitationAction, ElicitationSchema, InputRequest, InputRequests, InputRequiredResult,
    InputResponses,
};
use rmcp::service::RequestContext;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

/// The key of the one input request this module makes.
const INPUT_KEY: &str = "confirm";
/// The boolean field of the confirmation form.
const FIELD: &str = "confirm";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Answer with `input_required` and wait for the client's retry.
    InputRequired,
    /// The model quotes back the dry run's confirmation code.
    Code,
}

impl Gate {
    /// Choose the gate from what the client declared.
    pub fn choose(form_elicitation: bool, new_protocol: bool) -> Self {
        if form_elicitation && new_protocol {
            Self::InputRequired
        } else {
            Self::Code
        }
    }

    fn from_context(ctx: &RequestContext<RoleServer>) -> Self {
        let form = ctx
            .client_capabilities()
            .and_then(|caps| caps.elicitation)
            // A bare `elicitation: {}` predates the form/url split and means form.
            .is_some_and(|e| e.form.is_some() || e.url.is_none());
        // `2026-07-28` dropped the initialize handshake; that is also the
        // version that forbids server-to-client requests in favour of
        // `input_required`.
        let new_protocol = ctx.protocol_version().is_some_and(|v| !v.has_initialize());
        Self::choose(form, new_protocol)
    }
}

/// What the tool should do after asking for confirmation.
pub enum Decision {
    /// The user (or a valid code) approved this exact request: send it.
    Proceed,
    /// Do not send anything; return this to the caller.
    Respond(CallToolResult),
}

/// The confirmation gate for one tool call.
pub struct Confirmer {
    gate: Gate,
    input_responses: Option<InputResponses>,
    request_state: Option<String>,
    /// Set when the call must end with `input_required` instead of a tool
    /// result. Read by the tool wrapper after the call returns.
    input_required: Mutex<Option<InputRequiredResult>>,
}

impl Confirmer {
    pub fn new(
        ctx: &RequestContext<RoleServer>,
        input_responses: Option<InputResponses>,
        request_state: Option<String>,
    ) -> Self {
        Self {
            gate: Gate::from_context(ctx),
            input_responses,
            request_state,
            input_required: Mutex::new(None),
        }
    }

    /// A confirmer that always uses the confirmation code.
    #[cfg(test)]
    pub fn code_only() -> Self {
        Self::with_gate(Gate::Code, None, None)
    }

    #[cfg(test)]
    fn with_gate(
        gate: Gate,
        input_responses: Option<InputResponses>,
        request_state: Option<String>,
    ) -> Self {
        Self {
            gate,
            input_responses,
            request_state,
            input_required: Mutex::new(None),
        }
    }

    /// The `input_required` result this call must end with, if any.
    pub fn take_input_required(&self) -> Option<InputRequiredResult> {
        self.input_required
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// The tool's response: the pending `input_required` when the user is
    /// being asked, the tool result otherwise.
    pub fn respond(&self, result: CallToolResult) -> CallToolResponse {
        match self.take_input_required() {
            Some(input_required) => input_required.into(),
            None => result.into(),
        }
    }

    /// Whether this call will show a preview (a dry run or a confirmation
    /// form), so the caller can skip fetching one for a code-confirmed execute.
    pub fn needs_preview(&self, has_code: bool) -> bool {
        self.gate != Gate::Code || !has_code
    }

    /// Gate one money-moving call.
    ///
    /// `execute` is the code the model quoted, if any; it only counts under
    /// [`Gate::Code`]. `preview` is what the dry run would show.
    pub async fn confirm(
        &self,
        scope: &Scope,
        execute: Option<&str>,
        preview: serde_json::Value,
    ) -> Result<Decision, McpError> {
        match self.gate {
            Gate::Code => match execute {
                Some(code) => scope.verify(code).map(|()| Decision::Proceed),
                None => super::dry_run::result(scope, preview).map(Decision::Respond),
            },
            Gate::InputRequired => Ok(self.input_required_round(scope, &preview)),
        }
    }

    fn input_required_round(&self, scope: &Scope, preview: &serde_json::Value) -> Decision {
        let window = current_window();
        let state = request_state_in(scope, window);
        // The retry must be for the order the form described, within the
        // window: a different order or an expired approval recomputes a
        // different state, and the user is asked again.
        if is_live_state(scope, self.request_state.as_deref(), window) {
            let answer = self
                .input_responses
                .as_ref()
                .and_then(|r| r.get(INPUT_KEY))
                .and_then(|v| serde_json::from_value::<ElicitResult>(v.clone()).ok());
            if let Some(answer) = answer {
                return match accepted(&answer) {
                    true => Decision::Proceed,
                    false => Decision::Respond(not_confirmed(scope)),
                };
            }
        }
        // A retry that carried a state but did not match was approved for a
        // different request or too long ago: say why the user is asked again.
        let again = self.request_state.is_some();
        let mut requests = InputRequests::new();
        requests.insert(
            INPUT_KEY.to_string(),
            InputRequest::Elicitation(ElicitRequest::new(form(preview, again))),
        );
        *self
            .input_required
            .lock()
            .unwrap_or_else(|e| e.into_inner()) =
            Some(InputRequiredResult::new(Some(requests), Some(state)));
        // Placeholder for the metrics wrapper; the caller replaces it with the
        // `input_required` result above.
        Decision::Respond(crate::tools::tool_result(
            "Waiting for the user to confirm this request in the client.".to_string(),
        ))
    }
}

/// The opaque state echoed back on retry: a digest of the order and the
/// confirmation window, so a retry for a different order cannot reuse the
/// user's approval, and an approval cannot be replayed once it has expired.
#[cfg(test)]
fn request_state(scope: &Scope) -> String {
    request_state_in(scope, current_window())
}

fn request_state_in(scope: &Scope, window: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"longbridge-mcp/elicit-confirmation/v2");
    hasher.update(window.to_be_bytes());
    hasher.update(scope.to_string().as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("confirm-v2:{hex}")
}

/// Whether `state` is this order's state in a window that has not expired.
fn is_live_state(scope: &Scope, state: Option<&str>, window: u64) -> bool {
    state.is_some_and(|state| live_windows(window).any(|w| request_state_in(scope, w) == state))
}

fn accepted(answer: &ElicitResult) -> bool {
    answer.action == ElicitationAction::Accept
        && answer
            .content
            .as_ref()
            .and_then(|c| c.get(FIELD))
            .and_then(serde_json::Value::as_bool)
            == Some(true)
}

fn form(preview: &serde_json::Value, again: bool) -> ElicitRequestParams {
    ElicitRequestParams::FormElicitationParams {
        meta: None,
        message: format!(
            "{}{}\n\nNothing is sent until you confirm.",
            if again {
                "Your earlier confirmation has expired (confirmations last 10 to 20 minutes) \
                 or was for a different request, so please confirm again.\n\n"
            } else {
                ""
            },
            summary(preview)
        ),
        requested_schema: ElicitationSchema::builder()
            .required_bool_property(FIELD, |b| {
                b.title("Confirm")
                    .description("Tick to send this request exactly as shown above.")
            })
            .build_unchecked(),
    }
}

/// Fields of an existing order (as echoed by a cancel / replace preview)
/// worth showing a person, in reading order.
const ORDER_FIELDS: &[&str] = &[
    "symbol",
    "stock_name",
    "side",
    "order_type",
    "quantity",
    "price",
    "trigger_price",
    "time_in_force",
    "executed_quantity",
    "status",
    "lookup_failed",
    "note",
];
/// Preview keys holding an existing order.
const ORDER_KEYS: &[&str] = &["order", "current_order"];
/// Preview keys that mean nothing to a person.
const HIDDEN_KEYS: &[&str] = &["action", "client_request_id"];
/// Most lines a form shows before it says how many it left out.
const MAX_LINES: usize = 30;

/// A preview as the plain-text summary a person reads in the confirmation
/// form: a title for what is about to happen, then one `• Label: value` line
/// per field, with unset fields and bookkeeping left out.
fn summary(preview: &serde_json::Value) -> String {
    let serde_json::Value::Object(map) = preview else {
        return preview.to_string();
    };
    let title = map
        .get("action")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "Confirm this request".to_string(), title_of);
    let mut lines = Vec::new();
    for (key, value) in map {
        if !HIDDEN_KEYS.contains(&key.as_str()) {
            push_field(&mut lines, key, value, 0);
        }
    }
    if lines.len() > MAX_LINES {
        let more = lines.len() - MAX_LINES;
        lines.truncate(MAX_LINES);
        lines.push(format!("… and {more} more"));
    }
    format!("{title}\n\n{}", lines.join("\n"))
}

fn title_of(action: &str) -> String {
    match action {
        "submit_order" => "Place this order",
        "cancel_order" => "Cancel this order",
        "replace_order" => "Modify this order",
        "submit_multileg_order" => "Place this multi-leg option order",
        "fund_submit_order" => "Place this fund order",
        "fund_cancel_order" => "Cancel this fund order",
        "grid_submit" => "Start this grid strategy",
        "grid_replace" => "Change this grid strategy",
        "grid_cancel" => "Stop this grid strategy",
        "grid_suspend" => "Suspend this grid strategy",
        "grid_restart" => "Restart this grid strategy",
        "dca_create" => "Create this recurring investment (DCA) plan",
        "dca_update" => "Change this DCA plan",
        "dca_pause" => "Pause this DCA plan",
        "dca_resume" => "Resume this DCA plan",
        "dca_stop" => "Stop this DCA plan permanently",
        "alert_add" => "Add this price alert",
        "alert_delete" => "Delete this price alert",
        "alert_enable" => "Enable this price alert",
        "alert_disable" => "Disable this price alert",
        other => return label(other),
    }
    .to_string()
}

/// `new_trailing_amount` → `New trailing amount`.
fn label(key: &str) -> String {
    let text = key.replace('_', " ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn push_field(lines: &mut Vec<String>, key: &str, value: &serde_json::Value, depth: usize) {
    use serde_json::Value;
    let indent = "  ".repeat(depth);
    let label = label(key);
    match value {
        Value::Null => {}
        Value::String(s) if s.is_empty() => {}
        Value::String(s) => lines.push(format!("{indent}• {label}: {s}")),
        Value::Bool(_) | Value::Number(_) => lines.push(format!("{indent}• {label}: {value}")),
        Value::Array(items) if items.is_empty() => {}
        Value::Array(items) => {
            let shown: Vec<String> = items.iter().map(inline).collect();
            lines.push(format!("{indent}• {label}: {}", shown.join("; ")));
        }
        // Deep structures (a grid rule's levels) stay on one line.
        Value::Object(_) if depth >= 2 => {
            lines.push(format!("{indent}• {label}: {}", inline(value)));
        }
        Value::Object(map) => {
            let mut nested = Vec::new();
            if ORDER_KEYS.contains(&key) {
                for field in ORDER_FIELDS {
                    if let Some(v) = map.get(*field) {
                        push_field(&mut nested, field, v, depth + 1);
                    }
                }
            } else {
                for (k, v) in map {
                    push_field(&mut nested, k, v, depth + 1);
                }
            }
            if !nested.is_empty() {
                lines.push(format!("{indent}• {label}:"));
                lines.extend(nested);
            }
        }
    }
}

/// A value on one line: objects as `key value, …`, without JSON punctuation.
fn inline(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(map) => map
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| format!("{} {}", label(k).to_lowercase(), inline(v)))
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::Array(items) => items.iter().map(inline).collect::<Vec<_>>().join("; "),
        other => other.to_string(),
    }
}

fn not_confirmed(scope: &Scope) -> CallToolResult {
    crate::tools::tool_result(format!(
        "The user did not confirm this request ({scope}). Nothing was sent. Do not retry \
         unless the user asks again."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> Scope {
        Scope::order("Buy", "700.HK", "100", "400")
    }

    fn answer(action: ElicitationAction, confirm: Option<bool>) -> serde_json::Value {
        let mut r = ElicitResult::new(action);
        if let Some(c) = confirm {
            r = r.with_content(serde_json::json!({ FIELD: c }));
        }
        serde_json::to_value(r).unwrap()
    }

    fn retry(state: String, value: serde_json::Value) -> Confirmer {
        let mut responses = InputResponses::new();
        responses.insert(INPUT_KEY.to_string(), value);
        Confirmer::with_gate(Gate::InputRequired, Some(responses), Some(state))
    }

    #[test]
    fn gate_follows_client_capabilities() {
        assert_eq!(Gate::choose(true, true), Gate::InputRequired);
        assert_eq!(Gate::choose(false, true), Gate::Code);
        // Before 2026-07-28 elicitation is a mid-call server request, which
        // the stateless endpoints cannot route back.
        assert_eq!(Gate::choose(true, false), Gate::Code);
        assert_eq!(Gate::choose(false, false), Gate::Code);
    }

    #[tokio::test]
    async fn code_gate_keeps_the_two_step_flow() {
        let c = Confirmer::code_only();
        let scope = order();
        let Decision::Respond(dry) = c
            .confirm(&scope, None, serde_json::json!({}))
            .await
            .unwrap()
        else {
            panic!("no code must mean a dry run");
        };
        assert!(format!("{dry:?}").contains(&scope.code()));
        assert!(matches!(
            c.confirm(&scope, Some(&scope.code()), serde_json::json!({}))
                .await
                .unwrap(),
            Decision::Proceed
        ));
        assert!(
            c.confirm(&scope, Some("xyz"), serde_json::json!({}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn first_call_asks_and_ignores_a_quoted_code() {
        let c = Confirmer::with_gate(Gate::InputRequired, None, None);
        let scope = order();
        // Even a correct code does not skip the user.
        let d = c
            .confirm(
                &scope,
                Some(&scope.code()),
                serde_json::json!({
                    "action": "submit_order",
                    "symbol": "700.HK",
                    "side": "Buy",
                    "quantity": "100",
                    "price": "400",
                    "remark": null,
                }),
            )
            .await
            .unwrap();
        assert!(matches!(d, Decision::Respond(_)));
        let ir = c.take_input_required().expect("input_required");
        assert_eq!(
            ir.request_state.as_deref(),
            Some(request_state(&scope).as_str())
        );
        let wire = serde_json::to_value(&ir).unwrap();
        assert_eq!(wire["resultType"], "input_required");
        let req = &wire["inputRequests"][INPUT_KEY];
        assert_eq!(req["method"], "elicitation/create");
        assert_eq!(req["params"]["mode"], "form");
        assert!(
            req["params"]["message"]
                .as_str()
                .unwrap()
                .starts_with("Place this order\n\n• Symbol: 700.HK\n• Side: Buy\n")
        );
        assert_eq!(req["params"]["requestedSchema"]["required"][0], FIELD);
    }

    #[tokio::test]
    async fn accepted_retry_for_the_same_order_proceeds() {
        let scope = order();
        let c = retry(
            request_state(&scope),
            answer(ElicitationAction::Accept, Some(true)),
        );
        let d = c
            .confirm(&scope, None, serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(d, Decision::Proceed));
        assert!(c.take_input_required().is_none());
    }

    #[tokio::test]
    async fn declined_or_unticked_retry_sends_nothing() {
        let scope = order();
        for value in [
            answer(ElicitationAction::Decline, None),
            answer(ElicitationAction::Cancel, None),
            answer(ElicitationAction::Accept, Some(false)),
            answer(ElicitationAction::Accept, None),
        ] {
            let c = retry(request_state(&scope), value);
            let d = c
                .confirm(&scope, None, serde_json::json!({}))
                .await
                .unwrap();
            let Decision::Respond(r) = d else {
                panic!("must not proceed");
            };
            assert!(format!("{r:?}").contains("Nothing was sent"));
            assert!(c.take_input_required().is_none());
        }
    }

    #[tokio::test]
    async fn approval_for_one_order_does_not_send_another() {
        let approved = order();
        let other = Scope::order("Buy", "700.HK", "100", "410");
        let c = retry(
            request_state(&approved),
            answer(ElicitationAction::Accept, Some(true)),
        );
        let d = c
            .confirm(&other, None, serde_json::json!({}))
            .await
            .unwrap();
        assert!(matches!(d, Decision::Respond(_)));
        // It asks again, for the order actually being sent, and says why.
        let ir = c.take_input_required().expect("asks again");
        let wire = serde_json::to_value(&ir).unwrap();
        assert!(
            wire["inputRequests"][INPUT_KEY]["params"]["message"]
                .as_str()
                .unwrap()
                .starts_with("Your earlier confirmation has expired")
        );
        assert_eq!(
            ir.request_state.as_deref(),
            Some(request_state(&other).as_str())
        );
    }

    #[test]
    fn an_approval_expires_after_the_next_window() {
        let scope = order();
        let issued = request_state_in(&scope, 1000);
        assert!(is_live_state(&scope, Some(&issued), 1000));
        assert!(is_live_state(&scope, Some(&issued), 1001));
        assert!(!is_live_state(&scope, Some(&issued), 1002));
        assert!(!is_live_state(&scope, None, 1000));
    }

    #[test]
    fn summary_reads_as_text_not_json() {
        let text = summary(&serde_json::json!({
            "action": "cancel_order",
            "order_id": "701",
            "is_attached": false,
            "client_request_id": "abc",
            "order": {
                "order_id": "701",
                "symbol": "700.HK",
                "side": "Buy",
                "quantity": "100",
                "price": "400",
                "status": "New",
                "submitted_at": "2026-11-26T01:00:00Z",
                "msg": "",
            },
        }));
        assert_eq!(
            text,
            "Cancel this order\n\n• Order id: 701\n• Is attached: false\n• Order:\n  \
             • Symbol: 700.HK\n  • Side: Buy\n  • Quantity: 100\n  • Price: 400\n  • Status: New"
        );
        let legs = summary(&serde_json::json!({
            "action": "submit_multileg_order",
            "legs": [{"symbol": "AAPL260116C200000.US", "ratio_quantity": "1"}],
        }));
        assert!(
            legs.contains("• Legs: symbol AAPL260116C200000.US, ratio quantity 1"),
            "{legs}"
        );
        assert!(!legs.contains('{'), "{legs}");
    }

    #[test]
    fn equal_spellings_share_a_state() {
        assert_eq!(
            request_state(&order()),
            request_state(&Scope::order("buy", "700.hk", "100.0", "400.00"))
        );
    }
}
