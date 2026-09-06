//! Error and result types for the crate.

use std::fmt;

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Anything that can go wrong while talking to the MetaCPAN API.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The configured base URL, or a path joined onto it, was not a valid URL.
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),

    /// The HTTP request could not be completed (DNS, TLS, timeout, connection
    /// reset, and similar transport-level failures).
    #[error("http transport error: {0}")]
    Http(#[from] reqwest::Error),

    /// The request reached the API but it responded with a non-success status.
    ///
    /// MetaCPAN reports problems as a small JSON document with `code` and
    /// `message` fields; when that shape is present it is parsed out here,
    /// otherwise [`ApiError::message`] holds the raw response body.
    #[error("metacpan api error: {0}")]
    Api(ApiError),

    /// A filesystem error while reading from or clearing the on-disk response
    /// cache (see [`ClientBuilder::cache_dir`](crate::ClientBuilder::cache_dir)).
    #[error("cache i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A response body could not be deserialized into the expected type.
    #[error("could not decode response from {path}: {source}")]
    Decode {
        /// The request path that produced the undecodable body.
        path: String,
        /// The underlying serde error.
        #[source]
        source: serde_json::Error,
    },

    /// A request body could not be serialized to JSON (only the `POST`
    /// endpoints — [`Client::search`](crate::Client::search) and
    /// [`Client::post_json`](crate::Client::post_json) — send one).
    #[error("could not serialize request body: {0}")]
    EncodeBody(#[source] serde_json::Error),
}

impl Error {
    /// The HTTP status code, when this error carries one.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Api(e) => Some(e.code),
            Error::Http(e) => e.status().map(|s| s.as_u16()),
            _ => None,
        }
    }

    /// `true` when the API responded `404 Not Found` (nothing matched the
    /// requested author, release, module, ...).
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(404)
    }
}

/// The body of a MetaCPAN error response.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[non_exhaustive]
pub struct ApiError {
    /// HTTP-style status code echoed in the JSON body.
    pub code: u16,
    /// Human readable description of the failure.
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for ApiError {}
