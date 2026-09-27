<script lang="ts">
  import type { Kind } from "$lib/api";
  import type { Icon } from "$lib/icons";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { player } from "$lib/state/player.svelte";
  import { adding, type Then } from "$lib/state/adding.svelte";
  import { cardName, library } from "$lib/state/library.svelte";
  import { fade, fadeAway } from "$lib/motion";
  import Cover from "./Cover.svelte";
  import Empty from "./Empty.svelte";
  import SongRow from "./SongRow.svelte";
  import LinkRow from "./LinkRow.svelte";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import PlaylistIcon from "phosphor-svelte/lib/PlaylistIcon";
  import VinylRecordIcon from "phosphor-svelte/lib/VinylRecordIcon";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import MagnifyingGlassMinusIcon from "phosphor-svelte/lib/MagnifyingGlassMinusIcon";
  import YoutubeLogoIcon from "phosphor-svelte/lib/YoutubeLogoIcon";

  let { onOpen, onAdd }: { onOpen: (kind: Kind, id: number) => void; onAdd: (url: string, then: Then, title: string) => void } = $props();
  const GROUPS: { kind: Kind; label: Key; icon: Icon }[] = [
    { kind: "playlist", label: "kind.playlists", icon: PlaylistIcon },
    { kind: "album", label: "kind.albums", icon: VinylRecordIcon },
    { kind: "artist", label: "kind.artists", icon: UserIcon },
  ];
  const view = $derived(ui.search.kind === "text" ? ui.search : null);
  const tracks = $derived(view ? view.tracks.filter((x) => !library.hidden.has(`track:${x.id}`)) : []);
  const groups = $derived(view ? GROUPS.map((g) => ({ ...g, cards: view.collections.filter((c) => c.kind === g.kind) })).filter((g) => g.cards.length) : []);
</script>

{#if view}
  {#key view.query}
  <div in:fade|global out:fadeAway|global>
  {#if !tracks.length && !groups.length && view.youtube?.length === 0}
    <Empty icon={MagnifyingGlassMinusIcon} title={t("search.noMatchTitle", { query: view.query })}><p>{t("search.noMatchBody")}</p></Empty>
  {:else}
    {#if tracks.length}
      <h2 class="sec"><MusicNotesIcon size={18} />{t("search.songs")}</h2>
      {#each tracks.slice(0, 8) as song (song.id)}
        <SongRow track={adding.shown(song)} busy={adding.ids.has(song.id)} playing={player.track?.id === song.id} onTap={() => player.enqueue(song.id)} onNext={() => player.enqueue(song.id, true)} onMenu={(x, y, alignRight) => (ui.menu = { kind: "song", track: song, playlistId: null, x, y, alignRight: !!alignRight })} />
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
  {#if view.youtube?.length !== 0}
    <section transition:fade>
      <h2 class="sec"><YoutubeLogoIcon size={18} />{t("search.youtube")}</h2>
      {#each view.youtube ?? [null, null, null] as hit, i (hit?.url ?? i)}
        <LinkRow preview={hit} onAdd={(then) => hit && onAdd(hit.url, then, hit.title)} />
      {/each}
    </section>
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
