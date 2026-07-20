<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import {
    getUsage,
    getSettings,
    openSettingsWindow,
    openStatsWindow,
    openStyleWindow,
    setAlwaysOnTop,
    setMoveLock,
    getUpdateState,
    installUpdate,
    widgetConfig,
    type UsageSnapshot,
    type Settings,
    type UpdateInfo,
  } from "../lib/ipc";
  import WidgetStyle from "../lib/widgetStyles/WidgetStyle.svelte";
  import { DEFAULT_STYLE, colorsFor } from "../lib/widgetStyles/types";

  // The window is sized to the panel content; this caps how wide a long label can push it.
  const MAX_W = 360;
  // Below this content width, the tool icons are collapsed into a kebab dropdown so they
  // don't force the widget wider than its content.
  const ICON_ROW_MIN = 208;

  const SERVICE_NAMES: Record<string, string> = {
    claude: "Claude",
    antigravity: "Antigravity",
  };

  // Which service this widget window monitors, derived from its window label
  // ("widget" == claude, "widget-{service}" otherwise).
  function serviceFromLabel(label: string): string {
    if (label === "widget") return "claude";
    if (label.startsWith("widget-")) return label.slice("widget-".length);
    return "claude";
  }
  const myService = (() => {
    try {
      return serviceFromLabel(getCurrentWindow().label);
    } catch {
      return "claude";
    }
  })();
  const serviceName = SERVICE_NAMES[myService] ?? myService;

  const KEBAB = `<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><circle cx="8" cy="3.4" r="1.35"/><circle cx="8" cy="8" r="1.35"/><circle cx="8" cy="12.6" r="1.35"/></svg>`;
  const PIN = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 2.5h6M9.2 2.5v4.7l2.3 2.8H4.5L6.8 7.2V2.5M8 10v3.5"/></svg>`;
  const LOCK = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.3" rx="1.2"/><path d="M5.6 7V5.2a2.4 2.4 0 0 1 4.8 0V7"/></svg>`;
  const UNLOCK = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.3" rx="1.2"/><path d="M5.6 7V5.2a2.4 2.4 0 0 1 4.6-0.9"/></svg>`;
  const GEAR = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="2.1"/><path d="M8 1.7v1.7M8 12.6v1.7M14.3 8h-1.7M3.4 8H1.7M12.45 3.55l-1.2 1.2M4.75 11.25l-1.2 1.2M12.45 12.45l-1.2-1.2M4.75 4.75l-1.2-1.2"/></svg>`;
  const STATS = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.2 13.8V2.4M2.2 13.8h11.6M5 11.4V8.4M8 11.4V5.2M11 11.4V6.8"/></svg>`;
  const UPDATE = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2.4v7.4M5.2 7l2.8 2.9L10.8 7M3.4 13.2h9.2"/></svg>`;
  const PALETTE = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M8 1.8a6.2 6.2 0 1 0 0 12.4c1 0 1.5-.8 1.5-1.5 0-.9-.8-1.3-.8-2 0-.6.5-1 1.1-1h1.3A2.8 2.8 0 0 0 14.2 7 6.3 6.3 0 0 0 8 1.8Z"/><circle cx="5.4" cy="6.2" r=".9" fill="currentColor" stroke="none"/><circle cx="8" cy="4.9" r=".9" fill="currentColor" stroke="none"/><circle cx="10.6" cy="6.2" r=".9" fill="currentColor" stroke="none"/></svg>`;

  let snap = $state<UsageSnapshot | null>(null);
  let alwaysOnTop = $state(true);
  let moveLocked = $state(false);
  let displayMode = $state<"remaining" | "used">("remaining");
  let style = $state<string>(DEFAULT_STYLE);
  let now = $state(Date.now());
  let updateInfo = $state<UpdateInfo | null>(null);
  let updating = $state(false);
  let menuOpen = $state(false);
  let bodyWidth = $state(0);
  // Show the icon row inline when the content is wide enough; otherwise collapse to a menu.
  const collapsed = $derived(bodyWidth > 0 && bodyWidth < ICON_ROW_MIN);

  let panelEl: HTMLElement | undefined;
  let bodyEl = $state<HTMLElement | undefined>(undefined);
  let ro: ResizeObserver | undefined;
  let timer: number | undefined;
  let unlisteners: Array<() => void> = [];

  function applySettings(s: Settings) {
    const wc = widgetConfig(s, myService);
    alwaysOnTop = wc.always_on_top;
    moveLocked = wc.move_lock;
    displayMode = wc.display_mode;
    style = wc.style || DEFAULT_STYLE;
    applyTheme(s.theme as Theme);
    document.documentElement.style.setProperty("--panel-alpha", String(wc.opacity));
  }

  // Size the window to hug the panel content (width + height), so a compact style makes a
  // small widget and text never has to wrap.
  async function fitWindow() {
    if (!panelEl) return;
    if (bodyEl) bodyWidth = Math.ceil(bodyEl.getBoundingClientRect().width);
    const r = panelEl.getBoundingClientRect();
    const w = Math.min(MAX_W, Math.ceil(r.width));
    const h = Math.ceil(r.height);
    if (w < 40 || h < 30) return;
    try {
      const { LogicalSize } = await import("@tauri-apps/api/dpi");
      await getCurrentWindow().setSize(new LogicalSize(w, h));
    } catch {
      /* not in Tauri */
    }
  }

  // The kebab menu only exists in the collapsed header; close it when expanding.
  $effect(() => {
    if (!collapsed) menuOpen = false;
  });

  // Re-fit when the style, header layout, or the (in-flow) menu changes the content size.
  $effect(() => {
    style;
    menuOpen;
    collapsed;
    void fitWindow();
  });

  onMount(async () => {
    await initWindow();
    // Colour the widget's metrics to match the service's brand.
    const c = colorsFor(myService);
    document.documentElement.style.setProperty("--m1", c.m1);
    document.documentElement.style.setProperty("--m2", c.m2);
    try {
      snap = await getUsage(myService);
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
        await listen<UsageSnapshot>("usage://updated", (e) => {
          if (e.payload.service_id === myService) snap = e.payload;
        }),
      );
      unlisteners.push(
        await listen<Settings>("settings://changed", (e) => applySettings(e.payload)),
      );
      unlisteners.push(
        await listen<UpdateInfo>("update://available", (e) => (updateInfo = e.payload)),
      );
      unlisteners.push(await getCurrentWindow().onScaleChanged(() => void fitWindow()));
      unlisteners.push(
        await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (!focused) menuOpen = false;
        }),
      );
    } catch {
      /* preview */
    }
    if (panelEl && "ResizeObserver" in window) {
      ro = new ResizeObserver(() => void fitWindow());
      ro.observe(panelEl);
    }
    void fitWindow();
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
      await setAlwaysOnTop(myService, alwaysOnTop);
    } catch {
      /* preview */
    }
  }

  async function toggleLock() {
    moveLocked = !moveLocked;
    try {
      await setMoveLock(myService, moveLocked);
    } catch {
      /* preview */
    }
  }

  function startDrag(e: MouseEvent) {
    const target = e.target as HTMLElement;
    // A click on the panel background (not a control) closes an open menu and starts a drag.
    if (menuOpen && !target.closest(".menu") && !target.closest(".kebab")) menuOpen = false;
    if (moveLocked || e.button !== 0) return;
    if (target.closest("button")) return;
    getCurrentWindow()
      .startDragging()
      .catch(() => {});
  }

  function toggleMenu(e: MouseEvent) {
    e.stopPropagation();
    menuOpen = !menuOpen;
  }

  // Navigation items close the menu; toggle items keep it open so several can be flipped.
  function nav(fn: () => void) {
    menuOpen = false;
    fn();
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

  async function openStyle() {
    try {
      await openStyleWindow();
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
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="panel" bind:this={panelEl} onmousedown={startDrag}>
  <header>
    <span class="title">{serviceName}</span>
    {#if collapsed}
      <button
        class="kebab"
        class:on={menuOpen}
        title={$t("common.settings")}
        aria-label={$t("common.settings")}
        onclick={toggleMenu}>{@html KEBAB}</button>
    {:else}
      <div class="tools">
        {#if updateInfo?.available}
          <button
            class="tool update"
            disabled={updating}
            title={$t("widget.update", { version: updateInfo.version })}
            aria-label={$t("widget.update", { version: updateInfo.version })}
            onclick={doUpdate}>{@html UPDATE}</button>
        {/if}
        <button class="tool" title={$t("common.stats")} aria-label={$t("common.stats")} onclick={openStats}>{@html STATS}</button>
        <button class="tool" title={$t("widgetStyle.title")} aria-label={$t("widgetStyle.title")} onclick={openStyle}>{@html PALETTE}</button>
        <button class="tool" title={$t("common.settings")} aria-label={$t("common.settings")} onclick={openSettings}>{@html GEAR}</button>
        <button class="tool" class:active={alwaysOnTop} title={$t("widget.alwaysOnTop")} aria-label={$t("widget.alwaysOnTop")} onclick={toggleAoT}>{@html PIN}</button>
        <button class="tool" class:active={moveLocked} title={$t("widget.moveLock")} aria-label={$t("widget.moveLock")} onclick={toggleLock}>{@html moveLocked ? LOCK : UNLOCK}</button>
      </div>
    {/if}
  </header>

  {#if collapsed && menuOpen}
    <div class="menu">
      {#if updateInfo?.available}
        <button class="mitem accent" disabled={updating} onclick={() => nav(doUpdate)}>
          {@html UPDATE}<span>{$t("widget.update", { version: updateInfo.version })}</span>
        </button>
      {/if}
      <button class="mitem" class:on={alwaysOnTop} onclick={toggleAoT}>
        {@html PIN}<span>{$t("widget.alwaysOnTop")}</span>
      </button>
      <button class="mitem" class:on={moveLocked} onclick={toggleLock}>
        {@html moveLocked ? LOCK : UNLOCK}<span>{$t("widget.moveLock")}</span>
      </button>
      <button class="mitem" onclick={() => nav(openStyle)}>
        {@html PALETTE}<span>{$t("widgetStyle.title")}</span>
      </button>
      <button class="mitem" onclick={() => nav(openStats)}>
        {@html STATS}<span>{$t("common.stats")}</span>
      </button>
      <button class="mitem" onclick={() => nav(openSettings)}>
        {@html GEAR}<span>{$t("common.settings")}</span>
      </button>
    </div>
  {/if}

  {#if snap === null}
    <div class="empty">{$t("common.loading")}</div>
  {:else if snap.status !== "ok"}
    <div class="empty">
      {snap.status === "unauthorized" ? $t("common.sessionExpired") : $t("common.notLoggedIn")}
    </div>
  {:else}
    <div class="body" bind:this={bodyEl}>
      <WidgetStyle styleId={style} snapshot={snap} {now} {displayMode} />
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    /* Hug the content so the window can shrink to it (set by fitWindow). */
    width: max-content;
    max-width: 360px;
    padding: 8px 10px 9px;
    background: rgb(var(--panel));
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
    gap: 12px;
    margin-bottom: 7px;
    white-space: nowrap;
  }
  .title {
    font-size: 0.74rem;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: rgb(var(--fg-muted));
    white-space: nowrap;
  }
  .kebab {
    display: grid;
    place-items: center;
    width: 22px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: rgb(var(--fg-muted));
    cursor: default;
    flex-shrink: 0;
  }
  .kebab:hover,
  .kebab.on {
    background: rgb(var(--accent) / 0.14);
    color: rgb(var(--fg));
  }
  .tools {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 22px;
    height: 20px;
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
  .tool.update:disabled {
    opacity: 0.6;
  }
  .menu {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin-bottom: 7px;
    padding: 3px;
    border: 1px solid rgb(var(--border));
    border-radius: 8px;
    background: rgb(var(--panel));
  }
  .mitem {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: rgb(var(--fg));
    font-size: 0.76rem;
    text-align: left;
    cursor: default;
    white-space: nowrap;
  }
  .mitem:hover {
    background: rgb(var(--accent) / 0.14);
  }
  .mitem.on {
    color: rgb(var(--accent));
  }
  .mitem.accent {
    color: rgb(var(--accent));
    font-weight: 600;
  }
  .mitem:disabled {
    opacity: 0.6;
  }
  .body {
    display: flex;
    flex-direction: column;
  }
  .empty {
    text-align: center;
    font-size: 0.74rem;
    color: rgb(var(--fg-muted));
    padding: 12px 8px;
    white-space: nowrap;
  }
</style>
