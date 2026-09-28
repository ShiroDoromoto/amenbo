// **What the build screens say about saving** (`AMB-D-1005`). Nothing there has a Save press — every
// field writes on the spot (`AMB-D-32`) — so without this a reader cannot tell a field still being
// edited from one already written. The automation's screen and the action's screen say it the same
// way, in two places:
//
// - **On the head, always.** When the last write landed, as "Saved · <time>". Before the first write
//   it says that editing saves on the spot, which is what a reader looking for a Save press needs to
//   know. A refused write turns it red with core's reason, until the next write lands.
// - **Beside the field, for a moment.** "Saved" stands next to the field the write came from for two
//   seconds, then goes.
//
// **Which field a write came from is read off the event that made it.** Every write on a definition is
// sent from a handler — a box losing the caret, a pulldown changed, a press — and core tells its start
// before the call returns (`../core/automationSave`). So the screen notes the field
// each event is on as it passes down (the capture phase, which runs ahead of the handler), and a write
// that starts during that event came from that field. The note is dropped once the event is over, so
// a write started by nothing a reader did is marked on the head only. Events from the panel reach the
// screen too: it is drawn through a portal, and React carries events up its own tree.
import { useEffect, useRef, useState, type ReactNode, type SyntheticEvent } from "react";
import { createPortal } from "react-dom";
import { onAutomationSave } from "../core/automationSave";
import { dateLocale, errText, t, tf } from "../core/i18n";

/** How long "Saved" stands beside a field. */
export const MARK_MS = 2000;

/** One "Saved" beside a field: where it stands, in the window's coordinates. */
type Mark = { key: number; left: number; top: number; inside: boolean };

/** The handlers the screen's root takes, so each event notes the field it is on. */
type Capture = {
  onFocusCapture: (e: SyntheticEvent) => void;
  onBlurCapture: (e: SyntheticEvent) => void;
  onClickCapture: (e: SyntheticEvent) => void;
  onChangeCapture: (e: SyntheticEvent) => void;
  onKeyDownCapture: (e: SyntheticEvent) => void;
};

/** "Saved" beside this field: to its right, or just inside its right edge where the window ends. */
function markBeside(field: Element, key: number): Mark {
  const box = field.getBoundingClientRect();
  const inside = box.right + 80 > window.innerWidth;
  return { key, left: inside ? box.right - 6 : box.right + 6, top: box.top + box.height / 2, inside };
}

/** The time a write landed, as the head says it. */
function clock(at: Date): string {
  return at.toLocaleTimeString(dateLocale(), { hour: "2-digit", minute: "2-digit" });
}

/**
 * The head's line and the marks beside the fields, for one build screen. The screen spreads `capture`
 * on its root, draws `head` on its head and `marks` anywhere.
 */
export function useSaved(): { capture: Capture; head: ReactNode; marks: ReactNode } {
  const [at, setAt] = useState<Date | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [marks, setMarks] = useState<Mark[]>([]);
  // The field the event going on now is on, if any, and which field each write still out came from.
  const touched = useRef<Element | null>(null);
  const from = useRef(new Map<number, Element>());
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());

  useEffect(() => {
    const later = (fn: () => void, ms: number) => {
      const timer = setTimeout(() => {
        timers.current.delete(timer);
        fn();
      }, ms);
      timers.current.add(timer);
    };
    const stop = onAutomationSave((news) => {
      if (news.state === "start") {
        if (touched.current !== null) from.current.set(news.id, touched.current);
        return;
      }
      const field = from.current.get(news.id);
      from.current.delete(news.id);
      if (news.state === "failed") {
        setFailed(errText(news.error));
        return;
      }
      setAt(new Date());
      setFailed(null);
      // A field the write took away — a row deleted, a dialog closed — has nowhere to stand beside.
      if (field === undefined || !field.isConnected) return;
      const mark = markBeside(field, news.id);
      setMarks((now) => [...now, mark]);
      later(() => setMarks((now) => now.filter((one) => one.key !== mark.key)), MARK_MS);
    });
    const pending = timers.current;
    return () => {
      stop();
      for (const timer of pending) clearTimeout(timer);
      pending.clear();
    };
  }, []);

  const note = (e: SyntheticEvent) => {
    if (!(e.target instanceof Element)) return;
    touched.current = e.target;
    // Once the event is over. Not a microtask: React hears the capture and the bubble as two
    // listeners, and a microtask would run between them — before the handler that writes.
    setTimeout(() => {
      if (touched.current === e.target) touched.current = null;
    }, 0);
  };
  const capture: Capture = {
    onFocusCapture: note,
    onBlurCapture: note,
    onClickCapture: note,
    onChangeCapture: note,
    onKeyDownCapture: note,
  };

  const head = (
    <span className={failed === null ? "actsaved" : "actsaved actsaved--failed"} role="status">
      {failed !== null
        ? tf("auto.saved.failed", { reason: failed })
        : at !== null
          ? tf("auto.saved.at", { time: clock(at) })
          : t("auto.saved.onTheSpot")}
    </span>
  );

  const drawn =
    marks.length === 0
      ? null
      : createPortal(
          marks.map((one) => (
            <span
              key={one.key}
              className={one.inside ? "actsaved__mark actsaved__mark--inside" : "actsaved__mark"}
              style={{ left: one.left, top: one.top }}
              aria-hidden="true"
            >
              {t("auto.saved.mark")}
            </span>
          )),
          document.body,
        );

  return { capture, head, marks: drawn };
}
