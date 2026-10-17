import { app, applyTheme, emit, on } from "../lib/store";
import { ago, el, num } from "../lib/format";
import type { Settings } from "../lib/types";
import type { View } from "./view";

export function settingsView(onThemeChange: () => void): View {
  const root = el("section", { class: "view settings" });
  const cleanups: (() => void)[] = [];
  const draft: Settings = structuredClone(app.settings);

  const text = (value: string, attrs: Record<string, string> = {}) => {
    const i = el("input", { type: "text", ...attrs }) as HTMLInputElement;
    i.value = value;
    return i;
  };
  const field = (label: string, input: HTMLElement, help?: string) =>
    el("label", { class: "field" }, el("span", { class: "field-label" }, label), input, help ? el("span", { class: "help" }, help) : null);

  // ----- Station
  const name = text(draft.stationName);
  const url = text(draft.stationUrl, { placeholder: "http://192.168.1.20:8998", spellcheck: "false" });
  const user = text(draft.username ?? "", { autocomplete: "off" });
  const pass = el("input", { type: "password", autocomplete: "off" }) as HTMLInputElement;
  pass.value = draft.password ?? "";
  const poll = el("input", { type: "number", min: "3", max: "600", step: "1" }) as HTMLInputElement;
  poll.value = String(draft.pollSeconds);
  const testBtn = el("button", { class: "button secondary", type: "button" }, "Test connection");
  const testResult = el("p", { class: "test-result", role: "status" });
  testBtn.addEventListener("click", async () => {
    testResult.className = "test-result";
    testResult.textContent = "Connecting…";
    try {
      const r = await app.api.testStation(url.value, user.value || null, pass.value || null);
      testResult.className = "test-result ok";
      testResult.textContent = `Connected. ${num(r.temp)}${r.units.temp}, ${num(r.pressure)} ${r.units.press}, ${num(r.humidity, 0)}% humidity.`;
    } catch (e) {
      testResult.className = "test-result err";
      testResult.textContent = `${e}. Check the address is your MX interface (port 8998 by default) and that this computer can reach it.`;
    }
  });

  // ----- History
  const path = text(draft.dayfilePath ?? "", { placeholder: "Only needed without a station address", spellcheck: "false" });
  const browse = el("button", { class: "button secondary", type: "button" }, "Choose file");
  browse.addEventListener("click", async () => { const p = await app.api.pickFile(); if (p) path.value = p; });
  const histStatus = el("p", { class: "muted" });
  const syncBtn = el("button", { class: "button secondary", type: "button" }, "Download history again");
  const importBtn = el("button", { class: "button secondary", type: "button" }, "Import a dayfile");
  const clearBtn = el("button", { class: "button danger", type: "button" }, "Delete local history");
  const histMsg = el("p", { class: "test-result", role: "status" });
  syncBtn.addEventListener("click", async () => {
    histMsg.className = "test-result"; histMsg.textContent = "Downloading…";
    try { const n = await app.api.syncHistory(true); histMsg.className = "test-result ok"; histMsg.textContent = `Downloaded ${n} days.`; }
    catch (e) { histMsg.className = "test-result err"; histMsg.textContent = String(e); }
  });
  importBtn.addEventListener("click", async () => {
    const p = await app.api.pickFile();
    if (!p) return;
    try { const n = await app.api.importDayfile(p); histMsg.className = "test-result ok"; histMsg.textContent = `Imported ${n} days from ${p}.`; }
    catch (e) { histMsg.className = "test-result err"; histMsg.textContent = String(e); }
  });
  clearBtn.addEventListener("click", async () => {
    if (!confirm("Delete all daily history stored by Isobar on this computer? Your station's own files are not touched.")) return;
    await app.api.clearHistory();
    histMsg.className = "test-result"; histMsg.textContent = "Local history deleted.";
  });
  const renderHist = () => {
    const s = app.status;
    const p = app.syncProgress;
    histStatus.textContent = s.syncing && p
      ? `Downloading ${p[0]} of ${p[1]} days…`
      : `${s.dayCount} days stored. Last updated ${ago(s.lastSync)}.`;
  };
  renderHist();
  cleanups.push(on("status", renderHist), on("sync", renderHist));

  // ----- Alerts
  const a = draft.alerts;
  const alertsOn = el("input", { type: "checkbox" }) as HTMLInputElement;
  alertsOn.checked = a.enabled;
  const numIn = (v: number | null) => { const i = el("input", { type: "number", step: "0.5" }) as HTMLInputElement; i.value = v === null ? "" : String(v); return i; };
  const gust = numIn(a.gustAbove), frost = numIn(a.frostBelow), heat = numIn(a.heatAbove);
  const rainStart = el("input", { type: "checkbox" }) as HTMLInputElement;
  rainStart.checked = a.rainStart;

  // ----- Appearance
  const theme = el("select") as HTMLSelectElement;
  for (const [v, l] of [["system", "Match system"], ["light", "Light"], ["dark", "Dark"]]) theme.append(el("option", { value: v }, l));
  theme.value = draft.theme;
  theme.addEventListener("change", () => { applyTheme(theme.value as Settings["theme"]); onThemeChange(); });

  const saveBtn = el("button", { class: "button", type: "submit" }, "Save settings");
  const saveMsg = el("span", { class: "save-msg", role: "status" });
  const form = el("form", { class: "settings-form" },
    el("section", { class: "panel" },
      el("h2", {}, "Station"),
      field("Name", name, "Shown at the top of the Now screen and in alerts."),
      field("Cumulus MX address", url, "The address of the MX interface, usually port 8998. Isobar reads live conditions and daily history from it."),
      el("div", { class: "row" }, field("Username", user), field("Password", pass)),
      el("p", { class: "help" }, "Only needed if you've secured the MX interface. Stored unencrypted in Isobar's settings file."),
      field("Check every (seconds)", poll),
      el("div", { class: "row actions" }, testBtn), testResult,
    ),
    el("section", { class: "panel" },
      el("h2", {}, "History"),
      histStatus,
      el("div", { class: "row file" }, field("Dayfile on this computer", path, "Use this instead of a station address to read dayfile.txt directly, for example from a shared folder. Isobar re-reads it when it changes."), browse),
      el("div", { class: "row actions" }, syncBtn, importBtn, clearBtn), histMsg,
    ),
    el("section", { class: "panel" },
      el("h2", {}, "Alerts"),
      el("label", { class: "check" }, alertsOn, "Show desktop notifications"),
      el("div", { class: "row" }, field("Gust at or above", gust), field("Temperature at or below", frost), field("Temperature at or above", heat)),
      el("p", { class: "help" }, "In your station's units. Leave a box empty to turn that alert off. Each alerts at most once a day, gusts at most every three hours."),
      el("label", { class: "check" }, rainStart, "Rain starting after three dry hours"),
    ),
    el("section", { class: "panel" }, el("h2", {}, "Appearance"), field("Theme", theme)),
    el("div", { class: "save-bar" }, saveBtn, saveMsg),
  );

  const parseNum = (i: HTMLInputElement) => (i.value.trim() === "" ? null : Number(i.value));
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const next: Settings = {
      stationName: name.value.trim() || "My station",
      stationUrl: url.value.trim(),
      username: user.value.trim() || null,
      password: pass.value || null,
      dayfilePath: path.value.trim() || null,
      pollSeconds: Math.max(3, Math.min(600, Number(poll.value) || 10)),
      theme: theme.value as Settings["theme"],
      alerts: { enabled: alertsOn.checked, gustAbove: parseNum(gust), frostBelow: parseNum(frost), heatAbove: parseNum(heat), rainStart: rainStart.checked },
    };
    try {
      await app.api.saveSettings(next);
      app.settings = next;
      emit("settings");
      saveMsg.className = "save-msg ok";
      saveMsg.textContent = "Saved";
      setTimeout(() => (saveMsg.textContent = ""), 2500);
    } catch (err) {
      saveMsg.className = "save-msg err";
      saveMsg.textContent = `Couldn't save: ${err}`;
    }
  });

  root.append(el("header", { class: "view-head" }, el("h1", {}, "Settings")), form);
  if (app.api.demo) {
    root.insertBefore(el("p", { class: "notice" }, "You're viewing demo data in a browser. Run Isobar as a desktop app to connect a real station."), form);
  }
  return { el: root, destroy: () => cleanups.forEach((c) => c()) };
}
