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
 * **And it is where another pane is opened in the project already up.** Where the project is bound to
 * one folder nothing is asked at all; where it is bound to several the question is which of them, and
 * never anything outside the project (`./FolderChoice`).
 */
export function PaneRail({
  layout, names, projects, needy, onProject, onPick, onRename, onOpen,
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
  /** Open another pane in this project. */
  onOpen: (project: number) => void;
}) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    field.current?.select();
  }, [renaming]);

  return (
    <nav className="rail" aria-label={t("face.rail")}>
      {projects.map((project) => {
        const shown = layout.project === project.id;
        const panes = panesOf(layout, project.id);
        return (
          <div className="rail__project" key={project.id}>
            <div className="rail__projectrow">
              <button
                className={`rail__projectname${shown ? " rail__projectname--on" : ""}`}
                aria-current={shown ? "true" : undefined}
                onClick={() => onProject(project.id)}
              >
                {project.name}
                {/* Only for a project the reader is not looking at: the panes of the one they are
                    each say whose turn it is for themselves (`../talk/nameplate`). */}
                {!shown && panes.some((pane) => needy.has(pane.id)) && (
                  <span className="rail__needs" title={t("face.needsYou")} aria-hidden="true" />
                )}
              </button>
              {/* Beside the project it opens in, because that is what it is about. It is offered for
                  the project being shown and no other: opening a pane goes to it, and a press that
                  moved the screen somewhere else as a side effect is one nobody meant. */}
              {shown && (
                <button
                  className="rail__open"
                  title={t("face.openHere")}
                  aria-label={t("face.openHere")}
                  onClick={() => onOpen(project.id)}
                >
                  +
                </button>
              )}
            </div>
            {shown && inNameOrder(panes, names, layout.count).map(({ frame, label }) =>
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
        );
      })}
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
