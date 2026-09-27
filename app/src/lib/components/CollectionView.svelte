<script lang="ts">
  import type { Kind } from "$lib/api";
  import type { Icon } from "$lib/icons";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { cardName, library } from "$lib/state/library.svelte";
  import { minutes } from "$lib/format";
  import { fade } from "$lib/motion";
  import Cover from "./Cover.svelte";
  import SongRow from "./SongRow.svelte";
  import PlaylistIcon from "phosphor-svelte/lib/PlaylistIcon";
  import VinylRecordIcon from "phosphor-svelte/lib/VinylRecordIcon";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { adding } from "$lib/state/adding.svelte";
  import PlayIcon from "phosphor-svelte/lib/PlayIcon";
  import ShuffleIcon from "phosphor-svelte/lib/ShuffleIcon";

  const kinds: Record<Kind, { label: Key; icon: Icon }> = {
    playlist: { label: "kind.playlist", icon: PlaylistIcon },
    album: { label: "kind.album", icon: VinylRecordIcon },
    artist: { label: "kind.artist", icon: UserIcon },
  };
  const page = $derived(library.page);
  const tracks = $derived(library.visibleTracks);
  const length = $derived(minutes(tracks));
  const singable = $derived(adding.singable(tracks));
</script>

{#if page}
  {@const kind = kinds[page.card.kind]}
  {#key page.card.id}
    <section in:fade>
      <div class="hero">
        {#if !page.card.covers.length}
          <div class="cover none"><PlaylistIcon size={56} /></div>
        {:else}
          <div class="cover" class:round={page.card.kind === "artist"}><Cover covers={page.card.covers} size="100%" grid={page.card.kind === "playlist"} /></div>
        {/if}
        <div class="grow">
          <div class="cap hstack"><kind.icon size={14} />{t(kind.label)}</div>
          <h1>{cardName(page.card)}</h1>
          <p class="sub num">{#if page.card.subtitle}<span class="subtitle">{page.card.subtitle}</span> · {/if}{t("library.songs", { n: tracks.length })}{#if length} · {t("library.minutes", { n: length })}{/if}</p>
          <div class="actions">
            <button class="btn accent lg" disabled={!singable.length} onclick={() => player.playAll(singable, false)}><PlayIcon />{t("collection.sing")}</button>
            <button class="btn soft lg" disabled={!singable.length} onclick={() => player.playAll(singable, true)}><ShuffleIcon />{t("collection.shuffle")}</button>
          </div>
        </div>
      </div>
      <div class="rows">
        {#each tracks as song (song.id)}
          <SongRow track={adding.shown(song)} busy={adding.ids.has(song.id)} playing={player.track?.id === song.id} flash={ui.flash === song.id} onTap={() => player.enqueue(song.id)} onNext={() => player.enqueue(song.id, true)} />
        {:else}
          <p class="hstack muted none-yet"><PlaylistIcon />{t("library.emptyPlaylist")}</p>
        {/each}
      </div>
    </section>
  {/key}
{/if}

<style>
  .hero { display: flex; gap: var(--s7); align-items: flex-end; padding: var(--s2) 0 var(--s6); }
  .cover { width: 184px; aspect-ratio: 1; flex: none; border-radius: var(--r-sm); overflow: hidden; display: grid; box-shadow: 0 18px 40px -22px var(--shadow); }
  .cover.round { border-radius: 50%; }
  .cover.none { place-items: center; background: var(--raised); color: var(--faint); }
  h1 { font: 800 28px/1.1 var(--display); letter-spacing: -.02em; margin: var(--s2) 0 var(--s1); text-wrap: balance; }
  .sub { margin-bottom: var(--s5); }
  .subtitle { font-family: var(--body); font-size: 14px; }
  .actions { display: flex; gap: var(--s3); flex-wrap: wrap; align-items: center; min-height: 40px; }
  .none-yet { padding: var(--s4) var(--s2); }
</style>
