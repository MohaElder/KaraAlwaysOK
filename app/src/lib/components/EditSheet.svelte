<script lang="ts">
  import { composing } from "$lib/keys";
  import { ui } from "$lib/state/ui.svelte";
  import { manage } from "$lib/state/manage.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Sheet from "./Sheet.svelte";
  import Artwork from "./Artwork.svelte";
  import PencilSimpleIcon from "phosphor-svelte/lib/PencilSimpleIcon";
  import MusicNoteSimpleIcon from "phosphor-svelte/lib/MusicNoteSimpleIcon";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import VinylRecordIcon from "phosphor-svelte/lib/VinylRecordIcon";
  import XIcon from "phosphor-svelte/lib/XIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";

  const s = $derived(ui.sheet?.kind === "edit" ? ui.sheet : null);
  let title = $state("");
  let artist = $state("");
  let album = $state("");

  $effect(() => {
    if (!s) return;
    title = s.track.title;
    artist = s.track.artist ?? "";
    album = s.track.album ?? "";
  });

  const close = () => (ui.sheet = null);

  async function save() {
    if (!s) return;
    const track = s.track;
    close();
    await manage.edit(track, title, artist, album);
  }

  const focus = (node: HTMLInputElement) => node.focus();
  const onEnter = (e: KeyboardEvent) => e.key === "Enter" && !composing(e) && save();
</script>

{#if s}
  <Sheet icon={PencilSimpleIcon} title={t("menu.editInfo")} onClose={close}>
    <div class="edit">
      <Artwork track={s.track} size={128} />
      <div>
        <label class="efield"><span class="cap hstack"><MusicNoteSimpleIcon size={14} />{t("edit.title")}</span><input bind:value={title} onkeydown={onEnter} use:focus /></label>
        <label class="efield"><span class="cap hstack"><UserIcon size={14} />{t("kind.artist")}</span><input bind:value={artist} onkeydown={onEnter} /></label>
        <label class="efield"><span class="cap hstack"><VinylRecordIcon size={14} />{t("kind.album")}</span><input bind:value={album} onkeydown={onEnter} /></label>
      </div>
    </div>
    <div class="sfoot">
      <button class="btn" onclick={close}><XIcon />{t("common.cancel")}</button>
      <button class="btn accent" onclick={save}><CheckIcon />{t("common.save")}</button>
    </div>
  </Sheet>
{/if}

<style>
  .edit { display: grid; grid-template-columns: 128px minmax(0, 1fr); gap: var(--s5); align-items: start; }
  .efield { display: grid; gap: 6px; margin-bottom: var(--s3); }
  .efield input { height: 38px; padding: 0 var(--s3); border-radius: var(--r-sm); border: 1px solid var(--line); background: var(--bg); color: var(--text); font-size: 14px; outline: none; transition: border-color var(--t) var(--ease); }
  .efield input:focus { border-color: var(--accent); }
  .sfoot { display: flex; justify-content: flex-end; gap: var(--s2); margin-top: var(--s5); }
</style>
