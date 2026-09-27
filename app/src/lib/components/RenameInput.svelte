<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { composing } from "$lib/keys";

  let { value, onDone }: { value: string; onDone: (name: string | null) => void } = $props();
  let done = false;

  function finish(name: string | null) {
    if (done) return;
    done = true;
    onDone(name);
  }

  /** Focuses and selects the name; a field removed while typing saves what it holds. */
  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
    return { destroy: () => finish(node.value.trim()) };
  }
</script>

<input
  class="rename"
  {value}
  aria-label={t("playlist.nameLabel")}
  use:focusSelect
  onclick={(e) => e.stopPropagation()}
  onkeydown={(e) => {
    e.stopPropagation();
    if (composing(e)) return;
    if (e.key === "Enter") finish(e.currentTarget.value.trim());
    if (e.key === "Escape") finish(null);
  }}
  onblur={(e) => finish(e.currentTarget.value.trim())}
/>

<style>
  .rename { font: inherit; letter-spacing: inherit; color: inherit; background: transparent; border: 0; border-bottom: 2px solid var(--accent); outline: none; width: 100%; padding: 0; margin: 0; }
</style>
