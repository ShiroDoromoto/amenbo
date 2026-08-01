import { memo, useCallback, useEffect, useRef, useState, type DragEvent } from "react";
import { createPortal } from "react-dom";
import { dataAdapter } from "../mock/adapter";
import { useStore } from "../store/store";
import type { Status, TaskCard } from "../mock/types";
import {
  BlockedChips, DueChip, FacetAvatar, PremiseChangedChip, PriorityDot, StatusSelect, TaskIdChip,
} from "../components/atoms";
import { isClosed, STATUS_COLUMNS } from "../core/status";
import { Pager, usePager } from "../components/Pager";
import { useTaskPage, useTaskSearchIds } from "../core/reads";
import { isEnterSubmit } from "../core/keys";
import { ProjectFirstLoop } from "../components/FirstLoop";
import { AgentHookWiringRow } from "./AgentHookWiringRow";
import { inTauri } from "../core/snapshot";
import { DecisionsScreen } from "./DecisionsScreen";
import { CalendarView } from "./CalendarView";
import { TimelineView } from "./TimelineView";
import { errText, statusLabel, t, tf, viewLabel } from "../core/i18n";
import type { ComposeTarget } from "../shell/AppShell";
import { CLOSED_FILTER_VALUE, filterDimensions, parseRefQuery, passesFilters, selectionKey, type FilterSelection } from "../core/filters";
import { fetchProjectDimensionAssignments } from "../core/mutations";
import { DimensionManager } from "./DimensionManager";
import { BOARD_FLIP, useBoardFlip } from "./boardFlip";
import { BOARD_COLUMN_CAP } from "./boardLayout";

type View = "list" | "board" | "calendar" | "timeline";
const VIEWS: View[] = ["list", "board", "calendar", "timeline"];

// What the board's columns group by: `"status"` (a first-class field — the columns fall out of it), or the id
// of one of the project's dimensions, which splits the board into one column per value of that dimension.
const STATUS_GROUP = "status";

// Order of the closed column: most recently completed first (RFC3339 sorts lexicographically = chronologically).
// Tasks with no completion time sink to the bottom — which is where the rejected land, having none by definition.
function byCompletedDesc(a: TaskCard, b: TaskCard): number {
  const av = a.completedAt ?? "";
  const bv = b.completedAt ?? "";
  if (av === bv) return 0;
  if (!av) return 1;
  if (!bv) return -1;
  return av < bv ? 1 : -1;
}
// How many cards the closed column stacks. What has ended grows without bound as time passes, so the column
// carries only the most recent N and sends the rest to the list view through the "see closed in list" affordance.
const DONE_COLUMN_CAP = 20;

// The other columns must not grow the DOM without bound either: the same cap-plus-affordance the closed column
// uses keeps every task from being mounted (see BOARD_COLUMN_CAP in ./boardLayout).

/**
 * The board surface for one project: the view switcher (list/board/…) plus the tasks/decisions tabs. The initial
 * view is the project's default (`project.view`, persisted from the settings screen); switching views from the
 * header is transient and does not rewrite `project.view`. Tasks are fetched one project at a time via task_page
 * (the whole store is never held), and column grouping and the filter chips are layered on client-side (bounded
 * by the size of a project).
 *
 * **The search is core's, not the client's.** When the box recognises a task ref (`AMB-T-<n>`, or the bare
 * `#<n>` / `T-<n>`) it narrows to that number without asking core at all; anything else goes to `task_search`
 * ({@link useTaskSearchIds}) and comes back as the ids to narrow the page by. It has to: the word index spans
 * five faces — title, notes, raw comment bodies, the labels the task was placed on, and the names of what is
 * attached to it — and a card carries only the first two (a comment is a 💬 count here, not a body). The term
 * goes over structurally rather than as a `text:` written into this page's filter, so a phrase survives the
 * trip: the filter grammar splits on whitespace and would drop everything after the first word.
 *
 * The drag-end handler is
 * a stable reference (a fresh one per render would defeat the cards' memo). A drop onto a column sets status, and
 * even when the write layer rejects a reservation (todo→in_progress) the card's column is drawn from the status
 * in the source of truth, so it does not move — no optimistic update, and nothing to roll back.
 */
export function BoardScreen({
  projectId, headerSlot, selectedTaskId, onSelectTask, selectedDecisionId, onSelectDecision, onComposeTask, onOpenSettings,
}: {
  projectId: number;
  // Where the project header (toolbar) is drawn. It is portalled into AppShell's full-width header row, so the
  // right pane sits below the header. Null only while the slot is undetermined (before the first commit).
  headerSlot: HTMLElement | null;
  selectedTaskId: number | null;
  onSelectTask: (id: number) => void;
  selectedDecisionId: number | null;
  onSelectDecision: (id: number | null) => void;
  onComposeTask: (target: ComposeTarget) => void;
  onOpenSettings: () => void;
}) {
  const store = useStore();
  const [view, setView] = useState<View>(() => dataAdapter.getProject(projectId)?.view ?? "board");
  // The tasks surface (list/board/…) or the decisions one. Decisions shows only what sits under this project.
  const [tab, setTab] = useState<"tasks" | "decisions">("tasks");
  const [group, setGroup] = useState<string | number>(STATUS_GROUP);
  const [sel, setSel] = useState<FilterSelection>({});
  const [dimMgrOpen, setDimMgrOpen] = useState(false);
  const [dimAssign, setDimAssign] = useState<Record<string, number>>({});
  // For filtering: task assignments across every user-defined dimension (taskId→dimId→valueId). `dimAssign`
  // above holds only the one axis being grouped by, but filters must reach every dimension — hence all of them.
  const [filterDimAssign, setFilterDimAssign] = useState<Record<string, Record<number, number>>>({});
  // Free-word search, run by core over every face the word index carries (see the doc comment above).
  // Incremental, and ANDs with the filter chips.
  const [search, setSearch] = useState("");
  // Id of the card currently being dragged (drives the column highlight and dims the card that was grabbed).
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const clearDragging = useCallback(() => setDraggingId(null), []);
  // The board surface, for the move flourish. Only one `.board` mounts at a time, so both grouping
  // layouts share this ref. useBoardFlip is inert outside Tauri and when its flag is off.
  const boardRef = useRef<HTMLDivElement>(null);
  const armMove = useBoardFlip(boardRef, draggingId);
  // The status pull-down on a card is a local move we do want to animate (unlike a drag): arm the flourish just
  // before the write, so the card slides to its new column. A stable ref, to keep the cards' memo intact.
  const setStatusAnimated = useCallback((id: number, status: Status, reason?: string) => {
    armMove();
    store.setStatus(id, status, reason);
  }, [armMove, store]);
  const project = dataAdapter.getProject(projectId);
  const rawQ = search.trim();
  const ref = parseRefQuery(search);
  // A ref query is answered here, so it never becomes a text search: `T-12` is a number, not a word to look
  // for. `null` back from the hook is "nothing was asked", which is not the same as "nothing matched".
  const { hits, error: searchError } = useTaskSearchIds(projectId, ref ? "" : rawQ);
  const { tasks: all } = useTaskPage({ projectId, sort: "order" });
  const projectDims = project?.dimensions ?? [];
  // If the grouping axis names a dimension id, that is what splits the columns ("status", or a deleted id, → null).
  const groupingDimId =
    typeof group === "number" && projectDims.some((d) => d.id === group) ? group : null;
  // On a project switch, drop back to that project's default view (`project.view`). AppShell does not key
  // BoardScreen by projectId and so never remounts it, which means the useState initialiser does not re-run on a
  // switch — sync it here. The dep is projectId alone, so switching views inside one project does not fire it.
  useEffect(() => {
    const v = dataAdapter.getProject(projectId)?.view;
    if (v) setView(v);
  }, [projectId]);
  // If the chosen dimension is gone (deleted from the manager), fall the now-dangling group back to status.
  const dimIdsKey = projectDims.map((d) => d.id).join(",");
  useEffect(() => {
    if (typeof group === "number" && !projectDims.some((d) => d.id === group)) setGroup(STATUS_GROUP);
  }, [group, dimIdsKey]);
  // Pull the chosen dimension's task assignments (taskId→valueId) from the read-model in one go (Tauri only).
  useEffect(() => {
    if (!groupingDimId) { setDimAssign({}); return; }
    let alive = true;
    fetchProjectDimensionAssignments(projectId, groupingDimId).then((rows) => {
      if (!alive) return;
      const m: Record<string, number> = {};
      for (const r of rows) m[r.taskId] = r.valueId;
      setDimAssign(m);
    }).catch(() => {});
    return () => { alive = false; };
  }, [groupingDimId, projectId]);
  // Pull the assignments of every user-defined dimension, for filtering (independent of grouping; refetched when
  // the set of axes changes).
  useEffect(() => {
    if (dimIdsKey === "") { setFilterDimAssign({}); return; }
    let alive = true;
    Promise.all(
      projectDims.map((d) =>
        fetchProjectDimensionAssignments(projectId, d.id).then((rows) => ({ dimId: d.id, rows })),
      ),
    ).then((results) => {
      if (!alive) return;
      const m: Record<string, Record<number, number>> = {};
      for (const { dimId, rows } of results) {
        for (const r of rows) (m[r.taskId] ??= {})[dimId] = r.valueId;
      }
      setFilterDimAssign(m);
    }).catch(() => {});
    return () => { alive = false; };
  }, [projectId, dimIdsKey]);
  // The dimension whose values split the columns (when group names one). Null for "status" or a deleted id.
  const groupingDim = groupingDimId ? projectDims.find((d) => d.id === groupingDimId) ?? null : null;

  const dims = filterDimensions(projectDims, filterDimAssign);
  const tasks = all
    .filter((t) => passesFilters(t, dims, sel))
    .filter((t) =>
      ref ? ref.space === "task" && Number(t.id) === ref.num : hits === null || hits.has(Number(t.id)),
    );
  // view=list (the flat list) is windowed by the pager, which resets to the first page when the view or filters
  // change. usePager is a Hook, so it has to sit above the early-return guard below: if the open project is
  // deleted and `project` flips defined→undefined, the number of Hooks must stay the same (Rules of Hooks — a
  // violation throws during render and blacks out the screen). groupingDim/dims/tasks above are null-safe through
  // `project?.dimensions ?? []`, so they come out empty and reach no JSX before the guard returns the placeholder.
  const listPager = usePager(tasks, `${view}|${selectionKey(sel)}|${rawQ}`);
  // A project with nothing in it yet gets the first loop instead of empty columns (`AMB-D-414`) — the
  // one push that puts something on it. The question is about the project, not about the view: `all`
  // is the project's whole page, narrowed by nothing at all, so an empty `all` is the project itself
  // being empty, where an empty `tasks` may only be the search or the filter chips biting. Outside
  // Tauri there is no folder to open a terminal in, so the browser iteration keeps its bare columns.
  const untouched = all.length === 0 && inTauri();
  if (!project) return <div className="placeholder">{t("board.notFound")}</div>;
  const setDim = (id: string, value: string) => setSel((prev) => ({ ...prev, [id]: value }));

  // The project header (toolbar). It is portalled into AppShell's full-width header row so that it spans main
  // plus the right pane — the right pane begins below it. Not drawn for the first instant, while the slot is unset.
  const toolbar = (
    <div className="board__toolbar">
      <div className="viewtabs">
        {VIEWS.map((v) => (
          <button
            key={v}
            className={`viewtab ${tab === "tasks" && v === view ? "viewtab--active" : ""}`}
            onClick={() => { setTab("tasks"); setView(v); onSelectDecision(null); }}
          >
            {viewLabel(v)}
          </button>
        ))}
      </div>
      <span className="board__sep" aria-hidden="true" />
      <button
        className={`decisionsbtn ${tab === "decisions" ? "decisionsbtn--active" : ""}`}
        onClick={() => setTab("decisions")}
      >
        ⚖ {t("nav.decisions")}
        {project.proposedDecisionCount ? <span className="decisionsbtn__count">{project.proposedDecisionCount}</span> : null}
      </button>
      <div className="topbar__spacer" />
      <button className="btn" title={t("projset.title")} aria-label={t("projset.title")} onClick={onOpenSettings}>
        <span className="btn__glyph" aria-hidden="true">⚙</span>
      </button>
    </div>
  );

  return (
    <>
      {headerSlot && createPortal(toolbar, headerSlot)}

      {/* Above both tabs, because it is about the project and not about what is being looked at in it.
          It draws nothing where every folder of this project is wired, or where the project said no
          (`AMB-D-459`, `AMB-D-460`). */}
      <AgentHookWiringRow projectId={projectId} />

      {tab === "decisions" && (
        <DecisionsScreen
          projectId={projectId}
          selectedDecisionId={selectedDecisionId}
          onSelectDecision={onSelectDecision}
        />
      )}
      {tab === "tasks" && untouched && (
        <div className="board__firstloop">
          <ProjectFirstLoop projectId={projectId} onLinkFolder={onOpenSettings} />
        </div>
      )}

      {tab === "tasks" && !untouched && (
      <>
      <div className="filterbar">
        <input
          className="board__search"
          type="search"
          placeholder={t("board.searchPh")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ fontSize: "var(--fs-xs)", width: 180 }}
        />
        {/* A search that could not run narrows nothing, and narrowing nothing looks exactly like a word
            that matched everything. Say which it was, next to the box that asked. */}
        {searchError != null && (
          <span className="faint" role="alert" style={{ fontSize: "var(--fs-xs)" }}>
            ⚠ {t("board.searchFailed")} — {errText(searchError)}
          </span>
        )}
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>🔍 {t("board.filter")}</span>
        {dims.map((d) => (
          <label key={d.id} className="filtersel">
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{d.label()}</span>
            <select
              value={sel[d.id] ?? ""}
              onChange={(e) => setDim(d.id, e.target.value)}
              style={{ fontSize: "var(--fs-xs)" }}
            >
              <option value="">{t("filter.opt.all")}</option>
              {d.options.map((o) => (
                <option key={o.value} value={o.value}>{o.label()}</option>
              ))}
            </select>
          </label>
        ))}
        {view === "board" && (
          <div className="groupby">
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("board.group")}</span>
            <button
              className={`filterchip ${group === STATUS_GROUP ? "filterchip--on" : ""}`}
              onClick={() => setGroup(STATUS_GROUP)}
            >
              {t("filter.dim.status")}
            </button>
            {projectDims.map((d) => (
              <button
                key={d.id}
                className={`filterchip ${group === d.id ? "filterchip--on" : ""}`}
                onClick={() => setGroup(d.id)}
              >
                {d.name}
              </button>
            ))}
            <AddDimension onAdd={(name) => store.addDimension(projectId, name)} />
            {projectDims.length >= 1 && (
              <button className="filterchip" onClick={() => setDimMgrOpen(true)}>⚙ {t("board.manageDimensions")}</button>
            )}
          </div>
        )}
      </div>

      {view === "board" && !groupingDim && (
        <div className="board" ref={boardRef}>
          {STATUS_COLUMNS.map((st) => {
            // The done column is the *closed* column: a rejection folds in here rather than growing a
            // fifth one (`AMB-D-397`). It keeps the "done" heading and count — a rejected task is not an
            // achievement and must not be counted as one — and says how many rejected ride along beside it.
            const isDone = st === "done";
            const colTasks = tasks.filter((t) => (isDone ? isClosed(t.status) : t.status === st));
            // Newest completion first; a rejection has no completion time at all, so the rejected sink
            // below the done — which is the order to read them in anyway.
            const sorted = isDone ? colTasks.slice().sort(byCompletedDesc) : colTasks;
            const cards = isDone ? sorted.slice(0, DONE_COLUMN_CAP) : sorted;
            const rejected = isDone ? colTasks.filter((t) => t.status === "rejected").length : 0;
            const overflow = isDone && sorted.length > DONE_COLUMN_CAP
              ? {
                  total: sorted.length,
                  // The list narrowed to what this column holds — both terminals, as the CLI would ask
                  // for them (`status:done,rejected`).
                  onSeeAll: () => { setSel((s) => ({ ...s, status: CLOSED_FILTER_VALUE })); setView("list"); },
                }
              : undefined;
            return (
              <Column
                key={st}
                name={statusLabel(st)}
                cards={cards}
                count={isDone ? colTasks.length - rejected : undefined}
                note={rejected > 0 ? tf("board.rejectedCount", { n: rejected }) : undefined}
                overflow={overflow}
                selectedTaskId={selectedTaskId}
                onSelectTask={onSelectTask}
                onStatus={setStatusAnimated}
                onSeeAllList={() => setView("list")}
                // Only the todo column offers "add". The ＋ in the column head composes in the right pane and
                // adds the task directly under the project (a dimension value is assigned later, from the detail).
                onAdd={st === "todo"
                  ? () => onComposeTask({ projectId, label: project.name })
                  : undefined}
                droppable
                draggingId={draggingId}
                onCardDragStart={setDraggingId}
                onCardDragEnd={clearDragging}
                onDropTask={(id) => {
                  const tk = all.find((t) => t.id === id);
                  if (tk && tk.status !== st) store.setStatus(id, st);
                }}
              />
            );
          })}
        </div>
      )}

      {view === "board" && groupingDim && (
        <div className="board" ref={boardRef}>
          {groupingDim.values.map((v) => (
            <Column
              key={v.id}
              name={v.name}
              cards={tasks.filter((tk) => dimAssign[tk.id] === v.id)}
              selectedTaskId={selectedTaskId}
              onSelectTask={onSelectTask}
              onStatus={store.setStatus}
              onSeeAllList={() => setView("list")}
              droppable
              draggingId={draggingId}
              onCardDragStart={setDraggingId}
              onCardDragEnd={clearDragging}
              onDropTask={(id) => {
                if (dimAssign[id] === v.id) return;
                store.setTaskDimensionValue(id, v.id);
                setDimAssign((m) => ({ ...m, [id]: v.id }));
              }}
            />
          ))}
          <Column
            name={t("board.noDimensionValue")}
            cards={tasks.filter((tk) => !dimAssign[tk.id])}
            selectedTaskId={selectedTaskId}
            onSelectTask={onSelectTask}
            onStatus={store.setStatus}
            onSeeAllList={() => setView("list")}
            droppable
            draggingId={draggingId}
            onCardDragStart={setDraggingId}
            onCardDragEnd={clearDragging}
            onDropTask={(id) => {
              const cur = dimAssign[id];
              if (!cur) return;
              store.unsetTaskDimensionValue(id, cur);
              setDimAssign((m) => { const n = { ...m }; delete n[id]; return n; });
            }}
          />
          <AddDimensionValue onAdd={(name) => store.addDimensionValue(groupingDim.id, name)} />
        </div>
      )}

      {view === "list" && (
        <>
          <div className="list">
            {/* The row is a div, not a button: it carries the status control, and a select may not nest inside a button. */}
            {listPager.pageItems.map((t) => (
              <div key={t.id} className={`row ${t.id === selectedTaskId ? "row--selected" : ""}`} onClick={() => onSelectTask(t.id)} role="button" data-pane-select>
                <span className="row__status"><StatusSelect id={t.id} status={t.status} onStatus={store.setStatus} premiseChange={t.premiseChange} /></span>
                <span className={`row__title ${isClosed(t.status) ? "row__title--closed" : ""}`}>{t.title}</span>
                <span className="row__spacer" />
                <BlockedChips task={t} />
                <PremiseChangedChip task={t} />
                {t.assignee && <FacetAvatar actor={t.assignee} />}
                <PriorityDot priority={t.priority} />
                <DueChip due={t.due} />
              </div>
            ))}
          </div>
          <Pager
            page={listPager.page}
            pageCount={listPager.pageCount}
            total={listPager.total}
            start={listPager.start}
            pageSize={listPager.pageSize}
            onPage={listPager.setPage}
          />
        </>
      )}

      {view === "calendar" && (
        <CalendarView tasks={tasks} selectedTaskId={selectedTaskId} onSelectTask={onSelectTask} />
      )}

      {view === "timeline" && (
        <TimelineView tasks={tasks} selectedTaskId={selectedTaskId} onSelectTask={onSelectTask} />
      )}
      {dimMgrOpen && <DimensionManager projectId={projectId} onClose={() => setDimMgrOpen(false)} />}
      </>
      )}
    </>
  );
}

/** The affordance for creating a dimension. It appears as a compact chip at the end of the group toggles. */
function AddDimension({ onAdd }: { onAdd: (name: string) => void }) {
  const [adding, setAdding] = useState(false);
  const [text, setText] = useState("");
  const commit = () => { if (text.trim()) { onAdd(text.trim()); setText(""); } };
  return adding ? (
    <input
      className="column__addinput"
      autoFocus
      value={text}
      placeholder={t("board.dimensionNamePh")}
      style={{ fontSize: "var(--fs-xs)" }}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => { if (isEnterSubmit(e)) { commit(); setAdding(false); } if (e.key === "Escape") setAdding(false); }}
      onBlur={() => { commit(); setAdding(false); }}
    />
  ) : (
    <button className="filterchip" onClick={() => setAdding(true)}>＋ {t("board.addDimension")}</button>
  );
}

/** The affordance for adding a value — that is, a column — to the dimension currently being grouped by. */
function AddDimensionValue({ onAdd }: { onAdd: (name: string) => void }) {
  const [adding, setAdding] = useState(false);
  const [text, setText] = useState("");
  const commit = () => { if (text.trim()) { onAdd(text.trim()); setText(""); } };
  return adding ? (
    <input
      className="column__addinput board__addcolinput"
      autoFocus
      value={text}
      placeholder={t("board.dimensionValuePh")}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => { if (isEnterSubmit(e)) { commit(); setAdding(false); } if (e.key === "Escape") setAdding(false); }}
      onBlur={() => { commit(); setAdding(false); }}
    />
  ) : (
    <button className="board__addcol" onClick={() => setAdding(true)}>＋ {t("board.addDimensionValue")}</button>
  );
}

// The column is memoised as well. Its cards are memoised in turn, so re-rendering a column does not re-render
// the cards whose props are unchanged. The column itself usually does re-render (the card array and the drop
// handlers are fresh each render); what matters is that a change of selection stops before the sibling cards.
const Column = memo(function Column({
  name, cards, count, note, overflow, onSeeAllList, selectedTaskId, onSelectTask, onStatus, onAdd,
  droppable, draggingId, onDropTask, onCardDragStart, onCardDragEnd,
}: {
  name: string;
  cards: TaskCard[];
  /**
   * What the head counts, where that is not simply what the column holds. The closed column needs it:
   * it holds both terminals, and the figure under a "done" heading must count only the done.
   */
  count?: number;
  /** A muted figure beside the count, for what the column holds and the count deliberately leaves out. */
  note?: string;
  /** The done column past its cap: the true total, and the "see closed in list" affordance. */
  overflow?: { total: number; onSeeAll: () => void };
  // Where a non-done column past its cap sends the overflow (a switch to list view). Columns without it are uncapped.
  onSeeAllList?: () => void;
  selectedTaskId: number | null;
  onSelectTask: (id: number) => void;
  onStatus: (id: number, status: Status, reason?: string) => void;
  onAdd?: () => void;
  // Drag and drop (the status board only). Cards can be grabbed from a droppable column, and a drop sets status.
  droppable?: boolean;
  draggingId?: number | null;
  onDropTask?: (id: number) => void;
  onCardDragStart?: (id: number) => void;
  onCardDragEnd?: () => void;
}) {
  const [over, setOver] = useState(false);
  // Capping: for the done column the caller has already trimmed to N and passed `overflow`. Any other column is
  // trimmed to its first N only when it exceeds BOARD_COLUMN_CAP and has somewhere to send the rest (onSeeAllList).
  const capped = !overflow && !!onSeeAllList && cards.length > BOARD_COLUMN_CAP;
  const shownCards = capped ? cards.slice(0, BOARD_COLUMN_CAP) : cards;
  const hiddenCount = capped ? cards.length - BOARD_COLUMN_CAP : 0;
  const dnd = droppable && onDropTask
    ? {
        onDragOver: (e: DragEvent) => { e.preventDefault(); e.dataTransfer.dropEffect = "move"; if (!over) setOver(true); },
        // Keep the highlight from flickering as the pointer crosses children (clear it only on leaving the column).
        onDragLeave: (e: DragEvent) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver(false); },
        onDrop: (e: DragEvent) => {
          e.preventDefault();
          setOver(false);
          const id = Number(e.dataTransfer.getData("text/plain"));
          if (id) onDropTask(id);
        },
      }
    : {};
  return (
    <div
      className={`column ${droppable ? "column--droppable" : ""} ${over ? "column--dragover" : ""}`.trim()}
      {...dnd}
    >
      <div className="column__head">
        <span className="column__name">{name}</span>
        <span className="column__count">{count ?? (overflow ? overflow.total : cards.length)}</span>
        {note && <span className="column__note">{note}</span>}
        {onAdd && (
          <button className="column__addbtn" title={t("card.addTaskTip")} onClick={onAdd}>＋</button>
        )}
      </div>
      {shownCards.map((t) => (
        <TaskCardView
          key={t.id}
          task={t}
          selected={t.id === selectedTaskId}
          draggable={!!onCardDragStart}
          dragging={t.id === draggingId}
          onBeginDrag={onCardDragStart}
          onEndDrag={onCardDragEnd}
          onSelect={onSelectTask}
          onStatus={onStatus}
        />
      ))}
      {overflow ? (
        <button className="column__seeall" onClick={overflow.onSeeAll}>
          {tf("board.seeClosedInList", { n: overflow.total })}
        </button>
      ) : hiddenCount > 0 ? (
        <button className="column__seeall" onClick={onSeeAllList}>
          {tf("board.seeMoreInList", { n: hiddenCount })}
        </button>
      ) : null}
    </div>
  );
});

// Cards are memoised so that a change of selection does not re-render the siblings. For the memo to hold, the
// click props have to be stable references (onSelect=AppShell, onStatus=store, onBeginDrag=setDraggingId,
// onEndDrag=the stable clearDragging) and the id is bound inside the card from task.id. The status select in the
// footer must stop mousedown to suppress the card's drag start, or selecting and dragging cannot both work.
const TaskCardView = memo(function TaskCardView({
  task, selected, draggable, dragging, onBeginDrag, onEndDrag, onSelect, onStatus,
}: {
  task: TaskCard;
  selected: boolean;
  draggable?: boolean;
  dragging?: boolean;
  onBeginDrag?: (id: number) => void;
  onEndDrag?: () => void;
  onSelect: (id: number) => void;
  onStatus: (id: number, status: Status, reason?: string) => void;
}) {
  return (
    <div
      className={[
        "card",
        selected ? "card--selected" : "",
        // Struck through once it has ended, either way it ended. Which terminal it reached is the
        // pull-down's to say (it sits in the footer, set to `done` or `rejected`).
        isClosed(task.status) ? "card--closed" : "",
        draggable ? "card--draggable" : "",
        dragging ? "card--dragging" : "",
      ].join(" ")}
      // The move flourish keys on this to track a card across columns; the flag omits it when off.
      data-flip-id={BOARD_FLIP ? task.id : undefined}
      draggable={draggable}
      onDragStart={draggable ? (e: DragEvent) => {
        e.dataTransfer.setData("text/plain", String(task.id));
        e.dataTransfer.effectAllowed = "move";
        onBeginDrag?.(task.id);
      } : undefined}
      onDragEnd={draggable ? () => onEndDrag?.() : undefined}
      onClick={() => onSelect(task.id)}
      role="button"
      data-pane-select
    >
      <div className="card__title">{task.title}</div>

      <div className="card__row">
        {task.assignee && (
          <span className="card__assign" title={t("card.assigneeTip")}>
            {t("card.assignee")} <FacetAvatar actor={task.assignee} showName />
          </span>
        )}
      </div>

      <div className="card__row">
        <PriorityDot priority={task.priority} />
        <DueChip due={task.due} />
        <BlockedChips task={task} />
        <PremiseChangedChip task={task} />
      </div>

      <div className="card__footer">
        {task.comments > 0 && <span>💬 {task.comments}</span>}
        <TaskIdChip id={task.id} />
        <span className="card__spacer" />
        <StatusSelect id={task.id} status={task.status} onStatus={onStatus} premiseChange={task.premiseChange} />
      </div>
    </div>
  );
});
