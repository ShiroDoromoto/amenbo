// @vitest-environment jsdom
// What the file face has to get right, none of which is visible in the markup on its own.
//
// It is rooted at the **project's** folder: the rows are asked for with the folder the project is
// bound to, so a pane switching underneath them changes nothing (`AMB-T-3602`). It reads a file by
// asking the host what the file is, rather than deciding from the name — the panel draws Markdown
// as Markdown, and says plainly when there is nothing it can show. And the tree stays folded until
// somebody opens it, one level per opening, because a panel nobody is looking at must not walk a
// repository.
//
// **What the folder moving does is send everybody back to ask** (`AMB-D-785`). The host's word
// carries no rows, so what has to be right here is that the names of the open level and the colour
// beside them are read again — and that a word about another folder moves nothing in this one.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  DropEffectDto, FolderAppDto, FolderCarriedDto, FolderChangesDto, FolderEntryDto, FolderFileDto,
  GitEntryDto,
} from "../bindings/bindings";

const ROOT = "/work/repo";

/** The most recent of what the editor was asked to draw. */
const last = <T,>(all: T[]): T | undefined => all[all.length - 1];

/** What the host answers `folder_read` with, filled in around whatever a test cares about. */
const aFile = (about: Partial<FolderFileDto> = {}): FolderFileDto => ({
  truncated: false,
  bom: false,
  lineEnding: "lf",
  clean: true,
  ...about,
});

const hoisted = vi.hoisted(() => ({
  asked: [] as string[],
  entries: {} as Record<string, FolderEntryDto[]>,
  // Spelled out rather than built by `aFile`: `vi.hoisted` runs before this module's own bindings
  // exist. Every test replaces it in `beforeEach` anyway.
  file: { truncated: false, bom: false, lineEnding: "lf", clean: true } as FolderFileDto,
  /** Everyone listening for the host's word. The real event reaches all of them and each takes
   *  what names its own folder, so a stand-in that kept only the last would answer for one section
   *  and drop the news of every other. */
  takers: [] as ((changes: FolderChangesDto) => void)[],
  watching: { root: "", capped: false, unwatched: false, gone: false } as FolderChangesDto,
  /** What one named folder answers with, where a test gives several folders different news. */
  perRoot: {} as Record<string, FolderChangesDto>,
  /** What git says about each folder, by the folder it is about. */
  git: {} as Record<string, GitEntryDto[]>,
  /** What the host answers when asked what to open a file with — empty where the OS drew it. */
  apps: [] as FolderAppDto[],
  /** The folders the project is bound to. Empty is a project nobody has bound one to yet. */
  bound: [] as { path: string; exists: boolean }[],
  /** The host's side of the drag-and-drop subscription (`../core/hostDrop`). */
  dragging: null as null | ((event: { payload: unknown }) => void),
  /** What the editor was asked to draw, and whether it was allowed to be typed into. */
  editing: [] as { text: string; editable: boolean; name: string }[],
  /** What the host answers about a row it was asked to bin — a test makes one stop by filling it. */
  trashed: null as null | { gone: string[]; stopped: { name: string; why: string } | null },
  /** What comes back out of the bin. `null` is the host saying there is nothing left to undo. */
  restored: null as null | { back: string[]; stopped: { name: string; why: string } | null },
  /** Every carry the panel asked the host for, as it asked for it. */
  imported: [] as
    { paths: string[]; toRoot: string; to: string[]; effect: DropEffectDto }[],
  /** What the host answers a carry with — the whole list arriving, unless a test says otherwise. */
  carried: { arrived: [] as string[], stopped: null } as FolderCarriedDto,
  /** What the host refuses a name with, where a test is about the refusal. */
  refuse: null as unknown,
  /** Whether that refusal takes a moment to arrive — the ordering a real host has and a stand-in
   *  that throws on the spot does not. */
  slowRefusal: false,
}));

// The editor is loaded on demand and lays itself out by measuring, which jsdom cannot do — so what
// it was asked to draw is recorded instead, the same stand-in the Markdown face makes for mermaid.
vi.mock("./editorLoad", () => ({
  mountEditor: async (parent: HTMLElement, text: string, editable: boolean, name: string) => {
    hoisted.editing.push({ text, editable, name });
    const drawn = parent.ownerDocument.createElement("div");
    drawn.className = "cm-editor";
    drawn.textContent = text;
    parent.appendChild(drawn);
    return {
      show(next: string) { drawn.textContent = next; },
      close() { drawn.remove(); },
    };
  },
}));

// A file dragged in from the desktop reaches the application, and the page hears about it through
// this one event (`AMB-D-775`). It is the host's, so the test plays the host.
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (take: (event: { payload: unknown }) => void) => {
      hoisted.dragging = take;
      return () => { hoisted.dragging = null; };
    },
  }),
}));

vi.mock("./folder", () => ({
  folderWatch: async (projectId: number, root: string): Promise<FolderChangesDto> => {
    hoisted.asked.push(`watch:${projectId}:${root}`);
    return hoisted.perRoot[root] ?? hoisted.watching;
  },
  folderUnwatch: async (root: string) => { hoisted.asked.push(`unwatch:${root}`); },
  folderGitStatus: async (_projectId: number, root: string): Promise<GitEntryDto[]> => {
    hoisted.asked.push(`git:${root}`);
    return hoisted.git[root] ?? [];
  },
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
  folderTrash: async (_projectId: number, root: string, paths: string[][]) => {
    hoisted.asked.push(`trash:${root}:${paths.map((one) => one.join("/")).join(",")}`);
    return hoisted.trashed ?? { gone: paths.map((one) => one[one.length - 1] ?? ""), stopped: null };
  },
  folderUntrash: async () => {
    hoisted.asked.push("untrash");
    return hoisted.restored;
  },
  folderImport: async (
    _projectId: number,
    paths: string[],
    toRoot: string,
    to: string[],
    effect: DropEffectDto,
  ): Promise<FolderCarriedDto> => {
    hoisted.imported.push({ paths, toRoot, to, effect });
    return hoisted.carried;
  },
  folderMake: async (_projectId: number, root: string, path: string[], dir: boolean) => {
    hoisted.asked.push(`make:${root}:${path.join("/")}:${dir ? "dir" : "file"}`);
    if (hoisted.refuse === null) return;
    if (hoisted.slowRefusal) await new Promise((r) => setTimeout(r, 5));
    throw hoisted.refuse;
  },
  folderRename: async (_projectId: number, root: string, path: string[], name: string) => {
    hoisted.asked.push(`rename:${root}:${path.join("/")}:${name}`);
    if (hoisted.refuse !== null) throw hoisted.refuse;
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
import { type CmdError, errLabel, formatNumber, t, tf } from "../core/i18n";
import { subscribeNotice } from "../core/notice";

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

/** The same, over the whole page: the question before the bin is drawn onto `document.body`. */
const anyButton = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(text));

/** Press undo where the panel hears it: on the panel, not on the window (`AMB-D-780`). */
const undo = () => act(async () => {
  container.querySelector(".files")!.dispatchEvent(
    new KeyboardEvent("keydown", { key: "z", metaKey: true, bubbles: true }),
  );
  await new Promise((r) => setTimeout(r, 0));
});

/** The menu, opened on a row the way a person opens it. */
const menuOn = (el: Element | null | undefined) => act(async () => {
  el?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
  await new Promise((r) => setTimeout(r, 0));
});

/** The box a name is typed into, while one is being typed. */
const namebox = () => container.querySelector<HTMLInputElement>(".files__namebox");

/** Typing into it the way a person does: React hears the `input` event, not an assigned `value`. */
const type = (box: HTMLInputElement, text: string) => act(async () => {
  // The prototype is taken from the box's own window: React replaces the setter on the instance, and
  // this module's `HTMLInputElement` is not the one jsdom built the element from.
  const proto = box.ownerDocument.defaultView?.HTMLInputElement.prototype;
  const set = proto && Object.getOwnPropertyDescriptor(proto, "value")?.set;
  set?.call(box, text);
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

const press = (el: Element, key: string) => act(async () => {
  el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

/** Leaving the box, which is the other of the two ways a name is kept. `focusout` and not `blur`:
 *  React listens for the one that bubbles, and a `blur` dispatched here reaches no handler at all. */
const leave = (el: Element) => act(async () => {
  el.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

/**
 * The panel with the folder's own section unfolded, which is where every row is.
 *
 * The tree is folded until somebody opens it, and a test about what a row does has to get past that
 * first — so the opening is written once here rather than at the top of every one of them.
 */
async function drawOpen(props: Partial<Props> = {}) {
  await draw(props);
  await click(button(t("files.tree")));
  await settle();
}

beforeEach(() => {
  hoisted.asked = [];
  hoisted.editing = [];
  // One file in the folder, so a test that only wants a row to press has one without saying so.
  hoisted.entries = { "": [{ name: "a.md", isDir: false, ignored: false }] };
  hoisted.file = aFile();
  hoisted.apps = [];
  hoisted.refuse = null;
  hoisted.slowRefusal = false;
  hoisted.takers = [];
  hoisted.perRoot = {};
  hoisted.git = {};
  hoisted.watching = { root: ROOT, capped: false, unwatched: false, gone: false };
  hoisted.bound = [{ path: ROOT, exists: true }];
  hoisted.dragging = null;
  hoisted.trashed = null;
  hoisted.restored = { back: ["a.md"], stopped: null };
  // The question before a row goes to the bin is remembered per device, so a test that turns it off
  // would turn it off for the next one (`./askBeforeTrash`).
  localStorage.clear();
  hoisted.imported = [];
  hoisted.carried = { arrived: [], stopped: null };
  // Inside Tauri as far as the panel is concerned; without it there is no host to hear a drop from.
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe("the file face", () => {
  it("watches the project's folder, not a pane's", async () => {
    await drawOpen();
    expect(hoisted.asked).toContain(`watch:1:${ROOT}`);
    // And git is asked about the same folder, which is where the colour on each row comes from.
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(hoisted.asked).toContain(`entries:${ROOT}:`);
    expect(container.textContent).toContain("a.md");
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

  it("goes and asks again when the host says the folder moved", async () => {
    await drawOpen();
    expect(container.textContent).toContain("a.md");
    hoisted.entries[""] = [{ name: "main.rs", isDir: false, ignored: false }];
    hoisted.git[ROOT] = [{ path: ["main.rs"], index: " ", worktree: "M", isDir: false }];
    hoisted.asked = [];

    await act(async () => {
      tell({ root: ROOT, capped: false, unwatched: false, gone: false });
      await new Promise((r) => setTimeout(r, 0));
    });
    // The word carries nothing, so both readers go back to the host: the names of the level that is
    // open, and what git says about them (`AMB-D-785`).
    expect(hoisted.asked).toContain(`entries:${ROOT}:`);
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(container.textContent).toContain("main.rs");
    expect(container.textContent).not.toContain("a.md");
    expect(container.querySelector(".files__file--git-modified")?.textContent).toContain("main.rs");
  });

  it("says out loud when the folder was too big to look through all of", async () => {
    hoisted.watching = { root: ROOT, capped: true, unwatched: false, gone: false };
    await draw();
    // An unwatched half looks exactly like a half where nothing happened, so it is said rather
    // than left to be assumed (`AMB-T-3604`).
    expect(container.textContent).toContain(t("files.capped"));
    // And it is not the other reason: the folder's size is what stopped the walk, and the machine
    // has watches to spare (`AMB-D-778`).
    expect(container.textContent).not.toContain(t("files.unwatched"));
  });

  it("says out loud when the machine ran out of watches, and how to get more", async () => {
    hoisted.watching = { root: ROOT, capped: false, unwatched: true, gone: false };
    await draw();
    expect(container.textContent).toContain(t("files.unwatched"));
    // The fact alone reads as something the reader did to their own project. What they can act on
    // is the machine's supply, so the way out is drawn with it.
    expect(container.textContent).toContain(t("files.unwatchedHow"));
    expect(container.textContent).not.toContain(t("files.capped"));
  });

  it("says both when both are true", async () => {
    hoisted.watching = { root: ROOT, capped: true, unwatched: true, gone: false };
    await draw();
    // Two separate things have happened to one folder, and folding them into whichever was noticed
    // first would leave the other half of the story untold.
    expect(container.textContent).toContain(t("files.capped"));
    expect(container.textContent).toContain(t("files.unwatched"));
  });

  it("leaves another folder's news alone", async () => {
    await drawOpen();
    hoisted.entries[""] = [{ name: "main.rs", isDir: false, ignored: false }];
    hoisted.asked = [];
    await act(async () => {
      tell({ root: "/work/other", capped: false, unwatched: false, gone: false });
      await new Promise((r) => setTimeout(r, 0));
    });
    // Every watched folder is told about through the one listener, so a section that asked again on
    // somebody else's news would be reading the disk for every folder every time any of them moved.
    expect(hoisted.asked).toEqual([]);
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
    await drawOpen();
    const row = button("a.md")!;
    await act(async () => {
      row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 20 }));
    });
    // There is an editor here, and still a way out of it: what a person wants of a file is as often
    // to hand it to something else as to read it where it is.
    await click(button(t("files.openWith")));
    expect(hoisted.asked).toContain(`open:${ROOT}:a.md`);

    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.reveal")));
    expect(hoisted.asked).toContain(`reveal:${ROOT}:a.md`);
  });

  it("draws the applications where the machine has no dialog of its own", async () => {
    // macOS: Launch Services only lists, so the list comes back and the menu draws it itself.
    hoisted.apps = [
      { name: "Zed", path: "/Applications/Zed.app", usual: true },
      { name: "MuseScore 4", path: "/Applications/MuseScore 4.app", usual: false },
    ];
    await drawOpen();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.chooseApp")));
    expect(hoisted.asked).toContain(`ask:${ROOT}:a.md`);
    // The menu is still standing, now showing what came back — and the one the file would have
    // opened with anyway says so rather than relying on being first.
    expect(button(tf("files.appUsual", { name: "Zed" }))).toBeDefined();
    expect(button("MuseScore 4")).toBeDefined();
    // The three doors are gone: this is the same menu, not a second one over it.
    expect(button(t("files.reveal"))).toBeUndefined();

    await click(button("MuseScore 4"));
    expect(hoisted.asked).toContain(`with:${ROOT}:a.md:/Applications/MuseScore 4.app`);
  });

  it("steps aside where the machine drew the dialog itself", async () => {
    // Windows and Linux: the chooser was shown, the file is already open, and nothing came back.
    hoisted.apps = [];
    await drawOpen();
    await act(async () => {
      button("a.md")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    await click(button(t("files.chooseApp")));
    expect(hoisted.asked).toContain(`ask:${ROOT}:a.md`);
    // An empty answer is not an empty list to draw — there is nothing left to ask.
    expect(button(t("files.chooseApp"))).toBeUndefined();
    expect(button(t("files.reveal"))).toBeUndefined();
  });

  it("closes the menu on the next thing the person does", async () => {
    await drawOpen();
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
    hoisted.file = aFile({ text: "# from the pane" });
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
    hoisted.file = aFile({ text: "# again" });
    await draw({ show: { target: "notes/a.md", cwd: ROOT, nth: 1 } });
    await click(button(t("files.back")));
    await settle();
    expect(container.textContent).toContain(t("files.tree"));

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

  // The host says where the pointer is and nothing else, so which folder a file would land in is
  // the panel's own answer — and the answer a reader can see before they let go (`AMB-D-775`).
  it("marks the folder a file dragged in from the desktop would land in", async () => {
    hoisted.entries[""] = [
      { name: "src", isDir: true, ignored: false },
      { name: "README.md", isDir: false, ignored: false },
    ];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await draw();
    await click(button(t("files.tree")));
    await settle();
    await click(button("src"));
    await settle();

    // jsdom lays nothing out, so what is under the point is stated. What is being read is the walk
    // up from it: a file row belongs to the folder holding it, so hanging over `main.rs` is hanging
    // over `src` — the same folder as hanging over its name.
    // Each move is at a point of its own: a move that repeats the last one is dropped on the way in,
    // because macOS sends the same point twice while the drag stands still (`../core/hostDrop`).
    let step = 0;
    const over = (el: Element | null | undefined) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => el ?? null;
      step += 1;
      hoisted.dragging?.({ payload: { type: "over", position: { x: step, y: 1 } } });
      await new Promise((r) => setTimeout(r, 0));
    });
    const marked = () => container.querySelector(".files__into")?.getAttribute("data-into");

    await over(button("main.rs"));
    expect(marked()).toBe("src");

    // A row belonging to no folder in the tree belongs to the tree, which is the root itself.
    await over(button("README.md"));
    expect(marked()).toBeUndefined();
    expect(container.querySelector(".files__row--into")?.getAttribute("data-into")).toBe("");

    // Nothing under the pointer is nothing marked: a highlight left standing would name a folder
    // the reader had already dragged away from.
    await over(null);
    expect(container.querySelector(".files__into, .files__row--into")).toBeNull();
  });

  // The highlight said which folder; letting go is the panel carrying the files into that one and no
  // other. Both halves of the landing travel, because the same path inside two bound folders is two
  // places (`AMB-T-3781`).
  it("carries what was dropped into the folder the highlight named", async () => {
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    await draw();
    await click(button(t("files.tree")));
    await settle();
    await click(button("src"));
    await settle();

    const drop = (el: Element | null | undefined, paths: string[]) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => el ?? null;
      hoisted.dragging?.({ payload: { type: "drop", position: { x: 1, y: 1 }, paths } });
      await new Promise((r) => setTimeout(r, 0));
    });

    await drop(button("main.rs"), ["/Users/someone/Desktop/note.md"]);
    expect(hoisted.imported).toEqual([{
      paths: ["/Users/someone/Desktop/note.md"],
      toRoot: ROOT,
      to: ["src"],
      // Neither modifier was held, and the host reads that as neither: what a plain drop means is
      // decided where the carry is made, and it copies.
      effect: "default",
    }]);
    // And the highlight is gone the moment the files are let go.
    expect(container.querySelector(".files__into, .files__row--into")).toBeNull();

    // The bound folder itself is a landing like any other, and its path is no segments at all.
    await drop(container.querySelector("[data-into=\"\"]"), ["/Users/someone/Desktop/other.md"]);
    expect(hoisted.imported[1]?.to).toEqual([]);
  });

  // Nothing is said about what arrived — the folder is watched and is about to list it. What stops
  // has nothing drawing it, so that is what is said, and it says how far the carry got.
  it("says what a carry stopped on, and how much of it had already arrived", async () => {
    await draw();
    const said: string[] = [];
    const stop = subscribeNotice((line) => said.push(line));
    const drop = (paths: string[]) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => container.querySelector("[data-into]");
      hoisted.dragging?.({ payload: { type: "drop", position: { x: 1, y: 1 }, paths } });
      await new Promise((r) => setTimeout(r, 0));
    });

    hoisted.carried = { arrived: [], stopped: { name: "note.md", why: "no room left" } };
    await drop(["/a/note.md"]);
    expect(said).toEqual([tf("files.dropStopped", { name: "note.md", why: "no room left" })]);

    hoisted.carried = { arrived: ["one.md"], stopped: { name: "note.md", why: "no room left" } };
    await drop(["/a/one.md", "/a/note.md"]);
    expect(said[1]).toBe(
      tf("files.dropPartly", { name: "note.md", why: "no room left", count: formatNumber(1) }),
    );

    // A carry that got the whole way through says nothing at all.
    hoisted.carried = { arrived: ["one.md"], stopped: null };
    await drop(["/a/one.md"]);
    expect(said).toHaveLength(2);
    stop();
  });

  it("draws a Markdown file as Markdown", async () => {
    hoisted.file = aFile({ text: "# A heading" });
    await drawOpen();
    await click(button("a.md"));
    await settle();
    expect(hoisted.asked).toContain(`read:${ROOT}:a.md`);
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

  it("wears what git says about each row, and nothing where git says nothing", async () => {
    hoisted.entries[""] = [
      { name: "changed.md", isDir: false, ignored: false },
      { name: "staged.md", isDir: false, ignored: false },
      { name: "new.md", isDir: false, ignored: false },
      { name: "same.md", isDir: false, ignored: false },
    ];
    hoisted.git[ROOT] = [
      { path: ["changed.md"], index: " ", worktree: "M", isDir: false },
      { path: ["staged.md"], index: "A", worktree: " ", isDir: false },
      { path: ["new.md"], index: "?", worktree: "?", isDir: false },
    ];
    await drawOpen();
    const marked = (mark: string) =>
      container.querySelector(`.files__file--git-${mark}`)?.textContent;
    expect(marked("modified")).toContain("changed.md");
    expect(marked("added")).toContain("staged.md");
    expect(marked("untracked")).toContain("new.md");
    // The row git said nothing about is the row nothing is said about: no colour is an answer.
    const plain = [...container.querySelectorAll(".files__file")]
      .find((one) => one.textContent === "same.md");
    expect(plain?.className).toBe("files__file");
  });

  it("colours what is inside a folder git named as a whole", async () => {
    hoisted.entries[""] = [{ name: "fresh", isDir: true, ignored: false }];
    hoisted.entries["fresh"] = [{ name: "one.md", isDir: false, ignored: false }];
    // What git does with an untracked folder: it names the folder and stops. A tree that matched
    // paths exactly would leave every file in a brand-new folder colourless.
    hoisted.git[ROOT] = [{ path: ["fresh"], index: "?", worktree: "?", isDir: true }];
    await drawOpen();
    expect(container.querySelector(".files__dir--git-untracked")?.textContent).toContain("fresh");
    await click(button("fresh"));
    await settle();
    expect(container.querySelector(".files__file--git-untracked")?.textContent).toContain("one.md");
  });

  it("colours a folded folder for what is under it, and lets it go plain when it opens", async () => {
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    hoisted.entries["src"] = [{ name: "main.rs", isDir: false, ignored: false }];
    // git names the file and not the folder holding it, so a tree that matched what git said would
    // leave the folded row saying nothing about the change it is hiding (`AMB-D-795`).
    hoisted.git[ROOT] = [{ path: ["src", "main.rs"], index: " ", worktree: "M", isDir: false }];
    await drawOpen();
    expect(container.querySelector(".files__dir--git-modified")?.textContent).toContain("src");

    await click(button("src"));
    await settle();
    // Open, the row inside says it, and the folder stops saying it: two rows in one colour down one
    // column would be the tree saying "somewhere, something".
    expect(container.querySelector(".files__dir--git-modified")).toBeNull();
    expect(container.querySelector(".files__file--git-modified")?.textContent).toContain("main.rs");
  });

  it("colours a whole repository nothing is tracked in yet", async () => {
    hoisted.entries[""] = [{ name: "one.md", isDir: false, ignored: false }];
    // Zero segments is the bound folder itself, which is what git names when nothing under it is
    // tracked. Dropped, a new repository would have no colour anywhere.
    hoisted.git[ROOT] = [{ path: [], index: "?", worktree: "?", isDir: true }];
    await drawOpen();
    expect(container.querySelector(".files__file--git-untracked")?.textContent).toContain("one.md");
  });

  it("draws text that is not Markdown as it was written", async () => {
    hoisted.entries[""] = [{ name: "run.sh", isDir: false, ignored: false }];
    hoisted.file = aFile({ text: "#!/bin/sh\necho hi" });
    await draw();
    await click(button(t("files.tree")));
    await settle();
    await click(button("run.sh"));
    await settle();
    // Not a heading: the hash in a shell script is a comment, and a name is what decides that.
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector(".cm-editor")?.textContent).toBe("#!/bin/sh\necho hi");
    // And it is a file this panel could save, so it is one somebody may type into. The name goes
    // with it: what language a file is written in is the only thing that says how to colour it.
    expect(last(hoisted.editing))
      .toEqual({ text: "#!/bin/sh\necho hi", editable: true, name: "run.sh" });
  });

  /** A file the panel could never write back is read-only from the moment it opens. Saying so after
   *  somebody has typed into it would be worse than not letting them: the text they wrote would
   *  have nowhere to go (`AMB-D-773`). */
  it("opens a file it could not save without letting anyone type into it", async () => {
    hoisted.entries[""] = [{ name: "cut.txt", isDir: false, ignored: false }];
    hoisted.file = aFile({ text: "as far as it goes", truncated: true, clean: false });
    await draw();
    await click(button(t("files.tree")));
    await settle();
    await click(button("cut.txt"));
    await settle();
    expect(last(hoisted.editing)?.editable).toBe(false);

    // Whole, but in an encoding nothing writes back: the same answer, for the other reason.
    hoisted.file = aFile({ text: "read me", clean: false });
    await click(button(t("files.back")));
    await settle();
    await click(button("cut.txt"));
    await settle();
    expect(last(hoisted.editing)?.editable).toBe(false);
  });

  it("says so when the file is not something a panel can show", async () => {
    hoisted.file = aFile();
    await drawOpen();
    await click(button("a.md"));
    await settle();
    // The name says Markdown and the bytes say otherwise; the bytes win (`crate::folder`).
    expect(container.textContent).toContain(t("files.notText"));
  });

  it("points a picture at the door that hands out a file, not at bytes of its own", async () => {
    hoisted.file = aFile({ image: { mime: "image/png" } });
    await drawOpen();
    await click(button("a.md"));
    await settle();
    // The address is the project, the folder and the path this reader was opened on — the same
    // three the host resolved the answer through, so nothing had to be carried (`AMB-D-783`).
    expect(container.querySelector("img")?.getAttribute("src"))
      .toBe("amenbofile://localhost/1/%2Fwork%2Frepo/a.md?mime=image%2Fpng");
  });

  it("says what a picture it would not draw was measured at, and offers the way on", async () => {
    hoisted.file = aFile({ oversize: { bytes: 6 * 1024 * 1024, width: 40000, height: 30000 } });
    await drawOpen();
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
    expect(hoisted.asked).toContain(`open:${ROOT}:a.md`);
  });

  it("refuses on bytes alone where the picture would not say its size", async () => {
    hoisted.file = aFile({ oversize: { bytes: 6 * 1024 * 1024 } });
    await drawOpen();
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
    hoisted.file = aFile({ oversize: { bytes: 10 * 1024, width: 16000, height: 16000 } });
    await drawOpen();
    await click(button("a.md"));
    await settle();
    expect(container.textContent).toContain(
      formatNumber(10, { style: "unit", unit: "kilobyte", unitDisplay: "short", maximumFractionDigits: 0 }),
    );
  });

  it("goes all the way down to bytes rather than round a refused picture to nothing", async () => {
    // A header alone is the cheapest thing that can claim thirty thousand square, and it is under a
    // kilobyte. Rounded up a unit it would read "0 kB" — the same lie one unit further down.
    hoisted.file = aFile({ oversize: { bytes: 33, width: 30000, height: 30000 } });
    await drawOpen();
    await click(button("a.md"));
    await settle();
    expect(container.textContent).toContain(
      formatNumber(33, { style: "unit", unit: "byte", unitDisplay: "short", maximumFractionDigits: 0 }),
    );
  });

  it("says out loud when only the head of a long file is shown", async () => {
    hoisted.file = aFile({ text: "x".repeat(10), truncated: true, clean: false });
    await drawOpen();
    await click(button("a.md"));
    await settle();
    expect(container.textContent).toContain(t("files.cut"));
  });

  it("leaves this face when a reference in a file is followed", async () => {
    const left: number[] = [];
    hoisted.file = aFile({ text: "see AMB-T-12" });
    await drawOpen({ onOpenLedger: () => left.push(1) });
    await click(button("a.md"));
    await settle();
    // A reference selects on the other face. Following one from here without leaving would land on
    // a pane the reader cannot see — a link that looks alive and is not (`AMB-D-747`).
    const ref = [...container.querySelectorAll("a")].find((a) => a.textContent === "AMB-T-12");
    expect(ref).toBeDefined();
    await click(ref);
    expect(left).toEqual([1]);
  });
  it("asks before it puts a row in the bin, and bins it on yes", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    // Nothing has gone yet: the item opens the question, and the question is where the answer is.
    expect(hoisted.asked.some((one) => one.startsWith("trash:"))).toBe(false);
    expect(document.body.textContent).toContain(tf("files.trashAsk", { name: "a.md" }));

    await click(anyButton(t("files.trashGo")));
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md`);
  });

  it("bins nothing when the answer is no", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    await click(anyButton(t("files.trashKeep")));
    expect(hoisted.asked.some((one) => one.startsWith("trash:"))).toBe(false);
    expect(document.body.textContent).not.toContain(tf("files.trashAsk", { name: "a.md" }));
  });

  it("stops asking once the reader says not to, and only then", async () => {
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    // The checkbox takes effect on the answer, not on the tick: a reader who ticks it and cancels
    // has agreed to nothing.
    const quiet = document.querySelector<HTMLInputElement>(".trashask__quiet input")!;
    await act(async () => { quiet.click(); });
    await click(anyButton(t("files.trashGo")));
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md`);

    hoisted.asked = [];
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    // No question this time, and the row is in the bin.
    expect(anyButton(t("files.trashGo"))).toBeUndefined();
    expect(hoisted.asked).toContain(`trash:${ROOT}:a.md`);
  });

  it("puts back what the last press binned, on the machine's own key", async () => {
    await drawOpen();
    await undo();
    expect(hoisted.asked).toContain("untrash");
  });

  it("says what the machine said about a row that would not go", async () => {
    hoisted.trashed = {
      gone: [],
      stopped: { name: "a.md", why: "the volume \u201cAMBRO\u201d does not have one" },
    };
    await drawOpen();
    await menuOn(button("a.md"));
    await click(button(t("files.trash")));
    await click(anyButton(t("files.trashGo")));
    // The row it is about is gone from the list either way, so the sentence stands in the panel —
    // and it is the machine's own words, not a code with a template behind it.
    expect(container.querySelector(".files__stopped")?.textContent)
      .toContain("does not have one");
  });

});

describe("a project bound to several folders", () => {
  const OTHER = "/work/plugins";
  const both = () => { hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: true }]; };

  /** Both sections drawn with their trees unfolded — every row of both is then on the screen. */
  async function openBothTrees() {
    await draw();
    for (const head of [...container.querySelectorAll("button")].filter(
      (one) => one.textContent === t("files.tree"),
    )) {
      await click(head);
      await settle();
    }
  }

  /** One folder's section, found by the heading over it — the sections are ordered by path, and a
   *  test that counted on that would be about the ordering rather than about what it says it is. */
  const folderNamed = (label: string) =>
    [...container.querySelectorAll(".files__folder")].find(
      (one) => one.querySelector(".files__foldername")?.textContent === label,
    )!;

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

  it("asks git about each folder on its own, and colours each one by its own answer", async () => {
    both();
    // One is a repository with something changed in it; the other answers with nothing, which is
    // what a folder that is no repository answers — and it is not the first one's business.
    hoisted.git = { [OTHER]: [{ path: ["a.md"], index: " ", worktree: "M", isDir: false }] };
    await openBothTrees();
    expect(hoisted.asked).toContain(`git:${ROOT}`);
    expect(hoisted.asked).toContain(`git:${OTHER}`);
    expect(folderNamed("repo").querySelector(".files__file--git")).toBeNull();
    expect(folderNamed("plugins").querySelector(".files__file--git-modified")?.textContent)
      .toContain("a.md");
  });

  it("reads a file out of the folder its row was drawn in", async () => {
    both();
    hoisted.file = aFile({ text: "hello" });
    await openBothTrees();
    const row = [...folderNamed("plugins").querySelectorAll("button")]
      .find((one) => one.textContent === "a.md");
    await click(row);
    await settle();
    // The same path names a different file in each folder, so which folder the row was in has to
    // travel with it.
    expect(hoisted.asked).toContain(`read:${OTHER}:a.md`);
  });

  it("keeps a folder that has gone, and says that is what happened", async () => {
    hoisted.bound = [{ path: ROOT, exists: true }, { path: OTHER, exists: false }];
    await draw();
    // Dropped from the list it would look like a binding nobody ever made, and a reader would have
    // no way to tell a folder that moved from one they unbound themselves.
    expect(container.textContent).toContain(t("files.folderGone"));
    expect(hoisted.asked).not.toContain(`watch:1:${OTHER}`);
  });

  /** Every section draws a row for its own root, and the trees under two of them can hold the same
   *  names. A landing that said only the path would light a row up in both. */
  it("marks a drop's landing in the section the pointer is in, and in no other", async () => {
    both();
    hoisted.entries[""] = [{ name: "src", isDir: true, ignored: false }];
    await draw();
    for (const head of [...container.querySelectorAll("button")].filter(
      (one) => one.textContent === t("files.tree"),
    )) {
      await click(head);
      await settle();
    }
    const trees = [...container.querySelectorAll(".files__folder")];
    expect(trees).toHaveLength(2);

    let step = 0;
    const over = (el: Element | null | undefined) => act(async () => {
      (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint =
        () => el ?? null;
      step += 1;
      hoisted.dragging?.({ payload: { type: "over", position: { x: step, y: 1 } } });
      await new Promise((r) => setTimeout(r, 0));
    });

    const srcOf = (folder: Element) =>
      [...folder.querySelectorAll("button")].find((one) => one.textContent === "src");
    await over(srcOf(trees[1]!));
    // The row lit up is in the second folder, and the first folder's `src` is left alone.
    expect(trees[1]!.querySelectorAll(".files__into")).toHaveLength(1);
    expect(trees[0]!.querySelectorAll(".files__into")).toHaveLength(0);
  });

  // ── naming ──────────────────────────────────────────────────────────────────────────────────
  // Making a name and writing over one are the two doors `crate::folder_write` opens that have a
  // reader on this side. Both are typed into the row itself rather than into a dialog: what a person
  // is naming sits in a list, and the names already in it are what they are choosing against.

  it("makes a name in the folder that was pointed at, and looks at the folder again", async () => {
    hoisted.entries = { "": [] };
    await draw();
    // The heading is the folder's own row, and the only one there is to point at when the tree is
    // empty — which is exactly when a person wants to put something in it.
    await menuOn(button(t("files.tree")));
    // A folder is not something to hand to an application, so it is offered none of that.
    expect(button(t("files.openWith"))).toBeUndefined();

    await click(button(t("files.newFile")));
    await settle();
    const box = namebox();
    expect(box).not.toBeNull();

    await type(box!, "notes.md");
    await press(box!, "Enter");
    expect(hoisted.asked).toContain(`make:${ROOT}:notes.md:file`);
    // The names are read again rather than left to the watch, which says the same thing a debounce
    // later: a row somebody has just made belongs on the list before they look for it.
    expect(hoisted.asked.filter((one) => one === `entries:${ROOT}:`)).toHaveLength(2);
    expect(namebox()).toBeNull();
  });

  it("makes the name inside the folder that was pointed at, unfolding it to be typed in", async () => {
    hoisted.entries = {
      "": [
        { name: "src", isDir: true, ignored: false },
        { name: "README.md", isDir: false, ignored: false },
      ],
      src: [],
    };
    await drawOpen();

    await menuOn(button("src"));
    await click(button(t("files.newFolder")));
    await settle();
    // `src` was folded shut a moment ago. A box typed into a folder nobody can see would be a name
    // made somewhere the reader never looked.
    expect(hoisted.asked).toContain(`entries:${ROOT}:src`);
    await type(namebox()!, "deep");
    await press(namebox()!, "Enter");
    expect(hoisted.asked).toContain(`make:${ROOT}:src/deep:dir`);

    // A file is not something a name can be made in, so its menu offers none of it — what it is
    // offered is the hand-over doors and its own name.
    await menuOn(button("README.md"));
    expect(button(t("files.newFile"))).toBeUndefined();
    expect(button(t("files.rename"))).toBeDefined();
    expect(button(t("files.openWith"))).toBeDefined();
  });

  it("writes over the name of the row that was pointed at, starting from what it says", async () => {
    hoisted.entries = { "": [{ name: "notes.md", isDir: false, ignored: false }] };
    await drawOpen();

    await menuOn(button("notes.md"));
    await click(button(t("files.rename")));
    await settle();
    // The box opens on the name it is about, so changing one letter is one letter of typing —
    // which is the whole of what a case-only rename is (`crate::folder_write`).
    expect(namebox()?.value).toBe("notes.md");

    await type(namebox()!, "Notes.md");
    await press(namebox()!, "Enter");
    expect(hoisted.asked).toContain(`rename:${ROOT}:notes.md:Notes.md`);
  });

  /** Which names a machine will hold is the one thing a reader cannot work out for themselves, so
   *  the refusal is drawn where they are still typing — and drawn from the dictionary, because the
   *  sentence the command carries is English whoever is reading it (`AMB-D-413`). */
  it("says why a name was refused, and leaves what was typed where it was", async () => {
    const refusal: CmdError = {
      code: "folder_taken",
      message_en: "notes.md is already there",
      fields: { name: "notes.md" },
    };
    hoisted.entries = { "": [] };
    hoisted.refuse = refusal;
    // The answer takes a moment, which is the whole of what this is about: a browser blurs the box
    // the instant anything about it changes under the reader's fingers, and that lands while the
    // refusal is still on its way.
    hoisted.slowRefusal = true;
    await draw();
    await menuOn(button(t("files.tree")));
    await click(button(t("files.newFile")));
    await settle();

    await type(namebox()!, "notes.md");
    const box = namebox()!;
    await press(box, "Enter");
    // Left while the answer is out. A box that closed itself here would take the refusal with it,
    // and the reader would watch the name they typed vanish with nothing said.
    await leave(box);
    await act(async () => { await new Promise((r) => setTimeout(r, 20)); });
    expect(container.textContent).toContain(errLabel(refusal));
    // Still there, still holding what was typed: a refusal a person has to type their way back to
    // is one they were told nothing by. And the name was asked for once, not once per leaving.
    expect(namebox()?.value).toBe("notes.md");
    expect(hoisted.asked.filter((one) => one.startsWith("make:"))).toHaveLength(1);

    // Leaving it now is giving up on a name the machine has already answered about — not asking the
    // same question again.
    await leave(namebox()!);
    expect(namebox()).toBeNull();
    expect(hoisted.asked.filter((one) => one.startsWith("make:"))).toHaveLength(1);
    // And the sentence is the dictionary's rather than the command's, which is what makes it read
    // in a language core holds no prose for.
    expect(errLabel(refusal, "ja")).not.toContain("already");
  });

  it("offers no rename for the bound folder, and asks for nothing when the box is escaped", async () => {
    hoisted.entries = { "": [] };
    await draw();
    await menuOn(button(t("files.tree")));
    // The section's own root is the binding, and where a binding is changed is the project's
    // settings — not a row in the tree.
    expect(button(t("files.rename"))).toBeUndefined();

    await click(button(t("files.newFolder")));
    await settle();
    await type(namebox()!, "notes");
    await press(namebox()!, "Escape");
    expect(namebox()).toBeNull();
    expect(hoisted.asked.some((one) => one.startsWith("make:"))).toBe(false);
  });

  it("says so when a folder goes while it is being looked at", async () => {
    both();
    await draw();
    expect(container.textContent).not.toContain(t("files.folderGone"));
    await act(async () => {
      tell({ root: OTHER, capped: false, unwatched: false, gone: true });
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(container.textContent).toContain(t("files.folderGone"));
  });
});
