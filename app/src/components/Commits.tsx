// The commit-SHA view on a task's detail pane. A task carries many commit SHAs; amenbo
// keeps each as an opaque string — it never reads git, verifies the commit, or knows which forge it
// lives on, so the SHA is shown raw (no clickable forge link — a user who wants one attaches a url,
// which coexists). The SHA is validated at the ops door, so a bad value comes back as an error the
// input surfaces rather than landing. Recording a SHA already on the task is a no-op.

import { useState } from "react";
import { useTaskCommits, type TaskCommit } from "../core/reads";
import { addTaskCommit, removeTaskCommit } from "../core/mutations";
import { confirmDialog } from "../core/dialog";
import { errText, t, tf } from "../core/i18n";

/** Copy a SHA to the clipboard, flipping a "copied" hint for a moment. Best-effort — a clipboard failure is silent. */
function CopyButton({ sha }: { sha: string }) {
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(sha);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* no clipboard — nothing to show */
    }
  };
  return (
    <button className="feed__action" title={t("commit.copy")} onClick={() => void onCopy()}>
      {copied ? t("commit.copied") : "⧉"}
    </button>
  );
}

export function Commits({ taskId }: { taskId: number }) {
  const commits = useTaskCommits(taskId);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    const sha = draft.trim();
    if (!sha) return;
    setBusy(true);
    setError(null);
    try {
      await addTaskCommit(taskId, sha);
      setDraft("");
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const onRemove = async (c: TaskCommit) => {
    if (await confirmDialog(tf("commit.removeConfirm", { sha: c.sha }))) {
      await removeTaskCommit(taskId, c.sha);
    }
  };

  return (
    <div>
      <div className="detail__section-h">{t("commit.section")}</div>
      {commits.length === 0 ? (
        <div className="faint" style={{ fontSize: "var(--fs-sm)" }}>{t("commit.none")}</div>
      ) : (
        <div className="commits">
          {commits.map((c) => (
            <div className="commits__item" key={c.id}>
              <code className="commits__sha" title={c.sha}>{c.sha}</code>
              <CopyButton sha={c.sha} />
              <button className="feed__action" title={t("commit.remove")} onClick={() => void onRemove(c)}>✕</button>
            </div>
          ))}
        </div>
      )}
      <div className="compose" style={{ marginTop: 8 }}>
        <input
          className="compose__input"
          value={draft}
          placeholder={t("commit.placeholder")}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void submit(); } }}
        />
        {error && <div className="newproj__error" role="alert">⚠ {error}</div>}
        <div className="compose__actions">
          <span />
          <button className="btn btn--primary" disabled={busy || !draft.trim()} onClick={() => void submit()}>
            {t("commit.record")}
          </button>
        </div>
      </div>
    </div>
  );
}
