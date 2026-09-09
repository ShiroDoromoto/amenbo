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

/// What the startup clone check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwCheck {
    /// The machine answers with what is written down. Nothing to do.
    Same,
    /// A different string for the same machine — what changed is the reader, not the hardware
    /// (`AMB-D-870`). `bound_hw` wants rewriting; nobody wants telling.
    Restated,
    /// The store looks to have been copied onto a different machine (and wants forking).
    Cloned,
}

impl Identity {
    /// Issue the identity for a new store.
    pub fn generate(user_name: &str) -> Identity {
        Identity {
            user_name: user_name.to_string(),
            bound_hw: live_hw(),
        }
    }

    /// The startup clone check.
    ///
    /// 🚨 Three answers and not two, because **the string can change without the machine changing.**
    /// Windows was asked through PowerShell and is now asked of the firmware directly (`AMB-D-870`), and a
    /// build that spells the same UUID even slightly differently would tell every existing reader that
    /// their store had been copied. So a mismatch is put to the old reader once, and an agreement there is
    /// a restatement: rewrite what is written down, and say nothing to anybody.
    pub fn hw_check(&self) -> HwCheck {
        let live = live_hw();
        // When either side is unobtainable ("unknown"), call it a match rather than raise a false alarm.
        if self.bound_hw == "unknown" || live == "unknown" || self.bound_hw == live {
            return HwCheck::Same;
        }
        if restated(&self.bound_hw) { HwCheck::Restated } else { HwCheck::Cloned }
    }

    /// Whether the store looks to have been copied onto a different machine (and wants forking).
    pub fn hw_mismatch(&self) -> bool {
        self.hw_check() == HwCheck::Cloned
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

/// The UUID of the machine we are on. `AMENBO_HW_ID` overrides it, so development can pretend to be
/// another machine. The value is read from **the hardware**, not from a file on disk — a file would be
/// copied along with a clone.
///
/// Asked of the OS on the first call and kept for the life of the process (`AMB-D-868`). The hardware
/// does not change under a running process, so the answer cannot either; asking costs a process launch
/// on macOS (`ioreg`), which [`crate::Store::open_at`] would otherwise pay on every single write. The
/// CLI is one process per command, so it stays at the one ask it always had — which is why Windows
/// reads the firmware table itself rather than sending PowerShell to do it (`AMB-D-870`).
pub fn live_hw() -> String {
    static HW: OnceLock<String> = OnceLock::new();
    HW.get_or_init(|| {
        if let Some(v) = crate::env::hw_id() {
            return v.to_string_lossy().into_owned();
        }
        platform_hw().unwrap_or_else(|| "unknown".to_string())
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
    // Read straight out of the table the firmware left in memory, with no process launched for it
    // (`AMB-D-870`). PowerShell stays as the backstop for a machine whose table cannot be read at all,
    // where the answer would otherwise be lost and clone detection quietly switch itself off.
    smbios_table().as_deref().and_then(smbios_uuid).or_else(powershell_hw)
}

/// The raw SMBIOS table, as the firmware left it: asked for the size it needs, then read into that.
#[cfg(target_os = "windows")]
fn smbios_table() -> Option<Vec<u8>> {
    use windows_sys::Win32::System::SystemInformation::{GetSystemFirmwareTable, RSMB};

    // SAFETY: a null buffer with a zero size is how this call is asked for the size it wants.
    let size = unsafe { GetSystemFirmwareTable(RSMB, 0, std::ptr::null_mut(), 0) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: the buffer is `size` bytes long and outlives the call, which writes at most that many and
    // reports how many it wrote.
    let written = unsafe { GetSystemFirmwareTable(RSMB, 0, buf.as_mut_ptr(), size) };
    if written == 0 || written > size {
        return None;
    }
    buf.truncate(written as usize);
    Some(buf)
}

/// What every build before `AMB-D-870` asked, kept for the two jobs left to it: the machine whose
/// firmware table will not be read, and the one-off restatement in [`Identity::hw_check`]. Cached, so
/// both of them together cost the one process launch.
#[cfg(target_os = "windows")]
fn powershell_hw() -> Option<String> {
    static ASKED: OnceLock<Option<String>> = OnceLock::new();
    ASKED.get_or_init(ask_powershell).clone()
}

#[cfg(target_os = "windows")]
fn ask_powershell() -> Option<String> {
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

/// Whether the machine a store was last opened on is the one under us after all — put to the reader
/// that wrote `bound_hw` in the first place.
///
/// Windows alone has one, and it answers on the one upgrade that changes the reader: PowerShell is what
/// the older builds asked, so its answer is what an older `bound_hw` holds (`AMB-D-870`). `AMENBO_HW_ID`
/// is a development override standing in for the hardware, and a store bound under one is not a store to
/// go behind the override for.
#[cfg(target_os = "windows")]
fn restated(bound: &str) -> bool {
    crate::env::hw_id().is_none() && powershell_hw().as_deref() == Some(bound)
}

/// Nowhere else has a second reader: macOS and Linux spell the UUID the way they always did.
#[cfg(not(target_os = "windows"))]
fn restated(_bound: &str) -> bool {
    false
}

/// The machine UUID out of the raw SMBIOS table, spelled the way `Win32_ComputerSystemProduct` spells
/// it — which is what `bound_hw` holds on every machine that has run Amenbo before (`AMB-D-870`).
///
/// The table is a run of structures: four bytes of header (type, length, handle), a formatted area of
/// that length, then the strings it points into, ended by two NULs. The one wanted is type 1, System
/// Information, whose UUID is the sixteen bytes at offset 8.
///
/// 🚨 **The bytes are not in reading order.** From SMBIOS 2.6 the first three fields of the UUID
/// are little-endian on the wire, and what WMI hands back has already been turned around. Spelling it
/// any other way reads as a different machine to every store an older build wrote. What decides it is
/// the version in the table's own header, so firmware still on 2.5 is left the way round it is.
///
/// Not behind `cfg(windows)`, though only Windows calls it: nothing in here touches Windows, and a
/// parse whose only run is on one CI image is a parse nobody can try.
#[cfg(any(windows, test))]
fn smbios_uuid(raw: &[u8]) -> Option<String> {
    let major = *raw.get(1)?;
    let minor = *raw.get(2)?;
    let stated = u32::from_le_bytes(raw.get(4..8)?.try_into().ok()?) as usize;
    let all = raw.get(8..)?;
    // The stated length is the firmware's; what arrived is the call's. Believe whichever is shorter.
    let table = all.get(..stated).unwrap_or(all);

    let mut at = 0usize;
    while at + 4 <= table.len() {
        let kind = table[at];
        let formatted = table[at + 1] as usize;
        // Type 127 ends the table, and a structure shorter than its own header has nothing after it.
        if kind == 127 || formatted < 4 {
            return None;
        }
        // A 2.0 System Information structure stops before the UUID, so the field has to be inside the
        // formatted area rather than merely inside the table.
        if kind == 1 && formatted >= 24 {
            return uuid_text(table.get(at + 8..at + 24)?, major, minor);
        }
        // Past the formatted area lies the string set, ended by the two NULs that also end a set with
        // no strings in it at all.
        let mut end = at + formatted;
        while end + 1 < table.len() && !(table[end] == 0 && table[end + 1] == 0) {
            end += 1;
        }
        at = end + 2;
    }
    None
}

/// The sixteen bytes as a UUID reads, turned the right way round for the SMBIOS version that wrote them.
#[cfg(any(windows, test))]
fn uuid_text(bytes: &[u8], major: u8, minor: u8) -> Option<String> {
    // All-Fs is what a machine with no UUID set answers with; it is not an id.
    if bytes.iter().all(|&b| b == 0xFF) {
        return None;
    }
    let b: [u8; 16] = bytes.try_into().ok()?;
    let f = if (major, minor) >= (2, 6) {
        [b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]
    } else {
        b
    };
    let hex = |part: &[u8]| part.iter().map(|byte| format!("{byte:02X}")).collect::<String>();
    Some(format!("{}-{}-{}-{}-{}", hex(&f[0..4]), hex(&f[4..6]), hex(&f[6..8]), hex(&f[8..10]), hex(&f[10..16])))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_hw() -> Option<String> {
    // Linux: the DMI product_uuid. `/etc/machine-id` will not do — it is a file, so a clone carries it.
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

    /// A raw SMBIOS buffer as `GetSystemFirmwareTable` hands one back: the version bytes, the length,
    /// then the structures themselves.
    fn raw(major: u8, minor: u8, structures: &[u8]) -> Vec<u8> {
        let mut out = vec![0, major, minor, 0];
        out.extend_from_slice(&(structures.len() as u32).to_le_bytes());
        out.extend_from_slice(structures);
        out
    }

    /// A System Information structure (type 1) carrying `uuid` where the spec puts it, with a string
    /// after it — the shape the parse has to walk over to reach anything beyond.
    fn system_information(uuid: [u8; 16]) -> Vec<u8> {
        // Type, length (0x1B — the 2.1 structure), handle; then the four string numbers.
        let mut s = vec![1, 0x1B, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04];
        s.extend_from_slice(&uuid);
        // Wake-up type, SKU, family, then the strings and the NUL that ends the set.
        s.extend_from_slice(&[0x06, 0x05, 0x06]);
        s.extend_from_slice(b"ACME\0\0");
        s
    }

    /// The example the spec itself gives: the UUID 00112233-4455-6677-8899-AABBCCDDEEFF, on the wire.
    const WIRE: [u8; 16] = [
        0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
    ];

    /// 🚨 The whole of what makes this a drop-in for the PowerShell answer: uppercase, hyphenated, and
    /// with the first three fields turned back round. Spelled any other way, every store an older build
    /// wrote would read as one copied onto another machine.
    #[test]
    fn spells_the_uuid_the_way_wmi_spells_it() {
        let table = raw(3, 4, &system_information(WIRE));
        assert_eq!(smbios_uuid(&table).as_deref(), Some("00112233-4455-6677-8899-AABBCCDDEEFF"));
    }

    /// Firmware still on 2.5 wrote the bytes in reading order, and turning those round would invent a
    /// machine that does not exist.
    #[test]
    fn leaves_the_bytes_the_way_round_older_firmware_wrote_them() {
        let table = raw(2, 5, &system_information(WIRE));
        assert_eq!(smbios_uuid(&table).as_deref(), Some("33221100-5544-7766-8899-AABBCCDDEEFF"));
    }

    /// All-Fs is what a machine with no UUID set answers with. It is the same on every such machine, so
    /// taking it as an id would bind every one of them to the same store.
    #[test]
    fn says_nothing_for_a_machine_with_no_uuid_set() {
        let table = raw(3, 4, &system_information([0xFF; 16]));
        assert!(smbios_uuid(&table).is_none());
    }

    /// System Information is rarely the first structure, and the ones before it carry strings — so the
    /// walk has to step over a string set rather than over the formatted area alone.
    #[test]
    fn walks_past_the_structures_before_it() {
        // A BIOS Information structure (type 0), whose strings are what the naive step lands inside of.
        let mut bios = vec![0, 0x14, 0x00, 0x00];
        bios.extend_from_slice(&[0u8; 0x10]);
        bios.extend_from_slice(b"VENDOR\0v1.0\0\0");
        let mut structures = bios;
        structures.extend_from_slice(&system_information(WIRE));
        let table = raw(3, 4, &structures);
        assert_eq!(smbios_uuid(&table).as_deref(), Some("00112233-4455-6677-8899-AABBCCDDEEFF"));
    }

    /// A 2.0 System Information structure stops before the UUID. Reading sixteen bytes from offset 8
    /// anyway would hand back whatever the next structure begins with.
    #[test]
    fn says_nothing_where_the_structure_stops_before_the_uuid() {
        let structures = vec![1, 0x08, 0x01, 0x00, 0x01, 0x02, 0x03, 0x04, 0x00, 0x00];
        assert!(smbios_uuid(&raw(3, 4, &structures)).is_none());
    }

    /// Type 127 ends the table. Whatever bytes lie past it are not structures.
    #[test]
    fn says_nothing_when_the_table_ends_first() {
        let mut structures = vec![127, 0x04, 0x02, 0x00, 0x00, 0x00];
        structures.extend_from_slice(&system_information(WIRE));
        assert!(smbios_uuid(&raw(3, 4, &structures)).is_none());
    }

    /// A buffer too short to hold even the header it claims is not one to read fields out of.
    #[test]
    fn says_nothing_for_a_buffer_with_no_table_in_it() {
        assert!(smbios_uuid(&[]).is_none());
        assert!(smbios_uuid(&[0, 3, 4, 0]).is_none());
    }
}
