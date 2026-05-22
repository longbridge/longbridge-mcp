//! MCP prompts — reusable workflow templates for common Longbridge tasks.
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StockResearchArgs {
    /// Stock symbol in `TICKER.EXCHANGE` format (e.g. `AAPL.US`, `700.HK`, `600519.SH`)
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OptionsScreeningArgs {
    /// Underlying stock symbol (e.g. `AAPL.US`, `700.HK`)
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TradeExecutionArgs {
    /// Stock symbol to trade (e.g. `AAPL.US`, `700.HK`)
    pub symbol: String,
    /// Trade direction: `buy` or `sell`
    pub side: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketOverviewArgs {
    /// Market to focus on: `US`, `HK`, or `CN`. Omit to cover all three.
    pub market: Option<String>,
}

#[prompt_router(vis = "pub(crate)")]
impl Longbridge {
    /// Research a stock comprehensively: quote, static info, fundamentals, analyst ratings, news.
    #[prompt(
        name = "stock_research",
        description = "Comprehensive stock research workflow: real-time quote, company info, \
                       financial fundamentals, analyst ratings, and recent news. \
                       Provide a stock symbol such as AAPL.US or 700.HK."
    )]
    async fn stock_research_prompt(
        &self,
        Parameters(args): Parameters<StockResearchArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Research the stock {symbol} comprehensively. \
                 Use the following tools in order:\n\
                 1. `quote` — get the real-time price, change, and volume.\n\
                 2. `static_info` — retrieve the company name, exchange, sector, and listing details.\n\
                 3. `financial_summary` — fetch the latest revenue, earnings, P/E ratio, and EPS.\n\
                 4. `institution_rating` — get analyst ratings and consensus price target.\n\
                 5. `news` — retrieve the 5 most recent news articles.\n\n\
                 After gathering all data, write a concise investment brief covering: \
                 (a) current price and recent trend, \
                 (b) key valuation metrics, \
                 (c) analyst consensus and upside/downside to target, \
                 (d) notable news or catalysts."
            ),
        )]
    }

    /// Analyse the authenticated user's portfolio: positions, unrealised P&L, and cash balance.
    #[prompt(
        name = "portfolio_analysis",
        description = "Analyse the authenticated user's portfolio: open positions, unrealised P&L, \
                       sector exposure, cash balance, and today's profit & loss summary."
    )]
    async fn portfolio_analysis_prompt(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            "Analyse my current investment portfolio. Use the following tools:\n\
             1. `stock_positions` — list all open positions with symbol, quantity, average cost, \
             and unrealised P&L.\n\
             2. `account_balance` — retrieve my cash balance and total asset value.\n\
             3. `profit_analysis` — get today's realised and unrealised P&L breakdown.\n\
             4. `quote` (batch) — fetch current prices for every held symbol.\n\n\
             Then provide:\n\
             - Total portfolio value and overall P&L %\n\
             - Top 3 gainers and top 3 losers by unrealised P&L %\n\
             - Sector/market breakdown (HK, US, CN)\n\
             - Any position where unrealised loss exceeds 10% (flag as risk)\n\
             - Cash as a % of total assets",
        )]
    }

    /// Screen options for an underlying: chain, implied volatility, open interest, greeks.
    #[prompt(
        name = "options_screening",
        description = "Screen options for an underlying stock: option chain for the nearest two \
                       expiries, implied volatility, open interest, and key greeks. \
                       Provide an underlying symbol such as AAPL.US."
    )]
    async fn options_screening_prompt(
        &self,
        Parameters(args): Parameters<OptionsScreeningArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Screen options for the underlying {symbol}. Use the following tools:\n\
                 1. `quote` — get the current price and recent trend of {symbol}.\n\
                 2. `option_chain` — retrieve the option chain for the nearest two expiry dates.\n\
                 3. `option_quote` — fetch quotes (IV, greeks, open interest) for the \
                 5 most-active strikes near the current price (ATM ±5%).\n\n\
                 Then summarise:\n\
                 - Underlying price and recent direction\n\
                 - ATM implied volatility for both expiries\n\
                 - Highest open-interest call and put strikes\n\
                 - Whether the options market implies a significant move (IV > 20% for weeklies)\n\
                 - A brief view on whether the skew favours calls or puts"
            ),
        )]
    }

    /// Step-by-step trade execution: validate symbol, check price, estimate quantity, confirm, submit.
    #[prompt(
        name = "trade_execution",
        description = "Guided trade execution workflow: validate the symbol, check real-time price, \
                       estimate maximum order quantity given the account balance, ask for confirmation, \
                       then submit a limit order. Provide a symbol (e.g. AAPL.US) and side (buy/sell)."
    )]
    async fn trade_execution_prompt(
        &self,
        Parameters(args): Parameters<TradeExecutionArgs>,
    ) -> Vec<PromptMessage> {
        let symbol = &args.symbol;
        let side = &args.side;
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Help me execute a {side} order for {symbol}. \
                 Follow these steps precisely:\n\
                 1. `static_info` — confirm the symbol is valid and tradeable.\n\
                 2. `quote` — get the current bid, ask, and last price for {symbol}.\n\
                 3. `account_balance` (for buy) or `stock_positions` (for sell) — \
                 determine available funds or current position size.\n\
                 4. `estimate_max_buy_quantity` or position check — \
                 calculate the maximum quantity I can {side}.\n\
                 5. Present a trade summary (symbol, side, suggested price, suggested quantity) \
                 and **wait for my explicit confirmation** before proceeding.\n\
                 6. After confirmation, submit a limit order using `submit_order`.\n\n\
                 Do NOT submit any order without my explicit confirmation in step 5."
            ),
        )]
    }

    /// Market overview: major indices, top movers, anomalies, and today's finance calendar events.
    #[prompt(
        name = "market_overview",
        description = "Broad market overview: major indices performance, top gainers and losers, \
                       unusual trading volume, market temperature, and today's finance calendar events. \
                       Optionally specify a market (US, HK, or CN)."
    )]
    async fn market_overview_prompt(
        &self,
        Parameters(args): Parameters<MarketOverviewArgs>,
    ) -> Vec<PromptMessage> {
        let scope = match args.market.as_deref() {
            Some(m) => format!("Focus on the {m} market."),
            None => "Cover the US (NASDAQ/S&P 500), HK (Hang Seng), and CN A-share (CSI 300) markets.".to_string(),
        };
        vec![PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "Give me a comprehensive market overview. {scope}\n\
                 Use the following tools:\n\
                 1. `market_indices` or `quote` — get today's performance for major indices.\n\
                 2. `security_list` with `sort=change_rate` — \
                 retrieve the top 5 gainers and top 5 losers by % change.\n\
                 3. `market_turnover_anomaly` — check for unusual volume or trading anomalies.\n\
                 4. `market_temperature` — get the overall market sentiment/heat indicator.\n\
                 5. `finance_calendar` — list any earnings, dividends, or macro events today.\n\n\
                 Then provide:\n\
                 - One-line summary per index (level, change, % change)\n\
                 - Top 5 gainers and losers with symbol, price, and % change\n\
                 - Any anomalous volume stocks worth watching\n\
                 - Market temperature reading and what it implies\n\
                 - Key calendar events for today and tomorrow"
            ),
        )]
    }
}
