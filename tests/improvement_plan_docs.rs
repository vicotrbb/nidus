//! Regression coverage for improvement-plan status drift.

const IMPROVEMENT_PLAN: &str =
    include_str!("../docs/superpowers/plans/2026-06-26-nidus-framework-quality-improvements.md");

#[test]
fn deferred_items_section_does_not_relist_mitigated_wave_items() {
    let deferred = section("Deferred items", IMPROVEMENT_PLAN);

    for mitigated in [
        "F-CORE-4", "F-CORE-5", "F-HTTP-2", "F-HTTP-3", "F-HTTP-5", "F-HTTP-7", "F-HTTP-8", "O-1",
        "O-2", "EX-2", "CLI-1", "CLI-2", "T-1", "T-2",
    ] {
        assert!(
            !deferred.contains(mitigated),
            "`Deferred items` must not relist already mitigated item {mitigated}",
        );
    }

    for still_deferred in ["F-CORE-3", "F-MAC-1", "E-2", "AD-2", "AD-3", "BENCH-1"] {
        assert!(
            deferred.contains(still_deferred),
            "`Deferred items` must keep current intentional deferral {still_deferred}",
        );
    }
}

fn section<'a>(heading_prefix: &str, document: &'a str) -> &'a str {
    let marker = format!("## {heading_prefix}");
    let start = document
        .find(&marker)
        .unwrap_or_else(|| panic!("missing `{marker}` section"));
    let rest = &document[start..];
    let end = rest.find("\n---").unwrap_or(rest.len());
    &rest[..end]
}
