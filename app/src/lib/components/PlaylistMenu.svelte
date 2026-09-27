<script lang="ts">
  import { openCollection } from "$lib/api";
  import { ui } from "$lib/state/ui.svelte";
  import { player } from "$lib/state/player.svelte";
  import { cardName, library } from "$lib/state/library.svelte";
  import { adding } from "$lib/state/adding.svelte";
  import { manage } from "$lib/state/manage.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Menu from "./Menu.svelte";
  import PlayIcon from "phosphor-svelte/lib/PlayIcon";
  import ShuffleIcon from "phosphor-svelte/lib/ShuffleIcon";
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  const m = $derived(ui.menu?.kind === "playlist" ? ui.menu : null);
  let asking = $state(false);
  /** The open playlist's songs that Sing and Shuffle would play. */
  let singable = $state<number[]>([]);

  $effect(() => {
    if (!m) return;
    const menu = m;
    asking = false;
    singable = [];
    void openCollection(menu.card.id).then((page) => {
      if (ui.menu === menu) singable = adding.singable(library.visible(page));
    });
  });

  const close = () => (ui.menu = null);

  function sing(shuffle: boolean) {
    close();
    void player.playAll(singable, shuffle);
  }

  async function rename(id: number) {
    close();
    ui.clearSearch();
    await library.select(id);
    ui.renaming = { id, place: "hero", isNew: false, withSongs: false };
  }
</script>

{#if m}
  {@const card = m.card}
  <Menu x={m.x} y={m.y} onClose={close}>
    {#if asking}
      <div class="ask"><WarningIcon size={18} /><div class="grow"><b>{t("confirm.deleteSongTitle", { title: cardName(card) })}</b><small>{t("confirm.deletePlaylistBody")}</small></div></div>
      <div class="hstack end">
        <button class="btn" onclick={() => (asking = false)}>{t("common.cancel")}</button>
        <button class="btn accent" onclick={() => { close(); void manage.deletePlaylist(card); }}><TrashIcon />{t("common.delete")}</button>
      </div>
    {:else}
      <button class="opt" disabled={!singable.length} onclick={() => sing(false)}><PlayIcon size={18} /><span class="grow">{t("collection.sing")}</span></button>
      <button class="opt" disabled={!singable.length} onclick={() => sing(true)}><ShuffleIcon size={18} /><span class="grow">{t("collection.shuffle")}</span></button>
      {#if card.user}
        <div class="msep"></div>
        <button class="opt" onclick={() => rename(card.id)}><PencilSimpleIcon size={18} /><span class="grow">{t("playlist.rename")}</span></button>
        <button class="opt" onclick={() => (asking = true)}><TrashIcon size={18} /><span class="grow">{t("playlist.delete")}</span></button>
      {/if}
    {/if}
  </Menu>
{/if}

<style>
  .ask { display: flex; gap: var(--s3); padding: var(--s2) var(--s2) var(--s3); }
  .ask > :global(svg) { color: var(--accent); margin-top: 2px; flex: none; }
  .ask b { display: block; font-weight: 600; }
  .ask small { display: block; color: var(--muted); font-size: 12.5px; }
  .end { justify-content: flex-end; padding: 0 var(--s1) var(--s1); }
</style>
