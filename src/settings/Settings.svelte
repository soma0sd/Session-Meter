<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getVersion } from "@tauri-apps/api/app";
  import { version as bundledVersion } from "../../package.json";
  import { t, setLocale, localeNames, matchLocale, type Language } from "../lib/i18n";
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
    isMicrosoftStoreBuild,
    checkForUpdate,
    getUpdateState,
    installUpdate,
    openNewsWindow,
    type Settings,
    type ServiceStatus,
    type UpdateInfo,
    type UsageSnapshot,
  } from "../lib/ipc";

  const DEFAULTS: Settings = {
    theme: "system",
    language: "auto",
    refresh_interval_min: 5,
    github_auto_install_updates: false,
    widgets: {},
    notify: { enabled: true, session_threshold: 80, weekly_threshold: 80, on_reset: true },
    history_retention_days: 30,
    org_name: "",
    account_email: "",
    dock: {
      enabled: false,
      columns: 2,
      order: [],
      opacity: 0.9,
      always_on_top: true,
      move_lock: false,
      anchor_x: 0,
      anchor_y: 0,
    },
  };

  let s = $state<Settings>(structuredClone(DEFAULTS));
  let services = $state<ServiceStatus[]>([]);
  let autostart = $state(false);
  let autostartSaving = $state(false);
  let signingIn = $state<string | null>(null);
  let version = $state(bundledVersion);
  let updateInfo = $state<UpdateInfo | null>(null);
  let updateChecking = $state(false);
  let updateCheckFailed = $state(false);
  let updateInstalling = $state(false);
  let microsoftStore = $state<boolean | null>(null);

  let unlisteners: Array<() => void> = [];

  onMount(async () => {
    void initWindow().then((initial) => { if (initial) s = initial; });
    void refreshServices();
    void getAutostart().then((value) => (autostart = value)).catch(() => {});
    void isMicrosoftStoreBuild().then((value) => (microsoftStore = value)).catch(() => {});
    void getVersion().then((value) => (version = value)).catch(() => {});
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
        await listen<UpdateInfo>("update://available", (e) => {
          updateCheckFailed = false;
          updateInfo = e.payload;
        }),
        await listen<UsageSnapshot>("usage://updated", ({ payload }) => {
          if (payload.service_id !== "antigravity_ide") return;
          const status = services.find((svc) => svc.id === payload.service_id);
          if (!status) return;
          status.email = payload.account_email;
          status.subscription = payload.subscription;
          status.live_status = payload.status;
          status.antigravity_source = payload.antigravity_source;
        }),
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
    void getUpdateState().then((info) => {
      if (!updateInfo && !updateChecking && !updateCheckFailed) updateInfo = info;
    }).catch(() => {});
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

  async function onLanguage(v: Language) {
    s.language = v;
    await save();
    setLocale(v === "auto"
      ? await getEffectiveLocale().catch(() => matchLocale(navigator.language) ?? "en")
      : v);
  }

  async function onAutostart(v: boolean) {
    const previous = autostart;
    autostartSaving = true;
    autostart = v;
    try {
      await setAutostart(v);
    } catch {
      autostart = await getAutostart().catch(() => previous);
    } finally {
      autostartSaving = false;
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
    updateCheckFailed = false;
    try {
      updateInfo = await checkForUpdate();
    } catch {
      updateInfo = null;
      updateCheckFailed = true;
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
      {#if svc.id === "antigravity_ide"}
        <!-- No login/logout: monitoring is fully automatic (see service::has_session). This
             row is purely informational - current reachability + whatever account identity
             the IDE's local API happens to expose (best-effort, may be empty). -->
        <div class="account">
          <div class="acct">
            <span class="acct-name">{svc.name}</span>
            {#if svc.email}<span class="acct-email">{svc.email}</span>{/if}
            {#if svc.subscription}<span class="acct-sub">{svc.subscription}</span>{/if}
          </div>
          <span class="status-pill" class:on={svc.live_status === "ok"}>
            {#if svc.live_status === "ok" && svc.antigravity_source}
              {$t(svc.antigravity_source === "ide" ? "settings.antigravityIde" : "settings.antigravityCli")}
            {:else if svc.live_status === "not_running"}
              {$t("settings.antigravityWaiting")}
            {:else if svc.live_status}
              {$t("common.usageUnavailable")}
            {:else}
              {$t("common.loading")}
            {/if}
          </span>
        </div>
      {:else}
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
      {/if}
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
      <select id="lang" value={s.language} onchange={(e) => onLanguage(e.currentTarget.value as Language)}>
        <option value="auto">{$t("settings.language.auto")}</option>
        {#each Object.entries(localeNames) as [value, name] (value)}
          <option {value}>{name}</option>
        {/each}
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
      <input id="autostart" type="checkbox" checked={autostart} disabled={autostartSaving}
        onchange={(e) => onAutostart(e.currentTarget.checked)} />
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
      <span class="status" role="status">
        {#if microsoftStore}
          {$t("update.managedByStore")}
        {:else if updateCheckFailed}
          {$t("update.checkFailed")}
        {:else if updateInfo?.available}
          {$t("update.available", { version: updateInfo.version })}
        {:else if updateInfo}
          {$t("update.upToDate")}
        {:else}
          {$t("update.current", { version })}
        {/if}
      </span>
      {#if microsoftStore === false && updateInfo?.available}
        <button class="btn primary" onclick={installUpd} disabled={updateInstalling}>
          {updateInstalling ? $t("update.installing") : $t("update.install")}
        </button>
      {:else if microsoftStore === false}
        <button class="btn" onclick={checkUpdate} disabled={updateChecking}>
          {updateChecking ? $t("update.checking") : $t(updateCheckFailed ? "update.retry" : "update.check")}
        </button>
      {/if}
    </div>
    {#if microsoftStore === false}
      <div class="row">
        <div>
          <label for="github-auto-install">{$t("update.autoInstall")}</label>
          <p id="github-auto-install-hint" class="update-hint">{$t("update.autoInstallHint")}</p>
        </div>
        <input
          type="checkbox"
          id="github-auto-install"
          aria-describedby="github-auto-install-hint"
          checked={s.github_auto_install_updates}
          onchange={(event) => {
            s.github_auto_install_updates = event.currentTarget.checked;
            void save();
          }}
        />
      </div>
    {/if}
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
  .update-hint {
    margin: 2px 0 0;
    font-size: 0.75rem;
    color: rgb(var(--fg-muted));
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
  .status-pill {
    flex-shrink: 0;
    font-size: 0.72rem;
    font-weight: 600;
    padding: 3px 10px;
    border-radius: 999px;
    background: rgb(var(--fg-muted) / 0.16);
    color: rgb(var(--fg-muted));
    white-space: nowrap;
  }
  .status-pill.on {
    background: rgb(var(--ok) / 0.16);
    color: rgb(var(--ok));
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
