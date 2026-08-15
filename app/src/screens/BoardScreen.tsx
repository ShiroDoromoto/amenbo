import { memo, useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from "react";
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
import { asTyped, isEnterSubmit } from "../core/keys";
import { FirstLoop } from "../components/FirstLoop";
import { AgentHookWiringRow, useAgentHookWiring } from "./AgentHookWiringRow";
import { LinkFolderNotice } from "./LinkFolderNotice";
import { pickBoardNotice } from "./boardNotice";
import { useBoundFolders } from "../core/boundFolders";
import { inTauri } from "../core/snapshot";
import { DecisionsScreen } from "./DecisionsScreen";
import { CalendarView } from "./CalendarView";
import { TimelineView } from "./TimelineView";
import { errText, statusLabel, t, tf, viewLabel } from "../core/i18n";
import type { ComposeTarget } from "../shell/AppShell";
import {
  filterDimensions, parseRefQuery, passesFilters, selectionKey,
  type DimAssignments, type FilterSelection,
} from "../core/filters";
import { fetchProjectDimensionAssignments } from "../core/mutations";
import { useQuery } from "../core/query";
import { DimensionManager } from "./DimensionManager";
import { BOARD_FLIP, useBoardFlip } from "./boardFlip";
import { BOARD_COLUMN_CAP } from "./boardLayout";
import { cardChips, type CardChip } from "./cardChips";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";

type View = "list" | "board" | "calendar" | "timeline";
const VIEWS: View[] = ["list", "board", "calendar", "timeline"];

// What the board's columns group by: `"status"` (a first-class field — the columns fall out of it), or the id
// of one of the project's dimensions, which splits the board into one column per value of that dimension.
const STATUS_GROUP = "status";

// Stable empty map for the instant before the assignment read comes back (and in the browser mock, where it
// stays empty). A fresh `{}` per render would re-render every card for nothing.
const NO_ASSIGNMENTS: DimAssignments = {};

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
 * attached to it — and a card carries only the first two (a comment is a count here, not a body). The term
 * goes over structurally rather than through this page's filter, so a phrase survives the trip: the filter
 * grammar carries no words, and splits on whitespace besides.
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
  // Whether the filters are open. Closed is where a board starts: the values of every axis do not fit on a
  // line, and a reader who is not narrowing anything should be given that room for the tasks (`AMB-D-654`).
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [dimMgrOpen, setDimMgrOpen] = useState(false);
  const [dimAssign, setDimAssign] = useState<Record<string, number>>({});
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
  // What this project has left to wire. Read here rather than inside the row, because whether the row is
  // standing is one of the inputs to which notice the board draws (see `notice` below).
  const wiring = useAgentHookWiring(projectId);
  // The project's folders, for the same reason: whether it has one an AI could be started in decides
  // which notice is drawn, and the first loop needs the folder itself to speak about.
  const folders = useBoundFolders(projectId);
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
  // The assignments of every user-defined dimension (taskId→dimId→valueId), read for the whole board in one
  // go: `dimAssign` above holds only the axis being grouped by, while the filter chips and the cards' own
  // chips reach every axis. It goes through the query cache rather than a bare effect because a value
  // assigned elsewhere — the detail pane's selects, the CLI — acks with the "tasks" scope, and that is what
  // brings the answer back; an effect keyed on the set of axes would never hear about it, leaving the cards
  // drawing the classification the board had at mount.
  const filterDimAssign = useQuery<DimAssignments>(
    ["dimAssign", projectId, dimIdsKey],
    async () => {
      const results = await Promise.all(
        projectDims.map((d) =>
          fetchProjectDimensionAssignments(projectId, d.id).then((rows) => ({ dimId: d.id, rows })),
        ),
      );
      const m: DimAssignments = {};
      for (const { dimId, rows } of results) {
        for (const r of rows) (m[r.taskId] ??= {})[dimId] = r.valueId;
      }
      return m;
    },
  ).data ?? NO_ASSIGNMENTS;
  // The dimension whose values split the columns (when group names one). Null for "status" or a deleted id.
  const groupingDim = groupingDimId ? projectDims.find((d) => d.id === groupingDimId) ?? null : null;
  // What each card draws of its classification (the rule itself is `cardChips`). Memoised, and keyed on
  // identities that only a write moves, so the cards' own memo holds: a fresh array per card per render
  // would re-render every sibling card on a change of selection.
  const chips = useMemo(
    () => cardChips(projectDims, filterDimAssign, groupingDimId),
    [projectDims, filterDimAssign, groupingDimId],
  );

  const dims = filterDimensions(projectDims, filterDimAssign);
  // How many axes are actually narrowing, counted over the axes that exist: a selection left behind by a
  // deleted dimension narrows nothing and must not be counted as if it did.
  const narrowedAxes = dims.filter((d) => (sel[d.id]?.length ?? 0) > 0).length;
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
  // The one standing notice this board carries (`AMB-D-535`). Every candidate answers for itself whether
  // it has something to say, and the ordering — which of them wins when more than one does — is
  // `pickBoardNotice`'s alone.
  //
  // `linkFolder`: no folder an AI could be started in, so nothing here is reachable from a session
  // (`AMB-D-533`). Asked of the folders and not of the tasks — a project carrying tasks and no folder is
  // exactly the case nothing else on the screen speaks about. Nothing is drawn until the read comes back.
  //
  // `firstLoop`: a project with nothing in it yet gets the first loop instead of empty columns
  // (`AMB-D-414`) — the one push that puts something on it. The question is about the project, not about
  // the view: `all` is the project's whole page, narrowed by nothing at all, so an empty `all` is the
  // project itself being empty, where an empty `tasks` may only be the search or the filter chips biting.
  // Outside Tauri there is no folder to open a terminal in, so the browser iteration keeps its bare columns.
  const notice = pickBoardNotice({
    linkFolder: inTauri() && folders.answered && folders.live.length === 0,
    firstLoop: all.length === 0 && inTauri(),
    agentHookWiring: wiring.waiting.length > 0,
  });
  // Whether the columns give way to the notice, rather than standing under it. They give way only where
  // there is nothing in them and the notice is the whole of what there is to do here — empty columns under
  // a "link a folder" warning is a screen saying nothing twice. A project that holds tasks keeps its board
  // whatever is standing above it: a notice is a band over the work, not a replacement for it.
  const bareBoard = all.length === 0 && (notice === "linkFolder" || notice === "firstLoop");
  if (!project) return <div className="placeholder">{t("board.notFound")}</div>;
  // One value on one axis, turned on or off. Selecting is what composes the question (`AMB-D-655`), so
  // nothing here is exclusive: the values pile up within the axis, and an axis left empty narrows nothing.
  const toggleValue = (id: string, value: string) => setSel((prev) => {
    const chosen = prev[id] ?? [];
    return { ...prev, [id]: chosen.includes(value) ? chosen.filter((v) => v !== value) : [...chosen, value] };
  });

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
        <Icon name="scales" /> {t("nav.decisions")}
        {project.proposedDecisionCount ? <span className="decisionsbtn__count">{project.proposedDecisionCount}</span> : null}
      </button>
      <div className="topbar__spacer" />
      {/* The one control the filters have while they are closed, so it says how many axes are narrowing:
          a filter still in force with its values out of sight looks like tasks that are simply gone. It
          stands with the tasks it acts on — the decisions tab has filters of its own. */}
      {tab === "tasks" && !bareBoard && (
        <button
          className={`filtertoggle ${filtersOpen ? "filtertoggle--active" : ""}`}
          aria-expanded={filtersOpen}
          onClick={() => setFiltersOpen((open) => !open)}
        >
          <Icon name="search" /> {t("board.filters")}
          {narrowedAxes > 0 && <span className="filtertoggle__count">{narrowedAxes}</span>}
        </button>
      )}
      <button className="btn" title={t("projset.title")} aria-label={t("projset.title")} onClick={onOpenSettings}>
        <Icon name="gear" />
      </button>
    </div>
  );

  return (
    <>
      {headerSlot && createPortal(toolbar, headerSlot)}

      {/* Above both tabs, because a notice is about the project and not about what is being looked at in
          it. At most one of them is here, and the one that is won the ordering (`AMB-D-535`); what it
          beat is not lost with it — project settings lists every folder still waiting to be wired. */}
      {notice === "linkFolder" && <LinkFolderNotice onLinkFolder={onOpenSettings} />}
      {notice === "agentHookWiring" && <AgentHookWiringRow projectId={projectId} wiring={wiring} />}

      {tab === "decisions" && (
        <DecisionsScreen
          projectId={projectId}
          selectedDecisionId={selectedDecisionId}
          onSelectDecision={onSelectDecision}
        />
      )}
      {/* The loop speaks about a folder, and `linkFolder` standing ahead of it is what guarantees there
          is one to speak about. */}
      {tab === "tasks" && notice === "firstLoop" && folders.live[0] && (
        <div className="board__firstloop">
          <FirstLoop dir={folders.live[0].path} />
        </div>
      )}

      {tab === "tasks" && !bareBoard && (
      <>
      <div className="filterbar">
        <input
          {...asTyped}
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
          <ErrorNote tone="quiet">
            {t("board.searchFailed")} — {errText(searchError)}
          </ErrorNote>
        )}
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
              <button className="filterchip" onClick={() => setDimMgrOpen(true)}><Icon name="gear" /> {t("board.manageDimensions")}</button>
            )}
          </div>
        )}
      </div>

      {/* The filters themselves, opened in place under the bar the toggle sits above. One line per axis,
          because a row of values does not fold onto the same line as the next axis's (`AMB-D-654`) —
          and closed, none of it takes room from the tasks. */}
      {filtersOpen && (
        <div className="filterpanel">
          {dims.map((d) => (
            <div key={d.id} className="filterpanel__axis">
              <span className="faint filterpanel__label">{d.label()}</span>
              <div className="filterpanel__values">
                {/* Each value is a switch: what a reader composes here is the set to narrow to (`AMB-D-655`). */}
                {d.options.map((o) => {
                  const on = sel[d.id]?.includes(o.value) ?? false;
                  return (
                    <button
                      key={o.value}
                      className={`filterchip ${on ? "filterchip--on" : ""}`}
                      aria-pressed={on}
                      onClick={() => toggleValue(d.id, o.value)}
                    >
                      {o.label()}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}

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
                  // The list narrowed to what this column holds — both terminals, selected together, as
                  // the CLI would ask for them (`status:done,rejected`).
                  onSeeAll: () => { setSel((s) => ({ ...s, status: ["done", "rejected"] })); setView("list"); },
                }
              : undefined;
            return (
              <Column
                key={st}
                name={statusLabel(st)}
                cards={cards}
                chips={chips}
                count={isDone ? colTasks.length - rejected : undefined}
                note={rejected > 0 ? tf("board.rejectedCount", { n: rejected }) : undefined}
                overflow={overflow}
                selectedTaskId={selectedTaskId}
                onSelectTask={onSelectTask}
                onStatus={setStatusAnimated}
                onSeeAllList={() => setView("list")}
                // Only the todo column offers "add". The plus in the column head composes in the right pane and
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
              chips={chips}
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
            chips={chips}
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
      {...asTyped}
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
    <button className="filterchip" onClick={() => setAdding(true)}><Icon name="plus" /> {t("board.addDimension")}</button>
  );
}

/** The affordance for adding a value — that is, a column — to the dimension currently being grouped by. */
function AddDimensionValue({ onAdd }: { onAdd: (name: string) => void }) {
  const [adding, setAdding] = useState(false);
  const [text, setText] = useState("");
  const commit = () => { if (text.trim()) { onAdd(text.trim()); setText(""); } };
  return adding ? (
    <input
      {...asTyped}
      className="column__addinput board__addcolinput"
      autoFocus
      value={text}
      placeholder={t("board.dimensionValuePh")}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => { if (isEnterSubmit(e)) { commit(); setAdding(false); } if (e.key === "Escape") setAdding(false); }}
      onBlur={() => { commit(); setAdding(false); }}
    />
  ) : (
    <button className="board__addcol" onClick={() => setAdding(true)}><Icon name="plus" /> {t("board.addDimensionValue")}</button>
  );
}

// The column is memoised as well. Its cards are memoised in turn, so re-rendering a column does not re-render
// the cards whose props are unchanged. The column itself usually does re-render (the card array and the drop
// handlers are fresh each render); what matters is that a change of selection stops before the sibling cards.
const Column = memo(function Column({
  name, cards, chips, count, note, overflow, onSeeAllList, selectedTaskId, onSelectTask, onStatus, onAdd,
  droppable, draggingId, onDropTask, onCardDragStart, onCardDragEnd,
}: {
  name: string;
  cards: TaskCard[];
  /**
   * The classification each card draws, by task id (see `cardChips`). One map for the whole board, so the
   * per-card arrays keep their identity and the cards' memo survives a re-render of the column.
   */
  chips: Record<string, CardChip[]>;
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
          <button className="column__addbtn" title={t("card.addTaskTip")} onClick={onAdd}><Icon name="plus" /></button>
        )}
      </div>
      {shownCards.map((t) => (
        <TaskCardView
          key={t.id}
          task={t}
          chips={chips[t.id]}
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
  task, chips, selected, draggable, dragging, onBeginDrag, onEndDrag, onSelect, onStatus,
}: {
  task: TaskCard;
  /** The values this task carries on the axes flagged for the card. Undefined when it carries none. */
  chips?: CardChip[];
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

      {/* Its own row rather than more chips on the one above: what the task *is* reads apart from what is
          wrong with it. Drawn only when there is something to draw, so a card on a board with no flagged
          axis keeps exactly the shape it had. The axis is named in the tooltip — on the card the value
          alone is the fact, and spelling out "axis: value" on every chip spends the density `AMB-D-40`
          set out to protect. */}
      {chips && chips.length > 0 && (
        <div className="card__row">
          {chips.map((c) => (
            <span key={c.dimId} className="chip chip--dim" title={`${c.axis}: ${c.value}`}>{c.value}</span>
          ))}
        </div>
      )}

      <div className="card__footer">
        {task.comments > 0 && <span><Icon name="comment" /> {task.comments}</span>}
        <TaskIdChip id={task.id} />
        <span className="card__spacer" />
        <StatusSelect id={task.id} status={task.status} onStatus={onStatus} premiseChange={task.premiseChange} />
      </div>
    </div>
  );
});
