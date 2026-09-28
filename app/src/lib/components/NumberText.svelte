<script lang="ts">
  import { composing } from "$lib/keys";

  let { text, label, onEnter }: { text: string; label: string; onEnter: (n: number) => void } = $props();

  /** The first number typed, accepting − and a decimal comma; null when there is none. */
  function parse(s: string) {
    const m = s.replace(/−/g, "-").replace(",", ".").match(/[-+]?\d*\.?\d+/);
    return m ? parseFloat(m[0]) : null;
  }
</script>

<input
  value={text}
  aria-label={label}
  spellcheck="false"
  onfocus={(e) => e.currentTarget.select()}
  onblur={(e) => (e.currentTarget.value = text)}
  onkeydown={(e) => {
    if (composing(e)) return;
    const el = e.currentTarget;
    if (e.key === "Enter") {
      const n = parse(el.value);
      el.value = text;
      if (n !== null) onEnter(n);
      el.blur();
    } else if (e.key === "Escape") {
      e.stopPropagation();
      el.blur();
    }
  }}
/>
