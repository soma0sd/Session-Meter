// Tiny dependency-free SVG chart helpers. Values are percentages (0..100); y is
// inverted so 100% is at the top. Points are spaced evenly along the width.

export function mapY(v: number, h: number, pad: number): number {
  const c = Math.max(0, Math.min(100, v));
  return pad + (1 - c / 100) * (h - 2 * pad);
}

export function xAt(i: number, n: number, w: number, pad: number): number {
  if (n <= 1) return pad;
  return pad + (i / (n - 1)) * (w - 2 * pad);
}

export function linePath(values: number[], w: number, h: number, pad = 3): string {
  const n = values.length;
  if (n === 0) return "";
  if (n === 1) {
    const y = mapY(values[0], h, pad);
    return `M ${pad} ${y.toFixed(1)} L ${(w - pad).toFixed(1)} ${y.toFixed(1)}`;
  }
  return values
    .map((v, i) => {
      const x = xAt(i, n, w, pad);
      const y = mapY(v, h, pad);
      return `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(" ");
}

export function areaPath(values: number[], w: number, h: number, pad = 3): string {
  const line = linePath(values, w, h, pad);
  if (!line) return "";
  const n = values.length;
  const lastX = xAt(n - 1, n, w, pad);
  return `${line} L ${lastX.toFixed(1)} ${(h - pad).toFixed(1)} L ${pad} ${(h - pad).toFixed(1)} Z`;
}

// --- time-based variants: x positions each point by its timestamp within a [t0, t1] window,
// so different ranges (24h / 7d / 30d) render at their true time scale instead of stretching
// the same points evenly across the width. ---

export function xAtTime(t: number, t0: number, t1: number, w: number, pad = 3): number {
  if (!(t1 > t0)) return w - pad;
  const frac = Math.max(0, Math.min(1, (t - t0) / (t1 - t0)));
  return pad + frac * (w - 2 * pad);
}

export function linePathT(
  times: number[],
  values: number[],
  t0: number,
  t1: number,
  w: number,
  h: number,
  pad = 3,
): string {
  const n = Math.min(times.length, values.length);
  if (n === 0) return "";
  let d = "";
  for (let i = 0; i < n; i++) {
    const x = xAtTime(times[i], t0, t1, w, pad);
    const y = mapY(values[i], h, pad);
    d += `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${y.toFixed(1)} `;
  }
  return d.trim();
}

export function areaPathT(
  times: number[],
  values: number[],
  t0: number,
  t1: number,
  w: number,
  h: number,
  pad = 3,
): string {
  const n = Math.min(times.length, values.length);
  if (n === 0) return "";
  const line = linePathT(times, values, t0, t1, w, h, pad);
  const lastX = xAtTime(times[n - 1], t0, t1, w, pad);
  const firstX = xAtTime(times[0], t0, t1, w, pad);
  return `${line} L ${lastX.toFixed(1)} ${(h - pad).toFixed(1)} L ${firstX.toFixed(1)} ${(h - pad).toFixed(1)} Z`;
}
