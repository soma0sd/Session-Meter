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
