import { useState, useSyncExternalStore } from "react";
import { getSnapshot, subscribe } from "../core/snapshot";
import { t, tf } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { confirmDialog } from "../core/dialog";
import { fetchProjectDimensionAssignments } from "../core/mutations";
import { useStore } from "../store/store";
import { todayStr } from "../core/calendar";
import { currentTimeAxisValueId, isTimeAxis } from "../core/timeAxis";
import type { DimensionDto, DimensionValueDto } from "../bindings/bindings";
import { Icon } from "../components/Icon";

// The management panel for classification (unified dimensions), reached from the board's axis bar as a modal of its
// own. It exposes renaming a dimension, editing its notes and removing it; renaming and removing its values; the
// ordered toggle, the toggle that names an axis the time axis (role: time_axis), the toggle that puts an axis on the
// task card, the toggle that makes an axis refuse to be left empty, and, on an ordered dimension, reordering the
// values. Removing a value from a required axis is the one of those that asks a question first: the tasks answering
// with it have to be told which other value they move to, and the axis's last value is out of reach entirely, core
// keeping it so the demand stays answerable.
// Beside each name sits its readable key (`AMB-D-735`) — what the axis or the value answers to outside
// Amenbo, where a Japanese display name with spaces in it cannot go. A key is born with the row (derived from its
// id) and is edited here, never cleared; a key core refuses puts the field back and says why in a toast.
// Only on a named time axis do the values grow period fields (start/end date) and a "current" marker — a
// period is payload of the time-axis role, so no other axis shows dates. Naming one is not forced to be unique:
// core folds "the current era" to a single answer using the order of the dimensions.
export function DimensionManager({ projectId, onClose }: { projectId: number; onClose: () => void }) {
  const snap = useSyncExternalStore(subscribe, getSnapshot);
  const store = useStore();
  const project = snap.projects.find((p) => p.id === projectId);
  const dims = project?.dimensions ?? [];
  return (
    <div className="modal__overlay" onClick={onClose}>
      <div className="modal__card dimmgr" onClick={(e) => e.stopPropagation()}>
        <div className="dimmgr__head">
          <span className="dimmgr__title">{t("dimmgr.title")}</span>
          <button className="btn" onClick={onClose}>{t("dimmgr.close")}</button>
        </div>
        {dims.length === 0 ? (
          <p className="faint dimmgr__empty">{t("dimmgr.empty")}</p>
        ) : (
          <div className="dimmgr__list">
            {dims.map((d) => (
              <DimensionRow key={d.id} dim={d} projectId={projectId} store={store} />
            ))}
          </div>
        )}
        <AddInline
          className="dimmgr__adddim"
          buttonLabel={t("dimmgr.addDimension")}
          placeholder={t("dimmgr.namePh")}
          onAdd={(name) => store.addDimension(projectId, name)}
        />
      </div>
    </div>
  );
}

function DimensionRow({ dim, projectId, store }: { dim: DimensionDto; projectId: number; store: ReturnType<typeof useStore> }) {
  const currentId = currentTimeAxisValueId(dim, todayStr());
  async function removeDim() {
    if (await confirmDialog(tf("dimmgr.confirmRemoveDim", { name: dim.name }))) store.removeDimension(dim.id);
  }
  return (
    <div className="dimmgr__dim">
      <div className="dimmgr__dimhead">
        <InlineText
          className="dimmgr__name"
          value={dim.name}
          placeholder={t("dimmgr.namePh")}
          onCommit={(v) => store.renameDimension(dim.id, v)}
        />
        <InlineText
          className="dimmgr__slug"
          value={dim.slug ?? ""}
          placeholder={t("dimmgr.slugPh")}
          label={t("dimmgr.slug")}
          title={t("dimmgr.slugHint")}
          onCommit={(v) => store.setDimensionSlug(dim.id, v)}
        />
        <label className="dimmgr__ordered" title={t("dimmgr.orderedHint")}>
          <input
            type="checkbox"
            checked={dim.ordered}
            onChange={(e) => store.setDimensionOrdered(dim.id, e.target.checked)}
          />
          {t("dimmgr.ordered")}
        </label>
        <label className="dimmgr__ordered" title={t("dimmgr.timeAxisHint")}>
          <input
            type="checkbox"
            checked={isTimeAxis(dim)}
            onChange={(e) => store.setDimensionTimeAxis(dim.id, e.target.checked)}
          />
          {t("dimmgr.timeAxis")}
        </label>
        {/* Whether this axis rides on the task card. It sits beside the others because it is the same
            kind of thing: an answer the axis carries, so it moves for everyone at once and not just for
            whoever ticked it (`AMB-D-651`). */}
        <label className="dimmgr__ordered" title={t("dimmgr.showOnCardHint")}>
          <input
            type="checkbox"
            checked={dim.showOnCard}
            onChange={(e) => store.setDimensionShowOnCard(dim.id, e.target.checked)}
          />
          {t("dimmgr.showOnCard")}
        </label>
        {/* Whether this axis refuses to be left empty (`AMB-D-734`). It is the same kind of answer as the
            three beside it — the axis's own, so it moves for everyone — and it bites in one place: a task
            carrying no value here cannot finish its creation. An axis offering no values could never be
            answered, so core refuses to raise it there and the box stays off until a value exists. */}
        <label
          className="dimmgr__ordered"
          title={dim.values.length === 0 ? t("dimmgr.requiredNoValuesHint") : t("dimmgr.requiredHint")}
        >
          <input
            type="checkbox"
            checked={dim.required}
            disabled={dim.values.length === 0}
            onChange={(e) => store.setDimensionRequired(dim.id, e.target.checked)}
          />
          {t("dimmgr.required")}
        </label>
        <button className="feed__action dimmgr__danger" onClick={removeDim}>{t("dimmgr.removeDim")}</button>
      </div>
      <InlineText
        className="dimmgr__notes"
        value={dim.notes}
        placeholder={t("dimmgr.notesPh")}
        allowEmpty
        onCommit={(v) => store.updateDimension(dim.id, v)}
      />
      <div className={`dimmgr__values ${dim.ordered ? "dimmgr__values--ordered" : ""}`}>
        <span className="faint dimmgr__vlabel">{t("dimmgr.values")}</span>
        {dim.values.map((v, i) => (
          <ValueRow
            key={v.id}
            value={v}
            store={store}
            projectId={projectId}
            dimensionId={dim.id}
            required={dim.required}
            siblings={dim.values.filter((o) => o.id !== v.id)}
            ordered={dim.ordered}
            timeAxis={isTimeAxis(dim)}
            current={v.id === currentId}
            onMoveUp={i > 0 ? () => store.moveDimensionValue(v.id, { before: dim.values[i - 1].id }) : undefined}
            onMoveDown={
              i < dim.values.length - 1
                ? () => store.moveDimensionValue(v.id, { after: dim.values[i + 1].id })
                : undefined
            }
          />
        ))}
        <AddInline
          className="dimmgr__addval"
          buttonLabel={t("dimmgr.addValue")}
          placeholder={t("dimmgr.valueNamePh")}
          onAdd={(name) => store.addDimensionValue(dim.id, name)}
        />
      </div>
    </div>
  );
}

function ValueRow({ value, store, projectId, dimensionId, required, siblings, ordered, timeAxis, current, onMoveUp, onMoveDown }: {
  value: DimensionValueDto;
  store: ReturnType<typeof useStore>;
  projectId: number;
  dimensionId: number;
  required: boolean;
  siblings: DimensionValueDto[];
  ordered: boolean;
  timeAxis: boolean;
  current: boolean;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
}) {
  // Where the tasks answering with this value are to go, once the panel has decided it has to ask.
  // `null` is not asking; a number is the answer chosen so far, and 0 stands for "asked, nothing
  // chosen yet" so the button that carries it out can stay shut until one is.
  const [moveTo, setMoveTo] = useState<number | null>(null);
  // The last value of a required axis does not go at any price — core keeps it so the demand stays
  // answerable, and lowering the demand is the way out. Said on the button rather than found by
  // pressing it, the way the box that raises the demand says why it is off.
  const stuck = required && siblings.length === 0;
  async function removeValue() {
    // A required axis will not let the value take its tasks' answers with it, so the panel asks where
    // they go before it asks whether to delete — and only where there are any. The count is read at
    // the press rather than held: what it decides is one dialog, and a number kept since the panel
    // opened could have moved under it.
    if (required && (await countOnValue(projectId, dimensionId, value.id)) > 0) {
      setMoveTo(0);
      return;
    }
    if (await confirmDialog(tf("dimmgr.confirmRemoveValue", { name: value.name }))) store.removeDimensionValue(value.id);
  }
  async function removeValueMoving(to: DimensionValueDto) {
    if (await confirmDialog(tf("dimmgr.confirmRemoveValueMoving", { name: value.name, to: to.name }))) {
      store.removeDimensionValue(value.id, to.id);
    }
    setMoveTo(null);
  }
  // The date fields are edited one end at a time, but the backend is always sent both ends (the other keeps its current value).
  const setPeriod = (start: string | undefined, end: string | undefined) =>
    store.setDimensionValuePeriod(value.id, start || undefined, end || undefined);
  return (
    <div className={`dimmgr__val ${current ? "dimmgr__val--current" : ""}`}>
      {ordered && (
        <span className="dimmgr__reorder">
          <button
            className="dimmgr__movebtn"
            disabled={!onMoveUp}
            onClick={onMoveUp}
            aria-label={t("dimmgr.moveUp")}
            title={t("dimmgr.moveUp")}
          >
            <Icon name="arrowUp" />
          </button>
          <button
            className="dimmgr__movebtn"
            disabled={!onMoveDown}
            onClick={onMoveDown}
            aria-label={t("dimmgr.moveDown")}
            title={t("dimmgr.moveDown")}
          >
            <Icon name="arrowDown" />
          </button>
        </span>
      )}
      <InlineText
        className="dimmgr__valname"
        value={value.name}
        placeholder={t("dimmgr.valueNamePh")}
        onCommit={(v) => store.renameDimensionValue(value.id, v)}
      />
      <InlineText
        className="dimmgr__valslug"
        value={value.slug ?? ""}
        placeholder={t("dimmgr.slugPh")}
        label={t("dimmgr.valueSlug")}
        title={t("dimmgr.slugHint")}
        onCommit={(v) => store.setDimensionValueSlug(value.id, v)}
      />
      {timeAxis && (
        <span className="dimmgr__period">
          <DateOrOpen
            date={value.startOn}
            label={t("dimmgr.periodStart")}
            openLabel={t("dimmgr.periodStartOpen")}
            onChange={(d) => setPeriod(d, value.endOn)}
          />
          <span className="faint" aria-hidden="true">〜</span>
          <DateOrOpen
            date={value.endOn}
            label={t("dimmgr.periodEnd")}
            openLabel={t("dimmgr.periodEndOpen")}
            onChange={(d) => setPeriod(value.startOn, d)}
          />
          {current && (
            <span className="dimmgr__current" title={t("dimmgr.currentHint")}>{t("dimmgr.current")}</span>
          )}
        </span>
      )}
      {moveTo === null ? (
        <button
          className="feed__action dimmgr__danger"
          disabled={stuck}
          title={stuck ? t("dimmgr.removeValueLastHint") : undefined}
          onClick={removeValue}
        >
          {t("dimmgr.removeValue")}
        </button>
      ) : (
        <span className="dimmgr__reassign">
          <label className="faint" htmlFor={`dimmgr-move-${value.id}`}>{t("dimmgr.reassignTo")}</label>
          <select
            id={`dimmgr-move-${value.id}`}
            className="dimmgr__reassignpick"
            value={moveTo || ""}
            onChange={(e) => setMoveTo(Number(e.target.value))}
          >
            <option value="" disabled>{t("dimmgr.reassignPick")}</option>
            {siblings.map((o) => (
              <option key={o.id} value={o.id}>{o.name}</option>
            ))}
          </select>
          <button
            className="feed__action dimmgr__danger"
            disabled={!moveTo}
            onClick={() => {
              const to = siblings.find((o) => o.id === moveTo);
              if (to) void removeValueMoving(to);
            }}
          >
            {t("dimmgr.removeValue")}
          </button>
          <button className="feed__action" onClick={() => setMoveTo(null)}>{t("dimmgr.cancel")}</button>
        </span>
      )}
    </div>
  );
}

/** How many tasks on this project answer the axis with this value — the count core refuses a removal
 * over. Outside Tauri the read answers nothing, which reads as an unclassified value and leaves the
 * mock panel on its plain confirm. */
async function countOnValue(projectId: number, dimensionId: number, valueId: number): Promise<number> {
  const rows = await fetchProjectDimensionAssignments(projectId, dimensionId);
  return rows.filter((r) => r.valueId === valueId).length;
}

// One end of a period. An empty `<input type="date">` is drawn by the webview with **today** as a faint placeholder,
// which makes "no end date = still running" read as "it ends today". So an open end says so in words and only turns
// into a date field once clicked — the empty field is never put in front of a human. Clearing the date opens it again.
function DateOrOpen({ date, label, openLabel, onChange }: {
  date?: string;
  label: string;
  openLabel: string;
  onChange: (date: string | undefined) => void;
}) {
  const [editing, setEditing] = useState(false);
  if (!date && !editing) {
    return (
      <button className="dimmgr__open" title={label} onClick={() => setEditing(true)}>{openLabel}</button>
    );
  }
  return (
    <input
      type="date"
      className="dimmgr__date"
      autoFocus={editing}
      value={date ?? ""}
      aria-label={label}
      title={label}
      onChange={(e) => onChange(e.target.value || undefined)}
      onBlur={() => setEditing(false)}
    />
  );
}

// An edit field that commits on blur or Enter. When the value changes underneath it (from the snapshot), `key`
// remounts it and reseeds — this is a single-user tool, so losing a draft to a concurrent external change is a rare
// case we accept. The backend rejects an empty name, so a name commits only when it is non-empty and has changed;
// notes pass allowEmpty and may be cleared.
//
// A mutator that answers whether the write landed (the keys do — a shape or a collision core refuses) puts the field
// back on a refusal: a success reseeds through the snapshot, but a refusal moves nothing, so without this the box
// would keep sitting there showing a key nothing was saved under. The toast carries the reason.
function InlineText({ value, placeholder, onCommit, className, allowEmpty = false, label, title }: {
  value: string;
  placeholder?: string;
  onCommit: (v: string) => void | Promise<boolean>;
  className?: string;
  allowEmpty?: boolean;
  label?: string;
  title?: string;
}) {
  const commit = (el: HTMLInputElement) => {
    const v = el.value.trim();
    if (v === value) return;
    if (!v && !allowEmpty) { el.value = value; return; }
    const landed = onCommit(v);
    if (landed) void landed.then((ok) => { if (!ok) el.value = value; });
  };
  return (
    <input
      {...asTyped}
      key={value}
      className={className}
      defaultValue={value}
      placeholder={placeholder}
      aria-label={label}
      title={title}
      onKeyDown={(e) => {
        if (isEnterSubmit(e)) { e.preventDefault(); e.currentTarget.blur(); }
        if (e.key === "Escape") { e.currentTarget.value = value; e.currentTarget.blur(); }
      }}
      onBlur={(e) => commit(e.currentTarget)}
    />
  );
}

// "+ Add" opens an inline input (Enter or blur commits, Escape cancels). Shared by adding a dimension and adding a value.
function AddInline({ buttonLabel, placeholder, onAdd, className }: {
  buttonLabel: string;
  placeholder: string;
  onAdd: (name: string) => void;
  className?: string;
}) {
  const [adding, setAdding] = useState(false);
  const [text, setText] = useState("");
  const commit = () => { if (text.trim()) { onAdd(text.trim()); setText(""); } };
  return adding ? (
    <input
      {...asTyped}
      className={className}
      autoFocus
      value={text}
      placeholder={placeholder}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => {
        if (isEnterSubmit(e)) { commit(); setAdding(false); }
        if (e.key === "Escape") { setText(""); setAdding(false); }
      }}
      onBlur={() => { commit(); setAdding(false); }}
    />
  ) : (
    <button className={`filterchip ${className ?? ""}`} onClick={() => setAdding(true)}>{buttonLabel}</button>
  );
}
