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
// **A built-in set to wait says so, and what it waits for** (`AMB-D-969`): the line, the chips the
// placement's panel answered it with, and what the reader can do about it (`AMB-T-5753`) — it goes on
// by itself, and the row's pause and stop are the way to have it stop waiting. What would match is not
// listed here; how many are held back, and by what, is (`AMB-D-999`) — counted as the wait begins and
// when the store changes, apart from the look it repeats every second. Each thing holding tasks back
// is a way to where it is cleared: a value to the axis it is closed on, a task or a decision to itself.
//
// **A built-in being carried out says what it does** (`AMB-T-5753`), in the words the library reads
// it with. It has no terminal, and a turning mark alone says only that something is happening.
//
// **A script step's card says which program is running** (`AMB-D-1016`), by its path as the step was
// written with it: the program is the project's own, so there are no library words for what it does.
//
// **A built-in that has been carried out shows the way out it left by** (`AMB-T-5506`), as the mark the
// picture draws that way out with: the run goes on from there, and which way it went is what a reader
// watching the pane asks next.
//
// **The built-in that waits shows when its time comes** (`AMB-D-983`), down to the second. It has no
// terminal, and the moment it goes on is the one thing a reader watching it wants to know.
import type { BuiltinRun } from "../talk/automationStep";
import type { AutomationHeldBackDto } from "../bindings/bindings";
import { exactLabel, formatNumber, t, tf } from "../core/i18n";
import { builtinDoes } from "../core/builtinWords";
import { useHeldBack } from "../core/automations";
import { taskRef } from "../core/idref";
import { useRefNav } from "../core/refNav";
import { Icon } from "../components/Icon";
import { ExitMark, FilterChips } from "../screens/automationParts";

/** `run` is the run the pane draws — what is keeping it waiting is read by it. Null where there is none. */
export function BuiltinCard({ builtin, run = null }: { builtin: BuiltinRun; run?: number | null }) {
  const held = useHeldBack(builtin.waiting ? run : null);
  const word = t(builtin.finished ? "auto.run.completed" : builtin.waiting ? "auto.run.taskWait" : "auto.run.running");
  const does = builtin.finished || builtin.waiting ? null : builtinDoes(builtin.key);
  return (
    // A status and not an alert: it changes by itself as the run goes on, and each change is worth
    // hearing without taking the reader away from what they were doing.
    <div className="slot__builtin" role="status" aria-live="polite">
      {builtin.finished ? (
        <span className="slot__builtin-done" role="img" aria-label={word}>✓</span>
      ) : (
        <span className="slot__builtin-spin" role="img" aria-label={word} />
      )}
      {builtin.waiting && <p className="slot__builtin-word">{t("auto.run.body.taskWait")}</p>}
      {builtin.waiting && builtin.looksFor !== undefined && <FilterChips expression={builtin.looksFor} />}
      {builtin.waiting && held !== null && held.tasks > 0 && <HeldBack held={held} />}
      {builtin.waiting && <p className="slot__builtin-next">{t("auto.run.body.taskWaitNext")}</p>}
      {does !== null && <p className="slot__builtin-does">{does}</p>}
      {!builtin.finished && builtin.program !== undefined && (
        <code className="slot__builtin-program">{builtin.program}</code>
      )}
      {!builtin.finished && builtin.heldUntil !== undefined && (
        <span className="slot__builtin-until">{tf("auto.run.heldUntil", { at: exactLabel(builtin.heldUntil) })}</span>
      )}
      {builtin.finished && builtin.exitName !== undefined && (
        <ExitMark name={builtin.exitName} builtin={builtin.key} />
      )}
    </div>
  );
}

/**
 * **How many tasks the wait cannot take, and what stops them** (`AMB-D-999`) — one line per thing, with
 * how many tasks it holds. A value, a task and a decision are presses to where each is cleared; a start
 * day and an unfinished creation are said, since nothing but time or the task's own pane clears them.
 */
export function HeldBack({ held }: { held: AutomationHeldBackDto }) {
  const nav = useRefNav();
  const count = (n: number) => <span className="slot__held-count">{formatNumber(n)}</span>;
  return (
    <div className="slot__held">
      <p className="slot__held-head">{tf("auto.run.heldBack", { count: held.tasks })}</p>
      <ul className="slot__held-list">
        {held.values.map((v) => (
          <li key={`v${v.dimensionId}=${v.value}`}>
            <button
              type="button"
              className="feed__target"
              title={t("detail.waitingOnValuesHint")}
              disabled={!nav.openDimension}
              onClick={() => nav.openDimension?.(held.project, v.dimensionId)}
            >
              <Icon name="tag" /> {v.value} ({v.axis})
            </button>
            {count(v.count)}
          </li>
        ))}
        {held.blockers.map((b) => (
          <li key={`t${b.id}`}>
            <button
              type="button"
              className="feed__target"
              title={t("detail.blockedByHint")}
              disabled={!nav.selectTask}
              onClick={() => nav.selectTask?.(b.id)}
            >
              <Icon name="blocked" /> {`${taskRef(b.id)} ${b.title}`.trim()}
            </button>
            {count(b.count)}
          </li>
        ))}
        {held.decisions.map((d) => (
          <li key={`d${d.id}`}>
            <button
              type="button"
              className="feed__target"
              title={t("detail.premiseUnsettled")}
              disabled={!nav.selectDecision}
              onClick={() => nav.selectDecision?.(d.id)}
            >
              <Icon name="warning" /> {d.title || t("dec.unknownName")}
            </button>
            {count(d.count)}
          </li>
        ))}
        {held.notStarted > 0 && (
          <li title={held.firstStart === undefined ? undefined : tf("block.notStarted", { date: held.firstStart })}>
            <span><Icon name="hourglass" /> {held.firstStart ?? t("detail.notStarted")}</span>
            {count(held.notStarted)}
          </li>
        )}
        {held.drafts > 0 && (
          <li title={t("block.draft")}>
            <span><Icon name="pencil" /> {t("chip.draft")}</span>
            {count(held.drafts)}
          </li>
        )}
      </ul>
    </div>
  );
}
