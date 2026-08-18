import { useState, type DragEvent } from "react";
import { dataAdapter } from "../mock/adapter";
import { useInboxCount } from "../core/mailbox";
import { dueBadges, type DueCounts } from "../core/due";
import { useArchivedProjects, useDueCounts } from "../core/reads";
import { useStore } from "../store/store";
import { t } from "../core/i18n";
import { Icon, type IconName } from "../components/Icon";
import type { SmartView } from "../mock/types";
import type { Nav } from "./AppShell";

// Which icon each smart view is drawn with. The views arrive as ids alone, so the drawing
// is decided here rather than travelling with the data (`AMB-D-689`).
const VIEW_ICON: Record<string, IconName> = { inbox: "inbox", activity: "activity", due: "calendar" };

/**
 * The left sidebar (smart views, projects, other, and the collapsed archive). Reordering a project drags and drops
 * onto `moveProject`, and the new order arrives through the snapshot once the write is acked — there is no optimistic
 * state. Drag & drop inside the webview requires `dragDropEnabled:false` in tauri.conf.json. Where the row lands
 * (before/after) is computed from the drop's own clientY and the row under it, never from the stored `dropHint`: a
 * fast drag can end with the last dragover on a different row than the drop, leaving the hint unset or pointing
 * elsewhere, and trusting it would make the drop a silent no-op. The hint stays what it is — the visual indicator.
 */
export function Sidebar({ nav, onNav }: { nav: Nav; onNav: (n: Nav) => void }) {
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
  const onRowDragStart = (e: DragEvent, id: number) => {
    e.dataTransfer.setData("text/plain", String(id));
    e.dataTransfer.effectAllowed = "move";
    setDragId(id);
  };
  const onRowDragOver = (e: DragEvent, id: number) => {
    if (!dragId || dragId === id) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const r = e.currentTarget.getBoundingClientRect();
    const pos: "before" | "after" = e.clientY < r.top + r.height / 2 ? "before" : "after";
    setDropHint((h) => (h && h.id === id && h.pos === pos ? h : { id, pos }));
  };
  const onRowDrop = (e: DragEvent, id: number) => {
    e.preventDefault();
    const src = Number(e.dataTransfer.getData("text/plain")) || dragId;
    setDropHint(null);
    setDragId(null);
    if (!src || src === id) return;
    const r = e.currentTarget.getBoundingClientRect();
    const pos: "before" | "after" = e.clientY < r.top + r.height / 2 ? "before" : "after";
    store.moveProject(src, pos, id);
  };
  const endDrag = () => {
    setDragId(null);
    setDropHint(null);
  };

  return (
    <div className="sidebar">
      <div className="sidebar__group">
        <div className="sidebar__label">{t("side.smartViews")}</div>
        {views.map((v) => {
          const n: Nav = { type: "view", id: v.id };
          return (
            <button key={v.id} className={`navitem ${isActive(n) ? "navitem--active" : ""}`} onClick={() => onNav(n)}>
              {VIEW_ICON[v.id] ? <Icon name={VIEW_ICON[v.id]} /> : null}
              {t(`smartview.${v.id}`)}
              <ViewBadges view={v} inboxCount={inboxCount} due={due} />
            </button>
          );
        })}
      </div>

      <div className="sidebar__group">
        <div className="sidebar__label">{t("side.projects")}</div>
        {projects.map((p) => {
          const n: Nav = { type: "project", id: String(p.id) };
          // The drop indicator: a line above or below the midline. The row being dragged is dimmed.
          const hint = dropHint && dropHint.id === p.id ? dropHint.pos : null;
          const cls = [
            "navitem",
            isActive(n) ? "navitem--active" : "",
            dragId === p.id ? "navitem--dragging" : "",
            hint === "before" ? "navitem--drop-before" : "",
            hint === "after" ? "navitem--drop-after" : "",
          ].filter(Boolean).join(" ");
          return (
            <button
              key={p.id}
              className={cls}
              onClick={() => onNav(n)}
              draggable={canReorder}
              onDragStart={canReorder ? (e) => onRowDragStart(e, p.id) : undefined}
              onDragOver={canReorder ? (e) => onRowDragOver(e, p.id) : undefined}
              onDrop={canReorder ? (e) => onRowDrop(e, p.id) : undefined}
              onDragEnd={canReorder ? endDrag : undefined}
            >
              <span className="navitem__dot" style={{ background: p.color }} />
              {p.name}
              {(() => {
                const count = p.openCount + p.proposedDecisionCount;
                return count ? <span className="navitem__count">{count}</span> : null;
              })()}
            </button>
          );
        })}
        {(() => {
          const n: Nav = { type: "view", id: "newProject" };
          return (
            <button className={`navitem navitem--muted ${isActive(n) ? "navitem--active" : ""}`} onClick={() => onNav(n)}>
              <Icon name="plus" />
              {t("side.newProject")}
            </button>
          );
        })()}
      </div>

      {/* Plugins are a section of their own, not an item under "other" (`AMB-D-356`): finding one and
          managing what is installed are two surfaces, and this is where the second one joins. */}
      <div className="sidebar__group">
        <div className="sidebar__label">{t("side.plugins")}</div>
        {([
          { id: "plugins", icon: "puzzle", label: t("plugins.market") },
          { id: "pluginsInstalled", icon: "plug", label: t("plugins.installed") },
        ] as const).map((item) => {
          const n: Nav = { type: "view", id: item.id };
          return (
            <button
              key={item.id}
              className={`navitem ${isActive(n) ? "navitem--active" : ""}`}
              onClick={() => onNav(n)}
            >
              <Icon name={item.icon} />
              {item.label}
            </button>
          );
        })}
      </div>

      <div className="sidebar__group">
        <div className="sidebar__label">{t("side.other")}</div>
        {([
          // Search sits here rather than among the smart views: a smart view is a standing selection of
          // tasks, and this asks where a word is written across tasks and decisions both (`AMB-D-449`).
          { id: "search", icon: "search", label: t("nav.search") },
          { id: "commands", icon: "book", label: t("nav.commands") },
          // Connecting an AI is an app-level setting, not a project's (`AMB-D-681`), so it lives
          // here rather than folded into each project's own screen.
          { id: "mcp", icon: "link", label: t("nav.mcp") },
          { id: "settings", icon: "gear", label: t("nav.settings") },
          { id: "onboarding", icon: "goose", label: t("nav.onboarding") },
        ] as const).map((it) => {
          const n: Nav = { type: "view", id: it.id };
          return (
            <button key={it.id} className={`navitem ${isActive(n) ? "navitem--active" : ""}`} onClick={() => onNav(n)}>
              <Icon name={it.icon} />{it.label}
            </button>
          );
        })}
      </div>

      {archived.length > 0 && (
        <div className="sidebar__group">
          <button
            className="navitem navitem--muted"
            aria-expanded={archivedOpen}
            onClick={() => setArchivedOpen((v) => !v)}
          >
            <Icon name={archivedOpen ? "chevronDown" : "chevronRight"} />
            {t("side.archived")}
            <span className="navitem__count">{archived.length}</span>
          </button>
          {archivedOpen &&
            archived.map((p) => {
              const n: Nav = { type: "projectSettings", id: String(p.id) };
              return (
                <button
                  key={p.id}
                  className={`navitem navitem--muted ${isActive(n) ? "navitem--active" : ""}`}
                  onClick={() => onNav(n)}
                >
                  <span className="navitem__dot" style={{ background: p.color }} />
                  {p.name}
                </button>
              );
            })}
        </div>
      )}
    </div>
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
 */
function ViewBadges({ view, inboxCount, due }: { view: SmartView; inboxCount: number; due: DueCounts }) {
  if (view.id === "due") {
    const badges = dueBadges(due);
    if (badges.length === 0) return null;
    return (
      <span className="navitem__counts">
        {badges.map((b) => (
          <span
            key={b.step}
            className={`navitem__count navitem__count--${b.step}`}
            title={b.step === "stop" ? t("smartview.dueStop") : t("smartview.dueHeed")}
          >
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
