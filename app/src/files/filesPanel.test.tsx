// @vitest-environment jsdom
// What the file face has to get right, none of which is visible in the markup on its own.
//
// It is rooted at the **project's** folder: the rows are asked for with the folder the project is
// bound to, so a pane switching underneath them changes nothing (`AMB-T-3602`). It reads a file by
// asking the host what the file is, rather than deciding from the name — the panel draws Markdown
// as Markdown, and says plainly when there is nothing it can show. And the tree stays folded until
// somebody opens it, one level per opening, because a panel nobody is looking at must not walk a
// repository.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  FolderAppDto, FolderChangedDto, FolderChangesDto, FolderEntryDto, FolderFileDto,
} from "../bindings/bindings";

const ROOT = "/work/repo";

const hoisted = vi.hoisted(() => ({
  asked: [] as string[],
  entries: {} as Record<string, FolderEntryDto[]>,
  file: { truncated: false } as FolderFileDto,
  /** Everyone listening for the host's word. The real event reaches all of them and each takes
   *  what names its own folder, so a stand-in that kept only the last would answer for one section
   *  and drop the news of every other. */
  takers: [] as ((changes: FolderChangesDto) => void)[],
  watching: { root: "", changed: [] as FolderChangedDto[], partial: false, gone: false } as FolderChangesDto,
  /** What one named folder answers with, where a test gives several folders different news. */
  perRoot: {} as Record<string, FolderChangesDto>,
  /** What the host answers when asked what to open a file with — empty where the OS drew it. */
  apps: [] as FolderAppDto[],
  /** The folders the project is bound to. Empty is a project nobody has bound one to yet. */
  bound: [] as { path: string; exists: boolean }[],
}));

vi.mock("./folder", () => ({
  folderWatch: async (projectId: number, root: string): Promise<FolderChangesDto> => {
    hoisted.asked.push(`watch:${projectId}:${root}`);
    return hoisted.perRoot[root] ?? hoisted.watching;
  },
  folderUnwatch: async (root: string) => { hoisted.asked.push(`unwatch:${root}`); },
  onFolderChanged: async (take: (changes: FolderChangesDto) => void) => {
    hoisted.takers.push(take);
    return () => {
      hoisted.takers = hoisted.takers.filter((one) => one !== take);
      hoisted.asked.push("unlisten");
    };
  },
  folderEntries: async (_projectId: number, root: string, path: string[]): Promise<FolderEntryDto[]> => {
    hoisted.asked.push(`entries:${root}:${path.join("/")}`);
    return hoisted.entries[path.join("/")] ?? [];
  },
  folderRead: async (_projectId: number, root: string, path: string[]): Promise<FolderFileDto> => {
    hoisted.asked.push(`read:${root}:${path.join("/")}`);
    return hoisted.file;
  },
  folderOpenFile: async (_projectId: number, root: string, path: string[]) => {
    hoisted.asked.push(`open:${root}:${path.join("/")}`);
  },
  folderRevealFile: async (_projectId: number, root: string, path: string[]) => {
    hoisted.asked.push(`reveal:${root}:${path.join("/")}`);
  },
  folderOpenWith: async (_projectId: number, root: string, path: string[]): Promise<FolderAppDto[]> => {
    hoisted.asked.push(`ask:${root}:${path.join("/")}`);
    return hoisted.apps;
  },
  folderOpenFileWith: async (_projectId: number, root: string, path: string[], app: string) => {
    hoisted.asked.push(`with:${root}:${path.join("/")}:${app}`);
  },
}));

// The folders the project is bound to, answered without a store. `live` is derived the way the real
// read derives it, so a test can bind a folder that is not there.
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: hoisted.bound,
    live: hoisted.bound.filter((one) => one.exists),
    answered: true,
  }),
}));

// What a reference resolves to. The store is not here, and what is under test is what the panel
// does with the answer rather than how the answer is found.
vi.mock("../core/reads", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/reads")>()),
  resolveRef: async () => ({ kind: "task", id: 12, title: "a task", live: true }),
}));

import { FilesPanel } from "./FilesPanel";
import { formatNumber, t, tf } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The host's word that a folder moved, said the way it is said: once, to everyone listening. */
function tell(changes: FolderChangesDto) {
  for (const take of [...hoisted.takers]) take(changes);
}

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

type Props = Parameters<typeof FilesPanel>[0];

async function draw(props: Partial<Props> = {}) {
  await act(async () => {
    const projectId = "projectId" in props ? (props.projectId ?? null) : 1;
    // Which half is up, and the panel's own way out, belong to the terminal face around it
    // (`../shell/TerminalFace`). These tests are about what the files half draws, so the files half
    // is what they hand it.
    root.render(createElement(FilesPanel, {
      tab: "files", onTab: () => {}, onClose: () => {}, ...props, projectId,
    }));
  });
  await settle();
}

/**
 * Pressing something the way a browser does: the pointer goes down first, and only then does the
 * click land. Dispatching the click alone would pass over a menu that closes itself on the way
 * down — which is a menu whose items can never be reached.
 */
function click(el: Element | null | undefined) {
  return act(async () => {
    el?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    el?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** A size as the panel writes one, so the assertion is about the number and not about `Intl`. */
const megabytes = (n: number) =>
  formatNumber(n, { style: "unit", unit: "megabyte", unitDisplay: "short", maximumFractionDigits: 1 });

const button = (text: string) =>
  [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(text));

beforeEach(() => {
  hoisted.asked = [];
  hoisted.entries = {};
  hoisted.file = { truncated: false };
  hoisted.apps = [];
  hoisted.takers = [];
  hoisted.perRoot = {};
  hoisted.watching = {
    root: ROOT,
    changed: [{ path: ["notes", "a.md"], modified: new Date().toISOString() }],
    partial: false,
    gone: false,
  };
  hoisted.bound = [{ path: ROOT, exists: true }];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the file face", () => {
  it("watches the project's folder, not a pane's", async () => {
    await draw();
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    expect(container.textContent).toContain("a.md");
    // The folder the file is in is shown beside it: two files called the same thing are told apart
    // by where they are, and the row shows the name because that is what is read.
    expect(container.textContent).toContain("notes");
  });

  it("draws no switch of its own between the two halves", async () => {
    await draw();
    // The switch is the terminal face's top row, which is reachable while the panel is closed as
    // well. A second one in here would be two controls doing the same thing, and a reader would
    // have to work out which is the right one (`../shell/TerminalFace`).
    expect(container.querySelector("[role=\"tablist\"]")).toBeNull();
    expect(button(t("files.memo"))).toBeUndefined();
    // What did stay is the way out: it ends the panel rather than choosing a half.
    expect(container.querySelector(".files__close")).not.toBeNull();
  });

  it("redraws the row when the host says the folder moved", async () => {
    await draw();
    expect(container.textContent).toContain("a.md");
    await act(async () => {
      tell({
        root: ROOT,
        changed: [{ path: ["src", "main.rs"], modified: new Date().toISOString() }],
        partial: false,
        gone: false,
      });
      await new Promise((r) => setTimeout(r, 0));
    });
    // The list is the host's, whole: what it says now is what is drawn, with nothing merged in.
    expect(container.textContent).toContain("main.rs");
    expect(container.textContent).not.toContain("a.md");
  });

  it("says out loud when only part of the folder is watched", async () => {
    hoisted.watching = { root: ROOT, changed: [], partial: true, gone: false };
    await draw();
    // An unwatched half looks exactly like a half where nothing happened, so it is said rather
    // than left to be assumed (`AMB-T-3604`).
    expect(container.textContent).toContain(t("files.partial"));
  });

  it("leaves another folder's news alone", async () => {
    await draw();
    expect(container.textContent).toContain("a.md");
    await act(async () => {
      tell({
        root: "/work/other",
        changed: [{ path: ["src", "main.rs"], modified: new Date().toISOString() }],
        partial: false,
        gone: false,
      });
      await new Promise((r) => setTimeout(r, 0));
    });
    // Every watched folder is told about through the one listener, and a row rooted at one of them
    // that drew another's list would be saying something untrue about the folder it names.
    expect(container.textContent).toContain("a.md");
    expect(container.textContent).not.toContain("main.rs");
  });

  it("takes its watch down when the face goes away", async () => {
    await draw();
    await act(async () => { root.unmount(); });
    expect(hoisted.asked).toContain(`unwatch:${ROOT}`);
    expect(hoisted.asked).toContain("unlisten");
    // Re-created so afterEach's unmount has something to work on.
    container.remove();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  it("hands a file to the machine rather than trying to be one", async () => {
    await draw();
    const row = button("a.md")!;
    await act(async () => {
      row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 20 }));
    });
    // The face reads and does not edit: what it offers is the reader's own applications.
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:notes/a.md`);

    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.reveal")));
    expect(hoisted.asked).toContain(`reveal:${ROOT}:notes/a.md`);
  });

  it("draws the applications where the machine has no dialog of its own", async () => {
    // macOS: Launch Services only lists, so the list comes back and the menu draws it itself.
    hoisted.apps = [
      { name: "Zed", path: "/Applications/Zed.app", usual: true },
      { name: "MuseScore 4", path: "/Applications/MuseScore 4.app", usual: false },
    ];
    await draw();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.chooseApp")));
    expect(hoisted.asked).toContain(`ask:${ROOT}:notes/a.md`);
    // The menu is still standing, now showing what came back — and the one the file would have
    // opened with anyway says so rather than relying on being first.
    expect(button(tf("files.appUsual", { name: "Zed" }))).toBeDefined();
    expect(button("MuseScore 4")).toBeDefined();
    // The three doors are gone: this is the same menu, not a second one over it.
    expect(button(t("files.reveal"))).toBeUndefined();

    await click(button("MuseScore 4"));
    expect(hoisted.asked).toContain(`with:${ROOT}:notes/a.md:/Applications/MuseScore 4.app`);
  });

  it("steps aside where the machine drew the dialog itself", async () => {
    // Windows and Linux: the chooser was shown, the file is already open, and nothing came back.
    hoisted.apps = [];
    await draw();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.chooseApp")));
    expect(hoisted.asked).toContain(`ask:${ROOT}:notes/a.md`);
    // An empty answer is not an empty list to draw — there is nothing left to ask.
    expect(button(t("files.chooseApp"))).toBeUndefined();
    expect(button(t("files.reveal"))).toBeUndefined();
  });

  it("closes the menu on the next thing the person does", async () => {
    await draw();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeDefined();
    // A menu that outlived the next click would sit over rows it is no longer about — but the press
    // that starts a click on an item is not "the next thing", it is the choosing itself.
    await act(async () => {
      button(t("files.openWith"))!.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeDefined();
    await act(async () => {
      document.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(button(t("files.openWith"))).toBeUndefined();
  });

  it("opens a path clicked in a pane, read against that pane's folder", async () => {
    hoisted.file = { text: "# from the pane", truncated: false };
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 1 } });
    expect(hoisted.asked).toContain(`read:${ROOT}:notes/a.md`);
    expect(container.querySelector("h1")?.textContent).toBe("from the pane");
  });

  it("opens nothing for a path that lands outside the folder this face is rooted at", async () => {
    await draw({ show: { target: "/etc/passwd", cwd: ROOT, nth: 1 } });
    // The pane keeps the characters it drew; no reader is shown a file this face cannot answer for.
    expect(hoisted.asked.some((one) => one.startsWith("read:"))).toBe(false);
  });

  it("opens the same file again when it is clicked again", async () => {
    hoisted.file = { text: "# again", truncated: false };
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 1 } });
    await click(button(t("files.back")));
    await settle();
    expect(container.textContent).toContain(t("files.changed"));

    // The same file asked for a second time is a reader saying "open it" again.
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 2 } });
    expect(container.querySelector("h1")?.textContent).toBe("again");
  });

  it("says a project with no folder has none, rather than drawing empty rows", async () => {
    await draw({ projectId: null });
    expect(container.querySelector(".files__none")?.textContent).toBe(t("files.noFolder"));
    expect(hoisted.asked).toEqual([]);
  });

  it("draws the draft page for a project bound to no folder, which is where the files half stops", async () => {
    hoisted.bound = [];

    await draw({ tab: "memo" });
    expect(container.querySelector("textarea"), "the draft page was not drawn").toBeTruthy();
    expect(container.querySelector(".files__none")).toBeFalsy();

    // The half that has to be rooted somewhere is the only one the missing folder stops.
    await draw({ tab: "files" });
    expect(container.querySelector(".files__none")?.textContent).toBe(t("files.noFolder"));
  });

  it("leaves the tree folded, and opens it one level at a time", async () => {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: "README.md", isDir: false, ignored: false },
    ];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await draw();
    // Folded: nothing has been read about the folder itself yet.
    expect(hoisted.asked.filter((one) => one.startsWith("entries:"))).toEqual([]);

    await click(button(t("files.tree")));
    await settle();
    expect(hoisted.asked).toContain(`entries:${ROOT}:`);
    expect(container.textContent).toContain("README.md");
    // A folder inside it is a name until it is opened — its children cost nothing until then.
    expect(hoisted.asked).not.toContain(`entries:${ROOT}:src`);

    await click(button("src"));
    await settle();
    expect(hoisted.asked).toContain(`entries:${ROOT}:src`);
    expect(container.textContent).toContain("main.rs");
  });

  it("draws a Markdown file as Markdown", async () => {
    hoisted.file = { text: "# A heading", truncated: false };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:notes/a.md`);
    expect(container.querySelector("h1")?.textContent).toBe("A heading");
  });

  it("draws a name the repository ignores, and draws it faintly", async () => {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: ".env", isDir: false, ignored: true },
      { name: ".next", isDir: true, ignored: true },
    ];
    await draw();
    await click(button(t("files.tree")));
    await settle();
    // On the list, because what git does not record is still somebody's file — and faint, because
    // that is the whole of what being ignored says about it (`AMB-D-786`).
    expect(container.textContent).toContain(".env");
    expect(container.querySelector(".files__file--ignored")?.textContent).toContain(".env");
    expect(container.querySelector(".files__dir--ignored")?.textContent).toContain(".next");
    // The one nothing ignores is drawn as it always was.
    expect(container.querySelector(".files__dir:not(.files__dir--ignored)")?.textContent)
      .toContain("src");
  });

  it("draws text that is not Markdown as it was written", async () => {
    hoisted.entries[""] = [{ name: "run.sh", isDir: false, ignored: false }];
    hoisted.file = { text: "#!/bin/sh\necho hi", truncated: false };
    await draw();
    await click(button(t("files.tree")));
    await settle();
    await click(button("run.sh"));
    await settle();
    // Not a heading: the hash in a shell script is a comment, and a name is what decides that.
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector("pre")?.textContent).toBe("#!/bin/sh\necho hi");
  });

  it("says so when the file is not something a panel can show", async () => {
    hoisted.file = { truncated: false };
    await draw();
    await click(button("a.md"));
    await settle();
    // The name says Markdown and the bytes say otherwise; the bytes win (`crate::folder`).
    expect(container.textContent).toContain(t("files.notText"));
  });

  it("draws a picture out of the bytes the host carried", async () => {
    hoisted.file = { truncated: false, image: { mime: "image/png", base64: "AAAA" } };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(container.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AAAA");
  });

  it("says what a picture it would not draw was measured at, and offers the way on", async () => {
    hoisted.file = { truncated: false, oversize: { bytes: 6 * 1024 * 1024, width: 40000, height: 30000 } };
    await draw();
    await click(button("a.md"));
    await settle();
    // A refusal drawn as nothing at all reads as a damaged file, so both numbers travel with it and
    // the reader is pointed at something built to open it (`AMB-D-783`).
    expect(container.textContent).toContain(t("files.tooBig"));
    expect(container.textContent).toContain(megabytes(6));
    expect(container.textContent).toContain(
      tf("files.tooBigPixels", { width: formatNumber(40000), height: formatNumber(30000) }),
    );
    // Not the sentence for a file there is nothing to show of: this one is a picture, and it is
    // being refused rather than failed to be read.
    expect(container.textContent).not.toContain(t("files.notText"));

    await click(button(t("files.tooBigOpen")));
    // The way on is the one the list rows already open — nothing new was invented for this state.
    expect(button(t("files.chooseApp"))).toBeDefined();
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:notes/a.md`);
  });

  it("refuses on bytes alone where the picture would not say its size", async () => {
    hoisted.file = { truncated: false, oversize: { bytes: 6 * 1024 * 1024 } };
    await draw();
    await click(button("a.md"));
    await settle();
    // A size nobody could read is not printed as a guess (`crate::folder`).
    expect(container.textContent).toContain(megabytes(6));
    expect(container.textContent).not.toContain("×");
  });

  it("writes a refused picture's size in a unit that says something about it", async () => {
    // The pictures that cost the most to draw are the ones that compress best, so a refusal on
    // pixels alone is commonly a file of a few kilobytes. Rounded to megabytes it would read "0 MB"
    // — the file said to be empty, which is the opposite of what it is (`AMB-D-783`).
    hoisted.file = { truncated: false, oversize: { bytes: 10 * 1024, width: 16000, height: 16000 } };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(container.textContent).toContain(
      formatNumber(10, { style: "unit", unit: "kilobyte", unitDisplay: "short", maximumFractionDigits: 0 }),
    );
  });

  it("goes all the way down to bytes rather than round a refused picture to nothing", async () => {
    // A header alone is the cheapest thing that can claim thirty thousand square, and it is under a
    // kilobyte. Rounded up a unit it would read "0 kB" — the same lie one unit further down.
    hoisted.file = { truncated: false, oversize: { bytes: 33, width: 30000, height: 30000 } };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(container.textContent).toContain(
      formatNumber(33, { style: "unit", unit: "byte", unitDisplay: "short", maximumFractionDigits: 0 }),
    );
  });

  it("says out loud when only the head of a long file is shown", async () => {
    hoisted.file = { text: "x".repeat(10), truncated: true };
    await draw();
    await click(button("a.md"));
    await settle();
    expect(container.textContent).toContain(t("files.cut"));
  });

  it("leaves this face when a reference in a file is followed", async () => {
    const left: number[] = [];
    hoisted.file = { text: "see AMB-T-12", truncated: false };
    await draw({ onOpenLedger: () => left.push(1) });
    await click(button("a.md"));
    await settle();
    // A reference selects on the other face. Following one from here without leaving would land on
    // a pane the reader cannot see — a link that looks alive and is not (`AMB-D-747`).
    const ref = [...container.querySelectorAll("a")].find((a) => a.textContent === "AMB-T-12");
    expect(ref).toBeDefined();
    await click(ref);
    expect(left).toEqual([1]);
  });
});

describe("a project bound to several folders", () => {
  const OTHER = "/work/plugins";
  const both = () => { hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: true }]; };

  it("watches every one of them, not the first", async () => {
    both();
    await draw();
    // The first was never chosen — it was whichever sorted first — and the rest of the project was
    // invisible because of it (`AMB-D-778`).
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    expect(hoisted.asked).toContain(`watch:1:${OTHER}`);
  });

  it("names each one, and names none where there is only one to name", async () => {
    both();
    await draw();
    const headings = [...container.querySelectorAll(".files__foldername")];
    expect(headings.map((one) => one.textContent)).toEqual(["plugins", "repo"]);

    hoisted.bound = [{ path: ROOT, exists: true }];
    await draw();
    // One folder is drawn the way it always was: a heading over the only thing on the screen names
    // nothing the reader could confuse it with.
    expect(container.querySelectorAll(".files__foldername")).toHaveLength(0);
  });

  it("gives each folder its own news", async () => {
    both();
    hoisted.perRoot = {
      [ROOT]: { root: ROOT, changed: [{ path: ["here.md"], modified: new Date().toISOString() }], partial: false, gone: false },
      [OTHER]: { root: OTHER, changed: [{ path: ["there.md"], modified: new Date().toISOString() }], partial: false, gone: false },
    };
    await draw();
    expect(button("here.md")).toBeDefined();
    expect(button("there.md")).toBeDefined();
  });

  it("reads a file out of the folder its row was drawn in", async () => {
    both();
    hoisted.perRoot = {
      [ROOT]: { root: ROOT, changed: [], partial: false, gone: false },
      [OTHER]: { root: OTHER, changed: [{ path: ["there.md"], modified: new Date().toISOString() }], partial: false, gone: false },
    };
    hoisted.file = { text: "hello", truncated: false };
    await draw();
    await click(button("there.md"));
    await settle();
    // The same path names a different file in each folder, so which folder the row was in has to
    // travel with it.
    expect(hoisted.asked).toContain(`read:${OTHER}:there.md`);
  });

  it("keeps a folder that has gone, and says that is what happened", async () => {
    hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: false }];
    await draw();
    // Dropped from the list it would look like a binding nobody ever made, and a reader would have
    // no way to tell a folder that moved from one they unbound themselves.
    expect(container.textContent).toContain(t("files.folderGone"));
    expect(hoisted.asked).not.toContain(`watch:1:${OTHER}`);
  });

  it("says so when a folder goes while it is being looked at", async () => {
    both();
    await draw();
    expect(container.textContent).not.toContain(t("files.folderGone"));
    await act(async () => {
      tell({ root: OTHER, changed: [], partial: false, gone: true });
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(container.textContent).toContain(t("files.folderGone"));
  });
});
