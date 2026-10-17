import { COMPASS_LABELS } from "./format";

const NS = "http://www.w3.org/2000/svg";

/** A 16-petal rose of direction frequencies. */
export function windRoseSvg(counts: number[], total: number, opts: { label: string; max?: number } = { label: "Wind rose" }): SVGSVGElement {
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "-110 -110 220 220");
  svg.setAttribute("class", "rose");
  svg.setAttribute("role", "img");
  const pct = counts.map((c) => (total ? c / total : 0));
  const max = opts.max ?? Math.max(0.05, ...pct);
  const R = 82;

  for (const f of [0.25, 0.5, 0.75, 1]) {
    const c = document.createElementNS(NS, "circle");
    c.setAttribute("r", String(R * f));
    c.setAttribute("class", "rose-ring");
    svg.append(c);
  }
  const ringLabel = document.createElementNS(NS, "text");
  ringLabel.setAttribute("x", "3");
  ringLabel.setAttribute("y", String(-R - 3));
  ringLabel.setAttribute("class", "rose-scale");
  ringLabel.textContent = `${Math.round(max * 100)}%`;
  svg.append(ringLabel);

  pct.forEach((p, i) => {
    if (p <= 0) return;
    const r = (p / max) * R;
    const a0 = ((i * 22.5 - 9) * Math.PI) / 180;
    const a1 = ((i * 22.5 + 9) * Math.PI) / 180;
    const path = document.createElementNS(NS, "path");
    path.setAttribute(
      "d",
      `M0,0 L${Math.sin(a0) * r},${-Math.cos(a0) * r} A${r},${r} 0 0 1 ${Math.sin(a1) * r},${-Math.cos(a1) * r} Z`,
    );
    path.setAttribute("class", "rose-petal");
    const title = document.createElementNS(NS, "title");
    title.textContent = `${COMPASS_LABELS[i]}: ${(p * 100).toFixed(1)}% of days`;
    path.append(title);
    svg.append(path);
  });

  for (const [lab, deg] of [["N", 0], ["E", 90], ["S", 180], ["W", 270]] as const) {
    const a = (deg * Math.PI) / 180;
    const t = document.createElementNS(NS, "text");
    t.setAttribute("x", String(Math.sin(a) * (R + 16)));
    t.setAttribute("y", String(-Math.cos(a) * (R + 16) + 4));
    t.setAttribute("class", "rose-label");
    t.textContent = lab;
    svg.append(t);
  }
  const top = pct.indexOf(Math.max(...pct));
  svg.setAttribute("aria-label", `${opts.label}. Most common: ${COMPASS_LABELS[top]}, ${(pct[top] * 100).toFixed(0)}% of ${total} days.`);
  return svg;
}
