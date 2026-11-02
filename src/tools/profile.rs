//! `get_me`: which trading account the bearer token is bound to.
//!
//! Multi-account MCP gateways (mcp-portal, ChatGPT connectors) call a profile
//! tool once after each OAuth sign-in to tell connected accounts apart: same
//! `id` means the same account was signed in to again, a new `id` adds an
//! account, and `nickname` is the name users and models pick it by.
//!
//! The answer is read from the token's `sub` claim, not fetched: it needs no
//! upstream round trip and no scope, so it works for a token granted only the
//! quote scope and answers well inside the gateway's sign-in timeout.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Meta};
use serde::Serialize;
use serde_json::Value;

use crate::tools::{McpContext, tool_json};

/// Bullets standing in for the hidden head of an account number.
const MASK: &str = "••••";

/// How many trailing characters of the account number the nickname shows.
const TAIL_LEN: usize = 4;

/// The signed-in trading account, in the flat `{id, nickname, email}` shape
/// profile-tool consumers read.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Profile {
    /// Stable identifier of the trading account: its aaid, or
    /// `member_id:account_channel` for tokens that carry no aaid.
    pub id: String,
    /// Human-readable account name, unique among one member's accounts.
    pub nickname: String,
    /// Always empty: Longbridge accounts have no email to show.
    pub email: String,
}

/// `_meta` marking `get_me` as the profile tool (the ChatGPT connector
/// convention), so gateways that do not match on the tool name find it too.
pub fn profile_tool_meta() -> Meta {
    let mut meta = Meta::new();
    meta.0
        .insert("openai/profile".to_string(), Value::Bool(true));
    meta
}

/// Describe the trading account the request's token is bound to.
pub fn get_me(mctx: &McpContext) -> Result<CallToolResult, McpError> {
    let profile = profile_from_token(&mctx.token).ok_or_else(|| {
        McpError::invalid_request(
            "The access token does not identify a trading account. Reconnect Longbridge to \
             sign in again.",
            None,
        )
    })?;
    tool_json(&profile)
}

/// Build the profile from the token's `sub` claim, or `None` when the token
/// is not a Longbridge JWT or names no account.
fn profile_from_token(token: &str) -> Option<Profile> {
    let sub = crate::auth::jwt::subject(token)?;
    let account_channel = claim_str(&sub, "account_channel").unwrap_or_else(|| "lb".to_string());
    let aaid = claim_str(&sub, "target_aaid");
    let member_id = claim_str(&sub, "member_id");
    let id = match (&aaid, &member_id) {
        (Some(aaid), _) => aaid.clone(),
        (None, Some(member_id)) => format!("{member_id}:{account_channel}"),
        (None, None) => return None,
    };
    let tail_source = claim_str(&sub, "account_no")
        .or(aaid)
        .or(member_id)
        .unwrap_or_default();
    let nickname = format!(
        "Longbridge {} {MASK}{}",
        account_label(&account_channel),
        tail(&tail_source)
    );
    Some(Profile {
        id,
        nickname,
        email: String::new(),
    })
}

/// A non-empty claim as a string; numeric claims (e.g. `member_id`) are
/// stringified.
fn claim_str(sub: &Value, key: &str) -> Option<String> {
    match &sub[key] {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Account kind shown in the nickname, from the account channel.
fn account_label(account_channel: &str) -> &'static str {
    if account_channel.contains("papertrading") {
        "Paper Trading Account"
    } else if account_channel.starts_with("us") {
        "US Securities Account"
    } else {
        "Securities Account"
    }
}

/// The last [`TAIL_LEN`] characters of `s` (all of it when shorter).
fn tail(s: &str) -> &str {
    let start = s
        .char_indices()
        .rev()
        .nth(TAIL_LEN - 1)
        .map_or(0, |(i, _)| i);
    &s[start..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::jwt::test_token;

    /// A token whose `sub` claim is `sub` (a JSON object literal).
    fn token_with_sub(sub: &str) -> String {
        let payload = serde_json::json!({ "sub": sub, "exp": 1 }).to_string();
        test_token(&payload)
    }

    #[test]
    fn securities_account_uses_aaid_and_account_no_tail() {
        let token = token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb","target_aaid":"20012394","account_no":"H10012371"}"#,
        );
        assert_eq!(
            profile_from_token(&token),
            Some(Profile {
                id: "20012394".into(),
                nickname: "Longbridge Securities Account ••••2371".into(),
                email: String::new(),
            }),
            "a securities-account token must yield its aaid and masked account number"
        );
    }

    #[test]
    fn accounts_of_one_member_get_distinct_ids_and_names() {
        let live = profile_from_token(&token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb","target_aaid":"20012394","account_no":"H10012371"}"#,
        ))
        .expect("live account profile");
        let paper = profile_from_token(&token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb_papertrading","target_aaid":"20045638","account_no":"LB0000012323"}"#,
        ))
        .expect("paper account profile");
        assert_ne!(live.id, paper.id, "different accounts must differ by id");
        assert_ne!(
            live.nickname, paper.nickname,
            "different accounts must differ by nickname"
        );
        assert_eq!(paper.nickname, "Longbridge Paper Trading Account ••••2323");
    }

    #[test]
    fn token_without_aaid_falls_back_to_member_and_channel() {
        let token = token_with_sub(r#"{"member_id":5012345600,"account_channel":"us_lb_uat"}"#);
        let profile = profile_from_token(&token).expect("US token profile");
        assert_eq!(profile.id, "5012345600:us_lb_uat");
        assert_eq!(
            profile.nickname,
            "Longbridge US Securities Account ••••5600"
        );
    }

    #[test]
    fn unidentifiable_tokens_yield_none() {
        assert_eq!(
            profile_from_token("opaque-token"),
            None,
            "a non-JWT token names no account"
        );
        assert_eq!(
            profile_from_token(&token_with_sub(r#"{"account_channel":"lb"}"#)),
            None,
            "a sub without aaid or member_id names no account"
        );
    }

    #[test]
    fn tail_handles_short_and_multibyte_input() {
        assert_eq!(tail("H10012371"), "2371");
        assert_eq!(tail("12"), "12");
        assert_eq!(tail("账户号码一二"), "号码一二");
    }
}
