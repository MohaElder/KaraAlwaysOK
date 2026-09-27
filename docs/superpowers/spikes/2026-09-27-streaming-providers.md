# Phase 3 research: Spotify and Apple Music (2026-09-27)

Sources checked: official Apple and Spotify docs and license terms, Cider 1 (AGPL, archived) and Cider-2's public repo (issue tracker only, closed source), spotDL, and two motion-artwork tools. Full source list at the end.

## Apple Music
- **Library access** needs a developer token (an ES256 JWT signed with a MusicKit key, which requires a paid Apple Developer membership, ~$99/yr) plus a user token from MusicKit JS `authorize()`. That opens an Apple popup and needs an Apple Music subscription.
  - Inside Tauri the popup must be allowed with `on_new_window`. Popups are reported blocked on Windows/WebView2 (tauri#14263). Whether Apple accepts the app's origin is untested.
  - MusicKit JS must be loaded from Apple's CDN, not bundled.
- **Native MusicKit Swift** only works in a build signed with a team that has MusicKit enabled, which self-builds of an open-source app won't have. It's also macOS-only.
- **Cider 1 used a token scraped from the Apple Music web player** and faked the `origin` header. Apple calls that unsupported and blocked Cider in Dec 2022. Cider-2 is closed source.
- **License:** the MusicKit terms limit use to facilitating access to the user's Apple Music subscription, and say artwork and metadata may not be used separately from playback or playlist management. Driving karaoke from YouTube audio is a grey area. With a bring-your-own key, each user holds that license.
- **Animated (motion) artwork is not in the public Apple Music API.** Apple staff say `editorialVideo` isn't available to third-party apps.
  - It exists only on the private web-player host (`amp-api.music.apple.com … extend=editorialVideo`, HLS `.m3u8`), which needs the scraped token.
  - Only some albums have it.
  - WKWebView plays HLS natively; WebView2 would need hls.js or the mp4 variant.

## Spotify
- **Auth:** Authorization Code + PKCE with no secret. The redirect must be a loopback address (`http://127.0.0.1:PORT`; `localhost` is banned). Refresh tokens expire 6 months after consent.
- **Development mode, since Feb 2026:**
  - The app owner needs Premium, and each app allows at most 5 users.
  - **Playlist tracks are returned only for playlists the user owns or collaborates on**; followed and Spotify-made playlists return metadata only.
  - Search returns at most 10 results.
  - ISRC is back since March 2026.
  - Extended access needs a registered business with 250k monthly users.
- **Endpoints:** `GET /me/playlists`, `GET /playlists/{id}/items`, `GET /me/albums`, `GET /me/tracks`, `GET /me/following?type=artist`.
- **Canvas (looping video art) has no public API.** It exists only through the web player's private GraphQL with spoofed tokens. That's outside the terms and risks the account.

## Matching a streaming song to a fetchable upload
- ISRC first (Spotify `external_ids.isrc`; Apple catalog `isrc` via `include=catalog`). Search YouTube Music "songs" (the official "Topic" audio), then fall back to a YouTube search on "artist - title".
- Score candidates on title and artist similarity, album and duration, rejecting live, remix, cover, instrumental, slowed and similar versions (spotDL's approach). Remember the user's manual pick.
- Music videos have longer intros; the app's auto lyric sync covers the offset.

## Other constraints
- Keep tokens in the OS keychain.
- Honor 429s and `Retry-After` on both services.
- Spotify's flow works the same on Windows. Apple's popup doesn't yet on WebView2.

## Recommendation
- **Spotify first:** a user-supplied client ID with PKCE and a loopback redirect. Tell users up front what they need and what they won't see. No Canvas.
- **Apple second:** bring your own developer key (Team ID, Key ID, .p8), with tokens signed locally and a MusicKit JS popup for the user token. Spike the popup first. No motion artwork in the core app; at most an opt-in, clearly labelled unofficial add-on.

## Sources
- Apple: developer tokens https://developer.apple.com/documentation/applemusicapi/generating-developer-tokens ; user auth https://developer.apple.com/documentation/applemusicapi/user-authentication-for-musickit ; editorialVideo https://developer.apple.com/forums/thread/696843 ; license PDF https://developer.apple.com/support/downloads/terms/apple-developer-program/Apple-Developer-Program-License-Agreement-English.pdf ; Cider https://github.com/ciderapp/Cider , https://github.com/ciderapp/Cider-2 ; Tauri popups https://github.com/tauri-apps/tauri/issues/14263
- Spotify: Feb 2026 migration https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide ; redirect URIs https://developer.spotify.com/documentation/web-api/concepts/redirect_uri ; refresh tokens https://developer.spotify.com/blog/2026-06-18-refresh-token-expiration ; quotas https://developer.spotify.com/blog/2026-07-23-web-api-quota-updates ; policy https://developer.spotify.com/policy
- Matching: https://github.com/spotDL/spotify-downloader
