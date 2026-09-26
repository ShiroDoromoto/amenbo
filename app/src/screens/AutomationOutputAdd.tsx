// **Declare what a way out hands on** (`AMB-T-5257`), in one row inside that way out's card, opened by
// the "＋" beside its mark (`AMB-T-5526`).
//
// **A row, not a dialog.** What it asks is a name and a kind, and the way out it belongs to is the
// card it opens in — a dialog over the panel would hide the very card it is about. Whether the step
// is refused without it is one toggle on the row, starting at "required": nothing changes it once the
// output is declared, so it is said here or not at all.
//
// **The name follows the way out until somebody writes their own.** A way out that hands on one
// thing is named for what it hands on nine times out of ten — "the draft" leaving by "drafted" — so
// the box starts on the way out's own name and stops following the moment a reader touches it. It
// only starts there where the way out hands on nothing yet: a second output named after the way out
// would be the first one's name again.
//
// **Escape or the × puts it away**, unlike the dialog that writes a step: there is a name and a kind
// here and no prompt half written, so nothing is lost by closing it.
import { useState } from "react";
import { addAutomationOutput } from "../core/automations";
import { t } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { Icon } from "../components/Icon";
import { OUTPUT_KINDS } from "./automationPortKinds";
import type { AutomationExitDto } from "../bindings/bindings";

export function AutomationOutputAdd({
  exit,
  onClose,
}: {
  exit: AutomationExitDto;
  onClose: () => void;
}) {
  // Nothing written yet. The way out's own name stands in until it is, and `null` is what says so.
  const [own, setOwn] = useState<string | null>(null);
  const [kind, setKind] = useState<string>("value");
  const [required, setRequired] = useState(true);
  const name = own ?? (exit.outputs.length === 0 ? exit.name : "");

  const add = () => {
    if (name.trim() === "") return;
    void addAutomationOutput(exit.id, { name: name.trim(), kind, required });
    onClose();
  };

  return (
    <div className="autoout">
      <input
        {...asTyped}
        autoFocus
        className="autoout__name"
        placeholder={t("auto.add.namePh")}
        aria-label={t("auto.add.namePh")}
        value={name}
        onChange={(e) => setOwn(e.target.value)}
        onKeyDown={(e) => {
          if (isEnterSubmit(e)) add();
          else if (e.key === "Escape") onClose();
        }}
      />
      <span className="autoout__kinds" role="radiogroup" aria-label={t("auto.out.kind")}>
        {OUTPUT_KINDS.map((one) => (
          <button
            key={one.id}
            type="button"
            role="radio"
            aria-checked={kind === one.id}
            className={kind === one.id ? "autoout__kind autoout__kind--on" : "autoout__kind"}
            onClick={() => setKind(one.id)}
          >
            {one.label()}
          </button>
        ))}
      </span>
      <button
        type="button"
        className={required ? "autoout__kind autoout__kind--on autoout__req" : "autoout__kind autoout__req"}
        aria-pressed={required}
        onClick={() => setRequired(!required)}
      >
        {t("auto.decl.required")}
      </button>
      <button type="button" className="btn btn--primary" disabled={name.trim() === ""} onClick={add}>
        {t("auto.out.add")}
      </button>
      <button type="button" className="autoout__close" aria-label={t("auto.add.cancel")} onClick={onClose}>
        <Icon name="close" />
      </button>
    </div>
  );
}
