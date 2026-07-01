const views = document.querySelectorAll(".view");
const buttons = document.querySelectorAll(".nav-item");
const overviewService = document.querySelector("#overview-service");
const runtimeSummary = document.querySelector("#runtime-summary");
const overviewMap = document.querySelector("#overview-map");
const overviewActivity = document.querySelector("#overview-activity");
const graphMap = document.querySelector("#graph-map");
const graphList = document.querySelector("#graph-list");
const routesList = document.querySelector("#routes-list");
const timelineList = document.querySelector("#timeline-list");
const eventsList = document.querySelector("#events-list");
const jobsList = document.querySelector("#jobs-list");
const adaptersList = document.querySelector("#adapters-list");
const settingsList = document.querySelector("#settings-list");
const inspectorTitle = document.querySelector("#inspector-title");
const inspectorMeta = document.querySelector("#inspector-meta");
const inspector = document.querySelector("#inspector-output");
const runtimeStatus = document.querySelector("#runtime-status");
const status = document.querySelector("#connection-status");
const activityCount = document.querySelector("#activity-count");
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
  query: "",
  latestOperationId: null,
};

let activeViewTransition = null;
const mobileQuery = window.matchMedia("(max-width: 760px)");

for (const button of buttons) {
  button.addEventListener("click", () => activateView(button.dataset.view));
}

atlasSearch.addEventListener("input", () => {
  state.query = atlasSearch.value.trim().toLowerCase();
  renderAll();
});

mobileQuery.addEventListener("change", () => renderGraph());

function activateView(id) {
  const apply = () => {
    for (const item of buttons) item.classList.toggle("active", item.dataset.view === id);
    for (const view of views) view.classList.toggle("active", view.id === id);
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
  if (!document.startViewTransition || prefersReducedMotion()) {
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

function visibleGraphNodeIds() {
  const direct = new Set(state.graph.nodes.filter((node) => nodeMatches(node)).map((node) => node.id));
  if (!state.query) return direct;
  for (const edge of state.graph.edges) {
    if (direct.has(edge.source)) direct.add(edge.target);
    if (direct.has(edge.target)) direct.add(edge.source);
  }
  return direct;
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
  activityCount.textContent = `${state.timeline.length} records`;
}

function renderOverviewMap() {
  clear(overviewMap);
  const counts = graphCounts();
  const facts = [
    ["Modules", counts.get("module") ?? 0],
    ["Controllers", counts.get("controller") ?? 0],
    ["Providers", counts.get("provider") ?? 0],
    ["Routes", counts.get("route") ?? state.routes.length],
    ["Events", counts.get("event") ?? state.events.length],
    ["Jobs", counts.get("job") ?? state.jobs.length],
  ];
  for (const [label, value] of facts) {
    const node = document.createElement("button");
    node.type = "button";
    node.className = "atlas-fact";
    node.append(textNode("strong", null, String(value)), textNode("span", null, label));
    node.addEventListener("click", () => {
      state.query = label.toLowerCase().slice(0, -1);
      atlasSearch.value = state.query;
      renderAll();
    });
    overviewMap.appendChild(node);
  }
}

function laneFor(node) {
  if (node.kind === "runtime") return "runtime";
  if (node.kind === "module") return "modules";
  if (node.kind === "provider" || node.kind === "controller") return "components";
  if (node.kind === "route") return "routes";
  return "activity";
}

function layoutGraph(nodes) {
  const lanes = {
    runtime: { x: 7, nodes: [] },
    modules: { x: 25, nodes: [] },
    components: { x: 49, nodes: [] },
    routes: { x: 72, nodes: [] },
    activity: { x: 91, nodes: [] },
  };
  for (const node of nodes) lanes[laneFor(node)].nodes.push(node);

  const positions = new Map();
  for (const lane of Object.values(lanes)) {
    const count = lane.nodes.length;
    lane.nodes.forEach((node, index) => {
      const y = count === 1 ? 50 : 12 + (index * 76) / Math.max(1, count - 1);
      positions.set(node.id, { x: lane.x, y });
    });
  }
  return positions;
}

function renderGraph() {
  clear(graphMap);
  const visible = visibleGraphNodeIds();
  const nodes = state.graph.nodes.filter((node) => visible.has(node.id));
  graphMap.classList.toggle("is-outline", mobileQuery.matches);

  if (nodes.length === 0) {
    graphMap.appendChild(emptyState("Graph data appears here after the Nidus app records its module graph."));
    return;
  }

  if (mobileQuery.matches) {
    renderGraphOutline(nodes);
    syncSelectionState();
    focusRelations(state.selected?.id);
    return;
  }

  const positions = layoutGraph(nodes);
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("graph-edges");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("preserveAspectRatio", "none");

  for (const edge of state.graph.edges) {
    if (!visible.has(edge.source) || !visible.has(edge.target)) continue;
    const source = positions.get(edge.source);
    const target = positions.get(edge.target);
    if (!source || !target) continue;
    const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
    line.dataset.edgeId = edge.id;
    line.dataset.source = edge.source;
    line.dataset.target = edge.target;
    line.dataset.kind = edge.kind;
    line.setAttribute("x1", source.x);
    line.setAttribute("y1", source.y);
    line.setAttribute("x2", target.x);
    line.setAttribute("y2", target.y);
    svg.appendChild(line);
  }
  graphMap.appendChild(svg);

  for (const node of nodes) {
    const position = positions.get(node.id);
    const button = graphButton(node);
    button.style.left = `${position.x}%`;
    button.style.top = `${position.y}%`;
    graphMap.appendChild(button);
  }

  syncSelectionState();
  focusRelations(state.selected?.id);
}

function renderGraphOutline(nodes) {
  const order = ["runtime", "module", "controller", "provider", "route", "event", "job", "adapter"];
  for (const kind of order) {
    const groupNodes = nodes.filter((node) => node.kind === kind);
    if (groupNodes.length === 0) continue;
    const group = document.createElement("section");
    group.className = "outline-group";
    group.appendChild(textNode("h3", "outline-title", formatKind(kind)));
    for (const node of groupNodes) group.appendChild(graphButton(node));
    graphMap.appendChild(group);
  }
}

function graphButton(node) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `graph-node graph-node-${node.kind}`;
  button.dataset.nodeId = node.id;
  button.dataset.selectionKey = node.id;
  button.append(
    badge(formatKind(node.kind), "node-kind"),
    textNode("strong", null, node.label),
    textNode("span", null, node.summary ?? countSummary(node.counts)),
  );
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

function countSummary(counts = {}) {
  const entries = Object.entries(counts);
  if (entries.length === 0) return "inspectable node";
  return entries.map(([key, value]) => `${value} ${key.replaceAll("_", " ")}`).join(" / ");
}

function focusRelations(id) {
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

function selectNode(node) {
  state.selected = { id: node.id, type: "node", node };
  inspectorTitle.textContent = node.label;
  inspectorMeta.textContent = formatKind(node.kind);
  renderNodeInspector(node);
  document.querySelector(".inspector")?.classList.add("has-selection");
  syncSelectionState();
  focusRelations(node.id);
  animateInspector();
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
  inspector.appendChild(keyValues(kindDetails(node)));

  const relations = graphEdgesFor(node.id);
  if (relations.length > 0) {
    const list = document.createElement("ol");
    list.className = "relation-list";
    for (const edge of relations.slice(0, 10)) {
      const peer = graphNode(edge.source === node.id ? edge.target : edge.source);
      const item = document.createElement("li");
      item.append(
        badge(formatKind(edge.kind), "relation-kind"),
        textNode("span", null, peer?.label ?? edge.target),
      );
      list.appendChild(item);
    }
    inspector.append(textNode("h3", "inspector-subhead", "Relationships"), list);
  }

  const metadata = node.metadata?.operation ?? node.metadata;
  inspector.appendChild(codeBlock(metadata));
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

function renderGraphIndex() {
  clear(graphList);
  const visible = state.graph.nodes.filter((node) => nodeMatches(node));
  if (visible.length === 0) {
    showEmpty(graphList, "No graph node matches the current filter.");
    return;
  }
  for (const node of visible) {
    const item = document.createElement("li");
    item.className = "graph-row";
    item.append(
      badge(formatKind(node.kind), "node-kind"),
      textNode("strong", "row-title", node.label),
      textNode("span", "route-summary", node.summary ?? countSummary(node.counts)),
    );
    selectableRow(item, node, node.label, formatKind(node.kind), node.id);
    item.addEventListener("click", () => selectNode(node));
    graphList.appendChild(item);
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
  renderRuntimeSummary();
  renderOverviewMap();
  renderGraph();
  renderGraphIndex();
  renderRouteList(routesList, state.routes, "Route snapshots appear after the Nidus facade records mounted handlers.");
  renderOperationList(overviewActivity, state.timeline.slice(0, 12), "Activity appears here as the app records events and jobs.");
  renderOperationList(timelineList, state.timeline, "Timeline fills as the app records runtime activity.");
  renderOperationList(eventsList, state.events, "Observed events appear after publication.");
  renderOperationList(jobsList, state.jobs, "Observed jobs appear after execution.");
  renderOperationList(adaptersList, state.adapters, "Adapter operations appear when official hooks record them.");
  renderSettings();

  if (!state.selected) {
    const runtime = state.graph.nodes.find((node) => node.kind === "runtime") ?? state.graph.nodes[0];
    if (runtime) selectNode(runtime);
  } else {
    syncSelectionState();
    focusRelations(state.selected.id);
  }
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
