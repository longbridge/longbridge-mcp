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

/// `account_channel` from the `sub` claim, which Longbridge encodes as a
/// nested JSON string.
pub fn account_channel(token: &str) -> Option<String> {
    let claims = claims(token)?;
    let sub: Value = serde_json::from_str(claims["sub"].as_str()?).ok()?;
    sub["account_channel"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
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
}
