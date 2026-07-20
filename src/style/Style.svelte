<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import TitleBar from "../lib/TitleBar.svelte";
  import WidgetStyle from "../lib/widgetStyles/WidgetStyle.svelte";
  import { catalog, colorsFor } from "../lib/widgetStyles/types";
  import {
    getSettings,
    setSettings,
    getUsage,
    getServicesStatus,
    widgetConfig,
    type Settings,
    type UsageSnapshot,
    type ServiceStatus,
    type WidgetConfig,
  } from "../lib/ipc";

  let s = $state<Settings | null>(null);
  let services = $state<ServiceStatus[]>([]);
  let snaps = $state<Record<string, UsageSnapshot | null>>({});
  let now = $state(Date.now());
  let activeService = $state("claude");
  let timer: number | undefined;
  let unlisteners: Array<() => void> = [];

  function placeholder(service: string): UsageSnapshot {
    return {
      service_id: service,
      five_hour: { remaining: 54, utilization: 46, resets_at: new Date(Date.now() + 2 * 3600_000).toISOString() },
      weekly_primary: { remaining: 35, utilization: 65, resets_at: new Date(Date.now() + 4 * 86_400_000).toISOString() },
      primary_key: "five_hour",
      secondary_key: "seven_day",
      buckets: [],
      organization_name: "",
      account_email: "",
      subscription: "",
      fetched_at: "",
      status: "ok",
    };
  }

  const loggedIn = $derived(services.filter((x) => x.logged_in));

  // Colour the preview + thumbnails to match the active service's brand.
  $effect(() => {
    const c = colorsFor(activeService);
    document.documentElement.style.setProperty("--m1", c.m1);
    document.documentElement.style.setProperty("--m2", c.m2);
  });

  function previewSnap(service: string): UsageSnapshot {
    const snap = snaps[service];
    return snap && snap.status === "ok" ? snap : placeholder(service);
  }

  // Re-read the service list + each service's usage (called on mount, on login/logout, and
  // when the window regains focus, so a service signed in elsewhere shows up here).
  async function refreshServices() {
    try {
      services = await getServicesStatus();
    } catch {
      return;
    }
    const li = services.filter((x) => x.logged_in);
    if (li.length && !li.some((x) => x.id === activeService)) {
      activeService = li[0].id;
    }
    for (const svc of services.filter((x) => x.logged_in)) {
      try {
        const u = await getUsage(svc.id);
        snaps = { ...snaps, [svc.id]: u };
      } catch {
        /* preview */
      }
    }
  }

  onMount(async () => {
    await initWindow();
    try {
      s = await getSettings();
    } catch {
      /* preview */
    }
    await refreshServices();
    try {
      unlisteners.push(
        await listen<UsageSnapshot>("usage://updated", (e) => {
          snaps = { ...snaps, [e.payload.service_id]: e.payload };
        }),
      );
      unlisteners.push(await listen<Settings>("settings://changed", (e) => (s = e.payload)));
      unlisteners.push(await listen("session://changed", () => void refreshServices()));
      unlisteners.push(
        await listen<string>("theme://changed", (e) => applyTheme(e.payload as Theme)),
      );
      unlisteners.push(
        await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (focused) void refreshServices();
        }),
      );
    } catch {
      /* preview */
    }
    timer = window.setInterval(() => (now = Date.now()), 1000);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
    unlisteners.forEach((u) => u());
  });

  async function save() {
    if (!s) return;
    try {
      await setSettings($state.snapshot(s));
    } catch {
      /* preview */
    }
  }

  function updateWidget(service: string, patch: Partial<WidgetConfig>) {
    if (!s) return;
    const cur = widgetConfig(s, service);
    s.widgets = { ...s.widgets, [service]: { ...cur, ...patch } };
    void save();
  }
</script>

<div class="win">
  <TitleBar title={$t("widgetStyle.title")} />
  <main>
    {#if s === null}
      <div class="empty">{$t("common.loading")}</div>
    {:else if loggedIn.length === 0}
      <div class="empty">{$t("widgetStyle.noServices")}</div>
    {:else}
      {#if loggedIn.length > 1}
        <div class="tabs" role="tablist">
          {#each loggedIn as svc (svc.id)}
            <button
              class="tab"
              class:active={activeService === svc.id}
              role="tab"
              aria-selected={activeService === svc.id}
              onclick={() => (activeService = svc.id)}>{svc.name}</button>
          {/each}
        </div>
      {/if}

      {#each loggedIn.filter((x) => x.id === activeService) as svc (svc.id)}
        {@const wc = widgetConfig(s, svc.id)}
        <section class="svc">
          <div class="previewWrap">
            <div class="previewPanel">
              <WidgetStyle styleId={wc.style} snapshot={previewSnap(svc.id)} {now} displayMode={wc.display_mode} />
            </div>
          </div>

          <div class="field">
            <span class="flabel">{$t("widgetStyle.displayMode")}</span>
            <div class="toggle">
              <button
                type="button"
                class="tbtn"
                class:active={wc.display_mode === "remaining"}
                onclick={() => updateWidget(svc.id, { display_mode: "remaining" })}>
                {$t("widgetStyle.showRemaining")}
              </button>
              <button
                type="button"
                class="tbtn"
                class:active={wc.display_mode === "used"}
                onclick={() => updateWidget(svc.id, { display_mode: "used" })}>
                {$t("widgetStyle.showUsed")}
              </button>
            </div>
          </div>

          <div class="field col">
            <span class="flabel">{$t("widgetStyle.pickStyle")}</span>
            <div class="grid">
              {#each catalog as entry (entry.id)}
                <button
                  type="button"
                  class="card"
                  class:selected={wc.style === entry.id}
                  onclick={() => updateWidget(svc.id, { style: entry.id })}>
                  <div class="thumb">
                    <WidgetStyle styleId={entry.id} snapshot={previewSnap(svc.id)} {now} displayMode={wc.display_mode} />
                  </div>
                  <span class="cardlabel">{$t(entry.labelKey)}</span>
                </button>
              {/each}
            </div>
          </div>

          <div class="field">
            <span class="flabel">{$t("widgetStyle.opacity")}</span>
            <input
              type="range"
              min="0.3"
              max="1"
              step="0.05"
              value={wc.opacity}
              oninput={(e) => updateWidget(svc.id, { opacity: Number((e.target as HTMLInputElement).value) })} />
            <span class="fval">{Math.round(wc.opacity * 100)}%</span>
          </div>

          <label class="check">
            <input
              type="checkbox"
              checked={wc.always_on_top}
              onchange={(e) => updateWidget(svc.id, { always_on_top: (e.target as HTMLInputElement).checked })} />
            <span>{$t("widget.alwaysOnTop")}</span>
          </label>
        </section>
      {/each}
    {/if}
  </main>
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: rgb(var(--bg));
    color: rgb(var(--fg));
  }
  main {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .empty {
    text-align: center;
    color: rgb(var(--fg-muted));
    padding: 40px 0;
  }
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid rgb(var(--border));
  }
  .tab {
    padding: 6px 14px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: rgb(var(--fg-muted));
    font-size: 0.82rem;
    font-weight: 600;
    cursor: pointer;
  }
  .tab:hover {
    color: rgb(var(--fg));
  }
  .tab.active {
    color: rgb(var(--accent));
    border-bottom-color: rgb(var(--accent));
  }
  .svc {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 15px;
    border: 1px solid rgb(var(--border));
    border-radius: 11px;
    background: rgb(var(--panel));
  }
  .previewWrap {
    display: flex;
    justify-content: center;
    padding: 12px;
    background: rgb(var(--bg));
    border: 1px solid rgb(var(--border));
    border-radius: var(--radius);
  }
  .previewPanel {
    /* Hug the content like the real widget does. */
    width: max-content;
    min-width: 150px;
    max-width: 320px;
    padding: 9px 11px;
    background: rgb(var(--panel));
    border: 1px solid rgb(var(--border));
    border-radius: 12px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .field.col {
    flex-direction: column;
    align-items: stretch;
  }
  .flabel {
    font-size: 0.8rem;
    font-weight: 600;
    color: rgb(var(--fg-muted));
    min-width: 96px;
  }
  .fval {
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
    min-width: 40px;
    text-align: right;
  }
  input[type="range"] {
    flex: 1;
    accent-color: rgb(var(--accent));
  }
  .toggle {
    display: flex;
    gap: 6px;
  }
  .tbtn {
    padding: 5px 12px;
    font-size: 0.78rem;
    font-weight: 600;
    border: 1px solid rgb(var(--border));
    border-radius: 8px;
    background: rgb(var(--panel));
    color: rgb(var(--fg-muted));
    cursor: pointer;
  }
  .tbtn.active {
    background: rgb(var(--accent));
    border-color: rgb(var(--accent));
    color: rgb(var(--on-accent));
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 10px;
    margin-top: 4px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1.5px solid rgb(var(--border));
    border-radius: 10px;
    background: rgb(var(--panel));
    cursor: pointer;
    text-align: left;
  }
  .card:hover {
    border-color: rgb(var(--accent) / 0.5);
  }
  .card.selected {
    border-color: rgb(var(--accent));
    box-shadow: 0 0 0 1px rgb(var(--accent));
  }
  .thumb {
    min-height: 60px;
    display: flex;
    align-items: center;
    pointer-events: none;
    overflow: hidden;
  }
  .cardlabel {
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--fg-muted));
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.82rem;
    cursor: pointer;
  }
  .check input {
    accent-color: rgb(var(--accent));
  }
</style>
