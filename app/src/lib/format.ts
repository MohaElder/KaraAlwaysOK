/** 205_400 → "3:25"; nothing for an unknown length. */
export function duration(ms: number | null | undefined): string {
  if (ms == null) return "";
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export const clock = (seconds: number) => duration(seconds * 1000);

/** Total length of songs, in whole minutes. */
export function minutes(tracks: { durationMs: number | null }[]): number {
  return Math.round(tracks.reduce((sum, t) => sum + (t.durationMs ?? 0), 0) / 60_000);
}
