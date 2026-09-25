// The parts the build panels are drawn with — the action's and a spot's on an automation — so that a
// panel says what it holds by its shape rather than by a sentence under each field (`AMB-T-5522`).
// One part per shape: a heading or a switch drawn two ways would read as two different things.
//
// **A section is a heading with its own "＋ add".** The empty row a name is typed into is there only
// after that press, and goes again once the name is written: a row that is always open reads as one
// more thing declared, with nothing in it.
//
// **A declaration is a chip, and changing it is behind "⋯".** The chip is what the picture draws it as
// (`.actport`, coloured by what it carries), so the panel and the picture say the same thing the same
// way. What renames, re-kinds or removes it opens under the row on the "⋯", since those are the rare
// presses and the chip is what is read.
//
// **An on/off is a switch.** A switch says it writes at once, which every field here does.
import { useState, type ReactNode } from "react";
import { removeAutomationExit, renameAutomationExit } from "../core/automations";
import { t } from "../core/i18n";
import { DeclareRow, useDraft, type Choice, type Run } from "./automationPanel";
import { kindLabel } from "./automationPortKinds";
import type { AutomationExitDto } from "../bindings/bindings";

/** An on/off that writes the moment it is flipped. `boxed` draws it framed, standing apart — the
 *  start, on a spot and on a step alike. */
export function Switch({
  label,
  checked,
  onChange,
  disabled = false,
  boxed = false,
}: {
  label: string;
  checked: boolean;
  onChange: (to: boolean) => void;
  disabled?: boolean;
  boxed?: boolean;
}) {
  return (
    <label className={boxed ? "autoswitch autoswitch--box" : "autoswitch"}>
      <span>{label}</span>
      <span className="autoswitch__track">
        <input
          type="checkbox"
          role="switch"
          checked={checked}
          disabled={disabled}
          onChange={(e) => onChange(e.target.checked)}
        />
        <i aria-hidden="true" />
      </span>
    </label>
  );
}

/** A heading over a part of a panel, with the part under it — on every build panel, the spot's
 *  (`./AutomationStepPanel`) as well as the action's. */
export function Sec({
  title,
  onAdd,
  children,
}: {
  /** What the part is called — with a mark beside it where it has one, such as "required". */
  title: ReactNode;
  /** The press that opens a row to declare one more — absent where the part takes none. */
  onAdd?: () => void;
  children?: ReactNode;
}) {
  return (
    <section className="autosec">
      <div className="autosec__head">
        <span>{title}</span>
        {onAdd !== undefined && (
          <button type="button" className="autosec__add" onClick={onAdd}>
            {t("auto.decl.add")}
          </button>
        )}
      </div>
      {children}
    </section>
  );
}

/**
 * A section whose rows are declared: the heading's "＋ add" opens the row a name is typed into, and a
 * name written closes it again.
 */
export function DeclSec({
  title,
  what,
  kinds,
  onAdd,
  children,
}: {
  title: string;
  /** What the empty box says it wants. */
  what: string;
  kinds: Choice[] | null;
  onAdd: (name: string, kind: string) => Promise<boolean>;
  children?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Sec title={title} onAdd={() => setOpen(!open)}>
      {children}
      {open && (
        <DeclareRow
          what={what}
          kinds={kinds}
          autoFocus
          onCancel={() => setOpen(false)}
          onAdd={(name, kind) =>
            onAdd(name, kind).then((written) => {
              if (written) setOpen(false);
              return written;
            })
          }
        />
      )}
    </Sec>
  );
}

/** One declared thing as the picture draws it: its name, what it carries, and whether it is owed. */
export function DeclChip({
  name,
  kind,
  tone,
  required = false,
}: {
  name: string;
  /** What it carries, already in the words the screen shows. */
  kind?: string;
  /** Which colour it is drawn in — a port's kind, or `cfg` for a setting. */
  tone: string;
  required?: boolean;
}) {
  return (
    <span className="autodecl__chip">
      <span className={`actport actport--${tone}`}>
        {name}
        {kind !== undefined && <span className="actport__kind">{kind}</span>}
      </span>
      {required && <span className="autodecl__req">{t("auto.decl.required")}</span>}
    </span>
  );
}

/** A port as a chip, in its own kind's colour and word. */
export function PortChip({ port }: { port: { name: string; kind: string; required?: boolean } }) {
  return <DeclChip name={port.name} kind={kindLabel(port.kind)} tone={port.kind} required={port.required} />;
}

/**
 * One declared row: what is read, and the "⋯" that opens what changes it under the row.
 */
export function DeclItem({
  children,
  edit,
  below,
  className = "autodecl",
}: {
  /** The row as it is read — chips, and whatever stands beside them. */
  children: ReactNode;
  /** What renames, re-kinds or removes it — absent where nothing may. */
  edit?: ReactNode;
  /** What stands under the row whether or not it is being changed — where a way out goes next. */
  below?: ReactNode;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className={className}>
      <div className="autodecl__row">
        {children}
        {edit !== undefined && (
          <button
            type="button"
            className="autodecl__more"
            aria-expanded={open}
            aria-label={t("auto.decl.more")}
            title={t("auto.decl.more")}
            onClick={() => setOpen(!open)}
          >
            ⋯
          </button>
        )}
      </div>
      {open && edit}
      {below}
    </div>
  );
}

/** The small "＋" beside a way out's mark that declares one more thing leaving by it hands on. */
export function OutputPlus({ onPress }: { onPress: () => void }) {
  return (
    <button
      type="button"
      className="autodecl__plus"
      aria-label={t("auto.decl.outputAdd")}
      title={t("auto.decl.outputAdd")}
      onClick={onPress}
    >
      ＋
    </button>
  );
}

/**
 * What changes a way out, opened on its "⋯": its name, and the press that takes it away.
 */
export function ExitEdit({
  owner,
  ownerId,
  exit,
  run,
}: {
  /** Which layer declares it — a step, or the action. */
  owner: "step" | "action";
  ownerId: number;
  exit: AutomationExitDto;
  run: Run;
}) {
  const [name, setName] = useDraft(exit.name);
  const was = exit.name;
  return (
    <div className="autostep__decl">
      <input
        className="autostep__declname"
        aria-label={t("auto.step.exitName")}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => {
          // A way out keeps a name: emptied, it goes back to the one it had.
          const now = name.trim();
          if (now === "") setName(was);
          else if (now !== was) void run(renameAutomationExit(owner, ownerId, was, now));
        }}
      />
      <button
        type="button"
        className="btn"
        onClick={() => void run(removeAutomationExit(owner, ownerId, was))}
      >
        {t("auto.step.remove")}
      </button>
    </div>
  );
}
