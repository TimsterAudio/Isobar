import "@fontsource/barlow/400.css";
import "@fontsource/barlow/500.css";
import "@fontsource/barlow/600.css";
import "@fontsource/barlow-condensed/500.css";
import "@fontsource/barlow-condensed/600.css";
import "uplot/dist/uPlot.min.css";
import "./styles.css";

import { createApi } from "./lib/api";
import { app, applyTheme, init, on } from "./lib/store";
import { ago, el } from "./lib/format";
import { nowView } from "./views/now";
import { historyView } from "./views/history";
import { compareView } from "./views/compare";
import { insightsView } from "./views/insights";
import { settingsView } from "./views/settings";
import type { View } from "./views/view";

const ROUTES: { id: string; label: string; make: () => View }[] = [
  { id: "now", label: "Now", make: nowView },
  { id: "history", label: "History", make: historyView },
  { id: "compare", label: "Compare", make: compareView },
  { id: "insights", label: "Insights", make: insightsView },
  { id: "settings", label: "Settings", make: () => settingsView(rerenderCurrent) },
];

let current: View | null = null;
const main = el("main", { id: "main", tabindex: "-1" });
const nav = el("nav", { class: "rail-nav", "aria-label": "Sections" });
const railStatus = el("div", { class: "rail-status", role: "status" });

function route(): string {
  const id = location.hash.replace(/^#\//, "");
  return ROUTES.some((r) => r.id === id) ? id : "now";
}

function show() {
  const id = route();
  current?.destroy();
  const r = ROUTES.find((x) => x.id === id)!;
  current = r.make();
  main.replaceChildren(current.el);
  main.scrollTop = 0;
  for (const a of nav.querySelectorAll("a")) {
    if (a.dataset.id === id) a.setAttribute("aria-current", "page");
    else a.removeAttribute("aria-current");
  }
}

function rerenderCurrent() {
  current?.rerender?.();
}

function renderRailStatus() {
  const s = app.status;
  const hasStation = app.api.demo || !!app.settings.stationUrl;
  const state = !hasStation ? "none" : s.connected ? "live" : "offline";
  const label = !hasStation
    ? "No station"
    : s.syncing && app.syncProgress
      ? `Syncing ${app.syncProgress[0]}/${app.syncProgress[1]}`
      : s.connected
        ? `Live, ${ago(s.lastOk)}`
        : "Can't reach station";
  railStatus.replaceChildren(el("span", { class: `dot ${state}` }), el("span", {}, label));
  railStatus.title = s.lastError ?? "";
}

async function start() {
  const root = document.getElementById("app")!;
  root.append(el("p", { class: "booting" }, "Starting Isobar…"));
  const api = await createApi();
  await init(api);
  applyTheme(app.settings.theme);

  for (const r of ROUTES) {
    nav.append(el("a", { href: `#/${r.id}`, "data-id": r.id }, r.label));
  }
  const rail = el("aside", { class: "rail" },
    el("div", { class: "brand" }, el("span", { class: "brand-mark", "aria-hidden": "true" }), "Isobar"),
    nav,
    railStatus,
  );
  root.replaceChildren(el("a", { href: "#main", class: "skip" }, "Skip to content"), rail, main);
  root.classList.add("shell");

  window.addEventListener("hashchange", show);
  on("status", renderRailStatus);
  on("sync", renderRailStatus);
  on("settings", () => { renderRailStatus(); applyTheme(app.settings.theme); });
  setInterval(renderRailStatus, 1000);
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", rerenderCurrent);
  renderRailStatus();
  show();

  // First run: no station and no history yet, so start in Settings.
  if (!api.demo && !app.settings.stationUrl && !app.settings.dayfilePath && !app.days.length && !location.hash) {
    location.hash = "#/settings";
  }
}

start().catch((e) => {
  document.getElementById("app")!.replaceChildren(el("pre", { class: "fatal" }, `Isobar couldn't start.\n\n${e}`));
});
