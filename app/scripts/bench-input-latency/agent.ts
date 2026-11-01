// Put in place of `src/talk/agent.ts`: a pane's terminal, with no host behind it.
//
// It is the same emulator the panes draw with (xterm, DOM renderer, 13px), handed a line every 16ms
// the way an agent at work writes, so the page has the DOM churn of a busy terminal under every
// pane. The pane is told it has a session, which is what puts the box to write in under it.
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { PaneEvents, PaneStart } from "@app/talk/terminal";

export * from "@app/talk/agent";

/** How often the terminal is written to, in ms. */
const EVERY = 16;

let sessions = 0;

export async function mountAgentFrame(
  host: HTMLElement,
  _lang: unknown,
  on: PaneEvents,
  start: PaneStart = {},
): Promise<() => void> {
  const frame = document.createElement("div");
  frame.className = "agent__frame";
  frame.style.height = "100%";
  host.append(frame);
  const term = new Terminal({ fontFamily: "Menlo, monospace", fontSize: 13, cursorBlink: true, macOptionIsMeta: true });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(frame);
  fit.fit();
  const sized = new ResizeObserver(() => fit.fit());
  sized.observe(frame);
  let n = 0;
  const writing = setInterval(() => {
    n += 1;
    term.write(`\x1b[2m${new Date().toISOString()}\x1b[0m \x1b[36mline ${n}\x1b[0m the agent is working on it\r\n`);
  }, EVERY);
  const session = `bench-${++sessions}`;
  queueMicrotask(() => {
    on.opened(session, start.cwd ?? null, null);
    on.sized?.(term.cols, term.rows);
  });
  return () => {
    clearInterval(writing);
    sized.disconnect();
    term.dispose();
    frame.remove();
  };
}
