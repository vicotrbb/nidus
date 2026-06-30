#[test]
fn embedded_dashboard_assets_include_nidus_palette_and_no_forbidden_patterns() {
    let styles = include_str!("../assets/styles.css");
    assert!(styles.contains("oklch("));
    assert!(styles.contains("--brand:"));
    assert!(!styles.contains("background-clip: text"));
    assert!(!styles.contains("-webkit-background-clip: text"));
    assert!(!styles.contains("border-left: 2px"));
    assert!(!styles.contains("border-left: 3px"));
    assert!(!styles.contains("border-left: 4px"));
    assert!(!styles.contains("border-right: 2px"));
    assert!(!styles.contains("border-right: 3px"));
    assert!(!styles.contains("border-right: 4px"));
}
