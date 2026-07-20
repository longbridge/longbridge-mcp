//! Helpers for routing MCP tools to US-market SDK endpoints based on the
//! calling account's DC region and, for symbol-keyed fundamental tools, the
//! symbol's market suffix.

/// True when `symbol` is a US-listed equity/ETF symbol, e.g. "AAPL.US".
/// Case-insensitive on the suffix.
pub fn is_us_symbol(symbol: &str) -> bool {
    symbol
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("US"))
}

/// True when `symbol` is a US-DC crypto symbol, e.g. "BTCUSD.BKKT".
/// Case-insensitive on the suffix. `.HAS`/`.OSL` crypto symbols stay on the
/// existing HK-region static-info path and are not matched here.
pub fn is_us_crypto_symbol(symbol: &str) -> bool {
    symbol
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("BKKT"))
}

/// True when a US-region account is asking about a US symbol — the gate used
/// by every fundamental tool that has a US-specific SDK method.
pub async fn is_us_fundamental(mctx: &crate::tools::McpContext, symbol: &str) -> bool {
    is_us_symbol(symbol) && mctx.dc_region().await == longbridge::DcRegion::Us
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_us_symbol_matches_us_suffix_case_insensitively() {
        assert!(is_us_symbol("AAPL.US"));
        assert!(is_us_symbol("aapl.us"));
        assert!(!is_us_symbol("700.HK"));
        assert!(!is_us_symbol("AAPL"));
    }

    #[test]
    fn is_us_crypto_symbol_matches_only_bkkt() {
        assert!(is_us_crypto_symbol("BTCUSD.BKKT"));
        assert!(is_us_crypto_symbol("btcusd.bkkt"));
        assert!(!is_us_crypto_symbol("BTCUSD.HAS"));
        assert!(!is_us_crypto_symbol("BTCUSD.OSL"));
        assert!(!is_us_crypto_symbol("AAPL.US"));
    }
}
