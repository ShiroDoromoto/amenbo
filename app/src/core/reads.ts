// THE WIRING SEAM (paged read side).
//
// Views never hold every task in React state; they come through here for **just the window they show**
// (bounded memory). Inside Tauri that means invoking core's `task_page` (indexed SQLite projection +
// LIMIT/OFFSET) and `tasks_by_ids` (hydrating a set of ids); outside Tauri (`npm run dev` in a browser)
// the mock fixtures are filtered with a subset of the same grammar.
//
// Refetching is invalidation-driven, and invalidation is explicit. Our own writes touch only the keys the
// WriteAck names via its scope/affected ids (mutations.applyAck); an external write (`store-changed`) touches
// only the keys reached by folding the rows the **change feed** names into scopes
// (snapshot.watchStore→changes.drainChanges→query.invalidateScopes); read-receipt changes refetch coarsely
// (store.markSeen). That keeps "another process's write shows up in the view" without ever copying the whole
// store, and stops every write from refetching every view.
import { getSnapshot, inTauri, type Decision } from "./snapshot";
import { useQuery } from "./query";
import { loadCommentTasks, loadTriggeredAt } from "./readReceipts";
import { loadInboxArchived } from "./inboxArchive";
import { invoke } from "./ipc";
import { parseRef } from "./idref";
import { currentLang, type Lang } from "./i18n";
import type { TaskCard } from "../mock/types";
import type { ArchivedProjectDto, AttachmentDto, DecisionCommentDto, TaskPageDto, DecisionPageDto, RefTargetDto } from "../bindings/bindings";

// "Today" for the browser mock only. On the Tauri path core's today() resolves due:today, so this is unused.
const TODAY = "2026-06-21";

// The inbox is a small derived view refined on the client. Rather than every task, we pull at most this many
// candidates and narrow them down (my open tasks + unread ones is realistically a small set).
const SUPERSET_CAP = 500;

export interface TaskPageQuery {
  projectId?: number | null;
  filter?: string;
  sort?: string;
  limit?: number | null;
  offset?: number;
}

export interface TaskPage {
  tasks: TaskCard[];
  totalMatched: number;
}

/** Fetch one page of tasks (Tauri: `task_page`; browser: mock fixtures filtered with the same grammar). */
export async function fetchTaskPage(q: TaskPageQuery): Promise<TaskPage> {
  if (inTauri()) {
    const r = await invoke<TaskPageDto>("task_page", {
      projectId: q.projectId ?? null,
      filter: q.filter ?? "",
      sort: q.sort ?? "order",
      limit: q.limit ?? null,
      offset: q.offset ?? 0,
    });
    return { tasks: r.tasks, totalMatched: r.totalMatched };
  }
  return mockTaskPage(q);
}

/** Hydrate a set of ids into TaskCards (Tauri: `tasks_by_ids`; browser: from fixtures). Input order is kept. */
export async function fetchTasksByIds(ids: number[]): Promise<TaskCard[]> {
  if (ids.length === 0) return [];
  if (inTauri()) {
    return invoke<TaskCard[]>("tasks_by_ids", { ids });
  }
  const map = new Map(getSnapshot().tasks.map((t) => [t.id, t]));
  return ids.map((id) => map.get(id)).filter((t): t is TaskCard => !!t);
}

/** Subscribing hook around a paged query (refetches when the query, the page, or the data changes). */
export function useTaskPage(q: TaskPageQuery): TaskPage & { loading: boolean } {
  const { data, loading } = useQuery<TaskPage>(
    ["taskPage", q.projectId ?? null, q.filter ?? "", q.sort ?? "order", q.limit ?? null, q.offset ?? 0],
    () => fetchTaskPage(q),
  );
  return { tasks: data?.tasks ?? [], totalMatched: data?.totalMatched ?? 0, loading };
}

/** One row of the "archived" section at the bottom of the sidebar. */
export type ArchivedProject = ArchivedProjectDto;

/**
 * Fetch the archived projects. They are absent from snapshot's `projects` (= `project_overview` = active only),
 * so they need their own read path, `project_list_archived`. Most-recently-updated first, ties broken by id.
 * The browser iteration loop (`npm run dev`) has no archived projects, hence the empty array.
 */
export async function fetchArchivedProjects(): Promise<ArchivedProject[]> {
  if (inTauri()) return invoke<ArchivedProjectDto[]>("project_list_archived", {});
  return [];
}

/**
 * Subscribing read of the archived projects (the collapsible sidebar section). Archiving, unarchiving, deleting
 * and renaming a project all return a WriteAck scoped to `tasks` (commands.rs), so `applyAck` invalidates this
 * key and it refetches.
 */
export function useArchivedProjects(): ArchivedProject[] {
  const { data } = useQuery<ArchivedProject[]>(["archivedProjects"], fetchArchivedProjects);
  return data ?? [];
}

/** Fetch a project's decision records (without leaning on snapshot.decisions). */
export async function fetchDecisionPage(projectId: number): Promise<Decision[]> {
  if (inTauri()) {
    const r = await invoke<DecisionPageDto>("decision_page", {
      projectId,
      limit: null,
      offset: 0,
    });
    return r.decisions;
  }
  // Browser fallback: the fixtures carry no decisions (snapshot.decisions is empty).
  return getSnapshot().decisions.filter((d) => d.project?.id === projectId);
}

/** Subscribing read of a project's decision records, used by the decisions tab. Status filtering, search and
 * sorting are layered on top client-side because the row count is bounded (same policy as the board's tasks). */
export function useDecisionPage(projectId: number): Decision[] {
  const { data } = useQuery<Decision[]>(["decisions", projectId], () => fetchDecisionPage(projectId));
  return data ?? [];
}

/** Hydrate ids into decisions (Tauri: `decisions_by_ids`; browser: from the snapshot). Input order is kept. */
export async function fetchDecisionsByIds(ids: number[]): Promise<Decision[]> {
  if (ids.length === 0) return [];
  if (inTauri()) {
    return invoke<Decision[]>("decisions_by_ids", { ids });
  }
  const map = new Map(getSnapshot().decisions.map((d) => [d.id, d]));
  return ids.map((id) => map.get(id)).filter((d): d is Decision => !!d);
}

/** Subscribing read of a single decision (so the detail pane need not lean on snapshot.decisions). */
export function useDecision(id: number | null): Decision | undefined {
  const { data } = useQuery<Decision | undefined>(
    ["decision", id],
    () => (id ? fetchDecisionsByIds([id]).then((r) => r[0]) : Promise.resolve(undefined)),
  );
  return data;
}

/** One durable comment on a decision record (generated DTO). */
export type DecisionComment = DecisionCommentDto;

/** Fetch a decision's live comments, oldest first (Tauri: `decision_comments`; browser mock: empty). */
export async function fetchDecisionComments(decisionId: number): Promise<DecisionComment[]> {
  if (inTauri()) return invoke<DecisionCommentDto[]>("decision_comments", { decisionId });
  return [];
}

/**
 * Subscribing read of a decision's comments (the thread in the decision detail pane). Posting one returns a
 * WriteAck (decisions scope + the decision id), which invalidates `["decisionComments", id]` and triggers the
 * refetch (mutations.applyAck).
 */
export function useDecisionComments(decisionId: number | null): DecisionComment[] {
  const { data } = useQuery<DecisionComment[]>(
    ["decisionComments", decisionId],
    () => (decisionId ? fetchDecisionComments(decisionId) : Promise.resolve([])),
  );
  return data ?? [];
}

/** One option (flag). `help` is its description; `required` marks a mandatory flag. */
export type CommandFlag = { name: string; help: string; required?: boolean };
/** One positional argument. */
export type CommandArg = { name: string; help: string; required?: boolean };
/** The spec of one command (from `agent --json`, display only). Options = flags + args; samples = examples. */
export type CommandSpec = {
  name: string;
  summary: string;
  flags?: CommandFlag[];
  args?: CommandArg[];
  examples?: string[];
};
/** Command names grouped by capability. The details live in commands[] and are resolved by name. */
export type CommandCapability = { capability: string; commands: string[] };
/** The slice of the agent spec the GUI consumes (the command list plus the capability grouping). */
export type AgentSpec = { commands: CommandSpec[]; capabilities: CommandCapability[] };

const EMPTY_SPEC: AgentSpec = { commands: [], capabilities: [] };

/**
 * Fetch the `amenbo agent --json` spec (source of truth: core::agent). Tauri goes through the `agent_spec`
 * command; the browser (npm run dev) gets nothing, because the command reference and ⌘K are surfaces over real
 * Tauri data (the GUI never shells out to the CLI). The English spec of record is immutable — it is what AI
 * reads — so the GUI passes a locale (config.language) and core swaps only the prose for a translation right
 * before display (`build_localized`); an absent or unknown locale passes through as English.
 */
export async function fetchAgentSpec(locale: Lang = currentLang()): Promise<AgentSpec> {
  if (inTauri()) return invoke<AgentSpec>("agent_spec", { locale });
  return EMPTY_SPEC;
}

/** Read the agent spec for the command reference and ⌘K. It never changes while running, so fetch once (no subscription). */
export function useAgentSpec(): { spec: AgentSpec; loading: boolean } {
  // The language is part of the key, so switching languages refetches the spec in that language.
  const lang = currentLang();
  const { data, loading } = useQuery<AgentSpec>(["agent-spec", lang], () => fetchAgentSpec(lang));
  return { spec: data ?? EMPTY_SPEC, loading };
}

/** Index from command name to capability name, for the grouped display in the reference and ⌘K. First one wins, so a command lands in exactly one group. */
export function capabilityByCommand(spec: AgentSpec): Map<string, string> {
  const m = new Map<string, string>();
  for (const cap of spec.capabilities) for (const n of cap.commands) if (!m.has(n)) m.set(n, cap.capability);
  return m;
}

/** Subscribing read of a single task (fetched on its own via `tasks_by_ids`, not taken from snapshot.tasks). */
export function useTask(id: number | null): TaskCard | undefined {
  const { data } = useQuery<TaskCard | undefined>(
    ["task", id],
    () => (id !== null ? fetchTasksByIds([id]).then((r) => r[0]) : Promise.resolve(undefined)),
  );
  return data;
}

/** What an in-body link resolves to (generated DTO). `kind` decides whether the task or decision pane opens. */
export type RefTarget = RefTargetDto;

/**
 * Resolve one conversational reference found in a body (`#NNN` / `T-NN` / `D-NN`) to the id of a real entity.
 * Under Tauri this goes to core's `resolve_ref` (which reuses `resolve_any_ref`); in the browser mock it matches
 * the number against the snapshot. Numbers are **globally unique on this machine**, so no project context is
 * needed and `#NNN` names exactly one entity. Unknown or ambiguous input yields null.
 */
export async function resolveRef(input: string): Promise<RefTarget | null> {
  if (inTauri()) return invoke<RefTarget | null>("resolve_ref", { input });
  return mockResolveRef(input);
}

/** The browser-mock reference resolver: match on fixture id (the id *is* the conversational number), covering the smallest useful subset of core's branches. */
function mockResolveRef(input: string): RefTarget | null {
  const s = input.trim();
  const { tasks, decisions } = getSnapshot();
  const findTask = (n: number): RefTarget | null => {
    const t = tasks.find((x) => x.id === n);
    return t ? { kind: "task", id: t.id } : null;
  };
  const findDecision = (n: number): RefTarget | null => {
    const d = decisions.find((x) => x.id === n);
    return d ? { kind: "decision", id: d.id } : null;
  };
  const typed = parseRef(s);
  if (typed) return typed.space === "task" ? findTask(typed.num) : findDecision(typed.num);
  const num = /^(\d+)$/.exec(s);
  if (num) return findTask(Number(num[1])) ?? findDecision(Number(num[1]));
  return null;
}

/** One attachment (generated DTO). The viewer branches on `mime` and builds the stream URL from `blobHash`. */
export type Attachment = AttachmentDto;

/**
 * What an attachment can hang off. Besides a body (task/decision), an individual comment can carry attachments
 * too (task_comment/decision_comment). Matches the values core's `AttachmentTarget::parse` accepts.
 */
export type AttachTargetType = "task" | "decision" | "task_comment" | "decision_comment";

/** Fetch a target's live attachments in attach order (Tauri: `attachments_for`; browser: empty). */
export async function fetchAttachments(targetType: AttachTargetType, targetId: number): Promise<Attachment[]> {
  if (inTauri()) return invoke<AttachmentDto[]>("attachments_for", { targetType, targetId });
  return [];
}

/** Subscribing read of the attachments (add/remove return a WriteAck that invalidates the target id — applyAck). */
export function useAttachments(targetType: AttachTargetType, targetId: number | null): Attachment[] {
  const { data } = useQuery<Attachment[]>(
    ["attachments", targetType, targetId],
    () => (targetId !== null ? fetchAttachments(targetType, targetId) : Promise.resolve([])),
  );
  return data ?? [];
}

/** Pull a bounded candidate set (at most SUPERSET_CAP), the raw material the inbox is refined from. */
async function fetchSuperset(query: { filter: string; sort: string }): Promise<TaskCard[]> {
  const page = await fetchTaskPage({ ...query, limit: SUPERSET_CAP });
  return dedupById(page.tasks);
}

/**
 * One page of a smart view. The inbox is the only smart view there is — browsing completed work gets no view of
 * its own, it is a project's list view plus a `status:done` filter. The inbox does not fit a single query (an OR
 * of conditions, the unread-comment set, a state machine), so the bounded candidate set is refined on the client
 * and only then cut into a `page` window — no full copy of the store ever lands in JS.
 */
export function useSmartView(viewId: string, page: number, pageSize: number): { tasks: TaskCard[]; total: number } {
  const me = getSnapshot().meUserId;
  const { data } = useQuery<{ tasks: TaskCard[]; total: number }>(
    ["smartView", viewId, page, pageSize, me],
    () => fetchSmartView(viewId, page, pageSize, me),
  );
  return data ?? { tasks: [], total: 0 };
}

/** Fetch one page of a smart view (the plain async that useQuery calls behind the hook). */
async function fetchSmartView(viewId: string, page: number, pageSize: number, me: string): Promise<{ tasks: TaskCard[]; total: number }> {
  if (viewId === "inbox") {
    const full = await fetchInboxTasks(me);
    return { tasks: full.slice(page * pageSize, (page + 1) * pageSize), total: full.length };
  }
  if (viewId === "inbox-archived") {
    const full = await fetchInboxArchivedTasks();
    return { tasks: full.slice(page * pageSize, (page + 1) * pageSize), total: full.length };
  }
  return { tasks: [], total: 0 };
}

/**
 * The inbox's archived tab: the inbox items archived on *this* machine. Hydrates the ids in the device-local
 * `inbox_archive` table and orders them most-recently-archived first (the table appends, so reverse it). Any item
 * can be unarchived back into the inbox. The tasks themselves are untouched, so ids that were deleted or moved
 * simply fall out during hydration.
 */
async function fetchInboxArchivedTasks(): Promise<TaskCard[]> {
  const archivedIds = await loadInboxArchived();
  return fetchTasksByIds([...archivedIds].reverse());
}

/**
 * The inbox is "what I have to look at or act on". It is the union of a state-based set C (assigned to me on my
 * human facet, newly assigned by someone else, not yet started by me) and a comment-based set D (tasks carrying
 * a comment addressed to me). Membership does not depend on having read anything — clicking an item does not
 * evict it; archiving is the only way out. Read state feeds each item's `unread` display flag and nothing else.
 */
async function fetchInboxTasks(me: string): Promise<TaskCard[]> {
  const [cand, commentTasks, archivedIds] = await Promise.all([
    // C candidates: assigned to me (assignee:me = the human facet) and not done.
    fetchSuperset({ filter: "assignee:me done:false", sort: "order" }),
    loadCommentTasks(),
    loadInboxArchived(),
  ]);
  const archived = new Set(archivedIds);
  const unreadById = new Map(commentTasks.map((c) => [c.id, c.unread]));
  // D: tasks with a comment addressed to me (they stay once read, and leave when done).
  const dTasks = (await fetchTasksByIds(commentTasks.map((c) => c.id))).filter((t) => t.status !== "done");
  const cTasks = cand.filter((t) => {
    const mineHuman = t.assignee?.userId === me && t.assignee?.kind === "human";
    if (!mineHuman) return false;
    return t.status === "todo" && !!t.createdBy && t.createdBy.userId !== me;
  });
  // The archive set evicts unconditionally — the one and only exit, shared by C and D.
  const inbox = dedupById([...dTasks, ...cTasks]).filter((t) => !archived.has(t.id));
  // Ask core when each item last became inbox-worthy (its C/D trigger) and carry it on the item, for display and sorting.
  const triggers = await loadTriggeredAt(inbox.map((t) => t.id));
  const withTrigger = inbox.map((t) => ({ ...t, triggeredAt: triggers[t.id] ?? null, unread: unreadById.get(t.id) ?? false }));
  // Newest first (triggeredAt descending); unknown (null) sinks to the bottom. RFC3339 UTC, so a string compare is a time compare.
  return withTrigger.sort((a, b) => (b.triggeredAt ?? "").localeCompare(a.triggeredAt ?? ""));
}

/** One inbox item, reduced to what arrival detection needs: its id and whether it is unread. */
export interface InboxItemBrief {
  id: number;
  unread: boolean;
}

/**
 * The current inbox (C ∪ D) as `{ id, unread }`, in the view's own order. Exported so the nav badge count and
 * arrival detection (`mailbox.ts`) can consult it without the view being open: the badge counts the whole set,
 * while the notification fires only for the unread ones (`unread` is the per-item flag `fetchInboxTasks` already
 * computes). Empty until `me` is known (i.e. before the snapshot loads).
 */
export async function loadInboxItems(): Promise<InboxItemBrief[]> {
  const me = getSnapshot().meUserId;
  if (!me) return [];
  return (await fetchInboxTasks(me)).map((t) => ({ id: t.id, unread: t.unread ?? false }));
}

function dedupById(tasks: TaskCard[]): TaskCard[] {
  const seen = new Set<number>();
  const out: TaskCard[] = [];
  for (const t of tasks) {
    if (seen.has(t.id)) continue;
    seen.add(t.id);
    out.push(t);
  }
  return out;
}

/**
 * The browser-mock fallback (outside Tauri, i.e. iterating on the frontend alone). Filters the fixtures with a
 * subset of `task_page`'s grammar, evaluating only the tokens the views actually emit (status/done/assignee/due).
 * In the read-model, `text:` searches title + notes + comment bodies (query.rs); the mock has no comment bodies,
 * so it settles for title + notes. An assignee User token matches an exact id (case-insensitively) or a name — never
 * an id prefix, because ids *are* the conversational numbers and `12` would otherwise swallow `120`.
 */
function mockMatches(t: TaskCard, q: TaskPageQuery, me: string): boolean {
  if (q.projectId && t.projectId !== q.projectId) return false;
  for (const token of (q.filter ?? "").split(/\s+/).filter(Boolean)) {
    const [key, value] = token.split(":");
    switch (key) {
      case "status": if (t.status !== value) return false; break;
      case "done": if ((t.status === "done") !== (value === "true")) return false; break;
      case "due": if (value === "today" && t.due !== TODAY) return false; break;
      case "text": {
        const v = value.toLowerCase();
        if (!t.title.toLowerCase().includes(v) && !(t.notes ?? "").toLowerCase().includes(v)) return false;
        break;
      }
      case "assignee":
        if (value === "none" && t.assignee) return false;
        else if (value === "me" && !(t.assignee?.userId === me && t.assignee?.kind !== "ai")) return false;
        else if (value === "me-ai" && !(t.assignee?.userId === me && t.assignee?.kind === "ai")) return false;
        else if (value && !["none", "me", "me-ai"].includes(value)) {
          const v = value.toUpperCase();
          const byId = t.assignee?.userId?.toUpperCase() === v;
          const byName = t.assignee?.name?.toLowerCase() === value.toLowerCase();
          if (!byId && !byName) return false;
        }
        break;
      default: break; // Unsupported tokens are ignored — the mock is an approximation for iterating.
    }
  }
  return true;
}

function mockSort(tasks: TaskCard[], sort: string): TaskCard[] {
  const desc = sort.startsWith("-");
  const key = sort.replace(/^-/, "");
  const cmp = (a: TaskCard, b: TaskCard): number => {
    switch (key) {
      case "completed": {
        const av = a.completedAt ?? "", bv = b.completedAt ?? "";
        if (av === bv) return 0;
        if (!av) return 1;     // None sinks to the bottom (ascending)
        if (!bv) return -1;
        return av < bv ? -1 : 1;
      }
      case "title": return a.title.localeCompare(b.title);
      default: return 0;       // order/created keep the fixture order (stable in the mock).
    }
  };
  const out = [...tasks].sort(cmp);
  return desc ? out.reverse() : out;
}

function mockTaskPage(q: TaskPageQuery): TaskPage {
  const me = getSnapshot().meUserId;
  const matched = mockSort(getSnapshot().tasks.filter((t) => mockMatches(t, q, me)), q.sort ?? "order");
  const offset = q.offset ?? 0;
  const limit = q.limit ?? matched.length;
  return { tasks: matched.slice(offset, offset + limit), totalMatched: matched.length };
}
