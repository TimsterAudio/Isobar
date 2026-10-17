import uPlot from "uplot";
import { app, on, units } from "../lib/store";
import { alpha, axis, mountChart, palette, sparkline, timeAxis } from "../lib/charts";
import { ago, compass, el, num, signed } from "../lib/format";
import type { View } from "./view";

type Chart = { chart: uPlot; destroy: () => void };

function series(key: string): [number[], (number | null)[]] {
  const pts = app.samples[key] ?? [];
  return [pts.map((p) => p[0] / 1000), pts.map((p) => p[1])];
}

function trendWord(v: number | null | undefined, steady: number): string {
  if (v === null || v === undefined) return "";
  if (Math.abs(v) < steady) return "steady";
  return v > 0 ? "rising" : "falling";
}

function uvBand(uv: number | null | undefined): string {
  if (uv === null || uv === undefined) return "";
  if (uv < 3) return "low";
  if (uv < 6) return "moderate";
  if (uv < 8) return "high";
  if (uv < 11) return "very high";
  return "extreme";
}

function windDial(): { svg: SVGSVGElement; set: (bearing: number | null | undefined, avg: number | null | undefined) => void } {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "-60 -60 120 120");
  svg.setAttribute("class", "dial");
  svg.setAttribute("role", "img");
  const ring = document.createElementNS(NS, "circle");
  ring.setAttribute("r", "50");
  ring.setAttribute("class", "dial-ring");
  svg.append(ring);
  for (let i = 0; i < 32; i++) {
    const a = (i * 11.25 * Math.PI) / 180;
    const major = i % 8 === 0;
    const r1 = major ? 42 : i % 2 === 0 ? 45 : 47;
    const t = document.createElementNS(NS, "line");
    t.setAttribute("x1", String(Math.sin(a) * r1));
    t.setAttribute("y1", String(-Math.cos(a) * r1));
    t.setAttribute("x2", String(Math.sin(a) * 50));
    t.setAttribute("y2", String(-Math.cos(a) * 50));
    t.setAttribute("class", major ? "dial-tick major" : "dial-tick");
    svg.append(t);
  }
  for (const [label, deg] of [["N", 0], ["E", 90], ["S", 180], ["W", 270]] as const) {
    const a = (deg * Math.PI) / 180;
    const tx = document.createElementNS(NS, "text");
    tx.setAttribute("x", String(Math.sin(a) * 33));
    tx.setAttribute("y", String(-Math.cos(a) * 33 + 3.5));
    tx.setAttribute("class", "dial-label");
    tx.textContent = label;
    svg.append(tx);
  }
  const needle = document.createElementNS(NS, "g");
  needle.setAttribute("class", "dial-needle");
  const tri = document.createElementNS(NS, "path");
  // Points inward from the rim: the wind comes FROM this direction.
  tri.setAttribute("d", "M0,-56 L7,-41 L-7,-41 Z");
  needle.append(tri);
  svg.append(needle);
  const centre = document.createElementNS(NS, "text");
  centre.setAttribute("class", "dial-value");
  centre.setAttribute("y", "6");
  svg.append(centre);
  const unit = document.createElementNS(NS, "text");
  unit.setAttribute("class", "dial-unit");
  unit.setAttribute("y", "18");
  svg.append(unit);
  return {
    svg,
    set: (bearing, avg) => {
      needle.style.transform = `rotate(${bearing ?? 0}deg)`;
      needle.style.opacity = bearing === null || bearing === undefined ? "0" : "1";
      centre.textContent = num(avg, 0);
      unit.textContent = units().wind;
      svg.setAttribute("aria-label", `Wind from ${compass(bearing)} at ${num(avg, 0)} ${units().wind}`);
    },
  };
}

export function nowView(): View {
  const root = el("section", { class: "view now" });
  const cleanups: (() => void)[] = [];
  const charts: Chart[] = [];

  // --- Header
  const updated = el("span", { class: "updated" });
  const forecast = el("p", { class: "forecast" });
  root.append(
    el("header", { class: "view-head" },
      el("h1", {}, app.settings.stationName || "My station"),
      updated,
    ),
  );

  // --- Hero: temperature + thermograph
  const tempBig = el("div", { class: "temp-big" });
  const tempTrend = el("div", { class: "temp-trend" });
  const hiLo = el("dl", { class: "hilo" });
  const feels = el("p", { class: "feels" });
  const thermo = el("div", { class: "thermograph" });
  root.append(
    el("div", { class: "hero" },
      el("div", { class: "hero-reading" }, tempBig, tempTrend, hiLo, feels, forecast),
      el("figure", { class: "hero-chart" },
        thermo,
        el("figcaption", {}, el("span", { class: "key trace" }, "Temperature"), el("span", { class: "key dew" }, "Dew point"), el("span", { class: "muted" }, "Last 24 hours")),
      ),
    ),
  );

  // --- Instruments
  const dial = windDial();
  const windText = el("div", { class: "inst-detail" });
  const pressVal = el("div", { class: "inst-value" });
  const pressText = el("div", { class: "inst-detail" });
  const rainVal = el("div", { class: "inst-value" });
  const rainText = el("div", { class: "inst-detail" });
  const humVal = el("div", { class: "inst-value" });
  const humText = el("div", { class: "inst-detail" });
  const sunVal = el("div", { class: "inst-value" });
  const sunText = el("div", { class: "inst-detail" });
  const sparks = {
    gust: el("div", { class: "spark" }),
    press: el("div", { class: "spark" }),
    rain: el("div", { class: "spark" }),
    hum: el("div", { class: "spark" }),
    solar: el("div", { class: "spark" }),
  };
  const inst = (title: string, ...kids: Node[]) => el("article", { class: "instrument" }, el("h2", {}, title), ...kids);
  root.append(
    el("div", { class: "instruments" },
      inst("Wind", el("div", { class: "dial-wrap" }, dial.svg), windText, sparks.gust),
      inst("Pressure", pressVal, pressText, sparks.press),
      inst("Rain", rainVal, rainText, sparks.rain),
      inst("Humidity", humVal, humText, sparks.hum),
      inst("Sun", sunVal, sunText, sparks.solar),
    ),
  );

  const empty = el("div", { class: "empty" },
    el("h2", {}, "No live data yet"),
    el("p", {}, "Add your Cumulus MX address in Settings and Isobar will start reading your station."),
    el("a", { href: "#/settings", class: "button" }, "Open settings"),
  );

  function renderValues() {
    const r = app.live;
    const u = units();
    empty.remove();
    if (!r) {
      root.append(empty);
      return;
    }
    const t = r.temp;
    tempBig.innerHTML = "";
    tempBig.append(num(t, 1), el("span", { class: "deg" }, u.temp));
    const tr = r.tempTrend;
    tempTrend.textContent = tr === null || tr === undefined ? "" : `${tr > 0.05 ? "↑" : tr < -0.05 ? "↓" : "→"} ${signed(tr)}${u.temp} per hour`;
    tempTrend.className = `temp-trend ${tr && tr > 0.05 ? "up" : tr && tr < -0.05 ? "down" : ""}`;
    hiLo.innerHTML = "";
    hiLo.append(
      el("div", { class: "hi" }, el("dt", {}, "High"), el("dd", {}, `${num(r.tempHigh)}${u.temp}`, el("small", {}, r.tempHighTime ? ` ${r.tempHighTime}` : ""))),
      el("div", { class: "lo" }, el("dt", {}, "Low"), el("dd", {}, `${num(r.tempLow)}${u.temp}`, el("small", {}, r.tempLowTime ? ` ${r.tempLowTime}` : ""))),
    );
    const fl = r.feelsLike ?? r.apparent;
    feels.textContent = `Feels like ${num(fl)}${u.temp}, dew point ${num(r.dewPoint)}${u.temp}`;
    forecast.textContent = r.forecast ?? "";

    dial.set(r.bearing, r.windAvg);
    windText.innerHTML = "";
    windText.append(
      el("p", {}, `From ${r.bearingCompass ?? compass(r.bearing)}${r.bearing !== null && r.bearing !== undefined ? ` (${Math.round(r.bearing)}°)` : ""}`),
      el("p", {}, `Gusting ${num(r.gustRecent, 0)}, peak ${num(r.gustHigh, 0)} ${u.wind}`, el("small", {}, r.gustHighTime ? ` at ${r.gustHighTime}` : "")),
    );

    pressVal.innerHTML = "";
    pressVal.append(num(r.pressure, 1), el("span", { class: "unit" }, u.press));
    const pt = r.pressTrend;
    pressText.innerHTML = "";
    pressText.append(
      el("p", {}, pt !== null && pt !== undefined ? `${trendWord(pt, 0.1)}, ${signed(pt)} per hour` : r.pressTrendText ?? ""),
      el("p", { class: "muted" }, `Today ${num(r.pressLow, 1)} to ${num(r.pressHigh, 1)}`),
    );

    rainVal.innerHTML = "";
    rainVal.append(num(r.rainToday, 1), el("span", { class: "unit" }, u.rain));
    rainText.innerHTML = "";
    rainText.append(
      el("p", { class: (r.rainRate ?? 0) > 0 ? "raining" : "" }, (r.rainRate ?? 0) > 0 ? `Raining ${num(r.rainRate)} ${u.rain}/h` : "Not raining"),
      el("p", { class: "muted" }, `Month ${num(r.rainMonth)} · year ${num(r.rainYear)} ${u.rain}`),
    );

    humVal.innerHTML = "";
    humVal.append(num(r.humidity, 0), el("span", { class: "unit" }, "%"));
    humText.innerHTML = "";
    humText.append(
      el("p", {}, `Dew point ${num(r.dewPoint)}${u.temp}`),
      el("p", { class: "muted" }, `Today ${num(r.humLow, 0)}% to ${num(r.humHigh, 0)}%`),
    );

    sunVal.innerHTML = "";
    sunVal.append(num(r.solar, 0), el("span", { class: "unit" }, "W/m²"));
    sunText.innerHTML = "";
    sunText.append(
      el("p", {}, r.uv !== null && r.uv !== undefined ? `UV ${num(r.uv)}, ${uvBand(r.uv)}` : "No UV sensor"),
      el("p", { class: "muted" }, `Peak ${num(r.solarHigh, 0)} W/m²${r.uvHigh !== null && r.uvHigh !== undefined ? `, UV ${num(r.uvHigh)}` : ""}`),
    );
  }

  function renderUpdated() {
    const s = app.status;
    const stale = !s.lastOk || Date.now() - s.lastOk > Math.max(120e3, app.settings.pollSeconds * 3000);
    updated.className = `updated ${!s.connected ? "offline" : stale ? "stale" : "live"}`;
    updated.textContent = !app.settings.stationUrl && !app.api.demo
      ? "No station connected"
      : !s.connected && s.lastError
        ? `Offline: ${s.lastError}`
        : `Updated ${ago(s.lastOk)}`;
  }

  function renderCharts() {
    charts.splice(0).forEach((c) => c.destroy());
    const p = palette();
    const [tx, ty] = series("temp");
    const [, dy] = series("dew");
    const u = units();
    if (tx.length > 1) {
      charts.push(mountChart(thermo, 260, (w, h) => ({
        width: w, height: h,
        padding: [12, 8, 0, 4],
        legend: { show: false },
        cursor: { points: { size: 7 }, drag: { x: false, y: false } },
        scales: { x: { time: true } },
        axes: [
          timeAxis({ space: 70 }),
          axis({ size: 46, values: (_u, vals) => vals.map((v) => `${v}°`) }),
        ],
        series: [
          {},
          { label: `Temperature (${u.temp})`, stroke: p.trace, width: 2.5, spanGaps: true },
          { label: `Dew point (${u.temp})`, stroke: alpha(p.ink2, 0.7), width: 1.25, dash: [4, 3], spanGaps: true },
        ],
      }), [tx, ty, dy.length === tx.length ? dy : new Array(tx.length).fill(null)]));
    } else {
      thermo.replaceChildren(el("p", { class: "chart-empty" }, "The 24-hour trace fills in as Isobar records readings, one per minute."));
    }
    const mk = (host: HTMLElement, key: string, color: string, fill = false) => {
      const [x, y] = series(key);
      host.replaceChildren();
      if (x.length > 1) charts.push(sparkline(host, x, y, color, 44, fill));
    };
    mk(sparks.gust, "gust", p.wind);
    mk(sparks.press, "press", p.trace);
    mk(sparks.rain, "rain_today", p.rain, true);
    mk(sparks.hum, "hum", p.ink2);
    mk(sparks.solar, "solar", p.warn, true);
  }

  renderValues();
  renderUpdated();
  requestAnimationFrame(renderCharts);

  let lastChart = Date.now();
  cleanups.push(on("live", () => {
    renderValues();
    renderUpdated();
    if (Date.now() - lastChart > 60e3) { lastChart = Date.now(); renderCharts(); }
  }));
  cleanups.push(on("status", renderUpdated));
  cleanups.push(on("samples", renderCharts));
  const tick = setInterval(renderUpdated, 1000);
  cleanups.push(() => clearInterval(tick));

  return {
    el: root,
    destroy: () => { cleanups.forEach((c) => c()); charts.forEach((c) => c.destroy()); },
    rerender: renderCharts,
  };
}
