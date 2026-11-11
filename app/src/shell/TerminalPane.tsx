import { memo, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";
import { mountAgentFrame } from "../talk/agent";
import {
  boxHeight,
  endTerminal,
  focusTerminal,
  leavesForTerminal,
  mountTail,
  passedOn,
  pasteIntoTerminal,
  pressIntoTerminal,
  quotedPaths,
  sendIntoTerminal,
  sendsWhatIsWritten,
  stopsTheProgram,
  whyItStopped,
} from "../talk/terminal";
import { mountPlate, type Plate } from "../talk/plate";
import type { Plate as Row, Say } from "../talk/nameplate";
import { acknowledgeRun, stopLaunch, useLaunchState } from "../core/automations";
import { FacetAvatar, facetActor } from "../components/atoms";
import { confirmDialog, pickFiles, pickFolders } from "../core/dialog";
import { watchHostDrop } from "../core/hostDrop";
import { takesPastedFiles, takesPastedImages, writesPastedImage } from "../core/clipFiles";
import { pushNotice } from "../core/notice";
import { setComposeStartsOpen } from "../core/composeStartsOpen";
import { Menu, MenuItem } from "../components/Menu";
import { PaneMade, type Made } from "./PaneMade";
import type { FrameNames, NamedBy } from "../talk/frames";
import type { PaneStart } from "../talk/terminal";
import type { SessionMadeDto, SessionSaidDto } from "../bindings/bindings";
import { currentLang, errText, t, tf } from "../core/i18n";
import { useRefNav, type RunsTab } from "../core/refNav";
import { asTyped, isComposing, isEnterSubmit } from "../core/keys";
import { hostOs } from "../core/platform";
import { Icon } from "../components/Icon";
import { PaneModel } from "./PaneModel";
import { PaneSize } from "./PaneSize";
import { RunActs } from "./RunActs";
import { LaunchAttention, LaunchHead } from "./LaunchHead";
import { BuiltinCard } from "./BuiltinCard";
import { RunRoles, RunTermBar, useRoundFellLines } from "./RunPicture";
import { faceOf, keepFace, onTerminalShown, showTerminalOf, type Face } from "./runFaces";
import { RunOverCard, RunPausedCard } from "./RunBody";
import { useBoxNumber } from "../screens/boxNumber";
import type { BuiltinRun } from "../talk/automationStep";
import type { Size } from "../talk/layout";

/**
 * Put a record on the band's list, or leave the list as it is where it is already there.
 *
 * **One record is one entry however many times it arrives.** A count that went up twice for one
 * `task add` would be a number the pane cannot stand behind, which is the whole point of counting
 * commands that ran (`./PaneMade`). It arrives twice for a plain reason: what a session filed while
 * nothing was drawing it is handed over as a pane takes it up, and a pane that was there for it
 * heard the same record go past (`../talk/terminal`).
 */
function withRecord(made: Made[], one: SessionMadeDto): Made[] {
  return made.some((h) => h.space === one.kind && h.num === one.id)
    ? made
    : [...made, { space: one.kind, num: one.id }];
}

/**
 * Put the paths of what was dropped on a pane in front of whatever is running there (`AMB-D-820`).
 *
 * **Nothing is moved and nothing is copied**, so the host is not asked anything: what goes into the
 * terminal is where the file or the folder is now, exactly as the drop handed it over. A copy would
 * leave the reader with two of the same file and only one of them would ever be seen to change — a
 * fix made in the other shows neither in git's diff nor in the row of what changed (`AMB-D-785`).
 *
 * **Nothing is typed for the reader.** The paths are pasted and the newline is not sent
 * (`../talk/terminal`), so what happens next is theirs.
 *
 * Each path is quoted on its own. A screenshot's name has spaces in it on all three machines, and
 * with several of them the space between two paths would otherwise be the same character as the
 * space inside one (`AMB-D-801`).
 */
async function handOver(session: string, paths: string[]) {
  if (paths.length === 0) return;
  try {
    await pasteIntoTerminal(session, quotedPaths(paths));
  } catch (e: unknown) {
    // The terminal having ended between the drop and the paste is the whole of what this can be,
    // and it is worth saying rather than swallowing.
    pushNotice(errText(e));
  }
}

/**
 * One slot of the workspace: a frame, and the terminal in it when there is one.
 *
 * **A frame is a place, so an empty one is not nothing.** It is a slot on this page with a way to
 * open a terminal in it, and it stays a place after the program in it exits — what is on the screen
 * is what a terminal ends with, and taking the pane away would be the app deciding the reader had
 * finished reading it. So this puts the frame up once and then keeps it, whatever happens to the
 * process: what runs in it, what is offered when nothing can be started, and the row a closed frame
 * carries are all the frame's (`../talk/agent`).
 *
 * The pane comes down when the slot stops being on the face — carried onto another page, or fewer
 * panes were asked for — and **the terminal does not**: a pane is a drawing of a session, and
 * detaching leaves the session running for whichever slot draws it next (`../talk/terminal`). That is
 * why the slot's session id is handed back up: the frame is what remembers, and this is only what
 * draws.
 *
 * **The one control on the row removes the place**, which is the only thing on this face that does
 * (`../talk/layout`). It is not the same act as a program ending: what a terminal exits with stays on
 * the screen to be read, and a page that closed up under a reader because a shell finished would be
 * the app deciding they were done with it. So this is asked before it happens, and it ends whatever is
 * running on the way out — a session whose place has gone is one nobody can reach. On a launch's pane
 * that is the launch itself (`AMB-D-1154`): a launch and its pane are one to one.
 */
function Pane({
  frame, terminal, launch = null, at, hue, project, names, start, autoStart, focused, landed = false, offered = false, written,
  inserted = [], composeOpen, held = false, goes = null, run = null, expected, builtin = null, tail = null, tailCode = null, onGrab, onStretch,
  size, onSize,
  on,
}: PaneProps & {
  /** The handlers {@link TerminalPane} was handed on its newest render ({@link Handlers}). */
  on: { readonly current: Handlers };
}) {
  // The name this pane's terminal goes by on the host's side, which its name is kept against
  // (`../talk/frames`): the place's own, except on a launch's pane, where it is the seat's
  // (`../talk/layout`'s `terminalOf`).
  const termName = terminal ?? frame;
  const paneRef = useRef<HTMLDivElement>(null);
  /** Whether a press of the close button is still being answered. */
  const dropping = useRef(false);
  const labelRef = useRef<HTMLDivElement>(null);
  const plateRef = useRef<Plate | null>(null);
  // Once a terminal has been asked for here it stays asked for: a slot whose program exited keeps the
  // frame, and a person who pressed the button once has not un-pressed it.
  const [running, setRunning] = useState(autoStart);
  // The fact of the program having exited, which the screen cannot show on its own — what a finished
  // shell leaves behind looks exactly like one waiting to be typed at.
  const [ended, setEnded] = useState(false);
  // And why, for the few endings Amenbo had a hand in and the screen cannot own up to
  // (`../talk/terminal`). Null for every other ending, which is nearly all of them.
  const [stopped, setStopped] = useState<string | null>(null);
  // What the program exited with, as the pane heard it close — said on a failed run's terminal, under
  // what it left (`useRoundFellLines`). Null before it ends, and where it could not be asked.
  const [exitCode, setExitCode] = useState<number | null>(null);
  // The session running here, while one is. It is what the way out names, and it is null at exactly
  // the two moments there is nothing to end: before a terminal has opened, and after one has closed.
  const [live, setLive] = useState<string | null>(null);
  // What is running in it, as the session says it was started (`../talk/terminal`). It is read off the
  // session rather than off what this pane asked for, because a pane that adopted one never asked:
  // the program in a terminal was settled when it started, and the row that moves it to another model
  // is a question about that program (`./PaneModel`).
  const [inPane, setInPane] = useState<string | null>(null);
  // The same, as the handlers below read it. They are made once, when the terminal is mounted, so the
  // state above is the value it had then and never what `opened` put there a moment later.
  const inPaneRef = useRef<string | null>(null);
  // Where the row's menu was opened, while it is open. It is placed at the press rather than under
  // the button for the reason every other menu in the app is (`../components/Menu`).
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  // What the session in this pane has filed, in the order it filed them (`./PaneMade`). It is held
  // here and nowhere else: the notes are this terminal's, and a count written down would outlive the
  // session it is a count of (`AMB-D-897`).
  const [made, setMade] = useState<Made[]>([]);
  // Whether a drag from outside is over this pane. It is the whole of the receiving surface: nothing
  // is drawn until something is being carried, and what is carried is only known while it hangs there.
  const [handing, setHanding] = useState(false);
  // Whether the row is being named, and the box it is named in while it is. A frame's name is the
  // person's last word on it (`../talk/frames`), so it is typed on the row it belongs to rather than
  // in a window over the pane: what is being named is the line the box stands in.
  const [naming, setNaming] = useState(false);
  const nameField = useRef<HTMLInputElement>(null);
  // Whether the box under this pane is folded away (`AMB-D-889`). A pane whose reader has never said
  // comes up folded — its presses go to the program, which is what a pane is before anybody asks for
  // anything else: a person who writes paragraphs opens it, and a slash command typed at a CLI needs
  // the characters to reach the program to be completed at all, which a box in front of them would
  // stop.
  //
  // **Where it starts is what this machine last chose, and where it is now is the window's**
  // (`../core/composeStartsOpen`, `../talk/layout`). The habit answers a pane being made and nothing
  // after it: read again later, a press in one pane would fold another, and each fold wakes the
  // program inside to repaint (`AMB-D-864`). What this pane is now outlives the drawing, so the box
  // is still open in the window a terminal is split out into and in the next run (`AMB-D-890`).
  const folded = !composeOpen;
  // The box itself, which is measured rather than told how tall to be: how many lines a sentence
  // takes is the browser's answer, not one this can work out from the characters.
  const boxRef = useRef<HTMLTextAreaElement>(null);
  // Where the caret goes once a pasted path has been put in, and null whenever it is where the
  // browser left it. What is written in the box is the window's (`../talk/layout`), so text put in
  // here comes back down as a new value and the browser puts the caret at the end of it — which is
  // the wrong place for a paste made in the middle of a sentence.
  const caret = useRef<number | null>(null);
  // How many rows the terminal is drawing, as the emulator last measured it (`../talk/terminal`).
  // It is what turns the floor below into pixels, and it is 0 only before a terminal has said.
  const paneRows = useRef(0);
  // The height of one line in the box, taken while nothing is written in it. It is read rather than
  // computed from the line-height, so a font that rounds its lines differently is still one line.
  const oneLine = useRef(0);

  /** One of the run's moves at a time (`AMB-T-5507`), as on the "running" tab: a second press landing
   *  while the first is still opening a terminal would be answered off a run that has already moved.
   *  A refusal is said on the toast — the run may have ended a moment before the press. */
  const [pressing, setPressing] = useState(false);
  // Which face a run's pane is turned to. An ordinary pane has the terminal and nothing else.
  //
  // **A launch's pane with no seat in front yet has its roles and nothing else** — the launch waits for
  // a task to take and has no run (`../talk/layout`'s `stoodForLaunch`). There is no terminal to turn
  // to, and the first seat that arrives is a pane drawn afresh (`./WorkspaceFace` keys it by the seat).
  const seatless = run === null && launch !== null;
  const [face, setFace] = useState<Face>(() =>
    run === null ? (seatless ? "roles" : "terminal") : faceOf(run.run) ?? "roles");
  const runId = run?.run ?? null;
  // **A step that may wait for a person turns the pane to its terminal** (`AMB-T-5776`), so a question
  // it asks is not left behind the roles nobody reads it in. Whether the agent is waiting is not
  // something the app reads (`AMB-D-858`); the step's own `interactive` is what it goes by. The turn is
  // the step's, not the reader's: it is not kept, and once the step is over the pane goes back to the
  // face the reader last chose — the terminal, where that is what they chose. A press while the step
  // runs is the reader's choice, and is kept like any other.
  //
  // **Where the step turned it, the pane says so** in one line over the terminal: a pane that changed
  // under a reader with nothing to say why is one they cannot trust to stay put. The line is the
  // step's turn only — a pane the reader had on its terminal already was not turned.
  const asks = run !== null && run.interactive && (run.state === null || run.state.status === "running");
  const stepAt = run === null ? null : `${run.placement ?? ""}:${run.step}`;
  const [byStep, setByStep] = useState(false);
  useEffect(() => {
    if (runId === null) return;
    const kept = faceOf(runId);
    setFace(asks ? "terminal" : kept ?? "roles");
    setByStep(asks && kept !== "terminal");
  }, [runId, asks, stepAt]);
  const turn = (to: Face) => {
    if (runId !== null) keepFace(runId, to);
    setFace(to);
    setByStep(false);
  };
  /** A seat pressed: the one in front turns this pane to its terminal, and another is put in front in
   *  this same place, on its terminal (`onGoToRun`). */
  const openSeat = (other: number) => {
    if (other === run?.run) {
      turn("terminal");
      return;
    }
    showTerminalOf(other);
    on.current.onGoToRun?.(other);
  };
  // A press elsewhere turning this run to its terminal (`./runFaces`): its seat pressed among the roles
  // of its launch's pane, or the task this run's failed round was filed as.
  useEffect(() => {
    if (runId === null) return;
    return onTerminalShown((one) => {
      if (one !== runId) return;
      setFace("terminal");
      setByStep(false);
    });
  }, [runId]);
  const onRoles = (run !== null || seatless) && face === "roles";
  const press = async (act: () => Promise<unknown>) => {
    setPressing(true);
    try {
      await act();
    } catch (e) {
      pushNotice(errText(e));
    } finally {
      setPressing(false);
    }
  };

  /** **The run this pane draws is still going or held** — the two states the run's own moves are
   *  drawn for, and the two a run's own pane cannot be taken away in (`AMB-T-5529`). Stopping is its
   *  own press beside the state, so taking the pane away never has to stop a run on the way: a run
   *  left going with its pane gone would be one nobody could see, reach or stop. A run not read yet is
   *  not held to this — a pane whose run has gone from under its id would otherwise stand there for
   *  good. A launch's pane is not held to it either: taking it away stops the launch ({@link drop}). */
  const runLive = run?.state?.status === "running" || run?.state?.status === "paused";
  // Over, as the row says it — ended one of the three ways a run ends. Not read off `runLive`: a run's
  // state is null until it is first read, and that is not over.
  const over =
    run?.state?.status === "completed" || run?.state?.status === "failed" || run?.state?.status === "canceled";
  // Failed — the run whose terminal says how it ended and where the failure was filed, as its last
  // lines (`useRoundFellLines`): under the screen kept from it, which stands in for the terminal once
  // its program has ended — an agent wipes its screen as it ends, and what it wiped is in the kept one.
  const fell = run?.state?.status === "failed";
  const fellLines = useRoundFellLines(fell ? run : null, project, exitCode ?? tailCode);
  // Where a run's pane sends the reader on the ledger — the shell's in one window, the host's across two.
  const ledger = useRefNav();
  // The tab an over run is listed on: a failure waits on "running" until somebody acknowledges it.
  const runsTab: RunsTab =
    run?.state?.status === "failed" && !run.state.acknowledged ? "running" : "history";

  // The launch this pane draws, read for whether it has ended: one that has is not stopped again.
  const launchState = useLaunchState(launch);
  // The close button cannot be pressed: a run's own pane while its run is going or held.
  const stuck = launch === null && runLive;
  // Taking the pane away stops the launch it draws: one not ended yet.
  const stopsLaunch = launch !== null && launchState?.status !== "ended";

  /** Take the place away, once the person has said so. The terminal in it is ended first: a session
   *  whose pane has gone is one nobody can get back to.
   *
   *  **The one question is the plain one, whatever the session was doing** (`AMB-D-858`). What tied a
   *  pane to a task went through a key the world could rewrite behind the pane, so a question naming
   *  what was about to be lost named as often work somebody had already finished elsewhere.
   *  `face.dropConfirm` says what the loss is: the place, and with it the handle the talk in it is
   *  resumed from — so this press is what closes the way back into that conversation for good
   *  (`AMB-D-869`). It is the heavier of the app's two questions now, the way out of the app being
   *  the lighter one.
   *
   *  **A run's own pane — one no launch draws — is not taken away while its run is going or held**
   *  ({@link stuck}): the press is drawn and cannot be pressed. A pane press that stopped a run would
   *  be one a reader made meaning only to tidy the page; stopping is its own press beside the run's
   *  state. Once the run is over the pane goes without a question.
   *
   *  **A launch's pane is always asked about, and taking it away stops the launch** (`AMB-D-1154`),
   *  whether a seat is in front, going or held, or none yet. A launch left going with its pane gone
   *  waited on unseen and took the next task. The question says the launch stops with the pane; a
   *  "no" leaves the pane, the terminal and the launch as they were. A launch already ended is not
   *  stopped again — core refuses it — and its pane is asked about as any other place. Where the stop
   *  is refused the pane stays, and the refusal is said on the toast.
   *
   *  **One press at a time** ({@link dropping}): presses that pile up while the app is busy would
   *  otherwise each ask again, or take the frame away twice. */
  const drop = async () => {
    if (stuck || dropping.current) return;
    dropping.current = true;
    try {
      if (launch !== null && stopsLaunch) {
        const labels = { ok: t("auto.launch.stop"), cancel: t("auto.run.keepRunning") };
        if (!await confirmDialog(t("face.dropLaunchConfirm"), labels)) return;
        try {
          await stopLaunch(launch);
        } catch (e) {
          pushNotice(errText(e));
          return;
        }
      } else if (launch !== null || run === null) {
        // A run's own pane is not asked about: there is no way back into a step's conversation to
        // lose — every step opens one of its own — and the run it drew is over.
        if (!await confirmDialog(t("face.dropConfirm"))) return;
      }
      if (live !== null) await endTerminal(live).catch(() => {});
      on.current.onDrop(frame);
    } finally {
      dropping.current = false;
    }
  };

  /**
   * Measure the box and the pane, and set the box to the height {@link boxHeight} answers with
   * (`AMB-D-864`). Everything decided is decided there; this is the reading and the writing.
   */
  const fitBox = () => {
    const box = boxRef.current;
    const face = paneRef.current;
    if (box === null || face === null) return;
    const standing = box.getBoundingClientRect().height;
    const pane = face.getBoundingClientRect().height;
    // Measured with the height let go of, or a box already at three lines reports three lines for a
    // sentence that has come back down to one.
    box.style.height = "auto";
    const content = box.scrollHeight;
    // One line is read while nothing is written, which is the only moment the box is one line by
    // itself. It is read rather than computed off the line-height, so a font that rounds its lines
    // differently is still one line here.
    if (written === "") oneLine.current = content;
    box.style.height = `${boxHeight({
      content,
      standing,
      line: oneLine.current || content,
      pane,
      rows: paneRows.current,
    })}px`;
  };

  // Read at the moment the pane is measured rather than when the watch was taken up, so a watch that
  // outlives a render still fits the box to what is written now.
  const fitting = useRef(fitBox);
  fitting.current = fitBox;

  // What is written decides the height, so this runs after every render that could have changed it.
  useEffect(() => { fitBox(); });

  // And so does the room there is for it: a window pulled shorter has to take the box down with it,
  // and nothing about what is written has changed. It settles rather than looping — the height this
  // sets is the height the next measurement asks for.
  useEffect(() => {
    const face = paneRef.current;
    if (face === null) return;
    const watch = new ResizeObserver(() => fitting.current());
    watch.observe(face);
    return () => watch.disconnect();
  }, [live]);

  // Whether this pane draws a script step's terminal, which nobody writes into (`builtin`).
  const script = builtin?.program !== undefined;

  /** Send what has been written to the program in the pane, as the person's own line
   *  (`../talk/terminal`), and empty the box behind it. */
  const send = async () => {
    if (live === null || written === "") return;
    try {
      await sendIntoTerminal(live, written, inPane, inserted);
    } catch (e: unknown) {
      // The terminal having ended between the writing and the send is the whole of what this can be.
      // What was written stays in the box: it did not go, and a box emptied on a refusal would have
      // thrown away the only copy of it.
      pushNotice(errText(e));
      return;
    }
    // Emptied only once the line has actually gone. What follows it is a sentence of its own, and a
    // box that kept what was sent would make the next one the tail of the last.
    on.current.onWrite(frame, "");
  };

  /**
   * What a press in the box means, which is decided by what is in the box and by two presses that
   * do not ask (`AMB-D-864`, `AMB-D-876`).
   *
   * **Enter is another line and the send is `⌘/Ctrl+Enter`** (`../talk/terminal`), the box's either
   * way however empty it is. An Enter that is settling a conversion is neither (`../core/keys`), so
   * it is asked about before anything is done with the press.
   *
   * **`Escape` and `Ctrl+C` go to the program whatever is written.** They are the exception the rule
   * below makes, and they keep the keyboard where it is: stopping something is not leaving the
   * sentence, and a person who reaches for one of these usually means to carry on writing.
   *
   * **Everything else is the box's only while something is written in it.** An empty box has no
   * history to walk and no word to complete, so the presses that mean those things go to the program
   * instead, and the keyboard stays in the box for the next character.
   *
   * **The one way out is the ArrowUp on the first line, empty box or written one** (`AMB-T-4788`).
   * It moves the keyboard to the terminal and goes there itself, so a menu the program is drawing is
   * walked by the one press rather than by a press to leave and a press to move — and what is walked
   * can then be chosen, which an empty box's handed-on ArrowUp left no way to do. What is written
   * stays where it is.
   */
  const pressed = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (isEnterSubmit(e)) {
      if (!sendsWhatIsWritten(e)) return;
      e.preventDefault();
      void send();
      return;
    }
    if (live === null) return;
    // Everything below this hands the press to the program, and a press the input method is still
    // using is not one to hand anywhere: it is walking a list of candidates, accepting one or taking
    // the conversion back, and it only looks like `Escape`, `ArrowUp` or `Enter` from outside. The
    // terminal beside this box is guarded by the emulator's own composition helper, and this is the
    // same guard for the box, which reads its presses itself (`AMB-T-4777`).
    //
    // It is the one thing above the two that leave whatever is written (`AMB-D-876`): those go to
    // the program because neither has anything to do in a textarea, and mid-conversion `Escape` has
    // — it takes the conversion back.
    if (isComposing(e)) return;
    const stop = stopsTheProgram(e);
    if (stop !== null) {
      e.preventDefault();
      void pressIntoTerminal(live, stop).catch(() => {});
      return;
    }
    const out = leavesForTerminal(e, written, e.currentTarget.selectionStart);
    if (out !== null) {
      e.preventDefault();
      focusTerminal(paneRef.current);
      void pressIntoTerminal(live, out).catch(() => {});
      return;
    }
    if (written !== "") return;
    const data = passedOn(e);
    if (data === null) return;
    e.preventDefault();
    void pressIntoTerminal(live, data).catch(() => {});
  };

  /** What a press on this pane says, and where the keyboard goes because of it.
   *
   *  **A press that moves the focus hands the keyboard on with it** (`AMB-D-864`). The frame moving
   *  and the keyboard moving are one thing to the person doing it: they pressed the pane they mean
   *  to work in, and the next thing they do is type. Left to itself only the frame moved, and the
   *  characters went on landing in the pane they came from — the same disagreement a drop had
   *  before `AMB-T-4182` settled it there.
   *
   *  **Where it lands is whether the box is open** (`AMB-D-889`). A folded pane has no box to put it
   *  in and is a pane being worked through its program, so the keyboard goes to the terminal — which
   *  is also where a press on the terminal itself would have left it, and this is what keeps a press
   *  landing anywhere else in the pane from ending somewhere different.
   *
   *  **Only the press that moves it.** A press inside the pane already being worked in is left
   *  alone, so pressing the terminal is still how the keyboard is handed to the program running in
   *  it. What decides is what the pane was before the press, which is what `focused` still says
   *  here: `onFocus` is what changes it, and it is answered on the render after this one.
   *
   *  **A pane with nothing running in it has neither** a box nor a terminal to put the keyboard in,
   *  so nothing is moved and it stays where the person left it.
   *
   *  **Nor is anything moved in a run's pane turned to its roles.** Its terminal is still there
   *  under them, and what is typed into it reaches the agent running there — which nobody looking at
   *  the roles meant to type at. A press on the roles only picks the pane; the way to the program is
   *  turning to the terminal.
   *
   *  It comes after the emulator has had the press — that one takes the keyboard on its own
   *  mousedown, from an element inside this one — so this is the last word rather than the first.
   */
  const pressedOn = () => {
    const moving = !focused;
    on.current.onFocus(frame);
    if (!moving || onRoles) return;
    if (folded) focusTerminal(paneRef.current);
    else boxRef.current?.focus({ preventScroll: true });
  };

  /** What the press beside the box is called, with the keys that do the same thing in it
   *  (`AMB-D-876`). The two spellings are the machine's own — `⌘Enter` where the application's key
   *  is `⌘`, `Ctrl+Enter` on the other two — and neither is translated: they are the marks on the
   *  keyboard in front of the reader.
   *
   *  It is a key of its own rather than the plain "Send", which is still what a button that only
   *  sends is called (`./PaneModel`). */
  const sendLabel = tf("face.composeSendKeys", {
    keys: hostOs() === "macos" ? "⌘Enter" : "Ctrl+Enter",
  });

  useEffect(() => {
    if (!running) return;
    const host = paneRef.current;
    const label = labelRef.current;
    if (!host || !label) return;
    let taken = false;
    let detach: (() => void) | null = null;
    setEnded(false);
    // The line above the pane: what this pane is called, and the lamp that says whether anything is
    // coming out of it (`../talk/plate.ts`).
    const plate = mountPlate(label, termName, hue, run, launch !== null);
    plateRef.current = plate;
    // The row is readable from outside for as long as this pane is drawn, and no longer: a pane on
    // another page is not being measured at all, so a reading kept past this point would be the last
    // one this pane took rather than what is true now.
    on.current.onRow?.(frame, plate.read);
    void mountAgentFrame(host, currentLang(), {
      opened: (session, where, running, lastOutput) => {
        // The folder is what the row above the pane calls it until something names the frame
        // (`../talk/frames`), and it is the one the terminal actually runs in — which is not always
        // the one this slot was handed.
        plate.opened(where ?? start.cwd ?? null, lastOutput ?? null);
        setLive(session);
        // What the band counts belongs to the session and not to the place: a pane opened again is a
        // new session, and the records the last one filed are not this one's (`./PaneMade`).
        setMade([]);
        setInPane(running);
        inPaneRef.current = running;
        // Where the terminal actually runs and what is in it, neither of which is always what this
        // slot was handed: a pane that took one up learns both from the session (`../talk/layout`).
        on.current.onOpened(frame, session, where ?? start.cwd ?? null, running);
      },
      // A path drawn in this pane was clicked. Where it leads is the face's to work out — it knows
      // the folder this frame is in and the one the file face is rooted at (`AMB-T-3630`).
      path: (target) => on.current.onPath(frame, target),
      // Straight through and nowhere else: the row above the pane is the only thing that reads it, and
      // what it reads is the time (`../talk/moving`).
      output: () => plate.output(),
      // Nothing on this face is opened without a folder — the question is answered before the pane
      // is made (`./FolderChoice`) — so there is no choice for the frame to report.
      chose: () => {},
      said: (statement) => {
        // The band's own half of a statement, taken on the way past.
        if (statement.made) setMade((was) => withRecord(was, statement.made!));
        on.current.onSaid(statement);
      },
      closed: (session, code, noWayBack) => {
        plate.closed();
        setEnded(true);
        setExitCode(code);
        // What the program was is taken off the pane a line below, so why it stopped is settled here
        // while the two are still together.
        setStopped(whyItStopped(inPaneRef.current, code, noWayBack));
        setLive(null);
        setInPane(null);
        inPaneRef.current = null;
        on.current.onClosed(session);
      },
      // How many rows the terminal is drawing now, which is the one measurement the box below it
      // cannot take for itself (`../talk/terminal`). It is kept rather than pushed up: it changes
      // whenever the pane does, and a value in state would redraw the face for a number only the box
      // reads.
      sized: (_cols, rows) => {
        paneRows.current = rows;
        fitting.current();
      },
      // The window's own title is not the pane's to say — a face holds several panes, in either of
      // the windows it is drawn in. The name goes to the store, and what draws it is the line above
      // the pane.
      name: (text, by) => on.current.onName(termName, text, by),
      // What the session said while nothing was drawing it. Both halves land where the live ones do
      // — the band's count, and the name on the row — so a pane that was away reads the same as one
      // that was there for all of it (`../talk/terminal`).
      carried: (said, filed) => {
        setMade((was) => filed.reduce(withRecord, was));
        if (said !== null) on.current.onName(termName, said, "session", true);
      },
    }, start, project)
      .then((take) => {
        // Taken away while the host was still answering. Detaching leaves the terminal running for
        // whatever draws it next, which is exactly what a pane that never got shown should do.
        if (taken) take();
        else detach = take;
      })
      .catch(() => {});
    return () => {
      taken = true;
      detach?.();
      plate.stop();
      plateRef.current = null;
      on.current.onRow?.(frame, null);
      // **The turn is not taken down with the pane.** A pane goes away when it is carried onto
      // another page or the tasks face comes up, and neither of those is the agent finishing:
      // saying the turn was over would be saying it because of where the pane is drawn. What ends a
      // turn is the pane saying so, or the session ending.
      //
      // Nothing here has to hold that open. The row above the pane goes with the pane and says so on
      // its way out (`../talk/plate`).
    };
    // Only `running` is a reason to do any of this again. `start`, `frame` and `run` are what this
    // pane *is* — a change of any of them would be a different pane, and the face gives that one a
    // different key (`./WorkspaceFace`). The one exception is the run's task, told below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [running]);

  // **The row above a built-in's card** (`./BuiltinCard`). There is no terminal for the effect above
  // to put it up with, and the row is still what says which run this is and how far in it has got.
  // The card is a pane of its own key, one per step (`./WorkspaceFace`), so this is put up once and
  // taken down with it — the next built-in in a row is the next pane, on the same spot or not.
  //
  // **And above a run's pane with no terminal in it** (`AMB-T-5635`): one the store kept, come back
  // with the app for a run held or failed. What its terminal printed died with the process, and the
  // row is still what says which run this is and where it stopped. No terminal is opened here by
  // hand; the run's next step, where it is picked up again, is what brings one.
  const onBuiltin = builtin !== null && !script;
  const rowWithout = onBuiltin || (run !== null && !running);
  useEffect(() => {
    const label = labelRef.current;
    if (!rowWithout || !label) return;
    const plate = mountPlate(label, termName, hue, run, launch !== null);
    // A row is drawn only on a pane something has run in, and a built-in is that: Amenbo is running
    // it here — as a run's pane is, whatever ran in it last. There is no folder to head the row with,
    // and a run's row is headed with its automation.
    plate.opened(null);
    plateRef.current = plate;
    on.current.onRow?.(frame, plate.read);
    return () => {
      plate.stop();
      plateRef.current = null;
      on.current.onRow?.(frame, null);
    };
    // Once per pane: `frame`, `hue` and `run` are what the pane is, as said above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rowWithout]);

  // The keyboard, at the moment a terminal opens here and at every fold after it. What a person does
  // next in a place they just opened is type, and where that lands is whether the box is open
  // (`AMB-D-864`, `AMB-D-889`) — left to itself the keyboard is on the page, and the first thing
  // typed goes nowhere.
  //
  // **The fold is a second reason to answer this.** Opening the box is asking to write in it, and
  // shutting it takes away the thing the keyboard was in — a press after that has nowhere to go
  // unless the terminal is given it back here.
  //
  // **It is answered here rather than in `opened`**, which runs a render too early: the box is drawn
  // by the render that learns the session, so there is nothing to put the keyboard in until this one.
  //
  // **Only the pane being worked in.** Several panes open at once when the window comes back, and
  // each one taking the keyboard would leave it in whichever answered last. A pane the person opened
  // themselves is the one being worked in — opening a place moves the frame to it (`../talk/layout`)
  // — so the pane that should take it is the pane that has it.
  //
  // Those two are the whole of it. A pane that becomes the one being worked in later was pressed, and
  // the press has already said where the keyboard goes (`pressedOn`). A run's pane turned to its
  // roles keeps the keyboard off its terminal here too, for the reason said there.
  useEffect(() => {
    if (live === null || !focused || onRoles) return;
    if (folded) focusTerminal(paneRef.current);
    else boxRef.current?.focus({ preventScroll: true });
    // `focused` is read as it stands: the two reasons are the ones said above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [live, folded]);

  // A naming reaches every row, not only the one it happened in: the rail renames a pane that is not
  // the one being worked in, and the row above that pane is where the answer shows.
  useEffect(() => {
    plateRef.current?.named(names);
  }, [names]);

  // **The task a run's step took after it opened** (`AMB-T-5427`). The step is the same one and so is
  // its terminal, so the pane is not built again for it and the row is told instead. Keyed on what
  // the task is rather than on `run`, which is a value made afresh at every render of the face.
  const took = run?.task ?? null;
  useEffect(() => {
    plateRef.current?.took(took);
    // Watched by the task's fields, for the reason said above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [took?.ref, took?.title, took?.seq]);

  // **Where the run stands** (`AMB-T-5506`). It moves while a step's pane stands — the run ends, or is
  // paused, with its last step still up — so the row is told rather than the pane built again. Keyed
  // on the words, for `took`'s reason.
  const stood = run?.state ?? null;
  useEffect(() => {
    plateRef.current?.stated(stood);
    // Watched by the words, for `took`'s reason.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stood?.status, stood?.word]);

  // **The number of the box the step was opened from** (`AMB-T-5538`), read off the automation's
  // picture as it stands. It lands after the row is drawn — the picture is a read of its own — and a
  // rebuilt picture renumbers it, so the row is told rather than the pane built again.
  const box = useBoxNumber(run?.automationId ?? null, run?.placement ?? undefined) ?? null;
  useEffect(() => {
    plateRef.current?.numbered(box);
  }, [box]);

  // **When the step started**, which a built-in's card says only once it has: the card is written
  // over between its start and its end rather than built again (`./WorkspaceFace`).
  const startedAt = run?.startedAt ?? null;
  useEffect(() => {
    plateRef.current?.timed(startedAt);
  }, [startedAt]);

  // The name as it stands, ready to be typed over. A box opened on a pane already called something is
  // opened to change that name, and a reader who has to clear it first is being asked to type the old
  // one back in whenever they only meant to add a word.
  useEffect(() => {
    nameField.current?.select();
  }, [naming]);

  // Files dragged in from the desktop, which the host hands over as paths (`../core/hostDrop`).
  //
  // **The watch stands only while there is a terminal here to hand them to.** A slot with nothing
  // running in it has nowhere to put a path, and a surface that lit up over one would be a promise
  // the pane cannot keep. It is taken up per pane and matched on this pane's own frame, so a drop
  // that landed on the pane beside it is one this watch is never told about.
  //
  // **A drop is a person saying which pane they mean**, so both of the things that follow from
  // saying it are done here. Neither happens on its own: the selection moves on `onMouseDown` and
  // an outside drag never presses the page, and the focus is the browser's, which the same gesture
  // never reaches either. Without them the path is pasted into this pane and the reader's next
  // keystroke goes to the pane they were in before (`AMB-T-4182`).
  //
  // **Nor on a script's terminal** (`builtin`): nothing there reads what would be pasted.
  useEffect(() => {
    if (live === null || script) return;
    let alive = true;
    let stop: (() => void) | null = null;
    void watchHostDrop({
      select: `[data-hand="${frame}"]`,
      over: ({ el }) => { if (alive) setHanding(el !== null); },
      leave: () => { if (alive) setHanding(false); },
      drop: (_at, paths) => {
        if (!alive) return;
        setHanding(false);
        on.current.onFocus(frame);
        focusTerminal(paneRef.current);
        void handOver(live, paths);
      },
    }).then((off) => { if (alive) stop = off; else off(); });
    return () => {
      alive = false;
      stop?.();
    };
  }, [frame, live, script]);

  // Files and pictures pasted into the box, which reach it by two doors and go in as one thing.
  //
  // **What goes in is the path, quoted** — the same as a drop on the pane and for the same reason
  // (`AMB-D-832`). The box is not an editor: what is written in it is sent to the program in the
  // pane as the person's own line (`AMB-D-864`), so there is a shell behind it and a name with a
  // space in it would otherwise be two words.
  //
  // **A picture is written down before it can be named.** A screenshot on the clipboard is bytes
  // and no file, so the host puts it in this pane's own directory and answers with where it landed
  // (`AMB-D-854`). ⚠ It does not outlast the app — this is for handing a screenshot to something
  // running now, not for keeping one.
  //
  // **On Linux the picture comes in by the press rather than by the paste** — WebKitGTK hands a
  // paste nothing at all, so the clipboard is asked when `Ctrl+V` is pressed (`../core/clipFiles`).
  // `Ctrl+V` and not `Ctrl+Shift+V`: this is a box a person writes in, and there is no program here
  // to hand a control character to. On the other two machines that listener is never put on.
  //
  // **The fold is one of the reasons to do this again** (`AMB-D-889`): both doors are put on the box
  // itself, and a box that has just been opened is a different element from the one the listeners
  // were put on — left off the list, the first pane anybody folds never takes a paste again.
  useEffect(() => {
    const box = boxRef.current;
    if (box === null || live === null) return;
    // At the caret, taking the selection with it, which is what every other paste into a text box
    // does. What is written is read off the box rather than off `written`, so a listener that
    // outlives a keystroke still reads the sentence as it stands.
    // `put` names the paths of what was inserted, where what was inserted is paths: they are
    // remembered beside the body so that the send can wait the pane's agent out for the file it will
    // stop to read (`../talk/layout`, `AMB-D-879`).
    const insert = (arrived: string, put: readonly string[] = []) => {
      if (arrived === "") return;
      const from = box.selectionStart;
      const to = box.selectionEnd;
      on.current.onWrite(frame, box.value.slice(0, from) + arrived + box.value.slice(to), put);
      caret.current = from + arrived.length;
    };
    const writeImage = (bytes: Uint8Array, mime: string) => writesPastedImage(bytes, mime, live);
    const stopPaste = takesPastedFiles(
      box,
      (paths, words) => (paths.length > 0 ? insert(quotedPaths(paths), paths) : insert(words)),
      writeImage,
    );
    const stopPress = takesPastedImages(
      box,
      writeImage,
      (paths) => insert(quotedPaths(paths), paths),
      "textbox",
    );
    return () => {
      stopPaste();
      stopPress();
    };
  }, [frame, live, folded]);

  // And the caret put back, before the browser has drawn the box the sentence came back down into.
  useLayoutEffect(() => {
    const where = caret.current;
    if (where === null) return;
    caret.current = null;
    boxRef.current?.setSelectionRange(where, where);
  });

  return (
    <div
      className={`slot${focused ? " slot--focused" : ""}${landed ? " slot--landed" : ""}${
        held ? " slot--held" : ""}${run === null && !seatless ? "" : " slot--run"}${launch === null ? "" : " slot--launch"}`}
      style={at}
      data-hand={frame}
      onMouseDown={pressedOn}
    >
      {/* Everything this pane is, in one frame — the name row, the terminal, the box a line is
          written in, and the model row. It is the frame and not the terminal that says where the
          keyboard is (`../styles/global.css`): the box and the model row are as much what a press
          is answering to as the terminal is, and a mark drawn around the terminal alone left them
          outside it.

          The surface a drag is caught on and the menu are in here with them and belong to no row:
          both are placed against the slot rather than laid out in it, so the frame's flow never
          sees them and what they cover is still the whole slot. */}
      <div className="slot__frame">
        {/* What is said about this terminal, and the one control the place has. They share the row
            because the row is what is said about this pane, and removing it is the last thing there is
            to say. The control is drawn whether or not anything is running: a frame kept from the last
            run has no session and is still a place somebody has to be able to get rid of. */}
        <div
          className={`slot__bar${onGrab === undefined ? "" : " slot__bar--grab"}`}
          onPointerDown={(e) => {
            // The row is a handle and the things on it are not: a press that started on the menu or
            // on the box a name is typed in is that control's, and carrying the pane off under it
            // would take the press away from what it was aimed at.
            if (e.target instanceof Element && e.target.closest("button, input") !== null) return;
            on.current.onGrab?.(e);
          }}
        >
          {/* **A launch's header** (`./LaunchHead`), standing in the row: the launch and the moves made
              on the whole of it, beside the pane's own controls. It says nothing of the seat in front,
              and the seat's row is not drawn under it (`../talk/nameplate`'s `Plate.headed`): a seat's
              run is said on its own row in the roles (`AMB-D-1141`). */}
          {launch !== null && <LaunchHead launch={launch} project={project} />}
          {/* The line above the pane, which is empty until there is a session to say something about
              — and holds the row's width open either way, so the control does not walk across it.
              It stays up while a name is being typed in its place, out of sight rather than out of
              the page: what draws it was put there once and lives longer than any one naming
              (`../talk/plate`). */}
          <div className={`slot__plate${naming ? " slot__plate--behind" : ""}`} ref={labelRef} />
          {/* Where the name is typed, standing in the line's own place. Enter is the word taken and
              Escape is the row left as it was; leaving the box is the same as Escape, because a
              reader who has gone somewhere else has not said what to call this pane. */}
          {naming && (
            <input
              ref={nameField}
              className="slot__rename"
              defaultValue={names.get(termName) ?? ""}
              autoFocus
              aria-label={t("face.rename")}
              {...asTyped}
              onKeyDown={(e) => {
                if (isEnterSubmit(e)) {
                  e.preventDefault();
                  const text = e.currentTarget.value.trim();
                  // A person's word is the last one on a frame, and an empty box is not a word: it
                  // would otherwise take the name off a pane the agent had named (`../talk/frames`).
                  if (text) on.current.onName(termName, text, "person");
                  setNaming(false);
                }
                if (e.key === "Escape") setNaming(false);
              }}
              onBlur={() => setNaming(false)}
            />
          )}
          {/* What the row can do besides end the place. It is a menu rather than a row of buttons so
              that a face split four ways does not draw the same button four times over.

              **It is drawn only while a terminal is running**, and it is the one way in to naming a
              pane (`AMB-D-838`) — so an empty frame has no name. A place nobody has opened anything in
              is a place there is nothing to call. */}
          {/* The size, beside the menu rather than in it: it is about the place and not the terminal,
              so it is there whether or not anything is running — the way the corner is. */}
          {/* **The run's own moves, on the pane it is drawn in** (`AMB-T-5507`): the same two the
              "running" tab's row carries (`../screens/RunningTab`) — a run going is paused or
              force-cancelled, a paused one picked up again or cancelled (`AMB-D-1002`). A reader
              watching a run is in its pane, and had to go to the tab to act on what they were
              watching. They are drawn while the run is going or paused, and gone once it is over — the
              row says how it ended. **They are marks and not words** (`AMB-T-5529`), standing straight
              after the state they act on, so the row's words are left to the automation's name; each
              is named for a reader who cannot see it. Cancelling keeps the pane.

              **Not on a launch's pane**: its moves are the launch's, on the header above
              (`./LaunchHead`), and a seat's own are on the "running" tab's row. */}
          {runLive && run !== null && launch === null && (
            <span className="slot__runacts">
              <RunActs
                run={{
                  run: run.run,
                  status: run.state?.status ?? "running",
                  pauseRequested: run.state?.pauseRequested ?? false,
                  pauseBeforeNextTask: run.state?.pauseBeforeNextTask ?? false,
                  pausableBeforeNextTask: run.state?.pausableBeforeNextTask ?? false,
                }}
                busy={pressing}
                onPress={(move) => void press(move)}
              />
            </span>
          )}
          {/* **Where a finished run is read from now** (`AMB-T-5539`), on the ledger. The run's moves
              are gone once it is over, and this stands where they stood. That is the history tab — but
              a failure nobody has acknowledged is still on "running" (`AMB-D-955`), and sent to history
              the reader would not find it there (`AMB-T-5672`). */}
          {/* Not on a launch's pane: its header says where its history is read, whichever seat is in front. */}
          {run !== null && over && launch === null && ledger.openRuns !== undefined && (
            <button
              type="button"
              className="slot__runlink"
              title={t(runsTab === "running" ? "auto.run.seeRunning" : "auto.run.seeHistory")}
              onClick={() => ledger.openRuns?.(project, runsTab)}
            >
              {/* The word, and a mark that stands in for it where the pane is narrow: the word is
                  wider than the moves it replaces, and a narrow pane gave up the whole of the
                  run's name to it. */}
              <Icon name="clock" />
              <span className="slot__runlink-word">
                {t(runsTab === "running" ? "auto.run.seeRunning" : "auto.run.seeHistory")}
              </span>
            </button>
          )}
          {size !== undefined && onSize !== undefined && <PaneSize size={size} onSize={(to) => on.current.onSize?.(to)} />}
          {live !== null && !script && (
            <button
              className="slot__more"
              title={t("face.more")}
              aria-label={t("face.more")}
              aria-haspopup="menu"
              onClick={(e) => setMenuAt({ x: e.clientX, y: e.clientY })}
            >
              <Icon name="more" />
            </button>
          )}
          <button
            className="slot__end"
            title={t(stuck ? "face.dropRunLive" : stopsLaunch ? "face.dropLaunch" : "face.drop")}
            aria-label={t(stuck ? "face.dropRunLive" : stopsLaunch ? "face.dropLaunch" : "face.drop")}
            disabled={stuck}
            onClick={() => { void drop(); }}
          >
            <Icon name="close" />
          </button>
        </div>
        {/* **What needs a person's hand in a launch** (`./LaunchHead`), straight under its header. */}
        {launch !== null && <LaunchAttention launch={launch} onOpenSeat={openSeat} />}
        {/* **What a failed run is waiting for** (`AMB-T-5529`), on a band of its own under the row. The
            row already says which step, so the band says only why — by the way out it stopped at where
            it stopped at one calling for a person, and in a short phrase otherwise — and holds the
            press that answers it. "Acknowledge" is the "running" tab's, in the pane the reader is
            already watching; once pressed the band stays to say why, with nothing left to press.

            **Not on a launch's pane** (`AMB-D-1141`): no one run stands for the whole of it, and the run
            in front is only whose terminal the pane turns to — a band off it spoke for one failure of
            the launch's and stayed after it was seen. A task a failure stopped is under the header
            (`LaunchAttention`); the failure itself is read on the "running" tab's row and in the
            history. */}
        {launch === null && run?.state?.status === "failed" && (
          <div className="slot__band slot__band--fail" role="status">
            {/* The mark rides inside the reason, so a narrow pane that wraps the band never leaves it
                on a line of its own (`AMB-T-5965`). */}
            <span className="slot__band-why">
              <Icon name="warning" />
              {run.state.exit !== null
                ? stoppedAt(run.state.exit, run.state.errorExit)
                : run.state.why ?? run.state.word}
            </span>
            {/* The picture it stopped on, with the box it stopped at pressed (`AMB-T-5539`) — on the
                ledger, which is where a definition is read and changed. */}
            {ledger.openAutomation !== undefined && (
              <button
                type="button"
                className="slot__bandact slot__bandact--quiet"
                onClick={() => ledger.openAutomation?.(project, run.automationId, run.placement)}
              >
                {t("auto.run.seePicture")}
              </button>
            )}
            {!run.state.acknowledged && (
              <button
                type="button"
                className="slot__bandact"
                disabled={pressing}
                onClick={() => void press(() => acknowledgeRun(run.run))}
              >
                {t("auto.run.acknowledge")}
              </button>
            )}
            {/* Once it is pressed, who pressed it — a person's AI may say it for them (`AMB-D-989`). */}
            {run.state.acknowledgedBy !== undefined && (
              <span className="slot__band-seen">
                {t("auto.run.seenBy")}
                <FacetAvatar actor={facetActor(run.state.acknowledgedBy)} showName />
              </span>
            )}
          </div>
        )}
        {menuAt !== null && live !== null && (
          <Menu at={menuAt} onClose={() => setMenuAt(null)}>
            {/* The other way in, for a reader whose file is not somewhere they can drag it from. It
                ends where the drop ends: the path the thing is at is put in front of the agent.
                It is two items because the machine's picker takes `directory` as a yes or a no —
                one window cannot offer both — so the choice is made before the window opens
                (`../core/dialog`). */}
            <MenuItem
              onClick={() => {
                setMenuAt(null);
                void pickFiles().then((paths) => handOver(live, paths));
              }}
            >
              <Icon name="document" />
              {t("files.pasteFilePath")}
            </MenuItem>
            <MenuItem
              onClick={() => {
                setMenuAt(null);
                void pickFolders().then((paths) => handOver(live, paths));
              }}
            >
              <Icon name="folder" />
              {t("files.pasteFolderPath")}
            </MenuItem>
            {/* Held off from the two above because it is not their kind: they hand the terminal
                something, and this is about the place the terminal is drawn in. The row is where a
                person names a pane, because the row is the one thing on the face that belongs to the
                frame rather than to the session in it (`AMB-D-838`). */}
            {/* Not on a run's pane: its row is headed with the automation's name and never the place's
                (`../talk/plate`), so a name given here would be kept and never drawn. */}
            {run === null && !seatless && (
              <MenuItem
                apart
                onClick={() => {
                  setMenuAt(null);
                  setNaming(true);
                }}
              >
                <Icon name="pencil" />
                {t("face.rename")}
              </MenuItem>
            )}
          </Menu>
        )}
        {/* The receiving surface, drawn over the pane while something hangs on it and never otherwise —
            a file from the desktop, which this pane hears about itself, or a row of the panel, which
            the face is carrying and says so (`../files/handDrag`). It takes no pointer events: what is
            under the drag has to stay the pane, or the point being resolved would land on the surface
            itself and the highlight would flicker itself away. */}
        {(handing || offered) && <div className="slot__handing">{t("face.handHere")}</div>}
        {/* Where a carried pane would land, drawn down the edge it would go in at: the midline that
            settles it runs the way this pane has neighbours, so a pane taking the whole width says
            it above and below rather than left and right (`./paneDrag`). */}
        {goes !== null && (
          <div className="slot__goes" data-side={goes.side} data-axis={goes.axis} />
        )}
        {/* The corner, which is the other half of what a person can do to a pane's place. It is a
            corner and not an edge because what is being pulled is a size and not a width: both
            numbers come off the one drag, and an edge would only ever say one of them
            (`./paneDrag`). */}
        {onStretch !== undefined && (
          <div
            className="slot__corner"
            role="separator"
            title={t("face.paneSize")}
            aria-label={t("face.paneSize")}
            onPointerDown={(e) => on.current.onStretch?.(e)}
          />
        )}
        {/* Everything the terminal face is, kept mounted and in the layout while the roles are up:
            the terminal is a program writing into the rows it was told it has, and taking its element
            away would be ending what draws it. The roles are drawn over it, so it keeps its size when
            the face is turned (`../talk/terminal`). */}
        <div className="slot__body">
        {/* **The bar over a run's terminal** (`./RunPicture`): the way back to the roles — the only
            way, the row above has no other — whose round this is and, where a step turned the pane, why.
            It stands in the layout under the roles as the terminal does, so turning the face does not
            refit the terminal. */}
        {run !== null && (
          <RunTermBar
            run={run}
            project={project}
            launch={launch}
            byStep={byStep && face === "terminal"}
            onBack={() => turn("roles")}
          />
        )}
        {onRoles && (
          <RunRoles
            run={run}
            project={project}
            launch={launch}
            onOpen={openSeat}
            lastOutput={() => plateRef.current?.lastOutput() ?? null}
            expected={expected}
          />
        )}
        {/* **A run over, with no terminal up, says how it ended** (`./RunBody`) — in place of the card
            of a built-in it was stopped on too, which would otherwise go on saying it was at work. */}
        {/* **A step's tail, kept on the run** (`tail`), in place of either card below: it is what the
            step left to be read, and the row and the band already say how the run stands. */}
        {/* **A failed round's terminal says how it ended** (`./RunPicture`), as the last lines under the
            tail kept from it: its program ending, and where the failure was filed. */}
        {run !== null && !running && tail !== null ? (
          <SavedTail tail={tail} after={fell ? fellLines : []} />
        ) : run !== null && over && !running ? (
          <RunOverCard
            run={run}
            see={t(runsTab === "running" ? "auto.run.seeRunning" : "auto.run.seeHistory")}
            onSee={ledger.openRuns === undefined ? undefined : () => ledger.openRuns?.(project, runsTab)}
          />
        ) : builtin !== null && !script ? <BuiltinCard builtin={builtin} run={run?.run ?? null} /> : running
          ? (
            <>
              {/* Not on a run's pane: a step's program ending is the run moving on, and the row
                  already says where it stands. */}
              {ended && run === null && (
                <span className="workspace__note">
                  {t("face.ended")}
                  {stopped !== null && ` ${t(stopped)}`}
                </span>
              )}
              {/* The terminal a failed round fell in, its program ended: the screen kept from it in
                  its place, which still has what the program wiped as it ended, and how it ended
                  under that. The terminal stays mounted out of sight for its pane to take down. */}
              {ended && fell && <SavedTail tail={tail ?? ""} after={fellLines} />}
              <div className="workspace__face" ref={paneRef} hidden={ended && fell} />
            </>
          )
          // Not on a run's pane (`AMB-T-5667`): a terminal opened there would be an ordinary session
          // that has nothing to do with the run, under a row that goes on naming the run's step. Nor on
          // a launch's with no seat yet, for the same reason: its roles are drawn over this place.
          : run === null ? !seatless && (
            <button className="slot__open" onClick={() => setRunning(true)}>
              {t("face.open")}
            </button>
          )
          // A run held with no step up — come back with the app paused (`AMB-T-5635`).
          : run.state?.status === "paused" && <RunPausedCard />}
        {/* Where a person writes a line for whatever is running in this pane (`AMB-D-864`). It is the
            app's own box rather than the program's, which is what buys undo, redo and select-all in a
            pane whatever CLI is in it — the keys for those differ per CLI and one of them has no undo
            at all (`AMB-T-4575`), and a textarea has all three from the browser.

            **It stands under the terminal in the same column, and pushes it up.** Nothing is drawn
            over the pane: a terminal is told how many rows it has and goes on writing into every one
            of them, so a box covering the bottom would cover the line a program was asking a question
            on (`AMB-D-864`).

            **It is drawn while a terminal is running and does not move for the keyboard.** A box that
            appeared when it was written in would change the pane's height at the moment a person
            started typing, and every change of height wakes the program inside to repaint
            (`../talk/terminal`).

            **It is there while the reader has it open, and folded away otherwise** (`AMB-D-889`).
            What is written in it survives the fold — it belongs to the window rather than to this
            box (`../talk/layout`) — so a sentence left half-written comes back when it is opened
            again, and the press that folds it says so meanwhile. */}
        {live !== null && !folded && !script && (
          <div className="compose">
            <textarea
              ref={boxRef}
              className="compose__box"
              rows={1}
              value={written}
              placeholder={t("face.compose")}
              aria-label={t("face.compose")}
              {...asTyped}
              onChange={(e) => on.current.onWrite(frame, e.currentTarget.value)}
              onKeyDown={pressed}
            />
            <button
              className="compose__send"
              type="button"
              disabled={written === ""}
              title={sendLabel}
              aria-label={sendLabel}
              onClick={() => { void send(); }}
            >
              <Icon name="arrowUp" />
            </button>
          </div>
        )}
        {/* The pane's own band, under the box rather than over the terminal for the reason the box
            itself is: a terminal writes into every row it was told it has.

            **It stands under every running pane and keeps its height whatever is in it.** The press
            that folds the box is here, and a band that shrank when there was no model to name beside
            it would move that press from pane to pane (`AMB-D-889`). The model is the other thing on
            it and is drawn only where the host has a road (`./PaneModel`, `AMB-D-865`). */}
        {live !== null && !script && (
          <div className="panerow">
            {/* Open or shut, and nothing else (`AMB-D-889`). Which of the two it is, is said by
                `aria-expanded` and by the box standing there or not — so the mark is the same
                drawing in both states and names the press rather than the state. A mark that
                changed with the state was a second thing the reader had to read to learn what they
                were already looking at.

                **The press is the button and the mark is inside it.** A hit area on the drawing
                itself is a hit area that is swapped out under the pointer the moment it is used, and
                the click that was half-way through lands on nothing.

                **It says when it is holding something nobody can see.** A pane folded over a
                half-written sentence looks exactly like one folded over an empty box, and the
                sentence is still there and still unsent (`../talk/layout`).

                **Each press is also what the next pane will start as** (`../core/composeStartsOpen`),
                which is why there is no row for this in the settings screen: the switch is here, in
                front of whoever it is in effect for, and the way back is the same press. */}
            <button
              className={`panerow__fold${written === "" ? "" : " panerow__fold--holding"}`}
              type="button"
              aria-expanded={!folded}
              title={t(folded ? "face.composeOpen" : "face.composeFold")}
              aria-label={t(folded ? "face.composeOpen" : "face.composeFold")}
              onClick={() => {
                on.current.onFold(frame, folded);
                setComposeStartsOpen(folded);
              }}
            >
              <Icon name="keyboard" />
            </button>
            <PaneMade made={made} />
            <PaneModel frame={termName} session={live} agent={inPane} />
          </div>
        )}
        </div>
      </div>
    </div>
  );
}

/** What {@link TerminalPane} is handed. */
type PaneProps = {
  /** Which of the arrangement's places this is (`../talk/layout`). */
  frame: string;
  /** The name the terminal in it goes by on the host's side, where it is not `frame` — the seat in
   *  front on a launch's pane (`../talk/layout`'s `terminalOf`). Its name and its model are kept
   *  against this, and `onName` is told it. */
  terminal?: string;
  /** The launch this pane draws (`../talk/layout`'s `Frame.launch`), whose seats the roles list and
   *  whose header is drawn over the seat in front (`./LaunchHead`); null on an ordinary pane and on a
   *  run kept from before launches were. */
  launch?: number | null;
  /** Where on the page's grid this pane sits. It is worked out afresh from the order and this pane's
   *  size, and never held against the pane itself (`AMB-D-939`). Left out where this is drawn
   *  outside a page, which is a pane with no place to be given. */
  at?: CSSProperties;
  /** The hue the lamp above this pane is drawn in. It follows the slot rather than the frame, so it
   *  comes from the page that laid this one out (`../talk/moving`). */
  hue: number;
  /** Which project this place belongs to — whose answer the agent it opens with is kept against
   *  (`../talk/agent`). */
  project: number;
  /** What every frame is called, so a naming from anywhere reaches this row. */
  names: FrameNames;
  /** Which terminal to draw here, and where to start one. */
  start: PaneStart;
  /**
   * Where the automation run this pane is drawing has got to, or null on an ordinary pane
   * (`../talk/nameplate`).
   *
   * It is what the row says under the name, and it is also what the way out means here: **closing a
   * run's pane is stopping the run** (`drop`).
   */
  run?: Say | null;
  /**
   * **How long each run's step is expected to take**, in minutes, by the name its terminal goes by
   * (`runFrameId`) — what a quiet terminal of each seat on a launch's pane is measured against
   * (`./RunPicture`). The face hands a new one only when a step arrives.
   */
  expected?: ReadonlyMap<string, number>;
  /**
   * The built-in Amenbo is carrying out on this run's pane, or null (`./BuiltinCard`). While there is
   * one, it stands where the terminal would, and nothing is started here.
   *
   * **A script step comes this way too, and is drawn as a terminal** (`AMB-D-1016`): its program runs
   * on one, which the pane takes up the way it takes up an agent's step's (`PaneStart.readOnly`). Nobody
   * writes into it, so the box, the band under it and the ways to hand it a path are not drawn.
   */
  builtin?: BuiltinRun | null;
  /**
   * **What the run's last step left to be read** — what a script printed (`AMB-D-1016`), or the screen
   * an agent's step had as it failed the run — kept on the run, and drawn where its terminal would be
   * on a pane come back with the app or opened again: that terminal is gone. On a failed run it is also
   * drawn in place of the terminal it fell in once that terminal's program has ended. Null everywhere
   * else.
   */
  tail?: string | null;
  /** What the program of the step that left {@link PaneProps.tail} exited with, where that was kept
   *  beside it. Null everywhere else. */
  tailCode?: number | null;
  /** True for the slot that puts a terminal up without being asked — the one the face comes up with,
   *  and the one a person has just pressed the way in on. */
  autoStart: boolean;
  focused: boolean;
  /**
   * Whether a path has just been handed to this pane from somewhere else on the face, for as long as
   * the pane is saying so (`./WorkspaceFace`).
   *
   * **Told rather than kept here**, because what it is about is the act and not the pane: the face
   * knows a hand-over happened and the pane only draws it. What is drawn takes no room — the box
   * xterm measures is what the program inside is told its number of columns from, so a mark that
   * changed that box would be resizing somebody's editor to say a file had arrived.
   */
  landed?: boolean;
  /**
   * Whether something being carried inside the window is hanging over this pane
   * (`../files/handDrag`).
   *
   * **Told rather than watched for**, which is the difference between the two drags. A drop from the
   * desktop is the host's news and reaches each pane on its own; a row carried across the page is
   * one gesture the face is holding, and the pane it is over is the face's answer. Both light the
   * same surface, because to a reader they are the same act.
   */
  offered?: boolean;
  /** Whether this is the pane being carried, so it can stand back while it is out of its place
   *  (`./WorkspaceFace`). */
  held?: boolean;
  /**
   * Where a pane let go of over this one would land — before it in the order, after it, or nowhere.
   *
   * It is the one thing drawn while a pane is carried, because the panes themselves do not move
   * until the press ends: a page that rearranged under the hand would be moving terminals somebody
   * is reading, to show them an arrangement they have not asked for yet (`AMB-D-939`).
   */
  goes?: { readonly side: "before" | "after"; readonly axis: "across" | "down" } | null;
  /** Take hold of this pane by its row, to carry it somewhere else in the order. Left out where
   *  there is nothing to reorder — one pane is already in order (`./WorkspaceFace`). */
  onGrab?: (e: ReactPointerEvent<HTMLElement>) => void;
  /** Pull this pane's corner, to leave it at one of the sizes there are (`./paneDrag`). */
  onStretch?: (e: ReactPointerEvent<HTMLElement>) => void;
  /** The size this pane takes now, which the row's mark draws (`./PaneSize`). */
  size?: Size;
  /** Put this pane at one of the sizes, picked from the row — the way to every size the corner
   *  cannot pull to (`./PaneSize`). Left out where the pane has no page to be sized on. */
  onSize?: (to: Size) => void;
  onOpened: (frame: string, session: string, folder: string | null, agent: string | null) => void;
  onSaid: (statement: SessionSaidDto) => void;
  /** A file path drawn in this pane was clicked, as it was drawn. */
  onPath: (frame: string, target: string) => void;
  onClosed: (session: string) => void;
  /** Take this place away — the frame and not the program in it (`../talk/layout`). */
  onDrop: (frame: string) => void;
  /** `carried` says the name came over with the session rather than as it was said — the pane took
   *  up a terminal that had already named itself. It is written down the same way, and the provider
   *  is not told: it is the one that said it (`../talk/terminal`). `frame` is the name the terminal
   *  goes by (`terminal`), not always the place's id. */
  onName: (frame: string, name: string, by: NamedBy, carried?: boolean) => void;
  onFocus: (frame: string) => void;
  /** A way to read this pane's row, handed over while the pane is drawn and taken back when it is
   *  not. It is what lets a face draw this pane somewhere other than above it (`./PaneOrder`), and it
   *  is a way to ask rather than the answer: the row changes with every chunk that crosses, and a
   *  value pushed up on each of them would redraw the face for a mark that has not moved. */
  onRow?: (frame: string, read: (() => Row | null) | null) => void;
  /**
   * What has been written in the box under this pane and not sent yet (`AMB-D-864`).
   *
   * **Held by the window rather than here** (`../talk/layout`). A pane is taken down whenever it
   * stops being on the face — carried onto another page, the count changed, the tasks face came up —
   * and a half-written sentence kept in the drawing would go down with it. The terminal is the same
   * shape of thing from the other side: what is running belongs to the host, and this pane only draws
   * it.
   */
  written: string;
  /** What is in the box now, on its way to the window that holds it. Said as it is written and again
   *  with nothing in it once a line has gone. */
  onWrite: (frame: string, written: string, put?: readonly string[]) => void;
  /** The paths Amenbo put into this box, still standing in what is written (`../talk/layout`). They
   *  are what the send waits the pane's agent out for (`../talk/terminal`, `AMB-D-879`). */
  inserted?: readonly string[];
  /**
   * Whether the box under this pane is open (`AMB-D-890`).
   *
   * **Held by the window for the reason the draft is** (`../talk/layout`): a pane is taken down
   * whenever it stops being on the screen, and a reader who opened the box did not ask for it to
   * shut at any of those moments. It outlives the run as well, which the draft does not.
   */
  composeOpen: boolean;
  /** The press on this pane's band, said as the box opens or folds away. What the *next* pane will
   *  start as is a separate answer and is written down here (`../core/composeStartsOpen`). */
  onFold: (frame: string, open: boolean) => void;
  /** Go to the run `run` — a seat pressed among the roles of a launch's pane: that seat put in front
   *  in this same place. Left out where this is drawn outside the workspace. */
  onGoToRun?: (run: number) => void;
};

/** The functions among {@link PaneProps}: what the face wants done with what happens in the pane. */
type Handlers = Pick<
  PaneProps,
  | "onOpened" | "onSaid" | "onPath" | "onClosed" | "onDrop" | "onName" | "onFocus" | "onRow" | "onWrite" | "onFold"
  | "onGrab" | "onStretch" | "onSize" | "onGoToRun"
>;

/** {@link Pane}, drawn again only when something it draws has changed ({@link samePane}). */
const DrawnPane = memo(Pane, samePane);

/**
 * A pane on the face, drawn again only when something it draws has changed.
 *
 * **What is written in one pane's box is not a reason to draw the others.** The face draws itself
 * again on every key pressed in a box — what is written is the window's (`../talk/layout`) — and
 * every pane on the page went with it, which is what made typing heavy with several panes up.
 *
 * **The handlers go round the drawing, not through it.** The face hands a new function on each of
 * its renders. This holds the newest ones where {@link Pane} reads them at the moment they are used,
 * and the pane is not drawn again for being handed them: a pane left alone by {@link samePane} would
 * otherwise keep calling the ones from the last render it was drawn on. The pane is put up once and
 * lives longer than any one render, so its effects must not be re-run to see a newer callback either
 * — that would take the terminal down to learn something it could have been told.
 */
export function TerminalPane(props: PaneProps) {
  const {
    onOpened, onSaid, onPath, onClosed, onDrop, onName, onFocus, onRow, onWrite, onFold, onGrab, onStretch, onSize,
    onGoToRun,
  } = props;
  const on = useRef<Handlers>({
    onOpened, onSaid, onPath, onClosed, onDrop, onName, onFocus, onRow, onWrite, onFold, onGrab, onStretch, onSize,
    onGoToRun,
  });
  on.current = {
    onOpened, onSaid, onPath, onClosed, onDrop, onName, onFocus, onRow, onWrite, onFold, onGrab, onStretch, onSize,
    onGoToRun,
  };
  return <DrawnPane {...props} on={on} />;
}

/**
 * Whether two sets of props draw the same pane.
 *
 * **A function is compared by whether there is one, not by which.** The face makes its handlers
 * afresh on each render, and the pane reads them at the moment they are used rather than off the
 * props ({@link TerminalPane}), so a newer one changes nothing on the screen. Whether there is one
 * does: the row is a handle only where there is something to carry it past.
 *
 * **Everything else is compared by what it holds.** `start`, `at`, `goes` and `run` are made afresh
 * on each of the face's renders too, with the same values in them. What is not a plain object or an
 * array — `names` is a map — is compared as it is: the face hands a new one only when it has changed.
 */
export function samePane(was: PaneProps, now: PaneProps): boolean {
  const keys = new Set([...Object.keys(was), ...Object.keys(now)]) as Set<keyof PaneProps>;
  for (const key of keys) {
    const a = was[key];
    const b = now[key];
    if (typeof a === "function" || typeof b === "function") {
      if ((a === undefined) !== (b === undefined)) return false;
    } else if (!sameValue(a, b)) {
      return false;
    }
  }
  return true;
}

/** Two values holding the same thing, looked into where they are plain objects or arrays. */
function sameValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((one, i) => sameValue(one, b[i]));
  }
  if (Object.getPrototypeOf(a) !== Object.prototype || Object.getPrototypeOf(b) !== Object.prototype) return false;
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  return ka.every((k) => Object.prototype.hasOwnProperty.call(y, k) && sameValue(x[k], y[k]));
}

/** A step's tail kept on the run, drawn read-only on a terminal of its own that runs nothing (`../talk/terminal`). */
function SavedTail({ tail, after = [] }: { tail: string; after?: readonly string[] }) {
  const ref = useRef<HTMLDivElement>(null);
  // Drawn again only when what it says changes: the lines are worked out afresh on every update of
  // the run, and an emulator mounted again on each would flicker.
  const said = after.join("\n");
  useEffect(() => {
    const host = ref.current;
    if (host === null) return;
    return mountTail(host, tail, said === "" ? [] : said.split("\n"));
  }, [tail, said]);
  return (
    <div className="workspace__face">
      <div className="workspace__pane" ref={ref} />
    </div>
  );
}

/**
 * "Stopped at" a way out, with the way out drawn as the picture's chip for it. The sentence is the
 * language's to order (`face.runStoppedAt`), so the chip is found in it by a mark rather than by where
 * it falls in one language.
 */
function stoppedAt(exit: string, error: boolean) {
  const [before, after] = tf("face.runStoppedAt", { exit: EXIT }).split(EXIT);
  return (
    <>
      {before}
      <span className={`slot__exit${error ? " slot__exit--error" : ""}`}>{exit}</span>
      {after}
    </>
  );
}

/** Where the way out goes in {@link stoppedAt}'s sentence, until it is drawn in. */
const EXIT = "\u0000";
