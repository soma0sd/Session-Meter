<script lang="ts">
  import { t } from "../lib/i18n";
  import WidgetStyle from "../lib/widgetStyles/WidgetStyle.svelte";
  import { DEFAULT_STYLE, catalogEntry, colorsFor, serviceIcons } from "../lib/widgetStyles/types";
  import type { UsageSnapshot, WidgetConfig } from "../lib/ipc";

  // One service's usage readout: the styled body, the loading / signed-out / not-running
  // placeholder, and (Antigravity only) the model-group switch. A single-service widget window
  // holds exactly one of these; the docked window lays several out in a CSS grid, so the cell
  // must never depend on the window it is in - it only knows its own service.
  const SERVICE_NAMES: Record<string, string> = {
    claude: "Claude",
    codex: "Codex",
    gemini: "Gemini",
    antigravity_ide: "Antigravity",
  };

  let {
    service,
    snap,
    config,
    now,
    titled = false,
    sepLeft = false,
    sepTop = false,
    onPickGroup,
  }: {
    service: string;
    snap: UsageSnapshot | null;
    config: WidgetConfig;
    now: number;
    /** Name the service above the readout. The docked window does (its header names the app);
     *  a single-service window already names the service in its own header. */
    titled?: boolean;
    /** Grid separators, set by the docked window for cells that are not in the first
     *  column / first row. */
    sepLeft?: boolean;
    sepTop?: boolean;
    onPickGroup?: (group: "gemini" | "3p") => void;
  } = $props();

  const colors = $derived(colorsFor(service));
  const name = $derived(SERVICE_NAMES[service] ?? service);
  const icon = $derived(serviceIcons[service] ?? "");
  const isAntigravity = $derived(service === "antigravity_ide");
  const styleId = $derived(config.style || DEFAULT_STYLE);
  // A compact style has no room for a labelled switch row, so the Antigravity model-group
  // switch becomes two icon buttons tucked under the percentage readout (rendered by the style
  // itself through its `extra` slot). Detailed styles keep the labelled row below the body.
  const compact = $derived(catalogEntry(styleId).variant === "compact");
  // Antigravity-only: which model-group bucket pair (gemini | 3p) the headline shows.
  const headlineGroup = $derived(config.headline_group === "3p" ? "3p" : "gemini");
  const primaryKeyOverride = $derived(isAntigravity ? `${headlineGroup}-5h` : null);
  const secondaryKeyOverride = $derived(isAntigravity ? `${headlineGroup}-weekly` : null);
</script>

<!-- Icon form of the model-group switch, for compact styles: the Gemini sparkle and the Claude
     spark stand for the two groups (the same marks the widget titles use), with the full names
     as tooltips. Every label is an icon, so the switch never widens the readout it sits under. -->
{#snippet groupIcons()}
  <div class="iseg" role="group" aria-label={$t("widgetStyle.headlineGroup")}>
    <button
      class="ibtn"
      class:on={headlineGroup === "gemini"}
      aria-pressed={headlineGroup === "gemini"}
      title={$t("widgetStyle.groupGemini")}
      aria-label={$t("widgetStyle.groupGemini")}
      onclick={() => onPickGroup?.("gemini")}>{@html serviceIcons.gemini}</button>
    <button
      class="ibtn"
      class:on={headlineGroup === "3p"}
      aria-pressed={headlineGroup === "3p"}
      title={$t("widgetStyle.groupThirdParty")}
      aria-label={$t("widgetStyle.groupThirdParty")}
      onclick={() => onPickGroup?.("3p")}>{@html serviceIcons.claude}</button>
  </div>
{/snippet}

<!-- The metric colours are scoped to the cell (not the document) so the docked window can
     show every service in its own brand colours side by side. -->
<div class="cell" class:titled class:sepLeft class:sepTop style="--m1: {colors.m1}; --m2: {colors.m2};">
  {#if titled}
    <div class="ctitle">
      {#if icon}<span class="svc-icon">{@html icon}</span>{/if}
      <span>{name}</span>
    </div>
  {/if}
  {#if snap === null}
    <div class="empty">{$t("common.loading")}</div>
  {:else if snap.status !== "ok"}
    <div class="empty">
      {snap.status === "unauthorized"
        ? $t("common.sessionExpired")
        : snap.status === "not_running"
          ? $t("common.antigravityNotRunning")
          : snap.status === "error"
            ? $t("common.usageUnavailable")
            : $t("common.notLoggedIn")}
    </div>
  {:else}
    <div class="body">
      <WidgetStyle
        {styleId}
        snapshot={snap}
        {now}
        displayMode={config.display_mode}
        {primaryKeyOverride}
        {secondaryKeyOverride}
        extra={isAntigravity && compact ? groupIcons : undefined} />
    </div>
    {#if isAntigravity && !compact}
      <!-- Antigravity reports two model groups; this swaps which pair the headline shows,
           without opening the Widget Style window. Detailed styles only - compact ones show
           the icon form under their percentages instead (see `groupIcons` above). -->
      <div class="seg" role="group" aria-label={$t("widgetStyle.headlineGroup")}>
        <button
          class="sbtn"
          class:on={headlineGroup === "gemini"}
          aria-pressed={headlineGroup === "gemini"}
          title={$t("widgetStyle.groupGemini")}
          onclick={() => onPickGroup?.("gemini")}><span>{$t("widgetStyle.groupGemini")}</span></button>
        <button
          class="sbtn"
          class:on={headlineGroup === "3p"}
          aria-pressed={headlineGroup === "3p"}
          title={$t("widgetStyle.groupThirdParty")}
          onclick={() => onPickGroup?.("3p")}><span>{$t("widgetStyle.groupThirdParty")}</span></button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .cell {
    display: flex;
    flex-direction: column;
    /* A grid cell may be narrower than its content once the docked window hits its maximum
       width; clip rather than overflow into the neighbour. */
    min-width: 0;
    overflow: hidden;
  }
  .cell.titled {
    padding: 6px 9px 7px;
  }
  .cell.sepLeft {
    border-left: 1px solid rgb(var(--border));
  }
  .cell.sepTop {
    border-top: 1px solid rgb(var(--border));
  }
  .ctitle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-bottom: 5px;
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: rgb(var(--fg-muted));
    white-space: nowrap;
  }
  .svc-icon {
    display: inline-flex;
    /* Tinted with the service's brand colour (--m1, set on the cell above). */
    color: rgb(var(--m1));
  }
  .body {
    display: flex;
    flex-direction: column;
    /* Absorbs the slack when the docked grid makes this cell taller than its readout needs
       (rows share the height of the tallest cell), centring the readout instead of leaving
       it pinned under the title. */
    flex: 1 0 auto;
    justify-content: center;
  }
  .empty {
    text-align: center;
    font-size: 0.74rem;
    color: rgb(var(--fg-muted));
    padding: 12px 8px;
    white-space: nowrap;
    /* Same reason as `.body`: centre the message in a cell the grid has made taller. */
    flex: 1 0 auto;
    display: grid;
    place-items: center;
  }
  /* Antigravity's model-group switch. It must never widen the widget: every label is
     absolutely positioned, so none of them contributes to the cell's max-content width; all
     this row adds intrinsically is its own padding, border and gap. */
  .seg {
    display: flex;
    gap: 2px;
    margin-top: 7px;
    padding: 2px;
    border: 1px solid rgb(var(--border));
    border-radius: 8px;
    background: rgb(var(--track) / 0.45);
    /* The switch's own width is what decides whether its labels need the tighter style
       below (a narrow style such as the compact hex rings, or a narrow docked column). Inline
       size containment is safe here: the row's width comes from the cell (it stretches), and
       its labels are absolute so they never contributed to the intrinsic width anyway. */
    container-type: inline-size;
  }
  .sbtn {
    position: relative; /* containing block for the absolutely positioned label */
    flex: 1 1 0;
    min-width: 0;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: rgb(var(--fg-muted));
    cursor: default;
  }
  .sbtn > span {
    /* Absolute so the label never sets a floor under the widget's width; it is clipped with an
       ellipsis instead, and the full text stays available as the button's `title`. */
    position: absolute;
    inset: 0;
    padding: 0 4px;
    line-height: 20px;
    font-size: 0.66rem;
    font-weight: 600;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* A narrow switch (the same threshold that collapses a single widget's icon row) gets a
     tighter label so "Claude/GPT" still fits instead of being clipped. */
  @container (max-width: 208px) {
    .sbtn > span {
      font-size: 0.58rem;
      padding: 0 2px;
      letter-spacing: -0.01em;
    }
  }
  /* Narrower still (a compact style inside a three-column docked grid): one more notch down,
     which is what keeps "Claude/GPT" whole at the widths those cells actually get. */
  @container (max-width: 150px) {
    .sbtn > span {
      font-size: 0.54rem;
      padding: 0 1px;
      letter-spacing: -0.02em;
    }
  }
  .sbtn:hover {
    background: rgb(var(--accent) / 0.14);
    color: rgb(var(--fg));
  }
  .sbtn.on {
    /* Matches the Widget Style window's segmented control (`.tbtn.active`). */
    background: rgb(var(--accent));
    color: rgb(var(--on-accent));
  }
  /* Compact form of the switch: two icon buttons, about as wide as the "100%" they sit under. */
  .iseg {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid rgb(var(--border));
    border-radius: 7px;
    background: rgb(var(--track) / 0.45);
  }
  .ibtn {
    display: grid;
    place-items: center;
    width: 20px;
    height: 16px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: rgb(var(--fg-muted));
    cursor: default;
  }
  .ibtn:hover {
    background: rgb(var(--accent) / 0.14);
    color: rgb(var(--fg));
  }
  .ibtn.on {
    background: rgb(var(--accent));
    color: rgb(var(--on-accent));
  }
</style>
