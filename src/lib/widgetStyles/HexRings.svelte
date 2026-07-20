<script lang="ts">
  import { formatClock } from "../countdown";
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

  const OUTER = "M50 10 L84.64 30 L84.64 70 L50 90 L15.36 70 L15.36 30 Z";
  const INNER = "M50 20 L75.98 35 L75.98 65 L50 80 L24.02 65 L24.02 35 Z";
  const OUTER_LEN = 240;
  const INNER_LEN = 180;
  const clamp = (v: number) => Math.max(0, Math.min(100, v));
  const outerOff = $derived(OUTER_LEN - (clamp(pShown) / 100) * OUTER_LEN);
  const innerOff = $derived(INNER_LEN - (clamp(sShown) / 100) * INNER_LEN);
</script>

<div class="hx" class:compact={variant === "compact"}>
  <div class="hexwrap">
    <svg viewBox="0 0 100 100">
      <path class="track" d={OUTER} stroke-width="6" />
      <path class="p" d={OUTER} stroke-width="6" stroke-dasharray={OUTER_LEN} stroke-dashoffset={outerOff} />
      <path class="track" d={INNER} stroke-width="6" />
      <path class="s" d={INNER} stroke-width="6" stroke-dasharray={INNER_LEN} stroke-dashoffset={innerOff} />
    </svg>
    <div class="center">
      {#if variant === "compact"}
        <span class="ctime">{formatClock(primaryResetMs)}</span>
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
  .hx {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .hx.compact {
    gap: 9px;
  }
  .hexwrap {
    position: relative;
    width: 84px;
    height: 84px;
    flex-shrink: 0;
  }
  .hx.compact .hexwrap {
    width: 56px;
    height: 56px;
  }
  svg {
    width: 100%;
    height: 100%;
  }
  path {
    fill: none;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: stroke-dashoffset 0.4s cubic-bezier(0.1, 0.8, 0.3, 1);
  }
  path.track {
    stroke: rgb(var(--track));
  }
  path.p {
    stroke: rgb(var(--m1));
    filter: drop-shadow(0 0 2px rgb(var(--m1) / 0.5));
  }
  path.s {
    stroke: rgb(var(--m2));
  }
  .center {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
  }
  .cpct,
  .ctime {
    text-shadow:
      0 0 3px rgb(var(--panel)),
      0 0 3px rgb(var(--panel)),
      0 0 3px rgb(var(--panel));
  }
  .cpct {
    font-size: 1.1rem;
    font-weight: 800;
    color: rgb(var(--fg));
    font-variant-numeric: tabular-nums;
  }
  .ctime {
    font-size: 0.62rem;
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
    font-size: 1.15rem;
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
