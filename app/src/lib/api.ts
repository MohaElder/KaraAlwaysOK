import { Channel, invoke } from "@tauri-apps/api/core";

export type ProblemCode =
  | "newerLibrary" | "dataFolder" | "inUse" | "libraryOpen" | "engineDownload" | "engineStart" | "fileMoved" | "fileNotAllowed"
  | "notAudio" | "songGone" | "noAudio" | "download" | "downloaderSetup" | "unreadable" | "songAudio" | "empty" | "diskFull"
  | "save" | "separate" | "notPrepared" | "partNotReady" | "streamingLater" | "upgradeFailed" | "linkStreaming"
  | "linkUnsupported" | "noSongAtLink" | "readFailed" | "nothingPlaying" | "notALink";

/** How every command fails: a problem code to translate, and the English text. */
export interface AppError { problem: ProblemCode | null; message: string }

export const systemLocale = () => invoke<string | null>("system_locale");

export interface SetupProgress { step: number; steps: number; done: number; total: number | null }

export async function setupEngine(onProgress: (p: SetupProgress) => void): Promise<void> {
  const channel = new Channel<SetupProgress>();
  channel.onmessage = onProgress;
  await invoke("setup_engine", { onProgress: channel });
}

export const startupProblem = () => invoke<AppError | null>("startup_problem");
