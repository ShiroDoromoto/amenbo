// **What Amenbo is doing, in a run's pane, while it carries a built-in out** (`AMB-D-964`).
//
// A built-in is a step Amenbo carries out itself — finding a task, closing it, cutting a worktree —
// so there is no terminal for its pane to show. What stands there instead is one card: which built-in
// it is and the task it is about. It is written over, never added to: the next built-in replaces it,
// and the next agent's step takes the pane back to a terminal (`./WorkspaceFace`). What was done is
// kept on the run and read on the "running" and "history" tabs, not here.
//
// **A built-in set to wait says only that it is waiting, and for what** (`AMB-D-969`): the line and the
// filter it was answered with. What would match is not listed or counted — it is asked again every
// second, and a list would be asked of every task there is.
import type { BuiltinRun } from "../talk/automationStep";
import { t } from "../core/i18n";

export function BuiltinCard({ builtin }: { builtin: BuiltinRun }) {
  return (
    // A status and not an alert: it changes by itself as the run goes on, and each change is worth
    // hearing without taking the reader away from what they were doing.
    <div className="slot__builtin" role="status" aria-live="polite">
      <span className="slot__builtin-state">
        {t(builtin.waiting ? "auto.run.waitingForTask" : builtin.finished ? "face.builtinDone" : "face.builtinDoing")}
      </span>
      <strong className="slot__builtin-name">{builtin.name}</strong>
      {builtin.waiting && builtin.looksFor !== undefined && (
        <code className="slot__builtin-filter">{builtin.looksFor}</code>
      )}
      {!builtin.waiting && builtin.task !== undefined && (
        <span className="slot__builtin-task">
          <span className="slot__builtin-ref">{builtin.task.ref}</span>
          <span className="slot__builtin-title">{builtin.task.title}</span>
        </span>
      )}
    </div>
  );
}
