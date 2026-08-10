import { useState } from "react";
import type { PluginWantedSettingDto } from "../bindings/bindings";
import { errText, t } from "../core/i18n";
import { asTyped } from "../core/keys";
import {
  NONE_SELECTED,
  setPluginConfig,
  usePluginConfig,
  type PluginConfigField,
  type PluginInstall,
  type PluginLayer,
} from "../core/pluginInstalls";
import { optionLabel, settingLabel } from "../core/pluginText";

/**
 * The settings of one installed plugin, drawn from the schema its author declared (`AMB-D-356`).
 *
 * **A field that offers candidates is drawn as its candidates** (`AMB-D-415`), and carries three answers
 * rather than two: the boxes someone ticked, *none of them* — an answer of its own, and not the same as
 * silence — and nobody having answered, where the author's default is what runs and the form says so. The
 * way back to that last one is the same button that empties any other field, wearing the name of what it
 * does here.
 *
 * **The form is generic and amenbo judges nothing in it.** What the author declares is the *shape of the
 * answer* — a line, a masked pair for a secret, a set of candidates — never what a value must look like:
 * that stays the plugin author's to check at run time, and what amenbo enforces is the floor under any
 * value (a byte cap, no control characters) at the one write boundary every face shares.
 *
 * **A secret is written, never read back.** It never leaves core — the form has only "held / not held"
 * to draw — which is why setting one asks for it twice: with nothing to compare against afterwards, the
 * second box is the only check on a typo.
 *
 * **Every value belongs to one layer** (`AMB-D-434` / `AMB-D-601`), secret or not — a project's rows, or
 * the device's for a plugin its author declared the machine's — and **which one is the caller's to say**
 * (`AMB-D-447`): the form is drawn inside a row, and that row has already answered. A picker of its own
 * would be a second place to choose on a screen that has one, and the two would not agree.
 *
 * That is not the gate: which project a plugin *fires* in is its enable row, while a setting is
 * something any plugin can carry in a project it is off in — so a crossing has a form whether or not
 * the plugin is on there.
 *
 * The author's schema comes from the install (it is the same wherever you stand) and what is *held* is
 * read for the named layer, which is why the two arrive separately.
 */
export function PluginConfigForm({ install, layer, onWrote }: {
  install: PluginInstall;
  /**
   * The layer whose values are being written — the row this form was opened inside. `null` is the
   * device's own row, for a plugin its author declared the machine's (`AMB-D-601`).
   */
  layer: PluginLayer;
  /**
   * Called once a write to this crossing lands, so the row around the form can retire what a value
   * standing here made out of date — the refusal an enable met over a `required` setting this form has
   * just filled in.
   */
  onWrote?: () => void;
}) {
  // Only what the user actually typed, keyed by field. Kept apart from the stored values so a refetch
  // never argues with a half-typed box, and so a cleared box reads as "clear this", not as "unchanged".
  const [edits, setEdits] = useState<Record<string, string>>({});
  // A secret's two boxes. Empty means "leave what is held alone" — a secret is replaced or cleared, not
  // edited, because nothing here ever saw the value.
  const [secrets, setSecrets] = useState<Record<string, { value: string; confirm: string }>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // What the last write did, said back in its own words: a clear removed a value, a save wrote one.
  const [done, setDone] = useState<"plugins.cfg.saved" | "plugins.cfg.cleared" | null>(null);
  const { fields } = usePluginConfig(install.name, layer);

  // What that project holds for a key — absent until the read lands, which is not "unset".
  const heldFor = (key: string): PluginConfigField | undefined => fields.find((f) => f.key === key);
  const stored = (f: PluginWantedSettingDto) => heldFor(f.key)?.value ?? "";
  const shown = (f: PluginWantedSettingDto) => edits[f.key] ?? stored(f);
  // Which candidates are in force on screen: what is being edited, what the project holds, or — while
  // nobody has answered — the author's default, which is what a run would receive as things stand.
  const ticked = (f: PluginWantedSettingDto): string[] => {
    const edit = edits[f.key];
    // An edit of "" is the clear this form writes, and for a choice the clear *is* "back to the default"
    // — so the boxes go back to the author's, not blank. Blank is the answer the reserved word carries.
    const answer =
      edit === "" ? f.defaultValue ?? "" : edit ?? heldFor(f.key)?.value ?? f.defaultValue ?? "";
    return answer === "" || answer === NONE_SELECTED ? [] : answer.split(",");
  };

  const run = async (op: () => Promise<unknown>, said: typeof done) => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      await op();
      setDone(said);
      // Both doors are a write — a value saved, and one taken back — and either leaves whatever was said
      // about this crossing's settings out of date.
      onWrote?.();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  // One boundary call per field that changed. They are sequential on purpose: each is its own write,
  // and a refusal on the third must leave the first two written rather than roll back a store amenbo
  // never promised to.
  const onSave = () =>
    run(async () => {
      for (const f of install.config) {
        if (f.secret) {
          const pair = secrets[f.key];
          if (!pair || pair.value === "") continue;
          if (pair.value !== pair.confirm) throw new Error(t("plugins.cfg.secretMismatch"));
          await setPluginConfig(install.name, f.key, pair.value, layer);
        } else {
          const next = edits[f.key];
          if (next === undefined || next === stored(f)) continue;
          await setPluginConfig(install.name, f.key, next, layer);
        }
      }
      setEdits({});
      setSecrets({});
    }, "plugins.cfg.saved");

  const onClear = (f: PluginWantedSettingDto) =>
    run(async () => {
      await setPluginConfig(install.name, f.key, "", layer);
      setEdits((e) => ({ ...e, [f.key]: "" }));
      setSecrets((s) => ({ ...s, [f.key]: { value: "", confirm: "" } }));
    }, "plugins.cfg.cleared");

  return (
    <div className="plugcfg">
      {install.config.map((f) => (
        <div key={f.key} className="plugcfg__field">
          {/* A choice is a group of boxes, each with its own label, so the caption above it names the
              group rather than pointing at one input. */}
          <label
            className="plugcfg__label"
            htmlFor={f.fieldType === "multi" ? undefined : `cfg-${install.name}-${f.key}`}
          >
            {settingLabel(f)}
            {f.required && <span className="chip">{t("plugins.cfg.required")}</span>}
            {/* Which of the three answers this field is giving (`AMB-D-415`). "Nobody answered" is drawn
                as the default where the author wrote one — the value a run receives is not missing — and
                "none of them" is drawn as itself, since empty boxes alone would read as unanswered. */}
            {heldFor(f.key)?.state === "none" ? (
              <span className="chip">{t("plugins.cfg.noneChosen")}</span>
            ) : held(heldFor(f.key)) ? (
              f.secret && <span className="chip">{t("plugins.cfg.held")}</span>
            ) : f.defaultValue != null ? (
              <span className="chip">{t("plugins.cfg.default")}</span>
            ) : (
              <span className={f.required ? "chip chip--warn" : "chip"}>
                {t("plugins.cfg.unset")}
              </span>
            )}
          </label>
          {f.secret ? (
            <>
              <input
                {...asTyped}
                id={`cfg-${install.name}-${f.key}`}
                type="password"
                autoComplete="new-password"
                disabled={busy}
                value={secrets[f.key]?.value ?? ""}
                placeholder={heldFor(f.key)?.secretSet ? t("plugins.cfg.secretReplace") : ""}
                onChange={(e) =>
                  setSecrets((s) => ({
                    ...s,
                    [f.key]: { value: e.target.value, confirm: s[f.key]?.confirm ?? "" },
                  }))
                }
              />
              <input
                {...asTyped}
                type="password"
                autoComplete="new-password"
                disabled={busy}
                value={secrets[f.key]?.confirm ?? ""}
                placeholder={t("plugins.cfg.secretConfirm")}
                onChange={(e) =>
                  setSecrets((s) => ({
                    ...s,
                    [f.key]: { value: s[f.key]?.value ?? "", confirm: e.target.value },
                  }))
                }
              />
              {/* Where the secret is kept, said as the layer this form is writing at (`AMB-D-601`):
                  a project's row, or the device's. It is the one line here that names the place, so a
                  device row wearing the project's wording would be telling a reader their token went
                  somewhere it did not. */}
              <div className="faint plugcfg__note">
                {t(layer == null ? "plugins.cfg.secretNoteDevice" : "plugins.cfg.secretNote")}
              </div>
            </>
          ) : f.fieldType === "multi" ? (
            /* The candidates, in the author's order. Unticking the last box does not empty the field —
               it writes the word for "none of them", because empty is where an unanswered field already
               lives and the two answers must not collapse into one. */
            <div className="plugcfg__choices">
              {f.options.map((o) => (
                <label key={o.value} className="plugcfg__choice">
                  <input
                    type="checkbox"
                    disabled={busy}
                    checked={ticked(f).includes(o.value)}
                    onChange={(e) => {
                      const next = e.target.checked
                        ? [...ticked(f), o.value]
                        : ticked(f).filter((v) => v !== o.value);
                      setEdits((s) => ({
                        ...s,
                        [f.key]: next.length === 0 ? NONE_SELECTED : next.join(","),
                      }));
                    }}
                  />
                  {optionLabel(o)}
                </label>
              ))}
            </div>
          ) : (
            <input
              {...asTyped}
              id={`cfg-${install.name}-${f.key}`}
              type="text"
              disabled={busy}
              value={shown(f)}
              /* An empty box under a "default" chip would leave the value in force unreadable — the
                 candidates of a choice are ticked, and this is the same showing for a line. */
              placeholder={f.defaultValue ?? ""}
              onChange={(e) => setEdits((s) => ({ ...s, [f.key]: e.target.value }))}
            />
          )}
          {held(heldFor(f.key)) && (
            <button
              className="feed__action"
              disabled={busy}
              onClick={() => void onClear(f)}
            >
              {/* The same write either way — an empty value — said as what it does here: a field with a
                  default is not emptied by it, it goes back to what the author put behind it. */}
              {f.defaultValue != null ? t("plugins.cfg.restoreDefault") : t("plugins.cfg.clear")}
            </button>
          )}
        </div>
      ))}

      <div className="pluggate">
        <button className="btn" disabled={busy} onClick={() => void onSave()}>
          {busy ? t("plugins.cfg.saving") : t("plugins.cfg.save")}
        </button>
        {done && !busy && (
          <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t(done)}</span>
        )}
        {error && <div className="pluggate__note">{error}</div>}
      </div>
    </div>
  );
}

/**
 * Whether the named project holds a value for the field — and so whether there is anything to clear
 * (clearing a field that holds nothing is a no-op). A field that was never read for a project holds
 * nothing that can be said about it, which is not the same as holding no value.
 */
export function held(f: PluginConfigField | undefined): boolean {
  if (!f) return false;
  return f.secret ? f.secretSet : f.value != null;
}
