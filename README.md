# KaraAlwaysOK

A desktop karaoke app. Pick a song and sing over it: it strips the vocals out
(fully, or partly with a slider) and shows synced lyrics. Songs come from your
own files, pasted links, and later your Spotify / Apple Music library.
Local-only — audio, stems and lyrics stay on your computer.

The repo holds the engine — a Rust library (`kara-core`) and a CLI (`kara-cli`) for adding
songs, preparing them (download, convert, look up lyrics, take the vocals out) and exporting
the result — and the desktop app in `app/` (Tauri 2 + Svelte 5), which plays prepared songs
live with the singer slider, key and synced lyrics. The app speaks English, Japanese, Korean,
Simplified Chinese, Traditional Chinese and Spanish, following the system language.

## Build

```
source "$HOME/.cargo/env"   # if cargo isn't already on PATH
cargo build --release
```

The binary is `target/release/kara`.

## Run the app

```
cd app
npm install
npm run tauri:dev
```

`tauri:dev` hot-reloads the interface only; after changing Rust code, stop it and run it again.
The first launch downloads the singing engine (about 90 MB). The app and the CLI share the
same library in `~/Library/Application Support/kara-always-oki`, and only one of them can
use it at a time.

To try things without touching your library, point the app (or the CLI) at a scratch folder:
`KARA_DATA=/tmp/kara-scratch npm run tauri:dev`. Copy `runtime/` and `models/` into it from
the real folder to skip the download.

Debug builds optimize dependencies fully and our own crates lightly (see `[profile.dev]` in
`Cargo.toml`), so separation runs at usable speed in `tauri:dev` without a release build.

## Phone mics

Guests sing into their phones. Click the mic button at the top right, then scan the QR code with a
phone on the same Wi-Fi (or open the address shown and type the code). No app to install; voices
stay on your network. The first time, the phone warns that the page isn't trusted: on iPhone tap
Show Details, then visit this website; on Android tap Advanced, then Proceed. macOS may ask once to
allow incoming connections for KaraAlwaysOK. Keep phones away from the speakers; a mic that starts
to howl is turned down on its own. Up to four phones at once. Each guest can add an effect to their
own voice (Karaoke mix or Auto-tune) with a strength slider. Mics play through the Mac's current
speakers; Bluetooth speakers and headphones add a noticeable delay.

For development (debug builds): `KARA_PHONE_CODE=1234` starts a phone session at launch with that
code and `KARA_PHONE_PORT=8543` serves phones on that port only (leaving 443 and 80 alone);
`NODE_TLS_REJECT_UNAUTHORIZED=0 KARA_PHONE_CODE=1234 KARA_PHONE_PORT=8543 node app/scripts/phone-check.ts`
then talks to that app like a few phones and prints `phone check OK`. `app/scripts/isolated-check.sh`
builds and runs a separate copy of the app on a scratch library with both set (it never stops your
running app or uses port 1420). `KARA_MIC_BUFFER_MS` (10–60, default 10) sets the shortest mic delay
buffer. Auto-tune adds about 10 ms to a voice, up to about 25 ms for low voices.

## Use

```
kara add <path-or-link>              # add a file or a link to the library
kara prepare <track-id> [--cpu]       # download/convert, find lyrics, remove vocals
kara search <query>                   # search the library
kara export <track-id> <out-dir>      # write vocals.wav and instrumental.wav
kara bench <song> --model <onnx>      # measure separation speed and memory
```

`kara add` accepts a local audio file, a direct link to an audio file, or a
link to a page yt-dlp can pull audio from (YouTube, SoundCloud, Bandcamp,
etc.). Links to streaming services (Spotify, Apple Music, Tidal, Deezer)
can't be downloaded and are refused with a plain message.

`kara prepare` downloads the ONNX Runtime and the vocal-removal model on
first use (both are cached under the data folder afterward), then separates
the song's vocals from its instrumental in 10-second chunks. Preparing the
same song again is instant. Use `--cpu` to force CPU instead of CoreML.

All data lives under `~/Library/Application Support/kara-always-oki` by
default, or pass `--data <dir>` to use a different folder.

## Recommended specs

Measured with `kara bench` on Apple Silicon (see
`docs/superpowers/spikes/2026-09-26-model-choice.md` for the full
benchmark). These are recommendations, not enforced limits — there's no
hardware check in the code.

| Setup | Song length | Peak memory | Speed |
|---|---|---|---|
| Apple Silicon, CoreML | 3–5 min | ~1.4–1.6 GB | ~45x real time |
| Apple Silicon, CoreML | 18 min | ~2.2 GB | ~45x real time |

CPU-only: not recommended; it used more than 11 GB in testing, for no speed
benefit over CoreML. Windows support is planned; there are no measurements
for it yet.

## Development

```
cargo test --workspace
```

Some tests build small audio files with `ffmpeg` (embedded lyrics, MP3) and
macOS's `afconvert` (AAC), so running the tests needs `ffmpeg` installed. The app itself never
needs `ffmpeg` — it only downloads `yt-dlp` on its own, the first time you add
a link that needs it.

## Before the first release

Do these once, after the app is built and the GitHub repo exists:

1. Create the repo on GitHub and push.
2. Make the update signing key: run `cargo tauri signer generate -w ~/.tauri/kara-always-oki.key`
   and keep the private key and its password safe (never commit them).
3. Add GitHub secrets `TAURI_SIGNING_PRIVATE_KEY` (the private key's contents) and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. Put the public key in `plugins.updater.pubkey` in `app/src-tauri/tauri.conf.json`
   (replacing `REPLACE-WITH-UPDATER-PUBLIC-KEY`), and the repo owner in the updater
   endpoint URL (replacing `GITHUB-OWNER`).
5. Add a CI workflow that gates every push and pull request on: the Rust unit tests
   (`cargo test --workspace`) and `cargo clippy --workspace --all-targets -- -D warnings`;
   the app's unit tests (`npm test`) and browser tests; the type check (`npm run check`,
   which runs svelte-check and TypeScript); and the language check (every locale has
   exactly the English keys). Make the release workflow require it.
6. Push a `v*` tag. The Release workflow builds a draft release with the `.dmg` and
   `latest.json`; publish it so the app's update check can find it.
