// **When each run's terminal last printed**, read off the terminals Rust holds (`crate::pty::Pane`).
//
// A launch's pane draws one seat's terminal and holds the rest out of sight, so what the pane can hear
// of its own is the seat in front (`./plate`). The other seats' silences are read here instead: the
// time a terminal last wrote something is a fact about the session, kept where the session is
// (`AMB-D-860`), and handed back by `pty_sessions` beside the run the terminal carries a step of.
//
// **One reader, however many panes**: every pane that wants the times listens to the same reading, so
// three launches up is still one `pty_sessions` every few seconds. **It reads only while someone
// listens** — a pane with a seat at work out of sight — and stops when the last one goes, so a face
// with nothing running asks Rust for nothing.
//
// What is read is the terminal's output and nothing else: nothing an agent says about itself, and
// nothing is done about a silence (`AMB-D-748`).
import { useSyncExternalStore } from "react";
import { invoke } from "../core/ipc";
import type { PtySessionDto } from "../bindings/bindings";

/** How often the times are read again while someone listens. A silence is drawn in whole minutes, so
 *  a few seconds late is not something the reader can see. */
export const OUTPUTS_EVERY_MS = 5_000;

const NONE: ReadonlyMap<number, number> = new Map();

let outputs: ReadonlyMap<number, number> = NONE;
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;
let reading = false;

/** **When each run's terminal last printed**, by the run, out of the sessions `sessions`. A run whose
 *  steps have left more than one terminal open goes by the latest of them; a terminal that has printed
 *  nothing, or carries no run, is not in it. */
export function outputsOf(sessions: readonly PtySessionDto[]): ReadonlyMap<number, number> {
  const out = new Map<number, number>();
  for (const one of sessions) {
    if (one.run == null || one.lastOutput == null) continue;
    const had = out.get(one.run);
    if (had === undefined || one.lastOutput > had) out.set(one.run, one.lastOutput);
  }
  return out;
}

async function read(): Promise<void> {
  if (reading) return;
  reading = true;
  try {
    const sessions = await invoke<PtySessionDto[]>("pty_sessions");
    // The last listener may have gone while this was out: nobody is owed the answer.
    if (listeners.size === 0) return;
    outputs = outputsOf(sessions);
    for (const listener of listeners) listener();
  } catch {
    // Outside Tauri, or a reading that failed: the times stand as they were, and the next reading tries again.
  } finally {
    reading = false;
  }
}

/** **Listen to the times**, read again every {@link OUTPUTS_EVERY_MS} until the returned function is
 *  called. The first listener starts the reading and the last one to go stops it. */
export function hearOutputs(listener: () => void): () => void {
  listeners.add(listener);
  if (timer === null) {
    void read();
    timer = setInterval(() => void read(), OUTPUTS_EVERY_MS);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size > 0 || timer === null) return;
    clearInterval(timer);
    timer = null;
    outputs = NONE;
  };
}

function hearNothing(): () => void {
  return () => {};
}

function outputsNow(): ReadonlyMap<number, number> {
  return outputs;
}

/** **When each run's terminal last printed**, by the run, kept read while `hearing`. Not hearing, the
 *  caller adds nothing to the reading, and what it is handed is whatever stands — empty where nobody listens. */
export function useRunOutputs(hearing: boolean): ReadonlyMap<number, number> {
  return useSyncExternalStore(hearing ? hearOutputs : hearNothing, outputsNow);
}
