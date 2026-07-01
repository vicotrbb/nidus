#[test]
fn embedded_dashboard_assets_include_nidus_palette_and_no_forbidden_patterns() {
    let styles = include_str!("../assets/styles.css");
    assert!(styles.contains("oklch("));
    assert!(styles.contains("--brand:"));
    assert!(styles.contains("@media (max-width: 760px)"));
    assert!(!styles.contains("background-clip: text"));
    assert!(!styles.contains("-webkit-background-clip: text"));
    assert!(!styles.contains("backdrop-filter"));
    assert!(!styles.contains("border-left: 2px"));
    assert!(!styles.contains("border-left: 3px"));
    assert!(!styles.contains("border-left: 4px"));
    assert!(!styles.contains("border-right: 2px"));
    assert!(!styles.contains("border-right: 3px"));
    assert!(!styles.contains("border-right: 4px"));
}

#[test]
fn embedded_dashboard_shell_contains_runtime_introspection_surfaces() {
    let html = include_str!("../assets/index.html");
    assert!(html.contains("id=\"runtime-status\""));
    assert!(html.contains("id=\"overview-activity\""));
    assert!(html.contains("id=\"graph-map\""));
    assert!(html.contains("id=\"inspector-title\""));
    assert!(html.contains("id=\"inspector-meta\""));
}

#[test]
fn embedded_dashboard_script_renders_contextual_rows_and_topology() {
    let script = include_str!("../assets/app.js");
    assert!(script.contains("renderTopology"));
    assert!(script.contains("selectRecord"));
    assert!(script.contains("renderOperationList"));
    assert!(script.contains("renderRouteList"));
    assert!(script.contains("EventSource"));
}
