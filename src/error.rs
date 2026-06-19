use rmcp::model::ErrorData as McpError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("longport: {0}")]
    LongPort(Box<longport::Error>),
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl From<longport::Error> for Error {
    fn from(err: longport::Error) -> Self {
        Self::LongPort(Box::new(err))
    }
}

impl Error {
    /// Shorthand for use with `.map_err(Error::longport)`.
    pub fn longport(err: longport::Error) -> Self {
        Self::LongPort(Box::new(err))
    }
}

impl From<Error> for McpError {
    fn from(err: Error) -> Self {
        McpError::internal_error(err.to_string(), None)
    }
}
