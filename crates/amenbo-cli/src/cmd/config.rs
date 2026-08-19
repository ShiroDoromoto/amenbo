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
        human(flags, format!("default view: {}", store.config.default_view.as_str()));
        human(flags, "sync: not available in this build (local-first)");
        human(flags, format!("startup integrity check (startup_integrity_check): {} (read-only doctor at open; warnings only)", if store.config.startup_integrity_check { "on" } else { "off" }));
        human(flags, format!("update check (update_check): {} (asks amenbo's update endpoint whether a newer release is out; infra-side only, no user data; timeout + silent-fail + cached; AMENBO_UPDATE_CHECK=0 overrides)", if store.config.update_check { "on" } else { "off" }));
    }
    Ok(0)
}
