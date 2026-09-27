import type { Track } from "$lib/api";
import type { SearchView } from "$lib/search";

export type MenuState = { kind: "song"; track: Track; playlistId: number | null; x: number; y: number; alignRight: boolean };
export type SheetState = { kind: "edit"; track: Track };

class UiState {
  karaoke = $state(false);
  queueOpen = $state(false);
  moreOpen = $state(false);
  /** A song row to highlight briefly. */
  flash = $state<number | null>(null);
  search = $state<SearchView>({ kind: "none" });
  query = $state("");
  menu = $state<MenuState | null>(null);
  sheet = $state<SheetState | null>(null);

  clearSearch() {
    this.query = "";
    this.search = { kind: "none" };
  }
}

export const ui = new UiState();
