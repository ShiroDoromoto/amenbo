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

export function FilesPanel({
  projectId, tab, reading, onBack, onGone, onClose, onOpenLedger, onHandOver,
}: {
  /** The project the file belongs to; nothing is drawn without one. */
  projectId: number | null;
  /**
   * Which of the two halves is up.
   *
   * **The switch is the terminal face's top row and nowhere else.** This column drew tabs of its own
   * as well, and two controls that do the same thing leave a reader looking for the right one; the
   * row that stayed is the one that is also there while the column is closed
   * (`../shell/TerminalFace`).
   */
  tab: "files" | "memo";
  /** The file to draw, or nothing where none has been opened (`./FolderTree`). */
  reading: { root: string; path: string[] } | null;
  /** Leave the file — one layer of Escape, and the way back off the reading face. */
  onBack: () => void;
  /** The rows that have gone to the bin, so the tree in the rail hears about a file binned here. */
  onGone?: (root: string, went: string[]) => void;
  /** Put the column away. What opens it again is the top row, which is where it was opened from. */
  onClose: () => void;
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
   * The keys this column hears, rather than the window: the terminal beside it has its own idea of
   * what each of them means, and the boundary between the two is which of them the reader is in
   * (`AMB-D-780`).
   *
   * **One press, one layer.** The file goes first and the column itself after it, so the two things
   * a reader might mean by "back" are told apart by how many times they press rather than by
   * finding a different way out of each (`AMB-D-815`).
   */
  const onKey = (e: ReactKeyboardEvent) => {
    if (e.key === "Escape") {
      // Not this column's to take while something inside it is already answering to the same key: a
      // menu and the question before a bin both close on Escape, and a press counted twice would
      // carry the reader a layer past the one they asked for.
      if (trash.asking || (e.target as HTMLElement).closest('[role="menu"]') !== null) return;
      e.preventDefault();
      if (reading !== null) onBack();
      else onClose();
      return;
    }
    if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() === "z") {
      e.preventDefault();
      trash.undo();
    }
  };

  // The way to put the column away, and the whole of the row it sits on. It is drawn in every state
  // the column can be in — reading a file included — because a column that could only be closed
  // from one of its states is one a reader has to find their way back out of.
  const close = (
    <button className="files__close" title={t("pane.close")} onClick={onClose}>
      <Icon name="close" />
    </button>
  );

  // The draft page is the project's, and a project has one whether or not it is bound to a folder
  // (`./MemoPage`). So the half that is up is answered first, and only the files half goes on to ask
  // whether anything is open — a reader with nowhere to read files still has somewhere to write
  // (`AMB-T-3690`).
  if (projectId !== null && tab === "memo") {
    return (
      <div className="files">
        <div className="files__top">{close}</div>
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
        <div className="files__top">{close}</div>
        <p className="files__none">{t("files.nothingOpen")}</p>
      </div>
    );
  }

  return (
    // Focusable so the column can hold the key it hears, and taken off the tab order so that being
    // able to hold it costs nobody a stop on the way past (`AMB-D-780`).
    <div className="files" tabIndex={-1} onKeyDown={onKey}>
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
        close={close}
        aside={trash.aside}
        onHandOver={onHandOver}
      />
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
  projectId, root, path, onBack, onOpenLedger, onTrash, onKey, close, aside, onHandOver,
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
  /** The panel's own way out, drawn on this row: reading a file is not a state a reader should have
   *  to leave before they can close the panel (`./FilesPanel`). */
  close: ReactNode;
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
  const [failed, setFailed] = useState<string | null>(null);
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
  // reading the file again is a thing they ask for, and this is the asking (`AMB-D-784`).
  const [stale, setStale] = useState(false);
  // Where a picture too large to draw was handed on to the machine from. The same menu the list
  // rows open, opened here because this is the one state a reader reaches it from with no row under
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
        if (alive) setFailed(isErr(e, "folder_link") ? errText(e) : t("files.unreadable"));
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
      .catch((e) => setFailed(isErr(e, "folder_link") ? errText(e) : t("files.unreadable")));
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

  const save = async () => {
    const read = typed.current;
    if (!savable || keeping || file?.encoding === undefined || file.digest === undefined
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

  return (
    <div className="files files--reading" tabIndex={-1} onKeyDown={onKey}>
      {/* **The name has a row to itself, and what to do with the file has another.** The two used to
          share one, and the name was the only thing on it that could give way — every control beside
          it is as wide as its own words — so the name is what disappeared: `run.sh` came up as
          `r...` on a panel of ordinary width, which leaves a reader unable to say which file they
          are looking at. It got that way one control at a time, and no one of them was the mistake
          (`AMB-T-3866` measured the state the three arrived at).

          The split is by what a control is for, not by what fits: leaving and closing are the frame's
          and stay with the name, and the ones that act on the file stand together under it. The
          second row is drawn only where there is something to put on it, so a picture — which has
          nothing to switch, nothing to reopen and nothing to save — costs no line at all.

          The bin stays up here though it acts on the file, because it is a mark and not a word: an
          icon is the same narrow width whatever the reader's language, which is exactly what the
          three that moved were not. */}
      <div className="files__bar">
        <button className="files__back" onClick={onBack}>{t("files.back")}</button>
        <span className="files__name" title={path.join("/")}>{name}</span>
        <button className="files__trash" title={t("files.trash")} onClick={onTrash}>
          <Icon name="trash" />
        </button>
        {close}
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
              somewhere else for a reader to find the answer in. */}
          {savable && (
            <button
              className="files__keep"
              disabled={!edited || keeping || newline === null}
              onClick={() => { void save(); }}
            >
              {keeping ? t("files.saving") : edited ? t("files.save") : t("files.saved")}
            </button>
          )}
        </div>
      )}
      {aside}
      <div className="files__body">
        {failed !== null && <p className="files__none">{failed}</p>}
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
        {/* The file moved under the reader while they were typing in it. What is said is the fact
            and nothing else, and what is offered is the one thing this panel can do about it:
            lining the two texts up is the work of the agent in the pane (`AMB-D-784`). */}
        {stale && (
          <div className="files__changed">
            <p className="files__none">{t("files.changedUnderneath")}</p>
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
            <button
              className="files__hand"
              onClick={(e) => setMenu({ x: e.clientX, y: e.clientY })}
            >
              {t("files.tooBigOpen")}
            </button>
          </>
        )}
        {file !== null && file.text === undefined && file.image === undefined
          && file.oversize === undefined && (
          <p className="files__none">{t("files.notText")}</p>
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
