<script lang="ts">
  import type { Then } from "$lib/state/adding.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { fade, fadeAway } from "$lib/motion";
  import LinkRow from "./LinkRow.svelte";
  import LinkIcon from "phosphor-svelte/lib/LinkIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  let { onAdd }: { onAdd?: (url: string, then: Then, title: string) => void } = $props();
  const view = $derived(ui.search.kind === "link" ? ui.search : null);
</script>

{#if view}
  {#key view.url}
  <div in:fade|global out:fadeAway|global>
  <h2 class="sec"><LinkIcon size={18} />{t("search.fromLink")}</h2>
  {#if view.failed}
    <p class="hstack muted" in:fade><WarningIcon />{t("problem.noSongAtLink")}</p>
  {:else}
    <LinkRow preview={view.preview} host={view.host} onAdd={(then) => onAdd?.(view.url, then, view.preview?.title ?? "")} />
  {/if}
  </div>
  {/key}
{/if}

<style>
  .sec { display: flex; align-items: center; gap: var(--s2); margin: var(--s6) 0 var(--s2); font-size: 17px; font-weight: 600; }
  .sec :global(svg) { color: var(--muted); }
</style>
