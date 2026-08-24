// The first loop — the single push that joins the two ends: the user asks their AI, the AI writes to
// Amenbo, and what it wrote shows up on the board (`AMB-D-414`).
//
// **It is one press.** Starting in the terminal opens the folder's own pane, and the agent there is
// handed the launch instruction as its opening prompt before it starts (`amenbo_core::harness`), so
// there is nothing left to copy and nothing to paste. Which AI that is Amenbo does not know and does
// not ask (`../talk/agent`).
//
// **The way out to somebody else's terminal is not closed, it is folded.** VS Code, Zed, a plain
// shell — those readers still need the request text, and so does anybody whose machine has no agent
// this app can start, for whom the outside is the only road there is. So the fold is one press away
// and is never conditioned on what this machine can start: a way out that appears and disappears is
// one nobody can be told about.
//
// This is a part, not a place: the completion step of project creation shows it, and so does a board
// with nothing on it yet. So it takes the folder it speaks about and nothing about where it sits —
// where the terminal opens is the shell's (`../shell/AppShell`), which is the only thing that knows
// whether this window is holding the face at all.
import { useState } from "react";
import { useCliCommandName } from "../core/cliCommand";
import { errText, t, tf } from "../core/i18n";
import { ErrorNote } from "./ErrorNote";
import { Icon } from "./Icon";
import { NoCli } from "./NoCli";

/**
 * The whole flow, for a folder that is linked — `dir` is that folder.
 *
 * It takes the folder rather than looking one up: a project is bound to its folders and not the other way
 * round, so whoever draws the loop has already had to ask (`useBoundFolders`), and on the board the answer
 * decides whether the loop is what gets drawn at all (`AMB-D-533`).
 */
export function FirstLoop({ dir, onStart }: { dir: string; onStart: (dir: string) => void }) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Whether the way out to the reader's own terminal is open. It is closed to begin with so that the
  // one press is the only thing a first-time reader meets, and it is theirs to open — nothing here
  // decides for them that they are the sort of person who wants it.
  const [outside, setOutside] = useState(false);
  // The request tells the AI to run a command, so it has to name the one this build installs — and
  // where it installs none the reader can run, there is no request to hand over at all.
  const cli = useCliCommandName();
  const prompt = cli && tf("firstloop.prompt", { cmd: cli });

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
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
        <span className="firstloop__title"><Icon name="rocket" size="md" /> {t("firstloop.title")}</span>
        <span className="firstloop__intro muted">{t("firstloop.intro")}</span>
      </div>

      <button className="btn btn--primary" onClick={() => onStart(dir)}>
        <Icon name="keyboard" /> {t("firstloop.start")}
      </button>
      {/* What the folder *is*, not what the AI will do in it: an agent's own behaviour is not
          Amenbo's to promise (`AMB-D-749`). */}
      <span className="firstloop__stephint muted">{t("firstloop.startHint")}</span>

      <div className="firstloop__outside">
        <button className="btn firstloop__toggle" aria-expanded={outside} onClick={() => setOutside(!outside)}>
          <Icon name={outside ? "chevronDown" : "chevronRight"} /> {t("firstloop.outside")}
        </button>
        {outside && (
          <>
            <span className="firstloop__stephint muted">{t("firstloop.outsideHint")}</span>
            {prompt ? (
              <>
                <p className="firstloop__prompt">{prompt}</p>
                <button className="btn" onClick={() => void copy(prompt)}>
                  {copied ? <><Icon name="check" /> {t("firstloop.copied")}</> : <><Icon name="clipboard" /> {t("firstloop.copy")}</>}
                </button>
              </>
            ) : (
              <NoCli />
            )}
          </>
        )}
      </div>

      {/* The payoff, and the reason the loop is worth walking at all — kept as a sentence rather than
          a step, because there is nothing here for the reader to do. */}
      <div className="firstloop__end">
        <div className="firstloop__steptitle">{t("firstloop.appear")}</div>
        <div className="firstloop__stephint muted">{t("firstloop.appearHint")}</div>
      </div>

      {error && <ErrorNote>{error}</ErrorNote>}
    </div>
  );
}
