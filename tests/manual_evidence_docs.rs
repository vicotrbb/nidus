//! Regression coverage for manual example evidence drift.

const MANUAL_EVIDENCE: &str =
    include_str!("../docs/superpowers/audits/2026-06-26-manual-example-curl-evidence.md");

#[test]
fn auth_api_manual_evidence_matches_header_guard_behavior() {
    let auth_section = section("auth-api", MANUAL_EVIDENCE);

    for required in [
        "`GET /me` without `x-api-key` -> `HTTP/1.1 401 Unauthorized`",
        "`GET /me` with `x-api-key: wrong` -> `HTTP/1.1 401 Unauthorized`",
        "`GET /me` with `x-api-key: nidus-dev-secret` -> `HTTP/1.1 200 OK`",
    ] {
        assert!(
            auth_section.contains(required),
            "auth-api manual evidence must include current header-guard outcome: {required}",
        );
    }
}

fn section<'a>(heading: &str, document: &'a str) -> &'a str {
    let marker = format!("## {heading}");
    let start = document
        .find(&marker)
        .unwrap_or_else(|| panic!("missing `{marker}` section"));
    let rest = &document[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    &rest[..end]
}
