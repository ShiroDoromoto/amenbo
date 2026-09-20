//! A skin, as it comes off the file a person was handed.
//!
//! A skin is one YAML document: a header naming the skin, and a table of token values per theme
//! (`light:` / `dark:`). Nothing in it is executed and no selector can be written in it — what is
//! read here is names and values, and the `:root { … }` built from them is amenbo's own.
//!
//! **One skin is one file, in one of two shapes** (`AMB-D-936`): a zip holding that document as
//! `skin.yaml` beside the skin's materials, or the document on its own. [`document`] is where the
//! two come back together, and everything after it reads the one text. A zip is never unpacked to
//! disk — it is kept as it arrived and opened each time, so what is written out again is the
//! author's own file rather than one rebuilt from what was parsed.
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
    /// The name shown on screen where `titles` has nothing for the reader's language: the
    /// author's own word for their work, and the one every skin has.
    pub title: String,
    /// The name per language, where the author wrote any — language code to the name in that
    /// language (`AMB-D-935`). Which one a reader is shown is the window's call (`AMB-D-396`); the
    /// CLI stays on `title` (`AMB-D-11`). Carried as read, so a code this build has never heard of
    /// is here too — the window looks its own language up rather than walking the map.
    pub titles: BTreeMap<String, String>,
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
    /// The one font a skin may carry, as the document writes it. A pixel face is a look the name
    /// stack cannot reach: pointing at a family the reader does not have leaves the author's screen
    /// and theirs as different pictures, so the bytes travel with the colours.
    ///
    /// One, and no weights. Bold is synthesised. A second would raise the question of whether the
    /// size limit is per font or for the pair, and there is nothing a second buys that answers it.
    pub font: Option<FontFile>,
    /// The pictures a skin lays behind its surfaces, by the place each one goes (`AMB-D-936`).
    /// Held as the document writes them — the check is what rules on the place, the words and the
    /// name, for the reason the tables are held as written.
    ///
    /// One picture per place and none per side. What a background is for is the material a skin is
    /// made of, and a skin that draws paper draws paper on both sides of it.
    pub backgrounds: BTreeMap<String, Background>,
    /// The drawings a skin puts in place of this build's own, by the name of the icon each one
    /// stands in for (`AMB-D-937`). Held as the document writes them, for the reason the
    /// backgrounds are.
    ///
    /// One file per name, and a name the author left out keeps the drawing `Icon.tsx` holds. There
    /// is no stroke beside it: the fifty-one drawings are each adjusted against a line 1.75 wide,
    /// and a skin that could move that would move it under the ones it did not replace.
    pub icons: BTreeMap<String, String>,
    /// The header keys this build does not know, in order. A skin written for a later amenbo is read
    /// as far as it goes, so these are carried out to be warned about rather than to refuse on.
    pub unknown_keys: Vec<String>,
}

/// A skin's one embedded font, as it comes off the document. Held as written — the check is what
/// decodes it and rules on it, for the reason the tables are held as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct FontFile {
    /// The name the generated `@font-face` is given, and the one `font` puts at the head of its
    /// stack.
    #[serde(default)]
    pub family: String,
    /// `woff2`, and nothing else is taken.
    #[serde(default)]
    pub format: String,
    /// The file the face is in, by the name it has beside `skin.yaml` in the zip (`AMB-D-936`).
    /// A bare document has nothing beside it, so a name written in one has nothing behind it.
    #[serde(default)]
    pub file: String,
    /// The licence's name, for the line beside the skin.
    #[serde(default)]
    pub license: String,
    /// The licence in full. OFL asks that it travel with the font, and a skin is what the font
    /// travels in — so this is not a field that may be left out.
    #[serde(default)]
    pub license_text: String,
}

/// One picture a skin lays behind one of its surfaces, as it comes off the document.
///
/// The author writes a filename and, if they want them, two words. **They do not write `url()`** —
/// the place the picture is served from is amenbo's own, and a document that could spell an address
/// could spell one that is not on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Background {
    /// The file inside the skin's zip, by the name it has in there.
    #[serde(default)]
    pub file: String,
    /// How the picture is laid over its place — one of [`FITS`]. [`FIT_DEFAULT`] where the author
    /// wrote none.
    #[serde(default)]
    pub fit: String,
    /// Where in its place the picture sits — one of [`SPOTS`]. [`SPOT_DEFAULT`] where the author
    /// wrote none.
    #[serde(default)]
    pub at: String,
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
    /// The multipliers the author asked for, as the check settled them — the number it used, which
    /// is what they wrote unless it was brought inside the range.
    ///
    /// Held beside `values` rather than in it. A multiplier is not a token: what goes into the
    /// `:root` the window builds is the sizes it moved, and a declaration nobody reads would sit
    /// there for as long as the skin is on. But applying it is also the end of it — the number is
    /// nowhere in the sizes it produced — and a template written from an applied skin would hand
    /// the author back a ladder they did not write.
    pub scales: BTreeMap<String, String>,
}

/// Keep the entries of a header map that arrived as text, by name.
///
/// A value that is a number, a list or a map is dropped rather than refused, for the reason the
/// tables drop one: a document is read as far as it goes. It is not carried out to be warned about
/// the way a token's is, because what a dropped name costs here is one language's spelling of the
/// title, and `title` is standing behind it.
fn text_only(raw: BTreeMap<String, Value>) -> BTreeMap<String, String> {
    raw.into_iter().filter_map(|(key, value)| Some((key, value.as_str()?.to_string()))).collect()
}

/// Keep the backgrounds that arrived as a map of words, by the place each is named for.
///
/// An entry written as something else — a bare filename where a map belongs, a `file:` that is a
/// number — comes out as a background with nothing in it, which the check then drops by name. Read
/// as far as it goes rather than failing the document, the way a table's odd line is.
fn laid_out(raw: BTreeMap<String, Value>) -> BTreeMap<String, Background> {
    raw.into_iter()
        .map(|(place, value)| {
            (place, serde_norway::from_value::<Background>(value).unwrap_or_default())
        })
        .collect()
}

/// The file a material names, or why there is none to take. Asked of a background and of an icon
/// alike: what can be wrong with a filename does not depend on where in the document it was
/// written.
fn material_file(wrote: &str) -> Result<String, MaterialProblem> {
    let file = wrote.trim().to_string();
    if file.is_empty() {
        return Err(MaterialProblem::NoFile);
    }
    if !usable_file(&file) {
        return Err(MaterialProblem::UnusableFile);
    }
    Ok(file)
}

/// Keep the icons that arrived, by the name of the icon each one stands in for.
///
/// An entry written as something other than a filename — a map, a list, a number — comes out with
/// no file behind it, which the check then drops by the icon it was written for. Read as far as it
/// goes rather than failing the document, the way a background's odd line is.
fn drawn_for(raw: BTreeMap<String, Value>) -> BTreeMap<String, String> {
    raw.into_iter()
        .map(|(icon, value)| (icon, value.as_str().unwrap_or_default().to_string()))
        .collect()
}

impl Skin {
    /// Read one skin document. The header's four naming keys are required; everything else is
    /// optional, because a skin that sets ten colours and leaves the rest is the ordinary case.
    pub fn read(yaml: &str) -> Result<Skin, Error> {
        let w: Wire = serde_norway::from_str(yaml)?;
        Ok(Skin {
            name: w.name,
            title: w.title,
            titles: text_only(w.titles.unwrap_or_default()),
            author: w.author,
            version: w.version,
            skin_v: w.skin_v,
            themes: w.themes,
            license: w.license,
            homepage: w.homepage,
            light: ThemeTable::split(w.light.unwrap_or_default()),
            dark: ThemeTable::split(w.dark.unwrap_or_default()),
            font: w.font_file,
            backgrounds: laid_out(w.backgrounds.unwrap_or_default()),
            icons: drawn_for(w.icons.unwrap_or_default()),
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
        ThemeTable { values, not_text, scales: BTreeMap::new() }
    }
}

/// Put one family's multiplier to work: every token it moves, written out at this build's value
/// times the number, into the table the skin is applied from.
///
/// A number outside what was measured is brought back inside it and reported, the way a frame's
/// width is — refusing would leave the author with a screen that did not change and no reason
/// given. A value that is not a number at all is dropped: there is nothing to bring inside.
fn scale(
    side: Side,
    key: &str,
    wrote: &str,
    moves: &[&str],
    into: &mut BTreeMap<String, String>,
    asked_for: &mut BTreeMap<String, String>,
    warnings: &mut Vec<Warning>,
) {
    // Only text that is no number at all is dropped. A number outside what the family allows —
    // below its floor, above its ceiling, or negative — is brought inside and reported with both
    // values, because the author wrote something this build understood and is owed the answer.
    let Some(asked) = wrote.trim().parse::<f32>().ok().filter(|n| n.is_finite()) else {
        warnings.push(Warning::Scale {
            theme: side,
            key: key.to_string(),
            wrote: wrote.to_string(),
            used: None,
        });
        return;
    };
    let floor = if SCALE_TO_ZERO.contains(&key) { 0.0 } else { SCALE_MIN };
    let ceiling = if key == SCALE_UNBOUNDED { f32::INFINITY } else { SCALE_MAX };
    let used = asked.clamp(floor, ceiling);
    asked_for.insert(key.to_string(), format!("{used}"));
    if used != asked {
        warnings.push(Warning::Scale {
            theme: side,
            key: key.to_string(),
            wrote: wrote.to_string(),
            used: Some(format!("{used}")),
        });
    }
    for name in moves {
        let Some(at) = SIZED.binary_search_by_key(name, |(n, _, _)| n).ok() else {
            continue;
        };
        let base = match side {
            Side::Light => SIZED[at].1,
            Side::Dark => SIZED[at].2,
        };
        into.insert((*name).to_string(), lengths_times(base, used));
    }
}

/// One token's value with every length in it multiplied. A radius is one length; a shadow is three
/// and a colour, and the colour is left exactly as it was.
///
/// No rounding. A ladder rounded to whole pixels collapses a step at the small end, and a webview
/// draws a fraction of a pixel perfectly well.
fn lengths_times(value: &str, by: f32) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i > start && value[i..].starts_with("px") {
            if let Ok(n) = value[start..i].parse::<f32>() {
                let scaled = n * by;
                // Trailing zeros make `12px` read as `12.00px`, which is the same length written
                // as if somebody had thought about it.
                let text = format!("{scaled:.2}");
                out.push_str(text.trim_end_matches('0').trim_end_matches('.'));
                out.push_str("px");
                i += 2;
                continue;
            }
        }
        if i > start {
            out.push_str(&value[start..i]);
            continue;
        }
        out.push(value[i..].chars().next().unwrap_or(' '));
        i += value[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// Hold one side's frame values to what this build can draw, and say what was changed.
///
/// Two names and three rules. A style outside the three is dropped. A width that is not a length,
/// or is past the cap, is put back inside it. And a `double` frame is drawn at the one width that
/// splits on every engine measured, whatever the author asked for — which is the rule that cannot
/// be left to the author, because the widths that work are not a range but a range with a hole in
/// it, and the one that looks most sensible to write is inside the hole.
fn frame(side: Side, table: &mut ThemeTable, warnings: &mut Vec<Warning>) {
    if let Some(style) = table.values.get("border-style") {
        if !BORDER_STYLES.contains(&style.trim()) {
            let wrote = table.values.remove("border-style").unwrap_or_default();
            warnings.push(Warning::Frame { theme: side, key: "border-style", wrote, used: None });
        }
    }
    let doubled = table.values.get("border-style").is_some_and(|s| s.trim() == "double");

    if let Some(wrote) = table.values.get("border-w").cloned() {
        let held = match px(&wrote) {
            None => None,
            Some(n) if n < 0.0 => Some("0".to_string()),
            Some(n) if n > BORDER_W_MAX_PX => Some(format!("{BORDER_W_MAX_PX}px")),
            Some(_) => Some(wrote.trim().to_string()),
        };
        match held {
            None => {
                table.values.remove("border-w");
                warnings.push(Warning::Frame { theme: side, key: "border-w", wrote, used: None });
            }
            Some(held) => {
                table.values.insert("border-w".to_string(), held.clone());
                if held != wrote.trim() {
                    warnings.push(Warning::Frame {
                        theme: side,
                        key: "border-w",
                        wrote,
                        used: Some(held),
                    });
                }
            }
        }
    }

    if doubled {
        let wrote = table
            .values
            .insert("border-w".to_string(), BORDER_W_DOUBLE.to_string())
            .unwrap_or_default();
        if wrote.trim() != BORDER_W_DOUBLE {
            warnings.push(Warning::Frame {
                theme: side,
                key: "border-w",
                wrote,
                used: Some(BORDER_W_DOUBLE.to_string()),
            });
        }
    }
}

/// Hold the smoothing to the ways this build has a drawing for, and say what was dropped.
///
/// Unlike the frame pair there is nothing to put back inside — the value is a word, not a number,
/// so a word that is not one of the three is dropped and this build's own `antialiased` stands.
///
/// `subpixel-antialiased` is not among them. macOS stopped drawing subpixel antialiasing at all,
/// and Windows does not read this property (`AMB-T-5168`), so it is a word that would draw the
/// same as `auto` everywhere it was written — which is the kind of value that teaches an author
/// something untrue about what they are holding.
fn smoothing(side: Side, table: &mut ThemeTable, warnings: &mut Vec<Warning>) {
    let Some(wrote) = table.values.get("font-smooth") else { return };
    if SMOOTHINGS.contains(&wrote.trim()) {
        return;
    }
    let wrote = table.values.remove("font-smooth").unwrap_or_default();
    warnings.push(Warning::Choice { theme: side, key: "font-smooth", wrote });
}

/// One of a background's two words, held to the short list this build has a drawing for. What the
/// author wrote where they wrote nothing, and where they wrote a word that is not one of them, is
/// the same: the way this build lays a picture. The difference is that the second is reported.
fn one_of(
    place: &str,
    key: &'static str,
    wrote: &str,
    words: &[&str],
    stands: &str,
    warnings: &mut Vec<Warning>,
) -> String {
    let said = wrote.trim();
    if said.is_empty() {
        return stands.to_string();
    }
    if words.contains(&said) {
        return said.to_string();
    }
    warnings.push(Warning::BackgroundChoice {
        place: place.to_string(),
        key,
        wrote: said.to_string(),
    });
    stands.to_string()
}

/// A CSS length in pixels, or `None` where it is not one this build can read. Everything has to say
/// `px` — it is the only unit these two are written in, and a bare `0` never reaches here anyway:
/// YAML reads it as a number, which the check has already dropped as a value that is not text.
fn px(value: &str) -> Option<f32> {
    let v = value.trim();
    v.strip_suffix("px")?.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

/// The two names a face carried in the skin can be asked for under. A skin carries one face and
/// these are the two stacks on screen, so a face named in neither is a face nothing draws.
const FONT_STACKS: [&str; 2] = ["font", "font-mono"];

/// Does this stack ask for that family?
///
/// Read entry by entry rather than by looking for the name inside the line: `"Dot"` is in
/// `"DotGothic16"` and neither is the other, and a stack is a list of names before it is a string.
/// The quotes are the author's — a family whose name has a space has to be written in them — so
/// they come off before the comparison, and the comparison ignores case the way a family name is
/// matched.
fn names_family(stack: &str, family: &str) -> bool {
    let wanted = family.trim().trim_matches(['"', '\'']).trim();
    if wanted.is_empty() {
        return false;
    }
    stack.split(',').any(|entry| {
        let entry = entry.trim().trim_matches(['"', '\'']).trim();
        entry.eq_ignore_ascii_case(wanted)
    })
}

/// The bytes of one embedded font, or why it was set aside.
///
/// The face is a file the skin carries and the document names, so what is read here comes out of
/// [`Materials`] rather than out of the text. The name is held to [`usable_file`] — the same names
/// a background's file is held to — and then looked up in what the skin carries and nowhere else,
/// so no part of this reaches the filesystem.
fn read_font(font: &FontFile, materials: &Materials) -> Result<Vec<u8>, FontProblem> {
    if font.family.trim().is_empty() {
        return Err(FontProblem::NoFamily);
    }
    if font.format.trim() != "woff2" {
        return Err(FontProblem::Format(font.format.trim().to_string()));
    }
    let named = font.file.trim();
    if named.is_empty() {
        return Err(FontProblem::NoFile);
    }
    if !usable_file(named) {
        return Err(FontProblem::UnusableFile);
    }
    let bytes = materials.read(named).ok_or(FontProblem::Missing)?;
    if bytes.len() > FONT_MAX_BYTES {
        return Err(FontProblem::TooLarge { bytes: bytes.len() });
    }
    if !bytes.starts_with(WOFF2_MAGIC) {
        return Err(FontProblem::Unreadable);
    }
    Ok(bytes)
}

/// The document as serde reads it. Separate from [`Skin`] so the shape on disk can be forgiving —
/// `light:` written with nothing under it is null, not an empty map — while the type the rest of the
/// code holds is not.
#[derive(Deserialize)]
struct Wire {
    name: String,
    title: String,
    #[serde(default)]
    titles: Option<BTreeMap<String, Value>>,
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
    #[serde(default)]
    font_file: Option<FontFile>,
    #[serde(default)]
    backgrounds: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    icons: Option<BTreeMap<String, Value>>,
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
    "border-style", "border-w", "c-accent", "c-accent-faint", "c-accent-text", "c-accent-weak", "c-ai",
    "c-bg", "c-code-attribute", "c-code-comment", "c-code-constant", "c-code-function",
    "c-code-heading", "c-code-invalid", "c-code-keyword", "c-code-number", "c-code-operator",
    "c-code-string", "c-code-tag", "c-code-type", "c-code-variable", "c-dec-decided", "c-dec-draft",
    "c-dec-rejected", "c-done", "c-due-future", "c-due-overdue", "c-due-today", "c-due-tomorrow", "c-edge",
    "c-git-added", "c-git-modified", "c-git-untracked", "c-heed", "c-hover", "c-human", "c-on-accent",
    "c-on-done", "c-on-heed", "c-on-stop", "c-pane-bg", "c-pane-cursor", "c-pane-frame", "c-pane-text",
    "c-plain", "c-pri-high", "c-pri-low", "c-pri-med", "c-rule", "c-stop", "c-sunken",
    "c-surface", "c-text", "c-text-faint", "c-text-muted", "font", "font-mono", "font-smooth",
    "fw-bold", "fw-medium", "fw-normal", "icon-lg", "icon-md", "icon-sm", "identicon-l",
    "identicon-s", "lh",
    "measure-form", "measure-prose"
];

/// The families a skin moves by a multiplier rather than by writing values, and the tokens each
/// one moves.
///
/// **The ladders are already designed** (`AMB-D-842`): four steps of text, six of spacing, three
/// of radius, each step chosen against the others. Letting an author write the steps out one by
/// one is letting them write a set that is no longer a ladder. A multiplier moves the whole family
/// and keeps every relation in it.
///
/// It is also the only way these can be held at all. A colour can be measured — a length cannot:
/// there is no reading of `2px` that says it is too small, only a screen that turns out unusable.
/// So what is bounded is the multiplier, against what was measured to still work.
pub const SCALES: &[(&str, &[&str])] = &[
    ("fs-scale", &["fs-body", "fs-md", "fs-xl", "fs-xs"]),
    ("r-scale", &["r-lg", "r-md", "r-sm"]),
    ("s-scale", &["s-1", "s-2", "s-3", "s-4", "s-5", "s-6"]),
    ("shadow-scale", &["shadow-md", "shadow-sm"]),
];

/// The multipliers a skin may ask for, on the three families that decide whether a screen holds
/// together.
///
/// Measured (`AMB-T-5109`). The top is the topbar: its height is a token a skin may not move, and
/// at 1.35 the switch inside it is cut off top and bottom. The bottom is the tab strip, whose
/// short side goes under the 24 pixels a finger needs (WCAG 2.2 SC 2.5.8).
///
/// **One ceiling for the three, not one each.** They act at the same time, and a set of separate
/// ceilings is a set somebody reaches all of at once.
///
/// The floor is not shared, because the measurement behind it is not about all three. What 0.85
/// holds off is a tab strip too short for a finger and a switch too small to read — the spacing
/// and the type. See [`SCALE_TO_ZERO`] for the two it does not apply to.
pub const SCALE_MIN: f32 = 0.85;
pub const SCALE_MAX: f32 = 1.30;

/// The multipliers with nothing under them, which may therefore be taken all the way to zero.
///
/// **A floor answers "how small before this stops working", and for these two there is no such
/// size.** A square corner is a look, not a fault, and a screen with no shadows is a flat screen
/// rather than an unusable one — neither carries a word, a target or a control the way the type
/// and the spacing do. A floor of 0.85 on them was one measurement applied where it was not taken:
/// it left `r-scale: "0"` short of square and refused `shadow-scale: "0"` outright, which is the
/// one thing an author reaching for either of them is trying to write.
pub const SCALE_TO_ZERO: &[&str] = &["r-scale", "shadow-scale"];

/// The family whose multiplier has no ceiling either. Nothing broke at three times the default: a
/// shadow that is too large is ugly rather than unusable, and there is nothing under it to cut off.
pub const SCALE_UNBOUNDED: &str = "shadow-scale";

/// What this build sets each scaled token to, per side. A multiplier needs something to multiply,
/// and a compiled binary cannot read the stylesheet.
///
/// Held against `app/src/styles/tokens.css` by `guards/check-skin-vocabulary.sh`, the way the
/// colours are.
const SIZED: &[(&str, &str, &str)] = &[
    ("fs-body", "16px", "16px"),
    ("fs-md", "14px", "14px"),
    ("fs-xl", "20px", "20px"),
    ("fs-xs", "12px", "12px"),
    ("r-lg", "12px", "12px"),
    ("r-md", "8px", "8px"),
    ("r-sm", "5px", "5px"),
    ("s-1", "4px", "4px"),
    ("s-2", "8px", "8px"),
    ("s-3", "12px", "12px"),
    ("s-4", "16px", "16px"),
    ("s-5", "24px", "24px"),
    ("s-6", "32px", "32px"),
    ("shadow-md", "0 4px 12px rgba(35, 33, 28, 0.1)", "0 4px 12px rgba(0, 0, 0, 0.5)"),
    ("shadow-sm", "0 1px 2px rgba(35, 33, 28, 0.06), 0 1px 1px rgba(35, 33, 28, 0.04)", "0 1px 2px rgba(0, 0, 0, 0.4), 0 1px 1px rgba(0, 0, 0, 0.3)"),
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

/// The widths of frame a skin may ask for. Zero is a frame taken away, which is a look; past four
/// pixels a plain rule stops reading as a frame and starts reading as a band.
pub const BORDER_W_MAX_PX: f32 = 4.0;

/// The ways a frame may be drawn. `dashed` and `dotted` are not among them: they make a frame
/// harder to read and build nothing that `solid` and `double` do not, and in this application a
/// dashed rule is already saying something of its own (`AMB-D-928`).
pub const BORDER_STYLES: &[&str] = &["double", "none", "solid"];

/// The ways the glyphs may be drawn. `none` is the one a face drawn on a grid wants; `auto` hands
/// the choice back to the machine, which is not the same as this build's own `antialiased` on
/// every engine.
pub const SMOOTHINGS: &[&str] = &["antialiased", "auto", "none"];

/// The width a `double` frame is drawn at, whatever the author wrote.
///
/// Whether `double` splits into line, gap and line is decided by width × devicePixelRatio, and the
/// widths that split on both engines measured are 3.00–3.75px and 5.00px and up. Between them, at
/// 4.00–4.75px, WebKit draws one line. Five and up is past [`BORDER_W_MAX_PX`], so what is left is
/// the lower band, and three is the bottom of it.
///
/// **The author does not get to pick inside that band.** A range with a hole in it is not a thing
/// anybody remembers, and the one who writes the cap sees a single line and has no way to know it
/// was their own number that did it.
pub const BORDER_W_DOUBLE: &str = "3px";

/// The most an embedded font may weigh.
///
/// Set where a Japanese face fits: a Latin-only pixel font is a few kilobytes, and DotGothic16 —
/// which carries kana and han — is 500,480 bytes as one woff2. **It is not a time budget.** Two
/// megabytes takes about 48ms from file to glyphs, which nobody waits on.
///
/// Tighter than [`PACK_FILE_MAX_BYTES`], which is what any one file in a skin may weigh and is set
/// where a background fits. A file past this one, named as the face, is not a face.
pub const FONT_MAX_BYTES: usize = 2 * 1024 * 1024;

/// What a woff2 file opens with. Read so that "not woff2" is what the bytes say rather than what
/// the document claims about them.
const WOFF2_MAGIC: &[u8; 4] = b"wOF2";

/// The extension a bare skin document carries. What a skin was kept as before it could carry
/// anything beside the document. **Nothing arrives under it any more** ([`arriving`]) — it is
/// read, not taken in: a device's own file from before, and an archive carrying one.
pub const FILE_EXT: &str = ".yaml";

/// The extension a packed skin carries: the document and its materials in one zip (`AMB-D-936`).
pub const PACK_EXT: &str = ".zip";

/// Both, packed first, for everything that walks the skins directory. The order is the order a name
/// is looked for in, so a device holding both shapes under one name wears the packed one.
pub const FILE_EXTS: &[&str] = &[PACK_EXT, FILE_EXT];

/// What the document is called inside a packed skin. At the root, under one name, so what a reader
/// opens the zip to is the file they edit.
pub const PACK_DOCUMENT: &str = "skin.yaml";

/// The most one file inside a packed skin may weigh, unpacked. A background drawn for a large
/// screen at two device pixels per point fits; past it the file is carrying a photograph rather
/// than a look.
pub const PACK_FILE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// The most the whole of a packed skin may weigh, unpacked. Four backgrounds at the file ceiling,
/// with the icons and a face carrying kana and han beside them.
pub const PACK_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// What a zip opens with. Read so that "packed" is what the bytes say rather than what the name
/// they arrived under claims.
const ZIP_MAGIC: &[u8; 2] = b"PK";

/// The places a skin may lay a picture behind, each named by the colour token drawn there
/// (`AMB-D-936`). Four, and they are the four surfaces the application is built out of: the window
/// behind everything, the cards on it, the wells sunk into those, and the terminal's own field.
///
/// **A place rather than a selector.** What an author reaches is the surface, not the element —
/// which is the same line the tokens are drawn on, and the reason a skin cannot be written to hide
/// a control.
pub const BACKGROUNDS: &[&str] = &["c-bg", "c-pane-bg", "c-sunken", "c-surface"];

/// The ways a picture may be laid over its place. `cover` fills it and crops, `contain` fits the
/// whole picture in, `tile` repeats it from [`SPOT_DEFAULT`] outward.
///
/// Words rather than the CSS the window writes: what these turn into is amenbo's own, and a
/// document that could write `background-size` could write the rest of the declaration too.
pub const FITS: &[&str] = &["contain", "cover", "tile"];

/// How a picture is laid where the author said nothing. `cover` is the one that leaves no gap
/// whatever shape the window is dragged to, which is the answer somebody who did not think about
/// it wants.
pub const FIT_DEFAULT: &str = "cover";

/// Where in its place a picture sits. The nine a background has anywhere else, written as one word
/// each so that a value is a value rather than a pair to be parsed.
pub const SPOTS: &[&str] = &[
    "bottom", "bottom-left", "bottom-right", "center", "left", "right", "top", "top-left",
    "top-right",
];

/// Where a picture sits where the author said nothing.
pub const SPOT_DEFAULT: &str = "center";

/// The icons a skin may put its own drawing in place of, by the name each one is drawn under
/// (`AMB-D-937`). Fifty-one, and they are the whole set the window draws — a name the document
/// leaves out keeps this build's own drawing, so a skin replaces as few of them as it likes.
///
/// Sorted, and held against `IconName` in `app/src/components/Icon.tsx` by
/// `app/src/core/skin.test.ts` — an icon added there and not here is one a skin is told it may not
/// replace, which is not what happened.
pub const ICONS: &[&str] = &[
    "activity", "arrowDown", "arrowUp", "bell", "blocked", "calendar", "check", "checkSquare",
    "chevronDown", "chevronLeft", "chevronRight", "clipboard", "clock", "close", "comment",
    "document", "dot", "error", "foldLeft", "foldRight", "folder", "gavel", "gear", "goose",
    "hourglass", "inbox", "keyboard", "link", "menu", "more", "newWindow", "paneAcross",
    "paneDown", "paperclip", "pause", "pencil", "person", "pin", "plug", "plus", "refresh",
    "reorder", "reply", "robot", "rocket", "search", "stop", "tag", "trash", "unlock", "warning",
];

/// The most a material's name may be. Long enough for a folder and a filename inside the zip, and
/// short of a name carrying something other than a name.
pub const FILE_NAME_MAX: usize = 255;

/// What a material's name may not hold. It is written into the address the window fetches the file
/// by and into the `url()` that lays it, so out go the quote marks, the two characters a URL is cut
/// at, and everything [`NOT_IN_A_VALUE`] keeps out of a declaration.
pub const NOT_IN_A_FILE: &[char] =
    &['"', '\'', '#', '?', ';', '{', '}', '<', '>', '\\', '\n', '\r'];

/// The most a token's value may be. Counted the way the applying side counts it — `VALUE` in
/// `app/src/core/skin.ts` is a JavaScript regular expression, so its `{1,512}` counts UTF-16 code
/// units and a character outside the BMP counts as two. Long enough for a font stack naming a dozen
/// families, and short of a value that is carrying something other than a value.
pub const VALUE_MAX: usize = 512;

/// What a token's value may not hold: the punctuation that ends a declaration or opens a rule, the
/// backslash, and a newline. None of them has a use in a colour, a length, a font stack or a
/// keyword, and each is a way out of the declaration in an engine that counts brackets loosely.
pub const NOT_IN_A_VALUE: &[char] = &[';', '{', '}', '<', '>', '\\', '\n', '\r'];

/// Why a value is not one a token may be set to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueProblem {
    /// Nothing was written. A declaration with no value is not one, and the name is better left at
    /// this build's own than set to nothing.
    Empty,
    /// Holds one of [`NOT_IN_A_VALUE`].
    Punctuation(char),
    /// Holds a comment delimiter, which would take whatever follows it into the comment.
    Comment,
    /// Reaches for `url(`. No token here takes one, and a skin is a file somebody was handed: it
    /// says what colour a thing is, and it does not fetch from the reader's machine.
    Url,
    /// Longer than [`VALUE_MAX`].
    TooLong { units: usize },
}

impl ValueProblem {
    /// Why the value was dropped, as a phrase that follows "the value". English on both faces, the
    /// way a refusal is: a person told it in the terminal and a person shown it in the window are
    /// told the same thing.
    ///
    /// **The value itself is not in it.** It is the one string in the file written to get out of a
    /// declaration, and the terminal is a place where a string can do more than be read.
    pub fn en(&self) -> String {
        match self {
            ValueProblem::Empty => "is empty".to_string(),
            ValueProblem::Punctuation(c) => {
                let named = match c {
                    '\n' => "a newline".to_string(),
                    '\r' => "a carriage return".to_string(),
                    c => format!("'{c}'"),
                };
                format!("holds {named}, which no value may")
            }
            ValueProblem::Comment => "holds a comment delimiter".to_string(),
            ValueProblem::Url => "reaches for url(), and no token here takes one".to_string(),
            ValueProblem::TooLong { units } => {
                format!("is {units} characters, over the {VALUE_MAX} a value may be")
            }
        }
    }
}

/// May a token be set to this value? Asked in two places for one reason: this build drops what it
/// will not pass on, and the window that wears a skin asks again at the moment it writes the
/// declaration (`usable` in `app/src/core/skin.ts`). Neither can stand alone — the window is handed
/// a table over IPC and reads it where it arrives, and a value dropped there with nothing said is a
/// value the author never hears about. So the rule is written twice and held together by
/// `app/src/core/skin.test.ts`, which reads both spellings out of the tree.
pub fn usable_value(value: &str) -> Result<(), ValueProblem> {
    if value.is_empty() {
        return Err(ValueProblem::Empty);
    }
    let units: usize = value.chars().map(char::len_utf16).sum();
    if units > VALUE_MAX {
        return Err(ValueProblem::TooLong { units });
    }
    if let Some(c) = value.chars().find(|c| NOT_IN_A_VALUE.contains(c)) {
        return Err(ValueProblem::Punctuation(c));
    }
    if value.contains("/*") || value.contains("*/") {
        return Err(ValueProblem::Comment);
    }
    if reaches_for_url(value) {
        return Err(ValueProblem::Url);
    }
    Ok(())
}

/// Does the value reach for `url(`? The applying side asks `/url\s*\(/i`, so the word is found
/// whatever its case and the bracket is allowed to sit a space away from it.
fn reaches_for_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.match_indices("url").any(|(at, _)| lower[at + 3..].trim_start().starts_with('('))
}

/// How a skin's bytes are packed. One skin is one file either way — what differs is whether that
/// file is the document or a zip carrying it (`AMB-D-936`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Packing {
    /// The document on its own.
    Bare,
    /// A zip holding [`PACK_DOCUMENT`] and the skin's materials.
    Packed,
}

impl Packing {
    /// What the bytes say they are. The magic rather than the name they arrived under: a person
    /// renames a file, and what is inside it does not change with the name.
    pub fn of(bytes: &[u8]) -> Packing {
        if bytes.starts_with(ZIP_MAGIC) {
            Packing::Packed
        } else {
            Packing::Bare
        }
    }

    /// The extension a skin packed this way is kept under.
    pub fn ext(self) -> &'static str {
        match self {
            Packing::Bare => FILE_EXT,
            Packing::Packed => PACK_EXT,
        }
    }
}

/// The document out of a file arriving from outside — what `skin add` is handed, and what is
/// dropped on the panel. **A skin arrives as a zip and in no other shape** (`AMB-D-936`): the bare
/// document is turned away here, with the way to hand it over again.
///
/// Turned away rather than read, though [`document`] would read it: two shapes coming in is two of
/// everything behind the door — the check, the writing back out, the reading of materials — and
/// only one of the two can carry a material at all.
///
/// Weighed here rather than at each face, so no face can be the one that forgot: what a packed skin
/// unpacks to is the one thing about it that costs something to find out, and a file that will not
/// fit is not one to go on parsing.
pub fn arriving(bytes: &[u8]) -> Result<String, Error> {
    if Packing::of(bytes) == Packing::Bare {
        return Err(Error::invalid(format!(
            "a skin arrives as a zip, and this file is the document on its own. \
             Put it in a zip as {PACK_DOCUMENT} at the root, beside the materials it names, \
             and hand that zip over."
        )));
    }
    weigh(bytes)?;
    Ok(document(bytes)?.1)
}

/// The skin document out of whatever a file holds — the bytes themselves where they are the
/// document, and [`PACK_DOCUMENT`] out of the zip where they are a zip.
///
/// **The other entries are not unpacked here.** This runs every time a skin is read, including at
/// startup in every window, so what it costs has to be the document and not the archive. What it
/// does read of them is their names, which cost nothing and are the half that can point outside
/// the directory. The weights are read from the index, which a hostile file can lie about — the
/// answer to that is [`weigh`], which unpacks and counts, and runs once on the way in.
pub fn document(bytes: &[u8]) -> Result<(Packing, String), Error> {
    if Packing::of(bytes) == Packing::Bare {
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| Error::invalid("this file is neither a zip nor text"))?;
        return Ok((Packing::Bare, text));
    }
    let mut zip = open_pack(bytes)?;
    for at in 0..zip.len() {
        let entry = zip.by_index(at).map_err(unreadable_pack)?;
        held_in_the_pack(entry.name())?;
        if entry.size() > PACK_FILE_MAX_BYTES {
            return Err(too_heavy(entry.name(), entry.size(), PACK_FILE_MAX_BYTES));
        }
    }
    let mut found = zip.by_name(PACK_DOCUMENT).map_err(|_| {
        Error::invalid(format!("this zip has no {PACK_DOCUMENT} — that is the skin itself"))
    })?;
    let mut text = String::new();
    // Capped on the way out as well as by the index: what `size` reports is the file's own claim
    // about itself, and this is the one entry unpacked before anything has weighed it.
    use std::io::Read as _;
    (&mut found)
        .take(PACK_FILE_MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| Error::invalid(format!("{PACK_DOCUMENT} in this zip is not text")))?;
    if text.len() as u64 > PACK_FILE_MAX_BYTES {
        return Err(too_heavy(PACK_DOCUMENT, text.len() as u64, PACK_FILE_MAX_BYTES));
    }
    Ok((Packing::Packed, text))
}

/// Unpack the whole of a packed skin and count what comes out, so a file that unpacks to more than
/// it says is turned away at the door rather than on the day something reads it.
///
/// Counted while unpacking, entry by entry, and stopped at the ceiling — the point is a file whose
/// index says a few kilobytes and whose contents are gigabytes, so the number that decides is the
/// one coming out of the decoder, never the one in the header. Answers with the total.
fn weigh(bytes: &[u8]) -> Result<u64, Error> {
    use std::io::Read as _;
    let mut zip = open_pack(bytes)?;
    let mut total = 0u64;
    let mut sink = [0u8; 64 * 1024];
    for at in 0..zip.len() {
        let mut entry = zip.by_index(at).map_err(unreadable_pack)?;
        let name = entry.name().to_string();
        held_in_the_pack(&name)?;
        let mut weighs = 0u64;
        loop {
            let read = entry
                .read(&mut sink)
                .map_err(|_| Error::invalid(format!("'{name}' in this zip will not unpack")))?;
            if read == 0 {
                break;
            }
            weighs += read as u64;
            total += read as u64;
            if weighs > PACK_FILE_MAX_BYTES {
                return Err(too_heavy(&name, weighs, PACK_FILE_MAX_BYTES));
            }
            if total > PACK_MAX_BYTES {
                return Err(too_heavy("this zip", total, PACK_MAX_BYTES));
            }
        }
    }
    Ok(total)
}

/// One document, packed as a skin file — a zip holding it as [`PACK_DOCUMENT`] and nothing else.
///
/// What amenbo writes out when it is the one making the skin rather than taking one: the template.
/// A skin the device already holds is written out by copying its file, never by packing it again —
/// that file is the author's own, materials and all, and rebuilding it would hand back less than
/// arrived.
pub fn pack_document(yaml: &str) -> Result<Vec<u8>, Error> {
    use std::io::Write as _;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let how =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let packed = (|| -> Result<Vec<u8>, zip::result::ZipError> {
        out.start_file(PACK_DOCUMENT, how)?;
        out.write_all(yaml.as_bytes())?;
        Ok(out.finish()?.into_inner())
    })();
    packed.map_err(|e| Error::invalid(format!("this skin would not pack: {e}")))
}

/// The zip, open, or why it is not one this build can read.
fn open_pack(bytes: &[u8]) -> Result<zip::ZipArchive<std::io::Cursor<&[u8]>>, Error> {
    zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(unreadable_pack)
}

fn unreadable_pack(e: zip::result::ZipError) -> Error {
    Error::invalid(format!("this zip will not open: {e}"))
}

fn too_heavy(what: &str, weighs: u64, ceiling: u64) -> Error {
    Error::invalid(format!("'{what}' unpacks to {weighs} bytes, over the {ceiling} this build takes"))
}

/// Does this entry stay inside the skin? A name reaching up out of the archive, or starting from
/// the root, is the one way a zip writes outside the directory it was unpacked into — and it is
/// refused here rather than where a material is read, so no later reader has to remember.
///
/// Refused rather than skipped: a file carrying such a name is not a skin with one odd entry in it.
fn held_in_the_pack(name: &str) -> Result<(), Error> {
    if !inside_the_pack(name) {
        return Err(Error::invalid(format!(
            "'{name}' in this zip points outside it; a skin's files sit inside the skin"
        )));
    }
    Ok(())
}

/// Does this name stay inside the skin? Reaching up out of the archive, or starting from the root,
/// is the one way a zip writes outside the directory it was unpacked into.
fn inside_the_pack(name: &str) -> bool {
    !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.split(['/', '\\']).any(|part| part == "..")
        && !std::path::Path::new(name).is_absolute()
}

/// May a material be named this? Asked of what the document points at, rather than of what the zip
/// holds: a name is followed to a file, put into an address and written into a declaration, and
/// each of those is a place a name that is not one goes somewhere of its own.
pub fn usable_file(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= FILE_NAME_MAX
        && inside_the_pack(name)
        && !name.chars().any(|c| c.is_control() || NOT_IN_A_FILE.contains(&c))
}

/// A picture a skin carries, by what its bytes open with.
///
/// Four, and each is drawn by every engine amenbo runs on. **GIF is not among them.** What it adds
/// over the other four is animation, with nothing in the document that could stop it — a moving
/// picture behind the text is not a look somebody chose once, it is one they cannot put down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picture {
    Png,
    Jpeg,
    Webp,
    /// Drawn through `<img>` and `mask-image` rather than put into the document, which is what
    /// keeps a drawing a drawing (`AMB-D-936`).
    Svg,
}

/// How far into a file the opening of an SVG is looked for. An XML declaration and a doctype can
/// stand in front of the element, and neither is long.
const SVG_HEAD: usize = 4 * 1024;

impl Picture {
    /// What these bytes are, or `None` where they are not a picture this build draws.
    ///
    /// **The bytes rather than the extension** — the name is the author's word about the file and
    /// this is the file's own.
    pub fn of(bytes: &[u8]) -> Option<Picture> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(Picture::Png);
        }
        if bytes.starts_with(b"\xff\xd8\xff") {
            return Some(Picture::Jpeg);
        }
        // `RIFF` opens a container that holds more than pictures, so the form inside it is what
        // says this one is a picture.
        if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
            return Some(Picture::Webp);
        }
        opens_an_svg(bytes).then_some(Picture::Svg)
    }

    /// The one word for this form. Shown rather than translated: `png` is `png` in every
    /// language, the way a ratio and a token name are.
    pub fn word(self) -> &'static str {
        match self {
            Picture::Png => "png",
            Picture::Jpeg => "jpeg",
            Picture::Webp => "webp",
            Picture::Svg => "svg",
        }
    }

    /// What the file is served as. The door that hands a material to the window is told the type
    /// rather than left to work it out from the name.
    pub fn mime(self) -> &'static str {
        match self {
            Picture::Png => "image/png",
            Picture::Jpeg => "image/jpeg",
            Picture::Webp => "image/webp",
            Picture::Svg => "image/svg+xml",
        }
    }
}

/// Does this file open an SVG? There is no magic number to read — an SVG is text — so what is read
/// is the opening: a byte-order mark and whitespace out of the way, whatever declaration, doctype
/// or comment stands in front of the picture, and then the first element, which has to be the
/// `svg` one.
///
/// **The first element rather than the first mention of it.** A page with an `<svg>` somewhere
/// inside it is a page, and answering "svg" for one would hand the window a document to draw.
fn opens_an_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SVG_HEAD)];
    let text = String::from_utf8_lossy(head);
    let mut rest = text.trim_start_matches('\u{feff}').trim_start();
    loop {
        let cut = if let Some(after) = rest.strip_prefix("<!--") {
            after.find("-->").map(|at| &after[at + 3..])
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest.find('>').map(|at| &rest[at + 1..])
        } else {
            break;
        };
        // Nothing closes it inside what was read, so there is no first element to look at.
        let Some(after) = cut else { return false };
        rest = after.trim_start();
    }
    let Some(after) = rest.strip_prefix("<svg") else { return false };
    after.is_empty() || after.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/')
}

/// One of a packed skin's materials, by the name the document gave it.
///
/// Unpacked here and not kept: a skin is read at startup in every window, so what is held is the
/// file it arrived in, and a material is taken out of it at the moment something draws with it.
/// Capped the way the way in was — what the index claims about an entry is not what decides.
pub fn material(pack: &[u8], name: &str) -> Result<Vec<u8>, Error> {
    if !usable_file(name) {
        return Err(Error::invalid("that is not a name a file in a skin can have"));
    }
    if Packing::of(pack) == Packing::Bare {
        return Err(Error::invalid("this skin is one document, and carries no files"));
    }
    use std::io::Read as _;
    let mut zip = open_pack(pack)?;
    let mut found =
        zip.by_name(name).map_err(|_| Error::invalid(format!("this zip holds no '{name}'")))?;
    let mut bytes = Vec::new();
    (&mut found)
        .take(PACK_FILE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::invalid(format!("'{name}' in this zip will not unpack")))?;
    if bytes.len() as u64 > PACK_FILE_MAX_BYTES {
        return Err(too_heavy(name, bytes.len() as u64, PACK_FILE_MAX_BYTES));
    }
    Ok(bytes)
}

/// One of a packed skin's materials, read as a picture — the bytes and what they turned out to be.
///
/// Where the two parts of "is this a background" are put together: the document says which file,
/// and the file says what it is.
pub fn picture(pack: &[u8], name: &str) -> Result<(Picture, Vec<u8>), Error> {
    drawn(name, material(pack, name)?)
}

/// What these bytes are as a picture, or the sentence saying they are none.
///
/// Held apart from the two that ask it, because they come at the question from the two shapes a
/// skin's materials are held in — a zip ([`picture`]) and whatever [`Materials`] has ([`icon`]) —
/// and the answer is the bytes' own either way.
fn drawn(name: &str, bytes: Vec<u8>) -> Result<(Picture, Vec<u8>), Error> {
    match Picture::of(&bytes) {
        Some(picture) => Ok((picture, bytes)),
        None => Err(Error::invalid(format!(
            "'{name}' is not a picture this build draws — png, jpeg, webp and svg are"
        ))),
    }
}

/// One of a skin's icons, read as a drawing — the bytes and what they turned out to be.
///
/// Where the two parts of "is this an icon" are put together, the way [`picture`] does it for a
/// background. **A jpeg is not one of them.** An icon is laid as a mask and what is read off it is
/// the alpha; a jpeg carries none, so one put here would draw as a filled square where the drawing
/// was.
///
/// Takes [`Materials`] rather than a zip: a skin that ships inside the build carries its files
/// beside its document rather than in an archive, and an icon of one is still an icon.
pub fn icon(materials: &Materials<'_>, name: &str) -> Result<(Picture, Vec<u8>), Error> {
    let bytes = materials
        .read(name)
        .ok_or_else(|| Error::invalid(format!("this skin holds no '{name}'")))?;
    let (drawing, bytes) = drawn(name, bytes)?;
    if drawing == Picture::Jpeg {
        return Err(Error::invalid(format!(
            "'{name}' is a jpeg, which carries no transparency — png, webp and svg are the \
             drawings an icon is laid from"
        )));
    }
    Ok((drawing, bytes))
}

/// What one of a skin's files turned out to be, read off its bytes rather than off its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    /// A picture, in the form it is in.
    Picture(Picture),
    /// A face. woff2 is the one form a skin's font is taken in, so there is no second word here.
    Face,
    /// Bytes this build has nothing to do with. Still in the file, so still counted and still
    /// shown — what a reader is deciding about is the whole file.
    Neither,
}

impl Material {
    /// The one word for what this is, or `None` where it is neither a picture nor a face. A form
    /// reads the same in every language, so what a window says around this word is the only part
    /// of the line that is translated.
    pub fn word(self) -> Option<&'static str> {
        match self {
            Material::Picture(picture) => Some(picture.word()),
            Material::Face => Some("woff2"),
            Material::Neither => None,
        }
    }
}

/// One file a packed skin carries, as somebody being handed the skin sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carried {
    /// The name it has in the zip, which is the name the document points at it by.
    pub file: String,
    /// What it weighs unpacked, counted rather than read off the index.
    pub bytes: u64,
    /// What its bytes turned out to be.
    pub is: Material,
}

/// Every file a packed skin carries beside its document, in the order the zip holds them.
///
/// **What the file holds, not what the document names.** Somebody deciding whether to take another
/// person's skin in is deciding about the whole file, and a material nothing in the document points
/// at is still a file that arrived on their machine.
///
/// Each entry is unpacked to be weighed and to be read, because the index is the file's own claim
/// about itself and the opening is what says what the bytes are. Bounded the way the way in is:
/// the head is held and the rest is counted and let go, so a skin at the ceiling costs one buffer
/// rather than 32MB.
///
/// A bare document carries nothing and answers with nothing — it is one file, and that file is the
/// document.
pub fn carries(pack: &[u8]) -> Result<Vec<Carried>, Error> {
    if Packing::of(pack) == Packing::Bare {
        return Ok(Vec::new());
    }
    use std::io::Read as _;
    let mut zip = open_pack(pack)?;
    let mut out = Vec::new();
    let mut sink = [0u8; 64 * 1024];
    for at in 0..zip.len() {
        let mut entry = zip.by_index(at).map_err(unreadable_pack)?;
        let file = entry.name().to_string();
        held_in_the_pack(&file)?;
        if file == PACK_DOCUMENT || file.ends_with('/') {
            continue;
        }
        let mut head = Vec::new();
        let mut bytes = 0u64;
        loop {
            let read = entry
                .read(&mut sink)
                .map_err(|_| Error::invalid(format!("'{file}' in this zip will not unpack")))?;
            if read == 0 {
                break;
            }
            if head.len() < SVG_HEAD {
                head.extend_from_slice(&sink[..read.min(SVG_HEAD - head.len())]);
            }
            bytes += read as u64;
            if bytes > PACK_FILE_MAX_BYTES {
                return Err(too_heavy(&file, bytes, PACK_FILE_MAX_BYTES));
            }
        }
        let is = match Picture::of(&head) {
            Some(picture) => Material::Picture(picture),
            None if head.starts_with(WOFF2_MAGIC) => Material::Face,
            None => Material::Neither,
        };
        out.push(Carried { file, bytes, is });
    }
    Ok(out)
}

/// Where the files a skin's document names are read from (`AMB-D-936`). What an author writes is a
/// filename; what stands behind that name is this.
///
/// Held as the file the skin arrived in rather than as an unpacked directory. The zip is what the
/// device keeps, so unpacking it beside itself would leave two copies of every material to keep in
/// step, and the one that went stale would be the one being drawn.
#[derive(Debug, Clone)]
pub enum Materials<'a> {
    /// Nothing beside the document. A bare skin is one file, so a name written in one has nothing
    /// behind it.
    None,
    /// The zip the skin is held in, opened again for each file read out of it.
    Pack(std::borrow::Cow<'a, [u8]>),
    /// The files a shipped skin carries, held in the binary beside its document. A shipped skin
    /// does not arrive as a zip, and what a device's own reads out of its own file, it reads here.
    Shipped(&'static [(&'static str, &'static [u8])]),
}

impl<'a> Materials<'a> {
    /// What one file's bytes carry — the zip where they are one, and nothing where they are the
    /// document on its own. Read off the bytes the way [`Packing::of`] reads them, so a caller
    /// never carries the shape alongside the file it came off.
    pub fn of(bytes: &'a [u8]) -> Materials<'a> {
        match Packing::of(bytes) {
            Packing::Bare => Materials::None,
            Packing::Packed => Materials::Pack(std::borrow::Cow::Borrowed(bytes)),
        }
    }

    /// The bytes of one file the document names, or `None` where the skin carries no such file.
    ///
    /// The two shapes answer the same question — a device's own skin out of the zip it arrived in
    /// ([`material`]), and a shipped one out of the binary — so a caller that has a filename has
    /// one place to take it.
    ///
    /// Why it fails is dropped here. What asks is the check, which says what it made of the
    /// document, and "no such file" and "the zip will not open" are the same sentence to a reader
    /// whose face did not arrive. A caller that needs the reason calls [`material`].
    pub fn read(&self, file: &str) -> Option<Vec<u8>> {
        match self {
            Materials::None => None,
            Materials::Shipped(held) => {
                held.iter().find(|(name, _)| *name == file).map(|(_, bytes)| bytes.to_vec())
            }
            Materials::Pack(bytes) => material(bytes, file).ok(),
        }
    }
}

/// The skin a file in the skins directory is, by its name, with where its shape sits in
/// [`FILE_EXTS`] — `None` where the file is not one of a skin's shapes, or is named something a
/// skin may not be called.
fn name_of_file(file: &str) -> Option<(String, usize)> {
    FILE_EXTS.iter().enumerate().find_map(|(rank, ext)| {
        let name = file.strip_suffix(ext)?;
        usable_name(name).then(|| (name.to_string(), rank))
    })
}

/// The file this device keeps a skin under, and how it is packed — `None` where it keeps none.
///
/// Both shapes are looked for, packed first: a name is one skin, and which file behind it is a
/// fact for one place to settle rather than for every caller to go asking.
pub fn kept_file(paths: &crate::config::Paths, name: &str) -> Option<(Packing, std::path::PathBuf)> {
    if !usable_name(name) {
        return None;
    }
    [Packing::Packed, Packing::Bare].into_iter().find_map(|packing| {
        let at = paths.skin_file(name, packing.ext());
        at.is_file().then_some((packing, at))
    })
}

/// May this name be a skin's? Asked in two places for one reason: the name is what the skin is kept
/// under (`<base>/skins/<name><ext>`), so a name that is not a filename is a path somewhere else.
pub fn usable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

impl Skin {
    /// Every skin this device holds, by the name its file is under, with what reading that file
    /// gave. A file that will not read is carried out as the failure rather than dropped: it is one
    /// of the person's own files, and a list that quietly skipped it would leave them looking for a
    /// skin that is right there.
    pub fn installed_all(paths: &crate::config::Paths) -> Vec<(String, Result<Skin, Error>)> {
        // What this build ships comes first, in the order it names them, and the device's own
        // follow sorted. One list rather than two: a reader choosing a skin is choosing among all
        // of them, and a second place to look is a thing to remember. The order is not
        // alphabetical because the first of them is the one somebody is looking for in a hurry.
        let mut out: Vec<(String, Result<Skin, Error>)> = crate::skin_official::OFFICIAL
            .iter()
            .map(|o| (o.name.to_string(), Skin::read(o.yaml)))
            .collect();
        let Ok(entries) = std::fs::read_dir(paths.skins_dir()) else {
            return out; // nothing of this device's own
        };
        let mut found: Vec<(String, usize, Result<Skin, Error>)> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let file = e.file_name().to_string_lossy().into_owned();
                let (name, rank) = name_of_file(&file)?;
                let read = std::fs::read(e.path())
                    .map_err(Error::from)
                    .and_then(|bytes| document(&bytes))
                    .and_then(|(_, yaml)| Skin::read(&yaml));
                Some((name, rank, read))
            })
            .collect();
        // By name, and within a name by the order `FILE_EXTS` puts the shapes in. A device holding
        // both shapes under one word shows the packed one — the same one `installed` answers with,
        // because a list with two rows under one name is offering a skin nothing can reach.
        found.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        found.dedup_by(|a, b| a.0 == b.0);
        let mut found: Vec<(String, Result<Skin, Error>)> =
            found.into_iter().map(|(name, _, read)| (name, read)).collect();
        // A file under a shipped name cannot arrive through `install`, and one put there by hand
        // is not a second entry under that name — the shipped one is what `installed` answers with,
        // and a list that showed both would be showing a skin nothing can reach.
        found.retain(|(name, _)| !crate::skin_official::is_official(name));
        out.append(&mut found);
        out
    }

    /// Keep this file on the device under `name`, replacing whatever was there. The bytes are the
    /// author's own — a skin carries a licence text, a font and its materials, and re-writing it
    /// from what was parsed would hand on a different file from the one that arrived.
    ///
    /// They land under [`PACK_EXT`], the one shape a skin arrives in ([`arriving`]). A bare file
    /// held under that name from before goes with the write, so one name is one file.
    ///
    /// Written aside and renamed into place, so a reader never sees half a skin.
    pub fn install(paths: &crate::config::Paths, name: &str, bytes: &[u8]) -> Result<(), Error> {
        if !usable_name(name) {
            return Err(Error::invalid(format!("'{name}' is not a name a skin can be kept under")));
        }
        // The shipped names are the one thing a file cannot call itself. Taking it in would leave
        // two skins under one name, and every later sentence — which one is on, which one is being
        // replaced, which one `skin use` means — would have to say which.
        if crate::skin_official::is_official(name) {
            return Err(Error::invalid(crate::skin_official::name_is_ours(name)));
        }
        let dir = paths.skins_dir();
        std::fs::create_dir_all(&dir)?;
        let dest = paths.skin_file(name, PACK_EXT);
        let tmp = dir.join(format!("{name}{PACK_EXT}.tmp"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &dest)?;
        // The other shape, where the name was held in it. Done after the rename rather than
        // before: what is being replaced stays readable until its replacement is whole.
        for other in FILE_EXTS.iter().filter(|ext| **ext != PACK_EXT) {
            let _ = std::fs::remove_file(paths.skin_file(name, other));
        }
        Ok(())
    }

    /// Take the skin kept under `name` off the device. `false` when there was none — removing what
    /// is not there is the state the caller asked for, not a failure, and the caller says so.
    pub fn uninstall(paths: &crate::config::Paths, name: &str) -> Result<bool, Error> {
        if !usable_name(name) {
            return Ok(false);
        }
        // Said rather than quietly done nothing: the skin is right there in the list, so a caller
        // told "no skin is kept as 'washi'" would go looking for a file that was never a file.
        if crate::skin_official::is_official(name) {
            return Err(Error::invalid(crate::skin_official::not_on_the_device(name)));
        }
        let Some((_, at)) = kept_file(paths, name) else {
            return Ok(false);
        };
        match std::fs::remove_file(at) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// The skin kept under this name, with what its materials are read out of. Reading it is what
    /// lets the two versions be put side by side when a second file arrives calling itself the same
    /// thing.
    ///
    /// The skin and its materials travel together because they are one file: the document is read
    /// out of the zip, and so is the face it names, and a caller handed only the first would have
    /// nothing to read the second out of.
    ///
    /// A name that is not usable holds nothing, rather than reaching for a file: the answer to
    /// "what is installed as `../../etc/passwd`" is nothing, and it is not a question to ask the
    /// filesystem.
    pub fn installed(
        paths: &crate::config::Paths,
        name: &str,
    ) -> Result<Option<(Skin, Materials<'static>)>, Error> {
        if !usable_name(name) {
            return Ok(None);
        }
        if let Some(o) = crate::skin_official::find(name) {
            return Skin::read(o.yaml).map(|s| Some((s, Materials::Shipped(o.materials))));
        }
        let Some((_, at)) = kept_file(paths, name) else {
            return Ok(None);
        };
        match std::fs::read(at) {
            Ok(bytes) => {
                let (packing, yaml) = document(&bytes)?;
                let skin = Skin::read(&yaml)?;
                let materials = match packing {
                    Packing::Bare => Materials::None,
                    Packing::Packed => Materials::Pack(std::borrow::Cow::Owned(bytes)),
                };
                Ok(Some((skin, materials)))
            }
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
    /// The embedded font's bytes, decoded, where one came through. `None` where the skin carried
    /// none and where the one it carried was set aside — which side it was is in the warnings.
    pub font: Option<Vec<u8>>,
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
    /// A value that is text and is not a shape a token may be set to. Dropped, so the name keeps
    /// this build's own value — which is what the window would do with it anyway, silently.
    UnsafeValue { theme: Side, key: String, why: ValueProblem },
    /// A family's multiplier that was not taken as written. `used` is the number put to work
    /// instead, written out, or `None` where the value was not a number and the family did not
    /// move. Written rather than held as one, so a warning stays a thing two of them can be
    /// compared for being the same.
    Scale { theme: Side, key: String, wrote: String, used: Option<String> },
    /// A frame value that was not taken as written. `used` is what was put there instead, or
    /// `None` where the name was dropped and this build's own value stands.
    Frame { theme: Side, key: &'static str, wrote: String, used: Option<String> },
    /// A value outside the short list of words this build has a drawing for. Dropped, so the name
    /// keeps this build's own value; there is nothing to put back inside, the way a number has.
    Choice { theme: Side, key: &'static str, wrote: String },
    /// The embedded font was set aside. The colours are taken either way — a look built on a face
    /// nobody can read still has its palette, and refusing the file over it would throw that away.
    FontDropped(FontProblem),
    /// The face was read and nothing asks for it: neither side writes the family into `font` or
    /// `font-mono`, so it is registered with the window and never drawn.
    ///
    /// A warning rather than a refusal, and rather than putting the name at the head of a stack
    /// nobody wrote. Which of the two stacks it belongs at the head of is the author's to say —
    /// `retro` carries one face and names it in both — and a build that chose for them would be
    /// setting a screen in a face the document does not ask for.
    FontNotNamed { family: String },
    /// A background named for a place this build does not lay one behind.
    UnknownBackground { place: String },
    /// A background that is not laid. The place keeps its colour, which is what is under every
    /// background anyway.
    BackgroundDropped { place: String, why: MaterialProblem },
    /// A background's word that is not one this build has a drawing for. Dropped, so the picture is
    /// laid the way this build lays one; there is nothing to bring inside, the way a number has.
    BackgroundChoice { place: String, key: &'static str, wrote: String },
    /// An icon named for a drawing this build does not have — most often a skin written for a
    /// later amenbo, the way an unknown token is.
    UnknownIcon { name: String },
    /// An icon that is not drawn. The name keeps this build's own drawing, which is what stands
    /// under every replacement anyway.
    IconDropped { name: String, why: MaterialProblem },
}

/// Why a material the document names is not used — a background that is not laid, an icon that is
/// not drawn. One enum for both: what can be wrong with a filename is the same wherever it was
/// written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialProblem {
    /// No file named, so there is nothing to draw with.
    NoFile,
    /// A name that is not one a file in a skin can have — reaching out of the zip, or carrying
    /// what an address and a declaration are cut at.
    UnusableFile,
}

impl MaterialProblem {
    /// Why the material was set aside, as a phrase that follows the name it was written under.
    /// English on both faces, the way a refusal is.
    ///
    /// **The name itself is not in it**, for the reason a value is not in [`ValueProblem::en`]: it
    /// is the one string in the file written to get somewhere else, and the terminal is a place
    /// where a string can do more than be read.
    pub fn en(&self) -> String {
        match self {
            MaterialProblem::NoFile => "names no file".to_string(),
            MaterialProblem::UnusableFile => {
                "names a file a skin cannot hold".to_string()
            }
        }
    }
}

/// Why an embedded font was set aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontProblem {
    /// Not `woff2`. It is the one format taken, so there is nothing to try.
    Format(String),
    /// The file named is not a woff2 file.
    Unreadable,
    /// Larger than [`FONT_MAX_BYTES`].
    TooLarge { bytes: usize },
    /// No family name, so there is nothing to put at the head of the stack.
    NoFamily,
    /// No file named, so there is nowhere to read the face from.
    NoFile,
    /// A name that is not one a file in a skin can have — the same names a background's file is
    /// held to.
    UnusableFile,
    /// The skin carries no file under the name the document wrote.
    Missing,
}

impl FontProblem {
    /// Why the font was set aside, in one phrase. English on both faces, the way a refusal is: a
    /// person told it in the terminal and a person shown it in the window are told the same thing.
    pub fn en(&self) -> String {
        match self {
            FontProblem::Format(said) if said.is_empty() => {
                "no format; woff2 is the one taken".to_string()
            }
            FontProblem::Format(said) => format!("format is '{said}'; woff2 is the one taken"),
            FontProblem::Unreadable => "that file is not a woff2 file".to_string(),
            FontProblem::TooLarge { bytes } => {
                format!("{bytes} bytes, over the {FONT_MAX_BYTES} this build takes")
            }
            FontProblem::NoFamily => "no family name to put at the head of the stack".to_string(),
            FontProblem::NoFile => "no file named to read the face out of".to_string(),
            FontProblem::UnusableFile => "a file name a skin cannot hold".to_string(),
            FontProblem::Missing => "no file in this skin under that name".to_string(),
        }
    }
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
    /// Carried a font and no licence text. Unlike everything else about a font this is not a thing
    /// to drop and go on with: a face whose terms of redistribution are unknown is not one to put
    /// on somebody's machine, and the file it came in is what those terms have to travel in.
    FontWithoutLicenceText,
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
    ///
    /// `materials` is what the document's filenames stand for — the file the skin arrived in. A
    /// skin that names none is checked against [`Materials::None`] and nothing here reads.
    pub fn check(self, materials: &Materials) -> Result<Taken, Refusal> {
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

        // The font, before the tables: its one refusal is about the whole document, and a licence
        // nobody can read is not a thing to get past by dropping the face and going on.
        let mut font_bytes = None;
        if let Some(file) = &self.font {
            if file.license_text.trim().is_empty() {
                return Err(Refusal::FontWithoutLicenceText);
            }
            match read_font(file, materials) {
                Ok(bytes) => font_bytes = Some(bytes),
                Err(why) => warnings.push(Warning::FontDropped(why)),
            }
        }

        let mut skin = self;
        skin.unknown_keys = Vec::new();
        if font_bytes.is_none() {
            // Dropped, so it is not carried on: what is left names a family nothing supplies, and
            // `font` falls back to the stack the author wrote beside it.
            skin.font = None;
        }
        for side in [Side::Light, Side::Dark] {
            let table = match side {
                Side::Light => &mut skin.light,
                Side::Dark => &mut skin.dark,
            };
            for key in std::mem::take(&mut table.not_text) {
                warnings.push(Warning::NotText { theme: side, key });
            }
            let mut kept = std::collections::BTreeMap::new();
            let mut asked_for = std::collections::BTreeMap::new();
            for (key, value) in std::mem::take(&mut table.values) {
                if let Some((_, moves)) = SCALES.iter().find(|(name, _)| *name == key) {
                    scale(side, &key, &value, moves, &mut kept, &mut asked_for, &mut warnings);
                } else if OPEN.binary_search(&key.as_str()).is_ok() {
                    // The name is a skin's to move; whether the value is one it may be moved to is
                    // the next question, and the last place it can be answered out loud.
                    match usable_value(&value) {
                        Ok(()) => {
                            kept.insert(key, value);
                        }
                        Err(why) => warnings.push(Warning::UnsafeValue { theme: side, key, why }),
                    }
                } else if CLOSED.binary_search(&key.as_str()).is_ok() {
                    warnings.push(Warning::ClosedToken { theme: side, key });
                } else {
                    warnings.push(Warning::UnknownToken { theme: side, key });
                }
            }
            table.values = kept;
            table.scales = asked_for;
            frame(side, table, &mut warnings);
            smoothing(side, table, &mut warnings);
        }

        // Read after the tables, because what counts is what a side kept: a stack dropped for being
        // a shape a value may not have is a stack that names nothing, whatever it said.
        if let Some(file) = &skin.font {
            let named = [Side::Light, Side::Dark].iter().any(|side| {
                let table = skin.side(*side);
                FONT_STACKS
                    .iter()
                    .filter_map(|key| table.values.get(*key))
                    .any(|stack| names_family(stack, &file.family))
            });
            if !named {
                warnings.push(Warning::FontNotNamed { family: file.family.trim().to_string() });
            }
        }

        // The backgrounds, which are the header's rather than a side's. Whether the file named is
        // in the zip is not asked here — this runs on a document, and the file it came in is not
        // always beside it; `picture` is where a name meets its bytes.
        let mut laid = BTreeMap::new();
        for (place, wrote) in std::mem::take(&mut skin.backgrounds) {
            if BACKGROUNDS.binary_search(&place.as_str()).is_err() {
                warnings.push(Warning::UnknownBackground { place });
                continue;
            }
            let file = match material_file(&wrote.file) {
                Ok(file) => file,
                Err(why) => {
                    warnings.push(Warning::BackgroundDropped { place, why });
                    continue;
                }
            };
            let fit = one_of(&place, "fit", &wrote.fit, FITS, FIT_DEFAULT, &mut warnings);
            let at = one_of(&place, "at", &wrote.at, SPOTS, SPOT_DEFAULT, &mut warnings);
            laid.insert(place, Background { file, fit, at });
        }
        skin.backgrounds = laid;

        // The icons, ruled on the way the backgrounds are and for the same reason: the document
        // says which file, and whether that file is a drawing this build lays is read off its
        // bytes, in `icon`, where the name meets the zip it came in.
        let mut drawn = BTreeMap::new();
        for (name, wrote) in std::mem::take(&mut skin.icons) {
            if ICONS.binary_search(&name.as_str()).is_err() {
                warnings.push(Warning::UnknownIcon { name });
                continue;
            }
            let file = match material_file(&wrote) {
                Ok(file) => file,
                Err(why) => {
                    warnings.push(Warning::IconDropped { name, why });
                    continue;
                }
            };
            drawn.insert(name, file);
        }
        skin.icons = drawn;

        Ok(Taken { skin, font: font_bytes, warnings })
    }

    /// One side's table, by name.
    pub fn side(&self, side: Side) -> &ThemeTable {
        match side {
            Side::Light => &self.light,
            Side::Dark => &self.dark,
        }
    }
}

/// A zip holding the entries given, as bytes. At module level rather than in the tests below
/// because a skin's materials only exist inside a zip, so every module testing against one has to
/// be able to write one.
#[cfg(test)]
pub(crate) fn packed(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write as _;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let how =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        out.start_file(*name, how).unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
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
        // `font_file` is one this build has, so it is read rather than carried out as unknown.
        assert_eq!(s.unknown_keys, ["radius_scale"]);
        assert_eq!(s.font.as_ref().unwrap().family, "Silkscreen");
        assert_eq!(s.skin_v, 2);
    }

    #[test]
    fn the_names_per_language_are_read_and_are_not_carried_out_as_unknown() {
        let s = Skin::read(
            "name: n\ntitle: Retro\ntitles:\n  ja: \"レトロゲーム\"\n  fr: Rétro\nskin_v: 1\nthemes: [dark]\n",
        )
        .unwrap();
        assert_eq!(s.title, "Retro");
        assert_eq!(s.titles["ja"], "レトロゲーム");
        assert_eq!(s.titles["fr"], "Rétro");
        assert!(s.unknown_keys.is_empty(), "titles is a key this build knows");
    }

    #[test]
    fn a_skin_that_wrote_no_names_per_language_has_none() {
        let s = Skin::read(WASHI).unwrap();
        assert!(s.titles.is_empty());
    }

    #[test]
    fn a_name_per_language_that_is_not_text_is_dropped_and_the_rest_are_kept() {
        let s = Skin::read(
            "name: n\ntitle: t\ntitles:\n  ja: \"和紙\"\n  de: [a, b]\n  fr: 3\nskin_v: 1\nthemes: [dark]\n",
        )
        .unwrap();
        assert_eq!(s.titles["ja"], "和紙");
        assert!(!s.titles.contains_key("de"));
        assert!(!s.titles.contains_key("fr"));
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
        let skin = Skin::read(&doc(
            "skin_v: 1\nthemes: [light]\n",
            "light:\n  c-bg: \"#fff\"\n  border-w: \"2px\"\n",
        ))
        .unwrap()
        .check(&Materials::None)
        .unwrap();
        assert!(skin.warnings.is_empty(), "{:?}", skin.warnings);
        assert_eq!(skin.skin.light.values["c-bg"], "#fff");
        assert_eq!(skin.skin.light.values["border-w"], "2px");
    }

    #[test]
    fn a_name_in_neither_list_is_dropped_and_reported() {
        let taken = Skin::read(&doc(
            "skin_v: 1\nthemes: [light]\n",
            "light:\n  c-bg: \"#fff\"\n  c-sepia: \"#eee\"\n",
        ))
        .unwrap()
        .check(&Materials::None)
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
        .check(&Materials::None)
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
        .check(&Materials::None)
        .unwrap();
        assert_eq!(taken.skin.light.values["c-bg"], "#fff");
        assert_eq!(
            taken.warnings,
            [Warning::NotText { theme: Side::Light, key: "c-text".into() }]
        );
    }

    #[test]
    fn a_value_written_to_get_out_of_the_declaration_is_dropped_with_the_reason_named() {
        // The case the task opens with: the file is read, the check passes it, the screen does not
        // change, and nothing anywhere says why. Now the name is dropped and the author is told.
        let taken = Skin::read(&doc(
            "skin_v: 1\nthemes: [light]\n",
            "light:\n  c-bg: \"red; } body { display: none }\"\n  c-text: \"#000\"\n",
        ))
        .unwrap()
        .check(&Materials::None)
        .unwrap();
        assert!(!taken.skin.light.values.contains_key("c-bg"), "dropped, not worn");
        assert_eq!(taken.skin.light.values["c-text"], "#000", "the rest of the file still stands");
        assert_eq!(
            taken.warnings,
            [Warning::UnsafeValue {
                theme: Side::Light,
                key: "c-bg".into(),
                why: ValueProblem::Punctuation(';'),
            }]
        );
    }

    #[test]
    fn each_shape_a_value_may_not_have_is_named_by_its_own_reason() {
        let long = "a".repeat(VALUE_MAX + 1);
        let cases: &[(&str, ValueProblem)] = &[
            ("", ValueProblem::Empty),
            ("red;", ValueProblem::Punctuation(';')),
            ("red{", ValueProblem::Punctuation('{')),
            ("red<", ValueProblem::Punctuation('<')),
            ("red\\", ValueProblem::Punctuation('\\')),
            ("red\n", ValueProblem::Punctuation('\n')),
            ("red /* and */", ValueProblem::Comment),
            ("URL(x)", ValueProblem::Url),
            ("url (x)", ValueProblem::Url),
            (&long, ValueProblem::TooLong { units: VALUE_MAX + 1 }),
        ];
        for (value, why) in cases {
            assert_eq!(usable_value(value), Err(why.clone()), "{value:?}");
            assert!(!why.en().is_empty());
            assert!(!why.en().contains(*value) || value.is_empty(), "the value itself is not said");
        }
        assert_eq!(usable_value("#fff"), Ok(()));
        assert_eq!(usable_value("ui-sans-serif, 'Hiragino Sans', sans-serif"), Ok(()));
        assert_eq!(usable_value("0 1px 2px rgba(0, 0, 0, 0.4)"), Ok(()));
        assert_eq!(usable_value(&"a".repeat(VALUE_MAX)), Ok(()), "the cap is a length, not a bound");
    }

    #[test]
    fn a_value_is_counted_the_way_the_window_counts_it() {
        // `VALUE` on the applying side is a JavaScript regular expression, so its `{1,512}` counts
        // UTF-16 code units. Counting characters here would keep a value the window then drops
        // without a word, which is the hole this warning was added to close.
        let astral = "\u{1f600}".repeat(VALUE_MAX / 2);
        assert_eq!(usable_value(&astral), Ok(()), "512 units exactly");
        let over = format!("{astral}{}", '\u{1f600}');
        assert_eq!(usable_value(&over), Err(ValueProblem::TooLong { units: VALUE_MAX + 2 }));
    }

    #[test]
    fn a_header_key_this_build_has_no_meaning_for_is_reported_and_taken_off() {
        let taken =
            Skin::read(&doc("skin_v: 1\nthemes: [light]\nradius_scale: 1.5\n", "light:\n  c-bg: \"#fff\"\n"))
                .unwrap()
                .check(&Materials::None)
                .unwrap();
        assert_eq!(taken.warnings, [Warning::UnknownHeaderKey("radius_scale".into())]);
        assert!(taken.skin.unknown_keys.is_empty());
    }

    #[test]
    fn a_later_vocabulary_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 2\nthemes: [light]\n", "light:\n  c-bg: \"#fff\"\n"))
            .unwrap()
            .check(&Materials::None)
            .unwrap_err();
        assert_eq!(e, Refusal::SkinVAhead { declared: 2, understood: SKIN_V });
    }

    #[test]
    fn a_side_that_was_declared_and_left_empty_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 1\nthemes: [light, dark]\n", "light:\n  c-bg: \"#fff\"\n"))
            .unwrap()
            .check(&Materials::None)
            .unwrap_err();
        assert_eq!(e, Refusal::SideDeclaredEmpty(Side::Dark));
    }

    #[test]
    fn a_side_that_was_set_without_being_declared_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 1\nthemes: [light]\n", "light:\n  c-bg: \"#fff\"\ndark:\n  c-bg: \"#000\"\n"))
            .unwrap()
            .check(&Materials::None)
            .unwrap_err();
        assert_eq!(e, Refusal::SideNotDeclared(Side::Dark));
    }

    #[test]
    fn a_side_this_build_does_not_have_turns_the_whole_skin_away() {
        let e = Skin::read(&doc("skin_v: 1\nthemes: [light, sepia]\n", "light:\n  c-bg: \"#fff\"\n"))
            .unwrap()
            .check(&Materials::None)
            .unwrap_err();
        assert_eq!(e, Refusal::UnknownSide("sepia".into()));
    }

    /// A skin carrying a face, as the zip it arrives in: the document with the `font_file` fields
    /// given, and the bytes beside it under [`FONT_FILE`].
    ///
    /// The table names [`FONT_HEAD`]'s family, which is what a skin carrying a face is for. A
    /// document that carried one and named it nowhere would draw a warning of its own, and the
    /// cases below are about the face rather than about the stack.
    fn with_font(fields: &str, bytes: &[u8]) -> Vec<u8> {
        let yaml = format!(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\n  \
             font: '\"Silkscreen\", sans-serif'\nfont_file:\n{fields}"
        );
        packed(&[(FONT_FILE, bytes), (PACK_DOCUMENT, yaml.as_bytes())])
    }

    /// One skin out of the file it arrived in — read, and checked against what that file carries.
    fn out_of(file: &[u8]) -> Result<Taken, Refusal> {
        let (_, yaml) = document(file).unwrap();
        Skin::read(&yaml).unwrap().check(&Materials::of(file))
    }

    /// The name the face sits under in the zips these tests write.
    const FONT_FILE: &str = "silkscreen.woff2";

    /// The five fields a usable font needs, with the bytes left to the caller.
    const FONT_HEAD: &str = "  family: Silkscreen\n  format: woff2\n  file: silkscreen.woff2\n  \
         license: OFL-1.1\n  license_text: Copyright…\n";

    /// A skin setting the frame however the case wants it.
    fn framed(lines: &str) -> Taken {
        Skin::read(&format!("name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n{lines}"))
            .unwrap()
            .check(&Materials::None)
            .unwrap()
    }

    #[test]
    fn the_three_lists_do_not_overlap_and_every_size_a_multiplier_moves_has_one() {
        let mut sorted = SIZED.to_vec();
        sorted.sort_unstable_by_key(|(n, _, _)| *n);
        assert_eq!(SIZED, sorted, "SIZED is in order");
        for (scale, moves) in SCALES {
            assert!(!OPEN.contains(scale), "{scale} is a multiplier, not a token");
            for name in *moves {
                assert!(!OPEN.contains(name), "{name} is moved by {scale}, not written directly");
                assert!(!CLOSED.contains(name), "{name} is on two lists");
                assert!(SIZED.binary_search_by_key(name, |(n, _, _)| n).is_ok(), "{name} has no size");
            }
        }
    }

    #[test]
    fn a_multiplier_moves_a_whole_family_and_keeps_the_ladder() {
        let taken = framed("  fs-scale: \"1.25\"\n");
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        let v = &taken.skin.light.values;
        assert_eq!(v["fs-xs"], "15px", "12 × 1.25");
        assert_eq!(v["fs-md"], "17.5px");
        assert_eq!(v["fs-body"], "20px");
        assert_eq!(v["fs-xl"], "25px");
        // The steps are still four steps, in the order they were designed in.
        assert!(v["fs-xs"] < v["fs-md"] && v["fs-body"] < v["fs-xl"]);
        // And the multiplier itself is not left in the table as if it were a token.
        assert!(!v.contains_key("fs-scale"));
    }

    #[test]
    fn a_multiplier_past_what_was_measured_is_brought_back_inside_it() {
        let taken = framed("  s-scale: \"2\"\n");
        assert_eq!(taken.skin.light.values["s-1"], "5.2px", "4 × 1.3");
        assert_eq!(
            taken.warnings,
            [Warning::Scale {
                theme: Side::Light,
                key: "s-scale".into(),
                wrote: "2".into(),
                used: Some(format!("{SCALE_MAX}")),
            }]
        );
        assert_eq!(framed("  s-scale: \"0.1\"\n").skin.light.values["s-4"], "13.6px", "16 × 0.85");
    }

    /// The two families with no size that stops them working. A square corner is a look and a flat
    /// screen is a screen, so both go all the way down — and zero is the number an author reaching
    /// for either of them writes.
    #[test]
    fn the_two_with_nothing_under_them_go_all_the_way_to_zero() {
        let taken = framed("  r-scale: \"0\"\n  shadow-scale: \"0\"\n");
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        assert_eq!(taken.skin.light.values["r-sm"], "0px");
        assert_eq!(taken.skin.light.values["r-lg"], "0px");
        assert!(
            taken.skin.light.values["shadow-md"].starts_with("0 0px 0px"),
            "every length at nothing, the colour left alone: {}",
            taken.skin.light.values["shadow-md"]
        );
        // And the floor the other two keep is still under them.
        assert_eq!(framed("  s-scale: \"0\"\n").skin.light.values["s-4"], "13.6px", "16 × 0.85");
    }

    /// A number this build understood and could not use is said with both values. Only text that is
    /// no number at all is dropped — a negative is a number, and telling an author it was not would
    /// send them looking for a typo that is not there.
    #[test]
    fn a_negative_multiplier_is_brought_up_to_the_floor_and_said() {
        let taken = framed("  r-scale: \"-2\"\n");
        assert_eq!(taken.skin.light.values["r-md"], "0px");
        assert_eq!(
            taken.warnings,
            [Warning::Scale {
                theme: Side::Light,
                key: "r-scale".into(),
                wrote: "-2".into(),
                used: Some("0".into()),
            }]
        );
    }

    #[test]
    fn the_one_family_with_nothing_under_it_to_cut_off_is_not_bounded() {
        // A shadow three times over is ugly rather than unusable, so the number stands.
        let taken = framed("  shadow-scale: \"3\"\n");
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        assert!(taken.skin.light.values["shadow-md"].starts_with("0 12px 36px"), "every length, and
            the colour left alone: {}", taken.skin.light.values["shadow-md"]);
        assert!(taken.skin.light.values["shadow-md"].contains("rgba(35, 33, 28, 0.1)"));
    }

    #[test]
    fn a_multiplier_that_is_not_a_number_moves_nothing() {
        let taken = framed("  r-scale: \"big\"\n");
        assert!(!taken.skin.light.values.contains_key("r-md"));
        assert_eq!(
            taken.warnings,
            [Warning::Scale {
                theme: Side::Light,
                key: "r-scale".into(),
                wrote: "big".into(),
                used: None,
            }]
        );
    }

    #[test]
    fn a_size_written_out_by_name_is_not_a_name_this_build_has() {
        // The ladders move together or not at all: writing one step is writing a set that is no
        // longer a ladder, so the name is not one a skin may set.
        let taken = framed("  fs-md: \"40px\"\n");
        assert!(!taken.skin.light.values.contains_key("fs-md"));
        assert_eq!(
            taken.warnings,
            [Warning::UnknownToken { theme: Side::Light, key: "fs-md".into() }]
        );
    }

    #[test]
    fn a_frame_this_build_can_draw_is_taken_as_written() {
        let taken = framed("  border-w: 2px\n  border-style: solid\n");
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        assert_eq!(taken.skin.light.values["border-w"], "2px");
        assert_eq!(taken.skin.light.values["border-style"], "solid");
    }

    #[test]
    fn a_way_of_drawing_a_frame_this_build_does_not_offer_is_dropped() {
        for style in ["dashed", "dotted", "groove"] {
            let taken = framed(&format!("  border-style: \"{style}\"\n"));
            assert!(!taken.skin.light.values.contains_key("border-style"), "{style}");
            assert_eq!(
                taken.warnings,
                [Warning::Frame {
                    theme: Side::Light,
                    key: "border-style",
                    wrote: style.into(),
                    used: None
                }]
            );
        }
    }

    #[test]
    fn a_way_of_drawing_the_glyphs_this_build_offers_is_taken_as_written() {
        for word in SMOOTHINGS {
            let taken = framed(&format!("  font-smooth: \"{word}\"\n"));
            assert!(taken.warnings.is_empty(), "{word}: {:?}", taken.warnings);
            assert_eq!(taken.skin.light.values["font-smooth"], *word);
        }
    }

    #[test]
    fn a_way_of_drawing_the_glyphs_this_build_does_not_offer_is_dropped() {
        // `subpixel-antialiased` is the one an author is likeliest to reach for, and the one that
        // would draw as `auto` on every engine measured.
        for word in ["subpixel-antialiased", "grayscale", "off"] {
            let taken = framed(&format!("  font-smooth: \"{word}\"\n"));
            assert!(!taken.skin.light.values.contains_key("font-smooth"), "{word}");
            assert_eq!(
                taken.warnings,
                [Warning::Choice {
                    theme: Side::Light,
                    key: "font-smooth",
                    wrote: word.into()
                }]
            );
        }
    }

    #[test]
    fn a_frame_written_as_nothing_is_answered_as_an_empty_value_rather_than_as_a_style() {
        // The shape of a value is asked before what the value says, so an empty one is told what it
        // is — "none" is how a frame is taken away, and a reader shown "'' is not one this build
        // draws" would go looking for the style they mistyped.
        let taken = framed("  border-style: \"\"\n");
        assert!(!taken.skin.light.values.contains_key("border-style"));
        assert_eq!(
            taken.warnings,
            [Warning::UnsafeValue {
                theme: Side::Light,
                key: "border-style".into(),
                why: ValueProblem::Empty,
            }]
        );
    }

    #[test]
    fn a_width_past_what_reads_as_a_frame_is_put_back_inside_it() {
        let taken = framed("  border-w: 12px\n");
        assert_eq!(taken.skin.light.values["border-w"], "4px");
        assert_eq!(
            taken.warnings,
            [Warning::Frame {
                theme: Side::Light,
                key: "border-w",
                wrote: "12px".into(),
                used: Some("4px".into())
            }]
        );

        // Below zero is not a frame drawn the other way round; it is nothing.
        assert_eq!(framed("  border-w: -2px\n").skin.light.values["border-w"], "0");
        // A frame taken away is a look somebody may want, so zero is a length. Written as `0px`:
        // a bare `0` is a number to YAML, which the check drops the way it drops any value that
        // did not arrive as text.
        assert!(framed("  border-w: 0px\n").warnings.is_empty());
        assert_eq!(framed("  border-w: 0px\n").skin.light.values["border-w"], "0px");
        assert_eq!(
            framed("  border-w: 0\n").warnings,
            [Warning::NotText { theme: Side::Light, key: "border-w".into() }],
            "a bare zero is a number, and the check says so"
        );
    }

    #[test]
    fn a_width_that_is_not_a_length_is_dropped() {
        let taken = framed("  border-w: thick\n");
        assert!(!taken.skin.light.values.contains_key("border-w"));
        assert_eq!(
            taken.warnings,
            [Warning::Frame {
                theme: Side::Light,
                key: "border-w",
                wrote: "thick".into(),
                used: None
            }]
        );
    }

    #[test]
    fn a_double_frame_is_drawn_at_the_width_that_splits_whatever_was_asked_for() {
        // The widths that split on every engine measured are 3.00–3.75 and 5.00 up, and five is
        // past the cap — so what is left is one band, and the author does not pick inside it.
        for wrote in ["4px", "1px", "0"] {
            let taken = framed(&format!("  border-w: {wrote}\n  border-style: double\n"));
            assert_eq!(taken.skin.light.values["border-w"], BORDER_W_DOUBLE, "{wrote}");
            assert!(
                taken.warnings.iter().any(|w| matches!(
                    w,
                    Warning::Frame { key: "border-w", used: Some(used), .. } if used == BORDER_W_DOUBLE
                )),
                "the author is told which value was used instead: {:?}",
                taken.warnings
            );
        }
        // Asking for the width it would be drawn at anyway is not a thing to report.
        let taken = framed("  border-w: 3px\n  border-style: double\n");
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
    }

    #[test]
    fn a_double_frame_with_no_width_of_its_own_still_gets_one() {
        let taken = framed("  border-style: double\n");
        assert_eq!(taken.skin.light.values["border-w"], BORDER_W_DOUBLE);
    }

    #[test]
    fn a_font_the_document_names_comes_out_of_the_file_beside_it() {
        let bytes = [b"wOF2".as_slice(), &[0u8; 64]].concat();
        let taken = out_of(&with_font(FONT_HEAD, &bytes)).unwrap();
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        assert_eq!(taken.font.as_deref(), Some(bytes.as_slice()));
        assert_eq!(taken.skin.font.as_ref().unwrap().family, "Silkscreen");
        assert_eq!(taken.skin.font.as_ref().unwrap().file, FONT_FILE);
        assert_eq!(taken.skin.font.as_ref().unwrap().license, "OFL-1.1");
    }

    /// A face is registered under its family name, and a screen is set in it by a stack asking for
    /// that name. Carrying one and asking for it nowhere is a skin whose font does nothing, which
    /// is worth saying before the file is taken in rather than after.
    #[test]
    fn a_face_no_stack_asks_for_is_carried_and_never_drawn() {
        let bytes = [b"wOF2".as_slice(), &[0u8; 64]].concat();
        let yaml = format!(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\nfont_file:\n{FONT_HEAD}"
        );
        let file = packed(&[(FONT_FILE, bytes.as_slice()), (PACK_DOCUMENT, yaml.as_bytes())]);
        let taken = out_of(&file).unwrap();
        assert_eq!(taken.warnings, [Warning::FontNotNamed { family: "Silkscreen".into() }]);
        assert_eq!(taken.font.as_deref(), Some(bytes.as_slice()), "the face is taken either way");
    }

    /// Either stack answers, and so does either side: a skin carries one face and where it belongs
    /// is the author's to say. The name is matched as a name — quotes are how a family with a space
    /// in it is written, and case is not what tells two families apart.
    #[test]
    fn a_stack_asking_for_the_face_is_read_wherever_the_author_wrote_it() {
        let bytes = [b"wOF2".as_slice(), &[0u8; 64]].concat();
        for wrote in [
            "themes: [light]\nlight:\n  font: '\"Silkscreen\", sans-serif'\n",
            "themes: [light]\nlight:\n  font-mono: 'Silkscreen, monospace'\n",
            "themes: [light]\nlight:\n  font: 'silkscreen'\n",
            "themes: [dark]\ndark:\n  font: '\"Silkscreen\"'\n",
        ] {
            let yaml = format!("name: n\ntitle: t\nskin_v: 1\n{wrote}font_file:\n{FONT_HEAD}");
            let file = packed(&[(FONT_FILE, bytes.as_slice()), (PACK_DOCUMENT, yaml.as_bytes())]);
            let taken = out_of(&file).unwrap();
            assert!(taken.warnings.is_empty(), "{wrote}: {:?}", taken.warnings);
        }
    }

    /// A name that is only inside another name is not that name. `Silk` and `Silkscreen` are two
    /// families, and a stack asking for one is not asking for the other.
    #[test]
    fn a_stack_asking_for_a_name_the_family_starts_with_is_not_asking_for_the_family() {
        let bytes = [b"wOF2".as_slice(), &[0u8; 64]].concat();
        let yaml = format!(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  font: '\"Silk\", sans-serif'\n\
             font_file:\n{FONT_HEAD}"
        );
        let file = packed(&[(FONT_FILE, bytes.as_slice()), (PACK_DOCUMENT, yaml.as_bytes())]);
        let taken = out_of(&file).unwrap();
        assert_eq!(taken.warnings, [Warning::FontNotNamed { family: "Silkscreen".into() }]);
    }

    /// A face that was set aside is not carried on, so there is nothing left for a stack to ask
    /// for: the one thing said about it is why it was dropped.
    #[test]
    fn a_face_that_was_dropped_is_not_also_reported_as_one_nothing_asks_for() {
        let yaml = format!(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\nfont_file:\n{FONT_HEAD}"
        );
        let file = packed(&[(FONT_FILE, b"not a font at all"), (PACK_DOCUMENT, yaml.as_bytes())]);
        let taken = out_of(&file).unwrap();
        assert_eq!(taken.warnings, [Warning::FontDropped(FontProblem::Unreadable)]);
    }

    /// A document on its own is one file, so there is nothing beside it to be the face. The
    /// colours are taken and the name is said, the way any other face nobody can read is.
    #[test]
    fn a_bare_document_has_nothing_to_read_a_face_out_of() {
        let yaml = format!(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\nfont_file:\n{FONT_HEAD}"
        );
        let taken = out_of(yaml.as_bytes()).unwrap();
        assert_eq!(
            taken.warnings,
            [Warning::FontDropped(FontProblem::Missing)]
        );
        assert!(taken.font.is_none());
        assert_eq!(taken.skin.light.values["c-bg"], "#fff", "the colours are taken");
    }

    /// A name climbing out of the archive is not a name a file in a skin can have, and is turned
    /// down on the name rather than looked for. Nothing here reaches the filesystem either way.
    #[test]
    fn a_face_named_outside_the_skin_is_not_a_name_a_file_can_have() {
        for named in ["../../../../etc/passwd", "/etc/passwd"] {
            let fields = format!(
                "  family: Silkscreen\n  format: woff2\n  file: '{named}'\n  license: OFL-1.1\n  \
                 license_text: Copyright…\n"
            );
            let taken = out_of(&with_font(&fields, b"wOF2 and a face")).unwrap();
            assert_eq!(
                taken.warnings,
                [Warning::FontDropped(FontProblem::UnusableFile)],
                "{named}"
            );
            assert!(taken.font.is_none(), "{named}");
        }
    }

    #[test]
    fn a_font_that_is_not_what_it_says_is_set_aside_and_the_colours_are_taken() {
        let cases: Vec<(&str, Vec<u8>, FontProblem)> = vec![
            (
                "ttf",
                with_font(
                    "  family: F\n  format: ttf\n  file: silkscreen.woff2\n  license: X\n  \
                     license_text: Y\n",
                    b"wOF2....",
                ),
                FontProblem::Format("ttf".into()),
            ),
            (
                // Says woff2 and is not: the bytes are read rather than the claim about them.
                "not a face",
                with_font(FONT_HEAD, b"not a font at all"),
                FontProblem::Unreadable,
            ),
            (
                "no family",
                with_font(
                    "  family: \"\"\n  format: woff2\n  file: silkscreen.woff2\n  license: X\n  \
                     license_text: Y\n",
                    b"wOF2....",
                ),
                FontProblem::NoFamily,
            ),
            (
                "no file named",
                with_font(
                    "  family: F\n  format: woff2\n  license: X\n  license_text: Y\n",
                    b"wOF2....",
                ),
                FontProblem::NoFile,
            ),
            (
                // Named, and the skin carries no such file.
                "named and not there",
                with_font(
                    "  family: F\n  format: woff2\n  file: other.woff2\n  license: X\n  \
                     license_text: Y\n",
                    b"wOF2....",
                ),
                FontProblem::Missing,
            ),
        ];
        for (case, file, why) in cases {
            let taken = out_of(&file).unwrap();
            assert_eq!(taken.warnings, [Warning::FontDropped(why)], "{case}");
            assert!(taken.font.is_none(), "{case}");
            assert!(taken.skin.font.is_none(), "{case}: and the family is not carried on");
            assert_eq!(taken.skin.light.values["c-bg"], "#fff", "{case}: the colours are taken");
        }
    }

    #[test]
    fn a_font_past_the_size_this_build_takes_is_set_aside_with_its_weight() {
        let bytes = [b"wOF2".as_slice(), &vec![0u8; FONT_MAX_BYTES]].concat();
        let taken = out_of(&with_font(FONT_HEAD, &bytes)).unwrap();
        assert_eq!(taken.warnings, [Warning::FontDropped(FontProblem::TooLarge { bytes: bytes.len() })]);
        assert!(taken.font.is_none());
    }

    #[test]
    fn a_font_with_no_licence_text_turns_the_whole_skin_away() {
        // The one thing about a font that is not dropped and gone on with: the terms it may be
        // passed on under have to travel in the file it travels in.
        let file = with_font(
            "  family: F\n  format: woff2\n  file: silkscreen.woff2\n  license: OFL-1.1\n  \
             license_text: \"  \"\n",
            b"wOF2....",
        );
        assert_eq!(out_of(&file).unwrap_err(), Refusal::FontWithoutLicenceText);
    }

    #[test]
    fn a_skin_that_carries_no_font_says_so_rather_than_warning_about_one() {
        let taken = Skin::read("name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\n")
            .unwrap()
            .check(&Materials::None)
            .unwrap();
        assert!(taken.font.is_none());
        assert!(taken.warnings.is_empty());
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
            .check(&Materials::None)
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
        .check(&Materials::None)
        .unwrap();
        assert!(taken.skin.dark.values.is_empty());
        assert_eq!(
            taken.warnings,
            [Warning::UnknownToken { theme: Side::Dark, key: "c-sepia".into() }]
        );
    }

    // ---- the shape a skin arrives in (`AMB-D-936`) ----

    const ONE_SKIN: &str =
        "name: kozo\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#ffffff\"\n";

    #[test]
    fn the_document_comes_out_of_the_zip_and_out_of_a_bare_file_alike() {
        let bare = document(ONE_SKIN.as_bytes()).unwrap();
        assert_eq!(bare, (Packing::Bare, ONE_SKIN.to_string()));

        let zip = packed(&[
            ("background.png", b"not really a png"),
            (PACK_DOCUMENT, ONE_SKIN.as_bytes()),
        ]);
        let (packing, yaml) = document(&zip).unwrap();
        assert_eq!(packing, Packing::Packed);
        assert_eq!(yaml, ONE_SKIN);
        assert_eq!(Skin::read(&yaml).unwrap().name, "kozo");
    }

    #[test]
    fn a_zip_with_no_skin_in_it_says_which_file_is_missing() {
        let zip = packed(&[("colours.yaml", ONE_SKIN.as_bytes())]);
        let why = document(&zip).unwrap_err().to_string();
        assert!(why.contains(PACK_DOCUMENT), "{why}");
    }

    #[test]
    fn an_entry_pointing_out_of_the_zip_turns_the_whole_file_away() {
        // Refused rather than skipped, and refused on the way to the document: a file carrying
        // such a name is not a skin with one odd entry in it.
        for name in ["../escaped.png", "art/../../escaped.png", "/etc/passwd"] {
            let zip = packed(&[(name, b"x"), (PACK_DOCUMENT, ONE_SKIN.as_bytes())]);
            assert!(document(&zip).is_err(), "{name}");
            assert!(weigh(&zip).is_err(), "{name}");
        }
    }

    #[test]
    fn what_unpacks_to_more_than_this_build_takes_is_stopped_while_it_unpacks() {
        // The index says one thing and the decoder says another, which is the whole point: a run
        // of one byte compresses to almost nothing, so the number that decides has to be the one
        // coming out.
        let big = vec![0u8; PACK_FILE_MAX_BYTES as usize + 1];
        let zip = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes()), ("big.bin", &big)]);
        assert!(zip.len() < 64 * 1024, "the file itself is small: {} bytes", zip.len());
        let why = weigh(&zip).unwrap_err().to_string();
        assert!(why.contains("big.bin"), "{why}");
        // And the document still comes out, because reading it never touches that entry —
        // except that the index already says how heavy it is, which is enough to say no.
        assert!(document(&zip).is_err(), "the index is read on the way to the document");
    }

    #[test]
    fn a_skin_that_fits_is_weighed_and_says_what_it_came_to() {
        let zip = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes()), ("art.bin", &[7u8; 1000])]);
        assert_eq!(weigh(&zip).unwrap(), ONE_SKIN.len() as u64 + 1000);
    }

    #[test]
    fn what_a_skin_carries_is_listed_by_what_the_bytes_are_rather_than_what_they_are_called() {
        let zip = packed(&[
            (PACK_DOCUMENT, ONE_SKIN.as_bytes()),
            ("paper.png", A_PNG),
            // Named `.png` and is a face. What is said is what the bytes say.
            ("silkscreen.png", b"wOF2 and the rest of a face"),
            ("notes.txt", b"nothing this build draws"),
        ]);
        let carried = carries(&zip).unwrap();
        assert_eq!(
            carried.iter().map(|c| (c.file.as_str(), c.is)).collect::<Vec<_>>(),
            [
                ("paper.png", Material::Picture(Picture::Png)),
                ("silkscreen.png", Material::Face),
                ("notes.txt", Material::Neither),
            ],
            "the document is not one of the files it carries"
        );
        assert_eq!(carried[0].bytes, A_PNG.len() as u64, "weighed unpacked");
        assert_eq!(carried[0].is.word(), Some("png"));
        assert_eq!(carried[2].is.word(), None, "there is no word for bytes nothing draws");
    }

    #[test]
    fn a_skin_that_is_one_document_carries_nothing_to_list() {
        assert_eq!(carries(ONE_SKIN.as_bytes()).unwrap(), []);
    }

    #[test]
    fn a_file_that_unpacks_past_the_ceiling_stops_the_listing_rather_than_being_listed() {
        // The same number the way in counts to, and counted the same way — out of the decoder.
        let big = vec![0u8; PACK_FILE_MAX_BYTES as usize + 1];
        let zip = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes()), ("huge.bin", &big)]);
        let why = carries(&zip).unwrap_err().to_string();
        assert!(why.contains("huge.bin"), "{why}");
    }

    #[test]
    fn a_bare_document_is_turned_away_at_the_door_and_told_how_to_come_back() {
        let why = arriving(ONE_SKIN.as_bytes()).unwrap_err().to_string();
        assert!(why.contains("zip"), "{why}");
        assert!(why.contains(PACK_DOCUMENT), "the way back in is in the sentence: {why}");
        // And what a device already holds under the old shape still reads: the door is shut,
        // not the file.
        assert_eq!(document(ONE_SKIN.as_bytes()).unwrap().0, Packing::Bare);
    }

    #[test]
    fn a_zip_arrives_and_comes_out_as_its_document() {
        let zip = packed(&[("background.png", b"not really a png"), (PACK_DOCUMENT, ONE_SKIN.as_bytes())]);
        assert_eq!(arriving(&zip).unwrap(), ONE_SKIN);
        // Weighed on the way through, so a file that unpacks to more than this build takes never
        // reaches the check.
        let big = vec![0u8; PACK_FILE_MAX_BYTES as usize + 1];
        let heavy = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes()), ("big.bin", &big)]);
        assert!(arriving(&heavy).is_err());
    }

    #[test]
    fn a_document_amenbo_packs_itself_reads_back_as_the_skin_it_was() {
        // The round trip the template takes: written out packed, handed back in, and the same
        // skin comes out the other side.
        let packed = pack_document(ONE_SKIN).unwrap();
        assert_eq!(Packing::of(&packed), Packing::Packed);
        let (packing, yaml) = document(&packed).unwrap();
        assert_eq!(packing, Packing::Packed);
        assert_eq!(yaml, ONE_SKIN);
        assert_eq!(Skin::read(&yaml).unwrap().name, "kozo");
        assert_eq!(weigh(&packed).unwrap(), ONE_SKIN.len() as u64);
    }

    #[test]
    fn a_skin_is_kept_in_the_shape_it_arrived_in_and_read_back_out_of_it() {
        let paths = crate::config::Paths::at(amenbo_scratch::scratch("skins-packed"));
        let zip = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes())]);
        Skin::install(&paths, "kozo", &zip).unwrap();

        let at = paths.skin_file("kozo", PACK_EXT);
        assert!(at.is_file(), "kept as the zip it arrived as");
        assert_eq!(std::fs::read(&at).unwrap(), zip, "byte for byte, the author's own file");
        assert_eq!(kept_file(&paths, "kozo").map(|(p, _)| p), Some(Packing::Packed));
        let (read, materials) = Skin::installed(&paths, "kozo").unwrap().unwrap();
        assert_eq!(read.name, "kozo");
        assert!(
            matches!(materials, Materials::Pack(_)),
            "and its materials are read back out of the same zip"
        );
        assert_eq!(
            Skin::installed_all(&paths).iter().filter(|(n, _)| n == "kozo").count(),
            1
        );
        assert!(Skin::uninstall(&paths, "kozo").unwrap());
        assert!(!at.exists());
    }

    // ---- the pictures a skin lays behind its surfaces (`AMB-D-936`) ----

    /// A document laying the backgrounds written under it, over a skin that reads.
    fn laying(backgrounds: &str) -> Skin {
        Skin::read(&format!(
            "name: kozo\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#ffffff\"\nbackgrounds:\n{backgrounds}"
        ))
        .unwrap()
    }

    #[test]
    fn a_background_is_read_with_the_two_words_beside_its_file() {
        let taken = laying("  c-bg:\n    file: paper.png\n    fit: tile\n    at: top-left\n")
            .check(&Materials::None)
            .unwrap();
        let laid = &taken.skin.backgrounds["c-bg"];
        assert_eq!(laid.file, "paper.png");
        assert_eq!(laid.fit, "tile");
        assert_eq!(laid.at, "top-left");
        assert!(taken.warnings.is_empty());
    }

    #[test]
    fn a_background_written_as_a_file_alone_is_laid_the_way_this_build_lays_one() {
        // The ordinary case: somebody who has not thought about how it sits should not have to.
        let taken = laying("  c-surface:\n    file: art/grain.webp\n").check(&Materials::None).unwrap();
        let laid = &taken.skin.backgrounds["c-surface"];
        assert_eq!(laid.file, "art/grain.webp");
        assert_eq!(laid.fit, FIT_DEFAULT);
        assert_eq!(laid.at, SPOT_DEFAULT);
        assert!(taken.warnings.is_empty());
    }

    #[test]
    fn a_background_named_for_a_place_this_build_has_none_is_carried_out_by_name() {
        let taken = laying("  c-text:\n    file: paper.png\n").check(&Materials::None).unwrap();
        assert!(taken.skin.backgrounds.is_empty());
        assert_eq!(
            taken.warnings,
            [Warning::UnknownBackground { place: "c-text".into() }]
        );
    }

    #[test]
    fn a_background_with_no_file_behind_it_is_dropped_by_the_place_it_was_named_for() {
        // Written as a map with nothing in it, and written as something that is not a map at all
        // — both come to the same thing: there is no picture to lay.
        for wrote in ["  c-sunken:\n    fit: cover\n", "  c-sunken: paper.png\n"] {
            let taken = laying(wrote).check(&Materials::None).unwrap();
            assert!(taken.skin.backgrounds.is_empty(), "{wrote}");
            assert_eq!(
                taken.warnings,
                [Warning::BackgroundDropped {
                    place: "c-sunken".into(),
                    why: MaterialProblem::NoFile,
                }],
                "{wrote}"
            );
        }
    }

    #[test]
    fn a_file_that_reaches_out_of_the_skin_is_not_a_background() {
        for name in ["../../wallpaper.png", "/etc/passwd", "a;b{.png", "paper.png?x=1"] {
            let taken = laying(&format!("  c-bg:\n    file: '{name}'\n")).check(&Materials::None).unwrap();
            assert!(taken.skin.backgrounds.is_empty(), "{name}");
            assert_eq!(
                taken.warnings,
                [Warning::BackgroundDropped {
                    place: "c-bg".into(),
                    why: MaterialProblem::UnusableFile,
                }],
                "{name}"
            );
        }
    }

    #[test]
    fn a_word_this_build_has_no_drawing_for_is_reported_and_the_picture_is_still_laid() {
        let taken = laying("  c-bg:\n    file: paper.png\n    fit: stretch\n    at: middle\n")
            .check(&Materials::None)
            .unwrap();
        let laid = &taken.skin.backgrounds["c-bg"];
        assert_eq!(laid.fit, FIT_DEFAULT, "the picture is laid the way this build lays one");
        assert_eq!(laid.at, SPOT_DEFAULT);
        assert_eq!(
            taken.warnings,
            [
                Warning::BackgroundChoice {
                    place: "c-bg".into(),
                    key: "fit",
                    wrote: "stretch".into()
                },
                Warning::BackgroundChoice {
                    place: "c-bg".into(),
                    key: "at",
                    wrote: "middle".into()
                },
            ]
        );
    }

    #[test]
    fn every_place_this_build_lays_a_background_behind_is_a_name_a_skin_may_set() {
        // The place is the token drawn there, so a name that is not in the open vocabulary would
        // be a background behind a colour nobody can write.
        for place in BACKGROUNDS {
            assert!(OPEN.binary_search(place).is_ok(), "{place}");
        }
    }


    // ---- the drawings a skin puts in place of this build's own (`AMB-D-937`) ----

    /// A document naming the icons written under it, over a skin that reads.
    fn drawing(icons: &str) -> Skin {
        Skin::read(&format!(
            "name: kozo\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#ffffff\"\nicons:\n{icons}"
        ))
        .unwrap()
    }

    #[test]
    fn an_icon_is_read_as_the_one_file_written_beside_its_name() {
        let taken = drawing("  gear: art/gear.svg\n  gavel: gavel.png\n").check(&Materials::None).unwrap();
        assert_eq!(taken.skin.icons["gear"], "art/gear.svg");
        assert_eq!(taken.skin.icons["gavel"], "gavel.png");
        assert!(taken.warnings.is_empty());
    }

    #[test]
    fn an_icon_the_document_leaves_out_is_not_in_what_comes_back() {
        // What a skin does not replace keeps the drawing `Icon.tsx` holds, and the way that is
        // said here is by the name not being in the map at all.
        let taken = drawing("  gear: gear.svg\n").check(&Materials::None).unwrap();
        assert_eq!(taken.skin.icons.len(), 1);
        assert!(!taken.skin.icons.contains_key("gavel"));
    }

    #[test]
    fn an_icon_named_for_a_drawing_this_build_has_none_of_is_carried_out_by_name() {
        let taken = drawing("  sundial: sundial.svg\n").check(&Materials::None).unwrap();
        assert!(taken.skin.icons.is_empty());
        assert_eq!(taken.warnings, [Warning::UnknownIcon { name: "sundial".into() }]);
    }

    #[test]
    fn an_icon_with_no_file_behind_it_is_dropped_by_the_name_it_was_written_under() {
        // Written empty, and written as something that is not a filename at all — both come to
        // the same thing: there is no drawing to lay.
        for wrote in ["  gear: \"\"\n", "  gear: 7\n", "  gear:\n    file: gear.svg\n"] {
            let taken = drawing(wrote).check(&Materials::None).unwrap();
            assert!(taken.skin.icons.is_empty(), "{wrote}");
            assert_eq!(
                taken.warnings,
                [Warning::IconDropped {
                    name: "gear".into(),
                    why: MaterialProblem::NoFile,
                }],
                "{wrote}"
            );
        }
    }

    #[test]
    fn a_file_that_reaches_out_of_the_skin_is_not_an_icon() {
        for name in ["../../gear.svg", "/etc/passwd", "a;b{.svg", "gear.svg?x=1"] {
            let taken = drawing(&format!("  gear: '{name}'\n")).check(&Materials::None).unwrap();
            assert!(taken.skin.icons.is_empty(), "{name}");
            assert_eq!(
                taken.warnings,
                [Warning::IconDropped {
                    name: "gear".into(),
                    why: MaterialProblem::UnusableFile,
                }],
                "{name}"
            );
        }
    }

    #[test]
    fn every_icon_a_skin_may_replace_is_one_the_window_draws() {
        // The list here is the one `Icon.tsx` declares, held to it by `app/src/core/skin.test.ts`.
        // What this asserts is the shape it has to be in to be searched at all.
        let mut sorted = ICONS.to_vec();
        sorted.sort_unstable();
        assert_eq!(ICONS, sorted.as_slice(), "the list is searched by halving");
        assert_eq!(ICONS.len(), 51);
    }

    const A_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

    #[test]
    fn a_material_comes_out_of_the_zip_by_the_name_the_document_gave_it() {
        let zip = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes()), ("art/paper.png", A_PNG)]);
        assert_eq!(material(&zip, "art/paper.png").unwrap(), A_PNG);
        assert_eq!(picture(&zip, "art/paper.png").unwrap().0, Picture::Png);

        let why = material(&zip, "art/none.png").unwrap_err().to_string();
        assert!(why.contains("art/none.png"), "{why}");
        // A name the check would never have kept never reaches the archive.
        assert!(material(&zip, "../escaped.png").is_err());
        // And a skin that is one document carries no files at all.
        assert!(material(ONE_SKIN.as_bytes(), "art/paper.png").is_err());
    }

    #[test]
    fn a_file_that_is_not_a_picture_this_build_draws_is_said_so_by_its_bytes() {
        let zip = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes()), ("paper.png", b"not a png")]);
        let why = picture(&zip, "paper.png").unwrap_err().to_string();
        assert!(why.contains("paper.png"), "{why}");
    }

    #[test]
    fn a_drawing_with_nothing_to_read_a_mask_off_is_not_an_icon() {
        // A jpeg is a picture and is not a drawing: laid as a mask it is opaque everywhere, which
        // draws a filled square where the icon was.
        let jpeg: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF";
        let zip = packed(&[
            (PACK_DOCUMENT, ONE_SKIN.as_bytes()),
            ("gear.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            ("gear.jpg", jpeg),
        ]);
        let held = Materials::of(&zip);
        assert_eq!(icon(&held, "gear.svg").unwrap().0, Picture::Svg);
        assert_eq!(picture(&zip, "gear.jpg").unwrap().0, Picture::Jpeg, "it is a picture");
        let why = icon(&held, "gear.jpg").unwrap_err().to_string();
        assert!(why.contains("gear.jpg"), "{why}");
    }

    #[test]
    fn an_icon_of_a_skin_that_ships_inside_the_build_is_read_the_same_way() {
        // A shipped skin has no zip: its files sit in the binary beside its document. The window
        // asks for its drawings in the one place a name goes, so that shape has to answer too.
        let held = Materials::Shipped(&[("gear.png", A_PNG)]);
        assert_eq!(icon(&held, "gear.png").unwrap().0, Picture::Png);
        assert!(icon(&held, "gavel.png").is_err());
    }

    #[test]
    fn what_a_picture_is_comes_off_its_bytes_rather_than_off_its_name() {
        assert_eq!(Picture::of(A_PNG), Some(Picture::Png));
        assert_eq!(Picture::of(b"\xff\xd8\xff\xe0\x00\x10JFIF"), Some(Picture::Jpeg));
        assert_eq!(Picture::of(b"RIFF\x24\x00\x00\x00WEBPVP8 "), Some(Picture::Webp));
        assert_eq!(Picture::of(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), Some(Picture::Svg));
        assert_eq!(
            Picture::of(b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<svg/>"),
            Some(Picture::Svg),
            "a mark and a declaration in front of it are not the picture"
        );
        // `RIFF` opens more than pictures, and a page that mentions one is not one.
        assert_eq!(Picture::of(b"RIFF\x24\x00\x00\x00WAVEfmt "), None);
        assert_eq!(Picture::of(b"<html><body>an <svg> is written here</body></html>"), None);
        assert_eq!(Picture::of(b"GIF89a"), None, "a picture that moves is not one to sit behind text");
        assert_eq!(Picture::of(b""), None);
    }

    #[test]
    fn one_name_is_one_file_however_the_shape_changes_under_it() {
        // A bare skin from before, replaced by a packed one, leaves no `.yaml` behind: two files
        // under one word is one skin nothing can reach, and which of them answered would be the
        // directory's call. Written by hand, because that shape has no way in any more.
        let paths = crate::config::Paths::at(amenbo_scratch::scratch("skins-reshaped"));
        std::fs::create_dir_all(paths.skins_dir()).unwrap();
        std::fs::write(paths.skin_file("kozo", FILE_EXT), ONE_SKIN).unwrap();

        let zip = packed(&[(PACK_DOCUMENT, ONE_SKIN.as_bytes())]);
        Skin::install(&paths, "kozo", &zip).unwrap();
        assert!(paths.skin_file("kozo", PACK_EXT).is_file());
        assert!(!paths.skin_file("kozo", FILE_EXT).exists(), "the shape it left behind is gone");
        assert_eq!(Skin::installed_all(&paths).iter().filter(|(n, _)| n == "kozo").count(), 1);
    }
}
