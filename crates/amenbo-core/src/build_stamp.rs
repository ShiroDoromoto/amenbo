//! **Is this binary a release artifact?** — the build-time stamp, and the gate it feeds
//! (`AMB-D-378`).
//!
//! A store migration is the one irreversible thing Amenbo does to a user's data (`AMB-D-231`): the
//! chain moves the format forward, and the released app can no longer open what an unreleased build
//! carried past it. Everything else about a local build is harmless — it reads, it writes, it can be
//! thrown away.
//!
//! Nothing distinguished a working-tree build from a shipped one at run time. Both are named
//! `amenbo`, both report the same version (a bump happens at release, so an unreleased tree wears the
//! released number), and both point at the same app-data directory, because that directory is chosen
//! by `AMENBO_APP_NAME` at build time and only the *dev* channel passes it. On 2026-07-23 that gap
//! carried a real production store from format v4 to v7 — by opening a locally built bundle once, to
//! see whether it launched.
//!
//! **The stamp closes it.** `AMENBO_BUILD=release` is set in the release workflow's environment and
//! nowhere else — not in the Makefile, not in a plain `cargo build` — so it is present in exactly the
//! binaries public CI produced. There is no other way a distributed Amenbo is built: every artifact
//! comes from `_release.yml`, and the `script` channel ships those same bytes.
//!
//! **What the gate refuses is narrow**, and all four conditions must hold ([`refuses_migration`]): an
//! unstamped build, pointed at the production channel, against the real app-data root, with no escape
//! hatch set. Launching is never refused, nor is reading and writing a store already at this build's
//! format — a local build stays useful for reproducing a bug against real data, which is the reason
//! not to answer this by quietly sending local builds to their own app-data name.
//!
//! **The stamp is also what says a build is worth verifying** (`AMB-D-540`). Pre-distribution
//! verification exists to walk the bytes that ship, so its drivers refuse anything else — and the
//! only thing they can ask a binary is a question it answers, which is why [`is_release_build`] is
//! reported by the `version` face (`release_build` in its `--json`). Nothing else about a running
//! Amenbo tells the two apart: the version number is the released one on both sides of a release,
//! and a locally built binary answers to the production channel unless it was built for the dev one.
//!
//! **The hatch is a run, not a build.** `AMENBO_ALLOW_UNSTAMPED_MIGRATE=1`
//! ([`env::allow_unstamped_migrate`](crate::env::allow_unstamped_migrate)) opens the gate for one
//! invocation, for the case the gate cannot help with: a released build that cannot recover the store
//! itself. Because it lives in the environment it cannot ride along in a bundle a double-click
//! launches.

use crate::config::Paths;
use crate::error::{Error, Result};

/// The build-time stamp the release workflow sets. `Some("release")` in a distributed binary, `None`
/// in every locally built one (see the module docs on why nothing else may set it).
pub const STAMP: Option<&str> = option_env!("AMENBO_BUILD");

/// The stamp's one meaningful value: this binary came out of the release workflow.
const RELEASE: &str = "release";

/// Whether this binary carries the release stamp.
pub fn is_release_build() -> bool {
    STAMP == Some(RELEASE)
}

/// Refuse to run a format migration when an unreleased build is pointed at real production data
/// (`AMB-D-378`). Called by every path that runs the version chain — there is no surface-level
/// equivalent, because the gate belongs to the code that migrates, not to the CLI or the GUI.
pub fn ensure_may_migrate() -> Result<()> {
    if !refuses_migration(
        is_release_build(),
        Paths::APP_NAME,
        crate::env::home().is_some(),
        crate::env::allow_unstamped_migrate(),
    ) {
        return Ok(());
    }
    Err(Error::invalid(
        "refusing to migrate: this amenbo was not built by the release workflow, and migrating your \
         production store would carry it past what the released amenbo can open. Run the released \
         amenbo to migrate, point this build at an isolated store with AMENBO_HOME=<dir>, or — if \
         this build is deliberately the one that must do it — re-run it once with \
         AMENBO_ALLOW_UNSTAMPED_MIGRATE=1",
    ))
}

/// The rule itself, over the four facts (`AMB-D-378`): refuse only when **all** hold — the build has
/// no release stamp, it is pointed at the production channel, it is reading the real app-data root
/// (no `AMENBO_HOME`), and no escape hatch is set. Taking the facts as arguments is what makes the
/// rule testable: three of the four are fixed at compile time in any one test binary.
fn refuses_migration(stamped: bool, app_name: &str, isolated: bool, hatch: bool) -> bool {
    !stamped && app_name == Paths::PRODUCTION_APP_NAME && !isolated && !hatch
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROD: &str = Paths::PRODUCTION_APP_NAME;
    const DEV: &str = "amenbo-dev";

    /// The one refused combination: unstamped, production channel, real app-data, no hatch.
    #[test]
    fn an_unstamped_build_against_the_real_production_store_is_refused() {
        assert!(refuses_migration(false, PROD, false, false));
    }

    /// Any single condition falling away lets the migration through — the gate is an AND, and each
    /// arm is a way the migration is not the dangerous one.
    #[test]
    fn every_other_combination_passes() {
        assert!(!refuses_migration(true, PROD, false, false), "a release build is what ships");
        assert!(!refuses_migration(false, DEV, false, false), "the dev channel has its own app-data");
        assert!(!refuses_migration(false, PROD, true, false), "AMENBO_HOME is an isolated store");
        assert!(!refuses_migration(false, PROD, false, true), "the hatch was set deliberately");
    }

    /// The stamp is exact: a build that says something else is not a release build (and the tests
    /// themselves run unstamped, which is what makes the gate's default the safe one).
    #[test]
    fn only_the_release_value_counts_as_a_stamp() {
        assert!(!is_release_build(), "a test binary is never a release artifact");
        assert_eq!(RELEASE, "release", "the value _release.yml sets");
    }

    /// An isolated store is never gated, whatever the build: this is the arm that keeps `make verify`,
    /// the e2e suites and any `AMENBO_HOME=<dir>` session working from a working-tree build.
    #[test]
    fn an_isolated_store_is_never_gated() {
        assert!(!refuses_migration(false, PROD, true, false));
        assert!(!refuses_migration(true, PROD, true, false));
    }
}
