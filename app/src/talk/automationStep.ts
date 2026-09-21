// **The steps of a run, as they arrive in the window that draws them.**
//
// A run is a line of steps and each step is a terminal of its own, but it is one pane: the place
// stands still and what is running in it is swapped, because a pane per step would have the page
// rearrange itself under a reader at every report (`AMB-D-939`, `./layout`).
//
// **They arrive as an event and not as an answer**, because of where the two ends are. The press that
// starts a run is on the ledger, and the pane it opens is in the workspace — the same window in one
// shape of the app and the other window in the other (`AMB-D-753`). An answer handed back to whoever
// pressed would reach a screen with no pane to stand it in, so the host tells the window instead
// (`crate::automation`).
import { inTauri } from "../core/snapshot";
import type { AutomationStepOpenDto, AutomationStepRunDto } from "../bindings/bindings";

/** What one step's terminal is opened with (`crate::dto::AutomationStepRunDto`). */
export type StepRun = AutomationStepRunDto;

/** A step of a run, opened — or a run stopped instead, which opens no terminal. */
export type StepOpened = AutomationStepOpenDto;

/**
 * Hear every step of every run, for as long as the returned function is not called.
 *
 * Outside Tauri there is nothing to hear: no run is ever started in the browser fallback, so this
 * answers a stop that does nothing rather than leaving the caller to ask which it is in.
 */
export function onStep(heard: (one: StepOpened) => void): () => void {
  if (!inTauri()) return () => {};
  let gone = false;
  let stop: (() => void) | undefined;
  void import("@tauri-apps/api/event")
    .then(({ listen }) => listen<StepOpened>("automation-step", ({ payload }) => heard(payload)))
    .then((un) => {
      if (gone) un();
      else stop = un;
    })
    // A window that could not be reached hears no steps, which is the whole of what goes wrong: no
    // run is drawn in it, and everything else on the face goes on. It is swallowed rather than left
    // to the console because the bus is the host's and there is nothing this side could do about it.
    .catch(() => {});
  return () => {
    gone = true;
    stop?.();
  };
}
