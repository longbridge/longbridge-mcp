//! Predefined MCP prompt templates for common investment workflows.

use std::sync::OnceLock;

use rmcp::model::{GetPromptResult, ListPromptsResult, Prompt, PromptMessage, PromptMessageRole};

const NVDA_ANALYSIS_TEXT: &str = "\
Please analyze NVDA.US comprehensively using the available tools:

1. Recent price action: fetch the latest quote (use `quote`) and 30-day daily candlestick data \
(use `candlesticks`) to describe the price trend and momentum.
2. Latest earnings: retrieve the most recent financial report (use `financial_report_latest`) \
to determine whether NVIDIA beat or missed EPS and revenue estimates versus analyst consensus \
(use `forecast_eps`).
3. Profit margins: extract gross margin, operating margin, and net margin from the financial \
report or financial statement.
4. Forward guidance: summarize management guidance from the latest report and EPS forecast \
revisions.
5. Analyst outlook: retrieve institution ratings (use `institution_rating`) and analyst \
consensus (use `consensus`) to summarize the current Buy/Hold/Sell distribution and median \
price target.

Synthesize the above into a concise investment brief for NVDA.US.";

const COMPARE_STOCKS_TEXT: &str = "\
Please compare AAPL.US, TSLA.US, and NVDA.US across three dimensions to identify the best \
risk/reward opportunity:

1. Valuation: fetch P/E, P/B, and dividend yield for each stock (use `calc_indexes` or \
`valuation`).
2. Revenue growth: retrieve the latest financial reports (use `financial_report_latest`) for \
each ticker and compute year-over-year revenue growth rates.
3. Free cash flow: extract operating cash flow and capital expenditures from the latest \
financial statements (use `financial_statement`) to compute FCF and FCF yield.

Present the results in a comparison table, then conclude with a relative ranking and your view \
on which stock offers the best risk/reward at current prices.";

const PORTFOLIO_REVIEW_TEXT: &str = "\
Please review my Longbridge investment portfolio using the available tools:

1. Allocation: fetch my current stock holdings (use `stock_positions`) and fund holdings \
(use `fund_positions`) to show each position's size and its weight in the total portfolio value.
2. P&L drivers: use `profit_analysis` to identify which positions are the primary drivers of \
portfolio gains and losses.
3. Key risks: flag any positions with high concentration (>20% of total portfolio), significant \
unrealized losses (>15%), or unusually high beta/volatility.
4. Overvalued holdings: cross-reference current holdings with valuation metrics (P/E, P/B) via \
`calc_indexes` or `valuation` to identify any holdings trading at elevated multiples relative \
to sector peers.

Provide a structured summary with clear sections for each category and actionable observations \
where relevant.";

/// Returns the static list of all registered prompts.
pub(crate) fn all_prompts() -> &'static [Prompt] {
    static PROMPTS: OnceLock<Vec<Prompt>> = OnceLock::new();
    PROMPTS.get_or_init(|| {
        vec![
            Prompt::new(
                "nvda_analysis",
                Some("Analyze NVDA.US: recent price action, earnings beat/miss, profit margins, forward guidance, and analyst outlook"),
                None,
            ),
            Prompt::new(
                "compare_stocks",
                Some("Compare AAPL.US, TSLA.US, and NVDA.US on valuation, revenue growth, and free cash flow to identify the better buy"),
                None,
            ),
            Prompt::new(
                "portfolio_review",
                Some("Review my portfolio: allocation breakdown, key risks, P&L drivers, and potentially overvalued holdings"),
                None,
            ),
        ]
    })
}

/// Builds a `GetPromptResult` for the named prompt, or returns an `invalid_params`
/// error for unknown names.
pub(crate) fn get_prompt_result(name: &str) -> Result<GetPromptResult, rmcp::ErrorData> {
    let text = match name {
        "nvda_analysis" => NVDA_ANALYSIS_TEXT,
        "compare_stocks" => COMPARE_STOCKS_TEXT,
        "portfolio_review" => PORTFOLIO_REVIEW_TEXT,
        _ => {
            return Err(rmcp::ErrorData::invalid_params("unknown prompt", None));
        }
    };
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        PromptMessageRole::User,
        text,
    )]))
}

/// Returns `ListPromptsResult` with all registered prompts.
pub(crate) fn list_prompts_result() -> ListPromptsResult {
    ListPromptsResult::with_all_items(all_prompts().to_vec())
}
