// What jsdom does not implement, and the interface does not work without.
//
// It is loaded before every test file (`../vitest.config.ts`). Under the node environment there is
// nothing here to do, and each stand-in says so by only being put up where the real thing is absent
// — a browser, or a jsdom that grows one, keeps its own.

/**
 * A `ResizeObserver` that answers the constructor and never fires.
 *
 * **jsdom lays nothing out**, so it has no `ResizeObserver` at all: every box there is zero by zero
 * and nothing would ever be observed. The interface takes one up wherever a measurement has to keep
 * up with the window — the terminal in a pane, the box under it (`./shell/TerminalPane`) — so
 * without this a component test dies in the constructor rather than on whatever it was about.
 *
 * **Inert on purpose.** A test that means to say the pane changed size says so itself, by playing
 * whatever told the pane; a stand-in that invented callbacks would be inventing a layout jsdom does
 * not have.
 */
class UnlaidOutResizeObserver implements ResizeObserver {
  constructor(_watch: ResizeObserverCallback) {}
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = UnlaidOutResizeObserver;
}
