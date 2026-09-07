// The one line above a pane: what the pane is called, and nothing else.
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
// **A name too long for the row is elided, and given back in full by a panel of the row's own**
// (`../styles/global.css`). A name is what the agent typed, so it is the one thing here worth a way
// back to: the panel drops under the header, wraps inside the pane's own width, and takes no pointer
// events, so what is under it goes on being a terminal. It is dropped by a pointer resting on the row
// and by the keyboard reaching the controls beside it — the row itself is no tab stop.

import { hueOf } from "./moving";

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
  /** The frame the row belongs to. Its hue is what tells one pane from another. */
  readonly frame: string;
  /** Which of the two it is showing. */
  readonly face: Face;
};

/** Whether the lamp is lit, which is the stream and nothing else. */
export function faceOf(moving: boolean): Face {
  return moving ? "lit" : "out";
}

/** The whole row. */
export type Plate = {
  readonly name: string | null;
  readonly dot: Dot;
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
  const name = part("name");
  host.append(row);

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
  host.append(peek);

  return (plate: Plate | null) => {
    row.hidden = plate === null;
    if (plate === null) {
      // The panel comes down with the row it belongs to. It is said here as well as below because the
      // row being taken away is the one path that never reaches the name, and a panel left up is an
      // empty box with a border on it — dropped, on a pane that has never had a session, by a pointer
      // resting on the row's place or by the keyboard reaching the button that removes the pane.
      peek.hidden = true;
      return;
    }
    dot.style.setProperty("--dot-hue", String(hueOf(plate.dot.frame)));
    dot.dataset.face = plate.dot.face;
    name.textContent = plate.name ?? "";
    // And the same name again, unelided, in the panel that drops under the header. **The row carries
    // no tooltip of its own**: the machine draws one wherever the pointer happens to stop, and one of
    // those over the panel is the same word twice in two shapes.
    peekName.textContent = plate.name ?? "";
    peek.hidden = !plate.name;
  };
}
