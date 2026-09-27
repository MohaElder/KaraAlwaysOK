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
