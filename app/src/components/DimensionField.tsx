// The input for one classification axis, on whichever side is asking — the task detail pane or the
// decision pane (`AMB-D-781`). It draws what the axis says it admits (`AMB-D-826`): a single-select
// axis keeps the one select it always had, and a multi-select one draws the values the record carries
// as chips, each with a cross, plus a select that offers the values it does not carry yet.
//
// It holds no state. Both panes move first and let the write answer after, so what is drawn is the
// map the pane keeps and rolls back — passing that in, rather than keeping a copy here, is what stops
// the two from ever disagreeing about what "cleared" looks like.
//
// A closed value is not offered (`AMB-D-829`): closing retires it from what a record is newly filed
// under, which is exactly what this field does. What the record already carries is drawn whether or not
// the value is closed — a value that vanished from the field could never be taken off or replaced, and
// closing is meant to leave what was filed under it alone.
import type { DimensionDto, DimensionValueDto } from "../bindings/bindings";
import { t } from "../core/i18n";

/** The values this field offers: the open ones, plus whatever the record already carries. */
function offered(dim: DimensionDto, selected: readonly number[]): DimensionValueDto[] {
  return dim.values.filter((v) => !v.closed || selected.includes(v.id));
}

export function DimensionField({ dim, selected, onSet, onUnset }: {
  dim: DimensionDto;
  /** The value ids the record carries on this axis. Empty where it carries none. */
  selected: readonly number[];
  /** Put a value on the record. On a single-select axis core replaces the one that was there. */
  onSet: (valueId: number) => void;
  /** Take one value off the record — the only way off, on either kind of axis. */
  onUnset: (valueId: number) => void;
}) {
  if (dim.cardinality !== "multi") {
    const current = selected[0];
    return (
      <select
        className="inlineselect"
        value={current ?? ""}
        onChange={(e) => {
          const valueId = Number(e.target.value);
          if (valueId) onSet(valueId);
          else if (current) onUnset(current);
        }}
      >
        <option value="">{t("detail.none")}</option>
        {offered(dim, selected).map((v) => (
          <option key={v.id} value={v.id}>{v.name}</option>
        ))}
      </select>
    );
  }
  // The axis's own order, not the order the values were assigned in: a value added today would
  // otherwise sit at the end and the row would read differently on every record.
  const carried = dim.values.filter((v) => selected.includes(v.id));
  const rest = offered(dim, selected).filter((v) => !selected.includes(v.id));
  return (
    <span className="dimchips">
      {carried.length === 0 && rest.length === 0 && <span className="faint">{t("detail.none")}</span>}
      {carried.map((v) => (
        <span className="chip chip--dim" key={v.id}>
          {v.name}
          <button
            type="button"
            className="chip__x"
            title={t("detail.dimUnset")}
            aria-label={t("detail.dimUnset")}
            onClick={() => onUnset(v.id)}
          >
            ×
          </button>
        </span>
      ))}
      {rest.length > 0 && (
        <select
          className="inlineselect"
          value=""
          onChange={(e) => {
            const valueId = Number(e.target.value);
            if (valueId) onSet(valueId);
          }}
        >
          <option value="">＋ {t("detail.add")}</option>
          {rest.map((v) => (
            <option key={v.id} value={v.id}>{v.name}</option>
          ))}
        </select>
      )}
    </span>
  );
}
