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
    /// The one font a skin may carry, as the document writes it. A pixel face is a look the name
    /// stack cannot reach: pointing at a family the reader does not have leaves the author's screen
    /// and theirs as different pictures, so the bytes travel with the colours.
    ///
    /// One, and no weights. Bold is synthesised. A second would raise the question of whether the
    /// size limit is per font or for the pair, and there is nothing a second buys that answers it.
    pub font: Option<FontFile>,
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
    /// The licence's name, for the line beside the skin.
    #[serde(default)]
    pub license: String,
    /// The licence in full. OFL asks that it travel with the font, and a skin is what the font
    /// travels in — so this is not a field that may be left out.
    #[serde(default)]
    pub license_text: String,
    /// The bytes, base64. A block scalar leaves its wrapping newlines in the value, so what arrives
    /// here is not clean base64 and is not decoded until the check strips them.
    #[serde(default)]
    pub data: String,
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
            font: w.font_file,
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
    warnings: &mut Vec<Warning>,
) {
    let Some(asked) = wrote.trim().parse::<f32>().ok().filter(|n| n.is_finite() && *n > 0.0) else {
        warnings.push(Warning::Scale {
            theme: side,
            key: key.to_string(),
            wrote: wrote.to_string(),
            used: None,
        });
        return;
    };
    let used = if key == SCALE_UNBOUNDED { asked } else { asked.clamp(SCALE_MIN, SCALE_MAX) };
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

/// A CSS length in pixels, or `None` where it is not one this build can read. Everything has to say
/// `px` — it is the only unit these two are written in, and a bare `0` never reaches here anyway:
/// YAML reads it as a number, which the check has already dropped as a value that is not text.
fn px(value: &str) -> Option<f32> {
    let v = value.trim();
    v.strip_suffix("px")?.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

/// The bytes of one embedded font, or why it was set aside.
///
/// The newlines a block scalar leaves in the value are taken out first: what a YAML parser hands
/// back for `data: |` is wrapped at the column the author's editor wrapped it at, which is not
/// base64 any decoder accepts. Whitespace is all that is stripped — anything else that does not
/// decode is the file saying it is not what it claims.
fn read_font(file: &FontFile) -> Result<Vec<u8>, FontProblem> {
    use base64::Engine as _;

    if file.family.trim().is_empty() {
        return Err(FontProblem::NoFamily);
    }
    if file.format.trim() != "woff2" {
        return Err(FontProblem::Format(file.format.trim().to_string()));
    }
    let packed: String = file.data.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(packed)
        .map_err(|_| FontProblem::Unreadable)?;
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
    "c-surface", "c-text", "c-text-faint", "c-text-muted", "font", "font-mono", "fw-bold",
    "fw-medium", "fw-normal", "icon-lg", "icon-md", "icon-sm", "identicon-l", "identicon-s", "lh",
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
/// **One range for the three, not one each.** They act at the same time, and a set of separate
/// ceilings is a set somebody reaches all of at once.
pub const SCALE_MIN: f32 = 0.85;
pub const SCALE_MAX: f32 = 1.30;

/// The family whose multiplier is not bounded. Nothing broke at three times the default: a shadow
/// that is too large is ugly rather than unusable, and there is nothing under it to cut off.
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

/// The most an embedded font may weigh, decoded.
///
/// Set where a Japanese face fits: a Latin-only pixel font is a few kilobytes, and DotGothic16 —
/// which carries kana and han — is 500,480 bytes as one woff2. **It is not a time budget.** Two
/// megabytes takes about 48ms from file to glyphs, which nobody waits on. What it turns away is a
/// file that is not a font at all sitting in `data`.
pub const FONT_MAX_BYTES: usize = 2 * 1024 * 1024;

/// What a woff2 file opens with. Read so that "not woff2" is what the bytes say rather than what
/// the document claims about them.
const WOFF2_MAGIC: &[u8; 4] = b"wOF2";

/// The extension a skin's file carries. The one shape a skin is kept in, and read back by everything
/// that enumerates the directory — the device's own, and an archive's entries.
pub const FILE_EXT: &str = ".yaml";

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
        let mut found: Vec<(String, Result<Skin, Error>)> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let file = e.file_name().to_string_lossy().into_owned();
                let name = file.strip_suffix(FILE_EXT)?.to_string();
                if !usable_name(&name) {
                    return None;
                }
                let read = std::fs::read_to_string(e.path())
                    .map_err(Error::from)
                    .and_then(|yaml| Skin::read(&yaml));
                Some((name, read))
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        // A file under a shipped name cannot arrive through `install`, and one put there by hand
        // is not a second entry under that name — the shipped one is what `installed` answers with,
        // and a list that showed both would be showing a skin nothing can reach.
        found.retain(|(name, _)| !crate::skin_official::is_official(name));
        out.append(&mut found);
        out
    }

    /// Keep this document on the device under `name`, replacing whatever was there. The bytes are
    /// the author's own — a skin may carry a licence text and a font, and re-writing it from what
    /// was parsed would hand on a different file from the one that arrived.
    ///
    /// Written aside and renamed into place, so a reader never sees half a skin.
    pub fn install(paths: &crate::config::Paths, name: &str, yaml: &str) -> Result<(), Error> {
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
        let dest = paths.skin_file(name);
        let tmp = dir.join(format!("{name}{FILE_EXT}.tmp"));
        std::fs::write(&tmp, yaml)?;
        std::fs::rename(&tmp, &dest)?;
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
        match std::fs::remove_file(paths.skin_file(name)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(Error::from(e)),
        }
    }

    pub fn installed(paths: &crate::config::Paths, name: &str) -> Result<Option<Skin>, Error> {
        if !usable_name(name) {
            return Ok(None);
        }
        if let Some(yaml) = crate::skin_official::yaml(name) {
            return Skin::read(yaml).map(Some);
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
    /// The embedded font was set aside. The colours are taken either way — a look built on a face
    /// nobody can read still has its palette, and refusing the file over it would throw that away.
    FontDropped(FontProblem),
}

/// Why an embedded font was set aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontProblem {
    /// Not `woff2`. It is the one format taken, so there is nothing to try.
    Format(String),
    /// The base64 did not decode, or what came out is not a woff2 file.
    Unreadable,
    /// Larger than [`FONT_MAX_BYTES`].
    TooLarge { bytes: usize },
    /// No family name, so there is nothing to put at the head of the stack.
    NoFamily,
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
            FontProblem::Unreadable => "the data is not a woff2 file".to_string(),
            FontProblem::TooLarge { bytes } => {
                format!("{bytes} bytes, over the {FONT_MAX_BYTES} this build takes")
            }
            FontProblem::NoFamily => "no family name to put at the head of the stack".to_string(),
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

        // The font, before the tables: its one refusal is about the whole document, and a licence
        // nobody can read is not a thing to get past by dropping the face and going on.
        let mut font_bytes = None;
        if let Some(file) = &self.font {
            if file.license_text.trim().is_empty() {
                return Err(Refusal::FontWithoutLicenceText);
            }
            match read_font(file) {
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
            for (key, value) in std::mem::take(&mut table.values) {
                if let Some((_, moves)) = SCALES.iter().find(|(name, _)| *name == key) {
                    scale(side, &key, &value, moves, &mut kept, &mut warnings);
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
            frame(side, table, &mut warnings);
        }

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
        .check()
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
    fn a_value_written_to_get_out_of_the_declaration_is_dropped_with_the_reason_named() {
        // The case the task opens with: the file is read, the check passes it, the screen does not
        // change, and nothing anywhere says why. Now the name is dropped and the author is told.
        let taken = Skin::read(&doc(
            "skin_v: 1\nthemes: [light]\n",
            "light:\n  c-bg: \"red; } body { display: none }\"\n  c-text: \"#000\"\n",
        ))
        .unwrap()
        .check()
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

    /// A skin document carrying a font, built from the bytes the test wants in it.
    fn with_font(fields: &str, bytes: &[u8]) -> String {
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        format!(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\nfont_file:\n{fields}  data: {data}\n"
        )
    }

    /// The four fields a usable font needs, with the bytes left to the caller.
    const FONT_HEAD: &str =
        "  family: Silkscreen\n  format: woff2\n  license: OFL-1.1\n  license_text: Copyright…\n";

    /// A skin setting the frame however the case wants it.
    fn framed(lines: &str) -> Taken {
        Skin::read(&format!("name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n{lines}"))
            .unwrap()
            .check()
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
    fn a_font_that_is_what_it_says_comes_through_decoded() {
        let bytes = [b"wOF2".as_slice(), &[0u8; 64]].concat();
        let taken = Skin::read(&with_font(FONT_HEAD, &bytes)).unwrap().check().unwrap();
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        assert_eq!(taken.font.as_deref(), Some(bytes.as_slice()));
        assert_eq!(taken.skin.font.as_ref().unwrap().family, "Silkscreen");
        assert_eq!(taken.skin.font.as_ref().unwrap().license, "OFL-1.1");
    }

    #[test]
    fn the_newlines_a_block_scalar_leaves_in_are_not_the_fonts_fault() {
        // What a parser hands back for `data: |` is wrapped at the column the author's editor
        // wrapped it at, which is not base64 any decoder takes.
        use base64::Engine as _;
        let bytes = [b"wOF2".as_slice(), &[7u8; 200]].concat();
        let wrapped: String = base64::engine::general_purpose::STANDARD
            .encode(&bytes)
            .as_bytes()
            .chunks(76)
            .map(|line| format!("    {}\n", std::str::from_utf8(line).unwrap()))
            .collect();
        let yaml = format!(
            "name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\nfont_file:\n{FONT_HEAD}  data: |\n{wrapped}"
        );
        let taken = Skin::read(&yaml).unwrap().check().unwrap();
        assert_eq!(taken.font.as_deref(), Some(bytes.as_slice()));
    }

    #[test]
    fn a_font_that_is_not_what_it_says_is_set_aside_and_the_colours_are_taken() {
        let cases: Vec<(String, FontProblem)> = vec![
            (
                with_font("  family: F\n  format: ttf\n  license: X\n  license_text: Y\n", b"wOF2...."),
                FontProblem::Format("ttf".into()),
            ),
            (
                // Says woff2 and is not: the bytes are read rather than the claim about them.
                with_font(FONT_HEAD, b"not a font at all"),
                FontProblem::Unreadable,
            ),
            (
                with_font(
                    "  family: \"\"\n  format: woff2\n  license: X\n  license_text: Y\n",
                    b"wOF2....",
                ),
                FontProblem::NoFamily,
            ),
        ];
        for (yaml, why) in cases {
            let taken = Skin::read(&yaml).unwrap().check().unwrap();
            assert_eq!(taken.warnings, [Warning::FontDropped(why)], "{yaml}");
            assert!(taken.font.is_none());
            assert!(taken.skin.font.is_none(), "and the family is not carried on");
            assert_eq!(taken.skin.light.values["c-bg"], "#fff", "the colours are taken");
        }
    }

    #[test]
    fn a_font_past_the_size_this_build_takes_is_set_aside_with_its_weight() {
        let bytes = [b"wOF2".as_slice(), &vec![0u8; FONT_MAX_BYTES]].concat();
        let taken = Skin::read(&with_font(FONT_HEAD, &bytes)).unwrap().check().unwrap();
        assert_eq!(taken.warnings, [Warning::FontDropped(FontProblem::TooLarge { bytes: bytes.len() })]);
        assert!(taken.font.is_none());
    }

    #[test]
    fn a_font_with_no_licence_text_turns_the_whole_skin_away() {
        // The one thing about a font that is not dropped and gone on with: the terms it may be
        // passed on under have to travel in the file it travels in.
        let yaml = with_font(
            "  family: F\n  format: woff2\n  license: OFL-1.1\n  license_text: \"  \"\n",
            b"wOF2....",
        );
        assert_eq!(
            Skin::read(&yaml).unwrap().check().unwrap_err(),
            Refusal::FontWithoutLicenceText
        );
    }

    #[test]
    fn a_skin_that_carries_no_font_says_so_rather_than_warning_about_one() {
        let taken = Skin::read("name: n\ntitle: t\nskin_v: 1\nthemes: [light]\nlight:\n  c-bg: \"#fff\"\n")
            .unwrap()
            .check()
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
