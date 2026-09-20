// The question git is waiting on, put to the person in the window instead of to a terminal nobody
// is watching (`AMB-D-913`, `crate::folder_git_askpass`).
//
// **What it draws is git's own sentence, word for word.** `Username for 'https://github.com':`,
// `Enter passphrase for key '…':` — as git and ssh wrote them, in the language they wrote them in.
// Amenbo does not say them again in its own words and does not turn them into a code with a template
// behind it, for the reason every other sentence from git here is left alone (`AMB-D-906`, 3-4): a
// template is a rewriting, and what a reader gets here is what they would have got in a terminal.
//
// **Everything around it is this side's, and that is the part that is translated**: which call is
// waiting, the offer to keep the answer, and the two buttons.
//
// **Amenbo holds nothing of what is typed.** Kept, it goes to `git credential approve` and from
// there into this machine's own credential helper; not kept, it is used for that one call and
// dropped. The pane's agent runs git off the same helper, so a value Amenbo held as well would be a
// second answer to the same question with nothing on screen saying which one is in force
// (`AMB-D-913`).
//
// **A closed dialog is not an empty password.** git reads an empty password as a password, so
// "nobody answered" travels as its own thing the whole way down — and it is what git is told when
// this goes away with a question still on it, rather than leaving the call standing until the
// helper's own ten minutes run out.
//
// **It stands wherever the workspace is** (`../shell/WorkspaceFace`), which is the board before
// the terminal is split out and the talk window after (`AMB-D-753`). The host tells every window;
// only one of them has the face, so only one question is ever drawn.
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { GitAskDto } from "../bindings/bindings";
import { t, tf } from "../core/i18n";
import { folderGitAskpassSaid, onGitAsked } from "./folder";

/**
 * The questions git is waiting on, drawn one at a time.
 *
 * They are queued rather than replaced: a `git push` over HTTPS asks twice — who the credential is
 * for, then the password — and ssh asks once per key it tries. A second question arriving is not the
 * first one being withdrawn, and answering the wrong one would send a password back to whichever
 * call happened to ask last.
 */
export function GitAsk() {
  const [queue, setQueue] = useState<GitAskDto[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let gone = false;
    void onGitAsked((ask) => setQueue((waiting) => [...waiting, ask]))
      .then((un) => { if (gone) un(); else unlisten = un; })
      // Outside Tauri (`npm run dev` in a browser) there is no host running git, and a page that
      // draws the face is worth more than one that failed over a road nothing travels.
      .catch(() => {});
    return () => { gone = true; unlisten?.(); };
  }, []);

  // Everything still waiting when this goes away is told nobody answered. Without it the call would
  // stand until the helper gave up on its own, which is ten minutes of a window that folded back
  // looking like a `git fetch` that hung. The cleanup runs once, so what it has to work from is a
  // box the renders keep filling rather than the queue it closed over.
  const standing = useRef<GitAskDto[]>([]);
  useEffect(() => { standing.current = queue; }, [queue]);
  useEffect(() => () => {
    for (const ask of standing.current) void folderGitAskpassSaid(ask.id, null, false).catch(() => {});
  }, []);

  const asked = queue[0];
  const answer = useCallback((said: string | null, save: boolean) => {
    if (asked === undefined) return;
    void folderGitAskpassSaid(asked.id, said, save).catch(() => {});
    setQueue((waiting) => waiting.filter((one) => one.id !== asked.id));
  }, [asked]);

  if (asked === undefined) return null;
  // Keyed by the question, so the box is empty again for the second of git's two rather than still
  // holding the name typed into the first.
  return <Question key={asked.id} asked={asked} onAnswer={answer} />;
}

/** One question, on the screen. */
function Question({ asked, onAnswer }: {
  asked: GitAskDto;
  /** What the person said, and whether they asked for it to be kept. `null` is a dialog closed. */
  onAnswer: (said: string | null, save: boolean) => void;
}) {
  const [said, setSaid] = useState("");
  const [save, setSave] = useState(false);

  // The window comes forward. A password box behind what the reader is looking at is a call that has
  // stopped for no reason they can see, and this one stops for as long as nobody answers.
  useEffect(() => {
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => {
        const win = getCurrentWindow();
        void win.unminimize();
        return win.setFocus();
      })
      .catch(() => {});
  }, []);

  return createPortal(
    <div
      className="modal__overlay modal__overlay--raised"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onAnswer(null, false); }}
      onKeyDown={(e) => { if (e.key === "Escape") onAnswer(null, false); }}
    >
      <form
        className="gitask"
        role="dialog"
        aria-modal="true"
        aria-labelledby="gitask-asked"
        onSubmit={(e) => { e.preventDefault(); onAnswer(said, save); }}
      >
        <div className="gitask__doing">{tf("git.askDoing", { what: asked.doing })}</div>
        {/* git's own words, drawn as they arrived. ssh's run to several lines, so the box keeps the
            line breaks rather than running them together. */}
        <div className="gitask__asked" id="gitask-asked">{asked.asked}</div>
        <input
          className="gitask__said"
          type={asked.secret ? "password" : "text"}
          value={said}
          autoFocus
          // Nothing the browser has remembered belongs in a box whose answer goes to git.
          autoComplete="off"
          aria-label={asked.asked}
          onChange={(e) => setSaid(e.target.value)}
        />
        {/* Offered only where there is a whole credential to keep. git's first question names no
            user yet, and a key's passphrase is not a credential at all — the helper has nowhere to
            put either (`crate::folder_git_askpass`). */}
        {asked.savable && (
          <>
            <label className="gitask__save">
              <input type="checkbox" checked={save} onChange={(e) => setSave(e.target.checked)} />
              {t("git.askSave")}
            </label>
            <div className="gitask__where">{t("git.askSaveWhere")}</div>
          </>
        )}
        <div className="gitask__actions">
          <button className="gitask__action gitask__action--go" type="submit">
            {t("git.askGo")}
          </button>
          <button className="gitask__action" type="button" onClick={() => onAnswer(null, false)}>
            {t("git.askCancel")}
          </button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
