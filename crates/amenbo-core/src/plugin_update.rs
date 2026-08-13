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
//! [`plugin_install::fetch_verified_program`], so the signature and this OS's checksum are checked over
//! the new asset exactly as they were the first time (`AMB-D-351`); there is no entry point here that
//! fetches without verifying. The key they are checked against is the one the catalog serving the new
//! build answers for (`AMB-D-389`), taken from the same resolution — an update is not a way to install
//! bytes from a catalog whose key nothing was pinned to.
//!
//! **What an update resolves against is the catalog the install came from**
//! ([`plugin_installed::Origin`], `AMB-D-389`) — not the merged view by name. The merged view is how a
//! plugin is *found*, and a name in it belongs to whoever the fold gave it to; an update is not looking
//! for a plugin, it is replacing bytes that came from somewhere, and that somewhere is recorded beside
//! them. Resolving by name would mean a catalog earlier in the order publishing that name became the
//! place the next update fetched from — and, since each catalog answers for its own key, the swap would
//! verify against the new publisher's key and look exactly like an ordinary update. A name the recorded
//! catalog has stopped carrying is therefore no update at all, which is the honest answer: the shelf that
//! served this build has nothing newer on it. What is written goes through [`plugin_install::place`], so
//! the install order holds — the executable first, `manifest.json` last.
//!
//! **What an update leaves alone is as much the contract as what it moves.** The enable gate and the
//! values the new build still declares are keyed by the plugin's name and are not touched, so an updated
//! plugin comes back enabled, configured, and consented. Wiping a plugin's settings wholesale is
//! `uninstall`'s job and only `uninstall`'s (`AMB-D-357`).
//!
//! **The one exception is a value nothing declares any more** (`AMB-D-456`): once the new build and its
//! manifest are in place, every project's stored value under a key the new schema has stopped naming is
//! purged ([`plugin_config::purge_undeclared`]). Nothing would ever read it again — injection is handed
//! the schema, and both faces draw their forms from it — so what it would outlive its declaration inside
//! is `backup` and `export`. A rollback does not bring it back: what is retained is the build and its
//! manifest, one generation deep, and stretching that to the settings would make "one generation" mean
//! something different for each half.
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
//! **What "a different build" means here: `detail_sum`, and nothing else** (`AMB-D-438`). A manifest
//! carries no version number, so there is nothing to compare as one. What the list carries is one digest
//! per entry — `detail_sum`, over the whole document an install reads — and a value there that is not the
//! one the install recorded is the whole of what an update is. That document is everything an install acts
//! on (`AMB-D-385`): the assets and their digests, the config schema, the subscriptions, the compatibility
//! floor, and what the plugin says for itself at the AI's entry point. So the comparison covers what the
//! asset checksums could not — **an update that changes no binary is a real update**, and a check that
//! compared executables would keep every one of them from ever reaching anyone. The corollary
//! is that this reports *different*, not *newer*: a catalog that rolls an entry back offers that older
//! build as an update, which is right, because the catalog is the authority on what is published.
//!
//! The asset checksums are still checked, at the door where they mean something: the bytes that arrive
//! must be the bytes the entry published (`AMB-D-351`). What they are not is how a move is *noticed* —
//! that question is the list's, and one answer serves both faces, so a listing's mark and an update check
//! can no longer disagree about the same plugin.
//!
//! **Detection is therefore two steps, and only the first one judges** (`AMB-D-386`). The list everyone
//! fetches makes a plugin whose digest moved a [`Candidate`]; the document that digest covers is then
//! fetched for its *content* — the config schema, the compatibility floor, the asset to fetch — which is
//! everything a caller needs to decide and to apply ([`confirm`]). A [`Candidate`] is what a listing can
//! report without a request per plugin ([`available_cached`]), and an [`Update`] is the same plugin with
//! that document in hand.
//!
//! **It never reaches for the network on its own account.** With nothing installed there is nothing to
//! compare and the catalog is not touched at all; otherwise the list goes through
//! [`plugin_catalog::fresh`], whose freshness boundary means a trigger arriving inside the window is
//! answered from the cache. What a check costs beyond that is one small document per plugin whose digest
//! moved — nothing when nothing did, which is the ordinary case, and that is what keeps a check cheap
//! enough to hang off a listing (`AMB-D-359`). Somebody who asks past the window gets a fetch and only
//! then ([`Reach::Now`]) — the window is a default, not a ceiling.
//!
//! **And every check says what it was measured against** ([`Against`]). The boundary that makes a check
//! cheap also makes "nothing has changed" and "nothing had changed an hour ago" the same rows and the same
//! count, so the read is carried out beside the verdict rather than left for a reader to assume.

use std::path::PathBuf;

use crate::config::Paths;
use crate::error::{Error, ErrorCode, Msg, Result};
use crate::plugin_catalog::{self, DiscoveredEntry, Discovery};
use crate::plugin_installed::Origin;
use crate::plugin_config::Purged;
use crate::plugin_manifest::{Manifest, Platform, Translations};
use crate::plugin_subscribe::InstalledPlugin;
use crate::plugin_wire::ListEntry;
use crate::store::Store;
use crate::{plugin_compat, plugin_config, plugin_install, plugin_installed};

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
    /// What the catalog says about that build in other languages (`AMB-D-622`) — read off the same
    /// documents [`available`](Update::available) was joined from, and put beside the plugin when the
    /// build is. Carried here rather than re-read at the write, so an apply replaces the pair the
    /// catalog published together.
    pub available_i18n: Translations,
}

/// One installed plugin the catalog's **list** already says something has moved about — an update, before
/// the document that says *what moved* has been read (`AMB-D-386`).
///
/// The verdict is the same one an [`Update`] carries (`AMB-D-438`): the digest moved, so this is a
/// different entry from the one the install was made from. What it is missing is the content — the
/// description to show, the schema to judge, the asset to fetch — all of which live a document away, so
/// this is what a listing can report and an apply cannot work from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The plugin's name — its identity in the catalog and its directory under `plugins/`.
    pub name: String,
    /// The manifest of what is installed on this machine right now.
    pub installed: Manifest,
    /// The catalog entry for that name, and which catalog it came from — the second half being what
    /// says where the detail document is and whose key its asset must verify against (`AMB-D-389`).
    pub found: DiscoveredEntry,
}

/// Whether the catalog lists a different detail document than the installed record was written from
/// (`AMB-D-386`) — the comparison the one list fetch can make, and the whole of what a candidate is.
///
/// Either side without a digest compares as unchanged: a plugin placed by hand records none, and a catalog
/// that publishes none has nothing to be compared against. Neither is evidence of a new build, and
/// treating an absent digest as a difference would report every such plugin as updatable forever.
pub fn moved(installed: &Manifest, entry: &ListEntry) -> bool {
    match (&installed.detail_sum, &entry.detail_sum) {
        (Some(installed), Some(listed)) => installed != listed,
        _ => false,
    }
}

/// The candidates in one catalog for one set of installed plugins — the pure half, so the rule is testable
/// without a network or a disk.
///
/// Name-sorted, because [`plugin_installed::installed`] is: a listing and a check see the same order. A
/// plugin **its own catalog** does not list is passed over ([`Discovery::find_from`]) — it may have been
/// installed by hand, or delisted since, or the name may now belong to another catalog entirely, and none
/// of the three is something this layer can offer to fix.
pub fn compare(installed: &[InstalledPlugin], view: &Discovery) -> Vec<Candidate> {
    installed
        .iter()
        .filter_map(|plugin| {
            let found = view.find_from(&plugin.name, plugin.origin.as_ref())?;
            moved(&plugin.manifest, &found.entry).then(|| Candidate {
                name: plugin.name.clone(),
                installed: plugin.manifest.clone(),
                found: found.clone(),
            })
        })
        .collect()
}

/// Read the one document a candidate's digest pointed at (`AMB-D-386`) — the second step of detection, and
/// the only one that costs a request.
///
/// It **judges nothing**: the digest already said this entry is not the one the install was made from, and
/// that is the whole of what an update is (`AMB-D-438`). What the fetch is for is the content — the detail
/// is where the config schema, the compatibility floor and the asset this machine would fetch all are,
/// which is everything a caller has to judge before applying and everything the apply path needs.
///
/// The detail goes through the install door's own validation ([`plugin_install::catalog_manifest`]), so an
/// update is judged against exactly the manifest an install would have been.
fn confirm(paths: &Paths, candidate: &Candidate) -> Result<Update> {
    let (available, available_i18n) = plugin_install::catalog_manifest(paths, &candidate.found)?;
    Ok(Update {
        name: candidate.name.clone(),
        installed: candidate.installed.clone(),
        available,
        available_i18n,
    })
}

/// How far a check goes for its catalog (`AMB-D-359`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The incidental read: a cache inside the freshness window answers with no request at all. What
    /// makes a check cheap enough to be offered alongside anything else.
    Incidental,
    /// Ask the network first, whatever the cache's age. What somebody who has just published wants, and
    /// the only way past a window that is otherwise invisible from the answer.
    Now,
}

/// What a check was measured against — the other half of its verdict (`AMB-D-359`).
///
/// It is three states and not two because "no catalog was read" happens two ways that mean opposite
/// things: there was nothing to compare one against, or there was and none could be had. Folding them
/// together is how an empty list comes to look like a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Against {
    /// Nothing is installed, so no catalog was read at all. An empty verdict, and a true one.
    NothingInstalled,
    /// A catalog answered, read this way ([`plugin_catalog::Freshness`]) — the weakest read among the
    /// shelves that answered, since the verdict is only as current as the stalest of them.
    Catalog(plugin_catalog::Freshness),
    /// A catalog was wanted and not had: nothing fetched anywhere, and nothing cached. The verdict below
    /// is empty because nothing was learned, which is not the same as nothing having moved.
    Unavailable,
}

/// A check's whole answer: what has moved, and what that was measured against.
#[derive(Debug)]
pub struct Checked {
    /// Every installed plugin the catalog lists a different entry for.
    pub updates: Vec<Update>,
    /// How current the answer is — see [`Against`].
    pub against: Against,
}

/// Every installed plugin the catalog lists a different entry for, with what the answer was measured
/// against carried out beside it (`AMB-D-359`).
///
/// The two travel together because they come off one read: asking the catalog a second time to find out
/// how fresh the first answer was would be answering about a different read. That pairing is the whole
/// point — the freshness window makes "nothing has changed" and "nothing had changed an hour ago" the same
/// rows, and a face that reports the rows without it cannot tell a reader which one they are looking at.
///
/// The short-circuit is here rather than in a caller: with nothing installed there is nothing to compare a
/// catalog to, so none is read and [`Against::NothingInstalled`] says why the answer names no catalog.
/// Otherwise the index is read once — no network at all inside the freshness window under
/// [`Reach::Incidental`], one fetch of the whole index otherwise — and each candidate that comes out of it
/// then costs one small document ([`confirm`]), which is the price of the detail riding a document away.
///
/// **A candidate whose document cannot be read is passed over.** A detail that will not fetch, will not
/// parse or is not the one the entry listed is not an update anyone could apply right now, and one such
/// plugin must not cost the report of every other (`AMB-D-352`'s posture). Naming a plugin — `plugin
/// update <name>` — is what asks for the reason rather than the omission.
///
/// No catalog at all is not a failure: the verdict comes back empty with [`Against::Unavailable`] beside
/// it, which is the honest shape — being offline costs freshness, not function (`AMB-D-347`), and a caller
/// that could not read one has to be able to say so rather than report nothing found. The only failure is
/// not being able to read what is installed.
pub fn check(paths: &Paths, reach: Reach) -> Result<Checked> {
    let installed = plugin_installed::installed(paths)?;
    if installed.is_empty() {
        return Ok(Checked { updates: Vec::new(), against: Against::NothingInstalled });
    }
    let view = match reach {
        Reach::Incidental => plugin_catalog::discover(paths),
        Reach::Now => plugin_catalog::discover_now(paths),
    };
    let against = match view.freshness() {
        Some(read) => Against::Catalog(read),
        None => Against::Unavailable,
    };
    let updates = compare(&installed, &view)
        .iter()
        .filter_map(|candidate| confirm(paths, candidate).ok())
        .collect();
    Ok(Checked { updates, against })
}

/// The update **candidates** a cached catalog already knows of — the surface a listing shows without
/// reaching for the network (`AMB-D-359`).
///
/// Where [`check`] may spend one fetch past the freshness window and one per candidate, this never
/// does: `plugin list` answers the same offline (`no network, no catalog fetch`), so it reads only what
/// the last catalog fetch left beside the installs and leaves the rest to the explicit `plugin update
/// --check`. No cache, an unreadable one, or nothing installed is simply nothing to report — never an
/// error, because a listing does not fail on a catalog it has not got.
///
/// Candidates, and said so in the type: the verdict is the one an update carries, and what is missing is
/// the content a document away (`AMB-D-438`). Which is why a mark here and an update check now say the
/// same thing about the same plugin — the mark that could not be acted on was the two faces reading two
/// different comparisons.
#[must_use]
pub fn available_cached(paths: &Paths) -> Vec<Candidate> {
    let Ok(installed) = plugin_installed::installed(paths) else {
        return Vec::new();
    };
    compare(&installed, &plugin_catalog::cached_view(paths))
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
    /// What went with the declaration that stopped naming it (`AMB-D-456`) — zero on the ordinary update,
    /// where the new schema names everything the old one did.
    pub purged: Purged,
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

/// The translations retained beside the previous build (`AMB-D-622`), for the reason the manifest is:
/// what a form is labelled in is part of the build it describes, and a layer left over from the build a
/// rollback undid would label the restored one in another version's words.
///
/// **Absent is a state, not a gap.** A build nobody translated has no such file, so a rollback to it
/// removes whatever the newer build left rather than putting something back
/// ([`plugin_installed::record_translations`] does both through the same call).
#[must_use]
pub fn backup_translations_path(paths: &Paths, name: &str) -> PathBuf {
    let mut p = plugin_installed::translations_path(paths, name).into_os_string();
    p.push(".bak");
    PathBuf::from(p)
}

/// Apply the update for one installed plugin: fetch the catalog's build, verify it, retain the current
/// one, and put the new one in place.
///
/// `Ok(None)` means there was nothing to apply — the catalog publishes the entry the install was made
/// from — and nothing was written. That is a result, not a failure: `plugin update <name>` on a current
/// plugin is a no-op a caller can report plainly. It is answered from the list alone (`AMB-D-438`), which
/// is why a current plugin costs no second fetch.
///
/// Refuses, leaving the install untouched, when the plugin is not installed (that is `plugin install`'s
/// door), when the catalog it came from does not list it (installed by hand, delisted since, or a record
/// too old to name a catalog — [`no_build_for`] says which), when the offered build cannot run on this
/// amenbo, when the caller's `approve` gate holds it
/// back (a new `required` setting an enabled plugin has no value for — `AMB-D-359`), or when its asset does
/// not verify.
///
/// `approve` is handed the store and the new manifest after this confirms it is a different build and
/// before anything is fetched or written; returning `Err` keeps the working build in place. A caller with
/// no gate to add passes `|_, _| Ok(())`.
///
/// The store is taken because an apply ends inside it: the values of keys the new schema no longer
/// declares are purged once the build is in place (`AMB-D-456`, see the module docs).
pub fn apply(
    store: &mut Store,
    name: &str,
    approve: impl FnOnce(&Store, &Manifest) -> Result<()>,
) -> Result<Option<Replaced>> {
    let paths = &store.paths.clone();
    let installed = plugin_installed::read(paths, name)?;
    // Unreachable on a platform amenbo ships for, and a refusal rather than "nothing to do" if it ever is
    // reached: a caller who named a plugin is owed the reason, not a silent no-op. Asked here rather than
    // left to the download, which refuses in the vocabulary of what the *plugin* supports — true, and not
    // the answer when the gap is that amenbo has no name for this machine at all.
    Platform::here().ok_or_else(|| {
        Error::Invalid(
            Msg::new(format!(
                "plugin manifests cannot name {}, so '{name}' cannot be updated here",
                std::env::consts::OS
            ))
            .coded(ErrorCode::InvalidPluginUpdatePlatform)
            .with("name", name)
            .with("os", std::env::consts::OS),
        )
    })?;
    let view = plugin_catalog::for_install(paths)?;
    let found = view
        .find_from(name, installed.origin.as_ref())
        .ok_or_else(|| no_build_for(&view, name, installed.origin.as_ref()))?;
    // The list's answer first: unmoved there, and nothing about this plugin's install has changed at all,
    // so the second document need not be fetched to establish it.
    if !moved(&installed.manifest, &found.entry) {
        return Ok(None);
    }
    let candidate = Candidate {
        name: name.to_string(),
        installed: installed.manifest,
        found: found.clone(),
    };
    let update = confirm(paths, &candidate)?;
    // The caller's gate on the new schema, before the download (see the module docs): a refusal keeps the
    // working build exactly as it was.
    approve(store, &update.available)?;
    let mut replaced =
        replace(paths, &update, &candidate.found.trust_root()?, &candidate.found.origin())?;
    replaced.purged = purge_dropped(store, &replaced.to);
    Ok(Some(replaced))
}

/// The store half of an apply (`AMB-D-456`): the stored values of keys the build now in place does not
/// declare, taken once it is in place.
///
/// **Best-effort, and deliberately not a failure of the update.** The bytes are already replaced by the
/// time this runs, so an error here would have a caller report "not updated — it is as it was" about an
/// update that was carried out, which is the one thing a receipt must not say. What a failure leaves is
/// rows nothing reads, and the next update over the same plugin purges them: the purge is keyed on what is
/// declared now, so it is the same work whenever it happens.
fn purge_dropped(store: &mut Store, to: &Manifest) -> Purged {
    plugin_config::purge_undeclared(store, &to.name, &to.config).unwrap_or_else(|e| {
        tracing::warn!(
            plugin = %to.name,
            error = %e,
            "plugin update: settings the new build no longer declares were left in place"
        );
        Purged::default()
    })
}

/// Why there is nothing to update to, said in terms of the shelf that was actually looked on.
///
/// "No catalog lists it" would be the wrong sentence for every one of these: the look-up was never across
/// every catalog. A catalog that has been unregistered, one that could not be answered, one that dropped
/// the entry, and an install too old to say where it came from are four situations, three of which the
/// user can do something about — and the sentence is what tells them which one they are in.
fn no_build_for(view: &Discovery, name: &str, origin: Option<&Origin>) -> Error {
    let url = match origin {
        Some(Origin::Catalog(url)) => url,
        Some(Origin::Official) => {
            return Error::NotFound(
                Msg::new(format!(
                    "the official catalog does not list a plugin named '{name}', so there is no build to update to"
                ))
                .coded(ErrorCode::NotFoundPluginBuildOfficial)
                .with("name", name),
            )
        }
        None => {
            return Error::NotFound(
                Msg::new(format!(
                    "'{name}' does not record which catalog it came from, so it is looked for in the official catalog, which does not list it. Uninstall and install it again to record where it comes from."
                ))
                .coded(ErrorCode::NotFoundPluginBuildOriginUnknown)
                .with("name", name),
            )
        }
    };
    match view.sources.iter().find(|s| &s.url == url) {
        None => Error::NotFound(
            Msg::new(format!(
                "'{name}' was installed from {url}, which is no longer a registered catalog — register it again to update from it (amenbo updates a plugin only from the catalog it came from)"
            ))
            .coded(ErrorCode::NotFoundPluginBuildSourceGone)
            .with("name", name)
            .with("url", url),
        ),
        Some(source) if !source.reachable => Error::NotFound(
            Msg::new(format!(
                "'{name}' was installed from {url}, which did not answer and has nothing cached — there is nothing to compare its build against"
            ))
            .coded(ErrorCode::NotFoundPluginBuildSourceSilent)
            .with("name", name)
            .with("url", url),
        ),
        Some(_) => Error::NotFound(
            Msg::new(format!(
                "'{name}' was installed from {url}, which no longer lists it — there is no build to update to (amenbo updates a plugin only from the catalog it came from)"
            ))
            .coded(ErrorCode::NotFoundPluginBuildDelisted)
            .with("name", name)
            .with("url", url),
        ),
    }
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
/// still applied. A caller with no gate passes `|_, _| Ok(())`.
pub fn apply_all(
    store: &mut Store,
    approve: impl Fn(&Store, &Manifest) -> Result<()>,
) -> Result<Vec<Outcome>> {
    let paths = &store.paths.clone();
    let installed = plugin_installed::installed(paths)?;
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    // The list the way an install reads it, not the way a check does: applying is the explicit act, so it
    // asks the network the way `plugin_catalog::load` does and falls back to the cache only when there is no
    // answer. Replacing a binary on what a cache said an hour ago would be acting on stale evidence.
    let view = plugin_catalog::for_install(paths)?;
    let mut outcomes = Vec::new();
    for candidate in compare(&installed, &view) {
        match confirm(paths, &candidate) {
            Ok(update) => {
                let done = candidate.found.trust_root().and_then(|root| {
                    approve(store, &update.available)
                        .and_then(|()| replace(paths, &update, &root, &candidate.found.origin()))
                });
                outcomes.push(match done {
                    Ok(mut replaced) => {
                        replaced.purged = purge_dropped(store, &replaced.to);
                        Outcome::Replaced(Box::new(replaced))
                    }
                    Err(error) => Outcome::Failed { name: update.name, error },
                });
            }
            // Unlike a check, this run was asked to act, so a detail that could not be read is reported
            // rather than passed over — and the plugins beside it are still applied.
            Err(error) => outcomes.push(Outcome::Failed { name: candidate.name, error }),
        }
    }
    Ok(outcomes)
}

/// Put one resolved update in place — the whole write path, and the only one.
///
/// The order is the safety story (see the module docs): the two gates that can refuse run before the
/// network, and the network runs before anything on disk is touched, so a plugin that fails any of them
/// is left exactly as it was.
fn replace(
    paths: &Paths,
    update: &Update,
    root: &crate::plugin_provenance::TrustRoot,
    origin: &Origin,
) -> Result<Replaced> {
    // A build this amenbo cannot speak to is not an improvement. Refusing here keeps a working plugin
    // working — the alternative is replacing it with one that will be dropped at dispatch.
    if let Err(why) = plugin_compat::check(&update.available) {
        return Err(why.into_update_error(&update.name));
    }
    // Off the network and through the trust gates (`AMB-D-351`) — the same door the first install used.
    let program = plugin_install::fetch_verified_program(&update.available, root)?;
    retain_and_place(paths, update, &program, origin)
}

/// The disk half of [`replace`], with the verified bytes handed in — the seam a test drives, since
/// everything above it needs a network.
///
/// Retain first, overwrite second: the previous executable **and** the manifest describing it are copied
/// aside before [`plugin_install::place`] writes over either, so the pair a rollback needs is complete
/// from the first byte of the replacement onward.
///
/// The origin is re-written rather than left alone, and it is the same one the resolution used, so for a
/// plugin that had a record this changes nothing. What it does is finish the record for an install made
/// before there was one: such a plugin resolved on the official shelf to get here, so that is what it
/// came from, and after one update it says so.
fn retain_and_place(
    paths: &Paths,
    update: &Update,
    program: &[u8],
    origin: &Origin,
) -> Result<Replaced> {
    let name = &update.name;
    std::fs::copy(plugin_installed::program_path(paths, name), backup_path(paths, name))?;
    std::fs::copy(plugin_installed::manifest_path(paths, name), backup_manifest_path(paths, name))?;
    retain_translations(paths, name)?;

    plugin_installed::record_origin(paths, name, origin)?;
    plugin_installed::record_translations(paths, name, &update.available_i18n)?;
    let placed = plugin_install::place(paths, &update.available, program)?;
    Ok(Replaced {
        name: name.clone(),
        from: update.installed.clone(),
        to: placed.manifest,
        program: placed.program,
        backup: backup_path(paths, name),
        program_bytes: placed.program_bytes,
        // The disk half knows nothing of the store: what the new declaration dropped is filled in by
        // purge_dropped, which runs after this has put the manifest in place.
        purged: Purged::default(),
    })
}

/// Retain the translations of the build being replaced, in the state they are in — the file copied
/// aside when there is one, and any stale `.bak` cleared when there is not.
///
/// The clearing is the whole of why this is not a plain [`std::fs::copy`]: an install with no
/// translations would otherwise leave the *previous* update's `.bak` standing, and a rollback would put
/// back a layer two builds old as though it were the retained one.
fn retain_translations(paths: &Paths, name: &str) -> Result<()> {
    let from = plugin_installed::translations_path(paths, name);
    let backup = backup_translations_path(paths, name);
    if !from.exists() {
        match std::fs::remove_file(&backup) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(Error::from(e)),
        }
    }
    std::fs::copy(from, backup)?;
    Ok(())
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
/// `.bak` there is. The enable gate, the settings and the secrets are keyed elsewhere and left untouched:
/// what the restored manifest declares is answered by whatever is stored for it, and a value the update
/// purged for want of a declaration does not come back (`AMB-D-456`) — a rollback restores the pair, and
/// the pair is the build and its manifest.
pub fn rollback(paths: &Paths, name: &str) -> Result<RolledBack> {
    // Anchor on a real install: a rollback restores an executable *beside* a live plugin, so a name that
    // is not installed is `not_found`, not a bare "no backup".
    plugin_installed::read(paths, name)?;

    let backup_program = backup_path(paths, name);
    let backup_manifest = backup_manifest_path(paths, name);
    if !backup_program.exists() {
        return Err(Error::NotFound(
            Msg::new(format!(
                "plugin '{name}' has no retained build to roll back to — it was not updated, or a rollback already used it"
            ))
            .coded(ErrorCode::NotFoundPluginRollbackBuild)
            .with("name", name),
        ));
    }

    // Read both retained files before writing either: a rollback that put the old binary back under the
    // new manifest would describe the old build with the wrong checksum, config schema and floor.
    let program = std::fs::read(&backup_program)?;
    let raw = std::fs::read_to_string(&backup_manifest).map_err(|e| {
        Error::Invalid(
            Msg::new(format!(
                "plugin '{name}' has a retained binary but no manifest beside it ({e}) — the pair a rollback needs is incomplete"
            ))
            .coded(ErrorCode::InvalidPluginRollbackManifestAbsent)
            .with("name", name)
            .with("reason", e),
        )
    })?;
    let manifest: Manifest = serde_json::from_str(&raw).map_err(|e| {
        Error::Invalid(
            Msg::new(format!("plugin '{name}' has a retained manifest that will not parse: {e}"))
                .coded(ErrorCode::InvalidPluginRollbackManifestUnparsable)
                .with("name", name)
                .with("reason", e),
        )
    })?;

    // What the retained build was translated as, back with it — and removed when it had none, which is
    // what `record_translations` does with an empty map. Before the marker, as an install writes it.
    let retained_i18n = std::fs::read_to_string(backup_translations_path(paths, name))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    plugin_installed::record_translations(paths, name, &retained_i18n)?;

    let placed = plugin_install::place(paths, &manifest, &program)?;
    // The retained set is now the running build — there is nothing further back to go. Clear it so a
    // stale `.bak` cannot later masquerade as a fresh one (same close-out as self-update, `AMB-D-341`).
    let _ = std::fs::remove_file(&backup_program);
    let _ = std::fs::remove_file(&backup_manifest);
    let _ = std::fs::remove_file(backup_translations_path(paths, name));

    Ok(RolledBack { name: name.to_string(), restored: placed.manifest, program: placed.program })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_layer::Layer;
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
        published(Manifest {
            name: name.to_string(),
            desc: "a test plugin".to_string(),
            about: None,
            author: "amenbo".to_string(),
            repo: "ShiroDoromoto/amenbo".to_string(),
            os: vec![Os::Macos, Os::Linux],
            category: "workflow".to_string(),
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: format!("sha256:{checksum}"),
            signature: None,
            assets: Default::default(),
            official: false,
            detail_sum: None,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: Vec::new(),
            events: Vec::new(),
            agent: None,
            settings: None,
        })
    }

    /// A manifest as the catalog publishes it: the digest of its **own** detail document filled in, the
    /// way the catalog CI fills it (`AMB-D-386`). Computing it rather than writing one by hand is what
    /// keeps these tests honest — a fixture whose asset moves gets a digest that moves with it, exactly as
    /// a real publication would, and one whose description moves does not.
    fn published(mut manifest: Manifest) -> Manifest {
        use sha2::{Digest, Sha256};
        let (_, _, detail) = crate::plugin_wire::split(&manifest, &Default::default());
        let bytes = serde_json::to_vec(&detail).expect("a detail serializes");
        let hex: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        manifest.detail_sum = Some(format!("sha256:{hex}"));
        manifest
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
            origin: Some(Origin::Official),
        }
    }

    /// The list half of these manifests, as the merged view holds it (`AMB-D-385`/`AMB-D-389`) — each
    /// entry carrying the digest of the detail document it was published with, and the official catalog
    /// standing as the one that served them.
    fn catalog(entries: Vec<Manifest>) -> Discovery {
        Discovery::served_by(
            plugin_catalog::OFFICIAL_CATALOG_URL,
            true,
            None,
            crate::plugin_catalog::Catalog {
                generated_at: None,
                entries: entries.iter().map(listed).collect(),
                dropped: Vec::new(),
            },
        )
    }

    /// One manifest's list entry, digest included — what the catalog serves for it.
    fn listed(manifest: &Manifest) -> ListEntry {
        let (mut entry, _, _) = crate::plugin_wire::split(manifest, &Default::default());
        entry.detail_sum = manifest.detail_sum.clone();
        entry
    }

    /// A detail document that moved is something about how the plugin installs having moved, and that is
    /// the whole of what the list can say.
    #[test]
    fn a_changed_detail_digest_is_a_candidate() {
        let installed = vec![installed_plugin("worktree", "aa")];
        let candidates = compare(&installed, &catalog(vec![manifest("worktree", "bb")]));

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "worktree");
        assert_eq!(candidates[0].installed.checksum, "sha256:aa");
        assert_eq!(candidates[0].found.entry.detail_sum, manifest("worktree", "bb").detail_sum);
    }

    /// The same detail document is the same install — everything a list draws may have been re-written by
    /// the catalog without a byte of the plugin changing.
    #[test]
    fn a_rewritten_entry_over_the_same_detail_is_not_a_candidate() {
        let installed = vec![installed_plugin("worktree", "aa")];
        let mut entry = manifest("worktree", "aa");
        entry.desc = "a much better description".to_string();
        entry.official = true;

        assert_eq!(entry.detail_sum, published(entry.clone()).detail_sum, "the detail did not move");
        assert!(compare(&installed, &catalog(vec![entry])).is_empty());
    }

    /// Neither side's digest is evidence on its own: a plugin placed by hand records none, and reporting
    /// it as updatable forever would be the alternative.
    #[test]
    fn a_plugin_with_no_recorded_digest_is_never_a_candidate() {
        let mut by_hand = manifest("worktree", "aa");
        by_hand.detail_sum = None;
        let installed = vec![InstalledPlugin {
            name: "worktree".to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: by_hand,
            origin: Some(Origin::Official),
        }];

        assert!(compare(&installed, &catalog(vec![manifest("worktree", "bb")])).is_empty());
    }

    /// A plugin the catalog does not list has no build to be moved past — installed by hand, or delisted.
    #[test]
    fn a_plugin_the_catalog_does_not_list_is_passed_over() {
        let installed = vec![installed_plugin("homemade", "aa")];
        assert!(compare(&installed, &catalog(vec![manifest("worktree", "bb")])).is_empty());
    }

    /// The same manifests as one catalog would serve them, but from a registered catalog rather than the
    /// official one.
    fn third_party_catalog(url: &str, entries: Vec<Manifest>) -> Discovery {
        Discovery::served_by(
            url,
            false,
            Some("RWSgV8uCt8tyYg74JbwBblWoE+g7bxSGvK8blkKW7gUo3EuBXaqy5oMR"),
            crate::plugin_catalog::Catalog {
                generated_at: None,
                entries: entries.iter().map(listed).collect(),
                dropped: Vec::new(),
            },
        )
    }

    /// The whole point of recording where an install came from: a catalog that starts publishing a name
    /// already installed from somewhere else offers that install nothing. Without this the update would
    /// fetch from the new publisher — and verify, against the new publisher's key.
    #[test]
    fn another_catalog_publishing_the_same_name_offers_no_update() {
        let src = "https://example.invalid/third/catalog.json";
        let mut installed = installed_plugin("worktree", "aa");
        installed.origin = Some(Origin::Catalog(src.to_string()));

        // The official catalog now carries the name too, with a different build behind it.
        assert!(
            compare(std::slice::from_ref(&installed), &catalog(vec![manifest("worktree", "bb")]))
                .is_empty(),
            "the official catalog is not this plugin's publisher"
        );

        // Its own catalog is, and there the newer build is an update.
        let candidates =
            compare(&[installed], &third_party_catalog(src, vec![manifest("worktree", "bb")]));
        assert_eq!(candidates.len(), 1, "the catalog it came from still offers it");
        assert_eq!(candidates[0].found.source, src);
    }

    /// An install too old to record its origin is looked for on the official shelf, and nowhere else.
    #[test]
    fn an_install_with_no_recorded_origin_is_only_offered_the_official_build() {
        let mut installed = installed_plugin("worktree", "aa");
        installed.origin = None;

        assert_eq!(
            compare(std::slice::from_ref(&installed), &catalog(vec![manifest("worktree", "bb")]))
                .len(),
            1,
            "the official catalog answers for it"
        );
        assert!(
            compare(
                &[installed],
                &third_party_catalog(
                    "https://example.invalid/third/catalog.json",
                    vec![manifest("worktree", "bb")]
                )
            )
            .is_empty(),
            "and a registered catalog that now carries the name does not"
        );
    }

    /// A view in which `src` is registered, and answered or did not.
    fn view_with_source(src: &str, reachable: bool) -> Discovery {
        Discovery {
            entries: Vec::new(),
            shadowed: Vec::new(),
            sources: vec![crate::plugin_catalog::DiscoveredSource {
                url: src.to_string(),
                name: "an internal catalog".to_string(),
                fingerprint: None,
                official: false,
                reachable,
                offered: 0,
                freshness: reachable.then_some(crate::plugin_catalog::Freshness::Fetched),
            }],
            dropped: Vec::new(),
        }
    }

    /// The ways there is nothing to update to read differently, because they are different situations —
    /// unregistered, unreachable, delisted, and an install with no record — and the sentence is what tells
    /// the user which one they are in.
    #[test]
    fn the_refusal_names_the_shelf_that_was_looked_on() {
        let src = "https://example.invalid/third/catalog.json";
        let from = Some(Origin::Catalog(src.to_string()));
        let empty = || Discovery {
            entries: Vec::new(),
            shadowed: Vec::new(),
            sources: Vec::new(),
            dropped: Vec::new(),
        };

        let gone = no_build_for(&empty(), "worktree", from.as_ref());
        assert_eq!(gone.code(), "not_found_plugin_build_source_gone");
        assert!(format!("{gone:?}").contains("register it again"), "unregistered: {gone:?}");

        let silent = no_build_for(&view_with_source(src, false), "worktree", from.as_ref());
        assert!(format!("{silent:?}").contains("did not answer"), "unreachable: {silent:?}");

        let delisted = no_build_for(&view_with_source(src, true), "worktree", from.as_ref());
        assert!(format!("{delisted:?}").contains("no longer lists it"), "delisted: {delisted:?}");
        assert!(format!("{delisted:?}").contains(src), "and the catalog is named: {delisted:?}");

        let official = no_build_for(&empty(), "worktree", Some(&Origin::Official));
        assert!(
            format!("{official:?}").contains("official catalog"),
            "the official shelf is named: {official:?}"
        );

        let unrecorded = no_build_for(&empty(), "worktree", None);
        assert!(
            format!("{unrecorded:?}").contains("install it again"),
            "and an install with no record is told how to get one: {unrecorded:?}"
        );
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

        let names: Vec<_> = compare(&installed, &catalog).into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["alpha", "gamma"], "beta is current, and the rest stay sorted");
    }

    /// **An update that changes no binary is an update** (`AMB-D-438`) — the whole reason the comparison
    /// is the detail's digest rather than an asset's. Every one of these leaves the executable byte for
    /// byte what it was, and every one of them is something the user is owed.
    #[test]
    fn an_entry_whose_manifest_moved_but_whose_bytes_did_not_is_an_update() {
        let installed_manifest = published(manifest("worktree", "same-bytes"));
        let installed = vec![InstalledPlugin {
            name: "worktree".to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: installed_manifest.clone(),
            origin: Some(Origin::Official),
        }];

        let cases: [fn(&mut Manifest); 3] = [
            // A setting the author added.
            |m| {
                m.config.push(crate::plugin_manifest::ConfigField::new("base", "Base branch"))
            },
            // A subscription they changed.
            |m| m.events.push(crate::plugin_manifest::EventSubscription::new("task.done")),
            // The words the plugin says for itself at the AI's entry point (`AMB-D-437`).
            |m| {
                m.agent = Some(crate::plugin_manifest::AgentGuide {
                    when: "Starting work on a task that will produce commits".into(),
                    commands: Vec::new(),
                })
            },
        ];
        for moved in cases {
            let mut entry = installed_manifest.clone();
            moved(&mut entry);
            let entry = published(entry);
            assert_eq!(
                entry.asset_for(plat(Os::Macos)).map(|a| a.checksum),
                installed_manifest.asset_for(plat(Os::Macos)).map(|a| a.checksum),
                "the bytes are the ones already installed",
            );
            assert_eq!(
                compare(&installed, &catalog(vec![entry])).len(),
                1,
                "and the entry is still a different one from the one this was installed from",
            );
        }
    }

    /// A rebuild of another platform is an update here too (`AMB-D-438`): one digest covers the whole
    /// document, and the two faces read that one answer. What the second document is fetched for is the
    /// content, not a second verdict.
    #[test]
    fn a_rebuild_of_another_platform_is_still_a_different_entry() {
        let mut installed_manifest = manifest("worktree", "unused");
        installed_manifest.url = String::new();
        installed_manifest.checksum = String::new();
        installed_manifest.assets = [
            (plat(Os::Macos), asset("mac-1")),
            (plat_arch(Os::Linux, Arch::X64), asset("linux-x64-1")),
        ]
        .into_iter()
        .collect();
        let installed_manifest = published(installed_manifest);
        let installed = vec![InstalledPlugin {
            name: "worktree".to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: installed_manifest.clone(),
            origin: Some(Origin::Official),
        }];

        // Only the linux build was cut again.
        let mut entry = installed_manifest.clone();
        entry.assets.insert(plat_arch(Os::Linux, Arch::X64), asset("linux-x64-2"));
        let entry = published(entry);

        assert_eq!(
            compare(&installed, &catalog(vec![entry])).len(),
            1,
            "the document moved, so this is a different entry — on every machine, not only the linux one"
        );
    }

    /// The same entry re-published is not an update, whatever else the catalog did around it — which is
    /// what a deterministic signature buys (`AMB-D-438`): re-signing identical bytes leaves the digest
    /// where it was, so nothing is offered.
    #[test]
    fn an_entry_republished_unchanged_is_not_an_update() {
        let installed_manifest = published(manifest("worktree", "aa"));
        let installed = vec![InstalledPlugin {
            name: "worktree".to_string(),
            program: std::path::PathBuf::from("/dev/null"),
            manifest: installed_manifest.clone(),
            origin: Some(Origin::Official),
        }];

        assert!(
            compare(&installed, &catalog(vec![published(manifest("worktree", "aa"))])).is_empty(),
            "the same document publishes as the same digest"
        );
    }

    /// Nothing installed is answered without a catalog at all — including when there is no cache to read
    /// and no network to reach, which is the state a fresh machine is in.
    #[test]
    fn nothing_installed_is_answered_without_a_catalog() {
        let dir = amenbo_scratch::scratch("plugin-update-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths::at(dir);

        assert!(check(&paths, Reach::Incidental).unwrap().updates.is_empty());
    }

    // ---- applying one ----

    fn paths_at(tag: &str) -> Paths {
        let dir = amenbo_scratch::scratch(&format!("plugin-update-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Paths::at(dir)
    }

    /// A store on a scratch base — what the apply path takes, since it ends by purging the settings the
    /// new declaration dropped (`AMB-D-456`).
    fn store_at(tag: &str) -> Store {
        Store::open_at(paths_at(tag)).unwrap()
    }

    fn field(key: &str, secret: bool) -> crate::plugin_manifest::ConfigField {
        crate::plugin_manifest::ConfigField {
            secret,
            ..crate::plugin_manifest::ConfigField::new(key, key)
        }
    }

    fn mk_project(store: &mut Store, name: &str) -> i64 {
        store
            .project_add(crate::ops::project::NewProject {
                name: name.into(),
                view: crate::model::View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id
    }

    /// A plugin installed on disk with a schema, and a value stored for every field of it — the state an
    /// update's purge acts on.
    fn install_and_configure(store: &mut Store, manifest: &Manifest, project: i64) {
        install_on_disk(&store.paths.clone(), manifest, b"old");
        for f in &manifest.config {
            let value = if f.secret { "s3cret" } else { "main" };
            plugin_config::set(store, f, &manifest.name, Layer::Project(project), value).unwrap();
        }
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
        update_of_translated(installed, available, Translations::new())
    }

    /// The same, for a build the catalog publishes translations for — the layer that travels with it.
    fn update_of_translated(
        installed: Manifest,
        available: Manifest,
        available_i18n: Translations,
    ) -> Update {
        Update { name: installed.name.clone(), installed, available, available_i18n }
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
            retain_and_place(
                &paths,
                &update_of(before.clone(), after.clone()),
                b"#!/bin/sh\nnew\n",
                &Origin::Official,
            )
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
        assert!(compare(&[read], &catalog(vec![after])).is_empty(), "and it is current");
    }

    /// An update writes the origin down, which is how an install made before there was a record gets one:
    /// it resolved on the official shelf to be updated at all, so that is what it says afterwards.
    #[test]
    fn applying_records_the_shelf_the_build_came_from() {
        let paths = paths_at("apply-origin");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        assert_eq!(plugin_installed::origin(&paths, "worktree"), None, "nothing recorded yet");

        let src = "https://example.invalid/third/catalog.json";
        retain_and_place(
            &paths,
            &update_of(before, manifest("worktree", "bb")),
            b"new",
            &Origin::Catalog(src.to_string()),
        )
        .unwrap();

        assert_eq!(
            plugin_installed::read(&paths, "worktree").unwrap().origin,
            Some(Origin::Catalog(src.to_string()))
        );
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

        retain_and_place(&paths, &update_of(first, second.clone()), b"two", &Origin::Official).unwrap();
        retain_and_place(&paths, &update_of(second.clone(), third), b"three", &Origin::Official)
            .unwrap();

        assert_eq!(std::fs::read(backup_path(&paths, "worktree")).unwrap(), b"two");
        let retained: Manifest = serde_json::from_slice(
            &std::fs::read(backup_manifest_path(&paths, "worktree")).unwrap(),
        )
        .unwrap();
        assert_eq!(retained, second, "the retained manifest moved with the retained binary");
    }

    /// What an update leaves alone on disk: nothing in the plugin's home beside the two files it
    /// replaces, so whatever the plugin keeps for itself survives the round trip.
    #[test]
    fn applying_touches_nothing_but_the_binary_and_its_manifest() {
        let paths = paths_at("preserve");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        let stray = paths.plugin_dir("worktree").join("state.json");
        std::fs::write(&stray, b"whatever the plugin keeps").unwrap();

        retain_and_place(&paths, &update_of(before, manifest("worktree", "bb")), b"new", &Origin::Official)
            .unwrap();

        assert_eq!(std::fs::read(&stray).unwrap(), b"whatever the plugin keeps");
    }

    /// The store half of an apply (`AMB-D-456`): once the new build is in place, the value of a key it
    /// stopped declaring is gone, the values of the keys it still declares are untouched, and the receipt
    /// says which road each one went down.
    #[test]
    fn applying_takes_the_value_of_a_key_the_new_build_stopped_declaring() {
        let mut store = store_at("purge");
        let project = mk_project(&mut store, "proj");
        let mut before = manifest("worktree", "aa");
        before.config = vec![field("base", false), field("token", true)];
        install_and_configure(&mut store, &before, project);

        let mut after = manifest("worktree", "bb");
        after.config = vec![field("base", false)];
        let paths = store.paths.clone();
        let mut replaced =
            retain_and_place(&paths, &update_of(before, after), b"new", &Origin::Official).unwrap();
        replaced.purged = purge_dropped(&mut store, &replaced.to);

        assert_eq!(replaced.purged, Purged { settings: 0, secrets: 1 });
        assert_eq!(
            plugin_config::get(&store, &field("base", false), "worktree", Layer::Project(project))
                .unwrap()
                .as_deref(),
            Some("main"),
            "what the new build still declares keeps its value",
        );
        assert_eq!(
            plugin_config::get(&store, &field("token", true), "worktree", Layer::Project(project)).unwrap(),
            None,
        );
    }

    /// An update that declares everything the old one did leaves the store alone — the ordinary case, and
    /// the one a user must be able to trust.
    #[test]
    fn applying_a_build_that_declares_the_same_keys_takes_nothing() {
        let mut store = store_at("purge-none");
        let project = mk_project(&mut store, "proj");
        let mut before = manifest("worktree", "aa");
        before.config = vec![field("base", false), field("token", true)];
        install_and_configure(&mut store, &before, project);

        let mut after = manifest("worktree", "bb");
        after.config = before.config.clone();
        let paths = store.paths.clone();
        let replaced =
            retain_and_place(&paths, &update_of(before, after), b"new", &Origin::Official).unwrap();
        assert_eq!(purge_dropped(&mut store, &replaced.to), Purged::default());
        assert_eq!(
            plugin_config::get(&store, &field("token", true), "worktree", Layer::Project(project))
                .unwrap()
                .as_deref(),
            Some("s3cret"),
        );
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

        let err = replace(
            &paths,
            &update_of(before.clone(), after),
            &crate::plugin_provenance::TrustRoot::official(),
            &Origin::Official,
        )
            .unwrap_err();
        assert_eq!(err.code(), "invalid_plugin_update_incompatible");
        assert!(format!("{err:?}").contains("99.0.0"), "the floor is named: {err:?}");

        assert_eq!(std::fs::read(plugin_installed::program_path(&paths, "worktree")).unwrap(), b"old");
        assert_eq!(plugin_installed::read(&paths, "worktree").unwrap().manifest, before);
        assert!(!backup_path(&paths, "worktree").exists(), "and nothing was retained");
    }

    /// Updating what is not installed is `plugin install`'s door, not this one — and it is answered
    /// before a catalog is ever fetched.
    #[test]
    fn updating_something_not_installed_is_not_found() {
        let mut store = store_at("absent");
        assert_eq!(
            apply(&mut store, "worktree", |_, _| Ok(())).unwrap_err().code(),
            "not_found_plugin_installed"
        );
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
        retain_and_place(&paths, &update_of(before.clone(), after), b"#!/bin/sh\nnew\n", &Origin::Official)
            .unwrap();

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
        retain_and_place(&paths, &update_of(before, manifest("worktree", "bb")), b"new", &Origin::Official)
            .unwrap();

        rollback(&paths, "worktree").unwrap();
        assert!(!backup_path(&paths, "worktree").exists(), "the retained binary is gone");
        assert!(!backup_manifest_path(&paths, "worktree").exists(), "and its manifest with it");
        assert_eq!(rollback(&paths, "worktree").unwrap_err().code(), "not_found_plugin_rollback_build");
    }

    /// One language's overlay, as the catalog published it.
    fn translated(desc: &str) -> Translations {
        Translations::from([(
            "ja".to_string(),
            crate::plugin_manifest::ManifestOverlay {
                desc: Some(desc.to_string()),
                ..Default::default()
            },
        )])
    }

    /// **An update replaces the pair the catalog published together** (`AMB-D-622`): the build, and what
    /// it says in other languages. A layer left over from the build before would label the new one in the
    /// old one's words — and, being a layer, would never look wrong on screen.
    #[test]
    fn applying_replaces_the_translations_with_the_build() {
        let paths = paths_at("apply-i18n");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        plugin_installed::record_translations(&paths, "worktree", &translated("古い一行")).unwrap();

        retain_and_place(
            &paths,
            &update_of_translated(before, manifest("worktree", "bb"), translated("新しい一行")),
            b"new",
            &Origin::Official,
        )
        .unwrap();

        assert_eq!(plugin_installed::translations(&paths, "worktree"), translated("新しい一行"));
        assert_eq!(
            std::fs::read_to_string(backup_translations_path(&paths, "worktree"))
                .map(|raw| serde_json::from_str::<Translations>(&raw).unwrap())
                .unwrap(),
            translated("古い一行"),
            "and the layer that went with the retained build is retained with it",
        );
    }

    /// **A rollback restores the whole trio**, so the layer never outlives the build it describes — and
    /// a build nobody translated comes back untranslated rather than wearing its successor's words.
    #[test]
    fn rolling_back_restores_the_translations_the_update_replaced() {
        let paths = paths_at("rollback-i18n");
        let untranslated = manifest("worktree", "aa");
        install_on_disk(&paths, &untranslated, b"old");
        retain_and_place(
            &paths,
            &update_of_translated(
                untranslated,
                manifest("worktree", "bb"),
                translated("新しい一行"),
            ),
            b"new",
            &Origin::Official,
        )
        .unwrap();
        assert_eq!(plugin_installed::translations(&paths, "worktree"), translated("新しい一行"));

        rollback(&paths, "worktree").unwrap();

        assert!(
            plugin_installed::translations(&paths, "worktree").is_empty(),
            "the build that came back was never translated, so neither is what is on screen",
        );
        assert!(!backup_translations_path(&paths, "worktree").exists(), "and the retained set is gone");
    }

    /// A plugin that was never updated has no retained build: the rollback says so and changes nothing.
    #[test]
    fn rolling_back_a_plugin_that_was_never_updated_is_not_found() {
        let paths = paths_at("rollback-fresh");
        install_on_disk(&paths, &manifest("worktree", "aa"), b"only");

        assert_eq!(rollback(&paths, "worktree").unwrap_err().code(), "not_found_plugin_rollback_build");
        assert_eq!(std::fs::read(plugin_installed::program_path(&paths, "worktree")).unwrap(), b"only");
    }

    /// Rolling back what is not installed anchors on the missing install, not on a missing backup.
    #[test]
    fn rolling_back_something_not_installed_is_not_found() {
        let paths = paths_at("rollback-absent");
        assert_eq!(rollback(&paths, "worktree").unwrap_err().code(), "not_found_plugin_installed");
    }

    /// A rollback restores the pair, not the values: a setting the update purged for want of a
    /// declaration stays gone even though the restored manifest declares it again (`AMB-D-456` — what is
    /// retained is one generation of the build and its manifest, and nothing else).
    #[test]
    fn rolling_back_does_not_bring_a_purged_value_back() {
        let mut store = store_at("rollback-purged");
        let project = mk_project(&mut store, "proj");
        let mut before = manifest("worktree", "aa");
        before.config = vec![field("base", false)];
        install_and_configure(&mut store, &before, project);

        let paths = store.paths.clone();
        let replaced = retain_and_place(
            &paths,
            &update_of(before.clone(), manifest("worktree", "bb")),
            b"new",
            &Origin::Official,
        )
        .unwrap();
        assert_eq!(purge_dropped(&mut store, &replaced.to).settings, 1);

        rollback(&paths, "worktree").unwrap();
        assert_eq!(
            plugin_installed::read(&paths, "worktree").unwrap().manifest,
            before,
            "the build that declared it is running again",
        );
        assert_eq!(
            plugin_config::get(&store, &field("base", false), "worktree", Layer::Project(project)).unwrap(),
            None,
            "and the value it asked for does not come back with it",
        );
    }

    /// The gate, the settings and the secrets are keyed elsewhere: a rollback restores the two files it
    /// retained and leaves the rest of the home alone, exactly as an update does.
    #[test]
    fn rolling_back_touches_nothing_but_the_binary_and_its_manifest() {
        let paths = paths_at("rollback-preserve");
        let before = manifest("worktree", "aa");
        install_on_disk(&paths, &before, b"old");
        retain_and_place(&paths, &update_of(before, manifest("worktree", "bb")), b"new", &Origin::Official)
            .unwrap();
        let stray = paths.plugin_dir("worktree").join("state.json");
        std::fs::write(&stray, b"kept across the round trip").unwrap();

        rollback(&paths, "worktree").unwrap();
        assert_eq!(std::fs::read(&stray).unwrap(), b"kept across the round trip");
    }
}
