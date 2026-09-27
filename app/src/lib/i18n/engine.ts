import type { AppError } from "$lib/api";
import { DICTS, t, type Key } from "./index.svelte";

/** An error from the Rust side in the current language; an unknown code gets a generic text, no code its English text. */
export function say(e: unknown): string {
  const err = e as Partial<AppError> | null;
  if (!err?.problem) return String(err?.message ?? e);
  const key = `problem.${err.problem}`;
  return key in DICTS.en ? t(key as Key) : t("problem.unknown");
}
