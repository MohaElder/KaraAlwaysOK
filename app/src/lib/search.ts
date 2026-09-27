import type { CollectionCard, LinkPreview, SearchOutcome, Track } from "./api";

export type SearchView =
  | { kind: "none" }
  | { kind: "text"; query: string; tracks: Track[]; collections: CollectionCard[] }
  | { kind: "link"; url: string; host: string; preview: LinkPreview | null; failed: boolean }
  | { kind: "rejected"; streaming: boolean; host: string };

/** The view for a search result; the same link keeps the preview it already has. */
export function fromOutcome(prev: SearchView, input: string, o: SearchOutcome): SearchView {
  if (o.kind === "link") return prev.kind === "link" && prev.url === o.url ? prev : { kind: "link", url: o.url, host: o.host, preview: null, failed: false };
  if (o.kind === "rejected") return { kind: "rejected", streaming: o.streaming, host: o.host };
  const query = input.trim();
  return query ? { kind: "text", query, tracks: o.tracks, collections: o.collections } : { kind: "none" };
}

/** Adds a preview that arrived for `url`; one for any other link is dropped. Details only fill in. */
export function withPreview(view: SearchView, url: string, p: LinkPreview): SearchView {
  if (view.kind !== "link" || view.url !== url) return view;
  const old = view.preview;
  return {
    ...view,
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
