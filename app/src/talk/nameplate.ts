// The one line above a pane: what the pane is called — and, on a pane an automation run is drawn in,
// a second line saying where that run has got to.
//
// **It says the name because the name is the one thing about a pane that is not guesswork**
// (`AMB-D-862`). What the row carried beside it was whether somebody was being waited on, which is an
// agent's word about itself and can be neither confirmed nor taken back — a row that said it went on
// saying it while the agent worked. What is left is what a person gave the pane, or the folder its
// terminal runs in until they give it one (`./frames`).
//
// In front of the name is the lamp the pane is known by (`./moving`). It takes no words and has two
// faces: **lit** while output is arriving, and **out** the rest of the time. Being lit is a
// measurement of the stream and says nothing about what the stream means — a pane printing nothing
// may be building, thinking, or waiting on somebody (`AMB-D-858`).
//
// **A run's pane is drawn as its own kind of pane** (`AMB-T-5252`). It is headed with the automation
// the run was launched from rather than with the place's name (`./plate`), and the header is the
// run's, in the run's colour (`AMB-T-5428`). It is two lines, each about one thing (`AMB-T-5529`):
// the first says which automation and which step — a mark saying nobody is typing in this one, the
// automation, which run it is, and the step running — and the line under it says which task, by
// reference and by title, with how many tasks into the run it is. Every one of those is a value
// Amenbo holds — the execution rows and the ledger — so none of it is the agent's word about itself,
// which is the whole of what this row stopped saying (`AMB-D-858`). **What the step printed is not
// here**: that is in the terminal under the row, where a reader can scroll it.
//
// **The first line holds as little as it can**, because a pane is often a quarter of a window and
// every value on the line is one more thing the automation's name is cut short by. So the task count
// is on the task's line, which is what it counts, and a run's number is written the short way (`#12`).
//
// **Which step says the action it is inside where that says something** (`AMB-D-949`). A launch opens
// one spot of the picture into a column of steps, so a step's name on its own does not always say
// which spot of the automation a reader is watching — two spots standing on the same action run
// steps of the same names. An action of one step is most often named after that step, though, and
// the same word twice is a line spent saying nothing: the action is drawn only where its name is not
// the step's. The step's own name is the one drawn heavier, because it is the part that changes from
// one step to the next.
//
// **The run's state is on the row too** (`AMB-D-955`): running, paused, completed, failed or canceled,
// said in the words the "running" and "history" tabs say it in (`../core/runWords`), on a chip in the
// state's own colour. A pane outlives the step it was opened for, so without it a run that had
// finished and one that had stopped partway looked the same — the row went on naming the last step
// either way. **Running is drawn moving**, because it is the one state that is about now: the other
// four are over or held, and hold still. What a failure is waiting for is not the row's to say: it
// is the band under the header, which has the press that answers it (`../shell/TerminalPane`).
//
// **A name too long for the row is elided, and given back in full by a panel of the row's own**
// (`../styles/global.css`). A name is what the agent typed, so it is the one thing here worth a way
// back to: the panel drops under the header, wraps inside the pane's own width, and takes no pointer
// events, so what is under it goes on being a terminal. It is dropped by a pointer resting on the row
// and by the keyboard reaching the controls beside it — the row itself is no tab stop. A run's task
// is given back there too: the title on the row is the first thing given up as a pane narrows, and
// the panel says it whole.

import { t, tf } from "../core/i18n";

/** Which of the lamp's two faces it is showing. */
export type Face =
  /** Output is arriving. A glow, held still, in the pane's own hue. */
  | "lit"
  /** Not. **Out is not away**: the lamp sinks in place rather than going, because a mark that
   *  vanished would read as the pane having gone. Nothing is read into it (`AMB-D-858`) — a pane that
   *  is printing nothing may be building, thinking, or waiting on somebody who has not been told. */
  | "out";

/** The lamp in front of the name: which pane this is, and which face it is on (`./moving`). */
export type Dot = {
  /** The hue this pane is drawn in — what tells one pane on a page from the next (`hueOf`). */
  readonly hue: number;
  /** Which of the two it is showing. */
  readonly face: Face;
};

/** Whether the lamp is lit, which is the stream and nothing else. */
export function faceOf(moving: boolean): Face {
  return moving ? "lit" : "out";
}

/**
 * **The task a run is working**, as the ledger holds it (`AutomationRunTaskDto`).
 *
 * The row draws both halves and gives the title up first as a pane narrows: a reference is short
 * enough to survive a narrow pane, and a title is what says which task it is.
 */
export type Worked = {
  readonly ref: string;
  readonly title: string;
  /** Which task of the run this is, counted from 1. A run works one stretch per task, so this and not
   *  the move count is how many tasks in a reader is. */
  readonly seq: number;
};

/**
 * **Where the run a pane is drawing has got to** — the values the row says about it.
 *
 * All of them are Amenbo's own: the automation's name, the execution row this step is running
 * under, and the task the run reserved. Nothing an agent said about itself is among them
 * (`AMB-D-858`).
 */
export type Say = {
  /** The automation the run was launched from, by the name it holds now. It heads the row in place of
   *  the place's name (`./plate`); empty where the automation has since been deleted. */
  readonly automation: string;
  /** Which run it is — the handle a person has on it from anywhere else. */
  readonly run: number;
  /** The step running now, by the name it was built under. */
  readonly step: string;
  /** The automation the run was launched from, by its id, and the spot on its picture the step was
   *  opened from, or null where the run's copy names none. They are what the step is numbered by. */
  readonly automationId: number;
  readonly placement: number | null;
  /** **The number the picture gives that spot's box** (`AMB-T-5538`), the same one the build screen
   *  draws — never how many steps the run has taken. Null until the picture has been read, and where
   *  the spot is no longer on it: the row then says the step without one. */
  readonly box: number | null;
  /** The action the spot this step was opened from stands on, or null where that spot has been taken
   *  off the picture since — and then the row says the step alone, as it does where the action is
   *  named after the step. */
  readonly action: string | null;
  /** The task the run is working, or null where it is on none — yet, or while a built-in waits for
   *  the next one (`../shell/WorkspaceFace`). */
  readonly task: Worked | null;
  /** Where the run stands, or null until the first answer about it lands — and then the row says
   *  nothing about the state rather than guessing it from the step. */
  readonly state: RunState | null;
};

/**
 * **Where a run stands**, in the words the row says it with (`../core/runWords`).
 *
 * The words are worked out by whoever reads the run, not here: they are the tabs' words, and the tabs
 * are what a reader goes to next.
 */
export type RunState = {
  /** Which of the five it is (`AMB-D-955`) — what the mark is coloured and moved by. */
  readonly status: "running" | "paused" | "completed" | "failed" | "canceled";
  /** The state in one word, a pause asked for and not yet settled included. */
  readonly word: string;
  /** A pause has been asked for and the step under way has not finished yet — the run is still
   *  `running`, and a second press on pause would be asking for what is already coming. */
  readonly pauseRequested: boolean;
  /** Why it failed, in a short phrase — or null on anything but a failure, on one core gave no
   *  reason for, and on one said by its way out instead ({@link exit}). */
  readonly why: string | null;
  /** The way out a failed step stopped at to call for a person, as the picture names it — or null on
   *  anything else. */
  readonly exit: string | null;
  /** That way out is the error one, which the picture draws apart from the rest. */
  readonly errorExit: boolean;
  /** Somebody has said they saw the failure (`amenbo_core::ops::automation_stop::acknowledge`). */
  readonly acknowledged: boolean;
};

/** The whole row. */
export type Plate = {
  readonly name: string | null;
  readonly dot: Dot;
  /** The run this pane is drawing, or null for an ordinary pane. */
  readonly run: Say | null;
};

/**
 * Draw the row into `host`, and hand back the way to draw it again.
 *
 * The elements are made once and only their words change, so a redraw on every statement costs
 * nothing and nothing under the pointer moves out from under it.
 *
 * **Handing over nothing takes the row down.** A label is about a session, and a pane that has never
 * had one has nothing to be labelled — the face there is the invitation to choose a folder
 * (`AMB-T-3606`), and a row saying the session is silent would be saying it of a session that does
 * not exist. The row is hidden rather than removed for the same reason it is redrawn rather than
 * rebuilt.
 */
export function mountNameplate(host: HTMLElement): (plate: Plate | null) => void {
  const row = document.createElement("div");
  row.className = "plate";
  const part = (name: string) => {
    const el = document.createElement("span");
    el.className = `plate__${name}`;
    row.append(el);
    return el;
  };
  // The dot goes in first, so it is to the left of the name: it is what the row belongs to, and the
  // row reads from what it is towards what is happening in it.
  const dot = part("dot");
  dot.setAttribute("aria-hidden", "true");
  // The mark that says this pane is a run's. It is in front of the name, the way a run's header reads
  // from what kind of pane this is towards where it has got to — and it is a word rather than a glyph,
  // a mark nobody can read being one more thing to ask about.
  const auto = part("auto");
  auto.textContent = t("face.auto");
  const name = part("name");
  // Which run, and which step it is on, on the name's own line. They follow the name rather than
  // taking a line of their own because they are what the name is doing now, and the line under it is
  // the task's.
  const runNo = part("no");
  const into = part("into");
  into.textContent = "›";
  into.setAttribute("aria-hidden", "true");
  const step = part("step");
  // The run's state, last on the line: it is what the rest of the line is doing, and the pane's own
  // controls for it stand straight after it (`../shell/TerminalPane`).
  const state = part("state");
  host.append(row);

  // The task the run is working, on a line of its own under the name. It is a second row and not more
  // of the first one because the first is one line by construction, and a title elided into what
  // the run's values leave of a pane's width would be a word and a half (`../styles/global.css`).
  // It carries no label: the reference says it is a task.
  const runRow = document.createElement("div");
  runRow.className = "plate-run";
  const runPart = (kind: string) => {
    const el = document.createElement("span");
    el.className = `plate-run__${kind}`;
    runRow.append(el);
    return el;
  };
  const taskRef = runPart("task");
  const taskTitle = runPart("title");
  // Which task of the run it is. It counts tasks, so it stands with the task.
  const nth = runPart("nth");
  host.append(runRow);

  // The panel the name is read in full in. It is a sibling of the row rather than a child of it,
  // because the row is one line by construction (`../styles/global.css`) and a box that dropped out
  // of it would be a second thing that line had to hold. Made once, like everything else here: what
  // changes on a redraw is the word in it, and it does not move out from under a pointer that is on
  // the panel while it changes.
  const peek = document.createElement("div");
  peek.className = "plate-peek";
  peek.setAttribute("aria-hidden", "true");
  const peekName = document.createElement("b");
  peekName.className = "plate-peek__name";
  peek.append(peekName);
  // The run's task, in full. The row has room for the reference and the panel has room for the title,
  // which is what says which task it is without going to look it up.
  const peekTask = document.createElement("span");
  peekTask.className = "plate-peek__task";
  peek.append(peekTask);
  host.append(peek);

  return (plate: Plate | null) => {
    row.hidden = plate === null;
    // A run on no task yet has nothing for the second line to say, and an empty band under the first
    // would read as a task that is there and has no name.
    runRow.hidden = plate === null || plate.run?.task == null;
    if (plate === null) {
      // The panel comes down with the row it belongs to. It is said here as well as below because the
      // row being taken away is the one path that never reaches the name, and a panel left up is an
      // empty box with a border on it — dropped, on a pane that has never had a session, by a pointer
      // resting on the row's place or by the keyboard reaching the button that removes the pane.
      peek.hidden = true;
      return;
    }
    dot.style.setProperty("--dot-hue", String(plate.dot.hue));
    dot.dataset.face = plate.dot.face;
    name.textContent = plate.name ?? "";
    // And the same name again, unelided, in the panel that drops under the header. **The row carries
    // no tooltip of its own**: the machine draws one wherever the pointer happens to stop, and one of
    // those over the panel is the same word twice in two shapes.
    peekName.textContent = plate.name ?? "";
    // A run's pane is drawn as its own kind of pane, and an ordinary one is left exactly as it was:
    // the mark is away, the second row is down, and the panel says only the name.
    auto.hidden = runNo.hidden = into.hidden = step.hidden = plate.run === null;
    state.hidden = plate.run?.state == null;
    row.classList.toggle("plate--run", plate.run !== null);
    if (plate.run !== null) {
      // The run's number is the same in every language, and written the way a reader would type it
      // into the tabs' search.
      runNo.textContent = `#${plate.run.run}`;
      step.replaceChildren(...stepWords(plate.run.box, plate.run.action, plate.run.step));
      nth.textContent = plate.run.task === null ? "" : tf("face.runTask", { n: plate.run.task.seq });
      taskRef.textContent = plate.run.task?.ref ?? "";
      taskTitle.textContent = plate.run.task?.title ?? "";
      const now = plate.run.state;
      state.textContent = now?.word ?? "";
      if (now === null) delete state.dataset.state;
      else state.dataset.state = now.status;
    }
    peekTask.textContent = plate.run?.task
      ? `${plate.run.task.ref} ${plate.run.task.title}`
      : "";
    peek.hidden = !plate.name && !peekTask.textContent;
  };
}

/**
 * Which step, with the step's own name drawn heavier than the rest: the action and the step, or the
 * step alone where there is no action to say or the action is named after it. In front of either, the
 * number of the box it was opened from, drawn as the picture draws it (`AMB-T-5538`).
 */
function stepWords(box: number | null, action: string | null, stepName: string): Node[] {
  const b = document.createElement("b");
  b.textContent = stepName;
  const no: Node[] = [];
  if (box !== null) {
    const mark = document.createElement("span");
    mark.className = "autopic__no plate__box";
    mark.textContent = String(box);
    no.push(mark);
  }
  if (action === null || action === stepName) return [...no, b];
  const into = document.createElement("span");
  into.className = "plate__into";
  into.textContent = "›";
  into.setAttribute("aria-hidden", "true");
  return [...no, document.createTextNode(action), into, b];
}
