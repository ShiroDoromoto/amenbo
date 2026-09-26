import { useCallback, useRef, useState } from "react";

// A press that writes is sent once, and a second press before the answer comes back is dropped. Nothing
// else on the screen remembers that a write is on its way, so without this a button pressed twice — or
// pressed while the app was stuck, the presses piling up and going out together once it came free —
// sends the same write twice: two tasks, two identical comments, a move worked out from a position that
// the first move already changed.
//
// The guard is a ref, not state. State only takes effect at the next render, and two presses can both
// arrive before it; the ref is set by the first press itself. `busy` is state as well, for the button to
// draw itself shut — that is what the reader sees, the ref is what holds.

/** Run `fn` unless it is already running, and say whether it started. */
function start(inFlight: { current: boolean }, fn: () => unknown, done: () => void): boolean {
  if (inFlight.current) return false;
  inFlight.current = true;
  let p: Promise<unknown>;
  try {
    // A mutator that returns nothing (a test's stub, say) is over as soon as it returns.
    p = Promise.resolve(fn());
  } catch (e) {
    inFlight.current = false;
    done();
    throw e;
  }
  // `fn` answers for its own failures; the guard only needs to know when it is over.
  void p.then(
    () => { inFlight.current = false; done(); },
    () => { inFlight.current = false; done(); },
  );
  return true;
}

/**
 * One write at a time for one control. `run(fn)` starts `fn` and answers true, or answers false and does
 * nothing while the previous one has not come back. `busy` is true for as long as one is in flight.
 */
export function useSingleFlight(): { busy: boolean; run: (fn: () => unknown) => boolean } {
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const run = useCallback((fn: () => unknown): boolean => {
    const started = start(inFlight, fn, () => setBusy(false));
    if (started) setBusy(true);
    return started;
  }, []);
  return { busy, run };
}

/**
 * One write at a time per key — per task, on a board where every card can be moved on its own and only
 * the same card moved twice is the same write twice.
 */
export function useSingleFlightPerKey<K>(): (key: K, fn: () => unknown) => boolean {
  const inFlight = useRef(new Map<K, { current: boolean }>());
  return useCallback((key: K, fn: () => unknown): boolean => {
    const slot = inFlight.current.get(key) ?? { current: false };
    inFlight.current.set(key, slot);
    return start(slot, fn, () => inFlight.current.delete(key));
  }, []);
}
