import { useEffect, useState } from "react";
import { useStore } from "../store/store";
import { DateField } from "../components/atoms";
import { parseSchedule } from "../core/calendar";
import { t } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { useSingleFlight } from "../core/singleFlight";
import { useBoundFolders } from "../core/boundFolders";
import { useProjectRoles } from "../core/projectRoles";
import { dataAdapter } from "../mock/adapter";
import { RoleField, ROLE_NONE } from "../components/RoleField";
import { ErrorNote } from "../components/ErrorNote";
import type { Facet } from "../mock/types";

// Creating a new task in the right pane: enter a title plus notes (Markdown) and create it. Where it is created is
// chosen by the caller (the plus in a column header). A task only gets placed in a project; classification (assigning
// it to a dimension) is added afterwards from the task detail. Who it is handed to can be said here, and handed to
// the AI in a project that defines roles, it also has to say which role it is worked under.
export function TaskComposePane({
  projectId, label, onCreated, onCancel, onDirtyChange,
}: {
  projectId: number;
  // The name of the destination shown in the heading (the project name).
  label: string;
  /** Hands over the id of the task just created (the parent switches the right pane to its detail). null if it cannot be obtained. */
  onCreated: (newId: number | null) => void;
  onCancel: () => void;
  // Reports to the parent (AppShell) whether there is unsaved input. Used by the discard guard on an outside click or the cross.
  onDirtyChange?: (dirty: boolean) => void;
}) {
  const store = useStore();
  const [title, setTitle] = useState("");
  const [notes, setNotes] = useState("");
  // The two days are offered here as well as on the detail pane, for the task whose dates are already
  // known when it is filed. Neither is required — a task still registers on a title alone.
  const [due, setDue] = useState<string | null>(null);
  const [start, setStart] = useState<string | null>(null);
  // The folder the task is worked in (`AMB-D-1012`). Asked only when the project has several: with one,
  // core fills it in, and with none there is nothing to fill in. Which of several is not ours to pick
  // (`AMB-D-531`), so nothing is chosen until the person chooses.
  const folders = useBoundFolders(projectId);
  const choosing = folders.all.length >= 2;
  const [at, setAt] = useState("");
  const canCreate = title.trim() !== "" && (!choosing || at !== "");
  // The facet it is handed to, "" for nobody.
  const [assignee, setAssignee] = useState<Facet | "">("");
  // The role, asked of a task handed to the AI in a project that defines one. Nothing is chosen until
  // the person chooses, "no role" included (`AMB-D-531`); creating before that says why under the field
  // rather than greying the button, so the press is answered.
  const roles = useProjectRoles(projectId);
  const askingRole = assignee === "ai" && roles.length > 0;
  const [role, setRole] = useState("");
  const [roleMissing, setRoleMissing] = useState(false);
  const pickAssignee = (next: Facet | "") => { setAssignee(next); setRole(""); setRoleMissing(false); };

  // Dirty whenever there is input (title, notes, either day, a chosen folder, an assignee). On unmount, always drop it back to false.
  useEffect(() => {
    onDirtyChange?.(title.trim() !== "" || notes.trim() !== "" || due !== null || start !== null || at !== "" || assignee !== "");
    return () => onDirtyChange?.(false);
  }, [title, notes, due, start, at, assignee, onDirtyChange]);

  // The button, Enter and Cmd+Enter all come here, and a second of any of them before the answer is dropped:
  // each one that got through would file a task of its own.
  const { busy, run } = useSingleFlight();
  const submit = () => {
    if (!canCreate) return;
    if (askingRole && role === "") { setRoleMissing(true); return; }
    const roleId = askingRole && role !== ROLE_NONE ? Number(role) : null;
    run(async () => {
      const newId = await store.addTask(
        projectId, title.trim(), notes.trim() || undefined, due, start, choosing ? at : null, assignee || null, roleId,
      );
      onCreated(newId);
    });
  };

  return (
    <div className="detail">
      <div className="detail__body">
        <div className="detail__section-h">{t("compose.new")} · {label}</div>

        <input
          {...asTyped}
          className="writebox__input"
          style={{ minHeight: "unset", fontWeight: "var(--fw-bold)" }}
          autoFocus
          value={title}
          placeholder={t("compose.titlePh")}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (isEnterSubmit(e) && !e.shiftKey) { e.preventDefault(); submit(); }
            if (e.key === "Escape") onCancel();
          }}
        />

        <div style={{ marginTop: "var(--s-2)" }}>
          <div className="detail__section-h">{t("compose.notes")}</div>
          <textarea
            {...asTyped}
            className="writebox__input"
            rows={6}
            value={notes}
            placeholder={t("compose.notesPh")}
            onChange={(e) => setNotes(e.target.value)}
            onKeyDown={(e) => {
              if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); submit(); }
              if (e.key === "Escape") onCancel();
            }}
          />
        </div>

        <div className="detail__field" style={{ marginTop: "var(--s-2)" }}>
          <span className="detail__flabel">{t("date.due")}</span>
          <DateField label={t("date.due")} value={due ? parseSchedule(due) : null} onChange={setDue} />
        </div>
        <div className="detail__field">
          <span className="detail__flabel">{t("date.start")}</span>
          <DateField label={t("date.start")} value={start ? parseSchedule(start) : null} onChange={setStart} />
        </div>
        <div className="detail__field">
          <span className="detail__flabel">{t("detail.assignee")}</span>
          <select
            className="btn"
            aria-label={t("detail.assignee")}
            value={assignee}
            onChange={(e) => pickAssignee(e.target.value as Facet | "")}
          >
            <option value="">{t("detail.unassigned")}</option>
            {dataAdapter.listRoster().map((a) => <option value={a.kind} key={a.kind}>{a.name}</option>)}
          </select>
        </div>
        {askingRole && (
          <>
            <div className="detail__field">
              <span className="detail__flabel">{t("detail.role")}</span>
              <RoleField roles={roles} value={role} onChange={(v) => { setRole(v); setRoleMissing(false); }} />
            </div>
            {roleMissing && role === "" && <ErrorNote>{t("detail.roleRequired")}</ErrorNote>}
          </>
        )}
        {choosing && (
          <div className="detail__field">
            <span className="detail__flabel">{t("compose.folder")}</span>
            <select className="btn" aria-label={t("compose.folder")} value={at} onChange={(e) => setAt(e.target.value)}>
              <option value="" disabled>{t("compose.folderPick")}</option>
              {folders.all.map((f) => <option value={f.path} key={f.path}>{f.path}</option>)}
            </select>
          </div>
        )}

        <div className="writebox__actions" style={{ marginTop: "var(--s-2)" }}>
          <span className="meta">{t("compose.hint")}</span>
          <span>
            <button className="btn" onClick={onCancel}>{t("compose.cancel")}</button>
            <button className="btn btn--primary" style={{ marginLeft: 6 }} disabled={busy || !canCreate} onClick={submit}>{t("compose.create")}</button>
          </span>
        </div>
      </div>
    </div>
  );
}
