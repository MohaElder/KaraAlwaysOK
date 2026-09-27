<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Icon } from "$lib/icons";

  let { icon: IconC, warn = false, title, lead, children, action }: {
    icon?: Icon;
    warn?: boolean;
    title: string;
    lead: string;
    children?: Snippet;
    action?: Snippet;
  } = $props();
</script>

<div class="pcenter">
  <div class="bigicon" class:warn>{#if IconC}<IconC size={44} />{:else}<span class="spin"></span>{/if}</div>
  <h1>{title}</h1>
  <p class="lead">{lead}</p>
  {@render children?.()}
</div>
{@render action?.()}
