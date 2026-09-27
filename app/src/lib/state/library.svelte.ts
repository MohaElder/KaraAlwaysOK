import { SvelteSet } from "svelte/reactivity";
import { listCollections, openCollection, type CollectionCard, type CollectionPage, type Kind } from "$lib/api";
import { t } from "$lib/i18n/index.svelte";

/** The name to show for a collection: the app-made Imported playlist in the current language, others as stored. */
export const cardName = (c: CollectionCard) => (c.user || c.kind !== "playlist" ? c.name : t("library.imported"));

class LibraryState {
  kind = $state<Kind>("playlist");
  cards = $state<CollectionCard[]>([]);
  selected = $state<number | null>(null);
  page = $state<CollectionPage | null>(null);
  loaded = $state(false);
  /** Items taken off screen while their deletion waits for the undo toast. */
  hidden = new SvelteSet<string>();

  visibleCards = $derived(this.cards.filter((c) => !this.hidden.has(`playlist:${c.id}`)));
  visibleTracks = $derived(
    (this.page?.tracks ?? []).filter((t) => !this.hidden.has(`track:${t.id}`) && !this.hidden.has(`entry:${this.page?.card.id}:${t.id}`)),
  );

  async setKind(kind: Kind) {
    this.kind = kind;
    this.selected = null;
    await this.refresh();
  }

  async select(id: number) {
    this.selected = id;
    this.page = await openCollection(id);
  }

  /** Reloads the sidebar and the open collection, keeping the selection while it exists. */
  async refresh() {
    this.cards = await listCollections(this.kind);
    this.loaded = true;
    const keep = this.cards.find((c) => c.id === this.selected) ?? this.visibleCards[0];
    this.selected = keep?.id ?? null;
    this.page = keep ? await openCollection(keep.id) : null;
  }
}

export const library = new LibraryState();
