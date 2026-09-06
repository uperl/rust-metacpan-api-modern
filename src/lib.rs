//! A modern, asynchronous Rust interface to the [MetaCPAN] HTTP API
//! (<https://api.metacpan.org/>, served today from
//! <https://fastapi.metacpan.org/v1/>).
//!
//! The API is a thin REST layer over an Elasticsearch cluster of CPAN
//! metadata. This crate wraps the endpoints that have stable, document-shaped
//! responses in typed methods, and exposes the full Elasticsearch query DSL
//! for everything else.
//!
//! # Quick start
//!
//! ```no_run
//! # async fn run() -> Result<(), metacpan_api_modern::Error> {
//! use metacpan_api_modern::{Client, PodFormat};
//!
//! let mc = Client::new();
//!
//! // Look up an author.
//! let author = mc.author("PLICEASE").await?;
//! println!("{} <{}>", author.name.unwrap_or_default(), author.email.join(", "));
//!
//! // Latest release of a distribution.
//! let release = mc.release("FFI-Platypus").await?;
//! println!("{} {}", release.distribution.unwrap_or_default(), release.version.unwrap_or_default());
//!
//! // Resolve a module to the archive that provides it.
//! let dl = mc.download_url("FFI::Platypus").await?;
//! println!("{}", dl.download_url.unwrap_or_default());
//!
//! // Rendered documentation.
//! let pod = mc.pod("FFI::Platypus", PodFormat::Plain).await?;
//! println!("{}", &pod[..pod.len().min(200)]);
//!
//! // PAUSE upload permissions for a module.
//! let perm = mc.permission("FFI::Platypus").await?;
//! println!("owner {:?}, co-maint {:?}", perm.owner, perm.co_maintainers);
//! # Ok(())
//! # }
//! ```
//!
//! # Search
//!
//! [`Client::search`] posts a raw Elasticsearch query DSL body and
//! deserializes each hit's `_source` into a type you choose:
//!
//! ```no_run
//! # async fn run() -> Result<(), metacpan_api_modern::Error> {
//! use metacpan_api_modern::{Client, Release, SearchResponse};
//! use serde_json::json;
//!
//! let mc = Client::new();
//! let recent: SearchResponse<Release> = mc
//!     .search("release", &json!({
//!         "query": { "bool": { "must": [
//!             { "term": { "author": "PLICEASE" } },
//!             { "term": { "status": "latest" } }
//!         ]}},
//!         "size": 10,
//!         "sort": [{ "date": "desc" }]
//!     }))
//!     .await?;
//!
//! for release in recent.sources() {
//!     println!("{:?}", release.name);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Configuration
//!
//! [`Client::new`] targets the public production API. Use [`Client::builder`]
//! to change the base URL, `User-Agent`, or timeout, or to supply your own
//! [`reqwest::Client`]:
//!
//! ```no_run
//! # fn run() -> Result<(), metacpan_api_modern::Error> {
//! use std::time::Duration;
//!
//! let mc = metacpan_api_modern::Client::builder()
//!     .user_agent("my-app/1.0 (https://example.com)")
//!     .timeout(Duration::from_secs(30))
//!     .build()?;
//! # let _ = mc;
//! # Ok(())
//! # }
//! ```
//!
//! To avoid refetching the same data, point the client at a cache directory.
//! Every `GET` is then served from disk until its entry is older than the
//! time-to-live (one hour by default); `POST` searches are never cached:
//!
//! ```no_run
//! # fn run() -> Result<(), metacpan_api_modern::Error> {
//! use std::time::Duration;
//!
//! let mc = metacpan_api_modern::Client::builder()
//!     .cache_dir("/tmp/metacpan-cache")
//!     .cache_ttl(Duration::from_secs(24 * 60 * 60))
//!     .build()?;
//! # let _ = mc;
//! # Ok(())
//! # }
//! ```
//!
//! [MetaCPAN]: https://metacpan.org/

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod client;
mod error;
pub mod types;

pub use client::{
    Client, ClientBuilder, DEFAULT_BASE_URL, DEFAULT_CACHE_TTL, DEFAULT_USER_AGENT,
    DownloadUrlQuery, PodFormat,
};
pub use error::{ApiError, Error, Result};

#[doc(inline)]
pub use types::{
    Author, Changes, Dependency, Distribution, DownloadUrl, File, Hit, Hits, Mirror, ModuleInfo,
    Permission, Profile, Release, ReleaseCount, Resources, River, SearchResponse, Stat,
    TestSummary, Total,
};

// Re-export `reqwest` so downstream crates can name its types (for
// `ClientBuilder::http_client`) without a direct dependency or version skew.
pub use reqwest;
