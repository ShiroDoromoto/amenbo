//! The store's local identity.
//!
//! - **`bound_hw` (the machine)**: a signal for clone detection, nothing more. **It is not an identity.**
//!
//! It is stored locally in `identity.json` and **never synced** (a separate file from `store.sqlite`).
//!
//! It holds no device secret of any kind. Protecting what is on the device is left to the OS's full-disk
//! encryption (FileVault / BitLocker).

use std::path::Path;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The store's local identity (never synced). **It holds no secrets.**
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    /// The display name — the starting point for lining this device up with the "who" in the data
    /// (assignees, comment authors).
    pub user_name: String,
    /// The machine UUID as it was when this identity was generated. **A clone-detection signal only**,
    /// checked against the live one at startup.
    pub bound_hw: String,
}

impl Identity {
    /// Issue the identity for a new store.
    pub fn generate(user_name: &str) -> Identity {
        Identity {
            user_name: user_name.to_string(),
            bound_hw: live_hw(),
        }
    }

    /// The startup clone check. `Some(true)` means the store looks to have been copied onto a different
    /// machine (and wants forking); `Some(false)` is a check that was made and passed. `None` is neither
    /// — one of the two sides is [`UNKNOWN_HW`], so no check was made at all. Telling the two apart is
    /// what keeps a machine that hands out no hardware id (Linux, mostly — see [`platform_hw`]) from
    /// being reported as one that passed.
    pub fn hw_mismatch(&self) -> Option<bool> {
        hw_mismatch_between(&self.bound_hw, &live_hw())
    }

    /// Rebind after a clone is detected: point `bound_hw` at the machine we are actually on.
    pub fn rebind_hw(&mut self) {
        self.bound_hw = live_hw();
    }

    pub fn load(path: &Path) -> Result<Identity> {
        let raw = std::fs::read_to_string(path)?;
        serde_json::from_str(&raw).map_err(Error::from)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        // No secrets in here, so no special permissions to lock down. Replace it atomically
        // (write a temp file, then rename).
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

}

/// What [`live_hw`] answers when the machine hands out no hardware id. It is not an id: a store bound
/// to it and a store running on it are not the same machine, they are two machines nobody measured.
pub const UNKNOWN_HW: &str = "unknown";

/// The comparison behind [`Identity::hw_mismatch`], split out so the three answers can be tested without
/// a machine to run them on ([`live_hw`] settles once per process).
fn hw_mismatch_between(bound: &str, live: &str) -> Option<bool> {
    if bound == UNKNOWN_HW || live == UNKNOWN_HW {
        return None;
    }
    Some(bound != live)
}

/// The UUID of the machine we are on. `AMENBO_HW_ID` overrides it, so development can pretend to be
/// another machine. The value is read from **the hardware**, not from a file on disk — a file would be
/// copied along with a clone.
///
/// Asked of the OS on the first call and kept for the life of the process (`AMB-D-868`). The hardware
/// does not change under a running process, so the answer cannot either; asking costs a process launch
/// on macOS (`ioreg`) and on Windows (PowerShell), which [`crate::Store::open_at`] would otherwise pay
/// on every single write. The CLI is one process per command, so it stays at the one ask it always had.
pub fn live_hw() -> String {
    static HW: OnceLock<String> = OnceLock::new();
    HW.get_or_init(|| {
        if let Some(v) = crate::env::hw_id() {
            return v.to_string_lossy().into_owned();
        }
        platform_hw().unwrap_or_else(|| UNKNOWN_HW.to_string())
    })
    .clone()
}

#[cfg(target_os = "macos")]
fn platform_hw() -> Option<String> {
    // IOKit's IOPlatformUUID: from the hardware, not from a file.
    let out = crate::sys::command("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if let Some(idx) = line.find("IOPlatformUUID") {
            // The line reads: "IOPlatformUUID" = "XXXX-...."
            let rest = &line[idx..];
            if let Some(start) = rest.find("= \"") {
                let after = &rest[start + 3..];
                if let Some(end) = after.find('"') {
                    return Some(after[..end].to_string());
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn platform_hw() -> Option<String> {
    // The SMBIOS product UUID: it comes from the firmware — burnt into the hardware, not held in a file.
    // `wmic` is gone from recent Windows, so this goes through PowerShell's CIM.
    let out = crate::sys::command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance -ClassName Win32_ComputerSystemProduct).UUID",
        ])
        .output()
        .ok()?;
    let uuid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // Some machines answer with all-Fs when no UUID is set; treat that as "unknown" rather than as an id.
    if uuid.is_empty() || uuid.eq_ignore_ascii_case("FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF") {
        None
    } else {
        Some(uuid)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_hw() -> Option<String> {
    // Linux: the DMI product_uuid. `/etc/machine-id` will not do — it is a file, so a clone carries it.
    //
    // **On most machines this answers `None`**, and that is the expected outcome, not a failure: the
    // kernel creates `product_uuid` mode 0400, so nobody but root can open it, and an ARM machine has no
    // DMI table to create it from. Clone detection is simply not made on Linux — `whoami` says so rather
    // than reporting a check that passed. There is no stand-in: `/etc/machine-id` travels with a copy,
    // and a MAC address or a disk serial either misses a per-disk copy or cries wolf when the OS is
    // reinstalled or a dock is unplugged. The cost of the gap is one missing warning line, so it stands.
    std::fs::read_to_string("/sys/class/dmi/id/product_uuid")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `identity.json` carrying keys this build knows nothing about (keys, seeds, device labels) still
    /// loads: the unknown ones are ignored, not fatal.
    #[test]
    fn legacy_field_names_are_ignored_on_load() {
        let legacy = r#"{
            "replica_id": "01REPLICA",
            "user_id": "PUBKEY",
            "user_name": "Alice",
            "bound_hw": "hw-1",
            "user_secret": "SECRET",
            "user_public": "PUBKEY",
            "endpoint_secret": "DEV_SECRET",
            "endpoint_id": "DEV_PUBLIC",
            "device_public_key": "DEV_PUBLIC"
        }"#;
        let id: Identity = serde_json::from_str(legacy).expect("an identity.json with the legacy field names still loads");
        assert_eq!(id.user_name, "Alice");
        assert_eq!(id.bound_hw, "hw-1");
        // Writing it back drops the unknown keys, so the round trip settles on the schema.
        let json = serde_json::to_string(&id).unwrap();
        assert!(!json.contains("device_public_key") && !json.contains("device_secret_key"));
        let round: Identity = serde_json::from_str(&json).unwrap();
        assert_eq!(round.user_name, "Alice");
    }

    /// The clone check has three answers, and the third is the point: a machine that hands out no
    /// hardware id is not a machine that passed.
    #[test]
    fn an_unmeasured_side_is_neither_a_match_nor_a_mismatch() {
        assert_eq!(hw_mismatch_between("hw-1", "hw-1"), Some(false), "same machine — checked, and it passed");
        assert_eq!(hw_mismatch_between("hw-1", "hw-2"), Some(true), "another machine — checked, and it caught the copy");
        assert_eq!(hw_mismatch_between(UNKNOWN_HW, "hw-1"), None, "bound to nothing measurable — no check was made");
        assert_eq!(hw_mismatch_between("hw-1", UNKNOWN_HW), None, "running where nothing is measurable — no check was made");
        assert_eq!(hw_mismatch_between(UNKNOWN_HW, UNKNOWN_HW), None, "neither side measured — not a match");
    }
}
