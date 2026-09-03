import { panesOf, type Layout } from "../talk/layout";
import { inkOn, initialOf } from "./projectMark";
import type { Project } from "../mock/types";
import { Icon } from "../components/Icon";
import { t } from "../core/i18n";

/**
 * The projects, down the edge of the face, as the tabs the whole screen is switched with
 * (`AMB-D-838`).
 *
 * **A project is the container and not a row inside one.** Panes belong to a project and folders are
 * bound to it, so a list of projects held inside one half of one of the columns beside the panes had
 * the top of the hierarchy drawn under the bottom of it. Here it is what the face is stood on: the
 * column is at the edge, everything else is inside it, and moving is one press.
 *
 * **It cannot be put away.** The two columns beside the panes each carry a way to close them, because
 * each is taking width from the thing the face is for. This one does not, and that is the whole of
 * what it buys: a turn standing in a project nobody is looking at is knocked about by a dot on its
 * tab, and a column that could be closed would be a way to stop being told (`AMB-T-3610`).
 *
 * **Compact is where the names go, not the tabs.** The tabs stay whatever happens; what folds away is
 * the width the names take, leaving the colour a person gave the project and the first character of
 * what they called it (`./projectMark`). It is kept for the device (`../talk/columns`) — it is how
 * somebody likes to work rather than something one project's work wants — and the control for it sits
 * under the tabs rather than in the bar over the face, because it is about this column alone and is
 * pressed once and left.
 *
 * **The tabs scroll and the control does not.** A machine with a project for every folder it has ever
 * opened must not push the way back to the names off the bottom of the screen. What scrolls past the
 * top is a dot a reader cannot see, which is what the list of projects in the rail did before this
 * and is the price of the column being one press wide.
 */
export function ProjectTabs({
  layout, projects, needy, compact, onCompact, onProject,
}: {
  /** Which project is being shown, and which panes are in each — the dots are read off it. */
  layout: Layout;
  /** The projects this machine knows, in the order the ledger keeps them. */
  projects: readonly Project[];
  /** The frames a turn is standing in. What is drawn from it here is the dot on a project that is
   *  not the one being shown — the panes of the one that is say it for themselves. */
  needy: ReadonlySet<string>;
  compact: boolean;
  onCompact: (compact: boolean) => void;
  onProject: (project: number) => void;
}) {
  const fold = t(compact ? "face.tabsNamed" : "face.tabsCompact");

  return (
    <nav className={`ptabs${compact ? " ptabs--compact" : ""}`} aria-label={t("face.projects")}>
      <div className="ptabs__list">
        {projects.map((project) => {
          const shown = layout.project === project.id;
          // A project with no colour of its own has no ink either: the mark falls back to the face's
          // surface and its own text colour, which is readable in both themes.
          const ink = project.color ? inkOn(project.color) : null;
          return (
            <button
              key={project.id}
              className={`ptabs__tab${shown ? " ptabs__tab--on" : ""}`}
              // Going to a project, the way the row of pages goes to a page: the one on the screen is
              // where the reader already is.
              aria-current={shown ? "page" : undefined}
              // The name is said whether or not it is drawn: compact, the mark is the only thing on
              // the tab, and a colour is not something a reader can be asked to read out.
              aria-label={project.name}
              title={project.name}
              onClick={() => onProject(project.id)}
            >
              <span
                className="ptabs__mark"
                style={{ background: project.color, ...(ink === null ? {} : { color: ink }) }}
                aria-hidden="true"
              >
                {initialOf(project.name)}
              </span>
              {!compact && <span className="ptabs__name">{project.name}</span>}
              {/* Only for a project the reader is not looking at: the panes of the one they are each
                  say whose turn it is for themselves (`../talk/nameplate`). */}
              {!shown && panesOf(layout, project.id).some((pane) => needy.has(pane.id)) && (
                <span className="ptabs__needs" title={t("face.needsYou")} aria-hidden="true" />
              )}
            </button>
          );
        })}
      </div>
      {/* Which way the arrow points is which way the column goes, and the words say the same thing:
          the control is small and is drawn where a reader is not looking, so what it does must be
          readable without pressing it to find out. */}
      <button className="ptabs__fold" aria-label={fold} title={fold} onClick={() => onCompact(!compact)}>
        <Icon name={compact ? "chevronRight" : "chevronLeft"} />
      </button>
    </nav>
  );
}
