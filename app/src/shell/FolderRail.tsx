import type { ReactNode } from "react";
import type { Project } from "../mock/types";
import { t } from "../core/i18n";
import type { RailTab } from "../talk/columns";

/**
 * The column beside the panes: the folders of the project being shown, under its name
 * (`AMB-D-838`).
 *
 * **It holds one list at a time, and the row of tabs is how the other comes up** (`AMB-D-835`). A
 * column this narrow has room for one, so the folder's own names and what git says about it take
 * turns rather than standing one above the other. The other two things a person picks on this face
 * are elsewhere: the projects are the tabs at the edge (`./ProjectTabs`) and the panes are the
 * middle of the screen.
 *
 * **The name at the top is whose folders these are.** A tree drawn without it says which folders are
 * bound but not what they are bound to, and this face has two things called by a project's name —
 * the tab that is on, and this. They agree because they are the same answer read twice: what the tab
 * chose is what the tree is rooted in.
 *
 * **Under the name stands which of those folders the window is on** (`AMB-D-905`), where the project
 * has more than one. It is handed in for the reason the tree is: the choice is one answer with two
 * readers, so the face holds it and both are drawn from it.
 *
 * **Both halves are handed in rather than mounted here** (`../files/FolderTree`,
 * `../files/GitPanel`): what a row opens is drawn in the column on the other side of the panes, so
 * the two sides answer to one state — and the face that holds that state is the one place both of
 * them can be reached from.
 *
 * **The half that is down is not drawn.** Each of them reads the disk while it is on the screen, and
 * one kept mounted behind the other would go on asking for a folder nobody is looking at — which is
 * what the two halves exist to stop paying for (`AMB-T-4899`).
 */
export function FolderRail({ project, picker, tab, onTab, folders, git }: {
  /** The project being shown, or nothing while the face has not been told which one it is on. */
  project: Project | null;
  /** Which folder the window is on, drawn under the name (`../files/RootPick`). Nothing where the
   *  project is bound to one folder, which is the shape all but one project is in. */
  picker?: ReactNode;
  /** Which half is up. The face holds it, because what the column is drawn at is held there and the
   *  two halves are worth different widths (`../talk/columns`). */
  tab: RailTab;
  onTab: (which: RailTab) => void;
  folders: ReactNode;
  git: ReactNode;
}) {
  return (
    <nav className="rail" aria-label={t("face.railFolders")}>
      {/* Drawn even with no project to name, so what is below does not walk up the column for the
          moment the face has not been told which project it is on. */}
      <div className="rail__head">
        <h2 className="rail__title">{project?.name ?? ""}</h2>
        {picker}
        {/* Two of them, so they are drawn as the pair they are rather than as one control that
            says where it goes: a reader has to see the half they are not on to know it is there. */}
        <div className="rail__tabs" role="tablist">
          <RailTabButton on={tab} which="files" onTab={onTab} says={t("git.tabFiles")} />
          <RailTabButton on={tab} which="git" onTab={onTab} says={t("git.tabGit")} />
        </div>
      </div>
      {tab === "files" ? folders : git}
    </nav>
  );
}

/** One of the two, drawn as what it is: a tab, which says whether it is the one that is on. */
function RailTabButton({ on, which, onTab, says }: {
  on: RailTab;
  which: RailTab;
  onTab: (which: RailTab) => void;
  says: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={on === which}
      className={`rail__tab${on === which ? " rail__tab--on" : ""}`}
      onClick={() => onTab(which)}
    >
      {says}
    </button>
  );
}
