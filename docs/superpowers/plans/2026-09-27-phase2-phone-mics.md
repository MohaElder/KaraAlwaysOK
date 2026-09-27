# Phase 2 — Phone Mics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Guests scan a QR code on the Mac and use their phone as a live mic (about 30 ms to the speakers, with reverb, feedback control and up to 4 phones), and from the phone see the lyrics, set their own voice, its effect (karaoke mix or auto-tune) and the singer level, search songs and manage the queue.

**Architecture:** An HTTPS + WebSocket server inside the app (axum + axum-server with rustls/ring, a self-signed rcgen certificate kept in the data folder) serves the phone page and one WebSocket per phone that carries JSON control messages and 16-bit mic frames. Rust mixes the phones (per-phone adaptive jitter buffer with cubic resampling, howl control, the phone's own voice effect, gain, a shared reverb and a limiter — all in `kara-core::mic`) into its own low-latency cpal output stream, while the karaoke track keeps playing through the web view. The phone page is not a second bundle: it is a SvelteKit route (`/phone`) in the existing app, so it shares the build, tokens, fonts, i18n, motion, `Artwork`, `Toasts`, the lyric timeline and a queue list extracted from the queue panel; the Rust server serves it from the app's embedded files (`AppHandle::asset_resolver`, which falls back to `index.html` for app routes) and, under `tauri dev`, passes requests through to the Vite dev server. This is the least code: no second Vite config, no copied i18n or tokens, one `npm run build`.

**Tech Stack:** Rust 2021; kara-core (+ `realfft` already there); app crate adds axum 0.8 (`ws`), axum-server 0.7 (`tls-rustls-no-provider`), rustls 0.23 (`ring`), rcgen 0.13, time 0.3, tokio 1, reqwest 0.12 (already built by kara-core), qrcode 0.14 (`svg`), if-addrs 0.13, getrandom 0.3, cpal 0.16. Frontend: Svelte 5 + SvelteKit 2 (adapter-static), AudioWorklet, Screen Wake Lock, Playwright (existing suite) with in-page fakes.

**Spec:** `docs/superpowers/specs/2026-09-27-phase2-phone-mics-design.md` (authority), built on `docs/superpowers/specs/2026-09-26-kara-always-oki-design.md` (its UI, copy, language and engineering rules apply; its §3.4 WebRTC design is replaced by the Phase 2 spec). Evidence: `docs/superpowers/spikes/2026-09-27-phone-mics.md`. UX reference: `docs/prototype/hifi.html` — the "Phone preview" screens (Join, Connecting, Mic permission, Microphone mode with Mic / Songs / Queue, Reconnecting, Session ended, Mic blocked; CSS under `/* PHONE PAGE */`, script `pViews` / `pTabs` / `pGo`) and the Mac "Sing into your phone" window and mic pill (`#mics`, `renderMics`, `.pill`). Reuse their CSS and markup; the plan's code already does where it fits.

## Global Constraints

- Preconditions and where to work: Phase 1b is complete through its Task 32 (`phase1b-app` at c6f33da or later). **Execute in a separate git worktree**, never in the user's checkout `/Users/mohaelder/Repos/kara-always-oki`: that checkout runs the user's dev app (every edit under `app/src` there hot-reloads into it mid-song) and another session still commits Phase 1b there. Before Task 1: `cd /Users/mohaelder/Repos/kara-always-oki && git worktree add ../kara-always-oki-phase2 -b phase2-phone-mics phase1b-app`, then `cd ../kara-always-oki-phase2/app && npm ci`. Every path and command in this plan is relative to that worktree (its own `target/`, `app/build`, `app/.svelte-kit`, `.superpowers/sdd/…`); commit there on `phase2-phone-mics`. The real code at HEAD wins over any snippet here: when a name or signature differs, follow HEAD and keep this plan's behavior.
- **The user's dev app must never be disturbed.** The user runs `kara-app` with Vite on port 1420 from their own checkout while tasks run. No step may touch that checkout, stop or restart the app (`pkill`, `kill` by name), bind or wait on port 1420 (or 443/80, which the user's app may hold), run `app/scripts/app-check.sh` or `npm run tauri:dev`/`npm run app-check`, or play audible sound. End-to-end checks use the **isolated check** (below): its own target folder, its own build with the page inside, its own phone port, its own process id. The last task tells the user how to pick the work up; it starts nothing.
- Before any `cargo` command run `source "$HOME/.cargo/env"`.
- Code bar (the user's): comments only summarize what a function does (a variable gets one only if someone would reasonably ask) — no reasoning, history or postmortems; code explains itself through names and structure; less code and reuse over new layers (YAGNI); every test must be necessary, no redundant tests.
- Probe over reasoning: when an API or behavior is in doubt, run a focused command or test or read the real source (`~/.cargo/registry/src/*/<crate>`, `app/node_modules/<pkg>`) before deciding.
- UI copy is plain: no engineering words (no "certificate", "server", "WebSocket", "buffer", "latency", IPs or ports) — the one exception is the typed address in Task 6: a port shows only when 443 was taken, and `https://` only when port 80 couldn't be served. Every piece of UI text goes through `t(key, params)` in all six locales (`app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`); a task that adds text adds its keys to all six; `npm run check:i18n` must pass. Song titles, artists, names and lyrics show exactly as given. The phone page follows the phone's browser language.
- Phase 1 UI rules apply to the Mac window and the phone page: tokens only from `app/src/styles/tokens.css`, Phosphor Bold icons imported from `phosphor-svelte/lib/<Name>Icon`, glass for everything floating (pill, tab bar, banner, Voice/Singer/Effect buttons and panels, toasts), motion only through `$lib/motion` (`fade`, `slide`; 240 ms, one easing; fade only under reduced motion), light/dark from the system, icon beside every label except song info.
- Sample lyrics in code and tests are made up. Never real lyrics.
- Phone traffic stays on the LAN: the server answers only private, link-local and loopback addresses, and phones talk only to the Mac — voices never leave the network. Two stated exceptions: searching YouTube and downloading a guest's link use the internet on the Mac exactly as its own search and adding do, and the phone's Songs tab shows YouTube result thumbnails straight from YouTube's image server, as any web page does (proxying them through the Mac would add code for no guest-visible gain).
- Numbers (from the spec and spike): jitter buffer starts at 20 ms (the floor; `KARA_MIC_BUFFER_MS` overrides it for the manual test), grows 10 ms per underrun up to 60 ms, shrinks 2 ms per 10 s without one, trims a burst when more than 30 ms over target, fades 5 ms; per-phone gain = 2.0 × Mac volume % × phone Voice %, so never above 2.0; limiter ceiling 0.89; up to 4 phones; output stream 256 frames when the device allows; phones send about 5 ms per frame; the phone pings every second, retries every 2 s and gives up after 2 minutes, and treats 3 s without a message as a dropped connection; the Mac treats 5 s without a message as dropped, removes a dropped row after 2 minutes, and sends levels every 80 ms (the `"phones"` view only on changes); voice effects: Karaoke mix = 120 ms echo (feedback 0.35) + warm reverb, Auto-tune = nearest semitone with taps spaced by whole pitch periods (≥ 3.5 ms), adding ≤ ~12 ms for most voices and up to ~25 ms for low ones (the user's choice, spec updated), intensity 0 = dry, every change fades over 20 ms; the audio callback locks only the mixer, and nothing under that lock allocates, frees or grows; the phone asks YouTube 280 ms after typing stops (as the Mac does).
- Phone session wire contract: close codes 4001 = session ended or phone removed, 4002 = room full, 4003 = wrong code. Join code: 4 random digits, shown on the Mac as `OKI-1234`; the server compares digits only. Ports: HTTPS on 443 (8443 when 443 is taken), a plain-HTTP redirect on 80 when free. The certificate lives in `<data folder>/phones/certificate.json`, valid 800 days, remade 30 days before it runs out or when the Mac has an address it doesn't cover.
- Lock order: `AppState` mutexes (`player`, `lib`) may be held while taking the phone session lock (`player::update` broadcasts to phones), never the reverse — code holding the session lock must not call anything that locks `AppState`. The mixer lock is a leaf (the audio callback takes only it).
- The phone page (`app/src/routes/phone`, `app/src/lib/phone`) never calls Tauri at runtime; it may import types from `$lib/api` and shared UI modules that don't invoke commands.
- Verification is commands only; no task needs a real phone, ears or clicks — those go to the user checklist in Task 15. Checks: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cd app && npm test && npm run check && npm run check:i18n`, Playwright `cd app && npx playwright test <files>` (its own port 1430), and, from Task 6 on, the **isolated check** with the phone check: `cd app && KARA_PHONE_CODE=4827 KARA_PHONE_PORT=8543 zsh scripts/isolated-check.sh ../.superpowers/sdd/2026-09-27-phase2-phone-mics/shots/task-NN.png "NODE_TLS_REJECT_UNAUTHORIZED=0 node scripts/phone-check.ts"`, then `grep -q 'phone check OK' ../.superpowers/sdd/2026-09-27-phase2-phone-mics/shots/task-NN.while-open.txt` (Bash timeout 10 minutes; the first build in its own target folder is slow). It builds the page and the app with the page inside (the release serving path), runs that copy by process id on the scratch data folder, pictures its window and stops only that process; webview errors of that copy aren't in its log, which is why the UI is covered by Playwright. The task report names the picture.
- Never read or write `~/Library/Application Support/kara-always-oki`; the isolated check uses `.superpowers/sdd/2026-09-27-phase2-phone-mics/data` (a fresh one downloads the singing engine in the background, which the checks don't wait for).
- No remote actions: no `git push`, no `gh`, no tags.
- Dev-only knobs (debug builds): `KARA_PHONE_CODE` opens a phone session at launch with that code; `KARA_PHONE_PORT` serves phones only on that port and leaves 443/80 alone; `KARA_MIC_BUFFER_MS` sets the jitter buffer floor (clamped to 10–60). All are listed in the README.

## Review Focus

1. **Wi-Fi stalls then bursts** (TCP holds packets during a retransmit, then delivers 100+ ms at once) — the voice fades out instead of clicking, the buffer target grows, and after the burst the delay drops back to the target instead of staying 150 ms late; a phone that is reconnecting sends none of the sound it captured meanwhile. Tests: Task 1 `a_wifi_stall_fades_out_grows_the_target_and_a_burst_is_trimmed_back`; Task 10 `when Wi-Fi drops it rejoins as the same phone, sends no late sound, and gives up after two minutes`.
2. **Phone clock drift and 44.1 kHz phones** — a phone whose clock runs 0.1 % fast or slow, or that records at 44.1 kHz, plays at the right pitch for minutes with the delay steady and no underruns. Test: Task 1 `keeps_the_delay_steady_when_the_phone_clock_drifts_or_runs_at_44_1_khz`.
3. **Feedback (howl)** — a pure tone that lasts and grows (or already screams) is turned down within a second and comes back once it settles; singing with vibrato, a crescendo and a held note are left alone; four loud phones at full volume never pass the ceiling. Tests: Task 2 `a_growing_or_screaming_tone_is_turned_down_within_a_second_then_comes_back`, `singing_and_a_steady_held_note_are_left_alone`; Task 3 `four_phones_at_full_volume_never_pass_the_ceiling_and_the_reverb_dies_away`.
4. **Phone screen lock and reconnect** — a locked phone's socket can die silently or close late; the phone notices silence and rejoins into its own row, a late close of the old connection doesn't grey out the new one, the Mac notices a silent phone within 5 s, and a mic stopped by the lock screen waits for a tap. Tests: Task 6 `a_phone_that_drops_gets_its_own_row_back`; Task 10 `a connection gone silent is replaced, and a mic stopped by the lock screen waits for a tap`.
5. **The Mac's address changes** — a certificate is reused while it covers the Mac's addresses, remade when a new address appears (keeping the old ones) or a month before it runs out. Test: Task 6 `a_kept_certificate_is_reused_until_the_mac_gets_a_new_address_or_it_nears_expiry`.
6. **A 5th phone** — refused with "This room is full." while a dropped phone's row is still kept for it, until that phone has been gone for two minutes. Tests: Task 6 `a_phone_needs_the_code_and_the_fifth_is_refused`; Task 8 `a_phone_gone_for_two_minutes_loses_its_row`; Task 10 `a wrong code or a full room sends the guest back to Join with the reason`.
7. **The Mac sleeping, and a session nobody uses** — phones that can't reach the Mac show Session ended after two minutes; a dropped row leaves after two minutes and the session (server, output stream, ticker) ends when the window is closed and no phone is left, so the Mac can idle-sleep; on wake the session ends. Tests: Task 10 (as in 1); Task 8 `a_phone_gone_for_two_minutes_loses_its_row`. (The wake check itself is one comparison, left to the checklist.)
8. **Auto-tune and effect switching** — a note 30 cents flat or sharp lands on the semitone at 98, 110, 131, 262 and 440 Hz, intensity 50 corrects half, the delay while correcting stays ≤ 12 ms for voices ≥ 200 Hz and ≤ 25 ms for low ones, low sung notes never read outside the tap line (a panic there would abort the app from the audio callback), and switching effects or strength never clicks. Tests: Task 4 `auto_tune_pulls_low_and_high_notes_to_the_nearest_semitone_as_hard_as_asked`, `auto_tune_delays_most_voices_about_12_ms_and_low_voices_at_most_25`, `auto_tune_keeps_going_through_low_sung_notes_and_reads_right_at_the_edge_of_its_line`, `switching_effects_or_their_strength_never_clicks`, `karaoke_mix_is_dry_at_zero_and_adds_a_room_and_an_echo_when_up`.
9. **The output device going away** (USB or Bluetooth speakers unplugged) — no phone buffer grows past 200 ms, and the session ends with a toast (reopening on the new speakers if the window is open). Tests: Task 1 `a_phone_nobody_hears_holds_little_memory`; Task 9 (the stopped toast and reopening).

---

## File Structure

```
kara-always-oki/
├─ README.md                                   + Phone mics; dev knobs
├─ crates/kara-core/src/
│  ├─ lib.rs                                   + pub mod mic
│  ├─ problem.rs                               + PhonesStart, NoNetwork
│  └─ mic/
│     ├─ mod.rs                   (new)        Mixer, NewVoice, Gone, Level, voice_gain, reverb, limiter
│     ├─ buffer.rs                (new)        JitterBuffer (adaptive delay, cubic resampling, fades)
│     ├─ howl.rs                  (new)        Howl (feedback detector and gain)
│     └─ effects.rs               (new)        Effect, Effects (karaoke mix, auto-tune)
├─ app/
│  ├─ scripts/phone-check.ts      (new)        talks to a running app like a phone
│  ├─ scripts/isolated-check.sh   (new)        own build, own phone port, own process; never touches port 1420
│  ├─ scripts/window-id.swift                  also finds a window by process id
│  ├─ src-tauri/
│  │  ├─ Cargo.toml                            + server, certificate, audio crates
│  │  └─ src/
│  │     ├─ lib.rs                             + Phones state, commands, lyrics and link hooks, end on quit, KARA_PHONE_CODE
│  │     ├─ state.rs                           AppError.problem readable by the phone module
│  │     ├─ adding.rs                          YouTube search and link preview bodies shared with phones
│  │     ├─ player.rs                          queue entries carry `by`; snapshots reach phones
│  │     └─ phones/
│  │        ├─ mod.rs             (new)        session, Mac commands, phone messages, ticker
│  │        ├─ server.rs          (new)        HTTPS routes, phone page, pictures, one socket per phone
│  │        ├─ cert.rs            (new)        certificate kept/remade, LAN addresses
│  │        ├─ room.rs            (new)        join code, up to 4 rows, reconnects
│  │        └─ output.rs          (new)        cpal output stream playing the Mixer
│  ├─ src/
│  │  ├─ styles/tokens.css                     + --qr-dark, --qr-light
│  │  ├─ routes/phone/+page.svelte (new)       the phone page
│  │  └─ lib/
│  │     ├─ api.ts                             QueueEntry.by, phones commands and event, problem codes
│  │     ├─ art.ts                             /art/ links pass through (phone)
│  │     ├─ format.ts                          loudness()
│  │     ├─ i18n/*.ts                          mics.*, phone.*, queue.addedBy, problem.phonesStart/noNetwork, common.done
│  │     ├─ state/phones.svelte.ts (new)       the Mac window's view of the session
│  │     ├─ state/player.svelte.ts             reports the song clock to phones; a guest's song starts when idle
│  │     ├─ audio/streamer.ts                  clock() for phones
│  │     ├─ state/ui.svelte.ts                 sheet kind "mics"
│  │     ├─ components/QueueList.svelte (new)  queue list shared by the Mac panel and the phone
│  │     ├─ components/QueuePanel.svelte       uses QueueList
│  │     ├─ components/MicPill.svelte (new)    top-right pill (library and karaoke view)
│  │     ├─ components/MicsSheet.svelte (new)  "Sing into your phone" window
│  │     ├─ components/Sheet.svelte            + subtitle, wide
│  │     ├─ components/Karaoke.svelte          pill in the top-right cell
│  │     └─ phone/                 (new)
│  │        ├─ link.svelte.ts                  the phone's connection, mic and shared state
│  │        ├─ mic.ts, capture-worklet.ts      mic capture into 16-bit frames
│  │        ├─ phone.css                       shared phone styles from the prototype
│  │        ├─ Center.svelte, JoinScreen.svelte
│  │        ├─ MicTab.svelte, PhoneLyrics.svelte
│  │        └─ SongsTab.svelte
│  └─ tests/
│     ├─ fake-backend.ts                       + phones commands, guestAdds, levels, news levers
│     ├─ fake-phone.ts            (new)        in-page fake computer, mic, wake lock
│     ├─ phone.ts                 (new)        phone fixture and helpers
│     ├─ mics.spec.ts             (new)
│     ├─ phone.spec.ts            (new)
│     └─ queue.spec.ts                         + guest's name in the queue
```

## Phone protocol (summary; each task's Interfaces block is authoritative)

One WebSocket per phone at `wss://<mac>/ws`. Text frames are JSON with a `t` field; binary frames are the phone's mic as 16-bit little-endian mono PCM at the rate it announced.

Phone → Mac: `{t:"join", code, id, name}` (must be first) · `{t:"ping"}` (every second) · `{t:"live", on, rate}` · `{t:"voice", v}` · `{t:"effect", kind, amount}` · `{t:"singer", v}` · `{t:"add", trackId, next}` · `{t:"addLink", url, next}` · `{t:"move", key, to}` · `{t:"remove", key}` · `{t:"search", q, imported}` · `{t:"youtube", q}` · `{t:"preview", url}` · `{t:"lyrics", trackId}` · `{t:"leave"}`.

Mac → phone: `{t:"joined"}` · `{t:"player", snapshot}` (the app's `PlayerSnapshot`, song pictures as `/art/<file>`) · `{t:"clock", key, positionMs, playing}` · `{t:"lyrics", trackId, lyrics}` · `{t:"lyricsChanged", trackId}` · `{t:"results", q, outcome}` · `{t:"youtube", q, hits}` · `{t:"preview", url, preview}` · `{t:"level", v}` (every 80 ms) · `{t:"refused", problem}`. Close codes: 4001 ended/removed, 4002 full, 4003 wrong code.

Mac window events: `"phones"` (view, on changes) · `"phone-levels"` (every 80 ms) · `"phone-news"` (`joined` / `added`) · `"library"` (a guest's link entered or left the library).

---

### Task 1: A phone's jitter buffer — adaptive delay, cubic resampling, fades

**Files:**
- Create: `crates/kara-core/src/mic/mod.rs`
- Create: `crates/kara-core/src/mic/buffer.rs`
- Modify: `crates/kara-core/src/lib.rs` (add `pub mod mic;` after `pub mod lyrics;`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `kara_core::mic::JitterBuffer` with `new(in_rate: u32, out_rate: u32, floor_ms: f64) -> Self` (allocates its whole queue up front), `push(&mut self, samples: &[f32])` (never grows the queue: past 200 ms waiting — nobody is reading, e.g. the output device went away — it starts over), `pull(&mut self, out: &mut [f32])` (mono, output rate), `reset(&mut self)`, `fill_ms(&self) -> f64`, `target_ms(&self) -> f64`.

- [ ] **Step 1: Write the failing tests**

Create `crates/kara-core/src/mic/mod.rs`:

```rust
//! Phone mics: turning each phone's sound into one mix for the Mac's speakers.

mod buffer;

pub use buffer::JitterBuffer;
```

Create `crates/kara-core/src/mic/buffer.rs` with only the tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const OUT: u32 = 48_000;
    const FRAME: usize = 256;

    struct Run {
        out: Vec<f32>,
        /// (seconds, fill ms, target ms) after each output block.
        trace: Vec<(f64, f64, f64)>,
        buffer: JitterBuffer,
    }

    /// Plays `secs` of a 440 Hz tone from a phone that says it sends at `nominal` Hz while its clock makes `actual`;
    /// `arrive` says when a frame finished at a given time reaches the Mac.
    fn run(nominal: u32, actual: f64, secs: f64, arrive: impl Fn(f64) -> f64) -> Run {
        let mut frames = Vec::new();
        let mut n = 0;
        while n as f64 / actual < secs {
            let frame: Vec<f32> = (n..n + FRAME).map(|i| 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / nominal as f32).sin()).collect();
            n += FRAME;
            frames.push((arrive(n as f64 / actual), frame));
        }
        frames.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut buffer = JitterBuffer::new(nominal, OUT, 20.0);
        let (mut out, mut trace, mut next) = (Vec::new(), Vec::new(), 0);
        let mut block = [0.0; FRAME];
        for j in 0..(secs * OUT as f64 / FRAME as f64) as usize {
            let t = (j * FRAME) as f64 / OUT as f64;
            while next < frames.len() && frames[next].0 <= t {
                buffer.push(&frames[next].1);
                next += 1;
            }
            buffer.pull(&mut block);
            out.extend_from_slice(&block);
            trace.push((t, buffer.fill_ms(), buffer.target_ms()));
        }
        Run { out, trace, buffer }
    }

    /// No step between neighboring samples bigger than the tone itself makes; a click is about 0.5.
    fn smooth(out: &[f32]) -> bool {
        out.windows(2).all(|w| (w[1] - w[0]).abs() < 0.06)
    }

    #[test]
    fn keeps_the_delay_steady_when_the_phone_clock_drifts_or_runs_at_44_1_khz() {
        for (nominal, actual) in [(48_000, 48_048.0), (48_000, 47_952.0), (44_100, 44_100.0)] {
            let r = run(nominal, actual, 60.0, |t| t + 0.004);
            assert!(r.out[..(0.015 * OUT as f64) as usize].iter().all(|&x| x == 0.0), "{nominal}/{actual}: silent while filling up");
            assert!(smooth(&r.out), "{nominal}/{actual}: no clicks");
            assert_eq!(r.buffer.target_ms(), 20.0, "{nominal}/{actual}: never ran dry");
            let off = r.trace.iter().filter(|(t, ..)| *t > 30.0).map(|(_, fill, _)| (fill - 20.0).abs()).fold(0.0, f64::max);
            assert!(off < 8.0, "{nominal}/{actual}: the delay stays near 20 ms, it was off by {off:.1} ms");
        }
    }

    #[test]
    fn a_phone_nobody_hears_holds_little_memory() {
        let mut b = JitterBuffer::new(48_000, 48_000, 20.0);
        let room = b.queue.capacity();
        for _ in 0..10 * 48_000 / 256 {
            b.push(&[0.1; 256]);
        }
        assert!(b.fill_ms() <= HOLD_MS && b.queue.capacity() == room, "10 s unread stays within {HOLD_MS} ms without growing");
    }

    #[test]
    fn a_wifi_stall_fades_out_grows_the_target_and_a_burst_is_trimmed_back() {
        let r = run(48_000, 48_000.0, 70.0, |t| if (5.0..5.15).contains(&t) { 5.15 } else { t + 0.004 });
        assert!(smooth(&r.out), "fades instead of clicking");
        let (_, _, target) = *r.trace.iter().find(|(t, ..)| *t >= 6.0).unwrap();
        assert_eq!(target, 30.0, "the stall grew the target");
        let late = r.trace.iter().filter(|(t, ..)| (5.5..6.0).contains(t)).map(|(_, fill, _)| *fill).fold(0.0, f64::max);
        assert!(late < 40.0, "the burst was trimmed back near the target, the delay was {late:.1} ms");
        assert_eq!(r.buffer.target_ms(), 20.0, "a steady minute shrinks the target back to the floor");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core mic::buffer`
Expected: FAIL to compile — `cannot find type JitterBuffer`.

- [ ] **Step 3: Write the implementation**

Put this above the tests in `crates/kara-core/src/mic/buffer.rs`:

```rust
//! A phone's jitter buffer: holds its samples, resamples them to the output rate, and keeps the delay near a target that adapts to the Wi-Fi.

use std::collections::VecDeque;

const FADE_MS: f64 = 5.0;
const GROW_MS: f64 = 10.0;
const MAX_TARGET_MS: f64 = 60.0;
const SHRINK_MS: f64 = 2.0;
const STEADY_SECS: f64 = 10.0;
const TRIM_OVER_MS: f64 = 30.0;
const MAX_SKEW: f64 = 0.005;
/// The most sound the buffer ever holds; far above the 60 ms target plus a trimmed burst.
const HOLD_MS: f64 = 200.0;

pub struct JitterBuffer {
    queue: VecDeque<f32>,
    cap: usize,
    /// Read position in `queue`; at least 1 so the interpolation has a sample behind it.
    pos: f64,
    step: f64,
    in_rate: f64,
    out_rate: f64,
    floor_ms: f64,
    target_ms: f64,
    avg_fill_ms: f64,
    playing: bool,
    fade_in: f64,
    steady_secs: f64,
    /// A jump ahead after a burst: where to, and how far the crossfade has got (0 to 1).
    jump: Option<(f64, f64)>,
}

impl JitterBuffer {
    pub fn new(in_rate: u32, out_rate: u32, floor_ms: f64) -> Self {
        let cap = (HOLD_MS / 1000.0 * in_rate as f64) as usize;
        let mut queue = VecDeque::with_capacity(cap);
        queue.push_back(0.0);
        Self {
            queue,
            cap,
            pos: 1.0,
            step: in_rate as f64 / out_rate as f64,
            in_rate: in_rate as f64,
            out_rate: out_rate as f64,
            floor_ms,
            target_ms: floor_ms,
            avg_fill_ms: floor_ms,
            playing: false,
            fade_in: 0.0,
            steady_secs: 0.0,
            jump: None,
        }
    }

    /// Adds a phone's samples; when more than HOLD_MS would be waiting (nobody is reading), it starts over instead of growing.
    pub fn push(&mut self, samples: &[f32]) {
        if self.queue.len() + samples.len() > self.cap {
            self.reset();
        }
        self.queue.extend(samples);
    }

    /// Drops everything waiting; it fills up to the target again before playing.
    pub fn reset(&mut self) {
        self.queue.clear();
        self.queue.push_back(0.0);
        self.pos = 1.0;
        self.playing = false;
        self.jump = None;
    }

    /// Milliseconds of sound waiting to play.
    pub fn fill_ms(&self) -> f64 {
        (self.queue.len() as f64 - self.pos - 2.0).max(0.0) / self.in_rate * 1000.0
    }

    pub fn target_ms(&self) -> f64 {
        self.target_ms
    }

    /// Fills `out` at the output rate: silence while filling up, a short fade instead of a click when the sound runs out.
    pub fn pull(&mut self, out: &mut [f32]) {
        let fade = FADE_MS / 1000.0 * self.out_rate;
        for o in out {
            *o = 0.0;
            let fill = self.fill_ms();
            if !self.playing {
                if fill < self.target_ms {
                    continue;
                }
                self.playing = true;
                self.fade_in = 0.0;
                self.avg_fill_ms = fill;
            }
            let left = fill / 1000.0 * self.out_rate;
            if left < 1.0 {
                self.playing = false;
                self.target_ms = (self.target_ms + GROW_MS).min(MAX_TARGET_MS);
                self.steady_secs = 0.0;
                continue;
            }
            if self.jump.is_none() && fill > self.target_ms + TRIM_OVER_MS {
                self.jump = Some((self.pos + (fill - self.target_ms) / 1000.0 * self.in_rate, 0.0));
                self.avg_fill_ms = self.target_ms;
            }
            self.avg_fill_ms += (fill - self.avg_fill_ms) / self.out_rate;
            let skew = ((self.avg_fill_ms - self.target_ms) / self.target_ms * 0.01).clamp(-MAX_SKEW, MAX_SKEW);
            let mut x = self.at(self.pos);
            if let Some((to, k)) = self.jump {
                x = x * (1.0 - k) as f32 + self.at(to) * k as f32;
            }
            self.fade_in = (self.fade_in + 1.0 / fade).min(1.0);
            *o = x * self.fade_in.min(left / fade) as f32;
            self.advance(self.step * (1.0 + skew), fade);
            self.steady_secs += 1.0 / self.out_rate;
            if self.steady_secs >= STEADY_SECS {
                self.steady_secs = 0.0;
                self.target_ms = (self.target_ms - SHRINK_MS).max(self.floor_ms);
            }
        }
    }

    fn advance(&mut self, step: f64, fade: f64) {
        self.pos += step;
        if let Some((to, k)) = &mut self.jump {
            *to += step;
            *k += 1.0 / fade;
            if *k >= 1.0 {
                self.pos = *to;
                self.jump = None;
            }
        }
        let done = self.pos as usize - 1;
        if done > 0 {
            self.queue.drain(..done);
            self.pos -= done as f64;
            if let Some((to, _)) = &mut self.jump {
                *to -= done as f64;
            }
        }
    }

    /// The sound at fractional position `p`, by cubic interpolation.
    fn at(&self, p: f64) -> f32 {
        let i = p as usize;
        let t = (p - i as f64) as f32;
        let [a, b, c, d] = [i - 1, i, i + 1, i + 2].map(|k| self.queue.get(k).copied().unwrap_or(0.0));
        let c1 = 0.5 * (c - a);
        let c2 = a - 2.5 * b + 2.0 * c - 0.5 * d;
        let c3 = 0.5 * (d - a) + 1.5 * (b - c);
        ((c3 * t + c2) * t + c1) * t + b
    }
}
```

Add `pub mod mic;` to `crates/kara-core/src/lib.rs` after `pub mod lyrics;`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core mic::buffer`
Expected: PASS (3 tests). If a bound is missed, print the trace around the failing time and fix the code, not the bound: the bounds come from the spec (20 ms start, 10 ms growth, trim after bursts, 2 ms per 10 s shrink) and the tone's own slope.

- [ ] **Step 5: Lint and commit**

Run: `source "$HOME/.cargo/env" && cargo clippy -p kara-core --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/kara-core/src/lib.rs crates/kara-core/src/mic/mod.rs crates/kara-core/src/mic/buffer.rs
git commit -m "feat(core): a phone's jitter buffer with adaptive delay, resampling and fades"
```

---

### Task 2: Howl control — turn a feeding-back mic down until it settles

**Files:**
- Create: `crates/kara-core/src/mic/howl.rs`
- Modify: `crates/kara-core/src/mic/mod.rs`

**Interfaces:**
- Consumes: `realfft` (already a kara-core dependency; see `separate/stft.rs` for its use).
- Produces: `kara_core::mic::Howl` with `new(rate: u32) -> Self`, `feed(&mut self, samples: &[f32])`, `gain(&self) -> f32` (1.0, or less while turned down, never below 1/16), `down(&self) -> bool`.

- [ ] **Step 1: Write the failing tests**

In `crates/kara-core/src/mic/mod.rs` add `mod howl;` under `mod buffer;` and `pub use howl::Howl;` under the other `pub use`.

Create `crates/kara-core/src/mic/howl.rs` with the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: f32 = 48_000.0;

    /// A repeatable hiss at about -50 dBFS.
    fn hiss(n: usize) -> Vec<f32> {
        let mut s = 12_345u32;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 8) as f32 / (1u32 << 24) as f32 * 0.006 - 0.003
            })
            .collect()
    }

    /// `secs` of a `hz` sine whose amplitude follows `amp(t)`, over the hiss.
    fn tone(hz: f32, secs: f32, amp: impl Fn(f32) -> f32) -> Vec<f32> {
        hiss((secs * RATE) as usize)
            .into_iter()
            .enumerate()
            .map(|(i, h)| {
                let t = i as f32 / RATE;
                h + amp(t) * (TAU * hz * t).sin()
            })
            .collect()
    }

    /// Seconds until `h` turned the mic down while hearing `signal`, if it did.
    fn turned_down_after(h: &mut Howl, signal: &[f32]) -> Option<f32> {
        signal
            .chunks(256)
            .position(|c| {
                h.feed(c);
                h.down()
            })
            .map(|i| (i * 256) as f32 / RATE)
    }

    #[test]
    fn a_growing_or_screaming_tone_is_turned_down_within_a_second_then_comes_back() {
        let growing = tone(2_500.0, 2.0, |t| (0.01 * 10f32.powf(1.5 * t)).min(0.9));
        let screaming = tone(2_500.0, 2.0, |_| 0.8);
        for (what, signal) in [("growing", growing), ("screaming", screaming)] {
            let mut h = Howl::new(48_000);
            let when = turned_down_after(&mut h, &signal).unwrap_or(f32::INFINITY);
            assert!(when <= 1.0, "{what}: turned down after {when} s");
            h.feed(&hiss(8 * 48_000));
            assert!(!h.down(), "{what}: back to full once it settled");
        }
    }

    #[test]
    fn singing_and_a_steady_held_note_are_left_alone() {
        let mut voice = hiss((5.0 * RATE) as usize);
        let mut phase = 0.0f32;
        for (i, x) in voice.iter_mut().enumerate() {
            let t = i as f32 / RATE;
            phase += TAU * 220.0 * (1.0 + 0.02 * (TAU * 5.5 * t).sin()) / RATE;
            let crescendo_then_held = 0.02 + 0.08 * t.min(1.0);
            *x += crescendo_then_held * (1..=12).map(|h| (phase * h as f32).sin() / h as f32).sum::<f32>();
        }
        let held = tone(1_000.0, 3.0, |_| 0.1);
        let mut h = Howl::new(48_000);
        assert_eq!(turned_down_after(&mut h, &voice), None, "singing");
        assert_eq!(turned_down_after(&mut h, &held), None, "a held note");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core mic::howl`
Expected: FAIL to compile — `cannot find type Howl`.

- [ ] **Step 3: Write the implementation**

Put this above the tests in `crates/kara-core/src/mic/howl.rs`:

```rust
//! Feedback control for one mic: spots a single pure tone that lasts and keeps growing (or already screams) and turns the mic down until it settles.

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};
use std::ops::Range;
use std::sync::Arc;

const N: usize = 1024;
const LOW_HZ: f32 = 150.0;
const HIGH_HZ: f32 = 8_000.0;
/// Frames (about a quarter second at 48 kHz) a tone must last before the mic is turned down.
const RUN: u32 = 12;
const DOMINANCE: f32 = 100.0;
const LOUD: f32 = 0.01;
const SCREAMING: f32 = 0.5;
const GROWN: f32 = 4.0;
const CUT: f32 = 0.5;
const MIN_GAIN: f32 = 1.0 / 16.0;
/// Frames (about two seconds) without a tone before the gain starts coming back.
const SETTLE: u32 = 96;
const RECOVER: f32 = 1.012;

pub struct Howl {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    window_sum: f32,
    frame: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    bins: Range<usize>,
    bin: usize,
    run: u32,
    first_power: f32,
    quiet: u32,
    gain: f32,
}

impl Howl {
    pub fn new(rate: u32) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(N);
        let window: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
        let hz = rate as f32 / N as f32;
        Self {
            window_sum: window.iter().sum(),
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            bins: (LOW_HZ / hz) as usize..(HIGH_HZ / hz) as usize,
            fft,
            window,
            frame: Vec::with_capacity(N),
            bin: 0,
            run: 0,
            first_power: 0.0,
            quiet: 0,
            gain: 1.0,
        }
    }

    /// The mic's gain: 1, or less while it is turned down.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn down(&self) -> bool {
        self.gain < 0.99
    }

    pub fn feed(&mut self, samples: &[f32]) {
        for &s in samples {
            self.frame.push(s);
            if self.frame.len() == N {
                self.analyze();
                self.frame.clear();
            }
        }
    }

    /// Looks at one frame: a lasting single tone that grew or screams cuts the gain; two quiet seconds let it come back.
    fn analyze(&mut self) {
        for ((x, s), w) in self.input.iter_mut().zip(&self.frame).zip(&self.window) {
            *x = s * w;
        }
        if self.fft.process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch).is_err() {
            return;
        }
        let power = |k: usize| self.spectrum[k].norm_sqr();
        let Some((bin, peak)) = self.bins.clone().map(|k| (k, power(k))).max_by(|a, b| a.1.total_cmp(&b.1)) else { return };
        let others = self.bins.clone().filter(|k| k.abs_diff(bin) > 2).map(power).fold(0.0, f32::max);
        let amplitude = 2.0 * peak.sqrt() / self.window_sum;
        if amplitude < LOUD || peak < DOMINANCE * others {
            self.run = 0;
            self.quiet += 1;
            if self.quiet >= SETTLE {
                self.gain = (self.gain * RECOVER).min(1.0);
            }
            return;
        }
        self.quiet = 0;
        if self.run == 0 || bin.abs_diff(self.bin) > 1 {
            self.run = 0;
            self.first_power = peak;
        }
        self.run += 1;
        self.bin = bin;
        if self.run >= RUN && (peak >= GROWN * self.first_power || amplitude >= SCREAMING) {
            self.gain = (self.gain * CUT).max(MIN_GAIN);
            self.run = 0;
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core mic::howl`
Expected: PASS (2 tests).

- [ ] **Step 5: Lint and commit**

Run: `source "$HOME/.cargo/env" && cargo clippy -p kara-core --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/kara-core/src/mic/mod.rs crates/kara-core/src/mic/howl.rs
git commit -m "feat(core): turn a mic down when it starts to howl"
```

---

### Task 3: The phones' mix — gain cap, reverb, limiter, levels

**Files:**
- Modify: `crates/kara-core/src/mic/mod.rs`

**Interfaces:**
- Consumes: `JitterBuffer` (Task 1), `Howl` (Task 2).
- Produces (all in `kara_core::mic`):
  - `pub const MAX_GAIN: f32 = 2.0;`
  - `pub fn voice_gain(volume: u8, voice: u8) -> f32` — `MAX_GAIN × min(volume,100)/100 × min(voice,100)/100`.
  - `pub struct Level { pub id: Arc<str>, pub peak: f32, pub down: bool }` — a phone's loudest post-gain sample since the last read, and whether feedback control has it turned down.
  - `pub struct NewVoice` with `NewVoice::new(id: &str, in_rate: u32, out_rate: u32, floor_ms: f64)` — a phone's id, buffer and feedback control, built **outside** the mixer lock.
  - `pub struct Gone` — what a removed phone leaves behind, dropped by the caller after unlocking.
  - `pub struct Mixer` with `new(rate: u32, floor_ms: f64)`, `rate(&self) -> u32`, `floor_ms(&self) -> f64`, `add(&mut self, new: NewVoice) -> Option<NewVoice>` (a returning phone swaps in only the fresh buffer and keeps its gain and howl state; the leftovers come back for dropping outside the lock), `remove(&mut self, id: &str) -> Option<Gone>`, `push(&mut self, id: &str, samples: &[f32])`, `reset(&mut self, id: &str)`, `set_gain(&mut self, id: &str, gain: f32)`, `render(&mut self, out: &mut [f32])` (mono), `levels(&mut self, out: &mut Vec<Level>)` (fills a buffer the caller keeps).
  - Real-time rule: the audio callback locks the mixer (a `std::sync::Mutex`, by choice: holders are short). Everything else done under that lock is allocation-free and short — no building, freeing or growing: voices are built before locking (`NewVoice`), leftovers and removed voices are freed after unlocking (`Option<NewVoice>`, `Gone`), buffers are sized up front (Task 1), and levels are copied into the caller's reused buffer with `Arc<str>` ids.

- [ ] **Step 1: Write the failing test**

Replace `crates/kara-core/src/mic/mod.rs` with the module header and test below (the implementation follows in Step 3):

```rust
//! Phone mics: each phone's jitter buffer, gain and feedback control, summed through a shared reverb and a limiter.

mod buffer;
mod howl;

pub use buffer::JitterBuffer;
pub use howl::Howl;
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_phones_at_full_volume_never_pass_the_ceiling_and_the_reverb_dies_away() {
        assert_eq!(voice_gain(255, 255), MAX_GAIN);
        let ids = ["a", "b", "c", "d"];
        let mut m = Mixer::new(48_000, 20.0);
        for id in ids {
            m.add(NewVoice::new(id, 48_000, 48_000, 20.0));
            m.set_gain(id, voice_gain(100, 100));
        }
        let loud: Vec<f32> = (0..48_000).map(|i| if (i / 80) % 2 == 0 { 0.99 } else { -0.99 }).collect();
        let mut out = vec![0.0; 4 * 48_000];
        for (i, block) in out.chunks_mut(256).enumerate() {
            if let Some(part) = loud.get(i * 256..(i + 1) * 256) {
                for id in ids {
                    m.push(id, part);
                }
            }
            m.render(block);
        }
        let peak = out.iter().fold(0f32, |p, x| p.max(x.abs()));
        assert!(peak > 0.5 && peak <= CEILING + 1e-6, "peak {peak}");
        assert!(out[out.len() - 4_800..].iter().all(|x| x.abs() < 1e-3), "the reverb tail dies away");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core mic::tests`
Expected: FAIL to compile — `cannot find function voice_gain`.

- [ ] **Step 3: Write the implementation**

Insert between the `pub use` lines and the tests in `crates/kara-core/src/mic/mod.rs`:

```rust
pub const MAX_GAIN: f32 = 2.0;
const WET: f32 = 0.2;
const CEILING: f32 = 0.89;

/// A phone's gain from the Mac's volume slider and the phone's Voice slider (0–100 each), never above MAX_GAIN.
pub fn voice_gain(volume: u8, voice: u8) -> f32 {
    MAX_GAIN * f32::from(volume.min(100)) / 100.0 * f32::from(voice.min(100)) / 100.0
}

/// Samples one output callback may ask for; each phone's scratch buffer is this big from the start.
const MOST_FRAMES: usize = 4096;
const MOST_PHONES: usize = 8;

#[derive(Clone)]
pub struct Level {
    pub id: Arc<str>,
    pub peak: f32,
    pub down: bool,
}

/// A phone's id, buffer and feedback control, made before taking the mixer lock.
pub struct NewVoice {
    id: Arc<str>,
    buffer: JitterBuffer,
    howl: Howl,
}

impl NewVoice {
    pub fn new(id: &str, in_rate: u32, out_rate: u32, floor_ms: f64) -> Self {
        Self { id: id.into(), buffer: JitterBuffer::new(in_rate, out_rate, floor_ms), howl: Howl::new(out_rate) }
    }
}

struct Voice {
    id: Arc<str>,
    buffer: JitterBuffer,
    howl: Howl,
    gain: f32,
    peak: f32,
    scratch: Vec<f32>,
}

/// A removed phone's voice, freed wherever the caller drops it.
pub struct Gone {
    _voice: Voice,
}

pub struct Mixer {
    rate: u32,
    floor_ms: f64,
    voices: Vec<Voice>,
    reverb: Reverb,
    limiter: Limiter,
}

impl Mixer {
    pub fn new(rate: u32, floor_ms: f64) -> Self {
        Self { rate, floor_ms, voices: Vec::with_capacity(MOST_PHONES), reverb: Reverb::new(rate), limiter: Limiter::new(rate) }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn floor_ms(&self) -> f64 {
        self.floor_ms
    }

    /// Starts taking a phone's sound. A phone already here swaps in only the fresh buffer and keeps its gain and feedback state;
    /// what it doesn't use comes back, to be dropped after unlocking.
    pub fn add(&mut self, mut new: NewVoice) -> Option<NewVoice> {
        let id = new.id.clone();
        if let Some(v) = self.voice(&id) {
            std::mem::swap(&mut v.buffer, &mut new.buffer);
            return Some(new);
        }
        let NewVoice { id, buffer, howl } = new;
        self.voices.push(Voice { id, buffer, howl, gain: 1.0, peak: 0.0, scratch: Vec::with_capacity(MOST_FRAMES) });
        None
    }

    /// Takes a phone out of the mix; drop what comes back after unlocking.
    pub fn remove(&mut self, id: &str) -> Option<Gone> {
        let i = self.voices.iter().position(|v| &*v.id == id)?;
        Some(Gone { _voice: self.voices.swap_remove(i) })
    }

    pub fn push(&mut self, id: &str, samples: &[f32]) {
        if let Some(v) = self.voice(id) {
            v.buffer.push(samples);
        }
    }

    pub fn reset(&mut self, id: &str) {
        if let Some(v) = self.voice(id) {
            v.buffer.reset();
        }
    }

    pub fn set_gain(&mut self, id: &str, gain: f32) {
        if let Some(v) = self.voice(id) {
            v.gain = gain.min(MAX_GAIN);
        }
    }

    /// Mixes the next `out.len()` samples of every phone, with reverb, never above the ceiling.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        for v in &mut self.voices {
            v.scratch.resize(out.len(), 0.0);
            v.buffer.pull(&mut v.scratch);
            v.howl.feed(&v.scratch);
            let gain = v.gain * v.howl.gain();
            for (o, s) in out.iter_mut().zip(&v.scratch) {
                let x = s * gain;
                v.peak = v.peak.max(x.abs());
                *o += x;
            }
        }
        for o in out.iter_mut() {
            *o = self.limiter.process(*o + WET * self.reverb.process(*o));
        }
    }

    /// Each phone's loudest moment since the last call, into `out` (cleared first; its room is reused).
    pub fn levels(&mut self, out: &mut Vec<Level>) {
        out.clear();
        out.extend(self.voices.iter_mut().map(|v| Level { id: v.id.clone(), peak: std::mem::take(&mut v.peak), down: v.howl.down() }));
    }

    fn voice(&mut self, id: &str) -> Option<&mut Voice> {
        self.voices.iter_mut().find(|v| &*v.id == id)
    }
}

const COMB_INPUT: f32 = 0.25;
const COMB_FEEDBACK: f32 = 0.78;
const COMB_DAMP: f32 = 0.25;
const ALLPASS_FEEDBACK: f32 = 0.5;

struct Comb {
    buf: Vec<f32>,
    i: usize,
    low: f32,
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

/// A small room: four damped combs into two allpasses, the Freeverb layout.
struct Reverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
}

impl Reverb {
    fn new(rate: u32) -> Self {
        let len = |n: usize| n * rate as usize / 44_100;
        Self {
            combs: [1116, 1188, 1277, 1356].map(|n| Comb { buf: vec![0.0; len(n)], i: 0, low: 0.0 }).into(),
            allpasses: [556, 441].map(|n| Allpass { buf: vec![0.0; len(n)], i: 0 }).into(),
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let mut out = 0.0;
        for c in &mut self.combs {
            let y = c.buf[c.i];
            c.low = y * (1.0 - COMB_DAMP) + c.low * COMB_DAMP;
            c.buf[c.i] = x * COMB_INPUT + c.low * COMB_FEEDBACK;
            c.i = (c.i + 1) % c.buf.len();
            out += y;
        }
        for a in &mut self.allpasses {
            let y = a.buf[a.i];
            a.buf[a.i] = out + y * ALLPASS_FEEDBACK;
            a.i = (a.i + 1) % a.buf.len();
            out = y - out;
        }
        out
    }
}

/// Keeps the mix under the ceiling: turns down at once, comes back over about 80 ms.
struct Limiter {
    gain: f32,
    release: f32,
}

impl Limiter {
    fn new(rate: u32) -> Self {
        Self { gain: 1.0, release: 1.0 - (-1.0 / (0.08 * rate as f32)).exp() }
    }

    fn process(&mut self, x: f32) -> f32 {
        let room = if x.abs() > CEILING { CEILING / x.abs() } else { 1.0 };
        self.gain = room.min(self.gain + (1.0 - self.gain) * self.release);
        x * self.gain
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core mic::`
Expected: PASS (6 tests: buffer 3, howl 2, mixer 1).

- [ ] **Step 5: Lint and commit**

Run: `source "$HOME/.cargo/env" && cargo clippy -p kara-core --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/kara-core/src/mic/mod.rs
git commit -m "feat(core): mix phones with a gain cap, reverb and a limiter"
```

---
### Task 4: Voice effects — karaoke mix and auto-tune, per phone, before the mix

**Files:**
- Create: `crates/kara-core/src/mic/effects.rs`
- Modify: `crates/kara-core/src/mic/mod.rs`

**Interfaces:**
- Consumes: Task 3's private `Reverb` (`Reverb::new(rate)`, `process(x) -> f32`), `NewVoice`, `Mixer`.
- Produces (in `kara_core::mic`):
  - `pub enum Effect { None, KaraokeMix, AutoTune }` — `Deserialize` from `"none" | "karaokeMix" | "autoTune"`, `Default` = `None`.
  - `pub struct Effects` with `new(rate: u32)`, `set(&mut self, effect: Effect, amount: u8)` (intensity 0–100; 0 means no effect), `process(&mut self, samples: &mut [f32])` (in place; no allocation, no locking).
  - `NewVoice` also builds the phone's `Effects`; `Mixer::set_effect(&mut self, id: &str, effect: Effect, amount: u8)`; `render` applies each phone's effect after howl control hears the dry voice and before its gain.
  - Behavior:
    - No effect, and any effect at intensity 0, leave the voice untouched. Every change of effect or intensity fades over 20 ms, so switching never clicks (a new effect fades in only after the old one has faded out).
    - Karaoke mix = the voice + intensity × (a 120 ms echo with feedback 0.35 + a low-passed "warm" room reverb).
    - Auto-tune = chromatic. YIN pitch detection runs on the last ~33 ms, decimated to a quarter rate, every 256 samples. The target is the nearest semitone (A4 = 440 Hz). Intensity sets how far (0–100 % of the way) and how fast (about 50 ms at 1 %, 5 ms at 100 %) it pulls.
    - The auto-tune shift is two delay-line taps crossfaded over overlapping lives. Each tap restarts a **whole number of pitch periods** (at least 3.5 ms) from the other, so crossfades stay in phase and the note lands on the semitone at every pitch. The added delay is about twice that spacing: at most ~12 ms for most voices (≥ ~200 Hz), and up to ~25 ms for low voices (the spec, as the user chose). While auto-tune is off, the tap line keeps hearing the voice, so turning it on replays nothing old.

- [ ] **Step 1: Write the failing tests**

In `crates/kara-core/src/mic/mod.rs` add `mod effects;` and `pub use effects::{Effect, Effects};` next to the other modules.

Create `crates/kara-core/src/mic/effects.rs` with its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: f32 = 48_000.0;
    const PITCHES: [f32; 5] = [98.0, 110.0, 131.0, 262.0, 440.0];

    fn sine(hz: f32, secs: f32) -> Vec<f32> {
        (0..(secs * RATE) as usize).map(|i| 0.5 * (TAU * hz * i as f32 / RATE).sin()).collect()
    }

    /// A made-up sung note: ten harmonics with a slight vibrato.
    fn voice(hz: f32, secs: f32) -> Vec<f32> {
        let mut phase = 0.0f32;
        (0..(secs * RATE) as usize)
            .map(|i| {
                phase += TAU * hz * (1.0 + 0.004 * (TAU * 5.0 * i as f32 / RATE).sin()) / RATE;
                0.3 * (1..=10).map(|h| (phase * h as f32).sin() / h as f32).sum::<f32>()
            })
            .collect()
    }

    /// The frequency of `x`, from its rising zero crossings.
    fn hz(x: &[f32]) -> f32 {
        let ups: Vec<f32> = x.windows(2).enumerate().filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0).map(|(i, w)| i as f32 + w[0] / (w[0] - w[1])).collect();
        (ups.len() - 1) as f32 * RATE / (ups[ups.len() - 1] - ups[0])
    }

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    /// The semitone nearest `hz`.
    fn note(hz: f32) -> f32 {
        440.0 * 2f32.powf((12.0 * (hz / 440.0).log2()).round() / 12.0)
    }

    fn run(fx: &mut Effects, mut x: Vec<f32>) -> Vec<f32> {
        for block in x.chunks_mut(256) {
            fx.process(block);
        }
        x
    }

    fn with(effect: Effect, amount: u8, x: Vec<f32>) -> Vec<f32> {
        let mut fx = Effects::new(48_000);
        fx.set(effect, amount);
        run(&mut fx, x)
    }

    /// No step between neighboring samples bigger than a quiet 440 Hz tone with its echoes makes; a click is several times that.
    fn smooth(out: &[f32]) -> bool {
        out.windows(2).all(|w| (w[1] - w[0]).abs() < 0.06)
    }

    #[test]
    fn auto_tune_pulls_low_and_high_notes_to_the_nearest_semitone_as_hard_as_asked() {
        for sung in PITCHES {
            let target = note(sung);
            for amount in [100u8, 50] {
                for off in [-30.0, 30.0] {
                    let out = hz(&with(Effect::AutoTune, amount, sine(target * 2f32.powf(off / 1200.0), 2.5))[(0.4 * RATE) as usize..]);
                    let (now, want) = (cents(out, target), off * (1.0 - f32::from(amount) / 100.0));
                    assert!((now - want).abs() < 3.0, "{sung} Hz sung {off} cents off at intensity {amount}: {now:.1} cents off, want {want}");
                }
            }
        }
    }

    #[test]
    fn auto_tune_delays_most_voices_about_12_ms_and_low_voices_at_most_25() {
        for sung in PITCHES {
            let mut t = Tune::new(48_000);
            t.set(1.0);
            let mut most = 0.0f32;
            for x in sine(note(sung) * 2f32.powf(-30.0 / 1200.0), 2.0) {
                t.next(x);
                most = most.max(t.delay_now());
            }
            let bound = if sung >= 200.0 { 12.0 } else { 25.0 };
            assert!(most / RATE * 1000.0 <= bound, "{sung} Hz: delayed {:.1} ms while correcting", most / RATE * 1000.0);
        }
    }

    #[test]
    fn auto_tune_keeps_going_through_low_sung_notes_and_reads_right_at_the_edge_of_its_line() {
        let t = Tune::new(48_000);
        assert!(t.read(1e-6).is_finite(), "a delay a hair above zero at the start of the line");
        let mut fx = Effects::new(48_000);
        fx.set(Effect::AutoTune, 100);
        for sung in [110.0, 147.0, 196.0, 220.0] {
            for bend in [0.985, 1.0, 1.012] {
                assert!(run(&mut fx, voice(sung * bend, 2.0)).iter().all(|x| x.is_finite()));
            }
        }
    }

    #[test]
    fn karaoke_mix_is_dry_at_zero_and_adds_a_room_and_an_echo_when_up() {
        let mut click = vec![0.0; 48_000];
        click[0] = 1.0;
        assert_eq!(with(Effect::KaraokeMix, 0, click.clone()), click, "dry at intensity 0");
        assert_eq!(with(Effect::AutoTune, 0, click.clone()), click, "auto-tune is dry at intensity 0 too");
        assert_eq!(with(Effect::None, 100, click.clone()), click, "no effect is dry");
        let mut fx = Effects::new(48_000);
        fx.set(Effect::KaraokeMix, 100);
        run(&mut fx, vec![0.0; 2_400]);
        let wet = run(&mut fx, click);
        let room = wet[(0.03 * RATE) as usize..(0.1 * RATE) as usize].iter().map(|x| x * x).sum::<f32>();
        assert!(room > 1e-4, "the room answers before the first echo");
        assert!(wet[(ECHO_MS / 1000.0 * RATE) as usize].abs() > 0.2, "an echo after {ECHO_MS} ms");
    }

    #[test]
    fn switching_effects_or_their_strength_never_clicks() {
        let mut fx = Effects::new(48_000);
        let mut tone: Vec<f32> = sine(440.0 * 2f32.powf(-20.0 / 1200.0), 2.0).iter().map(|x| 0.4 * x).collect();
        let changes = [(Effect::AutoTune, 100), (Effect::KaraokeMix, 100), (Effect::KaraokeMix, 0), (Effect::AutoTune, 60), (Effect::None, 100)];
        for (part, (effect, amount)) in tone.chunks_mut((0.4 * RATE) as usize).zip(changes) {
            fx.set(effect, amount);
            for block in part.chunks_mut(256) {
                fx.process(block);
            }
        }
        assert!(smooth(&tone));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core mic::effects`
Expected: FAIL to compile — `cannot find type Effects`.

- [ ] **Step 3: Write the implementation**

Put this above the tests in `crates/kara-core/src/mic/effects.rs`:

```rust
//! A phone's voice effect before the mix: none, a karaoke mix (short echo and warm reverb), or auto-tune (pulled to the nearest semitone).

use super::Reverb;
use serde::Deserialize;
use std::collections::VecDeque;

const ECHO_MS: f32 = 120.0;
const ECHO_FEEDBACK: f32 = 0.35;
const ECHO_WET: f32 = 0.35;
const ROOM_WET: f32 = 0.5;
const WARM_HZ: f32 = 4_000.0;
/// How long switching effects or intensities takes, so a change never clicks.
const RAMP_MS: f32 = 20.0;
const LOW_HZ: f32 = 80.0;
const HIGH_HZ: f32 = 1_000.0;
const DECIMATE: usize = 4;
/// Decimated samples compared per pitch estimate (about 21 ms).
const WINDOW: usize = 256;
const LOOK_EVERY: usize = 256;
const YIN_THRESHOLD: f32 = 0.15;
const QUIET: f32 = 1e-5;
/// The auto-tune's two taps sit a whole number of periods apart, at least this far; the voice is delayed by about twice the spacing.
const MIN_SPACING_MS: f32 = 3.5;
const REST_SPACING_MS: f32 = 5.0;
const LINE_MS: f32 = 40.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    #[default]
    None,
    KaraokeMix,
    AutoTune,
}

pub struct Effects {
    /// The effect playing now, and the one asked for (with its intensity 0 to 1).
    active: Effect,
    wanted: (Effect, f32),
    amount: f32,
    wet: f32,
    ramp: f32,
    echo: Vec<f32>,
    echo_i: usize,
    room: Reverb,
    warm: f32,
    warm_k: f32,
    tune: Tune,
}

impl Effects {
    pub fn new(rate: u32) -> Self {
        Self {
            active: Effect::None,
            wanted: (Effect::None, 0.0),
            amount: 0.0,
            wet: 0.0,
            ramp: 1.0 / (RAMP_MS / 1000.0 * rate as f32),
            echo: vec![0.0; (ECHO_MS / 1000.0 * rate as f32) as usize],
            echo_i: 0,
            room: Reverb::new(rate),
            warm: 0.0,
            warm_k: 1.0 - (-std::f32::consts::TAU * WARM_HZ / rate as f32).exp(),
            tune: Tune::new(rate),
        }
    }

    /// Asks for an effect and its intensity, 0–100; intensity 0 means no effect. The change fades in over a few milliseconds.
    pub fn set(&mut self, effect: Effect, amount: u8) {
        let amount = f32::from(amount.min(100)) / 100.0;
        self.wanted = (if amount == 0.0 { Effect::None } else { effect }, amount);
    }

    /// Applies the effect to `samples` in place, fading between the effect playing and the one asked for.
    pub fn process(&mut self, samples: &mut [f32]) {
        for x in samples {
            let (want, amount) = self.wanted;
            if want != self.active && self.wet == 0.0 {
                self.active = want;
                self.tune.clear();
            }
            let target = if want == self.active { 1.0 } else { 0.0 };
            self.wet = if self.wet < target { (self.wet + self.ramp).min(target) } else { (self.wet - self.ramp).max(target) };
            if want == self.active {
                self.amount = if self.amount < amount { (self.amount + self.ramp).min(amount) } else { (self.amount - self.ramp).max(amount) };
                self.tune.set(self.amount);
            }
            let dry = *x;
            let wet = match self.active {
                Effect::AutoTune => self.tune.next(dry),
                other => {
                    self.tune.hear(dry);
                    if other == Effect::KaraokeMix { self.karaoke(dry) } else { dry }
                }
            };
            *x = dry + self.wet * (wet - dry);
        }
    }

    fn karaoke(&mut self, x: f32) -> f32 {
        let echoed = self.echo[self.echo_i];
        self.echo[self.echo_i] = x + echoed * ECHO_FEEDBACK;
        self.echo_i = (self.echo_i + 1) % self.echo.len();
        self.warm += (self.room.process(x) - self.warm) * self.warm_k;
        x + self.amount * (ECHO_WET * echoed + ROOM_WET * self.warm)
    }
}

/// Chromatic pitch correction: finds the pitch of the recent input and replays it through two taps whose delays slide at the
/// corrected speed; each tap restarts a whole number of periods from the other, so their crossfades stay in phase.
struct Tune {
    rate: f32,
    line: Vec<f32>,
    write: usize,
    phase: f32,
    life: f32,
    delay: [f32; 2],
    spacing: f32,
    ratio: f32,
    target: f32,
    strength: f32,
    glide: f32,
    history: VecDeque<f32>,
    sum: f32,
    count: usize,
    until_look: usize,
    cmnd: Vec<f32>,
    low_tau: usize,
    high_tau: usize,
}

impl Tune {
    fn new(rate: u32) -> Self {
        let rate = rate as f32;
        let slow = rate / DECIMATE as f32;
        let high_tau = (slow / LOW_HZ) as usize;
        let spacing = REST_SPACING_MS / 1000.0 * rate;
        Self {
            rate,
            line: vec![0.0; (LINE_MS / 1000.0 * rate) as usize],
            write: 0,
            phase: 0.0,
            life: 2.0 * spacing,
            delay: [spacing; 2],
            spacing,
            ratio: 1.0,
            target: 1.0,
            strength: 0.0,
            glide: 0.0,
            history: VecDeque::with_capacity(WINDOW + high_tau),
            sum: 0.0,
            count: 0,
            until_look: LOOK_EVERY,
            cmnd: vec![1.0; high_tau + 2],
            low_tau: (slow / HIGH_HZ) as usize,
            high_tau,
        }
    }

    /// Keeps the recent input while auto-tune is off, so turning it on replays nothing old.
    fn hear(&mut self, x: f32) {
        self.line[self.write] = x;
        self.write = (self.write + 1) % self.line.len();
    }

    /// Starts listening afresh with the taps at rest.
    fn clear(&mut self) {
        self.history.clear();
        self.spacing = REST_SPACING_MS / 1000.0 * self.rate;
        self.delay = [self.spacing; 2];
        self.life = 2.0 * self.spacing;
        self.phase = 0.0;
        self.ratio = 1.0;
        self.target = 1.0;
    }

    /// How far (0 to 1) and how fast the pitch is pulled.
    fn set(&mut self, amount: f32) {
        self.strength = amount;
        self.glide = 1.0 / ((0.05 - 0.045 * amount) * self.rate);
    }

    fn next(&mut self, x: f32) -> f32 {
        self.listen(x);
        self.shift(x)
    }

    /// Keeps a short, decimated history; every few milliseconds aims the ratio at the nearest semitone and spaces the taps by whole periods.
    fn listen(&mut self, x: f32) {
        self.sum += x;
        self.count += 1;
        if self.count == DECIMATE {
            if self.history.len() == WINDOW + self.high_tau {
                self.history.pop_front();
            }
            self.history.push_back(self.sum / DECIMATE as f32);
            self.sum = 0.0;
            self.count = 0;
        }
        self.until_look -= 1;
        if self.until_look == 0 {
            self.until_look = LOOK_EVERY;
            self.target = match self.pitch() {
                Some(hz) => {
                    let period = self.rate / hz;
                    self.spacing = period * (MIN_SPACING_MS / 1000.0 * self.rate / period).ceil();
                    let semis = 12.0 * (hz / 440.0).log2();
                    2f32.powf((semis.round() - semis) * self.strength / 12.0)
                }
                None => 1.0,
            };
        }
        self.ratio += (self.target - self.ratio) * self.glide;
    }

    /// The recent input's pitch in Hz (YIN), or None when it is quiet or not a clear note.
    fn pitch(&mut self) -> Option<f32> {
        if self.history.len() < WINDOW + self.high_tau {
            return None;
        }
        let h = self.history.make_contiguous();
        if h.iter().map(|x| x * x).sum::<f32>() / (h.len() as f32) < QUIET {
            return None;
        }
        let mut total = 0.0;
        for tau in 1..=self.high_tau {
            let d: f32 = (0..WINDOW).map(|j| (h[j] - h[j + tau]).powi(2)).sum();
            total += d;
            self.cmnd[tau] = if total > 0.0 { d * tau as f32 / total } else { 1.0 };
        }
        let mut tau = (self.low_tau.max(2)..self.high_tau).find(|&t| self.cmnd[t] < YIN_THRESHOLD)?;
        while tau + 1 < self.high_tau && self.cmnd[tau + 1] < self.cmnd[tau] {
            tau += 1;
        }
        let (a, b, c) = (self.cmnd[tau - 1], self.cmnd[tau], self.cmnd[tau + 1]);
        let bend = a - 2.0 * b + c;
        let shift = if bend.abs() > 1e-9 { 0.5 * (a - c) / bend } else { 0.0 };
        Some(self.rate / DECIMATE as f32 / (tau as f32 + shift))
    }

    /// Plays the two taps crossfaded over their overlapping lives; their delays slide by 1 - ratio a sample, which moves the pitch by `ratio`.
    fn shift(&mut self, x: f32) -> f32 {
        let len = self.line.len();
        self.line[self.write] = x;
        let most = (len - 2) as f32;
        for d in &mut self.delay {
            *d = (*d + 1.0 - self.ratio).clamp(0.0, most);
        }
        let before = self.phase;
        self.phase += 1.0 / self.life;
        if before < 0.5 && self.phase >= 0.5 {
            self.restart(1);
        }
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.restart(0);
        }
        let w = 1.0 - (2.0 * self.phase - 1.0).abs();
        let y = w * self.read(self.delay[0]) + (1.0 - w) * self.read(self.delay[1]);
        self.write = (self.write + 1) % len;
        y
    }

    /// Starts tap `i` again one spacing (whole periods) from the other: ahead of it while the delays shrink, behind it while they grow.
    fn restart(&mut self, i: usize) {
        let (other, s) = (self.delay[1 - i], self.spacing);
        let (later, sooner) = (other + s, other - s);
        self.delay[i] = if self.ratio >= 1.0 {
            if later <= 2.1 * s || sooner < 0.1 * s { later } else { sooner }
        } else if sooner >= 0.0 {
            sooner
        } else {
            later
        };
        self.life = 2.0 * s;
    }

    fn read(&self, delay: f32) -> f32 {
        let len = self.line.len();
        let at = (self.write as f32 - delay).rem_euclid(len as f32);
        let i = (at as usize) % len;
        let f = at - at.floor();
        self.line[i] * (1.0 - f) + self.line[(i + 1) % len] * f
    }

    /// How far behind the voice the taps are now, weighted by how loud each is, in samples.
    #[cfg(test)]
    fn delay_now(&self) -> f32 {
        let w = 1.0 - (2.0 * self.phase - 1.0).abs();
        w * self.delay[0] + (1.0 - w) * self.delay[1]
    }
}
```

In `crates/kara-core/src/mic/mod.rs`:
- `NewVoice` gains `effects: Effects`, built in `NewVoice::new` as `effects: Effects::new(out_rate)` (outside the lock, like the rest);
- `Voice` gains `effects: Effects`; in `add`, a new phone's voice takes it (`let NewVoice { id, buffer, howl, effects } = new;`), while a returning phone keeps its own and the unused one goes back with the other leftovers;
- new method:

```rust
    pub fn set_effect(&mut self, id: &str, effect: Effect, amount: u8) {
        if let Some(v) = self.voice(id) {
            v.effects.set(effect, amount);
        }
    }
```

- in `render`, right after `v.howl.feed(&v.scratch);` add `v.effects.process(&mut v.scratch);`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-core --release mic:: && cargo clippy -p kara-core --all-targets -- -D warnings`
Expected: PASS (11 tests: buffer 3, howl 2, mixer 1, effects 5; `--release` keeps the pitch tests to a few seconds); no warnings. Tasks 1–4's code and tests, as written here, were compiled, run and linted together in a scratch crate while planning. If a bound is missed after adapting it, print the measured pitch or delay every 0.1 s and fix the code, not the bound (the spec wants the nearest semitone, ≤ 15 ms for most voices and ~25 ms at most for low ones).

- [ ] **Step 5: Commit**

```bash
git add crates/kara-core/src/mic
git commit -m "feat(core): per-phone voice effects — karaoke mix and pitch-synchronous auto-tune"
```

---

### Task 5: Queue entries remember who added them; one queue list for the Mac and the phone

**Files:**
- Modify: `app/src-tauri/src/player.rs`
- Modify: `app/src/lib/api.ts` (`QueueEntry`)
- Create: `app/src/lib/components/QueueList.svelte`
- Modify: `app/src/lib/components/QueuePanel.svelte`
- Modify: `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`
- Modify: `app/tests/fake-backend.ts`
- Test: `app/src-tauri/src/player.rs` (existing test extended), `app/tests/queue.spec.ts`

**Interfaces:**
- Consumes: the Phase 1b `Player`, `snapshot`, `queue_add`, `QueuePanel` as at HEAD.
- Produces:
  - Rust: `Entry { key, track_id, by: Option<String> }`; `Player::add(&mut self, track_id: i64, next: bool, by: Option<String>)`; `QueueEntry { key: u64, track: Track, by: Option<String> }` (JSON `by: string | null`); `queue_song(p, lib, track_id, next, by: Option<String>)`; command `queue_add(app, state, track_id: i64, next: bool, by: Option<String>)` (the Mac passes nothing, so `None`).
  - TS: `QueueEntry { key: number; track: Track; by: string | null }`.
  - `QueueList.svelte` props: `{ snapshot: PlayerSnapshot; art?: number /* thumb px, default 36 */; nothingBody: string; emptyBody: string; onMove: (key: number, to: number) => void; onRemove: (key: number) => void }` — renders Now playing, "Up next · n" with drag by `.grip` (pointer events, touch too) and Remove, the empty states, and under each song "artist · added by Name" when a guest added it. Row markup keeps `.qrow`, `[data-qi]`, `.grip` and the "Remove" button name.
  - Fake backend lever `window.fake.guestAdds(trackId: number, by: string)`.
  - i18n key `queue.addedBy`.

- [ ] **Step 1: Write the failing tests**

In `app/src-tauri/src/player.rs` tests, rename and extend `the_snapshot_carries_the_queue_and_the_current_lyrics_timing`:

```rust
    #[test]
    fn the_snapshot_carries_the_queue_who_added_each_song_and_the_current_lyrics_timing() {
        let lib = Library::open_in_memory().unwrap();
        let add = |title| lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist: None, album: None, duration_ms: None }).unwrap();
        let (a, b) = (add("Paper Boats"), add("Rooftop Static"));
        let src = lib.add_source(a, SourceKind::File, "/made/up.wav", None).unwrap();
        lib.set_lyric_offset(src, -300).unwrap();
        let mut p = Player::default();
        p.play(&[a, b], 0);
        queue_song(&mut p, &lib, b, false, Some("Aiko".into())).unwrap();
        let json = serde_json::to_value(snapshot(&lib, &p).unwrap()).unwrap();
        assert_eq!((json["entries"][1]["track"]["title"].as_str(), json["current"].as_u64(), json["lyricOffsetMs"].as_i64()), (Some("Rooftop Static"), Some(0), Some(-300)));
        assert_eq!((json["entries"][0]["by"].is_null(), json["entries"][2]["by"].as_str()), (true, Some("Aiko")));
        assert_eq!(json["entries"][0]["track"]["vocalRemoval"], 100);
    }
```

In the same test module, every other `p.add(x, next)` becomes `p.add(x, next, None)` and every `queue_song(&mut p, &lib, id, next)` becomes `queue_song(&mut p, &lib, id, next, None)`.

Append to `app/tests/queue.spec.ts`:

```ts
test("songs a guest added say who added them", async ({ page }) => {
  await sing(page, "Paper Boats");
  await page.evaluate(() => window.fake.guestAdds(3, "Aiko"));
  await page.keyboard.press("Escape");
  await page.getByRole("region", { name: "Player" }).getByRole("button", { name: "Queue" }).click();
  const row = page.getByRole("complementary", { name: "Queue" }).locator("[data-qi]", { hasText: "Lemon Skies" });
  await expect(row).toContainText("The Porchlights · added by Aiko");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app player::`
Expected: FAIL to compile — `queue_song` takes 4 arguments.

Run: `cd app && npx playwright test tests/queue.spec.ts`
Expected: the new test FAILS — `window.fake.guestAdds is not a function`.

- [ ] **Step 3: Rust — entries carry `by`**

In `app/src-tauri/src/player.rs`:

```rust
pub struct Entry {
    pub key: u64,
    pub track_id: i64,
    pub by: Option<String>,
}
```

```rust
    fn entry(&mut self, track_id: i64, by: Option<String>) -> Entry {
        self.next_key += 1;
        Entry { key: self.next_key, track_id, by }
    }
```

`add` gains the guest's name:

```rust
    /// Queues a song at the end, or right after the current one; when idle it becomes the current song. `by` names the guest who added it.
    pub fn add(&mut self, track_id: i64, next: bool, by: Option<String>) {
        let e = self.entry(track_id, by);
```

(rest of `add` unchanged), `play` uses `self.entry(t, None)`, and:

```rust
#[derive(Clone, Serialize)]
pub struct QueueEntry {
    pub key: u64,
    pub track: Track,
    pub by: Option<String>,
}
```

In `snapshot`: `QueueEntry { key: e.key, track: lib.track(e.track_id)?, by: e.by.clone() }`.

```rust
/// Queues a song, refusing one that is no longer in the library.
fn queue_song(p: &mut Player, lib: &Library, track_id: i64, next: bool, by: Option<String>) -> anyhow::Result<()> {
    lib.track(track_id).context(Problem::SongGone)?;
    p.add(track_id, next, by);
    Ok(())
}
```

```rust
#[tauri::command]
pub fn queue_add(app: AppHandle, state: State<'_, AppState>, track_id: i64, next: bool, by: Option<String>) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, lib| queue_song(p, lib, track_id, next, by))
}
```

- [ ] **Step 4: TS type, text, fake**

`app/src/lib/api.ts`: `export interface QueueEntry { key: number; track: Track; by: string | null }`.

Add `"queue.addedBy"` after `"queue.remove"` in each locale:

| file | text |
|---|---|
| `en.ts` | `"queue.addedBy": "added by {name}",` |
| `ja.ts` | `"queue.addedBy": "{name} さんが追加",` |
| `ko.ts` | `"queue.addedBy": "{name} 님이 추가함",` |
| `zh-Hans.ts` | `"queue.addedBy": "{name} 添加",` |
| `zh-Hant.ts` | `"queue.addedBy": "{name} 新增",` |
| `es.ts` | `"queue.addedBy": "añadida por {name}",` |

`app/tests/fake-backend.ts`:
- `let queue: { key: number; trackId: number; by?: string }[] = [];`
- in `snapshot()`: `entries: queue.map((e) => ({ key: e.key, track: tracks.get(e.trackId)!, by: e.by ?? null }))`
- add to `fake`:

```ts
  /** A guest adds a song from their phone; with nothing playing it becomes the current song, as the real queue does. */
  guestAdds(trackId: number, by: string) {
    queue.push({ key: ++nextKey, trackId, by });
    if (current == null || ended) [current, ended] = [queue.length - 1, false];
    changed();
  },
```

- [ ] **Step 5: Extract `QueueList.svelte`**

Create `app/src/lib/components/QueueList.svelte` from the body of `QueuePanel.svelte` (same markup and drag code), taking the snapshot and callbacks as props:

```svelte
<script lang="ts">
  import type { PlayerSnapshot, QueueEntry } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";
  import { duration } from "$lib/format";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import Artwork from "./Artwork.svelte";
  import QueueIcon from "phosphor-svelte/lib/QueueIcon";
  import XIcon from "phosphor-svelte/lib/XIcon";
  import WaveformIcon from "phosphor-svelte/lib/WaveformIcon";
  import DotsSixVerticalIcon from "phosphor-svelte/lib/DotsSixVerticalIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";

  let { snapshot, art = 36, nothingBody, emptyBody, onMove, onRemove }: {
    snapshot: PlayerSnapshot;
    art?: number;
    nothingBody: string;
    emptyBody: string;
    onMove: (key: number, to: number) => void;
    onRemove: (key: number) => void;
  } = $props();

  let dragKey = $state<number | null>(null);
  let over = $state<number | null>(null);
  const now = $derived(snapshot.current == null ? null : (snapshot.entries[snapshot.current] ?? null));
  const upcoming = $derived.by(() => {
    const c = snapshot.current;
    return c == null ? [] : snapshot.entries.map((entry, index) => ({ entry, index })).filter(({ index }) => index > c);
  });
  const dragFrom = $derived(upcoming.find(({ entry }) => entry.key === dragKey)?.index ?? null);

  /** The artist, and who added the song when a guest did. */
  const byline = (e: QueueEntry) => [e.track.artist, e.by && t("queue.addedBy", { name: e.by })].filter(Boolean).join(" · ");

  /** The queue index of the upcoming row under the pointer. */
  function rowAt(e: PointerEvent) {
    const row = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-qi]");
    return row ? Number(row.dataset.qi) : null;
  }

  function drop(to: number | null) {
    const key = dragKey;
    const from = dragFrom;
    dragKey = over = null;
    if (key != null && from != null && to != null && from !== to) onMove(key, to);
  }
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === "Escape" && dragKey != null) dragKey = over = null;
}} />

{#if !now}
  <div class="qempty"><QueueIcon size={20} /><b>{t("queue.nothingPlaying")}</b><span>{nothingBody}</span></div>
{:else}
  <p class="cap hstack"><WaveformIcon size={14} />{t("queue.nowPlaying")}</p>
  <div class="qrow">
    <Artwork track={now.track} size={art} />
    <span class="grow"><b class="ell">{now.track.title}</b><small class="ell">{byline(now)}</small></span>
    <span class="num">{duration(now.track.durationMs)}</span>
  </div>
  <p class="cap hstack"><QueueIcon size={14} />{t("queue.upNext", { n: upcoming.length })}</p>
  {#each upcoming as { entry, index } (entry.key)}
    <div
      class="qrow"
      class:dragging={dragFrom === index}
      class:over={dragFrom != null && over === index}
      class:below={dragFrom != null && dragFrom < index}
      data-qi={index}
      transition:slide={{ x: 8, y: 0 }}
    >
      <span
        class="grip"
        aria-hidden="true"
        use:tip={t("queue.drag")}
        onpointerdown={(e) => {
          if (e.button !== 0) return;
          e.preventDefault();
          e.currentTarget.setPointerCapture(e.pointerId);
          dragKey = entry.key;
        }}
        onpointermove={(e) => {
          if (dragFrom != null) over = rowAt(e);
        }}
        onpointerup={(e) => drop(rowAt(e))}
        onpointercancel={() => (dragKey = over = null)}
      ><DotsSixVerticalIcon size={16} /></span>
      <Artwork track={entry.track} size={art} />
      <span class="grow"><b class="ell">{entry.track.title}</b><small class="ell">{byline(entry)}</small></span>
      <button class="ib" use:tip={t("queue.remove")} onclick={() => onRemove(entry.key)}><XIcon size={16} /></button>
    </div>
  {:else}
    <div class="qempty"><ListPlusIcon size={20} /><b>{t("queue.empty")}</b><span>{emptyBody}</span></div>
  {/each}
{/if}

<style>
  .cap { padding: var(--s3) var(--s2) var(--s1); }
  .qrow { display: flex; align-items: center; gap: var(--s2); min-height: var(--row); padding: 0 var(--s1) 0 var(--s2); border-radius: var(--r-sm); transition: opacity var(--t) var(--ease), background-color var(--t) var(--ease); }
  .qrow[data-qi]:hover { background: color-mix(in srgb, var(--text) 5%, transparent); }
  .qrow b { display: block; font-weight: 500; }
  .qrow small { display: block; color: var(--muted); font-size: 12.5px; }
  .grip { color: var(--faint); cursor: grab; display: grid; touch-action: none; }
  .dragging { opacity: .4; }
  .over { box-shadow: inset 0 2px 0 var(--accent); }
  .over.below { box-shadow: inset 0 -2px 0 var(--accent); }
  .qempty { display: grid; justify-items: center; text-align: center; gap: var(--s2); padding: var(--s7) var(--s4); color: var(--muted); font-size: 13px; }
  .qempty b { color: var(--text); font-size: 15px; }
</style>
```

Replace `app/src/lib/components/QueuePanel.svelte` with the panel shell around it:

```svelte
<script lang="ts">
  import { player } from "$lib/state/player.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import QueueList from "./QueueList.svelte";
  import QueueIcon from "phosphor-svelte/lib/QueueIcon";
  import XIcon from "phosphor-svelte/lib/XIcon";
</script>

{#if ui.queueOpen}
  <aside class="qpanel glass" class:dk={ui.karaoke} aria-label={t("queue.title")} transition:slide={{ x: 24, y: 0 }}>
    <div class="qhead">
      <QueueIcon size={18} /><h2 class="grow">{t("queue.title")}</h2>
      <button class="ib" use:tip={t("common.close")} onclick={() => (ui.queueOpen = false)}><XIcon size={18} /></button>
    </div>
    <div class="qbody">
      <QueueList
        snapshot={player.snapshot}
        nothingBody={t("queue.nothingPlayingBody")}
        emptyBody={t("queue.emptyBody")}
        onMove={(key, to) => void player.moveQueued(key, to)}
        onRemove={(key) => void player.removeQueued(key)}
      />
    </div>
  </aside>
{/if}

<style>
  .qpanel { position: fixed; z-index: 35; top: 92px; right: var(--s4); bottom: 104px; width: min(340px, calc(100% - 32px)); display: flex; flex-direction: column; padding: var(--s4) var(--s3) var(--s3); border-radius: var(--r-lg); }
  .qhead { display: flex; align-items: center; gap: var(--s2); padding: 0 var(--s1) var(--s3) var(--s2); }
  h2 { font: 800 20px/1.2 var(--display); letter-spacing: -.02em; }
  .qbody { flex: 1; min-height: 0; overflow: auto; }
</style>
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app player:: && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS; no warnings.

Run: `cd app && npm run check && npm run check:i18n && npx playwright test tests/queue.spec.ts`
Expected: 0 errors; every locale matches; both queue tests PASS (the old one proves the panel still reorders and removes).

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/src/player.rs app/src/lib/api.ts app/src/lib/components/QueueList.svelte app/src/lib/components/QueuePanel.svelte app/src/lib/i18n app/tests/fake-backend.ts app/tests/queue.spec.ts
git commit -m "feat(app): queue entries say which guest added them; shared queue list"
```

---

### Task 6: The phone server — certificate, LAN only, join code, four rows, the phone page, an isolated check

**Files:**
- Modify: `crates/kara-core/src/problem.rs` (add `PhonesStart`, `NoNetwork`)
- Modify: `app/src-tauri/Cargo.toml`
- Modify: `app/src-tauri/src/state.rs` (`AppError.problem` becomes `pub(crate)`)
- Create: `app/src-tauri/src/phones/mod.rs`, `phones/server.rs`, `phones/cert.rs`, `phones/room.rs`
- Modify: `app/src-tauri/src/lib.rs`
- Create: `app/scripts/phone-check.ts`, `app/scripts/isolated-check.sh`
- Modify: `app/scripts/window-id.swift` (also finds a window by process id)

**Interfaces:**
- Consumes: `player::player_state`, `player::snapshot`, `player::Player` (Task 5 signatures), `AppState.store` (`Store::root()`, `artwork_dir()`), `kara_core::store::write_atomic`, `kara_core::now_ms`.
- Produces:
  - Problems `Problem::PhonesStart` ("Couldn't start phone mics.") and `Problem::NoNetwork` ("Connect this computer to Wi-Fi to use phone mics."), codes `phonesStart`, `noNetwork`.
  - `phones::Phones` (managed state; `Default`).
  - Commands: `phones_open() -> PhonesView` (async; starts the session if needed, marks the window open), `phones_close()` (window closed; ends the session when no phone is joined), `phone_remove(id: String)`.
  - Event `"phones"` with `PhonesView { join: Option<JoinInfo>, phones: Vec<PhoneRow> }`; `JoinInfo { qr: String /* SVG, dark = currentColor */, code: String /* "OKI-1234" */, host: String /* "Name.local" */ }`; `host` is the bare `"Name.local"` whenever the plain-HTTP redirect on port 80 runs (the redirect adds `:8443` itself when 443 was taken), and `"https://Name.local"` — plus `":8443"` only when 443 was taken — when port 80 couldn't be bound (a bare name would then reach nothing, and `name.local:8443` without `https://` would send plain HTTP to the secure port). `PhoneRow { id, name, connected }` (Task 8 adds `volume`). `PhonesView::default()` (no join, no phones) is sent when the session ends. The event is sent only when something in it changes.
  - Event `"phone-news"` with `News` (`#[serde(tag = "kind")]`): `{ kind: "joined", name, mic }` when a phone gets a new row (not when it comes back); Task 7 adds `added`.
  - `phones::end(app: &AppHandle)`, `phones::ensure_session(app: &AppHandle, code: String) -> anyhow::Result<()>`, `phones::open(app, code) -> anyhow::Result<PhonesView>` (ensure + window open; the `KARA_PHONE_CODE` dev session uses it too, so it lasts until the app quits).
  - In `phones/mod.rs` for later tasks: `pub(crate) enum ToPhone<'a>` (`Joined`, `Player { snapshot: &'a PlayerSnapshot }`), `pub(crate) fn encode(msg: &ToPhone) -> String` (JSON with every `artworkPath` turned into `/art/<file name>`), `fn with<T>(app, f: impl FnOnce(&mut Session) -> T) -> Option<T>`, `fn changed(app)`, `pub(crate) fn admit(app, code, id, name, tx) -> Result<u64 /* connection */, u16 /* close code */>` (Task 8 also returns the mixer), `pub(crate) fn leave(app, id)`, `pub(crate) fn dropped(app, id, conn)`.
  - `phones/server.rs`: `pub(crate) enum FromPhone { Join { code, id, name }, Ping, Leave }` (`#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]`), `pub fn serve(app, tls, handle) -> anyhow::Result<(u16 /* HTTPS port */, bool /* port-80 redirect running */)>`. A connection that sends nothing for 5 s is treated as dropped (phones send `{t:"ping"}` every second, Task 10).
  - `phones/room.rs`: `Room { code, guests }`, `Room::admit(...) -> Result<bool /* a new row */, Refusal>`, `Guest { id, name, conn, tx }`, `Out { Text(String), Close(u16) }`, `Refusal { WrongCode, Full }`, `ENDED = 4001`, `new_code()`, `digits()`.
  - `phones/cert.rs`: `ensure(dir, name, ips, now) -> Result<(Vec<u8>, Vec<u8>)>`, `lan_ips() -> Vec<IpAddr>`, `is_lan(IpAddr) -> bool`, `local_name() -> Option<String>`.
  - Routes: `GET /` → redirect `/phone`; `GET /ws`; `GET /art/{name}`; any other GET → the app's page files (dev: the Vite dev server); other methods → 405. Only LAN peers. Plain HTTP on port 80 redirects (temporary, 307) to `https://<host>[:port]/phone`.
  - Debug builds read `KARA_PHONE_PORT`: when set, the server uses only that port and leaves port 80 alone, so the copy under test never meets the user's own app on 443/80.
  - `app/scripts/phone-check.ts`, printing `phone check OK`; it talks only to `https://127.0.0.1:$KARA_PHONE_PORT` and waits until that server accepts `KARA_PHONE_CODE`.
  - `app/scripts/isolated-check.sh <picture> [command]` — the **isolated check** every later task uses (Global Constraints).

- [ ] **Step 1: Dependencies and a probe**

Add to `[dependencies]` in `app/src-tauri/Cargo.toml`:

```toml
axum = { version = "0.8", features = ["ws"] }
axum-server = { version = "0.7", features = ["tls-rustls-no-provider"] }
rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
rcgen = "0.13"
time = "0.3"
tokio = { version = "1", features = ["sync", "macros", "time"] }
reqwest = { version = "0.12", default-features = false }
qrcode = { version = "0.14", default-features = false, features = ["svg"] }
if-addrs = "0.13"
getrandom = "0.3"
```

Run: `source "$HOME/.cargo/env" && cargo tree -p kara-app -e features -i aws-lc-rs 2>&1 | head -3`
Expected: `error: package ID specification 'aws-lc-rs' did not match any packages` (only ring is built, as the spike found). If `qrcode` has no `svg` feature at that version, read `~/.cargo/registry/src/*/qrcode-*/Cargo.toml` and use the feature that provides `qrcode::render::svg`.

- [ ] **Step 2: Write the failing tests**

Create `app/src-tauri/src/phones/cert.rs` with its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_certificate_is_reused_until_the_mac_gets_a_new_address_or_it_nears_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let home: IpAddr = "192.168.1.20".parse().unwrap();
        let office: IpAddr = "10.0.4.7".parse().unwrap();
        let now = 1_790_000_000;
        let first = ensure(dir.path(), "mac.local", &[home], now).unwrap();
        assert_eq!(ensure(dir.path(), "mac.local", &[home], now + 30 * DAY).unwrap(), first);
        let moved = ensure(dir.path(), "mac.local", &[office], now + 31 * DAY).unwrap();
        assert_ne!(moved, first);
        assert_eq!(ensure(dir.path(), "mac.local", &[home], now + 32 * DAY).unwrap(), moved, "still covers the first address");
        assert_ne!(ensure(dir.path(), "mac.local", &[home], now + (31 + 771) * DAY).unwrap(), moved, "remade a month before it runs out");
    }

    #[test]
    fn only_local_network_addresses_count_as_lan() {
        let lan = ["192.168.1.9", "10.1.2.3", "172.16.0.5", "169.254.10.1", "127.0.0.1", "::1", "fe80::1", "fd12::3", "::ffff:192.168.0.2"];
        let outside = ["8.8.8.8", "172.32.0.1", "2001:db8::1", "::ffff:1.1.1.1"];
        for a in lan {
            assert!(is_lan(a.parse().unwrap()), "{a} is on the local network");
        }
        for a in outside {
            assert!(!is_lan(a.parse().unwrap()), "{a} is not");
        }
    }
}
```

Create `app/src-tauri/src/phones/room.rs` with its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn tx() -> UnboundedSender<Out> {
        tokio::sync::mpsc::unbounded_channel().0
    }

    #[test]
    fn a_phone_needs_the_code_and_the_fifth_is_refused() {
        let mut room = Room::new("4827");
        assert_eq!(room.admit("1234", "a", "Aiko", 1, tx()), Err(Refusal::WrongCode));
        assert_eq!(room.admit("oki-4827", "a", "Aiko", 1, tx()), Ok(true));
        for id in ["b", "c", "d"] {
            assert_eq!(room.admit("4827", id, id, 2, tx()), Ok(true));
        }
        assert_eq!(room.admit("4827", "e", "Emi", 3, tx()), Err(Refusal::Full));
    }

    #[test]
    fn a_phone_that_drops_gets_its_own_row_back() {
        let mut room = Room::new("4827");
        for id in ["a", "b", "c", "d"] {
            room.admit("4827", id, id, 1, tx()).unwrap();
        }
        room.dropped("a", 1);
        assert!(room.guests[0].tx.is_none(), "greyed, not gone");
        assert_eq!(room.admit("4827", "e", "Emi", 2, tx()), Err(Refusal::Full), "its row is kept for it");
        assert_eq!(room.admit("4827", "a", "Aiko", 3, tx()), Ok(false), "back in its own row");
        room.dropped("a", 1);
        let a = &room.guests[0];
        assert_eq!((room.guests.len(), a.name.as_str(), a.tx.is_some()), (4, "Aiko", true), "a late close of the old connection changes nothing");
    }
}
```

Create `app/src-tauri/src/phones/mod.rs` with the module lines and its test (implementation in Step 4):

```rust
//! Phone mics: the session guests join, the Mac's commands for it, and what phones and the Mac window hear.

mod cert;
mod room;
mod server;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{snapshot, Player};
    use kara_core::library::{Library, NewTrack, ProviderId};

    #[test]
    fn song_pictures_reach_phones_as_links_to_this_server() {
        let lib = Library::open_in_memory().unwrap();
        let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: "Paper Boats", artist: None, album: None, duration_ms: None }).unwrap();
        lib.set_artwork(t, "/Users/someone/Library/Application Support/kara-always-oki/artwork/ab12.jpg").unwrap();
        let mut p = Player::default();
        p.add(t, false, None);
        let json: Value = serde_json::from_str(&encode(&ToPhone::Player { snapshot: &snapshot(&lib, &p).unwrap() })).unwrap();
        assert_eq!((json["t"].as_str(), json["snapshot"]["entries"][0]["track"]["artworkPath"].as_str()), (Some("player"), Some("/art/ab12.jpg")));
    }
}
```

Add `mod phones;` to `app/src-tauri/src/lib.rs` (after `mod player;`).

- [ ] **Step 3: Run the tests to verify they fail**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app phones::`
Expected: FAIL to compile — missing `ensure`, `Room`, `encode`, `server` module.

- [ ] **Step 4: Write the implementation**

`crates/kara-core/src/problem.rs`: add `PhonesStart,` and `NoNetwork,` after `NoSongAtLink,` in the enum, and in `Display`:

```rust
            Self::PhonesStart => "Couldn't start phone mics.",
            Self::NoNetwork => "Connect this computer to Wi-Fi to use phone mics.",
```

`app/src-tauri/src/state.rs`: `pub(crate) problem: Option<Problem>,` in `AppError`.

Above the tests in `app/src-tauri/src/phones/cert.rs`:

```rust
//! The server's self-signed certificate, made once and kept, and the Mac's addresses on the local network.

use anyhow::Result;
use kara_core::store::write_atomic;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;

const DAY: i64 = 86_400;
const VALID_DAYS: i64 = 800;
const RENEW_DAYS: i64 = 30;

#[derive(Serialize, Deserialize)]
struct Kept {
    name: String,
    ips: Vec<IpAddr>,
    made_at: i64,
    cert: String,
    key: String,
}

/// PEM certificate and key for `name` and `ips` at unix time `now`: the kept pair while it covers them and has a month left,
/// otherwise a new pair covering them and every address the kept one did, saved in `dir`.
pub fn ensure(dir: &Path, name: &str, ips: &[IpAddr], now: i64) -> Result<(Vec<u8>, Vec<u8>)> {
    let file = dir.join("certificate.json");
    let kept = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Kept>(&b).ok());
    if let Some(k) = &kept {
        if k.name == name && ips.iter().all(|ip| k.ips.contains(ip)) && now < k.made_at + (VALID_DAYS - RENEW_DAYS) * DAY {
            return Ok((k.cert.clone().into_bytes(), k.key.clone().into_bytes()));
        }
    }
    let mut all = kept.map(|k| k.ips).unwrap_or_default();
    for ip in ips {
        if !all.contains(ip) {
            all.push(*ip);
        }
    }
    let mut params = rcgen::CertificateParams::new(vec![name.to_string()])?;
    params.subject_alt_names.extend(all.iter().map(|ip| rcgen::SanType::IpAddress(*ip)));
    params.distinguished_name.push(rcgen::DnType::CommonName, "KaraAlwaysOK");
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = time::OffsetDateTime::from_unix_timestamp(now - DAY)?;
    params.not_after = time::OffsetDateTime::from_unix_timestamp(now + VALID_DAYS * DAY)?;
    let key = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    let k = Kept { name: name.to_string(), ips: all, made_at: now, cert: cert.pem(), key: key.serialize_pem() };
    std::fs::create_dir_all(dir)?;
    write_atomic(&file, &serde_json::to_vec(&k)?)?;
    Ok((k.cert.into_bytes(), k.key.into_bytes()))
}

/// The Mac's private IPv4 addresses, Wi-Fi and Ethernet first.
pub fn lan_ips() -> Vec<IpAddr> {
    let mut found: Vec<(bool, String, IpAddr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.ip().is_ipv4() && !i.is_loopback() && is_lan(i.ip()))
        .map(|i| (!i.name.starts_with("en"), i.name.clone(), i.ip()))
        .collect();
    found.sort();
    found.into_iter().map(|(.., ip)| ip).collect()
}

/// Whether an address is on the local network: private, link-local or this Mac.
pub fn is_lan(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_lan(v4.into()),
            None => v6.is_loopback() || v6.segments()[0] & 0xfe00 == 0xfc00 || v6.segments()[0] & 0xffc0 == 0xfe80,
        },
    }
}

/// This Mac's name on the local network, like "Yasushis-MacBook-Pro.local".
pub fn local_name() -> Option<String> {
    let out = std::process::Command::new("scutil").args(["--get", "LocalHostName"]).output().ok()?;
    let name = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !name.is_empty()).then(|| format!("{name}.local"))
}
```

Above the tests in `app/src-tauri/src/phones/room.rs`:

```rust
//! Who is in the room: up to four phones, each keeping its row through a dropped connection.

use tokio::sync::mpsc::UnboundedSender;

pub const MAX_PHONES: usize = 4;
pub const ENDED: u16 = 4001;

/// What goes out on a phone's connection.
pub enum Out {
    Text(String),
    Close(u16),
}

pub struct Guest {
    pub id: String,
    pub name: String,
    pub conn: u64,
    pub tx: Option<UnboundedSender<Out>>,
}

impl Guest {
    pub fn send(&self, out: Out) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(out);
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Refusal {
    WrongCode,
    Full,
}

impl Refusal {
    pub fn close_code(&self) -> u16 {
        match self {
            Self::Full => 4002,
            Self::WrongCode => 4003,
        }
    }
}

pub struct Room {
    pub code: String,
    pub guests: Vec<Guest>,
}

impl Room {
    pub fn new(code: &str) -> Self {
        Self { code: digits(code), guests: Vec::new() }
    }

    /// Lets a phone in with the right code: back into its own row if it was here (false), or a new row while there is room (true).
    pub fn admit(&mut self, code: &str, id: &str, name: &str, conn: u64, tx: UnboundedSender<Out>) -> Result<bool, Refusal> {
        if digits(code) != self.code {
            return Err(Refusal::WrongCode);
        }
        if let Some(g) = self.guest(id) {
            g.name = name.to_string();
            g.conn = conn;
            g.tx = Some(tx);
            return Ok(false);
        }
        if self.guests.len() >= MAX_PHONES {
            return Err(Refusal::Full);
        }
        self.guests.push(Guest { id: id.to_string(), name: name.to_string(), conn, tx: Some(tx) });
        Ok(true)
    }

    /// Marks a phone's connection gone, unless it already came back on a newer one.
    pub fn dropped(&mut self, id: &str, conn: u64) {
        if let Some(g) = self.guests.iter_mut().find(|g| g.id == id && g.conn == conn) {
            g.tx = None;
        }
    }

    pub fn guest(&mut self, id: &str) -> Option<&mut Guest> {
        self.guests.iter_mut().find(|g| g.id == id)
    }

    pub fn remove(&mut self, id: &str) -> Option<Guest> {
        let i = self.guests.iter().position(|g| g.id == id)?;
        Some(self.guests.remove(i))
    }
}

/// Only the digits of a typed code, so "OKI-4827", "oki 4827" and "4827" match.
pub fn digits(code: &str) -> String {
    code.chars().filter(char::is_ascii_digit).collect()
}

/// Four random digits.
pub fn new_code() -> String {
    let mut b = [0u8; 2];
    let _ = getrandom::fill(&mut b);
    format!("{:04}", u16::from_le_bytes(b) % 10_000)
}
```

Create `app/src-tauri/src/phones/server.rs`:

```rust
//! The HTTPS server phones use: the phone page, song pictures, and one WebSocket per phone.

use super::room::Out;
use anyhow::Context;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use serde::Deserialize;
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio::time::Instant;

const SILENT: Duration = Duration::from_secs(5);

/// What a phone tells the Mac.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum FromPhone {
    Join { code: String, id: String, name: String },
    Ping,
    Leave,
}

/// Serves on port 443 (8443 when taken) and sends plain requests on port 80 there; returns the HTTPS port and whether port 80 is
/// served. A debug build with `KARA_PHONE_PORT` serves only that port and leaves port 80 alone.
pub fn serve(app: AppHandle, tls: RustlsConfig, handle: axum_server::Handle) -> anyhow::Result<(u16, bool)> {
    let chosen = if cfg!(debug_assertions) { std::env::var("KARA_PHONE_PORT").ok().and_then(|p| p.parse::<u16>().ok()) } else { None };
    let ports = chosen.map_or(vec![443, 8443], |p| vec![p]);
    let (listener, port) = ports.into_iter().find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok().map(|l| (l, p))).context("no free port")?;
    listener.set_nonblocking(true)?;
    let routes = Router::new()
        .route("/ws", get(socket))
        .route("/art/{name}", get(art))
        .route("/", get(|| async { Redirect::temporary("/phone") }))
        .fallback(page)
        .layer(middleware::from_fn(lan_only))
        .with_state(app);
    let secure = axum_server::from_tcp_rustls(listener, tls).handle(handle.clone());
    tauri::async_runtime::spawn(secure.serve(routes.into_make_service_with_connect_info::<SocketAddr>()));
    if chosen.is_some() {
        return Ok((port, false));
    }
    let Ok(plain) = TcpListener::bind(("0.0.0.0", 80)) else { return Ok((port, false)) };
    plain.set_nonblocking(true)?;
    let to_https = move |headers: HeaderMap| async move {
        let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or_default();
        let host = host.split(':').next().unwrap_or_default();
        let port = if port == 443 { String::new() } else { format!(":{port}") };
        Redirect::temporary(&format!("https://{host}{port}/phone"))
    };
    tauri::async_runtime::spawn(axum_server::from_tcp(plain).handle(handle).serve(Router::new().fallback(to_https).into_make_service()));
    Ok((port, true))
}

async fn lan_only(ConnectInfo(peer): ConnectInfo<SocketAddr>, request: Request, next: Next) -> Response {
    if super::cert::is_lan(peer.ip()) {
        next.run(request).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

/// The phone page and its files: from the dev server under `tauri dev`, from the app's own files otherwise. Only GET.
async fn page(State(app): State<AppHandle>, method: Method, uri: Uri) -> Response {
    if method != Method::GET {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if tauri::is_dev() {
        let Some(base) = app.config().build.dev_url.clone() else { return StatusCode::NOT_FOUND.into_response() };
        let Ok(url) = base.join(uri.path_and_query().map_or("/", |p| p.as_str())) else { return StatusCode::BAD_REQUEST.into_response() };
        let Ok(r) = reqwest::get(url).await else { return StatusCode::BAD_GATEWAY.into_response() };
        let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let kind = r.headers().get(header::CONTENT_TYPE).cloned();
        let mut res = (status, r.bytes().await.unwrap_or_default()).into_response();
        if let Some(kind) = kind {
            res.headers_mut().insert(header::CONTENT_TYPE, kind);
        }
        return res;
    }
    match app.asset_resolver().get(uri.path().to_string()) {
        Some(a) => ([(header::CONTENT_TYPE, a.mime_type)], a.bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A song picture from the artwork folder.
async fn art(State(app): State<AppHandle>, Path(name): Path<String>) -> Response {
    if name.contains(['/', '\\']) || name.starts_with('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    let kind = match name.rsplit('.').next() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };
    let dir = app.state::<crate::state::AppState>().store.artwork_dir();
    match std::fs::read(dir.join(&name)) {
        Ok(bytes) => ([(header::CONTENT_TYPE, kind)], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn socket(State(app): State<AppHandle>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |ws| phone(app, ws))
}

/// One phone's connection: it joins first, then messages flow both ways until either side closes or the phone goes silent.
async fn phone(app: AppHandle, mut ws: WebSocket) {
    let Ok(Some(Ok(Message::Text(first)))) = tokio::time::timeout(Duration::from_secs(10), ws.recv()).await else { return };
    let Ok(FromPhone::Join { code, id, name }) = serde_json::from_str(first.as_str()) else { return };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let conn = match super::admit(&app, &code, &id, &name, tx) {
        Ok(conn) => conn,
        Err(close) => return close_with(&mut ws, close).await,
    };
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(Out::Text(text)) => if ws.send(Message::Text(text.into())).await.is_err() { break },
                Some(Out::Close(close)) => return close_with(&mut ws, close).await,
                None => return,
            },
            msg = ws.recv() => {
                heard = Instant::now();
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(FromPhone::Leave) = serde_json::from_str(text.as_str()) {
                            super::leave(&app, &id);
                            return close_with(&mut ws, 1000).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            },
            _ = tokio::time::sleep_until(heard + SILENT) => break,
        }
    }
    super::dropped(&app, &id, conn);
}

async fn close_with(ws: &mut WebSocket, code: u16) {
    let _ = ws.send(Message::Close(Some(CloseFrame { code, reason: Utf8Bytes::from_static("") }))).await;
}
```

Between the `mod` lines and the tests in `app/src-tauri/src/phones/mod.rs`:

```rust
use crate::player::PlayerSnapshot;
use crate::state::{AppError, AppState, Plain};
use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use kara_core::problem::Problem;
use qrcode::render::svg;
use qrcode::QrCode;
use room::{Out, Room, ENDED};
use serde::Serialize;
use serde_json::Value;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedSender;

static NEXT: AtomicU64 = AtomicU64::new(1);

/// The phone session while there is one; `opening` keeps two opens from starting two servers.
#[derive(Default)]
pub struct Phones {
    session: Mutex<Option<Session>>,
    opening: tauri::async_runtime::Mutex<()>,
}

struct Session {
    room: Room,
    window_open: bool,
    join: JoinInfo,
    server: axum_server::Handle,
}

#[derive(Clone, Serialize)]
pub struct JoinInfo {
    qr: String,
    code: String,
    host: String,
}

#[derive(Clone, Serialize)]
pub struct PhoneRow {
    id: String,
    name: String,
    connected: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct PhonesView {
    join: Option<JoinInfo>,
    phones: Vec<PhoneRow>,
}

/// What the Mac toasts about phones.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum News {
    Joined { name: String, mic: usize },
}

/// What the Mac tells a phone.
#[derive(Serialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ToPhone<'a> {
    Joined,
    Player { snapshot: &'a PlayerSnapshot },
}

impl Session {
    fn view(&self) -> PhonesView {
        let phones = self.room.guests.iter().map(|g| PhoneRow { id: g.id.clone(), name: g.name.clone(), connected: g.tx.is_some() }).collect();
        PhonesView { join: Some(self.join.clone()), phones }
    }
}

/// Starts a session with `code`: certificate, server, join details.
async fn start(app: &AppHandle, code: &str) -> anyhow::Result<Session> {
    let ips = cert::lan_ips();
    let ip = *ips.first().context(Problem::NoNetwork)?;
    let host = cert::local_name().unwrap_or_else(|| ip.to_string());
    let dir = app.try_state::<AppState>().context(Problem::LibraryOpen)?.store.root().join("phones");
    let (cert_pem, key_pem) = cert::ensure(&dir, &host, &ips, kara_core::now_ms() / 1000).context(Problem::PhonesStart)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls = RustlsConfig::from_pem(cert_pem, key_pem).await.context(Problem::PhonesStart)?;
    let server = axum_server::Handle::new();
    let (port, redirect) = server::serve(app.clone(), tls, server.clone()).context(Problem::PhonesStart)?;
    let room = Room::new(code);
    let join = join_info(&host, ip, port, redirect, &room.code).context(Problem::PhonesStart)?;
    Ok(Session { room, window_open: false, join, server })
}

/// What the Mac window shows to join: a QR code of the address with the code, the code, and the address to type
/// (the name alone while the port-80 redirect runs, the full address otherwise).
fn join_info(host: &str, ip: IpAddr, port: u16, redirect: bool, code: &str) -> anyhow::Result<JoinInfo> {
    let at = |h: &str| if port == 443 { h.to_string() } else { format!("{h}:{port}") };
    let url = format!("https://{}/phone?code={code}", at(&ip.to_string()));
    let qr = QrCode::new(url.as_bytes())?
        .render::<svg::Color>()
        .min_dimensions(176, 176)
        .quiet_zone(false)
        .dark_color(svg::Color("currentColor"))
        .light_color(svg::Color("transparent"))
        .build();
    let host = if redirect { host.to_string() } else { format!("https://{}", at(host)) };
    Ok(JoinInfo { qr, code: format!("OKI-{code}"), host })
}

/// Opens a session with `code` unless one is running.
pub async fn ensure_session(app: &AppHandle, code: String) -> anyhow::Result<()> {
    let phones = app.state::<Phones>();
    let _once = phones.opening.lock().await;
    if phones.session.lock().unwrap().is_some() {
        return Ok(());
    }
    let session = start(app, &code).await?;
    *phones.session.lock().unwrap() = Some(session);
    Ok(())
}

/// Runs `f` on the session, if there is one.
fn with<T>(app: &AppHandle, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let phones = app.try_state::<Phones>()?;
    let mut slot = phones.session.lock().unwrap();
    slot.as_mut().map(f)
}

/// Tells the Mac window who is here now.
fn changed(app: &AppHandle) {
    if let Some(view) = with(app, |s| s.view()) {
        let _ = app.emit("phones", view);
    }
}

/// Ends the session: every phone hears it ended and the server stops.
pub fn end(app: &AppHandle) {
    let Some(s) = app.try_state::<Phones>().and_then(|p| p.session.lock().unwrap().take()) else { return };
    for g in &s.room.guests {
        g.send(Out::Close(ENDED));
    }
    s.server.graceful_shutdown(Some(Duration::from_secs(1)));
    let _ = app.emit("phones", PhonesView::default());
}

/// Opens the window's session (starting one with `code` if none runs) and returns what the window shows.
pub async fn open(app: &AppHandle, code: String) -> anyhow::Result<PhonesView> {
    ensure_session(app, code).await?;
    Ok(with(app, |s| {
        s.window_open = true;
        s.view()
    })
    .unwrap_or_default())
}

#[tauri::command]
pub async fn phones_open(app: AppHandle) -> Result<PhonesView, AppError> {
    open(&app, room::new_code()).await.plain()
}

#[tauri::command]
pub fn phones_close(app: AppHandle) {
    let idle = with(&app, |s| {
        s.window_open = false;
        s.room.guests.is_empty()
    });
    if idle == Some(true) {
        end(&app);
    }
}

#[tauri::command]
pub fn phone_remove(app: AppHandle, id: String) {
    with(&app, |s| {
        if let Some(g) = s.room.remove(&id) {
            g.send(Out::Close(ENDED));
        }
    });
    changed(&app);
}

/// Lets a phone in or names the close code refusing it; a phone let in hears it joined and what's playing.
pub(crate) fn admit(app: &AppHandle, code: &str, id: &str, name: &str, tx: UnboundedSender<Out>) -> Result<u64, u16> {
    let conn = NEXT.fetch_add(1, Ordering::Relaxed);
    let joined = with(app, |s| s.room.admit(code, id, name, conn, tx.clone()).map(|new| new.then_some(s.room.guests.len())))
        .ok_or(ENDED)?
        .map_err(|r| r.close_code())?;
    changed(app);
    if let Some(mic) = joined {
        let _ = app.emit("phone-news", News::Joined { name: name.to_string(), mic });
    }
    let _ = tx.send(Out::Text(encode(&ToPhone::Joined)));
    if let Ok(snapshot) = crate::player::player_state(app.state()) {
        let _ = tx.send(Out::Text(encode(&ToPhone::Player { snapshot: &snapshot })));
    }
    Ok(conn)
}

/// A phone left: its row goes, and the session ends when nobody is left and the window is closed.
pub(crate) fn leave(app: &AppHandle, id: &str) {
    let idle = with(app, |s| {
        s.room.remove(id);
        !s.window_open && s.room.guests.is_empty()
    });
    if idle == Some(true) {
        end(app);
    } else {
        changed(app);
    }
}

/// A phone's connection closed; its row stays, greyed, until it comes back or is removed.
pub(crate) fn dropped(app: &AppHandle, id: &str, conn: u64) {
    with(app, |s| s.room.dropped(id, conn));
    changed(app);
}

/// A message as a phone reads it: JSON whose song pictures point at this server.
pub(crate) fn encode(msg: &ToPhone) -> String {
    let mut v = serde_json::to_value(msg).unwrap_or_default();
    art_links(&mut v);
    v.to_string()
}

fn art_links(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, x) in map.iter_mut() {
                match x {
                    Value::String(path) if k == "artworkPath" => {
                        let name = std::path::Path::new(path.as_str()).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        *path = format!("/art/{name}");
                    }
                    _ => art_links(x),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(art_links),
        _ => {}
    }
}
```

In `app/src-tauri/src/lib.rs`:
- in `setup`, after `let _ = engine::open_library(app.handle());`:

```rust
            app.manage(phones::Phones::default());
            #[cfg(debug_assertions)]
            {
                if let Ok(code) = std::env::var("KARA_PHONE_CODE") {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = phones::open(&handle, code).await;
                    });
                }
            }
```

- add `phones::phones_open, phones::phones_close, phones::phone_remove,` to `generate_handler!`.
- replace the `.run(...)` closure:

```rust
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. } => phones::end(app),
            tauri::RunEvent::Exit => kara_core::quiet::silence_output(),
            _ => {}
        });
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app phones:: && cargo test -p kara-core problem && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (cert 2, room 2, encode 1; the problem-code scan still passes); no warnings.

- [ ] **Step 6: The isolated check**

The user runs their dev app (`kara-app`, Vite on port 1420) from their own checkout while tasks run in the worktree, so no check may stop it, use port 1420 or 443/80, or run `app/scripts/app-check.sh`. This check builds its own copy with the page inside (which also exercises the release path: `/phone` and `/_app/*` from embedded files), gives it its own phone port (`KARA_PHONE_PORT`, default 8543), runs it by process id and stops only that process.

In `app/scripts/window-id.swift`, let an argument be a process id as well as a process name:

```swift
// Prints the window number of the first normal on-screen window owned by one of the given processes (names or process ids).
import CoreGraphics

let owners = Set(CommandLine.arguments.dropFirst())
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let owned = { (w: [String: Any]) in
    owners.contains(w[kCGWindowOwnerName as String] as? String ?? "") || owners.contains(String(w[kCGWindowOwnerPID as String] as? Int ?? -1))
}
if let w = windows.first(where: { owned($0) && ($0[kCGWindowLayer as String] as? Int) == 0 }) {
    print(w[kCGWindowNumber as String]!)
}
```

Create `app/scripts/isolated-check.sh`:

```zsh
#!/bin/zsh
# Builds the app with its page inside, runs that build on its own (scratch data, its own target folder and process), saves a
# picture of its window to $1, runs $2 (if given) while it is open, then stops only that process. Fails when it didn't build,
# start or show a window, or logged a panic or an error. Never touches port 1420 or another running KaraAlwaysOK.
set -u
shot=${1:A}
log=${shot:r}.log
cd "${0:A:h}/.."
work=${PWD:h}/.superpowers/sdd/2026-09-27-phase2-phone-mics
mkdir -p "${shot:h}" "$work/data"
rm -f "$shot" "${shot:r}.while-open.txt"
export KARA_PHONE_PORT=${KARA_PHONE_PORT:-8543}
fail() { echo "FAIL: $1 Output: $log"; exit 1; }

npm run build >"$log" 2>&1 || fail "The page didn't build."
source "$HOME/.cargo/env"
CARGO_TARGET_DIR="$work/target" cargo build -p kara-app --features tauri/custom-protocol >>"$log" 2>&1 || fail "The app didn't build."
KARA_DATA="$work/data" "$work/target/debug/kara-app" >>"$log" 2>&1 &
app=$!
trap 'kill $app 2>/dev/null; sleep 2; kill -9 $app 2>/dev/null' EXIT
trap "exit 130" INT TERM

id=""
for _ in {1..60}; do
  id=$(swift scripts/window-id.swift $app)
  [[ -n $id ]] && break
  kill -0 $app 2>/dev/null || fail "The app quit on its own."
  sleep 1
done
[[ -n $id ]] || fail "No window within 60 s."
sleep 2
caffeinate -u -t 2
screencapture -x -o -l "$id" "$shot"
[[ -n ${2:-} ]] && eval "$2" >"${shot:r}.while-open.txt" 2>&1
kill $app 2>/dev/null
sleep 2
kill -9 $app 2>/dev/null
trap - EXIT INT TERM

sed -n '/Finished/,$p' "$log" | grep -nE 'panicked|^error' && fail "Errors above."
[[ -s $shot ]] || fail "No picture was saved (Screen Recording permission, or the display is asleep)."
echo "OK. Picture: $shot"
```

- [ ] **Step 7: The phone check script**

Create `app/scripts/phone-check.ts`:

```ts
/** Talks to a running app's phone server the way phones do and prints "phone check OK" when it answers as it should. It talks only
 * to the port the app under test was given (KARA_PHONE_PORT), never to the user's own app. Run with NODE_TLS_REJECT_UNAUTHORIZED=0. */
const code = process.env.KARA_PHONE_CODE ?? "";
const origin = `https://127.0.0.1:${process.env.KARA_PHONE_PORT ?? "8543"}`;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

interface Phone { ws: WebSocket; got: { t: string; [k: string]: unknown }[]; closed: Promise<number> }

/** Joins as phone `id` and keeps the connection alive with a ping every second, as the phone page does. */
function phone(origin: string, id: string, joinCode = code): Promise<Phone> {
  const ws = new WebSocket(`${origin.replace("https", "wss")}/ws`);
  ws.binaryType = "arraybuffer";
  const got: Phone["got"] = [];
  const ping = setInterval(() => ws.readyState === WebSocket.OPEN && ws.send(JSON.stringify({ t: "ping" })), 1000);
  const closed = new Promise<number>((done) => ws.addEventListener("close", (e) => (clearInterval(ping), done(e.code))));
  ws.addEventListener("message", (e) => typeof e.data === "string" && got.push(JSON.parse(e.data)));
  return new Promise((ok, fail) => {
    ws.addEventListener("open", () => {
      ws.send(JSON.stringify({ t: "join", code: joinCode, id, name: `Guest ${id}` }));
      ok({ ws, got, closed });
    });
    ws.addEventListener("error", () => fail(new Error(`phone ${id} couldn't connect`)));
  });
}

async function until(test: () => boolean, what: string) {
  for (let i = 0; i < 50 && !test(); i++) await wait(100);
  if (!test()) throw new Error(`timed out waiting for ${what}`);
}

const heard = (p: Phone, t: string) => p.got.some((m) => m.t === t);

/** Whether the server at `origin` lets a phone in with our code. */
async function accepts(origin: string): Promise<boolean> {
  try {
    const p = await phone(origin, "probe");
    for (let i = 0; i < 20; i++) {
      if (heard(p, "joined")) {
        p.ws.send(JSON.stringify({ t: "leave" }));
        await p.closed;
        return true;
      }
      if (p.ws.readyState > WebSocket.OPEN) return false;
      await wait(100);
    }
    p.ws.close();
  } catch {
    return false;
  }
  return false;
}

/** Waits until the app under test has its phone server up. */
async function ready() {
  for (let i = 0; i < 30; i++) {
    if (await accepts(origin)) return;
    await wait(1000);
  }
  throw new Error("the app under test never took our code");
}

await ready();
const html = await (await fetch(`${origin}/phone`)).text();
const script = html.match(/["'](?:\.\/|\/)(_app\/[^"']+\.js)["']/)?.[1];
if (!html.includes("<html") || !script || !(await fetch(`${origin}/${script}`)).ok) throw new Error("the phone page or its files didn't load");
const wrong = await phone(origin, "wrong", code === "0000" ? "1111" : "0000");
if ((await wrong.closed) !== 4003) throw new Error("a wrong code was not refused");
const guests = await Promise.all(["1", "2", "3", "4"].map((id) => phone(origin, id)));
for (const g of guests) await until(() => heard(g, "joined") && heard(g, "player"), "joined and the queue");
const fifth = await phone(origin, "5");
if ((await fifth.closed) !== 4002) throw new Error("a fifth phone was not refused");
const first = guests[0];
first.ws.send(JSON.stringify({ t: "leave" }));
await first.closed;
const again = await phone(origin, "6");
await until(() => heard(again, "joined"), "a phone joining after one left");

console.log("phone check OK");
for (const g of [...guests.slice(1), again]) g.ws.close();
```

Tasks 7 and 8 insert their checks before the `console.log` line, using `guests[1]`.

Run the isolated check (Global Constraints) for `task-06.png`, then `grep -q 'phone check OK' ../.superpowers/sdd/2026-09-27-phase2-phone-mics/shots/task-06.while-open.txt`.
Expected: `OK. Picture: …` and the grep succeeds. The first build in its own target folder takes several minutes. If the grep fails, read `task-06.while-open.txt` and `task-06.log`. (macOS may show an "accept incoming connections" prompt; it doesn't block connections from this Mac.)

- [ ] **Step 8: Commit**

```bash
git add crates/kara-core/src/problem.rs app/src-tauri/Cargo.toml Cargo.lock app/src-tauri/src/state.rs app/src-tauri/src/lib.rs app/src-tauri/src/phones app/scripts/phone-check.ts app/scripts/isolated-check.sh app/scripts/window-id.swift
git commit -m "feat(app): phone server with a kept certificate, LAN only, a join code and four rows"
```

---
### Task 7: Phone commands — queue, links, singer, search, lyrics and where the song is

**Files:**
- Modify: `app/src-tauri/src/phones/mod.rs`, `phones/server.rs`
- Modify: `app/src-tauri/src/player.rs` (broadcast to phones)
- Modify: `app/src-tauri/src/lib.rs` (command, lyrics hook)
- Modify: `app/scripts/phone-check.ts`

**Interfaces:**
- Consumes: Task 6's `with`, `encode`, `ToPhone`, `News`, `admit`, `Guest::send`, `FromPhone::Ping`; Phase 1b commands called as plain functions: `player::set_singer(app, state, value: u8)`, `player::queue_add(app, state, track_id, next, by)` (Task 5), `adding::start_adding(state, track_id) -> Result<bool, AppError>`, `library::delete_track(app, state, track_id)`, `player::queue_move(app, state, key: u64, to: usize)`, `player::queue_remove(app, state, key: u64)`, `adding::add_link(state, url: String) -> Result<Track, AppError>`, `adding::search_input(lib, input, imported) -> anyhow::Result<SearchOutcome>`, `library::track_lyrics(state, track_id) -> Result<Lyrics, AppError>`; `AppError.problem`.
- Produces:
  - `FromPhone` gains `Singer { v: u8 }`, `Add { track_id: i64, next: bool }`, `AddLink { url: String, next: bool }`, `Move { key: u64, to: usize }`, `Remove { key: u64 }`, `Search { q: String, imported: String }`, `Lyrics { track_id: i64 }` (JSON `trackId`).
  - `ToPhone` gains `Clock { key: Option<u64>, position_ms: i64, playing: bool }`, `Lyrics { track_id: i64, lyrics: &Lyrics }`, `LyricsChanged { track_id: i64 }`, `Results { q: &str, outcome: &SearchOutcome }`, `Refused { problem: Option<Problem> }`.
  - `pub(crate) fn handle(app, id, name, msg: FromPhone)`; `pub fn send_player(app, snapshot: &PlayerSnapshot)`; `pub fn lyrics_changed(app, track_id: i64)`; `pub fn link_done(app, e: &Event)` (the engine event thread calls it for every event).
  - `News` gains `Added { name: String, title: String }`, sent when a guest's song is queued (Task 9 toasts "Aiko added “title”").
  - A guest's link follows the Mac's flow: `add_link`, then `start_adding`; it is queued (with the guest's name) once the engine says `Added`, or at once when `start_adding` says the song already has its audio or is queued. If the engine says `Failed`, the song leaves the library again and the guest gets `refused` with the engine's problem code.
  - Command `phones_clock(key: Option<u64>, position_ms: i64, playing: bool)` (the Mac reports where the song is; Task 9 calls it). A phone that joins gets `joined`, `player`, then `clock` (moved on by the time since the report while playing).
  - Tauri event `"library"` (no payload) when a guest's link enters or leaves the library, so the Mac refreshes it (Task 9 listens).
  - An empty phone search lists every song (the Imported playlist) by title; otherwise the Mac's search.

- [ ] **Step 1: Write the failing test**

Add to the tests in `app/src-tauri/src/phones/mod.rs`:

```rust
    #[test]
    fn an_empty_phone_search_lists_every_song_by_title() {
        let lib = Library::open_in_memory().unwrap();
        let imported = lib.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap();
        for title in ["lemon Skies", "Paper Boats", "Kettle Duet"] {
            let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist: None, album: None, duration_ms: None }).unwrap();
            lib.add_to_collection(imported, t).unwrap();
        }
        let titles = |q: &str| match phone_search(&lib, q, "Imported").unwrap() {
            SearchOutcome::Text { tracks, .. } => tracks.into_iter().map(|t| t.title).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert_eq!(titles(" "), ["Kettle Duet", "lemon Skies", "Paper Boats"]);
        assert_eq!(titles("paper"), ["Paper Boats"]);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app phones::tests::an_empty`
Expected: FAIL to compile — `cannot find function phone_search`.

- [ ] **Step 3: Write the implementation**

`app/src-tauri/src/phones/server.rs` — the full message list, and every message but `Leave` goes to `handle`:

```rust
/// What a phone tells the Mac.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum FromPhone {
    Join { code: String, id: String, name: String },
    Singer { v: u8 },
    Add { track_id: i64, next: bool },
    AddLink { url: String, next: bool },
    Move { key: u64, to: usize },
    Remove { key: u64 },
    Search { q: String, imported: String },
    Lyrics { track_id: i64 },
    Ping,
    Leave,
}
```

and in `phone()` the text arm becomes:

```rust
                Some(Ok(Message::Text(text))) => match serde_json::from_str(text.as_str()) {
                    Ok(FromPhone::Leave) => {
                        super::leave(&app, &id);
                        return close_with(&mut ws, 1000).await;
                    }
                    Ok(msg) => super::handle(&app, &id, &name, msg),
                    Err(_) => {}
                },
```

`app/src-tauri/src/phones/mod.rs`:

- imports: add `use crate::adding::{self, SearchOutcome};`, `use crate::library::{self, Lyrics};`, `use crate::player;`, `use kara_core::jobs::Event;`, `use kara_core::library::{CollectionKind, Library};`, `use server::FromPhone;`, and `std::time::Instant` next to `Duration`.
- `News` gains `Added { name: String, title: String },`.
- `ToPhone` becomes:

```rust
/// What the Mac tells a phone.
#[derive(Serialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ToPhone<'a> {
    Joined,
    Player { snapshot: &'a PlayerSnapshot },
    Clock { key: Option<u64>, position_ms: i64, playing: bool },
    Lyrics { track_id: i64, lyrics: &'a Lyrics },
    LyricsChanged { track_id: i64 },
    Results { q: &'a str, outcome: &'a SearchOutcome },
    Refused { problem: Option<Problem> },
}
```

- `Session` gains `clock: Clock` and `links: Vec<PendingLink>`, set in `start` to `Clock { key: None, position_ms: 0, playing: false, at: Instant::now() }` and `Vec::new()`, with:

```rust
/// A song a guest added by link, queued once it has been downloaded.
struct PendingLink {
    track_id: i64,
    guest: String,
    name: String,
    next: bool,
}
```

and:

```rust
/// Where the Mac last said the song was, and when.
struct Clock {
    key: Option<u64>,
    position_ms: i64,
    playing: bool,
    at: Instant,
}

impl Clock {
    /// Where the song is now, as a message for phones.
    fn message(&self) -> ToPhone<'static> {
        let moved = if self.playing { self.at.elapsed().as_millis() as i64 } else { 0 };
        ToPhone::Clock { key: self.key, position_ms: self.position_ms + moved, playing: self.playing }
    }
}
```

- `admit` also sends the clock, read while the session is locked and sent after the snapshot (never lock `AppState` while holding the session):

```rust
pub(crate) fn admit(app: &AppHandle, code: &str, id: &str, name: &str, tx: UnboundedSender<Out>) -> Result<u64, u16> {
    let conn = NEXT.fetch_add(1, Ordering::Relaxed);
    let (joined, clock) = with(app, |s| s.room.admit(code, id, name, conn, tx.clone()).map(|new| (new.then_some(s.room.guests.len()), encode(&s.clock.message()))))
        .ok_or(ENDED)?
        .map_err(|r| r.close_code())?;
    changed(app);
    if let Some(mic) = joined {
        let _ = app.emit("phone-news", News::Joined { name: name.to_string(), mic });
    }
    let _ = tx.send(Out::Text(encode(&ToPhone::Joined)));
    if let Ok(snapshot) = player::player_state(app.state()) {
        let _ = tx.send(Out::Text(encode(&ToPhone::Player { snapshot: &snapshot })));
    }
    let _ = tx.send(Out::Text(clock));
    Ok(conn)
}
```

- new functions:

```rust
/// Carries out what a phone asked for; songs it adds carry the guest's name. A refusal goes back to that phone.
pub(crate) fn handle(app: &AppHandle, id: &str, name: &str, msg: FromPhone) {
    let reply = match msg {
        FromPhone::Singer { v } => player::set_singer(app.clone(), app.state(), v).map(|_| None),
        FromPhone::Add { track_id, next } => queue_for(app, track_id, next, name).map(|()| None),
        FromPhone::AddLink { url, next } => add_link(app, id, name, url, next).map(|()| None),
        FromPhone::Move { key, to } => player::queue_move(app.clone(), app.state(), key, to).map(|_| None),
        FromPhone::Remove { key } => player::queue_remove(app.clone(), app.state(), key).map(|_| None),
        FromPhone::Search { q, imported } => {
            let found = phone_search(&app.state::<AppState>().lib.lock().unwrap(), &q, &imported);
            found.plain().map(|outcome| Some(encode(&ToPhone::Results { q: &q, outcome: &outcome })))
        }
        FromPhone::Lyrics { track_id } => library::track_lyrics(app.state(), track_id).map(|lyrics| Some(encode(&ToPhone::Lyrics { track_id, lyrics: &lyrics }))),
        FromPhone::Join { .. } | FromPhone::Ping | FromPhone::Leave => Ok(None),
    };
    match reply {
        Ok(Some(text)) => tell(app, id, text),
        Ok(None) => {}
        Err(e) => tell(app, id, encode(&ToPhone::Refused { problem: e.problem })),
    }
}

/// Sends `text` to one phone.
fn tell(app: &AppHandle, id: &str, text: String) {
    with(app, |s| {
        if let Some(g) = s.room.guest(id) {
            g.send(Out::Text(text));
        }
    });
}

/// Queues a song for guest `name` and tells the Mac window who added what.
fn queue_for(app: &AppHandle, track_id: i64, next: bool, name: &str) -> Result<(), AppError> {
    let snap = player::queue_add(app.clone(), app.state(), track_id, next, Some(name.to_string()))?;
    let title = snap.entries.iter().rev().find(|e| e.track.id == track_id).map(|e| e.track.title.clone()).unwrap_or_default();
    let _ = app.emit("phone-news", News::Added { name: name.to_string(), title });
    Ok(())
}

/// Adds a guest's link like the Mac does: queued once it's downloaded, or right away when it needs nothing more.
fn add_link(app: &AppHandle, id: &str, name: &str, url: String, next: bool) -> Result<(), AppError> {
    let track = adding::add_link(app.state(), url)?;
    let _ = app.emit("library", ());
    if !adding::start_adding(app.state(), track.id)? {
        return queue_for(app, track.id, next, name);
    }
    with(app, |s| s.links.push(PendingLink { track_id: track.id, guest: id.to_string(), name: name.to_string(), next }));
    Ok(())
}

/// The engine finished adding a song: a guest's link is queued now, or on failure leaves the library and the guest hears why.
pub fn link_done(app: &AppHandle, e: &Event) {
    let (Event::Added { track_id } | Event::Failed { track_id, .. }) = e else { return };
    let Some(link) = with(app, |s| s.links.iter().position(|l| l.track_id == *track_id).map(|i| s.links.remove(i))).flatten() else { return };
    let refused = match e {
        Event::Failed { problem, .. } => {
            let _ = library::delete_track(app.clone(), app.state(), link.track_id);
            let _ = app.emit("library", ());
            Some(*problem)
        }
        _ => queue_for(app, link.track_id, link.next, &link.name).err().map(|e| e.problem),
    };
    if let Some(problem) = refused {
        tell(app, &link.guest, encode(&ToPhone::Refused { problem }));
    }
}

/// What a phone's search shows: every song by title when it is empty, otherwise the Mac's search.
fn phone_search(lib: &Library, q: &str, imported: &str) -> anyhow::Result<SearchOutcome> {
    if !q.trim().is_empty() {
        return adding::search_input(lib, q, imported);
    }
    let mut tracks = Vec::new();
    for c in lib.collections(None, CollectionKind::Playlist)?.into_iter().filter(|c| !c.user) {
        tracks.extend(lib.collection_tracks(c.id)?);
    }
    tracks.sort_by_key(|t| t.title.to_lowercase());
    Ok(SearchOutcome::Text { tracks, collections: Vec::new() })
}

/// Sends one message to every connected phone.
fn broadcast(app: &AppHandle, msg: &ToPhone) {
    with(app, |s| {
        let text = encode(msg);
        for g in &s.room.guests {
            g.send(Out::Text(text.clone()));
        }
    });
}

/// Sends the queue to every phone.
pub fn send_player(app: &AppHandle, snapshot: &PlayerSnapshot) {
    broadcast(app, &ToPhone::Player { snapshot });
}

/// Tells phones a song's lyrics changed, so a phone showing it asks again.
pub fn lyrics_changed(app: &AppHandle, track_id: i64) {
    broadcast(app, &ToPhone::LyricsChanged { track_id });
}

/// The Mac says where the song is; phones follow it for the lyrics.
#[tauri::command]
pub fn phones_clock(app: AppHandle, key: Option<u64>, position_ms: i64, playing: bool) {
    with(&app, |s| s.clock = Clock { key, position_ms, playing, at: Instant::now() });
    broadcast(&app, &ToPhone::Clock { key, position_ms, playing });
}
```

`app/src-tauri/src/player.rs` — phones get every snapshot the Mac gets. In `update`, after `let _ = app.emit("player", &snap);` add `crate::phones::send_player(app, &snap);`; in `refresh_if_queued`, after its `app.emit("player", &snap)` add `crate::phones::send_player(app, &snap);`. (Lyric offsets, including automatic lyric sync, reach phones through these snapshots.)

`app/src-tauri/src/lib.rs`:
- add `phones::phones_clock,` to `generate_handler!`;
- in the engine event thread, after the `refresh_if_queued` line:

```rust
                    if let Event::Lyrics { track_id } = e {
                        phones::lyrics_changed(&handle, track_id);
                    }
                    phones::link_done(&handle, &e);
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all PASS; no warnings.

- [ ] **Step 5: Extend the phone check**

In `app/scripts/phone-check.ts`, insert before `console.log("phone check OK");`:

```ts
const second = guests[1];
await until(() => heard(second, "clock"), "where the song is");
second.ws.send(JSON.stringify({ t: "search", q: "", imported: "Imported" }));
await until(() => second.got.some((m) => m.t === "results" && m.q === ""), "search results");
second.ws.send(JSON.stringify({ t: "add", trackId: 987654321, next: false }));
await until(() => second.got.some((m) => m.t === "refused" && m.problem === "songGone"), "a missing song refused");
second.ws.send(JSON.stringify({ t: "lyrics", trackId: 987654321 }));
await until(() => heard(second, "lyrics"), "lyrics");
```

Run the isolated check with the phone check for `task-07.png` and grep `phone check OK` in `task-07.while-open.txt`.
Expected: both succeed.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/phones app/src-tauri/src/player.rs app/src-tauri/src/lib.rs app/scripts/phone-check.ts
git commit -m "feat(app): phones queue songs, set the singer, search, read lyrics and follow the song"
```

---

### Task 8: Phone sound — output stream, mix, effects, levels, gone phones, a new address, sleep

**Files:**
- Modify: `app/src-tauri/Cargo.toml` (`cpal = "0.16"`)
- Create: `app/src-tauri/src/phones/output.rs`
- Modify: `app/src-tauri/src/phones/mod.rs`, `phones/server.rs`, `phones/room.rs`
- Modify: `app/src-tauri/src/lib.rs` (command)
- Modify: `app/scripts/phone-check.ts`

**Interfaces:**
- Consumes: `kara_core::mic::{Mixer, NewVoice, Level, Effect, voice_gain}` (Tasks 3–4; `add` returns leftovers and `remove` returns `Gone`, both dropped after unlocking); Task 6/7 session code; `cert::ensure`, `cert::lan_ips`, `join_info(host, ip, port, redirect, code)`.
- Produces:
  - `Guest` gains `volume: u8` and `voice: u8` (80 for a new row), `effect: (Effect, u8)` (none, 0) and `dropped_at: Option<Instant>`, all kept across reconnects; `Room::dropped(id, conn, at: Instant)`; `Room::expire(now: Instant) -> Vec<Guest>` removes rows whose phone has been gone `GONE_AFTER` (2 minutes, the phone's own give-up time) or longer.
  - `FromPhone` gains `Live { on: bool, rate: u32 }` (mic on at `rate`, or muted), `Voice { v: u8 }` and `Effect { kind: Effect, amount: u8 }` (JSON `{t:"effect", kind:"none"|"karaokeMix"|"autoTune", amount}`); binary frames = 16-bit LE mono PCM.
  - `ToPhone` gains `Level { v: f32 }` (the phone's loudest post-gain sample in the last 80 ms).
  - `PhoneRow` gains `volume: u8`. The `"phones"` event still goes out only on changes; levels go every 80 ms in a small event `"phone-levels"` with `Vec<PhoneLevel>` (`{ id, level, down }`, built after unlocking the mixer from a reused `Vec<Level>`).
  - `News` gains `Stopped` (`{kind:"stopped"}`): the output device went away or failed (unplugged USB or Bluetooth speakers); the session ends so nothing piles up, and the Mac window, if open, starts a new one on the new default output (Task 9).
  - `output::Output::broken(&self) -> bool` — true once the stream reported an error.
  - Command `phone_volume(id: String, volume: u8)`.
  - `admit` returns `Result<(u64, Arc<Mutex<Mixer>>), u16>`; `pub(crate) fn hear(mixer: &Mutex<Mixer>, id: &str, bytes: &[u8])`.
  - `output::start(floor_ms: f64) -> anyhow::Result<(Output, Arc<Mutex<Mixer>>)>`; dropping `Output` stops the stream. `KARA_MIC_BUFFER_MS` is clamped to 10–60.
  - Every 80 ms the session (identified by `Session.id`) sends levels, removes rows gone for 2 minutes and then ends itself when the window is closed and no phone is left (spec §3.1); every 5 s it checks the Mac's addresses and, when they changed, remakes the certificate, reloads it into the running server and sends a new QR code; when the Mac wakes from sleep it ends.
  - A phone joining or unmuting builds its `NewVoice` before taking the mixer lock, so the audio callback never waits on an FFT plan.

- [ ] **Step 1: Write the failing test**

In `app/src-tauri/src/phones/room.rs` tests, add `use std::time::{Duration, Instant};`, change the two `room.dropped("a", 1)` calls in `a_phone_that_drops_gets_its_own_row_back` to `room.dropped("a", 1, Instant::now())`, and add:

```rust
    #[test]
    fn a_phone_gone_for_two_minutes_loses_its_row() {
        let mut room = Room::new("4827");
        room.admit("4827", "a", "Aiko", 1, tx()).unwrap();
        room.admit("4827", "b", "Ben", 1, tx()).unwrap();
        let t = Instant::now();
        room.dropped("a", 1, t);
        assert!(room.expire(t + GONE_AFTER - Duration::from_secs(1)).is_empty(), "kept while it may still come back");
        let gone = room.expire(t + GONE_AFTER);
        assert_eq!((gone.len(), gone[0].id.as_str(), room.guests.len()), (1, "a", 1));
    }
```

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app phones::room`
Expected: FAIL to compile — `dropped` takes 2 arguments; `expire` and `GONE_AFTER` don't exist.

- [ ] **Step 2: The output stream**

Add `cpal = "0.16"` to `app/src-tauri/Cargo.toml`. Create `app/src-tauri/src/phones/output.rs`:

```rust
//! The phones' mix on its own low-latency output stream to the Mac's default speakers.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use kara_core::mic::Mixer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

const FRAMES: u32 = 256;

/// Keeps the stream playing until dropped.
pub struct Output {
    _stop: mpsc::Sender<()>,
    broken: Arc<AtomicBool>,
}

impl Output {
    /// Whether the stream stopped with an error, e.g. its device went away.
    pub fn broken(&self) -> bool {
        self.broken.load(Ordering::Relaxed)
    }
}

/// Starts playing a new Mixer on the default output device; returns the mixer to feed.
pub fn start(floor_ms: f64) -> Result<(Output, Arc<Mutex<Mixer>>)> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let broken = Arc::new(AtomicBool::new(false));
    let report = broken.clone();
    std::thread::spawn(move || match open(floor_ms, report) {
        Ok((stream, mixer)) => {
            let _ = ready_tx.send(Ok(mixer));
            let _ = stop_rx.recv();
            drop(stream);
        }
        Err(e) => {
            let _ = ready_tx.send(Err(e));
        }
    });
    let mixer = ready_rx.recv()??;
    Ok((Output { _stop: stop_tx, broken }, mixer))
}

/// Opens the default output with a callback that only locks the mixer and renders (no allocation after its first call).
fn open(floor_ms: f64, broken: Arc<AtomicBool>) -> Result<(cpal::Stream, Arc<Mutex<Mixer>>)> {
    let device = cpal::default_host().default_output_device().context("no output device")?;
    let config = device.default_output_config()?;
    let channels = usize::from(config.channels());
    let rate = config.sample_rate();
    let buffer_size = match config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } if (*min..=*max).contains(&FRAMES) => cpal::BufferSize::Fixed(FRAMES),
        _ => cpal::BufferSize::Default,
    };
    let mixer = Arc::new(Mutex::new(Mixer::new(rate.0, floor_ms)));
    let feed = mixer.clone();
    let mut mono = Vec::new();
    let stream = device.build_output_stream(
        &cpal::StreamConfig { channels: config.channels(), sample_rate: rate, buffer_size },
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            mono.resize(data.len() / channels, 0.0);
            feed.lock().unwrap().render(&mut mono);
            for (frame, &x) in data.chunks_mut(channels).zip(&mono) {
                frame.fill(x);
            }
        },
        move |_| broken.store(true, Ordering::Relaxed),
        None,
    )?;
    stream.play()?;
    Ok((stream, mixer))
}
```
- [ ] **Step 3: Rows keep volume, voice and effect, and go after two minutes**

`app/src-tauri/src/phones/room.rs`:
- imports: `use kara_core::mic::Effect;` and `use std::time::{Duration, Instant};`
- `pub const GONE_AFTER: Duration = Duration::from_secs(120);`
- `Guest` gains `pub volume: u8, pub voice: u8, pub effect: (Effect, u8), pub dropped_at: Option<Instant>,`; the new-row push in `admit` sets `volume: 80, voice: 80, effect: (Effect::None, 0), dropped_at: None`, and a returning phone's row gets `g.dropped_at = None;`.
- `dropped` and the new `expire`:

```rust
    /// Marks a phone's connection gone at `at`, unless it already came back on a newer one.
    pub fn dropped(&mut self, id: &str, conn: u64, at: Instant) {
        if let Some(g) = self.guests.iter_mut().find(|g| g.id == id && g.conn == conn) {
            g.tx = None;
            g.dropped_at = Some(at);
        }
    }

    /// Removes and returns the rows whose phone has been gone for GONE_AFTER or longer.
    pub fn expire(&mut self, now: Instant) -> Vec<Guest> {
        let (gone, kept) = std::mem::take(&mut self.guests).into_iter().partition(|g| g.dropped_at.is_some_and(|at| now.duration_since(at) >= GONE_AFTER));
        self.guests = kept;
        gone
    }
```

- [ ] **Step 4: Sound into the mix, levels out, gone phones, addresses and sleep**

`app/src-tauri/src/phones/server.rs`:
- `FromPhone` gains `Live { on: bool, rate: u32 },`, `Voice { v: u8 },` and `Effect { kind: kara_core::mic::Effect, amount: u8 },`.
- in `phone()`: `let (conn, mixer) = match super::admit(&app, &code, &id, &name, tx) { Ok(joined) => joined, Err(close) => return close_with(&mut ws, close).await };` and a binary arm before `Some(Ok(_)) => {}`:

```rust
                    Some(Ok(Message::Binary(bytes))) => super::hear(&mixer, &id, &bytes),
```

`app/src-tauri/src/phones/mod.rs`:
- `mod output;` with the other modules; imports add `kara_core::mic::{voice_gain, Level, Mixer, NewVoice}`, `room::Guest`, `std::path::PathBuf`, `std::sync::Arc`, `std::time::SystemTime`.
- `ToPhone` gains `Level { v: f32 },`.
- `PhoneRow` gains `volume: u8,`, filled in `view` as `volume: g.volume`.
- `Session` gains:

```rust
    id: u64,
    mixer: Arc<Mutex<Mixer>>,
    output: output::Output,
    levels: Vec<Level>,
    tls: RustlsConfig,
    dir: PathBuf,
    host: String,
    port: u16,
    redirect: bool,
    ips: Vec<IpAddr>,
```

- `start` opens the output and keeps what re-addressing needs:

```rust
async fn start(app: &AppHandle, code: &str) -> anyhow::Result<Session> {
    let ips = cert::lan_ips();
    let ip = *ips.first().context(Problem::NoNetwork)?;
    let host = cert::local_name().unwrap_or_else(|| ip.to_string());
    let dir = app.try_state::<AppState>().context(Problem::LibraryOpen)?.store.root().join("phones");
    let (cert_pem, key_pem) = cert::ensure(&dir, &host, &ips, kara_core::now_ms() / 1000).context(Problem::PhonesStart)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls = RustlsConfig::from_pem(cert_pem, key_pem).await.context(Problem::PhonesStart)?;
    let floor = std::env::var("KARA_MIC_BUFFER_MS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(20.0).clamp(10.0, 60.0);
    let (output, mixer) = output::start(floor).context(Problem::PhonesStart)?;
    let server = axum_server::Handle::new();
    let (port, redirect) = server::serve(app.clone(), tls.clone(), server.clone()).context(Problem::PhonesStart)?;
    let room = Room::new(code);
    let join = join_info(&host, ip, port, redirect, &room.code).context(Problem::PhonesStart)?;
    let clock = Clock { key: None, position_ms: 0, playing: false, at: Instant::now() };
    Ok(Session {
        id: NEXT.fetch_add(1, Ordering::Relaxed),
        room,
        window_open: false,
        join,
        server,
        clock,
        links: Vec::new(),
        mixer,
        output,
        levels: Vec::with_capacity(8),
        tls,
        dir,
        host,
        port,
        redirect,
        ips,
    })
}
```

- `ensure_session` starts the ticker once the session is in place:

```rust
    let session = start(app, &code).await?;
    let id = session.id;
    *phones.session.lock().unwrap() = Some(session);
    tauri::async_runtime::spawn(tick(app.clone(), id));
    Ok(())
```

- `admit` hands the socket the mixer:

```rust
pub(crate) fn admit(app: &AppHandle, code: &str, id: &str, name: &str, tx: UnboundedSender<Out>) -> Result<(u64, Arc<Mutex<Mixer>>), u16> {
    let conn = NEXT.fetch_add(1, Ordering::Relaxed);
    let (joined, clock, mixer) = with(app, |s| {
        s.room.admit(code, id, name, conn, tx.clone()).map(|new| (new.then_some(s.room.guests.len()), encode(&s.clock.message()), s.mixer.clone()))
    })
    .ok_or(ENDED)?
    .map_err(|r| r.close_code())?;
    changed(app);
    if let Some(mic) = joined {
        let _ = app.emit("phone-news", News::Joined { name: name.to_string(), mic });
    }
    let _ = tx.send(Out::Text(encode(&ToPhone::Joined)));
    if let Ok(snapshot) = player::player_state(app.state()) {
        let _ = tx.send(Out::Text(encode(&ToPhone::Player { snapshot: &snapshot })));
    }
    let _ = tx.send(Out::Text(clock));
    Ok((conn, mixer))
}
```

- `dropped` records when: `with(app, |s| s.room.dropped(id, conn, Instant::now()));`
- a phone leaving or being removed leaves the mix, its voice freed after unlocking: in `leave` and `phone_remove`, right after `s.room.remove(…)`, add `let gone = s.mixer.lock().unwrap().remove(id);` and `drop(gone);` (`&id` in `phone_remove`; the `let` ends the lock before the drop).
- `News` gains `Stopped,`; a serializable level for the Mac window:

```rust
/// A phone's level as the Mac window's meter reads it.
#[derive(Serialize)]
pub struct PhoneLevel {
    id: String,
    level: f32,
    down: bool,
}
```
- `handle` gains three arms before `FromPhone::Join { .. } | FromPhone::Ping | FromPhone::Leave`:

```rust
        FromPhone::Live { on, rate } => {
            live(app, id, on, rate);
            Ok(None)
        }
        FromPhone::Voice { v } => {
            set_gain(app, id, |g| g.voice = v.min(100));
            Ok(None)
        }
        FromPhone::Effect { kind, amount } => {
            with(app, |s| {
                if let Some(g) = s.room.guest(id) {
                    g.effect = (kind, amount.min(100));
                    s.mixer.lock().unwrap().set_effect(id, kind, amount);
                }
            });
            Ok(None)
        }
```

- new functions:

```rust
/// A phone's mic went live at `rate` (a fresh buffer, built before taking the mixer lock) or was muted (its buffer empties).
fn live(app: &AppHandle, id: &str, on: bool, rate: u32) {
    if !(8_000..=96_000).contains(&rate) {
        return;
    }
    with(app, |s| {
        let Some(g) = s.room.guest(id) else { return };
        let (gain, (effect, amount)) = (voice_gain(g.volume, g.voice), g.effect);
        if !on {
            return s.mixer.lock().unwrap().reset(id);
        }
        let (out_rate, floor) = {
            let m = s.mixer.lock().unwrap();
            (m.rate(), m.floor_ms())
        };
        let voice = NewVoice::new(id, rate, out_rate, floor);
        let leftover = {
            let mut m = s.mixer.lock().unwrap();
            let leftover = m.add(voice);
            m.set_gain(id, gain);
            m.set_effect(id, effect, amount);
            leftover
        };
        drop(leftover);
    });
}

/// Changes a phone's row with `change` and applies its gain to the mix.
fn set_gain(app: &AppHandle, id: &str, change: impl FnOnce(&mut Guest)) {
    with(app, |s| {
        if let Some(g) = s.room.guest(id) {
            change(g);
            let gain = voice_gain(g.volume, g.voice);
            s.mixer.lock().unwrap().set_gain(id, gain);
        }
    });
}

/// Adds a phone's 16-bit little-endian samples to its buffer.
pub(crate) fn hear(mixer: &Mutex<Mixer>, id: &str, bytes: &[u8]) {
    let samples: Vec<f32> = bytes.chunks_exact(2).map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0).collect();
    mixer.lock().unwrap().push(id, &samples);
}

#[tauri::command]
pub fn phone_volume(app: AppHandle, id: String, volume: u8) {
    set_gain(&app, &id, |g| g.volume = volume.min(100));
    changed(&app);
}

/// What one tick found.
struct Tick {
    levels: Vec<PhoneLevel>,
    expired: bool,
    idle: bool,
    moved: bool,
    stopped: bool,
}

impl Session {
    /// One tick: each phone hears its level, rows gone for two minutes leave, and every 5 s the Mac's addresses are checked.
    fn tick(&mut self, n: u64, now: Instant) -> Tick {
        self.mixer.lock().unwrap().levels(&mut self.levels);
        for g in &self.room.guests {
            let v = self.levels.iter().find(|l| *l.id == *g.id).map_or(0.0, |l| l.peak);
            g.send(Out::Text(encode(&ToPhone::Level { v })));
        }
        let gone = self.room.expire(now);
        for g in &gone {
            let voice = self.mixer.lock().unwrap().remove(&g.id);
            drop(voice);
        }
        Tick {
            levels: self.levels.iter().map(|l| PhoneLevel { id: l.id.to_string(), level: l.peak, down: l.down }).collect(),
            expired: !gone.is_empty(),
            idle: !gone.is_empty() && !self.window_open && self.room.guests.is_empty(),
            moved: n.is_multiple_of(60) && cert::lan_ips() != self.ips,
            stopped: self.output.broken(),
        }
    }
}

/// Every 80 ms while this session lasts: levels, gone phones, a new address now and then; the session ends when the Mac wakes
/// from sleep, its output device goes away, or its last phone is gone with the window closed.
async fn tick(app: AppHandle, id: u64) {
    let mut every = tokio::time::interval(Duration::from_millis(80));
    let mut last = (SystemTime::now(), Instant::now());
    for n in 1u64.. {
        every.tick().await;
        let now = (SystemTime::now(), Instant::now());
        let slept = woke(now.0.duration_since(last.0).unwrap_or_default(), now.1 - last.1);
        last = now;
        let Some(t) = with(&app, |s| (s.id == id).then(|| s.tick(n, now.1))).flatten() else { return };
        if t.stopped {
            let _ = app.emit("phone-news", News::Stopped);
        }
        if slept || t.idle || t.stopped {
            return end(&app);
        }
        let _ = app.emit("phone-levels", &t.levels);
        if t.expired {
            changed(&app);
        }
        if t.moved {
            readdress(&app, id).await;
        }
    }
}

/// Whether the wall clock ran far ahead of the uptime clock between two ticks, as it does across sleep.
fn woke(wall: Duration, uptime: Duration) -> bool {
    wall > uptime + Duration::from_secs(10)
}

/// Makes a certificate for the Mac's new addresses, loads it into the running server and points the QR code at the new address.
async fn readdress(app: &AppHandle, id: u64) {
    let ips = cert::lan_ips();
    let Some(&ip) = ips.first() else { return };
    let Some((dir, host, port, redirect, code, tls)) = with(app, |s| (s.id == id).then(|| (s.dir.clone(), s.host.clone(), s.port, s.redirect, s.room.code.clone(), s.tls.clone()))).flatten() else { return };
    let Ok((cert, key)) = cert::ensure(&dir, &host, &ips, kara_core::now_ms() / 1000) else { return };
    if tls.reload_from_pem(cert, key).await.is_err() {
        return;
    }
    let Ok(join) = join_info(&host, ip, port, redirect, &code) else { return };
    with(app, |s| {
        if s.id == id {
            s.ips = ips;
            s.join = join;
        }
    });
    changed(app);
}
```

`app/src-tauri/src/lib.rs`: add `phones::phone_volume,` to `generate_handler!`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `source "$HOME/.cargo/env" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all PASS (room 3); no warnings.

- [ ] **Step 6: Extend the phone check with sound and an effect**

In `app/scripts/phone-check.ts`, insert before `console.log("phone check OK");` (after Task 7's lines):

```ts
second.ws.send(JSON.stringify({ t: "voice", v: 1 }));
second.ws.send(JSON.stringify({ t: "live", on: true, rate: 48000 }));
second.ws.send(JSON.stringify({ t: "effect", kind: "autoTune", amount: 100 }));
const frame = new Int16Array(240);
for (let i = 0; i < 400; i++) {
  for (let j = 0; j < frame.length; j++) frame[j] = Math.round(800 * Math.sin((2 * Math.PI * 440 * (i * frame.length + j)) / 48_000));
  second.ws.send(frame.slice().buffer);
  await wait(5);
}
await until(() => second.got.some((m) => m.t === "level" && Number(m.v) > 0), "its voice level");
```

(Voice 1 % and a -32 dBFS tone keep it below hearing, about -70 dBFS, since it plays through the Mac's speakers while the user is working.)

Run the isolated check with the phone check for `task-08.png` and grep `phone check OK` in `task-08.while-open.txt`.
Expected: both succeed; `task-08.log` shows no panics.

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/Cargo.toml Cargo.lock app/src-tauri/src/phones app/src-tauri/src/lib.rs app/scripts/phone-check.ts
git commit -m "feat(app): phones sing through the Mac's speakers with effects, levels, gone phones, new addresses and sleep handled"
```

---

### Task 9: The Mac side — mic pill, the "Sing into your phone" window, toasts, and guests' songs starting

**Files:**
- Modify: `app/src/lib/api.ts`, `app/src/lib/format.ts`, `app/src/styles/tokens.css`
- Modify: `app/src/lib/state/ui.svelte.ts`, `app/src/lib/state/player.svelte.ts`, `app/src/lib/audio/streamer.ts`
- Create: `app/src/lib/state/phones.svelte.ts`
- Create: `app/src/lib/components/MicPill.svelte`, `app/src/lib/components/MicsSheet.svelte`
- Modify: `app/src/lib/components/Sheet.svelte`, `app/src/lib/components/Karaoke.svelte`, `app/src/routes/+page.svelte`
- Modify: `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`
- Modify: `app/tests/fake-backend.ts`
- Test: `app/tests/mics.spec.ts`

**Interfaces:**
- Consumes: commands `phones_open`, `phones_close`, `phone_remove` (Task 6), `phone_volume` (Task 8), `phones_clock` (Task 7); events `"phones"` (Tasks 6/8), `"phone-levels"` (Task 8), `"phone-news"` (Tasks 6/7) and `"library"` (Task 7); problem codes `phonesStart`, `noNetwork` (Task 6); `fake.guestAdds` (Task 5).
- Produces:
  - `api.ts`: `JoinInfo { qr: string; code: string; host: string }`, `PhoneRow { id; name; connected; volume }`, `PhonesView { join: JoinInfo | null; phones: PhoneRow[] }`, `PhoneLevel { id; level; down }`, `PhoneNews = { kind: "joined"; name; mic } | { kind: "added"; name; title } | { kind: "stopped" }`, `phonesOpen()`, `phonesClose()`, `phoneVolume(id, volume)`, `phoneRemove(id)`, `phonesClock(key, positionMs, playing)`, `onPhones(cb)`, `onPhoneLevels(cb)`, `onPhoneNews(cb)`, `onLibraryChanged(cb)`; `ProblemCode` gains `"phonesStart" | "noNetwork"`.
  - `format.ts`: `loudness(peak: number): number` — 0..1 on a -50..0 dB scale (the Mac's 5-bar meter shows `Math.round(loudness(level) * 5)` bars; the phone's mic pulse uses it in Task 10).
  - `phones` store: `view: PhonesView`, `levels: Record<string, PhoneLevel>`, `init()`, `open()`, `close()`, `setVolume(id, volume)`, `remove(id, name)`. Toasts from the prototype: "Aiko joined as Mic 1" (microphone), "Aiko added “title”" (list-plus), and "Aiko removed" (user-minus) when the Mac removes a phone; "The speakers changed. Open phone mics and have phones join again." (speaker-slash) when the output went away. When the session ends while the window is open (the Mac woke, or the speakers changed), the window starts a new one instead of going blank.
  - `ui.sheet` kind `"mics"`; `Sheet` props `subtitle?: string`, `subtitleIcon?: Icon`, `wide?: boolean`.
  - Tokens `--qr-dark`, `--qr-light`. i18n keys `mics.*`, `common.done`, `problem.phonesStart`, `problem.noNetwork`.
  - `Streamer.clock(): number` — the song time phones should follow: like `position()`, but already moving during the short wait before sound starts (so phone lyrics don't run ahead). The player store reports `phonesClock(current entry key, clock ms, playing)` whenever the streamer's state changes.
  - A song a guest queues while the Mac is idle starts playing in the karaoke view, as in the prototype (an entry with `by` becoming current when nothing was playing).
  - Fake backend levers `window.fake.phones(view)`, `window.fake.levels(levels)`, `window.fake.news(news)`, `window.fake.sessionEnded()`.

- [ ] **Step 1: Write the failing tests**

In `app/tests/fake-backend.ts`: import `PhonesView` with the other types; add

```ts
let phonesView: PhonesView = { join: { qr: '<svg viewBox="0 0 1 1"><rect width="1" height="1"/></svg>', code: "OKI-4827", host: "Test-Mac.local" }, phones: [] };
```

to the state above `fake` (and `PhoneLevel`, `PhoneNews` to the type import); to `fake`:

```ts
  /** The phone session changes, as the app reports it. */
  phones(view: PhonesView) {
    phonesView = view;
    return emit("phones", view);
  },
  /** The phones' levels, as the app sends them every 80 ms. */
  levels(levels: PhoneLevel[]) {
    return emit("phone-levels", levels);
  },
  /** Something a phone did that the Mac toasts. */
  news(news: PhoneNews) {
    return emit("phone-news", news);
  },
  /** The session ends on its own (the Mac woke, the speakers changed); opening again gives the same view. */
  sessionEnded() {
    return emit("phones", { join: null, phones: [] });
  },
```

and to `commands`:

```ts
  phones_open: () => phonesView,
  phones_close: () => null,
  phone_volume: () => null,
  phone_remove: () => null,
  phones_clock: () => null,
```

Create `app/tests/mics.spec.ts`:

```ts
import { test, expect, calls, sing } from "./app";

test("the mic pill opens the phone window with the code, the address and how to get past the warning", async ({ page }) => {
  await page.getByRole("button", { name: "Phone mics" }).click();
  const sheet = page.getByRole("dialog", { name: "Sing into your phone" });
  await expect(sheet).toContainText("Scan with a phone on the same Wi-Fi.");
  await expect(sheet).toContainText("OKI-4827");
  await expect(sheet).toContainText("on Test-Mac.local");
  await expect(sheet).toContainText("iPhone: tap Show Details, then visit this website.");
  await expect(sheet).toContainText("Keep phones away from the speakers.");
  await expect(sheet.locator(".qr svg")).toBeVisible();
  await expect(sheet).toContainText("Waiting for phones to join…");
  await page.evaluate(() => window.fake.sessionEnded());
  await expect.poll(async () => (await calls(page, "phones_open")).length).toBe(2);
  await expect(sheet).toContainText("OKI-4827");
  await page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();
  expect(await calls(page, "phones_close")).toHaveLength(1);
});

test("joined phones show their name, level, volume and Remove, and the pill counts them", async ({ page }) => {
  await page.getByRole("button", { name: "Phone mics" }).click();
  const sheet = page.getByRole("dialog", { name: "Sing into your phone" });
  await page.evaluate(() =>
    window.fake.phones({
      join: { qr: "<svg></svg>", code: "OKI-4827", host: "Test-Mac.local" },
      phones: [
        { id: "a", name: "Aiko", connected: true, volume: 80 },
        { id: "b", name: "Ben", connected: false, volume: 80 },
        { id: "c", name: "Chie", connected: true, volume: 80 },
      ],
    }),
  );
  await page.evaluate(() => window.fake.levels([{ id: "a", level: 0.5, down: false }, { id: "c", level: 0.9, down: true }]));
  await expect(sheet).toContainText("Phones · 3");
  await expect(sheet.locator(".mic", { hasText: "Aiko" }).locator(".meter .lit")).toHaveCount(4);
  await expect(sheet.locator(".mic", { hasText: "Ben" })).toContainText("Reconnecting…");
  await expect(sheet.locator(".mic", { hasText: "Chie" })).toContainText("Turned down: too close to the speakers");
  await sheet.getByRole("slider", { name: "Volume for Aiko" }).fill("40");
  expect(await calls(page, "phone_volume")).toContainEqual({ id: "a", volume: 40 });
  await sheet.getByRole("button", { name: "Remove Aiko" }).click();
  expect(await calls(page, "phone_remove")).toEqual([{ id: "a" }]);
  await expect(page.getByText("Aiko removed")).toBeVisible();
  await sheet.getByRole("button", { name: "Done" }).click();
  await expect(page.getByRole("button", { name: "Phone mics" })).toContainText("3");
});

test("the app tells phones which song is playing and where", async ({ page }) => {
  await sing(page, "Paper Boats");
  await expect.poll(async () => (await calls(page, "phones_clock")).some((c) => c.key === 1 && c.playing === true)).toBe(true);
});

test("a guest's song starts when nothing is playing, and the Mac says who joined and who added what", async ({ page }) => {
  await page.evaluate(() => window.fake.guestAdds(3, "Aiko"));
  await expect(page.getByRole("region", { name: "Karaoke" })).toBeVisible();
  await page.waitForFunction(() => window.fake.started);
  await page.evaluate(() => window.fake.news({ kind: "joined", name: "Ben", mic: 2 }));
  await expect(page.getByText("Ben joined as Mic 2")).toBeVisible();
  await page.evaluate(() => window.fake.news({ kind: "added", name: "Ben", title: "Paper Boats" }));
  await expect(page.getByText("Ben added “Paper Boats”")).toBeVisible();
  await page.evaluate(() => window.fake.news({ kind: "stopped" }));
  await expect(page.getByText("The speakers changed. Open phone mics and have phones join again.")).toBeVisible();
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd app && npx playwright test tests/mics.spec.ts`
Expected: FAIL — no "Phone mics" button.

- [ ] **Step 3: API, format, tokens, stores**

`app/src/lib/api.ts`: add `| "phonesStart" | "noNetwork"` to `ProblemCode`, and after the player section:

```ts
export interface JoinInfo { qr: string; code: string; host: string }
export interface PhoneRow { id: string; name: string; connected: boolean; volume: number }
export interface PhonesView { join: JoinInfo | null; phones: PhoneRow[] }
export interface PhoneLevel { id: string; level: number; down: boolean }
export type PhoneNews = { kind: "joined"; name: string; mic: number } | { kind: "added"; name: string; title: string } | { kind: "stopped" };

export const phonesOpen = () => invoke<PhonesView>("phones_open");
export const phonesClose = () => invoke<void>("phones_close");
export const phoneVolume = (id: string, volume: number) => invoke<void>("phone_volume", { id, volume });
export const phoneRemove = (id: string) => invoke<void>("phone_remove", { id });
export const phonesClock = (key: number | null, positionMs: number, playing: boolean) => invoke<void>("phones_clock", { key, positionMs, playing });
export const onPhones = (cb: (v: PhonesView) => void) => listen<PhonesView>("phones", (e) => cb(e.payload));
export const onPhoneLevels = (cb: (levels: PhoneLevel[]) => void) => listen<PhoneLevel[]>("phone-levels", (e) => cb(e.payload));
export const onPhoneNews = (cb: (news: PhoneNews) => void) => listen<PhoneNews>("phone-news", (e) => cb(e.payload));
export const onLibraryChanged = (cb: () => void) => listen("library", () => cb());
```

`app/src/lib/format.ts`:

```ts
/** A sound's peak (0 to 1) placed on a -50 dB to 0 dB scale, 0 to 1, for meters. */
export const loudness = (peak: number) => Math.min(1, Math.max(0, (20 * Math.log10(Math.max(peak, 1e-6)) + 50) / 50));
```

`app/src/styles/tokens.css`: in the first `:root` block, after `--thumb: #FFFFFF; --bezel: #0B0F14;` add `--qr-dark: #10151B; --qr-light: #FFFFFF;` (the QR code stays dark on light in both modes).

`app/src/lib/state/ui.svelte.ts`: `export type SheetState = { kind: "edit"; track: Track } | { kind: "settings" } | { kind: "mics" };`

Create `app/src/lib/state/phones.svelte.ts`:

```ts
import { onPhoneLevels, onPhoneNews, onPhones, phoneRemove, phonesClose, phonesOpen, phoneVolume, type PhoneLevel, type PhoneNews, type PhonesView } from "$lib/api";
import { say } from "$lib/i18n/engine";
import { t } from "$lib/i18n/index.svelte";
import { toasts } from "./toasts.svelte";
import { ui } from "./ui.svelte";
import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
import SpeakerSlashIcon from "phosphor-svelte/lib/SpeakerSlashIcon";
import UserMinusIcon from "phosphor-svelte/lib/UserMinusIcon";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

/** The phone session as the Mac window shows it, and the toasts about what phones do. */
class PhonesState {
  view = $state<PhonesView>({ join: null, phones: [] });
  levels = $state<Record<string, PhoneLevel>>({});
  private shown = false;

  async init() {
    await onPhones((v) => {
      this.view = v;
      if (!v.join && this.shown) void this.open();
    });
    await onPhoneLevels((levels) => (this.levels = Object.fromEntries(levels.map((l) => [l.id, l]))));
    await onPhoneNews((n) => this.toast(n));
  }

  /** The window opened: starts the session if needed; if it can't, the window closes with the reason. */
  async open() {
    this.shown = true;
    try {
      const view = await phonesOpen();
      if (this.shown) this.view = view;
      else await phonesClose();
    } catch (e) {
      if (ui.sheet?.kind === "mics") ui.sheet = null;
      toasts.show(say(e), { icon: WarningIcon });
    }
  }

  async close() {
    this.shown = false;
    await phonesClose().catch(() => {});
  }

  setVolume(id: string, volume: number) {
    void phoneVolume(id, volume).catch(() => {});
  }

  remove(id: string, name: string) {
    void phoneRemove(id).catch(() => {});
    toasts.show(t("mics.removed", { name }), { icon: UserMinusIcon });
  }

  private toast(n: PhoneNews) {
    if (n.kind === "joined") toasts.show(t("mics.joined", { name: n.name, n: n.mic }), { icon: MicrophoneStageIcon });
    else if (n.kind === "added") toasts.show(t("mics.added", { name: n.name, title: n.title }), { icon: ListPlusIcon });
    else toasts.show(t("mics.stopped"), { icon: SpeakerSlashIcon });
  }
}

export const phones = new PhonesState();
```

`app/src/lib/audio/streamer.ts`, next to `position()`:

```ts
  /** The song time phones should follow: like `position()`, but already moving during the short wait before sound starts. */
  clock(): number {
    return this.phase === "playing" ? this.ctx.currentTime - this.anchor - this.key.latency : this.pos;
  }
```

`app/src/lib/state/player.svelte.ts`:
- import `phonesClock` from `$lib/api`, and at the end of `sync()`:

```ts
    void phonesClock(this.current?.key ?? null, Math.round(this.streamer.clock() * 1000), this.phase === "playing").catch(() => {});
```

- in `apply(s)`, remember whether the Mac was idle before taking the snapshot (`const wasIdle = this.idle;` as the first line) and, right before `void this.streamer.load(…)`, let a guest's song start:

```ts
    if (wasIdle && cur.by) {
      this.wantPlay = true;
      ui.karaoke = true;
    }
```

- [ ] **Step 4: Components**

`app/src/lib/components/Sheet.svelte` — optional subtitle and a wide variant:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Icon } from "$lib/icons";
  import { t } from "$lib/i18n/index.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { fade, slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import XIcon from "phosphor-svelte/lib/XIcon";

  let { icon: IconC, title, subtitle, subtitleIcon: SubIcon, wide = false, onClose, children }: {
    icon: Icon;
    title: string;
    subtitle?: string;
    subtitleIcon?: Icon;
    wide?: boolean;
    onClose: () => void;
    children: Snippet;
  } = $props();
</script>

<div class="scrim" class:dk={ui.karaoke} role="presentation" transition:fade onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="sheet" class:wide role="dialog" aria-label={title} transition:slide|global={{ y: 8 }}>
    <div class="shead">
      <div class="badge"><IconC size={20} /></div>
      <div class="grow"><h2>{title}</h2>{#if subtitle}<p class="muted hstack">{#if SubIcon}<SubIcon size={14} />{/if}{subtitle}</p>{/if}</div>
      <button class="ib" use:tip={t("common.close")} onclick={onClose}><XIcon size={18} /></button>
    </div>
    {@render children()}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 40; display: grid; place-items: center; padding: var(--s4); background: var(--scrim); -webkit-backdrop-filter: blur(3px); backdrop-filter: blur(3px); }
  .sheet { width: min(460px, 100%); max-height: calc(100vh - 32px); overflow: auto; padding: var(--s6); border-radius: var(--r-lg); background: var(--surface); border: 1px solid var(--line); box-shadow: 0 30px 80px -30px var(--shadow); }
  .sheet.wide { width: min(720px, 100%); }
  .shead { display: flex; align-items: center; gap: var(--s3); margin-bottom: var(--s5); }
  .shead p { margin-top: 2px; }
  h2 { font: 800 22px/1.2 var(--display); letter-spacing: -.02em; }
  .badge { width: 40px; height: 40px; border-radius: var(--r-sm); display: grid; place-items: center; background: var(--raised); flex: none; }
</style>
```

Create `app/src/lib/components/MicPill.svelte`:

```svelte
<script lang="ts">
  import { phones } from "$lib/state/phones.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { tip } from "$lib/tooltip.svelte";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import PlusIcon from "phosphor-svelte/lib/PlusIcon";

  const n = $derived(phones.view.phones.length);
</script>

<button class="pill glass" use:tip={t("mics.pill")} onclick={() => (ui.sheet = { kind: "mics" })}>
  <MicrophoneStageIcon size={22} />
  {#if n}<span>{n}</span>{:else}<PlusIcon size={14} />{/if}
</button>

<style>
  .pill { grid-column: 3; justify-self: end; display: flex; align-items: center; gap: var(--s2); height: 50px; padding: 0 var(--s5); border-radius: 999px; font: 500 15px var(--mono); }
  .pill:hover { background: var(--glass-hi); }
</style>
```

Create `app/src/lib/components/MicsSheet.svelte` (markup and CSS from the prototype's `#mics` and `renderMics`):

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { phones } from "$lib/state/phones.svelte";
  import { ui } from "$lib/state/ui.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { loudness } from "$lib/format";
  import { slide } from "$lib/motion";
  import { tip } from "$lib/tooltip.svelte";
  import Sheet from "./Sheet.svelte";
  import AndroidLogoIcon from "phosphor-svelte/lib/AndroidLogoIcon";
  import AppleLogoIcon from "phosphor-svelte/lib/AppleLogoIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import DeviceMobileIcon from "phosphor-svelte/lib/DeviceMobileIcon";
  import GlobeIcon from "phosphor-svelte/lib/GlobeIcon";
  import KeyboardIcon from "phosphor-svelte/lib/KeyboardIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import ShieldCheckIcon from "phosphor-svelte/lib/ShieldCheckIcon";
  import ShieldWarningIcon from "phosphor-svelte/lib/ShieldWarningIcon";
  import SpeakerHighIcon from "phosphor-svelte/lib/SpeakerHighIcon";
  import SpeakerSlashIcon from "phosphor-svelte/lib/SpeakerSlashIcon";
  import UserMinusIcon from "phosphor-svelte/lib/UserMinusIcon";
  import WifiHighIcon from "phosphor-svelte/lib/WifiHighIcon";

  const BARS = [5, 8, 11, 14, 16];
  const join = $derived(phones.view.join);
  const close = () => (ui.sheet = null);
  let drafts = $state<Record<string, number>>({});

  onMount(() => {
    void phones.open();
    return () => void phones.close();
  });
</script>

<Sheet icon={MicrophoneStageIcon} title={t("mics.title")} subtitle={t("mics.scan")} subtitleIcon={WifiHighIcon} wide onClose={close}>
  <div class="join">
    <div>
      <div class="qr">{#if join}{@html join.qr}{/if}</div>
      <p class="cap hstack typed"><KeyboardIcon size={14} />{t("mics.typeCode")}</p>
      <div class="code">{join?.code ?? ""}</div>
      <p class="curl hstack"><GlobeIcon size={14} />{t("mics.on", { host: join?.host ?? "" })}</p>
    </div>
    <div>
      <div class="tut">
        <b class="hstack"><ShieldWarningIcon size={16} />{t("mics.warnTitle")}</b>
        <p class="muted">{t("mics.warnSafe")}</p>
        <p class="hstack"><AppleLogoIcon size={16} />{t("mics.iphone")}</p>
        <p class="hstack"><AndroidLogoIcon size={16} />{t("mics.android")}</p>
        <p class="hstack"><ShieldCheckIcon size={16} />{t("mics.firewall")}</p>
        <p class="hstack"><SpeakerSlashIcon size={16} />{t("mics.apart")}</p>
      </div>
      <div class="mlist">
        <p class="cap hstack"><DeviceMobileIcon size={14} />{t("mics.phones", { n: phones.view.phones.length })}</p>
        {#each phones.view.phones as p, i (p.id)}
          {@const level = phones.levels[p.id]}
          {@const volume = drafts[p.id] ?? p.volume}
          <div class="mic" class:away={!p.connected} transition:slide={{ y: 8 }}>
            <DeviceMobileIcon size={18} />
            <div class="grow">
              <b class="ell">{p.name}</b>
              <small class="cap" class:hot={level?.down}>{!p.connected ? t("mics.away") : level?.down ? t("mics.turnedDown") : t("mics.micN", { n: i + 1 })}</small>
            </div>
            <span class="meter" aria-hidden="true">
              {#each BARS as h, b (b)}<i style:height="{h}px" class:lit={b < Math.round(loudness(level?.level ?? 0) * 5)}></i>{/each}
            </span>
            <label class="vol" use:tip={t("mics.volume")}>
              <SpeakerHighIcon size={16} />
              <input
                class="vs"
                type="range"
                min="0"
                max="100"
                value={volume}
                style:--v="{volume}%"
                aria-label={t("mics.volumeFor", { name: p.name })}
                oninput={(e) => {
                  drafts[p.id] = +e.currentTarget.value;
                  phones.setVolume(p.id, drafts[p.id]);
                }}
                onchange={() => delete drafts[p.id]}
              />
            </label>
            <button class="ib" use:tip={t("mics.remove", { name: p.name })} onclick={() => phones.remove(p.id, p.name)}><UserMinusIcon size={18} /></button>
          </div>
        {/each}
        <div class="mic wait"><WifiHighIcon size={18} />{phones.view.phones.length ? t("mics.waitingMore") : t("mics.waiting")}</div>
      </div>
    </div>
  </div>
  <div class="sfoot"><button class="btn accent" onclick={close}><CheckIcon />{t("common.done")}</button></div>
</Sheet>

<style>
  .join { display: grid; grid-template-columns: 200px minmax(0, 1fr); gap: var(--s6); }
  .qr { width: 200px; height: 200px; padding: var(--s3); border-radius: var(--r-lg); color: var(--qr-dark); background: var(--qr-light); border: 1px solid var(--line); }
  .qr :global(svg) { display: block; width: 100%; height: 100%; }
  .typed { margin-top: var(--s4); }
  .code { font: 500 26px/1 var(--mono); letter-spacing: .08em; margin-top: var(--s2); }
  .curl { margin-top: var(--s2); color: var(--muted); font-size: 13px; }
  .tut { display: grid; gap: var(--s2); padding: var(--s3) var(--s4); border-radius: var(--r-sm); background: var(--raised); font-size: 13px; }
  .tut .hstack { align-items: flex-start; }
  .tut :global(svg) { flex: none; margin-top: 2px; }
  .mlist { margin-top: var(--s4); }
  .mlist .cap { margin-bottom: var(--s2); }
  .mic { display: flex; align-items: center; gap: var(--s3); min-height: var(--row); border-top: 1px solid var(--line); transition: opacity var(--t) var(--ease); }
  .mic > :global(svg) { color: var(--muted); }
  .mic small { display: block; }
  .mic.away { opacity: .5; }
  .hot { color: var(--busy); }
  .mic.wait { color: var(--muted); font-size: 13px; }
  .mic.wait > :global(svg) { animation: pulse 1.6s var(--ease) infinite; }
  .meter { display: flex; align-items: flex-end; gap: 2px; height: 16px; }
  .meter i { width: 3px; border-radius: 1px; background: var(--line); transition: background-color 120ms linear; }
  .meter i.lit { background: var(--ready); }
  .vol { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  .vol .vs { width: 80px; }
  .sfoot { display: flex; justify-content: flex-end; margin-top: var(--s5); }
</style>
```

`app/src/lib/components/Karaoke.svelte`: import `MicPill` and replace the last `<div></div>` inside `.ktop` with `<MicPill />`.

`app/src/routes/+page.svelte`:
- imports: `MicPill`, `MicsSheet`, `{ phones } from "$lib/state/phones.svelte"`, and `onLibraryChanged` next to `reduceTransparency` from `$lib/api`;
- header: `<header class="top"><SearchBar bind:this={searchBar} onSubmitLink={(url, title) => adding.link(url, "", title)} /><MicPill /></header>`;
- after `<SettingsSheet />`: `{#if ui.sheet?.kind === "mics"}<MicsSheet />{/if}`;
- in `onMount`, after `adding.init();`: `void phones.init();` and `void onLibraryChanged(() => void library.refresh());`.

- [ ] **Step 5: Text in six languages**

Add after `"common.close"` (for `common.done`) and at the end of each locale (the rest). `en.ts`:

```ts
  "common.done": "Done",
  "problem.phonesStart": "Couldn't start phone mics.",
  "problem.noNetwork": "Connect this computer to Wi-Fi to use phone mics.",
  "mics.pill": "Phone mics",
  "mics.title": "Sing into your phone",
  "mics.scan": "Scan with a phone on the same Wi-Fi.",
  "mics.typeCode": "Or type this code",
  "mics.on": "on {host}",
  "mics.warnTitle": "The phone shows a warning the first time",
  "mics.warnSafe": "It's safe: the page comes from this computer.",
  "mics.iphone": "iPhone: tap Show Details, then visit this website.",
  "mics.android": "Android: tap Advanced, then Proceed.",
  "mics.firewall": "The first time, this computer may ask to allow incoming connections. Allow them.",
  "mics.apart": "Keep phones away from the speakers.",
  "mics.phones": "Phones · {n}",
  "mics.micN": "Mic {n}",
  "mics.away": "Reconnecting…",
  "mics.turnedDown": "Turned down: too close to the speakers",
  "mics.volume": "Volume",
  "mics.volumeFor": "Volume for {name}",
  "mics.remove": "Remove {name}",
  "mics.waiting": "Waiting for phones to join…",
  "mics.waitingMore": "Waiting for more phones…",
  "mics.joined": "{name} joined as Mic {n}",
  "mics.added": "{name} added “{title}”",
  "mics.removed": "{name} removed",
  "mics.stopped": "The speakers changed. Open phone mics and have phones join again.",
```

`ja.ts`:

```ts
  "common.done": "完了",
  "problem.phonesStart": "スマホマイクを開始できませんでした。",
  "problem.noNetwork": "スマホマイクを使うには、このコンピュータをWi-Fiに接続してください。",
  "mics.pill": "スマホマイク",
  "mics.title": "スマホに向かって歌おう",
  "mics.scan": "同じWi-Fiにつないだスマホでスキャンしてください。",
  "mics.typeCode": "またはこのコードを入力",
  "mics.on": "{host} で",
  "mics.warnTitle": "初回はスマホに警告が表示されます",
  "mics.warnSafe": "安全です。ページはこのコンピュータから送られています。",
  "mics.iphone": "iPhone：「詳細を表示」をタップし、「このWebサイトを閲覧」を選びます。",
  "mics.android": "Android：「詳細設定」をタップし、「アクセスする」を選びます。",
  "mics.firewall": "初回は、このコンピュータが受信接続の許可を求めることがあります。許可してください。",
  "mics.apart": "スマホはスピーカーから離してください。",
  "mics.phones": "スマホ · {n}",
  "mics.micN": "マイク {n}",
  "mics.away": "再接続中…",
  "mics.turnedDown": "音量を下げました：スピーカーに近すぎます",
  "mics.volume": "音量",
  "mics.volumeFor": "{name} の音量",
  "mics.remove": "{name} を外す",
  "mics.waiting": "スマホの参加を待っています…",
  "mics.waitingMore": "ほかのスマホを待っています…",
  "mics.joined": "{name} さんがマイク {n} で参加しました",
  "mics.added": "{name} さんが「{title}」を追加しました",
  "mics.removed": "{name} さんを外しました",
  "mics.stopped": "スピーカーが変わりました。スマホマイクを開いて、もう一度参加してもらってください。",
```

`ko.ts`:

```ts
  "common.done": "완료",
  "problem.phonesStart": "휴대폰 마이크를 시작할 수 없습니다.",
  "problem.noNetwork": "휴대폰 마이크를 쓰려면 이 컴퓨터를 Wi-Fi에 연결하세요.",
  "mics.pill": "휴대폰 마이크",
  "mics.title": "휴대폰에 대고 노래하세요",
  "mics.scan": "같은 Wi-Fi에 연결된 휴대폰으로 스캔하세요.",
  "mics.typeCode": "또는 이 코드를 입력하세요",
  "mics.on": "{host}에서",
  "mics.warnTitle": "처음에는 휴대폰에 경고가 표시됩니다",
  "mics.warnSafe": "안전합니다. 페이지는 이 컴퓨터에서 옵니다.",
  "mics.iphone": "iPhone: 세부사항 보기를 탭한 다음 이 웹 사이트 방문을 누르세요.",
  "mics.android": "Android: 고급을 탭한 다음 계속을 누르세요.",
  "mics.firewall": "처음에는 이 컴퓨터가 들어오는 연결을 허용할지 물을 수 있습니다. 허용하세요.",
  "mics.apart": "휴대폰을 스피커에서 멀리 두세요.",
  "mics.phones": "휴대폰 · {n}",
  "mics.micN": "마이크 {n}",
  "mics.away": "다시 연결하는 중…",
  "mics.turnedDown": "소리를 줄였습니다: 스피커에 너무 가깝습니다",
  "mics.volume": "볼륨",
  "mics.volumeFor": "{name} 볼륨",
  "mics.remove": "{name} 내보내기",
  "mics.waiting": "휴대폰 연결을 기다리는 중…",
  "mics.waitingMore": "다른 휴대폰을 기다리는 중…",
  "mics.joined": "{name} 님이 마이크 {n}(으)로 참여했습니다",
  "mics.added": "{name} 님이 “{title}”을(를) 추가했습니다",
  "mics.removed": "{name} 님을 내보냈습니다",
  "mics.stopped": "스피커가 바뀌었습니다. 휴대폰 마이크를 열고 휴대폰을 다시 참여시키세요.",
```

`zh-Hans.ts`:

```ts
  "common.done": "完成",
  "problem.phonesStart": "无法启动手机麦克风。",
  "problem.noNetwork": "请将这台电脑连接到 Wi-Fi 以使用手机麦克风。",
  "mics.pill": "手机麦克风",
  "mics.title": "用手机唱歌",
  "mics.scan": "用连接同一 Wi-Fi 的手机扫描。",
  "mics.typeCode": "或输入此代码",
  "mics.on": "网址：{host}",
  "mics.warnTitle": "第一次手机会显示警告",
  "mics.warnSafe": "这是安全的：页面来自这台电脑。",
  "mics.iphone": "iPhone：轻点“显示详细信息”，然后选择“访问此网站”。",
  "mics.android": "Android：轻点“高级”，然后选择“继续前往”。",
  "mics.firewall": "第一次，这台电脑可能会询问是否允许传入连接。请允许。",
  "mics.apart": "让手机远离扬声器。",
  "mics.phones": "手机 · {n}",
  "mics.micN": "麦克风 {n}",
  "mics.away": "正在重新连接…",
  "mics.turnedDown": "已调低：离扬声器太近",
  "mics.volume": "音量",
  "mics.volumeFor": "{name} 的音量",
  "mics.remove": "移除 {name}",
  "mics.waiting": "正在等待手机加入…",
  "mics.waitingMore": "正在等待更多手机…",
  "mics.joined": "{name} 以麦克风 {n} 加入",
  "mics.added": "{name} 添加了“{title}”",
  "mics.removed": "已移除 {name}",
  "mics.stopped": "扬声器变了。请打开手机麦克风，让手机重新加入。",
```

`zh-Hant.ts`:

```ts
  "common.done": "完成",
  "problem.phonesStart": "無法啟動手機麥克風。",
  "problem.noNetwork": "請將這台電腦連上 Wi-Fi 以使用手機麥克風。",
  "mics.pill": "手機麥克風",
  "mics.title": "用手機唱歌",
  "mics.scan": "用連接同一個 Wi-Fi 的手機掃描。",
  "mics.typeCode": "或輸入這組代碼",
  "mics.on": "網址：{host}",
  "mics.warnTitle": "第一次手機會顯示警告",
  "mics.warnSafe": "這是安全的：頁面來自這台電腦。",
  "mics.iphone": "iPhone：點一下「顯示詳細資訊」，然後選擇「造訪此網站」。",
  "mics.android": "Android：點一下「進階」，然後選擇「繼續前往」。",
  "mics.firewall": "第一次，這台電腦可能會詢問是否允許傳入連線。請允許。",
  "mics.apart": "讓手機遠離喇叭。",
  "mics.phones": "手機 · {n}",
  "mics.micN": "麥克風 {n}",
  "mics.away": "正在重新連線…",
  "mics.turnedDown": "已調低：離喇叭太近",
  "mics.volume": "音量",
  "mics.volumeFor": "{name} 的音量",
  "mics.remove": "移除 {name}",
  "mics.waiting": "正在等待手機加入…",
  "mics.waitingMore": "正在等待更多手機…",
  "mics.joined": "{name} 以麥克風 {n} 加入",
  "mics.added": "{name} 新增了「{title}」",
  "mics.removed": "已移除 {name}",
  "mics.stopped": "喇叭變了。請打開手機麥克風，讓手機重新加入。",
```

`es.ts`:

```ts
  "common.done": "Listo",
  "problem.phonesStart": "No se pudieron activar los micrófonos del móvil.",
  "problem.noNetwork": "Conecta este ordenador a una wifi para usar los micrófonos del móvil.",
  "mics.pill": "Micrófonos del móvil",
  "mics.title": "Canta con tu móvil",
  "mics.scan": "Escanéalo con un móvil conectado a la misma wifi.",
  "mics.typeCode": "O escribe este código",
  "mics.on": "en {host}",
  "mics.warnTitle": "La primera vez, el móvil muestra un aviso",
  "mics.warnSafe": "Es seguro: la página viene de este ordenador.",
  "mics.iphone": "iPhone: toca Mostrar detalles y luego visitar este sitio web.",
  "mics.android": "Android: toca Configuración avanzada y luego Acceder.",
  "mics.firewall": "La primera vez, este ordenador puede pedir permiso para conexiones entrantes. Permítelas.",
  "mics.apart": "Mantén los móviles lejos de los altavoces.",
  "mics.phones": "Móviles · {n}",
  "mics.micN": "Micro {n}",
  "mics.away": "Reconectando…",
  "mics.turnedDown": "Bajado: demasiado cerca de los altavoces",
  "mics.volume": "Volumen",
  "mics.volumeFor": "Volumen de {name}",
  "mics.remove": "Quitar a {name}",
  "mics.waiting": "Esperando a que se unan móviles…",
  "mics.waitingMore": "Esperando más móviles…",
  "mics.joined": "{name} se unió como micro {n}",
  "mics.added": "{name} añadió “{title}”",
  "mics.removed": "Se quitó a {name}",
  "mics.stopped": "Cambiaron los altavoces. Abre los micrófonos del móvil y pide a los móviles que se unan de nuevo.",
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd app && npm test && npm run check && npm run check:i18n && npx playwright test`
Expected: vitest PASS; 0 svelte-check errors; every locale matches; all Playwright tests PASS, including `mics.spec.ts` (4).

- [ ] **Step 7: Isolated check and commit**

Run the isolated check with the phone check for `task-09.png` (grep `phone check OK`). The picture shows the library with the pill (`+`) at the top right.

```bash
git add app/src app/tests/fake-backend.ts app/tests/mics.spec.ts
git commit -m "feat(app): mic pill and the Sing into your phone window with joined phones"
```

---
### Task 10: The phone page — join, mic permission, staying connected

**Files:**
- Create: `app/src/routes/phone/+page.svelte`
- Create: `app/src/lib/phone/link.svelte.ts`, `mic.ts`, `capture-worklet.ts`, `phone.css`, `Center.svelte`, `JoinScreen.svelte`, `MicTab.svelte`
- Modify: `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`
- Create: `app/tests/fake-phone.ts`, `app/tests/phone.ts`
- Test: `app/tests/phone.spec.ts`

**Interfaces:**
- Consumes: the wire contract (Tasks 6–8): `{t:"join", code, id, name}`, `{t:"live", on, rate}`, `{t:"ping"}`, `{t:"leave"}`; `joined`, `player`, `level`, `refused`; close codes 4001/4002/4003; the server drops a connection silent for 5 s. `loudness` (Task 9), `say`, `toasts`, `Toasts.svelte`, `$lib/motion`, `$lib/keys` `composing`.
- Produces:
  - Route `/phone` (the QR code opens `/phone?code=1234`).
  - `link` (in `$lib/phone/link.svelte.ts`): state `screen: "join" | "connecting" | "perm" | "mic" | "ended" | "blocked"`, `refusal: "wrongCode" | "full" | "unreachable" | null`, `reconnecting`, `code`, `fromQr`, `name`, `live`, `level`, `snapshot: PlayerSnapshot | null`; methods `start(): () => void`, `join()`, `allowMic()`, `toggleLive()`, `leave()`, `again()`; private `send(m: object)` and `receive(m)` that later tasks extend.
  - `openMic(onFrame: (pcm: ArrayBuffer) => void): Promise<Mic>`; `Mic { rate: number; running(): boolean; resume(): Promise<void>; stop(): void }`.
  - `Center.svelte` props `{ icon?: Icon; warn?: boolean; title: string; lead: string; children?: Snippet; action?: Snippet }` (no icon shows a spinner).
  - `MicTab.svelte` (the mic button and its state line; Task 11 adds the rest).
  - While joined, the page sends `{t:"ping"}` every second, holds a screen wake lock, and releases it on Leave or Session ended. The code shows with the Mac's `OKI-` prefix ("Joining OKI-4827"); only its digits are typed and sent.
  - Phone styles in `$lib/phone/phone.css` (global, loaded only by the phone route): `.phone .pscreen .pcenter .lead .bigicon .pnote .pbtn .logo .pfield .pinput .phead .chip .ptab .ptabs .pbanner`.
  - Test helpers in `app/tests/phone.ts`: `test`, `expect`, `sent(page)`, `frames(page)`, `server(page, msg)`, `joinAs(page, name?)`; the in-page fake `window.fakePhone` with `sent`, `frames`, `full`, `unreachable`, `micAllowed`, `wakeLocks`, `released`, `quiet`, `audio`, `snapshot`, `server(msg)`, `drop()`, `end()`, `frame()`.
  - i18n keys `phone.*` listed in Step 5.

- [ ] **Step 1: The fake computer, mic and wake lock**

Create `app/tests/fake-phone.ts`:

```ts
import type { PlayerSnapshot, Track } from "$lib/api";

declare global {
  interface Window {
    fakePhone: typeof fakePhone;
  }
}

const song = (id: number, title: string, artist: string | null): Track => ({
  id, provider: "local", title, artist, album: null, durationMs: 200_000, vocalRemoval: 100, keySemitones: 0, instrumental: false, artworkPath: null, artSeed: id * 40,
});

const songs = [song(1, "Paper Boats", "Juniper Row"), song(2, "Kettle Duet", "Juniper Row"), song(3, "Lemon Skies", "The Porchlights")];

let socket: FakeSocket | null = null;
let worklet: { port: { onmessage: ((e: MessageEvent) => void) | null } } | null = null;

/** The fake computer's side of the connection, and levers for tests through `window.fakePhone`. */
const fakePhone = {
  sent: [] as Record<string, unknown>[],
  frames: 0,
  full: false,
  unreachable: false,
  micAllowed: true,
  wakeLocks: 0,
  released: 0,
  /** True stops the computer's level messages, as a dead connection would. */
  quiet: false,
  audio: "running" as AudioContextState,
  snapshot: {
    entries: [{ key: 1, track: songs[0], by: null }, { key: 2, track: songs[2], by: "Ben" }, { key: 3, track: songs[1], by: null }],
    current: 0,
    ended: false,
    lyricOffsetMs: 0,
  } as PlayerSnapshot,
  /** A message from the computer. */
  server(msg: object) {
    socket?.deliver(msg);
  },
  /** The connection drops, as Wi-Fi does. */
  drop() {
    socket?.shut(1006);
  },
  /** The computer ends the session. */
  end() {
    socket?.shut(4001);
  },
  /** The mic makes one frame of sound. */
  frame() {
    worklet?.port.onmessage?.({ data: new Int16Array(256).buffer } as MessageEvent);
  },
};
window.fakePhone = fakePhone;

class FakeSocket {
  static readonly OPEN = 1;
  readyState = 0;
  binaryType = "blob";
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onclose: ((e: { code: number }) => void) | null = null;
  private beat: ReturnType<typeof setInterval> | undefined;

  constructor(readonly url: string) {
    socket = this;
    setTimeout(() => {
      if (fakePhone.unreachable) return this.shut(1006);
      this.readyState = 1;
      this.onopen?.();
      this.beat = setInterval(() => !fakePhone.quiet && this.deliver({ t: "level", v: 0.2 }), 80);
    });
  }

  send(data: string | ArrayBuffer) {
    if (typeof data !== "string") return void fakePhone.frames++;
    const m = JSON.parse(data);
    fakePhone.sent.push(m);
    this.reply(m);
  }

  /** The computer's answers. */
  reply(m: Record<string, unknown>) {
    if (m.t !== "join") return;
    if (m.code !== "4827") return this.shut(4003);
    if (fakePhone.full) return this.shut(4002);
    this.deliver({ t: "joined" });
    this.deliver({ t: "player", snapshot: fakePhone.snapshot });
  }

  close() {
    this.shut(1000);
  }

  deliver(msg: object) {
    setTimeout(() => this.readyState === 1 && this.onmessage?.({ data: JSON.stringify(msg) }));
  }

  shut(code: number) {
    if (this.readyState === 3) return;
    this.readyState = 3;
    clearInterval(this.beat);
    setTimeout(() => this.onclose?.({ code }));
  }
}
window.WebSocket = FakeSocket as unknown as typeof WebSocket;

class FakeNode {
  gain = { value: 1 };
  connect<T>(next: T) {
    return next;
  }
  disconnect() {}
}

window.AudioContext = class {
  sampleRate = 48_000;
  destination = new FakeNode();
  audioWorklet = { addModule: async () => {} };
  get state() {
    return fakePhone.audio;
  }
  createGain = () => new FakeNode();
  createMediaStreamSource = () => new FakeNode();
  resume = async () => void (fakePhone.audio = "running");
  close = async () => {};
} as unknown as typeof AudioContext;

window.AudioWorkletNode = class extends FakeNode {
  port = { onmessage: null };
  constructor() {
    super();
    worklet = this;
  }
} as unknown as typeof AudioWorkletNode;

const track = { readyState: "live", stop() {} };
navigator.mediaDevices.getUserMedia = async () => {
  if (!fakePhone.micAllowed) throw new DOMException("Permission denied", "NotAllowedError");
  return { getAudioTracks: () => [track], getTracks: () => [track] } as unknown as MediaStream;
};
Object.defineProperty(navigator, "wakeLock", {
  configurable: true,
  value: {
    request: async () => {
      fakePhone.wakeLocks++;
      return { release: async () => void fakePhone.released++ };
    },
  },
});
```

Create `app/tests/phone.ts`:

```ts
import { test as base, expect, type Page } from "@playwright/test";

export { expect };

/** The phone page with its computer, mic and wake lock faked (tests/fake-phone.ts); fails a test that throws in the page. */
export const test = base.extend({
  page: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.stack ?? e.message));
    await page.route(
      (url) => url.pathname === "/phone",
      async (route) => {
        const response = await route.fetch();
        const body = (await response.text()).replace("<head>", '<head><script type="module" src="/tests/fake-phone.ts"></script>');
        await route.fulfill({ response, body });
      },
    );
    await use(page);
    expect(errors).toEqual([]);
  },
});

/** Every message the page sent to the computer. */
export const sent = (page: Page) => page.evaluate(() => window.fakePhone.sent);

/** How many frames of sound the page sent. */
export const frames = (page: Page) => page.evaluate(() => window.fakePhone.frames);

/** Sends a message from the computer to the page. */
export const server = (page: Page, msg: object) => page.evaluate((m) => window.fakePhone.server(m), msg);

/** Opens the page from the QR code, joins as `name` and allows the mic. */
export async function joinAs(page: Page, name = "Aiko") {
  await page.goto("/phone?code=4827");
  await page.getByLabel("Your name").fill(name);
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await page.getByRole("button", { name: "Allow microphone" }).click();
  await expect(page.getByRole("button", { name: "Mute" })).toBeVisible();
}
```

- [ ] **Step 2: Write the failing tests**

Create `app/tests/phone.spec.ts`:

```ts
import { test, expect, sent, frames, joinAs } from "./phone";

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

test("a guest joins with the code from the QR code, allows the mic and can mute", async ({ page }) => {
  await page.goto("/phone?code=4827");
  await expect(page.getByLabel("Join code")).toHaveValue("4827");
  await expect(page.getByText("Filled in from the QR code")).toBeVisible();
  const join = page.getByRole("button", { name: "Join", exact: true });
  await expect(join).toBeDisabled();
  await page.getByLabel("Your name").fill("Aiko");
  await join.click();
  await page.getByRole("button", { name: "Allow microphone" }).click();
  await expect(page.getByText("Tap to mute")).toBeVisible();
  await expect(page.locator(".chip")).toHaveText("Aiko");
  await expect.poll(async () => (await sent(page)).some((m) => m.t === "ping")).toBe(true);
  expect(await sent(page)).toEqual(expect.arrayContaining([
    { t: "join", code: "4827", id: expect.any(String), name: "Aiko" },
    { t: "live", on: true, rate: 48000 },
  ]));
  expect(await page.evaluate(() => window.fakePhone.wakeLocks)).toBe(1);
  await page.evaluate(() => window.fakePhone.frame());
  expect(await frames(page)).toBe(1);
  await page.getByRole("button", { name: "Mute" }).click();
  await expect(page.getByText("Tap to sing")).toBeVisible();
  await page.evaluate(() => window.fakePhone.frame());
  expect(await frames(page)).toBe(1);
  expect((await sent(page)).filter((m) => m.t === "live").at(-1)).toEqual({ t: "live", on: false, rate: 48000 });
});

test("a wrong code or a full room sends the guest back to Join with the reason", async ({ page }) => {
  await page.goto("/phone?code=1111");
  await page.getByLabel("Your name").fill("Aiko");
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await expect(page.getByText("That code didn't work. Check it on the computer.")).toBeVisible();
  await page.getByLabel("Join code").fill("4827");
  await page.evaluate(() => (window.fakePhone.full = true));
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await expect(page.getByText("This room is full.")).toBeVisible();
});

test("a blocked mic shows how to allow it, and Try again asks again", async ({ page }) => {
  await page.goto("/phone?code=4827");
  await page.evaluate(() => (window.fakePhone.micAllowed = false));
  await page.getByLabel("Your name").fill("Aiko");
  await page.getByRole("button", { name: "Join", exact: true }).click();
  await page.getByRole("button", { name: "Allow microphone" }).click();
  await expect(page.getByRole("heading", { name: "Mic blocked" })).toBeVisible();
  await expect(page.getByText(/iPhone: tap aA/)).toBeVisible();
  await page.evaluate(() => (window.fakePhone.micAllowed = true));
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByText("Tap to mute")).toBeVisible();
});

test("when Wi-Fi drops it rejoins as the same phone, sends no late sound, and gives up after two minutes", async ({ page }) => {
  await page.clock.install();
  await joinAs(page);
  await page.evaluate(() => window.fakePhone.drop());
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeVisible();
  await expect(page.getByText("Waiting for Wi-Fi")).toBeVisible();
  await page.evaluate(() => window.fakePhone.frame());
  await page.clock.fastForward(2_100);
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeHidden();
  const joins = (await sent(page)).filter((m) => m.t === "join");
  expect(joins).toHaveLength(2);
  expect(joins[1].id).toBe(joins[0].id);
  expect(await frames(page)).toBe(0);
  expect((await sent(page)).filter((m) => m.t === "live")).toHaveLength(2);
  await page.evaluate(() => {
    window.fakePhone.unreachable = true;
    window.fakePhone.drop();
  });
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeVisible();
  await page.clock.fastForward(121_000);
  await expect(page.getByRole("heading", { name: "The host ended the session" })).toBeVisible();
});

test("when the host ends the session the guest sees it and can join again", async ({ page }) => {
  await joinAs(page, "Aiko");
  await page.evaluate(() => window.fakePhone.end());
  await expect(page.getByRole("heading", { name: "The host ended the session" })).toBeVisible();
  await expect(page.getByText("Thanks for singing, Aiko!")).toBeVisible();
  expect(await page.evaluate(() => window.fakePhone.released)).toBe(1);
  await page.getByRole("button", { name: "Join again" }).click();
  await expect(page.getByLabel("Your name")).toHaveValue("Aiko");
});

test("a connection gone silent is replaced, and a mic stopped by the lock screen waits for a tap", async ({ page }) => {
  await page.clock.install();
  await joinAs(page);
  await page.evaluate(() => (window.fakePhone.quiet = true));
  await page.clock.fastForward(3_500);
  await expect(page.getByText("Wi-Fi dropped. Reconnecting…")).toBeVisible();
  await page.evaluate(() => (window.fakePhone.quiet = false));
  await page.clock.fastForward(2_100);
  await expect.poll(async () => (await sent(page)).filter((m) => m.t === "join").length).toBe(2);
  await page.evaluate(() => {
    window.fakePhone.audio = "suspended";
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect(page.getByText("Tap to sing")).toBeVisible();
  await page.getByRole("button", { name: "Unmute" }).click();
  await expect(page.getByText("Tap to mute")).toBeVisible();
});

test.describe("on a phone set to Japanese", () => {
  test.use({ locale: "ja-JP" });

  test("the page follows the phone's language", async ({ page }) => {
    await page.goto("/phone?code=4827");
    await expect(page.getByRole("heading", { name: "カラオケに参加" })).toBeVisible();
  });
});
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd app && npx playwright test tests/phone.spec.ts`
Expected: FAIL — `/phone` shows SvelteKit's "Not found" page (no "Join code" field).

- [ ] **Step 4: Mic capture**

Create `app/src/lib/phone/capture-worklet.ts`:

```ts
/** Collects the mic's samples into 256-sample 16-bit frames (about 5 ms) and posts each one to the page. */
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: new () => AudioWorkletProcessor): void;

class Capture extends AudioWorkletProcessor {
  private frame = new Int16Array(256);
  private n = 0;

  process([input]: Float32Array[][]) {
    for (const x of input?.[0] ?? []) {
      this.frame[this.n++] = Math.max(-32768, Math.min(32767, Math.round(x * 32768)));
      if (this.n === this.frame.length) {
        this.port.postMessage(this.frame.buffer, [this.frame.buffer]);
        this.frame = new Int16Array(256);
        this.n = 0;
      }
    }
    return true;
  }
}

registerProcessor("capture", Capture);
```

Create `app/src/lib/phone/mic.ts`:

```ts
import workletUrl from "./capture-worklet.ts?worker&url";

export interface Mic {
  rate: number;
  running(): boolean;
  resume(): Promise<void>;
  stop(): void;
}

const capture = () => navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, autoGainControl: false, noiseSuppression: false } });

/** Opens the mic (echo cancellation on, auto gain off) and hands `onFrame` about 5 ms of 16-bit samples at a time. */
export async function openMic(onFrame: (pcm: ArrayBuffer) => void): Promise<Mic> {
  const ctx = new AudioContext();
  void ctx.resume();
  try {
    let stream = await capture();
    await ctx.audioWorklet.addModule(workletUrl);
    const node = new AudioWorkletNode(ctx, "capture");
    node.port.onmessage = (e: MessageEvent<ArrayBuffer>) => onFrame(e.data);
    const silent = ctx.createGain();
    silent.gain.value = 0;
    node.connect(silent).connect(ctx.destination);
    let source = ctx.createMediaStreamSource(stream);
    source.connect(node);
    return {
      rate: ctx.sampleRate,
      running: () => ctx.state === "running",
      async resume() {
        if (stream.getAudioTracks()[0]?.readyState === "ended") {
          source.disconnect();
          stream = await capture();
          source = ctx.createMediaStreamSource(stream);
          source.connect(node);
        }
        await ctx.resume();
      },
      stop() {
        stream.getTracks().forEach((t) => t.stop());
        void ctx.close();
      },
    };
  } catch (e) {
    void ctx.close();
    throw e;
  }
}
```

- [ ] **Step 5: Text in six languages**

Add at the end of each locale. `en.ts`:

```ts
  "phone.joinTitle": "Join the karaoke",
  "phone.joinLead": "Your phone becomes a microphone.",
  "phone.code": "Join code",
  "phone.codeFromQr": "Filled in from the QR code",
  "phone.name": "Your name",
  "phone.join": "Join",
  "phone.wrongCode": "That code didn't work. Check it on the computer.",
  "phone.full": "This room is full.",
  "phone.unreachable": "Couldn't reach the computer. Check that you're on the same Wi-Fi.",
  "phone.connecting": "Connecting…",
  "phone.joining": "Joining {code}",
  "phone.permTitle": "Use your microphone",
  "phone.permLead": "Let this page use your microphone so you can sing. Your voice goes only to the computer you joined.",
  "phone.permWarning": "Saw a warning page when you opened this? That's normal the first time, and safe to continue.",
  "phone.allow": "Allow microphone",
  "phone.blockedTitle": "Mic blocked",
  "phone.blockedLead": "This page isn't allowed to use your microphone. Here's how to turn it on:",
  "phone.blockedIphone": "iPhone: tap aA in the address bar, then Website Settings, then set Microphone to Allow.",
  "phone.blockedAndroid": "Android: tap the icon left of the address, then Permissions, then turn on Microphone.",
  "phone.endedTitle": "The host ended the session",
  "phone.endedLead": "Thanks for singing, {name}!",
  "phone.joinAgain": "Join again",
  "phone.leave": "Leave",
  "phone.tabMic": "Mic",
  "phone.tapToSing": "Tap to sing",
  "phone.tapToMute": "Tap to mute",
  "phone.waitingWifi": "Waiting for Wi-Fi",
  "phone.mute": "Mute",
  "phone.unmute": "Unmute",
  "phone.reconnecting": "Wi-Fi dropped. Reconnecting…",
```

`ja.ts`:

```ts
  "phone.joinTitle": "カラオケに参加",
  "phone.joinLead": "スマホがマイクになります。",
  "phone.code": "参加コード",
  "phone.codeFromQr": "QRコードから入力されました",
  "phone.name": "あなたの名前",
  "phone.join": "参加",
  "phone.wrongCode": "そのコードは使えませんでした。コンピュータで確認してください。",
  "phone.full": "このルームは満員です。",
  "phone.unreachable": "コンピュータに接続できませんでした。同じWi-Fiにつながっているか確認してください。",
  "phone.connecting": "接続中…",
  "phone.joining": "{code} に参加しています",
  "phone.permTitle": "マイクを使います",
  "phone.permLead": "歌えるように、このページにマイクの使用を許可してください。声は参加したコンピュータにだけ届きます。",
  "phone.permWarning": "開いたときに警告ページが出ましたか？初回は普通のことで、そのまま進んで大丈夫です。",
  "phone.allow": "マイクを許可",
  "phone.blockedTitle": "マイクがブロックされています",
  "phone.blockedLead": "このページはマイクの使用を許可されていません。オンにする方法：",
  "phone.blockedIphone": "iPhone：アドレスバーの「ぁあ」をタップし、「Webサイトの設定」で「マイク」を「許可」にします。",
  "phone.blockedAndroid": "Android：アドレスの左のアイコンをタップし、「権限」で「マイク」をオンにします。",
  "phone.endedTitle": "ホストがセッションを終了しました",
  "phone.endedLead": "{name} さん、歌ってくれてありがとう！",
  "phone.joinAgain": "もう一度参加",
  "phone.leave": "退出",
  "phone.tabMic": "マイク",
  "phone.tapToSing": "タップして歌う",
  "phone.tapToMute": "タップしてミュート",
  "phone.waitingWifi": "Wi-Fiを待っています",
  "phone.mute": "ミュート",
  "phone.unmute": "ミュート解除",
  "phone.reconnecting": "Wi-Fiが切れました。再接続しています…",
```

`ko.ts`:

```ts
  "phone.joinTitle": "노래방에 참여하기",
  "phone.joinLead": "휴대폰이 마이크가 됩니다.",
  "phone.code": "참여 코드",
  "phone.codeFromQr": "QR 코드에서 입력됨",
  "phone.name": "이름",
  "phone.join": "참여",
  "phone.wrongCode": "코드가 맞지 않습니다. 컴퓨터에서 확인하세요.",
  "phone.full": "이 방은 가득 찼습니다.",
  "phone.unreachable": "컴퓨터에 연결할 수 없습니다. 같은 Wi-Fi에 연결되어 있는지 확인하세요.",
  "phone.connecting": "연결 중…",
  "phone.joining": "{code}에 참여하는 중",
  "phone.permTitle": "마이크를 사용합니다",
  "phone.permLead": "노래할 수 있도록 이 페이지가 마이크를 사용하게 허용하세요. 목소리는 참여한 컴퓨터로만 전달됩니다.",
  "phone.permWarning": "이 페이지를 열 때 경고 페이지가 보였나요? 처음에는 정상이며 계속해도 안전합니다.",
  "phone.allow": "마이크 허용",
  "phone.blockedTitle": "마이크가 차단됨",
  "phone.blockedLead": "이 페이지는 마이크를 사용할 수 없습니다. 켜는 방법:",
  "phone.blockedIphone": "iPhone: 주소 막대의 가가를 탭하고 웹 사이트 설정에서 마이크를 허용으로 설정하세요.",
  "phone.blockedAndroid": "Android: 주소 왼쪽의 아이콘을 탭하고 권한에서 마이크를 켜세요.",
  "phone.endedTitle": "호스트가 세션을 종료했습니다",
  "phone.endedLead": "{name} 님, 노래해 줘서 고마워요!",
  "phone.joinAgain": "다시 참여",
  "phone.leave": "나가기",
  "phone.tabMic": "마이크",
  "phone.tapToSing": "탭해서 노래하기",
  "phone.tapToMute": "탭해서 음소거",
  "phone.waitingWifi": "Wi-Fi를 기다리는 중",
  "phone.mute": "음소거",
  "phone.unmute": "음소거 해제",
  "phone.reconnecting": "Wi-Fi 연결이 끊겼습니다. 다시 연결하는 중…",
```

`zh-Hans.ts`:

```ts
  "phone.joinTitle": "加入卡拉 OK",
  "phone.joinLead": "你的手机会变成麦克风。",
  "phone.code": "加入代码",
  "phone.codeFromQr": "已从二维码填入",
  "phone.name": "你的名字",
  "phone.join": "加入",
  "phone.wrongCode": "这个代码无效。请在电脑上核对。",
  "phone.full": "房间已满。",
  "phone.unreachable": "无法连接到电脑。请确认连接的是同一个 Wi-Fi。",
  "phone.connecting": "正在连接…",
  "phone.joining": "正在加入 {code}",
  "phone.permTitle": "使用你的麦克风",
  "phone.permLead": "允许此页面使用麦克风，你就可以唱歌了。你的声音只会传到你加入的电脑。",
  "phone.permWarning": "打开时看到了警告页面？第一次出现是正常的，可以放心继续。",
  "phone.allow": "允许使用麦克风",
  "phone.blockedTitle": "麦克风被阻止",
  "phone.blockedLead": "此页面无权使用你的麦克风。开启方法：",
  "phone.blockedIphone": "iPhone：轻点地址栏中的“大小”，然后“网站设置”，把“麦克风”设为“允许”。",
  "phone.blockedAndroid": "Android：轻点地址左侧的图标，然后“权限”，开启“麦克风”。",
  "phone.endedTitle": "主持人结束了本次活动",
  "phone.endedLead": "谢谢你的演唱，{name}！",
  "phone.joinAgain": "重新加入",
  "phone.leave": "离开",
  "phone.tabMic": "麦克风",
  "phone.tapToSing": "轻点开始唱",
  "phone.tapToMute": "轻点静音",
  "phone.waitingWifi": "正在等待 Wi-Fi",
  "phone.mute": "静音",
  "phone.unmute": "取消静音",
  "phone.reconnecting": "Wi-Fi 断开了。正在重新连接…",
```

`zh-Hant.ts`:

```ts
  "phone.joinTitle": "加入卡拉 OK",
  "phone.joinLead": "你的手機會變成麥克風。",
  "phone.code": "加入代碼",
  "phone.codeFromQr": "已從 QR 碼填入",
  "phone.name": "你的名字",
  "phone.join": "加入",
  "phone.wrongCode": "這組代碼無效。請在電腦上確認。",
  "phone.full": "房間已滿。",
  "phone.unreachable": "無法連線到電腦。請確認連上的是同一個 Wi-Fi。",
  "phone.connecting": "正在連線…",
  "phone.joining": "正在加入 {code}",
  "phone.permTitle": "使用你的麥克風",
  "phone.permLead": "允許此頁面使用麥克風，你就可以唱歌了。你的聲音只會傳到你加入的電腦。",
  "phone.permWarning": "打開時看到了警告頁面？第一次出現是正常的，可以放心繼續。",
  "phone.allow": "允許使用麥克風",
  "phone.blockedTitle": "麥克風被封鎖",
  "phone.blockedLead": "此頁面無權使用你的麥克風。開啟方法：",
  "phone.blockedIphone": "iPhone：點一下網址列中的「大小」，然後「網站設定」，把「麥克風」設為「允許」。",
  "phone.blockedAndroid": "Android：點一下網址左側的圖示，然後「權限」，開啟「麥克風」。",
  "phone.endedTitle": "主持人結束了這次活動",
  "phone.endedLead": "謝謝你的演唱，{name}！",
  "phone.joinAgain": "重新加入",
  "phone.leave": "離開",
  "phone.tabMic": "麥克風",
  "phone.tapToSing": "點一下開始唱",
  "phone.tapToMute": "點一下靜音",
  "phone.waitingWifi": "正在等待 Wi-Fi",
  "phone.mute": "靜音",
  "phone.unmute": "取消靜音",
  "phone.reconnecting": "Wi-Fi 斷線了。正在重新連線…",
```

`es.ts`:

```ts
  "phone.joinTitle": "Únete al karaoke",
  "phone.joinLead": "Tu móvil se convierte en un micrófono.",
  "phone.code": "Código para unirse",
  "phone.codeFromQr": "Rellenado desde el código QR",
  "phone.name": "Tu nombre",
  "phone.join": "Unirse",
  "phone.wrongCode": "Ese código no funciona. Compruébalo en el ordenador.",
  "phone.full": "Esta sala está llena.",
  "phone.unreachable": "No se pudo conectar con el ordenador. Comprueba que estás en la misma wifi.",
  "phone.connecting": "Conectando…",
  "phone.joining": "Uniéndote a {code}",
  "phone.permTitle": "Usa tu micrófono",
  "phone.permLead": "Deja que esta página use tu micrófono para que puedas cantar. Tu voz solo va al ordenador al que te uniste.",
  "phone.permWarning": "¿Viste una página de aviso al abrir esto? Es normal la primera vez y puedes continuar sin problema.",
  "phone.allow": "Permitir el micrófono",
  "phone.blockedTitle": "Micrófono bloqueado",
  "phone.blockedLead": "Esta página no tiene permiso para usar tu micrófono. Así puedes activarlo:",
  "phone.blockedIphone": "iPhone: toca aA en la barra de direcciones, luego Ajustes del sitio web y pon Micrófono en Permitir.",
  "phone.blockedAndroid": "Android: toca el icono a la izquierda de la dirección, luego Permisos y activa Micrófono.",
  "phone.endedTitle": "El anfitrión terminó la sesión",
  "phone.endedLead": "¡Gracias por cantar, {name}!",
  "phone.joinAgain": "Volver a unirse",
  "phone.leave": "Salir",
  "phone.tabMic": "Micro",
  "phone.tapToSing": "Toca para cantar",
  "phone.tapToMute": "Toca para silenciar",
  "phone.waitingWifi": "Esperando la wifi",
  "phone.mute": "Silenciar",
  "phone.unmute": "Activar sonido",
  "phone.reconnecting": "Se cortó la wifi. Reconectando…",
```

- [ ] **Step 6: The connection**

Create `app/src/lib/phone/link.svelte.ts`:

```ts
import type { PlayerSnapshot, ProblemCode } from "$lib/api";
import { say } from "$lib/i18n/engine";
import { toasts } from "$lib/state/toasts.svelte";
import { openMic, type Mic } from "./mic";
import WarningIcon from "phosphor-svelte/lib/WarningIcon";

export type Screen = "join" | "connecting" | "perm" | "mic" | "ended" | "blocked";
export type Refusal = "wrongCode" | "full" | "unreachable";

const ENDED = 4001;
const FULL = 4002;
const WRONG_CODE = 4003;
const RETRY_MS = 2_000;
const GIVE_UP_MS = 120_000;
const SILENT_MS = 3_000;

type FromMac =
  | { t: "joined" }
  | { t: "player"; snapshot: PlayerSnapshot }
  | { t: "level"; v: number }
  | { t: "refused"; problem: ProblemCode | null };

function recall(storage: () => Storage, key: string): string | null {
  try {
    return storage().getItem(key);
  } catch {
    return null;
  }
}

function keep(storage: () => Storage, key: string, value: string) {
  try {
    storage().setItem(key, value);
  } catch {
    return;
  }
}

/** The phone's side of the session: joining, the mic, staying connected, and what the computer shares. */
class PhoneLink {
  screen = $state<Screen>("join");
  refusal = $state<Refusal | null>(null);
  reconnecting = $state(false);
  code = $state(new URLSearchParams(location.search).get("code") ?? "");
  readonly fromQr = this.code !== "";
  name = $state(recall(() => localStorage, "phone.name") ?? "");
  live = $state(false);
  level = $state(0);
  snapshot = $state<PlayerSnapshot | null>(null);

  private readonly id = recall(() => sessionStorage, "phone.id") ?? crypto.randomUUID();
  private socket: WebSocket | null = null;
  private mic: Mic | null = null;
  private wake: WakeLockSentinel | null = null;
  private heard = 0;
  private lostAt = 0;
  private retry: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    keep(() => sessionStorage, "phone.id", this.id);
  }

  /** Follows the page being hidden and shown and replaces a connection gone silent; returns the cleanup. */
  start() {
    const shown = () => this.shown();
    document.addEventListener("visibilitychange", shown);
    const beat = setInterval(() => this.beat(), 1_000);
    return () => {
      document.removeEventListener("visibilitychange", shown);
      clearInterval(beat);
    };
  }

  join() {
    keep(() => localStorage, "phone.name", this.name.trim());
    this.refusal = null;
    this.screen = "connecting";
    this.connect();
  }

  /** Asks for the mic; the page then sends its sound while live. */
  async allowMic() {
    try {
      this.mic = await openMic((pcm) => this.hear(pcm));
      this.live = true;
      this.sendLive();
      this.screen = "mic";
    } catch {
      this.screen = "blocked";
    }
  }

  async toggleLive() {
    if (!this.mic) return;
    const on = !this.live;
    if (on) await this.mic.resume().catch(() => {});
    this.live = on && this.mic.running();
    this.sendLive();
  }

  leave() {
    this.send({ t: "leave" });
    this.stop();
    this.screen = "join";
  }

  again() {
    this.screen = "join";
  }

  private get joined() {
    return this.screen === "perm" || this.screen === "blocked" || this.screen === "mic";
  }

  private connect() {
    const ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`);
    ws.binaryType = "arraybuffer";
    this.socket = ws;
    this.heard = performance.now();
    ws.onopen = () => this.send({ t: "join", code: this.code, id: this.id, name: this.name.trim() });
    ws.onmessage = (e) => {
      this.heard = performance.now();
      this.receive(JSON.parse(e.data));
    };
    ws.onclose = (e) => {
      if (this.socket === ws) this.closed(e.code);
    };
  }

  private receive(m: FromMac) {
    if (m.t === "joined") {
      if (this.reconnecting) {
        this.reconnecting = false;
        this.sendLive();
      } else this.screen = "perm";
      void this.keepAwake();
    } else if (m.t === "player") this.snapshot = m.snapshot;
    else if (m.t === "level") this.level = m.v;
    else if (m.t === "refused") toasts.show(say(m), { icon: WarningIcon });
  }

  /** The connection closed with `code`: the session ended, the phone was refused, or it was lost and is tried again for two minutes. */
  private closed(code: number) {
    this.socket = null;
    if (code === ENDED || (this.reconnecting && code === WRONG_CODE)) return this.end();
    if (!this.joined) {
      this.refusal = code === FULL ? "full" : code === WRONG_CODE ? "wrongCode" : "unreachable";
      this.screen = "join";
      return;
    }
    if (!this.reconnecting) {
      this.reconnecting = true;
      this.lostAt = Date.now();
    }
    if (Date.now() - this.lostAt >= GIVE_UP_MS) return this.end();
    this.retry = setTimeout(() => this.connect(), RETRY_MS);
  }

  private end() {
    this.stop();
    this.screen = "ended";
  }

  private stop() {
    clearTimeout(this.retry);
    const ws = this.socket;
    this.socket = null;
    ws?.close();
    this.mic?.stop();
    this.mic = null;
    void this.wake?.release().catch(() => {});
    this.wake = null;
    this.live = false;
    this.reconnecting = false;
    this.snapshot = null;
  }

  private hear(pcm: ArrayBuffer) {
    if (this.live && !this.reconnecting && this.socket?.readyState === WebSocket.OPEN) this.socket.send(pcm);
  }

  private sendLive() {
    if (this.mic) this.send({ t: "live", on: this.live, rate: this.mic.rate });
  }

  /** Every second while joined: a ping so the computer knows the phone is there, and a connection that has said nothing for a few seconds counts as dropped. */
  private beat() {
    if (!this.joined || !this.socket) return;
    this.send({ t: "ping" });
    if (performance.now() - this.heard < SILENT_MS) return;
    const ws = this.socket;
    this.socket = null;
    ws.close();
    this.closed(1006);
  }

  /** Back on screen: keeps the screen awake again, and a mic the lock screen stopped waits for a tap. */
  private shown() {
    if (document.visibilityState !== "visible" || this.screen !== "mic") return;
    void this.keepAwake();
    if (this.live && this.mic && !this.mic.running()) {
      this.live = false;
      this.sendLive();
    }
  }

  private async keepAwake() {
    try {
      const wake = await navigator.wakeLock?.request("screen");
      if (!this.joined) return void wake?.release();
      void this.wake?.release().catch(() => {});
      this.wake = wake ?? null;
    } catch {
      return;
    }
  }

  private send(m: object) {
    if (this.socket?.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(m));
  }
}

export const link = new PhoneLink();
```

- [ ] **Step 7: The screens**

Create `app/src/lib/phone/phone.css` (from the prototype's phone CSS):

```css
.phone { --side-w: 0px; position: relative; display: flex; flex-direction: column; height: 100dvh; padding: var(--s4) var(--s5) var(--s5); overflow: hidden; }
.pscreen { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.pcenter { flex: 1; display: flex; flex-direction: column; justify-content: center; align-items: center; text-align: center; gap: var(--s3); }
.pscreen h1 { font: 800 30px/1.1 var(--display); letter-spacing: -.02em; }
.lead { color: var(--muted); font-size: 16px; }
.bigicon { width: 96px; height: 96px; border-radius: 50%; display: grid; place-items: center; background: var(--raised); }
.bigicon.warn { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
.bigicon .spin { width: 40px; height: 40px; border-width: 4px; }
.pnote { display: flex; gap: var(--s3); padding: var(--s3) var(--s4); border-radius: var(--r-lg); background: var(--raised); font-size: 14px; text-align: left; }
.pnote svg { flex: none; margin-top: 2px; color: var(--muted); }
.pbtn { width: 100%; height: 56px; margin-top: var(--s4); border-radius: var(--r-lg); font-size: 17px; font-weight: 600; }
.logo { width: 48px; height: 48px; border-radius: var(--r-lg); display: grid; place-items: center; background: var(--accent); color: var(--on-accent); }
.pfield { display: grid; gap: 6px; margin-top: var(--s5); }
.pinput { display: flex; align-items: center; gap: var(--s2); height: 52px; padding: 0 var(--s4); border-radius: var(--r-lg); background: var(--surface); border: 1px solid var(--line); }
.pinput input { flex: 1; min-width: 0; border: 0; background: none; outline: none; font-size: 18px; }
.pinput .code-in { font: 500 20px var(--mono); letter-spacing: .08em; text-transform: uppercase; }
.pinput svg { color: var(--ready); }
.phead { display: flex; align-items: center; gap: var(--s2); }
.chip { display: inline-flex; align-items: center; gap: 6px; max-width: 70%; height: 28px; padding: 0 var(--s3); border-radius: 999px; background: var(--accent); color: var(--on-accent); font: 600 14px var(--body); }
.ptab { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: auto; margin: 0 calc(-1 * var(--s5)) calc(-1 * var(--s5)); padding: 0 var(--s5) 96px; }
.ptabs { position: absolute; z-index: 2; left: var(--s5); right: var(--s5); bottom: var(--s5); display: grid; grid-auto-flow: column; grid-auto-columns: 1fr; padding: 2px var(--s2); border-radius: 999px; }
.ptabs button { display: grid; justify-items: center; align-content: center; gap: 2px; height: 54px; font-size: 11px; font-weight: 600; color: var(--muted); }
.ptabs button[aria-pressed="true"] { color: var(--accent); }
.pbanner { position: absolute; z-index: 3; left: var(--s4); right: var(--s4); top: var(--s4); min-height: 56px; display: flex; align-items: center; gap: var(--s3); padding: var(--s3) var(--s4); border-radius: var(--r-lg); font-weight: 500; }
```

Create `app/src/lib/phone/Center.svelte`:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Icon } from "$lib/icons";

  let { icon: IconC, warn = false, title, lead, children, action }: {
    icon?: Icon;
    warn?: boolean;
    title: string;
    lead: string;
    children?: Snippet;
    action?: Snippet;
  } = $props();
</script>

<div class="pcenter">
  <div class="bigicon" class:warn>{#if IconC}<IconC size={44} />{:else}<span class="spin"></span>{/if}</div>
  <h1>{title}</h1>
  <p class="lead">{lead}</p>
  {@render children?.()}
</div>
{@render action?.()}
```

Create `app/src/lib/phone/JoinScreen.svelte`:

```svelte
<script lang="ts">
  import { link } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { composing } from "$lib/keys";
  import { slide } from "$lib/motion";
  import ArrowRightIcon from "phosphor-svelte/lib/ArrowRightIcon";
  import CheckCircleIcon from "phosphor-svelte/lib/CheckCircleIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import QrCodeIcon from "phosphor-svelte/lib/QrCodeIcon";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  const REASONS = { wrongCode: "phone.wrongCode", full: "phone.full", unreachable: "phone.unreachable" } as const;
  const ready = $derived(!!link.code.trim() && !!link.name.trim());
  const go = () => ready && link.join();
</script>

<div class="logo"><MicrophoneStageIcon size={24} /></div>
<h1 class="title">{t("phone.joinTitle")}</h1>
<p class="lead">{t("phone.joinLead")}</p>
<label class="pfield">
  <span class="cap hstack"><QrCodeIcon size={14} />{t("phone.code")}</span>
  <span class="pinput">
    <span class="prefix">OKI-</span>
    <input class="code-in" bind:value={link.code} maxlength="4" inputmode="numeric" autocomplete="off" />
    {#if link.fromQr}<CheckCircleIcon size={20} />{/if}
  </span>
  {#if link.fromQr}<small class="muted">{t("phone.codeFromQr")}</small>{/if}
</label>
<label class="pfield">
  <span class="cap hstack"><UserIcon size={14} />{t("phone.name")}</span>
  <span class="pinput"><input bind:value={link.name} maxlength="20" autocomplete="nickname" onkeydown={(e) => e.key === "Enter" && !composing(e) && go()} /></span>
</label>
{#if link.refusal}
  <p class="why hstack" in:slide={{ y: -8 }}><WarningIcon size={16} />{t(REASONS[link.refusal])}</p>
{/if}
<div class="grow"></div>
<button class="btn accent pbtn" disabled={!ready} onclick={go}><ArrowRightIcon size={20} />{t("phone.join")}</button>

<style>
  .title { margin-top: var(--s5); }
  .lead { margin-top: var(--s2); }
  .why { margin-top: var(--s4); color: var(--accent); font-size: 14px; font-weight: 500; }
  .prefix { margin-right: -6px; font: 500 20px var(--mono); letter-spacing: .08em; color: var(--muted); }
</style>
```

Create `app/src/lib/phone/MicTab.svelte`:

```svelte
<script lang="ts">
  import { link } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { loudness } from "$lib/format";
  import MicrophoneSlashIcon from "phosphor-svelte/lib/MicrophoneSlashIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import WifiSlashIcon from "phosphor-svelte/lib/WifiSlashIcon";
</script>

<div class="grow"></div>
<button
  class="micbtn"
  class:muted={!link.live}
  disabled={link.reconnecting}
  aria-label={link.live ? t("phone.mute") : t("phone.unmute")}
  style:--lv={link.live ? loudness(link.level) : 0}
  onclick={() => link.toggleLive()}
>
  <span class="ring"></span>
  {#if link.live}<MicrophoneStageIcon size={64} />{:else}<MicrophoneSlashIcon size={64} />{/if}
</button>
<p class="pstate" class:muted={!link.live}>
  {#if link.reconnecting}<WifiSlashIcon size={16} />{t("phone.waitingWifi")}{:else}<span class="dot"></span>{link.live ? t("phone.tapToMute") : t("phone.tapToSing")}{/if}
</p>

<style>
  .micbtn { position: relative; flex: none; width: 156px; height: 156px; margin: 0 auto; border-radius: 50%; display: grid; place-items: center; background: var(--accent); color: var(--on-accent); }
  .micbtn > :global(svg) { position: relative; }
  .ring { position: absolute; inset: -14px; border-radius: 50%; border: 6px solid var(--accent); opacity: calc(var(--lv, 0) * .9); transform: scale(calc(1 + var(--lv, 0) * .12 * var(--motion))); transition: opacity 120ms linear, transform 120ms linear; }
  .micbtn.muted { background: var(--raised); color: var(--text); }
  .micbtn.muted .ring { opacity: 0; }
  .pstate { display: flex; justify-content: center; align-items: center; gap: var(--s2); margin-top: 36px; font-size: 13px; font-weight: 500; color: var(--muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ready); }
  .pstate.muted .dot { background: var(--faint); }
</style>
```

Create `app/src/routes/phone/+page.svelte`:

```svelte
<script lang="ts">
  import "$lib/phone/phone.css";
  import { onMount } from "svelte";
  import { link } from "$lib/phone/link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { slide } from "$lib/motion";
  import Toasts from "$lib/components/Toasts.svelte";
  import Center from "$lib/phone/Center.svelte";
  import JoinScreen from "$lib/phone/JoinScreen.svelte";
  import MicTab from "$lib/phone/MicTab.svelte";
  import AndroidLogoIcon from "phosphor-svelte/lib/AndroidLogoIcon";
  import AppleLogoIcon from "phosphor-svelte/lib/AppleLogoIcon";
  import ArrowClockwiseIcon from "phosphor-svelte/lib/ArrowClockwiseIcon";
  import HandWavingIcon from "phosphor-svelte/lib/HandWavingIcon";
  import MicrophoneIcon from "phosphor-svelte/lib/MicrophoneIcon";
  import MicrophoneSlashIcon from "phosphor-svelte/lib/MicrophoneSlashIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import ShieldWarningIcon from "phosphor-svelte/lib/ShieldWarningIcon";
  import SignOutIcon from "phosphor-svelte/lib/SignOutIcon";
  import WifiSlashIcon from "phosphor-svelte/lib/WifiSlashIcon";

  onMount(() => link.start());
</script>

<div class="phone">
  {#key link.screen}
    <div class="pscreen" in:slide={{ y: 8 }}>
      {#if link.screen === "join"}
        <JoinScreen />
      {:else if link.screen === "connecting"}
        <Center title={t("phone.connecting")} lead={t("phone.joining", { code: `OKI-${link.code.replace(/\D/g, "")}` })} />
      {:else if link.screen === "perm"}
        <Center icon={MicrophoneIcon} title={t("phone.permTitle")} lead={t("phone.permLead")}>
          <p class="pnote"><ShieldWarningIcon size={18} /><span>{t("phone.permWarning")}</span></p>
          {#snippet action()}<button class="btn accent pbtn" onclick={() => link.allowMic()}><MicrophoneIcon size={20} />{t("phone.allow")}</button>{/snippet}
        </Center>
      {:else if link.screen === "blocked"}
        <Center icon={MicrophoneSlashIcon} warn title={t("phone.blockedTitle")} lead={t("phone.blockedLead")}>
          <p class="pnote"><AppleLogoIcon size={18} /><span>{t("phone.blockedIphone")}</span></p>
          <p class="pnote"><AndroidLogoIcon size={18} /><span>{t("phone.blockedAndroid")}</span></p>
          {#snippet action()}<button class="btn accent pbtn" onclick={() => link.allowMic()}><ArrowClockwiseIcon size={20} />{t("common.tryAgain")}</button>{/snippet}
        </Center>
      {:else if link.screen === "ended"}
        <Center icon={HandWavingIcon} title={t("phone.endedTitle")} lead={t("phone.endedLead", { name: link.name })}>
          {#snippet action()}<button class="btn accent pbtn" onclick={() => link.again()}><ArrowClockwiseIcon size={20} />{t("phone.joinAgain")}</button>{/snippet}
        </Center>
      {:else}
        <header class="phead">
          <span class="chip"><MicrophoneStageIcon size={14} /><span class="ell">{link.name}</span></span>
          <span class="grow"></span>
          <button class="btn ghost" onclick={() => link.leave()}><SignOutIcon size={16} />{t("phone.leave")}</button>
        </header>
        <div class="ptab"><MicTab /></div>
        <nav class="ptabs glass">
          <button aria-pressed="true">{#if link.live}<MicrophoneStageIcon size={24} />{:else}<MicrophoneSlashIcon size={24} />{/if}<span>{t("phone.tabMic")}</span></button>
        </nav>
        {#if link.reconnecting}
          <div class="pbanner glass" role="status" transition:slide={{ y: -8 }}><span class="spin"></span><WifiSlashIcon size={18} /><span>{t("phone.reconnecting")}</span></div>
        {/if}
      {/if}
    </div>
  {/key}
  <Toasts />
</div>
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cd app && npm test && npm run check && npm run check:i18n && npx playwright test`
Expected: all PASS (7 new phone tests), 0 svelte-check errors, locales match.

Run: `cd app && npm run build`
Expected: the build succeeds and emits the capture worklet as its own file (`ls build/_app/immutable/workers/` or `grep -rl registerProcessor build/_app | head -1` finds it).

- [ ] **Step 9: Isolated check and commit**

Run the isolated check with the phone check for `task-10.png` (grep `phone check OK`; its `/phone` fetch now gets the phone page and its files from the embedded build).

```bash
git add app/src/routes/phone app/src/lib/phone app/src/lib/i18n app/tests/fake-phone.ts app/tests/phone.ts app/tests/phone.spec.ts
git commit -m "feat(app): phone page — join, mic permission, reconnecting, session ended"
```

---
### Task 11: The phone's Mic tab — the song, up next, lyrics, Voice and Singer

**Files:**
- Modify: `app/src/lib/phone/link.svelte.ts`, `app/src/lib/phone/MicTab.svelte`
- Create: `app/src/lib/phone/PhoneLyrics.svelte`
- Modify: `app/src/lib/art.ts`, `app/src/styles/base.css`
- Modify: `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`
- Modify: `app/tests/fake-phone.ts`
- Test: `app/tests/phone.spec.ts`

**Interfaces:**
- Consumes: `clock`, `lyrics`, `lyricsChanged` messages and `{t:"lyrics", trackId}`, `{t:"voice", v}`, `{t:"singer", v}` (Tasks 7/8); `timeline`, `itemAt`, `wordProgress`, `dotProgress` from `$lib/lyrics/timeline`; `Artwork.svelte`; Task 10's `link`.
- Produces:
  - `link` gains `clock`, `lyrics: { trackId: number; lyrics: Lyrics } | null`, `voice` (0–100; 80 at first, then what this phone last chose, kept in `localStorage` as `phone.voice` and sent with every `live` message so the Mac row always matches the phone), `current` / `next` (`QueueEntry | null`), `position(): number` (seconds into the current song), `setVoice(v)`, `setSinger(v)` (each sent at most every 100 ms, ending on the latest value). The page asks for the current song's lyrics once per song, again after `lyricsChanged`.
  - `PhoneLyrics.svelte` props `{ lyrics: Lyrics; t: number }` — the current line (word fill, countdown dots, duet part label) and the next line; "No lyrics found, sing it your way." only when the lookup found none (`source === "none"`), nothing while it is still looking (as on the Mac).
  - Slider thumbs show on touch screens (`@media (hover: none)` in `base.css`).
  - `art.ts`: a song picture path starting `/art/` is used as-is (the phone server's link); anything else goes through `convertFileSrc` as before.
  - Fake: `window.fakePhone.play(id): number` (the computer plays song `id` alone; returns its queue key) and made-up lyrics for songs 1 and 2.
  - i18n keys `phone.waitingSong`, `phone.addFromSongs`, `phone.upNext`, `phone.voice`, `phone.voiceAria`, `phone.singer`, `phone.partM`, `phone.partF`, `phone.partBoth`.

- [ ] **Step 1: Fake lyrics and a song lever**

In `app/tests/fake-phone.ts`: import `Lyrics, LyricLine` with the other types; add under `songs`:

```ts
const line = (voice: LyricLine["voice"], ...words: [string, number, number][]): LyricLine => ({
  start_ms: words[0][1], end_ms: words.at(-1)![2], text: words.map((w) => w[0]).join(" "), voice, words: words.map(([text, start_ms, end_ms]) => ({ text, start_ms, end_ms })),
});

const lyrics: Record<number, Lyrics> = {
  1: { source: "lrclib", lines: [
    line(undefined, ["Fold", 5000, 5500], ["the", 5500, 6000], ["morning", 6000, 7000], ["paper", 7000, 8000]),
    line(undefined, ["Send", 8500, 9000], ["it", 9000, 9500], ["down", 9500, 10000], ["the", 10000, 10500], ["drain", 10500, 11000]),
    line(undefined, ["Wave", 20000, 20500], ["from", 20500, 21000], ["the", 21000, 21500], ["bridge", 21500, 22000]),
  ] },
  2: { source: "embedded", lines: [
    line("m", ["Who", 1000, 1500], ["boiled", 1500, 2000], ["the", 2000, 2500], ["water", 2500, 3000]),
    line("f", ["I", 3000, 3500], ["did,", 3500, 4000], ["of", 4000, 4500], ["course", 4500, 5000]),
    line("both", ["Tea", 5000, 5500], ["for", 5500, 6000], ["two", 6000, 7000]),
  ] },
};
```

add to `fakePhone`:

```ts
  /** The computer plays song `id` alone; returns its queue key. */
  play(id: number) {
    const track = songs.find((s) => s.id === id)!;
    fakePhone.snapshot = { entries: [{ key: id * 10, track, by: null }], current: 0, ended: false, lyricOffsetMs: 0 };
    socket?.deliver({ t: "player", snapshot: fakePhone.snapshot });
    return id * 10;
  },
```

and at the top of `FakeSocket.reply`:

```ts
    if (m.t === "lyrics") return this.deliver({ t: "lyrics", trackId: m.trackId, lyrics: lyrics[Number(m.trackId)] ?? { source: "none", lines: [] } });
```

- [ ] **Step 2: Write the failing tests**

Add `server` to the import from `./phone` in `app/tests/phone.spec.ts` and append:

```ts
test("the Mic tab shows the song, what's next and the lyrics in time with the computer", async ({ page }) => {
  await joinAs(page);
  await expect(page.locator(".psong")).toContainText("Paper Boats");
  await expect(page.locator(".pnext")).toContainText("Up next: Lemon Skies · The Porchlights");
  expect(await sent(page)).toContainEqual({ t: "lyrics", trackId: 1 });
  await server(page, { t: "clock", key: 1, positionMs: 6_000, playing: false });
  await expect(page.locator(".plyr .now")).toHaveText("Fold the morning paper");
  await server(page, { t: "clock", key: 1, positionMs: 9_000, playing: false });
  await expect(page.locator(".plyr .now")).toHaveText("Send it down the drain");
  await expect(page.locator(".plyr .next")).toHaveText("Wave from the bridge");
  const key = await page.evaluate(() => window.fakePhone.play(2));
  await server(page, { t: "clock", key, positionMs: 1_200, playing: false });
  await expect(page.getByText("Male part")).toBeVisible();
  await page.evaluate(() => window.fakePhone.play(3));
  await expect(page.getByText("No lyrics found, sing it your way.")).toBeVisible();
  await server(page, { t: "player", snapshot: { entries: [], current: null, ended: false, lyricOffsetMs: 0 } });
  await expect(page.getByText("Waiting for a song")).toBeVisible();
});

test("Voice and Singer each open their own slider and send its value", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Voice" }).click();
  await page.getByRole("slider", { name: "Your voice volume" }).fill("55");
  await page.getByRole("button", { name: "Singer" }).click();
  await expect(page.getByRole("slider", { name: "Your voice volume" })).toBeHidden();
  const singer = page.getByRole("slider", { name: "Singer: left is the original, right removes the singer" });
  await expect(singer).toHaveValue("100");
  await singer.fill("30");
  await expect.poll(async () => (await sent(page)).filter((m) => m.t === "singer").at(-1)).toEqual({ t: "singer", v: 30 });
  expect(await sent(page)).toContainEqual({ t: "voice", v: 55 });
  await page.reload();
  await joinAs(page);
  expect(await sent(page)).toContainEqual({ t: "voice", v: 55 });
});
```

Run: `cd app && npx playwright test tests/phone.spec.ts`
Expected: the two new tests FAIL (no `.psong`, no Voice button).

- [ ] **Step 3: The link follows the song**

In `app/src/lib/phone/link.svelte.ts`:
- import `Lyrics` with the other types;
- `FromMac` gains:

```ts
  | { t: "clock"; key: number | null; positionMs: number; playing: boolean }
  | { t: "lyrics"; trackId: number; lyrics: Lyrics }
  | { t: "lyricsChanged"; trackId: number }
```

- fields after `snapshot`:

```ts
  clock = $state({ key: null as number | null, positionMs: 0, playing: false, at: 0 });
  lyrics = $state<{ trackId: number; lyrics: Lyrics } | null>(null);
  voice = $state(Number(recall(() => localStorage, "phone.voice") ?? 80));
  current = $derived(this.entryAt(0));
  next = $derived(this.entryAt(1));
```

and private fields `private asked: number | null = null;`, `private sentAt = new Map<string, number>();`, `private later = new Map<string, ReturnType<typeof setTimeout>>();`

- in `receive`, the `player` branch becomes `else if (m.t === "player") { this.snapshot = m.snapshot; this.wantLyrics(); }`, and add:

```ts
    else if (m.t === "clock") this.clock = { key: m.key, positionMs: m.positionMs, playing: m.playing, at: performance.now() };
    else if (m.t === "lyrics") {
      if (m.trackId === this.current?.track.id) this.lyrics = m;
    } else if (m.t === "lyricsChanged") {
      if (m.trackId === this.current?.track.id) {
        this.asked = null;
        this.wantLyrics();
      }
    }
```

- in `stop()` add `this.lyrics = null;` and `this.asked = null;`
- `sendLive` also sends the Voice level:

```ts
  private sendLive() {
    if (!this.mic) return;
    this.send({ t: "live", on: this.live, rate: this.mic.rate });
    this.send({ t: "voice", v: this.voice });
  }
```
- new methods:

```ts
  /** Seconds into the current song, as the computer last said, moving on while it plays. */
  position(): number {
    const c = this.clock;
    if (c.key == null || c.key !== this.current?.key) return 0;
    return (c.positionMs + (c.playing ? performance.now() - c.at : 0)) / 1000;
  }

  setVoice(v: number) {
    this.voice = v;
    keep(() => localStorage, "phone.voice", String(v));
    this.sendSoon({ t: "voice", v });
  }

  setSinger(v: number) {
    this.sendSoon({ t: "singer", v });
  }

  private entryAt(offset: number) {
    const s = this.snapshot;
    return s?.current == null ? null : (s.entries[s.current + offset] ?? null);
  }

  private wantLyrics() {
    const id = this.current?.track.id ?? null;
    if (id == null || id === this.asked) return;
    this.asked = id;
    this.lyrics = null;
    this.send({ t: "lyrics", trackId: id });
  }

  /** Sends a slider's value at most every 100 ms, always ending on the latest one. */
  private sendSoon(m: { t: string; v: number }) {
    clearTimeout(this.later.get(m.t));
    const go = () => {
      this.sentAt.set(m.t, performance.now());
      this.send(m);
    };
    const wait = 100 - (performance.now() - (this.sentAt.get(m.t) ?? -Infinity));
    if (wait <= 0) go();
    else this.later.set(m.t, setTimeout(go, wait));
  }
```

`app/src/lib/art.ts` — pictures from the phone server pass through:

```ts
/** A song picture's URL: the phone server's `/art/` link as it is, a file on this Mac through the asset protocol. */
const pictureUrl = (path: string) => (path.startsWith("/art/") ? path : convertFileSrc(path));
```

and in `artBackground` use `url("${pictureUrl(t.artworkPath)}")`.

- [ ] **Step 4: Lyrics and the tab**

Create `app/src/lib/phone/PhoneLyrics.svelte`:

```svelte
<script lang="ts">
  import type { Lyrics } from "$lib/api";
  import { dotProgress, itemAt, timeline, wordProgress } from "$lib/lyrics/timeline";
  import { t as text } from "$lib/i18n/index.svelte";
  import { fade } from "$lib/motion";
  import UserIcon from "phosphor-svelte/lib/UserIcon";
  import UsersIcon from "phosphor-svelte/lib/UsersIcon";

  let { lyrics, t }: { lyrics: Lyrics; t: number } = $props();

  const PARTS = { m: "phone.partM", f: "phone.partF", both: "phone.partBoth" } as const;
  const items = $derived(timeline(lyrics.lines));
  const i = $derived(itemAt(items, t));
  const now = $derived(items[i]);
  const next = $derived(items.slice(i + 1).find((x) => !x.gap));
</script>

{#if lyrics.source === "none"}
  <p class="now">{text("karaoke.noLyrics")}</p>
{:else if now}
  {#key i}
    <div in:fade>
      {#if now.gap}
        <p class="now dots">{#each [0, 1, 2] as d (d)}<i style:--p={dotProgress(now, d, t)}></i>{/each}</p>
      {:else}
        <div class="v-{now.line.voice ?? 'none'}">
          {#if now.line.voice}
            <span class="part cap">{#if now.line.voice === "both"}<UsersIcon size={14} />{:else}<UserIcon size={14} />{/if}{text(PARTS[now.line.voice])}</span>
          {/if}
          <p class="now">{#each now.line.words as w, j (j)}<span class="w" style:--p={wordProgress(w, t)}>{w.text}</span>{" "}{/each}</p>
        </div>
      {/if}
      {#if next && !next.gap}<p class="next v-{next.line.voice ?? 'none'}">{next.line.text}</p>{/if}
    </div>
  {/key}
{/if}

<style>
  .now { font: 800 26px/1.15 var(--display); letter-spacing: -.02em; text-wrap: balance; }
  .next { margin-top: var(--s2); font-size: 17px; color: var(--muted); }
  .v-m { text-align: left; }
  .v-f { text-align: right; }
  .v-both { text-align: center; }
  .part { display: inline-flex; align-items: center; gap: 4px; margin-bottom: var(--s1); }
  .w { --p: 0; background: linear-gradient(90deg, var(--text) calc(var(--p) * 100%), color-mix(in srgb, var(--text) 40%, transparent) calc(var(--p) * 100%)); -webkit-background-clip: text; background-clip: text; color: transparent; }
  .dots { display: flex; align-items: center; gap: 12px; height: 34px; }
  .dots i { --p: 0; width: 12px; height: 12px; border-radius: 50%; background: var(--text); opacity: calc(.25 + var(--p) * .75); }
</style>
```

Replace `app/src/lib/phone/MicTab.svelte` (the mic button and state line stay as in Task 10, now below the song and lyrics, with the Voice and Singer controls after them):

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { link } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { loudness } from "$lib/format";
  import { slide } from "$lib/motion";
  import Artwork from "$lib/components/Artwork.svelte";
  import PhoneLyrics from "./PhoneLyrics.svelte";
  import MicrophoneSlashIcon from "phosphor-svelte/lib/MicrophoneSlashIcon";
  import MicrophoneStageIcon from "phosphor-svelte/lib/MicrophoneStageIcon";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import SpeakerHighIcon from "phosphor-svelte/lib/SpeakerHighIcon";
  import UserSoundIcon from "phosphor-svelte/lib/UserSoundIcon";
  import WifiSlashIcon from "phosphor-svelte/lib/WifiSlashIcon";

  let now = $state(0);
  let open = $state<"voice" | "singer" | null>(null);
  let singerDraft = $state<number | null>(null);
  const track = $derived(link.current?.track ?? null);
  const upNext = $derived(link.next ? t("phone.upNext", { song: [link.next.track.title, link.next.track.artist].filter(Boolean).join(" · ") }) : "");
  const singer = $derived(singerDraft ?? track?.vocalRemoval ?? 100);

  $effect(() => {
    if (singerDraft === track?.vocalRemoval) singerDraft = null;
  });

  onMount(() => {
    let frame = requestAnimationFrame(function follow() {
      now = link.position();
      frame = requestAnimationFrame(follow);
    });
    return () => cancelAnimationFrame(frame);
  });
</script>

<div class="psong">
  {#if track}
    <Artwork {track} size={48} />
    <div class="grow"><b class="ell">{track.title}</b><span class="muted ell">{track.artist ?? ""}</span></div>
  {:else}
    <span class="th"><MusicNotesIcon size={20} /></span>
    <div class="grow"><b>{t("phone.waitingSong")}</b><span class="muted">{t("phone.addFromSongs")}</span></div>
  {/if}
</div>
<div class="pnext">
  {#key upNext}{#if upNext}<span in:slide={{ y: 4 }}>{upNext}</span><span aria-hidden="true">{upNext}</span>{/if}{/key}
</div>
<div class="plyr">
  {#if track && link.lyrics}<PhoneLyrics lyrics={link.lyrics.lyrics} t={now - (link.snapshot?.lyricOffsetMs ?? 0) / 1000} />{/if}
</div>
<div class="grow"></div>
<button
  class="micbtn"
  class:muted={!link.live}
  disabled={link.reconnecting}
  aria-label={link.live ? t("phone.mute") : t("phone.unmute")}
  style:--lv={link.live ? loudness(link.level) : 0}
  onclick={() => link.toggleLive()}
>
  <span class="ring"></span>
  {#if link.live}<MicrophoneStageIcon size={64} />{:else}<MicrophoneSlashIcon size={64} />{/if}
</button>
<p class="pstate" class:muted={!link.live}>
  {#if link.reconnecting}<WifiSlashIcon size={16} />{t("phone.waitingWifi")}{:else}<span class="dot"></span>{link.live ? t("phone.tapToMute") : t("phone.tapToSing")}{/if}
</p>
<div class="pctrls">
  <button class="pctl glass" aria-pressed={open === "voice"} onclick={() => (open = open === "voice" ? null : "voice")}><SpeakerHighIcon size={16} />{t("phone.voice")}</button>
  <button class="pctl glass" aria-pressed={open === "singer"} onclick={() => (open = open === "singer" ? null : "singer")}><UserSoundIcon size={16} />{t("phone.singer")}</button>
</div>
{#if open === "voice"}
  <label class="pslide glass" transition:slide={{ y: 8 }}>
    <SpeakerHighIcon size={18} />
    <input class="vs" type="range" min="0" max="100" value={link.voice} style:--v="{link.voice}%" aria-label={t("phone.voiceAria")} oninput={(e) => link.setVoice(+e.currentTarget.value)} />
    <output class="num">{link.voice}%</output>
  </label>
{:else if open === "singer"}
  <label class="pslide glass" transition:slide={{ y: 8 }}>
    <UserSoundIcon size={18} />
    <input
      class="vs"
      type="range"
      min="0"
      max="100"
      value={singer}
      style:--v="{singer}%"
      aria-label={t("singer.aria")}
      disabled={!track}
      oninput={(e) => {
        singerDraft = +e.currentTarget.value;
        link.setSinger(singerDraft);
      }}
    />
  </label>
{/if}

<style>
  .psong { display: flex; align-items: center; gap: var(--s3); margin-top: var(--s5); }
  .psong b, .psong .muted { display: block; }
  .psong b { font-weight: 600; font-size: 16px; }
  .th { width: 48px; height: 48px; flex: none; display: grid; place-items: center; border-radius: var(--r-sm); background: var(--raised); }
  .pnext { height: 18px; margin-top: var(--s3); overflow: hidden; white-space: nowrap; font-size: 13px; color: var(--muted); -webkit-mask-image: linear-gradient(90deg, transparent, #000 6%, #000 94%, transparent); mask-image: linear-gradient(90deg, transparent, #000 6%, #000 94%, transparent); }
  .pnext span { display: inline-block; padding-right: var(--s8); animation: ticker 14s linear infinite; }
  @keyframes ticker { to { transform: translateX(-100%); } }
  @media (prefers-reduced-motion: reduce) { .pnext span { animation: none; } }
  .plyr { min-height: 120px; margin-top: var(--s4); }
  .micbtn { position: relative; flex: none; width: 156px; height: 156px; margin: 0 auto; border-radius: 50%; display: grid; place-items: center; background: var(--accent); color: var(--on-accent); }
  .micbtn > :global(svg) { position: relative; }
  .ring { position: absolute; inset: -14px; border-radius: 50%; border: 6px solid var(--accent); opacity: calc(var(--lv, 0) * .9); transform: scale(calc(1 + var(--lv, 0) * .12 * var(--motion))); transition: opacity 120ms linear, transform 120ms linear; }
  .micbtn.muted { background: var(--raised); color: var(--text); }
  .micbtn.muted .ring { opacity: 0; }
  .pstate { display: flex; justify-content: center; align-items: center; gap: var(--s2); margin-top: 36px; font-size: 13px; font-weight: 500; color: var(--muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ready); }
  .pstate.muted .dot { background: var(--faint); }
  .pctrls { display: flex; justify-content: center; gap: var(--s3); margin-top: var(--s4); }
  .pctl { display: inline-flex; align-items: center; gap: 6px; height: 36px; padding: 0 var(--s4); border-radius: 999px; font-size: 13px; font-weight: 500; }
  .pctl[aria-pressed="true"] { color: var(--accent); }
  .pslide { display: flex; align-items: center; gap: var(--s3); height: 44px; margin-top: var(--s3); padding: 0 var(--s4); border-radius: 999px; }
  .pslide > :global(svg) { color: var(--muted); }
  .pslide .vs { flex: 1; width: auto; }
  .pslide output { min-width: 4ch; text-align: right; }
</style>
```

`app/src/styles/base.css`, after the `.vs` hover rule:

```css
@media (hover: none) { .vs::-webkit-slider-thumb { opacity: 1; } }
```

- [ ] **Step 5: Text in six languages**

Add at the end of each locale:

| key | en | ja | ko | zh-Hans | zh-Hant | es |
|---|---|---|---|---|---|---|
| `phone.waitingSong` | Waiting for a song | 曲を待っています | 노래를 기다리는 중 | 正在等待歌曲 | 正在等待歌曲 | Esperando una canción |
| `phone.addFromSongs` | Add a song from Songs. | 「曲」から曲を追加してください。 | 노래 탭에서 노래를 추가하세요. | 从“歌曲”添加一首歌。 | 從「歌曲」新增一首歌。 | Añade una canción desde Canciones. |
| `phone.upNext` | Up next: {song} | 次の曲：{song} | 다음 곡: {song} | 下一首：{song} | 下一首：{song} | A continuación: {song} |
| `phone.voice` | Voice | 声 | 목소리 | 人声 | 人聲 | Voz |
| `phone.voiceAria` | Your voice volume | あなたの声の音量 | 내 목소리 볼륨 | 你的人声音量 | 你的人聲音量 | Volumen de tu voz |
| `phone.singer` | Singer | 歌手 | 가수 | 原唱 | 原唱 | Cantante |
| `phone.partM` | Male part | 男性パート | 남성 파트 | 男声部分 | 男聲部分 | Parte masculina |
| `phone.partF` | Female part | 女性パート | 여성 파트 | 女声部分 | 女聲部分 | Parte femenina |
| `phone.partBoth` | Together | 一緒に | 함께 | 合唱 | 合唱 | Juntos |

(Each cell is the string value, e.g. `"phone.voice": "Voice",` in `en.ts`.)

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd app && npm test && npm run check && npm run check:i18n && npx playwright test`
Expected: all PASS (Task 10's phone tests still pass; 2 new ones pass), 0 svelte-check errors.

- [ ] **Step 7: Isolated check and commit**

Run the isolated check with the phone check for `task-11.png` (grep `phone check OK`).

```bash
git add app/src/lib/phone app/src/lib/art.ts app/src/styles/base.css app/src/lib/i18n app/tests/fake-phone.ts app/tests/phone.spec.ts
git commit -m "feat(app): phone Mic tab with the song, up next, lyrics, Voice and Singer"
```

---

### Task 12: The phone's Effect button — No effect, Karaoke mix, Auto-tune, with a strength slider

**Files:**
- Modify: `app/src/lib/phone/link.svelte.ts`, `app/src/lib/phone/MicTab.svelte`
- Modify: `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`
- Test: `app/tests/phone.spec.ts`

**Interfaces:**
- Consumes: `{t:"effect", kind: "none" | "karaokeMix" | "autoTune", amount: 0–100}` (Task 8; the Mac applies it to that phone's voice before the mix, Task 4); Task 11's `link.sendSoon`, `MicTab.svelte` controls row.
- Produces:
  - `link.effect: { kind: EffectKind; amount: number }` (`export type EffectKind = "none" | "karaokeMix" | "autoTune"`), starting as `{ kind: "none", amount: 50 }` or what this phone last chose (kept in `localStorage` as `phone.effect`); `link.setEffect(kind, amount)` (sent at most every 100 ms, ending on the latest). The effect is sent with every `live` message, so it reaches the Mac after joining, unmuting and reconnecting.
  - The Voice / Singer row gains a third button, Effect; it opens a panel with the three presets (one pressed) and the strength slider (disabled for No effect).
  - i18n keys `phone.effect`, `phone.fxNone`, `phone.fxKaraoke`, `phone.fxAutotune`, `phone.fxStrength`.

- [ ] **Step 1: Write the failing test**

Append to `app/tests/phone.spec.ts`:

```ts
test("Effect picks a preset and its strength, and the phone remembers it", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Effect", exact: true }).click();
  await expect(page.getByRole("slider", { name: "Effect strength" })).toBeDisabled();
  await page.getByRole("button", { name: "Auto-tune" }).click();
  await page.getByRole("slider", { name: "Effect strength" }).fill("60");
  await expect.poll(async () => (await sent(page)).filter((m) => m.t === "effect").at(-1)).toEqual({ t: "effect", kind: "autoTune", amount: 60 });
  await page.reload();
  await joinAs(page);
  expect(await sent(page)).toContainEqual({ t: "effect", kind: "autoTune", amount: 60 });
  await page.getByRole("button", { name: "Effect", exact: true }).click();
  await expect(page.getByRole("button", { name: "Auto-tune" })).toHaveAttribute("aria-pressed", "true");
});
```

Run: `cd app && npx playwright test tests/phone.spec.ts -g Effect`
Expected: FAIL — no Effect button.

- [ ] **Step 2: The link keeps and sends the effect**

In `app/src/lib/phone/link.svelte.ts`:

```ts
export type EffectKind = "none" | "karaokeMix" | "autoTune";

/** The effect this phone chose last time, or none at half strength. */
function savedEffect(): { kind: EffectKind; amount: number } {
  try {
    const e = JSON.parse(localStorage.getItem("phone.effect") ?? "null");
    if (["none", "karaokeMix", "autoTune"].includes(e?.kind) && Number.isInteger(e?.amount)) return e;
  } catch {
    return { kind: "none", amount: 50 };
  }
  return { kind: "none", amount: 50 };
}
```

- field after `voice`: `effect = $state(savedEffect());`
- `sendSoon`'s parameter becomes `m: { t: string; [k: string]: unknown }` (its body is unchanged);
- `sendLive` also sends the effect (after the Voice level, Task 11):

```ts
  private sendLive() {
    if (!this.mic) return;
    this.send({ t: "live", on: this.live, rate: this.mic.rate });
    this.send({ t: "voice", v: this.voice });
    this.send({ t: "effect", ...this.effect });
  }
```

- new method:

```ts
  /** Chooses the voice effect and its strength (0–100); this phone remembers it. */
  setEffect(kind: EffectKind, amount: number) {
    this.effect = { kind, amount };
    keep(() => localStorage, "phone.effect", JSON.stringify(this.effect));
    this.sendSoon({ t: "effect", kind, amount });
  }
```

- [ ] **Step 3: The Effect button and panel**

In `app/src/lib/phone/MicTab.svelte`:
- imports: `MagicWandIcon`, `ProhibitIcon`, `SparkleIcon`, `WaveformIcon` from `phosphor-svelte/lib/…Icon`, and `type EffectKind` from `./link.svelte`;
- `let open = $state<"voice" | "singer" | "effect" | null>(null);` and, in the script:

```ts
  const PRESETS = [
    { kind: "none", label: "phone.fxNone", icon: ProhibitIcon },
    { kind: "karaokeMix", label: "phone.fxKaraoke", icon: WaveformIcon },
    { kind: "autoTune", label: "phone.fxAutotune", icon: SparkleIcon },
  ] as const satisfies readonly { kind: EffectKind; label: string; icon: unknown }[];
```

- a third button at the end of `.pctrls`:

```svelte
  <button class="pctl glass" aria-pressed={open === "effect"} onclick={() => (open = open === "effect" ? null : "effect")}><MagicWandIcon size={16} />{t("phone.effect")}</button>
```

- after the `{:else if open === "singer"}` block, before `{/if}`:

```svelte
{:else if open === "effect"}
  <div class="pfx glass" transition:slide={{ y: 8 }}>
    <div class="presets">
      {#each PRESETS as p (p.kind)}
        <button class="preset" aria-pressed={link.effect.kind === p.kind} onclick={() => link.setEffect(p.kind, link.effect.amount)}><p.icon size={16} />{t(p.label)}</button>
      {/each}
    </div>
    <label class="pslide">
      <MagicWandIcon size={18} />
      <input
        class="vs"
        type="range"
        min="0"
        max="100"
        value={link.effect.amount}
        style:--v="{link.effect.amount}%"
        aria-label={t("phone.fxStrength")}
        disabled={link.effect.kind === "none"}
        oninput={(e) => link.setEffect(link.effect.kind, +e.currentTarget.value)}
      />
      <output class="num">{link.effect.amount}%</output>
    </label>
  </div>
```

- styles:

```css
  .pctrls { flex-wrap: wrap; }
  .pfx { margin-top: var(--s3); padding: var(--s2); border-radius: var(--r-lg); }
  .presets { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--s1); }
  .preset { display: grid; justify-items: center; gap: 2px; min-height: 52px; padding: var(--s2) var(--s1); border-radius: var(--r-sm); font-size: 12px; font-weight: 500; color: var(--muted); text-align: center; }
  .preset[aria-pressed="true"] { color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .pfx .pslide { margin-top: var(--s1); }
```

- [ ] **Step 4: Text in six languages**

| key | en | ja | ko | zh-Hans | zh-Hant | es |
|---|---|---|---|---|---|---|
| `phone.effect` | Effect | エフェクト | 효과 | 效果 | 效果 | Efecto |
| `phone.fxNone` | No effect | エフェクトなし | 효과 없음 | 无效果 | 無效果 | Sin efecto |
| `phone.fxKaraoke` | Karaoke mix | カラオケミックス | 노래방 믹스 | 卡拉 OK 混音 | 卡拉 OK 混音 | Mezcla karaoke |
| `phone.fxAutotune` | Auto-tune | オートチューン | 오토튠 | 自动修音 | 自動修音 | Autoafinación |
| `phone.fxStrength` | Effect strength | エフェクトの強さ | 효과 강도 | 效果强度 | 效果強度 | Intensidad del efecto |

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd app && npm run check && npm run check:i18n && npx playwright test`
Expected: all PASS, 0 svelte-check errors.

- [ ] **Step 6: Isolated check and commit**

Run the isolated check with the phone check for `task-12.png` (grep `phone check OK`).

```bash
git add app/src/lib/phone app/src/lib/i18n app/tests/phone.spec.ts
git commit -m "feat(app): phone voice effects — karaoke mix and auto-tune with a strength slider"
```

---

### Task 13: The phone's Songs and Queue tabs

**Files:**
- Modify: `app/src/lib/phone/link.svelte.ts`, `app/src/lib/phone/phone.css`
- Create: `app/src/lib/phone/SongsTab.svelte`
- Modify: `app/src/routes/phone/+page.svelte`
- Modify: `app/src/lib/i18n/{en,ja,ko,zh-Hans,zh-Hant,es}.ts`
- Modify: `app/tests/fake-phone.ts`
- Test: `app/tests/phone.spec.ts`

**Interfaces:**
- Consumes: `{t:"search", q, imported}` → `{t:"results", q, outcome}`, `{t:"add", trackId, next}`, `{t:"addLink", url, next}`, `{t:"move", key, to}`, `{t:"remove", key}` (Task 7); `QueueList.svelte` (Task 5); `SearchOutcome` from `$lib/api`; existing keys `search.songs`, `search.fromLink`, `search.noMatchTitle`, `search.noMatchBody`, `search.streamingTitle`, `search.streamingTip`, `search.unsupportedTitle`, `search.unsupportedTip`, `adding.fromHost`, `song.playNext`, `song.addToQueueLabel`, `queue.title`, `library.imported`.
- Produces:
  - `link` gains `results: { q: string; outcome: SearchOutcome } | null` (only the answer to the latest search is kept), `search(q)`, `add(trackId, next)`, `addLink(url, next)`, `move(key, to)`, `remove(key)`.
  - `SongsTab.svelte`: search box ("Search songs"), "All songs" for an empty search, rows with Play next / Add to queue (a check for a moment after a tap), a pasted link as one row, refused links and no matches explained.
  - The tab bar: Mic / Songs / Queue.
  - i18n keys `phone.searchPlaceholder`, `phone.allSongs`.

- [ ] **Step 1: A fake search**

In `app/tests/fake-phone.ts`, import `SearchOutcome` with the other types and add under `lyrics`:

```ts
function search(q: string): SearchOutcome {
  if (/^https?:\/\//.test(q)) {
    const host = new URL(q).hostname.replace(/^(www|open)\./, "");
    return /spotify/.test(host) ? { kind: "rejected", streaming: true, host } : { kind: "link", url: q, host };
  }
  const words = q.trim().toLowerCase();
  return { kind: "text", tracks: songs.filter((s) => s.title.toLowerCase().includes(words)), collections: [] };
}
```

and in `FakeSocket.reply`, next to the lyrics line:

```ts
    if (m.t === "search") return this.deliver({ t: "results", q: m.q, outcome: search(String(m.q)) });
```

- [ ] **Step 2: Write the failing tests**

Append to `app/tests/phone.spec.ts`:

```ts
test("Songs lists every song, searches as you type, and queues with Play next or Add to queue", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Songs", exact: true }).click();
  await expect(page.getByText("All songs")).toBeVisible();
  await expect(page.locator(".prow")).toHaveCount(3);
  const box = page.getByPlaceholder("Search songs");
  await box.fill("lem");
  await expect(page.locator(".prow")).toHaveText([/Lemon Skies/]);
  await page.getByRole("button", { name: "Play next" }).click();
  expect(await sent(page)).toContainEqual({ t: "add", trackId: 3, next: true });
  await expect(page.locator(".pib.done")).toHaveCount(1);
  await server(page, { t: "results", q: "old", outcome: { kind: "text", tracks: [], collections: [] } });
  await expect(page.locator(".prow")).toHaveText([/Lemon Skies/]);
  await box.fill("https://youtu.be/abc");
  await expect(page.getByText("Song from youtu.be")).toBeVisible();
  await page.getByRole("button", { name: /Add .* to the queue/ }).click();
  expect(await sent(page)).toContainEqual({ t: "addLink", url: "https://youtu.be/abc", next: false });
  await box.fill("https://open.spotify.com/track/x");
  await expect(page.getByText("spotify.com links can't be downloaded")).toBeVisible();
});

test("Queue shows who added songs, reorders by dragging and removes", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Queue", exact: true }).click();
  const rows = page.locator("[data-qi]");
  await expect(rows).toHaveText([/Lemon Skies/, /Kettle Duet/]);
  await expect(rows.first()).toContainText("added by Ben");
  await rows.filter({ hasText: "Kettle Duet" }).locator(".grip").hover();
  await page.mouse.down();
  const target = (await rows.filter({ hasText: "Lemon Skies" }).boundingBox())!;
  await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 5 });
  await page.mouse.up();
  expect(await sent(page)).toContainEqual({ t: "move", key: 3, to: 1 });
  await rows.filter({ hasText: "Lemon Skies" }).getByRole("button", { name: "Remove" }).click();
  expect(await sent(page)).toContainEqual({ t: "remove", key: 2 });
});
```

Run: `cd app && npx playwright test tests/phone.spec.ts`
Expected: the two new tests FAIL (no Songs or Queue tab).

- [ ] **Step 3: The link searches and queues**

In `app/src/lib/phone/link.svelte.ts`:
- import `SearchOutcome` with the other types and `{ t } from "$lib/i18n/index.svelte"`;
- `FromMac` gains `| { t: "results"; q: string; outcome: SearchOutcome }`;
- field `results = $state<{ q: string; outcome: SearchOutcome } | null>(null);` and `private query: string | null = null;`
- in `receive`: `else if (m.t === "results") { if (m.q === this.query) this.results = m; }`
- methods:

```ts
  /** Asks the computer to search; only the answer to the latest search is kept. */
  search(q: string) {
    this.query = q;
    this.send({ t: "search", q, imported: t("library.imported") });
  }

  add(trackId: number, next: boolean) {
    this.send({ t: "add", trackId, next });
  }

  addLink(url: string, next: boolean) {
    this.send({ t: "addLink", url, next });
  }

  move(key: number, to: number) {
    this.send({ t: "move", key, to });
  }

  remove(key: number) {
    this.send({ t: "remove", key });
  }
```

`app/src/lib/phone/phone.css`: add `--row: 62px;` to `.ptab` (queue rows are phone-sized).

- [ ] **Step 4: Songs tab and the tab bar**

Create `app/src/lib/phone/SongsTab.svelte`:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { link } from "./link.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import Artwork from "$lib/components/Artwork.svelte";
  import ArrowBendDownRightIcon from "phosphor-svelte/lib/ArrowBendDownRightIcon";
  import CheckIcon from "phosphor-svelte/lib/CheckIcon";
  import LinkIcon from "phosphor-svelte/lib/LinkIcon";
  import ListPlusIcon from "phosphor-svelte/lib/ListPlusIcon";
  import MagnifyingGlassIcon from "phosphor-svelte/lib/MagnifyingGlassIcon";
  import MagnifyingGlassMinusIcon from "phosphor-svelte/lib/MagnifyingGlassMinusIcon";
  import MusicNotesIcon from "phosphor-svelte/lib/MusicNotesIcon";
  import WarningIcon from "phosphor-svelte/lib/WarningIcon";

  let query = $state(link.results?.q ?? "");
  let done = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const found = $derived(link.results?.outcome ?? null);

  onMount(() => link.search(query));

  function typed() {
    clearTimeout(timer);
    timer = setTimeout(() => link.search(query), 200);
  }

  /** Queues with `add`, showing a check on the tapped button for a moment. */
  function queued(key: string, add: () => void) {
    add();
    done = key;
    setTimeout(() => {
      if (done === key) done = null;
    }, 1200);
  }
</script>

{#snippet buttons(id: string, title: string, add: (next: boolean) => void)}
  <button class="pib" class:done={done === `${id}:next`} aria-label={t("song.playNext")} onclick={() => queued(`${id}:next`, () => add(true))}>
    {#if done === `${id}:next`}<CheckIcon size={20} />{:else}<ArrowBendDownRightIcon size={20} />{/if}
  </button>
  <button class="pib" class:done={done === `${id}:end`} aria-label={t("song.addToQueueLabel", { title })} onclick={() => queued(`${id}:end`, () => add(false))}>
    {#if done === `${id}:end`}<CheckIcon size={20} />{:else}<ListPlusIcon size={20} />{/if}
  </button>
{/snippet}

<label class="psearch">
  <MagnifyingGlassIcon size={24} />
  <input bind:value={query} placeholder={t("phone.searchPlaceholder")} aria-label={t("phone.searchPlaceholder")} autocomplete="off" oninput={typed} />
</label>
<div class="plist">
  {#if found?.kind === "text"}
    {#if !query.trim()}<p class="cap hstack"><MusicNotesIcon size={14} />{t("phone.allSongs")}</p>{/if}
    {#each found.tracks as track (track.id)}
      <div class="prow">
        <Artwork {track} size={44} />
        <span class="grow"><b class="ell">{track.title}</b><small class="ell">{track.artist ?? ""}</small></span>
        {@render buttons(`song:${track.id}`, track.title, (next) => link.add(track.id, next))}
      </div>
    {:else}
      {#if query.trim()}
        <div class="pempty"><MagnifyingGlassMinusIcon size={32} /><b>{t("search.noMatchTitle", { query: query.trim() })}</b><span>{t("search.noMatchBody")}</span></div>
      {/if}
    {/each}
  {:else if found?.kind === "link"}
    {@const url = found.url}
    <p class="cap hstack"><LinkIcon size={14} />{t("search.fromLink")}</p>
    <div class="prow">
      <span class="th"><LinkIcon size={20} /></span>
      <span class="grow"><b class="ell">{t("adding.fromHost", { host: found.host })}</b></span>
      {@render buttons(`link:${url}`, found.host, (next) => link.addLink(url, next))}
    </div>
  {:else if found?.kind === "rejected"}
    <div class="pempty">
      <WarningIcon size={32} />
      <b>{t(found.streaming ? "search.streamingTitle" : "search.unsupportedTitle", { host: found.host })}</b>
      <span>{t(found.streaming ? "search.streamingTip" : "search.unsupportedTip")}</span>
    </div>
  {/if}
</div>

<style>
  .psearch { flex: none; display: flex; align-items: center; gap: var(--s3); height: 58px; margin-top: var(--s4); padding: 0 var(--s4); border-radius: var(--r-lg); background: var(--surface); border: 1.5px solid var(--line); transition: border-color var(--t) var(--ease), box-shadow var(--t) var(--ease); }
  .psearch:focus-within { border-color: var(--accent); box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 20%, transparent); }
  .psearch > :global(svg) { color: var(--muted); }
  .psearch input { flex: 1; min-width: 0; border: 0; background: none; outline: none; font-size: 18px; }
  .psearch input::placeholder { color: var(--muted); }
  .plist { display: grid; grid-template-columns: minmax(0, 1fr); margin-top: var(--s3); }
  .plist .cap { padding: var(--s3) 0 var(--s1); }
  .prow { display: flex; align-items: center; gap: var(--s3); min-height: 62px; }
  .prow b { display: block; font-weight: 600; }
  .prow small { display: block; color: var(--muted); font-size: 13px; }
  .th { width: 44px; height: 44px; flex: none; display: grid; place-items: center; border-radius: var(--r-sm); background: var(--raised); }
  .pib { width: 44px; height: 44px; flex: none; display: grid; place-items: center; border-radius: 50%; background: var(--raised); }
  .pib.done { color: var(--ready); }
  .pempty { display: grid; justify-items: center; gap: var(--s2); padding: var(--s7) 0; text-align: center; color: var(--muted); }
  .pempty b { color: var(--text); font-size: 16px; }
</style>
```

In `app/src/routes/phone/+page.svelte`:
- imports: `SongsTab` from `$lib/phone/SongsTab.svelte`, `QueueList` from `$lib/components/QueueList.svelte`, `MagnifyingGlassIcon`, `QueueIcon`;
- in the script: `let tab = $state<"mic" | "songs" | "queue">("mic");`
- the `.ptab` and `nav` become:

```svelte
        <div class="ptab">
          {#if tab === "mic"}
            <MicTab />
          {:else if tab === "songs"}
            <SongsTab />
          {:else if link.snapshot}
            <QueueList snapshot={link.snapshot} art={44} nothingBody={t("phone.addFromSongs")} emptyBody={t("phone.addFromSongs")} onMove={(key, to) => link.move(key, to)} onRemove={(key) => link.remove(key)} />
          {/if}
        </div>
        <nav class="ptabs glass">
          <button aria-pressed={tab === "mic"} onclick={() => (tab = "mic")}>{#if link.live}<MicrophoneStageIcon size={24} />{:else}<MicrophoneSlashIcon size={24} />{/if}<span>{t("phone.tabMic")}</span></button>
          <button aria-pressed={tab === "songs"} onclick={() => (tab = "songs")}><MagnifyingGlassIcon size={24} /><span>{t("search.songs")}</span></button>
          <button aria-pressed={tab === "queue"} onclick={() => (tab = "queue")}><QueueIcon size={24} /><span>{t("queue.title")}</span></button>
        </nav>
```

- [ ] **Step 5: Text in six languages**

| key | en | ja | ko | zh-Hans | zh-Hant | es |
|---|---|---|---|---|---|---|
| `phone.searchPlaceholder` | Search songs | 曲を検索 | 노래 검색 | 搜索歌曲 | 搜尋歌曲 | Buscar canciones |
| `phone.allSongs` | All songs | すべての曲 | 모든 노래 | 所有歌曲 | 所有歌曲 | Todas las canciones |

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd app && npm test && npm run check && npm run check:i18n && npx playwright test`
Expected: all PASS (2 new phone tests; the Mac queue tests still pass with the shared list), 0 svelte-check errors.

- [ ] **Step 7: Isolated check and commit**

Run the isolated check with the phone check for `task-13.png` (grep `phone check OK`).

```bash
git add app/src/lib/phone app/src/routes/phone app/src/lib/i18n app/tests/fake-phone.ts app/tests/phone.spec.ts
git commit -m "feat(app): phone Songs and Queue tabs"
```

---

### Task 14: Links and YouTube in the phone's Songs tab, as on the Mac

**Precondition:** Phase 1b Tasks 29 and 32 are committed (Task 32, c6f33da: YouTube's own web search first, yt-dlp as the fallback, plus search-bar suggestions). Step 2 moves the commands' bodies exactly as they are at HEAD; read them first (`grep -n "fn youtube_search\|fn link_preview" -A20 app/src-tauri/src/adding.rs`) and, if they differ from the snippets below, move the HEAD version.

**Files:**
- Modify: `app/src-tauri/src/adding.rs`
- Modify: `app/src-tauri/src/phones/server.rs`, `app/src-tauri/src/phones/mod.rs`
- Modify: `app/src/lib/phone/link.svelte.ts`, `app/src/lib/phone/SongsTab.svelte`
- Modify: `app/tests/fake-phone.ts`
- Test: `app/tests/phone.spec.ts`

**Interfaces:**
- Consumes (real code at HEAD): the command `adding::youtube_search(state, query: String) -> Result<Vec<SearchHit>, AppError>` (async; body `youtube::search(&query).or_else(|_| ytdlp::ensure(&bin_dir).and_then(|bin| preview::search(&bin, &query)))` — the direct web search with the yt-dlp fallback; `youtube_suggestions` is left alone, phones don't show suggestions), `adding::link_preview(state, url, on_update: Channel<LinkPreview>)`, `kara_core::ingest::preview::{SearchHit, LinkPreview}` (`SearchHit` = `url` + flattened `LinkPreview`); TS `SearchHit`, `LinkPreview` from `$lib/api`; `LinkRow.svelte` (`{ preview: LinkPreview | null; host?: string; onAdd: (then: "" | "queue" | "next") => void }`, a skeleton while `preview` is null); `withPreview`, `previewFailed` from `$lib/search`; the Mac's 280 ms wait before searching YouTube; key `search.youtube`.
- Produces:
  - `adding::find_on_youtube(store: &Store, query: &str) -> anyhow::Result<Vec<SearchHit>>` and `adding::preview_link(store: &Store, url: &str, send: impl FnMut(LinkPreview)) -> anyhow::Result<()>` — the commands' blocking bodies, now shared by the commands and the phones.
  - `FromPhone::Youtube { q }` → `ToPhone::Youtube { q, hits: &[SearchHit] }` (an empty list when the search fails, as the Mac shows nothing then); `FromPhone::Preview { url }` → `ToPhone::Preview { url, preview: Option<&LinkPreview> }` once per preview found (quick, then full), or once with `null` when none could be read. Both run off the phone's connection.
  - Phone Songs tab: a pasted link shows the Mac's link row (skeleton, then picture, title, channel · length); tapping it or Add to queue queues it, Play next plays it next. Word searches of 2+ characters also ask YouTube 280 ms after typing stops; its section shows skeletons while waiting, its videos as link rows, and nothing when YouTube found none. "Nothing matches" shows only when neither the library nor YouTube found anything. Only answers for the latest text are kept.

- [ ] **Step 1: Write the failing tests**

In `app/tests/fake-phone.ts`, in `FakeSocket.reply` next to the search line:

```ts
    if (m.t === "youtube")
      return this.deliver({ t: "youtube", q: m.q, hits: m.q === "zzqx" ? [] : [{ url: "https://www.youtube.com/watch?v=made-up", title: "Made-up Clip", channel: "Someone Sings", durationMs: 185_000, thumbnail: null }] });
    if (m.t === "preview")
      return this.deliver({ t: "preview", url: m.url, preview: { title: "Made-up Song", channel: "Someone Else", durationMs: 200_000, thumbnail: null } });
```

In `app/tests/phone.spec.ts`, in the Songs test, replace the two link lines

```ts
  await expect(page.getByText("Song from youtu.be")).toBeVisible();
  await page.getByRole("button", { name: /Add .* to the queue/ }).click();
```

with

```ts
  await expect(page.getByText("Made-up Song")).toBeVisible();
  await page.getByRole("button", { name: "Add to queue", exact: true }).click();
```

and, because a 3-letter search now also shows YouTube rows with their own Play next buttons, scope the song's Play next click: replace `await page.getByRole("button", { name: "Play next" }).click();` with `await page.locator(".prow", { hasText: "Lemon Skies" }).getByRole("button", { name: "Play next" }).click();`

and append:

```ts
test("Songs also finds videos on YouTube, which queue like a pasted link", async ({ page }) => {
  await joinAs(page);
  await page.getByRole("button", { name: "Songs", exact: true }).click();
  const box = page.getByPlaceholder("Search songs");
  await box.fill("clip");
  await expect(page.getByText("Made-up Clip")).toBeVisible();
  await expect(page.getByText("Someone Sings · 3:05")).toBeVisible();
  await expect(page.getByText("Nothing matches “clip”")).toBeHidden();
  await page.getByRole("button", { name: "Play next", exact: true }).click();
  expect(await sent(page)).toContainEqual({ t: "youtube", q: "clip" });
  expect(await sent(page)).toContainEqual({ t: "addLink", url: "https://www.youtube.com/watch?v=made-up", next: true });
  await box.fill("zzqx");
  await expect(page.getByText("Nothing matches “zzqx”")).toBeVisible();
});
```

Run: `cd app && npx playwright test tests/phone.spec.ts -g "Songs"`
Expected: FAIL — no "Made-up Song" or "Made-up Clip".

- [ ] **Step 2: Share the commands' bodies**

In `app/src-tauri/src/adding.rs` (import `kara_core::store::Store`), move the blocking bodies into plain functions and let the commands call them:

```rust
/// The top YouTube videos for `query`: YouTube's own web search, or yt-dlp's when that fails.
pub fn find_on_youtube(store: &Store, query: &str) -> anyhow::Result<Vec<SearchHit>> {
    youtube::search(query).or_else(|_| ytdlp::ensure(&store.bin_dir()).and_then(|bin| preview::search(&bin, query)))
}

/// The top YouTube videos for the search bar's words.
#[tauri::command]
pub async fn youtube_search(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, AppError> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || find_on_youtube(&store, &query).map_err(|e| coded(e, kara_core::problem::Problem::NoSongAtLink)))
        .await
        .map_err(AppError::from)?
        .plain()
}

/// What a link points to, passed to `send`: the site's quick preview when it has one, then the full details.
pub fn preview_link(store: &Store, url: &str, mut send: impl FnMut(LinkPreview)) -> anyhow::Result<()> {
    let url = link::parse_link(url).context(kara_core::problem::Problem::NotALink)?;
    if link::verdict(&url) == LinkVerdict::AudioFile {
        send(preview::file_preview(&url));
        return Ok(());
    }
    let quick = preview::oembed(&url).ok().flatten();
    if let Some(p) = &quick {
        send(p.clone());
    }
    match ytdlp::ensure(&store.bin_dir()).and_then(|bin| preview::probe(&bin, &url)) {
        Ok(full) => {
            send(full);
            Ok(())
        }
        Err(_) if quick.is_some() => Ok(()),
        Err(e) => Err(coded(e, kara_core::problem::Problem::NoSongAtLink)),
    }
}

/// Streams what a pasted link points to: the site's quick preview when it has one, then the full details.
#[tauri::command]
pub async fn link_preview(state: State<'_, AppState>, url: String, on_update: Channel<LinkPreview>) -> Result<(), AppError> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || preview_link(&store, &url, |p| drop(on_update.send(p))))
        .await
        .map_err(AppError::from)?
        .plain()
}
```

(This keeps Task 32's fast search for the Mac and gives it to phones.)

- [ ] **Step 3: Phones ask for them**

`app/src-tauri/src/phones/server.rs`: `FromPhone` gains `Youtube { q: String },` and `Preview { url: String },`.

`app/src-tauri/src/phones/mod.rs`:
- imports add `kara_core::ingest::preview::{LinkPreview, SearchHit}`;
- `ToPhone` gains `Youtube { q: &'a str, hits: &'a [SearchHit] },` and `Preview { url: &'a str, preview: Option<&'a LinkPreview> },`;
- `handle` gains, before `FromPhone::Join { .. } | …`:

```rust
        FromPhone::Youtube { q } => {
            later(app, id, move |app, id| {
                let store = app.state::<AppState>().store.clone();
                let hits = adding::find_on_youtube(&store, &q).unwrap_or_default();
                tell(app, id, encode(&ToPhone::Youtube { q: &q, hits: &hits }));
            });
            Ok(None)
        }
        FromPhone::Preview { url } => {
            later(app, id, move |app, id| {
                let store = app.state::<AppState>().store.clone();
                let found = adding::preview_link(&store, &url, |p| tell(app, id, encode(&ToPhone::Preview { url: &url, preview: Some(&p) })));
                if found.is_err() {
                    tell(app, id, encode(&ToPhone::Preview { url: &url, preview: None }));
                }
            });
            Ok(None)
        }
```

- new function:

```rust
/// Runs a slow lookup for phone `id` away from its connection.
fn later(app: &AppHandle, id: &str, work: impl FnOnce(&AppHandle, &str) + Send + 'static) {
    let (app, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn_blocking(move || work(&app, &id));
}
```

Run: `source "$HOME/.cargo/env" && cargo test -p kara-app && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS; no warnings.

- [ ] **Step 4: Phone**

`app/src/lib/phone/link.svelte.ts`:
- imports: `LinkPreview`, `SearchHit` types from `$lib/api`; `previewFailed`, `withPreview`, `type SearchView` from `$lib/search`;
- `FromMac` gains `| { t: "youtube"; q: string; hits: SearchHit[] } | { t: "preview"; url: string; preview: LinkPreview | null }`;
- fields: `videos = $state<{ q: string; hits: SearchHit[] } | null>(null);`, `pasted = $state<SearchView>({ kind: "none" });`, `private videoQuery: string | null = null;`
- in `receive`:

```ts
    else if (m.t === "youtube") {
      if (m.q === this.videoQuery) this.videos = m;
    } else if (m.t === "preview") this.pasted = m.preview ? withPreview(this.pasted, m.url, m.preview) : previewFailed(this.pasted, m.url);
```

- the `results` branch also asks for a pasted link's preview:

```ts
    else if (m.t === "results") {
      if (m.q !== this.query) return;
      this.results = m;
      const o = m.outcome;
      if (o.kind === "link" && !(this.pasted.kind === "link" && this.pasted.url === o.url)) {
        this.pasted = { kind: "link", url: o.url, host: o.host, preview: null, failed: false };
        this.send({ t: "preview", url: o.url });
      }
    }
```

- method:

```ts
  /** Asks the computer to look on YouTube; only the answer to the latest search is kept. */
  searchVideos(q: string) {
    this.videoQuery = q;
    this.videos = null;
    this.send({ t: "youtube", q });
  }
```

`app/src/lib/phone/SongsTab.svelte`:
- imports: `LinkRow` from `$lib/components/LinkRow.svelte`, `YoutubeLogoIcon`;
- script additions:

```ts
  let videoTimer: ReturnType<typeof setTimeout> | undefined;
  const words = $derived(query.trim().length >= 2 && !/^https?:\/\//i.test(query.trim()));
  const videos = $derived(link.videos?.q === query.trim() ? link.videos.hits : null);
  const nothing = $derived(found?.kind === "text" && !!query.trim() && !found.tracks.length && (!words || videos?.length === 0));

  /** Queues a link for "" or "queue" (at the end) and "next". */
  const addLink = (url: string) => (then: "" | "queue" | "next") => link.addLink(url, then === "next");
```

- `typed()` also asks YouTube:

```ts
  function typed() {
    clearTimeout(timer);
    clearTimeout(videoTimer);
    timer = setTimeout(() => link.search(query), 200);
    if (words) videoTimer = setTimeout(() => link.searchVideos(query.trim()), 280);
  }
```

- in the text branch, replace the `{:else}` part of `{#each found.tracks …}` (from `{:else}` through `{/each}`, the old no-match message) with `{/each}` followed by the no-match and YouTube parts:

```svelte
    {/each}
    {#if nothing}
      <div class="pempty"><MagnifyingGlassMinusIcon size={32} /><b>{t("search.noMatchTitle", { query: query.trim() })}</b><span>{t("search.noMatchBody")}</span></div>
    {/if}
    {#if words && videos?.length !== 0}
      <p class="cap hstack"><YoutubeLogoIcon size={14} />{t("search.youtube")}</p>
      {#each videos ?? [null, null, null] as hit, i (hit?.url ?? i)}
        <LinkRow preview={hit} host="youtube.com" onAdd={(then) => hit && addLink(hit.url)(then)} />
      {/each}
    {/if}
```

- the link branch (from `{:else if found?.kind === "link"}` up to `{:else if found?.kind === "rejected"}`) becomes the Mac's row:

```svelte
  {:else if found?.kind === "link" && link.pasted.kind === "link"}
    <p class="cap hstack"><LinkIcon size={14} />{t("search.fromLink")}</p>
    {#if link.pasted.failed}
      <div class="prow">
        <span class="th"><LinkIcon size={20} /></span>
        <span class="grow"><b class="ell">{t("adding.fromHost", { host: found.host })}</b></span>
        {@render buttons(`link:${found.url}`, found.host, (next) => link.addLink(found.url, next))}
      </div>
    {:else}
      <LinkRow preview={link.pasted.preview} host={found.host} onAdd={addLink(found.url)} />
    {/if}
```

- style: `.plist :global(.lthumb) { width: 96px; }` (the prototype's phone size).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd app && npm run check && npm run check:i18n && npx playwright test`
Expected: all PASS, 0 svelte-check errors.

- [ ] **Step 6: Isolated check and commit**

Run the isolated check with the phone check for `task-14.png` (grep `phone check OK`).

```bash
git add app/src-tauri/src/adding.rs app/src-tauri/src/phones app/src/lib/phone app/tests/fake-phone.ts app/tests/phone.spec.ts
git commit -m "feat(app): pasted links and YouTube results in the phone's Songs tab, as on the Mac"
```

---

### Task 15: README, full check, and the user's checklist

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: README "Phone mics" section and development notes; a message telling the user where the work is and how to try it, with the checklist below. This task starts no app for the user and touches nothing in their checkout.

- [ ] **Step 1: README**

Add after the "Run the app" section of `README.md`:

~~~markdown
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
running app or uses port 1420). `KARA_MIC_BUFFER_MS` (10–60, default 20) sets the shortest mic delay
buffer. Auto-tune adds about 10 ms to a voice, up to about 25 ms for low voices.
~~~

- [ ] **Step 2: Full check**

Run:
```bash
source "$HOME/.cargo/env" && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
cd app && npm test && npm run check && npm run check:i18n && npx playwright test && npm run build
```
Expected: all Rust, vitest and Playwright tests pass; 0 clippy warnings; 0 svelte-check errors; every locale matches; the build succeeds.

Then the isolated check with the phone check for `task-15.png` (grep `phone check OK`), and `git status` in the worktree shows nothing uncommitted.

- [ ] **Step 3: Commit and hand over**

```bash
git add README.md
git commit -m "docs: phone mics in the README"
```

Do not start, stop or restart any app, and don't merge. Tell the user: "Phase 2 is on branch `phase2-phone-mics` in the worktree `/Users/mohaelder/Repos/kara-always-oki-phase2` (your checkout and running app were never touched). To try it, stop your dev app, then either run `npm run tauri:dev` in the worktree's `app/` (it uses your usual library), or merge `phase2-phone-mics` into `phase1b-app` in your checkout and run your dev app there as usual. Then go through the checklist below. When you're done with the worktree: `git worktree remove ../kara-always-oki-phase2`."

## User acceptance checklist (for the user; does not block any task)

Run the Phase 2 code as the hand-over message says (from the worktree, or after merging). Use your usual library, an iPhone and, if you have one, an Android phone on the same Wi-Fi as the Mac, and a song with synced lyrics (the duet file from the Phase 1 checklist helps for item 7). Use the Mac's built-in speakers or wired speakers: Bluetooth output (AirPods and the like) adds 100+ ms, and the mics play on whatever output is the default when the window opens.

1. **First join.** Click the mic pill (`+`): the window shows the QR code, `OKI-` code, "on <your Mac>.local", the warning tutorial, the firewall line and the speaker tip. Scan with the iPhone: the warning page → Show Details → visit this website → Join (code filled in as OKI-…, tick shown) → type a name → Join → Allow microphone → the iOS prompt → the Mic tab. Time it: under 30 s. If macOS asks to allow incoming connections, allow. The Mac shows "<name> joined as Mic 1"; the row's meter moves when you speak; the pill shows 1.
2. **Typing the address.** On another phone, type the address the window shows ("<your Mac>.local") in the browser: it opens the Join page; type the code.
3. **Coming back.** Tap Leave, then Join again: no warning, in a few seconds, name remembered.
4. **Delay.** Record the room with QuickTime on the Mac while tapping the phone's mic with a fingernail; in an audio editor, measure from the tap's direct sound to its sound from the speakers. Under 60 ms. Repeat after restarting with `KARA_MIC_BUFFER_MS=10`, then `=40`; note which sounds best (say if 10 ms drops out).
5. **Two phones, a few minutes.** Both sing through a whole song: no dropouts or clicks, reverb on both, the Mac volume sliders (drag smoothly, no jumping back) and each phone's Voice slider change only that voice; the Singer slider on a phone moves the Mac's and the other phone's.
6. **Voice effects.** On one phone tap Effect: Karaoke mix at 0 % sounds dry, at 100 % gives a clear echo and a warm room; Auto-tune at 100 % pulls a slightly off note to pitch without an audible extra delay, at 30 % only nudges it. Try Auto-tune with a low (male) voice too: it should still land on the note, with the voice only slightly later. Switch between the effects while singing: no clicks. Reload the phone page and rejoin: the effect and the Voice level are still what you chose, on the phone and on the Mac row. The other phone's voice is unaffected.
7. **Feedback.** Hold a phone close to a speaker and raise its volume until it starts to howl: within about a second it drops and the Mac row says "Turned down: too close to the speakers"; move it away and within a few seconds it's back to normal. Singing a long held note at the phone does not turn it down.
8. **Lyrics on the phone.** The words fill in step with the Mac (not ahead of it right after a song starts); the dots count down before lines; "Up next" scrolls the next song; a duet song shows "Male part" / "Female part"; changing Lyrics timing on the Mac moves the phone's too; a song without lyrics says so only after the lookup finished.
9. **Songs and queue from the phone.** Search a library song by part of its title (also a Japanese one): Play next and Add to queue show a check, the Mac toasts "<name> added “…”" and its queue shows "added by <name>". With nothing playing, a song added from a phone starts on the Mac in the karaoke view — also right after launching the Mac app, before anyone has clicked anything in it (if it opens paused instead, say so). Paste a YouTube link: its picture and title appear; Add to queue downloads it (the Mac shows it in Imported) and it joins the queue once ready. Type a few words: a YouTube section appears; tapping one queues it. In Queue, drag to reorder and remove; the Mac follows.
10. **Screen lock.** Lock the phone for 10 s and unlock: at most a short "Reconnecting…", then back; if the mic stopped, the button says "Tap to sing" and one tap brings it back. While joined, the phone doesn't auto-lock; after Leave it can.
11. **Wi-Fi drop.** Turn the phone's Wi-Fi off for 10 s: the banner shows, the Mac row greys out ("Reconnecting…") within a few seconds; Wi-Fi on: back in the same row with the same volume and effect.
12. **Room full.** Join four devices (phones, or Safari tabs on the Mac at the address shown): a fifth sees "This room is full."
13. **Remove, leave for good, quit.** Remove a phone on the Mac: the Mac toasts "<name> removed" and that phone shows "The host ended the session". Close the mic window, then turn a joined phone's Wi-Fi off and leave it off: after about two minutes its row is gone and, with no phone left, the phone page no longer loads. Quit the app with phones joined: they show "The host ended the session" (right away, or within two minutes).
14. **Speakers change.** With phones singing, unplug the USB/Bluetooth speakers or switch the Mac's output: the Mac says "The speakers changed…", and an open mic window shows a new code on the new speakers; phones rejoin with it. Memory use of the app stays flat meanwhile (Activity Monitor).
15. **Window closed.** Close the window with phones joined: they keep singing and the pill counts them. Close it with none joined: the phone page no longer loads.
16. **Mac sleep.** With a phone joined, close the lid for a minute and open it: the phone shows "The host ended the session" (within two minutes); opening the window again shows a new code.
17. **New address.** With the window open, switch the Mac to another Wi-Fi network (or a phone hotspot): within about 5 s the QR code changes; a phone scanning it sees the warning once more, then joins.
18. **Languages and look.** Set a phone to Japanese, then Spanish: the page follows it. Switch the Mac through all six languages with the window open: nothing overflows. The phone follows its own light/dark setting; slider thumbs are visible on the phone; with Reduce Motion on, "Up next" stops scrolling.
