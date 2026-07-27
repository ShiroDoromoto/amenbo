// The onboarding screen is help and reference. Creating a project navigates to a GUI screen (NewProjectScreen);
// opening one (bind) has no GUI route, so the screen hands over the CLI command instead.
import { useState } from "react";
import { useCliCommandName } from "../core/cliCommand";
import { t } from "../core/i18n";
import type { Nav } from "../shell/AppShell";

export function OnboardingScreen({ onNav }: { onNav: (nav: Nav) => void }) {
  const cli = useCliCommandName();
  return (
    <div className="onboard">
      <div className="onboard__hero">
        <div className="placeholder__big">🪿</div>
        <h2>{t("onboard.welcome")}</h2>
        <p className="muted">{t("onboard.tagline")}</p>
        <div className="onboard__actions">
          <NavCard
            icon="🆕"
            label={t("onboard.createLabel")}
            hint={t("onboard.createHint")}
            go={t("onboard.createGo")}
            onClick={() => onNav({ type: "view", id: "newProject" })}
          />
          <CliCard
            icon="📂"
            label={t("onboard.openLabel")}
            hint={t("onboard.openHint")}
            cmd={`${cli} bind --project <${t("onboard.projectIdPh")}>`}
          />
        </div>
      </div>

      <div className="onboard__steps">
        <div className="onboard__stepshead">
          <h3 className="onboard__stepstitle">{t("onboard.stepsTitle")}</h3>
          <p className="muted">{t("onboard.stepsIntro")}</p>
        </div>
        {steps(cli).map((s, i) => (
          <Step key={s.cmd} n={i + 1} title={s.title} cmd={s.cmd}>
            {s.body}
          </Step>
        ))}
      </div>
    </div>
  );
}

/** The reference steps, worded for the CLI this build installs (`cli`). */
function steps(cli: string): { title: string; cmd: string; body: React.ReactNode }[] {
  return [
    {
      title: t("onboard.s1title"),
      cmd: `${cli} init`,
      body: <>{t("onboard.s1a")}<code>.amenbo</code>{t("onboard.s1b")}<code>AGENTS.md</code>{t("onboard.s1c")}</>,
    },
    {
      title: t("onboard.s2title"),
      cmd: "cat AGENTS.md",
      body: <><code>AGENTS.md</code>{t("onboard.s2a")}<code>{`${cli} agent --json`}</code>{t("onboard.s2b")}</>,
    },
    // The asking step hands over the very request the first loop copies (`FirstLoop`), so a reader
    // who meets both is taught one wording and not two.
    { title: t("onboard.s4title"), cmd: t("firstloop.prompt"), body: t("onboard.s4body") },
  ];
}

/** The create card: the primary action, which navigates to a GUI screen (NewProjectScreen) — so it shows no command. */
function NavCard({ icon, label, hint, go, onClick }: { icon: string; label: string; hint: string; go: string; onClick: () => void }) {
  return (
    <button className="onboard__action onboard__action--primary" onClick={onClick}>
      <span className="onboard__action-icon">{icon}</span>
      <span className="onboard__action-body">
        <span className="onboard__action-label">{label}</span>
        <span className="onboard__action-hint">{hint}</span>
        <span className="onboard__go">{go} →</span>
      </span>
    </button>
  );
}

/** The CLI card: for an action with no GUI route (opening a project), a click copies the command and points the user at a terminal. */
function CliCard({ icon, label, hint, cmd }: { icon: string; label: string; hint: string; cmd: string }) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(cmd);
      setError(false);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setError(true);
    }
  };
  return (
    <button className="onboard__action" onClick={copy}>
      <span className="onboard__action-icon">{icon}</span>
      <span className="onboard__action-body">
        <span className="onboard__action-label">
          {label}
          <span className="onboard__cli-tag">{t("onboard.cliTag")}</span>
        </span>
        <span className="onboard__action-hint">{copied ? t("onboard.copied") : error ? `${t("onboard.manualCopy")}: ${cmd}` : hint}</span>
        <code className="onboard__cmd">{cmd}</code>
      </span>
    </button>
  );
}

function Step({ n, title, cmd, children }: { n: number; title: string; cmd: string; children: React.ReactNode }) {
  return (
    <div className="onboard__step">
      <div className="onboard__num">{n}</div>
      <div className="onboard__stepbody">
        <div className="onboard__steptitle">{title}</div>
        <div className="muted">{children}</div>
        <code className="onboard__cmd">{cmd}</code>
      </div>
    </div>
  );
}
