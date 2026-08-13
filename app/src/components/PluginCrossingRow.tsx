import { useState } from "react";
import type { PluginCheckDto } from "../bindings/bindings";
import { errText, t, tn } from "../core/i18n";
import {
  crossingAt,
  setPluginEnabled,
  type PluginInstall,
  type PluginLayer,
} from "../core/pluginInstalls";
import { PluginConfigForm } from "./PluginConfigForm";

/**
 * One project × plugin crossing, as the row both plugin faces are built out of (`AMB-D-447`).
 *
 * The crossing is the unit, so everything about it is on this line: whether the plugin fires there, the
 * switch that moves it, and the settings that project holds — opened inside the row rather than in a
 * form that would ask which project all over again. Someone refused for want of a `required` value fills
 * it in without leaving the row that refused them.
 *
 * **The mark comes before the switch.** A crossing short of a `required` setting wears it whether or not
 * anyone has pressed anything, because that is what an enable there would be refused over
 * (`AMB-D-351`) — a warning that only appears after the refusal is a warning that arrived too late.
 *
 * **And a refusal lasts as long as it is true of the row.** It says which values were missing when the
 * switch was pressed, so a write to the settings in this row retires it: a row cannot say its settings
 * are filled in and that they are keeping it off. Whether it may be enabled now is answered by pressing
 * again — nothing else here can answer it.
 *
 * What differs between the two faces is the name on the row and nothing else: the plugin screen lists
 * projects, a project's settings list plugins, and the same crossing is the same row either way.
 *
 * **A device row is this same row at the other layer** (`AMB-D-601`). A plugin its author declared the
 * machine's crosses no project and has one gate, so what it gets is one of these with `layer` at `null` —
 * the same switch, the same settings opened inside it, and a name that says the device rather than a
 * project. Nothing here decides which: the caller read the declaration off the install.
 */
export function PluginCrossingRow({ install, layer, name }: {
  install: PluginInstall;
  /**
   * Which layer this row is at — the project whose gate the switch moves and whose values the form
   * writes, or `null` for the device's own row.
   */
  layer: PluginLayer;
  /** What names the row on this face: the project, the plugin, or the device it is the whole of. */
  name: string;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // What the last disable threw away, until the switch is moved again. Zero is not a state worth a line:
  // an empty queue is the ordinary case, and saying so every time would train the eye past the one time
  // it matters — the same silence the CLI keeps.
  const [dropped, setDropped] = useState(0);
  const [open, setOpen] = useState(false);
  // What the author's own check said the last time this switch was pressed (`AMB-D-664`). It is a fact
  // about one press, like the refusal beside it, so a write to this crossing's settings retires it.
  const [check, setCheck] = useState<PluginCheckDto | null>(null);
  const at = crossingAt(install, layer);

  const move = async (next: boolean) => {
    setBusy(true);
    setError(null);
    setDropped(0);
    setCheck(null);
    try {
      const moved = await setPluginEnabled(install.name, layer, next);
      setDropped(moved.droppedQueued);
      setCheck(moved.check ?? null);
      // A check that shut the gate says why in the author's own sentences, and those are drawn beside the
      // boxes they are about — so the form is opened rather than left as a button someone has to find.
      if (moved.check && !moved.check.ok) setOpen(true);
    } catch (e) {
      // A refusal is core's — a build this amenbo cannot speak to, a `required` setting this project
      // has no value for — and it is the sentence worth showing beside the place to fix it.
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="plugcross">
      <div className="pluggate">
        <span className="chip">{name}</span>
        {at.enabled && <span className="chip">{t("plugins.enabledChip")}</span>}
        {/* What the settings at this crossing amount to, in one word: the refusal waiting to happen, or
            that something is filled in — which is also why an off project is on the list at all. */}
        {at.requiredUnset ? (
          <span className="chip chip--warn">{t("plugins.cfg.requiredEmpty")}</span>
        ) : (
          at.hasValue && <span className="chip">{t("plugins.cfg.filled")}</span>
        )}
        <button
          className="feed__action"
          disabled={busy || (!at.enabled && !install.compatible)}
          onClick={() => void move(!at.enabled)}
        >
          {t(at.enabled ? "plugins.disable" : "plugins.enable")}
        </button>
        {/* Only for a plugin whose author declared any — an empty form is the form's own answer to
            whether there is anything to configure, and it is not worth a button. Operations count as
            something to open it for: a plugin with a button and no field still has a form. */}
        {(install.config.length > 0 || install.actions.length > 0) && (
          <button className="feed__action" onClick={() => setOpen((s) => !s)}>
            {t(open ? "plugins.cfg.hide" : "plugins.cfg.open")}
          </button>
        )}
        {/* The one thing a disable does that cannot be undone: those events are not delivered late, and
            re-enabling starts from now (`AMB-D-399`). The CLI has always said it; this is the same line. */}
        {dropped > 0 && <div className="pluggate__note">{tn("plugins.droppedQueued", dropped)}</div>}
        {error && <div className="pluggate__note">{error}</div>}
      </div>
      {/* A refusal is a fact about one attempt, so a write to this crossing's settings retires it: the
          sentence names the values that were missing when the switch was pressed, and after a save it
          would be a row saying its settings are filled in and unfillable in the same breath. What may be
          enabled now is answered by pressing again, which is the one thing that can answer it. */}
      {open && (
        <PluginConfigForm
          install={install}
          layer={layer}
          enabled={at.enabled}
          check={check}
          onWrote={() => {
            setError(null);
            // The verdict is about the values that were there when the switch was pressed, so a save
            // outlives it exactly as the refusal above does — and what is true now is answered by
            // pressing again.
            setCheck(null);
          }}
        />
      )}
    </div>
  );
}
