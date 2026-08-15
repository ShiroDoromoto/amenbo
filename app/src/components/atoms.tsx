import { useState } from "react";
import { createPortal } from "react-dom";
import type { Actor, Priority, Status, TaskCard } from "../mock/types";
import type { PremiseChangeDto } from "../bindings/bindings";
import { dueKind, todayStr } from "../core/calendar";
import { dueLabel, formatDayTime, formatNumber, priorityLabel, statusLabel, t, tf } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { getSnapshot } from "../core/snapshot";
import { pushNotice } from "../core/notice";
import { STATUS_ALL } from "../core/status";
import { taskRef } from "../core/idref";
import { Identicon } from "./identicon";
import { Icon } from "./Icon";

/**
 * The chip a task is called by. The id is the conversational number itself, so what we show and what we copy are the
 * same single ref — `AMB-T-<n>` — and clicking it copies that ref. Copying the namespaced form, not the bare
 * number, is the point: what leaves here gets pasted into commits and PRs, where a bare number names nothing.
 */
export function TaskIdChip({ id }: { id: number }) {
  const [copied, setCopied] = useState(false);
  const label = taskRef(id);
  const copy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(label);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch { /* where the clipboard is unavailable, quietly skip */ }
  };
  return (
    <button type="button" className="idchip" onClick={copy} title={t("id.copyTip")}>
      {copied ? t("id.copied") : label}
    </button>
  );
}

const PRIORITY_COLOR: Record<Priority, string> = {
  high: "var(--c-pri-high)",
  medium: "var(--c-pri-med)",
  low: "var(--c-pri-low)",
};

export function facetColor(kind: "human" | "ai") {
  return kind === "ai" ? "var(--c-ai)" : "var(--c-human)";
}
export function facetGlyph(kind: "human" | "ai") {
  return kind === "ai" ? "🤖" : "👤";
}

/** A distinct identicon seed per facet. The two facets (human / ai) seed off their kind, so they render as different
 * glyphs — tellable apart with no badge, even with no AI icon set. */
export function identiconSeed(actor: Actor) {
  return actor.kind;
}

// The avatar is the facet's registered image (config human_avatar / ai_avatar) when set,
// otherwise a deterministic identicon seeded per facet so human and AI read as distinct
// without any upload, server, or badge. The facet is conveyed by the ring colour
// (--c-human / --c-ai); the registered AI face or the AI-seeded identicon carries the AI
// identity, so no 🤖 glyph is layered on. Faces come from config and only ride on the
// roster actor (facet_actor keeps assignee/author DTOs face-less to stay light), so
// when the given actor has no avatar of its own we resolve it by kind from the roster.
export function FacetAvatar({ actor, showName }: { actor: Actor; showName?: boolean }) {
  const isAi = actor.kind === "ai";
  const avatar = actor.avatar
    ?? getSnapshot().roster.find((a) => a.kind === actor.kind)?.avatar;
  return (
    <span className="facet" title={tf("facet.named", { name: actor.name, facet: t(isAi ? "facet.ai" : "facet.human") })}>
      <span className="facet__base" style={{ borderColor: facetColor(actor.kind) }}>
        {avatar
          ? <img className="facet__img" src={avatar} alt="" width={18} height={18} />
          : <Identicon seed={identiconSeed(actor)} size={18} />}
      </span>
      {showName && <span className="facet__name">{actor.name}</span>}
    </span>
  );
}

/**
 * The one control that changes a task's status. Every surface offering the change — board card, list row,
 * inbox row — mounts this, so every value stays reachable everywhere and no surface can express the axis
 * as a two-value toggle (a toggle has to pick a landing status for the user, and picking `todo` silently
 * discards an `in_progress` reservation). It shows the current status by being set to it, so a row carrying
 * this needs no separate StatusBadge — which is also how the two terminals tell themselves apart, both
 * being struck through. It stops propagation itself: it always sits inside a row or card whose own click
 * selects the task, and changing status must not double as selecting.
 *
 * `rejected` is the one option that does not write on being picked: it asks for the reason first
 * ({@link RejectReasonModal}), and hands it on with the status. Cancelling writes nothing, and the
 * control snaps back to the status it is set to.
 */
export function StatusSelect({ id, status, onStatus, premiseChange, className = "inlineselect" }: {
  id: number;
  status: Status;
  // The reason rides along with the status because one status requires it: `rejected` is refused
  // without it, both here and in the write layer (`AMB-D-397`). It is absent for every other value.
  onStatus: (id: number, status: Status, reason?: string) => void;
  // The holder-side safety net of `AMB-D-366`: the premises pinned on after this task was reserved, if any.
  // Leaving `in_progress` (finishing, blocking) is the moment that must not be missed — so on that
  // transition, with a change present, a firm toast fires before the change is handed on. The transition is
  // never blocked (surface, not veto): the holder may still ship the part that stands on its own.
  premiseChange?: PremiseChangeDto | null;
  // The surfaces differ in how the control is dressed — compact among a card's chips, a full button in the
  // detail pane's action row — but it is one control, so the styling is the only thing a caller may vary.
  className?: string;
}) {
  const [rejecting, setRejecting] = useState(false);
  const commit = (next: Status, reason?: string) => {
    if (status === "in_progress" && next !== "in_progress" && premiseChange) {
      pushNotice(tf("premise.warn", { detail: premiseChangeDetail(premiseChange) }));
    }
    onStatus(id, next, reason);
  };
  // Picking `rejected` opens the question instead of writing: the toast above fires on the way to the
  // write, so it must not go off for a rejection that is still being reconsidered.
  const change = (next: Status) => {
    if (next === "rejected") setRejecting(true);
    else commit(next);
  };
  return (
    <>
      <select
        className={className}
        value={status}
        title={t("status.changeTip")}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => { e.stopPropagation(); change(e.target.value as Status); }}
      >
        {STATUS_ALL.map((s) => (
          <option key={s} value={s}>{statusLabel(s)}</option>
        ))}
      </select>
      {rejecting && (
        <RejectReasonModal
          id={id}
          onCancel={() => setRejecting(false)}
          onReject={(reason) => { setRejecting(false); commit("rejected", reason); }}
        />
      )}
    </>
  );
}

/**
 * What the pull-down asks before a task is rejected: why it will not be done. The reason is **required**,
 * and this is the surface that makes it so — the confirm button is dead until something is typed, so there
 * is no path from the pull-down to `rejected` that skips it (`AMB-D-397`; the CLI's `--reason` is required
 * for the same reason, and the command refuses an empty one whichever door it came through).
 *
 * It is a modal and not a native `prompt()`, which the Tauri webview does not implement (see `core/dialog`,
 * where the confirmation dialog had to go native for the same reason — there is no native text prompt to
 * delegate to). Esc and the cancel button both write nothing.
 */
function RejectReasonModal({ id, onCancel, onReject }: {
  id: number;
  onCancel: () => void;
  onReject: (reason: string) => void;
}) {
  const [text, setText] = useState("");
  const reason = text.trim();
  const submit = () => { if (reason) onReject(reason); };
  return createPortal(
    <div
      className="rejectask__overlay"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) onCancel(); }}
    >
      <div className="rejectask__modal" role="dialog" aria-modal="true" aria-labelledby="rejectask-title">
        <div className="rejectask__title" id="rejectask-title">{tf("reject.title", { ref: taskRef(id) })}</div>
        <div className="rejectask__why">{t("reject.why")}</div>
        <textarea
          {...asTyped}
          className="rejectask__input"
          autoFocus
          rows={4}
          value={text}
          placeholder={t("reject.placeholder")}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            // ⌘/Ctrl+Enter submits, as every other multi-line body in the app does — a bare Enter is a
            // newline, and a reason worth keeping is often more than one line.
            if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submit(); }
            if (e.key === "Escape") onCancel();
          }}
        />
        <div className="rejectask__actions">
          <button className="rejectask__action rejectask__action--go" disabled={!reason} onClick={submit}>
            {t("reject.confirm")}
          </button>
          <button className="rejectask__action" onClick={onCancel}>{t("reject.cancel")}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

export function PriorityDot({ priority }: { priority: Priority | null }) {
  if (!priority) return null;
  return (
    <span className="chip" style={{ color: PRIORITY_COLOR[priority] }}>
      ● {priorityLabel(priority)}
    </span>
  );
}

/**
 * The due date, coloured by how it stands against the device's own today (`todayStr()` at render, the same
 * source the calendar and timeline colour by). The label beside it is worded by the backend against the same
 * real day, so colour and wording have to be read off one clock — a colour judged against any other day says
 * "overdue" next to a label that says "tomorrow".
 */
export function DueChip({ due }: { due: string | null }) {
  if (!due) return null;
  const cls = `due--${dueKind(due, todayStr())}`;
  return <span className={`chip due ${cls}`}><Icon name="calendar" /> {dueLabel(due)}</span>;
}

/**
 * The chip that names, in the list itself, the premises blocking a reservation (`ready === false`) before anyone
 * tries to start. A barred circle = an unfinished dependency blocker; a warning triangle = a decision not yet
 * settled as grounds; an hourglass = a declared start day that has not come; a pencil = the creation is not
 * finished (`AMB-D-555`: a task still being written is on the board like any other, and what keeps it from
 * being picked up is the premise, not being hidden). The fourth one
 * carries a word rather than a count — there is nothing to count, and "still being created" is the whole fact.
 * Every `ready === false` has at least one of the four, so the chip row is never empty
 * where a reason exists — an unexplained "cannot start" reads as no reason at all. The reason a
 * reservation was refused only ever appears in a toast that vanishes in 4 seconds, so this is the one permanent place
 * it is visible before starting. It is a derived inability to start, on a different axis from a stop a person
 * declared (`status = blocked`), so it speaks in marks rather than colour and never blends into the status
 * colour range. `compact` is for the dense surfaces where the chip shares one line with a row
 * label (calendar, timeline): it drops the count and the chip background and leaves just the mark (the tooltip names
 * what is blocking).
 */
export function BlockedChips({ task, compact = false }: { task: TaskCard; compact?: boolean }) {
  const deps = task.blockedBy ?? [];
  const decisions = task.blockedByDecisions ?? [];
  if (task.ready) return null;
  const names = deps.map((b) => `${taskRef(b.id)} ${b.name}`).join(", ");
  const refs = decisions.map((d) => `${d.ref ?? ""} ${d.name}`.trim()).join(", ");
  const cls = compact ? "chip--blockglyph" : "chip chip--block";
  return (
    <>
      {deps.length > 0 && (
        <span
          className={cls}
          role="img"
          title={tf("block.deps", { names })}
          aria-label={tf("block.deps", { names })}
        >
          <Icon name="blocked" />{compact ? null : ` ${formatNumber(deps.length)}`}
        </span>
      )}
      {decisions.length > 0 && (
        <span
          className={cls}
          role="img"
          title={tf("block.decisions", { refs })}
          aria-label={tf("block.decisions", { refs })}
        >
          <Icon name="warning" />{compact ? null : ` ${formatNumber(decisions.length)}`}
        </span>
      )}
      {task.notStartedUntil && (
        <span
          className={cls}
          role="img"
          title={tf("block.notStarted", { date: task.notStartedUntil })}
          aria-label={tf("block.notStarted", { date: task.notStartedUntil })}
        >
          {/* The date, not a count: one start day is never plural, and the day itself is the fact. */}
          <Icon name="hourglass" />{compact ? null : ` ${task.notStartedUntil}`}
        </span>
      )}
      {task.draft && (
        <span
          className={cls}
          role="img"
          title={t("block.draft")}
          aria-label={t("block.draft")}
        >
          <Icon name="pencil" />{compact ? null : ` ${t("chip.draft")}`}
        </span>
      )}
    </>
  );
}

/** How a decision is named where a premise change lists one: the ref plus its title. */
export function premiseDecisionName(d: PremiseChangeDto["addedDecisions"][number]): string {
  return `${d.ref ?? ""} ${d.name ?? t("dec.unknownName")}`.trim();
}

/**
 * What changed under the holder, as one line — the text both premise-change surfaces show (the chip's
 * tooltip, and the firm toast fired on leaving `in_progress`). It lives here, and not inline in each, so the
 * two cannot come to name different subsets of the same change: the reopen axis (`AMB-D-373`) was once in
 * the chip and missing from the toast. The two axes read differently, so the decisions that *stopped being
 * settled* carry a tag — without it they are indistinguishable from the ones newly pinned on.
 */
export function premiseChangeDetail(pc: PremiseChangeDto): string {
  const named = (ds: PremiseChangeDto["addedDecisions"]) => ds.map(premiseDecisionName).join(", ");
  const blockers = pc.addedBlockers.map((b) => `${taskRef(b.id)} ${b.name}`).join(", ");
  const reopened = pc.reopenedDecisions.length > 0
    ? `${t("premise.noLongerSettled")}: ${named(pc.reopenedDecisions)}`
    : "";
  return [blockers, named(pc.addedDecisions), reopened].filter(Boolean).join(" / ");
}

/**
 * The holder-side surface of `AMB-D-366` and `AMB-D-373`: a chip on the row of a task whose premises shifted
 * **after it was reserved** — a blocker or an unsettled decision pinned on since it went `in_progress`, or a
 * decision that was already linked and has stopped being settled — each silently withdrawing readiness.
 * A bell = "something changed under your reservation"; the tooltip names what. It sits beside
 * {@link BlockedChips} but reads a different axis — not "why it cannot start" (a live derivation for anyone)
 * but "what changed since *you* took it" (only the holder is at risk) — so it speaks in its own mark. Core
 * only ever fills `premiseChange` for an `in_progress` task that actually acquired one, so the chip draws
 * exactly when it matters and nothing renders otherwise. `compact` drops the count for the dense surfaces,
 * matching `BlockedChips`.
 */
export function PremiseChangedChip({ task, compact = false }: { task: TaskCard; compact?: boolean }) {
  const pc = task.premiseChange;
  if (!pc) return null;
  const detail = premiseChangeDetail(pc);
  const count = pc.addedBlockers.length + pc.addedDecisions.length + pc.reopenedDecisions.length;
  const cls = compact ? "chip--blockglyph" : "chip chip--premise";
  return (
    <span className={cls} role="img" title={tf("premise.changed", { detail })} aria-label={tf("premise.changed", { detail })}>
      <Icon name="bell" />{compact ? null : ` ${formatNumber(count)}`}
    </span>
  );
}

/**
 * The same fact as {@link PremiseChangedChip}, spelled out: the detail pane's field naming every premise that
 * moved under the holder, each a chip that navigates to it. It reads a different axis from `blockedBy` (why
 * anyone cannot start it) — here it is what changed since *this* holder took it — so it is its own field,
 * permanent beside the transient toast the safety net fires at status change. The two decision axes are drawn
 * apart: a warning triangle for a premise **pinned on** after the reservation, an open padlock for one
 * already linked whose settlement **came off** (`AMB-D-373`) — one mark for both would leave the reader
 * unable to tell which to go and look at. It lives here rather than in the pane so the axes it draws stay tied to the chip's, and adding one
 * cannot again land in a single surface.
 */
export function PremiseChangedField({ pc, onSelectTask, onSelectDecision }: {
  pc: PremiseChangeDto;
  onSelectTask?: (id: number) => void;
  onSelectDecision?: (id: number) => void;
}) {
  return (
    <div className="detail__field">
      <span className="detail__flabel">{t("detail.premiseChanged")}</span>
      <span title={t("detail.premiseChangedHint")}>
        <Icon name="bell" />{" "}
        {pc.addedBlockers.map((b) => (
          <button
            type="button"
            className="chip chip--link"
            key={`b${b.id}`}
            style={{ marginRight: 4 }}
            title={t("detail.premiseAdded")}
            onClick={() => onSelectTask?.(b.id)}
          >
            <Icon name="blocked" /> {b.name}
          </button>
        ))}
        {pc.addedDecisions.map((d) => (
          <button
            type="button"
            className="chip chip--link"
            key={`d${d.id}`}
            style={{ marginRight: 4 }}
            title={t("detail.premiseAdded")}
            onClick={() => onSelectDecision?.(d.id)}
          >
            <Icon name="warning" /> {premiseDecisionName(d)}
          </button>
        ))}
        {pc.reopenedDecisions.map((d) => (
          <button
            type="button"
            className="chip chip--link"
            key={`r${d.id}`}
            style={{ marginRight: 4 }}
            title={t("detail.premiseReopened")}
            onClick={() => onSelectDecision?.(d.id)}
          >
            <Icon name="unlock" /> {premiseDecisionName(d)}
          </button>
        ))}
      </span>
    </div>
  );
}

/**
 * Inbox only: the chip showing when the activity that put this in the inbox last happened. `at` is RFC3339 UTC; null
 * (time unknown) renders nothing. Formatted as month/day plus time, in the locale dates are written
 * in (`dateLocale`).
 */
export function TriggeredAtChip({ at }: { at?: string | null }) {
  if (!at) return null;
  const label = formatDayTime(new Date(at));
  if (!label) return null;
  return <span className="chip" title={at}><Icon name="clock" /> {label}</span>;
}

