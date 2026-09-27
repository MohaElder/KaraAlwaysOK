<script lang="ts">
  import { onMount } from "svelte";
  import { link } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Artwork from "$lib/components/Artwork.svelte";
  import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import LinkIcon from "phosphor-svelte/lib/LinkIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
  import MagnifyingGlassIcon from "phosphor-svelte/lib/MagnifyingGlassIcon";
  import MagnifyingGlassMinusIcon from "phosphor-svelte/lib/MagnifyingGlassMinusIcon";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  let query = $state(link.results?.q ?? "");
  let done = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const found = $derived(link.results?.outcome ?? null);

  onMount(() => link.search(query));

  function typed() {
    clearTimeout(timer);
    timer = setTimeout(() => link.search(query), 200);
  }

  /** Queues with `add`, showing a check on the tapped button for a moment; taps while it shows are ignored. */
  function queued(key: string, add: () => void) {
    if (done === key) return;
    add();
    done = key;
    setTimeout(() => {
      if (done === key) done = null;
    }, 1200);
  }
</script>

{#snippet buttons(id: string, title: string, add: (next: boolean) => void)}
  <button class="pib" class:done={done === `${id}:next`} aria-label={t("song.playNext")} onclick={() => queued(`${id}:next`, () => add(true))}>
    {#if done === `${id}:next`}<CheckIcon size={20} />{:else}<ArrowBendDownRightIcon size={20} />{/if}
  </button>
  <button class="pib" class:done={done === `${id}:end`} aria-label={t("song.addToQueueLabel", { title })} onclick={() => queued(`${id}:end`, () => add(false))}>
    {#if done === `${id}:end`}<CheckIcon size={20} />{:else}<ListPlusIcon size={20} />{/if}
  </button>
{/snippet}

<label class="psearch">
  <MagnifyingGlassIcon size={24} />
  <input bind:value={query} placeholder={t("phone.searchPlaceholder")} aria-label={t("phone.searchPlaceholder")} autocomplete="off" maxlength="200" oninput={typed} />
</label>
<div class="plist">
  {#if found?.kind === "text"}
    {#if !query.trim()}<p class="cap hstack"><MusicNotesIcon size={14} />{t("phone.allSongs")}</p>{/if}
    {#each found.tracks as track (track.id)}
      <div class="prow">
        <Artwork {track} size={44} />
        <span class="grow"><b class="ell">{track.title}</b><small class="ell">{track.artist ?? ""}</small></span>
        {@render buttons(`song:${track.id}`, track.title, (next) => link.add(track.id, next))}
      </div>
    {:else}
      {#if query.trim()}
        <div class="pempty"><MagnifyingGlassMinusIcon size={32} /><b>{t("search.noMatchTitle", { query: query.trim() })}</b><span>{t("search.noMatchBody")}</span></div>
      {/if}
    {/each}
  {:else if found?.kind === "link"}
    {@const url = found.url}
    <p class="cap hstack"><LinkIcon size={14} />{t("search.fromLink")}</p>
    <div class="prow">
      <span class="th"><LinkIcon size={20} /></span>
      <span class="grow"><b class="ell">{t("adding.fromHost", { host: found.host })}</b></span>
      {@render buttons(`link:${url}`, found.host, (next) => link.addLink(url, next))}
    </div>
  {:else if found?.kind === "rejected"}
    <div class="pempty">
      <WarningIcon size={32} />
      <b>{t(found.streaming ? "search.streamingTitle" : "search.unsupportedTitle", { host: found.host })}</b>
      <span>{t(found.streaming ? "search.streamingTip" : "search.unsupportedTip")}</span>
    </div>
  {/if}
</div>

<style>
  .psearch { flex: none; display: flex; align-items: center; gap: var(--s3); height: 58px; margin-top: var(--s4); padding: 0 var(--s4); border-radius: var(--r-lg); background: var(--surface); border: 1.5px solid var(--line); transition: border-color var(--t) var(--ease), box-shadow var(--t) var(--ease); }
  .psearch:focus-within { border-color: var(--accent); box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 20%, transparent); }
  .psearch > :global(svg) { color: var(--muted); }
  .psearch input { flex: 1; min-width: 0; border: 0; background: none; outline: none; font-size: 18px; }
  .psearch input::placeholder { color: var(--muted); }
  .plist { display: grid; grid-template-columns: minmax(0, 1fr); margin-top: var(--s3); }
  .plist .cap { padding: var(--s3) 0 var(--s1); }
  .prow { display: flex; align-items: center; gap: var(--s3); min-height: 62px; }
  .prow b { display: block; font-weight: 600; }
  .prow small { display: block; color: var(--muted); font-size: 13px; }
  .th { width: 44px; height: 44px; flex: none; display: grid; place-items: center; border-radius: var(--r-sm); background: var(--raised); }
  .pib { width: 44px; height: 44px; flex: none; display: grid; place-items: center; border-radius: 50%; background: var(--raised); }
  .pib.done { color: var(--ready); }
  .pempty { display: grid; justify-items: center; gap: var(--s2); padding: var(--s7) 0; text-align: center; color: var(--muted); }
  .pempty b { color: var(--text); font-size: 16px; }
</style>
