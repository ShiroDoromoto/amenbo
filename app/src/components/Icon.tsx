import type { ReactNode } from "react";

/**
 * The icon set. Every icon is inline SVG drawn here, with no icon library behind it
 * (`AMB-D-686`) — the same shape as `BrandMark.tsx`, and for the same reason: nothing to
 * resolve at runtime, so it draws the same in the browser and in Tauri.
 *
 * The drawing convention, so the next icon added matches the ones already here:
 *
 * - **grid** — a 24×24 `viewBox`. The artwork stays inside 2..22, leaving a margin all
 *   round, so icons of different outlines still read as one size on the same row.
 * - **line, not fill** — outline only (`fill: none`), 1.75 units wide, round caps and joins.
 * - **one colour** — `currentColor` alone. An icon takes the colour of the text beside it,
 *   which is what carries it through both themes without a second set of values.
 *
 * Those three live in `.icon` (`components.css`), not in the entries below: what each
 * entry holds is geometry, and nothing else.
 *
 * How big it is drawn follows from what the icon *does*, never from the screen it is on
 * (`AMB-D-687`) — `sm` for nav and in-line marks, `md` for headings and chips, `lg` for
 * banners and onboarding. The three read the `--icon-*` tokens.
 */
export type IconSize = "sm" | "md" | "lg";

export type IconName =
  | "inbox"
  | "activity"
  | "puzzle"
  | "plug"
  | "search"
  | "book"
  | "link"
  | "gear"
  | "goose"
  | "chevronRight"
  | "chevronLeft"
  | "chevronDown"
  | "plus"
  | "menu"
  | "more"
  | "refresh"
  | "arrowUp"
  | "arrowDown"
  | "reply"
  | "clock"
  | "close"
  | "check"
  | "error"
  | "blocked"
  | "warning"
  | "hourglass"
  | "pencil"
  | "bell"
  | "unlock"
  | "calendar"
  | "pin"
  | "document"
  | "comment"
  | "tag"
  | "paperclip"
  | "checkSquare"
  | "scales"
  | "folder"
  | "trash"
  | "keyboard"
  | "clipboard"
  | "rocket"
  | "star"
  | "person"
  | "robot"
  | "dot"
  | "pause"
  | "stop"
  | "newWindow";

/**
 * The icons a call site outside React has to draw, as geometry rather than as elements.
 *
 * The pane's nameplate is built with the DOM directly — it is redrawn on every store write and only
 * its words change, so it is made once and never re-rendered (`../talk/nameplate`). It still has
 * marks in it, and a mark drawn there has to be the same mark as everywhere else. So the shapes of
 * those few live here, once, and both `ART` below and `iconSvg` are made out of them: a second
 * drawing kept in step by hand is a drawing that drifts.
 *
 * Each entry is the `d` of one path. Nothing here needs a rect or a circle, and the day one does is
 * the day to widen this — not the day to draw it twice.
 */
const DRAWN = {
  // Two upright bars — the agent has handed the turn over and is waiting on a person. It is the
  // transport mark on purpose: what it says is that something running has stopped for you.
  pause: ["M9.4 5.6v12.8", "M14.6 5.6v12.8"],
  // A square — the work in this pane has stopped. Not the pause: a pause is somebody's turn, and a
  // stop is a fact the ledger holds about the task, with nobody being asked for anything.
  stop: ["M5.6 5.6h12.8v12.8H5.6z"],
  // A warning triangle — grounds that are not settled.
  warning: [
    "M12.9 4.3l8.6 14.9a1 1 0 0 1-.9 1.5H3.4a1 1 0 0 1-.9-1.5l8.6-14.9a1 1 0 0 1 1.8 0z",
    "M12 9.8v4.4M12 17.4v.1",
  ],
} as const satisfies Partial<Record<IconName, readonly string[]>>;

/** An icon a call site outside React can ask for. The others are elements and nothing else. */
export type DrawnIcon = keyof typeof DRAWN;

/** The paths of a drawn icon as elements, so `ART` and `iconSvg` cannot fall out of step. */
function drawn(name: DrawnIcon): ReactNode {
  return <>{DRAWN[name].map((d) => <path key={d} d={d} />)}</>;
}

const ART: Record<IconName, ReactNode> = {
  // An envelope — the inbox smart view.
  inbox: (
    <>
      <rect x="2.8" y="5.4" width="18.4" height="13.2" rx="2" />
      <path d="M3.4 6.6 12 13.1l8.6-6.5" />
    </>
  ),
  // A pulse traced left to right — the activity stream.
  activity: <path d="M2.6 12h3.8l2.6-6.6 3.9 13.2 2.6-6.6h5.9" />,
  // A puzzle piece — the plugin market. Its knobs are what say "it fits into something".
  puzzle: (
    <path d="M4 7.5a1.2 1.2 0 0 1 1.2-1.2h3.4a2.4 2.4 0 1 1 4.8 0h3.4a1.2 1.2 0 0 1 1.2 1.2v3.4a2.4 2.4 0 1 1 0 4.8v3.4a1.2 1.2 0 0 1-1.2 1.2H5.2A1.2 1.2 0 0 1 4 19.1V7.5z" />
  ),
  // A plug — the plugins already installed on this machine.
  plug: (
    <>
      <path d="M8.6 2.8v4.8M15.4 2.8v4.8" />
      <path d="M5.2 7.6h13.6v3.6a6.8 6.8 0 0 1-13.6 0V7.6z" />
      <path d="M12 18v3.4" />
    </>
  ),
  // A magnifier — search.
  search: (
    <>
      <circle cx="10.6" cy="10.6" r="6.8" />
      <path d="M15.6 15.6 20.9 20.9" />
    </>
  ),
  // An open book — the command reference.
  book: (
    <>
      <path d="M12 7.4C10.4 5.9 7.9 5.2 3.6 5.2v12.4c4.3 0 6.8.7 8.4 2.2 1.6-1.5 4.1-2.2 8.4-2.2V5.2c-4.3 0-6.8.7-8.4 2.2z" />
      <path d="M12 7.4v12.4" />
    </>
  ),
  // Two links of a chain — the AI connected over MCP.
  link: (
    <>
      <path d="M10.2 13.2a4.6 4.6 0 0 0 6.9.5l2.8-2.8a4.6 4.6 0 0 0-6.5-6.5l-1.6 1.6" />
      <path d="M13.8 10.8a4.6 4.6 0 0 0-6.9-.5l-2.8 2.8a4.6 4.6 0 0 0 6.5 6.5l1.6-1.6" />
    </>
  ),
  // A cogwheel — settings. Eight teeth standing off a ring, drawn as spokes so the
  // outline stays open at 16px instead of filling in.
  gear: (
    <>
      <circle cx="12" cy="12" r="6.2" />
      <circle cx="12" cy="12" r="2.6" />
      <path d="M12 5.8V3.4M12 18.2v2.4M18.2 12h2.4M5.8 12H3.4M16.4 7.6l1.7-1.7M7.6 16.4l-1.7 1.7M16.4 16.4l1.7 1.7M7.6 7.6 5.9 5.9" />
    </>
  ),
  // A goose afloat — the onboarding view. The hardest of the set to draw, and the one
  // that settled the proportions: at 16px what names the bird is its neck and its beak,
  // so the neck takes two thirds of the height and the body stays shallow. Drawn with a
  // body as deep as the envelope is tall, the neck closes up and it reads as a blot.
  goose: (
    <>
      <circle cx="15.6" cy="5.2" r="3.4" />
      <path d="M18.9 4.4 21.6 5.4l-2.7 1.2" />
      <path d="M12.9 8c-3.1 2.4-4.5 5.4-4.2 9" />
      <path d="M8.7 17c-2.3-.6-4-1-6-.4 1.4 2.6 4.6 4.1 8.4 4.1 3.4 0 6.1-1.3 7.5-3.2-3.3.9-6.7.5-9.9-.5z" />
    </>
  ),
  // The disclosure pair — a section folded shut, and the same one open. The two sideways
  // ones double as "the one before / the one after" wherever a screen pages or steps: going
  // back a page, a month, or a step in history is the one movement, so it is the one mark.
  chevronRight: <path d="M9.4 5.6 15.8 12l-6.4 6.4" />,
  chevronLeft: <path d="M14.6 5.6 8.2 12l6.4 6.4" />,
  chevronDown: <path d="M5.6 9.4 12 15.8l6.4-6.4" />,
  // A plus — adding one more of what the section holds.
  plus: <path d="M12 4.8v14.4M4.8 12h14.4" />,

  // ----- the marks that say why a task cannot be started, and what moved under its holder -----
  // A barred circle — a dependency that is not finished yet.
  blocked: (
    <>
      <circle cx="12" cy="12" r="8.4" />
      <path d="M6.6 12h10.8" />
    </>
  ),
  warning: drawn("warning"),
  // An hourglass — a start day that has not come.
  hourglass: (
    <>
      <path d="M6.6 3.4h10.8M6.6 20.6h10.8" />
      <path d="M7.4 3.4c0 4 4.6 4.6 4.6 8.6s-4.6 4.6-4.6 8.6" />
      <path d="M16.6 3.4c0 4-4.6 4.6-4.6 8.6s4.6 4.6 4.6 8.6" />
    </>
  ),
  // A pencil — a task still being written.
  pencil: (
    <>
      <path d="M4 20h4l11-11a3 3 0 0 0-4-4L4 16v4z" />
      <path d="M14.4 6.6 17.4 9.6" />
    </>
  ),
  // A bell — a premise that moved after the task was reserved.
  bell: (
    <>
      <path d="M18 8.8a6 6 0 1 0-12 0c0 6.2-2.4 7.8-2.4 7.8h16.8S18 15 18 8.8z" />
      <path d="M10.3 20.4a2 2 0 0 0 3.4 0" />
    </>
  ),
  // An open padlock — grounds that were settled and have stopped being so.
  unlock: (
    <>
      <rect x="4.2" y="10.6" width="15.6" height="10" rx="2" />
      <path d="M8.2 10.6V6.9a3.8 3.8 0 0 1 7.2-1.7" />
    </>
  ),
  // A calendar — the day a task is due.
  calendar: (
    <>
      <rect x="3.2" y="5.4" width="17.6" height="15.4" rx="2" />
      <path d="M3.2 10.2h17.6M8 3.2v4.2M16 3.2v4.2" />
    </>
  ),

  // ----- the marks a search hit is read by: which record, and which of its faces -----
  // A ticked box — the hit is on a task.
  checkSquare: (
    <>
      <rect x="3.4" y="3.4" width="17.2" height="17.2" rx="2.4" />
      <path d="M7.8 12.2 10.9 15.3 16.4 9.2" />
    </>
  ),
  // A pair of scales — the hit is on a decision. The same mark the decisions screen uses.
  scales: (
    <>
      <path d="M12 3.4v17.4M7.4 20.8h9.2" />
      <path d="M3.2 7.4H5c2 0 5-1 7-2 2 1 5 2 7 2h1.8" />
      <path d="M2.2 16.2 5 8.4l2.8 7.8a4.6 4.6 0 0 1-5.6 0z" />
      <path d="M16.2 16.2 19 8.4l2.8 7.8a4.6 4.6 0 0 1-5.6 0z" />
    </>
  ),
  // A pushpin — the words are in the record's title.
  pin: (
    <>
      <path d="M9.4 3.2h5.2v6.3l3 3.4v1.5H6.4v-1.5l3-3.4V3.2z" />
      <path d="M12 14.4v6.4" />
    </>
  ),
  // A sheet — the words are in the record's body.
  document: (
    <>
      <path d="M13.6 3.2H7a2 2 0 0 0-2 2v13.6a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.6z" />
      <path d="M13.6 3.2v5.4H19M8.6 13.2h6.8M8.6 16.8h4.6" />
    </>
  ),
  // A bubble — the words are in a remark on the record.
  comment: (
    <path d="M12 4.2c-4.7 0-8.5 3.2-8.5 7.2 0 2 .9 3.8 2.4 5.1L4.6 20.4l4.5-1.7c.9.3 1.9.4 2.9.4 4.7 0 8.5-3.2 8.5-7.2S16.7 4.2 12 4.2z" />
  ),
  // A tag — the words are in a value the record is filed under.
  tag: (
    <>
      <path d="M2.6 4.6a2 2 0 0 1 2-2h6.6a2 2 0 0 1 1.4.6l8.2 8.2a2 2 0 0 1 0 2.8l-6.6 6.6a2 2 0 0 1-2.8 0L3.2 12.6a2 2 0 0 1-.6-1.4V4.6z" />
      <circle cx="7.3" cy="7.3" r="1.2" />
    </>
  ),
  // A paperclip — the words are in the name of something hung on the record.
  paperclip: (
    <path d="M20.6 11.2 12 19.8a5.4 5.4 0 0 1-7.6-7.6l8.6-8.6a3.6 3.6 0 0 1 5.1 5.1l-8.6 8.6a1.8 1.8 0 0 1-2.6-2.6l8-8" />
  ),

  // ----- the marks on the frame of a screen, which point at a move rather than at a record -----
  // Three rules — the sidebar, folded away and brought back.
  menu: <path d="M3.6 6.8h16.8M3.6 12h16.8M3.6 17.2h16.8" />,
  // Three dots in a row — the menu a row carries, as against the three lines above, which open the
  // rail. Two menus that looked the same would be two presses a reader has to tell apart by where
  // they are.
  more: (
    <>
      <circle cx="5.6" cy="12" r="1.15" />
      <circle cx="12" cy="12" r="1.15" />
      <circle cx="18.4" cy="12" r="1.15" />
    </>
  ),
  // An arrow come full circle — read this screen again from the store.
  refresh: (
    <>
      <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
    </>
  ),
  // Up and down — moving one row past its neighbour in an ordered list.
  arrowUp: <path d="M12 20.2V4.2M5.6 10.6 12 4.2l6.4 6.4" />,
  arrowDown: <path d="M12 3.8v16M5.6 13.4 12 19.8l6.4-6.4" />,
  // An arrow turning back on itself — answering the line it points at.
  reply: (
    <>
      <path d="M9.4 6.4 3.6 12l5.8 5.6" />
      <path d="M3.6 12h10.6a6 6 0 0 1 6 6v1.8" />
    </>
  ),
  // A stopwatch — when the thing that put this in the inbox happened.
  clock: (
    <>
      <circle cx="12" cy="13.6" r="7.6" />
      <path d="M12 9.4v4.2l2.8 1.7" />
      <path d="M9.6 2.6h4.8M12 2.6v3.4" />
    </>
  ),
  // A cross — the move that puts something away: a banner dismissed, a pane shut, a row taken off.
  close: <path d="M5.8 5.8 18.2 18.2M18.2 5.8 5.8 18.2" />,
  // A tick — what the reader asked for went through, and there is nothing left to do about it. It is
  // the same stroke `checkSquare` carries inside its box, drawn out to the same margins as the rest.
  check: <path d="M4.4 12.8 9.8 18.2 19.6 6.4" />,
  // A cross inside a circle — a fault, as against the triangle's warning. The two are read side by
  // side in one list, so they are told apart by their outline before either is read.
  error: (
    <>
      <circle cx="12" cy="12" r="8.4" />
      <path d="M9.2 9.2 14.8 14.8M14.8 9.2 9.2 14.8" />
    </>
  ),

  // ----- the marks on the moves the reader makes on their own machine, outside the store -----
  // A folder — the one thing on disk Amenbo asks for. Choosing one, opening one in the file
  // manager and re-linking one are the same object under three verbs, so they are the one mark.
  folder: (
    <path d="M2.8 6.6a2 2 0 0 1 2-2h4.1l2.3 2.7h8a2 2 0 0 1 2 2v9.1a2 2 0 0 1-2 2H4.8a2 2 0 0 1-2-2V6.6z" />
  ),
  // A bin — deleting the record for good. It is the one mark that is never undone, so it is drawn
  // as the container rather than as a cross: a cross is dismissal, which puts nothing away for ever.
  trash: (
    <>
      <path d="M4.2 6.6h15.6" />
      <path d="M9.6 6.6V4.5a1.3 1.3 0 0 1 1.3-1.3h2.2a1.3 1.3 0 0 1 1.3 1.3v2.1" />
      <path d="M6.4 6.6l.9 12.7a1.6 1.6 0 0 0 1.6 1.5h6.2a1.6 1.6 0 0 0 1.6-1.5l.9-12.7" />
      <path d="M10.2 10.4v6.6M13.8 10.4v6.6" />
    </>
  ),
  // A keyboard — opening a terminal already inside the folder. What it points at is the typing that
  // follows, which is the whole of what the button hands over.
  keyboard: (
    <>
      <rect x="2.4" y="6.4" width="19.2" height="11.2" rx="2" />
      <path d="M6 10.4h2M11 10.4h2M16 10.4h2" />
      <path d="M8 14.2h8" />
    </>
  ),
  // A clipboard — text copied for pasting somewhere Amenbo cannot reach: a shell, or the reader's
  // own AI. The clip is what separates it from the plain sheet the search hits use.
  clipboard: (
    <>
      <rect x="5.2" y="4.6" width="13.6" height="16" rx="2" />
      <rect x="9" y="2.6" width="6" height="3.8" rx="1.2" />
    </>
  ),
  // A rocket — the first loop, which is the one push that gets a new project moving.
  rocket: (
    <>
      <path d="M12 2.8c2.7 2.6 4.1 5.9 4.1 9.6v3.2H7.9v-3.2c0-3.7 1.4-7 4.1-9.6z" />
      <circle cx="12" cy="10.2" r="1.8" />
      <path d="M7.9 12.8 4.6 16v3.4l3.3-2.2M16.1 12.8l3.3 3.2v3.4l-3.3-2.2" />
      <path d="M10.3 18.6 12 21.4l1.7-2.8" />
    </>
  ),
  // A star — how many people have starred a plugin's repository on GitHub. It counts something
  // outside Amenbo, which is why it sits beside the download figure and not among the record marks.
  star: (
    <path d="M12 3.2l2.7 5.5 6.1.9-4.4 4.3 1 6-5.4-2.8-5.4 2.8 1-6L3.2 9.6l6.1-.9z" />
  ),

  // ----- the two facets, where the settings screen asks what each of them is called -----
  // A head and shoulders — the person. Everywhere else a facet is drawn as its avatar
  // (`FacetAvatar`), which is a picture of *this* human; this is the kind itself.
  person: (
    <>
      <circle cx="12" cy="7.8" r="3.9" />
      <path d="M4.6 20.6c0-3.9 3.3-6.2 7.4-6.2s7.4 2.3 7.4 6.2" />
    </>
  ),
  // A head with an aerial — that person's AI. It is told from the human by its outline
  // (a box against a circle), which is what carries the pair at 16px.
  robot: (
    <>
      <rect x="3.6" y="7.6" width="16.8" height="12" rx="2.6" />
      <path d="M12 3.4v4.2" />
      <path d="M8.8 12.6v1.8M15.2 12.6v1.8" />
    </>
  ),
  // A filled disc — the priority a task carries, which is read off the colour it is drawn in.
  // It is the one entry that is filled rather than stroked, so `.icon[data-icon="dot"]`
  // (`components.css`) turns the convention round for it.
  dot: <circle cx="12" cy="12" r="5.4" />,
  pause: drawn("pause"),
  stop: drawn("stop"),
  // Two panes, one lifted off the other — the terminal put into a window of its own. It is drawn as
  // the same thing twice because that is what the press does: the face is still here, and one of it
  // is now somewhere else.
  newWindow: (
    <>
      <path d="M7.4 16.6H4.6a1.4 1.4 0 0 1-1.4-1.4V4.6a1.4 1.4 0 0 1 1.4-1.4h10.6a1.4 1.4 0 0 1 1.4 1.4v2.8" />
      <rect x="7.4" y="7.4" width="13.4" height="13.4" rx="1.6" />
    </>
  ),
};

/**
 * The same icon, built with the DOM, for the one place that has no React around it
 * (`../talk/nameplate`). It carries the same class, the same `data-icon` and the same box, so the
 * stylesheet and anything reading the markup cannot tell the two apart — which is the point.
 *
 * Only the icons in `DRAWN` can be had this way, and the type says which — a mark this cannot draw is
 * a mistake the compiler catches rather than an empty box nobody would notice on the screen.
 */
export function iconSvg(name: DrawnIcon, size: IconSize = "sm"): SVGSVGElement {
  const paths = DRAWN[name];
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("class", `icon icon--${size}`);
  svg.setAttribute("data-icon", name);
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  for (const d of paths) {
    const path = document.createElementNS(ns, "path");
    path.setAttribute("d", d);
    svg.append(path);
  }
  return svg;
}

/**
 * One icon. `label` is for the rare icon that stands alone and has to say what it is; an
 * icon that sits beside its own text takes the default and stays out of the element's
 * accessible name, so the name is the text (`AMB-D-439`).
 */
export function Icon({ name, size = "sm", label }: { name: IconName; size?: IconSize; label?: string }) {
  return (
    <svg
      className={`icon icon--${size}`}
      // Which mark this is, in the markup. An icon carries no text, so without it a reader of the
      // DOM — a test, or anyone looking at what a row actually drew — has only path data to go on.
      data-icon={name}
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
    >
      {ART[name]}
    </svg>
  );
}
