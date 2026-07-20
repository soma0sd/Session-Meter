<script lang="ts">
  import { locale } from "../i18n";
  import { formatCountdown } from "../countdown";
  import { shownPct, type WidgetStyleProps } from "./types";
  import InfoGrid from "./InfoGrid.svelte";

  let {
    variant,
    displayMode,
    primaryPct,
    secondaryPct,
    primaryResetMs,
    secondaryResetMs,
    primaryLabel,
    secondaryLabel,
  }: WidgetStyleProps = $props();

  const pShown = $derived(shownPct(primaryPct, displayMode));
  const sShown = $derived(shownPct(secondaryPct, displayMode));

  const OUTER_R = 42;
  const INNER_R = 30;
  const OUTER_C = 2 * Math.PI * OUTER_R;
  const INNER_C = 2 * Math.PI * INNER_R;
  const clamp = (v: number) => Math.max(0, Math.min(100, v));
  const outerOff = $derived(OUTER_C - (clamp(sShown) / 100) * OUTER_C);
  const innerOff = $derived(INNER_C - (clamp(pShown) / 100) * INNER_C);
</script>

<div class="cr" class:compact={variant === "compact"}>
  <div class="ringwrap">
    <svg viewBox="0 0 100 100">
      <circle class="track" cx="50" cy="50" r={OUTER_R} stroke-width="6" />
      <circle class="s" cx="50" cy="50" r={OUTER_R} stroke-width="6" stroke-dasharray={OUTER_C} stroke-dashoffset={outerOff} transform="rotate(-90 50 50)" />
      <circle class="track" cx="50" cy="50" r={INNER_R} stroke-width="8" />
      <circle class="p" cx="50" cy="50" r={INNER_R} stroke-width="8" stroke-dasharray={INNER_C} stroke-dashoffset={innerOff} transform="rotate(-90 50 50)" />
    </svg>
    <div class="center">
      {#if variant === "compact"}
        <span class="ctime">{formatCountdown(primaryResetMs, $locale)}</span>
      {:else}
        <span class="cpct">{pShown}%</span>
      {/if}
    </div>
  </div>

  {#if variant === "detailed"}
    <InfoGrid {primaryLabel} {secondaryLabel} {pShown} {sShown} {primaryResetMs} {secondaryResetMs} />
  {:else}
    <div class="side">
      <span class="sp">{pShown}%</span>
      <span class="ss">{sShown}%</span>
    </div>
  {/if}
</div>

<style>
  .cr {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .cr.compact {
    gap: 9px;
  }
  .ringwrap {
    position: relative;
    width: 84px;
    height: 84px;
    flex-shrink: 0;
  }
  .cr.compact .ringwrap {
    width: 54px;
    height: 54px;
  }
  svg {
    width: 100%;
    height: 100%;
  }
  circle {
    fill: none;
    stroke-linecap: round;
    transition: stroke-dashoffset 0.4s cubic-bezier(0.1, 0.8, 0.3, 1);
  }
  .track {
    stroke: rgb(var(--track));
  }
  circle.p {
    stroke: rgb(var(--m1));
    filter: drop-shadow(0 0 2px rgb(var(--m1) / 0.5));
  }
  circle.s {
    stroke: rgb(var(--m2));
  }
  .center {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
  }
  /* Halo in the panel colour so the value stays readable where it crosses the rings. */
  .cpct,
  .ctime {
    text-shadow:
      0 0 3px rgb(var(--panel)),
      0 0 3px rgb(var(--panel)),
      0 0 3px rgb(var(--panel));
  }
  .cpct {
    font-size: 1.15rem;
    font-weight: 800;
    color: rgb(var(--fg));
    font-variant-numeric: tabular-nums;
  }
  .ctime {
    font-size: 0.66rem;
    color: rgb(var(--fg));
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .side {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }
  .sp {
    font-size: 1.2rem;
    font-weight: 800;
    line-height: 1;
    color: rgb(var(--m1));
    font-variant-numeric: tabular-nums;
  }
  .ss {
    font-size: 0.82rem;
    font-weight: 700;
    line-height: 1;
    color: rgb(var(--m2));
    font-variant-numeric: tabular-nums;
  }
</style>
