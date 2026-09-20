import { useState } from "react";
import { Menu, MenuItem } from "../components/Menu";
import { Icon, type IconName } from "../components/Icon";
import { listLabel, t, tn } from "../core/i18n";
import { decisionRef, taskRef, type RefSpace } from "../core/idref";
import { showRef } from "../talk/terminal";

/**
 * What this session has filed, on the band under the pane it was filed from (`AMB-D-897`).
 *
 * **It counts commands that ran, never an account of them.** Each record here arrived as a note a
 * create left in the pane's drop box on its way out, so what is drawn is a `task add` that returned
 * an id — the same footing the briefed mark stands on, and the reason `AMB-D-862` took the AI's own
 * word off this row.
 *
 * **Nothing about it is written down, and it lasts exactly as long as the session does.** The
 * records are kept on the terminal itself for as long as that terminal is running, and a pane taking
 * one up is handed them (`crate::pty::Pane::adopt`, `../talk/terminal`) — so turning the page, going
 * to another project or splitting the workspace out leaves the count where it was. It goes when the
 * terminal goes. This is the running session's own tally, and what survives it is the row on the
 * record, read afterwards from the record's own screen.
 *
 * **The band is for the count and the list is for the records.** A reader watching a session wants to
 * know how much it has filed, which is a number; a reader who wants one of them wants it open, which
 * is a press. So the number stands on the band and the refs are behind it, each one landing on the
 * same screen a ref drawn in the pane would (`../talk/terminal`).
 */
export function PaneMade({ made }: { made: Made[] }) {
  // Where the list was opened, while it is open — placed at the press rather than under the button,
  // the way every menu in the app is (`../components/Menu`).
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);

  // A session that has filed nothing draws nothing. The band keeps its height either way and the
  // press that folds the box stays where it was (`./TerminalPane`), so there is nothing to hold a
  // place for here.
  if (made.length === 0) return null;

  const tasks = made.filter((one) => one.space === "task").length;
  const decisions = made.length - tasks;
  // Each side named only where there is one, and the two joined the way this language joins words in
  // a sentence: what stands between them is a word in one language and a mark in another, and
  // neither is a separator this side can spell for itself (`../core/i18n`).
  const counted = listLabel([
    ...(tasks > 0 ? [tn("act.nTasks", tasks)] : []),
    ...(decisions > 0 ? [tn("act.nDecisions", decisions)] : []),
  ]);

  return (
    <div className="maderow">
      <button
        className="maderow__count"
        type="button"
        aria-haspopup="menu"
        title={t("face.madeHere")}
        onClick={(e) => setAt({ x: e.clientX, y: e.clientY })}
      >
        {counted}
      </button>
      {at !== null && (
        <Menu at={at} onClose={() => setAt(null)}>
          {/* Newest first: the one a reader is most likely to want open is the one just filed, and
              the list is read from the top. */}
          {[...made].reverse().map((one) => (
            <MenuItem
              key={`${one.space}-${one.num}`}
              onClick={() => {
                setAt(null);
                showRef(one.space, one.num);
              }}
            >
              <Icon name={KIND_ICON[one.space]} />
              {one.space === "task" ? taskRef(one.num) : decisionRef(one.num)}
            </MenuItem>
          ))}
        </Menu>
      )}
    </div>
  );
}

/** One record this session filed — the space it is in, and its number (`crate::dto::SessionMadeDto`). */
export type Made = { space: RefSpace; num: number };

/** The mark each space wears, the same pair a search hit is drawn with (`../screens/SearchScreen`). */
const KIND_ICON = { task: "checkSquare", decision: "gavel" } as const satisfies Record<RefSpace, IconName>;
