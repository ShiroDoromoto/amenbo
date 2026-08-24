import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AdriftSlot } from "./AdriftSlot";
import { FolderChoice } from "./FolderChoice";
import { TerminalPane } from "./TerminalPane";
import { PaneRail } from "./PaneRail";
import {
  frameLabel, frameNames, keepLayout, nameFrame, savedLayout, type FrameNames, type NamedBy,
} from "../talk/frames";
import {
  addPane, closedFrame, closedIn, COUNTS, EMPTY_LAYOUT, focusOn, goPage, goProject, laidOut, MAX_PAGES,
  movedTo, openedFrame, openedIn, pageCount, pageOfFrame, paneIn, restored, roomOnPage, setCount,
  sidesAreDrawers, slotsOf, type Count, type Layout,
} from "../talk/layout";
import { FilesPanel } from "../files/FilesPanel";
import { markRead, tookPoint, type PointedBySession } from "../files/pointed";
import { useBoundFolders } from "../core/boundFolders";
import { chooseFolderFor } from "../core/mutations";
import { dataAdapter } from "../mock/adapter";
import { invoke } from "../core/ipc";
import type { PtySessionDto } from "../bindings/bindings";
import { inTauri } from "../core/snapshot";
import { errText, t, tf, tn } from "../core/i18n";

/**
 * The terminal, drawn inside the board's window — the second face of the one window (`AMB-D-753`).
 *
 * **It is not an editor and it does not pick anybody's AI.** What it holds is where the work happens
 * and which project that is: projects, panes, pages, and the arrangement of them. Editing a file,
 * reading a diff, choosing a model, offering somewhere to type a prompt — none of it is here, and the
 * absence is the design rather than a gap: what an agent's own interface does well it does inside the
 * pane, weekly, and anything drawn out here would be a worse copy of it that has to be kept up
 * (`AMB-D-747`). What this face adds is the thing no agent can add for itself — several of them at
 * once, each answering for a folder, with a ledger on the other side of one switch.
 *
 * **The project is chosen first and the pane after it.** The rail names the projects; the one picked
 * there owns the whole screen, and a pane opened on it works in one of *that project's* folders
 * (`./FolderChoice`). There is no way to point a pane anywhere else, which is what makes the division
 * a division rather than a label.
 *
 * It is put up once and then left alone. Switching back to the ledger hides it with CSS rather than
 * taking it down, which is the one thing this component exists to guarantee: unmounting would take
 * the emulator with it, and a terminal whose pane went away is an agent nobody can get back to. The
 * caller therefore keeps this rendered for as long as the window is the terminal's home, and hides
 * it by hiding its own container.
 *
 * What it holds is the arrangement — which panes there are, whose project each is, which page is up,
 * how many it shows (`../talk/layout`) — because that is the one thing no pane can know: a pane is a
 * drawing of a session, and the places the drawings go are the face's. Turning a page takes panes down
 * and leaves the terminals in them running, which is the same thing splitting a window out does.
 *
 * When the window *stops* being the terminal's home — the user splits it out, or a language change
 * rebuilds the interface — this does come down, and the sessions do not: the panes detach, and this
 * face takes them up again when it comes back (see the adoption below).
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
 * `openIn` is the ledger asking for a folder to be worked in — the first loop's one button
 * (`app/src/components/FirstLoop.tsx`). It is where to work and not what to do about it: the project
 * it belongs to is the one the ledger was on, and whether a pane is made or an open one reached for
 * is this face's own (`../talk/layout`).
 *
 * Beside the page is the file face (`app/src/files/FilesPanel.tsx`), rooted at the project this face
 * is on — the one picked on the rail, not the one selected on the ledger. `projectId` is only where
 * the face **starts**: a person who came to the terminal from a project is looking at that project,
 * and after that the rail is what moves it.
 */
export function TerminalFace({
  onSplitOut,
  note,
  onWaiting,
  projectId,
  onOpenLedger,
  openIn,
}: {
  /** Take the terminal into a window of its own. What goes with it is the pane being worked in —
   *  the frame, so the window names the place it is drawing, and the terminal running there, so it
   *  takes that one up rather than guessing from a count of what is open (`../talk.ts`). Null where
   *  this face has no pane to be working in, which is the window with nothing to take up. */
  onSplitOut: (pane: { frame: string; session?: string } | null) => void;
  note: string | null;
  onWaiting: (waiting: boolean) => void;
  /** The project the face opens on, where the window has one to say. */
  projectId?: number | null;
  onOpenLedger?: () => void;
  /**
   * A folder the ledger asked this face to work in, whose project it is, and a count of the asking —
   * the same shape the file face's `show` takes, and for the same reason: pressing the button twice
   * is a reader saying it again, not a state that has not moved.
   */
  openIn?: { project: number | null; dir: string; nth: number } | null;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  // Nothing is open until somebody opens something: a pane is made by opening one (`../talk/layout`),
  // so the face comes up with the way in and no boxes.
  const [layout, setLayout] = useState<Layout>(EMPTY_LAYOUT);
  const [names, setNames] = useState<FrameNames>(new Map());
  // Whether the arrangement this device left behind has been answered for. Nothing is drawn into the
  // page before it has: a pane put up first and replaced afterwards would start a terminal in a
  // frame the restore was about to take away. Outside Tauri nothing is kept, so there is nothing to
  // wait for.
  const [settled, setSettled] = useState(!inTauri());
  // What each pane's agent has pointed at, and which sessions have ended. Both are the window's to
  // hold and nobody else's: a session has no existence outside the rectangle it runs in, so neither
  // has what was said in one (`AMB-D-749`).
  const [pointed, setPointed] = useState<PointedBySession>(new Map());
  // The file a path clicked in a pane asked for, and a count that makes asking twice two answers:
  // the same file clicked again is a reader saying "open it" again, not a state that has not moved.
  const [show, setShow] = useState<{ target: string; cwd: string | null; nth: number } | null>(null);
  const [ended, setEnded] = useState<ReadonlySet<string>>(new Set());
  // The pane the top row of the file face follows. A frame with nothing running in it points at
  // nothing, which is the empty row rather than the row of whichever pane spoke last.
  const focusedFrame = layout.frames.find((frame) => frame.id === layout.focus) ?? null;
  const focusedSession = focusedFrame?.session ?? null;
  const [width, setWidth] = useState(() => (typeof window === "undefined" ? 0 : window.innerWidth));
  const [railOpen, setRailOpen] = useState(false);
  // The question about where the pane being opened works, while it is up. It is not a frame: a place
  // is made by opening one, and one nobody finished opening is a box that says nothing
  // (`../talk/layout`). `note` on it is a binding the host refused.
  const [asking, setAsking] = useState<{ note: string | null; agent: string | null } | null>(null);

  const projects = dataAdapter.listProjects();
  const bound = useBoundFolders(layout.project);

  // The frames a person has just asked for a terminal in. It is a ref rather than state because it is
  // read at the moment a pane is put up and never drawn: a frame that has had its terminal started is
  // off the list, so coming back to a pane whose program exited offers the way to open one again
  // instead of quietly starting a second shell.
  const startNow = useRef(new Set<string>());
  // What each of those panes is to be opened with — the agent chosen on the empty frame it was
  // pressed on (`./AdriftSlot`). A ref rather than state for the same reason `startNow` is one: it
  // is read where the pane is built and never drawn, and it is this pane's alone, so nothing about
  // it belongs in the arrangement that is kept (`../talk/layout`).
  const startWith = useRef(new Map<string, string>());

  // Which panes have a turn standing in them — the agent said so, or the ledger says a task the pane
  // is holding is no longer ready (`../talk/plate`). It is state rather than a ref because it is
  // drawn: the page a pane is on wears a dot for it, and so does a project that is not the one being
  // shown, which is how a turn nobody is looking at is knocked about at all (`AMB-T-3610`). The shell
  // above is told the one fact it draws — that somebody's turn has come somewhere behind this face.
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
   * The pages of this project a turn is standing on, minus the one being shown.
   *
   * The page in front of the reader needs no dot: the panes on it are drawn, and each says for
   * itself whose turn it is (`../talk/nameplate`). A dot there would be the face telling somebody
   * about what they are looking at. A turn in **another project** is the rail's to show, for the same
   * reason: the digits are this project's pages and nothing else's.
   */
  const needyPages = useMemo(() => {
    const pages = new Set<number>();
    for (const frame of needy) {
      if (layout.frames.find((one) => one.id === frame)?.project !== layout.project) continue;
      const page = pageOfFrame(layout, frame);
      if (page !== null && page !== layout.page) pages.add(page);
    }
    return pages;
  }, [needy, layout]);

  /** The pane to go to when a person asks for the one that needs them. It may be in another project,
   *  and going to it takes the screen there — which is the whole of what being told means. */
  const needsYou = useMemo(
    () => layout.frames.find((frame) => needy.has(frame.id))?.id ?? null,
    [needy, layout],
  );

  useEffect(() => {
    let alive = true;
    void frameNames().then((known) => { if (alive) setNames(known); }).catch(() => {});
    return () => { alive = false; };
  }, []);

  // Which project the face opens on. It is taken once: after that the rail is what moves it, and a
  // face that followed the ledger's selection would take a person off the panes they were watching
  // every time they looked something up.
  const opensOn = projectId ?? projects[0]?.id ?? null;
  useEffect(() => {
    if (opensOn === null) return;
    setLayout((was) => (was.project === null ? { ...was, project: opensOn } : was));
  }, [opensOn]);

  // The arrangement, read once as the face comes up, and the terminals that are still running taken
  // up again. What comes back from the store is places and folders and no sessions, so a restored
  // pane is an offer to open a terminal — the person presses for the ones they want, and a window
  // that started them all would be starting work nobody asked for.
  //
  // **What is running is a different question, and it is answered here.** A session with no pane
  // drawing it is one this window split out and has just taken back (`AMB-D-753`): it is put in the
  // pane whose folder it is running in where there is one, and in a new pane on the project being
  // shown where there is not — a terminal nobody can see is a terminal nobody can end.
  const restoring = useRef(false);
  useEffect(() => {
    // Waiting on the project, where there is one to wait for: it answers for the panes an older
    // build kept without one (`../talk/layout`).
    if (restoring.current || (layout.project === null && projects.length > 0)) return;
    restoring.current = true;
    const onto = layout.project;
    let alive = true;
    void Promise.all([
      savedLayout().catch(() => null),
      inTauri()
        ? invoke<PtySessionDto[]>("pty_sessions").catch(() => [] as PtySessionDto[])
        : Promise.resolve([] as PtySessionDto[]),
    ])
      .then(([saved, running]) => {
        if (!alive) return;
        setLayout((was) => {
          let next = (saved === null ? null : restored(saved, onto)) ?? was;
          for (const session of running) {
            const free = next.frames.find(
              (frame) => frame.session === null && frame.folder === session.folder,
            );
            const frame = free ?? (next.project === null
              ? null
              : (() => {
                const made = openedFrame(next, next.project, session.folder);
                next = made.layout;
                return made.frame;
              })());
            if (!frame) continue;
            next = openedIn(next, frame.id, session.session, session.folder);
          }
          return next;
        });
      })
      .finally(() => { if (alive) setSettled(true); });
    return () => { alive = false; };
  }, [layout.project, projects.length]);

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

  /**
   * A path drawn in a pane was clicked, as it was drawn.
   *
   * It travels with the folder **that pane** is in, because that is what a relative one is read
   * against and the pane is the only thing that knows it. Whether it lands inside the folder the
   * file face is rooted at is the face's own question, and it is asked there (`../files/FilesPanel`).
   */
  const pathClicked = useCallback((frame: string, target: string) => {
    setLayout((was) => {
      const cwd = was.frames.find((one) => one.id === frame)?.folder ?? null;
      setShow((asked) => ({ target, cwd, nth: (asked?.nth ?? 0) + 1 }));
      return was;
    });
  }, []);

  const opened = useCallback((frame: string, session: string, folder: string | null) => {
    startNow.current.delete(frame);
    setLayout((was) => openedIn(was, frame, session, folder));
  }, []);

  /**
   * Make the pane, now that where it works has been answered.
   *
   * `agent` is what the empty frame was set to when it was pressed (`./AdriftSlot`). It rides beside
   * the frame rather than on it: what a pane opens with is that pane's, so it is not part of the
   * arrangement that comes back on the next run (`../talk/layout`), and the project's own answer is
   * kept where answers are kept (`../talk/agent`).
   */
  const openPane = useCallback((project: number, folder: string, agent: string | null) => {
    setAsking(null);
    setLayout((was) => {
      const made = openedFrame(was, project, folder);
      startNow.current.add(made.frame.id);
      if (agent !== null) startWith.current.set(made.frame.id, agent);
      return made.layout;
    });
  }, []);

  /** This project's first folder: chosen from outside the list because there is no list yet, and
   *  bound to this project rather than to one named after it (`../core/mutations`). */
  const bindFirstFolder = useCallback((project: number, agent: string | null) => {
    void chooseFolderFor(project)
      .then((chosen) => {
        // Cancelling is not a refusal and not an answer: nothing was opened, and nothing is left on
        // the screen to say it was.
        if (chosen === null) setAsking(null);
        else openPane(project, chosen, agent);
      })
      // The refusal has to be put somewhere, and where the reader was is where they still are: with
      // a folder to choose for this project.
      .catch((e: unknown) => setAsking({ note: errText(e), agent }));
  }, [openPane]);

  /**
   * Somebody asked for another pane in this project.
   *
   * Where the project is bound to one folder there is nothing to ask and the pane opens there — the
   * whole difference between the second pane in a project and the first is that the first had a
   * folder to settle. Where it is bound to several, or to none, the question goes up and no frame is
   * made until it is answered (`./FolderChoice`).
   */
  const askToOpen = useCallback((project: number, agent: string | null) => {
    setRailOpen(false);
    if (bound.live.length === 1) openPane(project, bound.live[0]!.path, agent);
    // A project bound to nothing has no list to choose from, so the press goes straight to the
    // picker: what is being answered is where this project *is*, and it is one press either way.
    else if (bound.live.length === 0) bindFirstFolder(project, agent);
    else setAsking({ note: null, agent });
  }, [bound.live, openPane, bindFirstFolder]);

  /**
   * Somebody asked for another pane, from the rail.
   *
   * **It does not open one.** What it does is go to where one would go — the page with a gap in it, or
   * a page brought into being where every one of them is full — and the empty frame there is what the
   * next press lands on (`../talk/layout`). The asking and the opening are two presses because the
   * empty frame is where what to open with is chosen, and a press that skipped it would be choosing
   * for the person.
   */
  const askForRoom = useCallback(() => {
    setAsking(null);
    setRailOpen(false);
    setLayout(addPane);
  }, []);

  // A folder the ledger handed in. Nothing is done with it before the restore has been answered for:
  // the panes that come back are what an already-open one is found among, and seeding one first would
  // put a terminal in a frame the restore was about to take away.
  //
  // The project is the one the ledger was on when the button was pressed: the first loop is a
  // project's own card, and a pane belongs to a project (`../talk/layout`).
  //
  // The press is honoured once. `nth` is what says a second press is a second answer, so pressing the
  // button again on a folder already open goes to that pane rather than opening a second terminal in
  // it.
  useEffect(() => {
    if (!settled || !openIn) return;
    const project = openIn.project ?? layout.project;
    if (project === null) return;
    setLayout((was) => {
      const open = paneIn(was, project, openIn.dir);
      if (open) return focusOn(was, open.id);
      const made = openedFrame(was, project, openIn.dir);
      // The same mark the rail's way in leaves: this frame is one a person pressed for, so the pane
      // opens rather than offering to.
      startNow.current.add(made.frame.id);
      return made.layout;
    });
    // `nth` is what makes the same folder asked for twice two answers.
  }, [openIn?.nth, settled]);

  const drawers = sidesAreDrawers(layout.count, width);
  // The paths alone, and a stable one per set of them: the empty frame reads what the agents are
  // traced across off this, and a fresh array every render would send it back to the host on every
  // keystroke elsewhere on the page (`./AdriftSlot`).
  const boundPaths = useMemo(() => bound.live.map((one) => one.path), [bound.live]);
  const page = layout.page;
  const slots = slotsOf(layout, page);
  const pages = pageCount(layout);
  // The one empty frame this page draws, where it has a gap to draw it in (`../talk/layout`). The
  // question about where a pane works stands in its place while it is up, because that is where the
  // answer appears: a question drawn anywhere else is one the reader has to go and find.
  const room = roomOnPage(layout, page);

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
      projects={projects}
      needy={needy}
      onProject={(project) => {
        setAsking(null);
        setLayout((was) => goProject(was, project));
        setRailOpen(false);
      }}
      onPick={(frame) => {
        setAsking(null);
        setLayout((was) => focusOn(was, frame));
        setRailOpen(false);
      }}
      onRename={(frame, name) => named(frame, name, "person")}
      onOpen={askForRoom}
    />
  );

  return (
    <div className={`termface${drawers ? " termface--drawers" : ""}`} ref={rootRef}>
      <div className="termface__bar">
        <button
          className="termface__action"
          onClick={() => onSplitOut(
            focusedFrame === null
              ? null
              : {
                  frame: focusedFrame.id,
                  ...(focusedSession === null ? {} : { session: focusedSession }),
                },
          )}
        >
          {t("face.splitOut")}
        </button>
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
            press it to find out. It is the most a page draws and not a number of boxes to fill —
            what is open is what is on the screen (`../talk/layout`).
            **It says what the number counts**, because the row of pages beside it is digits too: two
            rows of bare digits is a reader pressing one to find out which is which. */}
        <div className="termface__counts" role="radiogroup" aria-label={t("face.paneCount")}>
          {COUNTS.map((count) => (
            <button
              key={count}
              className={`termface__count${layout.count === count ? " termface__count--on" : ""}`}
              // One of three, and exactly one: a toggle each would say three independent things can
              // be on, which is not what the control does.
              role="radio"
              aria-checked={layout.count === count}
              // The question about where a pane works goes with it, the same way it goes when a page
              // or a pane is reached for: asking for a different split is a person doing something
              // else, and a question left up would be drawn on whatever page the split lands on.
              onClick={() => {
                setAsking(null);
                setLayout((was) => setCount(was, count as Count));
              }}
            >
              {tn("face.panes", count)}
            </button>
          ))}
        </div>
        {/* The pages of this project, as a row of the digits that reach them. **A project with one
            page draws none of it**: a single page nobody can go anywhere from is a control that says
            only where the reader already is. */}
        {pages > 1 && (
          <nav className="termface__pages" aria-label={t("face.pages")}>
            {Array.from({ length: pages }, (_, i) => i + 1).map((one) => (
              <button
                key={one}
                className={`termface__page${page === one ? " termface__page--on" : ""}${
                  needyPages.has(one) ? " termface__page--needs" : ""}`}
                // Going to a page, not turning something on: the one showing is the current page.
                aria-current={page === one ? "page" : undefined}
                title={needyPages.has(one) ? t("face.needsYou") : tf("face.page", { n: one })}
                onClick={() => { setAsking(null); setLayout((was) => goPage(was, one)); }}
              >
                {one}
                {/* One dot, and only for a turn that is standing. A pane that finished wears nothing:
                    what is over is not something a person is needed for (`AMB-T-3610`). */}
                {needyPages.has(one) && <span className="termface__needs" aria-hidden="true" />}
              </button>
            ))}
          </nav>
        )}
        {note !== null && <span className="termface__note">{note}</span>}
        {/* Said only when it was asked for. "As far as the ledger knows" is the whole of the claim:
            a pane nobody has heard from is not a pane where all is well (`AMB-D-748`). */}
        {nothingNeedsYou && <span className="termface__note">{t("face.nothingNeedsYou")}</span>}
      </div>
      <div className="termface__body">
        {drawers
          ? railOpen && <div className="termface__drawer">{rail}</div>
          : rail}
        {/* The page is the split that was asked for, whether or not there are panes to fill it: the
            count is the most a page draws, and a grid that shrank to what is open would make the
            split a thing a reader cannot see the effect of (`../talk/layout`). */}
        <div className={`termface__page-grid termface__page-grid--${layout.count}`}>
          {!settled || layout.project === null
            ? null
            : (
              <>
                {slots.map((frame) => (
                  <TerminalPane
                    key={frame.id}
                    frame={frame.id}
                    project={frame.project}
                    names={names}
                    start={{
                      session: frame.session,
                      // Nothing on this face takes up a terminal it was not given: which session
                      // belongs where is answered once, as the face comes up, and a pane left to
                      // guess would take the one running terminal off whichever pane had it.
                      adopt: false,
                      cwd: frame.folder,
                      agent: startWith.current.get(frame.id) ?? null,
                    }}
                    autoStart={frame.session !== null || startNow.current.has(frame.id)}
                    focused={layout.focus === frame.id}
                    onOpened={opened}
                    onPath={pathClicked}
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
                    onDrop={(id) => {
                      setLayout((was) => closedFrame(was, id));
                      // Nothing is owed to a place that has gone — a turn standing in it was standing
                      // on the page it was on, and the badge above must stop counting it.
                      paneWaiting(id, false);
                      startNow.current.delete(id);
                      startWith.current.delete(id);
                    }}
                    onName={named}
                    onFocus={(id) => setLayout((was) => focusOn(was, id))}
                    onWaiting={paneWaiting}
                  />
                ))}
                {asking !== null && (
                  <FolderChoice
                    folders={bound.live}
                    onPick={(folder) => openPane(layout.project!, folder, asking.agent)}
                    onBind={() => bindFirstFolder(layout.project!, asking.agent)}
                    note={asking.note}
                  />
                )}
                {/* One empty frame, at the first gap on the page, and none at all on a full one: it
                    is this page saying it has room (`./AdriftSlot`). */}
                {asking === null && room && (
                  <AdriftSlot
                    folders={boundPaths}
                    project={layout.project}
                    onOpenLedger={onOpenLedger}
                    onOpen={(agent) => askToOpen(layout.project!, agent)}
                  />
                )}
              </>
            )}
        </div>
        <FilesPanel
          projectId={layout.project}
          onOpenLedger={onOpenLedger}
          show={show}
          pointed={{
            points: focusedSession === null ? [] : (pointed.get(focusedSession) ?? []),
            // Which pane pointed, called what the rail and its own row call it (`../talk/frames`).
            name: focusedFrame === null ? null : frameLabel(names, focusedFrame.id, focusedFrame.folder),
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
