//! The asynchronous [`Client`] and its [`ClientBuilder`].

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;
use url::Url;

use crate::error::{ApiError, Error, Result};
use crate::types::{
    Author, Changes, Distribution, DownloadUrl, File, Mirror, MirrorList, Release, ReleaseEnvelope,
    SearchResponse,
};

/// Base URL of the public production MetaCPAN API.
///
/// This is the modern ("fastapi") host; the older `api.metacpan.org` name
/// redirects here. The trailing slash matters: paths are joined onto it.
pub const DEFAULT_BASE_URL: &str = "https://fastapi.metacpan.org/v1/";

/// Default `User-Agent` sent with every request.
pub const DEFAULT_USER_AGENT: &str =
    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// An asynchronous client for the MetaCPAN HTTP API.
///
/// A `Client` owns a [`reqwest::Client`], so it holds a connection pool and is
/// cheap to [`clone`](Clone::clone); share one across your application rather
/// than constructing them ad hoc.
///
/// ```no_run
/// # async fn run() -> Result<(), metacpan_api_modern::Error> {
/// let mc = metacpan_api_modern::Client::new();
/// let author = mc.author("PLICEASE").await?;
/// println!("{:?}", author.name);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: Url,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Create a client pointed at the public API with default settings.
    ///
    /// # Panics
    ///
    /// Panics if the underlying TLS backend cannot be initialised. Use
    /// [`Client::builder`] with [`ClientBuilder::build`] for a fallible path.
    pub fn new() -> Self {
        Self::builder()
            .build()
            .expect("failed to build default reqwest client")
    }

    /// Start configuring a client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// The base URL every request is resolved against.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// The underlying [`reqwest::Client`], for advanced or ad-hoc requests.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    // -- authors ---------------------------------------------------------

    /// Fetch a CPAN author by PAUSE id (case-insensitive on the server).
    ///
    /// `GET /author/{pauseid}`
    pub async fn author(&self, pauseid: &str) -> Result<Author> {
        self.get_json(&format!("author/{pauseid}")).await
    }

    // -- releases ------------------------------------------------------------

    /// Fetch the most recent release of a distribution.
    ///
    /// `GET /release/{distribution}`
    pub async fn release(&self, distribution: &str) -> Result<Release> {
        self.get_json(&format!("release/{distribution}")).await
    }

    /// Fetch one specific release by author and release name (the release name
    /// includes the version, e.g. `FFI-Platypus-2.10`).
    ///
    /// `GET /release/{author}/{release}`
    pub async fn release_version(&self, author: &str, release: &str) -> Result<Release> {
        let envelope: ReleaseEnvelope = self
            .get_json(&format!("release/{author}/{release}"))
            .await?;
        Ok(envelope.release)
    }

    // -- modules & files ---------------------------------------------------

    /// Resolve a module name to the file that provides it in the latest
    /// indexed release.
    ///
    /// `GET /module/{module}`
    pub async fn module(&self, module: &str) -> Result<File> {
        self.get_json(&format!("module/{module}")).await
    }

    /// Fetch metadata for one file within a release by its archive-relative
    /// path, e.g. `lib/FFI/Platypus.pm`.
    ///
    /// `GET /file/{author}/{release}/{path}`
    pub async fn file(&self, author: &str, release: &str, path: &str) -> Result<File> {
        self.get_json(&format!(
            "file/{author}/{release}/{}",
            path.trim_start_matches('/')
        ))
        .await
    }

    /// Fetch the raw, unrendered source of one file within a release.
    ///
    /// `GET /source/{author}/{release}/{path}`
    pub async fn source(&self, author: &str, release: &str, path: &str) -> Result<String> {
        self.get_text(
            &format!("source/{author}/{release}/{}", path.trim_start_matches('/')),
            &[],
        )
        .await
    }

    // -- pod ---------------------------------------------------------------

    /// Fetch rendered documentation for a module in the requested [`PodFormat`].
    ///
    /// `GET /pod/{module}`
    pub async fn pod(&self, module: &str, format: PodFormat) -> Result<String> {
        self.get_text(&format!("pod/{module}"), &[("content-type", format.mime())])
            .await
    }

    /// Fetch rendered documentation for a specific file within a release.
    ///
    /// `GET /pod/{author}/{release}/{path}`
    pub async fn pod_for_file(
        &self,
        author: &str,
        release: &str,
        path: &str,
        format: PodFormat,
    ) -> Result<String> {
        self.get_text(
            &format!("pod/{author}/{release}/{}", path.trim_start_matches('/')),
            &[("content-type", format.mime())],
        )
        .await
    }

    // -- distributions ---------------------------------------------------

    /// Fetch distribution-level aggregate data (CPAN River, bug counts,
    /// downstream packages, ...).
    ///
    /// `GET /distribution/{distribution}`
    pub async fn distribution(&self, distribution: &str) -> Result<Distribution> {
        self.get_json(&format!("distribution/{distribution}")).await
    }

    /// Fetch the change log of a distribution's latest release.
    ///
    /// `GET /changes/{distribution}`
    pub async fn changes(&self, distribution: &str) -> Result<Changes> {
        self.get_json(&format!("changes/{distribution}")).await
    }

    /// Fetch the change log of one specific release.
    ///
    /// `GET /changes/{author}/{release}`
    pub async fn changes_for_release(&self, author: &str, release: &str) -> Result<Changes> {
        self.get_json(&format!("changes/{author}/{release}")).await
    }

    // -- download_url -----------------------------------------------------

    /// Resolve the download URL for the latest stable release providing a
    /// module. Equivalent to [`download_url_with`](Self::download_url_with)
    /// with default options.
    ///
    /// `GET /download_url/{module}`
    pub async fn download_url(&self, module: &str) -> Result<DownloadUrl> {
        self.download_url_with(module, &DownloadUrlQuery::default())
            .await
    }

    /// Resolve a download URL, honouring a version constraint and/or developer
    /// releases. See [`DownloadUrlQuery`] for how the `version` string is
    /// interpreted server-side.
    ///
    /// `GET /download_url/{module}?version=...&dev=1`
    pub async fn download_url_with(
        &self,
        module: &str,
        query: &DownloadUrlQuery,
    ) -> Result<DownloadUrl> {
        let url = self.url(&format!("download_url/{module}"))?;
        let mut request = self.http.get(url);
        if let Some(version) = &query.version {
            request = request.query(&[("version", version)]);
        }
        if query.dev {
            request = request.query(&[("dev", "1")]);
        }
        let response = request.send().await?;
        decode(&format!("download_url/{module}"), response).await
    }

    // -- mirrors ---------------------------------------------------------

    /// List known CPAN mirrors.
    ///
    /// `GET /mirror`
    pub async fn mirrors(&self) -> Result<Vec<Mirror>> {
        let list: MirrorList = self.get_json("mirror").await?;
        Ok(list.mirrors)
    }

    // -- search --------------------------------------------------------

    /// Run an Elasticsearch query DSL body against `{type}/_search`.
    ///
    /// `type_` is a MetaCPAN document type such as `module`, `release`,
    /// `author`, `file`, `distribution`, or `favorite`. `T` is the shape each
    /// hit's `_source` is deserialized into — use [`serde_json::Value`] for an
    /// untyped result, or one of this crate's document types (e.g.
    /// [`Release`]) when the query targets that type.
    ///
    /// ```no_run
    /// # async fn run() -> Result<(), metacpan_api_modern::Error> {
    /// use serde_json::json;
    /// let mc = metacpan_api_modern::Client::new();
    /// let hits: metacpan_api_modern::SearchResponse<serde_json::Value> = mc
    ///     .search("release", &json!({
    ///         "query": { "term": { "author": "PLICEASE" } },
    ///         "size": 5,
    ///         "sort": [{ "date": "desc" }]
    ///     }))
    ///     .await?;
    /// println!("{} total", hits.total());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn search<T>(
        &self,
        type_: &str,
        query: &serde_json::Value,
    ) -> Result<SearchResponse<T>>
    where
        T: DeserializeOwned,
    {
        self.post_json(&format!("{type_}/_search"), query).await
    }

    /// Run a Lucene query-string search against `{type}/_search?q=...`.
    ///
    /// `GET /{type}/_search?q=...&size=...&from=...`
    pub async fn search_lucene<T>(
        &self,
        type_: &str,
        q: &str,
        size: Option<u32>,
        from: Option<u32>,
    ) -> Result<SearchResponse<T>>
    where
        T: DeserializeOwned,
    {
        let url = self.url(&format!("{type_}/_search"))?;
        let mut params: Vec<(&str, String)> = vec![("q", q.to_owned())];
        if let Some(size) = size {
            params.push(("size", size.to_string()));
        }
        if let Some(from) = from {
            params.push(("from", from.to_string()));
        }
        let response = self.http.get(url).query(&params).send().await?;
        decode(&format!("{type_}/_search"), response).await
    }

    // -- low-level helpers ---------------------------------------------

    /// Join a relative path onto the client's [`base_url`](Self::base_url).
    pub fn url(&self, path: &str) -> Result<Url> {
        Ok(self.base_url.join(path)?)
    }

    /// Perform a `GET` for `path` and deserialize a JSON body into `T`.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path)?;
        let response = self.http.get(url).send().await?;
        decode(path, response).await
    }

    /// Perform a `POST` of `body` as JSON to `path` and deserialize the JSON
    /// response into `T`.
    pub async fn post_json<B, T>(&self, path: &str, body: &B) -> Result<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = self.url(path)?;
        let response = self.http.post(url).json(body).send().await?;
        decode(path, response).await
    }

    /// Perform a `GET` for `path` with `query` params and return the response
    /// body verbatim as text.
    pub async fn get_text(&self, path: &str, query: &[(&str, &str)]) -> Result<String> {
        let url = self.url(path)?;
        let response = self.http.get(url).query(query).send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(api_error_from(status.as_u16(), &text));
        }
        Ok(text)
    }
}

/// Turn a non-success HTTP response into an [`Error`], preferring MetaCPAN's
/// `{ "code", "message" }` JSON shape and falling back to the raw body.
fn api_error_from(status: u16, body: &str) -> Error {
    match serde_json::from_str::<ApiError>(body) {
        Ok(parsed) => Error::Api(parsed),
        Err(_) => Error::Api(ApiError {
            code: status,
            message: if body.is_empty() {
                "<empty response body>".to_owned()
            } else {
                body.to_owned()
            },
        }),
    }
}

async fn decode<T: DeserializeOwned>(path: &str, response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let bytes = response.bytes().await?;
    if !status.is_success() {
        return Err(api_error_from(
            status.as_u16(),
            &String::from_utf8_lossy(&bytes),
        ));
    }
    serde_json::from_slice(&bytes).map_err(|source| Error::Decode {
        path: path.to_owned(),
        source,
    })
}

/// Documentation format understood by the `pod` endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PodFormat {
    /// Plain text (`text/plain`).
    Plain,
    /// Rendered HTML fragment (`text/html`).
    Html,
    /// Markdown (`text/x-markdown`).
    Markdown,
    /// The original POD source (`text/x-pod`).
    Pod,
}

impl PodFormat {
    /// The `content-type` query value MetaCPAN expects for this format.
    pub fn mime(self) -> &'static str {
        match self {
            PodFormat::Plain => "text/plain",
            PodFormat::Html => "text/html",
            PodFormat::Markdown => "text/x-markdown",
            PodFormat::Pod => "text/x-pod",
        }
    }
}

/// Options for [`Client::download_url_with`].
#[derive(Debug, Clone, Default)]
pub struct DownloadUrlQuery {
    /// A version constraint passed straight through to the API's `version`
    /// query parameter.
    ///
    /// An exact pin — `"== 2.08"` — is the form the current production
    /// deployment honours reliably; it also accepts range syntax such as
    /// `"<= 2.10"` or `"!= 2.00, >= 1.00"`, though ranges may fall through to
    /// the latest release. With no constraint the latest release is returned.
    pub version: Option<String>,
    /// Whether developer (trial) releases are eligible.
    pub dev: bool,
}

impl DownloadUrlQuery {
    /// Set the version constraint.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Allow developer (trial) releases to satisfy the request.
    pub fn dev(mut self, dev: bool) -> Self {
        self.dev = dev;
        self
    }
}

/// Builder for [`Client`].
#[derive(Debug, Default)]
pub struct ClientBuilder {
    base_url: Option<String>,
    user_agent: Option<String>,
    timeout: Option<Duration>,
    http: Option<reqwest::Client>,
}

impl ClientBuilder {
    /// Override the API base URL. A trailing slash is added if missing.
    ///
    /// Useful for a private MetaCPAN deployment or a recording proxy.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Override the `User-Agent` header. Ignored when a pre-built
    /// [`reqwest::Client`] is supplied via [`http_client`](Self::http_client).
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Set a total request timeout. Ignored when a pre-built
    /// [`reqwest::Client`] is supplied via [`http_client`](Self::http_client).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Supply a fully configured [`reqwest::Client`] (proxies, custom TLS,
    /// redirect policy, ...). Takes precedence over
    /// [`user_agent`](Self::user_agent) and [`timeout`](Self::timeout).
    pub fn http_client(mut self, http: reqwest::Client) -> Self {
        self.http = Some(http);
        self
    }

    /// Build the [`Client`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Url`] if the base URL does not parse, or
    /// [`Error::Http`] if a new [`reqwest::Client`] must be created and its
    /// TLS backend fails to initialise.
    pub fn build(self) -> Result<Client> {
        let mut base = self.base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        if !base.ends_with('/') {
            base.push('/');
        }
        let base_url = Url::parse(&base)?;

        let http = match self.http {
            Some(http) => http,
            None => {
                let mut builder = reqwest::Client::builder().user_agent(
                    self.user_agent
                        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_owned()),
                );
                if let Some(timeout) = self.timeout {
                    builder = builder.timeout(timeout);
                }
                builder.build()?
            }
        };

        Ok(Client { http, base_url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url_has_trailing_slash_and_joins_cleanly() {
        let client = Client::new();
        assert_eq!(client.base_url().as_str(), DEFAULT_BASE_URL);
        assert_eq!(
            client.url("author/PLICEASE").unwrap().as_str(),
            "https://fastapi.metacpan.org/v1/author/PLICEASE"
        );
    }

    #[test]
    fn builder_adds_missing_trailing_slash() {
        let client = Client::builder()
            .base_url("https://example.test/v1")
            .build()
            .unwrap();
        assert_eq!(
            client.url("module/Foo::Bar").unwrap().as_str(),
            "https://example.test/v1/module/Foo::Bar"
        );
    }

    #[test]
    fn download_url_query_builder() {
        let q = DownloadUrlQuery::default().version("<= 2.10").dev(true);
        assert_eq!(q.version.as_deref(), Some("<= 2.10"));
        assert!(q.dev);
    }

    #[test]
    fn pod_format_mimes() {
        assert_eq!(PodFormat::Plain.mime(), "text/plain");
        assert_eq!(PodFormat::Markdown.mime(), "text/x-markdown");
    }
}
