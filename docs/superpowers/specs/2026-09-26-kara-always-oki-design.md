# kara-always-oki — Design

Date: 2026-09-26
Status: approved in brainstorming, pending written-spec review

## 1. What it is

A desktop karaoke app. Pick any song and sing over it: the app strips the vocals (fully, or partly via a slider) and shows synced lyrics. Songs come from your own files, links, and later your Spotify / Apple Music library. Friends join by scanning a QR code and use their phones as live microphones.

- Open-source, personal use, distributed as GitHub release downloads.
- Target: MVP runs on the author's MacBook (Apple Silicon). Windows (and other hardware) is planned for the public release. No enforced hardware limits: measured requirements are published as recommended specs in README.md.
- Local-only: no servers we run, nothing we pay for. Audio, stems and lyrics stay on the computer.

### Success criteria (MVP)

1. Drop a file or paste a supported link → singing starts within ~10 s.
2. Vocal slider changes the mix live without restarting the song; slider at 0 is the original.
3. Separation stays ahead of playback for a normal song.
4. Synced lyrics show for songs LRCLIB or the file's tags know.

## 2. Experience (settled via prototype)

Reference prototype: `docs/prototype/experience.html` (also published as a private artifact). All songs and lyrics in it are made up; every song plays one demo tune.

**Library** — modeled on the Apple Music Mac app, styled after cosmos.so (quiet, image-led, grotesk type).
- Sidebar, three layers:
  1. Provider switcher: icon buttons (All icon, Apple Music logo, Spotify logo, Local/folder icon) with the name as a tooltip. Default is All, which merges connected providers.
  2. Tabs: **Playlists · Albums · Artists**, each with its icon beside the label.
  3. List of collections; selecting one shows its songs in the main pane (cover, title, *Sing* / *Shuffle*, song rows showing only artwork, title, artist – album, and length). No per-song processing status in lists: preparation happens while you play.
- In All, each collection shows small provider dots. Artists with the same name merge across providers.
- A disconnected provider shows a *Connect* prompt in its view.

**Search bar** — "Search anything, or paste a link". It is the app's main entry point: large and prominent at the top center.
- Text → search across all connected providers (songs, plus matching playlists/albums/artists).
- Link → a supported link shows a preview as a search result: the video/track thumbnail, its title, the channel and the duration, with the same add-to-queue / play-next actions (metadata from YouTube/SoundCloud/Vimeo oEmbed, falling back to `yt-dlp --dump-json`). No explanatory text.
- Link → accepted only if an open-source tool can pull audio from it (yt-dlp–supported sites such as YouTube, SoundCloud, Bandcamp, Vimeo, archive.org, Mixcloud) or it points directly to an audio file. Enter processes it.
- Spotify / Apple Music / other streaming links are rejected with a plain explanation and a nudge to search instead.
- Files can be dropped anywhere in the window.
- Added songs land in **Local › Imported**.

**Adding a song** — adding a link or file never auto-plays and never blocks the screen. Progress shows unobtrusively (a small progress toast and a loading row in Local › Imported): download → prepare the audio → find the lyrics. When done, a toast "Added to your library" with the song's thumbnail; tapping it opens Local › Imported at that song. Taking the vocals out happens when the song is played (playback starts once the first ~6 s are separated; separation runs ahead of the playhead; if playback catches up it pauses with "Catching up…"; you can't seek past what's ready). Link results offer Add to library (default), Add to queue, and Play next.

**Player bar** — floating, like Apple Music's mini player: previous / play / next, artwork + title (click to open/close the karaoke view), a thin progress line whose lighter band shows how far ahead the song is prepared, the vocal slider, and a "…" menu.

**Vocal slider** — one thin Apple-style slider with a mic icon. Fully left = off (original vocals, icon greys out). Fully right = instrumental (default). No presets.

**"…" menu** — Key (−6…+6 semitones), Lyrics timing (±0.1 s steps), which audio version a streaming song uses (phase 3), where lyrics came from.

**Karaoke view** — full screen, blurred artwork background, big lyrics with word-by-word fill, current line centered. Top-right: the mic pill. Collapse chevron returns to the library; music keeps playing.

**Managing the library** — local songs can be edited (title, artist, album, artwork) and deleted (which frees their storage; undo via toast). Playlists are user-made, live under Local, and can hold songs from any source (Apple Music, Spotify and Local mixed); they can be created, renamed, reordered, and deleted, and removing a song from a playlist never deletes the song. Every song row has a menu: Play next, Add to queue, Add to playlist, and for local songs Edit info and Delete. Streaming providers' own playlists stay read-only.

**Queue** — tapping a song row adds it to the end of the queue. If nothing is playing (empty or finished queue), it starts right away and opens the full karaoke view. A second button on each row, "Play next", puts it right after the current song. The player bar has a queue button on its right: it opens the queue list, where songs can be dragged to reorder or removed. Guests can also search and add songs to the queue from their phone.

**Karaoke view, details** — lyrics are centered. Before every sung part (the intro and each instrumental gap longer than ~3 s) show a 3-dot countdown, like Apple Music. The background is an animated gradient built from the album artwork's colors, slowly moving, like Apple Music. Duets: when the lyrics mark who sings a line (e.g. male / female / together, from LRC or TTML voice tags), show the parts distinctly (different alignment and tint); without such marks, lines show normally.

**Glass rule** — anything that floats on top of other content is glass (translucent, backdrop blur, hairline edge, no heavy shadow): the player bar and its buttons, the karaoke top controls, the sticky search bar and mic pill, menus, the queue panel, toasts, tooltips, and the phone's bottom bar and banners. Windows/sheets and plain surfaces (the sidebar, tabs) are not glass. Fall back to an opaque fill when the system asks for reduced transparency.

**Selection style** — selected items (sidebar, lists) use a plain tinted overlay with no shadow, like Apple Music.

**Mic pill** — top-right in both library and karaoke view. Handheld-karaoke-mic icon + number of joined phones, or a **+** when none. Opens the connect window: QR code, join code, a short tutorial for the one-time browser warning (iPhone: *Show Details → visit this website*; Android: *Advanced → Proceed*), and the list of joined phones with a level meter, volume and Remove.

**Icon-first rule** — use icons wherever an icon can carry the meaning, and show nothing that doesn't help the user act. Every label gets an icon beside it, except the core song info (song title, artist, album). Providers are shown by their logos. No status or detail that the user can't act on (e.g. no "Ready" / "Preparing" badges in song lists).

**Copy rule** — no engineering vocabulary in the UI (no "stems", "FLAC", "LRCLIB", "chunks", IPs/ports).

**Appearance rule** — light and dark mode, following the macOS system setting live (no in-app toggle needed). Every color comes from theme tokens defined for both modes; the karaoke view stays dark in both.

**Icons** — Phosphor Icons (MIT), Bold weight everywhere. Not Lucide.

**Design tokens** (picked 2026-09-27 in the design kit artifact https://claude.ai/artifact/RgkXxGA3WFmUDSvG2NepsE):
- Fonts: Geist (body), Bricolage Grotesque 800, tracking −0.02em (display: titles, lyrics), Geist Mono (captions, numbers).
- Type scale: lyric line 40, page title 28, section 17/600, body 14, caption 11/500 caps +0.07em, numbers 12 mono tabular.
- Spacing (Comfortable): 4, 8, 12, 16, 20, 24, 32, 40 px; radius 8 small / 14 large; list rows 52 px.
- Colors: "Walkman" — Sony TPS-L2 blue-grey body as the secondary, hotline-button orange as the accent.

| Token | Light | Dark |
|---|---|---|
| bg | #EEF1F4 | #10151B |
| side (blue-grey) | #D6DFE8 | #18212B |
| surface | #FFFFFF | #1B232D |
| raised | #DCE4EC | #243040 |
| line | #C9D3DE | #2D3948 |
| text | #1B2430 | #E8EEF4 |
| muted | #5B6878 | #8C9AAB |
| faint | #97A3B1 | #566374 |
| accent (hotline orange) | #DE6414 | #FF8C2E |
| on-accent | #FFFFFF | #1F0E02 |
| ready | #2F7D5B | #5FBF93 |
| busy | #A86B12 | #E3A948 |

The karaoke view stays dark in both modes and uses the dark accent.

**Motion rule** — every interaction animates: showing or hiding any view, sheet, menu, popover, button or status, and every state change. Use only two simple, consistent transitions: a fade, or a short slide (paired with a fade) along the direction the element comes from. Same durations and easing everywhere; they should feel natural, never showy. Respect the system "reduce motion" setting (fall back to fade only).

## 3. Architecture

Pipeline: **provider → processor → streamer → UI**. Rust does fetching, decoding and ML; the web view does playback and mixing.

```
kara-always-oki/
├─ crates/
│  ├─ kara-core      pure Rust, no Tauri
│  │  ├─ provider/   Provider trait → Local (MVP); Spotify, Apple Music (phase 3)
│  │  ├─ ingest/     file or link → decode → standard audio (FLAC, 44.1 kHz stereo)
│  │  ├─ separate/   ONNX Runtime session (CoreML), overlapping chunk plan, writes stem chunks
│  │  ├─ lyrics/     LRCLIB client, LRC + embedded-tag parsing, word timing
│  │  ├─ library/    SQLite (rusqlite), migrations, FTS5 search
│  │  ├─ cache/      disk budget, LRU eviction, startup cleanup
│  │  └─ jobs/       single worker, cancellable, priority = current song
│  └─ kara-cli       `kara ingest | separate | bench` — headless use and benchmarks
└─ app/
   ├─ src-tauri/     Tauri 2: commands split per module, progress events, binary chunk transfer;
   │                 phase 2: phone-mic HTTPS server (axum + rustls, rcgen self-signed cert) + WebSocket signaling
   └─ src/           Svelte 5 + SvelteKit (adapter-static) UI and Web Audio engine
```

### 3.1 Provider
`Provider` lists collections and tracks and says how to get audio for a track.
- **Local (MVP):** files the user dropped and links they pasted. Ingest creates album/artist collections from tags, so every provider produces collections the same way.
- **Spotify (phase 3):** Web API with OAuth PKCE; playlists, saved albums, followed artists. Audio comes from a matched upload (yt-dlp search), user can switch the match.
- **Apple Music (phase 3):** read the library from the Music app on the Mac (scripting bridge), avoiding the paid developer token. Same matching as Spotify.

### 3.2 Processor
One job chain per song, run by `jobs/` (one worker; switching songs cancels the old chain; current song first; when it finishes, the next queued song starts preparing).
1. **Fetch** — files: read in place. Links: yt-dlp downloads audio as M4A into a temp dir.
2. **Standardize** — decode with symphonia (pure Rust; MP3, AAC/M4A, FLAC, WAV, Vorbis, and audio inside MP4), resample with rubato to 44.1 kHz stereo (in memory, for separation and playback; the original file is what's kept on disk), compute `audio_hash` (SHA-256 of the decoded PCM). Same hash → reuse existing stems.
3. **Lyrics** (in parallel with separation) — embedded synced lyrics first, then LRCLIB by title + artist + duration. Line-level timings get even word timing across each line. Not found → stored as `none`, song still plays.
4. **Separate** — 10 s chunks with overlap for model context, cropped back to seamless edges (same idea as filmrev's `plan_tiles`). Model outputs vocals; instrumental = mix − vocals, so vocals + instrumental = original. Each chunk writes only `NNNN.vocals.flac` (24-bit, 6 dB headroom so it never clips) via `.part` + rename; the instrumental is computed live at playback as original − vocals. Progress events after each chunk.

Only yt-dlp is an external binary; no ffmpeg.

### 3.3 Streamer (web view)
- Fetches chunk pairs ahead of the playhead as raw bytes (Tauri `ipc::Response` / `Channel`, never JSON/base64), decodes, and schedules them back-to-back on the `AudioContext` clock.
- Mix graph: `vocals → gain(1 − slider)` + `instrumental` → key shift (signalsmith-stretch in an AudioWorklet) → output. Phase 2 adds phone-mic inputs (WebRTC) with reverb into the same output.
- Keeps a sliding window of decoded chunks (previous 1 + next 3 ≈ 50 MB).
- The audio clock drives lyric highlighting (with the source's lyric offset).

### 3.4 Phone mics (phase 2)
- The app runs an HTTPS server on the LAN with a self-signed certificate it generates on first launch (browsers only allow microphone access on HTTPS). The QR code encodes that address plus a join code.
- The phone page (bundled static page) captures the mic with echo cancellation on and connects by WebRTC to the desktop web view; signaling runs over the app's own WebSocket. Audio goes phone → Mac directly over Wi-Fi.
- Voices play live through the computer speakers with reverb. Expected delay ~40–100 ms.
- No external servers, no native phone app.

### 3.5 ML runtime (patterns from filmrev)
- `ort` 2.x with `load-dynamic`, CoreML execution provider on macOS.
- ONNX Runtime dylib and the model are downloaded on first use: manifest with SHA-256, verify in memory, write `.part`, rename, stream progress. The dylib must be re-signed with the app's Developer ID under Hardened Runtime.
- Session created once and reused (filmrev rebuilds per call — don't).
- Inference runs in `spawn_blocking`/a dedicated thread, never on the async runtime.
- Input/output names and shapes read from the model at load time.
- Memory sampler writes a periodic `MEM` line to the debug log.
- Release profile: `lto = "thin"`, `codegen-units = 1`; dev profile builds dependencies at `opt-level = 3`.

### 3.6 Model choice (spike, first plan task)
Benchmark via `kara bench`: an MDX-Net vocals ONNX model vs. an ONNX export of HTDemucs. Measure speed as a multiple of real time, peak memory, quality on a fixed test set, and license compatibility. Pick the winner; it must be ≥ 2× real time. Peak memory is recorded for the README's recommended specs, not enforced.

## 4. Data schema (SQLite)

```sql
provider_account(
  provider      TEXT PRIMARY KEY CHECK (provider IN ('spotify','apple')),
  display_name  TEXT,
  connected_at  INTEGER                 -- tokens live in the macOS Keychain
);

track(
  id            INTEGER PRIMARY KEY,
  provider      TEXT NOT NULL,          -- 'local' | 'spotify' | 'apple'
  provider_ref  TEXT,                   -- NULL for local
  title         TEXT NOT NULL,
  artist        TEXT,
  album         TEXT,
  duration_ms   INTEGER,
  artwork_path  TEXT,
  added_at      INTEGER NOT NULL,
  vocal_removal INTEGER NOT NULL DEFAULT 100,  -- 0 = original, 100 = instrumental
  key_semitones INTEGER NOT NULL DEFAULT 0,    -- −6 … +6
  UNIQUE (provider, provider_ref)
);

collection(
  id            INTEGER PRIMARY KEY,
  provider      TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('playlist','album','artist')),
  provider_ref  TEXT,
  name          TEXT NOT NULL,
  subtitle      TEXT,
  artwork_path  TEXT,
  UNIQUE (provider, kind, provider_ref)
);

collection_track(
  collection_id INTEGER NOT NULL REFERENCES collection ON DELETE CASCADE,
  track_id      INTEGER NOT NULL REFERENCES track ON DELETE CASCADE,
  position      INTEGER NOT NULL,
  PRIMARY KEY (collection_id, track_id)
);

audio_source(
  id              INTEGER PRIMARY KEY,
  track_id        INTEGER NOT NULL REFERENCES track ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('file','link','match')),
  uri             TEXT NOT NULL,        -- original path or URL
  label           TEXT,
  duration_ms     INTEGER,
  selected        INTEGER NOT NULL DEFAULT 0,
  audio_hash      TEXT,                 -- NULL until standardized
  lyric_offset_ms INTEGER NOT NULL DEFAULT 0,
  status          TEXT NOT NULL CHECK (status IN ('pending','fetching','ready','failed')),
  error           TEXT
);
CREATE UNIQUE INDEX one_selected_source ON audio_source(track_id) WHERE selected = 1;

separation(
  audio_hash    TEXT NOT NULL,
  model_id      TEXT NOT NULL,
  chunk_ms      INTEGER NOT NULL,
  chunks_total  INTEGER NOT NULL,
  chunks_done   INTEGER NOT NULL DEFAULT 0,
  status        TEXT NOT NULL CHECK (status IN ('queued','running','ready','failed','cancelled')),
  size_bytes    INTEGER NOT NULL DEFAULT 0,
  last_used_at  INTEGER,
  PRIMARY KEY (audio_hash, model_id)
);

lyrics(
  track_id   INTEGER PRIMARY KEY REFERENCES track ON DELETE CASCADE,
  source     TEXT NOT NULL CHECK (source IN ('lrclib','embedded','none')),
  lines      TEXT,   -- JSON [{start_ms,end_ms,text,words:[{start_ms,end_ms,text}]}]
  fetched_at INTEGER NOT NULL
);

setting(key TEXT PRIMARY KEY, value TEXT);   -- cache_budget_bytes, model_id, …

CREATE VIRTUAL TABLE track_fts USING fts5(title, artist, album, content='track', content_rowid='id');
```

Rules:
- UI status is derived, not stored: *New* (no separation row), *Preparing N%* (`chunks_done / chunks_total`), *Ready* (`status = 'ready'`).
- The vocal slider and key follow the track; lyric timing follows the audio source (a version with an intro shifts it).
- Not persisted: mic sessions, play queue, playback position.

### Files on disk (`~/Library/Application Support/kara-always-oki/`)
```
kara.db
runtime/libonnxruntime.dylib
models/<model_id>.onnx
bin/yt-dlp
tmp/                                          downloads in flight
audio/<audio_hash>/original.<ext>             the fetched file as-is (kept; the instrumental is derived from it)
audio/<audio_hash>/<model_id>/NNNN.vocals.flac
artwork/<sha>.jpg
```

## 5. Cache

- **Original + vocals only** (decided 2026-09-27). Keep the fetched file as-is and store only the vocals chunks; the instrumental is original − vocals, computed at playback. About 15–25 MB per 4-minute song, no clipping (vocals stored 24-bit with 6 dB headroom), and slider-at-0 plays the exact original. Switching models re-separates from the original.
- **Budget + LRU.** Default 5 GB (setting `cache_budget_bytes`). Over budget → delete stems of the least recently played songs (`last_used_at`); the track stays and goes back to *New*. Never evicted: the playing song, queued songs, and local files whose original has moved or been deleted.
- **Kept forever:** lyrics (including `none`, retried after 7 days) and artwork.
- **Outside the budget:** model, ONNX Runtime, yt-dlp.
- **Startup cleanup:** delete `.part` files and `tmp/`; recount `chunks_done` from complete chunk pairs on disk.
- **Web view memory:** sliding window of decoded chunks, see 3.3.

## 6. Error handling

- Link unsupported → rejected in the search bar before any work (see §2).
- Download fails / file unreadable → source `failed` with a plain message in the preparing sheet; nothing else changes.
- Separation fails mid-song → playback pauses at the last ready chunk with a message; retry restarts from the first missing chunk.
- Model / runtime / yt-dlp download fails or hash mismatch → retry prompt; nothing half-written is used.
- No lyrics → song plays, karaoke view says "No lyrics found".
- yt-dlp breaks because a site changed → self-update, then retry once.

## 7. Testing

- `kara-core`: unit tests on pure functions (chunk planning coverage and seams, LRC parsing, word timing, link verdicts, eviction choice, schema migrations) using `tempfile`.
- Seam test: separate a known file, sum vocals + instrumental, compare to the source within 16-bit rounding.
- `kara bench`: speed (× real time) and peak memory; used for the model spike and as a regression check.
- Frontend: vitest for the chunk scheduler, lyric timing and search/link handling.

## 8. Phases

1. **MVP** — Local provider (file drop, pasted link), processor, streamer, library + search, karaoke view, vocal slider, key, lyric timing, cache. Starts with the model spike.
2. **Phone mics** — HTTPS LAN server, QR join + tutorial, WebRTC into the mix, live through the speakers.
3. **Spotify + Apple Music** — providers, library sync, match to uploads, match switching.

Each phase gets its own implementation plan.

## 9. Out of scope (for now)

- Lyric timing from audio (Whisper) when no synced lyrics exist.
- Recording or exporting performances.
- Windows / Linux builds (planned for the public release).
- Mics plugged into the Mac.
- Scoring, duets, multiple lyric tracks.
