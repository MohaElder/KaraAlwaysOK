<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import CollectionView from "$lib/components/CollectionView.svelte";
  import Empty from "$lib/components/Empty.svelte";
  import GettingReady from "$lib/components/GettingReady.svelte";
  import Tooltip from "$lib/components/Tooltip.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import Karaoke from "$lib/components/Karaoke.svelte";
  import PlayerBar from "$lib/components/PlayerBar.svelte";
  import QueuePanel from "$lib/components/QueuePanel.svelte";
  import DropOverlay from "$lib/components/DropOverlay.svelte";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";
  import { t } from "$lib/i18n/index.svelte";
  import { engine } from "$lib/state/engine.svelte";
  import { library } from "$lib/state/library.svelte";
  import { player } from "$lib/state/player.svelte";
  import { adding } from "$lib/state/adding.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import SearchBar from "$lib/components/SearchBar.svelte";
  import SearchResults from "$lib/components/SearchResults.svelte";
  import LinkResult from "$lib/components/LinkResult.svelte";
  import type { Kind } from "$lib/api";

  let searchBar: SearchBar | undefined = $state();

  function openFromSearch(kind: Kind, id: number) {
    searchBar?.clear();
    void library.show(kind, id);
  }

  onMount(() => {
    engine.start();
    player.init();
    adding.init();
  });
</script>

<div class="app">
  <Sidebar />
  <main class="main" class:q-open={ui.queueOpen}>
    <header class="top"><SearchBar bind:this={searchBar} onSubmitLink={(url, title) => adding.link(url, "", title)} /></header>
    <div class="view">
      {#if ui.search.kind === "text"}
        <SearchResults onOpen={openFromSearch} />
      {:else if ui.search.kind === "link"}
        <LinkResult onAdd={(url, then, title) => { ui.clearSearch(); void adding.link(url, then, title); }} />
      {:else if library.error}
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
<Karaoke />
<PlayerBar />
<QueuePanel />
<GettingReady />
<DropOverlay />
<Toasts />
<Tooltip />

<style>
  .app { display: grid; grid-template-columns: var(--side-w) minmax(0, 1fr); height: 100%; }
  .main { position: relative; overflow: auto; min-height: 0; padding: 0 var(--s7) 140px; transition: padding-right var(--t) var(--ease); }
  .main.q-open { padding-right: 388px; }
  .top { position: sticky; top: 0; z-index: 5; display: grid; grid-template-columns: 1fr minmax(0, 640px) 1fr; align-items: center; gap: var(--s4); height: 84px; margin: 0 calc(-1 * var(--s7)); padding: 0 var(--s7); }
</style>
