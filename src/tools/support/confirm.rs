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
            InputRequest::Elicitation(ElicitRequest::new(form(scope, preview, again))),
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

fn form(scope: &Scope, preview: &serde_json::Value, again: bool) -> ElicitRequestParams {
    // Unset fields are noise in a form a person reads.
    let shown = match preview {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    };
    let details = serde_json::to_string_pretty(&shown).unwrap_or_default();
    ElicitRequestParams::FormElicitationParams {
        meta: None,
        message: format!(
            "{}Confirm: {scope}\n\nNothing is sent until you confirm.\n\n{details}",
            if again {
                "Your earlier confirmation has expired (confirmations last 10 to 20 minutes) \
                 or was for a different request, so please confirm again.\n\n"
            } else {
                ""
            }
        ),
        requested_schema: ElicitationSchema::builder()
            .required_bool_property(FIELD, |b| {
                b.title("Confirm")
                    .description("Tick to send this request exactly as shown above.")
            })
            .build_unchecked(),
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
            .confirm(&scope, Some(&scope.code()), serde_json::json!({"x": 1}))
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
                .contains("buy 100 700.HK @ 400")
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
    fn equal_spellings_share_a_state() {
        assert_eq!(
            request_state(&order()),
            request_state(&Scope::order("buy", "700.hk", "100.0", "400.00"))
        );
    }
}
