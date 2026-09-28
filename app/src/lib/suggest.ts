import { WEB_SOURCES, webSuggestions } from "./api";

/** Where search phrases come from while typing; streaming providers add theirs here. */
export const PHRASE_SOURCES: ((query: string) => Promise<string[]>)[] = WEB_SOURCES.map((source) => (query: string) => webSuggestions(source, query));

/** How many of your songs the suggestions show. */
export const SONG_SUGGESTIONS = 3;

/** Every source's phrases in order, once each (ignoring case), without the typed words themselves. */
export function mergePhrases(query: string, lists: string[][]): string[] {
  const seen = new Set([query.trim().toLowerCase()]);
  return lists.flat().filter((p) => {
    const key = p.trim().toLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** Phrases from every source for `query`; a source that fails adds none. */
export async function phrasesFor(query: string): Promise<string[]> {
  return mergePhrases(query, await Promise.all(PHRASE_SOURCES.map((source) => source(query).catch(() => []))));
}
