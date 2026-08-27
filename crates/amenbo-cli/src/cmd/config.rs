//! `config`: reading this store's configuration, and setting one key of it.

use serde_json::json;

use amenbo_core::{query, Store};

use crate::agent;
use crate::cli::*;
use crate::cmd::binding::upsert_agent_guidance;
use crate::output::{human, print_json, CliError, Flags};

pub(crate) fn config(store: &mut Store, flags: &Flags, sub: Option<ConfigCmd>) -> Result<i32, CliError> {
    if let Some(ConfigCmd::Set { key, value }) = sub {
        store.config.set(&key, &value).map_err(CliError::from)?;
        store.save_config().map_err(CliError::from)?;
        human(flags, format!("Updated setting: {key} = {value}"));
        // When the language changes, resync the managed block in the CWD's AGENTS.md and CLAUDE.md so the
        // language directive follows — otherwise the GUI switches language while the AI keeps writing in the
        // old one.
        if key == "language" {
            if let Ok(cwd) = std::env::current_dir() {
                upsert_agent_guidance(&cwd, store.config.language.as_deref());
            }
        }
        if flags.json {
            print_json(&json!({ "ok": true, "action": "config.set", "noop": false, "key": key, "value": value }));
        }
        return Ok(0);
    }

    let members = query::members(&store.config).count;

    let value = json!({
        "app_version": agent::VERSION,
        "schema_version": agent::SCHEMA_VERSION,
        "paths": {
            "config_file": store.paths.config_file.display().to_string(),
            "data_dir": store.paths.base_dir.display().to_string(),
            "store_file": store.paths.store_file.display().to_string(),
        },
        "settings": {
            "default_view": store.config.default_view.as_str(),
            "language": store.config.language,
            "date_locale": store.config.date_locale,
            "human_name": store.config.human_name,
            "ai_name": store.config.ai_name,
            "human_display_name": store.config.human_display_name(),
            "ai_display_name": store.config.ai_display_name(),
            "ai_allow_project_ops": store.config.ai_allow_project_ops,
            "startup_integrity_check": store.config.startup_integrity_check,
            "update_check": store.config.update_check,
        },
        "sync": {
            // This build ships no sync transport (local-first).
            "enabled": false,
            "members": members,
        },
        "export": { "default_format": "json" }
    });
    if flags.json {
        print_json(&value);
    } else {
        human(flags, format!("store file: {}", store.paths.store_file.display()));
        human(flags, "sync: not available in this build (local-first)");
        // Every setting `--json` carries under `settings`, in the same order, so the two faces of one
        // command answer with the same amount. `app_version` / `schema_version` / `paths` are not
        // repeated — they are already the "store file" line above, and none of them is a setting.
        // Each line names the key `config set` takes, because a value a reader cannot act on is a
        // value they cannot correct; unset says what the absence *means*, not just that it is absent.
        human(flags, format!("default view (default_view): {}", store.config.default_view.as_str()));
        human(flags, format!("language (language): {}", match store.config.language.as_deref() {
            Some(lang) => lang.to_string(),
            None => "not set (English)".to_string(),
        }));
        human(flags, format!("date format (date_locale): {}", match store.config.date_locale.as_deref() {
            Some(tag) => format!("{tag} (read by the GUI; the CLI writes dates one way)"),
            None => "not set (follows the language; read by the GUI only)".to_string(),
        }));
        human(flags, format!("your name (human_name): {}", named_or_default(store.config.human_name.as_deref(), &store.config.human_display_name())));
        human(flags, format!("the AI's name (ai_name): {}", named_or_default(store.config.ai_name.as_deref(), &store.config.ai_display_name())));
        human(flags, format!("AI may archive and delete projects (ai_allow_project_ops): {}", if store.config.ai_allow_project_ops { "on" } else { "off (the AI is refused; the reversible project operations are not gated)" }));
        human(flags, format!("startup integrity check (startup_integrity_check): {} (read-only doctor at open; warnings only)", if store.config.startup_integrity_check { "on" } else { "off" }));
        human(flags, format!("update check (update_check): {} (asks Amenbo's update endpoint whether a newer release is out; infra-side only, no user data; timeout + silent-fail + cached; AMENBO_UPDATE_CHECK=0 overrides)", if store.config.update_check { "on" } else { "off" }));
    }
    Ok(0)
}

/// A display name as the human face writes it: the value that was set, or — when nothing was — the
/// default that stands in for it, said as a default rather than as a value the reader typed. The two
/// name keys are the only settings whose effect is a *different* string from the one stored, and
/// `--json` carries both halves (`human_name` and `human_display_name`), so the line has to as well.
fn named_or_default(set: Option<&str>, effective: &str) -> String {
    match set {
        Some(name) if !name.trim().is_empty() => name.to_string(),
        _ => format!("not set (shown as \"{effective}\")"),
    }
}
