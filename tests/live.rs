//! Integration tests that hit the real `fastapi.metacpan.org` service.
//!
//! They are `#[ignore]`d by default so `cargo test` stays offline and
//! deterministic. Run them explicitly with:
//!
//! ```text
//! cargo test --test live -- --ignored --nocapture
//! ```

use metacpan_api_modern::{Client, DownloadUrlQuery, PodFormat, Release, SearchResponse};
use serde_json::json;

fn client() -> Client {
    Client::builder()
        .user_agent(concat!(
            "metacpan-api-modern-tests/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .unwrap()
}

#[tokio::test]
#[ignore = "network"]
async fn author_roundtrip() {
    let author = client().author("PLICEASE").await.unwrap();
    assert_eq!(author.pauseid.as_deref(), Some("PLICEASE"));
    assert!(author.name.is_some());
    assert!(!author.email.is_empty());
}

#[tokio::test]
#[ignore = "network"]
async fn missing_author_is_not_found() {
    let err = client()
        .author("THIS_PAUSE_ID_DOES_NOT_EXIST")
        .await
        .unwrap_err();
    assert!(err.is_not_found(), "expected 404, got {err:?}");
}

#[tokio::test]
#[ignore = "network"]
async fn release_latest_and_specific() {
    let mc = client();

    let latest = mc.release("FFI-Platypus").await.unwrap();
    assert_eq!(latest.distribution.as_deref(), Some("FFI-Platypus"));

    let specific = mc
        .release_version("PLICEASE", "FFI-Platypus-2.10")
        .await
        .unwrap();
    assert_eq!(specific.version.as_deref(), Some("2.10"));
    assert!(!specific.dependency.is_empty());
}

#[tokio::test]
#[ignore = "network"]
async fn module_and_download_url() {
    let mc = client();

    let file = mc.module("FFI::Platypus").await.unwrap();
    assert_eq!(file.distribution.as_deref(), Some("FFI-Platypus"));
    assert!(
        file.module
            .iter()
            .any(|m| m.name.as_deref() == Some("FFI::Platypus"))
    );

    let dl = mc.download_url("FFI::Platypus").await.unwrap();
    assert!(dl.download_url.unwrap().ends_with(".tar.gz"));

    let pinned = mc
        .download_url_with(
            "FFI::Platypus",
            &DownloadUrlQuery::default().version("== 2.08"),
        )
        .await
        .unwrap();
    assert_eq!(pinned.version.as_deref(), Some("2.08"));
    assert_eq!(pinned.status.as_deref(), Some("backpan"));
}

#[tokio::test]
#[ignore = "network"]
async fn pod_plain_text() {
    let pod = client().pod("Moose", PodFormat::Plain).await.unwrap();
    assert!(pod.contains("Moose"));
}

#[tokio::test]
#[ignore = "network"]
async fn distribution_and_changes() {
    let mc = client();

    let dist = mc.distribution("FFI-Platypus").await.unwrap();
    assert_eq!(dist.name.as_deref(), Some("FFI-Platypus"));
    assert!(dist.river.and_then(|r| r.total).unwrap_or(0) > 0);

    let changes = mc.changes("FFI-Platypus").await.unwrap();
    assert!(changes.content.unwrap_or_default().contains("FFI-Platypus"));
}

#[tokio::test]
#[ignore = "network"]
async fn mirrors_list() {
    let mirrors = client().mirrors().await.unwrap();
    assert!(!mirrors.is_empty());
}

#[tokio::test]
#[ignore = "network"]
async fn search_dsl_typed() {
    let resp: SearchResponse<Release> = client()
        .search(
            "release",
            &json!({
                "query": { "bool": { "must": [
                    { "term": { "author": "PLICEASE" } },
                    { "term": { "status": "latest" } }
                ]}},
                "size": 5,
                "sort": [{ "date": "desc" }]
            }),
        )
        .await
        .unwrap();
    assert!(resp.total() > 0);
    assert!(
        resp.sources()
            .all(|r| r.author.as_deref() == Some("PLICEASE"))
    );
}

#[tokio::test]
#[ignore = "network"]
async fn search_lucene_query_string() {
    let resp: SearchResponse<serde_json::Value> = client()
        .search_lucene("release", "distribution:Moose", Some(0), None)
        .await
        .unwrap();
    assert!(resp.total() > 0);
}
