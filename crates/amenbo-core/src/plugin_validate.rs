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
use crate::plugin_manifest::{Face, Manifest, Os};
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
/// The largest a config schema may be in total, summed over every field's key and label bytes
/// (`AMB-D-356`, the safe floor). Bounds the schema as a whole, complementing the per-field caps.
pub const MAX_CONFIG_SCHEMA_BYTES: usize = 8 * 1024;

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
    /// The id is reserved and cannot name a plugin.
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
    /// A value appears more than once where it must be unique (an OS, a config key).
    Duplicate,
    /// The config schema declares more fields than the cap.
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
        }
    }
}

impl Serialize for ProblemCode {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// One thing wrong with a manifest. It names *where* (a field path like `name` or `config[2].key`), *what*
/// rule broke ([`ProblemCode`], the machine token), and carries a bilingual sentence for a person
/// ([`Msg`]). A validator run returns a `Vec` of these; empty means the manifest passed.
#[derive(Clone, Debug)]
pub struct Problem {
    /// The field path the problem is at — `name`, `os`, `checksum`, `config[2].key`.
    pub location: String,
    /// The rule that broke — the stable token a machine reads.
    pub code: ProblemCode,
    /// The sentence a person reads, in both languages.
    pub message: Msg,
}

impl Problem {
    fn new(location: impl Into<String>, code: ProblemCode, en: impl Into<String>, ja: impl Into<String>) -> Self {
        Problem { location: location.into(), code, message: Msg::new(en, ja) }
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
        problems.push(Problem::new(loc, ProblemCode::Empty, "plugin name must not be empty", "プラグイン名は空にできません"));
        return problems; // nothing else is meaningful on an empty id
    }
    if name.len() < NAME_MIN_LEN {
        problems.push(Problem::new(
            loc,
            ProblemCode::TooShort,
            format!("plugin name is too short ({} chars; min {NAME_MIN_LEN})", name.chars().count()),
            format!("プラグイン名が短すぎます（{} 文字・下限 {NAME_MIN_LEN}）", name.chars().count()),
        ));
    }
    if name.len() > NAME_MAX_LEN {
        problems.push(Problem::new(
            loc,
            ProblemCode::TooLong,
            format!("plugin name is too long ({} chars; max {NAME_MAX_LEN})", name.chars().count()),
            format!("プラグイン名が長すぎます（{} 文字・上限 {NAME_MAX_LEN}）", name.chars().count()),
        ));
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        problems.push(Problem::new(
            loc,
            ProblemCode::BadChars,
            "plugin name may use only lowercase ASCII letters, digits and '-'",
            "プラグイン名に使えるのは小文字 ASCII 英数字と '-' だけです",
        ));
    }
    if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        problems.push(Problem::new(
            loc,
            ProblemCode::MustStartLetter,
            "plugin name must start with a lowercase letter",
            "プラグイン名は小文字英字で始める必要があります",
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        problems.push(Problem::new(
            loc,
            ProblemCode::HyphenEdge,
            "plugin name must not start or end with '-'",
            "プラグイン名の先頭・末尾を '-' にはできません",
        ));
    }
    if name.contains("--") {
        problems.push(Problem::new(
            loc,
            ProblemCode::DoubleHyphen,
            "plugin name must not contain '--'",
            "プラグイン名に '--' を含めることはできません",
        ));
    }
    if is_reserved_plugin_name(name) || RESERVED_NAMES.contains(&name) {
        problems.push(Problem::new(
            loc,
            ProblemCode::Reserved,
            format!("plugin name '{name}' is reserved"),
            format!("プラグイン名 '{name}' は予約されています"),
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
            format!("{field} は空にできません"),
        ));
        return;
    }
    if value.chars().any(|c| c.is_control()) {
        problems.push(Problem::new(
            field,
            ProblemCode::ControlChar,
            format!("{field} must not contain control characters"),
            format!("{field} に制御文字を含めることはできません"),
        ));
    }
    let len = value.chars().count();
    if len > max {
        problems.push(Problem::new(
            field,
            ProblemCode::TooLong,
            format!("{field} is too long ({len} chars; max {max})"),
            format!("{field} が長すぎます（{len} 文字・上限 {max}）"),
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
            "repo は 'owner/name'（GitHub の座標）である必要があります",
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
                format!("os が {} を挙げていますが、assets にその配布物がありません", os.as_str()),
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
                format!("assets が {} 向けに配布していますが、os がそれを挙げていません", platform.token()),
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
            "url は https:// の URL である必要があります",
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
            "checksum は 'sha256:' に続けて小文字16進64桁である必要があります",
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
            format!("min_amenbo は '1.8.0' のようなバージョンである必要があります（'{min}' は不可）"),
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
            "os には対応OSを1つ以上挙げる必要があります",
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
                format!("os に '{}' が重複しています", os.as_str()),
            ));
        }
    }
}

/// Check the config schema against the safe floor (`AMB-D-356`): a field-count cap, a total-size cap, a
/// key grammar, a label floor, and unique keys. The per-value byte/control floor is a different boundary —
/// it guards a *user-typed value* at write time ([`crate::plugin_config::check_value`]) — and is not here;
/// this validates the *author-declared schema*.
fn check_config(problems: &mut Vec<Problem>, m: &Manifest) {
    if m.config.len() > MAX_CONFIG_FIELDS {
        problems.push(Problem::new(
            "config",
            ProblemCode::TooManyFields,
            format!("config declares too many fields ({}; max {MAX_CONFIG_FIELDS})", m.config.len()),
            format!("config のフィールド数が多すぎます（{}・上限 {MAX_CONFIG_FIELDS}）", m.config.len()),
        ));
    }
    let total: usize = m.config.iter().map(|f| f.key.len() + f.label.len()).sum();
    if total > MAX_CONFIG_SCHEMA_BYTES {
        problems.push(Problem::new(
            "config",
            ProblemCode::SchemaTooLarge,
            format!("config schema is too large ({total} bytes; max {MAX_CONFIG_SCHEMA_BYTES})"),
            format!("config スキーマが大きすぎます（{total} バイト・上限 {MAX_CONFIG_SCHEMA_BYTES}）"),
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
                format!("config キー '{}' が重複して宣言されています", field.key),
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
        problems.push(Problem::new(loc, ProblemCode::Empty, "config key must not be empty", "config キーは空にできません"));
        return;
    }
    if key.len() > MAX_CONFIG_IDENT_BYTES {
        problems.push(Problem::new(
            loc.clone(),
            ProblemCode::TooLong,
            format!("config key is too long ({} bytes; max {MAX_CONFIG_IDENT_BYTES})", key.len()),
            format!("config キーが長すぎます（{} バイト・上限 {MAX_CONFIG_IDENT_BYTES}）", key.len()),
        ));
    }
    let well_formed = key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !well_formed {
        problems.push(Problem::new(
            loc,
            ProblemCode::BadKey,
            "config key must be a lowercase identifier ([a-z][a-z0-9_]*)",
            "config キーは小文字の識別子（[a-z][a-z0-9_]*）である必要があります",
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
                "購読の faces は空にできません",
            ));
        }
        if sub.reply && sub.faces != [Face::Cli] {
            problems.push(Problem::new(
                format!("events[{i}].reply"),
                ProblemCode::ReplyNeedsCli,
                "reply: true is only allowed when faces is exactly [cli]",
                "reply: true は faces がちょうど [cli] のときだけ許されます",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{Arch, Asset, ConfigField, EventSubscription, Face, Manifest, Os, Platform};

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
            // The one-file form: this manifest's single url serves both the OSes it lists.
            assets: Default::default(),
            events: Vec::new(),
            official: true,
            detail_sum: None,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: Some("1.8.0".into()),
            config: vec![
                ConfigField { key: "webhook_url".into(), label: "Webhook URL".into(), secret: true, required: true },
                ConfigField { key: "events".into(), label: "Events".into(), secret: false, required: false },
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
            .map(|i| ConfigField { key: format!("k{i}"), label: "L".into(), secret: false, required: false })
            .collect();
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::TooManyFields));
    }

    #[test]
    fn a_bad_config_key_is_refused() {
        for bad in ["Webhook", "1st", "web-hook", "web hook", ""] {
            let mut m = valid();
            m.config = vec![ConfigField { key: bad.into(), label: "L".into(), secret: false, required: false }];
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
            ConfigField { key: "dup".into(), label: "A".into(), secret: false, required: false },
            ConfigField { key: "dup".into(), label: "B".into(), secret: false, required: false },
        ];
        assert!(codes(&validate_manifest(&m)).contains(&ProblemCode::Duplicate));
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
