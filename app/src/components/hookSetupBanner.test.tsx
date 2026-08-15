// @vitest-environment jsdom
// The banner that says the lint is wired to nothing. Only the boundary is stubbed (core's `setup_notice` scan);
// the banner's own branching and wording run for real — including the part that matters most here, that it waits
// for the modal to finish asking before it reads the disk or says a word.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HookNoticeDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  /** What core's scan reports (the one probe per bound folder). */
  notices: [] as HookNoticeDto[],
  /** How many times `fetchHookNotices` was called — the evidence it happens once, and never before the modal is done. */
  calls: 0,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true, subscribe: () => () => {} };
});
vi.mock("../core/mutations", () => ({
  fetchHookNotices: () => {
    hoisted.calls += 1;
    return Promise.resolve(hoisted.notices);
  },
  // The remaining boundaries the AppShell module (where the banner lives) imports; unused by this test.
  fetchPointerIssues: () => Promise.resolve([]),
  repairPointers: () => Promise.resolve({ repaired: [], unresolved: [] }),
  fetchStaleManagedBlocks: () => Promise.resolve([]),
  fetchOrphanBindings: () => Promise.resolve([]),
  resyncManagedBlocks: () => Promise.resolve({ scanned: 0, updated: [] }),
  forgetOrphanBindings: () => Promise.resolve(0),
  openLatestInstaller: () => Promise.resolve(),
}));

import { HookSetupBanner } from "./HookSetupBanner";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

// core carries no prose — the slots and the command's own name arrive, and the banner words it.
function notice(over: Partial<HookNoticeDto> = {}): HookNoticeDto {
  return {
    projectName: "案件X",
    dir: "/w/案件X",
    cmd: "amenbo",
    unwired: ["pre-commit"],
    restored: [],
    ...over,
  };
}

async function render(asked: boolean) {
  await act(async () => {
    root.render(createElement(HookSetupBanner, { asked }));
  });
}

const lines = () => [...container.querySelectorAll(".healthbanner__line")].map((e) => e.textContent);

beforeEach(() => {
  hoisted.notices = [];
  hoisted.calls = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("lint setup banner", () => {
  // The whole reason the banner takes a prop instead of scanning on mount: it must not talk over the question, and
  // the disk it reports on is the disk answering that question just wrote to.
  it("reads nothing and shows nothing while the modal is still asking", async () => {
    hoisted.notices = [notice()];
    await render(false);

    expect(hoisted.calls, "the probe is not paid before the answers are in").toBe(0);
    expect(container.querySelector(".healthbanner")).toBeNull();
  });

  it("reads the disk once the modal is done, and once only", async () => {
    hoisted.notices = [notice()];
    await render(true);

    expect(hoisted.calls).toBe(1);
    expect(container.querySelector(".healthbanner")).not.toBeNull();
  });

  // A repository with hooks in every slot, and one whose owner said never, both come back from core as nothing to say.
  it("stays silent when core reports nothing", async () => {
    await render(true);
    expect(container.querySelector(".healthbanner")).toBeNull();
  });

  it("names the unwired slots and the command that installs them", async () => {
    hoisted.notices = [notice({ unwired: ["pre-commit", "commit-msg"] })];
    await render(true);

    const line = lines()[0]!;
    expect(line).toContain("案件X");
    expect(line).toContain("/w/案件X");
    expect(line).toContain("pre-commit, commit-msg");
    expect(line).toContain("amenbo hooks install");
  });

  // The dev channel is a different command, and the wording must follow it rather than spell `amenbo` in.
  it("says the command core reported, not a hardcoded name", async () => {
    hoisted.notices = [notice({ cmd: "amenbo-dev" })];
    await render(true);

    expect(lines()[0]).toContain("amenbo-dev hooks install");
    expect(lines()[0]).not.toContain("amenbo hooks install");
  });

  // A block found damaged or stale this session and put back is a warning of its own — the lint had briefly
  // stopped and amenbo repaired itself. It names the slots restored, and does not read as "still unwired".
  it("warns when a block was restored, naming the slots", async () => {
    hoisted.notices = [notice({ unwired: [], restored: ["pre-commit"] })];
    await render(true);

    const alert = container.querySelector<HTMLElement>('.healthbanner[role="alert"]');
    expect(alert, "a restoration is a warning").not.toBeNull();
    const text = alert!.textContent ?? "";
    expect(text).toContain("pre-commit");
    expect(text, "nothing is unwired, so it does not tell them to install").not.toContain("hooks install");
  });

  // Both live at once — one slot still unwired, another whose block was just restored — is two reports, not one.
  it("splits an unwired slot from a restored one into two separate banners", async () => {
    hoisted.notices = [notice({ unwired: ["commit-msg"], restored: ["pre-commit"] })];
    await render(true);

    const banners = [...container.querySelectorAll<HTMLElement>(".healthbanner")];
    expect(banners, "two things to say, two banners").toHaveLength(2);
    const unwired = banners.find((b) => b.textContent?.includes("hooks install"));
    const restored = banners.find((b) => !b.textContent?.includes("hooks install"));
    expect(unwired!.textContent).toContain("commit-msg");
    expect(restored!.textContent).toContain("pre-commit");
  });

  // Two banners, two dismisses: closing one must leave the other. They were sharing one dismiss flag, so a
  // single cross hid both — the bug this pins.
  it("dismisses each banner on its own — closing one leaves the other", async () => {
    hoisted.notices = [notice({ unwired: ["commit-msg"], restored: ["pre-commit"] })];
    await render(true);
    expect(container.querySelectorAll(".healthbanner")).toHaveLength(2);

    // Close the unwired banner (the one that mentions `hooks install`).
    const unwired = [...container.querySelectorAll<HTMLElement>(".healthbanner")].find((b) =>
      b.textContent?.includes("hooks install"),
    )!;
    await act(async () => {
      unwired.querySelector<HTMLButtonElement>(".healthbanner__close")!.click();
    });

    const left = [...container.querySelectorAll<HTMLElement>(".healthbanner")];
    expect(left, "the other banner survives").toHaveLength(1);
    expect(left[0].textContent, "the survivor is the restored one").not.toContain("hooks install");
    expect(left[0].textContent).toContain("pre-commit");
  });

  it("lists every unfinished repository, and the cross dismisses the banner for the session", async () => {
    hoisted.notices = [notice(), notice({ projectName: "案件Y", dir: "/w/案件Y" })];
    await render(true);

    expect(lines()).toHaveLength(2);

    await act(async () => {
      container.querySelector<HTMLButtonElement>(".healthbanner__close")!.click();
    });
    expect(container.querySelector(".healthbanner")).toBeNull();
  });

  // It reports; it never offers to write into git plumbing. That answer is the modal's to take.
  it("offers no install button", async () => {
    hoisted.notices = [notice()];
    await render(true);

    expect(container.querySelector(".healthbanner__action")).toBeNull();
  });
});
