//! The execution gate shared by every money-moving tool.
//!
//! `submit_order` / `cancel_order` / `replace_order` and the grid writes
//! (`grid_submit` / `grid_replace` / `grid_cancel` / `grid_suspend` /
//! `grid_restart`) preview by default and act only when the caller quotes back
//! the three-digit `confirmation_code` the preview returned.
//!
//! # Derived, not stored
//!
//! This server holds no persistent state and runs as any number of instances
//! behind a load balancer, so a code remembered by one process is invisible to
//! the next request: a dry run served by one instance and an execute served by
//! another would fail intermittently, for no reason the user could see.
//!
//! So the code is *recomputed* rather than remembered — three digits off a
//! digest of the canonical request, and nothing else. No clock, no stored
//! state, no per-instance value: every instance derives the same answer from
//! the same request, with no shared store and nothing to configure.
//!
//! # What it is for
//!
//! It catches the ordinary mistake: acting on an order nobody previewed, or on
//! a *different* order than the one previewed. Change the price after reading
//! the code and it stops matching, which is the case worth catching — the user
//! approved one order and a different one was about to be sent.
//!
//! It is not a secret. The request is the caller's own and this file is public,
//! so a caller determined to skip the preview could compute a code instead of
//! asking for one. Guarding against that would need a server-side secret, and
//! its cost — a value every instance must be given, and that silently breaks
//! the gate when they disagree — buys nothing against the mistake this exists
//! to catch. Enforcing that a *human* saw the preview is a different problem
//! again, and needs a control outside this process such as a client-side
//! approval hook.
//!
//! # Tolerant on purpose
//!
//! Inputs are canonicalised before hashing, so a code survives the harmless
//! rewordings a caller makes between two calls: `400` and `400.00` are the same
//! price, `buy` and `Buy` the same side, `700.hk` and `700.HK` the same symbol.
//! A confirmation that fails on a difference the user cannot see is worse than
//! no confirmation at all — it teaches people to distrust the gate.

use longbridge::Decimal;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use sha2::{Digest, Sha256};
use std::str::FromStr;

/// Canonical form of one field.
///
/// `Decimal` first: it collapses `400`, `400.0`, `400.00`, `+400`, `0400` and
/// `4e2`, which is where two calls differ most often. Anything that is not a
/// number is trimmed and upper-cased — symbols, sides and order types are all
/// matched case-insensitively upstream, so treating them as distinct here would
/// reject an order the exchange considers identical.
fn canonical(field: &str) -> String {
    let trimmed = field.trim();
    // `from_str` rejects exponent form, which JSON encoders do emit for very
    // small or very large values (a crypto price as `1e-8`), so try
    // `from_scientific` before concluding it is not a number.
    Decimal::from_str(trimmed)
        .or_else(|_| Decimal::from_scientific(trimmed))
        .map_or_else(|_| trimmed.to_uppercase(), |n| n.normalize().to_string())
}

/// The canonical description of what a confirmation code covers.
///
/// Every gated tool funnels through one of the constructors below, so a call
/// site cannot get the field order wrong, and so a mismatch can be explained by
/// showing two strings rather than two opaque digests.
///
/// The fields are deliberately few — the symbol, the side, the size and the
/// price are what a user reads off a preview and what a wrong order would get
/// wrong. Folding in the optional extras would buy very little and risk the
/// failure this gate can least afford: a caller that drops `remark` on the
/// second call being told its code is wrong, for a difference nobody can see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope(String);

impl Scope {
    /// `buy 100 700.HK @ 400` — a new order.
    ///
    /// `side` earns its place despite the rule above: it cannot drift between
    /// two calls that mean the same thing, while leaving it out would let a
    /// code previewed for a buy execute a sell.
    pub fn order(side: &str, symbol: &str, quantity: &str, price: &str) -> Self {
        let price = canonical(price);
        Self(format!(
            "{} {} {} @ {}",
            canonical(side).to_lowercase(),
            canonical(quantity),
            canonical(symbol),
            if price.is_empty() { "market" } else { &price },
        ))
    }

    /// `cancel order 20240101-1` — an action on an order that already exists.
    pub fn on_order(action: &str, order_id: &str) -> Self {
        Self(format!("{} order {}", action, canonical(order_id)))
    }

    /// `replace order 20240101-1 to 200 @ 255` — a change to an existing order.
    pub fn replace(order_id: &str, quantity: &str, price: &str) -> Self {
        let price = canonical(price);
        Self(format!(
            "replace order {} to {} @ {}",
            canonical(order_id),
            canonical(quantity),
            if price.is_empty() {
                "unchanged"
            } else {
                &price
            },
        ))
    }

    /// `grid submit 100 700.HK @ 449` — a grid strategy.
    pub fn grid(action: &str, symbol: &str, quantity: &str, base_price: &str) -> Self {
        Self(format!(
            "grid {} {} {} @ {}",
            action,
            canonical(quantity),
            canonical(symbol),
            canonical(base_price),
        ))
    }

    /// The three-digit code this scope is confirmed by.
    pub fn code(&self) -> String {
        let mut hasher = Sha256::new();
        // Domain separator: this digest must never coincide with any other use
        // of the same text elsewhere in the server.
        hasher.update(b"longbridge-mcp/execute-confirmation/v1");
        hasher.update(self.0.as_bytes());
        let digest = hasher.finalize();
        let n = (u32::from(digest[0]) << 8) | u32::from(digest[1]);
        format!("{:03}", n % 1000)
    }

    /// Check the code the caller quoted back.
    ///
    /// The only way to fail is to quote a code for a *different* order. Leading
    /// zeros and stray whitespace are forgiven — a code that arrives as `7`
    /// after a JSON round-trip is still the code this order was given. The
    /// canonical text goes into the error so a mismatch can be read rather than
    /// guessed at.
    pub fn verify(&self, code: &str) -> Result<(), McpError> {
        let trimmed = code.trim();
        let normalized = trimmed
            .parse::<u32>()
            .map_or_else(|_| trimmed.to_string(), |n| format!("{:03}", n % 1000));
        if normalized == self.code() {
            return Ok(());
        }
        Err(McpError::invalid_params(
            format!(
                "Confirmation code {trimmed} does not match this request ({self}). Call this \
                 tool again without `execute`, show the returned preview to the user, and \
                 only then re-call with the `confirmation_code` that preview returns."
            ),
            None,
        ))
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The standard dry-run envelope: `dry_run` distinguishes it from a real
/// result, `preview` carries what would have been sent, and
/// `confirmation_code` is what the caller must quote back.
pub fn result(scope: &Scope, preview: serde_json::Value) -> Result<CallToolResult, McpError> {
    let code = scope.code();
    crate::tools::tool_json(&serde_json::json!({
        "dry_run": true,
        "preview": preview,
        "confirmation_code": code,
        "next_step": format!(
            "DRY RUN — nothing was sent to the exchange. Show this preview to the user and \
             ask them to confirm it. Only call this tool again with execute=\"{code}\" after \
             the user has explicitly confirmed this exact order. The code applies only to this \
             exact request. Never quote it back on your own initiative."
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One order, written every way a caller might reasonably send it.
    ///
    /// A false rejection here is the failure this gate can least afford: the
    /// user sees the order they approved, the server insists the code is wrong,
    /// and the next thing the model learns is to stop trusting the gate. So the
    /// bar is that every spelling below produces one scope, and one code.
    fn same_order() -> Vec<Scope> {
        [
            ("Buy", "700.HK", "100", "400"),
            // Trailing zeros a JSON encoder or a caller adds.
            ("Buy", "700.HK", "100", "400.0"),
            ("Buy", "700.HK", "100", "400.00"),
            ("Buy", "700.HK", "100.00", "400"),
            // Signs and leading zeros that survive a round trip through a number.
            ("Buy", "700.HK", "100", "+400"),
            ("Buy", "700.HK", "0100", "0400"),
            // Exponent form, which JSON encoders emit for some values.
            ("Buy", "700.HK", "100", "4e2"),
            ("Buy", "700.HK", "1e2", "400"),
            // Case, on the symbol and on the side.
            ("buy", "700.HK", "100", "400"),
            ("BUY", "700.hk", "100", "400"),
            // Whitespace.
            (" Buy ", " 700.HK ", " 100 ", " 400 "),
        ]
        .into_iter()
        .map(|(side, symbol, qty, price)| Scope::order(side, symbol, qty, price))
        .collect()
    }

    /// Prices whose decimal places must survive, because dropping or adding one
    /// is a hundredfold error. Each pair is the same number written twice.
    fn same_fractional_price() -> Vec<(Scope, Scope)> {
        [
            ("0.5", "0.50"),
            ("0.5", ".5"),
            ("500.2", "500.200"),
            ("0.00000001", "1e-8"),
            ("1234.5678", "1234.56780"),
        ]
        .into_iter()
        .map(|(a, b)| {
            (
                Scope::order("buy", "700.HK", "100", a),
                Scope::order("buy", "700.HK", "100", b),
            )
        })
        .collect()
    }

    /// Orders that really are different, and so must not share a code.
    fn different_orders() -> Vec<Scope> {
        vec![
            Scope::order("Buy", "700.HK", "100", "410"),   // price
            Scope::order("Buy", "700.HK", "100", "400.1"), // price, one decimal place out
            Scope::order("Buy", "700.HK", "100", "40"),    // price, decimal point moved
            Scope::order("Buy", "700.HK", "200", "400"),   // quantity
            Scope::order("Buy", "9988.HK", "100", "400"),  // symbol
            Scope::order("Sell", "700.HK", "100", "400"),  // side
            Scope::order("Buy", "700.HK", "100", ""),      // limit dropped for a market order
            Scope::on_order("cancel", "700"),              // a different kind of action entirely
            Scope::replace("700", "100", "400"),
            Scope::grid("submit", "700.HK", "100", "400"),
        ]
    }

    #[test]
    fn a_code_is_always_three_digits() {
        for scope in same_order().into_iter().chain(different_orders()) {
            let code = scope.code();
            assert_eq!(code.len(), 3, "{scope} gave {code}");
            assert!(
                code.chars().all(|c| c.is_ascii_digit()),
                "{scope} gave {code}"
            );
        }
    }

    #[test]
    fn every_spelling_of_one_order_normalises_to_one_scope() {
        let all = same_order();
        let baseline = &all[0];
        assert_eq!(baseline.to_string(), "buy 100 700.HK @ 400");
        for scope in &all {
            assert_eq!(scope, baseline, "{scope} must normalise to {baseline}");
            assert!(
                scope.verify(&baseline.code()).is_ok(),
                "{scope} must verify"
            );
        }
    }

    #[test]
    fn fractional_prices_survive_normalisation() {
        // Trailing-zero stripping must not become decimal-point moving.
        for (a, b) in same_fractional_price() {
            assert_eq!(a, b, "{a} and {b} are the same price");
            assert!(b.verify(&a.code()).is_ok());
        }
    }

    #[test]
    fn the_code_a_preview_returns_always_verifies() {
        for scope in same_order().into_iter().chain(different_orders()) {
            assert!(
                scope.verify(&scope.code()).is_ok(),
                "{scope} must accept its own code"
            );
        }
    }

    #[test]
    fn a_different_order_does_not_inherit_the_code() {
        let approved = same_order()[0].code();
        for scope in different_orders() {
            assert!(
                scope.verify(&approved).is_err(),
                "{scope} must not accept the baseline code"
            );
        }
    }

    #[test]
    fn a_mangled_code_is_still_recognised() {
        // How a code comes back after a JSON round-trip that ate the leading
        // zeros, or a model that padded it with spaces.
        let scope = same_order().remove(0);
        let code = scope.code();
        let unpadded = code.trim_start_matches('0');
        let unpadded = if unpadded.is_empty() { "0" } else { unpadded };
        for quoted in [
            code.clone(),
            format!(" {code} "),
            unpadded.to_string(),
            format!(" {unpadded} "),
        ] {
            assert!(
                scope.verify(&quoted).is_ok(),
                "code {code} quoted as {quoted:?}"
            );
        }
    }

    #[test]
    fn nonsense_in_the_code_slot_is_rejected_without_panicking() {
        // The one thing that must never happen is a crash on the path that
        // decides whether real money moves.
        let scope = same_order().remove(0);
        for quoted in ["", " ", "abc", "-1", "1000", "99999999999999999999", "4.7"] {
            let _ = scope.verify(quoted);
        }
    }

    #[test]
    fn a_mismatch_explains_itself() {
        // A code that fails must say what this request actually is, or the
        // model has no way to see which of the two orders is the odd one.
        let err = Scope::order("buy", "700.HK", "100", "410")
            .verify(&Scope::order("buy", "700.HK", "100", "400").code())
            .expect_err("must reject");
        assert!(
            format!("{err:?}").contains("buy 100 700.HK @ 410"),
            "{err:?}"
        );
    }

    #[test]
    fn a_missing_price_reads_as_market_not_as_nothing() {
        assert_eq!(
            Scope::order("buy", "700.HK", "100", "").to_string(),
            "buy 100 700.HK @ market"
        );
    }

    #[test]
    fn every_distinct_order_gets_a_distinct_scope() {
        let mut seen = std::collections::HashSet::new();
        for scope in different_orders() {
            assert!(seen.insert(scope.to_string()), "{scope} collided");
        }
        assert!(seen.insert(same_order()[0].to_string()));
    }
}
