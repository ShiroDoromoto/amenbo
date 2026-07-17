import { useState } from "react";
import { Markdown } from "../components/Markdown";
import { Attachments } from "../components/Attachments";
import { CommentRow } from "../components/CommentRow";
import { inTauri, type Decision, type DecisionStatus } from "../core/snapshot";
import {
  acceptDecision, addDecisionComment, amendDecision, buildsOnDecision, editDecisionComment,
  rejectDecision, reopenDecision, removeDecisionComment, supersedeDecision, unlinkDecisionEdge,
} from "../core/mutations";
import { useDecision, useDecisionComments, useDecisionPage } from "../core/reads";
import {
  EDGE_KINDS, edgeCandidates, edgeRows, promotesToAccepted, standingOn, type EdgeKind, type EdgeRow,
} from "../core/decisionEdges";
import { confirmDialog } from "../core/dialog";
import { isEnterSubmit } from "../core/keys";
import { errText, statusLabel, t, tf } from "../core/i18n";
import { decisionRef } from "../core/idref";

// Colour of the status badge — keep it matching DecisionsScreen's statusColor. "Superseded" is not a
// status but a derived fact (current:false), so grey is decided by currency, not by status.
function statusColor(s: DecisionStatus): string {
  switch (s) {
    case "accepted": return "#2e9e6b";
    case "proposed": return "#b88600";
    case "rejected": return "#c0504d";
  }
}

/**
 * The detail pane for one decision record. It renders inside the right pane, where AppShell draws the
 * PaneHeader, so this component returns the body alone and matches TaskDetailPane's layout. Accepting
 * or rejecting may carry an optional reason, so the buttons do not act at once — they raise a
 * confirmation with a reason field.
 */
export function DecisionDetailPane({ decisionId, onOpenTask, onOpenDecision }: {
  decisionId: number;
  onOpenTask?: (id: number) => void;
  /** Opens the decision on the other end of an edge — superseded, amended or built on (mirrors onOpenTask). */
  onOpenDecision?: (id: number) => void;
}) {
  const d = useDecision(decisionId);
  // The comment thread exists only under Tauri (the browser mock has no decisions); posting refetches via the WriteAck.
  const comments = useDecisionComments(inTauri() ? decisionId : null);
  const [comment, setComment] = useState("");
  const [confirming, setConfirming] = useState<null | "accept" | "reject">(null);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (!d) return <div className="rightpane__empty">{t("dec.notFound")}</div>;

  const submitComment = () => {
    if (comment.trim()) { void addDecisionComment(d.id, comment.trim()); setComment(""); }
  };
  // Await the write and only then close the panel: a failed accept/reject must not read as a success.
  // On failure the reason the user typed stays put, so retrying costs nothing.
  const runDecision = async () => {
    if (!confirming) return;
    setBusy(true);
    setError(null);
    try {
      if (confirming === "accept") await acceptDecision(d.id, reason);
      else await rejectDecision(d.id, reason);
      setConfirming(null);
      setReason("");
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="detail__body">
      <div className="detail__title" style={{ alignItems: "baseline", gap: 8 }}>
        {d.ref && <span style={{ color: "var(--c-muted)", fontVariantNumeric: "tabular-nums" }}>{d.ref}</span>}
        <span>{d.title}</span>
        <span style={{
          fontSize: "var(--fs-xs)", padding: "1px 8px", borderRadius: 10, color: "#fff",
          background: d.current ? statusColor(d.status) : "#8a93a0",
        }}>{d.current ? t(`dec.status.${d.status}`) : t("dec.status.superseded")}</span>
      </div>

      {d.body && (
        <div className="markdown" style={{ marginTop: 8, fontSize: "var(--fs-body)", maxWidth: "var(--measure-prose)" }}>
          <Markdown>{d.body}</Markdown>
        </div>
      )}

      <DecisionEdges d={d} onOpenDecision={onOpenDecision} />

      {d.linkedTasks.length > 0 && (
        <div style={{ marginTop: 8, fontSize: "var(--fs-sm)" }}>
          {t("dec.linkedTasks")}:{" "}
          {d.linkedTasks.map((lt, i) => (
            <span key={lt.id} style={lt.status === "done" ? { color: "var(--c-muted)" } : undefined}>
              {i > 0 && ", "}
              <button
                className="feed__action"
                style={{ padding: "0 4px", ...(lt.status === "done" ? { opacity: 0.6 } : {}) }}
                onClick={() => onOpenTask?.(Number(lt.id))}
              >
                {lt.ref ?? ""} {lt.name}
              </button>
              {lt.status !== "todo" && (
                <span className="faint" style={{ fontSize: "var(--fs-xs)" }}> · {statusLabel(lt.status)}</span>
              )}
            </span>
          ))}
        </div>
      )}

      {d.status === "proposed" && (
        confirming ? (
          // Confirming an accept or reject, with an optional reason that is left behind as one comment.
          <div className="compose" style={{ marginTop: 12 }}>
            {confirming === "reject" && standingOn(d).length > 0 && (
              <div style={{ marginBottom: 8, fontSize: "var(--fs-sm)" }}>
                <div className="faint">{t("dec.revisit")}</div>
                {standingOn(d).map((s) => (
                  <button
                    key={s.id}
                    className="feed__action"
                    style={{ display: "block", padding: "0 4px" }}
                    onClick={() => onOpenDecision?.(s.id)}
                  >
                    {s.ref ?? ""} {s.name ?? t("dec.unknownName")}
                  </button>
                ))}
              </div>
            )}
            <textarea
              className="compose__input"
              rows={3}
              autoFocus
              placeholder={t("dec.reasonPh")}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              onKeyDown={(e) => {
                if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void runDecision(); }
                if (e.key === "Escape") { setConfirming(null); setReason(""); setError(null); }
              }}
            />
            {error && <div className="newproj__error" role="alert">⚠ {error}</div>}
            <div className="compose__actions">
              <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.commentHint")}</span>
              <span>
                <button className="btn" onClick={() => { setConfirming(null); setReason(""); setError(null); }}>{t("dec.cancel")}</button>
                <button className="btn btn--primary" style={{ marginLeft: 6 }} disabled={busy} onClick={() => void runDecision()}>
                  {confirming === "accept" ? t("dec.accept") : t("dec.reject")}
                </button>
              </span>
            </div>
          </div>
        ) : (
          <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
            <button className="feed__action" onClick={() => setConfirming("accept")}>{t("dec.accept")}</button>
            <button className="feed__action" onClick={() => setConfirming("reject")}>{t("dec.reject")}</button>
          </div>
        )
      )}

      {d.status === "accepted" && (
        <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
          <button className="feed__action" onClick={() => void reopenDecision(d.id)}>{t("dec.reopen")}</button>
        </div>
      )}

      {inTauri() && (
        <>
          <div className="detail__sep" style={{ marginTop: 12 }} />
          <Attachments target="decision" targetId={d.id} />

          <div className="detail__sep" />

          <div>
            <div className="detail__section-h">{t("dec.comments")} · 💬 {comments.length}</div>
            {comments.length === 0 ? (
              <div className="faint" style={{ marginTop: 6, fontSize: "var(--fs-sm)" }}>{t("detail.noComments")}</div>
            ) : (
              <div className="comments">
                {comments.map((c) => (
                  <CommentRow
                    key={c.id}
                    id={c.id}
                    author={c.author}
                    ago={c.ago}
                    editedAgo={c.editedAgo}
                    text={c.text}
                    target="decision_comment"
                    onEdit={(text) => void editDecisionComment(c.id, d.id, text)}
                    onRemove={() => void removeDecisionComment(c.id, d.id)}
                  />
                ))}
              </div>
            )}
            <div className="compose">
              <textarea
                className="compose__input"
                rows={3}
                placeholder={t("detail.commentPh")}
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                onKeyDown={(e) => {
                  if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submitComment(); }
                }}
              />
              <div className="compose__actions">
                <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.commentHint")}</span>
                <button className="btn btn--primary" disabled={!comment.trim()} onClick={submitComment}>{t("detail.send")}</button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

// Shows and edits the edges between decisions (supersedes / amends / builds_on). All three types, in
// both directions, are listed in one column, and any row can be unwired. Unwiring never undoes the
// decision itself — drop a supersedes and the other decision simply becomes current again.
function DecisionEdges({ d, onOpenDecision }: {
  d: Decision;
  onOpenDecision?: (id: number) => void;
}) {
  const rows = edgeRows(d);
  const unlink = async (r: EdgeRow) => {
    const target = r.target.ref ?? decisionRef(r.target.id);
    if (await confirmDialog(tf("dec.edge.unlinkConfirm", { target }))) {
      void unlinkDecisionEdge(r.from, r.to);
    }
  };
  return (
    <div style={{ marginTop: 8 }}>
      {rows.map((r) => (
        <div
          key={`${r.labelKey}-${r.target.id}`}
          style={{ marginTop: 4, fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}
        >
          {t(r.labelKey)}:{" "}
          <button
            className="feed__action"
            style={{ padding: "0 4px" }}
            onClick={() => onOpenDecision?.(r.target.id)}
          >
            {r.target.ref ?? ""} {r.target.name ?? t("dec.unknownName")}
          </button>
          {r.staleBy && (
            <span style={{ marginLeft: 6, color: "#c0504d" }}>
              ⚠ {tf("dec.premiseStale", { premise: r.target.ref ?? decisionRef(r.target.id), by: r.staleBy })}
            </span>
          )}
          {inTauri() && (
            <button
              className="feed__action"
              style={{ marginLeft: 6, fontSize: "var(--fs-xs)" }}
              onClick={() => void unlink(r)}
            >
              {t("dec.edge.unlink")}
            </button>
          )}
        </div>
      ))}
      {inTauri() && d.project && <DecisionEdgeCompose d={d} projectId={Number(d.project.id)} />}
    </div>
  );
}

/**
 * The flow for wiring an edge. Pick the type (stop reading it / read it alongside / read it first),
 * then pick the other decision from the same project. The direction is always new → old, so this
 * decision is the one doing the wiring, and decisions already connected drop out of the candidates —
 * one type per pair. Wiring a supersedes from a decision still under discussion makes core promote it
 * to accepted; blocking that in the UI would break core's invariant, so the UI only says what is
 * about to happen. A supersedes turns the other decision into history — it overturns it — so anything
 * built on top of that decision is shown beforehand too: as a warning with a way out, never as a
 * block on the supersede itself.
 */
function DecisionEdgeCompose({ d, projectId }: { d: Decision; projectId: number }) {
  const [open, setOpen] = useState(false);
  const [kind, setKind] = useState<EdgeKind>("buildsOn");
  const [query, setQuery] = useState("");
  const all = useDecisionPage(projectId);
  const link = async (target: Decision) => {
    const label = target.ref ?? decisionRef(target.id);
    if (promotesToAccepted(d, kind)) {
      if (!(await confirmDialog(tf("dec.edge.supersedeAcceptsConfirm", { target: label })))) return;
    }
    const standing = kind === "supersedes" ? standingOn(target) : [];
    if (standing.length > 0) {
      const list = standing.map((s) => `${s.ref ?? decisionRef(s.id)} ${s.name ?? t("dec.unknownName")}`).join("\n");
      if (!(await confirmDialog(tf("dec.edge.supersedeRevisitConfirm", { target: label, list })))) return;
    }
    if (kind === "supersedes") void supersedeDecision(d.id, target.id);
    else if (kind === "amends") void amendDecision(d.id, target.id);
    else void buildsOnDecision(d.id, target.id);
    setOpen(false);
    setQuery("");
  };
  if (!open) {
    return (
      <button className="feed__action" style={{ marginTop: 6 }} onClick={() => setOpen(true)}>
        ＋ {t("dec.edge.add")}
      </button>
    );
  }
  const candidates = edgeCandidates(all, d, query);
  return (
    <div style={{ marginTop: 6, display: "flex", flexDirection: "column", gap: 6, maxWidth: "var(--measure-prose)" }}>
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        <select className="btn" value={kind} onChange={(e) => setKind(e.target.value as EdgeKind)}>
          {EDGE_KINDS.map((k) => (
            <option key={k} value={k}>{t(`dec.edge.kind.${k}`)}</option>
          ))}
        </select>
        <input
          className="compose__input"
          style={{ flex: 1 }}
          autoFocus
          placeholder={t("dec.edge.searchPh")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Escape") { setOpen(false); setQuery(""); } }}
        />
        <button className="btn" onClick={() => { setOpen(false); setQuery(""); }}>{t("dec.edge.cancel")}</button>
      </div>
      {promotesToAccepted(d, kind) && (
        <div style={{ fontSize: "var(--fs-sm)", color: "#c0504d" }}>⚠ {t("dec.edge.supersedeAccepts")}</div>
      )}
      {candidates.length === 0 ? (
        <div className="faint" style={{ fontSize: "var(--fs-sm)" }}>{t("dec.edge.noCandidates")}</div>
      ) : (
        <ul className="apick__menu apick__menu--scroll" role="listbox" style={{ position: "static" }}>
          {candidates.map((c) => (
            <li key={c.id} role="option" aria-selected={false}>
              <button type="button" className="apick__opt" onClick={() => void link(c)}>
                <span style={{ color: "var(--c-muted)", fontVariantNumeric: "tabular-nums" }}>{c.ref}</span>
                <span>{c.title}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
