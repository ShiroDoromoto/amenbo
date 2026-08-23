import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AdriftSlot } from "./AdriftSlot";
import { TerminalPane } from "./TerminalPane";
import { PaneRail } from "./PaneRail";
import {
  frameNames, keepLayout, nameFrame, savedLayout, type FrameNames, type NamedBy,
} from "../talk/frames";
import {
  closedIn, COUNTS, EMPTY_LAYOUT, focusOn, folderOfPage, frameFor, goPage, laidOut, MAX_PAGES,
  movedTo, openedIn, pageCount, pageOfFrame, restored, setCount, settledIn, sidesAreDrawers,
  slotsOf,
  type Count, type Layout,
} from "../talk/layout";
import { FilesPanel } from "../files/FilesPanel";
import { markRead, tookPoint, type PointedBySession } from "../files/pointed";
import { inTauri } from "../core/snapshot";
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
 * What runs in a pane is not settled here either. The frame put up inside each slot asks the host
 * which agent that folder starts with, and draws the offer or the install notice where that has no
 * single answer (`../talk/agent.ts`) — which is also where a refusal to start one is shown, so
 * nothing here holds a failure of its own.
 *
 * `note` is what the shell has to say about this face that the pane cannot — a window that could not
 * be split out, which is the press of the button here having come to nothing.
 *
 * `onWaiting` is the one thing this face says back to the shell: whether **any** pane on it is
 * waiting on a person. Behind the other face no label above a pane can be seen at all, so the shell
 * puts a badge on the face switch instead (`./terminalBadge`) — and it is told the fact, not what to
 * do about it. Which pane it was is the rail's to show, and a badge that counted would be a number a
 * reader has to go and check.
 *
 * Beside the page is the file face (`app/src/files/FilesPanel.tsx`), which is rooted at the
 * project's folder rather than at any pane's session — so it does not move when the page or the
 * focused pane does (`AMB-T-3602`). It is why this component is told which project the window is on,
 * and how to leave this face for the ledger, neither of which a terminal has any use for.
 */
export function TerminalFace({
  onSplitOut,
  note,
  onWaiting,
  projectId,
  onOpenLedger,
}: {
  onSplitOut: () => void;
  note: string | null;
  onWaiting: (waiting: boolean) => void;
  projectId?: number | null;
  onOpenLedger?: () => void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  // The face comes up with a terminal, the way a single-pane face always did: the first slot of the
  // first page is made here so there is a place for it to be in.
  const [layout, setLayout] = useState<Layout>(() => {
    const made = frameFor(EMPTY_LAYOUT, 1, 0);
    return { ...made.layout, focus: made.frame.id };
  });
  const [names, setNames] = useState<FrameNames>(new Map());
  // Whether the arrangement this device left behind has been answered for. Nothing is drawn into a
  // slot before it has: a pane put up first and replaced afterwards would start a terminal in a
  // frame the restore was about to take away (`AMB-T-3607`). Outside Tauri nothing is kept, so there
  // is nothing to wait for.
  const [settled, setSettled] = useState(!inTauri());
  // What each pane's agent has pointed at, and which sessions have ended. Both are the window's to
  // hold and nobody else's: a session has no existence outside the rectangle it runs in, so neither
  // has what was said in one (`AMB-D-749`).
  const [pointed, setPointed] = useState<PointedBySession>(new Map());
  const [ended, setEnded] = useState<ReadonlySet<string>>(new Set());
  // The pane the top row of the file face follows. A frame with nothing running in it points at
  // nothing, which is the empty row rather than the row of whichever pane spoke last.
  const focusedSession =
    layout.frames.find((frame) => frame.id === layout.focus)?.session ?? null;
  const [width, setWidth] = useState(() => (typeof window === "undefined" ? 0 : window.innerWidth));
  const [railOpen, setRailOpen] = useState(false);

  // The frames a person has just asked for a terminal in, and the one the face opens with. It is a
  // ref rather than state because it is read at the moment a pane is put up and never drawn: a frame
  // that has had its terminal started is off the list, so turning back to a page whose program exited
  // offers the way to open one again instead of quietly starting a second shell.
  const startNow = useRef(new Set<string>([layout.frames[0]!.id]));

  // Which panes have a turn standing in them — the agent said so, or the ledger says a task the pane
  // is holding is no longer ready (`../talk/plate`). It is state rather than a ref because it is
  // drawn: the page a pane is on wears a dot for it, which is how a turn on a page nobody is looking
  // at is knocked about at all (`AMB-T-3610`). The shell above is told the one fact it draws — that
  // somebody's turn has come somewhere behind this face — and the set is what says which pane.
  const [needy, setNeedy] = useState<ReadonlySet<string>>(new Set());
  // Read through a ref for the same reason the panes' callbacks are: the face is mounted once and
  // must not come down to be handed a fresh one.
  const tell = useRef(onWaiting);
  tell.current = onWaiting;

  const paneWaiting = useCallback((frame: string, is: boolean) => {
    setNeedy((was) => {
      if (was.has(frame) === is) return was;
      const next = new Set(was);
      if (is) next.add(frame);
      else next.delete(frame);
      // The shell is told the answer for the face as a whole, and only when it turns over: a second
      // pane joining the first does not knock again.
      if ((next.size > 0) !== (was.size > 0)) tell.current(next.size > 0);
      return next;
    });
  }, []);

  /**
   * The pages a turn is standing on, minus the one being shown.
   *
   * The page in front of the reader needs no dot: the panes on it are drawn, and each says for
   * itself whose turn it is (`../talk/nameplate`). A dot there would be the face telling somebody
   * about what they are looking at.
   */
  const needyPages = useMemo(() => {
    const pages = new Set<number>();
    for (const frame of needy) {
      const page = pageOfFrame(layout, frame);
      if (page !== null && page !== layout.page) pages.add(page);
    }
    return pages;
  }, [needy, layout]);

  /** The pane to go to when a person asks for the one that needs them. */
  const needsYou = useMemo(
    () => layout.frames.find((frame) => needy.has(frame.id))?.id ?? null,
    [needy, layout],
  );

  useEffect(() => {
    let alive = true;
    void frameNames().then((known) => { if (alive) setNames(known); }).catch(() => {});
    return () => { alive = false; };
  }, []);

  // The arrangement, read once as the face comes up. What comes back is places and folders and no
  // sessions, so the frames that return are offers to open a terminal — the person presses for the
  // ones they want, and a window that started them all would be starting work nobody asked for.
  useEffect(() => {
    let alive = true;
    void savedLayout()
      .then((saved) => {
        if (!alive) return;
        const back = saved === null ? null : restored(saved);
        if (back) {
          setLayout(back);
          startNow.current.clear();
        }
      })
      .catch(() => {})
      .finally(() => { if (alive) setSettled(true); });
    return () => { alive = false; };
  }, []);

  // And kept as it changes. Only the shape is written, so a session opening or closing is not a
  // write — what is kept is where the panes are, not what is in them.
  const shape = JSON.stringify(laidOut(layout));
  useEffect(() => {
    // Before the restore has been answered for, what is here is the face's own opening arrangement:
    // writing that would overwrite the one being read with a blank one.
    if (!settled || !inTauri()) return;
    void keepLayout(JSON.parse(shape) as ReturnType<typeof laidOut>).catch(() => {});
  }, [settled, shape]);

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

  // A folder chosen in a slot puts the page in a project straight away, so the slots beside it open
  // there rather than asking again (`../talk/layout`). It is separate from `opened` because a folder
  // is answered before a terminal is started, and on a machine with nothing to start it is answered
  // and no terminal follows.
  const chose = useCallback((frame: string, folder: string) => {
    setLayout((was) => settledIn(was, frame, folder));
  }, []);

  const pages = pageCount(layout);
  const drawers = sidesAreDrawers(layout.count, width);

  // ⌘1〜9 for the pages. It is caught on the document because the pane below has the keys — a
  // terminal is given every keystroke that is a character, and a page is reached with the ones that
  // are not. The face is kept mounted while the ledger is up (see above), so a face nobody is looking
  // at must not answer: `hidden` on a container above is what says so.
  // What ⌘J says when it was pressed and there was nowhere to go. It is put up by the press and taken
  // down by the next one, because it is an answer to a question rather than a state of the face.
  const [nothingNeedsYou, setNothingNeedsYou] = useState(false);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!e.metaKey && !e.ctrlKey) return;
      if (e.altKey || e.shiftKey) return;
      if (rootRef.current?.closest("[hidden]")) return;
      // ⌘J goes to the pane whose turn is standing. **It moves and sends nothing** — the pane is
      // somebody's terminal, and typing into it on their behalf is not what being told means.
      if (e.key === "j" || e.key === "J") {
        e.preventDefault();
        setNothingNeedsYou(needsYou === null);
        if (needsYou !== null) setLayout((was) => focusOn(was, needsYou));
        return;
      }
      const digit = Number(e.key);
      if (!Number.isInteger(digit) || digit < 1 || digit > MAX_PAGES) return;
      e.preventDefault();
      setLayout((was) => goPage(was, digit));
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [needsYou]);

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
              className={`termface__page${layout.page === page ? " termface__page--on" : ""}${
                needyPages.has(page) ? " termface__page--needs" : ""}`}
              // Going to a page, not turning something on: the one showing is the current page.
              aria-current={layout.page === page ? "page" : undefined}
              title={needyPages.has(page) ? t("face.needsYou") : tf("face.page", { n: page })}
              onClick={() => setLayout((was) => goPage(was, page))}
            >
              {page}
              {/* One dot, and only for a turn that is standing. A pane that finished wears nothing:
                  what is over is not something a person is needed for (`AMB-T-3610`). */}
              {needyPages.has(page) && <span className="termface__needs" aria-hidden="true" />}
            </button>
          ))}
        </nav>
        {note !== null && <span className="termface__note">{note}</span>}
        {/* Said only when it was asked for. "As far as the ledger knows" is the whole of the claim:
            a pane nobody has heard from is not a pane where all is well (`AMB-D-748`). */}
        {nothingNeedsYou && <span className="termface__note">{t("face.nothingNeedsYou")}</span>}
      </div>
      <div className="termface__body">
        {drawers
          ? railOpen && <div className="termface__drawer">{rail}</div>
          : rail}
        <div className={`termface__page-grid termface__page-grid--${layout.count}`}>
          {(settled ? slotsOf(layout, layout.page) : []).map((frame, slot) =>
            frame
              ? (
                <TerminalPane
                  key={frame.id}
                  frame={frame.id}
                  names={names}
                  start={{
                    session: frame.session,
                    // Only the first slot of the first page may take up a terminal it did not start:
                    // that is where a session split out into its own window comes back to.
                    adopt: frame.id === layout.frames[0]!.id,
                    cwd: folderOfPage(layout, layout.page),
                  }}
                  autoStart={frame.session !== null || startNow.current.has(frame.id)}
                  focused={layout.focus === frame.id}
                  onOpened={opened}
                  onChose={chose}
                  onSaid={(statement) => {
                    if (statement.cwd) {
                      setLayout((was) => movedTo(was, statement.session, statement.cwd!));
                    }
                    setPointed((was) => tookPoint(was, statement));
                  }}
                  onClosed={(session) => {
                    setLayout((was) => closedIn(was, session));
                    setEnded((was) => new Set(was).add(session));
                    // A session that has ended is not a turn anybody can take: what is over is not
                    // something a person is needed for (`AMB-T-3610`).
                    paneWaiting(frame.id, false);
                  }}
                  onName={named}
                  onFocus={(id) => setLayout((was) => focusOn(was, id))}
                  onWaiting={paneWaiting}
                />
              )
              : (
                // An empty slot is a place, and a place with room to say something: where this
                // project has work nothing is doing any more, this is where it is put (`./AdriftSlot`).
                <AdriftSlot
                  key={`${layout.page}.${slot}`}
                  folder={folderOfPage(layout, layout.page)}
                  onOpenLedger={onOpenLedger}
                  onOpen={() => setLayout((was) => {
                    const made = frameFor(was, was.page, slot);
                    startNow.current.add(made.frame.id);
                    return focusOn(made.layout, made.frame.id);
                  })}
                />
              ))}
        </div>
        <FilesPanel
          projectId={projectId ?? null}
          onOpenLedger={onOpenLedger}
          pointed={{
            points: focusedSession === null ? [] : (pointed.get(focusedSession) ?? []),
            name: layout.focus === null ? null : (names.get(layout.focus) ?? null),
            ended: focusedSession !== null && ended.has(focusedSession),
            onRead: (at) => {
              if (focusedSession !== null) setPointed((was) => markRead(was, focusedSession, at));
            },
          }}
        />
      </div>
    </div>
  );
}
