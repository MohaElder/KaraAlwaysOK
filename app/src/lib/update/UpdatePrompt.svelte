<script lang="ts">
  import { updater } from "./updater.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Sheet from "$lib/components/Sheet.svelte";
  import ArrowsClockwiseIcon from "phosphor-svelte/lib/ArrowsClockwiseIcon";
  import DownloadSimpleIcon from "phosphor-svelte/lib/DownloadSimpleIcon";
  import ClockIcon from "phosphor-svelte/lib/ClockIcon";
  import ProhibitIcon from "phosphor-svelte/lib/ProhibitIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";

  const s = $derived(updater.state);
</script>

{#if s.kind !== "idle"}
  <Sheet icon={ArrowsClockwiseIcon} title={t("update.title")} onClose={() => updater.dismiss()}>
    {#if s.kind === "available"}
      <p class="muted">{t("update.available", { version: s.version })}</p>
      {#if s.notes}<pre class="notes">{s.notes}</pre>{/if}
      <div class="sfoot">
        <button class="btn ghost" onclick={() => updater.skip()}><ProhibitIcon />{t("update.skip")}</button>
        <button class="btn" onclick={() => updater.dismiss()}><ClockIcon />{t("update.later")}</button>
        <button class="btn accent" onclick={() => updater.install()}><DownloadSimpleIcon />{t("update.install")}</button>
      </div>
    {:else if s.kind === "downloading"}
      <p class="muted">{t("update.downloading")}</p>
      <div class="meter"><i style:width="{Math.round(s.fraction * 100)}%"></i></div>
    {:else}
      <p class="muted">{t(s.kind === "upToDate" ? "update.upToDate" : "update.failed")}</p>
      <div class="sfoot"><button class="btn accent" onclick={() => updater.dismiss()}><CheckIcon />{t("update.ok")}</button></div>
    {/if}
  </Sheet>
{/if}

<style>
  .notes { max-height: 200px; overflow: auto; white-space: pre-wrap; margin: var(--s3) 0 0; padding: var(--s3); border-radius: var(--r-sm); background: var(--bg); border: 1px solid var(--line); font: 12.5px/1.5 var(--body); }
  .meter { height: 8px; margin-top: var(--s3); border-radius: 4px; background: var(--raised); overflow: hidden; }
  .meter i { display: block; height: 100%; border-radius: 4px; background: var(--accent); transition: width var(--t) var(--ease); }
  .sfoot { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: var(--s2); margin-top: var(--s5); }
</style>
