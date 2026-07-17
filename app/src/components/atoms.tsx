import { useState } from "react";
import type { Actor, Priority, Status, TaskCard } from "../mock/types";
import { currentLang, priorityLabel, statusLabel, t, tf } from "../core/i18n";
import { getSnapshot } from "../core/snapshot";
import { taskRef } from "../core/idref";
import { Identicon } from "./identicon";

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

const STATUS_VAR: Record<Status, string> = {
  todo: "var(--c-todo)",
  in_progress: "var(--c-progress)",
  done: "var(--c-done)",
  blocked: "var(--c-blocked)",
};
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

/** A distinct identicon seed per facet. In a single store `userId` is always the person token ("human"), so we mix in
 * kind to force human and AI to render differently: they stay tellable apart with no badge, even with no AI icon set. */
function identiconSeed(actor: Actor) {
  return `${actor.userId}:${actor.kind}`;
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
    <span className="facet" title={`${actor.name}（${isAi ? t("facet.ai") : t("facet.human")}）`}>
      <span className="facet__base" style={{ borderColor: facetColor(actor.kind) }}>
        {avatar
          ? <img className="facet__img" src={avatar} alt="" width={18} height={18} />
          : <Identicon seed={identiconSeed(actor)} size={18} />}
      </span>
      {showName && <span className="facet__name">{actor.name}</span>}
    </span>
  );
}

export function StatusBadge({ status }: { status: Status }) {
  return (
    <span className="statusbadge">
      <span className="statusbadge__dot" style={{ background: STATUS_VAR[status] }} />
      {statusLabel(status)}
    </span>
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

export function DueChip({ due, label }: { due: string | null; label: string | null }) {
  if (!due) return null;
  const today = "2026-06-21";
  const cls = due < today ? "due--overdue" : due === today ? "due--today" : "due--future";
  return <span className={`chip due ${cls}`}>🗓 {label ?? due}</span>;
}

/**
 * The chip that names, in the list itself, the premises blocking a reservation (`ready === false`) before anyone
 * tries to start. ⛔ = an unfinished dependency blocker; ⚠ = a decision not yet settled as grounds. The reason a
 * reservation was refused only ever appears in a toast that vanishes in 4 seconds, so this is the one permanent place
 * it is visible before starting. It is a derived inability to start, on a different axis from a stop a person
 * declared (`status = blocked`, the StatusBadge's `--c-blocked`), so it speaks in glyphs rather than colour and never
 * blends into the status colour range. `compact` is for the dense surfaces where the chip shares one line with a row
 * label (calendar, timeline): it drops the count and the chip background and leaves just the glyph (the tooltip names
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
          {compact ? "⛔" : `⛔ ${deps.length}`}
        </span>
      )}
      {decisions.length > 0 && (
        <span
          className={cls}
          role="img"
          title={tf("block.decisions", { refs })}
          aria-label={tf("block.decisions", { refs })}
        >
          {compact ? "⚠" : `⚠ ${decisions.length}`}
        </span>
      )}
    </>
  );
}

/**
 * Inbox only: the chip showing when the activity that put this in the inbox last happened. `at` is RFC3339 UTC; null
 * (time unknown) renders nothing. Formatted as month/day plus time, in the locale that matches the current language.
 */
export function TriggeredAtChip({ at }: { at?: string | null }) {
  if (!at) return null;
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return null;
  const locale = currentLang() === "ja" ? "ja-JP" : "en-US";
  const label = new Intl.DateTimeFormat(locale, {
    month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit",
  }).format(d);
  return <span className="chip" title={at}>⏱ {label}</span>;
}

