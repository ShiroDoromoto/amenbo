// Noticing that the store has moved ahead of this build.
//
// A GUI that stays open can have its store carried forward by another process running a newer version (a newer
// CLI or GUI). Every open then fails hard with `format_ahead` — and missing that means quietly showing stale
// data while updates stop arriving, which is worse than crashing.
//
// So the detection sits at a single seam: `invoke` in `ipc.ts`. Every Tauri command passes through it, so
// whichever path is overtaken — a read, a write, a subscription — it lands here. Once the flag is raised it is
// never lowered: that the store is past this build's ceiling is a fact only a restart can change.

import type { ErrorCode } from "./errorCodes";

/** The stable code core's version gate (`ensure_format_supported`) fails with. */
const FORMAT_AHEAD: ErrorCode = "format_ahead";

let ahead = false;
const listeners = new Set<() => void>();

/** If `invoke` rejected with `format_ahead`, raise "overtaken" — and leave it raised. */
export function noteInvokeFailure(e: unknown): void {
  if (ahead) return;
  if (typeof e !== "object" || e === null) return;
  if ((e as { code?: unknown }).code !== FORMAT_AHEAD) return;
  ahead = true;
  for (const fn of listeners) fn();
}

/** Is the store past what this build supports — that is, should we fall to the restart screen? */
export function isFormatAhead(): boolean {
  return ahead;
}

/** Subscribe to the detection, returning an unsubscribe. If the flag is already up, the callback fires at once. */
export function subscribeFormatAhead(fn: () => void): () => void {
  if (ahead) fn();
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

/** Tests only: reset the module's state. */
export function resetFormatAheadForTest(): void {
  ahead = false;
  listeners.clear();
}
