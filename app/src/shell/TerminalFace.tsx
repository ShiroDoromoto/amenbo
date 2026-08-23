import { useEffect, useRef, useState } from "react";
import { mountAgentFrame } from "../talk/agent";
import { nameFrame, ONLY_FRAME } from "../talk/frames";
import { closed, NO_SESSIONS, opened, said, type Sessions } from "../talk/sessions";
import { currentLang, t } from "../core/i18n";

/**
 * The terminal, drawn inside the board's window — the second face of the one window (`AMB-D-753`).
 *
 * It is put up once and then left alone. Switching back to the ledger hides it with CSS rather than
 * taking it down, which is the one thing this component exists to guarantee: unmounting would take
 * the emulator with it, and a terminal whose pane went away is an agent nobody can get back to. The
 * caller therefore keeps this rendered for as long as the window is the terminal's home, and hides
 * it by hiding its own container.
 *
 * When the window *stops* being the terminal's home — the user splits it out, or a language change
 * rebuilds the interface — this does come down, and the session does not: the pane detaches, and
 * whatever draws next adopts what is still running (`app/src/talk/terminal.ts`).
 *
 * What runs in the pane is not settled here. The frame put up inside it asks the host which agent
 * this folder starts with, and draws the offer or the install notice where that has no single answer
 * (`app/src/talk/agent.ts`) — which is also where a refusal to start one is shown, so nothing here
 * holds a failure of its own.
 *
 * `note` is what the shell has to say about this face that the pane cannot — a window that could not
 * be split out, which is the press of the button here having come to nothing.
 */
export function TerminalFace({ onSplitOut, note }: { onSplitOut: () => void; note: string | null }) {
  const paneRef = useRef<HTMLDivElement>(null);
  // What is running in the pane. It is held for as long as the pane is, the way the talk window holds
  // its own (`app/src/talk/sessions.ts`) — a session has no existence outside the terminal it runs in,
  // so what knows about one is whatever is drawing it.
  const sessions = useRef<Sessions>(NO_SESSIONS);
  // What the pane has to say for itself when it is not simply a terminal: the host's refusal if one
  // could not be started, and the fact of the program having exited, which the screen cannot show on
  // its own — what a finished shell leaves behind looks exactly like one waiting to be typed at.
  const [ended, setEnded] = useState(false);

  useEffect(() => {
    const host = paneRef.current;
    if (!host) return;
    let taken = false;
    let detach: (() => void) | null = null;
    setEnded(false);
    void mountAgentFrame(host, currentLang(), {
      opened: (session, startedAt) => {
        sessions.current = opened(sessions.current, { session, startedAt });
      },
      said: (statement) => {
        sessions.current = said(sessions.current, statement);
      },
      closed: (session) => {
        sessions.current = closed(sessions.current, session);
        setEnded(true);
      },
      // The name is offered to the store and nothing here draws it: this window is the board, and
      // what it is called is not the pane's to say. What does draw it is the pane's own frame, once
      // there are frames to draw (`AMB-T-3607`).
      name: (text, by) => {
        void nameFrame(ONLY_FRAME, text, by).catch(() => {});
      },
    }, "termface__pane")
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
    };
  }, []);

  return (
    <div className="termface">
      <div className="termface__bar">
        <button className="termface__action" onClick={onSplitOut}>{t("face.splitOut")}</button>
        {ended && <span className="termface__note">{t("face.ended")}</span>}
        {note !== null && <span className="termface__note">{note}</span>}
      </div>
      <div className="termface__face" ref={paneRef} />
    </div>
  );
}
