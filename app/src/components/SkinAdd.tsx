// Taking a skin in: what the file says about itself, what the check set aside, and the pairings
// nobody would be able to read — all of it before anything is written.
//
// **A short pairing does not stop it.** What that would stop is an author trying their own work in
// progress, and there is no index here this could be keeping anybody off. The numbers are shown and
// the press is still there.
//
// A file the check turns away never gets this far: the read fails, and what is shown is the
// sentence the terminal prints for the same file.
import { useEffect, useRef, useState } from "react";

import type { SkinJudgementDto } from "../bindings/bindings";
import { ErrorNote } from "./ErrorNote";
import { pickFiles, pickSaveAs } from "../core/dialog";
import { watchHostDrop } from "../core/hostDrop";
import { errText, t, tf } from "../core/i18n";
import { langEndonym, type Lang } from "../core/i18n/lang";
import { addSkinFile, readSkinFile, scriptsMissingFrom, skinTitle, writeSkinTemplate } from "../core/skin";

/** What a warning's kind is said as. The key names it; the row carries the name it is about. */
const WHY: Record<string, string> = {
  unknown: "settings.skinWarnUnknown",
  closed: "settings.skinWarnClosed",
  notText: "settings.skinWarnNotText",
};

export function SkinAdd({ onAdded }: { onAdded: () => void }) {
  // The file being read over, and what reading it gave. Both go when the reader answers.
  const [path, setPath] = useState<string | null>(null);
  const [read, setRead] = useState<SkinJudgementDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The languages the file's font has no glyphs for. Empty where it carries none, and where this
  // window could not put the question.
  const [missing, setMissing] = useState<string[]>([]);
  const well = useRef<HTMLDivElement>(null);

  const look = (at: string) => {
    setPath(at);
    setRead(null);
    setError(null);
    setMissing([]);
    readSkinFile(at)
      .then((judged) => {
        setRead(judged);
        // A face that carries only Latin makes a Japanese screen half pixels and half the
        // machine's own letters. Worth knowing before it is taken in rather than after.
        if (judged.font) void scriptsMissingFrom(judged.font).then(setMissing).catch(() => {});
      })
      .catch((e) => setError(errText(e)));
  };

  const forget = () => {
    setPath(null);
    setRead(null);
    setError(null);
    setMissing([]);
  };

  // A file dragged in from the desktop lands on the application rather than on the page, so the
  // well is named to the host's watcher rather than given `ondrop` (`../core/hostDrop`).
  useEffect(() => {
    let stop: (() => void) | undefined;
    void watchHostDrop({
      select: ".skinwell",
      over: (at) => well.current?.classList.toggle("skinwell--over", at.el !== null),
      leave: () => well.current?.classList.remove("skinwell--over"),
      drop: (_at, paths) => {
        well.current?.classList.remove("skinwell--over");
        if (paths[0]) look(paths[0]);
      },
    })
      .then((off) => {
        stop = off;
      })
      .catch(() => {});
    return () => stop?.();
  }, []);

  const take = () => {
    if (!path || !read) return;
    addSkinFile(path, read.held)
      .then(() => {
        forget();
        onAdded();
      })
      .catch((e) => setError(errText(e)));
  };

  // Writing one out to start from. The whole file is the host's to build and to write; this side
  // only says where, so nothing of it passes through the page.
  const writeOut = () => {
    setError(null);
    void pickSaveAs("my-skin.yaml")
      .then((at) => (at ? writeSkinTemplate(at) : undefined))
      .catch((e) => setError(errText(e)));
  };

  return (
    <div className="settings__row">
      <span className="settings__k">{t("settings.skinAdd")}</span>
      <span>
        <div className="skinwell" ref={well}>
          <button className="btn" onClick={() => void pickFiles().then((p) => p[0] && look(p[0]))}>
            {t("settings.skinAddPick")}
          </button>
          <span className="meta">{t("settings.skinAddDrop")}</span>
        </div>
        {/* The other way in is to write one. A reader with no file to be handed starts from the
            screen they have: every colour, with a line saying what each is for. */}
        <div className="skinwrite">
          <button className="btn" onClick={writeOut}>{t("settings.skinWriteOut")}</button>
          <span className="meta">{t("settings.skinWriteOutNote")}</span>
        </div>

        {error && <ErrorNote tone="quiet">{error}</ErrorNote>}

        {read && (
          <div className="skinread">
            <div className="skinread__name">
              {skinTitle(read)}
              {read.version && <span className="meta"> {read.version}</span>}
              {read.author && <span className="meta"> · {read.author}</span>}
              {read.themes.length === 1 && (
                <span className="chip chip--heed">
                  {tf("settings.skinOnlySide", { side: sideWord(read.themes[0]!) })}
                </span>
              )}
            </div>

            {read.held && (
              <div className="meta">
                {tf("settings.skinReplace", {
                  held: read.heldVersion ?? t("settings.skinNoVersion"),
                  coming: read.version ?? t("settings.skinNoVersion"),
                })}
              </div>
            )}

            {missing.length > 0 && (
              <div className="meta">
                {tf("settings.skinFontMissing", {
                  langs: missing.map((l) => langEndonym(l as Lang)).join("、"),
                })}
              </div>
            )}

            {read.warnings.length > 0 && (
              <div>
                <div className="meta">{t("settings.skinDropped")}</div>
                <ul className="skinread__list">
                  {read.warnings.map((w) => (
                    <li key={`${w.theme ?? ""}.${w.key}`}>
                      <code>{w.theme ? `${w.theme}.${w.key}` : w.key}</code> —{" "}
                      {/* The font's reason is the file's own, so it is said rather than named by
                          kind — one dropped font can be four different things. */}
                      {w.detail ?? t(WHY[w.kind] ?? "settings.skinWarnUnknown")}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* The numbers, which read the same in every language: a token name, a ground, a ratio
                and the floor it is under. What is said around them is the sentence above. */}
            <div className="meta">
              {read.short.length === 0
                ? tf("settings.skinContrastClear", { n: read.measured })
                : tf("settings.skinContrastShort", { n: read.short.length, m: read.measured })}
            </div>
            {read.short.length > 0 && (
              <ul className="skinread__list">
                {read.short.map((r) => (
                  <li key={`${r.theme}.${r.ink}.${r.ground}`}>
                    <code>{r.theme}: {r.ink} / {r.ground}</code> — {r.ratio.toFixed(2)} &lt; {r.floor}
                  </li>
                ))}
              </ul>
            )}
            {read.unread.length > 0 && (
              <ul className="skinread__list">
                {read.unread.map((u) => (
                  <li key={u}><code>{u}</code> — {t("settings.skinUnmeasured")}</li>
                ))}
              </ul>
            )}

            <div className="skinfit__answer">
              <button className="btn" onClick={take}>{t("settings.skinAddTake")}</button>
              <button className="btn" onClick={forget}>{t("settings.skinKeep")}</button>
            </div>
          </div>
        )}
      </span>
    </div>
  );
}

/** A side, in the reader's language, off the words the theme row already writes. */
function sideWord(side: string): string {
  return side === "dark" ? t("settings.themeDark") : t("settings.themeLight");
}
