import type { PlayerSnapshot, ProblemCode } from "$lib/api";
import { say } from "$lib/i18n/engine";
import { toasts } from "$lib/state/toasts.svelte";
import { openMic, type Mic } from "./mic";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

export type Screen = "join" | "connecting" | "perm" | "mic" | "ended" | "lost" | "blocked";
export type Refusal = "wrongCode" | "full" | "unreachable";

const ENDED = 4001;
const FULL = 4002;
const WRONG_CODE = 4003;
const RETRY_MS = 2_000;
const GIVE_UP_MS = 120_000;
const SILENT_MS = 3_000;

type FromMac =
  | { t: "joined" }
  | { t: "player"; snapshot: PlayerSnapshot }
  | { t: "level"; v: number }
  | { t: "refused"; problem: ProblemCode | null };

function recall(storage: () => Storage, key: string): string | null {
  try {
    return storage().getItem(key);
  } catch {
    return null;
  }
}

function keep(storage: () => Storage, key: string, value: string) {
  try {
    storage().setItem(key, value);
  } catch {
    return;
  }
}

/** The phone's side of the session: joining, the mic, staying connected, and what the computer shares. */
class PhoneLink {
  screen = $state<Screen>("join");
  refusal = $state<Refusal | null>(null);
  reconnecting = $state(false);
  code = $state(new URLSearchParams(location.search).get("code") ?? "");
  readonly fromQr = this.code !== "";
  name = $state(recall(() => localStorage, "phone.name") ?? "");
  live = $state(false);
  level = $state(0);
  snapshot = $state<PlayerSnapshot | null>(null);

  private readonly id = recall(() => localStorage, "phone.id") ?? crypto.randomUUID();
  private socket: WebSocket | null = null;
  private mic: Mic | null = null;
  private wake: WakeLockSentinel | null = null;
  private heard = 0;
  private lostAt = 0;
  private retry: ReturnType<typeof setTimeout> | undefined;
  private asking = false;

  constructor() {
    keep(() => localStorage, "phone.id", this.id);
  }

  /** The join code's digits, as typed and sent. */
  get digits() {
    return this.code.replace(/\D/g, "");
  }

  /** Follows the page being hidden and shown and replaces a connection gone silent; returns the cleanup. */
  start() {
    const shown = () => this.shown();
    document.addEventListener("visibilitychange", shown);
    const beat = setInterval(() => this.beat(), 1_000);
    return () => {
      document.removeEventListener("visibilitychange", shown);
      clearInterval(beat);
    };
  }

  join() {
    if (this.screen !== "join") return;
    keep(() => localStorage, "phone.name", this.name.trim());
    this.refusal = null;
    this.screen = "connecting";
    this.connect();
  }

  /** Asks for the mic; the page then sends its sound while live. */
  async allowMic() {
    if (this.asking) return;
    this.asking = true;
    try {
      const mic = await openMic((pcm) => this.hear(pcm));
      if (!this.joined) return mic.stop();
      this.mic = mic;
      this.live = true;
      this.sendLive();
      this.screen = "mic";
    } catch {
      if (this.joined) this.screen = "blocked";
    } finally {
      this.asking = false;
    }
  }

  async toggleLive() {
    if (!this.mic) return;
    const on = !this.live;
    if (on) await this.mic.resume().catch(() => {});
    this.live = on && this.mic.running();
    this.sendLive();
  }

  leave() {
    this.send({ t: "leave" });
    this.stop();
    this.screen = "join";
  }

  again() {
    this.screen = "join";
  }

  private get joined() {
    return this.screen === "perm" || this.screen === "blocked" || this.screen === "mic";
  }

  private connect() {
    const ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`);
    ws.binaryType = "arraybuffer";
    this.socket = ws;
    this.heard = performance.now();
    ws.onopen = () => this.send({ t: "join", code: this.digits, id: this.id, name: this.name.trim() });
    ws.onmessage = (e) => {
      this.heard = performance.now();
      this.receive(JSON.parse(e.data));
    };
    ws.onclose = (e) => {
      if (this.socket === ws) this.closed(e.code);
    };
  }

  private receive(m: FromMac) {
    if (m.t === "joined") {
      if (this.reconnecting) this.reconnecting = false;
      else this.screen = "perm";
      this.sendSettings();
      void this.keepAwake();
    } else if (m.t === "player") this.snapshot = m.snapshot;
    else if (m.t === "level") this.level = m.v;
    else if (m.t === "refused") toasts.show(say(m), { icon: WarningIcon });
  }

  /** The connection closed with `code`: the session ended, the phone was refused, or it was lost and is tried again for two minutes. */
  private closed(code: number) {
    this.socket = null;
    if (code === ENDED || (this.reconnecting && code === WRONG_CODE)) return this.end();
    if (!this.joined) {
      this.refusal = code === FULL ? "full" : code === WRONG_CODE ? "wrongCode" : "unreachable";
      this.screen = "join";
      return;
    }
    if (!this.reconnecting) {
      this.reconnecting = true;
      this.lostAt = Date.now();
    }
    if (Date.now() - this.lostAt >= GIVE_UP_MS) return this.end("lost");
    this.retry = setTimeout(() => this.connect(), RETRY_MS);
  }

  private end(screen: "ended" | "lost" = "ended") {
    this.stop();
    this.screen = screen;
  }

  private stop() {
    clearTimeout(this.retry);
    const ws = this.socket;
    this.socket = null;
    ws?.close();
    this.mic?.stop();
    this.mic = null;
    void this.wake?.release().catch(() => {});
    this.wake = null;
    this.live = false;
    this.reconnecting = false;
    this.snapshot = null;
  }

  private hear(pcm: ArrayBuffer) {
    if (this.live && !this.reconnecting && this.socket?.readyState === WebSocket.OPEN) this.socket.send(pcm);
  }

  private sendLive() {
    if (this.mic) this.send({ t: "live", on: this.live, rate: this.mic.rate });
  }

  /** Sends what the computer keeps for this phone's row; called after every join. */
  private sendSettings() {
    this.sendLive();
  }

  /** Every second while joined: a ping so the computer knows the phone is there, and a connection that has said nothing for a few seconds counts as dropped. */
  private beat() {
    if (!this.joined || !this.socket) return;
    this.send({ t: "ping" });
    if (performance.now() - this.heard < SILENT_MS) return;
    const ws = this.socket;
    this.socket = null;
    ws.close();
    this.closed(1006);
  }

  /** Back on screen: keeps the screen awake again, and a mic the lock screen stopped waits for a tap. */
  private shown() {
    if (document.visibilityState !== "visible" || !this.joined) return;
    void this.keepAwake();
    if (this.screen === "mic" && this.live && this.mic && !this.mic.running()) {
      this.live = false;
      this.sendLive();
    }
  }

  private async keepAwake() {
    try {
      const wake = await navigator.wakeLock?.request("screen");
      if (!this.joined) return void wake?.release();
      void this.wake?.release().catch(() => {});
      this.wake = wake ?? null;
    } catch {
      return;
    }
  }

  private send(m: object) {
    if (this.socket?.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(m));
  }
}

export const link = new PhoneLink();
