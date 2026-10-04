// **A run's moves, as one mark and the list behind it** — on its pane (`./TerminalPane`), on its row of
// the "running" tab (`../screens/RunningTab`) and on its band over a build screen
// (`../screens/AutomationHeldBy`), so the three say the same moves the same way.
//
// **The mark is the state the run is in**: two bars while it is going, the triangle while it is held.
// Pressing it opens the list and moves nothing — every move is a pick from the list.
//
// **The list is the moves its state has** (`AMB-D-1002`, `AMB-D-1019`). A run going is paused at the
// end of the action under way, paused once the task it is on is over, or force-cancelled; a held one
// is picked up again or cancelled. The pause once the task is over is this run's alone, and can be
// picked only on a run that takes tasks and has been asked for neither pause yet
// (`pausableBeforeNextTask`). Once picked it cannot be taken back, and says it is waiting; the pause at
// the end of the action can still be picked while it waits, and holds the run sooner. Force cancel
// stands apart in the stop colour, and asks before it acts (`../core/automations`'s `forceCancelRun`).
//
// **It opens and is placed the way the pane's size list is** (`./PaneSize`, `AMB-D-959`): on the
// pointer coming onto the mark or on a press, gone 150ms after the pointer leaves, drawn against the
// window and kept clear of its right edge.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { cancelRun, forceCancelRun, pauseBeforeNextTask, pauseRun, resumeRun } from "../core/automations";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";
import type { AutomationRunCardDto } from "../bindings/bindings";

/** How long the list stays after the pointer leaves it, so a hand crossing the gap to it keeps it. */
const LINGER_MS = 150;

/** The window's edge the list keeps clear of. */
const EDGE = 8;

/** What the list is drawn from: the run, and where it stands. */
export type RunMoves = Pick<
  AutomationRunCardDto,
  "run" | "pauseRequested" | "pauseBeforeNextTask" | "pausableBeforeNextTask"
> & { readonly status: AutomationRunCardDto["status"] };

export function RunActs({
  run,
  busy,
  onPress,
}: {
  run: RunMoves;
  /** A move pressed a moment ago is still on its way, and no other is taken until it lands. */
  busy: boolean;
  /** Make the move picked. The caller says a refusal where it says its own. */
  onPress: (move: () => Promise<unknown>) => void;
}) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);
  const markRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const leaving = useRef<number | null>(null);
  const held = run.status === "paused";

  const stay = () => {
    if (leaving.current !== null) window.clearTimeout(leaving.current);
    leaving.current = null;
  };
  const show = () => {
    stay();
    setOpen(true);
  };
  const leave = () => {
    stay();
    leaving.current = window.setTimeout(() => setOpen(false), LINGER_MS);
  };
  useEffect(() => stay, []);

  useLayoutEffect(() => {
    if (!open) {
      setAt(null);
      return;
    }
    const mark = markRef.current?.getBoundingClientRect();
    const width = listRef.current?.offsetWidth ?? 0;
    if (mark === undefined) return;
    const left = Math.max(EDGE, Math.min(mark.left, window.innerWidth - width - EDGE));
    setAt({ left, top: mark.bottom + 4 });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  const pick = (move: () => Promise<unknown>) => () => {
    setOpen(false);
    onPress(move);
  };

  return (
    <span className="runacts">
      <button
        ref={markRef}
        type="button"
        className="runacts__mark"
        title={t("auto.run.acts")}
        aria-label={t("auto.run.acts")}
        aria-haspopup="menu"
        aria-expanded={open}
        onMouseEnter={show}
        onMouseLeave={leave}
        // It opens and does not toggle, for `PaneSize`'s reason.
        onClick={show}
      >
        <Icon name={held ? "play" : "pause"} />
        <span className="runacts__caret"><Icon name="chevronDown" /></span>
      </button>
      {open && createPortal(
        <div
          ref={listRef}
          className="runacts__list"
          role="menu"
          style={at === null ? { visibility: "hidden", left: 0, top: 0 } : at}
          onMouseEnter={stay}
          onMouseLeave={leave}
        >
          {held ? (
            <>
              <button type="button" role="menuitem" className="runacts__one" disabled={busy} onClick={pick(() => resumeRun(run.run))}>
                <Icon name="play" />
                {t("auto.run.resume")}
              </button>
              <span className="runacts__sep" aria-hidden="true" />
              <button
                type="button"
                role="menuitem"
                className="runacts__one runacts__one--stop"
                disabled={busy}
                onClick={pick(() => cancelRun(run.run))}
              >
                <Icon name="close" />
                {t("auto.run.cancel")}
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                role="menuitem"
                className="runacts__one"
                disabled={busy || run.pauseRequested}
                onClick={pick(() => pauseRun(run.run))}
              >
                <Icon name="pause" />
                {t("auto.run.pauseAfterAction")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="runacts__one"
                disabled={busy || !run.pausableBeforeNextTask}
                onClick={pick(() => pauseBeforeNextTask(run.run))}
              >
                <Icon name="pauseTask" />
                {run.pauseBeforeNextTask ? t("auto.run.pauseAfterTaskWaiting") : t("auto.run.pauseAfterTask")}
              </button>
              <span className="runacts__sep" aria-hidden="true" />
              <button
                type="button"
                role="menuitem"
                className="runacts__one runacts__one--stop"
                disabled={busy}
                onClick={pick(() => forceCancelRun(run.run))}
              >
                <Icon name="stop" />
                {t("auto.run.forceCancel")}
              </button>
            </>
          )}
        </div>,
        document.body,
      )}
    </span>
  );
}
