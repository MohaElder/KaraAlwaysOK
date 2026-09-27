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
  /** Set when the library couldn't be listed or a collection opened. */
  error = $state<unknown>(null);
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
    await this.show(id);
  }

  /** Reloads the sidebar and the open collection, keeping the selection while it is shown. */
  async refresh() {
    const kind = this.kind;
    try {
      const cards = await listCollections(kind);
      if (this.kind !== kind) return;
      this.cards = cards;
      this.loaded = true;
    } catch (e) {
      if (this.kind === kind) this.error = e;
      return;
    }
    const keep = this.visibleCards.find((c) => c.id === this.selected) ?? this.visibleCards[0];
    this.selected = keep?.id ?? null;
    await this.show(this.selected);
  }

  /** Opens collection `id`, unless another one was picked meanwhile. */
  private async show(id: number | null) {
    try {
      const page = id === null ? null : await openCollection(id);
      if (this.selected === id) [this.page, this.error] = [page, null];
    } catch (e) {
      if (this.selected === id) this.error = e;
    }
  }
}

export const library = new LibraryState();
