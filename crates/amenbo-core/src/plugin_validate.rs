//! The plugin manifest validator — the **one place** the manifest rules live (`AMB-D-354`).
//!
//! A manifest is untrusted third-party input ([`crate::plugin_manifest`] is the *shape*; serde rejects one
//! missing a required field). The *rules* on top of that shape — a name that fits the id grammar, a
//! well-formed checksum, a non-empty OS set, an amenbo floor that reads as a version, a config schema
//! within the safe floor — are here, so the one truth about them lives in one function. Two callers
//! share it (`AMB-D-354`):
//!
//! - **the door** (install / catalog intake, `AMB-T-1979`, future): fail-closed — a manifest with any
//!   problem is refused, never listed or installed.
//! - **`plugin validate`** (`AMB-T-1988`): an author self-checks a manifest before opening a catalog PR,
//!   seeing *every* problem at once rather than one per run.
//!
//! The catalog is delivered as two documents (`AMB-D-385`), so intake meets a manifest in halves: the
//! fetch of the list has only a list entry in hand and checks it with [`validate_list_entry`], and the
//! install door has both halves joined and checks the whole thing with [`validate_manifest`]. Same rules,
//! applied to whichever fields are present — a list entry is not refused for lacking a checksum it does
//! not carry, and the checksum is still refused where it does.
//!
//! Because both go through [`validate_manifest`], the door and the author's tool can never disagree about
//! what "valid" means. The validator **collects** problems ([`Vec<Problem>`], empty ⇒ valid) rather than
//! returning on the first: the door only asks "is it empty", while the author wants the whole list.
//!
//! **What it does not do.** It never judges *meaning* — that a URL resolves, that a category is one the
//! catalog curates, that a signature verifies (`AMB-T-1976`'s job at download time). It checks shape and
//! the safe floor only: enough to keep a malformed or hostile manifest from breaking a path, a display, or
//! the store (`AMB-D-354`/`AMB-D-356`). Output validation (a plugin's stdout/exit at run time) is a
//! different boundary and lives with the runners (`AMB-D-354`, `AMB-T-1972`/`AMB-T-1973`).

use std::collections::HashSet;

use serde::{Serialize, Serializer};

use crate::config::is_reserved_plugin_name;
use crate::error::Msg;
use crate::plugin_config::MAX_CONFIG_IDENT_BYTES;
use crate::plugin_manifest::{ConfigField, Face, FieldType, Manifest, Os, NONE_SELECTED};
use crate::plugin_wire::ListEntry;

/// The shortest a plugin id (`name`) may be (`AMB-D-360`).
pub const NAME_MIN_LEN: usize = 2;
/// The longest a plugin id (`name`) may be (`AMB-D-360`) — it becomes a directory name, a command
/// namespace and a config-key prefix, so it is kept short and strict.
pub const NAME_MAX_LEN: usize = 64;

/// The longest the one-line `desc` may be (characters). A list-view line, not a body — bounded so a
/// runaway value cannot break the catalog display.
pub const MAX_DESC_LEN: usize = 200;
/// The longest the `author` display string may be (characters).
pub const MAX_AUTHOR_LEN: usize = 100;
/// The longest the `category` label may be (characters).
pub const MAX_CATEGORY_LEN: usize = 40;
/// The longest a config field's human `label` may be (characters) — the display-name floor (`AMB-D-360`):
/// free text, but length-capped and control-char-free so a form field cannot break the layout.
pub const MAX_LABEL_LEN: usize = 100;

/// The most config fields a manifest may declare (`AMB-D-356`, the safe floor). A generous ceiling — a
/// real plugin needs a handful — whose only purpose is to stop a manifest declaring thousands of fields
/// and bloating the generated form / stored config.
pub const MAX_CONFIG_FIELDS: usize = 32;
/// The largest a config schema may be in total, summed over every field's declared text — its key and
/// label, its candidates' values and labels, and its default (`AMB-D-356`, the safe floor). Bounds the
/// schema as a whole, complementing the per-field caps: candidates are counted here because a handful of
/// fields can still carry a great deal of them.
pub const MAX_CONFIG_SCHEMA_BYTES: usize = 8 * 1024;
/// The most candidates one `multi` field may offer (`AMB-D-415`, the same safe floor the schema keeps).
/// Every one of them is a checkbox in the form, so the ceiling is what keeps a field a choice rather than
/// a catalog.
pub const MAX_CONFIG_OPTIONS: usize = 64;
/// The longest one candidate's stored `value` may be (characters) — it is a value that travels to the
/// plugin, not a sentence, and it is joined with its siblings into one stored string (`AMB-D-415`).
pub const MAX_OPTION_VALUE_LEN: usize = 100;
/// The longest a text field's `default` may be (characters). A default is a seed for a line the user
/// types, so it is bounded like one; a `multi` field's default is bounded by its candidates instead.
pub const MAX_DEFAULT_LEN: usize = 200;

/// The longest the agent block's `when` line may be (characters) — one line at the AI's entry point
/// (`AMB-D-437`), capped like every other one-line field.
pub const MAX_AGENT_WHEN_LEN: usize = 200;
/// The longest one agent command's `cmd` may be (characters). Shorter than the prose caps: it is a
/// subcommand and its arguments, not a sentence.
pub const MAX_AGENT_CMD_LEN: usize = 120;
/// The longest one agent command's `does` line may be (characters).
pub const MAX_AGENT_DOES_LEN: usize = 200;
/// The most commands one plugin may name in its agent block (`AMB-D-437`, the same safe floor the config
/// schema keeps). Every one of them is read into an AI's context on every `agent --json`, so the ceiling
/// is what stops one manifest from crowding out the document it is a guest in.
pub const MAX_AGENT_COMMANDS: usize = 16;

/// Ids reserved beyond the disk-layout name (`registry`, via [`is_reserved_plugin_name`]): the badge word
/// and amenbo's own namespace (`AMB-D-360`). Kept small on purpose — the strict id grammar plus command
/// namespacing (`AMB-D-346`, a plugin's commands are namespaced by its id) already prevent a plugin from
/// shadowing an amenbo subcommand, so this need not mirror the CLI's verb list, which would only rot.
const RESERVED_NAMES: &[&str] = &["official", "amenbo", "plugin", "plugins"];

/// A rule a manifest broke. It is the machine-stable half of a [`Problem`] — the human sentence rides in
/// [`Problem::message`]; this is the token a `--json` consumer keys on. The same pattern as
/// [`crate::validate::IssueRule`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemCode {
    /// A field that must hold a value is empty.
    Empty,
    /// A value is shorter than its floor (the id).
    TooShort,
    /// A value is longer than its cap.
    TooLong,
    /// A value holds a character its grammar forbids (the id: outside `[a-z0-9-]`).
    BadChars,
    /// The id does not start with a letter.
    MustStartLetter,
    /// The id starts or ends with `-`.
    HyphenEdge,
    /// The id contains `--`.
    DoubleHyphen,
    /// A reserved word is used where it may not be — as a plugin id, or as a `multi` candidate's `value`
    /// (the word that stores a deliberate empty choice, `AMB-D-415`).
    Reserved,
    /// A one-line text field holds a control character.
    ControlChar,
    /// The checksum is not `sha256:<64 lowercase hex>`.
    BadChecksum,
    /// The url is not an `https://` URL.
    BadUrl,
    /// The repo is not `owner/name`.
    BadRepo,
    /// The OS set is empty.
    EmptyOs,
    /// A value appears more than once where it must be unique (an OS, a config key, a `multi` field's
    /// candidate value).
    Duplicate,
    /// A declared list holds more entries than its cap — the config schema's fields, a `multi` field's
    /// candidates, an agent block's commands.
    TooManyFields,
    /// The config schema exceeds the total-size cap.
    SchemaTooLarge,
    /// A config field's key is not a valid identifier (`[a-z][a-z0-9_]*`).
    BadKey,
    /// A version field does not read as a version, so nothing can be compared against it
    /// (`min_amenbo`).
    BadVersion,
    /// The per-OS `assets` map and the declared `os` set do not answer for the same platforms
    /// (`AMB-D-381`).
    AssetMismatch,
    /// A subscription declares an empty `faces` set — a hook that fires on no face at all (`AMB-D-383`).
    EmptyFaces,
    /// A subscription asks for `reply: true` without `faces: [cli]` — output relayed to a caller no other
    /// face has (`AMB-D-383`).
    ReplyNeedsCli,
    /// A field declares `options` without being a `multi` field — candidates for something that is not a
    /// choice (`AMB-D-415`).
    OptionsNeedMulti,
    /// A `multi` field's `default` names something the field does not offer (`AMB-D-415`).
    NotAnOption,
}

impl ProblemCode {
    /// The contractual set — a CLI-side test checks the surface renders each.
    pub const ALL: &'static [ProblemCode] = &[
        Self::Empty,
        Self::TooShort,
        Self::TooLong,
        Self::BadChars,
        Self::MustStartLetter,
        Self::HyphenEdge,
        Self::DoubleHyphen,
        Self::Reserved,
        Self::ControlChar,
        Self::BadChecksum,
        Self::BadUrl,
        Self::BadRepo,
        Self::EmptyOs,
        Self::Duplicate,
        Self::TooManyFields,
        Self::SchemaTooLarge,
        Self::BadKey,
        Self::BadVersion,
        Self::AssetMismatch,
        Self::EmptyFaces,
        Self::ReplyNeedsCli,
        Self::OptionsNeedMulti,
        Self::NotAnOption,
    ];

    /// The one place a code string is written; `Serialize` goes through here too.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
            Self::BadChars => "bad_chars",
            Self::MustStartLetter => "must_start_letter",
            Self::HyphenEdge => "hyphen_edge",
            Self::DoubleHyphen => "double_hyphen",
            Self::Reserved => "reserved",
            Self::ControlChar => "control_char",
            Self::BadChecksum => "bad_checksum",
            Self::BadUrl => "bad_url",
            Self::BadRepo => "bad_repo",
            Self::EmptyOs => "empty_os",
            Self::Duplicate => "duplicate",
            Self::TooManyFields => "too_many_fields",
            Self::SchemaTooLarge => "schema_too_large",
            Self::BadKey => "bad_key",
            Self::BadVersion => "bad_version",
            Self::AssetMismatch => "asset_mismatch",
            Self::EmptyFaces => "empty_faces",
            Self::ReplyNeedsCli => "reply_needs_cli",
            Self::OptionsNeedMulti => "options_need_multi",
            Self::NotAnOption => "not_an_option",
        }
    }
}

impl Serialize for ProblemCode {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// One thing wrong with a manifest. It names *where* (a field path like `name` or `config[2].key`), *what*
/// rule broke ([`ProblemCode`], the machine token), and carries the sentence a person reads ([`Msg`]).
/// A validator run returns a `Vec` of these; empty means the manifest passed.
#[derive(Clone, Debug)]
pub struct Problem {
    /// The field path the problem is at — `name`, `os`, `checksum`, `config[2].key`.
    pub location: String,
    /// The rule that broke — the stable token a machine reads.
    pub code: ProblemCode,
    /// The sentence a person reads.
    pub message: Msg,
}

impl Problem {
    fn new(location: impl Into<String>, code: ProblemCode, en: impl Into<String>) -> Self {
        Problem { location: location.into(), code, message: Msg::new(en) }
    }
}

/// Validate a whole manifest against every rule, collecting **all** problems (`AMB-D-354`). Empty ⇒ valid.
/// The door treats any non-empty result as a fail-closed refusal; `plugin validate` shows the list.
pub fn validate_manifest(m: &Manifest) -> Vec<Problem> {
    let mut problems = Vec::new();

    problems.extend(validate_plugin_id(&m.name));
    check_line(&mut problems, "desc", &m.desc, MAX_DESC_LEN);
    check_line(&mut problems, "author", &m.author, MAX_AUTHOR_LEN);
    check_line(&mut problems, "category", &m.category, MAX_CATEGORY_LEN);
    check_repo(&mut problems, &m.repo);
    check_assets(&mut problems, m);
    check_min_amenbo(&mut problems, m.min_amenbo.as_deref());
    check_os(&mut problems, &m.os);
    check_config(&mut problems, m);
    check_events(&mut problems, m);
    check_agent(&mut problems, m);

    problems
}

/// Validate one **list** entry — the half of a catalog entry that rides in `catalog.json` (`AMB-D-385`),
/// which is all the intake of a catalog fetch has in front of it. Empty ⇒ valid, as above.
///
/// The rules are the same rules, applied to the fields that are there: an entry is not a manifest, so
/// asking it for a checksum it no longer carries would drop every well-formed entry in the catalog. What
/// an entry owes is what a browse view draws — an id that fits the grammar, one-line text within its
/// caps, GitHub coordinates, a non-empty OS set — plus the catalog's own digest being a digest
/// (`AMB-D-386`), since it is what an update is later detected by.
///
/// The rest of a manifest is checked at the install door, over the two halves joined
/// ([`crate::plugin_wire::join`]): the detail is untrusted delivery too, and that is where all of it is
/// in hand at once.
pub fn validate_list_entry(e: &ListEntry) -> Vec<Problem> {
    let mut problems = Vec::new();

    problems.extend(validate_plugin_id(&e.name));
    check_line(&mut problems, "desc", &e.desc, MAX_DESC_LEN);
    check_line(&mut problems, "author", &e.author, MAX_AUTHOR_LEN);
    check_line(&mut problems, "category", &e.category, MAX_CATEGORY_LEN);
    check_repo(&mut problems, &e.repo);
    check_os(&mut problems, &e.os);
    if let Some(sum) = &e.detail_sum {
        check_checksum(&mut problems, "detail_sum", sum);
    }

    problems
}

/// Validate a plugin id (`name`) against the grammar (`AMB-D-360`) — exposed on its own because the same
/// rules gate a name at install-time conflict resolution, not only at manifest intake. Each broken rule is
/// its own problem, so an author sees all of them at once.
pub fn validate_plugin_id(name: &str) -> Vec<Problem> {
    let mut problems = Vec::new();
    let loc = "name";

    if name.is_empty() {
        problems.push(Problem::new(loc, ProblemCode::Empty, "plugin name must not be empty"));
        return problems; // nothing else is meaningful on an empty id
    }
    if name.len() < NAME_MIN_LEN {
        problems.push(Problem::new(
            loc,
            ProblemCode::TooShort,
            format!("plugin name is too short ({} chars; min {NAME_MIN_LEN})", name.chars().count()),
        ));
    }
    if name.len() > NAME_MAX_LEN {
        problems.push(Problem::new(
            loc,
            ProblemCode::TooLong,
            format!("plugin name is too long ({} chars; max {NAME_MAX_LEN})", name.chars().count()),
        ));
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        problems.push(Problem::new(
            loc,
            ProblemCode::BadChars,
            "plugin name may use only lowercase ASCII letters, digits and '-'",
        ));
    }
    if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        problems.push(Problem::new(
            loc,
            ProblemCode::MustStartLetter,
            "plugin name must start with a lowercase letter",
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        problems.push(Problem::new(
            loc,
            ProblemCode::HyphenEdge,
            "plugin name must not start or end with '-'",
        ));
    }
    if name.contains("--") {
        problems.push(Problem::new(
            loc,
            ProblemCode::DoubleHyphen,
            "plugin name must not contain '--'",
        ));
    }
    if is_reserved_plugin_name(name) || RESERVED_NAMES.contains(&name) {
        problems.push(Problem::new(
            loc,
            ProblemCode::Reserved,
            format!("plugin name '{name}' is reserved"),
        ));
    }

    problems
}

/// Check a required one-line text field: non-empty, no control character (a newline included — these are
/// single lines), and within its character cap.
fn check_line(problems: &mut Vec<Problem>, field: &str, value: &str, max: usize) {
    if value.is_empty() {
        problems.push(Problem::new(
            field,
            ProblemCode::Empty,
            format!("{field} must not be empty"),
        ));
        return;
    }
    if value.chars().any(|c| c.is_control()) {
        problems.push(Problem::new(
            field,
            ProblemCode::ControlChar,
            format!("{field} must not contain control characters"),
        ));
    }
    let len = value.chars().count();
    if len > max {
        problems.push(Problem::new(
            field,
            ProblemCode::TooLong,
            format!("{field} is too long ({len} chars; max {max})"),
        ));
    }
}

/// Check `repo` is `owner/name`: exactly one `/`, both halves non-empty and made of GitHub-safe characters.
/// The coordinates a detail view reads stars/README from — shape only, never a network check.
fn check_repo(problems: &mut Vec<Problem>, repo: &str) {
    let ok = match repo.split_once('/') {
        Some((owner, name)) => {
            !owner.is_empty()
                && !name.is_empty()
                && !name.contains('/')
                && owner.chars().all(is_repo_char)
                && name.chars().all(is_repo_char)
        }
        None => false,
    };
    if !ok {
        problems.push(Problem::new(
            "repo",
            ProblemCode::BadRepo,
            "repo must be 'owner/name' (GitHub coordinates)",
        ));
    }
}

fn is_repo_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'
}

/// Check the entry actually publishes something, in one of the two forms a manifest may take
/// (`AMB-D-381`): one `url`/`checksum` serving every OS it lists, or one
/// [`Asset`](crate::plugin_manifest::Asset) per [`Platform`](crate::plugin_manifest::Platform) in
/// `assets` — where a platform key is an OS alone or an OS-arch pair (`AMB-D-384`).
///
/// Which form is in play is decided by `assets` alone, and the two rules that follow are what make the
/// per-platform form worth having: **every declared OS is answered by at least one key** — its
/// arch-agnostic `<os>` or any `<os>-<arch>` — so an entry cannot claim an OS it publishes nothing for,
/// and **no key names an OS the entry does not declare**, so bytes cannot be published for a platform the
/// plugin never said it runs on. Without the pair, `os` would go back to being a claim nothing checks —
/// which is the gap the decision exists to close. The coupling is kept at OS granularity (`AMB-D-384`):
/// arch subdivides beneath a declared OS, it does not have to be listed in `os` itself.
///
/// The single fields are *not* refused beside a map: `asset_for` prefers the map, and a leftover pair is
/// a document that says one thing twice, not one that says something wrong. Their shape is still checked
/// wherever they are non-empty, because an install can still be handed them by a manifest placed by hand.
fn check_assets(problems: &mut Vec<Problem>, m: &Manifest) {
    if m.assets.is_empty() {
        // The one-file form: the single pair *is* the distributable, so it is required, not optional.
        check_url(problems, "url", &m.url);
        check_checksum(problems, "checksum", &m.checksum);
        return;
    }

    // Each declared OS must be answered by at least one key — its arch-agnostic `<os>` or any of its
    // `<os>-<arch>` (`AMB-D-384`). The clean one-to-one `AMB-D-381` bought is kept at OS granularity; arch
    // may subdivide beneath it.
    for os in &m.os {
        if !m.assets.keys().any(|p| p.os == *os) {
            problems.push(Problem::new(
                "assets",
                ProblemCode::AssetMismatch,
                format!("os lists {} but assets has no distributable for it", os.as_str()),
            ));
        }
    }
    for (platform, asset) in &m.assets {
        let at = format!("assets.{}", platform.token());
        if !m.os.contains(&platform.os) {
            problems.push(Problem::new(
                at.clone(),
                ProblemCode::AssetMismatch,
                format!("assets publishes for {} but os does not list it", platform.token()),
            ));
        }
        check_url(problems, &format!("{at}.url"), &asset.url);
        check_checksum(problems, &format!("{at}.checksum"), &asset.checksum);
    }
    if !m.url.is_empty() {
        check_url(problems, "url", &m.url);
    }
    if !m.checksum.is_empty() {
        check_checksum(problems, "checksum", &m.checksum);
    }
}

/// Check `url` is an `https://` URL. Shape only (fail-closed on scheme): `http`/`file`/anything else is
/// refused so an install never fetches over a plaintext or local scheme — whether it *resolves* is the
/// download's concern (`AMB-T-1976`), not the manifest's.
fn check_url(problems: &mut Vec<Problem>, location: &str, url: &str) {
    if !url.starts_with("https://") || url.len() <= "https://".len() {
        problems.push(Problem::new(
            location,
            ProblemCode::BadUrl,
            "url must be an https:// URL",
        ));
    }
}

/// Check `checksum` is `sha256:<64 lowercase hex>` — the shape `AMB-T-1976` verifies against the download.
/// Only sha256 is accepted in v1: one algorithm keeps the digest a fixed, checkable string.
fn check_checksum(problems: &mut Vec<Problem>, location: &str, checksum: &str) {
    let ok = checksum
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    if !ok {
        problems.push(Problem::new(
            location,
            ProblemCode::BadChecksum,
            "checksum must be 'sha256:' followed by 64 lowercase hex digits",
        ));
    }
}

/// Check the amenbo floor a manifest declares: absent is fine (no floor), but a floor that is present
/// must be a version something can be compared against.
///
/// The rule is exactly what the comparison later does — [`crate::store::parse_version`], the one parser
/// — rather than a stricter grammar written out again here, so the door and the compatibility gate can
/// never disagree about what reads as a version. It is loose on purpose: `1`, `1.8`, `1.8.0-rc.1` and
/// `1.8.0+build` all compare fine; `latest` does not, and that is what this refuses.
///
/// [`crate::plugin_compat`] still treats an unreadable floor as "not met" — the door is where a bad
/// manifest is caught, not the only place it is survived, since a manifest can be replaced by an update
/// after it is already installed.
fn check_min_amenbo(problems: &mut Vec<Problem>, min_amenbo: Option<&str>) {
    let Some(min) = min_amenbo else { return };
    if crate::store::parse_version(min).is_none() {
        problems.push(Problem::new(
            "min_amenbo",
            ProblemCode::BadVersion,
            format!("min_amenbo must be a version like '1.8.0', not '{min}'"),
        ));
    }
}

/// Check the OS set: non-empty (a plugin must run somewhere) and free of duplicates. Takes the set
/// rather than the manifest, because a list entry carries the same set and owes the same rule.
fn check_os(problems: &mut Vec<Problem>, os: &[Os]) {
    if os.is_empty() {
        problems.push(Problem::new(
            "os",
            ProblemCode::EmptyOs,
            "os must list at least one operating system",
        ));
        return;
    }
    let mut seen = HashSet::new();
    for os in os {
        if !seen.insert(*os) {
            problems.push(Problem::new(
                "os",
                ProblemCode::Duplicate,
                format!("os lists '{}' more than once", os.as_str()),
            ));
        }
    }
}

/// Check the config schema against the safe floor (`AMB-D-356`, `AMB-D-415`): a field-count cap, a
/// total-size cap, a key grammar, a label floor, unique keys, and — for a field that declares a kind — the
/// shape its kind owes. The per-value byte/control floor is a different boundary — it guards a
/// *user-typed value* at write time ([`crate::plugin_config::check_value`]) — and is not here; this
/// validates the *author-declared schema*.
fn check_config(problems: &mut Vec<Problem>, m: &Manifest) {
    if m.config.len() > MAX_CONFIG_FIELDS {
        problems.push(Problem::new(
            "config",
            ProblemCode::TooManyFields,
            format!("config declares too many fields ({}; max {MAX_CONFIG_FIELDS})", m.config.len()),
        ));
    }
    let total: usize = m.config.iter().map(schema_bytes).sum();
    if total > MAX_CONFIG_SCHEMA_BYTES {
        problems.push(Problem::new(
            "config",
            ProblemCode::SchemaTooLarge,
            format!("config schema is too large ({total} bytes; max {MAX_CONFIG_SCHEMA_BYTES})"),
        ));
    }

    let mut seen = HashSet::new();
    for (i, field) in m.config.iter().enumerate() {
        check_config_key(problems, i, &field.key);
        check_line(problems, &format!("config[{i}].label"), &field.label, MAX_LABEL_LEN);
        if !field.key.is_empty() && !seen.insert(field.key.as_str()) {
            problems.push(Problem::new(
                format!("config[{i}].key"),
                ProblemCode::Duplicate,
                format!("config key '{}' is declared more than once", field.key),
            ));
        }
        check_config_kind(problems, i, field);
    }
}

/// How much of the schema's size budget one field spends: every string its author wrote into it.
fn schema_bytes(f: &ConfigField) -> usize {
    f.key.len()
        + f.label.len()
        + f.options.iter().map(|o| o.value.len() + o.label.len()).sum::<usize>()
        + f.default.as_deref().map_or(0, str::len)
}

/// Check one field's kind against what that kind owes (`AMB-D-415`).
///
/// A `multi` field is a choice, so it must offer something to choose from, each candidate must be one the
/// store can hold and hand back — a line, unique among its siblings, free of the comma the chosen values
/// are joined by, and not the reserved word an empty choice is stored as
/// ([`NONE_SELECTED`]) — and a declared default must name candidates
/// this field actually offers, since a default outside the choice is a value no form could ever produce.
///
/// A text field owes the opposite: **no** candidates. Ignoring them silently would let an author write a
/// choice, see a text box, and have nothing to read; naming it is the only way they learn `type` was the
/// key they missed.
fn check_config_kind(problems: &mut Vec<Problem>, i: usize, field: &ConfigField) {
    if field.field_type != FieldType::Multi {
        if !field.options.is_empty() {
            problems.push(Problem::new(
                format!("config[{i}].options"),
                ProblemCode::OptionsNeedMulti,
                "options may only be declared on a field with type: multi",
            ));
        }
        if let Some(default) = &field.default {
            check_line(problems, &format!("config[{i}].default"), default, MAX_DEFAULT_LEN);
        }
        return;
    }

    if field.options.is_empty() {
        problems.push(Problem::new(
            format!("config[{i}].options"),
            ProblemCode::Empty,
            "a multi field must declare the options it offers",
        ));
    }
    if field.options.len() > MAX_CONFIG_OPTIONS {
        problems.push(Problem::new(
            format!("config[{i}].options"),
            ProblemCode::TooManyFields,
            format!(
                "config field declares too many options ({}; max {MAX_CONFIG_OPTIONS})",
                field.options.len()
            ),
        ));
    }

    let mut seen = HashSet::new();
    for (j, option) in field.options.iter().enumerate() {
        let loc = format!("config[{i}].options[{j}].value");
        check_line(problems, &loc, &option.value, MAX_OPTION_VALUE_LEN);
        check_line(problems, &format!("config[{i}].options[{j}].label"), &option.label, MAX_LABEL_LEN);
        if option.value.contains(',') {
            problems.push(Problem::new(
                loc.clone(),
                ProblemCode::BadChars,
                "an option value must not contain ',' — chosen values are stored joined by one",
            ));
        }
        if option.value == NONE_SELECTED {
            problems.push(Problem::new(
                loc.clone(),
                ProblemCode::Reserved,
                format!("option value '{NONE_SELECTED}' is reserved for choosing nothing"),
            ));
        }
        if !option.value.is_empty() && !seen.insert(option.value.as_str()) {
            problems.push(Problem::new(
                loc,
                ProblemCode::Duplicate,
                format!("option value '{}' is declared more than once", option.value),
            ));
        }
    }

    let Some(default) = &field.default else { return };
    let offered: HashSet<&str> = field.options.iter().map(|o| o.value.as_str()).collect();
    for part in default.split(',') {
        if !offered.contains(part) {
            problems.push(Problem::new(
                format!("config[{i}].default"),
                ProblemCode::NotAnOption,
                format!("default '{part}' is not one of the options this field offers"),
            ));
        }
    }
}

/// Check one config field key: a storage key and (for a secret) an env-var stem, so it must be a plain
/// identifier — `[a-z][a-z0-9_]*` — and within the identifier byte cap the write boundary also enforces
/// ([`MAX_CONFIG_IDENT_BYTES`]).
fn check_config_key(problems: &mut Vec<Problem>, i: usize, key: &str) {
    let loc = format!("config[{i}].key");
    if key.is_empty() {
        problems.push(Problem::new(loc, ProblemCode::Empty, "config key must not be empty"));
        return;
    }
    if key.len() > MAX_CONFIG_IDENT_BYTES {
        problems.push(Problem::new(
            loc.clone(),
            ProblemCode::TooLong,
            format!("config key is too long ({} bytes; max {MAX_CONFIG_IDENT_BYTES})", key.len()),
        ));
    }
    let well_formed = key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !well_formed {
        problems.push(Problem::new(
            loc,
            ProblemCode::BadKey,
            "config key must be a lowercase identifier ([a-z][a-z0-9_]*)",
        ));
    }
}

/// Check each event subscription's `faces` / `reply` shape (`AMB-D-383`). The face *tokens* are already the
/// vocabulary — an unknown one fails to parse (`AMB-D-354`), so a parsed [`Manifest`] holds only `cli`/`gui`
/// and this need not re-check the value domain. Two rules remain, and they are the two the type cannot carry
/// on its own:
///
/// - **`faces` is non-empty** — a hook that fires on no face is a subscription that does nothing, which is
///   a mistake worth naming rather than silently dropping.
/// - **`reply: true` requires `faces: [cli]`** — the reply is relayed to the caller, and only the CLI face
///   has one (the GUI has no caller to hand a reply to). Declaring a reply on any other face set is asking
///   for something amenbo cannot deliver.
///
/// That each event *name* is a real v1 event is a different rule and a different boundary (`AMB-T-1988`);
/// this function judges the fire shape only.
fn check_events(problems: &mut Vec<Problem>, m: &Manifest) {
    for (i, sub) in m.events.iter().enumerate() {
        if sub.faces.is_empty() {
            problems.push(Problem::new(
                format!("events[{i}].faces"),
                ProblemCode::EmptyFaces,
                "a subscription's faces must not be empty",
            ));
        }
        if sub.reply && sub.faces != [Face::Cli] {
            problems.push(Problem::new(
                format!("events[{i}].reply"),
                ProblemCode::ReplyNeedsCli,
                "reply: true is only allowed when faces is exactly [cli]",
            ));
        }
    }
}

/// Check the agent block against the same floor every other author-written text keeps (`AMB-D-437`): the
/// `when` line and each command's two lines are one-line fields — non-empty, control-char-free, capped —
/// and the command list has a ceiling.
///
/// The block is optional, so a manifest without one has nothing here to break. What it says is never
/// judged: amenbo has no vocabulary for what a third party's plugin is for, and a door that ruled on the
/// wording would be the body of knowledge `AMB-D-437` deliberately refuses to hold. It rules on the shape
/// alone — enough that a line cannot break the entry-point document it lands in, and that a block claiming
/// to name an occasion actually names one.
fn check_agent(problems: &mut Vec<Problem>, m: &Manifest) {
    let Some(agent) = &m.agent else { return };

    check_line(problems, "agent.when", &agent.when, MAX_AGENT_WHEN_LEN);
    if agent.commands.len() > MAX_AGENT_COMMANDS {
        problems.push(Problem::new(
            "agent.commands",
            ProblemCode::TooManyFields,
            format!(
                "agent names too many commands ({}; max {MAX_AGENT_COMMANDS})",
                agent.commands.len()
            ),
        ));
    }
    for (i, command) in agent.commands.iter().enumerate() {
        check_line(problems, &format!("agent.commands[{i}].cmd"), &command.cmd, MAX_AGENT_CMD_LEN);
        check_line(problems, &format!("agent.commands[{i}].does"), &command.does, MAX_AGENT_DOES_LEN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{
        AgentCommand, AgentGuide, Arch, Asset, ConfigField, ConfigOption, EventSubscription, Face,
        Manifest, Os, Platform,
    };

    /// An arch-agnostic platform key (`<os>`).
    fn plat(os: Os) -> Platform {
        Platform { os, arch: None }
    }

    /// An arch-specific platform key (`<os>-<arch>`).
    fn plat_arch(os: Os, arch: Arch) -> Platform {
        Platform { os, arch: Some(arch) }
    }

    /// A well-formed distributable — the shape rules are what these tests are about, not the values.
    fn asset() -> Asset {
        Asset {
            url: "https://example.com/worktree-v1.tar.gz".into(),
            checksum: format!("sha256:{}", "a".repeat(64)),
            signature: None,
        }
    }

    fn valid() -> Manifest {
        Manifest {
            name: "worktree".into(),
            desc: "Isolate each task in its own git worktree".into(),
            author: "amenbo".into(),
            repo: "ShiroDoromoto/amenbo-plugin-worktree".into(),
            os: vec![Os::Macos, Os::Linux],
            category: "workflow".into(),
            url: "https://example.com/worktree-v1.tar.gz".into(),
            checksum: format!("sha256:{}", "a".repeat(64)),
            // signature (provenance) and events (subscription) are other boundaries' to validate — the
            // manifest-shape validator here neither reads nor checks them.
            signature: None,
            // The agent block is optional; the tests below are the ones that put one in.
            agent: None,
            // The one-file form: this manifest's single url serves both the OSes it lists.
            assets: Default::default(),
            events: Vec::new(),
            official: true,
            detail_sum: None,
            payload_v: 1,
            min_amenbo: Some("1.8.0".into()),
            config: vec![
                ConfigField { secret: true, required: true, ..ConfigField::new("webhook_url", "Webhook URL") },
                ConfigField::new("events", "Events"),
            ],
        }
    }

    fn codes(problems: &[Problem]) -> Vec<ProblemCode> {
        problems.iter().map(|p| p.code).collect()
    }

    #[test]
    fn a_valid_manifest_has_no_problems() {
        assert!(validate_manifest(&valid()).is_empty(), "a well-formed manifest passes");
    }

    #[test]
    fn a_config_free_manifest_is_valid() {
        let mut m = valid();
        m.config.clear();
        assert!(validate_manifest(&m).is_empty(), "no config schema is a plugin with no settings, not a problem");
    }

    /// The per-OS form (`AMB-D-381`): the map answers for exactly the platforms `os` claims, and each
    /// distributable's url and digest are checked where they are — under `assets.<os>`, so an author is
    /// told which one is wrong.
    #[test]
    fn a_per_os_manifest_is_valid_when_the_map_matches_the_declared_os_set() {
        let mut m = valid();
        m.url = String::new();
        m.checksum = String::new();
        m.assets = [(plat(Os::Macos), asset()), (plat(Os::Linux), asset())].into_iter().collect();
        assert!(validate_manifest(&m).is_empty(), "{:?}", validate_manifest(&m));

        // A platform claimed and not published: the entry offers an install that could never be served.
        let mut short = m.clone();
        short.assets.remove(&plat(Os::Linux));
        assert!(codes(&validate_manifest(&short)).contains(&ProblemCode::AssetMismatch));

        // And the other way — bytes published for a platform the plugin never said it runs on.
        let mut extra = m.clone();
        extra.assets.insert(plat(Os::Windows), asset());
        let problems = validate_manifest(&extra);
        assert!(codes(&problems).contains(&ProblemCode::AssetMismatch));
        assert!(
            problems.iter().any(|p| p.location == "assets.windows"),
            "the problem names which asset: {problems:?}"
        );

        // Each asset carries its own url and digest, so each is checked at its own location.
        let mut bad = m.clone();
        bad.assets.insert(
            plat(Os::Linux),
            Asset { url: "http://example.com/x".into(), checksum: "nope".into(), signature: None },
        );
        let problems = validate_manifest(&bad);
        assert!(problems.iter().any(|p| p.location == "assets.linux.url" && p.code == ProblemCode::BadUrl));
        assert!(problems
            .iter()
            .any(|p| p.location == "assets.linux.checksum" && p.code == ProblemCode::BadChecksum));
    }

    /// The os-arch form (`AMB-D-384`): an OS is answered by its arch-agnostic key or any `<os>-<arch>`, arch
    /// may subdivide beneath a declared OS, and a key naming an OS `os` never listed is still refused.
    #[test]
    fn an_os_may_be_answered_by_arch_specific_keys() {
        // linux split into two arch builds, macOS as one universal key — every declared OS is answered.
        let mut m = valid();
        m.url = String::new();
        m.checksum = String::new();
        m.os = vec![Os::Macos, Os::Linux];
        m.assets = [
            (plat(Os::Macos), asset()),
            (plat_arch(Os::Linux, Arch::X64), asset()),
            (plat_arch(Os::Linux, Arch::Arm64), asset()),
        ]
        .into_iter()
        .collect();
        assert!(validate_manifest(&m).is_empty(), "{:?}", validate_manifest(&m));

        // An OS answered by no key at all — neither <os> nor any <os>-<arch> — is the fail-open the door closes.
        let mut short = m.clone();
        short.assets.remove(&plat_arch(Os::Linux, Arch::X64));
        short.assets.remove(&plat_arch(Os::Linux, Arch::Arm64));
        assert!(codes(&validate_manifest(&short)).contains(&ProblemCode::AssetMismatch));

        // An arch key for an OS the entry never declared is refused, located by its full token.
        let mut extra = m.clone();
        extra.assets.insert(plat_arch(Os::Windows, Arch::X64), asset());
        let problems = validate_manifest(&extra);
        assert!(codes(&problems).contains(&ProblemCode::AssetMismatch));
        assert!(
            problems.iter().any(|p| p.location == "assets.windows-x64"),
            "the problem names the full platform token: {problems:?}"
        );
    }

    /// The one-file form still owes a url and a digest: with no map, they *are* the distributable, so an
    /// entry that publishes neither publishes nothing.
    #[test]
    fn a_manifest_with_neither_form_publishes_nothing() {
        let mut m = valid();
        m.url = String::new();
        m.checksum = String::new();
        let problems = codes(&validate_manifest(&m));
        assert!(problems.contains(&ProblemCode::BadUrl));
        assert!(problems.contains(&ProblemCode::BadChecksum));
    }

    #[test]
    fn the_id_grammar_is_enforced() {
        // Each id breaks exactly one documented rule.
        assert!(validate_plugin_id("worktree").is_empty());
        assert!(codes(&validate_plugin_id("")).contains(&ProblemCode::Empty));
        assert!(codes(&validate_plugin_id("a")).contains(&ProblemCode::TooShort));
        assert!(codes(&validate_plugin_id(&"a".repeat(65))).contains(&ProblemCode::TooLong));
        assert!(codes(&validate_plugin_id("Worktree")).contains(&ProblemCode::BadChars));
        assert!(codes(&validate_plugin_id("has space")).contains(&ProblemCode::BadChars));
        assert!(codes(&validate_plugin_id("1abc")).contains(&ProblemCode::MustStartLetter));
        assert!(codes(&validate_plugin_id("-abc")).contains(&ProblemCode::HyphenEdge));
        assert!(codes(&validate_plugin_id("abc-")).contains(&ProblemCode::HyphenEdge));
        assert!(codes(&validate_plugin_id("a--b")).contains(&ProblemCode::DoubleHyphen));
    }

    #[test]
    fn reserved_ids_are_refused() {
        // The layout name and the extra reserved words (`AMB-D-360`).
        assert!(codes(&validate_plugin_id("registry")).contains(&ProblemCode::Reserved));
        assert!(codes(&validate_plugin_id("official")).contains(&ProblemCode::Reserved));
        assert!(codes(&validate_plugin_id("amenbo")).contains(&ProblemCode::Reserved));
        assert!(codes(&validate_plugin_id("plugin")).contains(&ProblemCode::Reserved));
    }

    #[test]
    fn an_empty_os_set_is_refused() {
        let mut m = valid();
        m.os.clear();
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::EmptyOs));
    }

    #[test]
    fn a_duplicate_os_is_refused() {
        let mut m = valid();
        m.os = vec![Os::Macos, Os::Macos];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::Duplicate));
    }

    #[test]
    fn a_malformed_checksum_is_refused() {
        for bad in ["deadbeef", "sha256:short", "md5:00", &format!("sha256:{}", "A".repeat(64))] {
            let mut m = valid();
            m.checksum = bad.to_string();
            assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::BadChecksum), "'{bad}' is not a valid checksum");
        }
    }

    #[test]
    fn a_non_https_url_is_refused() {
        for bad in ["http://example.com/x", "file:///x", "example.com/x", "https://"] {
            let mut m = valid();
            m.url = bad.to_string();
            assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::BadUrl), "'{bad}' is not an https url");
        }
    }

    #[test]
    fn a_malformed_repo_is_refused() {
        for bad in ["justowner", "owner/", "/name", "owner/name/extra", "owner/na me"] {
            let mut m = valid();
            m.repo = bad.to_string();
            assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::BadRepo), "'{bad}' is not owner/name");
        }
    }

    #[test]
    fn a_control_character_in_a_line_is_refused() {
        let mut m = valid();
        m.desc = "line one\nline two".into();
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::ControlChar));
    }

    #[test]
    fn an_over_long_line_is_refused() {
        let mut m = valid();
        m.desc = "x".repeat(MAX_DESC_LEN + 1);
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::TooLong));
    }

    #[test]
    fn too_many_config_fields_is_refused() {
        let mut m = valid();
        m.config = (0..MAX_CONFIG_FIELDS + 1)
            .map(|i| ConfigField::new(format!("k{i}"), "L"))
            .collect();
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::TooManyFields));
    }

    #[test]
    fn a_bad_config_key_is_refused() {
        for bad in ["Webhook", "1st", "web-hook", "web hook", ""] {
            let mut m = valid();
            m.config = vec![ConfigField::new(bad, "L")];
            let cs = codes(&validate_manifest(&m));
            assert!(
                cs.contains(&ProblemCode::BadKey) || cs.contains(&ProblemCode::Empty),
                "'{bad}' is not a valid config key"
            );
        }
    }

    #[test]
    fn a_duplicate_config_key_is_refused() {
        let mut m = valid();
        m.config = vec![
            ConfigField::new("dup", "A"),
            ConfigField::new("dup", "B"),
        ];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::Duplicate));
    }

    /// A well-formed choice — candidates, and a default among them (`AMB-D-415`).
    fn multi(default: Option<&str>) -> ConfigField {
        ConfigField {
            field_type: FieldType::Multi,
            options: vec![
                ConfigOption { value: "task.done".into(), label: "完了した".into() },
                ConfigOption { value: "task.rejected".into(), label: "見送った".into() },
            ],
            default: default.map(str::to_string),
            ..ConfigField::new("events", "Events")
        }
    }

    #[test]
    fn a_multi_field_with_options_and_a_default_among_them_is_valid() {
        let mut m = valid();
        m.config = vec![multi(Some("task.done,task.rejected"))];
        assert!(validate_manifest(&m).is_empty(), "{:?}", validate_manifest(&m));

        // No default at all is equally fine: the field is simply unanswered until someone answers it.
        m.config = vec![multi(None)];
        assert!(validate_manifest(&m).is_empty(), "{:?}", validate_manifest(&m));
    }

    /// A choice with nothing to choose from is a form field a user cannot answer.
    #[test]
    fn a_multi_field_with_no_options_is_refused() {
        let mut m = valid();
        m.config = vec![ConfigField { options: Vec::new(), ..multi(None) }];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::Empty));
    }

    /// Candidates on a field that is not a choice: the author meant `type: multi` and would otherwise be
    /// shown a text box with their options nowhere.
    #[test]
    fn options_on_a_text_field_are_refused() {
        let mut m = valid();
        m.config = vec![ConfigField { field_type: FieldType::Text, ..multi(None) }];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::OptionsNeedMulti));
    }

    /// The stored value is the chosen values joined by commas, so a candidate carrying one could not be
    /// read back, and the reserved word for choosing nothing cannot also be something to choose.
    #[test]
    fn an_option_value_may_not_carry_a_comma_or_be_the_reserved_word() {
        let mut m = valid();
        m.config = vec![ConfigField {
            options: vec![ConfigOption { value: "a,b".into(), label: "L".into() }],
            ..multi(None)
        }];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::BadChars));

        m.config = vec![ConfigField {
            options: vec![ConfigOption { value: NONE_SELECTED.into(), label: "L".into() }],
            ..multi(None)
        }];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::Reserved));
    }

    #[test]
    fn a_duplicate_option_value_is_refused() {
        let mut m = valid();
        m.config = vec![ConfigField {
            options: vec![
                ConfigOption { value: "task.done".into(), label: "A".into() },
                ConfigOption { value: "task.done".into(), label: "B".into() },
            ],
            ..multi(None)
        }];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::Duplicate));
    }

    #[test]
    fn too_many_options_is_refused() {
        let mut m = valid();
        m.config = vec![ConfigField {
            options: (0..MAX_CONFIG_OPTIONS + 1)
                .map(|i| ConfigOption { value: format!("v{i}"), label: "L".into() })
                .collect(),
            ..multi(None)
        }];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::TooManyFields));
    }

    /// A default outside the candidates is a value no form could ever produce — including the reserved
    /// word, since "choose nothing" is the user's answer to give, not a default to declare.
    #[test]
    fn a_default_that_is_not_offered_is_refused() {
        for bad in ["task.created", "task.done,task.created", NONE_SELECTED, ""] {
            let mut m = valid();
            m.config = vec![multi(Some(bad))];
            assert!(
                codes(&validate_manifest(&m)).contains(&ProblemCode::NotAnOption),
                "'{bad}' is not one of the offered values"
            );
        }
    }

    /// A text field's default is a line like any other: bounded, and not a blank standing in for unset.
    #[test]
    fn a_text_default_is_held_to_the_one_line_floor() {
        let mut m = valid();
        m.config = vec![ConfigField {
            default: Some("x".repeat(MAX_DEFAULT_LEN + 1)),
            ..ConfigField::new("base", "Base branch")
        }];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::TooLong));

        m.config = vec![ConfigField {
            default: Some("main".into()),
            ..ConfigField::new("base", "Base branch")
        }];
        assert!(validate_manifest(&m).is_empty(), "a plain default on a text field is fine");
    }

    /// The size cap bounds the schema an author declares, candidates included — a handful of fields can
    /// carry a great many of them.
    #[test]
    fn options_count_towards_the_schema_size_cap() {
        let full_of_options = |key: &str| ConfigField {
            options: (0..MAX_CONFIG_OPTIONS)
                .map(|i| ConfigOption { value: format!("v{i}"), label: "l".repeat(MAX_LABEL_LEN) })
                .collect(),
            ..ConfigField { field_type: FieldType::Multi, ..ConfigField::new(key, "L") }
        };
        let mut m = valid();
        // Each field is within every per-field cap; together they are more schema than the whole may be.
        m.config = vec![full_of_options("a"), full_of_options("b")];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::SchemaTooLarge));
    }

    #[test]
    fn all_problems_are_collected_not_first_only() {
        // A manifest wrong in several places surfaces several problems in one run — the author-tool contract.
        let mut m = valid();
        m.name = "Bad Name".into();
        m.os.clear();
        m.checksum = "nope".into();
        let cs = codes(&validate_manifest(&m));
        assert!(cs.contains(&ProblemCode::BadChars));
        assert!(cs.contains(&ProblemCode::EmptyOs));
        assert!(cs.contains(&ProblemCode::BadChecksum));
    }

    // ---- events: faces / reply (`AMB-D-383`) ----

    #[test]
    fn a_bare_subscription_is_valid() {
        let mut m = valid();
        m.events = vec![EventSubscription::new("task.created"), EventSubscription::new("task.done")];
        assert!(validate_manifest(&m).is_empty(), "the notification default fires on both faces, no reply");
    }

    #[test]
    fn a_reply_hook_on_cli_alone_is_valid() {
        let mut m = valid();
        m.events =
            vec![EventSubscription { event: "task.status_changed".into(), faces: vec![Face::Cli], reply: true }];
        assert!(validate_manifest(&m).is_empty(), "the worktree advice shape passes");
    }

    #[test]
    fn an_empty_faces_set_is_refused() {
        let mut m = valid();
        m.events = vec![EventSubscription { event: "task.done".into(), faces: vec![], reply: false }];
        let problems = validate_manifest(&m);
        assert!(codes(&problems).contains(&ProblemCode::EmptyFaces));
        assert!(problems.iter().any(|p| p.location == "events[0].faces"), "names which subscription");
    }

    #[test]
    fn a_reply_hook_that_is_not_cli_only_is_refused() {
        // reply relays to the caller, and only the CLI face has one — so both-faces and gui-only are wrong.
        for faces in [vec![Face::Cli, Face::Gui], vec![Face::Gui]] {
            let mut m = valid();
            m.events = vec![EventSubscription { event: "task.done".into(), faces, reply: true }];
            let problems = validate_manifest(&m);
            assert!(codes(&problems).contains(&ProblemCode::ReplyNeedsCli), "{:?}", codes(&problems));
            assert!(problems.iter().any(|p| p.location == "events[0].reply"));
        }
    }

    // ---- agent (`AMB-D-437`) ----

    /// A well-formed block, and the two shapes an author may write: with a command face, and with none.
    #[test]
    fn an_agent_block_is_valid_with_or_without_commands() {
        let mut m = valid();
        m.agent = Some(AgentGuide {
            when: "Starting work on a task that will produce commits".into(),
            commands: vec![AgentCommand {
                cmd: "start <task-id>".into(),
                does: "Cuts a worktree outside the repo and returns the cd line to eval".into(),
            }],
        });
        assert!(validate_manifest(&m).is_empty(), "{:?}", validate_manifest(&m));

        // An observation-only plugin names its occasion and stops there.
        m.agent = Some(AgentGuide { when: "Never — it only watches".into(), commands: vec![] });
        assert!(validate_manifest(&m).is_empty(), "{:?}", validate_manifest(&m));

        // And declaring no block at all is the ordinary case.
        m.agent = None;
        assert!(validate_manifest(&m).is_empty());
    }

    #[test]
    fn an_agent_block_that_names_no_occasion_is_refused() {
        let mut m = valid();
        m.agent = Some(AgentGuide { when: String::new(), commands: vec![] });
        let problems = validate_manifest(&m);
        assert!(codes(&problems).contains(&ProblemCode::Empty));
        assert!(problems.iter().any(|p| p.location == "agent.when"), "{problems:?}");
    }

    /// The lines are one-line fields, and each is located precisely enough for an author to fix it.
    #[test]
    fn an_agent_line_that_breaks_the_one_line_floor_is_refused() {
        let over = |n: usize| "x".repeat(n + 1);
        let cases: [(&str, AgentGuide); 4] = [
            (
                "agent.when",
                AgentGuide { when: over(MAX_AGENT_WHEN_LEN), commands: vec![] },
            ),
            (
                "agent.commands[0].cmd",
                AgentGuide {
                    when: "w".into(),
                    commands: vec![AgentCommand { cmd: over(MAX_AGENT_CMD_LEN), does: "d".into() }],
                },
            ),
            (
                "agent.commands[0].does",
                AgentGuide {
                    when: "w".into(),
                    commands: vec![AgentCommand { cmd: "c".into(), does: over(MAX_AGENT_DOES_LEN) }],
                },
            ),
            (
                "agent.commands[0].does",
                AgentGuide {
                    when: "w".into(),
                    commands: vec![AgentCommand { cmd: "c".into(), does: "one\ntwo".into() }],
                },
            ),
        ];
        for (location, agent) in cases {
            let mut m = valid();
            m.agent = Some(agent);
            let problems = validate_manifest(&m);
            assert!(
                problems.iter().any(|p| p.location == location),
                "{location} must be reported: {problems:?}"
            );
        }
    }

    /// Every command is read into an AI's context on every `agent --json`, so the list has a ceiling.
    #[test]
    fn too_many_agent_commands_is_refused() {
        let mut m = valid();
        m.agent = Some(AgentGuide {
            when: "w".into(),
            commands: (0..MAX_AGENT_COMMANDS + 1)
                .map(|i| AgentCommand { cmd: format!("c{i}"), does: "does".into() })
                .collect(),
        });
        let problems = validate_manifest(&m);
        assert!(codes(&problems).contains(&ProblemCode::TooManyFields));
        assert!(problems.iter().any(|p| p.location == "agent.commands"), "{problems:?}");
    }

    // ---- min_amenbo ----

    #[test]
    fn no_floor_declared_is_fine() {
        let mut m = valid();
        m.min_amenbo = None;
        assert!(validate_manifest(&m).is_empty(), "a manifest may simply not declare a floor");
    }

    #[test]
    fn a_floor_that_is_not_a_version_is_refused_at_the_door() {
        // Without this rule each of these reaches the compatibility gate instead, where the plugin
        // installs fine and then silently never fires.
        for min in ["latest", "", "v1.8.0", "1.x", "one.eight.zero"] {
            let mut m = valid();
            m.min_amenbo = Some(min.into());
            let cs = codes(&validate_manifest(&m));
            assert!(cs.contains(&ProblemCode::BadVersion), "'{min}' must be refused, got {cs:?}");
        }
    }

    #[test]
    fn a_floor_reads_exactly_as_loosely_as_the_comparison_does() {
        // The door must not be stricter than the parser that later compares against it, or a manifest
        // amenbo could honour would be rejected for a shape the comparison accepts.
        for min in ["1", "1.8", "1.8.0", "1.8.0-rc.1", "1.8.0+build.5"] {
            let mut m = valid();
            m.min_amenbo = Some(min.into());
            assert!(validate_manifest(&m).is_empty(), "'{min}' compares fine, so it passes");
        }
    }

    #[test]
    fn every_code_has_a_distinct_string() {
        let mut seen = HashSet::new();
        for c in ProblemCode::ALL {
            assert!(seen.insert(c.as_str()), "duplicate code string {}", c.as_str());
        }
    }
}
