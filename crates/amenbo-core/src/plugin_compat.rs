//! The **compatibility gate** — does this amenbo speak what the plugin was written against
//! (`AMB-D-359`)?
//!
//! A manifest declares two compatibility facts ([`crate::plugin_manifest`] carries them, opaque):
//! [`payload_v`](Manifest::payload_v), the event-payload contract the plugin reads (`AMB-D-349`), and
//! [`min_amenbo`](Manifest::min_amenbo), the amenbo version it needs underneath it. This module is the
//! consuming half: it compares both against the running build and answers with an
//! [`Incompatibility`], so a plugin that cannot understand what it would be handed is stopped instead
//! of being fed a payload it will misread.
//!
//! **Two callers, two postures** — the same check, refused at the door and skipped at run time:
//!
//! - **`plugin enable`** refuses. Enabling is an explicit act on one named plugin, so the answer is an
//!   error naming the mismatch, and the gate stays closed (the fail-closed posture
//!   [`plugin_trust`](crate::plugin_trust) already takes on unsatisfied `required` settings).
//! - **the subscription resolver** ([`EnabledSubscribers`](crate::plugin_subscribe::EnabledSubscribers))
//!   warns and drops that one subscriber. Delivery is best-effort (`AMB-D-352`): an event still fires
//!   for every compatible plugin, because one plugin left behind by a payload bump must not silence the
//!   others. A plugin can also become incompatible *after* it was enabled — amenbo updates underneath an
//!   install — so the run-time side cannot lean on the enable-time check having run.
//!
//! **The payload contract is an equality, not a floor.** `v` moves only on a breaking change
//! (`AMB-D-349` — additive fields never bump it), so any difference in either direction is a contract
//! the two sides do not share: an amenbo whose `v` has outgrown the plugin would feed it a payload whose
//! meaning moved, and a plugin declaring a `v` above ours reads a contract this build cannot produce.
//!
//! **A floor amenbo cannot read is not a floor it can claim to meet.** The intake door refuses a
//! `min_amenbo` that does not read as a version ([`plugin_validate`](crate::plugin_validate), by the
//! same parser this module compares with), so one should not get this far. It is still checked here:
//! a plugin's manifest is replaced by an update long after it was installed, and this gate is what runs
//! at enable and at run time. Version comparison is loose but not limitless (`major.minor.patch`,
//! pre-release metadata ignored); when the floor does not parse at all, this gate reports it rather than
//! waving the plugin through on the strength of a string nobody could compare.

use std::fmt;

use crate::error::{Error, ErrorCode, Msg};
use crate::plugin_manifest::Manifest;
use crate::plugin_payload;
use crate::store::{parse_version, version_is_newer};

/// Why a plugin cannot run against this amenbo (`AMB-D-359`). Each variant carries both sides of the
/// mismatch, so a caller can name the numbers rather than only the verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incompatibility {
    /// The payload contract the plugin reads is not the one this amenbo speaks — in either direction
    /// (see the module docs on why this is an equality).
    Payload {
        /// The contract version the plugin's manifest declares.
        plugin: u32,
        /// The contract version this amenbo produces ([`plugin_payload::VERSION`]).
        amenbo: u32,
    },
    /// The running amenbo is below the plugin's declared floor.
    AmenboTooOld {
        /// The floor the manifest declares.
        min: String,
        /// The running amenbo version.
        running: String,
    },
    /// The declared floor is not a version this amenbo can compare against.
    UnreadableFloor {
        /// The uncomparable `min_amenbo` string, as the manifest wrote it.
        min: String,
    },
}

impl Incompatibility {
    /// The sentence — what a log line and the refusal it turns into both say.
    fn en(&self) -> String {
        match self {
            Self::Payload { plugin, amenbo } => format!(
                "it reads payload contract v{plugin}, and this amenbo speaks v{amenbo}"
            ),
            Self::AmenboTooOld { min, running } => {
                format!("it needs amenbo {min} or newer, and this is {running}")
            }
            Self::UnreadableFloor { min } => {
                format!("it declares a minimum amenbo version that cannot be read ('{min}')")
            }
        }
    }

    /// The verdict as one sentence that names itself — the reason under both refusals below.
    ///
    /// It rides as a [`Msg::part`] rather than as either refusal's own code, because the same three
    /// verdicts read under two different sentences (enabling, and updating). Splitting them the other
    /// way would be six codes saying three things.
    fn reason(&self) -> Msg {
        match self {
            Self::Payload { plugin, amenbo } => Msg::new(self.en())
                .coded(ErrorCode::PluginIncompatiblePayload)
                .with("plugin", plugin)
                .with("amenbo", amenbo),
            Self::AmenboTooOld { min, running } => Msg::new(self.en())
                .coded(ErrorCode::PluginIncompatibleAmenboOld)
                .with("min", min)
                .with("running", running),
            Self::UnreadableFloor { min } => Msg::new(self.en())
                .coded(ErrorCode::PluginIncompatibleFloorUnreadable)
                .with("min", min),
        }
    }

    /// Turn the verdict into the refusal a named plugin's caller returns (`plugin enable`).
    pub fn into_error(self, plugin: &str) -> Error {
        Error::Invalid(
            Msg::new(format!(
                "plugin '{plugin}' is not compatible with this amenbo: {}",
                self.en()
            ))
            .coded(ErrorCode::InvalidPluginIncompatible)
            .with("name", plugin)
            .part(self.reason()),
        )
    }

    /// Turn the verdict into the refusal an **update** returns (`plugin update`). The difference from
    /// [`into_error`](Self::into_error) is what the reader has to know afterwards: the verdict is about
    /// the build the catalog is offering, and refusing it changes nothing — the installed plugin keeps
    /// running as it was (`AMB-D-359`, failing safe). Written out separately rather than bolted on as a
    /// suffix, because the whole sentence reads differently.
    pub fn into_update_error(self, plugin: &str) -> Error {
        Error::Invalid(
            Msg::new(format!(
                "the build of '{plugin}' the catalog publishes does not run on this amenbo ({}) — nothing was replaced, and the installed build is untouched",
                self.en()
            ))
            .coded(ErrorCode::InvalidPluginUpdateIncompatible)
            .with("name", plugin)
            .part(self.reason()),
        )
    }
}

impl fmt::Display for Incompatibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.en())
    }
}

/// Check a manifest's compatibility declarations against the running build. `Ok(())` means the plugin
/// may be enabled and fired; the error says what does not match.
pub fn check(manifest: &Manifest) -> Result<(), Incompatibility> {
    check_against(manifest, plugin_payload::VERSION, crate::agent::VERSION)
}

/// [`check`] against a stated payload contract and amenbo version — the whole comparison, with the
/// running build's two constants passed in so it is testable without a time machine.
fn check_against(
    manifest: &Manifest,
    amenbo_payload_v: u32,
    running: &str,
) -> Result<(), Incompatibility> {
    if manifest.payload_v != amenbo_payload_v {
        return Err(Incompatibility::Payload {
            plugin: manifest.payload_v,
            amenbo: amenbo_payload_v,
        });
    }
    if let Some(min) = manifest.min_amenbo.as_deref() {
        // Read it first, compare second: `version_is_newer` answers "not newer" for a string it cannot
        // parse, which is the right default for noticing a release and the wrong one for a floor.
        if parse_version(min).is_none() {
            return Err(Incompatibility::UnreadableFloor { min: min.to_string() });
        }
        if version_is_newer(min, running) {
            return Err(Incompatibility::AmenboTooOld {
                min: min.to_string(),
                running: running.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::Os;

    /// A manifest carrying just the two compatibility fields; the rest is filler this gate never reads.
    fn manifest(payload_v: u32, min_amenbo: Option<&str>) -> Manifest {
        Manifest {
            name: "slack".into(),
            desc: String::new(),
            author: String::new(),
            repo: String::new(),
            os: vec![Os::Linux],
            category: String::new(),
            url: String::new(),
            checksum: String::new(),
            signature: None,
            assets: Default::default(),
            official: false,
            detail_sum: None,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v,
            min_amenbo: min_amenbo.map(str::to_string),
            config: Vec::new(),
            events: Vec::new(),
            agent: None,
        }
    }

    /// The ordinary case: the same contract, and a floor at or below the running build.
    #[test]
    fn a_matching_contract_and_a_met_floor_pass() {
        assert!(check_against(&manifest(1, None), 1, "1.8.0").is_ok(), "no floor is no requirement");
        assert!(check_against(&manifest(1, Some("1.8.0")), 1, "1.8.0").is_ok(), "equal meets the floor");
        assert!(check_against(&manifest(1, Some("1.7.0")), 1, "1.8.0").is_ok(), "newer clears it");
    }

    /// Every real manifest today targets the contract this build speaks — the gate is inert until a
    /// breaking bump moves one side.
    #[test]
    fn todays_baseline_manifest_passes_the_live_check() {
        assert!(check(&manifest(plugin_payload::VERSION, None)).is_ok());
    }

    /// amenbo outgrew the plugin: `v` moved on a breaking change, so the old plugin is stopped rather
    /// than fed a payload whose meaning moved (`AMB-D-349` / `AMB-D-359`).
    #[test]
    fn a_plugin_left_behind_by_a_payload_bump_is_incompatible() {
        let err = check_against(&manifest(1, None), 2, "1.8.0").unwrap_err();
        assert_eq!(err, Incompatibility::Payload { plugin: 1, amenbo: 2 });
        assert!(err.to_string().contains("v1"), "both sides are named: {err}");
        assert!(err.to_string().contains("v2"));
    }

    /// And the other direction: a plugin written against a contract this build cannot produce.
    #[test]
    fn a_plugin_reading_a_newer_contract_is_incompatible() {
        assert_eq!(
            check_against(&manifest(2, None), 1, "1.8.0").unwrap_err(),
            Incompatibility::Payload { plugin: 2, amenbo: 1 }
        );
    }

    /// The floor is above the running build: incompatible, and the refusal names the two versions.
    #[test]
    fn a_floor_above_the_running_build_is_incompatible() {
        let err = check_against(&manifest(1, Some("1.9.0")), 1, "1.8.0").unwrap_err();
        assert_eq!(
            err,
            Incompatibility::AmenboTooOld { min: "1.9.0".into(), running: "1.8.0".into() }
        );
        let refusal = format!("{:?}", err.into_error("slack"));
        assert!(refusal.contains("slack") && refusal.contains("1.9.0"), "{refusal}");
    }

    /// The floor comparison is numeric, not lexical — `1.10.0` is above `1.9.0`.
    #[test]
    fn the_floor_is_compared_numerically() {
        assert_eq!(
            check_against(&manifest(1, Some("1.10.0")), 1, "1.9.0").unwrap_err(),
            Incompatibility::AmenboTooOld { min: "1.10.0".into(), running: "1.9.0".into() }
        );
        assert!(check_against(&manifest(1, Some("1.9.0")), 1, "1.10.0").is_ok());
    }

    /// A floor nobody can compare is reported, not waved through: amenbo cannot claim to meet a version
    /// it could not read (see the module docs).
    #[test]
    fn an_unreadable_floor_is_incompatible() {
        let err = check_against(&manifest(1, Some("latest")), 1, "1.8.0").unwrap_err();
        assert_eq!(err, Incompatibility::UnreadableFloor { min: "latest".into() });
        assert!(err.to_string().contains("latest"), "the string is quoted back: {err}");
    }

    /// A pre-release running build still meets a floor at its release version — the metadata after `-`
    /// is ignored, the same loose reading the update check uses.
    #[test]
    fn pre_release_metadata_does_not_change_the_verdict() {
        assert!(check_against(&manifest(1, Some("1.8.0")), 1, "1.8.0-rc.1").is_ok());
    }
}
