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

/**
 * Read what is installed, resolved for `projectId` (Tauri: `plugin_installs`). The project is not a
 * choice of level — there is only one (`AMB-D-434`) — it is which project the answer is about: a plugin
 * asked about without one comes back with no `enabled` at all rather than "off".
 *
 * Outside Tauri — `npm run dev` in a browser — there is no plugins directory to read, so the mock says
 * nothing is installed rather than inventing state.
 */
export async function fetchPluginInstalls(projectId: number | null): Promise<PluginInstall[]> {
  if (inTauri()) return invoke<PluginInstall[]>("plugin_installs", { projectId });
  return NONE;
}

/** The installs, for the market to draw over the catalog. Local and cheap: no network, no catalog fetch. */
export function usePluginInstalls(
  projectId: number | null,
): { installs: PluginInstall[]; loading: boolean; error: unknown } {
  const { data, loading, error } = useQuery<PluginInstall[]>(["plugin-installs", projectId], () =>
    fetchPluginInstalls(projectId),
  );
  return { installs: data ?? NONE, loading, error };
}

/** Refetch the installs — after one landed, or after a gate moved. */
function reloadInstalls(): void {
  invalidateQueries((key) => key[0] === "plugin-installs");
}

/**
 * Install one plugin by name (Tauri: `plugin_install`). Every gate is core's: the catalog resolve, the
 * signature against amenbo's own key and the checksum over the bytes served — a refusal anywhere throws
 * with the reason, and nothing is written (`AMB-D-371`).
 *
 * It lands **inert**: the row it returns is an installed plugin with its gate shut.
 */
export async function installPlugin(
  name: string,
  projectId: number | null,
): Promise<PluginInstall | null> {
  if (!inTauri()) return null;
  const row = await invoke<PluginInstall>("plugin_install", { name, projectId });
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
  projectId: number | null,
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
 * `projectId` is the tier, not the gate: `null` writes the machine default and a project writes that
 * project's override, while a secret ignores it entirely — the author's flag decides where the value
 * lives, and this seam never says which is which. An **empty** value clears the setting.
 *
 * The installs are refetched afterwards because they carry what is now held, which is what the form
 * draws from.
 */
export async function setPluginConfig(
  name: string,
  key: string,
  value: string,
  projectId: number | null,
): Promise<void> {
  if (!inTauri()) return;
  await invoke<null>("plugin_config_set", { name, key, value, projectId });
  reloadInstalls();
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
  // An offer to update what is no longer here would be an offer to install it again (`AMB-D-359`).
  invalidateQueries((key) => key[0] === "plugin-updates");
  return removed;
}

/** The row for one catalog entry's name, or `undefined` when this machine does not hold it. */
export function installOf(installs: PluginInstall[], name: string): PluginInstall | undefined {
  return installs.find((i) => i.name === name);
}
