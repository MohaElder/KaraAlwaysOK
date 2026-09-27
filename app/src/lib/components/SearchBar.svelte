<script lang="ts">
  import { untrack } from "svelte";
  import { linkPreview, search } from "$lib/api";
  import { fromOutcome, previewFailed, withPreview } from "$lib/search";
  import { ui } from "$lib/state/ui.svelte";
  import { library } from "$lib/state/library.svelte";
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

  export function focus() {
    input?.focus();
  }

  function changed() {
    clearTimeout(timer);
    timer = setTimeout(run, 120);
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
  }

  $effect(() => {
    void library.cards;
    if (untrack(() => ui.search.kind === "text")) untrack(run);
  });

  export function clear() {
    seq++;
    clearTimeout(timer);
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
      onkeydown={(e) => {
        if (e.key !== "Escape") return;
        e.stopPropagation();
        clear();
        input?.blur();
      }}
      placeholder={t("search.placeholder")}
      aria-label={t("search.placeholder")}
      spellcheck="false"
    />
    {#if ui.query}
      <button type="button" class="ib" use:tip={t("search.clear")} transition:fade onclick={() => { clear(); input?.focus(); }}><XIcon size={16} /></button>
    {/if}
  </div>
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
  .hint > :global(svg) { color: var(--accent); margin-top: 1px; flex: none; }
  .hint b { display: block; font-weight: 600; }
  .hint small { font-size: 13px; color: var(--muted); }
</style>
