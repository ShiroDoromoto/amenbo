import type { ReactNode } from "react";
import type { Project } from "../mock/types";
import { t } from "../core/i18n";

/**
 * The column beside the panes: the folders of the project being shown, under its name
 * (`AMB-D-838`).
 *
 * **It holds one thing, so nothing here swaps.** A column this narrow has room for one list at a
 * time (`AMB-D-835`), and the other two things a person picks on this face are elsewhere: the
 * projects are the tabs at the edge (`./ProjectTabs`) and the panes are the middle of the screen.
 *
 * **The name at the top is whose folders these are.** A tree drawn without it says which folders are
 * bound but not what they are bound to, and this face has two things called by a project's name —
 * the tab that is on, and this. They agree because they are the same answer read twice: what the tab
 * chose is what the tree is rooted in.
 *
 * **The tree is handed in rather than mounted here** (`../files/FolderTree`): what a row opens is
 * drawn in the column on the other side of the panes, so the two sides answer to one state — and the
 * face that holds that state is the one place both of them can be reached from.
 */
export function FolderRail({ project, folders }: {
  /** The project being shown, or nothing while the face has not been told which one it is on. */
  project: Project | null;
  folders: ReactNode;
}) {
  return (
    <nav className="rail" aria-label={t("face.railFolders")}>
      {/* Drawn even with no project to name, so the tree below does not walk up the column for the
          moment the face has not been told which project it is on. */}
      <div className="rail__head">
        <h2 className="rail__title">{project?.name ?? ""}</h2>
      </div>
      {folders}
    </nav>
  );
}
