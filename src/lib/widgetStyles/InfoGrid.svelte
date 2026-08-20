<script lang="ts">
  import { locale, t } from "../i18n";
  import { formatCountdown } from "../countdown";

  // The detailed-style info panel: one block per window, collapsing to the primary block when
  // the snapshot has no secondary window.
  let {
    primaryLabel,
    secondaryLabel,
    pShown,
    sShown,
    hasSecondary = true,
    primaryResetMs,
    secondaryResetMs,
  }: {
    primaryLabel: string;
    secondaryLabel: string;
    pShown: number;
    sShown: number;
    hasSecondary?: boolean;
    primaryResetMs: number;
    secondaryResetMs: number;
  } = $props();
</script>

<div class="grid" class:single={!hasSecondary}>
  <div class="block" class:br={hasSecondary}>
    <div class="btitle">{primaryLabel}</div>
    <div class="item">
      <span class="k">{$t("widgetStyle.usage")}</span>
      <span class="v p">{pShown}%</span>
    </div>
    <div class="item">
      <span class="k">{$t("widgetStyle.resetIn")}</span>
      <span class="v">{formatCountdown(primaryResetMs, $locale)}</span>
    </div>
  </div>
  {#if hasSecondary}
    <div class="block">
      <div class="btitle">{secondaryLabel}</div>
      <div class="item">
        <span class="k">{$t("widgetStyle.usage")}</span>
        <span class="v s">{sShown}%</span>
      </div>
      <div class="item">
        <span class="k">{$t("widgetStyle.resetIn")}</span>
        <span class="v">{formatCountdown(secondaryResetMs, $locale)}</span>
      </div>
    </div>
  {/if}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    background: rgb(var(--track) / 0.4);
    border: 1px solid rgb(var(--border));
    border-radius: 10px;
    padding: 8px 10px;
    flex: 1;
  }
  .grid.single {
    grid-template-columns: 1fr;
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .block.br {
    border-right: 1px solid rgb(var(--border));
    padding-right: 8px;
  }
  .btitle {
    font-size: 0.6rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: rgb(var(--fg-muted));
    border-bottom: 1px dashed rgb(var(--border));
    padding-bottom: 2px;
    white-space: nowrap;
  }
  .item {
    display: flex;
    flex-direction: column;
  }
  .k {
    font-size: 0.58rem;
    color: rgb(var(--fg-muted));
    white-space: nowrap;
  }
  .v {
    font-size: 0.82rem;
    font-weight: 700;
    line-height: 1.2;
    color: rgb(var(--fg));
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .v.p {
    color: rgb(var(--m1));
  }
  .v.s {
    color: rgb(var(--m2));
  }
</style>
