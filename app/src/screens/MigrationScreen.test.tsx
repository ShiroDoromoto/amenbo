// @vitest-environment jsdom
// Only the boundaries are stubbed — the event subscriptions, `ui_language`, and the retry invoke — so the screen's own
// rendering and branching run for real.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MigrationStatusDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({ retries: 0 }));

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
    listenMigrationChanged: async () => () => {},
    listenMigrationProgress: async () => () => {},
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
});
