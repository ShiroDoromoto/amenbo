// @vitest-environment jsdom
// Settings > integrity (the doctor surface). Core may hand over the same issues either way, but if the screen
// settles for merely showing them, someone who only ever uses the GUI is left unable to fix anything. What is
// checked here is issue → display, and how destructive a repair is allowed to be.
//
// Only the boundaries are replaced (the reads, the repairs, the confirm dialog); the screen's rendering and
// branching are the real thing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DoctorIssueDto, DoctorReportDto, RestoreReportDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What `fetchDoctorReport` answers: consumed from the front on each check; the last one then repeats. */
  reports: [] as DoctorReportDto[],
  /** What the confirm dialog answers. */
  confirmAnswer: true,
  /** How many times a confirm dialog was raised. */
  confirms: 0,
  /** How many times `runDoctorFix` was let through. */
  fixes: 0,
  /** The arguments a per-row repair passed down the core path. */
  binds: [] as { project: number; dir: string }[],
  resyncs: [] as string[],
  /** The archive the restore picks (null = the file chooser was cancelled). */
  restoreArchive: null as string | null,
  /** The report `runRestore` comes back with. */
  restoreReport: { previousSavedTo: null, blobs: 0, superseded: 0, migration: null } as RestoreReportDto,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true };
});
vi.mock("../core/dialog", () => ({
  confirmDialog: () => {
    hoisted.confirms += 1;
    return Promise.resolve(hoisted.confirmAnswer);
  },
}));
vi.mock("../core/mutations", () => {
  const noop = () => Promise.resolve();
  return {
    fetchDoctorReport: () =>
      Promise.resolve(hoisted.reports.length > 1 ? hoisted.reports.shift()! : hoisted.reports[0]),
    runDoctorFix: () => {
      hoisted.fixes += 1;
      return Promise.resolve({ reclaimedBlobs: 0, freedBytes: 0, forgottenBindings: 1 });
    },
    bindFolder: (project: number, dir: string) => {
      hoisted.binds.push({ project, dir });
      return Promise.resolve();
    },
    resyncManagedBlocks: (dir: string) => {
      hoisted.resyncs.push(dir);
      return Promise.resolve({ folders: 1, rewritten: 1 });
    },
    fetchStoreLocations: () => Promise.resolve({ root: "/w" }),
    fileToAvatarDataUrl: () => Promise.resolve(""),
    listenDataProgress: () => Promise.resolve(() => {}),
    pickBackupPath: () => Promise.resolve(null),
    pickExportPath: () => Promise.resolve(null),
    pickRestoreArchive: () => Promise.resolve(hoisted.restoreArchive),
    runBackup: noop, runExport: noop,
    runRestore: () => Promise.resolve(hoisted.restoreReport),
    cancelDataOp: noop,
    setFacetNames: noop, setFacetAvatar: noop, setLanguage: noop, setPerfLog: noop, setUpdateCheck: noop,
  };
});

import { SettingsScreen } from "./SettingsScreen";
import { doctorText, t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

// Core carries no prose: only a kind and its params arrive, and this surface composes the sentence in the UI language.
function issue(over: Partial<DoctorIssueDto> = {}): DoctorIssueDto {
  return {
    kind: "orphan_binding",
    severity: "warning",
    target: "/w/ghost",
    params: { dir: "/w/ghost" },
    ...over,
  };
}

function report(over: Partial<DoctorReportDto> = {}): DoctorReportDto {
  return { ok: true, errors: 0, warnings: 0, issues: [], ...over };
}

/** Draw the screen and wait for doctor's first check to finish. */
async function render() {
  await act(async () => {
    root.render(createElement(SettingsScreen));
  });
}

/** The "repair" button, found by its i18n label. */
function fixButton(): HTMLButtonElement {
  const btn = [...container.querySelectorAll("button")].find((b) => b.textContent === t("settings.doctorFix"));
  if (!btn) throw new Error("no repair button");
  return btn as HTMLButtonElement;
}

/** A `runRestore` report; by default, a restore with nothing worth remarking on. */
function restored(over: Partial<RestoreReportDto> = {}): RestoreReportDto {
  return { previousSavedTo: null, blobs: 0, superseded: 0, migration: null, ...over };
}

/** A button found by its label, or null if there is none. */
function buttonByLabel(label: string): HTMLButtonElement | null {
  return ([...container.querySelectorAll("button")].find((b) => b.textContent === label) ?? null) as HTMLButtonElement | null;
}

beforeEach(() => {
  hoisted.reports = [report()];
  hoisted.confirmAnswer = true;
  hoisted.confirms = 0;
  hoisted.fixes = 0;
  hoisted.binds = [];
  hoisted.resyncs = [];
  hoisted.restoreArchive = null;
  hoisted.restoreReport = restored();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("Settings > Integrity (doctor surface)", () => {
  it("shows each issue core raised as a sentence composed in the UI language, together with how to fix it", async () => {
    hoisted.reports = [report({ warnings: 1, issues: [issue()] })];
    await render();
    const { message, fixHint } = doctorText(issue());
    expect(container.textContent).toContain(message);
    expect(container.textContent).toContain("/w/ghost"); // name what is broken
    expect(container.textContent).toContain(fixHint); // and point at the GUI's way of fixing it (the repair button)
    expect(container.textContent).not.toContain(t("settings.doctorClean"));
  });

  it("since the cleanup path is non-destructive, repair goes through without confirmation, then re-checks and reports whether it is fixed", async () => {
    hoisted.reports = [
      report({ warnings: 1, issues: [issue()] }),
      report(), // the re-check after the repair comes back clean
    ];
    await render();
    await act(async () => fixButton().click());
    expect(hoisted.confirms).toBe(0);
    expect(hoisted.fixes).toBe(1);
    expect(container.textContent).toContain(t("settings.doctorClean"));
  });

  // A real store held 411 of one kind. Drawing them all is what made this panel unreadable, and the
  // count is what the reader actually needs once the first few have said what the problem is.
  it("names the first few of a kind and counts the rest, with the hint said once", async () => {
    const many = Array.from({ length: 30 }, (_, i) =>
      issue({
        kind: "dead_ref",
        severity: "warning",
        target: `AMB-T-${i}`,
        params: { at: `AMB-T-${i}`, refs: "AMB-T-9999" },
      }));
    hoisted.reports = [report({ warnings: 30, issues: many })];
    await render();

    const drawn = many.filter((i) => container.textContent!.includes(doctorText(i).message)).length;
    expect(drawn).toBe(10);
    expect(container.textContent).toContain(tf("settings.doctorMore", { count: 20 }));

    // One hint for the kind, not one per row: it was the same sentence 30 times.
    const hint = doctorText(many[0]).fixHint;
    expect(container.textContent!.split(hint).length - 1).toBe(1);
  });

  // The sweep's own subject, said where it is: it does not repair what the list is showing, and being
  // read as the button that clears the count is exactly what went wrong before.
  it("says so when nothing in the list is repairable from here", async () => {
    hoisted.reports = [report({
      warnings: 1,
      issues: [issue({ kind: "dead_ref", params: { at: "AMB-T-1", refs: "AMB-T-9999" } })],
    })];
    await render();
    expect(container.textContent).toContain(t("settings.doctorNoneRepairable"));
  });

  it("stays quiet about that when a row does carry a repair", async () => {
    hoisted.reports = [report({ warnings: 1, issues: [issue({ kind: "stale_managed_block" })] })];
    await render();
    expect(container.textContent).not.toContain(t("settings.doctorNoneRepairable"));
  });
});

describe("Settings > Integrity (per-row repair; buttons appear only on rows whose fix is uniquely determined)", () => {
  const missingPointer = (): DoctorIssueDto => ({
    kind: "missing_pointer",
    severity: "warning",
    target: "/w/proj",
    params: { dir: "/w/proj", project: "3" },
  });
  const ambiguous = (): DoctorIssueDto => ({
    kind: "missing_pointer_ambiguous",
    severity: "warning",
    target: "/w/proj",
    params: { dir: "/w/proj", claims: "#3, #4" },
  });
  const staleBlock = (): DoctorIssueDto => ({
    kind: "stale_managed_block",
    severity: "warning",
    target: "/w/proj/AGENTS.md",
    params: { path: "/w/proj/AGENTS.md", dir: "/w/proj", version: "1", current: "2" },
  });

  it("rebinds a folder whose marker is gone to the recorded project via that row's button", async () => {
    hoisted.reports = [report({ warnings: 1, issues: [missingPointer()] }), report()];
    await render();
    await act(async () => buttonByLabel(t("settings.doctorRebind"))!.click());
    expect(hoisted.binds).toEqual([{ project: 3, dir: "/w/proj" }]);
    // Whether it is fixed is what the re-check says — no bare success message left standing on its own.
    expect(container.textContent).toContain(t("settings.doctorClean"));
  });

  it("resyncs stale AI guidance for that one folder only, via that row's button", async () => {
    hoisted.reports = [report({ warnings: 1, issues: [staleBlock()] }), report()];
    await render();
    await act(async () => buttonByLabel(t("managedBlock.resync"))!.click());
    expect(hoisted.resyncs).toEqual(["/w/proj"]); // this one folder, not every folder
  });

  it("an issue with no determinable binding target gets no button (never silently pick another project)", async () => {
    hoisted.reports = [report({ warnings: 1, issues: [ambiguous()] })];
    await render();
    expect(buttonByLabel(t("settings.doctorRebind"))).toBeNull();
    expect(container.textContent).toContain(doctorText(ambiguous()).fixHint); // the human decides, so the prose stays
    expect(hoisted.binds).toEqual([]);
  });
});

describe("Settings > Data (whole-store restore; the completion view says exactly what core's report says, matching the CLI)", () => {
  it("when a version chain ran, the completion view shows which version it carried from and to", async () => {
    hoisted.restoreArchive = "/w/backup.amenbo-backup";
    hoisted.restoreReport = restored({ migration: { from: 2, to: 3, applied: ["0003_add_thing"] } });
    await render();
    await act(async () => buttonByLabel(t("settings.restoreBtn"))!.click());
    expect(container.textContent).toContain(tf("settings.restoreDone", { attachments: 0 }));
    expect(container.textContent).toContain(
      tf("settings.restoreMigrated", { from: 2, to: 3, steps: "0003_add_thing" }),
    );
  });

  it("a restore that carried nothing does not claim it did", async () => {
    hoisted.restoreArchive = "/w/backup.amenbo-backup";
    hoisted.restoreReport = restored();
    await render();
    await act(async () => buttonByLabel(t("settings.restoreBtn"))!.click());
    expect(container.textContent).toContain(tf("settings.restoreDone", { attachments: 0 }));
    expect(container.textContent).not.toContain(t("settings.restoreMigrated").slice(0, 12));
  });

  it("the completion view shows the number of attachments written out and where the prior state was set aside", async () => {
    hoisted.restoreArchive = "/w/backup.amenbo-backup";
    const aside = "/w/stores/1/store.pre-restore-20260714T010203Z.sqlite";
    hoisted.restoreReport = restored({ previousSavedTo: aside, blobs: 7 });
    await render();
    await act(async () => buttonByLabel(t("settings.restoreBtn"))!.click());
    expect(container.textContent).toContain(tf("settings.restoreDone", { attachments: 7 }));
    expect(container.textContent).toContain(aside); // the way back the confirmation promised is actually spelled out
  });

  it("a restore with nothing to set aside does not claim a set-aside location", async () => {
    hoisted.restoreArchive = "/w/backup.amenbo-backup";
    hoisted.restoreReport = restored();
    await render();
    await act(async () => buttonByLabel(t("settings.restoreBtn"))!.click());
    expect(container.textContent).not.toContain(t("settings.restoreAside").slice(0, 8));
  });

  it("if it swept old rollback points, it does not silently hide how many it removed", async () => {
    hoisted.restoreArchive = "/w/backup.amenbo-backup";
    hoisted.restoreReport = restored({ previousSavedTo: "/w/aside.sqlite", superseded: 2 });
    await render();
    await act(async () => buttonByLabel(t("settings.restoreBtn"))!.click());
    expect(container.textContent).toContain(tf("settings.restoreSwept", { n: 2 }));
  });

  it("when there are no old rollback points to sweep, it does not say it removed any", async () => {
    hoisted.restoreArchive = "/w/backup.amenbo-backup";
    hoisted.restoreReport = restored({ previousSavedTo: "/w/aside.sqlite" });
    await render();
    await act(async () => buttonByLabel(t("settings.restoreBtn"))!.click());
    expect(container.textContent).not.toContain(t("settings.restoreSwept").slice(0, 8));
  });
});
