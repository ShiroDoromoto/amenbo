// Fixture: a believable sample workspace so the mock looks real.
// A single local store has exactly two facets — my own human and my own AI — so the whole cast is A_H / A_AI.
import type { Project, TaskCard, ActivityItem, Actor } from "./types";
import { taskRef } from "../core/idref";

const A_H: Actor = { name: "あなた", kind: "human" };
const A_AI: Actor = { name: "あなたの AI", kind: "ai" };

// The roster of a single local store: my own two facets, human and ai. This is what feeds the assignee picker (unassigned / human name / AI name).
export const roster: Actor[] = [A_H, A_AI];

export const projects: Project[] = [
  {
    id: 1,
    name: "サイト刷新",
    color: "#0e7c7b",
    view: "board",
    openCount: 5,
    proposedDecisionCount: 0,
    dimensions: [],
  },
  {
    id: 2,
    name: "採用",
    color: "#8a4fd0",
    view: "board",
    openCount: 2,
    proposedDecisionCount: 0,
    dimensions: [],
  },
];

// The generated `TaskCardDto` requires every field on the wire, because core always sends them all. A
// fixture wants to state only what matters, so this factory fills the rest of the wire fields with
// defaults and keeps the mock pleasant to write.
type MockTaskInput = Partial<TaskCard> &
  Pick<TaskCard, "id" | "title" | "projectId" | "status" | "assignee" | "priority" | "due" | "comments" | "createdBy">;
const mt = (t: MockTaskInput): TaskCard => ({
  ref: taskRef(t.id), notes: "", completedAt: null,
  ready: true, blockedBy: [], placement: null, linkedDecisions: [], blockedByDecisions: [], notStartedUntil: null,
  ...t,
});

export const tasks: TaskCard[] = [
  mt({
    id: 1,
    title: "ワイヤーフレーム作成",
    projectId: 1,
    status: "in_progress",
    assignee: A_H,
    priority: "high",
    due: "2026-06-22",
    comments: 3,
    createdBy: A_H,
  }),
  mt({
    id: 2,
    title: "配色パターン決め",
    projectId: 1,
    status: "todo",
    assignee: A_AI,
    priority: "medium",
    due: null,
    comments: 0,
    createdBy: A_H,
  }),
  mt({
    id: 3,
    title: "API 設計",
    projectId: 1,
    status: "in_progress",
    assignee: A_AI,
    priority: "high",
    due: "2026-06-23",
    comments: 1,
    createdBy: A_AI,
  }),
  mt({
    id: 4,
    title: "認証フロー実装",
    projectId: 1,
    status: "blocked",
    assignee: A_H,
    priority: "high",
    due: "2026-06-20",
    comments: 5,
    createdBy: A_H,
  }),
  mt({
    id: 5,
    title: "実装方針メモ",
    projectId: 1,
    status: "in_progress",
    assignee: A_AI,
    priority: "low",
    due: null,
    comments: 2,
    createdBy: A_AI,
  }),
  mt({
    id: 6,
    title: "配色チェック",
    projectId: 1,
    status: "done",
    assignee: A_H,
    priority: null,
    due: null,
    comments: 0,
    createdBy: A_H,
  }),
];

export const activity: ActivityItem[] = [
  {
    id: 1, seq: 0, at: "2026-06-21T09:58:00Z", kind: "system", author: A_AI,
    target: { type: "task", id: 3, title: "API 設計", live: true },
    event: { kind: "task.status_changed", status: "in_progress" },
  },
  {
    id: 2, seq: 0, at: "2026-06-21T09:55:00Z", kind: "comment", author: A_H,
    target: { type: "task", id: 1, title: "ワイヤーフレーム作成", live: true },
    text: "先方確認待ち。木曜には返ってくる想定。",
  },
  {
    id: 3, seq: 0, at: "2026-06-21T09:52:00Z", kind: "system", author: A_AI,
    target: { type: "task", id: 3, title: "API 設計", live: true },
    event: { kind: "task.created" },
    burstCount: 3,
  },
  {
    id: 4, seq: 0, at: "2026-06-21T09:48:00Z", kind: "system", author: A_H,
    target: { type: "project", id: 9, title: "旧サイト（統合前）", live: false },
    event: { kind: "project.deleted", tasks: 4, decisions: 1 },
  },
  {
    id: 5, seq: 0, at: "2026-06-21T09:42:00Z", kind: "system", author: A_H,
    target: { type: "task", id: 6, title: "配色チェック", live: true },
    event: { kind: "task.status_changed", status: "done" },
  },
  {
    id: 7, seq: 0, at: "2026-06-21T09:36:00Z", kind: "system", author: A_H,
    target: { type: "task", id: 11, title: "重複していた下書き", live: false },
    event: { kind: "task.deleted" },
  },
  {
    // A comment on a *live* decision — the row the feed opens, replies to and edits on the decision side, the twin
    // of the task-aimed comment above. Two ids here deliberately repeat one from another space: its *target* id is
    // also a task id, and its own row id is one the shared activity counter already gave out (`AMB-D-388` — a
    // decision comment is numbered against its own table). Anything routing or identifying by id alone is caught here.
    id: 2, seq: 1, at: "2026-06-21T09:33:00Z", kind: "comment", author: A_H,
    target: { type: "decision", id: 3, title: "RDB を真実源にする", live: true },
    text: "この線で進める。移行は次の版で。",
  },
  {
    id: 6, seq: 0, at: "2026-06-21T09:30:00Z", kind: "system", author: A_H,
    target: { type: "decision", id: 2, title: "旧方針の決定", live: false },
    event: { kind: "decision.deleted" },
  },
];
