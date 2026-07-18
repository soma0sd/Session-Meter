<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { listen } from "@tauri-apps/api/event";
  import { t, locale } from "../lib/i18n";
  import { initWindow } from "../lib/appinit";
  import { applyTheme, type Theme } from "../lib/theme";
  import TitleBar from "../lib/TitleBar.svelte";
  import { getChangelog } from "../lib/ipc";
  import { changelog } from "../lib/changelog";

  type Entry = { version: string; date: string; lines: string[] };

  let entries = $state<Entry[]>([]);
  let loading = $state(true);
  let unlisteners: Array<() => void> = [];

  // Parse a Keep-a-Changelog markdown file into version entries.
  function parse(md: string): Entry[] {
    const out: Entry[] = [];
    for (const block of md.split(/\n## /)) {
      const nl = block.indexOf("\n");
      const header = (nl >= 0 ? block.slice(0, nl) : block).trim();
      const m = header.match(/\[?(\d+\.\d+(?:\.\d+)?)\]?\s*-\s*(.+)/);
      if (!m) continue;
      const lines = [...block.matchAll(/^[-*]\s+(.+)$/gm)].map((x) =>
        x[1].replace(/\*\*/g, "").replace(/`/g, "").trim(),
      );
      if (lines.length) out.push({ version: m[1], date: m[2].trim(), lines });
    }
    return out;
  }

  // Bundled changelog, used when the GitHub fetch fails (offline / repo not published yet).
  function fallback(loc: string): Entry[] {
    return changelog.map((e) => ({
      version: e.version,
      date: e.date,
      lines: loc === "ko" ? e.ko : e.en,
    }));
  }

  async function load() {
    loading = true;
    const loc = get(locale);
    try {
      const md = await getChangelog(loc);
      const parsed = parse(md);
      entries = parsed.length ? parsed : fallback(loc);
    } catch {
      entries = fallback(loc);
    }
    loading = false;
  }

  onMount(async () => {
    await initWindow();
    await load();
    try {
      unlisteners.push(
        await listen<string>("theme://changed", (e) => applyTheme(e.payload as Theme)),
      );
    } catch {
      /* preview */
    }
  });

  onDestroy(() => unlisteners.forEach((u) => u()));
</script>

<div class="win">
  <TitleBar title={$t("news.title")} />
  <main>
    {#if loading}
      <div class="empty">{$t("common.loading")}</div>
    {:else}
      {#each entries as e (e.version)}
        <section>
          <div class="ver">
            <span class="v">v{e.version}</span>
            <span class="d">{e.date}</span>
          </div>
          <ul>
            {#each e.lines as line}
              <li>{line}</li>
            {/each}
          </ul>
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
  }
  main {
    flex: 1;
    overflow-y: auto;
    padding: 14px 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .ver {
    display: flex;
    align-items: baseline;
    gap: 9px;
    border-bottom: 1px solid rgb(var(--border));
    padding-bottom: 5px;
  }
  .v {
    font-size: 0.98rem;
    font-weight: 700;
    color: rgb(var(--accent));
    font-variant-numeric: tabular-nums;
  }
  .d {
    font-size: 0.74rem;
    color: rgb(var(--fg-muted));
  }
  ul {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  li {
    font-size: 0.83rem;
    line-height: 1.5;
    color: rgb(var(--fg));
  }
  .empty {
    text-align: center;
    font-size: 0.8rem;
    color: rgb(var(--fg-muted));
    padding: 20px;
  }
</style>
