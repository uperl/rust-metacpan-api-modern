//! The asynchronous [`Client`] and its [`ClientBuilder`].

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

/// Default time-to-live for cached `GET` responses: one hour.
///
/// Used when [`ClientBuilder::cache_dir`] is set without an explicit
/// [`ClientBuilder::cache_ttl`].
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

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
    cache: Option<HttpCache>,
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

    /// The directory this client caches `GET` responses in, or `None` when
    /// [`ClientBuilder::cache_dir`] was not set.
    pub fn cache_dir(&self) -> Option<&Path> {
        self.cache.as_ref().map(|cache| cache.dir.as_path())
    }

    /// Delete every entry from the on-disk cache.
    ///
    /// Removes the cache files this client writes (`*.cache`, plus any leftover
    /// `*.tmp`) from [`cache_dir`](Self::cache_dir); other files in the
    /// directory are left untouched. It is a no-op returning `Ok(())` when no
    /// cache is configured or the directory does not exist yet. The next
    /// request repopulates the cache as usual.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the directory cannot be listed or an entry
    /// cannot be removed.
    pub fn clear_cache(&self) -> Result<()> {
        match &self.cache {
            Some(cache) => cache.clear(),
            None => Ok(()),
        }
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
        let path = format!("download_url/{module}");
        let mut url = self.url(&path)?;
        if query.version.is_some() || query.dev {
            let mut pairs = url.query_pairs_mut();
            if let Some(version) = &query.version {
                pairs.append_pair("version", version);
            }
            if query.dev {
                pairs.append_pair("dev", "1");
            }
        }
        let raw = self.get_raw(url).await?;
        decode_bytes(&path, raw.status, &raw.bytes)
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
        let path = format!("{type_}/_search");
        let mut url = self.url(&path)?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", q);
            if let Some(size) = size {
                pairs.append_pair("size", &size.to_string());
            }
            if let Some(from) = from {
                pairs.append_pair("from", &from.to_string());
            }
        }
        let raw = self.get_raw(url).await?;
        decode_bytes(&path, raw.status, &raw.bytes)
    }

    // -- low-level helpers ---------------------------------------------

    /// Join a relative path onto the client's [`base_url`](Self::base_url).
    pub fn url(&self, path: &str) -> Result<Url> {
        Ok(self.base_url.join(path)?)
    }

    /// Perform a `GET` for `path` and deserialize a JSON body into `T`.
    ///
    /// When the client is configured with [`ClientBuilder::cache_dir`] the
    /// response is served from, and stored in, the on-disk cache.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path)?;
        let raw = self.get_raw(url).await?;
        decode_bytes(path, raw.status, &raw.bytes)
    }

    /// Perform a `POST` of `body` as JSON to `path` and deserialize the JSON
    /// response into `T`. `POST` requests are never cached.
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
    /// body verbatim as text. Honours the on-disk cache like
    /// [`get_json`](Self::get_json).
    pub async fn get_text(&self, path: &str, query: &[(&str, &str)]) -> Result<String> {
        let mut url = self.url(path)?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter());
        }
        let raw = self.get_raw(url).await?;
        if !(200..300).contains(&raw.status) {
            return Err(api_error_from(
                raw.status,
                &String::from_utf8_lossy(&raw.bytes),
            ));
        }
        Ok(String::from_utf8_lossy(&raw.bytes).into_owned())
    }

    /// Perform a `GET` for `url`, returning the raw status and body bytes.
    ///
    /// If an on-disk cache is configured and holds a fresh entry for this exact
    /// URL, that entry is returned without a network request. Otherwise the
    /// request is made and a successful (`2xx`) response is written to the
    /// cache before being returned.
    async fn get_raw(&self, url: Url) -> Result<RawResponse> {
        if let Some(cache) = &self.cache
            && let Some(hit) = cache.get(&url)
        {
            return Ok(hit);
        }
        let response = self.http.get(url.clone()).send().await?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await?.to_vec();
        if let Some(cache) = &self.cache
            && (200..300).contains(&status)
        {
            cache.put(&url, status, &bytes);
        }
        Ok(RawResponse { status, bytes })
    }
}

/// A `GET` response reduced to the parts this crate needs, so it can come
/// equally from the network or from the on-disk cache.
struct RawResponse {
    status: u16,
    bytes: Vec<u8>,
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
    let status = response.status().as_u16();
    let bytes = response.bytes().await?;
    decode_bytes(path, status, &bytes)
}

/// Shared by the cached and uncached paths: turn a status code and body bytes
/// into either a deserialized `T` or an [`Error`].
fn decode_bytes<T: DeserializeOwned>(path: &str, status: u16, bytes: &[u8]) -> Result<T> {
    if !(200..300).contains(&status) {
        return Err(api_error_from(status, &String::from_utf8_lossy(bytes)));
    }
    serde_json::from_slice(bytes).map_err(|source| Error::Decode {
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
    cache_dir: Option<PathBuf>,
    cache_ttl: Option<Duration>,
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

    /// Cache successful `GET` responses on disk under `dir`, reusing them until
    /// they are older than [`cache_ttl`](Self::cache_ttl) (one hour by default)
    /// before refetching.
    ///
    /// Every `GET` the client makes — the typed endpoint methods,
    /// [`Client::get_json`], [`Client::get_text`],
    /// [`download_url`](Client::download_url), and
    /// [`search_lucene`](Client::search_lucene) — is keyed by its full request
    /// URL, including the query string. `POST` requests (the Elasticsearch
    /// [`Client::search`]) are never cached, and only `2xx` responses are
    /// stored; errors are always refetched.
    ///
    /// The directory is created on first write. Entries are ordinary files
    /// named by a hash of the URL; deleting one, or the whole directory, simply
    /// forces a refetch. Cache reads and writes are best-effort — an I/O error
    /// falls back to a normal request rather than failing the call.
    pub fn cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(dir.into());
        self
    }

    /// Override how long a cached `GET` response stays fresh. Has no effect
    /// unless [`cache_dir`](Self::cache_dir) is also set. Defaults to
    /// [`DEFAULT_CACHE_TTL`] (one hour).
    pub fn cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = Some(ttl);
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

        let cache = self.cache_dir.map(|dir| HttpCache {
            dir,
            ttl: self.cache_ttl.unwrap_or(DEFAULT_CACHE_TTL),
        });

        Ok(Client {
            http,
            base_url,
            cache,
        })
    }
}

/// A filesystem cache of `GET` responses, configured by
/// [`ClientBuilder::cache_dir`].
///
/// Each entry is one file, named `<hash-of-url>.cache`, laid out as a short
/// UTF-8 header (`expires-at` unix seconds, then status, then the URL), a blank
/// line, and the raw response body. The URL is stored so a hash collision reads
/// as a miss rather than serving the wrong body.
#[derive(Debug, Clone)]
struct HttpCache {
    dir: PathBuf,
    ttl: Duration,
}

impl HttpCache {
    /// Stable-per-URL basename (without extension) for a cache entry.
    fn key(&self, url: &Url) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        url.as_str().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn entry_path(&self, url: &Url) -> PathBuf {
        self.dir.join(format!("{}.cache", self.key(url)))
    }

    /// Return a still-fresh cached response for `url`, if one exists. A missing
    /// file, an unreadable or malformed entry, a URL mismatch (hash collision),
    /// or an expired entry all read as a miss; expired entries are removed.
    fn get(&self, url: &Url) -> Option<RawResponse> {
        let path = self.entry_path(url);
        let raw = std::fs::read(&path).ok()?;
        let sep = raw.windows(2).position(|w| w == b"\n\n")?;
        let header = std::str::from_utf8(&raw[..sep]).ok()?;
        let mut lines = header.lines();
        let expires_at: u64 = lines.next()?.parse().ok()?;
        let status: u16 = lines.next()?.parse().ok()?;
        let cached_url = lines.next()?;
        if cached_url != url.as_str() {
            return None;
        }
        if now_secs() >= expires_at {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        Some(RawResponse {
            status,
            bytes: raw[sep + 2..].to_vec(),
        })
    }

    /// Delete every `*.cache` / `*.tmp` file in the cache directory, leaving
    /// any unrelated files (and a missing directory) alone.
    fn clear(&self) -> Result<()> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("cache" | "tmp")
            ) {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// Best-effort write of a response for `url`. Any I/O error is swallowed:
    /// the cache is an optimisation, never a source of failures.
    fn put(&self, url: &Url, status: u16, body: &[u8]) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let expires_at = now_secs().saturating_add(self.ttl.as_secs());
        let mut contents =
            format!("{expires_at}\n{status}\n{}\n\n", url.as_str()).into_bytes();
        contents.extend_from_slice(body);

        // Write to a unique temp file then rename, so a concurrent reader never
        // observes a half-written entry.
        let tmp = self.dir.join(format!("{}.{}.tmp", self.key(url), now_nanos()));
        if std::fs::write(&tmp, contents).is_ok() {
            let _ = std::fs::rename(&tmp, self.entry_path(url));
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
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

    #[test]
    fn http_cache_hit_miss_and_expiry() {
        let dir = std::env::temp_dir().join(format!(
            "metacpan-api-modern-cache-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let cache = HttpCache {
            dir: dir.clone(),
            ttl: Duration::from_secs(3600),
        };
        let url = Url::parse("https://example.test/v1/author/PLICEASE").unwrap();

        assert!(cache.get(&url).is_none(), "cold cache is a miss");

        cache.put(&url, 200, b"{\"name\":\"Graham\"}");
        let hit = cache.get(&url).expect("warm cache is a hit");
        assert_eq!(hit.status, 200);
        assert_eq!(hit.bytes, b"{\"name\":\"Graham\"}");

        let other = Url::parse("https://example.test/v1/author/OTHER").unwrap();
        assert!(cache.get(&other).is_none(), "a different url is a miss");

        let stale = HttpCache {
            dir: dir.clone(),
            ttl: Duration::from_secs(0),
        };
        stale.put(&url, 200, b"body");
        assert!(cache.get(&url).is_none(), "an expired entry is a miss");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builder_wires_up_cache() {
        let dir = std::env::temp_dir().join("metacpan-api-modern-cache-builder");
        let client = Client::builder()
            .cache_dir(&dir)
            .cache_ttl(Duration::from_secs(120))
            .build()
            .unwrap();
        let cache = client.cache.as_ref().expect("cache configured");
        assert_eq!(cache.dir, dir);
        assert_eq!(cache.ttl, Duration::from_secs(120));
    }

    #[test]
    fn no_cache_by_default() {
        assert!(Client::new().cache.is_none());
        assert!(Client::new().cache_dir().is_none());
    }

    #[test]
    fn clear_cache_removes_only_our_entries() {
        let dir = std::env::temp_dir().join(format!(
            "metacpan-api-modern-clear-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let client = Client::builder().cache_dir(&dir).build().unwrap();
        assert_eq!(client.cache_dir(), Some(dir.as_path()));

        let cache = client.cache.as_ref().unwrap();
        let url = Url::parse("https://example.test/v1/author/PLICEASE").unwrap();
        cache.put(&url, 200, b"{}");
        assert!(cache.get(&url).is_some());

        std::fs::write(dir.join("unrelated.txt"), b"keep me").unwrap();

        client.clear_cache().unwrap();
        assert!(cache.get(&url).is_none(), "cache entry was cleared");
        assert!(
            dir.join("unrelated.txt").exists(),
            "unrelated files are left alone"
        );

        // Idempotent, and fine once the directory is gone entirely.
        client.clear_cache().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        client.clear_cache().unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_cache_is_noop_without_a_cache() {
        Client::new().clear_cache().unwrap();
    }
}
