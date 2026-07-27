// The modal that gets first-run setup (language, then names, then theme) done in one flow. It is
// triggered by config.onboarded===false, regardless of whether there is any data, which also catches
// an existing user who never went through setup. The icon is only an identicon preview seeded from
// the human display name. The theme applies immediately (theme.ts). On completion saveOnboarding raises onboarded.
import { useState } from "react";
import { getSnapshot } from "../core/snapshot";
import { saveOnboarding } from "../core/mutations";
import { getThemePref, setThemePref, type ThemePref } from "../core/theme";
import { normalizeLang, t, type Lang } from "../core/i18n";
import { isEnterSubmit } from "../core/keys";

// During setup the UI language has to switch as it is picked, so labels are looked up in the language
// currently selected (snapshot.language is only settled on completion; the preview runs off the local
// lang). The default display names below (human / AI) follow the language, and count as "still the
// default": they are placeholders, the drafts start empty, and only what the user actually typed is
// written to config.human_name/ai_name.
const DEFAULT_HUMAN_NAMES = ["人間", "Human", "ローカルユーザー", "Local user"];
const DEFAULT_AI_NAMES = ["AI"];

function identiconColor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return `hsl(${h % 360} 55% 45%)`;
}

export function OnboardingSetup() {
  const snap = getSnapshot();
  const rosterHuman = snap.roster.find((a) => a.kind === "human")?.name ?? "";
  const rosterAi = snap.roster.find((a) => a.kind === "ai")?.name ?? "";

  const [step, setStep] = useState(0);
  const [lang, setLang] = useState<Lang>(normalizeLang(snap.language));
  const [humanName, setHumanName] = useState(DEFAULT_HUMAN_NAMES.includes(rosterHuman) ? "" : rosterHuman);
  const [aiName, setAiName] = useState(DEFAULT_AI_NAMES.includes(rosterAi) ? "" : rosterAi);
  const [theme, setTheme] = useState<ThemePref>(getThemePref);
  const [saving, setSaving] = useState(false);

  const changeTheme = (p: ThemePref) => { setThemePref(p); setTheme(p); };

  const finish = async (skip: boolean) => {
    setSaving(true);
    try {
      await saveOnboarding(
        skip ? null : lang,
        skip ? null : humanName.trim() || null,
        skip ? null : aiName.trim() || null,
      );
    } finally {
      setSaving(false);
    }
  };

  const seed = humanName || "amenbo";
  const initial = (humanName.trim()[0] ?? "🙂").toUpperCase();

  const steps = [
    <div key="lang" className="setup__step">
      <div className="setup__q">{t("setup.langQ", lang)}</div>
      <div className="setup__choices">
        {(["ja", "en"] as Lang[]).map((l) => (
          <button
            key={l}
            className={`setup__choice ${lang === l ? "setup__choice--on" : ""}`}
            onClick={() => setLang(l)}
          >
            {l === "ja" ? "日本語" : "English"}
          </button>
        ))}
      </div>
    </div>,
    <div key="name" className="setup__step">
      <div className="setup__q">{t("setup.nameQ", lang)}</div>
      <div className="setup__nameRow">
        <span className="setup__identicon" style={{ background: identiconColor(seed) }}>{initial}</span>
        <input
          className="setup__input"
          value={humanName}
          placeholder={t("setup.humanNamePh", lang)}
          aria-label={t("setup.humanNameLabel", lang)}
          autoFocus
          onChange={(e) => setHumanName(e.target.value)}
          onKeyDown={(e) => { if (isEnterSubmit(e)) setStep((s) => s + 1); }}
        />
      </div>
      <div className="setup__nameRow">
        <span className="setup__identicon" style={{ background: identiconColor(`${seed}-ai`) }}>🤖</span>
        <input
          className="setup__input"
          value={aiName}
          placeholder={t("setup.aiNamePh", lang)}
          aria-label={t("setup.aiNameLabel", lang)}
          onChange={(e) => setAiName(e.target.value)}
          onKeyDown={(e) => { if (isEnterSubmit(e)) setStep((s) => s + 1); }}
        />
      </div>
      <div className="setup__hint">{t("setup.nameHint", lang)}</div>
    </div>,
    <div key="theme" className="setup__step">
      <div className="setup__q">{t("setup.themeQ", lang)}</div>
      <div className="setup__choices">
        {(["os", "dark", "light"] as ThemePref[]).map((p) => (
          <button
            key={p}
            className={`setup__choice ${theme === p ? "setup__choice--on" : ""}`}
            onClick={() => changeTheme(p)}
          >
            {p === "os" ? t("settings.themeOs", lang) : p === "dark" ? t("settings.themeDark", lang) : t("settings.themeLight", lang)}
          </button>
        ))}
      </div>
    </div>,
  ];
  const last = step === steps.length - 1;

  return (
    <div className="setup__overlay">
      <div className="setup__modal">
        <div className="setup__hero">
          <div className="setup__goose">🪿</div>
          <h2>{t("setup.welcome", lang)}</h2>
          <p className="muted">{t("setup.tagline", lang)}</p>
        </div>

        {steps[step]}

        <div className="setup__dots">
          {steps.map((_, i) => (
            <span key={i} className={`setup__dot ${i === step ? "setup__dot--on" : ""}`} />
          ))}
        </div>

        <div className="setup__actions">
          <button className="feed__action" disabled={saving} onClick={() => finish(true)}>
            {t("setup.skip", lang)}
          </button>
          <div style={{ flex: 1 }} />
          {step > 0 && (
            <button className="btn" disabled={saving} onClick={() => setStep((s) => s - 1)}>
              {t("setup.back", lang)}
            </button>
          )}
          {!last ? (
            <button className="btn btn--primary" disabled={saving} onClick={() => setStep((s) => s + 1)}>
              {t("setup.next", lang)}
            </button>
          ) : (
            <button className="btn btn--primary" disabled={saving} onClick={() => finish(false)}>
              {t("setup.finish", lang)}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
