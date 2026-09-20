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
  // The three refusals closing a value raises (`AMB-D-829`). The panel holds the closable switch and the
  // close button side by side, so a value can be asked to close on an axis whose role was just dropped
  // elsewhere; it shuts the button on the last value a required axis still offers, and the refusal is the
  // backstop under a panel drawn a moment ago. The third arrives at the pickers rather than here — a
  // value closed on another face is gone from the select but may still be carried, and a record moving
  // onto it is turned away.
  "invalid_dimension_close_not_closable",
  "invalid_dimension_close_last_open",
  "invalid_dimension_set_closed_value",
  "invalid_task_required_dimension",
  // A status a task still being created cannot take (`AMB-D-846`). The panel does not draw the controls
  // on a draft card, so the refusal is the backstop under a window drawn before another device reopened
  // the creation.
  "invalid_task_status_draft",
  // Its decision twin: the same flag, read at the other door a record passes through once
  // (`decision finish-writing`), and the decision pane is where that button is.
  "invalid_decision_required_dimension",
  "invalid_decision_edit_rejected",
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

/** Core codes the webview never receives, because the only door they come through is the CLI. None is
 * declared today: every code core raises reaches a screen, so each one owes a template and sits in the
 * sentence list above. The list stays because the parity test reads every code core declares, and a door
 * the terminal alone can reach is a shape that comes back. */
export const CORE_CLI_ONLY_ERROR_CODES = [] as const;

/** Every code core can emit (`amenbo_core::ErrorCode::ALL`), at every grain. */
export const CORE_ERROR_CODES = [
  ...CORE_FAMILY_ERROR_CODES,
  ...CORE_SENTENCE_ERROR_CODES,
  ...CORE_CLI_ONLY_ERROR_CODES,
] as const;

/** GUI-specific codes the Tauri command layer raises via `CmdError::coded(...)` — contexts core knows nothing about.
 * They come from the guards in `project_add_folder`, which makes a folder into a new project (an existing
 * `.amenbo` gives `init_pointer_exists`; a marker plus several live stores claiming ownership is an
 * irrecoverable ambiguity and gives `init_ambiguous_owners`); the workspace's way in
 * (`folder_open`), which heals a pointer naming a project that is gone but leaves one written by
 * another channel's build alone and says so (`pointer_other_store`, `AMB-D-685`); the nested-binding guard in `project_bind_folder`,
 * which binds an existing folder to an existing project; and every open blocked while a startup migration holds
 * the store (`migrate::gate()` — mid-migration the format is half-moved, and after a failure it is still old);
 * and the three the terminal in a pane answers with, which are the operating system refusing to open one
 * (`pty_failed`), a session whose terminal has already closed (`pty_gone`), and an image pasted into it
 * that could not be written down (`pty_paste_failed`); the two a window refuses with, which is the terminal being split out into a
 * window of its own and the platform not building it (`window_failed`) or the window it built never
 * drawing anything (`talk_blank`); and the ones settling which
 * agent a folder opens with (`crate::wake`) — a folder that cannot be read (`wake_no_folder`), an
 * install that cannot find its own config (`wake_no_config`), a choice that could not be written down
 * (`wake_not_kept`), an agent id that names neither a catalog row nor one of this device's own
 * registrations (`wake_unknown_agent`), refused the same way wherever it arrives, and a registration
 * with a half missing (`wake_not_registered`); the four the file panel meets when it writes a
 * name into a folder (`crate::folder_write`) — the name is taken (`folder_taken`), the machine will
 * not hold it (`folder_name`), or the machine refused the making or the renaming itself for a reason
 * of its own (`folder_make`, `folder_rename`); and the three it answers a save with
 * (`crate::folder_save`) — a character the file's encoding has no room for
 * (`folder_unwritable_character` — a `✓` typed into a Shift_JIS file, named rather than mangled into
 * it), the file having moved between the read and the save
 * (`folder_changed_underneath` — an agent in the pane wrote to it while the editor held what was
 * there before), and everything else the filesystem said (`folder_not_saved`); and the one a read is
 * the one a replacement over a folder refuses outright with (`folder_replace_read_only` — one file
 * that cannot be written stops every file in the run, so that none of them is half applied,
 * `AMB-D-911`); and the one a read is
 * turned away with when the name is a link (`folder_link` — `AMB-D-782` refuses it on purpose, and
 * answering that with the same "not there" every other rule uses told the reader their file was
 * broken). */
export const TAURI_ERROR_CODES = [
  "clip_refused",
  "folder_changed_underneath",
  "folder_link",
  "folder_make",
  "folder_name",
  "folder_not_saved",
  "folder_rename",
  "folder_replace_read_only",
  "folder_taken",
  "folder_unwritable_character",
  "init_ambiguous_owners",
  "init_pointer_exists",
  "binding_nested_tree",
  "migration_failed",
  "migration_running",
  "pointer_other_store",
  "pty_failed",
  "pty_gone",
  "pty_paste_failed",
  "talk_blank",
  "wake_no_config",
  "wake_no_folder",
  "wake_not_kept",
  "wake_not_registered",
  "wake_unknown_agent",
  "window_failed",
] as const;

/** Every code a webview can receive — the contract that i18n and code-based branching may refer to. */
export const ERROR_CODES = [...CORE_ERROR_CODES, ...TAURI_ERROR_CODES] as const;

/** The type of a contract code (single source). */
export type ErrorCode = (typeof ERROR_CODES)[number];

/** Is this string a contract code? (Narrows.) */
export function isErrorCode(s: string): s is ErrorCode {
  return (ERROR_CODES as readonly string[]).includes(s);
}
