import { describe, expect, it } from "vitest";

import { addComment, addTask, deleteProject, deleteTask, finishTaskCreation, rejectTask, setDue, setStart, setStatus } from "./mutations";
import { agoLabel } from "./i18n";
import { addDays, todayStr } from "./calendar";
import { applySnapshot, getSnapshot, type Snapshot } from "./snapshot";
import type { TaskCard } from "../mock/types";

const task = (id: number, projectId: number | null) =>
  ({ id, title: `t${id}`, projectId }) as unknown as TaskCard;

const decision = (id: number, projectId: number) =>
  ({ id, title: `d${id}`, project: { id: projectId, name: "P" } }) as unknown as Snapshot["decisions"][number];

function seed(): void {
  applySnapshot({
    ...getSnapshot(),
    projects: [{ id: 1, name: "消される PJ" }, { id: 2, name: "残る PJ" }] as unknown as Snapshot["projects"],
    tasks: [task(10, 1), task(11, 2), task(12, null)],
    decisions: [decision(20, 1), decision(21, 2)],
    activity: [],
  });
}

describe("deleteProject (browser-loop mock)", () => {
  it("takes down its tasks and decisions with it (shows the same store as core's subtree delete)", async () => {
    seed();
    await deleteProject(1);
    const s = getSnapshot();

    expect(s.projects.map((p) => p.id)).toEqual([2]);
    // Delete only the project row and its tasks linger in the lists, still belonging to a project that no longer exists.
    expect(s.tasks.map((t) => t.id)).toEqual([11, 12]);
    expect(s.decisions.map((d) => d.id)).toEqual([21]);
  });

  it("pushes project.deleted onto the ledger, and that row has nowhere to open to", async () => {
    seed();
    await deleteProject(1);
    const row = getSnapshot().activity[0];

    expect(row.event?.kind).toBe("project.deleted");
    expect(row.target.type).toBe("project");
    expect(row.target.id).toBe(1);
    expect(row.target.title).toBe("消される PJ");
    expect(row.target.live).toBe(false);
  });
});

describe("deleteTask (browser-loop mock)", () => {
  it("the delete row also has nowhere to open to", async () => {
    seed();
    await deleteTask(10);
    const row = getSnapshot().activity[0];

    expect(row.event?.kind).toBe("task.deleted");
    expect(row.target.live).toBe(false);
  });
});

const full = (id: number, over: Partial<TaskCard> = {}) =>
  ({
    id, ref: `#${id}`, title: `t${id}`, notes: "", projectId: 1, status: "todo", assignee: null,
    priority: null, due: null, dueLabel: null, completedAt: null, comments: 0, createdBy: null,
    ready: true, blockedBy: [], placement: null, linkedDecisions: [], blockedByDecisions: [],
    ...over,
  }) as unknown as TaskCard;

function seedTasks(tasks: TaskCard[]): void {
  applySnapshot({ ...getSnapshot(), tasks, activity: [] });
}

describe("setStatus (browser-loop mock)", () => {
  it("done carries a completion time and loses it when reverted (status is the source of truth for done; completedAt is subordinate)", async () => {
    seedTasks([full(10)]);
    await setStatus(10, "done");
    expect(getSnapshot().tasks[0].completedAt).not.toBeNull();

    await setStatus(10, "todo");
    const t = getSnapshot().tasks[0];
    expect(t.status).toBe("todo");
    expect(t.completedAt).toBeNull();
  });

  it("reservation goes through only from todo (the mock honors the CAS too)", async () => {
    seedTasks([full(10, { status: "in_progress" })]);
    await expect(setStatus(10, "in_progress")).rejects.toMatchObject({ code: "already_reserved" });
    expect(getSnapshot().activity).toEqual([]); // A rejected operation leaves no trace in the ledger either
  });

  it("cannot reserve while prerequisites are unmet (ready guard)", async () => {
    seedTasks([full(10, { ready: false, blockedBy: [{ id: 9, name: "t9" }] })]);
    await expect(setStatus(10, "in_progress")).rejects.toMatchObject({ code: "not_ready" });
    expect(getSnapshot().tasks[0].status).toBe("todo");
  });

  it("once the blocker is done, the waiting task becomes ready to start", async () => {
    seedTasks([full(9), full(10, { ready: false, blockedBy: [{ id: 9, name: "t9" }] })]);
    await setStatus(9, "done");

    const waiting = getSnapshot().tasks.find((t) => t.id === 10)!;
    expect(waiting.blockedBy).toEqual([]);
    expect(waiting.ready).toBe(true);
    await setStatus(10, "in_progress"); // Now that it is ready, the reservation goes through
    expect(getSnapshot().tasks.find((t) => t.id === 10)!.status).toBe("in_progress");
  });
});

describe("the two stages of a creation (browser-loop mock)", () => {
  // The mock runs the same two stages core does (`AMB-D-554`); waving the first one through here would make
  // browser iteration the one place a task straight out of the compose form can be picked up.
  it("a creation lands unfinished and is refused a reservation until it is ended", async () => {
    seedTasks([]);
    const id = (await addTask(1, "作りかけ"))!;
    const created = getSnapshot().tasks.find((t) => t.id === id)!;
    expect(created.draft).toBe(true);
    expect(created.ready).toBe(false);

    await expect(setStatus(id, "in_progress")).rejects.toMatchObject({
      code: "not_ready",
      parts: [{ code: "not_ready_draft" }],
    });

    await finishTaskCreation(id);
    const finished = getSnapshot().tasks.find((t) => t.id === id)!;
    expect(finished.draft).toBe(false);
    expect(finished.ready).toBe(true);
    await setStatus(id, "in_progress");
    expect(getSnapshot().tasks.find((t) => t.id === id)!.status).toBe("in_progress");
  });

  it("ending a creation does not lift the premises that are someone else's to lift", async () => {
    seedTasks([]);
    const id = (await addTask(1, "先行待ち"))!;
    applySnapshot({
      ...getSnapshot(),
      tasks: getSnapshot().tasks.map((t) => (t.id === id ? { ...t, blockedBy: [{ id: 9, name: "t9" }] } : t)),
    });

    await finishTaskCreation(id);
    const t = getSnapshot().tasks.find((x) => x.id === id)!;
    expect(t.draft).toBe(false);
    expect(t.ready).toBe(false); // The blocker is still there, and readiness reads all four premises
  });
});

describe("rejectTask (browser-loop mock)", () => {
  it("keeps the reasoning as a comment, and carries no completion time (a terminal, not an achievement)", async () => {
    seedTasks([full(10)]);
    await rejectTask(10, "  測っても分岐が痩せていて何も変わらない  ");

    const t = getSnapshot().tasks[0];
    expect(t.status).toBe("rejected");
    expect(t.completedAt).toBeNull();
    expect(t.comments).toBe(1);
    expect(getSnapshot().activity.some((a) => a.text === "測っても分岐が痩せていて何も変わらない")).toBe(true);
  });

  it("refuses an empty reason, and writes nothing (the reason is the point of the command)", async () => {
    seedTasks([full(10)]);
    await expect(rejectTask(10, "   ")).rejects.toMatchObject({ code: "invalid_value" });
    expect(getSnapshot().tasks[0].status).toBe("todo");
    expect(getSnapshot().activity).toEqual([]);
  });

  it("releases what was waiting on it — a blocker decided against is a blocker no longer", async () => {
    seedTasks([full(9), full(10, { ready: false, blockedBy: [{ id: 9, name: "t9" }] })]);
    await rejectTask(9, "やらない");

    const waiting = getSnapshot().tasks.find((t) => t.id === 10)!;
    expect(waiting.blockedBy).toEqual([]);
    expect(waiting.ready).toBe(true);
  });

  it("does not pile the reason on a second time (re-rejecting changes nothing)", async () => {
    seedTasks([full(10, { status: "rejected" })]);
    await rejectTask(10, "また同じことを言う");
    expect(getSnapshot().tasks[0].comments).toBe(0);
  });
});

describe("deleteTask — cleaning up dependencies", () => {
  it("leaves no task still waiting on a deleted task (core removes the dependency edges too)", async () => {
    seedTasks([full(9), full(10, { ready: false, blockedBy: [{ id: 9, name: "t9" }] })]);
    await deleteTask(9);

    const waiting = getSnapshot().tasks.find((t) => t.id === 10)!;
    expect(waiting.blockedBy).toEqual([]);
    expect(waiting.ready).toBe(true);
  });
});

describe("addComment (browser-loop mock)", () => {
  it("increments the comment count and pushes its own row onto the ledger", async () => {
    seedTasks([full(10)]);
    await addComment(10, "やります");
    const s = getSnapshot();

    expect(s.tasks[0].comments).toBe(1);
    expect(s.activity[0].kind).toBe("comment");
    expect(s.activity[0].text).toBe("やります");
    // Stamped with now, checked against now — never against a literal, which would pin one language's
    // wording into a test about the ledger.
    expect(agoLabel(s.activity[0].at)).toBe(agoLabel(new Date().toISOString()));
  });

  it("does not push onto a nonexistent task (core rejects it on the foreign key)", async () => {
    seedTasks([full(10)]);
    await addComment(99, "宛先なし");
    expect(getSnapshot().activity).toEqual([]);
  });
});

describe("the two days (browser-loop mock)", () => {
  // A day either side of today, counted off the same clock the code under test counts off: the premise
  // is decided against the device's own calendar day, so a fixture built from any other one drifts.
  const dayFromNow = (n: number) => addDays(todayStr(), n);

  it("writes and takes away the due date, and nothing else moves with it", async () => {
    seedTasks([full(10)]);
    await setDue(10, "2099-12-31");
    expect(getSnapshot().tasks[0].due).toBe("2099-12-31");
    // The due date is not a premise: a task with one due long ago is still reservable.
    expect(getSnapshot().tasks[0].ready).toBe(true);

    await setDue(10, null);
    expect(getSnapshot().tasks[0].due).toBeNull();
  });

  it("holds the task unready while the start day is still ahead, and hands it back when it is not", async () => {
    seedTasks([full(10)]);
    const ahead = dayFromNow(3);
    await setStart(10, ahead);
    let t = getSnapshot().tasks[0];
    expect(t.startOn).toBe(ahead);
    expect(t.notStartedUntil).toBe(ahead);
    expect(t.ready).toBe(false);

    // A day that has come is still the value that was written — it just stops being a reason to wait.
    const past = dayFromNow(-3);
    await setStart(10, past);
    t = getSnapshot().tasks[0];
    expect(t.startOn).toBe(past);
    expect(t.notStartedUntil).toBeNull();
    expect(t.ready).toBe(true);

    await setStart(10, ahead);
    await setStart(10, null);
    t = getSnapshot().tasks[0];
    expect(t.startOn).toBeNull();
    expect(t.notStartedUntil).toBeNull();
    expect(t.ready).toBe(true);
  });
});
