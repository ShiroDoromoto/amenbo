//! The stand-in for a target amenbo has not learned to register on.
//!
//! It is not a failure and not a refusal to answer: reading the scheduler still works and says nothing
//! is held, and the two writes refuse with the plain fact — so a caller reports a target with no door
//! rather than a registration that was never written.
//!
//! Each OS's real door lands with the work that writes it (`AMB-T-3253`), and what goes inside one is
//! per-OS in a way the rest of [`super`] is deliberately not: a plist reached through `SMAppService`, a
//! scheduler task built from XML to get past the battery gates, and a pair of systemd user units are not
//! one shape with three spellings.

use crate::error::{Error, Result};

/// No door on this target.
pub(super) const AVAILABLE: bool = false;

/// What every face here says while there is no door: the plain fact, and no hint, because there is
/// nothing the reader could type to fix it.
const NO_DOOR: &str = "amenbo cannot register the hourly tick on this system yet";

/// Nothing is out of reach on a target where nothing is in reach: the honest answer about *where* the
/// door can be opened from is that there is no door, which [`AVAILABLE`] already says.
pub(super) fn reachable_from_here() -> bool {
    AVAILABLE
}

/// There is nothing to launch that would answer differently.
pub(super) fn relaunch_target() -> Option<std::path::PathBuf> {
    None
}

pub(super) fn probe() -> Result<bool> {
    Ok(false)
}

pub(super) fn register() -> Result<()> {
    Err(Error::invalid(NO_DOOR))
}

pub(super) fn unregister() -> Result<()> {
    Err(Error::invalid(NO_DOOR))
}
