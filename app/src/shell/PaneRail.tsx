import { useEffect, useRef, useState, type ReactNode } from "react";
import { paneLabels, type FrameNames } from "../talk/frames";
import { panesOf, type Frame, type Layout } from "../talk/layout";
import type { RailTab } from "../talk/columns";
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
 * **The projects are also drawn at the edge of the face** (`./ProjectTabs`, `AMB-D-838`), which is
 * where that choice is moving to: a project is what everything else here is inside, and the list of
 * them sitting in one half of one column had the top of the hierarchy drawn under the bottom of it.
 * Both are on the screen for now — this half is what `AMB-T-4281` takes away, leaving the tree.
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
 *
 * **The rail has a second half: the folder tree** (`AMB-D-835`). Two lists and a tree do not fit one
 * on top of another in a column this narrow, so the two halves are swapped rather than stacked, and
 * the width stays where it is. Which half is up is kept per project and changes only when a person
 * presses one of them: a rail that swapped itself would take the lists away from under the hand of
 * whoever was choosing a pane.
 */
export function PaneRail({
  layout, names, projects, needy, tab, onTab, folders, onProject, onPick, onRename,
}: {
  layout: Layout;
  names: FrameNames;
  /** The projects this machine knows, in the order the ledger keeps them. */
  projects: readonly Project[];
  /** Which half is up — the lists, or the tree (`../talk/columns`). */
  tab: RailTab;
  onTab: (tab: RailTab) => void;
  /**
   * The tree, drawn by whoever holds the file being read (`../files/FolderTree`).
   *
   * Handed in rather than mounted here: what a row opens is drawn in the column on the other side of
   * the panes, so the two sides answer to one state — and the face that holds that state is the one
   * place both of them can be reached from.
   */
  folders: ReactNode;
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
      {/* Both halves always shown, and exactly one of them on: which half is up is what a person is
          choosing between, and a control that only named the other one would make them press it to
          find out where they are. */}
      <div className="rail__tabs" role="radiogroup" aria-label={t("face.railHalves")}>
        {(["panes", "folders"] as const).map((half) => (
          <button
            key={half}
            className={`rail__tab${tab === half ? " rail__tab--on" : ""}`}
            role="radio"
            aria-checked={tab === half}
            onClick={() => onTab(half)}
          >
            {t(half === "panes" ? "face.railPanes" : "face.railFolders")}
          </button>
        ))}
      </div>
      {tab === "folders" ? folders : (
        <>
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
        </>
      )}
    </nav>
  );
}

/**
 * The panes of one project with what each is called, in name order.
 *
 * What each is called is worked out where the naming rules are (`../talk/frames`); the order is the
 * rail's own. A pane nobody has named is called after its folder, and both are names as far as the
 * order is concerned: sorting the unnamed ones away from the named would put a reader's own words in
 * one half of the list and the app's in the other, and the list is one list.
 */
function inNameOrder(
  panes: readonly Frame[],
  names: FrameNames,
  count: number,
): { frame: Frame; label: string }[] {
  const labels = paneLabels(panes, names, count);
  return panes
    .map((frame) => ({ frame, label: labels.get(frame.id) ?? frame.id }))
    .sort((a, b) => a.label.localeCompare(b.label));
}
