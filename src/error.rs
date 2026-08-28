use rmcp::model::ErrorData as McpError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("longbridge: {}", sanitize_longbridge_error(.0))]
    Longbridge(Box<longbridge::Error>),
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Catch-all for error text this crate already controls (e.g. a
    /// `serde_json`/`time` formatting failure). Do NOT build this from a
    /// `longbridge::Error`/`HttpClientError` via `.to_string()` — that skips
    /// [`sanitize_longbridge_error`] and can leak upstream response content
    /// (stack traces, gateway internals, a mismatched field's raw value)
    /// into both the ops log and, more consequentially, `tool_error()`'s
    /// client-facing text. Use `Error::longbridge(e.into())` instead; this
    /// exact mistake has recurred at call sites across this crate before.
    #[error("{0}")]
    Other(String),
}

/// Renders a `longbridge` SDK error for display — to the ops log, to
/// `tool_error()`'s client-facing text, or to any other place a `longbridge`
/// error's message ends up (e.g. `calendar.rs`'s `partial_reason`, which is
/// not even an error path). Most variants' `Display` is already short,
/// structured text (e.g. `"openapi error: code=... message=..."`, the SDK's
/// own business-error format), safe to pass through. Two variants aren't:
///
/// - `HttpClientError::UnexpectedHttpResponse` embeds the raw upstream HTTP
///   response body verbatim — an unparsed gateway error page, a backend
///   stack trace, or worse.
/// - `HttpClientError::DeserializeResponseBody` wraps a `serde_json::Error`
///   whose `Display` embeds the actual offending JSON value verbatim, with
///   no length cap (verified: a 500-character value came through in full).
///   For the envelope-level parse failure specifically, that "value" can be
///   the entire response body.
///
/// Both are stripped at the source, in the one place every `longbridge`
/// error should be rendered from, rather than trusting every call site that
/// handles a `longbridge`/`HttpClientError` directly to remember to route
/// through here. (`OAuth`/`SerializeRequestBody`/`Sse` construct their text
/// from a locally-raised error — token acquisition, our own outgoing body,
/// or an SSE transport error — not upstream response content, so they're
/// left as-is; they haven't been audited as rigorously as the two variants
/// above, though, so treat that as "not yet found unsafe" rather than
/// "verified safe.")
pub(crate) fn sanitize_longbridge_error(err: &longbridge::Error) -> String {
    use longbridge::httpclient::HttpClientError;
    match err {
        longbridge::Error::HttpClient(HttpClientError::UnexpectedHttpResponse {
            status,
            trace_id,
            ..
        }) => format!(
            "unexpected HTTP response: status={status}, trace_id={trace_id} (body omitted — unparseable as an OpenAPI response, likely a gateway or proxy error)"
        ),
        longbridge::Error::HttpClient(HttpClientError::DeserializeResponseBody(_)) => {
            "response body did not match the expected shape (details omitted — the \
             deserialize error can embed the raw offending value)"
                .to_string()
        }
        _ => err.to_string(),
    }
}

impl From<longbridge::Error> for Error {
    fn from(err: longbridge::Error) -> Self {
        Self::Longbridge(Box::new(err))
    }
}

impl Error {
    /// Shorthand for use with `.map_err(Error::longbridge)`.
    pub fn longbridge(err: longbridge::Error) -> Self {
        Self::Longbridge(Box::new(err))
    }
}

impl From<Error> for McpError {
    fn from(err: Error) -> Self {
        McpError::internal_error(err.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unexpected_http_response(body: &str) -> longbridge::Error {
        longbridge::Error::HttpClient(
            longbridge::httpclient::HttpClientError::UnexpectedHttpResponse {
                status: reqwest::StatusCode::BAD_GATEWAY,
                trace_id: "trace-123".to_string(),
                headers: Box::new(reqwest::header::HeaderMap::new()),
                body: body.to_string(),
            },
        )
    }

    #[test]
    fn unexpected_http_response_omits_the_raw_body() {
        let err = unexpected_http_response(
            "<html><body>500 Internal Server Error — stack trace: at db.query(secrets.rs:42)</body></html>",
        );
        let rendered = sanitize_longbridge_error(&err);
        assert!(
            !rendered.contains("stack trace") && !rendered.contains("secrets.rs"),
            "raw body leaked into rendered message: {rendered}"
        );
        assert!(
            rendered.contains("502") && rendered.contains("trace-123"),
            "status and trace_id must still be present for triage: {rendered}"
        );
    }

    #[test]
    fn deserialize_response_body_omits_the_embedded_value() {
        // serde_json's own message for a type mismatch embeds the actual
        // offending value verbatim, uncapped — this is the exact string
        // shape that construction produces.
        let err = longbridge::Error::HttpClient(
            longbridge::httpclient::HttpClientError::DeserializeResponseBody(
                "invalid type: string \"SECRET_TOKEN_ABCDEF123456\", expected u64 at line 1 column 37"
                    .to_string(),
            ),
        );
        let rendered = sanitize_longbridge_error(&err);
        assert!(
            !rendered.contains("SECRET_TOKEN_ABCDEF123456"),
            "embedded value leaked into rendered message: {rendered}"
        );
    }

    #[test]
    fn other_variants_pass_through_their_own_short_display() {
        let err = longbridge::Error::HttpClient(longbridge::httpclient::HttpClientError::OpenApi {
            code: 401102,
            message: "token verification failed".to_string(),
            trace_id: "trace-456".to_string(),
        });
        let rendered = sanitize_longbridge_error(&err);
        assert!(
            rendered.contains("token verification failed"),
            "non-UnexpectedHttpResponse variants must pass through unsanitized: {rendered}"
        );
    }

    #[test]
    fn error_longbridge_display_and_mcp_conversion_use_the_sanitized_text() {
        let err = Error::longbridge(unexpected_http_response("<html>leaked</html>"));
        assert!(
            !err.to_string().contains("leaked"),
            "Error::Longbridge's own Display must route through the sanitizer"
        );
        let mcp: McpError = err.into();
        assert!(
            !mcp.message.contains("leaked"),
            "the client-facing McpError message must never contain the raw upstream body"
        );
    }
}
