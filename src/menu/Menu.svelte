<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import {
    openStatsWindow,
    openSettingsWindow,
    openStyleWindow,
    getUpdateState,
    installUpdate,
    quitApp,
    type UpdateInfo,
  } from "../lib/ipc";

  const MENU_W = 196;

  type Item = { key: string; label: string; action: () => Promise<unknown> };

  // Usage refreshes automatically on the poll interval, so the menu has no manual refresh. The
  // widget show/hide is a toggle switch (below), not a plain row.
  const items: Item[] = [
    { key: "style", label: "menu.style", action: openStyleWindow },
    { key: "stats", label: "menu.stats", action: openStatsWindow },
    { key: "settings", label: "menu.settings", action: openSettingsWindow },
  ];

  let updateInfo = $state<UpdateInfo | null>(null);
  let navEl: HTMLElement | undefined;
  let ro: ResizeObserver | undefined;
  let unlisteners: Array<() => void> = [];

  // Size the frameless window to the menu content so a conditional update row doesn't leave
  // empty space or clip.
  async function fitHeight() {
    if (!navEl) return;
    const h = Math.ceil(navEl.getBoundingClientRect().height) + 6;
    if (h < 30) return;
    try {
      const { LogicalSize } = await import("@tauri-apps/api/dpi");
      await getCurrentWindow().setSize(new LogicalSize(MENU_W, h));
    } catch {
      /* not in Tauri */
    }
  }

  onMount(async () => {
    await initWindow();
    try {
      updateInfo = await getUpdateState();
    } catch {
      /* preview */
    }
    try {
      unlisteners.push(
        await listen<UpdateInfo>("update://available", (e) => (updateInfo = e.payload)),
      );
    } catch {
      /* preview */
    }
    if (navEl && "ResizeObserver" in window) {
      ro = new ResizeObserver(() => void fitHeight());
      ro.observe(navEl);
    }
    void fitHeight();
  });

  onDestroy(() => {
    ro?.disconnect();
    unlisteners.forEach((u) => u());
  });

  async function hide() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* not in Tauri */
    }
  }

  async function run(item: Item) {
    try {
      await item.action();
    } catch (e) {
      console.error(e);
    }
    await hide();
  }

  // Hide first, then install (download + install restarts the app).
  async function doUpdate() {
    await hide();
    try {
      await installUpdate();
    } catch (e) {
      console.error(e);
    }
  }

  async function quit() {
    try {
      await quitApp();
    } catch (e) {
      console.error(e);
    }
  }
</script>

<nav class="menu" bind:this={navEl}>
  {#if updateInfo?.available}
    <button class="item update" type="button" onclick={doUpdate}>
      {$t("menu.update", { version: updateInfo.version })}
    </button>
    <div class="sep"></div>
  {/if}
  {#each items as item (item.key)}
    <button class="item" type="button" onclick={() => run(item)}>
      {$t(item.label)}
    </button>
  {/each}
  <div class="sep"></div>
  <button class="item danger" type="button" onclick={quit}>
    {$t("menu.quit")}
  </button>
</nav>

<style>
  .menu {
    display: flex;
    flex-direction: column;
    padding: 5px;
    margin: 3px;
    background: rgb(var(--panel));
    color: rgb(var(--fg));
    border: 1px solid rgb(var(--border));
    border-radius: 10px;
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.28);
  }
  .item {
    text-align: left;
    padding: 8px 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: inherit;
    font-size: 0.82rem;
    cursor: default;
    transition: background 0.08s ease;
  }
  .item:hover {
    background: rgb(var(--accent) / 0.16);
  }
  .item.update {
    color: rgb(var(--accent));
    font-weight: 600;
  }
  .item.update:hover {
    background: rgb(var(--accent) / 0.16);
  }
  .item.danger {
    color: rgb(var(--danger));
  }
  .item.danger:hover {
    background: rgb(var(--danger) / 0.16);
  }
  .sep {
    height: 1px;
    margin: 5px 8px;
    background: rgb(var(--border));
  }
</style>
