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
  const clamp = (v: number) => Math.max(0, Math.min(100, v));
</script>

<div class="vp" class:compact={variant === "compact"}>
  <div class="pillars">
    <div class="pillar primary"><div class="fill p" style="height:{clamp(pShown)}%"></div></div>
    <div class="pillar secondary"><div class="fill s" style="height:{clamp(sShown)}%"></div></div>
  </div>

  {#if variant === "detailed"}
    <InfoGrid {primaryLabel} {secondaryLabel} {pShown} {sShown} {primaryResetMs} {secondaryResetMs} />
  {:else}
    <div class="side">
      <span class="sp">{pShown}%</span>
      <span class="ss">{sShown}%</span>
      <span class="rst">{formatClock(primaryResetMs)}</span>
    </div>
  {/if}
</div>

<style>
  .vp {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .vp.compact {
    gap: 9px;
  }
  .pillars {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    height: 82px;
    flex-shrink: 0;
  }
  .vp.compact .pillars {
    height: 50px;
    gap: 5px;
  }
  .pillar {
    position: relative;
    background: rgb(var(--track));
    border-radius: 7px;
    overflow: hidden;
  }
  .pillar.primary {
    width: 14px;
    height: 100%;
  }
  .pillar.secondary {
    width: 9px;
    height: 100%;
    border-radius: 5px;
  }
  .vp.compact .pillar.primary {
    width: 10px;
  }
  .vp.compact .pillar.secondary {
    width: 6px;
  }
  .fill {
    position: absolute;
    bottom: 0;
    left: 0;
    width: 100%;
    border-radius: inherit;
    transition: height 0.4s cubic-bezier(0.1, 0.8, 0.3, 1);
  }
  .fill.p {
    background: linear-gradient(to top, rgb(var(--m1) / 0.8), rgb(var(--m1)));
    box-shadow: 0 0 8px rgb(var(--m1) / 0.4);
  }
  .fill.s {
    background: rgb(var(--m2));
  }
  .side {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 1px;
  }
  .sp {
    font-size: 1.2rem;
    font-weight: 800;
    line-height: 1.1;
    color: rgb(var(--m1));
    font-variant-numeric: tabular-nums;
  }
  .ss {
    font-size: 0.82rem;
    font-weight: 700;
    line-height: 1.1;
    color: rgb(var(--m2));
    font-variant-numeric: tabular-nums;
  }
  .rst {
    font-size: 0.62rem;
    color: rgb(var(--fg-muted));
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
