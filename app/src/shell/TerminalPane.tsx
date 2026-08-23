import { useEffect, useRef, useState } from "react";
import { mountAgentFrame } from "../talk/agent";
import { mountPlate, type Plate } from "../talk/plate";
import type { FrameNames, NamedBy } from "../talk/frames";
import type { PaneStart } from "../talk/terminal";
import type { SessionSaidDto } from "../bindings/bindings";
import { currentLang, t } from "../core/i18n";

/**
 * One slot of the terminal face: a frame, and the terminal in it when there is one.
 *
 * **A frame is a place, so an empty one is not nothing.** It is a slot on this page with a way to
 * open a terminal in it, and it stays a place after the program in it exits — what is on the screen
 * is what a terminal ends with, and taking the pane away would be the app deciding the reader had
 * finished reading it. So this puts the frame up once and then keeps it, whatever happens to the
 * process: what runs in it, what is offered when nothing can be started, and the row a closed frame
 * carries are all the frame's (`../talk/agent`).
 *
 * The pane comes down when the slot stops being on the screen — the page turned, or fewer panes were
 * asked for — and **the terminal does not**: a pane is a drawing of a session, and detaching leaves
 * the session running for whichever slot draws it next (`../talk/terminal`). That is why the slot's
 * session id is handed back up: the frame is what remembers, and this is only what draws.
 */
export function TerminalPane({
  frame, names, start, autoStart, focused,
  onOpened, onSaid, onClosed, onName, onFocus, onWaiting,
}: {
  /** Which of the arrangement's places this is (`../talk/layout`). */
  frame: string;
  /** What every frame is called, so a naming from anywhere reaches this row. */
  names: FrameNames;
  /** Which terminal to draw here, and where to start one. */
  start: PaneStart;
  /** True for the slot that puts a terminal up without being asked — the one the face comes up with,
   *  and the one a person has just pressed the way in on. */
  autoStart: boolean;
  focused: boolean;
  onOpened: (frame: string, session: string, folder: string | null) => void;
  onSaid: (statement: SessionSaidDto) => void;
  onClosed: (session: string) => void;
  onName: (frame: string, name: string, by: NamedBy) => void;
  onFocus: (frame: string) => void;
  /** Whether a turn is standing in this pane. The face gathers them: behind the ledger no label can
   *  be seen at all, so what the shell badges is the face and not a pane (`./terminalBadge`). */
  onWaiting: (frame: string, waiting: boolean) => void;
}) {
  const paneRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  const plateRef = useRef<Plate | null>(null);
  // Once a terminal has been asked for here it stays asked for: a slot whose program exited keeps the
  // frame, and a person who pressed the button once has not un-pressed it.
  const [running, setRunning] = useState(autoStart);
  // The fact of the program having exited, which the screen cannot show on its own — what a finished
  // shell leaves behind looks exactly like one waiting to be typed at.
  const [ended, setEnded] = useState(false);

  // What the face wants done with what happens here, read at the moment it happens. The pane is put up
  // once and lives longer than any one render, so the effect below must not be re-run to see a newer
  // callback — that would take the terminal down to learn something it could have been told.
  const on = useRef({ onOpened, onSaid, onClosed, onName, onWaiting });
  on.current = { onOpened, onSaid, onClosed, onName, onWaiting };

  useEffect(() => {
    if (!running) return;
    const host = paneRef.current;
    const label = labelRef.current;
    if (!host || !label) return;
    let taken = false;
    let detach: (() => void) | null = null;
    setEnded(false);
    // The line above the pane. It holds what is known about the session running there for as long as
    // it runs, which is the same line the split-out window draws (`../talk/plate.ts`).
    const plate = mountPlate(
      label,
      currentLang,
      (waiting) => on.current.onWaiting(frame, waiting),
      frame,
    );
    plateRef.current = plate;
    void mountAgentFrame(host, currentLang(), {
      opened: (session, startedAt, where) => {
        plate.opened(session, startedAt);
        // Where the terminal actually runs, which is not always the folder this slot was handed: the
        // first frame on a page has none until somebody chooses one, and what they chose is what
        // settles the page (`../talk/layout`).
        on.current.onOpened(frame, session, where ?? start.cwd ?? null);
      },
      said: (statement) => {
        plate.said(statement);
        on.current.onSaid(statement);
      },
      closed: (session) => {
        plate.closed(session);
        setEnded(true);
        on.current.onClosed(session);
      },
      // The window's own title is not the pane's to say — this window is the board. The name goes to
      // the store, and what draws it is the line above the pane.
      name: (text, by) => on.current.onName(frame, text, by),
    }, "termface__pane", start)
      .then((take) => {
        // Taken away while the host was still answering. Detaching leaves the terminal running for
        // whatever draws it next, which is exactly what a pane that never got shown should do.
        if (taken) take();
        else detach = take;
      })
      .catch(() => {});
    return () => {
      taken = true;
      detach?.();
      plate.stop();
      plateRef.current = null;
      // A pane that has gone is not a turn that has been taken: what it was waiting on is still
      // waiting, but nothing on this screen can be looked at for it any more.
      on.current.onWaiting(frame, false);
    };
    // Only `running` is a reason to do any of this again. `start` and `frame` are what this pane *is*
    // — a change of either would be a different pane, and the face gives that one a different key.
  }, [running]);

  // A naming reaches every row, not only the one it happened in: the rail renames a pane that is not
  // the one being worked in, and the row above that pane is where the answer shows.
  useEffect(() => {
    plateRef.current?.named(names);
  }, [names]);

  return (
    <div
      className={`slot${focused ? " slot--focused" : ""}`}
      onMouseDown={() => onFocus(frame)}
    >
      {running
        ? (
          <>
            <div ref={labelRef} />
            {ended && <span className="termface__note">{t("face.ended")}</span>}
            <div className="termface__face" ref={paneRef} />
          </>
        )
        : (
          <button className="slot__open" onClick={() => setRunning(true)}>
            {t("face.open")}
          </button>
        )}
    </div>
  );
}
