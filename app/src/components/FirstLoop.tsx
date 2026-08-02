// The first loop — the single push that joins the two ends: the user asks their AI, the AI writes to
// amenbo, and what it wrote shows up on the board (`AMB-D-414`). Every move the GUI can make on the
// user's behalf, it makes: the terminal opens already inside the linked folder, and the request text
// is handed over finished, with no hole left to fill in. What stays with the user is launching their
// own AI and pasting — which AI that is, amenbo does not know, and does not ask.
//
// Where there is no terminal to open — a Linux box without `x-terminal-emulator`, say — the move
// falls back to handing over the folder's path to copy, so the loop still closes by the user opening
// their own terminal and cd-ing there. An error alone would end the walk right at step one.
//
// This is a part, not a place: the completion step of project creation shows it, and so does a board
// with nothing on it yet. So it takes the folder it speaks about and nothing about where it sits.
// One button per move, and each does only what its label says, so nothing the reader did not press
// happens behind their back.
import { useEffect, useState } from "react";
import { useCliCommandName } from "../core/cliCommand";
import { errText, t, tf } from "../core/i18n";
import { fetchBoundFolders, openTerminal } from "../core/mutations";

/**
 * The loop for a project, wherever the project is what the reader is looking at: it finds the folder
 * itself, since a project is bound to its folders and not the other way round. With none — or with
 * only folders that have since moved away — there is no terminal to open and nowhere for an AI to
 * write, so the invitation is to link one, and the loop waits behind it.
 *
 * Nothing is drawn until the answer is in. A flash of "link a folder" on a project that has one reads
 * as a broken binding, which is a worse thing to say than nothing.
 */
export function ProjectFirstLoop({ projectId, onLinkFolder }: { projectId: number; onLinkFolder: () => void }) {
  const [dir, setDir] = useState<string | null>(null);
  const [answered, setAnswered] = useState(false);

  useEffect(() => {
    let alive = true;
    setAnswered(false);
    fetchBoundFolders(projectId)
      .then((folders) => {
        if (!alive) return;
        setDir(folders.find((f) => f.exists)?.path ?? null);
        setAnswered(true);
      })
      .catch(() => { if (alive) setAnswered(true); }); // Unanswered is the same as unbound: the invitation is the safe half.
    return () => { alive = false; };
  }, [projectId]);

  if (!answered) return null;
  if (dir) return <FirstLoop dir={dir} />;

  return (
    <div className="firstloop">
      <div className="firstloop__head">
        <span className="firstloop__title">📂 {t("firstloop.noFolderTitle")}</span>
        <span className="firstloop__intro muted">{t("firstloop.noFolderHint")}</span>
      </div>
      <button className="btn" onClick={onLinkFolder}>{t("firstloop.noFolderBtn")}</button>
    </div>
  );
}

/** The whole flow, for a project whose folder is linked — `dir` is that folder. */
export function FirstLoop({ dir }: { dir: string }) {
  const [copied, setCopied] = useState<"prompt" | "path" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [noTerminal, setNoTerminal] = useState(false);
  // The request tells the AI to run a command, so it has to name the one this build installs.
  const prompt = tf("firstloop.prompt", { cmd: useCliCommandName() });

  const terminal = async () => {
    try {
      await openTerminal(dir);
      setError(null);
      setNoTerminal(false);
    } catch (e) {
      setError(errText(e));
      setNoTerminal(true);
    }
  };

  const copy = async (text: string, which: "prompt" | "path") => {
    try {
      await navigator.clipboard.writeText(text);
      setError(null);
      setCopied(which);
      setTimeout(() => setCopied(null), 2000);
    } catch (e) {
      setError(errText(e));
    }
  };

  return (
    <div className="firstloop">
      <div className="firstloop__head">
        <span className="firstloop__title">🚀 {t("firstloop.title")}</span>
        <span className="firstloop__intro muted">{t("firstloop.intro")}</span>
      </div>

      <Step n={1} title={t("firstloop.s1title")} hint={t("firstloop.s1hint")}>
        <button className="btn" onClick={() => void terminal()}>⌨️ {t("firstloop.s1btn")}</button>
        {noTerminal && (
          <div className="firstloop__fallback">
            <div className="firstloop__stephint muted">{t("firstloop.s1fallback")}</div>
            <p className="firstloop__path">{dir}</p>
            <button className="btn" onClick={() => void copy(dir, "path")}>
              {copied === "path" ? t("firstloop.copied") : `📋 ${t("firstloop.s1fallbackbtn")}`}
            </button>
          </div>
        )}
      </Step>

      <Step n={2} title={t("firstloop.s2title")} hint={t("firstloop.s2hint")}>
        <p className="firstloop__prompt">{prompt}</p>
        <button className="btn" onClick={() => void copy(prompt, "prompt")}>
          {copied === "prompt" ? t("firstloop.copied") : `📋 ${t("firstloop.s2btn")}`}
        </button>
      </Step>

      <Step n={3} title={t("firstloop.s3title")} hint={t("firstloop.s3hint")} />

      {error && <div className="firstloop__error" role="alert">⚠ {error}</div>}
    </div>
  );
}

/** One numbered move: what it is, why, and — for the two that have one — the button that does it. */
function Step({ n, title, hint, children }: { n: number; title: string; hint: string; children?: React.ReactNode }) {
  return (
    <div className="firstloop__step">
      <div className="firstloop__num">{n}</div>
      <div className="firstloop__stepbody">
        <div className="firstloop__steptitle">{title}</div>
        <div className="firstloop__stephint muted">{hint}</div>
        {children}
      </div>
    </div>
  );
}
