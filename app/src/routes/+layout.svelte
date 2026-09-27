<script lang="ts">
  import "@fontsource/geist/400.css";
  import "@fontsource/geist/500.css";
  import "@fontsource/geist/600.css";
  import "@fontsource/geist-mono/400.css";
  import "@fontsource/geist-mono/500.css";
  import "@fontsource/bricolage-grotesque/800.css";
  import "../styles/tokens.css";
  import "../styles/base.css";
  import IconContext from "phosphor-svelte/lib/IconContext";
  import { onMount } from "svelte";
  import { i18n } from "$lib/i18n/index.svelte";

  let { children } = $props();

  $effect(() => {
    document.documentElement.lang = i18n.locale;
  });

  onMount(() => {
    if (import.meta.env.DEV) void fetch("/__webview-log", { method: "POST", body: "ready" }).catch(() => {});
  });
</script>

<IconContext values={{ weight: "bold", size: 16 }}>
  {@render children()}
</IconContext>
