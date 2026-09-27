<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import CollectionView from "$lib/components/CollectionView.svelte";
  import Empty from "$lib/components/Empty.svelte";
  import GettingReady from "$lib/components/GettingReady.svelte";
  import Tooltip from "$lib/components/Tooltip.svelte";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";
  import { t } from "$lib/i18n/index.svelte";
  import { engine } from "$lib/state/engine.svelte";
  import { library } from "$lib/state/library.svelte";

  onMount(() => {
    engine.start();
  });
</script>

<div class="app">
  <Sidebar />
  <main class="main">
    <header class="top"></header>
    <div class="view">
      {#if library.error}
        <Empty icon={WarningIcon} title={t("problem.libraryOpen")} />
      {:else if library.page}
        <CollectionView />
      {:else if library.loaded && library.kind === "playlist"}
        <Empty icon={MusicNotesIcon} title={t("library.emptyTitle")}>
          <p>{t("library.emptyBody")}</p>
        </Empty>
      {/if}
    </div>
  </main>
</div>
<GettingReady />
<Tooltip />

<style>
  .app { display: grid; grid-template-columns: var(--side-w) minmax(0, 1fr); height: 100%; }
  .main { position: relative; overflow: auto; min-height: 0; padding: 0 var(--s7) 140px; transition: padding-right var(--t) var(--ease); }
  .top { position: sticky; top: 0; z-index: 5; display: grid; grid-template-columns: 1fr minmax(0, 640px) 1fr; align-items: center; gap: var(--s4); height: 84px; margin: 0 calc(-1 * var(--s7)); padding: 0 var(--s7); }
</style>
