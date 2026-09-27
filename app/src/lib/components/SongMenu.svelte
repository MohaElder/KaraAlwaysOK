<script lang="ts">
  import { ui } from "$lib/state/ui.svelte";
  import { player } from "$lib/state/player.svelte";
  import { adding } from "$lib/state/adding.svelte";
  import { manage } from "$lib/state/manage.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Menu from "./Menu.svelte";
  import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import TrashIcon from "phosphor-svelte/lib/TrashIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  const m = $derived(ui.menu?.kind === "song" ? ui.menu : null);
  let asking = $state(false);

  $effect(() => {
    if (m) asking = false;
  });

  const close = () => (ui.menu = null);
  const run = (action: () => unknown) => {
    close();
    void action();
  };
</script>

{#if m}
  {@const track = m.track}
  {@const busy = adding.ids.has(track.id)}
  <Menu x={m.x} y={m.y} alignRight={m.alignRight} onClose={close}>
    {#if asking}
      <div class="ask"><WarningIcon size={18} /><div class="grow"><b>{t("confirm.deleteSongTitle", { title: track.title })}</b><small>{t("confirm.deleteSongBody")}</small></div></div>
      <div class="hstack end">
        <button class="btn" onclick={() => (asking = false)}>{t("common.cancel")}</button>
        <button class="btn accent" onclick={() => run(() => manage.deleteSong(track))}><TrashIcon />{t("common.delete")}</button>
      </div>
    {:else}
      <button class="opt" disabled={busy} onclick={() => run(() => player.enqueue(track.id, true))}><ArrowBendDownRightIcon size={18} /><span class="grow">{t("song.playNext")}</span></button>
      <button class="opt" disabled={busy} onclick={() => run(() => player.enqueue(track.id))}><ListPlusIcon size={18} /><span class="grow">{t("song.addToQueue")}</span></button>
      {#if track.provider === "local"}
        <div class="msep"></div>
        <button class="opt" onclick={() => run(() => (ui.sheet = { kind: "edit", track }))}><PencilSimpleIcon size={18} /><span class="grow">{t("menu.editInfo")}</span></button>
        <button class="opt" onclick={() => (asking = true)}><TrashIcon size={18} /><span class="grow">{t("menu.deleteSong")}</span></button>
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
