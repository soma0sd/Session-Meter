<script lang="ts">
  import { t } from "../i18n";
  import type { UsageSnapshot } from "../ipc";
  import { catalogEntry, type DisplayMode } from "./types";
  import FocusSlim from "./FocusSlim.svelte";
  import ConcentricRings from "./ConcentricRings.svelte";
  import DualSemiArc from "./DualSemiArc.svelte";
  import VerticalPillars from "./VerticalPillars.svelte";
  import HexRings from "./HexRings.svelte";

  // Renders one of the five widget-style concepts (in its detailed/compact variant) from a
  // usage snapshot. Shared by the live widget and the style-window preview.
  let {
    styleId,
    snapshot,
    now,
    displayMode,
    primaryKeyOverride = null,
    secondaryKeyOverride = null,
  }: {
    styleId: string;
    snapshot: UsageSnapshot;
    now: number;
    displayMode: DisplayMode;
    /** Antigravity-only: show this bucket pair instead of the snapshot's own headline
     *  (five_hour/weekly_primary). Every other caller omits these and gets the default
     *  behavior unchanged. */
    primaryKeyOverride?: string | null;
    secondaryKeyOverride?: string | null;
  } = $props();

  const components = {
    focusSlim: FocusSlim,
    concentricRings: ConcentricRings,
    dualSemiArc: DualSemiArc,
    verticalPillars: VerticalPillars,
    hexRings: HexRings,
  };

  const entry = $derived(catalogEntry(styleId));
  const Comp = $derived(components[entry.concept]);

  function windowLabel(key: string | null | undefined, fallbackKey: string): string {
    const k = key ?? fallbackKey;
    const loc = $t("bucket." + k);
    if (!loc.startsWith("bucket.")) return loc;
    const b = snapshot.buckets.find((x) => x.key === k);
    return b?.label ?? k;
  }

  const primary = $derived(
    (primaryKeyOverride ? snapshot.buckets.find((b) => b.key === primaryKeyOverride) : undefined) ??
      snapshot.five_hour,
  );
  const secondary = $derived(
    (secondaryKeyOverride ? snapshot.buckets.find((b) => b.key === secondaryKeyOverride) : undefined) ??
      snapshot.weekly_primary,
  );
  const primaryPct = $derived(primary ? primary.utilization : 0);
  const secondaryPct = $derived(secondary ? secondary.utilization : 0);
  const primaryResetMs = $derived(
    primary ? Math.max(0, Date.parse(primary.resets_at) - now) : 0,
  );
  const secondaryResetMs = $derived(
    secondary ? Math.max(0, Date.parse(secondary.resets_at) - now) : 0,
  );
  const primaryLabel = $derived(windowLabel(primaryKeyOverride ?? snapshot.primary_key, "five_hour"));
  const secondaryLabel = $derived(
    windowLabel(secondaryKeyOverride ?? snapshot.secondary_key, "seven_day"),
  );
</script>

<Comp
  variant={entry.variant}
  {displayMode}
  {primaryPct}
  {secondaryPct}
  {primaryResetMs}
  {secondaryResetMs}
  {primaryLabel}
  {secondaryLabel} />
