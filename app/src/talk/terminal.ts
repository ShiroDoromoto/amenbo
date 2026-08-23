// One pane of the talk window: a real terminal, drawn by xterm.js over a PTY the host holds.
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
import type { PtyChunkDto, SessionSaidDto } from "../bindings/bindings";
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
  /** A terminal has started in this pane, under this session id. */
  opened(session: string, startedAt: string): void;
  /** The agent said something about its session. */
  said(statement: SessionSaidDto): void;
  /** The program in the terminal has exited. Nothing running is kept. */
  closed(session: string): void;
  /** Something has named this pane's frame. Whether the name takes is the store's to say — a person's
   *  name for a frame is not taken back off it by the agent (`./frames`). */
  name(name: string, by: NamedBy): void;
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
 * Fill `host` with a terminal, start a session in it, and return the way to take it away again.
 *
 * The host element is measured for the size in characters, and re-measured whenever it changes, so
 * what the program inside reads as the terminal's width is the pane's actual width — that is what a
 * full-screen interface reflows to.
 *
 * `on` is how the window is told what happened here — the session that started, what the agent said
 * about it, and the name the pane's frame should carry.
 */
export async function mountTerminal(host: HTMLElement, on: PaneEvents): Promise<() => void> {
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
  fit.fit();

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

  // The session is not known until the host answers, and the shell's first prompt can be on its way
  // before that answer lands. Listening first and holding what arrives is what keeps the prompt from
  // being the one thing the pane never shows.
  let session: string | null = null;
  const held: PtyChunkDto[] = [];
  let closed = false;

  const { listen } = await import("@tauri-apps/api/event");
  const unlistenOutput = await listen<PtyChunkDto>(OUTPUT_EVENT, ({ payload }) => {
    if (session === null) held.push(payload);
    else if (payload.session === session) term.write(decode(payload.base64));
  });
  const unlistenClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => {
    if (payload === session) {
      closed = true;
      on.closed(payload);
    }
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

  session = await invoke<string>("pty_open", { cwd: null, cols: term.cols, rows: term.rows });
  on.opened(session, new Date().toISOString());
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
  const resize = new ResizeObserver(() => {
    fit.fit();
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
    if (!closed) void invoke("pty_close", { session }).catch(() => {});
    term.dispose();
  };
}
