import { useEffect, useRef, useState } from "react";
import { Markdown } from "../components/Markdown";
import { Attachments } from "../components/Attachments";
import { CommentRow } from "../components/CommentRow";
import { inTauri, type Decision, type DecisionStatus } from "../core/snapshot";
import {
  acceptDecision, addDecisionComment, amendDecision, buildsOnDecision, editDecision, editDecisionComment,
  rejectDecision, reopenDecision, removeDecisionComment, supersedeDecision, unlinkDecisionEdge,
} from "../core/mutations";
import { useDecision, useDecisionComments, useDecisionPage } from "../core/reads";
import {
  EDGE_KINDS, edgeCandidates, edgeRows, promotesToAccepted, standingOn, type EdgeKind, type EdgeRow,
} from "../core/decisionEdges";
import { confirmDialog } from "../core/dialog";
import { isClosed } from "../core/status";
import { isEnterSubmit } from "../core/keys";
import { errText, statusLabel, t, tf } from "../core/i18n";
import { decisionRef } from "../core/idref";

// Colour of the status badge — keep it matching DecisionsScreen's statusColor. The badge says the status
// and nothing else (`AMB-D-410`); that this decision was overturned is an edge, and the edge list below
// is where it is read.
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
export function DecisionDetailPane({
  decisionId, onOpenTask, onOpenDecision, focusCommentAt, editCommentAt,
}: {
  decisionId: number;
  onOpenTask?: (id: number) => void;
  /** Opens the decision on the other end of an edge — superseded, amended or built on (mirrors onOpenTask). */
  onOpenDecision?: (id: number) => void;
  /** Nonce that focuses the comment box when opened via "↩ reply" in the activity feed. Every increment re-focuses, so you can reply to the same decision again and again. undefined = an ordinary selection. */
  focusCommentAt?: number;
  /** The comment to open in edit mode when opened via "✎" in the activity feed. The nonce lets the same comment be re-opened. The thread is drawn whole here, so unlike the task pane there is no window to widen first. */
  editCommentAt?: { commentId: number; nonce: number };
}) {
  const d = useDecision(decisionId);
  // The comment thread exists only under Tauri (the browser mock has no decisions); posting refetches via the WriteAck.
  const comments = useDecisionComments(inTauri() ? decisionId : null);
  const [comment, setComment] = useState("");
  const [confirming, setConfirming] = useState<null | "accept" | "reject">(null);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [commentError, setCommentError] = useState<string | null>(null);
  const [reopenError, setReopenError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Editing the title/body in place (proposed and accepted alike). One form drives the single `editDecision`
  // write; a rejected decision is terminal (core refuses it), so the edit affordance is hidden there.
  const [editing, setEditing] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [bodyDraft, setBodyDraft] = useState("");
  const [editError, setEditError] = useState<string | null>(null);
  const [editBusy, setEditBusy] = useState(false);
  // "↩ reply" from the activity feed. The focus cannot land at request time — the textarea is not rendered until
  // the decision has loaded — so the ask is held as a flag and spent on the render that produces the box.
  const commentRef = useRef<HTMLTextAreaElement>(null);
  const pendingCommentFocus = useRef(false);
  useEffect(() => {
    if (focusCommentAt === undefined) return;
    pendingCommentFocus.current = true;
  }, [focusCommentAt]);
  useEffect(() => {
    if (!pendingCommentFocus.current) return;
    const el = commentRef.current;
    if (!el) return; // Not rendered yet (the decision is still loading) — land it on the next render.
    pendingCommentFocus.current = false;
    el.focus();
    el.scrollIntoView?.({ block: "nearest" });
  });
  if (!d) return <div className="rightpane__empty">{t("dec.notFound")}</div>;

  const editable = d.status !== "rejected";
  const startEdit = () => {
    setTitleDraft(d.title);
    setBodyDraft(d.body ?? "");
    setEditError(null);
    setEditing(true);
  };
  // Await the write and close only once it lands: a refused edit must not read as a success. On failure the
  // drafts stay put, so retrying costs nothing. An unchanged form just closes (no write, no needless emit).
  const saveEdit = async () => {
    const title = titleDraft.trim();
    if (!title) return;
    if (title === d.title && bodyDraft === (d.body ?? "")) { setEditing(false); return; }
    setEditBusy(true);
    setEditError(null);
    try {
      await editDecision(d.id, title, bodyDraft);
      setEditing(false);
    } catch (e) {
      setEditError(errText(e));
    } finally {
      setEditBusy(false);
    }
  };

  // Await the post and only clear the box once it lands: a refused comment used to blank the input, losing the
  // body the user just wrote. On failure the text stays put and the error is shown, so retrying costs nothing.
  const submitComment = async () => {
    const body = comment.trim();
    if (!body) return;
    setCommentError(null);
    try {
      await addDecisionComment(d.id, body);
      setComment("");
    } catch (e) {
      setCommentError(errText(e));
    }
  };
  // Reopening (accepted → proposed) is a write like any other — surface a refusal instead of dropping it.
  const runReopen = async () => {
    setReopenError(null);
    try {
      await reopenDecision(d.id);
    } catch (e) {
      setReopenError(errText(e));
    }
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
        {editing ? (
          <input
            className="compose__input"
            style={{ minHeight: "unset", fontSize: "inherit", fontWeight: "inherit", flex: 1 }}
            autoFocus
            value={titleDraft}
            placeholder={t("dec.newTitlePh")}
            onChange={(e) => setTitleDraft(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); setEditing(false); } }}
          />
        ) : (
          <span style={editable ? { cursor: "text" } : undefined} title={editable ? t("detail.edit") : undefined} onDoubleClick={editable ? startEdit : undefined}>{d.title}</span>
        )}
        <span style={{
          fontSize: "var(--fs-xs)", padding: "1px 8px", borderRadius: 10, color: "#fff",
          background: statusColor(d.status),
        }}>{t(`dec.status.${d.status}`)}</span>
        {!editing && editable && (
          <button className="feed__action" style={{ marginLeft: 6 }} onClick={startEdit}>{t("detail.edit")}</button>
        )}
      </div>

      {editing ? (
        <div className="compose" style={{ marginTop: 8, maxWidth: "var(--measure-prose)" }}>
          <textarea
            className="compose__input"
            rows={8}
            value={bodyDraft}
            placeholder={t("dec.newBodyPh")}
            onChange={(e) => setBodyDraft(e.target.value)}
            onKeyDown={(e) => {
              if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void saveEdit(); }
              if (e.key === "Escape") setEditing(false);
            }}
          />
          {d.status === "accepted" && (
            <div className="faint" style={{ fontSize: "var(--fs-sm)", marginTop: 4 }}>{t("dec.editAcceptedHint")}</div>
          )}
          {editError && <div className="newproj__error" role="alert">⚠ {editError}</div>}
          <div className="compose__actions">
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.notesHint")}</span>
            <span>
              <button className="btn" onClick={() => setEditing(false)}>{t("detail.cancel")}</button>
              <button className="btn btn--primary" style={{ marginLeft: 6 }} disabled={editBusy || !titleDraft.trim()} onClick={() => void saveEdit()}>{t("detail.save")}</button>
            </span>
          </div>
        </div>
      ) : d.body ? (
        <div className="markdown" style={{ marginTop: 8, fontSize: "var(--fs-body)", maxWidth: "var(--measure-prose)" }} onDoubleClick={editable ? startEdit : undefined}>
          <Markdown>{d.body}</Markdown>
        </div>
      ) : null}

      <DecisionEdges d={d} onOpenDecision={onOpenDecision} />

      {d.linkedTasks.length > 0 && (
        <div style={{ marginTop: 8, fontSize: "var(--fs-sm)" }}>
          {t("dec.linkedTasks")}:{" "}
          {d.linkedTasks.map((lt, i) => (
            <span key={lt.id} style={isClosed(lt.status) ? { color: "var(--c-muted)" } : undefined}>
              {i > 0 && ", "}
              <button
                className="feed__action"
                style={{ padding: "0 4px", ...(isClosed(lt.status) ? { opacity: 0.6 } : {}) }}
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
          // Entry buttons are real buttons, not link-styled feed__action: accepting or rejecting a
          // decision is the pane's primary act and must not hide among the faint navigation links.
          <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
            <button className="btn btn--primary" onClick={() => setConfirming("accept")}>{t("dec.accept")}</button>
            <button className="btn btn--danger" onClick={() => setConfirming("reject")}>{t("dec.reject")}</button>
          </div>
        )
      )}

      {d.status === "accepted" && (
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 4 }}>
          <div style={{ display: "flex", gap: 8 }}>
            <button className="feed__action" onClick={() => void runReopen()}>{t("dec.reopen")}</button>
          </div>
          {reopenError && <div className="newproj__error" role="alert">⚠ {reopenError}</div>}
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
                    at={c.at}
                    editedAt={c.editedAt}
                    text={c.text}
                    target="decision_comment"
                    onEdit={(text) => editDecisionComment(c.id, d.id, text)}
                    onRemove={() => removeDecisionComment(c.id, d.id)}
                    startEditAt={editCommentAt?.commentId === c.id ? editCommentAt.nonce : undefined}
                  />
                ))}
              </div>
            )}
            <div className="compose">
              <textarea
                ref={commentRef}
                className="compose__input"
                rows={3}
                placeholder={t("detail.commentPh")}
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                onKeyDown={(e) => {
                  if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void submitComment(); }
                }}
              />
              {commentError && <div className="newproj__error" role="alert">⚠ {commentError}</div>}
              <div className="compose__actions">
                <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.commentHint")}</span>
                <button className="btn btn--primary" disabled={!comment.trim()} onClick={() => void submitComment()}>{t("detail.send")}</button>
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
// decision itself — drop a supersedes and what is gone is the claim that one replaced the other.
function DecisionEdges({ d, onOpenDecision }: {
  d: Decision;
  onOpenDecision?: (id: number) => void;
}) {
  const rows = edgeRows(d);
  const [error, setError] = useState<string | null>(null);
  const unlink = async (r: EdgeRow) => {
    const target = r.target.ref ?? decisionRef(r.target.id);
    if (await confirmDialog(tf("dec.edge.unlinkConfirm", { target }))) {
      setError(null);
      try {
        await unlinkDecisionEdge(r.from, r.to);
      } catch (e) {
        setError(errText(e));
      }
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
      {error && <div className="newproj__error" role="alert">⚠ {error}</div>}
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
  const [error, setError] = useState<string | null>(null);
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
    // Await the wiring: a refused edge must not close the picker as though it landed. On failure the picker
    // stays open with the error, so the user can retry or pick a different target.
    setError(null);
    try {
      if (kind === "supersedes") await supersedeDecision(d.id, target.id);
      else if (kind === "amends") await amendDecision(d.id, target.id);
      else await buildsOnDecision(d.id, target.id);
      setOpen(false);
      setQuery("");
    } catch (e) {
      setError(errText(e));
    }
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
      {error && <div className="newproj__error" role="alert">⚠ {error}</div>}
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
