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

use longbridge::DcRegion;
use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Meta};
use serde::Serialize;
use serde_json::Value;

use crate::tools::{McpContext, tool_json};

/// The signed-in trading account. `id`, `nickname` and `email` are the flat
/// shape profile-tool consumers read; the rest mirrors the account section of
/// the CLI's `longbridge auth status`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Profile {
    /// Stable identifier of the trading account: its aaid, or
    /// `member_id:account_channel` for tokens that carry no aaid.
    pub id: String,
    /// Human-readable account name, unique among one member's accounts.
    pub nickname: String,
    /// Always empty: Longbridge accounts have no email to show.
    pub email: String,
    /// Data-center region the account lives in: `AP` or `US`.
    pub dc_region: Option<&'static str>,
    /// Account number, e.g. `H11701971`.
    pub account_no: Option<String>,
    /// Longbridge member id that owns the account.
    pub member_id: Option<String>,
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
pub async fn get_me(mctx: &McpContext) -> Result<CallToolResult, McpError> {
    let mut profile = profile_from_token(&mctx.token).ok_or_else(|| {
        McpError::invalid_request(
            "The access token does not identify a trading account. Reconnect Longbridge to \
             sign in again.",
            None,
        )
    })?;
    profile.dc_region = Some(match mctx.dc_region().await {
        DcRegion::Us => "US",
        _ => "AP",
    });
    tool_json(&profile)
}

/// Build the profile from the token's `sub` claim, or `None` when the token
/// is not a Longbridge JWT or names no account.
fn profile_from_token(token: &str) -> Option<Profile> {
    let sub = crate::auth::jwt::subject(token)?;
    let account_channel = claim_str(&sub, "account_channel").unwrap_or_else(|| "lb".to_string());
    let aaid = claim_str(&sub, "target_aaid");
    let member_id = claim_str(&sub, "member_id");
    let account_no = claim_str(&sub, "account_no");
    let id = match (&aaid, &member_id) {
        (Some(aaid), _) => aaid.clone(),
        (None, Some(member_id)) => format!("{member_id}:{account_channel}"),
        (None, None) => return None,
    };
    // The full account number keeps names unique across one member's
    // accounts; tokens without one fall back to the aaid, then the member id.
    let number = account_no
        .as_deref()
        .or(aaid.as_deref())
        .or(member_id.as_deref())
        .unwrap_or_default();
    let nickname = format!("Longbridge {} {number}", account_label(&account_channel));
    Some(Profile {
        id,
        nickname,
        email: String::new(),
        dc_region: None,
        account_no,
        member_id,
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
    fn securities_account_reports_token_identity() {
        let token = token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb","target_aaid":"20012394","account_no":"H10012371"}"#,
        );
        assert_eq!(
            profile_from_token(&token),
            Some(Profile {
                id: "20012394".into(),
                nickname: "Longbridge Securities Account H10012371".into(),
                email: String::new(),
                dc_region: None,
                account_no: Some("H10012371".into()),
                member_id: Some("3162".into()),
            }),
            "a securities-account token must yield its aaid, account number and member id"
        );
    }

    #[test]
    fn tool_json_keeps_identity_fields() {
        let profile = profile_from_token(&token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb","target_aaid":"20012394","account_no":"H10012371"}"#,
        ))
        .expect("profile");
        let json = crate::serialize::to_tool_json(&profile).expect("profile must serialize");
        let json: Value = serde_json::from_str(&json).expect("tool JSON must parse");
        assert_eq!(json["id"], "20012394", "the tool serializer must keep id");
        assert_eq!(json["account_no"], "H10012371");
        assert_eq!(json["member_id"], "3162");
    }

    #[test]
    fn accounts_of_one_member_get_distinct_ids_and_names() {
        let live = profile_from_token(&token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb","target_aaid":"20012394","account_no":"H10012371"}"#,
        ))
        .expect("live account profile");
        let paper = profile_from_token(&token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb_papertrading","target_aaid":"20045638","account_no":"LB0000012371"}"#,
        ))
        .expect("paper account profile");
        assert_ne!(live.id, paper.id, "different accounts must differ by id");
        assert_ne!(
            live.nickname, paper.nickname,
            "accounts sharing an account-number tail must still differ by nickname"
        );
        assert_eq!(
            paper.nickname,
            "Longbridge Paper Trading Account LB0000012371"
        );
    }

    #[test]
    fn token_without_aaid_falls_back_to_member_and_channel() {
        let token = token_with_sub(r#"{"member_id":5012345600,"account_channel":"us_lb_uat"}"#);
        let profile = profile_from_token(&token).expect("US token profile");
        assert_eq!(profile.id, "5012345600:us_lb_uat");
        assert_eq!(profile.account_no, None);
        assert_eq!(
            profile.nickname,
            "Longbridge US Securities Account 5012345600"
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
}
