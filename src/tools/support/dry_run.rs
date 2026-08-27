//! The execution gate shared by every money-moving tool.
//!
//! `submit_order` / `cancel_order` / `replace_order` and the grid writes
//! (`grid_submit` / `grid_replace` / `grid_cancel` / `grid_suspend` /
//! `grid_restart`) preview by default and act only when the caller quotes back
//! the three-digit `confirmation_code` the preview returned.
//!
//! The code is **random**, not derived from the request. This server is open
//! source: anything computed from the arguments could be recomputed by a caller
//! that skipped the preview, which would leave the gate decorative. A random
//! code can only be learned by reading the preview.
//!
//! Pending codes live in memory rather than on disk — the server is stateless
//! across HTTP requests but long-lived as a process, and a restart simply
//! invalidates codes that were about to expire anyway. Three properties follow,
//! each closing a specific mistake:
//!
//! - **single use** — the entry is spent on the way through, *including on a
//!   wrong guess*, so a code can neither be replayed nor brute-forced through
//!   the thousand possibilities.
//! - **bound to the caller and the request** — entries are keyed by both, so a
//!   code cannot cross between users or survive an edit to the order.
//! - **short lived** — ten minutes.
//!
//! What it does not do is prove a *human* saw the preview: a model can call the
//! dry run, read the code and execute in the same turn. Enforcing human review
//! needs a control outside this process, such as a client-side approval hook.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use sha2::{Digest, Sha256};

/// Long enough for a user to read a preview and answer, short enough that a
/// code sitting in an old transcript is dead.
const TTL_SECONDS: u64 = 600;

/// Keyed by (caller, request) so a code can never be spent on a different
/// order, or by a different user who happened to draw the same three digits.
type Pending = HashMap<(String, String), (String, u64)>;

fn pending() -> &'static Mutex<Pending> {
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Stable identity of the action being previewed. Any field that changes what
/// would reach the exchange belongs in `parts`.
pub fn fingerprint(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        // Length-prefixed so ["ab", "c"] and ["a", "bc"] cannot collide.
        hasher.update(part.len().to_le_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Callers are identified by a hash of their bearer token: enough to keep two
/// users' pending codes apart without holding the token itself in this map.
fn caller_key(token: &str) -> String {
    fingerprint(&[token])
}

fn random_code() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let mut hasher = Sha256::new();
    hasher.update(nanos.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(now().to_le_bytes());
    let digest = hasher.finalize();
    let n = (u32::from(digest[0]) << 8) | u32::from(digest[1]);
    format!("{:03}", n % 1000)
}

fn issue(token: &str, request: &str) -> String {
    let code = random_code();
    let deadline = now() + TTL_SECONDS;
    if let Ok(mut map) = pending().lock() {
        // Drop anything expired on the way past, so the map stays bounded
        // without a background sweeper.
        map.retain(|_, (_, expires_at)| *expires_at > now());
        map.insert(
            (caller_key(token), request.to_string()),
            (code.clone(), deadline),
        );
    }
    code
}

/// Validate `code` for this caller and request, then spend it. Every failure
/// path leaves the caller with an explicit next step and no order placed.
pub fn consume(token: &str, request: &str, code: &str) -> Result<(), McpError> {
    let entry = pending()
        .lock()
        .ok()
        // Removed before it is checked: a rejected guess must not leave a live
        // code behind for a second attempt.
        .and_then(|mut map| map.remove(&(caller_key(token), request.to_string())));

    let Some((expected, expires_at)) = entry else {
        return Err(McpError::invalid_params(
            "No confirmation code is pending for this request. Call this tool again \
             without `execute` first, show the returned preview to the user, and only \
             then re-call with the `confirmation_code` it returned."
                .to_string(),
            None,
        ));
    };
    if now() > expires_at {
        return Err(McpError::invalid_params(
            format!(
                "That confirmation code has expired (codes last {} minutes). Call this \
                 tool again without `execute` to get a new one.",
                TTL_SECONDS / 60
            ),
            None,
        ));
    }
    if expected != code {
        return Err(McpError::invalid_params(
            "Confirmation code does not match the preview for this request. Call this \
             tool again without `execute` and use the code it returns."
                .to_string(),
            None,
        ));
    }
    Ok(())
}

/// The standard dry-run envelope: `dry_run` distinguishes it from a real
/// result, `preview` carries what would have been sent, and
/// `confirmation_code` is what the caller must quote back.
pub fn result(
    token: &str,
    request: &str,
    preview: serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let code = issue(token, request);
    crate::tools::tool_json(&serde_json::json!({
        "dry_run": true,
        "preview": preview,
        "confirmation_code": code,
        "next_step": format!(
            "DRY RUN — nothing was sent to the exchange. Show this preview to the user and \
             ask them to confirm it. Only call this tool again with execute=\"{code}\" after \
             the user has explicitly confirmed this exact order. The code is single use, \
             expires in {} minutes, and applies only to this exact request. Never quote it \
             back on your own initiative.",
            TTL_SECONDS / 60
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_three_digits() {
        let code = random_code();
        assert_eq!(code.len(), 3);
        assert!(code.chars().all(|c| c.is_ascii_digit()), "{code}");
    }

    #[test]
    fn fingerprint_is_unambiguous_across_field_boundaries() {
        assert_ne!(fingerprint(&["ab", "c"]), fingerprint(&["a", "bc"]));
    }

    #[test]
    fn a_matching_code_is_accepted_exactly_once() {
        let req = fingerprint(&["once", "700.HK"]);
        let code = issue("tok-once", &req);
        assert!(
            consume("tok-once", &req, &code).is_ok(),
            "first use must pass"
        );
        assert!(
            consume("tok-once", &req, &code).is_err(),
            "replay must fail"
        );
    }

    #[test]
    fn a_wrong_guess_spends_the_pending_code() {
        // Otherwise a model could sit and walk all 1000 values.
        let req = fingerprint(&["guess", "700.HK"]);
        let code = issue("tok-guess", &req);
        let wrong = if code == "000" { "001" } else { "000" };
        assert!(consume("tok-guess", &req, wrong).is_err());
        assert!(
            consume("tok-guess", &req, &code).is_err(),
            "the real code must not survive a failed guess"
        );
    }

    #[test]
    fn a_code_does_not_carry_over_to_a_different_request() {
        let previewed = fingerprint(&["buy", "700.HK", "400"]);
        let code = issue("tok-fp", &previewed);
        let edited = fingerprint(&["buy", "700.HK", "410"]);
        assert!(consume("tok-fp", &edited, &code).is_err());
    }

    #[test]
    fn a_code_does_not_cross_between_callers() {
        let req = fingerprint(&["cross", "700.HK"]);
        let code = issue("tok-a", &req);
        assert!(
            consume("tok-b", &req, &code).is_err(),
            "another caller must not be able to spend it"
        );
    }

    #[test]
    fn an_expired_code_is_rejected() {
        let req = fingerprint(&["expired", "700.HK"]);
        let code = random_code();
        pending().lock().unwrap().insert(
            (caller_key("tok-exp"), req.clone()),
            (code.clone(), now() - 1),
        );
        let err = consume("tok-exp", &req, &code).expect_err("expired must fail");
        assert!(format!("{err:?}").contains("expired"), "{err:?}");
    }
}
