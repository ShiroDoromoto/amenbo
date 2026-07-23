//! **Which installed plugins the catalog has moved past** — the detection half of an update
//! (`AMB-D-359`).
//!
//! There is no central server to ask, and there is no per-plugin request: the catalog amenbo already
//! fetches whole ([`plugin_catalog`], `AMB-D-347`) is the current index, and the manifest beside each
//! installed binary ([`plugin_installed`]) is what this machine has. Detection is those two lists laid
//! side by side, and nothing more — applying an update is a separate, explicit act.
//!
//! **What "a different build" means here.** A manifest carries no version number, so there is nothing to
//! compare as one. What it does carry is the `checksum` of the exact bytes the asset serves, which is the
//! build's identity: two manifests with the same digest point at the same executable, whatever else moved
//! around them, and a digest that differs is a different executable. So the comparison is the checksum —
//! content, not a claim about it. The corollary is that this reports *different*, not *newer*: a catalog
//! that rolls an entry back offers that older build as an update, which is right, because the catalog is
//! the authority on what is published.
//!
//! **It never reaches for the network on its own account.** With nothing installed there is nothing to
//! compare and the catalog is not touched at all; otherwise the read goes through
//! [`plugin_catalog::fresh`], whose freshness boundary means a trigger arriving inside the window is
//! answered from the cache. That is the whole reason a check is cheap enough to hang off a listing
//! (`AMB-D-359`).

use crate::config::Paths;
use crate::error::Result;
use crate::plugin_catalog::{self, Catalog};
use crate::plugin_installed;
use crate::plugin_manifest::Manifest;
use crate::plugin_subscribe::InstalledPlugin;

/// One installed plugin the catalog holds a different build of.
///
/// Both manifests are carried whole rather than a name and two digests: what changed *between* them is
/// what a caller has to act on — a description to show, a config schema that grew a `required` field, a
/// compatibility floor that moved — and the pair is the only place that is readable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    /// The plugin's name — its identity in the catalog and its directory under `plugins/`.
    pub name: String,
    /// The manifest of what is installed on this machine right now.
    pub installed: Manifest,
    /// The manifest the catalog holds for that name.
    pub available: Manifest,
}

/// Whether the catalog's entry is a different build from the installed one (see the module docs: the
/// asset's checksum is the build's identity, so it is the whole comparison).
pub fn differs(installed: &Manifest, available: &Manifest) -> bool {
    installed.checksum != available.checksum
}

/// The updates in one catalog for one set of installed plugins — the pure half, so the rule is testable
/// without a network or a disk.
///
/// Name-sorted, because [`plugin_installed::installed`] is: a listing and a check see the same order. A
/// plugin the catalog does not list is not an update and is passed over — it may have been installed by
/// hand or delisted since, and neither is something this layer can offer to fix.
pub fn compare(installed: &[InstalledPlugin], catalog: &Catalog) -> Vec<Update> {
    installed
        .iter()
        .filter_map(|plugin| {
            let entry = catalog.find(&plugin.name)?;
            differs(&plugin.manifest, &entry.manifest).then(|| Update {
                name: plugin.name.clone(),
                installed: plugin.manifest.clone(),
                available: entry.manifest.clone(),
            })
        })
        .collect()
}

/// Every installed plugin the catalog holds a different build of.
///
/// With nothing installed the answer is "no updates" without a catalog read at all — there is nothing to
/// compare it against, and a check should not spend a fetch to say so. Otherwise the catalog comes from
/// [`plugin_catalog::fresh`], so a check inside the freshness window costs no network at all and one
/// outside it costs a single fetch of the whole index.
///
/// Fails only when there is no catalog to compare against — nothing fetched and nothing cached. Being
/// offline with a cached copy is not a failure: the answer is then as fresh as the cache, which is the
/// deal a static index buys (`AMB-D-347`).
pub fn available(paths: &Paths) -> Result<Vec<Update>> {
    let installed = plugin_installed::installed(paths)?;
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    Ok(compare(&installed, &plugin_catalog::fresh(paths)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_catalog::Entry;
    use crate::plugin_manifest::Os;

    fn manifest(name: &str, checksum: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            desc: "a test plugin".to_string(),
            author: "amenbo".to_string(),
            repo: "ShiroDoromoto/amenbo".to_string(),
            os: vec![Os::Macos, Os::Linux],
            category: "workflow".to_string(),
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: format!("sha256:{checksum}"),
            signature: None,
            official: false,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: Vec::new(),
            events: Vec::new(),
        }
    }

    fn installed_plugin(name: &str, checksum: &str) -> InstalledPlugin {
        InstalledPlugin {
            name: name.to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: manifest(name, checksum),
        }
    }

    fn catalog(entries: Vec<Manifest>) -> Catalog {
        Catalog {
            generated_at: None,
            entries: entries.into_iter().map(|manifest| Entry { manifest, added_at: None }).collect(),
            dropped: Vec::new(),
        }
    }

    /// A digest that moved is a different executable, and that is the whole rule.
    #[test]
    fn a_changed_checksum_is_an_update() {
        let installed = vec![installed_plugin("worktree", "aa")];
        let updates = compare(&installed, &catalog(vec![manifest("worktree", "bb")]));

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "worktree");
        assert_eq!(updates[0].installed.checksum, "sha256:aa");
        assert_eq!(updates[0].available.checksum, "sha256:bb");
    }

    /// The same digest is the same build — everything else in the entry may have been re-written by the
    /// catalog without a byte of the plugin changing.
    #[test]
    fn the_same_checksum_is_not_an_update_however_the_entry_was_rewritten() {
        let installed = vec![installed_plugin("worktree", "aa")];
        let mut entry = manifest("worktree", "aa");
        entry.desc = "a much better description".to_string();
        entry.official = true;

        assert!(compare(&installed, &catalog(vec![entry])).is_empty());
    }

    /// A plugin the catalog does not list has no build to be moved past — installed by hand, or delisted.
    #[test]
    fn a_plugin_the_catalog_does_not_list_is_passed_over() {
        let installed = vec![installed_plugin("homemade", "aa")];
        assert!(compare(&installed, &catalog(vec![manifest("worktree", "bb")])).is_empty());
    }

    /// Several installs are answered in the order they came in — the name-sorted one.
    #[test]
    fn the_updates_keep_the_installed_order() {
        let installed = vec![
            installed_plugin("alpha", "aa"),
            installed_plugin("beta", "bb"),
            installed_plugin("gamma", "cc"),
        ];
        let catalog = catalog(vec![
            manifest("gamma", "cc-new"),
            manifest("alpha", "aa-new"),
            manifest("beta", "bb"),
        ]);

        let names: Vec<_> = compare(&installed, &catalog).into_iter().map(|u| u.name).collect();
        assert_eq!(names, vec!["alpha", "gamma"], "beta is current, and the rest stay sorted");
    }

    /// Nothing installed is answered without a catalog at all — including when there is no cache to read
    /// and no network to reach, which is the state a fresh machine is in.
    #[test]
    fn nothing_installed_is_answered_without_a_catalog() {
        let dir = amenbo_scratch::scratch("plugin-update-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::at(dir);

        assert!(available(&paths).unwrap().is_empty());
    }
}
