import { useEffect, useRef, useState } from "react";
import { mountTerminal, type Place } from "../talk/terminal";
import { mountPlate, type Plate } from "../talk/plate";
import type { FrameNames, NamedBy } from "../talk/frames";
import type { SessionSaidDto } from "../bindings/bindings";
import { t } from "../core/i18n";

/**
 * One slot of the terminal face: a frame, and the terminal in it when there is one.
 *
 * **A frame is a place, so an empty one is not nothing.** It is a slot on this page with a way to
 * open a terminal in it, and it stays a place after the program in it exits — what is on the screen
 * is what a terminal ends with, and taking the pane away would be the app deciding the reader had
 * finished reading it. So this starts a terminal once and then keeps the pane, whatever happens to
 * the process.
 *
 * The pane comes down when the slot stops being on the screen — the page turned, or fewer panes were
 * asked for — and **the terminal does not**: a pane is a drawing of a session, and detaching leaves
 * the session running for whichever slot draws it next (`../talk/terminal`). That is why the slot's
 * session id is handed back up: the frame is what remembers, and this is only what draws.
 */
export function TerminalPane({
  frame, names, place, autoStart, focused, onOpened, onSaid, onClosed, onName, onFocus,
}: {
  /** Which of the arrangement's places this is (`../talk/layout`). */
  frame: string;
  /** What every frame is called, so a naming from anywhere reaches this row. */
  names: FrameNames;
  /** Which terminal to draw here, and where to start one. */
  place: Place;
  /** True for the slot that puts a terminal up without being asked — the one the face comes up with. */
  autoStart: boolean;
  focused: boolean;
  onOpened: (frame: string, session: string, folder: string | null) => void;
  onSaid: (statement: SessionSaidDto) => void;
  onClosed: (session: string) => void;
  onName: (frame: string, name: string, by: NamedBy) => void;
  onFocus: (frame: string) => void;
}) {
  const paneRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  const plateRef = useRef<Plate | null>(null);
  // Once a terminal has been asked for here it stays asked for: a slot whose program exited keeps the
  // pane, and a person who pressed the button once has not un-pressed it.
  const [running, setRunning] = useState(autoStart);
  // What the pane has to say for itself when it is not simply a terminal: the host's refusal if one
  // could not be started, and the fact of the program having exited, which the screen cannot show on
  // its own — what a finished shell leaves behind looks exactly like one waiting to be typed at.
  const [failed, setFailed] = useState<string | null>(null);
  const [ended, setEnded] = useState(false);

  // What the face wants done with what happens here, read at the moment it happens. The pane is put up
  // once and lives longer than any one render, so the effect below must not be re-run to see a newer
  // callback — that would take the terminal down to learn something it could have been told.
  const on = useRef({ onOpened, onSaid, onClosed, onName });
  on.current = { onOpened, onSaid, onClosed, onName };

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
    const plate = mountPlate(label, undefined, frame);
    plateRef.current = plate;
    void mountTerminal(host, {
      opened: (session, startedAt) => {
        plate.opened(session, startedAt);
        on.current.onOpened(frame, session, place.cwd ?? null);
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
    }, place)
      .then((take) => {
        // Taken away while the host was still answering. Detaching leaves the terminal running for
        // whatever draws it next, which is exactly what a pane that never got shown should do.
        if (taken) take();
        else detach = take;
      })
      .catch((e: unknown) => setFailed(e instanceof Error ? e.message : String(e)));
    return () => {
      taken = true;
      detach?.();
      plate.stop();
      plateRef.current = null;
    };
    // Only `running` is a reason to do any of this again. `place` and `frame` are what this pane *is*
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
            {failed === null
              ? <div className="termface__pane" ref={paneRef} />
              : <div className="termface__failed">{failed}</div>}
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
