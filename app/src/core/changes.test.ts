import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
vi.mock("./ipc", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("./snapshot", () => ({ inTauri: () => true }));

import { drainChanges, foldScopes, takeChangeCursor, type ChangeRow } from "./changes";

const row = (dataset: string): ChangeRow => ({ dataset, rowId: 1, op: "update" });

/** One `changes_since` response. */
const page = (rows: ChangeRow[], cursor: number, more = false, expired = false) => ({
  rows,
  cursor,
  more,
  expired,
});

describe("foldScopes — folding datasets into invalidation scopes", () => {
  it("all task-side tables fold into tasks (comments, dependencies, dimension assignments included)", () => {
    const { scopes, unknown } = foldScopes([
      row("task"),
      row("task_comment"),
      row("task_dependency"),
      row("task_dimension_value"),
    ]);
    expect(unknown).toBe(false);
    expect([...scopes]).toEqual(["tasks"]);
  });

  it("decision-side tables fold into decisions", () => {
    const { scopes } = foldScopes([
      row("decision"),
      row("decision_comment"),
      row("decision_edge"),
      row("decision_dimension_value"),
    ]);
    expect([...scopes].sort()).toEqual(["decisions"]);
  });

  it("a task ⇄ decision link affects both sides (the task's decision badge, the decision's linked tasks)", () => {
    const { scopes } = foldScopes([row("decision_task_link")]);
    expect([...scopes].sort()).toEqual(["decisions", "tasks"]);
  });

  it("attachments and projects each go to their own scope (without dragging the task list along)", () => {
    expect([...foldScopes([row("attachment")]).scopes]).toEqual(["attachments"]);
    expect([...foldScopes([row("project")]).scopes]).toEqual(["projects"]);
  });

  // A plugin gate flipped from the CLI used to arrive as a dataset with no receiver, so every such write
  // cost a full re-read of everything on screen. It folds to the one surface that draws it.
  it("a plugin's gate and its settings fold into the plugin scope", () => {
    const { scopes, unknown } = foldScopes([row("plugin_enable"), row("plugin_config")]);
    expect(unknown).toBe(false);
    expect([...scopes]).toEqual(["plugins"]);
  });

  // The three tables this device keeps to itself. They are on the feed so a screen here hears them
  // change, and each is about a project — so that is the scope they fold to, rather than falling to the
  // full re-read an unknown dataset asks for.
  it("this device's own tables fold to the project they are about", () => {
    const { scopes, unknown } = foldScopes([
      row("binding_project_dir"),
      row("hook_optout"),
      row("harness_consent"),
    ]);
    expect(unknown).toBe(false);
    expect([...scopes]).toEqual(["projects"]);
  });

  it("no changes means no scopes", () => {
    expect(foldScopes([])).toEqual({ scopes: new Set(), unknown: false });
  });

  it("a dataset with no receiver is not folded but sets unknown (dropping it silently would freeze the screen on stale data)", () => {
    const { scopes, unknown } = foldScopes([row("task"), row("some_new_table")]);
    expect(unknown).toBe(true);
    expect(scopes.size).toBe(0); // a partial scope set would pass itself off as the whole story.
  });
});

describe("drainChanges — draining everything past the cursor and folding into scopes", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  /** Puts us in the state right after reading the store: a position taken via `change_cursor`. */
  async function startAt(head: number): Promise<void> {
    invoke.mockResolvedValueOnce(head);
    await takeChangeCursor();
    invoke.mockReset();
  }

  it("reads only past the cursor, and the next wake-up continues from there (O(number of changes))", async () => {
    await startAt(10);
    invoke.mockResolvedValueOnce(page([row("task")], 12));
    expect(await drainChanges()).toEqual({ scopes: new Set(["tasks"]), gap: false });
    expect(invoke).toHaveBeenCalledWith("changes_since", { cursor: 10, limit: null });

    invoke.mockResolvedValueOnce(page([row("decision")], 13));
    expect(await drainChanges()).toEqual({ scopes: new Set(["decisions"]), gap: false });
    expect(invoke).toHaveBeenLastCalledWith("changes_since", { cursor: 12, limit: null });
  });

  it("keeps pulling when a page is cut off, and unions the scopes across all pages", async () => {
    await startAt(0);
    invoke
      .mockResolvedValueOnce(page([row("task")], 1, true))
      .mockResolvedValueOnce(page([row("attachment")], 2));
    expect(await drainChanges()).toEqual({ scopes: new Set(["tasks", "attachments"]), gap: false });
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("cursor expiry is a gap (an empty reply is not read as \"no changes\"); after the re-read it resumes from the returned head", async () => {
    await startAt(1);
    invoke.mockResolvedValueOnce(page([], 900, false, true));
    expect(await drainChanges()).toEqual({ scopes: new Set(), gap: true });

    invoke.mockResolvedValueOnce(page([row("task")], 901));
    await drainChanges();
    expect(invoke).toHaveBeenLastCalledWith("changes_since", { cursor: 900, limit: null });
  });

  it("an empty feed is an answer, not a gap: a commit the feed collects no row for touches nothing to re-read", async () => {
    await startAt(5);
    invoke.mockResolvedValueOnce(page([], 5));
    expect(await drainChanges()).toEqual({ scopes: new Set(), gap: false });
  });

  it("an unfoldable dataset yields a gap (do not partially invalidate and mistake it for \"reflected\")", async () => {
    await startAt(5);
    invoke.mockResolvedValueOnce(page([row("task"), row("some_new_table")], 7));
    expect(await drainChanges()).toEqual({ scopes: new Set(), gap: true });
  });

  it("gap when we have lost our position — but it retakes the position before re-reading, and returns to the feed on the next wake-up", async () => {
    invoke.mockRejectedValueOnce(new Error("no store"));
    await takeChangeCursor(); // the startup fetch failed, so there is no position.
    invoke.mockReset();

    invoke.mockResolvedValueOnce(30); // this wake-up is a gap, but it takes a position first.
    expect(await drainChanges()).toEqual({ scopes: new Set(), gap: true });
    expect(invoke).toHaveBeenCalledWith("change_cursor", undefined);

    invoke.mockReset();
    invoke.mockResolvedValueOnce(page([row("task")], 31));
    expect(await drainChanges()).toEqual({ scopes: new Set(["tasks"]), gap: false });
    expect(invoke).toHaveBeenLastCalledWith("changes_since", { cursor: 30, limit: null });
  });

  it("if the feed cannot be read it drops the position and gaps; that position is retaken on the next wake-up (no permanent gap)", async () => {
    await startAt(3);
    invoke.mockRejectedValueOnce(new Error("gone"));
    expect(await drainChanges()).toEqual({ scopes: new Set(), gap: true });

    invoke.mockReset();
    invoke.mockResolvedValueOnce(7); // the position is retaken, though this round is still a gap.
    expect(await drainChanges()).toEqual({ scopes: new Set(), gap: true });

    invoke.mockReset();
    invoke.mockResolvedValueOnce(page([row("task")], 8));
    expect(await drainChanges()).toEqual({ scopes: new Set(["tasks"]), gap: false });
  });
});
