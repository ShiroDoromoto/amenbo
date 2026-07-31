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
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type {
  PluginConfigFieldDto,
  PluginGateMovedDto,
  PluginInstallDto,
  PluginProjectRowDto,
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
 * Outside Tauri — `npm run dev` in a browser — there is no plugins directory to read, so the mock says
 * nothing is installed rather than inventing state.
 */
export async function fetchPluginInstalls(): Promise<PluginInstall[]> {
  if (inTauri()) return invoke<PluginInstall[]>("plugin_installs");
  return NONE;
}

/** The installs, for the market to draw over the catalog. Local and cheap: no network, no catalog fetch. */
export function usePluginInstalls(): {
  installs: PluginInstall[];
  loading: boolean;
  error: unknown;
} {
  const { data, loading, error } = useQuery<PluginInstall[]>(["plugin-installs"], () =>
    fetchPluginInstalls(),
  );
  return { installs: data ?? NONE, loading, error };
}

/** Refetch the installs — after one landed, or after a gate moved. */
function reloadInstalls(): void {
  invalidateQueries((key) => key[0] === "plugin-installs");
}

/**
 * Read what one project holds for a plugin's declared settings (Tauri: `plugin_config_read`).
 *
 * Separate from the installs because a value is one project's and an install is not (`AMB-D-434`): with
 * no project named there is nothing to read, and no blanks are invented in its place — the form draws
 * the author's schema from the install row until a project answers for it.
 */
export async function fetchPluginConfig(
  name: string,
  projectId: number | null,
): Promise<PluginConfigField[]> {
  if (!inTauri() || projectId == null) return NO_FIELDS;
  return invoke<PluginConfigField[]>("plugin_config_read", { name, projectId });
}

/** What one project holds for a plugin — the settings form's own read. */
export function usePluginConfig(
  name: string,
  projectId: number | null,
): { fields: PluginConfigField[]; loading: boolean } {
  const { data, loading } = useQuery<PluginConfigField[]>(["plugin-config", name, projectId], () =>
    fetchPluginConfig(name, projectId),
  );
  return { fields: data ?? NO_FIELDS, loading };
}

/** Refetch what a project holds — after a value was written or cleared. */
function reloadConfig(): void {
  invalidateQueries((key) => key[0] === "plugin-config");
}

/**
 * Install one plugin by name (Tauri: `plugin_install`). Every gate is core's: the catalog resolve, the
 * signature against amenbo's own key and the checksum over the bytes served — a refusal anywhere throws
 * with the reason, and nothing is written (`AMB-D-371`).
 *
 * It lands **inert**: the row it returns is an installed plugin whose gate is open nowhere — which is
 * also why no project is named to install it (`AMB-D-412`).
 */
export async function installPlugin(name: string): Promise<PluginInstall | null> {
  if (!inTauri()) return null;
  const row = await invoke<PluginInstall>("plugin_install", { name });
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
 */
export async function setPluginEnabled(
  name: string,
  projectId: number,
  enabled: boolean,
): Promise<PluginGateMoved> {
  if (!inTauri()) return { enabled: false, droppedQueued: 0 };
  const now = await invoke<PluginGateMoved>("plugin_set_enabled", { name, projectId, enabled });
  reloadInstalls();
  return now;
}

/**
 * Write one setting the plugin's author declared (Tauri: `plugin_config_set`, `AMB-D-356`).
 *
 * `projectId` names the project the value belongs to and is required (`AMB-D-434`) — the author's
 * `secret` flag decides which of the two tables it lands in, and this seam never says which is which.
 * An **empty** value clears the setting.
 *
 * What that project holds is refetched afterwards, since it is what the form draws from.
 */
export async function setPluginConfig(
  name: string,
  key: string,
  value: string,
  projectId: number | null,
): Promise<void> {
  if (!inTauri()) return;
  await invoke<null>("plugin_config_set", { name, key, value, projectId });
  reloadConfig();
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
 * The state of one project × plugin crossing (`AMB-D-447`), for a face that is drawing that row.
 *
 * A project the install names nothing about is one that never held the plugin on and never filled it in
 * — off, holding nothing, and short of whatever the author marked `required` without a default. That
 * last part is read off the schema rather than left blank, so a row opened from the picker carries its
 * warning before the switch is pressed rather than after core refuses it.
 */
export function crossingAt(install: PluginInstall, projectId: number): PluginProjectRowDto {
  const row = install.projects.find((p) => p.project === projectId);
  if (row) return row;
  return {
    project: projectId,
    enabled: false,
    hasValue: false,
    requiredUnset: install.config.some((f) => f.required && f.defaultValue == null),
  };
}

/**
 * Whether the plugin fires in one project — that crossing's gate (`AMB-D-434`). A project with no row is
 * one that never held the plugin on and never filled it in, which reads the same as off.
 */
export function enabledIn(install: PluginInstall, projectId: number): boolean {
  return install.projects.some((p) => p.project === projectId && p.enabled);
}

/**
 * Whether the plugin fires anywhere at all — the one-line reading a badge about the *plugin* wants
 * (`AMB-D-412`). Which projects is a row's to name.
 */
export function firesAnywhere(install: PluginInstall): boolean {
  return install.projects.some((p) => p.enabled);
}

/** The row for one catalog entry's name, or `undefined` when this machine does not hold it. */
export function installOf(installs: PluginInstall[], name: string): PluginInstall | undefined {
  return installs.find((i) => i.name === name);
}
