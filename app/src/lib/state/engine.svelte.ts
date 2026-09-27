import { setupEngine, startupProblem, type SetupProgress } from "$lib/api";

class EngineState {
  phase = $state<"starting" | "downloading" | "done" | "ready" | "failed">("starting");
  progress = $state<SetupProgress | null>(null);
  error = $state<unknown>(null);
  private blocked = false;

  /** Gets the singing engine ready; the setup screen shows only while something downloads or fails. */
  async start() {
    if (this.phase === "failed") this.phase = "downloading";
    const problem = await startupProblem();
    if (problem) {
      this.blocked = true;
      this.error = problem;
      this.phase = "failed";
      return;
    }
    if (this.blocked) return location.reload();
    try {
      await setupEngine((p) => {
        this.phase = "downloading";
        this.progress = p;
      });
      if (this.phase === "downloading") {
        this.phase = "done";
        await new Promise((r) => setTimeout(r, 1100));
      }
      this.phase = "ready";
    } catch (e) {
      this.error = e;
      this.phase = "failed";
    }
  }
}

export const engine = new EngineState();
