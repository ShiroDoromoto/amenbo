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
  | "chevronDown"
  | "plus";

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
  // The disclosure pair — a section folded shut, and the same one open.
  chevronRight: <path d="M9.4 5.6 15.8 12l-6.4 6.4" />,
  chevronDown: <path d="M5.6 9.4 12 15.8l6.4-6.4" />,
  // A plus — adding one more of what the section holds.
  plus: <path d="M12 4.8v14.4M4.8 12h14.4" />,
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
