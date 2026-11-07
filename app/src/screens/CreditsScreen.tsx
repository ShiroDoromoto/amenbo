// Credits: Amenbo's own license and the third-party notices, read from the two files the app bundles
// beside itself (`LICENSE`, `THIRD-PARTY-NOTICES`). The texts are shown as written — in English, and
// never rebuilt here — so what the screen says is what ships in the bundle.
//
// `THIRD-PARTY-NOTICES` is about 2 MB. Drawing it as one block would lay out every line at once, so it is
// cut along the headings the generator writes and each part is drawn only once it is opened.
import { useEffect, useState } from "react";
import { errText, t } from "../core/i18n";
import { fetchLicenses } from "../core/mutations";
import type { LicensesDto } from "../bindings/bindings";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";

/** The line `scripts/gen-third-party-notices.mjs` draws above and below a section's title. */
const SECTION_RULE = "=".repeat(78);
/** The line it draws under one entry's title inside a section. */
const ENTRY_RULE = "-".repeat(78);

export type NoticeEntry = { title: string; text: string };
export type NoticeSection = { title: string; intro: string; entries: NoticeEntry[] };
export type Notices = { preamble: string; sections: NoticeSection[] };

/** Join lines back into text, without the blank lines at either end. */
function block(lines: string[]): string {
  return lines.join("\n").replace(/^\s*\n/, "").trimEnd();
}

/** The prefix the generator puts on the section that holds the license texts for a list section. */
const TEXTS_PREFIX = "License texts — ";
/** The prefix on the title of one license text, naming who uses it. */
const ENTRY_PREFIX = "Used by: ";

/**
 * Cut `THIRD-PARTY-NOTICES` along its headings: a section is a title between two `=` rules, and an entry
 * inside it is a title over a `-` rule. Whatever comes before the first heading of either kind is kept as
 * the preamble or the section's intro, so no line of the file is dropped.
 *
 * A license text can carry a heading of the same shape inside it (LLVM's does), so not every such
 * heading is the generator's. The generator writes the first section on its own, and every later one as
 * a pair — a list `T` and its `License texts — T` — so a heading counts only when it is the first or has
 * its partner in the file. The others stay in the text of the entry they sit in.
 */
export function splitNotices(text: string): Notices {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const found: number[] = [];
  for (let i = 0; i + 2 < lines.length; i++) {
    if (lines[i] === SECTION_RULE && lines[i + 2] === SECTION_RULE) found.push(i);
  }
  const headTitles = new Set(found.map((at) => lines[at + 1]));
  const paired = (title: string) =>
    title.startsWith(TEXTS_PREFIX) ? headTitles.has(title.slice(TEXTS_PREFIX.length)) : headTitles.has(TEXTS_PREFIX + title);
  const heads = found.filter((at, n) => n === 0 || paired(lines[at + 1]));
  const sections = heads.map((at, n) => {
    const body = lines.slice(at + 3, n + 1 < heads.length ? heads[n + 1] : lines.length);
    // A license text can draw a `-` rule of its own too. In a license-text section the generator heads
    // each entry `Used by: …`, so only those are taken as entries there.
    const isEntry = lines[at + 1].startsWith(TEXTS_PREFIX)
      ? (line: string) => line.startsWith(ENTRY_PREFIX)
      : (line: string) => line !== "";
    const titles: number[] = [];
    for (let j = 0; j + 1 < body.length; j++) {
      if (body[j + 1] === ENTRY_RULE && isEntry(body[j])) titles.push(j);
    }
    const entries = titles.map((tAt, k) => ({
      title: body[tAt],
      text: block(body.slice(tAt + 2, k + 1 < titles.length ? titles[k + 1] : body.length)),
    }));
    return { title: lines[at + 1], intro: block(body.slice(0, titles[0] ?? body.length)), entries };
  });
  return { preamble: block(lines.slice(0, heads[0] ?? lines.length)), sections };
}

export function CreditsScreen({ onBack }: { onBack: () => void }) {
  const [licenses, setLicenses] = useState<LicensesDto | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    fetchLicenses().then(setLicenses).catch((e) => setError(errText(e)));
  }, []);

  return (
    <>
      <div className="board__toolbar">
        <span className="board__title">
          <Icon name="document" size="md" /> {t("credits.title")}
        </span>
        <div className="topbar__spacer" />
        <button className="btn" onClick={onBack}><Icon name="chevronLeft" /> {t("credits.back")}</button>
      </div>

      <div className="settings">
        {error ? (
          <ErrorNote>{error}</ErrorNote>
        ) : licenses === undefined ? (
          <span className="meta">…</span>
        ) : licenses === null ? (
          <span className="meta">{t("credits.unavailable")}</span>
        ) : (
          <CreditsBody licenses={licenses} />
        )}
      </div>
    </>
  );
}

function CreditsBody({ licenses }: { licenses: LicensesDto }) {
  const notices = splitNotices(licenses.thirdPartyNotices);
  return (
    <>
      <div className="settings__section">
        <div className="settings__h">{t("credits.amenbo")}</div>
        <div className="settings__body">
          <Fold title="LICENSE (Apache-2.0)">
            <NoticeText text={licenses.license} />
          </Fold>
        </div>
      </div>

      <div className="settings__section">
        <div className="settings__h">{t("credits.thirdParty")}</div>
        <div className="settings__body">
          <span className="meta">{t("credits.thirdPartyNote")}</span>
          {notices.preamble && <NoticeText text={notices.preamble} />}
          {notices.sections.map((section) => (
            <Fold key={section.title} title={section.title}>
              {section.intro && <NoticeText text={section.intro} />}
              {section.entries.map((entry, i) => (
                <Fold key={i} title={entry.title}>
                  <NoticeText text={entry.text} />
                </Fold>
              ))}
            </Fold>
          ))}
        </div>
      </div>
    </>
  );
}

/** A title that opens onto what is under it. What is under it is not drawn until then. */
function Fold({ title, children }: { title: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="credits__fold">
      <button type="button" className="credits__toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Icon name={open ? "chevronDown" : "chevronRight"} /> {title}
      </button>
      {open && <div className="credits__inner">{children}</div>}
    </div>
  );
}

function NoticeText({ text }: { text: string }) {
  return <pre className="credits__text">{text}</pre>;
}
