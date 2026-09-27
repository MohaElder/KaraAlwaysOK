<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { adding } from "$lib/state/adding.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { fade } from "$lib/motion";
  import UploadSimpleIcon from "phosphor-svelte/lib/UploadSimpleIcon";

  let over = $state(false);

  onMount(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter") over = e.payload.paths.length > 0;
      else if (e.payload.type === "leave") over = false;
      else if (e.payload.type === "drop") {
        over = false;
        for (const path of e.payload.paths) void adding.file(path);
      }
    });
    return () => void unlisten.then((off) => off());
  });
</script>

{#if over}
  <div class="drop" transition:fade>
    <div class="badge"><UploadSimpleIcon size={26} /></div>
    <h2>{t("drop.title")}</h2>
    <p class="muted">{t("drop.body")}</p>
  </div>
{/if}

<style>
  .drop { position: fixed; inset: 0; z-index: 45; display: grid; place-content: center; justify-items: center; gap: var(--s2); outline: 2px dashed var(--accent); outline-offset: -12px; background: color-mix(in srgb, var(--bg) 88%, transparent); pointer-events: none; text-align: center; }
  h2 { font: 800 28px var(--display); letter-spacing: -.02em; }
  .badge { width: 56px; height: 56px; border-radius: var(--r-lg); display: grid; place-items: center; background: var(--accent); color: var(--on-accent); }
</style>
