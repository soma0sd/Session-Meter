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

  // The fill (and the number) follow the remaining/used toggle.
  const pShown = $derived(shownPct(primaryPct, displayMode));
  const sShown = $derived(shownPct(secondaryPct, displayMode));
</script>

{#if variant === "detailed"}
  <div class="fs">
    <div class="metric">
      <div class="head">
        <span class="label">{primaryLabel}</span>
        <span class="val p">{pShown}%</span>
      </div>
      <div class="bar thick"><div class="fill p" style="width:{pShown}%"></div></div>
    </div>
    <div class="metric">
      <div class="head">
        <span class="label sub">{secondaryLabel}</span>
        <span class="val s">{sShown}%</span>
      </div>
      <div class="bar thin"><div class="fill s" style="width:{sShown}%"></div></div>
    </div>
    <InfoGrid {primaryLabel} {secondaryLabel} {pShown} {sShown} {primaryResetMs} {secondaryResetMs} />
  </div>
{:else}
  <div class="fs compact">
    <div class="crow">
      <span class="label">{primaryLabel}</span>
      <span class="val p">{pShown}%</span>
    </div>
    <div class="bar thick"><div class="fill p" style="width:{pShown}%"></div></div>
    <div class="bar thin"><div class="fill s" style="width:{sShown}%"></div></div>
    <div class="crow small">
      <span class="val s">{sShown}%</span>
      <span class="reset">{formatClock(primaryResetMs)}</span>
    </div>
  </div>
{/if}

<style>
  .fs {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .fs.compact {
    gap: 4px;
  }
  .metric {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .head,
  .crow {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    white-space: nowrap;
  }
  .crow.small {
    gap: 10px;
  }
  .label {
    font-size: 0.75rem;
    color: rgb(var(--fg));
    white-space: nowrap;
  }
  .label.sub {
    font-size: 0.7rem;
    color: rgb(var(--fg-muted));
  }
  .val {
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .val.p {
    color: rgb(var(--m1));
  }
  .val.s {
    color: rgb(var(--m2));
    font-size: 0.82rem;
  }
  .crow.small .val.s {
    font-size: 0.72rem;
  }
  .bar {
    border-radius: 5px;
    background: rgb(var(--track));
    overflow: hidden;
  }
  .bar.thick {
    height: 9px;
  }
  .bar.thin {
    height: 4px;
  }
  .fill {
    height: 100%;
    border-radius: 5px;
    transition: width 0.4s cubic-bezier(0.1, 0.8, 0.3, 1);
  }
  .fill.p {
    background: linear-gradient(90deg, rgb(var(--m1) / 0.85), rgb(var(--m1)));
    box-shadow: 0 0 8px rgb(var(--m1) / 0.4);
  }
  .fill.s {
    background: rgb(var(--m2));
  }
  .reset {
    font-size: 0.64rem;
    color: rgb(var(--fg-muted));
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
