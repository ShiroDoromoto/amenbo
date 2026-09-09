// One pane of the terminal face: a real terminal, drawn by xterm.js over a PTY the host holds.
//
// The pane is a drawing of a session, not the session. A terminal belongs to the process, so a pane
// can be taken away and put up again — in the window the user split it out into, back in the board
// when they folded it, on the page they turned back to (`./layout`), or in the interface a language
// change rebuilt around it — and the program inside it never learns that any of it happened
// (`AMB-D-753`). What a pane cannot carry across is the emulator's scrollback, so what it draws first
// is the tail the host kept (`crate::pty`).
//
// Nothing here interprets what crosses. A chunk of output arrives base64-encoded because the bytes
// are not text — an escape sequence is split wherever the host's read ended, and a multi-byte
// character with it — and it is handed to the emulator exactly as it came, which is the one thing
// that can put the split ones back together. Keystrokes go the other way just as plainly: what the
// emulator produced for the key is what the program in the terminal is given, so arrow keys, Ctrl-C,
// tab completion and bracketed paste all work because nothing tried to make them work.
//
// **Nothing is read out of a person's typing at all.** A pane's frame is named by the agent running
// in it or by the person saying so, and by nothing else (`./frames`).
//
// Refs are read **after** all of that, off what was drawn rather than out of what crossed
// (`./refLinks`), which is what keeps the line above true: the stream is still nobody's to read, and
// a cell already holds a character every escape has finished with. It is the one thing owning a
// terminal buys that a standard one cannot do — the characters in a pane are records to amenbo and
// a string to everything else — and it costs the stream nothing.

import { FitAddon } from "@xterm/addon-fit";
import { Terminal, type IBufferCell } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { PtyChunkDto, PtyReplayDto, PtySessionDto, SessionSaidDto } from "../bindings/bindings";
import { takesPastedFiles, takesPastedImages, writesPastedImage } from "../core/clipFiles";
import type { RefSpace } from "../core/idref";
import { invoke } from "../core/ipc";
import { openExternalUrl } from "../core/mutations";
import { hostOs, type HostOs } from "../core/platform";
import { tidiedCopy } from "./copied";
import { type NamedBy } from "./frames";
import { httpUrl, pathsOnRow, refFromUrl, refsOnRow, urlsOnRow, type Cell, type Rows } from "./refLinks";

// The events the host sends this pane. Output is a chunk; closed is the program in the terminal
// having exited, which arrives once and is the last thing that session says.
const OUTPUT_EVENT = "pty://output";
export const CLOSED_EVENT = "pty://closed";

// What the agent in this pane says about its session, as the host reads it out of the drop box it was
// given (`AMB-D-749`). It is the one thing crossing this seam that has been read rather than carried:
// the surface layer is a vocabulary, and a verb of it is exactly as good as its word.
export const SAID_EVENT = "session://said";

// The opening instruction was left in this pane's input box unsent (`crate::pty`). It arrives once,
// only for the ending a person can finish, and it carries the session's id and nothing else.
const UNSENT_EVENT = "pty://unsent";

/**
 * What a pane tells the window it is in. The window holds what is known about its sessions and what its
 * frames are called; a pane is where those things happen, not where they are kept.
 */
export type PaneEvents = {
  /** A terminal is running in this pane, under this session id, in `folder`, with `agent` in it. It
   *  is said of a terminal this pane adopted as much as of one it started: what the window holds is
   *  what is running in it, and a session that moved windows is running in the one it moved to.
   *
   *  `agent` is the id the program was started as, and null for a plain prompt. It comes off the
   *  session rather than off what this pane asked for, which is the only reading that holds for a
   *  pane that adopted one (`crate::pty`). */
  opened(session: string, folder: string | null, agent: string | null): void;
  /** A chunk has crossed and been drawn. Said per chunk and carrying nothing: what is read off it is
   *  the time it happened, which is the one thing about a stream that means the same for every program
   *  in a pane (`./moving`). The tail a pane is handed on picking a terminal up is not one of these —
   *  it is output being drawn again, not output arriving. */
  output(): void;
  /** The agent said something about its session. */
  said(statement: SessionSaidDto): void;
  /** A file path drawn in this pane was clicked, as it was drawn. Where it leads is not the pane's to
   *  say: a relative one is read against the folder this session is in, and only the window knows
   *  whether that lands inside the folder the file face is rooted at (`AMB-T-3630`). */
  path(target: string): void;
  /** The program in the terminal has exited. Nothing running is kept. */
  closed(session: string): void;
  /** This frame has settled where it works, before anything is running there — the person chose a
   *  folder (`./agent`). It is said of the choice and not of the terminal because the two can be a
   *  long way apart, and a page that waited for a started terminal would ask its other slots again. */
  chose(folder: string): void;
  /** Something has named this pane's frame. Whether the name takes is the store's to say — a person's
   *  name for a frame is not taken back off it by the agent (`./frames`). */
  name(name: string, by: NamedBy): void;
  /**
   * How large the terminal is now, in characters — said whenever that changes, and once when the
   * pane comes up.
   *
   * **It is the one measurement only the emulator has.** What is drawn in a pane is a grid, and how
   * many rows fit in a box is a question about the font the emulator measured, not about the box: a
   * pane that wanted to keep the terminal above some number of rows cannot work that out from its
   * own pixels (`../shell/TerminalPane`). Optional because most panes never ask — a face that only
   * draws one has nothing to do with the answer.
   */
  sized?(cols: number, rows: number): void;
};

/**
 * Which terminal this pane draws — and, where one has to be started, where it opens and with what
 * running in it (`./agent`).
 *
 * A pane comes up for reasons it cannot tell apart from the inside: a person asked for a terminal
 * here, or the pane that had one moved, or the page it is on came back round. What the window holds is
 * which slot had what (`./layout`), so it says; a pane left to work it out would have to guess, and
 * there is nothing to guess from.
 *
 * The last two fields are only ever read when a terminal is **started**. A pane that takes up one
 * already running takes it as it is — the folder it is standing in and the program in it were settled
 * when it started, and a pane moving between windows or pages does not restart anything (`AMB-D-753`).
 */
export type PaneStart = {
  /** The terminal this slot already had. Taken up again where it is still running. */
  session?: string | null;
  /**
   * Whether a terminal running with no pane drawing it may be taken up here. It is how a session comes
   * back from the window it was split out into, so exactly one pane may offer: the slot that is the
   * terminal's home when the app is one window (`AMB-D-753`).
   */
  adopt?: boolean;
  /** The folder the shell starts in — canonical, as `wake_probe` answered with it. Panes on one page
   *  are opened in one folder, which is what keeps a screen to a single project (`./layout`); a page
   *  that has none yet is one where nothing has been started, and what the frame there puts up is the
   *  way to choose one (`./agent`). */
  cwd?: string | null;
  /**
   * The catalogued id of the agent to start (`crate::wake`). A pane with none is a bare prompt.
   * What crosses is the id and never a command line: the catalog on the host side turns it into one,
   * so nothing here can name a program.
   */
  agent?: string | null;
};

/**
 * The pane's own shell, standing among the agents as one more thing to start — the one value of
 * {@link PaneStart.agent} that is not a catalogued id.
 *
 * It is not an agent and has no row in the catalogue (`amenbo_core::harness`), so what a face passes
 * around is a *choice* rather than an agent id, and this is the one choice the catalogue does not
 * answer to. `pty_open` already opens a bare prompt for a pane it is given no agent for, so the id
 * is turned back into "none" at the single place a terminal is started (`./agent`).
 *
 * It is put wherever a face puts a choice, and nowhere else. A project that settled on one agent
 * still opens on it with nothing asked (`AMB-T-3606`) — the shell is something to reach for, not a
 * question to answer on the way in. It is never written down as the project's answer either
 * (`wake_remember`): "which agent do you work with here" is not a question a shell answers.
 */
export const SHELL = "shell";

/**
 * How many lines the box under a pane grows to before what is written scrolls inside it
 * (`AMB-D-864`).
 *
 * It is a cap on the box and not on what may be sent: a paragraph pasted in is still one line to
 * send, and a box that grew to hold it would leave a pane with no terminal in it. Ten is where a
 * written message stops being a line and starts being a document.
 */
const BOX_LINES = 10;

/**
 * How many rows the terminal keeps, whatever is written in the box below it (`AMB-D-864`).
 *
 * Below this a full-screen interface has nowhere to put what it is asking — the choices of a
 * first-run question, a diff, a menu — and a person writing a line would be answering something they
 * can no longer see. So the box folds here rather than the terminal.
 */
const FLOOR_ROWS = 8;

/**
 * How tall the box under a pane is to be, given what is written in it and the room the terminal has
 * to give up (`AMB-D-864`). Every length is in pixels but `rows`, which is in characters.
 *
 * **Two things stop the box growing, and the box is what folds to either.** Its own cap is
 * {@link BOX_LINES} lines, past which what is written scrolls inside it; the terminal's is
 * {@link FLOOR_ROWS} rows, which it keeps whatever is written below it. A pane in a face split eight
 * ways meets the second first, and a pane on its own meets the first.
 *
 * **The floor is in rows and the box is in pixels**, so a row's height is worked out from what the
 * emulator measured and what the pane stands at. The pane's own padding is counted into the row,
 * which makes the floor slightly generous — it errs towards the terminal keeping more, which is the
 * direction that cannot hide a question from a reader.
 *
 * **It settles in one pass.** Growing the box by some amount shrinks the pane above it by the same
 * amount, so `standing + pane` does not change and the height this answers with is the height it
 * answers with next time.
 */
export function boxHeight(
  { content, standing, line, pane, rows }: {
    /** How tall what is written needs the box to be. */
    content: number;
    /** How tall the box is at this moment. */
    standing: number;
    /** One line of it. */
    line: number;
    /** How tall the terminal above it is at this moment. */
    pane: number;
    /** How many rows the terminal is drawing in that, as the emulator measured it. 0 before it has
     *  said, where there is no floor to work out and the box's own cap is the whole of the answer. */
    rows: number;
  },
): number {
  const spare = rows > 0 ? Math.max(0, pane - (pane / rows) * FLOOR_ROWS) : pane;
  const cap = Math.max(line, Math.min(line * BOX_LINES, standing + spare));
  return Math.min(content, cap);
}

/**
 * How long the pane's size has to hold still before the host is told it (`AMB-D-864`).
 *
 * Long enough that a line being written is one call rather than one per character, short enough that
 * a person who stopped typing does not watch the terminal catch up. It is the gap between keystrokes
 * this is measured against and not their speed: a fast typist and a slow one both pause between
 * words, and neither pauses this long mid-word.
 */
const SETTLED_MS = 150;

/** What Shift-Enter is sent as: `ESC` and a carriage return, the form the programs that want it read. */
export const NEWLINE = "\x1b\r";

/** What the emulator gives the program for Enter: the carriage return that sends a line. */
export const SUBMIT = "\r";

/**
 * Whether the sentence this pane is holding goes out behind what just crossed to the program.
 *
 * `owed` is the host having said it left the opening sentence in this pane's input box, unsent
 * (`crate::pty`). `data` is what the emulator has just given the program for a press.
 *
 * **It is the person's Enter that is being waited for, and nothing stands in for it.** What the
 * hand-over cannot find out by looking is whether the pane is showing an input box or a program's own
 * first question; a person sending a line of their own settles that outright (`AMB-D-805`). So the
 * sentence follows their line rather than preceding it — what they wrote goes through untouched, and
 * the rescue happens behind it.
 *
 * **The crossing is compared whole rather than searched.** What travels here is one press at a time,
 * or a bracketed paste entire (`\x1b[200~…`), or the emulator's own answers to the program — and
 * those are escape sequences. A lone carriage return is a person pressing Enter, and an input method
 * settling a line is not one of them: what a composition produces is the text it composed.
 */
export function sendsTheSentence(owed: boolean, data: string): boolean {
  return owed && data === SUBMIT;
}

/**
 * What the terminal is given for the presses the box under it hands straight on (`AMB-D-864`).
 *
 * These four are what a person reaches for while the box is empty — the history of what they ran,
 * the completion of a word, the way out of a menu — and none of them is a thing to do to an empty
 * box. `Ctrl+C` is the fifth and is not written here, because it is the key with a modifier held.
 *
 * They are written out rather than produced by the emulator, which is the one place in this module
 * that is true of besides Shift-Enter ({@link NEWLINE}): the box is not the emulator's textarea, so
 * a press made in it never reaches xterm at all. What is written is the ordinary form — the same
 * bytes the emulator produces for a terminal in its ordinary cursor mode.
 */
const PASSED_ON: Record<string, string> = {
  ArrowUp: "\x1b[A",
  ArrowDown: "\x1b[B",
  Tab: "\t",
  Escape: "\x1b",
};

/** The fields of a press this module reads. Written as a shape so the same test serves a press the
 *  page reports and one React hands over. */
export type Press = {
  key: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
};

/**
 * What the terminal is given for this press, or nothing where the press is not one that is handed on.
 *
 * **It is asked of a box with nothing in it** (`AMB-D-864`). What decides where a press goes is what
 * the person has written, not what is on the screen: a box holding a half-written sentence keeps its
 * own arrows, and an empty one has nothing to keep them for. The one press a written box hands on as
 * well is {@link leavesForTerminal}'s, which asks this for the bytes.
 *
 * `Ctrl+C` is here and `Ctrl` with anything else is not. It is the one press that means "stop what is
 * running", which is the reason a person looks away from what they were writing; the rest of the
 * control keys are a text box's own, and taking them would leave a reader unable to move about in a
 * sentence they had started.
 */
export function passedOn(e: Press): string | null {
  if (e.ctrlKey) {
    return (e.key === "c" || e.key === "C") && !e.altKey && !e.metaKey && !e.shiftKey ? "\x03" : null;
  }
  if (e.altKey || e.metaKey || e.shiftKey) return null;
  return PASSED_ON[e.key] ?? null;
}

/**
 * What the terminal is given for the press that leaves a box with something written in it, or nothing
 * where this press is not that one (`AMB-D-864`).
 *
 * **It is the way back to a program that is asking something.** While a line is half written the
 * arrows are the box's, so a menu the program is drawing cannot be walked — and there is no way to
 * ask a terminal whether it is drawing one (`AMB-T-4569`). Rather than guess at the screen, one press
 * is left as the road out: `ArrowUp` with the caret on the first line, where a textarea does nothing
 * with it anyway. Nothing written is lost by taking it; the box is still there to come back to.
 *
 * `caret` is where the caret sits in `written` — the first line is the text in front of it holding no
 * newline. Below the first line the press is the box's, and walks up through what is written.
 *
 * **An empty box is not this road**, and answers `null` here: everything of an empty box's goes to
 * the program already ({@link passedOn}), and this one press is the exception a written box makes.
 */
export function leavesForTerminal(e: Press, written: string, caret: number): string | null {
  if (written === "" || e.key !== "ArrowUp") return null;
  if (written.slice(0, caret).includes("\n")) return null;
  return passedOn(e);
}

/**
 * Whether this press is the one the pane answers for rather than passing on.
 *
 * Shift and Enter, and nothing else held with them: what Alt or Ctrl with Enter means belongs to the
 * program in the pane, and a terminal that answered for those would be deciding it. A key press is
 * also two events, and only the down one is a press.
 */
export function isNewline(e: KeyboardEvent): boolean {
  return e.type === "keydown"
    && e.key === "Enter"
    && e.shiftKey
    && !e.altKey
    && !e.ctrlKey
    && !e.metaKey;
}

/**
 * Whether this press is the paste the emulator is to stay out of, so the page answers it instead.
 *
 * **Only on Windows, and only for `Ctrl+V`.** macOS makes the press with the meta key, which the
 * emulator never turns into a character, and Linux is answered from the press itself because a paste
 * there carries nothing (`../core/clipFiles`). Windows is the one machine where the press a person
 * makes for paste is also the press the emulator reads as a control character: `Ctrl+V` is `^V`
 * (0x16), and that is what the program in the pane was given.
 *
 * **What is bought is the paste that carries files.** `Ctrl+Shift+V` was the only way into a pane
 * here, and it is Chromium's "paste as text" — a clipboard holding files answers it with nothing at
 * all, `text/plain` included, so an image and a copy made in the file panel had no road into a
 * Windows pane at all. Left to the page, `Ctrl+V` arrives carrying `Files`, which is the door
 * `takesPastedFiles` is already standing at (`AMB-T-4574` measured both halves on the real engine).
 *
 * **What is paid is `^V`.** The character stops reaching the program, and with it readline's
 * quoted-insert and vim's block select — presses with no other spelling. The pane's own shell is
 * PowerShell, where `Ctrl+V` is bound to paste and so nothing changes; an agent that took `^V` for a
 * paste of its own is answered by this one instead, with an image arriving as the path it was
 * written to rather than as the image (`AMB-D-854`).
 *
 * **Nothing is prevented, and that is the whole of it.** Answering `false` tells the emulator to
 * leave the press alone; the browser then goes on with its own default, which for this key is the
 * paste. `Ctrl+Shift+V` still passes through untouched, and so does every press held with Alt or the
 * meta key, which belong to the program.
 */
export function isPagePaste(e: KeyboardEvent, os: HostOs = hostOs()): boolean {
  return os === "windows"
    && e.type === "keydown"
    && (e.key === "v" || e.key === "V")
    && e.ctrlKey
    && !e.shiftKey
    && !e.altKey
    && !e.metaKey;
}

// The colours a pane is drawn in — **the one part of the interface the theme does not reach**. They
// are tokens like everything else (`styles/tokens.css`), and the tokens they read are the ones no
// theme overrides, so light and dark give the same three values.
//
// It is not a preference. An agent picks the colours it writes in by asking the terminal what ground
// it is on, and this is what answers: xterm.js reports these for OSC 10 and 11. A ground that followed
// the theme would strand every answer already given — the escape that announces a colour change
// (DECSET 2031) is not implemented here (`AMB-T-3546`), so a terminal already running cannot be told
// the theme was switched, and what a TUI painted for the old ground stays painted. Dark always is the
// one arrangement where that cannot happen; the cost is a dark pane inside a light face, which is
// paid on purpose.
function paneColors(): { background: string; foreground: string; cursor: string } {
  const style = getComputedStyle(document.documentElement);
  const token = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    background: token("--c-pane-bg", "#242320"),
    foreground: token("--c-pane-text", "#ece9e1"),
    cursor: token("--c-pane-cursor", "#2ba6a4"),
  };
}

// The terminal's drawn buffer, as much of it as a ref scan reads (`./refLinks`). Cells are copied
// out one at a time rather than the row translated to a string: a wide character covers two columns
// and a blank one covers a column with nothing in it, so a string would lose exactly the arithmetic a
// clickable range is made of.
function rowsOf(term: Terminal): Rows {
  const buffer = term.buffer.active;
  // One cell object, filled again per column: `getCell` writes into what it is handed, and what is
  // read out of it here is copied immediately.
  let scratch: IBufferCell | undefined;
  return {
    length: buffer.length,
    wrapped: (y) => buffer.getLine(y)?.isWrapped ?? false,
    cells: (y) => {
      const line = buffer.getLine(y);
      if (!line) return [];
      const cells: Cell[] = [];
      for (let x = 0; x < line.length; x++) {
        scratch = line.getCell(x, scratch);
        cells.push({ chars: scratch?.getChars() ?? "", width: scratch?.getWidth() ?? 1 });
      }
      return cells;
    },
  };
}

// Show the record on the board. The window it is read in belongs to the same process and not to this
// webview, so raising it is the host's (`crate::windows::show_ref`); a failure leaves the pane exactly
// as it was, which is the honest outcome for a click that could not be carried out.
function showRef(space: RefSpace, num: number): void {
  void invoke("show_ref", { kind: space, id: num }).catch(() => {});
}

// A chunk as it travelled: base64 in, bytes out. `atob` gives one character per byte, which is what
// makes the char codes the bytes themselves.
function decode(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/**
 * Measure the pane and tell the terminal its size in characters, unless the pane is not on screen.
 *
 * A hidden pane measures zero, and fitting to zero is what would make the program inside reflow to
 * nothing while nobody was looking. Switching to the other face hides this one and leaves the
 * terminal running (`AMB-D-753`), so "no size" here means "not being shown", and the size it had is
 * the size to keep until it is shown again.
 */
function refit(fit: FitAddon, host: HTMLElement): boolean {
  if (host.clientWidth === 0 || host.clientHeight === 0) return false;
  fit.fit();
  return true;
}

/** As much of a terminal as one size can be read at — what `replayTail` takes a list of. */
type Replay = Pick<Terminal, "resize" | "write">;

/**
 * Read a session's tail back into a terminal: every run at the size it was written at, in order.
 *
 * **The bytes are a terminal's output, not lines.** Where one of them ended is already decided, and
 * an emulator told a narrower screen folds it somewhere else — which for a program that draws by
 * moving the cursor about leaves parts of old frames standing (`AMB-T-4514`). Read at the size they
 * were written at and reflowed afterwards, the logical lines keep, so the same reflow that would
 * have broken them fixes them.
 *
 * **It arrives in runs because a terminal resized while nobody was drawing it wrote part of its tail
 * at each size.** One size for the whole of it fixes the newest part and leaves the rest folded
 * wrong (`AMB-T-4516`). Which size each run is is the host's answer: the pane was not there while it
 * changed, so nothing sent the size along (`crate::pty::Recent`).
 *
 * **The first run may not be output at all.** A program asks for the modes it wants — bracketed
 * paste, focus reporting — as it starts, and those bytes are long out of the tail of a session that
 * has been running a while; the host puts them back in front of it so a pane built now comes up in
 * the modes the terminal is actually in (`crate::pty::Modes`, `AMB-T-4566`). They carry the size the
 * tail begins at, and nothing here has to tell them apart from the tail.
 *
 * **Each write is awaited.** `write` queues, so a resize let go in front of the bytes it is meant to
 * follow would land on them instead — the same wrong fold, put back by the thing that reads it.
 */
export async function replayTail(term: Replay, runs: PtyReplayDto[]): Promise<void> {
  for (const run of runs) {
    // A size of nothing is a run no size ever reached. Resizing to it would throw, and the run's
    // bytes are still worth having at whatever size the terminal is already at.
    if (run.cols > 0 && run.rows > 0) term.resize(run.cols, run.rows);
    if (run.base64) await new Promise<void>((wrote) => term.write(decode(run.base64), wrote));
  }
}

/**
 * Which terminal this pane is to draw, as the thing putting it up knows it.
 *
 * A pane comes up for reasons it cannot tell apart from the inside — a person asked for a terminal
 * here, or the pane that had one moved, or the interface was rebuilt around a running session. What
 * the window holds is which slot had what, so it says; a pane left to work it out for itself would
 * have to guess, and there is nothing to guess from.
 */
/** Put a terminal in front of this pane: the one it is meant to have, else a new one. */
async function draw(
  term: Terminal,
  fit: FitAddon,
  host: HTMLElement,
  start: PaneStart,
): Promise<PtySessionDto> {
  const open = await invoke<PtySessionDto[]>("pty_sessions").catch(() => [] as PtySessionDto[]);
  // The slot's own terminal where it has one. Otherwise, and only where this pane is the one that may:
  // a single open session is the only count that names one without guessing.
  const want = start.session
    ? open.find((one) => one.session === start.session)
    : start.adopt !== false && open.length === 1
      ? open[0]
      : undefined;
  if (want) {
    try {
      const replay = await invoke<PtyReplayDto[]>("pty_attach", { session: want.session });
      await replayTail(term, replay);
      refit(fit, host);
      // The program is told last, because until now it was writing to the old width and its next
      // line has to arrive at the new one.
      void invoke("pty_resize", { session: want.session, cols: term.cols, rows: term.rows })
        .catch(() => {});
      return want;
    } catch {
      // It ended between the two calls. Opening one is what the pane was there to do anyway.
    }
  }
  return await invoke<PtySessionDto>("pty_open", {
    cwd: start.cwd ?? null,
    agent: start.agent ?? null,
    cols: term.cols,
    rows: term.rows,
  });
}

/** The bytes that open and close a bracketed paste. A program that has turned bracketed paste on
 *  reads what is between them as text and never as keys (`crate::handover`). */
const PASTE_OPEN = "\x1b[200~";
const PASTE_CLOSE = "\x1b[201~";

/**
 * One path, written so a terminal reads it as one thing (`AMB-D-801`).
 *
 * **Quoted, always — and the escape is the machine's own.** A name with a space in it is two words
 * to a shell, and the commonest thing anybody hands a pane is a screenshot, whose name has spaces on
 * every one of the three. Quoting costs nothing on the other side: an agent reads the quotes as text
 * and reaches the same file with or without them, measured on both macOS and Windows (`AMB-T-4008`,
 * `AMB-T-4011`). What is *not* free is getting the escape wrong, so it is not guessed:
 *
 * | | a `'` inside the name |
 * |---|---|
 * | macOS, Linux | closed, escaped, reopened — `'\''` |
 * | Windows | doubled — `''`, which is PowerShell's own way (`crate::launch` starts no other shell) |
 *
 * **Both failures are worse than not quoting at all**, which is why the branch is here rather than
 * left to one form for everybody. A POSIX escape reaching PowerShell — or no escape at all reaching
 * either — leaves the shell waiting for a quote it never gets (`quote>`, `>>`): the reader's Enter
 * does not end it and the pane is stuck until they know to press Ctrl-C. A Windows escape reaching a
 * POSIX shell is the quiet one: `it''s.png` becomes `its.png`, a different name, with nothing said.
 */
export function quotedPath(path: string, os: HostOs = hostOs()): string {
  const inside = os === "windows" ? path.replace(/'/g, "''") : path.replace(/'/g, "'\\''");
  return `'${inside}'`;
}

/**
 * Several paths, written so a terminal reads them as several things.
 *
 * Each one is quoted on its own and a space joins them, which is the only reading under which the
 * space between two paths and the space inside a name are told apart. It is the same line a drop of
 * several files puts in a pane (`../shell/TerminalPane`), and it is written once so the two roads
 * cannot drift into disagreeing about it.
 */
export function quotedPaths(paths: string[], os: HostOs = hostOs()): string {
  return paths.map((one) => quotedPath(one, os)).join(" ");
}

/**
 * Put `text` in the input box of whatever is running in a terminal, as a paste.
 *
 * **Nothing is submitted.** The newline is left out for the reason the handover leaves it out
 * (`AMB-D-793`): what is on the screen at that moment may be a first-run question, and a carriage
 * return would answer it — a "1" that runs `curl … | sh` was one of the choices actually found there.
 * A person is sitting in front of this pane, so leaving the text where they can read it and press
 * Enter is the whole of what is owed.
 *
 * A write that cannot land is nothing to say: the session ended between the drop and this, and the
 * pane already draws what a terminal ends with.
 */
export async function pasteIntoTerminal(session: string, text: string): Promise<void> {
  await invoke<void>("pty_write", { session, data: `${PASTE_OPEN}${text}${PASTE_CLOSE}` });
}

/**
 * Send `text` to whatever is running in a terminal, as the line a person wrote and pressed send on.
 *
 * **It is the paste plus the return, and the difference from {@link pasteIntoTerminal} is who
 * pressed.** A path handed to a pane is put where the reader can look at it, because what is on the
 * screen may be a first-run question and a return would answer it (`AMB-D-793`). Here the person has
 * written the line themselves and said to send it, which is the same act as their Enter in the pane —
 * so the return goes with it (`AMB-D-864`).
 *
 * The text is wrapped as a paste for the reason every other crossing is: what a person writes here
 * has newlines in it, and an agent that reads a bracketed paste takes those as part of one message
 * rather than as that many lines sent one after another.
 *
 * **The opening sentence rides out behind it**, the same way it rides out behind a line typed in the
 * pane itself (`AMB-D-805`). Whether anything is owed is the host's to answer and it answers once
 * (`crate::pty::pty_brief`), so this asks on every send rather than keeping a copy of the answer: a
 * send is a person pressing something, not a keystroke, and one round trip per send costs nothing.
 */
export async function sendIntoTerminal(session: string, text: string): Promise<void> {
  await pasteIntoTerminal(session, text);
  await invoke<void>("pty_write", { session, data: SUBMIT });
  await invoke<void>("pty_brief", { session }).catch(() => {});
}

/**
 * Hand a press on to whatever is running in a terminal, exactly as {@link passedOn} wrote it.
 *
 * It is the road for a press made outside the emulator's own box — the one the pane draws under the
 * terminal (`AMB-D-864`). Nothing else goes this way: a press made in the terminal is the emulator's,
 * and what the program is given for it is what the emulator produced.
 */
export async function pressIntoTerminal(session: string, data: string): Promise<void> {
  await invoke<void>("pty_write", { session, data });
}

/**
 * Put the keyboard on the terminal drawn in `host`.
 *
 * **Nothing else moves the keyboard into a pane.** A terminal takes it the way anything on a page
 * does — the person presses it — and that is enough everywhere but one place: a file dropped from
 * the desktop never presses the page at all (`../core/hostDrop`), so the pane it landed on can be
 * the one being pointed at and still not be the one the next keystroke goes to (`AMB-T-4182`).
 *
 * What takes the focus is the box the emulator collects typing in — the only textarea inside the
 * frame, which is why it is asked for that way rather than by a class name of xterm's own. The box a
 * person writes a line in stands outside the frame and is never found from here (`AMB-D-864`). A
 * place with no terminal in it has none, and the focus then stays where it was: taking it off
 * whatever holds it, to give it to nothing, is worse than leaving it alone.
 */
export function focusTerminal(host: HTMLElement | null): void {
  host?.querySelector<HTMLTextAreaElement>("textarea")?.focus();
}

/**
 * End the program in a terminal.
 *
 * **It is the only way out.** Taking a pane away never ends one — that is a pane moving, and the
 * session outlives it (`AMB-D-753`) — so short of this a terminal ends when the program in it decides
 * to, which is the one thing a runaway will not do.
 *
 * What is on the screen stays as it is. The host tells the pane the program has closed, the same way
 * it does when one exits on its own, and the pane draws what a terminal ends with: its last output,
 * and the way to open another.
 */
export async function endTerminal(session: string): Promise<void> {
  await invoke<void>("pty_close", { session });
}

/**
 * Fill `host` with a terminal — the one already running, or a new one — and return the way to take
 * the pane away again.
 *
 * The host element is measured for the size in characters, and re-measured whenever it changes, so
 * what the program inside reads as the terminal's width is the pane's actual width — that is what a
 * full-screen interface reflows to.
 *
 * `on` is how the window is told what happened here — the session running in the pane, what the agent
 * said about it, and the name the pane's frame should carry. `start` is the other direction: which
 * terminal this slot is to draw, which only the window holding the arrangement knows — and, where one
 * has to be started, where it opens and what runs in it (`./agent`).
 *
 * What comes back takes the pane away and **leaves the terminal running** for whatever draws it next.
 * There is no other way to end a pane, because there is no way yet to end a terminal: nothing in the
 * interface says "close this", and a pane going away is always the pane moving (`AMB-T-3632`).
 */
export async function mountTerminal(
  host: HTMLElement,
  on: PaneEvents,
  start: PaneStart = {},
): Promise<() => void> {
  const term = new Terminal({
    fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim() ||
      "ui-monospace, monospace",
    fontSize: 13,
    cursorBlink: true,
    theme: paneColors(),
    // The second way a ref becomes clickable: our own output wraps one in OSC 8, so the escape says
    // where the text points and no pattern has to find it (`AMB-T-3595`). Non-HTTP addresses have to
    // be let through for `amenbo://` to arrive at all, and an address neither branch below claims is
    // dropped — a program can say a piece of text points anywhere, and only these two are followed.
    linkHandler: {
      allowNonHttpProtocols: true,
      activate: (_event, text) => {
        const target = refFromUrl(text);
        if (target) {
          showRef(target.space, target.num);
          return;
        }
        const url = httpUrl(text);
        if (url) void openExternalUrl(url);
      },
    },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(host);
  refit(fit, host);

  // The first way, and the one that works on any program's output: read the refs back out of what was
  // drawn. It covers what the escape cannot — an agent's own words, a git log, a grep — and the escape
  // covers what this cannot, which is a ref the pane wrapped or a TUI elided.
  const links = term.registerLinkProvider({
    provideLinks(bufferLineNumber, callback) {
      // The row number a provider is given counts from 1 over the whole buffer, scrollback included,
      // and is the same number a link's range is written in.
      const rows = rowsOf(term);
      const found = refsOnRow(rows, bufferLineNumber - 1);
      // Paths are read off the same drawn buffer, by the same rules, and are the second kind of
      // thing in a pane that is a thing rather than a string (`AMB-T-3630`).
      const paths = pathsOnRow(rows, bufferLineNumber - 1);
      // Addresses are the third, and they go first: xterm drops a link whose cells an earlier one
      // already claimed, and a ref number drawn inside an address belongs to the address.
      const urls = urlsOnRow(rows, bufferLineNumber - 1);
      callback([
        ...urls.map((url) => ({
          range: url.range,
          text: url.text,
          activate: () => void openExternalUrl(url.text),
        })),
        ...found.map((ref) => ({
          range: ref.range,
          text: ref.text,
          activate: () => showRef(ref.space, ref.num),
        })),
        ...paths.map((path) => ({
          range: path.range,
          text: path.text,
          activate: () => on.path(path.text),
        })),
      ]);
    },
  });

  // **What a copy carries is the selection with the agent's drawing taken out of it** (`AMB-D-867`,
  // rules in `./copied`). The screen is untouched and there is no second way to press: the drawing is
  // the pane's width and the agent's marks, never a character a person chose, so it is not something
  // to offer a choice about.
  //
  // Which agent is in the pane is answered below, once the host says — and it is read at the moment
  // of the copy rather than closed over, because a pane that took up a running session learns what is
  // in it after this is registered.
  //
  // Registered on `host` in the capture phase, which is the whole of how it gets in front of xterm:
  // xterm takes `copy` as it bubbles up to `term.element`, a child of this one, so this runs first and
  // `stopPropagation` is what keeps xterm's own handler from writing the raw selection over this
  // (`AMB-T-4615`). The page's default has to be refused as well, or the browser copies the DOM
  // selection and the two writes race.
  let agentInPane: string | null = start.agent ?? null;
  const onCopy = (e: ClipboardEvent) => {
    const selection = term.getSelection();
    // Nothing selected in the pane is somebody copying somewhere else. Leave it alone.
    if (selection === "") return;
    e.clipboardData?.setData("text/plain", tidiedCopy(selection, agentInPane));
    e.preventDefault();
    e.stopPropagation();
  };
  host.addEventListener("copy", onCopy, true);

  // The session is not known until the host answers, and what the terminal has to say can be on its
  // way before that answer lands — the first prompt of a shell being started, or the next line of a
  // build on a session being adopted. Listening first and holding what arrives is what keeps either
  // from being the one thing the pane never shows.
  let session: string | null = null;
  const held: PtyChunkDto[] = [];

  const { listen } = await import("@tauri-apps/api/event");
  const unlistenOutput = await listen<PtyChunkDto>(OUTPUT_EVENT, ({ payload }) => {
    if (session === null) held.push(payload);
    else if (payload.session === session) {
      term.write(decode(payload.base64));
      on.output();
    }
  });
  const unlistenClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => {
    if (payload === session) on.closed(payload);
  });
  // **All this event does is put the pane in the way of sending it**: from here on the person's next
  // Enter carries the sentence out behind their own line (`sendsTheSentence`). Nothing is drawn for it
  // — the row above a pane says the pane's name and no more (`AMB-D-862`).
  //
  // Nothing is held for it the way the output and the statements are, either. The hand-over gives up
  // only after a minute of looking at the pane, so this cannot arrive before the id it is about.
  let owed = false;
  const unlistenUnsent = await listen<string>(UNSENT_EVENT, ({ payload }) => {
    if (payload !== session) return;
    owed = true;
  });
  // Statements are held the same way the output is, and for the same reason: the host starts watching
  // the drop box the moment it opens the terminal, so the first thing an agent says can be on its way
  // before the id it was said under is known here.
  const saidBeforeKnown: SessionSaidDto[] = [];
  const take = (statement: SessionSaidDto) => {
    on.said(statement);
    if (statement.verb === "name" && statement.text) on.name(statement.text, "session");
  };
  const unlistenSaid = await listen<SessionSaidDto>(SAID_EVENT, ({ payload }) => {
    if (session === null) saidBeforeKnown.push(payload);
    else if (payload.session === session) take(payload);
  });

  const running = await draw(term, fit, host, start);
  session = running.session;
  // Which agent is in the pane, for the copy rules above. It comes off the session and not off
  // `start` for the reason the folder below does: a pane that took up a running terminal is running
  // whatever that one was started with, which `start` has no answer for.
  agentInPane = running.agent ?? null;
  // The folder comes off the session rather than off `start`, because those are the same answer only
  // for a terminal this pane started. One it took up runs where it was started, which is what the page
  // holding it has to be told (`./layout`).
  on.opened(running.session, running.folder ?? null, running.agent ?? null);
  for (const chunk of held.splice(0)) {
    if (chunk.session !== session) continue;
    term.write(decode(chunk.base64));
    // Held rather than replayed: these arrived while the pane was still being told what it was
    // drawing, moments ago, so they are output arriving like any other.
    on.output();
  }
  for (const statement of saidBeforeKnown.splice(0)) {
    if (statement.session === session) take(statement);
  }

  // Whatever the emulator made of a press, straight through to the program. **Nothing on the way out
  // is read**: what the program is given for a key is what the emulator produced for it, which is why
  // arrow keys, Ctrl-C and bracketed paste all work here without anything having tried to make them.
  const send = (data: string) => {
    void invoke("pty_write", { session, data }).catch(() => {});
  };
  const stream = term.onData((data) => {
    send(data);
    if (!sendsTheSentence(owed, data)) return;
    // **Asked behind their line, and asked once.** The write above went out first, so what the person
    // sent lands as they wrote it and the sentence follows rather than mixing into it. Whether
    // anything is really owed is the host's to answer — it holds the sentence and takes it as it goes
    // — and the answer cannot come back, so a later Enter is left alone rather than paying a round
    // trip per keystroke to be told nothing.
    owed = false;
    // The id is read out once here: nothing is owed before the host has answered with one, so this is
    // never the null it starts as.
    const its = session;
    if (its === null) return;
    void invoke("pty_brief", { session: its }).catch(() => {});
  });

  // **A paste carrying files is answered here; every other paste is the emulator's** — the reading
  // itself, and why it is taken on the way down, are `../core/clipFiles`. What is written is this
  // side's: pasted as they stand, a name with a space in it is two words to the shell, so the
  // quoting a drop already gets goes on them (`AMB-D-801`, `AMB-D-832`).
  //
  // Falling back to the words the paste carried is not a mishap — a file manager that puts a file on
  // and no path with it (`AMB-T-4220`) is exactly what the host is asked about, and if the host finds
  // nothing the reader still gets what they copied.
  //
  // **An image is written down first, because a path is the only shape this pane can take one in.**
  // A screenshot on the clipboard is bytes and no file, so there is nothing for the host to name
  // until they are put somewhere — which the pane's own directory is for, and which is why the
  // writing is asked for here rather than in the reading (`AMB-D-854`). What comes back is a path
  // like any other, quoted the same way.
  const writeImage = async (bytes: Uint8Array, mime: string): Promise<string[]> => {
    const its = session;
    if (its === null) return [];
    return await writesPastedImage(bytes, mime, its);
  };
  const stopPaste = takesPastedFiles(
    host,
    (paths, words) => {
      const its = session;
      if (its === null) return;
      const text = paths.length > 0 ? quotedPaths(paths) : words;
      if (text) void pasteIntoTerminal(its, text).catch(() => {});
    },
    writeImage,
  );

  // **On Linux the same image arrives by a different door** — `Ctrl+Shift+V` rather than the paste,
  // which carries nothing there (`../core/clipFiles`). What comes back is written and pasted the
  // same way; only the reading differs.
  const stopImagePress = takesPastedImages(
    host,
    writeImage,
    (paths) => {
      const its = session;
      if (its === null || paths.length === 0) return;
      void pasteIntoTerminal(its, quotedPaths(paths)).catch(() => {});
    },
    "terminal",
  );

  // **The two presses the emulator does not get to answer, and they are refused in opposite ways.**
  // Windows' `Ctrl+V` is left to the page, default and all, because the page's default is the paste
  // (`isPagePaste`). Shift-Enter is taken from both.
  //
  // **Shift-Enter, which is the one press the emulator cannot pass on.** What a terminal is given for
  // Enter is a carriage return, and it is given the same one whether or not Shift was held — so an
  // agent that takes Shift-Enter for "another line" and Enter for "send it" is handed two presses it
  // cannot tell apart, and every multi-line answer goes off half-written.
  //
  // What is sent instead is `ESC` and the carriage return, which is the form those programs read and
  // the only one this pane needs to speak. It is written here rather than announced: a terminal can
  // tell the program it will describe *every* press in a richer form — the kitty protocol, or
  // `modifyOtherKeys` — and a program that took the offer would then expect that form for presses this
  // has no answer for. Claiming a vocabulary and then not having it is worse than not claiming one
  // (`AMB-T-3612` measured what the terminals of the day actually do).
  //
  // **The press is taken away from the page as well as from the emulator.** Answering `false` here says
  // only that the emulator is to stay out of it; the browser goes on with its own default, which for
  // this key is a newline typed into the hidden box the emulator reads — and that newline reaches the
  // program as the carriage return this exists to replace. Both halves have to be refused, or the
  // press is sent twice and the second one wins.
  //
  // Read once rather than per press: the machine under a pane does not change while it is drawn.
  const os = hostOs();
  term.attachCustomKeyEventHandler((e) => {
    if (isPagePaste(e, os)) return false;
    if (!isNewline(e)) return true;
    e.preventDefault();
    send(NEWLINE);
    return false;
  });

  // Re-measure on every change of the pane's size, and tell the host what the new size is. Both
  // halves are needed: the first reflows what is already drawn, the second is what wakes the program
  // inside so it repaints at the new width.
  // Being shown again after the other face was up is such a change, which is what brings a hidden pane
  // back to the size of the window it is in.
  //
  // **The two halves are timed differently, and only since the box under the pane started moving the
  // pane's height.** Re-measuring is local and cheap, so it happens on every change and what is drawn
  // keeps up with the box growing under it. Telling the host is neither: a program in the pane
  // repaints its whole screen when the size it was given changes, so one call per keystroke leaves
  // the terminal flickering for as long as somebody is writing (`AMB-D-864`). So the call waits for
  // the changes to stop, and the size it sends is read when it fires rather than when it was asked
  // for — what the program is told is where the pane ended up, never a size it passed through.
  let settling: ReturnType<typeof setTimeout> | null = null;
  const resize = new ResizeObserver(() => {
    if (!refit(fit, host)) return;
    if (settling !== null) clearTimeout(settling);
    settling = setTimeout(() => {
      settling = null;
      void invoke("pty_resize", { session, cols: term.cols, rows: term.rows }).catch(() => {});
    }, SETTLED_MS);
    on.sized?.(term.cols, term.rows);
  });
  resize.observe(host);
  // Said once for the size the pane came up at. The observer above fires on changes, and a pane that
  // never changed size would otherwise leave the window with no answer at all.
  on.sized?.(term.cols, term.rows);

  return () => {
    resize.disconnect();
    if (settling !== null) clearTimeout(settling);
    host.removeEventListener("copy", onCopy, true);
    links.dispose();
    stream.dispose();
    stopPaste();
    stopImagePress();
    void unlistenOutput();
    void unlistenClosed();
    void unlistenSaid();
    void unlistenUnsent();
    term.dispose();
  };
}
