# metacpan-api-modern

A modern, asynchronous Rust interface to the [MetaCPAN] HTTP API
(<https://api.metacpan.org/>, served today from
<https://fastapi.metacpan.org/v1/>).

The API is a thin REST layer over an Elasticsearch cluster of CPAN metadata.
This crate wraps the endpoints with stable, document-shaped responses in typed
methods, and exposes the full Elasticsearch query DSL for everything else.

## Usage

```rust
use metacpan_api_modern::{Client, PodFormat};

#[tokio::main]
async fn main() -> Result<(), metacpan_api_modern::Error> {
    let mc = Client::new();

    // Author lookup.
    let author = mc.author("PLICEASE").await?;
    println!("{}", author.name.unwrap_or_default());

    // Latest release of a distribution.
    let release = mc.release("FFI-Platypus").await?;
    println!("{} {}", release.distribution.unwrap_or_default(), release.version.unwrap_or_default());

    // Specific release (name includes the version).
    let old = mc.release_version("PLICEASE", "FFI-Platypus-2.10").await?;
    println!("{} deps", old.dependency.len());

    // Resolve a module to the archive that provides it.
    let dl = mc.download_url("FFI::Platypus").await?;
    println!("{}", dl.download_url.unwrap_or_default());

    // With a version range / developer releases.
    use metacpan_api_modern::DownloadUrlQuery;
    let pinned = mc
        .download_url_with("FFI::Platypus", &DownloadUrlQuery::default().version("<= 2.08"))
        .await?;
    println!("{}", pinned.version.unwrap_or_default());

    // Rendered documentation.
    let pod = mc.pod("FFI::Platypus", PodFormat::Plain).await?;
    println!("{}", &pod[..pod.len().min(200)]);

    // Raw source of a file in a release.
    let src = mc.source("PLICEASE", "FFI-Platypus-2.10", "Makefile.PL").await?;
    println!("{} bytes", src.len());

    // Distribution aggregates and change log.
    let dist = mc.distribution("FFI-Platypus").await?;
    println!("river bucket {:?}", dist.river.and_then(|r| r.bucket));
    let changes = mc.changes("FFI-Platypus").await?;
    println!("{}", changes.content.unwrap_or_default().lines().next().unwrap_or_default());

    // Mirrors.
    let mirrors = mc.mirrors().await?;
    println!("{} mirrors", mirrors.len());

    // PAUSE upload permissions (PAUSE `06perms`).
    let perm = mc.permission("FFI::Platypus").await?;
    println!("owner {:?}", perm.owner);
    let mine = mc.permissions_by_author("PLICEASE").await?;
    println!("{} namespaces", mine.len());
    let some = mc.permissions_by_module(["Moose", "FFI::Platypus"]).await?;
    println!("{} looked up", some.len());

    Ok(())
}
```

### Search

`Client::search` posts a raw Elasticsearch query DSL body and deserializes each
hit's `_source` into a type you choose (use `serde_json::Value` for untyped
results):

```rust
use metacpan_api_modern::{Client, Release, SearchResponse};
use serde_json::json;

# async fn run() -> Result<(), metacpan_api_modern::Error> {
let mc = Client::new();
let recent: SearchResponse<Release> = mc
    .search("release", &json!({
        "query": { "term": { "author": "PLICEASE" } },
        "size": 10,
        "sort": [{ "date": "desc" }]
    }))
    .await?;

println!("{} total", recent.total());
for release in recent.sources() {
    println!("{:?}", release.name);
}
# Ok(())
# }
```

`Client::search_lucene` is the query-string variant
(`GET /{type}/_search?q=...`).

## Endpoints covered

| Method | Endpoint |
| --- | --- |
| `author` | `GET /author/{pauseid}` |
| `release` | `GET /release/{distribution}` |
| `release_version` | `GET /release/{author}/{release}` |
| `module` | `GET /module/{module}` |
| `file` | `GET /file/{author}/{release}/{path}` |
| `source` | `GET /source/{author}/{release}/{path}` |
| `pod` / `pod_for_file` | `GET /pod/...` |
| `distribution` | `GET /distribution/{distribution}` |
| `changes` / `changes_for_release` | `GET /changes/...` |
| `download_url` / `download_url_with` | `GET /download_url/{module}` |
| `mirrors` | `GET /mirror` |
| `permission` | `GET /permission/{module}` |
| `permissions_by_author` | `GET /permission/by_author/{author}` |
| `permissions_by_module` | `GET /permission/by_module?module=...` |
| `search` / `search_lucene` | `POST` / `GET /{type}/_search` |

For anything else, `Client::get_json`, `Client::post_json`, and
`Client::get_text` are public low-level helpers that resolve a path against the
configured base URL.

## Configuration

`Client::new()` targets the public production API. `Client::builder()` changes
the base URL, `User-Agent`, or timeout, or accepts a pre-built
`reqwest::Client` (for proxies, custom TLS, and so on).

### Caching

`ClientBuilder::cache_dir` caches successful (`2xx`) responses on the local
filesystem. A `GET` is keyed by its full request URL; a `POST` search
(`Client::search`, `Client::post_json`) by the URL together with its request
body, so different query bodies are cached separately. Each entry is reused
until it is older than `ClientBuilder::cache_ttl` (`DEFAULT_CACHE_TTL`, one
hour, by default).

```rust
use std::time::Duration;

let mc = metacpan_api_modern::Client::builder()
    .cache_dir("/tmp/metacpan-cache")
    .cache_ttl(Duration::from_secs(24 * 60 * 60)) // optional; default is 1 hour
    .build()?;
```

The directory is created on first write. Entries are ordinary files; deleting
them (or the whole directory) just forces a refetch. `Client::clear_cache()`
empties it programmatically (leaving any unrelated files in place), and
`Client::cache_dir()` returns the configured path.

## Testing

`cargo test` runs offline. The network integration tests are `#[ignore]`d:

```text
cargo test --test live -- --ignored --nocapture
```

## License

MIT
