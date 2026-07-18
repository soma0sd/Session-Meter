<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { t, locale } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import {
    getUsage,
    getSettings,
    setAlwaysOnTop,
    setMoveLock,
    openSettingsWindow,
    openStatsWindow,
    getUpdateState,
    installUpdate,
    type UsageSnapshot,
    type Settings,
    type UpdateInfo,
    type Bucket,
  } from "../lib/ipc";
  import { formatCountdown, formatResetDateTime } from "../lib/countdown";

  const WIDGET_W = 252;
  // Compact packs two fixed-size donuts, so it uses a narrower window - this halves the
  // side margins without changing the donut size or the gap between them.
  const WIDGET_W_COMPACT = 220;

  const PIN = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 2.5h6M9.2 2.5v4.7l2.3 2.8H4.5L6.8 7.2V2.5M8 10v3.5"/></svg>`;
  const LOCK = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.3" rx="1.2"/><path d="M5.6 7V5.2a2.4 2.4 0 0 1 4.8 0V7"/></svg>`;
  const UNLOCK = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.3" rx="1.2"/><path d="M5.6 7V5.2a2.4 2.4 0 0 1 4.6-0.9"/></svg>`;
  const GEAR = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="2.1"/><path d="M8 1.7v1.7M8 12.6v1.7M14.3 8h-1.7M3.4 8H1.7M12.45 3.55l-1.2 1.2M4.75 11.25l-1.2 1.2M12.45 12.45l-1.2-1.2M4.75 4.75l-1.2-1.2"/></svg>`;
  const STATS = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.2 13.8V2.4M2.2 13.8h11.6M5 11.4V8.4M8 11.4V5.2M11 11.4V6.8"/></svg>`;
  const UPDATE = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2.4v7.4M5.2 7l2.8 2.9L10.8 7M3.4 13.2h9.2"/></svg>`;

  let snap = $state<UsageSnapshot | null>(null);
  let alwaysOnTop = $state(true);
  let moveLocked = $state(false);
  let displayMode = $state<"remaining" | "used">("remaining");
  let layout = $state<"detailed" | "compact">("detailed");
  let now = $state(Date.now());
  let updateInfo = $state<UpdateInfo | null>(null);
  let updating = $state(false);

  let panelEl: HTMLElement | undefined;
  let ro: ResizeObserver | undefined;
  let timer: number | undefined;
  let unlisteners: Array<() => void> = [];

  function applySettings(s: Settings) {
    alwaysOnTop = s.always_on_top;
    moveLocked = s.move_lock;
    displayMode = s.tray_display;
    layout = (s.widget_layout as "detailed" | "compact") ?? "detailed";
    applyTheme(s.theme as Theme);
    document.documentElement.style.setProperty("--panel-alpha", String(s.widget_opacity));
  }

  // Resize the window to hug the panel content so there is no empty space below it.
  async function fitWindowHeight() {
    if (!panelEl) return;
    const h = Math.ceil(panelEl.getBoundingClientRect().height);
    if (h < 40) return;
    const w = layout === "compact" ? WIDGET_W_COMPACT : WIDGET_W;
    try {
      const { LogicalSize } = await import("@tauri-apps/api/dpi");
      await getCurrentWindow().setSize(new LogicalSize(w, h));
    } catch {
      /* not in Tauri */
    }
  }

  // Re-fit the window (including its width, for compact) whenever the layout changes.
  $effect(() => {
    layout;
    void fitWindowHeight();
  });

  onMount(async () => {
    await initWindow();
    try {
      snap = await getUsage();
    } catch {
      /* preview */
    }
    try {
      applySettings(await getSettings());
    } catch {
      /* preview */
    }
    try {
      updateInfo = await getUpdateState();
    } catch {
      /* preview */
    }
    try {
      unlisteners.push(
        await listen<UsageSnapshot>("usage://updated", (e) => (snap = e.payload)),
      );
      unlisteners.push(
        await listen<Settings>("settings://changed", (e) => applySettings(e.payload)),
      );
      unlisteners.push(
        await listen<UpdateInfo>("update://available", (e) => (updateInfo = e.payload)),
      );
      // The window is sized in logical px (DPI-independent), so it keeps the same apparent
      // size across HD..4K. Re-fit when the scale factor changes (e.g. dragged to another
      // monitor) so the logical size is re-asserted for the new DPI.
      unlisteners.push(await getCurrentWindow().onScaleChanged(() => void fitWindowHeight()));
    } catch {
      /* preview */
    }
    if (panelEl && "ResizeObserver" in window) {
      ro = new ResizeObserver(() => void fitWindowHeight());
      ro.observe(panelEl);
    }
    void fitWindowHeight();
    timer = window.setInterval(() => (now = Date.now()), 1000);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
    ro?.disconnect();
    unlisteners.forEach((u) => u());
  });

  async function toggleAoT() {
    alwaysOnTop = !alwaysOnTop;
    try {
      await setAlwaysOnTop(alwaysOnTop);
    } catch {
      /* preview */
    }
  }

  async function toggleLock() {
    moveLocked = !moveLocked;
    try {
      await setMoveLock(moveLocked);
    } catch {
      /* preview */
    }
  }

  function startDrag(e: MouseEvent) {
    if (moveLocked || e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    getCurrentWindow()
      .startDragging()
      .catch(() => {});
  }

  async function openSettings() {
    try {
      await openSettingsWindow();
    } catch {
      /* preview */
    }
  }

  async function openStats() {
    try {
      await openStatsWindow();
    } catch {
      /* preview */
    }
  }

  async function doUpdate() {
    updating = true;
    try {
      await installUpdate();
    } finally {
      updating = false;
    }
  }

  const buckets = $derived(snap?.buckets ?? []);
  const localeTag = $derived($locale === "ko" ? "ko-KR" : "en-US");

  function fmtResetAt(iso: string): string {
    return formatResetDateTime(iso, localeTag, $t("common.today"), $t("common.tomorrow"));
  }

  function barColor(remaining: number): string {
    return remaining < 20
      ? "rgb(var(--danger))"
      : remaining < 50
        ? "rgb(var(--warn))"
        : "rgb(var(--ok))";
  }

  // --- compact (donut) layout ---
  const DONUT_R = 40;
  const DONUT_CIRC = 2 * Math.PI * DONUT_R;
  function donutDash(shown: number): string {
    const on = (Math.max(0, Math.min(100, shown)) / 100) * DONUT_CIRC;
    return `${on} ${DONUT_CIRC - on}`;
  }
  // Compact mode shows the current session + weekly on one row; fall back to the first two
  // buckets if those exact keys are absent.
  const compactBuckets = $derived.by(() => {
    const pick = [
      buckets.find((b) => b.key === "five_hour"),
      buckets.find((b) => b.key === "seven_day"),
    ].filter(Boolean) as Bucket[];
    return pick.length ? pick : buckets.slice(0, 2);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="panel" class:compact={layout === "compact"} bind:this={panelEl} onmousedown={startDrag}>
  <header>
    {#if layout !== "compact"}
      <span class="title">{$t("app.name")}</span>
    {/if}
    <div class="tools">
      {#if updateInfo?.available}
        <button
          class="tool update"
          title={$t("widget.update", { version: updateInfo.version })}
          aria-label={$t("widget.update", { version: updateInfo.version })}
          disabled={updating}
          onclick={doUpdate}>{@html UPDATE}</button>
      {/if}
      <button
        class="tool"
        title={$t("common.stats")}
        aria-label={$t("common.stats")}
        onclick={openStats}>{@html STATS}</button>
      <button
        class="tool"
        title={$t("common.settings")}
        aria-label={$t("common.settings")}
        onclick={openSettings}>{@html GEAR}</button>
      <button
        class="tool"
        class:active={alwaysOnTop}
        title={$t("widget.alwaysOnTop")}
        aria-label={$t("widget.alwaysOnTop")}
        onclick={toggleAoT}>{@html PIN}</button>
      <button
        class="tool"
        class:active={moveLocked}
        title={$t("widget.moveLock")}
        aria-label={$t("widget.moveLock")}
        onclick={toggleLock}>{@html moveLocked ? LOCK : UNLOCK}</button>
    </div>
  </header>

  {#if snap === null}
    <div class="empty">{$t("common.loading")}</div>
  {:else if snap.status !== "ok"}
    <div class="empty">
      {snap.status === "unauthorized" ? $t("common.sessionExpired") : $t("common.notLoggedIn")}
    </div>
  {:else if layout === "compact"}
    <div class="donuts">
      {#each compactBuckets as b (b.key)}
        {@const shown = displayMode === "remaining" ? b.remaining : b.utilization}
        {@const remainMs = Math.max(0, Date.parse(b.resets_at) - now)}
        <div class="donut">
          <svg class="ring" viewBox="0 0 100 100">
            <circle class="track" cx="50" cy="50" r={DONUT_R} />
            {#if shown > 0}
              <circle
                class="value"
                cx="50"
                cy="50"
                r={DONUT_R}
                stroke={barColor(b.remaining)}
                stroke-dasharray={donutDash(shown)}
                transform="rotate(-90 50 50)" />
            {/if}
            <text class="num" x="50" y="45" text-anchor="middle" dominant-baseline="central"
              >{shown}%</text>
            {#if b.resets_at}
              <text class="sub" x="50" y="70" text-anchor="middle" dominant-baseline="central"
                >{formatCountdown(remainMs, $locale)}</text>
            {/if}
          </svg>
        </div>
      {/each}
    </div>
  {:else}
    <div class="rows">
      {#each buckets as b (b.key)}
        {@const localized = $t("bucket." + b.key)}
        {@const label = localized.startsWith("bucket.") ? b.label : localized}
        {@const shown = displayMode === "remaining" ? b.remaining : b.utilization}
        {@const remainMs = Math.max(0, Date.parse(b.resets_at) - now)}
        <div class="row">
          <div class="rowtop">
            <span class="label">{label}</span>
            <span class="pct">{shown}%</span>
          </div>
          <div class="bar">
            <div class="fill" style="width:{shown}%; background:{barColor(b.remaining)}"></div>
          </div>
          {#if b.resets_at}
            <div class="reset">{fmtResetAt(b.resets_at)} · {formatCountdown(remainMs, $locale)}</div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    padding: 9px 11px 10px;
    background: rgb(var(--panel));
    /* Whole-widget translucency (over a transparent window) driven by the opacity setting. */
    opacity: var(--panel-alpha);
    color: rgb(var(--fg));
    border: 1px solid rgb(var(--border));
    border-radius: 12px;
    user-select: none;
    cursor: default;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 7px;
  }
  .title {
    font-size: 0.74rem;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: rgb(var(--fg-muted));
  }
  .tools {
    display: flex;
    gap: 2px;
    /* Keep the tools right-aligned even when the title is hidden (compact mode). */
    margin-left: auto;
  }
  /* Compact mode: trim the outer padding so the side margins match the inter-donut gap. */
  .panel.compact {
    padding: 7px 6px 8px;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 24px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: rgb(var(--fg-muted));
    cursor: default;
  }
  .tool:hover {
    background: rgb(var(--accent) / 0.14);
    color: rgb(var(--fg));
  }
  .tool.active {
    color: rgb(var(--accent));
  }
  .tool.update {
    color: rgb(var(--accent));
  }
  .tool.update:hover {
    background: rgb(var(--accent) / 0.18);
    color: rgb(var(--accent));
  }
  .tool.update:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 9px;
    max-height: 320px;
    overflow-y: auto;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .rowtop {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .label {
    font-size: 0.76rem;
    color: rgb(var(--fg));
  }
  .pct {
    font-size: 0.82rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: rgb(var(--track));
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 3px;
    transition: width 0.4s ease;
  }
  .reset {
    font-size: 0.66rem;
    color: rgb(var(--fg-muted));
    font-variant-numeric: tabular-nums;
  }
  .donuts {
    display: flex;
    justify-content: center;
    gap: 18px;
  }
  .donut {
    display: flex;
  }
  .ring {
    display: block;
    width: 84px;
    height: 84px;
  }
  .ring .track {
    fill: none;
    stroke: rgb(var(--track));
    stroke-width: 11;
  }
  .ring .value {
    fill: none;
    stroke-width: 11;
    stroke-linecap: round;
    transition: stroke-dasharray 0.4s ease;
  }
  .ring .num {
    fill: rgb(var(--fg));
    font-size: 24px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .ring .sub {
    fill: rgb(var(--fg));
    font-size: 17px;
    font-variant-numeric: tabular-nums;
  }
  /* Halo in the panel colour so the numbers stay readable where they cross the ring. */
  .ring .num,
  .ring .sub {
    paint-order: stroke;
    stroke: rgb(var(--panel));
    stroke-width: 3.5px;
    stroke-linejoin: round;
  }
  .empty {
    text-align: center;
    font-size: 0.74rem;
    color: rgb(var(--fg-muted));
    padding: 12px 8px;
  }
</style>
