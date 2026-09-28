# Developing KaraAlwaysOK

Notes for working on the code. For what the app does and how to install it, see the [README](../README.md).

## Build

```bash
source "$HOME/.cargo/env"   # if cargo isn't already on PATH
cargo build --release
```

The CLI binary is `target/release/kara`.

## Run the app

```bash
cd app
npm install
npm run tauri:dev
```

- `tauri:dev` hot-reloads the interface only. After changing Rust code, stop it and run it again.
- The first launch downloads the vocal-removal engine (about 90 MB).
- The app and the CLI share one library in `~/Library/Application Support/kara-always-oki`, and only one of them can use it at a time.
- Debug builds optimize dependencies fully and our own crates lightly (see `[profile.dev]` in `Cargo.toml`), so vocal removal runs at usable speed in `tauri:dev` without a release build.

### A scratch library

To try things without touching your library, point the app (or the CLI, with `--data <dir>`) at a scratch folder:

```bash
KARA_DATA=/tmp/kara-scratch npm run tauri:dev
```

Copy `runtime/` and `models/` into it from the real folder to skip the download.

## Phone mics

Debug builds read these settings:

| Variable | What it does |
|---|---|
| `KARA_PHONE_CODE=1234` | Starts a phone session at launch with that code. |
| `KARA_PHONE_PORT=8543` | Serves phones on that port only, leaving 443 and 80 alone. |
| `KARA_MIC_BUFFER_MS` | The shortest mic delay buffer, 10–60 ms (default 10). |

Auto-tune adds about 6 ms to most voices, up to about 15 ms for the lowest.

To check the phone side end to end, start a debug build with both phone settings, then:

```bash
NODE_TLS_REJECT_UNAUTHORIZED=0 KARA_PHONE_CODE=1234 KARA_PHONE_PORT=8543 node app/scripts/phone-check.ts
```

It talks to the app like a few phones and prints `phone check OK`.

## Checking the real app

- `app/scripts/isolated-check.sh <picture.png> [command]` builds and runs a separate copy of the app (its own build folder and scratch library under `.superpowers/`) with `KARA_PHONE_PORT` set to 8543 unless you set it; add `KARA_PHONE_CODE=1234` to start a phone session too. It saves a picture of its window, runs the command while it is open, then stops only that copy. It never stops your running app or uses port 1420.
- `npm run app-check -- <picture.png> [command]` (in `app/`) starts the already-built app with `tauri:dev`, saves a picture of its window, runs the command while it is open, and quits it. It fails if port 1420 is busy or a KaraAlwaysOK window is already open.

Both fail when the app logged a panic or an error.

## Tests

The same checks CI runs:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd app
npm run check        # types
npm run check:i18n   # every language has every string
npm test             # unit tests
npx playwright test  # browser tests
```

- Some Rust tests build small audio files with `ffmpeg` (embedded lyrics, MP3) and macOS's `afconvert` (AAC), so running them needs `ffmpeg` installed. The app itself never needs `ffmpeg`; it only downloads `yt-dlp` on its own, the first time you add a link that needs it.
- The browser tests run the interface against a fake backend (`app/tests/fake-backend.ts`, and `app/tests/fake-phone.ts` for the phone page) on port 1430, so they can run beside `tauri:dev` on 1420.

Every push to `main` and every pull request runs the CI workflow (`.github/workflows/ci.yml`) with all of the above. A release runs it first and stops if anything fails.

## Benchmarks

`kara bench <song> --model <onnx>` measures vocal-removal speed and memory (`--cpu` to skip CoreML, `--runtime <dylib>` to use a local ONNX Runtime, `--out <dir>` to write the stems). The full model comparison is in [`superpowers/spikes/2026-09-26-model-choice.md`](superpowers/spikes/2026-09-26-model-choice.md).

## Releases

Push a plain `vX.Y.Z` tag. The Release workflow (`.github/workflows/release.yml`) runs CI, then builds a draft GitHub release with the Apple Silicon `.dmg` and `latest.json`. Publish the draft so the app's update check can find it.

The build needs the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the update signing key and its password; both are set). If the key ever has to be replaced, make a new one with `npx tauri signer generate -w ~/.tauri/kara-always-oki.key`, update both secrets, and put the new public key in `app/src-tauri/tauri.conf.json`. Never commit the private key. Installs made with the old key won't accept updates signed with the new one.

The app isn't signed with an Apple Developer ID yet; the workflow passes the `APPLE_*` secrets through, so adding them later turns signing and notarization on.
