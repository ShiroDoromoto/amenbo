// The single export point from which TS/the GUI refer to the codes in the `--json` error contract.
//
// On the producing side (Rust) the codes live in typed registries:
// - core's codes = `amenbo_core::ErrorCode` (`as_str()` / `ALL` in `crates/amenbo-core/src/error.rs`).
// - codes specific to the GUI surface = whatever the Tauri command layer raises via `CmdError::coded(...)`
//   (today, the guards in `project_add_folder`, which turns an existing folder into a new project —
//   `app/src-tauri/src/commands.rs`). CLI-only codes (`CliError`) never reach the webview, so they are excluded.
//
// This is the only TS definition of the codes a webview can receive. Copying code strings around by hand is
// forbidden; the Rust↔TS parity test in `errorCodes.test.ts` catches drift between the Rust source and this file.
// The generic fallback `"error"` — an ad-hoc error with no stable code — is not part of the contract and is excluded.

/** The family codes — one per `amenbo_core::Error` variant, and what a failure carries when it says
 * nothing finer about itself. They cover dozens of different sentences apiece, which is why they hold
 * no template: nothing could be written that would be true of all of them. A reader gets the English
 * sentence the command returned, which is the settled answer for a surface no one is translating. */
export const CORE_FAMILY_ERROR_CODES = [
  "not_found",
  "ambiguous_id",
  "invalid_value",
  "conflict",
  "already_reserved",
  "not_ready",
  "out_of_reach",
  "binding_stale",
  "format_ahead",
  "io_error",
  "parse_error",
  "storage_error",
  "store_busy",
] as const;

/** The sentence codes — one refusal each, so a dictionary can hold a template for it and the reader gets
 * it in their own language (`AMB-D-413`). Which ones exist is decided by measurement: these are the
 * refusals the GUI actually puts in front of a person. Every one of them owes a template
 * (`i18n/errors.test.ts` holds them to it) — splitting a code off its family and then not writing the
 * sentence would leave the reader exactly where they started. */
export const CORE_SENTENCE_ERROR_CODES = [
  // The reasons under `not_ready`. They arrive as the refusal's `parts` rather than as its code — a
  // reservation can be turned away for several reasons at once, and how many is known only at the
  // moment of refusing, so each reason is written from its own template and the front end joins them.
  "not_ready_open_blocker",
  "not_ready_premise_superseded",
  "not_ready_premise_rejected",
  "not_ready_premise_unsettled",
  "not_ready_not_started",
  "not_found_task",
  "not_found_decision",
  "not_found_project",
  "not_found_user",
  "not_found_comment",
  "not_found_dimension",
  "not_found_dimension_value",
  "not_found_blob",
  "invalid_commit_sha",
  "invalid_attachment_too_large",
  "invalid_dimension_period_order",
  "invalid_dimension_values_unordered",
  "invalid_decision_edit_rejected",
  "invalid_decision_accept_rejected",
  "invalid_decision_reject_accepted",
  "invalid_decision_reopen_rejected",
  "invalid_decision_self_supersede",
  "invalid_decision_self_amend",
  "invalid_decision_self_builds_on",
  // Settings > Data (back up / restore / export), where every refusal a person meets is about the path
  // or the file they chose. What the store itself being broken raises — a failed `integrity_check`, a
  // snapshot missing a column — keeps its family code: those name SQLite's own tables back at the
  // reader, and no sentence written here would tell them any more than core's already does.
  "invalid_backup_dest_is_dir",
  "invalid_backup_dest_exists",
  "invalid_restore_source_is_dir",
  "invalid_restore_not_an_archive",
  "invalid_restore_missing_snapshot",
  "invalid_restore_layout_too_old",
  "invalid_restore_layout_too_new",
  "invalid_restore_archive_newer",
  "invalid_export_dest_exists",
  // The startup migration screen, which is the whole window — the reader has nothing else to go on,
  // and the way back is what the sentence has to name.
  "invalid_migration_no_space",
  "invalid_migration_rolled_back",
  "invalid_migration_rollback_failed",
] as const;

/** Every code core can emit (`amenbo_core::ErrorCode::ALL`), at both grains. */
export const CORE_ERROR_CODES = [...CORE_FAMILY_ERROR_CODES, ...CORE_SENTENCE_ERROR_CODES] as const;

/** GUI-specific codes the Tauri command layer raises via `CmdError::coded(...)` — contexts core knows nothing about.
 * They come from the guards in `project_add_folder`, which makes a folder into a new project (an existing
 * `.amenbo` gives `init_pointer_exists`; a marker plus several live stores claiming ownership is an
 * irrecoverable ambiguity and gives `init_ambiguous_owners`); the nested-binding guard in `project_bind_folder`,
 * which binds an existing folder to an existing project; and every open blocked while a startup migration holds
 * the store (`migrate::gate()` — mid-migration the format is half-moved, and after a failure it is still old);
 * and the consent guard in `plugin_catalog_add_source`, where registering a catalog crosses a process
 * boundary between showing a fingerprint and agreeing to it, so the pin that is written has to be the one
 * that was on screen (`AMB-D-389`). */
export const TAURI_ERROR_CODES = [
  "init_ambiguous_owners",
  "init_pointer_exists",
  "binding_nested_tree",
  "migration_failed",
  "migration_running",
  "plugin_catalog_consent_required",
  "plugin_catalog_key_changed",
] as const;

/** Every code a webview can receive — the contract that i18n and code-based branching may refer to. */
export const ERROR_CODES = [...CORE_ERROR_CODES, ...TAURI_ERROR_CODES] as const;

/** The type of a contract code (single source). */
export type ErrorCode = (typeof ERROR_CODES)[number];

/** Is this string a contract code? (Narrows.) */
export function isErrorCode(s: string): s is ErrorCode {
  return (ERROR_CODES as readonly string[]).includes(s);
}
