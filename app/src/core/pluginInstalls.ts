// What this machine holds, beside the catalog it is browsing — the install/enable seam (`AMB-D-351`).
//
// The market's other seam (`pluginCatalog`) is about what *exists*; this one is about what is *here*, and
// the two are deliberately separate fetches: the catalog is a static file off the network, the installs are
// a directory on disk plus this store, and joining them is the screen's job (by name). So a catalog that
// cannot be reached still draws the plugins you have, and a machine with nothing installed costs one cheap
// local read.
//
// **Install is not enable** (`AMB-D-351`), so nothing here folds the two: `installPlugin` lands a plugin
// inert, and `setPluginEnabled` is the separate act that lets it run — which is itself the permission to
// run somebody else's code (`AMB-D-434`), so there is no second answer kept beside it.
import { useSyncExternalStore } from "react";
import { currentLang } from "./i18n";
import { invoke } from "./ipc";
import { inTauri, subscribe } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type {
  PluginActionDto,
  PluginActionRanDto,
  PluginCheckDto,
  PluginConfigFieldDto,
  PluginGateMovedDto,
  PluginInstallDto,
  PluginRemovedDto,
} from "../bindings/bindings";

/** One installed plugin and where its switch stands (generated DTO). */
export type PluginInstall = PluginInstallDto;
/** One setting its author declared, and what this machine holds for it (generated DTO). */
export type PluginConfigField = PluginConfigFieldDto;
/** What an uninstall found and removed (generated DTO). */
export type PluginRemoved = PluginRemovedDto;
/** Where a moved gate ended up, and what closing it dropped (generated DTO). */
export type PluginGateMoved = PluginGateMovedDto;
/** One operation its author declared, as a button and whatever that press asks for (generated DTO). */
export type PluginAction = PluginActionDto;
/** What one press did (generated DTO). */
export type PluginActionRan = PluginActionRanDto;
/** What the author's own check said about the values (generated DTO). */
export type PluginCheck = PluginCheckDto;

/**
 * Which layer a row is drawn at — a project's id, or `null` for the device's own row (`AMB-D-601`).
 *
 * `null` **is** the device layer and not a project nobody named, which mirrors what the row is underneath
 * (a `project_id` that is NULL). Nothing here picks it: the declaration on the install does, so a face
 * reads `install.device` to know which kind of row it is drawing, and passes the answer down.
 */
export type PluginLayer = number | null;

/**
 * The three readings a row draws, whichever layer it is at (`AMB-D-447` / `AMB-D-601`) — on or off, whether
 * anything is filled in, and whether a `required` setting is empty.
 *
 * It carries no project because a device row has none, and the two kinds of row are drawn by the same
 * component: what names the row on screen is the face's, not this.
 */
export type PluginRowState = { enabled: boolean; hasValue: boolean; requiredUnset: boolean };

const NONE: PluginInstall[] = [];
const NO_FIELDS: PluginConfigField[] = [];

/**
 * What a setting stores when someone looked at its candidates and wanted none of them (`AMB-D-415`) —
 * core's reserved spelling, written by the form and read back as the `none` state. An empty value still
 * means nobody answered, so the two answers need two spellings, and no candidate may take this one.
 */
export const NONE_SELECTED = "none";

/**
 * Read what is installed, and the state of every project × plugin crossing each one has a row at
 * (Tauri: `plugin_installs`, `AMB-D-447`).
 *
 * **It is asked from nowhere in particular** (`AMB-D-412`): every row names its own projects, so no face
 * has to pick one to look through, and a plugin left on somewhere else cannot be drawn as "off". Each
 * crossing arrives whole — on or off, whether it holds a value, whether a `required` one is empty — so a
 * face draws its rows from this one read instead of asking again per project.
 *
 * `lang` is what the declared settings come back captioned in (`AMB-D-623`). It reaches no catalog: an
 * install keeps the translations it was published with beside the binary (`AMB-D-622`), so this stays the
 * one cheap local read it has always been.
 *
 * Outside Tauri — `npm run dev` in a browser — there is no plugins directory to read, so the mock says
 * nothing is installed rather than inventing state.
 */
export async function fetchPluginInstalls(lang: string): Promise<PluginInstall[]> {
  if (inTauri()) return invoke<PluginInstall[]>("plugin_installs", { lang });
  return NONE;
}

/**
 * The installs, for the market to draw over the catalog. Local and cheap: no network, no catalog fetch —
 * which is also why the language is in the key rather than resolved once at mount: re-reading the rows
 * when the reader changes language costs a directory read.
 */
export function usePluginInstalls(): {
  installs: PluginInstall[];
  loading: boolean;
  error: unknown;
} {
  const lang = useSyncExternalStore(subscribe, currentLang);
  const { data, loading, error } = useQuery<PluginInstall[]>(["plugin-installs", lang], () =>
    fetchPluginInstalls(lang),
  );
  return { installs: data ?? NONE, loading, error };
}

/** Refetch the installs — after one landed, or after a gate moved. */
function reloadInstalls(): void {
  invalidateQueries((key) => key[0] === "plugin-installs");
}

/**
 * Read what one layer holds for a plugin's declared settings (Tauri: `plugin_config_read`).
 *
 * Separate from the installs because a value belongs to a layer and an install does not (`AMB-D-434`):
 * the form draws the author's schema from the install row, and what is *held* is read for the layer the
 * row it was opened in stands at.
 *
 * A `null` layer is the device's row (`AMB-D-601`), not an unasked question — which is why it is passed
 * through rather than short-circuited. Core settles the pairing off the declaration: a `scope: project`
 * plugin asked without a project is refused there, so no blanks are invented for it here.
 */
export async function fetchPluginConfig(
  name: string,
  layer: PluginLayer,
): Promise<PluginConfigField[]> {
  if (!inTauri()) return NO_FIELDS;
  return invoke<PluginConfigField[]>("plugin_config_read", { name, projectId: layer });
}

/** What one layer holds for a plugin — the settings form's own read. */
export function usePluginConfig(
  name: string,
  layer: PluginLayer,
): { fields: PluginConfigField[]; loading: boolean } {
  const { data, loading } = useQuery<PluginConfigField[]>(["plugin-config", name, layer], () =>
    fetchPluginConfig(name, layer),
  );
  return { fields: data ?? NO_FIELDS, loading };
}

/** Refetch what a project holds — after a value was written or cleared. */
function reloadConfig(): void {
  invalidateQueries((key) => key[0] === "plugin-config");
}

/**
 * Install one plugin by name (Tauri: `plugin_install`). Every gate is core's: the catalog resolve, the
 * signature against Amenbo's own key and the checksum over the bytes served — a refusal anywhere throws
 * with the reason, and nothing is written (`AMB-D-371`).
 *
 * It lands **inert**: the row it returns is an installed plugin whose gate is open nowhere — which is
 * also why no project is named to install it (`AMB-D-412`).
 */
export async function installPlugin(name: string): Promise<PluginInstall | null> {
  if (!inTauri()) return null;
  // The reader's language, for the row that comes back — the same one every other plugin read is drawn
  // in, so the plugin that just landed is not the one row captioned in English.
  const row = await invoke<PluginInstall>("plugin_install", { name, lang: currentLang() });
  reloadInstalls();
  return row;
}

/**
 * Move one plugin's gate (Tauri: `plugin_set_enabled`), and answer where it ended up **and what closing
 * it threw away**. Enabling is fail-closed in core on compatibility and on the author's `required`
 * settings, so a refusal here is a message to show, not a state to guess at.
 *
 * Calling it to enable **is** the permission (`AMB-D-434`): turning a plugin on is what running its code
 * means, so nothing is asked beside it.
 *
 * `droppedQueued` is the discard a disable makes (`AMB-D-399`): whatever was waiting on that plugin's
 * queue is gone, and it is not caught up on when the plugin comes back. The caller is expected to say so
 * — the count is the only trace those events leave.
 *
 * A `null` layer is the device's gate (`AMB-D-601`), and opening that one is the consent to let the plugin
 * read every project on the machine — which is why it is the same call and not a second question.
 */
export async function setPluginEnabled(
  name: string,
  layer: PluginLayer,
  enabled: boolean,
): Promise<PluginGateMoved> {
  if (!inTauri()) return { enabled: false, droppedQueued: 0 };
  const now = await invoke<PluginGateMoved>("plugin_set_enabled", {
    name,
    projectId: layer,
    enabled,
  });
  reloadInstalls();
  return now;
}

/**
 * Write one setting the plugin's author declared (Tauri: `plugin_config_set`, `AMB-D-356`).
 *
 * `layer` names where the value belongs — one project's rows (`AMB-D-434`) or, as `null`, the device's
 * (`AMB-D-601`). The author's `secret` flag decides which of the two tables it lands in, and this seam
 * never says which is which. An **empty** value clears the setting.
 *
 * What that layer holds is refetched afterwards, since it is what the form draws from.
 */
export async function setPluginConfig(
  name: string,
  key: string,
  value: string,
  layer: PluginLayer,
): Promise<void> {
  if (!inTauri()) return;
  await invoke<null>("plugin_config_set", { name, key, value, projectId: layer });
  reloadConfig();
}

/**
 * Raise the author's check on the values as they now stand (Tauri: `plugin_settings_check`, `AMB-D-664`)
 * — the second moment a check runs, and the one that takes nothing back.
 *
 * It belongs **after** the writes, not between them: the door above writes one setting at a time, and a
 * form with three changed boxes uses it three times, so this is called once when they have all landed.
 * Whatever it answers, the values stay written and an enabled plugin stays enabled — what the run is for
 * here is the sentence beside the box.
 *
 * `null` is a check nobody raised: the plugin declares none, or the crossing's gate is shut — running the
 * author's code is what enabling means (`AMB-D-351`), and a save is not that press.
 */
export async function checkPluginSettings(
  name: string,
  layer: PluginLayer,
): Promise<PluginCheck | null> {
  if (!inTauri()) return null;
  return invoke<PluginCheck | null>("plugin_settings_check", { name, projectId: layer });
}

/**
 * Press one operation the plugin's author declared (Tauri: `plugin_settings_action`, `AMB-D-664`).
 *
 * `cmd` names a declaration and is never a line composed here: core looks it up in the manifest and takes
 * the words from there, so this seam cannot ask a plugin to run something its author did not write
 * (`AMB-D-522`).
 *
 * `supplied` is what this press asked for (`ask`) — handed to that one run and stored nowhere. What the
 * run leaves behind is another matter: an operation writes back through `plugin config set` when it has
 * something to save (`AMB-D-406`), so what the layer holds is refetched afterwards, exactly as a write of
 * our own does. The refetch is unconditional because a plugin that ended up saying no may still have
 * written before it got there.
 *
 * A refusal — a shut gate, a plugin that will not start — throws; a plugin that ran and failed comes back
 * as `ok: false` with whatever line it wrote, because "it ran and said no" is an answer, not an error.
 */
export async function runPluginAction(
  name: string,
  cmd: string,
  supplied: Record<string, string>,
  layer: PluginLayer,
): Promise<PluginActionRan> {
  if (!inTauri()) return { ok: false, show: [] };
  const outcome = await invoke<PluginActionRan>("plugin_settings_action", {
    name,
    cmd,
    supplied,
    projectId: layer,
  });
  reloadConfig();
  return outcome;
}

/**
 * Remove one plugin and everything it left behind (Tauri: `plugin_uninstall`, `AMB-D-357`), and answer
 * what was actually found.
 *
 * **Uninstall is not disable**: the gates close on the way out, and the binary, the settings
 * in every project and the secrets go with it — a re-install starts clean. Ask before calling; the receipt
 * is for saying what went, not for asking whether it should.
 */
export async function uninstallPlugin(name: string): Promise<PluginRemoved | null> {
  if (!inTauri()) return null;
  const removed = await invoke<PluginRemoved>("plugin_uninstall", { name });
  reloadInstalls();
  reloadConfig();
  // An offer to update what is no longer here would be an offer to install it again (`AMB-D-359`).
  invalidateQueries((key) => key[0] === "plugin-updates");
  return removed;
}

/**
 * The state of one row (`AMB-D-447`), for a face that is drawing it — one project's crossing, or the
 * device's own row when `layer` is `null` (`AMB-D-601`).
 *
 * A layer the install names nothing about is one that never held the plugin on and never filled it in
 * — off, holding nothing, and short of whatever the author marked `required` without a default. That
 * last part is read off the schema rather than left blank, so a row opened from the picker carries its
 * warning before the switch is pressed rather than after core refuses it.
 */
export function crossingAt(install: PluginInstall, layer: PluginLayer): PluginRowState {
  const row = layer == null ? install.device : install.projects.find((p) => p.project === layer);
  if (row) return { enabled: row.enabled, hasValue: row.hasValue, requiredUnset: row.requiredUnset };
  return {
    enabled: false,
    hasValue: false,
    requiredUnset: install.config.some((f) => f.required && f.defaultValue == null),
  };
}

/**
 * Whether the plugin fires anywhere at all — the one-line reading a badge about the *plugin* wants
 * (`AMB-D-412`). Which projects is a row's to name.
 *
 * A plugin its author declared the machine's has one gate and no project rows (`AMB-D-601`), so reading
 * the project list for it would answer "nowhere" for something firing on the whole device.
 */
export function firesAnywhere(install: PluginInstall): boolean {
  if (install.device) return install.device.enabled;
  return install.projects.some((p) => p.enabled);
}

/** The row for one catalog entry's name, or `undefined` when this machine does not hold it. */
export function installOf(installs: PluginInstall[], name: string): PluginInstall | undefined {
  return installs.find((i) => i.name === name);
}
