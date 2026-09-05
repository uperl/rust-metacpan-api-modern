//! Offline tests: feed representative JSON payloads (shaped like real
//! `api.metacpan.org` responses, including the awkward cases) through the
//! crate's `serde` types.

use metacpan_api_modern::types::{MirrorList, ReleaseEnvelope};
use metacpan_api_modern::{Author, DownloadUrl, Release, SearchResponse};

#[test]
fn author_with_scalar_email_and_nested_extra() {
    let json = r#"{
        "pauseid": "PLICEASE",
        "name": "Graham Ollis 🔥",
        "asciiname": "Graham Ollis",
        "email": "plicease@cpan.org",
        "city": "Denver",
        "country": "US",
        "website": [],
        "profile": [{ "name": "github", "id": "plicease" }],
        "links": { "cpan_directory": "https://www.cpan.org/authors/id/P/PL/PLICEASE" },
        "release_count": { "cpan": 74, "latest": 244, "backpan-only": 2999 },
        "is_pause_custodial_account": false,
        "extra": { "everything": "awesome" },
        "updated": "2024-12-08T03:40:30"
    }"#;

    let author: Author = serde_json::from_str(json).unwrap();
    assert_eq!(author.pauseid.as_deref(), Some("PLICEASE"));
    assert_eq!(author.email, vec!["plicease@cpan.org"]);
    assert_eq!(author.profile[0].name.as_deref(), Some("github"));
    assert_eq!(author.release_count.unwrap().backpan_only, Some(2999));
    assert_eq!(
        author.links.get("cpan_directory").map(String::as_str),
        Some("https://www.cpan.org/authors/id/P/PL/PLICEASE")
    );
    // The author-supplied `extra` object is preserved in the catch-all.
    assert!(author.other.contains_key("extra"));
}

#[test]
fn author_with_array_email() {
    let json = r#"{ "pauseid": "X", "email": ["a@example.com", "b@example.com"] }"#;
    let author: Author = serde_json::from_str(json).unwrap();
    assert_eq!(author.email, vec!["a@example.com", "b@example.com"]);
}

#[test]
fn release_direct_and_enveloped() {
    let release_json = r#"{
        "name": "FFI-Platypus-2.10",
        "distribution": "FFI-Platypus",
        "author": "PLICEASE",
        "version": "2.10",
        "version_numified": 2.10,
        "abstract": "Write Perl bindings to non-Perl libraries with FFI. No XS required.",
        "date": "2024-12-18T18:12:39",
        "status": "cpan",
        "maturity": "released",
        "authorized": true,
        "first": false,
        "license": ["perl_5"],
        "provides": ["FFI::Platypus", "FFI::Platypus::Record"],
        "dependency": [
            { "module": "perl", "phase": "runtime", "relationship": "requires", "version": "5.008004" }
        ],
        "stat": { "mode": 33188, "mtime": 1734545559, "size": 137767 },
        "tests": { "pass": 618, "fail": 8, "na": "0", "unknown": "0" },
        "resources": {
            "homepage": "https://github.com/PerlFFI/FFI-Platypus",
            "repository": { "type": "git", "url": "https://github.com/PerlFFI/FFI-Platypus.git" },
            "bugtracker": { "web": "https://github.com/PerlFFI/FFI-Platypus/issues" }
        },
        "metadata": { "name": "FFI-Platypus", "version": "2.10" }
    }"#;

    let release: Release = serde_json::from_str(release_json).unwrap();
    assert_eq!(release.distribution.as_deref(), Some("FFI-Platypus"));
    assert!(
        release
            .r#abstract
            .as_deref()
            .unwrap()
            .starts_with("Write Perl bindings")
    );
    assert_eq!(release.license, vec!["perl_5"]);
    assert_eq!(release.dependency[0].module.as_deref(), Some("perl"));
    assert_eq!(release.stat.unwrap().size, Some(137767));
    // `na` / `unknown` arrive as strings; `flexible_number` normalises them.
    let tests = release.tests.unwrap();
    assert_eq!(tests.pass, Some(618));
    assert_eq!(tests.na, Some(0));
    assert_eq!(tests.unknown, Some(0));
    assert_eq!(
        release
            .resources
            .unwrap()
            .repository
            .unwrap()
            .r#type
            .as_deref(),
        Some("git")
    );

    let enveloped = format!(r#"{{ "release": {release_json}, "took": 1, "total": 1 }}"#);
    let env: ReleaseEnvelope = serde_json::from_str(&enveloped).unwrap();
    assert_eq!(env.release.name.as_deref(), Some("FFI-Platypus-2.10"));
}

#[test]
fn download_url_payload() {
    let json = r#"{
        "checksum_md5": "b3de40c4e8ef8e5d9015a1c855ad363e",
        "checksum_sha256": "f5dd3320e91b01f30bd6932fd3bfe4f374bc41e1908179985171fc64f95f0cf4",
        "date": "2026-01-12T10:32:36",
        "distribution": "FFI-Platypus",
        "download_url": "https://cpan.metacpan.org/authors/id/P/PL/PLICEASE/FFI-Platypus-2.11.tar.gz",
        "release": "FFI-Platypus-2.11",
        "status": "latest",
        "version": "2.11"
    }"#;
    let dl: DownloadUrl = serde_json::from_str(json).unwrap();
    assert_eq!(dl.version.as_deref(), Some("2.11"));
    assert_eq!(dl.status.as_deref(), Some("latest"));
    assert!(
        dl.download_url
            .unwrap()
            .ends_with("FFI-Platypus-2.11.tar.gz")
    );
}

#[test]
fn mirror_list() {
    let json = r#"{ "mirrors": [
        { "name": "www.cpan.org", "org": "Global CPAN CDN", "city": "Everywhere",
          "http": "http://www.cpan.org/", "location": [0.0, 0.0], "ccode": "zz", "tz": "0" }
    ], "total": 1, "took": 0 }"#;
    let list: MirrorList = serde_json::from_str(json).unwrap();
    let mirrors = list.mirrors;
    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].http.as_deref(), Some("http://www.cpan.org/"));
    assert_eq!(mirrors[0].location, vec![0.0, 0.0]);
}

#[test]
fn search_response_modern_total() {
    let json = r#"{
        "took": 5,
        "timed_out": false,
        "hits": {
            "total": { "value": 292, "relation": "eq" },
            "max_score": 9.6,
            "hits": [
                { "_index": "cpan_v1_01", "_id": "abc", "_score": 9.6,
                  "_source": { "name": "FFI-Platypus-2.10", "distribution": "FFI-Platypus" } }
            ]
        }
    }"#;
    let resp: SearchResponse<Release> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.total(), 292);
    assert_eq!(resp.hits.hits[0].id.as_deref(), Some("abc"));
    let sources = resp.into_sources();
    assert_eq!(sources[0].distribution.as_deref(), Some("FFI-Platypus"));
}

#[test]
fn search_response_legacy_integer_total() {
    let json = r#"{
        "took": 1,
        "timed_out": false,
        "hits": { "total": 292, "max_score": null, "hits": [] }
    }"#;
    let resp: SearchResponse<serde_json::Value> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.total(), 292);
    assert!(resp.hits.hits.is_empty());
}
