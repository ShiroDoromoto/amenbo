// **What the build screens say about saving** (`AMB-D-1005`). Every field writes on the spot
// (`AMB-D-32`), so without this a reader cannot tell a field still being edited from one already
// written. The automation's screen and the action's screen say it the same way, in two places:
//
// - **On the head, always.** It says the definition's saved state (`SavedState`): nothing saved yet,
//   changes not saved, or the version saved last and when. The writes land in the draft, and the
//   screen's own "Save" press is what makes it a version. A refused write turns it red with core's
//   reason, until the next write lands.
// - **Beside the field, for a moment.** "Written" stands next to the field the write came from for two
//   seconds, then goes.
//
// The lists say the saved state too, shorter (`SavedMark`): the version saved last, and a mark where the
// draft holds more — a draft left unsaved does not launch, so the list is where it is noticed.
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

/** How long "Written" stands beside a field. */
export const MARK_MS = 2000;

/** One "Written" beside a field: where it stands, in the window's coordinates. */
type Mark = { key: number; left: number; top: number; inside: boolean };

/** The handlers the screen's root takes, so each event notes the field it is on. */
type Capture = {
  onFocusCapture: (e: SyntheticEvent) => void;
  onBlurCapture: (e: SyntheticEvent) => void;
  onClickCapture: (e: SyntheticEvent) => void;
  onChangeCapture: (e: SyntheticEvent) => void;
  onKeyDownCapture: (e: SyntheticEvent) => void;
};

/** "Written" beside this field: to its right, or just inside its right edge where the window ends. */
function markBeside(field: Element, key: number): Mark {
  const box = field.getBoundingClientRect();
  const inside = box.right + 80 > window.innerWidth;
  return { key, left: inside ? box.right - 6 : box.right + 6, top: box.top + box.height / 2, inside };
}

/** When a version was saved, as the head says it — it may be another day's. */
function savedWhen(at: string): string {
  return new Date(at).toLocaleString(dateLocale(), {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** **A definition's saved state**: the version saved last, if any, and whether the draft holds more. */
export type SavedState = { saved?: { version: number; savedAt: string }; unsaved: boolean };

/** The head's line for a definition's saved state. */
function stateLine(state: SavedState): string {
  if (state.saved === undefined) return t("auto.saved.never");
  if (state.unsaved) return t("auto.saved.unsaved");
  return tf("auto.saved.version", { version: state.saved.version, time: savedWhen(state.saved.savedAt) });
}

/**
 * **A definition's saved state on a list row**: the version saved last, and "Unsaved" where the draft
 * holds more. Nothing for a definition with nothing saved and nothing written.
 */
export function SavedMark({ saved, unsaved }: { saved?: { version: number }; unsaved: boolean }) {
  if (saved === undefined && !unsaved) return null;
  return (
    <span className="savedmark">
      {saved !== undefined && tf("auto.saved.listVersion", { version: saved.version })}
      {unsaved && <span className="savedmark__unsaved">{t("auto.saved.listUnsaved")}</span>}
    </span>
  );
}

/**
 * The head's line and the marks beside the fields, for one build screen. The screen spreads `capture`
 * on its root, draws `head` on its head and `marks` anywhere.
 *
 * `state` is the definition's saved state, `null` until it is read.
 */
export function useSaved(state: SavedState | null): {
  capture: Capture;
  head: ReactNode;
  marks: ReactNode;
} {
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
        ? tf("auto.saved.writeFailed", { reason: failed })
        : state === null
          ? ""
          : stateLine(state)}
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
              {t("auto.saved.written")}
            </span>
          )),
          document.body,
        );

  return { capture, head, marks: drawn };
}
