import { useState } from "react";
import { errText, t, tf } from "../core/i18n";
import { setPluginConfig, type PluginConfigField, type PluginInstall } from "../core/pluginInstalls";

/**
 * The settings of one installed plugin, drawn from the schema its author declared (`AMB-D-356`).
 *
 * **The form is generic and amenbo judges nothing in it.** There are two kinds of field and no more —
 * a text box, and a masked pair for a secret — because the manifest carries a flag, not a type: what a
 * value must look like is the plugin author's to check at run time, and what amenbo enforces is the
 * floor under any value (a byte cap, no control characters) at the one write boundary every face
 * shares.
 *
 * **A secret is written, never read back.** It lives in the user-area secret file, off the store and
 * off every backup, so the form has only "held / not held" to draw — which is why setting one asks for
 * it twice: with nothing to compare against afterwards, the second box is the only check on a typo.
 * The tier switch does not apply to it either; there is one secret per key for the device.
 *
 * **Text has the two tiers, and the switch edits one of them.** A project override sits on top of the
 * machine default, so an empty override is not an empty value — it is no override, and the default
 * below shows through. The form says which one it is writing rather than resolving them into a single
 * effective value nobody could then clear.
 *
 * The tiers are the settings' own, not the gate's: which project a plugin *fires* in is the author's
 * declaration (`AMB-D-379`), while a project override is something any plugin's text setting can carry.
 * So the project is named here even for a plugin whose switch is the device's.
 */
export function PluginConfigForm({ install, projects, projectId, onProject }: {
  install: PluginInstall;
  /** The projects an override can be written for — the store's, for the picker. */
  projects: { id: number; name: string }[];
  /** The project this screen speaks for — whose override the form writes (`null` = none chosen). */
  projectId: number | null;
  onProject: (id: number | null) => void;
}) {
  const [scope, setScope] = useState<"machine" | "project">("machine");
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

  const tier = scope === "project" ? projectId : null;
  const onProjectTier = scope === "project";
  // The project tier with no project named writes nowhere: falling back to the machine default here
  // would put a value in the tier every project reads, which is the opposite of what was asked for.
  const unnamedProject = onProjectTier && projectId == null;
  const stored = (f: PluginConfigField) =>
    (onProjectTier ? f.projectValue : f.machineValue) ?? "";
  const shown = (f: PluginConfigField) => edits[f.key] ?? stored(f);

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
          await setPluginConfig(install.name, f.key, pair.value, null);
        } else {
          const next = edits[f.key];
          if (next === undefined || next === stored(f)) continue;
          await setPluginConfig(install.name, f.key, next, tier);
        }
      }
      setEdits({});
      setSecrets({});
    }, "plugins.cfg.saved");

  const onClear = (f: PluginConfigField) =>
    run(async () => {
      await setPluginConfig(install.name, f.key, "", f.secret ? null : tier);
      setEdits((e) => ({ ...e, [f.key]: "" }));
      setSecrets((s) => ({ ...s, [f.key]: { value: "", confirm: "" } }));
    }, "plugins.cfg.cleared");

  return (
    <div className="plugcfg">
      <div className="pluggate">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("plugins.cfg.tier")}</span>
        <select
          value={scope}
          disabled={busy}
          onChange={(e) => {
            setScope(e.target.value === "project" ? "project" : "machine");
            setEdits({});
            setDone(null);
          }}
          style={{ fontSize: "var(--fs-xs)" }}
        >
          <option value="machine">{t("plugins.cfg.tier.machine")}</option>
          <option value="project">{t("plugins.cfg.tier.project")}</option>
        </select>
        {onProjectTier && (
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
        )}
        {unnamedProject && (
          <div className="pluggate__note faint">{t("plugins.cfg.pickProjectNote")}</div>
        )}
      </div>

      {install.config.map((f) => (
        <div key={f.key} className="plugcfg__field">
          <label className="plugcfg__label" htmlFor={`cfg-${install.name}-${f.key}`}>
            {f.label}
            {f.required && <span className="chip">{t("plugins.cfg.required")}</span>}
            {unset(f) ? (
              <span className={f.required ? "chip chip--warn" : "chip"}>{t("plugins.cfg.unset")}</span>
            ) : (
              f.secret && <span className="chip">{t("plugins.cfg.held")}</span>
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
                placeholder={f.secretSet ? t("plugins.cfg.secretReplace") : ""}
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
              {/* What an empty override falls back to. Drawn only where it is the answer: at the
                  machine tier the box already holds that value. */}
              {onProjectTier && f.projectValue == null && f.machineValue != null && (
                <div className="faint plugcfg__note">
                  {tf("plugins.cfg.fallback", { value: f.machineValue })}
                </div>
              )}
            </>
          )}
          {held(f, onProjectTier) && (
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
 * Whether a field has no value anywhere — the state an enable is refused for while the author marked it
 * required. A text field is held by either tier, since the machine default is what a project without an
 * override runs on.
 */
function unset(f: PluginConfigField): boolean {
  return f.secret ? !f.secretSet : f.machineValue == null && f.projectValue == null;
}

/** Whether *this* tier holds something to clear — clearing a tier that holds nothing is a no-op. */
function held(f: PluginConfigField, onProjectTier: boolean): boolean {
  if (f.secret) return f.secretSet;
  return (onProjectTier ? f.projectValue : f.machineValue) != null;
}
