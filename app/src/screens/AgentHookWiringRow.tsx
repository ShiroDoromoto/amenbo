// What this project still has to wire, standing on the project's own screen — the acting face of what the
// GUI has to say about the session-start hook (`AMB-D-459`, `AMB-D-460`). It reports the work, explains
// what the text does, hands the text over, and takes the refusal that ends it.
//
// It is one of the board's standing notices and the board carries only one (`AMB-D-535`), so it is not
// always the one drawn. What it loses to does not take the work with it: project settings lists the
// folders waiting whatever the board is showing, and that is the looking-over face to this acting one.
//
// **Consent is per project, wiring is per folder — and that gap is what this closes.** A reader who
// answered yes and pasted into one of four folders is, on the record, done: the question has an answer and
// never comes back, while three folders go on starting their AI without amenbo and nothing says so. A
// question cannot carry that, because it is not a question — it is work left, and work left belongs on the
// screen rather than in a dialog.
//
// **The refusal is on the row, and can be given whenever it is read.** A no is what silences this
// (`harness::setup_notice`), so putting it behind a dialog asked once left a reader who changed their mind
// three steps from the only button that ends it. It is asked where it is answered.
//
// **Closing is not answering.** It records nothing and only takes the row off the screen in front of the
// reader; opening the project again brings it back, since the work behind it is still there. Nothing has
// to be silenced for good here — the row does not interrupt anybody, so someone who has not decided yet
// can walk past it without being made to say no.
//
// **One text, the folders listed under it.** The request for a harness is the same text wherever it is
// pasted (only the path changes), so it goes up once and the folders waiting for it are a list.
//
// The text is shown rather than hidden behind the copy button: what it asks for is an edit to a file the
// reader owns, made by an AI of theirs, so the moment to read it is before it is handed over.
import { useCallback, useEffect, useState } from "react";
import { answerAgentHookOffer, fetchAgentHookProjectWiring } from "../core/mutations";
import { errText, t, tf, tn } from "../core/i18n";
import { ErrorNote } from "../components/ErrorNote";
import type { AgentHookWiringDto } from "../bindings/bindings";
import { Icon } from "../components/Icon";

/** What a project still has to wire, and the row's way of saying a refusal has landed. */
export type AgentHookWiring = {
  /** The folders waiting, grouped by harness. Empty is nothing left to say. */
  waiting: AgentHookWiringDto[];
  /** Called once a refusal is recorded — the report ends, and the reading is not repeated to learn that. */
  answered: () => void;
};

/**
 * What this project has left to wire, read once for whoever needs it.
 *
 * It is a Hook rather than the row's own state because **whether the row has anything to say is part of
 * the board's ordering** (`pickBoardNotice`): the board draws one standing notice, and it cannot pick
 * between them without knowing which of them are standing. Reading it here also keeps it to one read where
 * two surfaces want the answer.
 *
 * It reads settings files on disk, so it is fetched when the project changes and not on every store tick:
 * a task moving on the board cannot wire a folder. A failure to read is swallowed and reports nothing — a
 * report that could not be made is not a report of trouble.
 */
export function useAgentHookWiring(projectId: number): AgentHookWiring {
  const [waiting, setWaiting] = useState<AgentHookWiringDto[]>([]);

  useEffect(() => {
    let alive = true;
    setWaiting([]);
    fetchAgentHookProjectWiring(projectId)
      .then((rows) => alive && setWaiting(rows))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [projectId]);

  const answered = useCallback(() => setWaiting([]), []);
  return { waiting, answered };
}

/**
 * `projectId` is the project being looked at — the row answers for that one alone, which is what lets it
 * name folders the reader can walk to from here, and what the refusal is recorded against. `wiring` is what
 * that project has left to wire ({@link useAgentHookWiring}).
 *
 * A failure to *record* is not swallowed: the row stays up with the reason on it, because a row that
 * vanished on a write that never landed would report an answer nobody has.
 */
export function AgentHookWiringRow({ projectId, wiring }: { projectId: number; wiring: AgentHookWiring }) {
  const { waiting } = wiring;
  // Which tool the reader picked. Unset means the first on offer — the only one where the project's folders
  // point at exactly one, and the head of the catalog where they point at none.
  const [picked, setPicked] = useState<string | null>(null);
  // Which tool's text was last copied, so the button can say so — by tool, since picking another one is
  // exactly the moment "Copied" would be a lie.
  const [copied, setCopied] = useState<string | null>(null);
  // Taken off the screen for now, recording nothing. It lives with the project on screen and no longer:
  // walking back in is what brings it back, which is the whole difference between this and the no.
  const [closed, setClosed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setPicked(null);
    setCopied(null);
    setClosed(false);
    setError(null);
  }, [projectId]);

  if (closed || waiting.length === 0) return null;

  const row = waiting.find((one) => one.tool.tool === picked) ?? waiting[0];

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(row.tool.request);
      setCopied(row.tool.tool);
      setTimeout(() => setCopied(null), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };

  // The one button here that writes anything. It forbids nothing — the text stays there for the asking on
  // the command line, and the project settings screen is the way back — but it ends the report.
  const refuse = async () => {
    setBusy(true);
    setError(null);
    try {
      await answerAgentHookOffer(projectId, false);
      wiring.answered();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="agenthookrow">
      <div className="agenthookrow__title"><Icon name="plug" size="md" /> {t("agentHookWiring.title")}</div>
      {/* Only where there is a choice to make. With one tool waiting, the folders have already said which
          one they are, and a picker holding a single value asks a question with no other answer. */}
      {waiting.length > 1 && (
        <select
          className="agenthookrow__pick"
          aria-label={t("agentHookWiring.pick")}
          value={row.tool.tool}
          onChange={(e) => setPicked(e.target.value)}
        >
          {waiting.map((one) => (
            <option key={one.tool.tool} value={one.tool.tool}>{one.tool.label}</option>
          ))}
        </select>
      )}
      <div className="agenthookrow__what">
        {tf("agentHookWiring.what", { tool: row.tool.label, file: row.tool.pasteInto })}
      </div>
      {/* Counted, because the one instruction the heading carries is what it means to paste "once": with a
          single folder that is the whole of it, and with several it is once over again in each. */}
      <div className="agenthookrow__folders">{tn("agentHookWiring.folders", row.dirs.length)}</div>
      <ul className="agenthookrow__dirs">
        {row.dirs.map((dir) => <li key={dir}>{dir}</li>)}
      </ul>
      <pre className="agenthookrow__request">{row.tool.request}</pre>

      {error && <ErrorNote>{error}</ErrorNote>}

      <div className="agenthookrow__actions">
        <button className="btn" onClick={() => void copy()}>
          {copied === row.tool.tool ? t("agentHookWiring.copied") : t("agentHookWiring.copy")}
        </button>
        <button className="btn" disabled={busy} onClick={() => void refuse()}>
          {t("agentHookWiring.no")}
        </button>
        {/* Its own label, not the shared "close" one: the two buttons beside each other differ in what they
            leave behind, and that is what each says (`AMB-D-663`). */}
        <button className="btn" disabled={busy} onClick={() => setClosed(true)}>
          {t("agentHookWiring.later")}
        </button>
      </div>
    </div>
  );
}
