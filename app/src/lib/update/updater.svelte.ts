import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "available"; version: string; notes: string }
  | { kind: "downloading"; fraction: number }
  | { kind: "upToDate" }
  | { kind: "error" }
  | { kind: "installFailed" };

const DAY_MS = 86_400_000;

function read(key: string): string {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    return;
  }
}

class Updater {
  state = $state<UpdateState>({ kind: "idle" });
  private pending: Update | null = null;

  /** At launch: at most once a day, quietly, and never for a skipped version. */
  async autoCheck() {
    if (!isTauri() || Date.now() - Number(read("update.lastCheck") || 0) < DAY_MS) return;
    write("update.lastCheck", String(Date.now()));
    const update = await check().catch(() => null);
    if (update && update.version !== read("update.skip")) this.offer(update);
  }

  /** From Settings: always checks, and says when there is nothing new. */
  async manualCheck() {
    write("update.lastCheck", String(Date.now()));
    try {
      const update = await check();
      if (update) this.offer(update);
      else this.state = { kind: "upToDate" };
    } catch {
      this.state = { kind: "error" };
    }
  }

  /** Downloads and installs the offered update, then restarts the app. */
  async install() {
    if (!this.pending) return;
    this.state = { kind: "downloading", fraction: 0 };
    let total = 0;
    let got = 0;
    try {
      await this.pending.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          if (this.state.kind === "downloading") this.state = { kind: "downloading", fraction: total ? got / total : 0 };
        }
      });
      await relaunch();
    } catch {
      this.state = { kind: "installFailed" };
    }
  }

  skip() {
    if (this.state.kind === "available") write("update.skip", this.state.version);
    this.dismiss();
  }

  dismiss() {
    this.state = { kind: "idle" };
  }

  private offer(update: Update) {
    this.pending = update;
    this.state = { kind: "available", version: update.version, notes: update.body ?? "" };
  }
}

export const updater = new Updater();
