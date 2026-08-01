import { useState, type DragEvent } from "react";
import { dataAdapter } from "../mock/adapter";
import { useInboxCount } from "../core/mailbox";
import { useArchivedProjects } from "../core/reads";
import { useStore } from "../store/store";
import { t } from "../core/i18n";
import type { Nav } from "./AppShell";

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
          // The inbox gets the reactive count that arrival detection feeds, and an alert-coloured badge to press what needs an answer.
          const count = v.id === "inbox" ? inboxCount : v.count;
          const alert = v.id === "inbox";
          return (
            <button key={v.id} className={`navitem ${isActive(n) ? "navitem--active" : ""}`} onClick={() => onNav(n)}>
              <span style={{ width: 16, textAlign: "center" }}>{v.icon}</span>
              {t(`smartview.${v.id}`)}
              {count ? <span className={`navitem__count ${alert ? "navitem__count--alert" : ""}`}>{count}</span> : null}
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
              <span style={{ width: 16, textAlign: "center" }}>＋</span>
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
          { id: "plugins", icon: "🧩", label: t("plugins.market") },
          { id: "pluginsInstalled", icon: "🔌", label: t("plugins.installed") },
        ] as const).map((item) => {
          const n: Nav = { type: "view", id: item.id };
          return (
            <button
              key={item.id}
              className={`navitem ${isActive(n) ? "navitem--active" : ""}`}
              onClick={() => onNav(n)}
            >
              <span style={{ width: 16, textAlign: "center" }}>{item.icon}</span>
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
          { id: "search", icon: "🔍", label: t("nav.search") },
          { id: "commands", icon: "📖", label: t("nav.commands") },
          { id: "settings", icon: "⚙", label: t("nav.settings") },
          { id: "onboarding", icon: "🪿", label: t("nav.onboarding") },
        ] as const).map((it) => {
          const n: Nav = { type: "view", id: it.id };
          return (
            <button key={it.id} className={`navitem ${isActive(n) ? "navitem--active" : ""}`} onClick={() => onNav(n)}>
              <span style={{ width: 16, textAlign: "center" }}>{it.icon}</span>{it.label}
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
            <span style={{ width: 16, textAlign: "center" }}>{archivedOpen ? "▾" : "▸"}</span>
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
