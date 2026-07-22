//! The plugin manifest validator — the **one place** the manifest rules live (`AMB-D-354`).
//!
//! A manifest is untrusted third-party input ([`crate::plugin_manifest`] is the *shape*; serde rejects one
//! missing a required field). The *rules* on top of that shape — a name that fits the id grammar, a
//! well-formed checksum, a non-empty OS set, a config schema within the safe floor — are here, so the one
//! truth about them lives in one function. Two callers share it (`AMB-D-354`):
//!
//! - **the door** (install / catalog intake, `AMB-T-1979`, future): fail-closed — a manifest with any
//!   problem is refused, never listed or installed.
//! - **`plugin validate`** (`AMB-T-1988`): an author self-checks a manifest before opening a catalog PR,
//!   seeing *every* problem at once rather than one per run.
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
use crate::plugin_manifest::Manifest;

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
    check_url(&mut problems, &m.url);
    check_checksum(&mut problems, &m.checksum);
    check_os(&mut problems, m);
    check_config(&mut problems, m);

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

/// Check `url` is an `https://` URL. Shape only (fail-closed on scheme): `http`/`file`/anything else is
/// refused so an install never fetches over a plaintext or local scheme — whether it *resolves* is the
/// download's concern (`AMB-T-1976`), not the manifest's.
fn check_url(problems: &mut Vec<Problem>, url: &str) {
    if !url.starts_with("https://") || url.len() <= "https://".len() {
        problems.push(Problem::new(
            "url",
            ProblemCode::BadUrl,
            "url must be an https:// URL",
            "url は https:// の URL である必要があります",
        ));
    }
}

/// Check `checksum` is `sha256:<64 lowercase hex>` — the shape `AMB-T-1976` verifies against the download.
/// Only sha256 is accepted in v1: one algorithm keeps the digest a fixed, checkable string.
fn check_checksum(problems: &mut Vec<Problem>, checksum: &str) {
    let ok = checksum
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    if !ok {
        problems.push(Problem::new(
            "checksum",
            ProblemCode::BadChecksum,
            "checksum must be 'sha256:' followed by 64 lowercase hex digits",
            "checksum は 'sha256:' に続けて小文字16進64桁である必要があります",
        ));
    }
}

/// Check the OS set: non-empty (a plugin must run somewhere) and free of duplicates.
fn check_os(problems: &mut Vec<Problem>, m: &Manifest) {
    if m.os.is_empty() {
        problems.push(Problem::new(
            "os",
            ProblemCode::EmptyOs,
            "os must list at least one operating system",
            "os には対応OSを1つ以上挙げる必要があります",
        ));
        return;
    }
    let mut seen = HashSet::new();
    for os in &m.os {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{ConfigField, Manifest, Os};

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
            official: true,
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

    #[test]
    fn every_code_has_a_distinct_string() {
        let mut seen = HashSet::new();
        for c in ProblemCode::ALL {
            assert!(seen.insert(c.as_str()), "duplicate code string {}", c.as_str());
        }
    }
}
