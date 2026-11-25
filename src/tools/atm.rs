//! Money-movement read tools: bank cards and their eDDA authorizations,
//! deposits, withdrawals, and stock transfers in and out of the account.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::serialize::{convert_unix_paths, drop_keys, strip_empty_strings, strip_nulls};
use crate::tools::support::http_client::{http_get_value, value_result};
use crate::tools::support::parse;

/// Bank-card fields passed through. This is an allowlist: the upstream card
/// also carries the holder's name and postal address (`payee_*`), the storage
/// keys of uploaded ID documents (`picture_urls`) and app styling, none of
/// which a support answer may see.
const CARD_FIELDS: &[&str] = &[
    "id",
    "bank",
    "bank_en",
    "bank_code",
    "account",
    "account_type",
    "region",
    "region_name",
    "country",
    "country_display",
    "remark",
    "status",
    "witness_state",
    "support_witness",
    "bind_state",
    "bind_tip",
    "created_at",
    "auth_list",
];

/// Fields passed through from each entry of a card's `auth_list` (allowlist).
const AUTH_FIELDS: &[&str] = &[
    "flow_id",
    "name",
    "currency",
    "status",
    "message",
    "is_support",
    "max_amount",
    "max_amount_unlimited",
    "daily_max_count",
    "daily_max_count_unlimited",
    "expiry_date",
    "long_term_valid",
    "doc_reviewing",
    "no_prior_deposit_reviewing",
];

/// Auth fields upstream fills with `"0"` / `0` when unset.
const AUTH_ZERO_UNSET: &[&str] = &["max_amount", "daily_max_count", "expiry_date"];

/// Stock-transfer fields that expose the counterparty broker's contact people
/// or the user's e-mail, which a support answer never needs.
const TRANSFER_CONTACT_KEYS: &[&str] = &[
    "contact",
    "contact_phone",
    "contact_email",
    "user_email",
    "website",
    "email_direct",
    "is_has_attachment",
];

/// Stock-transfer timestamps that arrive as unix seconds without an `_at`
/// suffix the transform would recognize, or as strings.
const TRANSFER_TIME_PATHS: &[&str] = &[
    "records.*.process_at",
    "records.*.exec_at",
    "records.*.created_at",
    "records.*.updated_at",
    "records.*.cancel_at",
    "records.*.refuse_at",
    "records.*.agree_at",
];

/// eDDA flows in a card's `auth_list`: bank-side and in-app eDDA.
const EDDA_FLOWS: &[&str] = &["hk_edda", "hk_sdda"];

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WithdrawalParam {
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepositParam {
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
    /// Filter by deposit states (comma-separated)
    pub states: Option<String>,
    /// Filter by currencies (comma-separated, e.g. "USD,HKD")
    pub currencies: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EddaStatusParam {
    /// Bank card id, from `bank_cards` (`cards[].id`)
    pub card_id: String,
    /// Authorized currency, e.g. "HKD" or "USD"
    pub currency: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StockTransferParam {
    /// Market filter: "HK", "US", "CN" or "SG" (default: all)
    pub market: Option<String>,
    /// Status code filter (default: all); codes are listed in the tool description
    pub state: Option<i32>,
    /// Created on or after this date (yyyy-mm-dd or RFC3339)
    pub start: Option<String>,
    /// Created on or before this date (yyyy-mm-dd or RFC3339)
    pub end: Option<String>,
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20, max 50)
    pub size: Option<u32>,
}

/// Keep only the last four characters of an account number.
fn mask_account(account: &str) -> String {
    let chars: Vec<char> = account.chars().filter(|c| !c.is_whitespace()).collect();
    let tail: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("****{tail}")
}

/// Readable name for an integer code, or `None` for an unknown code.
fn label(value: Option<&Value>, names: &[(i64, &'static str)]) -> Option<&'static str> {
    let code = value?.as_i64()?;
    names
        .iter()
        .find_map(|(c, name)| (*c == code).then_some(*name))
}

/// Insert `key: label` next to an integer code when the code is known.
fn add_label(
    card: &mut Map<String, Value>,
    code_key: &str,
    key: &str,
    names: &[(i64, &'static str)],
) {
    if let Some(name) = label(card.get(code_key), names) {
        card.insert(key.to_string(), Value::String(name.to_string()));
    }
}

const CARD_STATUS: &[(i64, &str)] = &[(0, "unverified"), (1, "under_review"), (2, "verified")];
const WITNESS_STATE: &[(i64, &str)] = &[(1, "not_required"), (2, "pending"), (3, "witnessed")];
const BIND_STATE: &[(i64, &str)] = &[(0, "binding"), (1, "bound"), (2, "failed")];
const AUTH_STATUS: &[(i64, &str)] = &[
    (0, "unauthorized"),
    (1, "authorized"),
    (2, "authorizing"),
    (3, "failed"),
];

/// Keep only `keys` of `obj`, in place.
fn retain_fields(obj: &mut Map<String, Value>, keys: &[&str]) {
    obj.retain(|k, _| keys.contains(&k.as_str()));
}

/// Whether an upstream value is the `0` / `"0"` "not set" placeholder.
fn is_zero_placeholder(value: &Value) -> bool {
    value.as_i64() == Some(0) || value.as_str() == Some("0")
}

/// Reshape one bank card from `GetWithdrawCardV3`: keep the allowlisted
/// fields, mask the account number, and name the status codes.
fn shape_card(card: &mut Value) {
    let Some(obj) = card.as_object_mut() else {
        return;
    };
    retain_fields(obj, CARD_FIELDS);
    if let Some(account) = obj
        .remove("account")
        .and_then(|a| a.as_str().map(mask_account))
    {
        obj.insert("account".into(), Value::String(account));
    }
    add_label(obj, "status", "verification", CARD_STATUS);
    // Days a card has been waiting for manual review since it was added, for
    // the "still under review after about two weeks" check.
    if obj.get("status").and_then(Value::as_i64) == Some(1)
        && let Some(days) = obj
            .get("created_at")
            .and_then(Value::as_str)
            .and_then(|c| days_between(c, None))
    {
        obj.insert("review_days".into(), Value::from(days));
    }
    add_label(obj, "witness_state", "witness", WITNESS_STATE);
    add_label(obj, "bind_state", "binding", BIND_STATE);
    if let Some(auths) = obj.get_mut("auth_list").and_then(Value::as_array_mut) {
        for auth in auths.iter_mut().filter_map(Value::as_object_mut) {
            retain_fields(auth, AUTH_FIELDS);
            auth.retain(|k, v| !(AUTH_ZERO_UNSET.contains(&k.as_str()) && is_zero_placeholder(v)));
            // A long-term authorization carries the 9999-12-31 sentinel as its
            // expiry; drop it, and turn a real expiry into a date.
            if auth.get("long_term_valid").and_then(Value::as_bool) == Some(true) {
                auth.remove("expiry_date");
            }
            let mut auth_value = Value::Object(std::mem::take(auth));
            convert_unix_paths(&mut auth_value, &["expiry_date"]);
            if let Value::Object(converted) = auth_value {
                *auth = converted;
            }
            add_label(auth, "status", "authorization", AUTH_STATUS);
        }
    }
}

/// Fetch and reshape the bank-card list (`GET /v2/account/bank-cards`).
async fn fetch_cards(mctx: &crate::tools::McpContext) -> Result<Vec<Value>, McpError> {
    let client = mctx.create_http_client();
    let channel = mctx.account_channel();
    let mut value = http_get_value(
        &client,
        "/v2/account/bank-cards",
        &[
            ("from_app_page", "cards"),
            ("account_channel", channel.as_str()),
        ],
    )
    .await?;
    let mut cards = match value.get_mut("list").map(Value::take) {
        Some(Value::Array(cards)) => cards,
        _ => Vec::new(),
    };
    cards.iter_mut().for_each(shape_card);
    Ok(cards)
}

/// List linked bank cards with their verification, binding and authorization
/// status.
pub async fn bank_cards(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let mut value = json!({ "cards": fetch_cards(mctx).await? });
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result(&value)
}

/// One eDDA authorization row: the card it belongs to plus the auth entry.
fn edda_row(card: &Value, auth: &Value) -> Value {
    let card_field = |key: &str| card.get(key).cloned().unwrap_or(Value::Null);
    json!({
        "card_id": card_field("id"),
        "bank": card_field("bank"),
        "bank_en": card_field("bank_en"),
        "account": card_field("account"),
        "card_binding": card_field("binding"),
        "flow": auth.get("flow_id"),
        "name": auth.get("name"),
        "currency": auth.get("currency"),
        "status": auth.get("status"),
        "authorization": auth.get("authorization"),
        "failure_reason": auth.get("message"),
        "max_amount": auth.get("max_amount"),
        "max_amount_unlimited": auth.get("max_amount_unlimited"),
        "daily_max_count": auth.get("daily_max_count"),
        "daily_max_count_unlimited": auth.get("daily_max_count_unlimited"),
        "expiry_date": auth.get("expiry_date"),
        "long_term_valid": auth.get("long_term_valid"),
    })
}

/// List the eDDA authorizations across all linked bank cards.
pub async fn edda_authorizations(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let cards = fetch_cards(mctx).await?;
    let rows: Vec<Value> = cards
        .iter()
        .flat_map(|card| {
            card.get("auth_list")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|auth| {
                    auth.get("flow_id")
                        .and_then(Value::as_str)
                        .is_some_and(|flow| EDDA_FLOWS.contains(&flow))
                })
                .map(move |auth| edda_row(card, auth))
        })
        .collect();
    let mut value = json!({ "authorizations": rows });
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result(&value)
}

/// Current eDDA authorization status of one bank card in one currency.
pub async fn edda_status(
    mctx: &crate::tools::McpContext,
    p: EddaStatusParam,
) -> Result<CallToolResult, McpError> {
    if p.card_id.is_empty() || !p.card_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(McpError::invalid_params(
            format!(
                "invalid card_id '{}': expected the numeric id from bank_cards",
                p.card_id
            ),
            None,
        ));
    }
    let client = mctx.create_http_client();
    let channel = mctx.account_channel();
    let currency = p.currency.trim().to_uppercase();
    let mut value = http_get_value(
        &client,
        "/v1/account/edda-status",
        &[
            ("card_id", p.card_id.as_str()),
            ("currency", currency.as_str()),
            ("account_channel", channel.as_str()),
        ],
    )
    .await?;
    let mut value = shape_edda_status(value.take());
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result(&value)
}

/// Normalize a `FindAuthStatus` reply to `{applications[]}`. The RPC now
/// returns `{list[]}` of eDDA authorization applications; older builds
/// returned a single `{id, status, message}`.
fn shape_edda_status(value: Value) -> Value {
    let mut applications = match value {
        Value::Object(mut obj) => match obj.remove("list") {
            Some(Value::Array(list)) => list,
            Some(_) => Vec::new(),
            None if obj.contains_key("status") => vec![Value::Object(obj)],
            None => Vec::new(),
        },
        _ => Vec::new(),
    };
    for app in applications.iter_mut().filter_map(Value::as_object_mut) {
        add_label(app, "status", "authorization", AUTH_STATUS);
        if let Some(message) = app.remove("message") {
            app.insert("failure_reason".into(), message);
        }
    }
    json!({ "applications": applications })
}

/// Query parameters shared by the transfer-in and transfer-out record lists.
fn transfer_params(
    p: &StockTransferParam,
    channel: String,
) -> Result<Vec<(&'static str, String)>, McpError> {
    let mut params = vec![
        ("page", p.page.unwrap_or(1).max(1).to_string()),
        ("size", p.size.unwrap_or(20).clamp(1, 50).to_string()),
        ("account_channel", channel),
    ];
    if let Some(market) = &p.market {
        params.push(("market", market.trim().to_uppercase()));
    }
    if let Some(state) = p.state {
        params.push(("state", state.to_string()));
    }
    if let Some(start) = &p.start {
        params.push((
            "start",
            parse::parse_rfc3339(start)?.unix_timestamp().to_string(),
        ));
    }
    if let Some(end) = &p.end {
        params.push((
            "end",
            parse::parse_rfc3339_end(end)?.unix_timestamp().to_string(),
        ));
    }
    Ok(params)
}

/// Whole days between two RFC3339 timestamps (`to` defaults to now).
fn days_between(from: &str, to: Option<&str>) -> Option<i64> {
    let from = parse::parse_rfc3339(from).ok()?;
    let to = match to {
        Some(t) => parse::parse_rfc3339(t).ok()?,
        None => time::OffsetDateTime::now_utc(),
    };
    Some((to - from).whole_days().max(0))
}

/// Fetch a stock-transfer record list and reshape it: drop contact details,
/// convert timestamps, flag whether the other broker was notified, and add
/// the days the application has been (or was) in progress.
async fn transfer_records(
    mctx: &crate::tools::McpContext,
    path: &str,
    p: StockTransferParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let params = transfer_params(&p, mctx.account_channel())?;
    let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut value = http_get_value(&client, path, &params).await?;
    drop_keys(&mut value, TRANSFER_CONTACT_KEYS);
    convert_unix_paths(&mut value, TRANSFER_TIME_PATHS);
    if let Some(records) = value.get_mut("records").and_then(Value::as_array_mut) {
        for record in records.iter_mut().filter_map(Value::as_object_mut) {
            if let Some(state) = record.get("app_state").and_then(Value::as_str) {
                let noticed = state == "noticed";
                record.insert("broker_notified".into(), Value::Bool(noticed));
            }
            let created = record.get("created_at").and_then(Value::as_str);
            let finished = record.get("exec_at").and_then(Value::as_str);
            if let Some(days) = created.and_then(|c| days_between(c, finished)) {
                record.insert("days_in_process".into(), Value::from(days));
            }
        }
    }
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result(&value)
}

/// List stock transfer-in applications (another broker → this account).
pub async fn stock_transfers_in(
    mctx: &crate::tools::McpContext,
    p: StockTransferParam,
) -> Result<CallToolResult, McpError> {
    transfer_records(mctx, "/v1/account/stock-transfers/in", p).await
}

/// List stock transfer-out applications (this account → another broker).
pub async fn stock_transfers_out(
    mctx: &crate::tools::McpContext,
    p: StockTransferParam,
) -> Result<CallToolResult, McpError> {
    transfer_records(mctx, "/v1/account/stock-transfers/out", p).await
}

/// Mask `bank_account` and summarize `vouchers` in deposit / withdrawal
/// records: the full account number and the uploaded receipt images are not
/// for a support answer.
fn shape_money_records(value: &mut Value, list_key: &str) {
    let Some(items) = value.get_mut(list_key).and_then(Value::as_array_mut) else {
        return;
    };
    for item in items.iter_mut().filter_map(Value::as_object_mut) {
        if let Some(account) = item
            .remove("bank_account")
            .and_then(|a| a.as_str().filter(|a| !a.is_empty()).map(mask_account))
        {
            item.insert("bank_account".into(), Value::String(account));
        }
        if let Some(vouchers) = item.remove("vouchers") {
            let count = vouchers.as_array().map_or(0, Vec::len);
            item.insert("voucher_count".into(), Value::from(count));
        }
    }
}

/// List withdrawal history for the current account.
pub async fn withdrawals(
    mctx: &crate::tools::McpContext,
    p: WithdrawalParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let channel = mctx.account_channel();
    let mut value = http_get_value(
        &client,
        "/v1/account/withdrawals",
        &[
            ("page", page_str.as_str()),
            ("size", size_str.as_str()),
            ("account_channel", channel.as_str()),
        ],
    )
    .await?;
    shape_money_records(&mut value, "list");
    value_result(&value)
}

/// List deposit history for the current account.
pub async fn deposits(
    mctx: &crate::tools::McpContext,
    p: DepositParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let channel = mctx.account_channel();
    let mut params: Vec<(&str, &str)> = vec![
        ("page", page_str.as_str()),
        ("size", size_str.as_str()),
        ("account_channel", channel.as_str()),
    ];
    if let Some(ref s) = p.states {
        params.push(("states", s.as_str()));
    }
    if let Some(ref c) = p.currencies {
        params.push(("currencies", c.as_str()));
    }
    let mut value = http_get_value(&client, "/v1/account/deposits", &params).await?;
    shape_money_records(&mut value, "items");
    value_result(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_account_keeps_last_four() {
        assert_eq!(
            mask_account("012 345 678901"),
            "****8901",
            "spaces must be ignored"
        );
        assert_eq!(mask_account("12"), "****12", "short numbers stay whole");
    }

    #[test]
    fn shape_card_masks_labels_and_drops_personal_data() {
        // Shape of a real canary response (GetWithdrawCardV3), 2026-11-25.
        let mut card = json!({
            "id": "434810", "bank_id": "79", "account_type": "ALL",
            "account": "266382841694216", "region": "HONGKONG", "bank": "389 MOX Bank",
            "region_name": "香港地区银行", "witness_state": 1, "status": 1, "bind_state": 1,
            "bind_tip": "pending review", "country": "Hong Kong",
            "picture_urls": "[\"id/abc\"]", "payee_street": "Nathan Road",
            "payee_postcode": "000000", "name": "CHAN TAI MAN",
            "auth_list": [{
                "flow_id": "hk_edda", "name": "eDDA", "status": 3, "message": "bank rejected",
                "custom": "789", "id": "0", "created_at": "0", "need_sms": 0,
                "daily_max_count": "0", "max_amount_unlimited": false,
            }],
        });
        shape_card(&mut card);
        assert_eq!(card["account"], "****4216", "account must be masked");
        assert_eq!(card["country"], "Hong Kong", "country passes through");
        assert_eq!(card["verification"], "under_review", "status label");
        assert_eq!(card["witness"], "not_required", "witness label");
        assert_eq!(card["binding"], "bound", "bind label");
        for key in [
            "picture_urls",
            "payee_street",
            "payee_postcode",
            "name",
            "bank_id",
        ] {
            assert!(card.get(key).is_none(), "card field {key} must be dropped");
        }
        let auth = &card["auth_list"][0];
        assert_eq!(auth["authorization"], "failed", "auth status label");
        assert_eq!(auth["name"], "eDDA", "auth name is kept");
        for key in ["custom", "id", "created_at", "need_sms", "daily_max_count"] {
            assert!(auth.get(key).is_none(), "auth field {key} must be dropped");
        }
        assert_eq!(auth["max_amount_unlimited"], false, "limit flags are kept");
    }

    #[test]
    fn edda_row_takes_card_context() {
        let mut card = json!({
            "id": 7, "bank": "HSBC", "account": "123456789", "bind_state": 1,
            "auth_list": [{"flow_id": "hk_edda", "currency": "HKD", "status": 1}],
        });
        shape_card(&mut card);
        let row = edda_row(&card, &card["auth_list"][0]);
        assert_eq!(row["card_id"], 7, "card id carried over");
        assert_eq!(row["account"], "****6789", "masked account carried over");
        assert_eq!(
            row["authorization"], "authorized",
            "auth label carried over"
        );
    }

    #[test]
    fn shape_edda_status_accepts_list_and_object() {
        let listed = shape_edda_status(json!({"list": [{"id": 1, "status": 3, "message": "x"}]}));
        assert_eq!(
            listed["applications"][0]["authorization"], "failed",
            "list shape labelled"
        );
        assert_eq!(
            listed["applications"][0]["failure_reason"], "x",
            "message renamed"
        );
        let single = shape_edda_status(json!({"id": 2, "status": 1}));
        assert_eq!(
            single["applications"][0]["authorization"], "authorized",
            "object shape wrapped"
        );
        assert_eq!(
            shape_edda_status(json!({"list": []}))["applications"],
            json!([]),
            "empty stays empty"
        );
    }

    #[test]
    fn shape_money_records_masks_and_counts() {
        let mut value = json!({"list": [{"id": 1, "bank_account": "012345678901", "fee": "5",
            "vouchers": [{"key": "k", "img_url": "https://x/y.png"}]}]});
        shape_money_records(&mut value, "list");
        let item = &value["list"][0];
        assert_eq!(item["bank_account"], "****8901", "account masked");
        assert_eq!(item["voucher_count"], 1, "vouchers summarized");
        assert!(item.get("vouchers").is_none(), "voucher urls dropped");
        assert_eq!(item["fee"], "5", "other fields untouched");
    }

    #[test]
    fn shape_card_counts_review_days() {
        let mut card = json!({"id": 1, "status": 1, "created_at": "2020-01-01T00:00:00Z"});
        shape_card(&mut card);
        assert!(
            card["review_days"].as_i64().is_some_and(|d| d > 14),
            "review days computed"
        );
        let mut verified = json!({"id": 2, "status": 2, "created_at": "2020-01-01T00:00:00Z"});
        shape_card(&mut verified);
        assert!(
            verified.get("review_days").is_none(),
            "only cards under review"
        );
    }

    #[test]
    fn days_between_counts_whole_days() {
        assert_eq!(
            days_between("2026-11-01T00:00:00Z", Some("2026-11-04T12:00:00Z")),
            Some(3),
            "partial days are dropped"
        );
        assert_eq!(
            days_between("not a date", None),
            None,
            "bad input is skipped"
        );
    }
}
