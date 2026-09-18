// Looking through the whole of one folder, without opening the files in it (`AMB-D-910`).
//
// **One folder, and it is the one the window is on.** Which folder that is belongs to the selector
// in the rail and to nothing else: a screen with a second way of saying it would put the window back
// to giving two answers, which is what `AMB-D-905` took away.
//
// **The answer is drawn as it arrives.** The first one percent of the hits is ready in 5-9 ms and
// the last of them a second later (`AMB-T-4917`), so a screen that waited for the whole answer would
// stand still through a search that was all but done at once. What arrives is a batch of files, and
// each batch is drawn as it lands.
//
// **A search is called off by the next one.** The host does that itself, in the window that asked
// (`crate::folder_search`), so nothing here has to remember to — what is here is the tag that says
// which search a batch belongs to, so the answer to the word before this one is dropped rather than
// drawn.
//
// **The tree and this do not draw the same files.** The tree draws what the repository ignores and
// says that it does (`AMB-D-786`); this leaves it out until the reader asks, because the two walks
// are 24,042 files against 177,752 and 148 MB against 1,652 MB. A file on the screen in the rail
// that this does not find is the one thing that reads as broken, so the screen says so.

import { useEffect, useMemo, useRef, useState } from "react";
import type {
  FolderReplaceFileDto, FolderReplacedDto, FolderSearchFileDto, FolderSearchLineDto,
} from "../bindings/bindings";
import { errText, t, tf } from "../core/i18n";
import { asTyped } from "../core/keys";
import type { OpenFile } from "./FilesPanel";
import {
  folderReplace, folderSearch, folderSearchStop, onFolderSearchDone, onFolderSearchFound,
} from "./folder";
import { ReplaceAsk } from "./ReplaceAsk";

/** How long after the last letter the folder is walked. */
const WAIT = 200;

/** The three switches, drawn and named the way the editor's own panel draws and names them
 *  (`./editorFind`) — one pair of switches in two places, not two that happen to look alike. */
const SWITCHES = [
  { of: "caseSensitive", mark: "Aa", says: "files.findCase" },
  { of: "regex", mark: ".*", says: "files.findRegex" },
  { of: "wholeWord", mark: "ab", says: "files.findWord" },
] as const;

/** What the reader has asked for, which is the whole of what a search is. */
type Asked = {
  query: string;
  caseSensitive: boolean;
  regex: boolean;
  wholeWord: boolean;
  ignored: boolean;
};

const NOTHING: Asked = {
  query: "",
  caseSensitive: false,
  regex: false,
  wholeWord: false,
  ignored: false,
};

/** Looking through one folder, and what was found in it. */
export function SearchPanel({ projectId, root, onOpen }: {
  projectId: number | null;
  /** The folder the window is on — the rail's answer, not this screen's (`AMB-D-905`). */
  root: string | null;
  /** Open one file at the line a hit is on. */
  onOpen: (at: OpenFile, line: number) => void;
}) {
  const [asked, setAsked] = useState<Asked>(NOTHING);
  const [files, setFiles] = useState<FolderSearchFileDto[]>([]);
  const [over, setOver] = useState<{ capped: boolean; hits: number; files: number } | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const field = useRef<HTMLInputElement | null>(null);

  /** Whether the replacing half is showing, and what it would write. */
  const [replacing, setReplacing] = useState(false);
  const [withText, setWithText] = useState("");
  /**
   * The hits the reader has taken out of the list, by their keys.
   *
   * **Taking one out is what keeps it from being written**, which is the only thing the list does
   * besides opening files — so it is a set of what is left out rather than a set of what is picked.
   * A reader who wants one file replaced takes the others out, and a reader who wants all of them
   * takes nothing out and presses.
   */
  const [dropped, setDropped] = useState<ReadonlySet<string>>(new Set());
  /** The question before a run that spans more than one file, or nothing. */
  const [asking, setAsking] = useState<{ hits: number; files: number } | null>(null);
  /** What the last run came to, or nothing before there has been one. */
  const [wrote, setWrote] = useState<FolderReplacedDto | null>(null);
  /** Why a whole run was refused — one file nothing may write to stops every file in it. */
  const [refusedWrite, setRefusedWrite] = useState<string | null>(null);
  /** Bumped after a replacement, so the same word is looked for again over what is there now. */
  const [round, setRound] = useState(0);

  // The screen is opened by a key and by nothing else, so a reader who pressed it is already typing.
  useEffect(() => { field.current?.focus(); }, []);

  /**
   * Which search is being drawn.
   *
   * **A ref and not state**, because it is read inside the listeners below: they are put on once and
   * would otherwise go on holding whichever count was current when they were.
   */
  const tag = useRef(0);

  useEffect(() => {
    let alive = true;
    const listening = Promise.all([
      onFolderSearchFound((found) => {
        if (!alive || found.tag !== tag.current) return;
        setFiles((was) => [...was, ...found.files]);
      }),
      onFolderSearchDone((done) => {
        if (!alive || done.tag !== tag.current) return;
        setRunning(false);
        setOver({ capped: done.capped, hits: done.hits, files: done.files });
      }),
    ]);
    return () => {
      alive = false;
      void listening.then((stops) => { for (const stop of stops) stop(); });
      // The reader has left the screen, and a walk of ten threads is still going.
      void folderSearchStop(tag.current);
    };
  }, []);

  useEffect(() => {
    // Put down before anything is asked, so a screen that has just been typed into is not drawing
    // the hits for the word before this one while it waits.
    setFiles([]);
    setOver(null);
    setRefused(null);
    if (projectId === null || root === null || asked.query === "") {
      setRunning(false);
      return;
    }
    let alive = true;
    const soon = window.setTimeout(() => {
      tag.current += 1;
      setRunning(true);
      void folderSearch(projectId, root, { ...asked, tag: tag.current })
        // A pattern the reader is still typing — `(` is half of something — is refused rather than
        // answered with nothing, which would read as "no such text in this folder".
        .catch((why: unknown) => {
          if (!alive) return;
          setRunning(false);
          setRefused(errText(why));
        });
    }, WAIT);
    return () => { alive = false; window.clearTimeout(soon); };
  }, [projectId, root, asked, round]);

  // Every hit is back in the list when the question changes: what was taken out was taken out of an
  // answer that is gone.
  //
  // **What a replacement came to is not cleared with them.** The run itself asks for the word again
  // (`round`), and a line saying what was written would be gone before it had been read — which is
  // exactly the moment the list stops holding the answer it is about.
  useEffect(() => { setDropped(new Set()); setWrote(null); setRefusedWrite(null); }, [asked]);
  useEffect(() => { setDropped(new Set()); }, [round]);

  /** What is left in the list — the whole of what a press would write. */
  const kept = useMemo(
    () => files
      .filter((one) => !dropped.has(one.path.join("/")))
      .map((one) => ({
        ...one,
        lines: one.lines.filter((line) => !dropped.has(hitKey(one.path, line.line))),
      }))
      .filter((one) => one.lines.length > 0),
    [files, dropped],
  );

  const drop = (key: string) => setDropped((was) => new Set(was).add(key));

  /** What is left, in the shape the host writes from. */
  const payload = (): FolderReplaceFileDto[] => kept.map((one) => ({
    path: one.path,
    seen: one.digest,
    at: one.lines.flatMap((line) => line.spans.map((span) => ({
      line: line.line,
      at: span.at,
      length: span.length,
    }))),
  }));

  const write = async () => {
    setAsking(null);
    if (projectId === null || root === null) return;
    setWrote(null);
    setRefusedWrite(null);
    try {
      // `asked` and not a copy of it: the hits in `payload()` came from this query, and the host
      // reads `$1` out of the match it finds at each of them. A query that had moved would be a
      // pattern that matches nothing there, and the file would be left alone.
      const done = await folderReplace(projectId, root, payload(), withText, asked);
      setWrote(done);
      // The files are not what they were, so what the list is drawing is not either.
      setRound((n) => n + 1);
    } catch (why: unknown) {
      setRefusedWrite(errText(why));
    }
  };

  const hits = kept.reduce((n, one) => n + one.lines.reduce((m, line) => m + line.spans.length, 0), 0);

  const change = (some: Partial<Asked>) => setAsked((was) => ({ ...was, ...some }));

  return (
    <div className="search">
      <div className="search__row">
        <input
          ref={field}
          className="search__field"
          aria-label={t("files.searchIn")}
          placeholder={t("files.searchIn")}
          value={asked.query}
          onChange={(e) => change({ query: e.target.value })}
          {...asTyped}
        />
        <button
          className="search__switch"
          type="button"
          title={replacing ? t("files.searchReplaceHide") : t("files.searchReplaceShow")}
          aria-label={replacing ? t("files.searchReplaceHide") : t("files.searchReplaceShow")}
          aria-expanded={replacing}
          onClick={() => setReplacing((was) => !was)}
        >
          {replacing ? "⌄" : "›"}
        </button>
        {SWITCHES.map(({ of, mark, says }) => (
          <button
            key={of}
            className="search__switch"
            type="button"
            title={t(says)}
            aria-label={t(says)}
            aria-pressed={asked[of]}
            onClick={() => change({ [of]: !asked[of] })}
          >
            {mark}
          </button>
        ))}
      </div>
      {/* Under the field, and drawn only once the reader has asked for it. What it writes is the
          files themselves, so it is not something to meet on the way to a search. */}
      {replacing && (
        <div className="search__row">
          <span className="search__under" />
          <input
            className="search__field"
            aria-label={t("files.searchReplaceWith")}
            placeholder={t("files.searchReplaceWith")}
            value={withText}
            onChange={(e) => setWithText(e.target.value)}
            {...asTyped}
          />
          <button
            className="search__does"
            type="button"
            disabled={hits === 0}
            onClick={() => {
              // **The question is asked where a press reaches more than one file**, and not at all
              // where it reaches one. A reader replacing inside one file can see what they are
              // about to do; over four of them they cannot (`AMB-T-4952` — the one product of four
              // that asks this way).
              if (kept.length >= 2) setAsking({ hits, files: kept.length });
              else void write();
            }}
          >
            {t("files.searchReplaceGo")}
          </button>
        </div>
      )}
      {/* Said where it is about to happen rather than in the question, because the question is not
          asked at all for one file. What takes a replacement back is git, and a folder outside one
          cannot take it back at all (`AMB-D-911`). */}
      {replacing && <p className="search__note">{t("files.searchNoUndo")}</p>}

      {/* The switch `AMB-D-910` puts on the screen, and the sentence that says why it is there: a
          file drawn in the tree and not found here is the one thing about this that reads as
          broken. */}
      <label className="search__ignored">
        <input
          type="checkbox"
          checked={asked.ignored}
          onChange={(e) => change({ ignored: e.target.checked })}
        />
        {t("files.searchIgnored")}
      </label>
      {!asked.ignored && <p className="search__note">{t("files.searchTreeNote")}</p>}

      {refused !== null && <p className="search__none">{refused}</p>}
      {refused === null && over !== null && over.hits === 0 && (
        <p className="search__none">{t("files.searchNone")}</p>
      )}
      {over !== null && over.hits > 0 && (
        <p className="search__count">
          {tf("files.searchFound", { hits: over.hits, files: files.length })}
        </p>
      )}
      {over?.capped === true && <p className="search__none">{t("files.searchCapped")}</p>}

      {refusedWrite !== null && <p className="search__none">{refusedWrite}</p>}
      {wrote !== null && (
        <>
          <p className="search__count">
            {tf("files.searchReplaced", {
              hits: wrote.done.reduce((n, one) => n + one.hits, 0),
              files: wrote.done.length,
            })}
          </p>
          {wrote.skipped.length > 0 && (
            <p className="search__none">
              {tf("files.searchLeftAlone", { files: wrote.skipped.length })}
            </p>
          )}
          {wrote.stopped !== undefined && wrote.stopped !== null && (
            <p className="search__none">
              {tf("files.searchStopped", {
                path: wrote.stopped.path.join("/"),
                reason: wrote.stopped.reason,
              })}
            </p>
          )}
        </>
      )}

      <ul className="search__files">
        {kept.map((one) => (
          <li key={one.path.join("/")} className="search__file">
            <div className="search__head">
              <p className="search__path" title={one.path.join("/")}>{one.path.join("/")}</p>
              {/* Drawn only while there is something a press would write. A list nobody is
                  replacing from is a list to read, and a way out of one row of it is a control
                  with nothing behind it. */}
              {replacing && (
                <button
                  className="search__drop"
                  type="button"
                  title={t("files.searchDrop")}
                  aria-label={t("files.searchDrop")}
                  onClick={() => drop(one.path.join("/"))}
                >
                  ✕
                </button>
              )}
            </div>
            <ul className="search__lines">
              {one.lines.map((line) => (
                <li key={`${line.line}:${line.from}`} className="search__linerow">
                  <button
                    className="search__hit"
                    type="button"
                    onClick={() => { if (root !== null) onOpen({ root, path: one.path }, line.line); }}
                  >
                    <span className="search__lineno">{line.line}</span>
                    <span className="search__text">{marked(line)}</span>
                  </button>
                  {replacing && (
                    <button
                      className="search__drop"
                      type="button"
                      title={t("files.searchDrop")}
                      aria-label={t("files.searchDrop")}
                      onClick={() => drop(hitKey(one.path, line.line))}
                    >
                      ✕
                    </button>
                  )}
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>
      {running && files.length === 0 && refused === null && (
        <p className="search__note">{t("files.searchLooking")}</p>
      )}
      {asking !== null && (
        <ReplaceAsk
          hits={asking.hits}
          files={asking.files}
          onGo={() => { void write(); }}
          onCancel={() => setAsking(null)}
        />
      )}
    </div>
  );
}

/** One hit's key: the file it is in and the line it is on. */
function hitKey(path: string[], line: number): string {
  return `${path.join("/")}\0${line}`;
}

/**
 * One line with its matches marked.
 *
 * **The columns are counted in UTF-16 code units and so is a string here**, which is why they can be
 * sliced with as they stand (`crate::folder_search`). They are counted from the start of the line
 * and the text may be a window of it, so where the window starts is taken off first.
 */
function marked(line: FolderSearchLineDto) {
  const out = [];
  let at = 0;
  for (const [n, span] of line.spans.entries()) {
    const from = span.at - line.from;
    const to = from + span.length;
    // A match that falls outside the piece of the line that was carried is one there is nothing to
    // mark here — the window holds the first of them and a long line may have more.
    if (to <= at || from < 0 || from > line.text.length) continue;
    if (from > at) out.push(line.text.slice(at, from));
    out.push(<mark key={n} className="search__mark">{line.text.slice(from, to)}</mark>);
    at = to;
  }
  out.push(line.text.slice(at));
  return out;
}
