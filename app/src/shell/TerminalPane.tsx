import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { mountAgentFrame } from "../talk/agent";
import {
  boxHeight,
  endTerminal,
  focusTerminal,
  leavesForTerminal,
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
import type { Plate as Row } from "../talk/nameplate";
import { confirmDialog, pickFiles, pickFolders } from "../core/dialog";
import { watchHostDrop } from "../core/hostDrop";
import { takesPastedFiles, takesPastedImages, writesPastedImage } from "../core/clipFiles";
import { pushNotice } from "../core/notice";
import { Menu, MenuItem } from "../components/Menu";
import type { FrameNames, NamedBy } from "../talk/frames";
import type { PaneStart } from "../talk/terminal";
import type { SessionSaidDto } from "../bindings/bindings";
import { currentLang, errText, t, tf } from "../core/i18n";
import { asTyped, isComposing, isEnterSubmit } from "../core/keys";
import { hostOs } from "../core/platform";
import { Icon } from "../components/Icon";
import { PaneModel } from "./PaneModel";

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
 * One slot of the terminal face: a frame, and the terminal in it when there is one.
 *
 * **A frame is a place, so an empty one is not nothing.** It is a slot on this page with a way to
 * open a terminal in it, and it stays a place after the program in it exits — what is on the screen
 * is what a terminal ends with, and taking the pane away would be the app deciding the reader had
 * finished reading it. So this puts the frame up once and then keeps it, whatever happens to the
 * process: what runs in it, what is offered when nothing can be started, and the row a closed frame
 * carries are all the frame's (`../talk/agent`).
 *
 * The pane comes down when the slot stops being on the screen — the page turned, or fewer panes were
 * asked for — and **the terminal does not**: a pane is a drawing of a session, and detaching leaves
 * the session running for whichever slot draws it next (`../talk/terminal`). That is why the slot's
 * session id is handed back up: the frame is what remembers, and this is only what draws.
 *
 * **The one control on the row removes the place**, which is the only thing on this face that does
 * (`../talk/layout`). It is not the same act as a program ending: what a terminal exits with stays on
 * the screen to be read, and a page that closed up under a reader because a shell finished would be
 * the app deciding they were done with it. So this is asked before it happens, and it ends whatever is
 * running on the way out — a session whose place has gone is one nobody can reach.
 */
export function TerminalPane({
  frame, project, names, start, autoStart, focused, landed = false, offered = false, written,
  inserted = [],
  onOpened, onSaid, onPath, onClosed, onDrop, onName, onFocus, onRow, onWrite,
}: {
  /** Which of the arrangement's places this is (`../talk/layout`). */
  frame: string;
  /** Which project this place belongs to — whose answer the agent it opens with is kept against
   *  (`../talk/agent`). */
  project: number;
  /** What every frame is called, so a naming from anywhere reaches this row. */
  names: FrameNames;
  /** Which terminal to draw here, and where to start one. */
  start: PaneStart;
  /** True for the slot that puts a terminal up without being asked — the one the face comes up with,
   *  and the one a person has just pressed the way in on. */
  autoStart: boolean;
  focused: boolean;
  /**
   * Whether a path has just been handed to this pane from somewhere else on the face, for as long as
   * the pane is saying so (`./TerminalFace`).
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
  onOpened: (frame: string, session: string, folder: string | null, agent: string | null) => void;
  onSaid: (statement: SessionSaidDto) => void;
  /** A file path drawn in this pane was clicked, as it was drawn. */
  onPath: (frame: string, target: string) => void;
  onClosed: (session: string) => void;
  /** Take this place away — the frame and not the program in it (`../talk/layout`). */
  onDrop: (frame: string) => void;
  onName: (frame: string, name: string, by: NamedBy) => void;
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
   * stops being on the screen — the page turned, the count changed, the tasks face came up — and a
   * half-written sentence kept in the drawing would go down with it. The terminal is the same shape
   * of thing from the other side: what is running belongs to the host, and this pane only draws it.
   */
  written: string;
  /** What is in the box now, on its way to the window that holds it. Said as it is written and again
   *  with nothing in it once a line has gone. */
  onWrite: (frame: string, written: string, put?: readonly string[]) => void;
  /** The paths Amenbo put into this box, still standing in what is written (`../talk/layout`). They
   *  are what the send waits the pane's agent out for (`../talk/terminal`, `AMB-D-879`). */
  inserted?: readonly string[];
}) {
  const paneRef = useRef<HTMLDivElement>(null);
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
  // Whether a drag from outside is over this pane. It is the whole of the receiving surface: nothing
  // is drawn until something is being carried, and what is carried is only known while it hangs there.
  const [handing, setHanding] = useState(false);
  // Whether the row is being named, and the box it is named in while it is. A frame's name is the
  // person's last word on it (`../talk/frames`), so it is typed on the row it belongs to rather than
  // in a window over the pane: what is being named is the line the box stands in.
  const [naming, setNaming] = useState(false);
  const nameField = useRef<HTMLInputElement>(null);
  // Whether the box is the one holding the keyboard, which is the whole of what the mark beside the
  // box says: a press is the box's while the box is the thing being typed at, and the terminal's
  // otherwise. The way out leaves a written box with the keyboard on the terminal
  // (`../talk/terminal`), and so does a person clicking the terminal, and the mark follows both
  // rather than go on naming the box.
  const [typing, setTyping] = useState(false);
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

  // What the face wants done with what happens here, read at the moment it happens. The pane is put up
  // once and lives longer than any one render, so the effect below must not be re-run to see a newer
  // callback — that would take the terminal down to learn something it could have been told.
  const on = useRef({ onOpened, onSaid, onPath, onClosed, onName, onFocus, onRow, onWrite });
  on.current = { onOpened, onSaid, onPath, onClosed, onName, onFocus, onRow, onWrite };

  /** Take the place away, once the person has said so. The terminal in it is ended first: a session
   *  whose pane has gone is one nobody can get back to.
   *
   *  **The one question is the plain one, whatever the session was doing** (`AMB-D-858`). What tied a
   *  pane to a task went through a key the world could rewrite behind the pane, so a question naming
   *  what was about to be lost named as often work somebody had already finished elsewhere.
   *  `face.dropConfirm` says what the loss is: the place, and with it the handle the talk in it is
   *  resumed from — so this press is what closes the way back into that conversation for good
   *  (`AMB-D-869`). It is the heavier of the app's two questions now, the way out of the app being
   *  the lighter one. */
  const drop = async () => {
    if (!await confirmDialog(t("face.dropConfirm"))) return;
    if (live !== null) await endTerminal(live).catch(() => {});
    onDrop(frame);
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
    onWrite(frame, "");
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
   * instead and the person keeps them without leaving the box.
   *
   * **The one way out of a written box is the ArrowUp on its first line.** It moves the keyboard to
   * the terminal and goes there itself, so a menu the program is drawing is walked by the one press
   * rather than by a press to leave and a press to move. What is written stays where it is, and the
   * mark beside the box goes on saying the way back.
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
   *  **A press that moves the focus puts the keyboard in the box** (`AMB-D-864`). The frame moving
   *  and the keyboard moving are one thing to the person doing it: they pressed the pane they mean
   *  to work in, and the next thing they do is write. Left to itself only the frame moved, and the
   *  characters went on landing in the pane they came from — the same disagreement a drop had
   *  before `AMB-T-4182` settled it there.
   *
   *  **Only the press that moves it.** A press inside the pane already being worked in is left
   *  alone, so pressing the terminal is still how the keyboard is handed to the program running in
   *  it. What decides is what the pane was before the press, which is what `focused` still says
   *  here: `onFocus` is what changes it, and it is answered on the render after this one.
   *
   *  **A pane with nothing running in it has no box** to put the keyboard in, so nothing is moved
   *  and it stays where the person left it.
   *
   *  It comes after the emulator has had the press — that one takes the keyboard on its own
   *  mousedown, from an element inside this one — so this is the last word rather than the first.
   */
  const pressedOn = () => {
    const moving = !focused;
    onFocus(frame);
    if (moving) boxRef.current?.focus();
  };

  /** Whether a press now would stay in the box — which is what the mark beside it names. It is the
   *  keyboard and nothing else: an empty box holding it keeps every ordinary character, and hands on
   *  only the four presses that walk a history, complete a word or leave a menu, with `Ctrl+C`
   *  (`../talk/terminal`). Asking what is written as well named the terminal while a person was
   *  typing the first character of a line into the box.
   *
   *  **`Escape` and `Ctrl+C` are outside what it names** (`AMB-D-876`): they go to the program from
   *  a written box too, and the mark goes on saying the box. What it is about is where the *typing*
   *  lands, and neither of those two is typing. */
  const keysHere = typing;

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
    const plate = mountPlate(label, frame);
    plateRef.current = plate;
    // The row is readable from outside for as long as this pane is drawn, and no longer: a pane on
    // another page is not being measured at all, so a reading kept past this point would be the last
    // one this pane took rather than what is true now.
    on.current.onRow?.(frame, plate.read);
    void mountAgentFrame(host, currentLang(), {
      opened: (session, where, running) => {
        // The folder is what the row above the pane calls it until something names the frame
        // (`../talk/frames`), and it is the one the terminal actually runs in — which is not always
        // the one this slot was handed.
        plate.opened(where ?? start.cwd ?? null);
        setLive(session);
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
      said: (statement) => on.current.onSaid(statement),
      closed: (session, code) => {
        plate.closed();
        setEnded(true);
        // What the program was is taken off the pane a line below, so why it stopped is settled here
        // while the two are still together.
        setStopped(whyItStopped(inPaneRef.current, code));
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
      name: (text, by) => on.current.onName(frame, text, by),
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
      // **The turn is not taken down with the pane.** A pane goes away when the person turns to
      // another page, which is exactly when they are not looking at it — saying the turn was over
      // because the page turned. What ends a turn is the pane saying so, or the session ending.
      //
      // Nothing here has to hold that open. The row above the pane goes with the pane and says so on
      // its way out (`../talk/plate`).
    };
    // Only `running` is a reason to do any of this again. `start` and `frame` are what this pane *is*
    // — a change of either would be a different pane, and the face gives that one a different key.
  }, [running]);

  // A naming reaches every row, not only the one it happened in: the rail renames a pane that is not
  // the one being worked in, and the row above that pane is where the answer shows.
  useEffect(() => {
    plateRef.current?.named(names);
  }, [names]);

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
  useEffect(() => {
    if (live === null) return;
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
  }, [frame, live]);

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
  }, [frame, live]);

  // And the caret put back, before the browser has drawn the box the sentence came back down into.
  useLayoutEffect(() => {
    const where = caret.current;
    if (where === null) return;
    caret.current = null;
    boxRef.current?.setSelectionRange(where, where);
  });

  return (
    <div
      className={`slot${focused ? " slot--focused" : ""}${landed ? " slot--landed" : ""}`}
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
        <div className="slot__bar">
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
              defaultValue={names.get(frame) ?? ""}
              autoFocus
              aria-label={t("face.rename")}
              {...asTyped}
              onKeyDown={(e) => {
                if (isEnterSubmit(e)) {
                  e.preventDefault();
                  const text = e.currentTarget.value.trim();
                  // A person's word is the last one on a frame, and an empty box is not a word: it
                  // would otherwise take the name off a pane the agent had named (`../talk/frames`).
                  if (text) onName(frame, text, "person");
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
          {live !== null && (
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
            title={t("face.drop")}
            aria-label={t("face.drop")}
            onClick={() => { void drop(); }}
          >
            <Icon name="close" />
          </button>
        </div>
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
          </Menu>
        )}
        {/* The receiving surface, drawn over the pane while something hangs on it and never otherwise —
            a file from the desktop, which this pane hears about itself, or a row of the panel, which
            the face is carrying and says so (`../files/handDrag`). It takes no pointer events: what is
            under the drag has to stay the pane, or the point being resolved would land on the surface
            itself and the highlight would flicker itself away. */}
        {(handing || offered) && <div className="slot__handing">{t("face.handHere")}</div>}
        {running
          ? (
            <>
              {ended && (
                <span className="termface__note">
                  {t("face.ended")}
                  {stopped !== null && ` ${t(stopped)}`}
                </span>
              )}
              <div className="termface__face" ref={paneRef} />
            </>
          )
          : (
            <button className="slot__open" onClick={() => setRunning(true)}>
              {t("face.open")}
            </button>
          )}
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
            (`../talk/terminal`). */}
        {live !== null && (
          <div className={`compose${written === "" ? "" : " compose--writing"}`}>
            {/* Which of the two the keyboard is answering to, said as the keyboard moves rather than
                after the fact. What it names is where a press goes, and that is the box for as long as
                the box is the thing being typed at — an empty one included, which keeps the characters
                and hands on the few presses it has nothing to do with (`keysHere`).

                A box the keyboard has left is the other way round, whatever is written in it, and the
                mark follows that too: the way out and a click into the terminal both take the keyboard
                away and leave the line where it is. */}
            <span className="compose__mark" title={t(keysHere ? "face.composeKeeps" : "face.composePasses")}>
              <Icon
                name={keysHere ? "pencil" : "keyboard"}
                label={t(keysHere ? "face.composeKeeps" : "face.composePasses")}
              />
            </span>
            <textarea
              ref={boxRef}
              className="compose__box"
              rows={1}
              value={written}
              placeholder={t("face.compose")}
              aria-label={t("face.compose")}
              {...asTyped}
              onChange={(e) => onWrite(frame, e.currentTarget.value)}
              onKeyDown={pressed}
              onFocus={() => setTyping(true)}
              onBlur={() => setTyping(false)}
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
        {/* Which model the program in this pane is answering on, and the press that moves it
            (`./PaneModel`, `AMB-D-865`). It is under the box rather than over the terminal for the
            reason the box itself is: a terminal writes into every row it was told it has.

            **It draws nothing for a pane whose program Amenbo cannot name** — the plain shell, and a
            command the reader registered — so the row is absent rather than dead there. */}
        {live !== null && <PaneModel frame={frame} session={live} agent={inPane} />}
      </div>
    </div>
  );
}
