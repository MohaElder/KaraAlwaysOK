import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { CollectionCard, EngineEvent, Kind, LyricLine, Lyrics, PlayerSnapshot, SearchOutcome, Track } from "$lib/api";

declare global {
  interface Window {
    fake: typeof fake;
  }
}

const GB = 1024 ** 3;
const CHUNK_FRAMES = 441_000;

const song = (id: number, title: string, artist: string | null, album: string | null, durationMs: number, artSeed: number): Track => ({
  id, provider: "local", title, artist, album, durationMs, vocalRemoval: 100, keySemitones: 0, instrumental: false, artworkPath: null, artSeed,
});

const line = (voice: LyricLine["voice"], ...words: [string, number, number][]): LyricLine => ({
  start_ms: words[0][1], end_ms: words.at(-1)![2], text: words.map((w) => w[0]).join(" "), voice, words: words.map(([text, start_ms, end_ms]) => ({ text, start_ms, end_ms })),
});

const tracks = new Map<number, Track>([
  [1, song(1, "Paper Boats", "Juniper Row", "Harbor Lights", 200_000, 20)],
  [2, song(2, "東京の夜空", "星野ミナ", "夜の歌", 185_000, 200)],
  [3, song(3, "Lemon Skies", "The Porchlights", "Harbor Lights", 240_000, 90)],
  [4, song(4, "Kettle Duet", "Juniper Row", null, 210_000, 300)],
  [5, song(5, "Quiet Hours", "Ada Vale", null, 150_000, 150)],
]);

const lyrics = new Map<number, Lyrics>([
  [1, { source: "lrclib", lines: [
    line(undefined, ["Fold", 5000, 5500], ["the", 5500, 6000], ["morning", 6000, 7000], ["paper", 7000, 8000]),
    line(undefined, ["Send", 8500, 9000], ["it", 9000, 9500], ["down", 9500, 10000], ["the", 10000, 10500], ["drain", 10500, 11000]),
    line(undefined, ["Wave", 20000, 20500], ["from", 20500, 21000], ["the", 21000, 21500], ["bridge", 21500, 22000]),
  ] }],
  [4, { source: "embedded", lines: [
    line("m", ["Who", 1000, 1500], ["boiled", 1500, 2000], ["the", 2000, 2500], ["water", 2500, 3000]),
    line("f", ["I", 3000, 3500], ["did,", 3500, 4000], ["of", 4000, 4500], ["course", 4500, 5000]),
    line("both", ["Tea", 5000, 5500], ["for", 5500, 6000], ["two", 6000, 7000]),
  ] }],
  [5, { source: "none", lines: [] }],
]);

const playlists = [
  { id: 1, name: "Imported", user: false, trackIds: [1, 2, 3, 4, 5] },
  { id: 2, name: "Friday Mix", user: true, trackIds: [1, 2, 3, 4] },
];

let queue: { key: number; trackId: number }[] = [];
let current: number | null = null;
let ended = false;
let nextKey = 0;
let nextId = 100;
let storage = { usedBytes: 1.5 * GB, limitBytes: 5 * GB };
const paths = new Map<string, number>();
const lyricOffsets = new Map<number, number>();
const ready = new Set(tracks.keys());
const groupIds = new Map<string, number>();

/** The fake backend's state and levers, for tests to read and pull through `window.fake`. */
const fake = {
  calls: [] as { cmd: string; args: Record<string, unknown> }[],
  /** The audio clock in seconds; the fake AudioContext reports it as `currentTime`. */
  time: 0,
  /** True once a song's audio started. */
  started: false,
  engine(e: EngineEvent) {
    if (e.kind === "added") ready.add(e.trackId);
    return emit("engine", e);
  },
  drop(paths: string[]) {
    return emit("tauri://drag-drop", { paths, position: { x: 0, y: 0 } });
  },
};
window.fake = fake;

function need<T>(value: T | undefined): T {
  if (value === undefined) throw { problem: "songGone", message: "Gone." };
  return value;
}

function groupId(kind: Kind, name: string) {
  const key = `${kind}:${name}`;
  if (!groupIds.has(key)) groupIds.set(key, 1000 + groupIds.size);
  return groupIds.get(key)!;
}

function collections(kind: Kind): { card: CollectionCard; tracks: Track[] }[] {
  const make = (id: number, name: string, list: Track[], user: boolean, subtitle: string | null = null) => ({
    card: { id, provider: "local" as const, kind, name, subtitle, user, count: list.length, covers: list.slice(0, 4) },
    tracks: list,
  });
  if (kind === "playlist") return playlists.map((p) => make(p.id, p.name, p.trackIds.map((id) => tracks.get(id)!), p.user));
  const groups = new Map<string, Track[]>();
  for (const t of tracks.values()) {
    const name = kind === "album" ? t.album : t.artist;
    if (name) groups.set(name, [...(groups.get(name) ?? []), t]);
  }
  return [...groups].map(([name, list]) => make(groupId(kind, name), name, list, false, kind === "album" ? list[0].artist : null));
}

const allCollections = () => (["playlist", "album", "artist"] as const).flatMap(collections);

function snapshot(): PlayerSnapshot {
  return { entries: queue.map((e) => ({ key: e.key, track: tracks.get(e.trackId)! })), current, ended, lyricOffsetMs: current === null ? 0 : lyricOffsets.get(queue[current].trackId) ?? 0 };
}

/** Sends the new queue to the app, as the real backend does after every change. */
function changed() {
  const s = snapshot();
  void emit("player", s);
  return s;
}

function upcoming() {
  return current == null || ended ? [] : queue.slice(current).map((e) => e.trackId);
}

function search(input: string, imported: string): SearchOutcome {
  if (/^https?:\/\//.test(input)) {
    const host = new URL(input).hostname.replace(/^www\./, "");
    if (/spotify|music\.apple/.test(host)) return { kind: "rejected", streaming: true, host };
    if (/youtu|soundcloud|bandcamp/.test(host)) return { kind: "link", url: input, host };
    return { kind: "rejected", streaming: false, host };
  }
  const q = input.trim().toLowerCase();
  const has = (s: string | null) => !!q && !!s?.toLowerCase().includes(q);
  return {
    kind: "text",
    tracks: [...tracks.values()].filter((t) => has(t.title) || has(t.artist) || has(t.album)),
    collections: allCollections().map((c) => c.card).filter((c) => has(c.user || c.kind !== "playlist" ? c.name : imported)),
  };
}

function addTrack(title: string) {
  const t = song(nextId++, title, null, null, 180_000, 42);
  tracks.set(t.id, t);
  playlists[0].trackIds.unshift(t.id);
  return t;
}

const commands: Record<string, (a: any) => unknown> = {
  system_locale: () => "en-US",
  startup_problem: () => null,
  setup_engine: () => null,
  reduce_transparency: () => false,
  "plugin:app|version": () => "0.1.0",
  "plugin:updater|check": () => null,
  list_collections: ({ kind }) => collections(kind).map((c) => c.card),
  open_collection: ({ id }) => need(allCollections().find((c) => c.card.id === id)),
  get_track: ({ trackId }) => need(tracks.get(trackId)),
  edit_track: ({ trackId, title, artist, album }) => Object.assign(need(tracks.get(trackId)), { title, artist: artist || null, album: album || null }),
  track_lyrics: ({ trackId }) => lyrics.get(trackId) ?? { source: "lrclib", lines: [line(undefined, ["La", 1000, 2000])] },
  player_state: snapshot,
  queue_add: ({ trackId, next }) => {
    need(tracks.get(trackId));
    const e = { key: ++nextKey, trackId };
    if (current == null || ended) [queue, current, ended] = [[...queue, e], queue.length, false];
    else queue.splice(next ? current + 1 : queue.length, 0, e);
    return changed();
  },
  play_tracks: ({ trackIds, start }) => {
    queue = trackIds.map((trackId: number) => ({ key: ++nextKey, trackId }));
    [current, ended] = [queue.length ? start : null, false];
    return changed();
  },
  skip: ({ delta }) => {
    if (current != null && current + delta >= 0 && current + delta < queue.length) [current, ended] = [current + delta, false];
    return changed();
  },
  song_ended: () => {
    if (current != null && current + 1 < queue.length) current++;
    else ended = current != null;
    return changed();
  },
  queue_move: ({ key, to }) => {
    const from = queue.findIndex((e) => e.key === key);
    const [e] = queue.splice(from, 1);
    queue.splice(Math.max(to, current! + 1), 0, e);
    return changed();
  },
  queue_remove: ({ key }) => {
    queue = queue.filter((e, i) => e.key !== key || i <= current!);
    return changed();
  },
  set_singer: ({ value }) => {
    need(tracks.get(queue[current!].trackId)).vocalRemoval = value;
    return changed();
  },
  set_key: ({ semitones }) => {
    need(tracks.get(queue[current!].trackId)).keySemitones = semitones;
    return changed();
  },
  set_lyric_offset: ({ ms }) => {
    lyricOffsets.set(queue[current!].trackId, ms);
    return changed();
  },
  retry_prepare: () => null,
  playback_info: ({ trackId }) => {
    const { durationMs } = need(tracks.get(trackId));
    const chunks = Math.ceil((durationMs! / 1000) * 44_100 / CHUNK_FRAMES);
    return { chunkFrames: CHUNK_FRAMES, chunksTotal: chunks, chunksDone: chunks, durationMs };
  },
  chunk_pcm: () => new ArrayBuffer(CHUNK_FRAMES * 16),
  search: ({ input, imported }) => search(input, imported),
  link_preview: ({ onUpdate }) => onUpdate.onmessage({ title: "Made-up Clip", channel: "Someone Sings", durationMs: 185_000, thumbnail: null }),
  youtube_search: () => [{ url: "https://www.youtube.com/watch?v=madeup", title: "Made-up Clip", channel: "Someone Sings", durationMs: 185_000, thumbnail: null }],
  add_file: ({ path }) => {
    if (!paths.has(path)) paths.set(path, addTrack(path.split("/").pop()!.replace(/\.\w+$/, "")).id);
    return tracks.get(paths.get(path)!);
  },
  add_link: () => addTrack("Made-up Clip"),
  start_adding: ({ trackId }) => !ready.has(trackId) && !upcoming().includes(trackId),
  delete_track: ({ trackId }) => {
    tracks.delete(trackId);
    for (const p of playlists) p.trackIds = p.trackIds.filter((id) => id !== trackId);
  },
  create_playlist: ({ name, trackIds }) => {
    const id = nextId++;
    playlists.push({ id, name, user: true, trackIds });
    return id;
  },
  rename_playlist: ({ id, name }) => void (playlists.find((p) => p.id === id)!.name = name),
  delete_playlist: ({ id }) => void playlists.splice(playlists.findIndex((p) => p.id === id), 1),
  add_to_playlist: ({ playlistId, trackId }) => {
    const p = playlists.find((p) => p.id === playlistId)!;
    if (p.trackIds.includes(trackId)) return false;
    p.trackIds.push(trackId);
    return true;
  },
  remove_from_playlist: ({ playlistId, trackId }) => {
    const p = playlists.find((p) => p.id === playlistId)!;
    p.trackIds = p.trackIds.filter((id) => id !== trackId);
  },
  move_in_playlist: ({ playlistId, trackId, to }) => {
    const p = playlists.find((p) => p.id === playlistId)!;
    p.trackIds = p.trackIds.filter((id) => id !== trackId);
    p.trackIds.splice(to, 0, trackId);
  },
  playlists_with: ({ trackId }) => playlists.filter((p) => p.user && p.trackIds.includes(trackId)).map((p) => p.id),
  storage_info: () => storage,
  set_storage_limit: ({ bytes }) => void (storage = { ...storage, limitBytes: bytes }),
  clear_storage: () => (storage = { ...storage, usedBytes: 0 }),
};

mockWindows("main");
mockIPC(
  (cmd, args = {}) => {
    const run = commands[cmd];
    if (!run) throw new Error(`fake backend has no command ${cmd}`);
    fake.calls.push({ cmd, args: args as Record<string, unknown> });
    return run(args);
  },
  { shouldMockEvents: true },
);

/** Stands in for Web Audio: nothing sounds, and the clock only moves when a test sets `fake.time`. */
class FakeNode {
  gain = { setTargetAtTime() {} };
  connect() {}
  disconnect() {}
}

window.AudioContext = class {
  state = "running";
  destination = new FakeNode();
  get currentTime() {
    return fake.time;
  }
  resume = async () => {};
  createGain = () => new FakeNode();
  createBuffer = (_channels: number, length: number, rate: number) => ({ duration: length / rate, copyToChannel() {} });
  createBufferSource = () =>
    Object.assign(new FakeNode(), {
      buffer: null,
      onended: null,
      start: () => void (fake.started = true),
      stop() {},
    });
} as unknown as typeof AudioContext;
