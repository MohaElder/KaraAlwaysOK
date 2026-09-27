import type { AppError } from "$lib/api";
import { DICTS, t, type Key } from "./index.svelte";

/** An error from the Rust side in the current language; a code this build doesn't know, or none at all, gets the generic text. */
export function say(e: unknown): string {
  const problem = (e as Partial<AppError> | null)?.problem;
  const key = `problem.${problem}`;
  return problem && key in DICTS.en ? t(key as Key) : t("problem.unknown");
}
