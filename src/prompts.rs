//! MCP prompts — reusable workflow templates organised by Longbridge capability domain.
//!
//! Each prompt returns a pre-filled `User` message that instructs the LLM to call
//! the relevant Longbridge tools in sequence and synthesise the results.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{PromptMessage, PromptMessageRole};
use rmcp::prompt;
use rmcp::prompt_router;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::Longbridge;

// ── Argument types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolArgs {
    /// Security symbol in `TICKER.EXCHANGE` format (e.g. `AAPL.US`, `700.HK`, `600519.SH`)
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TradeArgs {
    /// Security symbol (e.g. `AAPL.US`, `700.HK`)
    pub symbol: String,
    /// Trade direction: `buy` or `sell`
    pub side: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketArgs {
    /// Market filter: `US`, `HK`, or `CN`. Omit to cover all three.
    pub market: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QuantArgs {
    /// Security symbol to run the indicator against (e.g. `AAPL.US`, `700.HK`)
    pub symbol: String,
    /// Candlestick period: `1m`, `5m`, `15m`, `30m`, `60m`, `day`, `week`, `month` (default: `day`)
    pub period: Option<String>,
}

// ── Prompts ───────────────────────────────────────────────────────────────────

#[prompt_router(vis = "pub(crate)")]
impl Longbridge {
    /// Real-time and historical market data for a single security.
    #[prompt(
        name = "market_data",
        description = "Market data deep-dive for a security: real-time quote, order-book depth, \
                       intraday price series, candlestick chart, capital flow, broker holdings, \
                       and market anomaly alerts. Provide a symbol such as AAPL.US or 700.HK."
    )]
    async fn market_data_prompt(
        &self,
        Parameters(args): Parameters<SymbolArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Give me a comprehensive market-data view of {symbol}. \
                 Run these tools in order:\n\
                 1. `quote` — real-time price, change %, volume, turnover.\n\
                 2. `depth` — current bid/ask order-book depth.\n\
                 3. `intraday` — today's minute-by-minute price series.\n\
                 4. `candlesticks` (period=day, count=60) — last 60 daily candles.\n\
                 5. `capital_flow` — net capital inflow/outflow over recent sessions.\n\
                 6. `capital_distribution` — breakdown by large/medium/small holder flows.\n\
                 7. `broker_holding` — top-broker holding concentration.\n\
                 8. `trade_stats` — buy/sell/neutral volume distribution.\n\
                 9. `anomaly` — any recent unusual price or volume alerts for {symbol}.\n\n\
                 Summarise: current price, intraday trend, 60-day price range, \
                 capital-flow direction, dominant broker positioning, and any anomalies."
            ),
        )]
    }

    /// Options and warrants analysis for an underlying security.
    #[prompt(
        name = "derivatives",
        description = "Derivatives analysis: option chain (expiry dates, strikes, greeks, IV, \
                       open interest) and warrant list for an underlying security. \
                       For US stocks also includes real-time put/call volume and ratio. \
                       Provide an underlying symbol such as AAPL.US or 700.HK."
    )]
    async fn derivatives_prompt(
        &self,
        Parameters(args): Parameters<SymbolArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Analyse derivatives for the underlying {symbol}. Run these tools:\n\
                 1. `quote` — current price and direction of {symbol}.\n\
                 2. `option_chain_expiry_date_list` — list available option expiry dates.\n\
                 3. `option_chain_info_by_date` — chain for the nearest two expiries \
                 (strikes, call/put symbols, last price, IV, greeks).\n\
                 4. `option_quote` — live quotes (IV, delta, gamma, theta, vega, open interest) \
                 for ATM ± 2 strikes on both expiries.\n\
                 5. `option_volume` — real-time call/put volume and put/call ratio (US stocks).\n\
                 6. `warrant_list` — active warrants (HK stocks); skip for US.\n\n\
                 Summarise:\n\
                 - Underlying price and recent trend\n\
                 - ATM IV for the nearest two expiries and whether it is elevated\n\
                 - Highest open-interest call and put strikes\n\
                 - Put/call volume ratio and what it implies about market positioning\n\
                 - Notable warrant activity (HK only)"
            ),
        )]
    }

    /// Company fundamentals: financials, dividends, shareholders, and corporate actions.
    #[prompt(
        name = "fundamentals",
        description = "Company fundamentals deep-dive: income statement, balance sheet, \
                       cash flow, dividend history, major shareholders, corporate actions, \
                       and business segment breakdown. Provide a symbol such as AAPL.US or 700.HK."
    )]
    async fn fundamentals_prompt(
        &self,
        Parameters(args): Parameters<SymbolArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Give me a full fundamentals breakdown for {symbol}. Run these tools:\n\
                 1. `company` — business description, sector, employees, key executives.\n\
                 2. `financial_report` (kind=IS) — last 4 quarters income statement \
                 (revenue, gross profit, operating income, net income, EPS).\n\
                 3. `financial_report` (kind=BS) — latest balance sheet \
                 (cash, total assets, total liabilities, equity).\n\
                 4. `financial_report` (kind=CF) — latest cash flow statement \
                 (operating, investing, financing).\n\
                 5. `dividend` — dividend history and most recent yield.\n\
                 6. `business_segments` — current-period revenue by business segment.\n\
                 7. `shareholder_top` — top 20 institutional shareholders.\n\
                 8. `corp_action` — recent corporate actions (splits, buybacks, name changes).\n\n\
                 Summarise: revenue trend (YoY), key margins, balance-sheet health, \
                 dividend track record, top shareholders, and any significant corporate events."
            ),
        )]
    }

    /// Analyst research, valuation, consensus estimates, and stock screener.
    #[prompt(
        name = "research",
        description = "Analyst research and valuation: institution ratings, consensus EPS estimates, \
                       P/E-P/B-P/S multiples vs peers, industry valuation percentiles, \
                       and screener-based peer discovery. Provide a symbol such as AAPL.US or 700.HK."
    )]
    async fn research_prompt(
        &self,
        Parameters(args): Parameters<SymbolArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Produce a research summary for {symbol}. Run these tools:\n\
                 1. `institution_rating` — analyst buy/hold/sell breakdown and consensus target.\n\
                 2. `institution_rating_detail` — target price history from individual firms.\n\
                 3. `forecast_eps` — forward EPS estimates and revision trend.\n\
                 4. `consensus` — revenue and EPS consensus for upcoming quarters.\n\
                 5. `valuation` — current P/E, P/B, P/S vs sector median.\n\
                 6. `valuation_history` — trailing 1-year P/E and P/B history.\n\
                 7. `industry_valuation` — peer group valuation comparison table.\n\
                 8. `valuation_rank` — where {symbol} ranks in its industry percentile.\n\n\
                 Summarise:\n\
                 - Analyst consensus rating and upside/downside to target\n\
                 - EPS growth trajectory (actual vs estimate)\n\
                 - Whether the stock is cheap or expensive vs its own history and peers\n\
                 - Key valuation risks or catalysts to watch"
            ),
        )]
    }

    /// Portfolio positions, P&L, account balance, and cash flow.
    #[prompt(
        name = "portfolio_account",
        description = "Full portfolio and account review: open stock and fund positions, \
                       unrealised P&L, account cash balance, today's profit summary, \
                       and recent cash flow (deposits, withdrawals, dividends)."
    )]
    async fn portfolio_account_prompt(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            "Give me a complete portfolio and account overview. Run these tools:\n\
             1. `stock_positions` — all open stock positions with symbol, quantity, \
             average cost, current price, and unrealised P&L.\n\
             2. `fund_positions` — any open fund positions.\n\
             3. `account_balance` — cash balance by currency and total asset value.\n\
             4. `profit_analysis` — today's realised and unrealised P&L summary.\n\
             5. `quote` (batch) — refresh current prices for every held symbol.\n\
             6. `cash_flow` — last 30 days of cash flow (deposits, withdrawals, dividends).\n\n\
             Summarise:\n\
             - Total portfolio value and overall return %\n\
             - Top 3 gainers and top 3 losers by unrealised P&L %\n\
             - Market/currency exposure breakdown (HK, US, CN)\n\
             - Positions with unrealised loss > 10% (flag as risk)\n\
             - Cash as % of total assets\n\
             - Net cash flow for the period",
        )]
    }

    /// Guided buy or sell order execution with pre-trade checks.
    #[prompt(
        name = "orders_trading",
        description = "Guided trade execution: validate symbol, check real-time price, \
                       estimate maximum order quantity, present a trade summary for confirmation, \
                       then submit a limit order. Provide a symbol (e.g. AAPL.US) and side (buy/sell)."
    )]
    async fn orders_trading_prompt(
        &self,
        Parameters(args): Parameters<TradeArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        let side = &args.side;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Help me execute a {side} order for {symbol}. \
                 Follow these steps precisely:\n\
                 1. `static_info` — confirm {symbol} is valid and tradeable.\n\
                 2. `quote` — get the current bid, ask, and last price.\n\
                 3. `account_balance` (for buy) or `stock_positions` (for sell) — \
                 determine available funds or current position size.\n\
                 4. `estimate_max_purchase_quantity` — \
                 calculate the maximum quantity I can {side}.\n\
                 5. `today_orders` — check for any existing open orders in {symbol}.\n\
                 6. Present a trade summary (symbol, side, suggested limit price, \
                 suggested quantity, estimated total value) and \
                 **wait for my explicit confirmation** before proceeding.\n\
                 7. After I confirm, submit a limit order using `submit_order`.\n\n\
                 Do NOT call `submit_order` without my explicit confirmation in step 6."
            ),
        )]
    }

    /// IPO discovery: calendar, active subscriptions, and IPO order management.
    #[prompt(
        name = "ipo",
        description = "IPO workflow: view the upcoming IPO calendar, check active subscriptions, \
                       review recently listed IPOs, and summarise any open IPO orders and their P&L. \
                       Optionally specify a market (HK or US)."
    )]
    async fn ipo_prompt(
        &self,
        Parameters(args): Parameters<MarketArgs>,
    ) -> Vec<PromptMessage> {
        let scope = match args.market.as_deref() {
            Some(m) => format!("Focus on the {m} market."),
            None => "Cover both HK and US markets.".to_string(),
        };
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Give me an IPO overview. {scope} Run these tools:\n\
                 1. `ipo_calendar` — upcoming IPOs with subscription dates, pricing, and size.\n\
                 2. `ipo_subscriptions` — stocks currently open for subscription.\n\
                 3. `ipo_listed` — recently listed IPOs and their post-listing performance.\n\
                 4. `ipo_orders` — my current IPO order positions.\n\
                 5. `ipo_profit_loss` — overall IPO investment P&L summary.\n\n\
                 Summarise:\n\
                 - Top upcoming IPOs by size and expected listing date\n\
                 - Which subscriptions are currently open and close soon\n\
                 - Best and worst performing recent listings (% from issue price)\n\
                 - My open IPO orders and their current status\n\
                 - Total IPO portfolio P&L"
            ),
        )]
    }

    /// Quantitative analysis: run a technical-indicator script against historical K-line data.
    #[prompt(
        name = "quant",
        description = "Quantitative analysis: fetch historical candlestick data and run a \
                       server-side indicator script (e.g. RSI, MACD, Bollinger Bands) using \
                       the quant_run tool. Provide a symbol and optional period."
    )]
    async fn quant_prompt(
        &self,
        Parameters(args): Parameters<QuantArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        let period = args.period.as_deref().unwrap_or("day");
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Run a quantitative technical analysis on {symbol} (period={period}). \
                 Follow these steps:\n\
                 1. `candlesticks` (symbol={symbol}, period={period}, count=120) — \
                 fetch 120 historical OHLCV candles as the base data.\n\
                 2. `quant_run` with a script that computes:\n\
                    - 14-period RSI\n\
                    - 12/26/9 MACD (line, signal, histogram)\n\
                    - 20-period Bollinger Bands (upper, mid, lower)\n\
                    - 50-day and 200-day simple moving averages\n\
                 3. `capital_distribution` — confirm institutional vs retail flow direction.\n\
                 4. `short_positions` — current short interest level.\n\n\
                 Interpret the results:\n\
                 - Is RSI overbought (>70) or oversold (<30)?\n\
                 - MACD crossover signal (bullish/bearish)\n\
                 - Price position relative to Bollinger Bands\n\
                 - Golden cross / death cross status (50d vs 200d MA)\n\
                 - Overall technical bias (bullish / neutral / bearish) with key levels"
            ),
        )]
    }

    /// Watchlist, price alerts, and DCA recurring investment plans.
    #[prompt(
        name = "watchlist",
        description = "Watchlist and automation overview: review all watchlist groups and their \
                       securities, list active price alerts, and summarise DCA recurring \
                       investment plans with performance statistics."
    )]
    async fn watchlist_prompt(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            "Give me an overview of my watchlists, alerts, and DCA plans. Run these tools:\n\
             1. `watchlist` — all watchlist groups and their securities.\n\
             2. `quote` (batch) — current prices for every symbol across all watchlists.\n\
             3. `alert_list` — all configured price alerts with their conditions and status.\n\
             4. `dca_list` — all DCA recurring investment plans (active, suspended, finished).\n\
             5. `dca_stats` — aggregate DCA performance (total invested, current value, return).\n\
             6. `sharelist_popular` — trending community sharelists for new ideas.\n\n\
             Summarise:\n\
             - Watchlist snapshot: each group with top movers (% change today)\n\
             - Alerts close to triggering (within 2% of condition price)\n\
             - DCA plans: frequency, amount, next execution date, and return %\n\
             - Any DCA plans that are suspended and may need attention\n\
             - 2–3 popular community sharelists worth exploring",
        )]
    }

    /// News, analyst filings, community discussion, and finance calendar.
    #[prompt(
        name = "content",
        description = "Content and information workflow for a security: latest news articles, \
                       regulatory filings, community discussion topics, and upcoming finance \
                       calendar events (earnings, dividends, macro). \
                       Provide a symbol such as AAPL.US or 700.HK."
    )]
    async fn content_prompt(
        &self,
        Parameters(args): Parameters<SymbolArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Gather all content and information for {symbol}. Run these tools:\n\
                 1. `news` (symbol={symbol}, count=10) — latest 10 news articles.\n\
                 2. `news_search` (keyword={symbol}) — additional news from keyword search.\n\
                 3. `filings` (symbol={symbol}) — recent regulatory filings \
                 (8-K, 10-Q, 10-K, or HK equivalents).\n\
                 4. `topic` (symbol={symbol}) — top community discussion threads.\n\
                 5. `finance_calendar` (category=report) — upcoming earnings and \
                 dividend events for {symbol}.\n\
                 6. `invest_relation` (symbol={symbol}) — investor relations announcements.\n\n\
                 Summarise:\n\
                 - Top 3 most significant news stories and their potential market impact\n\
                 - Most recent material filing and its key disclosures\n\
                 - Community sentiment from discussion topics (bullish/bearish/neutral)\n\
                 - Next earnings date and analyst EPS expectation\n\
                 - Any upcoming dividends or investor relations events"
            ),
        )]
    }
}
