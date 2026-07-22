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
 * service's brand: Claude coral/amber, Gemini blue/purple, Antigravity IDE teal/violet. */
export const serviceColors: Record<string, { m1: string; m2: string }> = {
  claude: { m1: "217 119 87", m2: "224 164 88" },
  gemini: { m1: "66 133 244", m2: "167 85 247" },
  antigravity_ide: { m1: "20 184 166", m2: "139 92 246" },
};

export function colorsFor(service: string): { m1: string; m2: string } {
  return serviceColors[service] ?? { m1: "124 92 246", m2: "43 108 176" };
}

/** Small brand marks (raw inline SVG, sized for a 13x13 slot) identifying each service at a
 * glance - used in the widget title and the Placement tab's reorder list. Distinct
 * silhouettes: Claude a radial spark, Gemini a four-point sparkle, Antigravity an upward
 * chevron (a nod to "anti-gravity"). Shared between `Widget.svelte` and `Style.svelte` so
 * both stay in sync as services are added. */
export const serviceIcons: Record<string, string> = {
  claude: `<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8 1.7v12.6M1.7 8h12.6M3.5 3.5l9 9M12.5 3.5l-9 9"/></svg>`,
  gemini: `<svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor"><path d="M8 1.5Q8 8 14.5 8Q8 8 8 14.5Q8 8 1.5 8Q8 8 8 1.5Z"/></svg>`,
  antigravity_ide: `<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.2 10.5 8 3.5l5.8 7"/><path d="M4.6 13.2 8 9l3.4 4.2"/></svg>`,
};

export function iconFor(service: string): string {
  return serviceIcons[service] ?? "";
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
