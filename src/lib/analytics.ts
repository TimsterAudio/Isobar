import type { DayRecord, NumericDayField } from "./types";
import { addDays, compassIndex, daysBetween, parseIso } from "./format";

/** A day counts as a rain day at or above this (0.2 mm is the smallest reliable tip). */
export const RAIN_DAY = 0.2;

export const val = (d: DayRecord, f: NumericDayField): number | null => {
  const v = d[f];
  return typeof v === "number" && Number.isFinite(v) ? v : null;
};

export function inRange(days: DayRecord[], start: string, end: string): DayRecord[] {
  return days.filter((d) => d.date >= start && d.date <= end);
}

// ---------- Day-of-year helpers ----------

/** 0..365, using a leap year so 29 Feb gets its own slot. */
export function doyIndex(date: string): number {
  const [, m, d] = date.split("-").map(Number);
  return Math.round((new Date(2000, m - 1, d).getTime() - new Date(2000, 0, 1).getTime()) / 864e5);
}

export interface Normal { mean: number; sd: number; n: number }

/**
 * Climatology for one field: for each day of the year, the mean and standard
 * deviation of all values within ±window days of that date, across every year
 * except `excludeYear` (so a day isn't compared against itself).
 */
export function climatology(days: DayRecord[], field: NumericDayField, window = 7, excludeYear?: number): (Normal | null)[] {
  const buckets: number[][] = Array.from({ length: 366 }, () => []);
  for (const d of days) {
    if (excludeYear !== undefined && Number(d.date.slice(0, 4)) === excludeYear) continue;
    const v = val(d, field);
    if (v !== null) buckets[doyIndex(d.date)].push(v);
  }
  const out: (Normal | null)[] = [];
  for (let i = 0; i < 366; i++) {
    let n = 0, sum = 0, sum2 = 0;
    for (let k = -window; k <= window; k++) {
      for (const v of buckets[(i + k + 366) % 366]) { n++; sum += v; sum2 += v * v; }
    }
    if (n < 10) { out.push(null); continue; }
    const mean = sum / n;
    out.push({ mean, sd: Math.sqrt(Math.max(0, sum2 / n - mean * mean)), n });
  }
  return out;
}

export function yearsCovered(days: DayRecord[]): number {
  if (!days.length) return 0;
  return daysBetween(days[0].date, days[days.length - 1].date) / 365.25;
}

// ---------- Anomalies ----------

export interface Anomaly {
  date: string;
  field: "tempMax" | "tempMin";
  value: number;
  normal: number;
  z: number;
}

/** Days in [start, end] whose max or min sits more than `threshold` σ from the seasonal normal. */
export function anomalies(days: DayRecord[], start: string, end: string, threshold = 1.5): Anomaly[] {
  const fields = ["tempMax", "tempMin"] as const;
  const out: Anomaly[] = [];
  for (const field of fields) {
    const normals = climatology(days, field, 7);
    for (const d of inRange(days, start, end)) {
      const v = val(d, field);
      const nm = normals[doyIndex(d.date)];
      if (v === null || !nm || nm.sd < 0.1) continue;
      const z = (v - nm.mean) / nm.sd;
      if (Math.abs(z) >= threshold) out.push({ date: d.date, field, value: v, normal: nm.mean, z });
    }
  }
  return out.sort((a, b) => Math.abs(b.z) - Math.abs(a.z));
}

// ---------- Period summary ----------

export interface Extreme { value: number; date: string; time?: string | null }

export function extreme(days: DayRecord[], field: NumericDayField, kind: "max" | "min", timeField?: keyof DayRecord): Extreme | null {
  let best: Extreme | null = null;
  for (const d of days) {
    const v = val(d, field);
    if (v === null) continue;
    if (!best || (kind === "max" ? v > best.value : v < best.value)) {
      best = { value: v, date: d.date, time: timeField ? (d[timeField] as string | null | undefined) : undefined };
    }
  }
  return best;
}

export function mean(days: DayRecord[], field: NumericDayField): number | null {
  let n = 0, s = 0;
  for (const d of days) { const v = val(d, field); if (v !== null) { n++; s += v; } }
  return n ? s / n : null;
}

export function total(days: DayRecord[], field: NumericDayField): number {
  let s = 0;
  for (const d of days) s += val(d, field) ?? 0;
  return s;
}

export interface PeriodSummary {
  count: number;
  hottest: Extreme | null;
  coldest: Extreme | null;
  meanMax: number | null;
  meanMin: number | null;
  rain: number;
  rainDays: number;
  wettest: Extreme | null;
  gust: Extreme | null;
  pressLow: Extreme | null;
  pressHigh: Extreme | null;
  /** Expected values for the same calendar days from other years. */
  normal: { meanMax: number | null; meanMin: number | null; rain: number | null };
}

export function summarize(all: DayRecord[], start: string, end: string): PeriodSummary {
  const days = inRange(all, start, end);
  const year = Number(end.slice(0, 4));
  const cMax = climatology(all, "tempMax", 7, year);
  const cMin = climatology(all, "tempMin", 7, year);
  const cRain = climatology(all, "rain", 7, year);
  let nMax = 0, sMax = 0, nMin = 0, sMin = 0, rainNormal = 0, rainN = 0;
  for (let date = start; date <= end; date = addDays(date, 1)) {
    const i = doyIndex(date);
    if (cMax[i]) { nMax++; sMax += cMax[i]!.mean; }
    if (cMin[i]) { nMin++; sMin += cMin[i]!.mean; }
    if (cRain[i]) { rainN++; rainNormal += cRain[i]!.mean; }
  }
  const span = daysBetween(start, end) + 1;
  return {
    count: days.length,
    hottest: extreme(days, "tempMax", "max", "tempMaxTime"),
    coldest: extreme(days, "tempMin", "min", "tempMinTime"),
    meanMax: mean(days, "tempMax"),
    meanMin: mean(days, "tempMin"),
    rain: total(days, "rain"),
    rainDays: days.filter((d) => (val(d, "rain") ?? 0) >= RAIN_DAY).length,
    wettest: extreme(days, "rain", "max"),
    gust: extreme(days, "gustMax", "max", "gustTime"),
    pressLow: extreme(days, "pressMin", "min"),
    pressHigh: extreme(days, "pressMax", "max"),
    normal: {
      meanMax: nMax > span * 0.8 ? sMax / nMax : null,
      meanMin: nMin > span * 0.8 ? sMin / nMin : null,
      rain: rainN > span * 0.8 ? rainNormal : null,
    },
  };
}

// ---------- Aggregation for long ranges ----------

export type Bucket = "day" | "week" | "month";

export function bucketFor(spanDays: number): Bucket {
  if (spanDays > 3 * 365) return "month";
  if (spanDays > 200) return "week";
  return "day";
}

export interface Agg {
  key: string; // first date in bucket
  ts: number; // seconds, for uPlot
  tMax: number | null; tMin: number | null; tMaxMean: number | null; tMinMean: number | null;
  rain: number; gust: number | null; windMax: number | null;
  pMin: number | null; pMax: number | null;
}

function bucketKey(date: string, b: Bucket): string {
  if (b === "day") return date;
  if (b === "month") return date.slice(0, 7) + "-01";
  const d = parseIso(date);
  const dow = (d.getDay() + 6) % 7; // Monday start
  return addDays(date, -dow);
}

export function aggregate(days: DayRecord[], b: Bucket): Agg[] {
  const map = new Map<string, DayRecord[]>();
  for (const d of days) {
    const k = bucketKey(d.date, b);
    let arr = map.get(k);
    if (!arr) map.set(k, (arr = []));
    arr.push(d);
  }
  const out: Agg[] = [];
  for (const [key, arr] of map) {
    const mx = (f: NumericDayField) => extreme(arr, f, "max")?.value ?? null;
    const mn = (f: NumericDayField) => extreme(arr, f, "min")?.value ?? null;
    out.push({
      key,
      ts: parseIso(key).getTime() / 1000,
      tMax: mx("tempMax"), tMin: mn("tempMin"),
      tMaxMean: mean(arr, "tempMax"), tMinMean: mean(arr, "tempMin"),
      rain: total(arr, "rain"),
      gust: mx("gustMax"), windMax: mx("windAvgMax"),
      pMin: mn("pressMin"), pMax: mx("pressMax"),
    });
  }
  return out.sort((a, b2) => a.key.localeCompare(b2.key));
}

// ---------- Cross-year comparison ----------

export type CompareMetric = "tempMax" | "tempMin" | "rainCum" | "gustMax";

export interface YearSeries { year: number; values: (number | null)[] }

/**
 * One series per year, indexed by day of month (month given) or day of year
 * (month = null). Rain is cumulative.
 */
export function crossYear(days: DayRecord[], month: number | null, metric: CompareMetric): { length: number; series: YearSeries[] } {
  const length = month === null ? 366 : 31;
  const byYear = new Map<number, (number | null)[]>();
  for (const d of days) {
    const [y, m, dd] = d.date.split("-").map(Number);
    if (month !== null && m !== month + 1) continue;
    const idx = month === null ? doyIndex(d.date) : dd - 1;
    let arr = byYear.get(y);
    if (!arr) byYear.set(y, (arr = new Array(length).fill(null)));
    arr[idx] = val(d, metric === "rainCum" ? "rain" : metric);
  }
  const series: YearSeries[] = [];
  for (const [year, values] of [...byYear].sort((a, b) => a[0] - b[0])) {
    if (metric === "rainCum") {
      let acc = 0;
      let lastSeen = -1;
      values.forEach((v, i) => { if (v !== null) lastSeen = i; });
      for (let i = 0; i < values.length; i++) {
        if (i > lastSeen) { values[i] = null; continue; }
        acc += values[i] ?? 0;
        values[i] = Math.round(acc * 10) / 10;
      }
    }
    if (values.some((v) => v !== null)) series.push({ year, values });
  }
  return { length, series };
}

/** Mean across series at each index, ignoring the given year. */
export function meanSeries(series: YearSeries[], exclude?: number): (number | null)[] {
  const len = series[0]?.values.length ?? 0;
  const out: (number | null)[] = [];
  for (let i = 0; i < len; i++) {
    let n = 0, s = 0;
    for (const ys of series) {
      if (ys.year === exclude) continue;
      const v = ys.values[i];
      if (v !== null) { n++; s += v; }
    }
    out.push(n >= 2 ? Math.round((s / n) * 10) / 10 : null);
  }
  return out;
}

// ---------- Records ----------

export interface RecordRow { label: string; value: number | null; unit: "temp" | "rain" | "rate" | "wind" | "press" | "days"; date?: string; detail?: string }

export function records(days: DayRecord[]): RecordRow[] {
  const ex = (f: NumericDayField, k: "max" | "min", tf?: keyof DayRecord) => extreme(days, f, k, tf);
  const hot = ex("tempMax", "max", "tempMaxTime");
  const cold = ex("tempMin", "min", "tempMinTime");
  const warmNight = ex("tempMin", "max");
  const coolDay = ex("tempMax", "min");
  const wet = ex("rain", "max");
  const gust = ex("gustMax", "max", "gustTime");
  const pLow = ex("pressMin", "min");
  const pHigh = ex("pressMax", "max");
  const rate = ex("rainRateMax", "max");
  const dry = longestRun(days, (d) => (val(d, "rain") ?? 0) < RAIN_DAY);
  const wetRun = longestRun(days, (d) => (val(d, "rain") ?? 0) >= RAIN_DAY);
  const months = monthTotals(days);
  const wettestMonth = months.reduce<{ key: string; rain: number } | null>((b, m) => (!b || m.rain > b.rain ? m : b), null);
  const row = (label: string, e: Extreme | null, unit: RecordRow["unit"]): RecordRow =>
    ({ label, value: e?.value ?? null, unit, date: e?.date, detail: e?.time ? `at ${e.time}` : undefined });
  return [
    row("Hottest day", hot, "temp"),
    row("Coldest night", cold, "temp"),
    row("Warmest night", warmNight, "temp"),
    row("Coldest day", coolDay, "temp"),
    row("Wettest day", wet, "rain"),
    row("Highest rain rate", rate, "rate"),
    { label: "Wettest month", value: wettestMonth?.rain ?? null, unit: "rain", date: wettestMonth ? wettestMonth.key + "-01" : undefined, detail: "month" },
    row("Strongest gust", gust, "wind"),
    row("Lowest pressure", pLow, "press"),
    row("Highest pressure", pHigh, "press"),
    { label: "Longest dry spell", value: dry?.length ?? null, unit: "days", date: dry?.start, detail: dry ? `to ${dry.end}` : undefined },
    { label: "Longest wet spell", value: wetRun?.length ?? null, unit: "days", date: wetRun?.start, detail: wetRun ? `to ${wetRun.end}` : undefined },
  ];
}

function longestRun(days: DayRecord[], pred: (d: DayRecord) => boolean): { start: string; end: string; length: number } | null {
  let best: { start: string; end: string; length: number } | null = null;
  let start: string | null = null;
  let prev: string | null = null;
  let len = 0;
  for (const d of days) {
    const contiguous = prev !== null && daysBetween(prev, d.date) === 1;
    if (pred(d)) {
      if (!contiguous || start === null) { start = d.date; len = 0; }
      len++;
      if (!best || len > best.length) best = { start: start!, end: d.date, length: len };
    } else {
      start = null; len = 0;
    }
    prev = d.date;
  }
  return best;
}

export function monthTotals(days: DayRecord[]): { key: string; rain: number; n: number }[] {
  const m = new Map<string, { key: string; rain: number; n: number }>();
  for (const d of days) {
    const k = d.date.slice(0, 7);
    let e = m.get(k);
    if (!e) m.set(k, (e = { key: k, rain: 0, n: 0 }));
    e.rain += val(d, "rain") ?? 0;
    e.n++;
  }
  return [...m.values()];
}

// ---------- Monthly climate table ----------

export interface MonthClimate {
  month: number;
  meanMax: number | null; meanMin: number | null;
  meanRain: number | null; meanRainDays: number | null;
  recordHigh: Extreme | null; recordLow: Extreme | null;
  years: number;
}

/** Only months with at least 25 days of data count towards rain totals. */
export function monthlyClimate(days: DayRecord[]): MonthClimate[] {
  const out: MonthClimate[] = [];
  for (let m = 0; m < 12; m++) {
    const sel = days.filter((d) => Number(d.date.slice(5, 7)) === m + 1);
    const perMonth = new Map<string, DayRecord[]>();
    for (const d of sel) {
      const k = d.date.slice(0, 7);
      let a = perMonth.get(k);
      if (!a) perMonth.set(k, (a = []));
      a.push(d);
    }
    const complete = [...perMonth.values()].filter((a) => a.length >= 25);
    out.push({
      month: m,
      meanMax: mean(sel, "tempMax"),
      meanMin: mean(sel, "tempMin"),
      meanRain: complete.length ? complete.reduce((s, a) => s + total(a, "rain"), 0) / complete.length : null,
      meanRainDays: complete.length ? complete.reduce((s, a) => s + a.filter((d) => (val(d, "rain") ?? 0) >= RAIN_DAY).length, 0) / complete.length : null,
      recordHigh: extreme(sel, "tempMax", "max"),
      recordLow: extreme(sel, "tempMin", "min"),
      years: complete.length,
    });
  }
  return out;
}

// ---------- Wind ----------

/** Fraction of days with each of 16 dominant-direction sectors. */
export function windRose(days: DayRecord[], field: NumericDayField = "windDirDominant"): { counts: number[]; total: number } {
  const counts = new Array(16).fill(0);
  let n = 0;
  for (const d of days) {
    const b = val(d, field);
    if (b === null || (field === "windDirDominant" && b === 0 && (val(d, "windAvgMax") ?? 1) === 0)) continue;
    counts[compassIndex(b)]++;
    n++;
  }
  return { counts, total: n };
}
