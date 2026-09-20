// The two answers that decide what the application looks like, drawn together because one of them
// can close the other.
//
// A skin may be written for one side only, and while such a skin is on there is no theme to choose:
// the side it was not written for would fall back to the base colours the moment it was shown, which
// reads as a fault rather than as a skin. So the theme row is pinned to the declared side and says
// why, with the way out — taking the skin off — beside it rather than somewhere else.
//
// A skin is tried on before it is worn. Touching a name puts its colours on the frame below and
// nothing else, so a set nobody can read stays inside the box it is being read in — the screen that
// would be used to choose a different one keeps the colours it had.
import { useEffect, useRef, useState } from "react";

import type { SkinRowDto } from "../bindings/bindings";
import { pickSaveAs } from "../core/dialog";
import { t, tf } from "../core/i18n";
import { fitOnto, listSkins, pictureSlots, skinFontLicence, skinTables, skinTitle, useSkin, watchSkinChanged, writeSkinOut } from "../core/skin";
import { getThemePref, setThemePref, type ThemePref } from "../core/theme";
import { SkinAdd } from "./SkinAdd";

/** The side a one-sided skin pins the theme to, or nothing where the skin has both (or none is on). */
function pinnedSide(row: SkinRowDto | undefined): "light" | "dark" | undefined {
  if (!row || row.themes.length !== 1) return undefined;
  const only = row.themes[0];
  return only === "light" || only === "dark" ? only : undefined;
}

export function AppearanceSettings() {
  const [theme, setTheme] = useState<ThemePref>(getThemePref);
  const [rows, setRows] = useState<SkinRowDto[]>([]);
  const [on, setOn] = useState<string | null>(null);
  // The name being tried on, and the values it puts on the frame. `null` is nothing being tried.
  const [fitting, setFitting] = useState<{ name: string | null; title: string } | null>(null);
  const frame = useRef<HTMLDivElement>(null);
  // The licence of the shown skin's font, once the reader has asked to see it. A document rather
  // than a line, so it is fetched on asking and not on every listing.
  const [licence, setLicence] = useState<string | null>(null);

  // A window with no host to ask — a browser `npm run dev`, a test — holds no skins and wears none,
  // which is the screen the built-in colours are already on. Every road out of here says so the same
  // way: the answer to "cannot ask" is the answer to "nothing is held".
  const reload = () => {
    void listSkins()
      .then((held) => {
        setRows(held.skins);
        setOn(held.on);
      })
      .catch(() => {});
  };
  useEffect(reload, []);
  // And again whenever it is changed from outside this screen — another window, or the menu bar's
  // way out, whose click is answered by the host and never passes through here.
  useEffect(() => {
    let stop: (() => void) | undefined;
    void watchSkinChanged(reload).then((off) => {
      stop = off;
    });
    return () => stop?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The skin the select is showing: the one being tried on while a fitting is up, and the one that
  // is on otherwise. What is said under the select is about that one.
  const shownName = fitting ? fitting.name : on;
  const shown = rows.find((r) => r.name === shownName);

  /** Hand the shown skin's own file on, under the name it is kept under. */
  const writeOut = (name: string, suggested: string) => {
    void pickSaveAs(suggested)
      .then((at) => (at ? writeSkinOut(name, at) : undefined))
      .catch(() => {});
  };
  // The skin that is on: the one the theme row names, and the one it reads the declared side off.
  const onRow = rows.find((r) => r.name === on);
  // What the select says about the theme while a one-sided skin is on. Applying it is not this
  // screen's — `applySkin` holds the appearance to that side wherever a skin is worn, which is
  // every window and every startup, not only here.
  const pinned = pinnedSide(onRow);

  const tryOn = (name: string | null, title: string) => {
    setFitting({ name, title });
    setLicence(null);
    if (name === null) {
      fitOnto(frame.current, null);
      return;
    }
    void skinTables(name)
      .then((tables) => {
        // The frame shows the side the screen is on, which is the side the reader judges it by.
        const side = document.documentElement.dataset.theme === "dark" ? "dark" : "light";
        // The pictures are not a side's — the frame is a card, so the one it wears is the card's.
        fitOnto(frame.current, tables ? tables[side] : null, pictureSlots(tables));
      })
      .catch(() => fitOnto(frame.current, null));
  };

  const keep = () => {
    setFitting(null);
    fitOnto(frame.current, null);
  };

  const wear = () => {
    if (!fitting) return;
    void useSkin(fitting.name)
      .then(() => {
        keep();
        reload();
      })
      .catch(() => {});
  };

  return (
    <>
      <div className="settings__row">
        <span className="settings__k">{t("settings.skin")}</span>
        <span>
          <select
            className="btn"
            value={fitting ? (fitting.name ?? "") : (on ?? "")}
            onChange={(e) => {
              const name = e.target.value;
              const row = rows.find((r) => r.name === name);
              tryOn(name === "" ? null : name, row ? skinTitle(row) : t("settings.skinNone"));
            }}
          >
            <option value="">{t("settings.skinNone")}</option>
            {rows.map((r) => (
              <option key={r.name} value={r.name} disabled={r.error !== null}>
                {r.error === null ? skinTitle(r) : `${r.name} — ${t("settings.skinUnreadable")}`}
              </option>
            ))}
          </select>
          <div className="meta">{t("settings.skinHow")}</div>
          {/* Handing the shown skin on. Only for one this device keeps in a file — the four that
              ship inside the build are held in none, and what would be written out for them is
              this build's own values, which is what the template already writes. The file goes
              out as it came in, so what the next person opens is what the author sent. */}
          {shown?.fileName && (
            <div className="meta">
              <button className="btn" onClick={() => writeOut(shown.name, shown.fileName ?? "")}>
                {t("settings.skinWriteKept")}
              </button>{" "}
              {t("settings.skinWriteKeptNote")}
            </div>
          )}
          {/* Where the shown skin carries a face, where that face came from. OFL asks that the
              notice and the licence travel with the font and that a reader can see them, and the
              seeing is the screen's half — the travelling is the file's. */}
          {shown?.fontFamily && (
            <div className="meta">
              {tf("settings.skinFont", {
                family: shown.fontFamily,
                license: shown.fontLicense || "—",
              })}{" "}
              <button
                className="btn"
                onClick={() => {
                  if (licence !== null) return setLicence(null);
                  void skinFontLicence(shown.name).then((full) => setLicence(full ?? "")).catch(() => {});
                }}
              >
                {t("settings.skinFontLicence")}
              </button>
              {licence !== null && <pre className="skinlicence">{licence}</pre>}
            </div>
          )}
        </span>
      </div>

      {/* Taking one in sits under the list it lands in, so what was just added is the next thing
          read. */}
      <SkinAdd onAdded={reload} />

      {fitting && (
        <div className="settings__row">
          <span className="settings__k">{t("settings.skinTryOn")}</span>
          <span>
            {/* Everything drawn in the skin sits under this one element, which is what holds its
                values. The buttons below it are outside, so they are still legible when the colours
                being tried are not. */}
            <div className="skinfit" ref={frame}>
              <div className="skinfit__title">{fitting.title}</div>
              <div className="skinfit__body">{t("settings.skinTryOnSample")}</div>
              <div className="skinfit__marks">
                <span className="chip chip--accent">{fitting.name ?? t("settings.skinNone")}</span>
                <span className="skinfit__press">{t("settings.skinTryOn")}</span>
              </div>
            </div>
            {/* Outside the frame, so they are drawn in the colours that are on rather than in the
                ones being tried — the two presses that end a fitting cannot be the ones a set of
                colours makes unreadable. */}
            <div className="skinfit__answer">
              <button className="btn" onClick={wear}>{t("settings.skinApply")}</button>
              <button className="btn" onClick={keep}>{t("settings.skinKeep")}</button>
            </div>
          </span>
        </div>
      )}

      <div className="settings__row">
        <span className="settings__k">{t("settings.theme")}</span>
        <span>
          <select
            className="btn"
            value={pinned ?? theme}
            disabled={pinned !== undefined}
            onChange={(e) => {
              const pref = e.target.value as ThemePref;
              setThemePref(pref);
              setTheme(pref);
            }}
          >
            <option value="os">{t("settings.themeOs")}</option>
            <option value="dark">{t("settings.themeDark")}</option>
            <option value="light">{t("settings.themeLight")}</option>
          </select>
          {pinned && (
            <div className="meta">
              {tf("settings.skinThemePinned", {
                title: onRow ? skinTitle(onRow) : (on ?? ""),
                side: pinned === "dark" ? t("settings.themeDark") : t("settings.themeLight"),
              })}{" "}
              {/* Drawn in colours of its own rather than in tokens: this is the way out of a skin
                  that is already on, and a way out the skin can paint over is not one. */}
              <button className="skinesc" onClick={() => void useSkin(null).then(reload).catch(() => {})}>
                {t("settings.skinTakeOff")}
              </button>
            </div>
          )}
        </span>
      </div>
    </>
  );
}
