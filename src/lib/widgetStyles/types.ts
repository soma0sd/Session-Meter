// Shared types + catalog for the widget style library. Each "style" is one of five visual
// concepts in a detailed or compact variant (10 total). The same components render both the
// live desktop widget and the style-window preview, so they take pure data props.

export type WidgetVariant = "detailed" | "compact";
export type DisplayMode = "remaining" | "used";

export interface WidgetStyleProps {
  variant: WidgetVariant;
  displayMode: DisplayMode;
  /** Utilization % (0-100, used) of the primary window (Claude: 5-hour). */
  primaryPct: number;
  /** Utilization % (0-100, used) of the secondary window (Claude: weekly). */
  secondaryPct: number;
  /** Milliseconds until each window resets (for the countdown labels). */
  primaryResetMs: number;
  secondaryResetMs: number;
  primaryLabel: string;
  secondaryLabel: string;
}

export interface StyleCatalogEntry {
  id: string;
  concept: WidgetConcept;
  variant: WidgetVariant;
  labelKey: string;
}

export type WidgetConcept =
  | "focusSlim"
  | "concentricRings"
  | "dualSemiArc"
  | "verticalPillars"
  | "hexRings";

export const DEFAULT_STYLE = "focus-slim-detailed";

export const catalog: StyleCatalogEntry[] = [
  { id: "focus-slim-detailed", concept: "focusSlim", variant: "detailed", labelKey: "style.focusSlim.detailed" },
  { id: "focus-slim-compact", concept: "focusSlim", variant: "compact", labelKey: "style.focusSlim.compact" },
  { id: "concentric-rings-detailed", concept: "concentricRings", variant: "detailed", labelKey: "style.concentricRings.detailed" },
  { id: "concentric-rings-compact", concept: "concentricRings", variant: "compact", labelKey: "style.concentricRings.compact" },
  { id: "dual-semi-arc-detailed", concept: "dualSemiArc", variant: "detailed", labelKey: "style.dualSemiArc.detailed" },
  { id: "dual-semi-arc-compact", concept: "dualSemiArc", variant: "compact", labelKey: "style.dualSemiArc.compact" },
  { id: "vertical-pillars-detailed", concept: "verticalPillars", variant: "detailed", labelKey: "style.verticalPillars.detailed" },
  { id: "vertical-pillars-compact", concept: "verticalPillars", variant: "compact", labelKey: "style.verticalPillars.compact" },
  { id: "hex-rings-detailed", concept: "hexRings", variant: "detailed", labelKey: "style.hexRings.detailed" },
  { id: "hex-rings-compact", concept: "hexRings", variant: "compact", labelKey: "style.hexRings.compact" },
];

export function catalogEntry(id: string): StyleCatalogEntry {
  return catalog.find((c) => c.id === id) ?? catalog[0];
}

/** Per-service widget metric colours (RGB triplets for `rgb(var(--m1))`), matched to each
 * service's brand: Claude coral/amber, Gemini blue/purple. */
export const serviceColors: Record<string, { m1: string; m2: string }> = {
  claude: { m1: "217 119 87", m2: "224 164 88" },
  gemini: { m1: "66 133 244", m2: "167 85 247" },
};

export function colorsFor(service: string): { m1: string; m2: string } {
  return serviceColors[service] ?? { m1: "124 92 246", m2: "43 108 176" };
}

/** Colour band by remaining%: <20 danger, <50 warn, else ok. */
export function bandColor(remaining: number): string {
  return remaining < 20
    ? "rgb(var(--danger))"
    : remaining < 50
      ? "rgb(var(--warn))"
      : "rgb(var(--ok))";
}

/** The number shown for a window, per the display mode (used% or remaining%). */
export function shownPct(utilizationPct: number, mode: DisplayMode): number {
  const u = Math.max(0, Math.min(100, Math.round(utilizationPct)));
  return mode === "remaining" ? 100 - u : u;
}
