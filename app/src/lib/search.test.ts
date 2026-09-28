import { expect, it } from "vitest";
import type { LinkPreview, SearchHit, SearchOutcome } from "./api";
import { fromOutcome, previewFailed, withPreview, withWeb } from "./search";

const link = (url: string): SearchOutcome => ({ kind: "link", url, host: "youtu.be" });
const text: SearchOutcome = { kind: "text", tracks: [], collections: [] };
const preview = (title: string, more: Partial<LinkPreview> = {}): LinkPreview => ({ title, channel: null, durationMs: null, thumbnail: null, ...more });

it("a_late_preview_for_an_older_link_is_ignored", () => {
  let v = fromOutcome({ kind: "none" }, "youtu.be/a", link("https://youtu.be/a"));
  v = fromOutcome(v, "youtu.be/b", link("https://youtu.be/b"));
  v = withPreview(v, "https://youtu.be/a", preview("Old Song"));
  expect(v).toMatchObject({ kind: "link", url: "https://youtu.be/b", preview: null });
  v = fromOutcome(v, "paper", text);
  expect(withPreview(v, "https://youtu.be/b", preview("Late"))).toEqual(v);
});

it("a preview replaces an earlier failure, stays while the link is retyped, fills in later details, and clears on empty text", () => {
  let v = previewFailed(fromOutcome({ kind: "none" }, "youtu.be/a", link("https://youtu.be/a")), "https://youtu.be/a");
  v = withPreview(v, "https://youtu.be/a", preview("Made Up Song", { thumbnail: "t.jpg", channel: "Made Up Channel" }));
  v = withPreview(v, "https://youtu.be/a", preview("Made Up Song", { durationMs: 205_000, thumbnail: "other.webp" }));
  v = fromOutcome(v, " youtu.be/a ", link("https://youtu.be/a"));
  expect(v).toMatchObject({ failed: false, preview: { channel: "Made Up Channel", durationMs: 205_000, thumbnail: "t.jpg" } });
  expect(fromOutcome(v, "  ", text)).toEqual({ kind: "none" });
});

it("each site's results fill only the words they were found for, stay while those are retyped, and are skipped for one letter", () => {
  const hits: SearchHit[] = [{ url: "https://www.youtube.com/watch?v=a", ...preview("Made Up Song") }];
  let v = fromOutcome({ kind: "none" }, "paper", text);
  expect(v).toMatchObject({ web: { youtube: null, bilibili: null } });
  expect(withWeb(v, "youtube", "pape", hits)).toEqual(v);
  v = withWeb(v, "youtube", "paper", hits);
  expect(fromOutcome(v, " paper ", text)).toMatchObject({ web: { youtube: hits, bilibili: null } });
  expect(fromOutcome(v, "papers", text)).toMatchObject({ web: { youtube: null, bilibili: null } });
  expect(fromOutcome(v, "p", text)).toMatchObject({ web: { youtube: [], bilibili: [] } });
});
