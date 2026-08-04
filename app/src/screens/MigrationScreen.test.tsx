// @vitest-environment jsdom
// Only the boundaries are stubbed — the event subscriptions, `ui_language`, and the retry invoke — so the screen's own
// rendering and branching run for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MigrationStatusDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  retries: 0,
  /** What the read taken *after* the subscriptions are up answers with (null = the screen is left as it mounted). */
  pulled: null as MigrationStatusDto | null,
  /** Run while that read is in flight, so a test can land an event on the screen before the read comes back. */
  duringPull: null as (() => void) | null,
  /** The `migration-changed` handler the screen registered. */
  push: null as ((s: MigrationStatusDto) => void) | null,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async () => "ja"), // ui_language
}));
// i18n takes the UI language from snapshot.language (the migration screen itself re-reads it with `ui_language`).
vi.mock("../core/snapshot", () => ({
  inTauri: () => true,
  getSnapshot: () => ({ language: "ja" }),
}));
vi.mock("../core/migration", async () => {
  const actual = await vi.importActual<typeof import("../core/migration")>("../core/migration");
  return {
    ...actual,
    listenMigrationChanged: async (cb: (s: MigrationStatusDto) => void) => { hoisted.push = cb; return () => {}; },
    listenMigrationProgress: async () => () => {},
    migrationStatus: async () => { hoisted.duringPull?.(); return hoisted.pulled; },
    retryMigration: async () => { hoisted.retries += 1; },
  };
});

import { MigrationScreen } from "./MigrationScreen";

function status(over: Partial<MigrationStatusDto>): MigrationStatusDto {
  return { stage: "running", pending: null, progress: null, report: null, error: null, ...over };
}

let host: HTMLDivElement;
let root: Root;

async function render(initial: MigrationStatusDto, onDone = () => {}) {
  await act(async () => {
    root.render(createElement(MigrationScreen, { initial, onDone }));
  });
}

beforeEach(() => {
  hoisted.retries = 0;
  hoisted.pulled = null;
  hoisted.duringPull = null;
  hoisted.push = null;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("MigrationScreen", () => {
  it("during migration, shows the versions being carried, the step count, and the pre-migration backup size", async () => {
    await render(
      status({
        pending: {
          from: 2,
          to: 4,
          steps: 2,
          archiveBytes: 20 * 1024 * 1024,
          stagingBytes: 10 * 1024 * 1024,
          requiredBytes: 30 * 1024 * 1024,
          availableBytes: 500 * 1024 * 1024,
        },
        progress: { phase: "snapshotting", done: 0, total: 1 },
      }),
    );
    const text = host.textContent ?? "";
    expect(text).toContain("v2");
    expect(text).toContain("v4");
    expect(text).toContain("30 MiB"); // required
    expect(text).toContain("500 MiB"); // free
    expect(text).toContain("スナップショット作成");
    // A migration cannot be walked away from, so no cancel button is offered.
    expect(host.querySelectorAll("button")).toHaveLength(0);
  });

  it("does not name a version before the announce (while waiting on the lock)", async () => {
    await render(status({ pending: null }));
    const text = host.textContent ?? "";
    expect(text).not.toContain("v0"); // never name a version that does not exist
    expect(text).toContain("準備をしています");
  });

  it("on completion, shows where the pre-migration backup is and the old rollback points it swept", async () => {
    const onDone = vi.fn();
    await render(
      status({
        stage: "done",
        report: {
          from: 2,
          to: 4,
          backupPath: "/data/pre-migrate-20260714T010203Z.amenbo-backup",
          superseded: ["/data/pre-migrate-20260101T000000Z.amenbo-backup"],
        },
      }),
      onDone,
    );
    const text = host.textContent ?? "";
    expect(text).toContain("/data/pre-migrate-20260714T010203Z.amenbo-backup");
    expect(text).toContain("1 件");
    act(() => host.querySelector("button")!.click());
    expect(onDone).toHaveBeenCalled();
  });

  it("on failure, shows core's reason and lets you retry", async () => {
    await render(
      status({
        stage: "failed",
        error: {
          code: "invalid",
          message_en: "the migration failed and your store was rolled back",
          fields: null,
        },
      }),
    );
    expect(host.textContent ?? "").toContain("the migration failed and your store was rolled back");
    await act(async () => host.querySelector("button")!.click());
    expect(hoisted.retries).toBe(1);
  });

  it("when there is nothing to carry (another process already migrated), it passes through without showing the screen", async () => {
    const onDone = vi.fn();
    await render(status({ stage: "idle" }), onDone);
    expect(onDone).toHaveBeenCalled();
  });

  // A one-step chain takes about a second, which is less than it takes to raise a window: the end is published while
  // the screen is still wiring its subscriptions, and an event published to nobody is gone. The screen would then sit
  // on the last thing it was told — a progress bar over a store that finished moving — until the app is quit.
  it("takes up the end that was published before it could listen", async () => {
    hoisted.pulled = status({
      stage: "done",
      report: { from: 19, to: 20, backupPath: "/data/pre-migrate-20260803T185354Z.amenbo-backup", superseded: [] },
    });
    await render(status({ progress: { phase: "verifying", done: 0, total: 1 } }));
    expect(host.textContent ?? "").toContain("/data/pre-migrate-20260803T185354Z.amenbo-backup");
  });

  it("takes up an end that left nothing to show (the CLI carried the store while we waited on the lock)", async () => {
    const onDone = vi.fn();
    hoisted.pulled = status({ stage: "idle" });
    await render(status({}), onDone);
    expect(onDone).toHaveBeenCalled();
  });

  // The read is a catch-up, not a source of truth: it is answered from a stage that may already have moved on, so an
  // event that lands while it is in flight has to stand.
  it("does not let that read put back a stage an event has already moved past", async () => {
    hoisted.pulled = status({ progress: { phase: "verifying", done: 0, total: 1 } });
    hoisted.duringPull = () => {
      hoisted.push?.(status({
        stage: "done",
        report: { from: 19, to: 20, backupPath: "/data/pre-migrate-20260803T185354Z.amenbo-backup", superseded: [] },
      }));
    };
    await render(status({}));
    expect(host.textContent ?? "").toContain("/data/pre-migrate-20260803T185354Z.amenbo-backup");
  });
});
