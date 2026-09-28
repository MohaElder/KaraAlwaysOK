import type { LyricLine, Lyrics, PlayerSnapshot, SearchHit, SearchOutcome, Track, WebSource } from "$lib/api";

declare global {
  interface Window {
    fakePhone: typeof fakePhone;
  }
}

const song = (id: number, title: string, artist: string | null): Track => ({
  id, provider: "local", title, artist, album: null, durationMs: 200_000, vocalRemoval: 100, keySemitones: 0, instrumental: false, artworkPath: null, artSeed: id * 40,
});

const songs = [song(1, "Paper Boats", "Juniper Row"), song(2, "Kettle Duet", "Juniper Row"), song(3, "Lemon Skies", "The Porchlights")];

const line = (voice: LyricLine["voice"], ...words: [string, number, number][]): LyricLine => ({
  start_ms: words[0][1], end_ms: words.at(-1)![2], text: words.map((w) => w[0]).join(" "), voice, words: words.map(([text, start_ms, end_ms]) => ({ text, start_ms, end_ms })),
});

const lyrics: Record<number, Lyrics> = {
  1: { source: "lrclib", lines: [
    line(undefined, ["Fold", 5000, 5500], ["the", 5500, 6000], ["morning", 6000, 7000], ["paper", 7000, 8000]),
    line(undefined, ["Send", 8500, 9000], ["it", 9000, 9500], ["down", 9500, 10000], ["the", 10000, 10500], ["drain", 10500, 11000]),
    line(undefined, ["Wave", 20000, 20500], ["from", 20500, 21000], ["the", 21000, 21500], ["bridge", 21500, 22000]),
  ] },
  2: { source: "embedded", lines: [
    line("m", ["Who", 1000, 1500], ["boiled", 1500, 2000], ["the", 2000, 2500], ["water", 2500, 3000]),
    line("f", ["I", 3000, 3500], ["did,", 3500, 4000], ["of", 4000, 4500], ["course", 4500, 5000]),
    line("both", ["Tea", 5000, 5500], ["for", 5500, 6000], ["two", 6000, 7000]),
  ] },
};

const videos: Record<WebSource, SearchHit> = {
  youtube: { url: "https://www.youtube.com/watch?v=made-up", title: "Made-up Clip", channel: "Someone Sings", durationMs: 185_000, thumbnail: null },
  bilibili: { url: "https://www.bilibili.com/video/BV1madeup", title: "Made-up Cover", channel: "Someone Else", durationMs: 200_000, thumbnail: null },
};

function search(q: string): SearchOutcome {
  if (/^https?:\/\//.test(q)) {
    const host = new URL(q).hostname.replace(/^(www|open)\./, "");
    return /spotify/.test(host) ? { kind: "rejected", streaming: true, host } : { kind: "link", url: q, host };
  }
  const words = q.trim().toLowerCase();
  return { kind: "text", tracks: songs.filter((s) => s.title.toLowerCase().includes(words)), collections: [] };
}

let socket: FakeSocket | null = null;
let worklet: { port: { onmessage: ((e: MessageEvent) => void) | null } } | null = null;

/** The fake computer's side of the connection, and levers for tests through `window.fakePhone`. */
const fakePhone = {
  sent: [] as Record<string, unknown>[],
  frames: 0,
  full: false,
  unreachable: false,
  micAllowed: true,
  wakeLocks: 0,
  released: 0,
  /** True stops the computer's level messages, as a dead connection would. */
  quiet: false,
  audio: "running" as AudioContextState,
  snapshot: {
    entries: [{ key: 1, track: songs[0], by: null }, { key: 2, track: songs[2], by: "Ben" }, { key: 3, track: songs[1], by: null }],
    current: 0,
    ended: false,
    lyricOffsetMs: 0,
  } as PlayerSnapshot,
  /** A message from the computer. */
  server(msg: object) {
    socket?.deliver(msg);
  },
  /** The connection drops, as Wi-Fi does. */
  drop() {
    socket?.shut(1006);
  },
  /** The computer ends the session. */
  end() {
    socket?.shut(4001);
  },
  /** The computer plays song `id` alone; returns its queue key. */
  play(id: number) {
    const track = songs.find((s) => s.id === id)!;
    fakePhone.snapshot = { entries: [{ key: id * 10, track, by: null }], current: 0, ended: false, lyricOffsetMs: 0 };
    socket?.deliver({ t: "player", snapshot: fakePhone.snapshot });
    return id * 10;
  },
  /** The mic makes one frame of sound. */
  frame() {
    worklet?.port.onmessage?.({ data: new Int16Array(256).buffer } as MessageEvent);
  },
};
window.fakePhone = fakePhone;

class FakeSocket {
  static readonly OPEN = 1;
  readyState = 0;
  binaryType = "blob";
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onclose: ((e: { code: number }) => void) | null = null;
  private beat: ReturnType<typeof setInterval> | undefined;

  constructor(readonly url: string) {
    socket = this;
    setTimeout(() => {
      if (fakePhone.unreachable) return this.shut(1006);
      this.readyState = 1;
      this.onopen?.();
      this.beat = setInterval(() => !fakePhone.quiet && this.deliver({ t: "level", v: 0.2 }), 80);
    });
  }

  send(data: string | ArrayBuffer) {
    if (typeof data !== "string") return void fakePhone.frames++;
    const m = JSON.parse(data);
    fakePhone.sent.push(m);
    this.reply(m);
  }

  /** The computer's answers. */
  reply(m: Record<string, unknown>) {
    if (m.t === "search") return this.deliver({ t: "results", q: m.q, outcome: search(String(m.q)) });
    if (m.t === "web") return this.deliver({ t: "web", source: m.source, q: m.q, hits: m.q === "zzqx" ? [] : [videos[m.source as WebSource]] });
    if (m.t === "preview") return this.deliver({ t: "preview", url: m.url, preview: { title: "Made-up Song", channel: "Someone Else", durationMs: 200_000, thumbnail: null } });
    if (m.t === "lyrics") return this.deliver({ t: "lyrics", trackId: m.trackId, lyrics: lyrics[Number(m.trackId)] ?? { source: "none", lines: [] } });
    if (m.t !== "join") return;
    if (m.code !== "4827") return this.shut(4003);
    if (fakePhone.full) return this.shut(4002);
    this.deliver({ t: "joined" });
    this.deliver({ t: "player", snapshot: fakePhone.snapshot });
  }

  close() {
    this.shut(1000);
  }

  deliver(msg: object) {
    setTimeout(() => this.readyState === 1 && this.onmessage?.({ data: JSON.stringify(msg) }));
  }

  shut(code: number) {
    if (this.readyState === 3) return;
    this.readyState = 3;
    clearInterval(this.beat);
    setTimeout(() => this.onclose?.({ code }));
  }
}
window.WebSocket = FakeSocket as unknown as typeof WebSocket;

class FakeNode {
  gain = { value: 1 };
  connect<T>(next: T) {
    return next;
  }
  disconnect() {}
}

window.AudioContext = class {
  sampleRate = 48_000;
  destination = new FakeNode();
  audioWorklet = { addModule: async () => {} };
  get state() {
    return fakePhone.audio;
  }
  createGain = () => new FakeNode();
  createMediaStreamSource = () => new FakeNode();
  resume = async () => void (fakePhone.audio = "running");
  close = async () => {};
} as unknown as typeof AudioContext;

window.AudioWorkletNode = class extends FakeNode {
  port = { onmessage: null };
  constructor() {
    super();
    worklet = this;
  }
} as unknown as typeof AudioWorkletNode;

const track = { readyState: "live", stop() {} };
navigator.mediaDevices.getUserMedia = async () => {
  if (!fakePhone.micAllowed) throw new DOMException("Permission denied", "NotAllowedError");
  return { getAudioTracks: () => [track], getTracks: () => [track] } as unknown as MediaStream;
};
Object.defineProperty(navigator, "wakeLock", {
  configurable: true,
  value: {
    request: async () => {
      fakePhone.wakeLocks++;
      return { release: async () => void fakePhone.released++ };
    },
  },
});
