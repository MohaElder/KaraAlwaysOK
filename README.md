# kara-always-oki

A desktop karaoke app. Pick a song and sing over it: it strips the vocals out
(fully, or partly with a slider) and shows synced lyrics. Songs come from your
own files, pasted links, and later your Spotify / Apple Music library.
Local-only — audio, stems and lyrics stay on your computer.

This repo is the engine so far: a Rust library (`kara-core`) and a CLI
(`kara-cli`) for adding songs, preparing them (download, convert, look up
lyrics, take the vocals out) and exporting the result. The desktop app (Tauri
shell, UI, live playback) is a later phase.

## Build

```
source "$HOME/.cargo/env"   # if cargo isn't already on PATH
cargo build --release
```

The binary is `target/release/kara`.

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
| Any Mac, CPU only | 3 min | ~11 GB | not recommended |

CPU-only mode works but isn't recommended — its memory use is far higher for
no speed benefit. Windows support is planned; there are no measurements for
it yet.

## Development

```
cargo test --workspace
```

One test builds a small audio file with `ffmpeg` to check embedded-lyrics
handling, so running the tests needs `ffmpeg` installed. The app itself never
needs `ffmpeg` — it only downloads `yt-dlp` on its own, the first time you add
a link that needs it.
