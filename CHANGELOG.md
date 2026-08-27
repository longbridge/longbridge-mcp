# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Fixed

- `history_candlesticks_by_date` and `history_candlesticks_by_offset`: `trade_sessions` parameter is now optional (defaults to `"all"`). Previously, omitting it caused a deserialization error. Case-insensitive matching added for `"All"`, `"INTRADAY"`, etc.
- `finance_calendar`: `category` parameter is now case-insensitive (`"Report"` and `"REPORT"` are accepted in addition to `"report"`). Invalid values now return a helpful error listing all valid options.
- `institution_rating`: Switched to partial-success mode using `tokio::join!`. If one sub-request fails, the result still returns the successful data with a `warnings` field instead of failing entirely. Both requests must fail to return an error.
- `stock_positions`: `us_asset_overview` failure is now explicitly surfaced in a `warnings` field instead of being silently ignored. The main positions response is still returned successfully.
