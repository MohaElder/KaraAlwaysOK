import type { CollectionCard, Track } from "$lib/api";
import type { SearchView } from "$lib/search";

export type MenuState =
  | { kind: "song"; track: Track; playlistId: number | null; x: number; y: number; alignRight: boolean }
  | { kind: "playlist"; card: CollectionCard; x: number; y: number };
export type SheetState = { kind: "edit"; track: Track } | { kind: "settings" };

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
  /** The user playlist whose name is being typed, and where. */
  renaming = $state<{ id: number; place: "sidebar" | "hero"; isNew: boolean; withSongs: boolean } | null>(null);

  clearSearch() {
    this.query = "";
    this.search = { kind: "none" };
  }
}

export const ui = new UiState();
