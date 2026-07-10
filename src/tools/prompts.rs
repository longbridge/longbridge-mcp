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

#[cfg(test)]
mod tests {
    use super::*;

    // AC-04: list_prompts枚举结果恰好包含 3 个 prompt
    #[test]
    fn all_prompts_returns_three() {
        assert_eq!(all_prompts().len(), 3, "expected exactly 3 prompts");
    }

    // AC-04: 三个 prompt 的名称正确
    #[test]
    fn all_prompts_names_are_correct() {
        let names: Vec<&str> = all_prompts().iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"nvda_analysis"), "nvda_analysis must be registered");
        assert!(names.contains(&"compare_stocks"), "compare_stocks must be registered");
        assert!(names.contains(&"portfolio_review"), "portfolio_review must be registered");
    }

    // AC-04: 每个 prompt 都有非空 description
    #[test]
    fn all_prompts_have_non_empty_descriptions() {
        for prompt in all_prompts() {
            assert!(
                prompt.description.as_deref().is_some_and(|d| !d.is_empty()),
                "prompt {:?} must have a non-empty description",
                prompt.name
            );
        }
    }

    // AC-04: 所有 prompt 的 arguments 为空
    #[test]
    fn all_prompts_have_no_arguments() {
        for prompt in all_prompts() {
            let empty = prompt.arguments.as_ref().map_or(true, |a| a.is_empty());
            assert!(empty, "prompt {:?} should have no arguments", prompt.name);
        }
    }

    // AC-01: nvda_analysis 返回 Ok
    #[test]
    fn get_prompt_nvda_analysis_returns_ok() {
        assert!(get_prompt_result("nvda_analysis").is_ok(), "nvda_analysis must return Ok");
    }

    // AC-02: compare_stocks 返回 Ok
    #[test]
    fn get_prompt_compare_stocks_returns_ok() {
        assert!(get_prompt_result("compare_stocks").is_ok(), "compare_stocks must return Ok");
    }

    // AC-03: portfolio_review 返回 Ok
    #[test]
    fn get_prompt_portfolio_review_returns_ok() {
        assert!(get_prompt_result("portfolio_review").is_ok(), "portfolio_review must return Ok");
    }

    // AC-05: 未知 prompt 名返回 Err，不 panic
    #[test]
    fn get_prompt_unknown_returns_error() {
        assert!(
            get_prompt_result("unknown_prompt").is_err(),
            "unknown prompt name must return Err"
        );
    }

    // AC-01: nvda_analysis 文本包含目标股票代码
    #[test]
    fn nvda_analysis_text_contains_nvda_symbol() {
        assert!(
            NVDA_ANALYSIS_TEXT.contains("NVDA.US"),
            "nvda_analysis text must reference NVDA.US"
        );
    }

    // AC-02: compare_stocks 文本引用三只股票
    #[test]
    fn compare_stocks_text_references_all_three_tickers() {
        assert!(COMPARE_STOCKS_TEXT.contains("AAPL.US"), "must reference AAPL.US");
        assert!(COMPARE_STOCKS_TEXT.contains("TSLA.US"), "must reference TSLA.US");
        assert!(COMPARE_STOCKS_TEXT.contains("NVDA.US"), "must reference NVDA.US");
    }

    // AC-03: portfolio_review 文本引用 stock_positions 和 profit_analysis 工具
    #[test]
    fn portfolio_review_text_references_required_tools() {
        assert!(
            PORTFOLIO_REVIEW_TEXT.contains("stock_positions"),
            "must reference stock_positions"
        );
        assert!(
            PORTFOLIO_REVIEW_TEXT.contains("profit_analysis"),
            "must reference profit_analysis"
        );
    }

    // AC-04: list_prompts_result 返回 3 个条目
    #[test]
    fn list_prompts_result_has_three_items() {
        let result = list_prompts_result();
        assert_eq!(result.prompts.len(), 3, "list_prompts must return exactly 3 prompts");
    }
}
