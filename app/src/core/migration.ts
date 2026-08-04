// The front-end seam onto the startup migration.
//
// Nothing here can **start** a migration. It runs in exactly one place — core, at startup (`migrate::at_startup`) —
// and all the GUI may do is ask where it stands (`migration_status`) and put a failed one through again
// (`migration_retry`). There is no consent gate either.
//
// The stage is **both pulled and pushed**: the window mounts after the migration has already begun, so a
// subscription alone would miss the first announce — and a subscription wired *after* a read would miss the end of a
// run that finished in between. So the screen listens to `migration-changed` (the stage) and `migration-progress`
// (ticks from the pre-migration backup and from each step of the version chain) **first**, and only then reads the
// stage again: a read taken after the subscription is live can be superseded, never missed.

import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import type { DataProgressDto, MigrationStatusDto } from "../bindings/bindings";

/**
 * The stage, pulled once at startup. `idle` — nothing to carry forward, the normal case — collapses to null and the
 * caller simply proceeds with a normal boot. A failure collapses to null as well: there is nothing to diagnose here.
 * If the store is **newer** than this build, the `invoke` seam catches `format_ahead` and diverts to the restart gate;
 * and if there is no store, there is nothing to migrate.
 */
export async function migrationGate(): Promise<MigrationStatusDto | null> {
  const status = await migrationStatus();
  return status && status.stage !== "idle" ? status : null;
}

/**
 * The stage as core holds it right now, `idle` included — what the gate above is built on, and what the migration
 * screen reads a second time once its subscriptions are up. Core keeps the terminal stage in the same state the
 * events are published from, so this read answers with the end of a run whose event went out before anyone listened.
 * Null on every failure and outside Tauri, exactly as the gate is.
 */
export async function migrationStatus(): Promise<MigrationStatusDto | null> {
  if (!inTauri()) return null;
  try {
    return await invoke<MigrationStatusDto>("migration_status");
  } catch {
    return null;
  }
}

/** Run a failed migration again (**destructive**; the migration screen's "retry"). The outcome arrives as a stage event. */
export async function retryMigration(): Promise<void> {
  await invoke<null>("migration_retry");
}

/** Subscribe to stage changes (`migration-changed`). Always call the unlisten it returns. A no-op outside Tauri. */
export async function listenMigrationChanged(
  cb: (s: MigrationStatusDto) => void,
): Promise<() => void> {
  if (!inTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<MigrationStatusDto>("migration-changed", (e) => cb(e.payload));
}

/** Subscribe to migration progress (`migration-progress`: the backup's phases and the version chain's steps). Always call the unlisten it returns. */
export async function listenMigrationProgress(
  cb: (p: DataProgressDto) => void,
): Promise<() => void> {
  if (!inTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<DataProgressDto>("migration-progress", (e) => cb(e.payload));
}

/** Bytes to whole MiB, for showing sizes. 0 stays 0; anything else rounds up, so nothing ever looks smaller than it is. */
export function mib(bytes: number): number {
  return bytes === 0 ? 0 : Math.max(1, Math.ceil(bytes / (1024 * 1024)));
}
