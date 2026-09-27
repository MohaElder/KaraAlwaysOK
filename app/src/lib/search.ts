import type { CollectionCard, LinkPreview, SearchHit, SearchOutcome, Track, WebSource } from "./api";

export type SearchView =
  | { kind: "none" }
  | { kind: "text"; query: string; tracks: Track[]; collections: CollectionCard[]; web: Record<WebSource, SearchHit[] | null> }
  | { kind: "link"; url: string; host: string; preview: LinkPreview | null; failed: boolean }
  | { kind: "rejected"; streaming: boolean; host: string };

/** The view for a search result; the same link keeps the preview it already has, the same words each site's results (null while still to find). */
export function fromOutcome(prev: SearchView, input: string, o: SearchOutcome): SearchView {
  if (o.kind === "link") return prev.kind === "link" && prev.url === o.url ? prev : { kind: "link", url: o.url, host: o.host, preview: null, failed: false };
  if (o.kind === "rejected") return { kind: "rejected", streaming: o.streaming, host: o.host };
  const query = input.trim();
  if (!query) return { kind: "none" };
  const unasked = query.length >= 2 ? null : [];
  const web = prev.kind === "text" && prev.query === query ? prev.web : { youtube: unasked, bilibili: unasked };
  return { kind: "text", query, tracks: o.tracks, collections: o.collections, web };
}

/** Adds a preview that arrived for `url`, clearing an earlier failure; one for any other link is dropped. Details only fill in. */
export function withPreview(view: SearchView, url: string, p: LinkPreview): SearchView {
  if (view.kind !== "link" || view.url !== url) return view;
  const old = view.preview;
  return {
    ...view,
    failed: false,
    preview: {
      title: old?.title ?? p.title,
      channel: old?.channel ?? p.channel,
      durationMs: old?.durationMs ?? p.durationMs,
      thumbnail: old?.thumbnail ?? p.thumbnail,
    },
  };
}

/** Marks the link's preview as failed, unless one already arrived or the link changed. */
export function previewFailed(view: SearchView, url: string): SearchView {
  return view.kind === "link" && view.url === url && !view.preview ? { ...view, failed: true } : view;
}

/** Adds one site's results found for `query`; ones for any other words are dropped. */
export function withWeb(view: SearchView, source: WebSource, query: string, hits: SearchHit[]): SearchView {
  return view.kind === "text" && view.query === query ? { ...view, web: { ...view.web, [source]: hits } } : view;
}
