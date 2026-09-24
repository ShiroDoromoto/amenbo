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
// run's, in the run's colour (`AMB-T-5428`): the first line says the run — a mark saying nobody is
// typing in this one, the automation, which run it is, which step is running and how many moves in
// that is, and how many tasks in the run it is — and the line under it says the task the run is
// working, by reference and by title. Every one of those is a value
// Amenbo holds — the execution rows and the ledger — so none of it is the agent's word about itself,
// which is the whole of what this row stopped saying (`AMB-D-858`). **What the step printed is not
// here**: that is in the terminal under the row, where a reader can scroll it.
//
// **Which step says the action it is inside as well as its own name** (`AMB-D-949`). A launch opens
// one spot of the picture into a column of steps, so a step's name on its own no longer says which
// spot of the automation a reader is watching — two spots standing on the same action run steps of
// the same names. The two are one value and not two, because a language orders them its own way
// (`auto.run.inAction`), and because the row gives values up whole as a pane narrows: half of "which
// step" would be a name pointing at nothing. The step's own name is the one drawn heavier, because it
// is the part that changes from one step to the next — the action and the count only move with it.
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
  /** How many moves in this one is, counted from 1. A run may walk the same step several times, so
   *  the count and not the step's name is what says how far in a reader is. */
  readonly seq: number;
  /** The step running now, by the name it was built under. */
  readonly step: string;
  /** The action the spot this step was opened from stands on, or null where that spot has been taken
   *  off the picture since — and then the row says the step alone. */
  readonly action: string | null;
  /** The task the run is working, or null where it is on none yet. */
  readonly task: Worked | null;
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
  // Where the run has got to, on the name's own line. They follow the name rather than taking a line
  // of their own because they are what the name is doing now, and the line under it is the task's.
  const runNo = part("no");
  const step = part("step");
  const nth = part("nth");
  host.append(row);

  // The task the run is working, on a line of its own under the name. It is a second row and not more
  // of the first one because the first is one line by construction, and a title elided into what
  // the run's values leave of a pane's width would be a word and a half (`../styles/global.css`).
  const runRow = document.createElement("div");
  runRow.className = "plate-run";
  const runPart = (kind: string) => {
    const el = document.createElement("span");
    el.className = `plate-run__${kind}`;
    runRow.append(el);
    return el;
  };
  const taskLabel = runPart("label");
  taskLabel.textContent = t("auto.step.task");
  const taskRef = runPart("task");
  const taskTitle = runPart("title");
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
    auto.hidden = runNo.hidden = step.hidden = nth.hidden = plate.run === null;
    row.classList.toggle("plate--run", plate.run !== null);
    if (plate.run !== null) {
      runNo.textContent = tf("face.runNo", { n: plate.run.run });
      const which = plate.run.action === null
        ? STEP
        : tf("auto.run.inAction", { action: plate.run.action, step: STEP });
      step.replaceChildren(...stepWords(tf("auto.run.step", { n: plate.run.seq, step: which }), plate.run.step));
      nth.textContent = plate.run.task === null ? "" : tf("face.runTask", { n: plate.run.task.seq });
      nth.hidden = plate.run.task === null;
      taskRef.textContent = plate.run.task?.ref ?? "";
      taskTitle.textContent = plate.run.task?.title ?? "";
    }
    peekTask.textContent = plate.run?.task
      ? `${plate.run.task.ref} ${plate.run.task.title}`
      : "";
    peek.hidden = !plate.name && !peekTask.textContent;
  };
}

/** Where the step's own name goes in the sentence that says which step, until it is drawn in. The
 *  sentence is put together by the language (`auto.run.step`, `auto.run.inAction`), so the name is
 *  found in it by this rather than by where it happens to fall in one language's order. */
const STEP = "\u0000";

/** The sentence that says which step, with the step's own name drawn heavier than the rest. */
function stepWords(sentence: string, stepName: string): Node[] {
  const out: Node[] = [];
  sentence.split(STEP).forEach((text, i) => {
    if (i > 0) {
      const b = document.createElement("b");
      b.textContent = stepName;
      out.push(b);
    }
    if (text !== "") out.push(document.createTextNode(text));
  });
  return out;
}
