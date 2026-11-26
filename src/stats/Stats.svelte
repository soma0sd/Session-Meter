<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t, locale, localeTags } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import TitleBar from "../lib/TitleBar.svelte";
  import {
    getUsage,
    getHistory,
    getServicesStatus,
    type UsageSnapshot,
    type HistoryPoint,
    type Bucket,
    type ServiceStatus,
  } from "../lib/ipc";
  import { formatCountdown, formatResetDateTime } from "../lib/countdown";
  import { linePathT, areaPathT, mapY, thinSeriesT, xAtTime } from "../lib/charts";
  import { forecast, type Forecast } from "../lib/forecast";

  const CW = 620;
  const CH = 150;
  const CHART_PAD = 3;
  const yTicks = [100, 75, 50, 25, 0];
  type HistoryRange = "24h" | "7d" | "30d";

  let snap = $state<UsageSnapshot | null>(null);
  let history = $state<HistoryPoint[]>([]);
  let range = $state<HistoryRange>("24h");
  let chartEnd = $state(Date.now());
  let historyLoading = $state(false);
  let selectedIndex = $state<number | null>(null);
  let now = $state(Date.now());
  let services = $state<ServiceStatus[]>([]);
  let activeService = $state("claude");

  let timer: number | undefined;
  let unlisteners: Array<() => void> = [];
  let historyRequest = 0;
  let serviceRequest = 0;

  const loggedIn = $derived(services.filter((x) => x.logged_in));

  async function loadHistory() {
    const request = ++historyRequest;
    const end = Date.now();
    try {
      const points = await getHistory(range, activeService);
      if (request !== historyRequest) return;
      history = points;
      chartEnd = end;
    } catch {
      if (request === historyRequest) history = [];
    } finally {
      if (request === historyRequest) historyLoading = false;
    }
  }

  // The poller supplies new usage. Tab changes only ask for the locally available data.
  async function loadService() {
    const request = ++serviceRequest;
    const id = activeService;
    const previous = snap;
    historyLoading = true;
    const historyRead = loadHistory();
    try {
      const cached = await getUsage(id);
      if (request === serviceRequest && snap === previous) snap = cached;
    } catch {
      /* preview */
    }
    await historyRead;
  }

  async function switchService(id: string) {
    if (id === activeService) return;
    activeService = id;
    snap = null;
    history = [];
    selectedIndex = null;
    await loadService();
  }

  // Re-read the logged-in service list (on focus and on sign-in/out) so a service that signed in
  // elsewhere - or that was missed by an early startup fetch (the stats webview can load before
  // the app state is ready, unlike the style window it did not re-fetch) - appears as a tab. The
  // active tab keeps its data; only switch if the current service is no longer logged in.
  async function refreshServices() {
    let next: ServiceStatus[];
    try {
      next = await getServicesStatus();
    } catch {
      return;
    }
    services = next;
    const li = services.filter((x) => x.logged_in);
    if (li.length && !li.some((x) => x.id === activeService)) {
      await switchService(li[0].id);
    }
  }

  onMount(async () => {
    void initWindow();
    void refreshServices();
    void loadService();
    try {
      unlisteners.push(
        await listen<UsageSnapshot>("usage://updated", (e) => {
          if (e.payload.service_id === activeService) {
            snap = e.payload;
            void loadHistory();
          }
        }),
      );
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

  function setRange(r: HistoryRange) {
    if (range === r) return;
    range = r;
    history = [];
    historyLoading = true;
    selectedIndex = null;
    void loadHistory();
  }

  const localeTag = $derived(localeTags[$locale]);
  function fmtTime(iso: string): string {
    return formatResetDateTime(iso, localeTag, $t("common.today"), $t("common.tomorrow")) || "-";
  }

  const samples = $derived(history
    .map((point) => ({ ...point, time: Date.parse(point.at) }))
    .filter((point) => Number.isFinite(point.time))
    .sort((a, b) => a.time - b.time));

  function series(key: "five_hour" | "weekly") {
    const times: number[] = [];
    const values: number[] = [];
    for (const point of samples) {
      const value = point[key];
      if (value == null) continue;
      times.push(point.time);
      values.push(value);
    }
    return { times, values };
  }
  const primarySeries = $derived(series("five_hour"));
  const secondarySeries = $derived(series("weekly"));
  const secondary = $derived.by(() => {
    const key = snap?.secondary_key;
    if (!snap || !key) return null;
    return snap.buckets.find((bucket) => bucket.key === key) ?? snap.weekly_primary;
  });
  const hasSecondary = $derived(secondary !== null);
  const visibleSamples = $derived(samples.filter((p) => p.five_hour != null || (hasSecondary && p.weekly != null)));
  const activeIndex = $derived(Math.min(selectedIndex ?? visibleSamples.length - 1, visibleSamples.length - 1));
  const activeSample = $derived(visibleSamples[activeIndex]);
  const hasHistory = $derived(visibleSamples.length > 0);

  // Chart time window: points are placed by real timestamp within [winStart, now], so each
  // range renders at its true scale (24h vs 7d vs 30d look different even with the same data).
  const rangeMs = $derived(
    range === "24h" ? 24 * 3_600_000 : range === "7d" ? 7 * 86_400_000 : 30 * 86_400_000,
  );
  const winEnd = $derived(chartEnd);
  const winStart = $derived(winEnd - rangeMs);
  const primaryPlot = $derived(thinSeriesT(primarySeries.times, primarySeries.values, winStart, winEnd, CW));
  const secondaryPlot = $derived(thinSeriesT(secondarySeries.times, secondarySeries.values, winStart, winEnd, CW));
  const activeX = $derived(activeSample ? xAtTime(activeSample.time, winStart, winEnd, CW, CHART_PAD) : 0);
  const primaryLabel = $derived(windowLabel(snap?.primary_key, "five_hour"));
  const secondaryLabel = $derived(windowLabel(snap?.secondary_key, "seven_day"));
  const sampleDate = $derived(activeSample ? new Intl.DateTimeFormat(localeTag, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(activeSample.time) : "");
  const sampleDescription = $derived([
    sampleDate,
    activeSample?.five_hour == null ? "" : `${primaryLabel} ${activeSample.five_hour}%`,
    !hasSecondary || activeSample?.weekly == null ? "" : `${secondaryLabel} ${activeSample.weekly}%`,
  ].filter(Boolean).join(", "));

  function selectPoint(event: PointerEvent) {
    const rect = (event.currentTarget as SVGElement).getBoundingClientRect();
    if (rect.width <= 0 || !visibleSamples.length) return;
    const x = (event.clientX - rect.left) * CW / rect.width;
    const fraction = Math.max(0, Math.min(1, (x - CHART_PAD) / (CW - 2 * CHART_PAD)));
    const time = winStart + fraction * (winEnd - winStart);
    let first = 0;
    let last = visibleSamples.length - 1;
    while (first < last) {
      const middle = Math.floor((first + last) / 2);
      if (visibleSamples[middle].time < time) first = middle + 1;
      else last = middle;
    }
    selectedIndex = first > 0 && time - visibleSamples[first - 1].time <= visibleSamples[first].time - time
      ? first - 1
      : first;
  }

  function fc(data: ReturnType<typeof series>, bucket: { remaining: number } | null): Forecast {
    if (!bucket) return { depleting: false, minutesToEmpty: null };
    return forecast(data.times, data.values, bucket.remaining);
  }
  const fiveFc = $derived(fc(primarySeries, snap?.five_hour ?? null));
  const weekFc = $derived(
    fc(secondarySeries, secondary),
  );

  function fcText(f: Forecast, resetsAt: string | undefined): string {
    if (!f.depleting || f.minutesToEmpty == null) return $t("stats.forecast.stable");
    const time = formatCountdown(f.minutesToEmpty * 60000, $locale);
    const base = $t("stats.forecast.empties", { time });
    const resetMs = resetsAt ? Math.max(0, Date.parse(resetsAt) - now) : 0;
    return f.minutesToEmpty * 60000 < resetMs
      ? `${base} (${$t("stats.forecast.beforeReset")})`
      : $t("stats.forecast.safe");
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

  function windowLabel(key: string | null | undefined, fallbackKey: string): string {
    const resolved = key ?? fallbackKey;
    const bucket = snap?.buckets.find((b) => b.key === resolved);
    if (bucket) return bucketLabel(bucket);
    const localized = $t("bucket." + resolved);
    return localized.startsWith("bucket.") ? resolved : localized;
  }

  function activeServiceName(): string {
    return services.find((service) => service.id === activeService)?.name ?? activeService;
  }

  // Time axis across the top of the history chart. 24h shows clock time; wider ranges show
  // the date. Ticks are evenly spaced across the [winStart, winEnd] window that the chart
  // itself uses, so labels line up with the time-positioned points.
  function fmtAxis(ms: number): string {
    const d = new Date(ms);
    if (range === "24h") return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
    return `${d.getMonth() + 1}/${d.getDate()}`;
  }
  const axisTicks = $derived.by(() => {
    if (!hasHistory || !(winEnd > winStart)) return [] as { frac: number; label: string }[];
    const N = 4;
    return Array.from({ length: N }, (_, i) => {
      const frac = i / (N - 1);
      return { frac, label: fmtAxis(winStart + frac * (winEnd - winStart)) };
    });
  });
</script>

<div class="win">
  <TitleBar title={$t("stats.title")} />
  <main>
  {#if loggedIn.length > 1}
    <div class="tabs" role="tablist">
      {#each loggedIn as svc (svc.id)}
        <button
          class="tab"
          class:active={activeService === svc.id}
          role="tab"
          aria-selected={activeService === svc.id}
          onclick={() => switchService(svc.id)}>{svc.name}</button>
      {/each}
    </div>
  {/if}

  {#if snap && (snap.organization_name || snap.account_email || snap.subscription)}
    <div class="org">
      <span class="org-name">{snap.organization_name || activeServiceName()}</span>
      {#if snap.subscription}
        <span class="org-sub">{snap.subscription}</span>
      {/if}
      {#if snap.account_email}
        <span class="org-email">{snap.account_email}</span>
      {/if}
    </div>
  {/if}

  {#if snap === null}
    <div class="empty">{$t("common.loading")}</div>
  {:else if snap.status !== "ok"}
    <div class="empty">
      {snap.status === "unauthorized"
        ? $t("common.sessionExpired")
        : snap.status === "not_running"
          ? $t("common.antigravityNotRunning")
          : snap.status === "error"
            ? $t("common.usageUnavailable")
            : $t("common.notLoggedIn")}
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
          {#each (["24h", "7d", "30d"] as const) as period}
            <button class:active={range === period} aria-pressed={range === period} onclick={() => setRange(period)}>{period}</button>
          {/each}
        </div>
      </div>
      {#if hasHistory}
        <div class="axis-label">{$t("stats.historyRemaining")}</div>
        {#if axisTicks.length}
          <div class="axis">
            {#each axisTicks as tk (tk.frac)}
              <span
                class="tick"
                style="left:{tk.frac * 100}%; transform:translateX({tk.frac === 0
                  ? '0'
                  : tk.frac === 1
                    ? '-100%'
                    : '-50%'})">{tk.label}</span>
            {/each}
          </div>
        {/if}
        <div class="plot">
          <div class="y-axis" aria-hidden="true">
            {#each yTicks as value}
              <span style:top={`${mapY(value, CH, CHART_PAD)}px`}>{value}</span>
            {/each}
          </div>
          <svg class="chart" viewBox={`0 0 ${CW} ${CH}`} preserveAspectRatio="none" role="img"
            aria-label={$t("stats.historyRemaining")} onpointermove={selectPoint} onpointerdown={selectPoint}>
            {#each yTicks as g}
              <line x1="0" x2={CW} y1={mapY(g, CH, CHART_PAD)} y2={mapY(g, CH, CHART_PAD)} class="grid" />
            {/each}
            {#if hasSecondary && secondaryPlot.values.length >= 2}
              <path d={areaPathT(secondaryPlot.times, secondaryPlot.values, winStart, winEnd, CW, CH, CHART_PAD)} class="area week" />
              <path d={linePathT(secondaryPlot.times, secondaryPlot.values, winStart, winEnd, CW, CH, CHART_PAD)} class="line week" />
            {/if}
            {#if primaryPlot.values.length >= 2}
              <path d={areaPathT(primaryPlot.times, primaryPlot.values, winStart, winEnd, CW, CH, CHART_PAD)} class="area five" />
              <path d={linePathT(primaryPlot.times, primaryPlot.values, winStart, winEnd, CW, CH, CHART_PAD)} class="line five" />
            {/if}
            {#if activeSample}
              <line class="sample-marker" x1={activeX} x2={activeX} y1="0" y2={CH} />
              {#if activeSample.five_hour != null}
                <circle class="sample-point five" cx={activeX} cy={mapY(activeSample.five_hour, CH, CHART_PAD)} r="3" />
              {/if}
              {#if hasSecondary && activeSample.weekly != null}
                <circle class="sample-point week" cx={activeX} cy={mapY(activeSample.weekly, CH, CHART_PAD)} r="3" />
              {/if}
            {/if}
          </svg>
        </div>
        {#if visibleSamples.length > 1}
          <input class="sample-slider" type="range" min="0" max={visibleSamples.length - 1} value={activeIndex}
            aria-label={$t("stats.historySample")}
            aria-valuetext={sampleDescription}
            oninput={(event) => (selectedIndex = Number(event.currentTarget.value))} />
        {/if}
        <div class="legend">
          <time datetime={activeSample?.at}>{sampleDate}</time>
          <span class="key"><i class="sw five"></i>{primaryLabel} <strong>{activeSample?.five_hour == null ? "·" : `${activeSample.five_hour}%`}</strong></span>
          {#if hasSecondary}
            <span class="key"><i class="sw week"></i>{secondaryLabel} <strong>{activeSample?.weekly == null ? "·" : `${activeSample.weekly}%`}</strong></span>
          {/if}
        </div>
      {:else}
        <div class="nohist">{$t(historyLoading ? "common.loading" : "stats.noHistory")}</div>
      {/if}
    </section>

    <section class="grid2">
      <div class="panel">
        <h2>{$t("stats.forecast")}</h2>
        <div class="fc"><span class="fc-label">{primaryLabel}</span><span>{fcText(fiveFc, snap.five_hour?.resets_at)}</span></div>
        {#if hasSecondary}
          <div class="fc"><span class="fc-label">{secondaryLabel}</span><span>{fcText(weekFc, secondary?.resets_at)}</span></div>
        {/if}
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
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid rgb(var(--border));
    margin-bottom: 2px;
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
  .org-sub {
    font-size: 0.68rem;
    font-weight: 600;
    padding: 1px 8px;
    border-radius: 999px;
    background: rgb(var(--accent) / 0.16);
    color: rgb(var(--accent));
  }
  .axis {
    margin-inline-start: 26px;
    position: relative;
    height: 13px;
    margin-top: 6px;
    font-size: 0.64rem;
    color: rgb(var(--fg-muted));
    font-variant-numeric: tabular-nums;
  }
  .tick {
    position: absolute;
    top: 0;
    white-space: nowrap;
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
  .axis-label {
    color: rgb(var(--fg-muted));
    font-size: 0.72rem;
    margin-top: 8px;
  }
  .plot {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr);
    margin-top: 8px;
  }
  .y-axis {
    position: relative;
    color: rgb(var(--fg-muted));
    font-size: 0.65rem;
    font-variant-numeric: tabular-nums;
  }
  .y-axis span {
    position: absolute;
    right: 6px;
    transform: translateY(-50%);
  }
  .chart {
    width: 100%;
    height: 150px;
    display: block;
    cursor: crosshair;
  }
  .sample-marker {
    stroke: rgb(var(--fg-muted));
    vector-effect: non-scaling-stroke;
  }
  .sample-point.five {
    fill: rgb(var(--accent));
  }
  .sample-point.week {
    fill: rgb(var(--ok));
  }
  .sample-slider {
    display: block;
    width: calc(100% - 26px);
    margin: 3px 0 0 auto;
    accent-color: rgb(var(--accent));
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
    flex-wrap: wrap;
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
