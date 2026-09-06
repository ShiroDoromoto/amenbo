import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { dataAdapter } from "../mock/adapter";
import { useInboxCount } from "../core/mailbox";
import { dueBadges, type DueCounts, type DueStep } from "../core/due";
import { useArchivedProjects, useDueCounts } from "../core/reads";
import { useStore } from "../store/store";
import { t } from "../core/i18n";
import { flowEdges } from "../core/edgeScroll";
import { draggedFar } from "../core/pointerDrag";
import { Icon, type IconName } from "../components/Icon";
import type { SmartView } from "../mock/types";
import type { Nav } from "./AppShell";
import { landing } from "./rowDrag";
import { inkOn, initialOf } from "./projectMark";

// Which icon each smart view is drawn with. The views arrive as ids alone, so the drawing
// is decided here rather than travelling with the data (`AMB-D-689`).
const VIEW_ICON: Record<string, IconName> = { inbox: "inbox", activity: "activity", due: "calendar" };

/** A project row's own id, off the markup — nothing else in the sidebar carries one. */
function rowId(row: HTMLElement): number | null {
  const id = Number(row.dataset.projectId);
  return Number.isFinite(id) && id !== 0 ? id : null;
}

/**
 * The left sidebar (smart views, projects, other, and the collapsed archive). Reordering a project calls
 * `moveProject`, and the new order arrives through the snapshot once the write is acked — there is no optimistic
 * state.
 *
 * **The reorder is a press and a move, not the webview's drag.** The app itself takes what is dropped on it, and
 * with that switch thrown an in-window HTML5 drag does not fire at all on macOS and Windows (`AMB-D-775`). So a
 * press becomes a drag only past `DRAG_SLOP` (`../core/pointerDrag`), a press meant as a reorder does not navigate, and the
 * click that follows one is swallowed. A row held against the top or the bottom of the list scrolls it, which the
 * webview's drag never did either (`../core/edgeScroll`).
 *
 * Where the row lands (before/after) is computed from the release's own clientY and the row under it, never from the
 * stored `dropHint`: the hint is drawn a frame at a time and can be pointing at a row the pointer has already left,
 * and trusting it would make the release a silent no-op. The hint stays what it is — the visual indicator.
 */
export function Sidebar({
  nav, onNav, compact, onCompact,
}: {
  nav: Nav;
  onNav: (n: Nav) => void;
  /** Whether the column is drawn at its narrow width — the marks and the icons, without the names. */
  compact: boolean;
  onCompact: (compact: boolean) => void;
}) {
  const store = useStore();
  const views = dataAdapter.smartViews();
  const projects = dataAdapter.listProjects();
  // The inbox badge counts the real mailbox set. Subscribing here also drives arrival detection (sound / OS notification).
  const inboxCount = useInboxCount();
  const due = useDueCounts();
  const archived = useArchivedProjects();
  const [archivedOpen, setArchivedOpen] = useState(false);
  const isActive = (n: Nav) => nav.type === n.type && nav.id === n.id;

  const [dragId, setDragId] = useState<number | null>(null);
  const [dropHint, setDropHint] = useState<{ id: number; pos: "before" | "after" } | null>(null);
  const canReorder = projects.length > 1;
  // The press in flight. It is a ref because a move fires up to 165 times a second (`AMB-T-3755`) and none of what
  // it carries is drawn — what is drawn is the two pieces of state above, and those move at most once a frame.
  const press = useRef<{ id: number; from: { x: number; y: number }; dragging: boolean } | null>(null);
  // Whether the click behind the release we just saw is the tail of a drag. A press that travelled is not a
  // navigation, and the click arrives anyway.
  const swallowClick = useRef(false);
  // The hit test is put off to the next frame: the pointer reports far more often than the screen redraws — 165
  // times a second on Windows against 58 on Linux (`AMB-T-3755`) — and a second test inside one frame is a
  // rectangle read that nothing can act on. What the frame tests is where the pointer is *now*, not where it was
  // when the frame was asked for, so the moves in between are folded rather than dropped.
  const pending = useRef<number | null>(null);
  const latest = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  // How to call off the frame loop that scrolls the list under a held row.
  const stopFlow = useRef<(() => void) | null>(null);

  const stopPress = () => {
    press.current = null;
    if (pending.current !== null) cancelAnimationFrame(pending.current);
    pending.current = null;
    stopFlow.current?.();
    stopFlow.current = null;
    setDragId(null);
    setDropHint(null);
  };

  // A press outliving the sidebar would leave a frame loop scrolling a list that is gone.
  useEffect(() => () => stopFlow.current?.(), []);

  // Where the row would land, from wherever the pointer is now. Run from the frame a move asked for, and again on
  // every frame the list scrolled under a hand that is holding still — the rows have moved, so the row under the
  // pointer is a different one even though the pointer is not.
  const retest = () => {
    const held = press.current;
    if (held?.dragging !== true) return;
    const to = landing(held.id, latest.current, "data-project-row", rowId);
    setDropHint((h) => (h?.id === to?.id && h?.pos === to?.side ? h : to && { id: to.id, pos: to.side }));
  };

  // Two fences a held row needs, and neither is optional (`AMB-T-3755`).
  //
  // A right-click during a drag opens the webview's own menu on all three systems, and macOS then delivers no
  // pointer event at all until it is dismissed — measured at 12.2 seconds, which reads as "it froze while I was
  // holding it". On Windows the menu's first item reloads the page outright.
  //
  // The other is the selection: dragging across rows selects their text on macOS and Linux. `user-select` is put on
  // the body rather than on the rows because a selection begun on a row runs on across everything under it.
  useEffect(() => {
    if (dragId === null) return;
    const block = (e: Event) => e.preventDefault();
    document.addEventListener("contextmenu", block, true);
    document.body.classList.add("dragging-row");
    return () => {
      document.removeEventListener("contextmenu", block, true);
      document.body.classList.remove("dragging-row");
    };
  }, [dragId]);

  const onRowPointerDown = (e: ReactPointerEvent<HTMLElement>, id: number) => {
    // The primary button alone. A right-click is the menu's, and a middle-click is nobody's here.
    if (e.button !== 0) return;
    press.current = { id, from: { x: e.clientX, y: e.clientY }, dragging: false };
    latest.current = { x: e.clientX, y: e.clientY };
    // Captured from the press rather than from the threshold: it is what keeps the move and the release coming to
    // this row after the pointer has left it, including outside the window entirely (`AMB-T-3755`).
    e.currentTarget.setPointerCapture?.(e.pointerId);
    // The list flows while a row is held against its top or bottom edge, and only once the press has become a
    // drag: a click is not a reason to move the list out from under itself (`../core/edgeScroll`). A second press
    // arriving on top of a first calls the first one off, rather than leaving its frame loop running unowned.
    stopFlow.current?.();
    stopFlow.current = flowEdges(() => (press.current?.dragging === true ? latest.current : null), retest);
  };

  const onRowPointerMove = (e: ReactPointerEvent<HTMLElement>) => {
    const held = press.current;
    if (held === null) return;
    latest.current = { x: e.clientX, y: e.clientY };
    if (!held.dragging) {
      if (!draggedFar(held.from, latest.current)) return;
      held.dragging = true;
      swallowClick.current = true;
      setDragId(held.id);
    }
    // Against the selection, on top of the body's `user-select`: either one is enough on its own, and together they
    // cover all three systems (`AMB-T-3755`).
    e.preventDefault();
    if (pending.current !== null) return;
    pending.current = requestAnimationFrame(() => {
      pending.current = null;
      retest();
    });
  };

  const onRowPointerUp = (e: ReactPointerEvent<HTMLElement>) => {
    const held = press.current;
    // Read off the release itself rather than off the hint, which is a frame behind it.
    const to = held?.dragging === true
      ? landing(held.id, { x: e.clientX, y: e.clientY }, "data-project-row", rowId)
      : null;
    stopPress();
    if (held !== null && to !== null) store.moveProject(held.id, to.side, to.id);
  };

  // A drag the system took away — an incoming call, a screen lock. Nothing is written: what was interrupted is not
  // a choice anybody made.
  const onRowPointerCancel = () => {
    if (press.current?.dragging === true) swallowClick.current = true;
    stopPress();
  };

  const onRowClick = (n: Nav) => {
    if (swallowClick.current) {
      swallowClick.current = false;
      return;
    }
    onNav(n);
  };

  const fold = t(compact ? "face.tabsNamed" : "face.tabsCompact");

  return (
    <div className={`sidebar${compact ? " sidebar--compact" : ""}`}>
      <div className="sidebar__list">
        <div className="sidebar__group">
          {!compact && <div className="sidebar__label">{t("side.smartViews")}</div>}
          {views.map((v) => {
            const n: Nav = { type: "view", id: v.id };
            const label = t(`smartview.${v.id}`);
            return (
              <button
                key={v.id}
                className={`navitem ${isActive(n) ? "navitem--active" : ""}`}
                aria-label={label}
                title={label}
                onClick={() => onNav(n)}
              >
                {VIEW_ICON[v.id] ? <Icon name={VIEW_ICON[v.id]} /> : null}
                {!compact && <span className="navitem__name">{label}</span>}
                <ViewBadges view={v} inboxCount={inboxCount} due={due} compact={compact} />
              </button>
            );
          })}
        </div>

        <div className="sidebar__group">
          {!compact && <div className="sidebar__label">{t("side.projects")}</div>}
          {projects.map((p) => {
            const n: Nav = { type: "project", id: String(p.id) };
            // The drop indicator: a line above or below the midline. The row being dragged is dimmed.
            const hint = dropHint && dropHint.id === p.id ? dropHint.pos : null;
            const cls = [
              "navitem",
              isActive(n) ? "navitem--active" : "",
              canReorder ? "navitem--reorderable" : "",
              dragId === p.id ? "navitem--dragging" : "",
              hint === "before" ? "navitem--drop-before" : "",
              hint === "after" ? "navitem--drop-after" : "",
            ].filter(Boolean).join(" ");
            const count = p.openCount + p.proposedDecisionCount;
            return (
              <button
                key={p.id}
                className={cls}
                // The name is said whether or not it is drawn: compact, the mark is the only thing on
                // the row, and a colour is not something a reader can be asked to read out.
                aria-label={p.name}
                title={p.name}
                onClick={() => onRowClick(n)}
                // What the hit test looks for, and what it reads the row's own id off. The pointer is somewhere in
                // the document rather than on a React element, so the answer has to be written into the markup.
                data-project-row=""
                data-project-id={p.id}
                onPointerDown={canReorder ? (e) => onRowPointerDown(e, p.id) : undefined}
                onPointerMove={canReorder ? onRowPointerMove : undefined}
                onPointerUp={canReorder ? onRowPointerUp : undefined}
                onPointerCancel={canReorder ? onRowPointerCancel : undefined}
              >
                <ProjectMark color={p.color} name={p.name} icon={p.icon} />
                {!compact && <span className="navitem__name">{p.name}</span>}
                {count ? <span className="navitem__count">{count}</span> : null}
              </button>
            );
          })}
          {(() => {
            const n: Nav = { type: "view", id: "newProject" };
            const label = t("side.newProject");
            return (
              <button
                className={`navitem navitem--muted ${isActive(n) ? "navitem--active" : ""}`}
                aria-label={label}
                title={label}
                onClick={() => onNav(n)}
              >
                <Icon name="plus" />
                {!compact && <span className="navitem__name">{label}</span>}
              </button>
            );
          })()}
        </div>

        {/* Plugins are a section of their own, not an item under "other" (`AMB-D-356`): finding one and
            managing what is installed are two surfaces, and this is where the second one joins. */}
        <div className="sidebar__group">
          {!compact && <div className="sidebar__label">{t("side.plugins")}</div>}
          {([
            { id: "plugins", icon: "puzzle", label: t("plugins.market") },
            { id: "pluginsInstalled", icon: "plug", label: t("plugins.installed") },
          ] as const).map((item) => {
            const n: Nav = { type: "view", id: item.id };
            return (
              <button
                key={item.id}
                className={`navitem ${isActive(n) ? "navitem--active" : ""}`}
                aria-label={item.label}
                title={item.label}
                onClick={() => onNav(n)}
              >
                <Icon name={item.icon} />
                {!compact && <span className="navitem__name">{item.label}</span>}
              </button>
            );
          })}
        </div>

        <div className="sidebar__group">
          {!compact && <div className="sidebar__label">{t("side.other")}</div>}
          {([
            // Search sits here rather than among the smart views: a smart view is a standing selection of
            // tasks, and this asks where a word is written across tasks and decisions both (`AMB-D-449`).
            { id: "search", icon: "search", label: t("nav.search") },
            // Connecting an AI is an app-level setting, not a project's (`AMB-D-681`), so it lives
            // here rather than folded into each project's own screen.
            { id: "mcp", icon: "link", label: t("nav.mcp") },
            { id: "settings", icon: "gear", label: t("nav.settings") },
            { id: "onboarding", icon: "goose", label: t("nav.onboarding") },
          ] as const).map((it) => {
            const n: Nav = { type: "view", id: it.id };
            return (
              <button
                key={it.id}
                className={`navitem ${isActive(n) ? "navitem--active" : ""}`}
                aria-label={it.label}
                title={it.label}
                onClick={() => onNav(n)}
              >
                <Icon name={it.icon} />
                {!compact && <span className="navitem__name">{it.label}</span>}
              </button>
            );
          })}
        </div>

        {archived.length > 0 && (
          <div className="sidebar__group">
            <button
              className="navitem navitem--muted"
              aria-expanded={archivedOpen}
              aria-label={t("side.archived")}
              title={t("side.archived")}
              onClick={() => setArchivedOpen((v) => !v)}
            >
              <Icon name={archivedOpen ? "chevronDown" : "chevronRight"} />
              {!compact && <span className="navitem__name">{t("side.archived")}</span>}
              <span className="navitem__count">{archived.length}</span>
            </button>
            {archivedOpen &&
              archived.map((p) => {
                const n: Nav = { type: "projectSettings", id: String(p.id) };
                return (
                  <button
                    key={p.id}
                    className={`navitem navitem--muted ${isActive(n) ? "navitem--active" : ""}`}
                    aria-label={p.name}
                    title={p.name}
                    onClick={() => onNav(n)}
                  >
                    <ProjectMark color={p.color} name={p.name} icon={null} />
                    {!compact && <span className="navitem__name">{p.name}</span>}
                  </button>
                );
              })}
          </div>
        )}
      </div>
      {/* The way between the two widths, at the foot of the column and outside what scrolls — the same
          control the terminal face's tabs carry, in the same place and with the same words
          (`AMB-D-848`, `./ProjectTabs`). It is in the column rather than in the bar over the window
          because it is about this column alone: a control up there had to say which of the two faces
          it moved before a reader could use it, and it never did. */}
      <button className="sidebar__fold" aria-label={fold} title={fold} onClick={() => onCompact(!compact)}>
        <Icon name={compact ? "foldRight" : "foldLeft"} />
      </button>
    </div>
  );
}

/**
 * A project's mark in the rail: the image it was given, or the colour a person gave it with the first
 * character of its name written on it (`./projectMark`).
 *
 * It is the terminal face's tab mark, at the same 24px (`AMB-D-848`). Before this the rail drew an
 * 8px square of colour and no letter, so the same project arrived one way on one face and another on
 * the other, and the image a person had registered was shown on neither.
 *
 * **The image is the caller's to hand over.** An archived project is fetched over its own read path
 * and that shape carries the colour and the name alone, so those rows pass `null` rather than having
 * an image looked up for them here.
 */
function ProjectMark({ color, name, icon }: { color: string; name: string; icon: string | null }) {
  // A project whose colour cannot be read has no ink either: the mark keeps the rail's own surface
  // and the face's text colour, which is readable in both themes.
  const ink = color ? inkOn(color) : null;
  return (
    <span
      className="navitem__mark"
      // The image fills the mark, so the colour is left off underneath it — it would only show
      // through the corners of the picture.
      style={icon === null ? { background: color, ...(ink === null ? {} : { color: ink }) } : undefined}
      aria-hidden="true"
    >
      {icon === null ? initialOf(name) : <img className="navitem__icon" src={icon} alt="" />}
    </span>
  );
}

/**
 * The badge or badges a smart view carries.
 *
 * The inbox counts what needs an answer, on the accent. The due row is the one view that warns on two
 * steps at once — its day has gone or is today, and its day is tomorrow — so it draws one badge per
 * step rather than merging them: merged, it would have to pick a single colour and drop the other
 * count, and the two ask different things of the reader. Each badge carries its own words, so the
 * colour is never the only thing that says which step it is. Any other view shows what the data says
 * it has, or nothing.
 *
 * **Compact, the two are added up and drawn as one** (`AMB-D-848`). There is no room beside a 24px
 * mark for a second badge, and the corner it goes to holds one. What is kept is the sum and the more
 * urgent of the two colours, which is the reading a badge in a folded column is for — that there is
 * something here, and how soon. Which step each part of the sum stands on is on the badge's own
 * words, and the whole of it is a press away.
 */
function ViewBadges(
  { view, inboxCount, due, compact }:
  { view: SmartView; inboxCount: number; due: DueCounts; compact: boolean },
) {
  if (view.id === "due") {
    const badges = dueBadges(due);
    if (badges.length === 0) return null;
    const words = (step: DueStep) => (step === "stop" ? t("smartview.dueStop") : t("smartview.dueHeed"));
    if (compact) {
      // `dueBadges` answers most urgent first, so the head of it is the colour the sum is drawn in.
      const total = badges.reduce((n, b) => n + b.count, 0);
      return (
        <span
          className={`navitem__count navitem__count--${badges[0].step}`}
          title={badges.map((b) => words(b.step)).join(" / ")}
        >
          {total}
        </span>
      );
    }
    return (
      <span className="navitem__counts">
        {badges.map((b) => (
          <span key={b.step} className={`navitem__count navitem__count--${b.step}`} title={words(b.step)}>
            {b.count}
          </span>
        ))}
      </span>
    );
  }
  const count = view.id === "inbox" ? inboxCount : view.count;
  if (!count) return null;
  return <span className={`navitem__count ${view.id === "inbox" ? "navitem__count--alert" : ""}`}>{count}</span>;
}
