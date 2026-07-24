// What this machine holds, beside the catalog it is browsing — the install/enable seam (`AMB-D-351`).
//
// The market's other seam (`pluginCatalog`) is about what *exists*; this one is about what is *here*, and
// the two are deliberately separate fetches: the catalog is a static file off the network, the installs are
// a directory on disk plus this store, and joining them is the screen's job (by name). So a catalog that
// cannot be reached still draws the plugins you have, and a machine with nothing installed costs one cheap
// local read.
//
// **Install is not enable** (`AMB-D-351`), so nothing here folds the two: `installPlugin` lands a plugin
// inert, and `setPluginEnabled` is the separate act the consent is taken for. The consent question itself
// belongs to the face (the detail asks it, once per device — `consented` is how it knows), and calling
// enable is the answer core records.
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type { PluginInstallDto } from "../bindings/bindings";

/** One installed plugin and where its switch stands (generated DTO). */
export type PluginInstall = PluginInstallDto;

const NONE: PluginInstall[] = [];

/**
 * Read what is installed, resolved for `projectId` (Tauri: `plugin_installs`). The project is not a
 * choice of level — the author declared that (`AMB-D-379`) — it is which project the answer is about: a
 * project-scoped plugin asked about without one comes back with no `enabled` at all rather than "off".
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
 * Move one plugin's gate (Tauri: `plugin_set_enabled`), and answer where it ended up. Enabling is
 * fail-closed in core on compatibility and on the author's `required` settings, so a refusal here is a
 * message to show, not a state to guess at.
 *
 * Calling it to enable **is** the consent (`AMB-D-351`): ask before calling, and only the first time —
 * `consented` on the row is what says whether this device has already answered.
 */
export async function setPluginEnabled(
  name: string,
  projectId: number | null,
  enabled: boolean,
): Promise<boolean> {
  if (!inTauri()) return false;
  const now = await invoke<boolean>("plugin_set_enabled", { name, projectId, enabled });
  reloadInstalls();
  return now;
}

/** The row for one catalog entry's name, or `undefined` when this machine does not hold it. */
export function installOf(installs: PluginInstall[], name: string): PluginInstall | undefined {
  return installs.find((i) => i.name === name);
}
