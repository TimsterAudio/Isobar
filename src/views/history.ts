import uPlot from "uplot";
import { app, on, units } from "../lib/store";
import { aggregate, bucketFor, climatology, doyIndex, inRange, summarize, val, type Agg, type PeriodSummary } from "../lib/analytics";
import { alpha, axis, mountChart, palette, timeAxis } from "../lib/charts";
import { addDays, daysBetween, el, isoDate, MONTHS, niceDate, num, signed } from "../lib/format";
import type { DayRecord } from "../lib/types";
import type { View } from "./view";

type Preset = { id: string; label: string; range: (last: string, first: string) => [string, string] };

const PRESETS: Preset[] = [
  { id: "7d", label: "7 days", range: (l) => [addDays(l, -6), l] },
  { id: "30d", label: "30 days", range: (l) => [addDays(l, -29), l] },
  { id: "90d", label: "90 days", range: (l) => [addDays(l, -89), l] },
  { id: "month", label: "This month", range: (l) => [l.slice(0, 8) + "01", l] },
  { id: "ytd", label: "This year", range: (l) => [l.slice(0, 5) + "01-01", l] },
  { id: "12m", label: "12 months", range: (l) => [addDays(l, -364), l] },
  { id: "all", label: "All", range: (l, f) => [f, l] },
];

// Remember the range across navigation within a session.
let saved: { preset: string; start: string; end: string } | null = null;

type Chart = { chart: uPlot; destroy: () => void };

export function historyView(): View {
  const root = el("section", { class: "view history" });
  const charts: Chart[] = [];
  const cleanups: (() => void)[] = [];

  const chips = el("div", { class: "chips", role: "group", "aria-label": "Date range" });
  const startIn = el("input", { type: "date", "aria-label": "Start date" }) as HTMLInputElement;
  const endIn = el("input", { type: "date", "aria-label": "End date" }) as HTMLInputElement;
  const rangeLabel = el("p", { class: "range-label" });
  root.append(
    el("header", { class: "view-head" }, el("h1", {}, "History"), rangeLabel),
    el("div", { class: "toolbar" }, chips, el("div", { class: "custom-range" }, startIn, el("span", { class: "muted" }, "to"), endIn)),
  );

  const summaryHost = el("div", { class: "summary" });
  const tempHost = el("div", { class: "chart" });
  const rainHost = el("div", { class: "chart" });
  const windHost = el("div", { class: "chart" });
  const pressHost = el("div", { class: "chart" });
  const bucketNote = el("p", { class: "muted small" });
  const fig = (title: string, host: HTMLElement, ...keys: Node[]) =>
    el("figure", { class: "panel" }, el("figcaption", {}, el("h2", {}, title), ...keys), host);
  root.append(
    summaryHost,
    bucketNote,
    fig("Temperature", tempHost, el("span", { class: "key heat" }, "Max"), el("span", { class: "key cold" }, "Min")),
    el("div", { class: "pair" },
      fig("Rain", rainHost, el("span", { class: "key rain" }, "Daily"), el("span", { class: "key trace" }, "Running total")),
      fig("Wind", windHost, el("span", { class: "key wind" }, "Peak gust"), el("span", { class: "key ink2" }, "Highest average")),
    ),
    fig("Pressure range", pressHost),
  );

  // Calendar heatmap
  const heatYear = el("select", { "aria-label": "Year" }) as HTMLSelectElement;
  const heatMetric = el("select", { "aria-label": "Measure" }) as HTMLSelectElement;
  for (const [v, l] of [["tmaxAnom", "Max temperature vs normal"], ["tminAnom", "Min temperature vs normal"], ["rain", "Rainfall"]]) heatMetric.append(el("option", { value: v }, l));
  const heatHost = el("div", { class: "heatmap" });
  const heatLegend = el("div", { class: "heat-legend" });
  root.append(
    el("section", { class: "panel" },
      el("div", { class: "panel-head" }, el("h2", {}, "Year at a glance"), el("div", { class: "selects" }, heatMetric, heatYear)),
      heatHost, heatLegend,
    ),
  );

  let preset = saved?.preset ?? "30d";
  let start = saved?.start ?? "";
  let end = saved?.end ?? "";

  function bounds(): [string, string] | null {
    const d = app.days;
    if (!d.length) return null;
    return [d[0].date, d[d.length - 1].date];
  }

  function setPreset(id: string) {
    const b = bounds();
    if (!b) return;
    const p = PRESETS.find((x) => x.id === id)!;
    [start, end] = p.range(b[1], b[0]);
    if (start < b[0]) start = b[0];
    preset = id;
    update();
  }

  function renderChips() {
    chips.replaceChildren(...PRESETS.map((p) => {
      const b = el("button", { class: `chip${p.id === preset ? " on" : ""}`, "aria-pressed": String(p.id === preset) }, p.label);
      b.addEventListener("click", () => setPreset(p.id));
      return b;
    }));
  }

  startIn.addEventListener("change", () => { if (startIn.value) { start = startIn.value; preset = "custom"; update(); } });
  endIn.addEventListener("change", () => { if (endIn.value) { end = endIn.value; preset = "custom"; update(); } });
  heatYear.addEventListener("change", renderHeatmap);
  heatMetric.addEventListener("change", renderHeatmap);

  function update() {
    saved = { preset, start, end };
    renderChips();
    startIn.value = start;
    endIn.value = end;
    rangeLabel.textContent = start && end ? `${niceDate(start)} to ${niceDate(end)}` : "";
    renderSummary();
    renderCharts();
  }

  function stat(label: string, value: string, detail: string, tone = "") {
    return el("div", { class: `stat ${tone}` }, el("div", { class: "stat-label" }, label), el("div", { class: "stat-value" }, value), el("div", { class: "stat-detail" }, detail));
  }

  function vsNormal(v: number | null, n: number | null, unit: string): string {
    if (v === null || n === null) return "no normal yet";
    return `${signed(v - n)}${unit} vs normal`;
  }

  function renderSummary() {
    const u = units();
    if (!app.days.length) {
      summaryHost.replaceChildren(el("div", { class: "empty" },
        el("h2", {}, "No history yet"),
        el("p", {}, "Isobar reads your station's daily records from Cumulus MX. Connect a station or import a dayfile in Settings."),
        el("a", { href: "#/settings", class: "button" }, "Open settings")));
      return;
    }
    const s: PeriodSummary = summarize(app.days, start, end);
    const rainPct = s.normal.rain ? Math.round((s.rain / s.normal.rain) * 100) : null;
    const hotDelta = s.meanMax !== null && s.normal.meanMax !== null ? s.meanMax - s.normal.meanMax : 0;
    const coldDelta = s.meanMin !== null && s.normal.meanMin !== null ? s.meanMin - s.normal.meanMin : 0;
    summaryHost.replaceChildren(
      stat("Highest", s.hottest ? `${num(s.hottest.value)}${u.temp}` : "–", s.hottest ? niceDate(s.hottest.date) : "", "heat"),
      stat("Lowest", s.coldest ? `${num(s.coldest.value)}${u.temp}` : "–", s.coldest ? niceDate(s.coldest.date) : "", "cold"),
      stat("Average max", `${num(s.meanMax)}${u.temp}`, vsNormal(s.meanMax, s.normal.meanMax, u.temp), hotDelta > 0.5 ? "warm" : hotDelta < -0.5 ? "cool" : ""),
      stat("Average min", `${num(s.meanMin)}${u.temp}`, vsNormal(s.meanMin, s.normal.meanMin, u.temp), coldDelta > 0.5 ? "warm" : coldDelta < -0.5 ? "cool" : ""),
      stat("Rain", `${num(s.rain)} ${u.rain}`, `${s.rainDays} rain days${rainPct !== null ? `, ${rainPct}% of normal` : ""}`, "rain"),
      stat("Peak gust", s.gust ? `${num(s.gust.value, 0)} ${u.wind}` : "–", s.gust ? niceDate(s.gust.date) : "", "wind"),
    );
  }

  function renderCharts() {
    charts.splice(0).forEach((c) => c.destroy());
    const days = inRange(app.days, start, end);
    if (!days.length) {
      for (const h of [tempHost, rainHost, windHost, pressHost]) h.replaceChildren();
      bucketNote.textContent = "";
      return;
    }
    const span = daysBetween(start, end) + 1;
    const bucket = bucketFor(span);
    const agg = aggregate(days, bucket);
    bucketNote.textContent = bucket === "day" ? "" : `Showing ${bucket === "week" ? "weekly" : "monthly"} values for this long range: extremes for temperature and wind, totals for rain.`;
    const p = palette();
    const u = units();
    const xs = agg.map((a) => a.ts);
    const col = (f: (a: Agg) => number | null) => agg.map(f);
    const base = (w: number, h: number): uPlot.Options => ({
      width: w, height: h, padding: [10, 8, 0, 4],
      legend: { show: true, live: true },
      cursor: { drag: { x: true, y: false } },
      scales: { x: { time: true } },
      axes: [timeAxis(), axis({ size: 52 })],
      series: [{ label: "Date", value: (_u, v) => (v === null || v === undefined ? "–" : niceDate(isoDate(new Date(v * 1000)))) }],
    });

    // Temperature: band between max and min, with normals when available.
    const normMax = climatology(app.days, "tempMax");
    const normMin = climatology(app.days, "tempMin");
    const showNormals = bucket === "day";
    const nMax = showNormals ? agg.map((a) => normMax[doyIndex(a.key)]?.mean ?? null) : [];
    const nMin = showNormals ? agg.map((a) => normMin[doyIndex(a.key)]?.mean ?? null) : [];
    const tf = (_u: uPlot, v: number) => (v === null || v === undefined ? "–" : `${v.toFixed(1)}${u.temp}`);
    charts.push(mountChart(tempHost, 300, (w, h) => {
      const o = base(w, h);
      o.series.push(
        { label: "Max", stroke: p.heat, width: 1.75, value: tf, spanGaps: false },
        { label: "Min", stroke: p.cold, width: 1.75, value: tf, spanGaps: false },
      );
      if (showNormals) o.series.push(
        { label: "Normal max", stroke: alpha(p.heat, 0.45), width: 1, dash: [5, 4], value: tf },
        { label: "Normal min", stroke: alpha(p.cold, 0.45), width: 1, dash: [5, 4], value: tf },
      );
      o.bands = [{ series: [1, 2], fill: alpha(p.trace, 0.08) }];
      return o;
    }, (showNormals ? [xs, col((a) => a.tMax), col((a) => a.tMin), nMax, nMin] : [xs, col((a) => a.tMax), col((a) => a.tMin)]) as uPlot.AlignedData));

    // Rain: bars plus running total on a second scale.
    let acc = 0;
    const cum = agg.map((a) => (acc += a.rain, Math.round(acc * 10) / 10));
    const rf = (_u: uPlot, v: number) => (v === null || v === undefined ? "–" : `${v.toFixed(1)} ${u.rain}`);
    charts.push(mountChart(rainHost, 240, (w, h) => {
      const o = base(w, h);
      o.scales!.total = { range: (_u, _min, max) => [0, Math.max(1, max * 1.05)] };
      o.scales!.y = { range: (_u, _min, max) => [0, Math.max(1, max * 1.1)] };
      o.axes!.push(axis({ scale: "total", side: 1, grid: { show: false }, size: 52 }));
      o.series.push(
        { label: bucket === "day" ? "Rain" : "Rain total", stroke: p.rain, fill: alpha(p.rain, 0.55), width: 0, paths: uPlot.paths.bars!({ size: [0.75, 24], align: 1 }), points: { show: false }, value: rf },
        { label: "Running total", stroke: p.trace, width: 1.75, scale: "total", value: rf },
      );
      return o;
    }, [xs, col((a) => a.rain), cum]));

    const wf = (_u: uPlot, v: number) => (v === null || v === undefined ? "–" : `${v.toFixed(0)} ${u.wind}`);
    charts.push(mountChart(windHost, 240, (w, h) => {
      const o = base(w, h);
      o.series.push(
        { label: "Peak gust", stroke: p.wind, width: 1.75, value: wf },
        { label: "Highest average", stroke: p.ink2, width: 1.25, value: wf },
      );
      return o;
    }, [xs, col((a) => a.gust), col((a) => a.windMax)]));

    const pf = (_u: uPlot, v: number) => (v === null || v === undefined ? "–" : `${v.toFixed(1)} ${u.press}`);
    charts.push(mountChart(pressHost, 200, (w, h) => {
      const o = base(w, h);
      o.series.push(
        { label: "Highest", stroke: p.trace, width: 1.25, value: pf },
        { label: "Lowest", stroke: p.trace, width: 1.25, value: pf },
      );
      o.bands = [{ series: [1, 2], fill: alpha(p.trace, 0.12) }];
      return o;
    }, [xs, col((a) => a.pMax), col((a) => a.pMin)]));
  }

  function renderHeatmapYears() {
    const years = [...new Set(app.days.map((d) => d.date.slice(0, 4)))].sort().reverse();
    const current = heatYear.value;
    heatYear.replaceChildren(...years.map((y) => el("option", { value: y }, y)));
    heatYear.value = years.includes(current) ? current : years[0] ?? "";
  }

  function renderHeatmap() {
    const year = heatYear.value;
    const metric = heatMetric.value;
    const u = units();
    heatHost.replaceChildren();
    if (!year) return;
    const byDate = new Map<string, DayRecord>(app.days.filter((d) => d.date.startsWith(year)).map((d) => [d.date, d]));
    const field = metric === "tminAnom" ? "tempMin" : "tempMax";
    const normals = metric === "rain" ? null : climatology(app.days, field, 7, Number(year));
    const p = palette();
    const cells: HTMLElement[] = [el("div", { class: "hm-corner" })];
    for (let d = 1; d <= 31; d++) cells.push(el("div", { class: "hm-day" }, d % 5 === 1 || d === 31 ? String(d) : ""));
    for (let m = 0; m < 12; m++) {
      cells.push(el("div", { class: "hm-month" }, MONTHS[m]));
      for (let d = 1; d <= 31; d++) {
        const date = `${year}-${String(m + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
        const valid = new Date(Number(year), m, d).getMonth() === m;
        const rec = byDate.get(date);
        const c = el("div", { class: valid ? "hm-cell" : "hm-cell void" });
        if (rec && valid) {
          if (metric === "rain") {
            const r = val(rec, "rain") ?? 0;
            if (r >= 0.2) {
              const a = Math.min(1, 0.15 + Math.log10(1 + r) / 1.6);
              c.style.background = alpha(p.rain, a);
            }
            c.title = `${niceDate(date)}: ${num(r)} ${u.rain}`;
          } else {
            const v = val(rec, field);
            const nm = normals?.[doyIndex(date)];
            if (v !== null && nm && nm.sd > 0) {
              const z = Math.max(-2.5, Math.min(2.5, (v - nm.mean) / nm.sd));
              c.style.background = alpha(z >= 0 ? p.heat : p.cold, Math.min(0.9, Math.abs(z) / 2.5 + 0.06));
              c.title = `${niceDate(date)}: ${num(v)}${u.temp}, ${signed(v - nm.mean)}${u.temp} vs normal`;
            } else if (v !== null) {
              c.title = `${niceDate(date)}: ${num(v)}${u.temp}`;
            }
          }
        }
        cells.push(c);
      }
    }
    heatHost.append(...cells);
    heatLegend.replaceChildren(
      ...(metric === "rain"
        ? [el("span", {}, "Dry"), el("i", { class: "lg rain" }), el("span", {}, "Wet")]
        : [el("span", {}, "Colder than normal"), el("i", { class: "lg diverge" }), el("span", {}, "Warmer than normal")]),
    );
    if (!normals?.some(Boolean) && metric !== "rain") {
      heatLegend.append(el("span", { class: "muted" }, "Normals need data from other years first."));
    }
  }

  function full() {
    const b = bounds();
    if (b && (!start || !end || preset !== "custom")) {
      setPreset(preset === "custom" ? "30d" : preset);
    } else {
      update();
    }
    renderHeatmapYears();
    renderHeatmap();
  }

  requestAnimationFrame(full);
  cleanups.push(on("days", full));

  return {
    el: root,
    destroy: () => { cleanups.forEach((c) => c()); charts.forEach((c) => c.destroy()); },
    rerender: () => { renderCharts(); renderHeatmap(); },
  };
}
