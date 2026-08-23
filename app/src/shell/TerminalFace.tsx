import { useCallback, useEffect, useRef, useState } from "react";
import { TerminalPane } from "./TerminalPane";
import { PaneRail } from "./PaneRail";
import { frameNames, nameFrame, type FrameNames, type NamedBy } from "../talk/frames";
import {
  closedIn, COUNTS, EMPTY_LAYOUT, focusOn, folderOfPage, frameFor, goPage, MAX_PAGES, movedTo,
  openedIn, pageCount, setCount, sidesAreDrawers, slotsOf, type Count, type Layout,
} from "../talk/layout";
import { t, tf } from "../core/i18n";

/**
 * The terminal, drawn inside the board's window — the second face of the one window (`AMB-D-753`).
 *
 * It is put up once and then left alone. Switching back to the ledger hides it with CSS rather than
 * taking it down, which is the one thing this component exists to guarantee: unmounting would take
 * the emulator with it, and a terminal whose pane went away is an agent nobody can get back to. The
 * caller therefore keeps this rendered for as long as the window is the terminal's home, and hides
 * it by hiding its own container.
 *
 * What it holds is the arrangement — which frames there are, which page is up, how many panes it
 * shows (`../talk/layout`) — because that is the one thing no pane can know: a pane is a drawing of a
 * session, and the places the drawings go are the face's. Turning a page takes panes down and leaves
 * the terminals in them running, which is the same thing splitting a window out does.
 *
 * When the window *stops* being the terminal's home — the user splits it out, or a language change
 * rebuilds the interface — this does come down, and the sessions do not: the panes detach, and
 * whatever draws next adopts what is still running (`../talk/terminal.ts`).
 *
 * `note` is what the shell has to say about this face that the pane cannot — a window that could not
 * be split out, which is the press of the button here having come to nothing.
 */
export function TerminalFace({ onSplitOut, note }: { onSplitOut: () => void; note: string | null }) {
  const rootRef = useRef<HTMLDivElement>(null);
  // The face comes up with a terminal, the way a single-pane face always did: the first slot of the
  // first page is made here so there is a place for it to be in.
  const [layout, setLayout] = useState<Layout>(() => {
    const made = frameFor(EMPTY_LAYOUT, 1, 0);
    return { ...made.layout, focus: made.frame.id };
  });
  const [names, setNames] = useState<FrameNames>(new Map());
  const [width, setWidth] = useState(() => (typeof window === "undefined" ? 0 : window.innerWidth));
  const [railOpen, setRailOpen] = useState(false);

  // The frames a person has just asked for a terminal in, and the one the face opens with. It is a
  // ref rather than state because it is read at the moment a pane is put up and never drawn: a frame
  // that has had its terminal started is off the list, so turning back to a page whose program exited
  // offers the way to open one again instead of quietly starting a second shell.
  const startNow = useRef(new Set<string>([layout.frames[0]!.id]));

  useEffect(() => {
    let alive = true;
    void frameNames().then((known) => { if (alive) setNames(known); }).catch(() => {});
    return () => { alive = false; };
  }, []);

  // How wide the face actually is, which is half of whether the columns beside the panes are columns.
  useEffect(() => {
    const root = rootRef.current;
    if (!root || typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(() => {
      // A hidden face measures zero, and zero is not a narrow window — it is the other face being up.
      if (root.clientWidth > 0) setWidth(root.clientWidth);
    });
    watch.observe(root);
    return () => watch.disconnect();
  }, []);

  const named = useCallback((frame: string, name: string, by: NamedBy) => {
    // What comes back is the whole set rather than an acknowledgement: a naming can be refused, and
    // drawing what was asked for would show a name that is not the frame's (`../talk/frames`).
    void nameFrame(frame, name, by).then(setNames).catch(() => {});
  }, []);

  const opened = useCallback((frame: string, session: string, folder: string | null) => {
    startNow.current.delete(frame);
    setLayout((was) => openedIn(was, frame, session, folder));
  }, []);

  const pages = pageCount(layout);
  const drawers = sidesAreDrawers(layout.count, width);

  // ⌘1〜9 for the pages. It is caught on the document because the pane below has the keys — a
  // terminal is given every keystroke that is a character, and a page is reached with the ones that
  // are not. The face is kept mounted while the ledger is up (see above), so a face nobody is looking
  // at must not answer: `hidden` on a container above is what says so.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!e.metaKey && !e.ctrlKey) return;
      if (e.altKey || e.shiftKey) return;
      const digit = Number(e.key);
      if (!Number.isInteger(digit) || digit < 1 || digit > MAX_PAGES) return;
      if (rootRef.current?.closest("[hidden]")) return;
      e.preventDefault();
      setLayout((was) => goPage(was, digit));
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const rail = (
    <PaneRail
      layout={layout}
      names={names}
      onPick={(frame) => {
        setLayout((was) => focusOn(was, frame));
        setRailOpen(false);
      }}
      onRename={(frame, name) => named(frame, name, "person")}
    />
  );

  return (
    <div className={`termface${drawers ? " termface--drawers" : ""}`} ref={rootRef}>
      <div className="termface__bar">
        <button className="termface__action" onClick={onSplitOut}>{t("face.splitOut")}</button>
        {drawers && (
          <button
            className="termface__action"
            onClick={() => setRailOpen((open) => !open)}
            aria-expanded={railOpen}
          >
            {t("face.rail")}
          </button>
        )}
        {/* How many panes the page shows. Three steps, always all three shown: which one is on is
            what a person is choosing between, and a control that only says the next step makes them
            press it to find out. */}
        <div className="termface__counts" role="radiogroup" aria-label={t("face.paneCount")}>
          {COUNTS.map((count) => (
            <button
              key={count}
              className={`termface__count${layout.count === count ? " termface__count--on" : ""}`}
              // One of three, and exactly one: a toggle each would say three independent things can
              // be on, which is not what the control does.
              role="radio"
              aria-checked={layout.count === count}
              onClick={() => setLayout((was) => setCount(was, count as Count))}
            >
              {count}
            </button>
          ))}
        </div>
        {/* The pages, as a row of the digits that reach them. */}
        <nav className="termface__pages" aria-label={t("face.pages")}>
          {Array.from({ length: pages }, (_, i) => i + 1).map((page) => (
            <button
              key={page}
              className={`termface__page${layout.page === page ? " termface__page--on" : ""}`}
              // Going to a page, not turning something on: the one showing is the current page.
              aria-current={layout.page === page ? "page" : undefined}
              title={tf("face.page", { n: page })}
              onClick={() => setLayout((was) => goPage(was, page))}
            >
              {page}
            </button>
          ))}
        </nav>
        {note !== null && <span className="termface__note">{note}</span>}
      </div>
      <div className="termface__body">
        {drawers
          ? railOpen && <div className="termface__drawer">{rail}</div>
          : rail}
        <div className={`termface__page-grid termface__page-grid--${layout.count}`}>
          {slotsOf(layout, layout.page).map((frame, slot) =>
            frame
              ? (
                <TerminalPane
                  key={frame.id}
                  frame={frame.id}
                  names={names}
                  place={{
                    session: frame.session,
                    // Only the first slot of the first page may take up a terminal it did not start:
                    // that is where a session split out into its own window comes back to.
                    adopt: frame.id === layout.frames[0]!.id,
                    cwd: folderOfPage(layout, layout.page),
                  }}
                  autoStart={frame.session !== null || startNow.current.has(frame.id)}
                  focused={layout.focus === frame.id}
                  onOpened={opened}
                  onSaid={(statement) => {
                    if (statement.cwd) {
                      setLayout((was) => movedTo(was, statement.session, statement.cwd!));
                    }
                  }}
                  onClosed={(session) => setLayout((was) => closedIn(was, session))}
                  onName={named}
                  onFocus={(id) => setLayout((was) => focusOn(was, id))}
                />
              )
              : (
                <button
                  key={`${layout.page}.${slot}`}
                  className="slot slot--empty"
                  onClick={() => setLayout((was) => {
                    const made = frameFor(was, was.page, slot);
                    startNow.current.add(made.frame.id);
                    return focusOn(made.layout, made.frame.id);
                  })}
                >
                  {t("face.open")}
                </button>
              ))}
        </div>
      </div>
    </div>
  );
}
