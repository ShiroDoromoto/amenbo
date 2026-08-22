// The onboarding screen is help and reference. Both ways in are moves the GUI makes: creating a
// project navigates to a screen (NewProjectScreen), and opening one already on this device — picking
// it and linking a folder to it — happens here, through the call the project settings screen makes
// (`project_bind_folder`, what CLI `bind --project` does). The reference below describes that walk,
// so the screen holds one way of getting started rather than a button and a longer typed alternative.
import { useState } from "react";
import { useCliCommandName } from "../core/cliCommand";
import { NoCli } from "../components/NoCli";
import { errText, t, tf } from "../core/i18n";
import { bindFolder, pickFolder } from "../core/mutations";
import { inTauri } from "../core/snapshot";
import { ErrorNote } from "../components/ErrorNote";
import { dataAdapter } from "../mock/adapter";
import type { Project } from "../mock/types";
import type { Nav } from "../shell/AppShell";
import { Icon, type IconName } from "../components/Icon";

export function OnboardingScreen({ onNav }: { onNav: (nav: Nav) => void }) {
  // Linking needs both a project to link to and a folder picker, so the card is offered only where
  // there is one of each: in the browser there is no picker, and with no project there is nothing
  // to open — creating is then the only real move, and a dead second card would only ask to be tried.
  const projects = dataAdapter.listProjects();
  // The asking step hands over a request that names a command, so it has to be this build's own.
  const cli = useCliCommandName();
  return (
    <div className="onboard">
      <div className="onboard__hero">
        <Icon name="goose" size="lg" />
        <h2>{t("onboard.welcome")}</h2>
        <p className="muted">{t("onboard.tagline")}</p>
        <div className="onboard__actions">
          <NavCard
            icon="plus"
            label={t("onboard.createLabel")}
            hint={t("onboard.createHint")}
            go={t("onboard.createGo")}
            onClick={() => onNav({ type: "view", id: "newProject" })}
          />
          {inTauri() && projects.length > 0 && (
            <BindCard projects={projects} onBound={(id) => onNav({ type: "project", id: String(id) })} />
          )}
        </div>
      </div>

      <div className="onboard__steps">
        <div className="onboard__stepshead">
          <h3 className="onboard__stepstitle">{t("onboard.stepsTitle")}</h3>
          <p className="muted">{t("onboard.stepsIntro")}</p>
        </div>
        {steps(cli).map((s, i) => (
          <Step key={s.title} n={i + 1} title={s.title} cmd={s.cmd}>
            {s.body}
          </Step>
        ))}
      </div>
    </div>
  );
}

/**
 * The reference steps: the walk the two cards above start, from linking a folder to work on the board.
 * `cli` is the command name this build installs, which the asking step's request has to carry — and
 * `null` where it installs none the reader can run, which leaves that step with the reason instead.
 */
function steps(cli: string | null): { title: string; cmd?: string; body: React.ReactNode }[] {
  return [
    {
      title: t("onboard.s1title"),
      body: <>{t("onboard.s1a")}<code>.amenbo</code>{t("onboard.s1b")}<code>AGENTS.md</code>{t("onboard.s1c")}</>,
    },
    // The asking step hands over the very request the first loop copies (`FirstLoop`), so a reader
    // who meets both is taught one wording and not two.
    cli
      ? { title: t("onboard.s2title"), cmd: tf("firstloop.prompt", { cmd: cli }), body: t("onboard.s2body") }
      : { title: t("onboard.s2title"), body: <><NoCli />{t("onboard.s2body")}</> },
    { title: t("onboard.s3title"), body: t("onboard.s3body") },
  ];
}

/** The create card: the primary action, which navigates to a GUI screen (NewProjectScreen). */
function NavCard({ icon, label, hint, go, onClick }: { icon: IconName; label: string; hint: string; go: string; onClick: () => void }) {
  return (
    <button className="onboard__action onboard__action--primary" onClick={onClick}>
      <Icon name={icon} size="lg" />
      <span className="onboard__action-body">
        <span className="onboard__action-label">{label}</span>
        <span className="onboard__action-hint">{hint}</span>
        <span className="onboard__go">{go} <Icon name="chevronRight" /></span>
      </span>
    </button>
  );
}

/**
 * The open card: link a folder to a project already on this device. Closed it sits beside "create" as
 * one more card; opened it asks which project — a folder is bound to exactly one, and there can be
 * several — and then hands over the folder picker. What the binding leaves in the folder (the
 * `.amenbo` pointer and the AI guide) is what lets an AI started there operate that project, so the
 * walk ends on that project's board, where an empty one is already showing the first loop.
 */
function BindCard({ projects, onBound }: { projects: Project[]; onBound: (id: number) => void }) {
  const [open, setOpen] = useState(false);
  const [projectId, setProjectId] = useState(projects[0].id);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) {
    // The line saying where the card leads borrows the word the board's own no-folder warning
    // offers: both end in the same move, and one wording for it is one thing to learn.
    return (
      <button className="onboard__action" onClick={() => setOpen(true)}>
        <Icon name="folder" size="lg" />
        <span className="onboard__action-body">
          <span className="onboard__action-label">{t("onboard.openLabel")}</span>
          <span className="onboard__action-hint">{t("onboard.openHint")}</span>
          <span className="onboard__go">{t("noFolder.btn")} <Icon name="chevronRight" /></span>
        </span>
      </button>
    );
  }

  const link = async () => {
    setError(null);
    try {
      const dir = await pickFolder();
      if (!dir) return; // The picker was dismissed: not a failure, and nothing to say about it.
      setBusy(true);
      await bindFolder(projectId, dir);
      onBound(projectId);
    } catch (e) {
      // Folders already nested under an Amenbo-managed tree, and the like, are refused by Rust with a coded error.
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="onboard__action onboard__action--open">
      <Icon name="folder" size="lg" />
      <div className="onboard__action-body">
        <span className="onboard__action-label">{t("onboard.openLabel")}</span>
        <span className="onboard__action-hint">{t("onboard.openHint")}</span>
        <label className="onboard__field">
          <span className="onboard__action-hint">{t("detail.project")}</span>
          <select
            className="onboard__select"
            value={projectId}
            disabled={busy}
            onChange={(e) => setProjectId(Number(e.target.value))}
          >
            {projects.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </label>
        {error && <ErrorNote>{error}</ErrorNote>}
        <div className="onboard__actionrow">
          <button className="btn btn--primary" onClick={() => void link()} disabled={busy}>
            <Icon name="folder" /> {t("newproj.chooseFolder")}
          </button>
          <button className="btn" onClick={() => setOpen(false)} disabled={busy}>{t("newproj.cancel")}</button>
        </div>
      </div>
    </div>
  );
}

function Step({ n, title, cmd, children }: { n: number; title: string; cmd?: string; children: React.ReactNode }) {
  return (
    <div className="onboard__step">
      <div className="onboard__num">{n}</div>
      <div className="onboard__stepbody">
        <div className="onboard__steptitle">{title}</div>
        <div className="muted">{children}</div>
        {cmd && <code className="onboard__cmd">{cmd}</code>}
      </div>
    </div>
  );
}
