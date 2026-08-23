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
import { NOTHING_TYPED, typed, type NamedBy } from "./frames";
import { refFromUrl, refsOnRow, type Cell, type Rows } from "./refLinks";

// The events the host sends this pane. Output is a chunk; closed is the program in the terminal
// having exited, which arrives once and is the last thing that session says.
const OUTPUT_EVENT = "pty://output";
const CLOSED_EVENT = "pty://closed";

// What the agent in this pane says about its session, as the host reads it out of the drop box it was
// given (`AMB-D-749`). It is the one thing crossing this seam that has been read rather than carried:
// the surface layer is a vocabulary, and a verb of it is exactly as good as its word.
const SAID_EVENT = "session://said";

/**
 * What a pane tells the window it is in. The window holds what is known about its sessions and what its
 * frames are called; a pane is where those things happen, not where they are kept.
 */
export type PaneEvents = {
  /** A terminal is running in this pane, under this session id, since `startedAt`, in `folder`. It is
   *  said of a terminal this pane adopted as much as of one it started: what the window holds is what
   *  is running in it, and a session that moved windows is running in the one it moved to. */
  opened(session: string, startedAt: string, folder: string | null): void;
  /** The agent said something about its session. */
  said(statement: SessionSaidDto): void;
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

// The terminal's own colours, taken from the same tokens the rest of the interface is drawn from so
// a pane does not sit on the page as a black rectangle. Only the chrome follows the theme: the
// program inside cannot be told the colours changed (xterm.js does not implement the sequence that
// announces it), so what a TUI paints for itself stays as it chose.
function themeColors(): { background: string; foreground: string; cursor: string } {
  const style = getComputedStyle(document.documentElement);
  const token = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    background: token("--c-surface", "#ffffff"),
    foreground: token("--c-text", "#23211c"),
    cursor: token("--c-accent", "#0e7c7b"),
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
    theme: themeColors(),
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
      const found = refsOnRow(rowsOf(term), bufferLineNumber - 1);
      callback(found.map((ref) => ({
        range: ref.range,
        text: ref.text,
        activate: () => showRef(ref.space, ref.num),
      })));
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
    else if (payload.session === session) term.write(decode(payload.base64));
  });
  const unlistenClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => {
    if (payload === session) on.closed(payload);
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
    if (chunk.session === session) term.write(decode(chunk.base64));
  }
  for (const statement of saidBeforeKnown.splice(0)) {
    if (statement.session === session) take(statement);
  }

  // The first line a person sends into this pane names its frame, so a pane is called something before
  // anyone gets round to naming it. Only the first: the keys are followed until one line has been sent
  // and then let alone, and whether the name takes at all is the store's to say (`./frames`).
  let typing = NOTHING_TYPED;
  const keys = term.onData((data) => {
    void invoke("pty_write", { session, data }).catch(() => {});
    if (typing.sent) return;
    typing = typed(typing, data);
    if (typing.sent && typing.line) on.name(typing.line, "typed");
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

  // The theme is settled on the document element, so a change of it is an attribute change there.
  const theme = new MutationObserver(() => {
    term.options.theme = themeColors();
  });
  theme.observe(document.documentElement, { attributeFilter: ["data-theme"] });

  return () => {
    resize.disconnect();
    theme.disconnect();
    links.dispose();
    keys.dispose();
    void unlistenOutput();
    void unlistenClosed();
    void unlistenSaid();
    term.dispose();
  };
}
