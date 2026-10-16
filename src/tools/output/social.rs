//! Typed output schemas for the social / content, search, DCA, alert, and quant
//! tools. Mirrors the post-transform JSON shape produced by either `tool_json`
//! (SDK-typed responses) or the `http_*_tool` helpers (raw upstream JSON run
//! through `transform_json`) after the standard transform pipeline:
//!
//! - keys camelCase -> snake_case
//! - fields ending `_at` / `OffsetDateTime` -> RFC3339 `String`
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

/// Returned by `news_detail`. One news article's full detail.
///
/// HTTP passthrough of `GET /v1/content/news/{id}`'s `item`. proto3 JSON
/// encodes `int64` as a string, so `id` / `author.id` arrive as strings;
/// `published_at` (unix seconds) is converted to RFC3339 by the transform
/// pipeline. Per the passthrough convention above, every field is `Option` —
/// the upstream may omit unpopulated proto3 fields.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NewsDetailResponse {
    /// News article ID (numeric string).
    pub id: Option<String>,
    /// Title.
    pub title: Option<String>,
    /// Plain-text excerpt, no HTML tags.
    pub description: Option<String>,
    /// Markdown content.
    pub body: Option<String>,
    /// URL to the full article page.
    pub url: Option<String>,
    /// Article author.
    pub author: Option<NewsAuthor>,
    /// Attached images.
    pub images: Option<Vec<NewsImage>>,
    /// Comments count.
    pub comments_count: Option<i32>,
    /// Likes count.
    pub likes_count: Option<i32>,
    /// Shares count.
    pub shares_count: Option<i32>,
    /// Published time (RFC3339).
    pub published_at: Option<String>,
    /// Related stock tickers, format `<CODE>.<MARKET>` (e.g. "AAPL.US").
    pub tickers: Option<Vec<String>>,
}

/// Author of a news article.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NewsAuthor {
    /// Author ID (numeric string).
    pub id: Option<String>,
    /// Display name.
    pub name: Option<String>,
    /// Avatar URL.
    pub avatar: Option<String>,
}

/// An image attached to a news article.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NewsImage {
    /// Image URL.
    pub url: Option<String>,
    /// Width in pixels.
    pub width: Option<i32>,
    /// Height in pixels.
    pub height: Option<i32>,
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
/// the standard transform (`*_at` timestamps become RFC3339). Subset of the
/// wire payload — only the
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
    /// Security symbol, e.g. `"AAPL.US"`.
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
/// Plan fields follow the SDK `DcaPlan` model of the same endpoint (the
/// production sample had no plans); all optional.
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
    /// Plan status (Active / Suspended / Finished).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Security symbol (e.g. "AAPL.US").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Security name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_name: Option<String>,
    /// Market code, e.g. "US" / "HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Amount invested per cycle (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_invest_amount: Option<String>,
    /// Investment frequency (Daily / Weekly / Fortnightly / Monthly).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invest_frequency: Option<String>,
    /// Day of week for weekly plans, e.g. "Mon".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invest_day_of_week: Option<String>,
    /// Day of month for monthly plans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invest_day_of_month: Option<String>,
    /// Whether margin financing may be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_margin_finance: Option<bool>,
    /// Reminder lead time before execution, in hours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alter_hours: Option<String>,
    /// Next scheduled execution date (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_trd_date: Option<String>,
    /// Cumulative invested amount (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cum_amount: Option<String>,
    /// Number of completed investment cycles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_number: Option<i64>,
    /// Average cost (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_cost: Option<String>,
    /// Cumulative profit/loss (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cum_profit: Option<String>,
    /// Creation time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Last update time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
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
/// standard transform; all optional.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaStatsResponse {
    /// Total amount invested across plans (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_amount: Option<String>,
    /// Total profit/loss across plans (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_profit: Option<String>,
    /// Number of active plans (integer string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_count: Option<String>,
    /// Number of suspended plans (integer string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspended_count: Option<String>,
    /// Number of finished plans (integer string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_count: Option<String>,
    /// Days until the next scheduled investment (integer string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest_days: Option<String>,
    /// Plans with the nearest upcoming execution (same shape as `dca_list`'s
    /// `plans[]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearest_plans: Option<Vec<DcaPlan>>,
    /// Market codes the account has DCA investments in, e.g. ["US"].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invested_markets: Option<Vec<String>>,
}

/// Returned by `dca_check`. DCA-eligibility result per queried symbol,
/// forwarded after the standard transform. Verified against live responses.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaCheckResponse {
    /// Per-symbol support results, in request order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infos: Option<Vec<DcaCheckItem>>,
}

/// DCA-eligibility result for one symbol.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DcaCheckItem {
    /// Security symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Whether the symbol supports DCA recurring investment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_regular_saving: Option<bool>,
    /// Upstream DCA plan-type code for the symbol's market (opaque integer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regular_saving_plan_type: Option<i64>,
}
