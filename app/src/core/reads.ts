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
import { useRef } from "react";
import { getSnapshot, inTauri, type Decision } from "./snapshot";
import { useQuery } from "./query";
import { loadCommentTasks, loadTriggeredAt, loadReadReceipts } from "./readReceipts";
import { loadInboxArchived } from "./inboxArchive";
import { invoke } from "./ipc";
import { decisionRef, parseRef, taskRef } from "./idref";
import { currentLang, type Lang } from "./i18n";
import { isClosed } from "./status";
import type { TaskCard } from "../mock/types";
import type { ArchivedProjectDto, AttachmentDto, DecisionCommentDto, SearchHitDto, SearchResultDto, TaskCommitDto, TaskPageDto, DecisionPageDto, RefTargetDto } from "../bindings/bindings";

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

/** Subscribing read of a project's decision records, used by the decisions tab. Status filtering and sorting
 * are layered on top client-side because the row count is bounded (same policy as the board's tasks); the
 * text search is not — see {@link useDecisionSearchIds}. */
export function useDecisionPage(projectId: number): Decision[] {
  const { data } = useQuery<Decision[]>(["decisions", projectId], () => fetchDecisionPage(projectId));
  return data ?? [];
}

/**
 * The ids of a project's decisions matching `text` — the search, run by core rather than over the page.
 *
 * The page payload carries a decision's title and body but not its comment thread, so a client-side
 * substring match can only ever reach two of the three places the words reach, and the CLI answers a search
 * the GUI cannot. Loading every thread to close that gap is the opposite of what a bounded page is for, so
 * the query goes to core instead and comes back as ids the screen narrows what it already holds by.
 *
 * `hits === null` means "no search" (an empty box), which is not the same as "matched nothing" — the caller
 * must not confuse an unasked question with an empty answer.
 *
 * `error` is handed back rather than swallowed. A search that could not run leaves `hits` null, and null is
 * "narrow by nothing", so the screen shows **every** decision — a refusal wearing the face of a word that
 * matched everything. It has to be said out loud, or the next failure here is as quiet as the first was.
 */
export function useDecisionSearchIds(
  projectId: number,
  text: string,
): { hits: Set<number> | null; error: unknown } {
  return useSearchIds("decisionSearch", projectId, text, fetchDecisionSearchIds);
}

/**
 * The ids of a project's tasks matching `text` — the board's search, and the task twin of
 * {@link useDecisionSearchIds}.
 *
 * The board holds a page of the project's tasks, but a card carries no comment body (only the count),
 * no label and no attachment name, so a client-side substring match reaches two of the five faces the word
 * index carries. The typed text therefore goes to `task_search` — the same match the read-model runs — and
 * comes back as the ids to narrow what the screen already holds by. It goes as a term rather than through
 * the page's filter expression: the grammar carries no words at all, and could not carry a phrase if it did
 * — it splits on whitespace, so everything after the first word would be dropped (`AMB-D-449`).
 *
 * The three states, and the error, are exactly {@link useDecisionSearchIds}'s — see there.
 */
export function useTaskSearchIds(
  projectId: number,
  text: string,
): { hits: Set<number> | null; error: unknown } {
  return useSearchIds("taskSearch", projectId, text, fetchTaskSearchIds);
}

/** The one implementation behind both id-narrowing searches. `keyPrefix` keeps the two query caches apart. */
function useSearchIds(
  keyPrefix: string,
  projectId: number,
  text: string,
  fetchIds: (projectId: number, text: string) => Promise<number[] | null>,
): { hits: Set<number> | null; error: unknown } {
  const q = text.trim();
  const { data, error } = useQuery<number[] | null>(
    [keyPrefix, projectId, q],
    () => fetchIds(projectId, q),
  );
  // Hold the last answer while the next one is in flight. Every keystroke is a new query key, so `data` is
  // undefined for a render — and reading that as "no search" makes the list flash back to every row
  // between characters. An empty result is an answer and is kept as one; only "not back yet" falls through.
  const held = useRef<Set<number> | null>(null);
  if (q === "") held.current = null;
  else if (data) held.current = new Set(data);
  return { hits: held.current, error: q === "" ? undefined : error };
}

/** The fetch behind {@link useTaskSearchIds}. `null` for an empty query — nothing to ask. */
async function fetchTaskSearchIds(projectId: number, text: string): Promise<number[] | null> {
  if (text === "") return null;
  if (inTauri()) return invoke<number[]>("task_search", { projectId, text });
  // Browser fallback. The fixtures carry no comment bodies, no labels and no attachments, so title and
  // notes *are* the whole of what could match here — the mock's shape, not a second definition of the
  // search. Several words AND, as they do in core.
  const terms = text.toLowerCase().split(/\s+/).filter(Boolean);
  return getSnapshot()
    .tasks.filter((t) => t.projectId === projectId)
    .filter((t) => {
      const hay = `${t.title}\n${t.notes ?? ""}`.toLowerCase();
      return terms.every((term) => hay.includes(term));
    })
    .map((t) => t.id);
}

/** The fetch behind {@link useDecisionSearchIds}. `null` for an empty query — nothing to ask. */
async function fetchDecisionSearchIds(projectId: number, text: string): Promise<number[] | null> {
  if (text === "") return null;
  if (inTauri()) return invoke<number[]>("decision_search", { projectId, text });
  // Browser fallback. The fixtures carry no decisions and no decision comments at all, so title and body
  // *are* the whole of what could match here — this is the mock's shape, not a second definition of the
  // search.
  const needle = text.toLowerCase();
  return getSnapshot()
    .decisions.filter((d) => d.project?.id === projectId)
    .filter((d) => d.title.toLowerCase().includes(needle) || d.body.toLowerCase().includes(needle))
    .map((d) => d.id);
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

/** One place the words are written (generated DTO). */
export type SearchHit = SearchHitDto;
/** Which face a hit landed on — a title, a body, a comment, an axis label, an attachment's name. */
export type SearchFace = SearchHitDto["face"];
/** Which record the words are on — one of the two axes (`AMB-D-562`). `null` keeps both sides. */
export type SearchKind = "task" | "decision";

/** How many hits one page of the search screen holds. Core's own default is the same number. */
export const SEARCH_PAGE = 20;

export interface SearchQuery {
  /** The words as typed. Empty means nothing was asked. */
  text: string;
  kind: SearchKind | null;
  /** Which face of the record the words are on — the other axis. `null` keeps every face. */
  face: SearchFace | null;
  /**
   * The grammar of the side `kind` names — `task list`'s for a task, `decision list`'s for a decision
   * (`AMB-D-563`). Empty means no narrowing; a narrowing with no `kind` beside it is refused, there
   * being no vocabulary to read it in.
   */
  filter: string;
  /**
   * The one project to look in, held apart from `filter` rather than written into it (`AMB-D-564`):
   * a project is an axis both sides carry, so naming one inside the expression would take the
   * decisions out of the answer as a side effect. `null` is every project.
   */
  projectId: number | null;
  offset: number;
}

export interface SearchAnswer {
  hits: SearchHit[];
  totalMatched: number;
}

/**
 * Run the cross-cutting search (`AMB-D-449`) — every place the words are written, hit by hit, across
 * tasks, decisions and the comments on either.
 *
 * `null` for an empty box: an unasked question is not an empty answer, and the screen says so differently.
 */
export async function fetchSearch(q: SearchQuery): Promise<SearchAnswer | null> {
  const text = q.text.trim();
  if (text === "") return null;
  if (inTauri()) {
    const r = await invoke<SearchResultDto>("search", {
      text,
      kind: q.kind,
      face: q.face,
      filter: q.filter.trim() || null,
      projectId: q.projectId,
      limit: SEARCH_PAGE,
      offset: q.offset,
    });
    return { hits: r.hits, totalMatched: r.totalMatched };
  }
  return mockSearch(text, q);
}

/**
 * Subscribing read of one page of hits. The error is handed back rather than swallowed: a search that
 * could not run — an unparsable `filter`, above all — must not read as a word that matched nothing.
 */
export function useSearch(q: SearchQuery): { answer: SearchAnswer | null; loading: boolean; error: unknown } {
  const { data, loading, error } = useQuery<SearchAnswer | null>(
    ["search", q.text.trim(), q.kind, q.face, q.filter.trim(), q.projectId, q.offset],
    () => fetchSearch(q),
  );
  return { answer: data ?? null, loading, error };
}

/**
 * The browser fallback (`npm run dev`). The fixtures carry no comments, no dimension labels and no
 * attachments, so title and body *are* the whole of what could match here — the mock's shape, not a
 * second definition of the search. Nothing in a fixture carries the instant a hit would report, so the
 * row shows no time rather than an invented one.
 *
 * The standing is filled from the same fixture the hit came off, down to what a fixture actually holds:
 * the state and the priority, and no placements, because nothing here is filed on an axis. An empty list
 * is what that is — the absent standing means something else entirely (a record that stopped being
 * readable), and handing one back here would draw a row the Tauri path never draws.
 */
function mockSearch(text: string, q: SearchQuery): SearchAnswer {
  const needles = text.toLowerCase().split(/\s+/).filter(Boolean);
  const has = (s: string) => needles.every((n) => s.toLowerCase().includes(n));
  const hits: SearchHit[] = [];
  const push = (
    kind: "task" | "decision",
    face: SearchFace,
    ref: string,
    title: string,
    body: string,
    standing: SearchHit["standing"],
  ) => {
    if (q.face !== null && q.face !== face) return;
    const source = face === "title" ? title : body;
    if (!has(source)) return;
    const at = source.toLowerCase().indexOf(needles[0] ?? "");
    const snippet = source.slice(Math.max(0, at - 30), at + 90);
    hits.push({
      face,
      kind,
      ref,
      title,
      at: "",
      snippet,
      // Drawn from the same rule this mock already picked the row by — a lowercase substring, with none
      // of the core's folding. That keeps the browser's approximation one thing end to end rather than a
      // highlight disagreeing with the rows around it; the folded answer is the core's, and under Tauri
      // it is the core that answers.
      matches: mockRanges(snippet, needles),
      standing,
    });
  };
  // The scope reaches both sides alike, which is the whole reason it is an argument and not a key of
  // the narrowing expression (`AMB-D-564`).
  const inScope = (projectId: number | null | undefined) =>
    q.projectId === null || projectId === q.projectId;
  if (q.kind !== "decision") {
    for (const t of getSnapshot().tasks) {
      if (!inScope(t.projectId)) continue;
      const standing = { status: t.status, priority: t.priority ?? undefined, labels: [] };
      push("task", "title", taskRef(t.id), t.title, t.notes, standing);
      push("task", "body", taskRef(t.id), t.title, t.notes, standing);
    }
  }
  if (q.kind !== "task") {
    for (const d of getSnapshot().decisions) {
      if (!inScope(d.project?.id)) continue;
      const standing = { status: d.status, labels: [] };
      push("decision", "title", decisionRef(d.id), d.title, d.body, standing);
      push("decision", "body", decisionRef(d.id), d.title, d.body, standing);
    }
  }
  // Face first, as core orders it; within a face the fixtures carry no instant to break ties by.
  const tier: SearchFace[] = ["title", "body", "comment", "label", "attachment"];
  hits.sort((a, b) => tier.indexOf(a.face) - tier.indexOf(b.face));
  return { hits: hits.slice(q.offset, q.offset + SEARCH_PAGE), totalMatched: hits.length };
}

/**
 * Every place a needle sits in the excerpt, in the shape a hit row reads: character positions, sorted,
 * and merged so no two overlap — two needles landing on the same characters have to arrive as one run,
 * or the row would have to reconcile them.
 *
 * Characters, not code units: the row splits the excerpt with `Array.from`, and positions counted any
 * other way would point at the wrong place the moment a fixture holds one.
 */
function mockRanges(snippet: string, needles: string[]): SearchHit["matches"] {
  const hay = Array.from(snippet).map((c) => c.toLowerCase());
  const found: { start: number; end: number }[] = [];
  for (const raw of needles) {
    const needle = Array.from(raw.toLowerCase());
    if (needle.length === 0) continue;
    for (let i = 0; i + needle.length <= hay.length; i++) {
      if (needle.every((c, k) => hay[i + k] === c)) {
        found.push({ start: i, end: i + needle.length });
        i += needle.length - 1;
      }
    }
  }
  found.sort((a, b) => a.start - b.start || a.end - b.end);
  const merged: { start: number; end: number }[] = [];
  for (const r of found) {
    const last = merged[merged.length - 1];
    if (last && r.start <= last.end) last.end = Math.max(last.end, r.end);
    else merged.push({ ...r });
  }
  return merged;
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

/** One git commit SHA recorded on a task (generated DTO). amenbo stores it opaque — the AI reads git. */
export type TaskCommit = TaskCommitDto;

/** Fetch a task's recorded commit SHAs, oldest first (Tauri: `task_commits`; browser: empty). */
export async function fetchTaskCommits(taskId: number): Promise<TaskCommit[]> {
  if (inTauri()) return invoke<TaskCommitDto[]>("task_commits", { taskId });
  return [];
}

/** Subscribing read of a task's commit SHAs (add/remove return a WriteAck that invalidates the task id — applyAck). */
export function useTaskCommits(taskId: number | null): TaskCommit[] {
  const { data } = useQuery<TaskCommit[]>(
    ["commits", taskId],
    () => (taskId !== null ? fetchTaskCommits(taskId) : Promise.resolve([])),
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
  const { data } = useQuery<{ tasks: TaskCard[]; total: number }>(
    ["smartView", viewId, page, pageSize],
    () => fetchSmartView(viewId, page, pageSize),
  );
  return data ?? { tasks: [], total: 0 };
}

/** Fetch one page of a smart view (the plain async that useQuery calls behind the hook). */
async function fetchSmartView(viewId: string, page: number, pageSize: number): Promise<{ tasks: TaskCard[]; total: number }> {
  if (viewId === "inbox") {
    const full = await fetchInboxTasks();
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
 * The inbox is "what I have to look at or act on". It is the union of a state-based set C (assigned to my human
 * facet, created by an AI, not yet started — the AI→human hand-off) and a comment-based set D (tasks carrying
 * a comment addressed to me). Membership does not depend on having read anything — clicking an item does not
 * evict it; archiving is the only way out. Read state feeds each item's `unread` display flag and nothing else.
 */
async function fetchInboxTasks(): Promise<TaskCard[]> {
  const [cand, commentTasks, archivedIds] = await Promise.all([
    // C candidates: assigned to me (assignee:me = the human facet) and not done.
    fetchSuperset({ filter: "assignee:me done:false", sort: "order" }),
    loadCommentTasks(),
    loadInboxArchived(),
  ]);
  const archived = new Set(archivedIds);
  const unreadById = new Map(commentTasks.map((c) => [c.id, c.unread]));
  // D: tasks with a comment addressed to me (they stay once read, and leave when the task closes — either
  // terminal, since a task decided against has no more to say to me than a finished one).
  const dTasks = (await fetchTasksByIds(commentTasks.map((c) => c.id))).filter((t) => !isClosed(t.status));
  const cTasks = cand.filter((t) => {
    // Mine = assigned to my human facet, handed over by an AI facet that created it (the AI→human hand-off).
    if (t.assignee?.kind !== "human") return false;
    return t.status === "todo" && t.createdBy?.kind === "ai";
  });
  // The archive set evicts unconditionally — the one and only exit, shared by C and D.
  const dIds = new Set(dTasks.map((t) => t.id));
  const inbox = dedupById([...dTasks, ...cTasks]).filter((t) => !archived.has(t.id));
  // Ask core when each item last became inbox-worthy (its C/D trigger) and carry it on the item, for display and sorting.
  const triggers = await loadTriggeredAt(inbox.map((t) => t.id));
  // Source C carries no unread flag (unread is D's, comment-derived), so its notification eligibility is "unseen":
  // it entered the inbox (triggeredAt) after this device last viewed the task, or was never viewed. Load last-seen
  // state only when a C-only item exists to judge — D items are gated by `unread` and need none.
  const cOnly = inbox.some((t) => !dIds.has(t.id));
  const seenByTask = cOnly ? (await loadReadReceipts()).tasks : {};
  const withTrigger = inbox.map((t) => {
    const triggeredAt = triggers[t.id] ?? null;
    const lastSeen = seenByTask[String(t.id)];
    // Only source C is judged by unseen; D stays on `unread`. Unseen = never viewed, or entered after last view.
    const unseen = !dIds.has(t.id) && (!lastSeen || (triggeredAt !== null && lastSeen < triggeredAt));
    return { ...t, triggeredAt, unread: unreadById.get(t.id) ?? false, unseen };
  });
  // Newest first (triggeredAt descending); unknown (null) sinks to the bottom. RFC3339 UTC, so a string compare is a time compare.
  return withTrigger.sort((a, b) => (b.triggeredAt ?? "").localeCompare(a.triggeredAt ?? ""));
}

/**
 * One inbox item, reduced to what arrival detection needs: its id, whether it is unread (source D's comment-derived
 * flag), and whether it is unseen (source C's device-local "entered after last viewed, or never viewed"). A source
 * announces on its own gate — D on `unread`, C on `unseen` — so the two never overlap on one item.
 */
export interface InboxItemBrief {
  id: number;
  unread: boolean;
  unseen: boolean;
}

/**
 * The current inbox (C ∪ D) as `{ id, unread, unseen }`, in the view's own order. Exported so the nav badge count
 * and arrival detection (`mailbox.ts`) can consult it without the view being open: the badge counts the whole set,
 * while the notification fires for the ones eligible on their source's gate — D's `unread` or C's `unseen`, both
 * computed by `fetchInboxTasks`. Empty before the snapshot loads (the roster is the tell — it is filled only then).
 */
export async function loadInboxItems(): Promise<InboxItemBrief[]> {
  if (getSnapshot().roster.length === 0) return [];
  return (await fetchInboxTasks()).map((t) => ({ id: t.id, unread: t.unread ?? false, unseen: t.unseen ?? false }));
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
 * One token's value read as the CLI's comma-separated any-of (`status:todo,in_progress`, query.rs): the task
 * passes when any of the values named does. A value naming nothing narrows nothing, which is also how an axis
 * with none of its values chosen reaches here from the board's filters (`AMB-D-655`).
 */
function anyOf(value: string, test: (v: string) => boolean): boolean {
  const values = (value ?? "").split(",").filter(Boolean);
  return values.length === 0 || values.some(test);
}

/**
 * Whether the assignee answers to one value of an `assignee:` token: `none` is unassigned, `me`/`me-ai` resolve
 * by facet kind, a bare `human`/`ai` by kind too, and anything else is matched against the display name.
 */
function assigneeIs(t: TaskCard, value: string): boolean {
  if (value === "none") return !t.assignee;
  if (value === "me") return t.assignee?.kind === "human";
  if (value === "me-ai") return t.assignee?.kind === "ai";
  const v = value.toLowerCase();
  return t.assignee?.kind === v || t.assignee?.name?.toLowerCase() === v;
}

/**
 * The browser-mock fallback (outside Tauri, i.e. iterating on the frontend alone). Filters the fixtures with a
 * subset of `task_page`'s grammar, evaluating only the tokens the views actually emit
 * (status/done/assignee/priority/due). Words are not among them: the search is its own door now
 * ({@link useTaskSearchIds}), not a `text:` written into this expression.
 */
function mockMatches(t: TaskCard, q: TaskPageQuery): boolean {
  if (q.projectId && t.projectId !== q.projectId) return false;
  for (const token of (q.filter ?? "").split(/\s+/).filter(Boolean)) {
    const [key, value] = token.split(":");
    switch (key) {
      case "status": if (!anyOf(value, (v) => t.status === v)) return false; break;
      // `done:` asks whether the task is **closed**, not whether it was carried out (`AMB-D-397`).
      case "done": if (isClosed(t.status) !== (value === "true")) return false; break;
      case "due": if (value === "today" && t.due !== TODAY) return false; break;
      case "priority": if (!anyOf(value, (v) => (t.priority ?? "none") === v)) return false; break;
      case "assignee": if (!anyOf(value, (v) => assigneeIs(t, v))) return false; break;
      // Unsupported tokens are ignored — the mock is an approximation for iterating. `dim:` is one of them:
      // the fixtures carry no dimension assignments, so there is nothing here to read a value against.
      default: break;
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
  const matched = mockSort(getSnapshot().tasks.filter((t) => mockMatches(t, q)), q.sort ?? "order");
  const offset = q.offset ?? 0;
  const limit = q.limit ?? matched.length;
  return { tasks: matched.slice(offset, offset + limit), totalMatched: matched.length };
}
