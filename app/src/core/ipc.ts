// THE IPC SEAM (front perf instrumentation).
//
// Every Tauri command goes through this one place, which times each round trip and counts the
// invokes fired per frame (roughly, per user action). Over budget gets a console.warn (budget bust);
// verbose logs every invoke at debug. It is the front-end counterpart to core's per-layer perf
// instrumentation, gated by `config.perf_log` (off/budget-only/verbose) as handed to the webview: an
// explicit value wins, otherwise a dev build defaults to on (budget-only, via import.meta.env.DEV),
// matching core's default on this channel. When off, no measurement is paid for and the invoke goes
// straight through.
//
// The invariant is that all IPC passes here: never import `@tauri-apps/api/core` directly, always
// use this `invoke`. That invariant carries a second job — spotting a store this build has fallen
// behind (`format_ahead`, via `noteInvokeFailure`).

import { noteInvokeFailure } from "./formatAhead";

export type PerfMode = "off" | "budget-only" | "verbose";

// Budgets, aligned with core::perf. The front end pays for the IPC round trip and serialisation too,
// so these are a little looser than the 50ms budget for a command on its own.
const SLOW_INVOKE_MS = 80; // WARN once a single round trip exceeds this (50ms command budget plus round-trip slack).
const FANOUT_BUDGET = 8; // WARN once one frame fires more invokes than this (an N+1 invoke).

// Dev builds default to on (budget-only), release builds to off. An explicit config overrides this through applyPerfConfig.
let mode: PerfMode = import.meta.env.DEV ? "budget-only" : "off";

/**
 * Apply `config.perf_log` (an explicit value, or null when unset) to the front-end gate. An explicit
 * value (off/budget-only/verbose) wins; otherwise it falls back to the dev-build default of on
 * (budget-only). Called on every snapshot load.
 */
export function applyPerfConfig(perfLog: string | null | undefined): void {
  mode =
    perfLog === "off" || perfLog === "budget-only" || perfLog === "verbose"
      ? perfLog
      : import.meta.env.DEV
        ? "budget-only"
        : "off";
}

/** The current front-end instrumentation level (for tests and observation). */
export function perfMode(): PerfMode {
  return mode;
}

// Count the invokes fired within a frame and judge them at the frame boundary (rAF). A synchronous
// fan-out — the invokes one action sets off — approximates "how many per action", which is how an
// N+1 invoke shows itself.
let frameCount = 0;
let frameScheduled = false;
function noteFanout(): void {
  frameCount++;
  if (frameScheduled) return;
  frameScheduled = true;
  const flush = () => {
    const n = frameCount;
    frameCount = 0;
    frameScheduled = false;
    if (mode === "off") return;
    if (n > FANOUT_BUDGET) {
      console.warn(`[perf] invoke fan-out ${n} > ${FANOUT_BUDGET} in one frame (N+1 invoke?)`);
    } else if (mode === "verbose") {
      console.debug(`[perf] invoke fan-out ${n} in one frame`);
    }
  };
  if (typeof requestAnimationFrame !== "undefined") requestAnimationFrame(flush);
  else setTimeout(flush, 0);
}

/**
 * The single entry point for every Tauri command. It imports core lazily (so nothing imports it
 * outside Tauri; callers have already gated on inTauri) and times the round trip. With perf on, a
 * slow round trip is WARNed and verbose logs every invoke at debug; with perf off, neither the
 * timing nor the fan-out count is paid for and the invoke passes straight through. A rejection is
 * **rethrown, never swallowed**, but on its way out it is shown to `noteInvokeFailure`. Because
 * all IPC passes through here, a store that has moved ahead of this build (`format_ahead`) is caught
 * in one place whichever path hit it — read, write or subscription — even if the caller drops the
 * rejection.
 */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  if (mode === "off") {
    return tauriInvoke<T>(cmd, args).catch((e: unknown) => {
      noteInvokeFailure(e);
      throw e;
    });
  }
  noteFanout();
  const t0 = performance.now();
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (e) {
    noteInvokeFailure(e);
    throw e;
  } finally {
    const ms = Math.round(performance.now() - t0);
    if (ms > SLOW_INVOKE_MS) {
      console.warn(`[perf] invoke ${cmd} ${ms}ms > ${SLOW_INVOKE_MS}ms budget`);
    } else if (mode === "verbose") {
      console.debug(`[perf] invoke ${cmd} ${ms}ms`);
    }
  }
}
