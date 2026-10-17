import uPlot from "uplot";

export function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

export function palette() {
  return {
    ink: cssVar("--ink"),
    ink2: cssVar("--ink-2"),
    grid: cssVar("--grid"),
    trace: cssVar("--trace"),
    heat: cssVar("--heat"),
    cold: cssVar("--cold"),
    rain: cssVar("--rain"),
    wind: cssVar("--wind"),
    warn: cssVar("--warn"),
    faint: cssVar("--faint"),
    paper: cssVar("--paper"),
  };
}

export function alpha(hex: string, a: number): string {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  const n = parseInt(full, 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
}

const FONT = '500 12px "Barlow", system-ui, sans-serif';

export function axis(opts: Partial<uPlot.Axis> = {}): uPlot.Axis {
  const p = palette();
  return {
    stroke: p.ink2,
    font: FONT,
    grid: { stroke: p.grid, width: 1 },
    ticks: { stroke: p.grid, width: 1, size: 4 },
    ...opts,
  };
}

/** Mount a chart that tracks its container's width. Returns a cleanup fn. */
export function mountChart(
  host: HTMLElement,
  height: number,
  build: (width: number, height: number) => uPlot.Options,
  data: uPlot.AlignedData,
): { chart: uPlot; destroy: () => void } {
  const width = Math.max(200, host.clientWidth || 600);
  const chart = new uPlot(build(width, height), data, host);
  const ro = new ResizeObserver(() => {
    const w = host.clientWidth;
    if (w > 0 && Math.abs(w - chart.width) > 2) chart.setSize({ width: w, height });
  });
  ro.observe(host);
  return { chart, destroy: () => { ro.disconnect(); chart.destroy(); } };
}

/** Minimal line with no axes, for instrument sparklines. */
export function sparkline(host: HTMLElement, xs: number[], ys: (number | null)[], color: string, height = 44, fill = false) {
  return mountChart(
    host,
    height,
    (w, h) => ({
      width: w,
      height: h,
      padding: [4, 2, 2, 2],
      legend: { show: false },
      cursor: { show: false },
      select: { show: false, left: 0, top: 0, width: 0, height: 0 },
      axes: [{ show: false }, { show: false }],
      scales: { x: { time: true } },
      series: [{}, { stroke: color, width: 1.75, fill: fill ? alpha(color, 0.14) : undefined, spanGaps: true }],
    }),
    [xs, ys],
  );
}

export const tempFmt = (u: string) => (_: uPlot, v: number) => (v === null || v === undefined ? "–" : `${v.toFixed(1)}${u}`);

const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** Day-first time labels: "3pm", "16 Sep", "Sep 2026" depending on the visible span. */
export function timeAxis(opts: Partial<uPlot.Axis> = {}): uPlot.Axis {
  return axis({
    space: 64,
    values: (u, splits) => {
      const min = u.scales.x.min ?? 0;
      const max = u.scales.x.max ?? 0;
      const span = max - min;
      return splits.map((s) => {
        const d = new Date(s * 1000);
        if (span <= 2 * 86400) {
          const h = d.getHours();
          if (h === 0) return `${d.getDate()} ${MON[d.getMonth()]}`;
          return `${h % 12 || 12}${h < 12 ? "am" : "pm"}`;
        }
        if (span <= 400 * 86400) return `${d.getDate()} ${MON[d.getMonth()]}`;
        return `${MON[d.getMonth()]} ${d.getFullYear()}`;
      });
    },
    ...opts,
  });
}
