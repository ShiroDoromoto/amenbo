//! **Which installed plugins the catalog has moved past, and putting the new build in place** —
//! detection and application, the two halves of an update (`AMB-D-359`).
//!
//! There is no central server to ask, and there is no per-plugin request: the catalog amenbo already
//! fetches whole ([`plugin_catalog`], `AMB-D-347`) is the current index, and the manifest beside each
//! installed binary ([`plugin_installed`]) is what this machine has. Detection is those two lists laid
//! side by side, and nothing more — applying an update is a separate, explicit act, and it is the second
//! half of this module ([`apply`]). Nothing here is automatic: amenbo never applies an update on its own
//! account (`AMB-D-359` — the same explicit-consent posture as `install ≠ enable`).
//!
//! **Applying re-walks the install door, it does not shortcut it.** The bytes go through
//! [`plugin_install::fetch_verified_program`], so the catalog signature and this OS's checksum are checked
//! over the new asset exactly as they were the first time (`AMB-D-351`); there is no entry point here that
//! fetches without verifying. What is written goes through [`plugin_install::place`], so the install order
//! holds — the executable first, `manifest.json` last.
//!
//! **What an update leaves alone is as much the contract as what it moves.** The enable gate, the config
//! values and the secrets are all keyed by the plugin's name, in stores this module never opens: it
//! replaces one binary and one manifest inside the plugin's home and touches nothing else, so an updated
//! plugin comes back enabled, configured, and consented. Wiping those is `uninstall`'s job and only
//! `uninstall`'s (`AMB-D-357`).
//!
//! **It fails safe at every step.** The compatibility gate, the caller's `approve` gate, the download, the
//! verification and the backup all run before a single byte of the install is overwritten, so a failure in
//! any of them leaves the working plugin exactly as it was. Past that point the previous build is retained
//! beside the new one ([`backup_path`]) — the `.bak` a rollback restores from, in the shape self-update
//! already uses (`AMB-D-341`).
//!
//! **The caller's gate on the new schema.** [`apply`] and [`apply_all`] hand the new manifest to an
//! `approve` closure before writing, and honour a refusal by keeping the working build (`AMB-D-359`). That
//! is where the config re-check lives: a build whose new schema declares a `required` setting an *enabled*
//! plugin has no value for is held back — the same fail-before-write posture as compatibility, aligned with
//! the enable gate that already refuses this at install time (`AMB-D-351`/`AMB-D-356`). This module owns
//! the timing (after "is it a different build", before the network); the caller resolves the values and the
//! enabled state, exactly as [`plugin_trust::enable`](crate::plugin_trust::enable) takes its `has_value`.
//!
//! **What "a different build" means here.** A manifest carries no version number, so there is nothing to
//! compare as one. What it does carry is the `checksum` of the exact bytes the asset serves, which is the
//! build's identity: two manifests with the same digest point at the same executable, whatever else moved
//! around them, and a digest that differs is a different executable. So the comparison is the checksum —
//! content, not a claim about it. **This machine's** checksum, at that: a manifest publishes one
//! distributable per platform (`AMB-D-381`/`AMB-D-384`), so the digest compared is the one for the OS and
//! arch running the check (resolved os-arch then os by [`Manifest::asset_for`]), and a release that rebuilt
//! only Windows — or only linux-arm64 — is not an update for a Mac. The corollary is that this reports
//! *different*, not *newer*: a catalog that rolls an entry back offers that older build as an update,
//! which is right, because the catalog is the authority on what is published.
//!
//! **It never reaches for the network on its own account.** With nothing installed there is nothing to
//! compare and the catalog is not touched at all; otherwise the read goes through
//! [`plugin_catalog::fresh`], whose freshness boundary means a trigger arriving inside the window is
//! answered from the cache. That is the whole reason a check is cheap enough to hang off a listing
//! (`AMB-D-359`).

use std::path::PathBuf;

use crate::config::Paths;
use crate::error::{Error, Result};
use crate::plugin_catalog::{self, Catalog};
use crate::plugin_manifest::{Manifest, Platform};
use crate::plugin_subscribe::InstalledPlugin;
use crate::{plugin_compat, plugin_install, plugin_installed};

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

/// Whether the catalog's entry is a different build from the installed one, **for this platform** (see the
/// module docs: this platform's asset checksum is the build's identity, so it is the whole comparison).
/// The platform is resolved exact-then-OS-wide by [`Manifest::asset_for`] (`AMB-D-384`), so an arch-specific
/// build and an arch-agnostic one compare on the bytes this machine would actually run.
///
/// An entry that publishes nothing for this platform compares as unchanged. It is not an update anyone could
/// apply — there are no bytes to fetch — and reporting one would offer a fix that cannot run.
pub fn differs(installed: &Manifest, available: &Manifest, here: Platform) -> bool {
    match (installed.asset_for(here), available.asset_for(here)) {
        (Some(installed), Some(available)) => installed.checksum != available.checksum,
        _ => false,
    }
}

/// The updates in one catalog for one set of installed plugins — the pure half, so the rule is testable
/// without a network or a disk.
///
/// Name-sorted, because [`plugin_installed::installed`] is: a listing and a check see the same order. A
/// plugin the catalog does not list is not an update and is passed over — it may have been installed by
/// hand or delisted since, and neither is something this layer can offer to fix.
pub fn compare(installed: &[InstalledPlugin], catalog: &Catalog, here: Platform) -> Vec<Update> {
    installed
        .iter()
        .filter_map(|plugin| {
            let entry = catalog.find(&plugin.name)?;
            differs(&plugin.manifest, &entry.manifest, here).then(|| Update {
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
    // A platform amenbo's manifests cannot name publishes nothing here, so there is nothing to compare —
    // and no reason to spend a fetch establishing that.
    let (installed, Some(here)) = (plugin_installed::installed(paths)?, Platform::here()) else {
        return Ok(Vec::new());
    };
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    Ok(compare(&installed, &plugin_catalog::fresh(paths)?, here))
}

/// The updates a **cached** catalog already knows of — the surface a listing shows without reaching for
/// the network (`AMB-D-359`).
///
/// Where [`available`] may spend one fetch past the freshness window, this never does: `plugin list`
/// answers the same offline (`no network, no catalog fetch`), so it reads only what the last catalog fetch
/// left beside the installs and leaves the refetch to the explicit `plugin update --check`. No cache, an
/// unreadable one, or nothing installed is simply no updates — never an error, because a listing does not
/// fail on a catalog it has not got.
#[must_use]
pub fn available_cached(paths: &Paths) -> Vec<Update> {
    let Ok(installed) = plugin_installed::installed(paths) else {
        return Vec::new();
    };
    let (Some(here), Some(catalog)) = (Platform::here(), plugin_catalog::cached(paths)) else {
        return Vec::new();
    };
    compare(&installed, &catalog, here)
}

/// The same comparison as [`available`], against the **current** index rather than a fresh-enough one.
///
/// [`plugin_catalog::fresh`] is right for a check that hangs off something the user did anyway; applying
/// is the explicit act, so it asks the network the way an install does ([`plugin_catalog::load`]) and only
/// falls back to the cache when there is no answer. Applying what a cache said an hour ago would be
/// replacing a binary on stale evidence.
fn pending(paths: &Paths) -> Result<Vec<Update>> {
    let (installed, Some(here)) = (plugin_installed::installed(paths)?, Platform::here()) else {
        return Ok(Vec::new());
    };
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    Ok(compare(&installed, &plugin_catalog::load(paths)?, here))
}

/// What one applied update replaced — the receipt a caller reports from.
#[derive(Clone, Debug)]
pub struct Replaced {
    /// The plugin's name.
    pub name: String,
    /// The manifest that was installed before this ran — what the retained `.bak` is a copy of.
    pub from: Manifest,
    /// The manifest now on disk: the catalog's entry, written last.
    pub to: Manifest,
    /// The executable that was replaced.
    pub program: PathBuf,
    /// Where the previous build is retained ([`backup_path`]).
    pub backup: PathBuf,
    /// How large the new executable is, in bytes.
    pub program_bytes: usize,
}

/// How one plugin fared in an [`apply_all`] — a failure is a value, not the end of the run, because one
/// plugin whose asset will not verify must not leave the rest un-updated.
#[derive(Debug)]
pub enum Outcome {
    /// The new build is in place. Boxed because a receipt carries two whole manifests and a refusal
    /// carries a sentence — sized inline, every element of the list would pay for the larger one.
    Replaced(Box<Replaced>),
    /// Nothing was replaced, and the plugin is as it was.
    Failed {
        /// The plugin that could not be updated.
        name: String,
        /// Why — the refusal the single-plugin path would have returned.
        error: Error,
    },
}

/// Where the previous build is retained inside the plugin's home: `<name>` → `<name>.bak` (and
/// `<name>.exe` → `<name>.bak` on Windows), the shape self-update already uses (`AMB-D-341`).
///
/// **One**, not a history: each update overwrites it, so a rollback goes back exactly one build. A sibling
/// in the same directory, so retaining it is a plain copy — and `uninstall` removes the whole home, which
/// takes the retained build with it.
///
/// It is invisible to everything that reads an install: [`plugin_installed`] knows only `manifest.json`
/// and the executable's own name, so a `.bak` beside them is not a second plugin and not a broken one.
#[must_use]
pub fn backup_path(paths: &Paths, name: &str) -> PathBuf {
    plugin_installed::program_path(paths, name).with_extension("bak")
}

/// The manifest retained beside the previous build, so a rollback restores a *pair* that agrees.
///
/// Without it a rollback would put the old binary back under the new manifest — the wrong checksum, the
/// wrong config schema, the wrong compatibility floor — which is a broken install described as a working
/// one. Same role as self-update's version sidecar (`AMB-D-341`), one file up in fidelity because a
/// plugin's manifest carries more than a label.
#[must_use]
pub fn backup_manifest_path(paths: &Paths, name: &str) -> PathBuf {
    let mut p = plugin_installed::manifest_path(paths, name).into_os_string();
    p.push(".bak");
    PathBuf::from(p)
}

/// Apply the update for one installed plugin: fetch the catalog's build, verify it, retain the current
/// one, and put the new one in place.
///
/// `Ok(None)` means there was nothing to apply — the catalog publishes the build already installed — and
/// nothing was fetched or written. That is a result, not a failure: `plugin update <name>` on a current
/// plugin is a no-op a caller can report plainly.
///
/// Refuses, leaving the install untouched, when the plugin is not installed (that is `plugin install`'s
/// door), when the catalog does not list it (installed by hand, or delisted since — there is no build to
/// move to), when the offered build cannot run on this amenbo, when the caller's `approve` gate holds it
/// back (a new `required` setting an enabled plugin has no value for — `AMB-D-359`), or when its asset does
/// not verify.
///
/// `approve` is handed the new manifest after this confirms it is a different build and before anything is
/// fetched or written; returning `Err` keeps the working build in place. A caller with no gate to add
/// passes `|_| Ok(())`.
pub fn apply(
    paths: &Paths,
    name: &str,
    approve: impl FnOnce(&Manifest) -> Result<()>,
) -> Result<Option<Replaced>> {
    let installed = plugin_installed::read(paths, name)?;
    // Unreachable on a platform amenbo ships for, and a refusal rather than "nothing to do" if it ever is
    // reached: a caller who named a plugin is owed the reason, not a silent no-op.
    let here = Platform::here().ok_or_else(|| {
        Error::invalid(
            format!("plugin manifests cannot name {}, so '{name}' cannot be updated here", std::env::consts::OS),
            format!("プラグインの manifest は {} を名指せないので、ここでは '{name}' を更新できません", std::env::consts::OS),
        )
    })?;
    let catalog = plugin_catalog::load(paths)?;
    let entry = catalog.find(name).ok_or_else(|| {
        Error::not_found(
            format!("the catalog lists no plugin named '{name}', so there is no build to update to"),
            format!("目録にプラグイン '{name}' は無いので、更新先の版がありません"),
        )
    })?;
    if !differs(&installed.manifest, &entry.manifest, here) {
        return Ok(None);
    }
    // The caller's gate on the new schema, before the network (see the module docs): a refusal keeps the
    // working build exactly as it was.
    approve(&entry.manifest)?;
    replace(
        paths,
        &Update {
            name: name.to_string(),
            installed: installed.manifest,
            available: entry.manifest.clone(),
        },
    )
    .map(Some)
}

/// Apply every update the catalog holds, one plugin at a time.
///
/// Best-effort across plugins, exact within one: each is applied through the same [`replace`] the
/// single-plugin path uses, so a plugin that fails is left exactly as it was and the next one is still
/// attempted (`AMB-D-352`'s posture — one plugin's problem is not the others'). Plugins already on the
/// catalog's build are not in the result at all; there was nothing to do for them.
///
/// `approve` gates each plugin's new manifest the same way [`apply`] does, per plugin: one held back for an
/// unsatisfied `required` (`AMB-D-359`) is a [`Outcome::Failed`] carrying that reason, and the rest are
/// still applied. A caller with no gate passes `|_| Ok(())`.
pub fn apply_all(
    paths: &Paths,
    approve: impl Fn(&Manifest) -> Result<()>,
) -> Result<Vec<Outcome>> {
    Ok(pending(paths)?
        .into_iter()
        .map(|update| match approve(&update.available).and_then(|()| replace(paths, &update)) {
            Ok(replaced) => Outcome::Replaced(Box::new(replaced)),
            Err(error) => Outcome::Failed { name: update.name, error },
        })
        .collect())
}

/// Put one resolved update in place — the whole write path, and the only one.
///
/// The order is the safety story (see the module docs): the two gates that can refuse run before the
/// network, and the network runs before anything on disk is touched, so a plugin that fails any of them
/// is left exactly as it was.
fn replace(paths: &Paths, update: &Update) -> Result<Replaced> {
    // A build this amenbo cannot speak to is not an improvement. Refusing here keeps a working plugin
    // working — the alternative is replacing it with one that will be dropped at dispatch.
    if let Err(why) = plugin_compat::check(&update.available) {
        return Err(why.into_update_error(&update.name));
    }
    // Off the network and through the trust gates (`AMB-D-351`) — the same door the first install used.
    let program = plugin_install::fetch_verified_program(&update.available)?;
    retain_and_place(paths, update, &program)
}

/// The disk half of [`replace`], with the verified bytes handed in — the seam a test drives, since
/// everything above it needs a network.
///
/// Retain first, overwrite second: the previous executable **and** the manifest describing it are copied
/// aside before [`plugin_install::place`] writes over either, so the pair a rollback needs is complete
/// from the first byte of the replacement onward.
fn retain_and_place(paths: &Paths, update: &Update, program: &[u8]) -> Result<Replaced> {
    let name = &update.name;
    std::fs::copy(plugin_installed::program_path(paths, name), backup_path(paths, name))?;
    std::fs::copy(plugin_installed::manifest_path(paths, name), backup_manifest_path(paths, name))?;

    let placed = plugin_install::place(paths, &update.available, program)?;
    Ok(Replaced {
        name: name.clone(),
        from: update.installed.clone(),
        to: placed.manifest,
        program: placed.program,
        backup: backup_path(paths, name),
        program_bytes: placed.program_bytes,
    })
}

/// What one rollback restored — the receipt a caller reports from.
#[derive(Clone, Debug)]
pub struct RolledBack {
    /// The plugin's name.
    pub name: String,
    /// The manifest now on disk again: the build that was retained.
    pub restored: Manifest,
    /// The executable the restore wrote.
    pub program: PathBuf,
}

/// Undo the last [`apply`] for one plugin, restoring the build retained beside it (`AMB-D-359`).
///
/// The mirror of an apply, and offline: an update kept the previous executable and the manifest that
/// described it as a `.bak` pair ([`backup_path`]/[`backup_manifest_path`]), and this puts both back —
/// the pair, never one without the other, so the installed manifest never disagrees with the bytes beside
/// it. Nothing is fetched and nothing is verified: the retained build already passed the door on its way
/// in, and a rollback is a deliberate return to it (the same posture self-update's `--rollback` takes,
/// `AMB-D-341`).
///
/// The restore reuses [`plugin_install::place`], so the install order holds here too — the executable
/// first, `manifest.json` last — and an interrupted rollback reads as not-finished rather than as a home
/// half in each build. On success the retained copies are consumed: an update retains exactly one prior
/// build, so once it is the running one there is nothing further back to go, and a stale `.bak` must not
/// masquerade as a fresh one.
///
/// Refuses, changing nothing, when the plugin is not installed (there is nothing to roll back), or when
/// no retained build exists — either it was never updated, or a prior rollback already consumed the one
/// `.bak` there is. The enable gate, the settings and the secrets are keyed elsewhere and left untouched,
/// exactly as an update leaves them.
pub fn rollback(paths: &Paths, name: &str) -> Result<RolledBack> {
    // Anchor on a real install: a rollback restores an executable *beside* a live plugin, so a name that
    // is not installed is `not_found`, not a bare "no backup".
    plugin_installed::read(paths, name)?;

    let backup_program = backup_path(paths, name);
    let backup_manifest = backup_manifest_path(paths, name);
    if !backup_program.exists() {
        return Err(Error::not_found(
            format!("plugin '{name}' has no retained build to roll back to — it was not updated, or a rollback already used it"),
            format!("プラグイン '{name}' に戻せる版がありません——更新していないか、既にロールバック済みです"),
        ));
    }

    // Read both retained files before writing either: a rollback that put the old binary back under the
    // new manifest would describe the old build with the wrong checksum, config schema and floor.
    let program = std::fs::read(&backup_program)?;
    let raw = std::fs::read_to_string(&backup_manifest).map_err(|e| {
        Error::invalid(
            format!("plugin '{name}' has a retained binary but no manifest beside it ({e}) — the pair a rollback needs is incomplete"),
            format!("プラグイン '{name}' に退避バイナリはありますが manifest がありません（{e}）——ロールバックに必要な対が欠けています"),
        )
    })?;
    let manifest: Manifest = serde_json::from_str(&raw).map_err(|e| {
        Error::invalid(
            format!("plugin '{name}' has a retained manifest that will not parse: {e}"),
            format!("プラグイン '{name}' の退避 manifest を読めません：{e}"),
        )
    })?;

    let placed = plugin_install::place(paths, &manifest, &program)?;
    // The retained pair is now the running build — there is nothing further back to go. Clear both so a
    // stale `.bak` cannot later masquerade as a fresh one (same close-out as self-update, `AMB-D-341`).
    let _ = std::fs::remove_file(&backup_program);
    let _ = std::fs::remove_file(&backup_manifest);

    Ok(RolledBack { name: name.to_string(), restored: placed.manifest, program: placed.program })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_catalog::Entry;
    use crate::plugin_manifest::{Arch, Asset, Os};

    /// An arch-agnostic platform key (`<os>`).
    fn plat(os: Os) -> Platform {
        Platform { os, arch: None }
    }

    /// An arch-specific platform key (`<os>-<arch>`).
    fn plat_arch(os: Os, arch: Arch) -> Platform {
        Platform { os, arch: Some(arch) }
    }

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
            assets: Default::default(),
            official: false,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: Vec::new(),
            events: Vec::new(),
        }
    }

    fn asset(checksum: &str) -> Asset {
        Asset {
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: format!("sha256:{checksum}"),
            signature: None,
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
        let updates = compare(&installed, &catalog(vec![manifest("worktree", "bb")]), plat(Os::Macos));

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

        assert!(compare(&installed, &catalog(vec![entry]), plat(Os::Macos)).is_empty());
    }

    /// A plugin the catalog does not list has no build to be moved past — installed by hand, or delisted.
    #[test]
    fn a_plugin_the_catalog_does_not_list_is_passed_over() {
        let installed = vec![installed_plugin("homemade", "aa")];
        assert!(compare(&installed, &catalog(vec![manifest("worktree", "bb")]), plat(Os::Macos)).is_empty());
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

        let names: Vec<_> = compare(&installed, &catalog, plat(Os::Macos)).into_iter().map(|u| u.name).collect();
        assert_eq!(names, vec!["alpha", "gamma"], "beta is current, and the rest stay sorted");
    }

    /// A per-OS entry is compared **per OS** (`AMB-D-381`): the machine running the check sees only its own
    /// asset move. A release that rebuilt one platform is not an update for the others.
    #[test]
    fn a_per_os_entry_is_compared_against_this_machines_asset() {
        let mut installed_manifest = manifest("worktree", "unused");
        installed_manifest.url = String::new();
        installed_manifest.checksum = String::new();
        installed_manifest.assets = [
            (plat(Os::Macos), asset("mac-1")),
            (plat(Os::Linux), asset("linux-1")),
        ]
        .into_iter()
        .collect();
        let installed = vec![InstalledPlugin {
            name: "worktree".to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: installed_manifest.clone(),
        }];

        // Only Linux was rebuilt.
        let mut entry = installed_manifest.clone();
        entry.assets.insert(plat(Os::Linux), asset("linux-2"));

        assert!(
            compare(&installed, &catalog(vec![entry.clone()]), plat(Os::Macos)).is_empty(),
            "the mac asset is the one it was"
        );
        let on_linux = compare(&installed, &catalog(vec![entry]), plat(Os::Linux));
        assert_eq!(on_linux.len(), 1, "the linux asset moved");
        assert_eq!(
            on_linux[0].available.asset_for(plat(Os::Linux)).unwrap().checksum,
            "sha256:linux-2"
        );
    }

    /// The comparison is per **platform**, not just per OS (`AMB-D-384`): a release that rebuilt only
    /// linux-arm64 is an update for an arm64 machine and not for an x64 one, each seeing its own asset move.
    #[test]
    fn a_per_arch_entry_is_compared_against_this_machines_arch() {
        let mut installed_manifest = manifest("worktree", "unused");
        installed_manifest.url = String::new();
        installed_manifest.checksum = String::new();
        installed_manifest.os = vec![Os::Linux];
        installed_manifest.assets = [
            (plat_arch(Os::Linux, Arch::X64), asset("x64-1")),
            (plat_arch(Os::Linux, Arch::Arm64), asset("arm64-1")),
        ]
        .into_iter()
        .collect();

        // Only the arm64 build moved.
        let mut entry = installed_manifest.clone();
        entry.assets.insert(plat_arch(Os::Linux, Arch::Arm64), asset("arm64-2"));

        // The arm64 machine sees the move; the x64 machine sees the digest it always had.
        assert!(differs(&installed_manifest, &entry, plat_arch(Os::Linux, Arch::Arm64)), "arm64 moved");
        assert!(!differs(&installed_manifest, &entry, plat_arch(Os::Linux, Arch::X64)), "x64 is unchanged");

        let installed = vec![InstalledPlugin {
            name: "worktree".to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: installed_manifest,
        }];
        assert_eq!(
            compare(&installed, &catalog(vec![entry.clone()]), plat_arch(Os::Linux, Arch::Arm64)).len(),
            1,
            "the arm64 asset moved"
        );
        assert!(
            compare(&installed, &catalog(vec![entry]), plat_arch(Os::Linux, Arch::X64)).is_empty(),
            "the x64 asset is the one it was"
        );
    }

    /// An entry that publishes nothing for this OS offers no update: there would be no bytes to apply.
    #[test]
    fn an_entry_with_no_asset_here_is_not_an_update() {
        let mut installed_manifest = manifest("worktree", "unused");
        installed_manifest.url = String::new();
        installed_manifest.checksum = String::new();
        installed_manifest.assets = [(plat(Os::Linux), asset("linux-1"))].into_iter().collect();
        let installed = vec![InstalledPlugin {
            name: "worktree".to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: installed_manifest.clone(),
        }];
        let mut entry = installed_manifest.clone();
        entry.assets.insert(plat(Os::Linux), asset("linux-2"));

        assert!(compare(&installed, &catalog(vec![entry]), plat(Os::Macos)).is_empty());
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

    // ---- applying one ----

    fn paths_at(tag: &str) -> Paths {
        let dir = amenbo_scratch::scratch(&format!("plugin-update-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Paths::at(dir)
    }

    /// An installed plugin on disk: the executable holding `program`, and the manifest beside it.
    fn install_on_disk(paths: &Paths, manifest: &Manifest, program: &[u8]) {
        let home = paths.plugin_dir(&manifest.name);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(plugin_installed::program_path(paths, &manifest.name), program).unwrap();
        std::fs::write(
            plugin_installed::manifest_path(paths, &manifest.name),
            serde_json::to_string(manifest).unwrap(),
        )
        .unwrap();
    }

    fn update_of(installed: Manifest, available: Manifest) -> Update {
        Update { name: installed.name.clone(), installed, available }
    }

    /// The shape of the retained pair — a sibling of the executable, and of the manifest.
    #[test]
    fn the_previous_build_is_retained_beside_the_new_one() {
        let paths = paths_at("backup-paths");
        let home = paths.plugin_dir("worktree");

        assert_eq!(backup_path(&paths, "worktree"), home.join("worktree.bak"));
        assert_eq!(backup_manifest_path(&paths, "worktree"), home.join("manifest.json.bak"));
    }

    /// The heart of an apply: the new executable is in place, the previous one **and** the manifest that
    /// described it are retained, and the install reads back as the catalog's build.
    #[test]
    fn applying_replaces_the_build_and_retains_the_previous_pair() {
        let paths = paths_at("apply");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"#!/bin/sh\nold\n");
        let mut after = manifest("worktree", "bb");
        after.desc = "the newer build".to_string();

        let replaced =
            retain_and_place(&paths, &update_of(before.clone(), after.clone()), b"#!/bin/sh\nnew\n")
                .unwrap();

        assert_eq!(std::fs::read(&replaced.program).unwrap(), b"#!/bin/sh\nnew\n");
        assert_eq!(replaced.program_bytes, b"#!/bin/sh\nnew\n".len());
        assert_eq!(replaced.from.checksum, "sha256:aa");
        assert_eq!(replaced.to.checksum, "sha256:bb");

        // The retained pair: the previous binary, and the manifest that goes with it. A rollback that
        // restored one without the other would describe the old build with the new manifest.
        assert_eq!(std::fs::read(&replaced.backup).unwrap(), b"#!/bin/sh\nold\n");
        let retained: Manifest = serde_json::from_slice(
            &std::fs::read(backup_manifest_path(&paths, "worktree")).unwrap(),
        )
        .unwrap();
        assert_eq!(retained, before);

        // And what is installed now is the catalog's build, read back through the ordinary reader.
        let read = plugin_installed::read(&paths, "worktree").unwrap();
        assert_eq!(read.manifest, after);
        assert!(compare(&[read], &catalog(vec![after]), plat(Os::Macos)).is_empty(), "and it is current");
    }

    /// A second update overwrites the retained build rather than accumulating: a rollback goes back one
    /// step, and only one (`AMB-D-359` — one `.bak`).
    #[test]
    fn a_second_update_retains_only_the_build_it_replaced() {
        let paths = paths_at("one-backup");
        let first = manifest("worktree", "aa");
        install_on_disk(&paths, &first, b"one");
        let second = manifest("worktree", "bb");
        let third = manifest("worktree", "cc");

        retain_and_place(&paths, &update_of(first, second.clone()), b"two").unwrap();
        retain_and_place(&paths, &update_of(second.clone(), third), b"three").unwrap();

        assert_eq!(std::fs::read(backup_path(&paths, "worktree")).unwrap(), b"two");
        let retained: Manifest = serde_json::from_slice(
            &std::fs::read(backup_manifest_path(&paths, "worktree")).unwrap(),
        )
        .unwrap();
        assert_eq!(retained, second, "the retained manifest moved with the retained binary");
    }

    /// What an update leaves alone: the enable gate, the settings and the secrets are keyed by name in
    /// stores this module never opens, so an updated plugin comes back exactly as consented. The
    /// stand-in here is the home itself — nothing beside the two replaced files is touched.
    #[test]
    fn applying_touches_nothing_but_the_binary_and_its_manifest() {
        let paths = paths_at("preserve");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        let stray = paths.plugin_dir("worktree").join("state.json");
        std::fs::write(&stray, b"whatever the plugin keeps").unwrap();

        retain_and_place(&paths, &update_of(before, manifest("worktree", "bb")), b"new").unwrap();

        assert_eq!(std::fs::read(&stray).unwrap(), b"whatever the plugin keeps");
    }

    /// A build this amenbo cannot speak to is refused before the network is reached, and the working
    /// install is left exactly as it was (`AMB-D-359` — failing safe).
    #[test]
    fn an_incompatible_new_build_is_refused_with_the_install_untouched() {
        let paths = paths_at("incompatible");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        let mut after = manifest("worktree", "bb");
        after.min_amenbo = Some("99.0.0".to_string());

        let err = replace(&paths, &update_of(before.clone(), after)).unwrap_err();
        assert_eq!(err.code(), "invalid_value");
        assert!(format!("{err:?}").contains("99.0.0"), "the floor is named: {err:?}");

        assert_eq!(std::fs::read(plugin_installed::program_path(&paths, "worktree")).unwrap(), b"old");
        assert_eq!(plugin_installed::read(&paths, "worktree").unwrap().manifest, before);
        assert!(!backup_path(&paths, "worktree").exists(), "and nothing was retained");
    }

    /// Updating what is not installed is `plugin install`'s door, not this one — and it is answered
    /// before a catalog is ever fetched.
    #[test]
    fn updating_something_not_installed_is_not_found() {
        let paths = paths_at("absent");
        assert_eq!(apply(&paths, "worktree", |_| Ok(())).unwrap_err().code(), "not_found");
    }

    // ---- rolling one back ----

    /// The round trip: install, apply an update through the disk seam, then roll back — and the binary,
    /// the manifest, and the reader's verdict are all the pre-update build again.
    #[test]
    fn rolling_back_restores_the_build_the_update_replaced() {
        let paths = paths_at("rollback");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"#!/bin/sh\nold\n");
        let after = manifest("worktree", "bb");
        retain_and_place(&paths, &update_of(before.clone(), after), b"#!/bin/sh\nnew\n").unwrap();

        let rolled = rollback(&paths, "worktree").unwrap();
        assert_eq!(rolled.restored, before);
        assert_eq!(std::fs::read(&rolled.program).unwrap(), b"#!/bin/sh\nold\n");
        assert_eq!(plugin_installed::read(&paths, "worktree").unwrap().manifest, before);
    }

    /// One `.bak`, one step back: the retained pair is consumed, so a second rollback has nothing to
    /// restore (`AMB-D-341` — a rollback goes back exactly one build).
    #[test]
    fn a_rollback_consumes_the_retained_build() {
        let paths = paths_at("rollback-once");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        retain_and_place(&paths, &update_of(before, manifest("worktree", "bb")), b"new").unwrap();

        rollback(&paths, "worktree").unwrap();
        assert!(!backup_path(&paths, "worktree").exists(), "the retained binary is gone");
        assert!(!backup_manifest_path(&paths, "worktree").exists(), "and its manifest with it");
        assert_eq!(rollback(&paths, "worktree").unwrap_err().code(), "not_found");
    }

    /// A plugin that was never updated has no retained build: the rollback says so and changes nothing.
    #[test]
    fn rolling_back_a_plugin_that_was_never_updated_is_not_found() {
        let paths = paths_at("rollback-fresh");
        install_on_disk(&paths, &manifest("worktree", "aa"), b"only");

        assert_eq!(rollback(&paths, "worktree").unwrap_err().code(), "not_found");
        assert_eq!(std::fs::read(plugin_installed::program_path(&paths, "worktree")).unwrap(), b"only");
    }

    /// Rolling back what is not installed anchors on the missing install, not on a missing backup.
    #[test]
    fn rolling_back_something_not_installed_is_not_found() {
        let paths = paths_at("rollback-absent");
        assert_eq!(rollback(&paths, "worktree").unwrap_err().code(), "not_found");
    }

    /// The gate, the settings and the secrets are keyed elsewhere: a rollback restores the two files it
    /// retained and leaves the rest of the home alone, exactly as an update does.
    #[test]
    fn rolling_back_touches_nothing_but_the_binary_and_its_manifest() {
        let paths = paths_at("rollback-preserve");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        retain_and_place(&paths, &update_of(before, manifest("worktree", "bb")), b"new").unwrap();
        let stray = paths.plugin_dir("worktree").join("state.json");
        std::fs::write(&stray, b"kept across the round trip").unwrap();

        rollback(&paths, "worktree").unwrap();
        assert_eq!(std::fs::read(&stray).unwrap(), b"kept across the round trip");
    }
}
