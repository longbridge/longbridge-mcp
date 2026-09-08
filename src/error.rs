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

    /// The upstream OpenAPI business error code, when this wraps a
    /// `longbridge::Error` that carries one. Lets callers (e.g.
    /// `error_hint()`) classify an error by its structured code instead of
    /// pattern-matching the sanitized display text, which is fragile (a
    /// short numeric code can appear as a substring of an unrelated field).
    pub fn openapi_error_code(&self) -> Option<i64> {
        match self {
            Self::Longbridge(err) => err.openapi_error_code(),
            Self::Serialize(_) | Self::Http(_) | Self::Io(_) | Self::Other(_) => None,
        }
    }

    /// The bare upstream error message, without the SDK's structured/`Debug`
    /// wrapper — e.g. `"no quote access"` rather than
    /// `"response error: 7: detail:Some(WsResponseErrorDetail { code: 301604, msg: \"no quote access\" })"`.
    /// Used for the client-facing envelope `message`; the full wrapped form is
    /// still kept in the ops log (via the `Display` used for `McpError::message`).
    /// `None` when no cleaner form exists — the caller falls back to the display
    /// text.
    pub fn clean_message(&self) -> Option<String> {
        use longbridge::httpclient::HttpClientError;
        use longbridge::wsclient::WsClientError;

        match self {
            Self::Longbridge(err) => match err.as_ref() {
                longbridge::Error::WsClient(WsClientError::ResponseError {
                    detail: Some(detail),
                    ..
                }) => Some(detail.msg.clone()),
                longbridge::Error::HttpClient(HttpClientError::OpenApi { message, .. }) => {
                    Some(message.clone())
                }
                _ => None,
            },
            Self::Serialize(_) | Self::Http(_) | Self::Io(_) | Self::Other(_) => None,
        }
    }

    /// The restricted path and required/current data centers, when this
    /// wraps a `longbridge::httpclient::HttpClientError::DcRegionRestricted`.
    /// Same rationale as [`Self::openapi_error_code`]: lets `error_hint()`
    /// match structurally instead of on the Display text.
    pub fn dc_region_restricted(
        &self,
    ) -> Option<(&str, longbridge::DcRegion, longbridge::DcRegion)> {
        use longbridge::httpclient::HttpClientError;

        match self {
            Self::Longbridge(err) => match err.as_ref() {
                longbridge::Error::HttpClient(HttpClientError::DcRegionRestricted {
                    path,
                    required,
                    current,
                }) => Some((path.as_str(), *required, *current)),
                _ => None,
            },
            Self::Serialize(_) | Self::Http(_) | Self::Io(_) | Self::Other(_) => None,
        }
    }
}

impl From<Error> for McpError {
    fn from(err: Error) -> Self {
        let mut data = serde_json::Map::new();
        if let Some(code) = err.openapi_error_code() {
            data.insert("openapi_error_code".to_string(), serde_json::json!(code));
        }
        if let Some(message) = err.clean_message() {
            data.insert("upstream_message".to_string(), serde_json::json!(message));
        }
        if let Some((path, required, current)) = err.dc_region_restricted() {
            data.insert(
                "dc_region_restricted".to_string(),
                serde_json::json!({
                    "path": path,
                    "required": required.as_str(),
                    "current": current.as_str(),
                }),
            );
        }
        let data = (!data.is_empty()).then(|| serde_json::Value::Object(data));
        McpError::internal_error(err.to_string(), data)
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

    #[test]
    fn openapi_error_code_surfaces_the_business_code() {
        let err = Error::longbridge(longbridge::Error::HttpClient(
            longbridge::httpclient::HttpClientError::OpenApi {
                code: 301604,
                message: "no quote access".to_string(),
                trace_id: "trace-789".to_string(),
            },
        ));
        assert_eq!(
            err.openapi_error_code(),
            Some(301604),
            "the business error code must be extractable without parsing the display text"
        );
    }

    #[test]
    fn mcp_conversion_populates_the_structured_error_code() {
        let err = Error::longbridge(longbridge::Error::HttpClient(
            longbridge::httpclient::HttpClientError::OpenApi {
                code: 301604,
                message: "no quote access".to_string(),
                trace_id: "trace-789".to_string(),
            },
        ));
        let mcp: McpError = err.into();
        assert_eq!(
            mcp.data.as_ref().and_then(|d| d.get("openapi_error_code")),
            Some(&serde_json::json!(301604)),
            "McpError::data must carry the structured code for error_hint() to match on"
        );
    }

    #[test]
    fn non_longbridge_errors_have_no_structured_code() {
        let err = Error::Other("something went wrong".to_string());
        assert_eq!(
            err.openapi_error_code(),
            None,
            "errors not wrapping a longbridge::Error have no business code to surface"
        );
    }

    #[test]
    fn dc_region_restricted_surfaces_the_structured_fields() {
        let err = Error::longbridge(longbridge::Error::HttpClient(
            longbridge::httpclient::HttpClientError::DcRegionRestricted {
                path: "/v1/us/foo".to_string(),
                required: longbridge::DcRegion::Us,
                current: longbridge::DcRegion::Ap,
            },
        ));
        let (path, required, current) = err
            .dc_region_restricted()
            .expect("DcRegionRestricted must be extractable without parsing the display text");
        assert_eq!(path, "/v1/us/foo");
        assert_eq!(required, longbridge::DcRegion::Us);
        assert_eq!(current, longbridge::DcRegion::Ap);
    }

    #[test]
    fn mcp_conversion_populates_the_structured_dc_region_fields() {
        let err = Error::longbridge(longbridge::Error::HttpClient(
            longbridge::httpclient::HttpClientError::DcRegionRestricted {
                path: "/v1/us/foo".to_string(),
                required: longbridge::DcRegion::Us,
                current: longbridge::DcRegion::Ap,
            },
        ));
        let mcp: McpError = err.into();
        let dc = mcp
            .data
            .as_ref()
            .and_then(|d| d.get("dc_region_restricted"))
            .expect("McpError::data must carry the structured DC-region fields");
        assert_eq!(dc.get("path").and_then(|v| v.as_str()), Some("/v1/us/foo"));
        assert_eq!(dc.get("required").and_then(|v| v.as_str()), Some("us"));
        assert_eq!(dc.get("current").and_then(|v| v.as_str()), Some("ap"));
    }
}
