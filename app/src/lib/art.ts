import { convertFileSrc } from "@tauri-apps/api/core";
import type { Track } from "./api";

/** Three hues from a song's stored random seed (0–359). */
export function hues(seed: number): [number, number, number] {
  return [seed, (seed + 137) % 360, (seed + 211) % 360];
}

/** A song picture's URL: the phone server's `/art/` link as it is, a file on this Mac through the asset protocol. */
const pictureUrl = (path: string) => (path.startsWith("/art/") ? path : convertFileSrc(path));

/** CSS background for a song's artwork: its picture, or the gradient picked for it when it was added. */
export function artBackground(t: Pick<Track, "artSeed" | "artworkPath">): string {
  if (t.artworkPath) return `center / cover no-repeat url("${pictureUrl(t.artworkPath)}")`;
  const [a, b, c] = hues(t.artSeed);
  return `radial-gradient(circle at 22% 28%, hsl(${a} 82% 64%), transparent 55%), radial-gradient(circle at 78% 72%, hsl(${b} 76% 56%), transparent 60%), radial-gradient(circle at 62% 8%, hsl(${c} 88% 72%), transparent 46%), hsl(${b} 42% 20%)`;
}
