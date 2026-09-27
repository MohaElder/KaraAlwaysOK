import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ProblemCode =
  | "newerLibrary" | "dataFolder" | "inUse" | "libraryOpen" | "engineDownload" | "engineStart" | "fileMoved" | "fileNotAllowed"
  | "notAudio" | "songGone" | "noAudio" | "download" | "downloaderSetup" | "unreadable" | "songAudio" | "empty" | "diskFull"
  | "save" | "separate" | "notPrepared" | "partNotReady" | "streamingLater" | "upgradeFailed" | "linkStreaming"
  | "linkUnsupported" | "noSongAtLink" | "lyricsLookup" | "readFailed" | "nothingPlaying" | "notALink";

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

export type Kind = "playlist" | "album" | "artist";

export interface Track {
  id: number;
  provider: "local" | "spotify" | "apple";
  title: string;
  artist: string | null;
  album: string | null;
  durationMs: number | null;
  vocalRemoval: number;
  keySemitones: number;
  instrumental: boolean;
  artworkPath: string | null;
  artSeed: number;
}

export interface CollectionCard {
  id: number;
  provider: Track["provider"];
  kind: Kind;
  name: string;
  subtitle: string | null;
  user: boolean;
  count: number;
  covers: Track[];
}

export interface CollectionPage { card: CollectionCard; tracks: Track[] }

export interface Word { start_ms: number; end_ms: number; text: string }
export interface LyricLine { start_ms: number; end_ms: number; text: string; words: Word[]; voice?: "m" | "f" | "both" }
export interface Lyrics { source: "lrclib" | "embedded" | "none" | null; lines: LyricLine[] }

export const listCollections = (kind: Kind) => invoke<CollectionCard[]>("list_collections", { kind });
export const openCollection = (id: number) => invoke<CollectionPage>("open_collection", { id });
export const getTrack = (trackId: number) => invoke<Track>("get_track", { trackId });
export const editTrack = (trackId: number, title: string, artist: string, album: string) => invoke<Track>("edit_track", { trackId, title, artist, album });
export const trackLyrics = (trackId: number) => invoke<Lyrics>("track_lyrics", { trackId });
export const findLyricsAgain = (trackId: number) => invoke<boolean>("find_lyrics_again", { trackId });

export interface QueueEntry { key: number; track: Track }
export interface PlayerSnapshot { entries: QueueEntry[]; current: number | null; ended: boolean; lyricOffsetMs: number }

export const playerState = () => invoke<PlayerSnapshot>("player_state");
export const queueAdd = (trackId: number, next: boolean) => invoke<PlayerSnapshot>("queue_add", { trackId, next });
export const playTracks = (trackIds: number[], start: number) => invoke<PlayerSnapshot>("play_tracks", { trackIds, start });
export const skipSong = (delta: number) => invoke<PlayerSnapshot>("skip", { delta });
export const songEnded = () => invoke<PlayerSnapshot>("song_ended");
export const queueMove = (key: number, to: number) => invoke<PlayerSnapshot>("queue_move", { key, to });
export const queueRemove = (key: number) => invoke<PlayerSnapshot>("queue_remove", { key });
export const setSinger = (value: number) => invoke<PlayerSnapshot>("set_singer", { value });
export const setKey = (semitones: number) => invoke<PlayerSnapshot>("set_key", { semitones });
export const setLyricOffset = (ms: number) => invoke<PlayerSnapshot>("set_lyric_offset", { ms });
export const retryPrepare = () => invoke<void>("retry_prepare");
export const onPlayer = (cb: (s: PlayerSnapshot) => void) => listen<PlayerSnapshot>("player", (e) => cb(e.payload));

export interface PlaybackInfo { chunkFrames: number; chunksTotal: number | null; chunksDone: number; durationMs: number | null }

export const playbackInfo = (trackId: number) => invoke<PlaybackInfo>("playback_info", { trackId });
export const chunkPcm = (trackId: number, index: number) => invoke<ArrayBuffer>("chunk_pcm", { trackId, index });

export type EngineEvent =
  | { kind: "stage"; trackId: number; stage: "fetching" | "standardizing" | "findingLyrics" | "separating" }
  | { kind: "progress"; trackId: number; chunksDone: number; chunksTotal: number }
  | { kind: "lyrics"; trackId: number }
  | { kind: "renamed"; trackId: number }
  | { kind: "lyricOffset"; trackId: number }
  | { kind: "added"; trackId: number }
  | { kind: "ready"; trackId: number }
  | { kind: "failed"; trackId: number; message: string; problem: ProblemCode | null };

export const onEngine = (cb: (e: EngineEvent) => void) => listen<EngineEvent>("engine", (e) => cb(e.payload));

export interface LinkPreview { title: string; channel: string | null; durationMs: number | null; thumbnail: string | null }
export interface SearchHit extends LinkPreview { url: string }

export type SearchOutcome =
  | { kind: "text"; tracks: Track[]; collections: CollectionCard[] }
  | { kind: "link"; url: string; host: string }
  | { kind: "rejected"; streaming: boolean; host: string };

export const search = (input: string, imported: string) => invoke<SearchOutcome>("search", { input, imported });

export async function linkPreview(url: string, onUpdate: (p: LinkPreview) => void): Promise<void> {
  const channel = new Channel<LinkPreview>();
  channel.onmessage = onUpdate;
  await invoke("link_preview", { url, onUpdate: channel });
}

export const youtubeSearch = (query: string) => invoke<SearchHit[]>("youtube_search", { query });
export const youtubeSuggestions = (query: string) => invoke<string[]>("youtube_suggestions", { query });

export const addFile = (path: string) => invoke<Track>("add_file", { path });
export const addLink = (url: string) => invoke<Track>("add_link", { url });
export const startAdding = (trackId: number) => invoke<boolean>("start_adding", { trackId });
export const deleteTrack = (trackId: number) => invoke<void>("delete_track", { trackId });

export const createPlaylist = (name: string, trackIds: number[]) => invoke<number>("create_playlist", { name, trackIds });
export const renamePlaylist = (id: number, name: string) => invoke<void>("rename_playlist", { id, name });
export const deletePlaylist = (id: number) => invoke<void>("delete_playlist", { id });
export const addToPlaylist = (playlistId: number, trackId: number) => invoke<boolean>("add_to_playlist", { playlistId, trackId });
export const removeFromPlaylist = (playlistId: number, trackId: number) => invoke<void>("remove_from_playlist", { playlistId, trackId });
export const moveInPlaylist = (playlistId: number, trackId: number, to: number) => invoke<void>("move_in_playlist", { playlistId, trackId, to });
export const playlistsWith = (trackId: number) => invoke<number[]>("playlists_with", { trackId });

export interface StorageInfo { usedBytes: number; limitBytes: number }

export const storageInfo = () => invoke<StorageInfo>("storage_info");
export const setStorageLimit = (bytes: number) => invoke<void>("set_storage_limit", { bytes });
export const clearStorage = () => invoke<StorageInfo>("clear_storage");
export const reduceTransparency = () => invoke<boolean>("reduce_transparency");
