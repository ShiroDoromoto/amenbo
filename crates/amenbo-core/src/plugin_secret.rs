//! The **plugin secret file** — where a `secret` plugin config field is stored (`AMB-D-356`).
//!
//! amenbo does not judge what is secret; the plugin author declares it, field by field
//! ([`crate::plugin_manifest::ConfigField::secret`]). A field marked secret must never touch the store
//! or a backup, so it is routed here instead of to the two text tiers ([`crate::config::Config`] machine
//! default + the `plugin_config` store table). This file is that home: a single JSON document, `<base>/
//! plugin-secrets.json`, sitting flat under the base beside `config.json` — the same user area as
//! amenbo's own identity — but deliberately **outside the source of truth and outside every
//! backup/export** (`backup` snapshots `store.sqlite`, `export` walks the record tables; this file is
//! neither).
//!
//! Two things set it apart from [`crate::config::Config`]'s own file:
//!
//! - **Owner-only on disk (0600).** The file is created with mode 0600 on unix, so a secret is never
//!   world-readable even for the window between write and a later `chmod`. Off unix the mode is a no-op
//!   (the file still lives in the per-user app-data area).
//! - **A parse error is refused, not defaulted.** [`crate::config::Config::load`] falls back to defaults
//!   on a corrupt file because losing a preference is cheap. Losing a secret is not: defaulting to an
//!   empty map and then saving would *destroy* every secret on disk. So [`Secrets::load`] returns an
//!   error on a malformed file and only treats an **absent** file as empty (no secrets yet is the
//!   ordinary first state).
//!
//! The value is central and injected at run time (`AMB-T-2016`): a plugin never reads this file itself —
//! amenbo takes the secret out and hands it to the plugin process as an environment variable, off argv
//! and off logs.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The on-disk secret store: plugin name → field key → secret value. `transparent` so the file *is* the
/// map — a plain `{ "<plugin>": { "<key>": "<value>" } }` document, the same shape as the machine-default
/// text config, with nothing wrapped around it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secrets(BTreeMap<String, BTreeMap<String, String>>);

impl Secrets {
    /// Read the secret file. An **absent** file is the ordinary empty state (no secret has been set yet)
    /// and yields an empty store; a **malformed** file is an error, never silently emptied — defaulting
    /// and re-saving would erase the very secrets the file exists to hold.
    pub fn load(path: &Path) -> Result<Secrets> {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).map_err(|e| {
                Error::invalid(
                    format!("plugin secret file is malformed ({}): {e}", path.display()),
                    format!("プラグインの秘密ファイルが壊れています（{}）: {e}", path.display()),
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Secrets::default()),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// One plugin field's secret value, if set.
    pub fn get(&self, plugin: &str, key: &str) -> Option<&str> {
        self.0.get(plugin)?.get(key).map(String::as_str)
    }

    /// Set (`Some`) or clear (`None`) one plugin field's secret. Clearing removes the key, and the
    /// plugin's map with it once empty, so an unset field leaves no `{}` residue. Does **not** persist —
    /// the caller [`save`](Self::save)s.
    pub fn set(&mut self, plugin: &str, key: &str, value: Option<&str>) {
        match value {
            Some(v) => {
                self.0.entry(plugin.to_string()).or_default().insert(key.to_string(), v.to_string());
            }
            None => {
                if let Some(fields) = self.0.get_mut(plugin) {
                    fields.remove(key);
                    if fields.is_empty() {
                        self.0.remove(plugin);
                    }
                }
            }
        }
    }

    /// Write the secret file back, owner-only (0600 on unix), creating the base dir as needed. Atomic:
    /// the content is written to a sibling temp created with the restrictive mode from the start, then
    /// renamed into place, so a reader never sees a torn file and a secret is never briefly world-readable.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        write_owner_only(path, json.as_bytes())
    }
}

/// Write `contents` to `path` atomically and owner-only. The temp is created with mode 0600 **before**
/// any bytes land in it (unix), so the secret is never readable by anyone but the owner, not even for the
/// instant between create and a later `chmod`. Off unix the mode is a no-op — the file still lives in the
/// per-user app-data area — so the write is a plain create + rename.
fn write_owner_only(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write as _;

    let tmp = path.with_extension("tmp");

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(contents)?;
    f.sync_all()?;
    drop(f);

    // An existing file at `path` may predate this mode (e.g. written by an older build with the default
    // umask); the rename replaces it, and the new inode carries 0600.
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        amenbo_scratch::scratch(&format!("plugin-secret-{tag}"))
    }

    #[test]
    fn an_absent_file_is_empty_not_an_error() {
        let dir = scratch("absent");
        let s = Secrets::load(&dir.join("plugin-secrets.json")).unwrap();
        assert_eq!(s, Secrets::default(), "no file yet ⇒ no secrets, not a failure");
    }

    #[test]
    fn a_secret_round_trips_through_the_file() {
        let dir = scratch("roundtrip");
        let path = dir.join("plugin-secrets.json");

        let mut s = Secrets::default();
        s.set("slack", "webhook_url", Some("https://hooks.example/xyz"));
        s.save(&path).unwrap();

        let back = Secrets::load(&path).unwrap();
        assert_eq!(back.get("slack", "webhook_url"), Some("https://hooks.example/xyz"));
        assert_eq!(back, s);
    }

    #[test]
    fn clearing_a_field_removes_the_plugin_once_empty() {
        let dir = scratch("clear");
        let path = dir.join("plugin-secrets.json");

        let mut s = Secrets::default();
        s.set("slack", "webhook_url", Some("v"));
        s.set("slack", "webhook_url", None);
        assert_eq!(s, Secrets::default(), "the last field cleared leaves no empty plugin map behind");

        // And it round-trips as an empty document.
        s.save(&path).unwrap();
        assert_eq!(Secrets::load(&path).unwrap(), Secrets::default());
    }

    #[test]
    fn a_malformed_file_is_refused_not_silently_emptied() {
        let dir = scratch("malformed");
        let path = dir.join("plugin-secrets.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"{ this is not json").unwrap();

        let err = Secrets::load(&path).unwrap_err().to_string();
        assert!(
            err.contains("malformed") || err.contains("壊れて"),
            "a corrupt secret file must error rather than default to empty (which would erase it): {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = scratch("perms");
        let path = dir.join("plugin-secrets.json");

        let mut s = Secrets::default();
        s.set("p", "k", Some("v"));
        s.save(&path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the secret file must be owner-only, got {mode:o}");
    }
}
