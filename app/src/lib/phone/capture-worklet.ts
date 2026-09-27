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
