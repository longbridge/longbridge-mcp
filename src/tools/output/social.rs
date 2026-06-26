//! Typed output schemas for the social / content, search, DCA, alert, and quant
//! tools. Mirrors the post-transform JSON shape produced by either `tool_json`
//! (SDK-typed responses) or the `http_*_tool` helpers (raw upstream JSON run
//! through `transform_json`) after the standard transform pipeline:
//!
//! - keys camelCase -> snake_case
//! - fields ending `_at` / `OffsetDateTime` -> RFC3339 `String`
//! - `counter_id` -> `symbol`, `counter_ids` -> `symbols` (value normalized to
//!   `<CODE>.<MARKET>`); prefixed variants renamed in place
//! - `aaid` / `account_channel` -> `null`
//! - `Decimal` -> `String`
//! - `http_*_unix` `unix_paths` fields -> RFC3339 `String`
//!
//! Each struct is intended to back `#[tool(output_schema = ...)]` on the
//! corresponding tool method in [`super::super`]. We only declare a struct when
//! the response root is a JSON object (the MCP spec requires `outputSchema` to
//! have root `type: "object"`); array- or scalar-rooted tools are intentionally
//! omitted.
//!
//! For HTTP-passthrough tools the upstream payload is forwarded mostly intact;
//! only fields whose presence and meaning are documented on the tool are
//! declared here. Every such field is `Option` and the structs are an
//! intentional subset of the wire payload — additional upstream fields pass
//! through untouched and are not described here.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `topic_detail`. Full details of a single community topic.
///
/// SDK-typed (`longbridge::content::OwnedTopic`) and serialized via `tool_json`.
/// `created_at` / `updated_at` are emitted as RFC3339 strings.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TopicDetailResponse {
    /// Topic ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Plain-text excerpt / description.
    pub description: String,
    /// Markdown body.
    pub body: String,
    /// Topic author.
    pub author: TopicAuthor,
    /// Related stock tickers, format `<CODE>.<MARKET>` (e.g. "TSLA.US").
    pub tickers: Vec<String>,
    /// Hashtag names.
    pub hashtags: Vec<String>,
    /// Attached images.
    pub images: Vec<TopicImage>,
    /// Likes count.
    pub likes_count: i32,
    /// Comments count.
    pub comments_count: i32,
    /// Views count.
    pub views_count: i32,
    /// Shares count.
    pub shares_count: i32,
    /// Content type: "article" or "post".
    pub topic_type: String,
    /// URL to the full topic page.
    pub detail_url: String,
    /// Created time (RFC3339).
    pub created_at: String,
    /// Last updated time (RFC3339).
    pub updated_at: String,
}

/// Author of a topic or reply.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TopicAuthor {
    /// Member ID.
    pub member_id: String,
    /// Display name.
    pub name: String,
    /// Avatar URL.
    pub avatar: String,
}

/// An image attached to a topic or reply.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TopicImage {
    /// Original image URL.
    pub url: String,
    /// Small thumbnail URL.
    pub sm: String,
    /// Large image URL.
    pub lg: String,
}

/// Returned by `topic_create`. The handler wraps the new topic ID in a single
/// `{ "id": ... }` object.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TopicCreateResponse {
    /// ID of the newly-created topic. Pass to `topic_detail` / `topic_replies`.
    pub id: String,
}

/// Returned by `topic_create_reply`. The created reply.
///
/// SDK-typed (`longbridge::content::TopicReply`) and serialized via `tool_json`.
/// `created_at` is emitted as an RFC3339 string.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TopicCreateReplyResponse {
    /// Reply ID.
    pub id: String,
    /// Topic ID this reply belongs to.
    pub topic_id: String,
    /// Reply body (plain text).
    pub body: String,
    /// Parent reply ID (`"0"` means top-level).
    pub reply_to_id: String,
    /// Reply author.
    pub author: TopicAuthor,
    /// Attached images.
    pub images: Vec<TopicImage>,
    /// Likes count.
    pub likes_count: i32,
    /// Nested replies count.
    pub comments_count: i32,
    /// Created time (RFC3339).
    pub created_at: String,
}

/// Returned by `alert_list`. The upstream price-alert payload, forwarded after
/// the standard transform (note: upstream `counter_id` is renamed to `symbol`
/// and `*_at` timestamps become RFC3339). Subset of the wire payload — only the
/// documented fields are declared; all are optional.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AlertListResponse {
    /// Per-symbol alert groups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lists: Option<Vec<AlertSymbolGroup>>,
}

/// A group of alert indicators configured for one security.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AlertSymbolGroup {
    /// Security symbol (upstream `counter_id`, normalized to `<CODE>.<MARKET>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Configured alert indicators for this symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicators: Option<Vec<AlertIndicator>>,
}

/// A single configured price-alert indicator.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AlertIndicator {
    /// Alert (indicator) ID. Use as `alert_id` in alert_delete/enable/disable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Indicator type ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator_id: Option<String>,
    /// Alert condition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Threshold price or percentage value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    /// Alert frequency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
    /// Whether the alert is currently enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Time the alert last triggered (RFC3339), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered_at: Option<String>,
}

/// Returned by `alert_enable` / `alert_disable`. The handler builds this exact
/// object on success.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AlertToggleResponse {
    /// The alert (indicator) ID that was toggled.
    pub alert_id: String,
    /// New enabled state: `true` for enable, `false` for disable.
    pub enabled: bool,
}

/// Returned by `dca_list`. Upstream DCA plan-query payload forwarded after the
/// standard transform; the `next_trd_date` unix field is converted to RFC3339.
/// Subset of the wire payload — only documented fields are declared; all
/// optional.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaListResponse {
    /// Recurring-investment (DCA) plans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plans: Option<Vec<DcaPlan>>,
}

/// A single DCA recurring-investment plan.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaPlan {
    /// Plan ID. Use with dca_update / dca_pause / dca_resume / dca_stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    /// Security symbol (e.g. "AAPL.US").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Amount invested per cycle (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    /// Settlement currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Investment frequency (Daily / Weekly / Monthly).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
    /// Plan status (Active / Suspended / Finished).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Next scheduled execution date (RFC3339; upstream `next_trd_date`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_execution_date: Option<String>,
}

/// Returned by `dca_history`. Execution records for one DCA plan, forwarded
/// after the standard transform. Subset of the wire payload — only documented
/// fields are declared; all optional.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaHistoryResponse {
    /// Execution records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executions: Option<Vec<DcaExecution>>,
}

/// A single DCA plan execution record.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaExecution {
    /// Execution date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Quantity acquired (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    /// Amount invested (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    /// Execution price (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    /// Execution status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Resulting order ID, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
}

/// Returned by `dca_stats`. Aggregate DCA statistics forwarded after the
/// standard transform. Subset of the wire payload — only documented fields are
/// declared; all optional.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaStatsResponse {
    /// Total amount invested across plans (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_invested: Option<String>,
    /// Current total market value (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_value: Option<String>,
    /// Total return (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_return: Option<String>,
    /// Overall return rate (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_rate: Option<String>,
    /// Number of plans included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_count: Option<i64>,
    /// Per-symbol breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<DcaStatsItem>>,
}

/// Per-symbol DCA statistics line.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaStatsItem {
    /// Security symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Amount invested in this symbol (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invested: Option<String>,
    /// Current value of this symbol's position (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Return rate for this symbol (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_rate: Option<String>,
}

/// Returned by `dca_check`. DCA-eligibility result per queried symbol,
/// forwarded after the standard transform (upstream `counter_ids` query →
/// per-symbol items). Subset of the wire payload — only documented fields are
/// declared; all optional.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaCheckResponse {
    /// Per-symbol support results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<DcaCheckItem>>,
}

/// DCA-eligibility result for one symbol.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaCheckItem {
    /// Security symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Whether the symbol supports DCA recurring investment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_dca: Option<bool>,
    /// Reason when unsupported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
