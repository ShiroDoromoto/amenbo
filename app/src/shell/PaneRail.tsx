import { useEffect, useRef, useState } from "react";
import { folderName, type FrameNames } from "../talk/frames";
import { panesOf, type Frame, type Layout } from "../talk/layout";
import type { Project } from "../mock/types";
import { t } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";

/**
 * The rail beside the panes: the projects, and under the one being shown, its panes.
 *
 * **It is where the project is chosen, and choosing one changes the whole screen.** A pane belongs to
 * a project (`../talk/layout`) and there is no way to point one at a folder outside it, so the rail is
 * not a list of panes that happen to be grouped — it is the division itself, drawn. Picking a project
 * puts its panes on the face and takes the others off; picking a pane goes to it, bringing its page up
 * with it.
 *
 * **Only the shown project's panes are listed.** The others are one row each: what a person does with
 * a project they are not in is go to it, and a rail that unfolded every one of them would be a list of
 * everything this machine has ever opened. What a folded project still says is that somebody's turn is
 * standing in it — a dot, which is the only way a turn in a project nobody is looking at is knocked
 * about at all (`AMB-T-3610`).
 *
 * **Panes are in name order.** They are opened in whatever order the work went, and a list that kept
 * that order would move a pane's row every time an older one was closed and opened again. A name is
 * what a person looks for.
 *
 * **The rail is where a person names a pane.** A name belongs to the frame and a person's word is the
 * last one (`../talk/frames`), so the rename is here rather than on the pane: it is the one place a
 * frame with nothing running in it can still be named.
 *
 * **Two lists under two headings, and not one list that nests.** What a person picks here is a
 * project or a pane of it, and the two are different choices: nesting the panes under the row that
 * chooses their project made the rail one tree where the second choice looked like part of the
 * first.
 *
 * **Nothing is opened from here.** Whichever way the face is standing, the way in is already beside
 * the panes: a page with room draws an empty frame, which is where what to open with is chosen, and a
 * full page draws the strip (`./TerminalFace`). The rail's own way in went to the page with room —
 * which, pressed from a page that had room, was where the reader was already standing.
 */
export function PaneRail({
  layout, names, projects, needy, onProject, onPick, onRename,
}: {
  layout: Layout;
  names: FrameNames;
  /** The projects this machine knows, in the order the ledger keeps them. */
  projects: readonly Project[];
  /** The frames a turn is standing in. What is drawn from it here is the dot on a project that is not
   *  the one being shown — the panes of the one that is say it for themselves. */
  needy: ReadonlySet<string>;
  onProject: (project: number) => void;
  onPick: (frame: string) => void;
  onRename: (frame: string, name: string) => void;
}) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    field.current?.select();
  }, [renaming]);

  const shownProject = projects.find((one) => one.id === layout.project) ?? null;
  const panes = shownProject === null ? [] : panesOf(layout, shownProject.id);

  return (
    <nav className="rail" aria-label={t("face.rail")}>
      <div className="rail__head">
        <h2 className="rail__title">{t("face.projects")}</h2>
      </div>
      <div className="rail__list rail__list--projects">
        {projects.map((project) => {
          const shown = layout.project === project.id;
          return (
            <button
              key={project.id}
              className={`rail__project${shown ? " rail__project--on" : ""}`}
              aria-current={shown ? "true" : undefined}
              onClick={() => onProject(project.id)}
            >
              <span className="rail__name">{project.name}</span>
              {/* Only for a project the reader is not looking at: the panes of the one they are
                  each say whose turn it is for themselves (`../talk/nameplate`). */}
              {!shown && panesOf(layout, project.id).some((pane) => needy.has(pane.id)) && (
                <span className="rail__needs" title={t("face.needsYou")} aria-hidden="true" />
              )}
            </button>
          );
        })}
      </div>
      <div className="rail__head">
        <h2 className="rail__title">{t("face.sessions")}</h2>
      </div>
      <div className="rail__list rail__list--panes">
        {inNameOrder(panes, names, layout.count).map(({ frame, label }) =>
          renaming === frame.id
            ? (
              <input
                key={frame.id}
                ref={field}
                className="rail__rename"
                defaultValue={names.get(frame.id) ?? ""}
                autoFocus
                aria-label={t("face.rename")}
                {...asTyped}
                onKeyDown={(e) => {
                  if (isEnterSubmit(e)) {
                    e.preventDefault();
                    const text = e.currentTarget.value.trim();
                    if (text) onRename(frame.id, text);
                    setRenaming(null);
                  }
                  if (e.key === "Escape") setRenaming(null);
                }}
                onBlur={() => setRenaming(null)}
              />
            )
            : (
              <button
                key={frame.id}
                className={`rail__row${layout.focus === frame.id ? " rail__row--focused" : ""}`}
                onClick={() => onPick(frame.id)}
                onDoubleClick={() => setRenaming(frame.id)}
                title={t("face.rename")}
              >
                <span className="rail__name">{label}</span>
                {frame.session === null && <span className="rail__idle">·</span>}
              </button>
            ))}
      </div>
    </nav>
  );
}

/**
 * The panes of one project with what each is called, in name order.
 *
 * A pane nobody has named is called after the folder it works in (`../talk/frames`), and one that has
 * not been opened in a folder either is called where it is — the page it is on and its place on it,
 * counted the way the pages are. Both are names as far as the order is concerned: sorting the unnamed
 * ones away from the named would put a reader's own words in one half of the list and the app's in the
 * other, and the list is one list.
 *
 * **Two panes in one folder keep their places beside it.** A folder is not a name anybody chose, so
 * two panes working in the same one read the same — and a rail is where a person tells panes apart and
 * goes to one, which two identical rows are no use for. A name that repeats is left alone: two panes a
 * person called the same thing is a person's own business.
 */
function inNameOrder(
  panes: readonly Frame[],
  names: FrameNames,
  count: number,
): { frame: Frame; label: string }[] {
  const folders = panes.map((frame) => (names.has(frame.id) ? null : folderName(frame.folder)));
  const shared = new Set(folders.filter((one, at) => one !== null && folders.indexOf(one) !== at));
  return panes
    .map((frame, at) => {
      const place = `${Math.floor(at / count) + 1}.${(at % count) + 1}`;
      const folder = folders[at];
      return {
        frame,
        label: names.get(frame.id)
          ?? (folder === null ? place : shared.has(folder) ? `${folder} ${place}` : folder),
      };
    })
    .sort((a, b) => a.label.localeCompare(b.label));
}
