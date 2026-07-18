<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { t, locale } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import TitleBar from "../lib/TitleBar.svelte";
  import {
    getUsage,
    getHistory,
    refreshNow,
    type UsageSnapshot,
    type HistoryPoint,
    type Bucket,
  } from "../lib/ipc";
  import { formatCountdown, formatResetDateTime } from "../lib/countdown";
  import { linePath, areaPath, mapY } from "../lib/charts";
  import { forecast, type Forecast } from "../lib/forecast";

  const CW = 620;
  const CH = 150;

  let snap = $state<UsageSnapshot | null>(null);
  let history = $state<HistoryPoint[]>([]);
  let range = $state<"24h" | "7d">("24h");
  let now = $state(Date.now());

  let timer: number | undefined;
  let unlisteners: Array<() => void> = [];

  async function loadHistory() {
    try {
      history = await getHistory(range);
    } catch {
      history = [];
    }
  }

  onMount(async () => {
    await initWindow();
    try {
      snap = await getUsage();
    } catch {
      /* preview */
    }
    await loadHistory();
    // Auto-refresh on open for fresh numbers; further updates arrive via usage://updated.
    void refreshNow()
      .then((s) => {
        snap = s;
        void loadHistory();
      })
      .catch(() => {});
    try {
      unlisteners.push(
        await listen<UsageSnapshot>("usage://updated", (e) => {
          snap = e.payload;
          void loadHistory();
        }),
      );
      unlisteners.push(
        await listen<string>("theme://changed", (e) => applyTheme(e.payload as Theme)),
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

  async function setRange(r: "24h" | "7d") {
    range = r;
    await loadHistory();
  }

  const localeTag = $derived($locale === "ko" ? "ko-KR" : "en-US");
  function fmtTime(iso: string): string {
    return formatResetDateTime(iso, localeTag, $t("common.today"), $t("common.tomorrow")) || "-";
  }

  function seriesValues(key: "five_hour" | "weekly"): number[] {
    return history.map((p) => p[key]).filter((v): v is number => v != null);
  }
  function seriesTimes(key: "five_hour" | "weekly"): number[] {
    return history.filter((p) => p[key] != null).map((p) => Date.parse(p.at));
  }

  const fiveVals = $derived(seriesValues("five_hour"));
  const weekVals = $derived(seriesValues("weekly"));
  const hasHistory = $derived(fiveVals.length >= 2 || weekVals.length >= 2);

  function fc(key: "five_hour" | "weekly", bucket: { remaining: number; resets_at: string } | null): Forecast {
    if (!bucket) return { depleting: false, minutesToEmpty: null, beforeReset: false };
    const resetMs = Math.max(0, Date.parse(bucket.resets_at) - now);
    return forecast(seriesTimes(key), seriesValues(key), bucket.remaining, resetMs);
  }
  const fiveFc = $derived(fc("five_hour", snap?.five_hour ?? null));
  const weekFc = $derived(fc("weekly", snap?.weekly_primary ?? null));

  function fcText(f: Forecast): string {
    if (!f.depleting || f.minutesToEmpty == null) return $t("stats.forecast.stable");
    const time = formatCountdown(f.minutesToEmpty * 60000, $locale);
    const base = $t("stats.forecast.empties", { time });
    return f.beforeReset ? `${base} (${$t("stats.forecast.beforeReset")})` : $t("stats.forecast.safe");
  }

  function barColor(remaining: number): string {
    return remaining < 20
      ? "rgb(var(--danger))"
      : remaining < 50
        ? "rgb(var(--warn))"
        : "rgb(var(--ok))";
  }

  function bucketLabel(b: Bucket): string {
    const l = $t("bucket." + b.key);
    return l.startsWith("bucket.") ? b.label : l;
  }
</script>

<div class="win">
  <TitleBar title={$t("stats.title")} />
  <main>
  {#if snap && (snap.organization_name || snap.account_email)}
    <div class="org">
      <span class="org-name">{snap.organization_name || "Claude"}</span>
      {#if snap.account_email}
        <span class="org-email">{snap.account_email}</span>
      {/if}
    </div>
  {/if}

  {#if snap === null}
    <div class="empty">{$t("common.loading")}</div>
  {:else if snap.status !== "ok"}
    <div class="empty">
      {snap.status === "unauthorized" ? $t("common.sessionExpired") : $t("common.notLoggedIn")}
    </div>
  {:else}
    <section class="cards">
      {#each snap.buckets as b (b.key)}
        {@const remainMs = Math.max(0, Date.parse(b.resets_at) - now)}
        <div class="card">
          <div class="card-label">{bucketLabel(b)}</div>
          <div class="card-value" style="color:{barColor(b.remaining)}">{b.remaining}%</div>
          <div class="card-sub">{b.utilization}% {$t("stats.used")}</div>
          <div class="bar"><div class="fill" style="width:{b.remaining}%; background:{barColor(b.remaining)}"></div></div>
          {#if b.resets_at}
            <div class="card-reset">{$t("common.resetsIn", { time: formatCountdown(remainMs, $locale) })}</div>
          {/if}
        </div>
      {/each}
    </section>

    <section class="panel">
      <div class="panel-head">
        <h2>{$t("stats.history")}</h2>
        <div class="range">
          <button class:active={range === "24h"} onclick={() => setRange("24h")}>24h</button>
          <button class:active={range === "7d"} onclick={() => setRange("7d")}>7d</button>
        </div>
      </div>
      {#if hasHistory}
        <svg class="chart" viewBox={`0 0 ${CW} ${CH}`} preserveAspectRatio="none" role="img" aria-label={$t("stats.history")}>
          {#each [25, 50, 75] as g (g)}
            <line x1="0" x2={CW} y1={mapY(g, CH, 3)} y2={mapY(g, CH, 3)} class="grid" />
          {/each}
          {#if weekVals.length >= 2}
            <path d={areaPath(weekVals, CW, CH)} class="area week" />
            <path d={linePath(weekVals, CW, CH)} class="line week" />
          {/if}
          {#if fiveVals.length >= 2}
            <path d={areaPath(fiveVals, CW, CH)} class="area five" />
            <path d={linePath(fiveVals, CW, CH)} class="line five" />
          {/if}
        </svg>
        <div class="legend">
          <span class="key"><i class="sw five"></i>{$t("bucket.five_hour")}</span>
          <span class="key"><i class="sw week"></i>{$t("bucket.seven_day")}</span>
        </div>
      {:else}
        <div class="nohist">{$t("stats.noHistory")}</div>
      {/if}
    </section>

    <section class="grid2">
      <div class="panel">
        <h2>{$t("stats.forecast")}</h2>
        <div class="fc"><span class="fc-label">{$t("bucket.five_hour")}</span><span>{fcText(fiveFc)}</span></div>
        <div class="fc"><span class="fc-label">{$t("bucket.seven_day")}</span><span>{fcText(weekFc)}</span></div>
      </div>
      <div class="panel">
        <h2>{$t("stats.resetSchedule")}</h2>
        <table>
          <thead>
            <tr><th>{$t("stats.col.window")}</th><th>{$t("stats.col.resetsAt")}</th><th>{$t("stats.col.in")}</th></tr>
          </thead>
          <tbody>
            {#each snap.buckets as b (b.key)}
              {@const remainMs = Math.max(0, Date.parse(b.resets_at) - now)}
              <tr>
                <td>{bucketLabel(b)}</td>
                <td>{fmtTime(b.resets_at)}</td>
                <td class="mono">{formatCountdown(remainMs, $locale)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/if}
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
    padding: 16px 18px 22px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .org {
    display: flex;
    align-items: baseline;
    gap: 9px;
    flex-wrap: wrap;
  }
  .org-name {
    font-size: 0.9rem;
    font-weight: 600;
    color: rgb(var(--fg));
  }
  .org-email {
    font-size: 0.78rem;
    color: rgb(var(--fg-muted));
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px;
  }
  .card {
    padding: 11px 13px;
    border: 1px solid rgb(var(--border));
    border-radius: 11px;
    background: rgb(var(--panel));
  }
  .card-label {
    font-size: 0.74rem;
    color: rgb(var(--fg-muted));
  }
  .card-value {
    font-size: 1.7rem;
    font-weight: 700;
    line-height: 1.15;
    font-variant-numeric: tabular-nums;
  }
  .card-sub {
    font-size: 0.7rem;
    color: rgb(var(--fg-muted));
    margin-bottom: 7px;
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
  }
  .card-reset {
    margin-top: 6px;
    font-size: 0.68rem;
    color: rgb(var(--fg-muted));
    font-variant-numeric: tabular-nums;
  }
  .panel {
    padding: 13px 15px;
    border: 1px solid rgb(var(--border));
    border-radius: 11px;
    background: rgb(var(--panel));
  }
  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0 0 9px;
    font-size: 0.82rem;
    color: rgb(var(--fg-muted));
    font-weight: 600;
  }
  .panel-head h2 {
    margin: 0;
  }
  .range {
    display: flex;
    gap: 4px;
  }
  .range button {
    padding: 3px 9px;
    border: 1px solid rgb(var(--border));
    border-radius: 6px;
    background: transparent;
    color: rgb(var(--fg-muted));
    font-size: 0.72rem;
    cursor: pointer;
  }
  .range button.active {
    background: rgb(var(--accent) / 0.16);
    color: rgb(var(--accent));
    border-color: rgb(var(--accent) / 0.4);
  }
  .chart {
    width: 100%;
    height: 150px;
    margin-top: 8px;
    display: block;
  }
  .grid {
    stroke: rgb(var(--border));
    stroke-width: 1;
    stroke-dasharray: 3 4;
  }
  .line {
    fill: none;
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  .line.five {
    stroke: rgb(var(--accent));
  }
  .line.week {
    stroke: rgb(var(--ok));
  }
  .area {
    stroke: none;
  }
  .area.five {
    fill: rgb(var(--accent) / 0.14);
  }
  .area.week {
    fill: rgb(var(--ok) / 0.12);
  }
  .legend {
    display: flex;
    gap: 16px;
    margin-top: 6px;
    font-size: 0.72rem;
    color: rgb(var(--fg-muted));
  }
  .key {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .sw {
    width: 11px;
    height: 3px;
    border-radius: 2px;
  }
  .sw.five {
    background: rgb(var(--accent));
  }
  .sw.week {
    background: rgb(var(--ok));
  }
  .nohist {
    padding: 26px 0;
    text-align: center;
    color: rgb(var(--fg-muted));
    font-size: 0.78rem;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .fc {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 5px 0;
    font-size: 0.78rem;
    border-bottom: 1px solid rgb(var(--border) / 0.6);
  }
  .fc:last-child {
    border-bottom: none;
  }
  .fc-label {
    color: rgb(var(--fg-muted));
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.76rem;
  }
  th {
    text-align: left;
    font-weight: 600;
    color: rgb(var(--fg-muted));
    padding: 4px 6px;
    border-bottom: 1px solid rgb(var(--border));
  }
  td {
    padding: 5px 6px;
    border-bottom: 1px solid rgb(var(--border) / 0.5);
  }
  .mono {
    font-variant-numeric: tabular-nums;
  }
  .empty {
    padding: 40px 0;
    text-align: center;
    color: rgb(var(--fg-muted));
  }
  @media (max-width: 560px) {
    .grid2 {
      grid-template-columns: 1fr;
    }
  }
</style>
