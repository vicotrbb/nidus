const views = document.querySelectorAll(".view");
const buttons = document.querySelectorAll(".nav-item");
const overviewGrid = document.querySelector("#overview-grid");
const overviewService = document.querySelector("#overview-service");
const overviewActivity = document.querySelector("#overview-activity");
const activityCount = document.querySelector("#activity-count");
const overviewMap = document.querySelector("#overview-map");
const graphMap = document.querySelector("#graph-map");
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

const state = {
  overview: { service_name: "nidus-app", metrics: [] },
  routes: [],
  timeline: [],
  events: [],
  jobs: [],
  adapters: [],
  settings: {},
  selected: null,
  latestOperationId: null,
};

let activeViewTransition = null;

for (const button of buttons) {
  button.addEventListener("click", () => activateView(button.dataset.view));
}

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

function selectionKey(kind, value) {
  if (value && typeof value === "object") {
    if (value.id) return `${kind}:${value.id}`;
    if (value.method && value.path) return `route:${value.method}:${value.path}`;
    if (value.name && value.timestamp_ms) return `${kind}:${value.name}:${value.timestamp_ms}`;
  }
  return `${kind}:${String(value ?? "unknown")}`;
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

function selectable(node, record, title, meta, key = selectionKey("record", record)) {
  node.tabIndex = 0;
  node.dataset.selectionKey = key;
  node.classList.toggle("selected", state.selected?.key === key);
  node.addEventListener("click", () => selectRecord(record, title, meta, key));
  node.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectRecord(record, title, meta, key);
    }
  });
  return node;
}

function selectRecord(record, title, meta, key = selectionKey("record", record)) {
  state.selected = { record, title, meta, key };
  inspectorTitle.textContent = title;
  inspectorMeta.textContent = meta;
  inspector.textContent = JSON.stringify(record, null, 2);
  document.querySelector(".inspector")?.classList.add("has-selection");
  syncSelectionState();
  animateInspector();
}

function showEmpty(list, text) {
  clear(list);
  const item = document.createElement("li");
  item.className = "empty-state";
  item.textContent = text;
  list.appendChild(item);
}

function renderOverview() {
  clear(overviewGrid);
  overviewService.textContent = state.overview.service_name ?? "nidus-app";

  const metrics = [
    ...state.overview.metrics,
    { label: "Timeline", value: String(state.timeline.length) },
  ];

  for (const metric of metrics) {
    const node = document.createElement("article");
    node.className = "metric";
    node.append(textNode("strong", null, metric.value), textNode("span", null, metric.label));
    overviewGrid.appendChild(node);
  }

  renderOperationList(
    overviewActivity,
    state.timeline.slice(0, 10),
    "Activity appears here as the app records lifecycle, event, job, and adapter operations.",
  );
  activityCount.textContent = `${state.timeline.length} records`;
  renderOverviewMap();
}

function renderRouteList(list, routes, emptyText) {
  clear(list);
  if (routes.length === 0) {
    showEmpty(list, emptyText);
    return;
  }

  for (const route of routes) {
    const item = document.createElement("li");
    item.className = "route-row";
    item.append(
      badge(route.method, "method"),
      textNode("strong", "row-title", route.path),
      textNode("span", "route-summary", route.summary ?? "route handler"),
    );
    selectable(item, route, `${route.method} ${route.path}`, "route snapshot", selectionKey("route", route));
    list.appendChild(item);
  }
}

function renderOperationList(list, operations, emptyText) {
  clear(list);
  if (operations.length === 0) {
    showEmpty(list, emptyText);
    return;
  }

  for (const operation of operations) {
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
    selectable(
      item,
      operation,
      operation.name,
      `${formatKind(operation.kind)} / ${operation.status}`,
      selectionKey(operation.kind, operation),
    );
    list.appendChild(item);
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

function topologyNode(title, detail, record, className = "", key = selectionKey("topology", title)) {
  const item = document.createElement("button");
  item.className = `topology-node ${className}`.trim();
  item.type = "button";
  item.append(textNode("strong", null, title), textNode("span", null, detail));
  selectable(item, record, title, detail, key);
  return item;
}

function renderOverviewMap() {
  clear(overviewMap);
  const signals = [
    {
      title: "HTTP routes",
      detail: `${state.routes.length} mounted`,
      record: { routes: state.routes },
      key: "topology:routes",
    },
    {
      title: "Events",
      detail: `${state.events.length} captured`,
      record: { events: state.events },
      key: "topology:events",
    },
    {
      title: "Jobs",
      detail: `${state.jobs.length} captured`,
      record: { jobs: state.jobs },
      key: "topology:jobs",
    },
    {
      title: "Adapters",
      detail: `${state.adapters.length} observed`,
      record: { adapters: state.adapters },
      key: "topology:adapters",
    },
  ];
  for (const signal of signals) {
    overviewMap.appendChild(topologyNode(signal.title, signal.detail, signal.record, "", signal.key));
  }
}

function topologyGroup(title, nodes, emptyText) {
  const group = document.createElement("section");
  group.className = "topology-group";
  group.appendChild(textNode("div", "topology-group-title", title));
  if (nodes.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = emptyText;
    group.appendChild(empty);
    return group;
  }
  for (const node of nodes) group.appendChild(node);
  return group;
}

function renderTopology() {
  clear(graphMap);

  const spine = document.createElement("div");
  spine.className = "topology-spine";
  spine.appendChild(
    topologyNode(
      state.overview.service_name ?? "nidus-app",
      `${state.routes.length} routes / ${state.timeline.length} operations`,
      {
        service: state.overview.service_name,
        route_count: state.routes.length,
        operation_count: state.timeline.length,
        settings: state.settings,
      },
      "runtime",
      "topology:runtime",
    ),
  );
  spine.appendChild(
    topologyNode(
      "Dashboard stream",
      "SSE lifecycle heartbeat",
      { stream: "./stream", state: status.textContent },
      "",
      "topology:stream",
    ),
  );

  const lanes = document.createElement("div");
  lanes.className = "topology-lanes";
  lanes.append(
    topologyGroup(
      "Routes",
      state.routes
        .slice(0, 5)
        .map((route) =>
          topologyNode(`${route.method} ${route.path}`, route.summary ?? "handler", route, "", selectionKey("route", route)),
        ),
      "Route snapshots appear after the Nidus facade records mounted handlers.",
    ),
    topologyGroup(
      "Events",
      state.events
        .slice(0, 4)
        .map((operation) =>
          topologyNode(operation.name, formatTime(operation.timestamp_ms), operation, "", selectionKey(operation.kind, operation)),
        ),
      "Publish an event through the example API to populate this lane.",
    ),
    topologyGroup(
      "Jobs",
      state.jobs
        .slice(0, 4)
        .map((operation) =>
          topologyNode(operation.name, formatDuration(operation.duration_ms), operation, "", selectionKey(operation.kind, operation)),
        ),
      "Run a job through the example API to populate this lane.",
    ),
    topologyGroup(
      "Adapters",
      state.adapters
        .slice(0, 4)
        .map((operation) =>
          topologyNode(operation.name, operation.status, operation, "", selectionKey(operation.kind, operation)),
        ),
      "Adapter operations appear when official hooks record them.",
    ),
  );

  graphMap.append(spine, lanes);
}

function renderAll() {
  renderOverview();
  renderRouteList(routesList, state.routes, "Route snapshots appear after the Nidus facade records mounted handlers.");
  renderOperationList(timelineList, state.timeline, "Timeline fills as the app records runtime activity.");
  renderOperationList(eventsList, state.events, "Observed events appear after publication.");
  renderOperationList(jobsList, state.jobs, "Observed jobs appear after execution.");
  renderOperationList(adaptersList, state.adapters, "Adapter operations appear when official hooks record them.");
  renderSettings();
  renderTopology();

  if (!state.selected) {
    selectRecord(
      {
        service_name: state.overview.service_name,
        metrics: state.overview.metrics,
        timeline_records: state.timeline.length,
      },
      "Runtime overview",
      "live summary",
      "topology:runtime",
    );
  }
  syncSelectionState();
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

function syncSelectionState() {
  for (const node of document.querySelectorAll("[data-selection-key]")) {
    node.classList.toggle("selected", node.dataset.selectionKey === state.selected?.key);
  }
}

function animateInspector() {
  if (prefersReducedMotion() || !inspector.animate) return;
  inspector.animate(
    [
      { opacity: 0.82, transform: "translateY(4px)" },
      { opacity: 1, transform: "translateY(0)" },
    ],
    { duration: 220, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
  );
}

async function loadAll() {
  const [overview, routes, timeline, events, jobs, adapters, settings] = await Promise.all([
    getJson("./api/overview"),
    getJson("./api/routes"),
    getJson("./api/timeline"),
    getJson("./api/events"),
    getJson("./api/jobs"),
    getJson("./api/adapters"),
    getJson("./api/settings"),
  ]);

  Object.assign(state, { overview, routes, timeline, events, jobs, adapters, settings });
  renderAll();
}

function connectStream() {
  const stream = new EventSource("./stream");
  stream.addEventListener("open", () => {
    setConnectionState("live");
    renderTopology();
  });
  stream.addEventListener("message", (event) => {
    const operation = JSON.parse(event.data);
    state.latestOperationId = operation.id;
    state.timeline = [operation, ...state.timeline.filter((item) => item.id !== operation.id)].slice(0, 100);
    if (operation.kind === "event") state.events = [operation, ...state.events];
    if (operation.kind === "job") state.jobs = [operation, ...state.jobs];
    if (operation.kind === "adapter") state.adapters = [operation, ...state.adapters];
    renderAll();
    selectRecord(operation, operation.name, `${formatKind(operation.kind)} / ${operation.status}`, selectionKey(operation.kind, operation));
  });
  stream.addEventListener("error", () => {
    setConnectionState("reconnecting");
    renderTopology();
  });
}

try {
  await loadAll();
  connectStream();
} catch (error) {
  setConnectionState("error");
  selectRecord({ error: error.message }, "Dashboard error", "load failed");
}
