// Depletion forecast: fit a line to recent "remaining %" history and estimate when it
// reaches zero.

export interface Forecast {
  depleting: boolean;
  minutesToEmpty: number | null;
}

const STABLE = (): Forecast => ({ depleting: false, minutesToEmpty: null });

/**
 * @param times   epoch ms per sample
 * @param values  remaining % per sample (same length as times)
 * @param currentRemaining  latest remaining %
 */
export function forecast(
  times: number[],
  values: number[],
  currentRemaining: number,
): Forecast {
  const n = Math.min(times.length, values.length);
  if (n < 3) return STABLE();

  const t0 = times[0];
  let sx = 0;
  let sy = 0;
  let sxx = 0;
  let sxy = 0;
  for (let i = 0; i < n; i++) {
    const x = (times[i] - t0) / 60000; // minutes
    const y = values[i];
    sx += x;
    sy += y;
    sxx += x * x;
    sxy += x * y;
  }
  const denom = n * sxx - sx * sx;
  if (denom === 0) return STABLE();

  const slope = (n * sxy - sx * sy) / denom; // % per minute
  if (slope >= -0.01) return STABLE(); // flat or refilling

  const minutesToEmpty = currentRemaining / -slope;
  return { depleting: true, minutesToEmpty };
}
