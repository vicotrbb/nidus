const views = document.querySelectorAll(".view");
const buttons = document.querySelectorAll(".nav-item");
const overviewGrid = document.querySelector("#overview-grid");
const routesList = document.querySelector("#routes-list");
const timelineList = document.querySelector("#timeline-list");
const eventsList = document.querySelector("#events-list");
const jobsList = document.querySelector("#jobs-list");
const adaptersList = document.querySelector("#adapters-list");
const settingsList = document.querySelector("#settings-list");
const inspector = document.querySelector("#inspector-output");
const status = document.querySelector("#connection-status");

for (const button of buttons) {
  button.addEventListener("click", () => {
    for (const item of buttons) item.classList.remove("active");
    for (const view of views) view.classList.remove("active");
    button.classList.add("active");
    document.querySelector(`#${button.dataset.view}`).classList.add("active");
  });
}

async function getJson(path) {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`${path} returned ${response.status}`);
  return response.json();
}

function inspect(value) {
  inspector.textContent = JSON.stringify(value, null, 2);
}

function showEmpty(list, text) {
  list.innerHTML = "";
  const item = document.createElement("li");
  item.className = "empty-state";
  item.textContent = text;
  list.appendChild(item);
}

async function loadOverview() {
  const overview = await getJson("./api/overview");
  overviewGrid.innerHTML = "";
  for (const metric of overview.metrics) {
    const node = document.createElement("article");
    node.className = "metric";
    node.innerHTML = `<strong>${metric.value}</strong><span>${metric.label}</span>`;
    overviewGrid.appendChild(node);
  }
}

function appendOperation(list, operation, mode = "append") {
  const item = document.createElement("li");
  item.className = "timeline-item";
  item.tabIndex = 0;
  item.textContent = `${operation.kind} ${operation.name} ${operation.status}`;
  item.addEventListener("click", () => inspect(operation));
  if (mode === "prepend") list.prepend(item);
  else list.appendChild(item);
}

async function loadOperations(path, list, emptyText) {
  const operations = await getJson(path);
  list.innerHTML = "";
  if (operations.length === 0) {
    showEmpty(list, emptyText);
    return;
  }
  for (const operation of operations) appendOperation(list, operation);
}

async function loadRoutes() {
  const routes = await getJson("./api/routes");
  routesList.innerHTML = "";
  if (routes.length === 0) {
    showEmpty(routesList, "Route snapshot waiting for Nidus facade integration.");
    return;
  }
  for (const route of routes) {
    const item = document.createElement("li");
    item.className = "route-row";
    item.tabIndex = 0;
    item.innerHTML = `<span>${route.method}</span><strong>${route.path}</strong><em>${route.summary ?? "route"}</em>`;
    item.addEventListener("click", () => inspect(route));
    routesList.appendChild(item);
  }
}

async function loadSettings() {
  const settings = await getJson("./api/settings");
  settingsList.innerHTML = "";
  for (const [key, value] of Object.entries(settings)) {
    const term = document.createElement("dt");
    term.textContent = key.replaceAll("_", " ");
    const description = document.createElement("dd");
    description.textContent = String(value);
    settingsList.append(term, description);
  }
}

function connectStream() {
  const stream = new EventSource("./stream");
  stream.addEventListener("open", () => {
    status.textContent = "live";
  });
  stream.addEventListener("message", (event) => {
    const operation = JSON.parse(event.data);
    appendOperation(timelineList, operation, "prepend");
    inspect(operation);
  });
  stream.addEventListener("error", () => {
    status.textContent = "reconnecting";
  });
}

try {
  await Promise.all([
    loadOverview(),
    loadRoutes(),
    loadOperations("./api/timeline", timelineList, "Timeline fills as the app records activity."),
    loadOperations("./api/events", eventsList, "Observed events appear after publication."),
    loadOperations("./api/jobs", jobsList, "Observed jobs appear after execution."),
    loadOperations("./api/adapters", adaptersList, "Adapter operations appear when official hooks record them."),
    loadSettings(),
  ]);
  connectStream();
} catch (error) {
  status.textContent = "error";
  inspect({ error: error.message });
}
