import type { PlayerSnapshot, Track } from "$lib/api";

declare global {
  interface Window {
    fakePhone: typeof fakePhone;
  }
}

const song = (id: number, title: string, artist: string | null): Track => ({
  id, provider: "local", title, artist, album: null, durationMs: 200_000, vocalRemoval: 100, keySemitones: 0, instrumental: false, artworkPath: null, artSeed: id * 40,
});

const songs = [song(1, "Paper Boats", "Juniper Row"), song(2, "Kettle Duet", "Juniper Row"), song(3, "Lemon Skies", "The Porchlights")];

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
