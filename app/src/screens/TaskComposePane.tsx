import { useEffect, useState } from "react";
import { useStore } from "../store/store";
import { DateField } from "../components/atoms";
import { t } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";

// Creating a new task in the right pane: enter a title plus notes (Markdown) and create it. Where it is created is
// chosen by the caller (the plus in a column header). A task only gets placed in a project; classification (assigning
// it to a dimension) is added afterwards from the task detail.
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

  // Dirty whenever there is input (title, notes, or either day). On unmount, always drop it back to false.
  useEffect(() => {
    onDirtyChange?.(title.trim() !== "" || notes.trim() !== "" || due !== null || start !== null);
    return () => onDirtyChange?.(false);
  }, [title, notes, due, start, onDirtyChange]);

  const submit = async () => {
    if (!title.trim()) return;
    const newId = await store.addTask(projectId, title.trim(), notes.trim() || undefined, due, start);
    onCreated(newId);
  };

  return (
    <div className="detail">
      <div className="detail__body">
        <div className="detail__section-h">{t("compose.new")} · {label}</div>

        <input
          {...asTyped}
          className="compose__input"
          style={{ minHeight: "unset", fontWeight: "var(--fw-bold)" }}
          autoFocus
          value={title}
          placeholder={t("compose.titlePh")}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (isEnterSubmit(e) && !e.shiftKey) { e.preventDefault(); void submit(); }
            if (e.key === "Escape") onCancel();
          }}
        />

        <div style={{ marginTop: "var(--s-2)" }}>
          <div className="detail__section-h">{t("compose.notes")}</div>
          <textarea
            {...asTyped}
            className="compose__input"
            rows={6}
            value={notes}
            placeholder={t("compose.notesPh")}
            onChange={(e) => setNotes(e.target.value)}
            onKeyDown={(e) => {
              if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void submit(); }
              if (e.key === "Escape") onCancel();
            }}
          />
        </div>

        <div className="detail__field" style={{ marginTop: "var(--s-2)" }}>
          <span className="detail__flabel">{t("date.due")}</span>
          <DateField label={t("date.due")} value={due} onChange={setDue} />
        </div>
        <div className="detail__field">
          <span className="detail__flabel">{t("date.start")}</span>
          <DateField label={t("date.start")} value={start} onChange={setStart} />
        </div>

        <div className="compose__actions" style={{ marginTop: "var(--s-2)" }}>
          <span className="meta">{t("compose.hint")}</span>
          <span>
            <button className="btn" onClick={onCancel}>{t("compose.cancel")}</button>
            <button className="btn btn--primary" style={{ marginLeft: 6 }} disabled={!title.trim()} onClick={() => void submit()}>{t("compose.create")}</button>
          </span>
        </div>
      </div>
    </div>
  );
}
