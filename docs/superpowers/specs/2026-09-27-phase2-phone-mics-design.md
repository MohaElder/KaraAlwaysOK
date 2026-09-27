# KaraAlwaysOK Phase 2: Phone mics — Design

Date: 2026-09-27
Status: design approved in conversation; pending written-spec review
Builds on: `2026-09-26-kara-always-oki-design.md` (Phase 1). Its UI rules, languages, copy rules and engineering bar apply here too.
Evidence: `../spikes/2026-09-27-phone-mics.md`.

## 1. What it is

Guests at a karaoke night scan a QR code on the Mac and use their phone as a live mic. No app to install, nothing leaves the local Wi-Fi. Voices play through the Mac's speakers with reverb, right away. From the phone, guests also see the lyrics, adjust their own voice and the singer level, search songs (library and YouTube) and manage the queue.

### Success criteria

- A phone joins in under 30 s the first time (including the one-time certificate warning), and in a few seconds after that.
- Mouth-to-speaker delay is under 60 ms on normal home Wi-Fi (the spike measured ~30 ms plus the phone's own input delay).
- Up to 4 phones sing at once with no audible dropouts on normal Wi-Fi.
- Feedback never becomes a sustained howl: the app turns a howling mic down within about a second.
- Everything a guest can do on the phone matches the approved prototype's phone screens (`docs/prototype/hifi.html`, "Phone preview").

## 2. Experience

### Mac

- **Mic pill** (top right, as in the prototype): a karaoke-mic icon with the number of phones joined; a "+" when none.
- **"Sing into your phone" window** (as in the prototype):
  - Header: "Scan with a phone on the same Wi-Fi."
  - QR code; under it "Or type this code", the code, and "on kara.local" (the Mac's `.local` name).
  - The first-time tutorial for the phone's certificate warning (iPhone: Show Details → visit this website; Android: Advanced → Proceed), and a line saying macOS may ask once to allow incoming connections.
  - A tip: keep phones away from the speakers.
  - The list of joined phones: name, a live level meter, a volume slider, Remove. "Waiting for phones to join…" when empty.
- The singer slider, key, lyric timing and queue stay in sync with every phone.

### Phone

The approved prototype's phone screens:

- **Join**: the code is filled in from the QR code; the guest types a name; Join.
- **Mic permission** step, and a **Mic blocked** screen with how to turn it on (iPhone and Android).
- **Mic tab**:
  - The guest's name in an orange pill; Leave.
  - Now playing (artwork, title, artist), the scrolling "Up next" line (text only, live from the queue), and the lyrics with word-by-word fill, countdowns and duet sides.
  - The big mic button: "Tap to sing" when muted, "Tap to mute" when live; it pulses with the voice level.
  - Voice and Singer buttons below it; tapping one opens its slider.
- **Songs tab**: search the library and YouTube (same fuzzy search and YouTube section as the Mac); Play next / Add to queue. Songs a guest adds show their name in the queue.
- **Queue tab**: Now playing and Up next; drag to reorder, remove.
- **States**: Connecting, Reconnecting ("Wi-Fi dropped. Reconnecting…"), Session ended ("The host ended the session").
- The floating glass tab bar (Mic / Songs / Queue), glass rules and motion as in Phase 1.
- The phone page uses the six languages and follows the phone's language.
- While joined, the page keeps the screen awake (Screen Wake Lock) so a locked screen doesn't cut the mic.

## 3. Architecture

### 3.1 Server (Rust, in the app)

- An HTTPS + WebSocket server on the LAN (axum + rustls), started when the mic window opens or a phone is joined, stopped when the window is closed and no phones are joined.
- **Certificate**: self-signed, created once on first use and kept in the data folder, covering the Mac's LAN addresses and `<name>.local`. If the Mac's address changes, the certificate is re-made to cover it (phones accept the warning again).
- **Join code**: a short code made per session; a phone needs it to join. Closing the session invalidates it.
- **Serving**: the phone page is a static bundle served by this server.
- **Only LAN**: it accepts connections only from private-network addresses. No external servers, no internet traffic.
- One WebSocket per phone carries both control messages (JSON: join, commands, player snapshot, lyrics, events) and mic audio (binary frames).

### 3.2 Audio path

- **Phone**: getUserMedia (echo cancellation on, auto gain off) → an AudioWorklet → 16-bit PCM frames (~5 ms each) → the WebSocket. The phone sends only while unmuted.
- **Mac, per phone**: an adaptive buffer (starts at 20 ms, grows up to ~60 ms after underruns, shrinks slowly when steady), resampling to the output rate (also absorbs clock drift and 44.1 kHz phones), a short fade on underrun instead of a click.
- **Mix**: per-phone gain (the Mac's volume slider × the phone's Voice slider, capped) → sum → a shared reverb → a limiter → its own low-latency output stream (cpal, 128–256 frames). The karaoke track keeps playing through the web view; the device mixes the two streams.
- **Howl control**: per phone, a detector watches for a sustained narrow peak that keeps growing; when it fires, that mic's gain drops until it settles, and the Mac window shows the mic as "turned down".
- **Levels**: per-phone levels go to the Mac window (meters) and back to each phone (the mic button's pulse).
- Up to 4 phones; a 5th sees "This room is full."

### 3.3 Sync with the app

- The app's existing `player` snapshot (queue, current song, singer, key, lyric offset) is mirrored to phones as-is; phone commands (queue, play next, remove, reorder, singer) go through the same Rust commands the Mac UI uses. Lyrics and playback position reach phones so their word fill matches the Mac.
- Guests' names are stored on queue entries they add (the existing `by` field) and shown on both sides.

### 3.4 Errors and edge cases

- Phone loses Wi-Fi: its buffer fades out; the phone shows Reconnecting and rejoins with the same name without re-entering the code; the Mac keeps its row greyed until it's back or removed.
- The Mac removes a phone or ends the session: the phone shows Session ended.
- Mic permission denied: the Mic blocked screen.
- The phone's certificate isn't accepted yet: the page can't load, so the Mac window's tutorial covers it.
- The Mac sleeps: the session ends; phones show Session ended.

## 4. Testing

- Rust unit tests: the adaptive buffer (underrun growth, shrink, drift), resampling, the mixer's gain cap and limiter, howl detection on synthetic signals, join-code checks, certificate reuse.
- Browser tests (the Phase 1 Playwright suite): the phone page's screens and flows against a faked server.
- A manual session on the user's checklist: a few minutes of singing with two phones, measuring delay and listening for dropouts and feedback, with buffer targets of 10, 20 and 40 ms.

## 5. Out of scope

- Cancelling the Mac speakers' sound from the mics (relies on placement plus howl control).
- Recording performances.
- Windows (with the rest of the app, later).
- Joining over the internet.
