import { useState } from "react";
import type { BoundFolderDto } from "../bindings/bindings";
import { t } from "../core/i18n";

/**
 * Where in this project the pane about to be opened works.
 *
 * **It is asked before the pane is made, and only where there is something to ask.** A project bound
 * to one folder is not a question — the pane opens there and nothing is put on the screen — and a
 * question that has to be answered before a frame exists is one a person can walk away from without
 * leaving a box behind (`../talk/layout`).
 *
 * **The answers are the project's folders and nothing else.** A pane belongs to a project, so the
 * folders it can work in are the ones that project is bound to; a picker that could reach anywhere
 * would put a pane from another project on this screen, which is the one thing the rail promises
 * cannot happen. The only folder chosen from outside the list is a project's **first** one, where
 * there is no list yet: that press binds the folder to this project, and is the same one press the
 * first run has always been (`AMB-T-3606`).
 *
 * `note` is a refusal from the last attempt, kept under the question rather than in place of it: a
 * folder that could not be bound leaves the reader where they were, which is with one still to choose.
 */
export function FolderChoice({
  folders, onPick, onBind, note,
}: {
  /** The folders this project is bound to that are actually there. */
  folders: readonly BoundFolderDto[];
  onPick: (folder: string) => void;
  /** Bind this project's first folder — offered only where it has none. */
  onBind: () => void;
  note: string | null;
}) {
  // Pressed once. Both roads end in a pane being opened somewhere else on the face, and a second
  // press before that lands would open a second one.
  const [pressed, setPressed] = useState(false);

  return (
    <div className="slot slot--asking">
      <div className="agent__ask">
        <p className="agent__askTitle">
          {folders.length === 0 ? t("talk.folder") : t("face.whichFolder")}
        </p>
        {folders.length === 0
          ? (
            <button
              className="agent__choice"
              disabled={pressed}
              onClick={() => { setPressed(true); onBind(); }}
            >
              {t("talk.chooseFolder")}
            </button>
          )
          : folders.map((folder) => (
            <button
              key={folder.path}
              className="agent__choice"
              disabled={pressed}
              onClick={() => { setPressed(true); onPick(folder.path); }}
            >
              {folder.path}
            </button>
          ))}
        {note !== null && <p className="agent__failed">{note}</p>}
      </div>
    </div>
  );
}
