<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import {
    getUsage,
    getSettings,
    getDockMembers,
    openSettingsWindow,
    openStatsWindow,
    openStyleWindow,
    setAlwaysOnTop,
    setMoveLock,
    setDockAlwaysOnTop,
    setDockMoveLock,
    setWidgetHeadlineGroup,
    getUpdateState,
    installUpdate,
    widgetConfig,
    defaultWidgetConfig,
    type UsageSnapshot,
    type Settings,
    type UpdateInfo,
    type WidgetConfig,
  } from "../lib/ipc";
  import ServiceCell from "./ServiceCell.svelte";
  import { colorsFor, serviceIcons } from "../lib/widgetStyles/types";

  // The window is sized to the panel content; this caps how wide a long label can push one
  // service's cell. The docked window allows this much per column.
  const MAX_CELL_W = 360;
  // Below this content width, the tool icons are collapsed into a kebab dropdown so they
  // don't force the widget wider than its content.
  const ICON_ROW_MIN = 208;
  // Gap between the panel and the kebab popover, which is anchored fully outside the panel so
  // opening it never covers the usage readout the widget exists to show.
  const MENU_GAP = 4;
  // Label of the docked group window (`windows::DOCK_LABEL` on the Rust side).
  const DOCK_LABEL = "widget-dock";

  const SERVICE_NAMES: Record<string, string> = {
    claude: "Claude",
    codex: "Codex",
    gemini: "Gemini",
    antigravity_ide: "Antigravity",
  };

  // Which service this widget window monitors, derived from its window label
  // ("widget" == claude, "widget-{service}" otherwise).
  function serviceFromLabel(label: string): string {
    if (label === "widget") return "claude";
    if (label.startsWith("widget-")) return label.slice("widget-".length);
    return "claude";
  }

  // Dev/preview overrides (widget.html?service=antigravity_ide, widget.html?mode=dock), mirroring
  // appinit's `?lang=`. Outside Tauri `getCurrentWindow()` throws and every widget would
  // otherwise fall back to Claude, so neither the other services' widgets nor the docked window
  // could be previewed in the browser at all. A deployed window never carries a query string, so
  // this cannot fire in the real app.
  const params = new URLSearchParams(location.search);
  const windowLabel = (() => {
    try {
      return getCurrentWindow().label;
    } catch {
      return "";
    }
  })();
  // Group mode: this is the docked window, which hosts every docked service as a cell of one
  // grid (see dock.rs). Otherwise this window shows exactly one service.
  const isGroup = params.get("mode") === "dock" || windowLabel === DOCK_LABEL;
  const myService = isGroup ? "" : (params.get("service") ?? serviceFromLabel(windowLabel));
  const serviceName = SERVICE_NAMES[myService] ?? myService;
  const serviceIcon = isGroup ? "" : (serviceIcons[myService] ?? "");
  const serviceColors = isGroup ? null : colorsFor(myService);

  const KEBAB = `<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><circle cx="8" cy="3.4" r="1.35"/><circle cx="8" cy="8" r="1.35"/><circle cx="8" cy="12.6" r="1.35"/></svg>`;
  const PIN = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 2.5h6M9.2 2.5v4.7l2.3 2.8H4.5L6.8 7.2V2.5M8 10v3.5"/></svg>`;
  const LOCK = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.3" rx="1.2"/><path d="M5.6 7V5.2a2.4 2.4 0 0 1 4.8 0V7"/></svg>`;
  const UNLOCK = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.3" rx="1.2"/><path d="M5.6 7V5.2a2.4 2.4 0 0 1 4.6-0.9"/></svg>`;
  const GEAR = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="2.1"/><path d="M8 1.7v1.7M8 12.6v1.7M14.3 8h-1.7M3.4 8H1.7M12.45 3.55l-1.2 1.2M4.75 11.25l-1.2 1.2M12.45 12.45l-1.2-1.2M4.75 4.75l-1.2-1.2"/></svg>`;
  const STATS = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.2 13.8V2.4M2.2 13.8h11.6M5 11.4V8.4M8 11.4V5.2M11 11.4V6.8"/></svg>`;
  const UPDATE = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2.4v7.4M5.2 7l2.8 2.9L10.8 7M3.4 13.2h9.2"/></svg>`;
  const PALETTE = `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M8 1.8a6.2 6.2 0 1 0 0 12.4c1 0 1.5-.8 1.5-1.5 0-.9-.8-1.3-.8-2 0-.6.5-1 1.1-1h1.3A2.8 2.8 0 0 0 14.2 7 6.3 6.3 0 0 0 8 1.8Z"/><circle cx="5.4" cy="6.2" r=".9" fill="currentColor" stroke="none"/><circle cx="8" cy="4.9" r=".9" fill="currentColor" stroke="none"/><circle cx="10.6" cy="6.2" r=".9" fill="currentColor" stroke="none"/></svg>`;

  let settings = $state<Settings | null>(null);
  // Latest snapshot per service. A single-service window only ever fills its own entry; the
  // docked window keeps one per member.
  let snaps = $state<Record<string, UsageSnapshot | null>>({});
  // Docked window only: the services it shows, in placement order (Rust decides - see
  // `dock::sync` - so the window and the OS-level show/hide always agree).
  let members = $state<string[]>([]);
  const cells = $derived(isGroup ? members : [myService]);
  const columns = $derived(
    isGroup ? Math.max(1, Math.min(settings?.dock.columns ?? 1, Math.max(1, cells.length))) : 1,
  );
  const maxW = $derived(MAX_CELL_W * columns);
  // The window-level toggles come from the docked group's own config in group mode, and from
  // the service's widget config otherwise.
  const alwaysOnTop = $derived(
    settings === null
      ? true
      : isGroup
        ? settings.dock.always_on_top
        : widgetConfig(settings, myService).always_on_top,
  );
  const moveLocked = $derived(
    settings === null
      ? false
      : isGroup
        ? settings.dock.move_lock
        : widgetConfig(settings, myService).move_lock,
  );
  const opacity = $derived(
    settings === null
      ? 0.9
      : isGroup
        ? settings.dock.opacity
        : widgetConfig(settings, myService).opacity,
  );
  let now = $state(Date.now());
  let updateInfo = $state<UpdateInfo | null>(null);
  let updating = $state(false);
  let menuOpen = $state(false);
  let menuOpensUp = $state(false);
  let menuTopInset = $state(0);
  let contentWidth = $state(0);
  // Show the icon row inline when the content is wide enough; otherwise collapse to a menu.
  // Deliberately a function of the *content* width alone: it must not see the panel width, or
  // the header's own width would feed back into the judgement (see `.content` in the styles).
  const collapsed = $derived(contentWidth > 0 && contentWidth < ICON_ROW_MIN);

  let panelEl: HTMLElement | undefined;
  let menuEl = $state<HTMLElement | undefined>(undefined);
  let contentEl: HTMLElement | undefined;
  let ro: ResizeObserver | undefined;
  let contentRo: ResizeObserver | undefined;
  let timer: number | undefined;
  let unlisteners: Array<() => void> = [];
  let nativeTopInsetPx = 0;
  let fitRevision = 0;

  function cellConfig(service: string): WidgetConfig {
    return settings ? widgetConfig(settings, service) : defaultWidgetConfig();
  }

  function applySettings(s: Settings) {
    settings = s;
    applyTheme(s.theme as Theme);
  }

  $effect(() => {
    document.documentElement.style.setProperty("--panel-alpha", String(opacity));
  });

  // The static "widget" (Claude) window is created by Tauri's builder before `setup()` runs,
  // so its webview can start executing JS before Rust has finished loading settings and
  // `manage()`-ing AppState - an `invoke()` that early fails outright rather than returning
  // stale data. Silently swallowing that failure (the old `catch {}`) left `style` (and
  // everything else `applySettings` sets) stuck at its compile-time default forever. A few
  // quick retries cover that narrow startup window; when the first call already succeeds (the
  // overwhelmingly common case, and always the case for a dynamically-created window like
  // Gemini/Antigravity or the docked one, which only ever get created after setup() finishes),
  // this resolves on the first attempt with no extra cost. Only retries on a thrown error - a
  // call that succeeds with a legitimate "nothing yet" value (e.g. `getUsage` returning null) is
  // not retried, since that's a normal state, not a failure.
  async function retryInvoke<T>(fn: () => Promise<T>, attempts = 5, delayMs = 150): Promise<T | undefined> {
    for (let i = 0; i < attempts; i++) {
      try {
        return await fn();
      } catch {
        if (i < attempts - 1) await new Promise((r) => setTimeout(r, delayMs));
      }
    }
    return undefined;
  }

  // Pull the cached snapshot of every listed service this window has not seen yet (a member
  // that just joined the docked window, say). Later updates arrive over `usage://updated`.
  async function loadSnaps(list: string[]) {
    for (const svc of list) {
      if (svc in snaps) continue;
      const s = await retryInvoke(() => getUsage(svc));
      if (s !== undefined) snaps = { ...snaps, [svc]: s };
    }
  }

  async function preferUpwardMenu(panelH: number): Promise<boolean> {
    if (!menuEl) return false;
    try {
      const [position, monitor] = await Promise.all([
        getCurrentWindow().outerPosition(),
        currentMonitor(),
      ]);
      if (!monitor) return false;
      const scale = monitor.scaleFactor || window.devicePixelRatio || 1;
      const workArea = monitor.workArea;
      // The popover sits entirely outside the panel (below it, or above it when flipped), so
      // either direction needs the same amount of extra room: its own height plus the gap.
      const extra = Math.max(0, Math.ceil((menuEl.getBoundingClientRect().height + MENU_GAP) * scale));
      const availableBelow = workArea.position.y + workArea.size.height - (position.y + Math.ceil(panelH * scale));
      const availableAbove = position.y - workArea.position.y;
      return extra > availableBelow && availableAbove >= extra;
    } catch {
      return false;
    }
  }

  async function setNativeTopInset(inset: number) {
    const nextInsetPx = Math.round(inset * (window.devicePixelRatio || 1));
    const delta = nextInsetPx - nativeTopInsetPx;
    if (!delta) return;
    const { PhysicalPosition } = await import("@tauri-apps/api/dpi");
    const appWindow = getCurrentWindow();
    const position = await appWindow.outerPosition();
    await appWindow.setPosition(new PhysicalPosition(position.x, position.y - delta));
    nativeTopInsetPx = nextInsetPx;
  }

  // Size the native window to the visual union of the normal panel and absolute popover.
  // An upward menu receives transparent top space and shifts its native window by the same
  // amount, so the panel's desktop coordinates remain fixed while the popover stays visible.
  async function fitWindow() {
    if (!panelEl) return;
    const revision = ++fitRevision;
    if (contentEl) contentWidth = Math.ceil(contentEl.getBoundingClientRect().width);
    let r = panelEl.getBoundingClientRect();
    const panelW = Math.min(maxW, Math.ceil(r.width));
    const panelH = Math.ceil(r.height);
    if (panelW < 40 || panelH < 30) return;

    const shouldOpenUp = menuOpen && (await preferUpwardMenu(panelH));
    if (revision !== fitRevision) return;
    if (menuOpensUp !== shouldOpenUp) {
      menuOpensUp = shouldOpenUp;
      await tick();
      if (revision !== fitRevision || !panelEl) return;
      r = panelEl.getBoundingClientRect();
    }

    const menuRect = menuOpen && menuEl ? menuEl.getBoundingClientRect() : undefined;
    const desiredTopInset = menuRect && menuOpensUp ? Math.max(0, Math.ceil(r.top - menuRect.top)) : 0;
    if (menuTopInset !== desiredTopInset) {
      menuTopInset = desiredTopInset;
      await tick();
      if (revision !== fitRevision || !panelEl) return;
      r = panelEl.getBoundingClientRect();
    }

    const popover = menuOpen && menuEl ? menuEl.getBoundingClientRect() : undefined;
    const visualTop = popover ? Math.min(r.top, popover.top) : r.top;
    const visualBottom = popover ? Math.max(r.bottom, popover.bottom) : r.bottom;
    // The popover's labels are `nowrap` and can be wider than the panel that anchors it, so
    // the window has to widen for it too - otherwise the menu text is cut off at the window
    // edge. The panel itself is `width: max-content`, so the extra width stays transparent.
    const visualRight = popover ? Math.max(r.right, popover.right) : r.right;
    const w = Math.min(maxW, Math.max(panelW, Math.ceil(visualRight - r.left)));
    const h = Math.max(panelH, Math.ceil(visualBottom - visualTop));
    try {
      const { LogicalSize } = await import("@tauri-apps/api/dpi");
      await setNativeTopInset(menuTopInset);
      if (revision !== fitRevision) return;
      await getCurrentWindow().setSize(new LogicalSize(w, h));
    } catch {
      /* not in Tauri */
    }
  }

  // The kebab menu only exists in the collapsed header; close it when expanding.
  $effect(() => {
    if (!collapsed) menuOpen = false;
  });

  // Re-fit when the header layout or popover visibility changes the visual bounds. Content
  // changes (a style switch, a member joining the grid, a label change) resize the panel
  // itself, which the ResizeObservers below catch.
  $effect(() => {
    menuOpen;
    collapsed;
    columns;
    void fitWindow();
  });

  onMount(async () => {
    await initWindow();
    const initialSettings = await retryInvoke(() => getSettings());
    if (initialSettings !== undefined) applySettings(initialSettings);
    if (isGroup) {
      const initialMembers = await retryInvoke(() => getDockMembers());
      if (initialMembers !== undefined) members = initialMembers;
    }
    await loadSnaps(cells);
    try {
      updateInfo = await getUpdateState();
    } catch {
      /* preview */
    }
    try {
      unlisteners.push(
        await listen<UsageSnapshot>("usage://updated", (e) => {
          if (isGroup || e.payload.service_id === myService) {
            snaps = { ...snaps, [e.payload.service_id]: e.payload };
          }
        }),
      );
      unlisteners.push(
        await listen<Settings>("settings://changed", (e) => applySettings(e.payload)),
      );
      unlisteners.push(
        await listen<UpdateInfo>("update://available", (e) => (updateInfo = e.payload)),
      );
      if (isGroup) {
        unlisteners.push(
          await listen<string[]>("dock://members", (e) => {
            members = e.payload;
            void loadSnaps(e.payload);
          }),
        );
      }
      unlisteners.push(await getCurrentWindow().onScaleChanged(() => void fitWindow()));
      unlisteners.push(
        await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (!focused) menuOpen = false;
        }),
      );
    } catch {
      /* preview */
    }
    if ("ResizeObserver" in window) {
      if (panelEl) {
        ro = new ResizeObserver(() => void fitWindow());
        ro.observe(panelEl);
      }
      // The content can change width without the panel doing so (when the header is the
      // wider of the two), and `collapsed` has to follow the content.
      if (contentEl) {
        contentRo = new ResizeObserver(() => void fitWindow());
        contentRo.observe(contentEl);
      }
    }
    void fitWindow();
    timer = window.setInterval(() => (now = Date.now()), 1000);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
    ro?.disconnect();
    contentRo?.disconnect();
    unlisteners.forEach((u) => u());
  });

  // Optimistic toggles: flip the local copy first so the icon responds at once; the command
  // persists and re-broadcasts `settings://changed`, which re-applies the same value.
  async function toggleAoT() {
    if (!settings) return;
    const next = !alwaysOnTop;
    if (isGroup) settings.dock.always_on_top = next;
    else settings.widgets = { ...settings.widgets, [myService]: { ...widgetConfig(settings, myService), always_on_top: next } };
    try {
      if (isGroup) await setDockAlwaysOnTop(next);
      else await setAlwaysOnTop(myService, next);
    } catch {
      /* preview */
    }
  }

  async function toggleLock() {
    if (!settings) return;
    const next = !moveLocked;
    if (isGroup) settings.dock.move_lock = next;
    else settings.widgets = { ...settings.widgets, [myService]: { ...widgetConfig(settings, myService), move_lock: next } };
    try {
      if (isGroup) await setDockMoveLock(next);
      else await setMoveLock(myService, next);
    } catch {
      /* preview */
    }
  }

  // Antigravity only: pick which model group its cell's headline shows.
  async function pickGroup(service: string, group: "gemini" | "3p") {
    if (!settings) return;
    const cur = widgetConfig(settings, service);
    if (cur.headline_group === group) return;
    settings.widgets = { ...settings.widgets, [service]: { ...cur, headline_group: group } };
    try {
      await setWidgetHeadlineGroup(service, group);
    } catch {
      /* preview */
    }
  }

  // The whole window - one service's, or the docked group's - is a single OS window, so a
  // native drag moves everything in it together.
  function startDrag(e: PointerEvent) {
    const target = e.target as HTMLElement;
    // A click on the panel background (not a control) closes an open menu and starts a drag.
    if (menuOpen && !target.closest(".menu") && !target.closest(".kebab")) menuOpen = false;
    if (moveLocked || e.button !== 0) return;
    if (target.closest(".menu")) return;
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
<div class="widget-surface" style:padding-top={menuTopInset ? `${menuTopInset}px` : undefined}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="panel"
    class:group={isGroup}
    bind:this={panelEl}
    style:max-width="{maxW}px"
    style:--m1={serviceColors?.m1}
    style:--m2={serviceColors?.m2}
    onpointerdown={startDrag}>
    <header>
      <span class="title">
        {#if isGroup}
          <span>{$t("app.name")}</span>
        {:else}
          {#if serviceIcon}<span class="svc-icon">{@html serviceIcon}</span>{/if}
          <span>{serviceName}</span>
        {/if}
      </span>
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
      <div class="menu" class:up={menuOpensUp} bind:this={menuEl}>
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

    <!-- One cell per service. The docked window packs them row-major into `columns` columns of
         equal width (the widest cell sets it) and rows of equal height, so the cells line up
         with no per-window size bookkeeping. -->
    <div class="content" class:grid={isGroup} style:--cols={columns} bind:this={contentEl}>
      {#each cells as svc, i (svc)}
        <ServiceCell
          service={svc}
          snap={snaps[svc] ?? null}
          config={cellConfig(svc)}
          {now}
          titled={isGroup}
          sepLeft={isGroup && i % columns !== 0}
          sepTop={isGroup && i >= columns}
          onPickGroup={(g) => void pickGroup(svc, g)} />
      {/each}
      {#if cells.length === 0}
        <div class="empty">{$t("common.loading")}</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .widget-surface {
    display: block;
  }
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    /* Hug the content so the window can shrink to it (set by fitWindow). */
    width: max-content;
    padding: 8px 10px 9px;
    background: rgb(var(--panel));
    opacity: var(--panel-alpha);
    color: rgb(var(--fg));
    border: 1px solid rgb(var(--border));
    border-radius: 12px;
    user-select: none;
    cursor: default;
    overflow: visible;
  }
  /* The docked window: the cells carry their own padding, so the panel only keeps a thin
     frame and the header is inset to line up with the first cell's content. */
  .panel.group {
    padding: 8px 4px 4px;
  }
  .panel.group header {
    padding: 0 6px;
    margin-bottom: 4px;
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
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 0.74rem;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: rgb(var(--fg-muted));
    white-space: nowrap;
  }
  .svc-icon {
    display: inline-flex;
    /* Tinted with the service's brand colour (--m1, set on the panel for a single service). */
    color: rgb(var(--m1));
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
    /* Anchored to `.panel` (which is `position: relative` for exactly this reason). Without
       that, the containing block is the window itself, and the transparent top inset that
       `fitWindow` adds for an upward popover moves the panel while leaving the popover
       behind - it then hangs above y=0 and everything but its last row is clipped away by
       the window edge. */
    position: absolute;
    z-index: 10;
    /* The 4px offsets here and on `.menu.up` mirror the `MENU_GAP` script constant. */
    top: calc(100% + 4px);
    left: 0;
    /* Sized to its own labels, never narrower than the panel; `fitWindow` widens the window
       to match so nothing is cut off. */
    min-width: 100%;
    width: max-content;
    max-width: 340px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 3px;
    border: 1px solid rgb(var(--border));
    border-radius: 8px;
    background: rgb(var(--panel));
    box-shadow: 0 8px 22px rgb(0 0 0 / 0.22);
  }
  .menu.up {
    top: auto;
    bottom: calc(100% + 4px);
    box-shadow: 0 -8px 22px rgb(0 0 0 / 0.22);
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
  .content {
    display: flex;
    flex-direction: column;
    /* Root cause of the icon-collapse bug of old: `.panel` is a column flex container, and
       its children default to `align-items: stretch` - so without this, the content would
       stretch to *whatever width `.panel` currently happens to be* (which, before the icon
       row has collapsed, is the wide uncollapsed-header width) instead of its own narrow
       intrinsic width. `fitWindow()` then measures that already-stretched (wide) box,
       `collapsed` computes false again, the header stays wide, and the loop never escapes.
       `align-self: flex-start` + `width: max-content` make this element size to its own
       content regardless of the panel's current width, so the measurement is always genuine.
       Keep it. */
    align-self: flex-start;
    width: max-content;
    max-width: 100%;
  }
  .content.grid {
    display: grid;
    /* Every column as wide as the widest cell (a grid sized to its content shares the
       max-content width across `1fr` tracks), every row as tall as the tallest one. */
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    grid-auto-rows: 1fr;
  }
  .empty {
    text-align: center;
    font-size: 0.74rem;
    color: rgb(var(--fg-muted));
    padding: 12px 8px;
    white-space: nowrap;
  }
</style>
