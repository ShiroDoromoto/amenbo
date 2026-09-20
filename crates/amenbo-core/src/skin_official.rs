//! The skins amenbo ships with — held in the binary, not put on the device.
//!
//! **They are worked examples, one per shape of the open vocabulary.** A third party adds looks;
//! what ships is the set of kinds, so that an author can see what each part of the vocabulary is
//! for before writing any of it: colour alone (`washi`), colour with the lengths and the faces the
//! device already has (`terminal`), and a skin that carries its own face and redraws the frame
//! (`retro`). `high-contrast` is not an example — it is the way back when a skin has left the
//! screen unreadable, and it clears AAA (7:1) on every pairing, which the check does not ask of
//! anyone else.
//!
//! **Held in the binary rather than written into `<base>/skins/`.** A copy on the device is a copy
//! that can rot: it would be one build's idea of `washi` sitting under a later build's vocabulary,
//! with nothing able to correct it, and deleting one would raise the question of whether the next
//! launch puts it back. In the binary there is one answer — this build ships these four, and an
//! update ships whatever the next one does.
//!
//! **Their names are reserved.** [`crate::skin::Skin::install`] turns away a file calling itself
//! one of them, so the list never holds two skins under one name and `skin use washi` never has to
//! say which.
//!
//! The list is ordered as the settings screen reads it rather than alphabetically:
//! `high-contrast` is first because it is the one somebody is looking for in a hurry.

/// One skin this build carries, as the document an author would have been handed.
pub struct Official {
    /// The name it is held and worn under. Reserved: no file may be taken in calling itself this.
    pub name: &'static str,
    /// The whole document, byte for byte as it is in the tree — the same shape a skin arrives in
    /// from anybody else, so that reading one of these teaches the format.
    pub yaml: &'static str,
}

/// Every skin this build ships, in the order they are listed.
pub const OFFICIAL: &[Official] = &[
    Official { name: "high-contrast", yaml: include_str!("../skins/high-contrast.yaml") },
    Official { name: "washi", yaml: include_str!("../skins/washi.yaml") },
    Official { name: "terminal", yaml: include_str!("../skins/terminal.yaml") },
    Official { name: "retro", yaml: include_str!("../skins/retro.yaml") },
];

/// The document this build ships under `name`, if it ships one.
pub fn yaml(name: &str) -> Option<&'static str> {
    OFFICIAL.iter().find(|o| o.name == name).map(|o| o.yaml)
}

/// Is this a name this build ships? Asked where a device file would otherwise be written or
/// removed under it.
pub fn is_official(name: &str) -> bool {
    OFFICIAL.iter().any(|o| o.name == name)
}

/// What every face says to a file calling itself one of the shipped names. One sentence in one
/// place: the terminal, the window's judgement and the install all turn the same file away, and a
/// reader who saw two wordings would be asking whether they hit two different rules.
pub fn name_is_ours(name: &str) -> String {
    format!("'{name}' is a skin this Amenbo ships; rename the one being taken in")
}

/// What every face says to a request to take a shipped skin off the device. It is not there.
pub fn not_on_the_device(name: &str) -> String {
    format!("'{name}' is a skin this Amenbo ships; it is not on the device to remove")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::Skin;

    /// Every shipped skin is a skin: it reads, the check takes all of it, and nothing in it is a
    /// name or a value this build sets aside. A warning on one of these is a file that got past
    /// review, not a reader being told something.
    #[test]
    fn every_shipped_skin_reads_clean() {
        for o in OFFICIAL {
            let skin = Skin::read(o.yaml).unwrap_or_else(|e| panic!("{}: {e}", o.name));
            assert_eq!(skin.name, o.name, "the document's name is the name it is held under");
            assert!(skin.unknown_keys.is_empty(), "{}: {:?}", o.name, skin.unknown_keys);
            let taken = skin.check().unwrap_or_else(|r| panic!("{}: {r:?}", o.name));
            assert!(taken.warnings.is_empty(), "{}: {:?}", o.name, taken.warnings);
        }
    }

    /// And every one of them is readable: each pairing on each side it declares clears its floor.
    #[test]
    fn every_shipped_skin_clears_its_floors() {
        for o in OFFICIAL {
            let taken = Skin::read(o.yaml).unwrap().check().unwrap();
            let report = crate::skin_contrast::measure(&taken.skin);
            assert!(report.short.is_empty(), "{}: {:?}", o.name, report.short);
            assert!(report.unread.is_empty(), "{}: {:?}", o.name, report.unread);
            assert!(report.measured > 0, "{}: nothing was measured", o.name);
        }
    }

    /// `high-contrast` promises more than the check asks. It is the way back from a screen a skin
    /// has made unreadable, so every pairing clears AAA rather than AA — held here, because the
    /// promise is the whole of what that skin is for.
    #[test]
    fn high_contrast_clears_aaa() {
        let taken = Skin::read(yaml("high-contrast").unwrap()).unwrap().check().unwrap();
        let report = crate::skin_contrast::measure_at(&taken.skin, 7.0, 4.5);
        assert!(report.short.is_empty(), "{:?}", report.short);
        assert_eq!(report.measured, 70, "both sides, every pairing");
    }

    /// The set is the set of kinds, not a set of looks: one that is only colour, one that reaches
    /// the lengths and the faces the device has, and one that carries its own face and redraws the
    /// frame. A skin added without a kind of its own is a look, and looks are not amenbo's to add.
    #[test]
    fn the_set_covers_each_kind() {
        let kind = |name: &str| {
            let taken = Skin::read(yaml(name).unwrap()).unwrap().check().unwrap();
            let names: Vec<&str> = taken
                .skin
                .light
                .values
                .keys()
                .chain(taken.skin.dark.values.keys())
                .map(String::as_str)
                .collect();
            (names.iter().any(|n| !n.starts_with("c-")), taken.skin.font.is_some())
        };
        assert_eq!(kind("washi"), (false, false), "colour and nothing else");
        assert_eq!(kind("terminal"), (true, false), "the lengths too, with the device's own faces");
        assert_eq!(kind("retro"), (true, true), "and one that brings its own face");
        assert_eq!(
            Skin::read(yaml("retro").unwrap()).unwrap().themes,
            vec!["dark"],
            "retro is the black ground; there is no light side of it"
        );
    }

    /// The font travels with the skin, decoded and all — a reader who is handed `retro` is handed
    /// the face, not a name to go and find.
    #[test]
    fn retro_carries_its_face() {
        let taken = Skin::read(yaml("retro").unwrap()).unwrap().check().unwrap();
        let file = taken.skin.font.expect("retro carries a font");
        assert_eq!(file.family, "DotGothic16");
        assert!(file.license_text.contains("SIL OPEN FONT LICENSE"), "the terms travel with it");
        let bytes = taken.font.expect("the check took the bytes");
        assert_eq!(&bytes[..4], b"wOF2");
        assert!(bytes.len() < crate::skin::FONT_MAX_BYTES, "{} bytes", bytes.len());
    }

    /// A shipped name is not one a file may arrive under, and not one there is a file to remove.
    #[test]
    fn shipped_names_are_reserved() {
        let paths = crate::config::Paths::at(amenbo_scratch::scratch("shipped-skins"));
        let bare = crate::skin::Packing::Bare;
        assert!(Skin::install(&paths, "washi", bare, yaml("washi").unwrap().as_bytes()).is_err());
        assert!(Skin::uninstall(&paths, "washi").is_err());
        // And they are held without anything being on the device.
        assert!(Skin::installed(&paths, "washi").unwrap().is_some());
        let all = Skin::installed_all(&paths);
        assert_eq!(
            all.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            OFFICIAL.iter().map(|o| o.name).collect::<Vec<_>>(),
        );
    }
}
