<script lang="ts">
  import { composing } from "$lib/keys";
  import { untrack } from "svelte";
  import { linkPreview, search, youtubeSearch } from "$lib/api";
  import { fromOutcome, previewFailed, withPreview, withYoutube } from "$lib/search";
  import { phrasesFor, SONG_SUGGESTIONS } from "$lib/suggest";
  import { ui } from "$lib/state/ui.svelte";
  import { library } from "$lib/state/library.svelte";
  import { player } from "$lib/state/player.svelte";
  import { adding } from "$lib/state/adding.svelte";
  import Artwork from "./Artwork.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import { fade, slide } from "$lib/motion";
  import MagnifyingGlassIcon from "phosphor-svelte/lib/MagnifyingGlassIcon";
  import XIcon from "phosphor-svelte/lib/XIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  let { onSubmitLink }: { onSubmitLink?: (url: string, title: string) => void } = $props();
  let input: HTMLInputElement | undefined = $state();
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let youtubeTimer: ReturnType<typeof setTimeout> | undefined;
  let youtubeAsked: string | null = null;
  let phraseTimer: ReturnType<typeof setTimeout> | undefined;
  let phraseSeq = 0;
  let phrases = $state<string[]>([]);
  let suggesting = $state(false);
  let active = $state(-1);

  const songs = $derived(
    ui.search.kind === "text"
      ? ui.search.tracks.filter((x) => !library.hidden.has(`track:${x.id}`) && !adding.ids.has(x.id)).slice(0, SONG_SUGGESTIONS)
      : [],
  );
  const choices = $derived([...songs.map((track) => ({ track, text: track.title })), ...phrases.map((text) => ({ track: null, text }))]);
  const open = $derived(suggesting && ui.search.kind === "text" && choices.length > 0);

  export function focus() {
    input?.focus();
  }

  function changed() {
    clearTimeout(timer);
    clearTimeout(youtubeTimer);
    timer = setTimeout(run, 120);
    suggesting = true;
    active = -1;
    clearTimeout(phraseTimer);
    phraseTimer = setTimeout(suggest, 150);
  }

  async function suggest() {
    const mine = ++phraseSeq;
    const query = ui.query.trim();
    const found = query && ui.search.kind === "text" ? await phrasesFor(query) : [];
    if (mine === phraseSeq) phrases = found;
  }

  function hideSuggestions() {
    suggesting = false;
    active = -1;
  }

  function choose(i: number) {
    const { track, text } = choices[i];
    hideSuggestions();
    if (track) {
      player.enqueue(track.id);
      return;
    }
    ui.query = text;
    clearTimeout(timer);
    run();
  }

  async function run() {
    const mine = ++seq;
    const value = ui.query;
    const outcome = await search(value, t("library.imported"));
    if (mine !== seq) return;
    const before = ui.search;
    ui.search = fromOutcome(before, value, outcome);
    if (ui.search.kind === "link" && ui.search !== before) {
      const url = ui.search.url;
      linkPreview(url, (p) => (ui.search = withPreview(ui.search, url, p))).catch(() => (ui.search = previewFailed(ui.search, url)));
    }
    if (ui.search.kind === "text" && ui.search.youtube === null && ui.search.query !== youtubeAsked) {
      clearTimeout(youtubeTimer);
      youtubeTimer = setTimeout(findOnYoutube, 280);
    }
  }

  async function findOnYoutube() {
    if (ui.search.kind !== "text") return;
    const query = (youtubeAsked = ui.search.query);
    const hits = await youtubeSearch(query).catch(() => []);
    if (youtubeAsked === query) youtubeAsked = null;
    ui.search = withYoutube(ui.search, query, hits);
  }

  $effect(() => {
    void library.cards;
    if (untrack(() => ui.search.kind === "text")) untrack(run);
  });

  export function clear() {
    seq++;
    phraseSeq++;
    clearTimeout(timer);
    clearTimeout(youtubeTimer);
    clearTimeout(phraseTimer);
    phrases = [];
    hideSuggestions();
    ui.clearSearch();
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (ui.search.kind !== "link") return;
    const { url, preview } = ui.search;
    clear();
    input?.blur();
    onSubmitLink?.(url, preview?.title ?? "");
  }
</script>

<form class="search" role="search" autocomplete="off" onsubmit={submit}>
  <div class="sbox glass">
    <MagnifyingGlassIcon size={22} />
    <input
      bind:this={input}
      bind:value={ui.query}
      oninput={changed}
      onblur={hideSuggestions}
      onkeydown={(e) => {
        if (composing(e)) return e.key === "Enter" && e.preventDefault();
        if (open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
          e.preventDefault();
          active = Math.max(-1, Math.min(choices.length - 1, active + (e.key === "ArrowDown" ? 1 : -1)));
        } else if (open && e.key === "Enter" && choices[active]) {
          e.preventDefault();
          choose(active);
        } else if (e.key === "Escape") {
          e.stopPropagation();
          if (open) return hideSuggestions();
          clear();
          input?.blur();
        }
      }}
      placeholder={t("search.placeholder")}
      aria-label={t("search.placeholder")}
      aria-expanded={open}
      aria-controls={open ? "suggestions" : undefined}
      aria-activedescendant={open && active >= 0 ? `suggestion-${active}` : undefined}
      spellcheck="false"
    />
    {#if ui.query}
      <button type="button" class="ib" use:tip={t("search.clear")} transition:fade onclick={() => { clear(); input?.focus(); }}><XIcon size={16} /></button>
    {/if}
  </div>
  {#if open}
    <ul id="suggestions" class="suggest glass" role="listbox" aria-label={t("search.suggestions")} transition:fade onmousedown={(e) => e.preventDefault()}>
      {#each choices as c, i (c.track ? `song:${c.track.id}` : `phrase:${c.text}`)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li id="suggestion-{i}" role="option" aria-selected={i === active} class:on={i === active} onclick={() => choose(i)} onmousemove={() => (active = i)}>
          {#if c.track}<Artwork track={c.track} size={28} />{:else}<MagnifyingGlassIcon size={18} />{/if}
          <span class="ell">{c.text}</span>
        </li>
      {/each}
    </ul>
  {/if}
  {#if ui.search.kind === "rejected"}
    {@const r = ui.search}
    <div class="hint glass" role="status" transition:slide={{ y: -8 }}>
      <WarningIcon size={18} />
      <div class="grow">
        <b>{t(r.streaming ? "search.streamingTitle" : "search.unsupportedTitle", { host: r.host })}</b>
        <small>{t(r.streaming ? "search.streamingTip" : "search.unsupportedTip")}</small>
      </div>
    </div>
  {/if}
</form>

<style>
  .search { grid-column: 2; position: relative; }
  .sbox { display: flex; align-items: center; gap: var(--s3); height: 50px; padding: 0 var(--s3) 0 var(--s5); border-radius: 999px; transition: box-shadow var(--t) var(--ease); }
  .sbox:focus-within { box-shadow: inset 0 0 0 1px var(--glass-edge), 0 0 0 1.5px var(--accent), 0 0 0 5px color-mix(in srgb, var(--accent) 20%, transparent); }
  .sbox > :global(svg) { color: var(--muted); transition: color var(--t) var(--ease); }
  .sbox:focus-within > :global(svg) { color: var(--accent); }
  input { flex: 1; min-width: 0; border: 0; background: none; outline: none; font-size: 17px; }
  input::placeholder { color: var(--muted); }
  .ib { border-radius: 50%; }
  .hint { position: absolute; left: var(--s2); right: var(--s2); top: calc(100% + var(--s2)); display: flex; gap: var(--s3); padding: var(--s3) var(--s4); border-radius: var(--r-lg); }
  .suggest { position: absolute; left: var(--s2); right: var(--s2); top: calc(100% + var(--s2)); margin: 0; padding: var(--s2); list-style: none; border-radius: var(--r-lg); }
  .suggest li { display: flex; align-items: center; gap: var(--s3); min-height: 40px; padding: 0 var(--s3); border-radius: var(--r-sm); cursor: default; }
  .suggest li > :global(svg) { margin-inline: 5px; color: var(--muted); flex: none; }
  .suggest li.on { background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .hint > :global(svg) { color: var(--accent); margin-top: 1px; flex: none; }
  .hint b { display: block; font-weight: 600; }
  .hint small { font-size: 13px; color: var(--muted); }
</style>
