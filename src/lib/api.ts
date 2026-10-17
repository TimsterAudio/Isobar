// One interface, two implementations: the real Tauri backend, and a demo
// backend with synthetic data so the UI can be developed in a plain browser
// (`npm run dev`) without a station.

import type { DayRecord, LiveReading, Samples, Settings, Status } from "./types";

type Unlisten = () => void;

export interface Api {
  readonly demo: boolean;
  getSettings(): Promise<Settings>;
  saveSettings(s: Settings): Promise<void>;
  testStation(url: string, username: string | null, password: string | null): Promise<LiveReading>;
  getLive(): Promise<LiveReading | null>;
  getStatus(): Promise<Status>;
  getDays(): Promise<DayRecord[]>;
  getSamples(hours: number): Promise<Samples>;
  syncHistory(force: boolean): Promise<number>;
  importDayfile(path: string): Promise<number>;
  clearHistory(): Promise<void>;
  pickFile(): Promise<string | null>;
  on(event: "live", cb: (r: LiveReading) => void): Promise<Unlisten>;
  on(event: "status", cb: (s: Status) => void): Promise<Unlisten>;
  on(event: "history-updated", cb: (count: number) => void): Promise<Unlisten>;
  on(event: "samples-updated", cb: () => void): Promise<Unlisten>;
  on(event: "sync-progress", cb: (p: [number, number]) => void): Promise<Unlisten>;
}

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const dialog = await import("@tauri-apps/plugin-dialog");
  return {
    demo: false,
    getSettings: () => invoke("get_settings"),
    saveSettings: (settings) => invoke("save_settings", { settings }),
    testStation: (url, username, password) => invoke("test_station", { url, username, password }),
    getLive: () => invoke("get_live"),
    getStatus: () => invoke("get_status"),
    getDays: () => invoke("get_days"),
    getSamples: (hours) => invoke("get_samples", { hours }),
    syncHistory: (force) => invoke("sync_history", { force }),
    importDayfile: (path) => invoke("import_dayfile", { path }),
    clearHistory: () => invoke("clear_history"),
    pickFile: async () => {
      const picked = await dialog.open({
        multiple: false,
        directory: false,
        filters: [{ name: "Cumulus dayfile", extensions: ["txt", "csv"] }],
      });
      return typeof picked === "string" ? picked : null;
    },
    on: (async (event: string, cb: (payload: unknown) => void) =>
      listen(event, (e) => cb(e.payload))) as Api["on"],
  };
}

export async function createApi(): Promise<Api> {
  if (isTauri) return tauriApi();
  const { demoApi } = await import("./demo");
  return demoApi();
}
