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

/** The refusal core wrote, in both languages — what the restart screen shows when restarting is not the answer. */
export type FormatAheadDetail = { ja: string; en: string };

let ahead = false;
let detail: FormatAheadDetail | null = null;
const listeners = new Set<() => void>();

/** If `invoke` rejected with `format_ahead`, raise "overtaken" — and leave it raised. */
export function noteInvokeFailure(e: unknown): void {
  if (ahead) return;
  if (typeof e !== "object" || e === null) return;
  if ((e as { code?: unknown }).code !== FORMAT_AHEAD) return;
  ahead = true;
  // Keep the sentence, not just the fact. Restarting fixes the ordinary case — the new build is already on
  // disk — but when it is not, this refusal is the only thing that names the version that wrote the store and
  // the way back (there is no downgrade, so that way is a restore from the pre-migration backup). Nothing else
  // reaching the screen can say it: every command that could ask is refused by the same gate.
  const { message, message_en } = e as { message?: unknown; message_en?: unknown };
  if (typeof message === "string" && typeof message_en === "string") {
    detail = { ja: message, en: message_en };
  }
  for (const fn of listeners) fn();
}

/** The refusal's own words, or `null` if the rejection carried none. */
export function formatAheadDetail(): FormatAheadDetail | null {
  return detail;
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
  detail = null;
  listeners.clear();
}
