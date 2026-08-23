// The file face: the right side of the terminal face, where what the agent in the pane is doing to
// the folder can be seen without leaving the window (`AMB-T-3602`).
//
// **It belongs to the project, not to the pane.** The rows are rooted at a folder the project is
// bound to, so switching panes does not move them — what changed in the repository is the same
// question whichever terminal is in front of it. The row that does follow the pane is the one above
// these two ("what was pointed at"), and it is `AMB-T-3603`'s to add.
//
// **What changed lately is asked, not watched.** This walks the folder when it goes up and again
// when the window is focused, which is what a person coming back from the editor wants and costs a
// walk. Being told as it happens is a watch, with a walk of its own to reconcile against, and that
// is `AMB-T-3604`'s.
//
// **What a file is, is the host's answer, not this side's guess.** A NUL in the head makes it
// binary and the first bytes make it a picture (`crate::folder`); the name decides only whether
// text is drawn as Markdown, which is a question about rendering rather than about what the file
// is.
import { useCallback, useEffect, useMemo, useState } from "react";
import type { FolderChangedDto } from "../bindings/bindings";
import { Markdown } from "../components/Markdown";
import { useBoundFolders } from "../core/boundFolders";
import { t, whenLabel } from "../core/i18n";
import { RefNavProvider, useRefNav } from "../core/refNav";
import { folderEntries, folderRead, folderRecent } from "./folder";

/** The names a file's text is drawn as Markdown under. The one thing here the name decides. */
const MARKDOWN = [".md", ".markdown"];

export function FilesPanel({ projectId, onOpenLedger }: {
  /** The project whose folder the face is rooted at; nothing is drawn without one. */
  projectId: number | null;
  /** Leave the terminal face for the ledger — what a reference in a file means when it is clicked. */
  onOpenLedger?: () => void;
}) {
  // `0` names no project, which is what the folder read then answers with: none. A window with no
  // project on it draws the invitation, the same as one whose project has no folder.
  const folders = useBoundFolders(projectId ?? 0);
  const root = folders.live[0]?.path ?? null;
  const [changed, setChanged] = useState<FolderChangedDto[]>([]);
  const [treeOpen, setTreeOpen] = useState(false);
  const [reading, setReading] = useState<string[] | null>(null);

  const refresh = useCallback(() => {
    if (projectId === null || root === null) return;
    void folderRecent(projectId, root).then(setChanged).catch(() => setChanged([]));
  }, [projectId, root]);

  useEffect(() => {
    refresh();
    // Coming back to the window is the moment the list is most likely to be stale: the walk happens
    // while the person was away in something else.
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);

  if (projectId === null || root === null) {
    // A read that has not come back draws nothing at all: a flash of "no folder" on a project that
    // has one reads as a broken binding (`core/boundFolders`).
    return folders.answered
      ? <div className="files files--empty">{t("files.noFolder")}</div>
      : <div className="files" />;
  }

  if (reading !== null) {
    return (
      <FileReader
        projectId={projectId}
        root={root}
        path={reading}
        onBack={() => setReading(null)}
        onOpenLedger={onOpenLedger}
      />
    );
  }

  return (
    <div className="files">
      <section className="files__row">
        <h3 className="files__head">{t("files.changed")}</h3>
        {changed.length === 0
          ? <p className="files__none">{t("files.nothingChanged")}</p>
          : (
            <ul className="files__list">
              {changed.map((one) => (
                <li key={one.path.join("/")}>
                  <button className="files__file" onClick={() => setReading(one.path)}>
                    <span className="files__name">{one.path[one.path.length - 1]}</span>
                    <span className="files__when">{whenLabel(one.modified)}</span>
                  </button>
                  <span className="files__where">{one.path.slice(0, -1).join("/")}</span>
                </li>
              ))}
            </ul>
          )}
      </section>
      <section className="files__row">
        <button
          className="files__head files__head--button"
          aria-expanded={treeOpen}
          onClick={() => setTreeOpen((open) => !open)}
        >
          {t("files.tree")}
        </button>
        {/* Folded until it is asked for, and each level asked for only when it is opened: a tree is
            not the point of this face, and an unfolded one would read the whole repository to draw
            a panel nobody was looking at. */}
        {treeOpen && (
          <Level projectId={projectId} root={root} path={[]} onRead={setReading} />
        )}
      </section>
    </div>
  );
}

/** One folder's worth of names, and whatever of it has been opened. */
function Level({ projectId, root, path, onRead }: {
  projectId: number;
  root: string;
  path: string[];
  onRead: (path: string[]) => void;
}) {
  const [names, setNames] = useState<{ name: string; isDir: boolean }[]>([]);
  const [open, setOpen] = useState<string[]>([]);

  useEffect(() => {
    let alive = true;
    void folderEntries(projectId, root, path)
      .then((rows) => { if (alive) setNames(rows); })
      .catch(() => { if (alive) setNames([]); });
    return () => { alive = false; };
    // `path` is rebuilt by the parent on every render, so the array itself is not what to watch.
  }, [projectId, root, path.join("/")]);

  return (
    <ul className="files__list files__list--tree">
      {names.map((one) => (
        <li key={one.name}>
          {one.isDir
            ? (
              <>
                <button
                  className="files__dir"
                  aria-expanded={open.includes(one.name)}
                  onClick={() => setOpen((was) =>
                    was.includes(one.name) ? was.filter((n) => n !== one.name) : [...was, one.name]
                  )}
                >
                  {one.name}
                </button>
                {open.includes(one.name) && (
                  <Level projectId={projectId} root={root} path={[...path, one.name]} onRead={onRead} />
                )}
              </>
            )
            : (
              <button className="files__file" onClick={() => onRead([...path, one.name])}>
                <span className="files__name">{one.name}</span>
              </button>
            )}
        </li>
      ))}
    </ul>
  );
}

/** One file, as far as a panel can show it. */
function FileReader({ projectId, root, path, onBack, onOpenLedger }: {
  projectId: number;
  root: string;
  path: string[];
  onBack: () => void;
  onOpenLedger?: () => void;
}) {
  const [file, setFile] = useState<{ text?: string; truncated: boolean; image?: { mime: string; base64: string } } | null>(null);
  const [failed, setFailed] = useState(false);
  const name = path[path.length - 1];

  useEffect(() => {
    let alive = true;
    setFile(null);
    setFailed(false);
    void folderRead(projectId, root, path)
      .then((one) => { if (alive) setFile(one); })
      .catch(() => { if (alive) setFailed(true); });
    return () => { alive = false; };
  }, [projectId, root, path.join("/")]);

  // A reference in a file is a live link or it is nothing at all (`AMB-D-747`). Selecting the task
  // happens on the other face, so following one from here has to leave this face — otherwise the
  // click lands on a pane the reader cannot see and reads as a link that does nothing.
  const outer = useRefNav();
  const nav = useMemo(() => ({
    selectTask: (id: number) => { onOpenLedger?.(); outer.selectTask?.(id); },
    selectDecision: (id: number | null) => { onOpenLedger?.(); outer.selectDecision?.(id); },
  }), [outer, onOpenLedger]);

  return (
    <div className="files files--reading">
      <div className="files__bar">
        <button className="files__back" onClick={onBack}>{t("files.back")}</button>
        <span className="files__name" title={path.join("/")}>{name}</span>
      </div>
      <div className="files__body">
        {failed && <p className="files__none">{t("files.unreadable")}</p>}
        {file?.image !== undefined && (
          <img className="files__image" alt={name} src={`data:${file.image.mime};base64,${file.image.base64}`} />
        )}
        {file?.text !== undefined && (
          MARKDOWN.some((ext) => name.toLowerCase().endsWith(ext))
            ? <RefNavProvider value={nav}><Markdown>{file.text}</Markdown></RefNavProvider>
            : <pre className="files__text">{file.text}</pre>
        )}
        {file !== null && file.text === undefined && file.image === undefined && (
          <p className="files__none">{t("files.notText")}</p>
        )}
        {file?.truncated === true && <p className="files__none">{t("files.cut")}</p>}
      </div>
    </div>
  );
}
