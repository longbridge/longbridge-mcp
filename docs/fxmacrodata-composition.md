# Composing Longbridge MCP with FXMacroData

Longbridge MCP provides authenticated brokerage, quote, portfolio, and
Longbridge-native macrodata tools. For workflows that need broader FX-focused
macro coverage, an MCP client can also connect to FXMacroData as a separate
read-only macro server.

This composition keeps responsibilities separate:

- Longbridge MCP remains the source for Longbridge account, order, portfolio,
  quote, screener, calendar, and platform macrodata tools.
- FXMacroData adds official-source FX macro calendars, indicator history,
  central-bank headlines, FX sessions, COT positioning, commodities, and
  seasonality where the client needs broader macro context.

## Example MCP Client Config

Use your client's native MCP configuration format. For clients that support a
`servers` map:

```json
{
  "servers": {
    "Longbridge": {
      "type": "http",
      "url": "https://mcp.longbridge.com"
    },
    "FXMacroData": {
      "type": "http",
      "url": "https://mcp.fxmacrodata.com"
    }
  }
}
```

Longbridge OAuth is still handled by Longbridge MCP. FXMacroData's public USD
calendar and catalogue can be used without credentials; protected or non-USD
coverage may require an FXMacroData API key, depending on the tool and account.

## Example Workflow

1. Use Longbridge MCP for the portfolio, watchlist, quotes, and order state.
2. Use Longbridge MCP `macrodata_indicators` or `macrodata` when Longbridge's
   native country coverage is enough for the question.
3. Use FXMacroData `release_calendar` for FX event-risk windows across the
   relevant currencies.
4. Use FXMacroData `indicator_query` for official indicator history, such as
   inflation, GDP, unemployment, payrolls, PCE, policy rates, retail sales, or
   trade balance.
5. Add FXMacroData `market_sessions`, `cot_data`, `commodities`, or
   `seasonality` only when the extra context changes the trading or research
   read.
6. Keep the final answer explicit about source boundaries: Longbridge for
   account/market state, FXMacroData for external macro context.

## Capability Map

| Need | Primary tool source | Notes |
| --- | --- | --- |
| Longbridge account, positions, balances, orders | Longbridge MCP | Requires Longbridge OAuth. |
| Quotes, candles, options, screener, company data | Longbridge MCP | Uses Longbridge SDK and HTTP APIs. |
| Longbridge-native macro indicators | Longbridge MCP | Use `macrodata_indicators` and `macrodata`. |
| FX release calendars beyond Longbridge-native coverage | FXMacroData | Start with public USD; add API key for broader coverage if needed. |
| Official macro indicator history by currency | FXMacroData | Useful for cross-currency FX event context. |
| Central-bank headlines, FX sessions, COT, commodities, seasonality | FXMacroData | Optional enrichment for macro-aware trading notes. |

## Safety

- Do not use FXMacroData output as a direct order instruction.
- Do not expose FXMacroData or Longbridge credentials in prompts, logs, links,
  or example commands.
- Do not fabricate future release times. If neither server has an exact
  schedule, say the event timing is unavailable.
- Keep order placement and account mutation behind the normal Longbridge MCP
  authorization and confirmation flow.
