// The device's shelf of notification targets — the read and write seam for `screens/NotifyTargetsSetting`
// (`AMB-D-885`).
//
// A connection is written **here once, under a name**, and a project selects from the shelf rather than
// holding a connection of its own. So this seam takes no project: the shelf is the device's, it answers
// the same wherever the screen is standing, and the project side (which targets carry it, what it
// reports) is a different surface over different rows.
//
// **No credential is ever read back.** A row says whether it holds one (`secretSet`) and nothing more, so
// a form draws a masked box it cannot repopulate — which is exactly what `saveNotifyTarget` leaving
// `secret` undefined means: keep what is held.
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type { NotifyCheckedDto, NotifyTargetDto } from "../bindings/bindings";

/** One connection this device can send through, under a name (generated DTO). */
export type NotifyTarget = NotifyTargetDto;
/** What carries a notification — the two the model declares. */
export type NotifyKind = NotifyTarget["kind"];

/** The kinds a target can be raised as, in the order the "add" menu offers them. */
export const NOTIFY_KINDS: NotifyKind[] = ["slack", "mail"];

const NONE: NotifyTarget[] = [];

/** The shelf, as core holds it. Empty outside Tauri, where there is no store to ask. */
export async function fetchNotifyTargets(): Promise<NotifyTarget[]> {
  if (!inTauri()) return NONE;
  return invoke<NotifyTarget[]>("notify_targets");
}

/** The shelf, for the settings section that draws it. */
export function useNotifyTargets(): { targets: NotifyTarget[]; loading: boolean; error: unknown } {
  const { data, loading, error } = useQuery<NotifyTarget[]>(["notify-targets"], fetchNotifyTargets);
  return { targets: data ?? NONE, loading, error };
}

/** Refetch the shelf — after a row was raised, saved, marked or deleted. */
function reload(): void {
  invalidateQueries((key) => key[0] === "notify-targets");
}

/**
 * Raise a target on the shelf under a name. The connection is written afterwards, by a save — which is
 * what gives the credential a row to hang off.
 *
 * The first one ever raised carries the default mark, core's doing: there is nothing else it could point
 * at.
 */
export async function addNotifyTarget(kind: NotifyKind, name: string): Promise<NotifyTarget | null> {
  if (!inTauri()) return null;
  const row = await invoke<NotifyTarget>("notify_target_add", { kind, name });
  reload();
  return row;
}

/** What one save carries. The SMTP fields are a mail target's; core ignores them on a Slack row. */
export type NotifyTargetEdit = {
  name: string;
  smtpHost?: string;
  smtpPort?: number;
  smtpUser?: string;
  mailFrom?: string;
  /**
   * The credential — a Slack webhook URL, a mail password. **Left out, what is held stays**: the box it
   * was typed into is masked and cannot be read back, so an untouched form has nothing to send. An empty
   * string clears it.
   */
  secret?: string;
};

/** Write one target's connection, whole — the form is filled in whole, so it is saved whole. */
export async function saveNotifyTarget(
  id: number,
  edit: NotifyTargetEdit,
): Promise<NotifyTarget | null> {
  if (!inTauri()) return null;
  const row = await invoke<NotifyTarget>("notify_target_save", { id, ...edit });
  reload();
  return row;
}

/**
 * Move the default mark onto this target — where a **newly created** project starts out pointing. The
 * projects already standing keep the selection they made.
 */
export async function setDefaultNotifyTarget(id: number): Promise<void> {
  if (!inTauri()) return;
  await invoke<NotifyTarget>("notify_target_set_default", { id });
  reload();
}

/**
 * Delete a target, and with it every project's selection of it and the credential it held. Answers with
 * the projects that lost it, which is what the sentence after the press is written from.
 */
export async function deleteNotifyTarget(id: number): Promise<number[]> {
  if (!inTauri()) return [];
  const lost = await invoke<number[]>("notify_target_delete", { id });
  reload();
  return lost;
}

/** What a connection check found, and how much of it was actually asked (generated DTO). */
export type NotifyChecked = NotifyCheckedDto;

/**
 * Ask whether this target's connection is usable, without posting to prove it.
 *
 * How much that means is the kind's, and the answer says which: a mail relay is connected to and the
 * account offered to it, a Slack webhook has only its URL read. Neither writes anything, so nothing is
 * refetched afterwards.
 */
export async function checkNotifyTarget(id: number): Promise<NotifyChecked | null> {
  if (!inTauri()) return null;
  return invoke<NotifyChecked>("notify_target_check", { id });
}

/**
 * Send one message through this target — the only thing that answers whether it still works.
 *
 * It is asked of the shelf and not of a project, so it carries no project's name: a mail target sends to
 * the account it authenticates as, which is the mailbox its owner reads.
 */
export async function testNotifyTarget(id: number): Promise<void> {
  if (!inTauri()) return;
  await invoke<null>("notify_target_test", { id });
}
