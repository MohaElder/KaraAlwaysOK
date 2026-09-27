class UiState {
  karaoke = $state(false);
  queueOpen = $state(false);
  /** A song row to highlight briefly. */
  flash = $state<number | null>(null);
}

export const ui = new UiState();
