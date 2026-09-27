import SignalsmithStretch, { type StretchNode } from "signalsmith-stretch";

/** Shifts the key of everything connected to `input`; at 0 semitones it is bypassed so the original plays untouched. */
export class KeyShift {
  readonly input: GainNode;
  /** False once the key shifter failed to start on this machine. */
  works = true;
  private stretch: Promise<StretchNode> | null = null;
  private target: AudioNode;
  private semitones = 0;
  private lag = 0;

  constructor(
    private ctx: AudioContext,
    private onBroken: () => void,
  ) {
    this.input = ctx.createGain();
    this.target = ctx.destination;
    this.input.connect(this.target);
  }

  /** Seconds the shifted sound trails the song clock. */
  get latency(): number {
    return this.semitones === 0 ? 0 : this.lag;
  }

  async set(semitones: number) {
    if (!this.works || semitones === this.semitones) return;
    this.semitones = semitones;
    let stretch: StretchNode | null = null;
    if (semitones !== 0 || this.stretch) {
      try {
        stretch = await (this.stretch ??= this.create());
      } catch {
        if (!this.works) return;
        this.works = false;
        this.stretch = null;
        this.semitones = 0;
        this.route(this.ctx.destination);
        this.onBroken();
        return;
      }
    }
    if (semitones !== this.semitones) return;
    void stretch?.schedule({ semitones });
    this.route(semitones === 0 || !stretch ? this.ctx.destination : stretch);
  }

  private route(target: AudioNode) {
    if (target === this.target) return;
    this.input.disconnect();
    this.input.connect(target);
    this.target = target;
  }

  private async create(): Promise<StretchNode> {
    const hung = new Promise<never>((_, fail) => setTimeout(() => this.ctx.state === "running" && fail(new Error("key shift did not start")), 10_000));
    const node = await Promise.race([SignalsmithStretch(this.ctx), hung]);
    node.connect(this.ctx.destination);
    this.lag = await node.latency();
    await node.start();
    return node;
  }
}
