// The warning a project gets on its own board when there is no folder an AI could be started in
// (`AMB-D-533`). It is drawn whatever the board holds — the count of tasks is not part of the question,
// since a project carrying forty of them and no folder is exactly the one nothing else on the screen
// speaks about.
//
// **Why it warns rather than invites.** A binding is what an AI can reach (`AMB-D-222`), so a project
// with none is one no AI can read or write. What is left is a task list kept by hand, which is not what
// amenbo is for. Creation asks for a folder (`AMB-D-528`), so a project reaches this state by having its
// last one unbound — which is allowed — or by predating that rule.
//
// It carries the one move that ends it, and nothing else: what it is short of is a folder.
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";

export function LinkFolderNotice({ onLinkFolder }: { onLinkFolder: () => void }) {
  return (
    <div className="boardwarn" role="status">
      <div className="boardwarn__title"><Icon name="warning" /> {t("noFolder.title")}</div>
      <div className="boardwarn__what">{t("noFolder.hint")}</div>
      <div className="boardwarn__actions">
        <button className="btn" onClick={onLinkFolder}>📂 {t("noFolder.btn")}</button>
      </div>
    </div>
  );
}
