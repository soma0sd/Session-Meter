<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t, setLocale, type Locale } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import TitleBar from "../lib/TitleBar.svelte";
  import {
    getSettings,
    setSettings,
    getEffectiveLocale,
    getServicesStatus,
    openLoginWindow,
    clearSession,
    openStyleWindow,
    setAutostart,
    getAutostart,
    checkForUpdate,
    installUpdate,
    openNewsWindow,
    type Settings,
    type ServiceStatus,
    type UpdateInfo,
  } from "../lib/ipc";

  const DEFAULTS: Settings = {
    theme: "system",
    language: "auto",
    refresh_interval_min: 5,
    widgets: {},
    notify: { enabled: true, session_threshold: 80, weekly_threshold: 80, on_reset: true },
    history_retention_days: 30,
    org_name: "",
    account_email: "",
  };

  let s = $state<Settings>(structuredClone(DEFAULTS));
  let services = $state<ServiceStatus[]>([]);
  let autostart = $state(false);
  let signingIn = $state<string | null>(null);
  let version = $state("0.0.1");
  let updateInfo = $state<UpdateInfo | null>(null);
  let updateChecking = $state(false);
  let updateInstalling = $state(false);

  let unlisteners: Array<() => void> = [];

  onMount(async () => {
    await initWindow();
    try {
      s = await getSettings();
    } catch {
      /* preview */
    }
    await refreshServices();
    try {
      autostart = await getAutostart();
    } catch {
      /* preview */
    }
    try {
      const mod = await import("@tauri-apps/api/app");
      version = await mod.getVersion();
    } catch {
      /* preview */
    }
    try {
      unlisteners.push(
        await listen<Settings>("settings://changed", (e) => (s = e.payload)),
      );
      unlisteners.push(
        await listen<string>("theme://changed", (e) => applyTheme(e.payload as Theme)),
      );
      unlisteners.push(
        await listen("session://changed", () => {
          signingIn = null;
          void refreshServices();
        }),
      );
      // Startup update check emits this when a newer signed release exists.
      unlisteners.push(
        await listen<UpdateInfo>("update://available", (e) => (updateInfo = e.payload)),
      );
      // Re-read settings + signed-in state whenever the window regains focus, so it starts from
      // the latest saved values and a change here can't clobber one made in another window (e.g.
      // the widget style window) that this window had not yet picked up.
      unlisteners.push(
        await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (focused) {
            void refreshSettings();
            void refreshServices();
          }
        }),
      );
    } catch {
      /* preview */
    }
  });

  onDestroy(() => unlisteners.forEach((u) => u()));

  async function save() {
    try {
      await setSettings($state.snapshot(s));
    } catch {
      /* preview */
    }
  }

  async function onTheme(v: Theme) {
    s.theme = v;
    applyTheme(v);
    await save();
  }

  async function onLanguage(v: "auto" | "ko" | "en") {
    s.language = v;
    await save();
    let loc: Locale = "en";
    if (v === "auto") {
      try {
        loc = await getEffectiveLocale();
      } catch {
        loc = (navigator.language || "en").toLowerCase().startsWith("ko") ? "ko" : "en";
      }
    } else {
      loc = v;
    }
    setLocale(loc);
  }

  async function onAutostart(v: boolean) {
    autostart = v;
    try {
      await setAutostart(v);
    } catch {
      /* preview */
    }
  }

  async function refreshServices() {
    try {
      services = await getServicesStatus();
    } catch {
      /* preview */
    }
  }

  // Re-read the persisted settings (on focus) so this window never saves a stale snapshot that
  // would revert a change made elsewhere. Safe because every control here saves on change.
  async function refreshSettings() {
    try {
      s = await getSettings();
    } catch {
      /* preview */
    }
  }

  async function signIn(service: string) {
    signingIn = service;
    try {
      await openLoginWindow(service);
    } catch {
      // Login for this service may not be available yet (e.g. Gemini before sign-in).
    } finally {
      // The login window is now open (Claude); capture is driven by Rust and a
      // `session://changed` event refreshes the account list. Reset the button so it never
      // sticks in "waiting" if the user cancels the login window.
      signingIn = null;
    }
  }

  async function signOut(service: string) {
    try {
      await clearSession(service);
    } catch {
      /* preview */
    }
    await refreshServices();
  }

  async function checkUpdate() {
    updateChecking = true;
    try {
      updateInfo = await checkForUpdate();
    } catch {
      updateInfo = { available: false, version: "", notes: "" };
    } finally {
      updateChecking = false;
    }
  }

  async function installUpd() {
    updateInstalling = true;
    try {
      await installUpdate();
    } finally {
      updateInstalling = false;
    }
  }

  async function viewNews() {
    try {
      await openNewsWindow();
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

  const intervals = [1, 5, 10, 15, 30];
  const thresholds = [50, 60, 70, 80, 90, 95];
</script>

<div class="win">
  <TitleBar title={$t("settings.title")} />
  <main>
  <section>
    <h2>{$t("settings.section.account")}</h2>
    {#each services as svc (svc.id)}
      <div class="account">
        <div class="acct">
          <span class="acct-name">{svc.name}</span>
          {#if svc.logged_in}
            {@const account = svc.email || (svc.org_name && svc.org_name !== svc.name ? svc.org_name : "")}
            {#if account}<span class="acct-email">{account}</span>{/if}
            {#if svc.subscription}<span class="acct-sub">{svc.subscription}</span>{/if}
          {:else}
            <span class="acct-email">{$t("settings.signedOut")}</span>
          {/if}
        </div>
        {#if svc.logged_in}
          <button class="btn" onclick={() => signOut(svc.id)}>{$t("settings.signOut")}</button>
        {:else}
          <button
            class="btn primary"
            onclick={() => signIn(svc.id)}
            disabled={signingIn === svc.id}>
            {signingIn === svc.id ? $t("settings.signingIn") : $t("settings.signIn")}
          </button>
        {/if}
      </div>
    {/each}
  </section>

  <section>
    <h2>{$t("settings.section.general")}</h2>

    <div class="row">
      <label for="theme">{$t("settings.theme")}</label>
      <select id="theme" value={s.theme} onchange={(e) => onTheme(e.currentTarget.value as Theme)}>
        <option value="system">{$t("settings.theme.system")}</option>
        <option value="light">{$t("settings.theme.light")}</option>
        <option value="dark">{$t("settings.theme.dark")}</option>
      </select>
    </div>

    <div class="row">
      <label for="lang">{$t("settings.language")}</label>
      <select id="lang" value={s.language} onchange={(e) => onLanguage(e.currentTarget.value as "auto" | "ko" | "en")}>
        <option value="auto">{$t("settings.language.auto")}</option>
        <option value="ko">{$t("settings.language.ko")}</option>
        <option value="en">{$t("settings.language.en")}</option>
      </select>
    </div>

    <div class="row">
      <label for="interval">{$t("settings.refreshInterval")}</label>
      <select
        id="interval"
        value={String(s.refresh_interval_min)}
        onchange={(e) => {
          s.refresh_interval_min = Number(e.currentTarget.value);
          save();
        }}>
        {#each intervals as n (n)}
          <option value={String(n)}>{$t("unit.minutes", { n })}</option>
        {/each}
      </select>
    </div>

    <div class="row">
      <label for="autostart">{$t("settings.autostart")}</label>
      <input id="autostart" type="checkbox" checked={autostart} onchange={(e) => onAutostart(e.currentTarget.checked)} />
    </div>
  </section>

  <section>
    <h2>{$t("settings.section.widget")}</h2>
    <div class="row">
      <span class="status">{$t("widgetStyle.title")}</span>
      <button class="btn" onclick={openStyle}>{$t("settings.openStyle")}</button>
    </div>
  </section>

  <section>
    <h2>{$t("settings.section.notifications")}</h2>
    <div class="row">
      <label for="notify">{$t("settings.notify.enabled")}</label>
      <input
        id="notify"
        type="checkbox"
        checked={s.notify.enabled}
        onchange={(e) => {
          s.notify.enabled = e.currentTarget.checked;
          save();
        }} />
    </div>
    <div class="row">
      <label for="onreset" class:dim={!s.notify.enabled}>{$t("settings.notify.onReset")}</label>
      <input
        id="onreset"
        type="checkbox"
        checked={s.notify.on_reset}
        disabled={!s.notify.enabled}
        onchange={(e) => {
          s.notify.on_reset = e.currentTarget.checked;
          save();
        }} />
    </div>
    <div class="row">
      <label for="sesTh" class:dim={!s.notify.enabled}>{$t("settings.notify.sessionThreshold")}</label>
      <select
        id="sesTh"
        disabled={!s.notify.enabled}
        value={String(s.notify.session_threshold)}
        onchange={(e) => {
          s.notify.session_threshold = Number(e.currentTarget.value);
          save();
        }}>
        {#each thresholds as n (n)}
          <option value={String(n)}>{n}%</option>
        {/each}
      </select>
    </div>
    <div class="row">
      <label for="wkTh" class:dim={!s.notify.enabled}>{$t("settings.notify.weeklyThreshold")}</label>
      <select
        id="wkTh"
        disabled={!s.notify.enabled}
        value={String(s.notify.weekly_threshold)}
        onchange={(e) => {
          s.notify.weekly_threshold = Number(e.currentTarget.value);
          save();
        }}>
        {#each thresholds as n (n)}
          <option value={String(n)}>{n}%</option>
        {/each}
      </select>
    </div>
  </section>

  <section>
    <h2>{$t("settings.section.update")}</h2>
    <div class="row">
      <span class="status">
        {#if updateInfo?.available}
          {$t("update.available", { version: updateInfo.version })}
        {:else if updateInfo}
          {$t("update.upToDate")}
        {:else}
          {$t("update.current", { version })}
        {/if}
      </span>
      {#if updateInfo?.available}
        <button class="btn primary" onclick={installUpd} disabled={updateInstalling}>
          {updateInstalling ? $t("update.installing") : $t("update.install")}
        </button>
      {:else}
        <button class="btn" onclick={checkUpdate} disabled={updateChecking}>
          {updateChecking ? $t("update.checking") : $t("update.check")}
        </button>
      {/if}
    </div>
    <div class="row">
      <span class="status">{$t("update.newsHint")}</span>
      <button class="btn" onclick={viewNews}>{$t("update.news")}</button>
    </div>
  </section>

  <footer>
    <span>{$t("app.name")}</span>
    <span class="muted">{$t("settings.version")} {version}</span>
  </footer>
  </main>
</div>

<style>
  .win {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  main {
    flex: 1;
    overflow-y: auto;
    padding: 16px 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 13px 15px;
    border: 1px solid rgb(var(--border));
    border-radius: 11px;
    background: rgb(var(--panel));
  }
  h2 {
    margin: 0 0 2px;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: rgb(var(--fg-muted));
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  label {
    font-size: 0.84rem;
  }
  select {
    min-width: 130px;
    padding: 5px 8px;
    border: 1px solid rgb(var(--border));
    border-radius: 7px;
    background: rgb(var(--panel));
    color: rgb(var(--fg));
    font-size: 0.82rem;
  }
  input[type="checkbox"] {
    width: 17px;
    height: 17px;
    accent-color: rgb(var(--accent));
  }
  .account {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .dim {
    color: rgb(var(--fg-muted));
    opacity: 0.6;
  }
  select:disabled,
  input:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .status {
    font-size: 0.82rem;
    color: rgb(var(--fg-muted));
    /* Let a long org name ellipsize instead of pushing the sign-out button out. */
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acct {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .acct-name {
    font-size: 0.86rem;
    font-weight: 600;
    color: rgb(var(--fg));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acct-email {
    font-size: 0.74rem;
    color: rgb(var(--fg-muted));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .acct-sub {
    align-self: flex-start;
    max-width: 100%;
    margin-top: 2px;
    font-size: 0.66rem;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
    background: rgb(var(--accent) / 0.16);
    color: rgb(var(--accent));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn {
    padding: 6px 13px;
    border: 1px solid rgb(var(--border));
    border-radius: 8px;
    background: transparent;
    color: rgb(var(--fg));
    font-size: 0.8rem;
    cursor: pointer;
  }
  .btn:hover {
    border-color: rgb(var(--accent));
  }
  .btn.primary {
    background: rgb(var(--accent));
    border-color: rgb(var(--accent));
    color: rgb(var(--on-accent));
  }
  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  footer {
    display: flex;
    justify-content: space-between;
    padding-top: 6px;
    border-top: 1px solid rgb(var(--border));
    font-size: 0.74rem;
  }
  .muted {
    color: rgb(var(--fg-muted));
  }
</style>
