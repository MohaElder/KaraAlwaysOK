<script lang="ts">
  import type { Kind } from "$lib/api";
  import type { Icon } from "$lib/icons";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { player } from "$lib/state/player.svelte";
  import { cardName } from "$lib/state/library.svelte";
  import { fade } from "$lib/motion";
  import Cover from "./Cover.svelte";
  import Empty from "./Empty.svelte";
  import SongRow from "./SongRow.svelte";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import PlaylistIcon from "phosphor-svelte/lib/PlaylistIcon";
  import VinylRecordIcon from "phosphor-svelte/lib/VinylRecordIcon";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import MagnifyingGlassMinusIcon from "phosphor-svelte/lib/MagnifyingGlassMinusIcon";

  let { onOpen }: { onOpen: (kind: Kind, id: number) => void } = $props();
  const GROUPS: { kind: Kind; label: Key; icon: Icon }[] = [
    { kind: "playlist", label: "kind.playlists", icon: PlaylistIcon },
    { kind: "album", label: "kind.albums", icon: VinylRecordIcon },
    { kind: "artist", label: "kind.artists", icon: UserIcon },
  ];
  const view = $derived(ui.search.kind === "text" ? ui.search : null);
  const groups = $derived(view ? GROUPS.map((g) => ({ ...g, cards: view.collections.filter((c) => c.kind === g.kind) })).filter((g) => g.cards.length) : []);
</script>

{#if view}
  {#key view.query}
  <div in:fade>
  {#if !view.tracks.length && !groups.length}
    <Empty icon={MagnifyingGlassMinusIcon} title={t("search.noMatchTitle", { query: view.query })}><p>{t("search.noMatchBody")}</p></Empty>
  {:else}
    {#if view.tracks.length}
      <h2 class="sec"><MusicNotesIcon size={18} />{t("search.songs")}</h2>
      {#each view.tracks.slice(0, 8) as song (song.id)}
        <SongRow track={song} playing={player.track?.id === song.id} onTap={() => player.enqueue(song.id)} onNext={() => player.enqueue(song.id, true)} />
      {/each}
    {/if}
    {#each groups as g (g.kind)}
      <h2 class="sec"><g.icon size={18} />{t(g.label)}</h2>
      <div class="cards">
        {#each g.cards as c (c.id)}
          <button class="card" onclick={() => onOpen(c.kind, c.id)}>
            <span class="cv" class:round={c.kind === "artist"}><Cover covers={c.covers} size="100%" grid={c.kind === "playlist"} /></span>
            <b class="ell">{cardName(c)}</b>
          </button>
        {/each}
      </div>
    {/each}
  {/if}
  </div>
  {/key}
{/if}

<style>
  .sec { display: flex; align-items: center; gap: var(--s2); margin: var(--s6) 0 var(--s2); font-size: 17px; font-weight: 600; }
  .sec :global(svg) { color: var(--muted); }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(132px, 1fr)); gap: var(--s4); }
  .card { display: grid; gap: var(--s2); text-align: left; padding: var(--s2); margin: calc(-1 * var(--s2)); border-radius: var(--r-lg); }
  .cv { display: grid; aspect-ratio: 1; border-radius: var(--r-sm); overflow: hidden; }
  .cv.round { border-radius: 50%; }
  .card:hover { background: var(--surface); }
  .card b { font-weight: 500; }
</style>
