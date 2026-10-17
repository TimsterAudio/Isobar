import type { Api } from "./api";
import type { DayRecord, LiveReading, Samples, Settings, Status, Units } from "./types";

export interface AppState {
  api: Api;
  settings: Settings;
  status: Status;
  live: LiveReading | null;
  days: DayRecord[];
  samples: Samples;
  syncProgress: [number, number] | null;
}

type Key = "live" | "status" | "days" | "samples" | "settings" | "sync";
type Listener = () => void;

const listeners = new Map<Key, Set<Listener>>();

export const app = {} as AppState;

export function on(key: Key, fn: Listener): () => void {
  let set = listeners.get(key);
  if (!set) listeners.set(key, (set = new Set()));
  set.add(fn);
  return () => set!.delete(fn);
}

export function emit(key: Key) {
  listeners.get(key)?.forEach((fn) => fn());
}

export function units(): Units {
  return app.live?.units ?? { temp: "°C", wind: "km/h", press: "hPa", rain: "mm" };
}

export async function init(api: Api) {
  app.api = api;
  const [settings, status, live, days, samples] = await Promise.all([
    api.getSettings(),
    api.getStatus(),
    api.getLive(),
    api.getDays(),
    api.getSamples(24),
  ]);
  Object.assign(app, { settings, status, live, days, samples, syncProgress: null });

  await api.on("live", (r) => {
    app.live = r;
    appendSample(r);
    emit("live");
  });
  await api.on("status", (s) => {
    app.status = s;
    emit("status");
  });
  await api.on("history-updated", async () => {
    app.days = await api.getDays();
    app.syncProgress = null;
    emit("days");
    emit("sync");
  });
  await api.on("samples-updated", async () => {
    app.samples = await api.getSamples(24);
    emit("samples");
  });
  await api.on("sync-progress", (p) => {
    app.syncProgress = p;
    emit("sync");
  });
}

/** Keep the in-memory 24 h series moving between backend samples. */
function appendSample(r: LiveReading) {
  const pairs: [string, number | null | undefined][] = [
    ["temp", r.temp], ["dew", r.dewPoint], ["press", r.pressure], ["wind", r.windAvg],
    ["gust", r.gustRecent], ["hum", r.humidity], ["rain_today", r.rainToday], ["solar", r.solar],
  ];
  const cutoff = r.fetchedAt - 24 * 3600e3;
  for (const [k, v] of pairs) {
    if (v === null || v === undefined) continue;
    const arr = (app.samples[k] ??= []);
    const last = arr[arr.length - 1];
    if (!last || r.fetchedAt - last[0] >= 60e3) arr.push([r.fetchedAt, v]);
    while (arr.length && arr[0][0] < cutoff) arr.shift();
  }
}

export function applyTheme(theme: Settings["theme"]) {
  if (theme === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", theme);
}
