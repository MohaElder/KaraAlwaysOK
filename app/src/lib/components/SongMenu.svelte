<script lang="ts">
  import { listCollections, playlistsWith, type CollectionCard } from "$lib/api";
  import { ui } from "$lib/state/ui.svelte";
  import { player } from "$lib/state/player.svelte";
  import { adding } from "$lib/state/adding.svelte";
  import { library } from "$lib/state/library.svelte";
  import { manage } from "$lib/state/manage.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Menu from "./Menu.svelte";
  import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
  import PlaylistIcon from "phosphor-svelte/lib/PlaylistIcon";
  import CaretRightIcon from "phosphor-svelte/lib/CaretRightIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import MinusCircleIcon from "phosphor-svelte/lib/MinusCircleIcon";
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import QuotesIcon from "phosphor-svelte/lib/QuotesIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";

  const m = $derived(ui.menu?.kind === "song" ? ui.menu : null);
  let last: NonNullable<typeof m> | null = null;
  /** The open menu, kept while it closes so its contents never read a menu that is gone. */
  const shown = $derived.by(() => (last = m ?? last));
  let asking = $state(false);
  let sub = $state<{ x: number; y: number } | null>(null);
  let playlists = $state<CollectionCard[]>([]);
  let holding = $state<number[]>([]);

  $effect(() => {
    if (!m) return;
    const menu = m;
    asking = false;
    sub = null;
    holding = [];
    void listCollections("playlist").then((cs) => (playlists = cs.filter((c) => c.user && !library.hidden.has(`playlist:${c.id}`))));
    void playlistsWith(menu.track.id).then((ids) => {
      if (ui.menu === menu) holding = ids;
    });
  });

  const close = () => (ui.menu = null);
  const run = (action: () => unknown) => {
    void action();
    close();
  };
  const hideSub = () => (sub = null);

  /** Opens the playlist list beside the menu, on the side with room. */
  function openSub(e: Event) {
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const c = (e.currentTarget as HTMLElement).closest(".menu")!.getBoundingClientRect();
    sub = { x: c.right + 4 + c.width > innerWidth ? c.left - 4 - c.width : c.right + 4, y: b.top - 8 };
  }
</script>

{#if m}
  {@const track = shown!.track}
  {@const playlistId = shown!.playlistId}
  {@const busy = adding.ids.has(track.id)}
  <Menu x={m.x} y={m.y} alignRight={m.alignRight} onClose={close}>
    {#if asking}
      <div class="ask"><WarningIcon size={18} /><div class="grow"><b>{t("confirm.deleteSongTitle", { title: track.title })}</b><small>{t("confirm.deleteSongBody")}</small></div></div>
      <div class="hstack end">
        <button class="btn" onclick={() => (asking = false)}>{t("common.cancel")}</button>
        <button class="btn accent" onclick={() => run(() => manage.deleteSong(track))}><TrashIcon />{t("common.delete")}</button>
      </div>
    {:else}
      <button class="opt" disabled={busy} onpointerenter={hideSub} onclick={() => run(() => player.enqueue(track.id, true))}><ArrowBendDownRightIcon size={18} /><span class="grow">{t("song.playNext")}</span></button>
      <button class="opt" disabled={busy} onpointerenter={hideSub} onclick={() => run(() => player.enqueue(track.id))}><ListPlusIcon size={18} /><span class="grow">{t("song.addToQueue")}</span></button>
      <div class="msep"></div>
      <button class="opt" onpointerenter={openSub} onclick={openSub}><PlaylistIcon size={18} /><span class="grow">{t("menu.addToPlaylist")}</span><CaretRightIcon size={14} /></button>
      {#if playlistId != null}
        <button class="opt" onpointerenter={hideSub} onclick={() => run(() => manage.removeFromPlaylist(playlistId, track))}><MinusCircleIcon size={18} /><span class="grow">{t("menu.removeFromPlaylist")}</span></button>
      {/if}
      <div class="msep"></div>
      <button class="opt" onpointerenter={hideSub} onclick={() => run(() => manage.findLyrics(track))}><QuotesIcon size={18} /><span class="grow">{t("menu.findLyrics")}</span></button>
      {#if track.provider === "local"}
        <button class="opt" onpointerenter={hideSub} onclick={() => run(() => (ui.sheet = { kind: "edit", track }))}><PencilSimpleIcon size={18} /><span class="grow">{t("menu.editInfo")}</span></button>
        <button class="opt" disabled={busy} onpointerenter={hideSub} onclick={() => (asking = true)}><TrashIcon size={18} /><span class="grow">{t("menu.deleteSong")}</span></button>
      {/if}
    {/if}
  </Menu>
  {#if sub && !asking}
    <Menu x={sub.x} y={sub.y} onClose={close}>
      {#each playlists as p (p.id)}
        <button class="opt" onclick={() => run(() => manage.addToPlaylist(p, track))}>
          <PlaylistIcon size={18} /><span class="grow ell">{p.name}</span>{#if holding.includes(p.id)}<CheckIcon size={14} />{/if}
        </button>
      {/each}
      {#if playlists.length}<div class="msep"></div>{/if}
      <button class="opt" onclick={() => run(() => manage.newPlaylist([track.id]))}><PlusIcon size={18} /><span class="grow">{t("menu.newPlaylistDots")}</span></button>
    </Menu>
  {/if}
{/if}

<style>
  .ask { display: flex; gap: var(--s3); padding: var(--s2) var(--s2) var(--s3); }
  .ask > :global(svg) { color: var(--accent); margin-top: 2px; flex: none; }
  .ask b { display: block; font-weight: 600; }
  .ask small { display: block; color: var(--muted); font-size: 12.5px; }
  .end { justify-content: flex-end; padding: 0 var(--s1) var(--s1); }
</style>
