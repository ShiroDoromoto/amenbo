import { memo, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useStore } from "../store/store";
import type { Status, TaskCard } from "../mock/types";
import {
  BlockedChips, DueChip, FacetAvatar, PremiseChangedChip, PriorityDot, StatusSelect, TaskIdChip, TriggeredAtChip,
} from "../components/atoms";
import { Pager, PAGE_SIZE } from "../components/Pager";
import { Icon } from "../components/Icon";
import { useSmartView } from "../core/reads";
import { t } from "../core/i18n";
import { isClosed } from "../core/status";

// Shared list view for the list smart views (inbox / archive / due). They are saved filters — the same
// query the AI uses via mailbox. Each view pulls only its current page via core/reads (server-side
// LIMIT/OFFSET, or a bounded refine for inbox and due); no full task array is held in JS. The view's name is
// the sidebar's job alone — the header row carries only controls.
export function ListScreen({
  viewId, headerSlot, selectedTaskId, onSelectTask,
}: {
  viewId: string;
  // Where the header row (the row of controls) is rendered: the same full-width header row BoardScreen uses. The right pane sits below it.
  headerSlot: HTMLElement | null;
  selectedTaskId: number | null;
  onSelectTask: (id: number) => void;
}) {
  const store = useStore();
  const [page, setPage] = useState(0);
  const isInbox = viewId === "inbox";
  const [tab, setTab] = useState<"inbox" | "archived">("inbox");
  const archived = isInbox && tab === "archived";
  // The archive tab reads a smart view of its own (inbox-archived).
  const effectiveViewId = archived ? "inbox-archived" : viewId;
  // Switching view or tab goes back to the first page.
  useEffect(() => { setPage(0); }, [effectiveViewId]);
  // Leaving the inbox resets the tab, so the next visit starts on the inbox tab.
  useEffect(() => { if (!isInbox) setTab("inbox"); }, [isInbox]);

  const { tasks, total } = useSmartView(effectiveViewId, page, PAGE_SIZE);
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  // Clamp the page once the count shrinks out from under it (a filter, or a change arriving from elsewhere).
  useEffect(() => { if (page > 0 && page >= pageCount) setPage(pageCount - 1); }, [page, pageCount]);
  const start = page * PAGE_SIZE;

  // The header band carries controls and nothing else, so it is raised only by a view that has some. The
  // inbox has its two tabs; the due row has nothing to put there, and an empty bordered band above its
  // list would read as something that failed to draw.
  const toolbar = (
    <div className="board__toolbar">
      <div className="viewtabs">
        <button
          className={`viewtab ${!archived ? "viewtab--active" : ""}`}
          onClick={() => setTab("inbox")}
        >
          {t("list.tabInbox")}
        </button>
        <button
          className={`viewtab ${archived ? "viewtab--active" : ""}`}
          onClick={() => setTab("archived")}
        >
          {t("list.tabArchived")}
        </button>
      </div>
    </div>
  );

  return (
    <>
      {headerSlot && isInbox && createPortal(toolbar, headerSlot)}

      {total === 0 ? (
        <div className="placeholder">
          <Icon name="check" size="lg" />
          <div>{emptyLine(viewId, archived)}</div>
        </div>
      ) : (
        <>
          <div className="list">
            {tasks.map((t) => (
              <TaskRow
                key={t.id}
                task={t}
                selected={t.id === selectedTaskId}
                showUnread={isInbox && !archived}
                onMarkRead={isInbox && !archived ? store.markSeen : undefined}
                onArchive={isInbox && !archived ? store.archiveInbox : undefined}
                onUnarchive={archived ? store.unarchiveInbox : undefined}
                onSelect={onSelectTask}
                onStatus={store.setStatus}
              />
            ))}
          </div>
          <Pager
            page={page}
            pageCount={pageCount}
            total={total}
            start={start}
            pageSize={PAGE_SIZE}
            onPage={setPage}
          />
        </>
      )}
    </>
  );
}

/**
 * What an empty view says. Each of these lists is a different absence, and one line for all of them would
 * report the wrong one: an empty inbox is nothing waiting on you, an empty due row is nothing pressing.
 */
function emptyLine(viewId: string, archived: boolean): string {
  if (archived) return t("list.emptyArchived");
  if (viewId === "inbox") return t("list.emptyInbox");
  if (viewId === "due") return t("list.emptyDue");
  return t("list.empty");
}

// The row is memoised so that changing the selection does not re-render every sibling row. For that
// to hold, the parent must not build the click handlers inline per row: it passes stable references
// (the store's mutators, AppShell's onSelectTask) and the row binds task.id when it calls them. Only
// the rows whose props actually changed — selected, say — re-render.
const TaskRow = memo(function TaskRow({
  task, selected, showUnread, onMarkRead, onArchive, onUnarchive, onSelect, onStatus,
}: {
  task: TaskCard;
  selected: boolean;
  // Inbox tab only: light the unread dot on rows that still have unread comments addressed to me.
  // Clicking the row calls markTaskSeen and puts it out, but membership of the list follows the
  // comments, not their read state, so the row itself stays.
  showUnread: boolean;
  // Inbox tab only: mark the row read explicitly (the dot goes out; the row stays in the inbox). Shown on unread rows only.
  onMarkRead?: (id: number) => void;
  // Inbox tab only: archive the row out of the inbox. undefined — and so hidden — on other views and on the archive tab.
  onArchive?: (id: number) => void;
  // Archive tab only: unarchive the row back into the inbox. undefined — and so hidden — everywhere else.
  onUnarchive?: (id: number) => void;
  onSelect: (id: number) => void;
  onStatus: (id: number, status: Status, reason?: string) => void;
}) {
  const unread = showUnread && !!task.unread;
  return (
    <div className={`row ${selected ? "row--selected" : ""}`} onClick={() => onSelect(task.id)} role="button" data-pane-select>
      <span className="row__unread" aria-hidden={!unread}>
        {unread && <span className="row__unread-dot" role="img" aria-label={t("list.unread")} />}
      </span>
      <span className="row__status"><StatusSelect id={task.id} status={task.status} onStatus={onStatus} premiseChange={task.premiseChange} draft={task.draft} /></span>
      <span className={`row__title ${isClosed(task.status) ? "row__title--closed" : ""}`}>{task.title}</span>
      <span className="row__spacer" />
      <TaskIdChip id={task.id} />
      <BlockedChips task={task} />
      <PremiseChangedChip task={task} />
      {task.assignee && <FacetAvatar actor={task.assignee} />}
      <PriorityDot priority={task.priority} />
      {showUnread && <TriggeredAtChip at={task.triggeredAt} />}
      <DueChip due={task.due} />
      {(onMarkRead || onArchive || onUnarchive) && (
        <span className="row__actions">
          {unread && onMarkRead && (
            <button
              type="button"
              className="btn"
              title={t("list.markRead")}
              onClick={(e) => { e.stopPropagation(); onMarkRead(task.id); }}
            >
              {t("list.markRead")}
            </button>
          )}
          {onArchive && (
            <button
              type="button"
              className="btn"
              title={t("list.archive")}
              aria-label={t("list.archive")}
              onClick={(e) => { e.stopPropagation(); onArchive(task.id); }}
            >
              {t("list.dismiss")}
            </button>
          )}
          {onUnarchive && (
            <button
              type="button"
              className="btn"
              title={t("list.unarchiveTitle")}
              aria-label={t("list.unarchiveTitle")}
              onClick={(e) => { e.stopPropagation(); onUnarchive(task.id); }}
            >
              {t("list.unarchive")}
            </button>
          )}
        </span>
      )}
    </div>
  );
});
