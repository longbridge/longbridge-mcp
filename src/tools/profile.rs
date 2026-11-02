//! `get_me`: which trading account the bearer token is bound to.
//!
//! Multi-account MCP gateways (mcp-portal, ChatGPT connectors) call a profile
//! tool once after each OAuth sign-in to tell connected accounts apart: same
//! `id` means the same account was signed in to again, a new `id` adds an
//! account, and `nickname` is the name users and models pick it by.
//!
//! Everything but the account type is read from the token's `sub` claim, so
//! the core answer needs no upstream round trip and no scope: it works for a
//! token granted only the quote scope. The account type (e.g. `M` for margin)
//! is not in the token; like `longbridge auth status` in the CLI it is read
//! from the latest daily statement, best effort and under a short deadline.

use std::sync::LazyLock;
use std::time::Duration;

use longbridge::asset::{GetStatementListOptions, GetStatementOptions, StatementType};
use longbridge::{AssetContext, DcRegion};
use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Meta};
use serde::Serialize;
use serde_json::Value;

use crate::error::Error;
use crate::tools::{McpContext, tool_result};

/// Bullets standing in for the hidden head of an account number.
const MASK: &str = "••••";

/// How many trailing characters of the account number the nickname shows.
const TAIL_LEN: usize = 4;

/// Deadline for the best-effort account-type lookup. Gateways call `get_me`
/// synchronously during sign-in, so a slow statement service must not hold
/// the identity answer back.
const ACCOUNT_TYPE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Client for downloading statement files from their pre-signed URLs.
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(ACCOUNT_TYPE_TIMEOUT)
        .user_agent(concat!("longbridge-mcp/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("statement http client must build with static settings")
});

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
    /// The trading account's aaid, when the token carries one.
    pub aaid: Option<String>,
    /// Account channel, e.g. `lb` or `lb_papertrading`.
    pub account_channel: String,
    /// Data-center region the account lives in: `AP` or `US`.
    pub dc_region: Option<&'static str>,
    /// Account number, e.g. `H11701971`.
    pub account_no: Option<String>,
    /// Account type from the latest daily statement, e.g. `M` (margin) or `C`
    /// (cash); absent when there is no statement or it could not be read.
    pub account_type: Option<String>,
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
    let region = mctx.dc_region().await;
    profile.dc_region = Some(match region {
        DcRegion::Us => "US",
        _ => "AP",
    });
    // Statements are served by the AP asset endpoints only.
    if region != DcRegion::Us {
        profile.account_type = tokio::time::timeout(ACCOUNT_TYPE_TIMEOUT, account_type(mctx))
            .await
            .ok()
            .flatten();
    }
    // Plain serde, not `to_tool_json`: that serializer nulls `aaid` and
    // `account_channel`, which are exactly what this tool exists to report.
    let json = serde_json::to_string(&profile).map_err(Error::Serialize)?;
    Ok(tool_result(json))
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
    let tail_source = account_no
        .as_deref()
        .or(aaid.as_deref())
        .or(member_id.as_deref())
        .unwrap_or_default();
    let nickname = format!(
        "Longbridge {} {MASK}{}",
        account_label(&account_channel),
        tail(tail_source)
    );
    Some(Profile {
        id,
        nickname,
        email: String::new(),
        aaid,
        account_channel,
        dc_region: None,
        account_no,
        account_type: None,
        member_id,
    })
}

/// The account type (`MemberInfo.AccountType`) from the latest daily
/// statement, or `None` when there is none or any step fails.
async fn account_type(mctx: &McpContext) -> Option<String> {
    let ctx = AssetContext::new(mctx.create_config());
    let list = ctx
        .statements(
            GetStatementListOptions::new(StatementType::Daily)
                .page(1)
                .page_size(1),
        )
        .await
        .ok()?;
    let file_key = list.list.into_iter().next()?.file_key;
    let url = ctx
        .statement_download_url(GetStatementOptions::new(file_key))
        .await
        .ok()?
        .url;
    let statement: Value = HTTP_CLIENT.get(url).send().await.ok()?.json().await.ok()?;
    statement_account_type(&statement)
}

/// `MemberInfo.AccountType` of a statement file, when non-empty.
fn statement_account_type(statement: &Value) -> Option<String> {
    statement["MemberInfo"]["AccountType"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
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
    fn securities_account_reports_token_identity() {
        let token = token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb","target_aaid":"20012394","account_no":"H10012371"}"#,
        );
        assert_eq!(
            profile_from_token(&token),
            Some(Profile {
                id: "20012394".into(),
                nickname: "Longbridge Securities Account ••••2371".into(),
                email: String::new(),
                aaid: Some("20012394".into()),
                account_channel: "lb".into(),
                dc_region: None,
                account_no: Some("H10012371".into()),
                account_type: None,
                member_id: Some("3162".into()),
            }),
            "a securities-account token must yield its aaid, account number and member id"
        );
    }

    #[test]
    fn serialized_profile_keeps_aaid_and_account_channel() {
        let profile = profile_from_token(&token_with_sub(
            r#"{"member_id":3162,"account_channel":"lb","target_aaid":"20012394","account_no":"H10012371"}"#,
        ))
        .expect("profile");
        let json: Value = serde_json::to_value(&profile).expect("profile must serialize to JSON");
        assert_eq!(json["aaid"], "20012394");
        assert_eq!(json["account_channel"], "lb");
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
        assert_eq!(profile.aaid, None);
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
    fn reads_account_type_from_statement_member_info() {
        let statement =
            serde_json::json!({ "MemberInfo": { "AccountNo": "H1", "AccountType": "M" } });
        assert_eq!(statement_account_type(&statement).as_deref(), Some("M"));
        let blank = serde_json::json!({ "MemberInfo": { "AccountType": "" } });
        assert_eq!(statement_account_type(&blank), None);
    }

    #[test]
    fn tail_handles_short_and_multibyte_input() {
        assert_eq!(tail("H10012371"), "2371");
        assert_eq!(tail("12"), "12");
        assert_eq!(tail("账户号码一二"), "号码一二");
    }
}
