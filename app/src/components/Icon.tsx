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
  | "refresh"
  | "arrowUp"
  | "arrowDown"
  | "reply"
  | "clock"
  | "close"
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
  | "scales";

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
  // A warning triangle — grounds that are not settled.
  warning: (
    <>
      <path d="M12.9 4.3l8.6 14.9a1 1 0 0 1-.9 1.5H3.4a1 1 0 0 1-.9-1.5l8.6-14.9a1 1 0 0 1 1.8 0z" />
      <path d="M12 9.8v4.4M12 17.4v.1" />
    </>
  ),
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
  // A cross inside a circle — a fault, as against the triangle's warning. The two are read side by
  // side in one list, so they are told apart by their outline before either is read.
  error: (
    <>
      <circle cx="12" cy="12" r="8.4" />
      <path d="M9.2 9.2 14.8 14.8M14.8 9.2 9.2 14.8" />
    </>
  ),
};

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
