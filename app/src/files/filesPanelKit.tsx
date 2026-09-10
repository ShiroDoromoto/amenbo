// The ground every road through the file face is walked on: the host stood in for, the two columns
// put together the way the terminal face puts them, and the small words a test presses and reads
// with. It is imported first by each of the `filesPanel.*.test.tsx` files, which is what puts the
// stand-ins in place before the panel itself is loaded.
import { act, createElement, Fragment, useEffect, useRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, vi } from "vitest";
import type {
  DropEffectDto, FolderAppDto, FolderCarriedDto, FolderChangesDto, FolderEntryDto, FolderFileDto,
  GitEntryDto,
} from "../bindings/bindings";

export const ROOT = "/work/repo";

/** The most recent of what the editor was asked to draw. */
export const last = <T,>(all: T[]): T | undefined => all[all.length - 1];

/** What the host answers `folder_read` with, filled in around whatever a test cares about. */
export const aFile = (about: Partial<FolderFileDto> = {}): FolderFileDto => ({
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
  /** The encodings the host says a file may be reopened in. */
  encodings: [] as string[],
  /** The folders the project is bound to. Empty is a project nobody has bound one to yet. */
  bound: [] as { path: string; exists: boolean }[],
  /** The host's side of the drag-and-drop subscription (`../core/hostDrop`). */
  dragging: null as null | ((event: { payload: unknown }) => void),
  /** What the editor was asked to draw, and whether it was allowed to be typed into. */
  editing: [] as { text: string; editable: boolean; name: string }[],
  /** Every text the panel replaced the standing editor's document with — a file read again is one
   *  of the two ways this happens, and "nothing was replaced" is a thing to assert (`AMB-D-784`). */
  shown: [] as string[],
  /** What the host answers about a row it was asked to bin — a test makes one stop by filling it. */
  trashed: null as null | { gone: string[]; stopped: { name: string; why: string } | null },
  /** What comes back out of the bin. `null` is the host saying there is nothing left to undo. */
  restored: null as null | { back: string[]; stopped: { name: string; why: string } | null },
  /** Every carry the panel asked the host for, as it asked for it. */
  imported: [] as
    { paths: string[]; toRoot: string; to: string[]; effect: DropEffectDto }[],
  /** Every carry inside the panel the host was asked for, as it was asked (`./handDrag`). */
  carries: [] as
    { how: "move" | "copy"; root: string; paths: string[][]; toRoot: string; to: string[] }[],
  /** What the host answers a carry with — the whole list arriving, unless a test says otherwise. */
  carried: { arrived: [] as string[], stopped: null } as FolderCarriedDto,
  /** What the host refuses a name with, where a test is about the refusal. */
  refuse: null as unknown,
  /** Where a test holds that refusal back. The answer waits on this until the test lets it go, so
   *  the ordering a real host has — the box blurring while the answer is still out — is arranged
   *  rather than guessed at (`AMB-T-4481`). */
  heldRefusal: null as null | Promise<void>,
  /** The way to tell the panel a person typed, as the stand-in editor took it. */
  typing: null as null | (() => void),
  /** Every save the panel asked for, and what it sent with it — the mark of what was read
   *  included, which is what the host weighs the file against (`AMB-D-784`). */
  saved: [] as {
    path: string; text: string; encoding: string; bom: boolean; lineEnding: string; seen: string;
  }[],
  /** Every comparison the panel asked for, as the two texts it handed over. */
  compared: [] as { theirs: string; mine: string }[],
  /** The mark a save answers with: what the file is once this save has landed. */
  keptDigest: "after",
  /** What the next save is refused with. Its own field: a name and a save are refused for different
   *  reasons, and a test about one must not arm the other. */
  refuseSave: null as unknown,
  /** What the next read is refused with. Its own field for the same reason a save has one: a read
   *  and a save are turned away for different reasons. */
  refuseRead: null as unknown,
  /** Every question the panel put in front of a press that loses what a reader typed, as it was
   *  worded. */
  confirmed: [] as string[],
  /** The answers waiting for those questions, in order. An empty list is a reader who says yes —
   *  which is what a test about anything else wants. */
  answers: [] as boolean[],
}));

// Handed on in a statement of its own: what `vi.hoisted` returns is moved above this file's imports,
// and a declaration that is moved cannot carry an `export` with it.
export { hoisted };

// The editor is loaded on demand and lays itself out by measuring, which jsdom cannot do — so what
// it was asked to draw is recorded instead, the same stand-in the Markdown face makes for mermaid.
vi.mock("./editorLoad", () => ({
  mountEditor: async (
    parent: HTMLElement, text: string, editable: boolean, name: string, onEdit?: () => void,
  ) => {
    hoisted.editing.push({ text, editable, name });
    const drawn = parent.ownerDocument.createElement("div");
    drawn.className = "cm-editor";
    drawn.textContent = text;
    parent.appendChild(drawn);
    hoisted.typing = () => onEdit?.();
    return {
      show(next: string) { hoisted.shown.push(next); drawn.textContent = next; },
      text() { return drawn.textContent ?? ""; },
      close() { drawn.remove(); hoisted.typing = null; },
    };
  },
}));

// The two texts side by side are two editors laying themselves out by measuring, which jsdom cannot
// do either — so what the panel handed over is recorded, the same stand-in the editor has.
vi.mock("./diffLoad", () => ({
  mountDiff: async (parent: HTMLElement, theirs: string, mine: string) => {
    hoisted.compared.push({ theirs, mine });
    const drawn = parent.ownerDocument.createElement("div");
    drawn.className = "cm-mergeView";
    parent.appendChild(drawn);
    return { close() { drawn.remove(); } };
  },
}));

// The question in front of a press that throws away what a reader typed. It is the machine's own
// dialog and jsdom has none, so the answer is arranged here — and the wording is kept, because
// which file the reader was asked about is part of what the question has to get right.
vi.mock("../core/dialog", () => ({
  confirmDialog: async (message: string) => {
    hoisted.confirmed.push(message);
    return hoisted.answers.shift() ?? true;
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
  folderRead: async (
    _projectId: number,
    root: string,
    path: string[],
    encoding?: string,
  ): Promise<FolderFileDto> => {
    hoisted.asked.push(`read:${root}:${path.join("/")}${encoding === undefined ? "" : `:${encoding}`}`);
    if (hoisted.refuseRead !== null) throw hoisted.refuseRead;
    return hoisted.file;
  },
  folderEncodings: async (): Promise<string[]> => {
    hoisted.asked.push("encodings");
    return hoisted.encodings;
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
  folderClipCopy: async (_projectId: number, root: string, paths: string[][]) => {
    hoisted.asked.push(`clip-copy:${root}:${paths.map((one) => one.join("/")).join(",")}`);
  },
  folderClipPaste: async (
    _projectId: number,
    toRoot: string,
    to: string[],
  ): Promise<FolderCarriedDto> => {
    hoisted.asked.push(`clip-paste:${toRoot}:${to.join("/")}`);
    return hoisted.carried;
  },
  folderMove: async (
    _projectId: number,
    root: string,
    paths: string[][],
    toRoot: string,
    to: string[],
  ): Promise<FolderCarriedDto> => {
    hoisted.carries.push({ how: "move", root, paths, toRoot, to });
    return hoisted.carried;
  },
  folderCopy: async (
    _projectId: number,
    root: string,
    paths: string[][],
    toRoot: string,
    to: string[],
  ): Promise<FolderCarriedDto> => {
    hoisted.carries.push({ how: "copy", root, paths, toRoot, to });
    return hoisted.carried;
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
    if (hoisted.heldRefusal !== null) await hoisted.heldRefusal;
    throw hoisted.refuse;
  },
  folderSave: async (
    _projectId: number, root: string, path: string[],
    text: string, encoding: string, bom: boolean, lineEnding: string, seen: string,
  ): Promise<string> => {
    hoisted.asked.push(`save:${root}:${path.join("/")}`);
    if (hoisted.refuseSave !== null) throw hoisted.refuseSave;
    hoisted.saved.push({ path: path.join("/"), text, encoding, bom, lineEnding, seen });
    return hoisted.keptDigest;
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

import { FilesPanel, openKey, type OpenFile, type Typed } from "./FilesPanel";
import { FolderTree } from "./FolderTree";
import { fileUnderAny } from "./fileUnder";
import { formatNumber } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

export let container: HTMLDivElement;
export let root: Root;

/** The host's word that a folder moved, said the way it is said: once, to everyone listening. */
export function tell(changes: FolderChangesDto) {
  for (const take of [...hoisted.takers]) take(changes);
}

export async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

/**
 * Hold the host's refusal of a name back, and hand over the way to let it arrive.
 *
 * The order of the two is the whole of what the refusal test is about: the box blurs while the
 * answer is still on its way. A stand-in that sleeps a few milliseconds before throwing is guessing
 * that the test reaches the blur inside those milliseconds — a guess that holds on a machine running
 * this file alone and fails under the whole suite, where every file has a worker of its own and each
 * step takes longer than the sleep. Then the answer lands first, the blur gives up on a name already
 * refused, and the refusal leaves the screen with the box: a red build saying nothing about the code
 * (`AMB-T-3858`, `AMB-T-4481`). Held rather than slept, there is no length to be wrong about.
 */
export function holdRefusal() {
  let letGo = () => {};
  hoisted.heldRefusal = new Promise<void>((resolve) => { letGo = resolve; });
  return async () => {
    hoisted.heldRefusal = null;
    await act(async () => { letGo(); await new Promise((r) => setTimeout(r, 0)); });
  };
}

export type Props = Parameters<typeof FilesPanel>[0] & Parameters<typeof FolderTree>[0] & {
  /** A path clicked in a pane, which the face around the two columns resolves (`Columns`). */
  show?: { target: string; cwd: string | null; nth: number } | null;
};

/**
 * The two columns as the terminal face puts them together: the tree in the rail, and the file it
 * opens on the other side of the panes (`AMB-D-835`, `../shell/TerminalFace`).
 *
 * They are drawn together here because that is what a road through this face walks — a row is
 * pressed in one column and what it opens appears in the other — and the file being read is the one
 * piece of state the two share, so the wiring that holds it is written out the way the face writes
 * it.
 */
export function Columns({ show, ...props }: Partial<Props> & { projectId: number | null }) {
  // The files the column is holding and the one on top, wired the way the face wires them
  // (`../shell/TerminalFace`).
  const [open, setOpen] = useState<OpenFile[]>([]);
  const [showing, setShowing] = useState<string | null>(null);
  // What each open file was left holding, which the face keeps for the same reason it keeps the
  // list: the column draws one of them and the rest are off the screen (`../shell/TerminalFace`).
  const [typed, setTyped] = useState<Record<string, Typed>>({});
  const keepTyped = (at: OpenFile, one: Typed | null) => setTyped((was) => {
    const key = openKey(at);
    // A file the column is no longer holding holds nothing: a tab closed is answered after the file
    // it was on has left the screen, and the face drops what arrives then (`../shell/TerminalFace`).
    if (one !== null) return holding.current.has(key) ? { ...was, [key]: one } : was;
    if (!(key in was)) return was;
    const left = { ...was };
    delete left[key];
    return left;
  });
  // The step the column is standing on, which opening a file asks for and the face keeps.
  const [wide, setWide] = useState(false);
  // Which half is up. The face keeps it and the column reads it, so the harness holds it too
  // (`../talk/columns`).
  const [tab, setTab] = useState<"files" | "memo">(props.tab ?? "files");
  const reading = open.find((one) => openKey(one) === showing) ?? open[0] ?? null;
  // The keys the column is holding as of this draw, for the answer that arrives after a tab has
  // gone: the face reads its own state where this harness has to keep a mirror of it.
  const holding = useRef(new Set<string>());
  holding.current = new Set(open.map(openKey));
  const openOne = (at: OpenFile) => {
    setOpen((was) => (was.some((one) => openKey(one) === openKey(at)) ? was : [...was, at]));
    setShowing(openKey(at));
    setWide(true);
  };
  const closeOne = (at: OpenFile) => {
    const key = openKey(at);
    setOpen((was) => was.filter((one) => openKey(one) !== key));
    setShowing((now) => (now === key ? null : now));
    keepTyped(at, null);
  };
  const gone = (root: string, went: string[]) => {
    const dead = new Set(went.map((one) => `${root} ${one}`));
    setOpen((was) => was.filter((one) => !dead.has(openKey(one))));
    setShowing((now) => (now !== null && dead.has(now) ? null : now));
  };
  // A path clicked in a pane opens where it lands inside a bound folder, and nowhere else
  // (`AMB-D-747`). The count is what makes the same file asked for twice two answers.
  useEffect(() => {
    if (show === undefined || show === null) return;
    const found = fileUnderAny(hoisted.bound.map((one) => one.path), show.cwd, show.target);
    if (found) openOne(found);
  }, [show?.nth]);
  // Each in the place the face puts it, so a road can say which column it means: the same class name
  // is drawn in both, and a test that asked the document for it would be handed whichever came
  // first.
  return createElement(
    Fragment,
    null,
    createElement("div", { className: "rail" }, createElement(FolderTree, {
      projectId: props.projectId,
      reading,
      onRead: openOne,
      onGone: gone,
      onHandOver: props.onHandOver,
      onCarry: props.onCarry,
    })),
    createElement("div", { className: "termface__column--side" }, createElement(FilesPanel, {
      projectId: props.projectId,
      tab,
      onTab: setTab,
      open,
      reading,
      typed,
      onTyped: keepTyped,
      onPick: (at: OpenFile) => setShowing(openKey(at)),
      onCloseTab: closeOne,
      onBack: () => { if (reading !== null) closeOne(reading); },
      onGone: gone,
      onClose: props.onClose ?? (() => {}),
      wide,
      onWide: setWide,
      onOpenLedger: props.onOpenLedger,
      onHandOver: props.onHandOver,
    })),
  );
}

export async function draw(props: Partial<Props> = {}) {
  await act(async () => {
    const projectId = "projectId" in props ? (props.projectId ?? null) : 1;
    // Which half is up, and the column's own way out, belong to the terminal face around it
    // (`../shell/TerminalFace`). These tests are about what the two columns draw, so what the face
    // would answer is what they hand them.
    root.render(createElement(Columns, { ...props, projectId }));
  });
  await settle();
}

/**
 * Pressing something the way a browser does: the pointer goes down first, and only then does the
 * click land. Dispatching the click alone would pass over a menu that closes itself on the way
 * down — which is a menu whose items can never be reached.
 */
export function click(el: Element | null | undefined) {
  return act(async () => {
    el?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    el?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/**
 * Opening a file, which is two presses and not one (`AMB-D-835`): the first picks the row out and
 * the second asks for what is inside it. The browser sends both clicks and the double on top of
 * them, so that is what a test that opens a file sends.
 */
export function openFile(
  el: Element | null | undefined,
  keys: { metaKey?: boolean; ctrlKey?: boolean; shiftKey?: boolean } = {},
) {
  return act(async () => {
    el?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    el?.dispatchEvent(new MouseEvent("click", { bubbles: true, ...keys }));
    el?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 2, ...keys }));
    el?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, ...keys }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/**
 * The same, with a key held down: how a reader takes one row into the selection, or reaches a run
 * of them at once (`AMB-T-4229`).
 */
export function clickWith(
  el: Element | null | undefined,
  keys: { metaKey?: boolean; ctrlKey?: boolean; shiftKey?: boolean },
) {
  return act(async () => {
    el?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    el?.dispatchEvent(new MouseEvent("click", { bubbles: true, ...keys }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** The user agent jsdom reports, which places the webview on neither a Mac nor Windows
 *  (`../core/platform`) — so the key that takes a row into the selection is Ctrl. */
export const NOT_A_MAC = navigator.userAgent;

/** Read the rest of a test as a Mac, which is where ⌘ and Ctrl part company. */
export const onMac = () => Object.defineProperty(navigator, "userAgent", {
  configurable: true,
  value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
});

/** The rows picked out of one tree, in the order they are drawn — read off what a reader is told
 *  about them rather than off the class the stylesheet happens to hang the band on. */
export const pickedIn = (scope: ParentNode): string[] =>
  [...scope.querySelectorAll<HTMLElement>('[role="treeitem"][aria-selected="true"]')].map(labelOf);

/** A size as the panel writes one, so the assertion is about the number and not about `Intl`. */
export const megabytes = (n: number) =>
  formatNumber(n, { style: "unit", unit: "megabyte", unitDisplay: "short", maximumFractionDigits: 1 });

/**
 * A control by the words on it: a button, or one of the tree's rows.
 *
 * The rows are items rather than buttons, because the whole tree carries one stop in the tab order
 * instead of one per row (`./FilesPanel`). What a row is called is read off its own line rather than
 * off the item holding it, which is where the name a row is found by is drawn.
 */
export const button = (text: string): HTMLElement | undefined =>
  [...container.querySelectorAll<HTMLElement>("button, [role=\"treeitem\"]")]
    .find((b) => labelOf(b).includes(text));

export const labelOf = (el: HTMLElement): string =>
  (el.getAttribute("role") === "treeitem"
    ? el.querySelector(":scope > .files__dir, :scope > .files__file")?.textContent
    : el.textContent) ?? "";

/** The same, where what is asked is a button's own state rather than a press on it. */
export const pressable = (text: string): HTMLButtonElement | undefined =>
  [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(text));

/** One row of one folder's tree, named exactly — for a test about which of two folders it is in. */
export const rowIn = (folder: Element, name: string): HTMLElement | undefined =>
  [...folder.querySelectorAll<HTMLElement>("[role=\"treeitem\"]")]
    .find((one) => labelOf(one) === name);

/** The same, over the whole page: the question before the bin is drawn onto `document.body`. */
export const anyButton = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(text));

/** One of the answers on the difference screen. Named apart from `anyButton` because the notice
 *  behind it carries the same two words: over the whole page, the press would land on the panel's
 *  copy and the screen would stay up. */
export const diffButton = (text: string) =>
  [...document.querySelectorAll(".filediff button")].find((b) => b.textContent?.includes(text));

/** Press one of the machine's own keys on a row, the way a reader standing on it does. */
export const pressOn = (el: Element | null | undefined, key: string) => act(async () => {
  el?.dispatchEvent(new KeyboardEvent("keydown", { key, metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

/** The row a name is drawn on, which is what the keyboard stands on (`role="treeitem"`). */
export const rowFor = (name: string) =>
  [...container.querySelectorAll<HTMLElement>('[role="treeitem"]')]
    .find((row) => row.textContent?.includes(name));

/** Press undo where it is heard: on the tree in the rail, not on the window (`AMB-D-780`). The rail
 *  is drawn first, so the first `.files` in the container is the tree's (`Columns`). */
export const undo = () => act(async () => {
  container.querySelector(".files")!.dispatchEvent(
    new KeyboardEvent("keydown", { key: "z", metaKey: true, bubbles: true }),
  );
  await new Promise((r) => setTimeout(r, 0));
});

/** The same press, made where the reader's hands are rather than on the panel around them. */
export const undoOn = (el: Element) => act(async () => {
  el.dispatchEvent(new KeyboardEvent("keydown", { key: "z", metaKey: true, bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

/** The menu, opened on a row the way a person opens it. */
export const menuOn = (el: Element | null | undefined) => act(async () => {
  el?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
  await new Promise((r) => setTimeout(r, 0));
});

/** The box a name is typed into, while one is being typed. */
export const namebox = () => container.querySelector<HTMLInputElement>(".files__namebox");

/** Typing into it the way a person does: React hears the `input` event, not an assigned `value`. */
export const type = (box: HTMLInputElement, text: string) => act(async () => {
  // The prototype is taken from the box's own window: React replaces the setter on the instance, and
  // this module's `HTMLInputElement` is not the one jsdom built the element from.
  const proto = box.ownerDocument.defaultView?.HTMLInputElement.prototype;
  const set = proto && Object.getOwnPropertyDescriptor(proto, "value")?.set;
  set?.call(box, text);
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

export const press = (el: Element, key: string) => act(async () => {
  el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

/** Leaving the box, which is the other of the two ways a name is kept. `focusout` and not `blur`:
 *  React listens for the one that bubbles, and a `blur` dispatched here reaches no handler at all. */
export const leave = (el: Element) => act(async () => {
  el.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

/**
 * The panel with the folder's own section unfolded, which is where every row is.
 *
 * The section stands unfolded from the moment the panel is drawn (`./FilesPanel`), so what is left
 * here is the settling — the first level is read off the host and the rows arrive with it. It keeps
 * its own name because what a test using it is about starts with those rows on the screen.
 */
/**
 * A container and a root stood up again, for the one test that unmounts the face itself: `afterEach`
 * unmounts what is standing, and there has to be something left for it to work on.
 */
export function standUpAgain() {
  container.remove();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
}

export async function drawOpen(props: Partial<Props> = {}) {
  await draw(props);
  await settle();
}

beforeEach(() => {
  hoisted.asked = [];
  hoisted.editing = [];
  hoisted.shown = [];
  hoisted.typing = null;
  hoisted.saved = [];
  hoisted.compared = [];
  hoisted.keptDigest = "after";
  hoisted.refuseSave = null;
  hoisted.refuseRead = null;
  hoisted.confirmed = [];
  hoisted.answers = [];
  // One file in the folder, so a test that only wants a row to press has one without saying so.
  hoisted.entries = { "": [{ name: "a.md", isDir: false, ignored: false }] };
  hoisted.file = aFile();
  hoisted.apps = [];
  hoisted.encodings = ["UTF-8", "Shift_JIS", "EUC-JP", "windows-1252", "ISO-2022-JP"];
  hoisted.refuse = null;
  hoisted.heldRefusal = null;
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
  hoisted.carries = [];
  hoisted.carried = { arrived: [], stopped: null };
  // Inside Tauri as far as the panel is concerned; without it there is no host to hear a drop from.
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  Object.defineProperty(navigator, "userAgent", { configurable: true, value: NOT_A_MAC });
  act(() => root.unmount());
  container.remove();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});
