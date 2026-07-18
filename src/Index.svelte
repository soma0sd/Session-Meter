<script lang="ts">
  // The root entry is not shown as a window in normal use; SessionMeter lives in the
  // tray plus its dedicated widget/stats/settings windows. Kept as a valid MPA entry.
  import { t } from "./lib/i18n";
</script>

<main>
  <h1>{$t("app.name")}</h1>
  <p>{$t("common.loading")}</p>
</main>

<style>
  main {
    display: grid;
    place-content: center;
    height: 100%;
    text-align: center;
    color: rgb(var(--fg-muted));
  }
  h1 {
    margin: 0 0 0.25rem;
    font-size: 1.1rem;
    color: rgb(var(--fg));
  }
  p {
    margin: 0;
    font-size: 0.85rem;
  }
</style>
