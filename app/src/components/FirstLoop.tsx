// The first loop — the single push that joins the two ends: the user asks their AI, the AI writes to
// amenbo, and what it wrote shows up on the board (`AMB-D-414`). Every move the GUI can make on the
// user's behalf, it makes: the terminal opens already inside the linked folder, and the request text
// is handed over finished, with no hole left to fill in. What stays with the user is launching their
// own AI and pasting — which AI that is, amenbo does not know, and does not ask.
//
// This is a part, not a place: the completion step of project creation shows it, and so does a board
// with nothing on it yet. So it takes the folder it speaks about and nothing about where it sits.
// One button per move, and each does only what its label says, so nothing the reader did not press
// happens behind their back.
import { useState } from "react";
import { errText, t } from "../core/i18n";
import { openTerminal } from "../core/mutations";

/** The whole flow, for a project whose folder is linked — `dir` is that folder. */
export function FirstLoop({ dir }: { dir: string }) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const terminal = async () => {
    try {
      await openTerminal(dir);
      setError(null);
    } catch (e) {
      setError(errText(e));
    }
  };

  const copyPrompt = async () => {
    try {
      await navigator.clipboard.writeText(t("firstloop.prompt"));
      setError(null);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
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
      </Step>

      <Step n={2} title={t("firstloop.s2title")} hint={t("firstloop.s2hint")}>
        <p className="firstloop__prompt">{t("firstloop.prompt")}</p>
        <button className="btn" onClick={() => void copyPrompt()}>
          {copied ? t("firstloop.copied") : `📋 ${t("firstloop.s2btn")}`}
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
