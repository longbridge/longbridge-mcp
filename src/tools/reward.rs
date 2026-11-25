//! Reward-center read tools: the user's activity reward records, coupon
//! cards, and the orders a coupon was applied to.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use serde_json::{Value, json};

use crate::serialize::{convert_unix_paths, drop_keys, strip_empty_strings, strip_nulls};
use crate::tools::support::http_client::{http_get_value, value_result};

/// Fields kept from each reward sub-object (`card` / `cash` / `prize`). This is
/// an allowlist: prize rewards also carry redemption secrets (`coupon_secret`)
/// and the shipping contact (`express_info`, `addresses`), which must never
/// reach a model.
const REWARD_FIELDS: &[&str] = &[
    "record_id",
    "activity_title",
    "title",
    "description",
    "card_type",
    "card_status",
    "prize_type",
    "amount",
    "currency",
    "status",
    "deliver_status",
    "exit_status",
    "send_time",
    "start_time",
    "end_time",
    "effective_time",
    "receive_deadline",
];

/// Reward-center `status` codes (from the app's reward page).
const REWARD_STATUS: &[(i64, &str)] = &[
    (0, "upcoming"),
    (1, "cancelled"),
    (2, "exited_activity"),
    (8, "unclaimed"),
    (10, "issuing"),
    (12, "issued"),
    (13, "failed"),
    (16, "exchanged"),
    (17, "expired"),
    (18, "refunded"),
];

/// Reward-center `deliver_status` codes: what happened after `issued`.
const DELIVER_STATUS: &[(i64, &str)] = &[
    (0, "not_activated"),
    (1, "activated_or_shipping"),
    (2, "consumed"),
    (5, "e_voucher_activating"),
    (6, "e_voucher_expired_before_activation"),
    (7, "e_voucher_invalid"),
    (8, "address_missing"),
    (9, "address_filled"),
    (10, "shipping"),
    (11, "address_filled"),
];

/// Reward-center `reward_type` codes, which the `kind` filter maps onto.
const REWARD_TYPES: &[(i64, &str)] = &[(0, "cash"), (1, "gift"), (2, "coupon"), (3, "task")];

/// Coupon `card_status` codes (from the app's coupon list).
const CARD_STATUS: &[(i64, &str)] = &[
    (0, "unused"),
    (1, "used"),
    (2, "expired"),
    (3, "locked"),
    (4, "in_use"),
    (5, "processing"),
];

/// Longest `description` kept for a coupon when its terms are not requested.
const SHORT_DESCRIPTION_CHARS: usize = 120;

/// Most upstream pages read when filtering rewards by date.
const MAX_REWARD_PAGES: u32 = 10;

/// Commission-card `bz_types`: the instruments the card applies to.
const TARGET_TYPES: &[(i64, &str)] = &[
    (0, "stocks"),
    (1, "options"),
    (2, "warrants"),
    (3, "ETF"),
    (4, "LOF"),
];

/// Name for an integer code, or `None` for an unknown code.
fn code_name(value: Option<&Value>, names: &[(i64, &'static str)]) -> Option<&'static str> {
    let code = value?.as_i64()?;
    names
        .iter()
        .find_map(|(c, name)| (*c == code).then_some(*name))
}

/// Reward fields that are unix seconds.
const REWARD_TIME_KEYS: &[&str] = &[
    "send_time",
    "start_time",
    "end_time",
    "effective_time",
    "receive_deadline",
];

/// Coupon-card fields that only style the app's card list.
const CARD_UI_KEYS: &[&str] = &[
    "cover",
    "background",
    "short_background",
    "style",
    "new_flag",
    "invisible",
    "member_id",
];

/// Coupon-card fields that are unix seconds.
const CARD_TIME_PATHS: &[&str] = &[
    "cards.*.start_time",
    "cards.*.end_time",
    "cards.*.activate_time",
    "cards.*.quote.current_end_time",
    "cards.*.commission.commission_expired_time",
];

/// Card-level fields that duplicate the typed sub-object (`commission{…}`,
/// `quote{…}`); upstream marks them deprecated.
const CARD_DEPRECATED_KEYS: &[&str] = &[
    "receive_duration",
    "quote_duration",
    "markets",
    "rebate_rate",
    "bz_types",
    "commission_balance",
    "commission_duration",
];

/// Card fields (at any depth) upstream fills with `"0"` when unset.
const CARD_ZERO_UNSET: &[&str] = &[
    "activate_time",
    "commission_expired_time",
    "current_end_time",
    "settled_store_id",
];

/// Order fields that only colour the app's order list.
const ORDER_UI_KEYS: &[&str] = &["state_color"];

/// Remove `"0"` / `0` placeholders under `keys` at any depth.
fn drop_zero_placeholders(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(map) => {
            map.retain(|k, v| {
                !(keys.contains(&k.as_str()) && (v.as_i64() == Some(0) || v.as_str() == Some("0")))
            });
            map.values_mut()
                .for_each(|v| drop_zero_placeholders(v, keys));
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|v| drop_zero_placeholders(v, keys)),
        _ => {}
    }
}

/// Clean one coupon card: drop deprecated duplicates and `"0"` placeholders,
/// and replace a never-expiring card's far-future `end_time` (left as raw
/// unix seconds because it is past 2100) with `no_expiry: true`.
fn shape_coupon(card: &mut Value, with_terms: bool) {
    if !with_terms && let Some(obj) = card.as_object_mut() {
        obj.remove("detail");
        if let Some(Value::String(text)) = obj.get_mut("description")
            && text.chars().count() > SHORT_DESCRIPTION_CHARS
        {
            let short: String = text.chars().take(SHORT_DESCRIPTION_CHARS).collect();
            *text = format!("{short}…");
        }
    }
    let Some(obj) = card.as_object_mut() else {
        return;
    };
    for key in CARD_DEPRECATED_KEYS {
        obj.remove(*key);
    }
    let raw_end = obj
        .get("end_time")
        .and_then(Value::as_str)
        .is_some_and(|t| t.chars().all(|c| c.is_ascii_digit()));
    if raw_end {
        obj.remove("end_time");
        obj.insert("no_expiry".into(), Value::Bool(true));
    }
    drop_zero_placeholders(card, CARD_ZERO_UNSET);
    if let Some(obj) = card.as_object_mut()
        && let Some(name) = code_name(obj.get("card_status"), CARD_STATUS)
    {
        obj.insert("card_status_label".into(), Value::String(name.to_string()));
    }
    // Name the instruments a commission card applies to. (IPO cards reuse
    // `bz_types` with other meanings, so only `commission` is labelled.)
    if let Some(commission) = card.get_mut("commission").and_then(Value::as_object_mut) {
        let targets: Vec<Value> = commission
            .get("bz_types")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|t| code_name(Some(t), TARGET_TYPES))
            .map(|name| Value::String(name.to_string()))
            .collect();
        if !targets.is_empty() {
            commission.insert("targets".into(), Value::Array(targets));
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RewardRecordsParam {
    /// Reward type filter: "cash", "gift", "coupon" or "task" (default: all)
    pub kind: Option<String>,
    /// Issued on or after this date (yyyy-mm-dd or RFC3339); with start/end the tool pages through the history itself
    pub start: Option<String>,
    /// Issued on or before this date (yyyy-mm-dd or RFC3339)
    pub end: Option<String>,
    /// Page number (default: 1; ignored when start or end is given)
    pub page: Option<u32>,
    /// Page size (default: 20, max 50)
    pub size: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CouponsParam {
    /// "valid" (not yet expired, default) or "expired"
    pub status: Option<String>,
    /// Card type filter, e.g. "commission", "platform_fee_deduct", "stock", "quote", "cash" (default: all)
    pub card_type: Option<String>,
    /// Include each card's full terms of use (`description`, `detail`); default false returns a short description only
    pub with_terms: Option<bool>,
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20, max 50)
    pub size: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CouponOrdersParam {
    /// Coupon card id, from `coupons` (`cards[].card_id`)
    pub card_id: String,
}

fn page_params(page: Option<u32>, size: Option<u32>) -> (String, String) {
    (
        page.unwrap_or(1).max(1).to_string(),
        size.unwrap_or(20).clamp(1, 50).to_string(),
    )
}

/// Flatten one reward-list item into `{kind, reward_type, …allowlisted fields}`.
fn shape_reward(item: &Value) -> Option<Value> {
    let found = ["cash", "card", "prize"]
        .iter()
        .find_map(|kind| item.get(*kind).map(|detail| (*kind, detail)));
    let mut row = serde_json::Map::new();
    if let Some(reward_type) = item.get("reward_type") {
        row.insert("reward_type".into(), reward_type.clone());
        if let Some(name) = code_name(Some(reward_type), REWARD_TYPES) {
            row.insert("reward_type_label".into(), Value::String(name.to_string()));
        }
    }
    match found {
        Some((kind, detail)) => {
            row.insert("kind".into(), Value::String(kind.to_string()));
            for key in REWARD_FIELDS {
                if let Some(v) = detail.get(*key) {
                    row.insert((*key).to_string(), v.clone());
                }
            }
        }
        // A record without a known detail object (e.g. task coins): keep the
        // type so the record is not silently lost.
        None if row.contains_key("reward_type") => {}
        None => return None,
    }
    if let Some(name) = code_name(row.get("status"), REWARD_STATUS) {
        row.insert("status_label".into(), Value::String(name.to_string()));
    }
    if let Some(name) = code_name(row.get("deliver_status"), DELIVER_STATUS) {
        row.insert(
            "deliver_status_label".into(),
            Value::String(name.to_string()),
        );
    }
    let mut row = Value::Object(row);
    convert_unix_paths(&mut row, REWARD_TIME_KEYS);
    Some(row)
}

/// Map the `kind` parameter to the upstream `reward_type` code.
fn reward_type_code(kind: Option<&str>) -> Result<Option<&'static str>, McpError> {
    match kind.map(|k| k.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("all") => Ok(None),
        Some("cash") => Ok(Some("0")),
        Some("gift") | Some("prize") => Ok(Some("1")),
        Some("coupon") | Some("card") => Ok(Some("2")),
        Some("task") => Ok(Some("3")),
        Some(other) => Err(McpError::invalid_params(
            format!("invalid kind '{other}': expected cash, gift, coupon or task"),
            None,
        )),
    }
}

/// Whether a shaped reward's `send_time` falls inside `[start, end]`.
/// A record without a send time is kept: it has not been issued yet.
fn in_range(
    row: &Value,
    start: Option<&time::OffsetDateTime>,
    end: Option<&time::OffsetDateTime>,
) -> bool {
    let Some(sent) = row
        .get("send_time")
        .and_then(Value::as_str)
        .and_then(|t| crate::tools::support::parse::parse_rfc3339(t).ok())
    else {
        return true;
    };
    start.is_none_or(|s| sent >= *s) && end.is_none_or(|e| sent <= *e)
}

/// List the user's activity reward records, newest first.
pub async fn reward_records(
    mctx: &crate::tools::McpContext,
    p: RewardRecordsParam,
) -> Result<CallToolResult, McpError> {
    use crate::tools::support::parse;

    let client = mctx.create_http_client();
    let channel = mctx.account_channel();
    let reward_type = reward_type_code(p.kind.as_deref())?;
    let start = p.start.as_deref().map(parse::parse_rfc3339).transpose()?;
    let end = p.end.as_deref().map(parse::parse_rfc3339_end).transpose()?;
    let dated = start.is_some() || end.is_some();
    let (first_page, page_size) = if dated {
        (1, "50".to_string())
    } else {
        let (page, size) = page_params(p.page, p.size);
        (page.parse().unwrap_or(1), size)
    };

    let mut rewards = Vec::new();
    let mut total;
    let mut page = first_page;
    loop {
        let current = page.to_string();
        let mut params = vec![
            ("current", current.as_str()),
            ("page_size", page_size.as_str()),
            ("account_channel", channel.as_str()),
        ];
        if let Some(t) = reward_type {
            params.push(("reward_type", t));
        }
        let raw = http_get_value(&client, "/v1/account/rewards", &params).await?;
        total = raw.get("total").cloned().unwrap_or(Value::Null);
        let items = raw
            .get("list")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let fetched = items.len();
        let rows: Vec<Value> = items.iter().filter_map(shape_reward).collect();
        if !dated {
            rewards = rows;
            break;
        }
        // Newest first: once a page ends before `start`, older pages cannot match.
        let past_start = rows
            .last()
            .is_some_and(|last| start.is_some() && !in_range(last, start.as_ref(), None));
        rewards.extend(
            rows.into_iter()
                .filter(|r| in_range(r, start.as_ref(), end.as_ref())),
        );
        if past_start || fetched < 50 || page >= MAX_REWARD_PAGES {
            break;
        }
        page += 1;
    }
    let mut value = json!({ "rewards": rewards, "total": total });
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result(&value)
}

/// Map the `status` parameter to the upstream `card_status` filter.
fn card_status_code(status: Option<&str>) -> Result<&'static str, McpError> {
    match status.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("valid") => Ok("-1"),
        Some("expired") => Ok("2"),
        Some(other) => Err(McpError::invalid_params(
            format!("invalid status '{other}': expected \"valid\" or \"expired\""),
            None,
        )),
    }
}

/// List the user's coupon cards.
pub async fn coupons(
    mctx: &crate::tools::McpContext,
    p: CouponsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let channel = mctx.account_channel();
    let status = card_status_code(p.status.as_deref())?;
    let (current, page_size) = page_params(p.page, p.size);
    let mut params = vec![
        ("current", current.as_str()),
        ("page_size", page_size.as_str()),
        ("card_status", status),
        ("account_channel", channel.as_str()),
    ];
    if let Some(t) = p
        .card_type
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        params.push(("card_type", t));
    }
    let mut raw = http_get_value(&client, "/v1/account/cards", &params).await?;
    // The app splits the list into pinned top / main / bottom sections; a
    // support answer needs them as one list.
    let cards: Vec<Value> = ["top_list", "list", "bottom_list"]
        .iter()
        .filter_map(|key| raw.get_mut(*key).map(Value::take))
        .filter_map(|section| match section {
            Value::Array(cards) => Some(cards),
            _ => None,
        })
        .flatten()
        .collect();
    let mut value = json!({
        "cards": cards,
        "total": raw.get("total"),
        "expired_count": raw.get("expired_count"),
    });
    drop_keys(&mut value, CARD_UI_KEYS);
    convert_unix_paths(&mut value, CARD_TIME_PATHS);
    if let Some(cards) = value.get_mut("cards").and_then(Value::as_array_mut) {
        let with_terms = p.with_terms.unwrap_or(false);
        cards
            .iter_mut()
            .for_each(|card| shape_coupon(card, with_terms));
    }
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    value_result(&value)
}

/// Whether `id` has the shape of a coupon card id (32 hex characters).
fn is_coupon_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// List the orders a coupon card was applied to: `locked` (in progress) and
/// `settled` (rebate settled).
pub async fn coupon_orders(
    mctx: &crate::tools::McpContext,
    p: CouponOrdersParam,
) -> Result<CallToolResult, McpError> {
    let card_id = p.card_id.trim();
    if !is_coupon_id(card_id) {
        let hint = if !card_id.is_empty() && card_id.chars().all(|c| c.is_ascii_digit()) {
            " — this looks like a bank card id from bank_cards"
        } else {
            ""
        };
        return Err(McpError::invalid_params(
            format!(
                "invalid card_id '{card_id}': expected the 32-character coupon id from coupons (cards[].card_id){hint}"
            ),
            None,
        ));
    }
    let client = mctx.create_http_client();
    let path = "/v1/account/card-orders";
    let locked_params = [("card_id", card_id), ("order_type", "locked")];
    let settled_params = [("card_id", card_id), ("order_type", "settled")];
    // Sequential on purpose: the gateway rate-limits this route per member
    // tightly, and two concurrent calls draw 429s.
    let locked = http_get_value(&client, path, &locked_params).await?;
    let settled = http_get_value(&client, path, &settled_params).await?;
    let orders = |v: Value| v.get("orders").cloned().unwrap_or_else(|| json!([]));
    let mut value = json!({
        "card_id": card_id,
        "locked": orders(locked),
        "settled": orders(settled),
    });
    drop_keys(&mut value, ORDER_UI_KEYS);
    strip_empty_strings(&mut value);
    value_result(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_reward_drops_secrets() {
        let item = json!({
            "reward_type": 3,
            "prize": {
                "activity_title": "Open account",
                "title": "Coffee",
                "status": 2,
                "send_time": 1_790_000_000,
                "coupon_secret": {"secret": "ABC-123"},
                "express_info": {"phone": "12345678"},
                "addresses": [{"detail": "somewhere"}]
            }
        });
        let row = shape_reward(&item).expect("a prize item must shape");
        assert_eq!(row["kind"], "prize", "kind comes from the sub-object");
        assert_eq!(row["reward_type"], 3, "reward_type carried over");
        assert!(
            row.get("coupon_secret").is_none(),
            "secrets must be dropped"
        );
        assert!(row.get("express_info").is_none(), "contact must be dropped");
        assert!(row.get("addresses").is_none(), "addresses must be dropped");
        assert!(
            row["send_time"]
                .as_str()
                .is_some_and(|t| t.starts_with("2026-")),
            "send_time must be RFC3339"
        );
    }

    #[test]
    fn shape_reward_labels_status() {
        let item = json!({"reward_type": 0, "cash": {"status": 12, "amount": 38}});
        let row = shape_reward(&item).expect("a cash item must shape");
        assert_eq!(row["status_label"], "issued", "12 is issued");
        let item = json!({"reward_type": 3, "prize": {"status": 12, "deliver_status": 8}});
        let row = shape_reward(&item).expect("a prize item must shape");
        assert_eq!(
            row["deliver_status_label"], "address_missing",
            "deliver label"
        );
        let unknown = shape_reward(&json!({"cash": {"status": 99}})).expect("shapes");
        assert!(
            unknown.get("status_label").is_none(),
            "unknown code has no label"
        );
    }

    #[test]
    fn shape_reward_skips_unknown_items() {
        let task = shape_reward(&json!({"reward_type": 3})).expect("typed record kept");
        assert_eq!(task["reward_type_label"], "task", "type labelled");
        assert!(
            shape_reward(&json!({"other": 1})).is_none(),
            "nothing to show"
        );
    }

    #[test]
    fn shape_coupon_cleans_real_card() {
        // Shape of a real production lifetime commission card, 2026-11-25.
        let mut value = json!({"cards": [{
            "card_id": "bd6b", "card_type": "commission", "start_time": 1_750_313_704,
            "end_time": "8061747303", "card_status": 4, "activate_time": "0",
            "commission": {"markets": ["US"], "bz_types": [0, 2, 3], "commission_expired_time": "0"},
            "markets": ["US"], "rebate_rate": "1", "quote_duration": "0",
            "available_trade_session": "夜盘、盘中+盘前盘后",
        }]});
        convert_unix_paths(&mut value, CARD_TIME_PATHS);
        shape_coupon(&mut value["cards"][0], false);
        let card = &value["cards"][0];
        assert_eq!(
            card["no_expiry"], true,
            "far-future end_time becomes no_expiry"
        );
        assert!(card.get("end_time").is_none(), "raw end_time removed");
        assert!(
            card.get("markets").is_none(),
            "deprecated duplicate removed"
        );
        assert!(
            card.get("activate_time").is_none(),
            "zero placeholder removed"
        );
        assert!(
            card["commission"].get("commission_expired_time").is_none(),
            "nested zero placeholder removed"
        );
        assert_eq!(
            card["commission"]["markets"],
            json!(["US"]),
            "typed markets kept"
        );
        assert!(card["start_time"].is_string(), "start_time converted");
        assert_eq!(
            card["available_trade_session"], "夜盘、盘中+盘前盘后",
            "session kept"
        );
    }

    #[test]
    fn shape_coupon_labels_and_trims_terms() {
        let long = "x".repeat(300);
        let mut card = json!({"card_status": 4, "description": long, "detail": "terms"});
        shape_coupon(&mut card, false);
        assert_eq!(card["card_status_label"], "in_use", "status labelled");
        assert!(card.get("detail").is_none(), "detail dropped by default");
        assert_eq!(
            card["description"].as_str().map(|d| d.chars().count()),
            Some(SHORT_DESCRIPTION_CHARS + 1),
            "description shortened"
        );
        let mut full = json!({"description": "x".repeat(300), "detail": "terms"});
        shape_coupon(&mut full, true);
        assert_eq!(full["detail"], "terms", "with_terms keeps detail");
    }

    #[test]
    fn reward_type_code_maps_kinds() {
        assert_eq!(
            reward_type_code(Some("coupon")).ok(),
            Some(Some("2")),
            "coupon"
        );
        assert_eq!(reward_type_code(None).ok(), Some(None), "all");
        assert!(reward_type_code(Some("stock")).is_err(), "unknown kind");
    }

    #[test]
    fn in_range_uses_send_time() {
        use crate::tools::support::parse::{parse_rfc3339, parse_rfc3339_end};
        let start = parse_rfc3339("2026-08-01").expect("date");
        let end = parse_rfc3339_end("2026-08-31").expect("date");
        let inside = json!({"send_time": "2026-08-05T08:18:16Z"});
        let before = json!({"send_time": "2026-07-07T14:37:25Z"});
        assert!(in_range(&inside, Some(&start), Some(&end)), "inside");
        assert!(!in_range(&before, Some(&start), Some(&end)), "before start");
        assert!(in_range(&json!({}), Some(&start), None), "unsent kept");
    }

    #[test]
    fn coupon_id_shape() {
        assert!(
            is_coupon_id("bd6b40914cd411f08becae6ae6fae931"),
            "real coupon id"
        );
        assert!(!is_coupon_id("493082"), "bank card id");
    }

    #[test]
    fn card_status_code_maps_and_rejects() {
        assert_eq!(card_status_code(None).ok(), Some("-1"), "default is valid");
        assert_eq!(
            card_status_code(Some("Expired")).ok(),
            Some("2"),
            "case-insensitive"
        );
        assert!(
            card_status_code(Some("used")).is_err(),
            "unsupported filter"
        );
    }
}
