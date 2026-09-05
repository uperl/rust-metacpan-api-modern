//! Strongly typed views of the JSON documents returned by the MetaCPAN API.
//!
//! MetaCPAN is backed by Elasticsearch and its documents are large, loosely
//! specified, and grow new fields over time. Every struct here therefore:
//!
//! * makes almost every field [`Option`] or a collection that defaults to
//!   empty, so a missing key is never a hard error, and
//! * keeps a `#[serde(flatten)]` `other` map that captures any field this
//!   crate does not model yet, so nothing from the response is ever lost.

use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

/// Freeform bag of fields returned by the API but not modelled explicitly.
pub type Extra = Map<String, Value>;

/// Accept a number, a stringified number, an empty string, or `null`, and
/// yield `Option<T>`.
///
/// Elasticsearch `_source` documents routinely serialize the same numeric
/// field as `8` in one record and `"0"` in the next (MetaCPAN's CPAN Testers
/// tallies are a live example), so every modelled numeric field goes through
/// this rather than trusting the wire type.
fn flexible_number<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr + Deserialize<'de>,
    <T as FromStr>::Err: Display,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr<T> {
        Null,
        Num(T),
        Str(String),
    }

    match NumOrStr::<T>::deserialize(deserializer)? {
        NumOrStr::Null => Ok(None),
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                trimmed.parse().map(Some).map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Accept either a bare scalar or an array of them, always yielding a [`Vec`].
///
/// Several MetaCPAN fields (`license`, an author's `email`, ...) are documented
/// as lists but are sometimes serialized as a single string, or as `null`.
fn one_or_many<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany<T> {
        Null,
        One(T),
        Many(Vec<T>),
    }

    Ok(match OneOrMany::<T>::deserialize(deserializer)? {
        OneOrMany::Null => Vec::new(),
        OneOrMany::One(value) => vec![value],
        OneOrMany::Many(values) => values,
    })
}

// ---------------------------------------------------------------------------
// author
// ---------------------------------------------------------------------------

/// A CPAN author, as returned by `GET /author/{pauseid}`.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Author {
    /// The author's PAUSE id, e.g. `PLICEASE`.
    pub pauseid: Option<String>,
    /// Display name, which may contain arbitrary Unicode.
    pub name: Option<String>,
    /// ASCII transliteration of [`name`](Self::name).
    pub asciiname: Option<String>,
    /// Public contact addresses. Normalised to a list even when the API sends
    /// a single string.
    #[serde(default, deserialize_with = "one_or_many")]
    pub email: Vec<String>,
    /// URL of the author's Gravatar image.
    pub gravatar_url: Option<String>,
    /// Free-form "city" line from the author's profile.
    pub city: Option<String>,
    /// Free-form "region" / state line.
    pub region: Option<String>,
    /// ISO country code from the author's profile.
    pub country: Option<String>,
    /// Personal or project websites.
    #[serde(default, deserialize_with = "one_or_many")]
    pub website: Vec<String>,
    /// Social / code-hosting profiles (GitHub, GitLab, Mastodon, ...).
    #[serde(default)]
    pub profile: Vec<Profile>,
    /// Convenience links MetaCPAN pre-computes for this author, keyed by a
    /// short name such as `cpan_directory` or `cpantesters_reports`.
    #[serde(default)]
    pub links: std::collections::BTreeMap<String, String>,
    /// Counts of releases in various states.
    pub release_count: Option<ReleaseCount>,
    /// Last time the author profile was updated (ISO 8601, no timezone).
    pub updated: Option<String>,
    /// Whether this is a PAUSE-managed custodial account for an inactive author.
    pub is_pause_custodial_account: Option<bool>,
    /// Internal MetaCPAN user id, present when the PAUSE account is linked.
    pub user: Option<String>,
    /// Everything else the endpoint returned (including the author-supplied
    /// `extra` object).
    #[serde(flatten)]
    pub other: Extra,
}

/// One entry in [`Author::profile`].
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Profile {
    /// Name of the service, e.g. `github`.
    pub name: Option<String>,
    /// The author's identifier on that service.
    pub id: Option<String>,
}

/// Release tallies attached to an [`Author`].
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ReleaseCount {
    /// Distributions with at least one release currently on CPAN.
    #[serde(default, deserialize_with = "flexible_number")]
    pub cpan: Option<u32>,
    /// Distributions whose latest release is indexed as `latest`.
    #[serde(default, deserialize_with = "flexible_number")]
    pub latest: Option<u32>,
    /// Distributions that only survive on BackPAN.
    #[serde(rename = "backpan-only", default, deserialize_with = "flexible_number")]
    pub backpan_only: Option<u32>,
}

// ---------------------------------------------------------------------------
// release
// ---------------------------------------------------------------------------

/// A distribution release, as returned by `GET /release/{distribution}` or
/// `GET /release/{author}/{release}`.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Release {
    /// Release name including version, e.g. `FFI-Platypus-2.10`.
    pub name: Option<String>,
    /// Distribution name, e.g. `FFI-Platypus`.
    pub distribution: Option<String>,
    /// PAUSE id of the uploading author.
    pub author: Option<String>,
    /// Version string as declared by the distribution.
    pub version: Option<String>,
    /// Numeric form of [`version`](Self::version) for ordering.
    #[serde(default, deserialize_with = "flexible_number")]
    pub version_numified: Option<f64>,
    /// Short description of the distribution.
    #[serde(rename = "abstract")]
    pub r#abstract: Option<String>,
    /// Archive file name, e.g. `FFI-Platypus-2.10.tar.gz`.
    pub archive: Option<String>,
    /// Upload timestamp (ISO 8601, no timezone).
    pub date: Option<String>,
    /// Index status: `latest`, `cpan`, or `backpan`.
    pub status: Option<String>,
    /// Maturity: `released` or `developer`.
    pub maturity: Option<String>,
    /// Whether the upload is authorized for its namespace.
    pub authorized: Option<bool>,
    /// Whether the distribution is marked deprecated.
    pub deprecated: Option<bool>,
    /// Whether this is the author's first release of the distribution.
    pub first: Option<bool>,
    /// SPDX-ish license tokens declared in metadata.
    #[serde(default, deserialize_with = "one_or_many")]
    pub license: Vec<String>,
    /// Direct download URL on the CPAN CDN.
    pub download_url: Option<String>,
    /// MD5 checksum of the archive.
    pub checksum_md5: Option<String>,
    /// SHA-256 checksum of the archive.
    pub checksum_sha256: Option<String>,
    /// Name of the change log file within the archive.
    pub changes_file: Option<String>,
    /// Main module of the distribution.
    pub main_module: Option<String>,
    /// Package names the release provides.
    #[serde(default)]
    pub provides: Vec<String>,
    /// Declared prerequisites across all phases.
    #[serde(default)]
    pub dependency: Vec<Dependency>,
    /// `stat(2)`-like metadata for the archive file.
    pub stat: Option<Stat>,
    /// Curated links from distribution metadata (repository, homepage, ...).
    pub resources: Option<Resources>,
    /// CPAN Testers pass/fail tallies (only on the by-author endpoint).
    pub tests: Option<TestSummary>,
    /// The raw, unmodelled `META.json` / `META.yml` document.
    pub metadata: Option<Value>,
    /// MetaCPAN document id.
    pub id: Option<String>,
    /// Any other fields present in the response.
    #[serde(flatten)]
    pub other: Extra,
}

/// Envelope used by `GET /release/{author}/{release}`, which wraps the
/// document in a `release` key. [`Client::release_version`] unwraps it for
/// you; it is public only so the shape can be named in tests.
///
/// [`Client::release_version`]: crate::Client::release_version
#[doc(hidden)]
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ReleaseEnvelope {
    /// The wrapped release document.
    pub release: Release,
}

/// A single prerequisite from [`Release::dependency`].
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Dependency {
    /// Required module name.
    pub module: Option<String>,
    /// Phase: `configure`, `build`, `test`, `runtime`, or `develop`.
    pub phase: Option<String>,
    /// Relationship: `requires`, `recommends`, `suggests`, or `conflicts`.
    pub relationship: Option<String>,
    /// Minimum version, as a string (`"0"` when unconstrained).
    pub version: Option<String>,
}

/// `stat(2)`-style file metadata.
#[derive(Debug, Clone, Copy, Deserialize)]
#[non_exhaustive]
pub struct Stat {
    /// Unix mode bits.
    #[serde(default, deserialize_with = "flexible_number")]
    pub mode: Option<u32>,
    /// Modification time as a Unix timestamp.
    #[serde(default, deserialize_with = "flexible_number")]
    pub mtime: Option<i64>,
    /// Size in bytes.
    #[serde(default, deserialize_with = "flexible_number")]
    pub size: Option<u64>,
}

/// Curated resource links from distribution metadata.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Resources {
    /// Project homepage.
    pub homepage: Option<String>,
    /// License URLs.
    #[serde(default, deserialize_with = "one_or_many")]
    pub license: Vec<String>,
    /// Issue tracker.
    pub bugtracker: Option<Bugtracker>,
    /// Source repository.
    pub repository: Option<Repository>,
    /// Anything else under `resources`.
    #[serde(flatten)]
    pub other: Extra,
}

/// Issue-tracker links from [`Resources`].
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Bugtracker {
    /// Web UI for the tracker.
    pub web: Option<String>,
    /// Email address that files a ticket.
    pub mailto: Option<String>,
}

/// Source-repository links from [`Resources`].
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Repository {
    /// VCS type, e.g. `git`.
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    /// Clone URL.
    pub url: Option<String>,
    /// Web UI for browsing the repository.
    pub web: Option<String>,
}

/// CPAN Testers tallies for a release.
#[derive(Debug, Clone, Copy, Deserialize)]
#[non_exhaustive]
pub struct TestSummary {
    /// Reports that passed.
    #[serde(default, deserialize_with = "flexible_number")]
    pub pass: Option<u32>,
    /// Reports that failed.
    #[serde(default, deserialize_with = "flexible_number")]
    pub fail: Option<u32>,
    /// Reports marked not-applicable.
    #[serde(default, deserialize_with = "flexible_number")]
    pub na: Option<u32>,
    /// Reports with an unknown grade.
    #[serde(default, deserialize_with = "flexible_number")]
    pub unknown: Option<u32>,
}

// ---------------------------------------------------------------------------
// file / module
// ---------------------------------------------------------------------------

/// A file within a release, as returned by `GET /file/{author}/{release}/{path}`
/// and by `GET /module/{module}` (which resolves to the file providing that
/// module in the latest release).
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct File {
    /// Base name of the file, e.g. `Platypus.pm`.
    pub name: Option<String>,
    /// Path of the file within the archive, e.g. `lib/FFI/Platypus.pm`.
    pub path: Option<String>,
    /// Distribution the file belongs to.
    pub distribution: Option<String>,
    /// PAUSE id of the release author.
    pub author: Option<String>,
    /// Release name including version.
    pub release: Option<String>,
    /// Version associated with the file's primary module.
    pub version: Option<String>,
    /// Numeric form of [`version`](Self::version).
    #[serde(default, deserialize_with = "flexible_number")]
    pub version_numified: Option<f64>,
    /// `abstract` line parsed from the file's POD.
    #[serde(rename = "abstract")]
    pub r#abstract: Option<String>,
    /// First paragraph(s) of the DESCRIPTION section.
    pub description: Option<String>,
    /// Name of the module treated as this file's documentation.
    pub documentation: Option<String>,
    /// Length of the documentation module name.
    #[serde(default, deserialize_with = "flexible_number")]
    pub documentation_length: Option<u64>,
    /// MIME type MetaCPAN assigned to the file.
    pub mime: Option<String>,
    /// Index status inherited from the release: `latest`, `cpan`, `backpan`.
    pub status: Option<String>,
    /// Maturity inherited from the release.
    pub maturity: Option<String>,
    /// Upload timestamp of the release.
    pub date: Option<String>,
    /// Direct download URL of the containing archive.
    pub download_url: Option<String>,
    /// Whether the file is authorized for its namespace.
    pub authorized: Option<bool>,
    /// Whether the file is indexed by PAUSE.
    pub indexed: Option<bool>,
    /// Whether MetaCPAN considers the file binary.
    pub binary: Option<bool>,
    /// Whether the file (or its module) is deprecated.
    pub deprecated: Option<bool>,
    /// Whether the path is a directory.
    pub directory: Option<bool>,
    /// Favourite count of the containing distribution.
    #[serde(default, deserialize_with = "flexible_number")]
    pub dist_fav_count: Option<u64>,
    /// Directory depth of the file within the archive.
    #[serde(default, deserialize_with = "flexible_number")]
    pub level: Option<u64>,
    /// Rendered POD text, when requested.
    pub pod: Option<String>,
    /// `[offset, length]` spans of POD blocks within the file.
    #[serde(default)]
    pub pod_lines: Vec<Value>,
    /// Source lines of code.
    #[serde(default, deserialize_with = "flexible_number")]
    pub sloc: Option<u64>,
    /// Source lines of POD.
    #[serde(default, deserialize_with = "flexible_number")]
    pub slop: Option<u64>,
    /// `stat(2)`-like metadata for the file.
    pub stat: Option<Stat>,
    /// Modules declared in this file.
    #[serde(default)]
    pub module: Vec<ModuleInfo>,
    /// Elasticsearch completion-suggester payload.
    pub suggest: Option<Value>,
    /// MetaCPAN document id.
    pub id: Option<String>,
    /// Any other fields present in the response.
    #[serde(flatten)]
    pub other: Extra,
}

/// One module declared within a [`File`].
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct ModuleInfo {
    /// Fully-qualified module name.
    pub name: Option<String>,
    /// Version declared for the module.
    pub version: Option<String>,
    /// Numeric form of the version.
    #[serde(default, deserialize_with = "flexible_number")]
    pub version_numified: Option<f64>,
    /// Whether the module is authorized for its namespace.
    pub authorized: Option<bool>,
    /// Whether the module is indexed by PAUSE.
    pub indexed: Option<bool>,
    /// Path (within some release) of the POD associated with this module.
    pub associated_pod: Option<String>,
}

// ---------------------------------------------------------------------------
// distribution
// ---------------------------------------------------------------------------

/// Distribution-level aggregate data, as returned by
/// `GET /distribution/{distribution}`.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Distribution {
    /// Distribution name.
    pub name: Option<String>,
    /// CPAN River metrics (how many things depend on this distribution).
    pub river: Option<River>,
    /// Per-tracker bug counts (`github`, `rt`, ...). Shape varies by tracker,
    /// so it is left as raw JSON.
    pub bugs: Option<Value>,
    /// Repository host metadata (stars, watchers, ...), by host.
    pub repo: Option<Value>,
    /// Known downstream OS packages, keyed by distro (`debian`, `fedora`, ...).
    #[serde(default)]
    pub external_package: std::collections::BTreeMap<String, String>,
    /// Any other fields present in the response.
    #[serde(flatten)]
    pub other: Extra,
}

/// CPAN River metrics from [`Distribution::river`].
#[derive(Debug, Clone, Copy, Deserialize)]
#[non_exhaustive]
pub struct River {
    /// Bucket 0–5; higher means more depended upon.
    #[serde(default, deserialize_with = "flexible_number")]
    pub bucket: Option<u32>,
    /// Number of distinct authors among immediate dependents.
    #[serde(default, deserialize_with = "flexible_number")]
    pub bus_factor: Option<u32>,
    /// Distributions that depend on this one directly.
    #[serde(default, deserialize_with = "flexible_number")]
    pub immediate: Option<u64>,
    /// Distributions that depend on this one directly or transitively.
    #[serde(default, deserialize_with = "flexible_number")]
    pub total: Option<u64>,
}

// ---------------------------------------------------------------------------
// download_url
// ---------------------------------------------------------------------------

/// Result of `GET /download_url/{module}`: the archive that satisfies a module
/// request, honouring version ranges and the developer-release flag.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct DownloadUrl {
    /// Direct download URL on the CPAN CDN.
    pub download_url: Option<String>,
    /// Version of the resolved release.
    pub version: Option<String>,
    /// Resolved release name including version.
    pub release: Option<String>,
    /// Distribution name.
    pub distribution: Option<String>,
    /// Index status of the resolved release.
    pub status: Option<String>,
    /// Upload timestamp of the resolved release.
    pub date: Option<String>,
    /// MD5 checksum of the archive.
    pub checksum_md5: Option<String>,
    /// SHA-256 checksum of the archive.
    pub checksum_sha256: Option<String>,
    /// Any other fields present in the response.
    #[serde(flatten)]
    pub other: Extra,
}

// ---------------------------------------------------------------------------
// changes
// ---------------------------------------------------------------------------

/// A distribution's change log, as returned by `GET /changes/{distribution}`.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Changes {
    /// File name of the change log within the archive.
    pub name: Option<String>,
    /// Full text of the change log.
    pub content: Option<String>,
    /// MetaCPAN category, typically `changelog`.
    pub category: Option<String>,
    /// PAUSE id of the release author.
    pub author: Option<String>,
    /// Release the change log was taken from.
    pub release: Option<String>,
    /// Distribution name.
    pub distribution: Option<String>,
    /// Any other fields present in the response.
    #[serde(flatten)]
    pub other: Extra,
}

// ---------------------------------------------------------------------------
// mirror
// ---------------------------------------------------------------------------

/// Envelope returned by `GET /mirror`. [`Client::mirrors`] unwraps it for you;
/// it is public only so the shape can be named in tests.
///
/// [`Client::mirrors`]: crate::Client::mirrors
#[doc(hidden)]
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct MirrorList {
    /// The mirror entries.
    #[serde(default)]
    pub mirrors: Vec<Mirror>,
}

/// A single CPAN mirror entry.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Mirror {
    /// Mirror host name.
    pub name: Option<String>,
    /// Operating organisation.
    pub org: Option<String>,
    /// City.
    pub city: Option<String>,
    /// Region / state.
    pub region: Option<String>,
    /// Country.
    pub country: Option<String>,
    /// Continent.
    pub continent: Option<String>,
    /// Two-letter country code.
    pub ccode: Option<String>,
    /// HTTP base URL, when the mirror serves over HTTP.
    pub http: Option<String>,
    /// FTP base URL, when the mirror serves over FTP.
    pub ftp: Option<String>,
    /// rsync URL, when the mirror serves over rsync.
    pub rsync: Option<String>,
    /// Upstream source the mirror pulls from.
    pub src: Option<String>,
    /// Sync frequency, e.g. `daily` or `instant`.
    pub freq: Option<String>,
    /// Date the mirror was added.
    pub inceptdate: Option<String>,
    /// `[latitude, longitude]` of the mirror.
    #[serde(default)]
    pub location: Vec<f64>,
    /// Timezone offset as a string.
    pub tz: Option<String>,
    /// Any other fields present in the entry.
    #[serde(flatten)]
    pub other: Extra,
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

/// A parsed Elasticsearch search response.
///
/// `T` is the type each hit's `_source` is deserialized into; use
/// [`serde_json::Value`] when you do not want a fixed shape.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct SearchResponse<T> {
    /// Milliseconds Elasticsearch spent on the query.
    pub took: Option<u64>,
    /// Whether the query timed out server-side.
    pub timed_out: Option<bool>,
    /// The hits container.
    pub hits: Hits<T>,
    /// Aggregation results, when the query requested any.
    #[serde(rename = "aggregations")]
    pub aggregations: Option<Value>,
}

impl<T> SearchResponse<T> {
    /// Total number of documents that matched, ignoring pagination.
    pub fn total(&self) -> u64 {
        self.hits.total.value()
    }

    /// Consume the response and return just the `_source` of each hit.
    pub fn into_sources(self) -> Vec<T> {
        self.hits.hits.into_iter().map(|hit| hit.source).collect()
    }

    /// Borrow the `_source` of each hit.
    pub fn sources(&self) -> impl Iterator<Item = &T> {
        self.hits.hits.iter().map(|hit| &hit.source)
    }
}

/// The `hits` object of a [`SearchResponse`].
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Hits<T> {
    /// Match count. Elasticsearch 7+ reports an object; older versions report
    /// a bare integer. Both are accepted.
    pub total: Total,
    /// Highest `_score` among the hits, if scored.
    pub max_score: Option<f64>,
    /// The returned page of hits.
    pub hits: Vec<Hit<T>>,
}

/// Elasticsearch hit-total, tolerant of both the modern and legacy encodings.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Total {
    /// Legacy encoding: a bare document count.
    Count(u64),
    /// Modern encoding: a value plus a relation (`eq` or `gte`).
    Tracked {
        /// The (possibly lower-bound) match count.
        value: u64,
        /// `eq` for an exact count, `gte` when the count was capped.
        relation: String,
    },
}

impl Total {
    /// The match count as a plain integer.
    pub fn value(&self) -> u64 {
        match self {
            Total::Count(n) => *n,
            Total::Tracked { value, .. } => *value,
        }
    }
}

/// One search hit.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct Hit<T> {
    /// Elasticsearch index the hit came from.
    #[serde(rename = "_index")]
    pub index: Option<String>,
    /// Document id.
    #[serde(rename = "_id")]
    pub id: Option<String>,
    /// Relevance score, when the query is scored.
    #[serde(rename = "_score")]
    pub score: Option<f64>,
    /// The document body.
    #[serde(rename = "_source")]
    pub source: T,
    /// Requested stored/`fields` projection, when used instead of `_source`.
    #[serde(default)]
    pub fields: Option<Value>,
}
