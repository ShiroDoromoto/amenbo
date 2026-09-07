import { type Layout } from "../talk/layout";
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
 * each is taking width from the thing the face is for. This one does not: a project is what every
 * other column here is about, so closing it would leave a reader with no way to say which project
 * they are in.
 *
 * **Compact is where the names go, not the tabs.** The tabs stay whatever happens; what folds away is
 * the width the names take, leaving the mark: the image the project was given, or the colour a person
 * gave it and the first character of what they called it (`./projectMark`). It is kept for the device
 * (`../talk/columns`) — it is how somebody likes to work rather than something one project's work
 * wants — and the control for it sits under the tabs rather than in the bar over the face, because it
 * is about this column alone and is pressed once and left.
 *
 * **An image, where the project has one.** A person can register one in the project's settings
 * (`AMB-D-838`, `AMB-D-839`), and it stands in the mark's place rather than beside it: the mark is the
 * one thing on a compact tab, and a picture next to a letter for the same project would be saying it
 * twice in the width there is for saying it once. Nothing is registered for most projects and nothing
 * has to be — the colour and the letter are what they keep.
 *
 * **The tabs scroll and the control does not.** A machine with a project for every folder it has ever
 * opened must not push the way back to the names off the bottom of the screen. What scrolls past the
 * top is a dot a reader cannot see, which is what the list of projects in the rail did before this
 * and is the price of the column being one press wide.
 */
export function ProjectTabs({
  layout, projects, compact, onCompact, onProject,
}: {
  /** Which project is being shown, and which panes are in each. */
  layout: Layout;
  /** The projects this machine knows, in the order the ledger keeps them. */
  projects: readonly Project[];
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
          // The image a person registered for the project, where there is one. It fills the mark, so
          // the colour is left off underneath it: a picture with somebody's colour showing through its
          // corners is the colour looking like part of the picture.
          const icon = project.icon;
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
                style={icon === null ? { background: project.color, ...(ink === null ? {} : { color: ink }) } : undefined}
                aria-hidden="true"
              >
                {icon === null ? initialOf(project.name) : <img className="ptabs__icon" src={icon} alt="" />}
              </span>
              {!compact && <span className="ptabs__name">{project.name}</span>}
            </button>
          );
        })}
      </div>
      {/* Which way the arrow points is which way the column goes, and the words say the same thing:
          the control is small and is drawn where a reader is not looking, so what it does must be
          readable without pressing it to find out. The mark is the arrow run into the window's edge
          rather than a chevron (`AMB-D-848`) — a chevron is the disclosure pair and says a section is
          folded, where what this does is move a column between two widths. */}
      <button className="ptabs__fold" aria-label={fold} title={fold} onClick={() => onCompact(!compact)}>
        <Icon name={compact ? "foldRight" : "foldLeft"} />
      </button>
    </nav>
  );
}
