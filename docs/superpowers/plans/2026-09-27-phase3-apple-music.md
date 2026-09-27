# Phase 3 — Apple Music Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The user's Apple Music playlists, albums and artists appear in KaraAlwaysOK after one login; tapping one of their songs finds a YouTube upload of it (official audio by ISRC first), prepares and plays it like any song, "Change match…" fixes wrong picks, animated covers loop where Apple has them, and Apple Music songs and phrases show up in search.

**Architecture:** kara-core gains `apple` (a read-only client for `amp-api.music.apple.com` that uses the web player's own tokens, the macOS Keychain, parsers for Apple's answers, the library sync and catalog look-ups) and `matching` (YouTube Music songs by ISRC, then YouTube videos for "artist title", scored and filtered). One new library upgrade step adds the few columns this needs; matches are ordinary audio sources of kind `match`, so lyric timing stays per upload. The app gains `sources.rs` (an incognito Tauri login window polled with `eval_with_callback` — Apple's page gets no IPC — plus the status event and a background refresh) and `matches.rs`; the frontend adds Settings › Sources, the provider switcher, the Change match sheet, a `MotionArt` video layer and Apple results in search.

**Tech Stack:** Rust 2021 (rusqlite, reqwest blocking + rustls, serde_json, `security-framework` 3 for the Keychain — already in the lock file), Tauri 2.12 (`WebviewWindowBuilder::incognito`, `on_new_window`, `eval_with_callback`, `cookies_for_url`), Phase 2's shared YouTube search, artwork helper and isolated check, Svelte 5 + SvelteKit 2, vitest, Playwright with the faked backend.

**Spec:** `docs/superpowers/specs/2026-09-27-phase3-streaming-providers-design.md` (authority). Runs after `docs/superpowers/plans/2026-09-27-phase2-phone-mics.md` (Phase 2) and builds on its code. Its scope note rules: **Apple Music only**; everything about Spotify is deferred and not built here. Built on `docs/superpowers/specs/2026-09-26-kara-always-oki-design.md` (UI, copy, language and engineering rules; "Suggestions while typing"). Evidence: `docs/superpowers/spikes/2026-09-27-streaming-providers.md` (the web-player route, verified with the user's account). UX reference: `docs/prototype/hifi.html` — the provider switcher (`#prov`, `.seg`, `PROV`, `plogos`) and the Sources rows (`renderSources`, `.srow`).

**Probed while writing this plan (2026-09-27, public data only, no account):**
- `amp-api.music.apple.com` answers the web player's requests only with the `Origin: https://music.apple.com` header (401 without it). The app takes the developer token only from the login window; it never reads it from Apple's page itself.
- `GET /v1/catalog/us/songs?filter[isrc]=…` works; catalog songs carry `isrc` and `url` = `https://music.apple.com/us/album/<slug>/<albumId>?i=<songId>` (the album id without another request).
- `GET /v1/catalog/us/search/suggestions?term=…&kinds=terms` answers `results.suggestions[]` with `kind`, `searchTerm`, `displayTerm`.
- `extend=editorialVideo` works on `/v1/catalog/us/albums/<id>` and `/v1/catalog/us/playlists/<pl.id>`; it gives `motionDetailSquare`, `motionSquareVideo1x1`, `motionDetailTall`, `motionTallVideo3x4` (playlists also `motionWideVideo21x9`), each `{ previewFrame, video }` where `video` is an HLS `.m3u8` on `mvod.itunes.apple.com` that loads without any header.
- `POST https://music.youtube.com/youtubei/v1/search` with client `WEB_REMIX` and the songs filter answers an ISRC query in ~0.5 s: a `musicShelfRenderer` "Songs" whose first row was the right song (title; then `artist • album • 5:55` runs, the album run with `pageType: MUSIC_PAGE_TYPE_ALBUM`) followed by unrelated rows (videos with `… views`) — so every candidate must be scored.
- Tauri 2.12 on macOS: `on_new_window(|_, _| NewWindowResponse::Allow)` opens popups sharing the opener's `WKWebViewConfiguration` (so the same incognito data store); `eval_with_callback` uses `evaluateJavaScript` and hands back JSON (`""` for `null`/`undefined`).
- Library resources (`/v1/me/library/…`) need the user's token and were verified only by the spike; their samples below follow Apple's documented `LibraryPlaylists` / `LibraryAlbums` / `LibrarySongs` shapes and Cider's request parameters. Task 5's ignored live test and the final checklist check them against the real account.

## Global Constraints

- Preconditions: Phase 2 (`docs/superpowers/plans/2026-09-27-phase2-phone-mics.md`) is finished — its last task done and reviewed — on `phase2-phone-mics`, which includes Phase 1b through Task 33 (`285675b`: `info_edited`, the lyrics swap-retry, keeping a song's info on re-download). Phase 3 works in its own git worktree (superpowers:using-git-worktrees) on branch `phase3-apple-music` made from the then-current tip of `phase2-phone-mics`. It reuses Phase 2's code: `adding::find_on_youtube` (moved into kara-core by Task 6), the `art.ts` picture helper and `phones::encode`'s artwork links (Task 8), `app/scripts/isolated-check.sh` and `window-id.swift` (used unchanged), `Entry.by` in the player. Every task that touches a file Phase 2 changed says "read the real code first": the code at HEAD wins over any snippet here — when a name or signature differs, follow HEAD and keep this plan's behavior.
- **The user's dev app must never be disturbed.** The user runs `kara-app` with Vite on port 1420 while tasks run. No step may stop or restart it (`pkill`, `kill` by name), bind or wait on port 1420, run `app/scripts/app-check.sh`, `npm run app-check` or `npm run tauri:dev`, or build into the shared `target/` with different features. Running the app is only through the **isolated check** (Task 7): its own target folder, its own build with the page inside, its own process id, scratch data. The last task asks the user to restart their dev app; it starts nothing.
- The worktree matters: the user's dev app hot-reloads the page from the main checkout, so half-done frontend work must never land there.
- Never read or write `~/Library/Application Support/kara-always-oki`. The isolated check is Phase 2's script, which uses its own scratch data folder and target folder; pictures and logs go to `.superpowers/sdd/2026-09-27-phase3-apple-music/shots/`.
- Before any `cargo` command run `source "$HOME/.cargo/env"`.
- Code bar (the user's): comments only summarize what a function does (a constant or field gets one only if someone would reasonably ask) — no reasoning, history or postmortems; code explains itself through names and structure; less code and reuse over new layers (YAGNI); every test must be necessary — no redundant tests.
- Probe over reasoning: when an API or behavior is in doubt, run a focused command or test or read the real source (`~/.cargo/registry/src/*/<crate>`, `app/node_modules/<pkg>`) before deciding. Read-only live probes are allowed against YouTube and YouTube Music only — never any request to Apple Music with a token. If the environment refuses a probe, skip it and rely on the samples.
- Every task's checks are commands a subagent can run without the user's Apple account. Anything that needs the real account is in the final checklist (Task 15).
- **The login is secret.** The developer token, the user token and the `media-user-token` cookie live only in memory and in the macOS Keychain (service `world.aako.kara-always-oki`, account `apple-music`). They are sent only to `amp-api.music.apple.com`. They never go into the library database, logs, `eprintln!`/`println!`, error messages, test output, task reports, commits or `Debug` output (`Tokens` has a redacting `Debug`). The login window gets no IPC permissions: `app/src-tauri/capabilities/default.json` stays `"windows": ["main"]`. The Keychain item is shared by every data folder, so no check (isolated or scratch) ever presses Connect or Disconnect.
- Read-only: the app only sends GET requests to Apple Music and never changes the user's Apple Music library.
- UI copy is plain: no engineering words ("token", "API", "Keychain", "sync", "ISRC", "HLS", "cookie", "web player") — Apple Music and "login" are fine. Every piece of UI text goes through `t(key, params)` in all six locales (`app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`, appended at the end of each file); a task that adds or removes text changes all six; `npm run check:i18n` must pass. Each new `Problem` gets a `problem.<code>` key and joins `ProblemCode` in `app/src/lib/api.ts`. Song titles, artists and album names show exactly as Apple gives them.
- Phase 1 UI rules: tokens only from `app/src/styles/tokens.css`, Phosphor Bold icons from `phosphor-svelte/lib/<Name>Icon`, glass for everything floating, motion only through `$lib/motion` (`fade`, `slide`; fade only under reduced motion), icon beside every label except song info, providers shown by their logos (Apple Music: `AppleLogoIcon`; Local: `FolderIcon`; All: `SquaresFourIcon`).
- Sample data in code and tests is made up (titles, artists, ids, URLs); recorded samples are anonymized (no real titles, ids, names or tokens). Ignored network tests may use a real public query or video. Never real lyrics.
- Numbers: requests to Apple at most one per 250 ms; a 429 with `Retry-After` ≤ 30 s is waited out once, otherwise "busy"; a refused login (401/403) marks Apple Music signed out at once ("Log in to Apple Music again"); the app refreshes the library on start when the last refresh is older than 6 hours; library lists and collection songs are read 100 per page; artwork URLs use `600x600`; an animated cover (or its absence) is kept 30 days; matching accepts official audio within 15 s of the song's length and videos within 90 s, keeps 8 uploads per song; Apple search asks for 10 songs and 5 phrases, 280 ms after typing stops (the same timer as YouTube).
- No remote actions: no `git push`, no `gh`, no tags.
- Verification per task: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cd app && npm run check && npm test && npm run check:i18n`, Playwright `cd app && npx playwright test <file>` (its own port 1430), and from Task 7 on the isolated check (Phase 2's `app/scripts/isolated-check.sh`) where the task says so. The task report names every command run and its result.

## Review Focus

1. **An Apple playlist mistaken for Imported** — today the app calls every non-user playlist "Imported" (the sidebar name, search by "Imported", "reveal" after adding). An Apple playlist must keep its own name, never match a search for "Imported", and adding a song must still reveal Local › Imported. Tests: Task 8 `an_apple_playlist_is_found_by_its_own_name_not_as_imported`; Task 11 `an Apple Music playlist keeps its own name and its songs queue like any other`.
2. **The login leaking** — tokens must not appear in `Debug` output, error text (anyhow chains included), logs or the database. Test: Task 3 `the_login_never_shows_in_debug_output_or_error_messages`; Task 1 stores no token column; Task 7 leaves the login window without IPC permission (reviewer checks `capabilities/default.json` is unchanged).
3. **A refresh that fails, or races the queue** — a failed refresh keeps everything already synced (the list of collections may be half updated, but nothing that still exists is lost and nothing is pruned before a sync completes); songs in the queue or already sung are never pruned (the queue snapshot would break); disconnecting keeps songs sung or in your own playlists. Tests: Task 5 `a_sync_that_fails_leaves_the_library_as_it_was`, `a_later_sync_refetches_only_changed_collections_drops_gone_ones_and_keeps_sung_or_queued_songs`, `disconnecting_removes_apple_music_but_keeps_songs_sung_or_in_your_own_playlists`.
4. **A login that runs out** — a refused login (401/403) marks the account signed out once (one toast with Log in), not an error per request, and synced songs keep working. Tests: Task 3 `a_refused_login_means_signed_out_after_one_request`; Task 10 `a login that ran out says so, and Log in opens the login window again`.
5. **Wrong versions and non-Latin titles** — live, karaoke, cover, sped-up and other singers' uploads are refused unless the song itself is one; titles in 「」《》 with the singer outside still match; a song whose uploads are all refused keeps them for Change match and isn't searched again on every play. Tests: Task 6 `the_official_audio_found_by_isrc_is_used_and_other_songs_and_versions_are_refused`, `a_songs_own_live_version_accepts_live_uploads_and_titles_in_brackets_match`, `a_song_nothing_fits_keeps_the_uploads_for_change_match_and_is_not_searched_again`.

---

## File Structure

Paths are as on `phase2-phone-mics` when Phase 2 is finished; files Phase 2 created or changed are marked "(Phase 2)".

```
kara-always-oki/
├─ README.md                                    + Apple Music; KARA_APPLE_LOGIN_CHECK
├─ crates/kara-core/
│  ├─ Cargo.toml                                + security-framework (macOS)
│  └─ src/
│     ├─ lib.rs                                 + pub mod apple, matching
│     ├─ problem.rs                             + AppleSignedOut, AppleUnreachable, AppleChanged, AppleBusy, LoginSave, NoMatch, MatchLookup; − StreamingLater
│     ├─ library/mod.rs                         the next upgrade step; StreamingTrack/Collection (with artwork), Account; CollectionRow.artwork_path;
│     │                                         streaming upserts, versions, pruning, accounts, motion cache; match sources; search shows only your songs
│     ├─ lyrics.rs                              key, similar, singer_likeness, words → pub(crate)
│     ├─ jobs.rs                                a streaming song gets matched before fetching; match downloads are temporary; no lyrics rename for streaming songs
│     ├─ matching.rs              (new)         Finder, YouTube, score, find, search_once, ensure
│     ├─ ingest/
│     │  ├─ mod.rs                              match sources download without renaming the song; find_on_youtube (moved from the app)
│     │  ├─ youtube.rs                          clock_ms → pub(crate)
│     │  ├─ ytmusic.rs            (new)         YouTube Music songs search
│     │  └─ ytmusic-search.sample.json (new)
│     └─ apple/
│        ├─ mod.rs                (new)         Tokens, Reply, Client (pace, 429, pages)
│        ├─ keychain.rs           (new)         load/save/delete the login (macOS)
│        ├─ parse.rs              (new)         Apple answers → StreamingTrack/StreamingCollection, motion, phrases
│        ├─ sync.rs               (new)         sync, disconnect
│        ├─ catalog.rs            (new)         motion (cached), search, suggestions
│        └─ samples/*.json        (new)         anonymized answers
├─ app/
│  ├─ scripts/isolated-check.sh, window-id.swift (Phase 2)   used unchanged
│  ├─ src-tauri/src/
│  │  ├─ lib.rs                   (Phase 2)     Apple state, start refresh, KARA_APPLE_LOGIN_CHECK, commands
│  │  ├─ sources.rs               (new)         login window, status event, refresh, disconnect, motion, Apple search
│  │  ├─ matches.rs               (new)         match_candidates, choose_match, choose_match_link
│  │  ├─ library.rs                             cards/page by provider, artists merged in All, providers on cards
│  │  ├─ adding.rs                (Phase 2)     Imported only for Local; calls ingest::find_on_youtube
│  │  ├─ phones/mod.rs            (Phase 2)     encode keeps web artwork addresses as they are
│  │  └─ player.rs                (Phase 2)     Player::track_ids, Player::renew, restart
│  ├─ src/lib/
│  │  ├─ api.ts                                 Apple, match and motion commands; CollectionCard.providers/artworkPath; problem codes
│  │  ├─ art.ts                   (Phase 2)     one picture helper: /art/ links and web addresses as they are
│  │  ├─ format.ts                              ago()
│  │  ├─ search.ts / search.test.ts             Apple results in the text view
│  │  ├─ state/sources.svelte.ts  (new)         Apple status, toasts, Apple phrases while signed in
│  │  ├─ state/library.svelte.ts                provider, cardName fix
│  │  ├─ state/adding.svelte.ts                 reveal finds Local › Imported
│  │  ├─ state/player.svelte.ts   (Phase 2)     "No singable version" offers Change match
│  │  ├─ state/ui.svelte.ts       (Phase 2)     sheet kind "match"
│  │  ├─ components/SettingsSheet.svelte        Sources section
│  │  ├─ components/Sidebar.svelte              provider switcher, logos, Apple notes
│  │  ├─ components/Cover.svelte                a collection's own artwork when it has one
│  │  ├─ components/ProviderLogos.svelte (new)
│  │  ├─ components/CollectionView.svelte       logos, own artwork, animated cover
│  │  ├─ components/SongRow.svelte              source logo in your own playlists
│  │  ├─ components/MatchSheet.svelte   (new)   Change match
│  │  ├─ components/SongMenu.svelte / MoreMenu.svelte   Change match…
│  │  ├─ components/MotionArt.svelte    (new)   looping video over a still cover
│  │  ├─ components/KaraokeBackground.svelte    animated background, web artwork
│  │  ├─ components/SearchBar.svelte / SearchResults.svelte   Apple results
│  │  └─ i18n/*.ts                              keys per task
│  ├─ src/routes/+page.svelte     (Phase 2)     sources.init, MatchSheet, empty state per provider
│  └─ tests/
│     ├─ fake-backend.ts          (Phase 2)     Apple account, collections by provider, matches, motion, Apple search
│     └─ apple-music.spec.ts      (new)
```

## Task list

1. Library: streaming songs and collections (with artwork), accounts, animated-cover cache (the next upgrade step)
2. Match uploads as audio sources; downloading a match
3. Apple Music client: the login, the Keychain, requests
4. Reading Apple's answers (with anonymized samples)
5. Sync and disconnect
6. Matching a streaming song to an upload (on Phase 2's shared YouTube search)
7. The app: login window, Sources status, refresh, disconnect
8. The app: collections by source, artists merged in All, Imported only for Local; web artwork on the Mac and on phones
9. The app: Change match, animated covers and Apple search commands
10. Settings › Sources
11. The provider switcher, and collections' own artwork
12. The Change match sheet
13. Animated covers
14. Apple Music in search
15. README, full check and the user's checklist (on a copy of the library)

---

### Task 1: Library — streaming songs and collections (with artwork), accounts, animated-cover cache (the next upgrade step)

**Files:**
- Modify: `crates/kara-core/src/library/mod.rs` (types after `LyricsRow`; `CollectionRow`, `COLLECTION_COLS`, `collection_row`; `STEPS`; methods in a new `// ---- streaming ----` block after the `// ---- settings ----` methods; tests)

**Interfaces:**
- Consumes: `Library`, `ProviderId`, `CollectionKind`, `STEPS` (existing).
- Produces (all `pub`, in `kara_core::library`):
  - `struct StreamingTrack { provider_ref: String, title: String, artist: Option<String>, album: Option<String>, duration_ms: Option<i64>, isrc: Option<String>, album_ref: Option<String>, artwork: Option<String> }` (derive `Clone, Debug, PartialEq`)
  - `struct StreamingCollection { kind: CollectionKind, provider_ref: String, name: String, subtitle: Option<String>, artwork: Option<String>, catalog_ref: Option<String>, version: Option<String> }` (derive `Clone, Debug, PartialEq`)
  - `CollectionRow` gains `artwork_path: Option<String>` (JSON `artworkPath`): a streaming collection's own artwork (a web address); None for local ones.
  - `struct Account { display_name: Option<String>, refreshed_at: Option<i64>, signed_out: bool }` (derive `Clone, Debug, PartialEq, serde::Serialize`, `camelCase`)
  - `Library::upsert_streaming_track(&self, provider: ProviderId, t: &StreamingTrack) -> Result<i64>`
  - `Library::track_isrc(&self, id: i64) -> Result<Option<String>>`, `Library::track_album_ref(&self, id: i64) -> Result<Option<String>>`
  - `Library::upsert_streaming_collection(&self, provider: ProviderId, c: &StreamingCollection) -> Result<i64>`
  - `Library::collection_version(&self, id: i64) -> Result<Option<String>>`, `Library::collection_catalog_ref(&self, id: i64) -> Result<Option<String>>`
  - `Library::set_collection_tracks(&self, id: i64, track_ids: &[i64], version: Option<&str>) -> Result<()>`
  - `Library::delete_collections_except(&self, provider: ProviderId, kind: CollectionKind, keep: &[i64]) -> Result<()>`
  - `Library::prune_streaming_tracks(&self, provider: ProviderId, keep: &[i64]) -> Result<()>`
  - `Library::account(&self, provider: ProviderId) -> Result<Option<Account>>`, `connect_account(&self, provider, display_name: Option<&str>) -> Result<()>`, `set_refreshed(&self, provider, at_ms: i64) -> Result<()>`, `set_signed_out(&self, provider) -> Result<()>`, `delete_account(&self, provider) -> Result<()>`
  - `Library::motion(&self, reference: &str) -> Result<Option<(Option<String>, i64)>>`, `Library::set_motion(&self, reference: &str, url: Option<&str>) -> Result<()>`
  - test helper `fn streaming(id: &str, title: &str) -> StreamingTrack` in `library::tests` (Task 2 reuses it)
  - The step also adds `audio_source.channel` and `audio_source.thumbnail` (used by Task 2).

- [ ] **Step 1: Write the failing tests**

In `crates/kara-core/src/library/mod.rs`, inside `mod tests`, add:

```rust
    fn streaming(id: &str, title: &str) -> StreamingTrack {
        StreamingTrack {
            provider_ref: id.into(),
            title: title.into(),
            artist: Some("Juniper Row".into()),
            album: Some("Night Line".into()),
            duration_ms: Some(214_000),
            isrc: Some("QZZZZ2600001".into()),
            album_ref: Some("900001".into()),
            artwork: Some("https://example.com/made-up/600x600bb.jpg".into()),
        }
    }

    #[test]
    fn a_streaming_song_is_updated_in_place_by_its_service_id() {
        let l = lib();
        let id = l.upsert_streaming_track(ProviderId::Apple, &streaming("1001", "Neon Tidewater")).unwrap();
        let again = l.upsert_streaming_track(ProviderId::Apple, &streaming("1001", "Neon Tidewater (2026 Mix)")).unwrap();
        assert_eq!(again, id);
        let t = l.track(id).unwrap();
        assert_eq!(
            (t.title.as_str(), t.provider, t.artwork_path.as_deref()),
            ("Neon Tidewater (2026 Mix)", ProviderId::Apple, Some("https://example.com/made-up/600x600bb.jpg"))
        );
        assert_eq!(l.track_isrc(id).unwrap().as_deref(), Some("QZZZZ2600001"));
        assert_eq!(l.track_album_ref(id).unwrap().as_deref(), Some("900001"));
    }

    #[test]
    fn pruning_keeps_streaming_songs_that_are_in_a_collection_were_sung_or_are_kept() {
        let l = lib();
        let [listed, sung, queued, gone] = ["1", "2", "3", "4"].map(|r| l.upsert_streaming_track(ProviderId::Apple, &streaming(r, r)).unwrap());
        let local_orphan = l.add_track(&local("Local orphan", None)).unwrap();
        let playlist = StreamingCollection {
            kind: CollectionKind::Playlist,
            provider_ref: "p.1".into(),
            name: "Late Night Drive".into(),
            subtitle: None,
            artwork: Some("https://example.com/made-up/playlist/600x600bb.jpg".into()),
            catalog_ref: None,
            version: Some("v1".into()),
        };
        let pl = l.upsert_streaming_collection(ProviderId::Apple, &playlist).unwrap();
        l.set_collection_tracks(pl, &[listed, listed], Some("v1")).unwrap();
        assert_eq!(l.collection_version(pl).unwrap().as_deref(), Some("v1"));
        assert_eq!(l.collection(pl).unwrap().artwork_path.as_deref(), Some("https://example.com/made-up/playlist/600x600bb.jpg"));
        let s = l.add_source(sung, SourceKind::Match, "https://www.youtube.com/watch?v=aaaaaaaaaaa", None).unwrap();
        l.set_source_audio(s, "h", 1000).unwrap();

        l.prune_streaming_tracks(ProviderId::Apple, &[queued]).unwrap();
        for kept in [listed, sung, queued, local_orphan] {
            assert!(l.track(kept).is_ok(), "{kept}");
        }
        assert!(l.track(gone).is_err());

        l.delete_collections_except(ProviderId::Apple, CollectionKind::Playlist, &[]).unwrap();
        l.prune_streaming_tracks(ProviderId::Apple, &[]).unwrap();
        assert!(l.track(listed).is_err() && l.track(queued).is_err());
        assert!(l.track(sung).is_ok());
    }

    #[test]
    fn a_signed_out_account_is_signed_in_again_by_connecting() {
        let l = lib();
        assert_eq!(l.account(ProviderId::Apple).unwrap(), None);
        l.connect_account(ProviderId::Apple, Some("Mina")).unwrap();
        l.set_refreshed(ProviderId::Apple, 42).unwrap();
        l.set_signed_out(ProviderId::Apple).unwrap();
        assert!(l.account(ProviderId::Apple).unwrap().unwrap().signed_out);
        l.connect_account(ProviderId::Apple, Some("Mina")).unwrap();
        assert_eq!(l.account(ProviderId::Apple).unwrap(), Some(Account { display_name: Some("Mina".into()), refreshed_at: Some(42), signed_out: false }));
        l.delete_account(ProviderId::Apple).unwrap();
        assert_eq!(l.account(ProviderId::Apple).unwrap(), None);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core library::`
Expected: compile errors — `StreamingTrack`, `upsert_streaming_track`, `Account`, … not found.

- [ ] **Step 3: Add the upgrade step**

Append at the very end of `STEPS`, after the last step at HEAD (`info_edited` today). Never write its number anywhere; the tests use `STEPS.len()`:

```rust
    |tx| {
        Ok(tx.execute_batch(
            "ALTER TABLE track ADD COLUMN isrc TEXT;
             ALTER TABLE track ADD COLUMN album_ref TEXT;
             ALTER TABLE collection ADD COLUMN catalog_ref TEXT;
             ALTER TABLE collection ADD COLUMN version TEXT;
             ALTER TABLE provider_account ADD COLUMN refreshed_at INTEGER;
             ALTER TABLE provider_account ADD COLUMN signed_out INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE audio_source ADD COLUMN channel TEXT;
             ALTER TABLE audio_source ADD COLUMN thumbnail TEXT;
             CREATE TABLE motion (ref TEXT PRIMARY KEY, url TEXT, fetched_at INTEGER NOT NULL);",
        )?)
    },
```

- [ ] **Step 4: Add the types and methods**

After `pub struct LyricsRow { … }` add:

```rust
/// A song as a streaming service lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamingTrack {
    /// The service's id for the song.
    pub provider_ref: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub isrc: Option<String>,
    /// The service's catalog id for the song's album.
    pub album_ref: Option<String>,
    /// A web address of the song's artwork.
    pub artwork: Option<String>,
}

/// A playlist or album as a streaming service lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamingCollection {
    pub kind: CollectionKind,
    pub provider_ref: String,
    pub name: String,
    pub subtitle: Option<String>,
    /// A web address of its own artwork.
    pub artwork: Option<String>,
    /// The service's catalog id for it, where its animated cover lives.
    pub catalog_ref: Option<String>,
    /// Differs from the last one seen whenever its songs may have changed.
    pub version: Option<String>,
}

/// A connected streaming account.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub display_name: Option<String>,
    pub refreshed_at: Option<i64>,
    /// The service refused the saved login; it needs a new one.
    pub signed_out: bool,
}

```

Give `CollectionRow` its artwork (after `user`):

```rust
    /// A streaming collection's own artwork (a web address); None for local ones.
    pub artwork_path: Option<String>,
```

and read it: `const COLLECTION_COLS: &str = "id, provider, kind, name, subtitle, provider_ref LIKE 'user:%', artwork_path";`, and in `collection_row` add `artwork_path: r.get(6)?`.

```rust
/// "1,2,3", for an SQL `IN (…)` list of ids.
fn id_list(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}
```

After the `// ---- settings ----` methods (still inside `impl Library`) add:

```rust
    // ---- streaming ----

    /// Adds a streaming song, or updates the one with the same service id; returns its id.
    pub fn upsert_streaming_track(&self, provider: ProviderId, t: &StreamingTrack) -> Result<i64> {
        Ok(self.conn.query_row(
            "INSERT INTO track (provider, provider_ref, title, artist, album, duration_ms, isrc, album_ref, artwork_path, added_at, art_seed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, abs(random()) % 360)
             ON CONFLICT (provider, provider_ref) DO UPDATE SET title = excluded.title, artist = excluded.artist, album = excluded.album,
               duration_ms = excluded.duration_ms, isrc = excluded.isrc, album_ref = excluded.album_ref, artwork_path = excluded.artwork_path
             RETURNING id",
            params![provider, t.provider_ref, t.title, t.artist, t.album, t.duration_ms, t.isrc, t.album_ref, t.artwork, crate::now_ms()],
            |r| r.get(0),
        )?)
    }

    pub fn track_isrc(&self, id: i64) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT isrc FROM track WHERE id = ?1", [id], |r| r.get(0))?)
    }

    pub fn track_album_ref(&self, id: i64) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT album_ref FROM track WHERE id = ?1", [id], |r| r.get(0))?)
    }

    /// Adds a streaming playlist or album, or updates the one with the same service id; returns its id.
    pub fn upsert_streaming_collection(&self, provider: ProviderId, c: &StreamingCollection) -> Result<i64> {
        Ok(self.conn.query_row(
            "INSERT INTO collection (provider, kind, provider_ref, name, subtitle, artwork_path, catalog_ref) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (provider, kind, provider_ref) DO UPDATE SET name = excluded.name, subtitle = excluded.subtitle,
               artwork_path = excluded.artwork_path, catalog_ref = excluded.catalog_ref
             RETURNING id",
            params![provider, c.kind, c.provider_ref, c.name, c.subtitle, c.artwork, c.catalog_ref],
            |r| r.get(0),
        )?)
    }

    /// The version the collection's songs were last saved from.
    pub fn collection_version(&self, id: i64) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT version FROM collection WHERE id = ?1", [id], |r| r.get(0))?)
    }

    pub fn collection_catalog_ref(&self, id: i64) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT catalog_ref FROM collection WHERE id = ?1", [id], |r| r.get(0))?)
    }

    /// Replaces a collection's songs with `track_ids`, in order, and records the version they came from.
    pub fn set_collection_tracks(&self, id: i64, track_ids: &[i64], version: Option<&str>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM collection_track WHERE collection_id = ?1", [id])?;
        for (pos, t) in track_ids.iter().enumerate() {
            tx.execute(
                "INSERT OR IGNORE INTO collection_track (collection_id, track_id, position) VALUES (?1, ?2, ?3)",
                params![id, t, pos as i64],
            )?;
        }
        tx.execute("UPDATE collection SET version = ?2 WHERE id = ?1", params![id, version])?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a provider's collections of `kind`, except those in `keep`.
    pub fn delete_collections_except(&self, provider: ProviderId, kind: CollectionKind, keep: &[i64]) -> Result<()> {
        let sql = format!("DELETE FROM collection WHERE provider = ?1 AND kind = ?2 AND id NOT IN ({})", id_list(keep));
        self.conn.execute(&sql, params![provider, kind])?;
        Ok(())
    }

    /// Deletes a provider's songs that are in no collection and were never got ready to sing, except those in `keep`.
    pub fn prune_streaming_tracks(&self, provider: ProviderId, keep: &[i64]) -> Result<()> {
        let sql = format!(
            "DELETE FROM track WHERE provider = ?1 AND id NOT IN ({})
               AND id NOT IN (SELECT track_id FROM collection_track)
               AND id NOT IN (SELECT track_id FROM audio_source WHERE audio_hash IS NOT NULL)",
            id_list(keep)
        );
        self.conn.execute(&sql, [provider])?;
        Ok(())
    }

    pub fn account(&self, provider: ProviderId) -> Result<Option<Account>> {
        Ok(self
            .conn
            .query_row("SELECT display_name, refreshed_at, signed_out FROM provider_account WHERE provider = ?1", [provider], |r| {
                Ok(Account { display_name: r.get(0)?, refreshed_at: r.get(1)?, signed_out: r.get(2)? })
            })
            .optional()?)
    }

    /// Records a connected account; connecting again clears a refused login.
    pub fn connect_account(&self, provider: ProviderId, display_name: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO provider_account (provider, display_name, connected_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (provider) DO UPDATE SET display_name = excluded.display_name, signed_out = 0",
            params![provider, display_name, crate::now_ms()],
        )?;
        Ok(())
    }

    pub fn set_refreshed(&self, provider: ProviderId, at_ms: i64) -> Result<()> {
        self.conn.execute("UPDATE provider_account SET refreshed_at = ?2 WHERE provider = ?1", params![provider, at_ms])?;
        Ok(())
    }

    pub fn set_signed_out(&self, provider: ProviderId) -> Result<()> {
        self.conn.execute("UPDATE provider_account SET signed_out = 1 WHERE provider = ?1", [provider])?;
        Ok(())
    }

    pub fn delete_account(&self, provider: ProviderId) -> Result<()> {
        self.conn.execute("DELETE FROM provider_account WHERE provider = ?1", [provider])?;
        Ok(())
    }

    /// The animated cover saved for `reference` (None inside when it has none), and when it was looked up.
    pub fn motion(&self, reference: &str) -> Result<Option<(Option<String>, i64)>> {
        Ok(self
            .conn
            .query_row("SELECT url, fetched_at FROM motion WHERE ref = ?1", [reference], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    }

    pub fn set_motion(&self, reference: &str, url: Option<&str>) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO motion (ref, url, fetched_at) VALUES (?1, ?2, ?3)", params![reference, url, crate::now_ms()])?;
        Ok(())
    }
```

- [ ] **Step 5: Run the tests**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core library::`
Expected: PASS, including the existing upgrade tests (`opening_runs_the_steps_a_library_has_not_had`, `a_version_1_database_gains_the_instrumental_flag`).

- [ ] **Step 6: Workspace check and commit**

Run: `source "$HOME/.cargo/env" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all pass, no warnings.

```bash
git add crates/kara-core/src/library/mod.rs
git commit -m "feat(core): library keeps streaming songs, collections, accounts and animated covers"
```

---

### Task 2: Match uploads as audio sources; downloading a match

**Files:**
- Modify: `crates/kara-core/src/library/mod.rs` (`AudioSource`, `SOURCE_COLS`, `source_row`, new methods after `selected_source`, tests)
- Modify: `crates/kara-core/src/ingest/mod.rs:150-185` (`fetch_audio`), new `download_from_page`
- Modify: `crates/kara-core/src/jobs.rs` (`load_or_fetch`: remove the temporary download for match sources too)
- Modify: `crates/kara-core/src/problem.rs` (remove `StreamingLater`)
- Modify: `app/src/lib/api.ts` (`ProblemCode`: remove `"streamingLater"`), `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts` (remove `"problem.streamingLater"`)

**Interfaces:**
- Consumes: Task 1's upgrade step columns and `streaming()` test helper; `ingest::preview::SearchHit { url, preview: LinkPreview { title, channel, duration_ms, thumbnail } }`.
- Produces:
  - `AudioSource` gains `duration_ms: Option<i64>`, `channel: Option<String>`, `thumbnail: Option<String>`, `selected: bool`.
  - `Library::add_match(&self, track_id: i64, hit: &SearchHit) -> Result<i64>` — saved not in use; an upload the song already has keeps its id (no duplicates when two searches race).
  - `Library::sources(&self, track_id: i64) -> Result<Vec<AudioSource>>` — in the order added.
  - `Library::select_source(&self, track_id: i64, source_id: i64) -> Result<()>` — fails (and changes nothing) when the source isn't the song's.
  - `ingest::fetch_audio` downloads a `SourceKind::Match` source like a link but never renames the song, files it in collections or saves its thumbnail.

- [ ] **Step 1: Write the failing tests**

In `library/mod.rs` tests add (top of `mod tests`: `use crate::ingest::preview::LinkPreview;` — `SearchHit` comes from `super::*`):

```rust
    #[test]
    fn a_streaming_song_switches_between_its_matches_each_keeping_its_own_lyric_timing() {
        let l = lib();
        let t = l.upsert_streaming_track(ProviderId::Apple, &streaming("1001", "Neon Tidewater")).unwrap();
        let hit = |id: &str, title: &str| SearchHit {
            url: format!("https://www.youtube.com/watch?v={id}"),
            preview: LinkPreview { title: title.into(), channel: Some("Juniper Row - Topic".into()), duration_ms: Some(214_000), thumbnail: None },
        };
        let a = l.add_match(t, &hit("aaaaaaaaaaa", "Neon Tidewater")).unwrap();
        let b = l.add_match(t, &hit("bbbbbbbbbbb", "Neon Tidewater (Official Video)")).unwrap();
        assert_eq!(l.add_match(t, &hit("aaaaaaaaaaa", "Neon Tidewater")).unwrap(), a);
        assert_eq!(l.selected_source(t).unwrap(), None);
        l.select_source(t, a).unwrap();
        l.set_lyric_offset(a, 1_500).unwrap();
        l.select_source(t, b).unwrap();
        let sources = l.sources(t).unwrap();
        assert_eq!(sources.iter().map(|s| (s.id, s.selected)).collect::<Vec<_>>(), [(a, false), (b, true)]);
        assert_eq!(
            (sources[0].lyric_offset_ms, sources[1].label.as_deref(), sources[1].channel.as_deref(), sources[1].duration_ms),
            (1_500, Some("Neon Tidewater (Official Video)"), Some("Juniper Row - Topic"), Some(214_000))
        );
        assert!(l.select_source(t, 999).is_err());
        assert_eq!(l.selected_source(t).unwrap().unwrap().id, b);
    }
```

In `ingest/mod.rs` tests add (an opt-in network test; it downloads a 19-second public video):

```rust
    #[test]
    #[ignore = "network"]
    fn a_youtube_match_downloads_without_renaming_the_song() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        let song = crate::library::StreamingTrack { provider_ref: "1".into(), title: "Made Up Name".into(), artist: None, album: None, duration_ms: None, isrc: None, album_ref: None, artwork: None };
        let t = lib.upsert_streaming_track(ProviderId::Apple, &song).unwrap();
        let hit = preview::SearchHit { url: "https://www.youtube.com/watch?v=jNQXAC9IVRw".into(), preview: preview::LinkPreview { title: "An upload".into(), channel: None, duration_ms: None, thumbnail: None } };
        let s = lib.add_match(t, &hit).unwrap();
        lib.select_source(t, s).unwrap();
        let path = fetch_audio(&lib, &store, &lib.track(t).unwrap(), &lib.selected_source(t).unwrap().unwrap()).unwrap();
        assert!(path.exists());
        assert_eq!(lib.track(t).unwrap().title, "Made Up Name");
        assert!(lib.collections(None, CollectionKind::Playlist).unwrap().is_empty());
    }
```

- [ ] **Step 2: Run to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core library::tests::a_streaming_song_switches`
Expected: compile error — `add_match` / `sources` / `select_source` not found.

- [ ] **Step 3: Extend `AudioSource` and add the methods**

In `library/mod.rs`:

```rust
use crate::ingest::preview::SearchHit;
```

Add to `pub struct AudioSource` after `error`:

```rust
    pub duration_ms: Option<i64>,
    /// Who uploaded a streaming song's match.
    pub channel: Option<String>,
    pub thumbnail: Option<String>,
    /// It is the song's audio now.
    pub selected: bool,
```

Replace `SOURCE_COLS` and extend `source_row`:

```rust
const SOURCE_COLS: &str =
    "id, track_id, kind, uri, label, audio_hash, lyric_offset_ms, lyric_offset_manual, lyric_synced_at, status, error, duration_ms, channel, thumbnail, selected";
```

```rust
        error: r.get(10)?,
        duration_ms: r.get(11)?,
        channel: r.get(12)?,
        thumbnail: r.get(13)?,
        selected: r.get(14)?,
```

After `selected_source` add:

```rust
    /// Saves an upload a streaming song could be sung from, not yet in use; returns its id (the saved one's, when the song has it already).
    pub fn add_match(&self, track_id: i64, hit: &SearchHit) -> Result<i64> {
        let saved = self.conn.query_row("SELECT id FROM audio_source WHERE track_id = ?1 AND uri = ?2", params![track_id, hit.url], |r| r.get(0)).optional()?;
        if let Some(id) = saved {
            return Ok(id);
        }
        self.conn.execute(
            "INSERT INTO audio_source (track_id, kind, uri, label, duration_ms, channel, thumbnail, status) VALUES (?1, 'match', ?2, ?3, ?4, ?5, ?6, 'pending')",
            params![track_id, hit.url, hit.preview.title, hit.preview.duration_ms, hit.preview.channel, hit.preview.thumbnail],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Every audio source of a song, in the order they were added.
    pub fn sources(&self, track_id: i64) -> Result<Vec<AudioSource>> {
        let sql = format!("SELECT {SOURCE_COLS} FROM audio_source WHERE track_id = ?1 ORDER BY id");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([track_id], source_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Makes `source_id` the song's audio instead of the one in use.
    pub fn select_source(&self, track_id: i64, source_id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE audio_source SET selected = 0 WHERE track_id = ?1", [track_id])?;
        let n = tx.execute("UPDATE audio_source SET selected = 1 WHERE id = ?2 AND track_id = ?1", params![track_id, source_id])?;
        anyhow::ensure!(n == 1, Problem::SongGone);
        tx.commit()?;
        Ok(())
    }
```

- [ ] **Step 4: Download match sources**

In `ingest/mod.rs` add above `fetch_audio`:

```rust
/// Downloads a page's audio with yt-dlp, which is updated once if that fails.
fn download_from_page(store: &Store, url: &Url) -> Result<ytdlp::Fetched> {
    let bin_dir = store.bin_dir();
    let bin = ytdlp::ensure(&bin_dir).context(Problem::DownloaderSetup)?;
    ytdlp::download(&bin, url.as_str(), &store.tmp_dir(), || ytdlp::update(&bin_dir)).context(Problem::Download)
}
```

Read `fetch_audio` at HEAD first (Task 33 guards the song's info with `if source.audio_hash.is_none()`; keep that guard exactly). Replace the `SourceKind::Link => { … }` and `SourceKind::Match => bail!(Problem::StreamingLater),` arms with one arm whose `Extractable` body is HEAD's, only fetching through `download_from_page`, plus one guard arm for matches:

```rust
        SourceKind::Link | SourceKind::Match => {
            let url = Url::parse(&source.uri)?;
            match link::verdict(&url) {
                LinkVerdict::AudioFile => download_file(&url, &store.tmp_dir()).context(Problem::Download),
                LinkVerdict::Extractable if source.kind == SourceKind::Match => Ok(download_from_page(store, &url)?.path),
                LinkVerdict::Extractable => {
                    let f = download_from_page(store, &url)?;
                    if source.audio_hash.is_none() {
                        lib.update_track_meta(track.id, &f.title, f.artist.as_deref(), f.album.as_deref())?;
                        let mut texts = vec![f.title.as_str(), f.album.as_deref().unwrap_or_default()];
                        texts.extend(f.tags.iter().map(String::as_str));
                        if looks_instrumental(&texts) {
                            lib.mark_instrumental(track.id)?;
                        }
                        link_collections(lib, track.id, f.artist.as_deref(), f.album.as_deref())?;
                    }
                    if let Some(url) = f.thumbnail.as_deref().filter(|_| track.artwork_path.is_none()) {
                        let _ = download_artwork(lib, store, track.id, url);
                    }
                    Ok(f.path)
                }
                _ => bail!(link::rejection(&url).unwrap_or(Problem::LinkUnsupported)),
            }
        }
```

In `jobs.rs` `load_or_fetch`, the temporary download is removed for both kinds:

```rust
    if source.kind != SourceKind::File {
        let _ = std::fs::remove_file(&path);
    }
```

- [ ] **Step 5: Remove `StreamingLater`**

Delete the `StreamingLater` variant and its `Display` arm from `problem.rs`; delete `| "streamingLater"` from `ProblemCode` in `app/src/lib/api.ts`; delete the `"problem.streamingLater": …` line from all six i18n files.

- [ ] **Step 6: Run the tests and checks**

Run: `source "$HOME/.cargo/env" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cd app && npm run check && npm run check:i18n`
Expected: all pass — including the existing `ingest::tests::downloading_a_link_again_keeps_the_songs_info`. Optionally run `cargo test -p kara-core a_youtube_match_downloads -- --ignored` (network; ~20 s) and report the result.

- [ ] **Step 7: Commit**

```bash
git add crates/kara-core/src/library/mod.rs crates/kara-core/src/ingest/mod.rs crates/kara-core/src/jobs.rs crates/kara-core/src/problem.rs app/src/lib/api.ts app/src/lib/i18n
git commit -m "feat(core): a streaming song's uploads are audio sources it can switch between"
```

---
### Task 3: Apple Music client — the login, the Keychain, requests

**Files:**
- Create: `crates/kara-core/src/apple/mod.rs`, `crates/kara-core/src/apple/keychain.rs`
- Modify: `crates/kara-core/src/lib.rs` (`pub mod apple;`), `crates/kara-core/Cargo.toml` (macOS dependency), `crates/kara-core/src/problem.rs`
- Modify: `app/src/lib/api.ts` (`ProblemCode`), `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`

**Interfaces:**
- Consumes: `problem::Problem`, `problem::problem`.
- Produces (in `kara_core::apple`):
  - `struct Tokens { developer: String, user: String, storefront: String }` (derive `Clone, PartialEq, Serialize, Deserialize`; `Debug` shows only the storefront)
  - `struct Reply { status: u16, retry_after_s: Option<u64>, body: String }`
  - `pub(crate) type Get = Box<dyn Fn(&str, &[(&str, &str)]) -> Result<Reply> + Send + Sync>`
  - `Client::new(tokens: Tokens) -> Client`; `pub(crate) Client::with(tokens: Tokens, get: Get) -> Client`
  - `Client::storefront(&self) -> String`
  - `Client::find(&self, path: &str) -> Result<Option<serde_json::Value>>` (None on 404; 401/403 → `Problem::AppleSignedOut` at once), `Client::get(&self, path) -> Result<Value>`, `Client::all(&self, path) -> Result<Vec<Value>>` (every page's `data`, empty on 404)
  - `#[cfg(test)] pub(crate) fn fake_client(answer: impl Fn(&str) -> Reply + Send + Sync + 'static) -> (Client, Arc<Mutex<Vec<String>>>)` — answers each URL and records every URL asked; its login is `dev-secret` / `user-secret` / `us`. `#[cfg(test)] pub(crate) fn reply(status: u16, body: impl ToString) -> Reply`.
  - `apple::keychain::{load() -> Result<Option<Tokens>>, save(&Tokens) -> Result<()>, delete() -> Result<()>}` (macOS)
  - `Problem::{AppleSignedOut, AppleUnreachable, AppleChanged, AppleBusy, LoginSave}`

- [ ] **Step 1: Add the problems and their text**

In `problem.rs` add the variants after the last one at HEAD and their `Display` arms:

```rust
    AppleSignedOut,
    AppleUnreachable,
    AppleChanged,
    AppleBusy,
    LoginSave,
```

```rust
            Self::AppleSignedOut => "Log in to Apple Music again to refresh your library.",
            Self::AppleUnreachable => "Couldn't reach Apple Music. Check your connection.",
            Self::AppleChanged => "Apple Music changed how it works, so your library can't refresh for now. Your songs still work.",
            Self::AppleBusy => "Apple Music is busy. Your library refreshes again later.",
            Self::LoginSave => "Couldn't keep your Apple Music login on this Mac.",
```

In `app/src/lib/api.ts` add `| "appleSignedOut" | "appleUnreachable" | "appleChanged" | "appleBusy" | "loginSave"` to `ProblemCode`. Append to the i18n files:

`en.ts`:
```ts
  "problem.appleSignedOut": "Log in to Apple Music again to refresh your library.",
  "problem.appleUnreachable": "Couldn't reach Apple Music. Check your connection.",
  "problem.appleChanged": "Apple Music changed how it works, so your library can't refresh for now. Your songs still work.",
  "problem.appleBusy": "Apple Music is busy. Your library refreshes again later.",
  "problem.loginSave": "Couldn't keep your Apple Music login on this Mac.",
```
`ja.ts`:
```ts
  "problem.appleSignedOut": "ライブラリを更新するには、Apple Music にもう一度ログインしてください。",
  "problem.appleUnreachable": "Apple Music に接続できませんでした。インターネット接続を確認してください。",
  "problem.appleChanged": "Apple Music の仕組みが変わったため、今はライブラリを更新できません。曲はそのまま使えます。",
  "problem.appleBusy": "Apple Music が混み合っています。ライブラリはあとでもう一度更新されます。",
  "problem.loginSave": "Apple Music のログインをこの Mac に保存できませんでした。",
```
`ko.ts`:
```ts
  "problem.appleSignedOut": "보관함을 새로 고치려면 Apple Music에 다시 로그인하세요.",
  "problem.appleUnreachable": "Apple Music에 연결할 수 없습니다. 인터넷 연결을 확인하세요.",
  "problem.appleChanged": "Apple Music의 작동 방식이 바뀌어 지금은 보관함을 새로 고칠 수 없습니다. 노래는 그대로 쓸 수 있습니다.",
  "problem.appleBusy": "Apple Music이 혼잡합니다. 보관함은 나중에 다시 새로 고쳐집니다.",
  "problem.loginSave": "이 Mac에 Apple Music 로그인을 저장할 수 없습니다.",
```
`zh-Hans.ts`:
```ts
  "problem.appleSignedOut": "请重新登录 Apple Music 以刷新资料库。",
  "problem.appleUnreachable": "无法连接 Apple Music。请检查网络连接。",
  "problem.appleChanged": "Apple Music 的运作方式有变，暂时无法刷新资料库。你的歌曲仍可正常使用。",
  "problem.appleBusy": "Apple Music 正忙。资料库稍后会再次刷新。",
  "problem.loginSave": "无法在这台 Mac 上保存你的 Apple Music 登录。",
```
`zh-Hant.ts`:
```ts
  "problem.appleSignedOut": "請重新登入 Apple Music 以重新整理資料庫。",
  "problem.appleUnreachable": "無法連線到 Apple Music。請檢查網路連線。",
  "problem.appleChanged": "Apple Music 的運作方式有所改變，暫時無法重新整理資料庫。你的歌曲仍可正常使用。",
  "problem.appleBusy": "Apple Music 目前忙碌中。資料庫稍後會再次重新整理。",
  "problem.loginSave": "無法在這台 Mac 上保存你的 Apple Music 登入。",
```
`es.ts`:
```ts
  "problem.appleSignedOut": "Vuelve a iniciar sesión en Apple Music para actualizar tu biblioteca.",
  "problem.appleUnreachable": "No se pudo conectar con Apple Music. Revisa tu conexión.",
  "problem.appleChanged": "Apple Music cambió su funcionamiento, así que por ahora tu biblioteca no se puede actualizar. Tus canciones siguen funcionando.",
  "problem.appleBusy": "Apple Music está ocupado. Tu biblioteca se actualizará más tarde.",
  "problem.loginSave": "No se pudo guardar tu sesión de Apple Music en este Mac.",
```

- [ ] **Step 2: Write the failing tests**

Create `crates/kara-core/src/apple/mod.rs` with only the module doc and tests for now (`//! …` line from Step 4, then):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::problem::{problem, Problem};
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::Arc;

    #[test]
    fn requests_send_the_web_players_headers_and_lists_follow_every_page_keeping_their_query() {
        let headers = Arc::new(Mutex::new(Vec::new()));
        let seen = headers.clone();
        let get: Get = Box::new(move |url: &str, h: &[(&str, &str)]| {
            seen.lock().unwrap().push(h.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>());
            Ok(if url.ends_with("?offset=2&limit=2&include=catalog") {
                reply(200, r#"{"data":[{"id":"c"}]}"#)
            } else {
                reply(200, r#"{"data":[{"id":"a"},{"id":"b"}],"next":"/v1/me/library/playlists?offset=2"}"#)
            })
        });
        let c = Client::with(Tokens { developer: "dev-secret".into(), user: "user-secret".into(), storefront: "us".into() }, get);
        let ids: Vec<_> = c.all("/v1/me/library/playlists?limit=2&include=catalog").unwrap().iter().map(|v| v["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert_eq!(headers.lock().unwrap()[0], ["Authorization: Bearer dev-secret", "Origin: https://music.apple.com", "media-user-token: user-secret"]);
        let (c, _) = fake_client(|_| reply(404, ""));
        assert!(c.all("/v1/me/library/playlists/p.1/tracks").unwrap().is_empty());
    }

    #[test]
    fn a_refused_login_means_signed_out_after_one_request() {
        for status in [401, 403] {
            let (c, asked) = fake_client(move |_| reply(status, ""));
            assert_eq!(problem(&c.get("/v1/me/storefront").unwrap_err()), Some(Problem::AppleSignedOut));
            assert_eq!(asked.lock().unwrap().len(), 1);
        }
    }

    #[test]
    fn a_busy_apple_music_is_asked_again_once_when_it_says_how_long_to_wait() {
        let busy = |s: u64| Reply { status: 429, retry_after_s: Some(s), body: String::new() };
        let n = AtomicUsize::new(0);
        let (c, _) = fake_client(move |_| if n.fetch_add(1, SeqCst) == 0 { busy(0) } else { reply(200, "{}") });
        c.get("/v1/x").unwrap();
        let (c, asked) = fake_client(move |_| busy(0));
        assert_eq!(problem(&c.get("/v1/x").unwrap_err()), Some(Problem::AppleBusy));
        assert_eq!(asked.lock().unwrap().len(), 2);
        let (c, asked) = fake_client(move |_| busy(3600));
        assert_eq!(problem(&c.get("/v1/x").unwrap_err()), Some(Problem::AppleBusy));
        assert_eq!(asked.lock().unwrap().len(), 1);
    }

    #[test]
    fn the_login_never_shows_in_debug_output_or_error_messages() {
        let tokens = Tokens { developer: "dev-secret".into(), user: "user-secret".into(), storefront: "us".into() };
        let (c, _) = fake_client(|_| reply(500, "dev-secret user-secret"));
        let err = c.get("/v1/x").unwrap_err();
        let (c2, _) = fake_client(|_| reply(200, "not json dev-secret user-secret"));
        let err2 = c2.get("/v1/x").unwrap_err();
        for text in [format!("{tokens:?}"), format!("{err:#}"), format!("{err:?}"), format!("{err2:#}"), format!("{err2:?}")] {
            assert!(!text.contains("user-secret") && !text.contains("dev-secret"), "{text}");
        }
    }
}
```

Add `pub mod apple;` to `crates/kara-core/src/lib.rs`.

- [ ] **Step 3: Run to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core apple::`
Expected: compile errors — `Client`, `Tokens`, `fake_client`, … not found.

- [ ] **Step 4: Write the client**

Replace the top of `apple/mod.rs` (above the tests) with:

```rust
//! Apple Music's web API as music.apple.com's own player calls it: read-only, paced, with the web player's login.

#[cfg(target_os = "macos")]
pub mod keychain;

use crate::problem::Problem;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const API: &str = "https://amp-api.music.apple.com";
const ORIGIN: &str = "https://music.apple.com";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
/// Time between two requests.
const PACE: Duration = Duration::from_millis(if cfg!(test) { 0 } else { 250 });
/// The longest wait a busy Apple Music may ask for before one more try.
const MAX_WAIT: Duration = Duration::from_secs(30);

/// The web player's login: its developer token, the user's token and their storefront. Never printed or logged.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tokens {
    pub developer: String,
    pub user: String,
    pub storefront: String,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens").field("storefront", &self.storefront).finish_non_exhaustive()
    }
}

/// An answer: its status, the seconds a busy answer asks to wait, and its body.
pub struct Reply {
    pub status: u16,
    pub retry_after_s: Option<u64>,
    pub body: String,
}

/// Sends a GET with headers.
pub(crate) type Get = Box<dyn Fn(&str, &[(&str, &str)]) -> Result<Reply> + Send + Sync>;

pub struct Client {
    tokens: Tokens,
    get: Get,
    last: Mutex<Option<Instant>>,
}

impl Client {
    pub fn new(tokens: Tokens) -> Self {
        Self::with(tokens, Box::new(http_get))
    }

    pub(crate) fn with(tokens: Tokens, get: Get) -> Self {
        Self { tokens, get, last: Mutex::new(None) }
    }

    pub fn storefront(&self) -> String {
        self.tokens.storefront.clone()
    }

    /// GETs `path` ("/v1/…"); None when Apple has nothing there. A busy answer is waited out once when it says how long.
    pub fn find(&self, path: &str) -> Result<Option<Value>> {
        let mut waited = false;
        loop {
            let reply = self.send(path)?;
            match reply.status {
                200..=299 => return Ok(Some(serde_json::from_str(&reply.body).context(Problem::AppleChanged)?)),
                404 => return Ok(None),
                401 | 403 => bail!(Problem::AppleSignedOut),
                429 => match reply.retry_after_s.map(Duration::from_secs) {
                    Some(wait) if !waited && wait <= MAX_WAIT => {
                        waited = true;
                        std::thread::sleep(wait);
                    }
                    _ => bail!(Problem::AppleBusy),
                },
                500..=599 => bail!(Problem::AppleUnreachable),
                _ => bail!(Problem::AppleChanged),
            }
        }
    }

    /// GETs `path`; nothing there means Apple changed.
    pub fn get(&self, path: &str) -> Result<Value> {
        self.find(path)?.context(Problem::AppleChanged)
    }

    /// Every item of a list, following its `next` pages; none when Apple has nothing there.
    pub fn all(&self, path: &str) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        let mut next = Some(path.to_string());
        while let Some(page_path) = next.take() {
            let Some(page) = self.find(&page_path)? else { break };
            items.extend(page["data"].as_array().cloned().unwrap_or_default());
            next = page["next"].as_str().map(|n| with_query(n, path));
        }
        Ok(items)
    }

    /// Sends one request, at most one per `PACE`.
    fn send(&self, path: &str) -> Result<Reply> {
        {
            let mut last = self.last.lock().unwrap();
            if let Some(wait) = last.and_then(|at| PACE.checked_sub(at.elapsed())) {
                std::thread::sleep(wait);
            }
            *last = Some(Instant::now());
        }
        let bearer = format!("Bearer {}", self.tokens.developer);
        let headers = [("Authorization", bearer.as_str()), ("Origin", ORIGIN), ("media-user-token", self.tokens.user.as_str())];
        (self.get)(&format!("{API}{path}"), &headers).context(Problem::AppleUnreachable)
    }
}

/// `next` with the parts of `path`'s query it doesn't set itself.
fn with_query(next: &str, path: &str) -> String {
    let (base, own) = next.split_once('?').unwrap_or((next, ""));
    let name = |p: &str| p.split('=').next().unwrap_or_default().to_string();
    let mine: Vec<&str> = own.split('&').filter(|p| !p.is_empty()).collect();
    let taken: Vec<String> = mine.iter().map(|p| name(p)).collect();
    let theirs = path.split_once('?').map_or("", |(_, q)| q).split('&').filter(|p| !p.is_empty() && !taken.contains(&name(p)));
    let query: Vec<&str> = mine.iter().copied().chain(theirs).collect();
    if query.is_empty() { base.to_string() } else { format!("{base}?{}", query.join("&")) }
}

/// One HTTP client for every request, so later ones reuse the connection.
fn http() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

fn http_get(url: &str, headers: &[(&str, &str)]) -> Result<Reply> {
    let mut request = http().get(url);
    for (k, v) in headers {
        request = request.header(*k, *v);
    }
    let resp = request.send()?;
    let retry_after_s = resp.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()?.trim().parse().ok());
    Ok(Reply { status: resp.status().as_u16(), retry_after_s, body: resp.text()? })
}

/// A client for tests: `answer(url)` answers every request and every URL asked is recorded.
#[cfg(test)]
pub(crate) fn fake_client(answer: impl Fn(&str) -> Reply + Send + Sync + 'static) -> (Client, std::sync::Arc<Mutex<Vec<String>>>) {
    let asked = std::sync::Arc::new(Mutex::new(Vec::new()));
    let log = asked.clone();
    let get: Get = Box::new(move |url: &str, _: &[(&str, &str)]| {
        log.lock().unwrap().push(url.to_string());
        Ok(answer(url))
    });
    (Client::with(Tokens { developer: "dev-secret".into(), user: "user-secret".into(), storefront: "us".into() }, get), asked)
}

#[cfg(test)]
pub(crate) fn reply(status: u16, body: impl ToString) -> Reply {
    Reply { status, retry_after_s: None, body: body.to_string() }
}
```

- [ ] **Step 5: The Keychain**

In `crates/kara-core/Cargo.toml`, add under `[target.'cfg(target_os = "macos")'.dependencies]`:

```toml
security-framework = "3"
```

Create `crates/kara-core/src/apple/keychain.rs`:

```rust
//! The Apple Music login, kept in this Mac's Keychain.

use super::Tokens;
use anyhow::Result;
use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

const SERVICE: &str = "world.aako.kara-always-oki";
const ACCOUNT: &str = "apple-music";
/// errSecItemNotFound.
const NOT_FOUND: i32 = -25300;

pub fn load() -> Result<Option<Tokens>> {
    match get_generic_password(SERVICE, ACCOUNT) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.code() == NOT_FOUND => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn save(tokens: &Tokens) -> Result<()> {
    Ok(set_generic_password(SERVICE, ACCOUNT, &serde_json::to_vec(tokens)?)?)
}

pub fn delete() -> Result<()> {
    match delete_generic_password(SERVICE, ACCOUNT) {
        Err(e) if e.code() != NOT_FOUND => Err(e.into()),
        _ => Ok(()),
    }
}
```

The Keychain isn't touched by any test (it is the user's real Keychain, shared by every data folder); the final checklist covers it.

- [ ] **Step 6: Run the tests and checks**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core apple:: && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cd app && npm run check && npm run check:i18n`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/kara-core/Cargo.toml Cargo.lock crates/kara-core/src/lib.rs crates/kara-core/src/apple crates/kara-core/src/problem.rs app/src/lib/api.ts app/src/lib/i18n
git commit -m "feat(core): read-only Apple Music client with the web player's login, kept in the Keychain"
```

---

### Task 4: Reading Apple's answers (with anonymized samples)

**Files:**
- Create: `crates/kara-core/src/apple/parse.rs`
- Create: `crates/kara-core/src/apple/samples/library-playlists.json`, `library-albums.json`, `library-tracks.json`, `catalog-search.json`, `catalog-suggestions.json`, `catalog-album-motion.json`
- Modify: `crates/kara-core/src/apple/mod.rs` (`pub mod parse;`)

**Interfaces:**
- Consumes: `library::{StreamingTrack, StreamingCollection, CollectionKind}` (Task 1).
- Produces (in `kara_core::apple::parse`, each taking `&serde_json::Value`):
  - `song(item) -> Option<StreamingTrack>` — `songs`/`library-songs` only; `provider_ref` = catalog id when known, else the library id; ISRC and album id from the catalog song (`include=catalog`); artwork at 600×600
  - `playlist(item) -> Option<StreamingCollection>` — `version` = `lastModifiedDate` only (None: its songs are read on every refresh — Apple-made and followed playlists change without the user); `artwork` = its own, else the catalog playlist's, at 600×600; `catalog_ref` = `playParams.globalId` or the catalog relationship's id; `subtitle` = the catalog playlist's `curatorName`
  - `album(item) -> Option<StreamingCollection>` — `subtitle` = `artistName`; `artwork` at 600×600; `catalog_ref` = catalog id; `version` = `"<dateAdded>:<trackCount>"`
  - Artwork templates have `{w}x{h}` and may have `{c}` / `{f}`; they become `600x600`, `bb` and `jpg`.
  - `motion(answer) -> Option<String>` — `motionDetailSquare`, else `motionSquareVideo1x1`
  - `storefront(answer) -> Option<String>`, `display_name(answer) -> Option<String>`
  - `search_songs(answer) -> Vec<StreamingTrack>`, `suggestions(answer) -> Vec<String>` (terms only)

- [ ] **Step 1: Add the samples**

The catalog samples are trimmed from real answers (field names and nesting as Apple sends them) with made-up names, ids and URLs; the library samples follow Apple's documented `LibraryPlaylists` / `LibraryAlbums` / `LibrarySongs` resources with `include=catalog`.

`crates/kara-core/src/apple/samples/library-playlists.json`:
```json
{
  "next": "/v1/me/library/playlists?offset=2",
  "data": [
    {
      "id": "p.MadeUp0001",
      "type": "library-playlists",
      "href": "/v1/me/library/playlists/p.MadeUp0001",
      "attributes": {
        "name": "Late Night Drive",
        "canEdit": true,
        "isPublic": false,
        "hasCatalog": false,
        "dateAdded": "2025-11-02T09:15:00Z",
        "lastModifiedDate": "2026-09-20T21:04:11Z",
        "playParams": { "id": "p.MadeUp0001", "kind": "playlist", "isLibrary": true }
      }
    },
    {
      "id": "p.MadeUp0002",
      "type": "library-playlists",
      "href": "/v1/me/library/playlists/p.MadeUp0002",
      "attributes": {
        "name": "Harbor Hits",
        "canEdit": false,
        "hasCatalog": true,
        "dateAdded": "2026-01-05T12:00:00Z",
        "lastModifiedDate": "2026-09-26T04:00:00Z",
        "artwork": { "url": "https://is1-ssl.mzstatic.com/image/thumb/Features/v4/ma/de/up/madeup.jpg/{w}x{h}{c}.{f}", "width": 1080, "height": 1080 },
        "playParams": { "id": "p.MadeUp0002", "kind": "playlist", "isLibrary": true, "globalId": "pl.madeup00000000000000000000000002" }
      },
      "relationships": {
        "catalog": {
          "href": "/v1/me/library/playlists/p.MadeUp0002/catalog",
          "data": [{ "id": "pl.madeup00000000000000000000000002", "type": "playlists", "attributes": { "name": "Harbor Hits", "curatorName": "Made Up Radio" } }]
        }
      }
    }
  ]
}
```

`samples/library-albums.json`:
```json
{
  "data": [
    {
      "id": "l.MadeUp0003",
      "type": "library-albums",
      "href": "/v1/me/library/albums/l.MadeUp0003",
      "attributes": {
        "name": "Night Line",
        "artistName": "Juniper Row",
        "trackCount": 2,
        "dateAdded": "2026-03-14T18:22:05Z",
        "artwork": { "url": "https://is1-ssl.mzstatic.com/image/thumb/Music/v4/ma/de/up/madeup.jpg/{w}x{h}bb.jpg", "width": 1200, "height": 1200 },
        "playParams": { "id": "l.MadeUp0003", "kind": "album", "isLibrary": true }
      },
      "relationships": {
        "catalog": { "href": "/v1/me/library/albums/l.MadeUp0003/catalog", "data": [{ "id": "900001", "type": "albums", "attributes": { "name": "Night Line" } }] }
      }
    }
  ]
}
```

`samples/library-tracks.json`:
```json
{
  "data": [
    {
      "id": "i.MadeUp0004",
      "type": "library-songs",
      "href": "/v1/me/library/songs/i.MadeUp0004",
      "attributes": {
        "name": "Neon Tidewater",
        "artistName": "Juniper Row",
        "albumName": "Night Line",
        "durationInMillis": 214000,
        "artwork": { "url": "https://is1-ssl.mzstatic.com/image/thumb/Music/v4/ma/de/up/madeup.jpg/{w}x{h}bb.jpg", "width": 1200, "height": 1200 },
        "playParams": { "id": "i.MadeUp0004", "kind": "song", "isLibrary": true, "catalogId": "900002" }
      },
      "relationships": {
        "catalog": {
          "href": "/v1/me/library/songs/i.MadeUp0004/catalog",
          "data": [{ "id": "900002", "type": "songs", "attributes": { "name": "Neon Tidewater", "isrc": "QZZZZ2600001", "url": "https://music.apple.com/us/album/night-line/900001?i=900002" } }]
        }
      }
    },
    {
      "id": "i.MadeUp0005",
      "type": "library-songs",
      "href": "/v1/me/library/songs/i.MadeUp0005",
      "attributes": {
        "name": "Kitchen Demo 3",
        "artistName": "Mina Okada",
        "durationInMillis": 151000,
        "playParams": { "id": "i.MadeUp0005", "kind": "song", "isLibrary": true }
      }
    },
    {
      "id": "i.MadeUp0006",
      "type": "library-music-videos",
      "href": "/v1/me/library/music-videos/i.MadeUp0006",
      "attributes": { "name": "Neon Tidewater (Video)", "artistName": "Juniper Row", "playParams": { "id": "i.MadeUp0006", "kind": "musicVideo", "isLibrary": true } }
    }
  ]
}
```

`samples/catalog-search.json`:
```json
{
  "results": {
    "songs": {
      "href": "/v1/catalog/us/search?limit=10&term=paper+boats&types=songs",
      "data": [
        {
          "id": "900010",
          "type": "songs",
          "href": "/v1/catalog/us/songs/900010",
          "attributes": {
            "albumName": "Harbor Lights",
            "artistName": "Juniper Row",
            "artwork": { "bgColor": "ffffff", "height": 3000, "width": 3000, "url": "https://is1-ssl.mzstatic.com/image/thumb/Music211/v4/ma/de/up/madeup.jpg/{w}x{h}bb.jpg" },
            "durationInMillis": 200000,
            "isrc": "QZZZZ2600010",
            "name": "Paper Boats",
            "playParams": { "id": "900010", "kind": "song" },
            "url": "https://music.apple.com/us/album/paper-boats/900009?i=900010"
          }
        }
      ]
    }
  },
  "meta": { "results": { "order": ["songs"], "rawOrder": ["songs"] } }
}
```

`samples/catalog-suggestions.json`:
```json
{
  "results": {
    "suggestions": [
      { "kind": "terms", "searchTerm": "paper boats", "displayTerm": "paper boats" },
      { "kind": "terms", "searchTerm": "paper boats juniper row", "displayTerm": "paper boats juniper row" },
      { "kind": "topResults", "content": { "id": "900010", "type": "songs", "attributes": { "name": "Paper Boats" } } }
    ]
  }
}
```

`samples/catalog-album-motion.json`:
```json
{
  "data": [
    {
      "id": "900001",
      "type": "albums",
      "href": "/v1/catalog/us/albums/900001",
      "attributes": {
        "editorialVideo": {
          "motionDetailSquare": {
            "previewFrame": { "url": "https://is1-ssl.mzstatic.com/image/thumb/Video/v4/ma/de/up/preview.png/{w}x{h}bb.jpg", "width": 3840, "height": 3840 },
            "video": "https://mvod.itunes.apple.com/itunes-assets/HLSMusic/v4/ma/de/up/P000000001_default.m3u8"
          },
          "motionDetailTall": {
            "previewFrame": { "url": "https://is1-ssl.mzstatic.com/image/thumb/Video/v4/ma/de/up/tall.png/{w}x{h}bb.jpg", "width": 3072, "height": 4096 },
            "video": "https://mvod.itunes.apple.com/itunes-assets/HLSMusic/v4/ma/de/up/P000000002_default.m3u8"
          },
          "motionSquareVideo1x1": {
            "previewFrame": { "url": "https://is1-ssl.mzstatic.com/image/thumb/Video/v4/ma/de/up/preview.png/{w}x{h}bb.jpg", "width": 3840, "height": 3840 },
            "video": "https://mvod.itunes.apple.com/itunes-assets/HLSMusic/v4/ma/de/up/P000000001_default.m3u8"
          }
        }
      }
    }
  ]
}
```

- [ ] **Step 2: Write the failing tests**

Create `crates/kara-core/src/apple/parse.rs` with the doc line from Step 4 and these tests; add `pub mod parse;` to `apple/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn library_playlists_read_with_their_catalog_curator_and_when_they_changed() {
        let answer = sample(include_str!("samples/library-playlists.json"));
        let lists: Vec<_> = answer["data"].as_array().unwrap().iter().filter_map(playlist).collect();
        assert_eq!(
            lists,
            vec![
                StreamingCollection {
                    kind: CollectionKind::Playlist,
                    provider_ref: "p.MadeUp0001".into(),
                    name: "Late Night Drive".into(),
                    subtitle: None,
                    artwork: None,
                    catalog_ref: None,
                    version: Some("2026-09-20T21:04:11Z".into()),
                },
                StreamingCollection {
                    kind: CollectionKind::Playlist,
                    provider_ref: "p.MadeUp0002".into(),
                    name: "Harbor Hits".into(),
                    subtitle: Some("Made Up Radio".into()),
                    artwork: Some("https://is1-ssl.mzstatic.com/image/thumb/Features/v4/ma/de/up/madeup.jpg/600x600bb.jpg".into()),
                    catalog_ref: Some("pl.madeup00000000000000000000000002".into()),
                    version: Some("2026-09-26T04:00:00Z".into()),
                },
            ]
        );
    }

    #[test]
    fn library_albums_read_with_their_catalog_album_and_a_version_that_changes_with_their_songs() {
        let answer = sample(include_str!("samples/library-albums.json"));
        assert_eq!(
            album(&answer["data"][0]),
            Some(StreamingCollection {
                kind: CollectionKind::Album,
                provider_ref: "l.MadeUp0003".into(),
                name: "Night Line".into(),
                subtitle: Some("Juniper Row".into()),
                artwork: Some("https://is1-ssl.mzstatic.com/image/thumb/Music/v4/ma/de/up/madeup.jpg/600x600bb.jpg".into()),
                catalog_ref: Some("900001".into()),
                version: Some("2026-03-14T18:22:05Z:2".into()),
            })
        );
    }

    #[test]
    fn songs_read_with_isrc_album_and_artwork_and_videos_are_skipped() {
        let answer = sample(include_str!("samples/library-tracks.json"));
        let songs: Vec<_> = answer["data"].as_array().unwrap().iter().filter_map(song).collect();
        assert_eq!(
            songs,
            vec![
                StreamingTrack {
                    provider_ref: "900002".into(),
                    title: "Neon Tidewater".into(),
                    artist: Some("Juniper Row".into()),
                    album: Some("Night Line".into()),
                    duration_ms: Some(214_000),
                    isrc: Some("QZZZZ2600001".into()),
                    album_ref: Some("900001".into()),
                    artwork: Some("https://is1-ssl.mzstatic.com/image/thumb/Music/v4/ma/de/up/madeup.jpg/600x600bb.jpg".into()),
                },
                StreamingTrack { provider_ref: "i.MadeUp0005".into(), title: "Kitchen Demo 3".into(), artist: Some("Mina Okada".into()), album: None, duration_ms: Some(151_000), isrc: None, album_ref: None, artwork: None },
            ]
        );
    }

    #[test]
    fn catalog_answers_give_songs_phrases_animated_covers_the_storefront_and_the_name() {
        let found = search_songs(&sample(include_str!("samples/catalog-search.json")));
        assert_eq!((found.len(), found[0].provider_ref.as_str(), found[0].isrc.as_deref(), found[0].album_ref.as_deref()), (1, "900010", Some("QZZZZ2600010"), Some("900009")));
        assert_eq!(suggestions(&sample(include_str!("samples/catalog-suggestions.json"))), ["paper boats", "paper boats juniper row"]);
        assert_eq!(
            motion(&sample(include_str!("samples/catalog-album-motion.json"))).as_deref(),
            Some("https://mvod.itunes.apple.com/itunes-assets/HLSMusic/v4/ma/de/up/P000000001_default.m3u8")
        );
        assert_eq!(motion(&json!({ "data": [{ "id": "900003", "type": "albums", "attributes": {} }] })), None);
        assert_eq!(storefront(&json!({ "data": [{ "id": "jp", "type": "storefronts", "attributes": { "name": "Japan" } }] })).as_deref(), Some("jp"));
        assert_eq!(display_name(&json!({ "data": [{ "id": "sp.madeup", "type": "social-profiles", "attributes": { "name": "Mina" } }] })).as_deref(), Some("Mina"));
    }
}
```

- [ ] **Step 3: Run to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core apple::parse`
Expected: compile errors — `playlist`, `album`, `song`, … not found.

- [ ] **Step 4: Write the parsers**

Top of `apple/parse.rs`:

```rust
//! Apple Music answers turned into the library's streaming songs and collections.

use crate::library::{CollectionKind, StreamingCollection, StreamingTrack};
use serde_json::Value;

/// The size artwork is asked for.
const ARTWORK_SIZE: &str = "600x600";

fn text(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(String::from)
}

/// An artwork URL template ("…/{w}x{h}bb.jpg", "…/{w}x{h}{c}.{f}") at the size the app shows.
fn artwork(attrs: &Value) -> Option<String> {
    text(&attrs["artwork"]["url"]).map(|u| u.replace("{w}x{h}", ARTWORK_SIZE).replace("{c}", "bb").replace("{f}", "jpg"))
}

/// The album id in a catalog song's link ("…/album/<name>/<id>?i=…").
fn album_in_url(url: &str) -> Option<String> {
    let id = url.split("/album/").nth(1)?.split(['?', '#']).next()?.rsplit('/').next()?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).then(|| id.to_string())
}

/// The catalog resource a library resource is linked to (`include=catalog`), or null.
fn catalog(item: &Value) -> &Value {
    &item["relationships"]["catalog"]["data"][0]
}

/// A song from a library or catalog song; None for anything else (music videos, episodes).
pub fn song(item: &Value) -> Option<StreamingTrack> {
    if !matches!(item["type"].as_str(), Some("songs" | "library-songs")) {
        return None;
    }
    let attrs = &item["attributes"];
    let linked = catalog(item);
    let catalog_attrs = if linked.is_null() { attrs } else { &linked["attributes"] };
    Some(StreamingTrack {
        provider_ref: text(&attrs["playParams"]["catalogId"]).or_else(|| text(&linked["id"])).or_else(|| text(&item["id"]))?,
        title: text(&attrs["name"])?,
        artist: text(&attrs["artistName"]),
        album: text(&attrs["albumName"]),
        duration_ms: attrs["durationInMillis"].as_i64(),
        isrc: text(&catalog_attrs["isrc"]),
        album_ref: text(&catalog_attrs["url"]).and_then(|u| album_in_url(&u)),
        artwork: artwork(attrs).or_else(|| artwork(catalog_attrs)),
    })
}

/// A library playlist; without `lastModifiedDate` it has no version, so its songs are read on every refresh.
pub fn playlist(item: &Value) -> Option<StreamingCollection> {
    let attrs = &item["attributes"];
    Some(StreamingCollection {
        kind: CollectionKind::Playlist,
        provider_ref: text(&item["id"])?,
        name: text(&attrs["name"])?,
        subtitle: text(&catalog(item)["attributes"]["curatorName"]),
        artwork: artwork(attrs).or_else(|| artwork(&catalog(item)["attributes"])),
        catalog_ref: text(&attrs["playParams"]["globalId"]).or_else(|| text(&catalog(item)["id"])),
        version: text(&attrs["lastModifiedDate"]),
    })
}

/// A library album.
pub fn album(item: &Value) -> Option<StreamingCollection> {
    let attrs = &item["attributes"];
    Some(StreamingCollection {
        kind: CollectionKind::Album,
        provider_ref: text(&item["id"])?,
        name: text(&attrs["name"])?,
        subtitle: text(&attrs["artistName"]),
        artwork: artwork(attrs),
        catalog_ref: text(&catalog(item)["id"]).or_else(|| text(&attrs["playParams"]["catalogId"])),
        version: Some(format!("{}:{}", text(&attrs["dateAdded"]).unwrap_or_default(), attrs["trackCount"].as_i64().unwrap_or(0))),
    })
}

/// The square looping video in an `extend=editorialVideo` answer, if the album or playlist has one.
pub fn motion(answer: &Value) -> Option<String> {
    let video = &answer["data"][0]["attributes"]["editorialVideo"];
    ["motionDetailSquare", "motionSquareVideo1x1"].iter().find_map(|k| text(&video[*k]["video"]))
}

pub fn storefront(answer: &Value) -> Option<String> {
    text(&answer["data"][0]["id"])
}

/// The name on the user's Apple Music profile.
pub fn display_name(answer: &Value) -> Option<String> {
    text(&answer["data"][0]["attributes"]["name"])
}

pub fn search_songs(answer: &Value) -> Vec<StreamingTrack> {
    answer["results"]["songs"]["data"].as_array().into_iter().flatten().filter_map(song).collect()
}

/// The search phrases in a suggestions answer.
pub fn suggestions(answer: &Value) -> Vec<String> {
    answer["results"]["suggestions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["kind"] == "terms")
        .filter_map(|s| text(&s["displayTerm"]).or_else(|| text(&s["searchTerm"])))
        .collect()
}
```

- [ ] **Step 5: Run the tests and checks**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core apple::parse && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/kara-core/src/apple
git commit -m "feat(core): read Apple Music playlists, albums, songs, animated covers and phrases"
```

---

### Task 5: Sync and disconnect

**Files:**
- Create: `crates/kara-core/src/apple/sync.rs`
- Modify: `crates/kara-core/src/apple/mod.rs` (`pub mod sync;`)

**Interfaces:**
- Consumes: `Client::{all, get}`, `fake_client`, `reply` (Task 3); `parse::{playlist, album, song}` (Task 4); Task 1's library methods.
- Produces (in `kara_core::apple::sync`):
  - `sync(lib: &Library, client: &Client, keep: &[i64], listed: &mut dyn FnMut()) -> Result<()>` — lists playlists and albums (adding, renaming, deleting gone ones), calls `listed()`, reads the songs of each collection whose version changed or that has none, rebuilds Apple artists from album artists, prunes songs in no collection that were never prepared (except `keep`), sets `refreshed_at`. A failure stops it where it is: what it saved stays, nothing is pruned.
  - `disconnect(lib: &Library, keep: &[i64]) -> Result<()>` — deletes Apple's collections and account; keeps songs that were prepared, are in your own playlists, or are in `keep`.

- [ ] **Step 1: Write the failing tests**

Create `crates/kara-core/src/apple/sync.rs` with the doc line from Step 3 and:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::apple::{fake_client, reply};
    use crate::library::{CollectionKind, ProviderId, SourceKind};
    use crate::problem::{problem, Problem};
    use serde_json::{json, Value};

    fn playlist(id: &str, name: &str, version: &str) -> Value {
        json!({ "id": id, "type": "library-playlists", "attributes": { "name": name, "lastModifiedDate": version, "playParams": { "id": id, "kind": "playlist", "isLibrary": true } } })
    }

    /// A followed or Apple-made playlist: Apple gives it no `lastModifiedDate`.
    fn followed(id: &str, name: &str) -> Value {
        json!({ "id": id, "type": "library-playlists", "attributes": { "name": name, "playParams": { "id": id, "kind": "playlist", "isLibrary": true } } })
    }

    fn album(id: &str, name: &str, artist: &str) -> Value {
        json!({ "id": id, "type": "library-albums", "attributes": { "name": name, "artistName": artist, "trackCount": 1, "dateAdded": "2026-03-14T18:22:05Z" } })
    }

    fn song(id: &str, title: &str, artist: &str) -> Value {
        json!({ "id": format!("i.{id}"), "type": "library-songs", "attributes": { "name": title, "artistName": artist, "durationInMillis": 200_000, "playParams": { "id": format!("i.{id}"), "kind": "song", "isLibrary": true, "catalogId": id } } })
    }

    /// A client answering each URL from the first entry whose key it contains, else "nothing there".
    fn apple(answers: Vec<(&'static str, Vec<Value>)>) -> (Client, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        fake_client(move |url| match answers.iter().find(|(key, _)| url.contains(key)) {
            Some((_, items)) => reply(200, json!({ "data": items })),
            None => reply(404, ""),
        })
    }

    fn names(lib: &Library, kind: CollectionKind) -> Vec<String> {
        lib.collections(Some(ProviderId::Apple), kind).unwrap().into_iter().map(|c| c.name).collect()
    }

    fn songs_of(lib: &Library, kind: CollectionKind, name: &str) -> Vec<String> {
        let c = lib.collections(Some(ProviderId::Apple), kind).unwrap().into_iter().find(|c| c.name == name).unwrap();
        lib.collection_tracks(c.id).unwrap().into_iter().map(|t| t.title).collect()
    }

    fn first_sync(lib: &Library) {
        lib.connect_account(ProviderId::Apple, None).unwrap();
        let (c, _) = apple(vec![
            ("library/playlists?", vec![playlist("p.1", "Late Night Drive", "v1"), playlist("p.2", "Rainy Day", "v1"), followed("p.3", "Harbor Hits")]),
            ("library/albums?", vec![album("l.1", "Night Line", "Juniper Row")]),
            ("playlists/p.1/tracks", vec![song("901", "Neon Tidewater", "Juniper Row"), song("902", "Umbrella Weather", "Odd Hours Club")]),
            ("playlists/p.3/tracks", vec![song("907", "Salt Road", "Mina Okada")]),
            ("playlists/p.2/tracks", vec![song("903", "Kettle Rain", "Ada Vale"), song("904", "Window Seat", "Ada Vale"), song("906", "Paper Lanterns", "Ada Vale")]),
            ("albums/l.1/tracks", vec![song("901", "Neon Tidewater", "Juniper Row")]),
        ]);
        let mut listed = 0;
        sync(lib, &c, &[], &mut || listed += 1).unwrap();
        assert_eq!(listed, 1);
    }

    fn id_of(lib: &Library, title: &str) -> i64 {
        lib.search(title, 1).unwrap()[0].id
    }

    #[test]
    fn the_first_sync_brings_playlists_albums_their_songs_and_album_artists() {
        let lib = Library::open_in_memory().unwrap();
        first_sync(&lib);
        assert_eq!(names(&lib, CollectionKind::Playlist), ["Harbor Hits", "Late Night Drive", "Rainy Day"]);
        assert_eq!((names(&lib, CollectionKind::Album), names(&lib, CollectionKind::Artist)), (vec!["Night Line".to_string()], vec!["Juniper Row".to_string()]));
        assert_eq!(songs_of(&lib, CollectionKind::Playlist, "Late Night Drive"), ["Neon Tidewater", "Umbrella Weather"]);
        assert_eq!(songs_of(&lib, CollectionKind::Artist, "Juniper Row"), ["Neon Tidewater"]);
        assert_eq!(lib.track(id_of(&lib, "Neon Tidewater")).unwrap().provider, ProviderId::Apple);
        assert!(lib.account(ProviderId::Apple).unwrap().unwrap().refreshed_at.is_some());
    }

    #[test]
    fn a_later_sync_refetches_only_changed_collections_drops_gone_ones_and_keeps_sung_or_queued_songs() {
        let lib = Library::open_in_memory().unwrap();
        first_sync(&lib);
        let sung = id_of(&lib, "Kettle Rain");
        let s = lib.add_source(sung, SourceKind::Match, "https://www.youtube.com/watch?v=aaaaaaaaaaa", None).unwrap();
        lib.set_source_audio(s, "h", 1000).unwrap();
        let queued = id_of(&lib, "Window Seat");
        let (c, asked) = apple(vec![
            ("library/playlists?", vec![playlist("p.1", "Late Night Drive", "v2"), followed("p.3", "Harbor Hits")]),
            ("library/albums?", vec![album("l.1", "Night Line", "Juniper Row")]),
            ("playlists/p.1/tracks", vec![song("902", "Umbrella Weather", "Odd Hours Club"), song("905", "Glass Harbor", "Juniper Row")]),
            ("playlists/p.3/tracks", vec![song("908", "Tin Roof", "Mina Okada")]),
            ("albums/l.1/tracks", vec![song("901", "Neon Tidewater", "Juniper Row")]),
        ]);
        sync(&lib, &c, &[queued], &mut || {}).unwrap();
        assert_eq!(names(&lib, CollectionKind::Playlist), ["Harbor Hits", "Late Night Drive"]);
        assert_eq!(songs_of(&lib, CollectionKind::Playlist, "Late Night Drive"), ["Umbrella Weather", "Glass Harbor"]);
        assert_eq!(songs_of(&lib, CollectionKind::Playlist, "Harbor Hits"), ["Tin Roof"], "a playlist without a version is read every time");
        assert!(!asked.lock().unwrap().iter().any(|u| u.contains("albums/l.1/tracks")));
        assert!(!lib.search("Neon Tidewater", 1).unwrap().is_empty(), "still on its album");
        assert!(lib.track(sung).is_ok(), "sung");
        assert!(lib.track(queued).is_ok(), "queued");
        assert!(lib.search("Paper Lanterns", 1).unwrap().is_empty());
    }

    #[test]
    fn a_sync_that_fails_leaves_the_library_as_it_was() {
        let lib = Library::open_in_memory().unwrap();
        first_sync(&lib);
        let before = lib.account(ProviderId::Apple).unwrap();
        let (c, _) = fake_client(|_| reply(500, ""));
        assert_eq!(problem(&sync(&lib, &c, &[], &mut || {}).unwrap_err()), Some(Problem::AppleUnreachable));
        assert_eq!(names(&lib, CollectionKind::Playlist), ["Harbor Hits", "Late Night Drive", "Rainy Day"]);
        assert_eq!(songs_of(&lib, CollectionKind::Playlist, "Rainy Day"), ["Kettle Rain", "Window Seat", "Paper Lanterns"]);
        assert_eq!(lib.account(ProviderId::Apple).unwrap(), before);
    }

    #[test]
    fn disconnecting_removes_apple_music_but_keeps_songs_sung_or_in_your_own_playlists() {
        let lib = Library::open_in_memory().unwrap();
        first_sync(&lib);
        let mine = lib.create_playlist("Friday Mix").unwrap();
        lib.add_to_collection(mine, id_of(&lib, "Neon Tidewater")).unwrap();
        let sung = id_of(&lib, "Kettle Rain");
        let s = lib.add_source(sung, SourceKind::Match, "https://www.youtube.com/watch?v=aaaaaaaaaaa", None).unwrap();
        lib.set_source_audio(s, "h", 1000).unwrap();
        let other = id_of(&lib, "Umbrella Weather");
        disconnect(&lib, &[]).unwrap();
        for kind in [CollectionKind::Playlist, CollectionKind::Album, CollectionKind::Artist] {
            assert!(names(&lib, kind).is_empty());
        }
        assert_eq!(lib.account(ProviderId::Apple).unwrap(), None);
        assert_eq!(lib.collection_tracks(mine).unwrap()[0].title, "Neon Tidewater");
        assert!(lib.track(sung).is_ok());
        assert!(lib.track(other).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs your Apple Music login in the Keychain; prints counts only"]
    fn live_library_has_what_sync_reads() {
        let client = Client::new(crate::apple::keychain::load().unwrap().expect("connect Apple Music in the app first"));
        let playlists = client.all(PLAYLISTS).unwrap();
        let albums = client.all(ALBUMS).unwrap();
        let first = playlists.iter().find_map(parse::playlist).expect("a playlist");
        let songs: Vec<_> = client.all(&songs_path(first.kind, &first.provider_ref)).unwrap().iter().filter_map(parse::song).collect();
        println!(
            "playlists {} (read {}), albums {} (read {}), first playlist songs {}, with ISRC {}, with album {}, with artwork {}",
            playlists.len(),
            playlists.iter().filter_map(parse::playlist).count(),
            albums.len(),
            albums.iter().filter_map(parse::album).count(),
            songs.len(),
            songs.iter().filter(|s| s.isrc.is_some()).count(),
            songs.iter().filter(|s| s.album_ref.is_some()).count(),
            songs.iter().filter(|s| s.artwork.is_some()).count(),
        );
    }
}
```

- [ ] **Step 2: Run to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core apple::sync`
Expected: compile errors — `sync`, `disconnect`, `PLAYLISTS`, `songs_path` not found.

- [ ] **Step 3: Write the sync**

Top of `apple/sync.rs` (and `pub mod sync;` in `apple/mod.rs`):

```rust
//! Brings the user's Apple Music playlists, albums and album artists into the library, and takes them out again.

use super::{parse, Client};
use crate::library::{CollectionKind, Library, ProviderId};
use anyhow::Result;

const PLAYLISTS: &str = "/v1/me/library/playlists?limit=100&include%5Blibrary-playlists%5D=catalog";
const ALBUMS: &str = "/v1/me/library/albums?limit=100&include%5Blibrary-albums%5D=catalog";

/// Where a library playlist's or album's songs are listed, with their catalog songs.
fn songs_path(kind: CollectionKind, id: &str) -> String {
    let what = if kind == CollectionKind::Playlist { "playlists" } else { "albums" };
    format!("/v1/me/library/{what}/{id}/tracks?limit=100&include%5Blibrary-songs%5D=catalog")
}

/// Adds and updates Apple Music's playlists and albums and deletes gone ones, calls `listed`, reads the songs
/// of those that changed, rebuilds the album artists, and prunes songs no longer anywhere except those in `keep`.
/// A failure stops it where it is: what it saved stays, and nothing is pruned.
pub fn sync(lib: &Library, client: &Client, keep: &[i64], listed: &mut dyn FnMut()) -> Result<()> {
    let mut remote: Vec<_> = client.all(PLAYLISTS)?.iter().filter_map(parse::playlist).collect();
    remote.extend(client.all(ALBUMS)?.iter().filter_map(parse::album));
    let ids = remote.iter().map(|c| lib.upsert_streaming_collection(ProviderId::Apple, c)).collect::<Result<Vec<_>>>()?;
    for kind in [CollectionKind::Playlist, CollectionKind::Album] {
        lib.delete_collections_except(ProviderId::Apple, kind, &ids)?;
    }
    listed();
    for (c, &id) in remote.iter().zip(&ids) {
        if c.version.is_some() && lib.collection_version(id)? == c.version {
            continue;
        }
        let songs = client.all(&songs_path(c.kind, &c.provider_ref))?;
        let tracks = songs.iter().filter_map(parse::song).map(|t| lib.upsert_streaming_track(ProviderId::Apple, &t)).collect::<Result<Vec<_>>>()?;
        lib.set_collection_tracks(id, &tracks, c.version.as_deref())?;
    }
    rebuild_artists(lib)?;
    lib.prune_streaming_tracks(ProviderId::Apple, keep)?;
    lib.set_refreshed(ProviderId::Apple, crate::now_ms())
}

/// One Apple artist per album artist (same name ignoring case), holding their albums' songs.
fn rebuild_artists(lib: &Library) -> Result<()> {
    let mut artists: Vec<(String, Vec<i64>)> = Vec::new();
    for album in lib.collections(Some(ProviderId::Apple), CollectionKind::Album)? {
        let Some(name) = album.subtitle else { continue };
        let songs = lib.collection_tracks(album.id)?.into_iter().map(|t| t.id);
        match artists.iter_mut().find(|(n, _)| n.to_lowercase() == name.to_lowercase()) {
            Some((_, ids)) => ids.extend(songs),
            None => artists.push((name, songs.collect())),
        }
    }
    let mut ids = Vec::new();
    for (name, songs) in &artists {
        let id = lib.upsert_collection(ProviderId::Apple, CollectionKind::Artist, &format!("artist:{}", name.to_lowercase()), name, None)?;
        lib.set_collection_tracks(id, songs, None)?;
        ids.push(id);
    }
    lib.delete_collections_except(ProviderId::Apple, CollectionKind::Artist, &ids)
}

/// Takes Apple Music out of the library: its collections and account, and its songs except those prepared,
/// in your own playlists or in `keep`.
pub fn disconnect(lib: &Library, keep: &[i64]) -> Result<()> {
    for kind in [CollectionKind::Playlist, CollectionKind::Album, CollectionKind::Artist] {
        lib.delete_collections_except(ProviderId::Apple, kind, &[])?;
    }
    lib.prune_streaming_tracks(ProviderId::Apple, keep)?;
    lib.delete_account(ProviderId::Apple)
}
```

- [ ] **Step 4: Run the tests and checks**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core apple:: && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all pass (the live test stays ignored).

- [ ] **Step 5: Commit**

```bash
git add crates/kara-core/src/apple
git commit -m "feat(core): sync Apple Music playlists, albums and artists into the library"
```

---

### Task 6: Matching a streaming song to an upload (on Phase 2's shared YouTube search)

**Files:**
- Create: `crates/kara-core/src/ingest/ytmusic.rs`, `crates/kara-core/src/ingest/ytmusic-search.sample.json`, `crates/kara-core/src/matching.rs`
- Modify: `crates/kara-core/src/ingest/mod.rs` (`pub mod ytmusic;`, `find_on_youtube` moved here), `crates/kara-core/src/ingest/youtube.rs` (`clock_ms` → `pub(crate)`), `crates/kara-core/src/lyrics.rs` (`key`, `similar`, `singer_likeness`, `words` → `pub(crate)`), `crates/kara-core/src/lib.rs` (`pub mod matching;`), `crates/kara-core/src/jobs.rs` (`prepare_inner`; `rename` leaves streaming songs alone), `crates/kara-core/src/problem.rs`
- Modify (read them first — Phase 2's code): `app/src-tauri/src/adding.rs` (`find_on_youtube` leaves for kara-core) and every caller of `adding::find_on_youtube` (the `youtube_search` command and Phase 2's phone search)
- Modify: `app/src/lib/api.ts` (`ProblemCode`), `app/src/lib/i18n/*.ts`

**Interfaces:**
- Consumes: `Library::{track, track_isrc, add_match, sources, select_source, selected_source}` (Tasks 1–2); `ingest::youtube::search`, `ingest::ytdlp`, `ingest::preview::{search, SearchHit, LinkPreview}`; `lyrics::{clean_title, clean_artist}`.
- Produces:
  - `ingest::ytmusic::{Song { hit: SearchHit, album: Option<String> }, songs(query: &str) -> Result<Vec<Song>>, parse_songs(json: &str) -> Result<Vec<Song>>}`
  - `ingest::find_on_youtube(store: &Store, query: &str) -> Result<Vec<SearchHit>>` — Phase 2's `adding::find_on_youtube` (YouTube's web search, else yt-dlp's), moved unchanged into kara-core so the app, the phones and matching share one function
  - `matching::Finder` trait `{ fn songs(&self, query: &str) -> Result<Vec<Song>>; fn videos(&self, query: &str) -> Result<Vec<SearchHit>>; }`, `matching::YouTube { store: Store }`
  - `matching::score(track: &Track, hit: &SearchHit, album: Option<&str>, official: bool) -> Option<f64>`
  - `matching::find(track: &Track, isrc: Option<&str>, finder: &dyn Finder) -> Result<Vec<(Option<f64>, SearchHit)>>` — accepted best first, then refused in found order, at most 8
  - `matching::search_once(lib: &Library, track_id: i64, finder: &dyn Finder) -> Result<()>` — saves them, puts the best accepted one in use
  - `matching::ensure(lib: &Library, track_id: i64, finder: &dyn Finder) -> Result<()>` — searches only when the song has no uploads; `Problem::NoMatch` when none is in use
  - `Problem::{NoMatch, MatchLookup}`; `jobs::prepare` matches a streaming song before fetching it; the lyrics swap-retry (`jobs::rename`) never renames a streaming song (the service's names are kept).

- [ ] **Step 1: Problems and their text**

`problem.rs` variants and `Display` arms:

```rust
    NoMatch,
    MatchLookup,
```

```rust
            Self::NoMatch => "No singable version found. Use Change match to pick one or paste a link.",
            Self::MatchLookup => "Couldn't look for a version to sing. Check your connection.",
```

`api.ts`: add `| "noMatch" | "matchLookup"` to `ProblemCode`. Append to the i18n files:

- `en.ts`: `"problem.noMatch": "No singable version found. Use Change match to pick one or paste a link.",` `"problem.matchLookup": "Couldn't look for a version to sing. Check your connection.",`
- `ja.ts`: `"problem.noMatch": "歌えるバージョンが見つかりませんでした。「バージョンを変更」で選ぶか、リンクを貼り付けてください。",` `"problem.matchLookup": "歌うバージョンを探せませんでした。インターネット接続を確認してください。",`
- `ko.ts`: `"problem.noMatch": "부를 수 있는 버전을 찾지 못했습니다. '버전 변경'에서 고르거나 링크를 붙여 넣으세요.",` `"problem.matchLookup": "부를 버전을 찾을 수 없습니다. 인터넷 연결을 확인하세요.",`
- `zh-Hans.ts`: `"problem.noMatch": "找不到可以演唱的版本。请用“更换版本”挑选一个，或粘贴链接。",` `"problem.matchLookup": "无法查找可演唱的版本。请检查网络连接。",`
- `zh-Hant.ts`: `"problem.noMatch": "找不到可以演唱的版本。請用「更換版本」挑選一個，或貼上連結。",` `"problem.matchLookup": "無法查找可演唱的版本。請檢查網路連線。",`
- `es.ts`: `"problem.noMatch": "No se encontró una versión para cantar. Usa Cambiar versión para elegir una o pega un enlace.",` `"problem.matchLookup": "No se pudo buscar una versión para cantar. Revisa tu conexión.",`

(Task 12 names the menu item "Change match…" / バージョンを変更… / 버전 변경… / 更换版本… / 更換版本… / Cambiar versión…, matching these texts.)

- [ ] **Step 2: YouTube Music's song search — failing test**

Create `crates/kara-core/src/ingest/ytmusic-search.sample.json` (a trimmed answer with made-up content; the real one has more keys):

```json
{
  "contents": {
    "tabbedSearchResultsRenderer": {
      "tabs": [{
        "tabRenderer": {
          "content": {
            "sectionListRenderer": {
              "contents": [{
                "musicShelfRenderer": {
                  "title": { "runs": [{ "text": "Songs" }] },
                  "contents": [
                    {
                      "musicResponsiveListItemRenderer": {
                        "thumbnail": { "musicThumbnailRenderer": { "thumbnail": { "thumbnails": [
                          { "url": "https://lh3.googleusercontent.com/madeup=w60-h60", "width": 60, "height": 60 },
                          { "url": "https://lh3.googleusercontent.com/madeup=w120-h120", "width": 120, "height": 120 }
                        ] } } },
                        "flexColumns": [
                          { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Paper Boats" }] } } },
                          { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [
                            { "text": "Juniper Row", "navigationEndpoint": { "browseEndpoint": { "browseId": "UCmadeup", "browseEndpointContextSupportedConfigs": { "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ARTIST" } } } } },
                            { "text": " • " },
                            { "text": "Harbor Lights", "navigationEndpoint": { "browseEndpoint": { "browseId": "MPREb_madeup", "browseEndpointContextSupportedConfigs": { "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ALBUM" } } } } },
                            { "text": " • " },
                            { "text": "3:21" }
                          ] } } },
                          { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "1.2M plays" }] } } }
                        ],
                        "playlistItemData": { "videoId": "aaaaaaaaaaa" }
                      }
                    },
                    {
                      "musicResponsiveListItemRenderer": {
                        "flexColumns": [
                          { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [{ "text": "Rooftop Static (Official Video)" }] } } },
                          { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": [
                            { "text": "Dani Sato" }, { "text": " • " }, { "text": "238M views" }, { "text": " • " }, { "text": "3:45" }
                          ] } } }
                        ],
                        "playlistItemData": { "videoId": "bbbbbbbbbbb" }
                      }
                    }
                  ]
                }
              }]
            }
          }
        }
      }]
    }
  }
}
```

Create `crates/kara-core/src/ingest/ytmusic.rs` with the doc line from Step 4 and:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs_read_title_artist_album_length_and_picture() {
        let hit = |id: &str, title: &str, channel: &str, ms: i64, thumbnail: Option<&str>| SearchHit {
            url: format!("https://www.youtube.com/watch?v={id}"),
            preview: LinkPreview { title: title.into(), channel: Some(channel.into()), duration_ms: Some(ms), thumbnail: thumbnail.map(Into::into) },
        };
        assert_eq!(
            parse_songs(include_str!("ytmusic-search.sample.json")).unwrap(),
            vec![
                Song { hit: hit("aaaaaaaaaaa", "Paper Boats", "Juniper Row", 201_000, Some("https://lh3.googleusercontent.com/madeup=w120-h120")), album: Some("Harbor Lights".into()) },
                Song { hit: hit("bbbbbbbbbbb", "Rooftop Static (Official Video)", "Dani Sato", 225_000, None), album: None },
            ]
        );
    }

    #[test]
    #[ignore = "network"]
    fn live_isrc_search_answers_fast() {
        let start = std::time::Instant::now();
        let found = songs("GBUM71029604").unwrap();
        println!("{} songs in {:?}", found.len(), start.elapsed());
        assert!(!found.is_empty());
    }
}
```

Add `pub mod ytmusic;` to `ingest/mod.rs`.

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core ingest::ytmusic`
Expected: compile errors — `parse_songs`, `Song` not found.

- [ ] **Step 3: Matching — failing tests**

Create `crates/kara-core/src/matching.rs` with the doc line from Step 5 and:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::preview::LinkPreview;
    use crate::library::{ProviderId, StreamingTrack};
    use crate::problem::problem;
    use std::cell::Cell;

    struct Fake {
        songs: Vec<Song>,
        videos: Vec<SearchHit>,
        songs_fail: bool,
        videos_fail: bool,
        asked: Cell<usize>,
    }

    impl Fake {
        fn new(songs: Vec<Song>, videos: Vec<SearchHit>) -> Self {
            Self { songs, videos, songs_fail: false, videos_fail: false, asked: Cell::new(0) }
        }
    }

    impl Finder for Fake {
        fn songs(&self, _: &str) -> Result<Vec<Song>> {
            self.asked.set(self.asked.get() + 1);
            anyhow::ensure!(!self.songs_fail, "offline");
            Ok(self.songs.clone())
        }
        fn videos(&self, _: &str) -> Result<Vec<SearchHit>> {
            self.asked.set(self.asked.get() + 1);
            anyhow::ensure!(!self.videos_fail, "offline");
            Ok(self.videos.clone())
        }
    }

    fn hit(id: &str, title: &str, channel: &str, secs: i64) -> SearchHit {
        SearchHit { url: format!("https://www.youtube.com/watch?v={id}"), preview: LinkPreview { title: title.into(), channel: Some(channel.into()), duration_ms: Some(secs * 1000), thumbnail: None } }
    }

    fn apple_song(lib: &Library, title: &str, artist: &str, album: Option<&str>, secs: i64, isrc: Option<&str>) -> i64 {
        let t = StreamingTrack {
            provider_ref: format!("ref-{title}"),
            title: title.into(),
            artist: Some(artist.into()),
            album: album.map(Into::into),
            duration_ms: Some(secs * 1000),
            isrc: isrc.map(Into::into),
            album_ref: None,
            artwork: None,
        };
        lib.upsert_streaming_track(ProviderId::Apple, &t).unwrap()
    }

    fn picked(lib: &Library, track: i64) -> Vec<(String, bool)> {
        lib.sources(track).unwrap().into_iter().map(|s| (s.uri.rsplit('=').next().unwrap().to_string(), s.selected)).collect()
    }

    #[test]
    fn the_official_audio_found_by_isrc_is_used_and_other_songs_and_versions_are_refused() {
        let lib = Library::open_in_memory().unwrap();
        let t = apple_song(&lib, "Paper Boats", "Juniper Row", Some("Harbor Lights"), 200, Some("QZZZZ2600010"));
        let finder = Fake::new(
            vec![
                Song { hit: hit("zzzzzzzzzzz", "Guli Lamp", "Someone Else", 301), album: None },
                Song { hit: hit("aaaaaaaaaaa", "Paper Boats", "Juniper Row", 201), album: Some("Harbor Lights".into()) },
            ],
            vec![
                hit("bbbbbbbbbbb", "Juniper Row - Paper Boats (Official Video)", "Harbor Records", 262),
                hit("ccccccccccc", "Paper Boats (Live at the Pier)", "Juniper Row", 240),
                hit("ddddddddddd", "Paper Boats - Karaoke Version", "Sing Along Made Up", 200),
                hit("eeeeeeeeeee", "Paper Boats", "Other Band", 200),
                hit("fffffffffff", "Paper Boats (Extended)", "Juniper Row", 420),
            ],
        );
        ensure(&lib, t, &finder).unwrap();
        let expected = [("aaaaaaaaaaa", true), ("bbbbbbbbbbb", false), ("zzzzzzzzzzz", false), ("ccccccccccc", false), ("ddddddddddd", false), ("eeeeeeeeeee", false), ("fffffffffff", false)];
        assert_eq!(picked(&lib, t), expected.map(|(u, s)| (u.to_string(), s)));
    }

    #[test]
    fn a_songs_own_live_version_accepts_live_uploads_and_titles_in_brackets_match() {
        let lib = Library::open_in_memory().unwrap();
        let live = apple_song(&lib, "Harbor Lights (Live)", "Juniper Row", None, 260, None);
        ensure(&lib, live, &Fake::new(vec![], vec![hit("ggggggggggg", "Harbor Lights (Live)", "Juniper Row", 262)])).unwrap();
        assert_eq!(picked(&lib, live), [("ggggggggggg".to_string(), true)]);
        let jp = apple_song(&lib, "夜の舟", "星野ミナ", None, 243, None);
        ensure(&lib, jp, &Fake::new(vec![], vec![hit("hhhhhhhhhhh", "星野ミナ「夜の舟」Official Music Video", "星野ミナ Official", 250)])).unwrap();
        assert_eq!(picked(&lib, jp), [("hhhhhhhhhhh".to_string(), true)]);
    }

    #[test]
    fn a_song_nothing_fits_keeps_the_uploads_for_change_match_and_is_not_searched_again() {
        let lib = Library::open_in_memory().unwrap();
        let t = apple_song(&lib, "Paper Boats", "Juniper Row", None, 200, None);
        let finder = Fake::new(vec![], vec![hit("eeeeeeeeeee", "Paper Boats", "Other Band", 200)]);
        assert_eq!(problem(&ensure(&lib, t, &finder).unwrap_err()), Some(Problem::NoMatch));
        assert_eq!(problem(&ensure(&lib, t, &finder).unwrap_err()), Some(Problem::NoMatch));
        assert_eq!((finder.asked.get(), picked(&lib, t)), (1, vec![("eeeeeeeeeee".to_string(), false)]));
    }

    #[test]
    fn a_search_that_failed_without_a_fitting_upload_saves_nothing_so_the_next_play_searches_again() {
        let lib = Library::open_in_memory().unwrap();
        let t = apple_song(&lib, "Paper Boats", "Juniper Row", None, 200, Some("QZZZZ2600010"));
        let finder = Fake { songs_fail: true, ..Fake::new(vec![], vec![hit("eeeeeeeeeee", "Paper Boats", "Other Band", 200)]) };
        assert_eq!(problem(&ensure(&lib, t, &finder).unwrap_err()), Some(Problem::MatchLookup));
        assert!(lib.sources(t).unwrap().is_empty());
        let offline = Fake { songs_fail: true, videos_fail: true, ..Fake::new(vec![], vec![]) };
        assert_eq!(problem(&ensure(&lib, t, &offline).unwrap_err()), Some(Problem::MatchLookup));
        ensure(&lib, t, &finder).unwrap_err();
        assert_eq!((finder.asked.get(), offline.asked.get()), (4, 2));
    }
}
```

Add `pub mod matching;` to `lib.rs`.

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core matching`
Expected: compile errors — `Finder`, `ensure`, `Song` not found.

- [ ] **Step 4: Write the YouTube Music search**

In `ingest/youtube.rs` make `clock_ms` `pub(crate)`. Top of `ingest/ytmusic.rs`:

```rust
//! YouTube Music's song search, called directly: official audio uploads with their artist, album and length.

use super::preview::{LinkPreview, SearchHit};
use super::youtube::clock_ms;
use anyhow::Result;
use serde_json::{json, Value};
use std::time::Duration;

/// The web client music.youtube.com identifies as.
const CLIENT_VERSION: &str = "1.20260921.01.00";
/// The "Songs" filter.
const SONGS_ONLY: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA==";

/// A song found on YouTube Music, with its album when it names one.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub hit: SearchHit,
    pub album: Option<String>,
}

/// YouTube Music's songs for `query` (words or an ISRC).
pub fn songs(query: &str) -> Result<Vec<Song>> {
    let body = json!({ "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": CLIENT_VERSION, "hl": "en" } }, "query": query, "params": SONGS_ONLY });
    let text = reqwest::blocking::Client::new()
        .post("https://music.youtube.com/youtubei/v1/search?prettyPrint=false")
        .timeout(Duration::from_secs(5))
        .header("Origin", "https://music.youtube.com")
        .json(&body)
        .send()?
        .error_for_status()?
        .text()?;
    parse_songs(&text)
}

/// The rows of a YouTube Music search answer: the title, then "artist • album • 3:25" (a video has views instead of an album).
pub fn parse_songs(json: &str) -> Result<Vec<Song>> {
    let answer: Value = serde_json::from_str(json)?;
    let mut rows = Vec::new();
    collect_rows(&answer, &mut rows);
    Ok(rows.into_iter().filter_map(song).collect())
}

fn collect_rows<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => match m.get("musicResponsiveListItemRenderer") {
            Some(row) => out.push(row),
            None => m.values().for_each(|x| collect_rows(x, out)),
        },
        Value::Array(a) => a.iter().for_each(|x| collect_rows(x, out)),
        _ => {}
    }
}

fn runs(column: &Value) -> &[Value] {
    column["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn song(row: &Value) -> Option<Song> {
    let id = row["playlistItemData"]["videoId"].as_str()?;
    let joined = |runs: &[Value]| runs.iter().filter_map(|r| r["text"].as_str()).collect::<String>();
    let title = joined(runs(&row["flexColumns"][0]));
    let details = runs(&row["flexColumns"][1]);
    let line = joined(details);
    let is_album = |r: &Value| r["navigationEndpoint"]["browseEndpoint"]["browseEndpointContextSupportedConfigs"]["browseEndpointContextMusicConfig"]["pageType"] == "MUSIC_PAGE_TYPE_ALBUM";
    let thumbnails = row["thumbnail"]["musicThumbnailRenderer"]["thumbnail"]["thumbnails"].as_array();
    let preview = LinkPreview {
        title,
        channel: line.split(" • ").next().filter(|s| !s.is_empty()).map(String::from),
        duration_ms: line.rsplit(" • ").next().and_then(clock_ms),
        thumbnail: thumbnails.and_then(|t| t.last()).and_then(|t| t["url"].as_str()).map(String::from),
    };
    let album = details.iter().find(|r| is_album(r)).and_then(|r| r["text"].as_str()).map(String::from);
    (!preview.title.is_empty()).then(|| Song { hit: SearchHit { url: format!("https://www.youtube.com/watch?v={id}"), preview }, album })
}
```

Move Phase 2's `adding::find_on_youtube` (read it at HEAD) into `ingest/mod.rs` unchanged — at the time of writing Phase 2's plan gives it as:

```rust
/// The top YouTube videos for `query`: YouTube's own web search, or yt-dlp's when that fails.
pub fn find_on_youtube(store: &Store, query: &str) -> Result<Vec<preview::SearchHit>> {
    youtube::search(query).or_else(|_| ytdlp::ensure(&store.bin_dir()).and_then(|bin| preview::search(&bin, query)))
}
```

Delete it from `adding.rs` and point its callers (the `youtube_search` command, Phase 2's phone search) at `ingest::find_on_youtube`; drop the imports that become unused. One function, used by the Mac, the phones and matching.

- [ ] **Step 5: Write the matching**

In `lyrics.rs` make `key`, `similar`, `singer_likeness` and `words` `pub(crate)`. Top of `matching.rs`:

```rust
//! Finding an upload to sing a streaming song from: its official audio on YouTube Music by ISRC, then YouTube videos
//! for "artist title", scored on title, singer, album and length, with other versions refused.

use crate::ingest::preview::SearchHit;
use crate::ingest::find_on_youtube;
use crate::ingest::ytmusic::{self, Song};
use crate::library::{Library, Track};
use crate::lyrics::{clean_artist, clean_title, key, similar, singer_likeness, words};
use crate::problem::Problem;
use anyhow::{Context, Result};
use std::collections::HashSet;
use crate::store::Store;

/// How far an official audio upload's length may be from the song's.
const SONG_OFF_S: f64 = 15.0;
/// How far a video's length may be (music videos add intros and outros).
const VIDEO_OFF_S: f64 = 90.0;
/// How many uploads a song keeps to choose from.
const KEEP: usize = 8;
/// Words that mark another version of a song, refused unless the song's own title has them.
const OTHER_VERSIONS: &[&str] = &[
    "live", "remix", "cover", "instrumental", "inst", "karaoke", "off vocal", "sped up", "slowed", "nightcore", "8d", "acoustic",
    "a cappella", "acapella", "piano", "reaction", "tutorial", "伴奏", "カラオケ", "翻唱", "现场", "現場", "ライブ", "라이브",
];

/// Where uploads are found.
pub trait Finder {
    /// Official audio on YouTube Music.
    fn songs(&self, query: &str) -> Result<Vec<Song>>;
    /// YouTube videos.
    fn videos(&self, query: &str) -> Result<Vec<SearchHit>>;
}

/// YouTube Music and YouTube (with the app's yt-dlp in `store` when YouTube's own search fails).
pub struct YouTube {
    pub store: Store,
}

impl Finder for YouTube {
    fn songs(&self, query: &str) -> Result<Vec<Song>> {
        ytmusic::songs(query)
    }
    fn videos(&self, query: &str) -> Result<Vec<SearchHit>> {
        find_on_youtube(&self.store, query)
    }
}

/// Whether `text` holds `marker` as whole words (anywhere, for CJK markers).
fn has(text: &str, marker: &str) -> bool {
    if marker.is_ascii() {
        format!(" {} ", words(text).join(" ")).contains(&format!(" {marker} "))
    } else {
        text.contains(marker)
    }
}

/// How well an upload fits `track` (higher is better), or None when it is another song, another singer's or another version.
pub fn score(track: &Track, hit: &SearchHit, album: Option<&str>, official: bool) -> Option<f64> {
    let (theirs, named) = clean_title(&hit.preview.title, hit.preview.channel.as_deref());
    let (ours, _) = clean_title(&track.title, track.artist.as_deref());
    let same_title = key(&theirs) == key(&ours);
    if !(same_title || similar(&ours, &theirs) && similar(&theirs, &ours)) {
        return None;
    }
    if OTHER_VERSIONS.iter().any(|v| has(&hit.preview.title, v) && !has(&track.title, v)) {
        return None;
    }
    let their_singers: Vec<String> = named.iter().map(String::as_str).chain(hit.preview.channel.as_deref()).flat_map(clean_artist).collect();
    let our_singers: Vec<String> = track.artist.iter().flat_map(|a| clean_artist(a)).collect();
    let singer = their_singers.iter().flat_map(|a| our_singers.iter().map(move |b| singer_likeness(a, b))).max().unwrap_or(0);
    if singer == 0 && !our_singers.is_empty() {
        return None;
    }
    let off_s = match (hit.preview.duration_ms, track.duration_ms) {
        (Some(a), Some(b)) => (a - b).abs() as f64 / 1000.0,
        _ => 0.0,
    };
    if off_s > if official { SONG_OFF_S } else { VIDEO_OFF_S } {
        return None;
    }
    let same_album = album.zip(track.album.as_deref()).is_some_and(|(a, b)| key(a) == key(b));
    let title_points = if same_title { 2.0 } else { 1.0 };
    let album_points = if same_album { 0.5 } else { 0.0 };
    let official_points = if official { 1.0 } else { 0.0 };
    Some(title_points + f64::from(singer) + album_points + official_points - off_s / 30.0)
}

/// Uploads for `track`, each with its score (None: refused): official audio for its ISRC, then videos for "artist title";
/// accepted ones best first, then refused ones as found, at most `KEEP`. Fails when a search failed and nothing found fits,
/// so a worse list is never kept for good.
pub fn find(track: &Track, isrc: Option<&str>, finder: &dyn Finder) -> Result<Vec<(Option<f64>, SearchHit)>> {
    let mut found = Vec::new();
    let mut failed = None;
    if let Some(isrc) = isrc {
        match finder.songs(isrc) {
            Ok(songs) => found.extend(songs.into_iter().map(|s| (score(track, &s.hit, s.album.as_deref(), true), s.hit))),
            Err(e) => failed = Some(e),
        }
    }
    let query = format!("{} {}", track.artist.as_deref().unwrap_or_default(), track.title);
    match finder.videos(query.trim()) {
        Ok(videos) => found.extend(videos.into_iter().map(|v| (score(track, &v, None, false), v))),
        Err(e) => failed = failed.or(Some(e)),
    }
    if let Some(e) = failed.filter(|_| !found.iter().any(|(score, _)| score.is_some())) {
        return Err(e.context(Problem::MatchLookup));
    }
    let mut seen = HashSet::new();
    found.retain(|(_, hit)| seen.insert(hit.url.clone()));
    found.sort_by(|a, b| b.0.unwrap_or(f64::MIN).total_cmp(&a.0.unwrap_or(f64::MIN)));
    found.truncate(KEEP);
    Ok(found)
}

/// Saves the uploads found for a streaming song and puts the best accepted one in use.
pub fn search_once(lib: &Library, track_id: i64, finder: &dyn Finder) -> Result<()> {
    let track = lib.track(track_id)?;
    let mut best = None;
    for (score, hit) in find(&track, lib.track_isrc(track_id)?.as_deref(), finder)? {
        let id = lib.add_match(track_id, &hit)?;
        if best.is_none() && score.is_some() {
            best = Some(id);
        }
    }
    match best {
        Some(id) => lib.select_source(track_id, id),
        None => Ok(()),
    }
}

/// Makes sure a streaming song has an upload in use, searching only when it has none yet; fails when none fits.
pub fn ensure(lib: &Library, track_id: i64, finder: &dyn Finder) -> Result<()> {
    if lib.selected_source(track_id)?.is_some() {
        return Ok(());
    }
    if lib.sources(track_id)?.is_empty() {
        search_once(lib, track_id, finder)?;
    }
    lib.selected_source(track_id)?.map(drop).context(Problem::NoMatch)
}
```

- [ ] **Step 6: Prepare a streaming song**

In `jobs.rs` `prepare_inner`, right after `let track = lib.track(track_id).context(Problem::SongGone)?;`:

```rust
    if track.provider != ProviderId::Local {
        crate::matching::ensure(lib, track_id, &crate::matching::YouTube { store: ctx.store.clone() })?;
    }
```

(import `ProviderId` from `crate::library`).

Read `rename` in `jobs.rs` at HEAD (Task 33). A streaming service's names are the song's names: change its filter to `swapped.filter(|_| !t.info_edited && t.provider == ProviderId::Local)` and its doc's last clause to "a song whose info the user edited, or a streaming service's song, keeps it." Add next to `a_song_whose_info_the_user_edited_keeps_it_when_lyrics_are_found_swapped` in `jobs.rs` tests:

```rust
    #[test]
    fn a_streaming_song_keeps_its_services_names_when_lyrics_are_found_swapped() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let song = crate::library::StreamingTrack {
            provider_ref: "900002".into(),
            title: "Made Up Album".into(),
            artist: Some("City of Stars".into()),
            album: None,
            duration_ms: Some(150_000),
            isrc: None,
            album_ref: None,
            artwork: None,
        };
        let t = lib.upsert_streaming_track(crate::library::ProviderId::Apple, &song).unwrap();
        let mut events = Vec::new();
        assert!(find_lyrics_again(&c, &lib, &lyrics::Candidates(vec![("City of Stars", "Ryan Gosling & Emma Stone", 150.0)]), t, &mut |e| events.push(e)).unwrap());
        let track = lib.track(t).unwrap();
        assert_eq!((track.title.as_str(), track.artist.as_deref(), events), ("Made Up Album", Some("City of Stars"), vec![Event::Lyrics { track_id: t }]));
    }
```

(It fails before the filter change: the song would be renamed and filed under Local › Imported.)

- [ ] **Step 7: Run the tests and checks**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core ingest::ytmusic && cargo test -p kara-core matching && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cd app && npm run check && npm run check:i18n`
Expected: all pass. If a scoring expectation fails, print `score(...)` for that hit in the test (temporarily) and fix the rule, not the expectation — each expectation is a behavior the spec asks for. Optionally run `cargo test -p kara-core live_isrc_search -- --ignored --nocapture` (network) and report its time.

- [ ] **Step 8: Commit**

```bash
git add crates/kara-core app/src-tauri/src app/src/lib/api.ts app/src/lib/i18n
git commit -m "feat(core): match streaming songs to YouTube uploads, official audio by ISRC first"
```

---

### Task 7: The app — login window, Sources status, refresh, disconnect

**Files:**
- Create: `app/src-tauri/src/sources.rs`
- Modify: `app/src-tauri/src/lib.rs` (`mod sources;`, manage `sources::Apple`, `sources::start`, commands), `app/src-tauri/src/player.rs` (`Player::track_ids`)
- Use unchanged: Phase 2's `app/scripts/isolated-check.sh` and `window-id.swift` (read them first)

**Interfaces:**
- Consumes: `apple::{Client, Tokens, keychain, parse, sync}` (Tasks 3–5), `Library::{account, connect_account, set_signed_out}` (Task 1), `AppState`, `state::{AppError, Plain}`.
- Produces:
  - Tauri state `sources::Apple` (managed in `setup`); `sources::client(app: &AppHandle) -> anyhow::Result<Option<Arc<Client>>>` (None unless Apple Music is connected and not signed out; a Keychain read that fails counts as signed out); `sources::refresh(app: &AppHandle)`; `sources::start(app: &AppHandle)`.
  - Commands: `apple_status() -> AppleStatus`, `apple_connect()`, `apple_refresh()`, `apple_disconnect()`.
  - Event `"sources"` with `AppleStatus { account: Option<Account>, connecting: bool, refreshing: bool, problem: Option<Problem>, revision: u32 }` (camelCase); `revision` goes up whenever Apple Music content in the library changed.
  - `Player::track_ids(&self) -> Vec<i64>` (every song in the queue).
  - Debug-only knob `KARA_APPLE_LOGIN_CHECK=1`: at launch, opens the login window, and once the page's player has loaded prints `apple login check: the page is ready` to stderr and closes the window (no token is printed).

- [ ] **Step 1: Write the failing test**

Create `app/src-tauri/src/sources.rs` with the doc line from Step 3 and:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_page_gives_the_developer_token_first_and_the_users_once_signed_in() {
        assert_eq!(parse_login(""), None);
        assert_eq!(parse_login("null"), None);
        assert_eq!(parse_login(r#"{"developer":"","user":""}"#), None);
        assert_eq!(parse_login(r#"{"developer":"d","user":""}"#), Some(("d".into(), None)));
        assert_eq!(parse_login(r#"{"developer":"d","user":"u"}"#), Some(("d".into(), Some("u".into()))));
    }
}
```

Add `mod sources;` to `lib.rs`.

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app sources`
Expected: compile error — `parse_login` not found.

- [ ] **Step 2: `Player::track_ids`**

Read `player.rs` at HEAD first (Phase 2 gave `Entry` a `by` field). In `impl Player`:

```rust
    /// Every song in the queue, played or not.
    pub fn track_ids(&self) -> Vec<i64> {
        self.entries.iter().map(|e| e.track_id).collect()
    }
```

- [ ] **Step 3: Write `sources.rs`**

```rust
//! Apple Music in the app: the login window, the login kept on this Mac, refreshing the library, and what Settings › Sources shows.

use crate::state::{AppError, AppState, Plain};
use anyhow::Context;
use kara_core::apple::{keychain, parse, sync, Client, Tokens};
use kara_core::library::{Account, Library, ProviderId};
use kara_core::problem::{problem, Problem};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const LOGIN_WINDOW: &str = "apple-login";
const LOGIN_PAGE: &str = "https://music.apple.com/login";
/// Reads the web player's tokens from the login page; the user's is empty until they have signed in.
const READ_LOGIN: &str =
    "(() => { try { const m = MusicKit.getInstance(); return { developer: m.developerToken || '', user: m.musicUserToken || '' }; } catch { return null; } })()";
/// How old the last refresh may be before starting the app refreshes again.
const REFRESH_AFTER_MS: i64 = 6 * 3600 * 1000;

/// Apple Music while the app runs.
#[derive(Default)]
pub struct Apple {
    client: Mutex<Option<Arc<Client>>>,
    connecting: AtomicBool,
    refreshing: AtomicBool,
    problem: Mutex<Option<Problem>>,
    revision: AtomicU32,
}

/// What Settings › Sources and the provider switcher show.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleStatus {
    account: Option<Account>,
    connecting: bool,
    refreshing: bool,
    /// Why the last refresh failed.
    problem: Option<Problem>,
    /// Goes up whenever Apple Music songs or collections in the library changed.
    revision: u32,
}

fn apple(app: &AppHandle) -> State<'_, Apple> {
    app.state::<Apple>()
}

fn library(app: &AppHandle) -> anyhow::Result<State<'_, AppState>> {
    app.try_state::<AppState>().context(Problem::LibraryOpen)
}

fn status(app: &AppHandle) -> anyhow::Result<AppleStatus> {
    let a = apple(app);
    let account = library(app)?.lib.lock().unwrap().account(ProviderId::Apple)?;
    let problem = *a.problem.lock().unwrap();
    Ok(AppleStatus { account, connecting: a.connecting.load(SeqCst), refreshing: a.refreshing.load(SeqCst), problem, revision: a.revision.load(SeqCst) })
}

/// Tells the app Apple Music's status changed.
fn changed(app: &AppHandle) {
    if let Ok(s) = status(app) {
        let _ = app.emit("sources", s);
    }
}

/// The Apple Music client, from the login kept on this Mac; None unless Apple Music is connected and signed in.
pub fn client(app: &AppHandle) -> anyhow::Result<Option<Arc<Client>>> {
    let signed_in = library(app)?.lib.lock().unwrap().account(ProviderId::Apple)?.is_some_and(|a| !a.signed_out);
    if !signed_in {
        return Ok(None);
    }
    let state = apple(app);
    let mut client = state.client.lock().unwrap();
    if client.is_none() {
        *client = keychain::load().context(Problem::AppleSignedOut)?.map(|t| Arc::new(Client::new(t)));
    }
    Ok(client.clone())
}

#[tauri::command]
pub fn apple_status(app: AppHandle) -> Result<AppleStatus, AppError> {
    status(&app).plain()
}

/// Opens the Apple Music login window, or brings it forward.
#[tauri::command]
pub fn apple_connect(app: AppHandle) -> Result<(), AppError> {
    open_login(&app).plain()
}

#[tauri::command]
pub fn apple_refresh(app: AppHandle) {
    refresh(&app);
}

/// Forgets the Apple Music login and takes its collections out of the library; songs prepared, queued or in your own playlists stay.
#[tauri::command]
pub fn apple_disconnect(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    keychain::delete().plain()?;
    *apple(&app).client.lock().unwrap() = None;
    let keep = state.player.lock().unwrap().track_ids();
    sync::disconnect(&state.lib.lock().unwrap(), &keep).plain()?;
    apple(&app).revision.fetch_add(1, SeqCst);
    changed(&app);
    Ok(())
}

fn open_login(app: &AppHandle) -> anyhow::Result<()> {
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW) {
        return Ok(w.set_focus()?);
    }
    let window = WebviewWindowBuilder::new(app, LOGIN_WINDOW, WebviewUrl::External(LOGIN_PAGE.parse()?))
        .title("Apple Music")
        .inner_size(500.0, 720.0)
        .incognito(true)
        .on_new_window(|_, _| NewWindowResponse::Allow)
        .build()?;
    apple(app).connecting.store(true, SeqCst);
    changed(app);
    let app = app.clone();
    std::thread::spawn(move || {
        watch_login(&app, &window);
        apple(&app).connecting.store(false, SeqCst);
        changed(&app);
    });
    Ok(())
}

/// The developer token and, once signed in, the user token from the login page's answer.
fn parse_login(json: &str) -> Option<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let text = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(String::from);
    Some((text("developer")?, text("user")))
}

/// The user token from the web player's cookie, for when the page doesn't hand it out.
fn login_cookie(window: &WebviewWindow) -> Option<String> {
    let cookies = window.cookies_for_url("https://music.apple.com".parse().ok()?).ok()?;
    cookies.into_iter().find(|c| c.name() == "media-user-token").map(|c| c.value().to_string()).filter(|v| !v.is_empty())
}

/// Asks the login page for the web player's tokens every second until the user has signed in (then connects and
/// closes the window) or closes it. A login that fails to connect isn't tried again until the page has another one.
/// With KARA_APPLE_LOGIN_CHECK it only reports that the page is ready.
fn watch_login(app: &AppHandle, window: &WebviewWindow) {
    let check = cfg!(debug_assertions) && std::env::var_os("KARA_APPLE_LOGIN_CHECK").is_some();
    let (tx, rx) = mpsc::channel();
    let mut tried: Option<String> = None;
    while app.get_webview_window(LOGIN_WINDOW).is_some() {
        std::thread::sleep(Duration::from_secs(1));
        let tx = tx.clone();
        if window.eval_with_callback(READ_LOGIN, move |json| drop(tx.send(json))).is_err() {
            break;
        }
        let Ok(json) = rx.recv_timeout(Duration::from_secs(5)) else { continue };
        let Some((developer, user)) = parse_login(&json) else { continue };
        if check {
            eprintln!("apple login check: the page is ready");
            let _ = window.close();
            break;
        }
        let Some(user) = user.or_else(|| login_cookie(window)) else { continue };
        if tried.as_ref() == Some(&user) {
            continue;
        }
        tried = Some(user.clone());
        match connect(app, developer, user) {
            Ok(()) => {
                let _ = window.close();
                break;
            }
            Err(e) => {
                *apple(app).problem.lock().unwrap() = problem(&e);
                changed(app);
            }
        }
    }
}

/// Checks the login with Apple Music, keeps it on this Mac, records the account and starts the first refresh.
fn connect(app: &AppHandle, developer: String, user: String) -> anyhow::Result<()> {
    let probe = Client::new(Tokens { developer: developer.clone(), user: user.clone(), storefront: String::new() });
    let storefront = parse::storefront(&probe.get("/v1/me/storefront")?).context(Problem::AppleChanged)?;
    let name = probe.find("/v1/me/social-profile").ok().flatten().as_ref().and_then(parse::display_name);
    let tokens = Tokens { developer, user, storefront };
    keychain::save(&tokens).context(Problem::LoginSave)?;
    *apple(app).client.lock().unwrap() = Some(Arc::new(Client::new(tokens)));
    library(app)?.lib.lock().unwrap().connect_account(ProviderId::Apple, name.as_deref())?;
    *apple(app).problem.lock().unwrap() = None;
    refresh(app);
    Ok(())
}

/// Refreshes the Apple Music library in the background, one refresh at a time.
pub fn refresh(app: &AppHandle) {
    if apple(app).refreshing.swap(true, SeqCst) {
        return;
    }
    changed(app);
    let app = app.clone();
    std::thread::spawn(move || {
        let result = run_refresh(&app);
        let a = apple(&app);
        *a.problem.lock().unwrap() = result.as_ref().err().and_then(problem);
        a.refreshing.store(false, SeqCst);
        a.revision.fetch_add(1, SeqCst);
        changed(&app);
    });
}

/// Syncs with Apple Music on its own library connection; a refused or missing login marks the account signed out,
/// and a disconnect that happened meanwhile is finished.
fn run_refresh(app: &AppHandle) -> anyhow::Result<()> {
    let state = library(app)?;
    let lib = Library::open(&state.store.db_path())?;
    let keep = state.player.lock().unwrap().track_ids();
    let result = match client(app) {
        Ok(Some(client)) => sync::sync(&lib, &client, &keep, &mut || {
            apple(app).revision.fetch_add(1, SeqCst);
            changed(app);
        }),
        Ok(None) => Err(Problem::AppleSignedOut.into()),
        Err(e) => Err(e),
    };
    if result.as_ref().err().and_then(problem) == Some(Problem::AppleSignedOut) {
        lib.set_signed_out(ProviderId::Apple)?;
    }
    if lib.account(ProviderId::Apple)?.is_none() {
        sync::disconnect(&lib, &keep)?;
    }
    result
}

/// At launch: refreshes when Apple Music is connected and the last refresh is older than six hours.
/// With KARA_APPLE_LOGIN_CHECK (debug builds) it opens the login window instead.
pub fn start(app: &AppHandle) {
    if cfg!(debug_assertions) && std::env::var_os("KARA_APPLE_LOGIN_CHECK").is_some() {
        let _ = open_login(app);
        return;
    }
    let Ok(state) = library(app) else { return };
    let account = state.lib.lock().unwrap().account(ProviderId::Apple).ok().flatten();
    if account.is_some_and(|a| !a.signed_out && a.refreshed_at.is_none_or(|at| kara_core::now_ms() - at > REFRESH_AFTER_MS)) {
        refresh(app);
    }
}
```

These Tauri 2.12 items were checked in `~/.cargo/registry/src/*/tauri-2.12.0/src/webview/`: `tauri::webview::NewWindowResponse`, `WebviewWindowBuilder::{incognito, on_new_window}`, `WebviewWindow::{eval_with_callback, cookies_for_url}` (the cookie has `name()` / `value()`). Keep `app/src-tauri/capabilities/default.json` unchanged (the login window must get no IPC).

- [ ] **Step 4: Wire it up**

Read `lib.rs` at HEAD first (Phase 2 added `phones` state and commands). In `setup`, before `let _ = engine::open_library(app.handle());` add `app.manage(sources::Apple::default());`, and after it add `sources::start(app.handle());`. Add to `generate_handler!`:

```rust
            sources::apple_status,
            sources::apple_connect,
            sources::apple_refresh,
            sources::apple_disconnect,
```

- [ ] **Step 5: Run the unit test and checks**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app sources && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 6: Run the login window in the isolated check**

Run (Bash timeout 10 minutes; Phase 2's script, used as it is):
```bash
cd app && KARA_APPLE_LOGIN_CHECK=1 zsh scripts/isolated-check.sh ../.superpowers/sdd/2026-09-27-phase3-apple-music/shots/task-07.png "sleep 30" \
  && grep -q 'apple login check: the page is ready' ../.superpowers/sdd/2026-09-27-phase3-apple-music/shots/task-07.log
```
Expected: `OK. Picture: …` and the grep succeeds — the incognito login window loaded music.apple.com and its player answered `MusicKit.getInstance()` before anyone logged in. Nobody logs in and Connect/Disconnect are never pressed here (the Keychain item is shared with the user's real login).

If the grep fails, read `task-07.log` (never paste token-like strings into the report) and check whether the page's `MusicKit` exists before sign-in. If it doesn't, change `watch_login` to wait for the `media-user-token` cookie (`login_cookie`) first and only then read `developerToken` from the page (the player is loaded after sign-in), make the knob report readiness once the page has finished loading instead, and say so in the report.

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/src/sources.rs app/src-tauri/src/lib.rs app/src-tauri/src/player.rs
git commit -m "feat(app): Apple Music login window, kept login, background refresh and disconnect"
```

---

### Task 8: The app — collections by source, artists merged in All, Imported only for Local; web artwork on the Mac and on phones

**Files:**
- Modify: `app/src-tauri/src/library.rs` (`CollectionCard.providers`, `card`, `cards`, `page`, `list_collections`, `open_collection`, tests)
- Modify: `app/src-tauri/src/adding.rs` (`search_input`)
- Modify: `app/src/lib/api.ts` (the changed commands' callers keep working)
- Modify (read them first — Phase 2's code): `app/src/lib/art.ts` (Phase 2's `pictureUrl`), `app/src/lib/components/KaraokeBackground.svelte`, `app/src-tauri/src/phones/mod.rs` (`encode`'s `art_links`)

**Interfaces:**
- Consumes: Task 1's library methods (tests), `Library::collections(provider, kind)`.
- Produces:
  - `CollectionCard` gains `providers: Vec<ProviderId>` (where its songs come from; its own provider when empty) → JSON `providers`.
  - `library::cards(lib: &Library, kind: CollectionKind, provider: Option<ProviderId>) -> anyhow::Result<Vec<CollectionCard>>` — `None` = All, where artists with the same name (ignoring case) are one card with every source's songs.
  - `library::page(lib: &Library, id: i64, all: bool) -> anyhow::Result<CollectionPage>`
  - Commands: `list_collections(kind: String, provider: Option<String>)`, `open_collection(id: i64, all: bool)`.
  - `adding::search_input` shows "Imported" (the user's word) only for Local's app-made playlist.
  - `api.ts`: `CollectionCard.providers: Track["provider"][]` and `CollectionCard.artworkPath: string | null`; `listCollections(kind: Kind, provider: "local" | "apple" | null = null)`; `openCollection(id: number, all = true)` — existing callers keep today's behavior.
  - `art.ts`: one exported helper `artworkUrl(path: string): string` replacing Phase 2's `pictureUrl` — the phone server's `/art/` links and web addresses (`http(s)://`, Apple's artwork) as they are, files on this Mac through `convertFileSrc`. `artBackground` and `KaraokeBackground` use it.
  - `phones::encode` rewrites only file paths to `/art/<file name>`; web artwork reaches phones as it is.

- [ ] **Step 1: Write the failing tests**

In `library.rs` tests, update the existing test's calls to `cards(&lib, CollectionKind::Playlist, None)`, `cards(&lib, CollectionKind::Album, None)` and `page(&lib, album, false)`, and add:

```rust
    #[test]
    fn each_source_lists_its_own_collections_and_all_shows_same_named_artists_once_with_every_sources_songs() {
        use kara_core::library::{StreamingCollection, StreamingTrack};
        let lib = Library::open_in_memory().unwrap();
        let local = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: "Paper Boats", artist: Some("Juniper Row"), album: None, duration_ms: None }).unwrap();
        kara_core::ingest::link_collections(&lib, local, Some("Juniper Row"), None).unwrap();
        let song = StreamingTrack { provider_ref: "900002".into(), title: "Neon Tidewater".into(), artist: Some("Juniper Row".into()), album: None, duration_ms: None, isrc: None, album_ref: None, artwork: None };
        let apple = lib.upsert_streaming_track(ProviderId::Apple, &song).unwrap();
        let artist = lib.upsert_collection(ProviderId::Apple, CollectionKind::Artist, "artist:juniper row", "Juniper Row", None).unwrap();
        lib.set_collection_tracks(artist, &[apple], None).unwrap();
        let drive = StreamingCollection { kind: CollectionKind::Playlist, provider_ref: "p.1".into(), name: "Late Night Drive".into(), subtitle: None, artwork: None, catalog_ref: None, version: None };
        let pl = lib.upsert_streaming_collection(ProviderId::Apple, &drive).unwrap();
        lib.set_collection_tracks(pl, &[apple], None).unwrap();

        let names = |kind, provider| cards(&lib, kind, provider).unwrap().into_iter().map(|c| c.row.name).collect::<Vec<_>>();
        assert_eq!(names(CollectionKind::Playlist, Some(ProviderId::Apple)), ["Late Night Drive"]);
        assert_eq!(names(CollectionKind::Playlist, Some(ProviderId::Local)), ["Imported"]);
        assert_eq!(names(CollectionKind::Playlist, None), ["Imported", "Late Night Drive"]);
        let all = cards(&lib, CollectionKind::Artist, None).unwrap();
        assert_eq!((all.len(), all[0].count), (1, 2));
        assert!(all[0].providers.contains(&ProviderId::Local) && all[0].providers.contains(&ProviderId::Apple));
        assert_eq!(page(&lib, all[0].row.id, true).unwrap().tracks.len(), 2);
        assert_eq!(page(&lib, artist, false).unwrap().tracks.len(), 1);
        assert_eq!(cards(&lib, CollectionKind::Artist, Some(ProviderId::Apple)).unwrap()[0].count, 1);
    }
```

In `adding.rs` tests add:

```rust
    #[test]
    fn an_apple_playlist_is_found_by_its_own_name_not_as_imported() {
        use kara_core::library::{StreamingCollection, StreamingTrack};
        let lib = Library::open_in_memory().unwrap();
        let local = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: "Paper Boats", artist: None, album: None, duration_ms: None }).unwrap();
        kara_core::ingest::link_collections(&lib, local, None, None).unwrap();
        let song = StreamingTrack { provider_ref: "900002".into(), title: "Neon Tidewater".into(), artist: None, album: None, duration_ms: None, isrc: None, album_ref: None, artwork: None };
        let apple = lib.upsert_streaming_track(ProviderId::Apple, &song).unwrap();
        let drive = StreamingCollection { kind: CollectionKind::Playlist, provider_ref: "p.1".into(), name: "Late Night Drive".into(), subtitle: None, artwork: None, catalog_ref: None, version: None };
        let pl = lib.upsert_streaming_collection(ProviderId::Apple, &drive).unwrap();
        lib.set_collection_tracks(pl, &[apple], None).unwrap();
        let found = |input: &str| match search_input(&lib, input, "Importadas").unwrap() {
            SearchOutcome::Text { collections, .. } => collections.into_iter().map(|c| c.row.name).collect::<Vec<_>>(),
            _ => panic!("not a text search"),
        };
        assert_eq!(found("importadas"), ["Imported"]);
        assert_eq!(found("late night"), ["Late Night Drive"]);
    }
```

- [ ] **Step 2: Run to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app library && cargo test -p kara-app adding`
Expected: compile errors (`cards` takes 2 arguments, `providers` missing); after fixing signatures, `an_apple_playlist_is_found…` fails because "Late Night Drive" matches "importadas".

- [ ] **Step 3: Implement**

In `library.rs` (import `ProviderId`):

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionCard {
    #[serde(flatten)]
    pub row: CollectionRow,
    pub count: usize,
    pub covers: Vec<Track>,
    /// Where its songs come from; its own provider when it has none.
    pub providers: Vec<ProviderId>,
}

/// A collection with its song count, its first four songs for the cover and where its songs come from.
fn card(row: CollectionRow, tracks: &[Track]) -> CollectionCard {
    let mut providers = Vec::new();
    for t in tracks {
        if !providers.contains(&t.provider) {
            providers.push(t.provider);
        }
    }
    if providers.is_empty() {
        providers.push(row.provider);
    }
    CollectionCard { count: tracks.len(), covers: tracks.iter().take(4).cloned().collect(), providers, row }
}

/// Artists with the same name, ignoring case, are one artist in All.
fn same_artist(a: &CollectionRow, b: &CollectionRow) -> bool {
    a.kind == CollectionKind::Artist && b.kind == CollectionKind::Artist && a.name.to_lowercase() == b.name.to_lowercase()
}

/// Every collection of `kind` from `provider` (None: all, with same-named artists as one); empty ones show only if the user made them.
pub fn cards(lib: &Library, kind: CollectionKind, provider: Option<ProviderId>) -> anyhow::Result<Vec<CollectionCard>> {
    let mut groups: Vec<(CollectionRow, Vec<Track>)> = Vec::new();
    for row in lib.collections(provider, kind)? {
        let tracks = lib.collection_tracks(row.id)?;
        match groups.last_mut().filter(|(first, _)| provider.is_none() && same_artist(first, &row)) {
            Some((_, all)) => all.extend(tracks),
            None => groups.push((row, tracks)),
        }
    }
    Ok(groups.into_iter().map(|(row, tracks)| card(row, &tracks)).filter(|c| c.count > 0 || c.row.user).collect())
}

/// A collection and its songs; in All (`all`), an artist's songs from every source.
pub fn page(lib: &Library, id: i64, all: bool) -> anyhow::Result<CollectionPage> {
    let row = lib.collection(id)?;
    let mut tracks = Vec::new();
    if all && row.kind == CollectionKind::Artist {
        for other in lib.collections(None, CollectionKind::Artist)?.iter().filter(|o| same_artist(o, &row)) {
            tracks.extend(lib.collection_tracks(other.id)?);
        }
    } else {
        tracks = lib.collection_tracks(id)?;
    }
    Ok(CollectionPage { card: card(row, &tracks), tracks })
}

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>, kind: String, provider: Option<String>) -> Result<Vec<CollectionCard>, AppError> {
    let kind: CollectionKind = kind.parse().plain()?;
    let provider = provider.map(|p| p.parse::<ProviderId>()).transpose().plain()?;
    cards(&state.lib.lock().unwrap(), kind, provider).plain()
}

#[tauri::command]
pub fn open_collection(state: State<'_, AppState>, id: i64, all: bool) -> Result<CollectionPage, AppError> {
    page(&state.lib.lock().unwrap(), id, all).plain()
}
```

In `adding.rs` `search_input`: `all.extend(cards(lib, kind, None)?);` and the name a card is matched by:

```rust
    let imported_name = |c: &CollectionCard| c.row.provider == ProviderId::Local && c.row.kind == CollectionKind::Playlist && !c.row.user;
    let collections = best_first(all, |c| fuzzy.score(if imported_name(c) { imported } else { &c.row.name }));
```

(import `ProviderId`).

In `app/src/lib/api.ts` add `providers: Track["provider"][];` and `artworkPath: string | null;` to `CollectionCard` and change:

```ts
export const listCollections = (kind: Kind, provider: "local" | "apple" | null = null) => invoke<CollectionCard[]>("list_collections", { kind, provider });
export const openCollection = (id: number, all = true) => invoke<CollectionPage>("open_collection", { id, all });
```

- [ ] **Step 4: Web artwork on the Mac and on phones**

Apple songs' and collections' artwork is a web address (`https://is1-ssl.mzstatic.com/…/600x600bb.jpg`). Read `art.ts`, `KaraokeBackground.svelte` and `phones/mod.rs` at HEAD first.

In `app/src-tauri/src/phones/mod.rs` tests add:

```rust
    #[test]
    fn web_artwork_reaches_phones_as_it_is() {
        let mut v = serde_json::json!({ "a": { "artworkPath": "https://is1-ssl.mzstatic.com/made/up/600x600bb.jpg" }, "b": [{ "artworkPath": "/Users/me/Library/art/ab12.jpg" }] });
        art_links(&mut v);
        assert_eq!(v, serde_json::json!({ "a": { "artworkPath": "https://is1-ssl.mzstatic.com/made/up/600x600bb.jpg" }, "b": [{ "artworkPath": "/art/ab12.jpg" }] }));
    }
```

Run `cargo test -p kara-app web_artwork_reaches_phones` — it fails (the address becomes `/art/600x600bb.jpg`). In `art_links`, rewrite only paths that aren't web addresses:

```rust
                    Value::String(path) if k == "artworkPath" && !path.starts_with("https://") && !path.starts_with("http://") => {
```

In `app/src/lib/art.ts` replace Phase 2's `pictureUrl` with one exported helper and use it everywhere `pictureUrl` was used:

```ts
/** A picture's URL: the phone server's `/art/` links and web addresses as they are, a file on this Mac through the asset protocol. */
export const artworkUrl = (path: string) => (/^(https?:\/\/|\/art\/)/.test(path) ? path : convertFileSrc(path));
```

In `KaraokeBackground.svelte` import `artworkUrl` from `$lib/art` and use `artworkUrl(track.artworkPath)` instead of `convertFileSrc(track.artworkPath)` (drop the unused import).

- [ ] **Step 5: Run the tests and checks**

Run: `source "$HOME/.cargo/env" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cd app && npm run check && npm test && npx playwright test library playlists karaoke`
Expected: all pass (Phase 2's phone tests included).

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/library.rs app/src-tauri/src/adding.rs app/src-tauri/src/phones app/src/lib/api.ts app/src/lib/art.ts app/src/lib/components/KaraokeBackground.svelte
git commit -m "feat(app): collections by source; artists merged in All; only Local has Imported; web artwork on the Mac and phones"
```

---

### Task 9: The app — Change match, animated covers and Apple search commands

**Files:**
- Create: `crates/kara-core/src/apple/catalog.rs`; `app/src-tauri/src/matches.rs`
- Modify: `crates/kara-core/src/apple/mod.rs` (`pub mod catalog;`), `crates/kara-core/src/library/mod.rs` (`search` lists only your songs)
- Modify: `app/src-tauri/src/sources.rs` (motion and search commands), `app/src-tauri/src/player.rs` (`Player::renew`, `restart`), `app/src-tauri/src/lib.rs` (`mod matches;`, commands)

**Interfaces:**
- Consumes: `apple::{Client, parse, fake_client, reply}`, `sources::client` (Task 7), `matching::{search_once, YouTube { store }}` (Task 6), read `player.rs` at HEAD first (Phase 2), `Library::{sources, add_match, select_source, track_album_ref, collection_catalog_ref, motion, set_motion}`.
- Produces:
  - `apple::catalog::motion(lib: &Library, client: &Client, kind: CollectionKind, catalog_ref: &str) -> Result<Option<String>>` (kept 30 days; artists have none)
  - `apple::catalog::search(lib: &Library, client: &Client, query: &str) -> Result<Vec<i64>>` (up to 10 songs, saved as Apple songs)
  - `apple::catalog::suggestions(client: &Client, query: &str) -> Result<Vec<String>>` (up to 5)
  - `Library::search` lists local songs and streaming songs that are in a collection or have an upload — not songs that were only search results.
  - Commands: `track_motion(track_id) -> Option<String>`, `collection_motion(collection_id) -> Option<String>` (None on any failure), `apple_search(query) -> Vec<Track>`, `apple_suggestions(query) -> Vec<String>` (both empty without Apple Music), `match_candidates(track_id) -> Vec<MatchCandidate>`, `choose_match(track_id, source_id)`, `choose_match_link(track_id, url) -> String` (the title in use).
  - `MatchCandidate { sourceId, url, title, channel, durationMs, thumbnail, inUse }` (camelCase JSON).
  - `Player::renew(&mut self, track_id: i64)`; `player::restart(app: &AppHandle, state: &AppState, track_id: i64) -> Result<PlayerSnapshot, AppError>` — when the song is coming up, stops the worker's job (it is queued again at once) and, if it is the current one, gives its entry a new key so the app loads its new audio.

- [ ] **Step 1: Write the failing tests**

Create `crates/kara-core/src/apple/catalog.rs` with the doc line from Step 3 and:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::apple::{fake_client, reply};

    #[test]
    fn an_animated_cover_or_its_absence_is_looked_up_once() {
        let lib = Library::open_in_memory().unwrap();
        let (c, asked) = fake_client(|url| {
            if url.contains("/albums/900001?") { reply(200, include_str!("samples/catalog-album-motion.json")) } else { reply(404, "") }
        });
        let url = motion(&lib, &c, CollectionKind::Album, "900001").unwrap();
        assert_eq!(url.as_deref(), Some("https://mvod.itunes.apple.com/itunes-assets/HLSMusic/v4/ma/de/up/P000000001_default.m3u8"));
        assert_eq!(motion(&lib, &c, CollectionKind::Album, "900001").unwrap(), url);
        assert_eq!(motion(&lib, &c, CollectionKind::Playlist, "pl.madeup").unwrap(), None);
        assert_eq!(motion(&lib, &c, CollectionKind::Playlist, "pl.madeup").unwrap(), None);
        assert_eq!(motion(&lib, &c, CollectionKind::Artist, "x").unwrap(), None);
        assert_eq!(*asked.lock().unwrap(), ["https://amp-api.music.apple.com/v1/catalog/us/albums/900001?extend=editorialVideo", "https://amp-api.music.apple.com/v1/catalog/us/playlists/pl.madeup?extend=editorialVideo"]);
    }

    #[test]
    fn search_saves_the_songs_it_finds_so_they_can_be_queued_and_suggests_phrases() {
        let lib = Library::open_in_memory().unwrap();
        let (c, asked) = fake_client(|url| {
            if url.contains("/search/suggestions?") { reply(200, include_str!("samples/catalog-suggestions.json")) } else { reply(200, include_str!("samples/catalog-search.json")) }
        });
        let ids = search(&lib, &c, "paper boats").unwrap();
        let t = lib.track(ids[0]).unwrap();
        assert_eq!((t.title.as_str(), t.provider, lib.track_isrc(t.id).unwrap().as_deref()), ("Paper Boats", ProviderId::Apple, Some("QZZZZ2600010")));
        assert!(lib.search("paper boats", 5).unwrap().is_empty(), "a search result isn't one of your songs");
        assert_eq!(suggestions(&c, "paper b").unwrap(), ["paper boats", "paper boats juniper row"]);
        assert!(asked.lock().unwrap()[0].ends_with("term=paper+boats"));
    }
}
```

In `app/src-tauri/src/player.rs` tests add:

```rust
    #[test]
    fn a_song_whose_audio_changed_gets_a_new_entry_key_only_when_it_is_playing() {
        let mut p = Player::default();
        p.play(&[1, 2], 0);
        let keys = |p: &Player| p.entries.iter().map(|e| e.key).collect::<Vec<_>>();
        let before = keys(&p);
        p.renew(2);
        assert_eq!(keys(&p), before);
        p.renew(1);
        assert_ne!(keys(&p)[0], before[0]);
        assert_eq!((keys(&p)[1], p.current_track()), (before[1], Some(1)));
    }
```

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core apple::catalog && cargo test -p kara-app player`
Expected: compile errors — `motion`, `search`, `renew` not found.

- [ ] **Step 2: Your songs in search**

In `Library::search`, replace the statement's SQL with:

```rust
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TRACK_COLS} FROM track t WHERE t.provider = 'local'
               OR EXISTS (SELECT 1 FROM collection_track ct WHERE ct.track_id = t.id)
               OR EXISTS (SELECT 1 FROM audio_source s WHERE s.track_id = t.id)"
        ))?;
```

and extend its doc comment: `/// Fuzzy search over your songs' title, artist and album, best matches first; a title match counts a little more. Streaming songs count as yours once they are in a collection or have an upload.`

- [ ] **Step 3: Catalog look-ups**

Top of `apple/catalog.rs` (and `pub mod catalog;` in `apple/mod.rs`):

```rust
//! Apple Music's catalog: animated covers, search and search phrases.

use super::{parse, Client};
use crate::library::{CollectionKind, Library, ProviderId};
use anyhow::Result;

/// How long a looked-up animated cover, or its absence, is kept.
const MOTION_KEPT_MS: i64 = 30 * 24 * 3600 * 1000;
const SEARCH_SONGS: usize = 10;
const SUGGESTIONS: usize = 5;

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// The looping video of a catalog album or playlist, looked up at most once a month.
pub fn motion(lib: &Library, client: &Client, kind: CollectionKind, catalog_ref: &str) -> Result<Option<String>> {
    let what = match kind {
        CollectionKind::Album => "albums",
        CollectionKind::Playlist => "playlists",
        CollectionKind::Artist => return Ok(None),
    };
    let reference = format!("{what}:{catalog_ref}");
    if let Some((url, at)) = lib.motion(&reference)? {
        if crate::now_ms() - at < MOTION_KEPT_MS {
            return Ok(url);
        }
    }
    let answer = client.find(&format!("/v1/catalog/{}/{what}/{catalog_ref}?extend=editorialVideo", client.storefront()))?;
    let url = answer.as_ref().and_then(parse::motion);
    lib.set_motion(&reference, url.as_deref())?;
    Ok(url)
}

/// Catalog songs for `query`, saved as Apple songs so they can be queued like any song (a refresh prunes them unless used).
pub fn search(lib: &Library, client: &Client, query: &str) -> Result<Vec<i64>> {
    let answer = client.get(&format!("/v1/catalog/{}/search?types=songs&limit={SEARCH_SONGS}&term={}", client.storefront(), encode(query)))?;
    parse::search_songs(&answer).iter().map(|t| lib.upsert_streaming_track(ProviderId::Apple, t)).collect()
}

/// Apple Music's search phrases for what's been typed.
pub fn suggestions(client: &Client, query: &str) -> Result<Vec<String>> {
    let answer = client.get(&format!("/v1/catalog/{}/search/suggestions?kinds=terms&limit={SUGGESTIONS}&term={}", client.storefront(), encode(query)))?;
    Ok(parse::suggestions(&answer))
}
```

The test expects the search URL to end with `term=paper+boats`, so `term` stays last.

- [ ] **Step 4: Restarting a song whose audio changed**

In `player.rs`, `impl Player`:

```rust
    /// Gives the current entry a new key when it is `track_id`, so the app loads its audio again.
    pub fn renew(&mut self, track_id: i64) {
        if let Some(c) = self.current.filter(|&c| self.entries[c].track_id == track_id) {
            self.next_key += 1;
            self.entries[c].key = self.next_key;
        }
    }
```

and below `update`:

```rust
/// Prepares a song again after its audio changed: when it is coming up, stops the worker's job so it starts afresh,
/// and if it is playing, starts it over.
pub(crate) fn restart(app: &AppHandle, state: &AppState, track_id: i64) -> Result<PlayerSnapshot, AppError> {
    let coming_up = state.player.lock().unwrap().upcoming().contains(&track_id);
    if let Some(w) = state.worker.lock().unwrap().as_ref().filter(|_| coming_up) {
        w.play(Vec::new());
    }
    update(app, state, |p, _| {
        p.renew(track_id);
        Ok(())
    })
}
```

- [ ] **Step 5: Match commands**

Create `app/src-tauri/src/matches.rs`:

```rust
//! Changing which upload a streaming song is sung from.

use crate::state::{AppError, AppState, Plain};
use anyhow::Context;
use kara_core::ingest::link::{self, LinkVerdict};
use kara_core::ingest::preview::{self, LinkPreview, SearchHit};
use kara_core::library::{AudioSource, Library};
use kara_core::matching;
use kara_core::problem::Problem;
use serde::Serialize;
use tauri::{AppHandle, State};

/// An upload a streaming song can be sung from, as the Change match sheet shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidate {
    source_id: i64,
    url: String,
    title: String,
    channel: Option<String>,
    duration_ms: Option<i64>,
    thumbnail: Option<String>,
    in_use: bool,
}

impl From<AudioSource> for MatchCandidate {
    fn from(s: AudioSource) -> Self {
        Self { source_id: s.id, title: s.label.unwrap_or_else(|| s.uri.clone()), url: s.uri, channel: s.channel, duration_ms: s.duration_ms, thumbnail: s.thumbnail, in_use: s.selected }
    }
}

/// The uploads a streaming song can be sung from, looking for them the first time.
#[tauri::command]
pub async fn match_candidates(state: State<'_, AppState>, track_id: i64) -> Result<Vec<MatchCandidate>, AppError> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<MatchCandidate>> {
        let lib = Library::open(&store.db_path())?;
        if lib.sources(track_id)?.is_empty() {
            matching::search_once(&lib, track_id, &matching::YouTube { store: store.clone() })?;
        }
        Ok(lib.sources(track_id)?.into_iter().map(Into::into).collect())
    })
    .await?
    .plain()
}

/// Sings a streaming song from one of its uploads from now on.
#[tauri::command]
pub fn choose_match(app: AppHandle, state: State<'_, AppState>, track_id: i64, source_id: i64) -> Result<(), AppError> {
    state.lib.lock().unwrap().select_source(track_id, source_id).plain()?;
    crate::player::restart(&app, state.inner(), track_id).map(drop)
}

/// Sings a streaming song from a pasted link from now on; returns the title shown for it.
#[tauri::command]
pub async fn choose_match_link(app: AppHandle, state: State<'_, AppState>, track_id: i64, url: String) -> Result<String, AppError> {
    let url = link::parse_link(&url).map(|u| link::canonical(&u)).context(Problem::NotALink).plain()?;
    if !matches!(link::verdict(&url), LinkVerdict::Extractable | LinkVerdict::AudioFile) {
        return Err(anyhow::Error::from(link::rejection(&url).unwrap_or(Problem::LinkUnsupported)).into());
    }
    let found = {
        let url = url.clone();
        tauri::async_runtime::spawn_blocking(move || preview::oembed(&url).ok().flatten()).await?
    };
    let hit = SearchHit { url: url.to_string(), preview: found.unwrap_or(LinkPreview { title: url.to_string(), channel: None, duration_ms: None, thumbnail: None }) };
    {
        let lib = state.lib.lock().unwrap();
        let id = lib.add_match(track_id, &hit).plain()?;
        lib.select_source(track_id, id).plain()?;
    }
    crate::player::restart(&app, state.inner(), track_id)?;
    Ok(hit.preview.title)
}
```

- [ ] **Step 6: Motion and Apple search commands**

Append to `app/src-tauri/src/sources.rs` (imports: `kara_core::apple::catalog`, `kara_core::library::{CollectionKind, Track}`):

```rust
/// Looks up the animated cover of what `target` names in the library; None without Apple Music or on any failure.
async fn motion_for(
    app: AppHandle,
    target: impl FnOnce(&Library) -> anyhow::Result<Option<(CollectionKind, String)>> + Send + 'static,
) -> Result<Option<String>, AppError> {
    let found = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Option<String>> {
        let Some(client) = client(&app)? else { return Ok(None) };
        let lib = Library::open(&library(&app)?.store.db_path())?;
        let Some((kind, id)) = target(&lib)? else { return Ok(None) };
        catalog::motion(&lib, &client, kind, &id)
    })
    .await?;
    Ok(found.unwrap_or(None))
}

/// The looping video of a song's album.
#[tauri::command]
pub async fn track_motion(app: AppHandle, track_id: i64) -> Result<Option<String>, AppError> {
    motion_for(app, move |lib| Ok(lib.track_album_ref(track_id)?.map(|id| (CollectionKind::Album, id)))).await
}

/// The looping video of an Apple album or playlist.
#[tauri::command]
pub async fn collection_motion(app: AppHandle, collection_id: i64) -> Result<Option<String>, AppError> {
    motion_for(app, move |lib| {
        let kind = lib.collection(collection_id)?.kind;
        Ok(lib.collection_catalog_ref(collection_id)?.map(|id| (kind, id)))
    })
    .await
}

/// Apple Music songs for the search bar's words; none without Apple Music.
#[tauri::command]
pub async fn apple_search(app: AppHandle, query: String) -> Result<Vec<Track>, AppError> {
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<Track>> {
        let Some(client) = client(&app)? else { return Ok(Vec::new()) };
        let lib = Library::open(&library(&app)?.store.db_path())?;
        catalog::search(&lib, &client, &query)?.into_iter().map(|id| lib.track(id)).collect()
    })
    .await?
    .plain()
}

/// Apple Music's search phrases for the search bar's words; none without Apple Music or when it can't be reached.
#[tauri::command]
pub async fn apple_suggestions(app: AppHandle, query: String) -> Result<Vec<String>, AppError> {
    let phrases = tauri::async_runtime::spawn_blocking(move || client(&app).ok().flatten().and_then(|c| catalog::suggestions(&c, &query).ok()));
    Ok(phrases.await?.unwrap_or_default())
}
```

In `lib.rs` add `mod matches;` and register:

```rust
            sources::track_motion,
            sources::collection_motion,
            sources::apple_search,
            sources::apple_suggestions,
            matches::match_candidates,
            matches::choose_match,
            matches::choose_match_link,
```

- [ ] **Step 7: Run the tests and checks**

Run: `source "$HOME/.cargo/env" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all pass (existing search tests still pass: their songs are local).

- [ ] **Step 8: Commit**

```bash
git add crates/kara-core/src/apple crates/kara-core/src/library/mod.rs app/src-tauri/src
git commit -m "feat(app): Change match, animated covers and Apple Music search commands"
```

---

### Task 10: Settings › Sources

**Files:**
- Modify: `app/src/lib/api.ts`, `app/src/lib/format.ts` (read it first: Phase 2 added `loudness()`); Create: `app/src/lib/state/sources.svelte.ts`
- Modify: `app/src/lib/components/SettingsSheet.svelte`, `app/src/routes/+page.svelte`, `app/src/lib/i18n/*.ts`
- Modify: `app/tests/fake-backend.ts` (read it first: Phase 2 changed it); Create: `app/tests/apple-music.spec.ts`

**Interfaces:**
- Consumes: commands `apple_status`, `apple_connect`, `apple_refresh`, `apple_disconnect`, event `"sources"` (Task 7); `library.refresh()`, `toasts`, `say`, `t`.
- Produces:
  - `api.ts`: `interface Account { displayName: string | null; refreshedAt: number | null; signedOut: boolean }`, `interface AppleStatus { account: Account | null; connecting: boolean; refreshing: boolean; problem: ProblemCode | null; revision: number }`, `appleStatus()`, `appleConnect()`, `appleRefresh()`, `appleDisconnect()`, `onSources(cb)`.
  - `format.ts`: `ago(ms: number, locale: string, now = Date.now()): string`.
  - `state/sources.svelte.ts`: `sources` with `apple: AppleStatus`, `connected` (an account exists, even signed out), `searchable` (signed in), `init()`, `connect()`; reloads the library when `revision` or the account's presence changes; toasts "Connected to Apple Music[ as …]" after a login and the login-ran-out message with Log in.
  - Fake backend: `window.fake.appleConnected()`, `window.fake.appleSignedOut()`; Apple songs 201 "Neon Tidewater" (Juniper Row, Night Line) and 202 "Umbrella Weather" (Odd Hours Club, Odd Hours); account name "Mina".

- [ ] **Step 1: Write the failing tests**

`app/tests/apple-music.spec.ts`:

```ts
import type { Page } from "@playwright/test";
import { test, expect, calls } from "./app";

const openSettings = async (page: Page) => {
  await page.getByRole("button", { name: "Settings" }).click();
  return page.getByRole("dialog", { name: "Settings" });
};

test("connecting Apple Music opens its login, then shows the account and when it was refreshed", async ({ page }) => {
  const settings = await openSettings(page);
  await expect(settings).toContainText("Your playlists, albums and artists");
  await settings.getByRole("button", { name: "Connect" }).click();
  expect(await calls(page, "apple_connect")).toHaveLength(1);
  await expect(page.locator(".toasts")).toContainText("Connected to Apple Music as Mina");
  await expect(settings).toContainText("Mina · Refreshed");
  await expect(settings.getByRole("button", { name: "Disconnect" })).toBeVisible();
});

test("disconnecting asks first, then Apple Music can be connected again", async ({ page }) => {
  await page.evaluate(() => window.fake.appleConnected());
  const settings = await openSettings(page);
  await settings.getByRole("button", { name: "Disconnect" }).click();
  await expect(settings).toContainText("Disconnect Apple Music? Its playlists and albums leave KaraAlwaysOK. Songs you've sung stay.");
  await settings.locator(".confirm").getByRole("button", { name: "Disconnect" }).click();
  await expect(page.locator(".toasts")).toContainText("Apple Music disconnected");
  await expect(settings.getByRole("button", { name: "Connect" })).toBeVisible();
});

test("a login that ran out says so, and Log in opens the login window again", async ({ page }) => {
  await page.evaluate(() => window.fake.appleConnected());
  await page.evaluate(() => window.fake.appleSignedOut());
  const toast = page.locator(".toasts");
  await expect(toast).toContainText("Log in to Apple Music again");
  await toast.getByRole("button", { name: "Log in" }).click();
  expect(await calls(page, "apple_connect")).toHaveLength(1);
});
```

- [ ] **Step 2: Run to see them fail**

Run: `cd app && npx playwright test apple-music`
Expected: FAIL — the fake backend has no `apple_status` and Settings has no Sources section.

- [ ] **Step 3: The fake backend**

In `app/tests/fake-backend.ts`: add `AppleStatus` to the `$lib/api` type import. After `const groupIds = new Map<string, number>();` add:

```ts
const appleSong = (id: number, title: string, artist: string, album: string, durationMs: number, artSeed: number): Track => ({
  ...song(id, title, artist, album, durationMs, artSeed),
  provider: "apple",
});
const appleTracks = [appleSong(201, "Neon Tidewater", "Juniper Row", "Night Line", 214_000, 250), appleSong(202, "Umbrella Weather", "Odd Hours Club", "Odd Hours", 178_000, 60)];
let apple: AppleStatus = { account: null, connecting: false, refreshing: false, problem: null, revision: 0 };

/** Sends Apple Music's new status to the app, as the real backend does after every change. */
function setApple(change: Partial<AppleStatus>) {
  apple = { ...apple, ...change };
  void emit("sources", apple);
}

/** Apple Music as right after a login: its songs are in the library and it was just refreshed. */
function connectApple() {
  for (const t of appleTracks) tracks.set(t.id, t);
  setApple({ account: { displayName: "Mina", refreshedAt: Date.now(), signedOut: false }, connecting: false, revision: apple.revision + 1 });
}
```

In the `fake` object add:

```ts
  /** Apple Music connects as if the user had just logged in. */
  appleConnected: () => connectApple(),
  /** Apple Music refuses the saved login. */
  appleSignedOut: () => setApple({ account: { ...apple.account!, signedOut: true } }),
```

In `commands` add:

```ts
  apple_status: () => apple,
  apple_connect: () => {
    setApple({ connecting: true });
    setTimeout(connectApple, 50);
  },
  apple_refresh: () => setApple({ account: { ...apple.account!, refreshedAt: Date.now() }, revision: apple.revision + 1 }),
  apple_disconnect: () => {
    for (const t of appleTracks) tracks.delete(t.id);
    setApple({ account: null, revision: apple.revision + 1 });
  },
```

- [ ] **Step 4: API, `ago` and the sources state**

`api.ts`:

```ts
export interface Account { displayName: string | null; refreshedAt: number | null; signedOut: boolean }
export interface AppleStatus { account: Account | null; connecting: boolean; refreshing: boolean; problem: ProblemCode | null; revision: number }

export const appleStatus = () => invoke<AppleStatus>("apple_status");
export const appleConnect = () => invoke<void>("apple_connect");
export const appleRefresh = () => invoke<void>("apple_refresh");
export const appleDisconnect = () => invoke<void>("apple_disconnect");
export const onSources = (cb: (s: AppleStatus) => void) => listen<AppleStatus>("sources", (e) => cb(e.payload));
```

`format.ts`:

```ts
/** How long ago `ms` (since 1970) was, like "5 minutes ago", in `locale`, in the largest unit that fits. */
export function ago(ms: number, locale: string, now = Date.now()): string {
  const s = Math.round((ms - now) / 1000);
  const units: [Intl.RelativeTimeFormatUnit, number][] = [["day", 86_400], ["hour", 3_600], ["minute", 60]];
  const [unit, size] = units.find(([, size]) => Math.abs(s) >= size) ?? ["second", 1];
  return new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(Math.round(s / size), unit);
}
```

Create `app/src/lib/state/sources.svelte.ts`:

```ts
import { appleConnect, appleStatus, onSources, type AppleStatus } from "$lib/api";
import { say } from "$lib/i18n/engine";
import { t } from "$lib/i18n/index.svelte";
import { library } from "./library.svelte";
import { toasts } from "./toasts.svelte";
import AppleLogoIcon from "phosphor-svelte/lib/AppleLogoIcon";
import SignInIcon from "phosphor-svelte/lib/SignInIcon";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

const signedIn = (s: AppleStatus) => !!s.account && !s.account.signedOut;

class Sources {
  apple = $state<AppleStatus>({ account: null, connecting: false, refreshing: false, problem: null, revision: 0 });
  /** Apple Music is connected, even if its login ran out. */
  connected = $derived(this.apple.account !== null);
  /** Apple Music can answer searches now. */
  searchable = $derived(signedIn(this.apple));

  /** Follows Apple Music's status from the backend. */
  async init() {
    await onSources((s) => this.apply(s));
    this.apply(await appleStatus().catch(() => this.apple));
  }

  /** Opens the Apple Music login window. */
  connect() {
    appleConnect().catch((e) => toasts.show(say(e), { icon: WarningIcon }));
  }

  /** Takes a new status: reloads the library when Apple Music content changed, and says when a login worked or ran out. */
  private apply(s: AppleStatus) {
    const before = this.apple;
    this.apple = s;
    if (s.revision !== before.revision || !!s.account !== !!before.account) void library.refresh();
    if (before.connecting && signedIn(s) && !signedIn(before)) {
      const name = s.account?.displayName;
      toasts.show(name ? t("sources.connectedAs", { name }) : t("sources.connected"), { icon: AppleLogoIcon });
    }
    if (s.account?.signedOut && !before.account?.signedOut) {
      toasts.show(t("problem.appleSignedOut"), { icon: WarningIcon, action: { label: t("sources.logIn"), icon: SignInIcon, run: () => this.connect() } });
    }
  }
}

export const sources = new Sources();
```

In `+page.svelte` (read it first: Phase 2 added the mic pill and sheet) import `sources` and add `void sources.init();` to `onMount`.

- [ ] **Step 5: The Sources section**

In `SettingsSheet.svelte` add to the script (merge the `$lib/api` import):

```ts
  import { appleDisconnect, appleRefresh, clearStorage, setStorageLimit, storageInfo, type StorageInfo } from "$lib/api";
  import { ago } from "$lib/format";
  import { sources } from "$lib/state/sources.svelte";
  import AppleLogoIcon from "phosphor-svelte/lib/AppleLogoIcon";
  import PlugsConnectedIcon from "phosphor-svelte/lib/PlugsConnectedIcon";
  import PlugsIcon from "phosphor-svelte/lib/PlugsIcon";
  import SignInIcon from "phosphor-svelte/lib/SignInIcon";

  let disconnecting = $state(false);
  const apple = $derived(sources.apple);
  /** The line under Apple Music: what it adds, the login to redo, or the account with its last refresh or problem. */
  const appleLine = $derived.by(() => {
    const a = apple.account;
    if (!a) return t(apple.connecting ? "sources.connecting" : "sources.about");
    if (a.signedOut) return t("sources.logInAgain");
    if (apple.refreshing) return t("sources.refreshing");
    const last = apple.problem ? say({ problem: apple.problem }) : a.refreshedAt ? t("sources.refreshed", { when: ago(a.refreshedAt, i18n.locale) }) : "";
    return [a.displayName, last].filter(Boolean).join(" · ");
  });

  async function disconnect() {
    disconnecting = false;
    try {
      await appleDisconnect();
      toasts.show(t("sources.disconnected"), { icon: PlugsIcon });
    } catch (e) {
      failed(e);
    }
  }
```

In the `$effect` that runs when the sheet opens, also set `disconnecting = false;`. Insert as the first section inside `<Sheet …>`:

```svelte
    <section class="set">
      <h3><PlugsConnectedIcon size={18} />{t("sources.title")}</h3>
      <div class="hstack source">
        <span class="logo"><AppleLogoIcon size={20} /></span>
        <div class="grow"><b>{t("sources.apple")}</b><small class="muted">{appleLine}</small></div>
        {#if !apple.account || apple.account.signedOut}
          <button class="btn accent" disabled={apple.connecting} onclick={() => sources.connect()}>
            {#if apple.account}<SignInIcon />{t("sources.logIn")}{:else}<PlugsConnectedIcon />{t("sources.connect")}{/if}
          </button>
        {:else}
          <button class="ib" use:tip={t("sources.refresh")} disabled={apple.refreshing} onclick={() => void appleRefresh().catch(failed)}><ArrowsClockwiseIcon size={18} /></button>
          <button class="btn" disabled={apple.refreshing} onclick={() => (disconnecting = true)}><PlugsIcon />{t("sources.disconnect")}</button>
        {/if}
      </div>
      {#if disconnecting}
        <div class="hstack confirm" in:fade>
          <WarningIcon /><span class="grow">{t("sources.disconnectAsk")}</span>
          <button class="btn" onclick={() => (disconnecting = false)}>{t("common.cancel")}</button>
          <button class="btn accent" onclick={disconnect}><PlugsIcon />{t("sources.disconnect")}</button>
        </div>
      {/if}
    </section>
```

and to its `<style>`:

```css
  .source b { display: block; font-weight: 600; }
  .source small { display: block; font-size: 12.5px; }
  .logo { width: 36px; height: 36px; border-radius: var(--r-sm); display: grid; place-items: center; background: var(--raised); flex: none; }
```

- [ ] **Step 6: Text in six languages**

Append:

`en.ts`:
```ts
  "sources.title": "Sources",
  "sources.apple": "Apple Music",
  "sources.about": "Your playlists, albums and artists",
  "sources.connect": "Connect",
  "sources.connecting": "Log in in the Apple Music window",
  "sources.connected": "Connected to Apple Music",
  "sources.connectedAs": "Connected to Apple Music as {name}",
  "sources.logIn": "Log in",
  "sources.logInAgain": "Log in again to refresh your library",
  "sources.refresh": "Refresh",
  "sources.refreshing": "Refreshing…",
  "sources.refreshed": "Refreshed {when}",
  "sources.disconnect": "Disconnect",
  "sources.disconnectAsk": "Disconnect Apple Music? Its playlists and albums leave KaraAlwaysOK. Songs you've sung stay.",
  "sources.disconnected": "Apple Music disconnected",
```
`ja.ts`:
```ts
  "sources.title": "ソース",
  "sources.apple": "Apple Music",
  "sources.about": "プレイリスト、アルバム、アーティスト",
  "sources.connect": "接続",
  "sources.connecting": "Apple Music のウインドウでログインしてください",
  "sources.connected": "Apple Music に接続しました",
  "sources.connectedAs": "{name} として Apple Music に接続しました",
  "sources.logIn": "ログイン",
  "sources.logInAgain": "ライブラリを更新するには、もう一度ログインしてください",
  "sources.refresh": "更新",
  "sources.refreshing": "更新中…",
  "sources.refreshed": "最終更新：{when}",
  "sources.disconnect": "接続を解除",
  "sources.disconnectAsk": "Apple Music の接続を解除しますか？プレイリストとアルバムは KaraAlwaysOK から消えます。歌った曲は残ります。",
  "sources.disconnected": "Apple Music の接続を解除しました",
```
`ko.ts`:
```ts
  "sources.title": "소스",
  "sources.apple": "Apple Music",
  "sources.about": "플레이리스트, 앨범, 아티스트",
  "sources.connect": "연결",
  "sources.connecting": "Apple Music 창에서 로그인하세요",
  "sources.connected": "Apple Music에 연결되었습니다",
  "sources.connectedAs": "{name}(으)로 Apple Music에 연결되었습니다",
  "sources.logIn": "로그인",
  "sources.logInAgain": "보관함을 새로 고치려면 다시 로그인하세요",
  "sources.refresh": "새로 고침",
  "sources.refreshing": "새로 고치는 중…",
  "sources.refreshed": "마지막 새로 고침: {when}",
  "sources.disconnect": "연결 해제",
  "sources.disconnectAsk": "Apple Music 연결을 해제할까요? 플레이리스트와 앨범은 KaraAlwaysOK에서 사라집니다. 부른 노래는 남습니다.",
  "sources.disconnected": "Apple Music 연결을 해제했습니다",
```
`zh-Hans.ts`:
```ts
  "sources.title": "来源",
  "sources.apple": "Apple Music",
  "sources.about": "你的播放列表、专辑和艺人",
  "sources.connect": "连接",
  "sources.connecting": "请在 Apple Music 窗口中登录",
  "sources.connected": "已连接 Apple Music",
  "sources.connectedAs": "已以 {name} 的身份连接 Apple Music",
  "sources.logIn": "登录",
  "sources.logInAgain": "重新登录以刷新资料库",
  "sources.refresh": "刷新",
  "sources.refreshing": "正在刷新…",
  "sources.refreshed": "上次刷新：{when}",
  "sources.disconnect": "断开连接",
  "sources.disconnectAsk": "要断开 Apple Music 吗？它的播放列表和专辑将从 KaraAlwaysOK 中移除。你唱过的歌曲会保留。",
  "sources.disconnected": "已断开 Apple Music",
```
`zh-Hant.ts`:
```ts
  "sources.title": "來源",
  "sources.apple": "Apple Music",
  "sources.about": "你的播放清單、專輯和藝人",
  "sources.connect": "連接",
  "sources.connecting": "請在 Apple Music 視窗中登入",
  "sources.connected": "已連接 Apple Music",
  "sources.connectedAs": "已以 {name} 的身分連接 Apple Music",
  "sources.logIn": "登入",
  "sources.logInAgain": "重新登入以重新整理資料庫",
  "sources.refresh": "重新整理",
  "sources.refreshing": "正在重新整理…",
  "sources.refreshed": "上次重新整理：{when}",
  "sources.disconnect": "中斷連接",
  "sources.disconnectAsk": "要中斷 Apple Music 嗎？它的播放清單和專輯會從 KaraAlwaysOK 移除。你唱過的歌曲會保留。",
  "sources.disconnected": "已中斷 Apple Music",
```
`es.ts`:
```ts
  "sources.title": "Fuentes",
  "sources.apple": "Apple Music",
  "sources.about": "Tus listas, álbumes y artistas",
  "sources.connect": "Conectar",
  "sources.connecting": "Inicia sesión en la ventana de Apple Music",
  "sources.connected": "Conectado a Apple Music",
  "sources.connectedAs": "Conectado a Apple Music como {name}",
  "sources.logIn": "Iniciar sesión",
  "sources.logInAgain": "Vuelve a iniciar sesión para actualizar tu biblioteca",
  "sources.refresh": "Actualizar",
  "sources.refreshing": "Actualizando…",
  "sources.refreshed": "Actualizado {when}",
  "sources.disconnect": "Desconectar",
  "sources.disconnectAsk": "¿Desconectar Apple Music? Sus listas y álbumes saldrán de KaraAlwaysOK. Las canciones que cantaste se quedan.",
  "sources.disconnected": "Apple Music desconectado",
```

- [ ] **Step 7: Run the checks**

Run: `cd app && npm run check && npm test && npm run check:i18n && npx playwright test apple-music settings`
Expected: all pass (the existing Settings tests still find Storage, Language and About).

- [ ] **Step 8: Commit**

```bash
git add app/src app/tests
git commit -m "feat(app): Settings › Sources connects, refreshes and disconnects Apple Music"
```

---

### Task 11: The provider switcher, and collections' own artwork

**Files:**
- Modify: `app/src/lib/state/library.svelte.ts`, `app/src/lib/state/sources.svelte.ts`, `app/src/lib/state/adding.svelte.ts`
- Create: `app/src/lib/components/ProviderLogos.svelte`
- Modify: `app/src/lib/components/Sidebar.svelte`, `CollectionView.svelte`, `SongRow.svelte`, `Cover.svelte`, `SearchResults.svelte`, `app/src/routes/+page.svelte`
- Modify: `app/src/lib/i18n/*.ts`, `app/tests/fake-backend.ts`, `app/tests/apple-music.spec.ts`

**Interfaces:**
- Consumes: `listCollections(kind, provider)`, `openCollection(id, all)` and `CollectionCard.providers` (Task 8); `sources.connected`, `sources.apple.refreshing` (Task 10).
- Produces:
  - `library.svelte.ts`: `type Provider = "all" | "local" | "apple"`, `library.provider`, `library.setProvider(p)`; `library.show()` switches to All; `cardName` calls only Local's app-made playlist "Imported".
  - `ProviderLogos.svelte` `{ providers: Track["provider"][] }` — small logos with the source's name as tooltip.
  - `SongRow` prop `source?: boolean` — shows a streaming song's logo (used in your own playlists).
  - `Cover` prop `artwork?: string | null` — a collection's own picture (Apple playlists and albums) when set, else the Phase 1 song grid; every `Cover` of a card passes `artwork={card.artworkPath}`.
  - Fake backend: `playlists` entries carry `provider` and `artworkPath`; Apple playlist 301 "Late Night Drive" (songs 201, 202, artwork `https://example.com/made-up/late-night-drive/600x600bb.jpg`) while connected; `collections(kind, provider)`.

- [ ] **Step 1: Write the failing tests**

Append to `app/tests/apple-music.spec.ts`:

```ts
test("once Apple Music is connected, the sidebar shows all, only local or only Apple Music collections", async ({ page }) => {
  const source = page.getByRole("toolbar", { name: "Source" });
  await expect(source).toBeHidden();
  await page.evaluate(() => window.fake.appleConnected());
  await expect(source.getByRole("button")).toHaveCount(3);
  const sidebar = page.locator("aside");
  await expect(sidebar.getByRole("button", { name: /Late Night Drive/ })).toBeVisible();
  await source.getByRole("button", { name: "Apple Music" }).click();
  await expect(sidebar.locator(".item")).toHaveText([/Late Night Drive/]);
  await source.getByRole("button", { name: "Local" }).click();
  await expect(sidebar.getByRole("button", { name: /Imported/ })).toBeVisible();
  await expect(sidebar.getByRole("button", { name: /Late Night Drive/ })).toBeHidden();
});

test("an Apple Music playlist keeps its own name and its songs queue like any other", async ({ page }) => {
  await page.evaluate(() => window.fake.appleConnected());
  await page.locator("aside").getByRole("button", { name: /Late Night Drive/ }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Late Night Drive");
  await expect(page.locator(".hero .cover .art").first()).toHaveCSS("background-image", /made-up\/late-night-drive/);
  await page.locator(".row", { hasText: "Neon Tidewater" }).click();
  expect(await calls(page, "queue_add")).toEqual([{ trackId: 201, next: false }]);
});

test("in All, an artist from both sources shows once with songs from both", async ({ page }) => {
  await page.evaluate(() => window.fake.appleConnected());
  await page.getByRole("button", { name: "Artists", exact: true }).click();
  const juniper = page.locator("aside").getByRole("button", { name: /Juniper Row/ });
  await expect(juniper).toHaveCount(1);
  await juniper.click();
  await expect(page.locator(".rows .row")).toHaveCount(3);
});
```

Run: `cd app && npx playwright test apple-music`
Expected: the three new tests FAIL (no "Source" toolbar; the Apple playlist isn't listed).

- [ ] **Step 2: The fake backend by source**

In `app/tests/fake-backend.ts`:

```ts
type Source = Track["provider"];
```

Give `playlists` a type, a provider and artwork: `const playlists: { id: number; name: string; user: boolean; provider: Source; artworkPath: string | null; trackIds: number[] }[] = [ { id: 1, name: "Imported", user: false, provider: "local", artworkPath: null, trackIds: [1, 2, 3, 4, 5] }, { id: 2, name: "Friday Mix", user: true, provider: "local", artworkPath: null, trackIds: [1, 2, 3, 4] } ];` (keep whatever else Phase 2 put there) and in `create_playlist` push `{ id, name, user: true, provider: "local", artworkPath: null, trackIds }`. Next to `appleTracks` add:

```ts
const applePlaylists: typeof playlists = [{ id: 301, name: "Late Night Drive", user: false, provider: "apple", artworkPath: "https://example.com/made-up/late-night-drive/600x600bb.jpg", trackIds: [201, 202] }];
```

Replace `collections` with:

```ts
function collections(kind: Kind, provider: Source | null = null): { card: CollectionCard; tracks: Track[] }[] {
  const make = (id: number, name: string, list: Track[], user: boolean, source: Source, subtitle: string | null = null, artworkPath: string | null = null) => ({
    card: { id, provider: source, kind, name, subtitle, user, artworkPath, count: list.length, covers: list.slice(0, 4), providers: list.length ? [...new Set(list.map((t) => t.provider))] : [source] },
    tracks: list,
  });
  if (kind === "playlist") {
    return [...playlists, ...(apple.account ? applePlaylists : [])]
      .filter((p) => !provider || p.provider === provider)
      .map((p) => make(p.id, p.name, p.trackIds.map((id) => tracks.get(id)!), p.user, p.provider, null, p.artworkPath));
  }
  const groups = new Map<string, Track[]>();
  for (const t of tracks.values()) {
    const name = kind === "album" ? t.album : t.artist;
    if (name && (!provider || t.provider === provider)) groups.set(name, [...(groups.get(name) ?? []), t]);
  }
  return [...groups].map(([name, list]) => make(groupId(kind, name), name, list, false, list[0].provider, kind === "album" ? list[0].artist : null));
}
```

and `list_collections: ({ kind, provider }) => collections(kind, provider ?? null).map((c) => c.card),`. `allCollections` must not hand `flatMap`'s index to `collections` as the provider:

```ts
const allCollections = () => (["playlist", "album", "artist"] as const).flatMap((k) => collections(k));
```

- [ ] **Step 3: Library state**

`library.svelte.ts`:

```ts
export type Provider = "all" | "local" | "apple";

/** The name to show for a collection: Local's app-made Imported playlist in the current language, others as stored. */
export const cardName = (c: CollectionCard) => (c.provider === "local" && c.kind === "playlist" && !c.user ? t("library.imported") : c.name);
```

In the class: add `provider = $state<Provider>("all");` and

```ts
  async setProvider(provider: Provider) {
    this.provider = provider;
    this.selected = null;
    await this.refresh();
  }
```

`show(kind, id)` sets `this.provider = "all";` first (its doc: `/** Opens a collection of \`kind\` in the sidebar, under All. */`). In `refresh()`:

```ts
    const [kind, provider] = [this.kind, this.provider];
    try {
      const cards = await listCollections(kind, provider === "all" ? null : provider);
      if (this.kind !== kind || this.provider !== provider) return;
```

(and `if (this.kind === kind && this.provider === provider) this.error = e;` in the catch). In `open()`: `await openCollection(id, this.provider === "all")`.

`adding.svelte.ts` `reveal()`:

```ts
    library.provider = "all";
    await library.setKind("playlist");
    const imported = library.cards.find((c) => c.provider === "local" && c.kind === "playlist" && !c.user);
```

`sources.svelte.ts` `apply()`, before the refresh line: `if (!s.account && library.provider === "apple") library.provider = "all";`

- [ ] **Step 4: Logos and the switcher**

Create `app/src/lib/components/ProviderLogos.svelte`:

```svelte
<script lang="ts">
  import type { Track } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import AppleLogoIcon from "phosphor-svelte/lib/AppleLogoIcon";
  import FolderIcon from "phosphor-svelte/lib/FolderIcon";

  let { providers }: { providers: Track["provider"][] } = $props();
</script>

<span class="logos">
  {#each providers as p (p)}
    {#if p === "apple"}<span use:tip={t("sources.apple")}><AppleLogoIcon size={12} /></span>{:else if p === "local"}<span use:tip={t("provider.local")}><FolderIcon size={12} /></span>{/if}
  {/each}
</span>

<style>
  .logos { display: inline-flex; gap: 4px; vertical-align: -1px; color: var(--faint); flex: none; }
  .logos span { display: inline-flex; }
</style>
```

In `Sidebar.svelte` import `sources`, `ProviderLogos`, `type Provider` and the icons `SquaresFourIcon`, `FolderIcon`, `AppleLogoIcon`, and add:

```ts
  const providers: { id: Provider; label: Key; icon: Icon }[] = [
    { id: "all", label: "provider.all", icon: SquaresFourIcon },
    { id: "local", label: "provider.local", icon: FolderIcon },
    { id: "apple", label: "sources.apple", icon: AppleLogoIcon },
  ];
```

Above the kind tabs:

```svelte
  {#if sources.connected}
    <div class="seg" role="toolbar" aria-label={t("library.source")} transition:slide={{ y: -4 }}>
      {#each providers as p (p.id)}
        <button aria-pressed={library.provider === p.id} use:tip={t(p.label)} onclick={() => library.setProvider(p.id)}><p.icon size={18} /></button>
      {/each}
    </div>
  {/if}
```

The New playlist row shows only when `library.kind === "playlist" && library.provider !== "apple"`. After a card's name add `{#if library.provider === "all" && sources.connected}<ProviderLogos providers={c.providers} />{/if}`. Replace the empty note with:

```svelte
      {#if library.provider === "apple" && sources.apple.refreshing}
        <div class="note" in:fade><span class="spin"></span>{t("sources.gettingLibrary")}</div>
      {:else if library.loaded}
        <div class="note" in:fade><TrayIcon size={16} />{t("library.nothingHere")}</div>
      {/if}
```

Styles (from the prototype's `.seg`):

```css
  .seg { display: grid; grid-template-columns: repeat(3, 1fr); gap: 2px; padding: 3px; margin-bottom: var(--s3); border-radius: var(--r-sm); background: var(--raised); }
  .seg button { height: 30px; display: grid; place-items: center; border-radius: calc(var(--r-sm) - 3px); color: var(--muted); }
  .seg button:hover { color: var(--text); }
  .seg button[aria-pressed="true"] { background: var(--surface); color: var(--accent); }
```

`CollectionView.svelte`: import `sources`, `ProviderLogos`; in the hero caption after the kind label add `{#if library.provider === "all" && sources.connected}<ProviderLogos providers={page.card.providers} />{/if}`; pass `source={page.card.user && sources.connected}` to `SongRow`.

`SongRow.svelte`: add prop `source = false` (typed `source?: boolean`) and import `ProviderLogos`; the artist line becomes:

```svelte
    <div class="a ell">{#if source && track.provider !== "local"}<ProviderLogos providers={[track.provider]} />{" "}{/if}{[track.artist, track.album].filter(Boolean).join(" – ")}</div>
```

`+page.svelte`: the empty-library message shows only when `library.loaded && library.kind === "playlist" && library.provider !== "apple"`.

`Cover.svelte` — a collection's own artwork first:

```svelte
<script lang="ts">
  import type { Track } from "$lib/api";
  import Artwork from "./Artwork.svelte";

  let { covers, size, grid = false, round = false, artwork = null }: { covers: Track[]; size: number | string; grid?: boolean; round?: boolean; artwork?: string | null } = $props();
  const px = $derived(typeof size === "number" ? `${size}px` : size);
</script>

{#if artwork}
  <Artwork track={{ artSeed: 0, artworkPath: artwork }} {size} {round} />
{:else if grid && covers.length >= 4}
  <span class="grid" style:width={px} style:height={px}>{#each covers.slice(0, 4) as t (t.id)}<Artwork track={t} size="100%" square />{/each}</span>
{:else}
  <Artwork track={covers[0] ?? null} {size} {round} />
{/if}
```

(keep its `<style>`). Pass `artwork={c.artworkPath}` / `artwork={page.card.artworkPath}` to the `Cover`s in `Sidebar.svelte`, `CollectionView.svelte` and `SearchResults.svelte`.

- [ ] **Step 5: Text in six languages**

Append:
- `en.ts`: `"provider.all": "All",` `"provider.local": "Local",` `"library.source": "Source",` `"sources.gettingLibrary": "Getting your Apple Music library…",`
- `ja.ts`: `"provider.all": "すべて",` `"provider.local": "ローカル",` `"library.source": "ソース",` `"sources.gettingLibrary": "Apple Music のライブラリを取得しています…",`
- `ko.ts`: `"provider.all": "전체",` `"provider.local": "로컬",` `"library.source": "소스",` `"sources.gettingLibrary": "Apple Music 보관함을 가져오는 중…",`
- `zh-Hans.ts`: `"provider.all": "全部",` `"provider.local": "本地",` `"library.source": "来源",` `"sources.gettingLibrary": "正在获取你的 Apple Music 资料库…",`
- `zh-Hant.ts`: `"provider.all": "全部",` `"provider.local": "本機",` `"library.source": "來源",` `"sources.gettingLibrary": "正在取得你的 Apple Music 資料庫…",`
- `es.ts`: `"provider.all": "Todo",` `"provider.local": "Local",` `"library.source": "Fuente",` `"sources.gettingLibrary": "Obteniendo tu biblioteca de Apple Music…",`

- [ ] **Step 6: Run the checks**

Run: `cd app && npm run check && npm test && npm run check:i18n && npx playwright test`
Expected: the whole suite passes (the existing tests still find "Imported", open collections and reveal added songs).

- [ ] **Step 7: Commit**

```bash
git add app/src app/tests
git commit -m "feat(app): provider switcher — All, Local and Apple Music, with source logos"
```

---

### Task 12: The Change match sheet

**Files:**
- Create: `app/src/lib/components/MatchSheet.svelte`
- Modify: `app/src/lib/api.ts`, `app/src/lib/state/ui.svelte.ts`, `app/src/lib/state/player.svelte.ts`, `app/src/lib/components/SongMenu.svelte`, `MoreMenu.svelte`, `app/src/routes/+page.svelte`, `app/src/lib/i18n/*.ts` (read `ui.svelte.ts`, `player.svelte.ts` and `+page.svelte` first: Phase 2 changed them)
- Modify: `app/tests/fake-backend.ts`, `app/tests/apple-music.spec.ts`

**Interfaces:**
- Consumes: commands `match_candidates`, `choose_match`, `choose_match_link` (Task 9); `Sheet`, `duration`, `toasts`, `say`.
- Produces:
  - `api.ts`: `interface MatchCandidate { sourceId: number; url: string; title: string; channel: string | null; durationMs: number | null; thumbnail: string | null; inUse: boolean }`, `matchCandidates(trackId)`, `chooseMatch(trackId, sourceId)`, `chooseMatchLink(trackId, url): Promise<string>`.
  - `SheetState` gains `{ kind: "match"; track: Track }`.
  - Song menu › "Change match…" and the "…" menu's Version row for streaming songs; a "No singable version found" toast offers Change match… instead of Try again.

- [ ] **Step 1: Write the failing tests**

Append to `app/tests/apple-music.spec.ts` (add `row, sing` to the import from `./app`):

```ts
const openLateNightDrive = async (page: Page) => {
  await page.evaluate(() => window.fake.appleConnected());
  await page.locator("aside").getByRole("button", { name: /Late Night Drive/ }).click();
};

const changeMatch = async (page: Page, title: string) => {
  await row(page, title).click({ button: "right" });
  await page.getByRole("menu").getByRole("button", { name: "Change match…" }).click();
  return page.getByRole("dialog", { name: "Change match" });
};

test("Change match lists the versions found, marks the one in use and switches to another", async ({ page }) => {
  await openLateNightDrive(page);
  const sheet = await changeMatch(page, "Neon Tidewater");
  await expect(sheet.locator(".cand")).toHaveText([/Neon Tidewater.*Juniper Row - Topic.*In use/, /Official Video.*Harbor Records/]);
  await sheet.getByRole("button", { name: /Official Video/ }).click();
  expect(await calls(page, "choose_match")).toEqual([{ trackId: 201, sourceId: 2 }]);
  await expect(page.locator(".toasts")).toContainText("Now singing from “Juniper Row - Neon Tidewater (Official Video)”");
  await expect(sheet).toBeHidden();
});

test("a pasted link becomes the version in use", async ({ page }) => {
  await openLateNightDrive(page);
  const sheet = await changeMatch(page, "Neon Tidewater");
  await sheet.getByRole("textbox", { name: "Paste a link to a video or a song file" }).fill("https://youtu.be/zzzzzzzzzzz");
  await sheet.getByRole("button", { name: "Use" }).click();
  expect(await calls(page, "choose_match_link")).toEqual([{ trackId: 201, url: "https://youtu.be/zzzzzzzzzzz" }]);
  await expect(page.locator(".toasts")).toContainText("Now singing from “Pasted Made-up Clip”");
});

test("a song with no version to sing offers Change match right away", async ({ page }) => {
  await openLateNightDrive(page);
  await sing(page, "Neon Tidewater");
  await page.evaluate(() => window.fake.engine({ kind: "failed", trackId: 201, message: "", problem: "noMatch" }));
  const toast = page.locator(".toasts");
  await expect(toast).toContainText("No singable version found.");
  await toast.getByRole("button", { name: "Change match…" }).click();
  await expect(page.getByRole("dialog", { name: "Change match" })).toBeVisible();
});
```

Run: `cd app && npx playwright test apple-music`
Expected: the three new tests FAIL (no "Change match…" in the menu).

- [ ] **Step 2: The fake backend**

Add `MatchCandidate` to the type import and:

```ts
const candidates = new Map<number, MatchCandidate[]>();
```

```ts
  match_candidates: ({ trackId }) => {
    if (!candidates.has(trackId)) {
      candidates.set(trackId, [
        { sourceId: 1, url: "https://www.youtube.com/watch?v=aaaaaaaaaaa", title: "Neon Tidewater", channel: "Juniper Row - Topic", durationMs: 214_000, thumbnail: null, inUse: true },
        { sourceId: 2, url: "https://www.youtube.com/watch?v=bbbbbbbbbbb", title: "Juniper Row - Neon Tidewater (Official Video)", channel: "Harbor Records", durationMs: 262_000, thumbnail: null, inUse: false },
      ]);
    }
    return candidates.get(trackId);
  },
  choose_match: ({ trackId, sourceId }) => {
    for (const c of candidates.get(trackId) ?? []) c.inUse = c.sourceId === sourceId;
  },
  choose_match_link: () => "Pasted Made-up Clip",
```

- [ ] **Step 3: API and state**

`api.ts`:

```ts
export interface MatchCandidate { sourceId: number; url: string; title: string; channel: string | null; durationMs: number | null; thumbnail: string | null; inUse: boolean }

export const matchCandidates = (trackId: number) => invoke<MatchCandidate[]>("match_candidates", { trackId });
export const chooseMatch = (trackId: number, sourceId: number) => invoke<void>("choose_match", { trackId, sourceId });
export const chooseMatchLink = (trackId: number, url: string) => invoke<string>("choose_match_link", { trackId, url });
```

`ui.svelte.ts`: add `| { kind: "match"; track: Track }` to `SheetState` (keep Phase 2's `"mics"`).

`player.svelte.ts` (import `SwapIcon`, `type AppError`):

```ts
  /** Tells the user the song can't play right now, with a way on: Change match when no version fits, else preparing it again. */
  private showProblem(e: unknown) {
    const track = this.track;
    const action =
      (e as Partial<AppError> | null)?.problem === "noMatch" && track
        ? { label: t("menu.changeMatch"), icon: SwapIcon, run: () => (ui.sheet = { kind: "match", track }) }
        : { label: t("common.tryAgain"), icon: ArrowClockwiseIcon, run: () => void retryPrepare() };
    toasts.show(say(e), { icon: WarningIcon, action });
  }
```

- [ ] **Step 4: The sheet**

Create `app/src/lib/components/MatchSheet.svelte`:

```svelte
<script lang="ts">
  import { chooseMatch, chooseMatchLink, matchCandidates, type MatchCandidate } from "$lib/api";
  import { say } from "$lib/i18n/engine";
  import { t } from "$lib/i18n/index.svelte";
  import { duration } from "$lib/format";
  import { fade } from "$lib/motion";
  import { ui } from "$lib/state/ui.svelte";
  import { toasts } from "$lib/state/toasts.svelte";
  import Sheet from "./Sheet.svelte";
  import SwapIcon from "phosphor-svelte/lib/SwapIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import LinkIcon from "phosphor-svelte/lib/LinkIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  const track = $derived(ui.sheet?.kind === "match" ? ui.sheet.track : null);
  let candidates = $state<MatchCandidate[] | null>(null);
  let link = $state("");
  let busy = $state(false);
  const failed = (e: unknown) => void toasts.show(say(e), { icon: WarningIcon });

  $effect(() => {
    if (!track) return;
    const id = track.id;
    candidates = null;
    link = "";
    matchCandidates(id).then(
      (found) => {
        if (track?.id === id) candidates = found;
      },
      (e) => {
        if (track?.id === id) candidates = [];
        failed(e);
      },
    );
  });

  /** Puts a version in use (`pick` returns its title), says so and closes. */
  async function use(pick: () => Promise<string>) {
    busy = true;
    try {
      const title = await pick();
      toasts.show(t("match.changed", { title }), { icon: SwapIcon });
      ui.sheet = null;
    } catch (e) {
      failed(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if track}
  <Sheet icon={SwapIcon} title={t("match.title")} onClose={() => (ui.sheet = null)}>
    <p class="song"><b class="ell">{track.title}</b><span class="muted ell">{track.artist ?? ""}</span></p>
    <div class="list">
      {#if candidates === null}
        {#each [0, 1, 2] as i (i)}<div class="cand"><span class="thumb"><i class="bone"></i></span><span class="grow"><i class="bone" style="width:60%"></i><i class="bone" style="width:35%"></i></span></div>{/each}
      {:else}
        {#each candidates as c (c.sourceId)}
          <button class="cand" disabled={busy || c.inUse} onclick={() => use(() => chooseMatch(track.id, c.sourceId).then(() => c.title))} in:fade>
            <span class="thumb" style:background-image={c.thumbnail ? `url("${c.thumbnail}")` : null}>{#if c.durationMs}<span class="dur">{duration(c.durationMs)}</span>{/if}</span>
            <span class="grow"><b class="ell">{c.title}</b><span class="muted ell">{c.channel ?? ""}</span></span>
            {#if c.inUse}<span class="tag"><CheckIcon size={14} />{t("match.inUse")}</span>{/if}
          </button>
        {:else}
          <p class="muted">{t("match.none")}</p>
        {/each}
      {/if}
    </div>
    <form class="hstack paste" onsubmit={(e) => { e.preventDefault(); const url = link.trim(); if (url) use(() => chooseMatchLink(track.id, url)); }}>
      <LinkIcon size={18} />
      <input bind:value={link} placeholder={t("match.paste")} aria-label={t("match.paste")} spellcheck="false" />
      <button class="btn accent" disabled={busy || !link.trim()}>{t("match.use")}</button>
    </form>
  </Sheet>
{/if}

<style>
  .song { display: grid; margin: calc(-1 * var(--s3)) 0 var(--s4); }
  .song b { font-weight: 600; }
  .list { display: grid; gap: var(--s1); margin-bottom: var(--s4); }
  .cand { display: flex; align-items: center; gap: var(--s3); padding: var(--s2); border-radius: var(--r-sm); text-align: left; }
  .cand:not(:disabled):hover { background: color-mix(in srgb, var(--text) 5%, transparent); }
  .cand:disabled { opacity: 1; cursor: default; }
  .cand .grow { display: grid; min-width: 0; }
  .cand b { font-weight: 500; }
  .thumb { position: relative; width: 96px; aspect-ratio: 16 / 9; flex: none; border-radius: 6px; background: var(--raised) center / cover no-repeat; overflow: hidden; }
  .dur { position: absolute; right: 4px; bottom: 4px; padding: 0 4px; border-radius: 4px; font-size: 11px; background: color-mix(in srgb, #000 60%, transparent); color: #fff; }
  .tag { display: flex; align-items: center; gap: 4px; font-size: 12.5px; color: var(--accent); flex: none; }
  .paste { gap: var(--s2); }
  .paste > :global(svg) { color: var(--muted); flex: none; }
  .paste input { flex: 1; min-width: 0; height: 34px; padding: 0 var(--s3); border-radius: var(--r-sm); border: 1px solid var(--line); background: var(--bg); color: var(--text); font: inherit; }
</style>
```

Add `<MatchSheet />` to `+page.svelte` next to `<EditSheet />`. The `.bone` class already exists (used by `LinkRow`); `#fff`/`#000` in `.dur` mirror `LinkRow`'s duration badge — if `LinkRow` uses tokens there, use the same ones.

- [ ] **Step 5: The menus**

`SongMenu.svelte` (import `SwapIcon`), after the "Find lyrics again" option:

```svelte
      {#if track.provider !== "local"}
        <button class="opt" onpointerenter={hideSub} onclick={() => run(() => (ui.sheet = { kind: "match", track }))}><SwapIcon size={18} /><span class="grow">{t("menu.changeMatch")}</span></button>
      {/if}
```

`MoreMenu.svelte` (import `SwapIcon`), after the Lyrics row:

```svelte
  {#if player.track && player.track.provider !== "local"}
    {@const track = player.track}
    <div class="mrow">
      <SwapIcon size={18} /><span class="grow">{t("menu.version")}</span>
      <button class="btn soft pill" onclick={() => { ui.moreOpen = false; ui.sheet = { kind: "match", track }; }}>{t("match.change")}</button>
    </div>
  {/if}
```

- [ ] **Step 6: Text in six languages**

Append:
- `en.ts`: `"menu.changeMatch": "Change match…",` `"menu.version": "Version",` `"match.title": "Change match",` `"match.change": "Change",` `"match.inUse": "In use",` `"match.none": "No versions found. Paste a link instead.",` `"match.paste": "Paste a link to a video or a song file",` `"match.use": "Use",` `"match.changed": "Now singing from “{title}”",`
- `ja.ts`: `"menu.changeMatch": "バージョンを変更…",` `"menu.version": "バージョン",` `"match.title": "バージョンを変更",` `"match.change": "変更",` `"match.inUse": "使用中",` `"match.none": "バージョンが見つかりませんでした。代わりにリンクを貼り付けてください。",` `"match.paste": "動画か曲ファイルのリンクを貼り付け",` `"match.use": "使う",` `"match.changed": "「{title}」で歌います",`
- `ko.ts`: `"menu.changeMatch": "버전 변경…",` `"menu.version": "버전",` `"match.title": "버전 변경",` `"match.change": "변경",` `"match.inUse": "사용 중",` `"match.none": "버전을 찾지 못했습니다. 대신 링크를 붙여 넣으세요.",` `"match.paste": "동영상이나 노래 파일 링크 붙여 넣기",` `"match.use": "사용",` `"match.changed": "이제 “{title}”(으)로 부릅니다",`
- `zh-Hans.ts`: `"menu.changeMatch": "更换版本…",` `"menu.version": "版本",` `"match.title": "更换版本",` `"match.change": "更换",` `"match.inUse": "正在使用",` `"match.none": "找不到任何版本。请改为粘贴链接。",` `"match.paste": "粘贴视频或歌曲文件的链接",` `"match.use": "使用",` `"match.changed": "现在使用“{title}”演唱",`
- `zh-Hant.ts`: `"menu.changeMatch": "更換版本…",` `"menu.version": "版本",` `"match.title": "更換版本",` `"match.change": "更換",` `"match.inUse": "使用中",` `"match.none": "找不到任何版本。請改為貼上連結。",` `"match.paste": "貼上影片或歌曲檔案的連結",` `"match.use": "使用",` `"match.changed": "現在使用「{title}」演唱",`
- `es.ts`: `"menu.changeMatch": "Cambiar versión…",` `"menu.version": "Versión",` `"match.title": "Cambiar versión",` `"match.change": "Cambiar",` `"match.inUse": "En uso",` `"match.none": "No se encontraron versiones. Pega un enlace en su lugar.",` `"match.paste": "Pega un enlace a un video o a un archivo de canción",` `"match.use": "Usar",` `"match.changed": "Ahora cantas con «{title}»",`

- [ ] **Step 7: Run the checks**

Run: `cd app && npm run check && npm test && npm run check:i18n && npx playwright test apple-music song-menu player`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add app/src app/tests
git commit -m "feat(app): Change match sheet — pick another version or paste a link"
```

---

### Task 13: Animated covers

**Files:**
- Create: `app/src/lib/components/MotionArt.svelte`
- Modify: `app/src/lib/api.ts`, `app/src/lib/components/CollectionView.svelte`, `KaraokeBackground.svelte`
- Modify: `app/tests/fake-backend.ts`, `app/tests/apple-music.spec.ts`

**Interfaces:**
- Consumes: commands `track_motion`, `collection_motion` (Task 9); `artworkUrl` (Task 8).
- Produces:
  - `api.ts`: `trackMotion(trackId): Promise<string | null>`, `collectionMotion(collectionId): Promise<string | null>`.
  - `MotionArt.svelte` `{ src: string | null }` — a muted looping `<video>` over whatever is under it, shown only once it plays; not there at all under Reduce motion (follows the setting live).

- [ ] **Step 1: Write the failing test**

Append to `app/tests/apple-music.spec.ts`:

```ts
test("an Apple Music album's animated cover sits over its still cover, the karaoke view has one too, and Reduce motion removes them", async ({ page }) => {
  await page.evaluate(() => window.fake.appleConnected());
  await page.getByRole("button", { name: "Albums", exact: true }).click();
  await page.locator("aside").getByRole("button", { name: /Night Line/ }).click();
  const video = page.locator(".hero video");
  await expect(video).toHaveAttribute("src", "/tests/made-up-motion.m3u8");
  await expect(video).toHaveCSS("opacity", "0");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await expect(video).toHaveCount(0);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await sing(page, "Neon Tidewater");
  await expect(page.locator(".kbg video")).toHaveAttribute("src", "/tests/made-up-motion.m3u8");
});
```

(The made-up file doesn't exist, so the video never plays and the still cover stays visible — the fallback path. Real HLS playback is checked on the user's checklist.)

Run: `cd app && npx playwright test apple-music`
Expected: the new test FAILS (no video in the hero).

- [ ] **Step 2: Fake backend and API**

Fake `commands`:

```ts
  collection_motion: ({ collectionId }) => (allCollections().find((c) => c.card.id === collectionId)?.card.provider === "apple" ? "/tests/made-up-motion.m3u8" : null),
  track_motion: ({ trackId }) => (tracks.get(trackId)?.provider === "apple" ? "/tests/made-up-motion.m3u8" : null),
```

`api.ts`:

```ts
export const trackMotion = (trackId: number) => invoke<string | null>("track_motion", { trackId });
export const collectionMotion = (collectionId: number) => invoke<string | null>("collection_motion", { collectionId });
```

- [ ] **Step 3: `MotionArt`**

Create `app/src/lib/components/MotionArt.svelte`:

```svelte
<script lang="ts">
  let { src }: { src: string | null } = $props();
  const query = matchMedia("(prefers-reduced-motion: reduce)");
  let still = $state(query.matches);
  let playing = $state(false);

  $effect(() => {
    const follow = () => (still = query.matches);
    query.addEventListener("change", follow);
    return () => query.removeEventListener("change", follow);
  });

  $effect(() => {
    void src;
    playing = false;
  });
</script>

{#if src && !still}
  <video class="motion" class:playing {src} autoplay muted loop playsinline aria-hidden="true" onplaying={() => (playing = true)}></video>
{/if}

<style>
  .motion { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: 0; transition: opacity var(--t) var(--ease); pointer-events: none; }
  .playing { opacity: 1; }
</style>
```

Probe once in the isolated check's picture or with `npm run check` that Svelte sets `muted` as a property (WebKit only autoplays muted video); if it doesn't, bind it: `bind:muted={alwaysMuted}` with `const alwaysMuted = true`.

- [ ] **Step 4: Collection header and karaoke background**

`CollectionView.svelte` (import `collectionMotion`, `MotionArt`):

```ts
  let motion = $state<string | null>(null);
  $effect(() => {
    const card = page?.card;
    motion = null;
    if (!card || card.provider !== "apple") return;
    collectionMotion(card.id).then((m) => {
      if (library.page?.card.id === card.id) motion = m;
    }, () => {});
  });
```

Put `<MotionArt src={motion} />` inside the cover `<div class="cover" …>` after `<Cover …/>` (Task 11 gave it `artwork`), and add `position: relative;` to `.cover`.

`KaraokeBackground.svelte`:

```svelte
<script lang="ts">
  import { trackMotion, type Track } from "$lib/api";
  import { artworkUrl, hues } from "$lib/art";
  import MotionArt from "./MotionArt.svelte";

  let { track }: { track: Track } = $props();
  const h = $derived(hues(track.artSeed));
  const image = $derived(track.artworkPath ? `url("${artworkUrl(track.artworkPath)}")` : null);
  let motion = $state<string | null>(null);

  $effect(() => {
    const id = track.id;
    motion = null;
    if (track.provider !== "apple") return;
    trackMotion(id).then((m) => {
      if (track.id === id) motion = m;
    }, () => {});
  });
</script>

<div class="kbg" style:--h0={h[0]} style:--h1={h[1]} style:--h2={h[2]}>
  {#each [0, 1, 2] as i (i)}<i class:img={!!image} style:background-image={image}></i>{/each}
  <MotionArt src={motion} />
</div>
```

(keep its existing `<style>`; the karaoke view's `.shade` above it keeps the lyrics readable).

- [ ] **Step 5: Run the checks**

Run: `cd app && npm run check && npm test && npx playwright test apple-music karaoke library`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add app/src app/tests
git commit -m "feat(app): animated covers on Apple Music collections and the karaoke background"
```

---

### Task 14: Apple Music in search

**Files:**
- Modify: `app/src/lib/api.ts`, `app/src/lib/search.ts`, `app/src/lib/search.test.ts`, `app/src/lib/state/sources.svelte.ts`
- Modify: `app/src/lib/components/SearchBar.svelte`, `SearchResults.svelte`
- Modify: `app/tests/fake-backend.ts`, `app/tests/apple-music.spec.ts`

**Interfaces:**
- Consumes: commands `apple_search`, `apple_suggestions` (Task 9); `sources.searchable` (Task 10); `PHRASE_SOURCES` (Phase 1b Task 32).
- Produces:
  - `api.ts`: `appleSearch(query): Promise<Track[]>`, `appleSuggestions(query): Promise<string[]>`.
  - `SearchView` text gains `apple: Track[] | null` (null while looking; `[]` when Apple Music can't search); `fromOutcome(prev, input, o, apple = false)`; `withApple(view, query, tracks)`.
  - Search results show an "Apple Music" section (song rows that queue like any song) between your collections and YouTube; Apple phrases follow YouTube's in the suggestions.

- [ ] **Step 1: Write the failing tests**

In `app/src/lib/search.test.ts` import `withApple` and `type Track`, and add:

```ts
it("apple music results come only when it can search, and fill only the words they were found for", () => {
  const found: Track = { id: 203, provider: "apple", title: "Lemon Tree Radio", artist: null, album: null, durationMs: null, vocalRemoval: 100, keySemitones: 0, instrumental: false, artworkPath: null, artSeed: 1 };
  expect(fromOutcome({ kind: "none" }, "lemon", text)).toMatchObject({ apple: [] });
  let v = fromOutcome({ kind: "none" }, "lemon", text, true);
  expect(v).toMatchObject({ apple: null });
  expect(withApple(v, "lem", [found])).toEqual(v);
  v = withApple(v, "lemon", [found]);
  expect(fromOutcome(v, " lemon ", text, true)).toMatchObject({ apple: [found] });
});
```

Append to `app/tests/apple-music.spec.ts`:

```ts
test("once connected, Apple Music phrases and songs show in search, and its songs queue like any other", async ({ page }) => {
  await page.evaluate(() => window.fake.appleConnected());
  const box = page.getByRole("combobox", { name: "Search anything, or paste a link" });
  await box.pressSequentially("lemon");
  await expect(page.getByRole("listbox", { name: "Suggestions" }).getByRole("option")).toHaveText(["Lemon Skies", "lemon karaoke", "lemon live", "lemon apple"]);
  await box.press("Escape");
  await expect(page.getByRole("main").getByRole("heading", { level: 2 })).toHaveText(["Songs", "Apple Music", "YouTube"]);
  await page.locator("main .row", { hasText: "Lemon Tree Radio" }).click();
  expect(await calls(page, "queue_add")).toEqual([{ trackId: 203, next: false }]);
});
```

Run: `cd app && npx vitest run src/lib/search.test.ts && npx playwright test apple-music`
Expected: FAIL — `withApple` isn't exported; no Apple section.

- [ ] **Step 2: The fake backend**

```ts
/** Songs only found by an Apple Music search: not in the library's own search or collections, as in the real backend. */
const searchOnly = new Set<number>();
```

```ts
  apple_suggestions: ({ query }) => (apple.account ? [`${query} apple`] : []),
  apple_search: () => {
    const found = appleSong(203, "Lemon Tree Radio", "Odd Hours Club", "Odd Hours", 190_000, 30);
    tracks.set(found.id, found);
    searchOnly.add(found.id);
    return [found];
  },
```

In the fake `search()`, iterate `[...tracks.values()].filter((t) => !searchOnly.has(t.id))`; in `collections()`'s album/artist loop skip `searchOnly` ids too.

- [ ] **Step 3: API, the view and phrases**

`api.ts`:

```ts
export const appleSearch = (query: string) => invoke<Track[]>("apple_search", { query });
export const appleSuggestions = (query: string) => invoke<string[]>("apple_suggestions", { query });
```

`search.ts` — the text view carries `apple: Track[] | null`, and:

```ts
/** The view for a search result; the same link keeps the preview it already has, the same words their YouTube and
 * Apple Music results (null while still to find; Apple Music only when `apple` can search). */
export function fromOutcome(prev: SearchView, input: string, o: SearchOutcome, apple = false): SearchView {
  if (o.kind === "link") return prev.kind === "link" && prev.url === o.url ? prev : { kind: "link", url: o.url, host: o.host, preview: null, failed: false };
  if (o.kind === "rejected") return { kind: "rejected", streaming: o.streaming, host: o.host };
  const query = input.trim();
  if (!query) return { kind: "none" };
  const kept = prev.kind === "text" && prev.query === query ? prev : null;
  const online = query.length >= 2 ? null : [];
  return { kind: "text", query, tracks: o.tracks, collections: o.collections, youtube: kept ? kept.youtube : online, apple: !apple ? [] : kept ? kept.apple : online };
}

/** Adds the Apple Music songs found for `query`; ones for any other words are dropped. */
export function withApple(view: SearchView, query: string, apple: Track[]): SearchView {
  return view.kind === "text" && view.query === query ? { ...view, apple } : view;
}
```

`sources.svelte.ts` (import `appleSuggestions` from `$lib/api` and `PHRASE_SOURCES` from `$lib/suggest`): at the start of `init()` add Apple Music's phrases after YouTube's, asked only while Apple Music can search:

```ts
    PHRASE_SOURCES.push((query) => (this.searchable ? appleSuggestions(query) : Promise.resolve([])));
```

- [ ] **Step 4: Search bar and results**

`SearchBar.svelte`: rename `youtubeTimer` / `youtubeAsked` / `findOnYoutube` to `onlineTimer` / `onlineAsked` / `findOnline` everywhere in the file; import `appleSearch`, `withApple` and `sources`. In `run()`:

```ts
    ui.search = fromOutcome(before, value, outcome, sources.searchable);
```

```ts
    if (ui.search.kind === "text" && (ui.search.youtube === null || ui.search.apple === null) && ui.search.query !== onlineAsked) {
      clearTimeout(onlineTimer);
      onlineTimer = setTimeout(findOnline, 280);
    }
```

and replace the YouTube lookup with:

```ts
  /** Looks the words up on YouTube and Apple Music (whichever is still to find), filling each in as it answers. */
  async function findOnline() {
    if (ui.search.kind !== "text") return;
    const { query, youtube, apple } = ui.search;
    onlineAsked = query;
    await Promise.all([
      youtube === null && youtubeSearch(query).catch(() => []).then((hits) => (ui.search = withYoutube(ui.search, query, hits))),
      apple === null && appleSearch(query).catch(() => []).then((found) => (ui.search = withApple(ui.search, query, found))),
    ]);
    if (onlineAsked === query) onlineAsked = null;
  }
```

`SearchResults.svelte` (import `AppleLogoIcon`):

```ts
  const appleSongs = $derived(view?.apple ? view.apple.filter((x) => !tracks.some((y) => y.id === x.id)) : []);
```

The nothing-matches condition also needs `view.apple?.length === 0`. Before the YouTube section add:

```svelte
  {#if appleSongs.length}
    <section transition:fade>
      <h2 class="sec"><AppleLogoIcon size={18} />{t("sources.apple")}</h2>
      {#each appleSongs as song (song.id)}
        <SongRow track={song} playing={player.track?.id === song.id} onTap={() => player.enqueue(song.id)} onNext={() => player.enqueue(song.id, true)} onMenu={(x, y, alignRight) => (ui.menu = { kind: "song", track: song, playlistId: null, x, y, alignRight: !!alignRight })} />
      {/each}
    </section>
  {/if}
```

- [ ] **Step 5: Run the checks**

Run: `cd app && npm run check && npm test && npx playwright test`
Expected: the whole suite passes (the existing search tests still see only local and YouTube while Apple Music isn't connected).

- [ ] **Step 6: Commit**

```bash
git add app/src app/tests
git commit -m "feat(app): Apple Music songs and phrases in search"
```

---

### Task 15: README, full check and the user's checklist (on a copy of the library)

**Files:**
- Modify: `README.md` (read it first: Phase 2 added its phone section and the isolated check)

- [ ] **Step 1: README**

After the `## Use` section add:

```markdown
## Apple Music

Settings › Sources › Connect opens Apple Music's own login page in a separate window. After you log in, your
playlists (including the ones you follow and Apple's own), albums and their artists appear under Apple Music in
the sidebar. The app refreshes them when it starts (at most every six hours) and when you press Refresh. It only
reads your library — it never changes it and never plays Apple Music audio: when you sing one of its songs, the
app finds the same song on YouTube (the official audio when there is one) and prepares that. Song menu ›
Change match… picks another version or takes a pasted link. Animated album covers play where Apple has them.
When the login runs out, Settings › Sources says "Log in to Apple Music again".

The app uses the same web service and login as music.apple.com's own player (as Cider did). Apple doesn't offer
this to other apps and it is against Apple Music's terms; this is a personal, non-commercial project, and each
person uses their own account. The login is kept in your Mac's Keychain (item `world.aako.kara-always-oki`, one
for every data folder) and is only ever sent to Apple. Disconnect removes it along with Apple's playlists and
albums; songs you've sung stay.
```

In `## Development` add:

```markdown
`KARA_APPLE_LOGIN_CHECK=1` (debug builds) opens the Apple Music login window at launch and prints
`apple login check: the page is ready` once Apple's player has loaded, without logging in. With your account
connected, `cargo test -p kara-core live_library_has_what_sync_reads -- --ignored --nocapture` prints how many
playlists, albums and songs the app can read (counts only).
```

- [ ] **Step 2: Full check**

Run:
```bash
source "$HOME/.cargo/env" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cd app && npm run check && npm test && npm run check:i18n && npx playwright test
```
Expected: everything passes, 0 warnings (Phase 2's tests included).

- [ ] **Step 3: The isolated checks**

Run (Bash timeout 10 minutes each):
```bash
cd app && zsh scripts/isolated-check.sh ../.superpowers/sdd/2026-09-27-phase3-apple-music/shots/task-15.png "sleep 5"
```
Expected: `OK. Picture: …` — the library on scratch data (no Apple Music switcher). Then run Phase 2's own phone check exactly as its plan's Global Constraints give it (its `KARA_PHONE_CODE` / `KARA_PHONE_PORT` command and the `phone check OK` grep), with the picture at `../.superpowers/sdd/2026-09-27-phase3-apple-music/shots/task-15-phones.png` — Phase 3 must not have broken the phones. Finally `grep -rnE 'eyJ[A-Za-z0-9_-]{20,}' ../.superpowers/sdd/2026-09-27-phase3-apple-music/shots || echo "no tokens in logs"` must print `no tokens in logs`.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: Apple Music and the login check"
```

- [ ] **Step 5: Hand over**

Give the user the checklist below. Start nothing yourself.

## User acceptance checklist (for the user, with their real Apple Music account; does not block any task)

**Before you start — use a copy of your library.** This build upgrades the library it opens, and an upgraded library won't open in older builds (the `phase1b-app` or Phase 2 dev app) until Phase 3 is merged. So quit your dev app, copy the data folder, and run this branch's dev app on the copy:

```
cp -R ~/Library/Application\ Support/kara-always-oki ~/kara-phase3-data
cd <the phase3-apple-music worktree>/app && KARA_DATA=~/kara-phase3-data npm run tauri:dev
```

The Apple Music login lives in your Keychain, not in the data folder, so it is shared with every build: connecting here connects them all, and Disconnect (step 12) removes it everywhere.

1. **Connect.** Settings › Sources › Connect: a window opens on music.apple.com's sign-in; sign in (Apple ID, two-factor; Apple's own pop-up window opens and closes). The window closes by itself, a toast says "Connected to Apple Music as …" (or without a name), and the row shows "Refreshing…", then your name · "Refreshed now". macOS may ask whether kara-app may use "world.aako.kara-always-oki" in your Keychain — choose Always Allow (a rebuilt dev app may ask again).
2. **Library.** The All · Local · Apple Music switcher appears. Apple Music › Playlists lists your playlists, including followed and Apple-made ones and **Favorite Songs** (your liked songs) — check that it's there and full; compare the count with the Music app. Playlists show Apple's own cover pictures. Albums lists your albums; Artists lists their album artists. Note how long the first refresh took for your library size.
3. **All.** Apple collections show the Apple logo; an artist you have both locally and on Apple Music shows once with songs from both; your own playlists can take Apple songs (Add to playlist) and show a small Apple logo on them.
4. **Sing.** Tap ten Apple songs across styles and languages (include Japanese, Chinese and Korean titles, a live song and a remix): each plays within about 10–20 s, from the right version (official audio where it exists). Note any wrong pick.
5. **Change match.** On a song, Song menu › Change match…: the versions found, the one in use checked; pick another — "Now singing from …", and the playing song starts over from that version, with its own lyric timing. Paste a YouTube link — it becomes the version in use. The player's "…" menu has the same Version › Change.
6. **Nothing fits.** An obscure or self-uploaded song with no fitting version: "No singable version found…" with Change match….
7. **Animated covers.** Open an album that has an animated cover on Apple Music (e.g. a recent major release): the header loops it; sing a song from it: the karaoke background loops it. Turn on Reduce motion: both go still.
8. **Phones.** Start a phone session and join from your phone: an Apple song in the queue shows its cover on the phone too.
9. **Search.** Type a song you don't have: Apple Music phrases follow YouTube's in the suggestions; results show an Apple Music section whose songs queue and play.
10. **Refresh.** In the Music app, add a song to a playlist and delete another playlist; press Refresh: the song appears, the playlist is gone, and a song you had sung from it is still found by search.
11. **Relaunch.** Quit and relaunch within six hours: no refresh starts (the time stays). After six hours a relaunch refreshes.
12. **Login runs out, then Disconnect.** Quit the app, delete the `world.aako.kara-always-oki` item in Keychain Access, relaunch and press Refresh (or wait for the start refresh): "Log in to Apple Music again" with Log in; the synced playlists still work; Log in opens the window and fixes it. Then Settings › Sources › Disconnect asks first; afterwards Apple playlists and albums are gone, the switcher disappears, songs you've sung (and those in your own playlists) still play, and the Keychain item is gone. Connect again if you want to keep using it.
13. **Nothing leaks.** `sqlite3 ~/kara-phase3-data/kara.db 'select * from provider_account'` shows no token; the terminal running `tauri:dev` never shows a string starting with `eyJ`.
14. **If something looks off** (empty playlists, no animated covers, wrong counts), run `cargo test -p kara-core live_library_has_what_sync_reads -- --ignored --nocapture` and share its one line of counts.
