//! Whether a skin can be read.
//!
//! A skin picks its own colours, so "the accent went bright yellow and the white on it disappeared"
//! is an ordinary thing to write by accident. The author does not see it on their own screen, and
//! nobody receiving the file is going to look at every pairing — but the difference in brightness is
//! a number, so it is measured here rather than looked at.
//!
//! The measure is WCAG's contrast ratio, which runs from 1 (one colour with itself, unreadable) to
//! 21 (pure black on pure white). Text is held to AA, 4.5, and the one step below reading text to 3.
//! AAA is not asked of anybody: it would rule out a pale paper with a brown ink, which is a skin
//! somebody should be able to write. A skin that wants it says so by clearing it.
//!
//! **A reading under the floor does not turn the skin away.** What it turns away would be an author
//! trying their own work in progress, and there is no index this could be keeping anyone off. The
//! pairings that fell are named with their numbers, and the person decides.
//!
//! Only text over its own ground is measured. A line or an icon that is faint is harder to see and
//! still not unreadable, and where that line falls is the author's taste.

use crate::skin::{Side, Skin};

/// Anything read as text.
const TEXT: f64 = 4.5;
/// The step below it: a date, a count, the note under a control. Read when looked for rather than
/// read through, which is the distinction WCAG draws at 3.
const ASIDE: f64 = 3.0;

/// The pairings that are one ink over one ground, in the order they are reported.
const PAIRINGS: &[(&str, &str, f64)] = &[
    ("c-text", "c-bg", TEXT),
    ("c-text", "c-surface", TEXT),
    ("c-text", "c-sunken", TEXT),
    ("c-text-muted", "c-surface", ASIDE),
    ("c-on-accent", "c-accent", TEXT),
    ("c-on-heed", "c-heed", TEXT),
    ("c-on-stop", "c-stop", TEXT),
    ("c-on-done", "c-done", TEXT),
    ("c-pane-text", "c-pane-bg", TEXT),
];

/// The colours a file's own text is read in. Each of them lands on both grounds below, so they are
/// held as a list rather than written out twenty-six times.
const CODE_INKS: &[&str] = &[
    "c-code-attribute", "c-code-comment", "c-code-constant", "c-code-function", "c-code-heading",
    "c-code-invalid", "c-code-keyword", "c-code-number", "c-code-operator", "c-code-string",
    "c-code-tag", "c-code-type", "c-code-variable",
];

/// The two grounds a file is read on: the pane itself, and the row under the cursor.
const CODE_GROUNDS: &[&str] = &["c-surface", "c-sunken"];

/// Every colour token as this build sets it, per side, with the aliases followed to the value they
/// stand for. A skin sets a handful of names and keeps the rest, so measuring one of its pairings
/// means reading the side it did not write — which a compiled binary cannot get from the stylesheet.
///
/// Sorted, and held against `app/src/styles/tokens.css` by `guards/check-skin-vocabulary.sh`: a
/// value moved there and not here would be measured against a colour nobody is looking at.
const BASE: &[(&str, &str, &str)] = &[
    ("c-accent", "#0d7777", "#2ba6a4"),
    ("c-accent-faint", "#e9f4f4", "#0e2f2f"),
    ("c-accent-text", "#095a59", "#7fd6d4"),
    ("c-accent-weak", "#d6ecec", "#11403f"),
    ("c-ai", "#884bcf", "#b083ea"),
    ("c-bg", "#f7f6f3", "#1b1a17"),
    ("c-blocked", "#995d18", "#e0a04a"),
    ("c-brand-mark", "#000000", "#ffffff"),
    ("c-code-attribute", "#2f6aa8", "#7fb0e8"),
    ("c-code-comment", "#7a7263", "#8d8578"),
    ("c-code-constant", "#a35a24", "#e0a04a"),
    ("c-code-function", "#2f6aa8", "#7fb0e8"),
    ("c-code-heading", "#2f6aa8", "#7fb0e8"),
    ("c-code-invalid", "#c0392b", "#e06155"),
    ("c-code-keyword", "#9a4f8c", "#cf8fc4"),
    ("c-code-number", "#a35a24", "#e0a04a"),
    ("c-code-operator", "#6f6a5e", "#a8a399"),
    ("c-code-string", "#3d7a4a", "#86c98f"),
    ("c-code-tag", "#9a4f8c", "#cf8fc4"),
    ("c-code-type", "#0e7c7b", "#2ba6a4"),
    ("c-code-variable", "#23211c", "#ece9e1"),
    ("c-dec-decided", "#217a48", "#46c97e"),
    ("c-dec-draft", "#995d18", "#e0a04a"),
    ("c-dec-rejected", "#c0392b", "#e37166"),
    ("c-done", "#217a48", "#46c97e"),
    ("c-due-future", "#6f6a5e", "#a8a399"),
    ("c-due-overdue", "#c0392b", "#e37166"),
    ("c-due-today", "#c0392b", "#e37166"),
    ("c-due-tomorrow", "#995d18", "#e0a04a"),
    ("c-edge", "#91876d", "#787569"),
    ("c-git-added", "#2b9d5b", "#46c97e"),
    ("c-git-modified", "#c6791f", "#e0a04a"),
    ("c-git-untracked", "#7d7a2c", "#b6b25a"),
    ("c-heed", "#995d18", "#e0a04a"),
    ("c-hover", "#f0ede6", "#2c2b27"),
    ("c-human", "#3365d4", "#6f97ec"),
    ("c-on-accent", "#ffffff", "#0e3434"),
    ("c-on-done", "#ffffff", "#174c2e"),
    ("c-on-heed", "#ffffff", "#5a3a10"),
    ("c-on-stop", "#ffffff", "#551610"),
    ("c-pane-bg", "#242320", "#242320"),
    ("c-pane-cursor", "#2ba6a4", "#2ba6a4"),
    ("c-pane-frame", "#f0ede6", "#2c2b27"),
    ("c-pane-text", "#ece9e1", "#ece9e1"),
    ("c-plain", "#6f6a5e", "#a8a399"),
    ("c-pri-high", "#c0392b", "#e37166"),
    ("c-pri-low", "#6f6a5e", "#a8a399"),
    ("c-pri-med", "#995d18", "#e0a04a"),
    ("c-progress", "#0d7777", "#2ba6a4"),
    ("c-rule", "#beb6a3", "#514f48"),
    ("c-stop", "#c0392b", "#e37166"),
    ("c-sunken", "#f1efe9", "#1f1e1b"),
    ("c-surface", "#ffffff", "#242320"),
    ("c-text", "#23211c", "#ece9e1"),
    ("c-text-faint", "#9b958a", "#7d786e"),
    ("c-text-muted", "#6f6a5e", "#a8a399"),
    ("c-todo", "#6f6a5e", "#a8a399"),
    ("k-mail", "#4e6a7d", "#4e6a7d"),
    ("k-slack", "#611f69", "#611f69"),
];

/// What each colour is for, in one line, written beside it in a template. An author edits a value
/// they can place; a list of names is a list of guesses.
///
/// English, on both faces and in every language the application is read in. The file goes from
/// person to person as an attachment, and a template written in the language of whoever generated
/// it is a file its next reader cannot follow.
const ABOUT: &[(&str, &str)] = &[
    ("c-accent", "the way in: a link, the press that acts, the tab that is on"),
    ("c-accent-faint", "the faintest wash of the accent, for a ground"),
    ("c-accent-text", "the accent, dark enough to read as words"),
    ("c-accent-weak", "a tint of the accent, for the row that is selected"),
    ("c-ai", "the AI facet"),
    ("c-bg", "the page"),
    ("c-blocked", "a task held up"),
    ("c-code-attribute", "an attribute, in a file being read"),
    ("c-code-comment", "a comment, in a file being read"),
    ("c-code-constant", "a constant, in a file being read"),
    ("c-code-function", "a function's name, in a file being read"),
    ("c-code-heading", "a heading, in a file being read"),
    ("c-code-invalid", "something the grammar cannot place"),
    ("c-code-keyword", "a keyword, in a file being read"),
    ("c-code-number", "a number, in a file being read"),
    ("c-code-operator", "an operator, in a file being read"),
    ("c-code-string", "a string, in a file being read"),
    ("c-code-tag", "a tag, in a file being read"),
    ("c-code-type", "a type, in a file being read"),
    ("c-code-variable", "a variable, in a file being read — most of a file is this"),
    ("c-dec-decided", "a decision that was settled"),
    ("c-dec-draft", "a decision still being written"),
    ("c-dec-rejected", "a decision that was turned down"),
    ("c-done", "something that went through"),
    ("c-due-future", "a day still ahead"),
    ("c-due-overdue", "a day that went past"),
    ("c-due-today", "today"),
    ("c-due-tomorrow", "tomorrow"),
    ("c-edge", "the outline of a control: an input, a button, a checkbox"),
    ("c-git-added", "a file git has not seen before, staged"),
    ("c-git-modified", "a file git sees as changed"),
    ("c-git-untracked", "a file git is not following"),
    ("c-heed", "it moves — know this while it does"),
    ("c-hover", "laid over a ground where the pointer rests"),
    ("c-human", "the human facet"),
    ("c-on-accent", "the words on the accent"),
    ("c-on-done", "the words on a done fill"),
    ("c-on-heed", "the words on a heed fill"),
    ("c-on-stop", "the words on a stop fill"),
    ("c-pane-bg", "inside a terminal pane"),
    ("c-pane-cursor", "the cursor in a terminal pane"),
    ("c-pane-frame", "the ground a pane's frame stands on"),
    ("c-pane-text", "the text in a terminal pane"),
    ("c-plain", "nothing is asked"),
    ("c-pri-high", "high priority"),
    ("c-pri-low", "low priority"),
    ("c-pri-med", "medium priority"),
    ("c-progress", "a task in progress"),
    ("c-rule", "a separator: a row's underline, a card's edge"),
    ("c-stop", "nothing moves until a hand is put to it"),
    ("c-sunken", "a well: a code block, a field set into the page"),
    ("c-surface", "a card, standing on the page"),
    ("c-text", "what is read"),
    ("c-text-faint", "not for words: a separator mark, the pale side of an icon"),
    ("c-text-muted", "read when looked for: a date, a count, a note"),
    ("c-todo", "a task not started"),
];

/// A skin document, written out full: every colour a skin may set, on both sides, with a line
/// saying what each one is for. An author starts from the screen in front of them and edits values,
/// rather than from an empty file and a vocabulary to look up.
///
/// `from` is the skin the values are taken from — the one that is on — or `None` for this build's
/// own. Either way **every** name is written, so a file that set ten colours comes back out with
/// all of them: what the author was handed is filled in from the base, and nothing they might want
/// to change is missing from the page.
///
/// **It is not a copy.** The name it gives itself is not the name it was taken from, because a file
/// that calls itself what is already held replaces that one on the way back in.
///
/// Two kinds of name are left out. The ones a skin may set that carry no colour (the type scale, the
/// spacing, the font stacks) have nothing this table could write beside them, and are the author's
/// to add — the check says whether a name is one. The ones a skin may not set are not written at
/// all: a template that offered them would be teaching the author a line the check then drops.
pub fn template(name: &str, from: Option<&Skin>) -> String {
    let mut out = String::new();
    out.push_str(&format!("name: {name}\n"));
    out.push_str(&format!("title: {name}\n"));
    out.push_str("skin_v: 1\n");
    out.push_str("themes: [light, dark]\n");
    for side in [Side::Light, Side::Dark] {
        out.push_str(&format!("{side}:\n"));
        for entry in BASE.iter().filter(|(n, _, _)| crate::skin::OPEN.binary_search(n).is_ok()) {
            let base = match side {
                Side::Light => entry.1,
                Side::Dark => entry.2,
            };
            let value = from
                .and_then(|s| s.side(side).values.get(entry.0))
                .map(String::as_str)
                .unwrap_or(base);
            let about = ABOUT
                .binary_search_by_key(&entry.0, |(n, _)| n)
                .map(|at| ABOUT[at].1)
                .unwrap_or("");
            out.push_str(&format!("  {}: \"{}\"  # {}\n", entry.0, value, about));
        }
    }
    out
}

/// The name a template gives itself, from the name of whatever it was taken from. Not that name:
/// a file calling itself what is already held replaces it on the way back in, which is not what
/// somebody writing their own from an existing one is asking for.
pub fn template_name(from: Option<&Skin>) -> String {
    match from {
        Some(s) => format!("{}-copy", s.name),
        None => "my-skin".to_string(),
    }
}

/// One pairing, measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub side: Side,
    pub ink: &'static str,
    pub ground: &'static str,
    pub ratio: f64,
    pub floor: f64,
}

/// A colour that could not be measured, and the text it was written as. This build reads `#rgb` and
/// `#rrggbb`; a colour written any other way is reported rather than passed, so a pairing does not
/// go unmeasured with nobody told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    pub side: Side,
    pub name: &'static str,
    pub value: String,
}

/// What the measuring found. Silence is the good answer: a report with nothing in it is a skin every
/// pairing of which clears its floor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// The pairings under their floor, worst first.
    pub short: Vec<Reading>,
    /// The colours this build could not read a number from.
    pub unread: Vec<Unread>,
    /// How many pairings were measured, so "nothing fell" can be told from "nothing ran".
    pub measured: usize,
}

impl Report {
    /// Did every pairing that could be measured clear its floor?
    pub fn is_clear(&self) -> bool {
        self.short.is_empty()
    }
}

/// Measure one skin, on each side it declares.
///
/// The skin is read as it would be worn: a name it sets is its value, and a name it leaves is this
/// build's. A side it did not declare is not measured — it is not shown while that skin is on.
pub fn measure(skin: &Skin) -> Report {
    let mut report = Report::default();
    for side in [Side::Light, Side::Dark] {
        if !skin.themes.iter().any(|w| w == side.as_str()) {
            continue;
        }
        let pairs = PAIRINGS.iter().copied().chain(
            CODE_INKS
                .iter()
                .flat_map(|ink| CODE_GROUNDS.iter().map(move |ground| (*ink, *ground, TEXT))),
        );
        for (ink, ground, floor) in pairs {
            let (Some(a), Some(b)) = (
                colour(skin, side, ink, &mut report),
                colour(skin, side, ground, &mut report),
            ) else {
                continue;
            };
            report.measured += 1;
            let ratio = contrast(a, b);
            if ratio < floor {
                report.short.push(Reading { side, ink, ground, ratio, floor });
            }
        }
    }
    report.short.sort_by(|a, b| a.ratio.total_cmp(&b.ratio));
    report
}

/// One token's value as the skin would wear it, or `None` with the reason recorded.
fn colour(skin: &Skin, side: Side, name: &'static str, report: &mut Report) -> Option<[f64; 3]> {
    let written = skin.side(side).values.get(name).map(String::as_str);
    let value = written.unwrap_or_else(|| base(name, side));
    match channels(value) {
        Some(c) => Some(c),
        None => {
            let unread = Unread { side, name, value: value.to_string() };
            if !report.unread.contains(&unread) {
                report.unread.push(unread);
            }
            None
        }
    }
}

/// This build's value for one token, on one side.
fn base(name: &str, side: Side) -> &'static str {
    let at = BASE
        .binary_search_by_key(&name, |(n, _, _)| n)
        .unwrap_or_else(|_| panic!("{name} is measured but is not in the base palette"));
    match side {
        Side::Light => BASE[at].1,
        Side::Dark => BASE[at].2,
    }
}

/// The three channels of `#rgb` or `#rrggbb`, 0 to 1. Everything a skin may write is text, so the
/// length is counted in bytes only once the string is known to be ASCII, three characters of
/// another script being six of them.
fn channels(value: &str) -> Option<[f64; 3]> {
    let hex = value.trim().strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    let byte = |s: &str| u8::from_str_radix(s, 16).ok().map(|n| f64::from(n) / 255.0);
    match hex.len() {
        3 => {
            let d: Vec<char> = hex.chars().collect();
            Some([
                byte(&format!("{0}{0}", d[0]))?,
                byte(&format!("{0}{0}", d[1]))?,
                byte(&format!("{0}{0}", d[2]))?,
            ])
        }
        6 => Some([byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?]),
        _ => None,
    }
}

/// Relative luminance, as WCAG 2.x defines it.
fn luminance(c: [f64; 3]) -> f64 {
    let linear = |v: f64| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2])
}

/// The contrast ratio between two colours, in either order.
fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A skin that sets the given lines on the given sides.
    fn skin(themes: &str, body: &str) -> Skin {
        Skin::read(&format!("name: n\ntitle: t\nskin_v: 1\nthemes: {themes}\n{body}")).unwrap()
    }

    #[test]
    fn the_base_palette_is_in_order_and_holds_every_name_that_is_measured() {
        let mut sorted = BASE.to_vec();
        sorted.sort_unstable_by_key(|(n, _, _)| *n);
        assert_eq!(BASE, sorted, "BASE is in order");
        let named = PAIRINGS
            .iter()
            .flat_map(|(ink, ground, _)| [*ink, *ground])
            .chain(CODE_INKS.iter().copied())
            .chain(CODE_GROUNDS.iter().copied());
        for name in named {
            assert!(base(name, Side::Light).starts_with('#'), "{name} has a base value");
        }
    }

    #[test]
    fn nothing_but_a_files_own_colours_falls_short_in_this_builds_palette() {
        // Not a test of the skin path: it is this build's palette, measured through the pairings
        // declared above. Every one of them is a number that can be read, and every pairing outside
        // the thirteen a file's text is drawn in clears its floor.
        //
        // Those thirteen sit between 4.14 and 4.50 against the sunken ground and the dark surface —
        // short of AA by a hair, and short of it before this file existed: the guard over the
        // palette measures the reading inks and leaves a file's own colours out. Moving four values
        // is a change to what the screen looks like, so it is one of its own, and the line here is
        // held at everything else.
        let report = measure(&skin("[light, dark]", "light:\n  c-bg: \"#f7f6f3\"\ndark:\n  c-bg: \"#1b1a17\"\n"));
        assert_eq!(report.measured, 2 * (PAIRINGS.len() + CODE_INKS.len() * CODE_GROUNDS.len()));
        assert!(report.unread.is_empty(), "{:?}", report.unread);
        let outside: Vec<_> =
            report.short.iter().filter(|r| !r.ink.starts_with("c-code-")).collect();
        assert!(outside.is_empty(), "{outside:?}");
    }

    #[test]
    fn an_ink_that_disappears_into_its_own_fill_is_named_with_its_number() {
        // The case the spec opens with: the accent goes bright and the white on it goes with it.
        let report = measure(&skin("[light]", "light:\n  c-accent: \"#ffee00\"\n"));
        let fell: Vec<_> = report
            .short
            .iter()
            .filter(|r| r.ink == "c-on-accent")
            .collect();
        assert_eq!(fell.len(), 1, "{:?}", report.short);
        assert_eq!(fell[0].ground, "c-accent");
        assert_eq!(fell[0].floor, TEXT);
        assert!(fell[0].ratio < 1.3, "white on that yellow reads 1.1-ish: {}", fell[0].ratio);
    }

    #[test]
    fn a_side_the_skin_did_not_declare_is_not_measured() {
        let report = measure(&skin("[light]", "light:\n  c-bg: \"#fff\"\n"));
        assert_eq!(report.measured, PAIRINGS.len() + CODE_INKS.len() * CODE_GROUNDS.len());
        assert!(report.short.iter().all(|r| r.side == Side::Light));
    }

    #[test]
    fn the_worst_pairing_is_reported_first() {
        let report = measure(&skin("[light]", "light:\n  c-text: \"#f8f7f4\"\n  c-text-muted: \"#e0ddd6\"\n"));
        assert!(report.short.len() >= 2, "{:?}", report.short);
        for pair in report.short.windows(2) {
            assert!(pair[0].ratio <= pair[1].ratio, "{:?}", report.short);
        }
    }

    #[test]
    fn a_colour_this_build_cannot_read_a_number_from_is_reported_rather_than_skipped() {
        let report = measure(&skin("[light]", "light:\n  c-bg: rebeccapurple\n"));
        assert_eq!(
            report.unread,
            [Unread { side: Side::Light, name: "c-bg", value: "rebeccapurple".into() }]
        );
        // The pairings that did not need it were still measured.
        assert!(report.measured > 0);
    }

    #[test]
    fn the_short_form_of_a_colour_is_read_as_the_long_one() {
        assert_eq!(channels("#fff"), channels("#ffffff"));
        assert_eq!(channels("#0a0"), channels("#00aa00"));
        assert_eq!(channels("#\u{65e5}\u{672c}"), None, "six bytes, two characters");
        assert_eq!(channels("#ggg"), None);
    }

    #[test]
    fn every_colour_the_template_writes_says_what_it_is_for() {
        let mut sorted = ABOUT.to_vec();
        sorted.sort_unstable_by_key(|(n, _)| *n);
        assert_eq!(ABOUT, sorted, "ABOUT is in order");
        for (name, _, _) in BASE.iter().filter(|(n, _, _)| crate::skin::OPEN.binary_search(n).is_ok()) {
            assert!(
                ABOUT.binary_search_by_key(name, |(n, _)| n).is_ok(),
                "{name} is written into a template with nothing said about it"
            );
        }
        for (name, _) in ABOUT {
            assert!(BASE.binary_search_by_key(name, |(n, _, _)| n).is_ok(), "{name} is no longer a colour");
        }
    }

    #[test]
    fn a_template_taken_from_a_skin_keeps_its_values_and_fills_in_the_rest() {
        let washi = Skin::read(
            "name: washi\ntitle: t\nskin_v: 1\nthemes: [light, dark]\nlight:\n  c-bg: \"#faf7f0\"\ndark:\n  c-bg: \"#1a1713\"\n",
        )
        .unwrap()
        .check()
        .unwrap()
        .skin;

        assert_eq!(template_name(Some(&washi)), "washi-copy", "not the name it was taken from");
        let yaml = template(&template_name(Some(&washi)), Some(&washi));
        let out = Skin::read(&yaml).unwrap().check().unwrap().skin;
        assert_eq!(out.name, "washi-copy");
        assert_eq!(out.light.values["c-bg"], "#faf7f0", "what the author set");
        assert_eq!(out.light.values["c-text"], base("c-text", Side::Light), "and the rest, filled in");
        let open_colours =
            BASE.iter().filter(|(n, _, _)| crate::skin::OPEN.binary_search(n).is_ok()).count();
        assert_eq!(out.light.values.len(), open_colours, "every colour, not the ten it set");
    }

    #[test]
    fn the_template_is_a_skin_that_reads_back_clear() {
        let yaml = template("my-skin", None);
        let taken = Skin::read(&yaml).unwrap().check().expect("the template is a skin");
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        let open_colours =
            BASE.iter().filter(|(n, _, _)| crate::skin::OPEN.binary_search(n).is_ok()).count();
        assert_eq!(taken.skin.light.values.len(), open_colours, "every colour a skin may set");
        assert_eq!(taken.skin.dark.values.len(), open_colours);
        assert!(!yaml.contains("k-slack"), "and none it may not");
        assert!(yaml.contains("# what is read"), "each one says what it is for");
        // The values are this build's, so what the template measures is what the screen measures.
        let from_template = measure(&taken.skin);
        assert!(from_template.unread.is_empty());
        assert!(from_template.short.iter().all(|r| r.ink.starts_with("c-code-")));
    }

    #[test]
    fn the_measure_runs_from_one_to_twenty_one() {
        let black = channels("#000000").unwrap();
        let white = channels("#ffffff").unwrap();
        assert!((contrast(black, white) - 21.0).abs() < 1e-9);
        assert!((contrast(white, black) - 21.0).abs() < 1e-9, "either order");
        assert!((contrast(black, black) - 1.0).abs() < 1e-9);
    }
}
