import { useState } from "react";
import type {
  PluginCheckDto,
  PluginFormEntryDto,
  PluginWantedSettingDto,
} from "../bindings/bindings";
import { errText, t } from "../core/i18n";
import { PluginShowParts } from "./PluginShowParts";
import { asTyped } from "../core/keys";
import {
  NONE_SELECTED,
  checkPluginSettings,
  formFields,
  runPluginAction,
  setPluginConfig,
  usePluginConfig,
  whenShows,
  type PluginAction,
  type PluginActionRan,
  type PluginConfigField,
  type PluginInstall,
  type PluginLayer,
} from "../core/pluginInstalls";
import {
  actionLabel,
  askLabel,
  optionLabel,
  settingHelp,
  settingLabel,
  settingPlaceholder,
} from "../core/pluginText";

/**
 * The settings of one installed plugin, drawn from the schema its author declared (`AMB-D-356`).
 *
 * **A field that offers candidates is drawn as its candidates** (`AMB-D-415`), and carries three answers
 * rather than two: the boxes someone ticked, *none of them* — an answer of its own, and not the same as
 * silence — and nobody having answered, where the author's default is what runs and the form says so. The
 * way back to that last one is the same button that empties any other field, wearing the name of what it
 * does here.
 *
 * **The form is generic and Amenbo judges nothing in it.** What the author declares is the *shape of the
 * answer* — a line, a masked pair for a secret, a set of candidates — never what a value must look like:
 * that stays the plugin author's to check at run time, and what Amenbo enforces is the floor under any
 * value (a byte cap, no control characters) at the one write boundary every face shares.
 *
 * **A secret is written, never read back.** It never leaves core — the form has only "held / not held"
 * to draw — which is why setting one asks for it twice: with nothing to compare against afterwards, the
 * second box is the only check on a typo.
 *
 * **The author may say more than a caption** (`AMB-D-656`): a paragraph under the input, an example
 * inside an empty one, and — for a value their own plugin writes back — no input at all. What they wrote
 * is drawn as plain text, with no Markdown and no link: a form asking for a credential is the last screen
 * on which to offer somewhere its author chose to go.
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
 * **The author's own code speaks on this screen and nowhere else** (`AMB-D-664`). Three things reach it:
 * what their `check` said about the values — one sentence at the head, and one beside each box it named —
 * the operations they declared, drawn as buttons, and the parts either of those asked to have drawn
 * (`AMB-D-727`). Every string in all three is plain text for the reason everything else here is, and a
 * press runs a call the manifest named rather than anything this form composed (`AMB-D-522`).
 *
 * **A part is drawn where its author put it.** The manifest's own parts are entries in the same `config`
 * list the settings are, so the way to the page that issues a token stands above the box the token goes
 * in; a run's arrive under the button that raised it, and only the last press's are on screen at once.
 *
 * **The check is raised twice, and only one of them can refuse anything.** At the switch it decides whether
 * the gate opens; here, after a save at a crossing the plugin is already on, it decides nothing at all —
 * the value stays written and the plugin stays on, and what the run is for is the sentence beside the box
 * someone can still go back and fix.
 *
 * **A button needs an open gate.** Running the plugin's code is what enabling means (`AMB-D-351`), so the
 * operations are drawn but not pressable while the crossing is off, with a line saying which.
 *
 * The author's schema comes from the install (it is the same wherever you stand) and what is *held* is
 * read for the named layer, which is why the two arrive separately.
 */
export function PluginConfigForm({ install, layer, enabled, check, onWrote }: {
  install: PluginInstall;
  /**
   * The layer whose values are being written — the row this form was opened inside. `null` is the
   * device's own row, for a plugin its author declared the machine's (`AMB-D-601`).
   */
  layer: PluginLayer;
  /**
   * Whether the plugin fires at this crossing — what decides if its operations may be pressed
   * (`AMB-D-664`). The row knows; the form does not read the gate a second time.
   */
  enabled: boolean;
  /**
   * What the author's check last said about this crossing's values (`AMB-D-664`), or nothing — a plugin
   * that declares no check, or a crossing nobody has pressed or saved at this session. It is a fact about
   * one run, so it is the row's to hold and to retire.
   */
  check?: PluginCheckDto | null;
  /**
   * Called once a write to this crossing lands, so the row around the form can retire what a value
   * standing here made out of date — the refusal an enable met over a `required` setting this form has
   * just filled in.
   *
   * What it is handed is the check raised on the values as they now stand, which replaces the one the
   * switch left: `null` where nothing was raised — an off crossing, a plugin declaring no check.
   */
  onWrote?: (check: PluginCheckDto | null) => void;
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
  // Which operation is being pressed, which one has its boxes open, what has been typed into them, and
  // what the last press of each one came back with. The typed values are wiped on every open and every
  // press: this face is the one place a value is allowed to exist, and it does not outlive the run.
  const [pressing, setPressing] = useState<string | null>(null);
  const [asking, setAsking] = useState<string | null>(null);
  const [asked, setAsked] = useState<Record<string, string>>({});
  // The last press and what it answered — **one at a time** (`AMB-D-727`). What a run puts on the form
  // is about the run: pressing the same button again, or a different one, replaces it, and leaving the
  // screen takes it with the form. A second button's answer standing beside the first would read as one
  // thing said twice.
  const [ran, setRan] = useState<{ cmd: string; outcome: PluginActionRan } | null>(null);
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

  // Where every setting stands *on screen*, which is what its author's conditions are read against
  // (`AMB-D-727`): what was typed, else what the layer holds, else the default behind it. This is the half
  // of a `when` core cannot answer — the store has not been told about a box that is still being filled in,
  // so someone ticking Cloudflare would otherwise wait for a save to see its fields.
  const answers: Record<string, string> = {};
  for (const f of formFields(install.config)) {
    const answer = f.fieldType === "multi" ? ticked(f).join(",") : shown(f) || f.defaultValue || "";
    if (answer !== "") answers[f.key] = answer;
  }
  // The form as it stands, **in the author's declared order** (`AMB-D-727`): the settings whose conditions
  // hold, each keeping only the candidates whose own do, with the parts they wrote between them read the
  // same way and left where they stand — a form that hid Cloudflare's fields but kept its button, or its
  // caption, would leave a step nobody could follow. The operations go the same way.
  const drawn: PluginFormEntryDto[] = install.config.flatMap((entry): PluginFormEntryDto[] => {
    if (entry.kind === "part") return whenShows(entry.when, answers) ? [entry] : [];
    const f = entry.field;
    if (!whenShows(f.when, answers)) return [];
    const options = f.options.filter((o) => whenShows(o.when, answers));
    return [{ ...entry, field: { ...f, options } }];
  });
  const actions = install.actions.filter((a) => whenShows(a.when, answers));

  // What the author's check says about the values a write has just left behind (`AMB-D-664`), or nothing
  // — a plugin that declares no check, and a crossing whose gate is shut, which core is the one to answer
  // (running their code is what enabling means, `AMB-D-351`).
  //
  // A refusal to raise it at all is swallowed on purpose: the write has already landed by then, and a save
  // that succeeded must not wear an error over what was asked about it afterwards. The run is on the
  // execution log either way (`AMB-D-361`).
  const checkAfterWrite = async (): Promise<PluginCheckDto | null> => {
    try {
      return await checkPluginSettings(install.name, layer);
    } catch {
      return null;
    }
  };

  const run = async (op: () => Promise<unknown>, said: typeof done) => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      await op();
      setDone(said);
      // Both doors are a write — a value saved, and one taken back — and either leaves whatever was said
      // about this crossing's settings out of date. What replaces it is the check raised on what stands
      // there now: after the write, and never in place of it (`AMB-D-664`).
      onWrote?.(await checkAfterWrite());
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  // One boundary call per field that changed. They are sequential on purpose: each is its own write,
  // and a refusal on the third must leave the first two written rather than roll back a store Amenbo
  // never promised to.
  const onSave = () =>
    run(async () => {
      for (const f of formFields(install.config)) {
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

  // Press one operation: the boxes it asked for go with the call and are dropped on the way back, whether
  // it succeeded or not. A refusal — a shut gate, a plugin that will not start — is a sentence like any
  // other error here; a run that failed is an answer and is drawn as one.
  const onPress = async (a: PluginAction) => {
    setPressing(a.cmd);
    setError(null);
    // Whatever the last press left goes now, not when this one answers: what is on screen while a run is
    // in flight must not read as this run's.
    setRan(null);
    try {
      const outcome = await runPluginAction(install.name, a.cmd, asked, layer);
      setRan({ cmd: a.cmd, outcome });
      setAsking(null);
      setAsked({});
    } catch (e) {
      setError(errText(e));
    } finally {
      setPressing(null);
    }
  };

  return (
    <div className="plugcfg">
      {/* What the author's check said about these settings as a whole (`AMB-D-664`), at the head of the
          form because that is what it is about. A silence has no sentence of theirs in it, so Amenbo says
          what happened in its own words and leaves the reason to the run log. */}
      {check && !check.ok && (
        <div className="pluggate__note">
          {check.answered ? check.message ?? t("plugins.check.refused") : t("plugins.check.noAnswer")}
        </div>
      )}
      {check?.ok && check.message && <div className="plugcfg__note">{check.message}</div>}
      {/* And what the check asked to have drawn (`AMB-D-727`) — the way to the page that issues the value
          it is refusing over, most often, which is why it stands with its sentences. */}
      {check && <PluginShowParts parts={check.show} />}
      {/* The author's declared order, parts and settings together (`AMB-D-727`): where a part sits is what
          it is for — the way to the page that issues a token belongs above the box the token goes in. */}
      {drawn.map((entry, at) => {
        if (entry.kind === "part") {
          return <PluginShowParts key={`part-${at}`} parts={[entry.part]} />;
        }
        const f = entry.field;
        return (
        <div key={f.key} className="plugcfg__field">
          {/* A choice is a group of boxes, each with its own label, so the caption above it names the
              group rather than pointing at one input. */}
          <label
            className="fieldlabel plugcfg__label"
            htmlFor={
              f.fieldType === "multi" || f.readonly
                ? undefined
                : `cfg-${install.name}-${f.key}`
            }
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
              <span className={f.required ? "chip chip--heed" : "chip"}>
                {t("plugins.cfg.unset")}
              </span>
            )}
          </label>
          {f.readonly ? (
            /* The plugin writes this one (`AMB-D-656`), so there is nothing to type: the value stands
               where its input would, and the clear button below is not drawn either. A secret readonly
               field is down to the chip above — its value never leaves core, so there is none to show. */
            !f.secret && stored(f) !== "" && <div className="plugcfg__fixed">{stored(f)}</div>
          ) : f.secret ? (
            <>
              <input
                {...asTyped}
                id={`cfg-${install.name}-${f.key}`}
                type="password"
                autoComplete="new-password"
                disabled={busy}
                value={secrets[f.key]?.value ?? ""}
                placeholder={
                  heldFor(f.key)?.secretSet
                    ? t("plugins.cfg.secretReplace")
                    : settingPlaceholder(f) ?? ""
                }
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
              /* How wide the box is drawn is read off what the author already wrote: a default or an
                 example short enough to be a port or a flag gets a box that size, and everything else
                 takes the form's width (`AMB-D-842`). */
              className={shortHint(f) ? "plugcfg__short" : undefined}
              size={shortHint(f)?.length}
              value={shown(f)}
              /* An empty box under a "default" chip would leave the value in force unreadable — the
                 candidates of a choice are ticked, and this is the same showing for a line. The
                 author's example takes the slot only where there is no default to show in it: a
                 default is the value a run really receives, and an example is not (`AMB-D-656`). */
              placeholder={f.defaultValue ?? settingPlaceholder(f) ?? ""}
              onChange={(e) => setEdits((s) => ({ ...s, [f.key]: e.target.value }))}
            />
          )}
          {/* The author's own paragraph about this field (`AMB-D-656`), under the input it explains.
              Text and nothing else: the newlines are theirs, and no Markdown or link is drawn from it —
              this is the screen a secret is typed into. */}
          {settingHelp(f) && <div className="hint plugcfg__help">{settingHelp(f)}</div>}
          {/* What the author's check said about *this* box (`AMB-D-664`) — beside the one it named, which
              is the whole reason a verdict carries keys at all. Plain text, like their paragraph above. */}
          {check?.fields[f.key] && (
            <div className="pluggate__note">{check.fields[f.key]}</div>
          )}
          {!f.readonly && held(heldFor(f.key)) && (
            <button
              className="btn"
              disabled={busy}
              onClick={() => void onClear(f)}
            >
              {/* The same write either way — an empty value — said as what it does here: a field with a
                  default is not emptied by it, it goes back to what the author put behind it. */}
              {f.defaultValue != null ? t("plugins.cfg.restoreDefault") : t("plugins.cfg.clear")}
            </button>
          )}
        </div>
        );
      })}

      <div className="pluggate">
        <button className="btn" disabled={busy} onClick={() => void onSave()}>
          {busy ? t("plugins.cfg.saving") : t("plugins.cfg.save")}
        </button>
        {done && !busy && (
          <span className="meta">{t(done)}</span>
        )}
        {error && <div className="pluggate__note">{error}</div>}
      </div>

      {/* The operations their author declared (`AMB-D-664`). Drawn under the fields because that is what
          they act on, and only for a plugin that declared any — a form with none is the form it was. */}
      {actions.length > 0 && (
        <div className="plugcfg__acts">
          <div className="faint plugcfg__note">{t("plugins.act.title")}</div>
          {!enabled && <div className="plugcfg__note">{t("plugins.act.needsEnabled")}</div>}
          {actions.map((a) => (
            <div key={a.cmd} className="plugcfg__act">
              <button
                className="btn"
                disabled={!enabled || pressing !== null}
                onClick={() => {
                  // A press that asks for nothing runs; one that asks opens its boxes first, empty every
                  // time — nothing here remembers what was typed into them last.
                  if (a.ask.length === 0) return void onPress(a);
                  setAsked({});
                  setAsking((s) => (s === a.cmd ? null : a.cmd));
                }}
              >
                {pressing === a.cmd ? t("plugins.act.running") : actionLabel(a)}
              </button>
              {asking === a.cmd && (
                <div className="plugcfg__ask">
                  {a.ask.map((f) => (
                    <label key={f.key} className="plugcfg__field">
                      <span className="fieldlabel plugcfg__label">{askLabel(f)}</span>
                      <input
                        {...asTyped}
                        type={f.secret ? "password" : "text"}
                        autoComplete={f.secret ? "new-password" : "off"}
                        disabled={pressing !== null}
                        value={asked[f.key] ?? ""}
                        onChange={(e) =>
                          setAsked((s) => ({ ...s, [f.key]: e.target.value }))
                        }
                      />
                    </label>
                  ))}
                  {/* The one thing worth saying about these boxes: what is typed into them goes to this
                      run and is kept nowhere, which is what makes them different from the form above. */}
                  <div className="faint plugcfg__note">{t("plugins.act.askNote")}</div>
                  {/* Run and cancel side by side, the way every other form in the app closes: the one
                      that runs carries the accent, the one that backs out is the plain button. */}
                  <div className="plugcfg__askacts">
                    <button
                      className="btn btn--primary"
                      disabled={pressing !== null}
                      onClick={() => void onPress(a)}
                    >
                      {t("plugins.act.run")}
                    </button>
                    <button
                      className="btn"
                      disabled={pressing !== null}
                      onClick={() => {
                        setAsking(null);
                        setAsked({});
                      }}
                    >
                      {t("plugins.act.cancel")}
                    </button>
                  </div>
                </div>
              )}
              {/* What the press did: the author's own line where they wrote one, and Amenbo's word for it
                  where they did not — and then whatever they asked to have drawn (`AMB-D-727`), which is
                  where a QR to hold a phone up to or an address to copy arrives. */}
              {ran?.cmd === a.cmd && pressing !== a.cmd && (
                <>
                  <div className={ran.outcome.ok ? "faint plugcfg__note" : "pluggate__note"}>
                    {ran.outcome.message ??
                      t(ran.outcome.ok ? "plugins.act.ok" : "plugins.act.failed")}
                  </div>
                  <PluginShowParts parts={ran.outcome.show} />
                </>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * The width hint the author's own writing gives a box: a default or an example short enough to be a
 * port number or a flag. Anything longer says nothing about width — a token and a URL are both just
 * "long" — so the box takes the form's width instead.
 */
function shortHint(f: PluginWantedSettingDto): string | undefined {
  const hint = f.defaultValue ?? settingPlaceholder(f) ?? "";
  return hint.length > 0 && hint.length <= 12 ? hint : undefined;
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
