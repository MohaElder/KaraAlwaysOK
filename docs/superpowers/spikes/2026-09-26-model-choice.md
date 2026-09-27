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

## Round 1 gate check (superseded — see "Memory investigation" below)

Gate (as originally stated): ≥ 2x real time **and** ≤ 1500 MB peak, CoreML or CPU, recorded both ways.

- Speed: both models pass comfortably in both modes.
- Memory: both models fail badly in both modes — CoreML ~5.8–7.0 GB, CPU ~9.8 GB, vs. a 1500 MB budget.

Round 1's conclusion was BLOCKED on memory. The controller ruling for round 2 was that "most likely activation memory" was an unprobed guess, and directed a real memory investigation (probe over reasoning) before accepting that. That investigation, below, found and fixed the actual cause without touching the model or its parameters.

## Memory investigation (round 2)

### Locating the cost

Instrumented a scratch probe (`crates/kara-cli/examples/memprobe.rs`, deleted after use — not committed) that prints RSS after decode, after session creation, after the first inference, and at intervals through the rest of the song. Kim_Vocal_2, "To Infinity N Beyond":

| Stage | CoreML (default NeuralNetwork format) | CPU |
|---|---|---|
| After decode | 78 MB | 78 MB |
| After session load | 295 MB | 210 MB |
| After first chunk (first inference) | 5615 MB | 5602 MB |
| Chunk 5 | 5637 MB | 8343 MB |
| Chunk 10 | 5643 MB | 10949 MB |
| Chunk 15 / final (chunk 18) | 5643 MB | 11247 MB |

Answer to "fixed at load, fixed at first inference, or growing per segment": **CoreML jumps to ~5.6 GB at the very first inference call and then stays flat** (+28 MB over the rest of the song — not a leak, and not present at session load, so it's compiled/allocated lazily on first `Run()`). **CPU jumps similarly at first inference but then keeps growing** for about 15 segments before plateauing at ~11.2 GB — consistent with ONNX Runtime's CPU arena allocator repeatedly growing its high-water mark rather than reusing buffers, on a model whose per-segment tensor (`dim_f=3072 x dim_t=256`) is unusually large.

### Levers tried

All runs: Kim_Vocal_2, "To Infinity N Beyond", via the same probe, `RESULT peak_mb=... x_real_time=...`.

**CPU:** none of the standard levers help — every combination stays in the 10.9–12.7 GB range (noise-level differences from run to run):

| Config | Peak MB | x real time |
|---|---|---|
| default | 10873 | 5.15 |
| `--no-arena` (CPU EP `with_arena_allocator(false)`) | 12131 | 4.99 |
| `--intra 4` | 12468 | 4.86 |
| `--intra 4 --no-arena` | 12167 | 4.87 |
| `--intra 2` | 12147 | 3.65 |
| `--no-mem-pattern` | 12659 | 4.76 |
| `--inter 1 --intra 2` | 12249 | 3.20 |

CPU is not viable under any tried configuration.

**CoreML:**

| Config | Peak MB | x real time | Notes |
|---|---|---|---|
| default (`NeuralNetwork` format, `ComputeUnits::All`) | 5629 | 14.43 | round 1 baseline |
| `--compute-units ane` (`CPUAndNeuralEngine`) | 5159 | 9.82 | slower, barely less memory |
| `--compute-units cpu` (`CPUOnly`) | 7333 | 5.30 | worse on both axes |
| `--static-shapes` (`RequireStaticInputShapes(true)`) | 14738 | 5.10 | the model's input has a symbolic `batch_size` dim, so this disqualifies CoreML entirely and silently falls back to plain CPU (confirmed: session-load RSS and speed both match the CPU baseline, not CoreML's) |
| **`--mlprogram` (`ModelFormat::MLProgram`)** | **735–1000** (short songs) | **44–46** | **the winner — see below** |
| `--mlprogram --compute-units ane` | 1359 | 5.74 | also low-memory, but worse than plain MLProgram on both axes |

`ModelFormat::MLProgram` (Core ML 5+/macOS 12+ model format, vs. the EP's default legacy `NeuralNetwork` format) cuts CoreML's peak memory by roughly 7x and, unexpectedly, makes it 3x *faster* — both improvements together, no tradeoff found.

One wrinkle: MLProgram mode prints `E5RT encountered an STL exception. msg = Input: input has unbounded dimension which is not supported...` to stdout after every run. Confirmed this is benign teardown noise, not a correctness problem: it appears strictly after the program's own final "wrote ..." line (i.e. after all inference and file-writing completed, during process exit / session teardown), the process still exits 0, and — most importantly — the actual separated audio was cross-checked and is correct (next section).

### Quality cross-check (MLProgram vs. the original NeuralNetwork output)

Before trusting the low-memory, high-speed MLProgram numbers, scored its output against (a) the real instrumental and (b) the exact WAV produced by the unmodified `NeuralNetwork`-format CoreML run from round 1, using the same SDR scorer:

| Comparison | SDR | Gain-matched SDR |
|---|---|---|
| Kim_Vocal_2/MLProgram vs. real instrumental, Infinity | 0.07 dB | 0.39 dB |
| Kim_Vocal_2/MLProgram vs. round-1 NeuralNetwork output, Infinity | 56.33 dB | 56.33 dB (gain 1.0000) |
| Kim_Vocal_2/MLProgram vs. real instrumental, Nara | 9.76 dB | 9.85 dB |
| Kim_Vocal_2/MLProgram vs. round-1 NeuralNetwork output, Nara | 61.82 dB | 61.82 dB (gain 1.0000) |
| Voc_FT/MLProgram vs. real instrumental, Nara | 9.81 dB | 9.90 dB |
| Voc_FT/MLProgram vs. round-1 NeuralNetwork output, Nara | 63.09 dB | 63.09 dB (gain 1.0000) |

MLProgram's output matches the original NeuralNetwork output to 56–63 dB SDR (i.e. essentially bit-identical, modulo different compiled kernels) and reproduces round 1's real-instrumental scores to within 0.01 dB. **The memory and speed wins are real, not a broken or partial fallback.**

### `dim_t` (the third lever): confirmed not viable, by probe

Per the brief, only worth trying if the model's time axis is dynamic. Read the input shape straight from the loaded ONNX session: `Tensor<f32>(batch_size, 4, 3072, 256)` — the batch dim is symbolic, but the `dim_t` axis is a fixed literal `256`, not dynamic. Confirmed by actually trying `dim_t=128` (a smaller `MdxParams.dim_t`, producing a `[1,4,3072,128]` input): ONNX Runtime rejects it outright —
```
Error: Got invalid dimensions for input: input for the following indices
 index: 3 Got: 128 Expected: 256
 Please fix either the inputs/outputs or the model.
```
So this lever is closed: `dim_t` would require re-exporting the model, not a runtime option, and per the ruling's own gate note this makes MLProgram's "stay at dim_t 256" outcome the preferred one anyway.

### Memory metric fix (fix round 1)

A reviewer caught that `kara bench`'s memory number understated real usage: it read `ru_maxrss` via `getrusage()`, which macOS does not keep in step with actual physical footprint. The reviewer measured `/usr/bin/time -l`'s "peak memory footprint" line at 1.4–1.8x the number `kara bench` printed for the same runs.

Fixed `crates/kara-cli/src/main.rs` to read `ri_lifetime_max_phys_footprint` from `proc_pid_rusage(getpid(), RUSAGE_INFO_V4, ...)` instead (the same counter macOS's own tools use), keeping the output line labeled `peak memory`. Probed the fix against `/usr/bin/time -l` on the Infinity song, back to back:
```
kara bench:            peak memory      1415 MB
/usr/bin/time -l:      1483687400  peak memory footprint   (= 1414.9 MB)
```
and a second run:
```
kara bench:            peak memory      1437 MB
/usr/bin/time -l:      1506854352  peak memory footprint   (= 1436.7 MB)
```
Agreement is within measurement noise both times. All memory numbers below this point use the fixed metric; the exploratory tables above (round 1's benchmark table, and round 2's CPU/CoreML lever tables) were measured with the old, undercounting metric and are **not** re-stated here — their *relative* comparisons (CPU unsalvageable at any lever, MLProgram ~7x under any other CoreML config) still hold since every number in those tables undercounts by roughly the same factor, but their absolute MB figures should not be read as physical footprint.

### Numbers for the chosen config, re-measured with the fixed metric

| Model | Song | Length | x real time | Peak MB (physical footprint) |
|---|---|---|---|---|
| Kim_Vocal_2 | To Infinity N Beyond | 184.3 s | 45.3x | 1421 |
| Kim_Vocal_2 | To Nara From Eurasia | 332.9 s | 46.3x | 1567 |
| Kim_Vocal_2 | DiscA (speed/mem only) | 1101.0 s | 46.2x | 2152 |
| Voc_FT | To Infinity N Beyond | 184.3 s | 45.1x | 1437 |
| Voc_FT | To Nara From Eurasia | 332.9 s | 46.3x | 1570 |

Via the real `kara bench` binary. There is no enforced memory limit for this pipeline — the user's decision is that these measured numbers become README recommendations, not a hard pass/fail gate, so the table above is reported as-is rather than checked against a threshold. `kara bench` holds the whole song's separated audio in RAM to write the listening WAVs at the end, which the real chunked pipeline does not do (it writes each 10 s chunk to disk as it goes) — production peak memory on long songs should be lower than this harness reports, not higher.

### Final decision

**Kim_Vocal_2, CoreML with `ModelFormat::MLProgram`.** Changed `crates/kara-core/src/separate/onnx.rs` to set `.with_model_format(ModelFormat::MLProgram)` on the CoreML execution provider (nothing else — no thread/arena/compute-unit overrides, since none of those helped and MLProgram alone already gives a large win on both memory and speed). `DEFAULT_MODEL` added to `crates/kara-core/src/separate/mod.rs`:

```
id: "kim-vocal-2"
asset.file_name: "Kim_Vocal_2.onnx"
asset.url: https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx
asset.sha256: ce74ef3b6a6024ce44211a07be9cf8bc6d87728cc852a68ab34eb8e58cde9c8b
params: n_fft 7680, hop 1024, dim_f 3072, dim_t 256, compensate 1.009
```

**Why Kim_Vocal_2 and not Voc_FT:** the controller's instruction was to prefer Kim_Vocal_2 unless the probes separate the two models — and they don't. Quality ties (mean gain-matched SDR 5.12 dB vs. 5.15 dB, a 0.03 dB gap). Speed ties too: a reviewer ran a back-to-back A/B and got 44.89x vs. 44.98x, within run-to-run noise (this session's own numbers above show the same ±1x noise band across repeated runs of the *same* model). Memory ties as well (1421–2152 MB vs. 1437–1570 MB across the same songs). With nothing separating them, Kim_Vocal_2 is the pick by the controller's stated tiebreak, not because either candidate lost on any measured axis.

Ear-checking is still pending for the user. Bench WAVs are at `~/Library/Application Support/kara-always-oki/bench/{kim-vocal-2,voc-ft}/{to-infinity,to-nara}/` (round 1, `NeuralNetwork` format) and `~/Library/Application Support/kara-always-oki/bench/kim-vocal-2-mlprogram/{to-infinity,to-nara}/` (round 2, `MLProgram` format, quality-confirmed equivalent to round 1's).

License finding from round 1 stands: the model weight files' license is effectively unstated — still worth raising with the user before shipping a model download in the app.
