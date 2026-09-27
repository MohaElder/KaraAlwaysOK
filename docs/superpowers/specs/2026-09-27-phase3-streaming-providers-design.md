# KaraAlwaysOK Phase 3: Apple Music — Design

Date: 2026-09-27
Status: design approved in conversation; pending written-spec review
Builds on: `2026-09-26-kara-always-oki-design.md` (Phase 1). Its UI rules, languages, copy rules and engineering bar apply.
Research: `../spikes/2026-09-27-streaming-providers.md`.

> **Scope change (2026-09-27): Apple Music only.** A spike confirmed the Apple Music web-player route end to end (library playlists, playlist tracks, and animated artwork via `editorialVideo` HLS). Spotify's web player did not hand a logged-in token to our login window in three attempts (only anonymous tokens and a client token), matching how actively Spotify guards its web player since 2025; Cider supports only Apple Music as well. Spotify is dropped for now; everything below that mentions Spotify is deferred and not part of this phase's plan.

## 1. What it is

Your Apple Music and Spotify libraries appear inside KaraAlwaysOK: playlists (including followed and service-made ones), saved albums, followed artists and liked songs, with their artwork, including animated artwork where it exists. When you sing one of those songs, the app finds a version it can fetch (the audio always comes from YouTube; never from Apple Music or Spotify), separates the vocals and shows the lyrics, like any other song.

### How it reads your library (decided)

The official APIs don't allow what we need: Spotify's development mode hides the tracks of playlists you don't own, and Apple requires a paid developer account per user and doesn't expose animated artwork at all. So, like Cider, the app signs in through each service's real web page inside the app, keeps the web player's own tokens, and calls the same web APIs the web player uses. This is against both services' terms; the user accepted that for this personal, non-commercial, open-source app. Each person uses their own account; tokens stay in their Mac's Keychain.

### Success criteria

- Connecting a service takes one login, with no developer accounts or pasted IDs.
- All the user's playlists show their songs, including followed and service-made playlists.
- Tapping a streaming song plays a correct match most of the time, and "Change match…" fixes the rest in a few taps.
- Animated artwork plays where the service has it.
- When a service breaks the web API, the app says so plainly and everything already synced keeps working.

## 2. Experience

- **Provider switcher** (as in the prototype) at the top of the sidebar: All · Local · Apple Music · Spotify, with the services' logos. Only connected services appear.
- **Settings › Sources**: each service with Connect / Disconnect, the account name, the last refresh time, and Refresh.
- **Connect** opens a window with the service's real login page (music.apple.com or open.spotify.com). When sign-in completes, the app keeps the tokens and closes the window: "Connected to Spotify as …".
- **Expired or refused tokens**: "Log in to Spotify again" with a button that reopens the login window. Nothing else breaks.
- **Browsing**: collections from a service look and behave like local ones (Cover rule, Sing/Shuffle, song rows, search). User-made KaraAlwaysOK playlists can mix songs from every source.
- **Song menu › Change match…**: a sheet listing the candidate uploads (thumbnail, title, channel, length, which one is in use) plus "Paste a link". The choice is remembered for that song.
- **Search suggestions**: once connected, Apple Music and Spotify results and suggestions appear in the search bar's suggestions and results alongside local and YouTube.
- **Artwork**: normal covers everywhere; where a collection or song has animated artwork, it loops on the collection header and as the karaoke background, falls back to the still cover, and stays still under Reduce motion.
- The app never writes to your Apple Music or Spotify library.

## 3. Architecture

### 3.1 Sign-in and tokens

- A separate login window (Tauri webview) loads the service's web login. The app watches the window's cookies and page until the session is signed in, then extracts what the web player uses:
  - **Apple Music**: the web player's developer token (embedded in music.apple.com's web app) and the `media-user-token` cookie, plus the storefront.
  - **Spotify**: the `sp_dc` login cookie, exchanged for short-lived web-player access tokens (and the client token the web API expects), refreshed as the web player does.
- Tokens are stored in the macOS Keychain, never in the library database or logs. Disconnect deletes them and that service's synced collections (songs you've already sung stay playable).
- The exact token-extraction steps are confirmed by a spike (§5) before the plan is written.

### 3.2 Provider clients (Rust, in kara-core)

- One small client per service with the calls the web player makes: list playlists (owned and followed), playlist items, saved albums, album tracks, followed artists and their top songs, liked songs, and artwork (including animated: Apple `editorialVideo` HLS; Spotify Canvas).
- Requests stay light: paced, cached, only on refresh or when a collection is opened; 429s honor Retry-After; responses map to the existing library model (tracks with `provider` + `provider_ref`, ISRC when available; collections per provider).
- Errors are coded (Phase 1's problem codes): signed out, service changed/unavailable, rate limited.

### 3.3 Sync

- On app start (at most every few hours) and on Refresh: pull collections and their items; add/update/remove the service's collections and tracks in the library; keep matches, lyrics and prepared audio of tracks that still exist.
- The existing library upgrade mechanism adds any fields needed (e.g. ISRC, match state).

### 3.4 Matching

- On first play or queue of a streaming track (not up front): find candidates via the ISRC on YouTube Music's songs search (official "Topic" audio), then `ytsearch` on "artist – title".
- Score: normalized title and main-artist similarity, album, duration closeness; reject live/remix/cover/instrumental/sped-up versions unless the original is one. The best candidate becomes the track's audio source; the list is kept for Change match.
- Manual choices (a candidate or a pasted link) are remembered and never overridden. Lyric timing is per audio source (Phase 1), so each match keeps its own; auto lyric sync handles intros.

### 3.5 Animated artwork

- Apple: HLS streams play natively in the macOS web view's `<video>`. Spotify Canvas: short MP4 loops. Fetched only for visible collections and the current song; cached with the artwork.

## 4. Errors and edge cases

- Token expired/revoked: that service shows "Log in again"; synced content stays.
- The service changes its web API: refresh and new artwork fail with a plain message; synced collections, matches and prepared songs keep working.
- No good match: the song shows "No singable version found" with Change match (paste a link).
- A playlist item that isn't a song (podcast episode, local file): skipped.
- Rate limited: back off and retry later; no error spam.

## 5. Spike before planning

A throwaway test with the user's real accounts (the user logs in once to each; tokens stay on the Mac and are deleted after): confirm that the token extraction works inside a Tauri/wry window, and that library, playlist-items and animated-artwork calls work with those tokens. Findings go into the research doc; nothing is kept.

## 6. Testing

- Rust unit tests on recorded, anonymized response samples (made-up titles): mapping, sync add/update/remove, matching scores and rejections.
- Browser tests (the Phase 1 Playwright suite with the faked backend): the provider switcher, Sources connect/disconnect states, Change match sheet, animated artwork fallback.
- A manual check on the user's checklist with their real accounts.

## 7. Out of scope

- Playing Apple Music or Spotify audio.
- Editing Apple Music or Spotify libraries or playlists.
- Windows (later, with the rest of the app).
