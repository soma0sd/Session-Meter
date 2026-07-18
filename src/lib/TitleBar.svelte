<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "../lib/i18n";

  // Props (Svelte 5 runes)
  let { title = "" }: { title?: string } = $props();

  // Inline SVG icons using currentColor so they follow the button's text color.
  const MINIMIZE = `<svg viewBox="0 0 12 12" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"><line x1="2" y1="6" x2="10" y2="6"/></svg>`;
  const CLOSE = `<svg viewBox="0 0 12 12" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"><line x1="2.5" y1="2.5" x2="9.5" y2="9.5"/><line x1="9.5" y1="2.5" x2="2.5" y2="9.5"/></svg>`;

  async function minimize() {
    try {
      await getCurrentWindow().minimize();
    } catch {
      /* no-op in a plain browser preview */
    }
  }

  // For settings/stats we HIDE instead of close so the app stays in the tray.
  async function hide() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* no-op in a plain browser preview */
    }
  }

  // Keep control buttons out of the drag region so a click doesn't start a drag.
  function stopDrag(e: MouseEvent) {
    e.stopPropagation();
  }
</script>

<div class="titlebar" data-tauri-drag-region>
  <span class="title" data-tauri-drag-region>{title}</span>
  <div class="controls">
    <button
      type="button"
      class="ctl min"
      aria-label={$t("common.minimize")}
      title={$t("common.minimize")}
      onmousedown={stopDrag}
      onclick={minimize}>{@html MINIMIZE}</button>
    <button
      type="button"
      class="ctl close"
      aria-label={$t("common.close")}
      title={$t("common.close")}
      onmousedown={stopDrag}
      onclick={hide}>{@html CLOSE}</button>
  </div>
</div>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 34px;
    flex: 0 0 34px;
    padding-left: 12px;
    background: rgb(var(--panel));
    color: rgb(var(--fg));
    border-bottom: 1px solid rgb(var(--border));
    user-select: none;
    -webkit-user-select: none;
    cursor: default;
  }
  .title {
    font-size: 0.76rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    color: rgb(var(--fg-muted));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none; /* let drags pass through to the bar */
  }
  .controls {
    display: flex;
    align-items: stretch;
    height: 100%;
  }
  .ctl {
    display: grid;
    place-items: center;
    width: 46px;
    height: 100%;
    padding: 0;
    margin: 0;
    border: 0;
    background: transparent;
    color: rgb(var(--fg-muted));
    cursor: default;
    -webkit-app-region: no-drag;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .ctl:hover {
    color: rgb(var(--fg));
  }
  .ctl.min:hover {
    background: rgb(var(--accent) / 0.16);
  }
  .ctl.close:hover {
    background: rgb(var(--danger));
    color: #fff;
  }
  .ctl:focus-visible {
    outline: 2px solid rgb(var(--accent));
    outline-offset: -2px;
  }
</style>
