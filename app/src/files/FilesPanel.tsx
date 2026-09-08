// The reading column: the far side of the terminal face, where a file opened in the rail's tree is
// read without leaving the window (`AMB-T-3602`).
//
// **It draws the file and nothing else.** Finding one is the tree's, and the tree is in the rail on
// the other side of the panes (`AMB-D-835`); what stands here is the file, the draft page, or the
// line saying nothing is open. The two used to share this column, and a file being read was drawn
// over the tree — which is what made picking a second row out impossible once the first was open.
//
// **The file it draws belongs to the project.** It is opened from a folder the project is bound to,
// so switching panes does not move it — what changed in the repository is the same question
// whichever terminal is in front of it.
//
// **What a file is, is the host's answer, not this side's guess.** A NUL in the head makes it
// binary and the first bytes make it a picture (`crate::folder`); the name decides only whether
// text is drawn as Markdown, which is a question about rendering rather than about what the file
// is.
import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";
import type { FolderFileDto } from "../bindings/bindings";
import { Markdown } from "../components/Markdown";
import { Menu, MenuItem } from "../components/Menu";
import { fileUrl } from "../core/fileUrl";
import { errText, formatNumber, isErr, t, tf } from "../core/i18n";
import { RefNavProvider, useRefNav, type RefNav } from "../core/refNav";
import {
  folderEncodings, folderRead, folderSave, folderUnwatch, folderWatch, onFolderChanged,
} from "./folder";
import { FileMenu } from "./FileMenu";
import { useTrash } from "./trash";
import { FileEditor } from "./FileEditor";
import { MemoPage } from "./MemoPage";
import { Icon } from "../components/Icon";

/** The names a file's text is drawn as Markdown under. The one thing here the name decides. */
const MARKDOWN = [".md", ".markdown"];

/** One file this column is holding: the bound folder it came out of, and its path inside it. */
export type OpenFile = { root: string; path: string[] };

/**
 * One file named apart from the others, for a key and for telling two of them apart.
 *
 * The folder travels with the path because the same path means a different file in each bound
 * folder, and a project bound to two of them can have a `README.md` in each (`AMB-D-778`).
 */
export function openKey(one: OpenFile): string {
  return `${one.root}\0${one.path.join("/")}`;
}

export function FilesPanel({
  projectId, tab, onTab, open, reading, onPick, onCloseTab, onBack, onGone, onClose, wide, onWide,
  onOpenLedger, onHandOver,
}: {
  /** The project the file belongs to; nothing is drawn without one. */
  projectId: number | null;
  /**
   * Which of the two halves is up.
   *
   * **The switch is this column's own row of tabs and nowhere else.** The top row above the panes
   * asked the same question once, and two controls that do the same thing leave a reader looking for
   * the right one; the row that stayed is the one that can also say which files are open, and that
   * draws the draft page as a tab beside them. What the top row kept is opening the column and
   * closing it (`../shell/TerminalFace`).
   */
  tab: "files" | "memo";
  /** Ask for the other half — the tabs, which are the one door to it. */
  onTab: (tab: "files" | "memo") => void;
  /**
   * The files this column is holding, in the order they were opened (`AMB-D-835`).
   *
   * Several, because reading one file while another stays open is what a reader does: following a
   * reference is a second file to hold, not a first one to give up. They are held by the face rather
   * than here — the tree in the rail marks the row each was opened from, so both columns answer to
   * the same list.
   */
  open: readonly OpenFile[];
  /** Which of them is on top, or nothing where none has been opened. */
  reading: OpenFile | null;
  /** Bring one of the open files up. */
  onPick: (at: OpenFile) => void;
  /** Let one go. What is left is what a reader still has open, and the column stands on the one
   *  beside it — a tab closed is not a column closed. */
  onCloseTab: (at: OpenFile) => void;
  /** Leave the file that is up, which is closing its tab: the way back off the reading face. */
  onBack: () => void;
  /** The rows that have gone to the bin, so the tree in the rail hears about a file binned here. */
  onGone?: (root: string, went: string[]) => void;
  /** Put the column away. What opens it again is the top row, which is where it was opened from. */
  onClose: () => void;
  /**
   * Whether the column is standing on its wide width — the one that lies over the panes — rather
   * than the narrow one they are drawn beside (`AMB-D-835`).
   *
   * The step is the face's because the face is what draws the column at it, and because everything
   * that puts it back narrow happens outside this column: a press on a pane, a press on the rail.
   * What is here is the two ways a reader asks from inside — the control, and the key.
   */
  wide: boolean;
  onWide: (want: boolean) => void;
  /** Leave the terminal face for the ledger — what a reference or a record means when it is clicked. */
  onOpenLedger?: () => void;
  /** Hand the file being read to the pane the reader is working in (`../shell/TerminalFace`). */
  onHandOver?: (wholes: string[]) => void;
}) {
  // The bin, for the file on the screen. The tree in the rail holds one of its own for the rows
  // picked out there: what is shared is how a press behaves, not one question for the two of them
  // (`./trash`).
  const trash = useTrash(projectId, onGone);

  /**
   * The one key this column hears, rather than the window: the terminal beside it has its own idea
   * of what it means, and the boundary between the two is which of them the reader is in
   * (`AMB-D-780`).
   *
   * **Undo is not one of them.** The bin belongs to the tree, and the tree is in the rail
   * (`AMB-D-835`) — so what a reader takes back here is their own writing, and `⌘Z` is the draft
   * page's or the editor's rather than this column's (`AMB-T-4523`).
   *
   * **One press, one layer.** The wide width goes first and the column itself after it, so the two
   * things a reader might mean by "back" are told apart by how many times they press rather than by
   * finding a different way out of each (`AMB-D-815`). The width is not a layer of its own on top of
   * that — it *is* the first one, which is why a column standing narrow closes on one press.
   */
  const onKey = (e: ReactKeyboardEvent) => {
    if (e.key !== "Escape") return;
    // Not this column's to take while something inside it is already answering to the same key: a
    // menu and the question before a bin both close on Escape, and a press counted twice would
    // carry the reader a layer past the one they asked for.
    if (trash.asking || (e.target as HTMLElement).closest('[role="menu"]') !== null) return;
    e.preventDefault();
    if (wide) onWide(false);
    else onClose();
  };

  // The width and the way out, and the row they sit on. It stands first in every state the column
  // can be in — reading a file included — because a column that could only be closed from one of
  // its states is one a reader has to find their way back out of.
  //
  // **It is the same row in the same place whichever half is up.** The draft page and a file used to
  // put it at opposite ends of the column, so crossing between them moved everything under it a line
  // up or down, and the way out was somewhere else each time (`AMB-T-4271`).
  //
  // **One control with two ends, and what it says is the end it goes to.** A column already lying
  // over the panes offers to give them back; a narrow one offers the room to read in. The mark is
  // the direction the edge would move, which is the same thing said without a word (`AMB-D-835`).
  const top = (
    <div className="files__top">
      <button
        className="files__width"
        title={t("files.width")}
        aria-pressed={wide}
        onClick={() => onWide(!wide)}
      >
        <Icon name={wide ? "chevronRight" : "chevronLeft"} />
      </button>
      <button className="files__close" title={t("pane.close")} onClick={onClose}>
        <Icon name="close" />
      </button>
    </div>
  );

  // The row of tabs, drawn in every state this column can be in: the draft page is one of them, and
  // a reader who has it up has to be able to reach the files they left open (`AMB-D-835`).
  const tabs = projectId === null ? null : (
    <FileTabs
      open={open}
      showing={tab === "memo" ? null : reading}
      memo={tab === "memo"}
      onMemo={() => onTab("memo")}
      onPick={(at) => { onTab("files"); onPick(at); }}
      onCloseTab={onCloseTab}
    />
  );

  // The draft page is the project's, and a project has one whether or not it is bound to a folder
  // (`./MemoPage`). So the half that is up is answered first, and only the files half goes on to ask
  // whether anything is open — a reader with nowhere to read files still has somewhere to write
  // (`AMB-T-3690`).
  if (projectId !== null && tab === "memo") {
    return (
      <div className="files" tabIndex={-1} onKeyDown={onKey}>
        {top}
        {tabs}
        <MemoPage projectId={projectId} />
      </div>
    );
  }

  // Nothing opened yet. The column is where a file is read and no longer where one is found — the
  // tree is in the rail (`AMB-D-835`) — so what stands here is the line saying where to press,
  // rather than a list this column no longer holds.
  if (projectId === null || reading === null) {
    return (
      <div className="files files--empty" tabIndex={-1} onKeyDown={onKey}>
        {top}
        {tabs}
        <p className="files__none">{t("files.nothingOpen")}</p>
      </div>
    );
  }

  return (
    // Focusable so the column can hold the key it hears, and taken off the tab order so that being
    // able to hold it costs nobody a stop on the way past (`AMB-D-780`).
    <div className="files" tabIndex={-1} onKeyDown={onKey}>
      {top}
      {/* Drawn at every width, the narrow one included: a reader who cannot see what is open is a
          reader holding files they have no way back to, and that is worse in a narrow column rather
          than better (`AMB-D-835`). */}
      {tabs}
      <FileReader
        projectId={projectId}
        root={reading.root}
        path={reading.path}
        onBack={onBack}
        onOpenLedger={onOpenLedger}
        // The file on the screen and never what is picked out in the rail: the reading column is
        // about one file, and a bin pressed here is about the one being read.
        onTrash={() => trash.askTrash(reading.root, [reading.path])}
        onKey={onKey}
        aside={trash.aside}
        onHandOver={onHandOver}
      />
    </div>
  );
}

/**
 * The files this column is holding, as a row of tabs.
 *
 * **The row scrolls sideways; it is not paged.** An arrow at each end would take something like 40px
 * of a 190px row to say what the scroll already says, and what it buys is a reader pressing one of
 * them repeatedly. One control at the end lists everything by name instead — which is also the
 * answer for the tab that is off the end of the row (`AMB-D-835`).
 *
 * **A wheel with no sideways axis still moves it.** A trackpad has one and most mice do not, so a
 * plain vertical wheel over this row is read as a sideways one — otherwise the row is unreachable on
 * a machine whose pointer cannot ask for it.
 *
 * **The file that comes up brings itself into view.** Marking it and leaving it off the end of the
 * row would be a face saying which tab is on to a reader who cannot see it.
 */
function FileTabs({ open, showing, memo, onMemo, onPick, onCloseTab }: {
  open: readonly OpenFile[];
  showing: OpenFile | null;
  /** Whether the draft page is the one on top. */
  memo: boolean;
  onMemo: () => void;
  onPick: (at: OpenFile) => void;
  onCloseTab: (at: OpenFile) => void;
}) {
  const strip = useRef<HTMLDivElement | null>(null);
  // Where the list of everything open was asked for, drawn like the row menu because it is the same
  // kind of thing: a short list of answers to one question, at the control that asked it.
  const [listing, setListing] = useState<{ x: number; y: number } | null>(null);
  const on = showing === null ? null : openKey(showing);

  useEffect(() => {
    strip.current?.querySelector('[aria-current="true"]')
      ?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [on]);

  return (
    <div className="files__tabs">
      <div
        className="files__strip"
        ref={strip}
        onWheel={(e) => {
          // Left alone where the pointer already has a sideways axis: a trackpad's own sideways
          // swipe arrives as `deltaX` and the row scrolls on it without anything here.
          if (e.deltaX !== 0 || e.deltaY === 0 || strip.current === null) return;
          strip.current.scrollLeft += e.deltaY;
        }}
      >
        {/* First, and never closed: the draft page is the project's and is always there, where a
            file is one a reader opened and can let go of (`./MemoPage`). */}
        <span className={`files__tab${memo ? " files__tab--on" : ""}`}>
          <button
            className="files__tabname"
            aria-current={memo ? "true" : undefined}
            onClick={onMemo}
          >
            {t("files.memo")}
          </button>
        </span>
        {open.map((one) => {
          const key = openKey(one);
          return (
            <span key={key} className={`files__tab${key === on ? " files__tab--on" : ""}`}>
              {/* The path in the title and the name on the tab: two files of the same name in two
                  folders are two tabs reading alike, and where each came from is what tells them
                  apart. */}
              <button
                className="files__tabname"
                aria-current={key === on ? "true" : undefined}
                title={one.path.join("/")}
                onClick={() => onPick(one)}
              >
                {one.path[one.path.length - 1] ?? ""}
              </button>
              <button
                className="files__tabclose"
                title={t("pane.close")}
                onClick={() => onCloseTab(one)}
              >
                <Icon name="close" />
              </button>
            </span>
          );
        })}
      </div>
      <button
        className="files__more"
        title={t("files.openFiles")}
        aria-label={t("files.openFiles")}
        onClick={(e) => setListing({ x: e.clientX, y: e.clientY })}
      >
        <Icon name="more" />
      </button>
      {listing !== null && (
        <Menu at={listing} onClose={() => setListing(null)}>
          {open.map((one) => (
            <MenuItem key={openKey(one)} onClick={() => { setListing(null); onPick(one); }}>
              {one.path[one.path.length - 1] ?? ""}
            </MenuItem>
          ))}
        </Menu>
      )}
    </div>
  );
}

/**
 * Following something from this face means leaving it: what a record opens on is the ledger, and a
 * click that selected it behind this face would look like a link that did nothing (`AMB-D-747`).
 */
function useLedgerNav(onOpenLedger?: () => void): RefNav {
  const outer = useRefNav();
  return useMemo(() => ({
    selectTask: (id: number) => { onOpenLedger?.(); outer.selectTask?.(id); },
    selectDecision: (id: number | null) => { onOpenLedger?.(); outer.selectDecision?.(id); },
  }), [outer, onOpenLedger]);
}


/**
 * How a file's lines end, once it is a thing a save can be asked for.
 *
 * `null` is the file that has both kinds and has not been asked about yet — the one state where a
 * save is refused for a reason that is not about the file being unsavable (`AMB-D-773`).
 */
type Newline = "lf" | "crlf" | null;

/**
 * Whether a refusal is the host saying the file moved under the reader (`crate::folder_save`).
 *
 * It is the one save refusal the panel acts on rather than prints: every other is a sentence and a
 * reader who can try again, where this one has an answer of its own to offer (`AMB-D-784`).
 */
function changedUnderneath(e: unknown): boolean {
  return typeof e === "object" && e !== null
    && (e as { code?: unknown }).code === "folder_changed_underneath";
}

/** One file, as far as a panel can show it. */
function FileReader({
  projectId, root, path, onBack, onOpenLedger, onTrash, onKey, aside, onHandOver,
}: {
  projectId: number;
  root: string;
  path: string[];
  onBack: () => void;
  onOpenLedger?: () => void;
  /** Send the file being read to the machine's bin. The panel takes it off the screen from there. */
  onTrash: () => void;
  /** Undo, heard here for the same reason it is heard on the list: a file can go to the bin from
   *  this state too (`./FilesPanel`). */
  onKey: (e: ReactKeyboardEvent) => void;
  /** The question about the bin and the last refusal, both of which outlive this state. */
  aside: ReactNode;
  /** Hand this file to the pane being worked in, where there is one (`./FilesPanel`). */
  onHandOver?: (wholes: string[]) => void;
}) {
  const [file, setFile] = useState<FolderFileDto | null>(null);
  // Why the file did not open, in the reader's own language. A link is not a broken file: the host
  // refuses one on purpose (`AMB-D-782`), and a person sharing a `CLAUDE.md` between projects that
  // way is the first to meet it — so that refusal is drawn in its own words and everything else
  // keeps the one sentence there is nothing finer to say than.
  //
  // **`onward` is whether the file may still be handed to the machine.** It travels with the
  // sentence because the two refusals do not answer it the same way: a file that could not be read
  // is one another application may well open, and a link is one this face declined to follow —
  // handing it out would be following it after all.
  const [failed, setFailed] = useState<{ said: string; onward: boolean } | null>(null);
  // The encoding the reader named, once they have. Nothing until then: the host's guess is right
  // for 644 files in 645, and asking for one up front would be putting the question to everybody
  // to catch the one (`AMB-D-773`).
  const [asked, setAsked] = useState<string | undefined>(undefined);
  // Where the list of encodings was opened from, drawn like the file menu because it is the same
  // kind of thing: a short list of answers to one question, at the control that asked it.
  const [picking, setPicking] = useState<{ x: number; y: number } | null>(null);
  // The way to read what is in the editor, handed over once it is up. Nothing is saved before that:
  // the editor is where the text is (`./FileEditor`).
  const typed = useRef<(() => string) | null>(null);
  // Whether there is anything to save. It is set by the editor telling this side that a person
  // typed, rather than by comparing texts — the comparison would mean holding a second copy of the
  // document up here and reading it on every keystroke.
  const [edited, setEdited] = useState(false);
  const [keeping, setKeeping] = useState(false);
  // Why the last save did not happen, in the reader's own language. Cleared when another is tried.
  const [refused, setRefused] = useState<string | null>(null);
  // Which newline to write. A file with one kind keeps it; a file with both has none until the
  // reader picks, and the save waits for that rather than guessing.
  const [newline, setNewline] = useState<Newline>(null);
  // Whether the file moved under a reader who has typed. Nothing of theirs is taken away by it —
  // which of the two texts stands is a thing they say, and this is the asking (`AMB-D-784`). The
  // saying has two answers: take the disk's, or write their own over it (`AMB-D-863`).
  const [stale, setStale] = useState(false);
  // Where a file this face would not draw was handed on to the machine from. The same menu the list
  // rows open, opened here because these are the states a reader reaches it from with no row under
  // the pointer.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  // Whether a Markdown file is being shown as the text it is rather than as what that text draws.
  // **It goes back with every file opened**, deliberately: what a person opens a Markdown file for
  // is to read it, and a choice that outlived the file would be a setting nobody set — one edit and
  // every Markdown file afterwards opens as source, the ones they only wanted to read included.
  const [asText, setAsText] = useState(false);
  const name = path[path.length - 1];
  // The one thing the name decides, and the only file there are two ways to show (`MARKDOWN`).
  const markdown = MARKDOWN.some((ext) => name.toLowerCase().endsWith(ext));

  // A different file is a different question: what the reader named was this file's encoding, and
  // carrying it to the next one would open that one in an encoding nobody chose for it.
  useEffect(() => setAsked(undefined), [projectId, root, path.join("/")]);

  // What the file was as it was last read, and whether there is anything of the reader's to lose by
  // replacing it. Held in a ref rather than read out of the effect below: that effect is subscribed
  // once per file, and taking these as reasons to re-subscribe would install a fresh watch over the
  // folder the first time somebody typed.
  const held = useRef({ edited, digest: file?.digest });
  held.current = { edited, digest: file?.digest };

  // Why a read did not answer, and whether the file may still be handed on from where it stopped.
  const unanswered = (e: unknown) => (
    isErr(e, "folder_link")
      ? { said: errText(e), onward: false }
      : { said: t("files.unreadable"), onward: true }
  );

  // One file as it has just been read. The newline travels with the text because a file read again
  // is a file whose lines may end differently than they did — and both kinds in one file is the one
  // answer this side cannot act on by itself.
  const take = (one: FolderFileDto) => {
    setFile(one);
    setNewline(one.lineEnding === "mixed" ? null : one.lineEnding);
  };

  useEffect(() => {
    let alive = true;
    setFile(null);
    setFailed(null);
    setAsText(false);
    setEdited(false);
    setRefused(null);
    setNewline(null);
    setStale(false);
    void folderRead(projectId, root, path, asked)
      .then((one) => { if (alive) take(one); })
      .catch((e) => {
        if (alive) setFailed(unanswered(e));
      });
    return () => { alive = false; };
  }, [projectId, root, path.join("/"), asked]);

  // The file moving under the reader while they have it open.
  //
  // **This face watches the folder itself**, because the tree that was watching it is not on the
  // page while a file is being read — the panel draws one or the other. What arrives says only that
  // the folder moved (`AMB-D-785`), so the answer is to read the file again and compare the mark:
  // the same mark is this file standing still while something else in the folder changed, which is
  // most of what arrives here and draws nothing at all.
  //
  // **A reader who has typed nothing is simply shown what the file says now.** This panel sits
  // beside an agent that edits the same files, and a reader looking at what it changed an hour ago
  // reads it as the agent having done nothing (`AMB-D-784`).
  //
  // **A picture travels this road too and needs nothing of its own** (`AMB-D-797`). It has a mark
  // like any other file, nobody can have typed into it, and what redraws it is the address it is
  // fetched from carrying that mark. What is not watched is what is not drawn: a picture refused
  // for its size, and a binary.
  const tracked = file?.digest !== undefined;
  useEffect(() => {
    if (!tracked) return;
    let alive = true;
    const look = () => {
      // In the encoding the reader named, where they named one: a file read again in a guess they
      // had already overruled would put the panel back where they started (`AMB-D-773`).
      void folderRead(projectId, root, path, asked)
        .then((fresh) => {
          if (!alive || fresh.digest === undefined || fresh.digest === held.current.digest) return;
          if (held.current.edited) setStale(true);
          else take(fresh);
        })
        // A read that did not answer leaves what is drawn where it is. The file may be being
        // written this very instant, and taking a reader's text off the screen for a moment of the
        // disk's is worse than being a moment out of date — a file that has really gone is what the
        // save then says, to somebody who asked for it.
        .catch(() => {});
    };
    // Subscribed before the watch is asked for, the same order the tree takes: the first thing the
    // folder does could happen while the host is still walking it.
    const listening = onFolderChanged((changes) => { if (alive && changes.root === root) look(); });
    void folderWatch(projectId, root).catch(() => {});
    return () => {
      alive = false;
      void listening.then((stop) => stop());
      void folderUnwatch(root);
    };
  }, [projectId, root, path.join("/"), tracked, asked]);

  // Taking what is on the disk now, over what the reader has typed. It is the one thing here that
  // loses somebody's work, which is why nothing does it on their behalf (`AMB-D-784`).
  const readAgain = () => {
    void folderRead(projectId, root, path, asked)
      .then((fresh) => { take(fresh); setEdited(false); setStale(false); setRefused(null); })
      .catch((e) => setFailed(unanswered(e)));
  };

  // Whether this file is one the panel can write back at all. The host says so before a reader has
  // typed a character: a file cut at the read cap, or one whose bytes and text do not round-trip,
  // is drawn read-only from the start (`AMB-D-773`).
  //
  // **A Markdown file being drawn is not one of them.** There is no editor on the rendering, so
  // there is no text to write and nothing a save could mean — the switch beside the name is what
  // makes it savable, by putting the text on the screen.
  const savable = file?.text !== undefined
    && file.encoding !== undefined
    && !file.truncated
    && file.clean
    && (!markdown || asText);

  // Whether the reader's own text is a thing that could be written over the file at all. It is what
  // the save control asks minus the mark — the one thing the offer below replaces — so a file this
  // panel could not write back, or one whose newline nobody has picked, has no such offer on it
  // rather than a control that would refuse the press.
  const overwritable = savable && newline !== null && file?.digest !== undefined;

  // **A file already known to have moved is not sent to the door a second time.** The mark this
  // panel holds is the one the host refuses, so the press would spend a round trip and land back on
  // the state the reader is already looking at, having said nothing about having been heard
  // (`AMB-T-4401`). What the reader has is the offer below, and the control above says the same by
  // being shut — the same shape a file with both kinds of newline is held in.
  const save = async () => {
    const read = typed.current;
    if (!savable || keeping || stale || file?.encoding === undefined || file.digest === undefined
      || read === null || newline === null) return;
    setKeeping(true);
    setRefused(null);
    try {
      const kept = await folderSave(
        projectId, root, path, read(), file.encoding, file.bom, newline, file.digest,
      );
      setEdited(false);
      // What is on the disk now has one kind of newline, so the question is not asked again, and it
      // is the mark this save came back with — without taking that, the panel's next look at the
      // folder would find its own writing and read it as somebody else's (`AMB-D-784`). The text is
      // left where it is: replacing it would be handing the editor its own document back and moving
      // the caret to the top for the trouble.
      setFile({ ...file, lineEnding: newline, digest: kept });
    } catch (e) {
      // The one refusal that is not a sentence to read and be done with: the file moved under the
      // reader, and what that wants is the offer below rather than a line of prose.
      if (changedUnderneath(e)) setStale(true);
      else setRefused(errText(e));
    } finally {
      setKeeping(false);
    }
  };

  // Taking what the reader has typed, over what is on the disk now.
  //
  // **The file is read again for its mark, and for nothing else.** The mark this panel is holding
  // is the one the door refuses (`AMB-T-4401`), so a save carrying it would spend a round trip to
  // be told what the reader was just told; what the door will write over is the file as it stands
  // now, and this is the reading that asks what that is. Nothing of what came back is drawn — the
  // whole of this press is the reader saying they want their own text and not the disk's
  // (`AMB-D-863`).
  //
  // **It is the file, not the lines.** Which of the two texts stands is the question, and taking
  // some lines from each is not one of the answers — a reader who wants that takes one side and
  // edits it.
  const keepMine = async () => {
    const read = typed.current;
    if (!overwritable || keeping || file?.encoding === undefined || file.digest === undefined
      || read === null || newline === null) return;
    setKeeping(true);
    setRefused(null);
    try {
      const fresh = await folderRead(projectId, root, path, asked);
      const kept = await folderSave(
        projectId, root, path, read(), file.encoding, file.bom, newline,
        // A file that came back without a mark is one this panel could not write back at all any
        // more. The mark it is holding goes instead of a guess, and the door is what says no to it.
        fresh.digest ?? file.digest,
      );
      setEdited(false);
      setStale(false);
      // The same mark the ordinary save takes, for the same reason: without it the panel's next
      // look at the folder would find its own writing and read it as somebody else's. The text is
      // left where it is — it is the reader's own, and it is what was just written.
      setFile({ ...file, lineEnding: newline, digest: kept });
    } catch (e) {
      // Somebody wrote to the file between that reading and this save, or the encoding has no room
      // for a character in it. The offer is already on the screen, so what is added is the sentence
      // saying this press wrote nothing.
      setRefused(errText(e));
    } finally {
      setKeeping(false);
    }
  };

  // The keystroke everything else in the world saves with. It is taken on the window rather than
  // inside the editor because the reader may have clicked away from it — and it is taken only
  // while there is something to save, so nothing is swallowed on a file that cannot be.
  useEffect(() => {
    if (!savable) return;
    const key = (e: KeyboardEvent) => {
      if (e.key !== "s" || !(e.metaKey || e.ctrlKey) || e.altKey) return;
      e.preventDefault();
      void save();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  // A reference in a file is a live link or it is nothing at all (`AMB-D-747`), and following one
  // leaves this face: what a record opens on is the ledger.
  const nav = useLedgerNav(onOpenLedger);

  // What the row under the name has on it. Named here rather than asked three times in the markup,
  // because whether the row exists at all is the same question as whether anything would be on it.
  const switchable = file?.text !== undefined && markdown;
  const readAs = file?.text !== undefined && file.encoding !== undefined;

  // The way on out of a file this face does not draw — the same menu the tree rows carry, opened
  // here because these are the states a reader reaches it from with no row under the pointer. It is
  // written once and worn by all three refusals: a line that only says no leaves the reader holding
  // a file they still want opened, and which of the three stopped it does not change the answer.
  const onward = (
    <button
      className="files__hand"
      onClick={(e) => setMenu({ x: e.clientX, y: e.clientY })}
    >
      {t("files.openElsewhere")}
    </button>
  );

  return (
    <div className="files files--reading" tabIndex={-1} onKeyDown={onKey}>
      {/* **The name has a row to itself, and what to do with the file has another.** The two used to
          share one, and the name was the only thing on it that could give way — every control beside
          it is as wide as its own words — so the name is what disappeared: `run.sh` came up as
          `r...` on a panel of ordinary width, which leaves a reader unable to say which file they
          are looking at. It got that way one control at a time, and no one of them was the mistake
          (`AMB-T-3866` measured the state the three arrived at).

          The split is by what a control is for, not by what fits: the way off this file stays with
          the name, and the ones that act on the file stand together under it. The second row is
          drawn only where there is something to put on it, so a picture — which has nothing to
          switch, nothing to reopen and nothing to save — costs no line at all.

          **What ends the column itself is not on either of them.** The width and the way out are the
          column's own rather than this file's, and they stand on the top row every state has
          (`./FilesPanel`) — down here they were in a different place for a file than for the draft
          page (`AMB-T-4271`).

          The bin stays up here though it acts on the file, because it is a mark and not a word: an
          icon is the same narrow width whatever the reader's language, which is exactly what the
          three that moved were not. */}
      <div className="files__bar">
        <button className="files__back" onClick={onBack}>{t("files.closeFile")}</button>
        <span className="files__name" title={path.join("/")}>{name}</span>
        <button className="files__trash" title={t("files.trash")} onClick={onTrash}>
          <Icon name="trash" />
        </button>
      </div>
      {(switchable || readAs || savable) && (
        <div className="files__tools">
          {/* Drawn for a Markdown file and for nothing else: every other file has one way to be
              shown, and a switch with nowhere to switch to is a control that answers nothing. What
              it says is where it goes rather than where it is — the reader can see where they
              are. */}
          {switchable && (
            <button className="files__view" onClick={() => setAsText((was) => !was)}>
              {t(asText ? "files.read" : "files.edit")}
            </button>
          )}
          {/* What the bytes were read as. The guess reports no confidence and breaks nothing visible
              when it is wrong, so the reader is the only one who can catch it — and they can only
              catch it if they are told what was guessed (`AMB-D-773`). Text only: a picture has no
              encoding to be wrong about. */}
          {file?.text !== undefined && file.encoding !== undefined && (
            <button
              className="files__encoding"
              title={t("files.reopenWith")}
              onClick={(e) => setPicking({ x: e.clientX, y: e.clientY })}
            >
              {file.encoding}
              {" · "}
              {file.lineEnding === "mixed" ? t("files.lineEndingMixed") : file.lineEnding.toUpperCase()}
            </button>
          )}
          {/* One control saying which of three things is true, rather than a button and a word
              somewhere else for a reader to find the answer in. It is shut while the file is known
              to have moved under the reader: a save that cannot be taken is not a press to offer,
              and why it cannot is already on the page below it (`AMB-D-784`). */}
          {savable && (
            <button
              className="files__keep"
              disabled={!edited || keeping || newline === null || stale}
              onClick={() => { void save(); }}
            >
              {keeping ? t("files.saving") : edited ? t("files.save") : t("files.saved")}
            </button>
          )}
        </div>
      )}
      {aside}
      <div className="files__body">
        {failed !== null && (
          <>
            <p className="files__none">{failed.said}</p>
            {failed.onward && onward}
          </>
        )}
        {/* The picture is fetched rather than carried: `folderRead` says only that there is one
            and what type it is, and the door that hands out a file by its path is addressed with
            the same project, folder and path this reader was opened on (`AMB-D-783`). It draws
            top to bottom as it arrives, where a `data:` URL drew all at once or not at all.

            The mark goes on the address so that the picture is fetched again when — and only
            when — the file behind it moved (`AMB-D-797`). Without it the address of a rewritten
            picture is the address of the old one, and the reader watches an agent redraw a diagram
            that never changes on screen. */}
        {file?.image !== undefined && (
          <img
            className="files__image"
            alt={name}
            src={fileUrl(projectId, root, path, file.image.mime, file.digest)}
          />
        )}
        {/* The text is what the file holds and the rendering is a view of it (`AMB-D-41`), so the
            editor is reachable for a Markdown file too — otherwise the one kind of file an agent
            writes most is the one kind nobody could correct. */}
        {file?.text !== undefined && (
          markdown && !asText
            ? <RefNavProvider value={nav}><Markdown>{file.text}</Markdown></RefNavProvider>
            : (
              <FileEditor
                text={file.text}
                editable={!file.truncated && file.clean}
                name={name}
                onEdit={() => setEdited(true)}
                hold={(read) => { typed.current = read; }}
              />
            )
        )}
        {/* Said before the reader types rather than after they press save: a file with both kinds
            of newline comes out of a save with one, and that is a change to every line of the other
            kind (`AMB-D-773`).
            The choice sits with the sentence that explains it rather than up in the bar — the bar
            is as wide as the panel, and a control there would push the file's own name off it. */}
        {savable && file?.lineEnding === "mixed" && (
          <div className="files__newlines">
            <p className="files__none">{t("files.newlinesMixed")}</p>
            <select
              className="files__newline"
              aria-label={t("files.newlineChoose")}
              value={newline ?? ""}
              onChange={(e) => setNewline(e.target.value === "crlf" ? "crlf" : "lf")}
            >
              <option value="" disabled>{t("files.newlineChoose")}</option>
              <option value="lf">{t("files.newlineLf")}</option>
              <option value="crlf">{t("files.newlineCrlf")}</option>
            </select>
          </div>
        )}
        {/* The file moved under the reader while they were typing in it. What is said is the fact,
            and under it the two answers there are: write their own text over the file, or take
            what the disk says and lose theirs. The panel settles it rather than handing it to the
            agent in the pane, which cannot see an editor nobody has saved out of (`AMB-D-863`). */}
        {stale && (
          <div className="files__changed">
            <p className="files__none">{t("files.changedUnderneath")}</p>
            {overwritable && (
              <button className="files__mine" onClick={() => void keepMine()} disabled={keeping}>
                {t("files.keepMine")}
              </button>
            )}
            <button className="files__reread" onClick={readAgain}>{t("files.readAgain")}</button>
          </div>
        )}
        {refused !== null && <p className="files__none">{refused}</p>}
        {/* A picture refused is not a picture missing. Drawn as nothing at all it reads as a
            damaged file, so the refusal says what it measured and hands the file on to something
            built to open it (`AMB-D-783`). */}
        {file?.oversize !== undefined && (
          <>
            <p className="files__none">{t("files.tooBig")}</p>
            <p className="files__none">{measured(file.oversize)}</p>
            {onward}
          </>
        )}
        {file !== null && file.text === undefined && file.image === undefined
          && file.oversize === undefined && (
          <>
            <p className="files__none">{t("files.notText")}</p>
            {onward}
          </>
        )}
        {file?.truncated === true && <p className="files__none">{t("files.cut")}</p>}
      </div>
      {menu !== null && (
        <FileMenu
          projectId={projectId}
          root={root}
          path={path}
          // The file being read, and nothing picked out behind it: this menu is opened on the face
          // showing one file (`FilesPanel`).
          about={[path]}
          dir={false}
          at={menu}
          onClose={() => setMenu(null)}
          onTrash={onTrash}
          onHandOver={onHandOver}
        />
      )}
      {picking !== null && (
        <EncodingMenu
          at={picking}
          onPick={(one) => { setPicking(null); setAsked(one); }}
          onClose={() => setPicking(null)}
        />
      )}
    </div>
  );
}

/**
 * The encodings a file can be reopened in, as a list to pick from.
 *
 * **This is the items, not the box** — the same shell the file rows' menu wears
 * (`../components/Menu`). Written on its own it closed on every key, which is the bug `AMB-D-780`
 * took out of the other one and left standing here: a reader walking the list with the arrows shut
 * it on the way past.
 *
 * **The list comes from the host.** Which encodings may be offered is which ones can be written
 * back, and that is `crate::encoding`'s to say — a copy kept here would go on offering one the day
 * it stopped being written (`AMB-D-773`). It arrives after the box is drawn, so the names are what
 * the shell is told its face is: the item the reader was standing on is gone the moment they land.
 *
 * A file that is not clean is still on this road, and is the road's whole point: a guess that went
 * wrong is exactly the file whose bytes and text no longer say the same thing.
 */
function EncodingMenu({ at, onPick, onClose }: {
  at: { x: number; y: number };
  onPick: (encoding: string) => void;
  onClose: () => void;
}) {
  const [names, setNames] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    void folderEncodings()
      .then((found) => { if (alive) setNames(found); })
      .catch(() => { if (alive) onClose(); });
    return () => { alive = false; };
  }, []);

  return (
    <Menu at={at} face={names} onClose={onClose}>
      {names.map((one) => (
        <MenuItem key={one} onClick={() => onPick(one)}>{one}</MenuItem>
      ))}
    </Menu>
  );
}

/**
 * What a refused picture is refused for, in the two numbers that were measured.
 *
 * The pixels are absent where the front of the file did not say — a picture that would not say its
 * size is refused on its bytes alone (`crate::folder`), and printing a size nobody read would be
 * inventing one.
 */
function measured(oversize: NonNullable<FolderFileDto["oversize"]>): string {
  const size = fileSize(oversize.bytes);
  if (oversize.width === undefined || oversize.height === undefined) return size;
  return `${size} · ${tf("files.tooBigPixels", {
    width: formatNumber(oversize.width),
    height: formatNumber(oversize.height),
  })}`;
}

/**
 * A file's size, in the unit that says something about it.
 *
 * **Megabytes alone would print "0 MB" for the case this exists to explain.** A picture is refused
 * on pixels as well as bytes, and the pictures that cost the most to draw are the ones that
 * compress best — ten kilobytes of lossless WebP decodes to over a gigabyte (`AMB-D-783`), and a
 * header claiming thirty thousand square costs less than a kilobyte to write. A refusal that reads
 * "0 MB" tells the reader the file is empty, which is the opposite of true, so the unit goes down
 * as far as bytes rather than ever rounding to nothing.
 *
 * The unit's own name comes from `Intl` rather than the dictionary: it is one of the things a
 * locale already knows how to write, down to `Mo` in French.
 */
function fileSize(bytes: number): string {
  const mib = 1024 * 1024;
  if (bytes >= mib) return formatNumber(bytes / mib, unit("megabyte", 1));
  if (bytes >= 1024) return formatNumber(bytes / 1024, unit("kilobyte", 0));
  return formatNumber(bytes, unit("byte", 0));
}

function unit(name: string, maximumFractionDigits: number): Intl.NumberFormatOptions {
  return { style: "unit", unit: name, unitDisplay: "short", maximumFractionDigits };
}
