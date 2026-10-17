// Synthetic station for developing the UI in a browser. Southern-hemisphere
// temperate climate with seasonal temperatures, bursty rain and fronts.

import type { Api } from "./api";
import type { DayRecord, LiveReading, Samples, Settings, Status } from "./types";
import { isoDate } from "./format";

function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

function gauss(r: () => number) {
  const u = Math.max(r(), 1e-9);
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * r());
}

function makeDays(): DayRecord[] {
  const r = rng(42);
  const out: DayRecord[] = [];
  const start = new Date(2018, 2, 1);
  const end = new Date();
  end.setDate(end.getDate() - 1);
  let anomaly = 0;
  let wetness = 0;
  for (let d = new Date(start); d <= end; d.setDate(d.getDate() + 1)) {
    const doy = Math.floor((d.getTime() - new Date(d.getFullYear(), 0, 0).getTime()) / 864e5);
    const season = Math.cos((2 * Math.PI * (doy - 20)) / 365.25); // +1 mid-Jan, -1 mid-Jul
    anomaly = anomaly * 0.7 + gauss(r) * 2.2;
    const hot = r() < 0.04 * Math.max(0, season) ? 9 + r() * 8 : 0;
    const tmax = 20 + 6.5 * season + anomaly + hot;
    const tmin = tmax - (8 + 2 * season + gauss(r) * 1.5) - hot * 0.5;
    wetness = wetness * 0.55 + (r() < 0.3 + 0.12 * -season ? r() : 0);
    const rain = wetness > 0.35 ? Math.round(-Math.log(Math.max(r(), 1e-6)) * 4 * wetness * 10) / 10 : 0;
    const gust = Math.round((28 + 12 * wetness + gauss(r) * 8 + (hot ? 15 : 0)) * 10) / 10;
    const dirs = hot ? [0, 22.5, 337.5] : wetness > 0.3 ? [202.5, 225, 247.5, 270] : [0, 180, 202.5, 225, 337.5, 157.5];
    const dir = dirs[Math.floor(r() * dirs.length)] + Math.round(gauss(r) * 8);
    const pBase = 1016 - 10 * wetness + gauss(r) * 4;
    const dewMax = Math.min(tmax - 1, tmin + 4 + gauss(r) * 2);
    out.push({
      date: isoDate(d),
      tempMax: round1(Math.max(tmax, tmin + 2)),
      tempMaxTime: "14:" + String(Math.floor(r() * 60)).padStart(2, "0"),
      tempMin: round1(tmin),
      tempMinTime: "06:" + String(Math.floor(r() * 60)).padStart(2, "0"),
      tempAvg: round1((tmax + tmin) / 2),
      rain: Math.max(0, rain),
      rainRateMax: rain > 0 ? round1(rain * (0.8 + r() * 3)) : 0,
      gustMax: Math.max(5, gust),
      gustBearing: (dir + 360) % 360,
      windAvgMax: round1(gust * 0.45),
      windRun: round1(gust * 6 + r() * 60),
      windDirDominant: Math.round((dir + 360) % 360),
      pressMin: round1(pBase - 2 - r() * 5),
      pressMax: round1(pBase + 2 + r() * 4),
      humMin: Math.round(35 + 20 * -season + wetness * 30 + gauss(r) * 6),
      humMax: Math.round(Math.min(99, 85 + wetness * 12 + gauss(r) * 3)),
      dewMax: round1(dewMax),
      dewMin: round1(dewMax - 4 - r() * 3),
      solarMax: Math.round(Math.max(80, 650 + 350 * season - wetness * 400 + gauss(r) * 60)),
      uvMax: round1(Math.max(0.5, 6 + 5 * season - wetness * 4)),
      sunshine: round1(Math.max(0, 7 + 3 * season - wetness * 8)),
    });
  }
  return out;
}

const round1 = (x: number) => Math.round(x * 10) / 10;

export function demoApi(): Api {
  const days = makeDays();
  let settings: Settings = {
    stationName: "Demo station",
    stationUrl: "",
    username: null,
    password: null,
    dayfilePath: null,
    pollSeconds: 10,
    theme: "system",
    alerts: { enabled: true, gustAbove: 60, frostBelow: 2, heatAbove: 35, rainStart: true },
  };

  // A smooth synthetic last 24 hours.
  const now = Date.now();
  const samples: Samples = { temp: [], dew: [], press: [], wind: [], gust: [], hum: [], rain_today: [], solar: [], bearing: [] };
  const r = rng(7);
  let rainAcc = 0;
  for (let t = now - 24 * 3600e3; t <= now; t += 5 * 60e3) {
    const h = new Date(t).getHours() + new Date(t).getMinutes() / 60;
    const diurnal = -Math.cos(((h - 3) / 24) * 2 * Math.PI); // min ~3am, max ~3pm
    const temp = 15 + 5 * diurnal + Math.sin(t / 3.1e6) * 0.6;
    samples.temp.push([t, round1(temp)]);
    samples.dew.push([t, round1(8 + Math.sin(t / 9e6) * 1.5)]);
    samples.press.push([t, round1(1018 - (t - now + 24 * 3600e3) / 3600e3 * 0.25 + Math.sin(t / 5e6) * 0.4)]);
    const wind = Math.max(0, 10 + 6 * Math.max(0, diurnal) + (r() - 0.5) * 6);
    samples.wind.push([t, round1(wind)]);
    samples.gust.push([t, round1(wind * (1.5 + r() * 0.6))]);
    samples.hum.push([t, Math.round(65 - 20 * diurnal + (r() - 0.5) * 4)]);
    if (h > 4 && h < 6) rainAcc += 0.2;
    if (new Date(t).getHours() === 0 && new Date(t).getMinutes() < 5) rainAcc = 0;
    samples.rain_today.push([t, round1(rainAcc)]);
    samples.solar.push([t, Math.round(Math.max(0, Math.sin(((h - 6) / 13) * Math.PI)) * 780 * (0.8 + r() * 0.2))]);
    samples.bearing.push([t, Math.round(200 + Math.sin(t / 4e6) * 25)]);
  }

  const last = (k: string) => samples[k][samples[k].length - 1][1];
  const live = (): LiveReading => {
    const jitter = (Math.random() - 0.5) * 0.2;
    return {
      fetchedAt: Date.now(),
      temp: round1(last("temp") + jitter),
      tempHigh: 19.8, tempHighTime: "14:52", tempLow: 9.6, tempLowTime: "05:41", tempTrend: 0.4,
      feelsLike: round1(last("temp") - 1.4), dewPoint: last("dew"),
      humidity: last("hum"), humHigh: 88, humLow: 44,
      pressure: last("press"), pressHigh: 1018.4, pressLow: 1011.9, pressTrend: -0.6,
      windAvg: last("wind"), gustRecent: last("gust"), gustHigh: 47.2, gustHighTime: "13:18",
      bearing: last("bearing"), bearingAvg: 205, bearingCompass: "SSW",
      rainToday: last("rain_today"), rainRate: 0, rainRateHigh: 3.6, rainMonth: 41.8, rainYear: 512.4,
      solar: last("solar"), solarHigh: 812, uv: 3.2, uvHigh: 6.1,
      forecast: "Fine, becoming less settled",
      units: { temp: "°C", wind: "km/h", press: "hPa", rain: "mm" },
    };
  };

  const status: Status = {
    connected: true, lastOk: Date.now(), lastError: null, syncing: false,
    dayCount: days.length, lastSync: Date.now() - 600e3,
  };
  const liveListeners: ((r: LiveReading) => void)[] = [];
  const statusListeners: ((s: Status) => void)[] = [];
  setInterval(() => {
    const reading = live();
    status.lastOk = reading.fetchedAt;
    liveListeners.forEach((cb) => cb(reading));
    statusListeners.forEach((cb) => cb({ ...status }));
  }, 5000);

  return {
    demo: true,
    getSettings: async () => structuredClone(settings),
    saveSettings: async (s) => { settings = structuredClone(s); },
    testStation: async () => live(),
    getLive: async () => live(),
    getStatus: async () => ({ ...status }),
    getDays: async () => days,
    getSamples: async (hours) => {
      const since = Date.now() - hours * 3600e3;
      const out: Samples = {};
      for (const [k, v] of Object.entries(samples)) out[k] = v.filter(([t]) => t >= since);
      return out;
    },
    syncHistory: async () => days.length,
    importDayfile: async () => 0,
    clearHistory: async () => {},
    pickFile: async () => null,
    on: (async (event: string, cb: (p: unknown) => void) => {
      const list: unknown[] = event === "live" ? liveListeners : event === "status" ? statusListeners : [];
      list.push(cb);
      return () => { const i = list.indexOf(cb); if (i >= 0) list.splice(i, 1); };
    }) as Api["on"],
  };
}
