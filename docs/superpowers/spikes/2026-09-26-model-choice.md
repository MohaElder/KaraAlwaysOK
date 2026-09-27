# Model choice — spike results

Machine: MacBook Pro, Apple M5 Pro (6P+12E cores), 48 GB RAM, macOS 26.6.2 (build 25G83).

## Candidates

- `Kim_Vocal_2.onnx` — `https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx`
- `UVR-MDX-NET-Voc_FT.onnx` — `https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/UVR-MDX-NET-Voc_FT.onnx`

SHA-256 (downloaded once, `shasum -a 256`):
- `Kim_Vocal_2.onnx`: `ce74ef3b6a6024ce44211a07be9cf8bc6d87728cc852a68ab34eb8e58cde9c8b`
- `UVR-MDX-NET-Voc_FT.onnx`: `534b2070fcc7df514b13ef660dc8cbb328679c2374d04354a5c42bb14ecce111`

Parameters (looked up by MD5 of the model's last 10,000 KiB against
`https://raw.githubusercontent.com/TRvlvr/application_data/main/mdx_model_data/model_data_new.json`
— probed live, see report for the exact commands/output):

| Model | MD5 (last 10,000 KiB) | n_fft | dim_f | dim_t | hop | compensate |
|---|---|---|---|---|---|---|
| Kim_Vocal_2 | `970b3f9492014d18fefeedfe4773cb42` | 7680 | 3072 | 256 (2^8) | 1024 | 1.009 |
| UVR-MDX-NET-Voc_FT | `77d07b2667ddf05b9e3175941b4454a0` | 7680 | 3072 | 256 (2^8) | 1024 | 1.021 |

Both match the values the bench hard-codes (`primary_stem: 'Vocals'` for both).

## License

No explicit license was found for the model weight files themselves:

- `TRvlvr/model_repo` (hosts the `.onnx` files): GitHub reports no repository license. Root contains only `README.rd` (content: the single line `model_repo`) and `update_patches.txt`. No `LICENSE` file.
- `TRvlvr/application_data` (hosts `model_data_new.json`): same — no license field, no `README.md`, no `LICENSE` file.
- `Anjok07/ultimatevocalremovergui` (the UVR GUI project that trains/publishes these models): GitHub's repo metadata reports `MIT`, but no `LICENSE` file actually exists at the repo root and the dedicated `/license` API endpoint 404s — the detection looks stale/inconsistent. The README's own License section says: "The **Ultimate Vocal Remover GUI** code is [MIT-licensed](LICENSE)" and separately: "Please Note: For all third-party application developers who wish to use our models, please honor the MIT license by providing credit to UVR and its developers."

So: the UVR *code* is (nominally) MIT; the README asks third parties to credit UVR when using "our models," but that is a request, not a formal license grant, disclaimer, or redistribution term for the model weight files. **The model files' license is effectively unstated** — this is a finding to raise with the user before shipping a model download in the app, not a blocker for this benchmark.

## Benchmark

`kara bench <song> --model <onnx> --compensate <value>` (release build), CoreML unless noted. Test songs: two scored mix/instrumental pairs, plus two long files for speed/memory only.

| Model | Song | Length | x real time (CoreML) | x real time (CPU) | First 10 s chunk | Peak MB (CoreML) | Peak MB (CPU) |
|---|---|---|---|---|---|---|---|
| Kim_Vocal_2 | To Infinity N Beyond | 184.3 s | 14.30–14.41x | 5.03x | 1.2–1.9 s | 5824 | 9831 |
| Kim_Vocal_2 | To Nara From Eurasia | 332.9 s | 14.85x | — | 1.2 s | 5961 | — |
| Kim_Vocal_2 | DiscA (speed/mem only) | 1101.0 s | 14.59x | — | 1.2 s | 6919 | — |
| Kim_Vocal_2 | DiscB (speed/mem only) | 1101.2 s | 8.69x* | — | 2.0 s | 7041 | — |
| Voc_FT | To Infinity N Beyond | 184.3 s | 14.31x | — | 1.2 s | 5825 | — |
| Voc_FT | To Nara From Eurasia | 332.9 s | 14.64x | — | 1.2 s | 5960 | — |
| Voc_FT | DiscA (speed/mem only) | 1101.0 s | 14.42x | — | 1.2 s | 6990 | — |

\* DiscB's CoreML run overlapped with an unrelated CPU-bound scoring script on this machine; treat 8.69x as a lower bound, not a clean measurement.

Both models pass the **speed** half of the gate by a wide margin in both CoreML and CPU mode (≥ 5x real time, gate is ≥ 2x). Neither passes the **memory** half: CoreML peak is ~5.8–7.0 GB and CPU peak is ~9.8 GB, both roughly 4–6.5x over the 1500 MB budget, on the shortest test song already. Memory has a small song-length-dependent component (the bench harness itself accumulates the whole song's output in RAM for `--out`) but the dominant cost (~5.7–5.8 GB, present even on a 3-minute song with no `--out`) is fixed overhead from the ONNX Runtime session — most likely intermediate activation memory for this model's unusually large per-segment tensor (`dim_f=3072 x dim_t=256`), not something tunable in `separate/onnx.rs` as currently written. Cross-checked independently with `/usr/bin/time -l` (max resident set size ≈ 6.1 GB for the same CoreML run), so this is not an artifact of `peak_rss_mb()`.

## SDR scoring

Ruling: score each model's estimated instrumental (from `kara bench --out`) against the real released instrumental, per scored pair, after aligning by cross-correlation and trimming to the overlap. `SDR = 10*log10(||ref||^2 / ||ref-est||^2)`, plus SDR after the least-squares optimal gain on the estimate. Baseline = the unseparated mix scored the same way against the real instrumental.

Scoring script (numpy + scipy, run from outside the repo — see report for the full listing and every command):

```python
def align(ref_m, est_m):
    from scipy.signal import fftconvolve
    corr = fftconvolve(est_m, ref_m[::-1], mode="full")
    lag = np.argmax(corr) - (len(ref_m) - 1)
    ...

def sdr(ref, est):
    err = ref - est
    return 10 * np.log10(np.sum(ref ** 2) / np.sum(err ** 2))

def gain_matched_sdr(ref, est):
    g = np.dot(ref.ravel(), est.ravel()) / np.dot(est.ravel(), est.ravel())
    return sdr(ref, g * est), g
```

Probe (sanity check before trusting the scorer): real instrumental scored against itself gives `SDR = inf dB` (perfect match, as expected); the raw mix scored against the real instrumental gives the baseline below.

| Song | Baseline SDR (mix vs. real inst.) | Baseline gain-matched SDR |
|---|---|---|
| To Infinity N Beyond | -1.09 dB | 0.28 dB |
| To Nara From Eurasia | 9.34 dB | 9.51 dB |

| Model | Song | SDR | Gain-matched SDR |
|---|---|---|---|
| Kim_Vocal_2 | To Infinity N Beyond | 0.07 dB | 0.39 dB |
| Kim_Vocal_2 | To Nara From Eurasia | 9.75 dB | 9.85 dB |
| Voc_FT | To Infinity N Beyond | 0.07 dB | 0.39 dB |
| Voc_FT | To Nara From Eurasia | 9.80 dB | 9.90 dB |

Mean gain-matched SDR: Kim_Vocal_2 = 5.12 dB, Voc_FT = 5.15 dB (both ≈ 0.03 dB apart — a tie, well within the 0.5 dB rule). Both models beat their song's baseline, but only barely on "To Infinity N Beyond" (a dense mix): 0.39 dB vs. a 0.28 dB baseline. Amplitude checks (see report) show the released "instrumental" tracks are not simple mix-minus-vocals renders (e.g. the Infinity instrumental release is *louder* than the mix that contains it), which caps the achievable raw SDR regardless of separation quality — expected for real-world release pairs rather than synthetic ones, per the ruling.

Ear-checking is still pending for the user. The bench WAVs (`vocals.wav`/`instrumental.wav` per model per song) are at `~/Library/Application Support/kara-always-oki/bench/<model>/<song>/` for that: `bench/kim-vocal-2/{to-infinity,to-nara}/`, `bench/voc-ft/{to-infinity,to-nara}/`.

## Gate and decision

Gate: ≥ 2x real time **and** ≤ 1500 MB peak, CoreML or CPU, recorded both ways.

- Speed: both models pass comfortably in both modes.
- Memory: both models fail badly in both modes — CoreML ~5.8–7.0 GB, CPU ~9.8 GB, vs. a 1500 MB budget.

**Decision: BLOCKED.** Neither `Kim_Vocal_2` nor `UVR-MDX-NET-Voc_FT` passes the gate, so no `DEFAULT_MODEL` was added and Task 6 does not proceed. The two candidates are effectively tied on quality (mean gain-matched SDR 5.12 vs. 5.15 dB) and on speed, so this is not a close call decided in either model's favor — it's a memory-budget problem shared by both, at these fixed model parameters (`dim_f=3072`, `dim_t=256`).

Options for the user, in the order the brief raised them:
1. **CPU vs. CoreML** — already recorded above; CPU is worse on memory (9.8 GB vs. 5.8 GB) though somewhat better on model-load latency. Neither gets close to 1500 MB.
2. **Smaller `dim_t`** — UVR's inference code treats `dim_t` (the model's temporal window) as a runtime knob independent of the model file; a smaller `dim_t` (e.g. 128 or 64 instead of 256) shrinks the per-segment tensor `[1, 4, 3072, dim_t]` and its intermediate activations, which is the most likely lever on the ~5.7 GB fixed cost — at some cost to speed (more, smaller segments) and untested effect on quality, since these models were trained/tuned around `dim_t=256`.
3. **HTDemucs** — not benchmarked. It needs a different, heavier pipeline that doesn't exist in this codebase yet (no HTDemucs runner, no `kara bench` support for it), so standing it up was out of scope for this spike; the controller's ruling was to stop and report the numbers rather than open a second, unimplemented pipeline mid-spike.
4. **ONNX Runtime session limits** — not tried here (`separate/onnx.rs` uses default `SessionBuilder` options): capping intra-op thread count and/or the CPU arena's memory limit could reduce the ~5.7 GB fixed overhead without touching the model itself, but this needs its own probe and is a code change, not a benchmark knob.
