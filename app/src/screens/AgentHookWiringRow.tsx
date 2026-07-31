// What this project still has to wire, standing on the project's own screen until the last of its folders
// is wired (`AMB-D-459`). It is the catalogue's other half from `AgentHookConsentModal`: the modal asks,
// once, and is done; this reports, and only the wiring ends it.
//
// **Consent is per project, wiring is per folder — and that gap is what this closes.** A reader who
// answered yes and pasted into one of four folders is, on the record, done: the question has an answer and
// never comes back, while three folders go on starting their AI without amenbo and nothing says so. A
// question cannot carry that, because it is not a question — it is work left, and work left belongs on the
// screen rather than in a dialog.
//
// **So there is no close button, and no dismissal.** Dismissing is how a reader puts a question off, and
// this is not one; the row goes when the folders behind it are wired, which is the only ending it has. It
// draws nothing where there is nothing left — including where the project said no, which core keeps silent.
//
// **One text, the folders listed under it.** The request for a harness is the same text wherever it is
// pasted (only the path changes), so it goes up once and the folders waiting for it are a list. The
// per-folder shape this replaces put the whole request on screen once per folder, which at four folders is
// four screens of identical text and nothing anybody reads.
//
// The text is shown rather than hidden behind the copy button, for the reason the banner shows it: what it
// asks for is an edit to a file the reader owns, made by an AI of theirs, so the moment to read it is
// before it is handed over.
import { useEffect, useState } from "react";
import { fetchAgentHookProjectWiring } from "../core/mutations";
import { t, tf } from "../core/i18n";
import type { AgentHookWiringDto } from "../bindings/bindings";

/**
 * `projectId` is the project being looked at — the row answers for that one alone, which is what lets it
 * name folders the reader can walk to from here.
 *
 * `turn` is the one-question-at-a-time rule reaching this surface: false while the question about this
 * project is still up, and nothing is fetched or drawn until it goes true. The disk it reads is the disk
 * that question writes to — a refusal recorded there silences this — so reading ahead of it would report a
 * setup the reader has just declined.
 *
 * It reads settings files on disk, so it is fetched when the project changes and not on every store tick:
 * a task moving on the board cannot wire a folder. A failure to read is swallowed and draws nothing — a
 * report that could not be made is not a report of trouble.
 */
export function AgentHookWiringRow({ projectId, turn }: { projectId: number; turn: boolean }) {
  const [waiting, setWaiting] = useState<AgentHookWiringDto[]>([]);
  // Which tool the reader picked. Unset means the first on offer — the only one where the project's folders
  // point at exactly one, and the head of the catalog where they point at none.
  const [picked, setPicked] = useState<string | null>(null);
  // Which tool's text was last copied, so the button can say so — by tool, since picking another one is
  // exactly the moment "Copied" would be a lie.
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setWaiting([]);
    setPicked(null);
    setCopied(null);
    if (!turn) return;
    fetchAgentHookProjectWiring(projectId)
      .then((rows) => alive && setWaiting(rows))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [projectId, turn]);

  if (waiting.length === 0) return null;

  const row = waiting.find((one) => one.tool.tool === picked) ?? waiting[0];

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(row.tool.request);
      setCopied(row.tool.tool);
      setTimeout(() => setCopied(null), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };

  return (
    <div className="agenthookrow">
      <div className="agenthookrow__title">🔌 {t("agentHookWiring.title")}</div>
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
      <div className="agenthookrow__folders">{t("agentHookWiring.folders")}</div>
      <ul className="agenthookrow__dirs">
        {row.dirs.map((dir) => <li key={dir}>{dir}</li>)}
      </ul>
      <pre className="agenthookrow__request">{row.tool.request}</pre>
      <button className="btn" onClick={() => void copy()}>
        {copied === row.tool.tool ? t("agentHookWiring.copied") : t("agentHookWiring.copy")}
      </button>
    </div>
  );
}
