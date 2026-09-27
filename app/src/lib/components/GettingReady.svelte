<script lang="ts">
  import { engine } from "$lib/state/engine.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { say } from "$lib/i18n/engine";
  import { fade, slide } from "$lib/motion";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";
  import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";

  const shown = $derived(engine.phase === "downloading" || engine.phase === "done" || engine.phase === "failed");
  const fraction = $derived.by(() => {
    const p = engine.progress;
    return p ? (p.step - 1 + (p.total ? p.done / p.total : 0)) / p.steps : 0;
  });
  const mb = (bytes: number) => Math.round(bytes / 1_000_000);
</script>

{#if shown}
  <div class="first" transition:fade>
    {#key engine.phase}
      <div class="card" in:slide>
        <div class="logo">
          {#if engine.phase === "done"}<CheckIcon size={36} />{:else if engine.phase === "failed"}<WarningIcon size={36} />{:else}<MicrophoneStageIcon size={36} />{/if}
        </div>
        {#if engine.phase === "done"}
          <h1>{t("setup.doneTitle")}</h1>
          <p class="muted">{t("setup.doneBody")}</p>
        {:else if engine.phase === "failed"}
          <h1>{t("setup.failedTitle")}</h1>
          <p class="muted">{say(engine.error)}</p>
          <button class="btn accent" onclick={() => engine.start()}><ArrowClockwiseIcon />{t("common.tryAgain")}</button>
        {:else}
          <h1>{t("setup.title")}</h1>
          <p class="muted">{t("setup.body")}</p>
          <div class="meter"><i style:width="{fraction * 100}%"></i></div>
          {#if engine.progress?.total}<span class="num">{t("setup.progress", { done: mb(engine.progress.done), total: mb(engine.progress.total) })}</span>{/if}
        {/if}
      </div>
    {/key}
  </div>
{/if}

<style>
  .first { position: fixed; inset: 0; z-index: 55; display: grid; place-items: center; padding: var(--s4); background: var(--bg); }
  .card { grid-area: 1 / 1; display: grid; justify-items: center; text-align: center; gap: var(--s3); width: min(380px, 100%); }
  h1 { font: 800 28px/1.1 var(--display); letter-spacing: -.02em; margin-top: var(--s2); }
  .logo { width: 72px; height: 72px; border-radius: var(--r-lg); display: grid; place-items: center; background: var(--accent); color: var(--on-accent); }
  .meter { width: 100%; height: 8px; margin-top: var(--s3); border-radius: 4px; background: var(--raised); overflow: hidden; }
  .meter i { display: block; height: 100%; border-radius: 4px; background: var(--accent); }
</style>
