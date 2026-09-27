import { i18n } from "$lib/i18n/index.svelte";

export const ssr = false;

export const load = async () => {
  await i18n.init();
};
