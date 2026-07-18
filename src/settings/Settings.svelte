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
    getSessionStatus,
    openLoginWindow,
    clearSession,
    setAutostart,
    getAutostart,
    checkForUpdate,
    installUpdate,
    openNewsWindow,
    type Settings,
    type SessionStatus,
    type UpdateInfo,
  } from "../lib/ipc";

  const DEFAULTS: Settings = {
    theme: "system",
    language: "auto",
    widget_opacity: 0.9,
    refresh_interval_min: 5,
    always_on_top: true,
    move_lock: false,
    tray_display: "remaining",
    widget_layout: "detailed",
    widget_visible: true,
    notify: { enabled: true, session_threshold: 80, weekly_threshold: 80, on_reset: true },
    history_retention_days: 30,
    org_name: "",
    account_email: "",
  };

  let s = $state<Settings>(structuredClone(DEFAULTS));
  let session = $state<SessionStatus>({ logged_in: false, org_name: "", email: "" });
  let autostart = $state(false);
  let signingIn = $state(false);
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
    await refreshSession();
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
        await listen<SessionStatus>("session://changed", (e) => {
          session = e.payload;
          signingIn = false;
        }),
      );
      // Startup update check emits this when a newer signed release exists.
      unlisteners.push(
        await listen<UpdateInfo>("update://available", (e) => (updateInfo = e.payload)),
      );
      // Re-check the signed-in state whenever the settings window regains focus, so the
      // account panel is correct after signing in/out from elsewhere.
      unlisteners.push(
        await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (focused) void refreshSession();
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

  async function refreshSession() {
    try {
      session = await getSessionStatus();
    } catch {
      /* preview */
    }
  }

  async function signIn() {
    signingIn = true;
    try {
      await openLoginWindow();
    } finally {
      // The login window is now open; capture is driven by Rust and a
      // `session://changed` event updates the signed-in state on success. Reset the
      // button so it never sticks in "waiting" if the user cancels the login window.
      signingIn = false;
    }
  }

  async function signOut() {
    try {
      await clearSession();
    } catch {
      /* preview */
    }
    session = { logged_in: false, org_name: "", email: "" };
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

  const intervals = [1, 5, 10, 15, 30];
  const thresholds = [50, 60, 70, 80, 90, 95];
</script>

<div class="win">
  <TitleBar title={$t("settings.title")} />
  <main>
  <section>
    <h2>{$t("settings.section.account")}</h2>
    <div class="account">
      {#if session.logged_in}
        <div class="acct">
          <span class="acct-name">{session.org_name || "Claude"}</span>
          {#if session.email}
            <span class="acct-email">{session.email}</span>
          {/if}
        </div>
        <button class="btn" onclick={signOut}>{$t("settings.signOut")}</button>
      {:else}
        <span class="status">{$t("settings.signedOut")}</span>
        <button class="btn primary" onclick={signIn} disabled={signingIn}>
          {signingIn ? $t("settings.signingIn") : $t("settings.signIn")}
        </button>
      {/if}
    </div>
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
      <label for="display">{$t("settings.trayDisplay")}</label>
      <select
        id="display"
        value={s.tray_display}
        onchange={(e) => {
          s.tray_display = e.currentTarget.value as "remaining" | "used";
          save();
        }}>
        <option value="remaining">{$t("common.remaining")}</option>
        <option value="used">{$t("common.used")}</option>
      </select>
    </div>

    <div class="row">
      <label for="layout">{$t("settings.widgetLayout")}</label>
      <select
        id="layout"
        value={s.widget_layout}
        onchange={(e) => {
          s.widget_layout = e.currentTarget.value as "detailed" | "compact";
          save();
        }}>
        <option value="detailed">{$t("settings.layout.detailed")}</option>
        <option value="compact">{$t("settings.layout.compact")}</option>
      </select>
    </div>

    <div class="row">
      <label for="opacity">{$t("settings.widgetOpacity")}</label>
      <div class="slider">
        <input
          id="opacity"
          type="range"
          min="0.3"
          max="1"
          step="0.05"
          value={s.widget_opacity}
          onchange={(e) => {
            s.widget_opacity = Number(e.currentTarget.value);
            save();
          }} />
        <span class="pct">{Math.round(s.widget_opacity * 100)}%</span>
      </div>
    </div>

    <div class="row">
      <label for="aot">{$t("widget.alwaysOnTop")}</label>
      <input
        id="aot"
        type="checkbox"
        checked={s.always_on_top}
        onchange={(e) => {
          s.always_on_top = e.currentTarget.checked;
          save();
        }} />
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
  .slider {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 150px;
  }
  input[type="range"] {
    flex: 1;
    accent-color: rgb(var(--accent));
  }
  .pct {
    font-size: 0.76rem;
    color: rgb(var(--fg-muted));
    width: 34px;
    text-align: right;
    font-variant-numeric: tabular-nums;
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
