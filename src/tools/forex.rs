//! Forex (currency exchange) channel.
//!
//! Wraps `longbridge::forex::ForexContext`. The flow is three steps and
//! asynchronous: `forex_quote` locks a rate and returns a `quote_id`,
//! `forex_submit_order` places the conversion against that quote (acceptance
//! only), and `forex_order` polls the order by `client_order_id` until it
//! reaches a terminal state.

use longbridge::forex::{ForexContext, GetForexQuoteOptions, SubmitForexOrderOptions};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::dry_run;
use crate::tools::{McpContext, tool_json};

/// Build a [`ForexContext`] for this request.
fn context(mctx: &McpContext) -> ForexContext {
    ForexContext::new(mctx.create_config())
}

// ── param structs ───────────────────────────────────────────────────────────

/// Parameters for `forex_quote`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForexQuoteParam {
    /// Convert-out currency, ISO 4217 (e.g. `USD`).
    pub from: String,
    /// Convert-in currency (e.g. `HKD`).
    pub to: String,
    /// Convert-out amount as a decimal string (e.g. `"1000.00"`). Mutually
    /// exclusive with `target_amount`.
    pub amount: Option<String>,
    /// Convert-in amount as a decimal string. Mutually exclusive with `amount`.
    pub target_amount: Option<String>,
}

/// Parameters for `forex_submit_order`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForexSubmitOrderParam {
    /// The `quote_id` returned by `forex_quote` (binds the locked rate).
    pub quote_id: String,
    /// Your own order id, unique across your accounts (the idempotency key).
    pub client_order_id: String,
    /// The `confirmation_code` from this order's dry run. WITHOUT IT NOTHING IS
    /// SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the forex channel.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed that exact order. The code is single use,
    /// expires in 10 minutes, and applies only to this exact order — change any
    /// field and it stops working. Never quote it back on your own initiative,
    /// and never in the same turn the user first asks.
    pub execute: Option<String>,
}

/// Parameters for `forex_order`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForexOrderParam {
    /// The `client_order_id` you submitted with `forex_submit_order`.
    pub client_order_id: String,
}

// ── handlers ─────────────────────────────────────────────────────────────────

/// Get a currency-exchange quote.
pub async fn forex_quote(
    mctx: &McpContext,
    p: ForexQuoteParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetForexQuoteOptions::new();
    if let Some(amount) = p.amount {
        let amount = amount.parse::<rust_decimal::Decimal>().map_err(|e| {
            McpError::invalid_params(format!("invalid amount '{amount}': {e}"), None)
        })?;
        opts = opts.amount(amount);
    }
    if let Some(target_amount) = p.target_amount {
        let target_amount = target_amount
            .parse::<rust_decimal::Decimal>()
            .map_err(|e| {
                McpError::invalid_params(
                    format!("invalid target_amount '{target_amount}': {e}"),
                    None,
                )
            })?;
        opts = opts.target_amount(target_amount);
    }
    let data = context(mctx)
        .quote(p.from, p.to, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

/// Query a forex order by `client_order_id`.
pub async fn forex_order(
    mctx: &McpContext,
    p: ForexOrderParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .order(p.client_order_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

/// Submit a forex order against a quote (dry-run gated).
pub async fn forex_submit_order(
    mctx: &McpContext,
    p: ForexSubmitOrderParam,
) -> Result<CallToolResult, McpError> {
    // `quote_id` + `client_order_id` fully determine the order (the quote fixes
    // the pair, amount and rate), so binding both to the confirmation code means
    // a preview can never be confirmed by a materially different request.
    let scope =
        dry_run::Scope::on_order("submit forex", &p.client_order_id).and("quote_id", &p.quote_id);

    // Two-step by design: without a confirmation code this places nothing.
    let Some(code) = p.execute.clone() else {
        return dry_run::result(
            &scope,
            serde_json::json!({
                "action": "forex_submit_order",
                "quote_id": p.quote_id,
                "client_order_id": p.client_order_id,
            }),
        );
    };
    scope.verify(&code)?;

    let client_order_id = p.client_order_id.clone();
    let opts = SubmitForexOrderOptions::new(p.quote_id, p.client_order_id);
    // submit_order returns `()` — acceptance only. Echo the id and point the
    // caller at forex_order for the (asynchronous) final state.
    context(mctx)
        .submit_order(opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&serde_json::json!({
        "dry_run": false,
        "accepted": true,
        "client_order_id": client_order_id,
        "next_step": "poll forex_order with this client_order_id until state is success or failed",
    }))
}
