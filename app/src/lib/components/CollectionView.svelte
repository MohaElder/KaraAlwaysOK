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
  import { manage } from "$lib/state/manage.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import RenameInput from "./RenameInput.svelte";
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";
  import XIcon from "phosphor-svelte/lib/XIcon";
  import DotsSixVerticalIcon from "phosphor-svelte/lib/DotsSixVerticalIcon";

  const kinds: Record<Kind, { label: Key; icon: Icon }> = {
    playlist: { label: "kind.playlist", icon: PlaylistIcon },
    album: { label: "kind.album", icon: VinylRecordIcon },
    artist: { label: "kind.artist", icon: UserIcon },
  };
  const page = $derived(library.page);
  const tracks = $derived(library.visibleTracks);
  const length = $derived(minutes(tracks));
  const singable = $derived(adding.singable(tracks));
  const order = $derived(page?.tracks.map((x) => x.id) ?? []);
  /** The playlist whose delete question is showing. */
  let asking = $state<number | null>(null);
  let dragId = $state<number | null>(null);
  let over = $state<number | null>(null);

  /** The song id of the row under the pointer. */
  function rowAt(e: PointerEvent) {
    const row = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>(".rows [data-track]");
    return row ? Number(row.dataset.track) : null;
  }

  function drop(target: number | null) {
    const id = dragId;
    dragId = over = null;
    if (page && id != null && target != null && id !== target) void manage.move(page.card.id, id, order.indexOf(target));
  }
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === "Escape" && dragId != null) dragId = over = null;
}} />

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
          {#if ui.renaming?.place === "hero" && ui.renaming.id === page.card.id}
            <h1><RenameInput value={page.card.name} onDone={(name) => manage.finishRename(name)} /></h1>
          {:else}
            <h1>{cardName(page.card)}</h1>
          {/if}
          <p class="sub num">{#if page.card.subtitle}<span class="subtitle">{page.card.subtitle}</span> · {/if}{t("library.songs", { n: tracks.length })}{#if length} · {t("library.minutes", { n: length })}{/if}</p>
          <div class="actions">
            {#if asking === page.card.id}
              <div class="hstack confirm" in:fade>
                <WarningIcon /><span>{t("playlist.deleteAsk")}</span>
                <button class="btn" onclick={() => (asking = null)}><XIcon />{t("common.cancel")}</button>
                <button class="btn accent" onclick={() => { asking = null; void manage.deletePlaylist(page.card); }}><TrashIcon />{t("common.delete")}</button>
              </div>
            {:else}
              <button class="btn accent lg" disabled={!singable.length} onclick={() => player.playAll(singable, false)}><PlayIcon />{t("collection.sing")}</button>
              <button class="btn soft lg" disabled={!singable.length} onclick={() => player.playAll(singable, true)}><ShuffleIcon />{t("collection.shuffle")}</button>
              {#if page.card.user}
                <button class="ib big" use:tip={t("playlist.rename")} onclick={() => (ui.renaming = { id: page.card.id, place: "hero", isNew: false, withSongs: false })}><PencilSimpleIcon size={18} /></button>
                <button class="ib big" use:tip={t("playlist.delete")} onclick={() => (asking = page.card.id)}><TrashIcon size={18} /></button>
              {/if}
            {/if}
          </div>
        </div>
      </div>
      <div class="rows">
        {#each tracks as song (song.id)}
          {#snippet grip()}
            <span
              class="grip"
              aria-hidden="true"
              use:tip={t("queue.drag")}
              onclick={(e) => e.stopPropagation()}
              onpointerdown={(e) => {
                if (e.button !== 0) return;
                e.preventDefault();
                e.currentTarget.setPointerCapture(e.pointerId);
                dragId = song.id;
              }}
              onpointermove={(e) => {
                if (dragId != null) over = rowAt(e);
              }}
              onpointerup={(e) => drop(rowAt(e))}
              onpointercancel={() => (dragId = over = null)}
            ><DotsSixVerticalIcon size={14} /></span>
          {/snippet}
          <div
            class:dragging={dragId === song.id}
            class:over={dragId != null && over === song.id}
            class:below={dragId != null && order.indexOf(dragId) < order.indexOf(song.id)}
          >
            <SongRow
              track={adding.shown(song)}
              grip={page.card.user ? grip : undefined}
              busy={adding.ids.has(song.id)}
              playing={player.track?.id === song.id}
              flash={ui.flash === song.id}
              onTap={() => player.enqueue(song.id)}
              onNext={() => player.enqueue(song.id, true)}
              onMenu={(x, y, alignRight) => (ui.menu = { kind: "song", track: song, playlistId: page.card.user ? page.card.id : null, x, y, alignRight: !!alignRight })}
            />
          </div>
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
  .big { width: 40px; height: 40px; }
  .confirm { font-size: 13px; flex-wrap: wrap; }
  .confirm > :global(svg:first-child) { color: var(--accent); }
  .grip { color: var(--faint); cursor: grab; display: grid; touch-action: none; opacity: 0; transition: opacity var(--t) var(--ease); }
  .rows :global(.row:hover .grip), .dragging .grip { opacity: 1; }
  .dragging { opacity: .4; }
  .over { box-shadow: inset 0 2px 0 var(--accent); border-radius: var(--r-sm); }
  .over.below { box-shadow: inset 0 -2px 0 var(--accent); }
  .none-yet { padding: var(--s4) var(--s2); }
</style>
