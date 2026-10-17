import uPlot from "uplot";
import { app, on, units } from "../lib/store";
import { crossYear, extreme, inRange, mean, meanSeries, RAIN_DAY, total, val, windRose, type CompareMetric } from "../lib/analytics";
import { alpha, axis, mountChart, palette } from "../lib/charts";
import { addDays, el, MONTHS, MONTHS_LONG, niceDate, num, signed } from "../lib/format";
import { windRoseSvg } from "../lib/windrose";
import type { View } from "./view";

type Chart = { chart: uPlot; destroy: () => void };

const METRICS: { id: CompareMetric; label: string }[] = [
  { id: "tempMax", label: "Daily maximum" },
  { id: "tempMin", label: "Daily minimum" },
  { id: "rainCum", label: "Rain, running total" },
  { id: "gustMax", label: "Peak gust" },
];

// Day-of-year index of the first of each month in a leap year.
const MONTH_STARTS = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];

let mode: "years" | "periods" = "years";

export function compareView(): View {
  const root = el("section", { class: "view compare" });
  const charts: Chart[] = [];
  const cleanups: (() => void)[] = [];

  const tabYears = el("button", { class: "tab", role: "tab" }, "Across years");
  const tabPeriods = el("button", { class: "tab", role: "tab" }, "Two periods");
  const body = el("div", { class: "compare-body" });
  root.append(
    el("header", { class: "view-head" }, el("h1", {}, "Compare"), el("div", { class: "tabs", role: "tablist" }, tabYears, tabPeriods)),
    body,
  );
  tabYears.addEventListener("click", () => { mode = "years"; render(); });
  tabPeriods.addEventListener("click", () => { mode = "periods"; render(); });

  // ----- Across years
  const monthSel = el("select", { "aria-label": "Month" }) as HTMLSelectElement;
  monthSel.append(el("option", { value: "year" }, "Whole year"));
  MONTHS_LONG.forEach((m, i) => monthSel.append(el("option", { value: String(i) }, m)));
  const metricSel = el("select", { "aria-label": "Measure" }) as HTMLSelectElement;
  METRICS.forEach((m) => metricSel.append(el("option", { value: m.id }, m.label)));
  const yearSel = el("select", { "aria-label": "Highlighted year" }) as HTMLSelectElement;
  const chartHost = el("div", { class: "chart" });
  const rankHost = el("div", { class: "ranking" });
  const yearsPanel = el("div", { class: "years-layout" },
    el("figure", { class: "panel" },
      el("figcaption", {}, el("h2", { class: "years-title" }), el("span", { class: "key trace" }, "Highlighted year"), el("span", { class: "key ink2 dashed" }, "Average of other years"), el("span", { class: "key faint" }, "Other years")),
      chartHost),
    el("aside", { class: "panel" }, el("h2", {}, "How each year compares"), rankHost),
  );
  const yearsControls = el("div", { class: "toolbar" },
    el("label", {}, "Period", monthSel), el("label", {}, "Measure", metricSel), el("label", {}, "Highlight", yearSel));
  for (const s of [monthSel, metricSel, yearSel]) s.addEventListener("change", renderYears);

  // Default to this month and rain total, the most-asked question.
  const lastDate = () => app.days[app.days.length - 1]?.date;
  monthSel.value = lastDate() ? String(Number(lastDate()!.slice(5, 7)) - 1) : "year";
  metricSel.value = "rainCum";

  function renderYears() {
    charts.splice(0).forEach((c) => c.destroy());
    const u = units();
    const month = monthSel.value === "year" ? null : Number(monthSel.value);
    const metric = metricSel.value as CompareMetric;
    const { length, series } = crossYear(app.days, month, metric);
    const years = series.map((s) => s.year);
    const prev = yearSel.value;
    yearSel.replaceChildren(...[...years].reverse().map((y) => el("option", { value: String(y) }, String(y))));
    yearSel.value = years.includes(Number(prev)) ? prev : String(years[years.length - 1] ?? "");
    const hl = Number(yearSel.value);
    (yearsPanel.querySelector(".years-title") as HTMLElement).textContent =
      `${METRICS.find((m) => m.id === metric)!.label}, ${month === null ? "whole year" : MONTHS_LONG[month]}`;

    if (series.length === 0) {
      chartHost.replaceChildren(el("p", { class: "chart-empty" }, "No data for this period yet."));
      rankHost.replaceChildren();
      return;
    }

    const p = palette();
    const xs = Array.from({ length }, (_, i) => i);
    const avg = meanSeries(series, hl);
    const others = series.filter((s) => s.year !== hl);
    const main = series.find((s) => s.year === hl);
    const unit = metric === "rainCum" ? ` ${u.rain}` : metric === "gustMax" ? ` ${u.wind}` : u.temp;
    const fmt = (_u: uPlot, v: number) => (v === null || v === undefined ? "–" : `${v.toFixed(1)}${unit}`);
    const xLabel = (i: number) => {
      if (month !== null) return `${i + 1} ${MONTHS[month]}`;
      let m = 0;
      for (let k = 0; k < MONTH_STARTS.length; k++) if (MONTH_STARTS[k] <= i) m = k;
      return `${i - MONTH_STARTS[m] + 1} ${MONTHS[m]}`;
    };
    charts.push(mountChart(chartHost, 360, (w, h) => ({
      width: w, height: h, padding: [10, 8, 0, 4],
      legend: { show: false },
      cursor: { drag: { x: true, y: false } },
      scales: { x: { time: false } },
      axes: [
        month === null
          ? axis({ splits: () => MONTH_STARTS, values: (_u, v) => v.map((i) => MONTHS[MONTH_STARTS.indexOf(i)] ?? "") })
          : axis({ values: (_u, v) => v.map((i) => (Number.isInteger(i) ? String(i + 1) : "")) }),
        axis({ size: 52 }),
      ],
      series: [
        { value: (_u, v) => (v === null || v === undefined ? "" : xLabel(v)) },
        ...others.map((s) => ({ label: String(s.year), stroke: alpha(p.ink2, 0.28), width: 1, points: { show: false }, value: fmt, spanGaps: metric !== "rainCum" })),
        { label: "Average", stroke: p.ink2, width: 1.5, dash: [6, 4], points: { show: false }, value: fmt, spanGaps: true },
        ...(main ? [{ label: String(hl), stroke: p.trace, width: 2.75, value: fmt, spanGaps: metric !== "rainCum" }] : []),
      ],
    }), [xs, ...others.map((s) => s.values), avg, ...(main ? [main.values] : [])] as uPlot.AlignedData));

    // Ranking table for the chosen period
    const rows = series.map((s) => {
      const days = app.days.filter((d) => d.date.startsWith(String(s.year)) && (month === null || Number(d.date.slice(5, 7)) === month + 1));
      return {
        year: s.year,
        n: days.length,
        meanMax: mean(days, "tempMax"),
        meanMin: mean(days, "tempMin"),
        rain: total(days, "rain"),
        rainDays: days.filter((d) => (val(d, "rain") ?? 0) >= RAIN_DAY).length,
        gust: extreme(days, "gustMax", "max")?.value ?? null,
      };
    });
    const key: Record<CompareMetric, (r: (typeof rows)[number]) => number> = {
      tempMax: (r) => r.meanMax ?? -Infinity,
      tempMin: (r) => r.meanMin ?? -Infinity,
      rainCum: (r) => r.rain,
      gustMax: (r) => r.gust ?? -Infinity,
    };
    rows.sort((a, b) => key[metric](b) - key[metric](a));
    const expected = month === null ? 365 : 28;
    const table = el("table", { class: "data" },
      el("thead", {}, el("tr", {},
        el("th", {}, "Year"), el("th", { class: "n" }, `Avg max`), el("th", { class: "n" }, "Avg min"),
        el("th", { class: "n" }, `Rain`), el("th", { class: "n" }, "Rain days"), el("th", { class: "n" }, "Gust"))),
      el("tbody", {}, ...rows.map((r) => el("tr", { class: r.year === hl ? "hl" : "" },
        el("td", {}, String(r.year), r.n < expected ? el("span", { class: "partial", title: `${r.n} days of data` }, " partial") : ""),
        el("td", { class: "n" }, num(r.meanMax)), el("td", { class: "n" }, num(r.meanMin)),
        el("td", { class: "n" }, num(r.rain)), el("td", { class: "n" }, String(r.rainDays)), el("td", { class: "n" }, num(r.gust, 0))))),
    );
    rankHost.replaceChildren(table, el("p", { class: "muted small" }, `Sorted by ${METRICS.find((m) => m.id === metric)!.label.toLowerCase()}. Temperatures in ${u.temp}, rain in ${u.rain}, gusts in ${u.wind}.`));
  }

  // ----- Two periods
  const inputs = { aS: dateIn("Period A start"), aE: dateIn("Period A end"), bS: dateIn("Period B start"), bE: dateIn("Period B end") };
  function dateIn(label: string) {
    const i = el("input", { type: "date", "aria-label": label }) as HTMLInputElement;
    i.addEventListener("change", renderPeriods);
    return i;
  }
  const quick = el("div", { class: "chips" });
  const quickDefs: [string, () => [string, string, string, string]][] = [
    ["This month vs same month last year", () => {
      const l = lastDate()!; const s = l.slice(0, 8) + "01";
      return [addYears(s, -1), addYears(l, -1), s, l];
    }],
    ["This year vs last year, to date", () => {
      const l = lastDate()!; const s = l.slice(0, 5) + "01-01";
      return [addYears(s, -1), addYears(l, -1), s, l];
    }],
    ["Last 30 days vs the 30 before", () => {
      const l = lastDate()!;
      return [addDays(l, -59), addDays(l, -30), addDays(l, -29), l];
    }],
  ];
  for (const [label, fn] of quickDefs) {
    const b = el("button", { class: "chip" }, label);
    b.addEventListener("click", () => {
      if (!lastDate()) return;
      [inputs.aS.value, inputs.aE.value, inputs.bS.value, inputs.bE.value] = fn();
      renderPeriods();
    });
    quick.append(b);
  }
  const periodsTable = el("div", {});
  const roses = el("div", { class: "roses" });
  const periodsPanel = el("div", {},
    el("div", { class: "toolbar periods" },
      el("div", { class: "period a" }, el("span", { class: "tag" }, "A"), inputs.aS, el("span", { class: "muted" }, "to"), inputs.aE),
      el("div", { class: "period b" }, el("span", { class: "tag" }, "B"), inputs.bS, el("span", { class: "muted" }, "to"), inputs.bE)),
    quick,
    el("div", { class: "pair" }, el("section", { class: "panel" }, el("h2", {}, "Side by side"), periodsTable), el("section", { class: "panel" }, el("h2", {}, "Dominant wind direction"), roses)),
  );

  function renderPeriods() {
    const u = units();
    const { aS, aE, bS, bE } = inputs;
    if (!aS.value || !aE.value || !bS.value || !bE.value) {
      periodsTable.replaceChildren(el("p", { class: "muted" }, "Choose two date ranges, or start from one of the shortcuts above."));
      roses.replaceChildren();
      return;
    }
    const A = inRange(app.days, aS.value, aE.value);
    const B = inRange(app.days, bS.value, bE.value);
    type Row = [string, number | null, number | null, string, number];
    const rd = (d: typeof A) => d.filter((x) => (val(x, "rain") ?? 0) >= RAIN_DAY).length;
    const rows: Row[] = [
      ["Average max", mean(A, "tempMax"), mean(B, "tempMax"), u.temp, 1],
      ["Average min", mean(A, "tempMin"), mean(B, "tempMin"), u.temp, 1],
      ["Highest", extreme(A, "tempMax", "max")?.value ?? null, extreme(B, "tempMax", "max")?.value ?? null, u.temp, 1],
      ["Lowest", extreme(A, "tempMin", "min")?.value ?? null, extreme(B, "tempMin", "min")?.value ?? null, u.temp, 1],
      ["Rain", total(A, "rain"), total(B, "rain"), ` ${u.rain}`, 1],
      ["Rain days", rd(A), rd(B), "", 0],
      ["Peak gust", extreme(A, "gustMax", "max")?.value ?? null, extreme(B, "gustMax", "max")?.value ?? null, ` ${u.wind}`, 0],
      ["Days with data", A.length, B.length, "", 0],
    ];
    periodsTable.replaceChildren(el("table", { class: "data" },
      el("thead", {}, el("tr", {}, el("th", {}, ""), el("th", { class: "n" }, "A"), el("th", { class: "n" }, "B"), el("th", { class: "n" }, "B − A"))),
      el("tbody", {}, ...rows.map(([label, a, b, unit, dp]) => el("tr", {},
        el("td", {}, label),
        el("td", { class: "n" }, a === null ? "–" : `${num(a, dp)}${unit}`),
        el("td", { class: "n" }, b === null ? "–" : `${num(b, dp)}${unit}`),
        el("td", { class: "n diff" }, a === null || b === null ? "–" : `${signed(b - a, dp)}${unit}`)))),
    ));
    const rA = windRose(A), rB = windRose(B);
    const max = Math.max(0.05, ...rA.counts.map((c) => c / (rA.total || 1)), ...rB.counts.map((c) => c / (rB.total || 1)));
    roses.replaceChildren(
      el("figure", {}, windRoseSvg(rA.counts, rA.total, { label: "Period A", max }), el("figcaption", {}, `A: ${niceDate(aS.value)} to ${niceDate(aE.value)}`)),
      el("figure", {}, windRoseSvg(rB.counts, rB.total, { label: "Period B", max }), el("figcaption", {}, `B: ${niceDate(bS.value)} to ${niceDate(bE.value)}`)),
    );
  }

  function render() {
    tabYears.setAttribute("aria-selected", String(mode === "years"));
    tabPeriods.setAttribute("aria-selected", String(mode === "periods"));
    charts.splice(0).forEach((c) => c.destroy());
    if (!app.days.length) {
      body.replaceChildren(el("div", { class: "empty" }, el("h2", {}, "Nothing to compare yet"), el("p", {}, "Comparisons appear once Isobar has your station's daily history."), el("a", { href: "#/settings", class: "button" }, "Open settings")));
      return;
    }
    if (mode === "years") {
      body.replaceChildren(yearsControls, yearsPanel);
      requestAnimationFrame(renderYears);
    } else {
      body.replaceChildren(periodsPanel);
      if (!inputs.aS.value && lastDate()) [inputs.aS.value, inputs.aE.value, inputs.bS.value, inputs.bE.value] = quickDefs[0][1]();
      renderPeriods();
    }
  }

  render();
  cleanups.push(on("days", render));
  return {
    el: root,
    destroy: () => { cleanups.forEach((c) => c()); charts.forEach((c) => c.destroy()); },
    rerender: render,
  };
}

function addYears(iso: string, n: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  const nd = Math.min(d, new Date(y + n, m, 0).getDate());
  return `${y + n}-${String(m).padStart(2, "0")}-${String(nd).padStart(2, "0")}`;
}
