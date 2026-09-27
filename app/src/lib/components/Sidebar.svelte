<script lang="ts">
  import type { Kind } from "$lib/api";
  import type { Icon } from "$lib/icons";
  import { t, type Key } from "$lib/i18n/index.svelte";
  import { cardName, library } from "$lib/state/library.svelte";
  import { fade, slide } from "$lib/motion";
  import { ui } from "$lib/state/ui.svelte";
  import { manage } from "$lib/state/manage.svelte";
  import Cover from "./Cover.svelte";
  import RenameInput from "./RenameInput.svelte";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";
  import PlaylistIcon from "phosphor-svelte/lib/PlaylistIcon";
  import VinylRecordIcon from "phosphor-svelte/lib/VinylRecordIcon";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import TrayIcon from "phosphor-svelte/lib/TrayIcon";

  const tabs: { kind: Kind; label: Key; icon: Icon }[] = [
    { kind: "playlist", label: "kind.playlists", icon: PlaylistIcon },
    { kind: "album", label: "kind.albums", icon: VinylRecordIcon },
    { kind: "artist", label: "kind.artists", icon: UserIcon },
  ];
</script>

<aside class="side">
  <div class="tabs" role="toolbar" aria-label={t("library.browseBy")}>
    {#each tabs as tab (tab.kind)}
      <button aria-pressed={library.kind === tab.kind} onclick={() => library.setKind(tab.kind)}><tab.icon size={15} />{t(tab.label)}</button>
    {/each}
  </div>
  <nav class="items">
    {#if library.kind === "playlist"}
      <button class="item newpl" onclick={() => manage.newPlaylist()}><span class="plus"><PlusIcon size={16} /></span><span class="nm grow">{t("library.newPlaylist")}</span></button>
    {/if}
    {#each library.visibleCards as c (c.id)}
      <button
        class="item"
        aria-current={library.selected === c.id}
        onclick={() => library.select(c.id)}
        oncontextmenu={(e) => {
          if (c.kind !== "playlist") return;
          e.preventDefault();
          ui.menu = { kind: "playlist", card: c, x: e.clientX, y: e.clientY };
        }}
        in:slide={{ y: 4 }}
      >
        <Cover covers={c.covers} size={32} grid={c.kind === "playlist"} round={c.kind === "artist"} />
        {#if ui.renaming?.place === "sidebar" && ui.renaming.id === c.id}
          <span class="nm grow"><RenameInput value={c.name} onDone={(name) => manage.finishRename(name)} /></span>
        {:else}
          <span class="nm grow ell">{cardName(c)}</span>
        {/if}
      </button>
    {:else}
      {#if library.loaded}<div class="note" in:fade><TrayIcon size={16} />{t("library.nothingHere")}</div>{/if}
    {/each}
  </nav>
  <div class="foot"></div>
</aside>

<style>
  .side { display: flex; flex-direction: column; min-height: 0; padding: var(--s3); background: var(--side); border-right: 1px solid var(--line); }
  .tabs { display: flex; justify-content: space-between; padding: var(--s2) var(--s1) 0; border-bottom: 1px solid var(--line); }
  .tabs button { display: flex; align-items: center; gap: 6px; padding-bottom: var(--s2); font-size: 13px; font-weight: 600; color: var(--muted); box-shadow: inset 0 -2px 0 transparent; }
  .tabs button:hover { color: var(--text); }
  .tabs button[aria-pressed="true"] { color: var(--text); box-shadow: inset 0 -2px 0 var(--accent); }
  .items { flex: 1; min-height: 0; overflow: auto; padding: var(--s2) 0; display: flex; flex-direction: column; gap: 2px; }
  .item { display: flex; align-items: center; gap: var(--s3); width: 100%; height: 44px; flex: none; padding: 0 var(--s2); border-radius: var(--r-sm); text-align: left; }
  .item:hover { background: color-mix(in srgb, var(--text) 5%, transparent); }
  .item[aria-current="true"] { background: color-mix(in srgb, var(--text) 10%, transparent); }
  .item[aria-current="true"] .nm { color: var(--accent); }
  .nm { font-weight: 500; transition: color var(--t) var(--ease); }
  .plus { width: 32px; height: 32px; border-radius: 6px; display: grid; place-items: center; background: color-mix(in srgb, var(--text) 8%, transparent); color: var(--muted); flex: none; }
  .newpl .nm { color: var(--muted); }
  .note { display: flex; gap: var(--s2); align-items: center; padding: var(--s4) var(--s2); color: var(--muted); font-size: 13px; }
  .foot { display: flex; align-items: center; justify-content: flex-end; min-height: 44px; padding-top: var(--s3); border-top: 1px solid var(--line); }
</style>
