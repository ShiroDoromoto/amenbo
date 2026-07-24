import { useEffect, useRef, useState } from "react";
import { useStore } from "../store/store";
import { FacetAvatar } from "../components/atoms";
import { t } from "../core/i18n";
import { loadActivityPage } from "../core/activity";
import { inTauri } from "../core/snapshot";
import { confirmDialog } from "../core/dialog";
import {
  matchesActivityFilter,
  type ActivityKindFilter,
  type ActivityFacetFilter,
} from "../core/activityFilter";
import type { ActivityItem } from "../mock/types";

// Windowing: history runs to thousands of rows, so the DOM holds only the viewport plus overscan.
// Row height varies (rows can wrap), but an estimated ROW_H with a generous OVERSCAN always fills the
// viewport (the scrollbar thumb is then only approximate). PAGE is one page of core's `activity_page`.
const ROW_H = 60;
const OVERSCAN = 8;
const PAGE = 100;

/**
 * The activity feed (history across every project). A row opens whatever it names, as long as that is still there:
 * tasks and decisions both, since a decision row is either a deletion or a comment on a live decision. A project row is
 * always a deletion (the ledger names one nowhere else), so it has nothing to open. The reply / edit / remove actions
 * follow the same two kinds — a comment thread hangs off a task or off a decision, and the row does not care which.
 * Comments are not edited on this surface: a row is a one-line summary, and a multi-line editor would change its height
 * and wreck the virtual scroller's row-height estimate. Editing, like replying, opens the detail pane with that comment
 * in edit mode. Filtering picks independently on two axes: kind (system / comment) × facet (human / AI); both default
 * to all.
 */
export function ActivityFeed({
  onOpenTask,
  onOpenDecision,
  onReplyToTask,
  onReplyToDecision,
  onEditComment,
  onEditDecisionComment,
}: {
  onOpenTask: (id: number) => void;
  onOpenDecision: (id: number) => void;
  // A reply lands on the target's timeline (there are no per-comment threads): open the detail and focus the comment box.
  onReplyToTask: (id: number) => void;
  onReplyToDecision: (id: number) => void;
  onEditComment: (taskId: number, commentId: number) => void;
  onEditDecisionComment: (decisionId: number, commentId: number) => void;
}) {
  const store = useStore();
  // seed = the newest 100 rows from the snapshot (swapped out by the subscription). Older pages are appended to it.
  const seed = store.listActivity();
  const [older, setOlder] = useState<ActivityItem[]>([]);
  const [exhausted, setExhausted] = useState(!inTauri()); // The browser mock has no history to page back into.
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportH, setViewportH] = useState(600);
  const [kindFilter, setKindFilter] = useState<ActivityKindFilter>("all");
  const [facetFilter, setFacetFilter] = useState<ActivityFacetFilter>("all");
  const loadingRef = useRef(false);
  const scrollerRef = useRef<HTMLDivElement>(null);

  // When another process appends rows the seed shifts, moving the boundary between seed and older, so the pages
  // already appended are dropped and the history is rebuilt. The head id detects that the seed changed.
  const seedKey = `${seed.length}:${seed[0]?.id ?? ""}`;
  useEffect(() => {
    setOlder([]);
    setExhausted(!inTauri());
  }, [seedKey]);

  // raw = everything loaded so far (the basis for the paging offset); items = what survives the filter (the basis for windowing).
  const raw = older.length ? dedupById(seed.concat(older)) : seed;
  const noFilter = kindFilter === "all" && facetFilter === "all";
  const items = noFilter ? raw : raw.filter((it) => matchesActivityFilter(it, kindFilter, facetFilter));
  const total = items.length;

  // Fetch and append the next page of older rows. The offset counts raw, unfiltered rows: under a filter the loaded
  // count and the shown count diverge, and offsetting by the shown count would refetch the same range forever.
  const loadMore = () => {
    if (exhausted || loadingRef.current) return;
    loadingRef.current = true;
    void loadActivityPage(raw.length, PAGE).then((page) => {
      loadingRef.current = false;
      if (page.length < PAGE) setExhausted(true);
      if (page.length) setOlder((prev) => prev.concat(page));
    });
  };

  // Append once the window nears the tail.
  const maybeLoadMore = (endIdx: number) => {
    if (endIdx >= total - OVERSCAN) loadMore();
  };

  useEffect(() => {
    if (scrollerRef.current) setViewportH(scrollerRef.current.clientHeight);
  }, []);

  // While a strict filter leaves too little to fill the viewport, keep pulling older pages until they run out — a
  // sparse filter must not look like "too short to scroll, so that's all there is". Reruns only as total/viewportH move.
  useEffect(() => {
    if (!exhausted && !loadingRef.current && total * ROW_H <= viewportH) loadMore();
  }, [total, viewportH, exhausted, raw.length]);

  const startIdx = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN);
  const windowLen = Math.ceil(viewportH / ROW_H) + OVERSCAN * 2;
  const endIdx = Math.min(total, startIdx + windowLen);
  const padTop = startIdx * ROW_H;
  const padBottom = Math.max(0, (total - endIdx) * ROW_H);
  const windowItems = items.slice(startIdx, endIdx);

  const openTarget = (it: ActivityItem): (() => void) | null => {
    if (!it.target.live) return null; // A deleted target stays in the ledger but has nowhere to go.
    if (it.target.type === "task") return () => onOpenTask(it.target.id);
    if (it.target.type === "decision") return () => onOpenDecision(it.target.id);
    return null; // A project row is always a deletion, so it never reaches here live.
  };

  // Whether this row's comment can be acted on: a comment hanging off a live task or a live decision. A comment whose
  // target is gone has no thread left to reply to, edit in, or delete from — the row is history and nothing else.
  const commentOn = (it: ActivityItem): "task" | "decision" | null =>
    it.kind === "comment" && it.target.live && it.target.type !== "project" ? it.target.type : null;

  const remove = async (it: ActivityItem, kind: "task" | "decision") => {
    if (!(await confirmDialog(t("comment.removeConfirm")))) return;
    if (kind === "task") store.removeComment(it.id, it.target.id);
    else store.removeDecisionComment(it.id, it.target.id);
  };

  return (
    <>
      <div className="board__toolbar">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("activity.filterKind")}</span>
        <button className={chipCls(kindFilter === "all")} onClick={() => setKindFilter("all")}>{t("activity.filterAll")}</button>
        <button className={chipCls(kindFilter === "system")} onClick={() => setKindFilter("system")}>{t("activity.filterSystem")}</button>
        <button className={chipCls(kindFilter === "comment")} onClick={() => setKindFilter("comment")}>{t("activity.filterComment")}</button>
        <div className="board__sep" />
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("activity.filterFacet")}</span>
        <button className={chipCls(facetFilter === "all")} onClick={() => setFacetFilter("all")}>{t("activity.filterAll")}</button>
        <button className={chipCls(facetFilter === "human")} onClick={() => setFacetFilter("human")}>{t("activity.filterHuman")}</button>
        <button className={chipCls(facetFilter === "ai")} onClick={() => setFacetFilter("ai")}>{t("activity.filterAi")}</button>
        <div className="topbar__spacer" />
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("activity.note")}</span>
      </div>
      <div
        ref={scrollerRef}
        className="feed feed--virtual"
        onScroll={(e) => {
          const el = e.currentTarget;
          setScrollTop(el.scrollTop);
          setViewportH(el.clientHeight);
          const end = Math.min(total, Math.floor(el.scrollTop / ROW_H) + Math.ceil(el.clientHeight / ROW_H) + OVERSCAN);
          maybeLoadMore(end);
        }}
      >
        <div style={{ height: padTop }} />
        {windowItems.map((it) => {
          const open = openTarget(it);
          const actsOn = commentOn(it);
          return (
            <div key={it.id} className="feed__item">
              <FacetAvatar actor={it.author} />
              <div className="feed__body">
                <div className="feed__line">
                  <strong>{it.author.name}</strong>{" "}
                  {it.kind === "comment" ? `「${it.text}」` : it.event?.text}
                  {it.burstCount ? <span className="faint"> ⌄</span> : null}
                </div>
                <div className="feed__meta">
                  <span>{it.ago}{it.editedAgo && <span className="faint"> · {t("comment.edited")} {it.editedAgo}</span>}</span>
                  {open ? (
                    <button className="feed__target" onClick={open}>
                      → {it.target.title}
                    </button>
                  ) : (
                    <span className="feed__target feed__target--gone">→ {it.target.title}</span>
                  )}
                  {actsOn && (
                    <button
                      className="feed__action"
                      onClick={() =>
                        actsOn === "task" ? onReplyToTask(it.target.id) : onReplyToDecision(it.target.id)
                      }
                    >
                      ↩ {t("activity.reply")}
                    </button>
                  )}
                  {inTauri() && actsOn && (
                    <>
                      <button
                        className="feed__action"
                        title={t("comment.edit")}
                        onClick={() =>
                          actsOn === "task"
                            ? onEditComment(it.target.id, it.id)
                            : onEditDecisionComment(it.target.id, it.id)
                        }
                      >
                        ✎
                      </button>
                      <button
                        className="feed__action"
                        title={t("comment.remove")}
                        onClick={() => void remove(it, actsOn)}
                      >
                        ✕
                      </button>
                    </>
                  )}
                </div>
              </div>
            </div>
          );
        })}
        <div style={{ height: padBottom }} />
      </div>
    </>
  );
}

/** Build the class for a filter chip from its selected state. */
function chipCls(on: boolean): string {
  return on ? "filterchip filterchip--on" : "filterchip";
}

/** Dedupe by id, so an item arriving on both sides of the seed/older-page boundary appears once. Order is preserved. */
function dedupById(list: ActivityItem[]): ActivityItem[] {
  const seen = new Set<number>();
  const out: ActivityItem[] = [];
  for (const it of list) {
    if (seen.has(it.id)) continue;
    seen.add(it.id);
    out.push(it);
  }
  return out;
}
