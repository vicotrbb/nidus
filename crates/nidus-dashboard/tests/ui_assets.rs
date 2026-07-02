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
    assert!(html.contains("class=\"nav-item active\" data-view=\"home\">Home</button>"));
    assert!(html.contains("class=\"nav-item\" data-view=\"atlas\">Atlas</button>"));
    assert!(html.find("data-view=\"home\"").unwrap() < html.find("data-view=\"atlas\"").unwrap());
    assert!(!html.contains("data-view=\"events\""));
    assert!(!html.contains("data-view=\"jobs\""));
    assert!(html.contains("id=\"home\""));
    assert!(html.contains("id=\"home-runtime\""));
    assert!(html.contains("id=\"home-shape\""));
    assert!(html.contains("id=\"home-activity\""));
    assert!(html.contains("id=\"home-timing\""));
    assert!(html.contains("id=\"home-signals\""));
    assert!(html.contains("id=\"graph-map\""));
    assert!(html.contains("id=\"graph-mode-control\""));
    assert!(html.contains("data-graph-mode=\"structure\""));
    assert!(html.contains("data-graph-mode=\"routes\""));
    assert!(html.contains("data-graph-mode=\"activity\""));
    assert!(!html.contains("id=\"overview-activity\""));
    assert!(!html.contains("id=\"overview-map\""));
    assert!(!html.contains("id=\"module-detail\""));
    assert!(!html.contains("class=\"activity-rail\""));
    assert!(html.contains("id=\"atlas-search\""));
    assert!(html.contains("id=\"timeline-filter\""));
    assert!(html.contains("data-timeline-filter=\"all\""));
    assert!(html.contains("data-timeline-filter=\"events\""));
    assert!(html.contains("data-timeline-filter=\"jobs\""));
    assert!(!html.contains("id=\"events\""));
    assert!(!html.contains("id=\"jobs\""));
    assert!(!html.contains("id=\"events-list\""));
    assert!(!html.contains("id=\"jobs-list\""));
    assert!(html.contains("id=\"inspector-title\""));
    assert!(html.contains("id=\"inspector-meta\""));
    assert!(html.contains("Nidus Runtime Atlas"));
}

#[test]
fn embedded_dashboard_shell_uses_real_nidus_logo_assets() {
    let html = include_str!("../assets/index.html");
    let styles = include_str!("../assets/styles.css");
    let router = include_str!("../src/router.rs");

    assert!(html.contains("rel=\"icon\""));
    assert!(html.contains("./assets/favicon-branded-32.png"));
    assert!(html.contains("rel=\"apple-touch-icon\""));
    assert!(html.contains("./assets/apple-touch-icon.png"));
    assert!(html.contains("class=\"brand-logo\""));
    assert!(html.contains("./assets/logo-mark-square-transparent.png"));
    assert!(!html.contains("class=\"brand-mark\""));
    assert!(styles.contains(".brand-logo"));
    assert!(!styles.contains(".brand-mark span"));
    assert!(router.contains("LOGO_MARK_PNG"));
    assert!(router.contains("FAVICON_BRANDED_32_PNG"));
}

#[test]
fn embedded_dashboard_script_renders_contextual_rows_and_topology() {
    let script = include_str!("../assets/app.js");
    assert!(script.contains("./api/graph"));
    assert!(script.contains("renderGraph"));
    assert!(script.contains("renderHome"));
    assert!(script.contains("homeMetric"));
    assert!(script.contains("humanReadableSettingValue"));
    assert!(script.contains("Metadata only"));
    assert!(script.contains("Payloads redacted"));
    assert!(script.contains("Auth disabled locally"));
    assert!(script.contains("Bearer token"));
    assert!(script.contains("Memory"));
    assert!(script.contains("SQLite"));
    assert!(script.contains("operationTimingSnapshot"));
    assert!(script.contains("state.timelineFilter"));
    assert!(script.contains("filteredTimelineOperations"));
    assert!(script.contains("data-timeline-filter"));
    assert!(!script.contains("#events-list"));
    assert!(!script.contains("#jobs-list"));
    assert!(script.contains("deriveModuleSummaries"));
    assert!(script.contains("layoutTopologyNodes"));
    assert!(script.contains("routeTopologyEdges"));
    assert!(script.contains("moduleNodeMetrics"));
    assert!(script.contains("topologyEdgePath"));
    assert!(script.contains("visibleNodesForMode"));
    assert!(script.contains("visibleEdgesForMode"));
    assert!(script.contains("selectDefaultModule"));
    assert!(script.contains("applySearchSpotlight"));
    assert!(!script.contains("renderSelectedModuleDetail"));
    assert!(script.contains("truncateMiddle"));
    assert!(script.contains("activity-signal"));
    assert!(script.contains("graphMode"));
    assert!(script.contains("module_import"));
    assert!(script.contains("focusRelations"));
    assert!(script.contains("renderNodeInspector"));
    assert!(script.contains("Imports / exports"));
    assert!(script.contains("Recent activity"));
    assert!(script.contains("Raw JSON"));
    assert!(script.contains("selectRecord"));
    assert!(script.contains("renderOperationList"));
    assert!(script.contains("renderRouteList"));
    assert!(script.contains("EventSource"));
}

#[test]
fn embedded_dashboard_assets_pin_runtime_circuit_map_language() {
    let styles = include_str!("../assets/styles.css");
    let script = include_str!("../assets/app.js");

    assert!(styles.contains(".graph-node-module"));
    assert!(styles.contains(".topology-port"));
    assert!(styles.contains(".module-metrics"));
    assert!(styles.contains(".activity-signal"));
    assert!(styles.contains(".graph-map.is-outline"));
    assert!(styles.contains("[data-mode=\"routes\"]"));
    assert!(styles.contains("[data-mode=\"activity\"]"));

    assert!(!styles.contains("border-radius: 38%"));
    assert!(!styles.contains("border-radius: 43%"));
    assert!(!styles.contains("radial-gradient(circle"));

    assert!(script.contains("document.createElementNS(\"http://www.w3.org/2000/svg\", \"path\")"));
    assert!(!script.contains("document.createElementNS(\"http://www.w3.org/2000/svg\", \"line\")"));
    assert!(script.contains("data-role\", \"primary-label\""));
    assert!(script.contains("moduleImportEdgePath"));
    assert!(!script.contains("outsideX = source.x > 60 ? 100 : 0"));
}

#[test]
fn embedded_dashboard_routes_topology_edges_from_measured_node_ports() {
    let script = include_str!("../assets/app.js");

    assert!(script.contains("measureTopologyNodes"));
    assert!(script.contains("edgeAnchorPoint"));
    assert!(script.contains("topologyEdgePath(edge, anchors)"));
    assert!(!script.contains("setAttribute(\"viewBox\", \"0 0 100 100\")"));
    assert!(!script.contains("function topologyHalfWidth"));
}
