//! A skin, as it comes off the file a person was handed.
//!
//! A skin is one YAML document: a header naming the skin, and a table of token values per theme
//! (`light:` / `dark:`). Nothing in it is executed and no selector can be written in it — what is
//! read here is names and values, and the `:root { … }` built from them is amenbo's own.
//!
//! **This is the reader, not the check.** It turns the document into [`Skin`] and stops there: a key
//! this build does not know, a value that is not text, a `skin_v` from a later vocabulary and a
//! `themes` line that disagrees with the tables are all things the reader carries out rather than
//! rules on, because the answer to each of them is a verdict with a warning attached and a verdict
//! belongs to the check. The reader fails only where there is nothing to hand on: a document that is
//! not YAML, and a header missing the four keys that say which skin this is.
//!
//! It sits in core because both faces take a skin in — the window a person drops a file on, and the
//! CLI that validates one before it is passed around. A reader on one side only is a skin the other
//! side cannot open.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_norway::Value;

use crate::error::Error;

/// The vocabulary this build speaks. A document declaring a higher `skin_v` was written against
/// names that are not here; what to do about that is the check's call (it refuses the skin), and the
/// reader only carries the number.
pub const SKIN_V: u32 = 1;

/// One skin, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skin {
    /// The identifier a skin is told apart by — the file it arrived in may be called anything.
    pub name: String,
    /// The name shown on screen. Not translated: it is the author's own word for their work.
    pub title: String,
    pub author: Option<String>,
    pub version: Option<String>,
    /// The vocabulary the author wrote against, to be read against [`SKIN_V`].
    pub skin_v: u32,
    /// Which sides the author says they made. Carried as written rather than as a pair of flags: a
    /// word that is neither `light` nor `dark` is something to report, and a reader that refused it
    /// would leave the check with nothing to name.
    pub themes: Vec<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub light: ThemeTable,
    pub dark: ThemeTable,
    /// The header keys this build does not know, in order. A skin written for a later amenbo is read
    /// as far as it goes, so these are carried out to be warned about rather than to refuse on.
    pub unknown_keys: Vec<String>,
}

/// The values one side of a skin sets. Token names come without the leading `--`: what the document
/// holds is the name, not the CSS spelling of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemeTable {
    /// name → value, for every entry that arrived as text.
    pub values: BTreeMap<String, String>,
    /// The names whose value was not text — a length where a colour belongs, a list, a nested map.
    /// Kept so the check can name each one while dropping it, rather than the whole table failing on
    /// one mistyped line.
    pub not_text: Vec<String>,
}

impl Skin {
    /// Read one skin document. The header's four naming keys are required; everything else is
    /// optional, because a skin that sets ten colours and leaves the rest is the ordinary case.
    pub fn read(yaml: &str) -> Result<Skin, Error> {
        let w: Wire = serde_norway::from_str(yaml)?;
        Ok(Skin {
            name: w.name,
            title: w.title,
            author: w.author,
            version: w.version,
            skin_v: w.skin_v,
            themes: w.themes,
            license: w.license,
            homepage: w.homepage,
            light: ThemeTable::split(w.light.unwrap_or_default()),
            dark: ThemeTable::split(w.dark.unwrap_or_default()),
            unknown_keys: w.rest.into_keys().collect(),
        })
    }
}

impl ThemeTable {
    /// Sort one side's entries into the ones that are text and the names of the ones that are not.
    fn split(raw: BTreeMap<String, Value>) -> ThemeTable {
        let mut values = BTreeMap::new();
        let mut not_text = Vec::new();
        for (key, value) in raw {
            match value.as_str() {
                Some(text) => {
                    values.insert(key, text.to_string());
                }
                None => not_text.push(key),
            }
        }
        ThemeTable { values, not_text }
    }
}

/// The document as serde reads it. Separate from [`Skin`] so the shape on disk can be forgiving —
/// `light:` written with nothing under it is null, not an empty map — while the type the rest of the
/// code holds is not.
#[derive(Deserialize)]
struct Wire {
    name: String,
    title: String,
    skin_v: u32,
    themes: Vec<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    light: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    dark: Option<BTreeMap<String, Value>>,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const WASHI: &str = r##"
name: washi
title: 和紙
author: Alice
version: 1.2.0
skin_v: 1
themes: [light, dark]
license: CC-BY-4.0
homepage: https://example.invalid/washi
light:
  c-bg: "#faf7f0"
  c-text: "#2b2620"
dark:
  c-bg: "#1a1713"
"##;

    #[test]
    fn reads_the_header_and_both_tables() {
        let s = Skin::read(WASHI).unwrap();
        assert_eq!(s.name, "washi");
        assert_eq!(s.title, "和紙");
        assert_eq!(s.author.as_deref(), Some("Alice"));
        assert_eq!(s.version.as_deref(), Some("1.2.0"));
        assert_eq!(s.skin_v, 1);
        assert_eq!(s.themes, ["light", "dark"]);
        assert_eq!(s.license.as_deref(), Some("CC-BY-4.0"));
        assert_eq!(s.homepage.as_deref(), Some("https://example.invalid/washi"));
        assert_eq!(s.light.values["c-bg"], "#faf7f0");
        assert_eq!(s.light.values["c-text"], "#2b2620");
        assert_eq!(s.dark.values["c-bg"], "#1a1713");
        assert!(s.unknown_keys.is_empty());
    }

    #[test]
    fn a_side_the_author_left_empty_reads_as_an_empty_table() {
        // The case the check refuses on: `themes` says both, and one side holds nothing. It has to
        // get that far to be refused for the right reason.
        let s = Skin::read("name: n\ntitle: t\nskin_v: 1\nthemes: [light, dark]\nlight:\n  c-bg: \"#fff\"\ndark:\n").unwrap();
        assert!(s.dark.values.is_empty());
        assert!(s.dark.not_text.is_empty());
        assert_eq!(s.themes, ["light", "dark"]);
    }

    #[test]
    fn a_value_that_is_not_text_is_set_aside_by_name_rather_than_failing_the_table() {
        let s = Skin::read(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\n  s-3: 12\n  c-text: [1, 2]\n",
        )
        .unwrap();
        assert_eq!(s.light.values["c-bg"], "#fff");
        assert_eq!(s.light.not_text, ["c-text", "s-3"]);
    }

    #[test]
    fn a_header_key_this_build_does_not_know_is_carried_out_by_name() {
        let s = Skin::read("name: n\ntitle: t\nskin_v: 2\nthemes: [dark]\nradius_scale: 1.5\nfont_file:\n  family: Silkscreen\n").unwrap();
        assert_eq!(s.unknown_keys, ["font_file", "radius_scale"]);
        assert_eq!(s.skin_v, 2);
    }

    #[test]
    fn a_header_missing_one_of_the_four_naming_keys_is_refused_here() {
        let e = Skin::read("title: t\nskin_v: 1\nthemes: [light]\n").unwrap_err();
        assert_eq!(e.code(), "parse_error");
        assert!(e.to_string().contains("name"), "says which key: {e}");
    }

    #[test]
    fn a_document_that_is_not_yaml_is_refused_here() {
        let e = Skin::read("name: n\n  title: t\n").unwrap_err();
        assert_eq!(e.code(), "parse_error");
    }
}
