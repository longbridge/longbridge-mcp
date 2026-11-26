//! Unverified JWT payload inspection.
//!
//! The upstream Longbridge gateway is the authority on every token; this
//! server never verifies signatures. It only *reads* a few claims to make
//! better decisions before a request goes upstream — e.g. rejecting an already
//! expired token with an HTTP 401 the client can act on, instead of letting
//! each tool call fail upstream with `token is expired`. Every reader is
//! fail-open: an opaque or malformed token yields `None` and changes nothing.

use serde_json::Value;

/// Decoded payload claims of a JWT, or `None` if `token` is not a JWT.
pub fn claims(token: &str) -> Option<Value> {
    let payload_b64 = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload_b64)?;
    serde_json::from_slice(&bytes).ok()
}

/// The `exp` claim (seconds since the Unix epoch), if present.
pub fn expires_at(token: &str) -> Option<i64> {
    claims(token)?.get("exp")?.as_i64()
}

/// True when the token carries an `exp` claim that is at or before `now`
/// (seconds since the Unix epoch). A token without `exp` is never "expired".
pub fn is_expired_at(token: &str, now: i64) -> bool {
    expires_at(token).is_some_and(|exp| exp <= now)
}

/// The `sub` claim, which Longbridge encodes as a nested JSON string naming the
/// trading account the token is bound to (`target_aaid`, `account_no`,
/// `account_channel`, `member_id`, ...).
pub fn subject(token: &str) -> Option<Value> {
    let claims = claims(token)?;
    serde_json::from_str(claims["sub"].as_str()?).ok()
}

/// The account channel the token names (e.g. `lb`, `lb_papertrading`), or
/// `None` when the token is not a JWT or names none.
///
/// Read, in order, from the top-level `ac` claim (API-key access tokens, whose
/// `sub` is not JSON), from `account_channel` inside the `sub` claim (OAuth
/// tokens), then from a top-level `account_channel` claim; values are trimmed
/// and empty ones skipped. This matches the Rust and Go SDKs. Callers fall
/// back to `lb`, as the SDKs do.
pub fn account_channel(token: &str) -> Option<String> {
    let token = token.strip_prefix("Bearer ").unwrap_or(token);
    let claims = claims(token)?;
    let non_empty = |v: Option<&Value>| {
        v?.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let from_sub = || {
        let sub: Value = serde_json::from_str(claims.get("sub")?.as_str()?).ok()?;
        non_empty(sub.get("account_channel"))
    };
    non_empty(claims.get("ac"))
        .or_else(from_sub)
        .or_else(|| non_empty(claims.get("account_channel")))
}

/// Minimal base64url decoder (no padding required, no external crate).
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut table = [0xffu8; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[c as usize] = i as u8;
    }
    // base64url uses - and _ instead of + and /
    table[b'-' as usize] = 62;
    table[b'_' as usize] = 63;

    let input: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut i = 0;
    while i < input.len() {
        let get = |pos: usize| -> Option<u8> {
            input.get(pos).and_then(|&b| {
                let v = table[b as usize];
                if v == 0xff { None } else { Some(v) }
            })
        };
        let b0 = get(i)?;
        let b1 = get(i + 1)?;
        out.push((b0 << 2) | (b1 >> 4));
        if let Some(b2) = get(i + 2) {
            out.push((b1 << 4) | (b2 >> 2));
            if let Some(b3) = get(i + 3) {
                out.push((b2 << 6) | b3);
            }
        }
        i += 4;
    }
    Some(out)
}

/// An unsigned JWT with the given JSON payload, for tests.
#[cfg(test)]
pub(crate) fn test_token(payload: &str) -> String {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = payload.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(table[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    format!("eyJhbGciOiJub25lIn0.{out}.sig")
}

/// [`test_token`] with the `hk_m_` region prefix Longbridge puts on the header
/// segment of API-key access tokens, for tests.
#[cfg(test)]
pub(crate) fn test_prefixed_token(payload: &str) -> String {
    format!("hk_m_{}", test_token(payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_exp() {
        let t = test_token(r#"{"exp":1000}"#);
        assert_eq!(expires_at(&t), Some(1000));
        assert!(
            is_expired_at(&t, 1000),
            "a token is expired at its exp second"
        );
        assert!(!is_expired_at(&t, 999), "a token is valid before its exp");
    }

    #[test]
    fn opaque_or_claimless_tokens_are_never_expired() {
        assert!(
            !is_expired_at("opaque-token", i64::MAX),
            "opaque tokens are never treated as expired"
        );
        let t = test_token(r#"{"sub":"x"}"#);
        assert!(
            !is_expired_at(&t, i64::MAX),
            "a JWT without exp is never treated as expired"
        );
    }

    #[test]
    fn reads_account_channel_from_nested_sub() {
        let t = test_token(r#"{"sub":"{\"account_channel\":\"lb_papertrading\"}"}"#);
        assert_eq!(account_channel(&t).as_deref(), Some("lb_papertrading"));
    }

    #[test]
    fn reads_account_channel_from_top_level_ac() {
        let payload = r#"{"iss":"longbridge","sub":"access_token","ak":"example","aaid":1,"ac":"lb_papertrading"}"#;
        for t in [test_token(payload), test_prefixed_token(payload)] {
            assert_eq!(
                account_channel(&t).as_deref(),
                Some("lb_papertrading"),
                "ac claim on {t}"
            );
        }
        assert_eq!(
            account_channel(&format!("Bearer {}", test_prefixed_token(payload))).as_deref(),
            Some("lb_papertrading"),
            "a Bearer prefix is ignored"
        );
    }

    #[test]
    fn account_channel_claim_precedence() {
        let t = test_prefixed_token(
            r#"{"ac":" us ","sub":"{\"account_channel\":\"lb\"}","account_channel":"x"}"#,
        );
        assert_eq!(
            account_channel(&t).as_deref(),
            Some("us"),
            "ac wins, trimmed"
        );
        let t = test_token(
            r#"{"ac":"  ","sub":"{\"account_channel\":\"lb_papertrading\"}","account_channel":"x"}"#,
        );
        assert_eq!(
            account_channel(&t).as_deref(),
            Some("lb_papertrading"),
            "a blank ac falls through to sub"
        );
        let t = test_token(r#"{"sub":"access_token","account_channel":"us_lb"}"#);
        assert_eq!(
            account_channel(&t).as_deref(),
            Some("us_lb"),
            "top-level account_channel is the last resort"
        );
    }

    #[test]
    fn account_channel_absent() {
        let t = test_prefixed_token(r#"{"iss":"longbridge","sub":"access_token"}"#);
        assert_eq!(account_channel(&t), None, "a JWT naming no channel");
        assert_eq!(account_channel("opaque-token"), None, "not a JWT");
    }
}
