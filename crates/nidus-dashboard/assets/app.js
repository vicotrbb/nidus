const views = document.querySelectorAll(".view");
const buttons = document.querySelectorAll(".nav-item");
const overviewGrid = document.querySelector("#overview-grid");
const timelineList = document.querySelector("#timeline-list");
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

async function loadOverview() {
  const response = await fetch("./api/overview");
  const overview = await response.json();
  overviewGrid.innerHTML = "";
  for (const metric of overview.metrics) {
    const node = document.createElement("article");
    node.className = "metric";
    node.innerHTML = `<strong>${metric.value}</strong><span>${metric.label}</span>`;
    overviewGrid.appendChild(node);
  }
}

function appendOperation(operation) {
  const item = document.createElement("li");
  item.className = "timeline-item";
  item.tabIndex = 0;
  item.textContent = `${operation.kind} ${operation.name} ${operation.status}`;
  item.addEventListener("click", () => {
    inspector.textContent = JSON.stringify(operation, null, 2);
  });
  timelineList.prepend(item);
}

function connectStream() {
  const stream = new EventSource("./stream");
  stream.addEventListener("open", () => {
    status.textContent = "live";
  });
  stream.addEventListener("message", (event) => {
    appendOperation(JSON.parse(event.data));
  });
  stream.addEventListener("error", () => {
    status.textContent = "reconnecting";
  });
}

await loadOverview();
connectStream();
