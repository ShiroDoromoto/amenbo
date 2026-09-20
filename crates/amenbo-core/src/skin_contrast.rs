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
    ("c-brand-mark", "#000000", "#ffffff"),
    ("c-code-attribute", "#2f6aa8", "#7fb0e8"),
    ("c-code-comment", "#746b59", "#928a7e"),
    ("c-code-constant", "#a35a24", "#e0a04a"),
    ("c-code-function", "#2f6aa8", "#7fb0e8"),
    ("c-code-heading", "#2f6aa8", "#7fb0e8"),
    ("c-code-invalid", "#c0392b", "#e37166"),
    ("c-code-keyword", "#9a4f8c", "#cf8fc4"),
    ("c-code-number", "#a35a24", "#e0a04a"),
    ("c-code-operator", "#6f6a5e", "#a8a399"),
    ("c-code-string", "#3c7849", "#86c98f"),
    ("c-code-tag", "#9a4f8c", "#cf8fc4"),
    ("c-code-type", "#0d7777", "#2ba6a4"),
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
    ("c-rule", "#beb6a3", "#514f48"),
    ("c-stop", "#c0392b", "#e37166"),
    ("c-sunken", "#f1efe9", "#1f1e1b"),
    ("c-surface", "#ffffff", "#242320"),
    ("c-text", "#23211c", "#ece9e1"),
    ("c-text-faint", "#9b958a", "#7d786e"),
    ("c-text-muted", "#6f6a5e", "#a8a399"),
    ("k-mail", "#4e6a7d", "#4e6a7d"),
    ("k-slack", "#611f69", "#611f69"),
];

/// Every other name a skin may set, as this build sets it. Not colours: a frame, the font stacks,
/// the weights, the icon and identicon steps, the line height, the reading widths.
///
/// **One value, not one per side.** Each of these is declared once in `:root` and no side overrides
/// it, so a template that wrote two would be offering a difference the stylesheet does not make.
/// The document still carries them per side, because that is the shape a skin has — an author who
/// wants a heavier frame in the dark writes it there and leaves the light one alone.
///
/// Held against `app/src/styles/tokens.css` by `guards/check-skin-vocabulary.sh`, the way the
/// colours are: a value moved there and not here would put a number into a template that no screen
/// is drawn at.
const PLAIN: &[(&str, &str)] = &[
    ("border-style", "solid"),
    ("border-w", "1px"),
    ("font", "system-ui, -apple-system, \"Hiragino Sans\", \"Noto Sans JP\", sans-serif"),
    ("font-mono", "ui-monospace, \"SFMono-Regular\", \"Menlo\", monospace"),
    ("font-smooth", "antialiased"),
    ("fw-bold", "680"),
    ("fw-medium", "550"),
    ("fw-normal", "400"),
    ("icon-lg", "24px"),
    ("icon-md", "20px"),
    ("icon-sm", "16px"),
    ("identicon-l", "52%"),
    ("identicon-s", "58%"),
    ("lh", "1.5"),
    ("measure-form", "28rem"),
    ("measure-prose", "40rem"),
];

/// The header keys an author writes and this build has no value for, with the shape each one takes.
/// Written commented out: there is nothing to fill in for somebody's own name or their own licence,
/// and a blank string written as a value is a value. Where the skin a template is taken from carries
/// one, that one is written instead.
const HEADERS: &[(&str, &str)] = &[
    ("author", "\"your name\""),
    ("version", "\"1.0\""),
    ("license", "\"CC BY 4.0\""),
    ("homepage", "\"https://example.org/my-skin\""),
];

/// What each name is for, in one line, written beside it in a template. An author edits a value
/// they can place; a list of names is a list of guesses.
///
/// English, on both faces and in every language the application is read in. The file goes from
/// person to person as an attachment, and a template written in the language of whoever generated
/// it is a file its next reader cannot follow.
const ABOUT: &[(&str, &str)] = &[
    ("border-style", "how an ordinary line is drawn: solid, double, or none"),
    ("border-w", "how thick an ordinary line is, up to 4px"),
    ("c-accent", "the way in: a link, the press that acts, the tab that is on"),
    ("c-accent-faint", "the faintest wash of the accent, for a ground"),
    ("c-accent-text", "the accent, dark enough to read as words"),
    ("c-accent-weak", "a tint of the accent, for the row that is selected"),
    ("c-ai", "the AI facet"),
    ("c-bg", "the page"),
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
    ("c-rule", "a separator: a row's underline, a card's edge"),
    ("c-stop", "nothing moves until a hand is put to it"),
    ("c-sunken", "a well: a code block, a field set into the page"),
    ("c-surface", "a card, standing on the page"),
    ("c-text", "what is read"),
    ("c-text-faint", "not for words: a separator mark, the pale side of an icon"),
    ("c-text-muted", "read when looked for: a date, a count, a note"),
    ("font", "the stack the interface is set in"),
    ("font-mono", "the stack a path, a command and a file are set in"),
    ("font-smooth", "how the glyphs are drawn: antialiased, none, or auto"),
    ("fs-scale", "the type ladder, all four steps at once"),
    ("fw-bold", "the weight a heading is drawn at"),
    ("fw-medium", "the weight a label and a chosen row are drawn at"),
    ("fw-normal", "the weight ordinary text is drawn at"),
    ("icon-lg", "an icon on a banner or an onboarding page"),
    ("icon-md", "an icon in a heading or a chip"),
    ("icon-sm", "an icon in the nav, or set in a line of text"),
    ("identicon-l", "how light the generated avatar is drawn"),
    ("identicon-s", "how strong the colour of that avatar is"),
    ("lh", "the height of a line of text"),
    ("measure-form", "how wide a box a value is typed into is let get"),
    ("measure-prose", "how wide a run of prose is let get"),
    ("r-scale", "how round the corners are, all three steps at once"),
    ("s-scale", "the spacing ladder, all six steps at once"),
    ("shadow-scale", "how far a shadow is thrown"),
];

/// A skin document, written out full: every name a skin may set, on both sides, with a line saying
/// what each one is for. An author starts from the screen in front of them and edits values, rather
/// than from an empty file and a vocabulary to look up.
///
/// `from` is the skin the values are taken from — the one that is on — or `None` for this build's
/// own. Either way **every** name is written, so a file that set ten colours comes back out with
/// all of them: what the author was handed is filled in from the base, and nothing they might want
/// to change is missing from the page.
///
/// **It is not a copy.** The name it gives itself is not the name it was taken from, because a file
/// that calls itself what is already held replaces that one on the way back in.
///
/// What is written as a value is everything that has one — the colours, the frame, the fonts, the
/// weights and widths, and the four multipliers at whatever was asked for, `1` where nothing was.
/// What is written as a
/// comment is what only the author can fill in: their name, their licence, a name per language, a
/// font carried in the skin. A commented line is a shape to copy; a value is a value, and a template
/// that put the author's own licence inside a comment would be handing back something they could not
/// paste out again.
///
/// The names a skin may **not** set are not written at all: a template that offered them would be
/// teaching the author a line the check then drops.
///
/// A multiplier is read from where the check put it (`ThemeTable::scales`) rather than from the
/// table, because applying one leaves the sizes it moved and not the number behind them.
pub fn template(name: &str, from: Option<&Skin>) -> String {
    let mut out = String::new();
    out.push_str(&format!("name: {name}\n"));
    out.push_str(&format!("title: {name}\n"));
    out.push_str("skin_v: 1\n");
    out.push_str("themes: [light, dark]\n");
    write_headers(&mut out, from);
    write_font(&mut out, from);
    for side in [Side::Light, Side::Dark] {
        out.push_str(&format!("\n{side}:\n"));
        for (token, light, dark) in
            BASE.iter().filter(|(n, _, _)| crate::skin::OPEN.binary_search(n).is_ok())
        {
            let base = match side {
                Side::Light => light,
                Side::Dark => dark,
            };
            write_token(&mut out, side, from, token, base);
        }
        for (token, base) in PLAIN {
            write_token(&mut out, side, from, token, base);
        }
        for (token, _) in crate::skin::SCALES {
            let asked = from.and_then(|s| s.side(side).scales.get(*token)).map(String::as_str);
            let value = asked
                .or_else(|| from.and_then(|s| s.side(side).values.get(*token)).map(String::as_str))
                .unwrap_or("1");
            out.push_str(&format!(
                "  {token}: {}  # {}{}\n",
                quoted(value),
                about(token),
                scale_range(token)
            ));
        }
    }
    out
}

/// One line of a side's table: the value the skin being taken from set, or this build's own, with
/// what the name is for written beside it.
fn write_token(out: &mut String, side: Side, from: Option<&Skin>, token: &str, base: &str) {
    let value =
        from.and_then(|s| s.side(side).values.get(token)).map(String::as_str).unwrap_or(base);
    out.push_str(&format!("  {token}: {}  # {}\n", quoted(value), about(token)));
}

/// The multipliers a family takes, as a phrase. Written out rather than left to the check, because a
/// number with no range beside it is one an author finds the edge of by being told they went past it.
fn scale_range(family: &str) -> String {
    use crate::skin::{SCALE_MAX, SCALE_MIN, SCALE_TO_ZERO, SCALE_UNBOUNDED};
    let floor = if SCALE_TO_ZERO.contains(&family) { 0.0 } else { SCALE_MIN };
    if family == SCALE_UNBOUNDED {
        format!(" — {floor} and up")
    } else {
        format!(" — {floor} to {SCALE_MAX}")
    }
}

/// What a name is for, or nothing where this build has no line for it.
fn about(token: &str) -> &'static str {
    ABOUT.binary_search_by_key(&token, |(n, _)| n).map(|at| ABOUT[at].1).unwrap_or("")
}

/// One value, as a YAML double-quoted scalar. A font stack names its families in quotes of its own,
/// and pasted between two more it would end the scalar in the middle of a family name.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The header keys that are the author's own: their name, their version, their licence, where the
/// skin lives, and the name per language. Written as values where the skin being taken from carries
/// them, and as commented shapes where it does not.
fn write_headers(out: &mut String, from: Option<&Skin>) {
    out.push_str("\n# The author's own. Uncomment what applies; a line left out costs nothing.\n");
    for (key, shape) in HEADERS {
        let held = from.and_then(|s| match *key {
            "author" => s.author.as_deref(),
            "version" => s.version.as_deref(),
            "license" => s.license.as_deref(),
            _ => s.homepage.as_deref(),
        });
        match held {
            Some(value) => out.push_str(&format!("{key}: {}\n", quoted(value))),
            None => out.push_str(&format!("# {key}: {shape}\n")),
        }
    }
    let titles = from.map(|s| &s.titles).filter(|t| !t.is_empty());
    match titles {
        Some(titles) => {
            out.push_str("titles:  # the name on screen, per language\n");
            for (language, title) in titles {
                out.push_str(&format!("  {language}: {}\n", quoted(title)));
            }
        }
        None => {
            out.push_str("# titles:  # the name on screen per language; `title` stands where there is none\n");
            out.push_str("#   ja: \"みずいろ\"\n");
            out.push_str("#   fr: \"Bleu d'eau\"\n");
        }
    }
}

/// The one font a skin may carry. Written out where the skin being taken from has one — the family,
/// the licence and the file it is in — and as a commented shape where it does not.
///
/// **The face itself does not come along.** What is written here is a document, and the file it
/// names sits beside that document in the skin's zip (`AMB-D-936`); an author starting from this
/// puts the woff2 next to it under the name written here.
fn write_font(out: &mut String, from: Option<&Skin>) {
    let Some(font) = from.and_then(|s| s.font.as_ref()) else {
        out.push_str("\n# One face, carried in the skin so it travels with the colours: the woff2\n");
        out.push_str("# sits beside this document, up to 2MB, and the licence in full goes here — a\n");
        out.push_str("# skin carrying a font and no licence text is turned away.\n");
        out.push_str("# font_file:\n");
        out.push_str("#   family: \"My Face\"\n");
        out.push_str("#   format: woff2\n");
        out.push_str("#   file: \"my-face.woff2\"\n");
        out.push_str("#   license: \"SIL Open Font License 1.1\"\n");
        out.push_str("#   license_text: |\n");
        out.push_str("#     the licence, in full\n");
        return;
    };
    out.push_str("\nfont_file:\n");
    out.push_str(&format!("  family: {}\n", quoted(&font.family)));
    out.push_str(&format!("  format: {}\n", quoted(&font.format)));
    out.push_str(&format!("  file: {}\n", quoted(&font.file)));
    out.push_str(&format!("  license: {}\n", quoted(&font.license)));
    out.push_str("  license_text: |2\n");
    for line in font.license_text.lines() {
        out.push_str(&format!("    {line}\n"));
    }
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
/// pairing of which clears its floor, on a screen where every ground is a colour.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// The pairings under their floor, worst first.
    pub short: Vec<Reading>,
    /// The colours this build could not read a number from.
    pub unread: Vec<Unread>,
    /// The grounds a picture is laid over, in the order the pairings walk them. Nothing over one of
    /// these was measured: what a ratio would be saying is that a colour is under the text, and
    /// what is under it is a picture (`AMB-D-936`).
    ///
    /// Named by the ground rather than by the pairing. A picture behind `c-surface` takes fifteen
    /// pairings out at once, and fifteen rows saying the same sentence is not fifteen things a
    /// reader wants to know.
    pub covered: Vec<&'static str>,
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
    measure_at(skin, TEXT, ASIDE)
}

/// The same measuring, against floors the caller names. **Not a setting** — the floors a skin is
/// held to are [`TEXT`] and [`ASIDE`], and nothing offers to move them. It is here because one
/// skin promises more than it is held to: `high-contrast` is the way back from an unreadable
/// screen, so it clears AAA (7:1), and a promise with nothing measuring it is a promise that goes
/// quietly wrong the first time a colour is touched.
pub fn measure_at(skin: &Skin, text: f64, aside: f64) -> Report {
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
        for (ink, ground, written) in pairs {
            // A picture over the ground, and the two colours no longer say whether the text can be
            // read. Set aside rather than measured against the colour underneath: a number that
            // cleared its floor would be read as a verdict on a screen nobody has measured.
            if skin.backgrounds.contains_key(ground) {
                if !report.covered.contains(&ground) {
                    report.covered.push(ground);
                }
                continue;
            }
            let floor = if written == TEXT { text } else { aside };
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
    use crate::skin::Materials;

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
    fn nothing_in_this_builds_palette_falls_short() {
        // Not a test of the skin path: it is this build's palette, measured through the pairings
        // declared above. Every one of them is a number that can be read, and every one clears its
        // floor — a file's own thirteen colours included, since `guards/check-token-contrast.sh`
        // now measures them over the same two grounds.
        let report = measure(&skin("[light, dark]", "light:\n  c-bg: \"#f7f6f3\"\ndark:\n  c-bg: \"#1b1a17\"\n"));
        assert_eq!(report.measured, 2 * (PAIRINGS.len() + CODE_INKS.len() * CODE_GROUNDS.len()));
        assert!(report.unread.is_empty(), "{:?}", report.unread);
        assert!(report.is_clear(), "{:?}", report.short);
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
    fn a_ground_with_a_picture_over_it_is_set_aside_rather_than_measured() {
        // The text over it may be perfectly readable or not readable at all, and the two colours
        // no longer say which. Read through the check, since the backgrounds it drops are not laid.
        let taken = skin(
            "[light]",
            "light:\n  c-bg: \"#fff\"\nbackgrounds:\n  c-surface:\n    file: paper.png\n",
        )
        .check(&crate::skin::Materials::None)
        .unwrap();
        let report = measure(&taken.skin);
        assert_eq!(report.covered, ["c-surface"]);
        assert!(
            !report.short.iter().any(|r| r.ground == "c-surface"),
            "{:?}",
            report.short
        );
        // Every pairing on that ground, once: the three that name it and the thirteen inks a file
        // is read in.
        let over_it = PAIRINGS.iter().filter(|(_, g, _)| *g == "c-surface").count() + CODE_INKS.len();
        let all = PAIRINGS.len() + CODE_INKS.len() * CODE_GROUNDS.len();
        assert_eq!(report.measured, all - over_it);
    }

    #[test]
    fn the_grounds_a_picture_is_not_over_are_measured_as_they_were() {
        let taken = skin(
            "[light]",
            "light:\n  c-text: \"#f8f7f4\"\nbackgrounds:\n  c-bg:\n    file: paper.png\n",
        )
        .check(&crate::skin::Materials::None)
        .unwrap();
        let report = measure(&taken.skin);
        assert_eq!(report.covered, ["c-bg"]);
        // Near-white text still falls on the two grounds no picture is over.
        let fell: Vec<_> = report.short.iter().filter(|r| r.ink == "c-text").collect();
        assert_eq!(fell.len(), 2, "{:?}", report.short);
        assert!(fell.iter().all(|r| r.ground != "c-bg"));
    }

    #[test]
    fn a_skin_that_lays_no_picture_is_measured_exactly_as_before() {
        let taken =
            skin("[light]", "light:\n  c-bg: \"#fff\"\n").check(&crate::skin::Materials::None).unwrap();
        let report = measure(&taken.skin);
        assert!(report.covered.is_empty());
        assert_eq!(report.measured, PAIRINGS.len() + CODE_INKS.len() * CODE_GROUNDS.len());
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

    /// How many names one side of a checked template holds: the ones written directly, and the
    /// ones the four multipliers moved (a multiplier is not itself a token).
    fn settled() -> usize {
        crate::skin::OPEN.len()
            + crate::skin::SCALES.iter().map(|(_, moves)| moves.len()).sum::<usize>()
    }

    /// Every name the template writes, in the order the file writes them.
    fn written() -> Vec<&'static str> {
        let colours = BASE
            .iter()
            .filter(|(n, _, _)| crate::skin::OPEN.binary_search(n).is_ok())
            .map(|(n, _, _)| *n);
        let plain = PLAIN.iter().map(|(n, _)| *n);
        let scales = crate::skin::SCALES.iter().map(|(n, _)| *n);
        colours.chain(plain).chain(scales).collect()
    }

    #[test]
    fn every_name_the_template_writes_says_what_it_is_for() {
        let mut sorted = ABOUT.to_vec();
        sorted.sort_unstable_by_key(|(n, _)| *n);
        assert_eq!(ABOUT, sorted, "ABOUT is in order");
        let mut plain = PLAIN.to_vec();
        plain.sort_unstable_by_key(|(n, _)| *n);
        assert_eq!(PLAIN, plain, "PLAIN is in order");
        for name in written() {
            assert!(
                ABOUT.binary_search_by_key(&name, |(n, _)| n).is_ok(),
                "{name} is written into a template with nothing said about it"
            );
        }
        for (name, _) in ABOUT {
            assert!(written().contains(name), "{name} is said and no longer written");
        }
    }

    #[test]
    fn the_template_carries_every_name_a_skin_may_set() {
        let yaml = template("my-skin", None);
        for name in crate::skin::OPEN.iter().chain(crate::skin::SCALES.iter().map(|(n, _)| n)) {
            assert!(
                yaml.contains(&format!("  {name}: ")),
                "{name} may be set and the template does not offer it"
            );
        }
        // The multipliers say how far they go, which the value alone does not.
        assert!(yaml.contains("fs-scale: \"1\"") && yaml.contains("0.85 to 1.3"));
        assert!(yaml.contains("shadow-scale: \"1\"") && yaml.contains("0 and up"));
        // What only the author can fill in is offered as a shape to copy.
        for shape in ["# author:", "# license:", "# titles:", "# font_file:"] {
            assert!(yaml.contains(shape), "{shape} is not offered");
        }
    }

    #[test]
    fn a_template_taken_from_a_skin_keeps_its_values_and_fills_in_the_rest() {
        let washi = Skin::read(
            "name: washi\ntitle: t\nskin_v: 1\nthemes: [light, dark]\nlight:\n  c-bg: \"#faf7f0\"\ndark:\n  c-bg: \"#1a1713\"\n",
        )
        .unwrap()
        .check(&Materials::None)
        .unwrap()
        .skin;

        assert_eq!(template_name(Some(&washi)), "washi-copy", "not the name it was taken from");
        let yaml = template(&template_name(Some(&washi)), Some(&washi));
        let out = Skin::read(&yaml).unwrap().check(&Materials::None).unwrap().skin;
        assert_eq!(out.name, "washi-copy");
        assert_eq!(out.light.values["c-bg"], "#faf7f0", "what the author set");
        assert_eq!(out.light.values["c-text"], base("c-text", Side::Light), "and the rest, filled in");
        assert_eq!(out.light.values.len(), settled(), "every name, not the one it set");
    }

    #[test]
    fn a_template_taken_from_a_skin_carries_what_only_its_author_could_write() {
        let bytes = [b"wOF2".as_slice(), &[7u8; 200]].concat();
        let document = "name: mine\ntitle: Mine\nskin_v: 1\nthemes: [light, dark]\n\
             author: Alice\nversion: \"2.1\"\nlicense: CC BY 4.0\nhomepage: https://example.org/mine\n\
             titles:\n  ja: \"わたしの\"\n\
             font_file:\n  family: Pixel\n  format: woff2\n  file: pixel.woff2\n  license: OFL 1.1\n\
             \x20 license_text: |\n    OFL, in full\n      an indented clause\n\
             light:\n  c-bg: \"#faf7f0\"\ndark:\n  c-bg: \"#1a1713\"\n";
        let zip = crate::skin::packed(&[
            ("pixel.woff2", &bytes),
            (crate::skin::PACK_DOCUMENT, document.as_bytes()),
        ]);
        let carries = Materials::Pack(std::borrow::Cow::Owned(zip));
        let mine = Skin::read(document).unwrap().check(&carries).unwrap().skin;

        let yaml = template("mine-copy", Some(&mine));
        // Checked against the same skin's materials: what the template writes is the document, and
        // the file it names is the one still sitting in the zip the author wrote it in.
        let out = Skin::read(&yaml).unwrap().check(&carries).expect("still a skin");
        assert_eq!(out.skin.author.as_deref(), Some("Alice"), "the author's own, not the shape");
        assert_eq!(out.skin.version.as_deref(), Some("2.1"));
        assert_eq!(out.skin.license.as_deref(), Some("CC BY 4.0"));
        assert_eq!(out.skin.homepage.as_deref(), Some("https://example.org/mine"));
        assert_eq!(out.skin.titles.get("ja").map(String::as_str), Some("わたしの"));
        let font = out.skin.font.as_ref().expect("the face travels with the file");
        assert_eq!(font.family, "Pixel");
        assert_eq!(font.file, "pixel.woff2", "the file it is in, by name");
        assert!(font.license_text.contains("an indented clause"), "the licence, as written");
        assert_eq!(out.font.as_deref(), Some(bytes.as_slice()), "and the bytes behind that name");
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    }

    #[test]
    fn a_ladder_the_author_asked_for_comes_back_out_of_the_template() {
        let wide = Skin::read(
            "name: wide\ntitle: t\nskin_v: 1\nthemes: [light, dark]\n\
             light:\n  fs-scale: \"1.2\"\n  shadow-scale: \"0\"\n  r-scale: \"9\"\n\
             dark:\n  fs-scale: \"1.2\"\n",
        )
        .unwrap()
        .check(&Materials::None)
        .unwrap()
        .skin;

        let yaml = template("wide-copy", Some(&wide));
        let out = Skin::read(&yaml).unwrap().check(&Materials::None).unwrap().skin;
        assert_eq!(out.light.scales["fs-scale"], "1.2", "the ladder the author wrote");
        assert_eq!(out.dark.scales["fs-scale"], "1.2", "on the side they wrote it");
        assert_eq!(out.light.scales["shadow-scale"], "0", "and the one they took away");
        // What was brought inside the range comes back as the number that was used, so a second
        // pass through the template does not report the same value a second time.
        assert_eq!(out.light.scales["r-scale"], format!("{}", crate::skin::SCALE_MAX));
        assert_eq!(out.dark.scales["r-scale"], "1", "untouched on the side that left it alone");
    }

    #[test]
    fn the_template_is_a_skin_that_reads_back_clear() {
        let yaml = template("my-skin", None);
        let taken = Skin::read(&yaml).unwrap().check(&Materials::None).expect("the template is a skin");
        assert!(taken.warnings.is_empty(), "{:?}", taken.warnings);
        assert_eq!(taken.skin.light.values.len(), settled(), "every name a skin may set");
        assert_eq!(taken.skin.dark.values.len(), settled());
        assert!(!yaml.contains("k-slack"), "and none it may not");
        assert!(yaml.contains("# what is read"), "each one says what it is for");
        // The values are this build's, so what the template measures is what the screen measures.
        let from_template = measure(&taken.skin);
        assert!(from_template.unread.is_empty());
        assert!(from_template.is_clear(), "{:?}", from_template.short);
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
