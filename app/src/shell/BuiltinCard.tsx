// **What Amenbo is doing, in a run's pane, while it carries a built-in out** (`AMB-D-964`).
//
// A built-in is a step Amenbo carries out itself — finding a task, closing it, cutting a worktree —
// so there is no terminal for its pane to show. What stands there instead is one card, and it says
// where the built-in has got to by marks rather than sentences: a turning mark while Amenbo is at it
// or waiting, a tick once it is done. Which built-in it is and the task it is about are the row's to
// say (`../talk/nameplate`), so the card does not say them again. It is written over, never added to:
// the next built-in replaces it, and the next agent's step takes the pane back to a terminal
// (`./WorkspaceFace`). What was done is kept on the run and read on the "running" and "history" tabs,
// not here.
//
// **A built-in set to wait shows what it waits for** (`AMB-D-969`), as the chips the placement's panel
// answered it with. What would match is not listed or counted — it is asked again every second, and a
// list would be asked of every task there is.
//
// **A built-in that has been carried out shows the way out it left by** (`AMB-T-5506`), as the mark the
// picture draws that way out with: the run goes on from there, and which way it went is what a reader
// watching the pane asks next.
//
// **The built-in that waits shows when its time comes** (`AMB-D-983`), down to the second. It has no
// terminal, and the moment it goes on is the one thing a reader watching it wants to know.
import type { BuiltinRun } from "../talk/automationStep";
import { exactLabel, t, tf } from "../core/i18n";
import { ExitMark, FilterChips } from "../screens/automationParts";

export function BuiltinCard({ builtin }: { builtin: BuiltinRun }) {
  const word = t(builtin.finished ? "auto.run.completed" : builtin.waiting ? "auto.run.taskWait" : "auto.run.running");
  return (
    // A status and not an alert: it changes by itself as the run goes on, and each change is worth
    // hearing without taking the reader away from what they were doing.
    <div className="slot__builtin" role="status" aria-live="polite">
      {builtin.finished ? (
        <span className="slot__builtin-done" role="img" aria-label={word}>✓</span>
      ) : (
        <span className="slot__builtin-spin" role="img" aria-label={word} />
      )}
      {builtin.waiting && builtin.looksFor !== undefined && <FilterChips expression={builtin.looksFor} />}
      {!builtin.finished && builtin.heldUntil !== undefined && (
        <span className="slot__builtin-until">{tf("auto.run.heldUntil", { at: exactLabel(builtin.heldUntil) })}</span>
      )}
      {builtin.finished && builtin.exitName !== undefined && (
        <ExitMark name={builtin.exitName} builtin={builtin.key} />
      )}
    </div>
  );
}
