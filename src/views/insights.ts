import { app, on, units } from "../lib/store";
import { anomalies, inRange, monthlyClimate, records, windRose, yearsCovered, type RecordRow } from "../lib/analytics";
import { addDays, COMPASS_LABELS, el, MONTHS, MONTHS_LONG, niceDate, num, signed } from "../lib/format";
import { windRoseSvg } from "../lib/windrose";
import type { View } from "./view";

let anomalyWindow = 90;
let recordScope: "all" | "year" = "all";

export function insightsView(): View {
  const root = el("section", { class: "view insights" });
  const cleanups: (() => void)[] = [];
  root.append(el("header", { class: "view-head" }, el("h1", {}, "Insights")));
  const body = el("div", { class: "insights-body" });
  root.append(body);

  function render() {
    const days = app.days;
    if (!days.length) {
      body.replaceChildren(el("div", { class: "empty" }, el("h2", {}, "No history yet"), el("p", {}, "Records, anomalies and climate summaries appear once Isobar has your station's daily history."), el("a", { href: "#/settings", class: "button" }, "Open settings")));
      return;
    }
    const u = units();
    const last = days[days.length - 1].date;

    // ----- Anomalies
    const winChips = el("div", { class: "chips" }, ...[30, 90, 365].map((n) => {
      const b = el("button", { class: `chip${n === anomalyWindow ? " on" : ""}`, "aria-pressed": String(n === anomalyWindow) }, n === 365 ? "12 months" : `${n} days`);
      b.addEventListener("click", () => { anomalyWindow = n; render(); });
      return b;
    }));
    const anomalyPanel = el("section", { class: "panel anomalies" },
      el("div", { class: "panel-head" }, el("h2", {}, "Unusual days"), winChips),
      el("p", { class: "muted" }, "Each day is compared with the normal for that time of year: the average of the same dates, two weeks either side, in every other year."));
    if (yearsCovered(days) < 2) {
      anomalyPanel.append(el("p", {}, "This needs at least two years of history to know what normal looks like."));
    } else {
      const list = anomalies(days, addDays(last, -(anomalyWindow - 1)), last, 1.5).slice(0, 14);
      if (!list.length) {
        anomalyPanel.append(el("p", {}, "Nothing unusual. Every day in this window was within 1.5 standard deviations of normal."));
      } else {
        anomalyPanel.append(el("ol", { class: "anomaly-list" }, ...list.map((a) => {
          const warm = a.z > 0;
          const what = a.field === "tempMax" ? (warm ? "Hot day" : "Cold day") : warm ? "Warm night" : "Cold night";
          const bar = el("span", { class: `zbar ${warm ? "warm" : "cool"}` });
          bar.style.setProperty("--w", `${Math.min(100, (Math.abs(a.z) / 3.5) * 100)}%`);
          return el("li", {},
            el("span", { class: "a-date" }, niceDate(a.date)),
            el("span", { class: "a-what" }, what),
            el("span", { class: "a-val" }, `${num(a.value)}${u.temp}`),
            el("span", { class: "a-norm muted" }, `normal ${num(a.normal)}${u.temp}, ${signed(a.value - a.normal)}${u.temp}`),
            el("span", { class: "a-z" }, bar, el("span", { class: "z" }, `${signed(a.z, 1)}σ`)),
          );
        })));
      }
    }

    // ----- Records
    const scopeChips = el("div", { class: "chips" }, ...(["all", "year"] as const).map((s) => {
      const b = el("button", { class: `chip${s === recordScope ? " on" : ""}`, "aria-pressed": String(s === recordScope) }, s === "all" ? "All time" : `This year`);
      b.addEventListener("click", () => { recordScope = s; render(); });
      return b;
    }));
    const recDays = recordScope === "all" ? days : inRange(days, last.slice(0, 5) + "01-01", last);
    const unitOf = (r: RecordRow) => (r.unit === "temp" ? u.temp : r.unit === "rain" ? ` ${u.rain}` : r.unit === "rate" ? ` ${u.rain}/h` : r.unit === "wind" ? ` ${u.wind}` : r.unit === "press" ? ` ${u.press}` : " days");
    const recPanel = el("section", { class: "panel" },
      el("div", { class: "panel-head" }, el("h2", {}, "Records"), scopeChips),
      el("dl", { class: "records" }, ...records(recDays).map((r) => el("div", { class: `rec ${r.unit}` },
        el("dt", {}, r.label),
        el("dd", {},
          el("span", { class: "rec-val" }, r.value === null ? "–" : `${num(r.value, r.unit === "days" || r.unit === "wind" ? 0 : 1)}${unitOf(r)}`),
          el("span", { class: "rec-when muted" }, r.date ? (r.detail === "month" ? `${MONTHS_LONG[Number(r.date.slice(5, 7)) - 1]} ${r.date.slice(0, 4)}` : `${niceDate(r.date)}${r.detail ? ` ${r.detail.startsWith("to ") ? "to " + niceDate(r.detail.slice(3)) : r.detail}` : ""}`) : ""),
        )))),
    );

    // ----- Climate table
    const clim = monthlyClimate(days);
    const climPanel = el("section", { class: "panel" },
      el("h2", {}, "Your station's climate"),
      el("p", { class: "muted" }, `Monthly averages from ${niceDate(days[0].date)} to ${niceDate(last)}. Rain figures only count months with at least 25 days of data.`),
      el("div", { class: "scroll-x" }, el("table", { class: "data climate" },
        el("thead", {}, el("tr", {}, el("th", {}, ""), ...MONTHS.map((m) => el("th", { class: "n" }, m)))),
        el("tbody", {},
          climRow("Average max", clim.map((c) => num(c.meanMax)), "heat"),
          climRow("Average min", clim.map((c) => num(c.meanMin)), "cold"),
          climRow("Highest", clim.map((c) => num(c.recordHigh?.value)), "", clim.map((c) => (c.recordHigh ? niceDate(c.recordHigh.date) : ""))),
          climRow("Lowest", clim.map((c) => num(c.recordLow?.value)), "", clim.map((c) => (c.recordLow ? niceDate(c.recordLow.date) : ""))),
          climRow(`Rain (${u.rain})`, clim.map((c) => num(c.meanRain)), "rain"),
          climRow("Rain days", clim.map((c) => num(c.meanRainDays, 0)), ""),
          climRow("Years", clim.map((c) => String(c.years)), "muted"),
        ))),
    );

    // ----- Wind rose
    const yearDays = inRange(days, last.slice(0, 5) + "01-01", last);
    const rAll = windRose(days), rYear = windRose(yearDays);
    const roseMax = Math.max(0.05, ...rAll.counts.map((c) => c / (rAll.total || 1)), ...rYear.counts.map((c) => c / (rYear.total || 1)));
    const topDir = (r: { counts: number[] }) => COMPASS_LABELS[r.counts.indexOf(Math.max(...r.counts))];
    const rosePanel = el("section", { class: "panel" },
      el("h2", {}, "Prevailing wind"),
      el("p", { class: "muted" }, "Share of days by dominant wind direction."),
      el("div", { class: "roses" },
        el("figure", {}, windRoseSvg(rAll.counts, rAll.total, { label: "All time", max: roseMax }), el("figcaption", {}, `All time, mostly ${topDir(rAll)}`)),
        el("figure", {}, windRoseSvg(rYear.counts, rYear.total, { label: "This year", max: roseMax }), el("figcaption", {}, `${last.slice(0, 4)}, mostly ${rYear.total ? topDir(rYear) : "–"}`)),
      ),
    );

    body.replaceChildren(el("div", { class: "pair wide-left" }, anomalyPanel, recPanel), climPanel, rosePanel);
  }

  render();
  cleanups.push(on("days", render));
  return { el: root, destroy: () => cleanups.forEach((c) => c()), rerender: render };
}

function climRow(label: string, values: string[], tone: string, titles?: string[]) {
  return el("tr", { class: tone }, el("th", { scope: "row" }, label), ...values.map((v, i) => el("td", { class: "n", ...(titles?.[i] ? { title: titles[i] } : {}) }, v)));
}
