// THE WIRING SEAM (write side).
//
// Writes: inside Tauri we invoke core's per-action commands (task_add/status/comment_add) and
// use the returned **WriteAck (affected ids + invalidation scopes)** to invalidate only the query
// keys it names — no wholesale snapshot swap, no refetch storm across every view, no optimistic
// updates. Reads that still hang off the snapshot cache (projects/roster/activity/config) are
// refreshed once via `loadSnapshot`. Command arguments are passed in camelCase; Tauri converts them
// to the snake_case Rust parameters. Outside Tauri (`npm run dev` in a browser) the mutations are
// faked against the cache — plus a coarse query invalidation — so the front end can still be
// iterated on alone.
import { applySnapshot, getSnapshot, inTauri, loadSnapshot, type Snapshot } from "./snapshot";
import { applyPerfConfig, invoke } from "./ipc";
import { invalidateAllQueries, invalidateQueries, type QueryKey } from "./query";
import { type AttachTargetType } from "./reads";
import { guessLang, t, tf, type CmdError, type CmdErrorPart, type ViewKind } from "./i18n";
import { isClosed } from "./status";
import type { ActivityItem, Facet, Priority, Status, TaskCard } from "../mock/types";
import type { ActivityTargetDto, AgentHookRequestsDto, AgentHookWiringDto, BoundFolderDto, EventDto, DimensionTaskValueDto, DoctorFixDto, DoctorIssueDto, DoctorReportDto, HookNoticeDto, HookOfferDto, McpRequestDto, McpSetupDto, PointerRepairDto, ProjectDto, ProjectSettingsDto, ResyncReportDto, StaleBlockDto, StoreLocationsDto, TaskDimensionAssignmentDto, DecisionDimensionAssignmentDto, BackupReportDto, ExportReportDto, DataProgressDto, RestoreReportDto } from "../bindings/bindings";
import { taskRef } from "./idref";
import { todayStr } from "./calendar";

/**
 * What a write command returns (mirrors the Rust `WriteAck`). It is never a whole snapshot: the GUI
 * invalidates only the query keys implied by these affected ids and scopes.
 */
export interface WriteAck {
  tasks: number[];
  decisions: number[];
  scopes: string[]; // "tasks" / "decisions" (empty = nothing to invalidate)
}

function me() {
  const s = getSnapshot();
  const name = s.roster.find((a) => a.kind === "human")?.name ?? tf("common.you");
  return { name, kind: "human" as const };
}

/**
 * Invoke a Tauri command and apply the WriteAck it returns (invalidate the named queries, refresh the
 * snapshot). It **returns** the `loadSnapshot` promise that `applyAck` hands back, making the call
 * awaitable — so `await deleteProject(...)` resolving means the snapshot has already caught up, and a
 * caller that reads the list right after (`goToFirstProject` following a delete or archive) cannot
 * pick the now-gone project out of a stale one.
 */
async function invokeAck(cmd: string, args: Record<string, unknown>): Promise<void> {
  return applyAck(await invoke<WriteAck>(cmd, args));
}

/**
 * Apply a WriteAck: invalidate exactly the query keys implied by its `scopes` and affected ids (no
 * optimistic update), and refresh the reads that still hang off the snapshot cache via
 * `loadSnapshot`. The return value is that `loadSnapshot` promise, so a caller that **depends on the
 * refreshed snapshot** (`createProject`, which navigates to the project it just made) can `await` it;
 * callers that do not can ignore it — fire-and-forget, since the refresh reaches subscribers anyway.
 * The key-by-key mapping follows the ack's granularity: the archived-projects list refetches on the
 * "tasks" scope, because archiving, unarchiving, deleting and renaming a project all return it
 * (commands.rs); decision comments and attachments refetch the open thread or viewer whenever the
 * target id appears in the ack. `loadSnapshot` only wakes snapshot subscribers (the adapter-backed
 * screens, the inbox badge) — it is not a refetch of every view — and the watcher's own
 * store-changed events are debounced against the signature recorded here. There is deliberately **no
 * way to refetch everything through an ack**: a wholesale swap (full restore) and a refetch that
 * distrusts the invalidation signal altogether (a watcher gap, a manual refresh) bypass the ack and
 * call `invalidateAllQueries` themselves.
 */
function applyAck(ack: WriteAck): Promise<void> {
  const scopes = new Set(ack.scopes);
  const tasks = new Set(ack.tasks);
  const decisions = new Set(ack.decisions);
  invalidateQueries((key: QueryKey) => {
    switch (key[0]) {
      case "task": return tasks.has(key[1] as number);
      case "decision": return decisions.has(key[1] as number);
      case "taskPage":
      case "smartView": return scopes.has("tasks");
      // The board's dimension assignments (which task sits on which value of which axis). Assigning one,
      // and every edit to the axes themselves, acks with the "tasks" scope — which is what keeps the
      // filter chips and the classification drawn on the cards from answering with a stale map.
      case "dimAssign": return scopes.has("tasks");
      case "archivedProjects": return scopes.has("tasks");
      case "decisions": return scopes.has("decisions");
      case "decisionComments": return decisions.has(key[1] as number);
      case "attachments": return tasks.has(Number(key[2])) || decisions.has(Number(key[2]));
      case "commits": return tasks.has(key[1] as number);
      default: return false;
    }
  });
  return loadSnapshot();
}

/** Browser fallback: mutate the cache in the mock and publish it (with a coarse query refetch). */
function mockMutate(fn: (snap: Snapshot) => Snapshot): void {
  applySnapshot(fn(getSnapshot()));
  invalidateAllQueries(); // Browser iteration: the useQuery-backed views refetch after the mock mutation too.
}

/**
 * Let the mock fail in the same shape core does. The Tauri path rejects with a structured `CmdError`
 * (src-tauri/error.rs) that callers flatten to one line with `errText`; if the mock throws the same
 * shape, browser iteration agrees with the real store down to which operations get refused. A mock
 * that quietly lets everything through would leave the front end believing it works.
 */
function mockErr(
  code: string,
  en: string,
  structured?: { fields?: Record<string, unknown>; parts?: CmdErrorPart[] },
): CmdError {
  return { code, message_en: en, ...structured };
}

/**
 * The reasons a reservation is refused, in the shape core sends them (`ops::task::not_ready`). The mock
 * has the same three premises on the card, so it names the same three reasons; a decision the mock holds
 * no ref for is named by its title, which is the only handle it has. English is written here only as the
 * fallback for a reader whose language has no template — the sentence they actually get is the template.
 */
function notReadyParts(t: TaskCard): CmdErrorPart[] {
  const parts: CmdErrorPart[] = [];
  for (const b of t.blockedBy) {
    parts.push({
      code: "not_ready_open_blocker",
      message_en: `blocker ${b.name} is not done`,
      fields: { ref: b.name },
    });
  }
  for (const d of t.blockedByDecisions) {
    const ref = d.ref ?? d.name ?? String(d.id);
    parts.push({
      code: "not_ready_premise_unsettled",
      message_en: `premise ${ref} is not settled — wait for the ruling, or unlink it`,
      fields: { ref },
    });
  }
  if (t.notStartedUntil) {
    parts.push({
      code: "not_ready_not_started",
      message_en: `it is not due to start until ${t.notStartedUntil}`,
      fields: { start: t.notStartedUntil },
    });
  }
  if (t.draft) {
    parts.push({
      code: "not_ready_draft",
      message_en: "it is still being created — finish creating it first",
      fields: null,
    });
  }
  return parts;
}

/**
 * The mock's copy of core's `not_started_until`: a start day is a reason to wait only while it is still
 * ahead. Dates are `YYYY-MM-DD` and hold no time zone (`AMB-D-429`), so the comparison is the strings'
 * own order, against this machine's calendar day.
 */
function startAhead(start: string | null): string | null {
  return start && start > todayStr() ? start : null;
}

/**
 * The mock's copy of core's readiness derivation: no unfinished blocker, no unsettled grounding decision,
 * the declared start day arrived, and the creation finished (core's `reserve_blockers`, whose emptiness
 * *is* `ready`). It lives in one place so that clearing any one premise cannot come to disagree with
 * clearing another about what the remaining three mean.
 */
function readyOf(t: Pick<TaskCard, "blockedBy" | "blockedByDecisions" | "notStartedUntil" | "draft">): boolean {
  return t.blockedBy.length === 0 && t.blockedByDecisions.length === 0 && t.notStartedUntil == null && !t.draft;
}

/**
 * Rebuild the dependents when one blocker goes away (completed or deleted). In core, `blocked_by` is
 * derived as "blockers not yet done" and `ready` by {@link readyOf}'s four premises; the mock runs the
 * same derivation here. Clearing a blocker cannot clear the other three, so they are re-read rather
 * than assumed away.
 * **The dependency edges themselves are not in the mock fixtures**, though — the snapshot only carries
 * the already-derived `blockedBy` — so reopening a task (done → todo) **cannot put its blockers back**.
 * That is a face we chose not to act out. To see dependencies re-form during browser iteration, edit
 * the fixtures.
 */
function unblock(tasks: TaskCard[], blockerId: number): TaskCard[] {
  return tasks.map((x) => {
    if (!x.blockedBy?.some((b) => b.id === blockerId)) return x;
    const blockedBy = x.blockedBy.filter((b) => b.id !== blockerId);
    return { ...x, blockedBy, ready: readyOf({ ...x, blockedBy }) };
  });
}

function sysRow(target: ActivityTargetDto, event: EventDto): ActivityItem {
  return {
    id: Date.now(),
    // The shared activity counter (`AMB-D-388`): a ledger row is numbered against it, and so is the
    // optimistic stand-in for one.
    seq: 0,
    at: new Date().toISOString(),
    kind: "system",
    author: me(),
    target,
    event,
  };
}

/**
 * A system row about a task. Only the deletion row is `live:false` — what that row reports is the
 * target ceasing to exist, so there is nothing to open. Every other row names a target we just
 * touched, which is therefore still there.
 */
function sysItem(taskId: number, title: string, event: EventDto): ActivityItem {
  return sysRow({ type: "task", id: taskId, title, live: event.kind !== "task.deleted" }, event);
}

/** Returns the id of the task just created, so the caller can open its detail pane. Null if it cannot be resolved. */
export async function addTask(
  projectId: number | null,
  title: string,
  notes?: string,
  due?: string | null,
  start?: string | null,
): Promise<number | null> {
  if (inTauri()) {
    // Pass a project and core places the task there so it lands on that board; its
    // classification (dimension values) is assigned afterwards. With no project it is an inbox
    // (unfiled) task. task_add's WriteAck carries the new task among its affected ids (commands.rs),
    // so we apply the ack and lift the id out of it.
    const ack = await invoke<WriteAck>("task_add", {
      projectId, title, notes: notes ?? null, due: due ?? null, start: start ?? null,
    });
    applyAck(ack);
    return ack.tasks[0] ?? null;
  }
  const id = Date.now();
  mockMutate((s) => {
    // Every field of the wire TaskCardDto is required (core always sends them all), so the mock fills
    // in defaults. Placement is expressed with `projectId` alone and `placement` is left null — the
    // mock has no project names to build one from, and the detail pane falls back to `projectId`
    // (see `placementOf`).

    const task: TaskCard = {
      id, title, notes: notes ?? "", status: "todo", assignee: null, priority: null,
      due: due ?? null, comments: 0, createdBy: me(),
      ref: taskRef(id), projectId, completedAt: null,
      createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(),
      // A creation lands unfinished, exactly as core's `add` leaves it (`AMB-D-554`): the task is on the
      // board and refused a reservation until the detail pane's "finish creating" ends the second stage.
      ready: false, blockedBy: [], placement: null, linkedDecisions: [], blockedByDecisions: [],
      startOn: start ?? null, notStartedUntil: startAhead(start ?? null),
      draft: true,
    };
    return { ...s, tasks: [...s.tasks, task], activity: [sysItem(id, title, { kind: "task.created" }), ...s.activity] };
  });
  return id;
}

/**
 * Finish creating a task — the second stage of the creation `addTask` began (`AMB-D-554`). It clears the
 * fourth premise and nothing else, so the task stops being held out of the mailbox and out of a
 * reservation. One direction: there is no way back, and a task filed by mistake is rejected or deleted.
 * Finishing one already finished is a no-op on both faces.
 */
export async function finishTaskCreation(id: number): Promise<void> {
  if (inTauri()) return invokeAck("task_finish_creating", { id });
  mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => {
      if (x.id !== id || !x.draft) return x;
      const finished = { ...x, draft: false };
      return { ...finished, ready: readyOf(finished) };
    }),
  }));
}

/**
 * Set the status. **Reserving a task (→ in_progress) passes the same two guards core applies**: a
 * compare-and-swap that only fires when the task is currently `todo` (this is what stops two actors
 * starting the same work), and `ready` — no unfinished blocker and no unsettled grounding decision.
 * If the mock waved these through, browser iteration would be the only place a task "could be
 * reserved" that Tauri refuses.
 */
export async function setStatus(id: number, status: Status): Promise<void> {
  if (inTauri()) return invokeAck("task_status", { id, status });
  const t = getSnapshot().tasks.find((x) => x.id === id);
  if (!t) return;
  if (status === "in_progress") {
    if (t.status !== "todo") {
      throw mockErr(
        "already_reserved",
        `cannot reserve task ${t.ref}: it is '${t.status}', not 'todo' (reserve is todo → in_progress)`,
        { fields: { ref: t.ref } },
      );
    }
    if (!t.ready) {
      // Both refusals carry what their template interpolates. Sending the code without the values would
      // put `{ref}` on screen — the template is found either way, and the fields are the only thing that
      // can fill it. The reasons the mock can name come from the same three premises core derives
      // readiness from, so browser iteration agrees on *why* as well as on *whether*.
      const parts = notReadyParts(t);
      throw mockErr(
        "not_ready",
        `cannot reserve task ${t.ref}: its premises (blockers / grounding decisions) are not met`,
        { fields: { ref: t.ref }, parts },
      );
    }
  }
  // Status is the single source of truth for completion; `completedAt` only carries a value while a task is done
  // (`ops::task::set_status`) — a rejection is a terminal but not an achievement, so it leaves the field unset.
  const completedAt = status === "done" ? new Date().toISOString() : null;
  mockMutate((s) => ({
    ...s,
    // A task that closes — either terminal — drops out of the blockers of everything waiting on it (core's
    // `blocked_by` derivation asks `is_closed`, so a rejection releases its dependents just as a completion does).
    tasks: (isClosed(status) ? unblock(s.tasks, id) : s.tasks)
      .map((x) => (x.id === id ? { ...x, status, completedAt } : x)),
    activity: [sysItem(id, t.title, { kind: "task.status_changed", status }), ...s.activity],
  }));
}

/**
 * End a task that will not be done, with the reasoning kept (`AMB-D-397`) — the write behind the pull-down's
 * `rejected`. The reason is **required**: this is what `setStatus` cannot ask for, and the whole point of the
 * separate door (the CLI draws the same line, `task reject --reason` beside `task status`). It lands as a
 * comment on the timeline, so free text keeps its one home.
 */
export async function rejectTask(id: number, reason: string): Promise<void> {
  if (inTauri()) return invokeAck("task_reject", { id, reason });
  // The mock trims and refuses as the command does, so browser iteration is not the one place a task
  // can be rejected with nothing said about why.
  const text = reason.trim();
  if (!text) {
    throw mockErr(
      "invalid_value",
      "a rejection needs its reason — say why the task will not be done",
    );
  }
  const t = getSnapshot().tasks.find((x) => x.id === id);
  if (!t) return;
  if (t.status === "rejected") return; // Idempotent, and the reason is not piled on a second time.
  await setStatus(id, "rejected");
  await addComment(id, text);
}

/**
 * Delete a task — a hard, irreversible delete that takes its comments, dependencies and attachments
 * with it. **The caller must put a confirmation dialog in front of this**, so nothing is lost by a slip.
 */
export async function deleteTask(id: number): Promise<void> {
  if (inTauri()) return invokeAck("task_delete", { id });
  const t = getSnapshot().tasks.find((x) => x.id === id);
  if (!t) return;
  mockMutate((s) => ({
    ...s,
    tasks: unblock(s.tasks, id).filter((x) => x.id !== id),
    activity: [sysItem(id, t.title, { kind: "task.deleted" }), ...s.activity],
  }));
}

/**
 * Change the priority. **No activity row is logged** — the system events core keeps are created /
 * status_changed / assigned / moved / unblocked / deleted (`activity_log::event`), and edits to
 * priority, notes and title are not among them. A row appended here would make the feed busier in
 * browser iteration than it ever is in the real app. `setNotes`, `setTitle` and `updateProject`
 * are silent for the same reason.
 */
export async function setPriority(id: number, priority: Priority | null): Promise<void> {
  if (inTauri()) return invokeAck("task_set_priority", { id, priority });
  const t = getSnapshot().tasks.find((x) => x.id === id);
  if (!t) return;
  mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => (x.id === id ? { ...x, priority } : x)),
  }));
}

/**
 * Set or clear the due date (`YYYY-MM-DD`, or null to take it away). Silent on the timeline for the same
 * reason `setPriority` is: core keeps no system event for an edited date.
 */
export async function setDue(id: number, due: string | null): Promise<void> {
  if (inTauri()) return invokeAck("task_set_due", { id, due });
  mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => (x.id === id ? { ...x, due } : x)),
  }));
}

/**
 * Set or clear the start day (`YYYY-MM-DD`, or null to take it away). Unlike the due date this one is a
 * premise: while the day is still ahead the task cannot be reserved, so the mock re-derives that here —
 * browser iteration shows the same card the desktop would.
 */
export async function setStart(id: number, start: string | null): Promise<void> {
  if (inTauri()) return invokeAck("task_set_start", { id, start });
  mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => {
      if (x.id !== id) return x;
      const notStartedUntil = startAhead(start);
      return { ...x, startOn: start, notStartedUntil, ready: readyOf({ ...x, notStartedUntil }) };
    }),
  }));
}

/**
 * Create a project (the equivalent of core/CLI `init`), called from the input step of the creation
 * screen. `name` is required — the front end guarantees it is non-empty and core defensively fills in
 * a default name anyway. `dir` **binds that folder to the new project** (an internal `init`: it drops a
 * `.amenbo` pointer and the AI guide into the folder), and on the desktop it is what makes the project
 * reachable at all, so the screen does not offer a create without one (`AMB-D-532`). Afterwards it
 * **awaits** the `loadSnapshot` refresh and returns the project's id, so the caller can navigate
 * straight to its board; null if that cannot be resolved. Outside Tauri (browser iteration) `dir` is
 * ignored — the mock binds nothing — and the project is faked into the cache.
 */
export async function createProject(name: string, dir: string | null): Promise<number | null> {
  if (inTauri()) {
    const before = new Set(getSnapshot().projects.map((p) => p.id));
    // One command, because there is one kind of project on the desktop: bound to the folder it was made
    // for. A null that reached here anyway is refused at the boundary, which is the right end for it —
    // better than a project no AI can reach.
    const ack = await invoke<WriteAck>("project_add_folder", { dir, name });
    // Wait for the refreshed snapshot to carry the new project before resolving its id: it is the one that was not there before the call.
    await applyAck(ack);
    return getSnapshot().projects.find((p) => !before.has(p.id))?.id ?? null;
  }
  const id = Date.now();
  mockMutate((s) => ({
    ...s,
    projects: [
      ...s.projects,
      { id, name, color: "#6b7280", view: "board", openCount: 0, proposedDecisionCount: 0, dimensions: [] },
    ],
  }));
  return id;
}


/**
 * Fetch one project's editable fields (name/notes/color/view/archived) to prefill the project
 * settings screen. The snapshot's ProjectDto does not carry notes or archived, so they are hydrated
 * from core only when that screen is opened. Outside Tauri (browser iteration) the value is assembled
 * from the cached ProjectDto — notes come back empty because the cache has none, archived as false.
 * Null if the project is not found.
 */
export async function fetchProjectSettings(projectId: number): Promise<ProjectSettingsDto | null> {
  if (inTauri()) {
    try {
      return await invoke<ProjectSettingsDto>("project_get", { projectId });
    } catch {
      return null;
    }
  }
  const p = getSnapshot().projects.find((x) => x.id === projectId);
  return p ? { id: p.id, name: p.name, notes: "", color: p.color, view: p.view, archived: false } : null;
}

/**
 * Update a project's settings — rename, notes, color, default view (the same shape as CLI `project
 * update`). Only the fields passed are changed; undefined leaves a field as it is. Outside Tauri the
 * cached ProjectDto is swapped in place (notes are not in the cache, so they are not reflected).
 */
export async function updateProject(
  projectId: number,
  patch: { name?: string; notes?: string; color?: string; view?: ProjectDto["view"] },
): Promise<void> {
  if (inTauri()) {
    return invokeAck("project_update", {
      projectId,
      name: patch.name ?? null,
      notes: patch.notes ?? null,
      color: patch.color ?? null,
      view: patch.view ?? null,
    });
  }
  mockMutate((s) => ({
    ...s,
    projects: s.projects.map((p) =>
      p.id === projectId
        ? { ...p, name: patch.name ?? p.name, color: patch.color ?? p.color, view: patch.view ?? p.view }
        : p,
    ),
  }));
}

/**
 * Reorder a project — this is what the sidebar's drag-and-drop calls. A `position` of `before` or
 * `after` needs an `anchorId`: the id of the project to sit next to.
 * Outside Tauri (browser iteration) the cached projects array is reordered by the same rules, since
 * there the array order is the ordering.
 */
export async function moveProject(
  projectId: number,
  position: "top" | "bottom" | "before" | "after",
  anchorId?: number,
): Promise<void> {
  if (inTauri()) {
    return invokeAck("project_move", {
      projectId,
      position,
      anchorId: anchorId ?? null,
    });
  }
  mockMutate((s) => {
    const moved = s.projects.find((p) => p.id === projectId);
    if (!moved) return s;
    const rest = s.projects.filter((p) => p.id !== projectId);
    let insertAt: number;
    switch (position) {
      case "top":
        insertAt = 0;
        break;
      case "bottom":
        insertAt = rest.length;
        break;
      case "before": {
        const i = rest.findIndex((p) => p.id === anchorId);
        insertAt = i < 0 ? rest.length : i;
        break;
      }
      case "after": {
        const i = rest.findIndex((p) => p.id === anchorId);
        insertAt = i < 0 ? rest.length : i + 1;
        break;
      }
    }
    return { ...s, projects: [...rest.slice(0, insertAt), moved, ...rest.slice(insertAt)] };
  });
}

/**
 * Archive or unarchive a project (the same shape as CLI `project archive` / `unarchive`). The sidebar
 * in browser iteration only shows live projects, so archiving drops the project from the cached list
 * while its tasks stay: archiving hides, and only deletion removes. **Unarchiving is not acted out** —
 * a dropped project is no longer in the cache and cannot be brought back, because the archived list
 * can only be read from core.
 */
export async function setProjectArchived(projectId: number, archived: boolean): Promise<void> {
  if (inTauri()) {
    return invokeAck("project_set_archived", { projectId, archived });
  }
  mockMutate((s) => ({ ...s, projects: archived ? s.projects.filter((p) => p.id !== projectId) : s.projects }));
}

/**
 * Delete a project (the same shape as CLI `project delete`). Its tasks and decisions **go with it** —
 * core's `ops::project::delete` hard-deletes the whole subtree. Keeping them and merely hiding the
 * project is archiving's job.
 */
export async function deleteProject(projectId: number): Promise<void> {
  if (inTauri()) {
    return invokeAck("project_delete", { projectId });
  }
  const gone = getSnapshot().projects.find((p) => p.id === projectId);
  mockMutate((s) => {
    // Drop the same subtree core drops. Removing only the project row would leave tasks and decisions
    // in the cache still belonging to a project that no longer exists, and browser iteration would be
    // looking at a store Tauri never shows.
    const tasks = s.tasks.filter((t) => t.projectId !== projectId);
    const decisions = s.decisions.filter((d) => d.project?.id !== projectId);
    // The counts the line reports are what actually went, so the browser reads the same sentence
    // Tauri would write from the ledger.
    const went = { tasks: s.tasks.length - tasks.length, decisions: s.decisions.length - decisions.length };
    return {
      ...s,
      projects: s.projects.filter((p) => p.id !== projectId),
      tasks,
      decisions,
      activity: gone
        ? [
            sysRow({ type: "project", id: projectId, title: gone.name, live: false }, { kind: "project.deleted", ...went }),
            ...s.activity,
          ]
        : s.activity,
    };
  });
}

/**
 * Folder management on the project settings screen: look up, in reverse, the folders bound to this
 * project — the ones whose `.amenbo` points at it (many folders may point at one project). Each folder
 * comes with an existence flag; a stale one (moved or deleted) reports `exists:false`.
 * Outside Tauri (browser iteration) nothing touches the filesystem, so this is an empty array.
 */
export async function fetchBoundFolders(projectId: number): Promise<BoundFolderDto[]> {
  if (!inTauri()) return [];
  return await invoke<BoundFolderDto[]>("project_bound_folders", { projectId });
}

/**
 * Folder management on the project settings screen: bind an existing folder to this existing project
 * (the Tauri path for `bind --project`). It drops a `.amenbo` pointer and the AI guide into the folder,
 * which is what lets an AI started there operate on this project. If an ancestor is already an
 * Amenbo-managed tree, Rust refuses with `binding_nested_tree`. Outside Tauri this is a no-op.
 */
export async function bindFolder(projectId: number, dir: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("project_bind_folder", { projectId, dir });
}

/**
 * Folder management on the project settings screen: unbind this folder (the Tauri path for `unbind`).
 * It removes only that folder's `.amenbo` and managed block; the store itself stays, and the other
 * folders bound to the same project are untouched. Even for a stale folder (moved or deleted) the
 * registration is still cleaned up. Outside Tauri this is a no-op.
 */
export async function unbindFolder(dir: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("project_unbind_folder", { dir });
}

/**
 * After the binary is updated, the CLAUDE.md / AGENTS.md in a bound folder can be left holding a
 * managed block from the older version. This lists those folders through the same core detection path
 * as CLI `doctor` (`agents::stale_bound_blocks`) — read-only, nothing is rewritten.
 * Outside Tauri (browser iteration) nothing touches the filesystem, so this is an empty array.
 */
export async function fetchStaleManagedBlocks(): Promise<StaleBlockDto[]> {
  if (!inTauri()) return [];
  return await invoke<StaleBlockDto[]>("stale_managed_blocks");
}

/**
 * Re-sync stale managed blocks to the current version (the same core path as CLI `sync-guide`,
 * `agents::resync_bound_blocks`). With `dir` it does one folder, without it every bound folder. It
 * writes only when the content actually changes (low churn), keeps the language label, and never
 * touches anything outside the markers. It writes markdown on disk and does not change the store, so
 * there is no snapshot to refresh. Outside Tauri it returns an empty report.
 */
export async function resyncManagedBlocks(dir?: string): Promise<ResyncReportDto> {
  if (!inTauri()) return { scanned: 0, updated: [] };
  return await invoke<ResyncReportDto>("resync_managed_blocks", { dir: dir ?? null });
}

/**
 * List the bound-folder rows that no live project claims — debris a deleted project left behind in the
 * index (same core detection path as CLI `doctor`, `binding::orphan_dirs`; read-only). The folder list
 * is looked up in reverse from each project, so these rows structurally cannot appear there: this is
 * the only surface in the GUI that shows them. Outside Tauri this is an empty array.
 */
export async function fetchOrphanBindings(): Promise<string[]> {
  if (!inTauri()) return [];
  return await invoke<string[]>("orphan_bindings");
}

/**
 * List the bound folders whose `.amenbo` is broken — an old format, or gone (same core detection path
 * as CLI `doctor`, `doctor::pointer_issues`; read-only). The startup health banner calls this **exactly
 * once, at startup**: the snapshot (`startupHealth`) is refetched on every store-changed tick, and a
 * filesystem walk proportional to the number of bound folders must not ride along on that path.
 * Outside Tauri this is an empty array.
 */
export async function fetchPointerIssues(): Promise<DoctorIssueDto[]> {
  if (!inTauri()) return [];
  return await invoke<DoctorIssueDto[]>("pointer_issues");
}

/**
 * The lint-hook question waiting for the user, or null when there is none — **there is one of it on this
 * device, ever**, not one per bound repository. Called **exactly once, at startup**, for the reason
 * `fetchPointerIssues` is: it probes the filesystem once per bound folder, which has no business on the
 * store-changed tick.
 *
 * The judgment of what to ask is core's (`hooks::reconcile`) — the same one the CLI puts its terminal
 * prompt behind. What comes back is only the material to word the question from. The same call carries an
 * answer already given out to the folders bound since it, which is why it is a command and not a read.
 * Outside Tauri there is never a question.
 */
export async function fetchHookOffer(): Promise<HookOfferDto | null> {
  if (!inTauri()) return null;
  return await invoke<HookOfferDto | null>("hook_offer");
}

/**
 * The bound repositories where the lint is not actually running (core's `hooks::setup_notice`) — the
 * standing report behind the banner, as opposed to `fetchHookOffer`'s one-time question.
 *
 * Called **once, after the modal has had its turn**, which is an ordering rather than a convenience:
 * this probes the disk that answering the question just changed, and a notice read any earlier would
 * name slots that are now wired. Outside Tauri this is an empty array.
 */
export async function fetchHookNotices(): Promise<HookNoticeDto[]> {
  if (!inTauri()) return [];
  return await invoke<HookNoticeDto[]>("hook_notices");
}

/**
 * Record the device's answer about the lint hooks, and wire every bound repository on a yes. One answer,
 * once — it takes no repository, because it is not about one.
 *
 * **Call it only when there is an answer.** The record decides whether the question is ever asked
 * again, so a dismissed modal must call nothing at all and leave the device unanswered for the next
 * startup — which is why there is no third value to pass here. Throws only if the answer itself could not
 * be recorded; a repository that could not be wired does not cost the answer, since it was about all of
 * them.
 */
export async function answerHookOffer(yes: boolean): Promise<void> {
  if (!inTauri()) return;
  await invoke("hook_answer", { yes });
}

/**
 * Whether the tick's banner has a question to put on this device today (`AMB-D-718`). The whole
 * judgement is core's — the answer on record, a day already put off, work with days on it, and something
 * installed to carry the warning outward.
 *
 * Called **once, at app startup**. It is read then rather than on every store tick because the answer it
 * turns on is the device's and does not move while the app is open: the pass that settles it against the
 * scheduler has already run by the time the window draws. Outside Tauri there is no scheduler and no
 * question.
 */
export async function fetchTickBanner(): Promise<boolean> {
  if (!inTauri()) return false;
  return await invoke<boolean>("tick_banner");
}

/**
 * Record the device's answer about the hourly tick: a yes registers the timer with this machine's
 * scheduler first, a no takes it away first, and only then is the answer written.
 *
 * Both faces come through here — the band that puts the question (`AMB-D-718`) and the settings switch
 * that is the way back from a no. **Call it only when there is an answer.** "Later" is not one: it puts
 * the band off for the day ({@link deferTickBanner}) and leaves the question open, which is why there is
 * no third value here.
 *
 * Throws if the scheduler refused, and nothing is recorded when it does: the config must never claim a
 * timer the machine is not holding, nor drop one it is. `config.json` sits outside the store, so the ack
 * reloads the snapshot — which is what carries a band's answer to the settings switch.
 */
export async function answerTick(yes: boolean): Promise<void> {
  if (inTauri()) return invokeAck("tick_answer", { yes });
  mockMutate((s) => ({ ...s, tickConsent: yes ? "yes" : "no" }));
}

/**
 * Put the tick's banner off until tomorrow. It records the day and nothing else — the question stays
 * unanswered, and the banner is back the next day the conditions hold.
 */
export async function deferTickBanner(): Promise<void> {
  if (!inTauri()) return;
  await invoke("tick_banner_later");
}

/**
 * The nudges core says are due now, as the ids they are declared under (`AMB-D-542`, `AMB-D-544`). The
 * judgement is core's — which thresholds are met, and what has already gone out — so what comes back is
 * only which of them to put; the wording and the look are this surface's.
 *
 * `openStages` is this surface's half of it: the stages it is currently in, since the settings a stage
 * asks about are held here. A stage left off the list holds its nudge back, so what cannot be vouched
 * for stays unput.
 *
 * Outside Tauri there is never a nudge.
 */
export async function fetchPendingNudges(openStages: string[]): Promise<string[]> {
  if (!inTauri()) return [];
  return await invoke<string[]>("pending_nudges", { openStages });
}

/**
 * Tell core a nudge has been put. **Call it once the nudge is on screen**, not when it came back due: a
 * once-only nudge recorded and never shown is one the person never saw, and nothing will raise it again.
 */
export async function markNudgePut(nudgeId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("mark_nudge_put", { nudgeId });
}

/**
 * What one project still has to wire, grouped by harness — the standing row on the project screen, which
 * is the GUI's only face for this (`AMB-D-459`, `AMB-D-460`). Each entry carries the tool's request once
 * and the folders of this project waiting for it, so the text goes up a single time however many folders
 * are behind it.
 *
 * Empty once every folder is wired, which is how the row goes away; empty as well where the project
 * answered no, since core keeps a refusal silent. Refetched when the project changes, not on every store
 * tick: what it reads is settings files on disk, which a task moving on the board does not touch.
 * Outside Tauri this is an empty array.
 */
export async function fetchAgentHookProjectWiring(projectId: number): Promise<AgentHookWiringDto[]> {
  if (!inTauri()) return [];
  return await invoke<AgentHookWiringDto[]>("agent_hook_project_wiring", { projectId });
}

/**
 * The whole harness catalog and this project's folders, so the request for any tool can be taken from the
 * settings screen whatever is already wired (`AMB-D-670`).
 *
 * It is a second read rather than a filter over `fetchAgentHookProjectWiring` because that one goes quiet:
 * the wiring landing, or a refusal, empties it — and the reader who moved from one tool to another is
 * exactly the reader it is empty for. This one hangs on neither.
 *
 * Outside Tauri there is no catalog to read, so it answers with nothing on both lists.
 */
export async function fetchAgentHookRequests(projectId: number): Promise<AgentHookRequestsDto> {
  if (!inTauri()) return { tools: [], dirs: [] };
  return await invoke<AgentHookRequestsDto>("agent_hook_requests", { projectId });
}

/**
 * The projects an app can be let reach, and every app with what it already holds (`AMB-D-673`,
 * `AMB-D-681`).
 *
 * Read when the screen opens rather than on every store tick: what it reads is settings files on
 * other apps' disks, which nothing Amenbo does can move. The browser iteration answers with nothing —
 * there is no app on this machine to ask about.
 */
export async function fetchMcpSetup(): Promise<McpSetupDto | null> {
  if (!inTauri()) return null;
  return await invoke<McpSetupDto>("mcp_setup");
}

/**
 * The two texts one app's row hands over, for the projects ticked on it.
 *
 * Fetched as the ticks move rather than when a button is pressed: copying is a synchronous move on a
 * text the row is already holding, and a button that had to go and ask first could hand over an empty
 * clipboard with no second chance to notice.
 */
export async function fetchMcpRequest(app: string, projectIds: number[]): Promise<McpRequestDto> {
  if (!inTauri()) return { add: "", remove: "" };
  return await invoke<McpRequestDto>("mcp_request_for", { app, projectIds });
}

/**
 * Write the bundle for the projects ticked into a folder the reader picks, and hand back where it
 * landed (`AMB-D-672`). `null` is the picker having been closed without one.
 *
 * The folder is asked for rather than chosen here: what happens to this file next is that the reader
 * opens it, so it has to land somewhere they will find it again.
 */
export async function saveMcpBundle(projectIds: number[]): Promise<string | null> {
  if (!inTauri()) return null;
  const into = await pickFolder();
  if (into === null) return null;
  return await invoke<string>("mcp_bundle_write", { projectIds, intoDir: into });
}

/**
 * Record what this **project** answered about starting its AI on amenbo. It takes a project rather than
 * the device: the answer changes with the place, so unlike the lint's it is not one answer for everything.
 *
 * **Call it only when there is an answer.** On this surface that means the row's "no" — the close button
 * records nothing and leaves the project unanswered, which is the same third value `answerHookOffer` has
 * no room for (`AMB-D-460`). A yes wires nothing either: Amenbo writes no provider settings file, so what
 * a yes buys is the text, and the row keeps reporting until the paste lands.
 */
export async function answerAgentHookOffer(projectId: number, yes: boolean): Promise<void> {
  if (!inTauri()) return;
  await invoke("agent_hook_answer", { projectId, yes });
}

/**
 * What this project answered about starting its AI on Amenbo — true, false, or null where it has never
 * been asked. The third value is the one the project settings screen exists to show: a refusal is the
 * answer that takes the standing row away, and an unanswered project looks no different once the wiring
 * has landed.
 *
 * It says what was answered and nothing about the wiring, which is read from the folder every time
 * (`fetchAgentHookProjectWiring`). Outside Tauri there is no record to read.
 */
export async function fetchAgentHookConsent(projectId: number): Promise<boolean | null> {
  if (!inTauri()) return null;
  return await invoke<boolean | null>("agent_hook_consent", { projectId });
}

/**
 * Forget this project's answer, putting it back to never having been asked, so the standing row comes
 * back the next time the project is opened. This is the only way back from a no.
 *
 * Clearing a project that never answered is the state asked for, not an error.
 */
export async function clearAgentHookConsent(projectId: number): Promise<void> {
  if (!inTauri()) return;
  await invoke("agent_hook_consent_clear", { projectId });
}

/**
 * Repair broken `.amenbo` pointers — old format or missing (the same core path as CLI `doctor --fix`,
 * `binding::repair_pointers`). Only folders whose owner is unambiguous are rewritten; the ambiguous
 * ones come back under `unresolved`. It writes each folder's `.amenbo` and nothing in the store, so
 * there is no snapshot to refresh. Outside Tauri it returns an empty report.
 */
export async function repairPointers(): Promise<PointerRepairDto> {
  if (!inTauri()) return { repaired: [], unresolved: [] };
  return await invoke<PointerRepairDto>("repair_pointers");
}

/**
 * Forget the orphaned folder rows in the index (the same core path as CLI `doctor --fix`,
 * `Store::forget_orphan_dirs`). It drops index rows only — neither the folders' contents nor their
 * `.amenbo` files are touched. Returns how many were forgotten; 0 outside Tauri.
 */
export async function forgetOrphanBindings(): Promise<number> {
  if (!inTauri()) return 0;
  return await invoke<number>("forget_orphan_bindings");
}

/**
 * The check results the doctor surface (Settings > Integrity) reads. Through the same core path as CLI
 * `amenbo doctor` (`doctor::report`), it returns both the store's internal consistency and the state of
 * this machine's bound folders (`.amenbo` and the AI guide) in one report; read-only.
 * Outside Tauri (browser iteration) neither the filesystem nor the store is touched, so it reports a clean bill.
 */
export async function fetchDoctorReport(): Promise<DoctorReportDto> {
  if (!inTauri()) return { ok: true, errors: 0, warnings: 0, issues: [] };
  return await invoke<DoctorReportDto>("doctor_report");
}

/**
 * Run the repairs (the same core cleanup entry point as CLI `doctor --fix`). Everything it sweeps is
 * something no live read refers to — attachments with a zero reference count, folder rows no project
 * claims — so all of it is non-destructive and the caller asks for no confirmation. There is nothing
 * for a snapshot or a query to refetch either, which is why no ack comes back. Outside Tauri it does nothing.
 */
export async function runDoctorFix(): Promise<DoctorFixDto> {
  if (!inTauri()) return { sweptAttachments: 0, reclaimedBlobs: 0, freedBytes: 0, forgottenBindings: 0 };
  return await invoke<DoctorFixDto>("doctor_fix");
}

/**
 * The shortest path to an update: open this OS's all-in-one installer (GUI and CLI together) in the
 * default browser. Core resolves the installer URL for the current platform from the published
 * latest.json, falling back to the latest release page if it has not been fetched or lists no
 * installer for us, and opens it. There is no self-update — it only opens. Returns the URL it opened.
 * Outside Tauri (browser iteration) it does nothing and returns null.
 */
export async function openLatestInstaller(): Promise<string | null> {
  if (!inTauri()) return null;
  return await invoke<string>("open_latest_installer");
}

/** What the in-app self-update is doing, for the banner to draw. `downloading` carries bytes so far and
 *  the total when the manifest gave one (`null` = size unknown → an indeterminate bar). */
export type UpdateProgress =
  | { phase: "checking" }
  | { phase: "downloading"; downloaded: number; total: number | null }
  | { phase: "installing" }
  | { phase: "ready" };

/**
 * Run the in-app self-update. It asks the Tauri updater manifest (`latest-tauri.json`) whether a newer
 * signed build exists and, if so, downloads it — minisign verification is mandatory and cannot be
 * disabled — then installs it in place, reporting progress through `onProgress`. The caller restarts
 * (`restartApp`) once this resolves with `true`, which is the user-pressed apply step; there is no
 * silent background update. Returns `false` when the manifest offers nothing newer (the banner then
 * falls back to opening the installer), and outside Tauri it returns `false` too. The detection that
 * raises the banner still comes from core's `latest.json` (`version_status`); this only runs once the
 * user acts on it.
 */
export async function installUpdate(onProgress: (p: UpdateProgress) => void): Promise<boolean> {
  if (!inTauri()) return false;
  const { check } = await import("@tauri-apps/plugin-updater");
  onProgress({ phase: "checking" });
  const update = await check();
  if (!update) return false;
  let downloaded = 0;
  let total: number | null = null;
  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? null;
        onProgress({ phase: "downloading", downloaded: 0, total });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress({ phase: "downloading", downloaded, total });
        break;
      case "Finished":
        onProgress({ phase: "installing" });
        break;
    }
  });
  onProgress({ phase: "ready" });
  return true;
}

/** Exit and relaunch the executable (the "restart to apply" step after an in-app update installs). No
 *  network, no store write — it just becomes the binary already on disk. Outside Tauri it does nothing. */
export async function restartApp(): Promise<void> {
  if (!inTauri()) return;
  await invoke("restart_app");
}

/**
 * The general assignment call. The facet (kind = the human, or that person's AI) is what a task is
 * assigned to; kind=null unassigns. Corresponds to the CLI's task assign.
 */
export async function setAssignee(id: number, kind: Facet | null): Promise<void> {
  if (inTauri()) return invokeAck("task_assign", { id, kind });
  const snap = getSnapshot();
  const t = snap.tasks.find((x) => x.id === id);
  if (!t) return;
  if (kind === null) {
    return mockMutate((s) => ({
      ...s,
      tasks: s.tasks.map((x) => (x.id === id ? { ...x, assignee: null } : x)),
      activity: [sysItem(id, t.title, { kind: "task.assigned" }), ...s.activity],
    }));
  }
  const name = snap.roster.find((a) => a.kind === kind)?.name ?? kind;
  return mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => (x.id === id ? { ...x, assignee: { name, kind } } : x)),
    activity: [
      sysItem(id, t.title, { kind: "task.assigned", toKind: kind }),
      ...s.activity,
    ],
  }));
}

/**
 * Open the native folder picker and return the absolute path chosen — this backs every place a folder
 * is asked for: the creation screen's field, the settings screen's list, and the terminal face's way in
 * (`chooseWorkFolder`). Null if the dialog is cancelled. **It does not read the folder's contents**: it
 * returns a path string, and the actual binding (placing the `.amenbo` pointer) is done on the Rust
 * side by whatever the caller does with it. Outside Tauri (browser) nothing touches the filesystem, so
 * this is a no-op returning null.
 */
export async function pickFolder(): Promise<string | null> {
  if (!inTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false });
  return typeof dir === "string" ? dir : null; // anything but a string means the dialog was cancelled
}

/**
 * Choose the folder to work in, and make it one that can be worked in — the first run's way in
 * (`AMB-T-3606`).
 *
 * **One press is the whole of it.** Picking a folder there means three things at once: the folder is
 * the one the AI is shown, it belongs to a project (raised here and named after the folder if it did
 * not already), and a terminal opens in it. Nothing is typed and nothing is submitted, so there is no
 * step between the dialog closing and the pane opening — which is why this binds rather than handing
 * the caller a path to bind later, and why a folder that already belongs to a project comes back
 * unchanged instead of as a refusal (`folder_open`).
 *
 * **It is the road for a machine that has no project yet**, and that is the only place left that
 * takes it: a pane belongs to a project, so wherever there is one to belong to the folder is chosen
 * among that project's own (`chooseFolderFor`, `../talk/agent`). Raising a project from a folder's
 * name is right when there is none to raise it beside, and wrong the moment there is.
 *
 * Returns the folder chosen, or null where nothing was — the dialog cancelled, or the browser
 * iteration, which has no folders to choose from. Anything the binding refuses is thrown, because the
 * caller has an invitation on screen to say it on.
 */
export async function chooseWorkFolder(): Promise<string | null> {
  const dir = await pickFolder();
  if (dir === null) return null;
  await invokeAck("folder_open", { dir });
  return dir;
}

/**
 * Choose this project's **first** folder — the way in, on either face, for a project bound to none.
 *
 * It differs from `chooseWorkFolder` in the one way that matters there: the folder is bound to the
 * project the person is already looking at, rather than to whatever project the folder's own name
 * would raise. A pane belongs to a project (`../talk/layout`), so a folder chosen to open a pane in
 * has to end up in that project or the pane would be somewhere else entirely. The board's face takes
 * it from the rail (`../shell/TerminalFace`) and the window split out of it from the arrangement the
 * board left (`../talk/agent`).
 *
 * Returns the folder chosen, or null where nothing was — the dialog cancelled, or the browser
 * iteration, which has no folders to choose from. A binding the host refuses is thrown, because the
 * caller has the question on screen to say it on.
 */
export async function chooseFolderFor(projectId: number): Promise<string | null> {
  const dir = await pickFolder();
  if (dir === null) return null;
  await bindFolder(projectId, dir);
  return dir;
}

/**
 * One of the "what next" actions on the project-created screen: reveal the bound folder in the OS file
 * manager (Finder/Explorer). Read-only, so no ack. Outside Tauri (browser iteration) it is a no-op.
 */
export async function revealFolder(path: string): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("reveal_folder", { path });
}

/**
 * One of the "what next" actions on the project-created screen: open the bound folder in a terminal,
 * where `amenbo status` can be run. Outside Tauri (browser iteration) it is a no-op.
 */
export async function openTerminal(path: string): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("open_terminal", { path });
}

/**
 * Open an external URL in the default browser on the user's initiative (the TopBar's "Amenbo" link to
 * the product page). This is the user launching a browser, not the app talking to the network, so it
 * does not breach the no-network policy. Outside Tauri (`npm run dev` browser iteration) it falls back
 * to `window.open`.
 */
export async function openExternalUrl(url: string): Promise<void> {
  if (!inTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  try {
    await openUrl(url);
  } catch (e) {
    // Callers fire this as `void openExternalUrl(...)`, so a rejection would vanish silently — and the
    // opener plugin rejects when the URL is outside the granted scope, which looks exactly like "the
    // link does nothing." Surface it instead of swallowing it.
    console.error("[amenbo] openExternalUrl failed:", url, e);
  }
}

/**
 * Set or clear a facet's (human / AI) avatar image. A `dataUrl` (data:image/…) sets it; `null` reverts
 * to the identicon. Shrink the image before passing it in (use `fileToAvatarDataUrl`). It lives in
 * `config.human_avatar` / `ai_avatar`, so it comes back through snapshot.roster. Only the facet named
 * is sent; the other is left out of the payload entirely, which means "leave as is" — clearing is the
 * empty string. Outside Tauri (browser) the cached roster is edited directly.
 */
export async function setFacetAvatar(kind: "human" | "ai", dataUrl: string | null): Promise<void> {
  if (inTauri()) {
    // Keep all three states distinct: the named facet is Some (empty string = clear), the other is undefined = leave as is.
    const value = dataUrl ?? "";
    return invokeAck("set_facet_avatars", {
      humanAvatar: kind === "human" ? value : undefined,
      aiAvatar: kind === "ai" ? value : undefined,
    });
  }
  mockMutate((s) => ({
    ...s,
    roster: s.roster.map((a) => (a.kind === kind ? { ...a, avatar: dataUrl ?? undefined } : a)),
  }));
}

/**
 * Rename yourself (the human / ai facets). This applies core's rename to the two names held in config,
 * the same path onboarding uses. Core trims surrounding whitespace and refuses an empty name. Once
 * saved, watch_store and loadSnapshot bring roster[].name back and the UI updates on its own. Outside
 * Tauri (browser) it only rewrites the cached roster.
 */
export async function setFacetNames(humanName: string | null, aiName: string | null): Promise<void> {
  if (inTauri()) return invokeAck("set_facet_names", { humanName, aiName });
  mockMutate((s) => ({
    ...s,
    roster: s.roster.map((a) =>
      a.kind === "human" && humanName?.trim() ? { ...a, name: humanName.trim() }
      : a.kind === "ai" && aiName?.trim() ? { ...a, name: aiName.trim() }
      : a,
    ),
  }));
}

/**
 * Shrink an image File to a small square PNG data URL (96px by default). It centre-crops so the result
 * sits well in a round avatar, and keeps the store from bloating — core enforces an upper bound as
 * well. It goes through canvas, so it works the same in a browser and in the Tauri webview.
 */
export async function fileToAvatarDataUrl(file: File, size = 96): Promise<string> {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const el = new Image();
      el.onload = () => resolve(el);
      el.onerror = () => reject(new Error("画像を読み込めませんでした"));
      el.src = url;
    });
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas を初期化できませんでした");
    // Crop the centre square, then draw it at size×size.
    const side = Math.min(img.width, img.height);
    const sx = (img.width - side) / 2;
    const sy = (img.height - side) / 2;
    ctx.drawImage(img, sx, sy, side, side, 0, 0, size, size);
    return canvas.toDataURL("image/png");
  } finally {
    URL.revokeObjectURL(url);
  }
}

/**
 * Change the user's language (`config.language`) after the fact. What re-renders i18n **without a
 * restart** is the `loadSnapshot` that follows the ack, which re-reads the snapshot's language — and
 * because that load also records the store signature, the `store-changed` our own write now raises
 * (config.json is part of that signature) is filtered out as ours rather than costing a second
 * re-read. Outside Tauri (browser) it just swaps the cached language.
 */
export async function setLanguage(language: string): Promise<void> {
  if (inTauri()) return invokeAck("config_set_language", { language });
  mockMutate((s) => ({ ...s, language }));
}

/**
 * Settle the language on a first launch, instead of asking for it. Nobody is asked which language to
 * read in, so with `config.language` still unset the OS's answer is written as if it had been
 * chosen — through the same path the settings screen uses, which is what also carries it into the
 * managed block of every bound folder (`AGENTS.md` / `CLAUDE.md`). Leaving it unset instead would show
 * a Japanese reader a Japanese window while telling their AI to write English.
 *
 * It runs on every startup and writes on almost none of them: a language already chosen — including
 * one settled here on an earlier launch — is never overwritten, so this is not a way for the OS to
 * keep re-deciding a question the user has answered.
 */
export async function settleLanguage(): Promise<void> {
  if (getSnapshot().language?.trim()) return;
  await setLanguage(guessLang());
}

// ───────────────────────── Developer settings, backup / restore ─────────────────────────

/**
 * Switch the perf-instrumentation level (`config.perf_log`). On the core side, config_set_perf_log
 * reloads the tracing filter in place. The front-end instrumentation gate (ipc.ts) would not follow:
 * config_set_perf_log's ack carries no scope, so loadSnapshot does not propagate it immediately —
 * hence `applyPerfConfig` is called directly here to bring both sides into step **without a restart**.
 */
export async function setPerfLog(mode: string): Promise<void> {
  if (inTauri()) {
    await invokeAck("config_set_perf_log", { mode });
    applyPerfConfig(mode);
    return;
  }
  mockMutate((s) => ({ ...s, perfLog: mode }));
  applyPerfConfig(mode);
}

/**
 * Turn the update check (`config.update_check`) on or off. `config.json` lives outside the store, so it
 * is the `loadSnapshot` at the tail of the ack that re-reads the snapshot's `updateCheck` and brings the
 * toggle — and, when switching off, the upstream update banner — into step **without a restart**.
 * Outside Tauri (browser) it just swaps the cached `updateCheck`.
 */
export async function setUpdateCheck(enabled: boolean): Promise<void> {
  if (inTauri()) return invokeAck("config_set_update_check", { enabled });
  mockMutate((s) => ({ ...s, updateCheck: enabled }));
}

/**
 * Change the view a new project opens in (`config.default_view`). `config.json` lives outside the
 * store, so it is the `loadSnapshot` at the tail of the ack that re-reads the snapshot's
 * `defaultView` and brings the pull-down into step **without a restart**. Nothing else on screen
 * moves: every project already carries its own view, and this is only the answer for the next one
 * created without one. Outside Tauri (browser) it just swaps the cached `defaultView`.
 */
export async function setDefaultView(view: ViewKind): Promise<void> {
  if (inTauri()) return invokeAck("config_set_default_view", { view });
  mockMutate((s) => ({ ...s, defaultView: view }));
}

/**
 * Turn "start when I log in" (`config.autostart`) on or off. Core writes the OS registration first and
 * saves the setting only if that came back, so the `autostart` the ack's `loadSnapshot` re-reads is
 * always the one the login actually does — a rejection leaves the switch where it was rather than
 * moving it onto a registration that was never written. Outside Tauri (browser) there is no login to
 * register with, so it just swaps the cached value.
 */
export async function setAutostart(enabled: boolean): Promise<void> {
  if (inTauri()) return invokeAck("config_set_autostart", { enabled });
  mockMutate((s) => ({ ...s, autostart: enabled }));
}

/** The timestamp in the default backup filename — colon-free, so it is legal in a filename. */
function backupStamp(): string {
  return new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
}

/** The file extension of a whole-store archive. Mirrors core's `ARCHIVE_EXT` — rename one and you must rename the other. */
export const WORLD_ARCHIVE_EXT = "amenbo-backup";

/**
 * Choose, in a save dialog, where the whole-store backup goes: one archive holding every project on
 * this machine plus the root overview store. Null on cancel, and outside Tauri (browser mock).
 */
export async function pickBackupPath(): Promise<string | null> {
  if (!inTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({
    title: t("settings.backupDialogTitle"),
    defaultPath: `amenbo-backup-${backupStamp()}.${WORLD_ARCHIVE_EXT}`,
    filters: [{ name: "Amenbo backup", extensions: [WORLD_ARCHIVE_EXT] }],
  });
  return typeof path === "string" ? path : null;
}

/**
 * Write the whole-store backup to the chosen path (core `run_backup`). Progress arrives separately on
 * the `data-progress` event (`listenDataProgress`). The caller obtains the path first via `pickBackupPath`.
 */
export async function runBackup(path: string): Promise<BackupReportDto> {
  return invoke<BackupReportDto>("run_backup", { path });
}

/** Pick the archive to restore from. Null on cancel, and outside Tauri. */
export async function pickRestoreArchive(): Promise<string | null> {
  if (!inTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    title: t("settings.restoreDialogTitle"),
    multiple: false,
    directory: false,
    filters: [{ name: "Amenbo backup", extensions: [WORLD_ARCHIVE_EXT] }],
  });
  return typeof picked === "string" ? picked : null;
}

/**
 * Restore everything from the chosen archive (core `run_restore` — **destructive**). The caller must
 * put a confirmation dialog in front of it. A restore swaps out all the data, so on success every query
 * is invalidated and the snapshot refetched to rebuild the screen from scratch.
 */
export async function runRestore(path: string): Promise<RestoreReportDto> {
  const report = await invoke<RestoreReportDto>("run_restore", { path });
  invalidateAllQueries();
  void loadSnapshot();
  return report;
}

/** Abort a running backup/restore at the next store boundary — the progress modal's "cancel" (core `cancel_data_op`). */
export async function cancelDataOp(): Promise<void> {
  if (inTauri()) await invoke("cancel_data_op");
}

/**
 * Subscribe to the per-store progress of a backup/restore (the `data-progress` event). Always call the
 * unlisten it returns once the operation finishes. Outside Tauri (browser mock) it is a no-op.
 */
export async function listenDataProgress(cb: (p: DataProgressDto) => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<DataProgressDto>("data-progress", (e) => cb(e.payload));
}

/**
 * Choose where an export is written. An export is a **directory** (`export.json` plus the attachment
 * bytes), so the save dialog asks for a path that **does not exist yet** and core creates the directory
 * there — picking an existing folder would scatter our files among someone else's.
 * Null on cancel, and outside Tauri (browser mock).
 */
export async function pickExportPath(): Promise<string | null> {
  if (!inTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({
    title: t("settings.exportDialogTitle"),
    defaultPath: `amenbo-export-${backupStamp()}`,
  });
  return typeof path === "string" ? path : null;
}

/**
 * Write the full export to the chosen location (core `run_export`): a directory holding `export.json`
 * and the attachment bytes. It is for moving to another tool and carries no secrets (keys, identity).
 * Progress arrives separately on the `data-progress` event (`listenDataProgress`). The caller obtains
 * the path first via `pickExportPath`.
 */
export async function runExport(path: string): Promise<ExportReportDto> {
  return invoke<ExportReportDto>("run_export", { path });
}

/**
 * Backs the "where the data lives" panel in Settings > Data: the real path of the app-data root, and
 * the ids of the project stores that actually exist on this machine. It reflects the multi-store
 * reality without assuming any one OS's layout.
 * Null outside Tauri (browser mock).
 */
export async function fetchStoreLocations(): Promise<StoreLocationsDto | null> {
  if (!inTauri()) return null;
  return invoke<StoreLocationsDto>("store_locations");
}

/**
 * What this build calls itself in the header — `DEV`, or `DEV AMB-T-<id>` for a task's throwaway
 * instance — and null on production, which wears none.
 *
 * Asked **once, at startup**: the channel is stamped in at build time, so the answer cannot change
 * while the process runs. Null in the browser too, where there is no build to ask about.
 */
export async function fetchDevBadge(): Promise<string | null> {
  if (!inTauri()) return null;
  return await invoke<string | null>("dev_badge");
}

/**
 * How this build's CLI is run where the user types it — `amenbo`, `amenbo-dev` on the shared dev
 * build, and the path into the bundle on a macOS preview, whose CLI no installer puts on PATH.
 * Every screen that hands over a command to run asks for it instead of spelling `amenbo`, because a
 * dev window naming the production CLI sends the reader to a command that is not there.
 *
 * **Null means this build has no command a reader can run at all** — a Linux preview, whose CLI is
 * inside an AppImage and exists only while the app is open. A screen that gets it says so rather
 * than naming something. In the browser there is no build to ask, so the production name is
 * answered: it is the one a reader of the web preview would install.
 *
 * Asked **once, at startup**, like the badge: the channel is stamped in at build time.
 */
export async function fetchCliCommandName(): Promise<string | null> {
  if (!inTauri()) return "amenbo";
  return await invoke<string | null>("cli_command_name");
}

/**
 * Open the folder holding this machine's logs, beside the location line in Settings > Data: the one
 * step between "please attach your logs" and a file the user can drag onto an issue. Core rejects the
 * call when there is no folder yet, and the caller shows what it said. A no-op outside Tauri, where
 * there is neither a folder nor a file manager to open it with.
 */
export async function openLogsDir(): Promise<void> {
  if (!inTauri()) return;
  return invoke<void>("open_logs_dir");
}

/** Set or edit the notes (Markdown); the empty string clears them. */
export async function setNotes(id: number, notes: string): Promise<void> {
  if (inTauri()) return invokeAck("task_set_notes", { id, notes });
  mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => (x.id === id ? { ...x, notes } : x)),
  }));
}

/** Edit a task's title. Core refuses an empty title; the caller guards against it as well. */
export async function setTitle(id: number, title: string): Promise<void> {
  if (inTauri()) return invokeAck("task_set_title", { id, title });
  mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => (x.id === id ? { ...x, title } : x)),
  }));
}

/**
 * What an attachment can hang off. Besides the body of a task or a decision record, each individual
 * comment can carry attachments too. Attachments exist only inside Tauri — the mock fixtures have none
 * — so in browser iteration every attachment call below is a no-op.
 */
export type AttachTarget = AttachTargetType;

/**
 * Attach files chosen in the file picker as blobs. They go in by path, which means core streams them —
 * safe even for large files. The picker allows a multiple selection; choosing nothing is a no-op.
 */
export async function pickAndAttach(target: AttachTarget, targetId: number): Promise<void> {
  if (!inTauri()) return;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: true, directory: false });
  if (!picked) return;
  const paths = Array.isArray(picked) ? picked : [picked];
  for (const path of paths) {
    await invokeAck("attachment_add", { targetType: target, targetId, path });
  }
}

/**
 * Attach dropped Files as blobs, by bytes. Drag-and-drop inside the webview cannot give us an OS path
 * (`dragDropEnabled:false`), so we read the file and hand over the bytes. For large files, prefer the
 * picker (`pickAndAttach`).
 */
export async function attachDroppedFiles(target: AttachTarget, targetId: number, files: FileList | File[]): Promise<void> {
  if (!inTauri()) return;
  for (const file of Array.from(files)) {
    const bytes = new Uint8Array(await file.arrayBuffer());
    await invokeAck("attachment_add_bytes", { targetType: target, targetId, filename: file.name, bytes });
  }
}

/** Remove an attachment (the row is hard-deleted). The blob itself survives until nothing references it and the GC reclaims it. */
export async function removeAttachment(id: number, target: AttachTarget, targetId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("attachment_remove", { id, targetType: target, targetId });
}

/** Open a url-mode attachment in the OS's default app. A url is the only kind with anywhere to open: a blob is a file, so it is downloaded (`saveAttachment`). Read-only, so no ack. */
export async function openAttachment(url: string): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("attachment_open", { url });
}

/**
 * Download a blob attachment: the user picks a destination in the OS save dialog and the blob is
 * written there. This is the way an attachment leaves the store as a file the user keeps — opening
 * it externally hands the OS a temp copy, which is not somewhere anyone can find again. Cancelling
 * the dialog is a no-op. Read-only for the store, so no ack.
 */
export async function saveAttachment(blobHash: string, filename: string | null): Promise<void> {
  if (!inTauri()) return;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const dest = await save({ defaultPath: filename ?? undefined });
  if (!dest) return;
  await invoke<void>("attachment_save", { blobHash, dest });
}

/**
 * Record a git commit SHA on a task. The SHA is validated at the ops door — full-length
 * lower-case hex only — so a bad value rejects with a structured error the caller surfaces. Recording
 * a SHA already on the task is a no-op. Commit SHAs exist only inside Tauri, so this is a no-op in
 * browser iteration.
 */
export async function addTaskCommit(taskId: number, sha: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("task_commit_add", { taskId, sha });
}

/** Forget a commit SHA on a task (a hard delete; a SHA not recorded is a no-op). */
export async function removeTaskCommit(taskId: number, sha: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("task_commit_remove", { taskId, sha });
}

/**
 * Record a decision (Proposed, under project_id). Decisions exist only inside Tauri — the mock fixtures
 * have none — so in browser iteration every decision call below is a no-op.
 */
export async function addDecision(projectId: number, title: string, body: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_add", { projectId, title, body });
}

/** Add a comment to a decision record (its own decision_comment table). */
export async function addDecisionComment(id: number, text: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_comment_add", { decisionId: id, text });
}

/** Edit a decision comment in place; its id, position in the thread and attachments all survive. */
export async function editDecisionComment(commentId: number, decisionId: number, text: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_comment_edit", {
    id: commentId,
    decisionId,
    text,
  });
}

/** Retract a decision comment posted by mistake (hard delete, attachments and all). */
export async function removeDecisionComment(commentId: number, decisionId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_comment_remove", {
    id: commentId,
    decisionId,
  });
}

/**
 * Accept a decision (Proposed → Accepted). Passing a `reason` also posts it as one comment — this is the
 * GUI form of `decision accept --reason`: the rationale is a comment, not a field of its own.
 */
export async function acceptDecision(id: number, reason?: string): Promise<void> {
  if (!inTauri()) return;
  const r = reason?.trim();
  if (r) await addDecisionComment(id, r);
  return invokeAck("decision_accept", { id });
}

/** Reject a decision (Proposed → Rejected). Symmetrically with accept, a `reason` is posted as one comment. */
export async function rejectDecision(id: number, reason?: string): Promise<void> {
  if (!inTauri()) return;
  const r = reason?.trim();
  if (r) await addDecisionComment(id, r);
  return invokeAck("decision_reject", { id });
}

/** Put an accepted decision back under discussion (Accepted → Proposed). This is the sanctioned way to make a small correction without polluting the supersede chain: non-destructive, reversible, auditable. */
export async function reopenDecision(id: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_reopen", { id });
}

/** Edit a decision's title/body in place — proposed or accepted alike (`AMB-D-363`); rejected is terminal. */
export async function editDecision(id: number, title: string | null, body: string | null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_edit", { id, title, body });
}

/** Have decision `newId` replace `oldId` (the supersession chain). */
export async function supersedeDecision(newId: number, oldId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_supersede", { newId, oldId });
}

/** Have decision `newId` amend `oldId` in part; `oldId` stays in force. */
export async function amendDecision(newId: number, oldId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_amend", { newId, oldId });
}

/** Record that decision `newId` builds on `oldId`; both stay in force. */
export async function buildsOnDecision(newId: number, oldId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_builds_on", { newId, oldId });
}

/**
 * Remove one edge between two decisions (all three kinds go through here). Name it in the direction it
 * was drawn, new → old: once the pair is fixed there is only one edge, so the kind need not be given.
 * This corrects a miswiring; it does not undo a decision.
 */
export async function unlinkDecisionEdge(decisionId: number, targetDecisionId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_unlink_edge", { decisionId, targetDecisionId });
}

/** Link or unlink a decision and a task. */
export async function setDecisionLink(decisionId: number, taskId: number, link: boolean): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_set_link", { decisionId, taskId, link });
}

/** Promote a task comment into a decision: the comment becomes the body, the task's project becomes the owner, and the decision is linked back to the task. */
export async function promoteComment(commentId: number, title: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_promote", { commentId, title });
}

/**
 * Add a dimension — a classification axis — under project_id, at the end. Dimensions are the only way
 * classification is expressed. They exist only inside Tauri (the mock fixtures have none), so in
 * browser iteration every dimension call below is a no-op.
 */
export async function addDimension(projectId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_add", { projectId, name });
}

/** Rename a dimension. */
export async function renameDimension(id: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_rename", { id, name });
}

/**
 * Rename a dimension's readable key — what names the axis outside Amenbo, where a display name with
 * spaces in it cannot go (`AMB-D-735`). A key is replaced, never cleared: every axis is born with one
 * derived from its id, and the backend refuses a shape it cannot carry out (lower-case ASCII letters,
 * digits and hyphens, opening with a letter) or a key another axis in the project already answers to.
 */
export async function setDimensionSlug(id: number, slug: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_set_slug", { id, slug });
}

/** Update a dimension's description (notes). */
export async function updateDimension(id: number, notes: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_update", { id, notes });
}

/** Toggle whether a dimension's values are ordered. Turning it on is what makes reordering them (moveDimensionValue) work. */
export async function setDimensionOrdered(id: number, ordered: boolean): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_update", { id, ordered });
}

/**
 * Name this dimension the project's time axis, or take the role away (role: time_axis). Once named, its
 * values carry a period (start and end date), and the value whose period covers today is the "current era".
 */
export async function setDimensionTimeAxis(id: number, timeAxis: boolean): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_update", { id, timeAxis });
}

/**
 * Put this dimension on the task card, or take it off again. The answer belongs to the axis and not to
 * this device (`AMB-D-651`), so the toggle moves it for every face and every machine — which is why it
 * goes through the same op as the rest of the axis rather than into a local setting.
 */
export async function setDimensionShowOnCard(id: number, showOnCard: boolean): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_update", { id, showOnCard });
}

/**
 * Make this dimension refuse to be left empty, or let it be left empty again (`AMB-D-734`). It bites at
 * one point only — a task cannot finish its creation while it carries no value here — so raising it
 * leaves every task already through that door alone. Core refuses to raise it on an axis that offers no
 * values, since nobody could answer it; the panel keeps the box off in that case, and the refusal is the
 * backstop.
 */
export async function setDimensionRequired(id: number, required: boolean): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_update", { id, required });
}

/** Delete a dimension; its values and the task assignments to them go with it. */
export async function removeDimension(id: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_rm", { id });
}

/** Add a value (a choice) to a dimension, at the end. */
export async function addDimensionValue(dimensionId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_value_add", { dimensionId, name });
}

/** Rename a dimension value. */
export async function renameDimensionValue(valueId: number, name: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_value_rename", { valueId, name });
}

/** Rename a dimension value's readable key — the value's counterpart of `setDimensionSlug`, unique within its axis. */
export async function setDimensionValueSlug(valueId: number, slug: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_value_set_slug", { valueId, slug });
}

/**
 * Replace the period of a value on the time axis (inclusive, `YYYY-MM-DD`). An empty string or
 * undefined leaves that end open — no `endOn` means "still running". The backend refuses a value that
 * does not belong to a time_axis dimension.
 */
export async function setDimensionValuePeriod(
  valueId: number,
  startOn: string | undefined,
  endOn: string | undefined,
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_value_set_period", {
    valueId,
    startOn,
    endOn,
  });
}

/** Delete a dimension value; the task assignments to it go with it, unless `reassignTo` names another
 * value of the same axis for them to move to — which a required axis demands whenever there are any. */
export async function removeDimensionValue(valueId: number, reassignTo: number | null = null): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_value_rm", { valueId, reassignTo });
}

/** Reorder a dimension value: give the anchor value's id under exactly one of `before` / `after`. Ordered dimensions only. */
export async function moveDimensionValue(
  valueId: number,
  pos: { before?: number; after?: number },
): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_value_move", {
    valueId,
    before: pos.before,
    after: pos.after,
  });
}

/** Assign a dimension value to a task. On a single-select dimension this replaces whatever that task already had on the same axis. */
export async function setTaskDimensionValue(taskId: number, valueId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("task_set_dimension_value", { taskId, valueId });
}

/** Take one dimension-value assignment off a task. */
export async function unsetTaskDimensionValue(taskId: number, valueId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("task_unset_dimension_value", { taskId, valueId });
}

/** The dimension assignments a task currently carries (dimensionId → valueId), for the detail pane's current-value display. */
export async function fetchTaskDimensions(taskId: number): Promise<TaskDimensionAssignmentDto[]> {
  if (!inTauri()) return [];
  return invoke<TaskDimensionAssignmentDto[]>("task_dimensions", { taskId });
}

/** Assign a dimension value to a decision — the decision side of `setTaskDimensionValue`. On a single-select dimension this replaces whatever that decision already had on the same axis. */
export async function setDecisionDimensionValue(decisionId: number, valueId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_set_dimension_value", { decisionId, valueId });
}

/** Take one dimension-value assignment off a decision. */
export async function unsetDecisionDimensionValue(decisionId: number, valueId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("decision_unset_dimension_value", { decisionId, valueId });
}

/** The dimension assignments a decision currently carries (dimensionId → valueId), for the decision pane's current-value display. */
export async function fetchDecisionDimensions(decisionId: number): Promise<DecisionDimensionAssignmentDto[]> {
  if (!inTauri()) return [];
  return invoke<DecisionDimensionAssignmentDto[]>("decision_dimensions", { decisionId });
}

/** All task assignments for one project × dimension (taskId → valueId) in a single call, so the board can group by value. */
export async function fetchProjectDimensionAssignments(
  projectId: number,
  dimensionId: number,
): Promise<DimensionTaskValueDto[]> {
  if (!inTauri()) return [];
  return invoke<DimensionTaskValueDto[]>("project_dimension_assignments", {
    projectId,
    dimensionId,
  });
}

/** Retract a comment posted by mistake (hard delete, attachments and all). */
export async function removeComment(commentId: number, taskId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("comment_remove", { id: commentId, taskId });
}

/** Edit a comment in place; its id, position in the thread and attachments all survive. */
export async function editComment(commentId: number, taskId: number, text: string): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("comment_edit", { id: commentId, taskId, text });
}

export async function addComment(taskId: number, text: string): Promise<void> {
  if (inTauri()) return invokeAck("comment_add", { taskId, text });
  // Core refuses a comment on a task that does not exist (foreign key), so the mock does not stack an orphan row either.
  const t = getSnapshot().tasks.find((x) => x.id === taskId);
  if (!t) return;
  mockMutate((s) => ({
    ...s,
    tasks: s.tasks.map((x) => (x.id === taskId ? { ...x, comments: x.comments + 1 } : x)),
    activity: [
      {
        id: Date.now(), seq: 0, at: new Date().toISOString(), kind: "comment",
        author: me(), target: { type: "task", id: taskId, title: t.title, live: true }, text,
      },
      ...s.activity,
    ],
  }));
}
