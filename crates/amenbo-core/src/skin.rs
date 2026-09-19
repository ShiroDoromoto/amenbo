//! A skin, as it comes off the file a person was handed.
//!
//! A skin is one YAML document: a header naming the skin, and a table of token values per theme
//! (`light:` / `dark:`). Nothing in it is executed and no selector can be written in it — what is
//! read here is names and values, and the `:root { … }` built from them is amenbo's own.
//!
//! Reading and judging are two steps, and they are apart on purpose. [`Skin::read`] turns the
//! document into a [`Skin`] and rules on nothing: a key this build does not know, a value that is
//! not text, a later `skin_v` and a `themes` line that disagrees with the tables are all carried out
//! by name. It fails only where there is nothing to hand on — a document that is not YAML, and a
//! header missing the four keys that say which skin this is. [`Skin::check`] then says what may be
//! worn of what was read, and a reader that had already dropped those things would leave it with
//! nothing to report.
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

/// The names a skin may set, without the leading `--` the CSS spelling carries: what the document
/// holds is the name, and the property built from it is amenbo's own.
///
/// Sorted, and held against `app/src/styles/tokens.css` by `guards/check-skin-vocabulary.sh` — a
/// token added there and named in neither list turns that guard red, which is what makes the choice
/// between the two lists one somebody makes rather than one that happens.
pub const OPEN: &[&str] = &[
    "c-accent", "c-accent-faint", "c-accent-text", "c-accent-weak", "c-ai", "c-bg", "c-blocked",
    "c-code-attribute", "c-code-comment", "c-code-constant", "c-code-function", "c-code-heading",
    "c-code-invalid", "c-code-keyword", "c-code-number", "c-code-operator", "c-code-string", "c-code-tag",
    "c-code-type", "c-code-variable", "c-dec-decided", "c-dec-draft", "c-dec-rejected", "c-done",
    "c-due-future", "c-due-overdue", "c-due-today", "c-due-tomorrow", "c-edge", "c-git-added",
    "c-git-modified", "c-git-untracked", "c-heed", "c-hover", "c-human", "c-on-accent", "c-on-done",
    "c-on-heed", "c-on-stop", "c-pane-bg", "c-pane-cursor", "c-pane-frame", "c-pane-text", "c-plain",
    "c-pri-high", "c-pri-low", "c-pri-med", "c-progress", "c-rule", "c-stop", "c-sunken", "c-surface",
    "c-text", "c-text-faint", "c-text-muted", "c-todo", "font", "font-mono", "fs-body", "fs-md", "fs-xl",
    "fs-xs", "fw-bold", "fw-medium", "fw-normal", "icon-lg", "icon-md", "icon-sm", "identicon-l",
    "identicon-s", "lh", "measure-form", "measure-prose", "r-lg", "r-md", "r-sm", "s-1", "s-2", "s-3",
    "s-4", "s-5", "s-6", "shadow-md", "shadow-sm"
];

/// The names a skin may not set, though the tokens exist. Each of them says something rather than
/// shows something. `k-slack` and `k-mail` name which service a notification goes to and
/// `c-brand-mark` is amenbo's own mark, so a skin moving one of the three would have it point at
/// something else. `sidebar-w`, `rightpane-w` and `topbar-h` are the layout's own sizes, and the
/// first two are set by a person dragging a handle — a second opinion from the skin leaves nobody
/// able to say which of the two won. `tap-min` is the short side a finger needs, which is a floor
/// rather than a taste.
pub const CLOSED: &[&str] = &[
    "c-brand-mark", "k-mail", "k-slack", "rightpane-w", "sidebar-w", "tap-min", "topbar-h"
];

/// The most a skin's name may be. It is an identifier rather than a title — the word on screen is
/// `title`, which is the author's own and in their own script — and it is also the stem of the file
/// the skin is kept under, so what it may hold is what a filename may hold on every platform amenbo
/// runs on. Lowercase ASCII, digits, `-` and `_`, opening on a letter or a digit.
pub const NAME_MAX: usize = 64;

/// May this name be a skin's? Asked in two places for one reason: the name is what the skin is kept
/// under (`<base>/skins/<name>.yaml`), so a name that is not a filename is a path somewhere else.
pub fn usable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

impl Skin {
    /// The skin kept under this name, if one is. Reading it is what lets the two versions be put
    /// side by side when a second file arrives calling itself the same thing.
    ///
    /// A name that is not usable holds nothing, rather than reaching for a file: the answer to
    /// "what is installed as `../../etc/passwd`" is nothing, and it is not a question to ask the
    /// filesystem.
    pub fn installed(paths: &crate::config::Paths, name: &str) -> Result<Option<Skin>, Error> {
        if !usable_name(name) {
            return Ok(None);
        }
        match std::fs::read_to_string(paths.skin_file(name)) {
            Ok(yaml) => Skin::read(&yaml).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::from(e)),
        }
    }
}

/// What the check made of one skin: the skin as it may be applied, and what was set aside on the
/// way. Warnings do not stop it — a skin written for a later amenbo is worn as far as it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// The skin with everything the check dropped removed, so applying it cannot reach a name the
    /// check refused.
    pub skin: Skin,
    pub warnings: Vec<Warning>,
}

/// One thing the check set aside. Each names what it dropped, so a person is told what of their file
/// did not arrive rather than only that something did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// A header key this build has no meaning for.
    UnknownHeaderKey(String),
    /// A name that is in neither vocabulary — most often a skin written for a later amenbo.
    UnknownToken { theme: Side, key: String },
    /// A name this build keeps to itself.
    ClosedToken { theme: Side, key: String },
    /// A value that did not arrive as text: a length where a colour belongs, a list, a nested map.
    NotText { theme: Side, key: String },
}

/// Why a whole skin was turned away. Four shapes, and each of them is a statement the document makes
/// about itself that this build cannot honour — which is why none of them can be narrowed to a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Written against a later vocabulary. Guessing at names that are not here would apply meanings
    /// nobody wrote.
    SkinVAhead { declared: u32, understood: u32 },
    /// A side this build does not have.
    UnknownSide(String),
    /// Declared a side and then set nothing on it. Half the screen would fall back to the base
    /// colours the moment that side is shown, which reads as a fault rather than as a skin.
    SideDeclaredEmpty(Side),
    /// Set a side it did not declare. The author says which sides they made, and a table they did
    /// not claim is one they did not say they had looked at.
    SideNotDeclared(Side),
    /// Named itself something that cannot be a filename, and so cannot be kept.
    UnusableName(String),
}

/// One of the two sides a skin may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Light,
    Dark,
}

impl Side {
    /// The word the document writes this side as.
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Light => "light",
            Side::Dark => "dark",
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Skin {
    /// Judge a skin that was read, and say what may be worn of it.
    ///
    /// Two ways out, and which one a rule takes follows from whether what is left still means what
    /// the author wrote. A name this build does not have is dropped and reported, because the rest of
    /// the file still says what it said. A statement about the whole document — the vocabulary it was
    /// written against, the sides it claims — cannot be dropped that way: what is left would be a
    /// skin nobody wrote. A name the author did not set is not a rule at all; it keeps the base value,
    /// which is what lets a skin be ten lines long.
    pub fn check(self) -> Result<Taken, Refusal> {
        if !usable_name(&self.name) {
            return Err(Refusal::UnusableName(self.name.clone()));
        }
        if self.skin_v > SKIN_V {
            return Err(Refusal::SkinVAhead { declared: self.skin_v, understood: SKIN_V });
        }

        let mut declared = Vec::new();
        for word in &self.themes {
            match word.as_str() {
                "light" => declared.push(Side::Light),
                "dark" => declared.push(Side::Dark),
                _ => return Err(Refusal::UnknownSide(word.clone())),
            }
        }

        for side in [Side::Light, Side::Dark] {
            let table = self.side(side);
            let claimed = declared.contains(&side);
            let holds = !table.values.is_empty() || !table.not_text.is_empty();
            match (claimed, holds) {
                (true, false) => return Err(Refusal::SideDeclaredEmpty(side)),
                (false, true) => return Err(Refusal::SideNotDeclared(side)),
                _ => {}
            }
        }

        let mut warnings: Vec<Warning> = self
            .unknown_keys
            .iter()
            .map(|k| Warning::UnknownHeaderKey(k.clone()))
            .collect();

        let mut skin = self;
        skin.unknown_keys = Vec::new();
        for side in [Side::Light, Side::Dark] {
            let table = match side {
                Side::Light => &mut skin.light,
                Side::Dark => &mut skin.dark,
            };
            for key in std::mem::take(&mut table.not_text) {
                warnings.push(Warning::NotText { theme: side, key });
            }
            let mut kept = std::collections::BTreeMap::new();
            for (key, value) in std::mem::take(&mut table.values) {
                if OPEN.binary_search(&key.as_str()).is_ok() {
                    kept.insert(key, value);
                } else if CLOSED.binary_search(&key.as_str()).is_ok() {
                    warnings.push(Warning::ClosedToken { theme: side, key });
                } else {
                    warnings.push(Warning::UnknownToken { theme: side, key });
                }
            }
            table.values = kept;
        }

        Ok(Taken { skin, warnings })
    }

    /// One side's table, by name.
    pub fn side(&self, side: Side) -> &ThemeTable {
        match side {
            Side::Light => &self.light,
            Side::Dark => &self.dark,
        }
    }
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

    /// A skin document built from a header and whatever lines the test is about.
    fn doc(header: &str, body: &str) -> String {
        format!("name: n\ntitle: t\n{header}{body}")
    }

    #[test]
    fn both_lists_are_sorted_and_do_not_overlap() {
        // The check looks a name up by binary search in each, so an unsorted list would answer
        // "unknown" for a name that is right there.
        let mut sorted = OPEN.to_vec();
        sorted.sort_unstable();
        assert_eq!(OPEN, sorted, "OPEN is in order");
        let mut sorted = CLOSED.to_vec();
        sorted.sort_unstable();
        assert_eq!(CLOSED, sorted, "CLOSED is in order");
        for name in CLOSED {
            assert!(!OPEN.contains(name), "{name} is on both lists");
        }
    }

    #[test]
    fn a_skin_that_sets_what_it_may_is_taken_whole() {
        let skin = Skin::read(&doc("skin_v: 1\nthemes: [light]\n", "light:\n  c-bg: \"#fff\"\n  s-3: 10px\n"))
            .unwrap()
            .check()
            .unwrap();
        assert!(skin.warnings.is_empty(), "{:?}", skin.warnings);
        assert_eq!(skin.skin.light.values["c-bg"], "#fff");
        assert_eq!(skin.skin.light.values["s-3"], "10px");
    }

    #[test]
    fn a_name_in_neither_list_is_dropped_and_reported() {
        let taken = Skin::read(&doc(
            "skin_v: 1\nthemes: [light]\n",
            "light:\n  c-bg: \"#fff\"\n  c-sepia: \"#eee\"\n",
        ))
        .unwrap()
        .check()
        .unwrap();
        assert!(!taken.skin.light.values.contains_key("c-sepia"));
        assert_eq!(
            taken.warnings,
            [Warning::UnknownToken { theme: Side::Light, key: "c-sepia".into() }]
        );
    }

    #[test]
    fn a_name_this_build_keeps_to_itself_is_dropped_under_its_own_warning() {
        let taken = Skin::read(&doc(
            "skin_v: 1\nthemes: [dark]\n",
            "dark:\n  k-slack: \"#123456\"\n  sidebar-w: 400px\n",
        ))
        .unwrap()
        .check()
        .unwrap();
        assert!(taken.skin.dark.values.is_empty());
        assert_eq!(
            taken.warnings,
            [
                Warning::ClosedToken { theme: Side::Dark, key: "k-slack".into() },
                Warning::ClosedToken { theme: Side::Dark, key: "sidebar-w".into() },
            ]
        );
    }

    #[test]
    fn a_value_that_is_not_text_is_dropped_without_taking_the_table_with_it() {
        let taken = Skin::read(&doc(
            "skin_v: 1\nthemes: [light]\n",
            "light:\n  c-bg: \"#fff\"\n  c-text: [1, 2]\n",
        ))
        .unwrap()
        .check()
        .unwrap();
        assert_eq!(taken.skin.light.values["c-bg"], "#fff");
        assert_eq!(
            taken.warnings,
            [Warning::NotText { theme: Side::Light, key: "c-text".into() }]
        );
    }

    #[test]
    fn a_header_key_this_build_has_no_meaning_for_is_reported_and_taken_off() {
        let taken =
            Skin::read(&doc("skin_v: 1\nthemes: [light]\nradius_scale: 1.5\n", "light:\n  c-bg: \"#fff\"\n"))
                .unwrap()
                .check()
                .unwrap();
        assert_eq!(taken.warnings, [Warning::UnknownHeaderKey("radius_scale".into())]);
        assert!(taken.skin.unknown_keys.is_empty());
    }

    #[test]
    fn a_later_vocabulary_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 2\nthemes: [light]\n", "light:\n  c-bg: \"#fff\"\n"))
            .unwrap()
            .check()
            .unwrap_err();
        assert_eq!(e, Refusal::SkinVAhead { declared: 2, understood: SKIN_V });
    }

    #[test]
    fn a_side_that_was_declared_and_left_empty_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 1\nthemes: [light, dark]\n", "light:\n  c-bg: \"#fff\"\n"))
            .unwrap()
            .check()
            .unwrap_err();
        assert_eq!(e, Refusal::SideDeclaredEmpty(Side::Dark));
    }

    #[test]
    fn a_side_that_was_set_without_being_declared_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 1\nthemes: [light]\n", "light:\n  c-bg: \"#fff\"\ndark:\n  c-bg: \"#000\"\n"))
            .unwrap()
            .check()
            .unwrap_err();
        assert_eq!(e, Refusal::SideNotDeclared(Side::Dark));
    }

    #[test]
    fn a_side_this_build_does_not_have_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 1\nthemes: [light, sepia]\n", "light:\n  c-bg: \"#fff\"\n"))
            .unwrap()
            .check()
            .unwrap_err();
        assert_eq!(e, Refusal::UnknownSide("sepia".into()));
    }

    #[test]
    fn a_name_that_cannot_be_a_filename_turns_the_whole_skin_away() {
        for bad in ["../evil", "was/hi", "Washi", "-washi", "", &"w".repeat(NAME_MAX + 1)] {
            assert!(!usable_name(bad), "{bad:?} is not a name a skin may be kept under");
        }
        for good in ["washi", "high-contrast", "retro_game", "8bit"] {
            assert!(usable_name(good), "{good:?} is");
        }
        let e = Skin::read("name: ../evil\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\n")
            .unwrap()
            .check()
            .unwrap_err();
        assert_eq!(e, Refusal::UnusableName("../evil".into()));
    }

    #[test]
    fn a_side_that_holds_only_values_the_check_drops_still_counts_as_set() {
        // The refusal is about what the author claimed to have made, not about what survived the
        // check: a dark table of nothing but names this build dropped was still written.
        let taken = Skin::read(&doc(
            "skin_v: 1\nthemes: [light, dark]\n",
            "light:\n  c-bg: \"#fff\"\ndark:\n  c-sepia: \"#000\"\n",
        ))
        .unwrap()
        .check()
        .unwrap();
        assert!(taken.skin.dark.values.is_empty());
        assert_eq!(
            taken.warnings,
            [Warning::UnknownToken { theme: Side::Dark, key: "c-sepia".into() }]
        );
    }
}
