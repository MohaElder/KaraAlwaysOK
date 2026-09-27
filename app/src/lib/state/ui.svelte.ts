import type { SearchView } from "$lib/search";

class UiState {
  karaoke = $state(false);
  queueOpen = $state(false);
  moreOpen = $state(false);
  /** A song row to highlight briefly. */
  flash = $state<number | null>(null);
  search = $state<SearchView>({ kind: "none" });
  query = $state("");

  clearSearch() {
    this.query = "";
    this.search = { kind: "none" };
  }
}

export const ui = new UiState();
