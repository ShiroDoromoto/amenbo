import { useState } from "react";
import type { PluginWantedSettingDto } from "../bindings/bindings";
import { errText, t } from "../core/i18n";
import {
  setPluginConfig,
  usePluginConfig,
  type PluginConfigField,
  type PluginInstall,
} from "../core/pluginInstalls";

/**
 * The settings of one installed plugin, drawn from the schema its author declared (`AMB-D-356`).
 *
 * **The form is generic and amenbo judges nothing in it.** There are two kinds of field and no more —
 * a text box, and a masked pair for a secret — because the manifest carries a flag, not a type: what a
 * value must look like is the plugin author's to check at run time, and what amenbo enforces is the
 * floor under any value (a byte cap, no control characters) at the one write boundary every face
 * shares.
 *
 * **A secret is written, never read back.** It never leaves core — the form has only "held / not held"
 * to draw — which is why setting one asks for it twice: with nothing to compare against afterwards, the
 * second box is the only check on a typo.
 *
 * **Every value is one project's** (`AMB-D-434`), secret or not, so the form edits one project and says
 * which. That is not the gate: which project a plugin *fires* in is its enable row, while a setting is
 * something any plugin can carry in a project it is off in — so the project is named here whether or
 * not the plugin is on in it.
 *
 * The author's schema comes from the install (it is the same wherever you stand) and what is *held*
 * is read for the named project, which is why the two arrive separately: until a project is picked
 * there is a form to draw and nothing to draw in it.
 */
export function PluginConfigForm({ install, projects, projectId, onProject }: {
  install: PluginInstall;
  /** The projects an override can be written for — the store's, for the picker. */
  projects: { id: number; name: string }[];
  /** The project this screen speaks for — whose override the form writes (`null` = none chosen). */
  projectId: number | null;
  onProject: (id: number | null) => void;
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
  const { fields } = usePluginConfig(install.name, projectId);

  // With no project named there is nowhere to write: a setting belongs to a project and to nothing else.
  const unnamedProject = projectId == null;
  // What that project holds for a key — absent while no project is named, which is not "unset".
  const heldFor = (key: string): PluginConfigField | undefined => fields.find((f) => f.key === key);
  const stored = (f: PluginWantedSettingDto) => heldFor(f.key)?.value ?? "";
  const shown = (f: PluginWantedSettingDto) => edits[f.key] ?? stored(f);

  const run = async (op: () => Promise<unknown>, said: typeof done) => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      await op();
      setDone(said);
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
          await setPluginConfig(install.name, f.key, pair.value, projectId);
        } else {
          const next = edits[f.key];
          if (next === undefined || next === stored(f)) continue;
          await setPluginConfig(install.name, f.key, next, projectId);
        }
      }
      setEdits({});
      setSecrets({});
    }, "plugins.cfg.saved");

  const onClear = (f: PluginWantedSettingDto) =>
    run(async () => {
      await setPluginConfig(install.name, f.key, "", projectId);
      setEdits((e) => ({ ...e, [f.key]: "" }));
      setSecrets((s) => ({ ...s, [f.key]: { value: "", confirm: "" } }));
    }, "plugins.cfg.cleared");

  return (
    <div className="plugcfg">
      <div className="pluggate">
        <select
          value={projectId ?? ""}
          disabled={busy}
          onChange={(e) => {
            onProject(e.target.value === "" ? null : Number(e.target.value));
            setEdits({});
            setDone(null);
          }}
          style={{ fontSize: "var(--fs-xs)" }}
        >
          <option value="">{t("plugins.cfg.pickProject")}</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
        {unnamedProject && (
          <div className="pluggate__note faint">{t("plugins.cfg.pickProjectNote")}</div>
        )}
      </div>

      {install.config.map((f) => (
        <div key={f.key} className="plugcfg__field">
          <label className="plugcfg__label" htmlFor={`cfg-${install.name}-${f.key}`}>
            {f.label}
            {f.required && <span className="chip">{t("plugins.cfg.required")}</span>}
            {held(heldFor(f.key)) ? (
              f.secret && <span className="chip">{t("plugins.cfg.held")}</span>
            ) : (
              !unnamedProject && (
                <span className={f.required ? "chip chip--warn" : "chip"}>
                  {t("plugins.cfg.unset")}
                </span>
              )
            )}
          </label>
          {f.secret ? (
            <>
              <input
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
              <div className="faint plugcfg__note">{t("plugins.cfg.secretNote")}</div>
            </>
          ) : (
            <>
              <input
                id={`cfg-${install.name}-${f.key}`}
                type="text"
                disabled={busy}
                value={shown(f)}
                onChange={(e) => setEdits((s) => ({ ...s, [f.key]: e.target.value }))}
              />
            </>
          )}
          {held(heldFor(f.key)) && (
            <button
              className="feed__action"
              disabled={busy || unnamedProject}
              onClick={() => void onClear(f)}
            >
              {t("plugins.cfg.clear")}
            </button>
          )}
        </div>
      ))}

      <div className="pluggate">
        <button className="btn" disabled={busy || unnamedProject} onClick={() => void onSave()}>
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
