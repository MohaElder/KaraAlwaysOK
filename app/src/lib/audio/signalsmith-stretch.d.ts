declare module "signalsmith-stretch" {
  export interface StretchNode extends AudioWorkletNode {
    start(when?: number): Promise<unknown>;
    stop(when?: number): Promise<unknown>;
    schedule(change: { output?: number; active?: boolean; semitones?: number; rate?: number }): Promise<unknown>;
    latency(): Promise<number>;
  }
  export default function SignalsmithStretch(ctx: BaseAudioContext, options?: AudioWorkletNodeOptions): Promise<StretchNode>;
}
