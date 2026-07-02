const views = document.querySelectorAll(".view");
const buttons = document.querySelectorAll(".nav-item");
const homeLinks = document.querySelectorAll("[data-home-target]");
const overviewService = document.querySelector("#overview-service");
const runtimeSummary = document.querySelector("#runtime-summary");
const homeRuntime = document.querySelector("#home-runtime");
const homeShape = document.querySelector("#home-shape");
const homeActivity = document.querySelector("#home-activity");
const homeTiming = document.querySelector("#home-timing");
const homeSignals = document.querySelector("#home-signals");
const graphMap = document.querySelector("#graph-map");
const graphModeControl = document.querySelector("#graph-mode-control");
const graphModeButtons = document.querySelectorAll("[data-graph-mode]");
const routesList = document.querySelector("#routes-list");
const timelineList = document.querySelector("#timeline-list");
const timelineFilterButtons = document.querySelectorAll("[data-timeline-filter]");
const adaptersList = document.querySelector("#adapters-list");
const settingsList = document.querySelector("#settings-list");
const inspectorTitle = document.querySelector("#inspector-title");
const inspectorMeta = document.querySelector("#inspector-meta");
const inspector = document.querySelector("#inspector-output");
const runtimeStatus = document.querySelector("#runtime-status");
const status = document.querySelector("#connection-status");
const routeCount = document.querySelector("#route-count");
const atlasSearch = document.querySelector("#atlas-search");

const state = {
  overview: { service_name: "nidus-app", metrics: [] },
  graph: { service_name: "nidus-app", nodes: [], edges: [], groups: [] },
  routes: [],
  timeline: [],
  events: [],
  jobs: [],
  adapters: [],
  settings: {},
  selected: null,
  graphMode: "structure",
  timelineFilter: "all",
  query: "",
  latestOperationId: null,
};

let activeViewTransition = null;
const mobileQuery = window.matchMedia("(max-width: 760px)");
document.body.dataset.activeView = "home";

for (const button of buttons) {
  button.addEventListener("click", () => activateView(button.dataset.view));
}

for (const button of homeLinks) {
  button.addEventListener("click", () => activateView(button.dataset.homeTarget));
}

atlasSearch.addEventListener("input", () => {
  state.query = atlasSearch.value.trim().toLowerCase();
  renderAll();
});

for (const button of graphModeButtons) {
  button.addEventListener("click", () => {
    state.graphMode = button.dataset.graphMode;
    if (state.graphMode === "routes") ensureRouteSourceSelection();
    renderAll();
  });
}

for (const button of timelineFilterButtons) {
  button.addEventListener("click", () => {
    state.timelineFilter = button.dataset.timelineFilter;
    renderAll();
  });
}

mobileQuery.addEventListener("change", () => renderGraph());

function activateView(id) {
  const apply = () => {
    for (const item of buttons) item.classList.toggle("active", item.dataset.view === id);
    for (const view of views) view.classList.toggle("active", view.id === id);
    document.body.dataset.activeView = id;
  };
  runViewTransition(apply);
}

async function getJson(path) {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`${path} returned ${response.status}`);
  return response.json();
}

function setConnectionState(nextState) {
  runtimeStatus.dataset.state = nextState;
  status.textContent = nextState;
}

function prefersReducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function runViewTransition(apply) {
  if (!document.startViewTransition || prefersReducedMotion() || mobileQuery.matches) {
    apply();
    return;
  }
  if (activeViewTransition) activeViewTransition.skipTransition();
  const transition = document.startViewTransition(apply);
  activeViewTransition = transition;
  transition.finished.finally(() => {
    if (activeViewTransition === transition) activeViewTransition = null;
  });
}

function clear(node) {
  node.replaceChildren();
}

function textNode(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.textContent = text;
  return node;
}

function badge(text, className = "badge") {
  return textNode("span", className, text);
}

function formatKind(value) {
  return String(value ?? "record").replaceAll("_", " ");
}

function formatTime(timestampMs) {
  if (!timestampMs) return "time unavailable";
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(timestampMs));
}

function formatDuration(durationMs) {
  if (durationMs === null || durationMs === undefined) return "duration unknown";
  return `${durationMs} ms`;
}

function graphNode(id) {
  return state.graph.nodes.find((node) => node.id === id);
}

function graphEdgesFor(id) {
  return state.graph.edges.filter((edge) => edge.source === id || edge.target === id);
}

function moduleNodes(graph = state.graph) {
  return graph.nodes.filter((node) => node.kind === "module");
}

function runtimeNode(graph = state.graph) {
  return graph.nodes.find((node) => node.kind === "runtime") ?? graph.nodes[0];
}

function moduleNodeId(name) {
  return `module:${name}`;
}

function selectedNode() {
  return state.selected?.node ?? graphNode(state.selected?.id);
}

function moduleIdForNode(node) {
  if (!node) return null;
  if (node.kind === "module") return node.id;
  const moduleName = node.metadata?.module ?? node.metadata?.operation?.attributes?.module;
  return moduleName ? moduleNodeId(moduleName) : null;
}

function selectedModuleId() {
  const selectedModule = moduleIdForNode(selectedNode());
  if (selectedModule) return selectedModule;
  return selectDefaultModule()?.id ?? null;
}

function nodeMatches(node, query = state.query) {
  if (!query) return true;
  const haystack = [
    node.kind,
    node.label,
    node.summary,
    Object.keys(node.counts ?? {}).join(" "),
    JSON.stringify(node.metadata ?? {}),
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(query);
}

function selectDefaultModule(graph = state.graph) {
  const modules = moduleNodes(graph);
  if (modules.length === 0) return runtimeNode(graph);
  const appModule = modules.find((node) => node.label === "AppModule");
  if (appModule) return appModule;
  const degree = new Map(modules.map((node) => [node.id, 0]));
  for (const edge of graph.edges) {
    if (degree.has(edge.source)) degree.set(edge.source, degree.get(edge.source) + 1);
    if (degree.has(edge.target)) degree.set(edge.target, degree.get(edge.target) + 1);
  }
  return [...modules].sort((left, right) => (degree.get(right.id) ?? 0) - (degree.get(left.id) ?? 0))[0];
}

function deriveModuleSummaries(graph = state.graph, timeline = state.timeline) {
  const summaries = new Map();
  for (const node of moduleNodes(graph)) {
    summaries.set(node.id, {
      module: node,
      imports: [...(node.metadata?.imports ?? [])],
      exports: [...(node.metadata?.exports ?? [])],
      providers: [],
      controllers: [],
      routes: [],
      activity: [],
    });
  }

  for (const node of graph.nodes) {
    const moduleId = moduleIdForNode(node);
    const summary = summaries.get(moduleId);
    if (!summary) continue;
    if (node.kind === "provider") summary.providers.push(node);
    if (node.kind === "controller") summary.controllers.push(node);
    if (node.kind === "route") summary.routes.push(node);
    if (["event", "job", "adapter"].includes(node.kind)) summary.activity.push(node);
  }

  for (const operation of timeline) {
    const moduleName = operation.attributes?.module;
    const summary = moduleName ? summaries.get(moduleNodeId(moduleName)) : null;
    if (summary && !summary.activity.some((item) => item.id === operation.id || item.metadata?.operation?.id === operation.id)) {
      summary.activity.push(operation);
    }
  }

  return summaries;
}

function moduleBadgeFacts(summary) {
  return [
    ["prov", summary.providers.length],
    ["ctrl", summary.controllers.length],
    ["routes", summary.routes.length],
  ];
}

function moduleNodeMetrics(summary) {
  return moduleBadgeFacts(summary).map(([label, value]) => ({ label, value }));
}

function nodeMatchesModuleSummary(node, summaries, query = state.query) {
  if (!query || node.kind !== "module") return false;
  const summary = summaries.get(node.id);
  if (!summary) return false;
  const haystack = [
    summary.imports.join(" "),
    summary.exports.join(" "),
    summary.providers.map((item) => item.label).join(" "),
    summary.controllers.map((item) => item.label).join(" "),
    summary.routes.map((item) => `${item.metadata?.method ?? ""} ${item.metadata?.path ?? item.label}`).join(" "),
    summary.activity.map((item) => item.name ?? item.label ?? item.metadata?.operation?.name).join(" "),
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(query);
}

function visibleNodesForMode(mode = state.graphMode, graph = state.graph) {
  const visible = new Map();
  const runtime = runtimeNode(graph);
  if (runtime) visible.set(runtime.id, runtime);

  for (const node of moduleNodes(graph)) visible.set(node.id, node);

  const summary = deriveModuleSummaries(graph).get(selectedModuleId());
  if (summary && mode === "routes") {
    for (const node of [...summary.controllers, ...summary.routes]) visible.set(node.id, node);
  }

  return [...visible.values()];
}

function visibleEdgesForMode(mode = state.graphMode, graph = state.graph) {
  const visible = new Set(visibleNodesForMode(mode, graph).map((node) => node.id));
  const selected = selectedModuleId();
  return graph.edges.filter((edge) => {
    if (!visible.has(edge.source) || !visible.has(edge.target)) return false;
    if (edge.kind === "runtime_module") return true;
    if (edge.kind === "module_import") return true;
    if (mode === "routes") {
      if (edge.kind === "module_controller") return edge.source === selected;
      if (edge.kind === "controller_route") {
        const controller = graph.nodes.find((node) => node.id === edge.source);
        return moduleIdForNode(controller) === selected;
      }
    }
    return false;
  });
}

function applySearchSpotlight(root, nodes, summaries = deriveModuleSummaries()) {
  root.classList.toggle("is-searching", Boolean(state.query));
  for (const nodeElement of root.querySelectorAll("[data-node-id]")) {
    const node = nodes.find((item) => item.id === nodeElement.dataset.nodeId);
    const matched = Boolean(state.query && node && (nodeMatches(node) || nodeMatchesModuleSummary(node, summaries)));
    nodeElement.classList.toggle("is-search-match", matched);
    nodeElement.classList.toggle("is-search-muted", Boolean(state.query && !matched));
  }
}

function graphCounts() {
  const counts = new Map();
  for (const node of state.graph.nodes) {
    counts.set(node.kind, (counts.get(node.kind) ?? 0) + 1);
  }
  return counts;
}

function renderRuntimeSummary() {
  const counts = graphCounts();
  overviewService.textContent = state.graph.service_name ?? state.overview.service_name ?? "nidus-app";
  runtimeSummary.textContent = [
    `${counts.get("module") ?? 0} modules`,
    `${counts.get("controller") ?? 0} controllers`,
    `${counts.get("provider") ?? 0} providers`,
    `${counts.get("route") ?? state.routes.length} routes`,
    `${state.timeline.length} timeline records`,
  ].join(" / ");
  routeCount.textContent = `${state.routes.length} routes`;
}

function humanReadableSettingValue(key, fallback = "Unknown") {
  const value = state.settings?.[key];
  const displayValues = {
    auth_mode: {
      bearer: "Bearer token",
      bearer_token: "Bearer token",
      unsafe_disabled_for_local_development: "Auth disabled locally",
    },
    capture_mode: {
      metadata_only: "Metadata only",
      payloads_redacted: "Payloads redacted",
    },
    storage_mode: {
      memory: "Memory",
      sqlite: "SQLite",
    },
  };
  const mapped = displayValues[key]?.[value];
  if (mapped) return mapped;
  if (!value) return fallback;
  const words = String(value).replaceAll("_", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function statusCount(statusName) {
  return state.timeline.filter((operation) => operation.status === statusName).length;
}

function knownDurationOperations() {
  return state.timeline.filter((operation) => Number.isFinite(operation.duration_ms));
}

function operationTimingSnapshot() {
  const known = knownDurationOperations();
  const latest = known[0];
  const total = known.reduce((sum, operation) => sum + operation.duration_ms, 0);
  const slowest = known.reduce((current, operation) => {
    if (!current) return operation;
    return operation.duration_ms > current.duration_ms ? operation : current;
  }, null);

  return {
    average: known.length ? Math.round(total / known.length) : null,
    slowest: slowest?.duration_ms,
    latest: latest?.duration_ms,
    count: known.length,
  };
}

function homeMetric(label, value, hint) {
  const group = document.createElement("div");
  group.className = "home-metric";
  const term = document.createElement("dt");
  term.textContent = label;
  const description = document.createElement("dd");
  description.appendChild(textNode("strong", null, String(value)));
  if (hint) description.appendChild(textNode("span", null, hint));
  group.append(term, description);
  return group;
}

function renderHomeMetrics(target, metrics) {
  clear(target);
  for (const metric of metrics) {
    target.appendChild(homeMetric(metric.label, metric.value, metric.hint));
  }
}

function renderHomeSignals() {
  clear(homeSignals);
  const latest = state.timeline.slice(0, 5);
  if (latest.length === 0) {
    showEmpty(homeSignals, "Recent runtime records appear here after the app records activity.");
    return;
  }
  for (const operation of latest) {
    const item = document.createElement("li");
    item.className = "record-row compact-signal";
    item.append(
      textNode("strong", "row-title", operation.name),
      textNode("span", "route-summary", `${formatKind(operation.kind)} / ${operation.status}`),
      textNode("span", "route-summary", formatDuration(operation.duration_ms)),
    );
    selectableRow(
      item,
      operation,
      operation.name,
      `${formatKind(operation.kind)} / ${operation.status}`,
      operationNodeId(operation),
    );
    homeSignals.appendChild(item);
  }
}

function renderHome() {
  const counts = graphCounts();
  const timing = operationTimingSnapshot();
  renderHomeMetrics(homeRuntime, [
    { label: "Service", value: state.graph.service_name ?? state.overview.service_name ?? "nidus-app" },
    { label: "Connection", value: status.textContent || "connecting" },
    { label: "Storage", value: humanReadableSettingValue("storage_mode") },
    { label: "Capture", value: humanReadableSettingValue("capture_mode") },
    { label: "Auth", value: humanReadableSettingValue("auth_mode") },
  ]);
  renderHomeMetrics(homeShape, [
    { label: "Modules", value: counts.get("module") ?? 0 },
    { label: "Controllers", value: counts.get("controller") ?? 0 },
    { label: "Providers", value: counts.get("provider") ?? 0 },
    { label: "Routes", value: counts.get("route") ?? state.routes.length },
    { label: "Adapters", value: counts.get("adapter") ?? state.adapters.length },
  ]);
  renderHomeMetrics(homeActivity, [
    { label: "Timeline records", value: state.timeline.length },
    { label: "Success", value: statusCount("success") },
    { label: "Failure", value: statusCount("failure") },
    { label: "Running", value: statusCount("running") },
    { label: "Recent events", value: state.events.length },
    { label: "Recent jobs", value: state.jobs.length },
  ]);
  renderHomeMetrics(homeTiming, [
    { label: "Average duration", value: formatDuration(timing.average), hint: "known operations" },
    { label: "Slowest duration", value: formatDuration(timing.slowest), hint: "operation timing" },
    { label: "Latest duration", value: formatDuration(timing.latest), hint: "latest known" },
    { label: "Known durations", value: timing.count, hint: "operation count" },
  ]);
  renderHomeSignals();
}

function laneFor(node) {
  if (node.kind === "runtime") return "runtime";
  if (node.kind === "module") return "modules";
  if (node.kind === "provider" || node.kind === "controller") return "components";
  if (node.kind === "route") return "routes";
  return "modules";
}

function distributeY(count, top = 18, bottom = 82) {
  if (count <= 1) return [50];
  return Array.from({ length: count }, (_, index) => top + (index * (bottom - top)) / Math.max(1, count - 1));
}

function orderModulesForTopology(nodes) {
  const selected = selectedModuleId();
  return [...nodes].sort((left, right) => {
    if (left.id === selected) return -1;
    if (right.id === selected) return 1;
    if (left.label === "AppModule") return -1;
    if (right.label === "AppModule") return 1;
    return left.label.localeCompare(right.label);
  });
}

function placeStructureModules(positions, modules) {
  const ordered = orderModulesForTopology(modules);
  if (ordered.length < 4) {
    const ySlots = distributeY(ordered.length, 30, 70);
    ordered.forEach((node, index) => positions.set(node.id, { x: 58, y: ySlots[index] }));
    return;
  }

  const slots = [
    { x: 45, y: 50 },
    { x: 83, y: 32 },
    { x: 83, y: 68 },
    { x: 45, y: 24 },
    { x: 45, y: 76 },
    { x: 83, y: 18 },
    { x: 83, y: 82 },
    { x: 45, y: 14 },
    { x: 45, y: 86 },
  ];
  ordered.forEach((node, index) => {
    const slot = slots[index] ?? {
      x: index % 2 === 0 ? 45 : 83,
      y: 14 + ((Math.floor(index / 2) * 14) % 72),
    };
    positions.set(node.id, slot);
  });
}

function placeRouteModules(positions, modules) {
  const ordered = orderModulesForTopology(modules);
  const [selected, ...rest] = ordered;
  if (selected) positions.set(selected.id, { x: 45, y: 50 });
  const ySlots = [18, 34, 66, 82, 10, 90];
  rest.forEach((node, index) => positions.set(node.id, { x: 45, y: ySlots[index] ?? 18 + ((index * 16) % 64) }));
}

function placeLane(positions, nodes, x, top = 18, bottom = 82) {
  const ySlots = distributeY(nodes.length, top, bottom);
  nodes.forEach((node, index) => positions.set(node.id, { x, y: ySlots[index] }));
}

function layoutTopologyNodes(nodes, summaries = deriveModuleSummaries(), mode = state.graphMode) {
  const positions = new Map();
  const runtime = nodes.find((node) => node.kind === "runtime");
  const modules = nodes.filter((node) => node.kind === "module");
  const controllers = nodes.filter((node) => node.kind === "controller");
  const routes = nodes.filter((node) => node.kind === "route");
  if (runtime) positions.set(runtime.id, { x: 14, y: 50 });

  if (mode === "routes") {
    placeRouteModules(positions, modules);
    if (controllers.length === 1) {
      positions.set(controllers[0].id, { x: 76, y: 30 });
    } else {
      placeLane(positions, controllers, 76, 20, 80);
    }
    placeLane(positions, routes, 84, 12, 88);
    return positions;
  }

  placeStructureModules(positions, modules, summaries);
  return positions;
}

function topologyHeight(nodes, mode = state.graphMode) {
  const laneCounts = nodes.reduce(
    (counts, node) => {
      counts[laneFor(node)] = (counts[laneFor(node)] ?? 0) + 1;
      return counts;
    },
    { runtime: 0, modules: 0, components: 0, routes: 0 },
  );
  const laneMax = Math.max(...Object.values(laneCounts));
  if (mode === "routes") return Math.max(760, laneMax * 150);
  return Math.max(700, laneMax * 112);
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function pointFromRect(rect, graphRect) {
  return {
    x: rect.left + rect.width / 2 - graphRect.left,
    y: rect.top + rect.height / 2 - graphRect.top,
  };
}

function measureTopologyNodes() {
  const graphRect = graphMap.getBoundingClientRect();
  const anchors = new Map();
  for (const node of graphMap.querySelectorAll("[data-node-id]")) {
    const rect = node.getBoundingClientRect();
    const input = node.querySelector(".topology-port-input")?.getBoundingClientRect() ?? rect;
    const output = node.querySelector(".topology-port-output")?.getBoundingClientRect() ?? rect;
    anchors.set(node.dataset.nodeId, {
      center: pointFromRect(rect, graphRect),
      input: pointFromRect(input, graphRect),
      output: pointFromRect(output, graphRect),
      rect: {
        left: rect.left - graphRect.left,
        right: rect.right - graphRect.left,
        top: rect.top - graphRect.top,
        bottom: rect.bottom - graphRect.top,
        width: rect.width,
        height: rect.height,
      },
    });
  }
  return {
    width: graphRect.width,
    height: graphRect.height,
    anchors,
  };
}

function edgeAnchorPoint(anchor, side) {
  return side === "input" ? anchor.input : anchor.output;
}

function roundedPoint(point) {
  return {
    x: Math.round(point.x * 10) / 10,
    y: Math.round(point.y * 10) / 10,
  };
}

function orthogonalPath(start, end, busX) {
  const a = roundedPoint(start);
  const b = roundedPoint(end);
  const x = Math.round(busX * 10) / 10;
  return `M ${a.x} ${a.y} L ${x} ${a.y} L ${x} ${b.y} L ${b.x} ${b.y}`;
}

function forwardBusX(start, end, canvas) {
  const minGap = 32;
  if (end.x > start.x + minGap) {
    return clamp((start.x + end.x) / 2, start.x + minGap, end.x - minGap);
  }
  return clamp(Math.max(start.x, end.x) + 56, Math.max(start.x, end.x) + minGap, canvas.width - 24);
}

function moduleImportEdgePath(source, target, canvas) {
  const start = edgeAnchorPoint(source, "output");
  const end = edgeAnchorPoint(target, "input");
  return orthogonalPath(start, end, forwardBusX(start, end, canvas));
}

function topologyEdgePath(edge, anchors) {
  const source = anchors.anchors.get(edge.source);
  const target = anchors.anchors.get(edge.target);
  if (!source || !target) return null;
  const start = edgeAnchorPoint(source, "output");
  const end = edgeAnchorPoint(target, "input");

  if (edge.kind === "module_import") {
    return moduleImportEdgePath(source, target, anchors);
  }

  if (edge.kind === "module_controller" || edge.kind === "controller_route") {
    return orthogonalPath(start, end, forwardBusX(start, end, anchors));
  }

  return orthogonalPath(start, end, forwardBusX(start, end, anchors));
}

function routeTopologyEdges(edges, anchors) {
  return edges
    .map((edge) => {
      const path = topologyEdgePath(edge, anchors);
      return path ? { edge, path } : null;
    })
    .filter(Boolean);
}

function renderGraph() {
  clear(graphMap);
  const summaries = deriveModuleSummaries();
  const nodes = visibleNodesForMode();
  const visible = new Set(nodes.map((node) => node.id));
  const edges = visibleEdgesForMode();
  graphMap.dataset.mode = state.graphMode;
  graphMap.classList.toggle("is-outline", mobileQuery.matches);
  for (const button of graphModeButtons) {
    button.classList.toggle("active", button.dataset.graphMode === state.graphMode);
  }
  if (graphModeControl) graphModeControl.dataset.mode = state.graphMode;

  if (nodes.length === 0) {
    graphMap.appendChild(emptyState("Graph data appears here after the Nidus app records its module graph."));
    return;
  }

  if (mobileQuery.matches) {
    graphMap.style.minHeight = "";
    renderGraphOutline(nodes, summaries);
    syncSelectionState();
    focusRelations(state.selected?.id);
    applySearchSpotlight(graphMap, nodes, summaries);
    return;
  }

  graphMap.style.minHeight = `${topologyHeight(nodes)}px`;
  const positions = layoutTopologyNodes(nodes, summaries);
  for (const node of nodes) {
    const position = positions.get(node.id);
    const button = graphButton(node, summaries);
    button.style.left = `${position.x}%`;
    button.style.top = `${position.y}%`;
    graphMap.appendChild(button);
  }

  syncSelectionState();
  const anchors = measureTopologyNodes();
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("graph-edges");
  svg.setAttribute("viewBox", `0 0 ${Math.round(anchors.width)} ${Math.round(anchors.height)}`);
  svg.setAttribute("preserveAspectRatio", "none");

  for (const routed of routeTopologyEdges(edges, anchors)) {
    if (!visible.has(routed.edge.source) || !visible.has(routed.edge.target)) continue;
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.dataset.edgeId = routed.edge.id;
    path.dataset.source = routed.edge.source;
    path.dataset.target = routed.edge.target;
    path.dataset.kind = routed.edge.kind;
    path.setAttribute("d", routed.path);
    path.setAttribute("fill", "none");
    svg.appendChild(path);
  }
  graphMap.prepend(svg);

  focusRelations(state.selected?.id);
  applySearchSpotlight(graphMap, nodes, summaries);
}

function renderGraphOutline(nodes, summaries) {
  const order = ["runtime", "module", "controller", "route"];
  for (const kind of order) {
    const groupNodes = nodes.filter((node) => node.kind === kind);
    if (groupNodes.length === 0) continue;
    const group = document.createElement("section");
    group.className = "outline-group";
    group.appendChild(textNode("h3", "outline-title", formatKind(kind)));
    for (const node of groupNodes) group.appendChild(graphButton(node, summaries));
    graphMap.appendChild(group);
  }
}

function graphButton(node, summaries = deriveModuleSummaries()) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `graph-node graph-node-${node.kind}`;
  button.dataset.nodeId = node.id;
  button.dataset.kind = node.kind;
  button.dataset.selectionKey = node.id;
  button.setAttribute("aria-label", `${formatKind(node.kind)} ${node.label}`);
  button.title = node.label;
  const label = node.kind === "route" ? truncateMiddle(node.label, 34) : node.label;
  const summary = node.summary ?? countSummary(node.counts);
  const primaryLabel = textNode("strong", null, label);
  primaryLabel.setAttribute("data-role", "primary-label");
  button.append(
    topologyPort("input"),
    badge(formatKind(node.kind), "node-kind"),
    primaryLabel,
    textNode("span", "node-summary", summary),
    topologyPort("output"),
  );
  if (node.kind === "module") {
    const summary = summaries.get(node.id);
    if (summary) {
      const facts = document.createElement("span");
      facts.className = "module-metrics";
      for (const { label, value } of moduleNodeMetrics(summary)) {
        facts.appendChild(badge(`${value} ${label}`, "module-metric"));
      }
      if (summary.activity.length > 0) facts.appendChild(activitySignal(summary.activity.length));
      button.appendChild(facts);
    }
  }
  button.addEventListener("click", () => selectNode(node));
  button.addEventListener("focus", () => focusRelations(node.id));
  button.addEventListener("mouseenter", () => focusRelations(node.id));
  button.addEventListener("mouseleave", () => focusRelations(state.selected?.id));
  button.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectNode(node);
    }
  });
  return button;
}

function topologyPort(side) {
  const port = document.createElement("span");
  port.className = `topology-port topology-port-${side}`;
  port.setAttribute("aria-hidden", "true");
  return port;
}

function activitySignal(count) {
  const signal = textNode("span", "activity-signal", String(count));
  signal.title = `${count} recent activity records`;
  return signal;
}

function truncateMiddle(value, maxLength = 42) {
  const text = String(value ?? "");
  if (text.length <= maxLength) return text;
  const keep = Math.max(6, Math.floor((maxLength - 1) / 2));
  return `${text.slice(0, keep)}...${text.slice(-keep)}`;
}

function countSummary(counts = {}) {
  const entries = Object.entries(counts);
  if (entries.length === 0) return "inspectable node";
  return entries.map(([key, value]) => `${value} ${key.replaceAll("_", " ")}`).join(" / ");
}

function focusRelations(id) {
  if (graphNode(id)?.kind === "runtime") id = null;
  const related = new Set();
  if (id) {
    related.add(id);
    for (const edge of state.graph.edges) {
      if (edge.source === id || edge.target === id) {
        related.add(edge.source);
        related.add(edge.target);
      }
    }
  }

  for (const node of graphMap.querySelectorAll("[data-node-id]")) {
    const active = !id || related.has(node.dataset.nodeId);
    node.classList.toggle("is-related", Boolean(id && active));
    node.classList.toggle("is-dimmed", Boolean(id && !active));
  }
  for (const edge of graphMap.querySelectorAll("[data-edge-id]")) {
    const active = id && (edge.dataset.source === id || edge.dataset.target === id);
    edge.classList.toggle("is-related", Boolean(active));
    edge.classList.toggle("is-dimmed", Boolean(id && !active));
  }
}

function setSelectedNode(node) {
  state.selected = { id: node.id, type: "node", node };
  inspectorTitle.textContent = node.label;
  inspectorMeta.textContent = formatKind(node.kind);
  renderNodeInspector(node);
  document.querySelector(".inspector")?.classList.add("has-selection");
}

function selectNode(node) {
  setSelectedNode(node);
  syncSelectionState();
  focusRelations(node.id);
  animateInspector();
}

function ensureDefaultSelection() {
  if (state.selected?.type === "node" && graphNode(state.selected.id)) return;
  if (state.selected?.type === "record") return;
  const node = selectDefaultModule();
  if (node) setSelectedNode(node);
}

function ensureRouteSourceSelection() {
  const summaries = deriveModuleSummaries();
  const selectedSummary = summaries.get(selectedModuleId());
  if (selectedSummary && (selectedSummary.controllers.length > 0 || selectedSummary.routes.length > 0)) return;
  const routeSource = [...summaries.values()].find((summary) => summary.controllers.length > 0 || summary.routes.length > 0);
  if (routeSource) setSelectedNode(routeSource.module);
}

function selectRecord(record, title, meta, key) {
  const node = key ? graphNode(key) : null;
  if (node) {
    selectNode(node);
    return;
  }
  state.selected = { id: key, type: "record", record };
  inspectorTitle.textContent = title;
  inspectorMeta.textContent = meta;
  renderRecordInspector(record);
  document.querySelector(".inspector")?.classList.add("has-selection");
  syncSelectionState();
  focusRelations(null);
  animateInspector();
}

function syncSelectionState() {
  for (const node of document.querySelectorAll("[data-selection-key]")) {
    node.classList.toggle("selected", node.dataset.selectionKey === state.selected?.id);
  }
}

function animateInspector() {
  if (prefersReducedMotion() || !inspector.animate) return;
  inspector.animate(
    [
      { opacity: 0.82, transform: "translateY(5px)" },
      { opacity: 1, transform: "translateY(0)" },
    ],
    { duration: 220, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
  );
}

function renderNodeInspector(node) {
  clear(inspector);
  inspector.appendChild(detailSummary(node.summary ?? countSummary(node.counts)));

  if (node.kind === "module") {
    const summary = deriveModuleSummaries().get(node.id);
    if (summary) {
      inspector.append(
        inspectorSection("Imports / exports", [
          ...summary.imports.map((item) => ["imports", item]),
          ...summary.exports.map((item) => ["exports", item]),
        ]),
        inspectorSection(
          "Providers",
          summary.providers.map((item) => [item.metadata?.exported ? "exported" : "local", item.label]),
        ),
        inspectorSection(
          "Controllers",
          summary.controllers.map((item) => [`${item.counts?.routes ?? 0} routes`, item.label]),
        ),
        inspectorSection(
          "Routes",
          summary.routes.map((item) => [
            item.metadata?.method ?? "route",
            truncateMiddle(item.metadata?.path ?? item.label, 48),
          ]),
        ),
        inspectorSection(
          "Recent activity",
          summary.activity.slice(0, 8).map((item) => [
            formatKind(item.kind ?? item.metadata?.operation?.kind ?? "activity"),
            item.name ?? item.label ?? item.metadata?.operation?.name,
          ]),
        ),
      );
    }
  } else if (node.kind === "route") {
    inspector.append(
      keyValues(kindDetails(node)),
      inspectorSection("Route anatomy", [
        ["module", node.metadata?.module],
        ["controller", node.metadata?.controller],
        ["method", node.metadata?.method],
        ["path", node.metadata?.path],
      ]),
    );
  } else if (["event", "job", "adapter"].includes(node.kind)) {
    inspector.append(
      keyValues(kindDetails(node)),
      inspectorSection("Recent activity", [
        ["status", node.metadata?.operation?.status ?? node.status],
        ["correlation", node.metadata?.operation?.correlation_id ?? "none"],
        ["duration", formatDuration(node.metadata?.operation?.duration_ms)],
      ]),
    );
  } else {
    inspector.appendChild(keyValues(kindDetails(node)));
  }

  const metadata = node.metadata?.operation ?? node.metadata;
  inspector.appendChild(textNode("h3", "inspector-subhead", "Raw JSON"));
  inspector.appendChild(codeBlock(metadata));
}

function inspectorSection(title, rows) {
  const section = document.createElement("section");
  section.className = "inspector-section";
  section.appendChild(textNode("h3", "inspector-subhead", title));
  const normalized = rows.filter(([, value]) => value !== null && value !== undefined && value !== "");
  if (normalized.length === 0) {
    section.appendChild(textNode("p", "muted-line", "none"));
    return section;
  }
  const list = document.createElement("ol");
  list.className = "relation-list";
  for (const [kind, value] of normalized) {
    const item = document.createElement("li");
    item.append(badge(formatKind(kind), "relation-kind"), textNode("span", null, String(value)));
    list.appendChild(item);
  }
  section.appendChild(list);
  return section;
}

function kindDetails(node) {
  const metadata = node.metadata ?? {};
  if (node.kind === "module") {
    return {
      Imports: listText(metadata.imports),
      Providers: listText(metadata.providers),
      Controllers: listText(metadata.controllers),
      Exports: listText(metadata.exports),
    };
  }
  if (node.kind === "controller") {
    return {
      Module: metadata.module,
      Prefix: metadata.prefix,
      Routes: `${node.counts?.routes ?? 0}`,
    };
  }
  if (node.kind === "route") {
    return {
      Method: metadata.method,
      Path: metadata.path,
      Controller: metadata.controller,
      Guards: listText(metadata.guards),
      Pipes: listText(metadata.pipes),
      Validates: String(Boolean(metadata.validates)),
    };
  }
  if (node.kind === "provider") {
    return {
      Module: metadata.module,
      Exported: String(Boolean(metadata.exported)),
    };
  }
  if (["event", "job", "adapter"].includes(node.kind)) {
    const operation = metadata.operation ?? {};
    return {
      Status: operation.status ?? node.status,
      Time: formatTime(operation.timestamp_ms),
      Duration: formatDuration(operation.duration_ms),
      Correlation: operation.correlation_id ?? "none",
    };
  }
  return {
    Status: node.status ?? "ready",
    Counts: countSummary(node.counts),
  };
}

function listText(value) {
  if (Array.isArray(value)) return value.length ? value.join(", ") : "none";
  if (value === null || value === undefined) return "none";
  return String(value);
}

function keyValues(values) {
  const dl = document.createElement("dl");
  dl.className = "inspector-kv";
  for (const [key, value] of Object.entries(values)) {
    dl.append(textNode("dt", null, key), textNode("dd", null, value ?? "none"));
  }
  return dl;
}

function detailSummary(text) {
  const node = document.createElement("p");
  node.className = "inspector-summary";
  node.textContent = text;
  return node;
}

function codeBlock(value) {
  const pre = document.createElement("pre");
  pre.textContent = JSON.stringify(value ?? {}, null, 2);
  return pre;
}

function renderRecordInspector(record) {
  clear(inspector);
  inspector.appendChild(detailSummary(record.summary ?? record.name ?? record.path ?? "runtime record"));
  if (record.method && record.path) {
    inspector.appendChild(
      inspectorSection("Route anatomy", [
        ["method", record.method],
        ["path", record.path],
        ["summary", record.summary],
      ]),
    );
  } else {
    inspector.appendChild(
      inspectorSection("Recent activity", [
        ["kind", record.kind],
        ["status", record.status],
        ["time", formatTime(record.timestamp_ms)],
        ["duration", formatDuration(record.duration_ms)],
        ["correlation", record.correlation_id ?? "none"],
      ]),
    );
  }
  inspector.appendChild(textNode("h3", "inspector-subhead", "Raw JSON"));
  inspector.appendChild(codeBlock(record));
}

function selectableRow(item, record, title, meta, key) {
  item.tabIndex = 0;
  item.dataset.selectionKey = key;
  item.addEventListener("click", () => selectRecord(record, title, meta, key));
  item.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectRecord(record, title, meta, key);
    }
  });
  return item;
}

function emptyState(text) {
  const item = document.createElement("div");
  item.className = "empty-state";
  item.textContent = text;
  return item;
}

function showEmpty(list, text) {
  clear(list);
  const item = document.createElement("li");
  item.className = "empty-state";
  item.textContent = text;
  list.appendChild(item);
}

function routeNodeId(route) {
  return `route:${route.method} ${route.path}`;
}

function operationNodeId(operation) {
  return `operation:${operation.id}`;
}

function renderRouteList(list, routes, emptyText) {
  clear(list);
  const filtered = routes.filter((route) =>
    [route.method, route.path, route.summary].join(" ").toLowerCase().includes(state.query),
  );
  if (filtered.length === 0) {
    showEmpty(list, emptyText);
    return;
  }
  for (const route of filtered) {
    const item = document.createElement("li");
    item.className = "route-row";
    item.append(
      badge(route.method, "method"),
      textNode("strong", "row-title", route.path),
      textNode("span", "route-summary", route.summary ?? "controller route"),
    );
    selectableRow(item, route, `${route.method} ${route.path}`, "route snapshot", routeNodeId(route));
    list.appendChild(item);
  }
}

function renderOperationList(list, operations, emptyText) {
  clear(list);
  const filtered = operations.filter((operation) =>
    [operation.kind, operation.name, operation.status, operation.correlation_id, JSON.stringify(operation.attributes ?? {})]
      .join(" ")
      .toLowerCase()
      .includes(state.query),
  );
  if (filtered.length === 0) {
    showEmpty(list, emptyText);
    return;
  }

  for (const operation of filtered) {
    const item = document.createElement("li");
    item.className = "record-row";
    if (operation.id && operation.id === state.latestOperationId) item.classList.add("is-new");

    const title = document.createElement("div");
    title.className = "row-topline";
    title.append(
      textNode("strong", "row-title", operation.name),
      badge(operation.status, `status status-${operation.status}`),
    );

    const metadata = document.createElement("div");
    metadata.className = "row-metadata";
    metadata.append(
      badge(formatKind(operation.kind)),
      textNode("span", null, formatTime(operation.timestamp_ms)),
      textNode("span", null, formatDuration(operation.duration_ms)),
    );
    if (operation.correlation_id) metadata.append(textNode("span", null, operation.correlation_id));

    item.append(title, metadata);
    selectableRow(
      item,
      operation,
      operation.name,
      `${formatKind(operation.kind)} / ${operation.status}`,
      operationNodeId(operation),
    );
    list.appendChild(item);
  }
}

function filteredTimelineOperations() {
  if (state.timelineFilter === "all") return state.timeline;
  if (state.timelineFilter === "events") return state.timeline.filter((operation) => operation.kind === "event");
  if (state.timelineFilter === "jobs") return state.timeline.filter((operation) => operation.kind === "job");
  return state.timeline;
}

function renderTimelineFilter() {
  for (const button of timelineFilterButtons) {
    button.classList.toggle("active", button.dataset.timelineFilter === state.timelineFilter);
  }
}

function renderSettings() {
  clear(settingsList);
  for (const [key, value] of Object.entries(state.settings)) {
    const term = document.createElement("dt");
    term.textContent = key.replaceAll("_", " ");
    const description = document.createElement("dd");
    description.textContent = String(value);
    settingsList.append(term, description);
  }
}

function renderAll() {
  ensureDefaultSelection();
  renderRuntimeSummary();
  renderHome();
  renderGraph();
  renderRouteList(routesList, state.routes, "Route snapshots appear after the Nidus facade records mounted handlers.");
  renderTimelineFilter();
  renderOperationList(timelineList, filteredTimelineOperations(), "Timeline fills as the app records runtime activity.");
  renderOperationList(adaptersList, state.adapters, "Adapter operations appear when official hooks record them.");
  renderSettings();

  syncSelectionState();
  focusRelations(state.selected?.id);
}

async function refreshGraph() {
  state.graph = await getJson("./api/graph");
  renderAll();
}

async function loadAll() {
  const [overview, graph, routes, timeline, events, jobs, adapters, settings] = await Promise.all([
    getJson("./api/overview"),
    getJson("./api/graph"),
    getJson("./api/routes"),
    getJson("./api/timeline"),
    getJson("./api/events"),
    getJson("./api/jobs"),
    getJson("./api/adapters"),
    getJson("./api/settings"),
  ]);

  Object.assign(state, { overview, graph, routes, timeline, events, jobs, adapters, settings });
  renderAll();
}

function connectStream() {
  const stream = new EventSource("./stream");
  stream.addEventListener("open", () => {
    setConnectionState("live");
  });
  stream.addEventListener("message", async (event) => {
    const operation = JSON.parse(event.data);
    state.latestOperationId = operation.id;
    state.timeline = [operation, ...state.timeline.filter((item) => item.id !== operation.id)].slice(0, 100);
    if (operation.kind === "event") state.events = [operation, ...state.events];
    if (operation.kind === "job") state.jobs = [operation, ...state.jobs];
    if (operation.kind === "adapter") state.adapters = [operation, ...state.adapters];
    await refreshGraph();
    if (operation.kind !== "lifecycle") {
      selectRecord(operation, operation.name, `${formatKind(operation.kind)} / ${operation.status}`, operationNodeId(operation));
    }
  });
  stream.addEventListener("error", () => {
    setConnectionState("reconnecting");
  });
}

try {
  await loadAll();
  connectStream();
} catch (error) {
  setConnectionState("error");
  selectRecord({ error: error.message }, "Dashboard error", "load failed");
}
