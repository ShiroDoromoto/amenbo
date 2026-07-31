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
import { t, tf, type CmdError, type CmdErrorPart } from "./i18n";
import { isClosed } from "./status";
import type { ActivityItem, Facet, Priority, Status, TaskCard } from "../mock/types";
import type { ActivityTargetDto, AgentHookNoticeDto, AgentHookOfferDto, BoundFolderDto, EventDto, DimensionTaskValueDto, DoctorFixDto, DoctorIssueDto, DoctorReportDto, HookNoticeDto, HookOfferDto, PointerRepairDto, ProjectDto, ProjectSettingsDto, ResyncReportDto, StaleBlockDto, StoreLocationsDto, TaskDimensionAssignmentDto, BackupReportDto, ExportReportDto, DataProgressDto, RestoreReportDto } from "../bindings/bindings";
import { taskRef } from "./idref";

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
  return parts;
}

/**
 * Rebuild the dependents when one blocker goes away (completed or deleted). In core, `blocked_by` is
 * derived as "blockers not yet done" and `ready` as "no unfinished blocker, no unsettled grounding
 * decision, and the declared start day arrived" (core's `reserve_blockers`, whose emptiness *is*
 * `ready`); the mock runs the same derivation here. Clearing a blocker cannot clear the other two, so
 * they are re-read rather than assumed away.
 * **The dependency edges themselves are not in the mock fixtures**, though — the snapshot only carries
 * the already-derived `blockedBy` — so reopening a task (done → todo) **cannot put its blockers back**.
 * That is a face we chose not to act out. To see dependencies re-form during browser iteration, edit
 * the fixtures.
 */
function unblock(tasks: TaskCard[], blockerId: number): TaskCard[] {
  return tasks.map((x) => {
    if (!x.blockedBy?.some((b) => b.id === blockerId)) return x;
    const blockedBy = x.blockedBy.filter((b) => b.id !== blockerId);
    const ready =
      blockedBy.length === 0 && x.blockedByDecisions.length === 0 && x.notStartedUntil == null;
    return { ...x, blockedBy, ready };
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
export async function addTask(projectId: number | null, title: string, notes?: string): Promise<number | null> {
  if (inTauri()) {
    // Pass a project and core places the task there so it lands on that board; its
    // classification (dimension values) is assigned afterwards. With no project it is an inbox
    // (unfiled) task. task_add's WriteAck carries the new task among its affected ids (commands.rs),
    // so we apply the ack and lift the id out of it.
    const ack = await invoke<WriteAck>("task_add", { projectId, title, notes: notes ?? null });
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
      due: null, comments: 0, createdBy: me(),
      ref: taskRef(id), projectId, completedAt: null,
      ready: true, blockedBy: [], placement: null, linkedDecisions: [], blockedByDecisions: [], notStartedUntil: null,
    };
    return { ...s, tasks: [...s.tasks, task], activity: [sysItem(id, title, { kind: "task.created" }), ...s.activity] };
  });
  return id;
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
 * Create a project (the equivalent of core/CLI `init`), called from the input step of the creation
 * screen. `name` is required — the front end guarantees it is non-empty and core defensively fills in
 * a default name anyway. Passing `dir` **binds that folder to the new project** (an internal `init`:
 * it drops a `.amenbo` pointer and the AI guide into the folder); `dir=null` creates a project by name
 * only. Afterwards it **awaits** the `loadSnapshot` refresh and returns the project's id, so the caller
 * can navigate straight to its board; null if that cannot be resolved. Outside Tauri (browser
 * iteration) `dir` is ignored and the project is faked into the cache.
 */
export async function createProject(name: string, dir: string | null): Promise<number | null> {
  if (inTauri()) {
    const before = new Set(getSnapshot().projects.map((p) => p.id));
    const ack = dir
      ? await invoke<WriteAck>("project_add_folder", { dir, name })
      : await invoke<WriteAck>("project_add", { name });
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
 * amenbo-managed tree, Rust refuses with `binding_nested_tree`. Outside Tauri this is a no-op.
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
 * The one question about starting this project's AI on `amenbo agent` at session start, or null when there
 * is none (core's `harness::reconcile`, over the project's bound folders taken together). `canAsk` is the
 * one-question-at-a-time rule: with the lint's modal already up this run it goes false, and the probe only
 * adopts a wiring already on disk — nothing is asked and nothing about the question is recorded, so it
 * comes round the next time the project is opened.
 *
 * Called when a project is opened (`AMB-D-459`), and it is a command rather than a read for
 * `fetchHookOffer`'s reason: the same call is what adopts a project somebody wired by hand. Outside Tauri
 * there is never a question.
 */
export async function fetchAgentHookOffer(
  projectId: number,
  canAsk: boolean,
): Promise<AgentHookOfferDto | null> {
  if (!inTauri()) return null;
  return await invoke<AgentHookOfferDto | null>("agent_hook_offer", { projectId, canAsk });
}

/**
 * The bound folders whose AI is not started on amenbo (core's `harness::setup_notice`) — the standing
 * report behind the banner, carrying each unwired tool's request so the banner can show it and the copy
 * button has it in hand.
 *
 * Called after the modal has had its turn, so a folder just adopted or just answered is read in the state
 * that left it. Outside Tauri this is an empty array.
 */
export async function fetchAgentHookNotices(): Promise<AgentHookNoticeDto[]> {
  if (!inTauri()) return [];
  return await invoke<AgentHookNoticeDto[]>("agent_hook_notices");
}

/**
 * Record what this **project** answered about starting its AI on amenbo. The project comes from the offer:
 * the answer changes with the place, so unlike the lint's it is not the device's.
 *
 * **Call it only when there is an answer** — a dismissed modal calls nothing and leaves the project
 * unanswered, which is the same third value `answerHookOffer` has no room for. A yes wires nothing: amenbo
 * writes no provider settings file, so what a yes buys is the text, and the banner keeps reporting until
 * the paste lands.
 */
export async function answerAgentHookOffer(projectId: number, yes: boolean): Promise<void> {
  if (!inTauri()) return;
  await invoke("agent_hook_answer", { projectId, yes });
}

/**
 * What this project answered about starting its AI on amenbo — true, false, or null where it has never
 * been asked. The third value is the one the project settings screen exists to show: a refusal reads
 * the same as an unanswered project everywhere else, because both are silent.
 *
 * It says what was answered and nothing about the wiring, which is read from the folder every time
 * (`fetchAgentHookNotices`). Outside Tauri there is no record to read.
 */
export async function fetchAgentHookConsent(projectId: number): Promise<boolean | null> {
  if (!inTauri()) return null;
  return await invoke<boolean | null>("agent_hook_consent", { projectId });
}

/**
 * Forget this project's answer, putting it back to never having been asked, so the question is put again
 * the next time the project is opened. This is the only way back from a no.
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
  if (!inTauri()) return { reclaimedBlobs: 0, freedBytes: 0, forgottenBindings: 0 };
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
 * Open the native folder picker and return the absolute path chosen — this backs the folder field on
 * the creation screen. Null if the dialog is cancelled. **It does not read the folder's contents**: it
 * returns a path string, and the actual binding (placing the `.amenbo` pointer) is done on the Rust
 * side by `createProject(name, dir)`. Outside Tauri (browser) nothing touches the filesystem, so this
 * is a no-op returning null.
 */
export async function pickFolder(): Promise<string | null> {
  if (!inTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false });
  return typeof dir === "string" ? dir : null; // anything but a string means the dialog was cancelled
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
 * Save the first-run setup: apply the language and display names (all optional) and raise
 * `config.onboarded=true` so the flow never shows again. Skipping is calling it with nulls, which
 * raises the flag alone.
 * Outside Tauri (browser) only onboarded is raised, in the cache, for front-end-only iteration.
 */
export async function saveOnboarding(
  language: string | null,
  humanName: string | null,
  aiName: string | null,
): Promise<void> {
  if (inTauri()) return invokeAck("onboarding_save", { language, humanName, aiName });
  mockMutate((s) => ({
    ...s,
    onboarded: true,
    language: language ?? s.language,
    roster: s.roster.map((a) =>
      a.kind === "human" && humanName?.trim() ? { ...a, name: humanName.trim() }
      : a.kind === "ai" && aiName?.trim() ? { ...a, name: aiName.trim() }
      : a,
    ),
  }));
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
    filters: [{ name: "amenbo backup", extensions: [WORLD_ARCHIVE_EXT] }],
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
    filters: [{ name: "amenbo backup", extensions: [WORLD_ARCHIVE_EXT] }],
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
 * What this build's CLI is called where the user types it — `amenbo`, or `amenbo-dev` on a dev
 * build. Every screen that hands over a command to run asks for it instead of spelling `amenbo`,
 * because a dev window naming the production CLI sends the reader to a command that is not there.
 *
 * Asked **once, at startup**, like the badge: the channel is stamped in at build time. Null in the
 * browser, where there is no build to ask — the caller keeps showing the production name, which is
 * the one a reader of the web preview would install.
 */
export async function fetchCliCommandName(): Promise<string | null> {
  if (!inTauri()) return null;
  return await invoke<string>("cli_command_name");
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

/** Delete a dimension value; the task assignments to it go with it. */
export async function removeDimensionValue(valueId: number): Promise<void> {
  if (!inTauri()) return;
  return invokeAck("dimension_value_rm", { valueId });
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
