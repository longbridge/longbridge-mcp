//! Money-movement read tools: bank cards and their eDDA authorizations,
//! deposits, withdrawals, and stock transfers in and out of the account.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::serialize::{convert_unix_paths, drop_keys, strip_empty_strings, strip_nulls};
use crate::tools::support::http_client::{http_get_value, value_result_hong_kong};
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

/// Display-only card fields dropped after shaping.
const CARD_DISPLAY_FIELDS: &[&str] = &["support_witness", "country_display"];

/// Fields kept from an eDDA application (`FindAuthStatus`), after labelling.
const EDDA_APPLICATION_FIELDS: &[&str] = &[
    "id",
    "flow_id",
    "name",
    "currency",
    "authorization",
    "status",
    "failure_reason",
    "created_at",
    "updated_at",
];

/// Fields kept from a stock-transfer record, after shaping (allowlist).
const TRANSFER_FIELDS: &[&str] = &[
    "id",
    "order_id",
    "market",
    "broker",
    "broker_en",
    "code",
    "account",
    "state",
    "state_code",
    "state_label",
    "can_modify",
    "app_state",
    "user_notified_broker",
    "stocks",
    "created_at",
    "updated_at",
    "process_at",
    "exec_at",
    "agree_at",
    "refuse_at",
    "cancel_at",
    "days_in_process",
    "remark",
    "reason",
];

/// Fields kept for each stock in a transfer record (allowlist).
const TRANSFER_STOCK_FIELDS: &[&str] = &[
    "code",
    "name",
    "name_en",
    "market",
    "counter_id",
    "amount",
    "currency",
    "state",
    "state_label",
];

/// Fields kept from a deposit record, after shaping (allowlist).
const DEPOSIT_FIELDS: &[&str] = &[
    "id",
    "created_at",
    "updated_at",
    "currency",
    "amount",
    "receivable_amount",
    "state",
    "state_code",
    "state_label",
    "sub_state_code",
    "bank_operation_type_name",
    "reference_no",
    "can_cancel",
    "voucher_count",
];

/// Fields kept from a withdrawal record, after shaping (allowlist).
const WITHDRAWAL_FIELDS: &[&str] = &[
    "id",
    "order_id",
    "created_at",
    "currency",
    "amount",
    "fee",
    "settlement_amount",
    "state",
    "state_label",
    "state_description",
    "bank_account",
    "display_bank_account",
    "user_bank_swift_code",
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

const TRANSFER_IN_STATE: &[(i64, &str)] = &[
    (0, "submitted"),
    (1, "cancelled"),
    (3, "completed"),
    (5, "processing"),
    (7, "partially_completed"),
    (8, "failed"),
];
const TRANSFER_OUT_STATE: &[(i64, &str)] = &[
    (0, "submitted"),
    (1, "processing"),
    (2, "executing"),
    (3, "completed"),
    (4, "cancelled"),
    (5, "partially_completed"),
    // The app shows 6 as 失败 (failed); the proto comment says partial
    // failure. Follow what the customer sees.
    (6, "failed"),
    (7, "rejected"),
];
const DEPOSIT_STATE: &[(i64, &str)] = &[
    (0, "processing"),
    (1, "credited"),
    (2, "failed"),
    (3, "cancelled"),
    (4, "invalidated"),
    (5, "rejected"),
];
/// Withdrawal `state` codes, as the app's withdrawal records read them.
const WITHDRAWAL_STATE: &[(i64, &str)] = &[
    (0, "submitted"),
    (1, "processing"),
    (2, "rejected"),
    (3, "cancelled"),
    (4, "remitted"),
    (5, "failed"),
    (6, "refunded"),
    (7, "approved_awaiting_payout"),
    (8, "large_amount_review"),
    (9, "change_pending_approval"),
    (10, "deletion_pending_approval"),
    (11, "deleted"),
    (12, "rejection_pending_approval"),
    (15, "awaiting_confirmation"),
];

/// How a stock-transfer list labels its codes and which codes are final.
struct TransferKind {
    path: &'static str,
    states: &'static [(i64, &'static str)],
    /// Cancelled / rejected / failed: no execution date, so the elapsed days
    /// stop at the last update instead of running on to today.
    terminal: &'static [i64],
    /// Completed or partially completed: finished, so the count stops at the
    /// execution date, or at the last update when that date is unset.
    completed: &'static [i64],
    /// Whether `can_modify` means "can still be cancelled or changed". On
    /// transfer-out records it is 0 even for a cancellable submitted one, so
    /// it is dropped there and `state_code` 0 decides instead.
    keeps_can_modify: bool,
}

const TRANSFER_IN: TransferKind = TransferKind {
    path: "/v1/account/stock-transfers/in",
    states: TRANSFER_IN_STATE,
    terminal: &[1, 8],
    completed: &[3, 7],
    keeps_can_modify: true,
};
const TRANSFER_OUT: TransferKind = TransferKind {
    path: "/v1/account/stock-transfers/out",
    states: TRANSFER_OUT_STATE,
    terminal: &[4, 6, 7],
    completed: &[3, 5],
    keeps_can_modify: false,
};

/// Stock-transfer record fields with no use in a support answer: internal
/// position bookkeeping and an empty cash list.
const TRANSFER_RECORD_NOISE: &[&str] = &["cash", "market_symbol", "broker_id"];

/// Per-stock fields of a transfer: internal bookkeeping, plus the storage
/// keys of uploaded proof documents (`usi_attachments`).
const TRANSFER_STOCK_NOISE: &[&str] = &[
    "usi_attachments",
    "is_reality",
    "street_amount",
    "nominee_amount",
    "own_amount",
    "tax_enable",
    "effect_state",
    "certificate_count",
];

/// Whether a converted timestamp is an unset placeholder: a date before
/// [`TRANSFER_MIN_YEAR`], or a raw `0` / `"0"` the date conversion left alone.
fn is_placeholder_time(value: &Value) -> bool {
    match value {
        Value::String(t) => t == "0" || t.get(..4).is_some_and(|year| year < TRANSFER_MIN_YEAR),
        Value::Number(n) => n.as_i64().is_some_and(|n| n <= 0),
        _ => false,
    }
}

/// Per-stock transfer `state`, as the app's record detail reads it.
const TRANSFER_STOCK_STATE: &[(i64, &str)] = &[(0, "processing"), (1, "succeeded"), (2, "failed")];

/// Transfer timestamps upstream fills with a placeholder when unset (0, which
/// becomes 1970, or a negative value): anything before this year is dropped.
const TRANSFER_MIN_YEAR: &str = "2000";

/// eDDA flows in a card's `auth_list`: bank-side and in-app eDDA.
const EDDA_FLOWS: &[&str] = &["hk_edda", "hk_sdda"];

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WithdrawalParam {
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
    /// Applied on or after this date (yyyy-mm-dd or RFC3339)
    pub start: Option<String>,
    /// Applied on or before this date (yyyy-mm-dd or RFC3339)
    pub end: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepositParam {
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20, max 20)
    pub size: Option<u32>,
    /// Filter by state codes, comma-separated (e.g. "0,2"): 0 processing, 1 credited, 2 failed, 3 cancelled, 4 invalidated, 5 rejected
    pub states: Option<String>,
    /// Filter by currencies (comma-separated, e.g. "USD,HKD")
    pub currencies: Option<String>,
    /// Applied on or after this date (yyyy-mm-dd or RFC3339)
    pub start: Option<String>,
    /// Applied on or before this date (yyyy-mm-dd or RFC3339)
    pub end: Option<String>,
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
    obj: &mut Map<String, Value>,
    code_key: &str,
    key: &str,
    names: &[(i64, &'static str)],
) {
    if let Some(name) = label(obj.get(code_key), names) {
        obj.insert(key.to_string(), Value::String(name.to_string()));
    }
}

/// Replace the integer code at `code_key` with `key: label` when the code is
/// known; otherwise keep the raw code under `code_key`, so an undocumented
/// code is still shown (as the CLI does).
fn label_code(
    obj: &mut Map<String, Value>,
    code_key: &str,
    key: &str,
    names: &[(i64, &'static str)],
) {
    if let Some(name) = label(obj.get(code_key), names) {
        obj.remove(code_key);
        obj.insert(key.to_string(), Value::String(name.to_string()));
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
    // Labelled codes are dropped: the raw numbers are ambiguous next to an
    // auth entry's own `status` (1 = under review here, authorized there).
    label_code(obj, "status", "verification", CARD_STATUS);
    label_code(obj, "witness_state", "witness", WITNESS_STATE);
    label_code(obj, "bind_state", "binding", BIND_STATE);
    for key in CARD_DISPLAY_FIELDS {
        obj.remove(*key);
    }
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
            label_code(auth, "status", "authorization", AUTH_STATUS);
            if let Some(message) = auth.remove("message") {
                auth.insert("failure_reason".into(), message);
            }
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
    value_result_hong_kong(value)
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
        "bind_state": card_field("bind_state"),
        "flow": auth.get("flow_id"),
        "name": auth.get("name"),
        "currency": auth.get("currency"),
        "authorization": auth.get("authorization"),
        "status": auth.get("status"),
        "failure_reason": auth.get("failure_reason"),
        "max_amount": auth.get("max_amount"),
        "max_amount_unlimited": auth.get("max_amount_unlimited"),
        "daily_max_count": auth.get("daily_max_count"),
        "daily_max_count_unlimited": auth.get("daily_max_count_unlimited"),
        "expiry_date": auth.get("expiry_date"),
        "long_term_valid": auth.get("long_term_valid"),
        "is_support": auth.get("is_support"),
        "doc_reviewing": auth.get("doc_reviewing"),
        "no_prior_deposit_reviewing": auth.get("no_prior_deposit_reviewing"),
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
    value_result_hong_kong(value)
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
    value_result_hong_kong(value)
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
        label_code(app, "status", "authorization", AUTH_STATUS);
        if let Some(message) = app.remove("message") {
            app.insert("failure_reason".into(), message);
        }
        retain_fields(app, EDDA_APPLICATION_FIELDS);
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
            parse::parse_rfc3339_hong_kong(start)?
                .unix_timestamp()
                .to_string(),
        ));
    }
    if let Some(end) = &p.end {
        params.push((
            "end",
            parse::parse_rfc3339_end_hong_kong(end)?
                .unix_timestamp()
                .to_string(),
        ));
    }
    Ok(params)
}

/// Unix-second bounds for an optional `start` / `end` date filter.
fn date_range(
    start: Option<&str>,
    end: Option<&str>,
) -> Result<(Option<String>, Option<String>), McpError> {
    let start = start
        .map(|s| parse::parse_rfc3339_hong_kong(s).map(|t| t.unix_timestamp().to_string()))
        .transpose()?;
    let end = end
        .map(|e| parse::parse_rfc3339_end_hong_kong(e).map(|t| t.unix_timestamp().to_string()))
        .transpose()?;
    Ok((start, end))
}

/// Whole days between two RFC3339 timestamps (`to` defaults to now).
fn days_between(from: &str, to: Option<&str>) -> Option<i64> {
    let from = parse::parse_rfc3339_hong_kong(from).ok()?;
    let to = match to {
        Some(t) => parse::parse_rfc3339_hong_kong(t).ok()?,
        None => time::OffsetDateTime::now_utc(),
    };
    Some((to - from).whole_days().max(0))
}

/// Fetch a stock-transfer record list and reshape it: drop contact details,
/// convert timestamps, flag whether the other broker was notified, and add
/// the days the application has been (or was) in progress.
async fn transfer_records(
    mctx: &crate::tools::McpContext,
    kind: &TransferKind,
    p: StockTransferParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let params = transfer_params(&p, mctx.account_channel())?;
    let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut value = http_get_value(&client, kind.path, &params).await?;
    drop_keys(&mut value, TRANSFER_CONTACT_KEYS);
    convert_unix_paths(&mut value, TRANSFER_TIME_PATHS);
    if let Some(records) = value.get_mut("records").and_then(Value::as_array_mut) {
        records
            .iter_mut()
            .filter_map(Value::as_object_mut)
            .for_each(|record| shape_transfer(record, kind));
    }
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result_hong_kong(value)
}

/// Shape one stock-transfer record: drop the proof-document storage keys and
/// the account holder's name at the other broker (record level only, since
/// `stocks[].name` is the stock name), then add `state_label`,
/// `user_notified_broker` and `days_in_process`.
fn shape_transfer(record: &mut Map<String, Value>, kind: &TransferKind) {
    record.remove("attachments");
    record.remove("name");
    for key in TRANSFER_RECORD_NOISE {
        record.remove(*key);
    }
    // The customer's account number at the other broker: last 4 digits only.
    if let Some(account) = record
        .remove("account")
        .and_then(|a| a.as_str().filter(|a| !a.is_empty()).map(mask_account))
    {
        record.insert("account".into(), Value::String(account));
    }
    // Unset dates arrive as 1970 (0) or earlier (negative): drop them, or an
    // open application would read as executed in 1970.
    record.retain(|key, value| !(key.ends_with("_at") && is_placeholder_time(value)));
    // Transfer-out records name the broker's English name `name_en`; use the
    // transfer-in name so both lists read the same.
    if !record.contains_key("broker_en")
        && let Some(name) = record.remove("name_en")
    {
        record.insert("broker_en".into(), name);
    }
    if let Some(stocks) = record.get_mut("stocks").and_then(Value::as_array_mut) {
        for stock in stocks.iter_mut().filter_map(Value::as_object_mut) {
            for key in TRANSFER_STOCK_NOISE {
                stock.remove(*key);
            }
            add_label(stock, "state", "state_label", TRANSFER_STOCK_STATE);
        }
    }
    add_label(record, "state_code", "state_label", kind.states);
    // `noticed`: the user tapped "I have notified the broker" in the app.
    // It is the user's own confirmation, not a Longbridge action.
    if let Some(state) = record.get("app_state").and_then(Value::as_str) {
        let noticed = state == "noticed";
        record.insert("user_notified_broker".into(), Value::Bool(noticed));
    }
    let text = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    let code = record.get("state_code").and_then(Value::as_i64);
    let finished =
        code.is_some_and(|code| kind.terminal.contains(&code) || kind.completed.contains(&code));
    // Executed: until execution. Finished without an execution date: until it
    // ended. Still open: until now.
    let until = text("exec_at").or_else(|| {
        if finished {
            ["cancel_at", "refuse_at", "updated_at"]
                .iter()
                .find_map(|key| text(key))
        } else {
            None
        }
    });
    if finished && until.is_none() {
        finish_transfer(record, kind);
        return;
    }
    if let Some(days) = text("created_at").and_then(|c| days_between(&c, until.as_deref())) {
        record.insert("days_in_process".into(), Value::from(days));
    }
    finish_transfer(record, kind);
}

/// Keep only the allowlisted record and per-stock fields.
fn finish_transfer(record: &mut Map<String, Value>, kind: &TransferKind) {
    if !kind.keeps_can_modify {
        record.remove("can_modify");
    }
    retain_fields(record, TRANSFER_FIELDS);
    if let Some(stocks) = record.get_mut("stocks").and_then(Value::as_array_mut) {
        for stock in stocks.iter_mut().filter_map(Value::as_object_mut) {
            retain_fields(stock, TRANSFER_STOCK_FIELDS);
        }
    }
}

/// List stock transfer-in applications (another broker → this account).
pub async fn stock_transfers_in(
    mctx: &crate::tools::McpContext,
    p: StockTransferParam,
) -> Result<CallToolResult, McpError> {
    transfer_records(mctx, &TRANSFER_IN, p).await
}

/// List stock transfer-out applications (this account → another broker).
pub async fn stock_transfers_out(
    mctx: &crate::tools::McpContext,
    p: StockTransferParam,
) -> Result<CallToolResult, McpError> {
    transfer_records(mctx, &TRANSFER_OUT, p).await
}

/// Mask `bank_account` and summarize `vouchers` in deposit / withdrawal
/// records: the full account number and the uploaded receipt images are not
/// for a support answer.
fn shape_money_records(
    value: &mut Value,
    list_key: &str,
    state_key: &str,
    states: &[(i64, &'static str)],
    page_size: usize,
    fields: &[&str],
) {
    // A full page means there may be more; this is the reliable signal, since
    // the deposit list's `total` follows the page size, not the record count.
    let full_page = value
        .get(list_key)
        .and_then(Value::as_array)
        .is_some_and(|items| items.len() >= page_size);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("has_more".into(), Value::Bool(full_page));
    }
    let Some(items) = value.get_mut(list_key).and_then(Value::as_array_mut) else {
        return;
    };
    for item in items.iter_mut().filter_map(Value::as_object_mut) {
        add_label(item, state_key, "state_label", states);
        if let Some(account) = item
            .remove("bank_account")
            .and_then(|a| a.as_str().filter(|a| !a.is_empty()).map(mask_account))
        {
            item.insert("bank_account".into(), Value::String(account));
        }
        // Usually a short tail like "尾号 0578"; mask it if it carries a longer
        // run of digits.
        if let Some(display) = item.get("display_bank_account").and_then(Value::as_str)
            && display.chars().filter(char::is_ascii_digit).count() > 4
        {
            let masked = mask_account(display);
            item.insert("display_bank_account".into(), Value::String(masked));
        }
        if let Some(vouchers) = item.remove("vouchers") {
            let count = vouchers.as_array().map_or(0, Vec::len);
            item.insert("voucher_count".into(), Value::from(count));
        }
        retain_fields(item, fields);
    }
}

/// List withdrawal history for the current account.
pub async fn withdrawals(
    mctx: &crate::tools::McpContext,
    p: WithdrawalParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).clamp(1, 50).to_string();
    let channel = mctx.account_channel();
    let (begin, end) = date_range(p.start.as_deref(), p.end.as_deref())?;
    let mut params: Vec<(&str, &str)> = vec![
        ("page", page_str.as_str()),
        ("size", size_str.as_str()),
        ("account_channel", channel.as_str()),
    ];
    if let Some(ref b) = begin {
        params.push(("begin", b.as_str()));
    }
    if let Some(ref e) = end {
        params.push(("end", e.as_str()));
    }
    let mut value = http_get_value(&client, "/v1/account/withdrawals", &params).await?;
    let page_size = p.size.unwrap_or(20).clamp(1, 50) as usize;
    shape_money_records(
        &mut value,
        "list",
        "state",
        WITHDRAWAL_STATE,
        page_size,
        WITHDRAWAL_FIELDS,
    );
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result_hong_kong(value)
}

/// Largest deposit page size upstream honours. Above it the deposit list
/// silently falls back to 10 rows, which would read as a short last page and
/// hide the remaining records.
const DEPOSIT_MAX_PAGE_SIZE: usize = 20;

/// The deposit page size sent upstream: default 20, clamped to
/// [`DEPOSIT_MAX_PAGE_SIZE`].
fn deposit_page_size(size: Option<u32>) -> usize {
    (size.unwrap_or(20) as usize).clamp(1, DEPOSIT_MAX_PAGE_SIZE)
}

/// List deposit history for the current account.
pub async fn deposits(
    mctx: &crate::tools::McpContext,
    p: DepositParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let page_size = deposit_page_size(p.size);
    let size_str = page_size.to_string();
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
    let (from_date, to_date) = date_range(p.start.as_deref(), p.end.as_deref())?;
    if let Some(ref f) = from_date {
        params.push(("from_date", f.as_str()));
    }
    if let Some(ref t) = to_date {
        params.push(("to_date", t.as_str()));
    }
    let mut value = http_get_value(&client, "/v1/account/deposits", &params).await?;
    shape_money_records(
        &mut value,
        "items",
        "state_code",
        DEPOSIT_STATE,
        page_size,
        DEPOSIT_FIELDS,
    );
    // Upstream `total` is capped at the page size, so it reads as a wrong
    // record count; `has_more` replaces it.
    if let Some(obj) = value.as_object_mut() {
        obj.remove("total");
    }
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result_hong_kong(value)
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
        assert_eq!(auth["failure_reason"], "bank rejected", "message renamed");
        assert_eq!(auth["name"], "eDDA", "auth name is kept");
        for key in ["custom", "id", "created_at", "need_sms", "daily_max_count"] {
            assert!(auth.get(key).is_none(), "auth field {key} must be dropped");
        }
        assert_eq!(auth["max_amount_unlimited"], false, "limit flags are kept");
    }

    #[test]
    fn shape_card_keeps_unlabelled_codes() {
        let mut card = json!({
            "id": "1", "account": "123456789", "status": 9, "witness_state": 2,
            "bind_state": 7, "support_witness": true,
            "auth_list": [
                {"flow_id": "hk_edda", "currency": "HKD", "status": 8},
                {"flow_id": "hk_edda", "currency": "USD", "status": 1},
            ],
        });
        shape_card(&mut card);
        assert_eq!(card["status"], 9, "unknown card status keeps its code");
        assert!(card.get("verification").is_none(), "no label for code 9");
        assert_eq!(card["witness"], "pending", "known witness state labelled");
        assert!(
            card.get("witness_state").is_none(),
            "a labelled code is dropped"
        );
        assert_eq!(card["bind_state"], 7, "unknown bind state keeps its code");
        assert!(card.get("binding").is_none(), "no label for bind state 7");
        assert!(
            card.get("support_witness").is_none(),
            "display-only fields are dropped"
        );
        let unknown = &card["auth_list"][0];
        assert_eq!(unknown["status"], 8, "unknown auth status keeps its code");
        assert!(unknown.get("authorization").is_none(), "no label for 8");
        let known = &card["auth_list"][1];
        assert_eq!(known["authorization"], "authorized", "known auth labelled");
        assert!(known.get("status").is_none(), "a labelled code is dropped");

        let row = edda_row(&card, unknown);
        assert_eq!(row["status"], 8, "edda row carries the unlabelled code");
        assert!(
            row["authorization"].is_null(),
            "edda row has no label for it"
        );
        assert_eq!(row["bind_state"], 7, "edda row carries the raw bind state");
        assert!(
            row["card_binding"].is_null(),
            "edda row has no binding label for 7"
        );
        let row = edda_row(&card, known);
        assert!(row["status"].is_null(), "edda row omits a labelled code");
    }

    #[test]
    fn shape_edda_status_keeps_unlabelled_codes() {
        let value = shape_edda_status(json!({"list": [
            {"id": 1, "status": 6},
            {"id": 2, "status": 2},
        ]}));
        let apps = &value["applications"];
        assert_eq!(apps[0]["status"], 6, "unknown status keeps its code");
        assert!(apps[0].get("authorization").is_none(), "no label for 6");
        assert_eq!(apps[1]["authorization"], "authorizing", "known labelled");
        assert!(
            apps[1].get("status").is_none(),
            "a labelled code is dropped"
        );
    }

    #[test]
    fn edda_row_takes_card_context() {
        let mut card = json!({
            "id": 7, "bank": "HSBC", "account": "123456789", "bind_state": 1,
            "auth_list": [{"flow_id": "hk_edda", "currency": "HKD", "status": 1,
                "doc_reviewing": true}],
        });
        shape_card(&mut card);
        let row = edda_row(&card, &card["auth_list"][0]);
        assert_eq!(row["card_id"], 7, "card id carried over");
        assert_eq!(row["card_binding"], "bound", "binding label carried over");
        assert!(
            row["bind_state"].is_null(),
            "a labelled bind state is dropped"
        );
        assert_eq!(row["account"], "****6789", "masked account carried over");
        assert_eq!(
            row["authorization"], "authorized",
            "auth label carried over"
        );
        assert!(
            row.get("doc_reviewing").is_some(),
            "review flags carried over"
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
    fn shape_money_records_masks_and_allowlists_withdrawals() {
        let mut value = json!({"list": [{"id": 1, "state": 4, "bank_account": "012345678901",
            "fee": "5", "raw_state": 4, "payee_name": "CHAN TAI MAN"}]});
        shape_money_records(
            &mut value,
            "list",
            "state",
            WITHDRAWAL_STATE,
            20,
            WITHDRAWAL_FIELDS,
        );
        let item = &value["list"][0];
        assert_eq!(item["bank_account"], "****8901", "account masked");
        assert_eq!(item["fee"], "5", "allowlisted field kept");
        assert_eq!(item["state_label"], "remitted", "state labelled");
        assert!(item.get("payee_name").is_none(), "unknown field dropped");
        assert!(item.get("raw_state").is_none(), "raw_state dropped");
        assert_eq!(value["has_more"], false, "short page: no more");
    }

    #[test]
    fn shape_money_records_summarizes_deposit_vouchers() {
        let mut value = json!({"items": [{"id": 1, "state_code": 1,
            "vouchers": [{"key": "k", "img_url": "https://x/y.png"}]}]});
        shape_money_records(
            &mut value,
            "items",
            "state_code",
            DEPOSIT_STATE,
            20,
            DEPOSIT_FIELDS,
        );
        let item = &value["items"][0];
        assert_eq!(item["voucher_count"], 1, "vouchers summarized");
        assert!(item.get("vouchers").is_none(), "voucher urls dropped");
        assert_eq!(item["state_label"], "credited", "state labelled");
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
    fn shape_transfer_cleans_a_real_record() {
        // Shape of a real canary transfer-in record, 2026-11-25.
        let mut record = json!({
            "id": "7053", "market_symbol": "US", "broker_id": "9", "account": "11222333444555",
            "created_at": "2026-11-25T09:59:06Z", "exec_at": "1970-01-01T00:00:00Z",
            "cancel_at": "1906-06-07T18:35:44Z", "app_state": "default", "state_code": 0,
            "cash": [], "stocks": [{"code": "AAPL", "name": "苹果", "amount": "100", "state": 0,
                "usi_attachments": ["k"], "is_reality": 0, "street_amount": "0"}],
        });
        shape_transfer(record.as_object_mut().expect("object"), &TRANSFER_IN);
        assert_eq!(record["account"], "****4555", "other-broker account masked");
        assert!(record.get("exec_at").is_none(), "1970 placeholder dropped");
        assert!(
            record.get("cancel_at").is_none(),
            "pre-1970 placeholder dropped"
        );
        assert!(
            record["days_in_process"].as_i64().is_some_and(|d| d >= 0),
            "open: counted"
        );
        assert_eq!(
            record["user_notified_broker"], false,
            "default: not notified"
        );
        for key in ["cash", "market_symbol", "broker_id"] {
            assert!(record.get(key).is_none(), "{key} dropped");
        }
        let stock = &record["stocks"][0];
        assert!(stock.get("usi_attachments").is_none(), "proof keys dropped");
        assert!(stock.get("street_amount").is_none(), "bookkeeping dropped");
        assert_eq!(stock["name"], "苹果", "stock name kept");
        assert_eq!(
            stock["state_label"], "processing",
            "per-stock state labelled"
        );
    }

    #[test]
    fn shape_transfer_out_names_the_broker_like_transfer_in() {
        let mut record = json!({"name_en": "Futu Securities", "state_code": 0});
        shape_transfer(record.as_object_mut().expect("object"), &TRANSFER_OUT);
        assert_eq!(record["broker_en"], "Futu Securities", "renamed");
        assert!(record.get("name_en").is_none(), "old name gone");
    }

    #[test]
    fn shape_transfer_completed_without_exec_date_stops_at_update() {
        let mut done = json!({"state_code": 3, "created_at": "2024-01-01T00:00:00Z",
            "exec_at": "0", "updated_at": "2024-01-06T00:00:00Z"});
        shape_transfer(done.as_object_mut().expect("object"), &TRANSFER_IN);
        assert!(
            done.get("exec_at").is_none(),
            "raw \"0\" placeholder dropped"
        );
        assert_eq!(
            done["days_in_process"], 5,
            "completed: until the last update"
        );
    }

    #[test]
    fn shape_money_records_masks_long_display_account() {
        let mut value = json!({"list": [{"display_bank_account": "012345678901"},
            {"display_bank_account": "尾号 0578"}]});
        shape_money_records(
            &mut value,
            "list",
            "state",
            WITHDRAWAL_STATE,
            20,
            WITHDRAWAL_FIELDS,
        );
        assert_eq!(
            value["list"][0]["display_bank_account"], "****8901",
            "long run masked"
        );
        assert_eq!(
            value["list"][1]["display_bank_account"], "尾号 0578",
            "tail kept"
        );
    }

    #[test]
    fn shape_transfer_stops_the_clock_at_the_end() {
        let mut cancelled = json!({"state_code": 1, "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-11T00:00:00Z"});
        shape_transfer(cancelled.as_object_mut().expect("object"), &TRANSFER_IN);
        assert_eq!(
            cancelled["days_in_process"], 10,
            "cancelled: until the last update"
        );
        assert_eq!(cancelled["state_label"], "cancelled", "state labelled");

        let mut done = json!({"state_code": 3, "created_at": "2024-01-01T00:00:00Z",
            "exec_at": "2024-01-04T00:00:00Z", "app_state": "noticed",
            "name": "CHAN TAI MAN", "attachments": "[\"ai/x\"]",
            "stocks": [{"code": "700", "name": "Tencent"}]});
        shape_transfer(done.as_object_mut().expect("object"), &TRANSFER_IN);
        assert_eq!(done["days_in_process"], 3, "executed: until execution");
        assert_eq!(done["user_notified_broker"], true, "notified flag");
        assert!(done.get("name").is_none(), "holder name dropped");
        assert!(done.get("attachments").is_none(), "proof keys dropped");
        assert_eq!(done["stocks"][0]["name"], "Tencent", "stock name kept");

        let mut rejected = json!({"state_code": 7, "created_at": "2024-01-01T00:00:00Z"});
        shape_transfer(rejected.as_object_mut().expect("object"), &TRANSFER_OUT);
        assert!(
            rejected.get("days_in_process").is_none(),
            "no end date: no count"
        );

        let mut open = json!({"state_code": 1, "created_at": "2024-01-01T00:00:00Z"});
        shape_transfer(open.as_object_mut().expect("object"), &TRANSFER_OUT);
        assert!(
            open["days_in_process"].as_i64().is_some_and(|d| d > 300),
            "open: until now"
        );
    }

    #[test]
    fn date_range_converts_and_rejects() {
        let (s, e) = date_range(Some("2026-01-01"), Some("2026-01-01")).expect("valid dates");
        assert_eq!(s.as_deref(), Some("1767196800"), "start of day, Hong Kong");
        assert_eq!(e.as_deref(), Some("1767283199"), "end of day, Hong Kong");
        assert!(
            date_range(Some("yesterday"), None).is_err(),
            "bad date rejected"
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

    #[test]
    fn deposit_page_size_caps_at_what_upstream_honours() {
        assert_eq!(deposit_page_size(None), 20, "default page size");
        assert_eq!(deposit_page_size(Some(5)), 5, "small size kept");
        assert_eq!(deposit_page_size(Some(0)), 1, "zero raised to one");
        assert_eq!(
            deposit_page_size(Some(50)),
            20,
            "above 20 upstream falls back to 10 rows, so cap at 20"
        );
    }
}
