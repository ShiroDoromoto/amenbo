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
 * nothing finer about itself. Most cover dozens of different sentences apiece and so hold no template:
 * nothing could be written that would be true of all of them, and the reader gets the English sentence
 * the command returned. A few hold one anyway, and the test of which is whether every sentence under the
 * family says the same thing to a reader — the store is busy, or the store itself gave way. There the
 * template is what says it in their language, and the prose underneath is a detail for the log rather
 * than for the screen. */
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
  "not_ready_draft",
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
  // The three refusals a required axis raises (`AMB-D-734`). All reach the screen: the panel holds the
  // button that ends a creation, but another device can raise the flag between the render and the click,
  // and clearing a task's value on such an axis is offered by the same select that sets one. The panel
  // greys its required box on an axis offering no values, for the same reason core refuses one — but the
  // count it greys by is the one it last read, and another device removing the last value re-opens the box.
  "invalid_dimension_required_without_values",
  "invalid_dimension_required_unset",
  // The two refusals a slug meets (`AMB-D-735`): a shape the door does not take, and one another axis
  // or value in the same reach already answers to. The classification panel is where a slug is typed,
  // so both land on the screen.
  "invalid_dimension_slug_shape",
  "invalid_dimension_slug_taken",
  // The refusal a name meets (`AMB-D-819`): whitespace inside it, which would leave the axis or the value
  // unreachable by the one key a person actually remembers. Typed in the same panel as the slug, so it
  // lands on the screen along with them.
  "invalid_dimension_name_whitespace",
  // The pair a time axis will not enter into (`AMB-D-826`): the era resolution reads one value, so an
  // axis that admits several cannot be the one it reads. The classification panel holds both switches
  // side by side, so flipping either one onto the other lands the refusal on the screen.
  "invalid_dimension_multi_time_axis",
  // Its other half: the way back down, refused while records still answer that axis with several
  // values (`AMB-D-826`). The same panel is where the switch is lowered, and the count the sentence
  // carries is the size of what the reader is being asked to clear first.
  "invalid_dimension_demote_holders",
  "invalid_task_required_dimension",
  // Its decision twin: the same flag, read at the other door a record passes through once
  // (`decision accept`), and the decision pane is where that button is.
  "invalid_decision_required_dimension",
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
  // The plugin screens. Almost everything that goes wrong there comes from a machine the reader does
  // not control — a catalog document, an asset off the network, a manifest an update replaced — so the
  // refusals are many, and which one arrived is what says whether to retry, to ask the publisher, or to
  // uninstall and start again.
  "not_found_plugin_in_catalog",
  "not_found_plugin_installed",
  "not_found_plugin_build_official",
  "not_found_plugin_build_origin_unknown",
  "not_found_plugin_build_source_gone",
  "not_found_plugin_build_source_silent",
  "not_found_plugin_build_delisted",
  "conflict_plugin_installed",
  "conflict_plugin_install_broken",
  "invalid_catalog_unreadable",
  "invalid_catalog_version_ahead",
  "invalid_catalog_detail_swapped",
  "invalid_catalog_detail_unreadable",
  "invalid_catalog_detail_names_other",
  "invalid_catalog_url_scheme",
  "invalid_catalog_url_official",
  "invalid_catalog_key_rotated",
  "invalid_catalog_key_document",
  "invalid_catalog_key_absent",
  "invalid_plugin_entry",
  "invalid_plugin_entry_dropped",
  "invalid_plugin_entry_duplicate",
  "invalid_plugin_os_unsupported",
  "invalid_plugin_asset_absent",
  "invalid_plugin_asset_empty",
  "invalid_plugin_asset_zip_off_windows",
  "invalid_plugin_asset_tar_unreadable",
  "invalid_plugin_asset_zip_unreadable",
  "invalid_plugin_asset_without_program",
  "invalid_plugin_manifest_unwritable",
  "invalid_plugin_checksum_format",
  "invalid_plugin_checksum_mismatch",
  "invalid_plugin_checksum_length",
  "invalid_plugin_checksum_not_hex",
  "invalid_plugin_key_malformed",
  "invalid_plugin_signature_malformed",
  "invalid_plugin_signature_mismatch",
  "invalid_plugin_unsigned",
  "invalid_plugin_manifest_malformed",
  "invalid_plugin_manifest_names_other",
  "invalid_plugin_program_absent",
  "invalid_plugin_update_platform",
  "invalid_plugin_project_required",
  "invalid_plugin_settings_required",
  "invalid_plugin_config_value_too_large",
  "invalid_plugin_config_value_control_chars",
  // Why a plugin does not run against this build. They arrive as the refusal's `parts`, because the
  // same three verdicts read under two different sentences — one for enabling, one for updating —
  // and writing them as six codes would be six templates saying three things.
  "invalid_plugin_incompatible",
  "invalid_plugin_update_incompatible",
  "plugin_incompatible_payload",
  "plugin_incompatible_amenbo_old",
  "plugin_incompatible_floor_unreadable",
] as const;

/** Core codes the webview never receives, because the only door they come through is the CLI. Two doors
 * are like that. Putting a plugin back on its earlier build is one (`AMB-D-522`): the screen offers no way
 * to ask for it, so nothing on it can be refused for these reasons. The other is an enable turned away by
 * the plugin's own check (`AMB-D-664`): the switch on screen is handed the verdict and a gate that did not
 * move — the author's sentences *are* the refusal there — so this one travels to the terminal alone. They
 * are listed because the parity test reads every code core declares, not because a reader ever meets one
 * — which is also why they owe no template. A code that reaches a screen belongs in the sentence list
 * instead, with its prose. */
export const CORE_CLI_ONLY_ERROR_CODES = [
  // The three refusals closing a value raises (`AMB-D-829`): an axis nobody nominated closable, the last
  // value a required axis still offers, and a closed value somebody tried to file a record under. The one
  // door that reaches them is `dimension value-close` / `value-reopen` in the terminal; the classification
  // panel has no close control, so nothing on a screen can be refused for these reasons. The day the panel
  // grows one, these move into the sentence list above, with the prose they then owe.
  "invalid_dimension_close_not_closable",
  "invalid_dimension_close_last_open",
  "invalid_dimension_set_closed_value",
  "not_found_plugin_rollback_build",
  "invalid_plugin_rollback_manifest_absent",
  "invalid_plugin_rollback_manifest_unparsable",
  "invalid_plugin_check_refused",
  "invalid_plugin_check_silent",
] as const;

/** Every code core can emit (`amenbo_core::ErrorCode::ALL`), at every grain. */
export const CORE_ERROR_CODES = [
  ...CORE_FAMILY_ERROR_CODES,
  ...CORE_SENTENCE_ERROR_CODES,
  ...CORE_CLI_ONLY_ERROR_CODES,
] as const;

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
