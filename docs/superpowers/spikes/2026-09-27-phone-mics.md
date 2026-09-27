# Phone mics spike (Phase 2)

Date: 2026-09-27. One session: an iPhone (iOS 18.6, WebKit browser) and a MacBook Pro on the same Wi-Fi. The receiver was a wry 0.57 / WKWebView window, the same stack KaraAlwaysOK uses, playing through the built-in speakers at 48 kHz. The spike code was thrown away; only these numbers are kept. The user said both paths "felt pretty good".

Delay is measured from the clap's onset in the phone's own mic stream to its onset on the Mac, with the phone's clock synced to the server over WebSocket pings (RTT 7–9 ms). It leaves out the phone's input latency, so real mouth-to-speaker delay is probably 5–15 ms higher.

## Path A: WebRTC into the app's web view

- Connected on the first try (host to host; the Mac offered only mDNS `.local` candidates, which worked here but could fail on Wi-Fi that isolates clients).
- **Delay: median 74.6 ms** (73.7–86.1, n=10).
- WebKit's jitter buffer stayed at ~48 ms and can't be lowered (`jitterBufferTarget` isn't supported in WKWebView). WebAudio output added ~16 ms.
- 0 of ~1,220 packets lost; RTT 6–11 ms.

## Path B: mic PCM over a WebSocket to Rust, played with cpal

- **Delay: ~30 ms to the speakers** (29.0–30.2, n=3), with a 20 ms buffer; ~5 ms to reach the Mac.
- Arrival jitter 4–15 ms, 0 underruns and 0 drops over ~25 s. 768 kbps per phone.

## Recommendation

Path B: about 2.5× faster than A, under the 40 ms low end of the target, with room left for reverb and a larger buffer on poor Wi-Fi. It needs no WebRTC or ICE signaling and doesn't depend on the web view. The mic plays on its own CoreAudio stream from Rust, and the karaoke track keeps playing through WebAudio; the device mixes them.

## What the real app needs

- A self-signed certificate made once and kept (covering the LAN IPs and the `.local` name), so each phone accepts the warning only once.
- A per-phone adaptive buffer (start at 20 ms, grow to ~60 ms on underruns, shrink slowly), gentle resampling for clock drift and 44.1 kHz phones, and a fade on underrun (TCP can stall and then burst after a Wi-Fi retransmit).
- Per-mic gain, a limiter and reverb in Rust.
- Feedback safeguards: the phone's echo cancellation doesn't cancel the Mac's speakers. Cap the gain, add a limiter, suggest keeping phones away from the speakers, and optionally turn a mic down when it starts to howl. Clipping seen during the path A test may have been feedback.
- A screen wake lock on the phone (a locked phone stops sending) and a clear "disconnected" state.
- Before committing: a longer session with singing, other Wi-Fi traffic, and 10 and 40 ms buffers.
