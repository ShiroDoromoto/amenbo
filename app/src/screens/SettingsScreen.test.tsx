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
  /** What `fetchDevBadge` answers — the badge text on a development build, null on a shipped one. */
  devBadge: null as string | null,
  /** Every view `setDefaultView` was asked to write, in the order the pull-down asked for them. */
  defaultViews: [] as string[],
  /** Every answer the tick row wrote, in order (`true` is a yes). */
  tickAnswers: [] as boolean[],
  /** What `answerTick` should refuse with, or null to let it land. */
  tickFails: null as string | null,
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
    setAutostart: noop,
    setDefaultView: (view: string) => { hoisted.defaultViews.push(view); return Promise.resolve(); },
    answerTick: (yes: boolean) => {
      if (hoisted.tickFails) return Promise.reject(new Error(hoisted.tickFails));
      hoisted.tickAnswers.push(yes);
      return Promise.resolve();
    },
    fetchDevBadge: () => Promise.resolve(hoisted.devBadge),
  };
});

import { SettingsScreen } from "./SettingsScreen";
import { applySnapshot, getSnapshot } from "../core/snapshot";
import { doctorText, t, tn, tf } from "../core/i18n";

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

/** The section headings, as a reader meets them going down the screen. Read as elements rather than
 *  searched for in the page text, since one label is a prefix of another in some languages. */
function sectionTitles(): string[] {
  return [...container.querySelectorAll(".settings__h")].map((h) => h.textContent ?? "");
}

/** The label of every setting row, read the same way and for the same reason. */
function rowLabels(): string[] {
  return [...container.querySelectorAll(".settings__k")].map((k) => k.textContent ?? "");
}

/** The pull-down of the row carrying this label — reached through the row, since the screen draws
 *  several selects and their options are the only thing telling them apart. */
function selectInRow(label: string): HTMLSelectElement {
  const row = [...container.querySelectorAll(".settings__row")]
    .find((r) => r.querySelector(".settings__k")?.textContent === label);
  const sel = row?.querySelector("select");
  if (!sel) throw new Error(`no select in the row labelled ${label}`);
  return sel as HTMLSelectElement;
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
  hoisted.devBadge = null;
  hoisted.defaultViews = [];
  hoisted.tickAnswers = [];
  hoisted.tickFails = null;
  applySnapshot({ ...getSnapshot(), tickConsent: null, tickRemovalLeavesARow: false });
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
    expect(container.textContent).toContain(tn("settings.restoreSwept", 2));
  });

  it("when there are no old rollback points to sweep, it does not say it removed any", async () => {
    hoisted.restoreArchive = "/w/backup.amenbo-backup";
    hoisted.restoreReport = restored({ previousSavedTo: "/w/aside.sqlite" });
    await render();
    await act(async () => buttonByLabel(t("settings.restoreBtn"))!.click());
    // A prefix of the sentence, so this catches it whichever arm the count would have picked.
    expect(container.textContent).not.toContain(tn("settings.restoreSwept", 0).slice(0, 30));
  });
});

describe("Settings > Startup (a control only a shipped build can honour)", () => {
  it("a shipped build is offered the switch, under its own section", async () => {
    await render();
    expect(sectionTitles()).toContain(t("settings.startup"));
    expect(rowLabels()).toContain(t("settings.autostart"));
  });

  it("a development build gets neither the switch nor an empty section over it", async () => {
    hoisted.devBadge = "DEV";
    await render();
    // Such a build registers nothing at login, so the switch would be an offer it cannot keep.
    expect(rowLabels()).not.toContain(t("settings.autostart"));
    expect(sectionTitles()).not.toContain(t("settings.startup"));
    // The sections around it stay, so this is one section leaving and not the screen giving up.
    expect(rowLabels()).toContain(t("settings.perfLog"));
    expect(rowLabels()).toContain(t("settings.dataPath"));
  });
});

describe("Settings > Updates (a control only a shipped build can honour)", () => {
  it("a shipped build is offered the switch, under its own section", async () => {
    await render();
    expect(sectionTitles()).toContain(t("settings.updates"));
    expect(rowLabels()).toContain(t("settings.updateCheck"));
  });

  it("a development build gets neither the switch nor an empty section over it", async () => {
    hoisted.devBadge = "DEV";
    await render();
    expect(rowLabels()).not.toContain(t("settings.updateCheck"));
    // The heading goes with it: a section whose only control is gone is a title over nothing.
    expect(sectionTitles()).not.toContain(t("settings.updates"));
    // The sections around it stay, so this is one section leaving and not the screen giving up.
    expect(rowLabels()).toContain(t("settings.perfLog"));
    expect(rowLabels()).toContain(t("settings.dataPath"));
  });
});

describe("Settings > Appearance (the view a project created without one opens in)", () => {
  it("stands at the value config holds, and writes the one that is chosen", async () => {
    await render();
    expect(rowLabels()).toContain(t("settings.defaultView"));
    const pick = selectInRow(t("settings.defaultView"));
    // Core's own default, as the empty snapshot carries it — the row reads config rather than
    // starting from a value of its own.
    expect(pick.value).toBe("board");
    // All four, so the row cannot quietly offer a subset of what `config set default_view` takes.
    expect([...pick.options].map((o) => o.value)).toEqual(["list", "board", "calendar", "timeline"]);

    await act(async () => {
      pick.value = "calendar";
      pick.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.defaultViews).toEqual(["calendar"]);
  });
});

// The way back from a "don't show this again" on the band (`AMB-D-718`): once the question is answered
// the band never returns, so this row is the only place the answer can still be moved.
describe("Settings > due warnings (the hourly tick)", () => {
  /** Put an answer on record the way a snapshot carries it, and redraw. */
  async function withConsent(consent: string | null, leavesARow = false) {
    applySnapshot({ ...getSnapshot(), tickConsent: consent, tickRemovalLeavesARow: leavesARow });
    await render();
  }

  async function choose(value: "on" | "off") {
    const pick = selectInRow(t("settings.tick"));
    await act(async () => {
      pick.value = value;
      pick.dispatchEvent(new Event("change", { bubbles: true }));
    });
  }

  // Two positions over three states. Never having answered is not a setting to sit in — what it means
  // on the machine is that no timer is held, which is what "off" already says.
  it("stands where the answer on record puts it, and reads an unanswered device as off", async () => {
    await withConsent("yes");
    expect(selectInRow(t("settings.tick")).value).toBe("on");

    await withConsent("no");
    expect(selectInRow(t("settings.tick")).value).toBe("off");

    await withConsent(null);
    expect(selectInRow(t("settings.tick")).value).toBe("off");
  });

  it("writes the answer that was chosen, each way", async () => {
    await withConsent("no");
    await choose("on");
    expect(hoisted.tickAnswers).toEqual([true]);

    await withConsent("yes");
    await choose("off");
    expect(hoisted.tickAnswers).toEqual([true, false]);
  });

  // A development build draws this one, unlike the two rows above it: nothing is registered here that
  // was not asked for, so the build that was asked has to be able to take it back.
  it("is drawn on a development build too", async () => {
    hoisted.devBadge = "DEV";
    await withConsent(null);
    expect(rowLabels()).toContain(t("settings.tick"));
    expect(sectionTitles()).toContain(t("settings.dueWarning"));
    // …while the two that are withheld from that channel are still withheld.
    expect(rowLabels()).not.toContain(t("settings.autostart"));
    expect(rowLabels()).not.toContain(t("settings.updateCheck"));
  });

  // macOS keeps its own record of the row, and `unregister` does not reach it. Said here for the reason
  // `tick uninstall` says it: unsaid, the row still sitting in login items reads as a failed removal.
  it("says the row outlives the removal, where the OS keeps one", async () => {
    await withConsent("yes", true);
    await choose("off");
    expect(container.textContent).toContain(t("settings.tickRowRemains"));
  });

  it("says nothing of the sort where the removal takes the row with it", async () => {
    await withConsent("yes", false);
    await choose("off");
    expect(container.textContent).not.toContain(t("settings.tickRowRemains"));
  });

  // Switching on is one act with the registration, so a scheduler that refused must leave the row where
  // it was rather than reading as a timer nobody is holding.
  it("keeps the reason on the row when the scheduler refused", async () => {
    hoisted.tickFails = "the scheduler would not take it";
    await withConsent(null);
    await choose("on");

    expect(container.textContent).toContain("the scheduler would not take it");
    expect(hoisted.tickAnswers).toEqual([]);
    expect(selectInRow(t("settings.tick")).value, "the answer on record has not moved").toBe("off");
  });
});
