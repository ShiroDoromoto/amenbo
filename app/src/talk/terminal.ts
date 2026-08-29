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
// The one thing a pane reads out of a person's typing is the first line they send, which names the
// frame — and it is read off their presses rather than off the stream, because the stream carries the
// emulator's answers to the program as well and nothing there tells the two apart (`./frames`).
//
// Refs are read **after** all of that, off what was drawn rather than out of what crossed
// (`./refLinks`), which is what keeps the line above true: the stream is still nobody's to read, and
// a cell already holds a character every escape has finished with. It is the one thing owning a
// terminal buys that a standard one cannot do — the characters in a pane are records to amenbo and
// a string to everything else — and it costs the stream nothing.

import { FitAddon } from "@xterm/addon-fit";
import { Terminal, type IBufferCell } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { PtyChunkDto, PtySessionDto, SessionSaidDto } from "../bindings/bindings";
import type { RefSpace } from "../core/idref";
import { invoke } from "../core/ipc";
import { hostOs, type HostOs } from "../core/platform";
import { NOTHING_TYPED, pressedKey, typed, type NamedBy, type Pressed } from "./frames";
import { pathsOnRow, refFromUrl, refsOnRow, type Cell, type Rows } from "./refLinks";

// The events the host sends this pane. Output is a chunk; closed is the program in the terminal
// having exited, which arrives once and is the last thing that session says.
const OUTPUT_EVENT = "pty://output";
const CLOSED_EVENT = "pty://closed";

// What the agent in this pane says about its session, as the host reads it out of the drop box it was
// given (`AMB-D-749`). It is the one thing crossing this seam that has been read rather than carried:
// the surface layer is a vocabulary, and a verb of it is exactly as good as its word.
const SAID_EVENT = "session://said";

// The opening instruction was left in this pane's input box unsent (`crate::pty`). It arrives once,
// only for the ending a person can finish, and it carries the session's id and nothing else.
const UNSENT_EVENT = "pty://unsent";

/**
 * What a pane tells the window it is in. The window holds what is known about its sessions and what its
 * frames are called; a pane is where those things happen, not where they are kept.
 */
export type PaneEvents = {
  /** A terminal is running in this pane, under this session id, since `startedAt`, in `folder`. It is
   *  said of a terminal this pane adopted as much as of one it started: what the window holds is what
   *  is running in it, and a session that moved windows is running in the one it moved to. */
  opened(session: string, startedAt: string, folder: string | null): void;
  /** A chunk has crossed and been drawn. Said per chunk and carrying nothing: what is read off it is
   *  the time it happened, which is the one thing about a stream that means the same for every program
   *  in a pane (`./moving`). The tail a pane is handed on picking a terminal up is not one of these —
   *  it is output being drawn again, not output arriving. */
  output(): void;
  /** The agent said something about its session. */
  said(statement: SessionSaidDto): void;
  /** The sentence Amenbo opens an agent with is sitting in this pane's input box, unsent. What the
   *  person is owed here is that it is theirs to send — a box holding it looks exactly like a box
   *  that was emptied by the program reading it (`AMB-D-805`). */
  unsent(session: string): void;
  /** That sentence has gone out of the input box, on the reader's own Enter. What the row was owed
   *  while it sat there was that it was theirs to send, and there is nothing left to send: a row
   *  still saying so would point them at a keypress that now does nothing. It is not the agent having
   *  read it — that is the agent's own word (`AMB-D-805`) — and a pane can send the sentence to a
   *  program that never says it. */
  sent(session: string): void;
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

/**
 * Which terminal this pane is to draw, as the thing putting it up knows it.
 *
 * A pane comes up for reasons it cannot tell apart from the inside — a person asked for a terminal
 * here, or the pane that had one moved, or the interface was rebuilt around a running session. What
 * the window holds is which slot had what, so it says; a pane left to work it out for itself would
 * have to guess, and there is nothing to guess from.
 */
/** Put a terminal in front of this pane: the one it is meant to have, else a new one. */
async function draw(term: Terminal, start: PaneStart): Promise<PtySessionDto> {
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
      const replay = await invoke<string>("pty_attach", { session: want.session });
      if (replay) term.write(decode(replay));
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
    // be let through for `amenbo://` to arrive at all — which is safe here because arriving is not
    // being followed: an address this does not recognise is dropped, and the ones it does recognise
    // reach a function that selects a record by number. Nothing here opens a URL.
    linkHandler: {
      allowNonHttpProtocols: true,
      activate: (_event, text) => {
        const target = refFromUrl(text);
        if (target) showRef(target.space, target.num);
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
      callback([
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
  // Nothing is held for this one the way the output and the statements are. The hand-over gives up
  // only after a minute of looking at the pane, so this cannot arrive before the id it is about.
  //
  // It is also what puts this pane in the way of sending it: from here on the person's next Enter
  // carries the sentence out behind their own line (`sendsTheSentence`).
  let owed = false;
  const unlistenUnsent = await listen<string>(UNSENT_EVENT, ({ payload }) => {
    if (payload !== session) return;
    owed = true;
    on.unsent(payload);
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

  const running = await draw(term, start);
  session = running.session;
  // The host's own answer for when it began, not the moment this pane went up: a session that moved
  // windows started when it started, and a pane that said otherwise would have the window telling the
  // reader the wrong thing about how long their work has been running.
  // The folder comes off the session rather than off `start`, because those are the same answer only
  // for a terminal this pane started. One it took up runs where it was started, which is what the page
  // holding it has to be told (`./layout`).
  on.opened(running.session, running.startedAt, running.folder ?? null);
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
    // never the null it starts as, and holding it keeps the answer landing on the pane that asked.
    const its = session;
    if (its === null) return;
    void invoke<boolean>("pty_brief", { session: its })
      // Only where something actually went: the host holds the sentence and is the one that can say
      // whether this press was the one that sent it.
      .then((went) => {
        if (went) on.sent(its);
      })
      .catch(() => {});
  });

  // The first line a person sends into this pane names its frame, so a pane is called something before
  // anyone gets round to naming it. Only the first: the presses are followed until one line has been
  // sent and then let alone, and whether the name takes at all is the store's to say (`./frames`).
  //
  // **It is read off their presses and not off the stream above**, which carries the emulator's own
  // answers to the program — the colour it is drawn in, where its cursor is — beside the person's
  // typing, with nothing to tell the two apart. Read there, an agent that asked what colour it was
  // being drawn in named its own pane `10;rgb:ecec/e9e9/…` (`AMB-T-3668`, `AMB-D-748`).
  let typing = NOTHING_TYPED;
  const wrote = (did: Pressed | null) => {
    if (typing.sent || did === null) return;
    typing = typed(typing, did);
    if (typing.sent && typing.line) on.name(typing.line, "typed");
  };
  const presses = term.onKey(({ domEvent }) => wrote(pressedKey(domEvent)));
  // An input method is a line written a key at a time and settled all at once, so what a person wrote
  // in one is taken where it is settled rather than while it is being guessed at. It is read off the
  // box the emulator collects their typing in, which is where a composition happens.
  const textarea = term.textarea;
  const composed = (e: CompositionEvent) => {
    if (e.data) wrote({ kind: "text", text: e.data });
  };
  textarea?.addEventListener("compositionend", composed);

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
  term.attachCustomKeyEventHandler((e) => {
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
  const resize = new ResizeObserver(() => {
    if (!refit(fit, host)) return;
    void invoke("pty_resize", { session, cols: term.cols, rows: term.rows }).catch(() => {});
  });
  resize.observe(host);

  return () => {
    resize.disconnect();
    links.dispose();
    stream.dispose();
    presses.dispose();
    textarea?.removeEventListener("compositionend", composed);
    void unlistenOutput();
    void unlistenClosed();
    void unlistenSaid();
    void unlistenUnsent();
    term.dispose();
  };
}
