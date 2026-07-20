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

  const OUTER_LEN = Math.PI * 45;
  const INNER_LEN = Math.PI * 33;
  const clamp = (v: number) => Math.max(0, Math.min(100, v));
  const outerOff = $derived(OUTER_LEN - (clamp(pShown) / 100) * OUTER_LEN);
  const innerOff = $derived(INNER_LEN - (clamp(sShown) / 100) * INNER_LEN);
</script>

<div class="da" class:compact={variant === "compact"}>
  <div class="arcwrap">
    <svg viewBox="0 0 100 56">
      <path class="track" d="M5 50 A45 45 0 0 1 95 50" stroke-width="6" />
      <path class="p" d="M5 50 A45 45 0 0 1 95 50" stroke-width="6" stroke-dasharray={OUTER_LEN} stroke-dashoffset={outerOff} />
      <path class="track" d="M17 50 A33 33 0 0 1 83 50" stroke-width="6" />
      <path class="s" d="M17 50 A33 33 0 0 1 83 50" stroke-width="6" stroke-dasharray={INNER_LEN} stroke-dashoffset={innerOff} />
    </svg>
    <div class="center">{formatCountdown(primaryResetMs, $locale)}</div>
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
  .da {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .da.compact {
    gap: 9px;
  }
  .arcwrap {
    position: relative;
    width: 112px;
    height: 64px;
    flex-shrink: 0;
  }
  .da.compact .arcwrap {
    width: 74px;
    height: 42px;
  }
  svg {
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  path {
    fill: none;
    stroke-linecap: round;
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
    bottom: 0;
    left: 50%;
    transform: translateX(-50%);
    font-size: 0.72rem;
    font-weight: 700;
    color: rgb(var(--fg));
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    text-shadow:
      0 0 3px rgb(var(--panel)),
      0 0 3px rgb(var(--panel)),
      0 0 3px rgb(var(--panel));
  }
  .da.compact .center {
    font-size: 0.6rem;
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
