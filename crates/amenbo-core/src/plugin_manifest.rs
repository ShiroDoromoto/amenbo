//! The plugin manifest: one entry in the distribution catalog, describing a plugin well enough to
//! list it, judge it, and fetch it — without hitting a central server (`AMB-D-347`).
//!
//! A plugin is distributed as a manifest in a public **catalog repository**: a third party opens a PR
//! adding `plugins/<name>.yaml`, and CI aggregates the reviewed manifests into one `catalog.json` that
//! the GUI fetches once (`AMB-D-347`). This module defines the *shape* of one such entry — the type both
//! sides share. It does not read the catalog, fetch anything, or install: it is the schema the fetch
//! (`AMB-T-1979`), the aggregation CI (`AMB-T-1978`), and the provenance check (`AMB-T-1976`) all speak.
//!
//! ```json
//! {
//!   "name": "worktree", "desc": "Isolate each task in its own git worktree",
//!   "author": "amenbo", "repo": "ShiroDoromoto/amenbo-plugin-worktree",
//!   "os": ["macos", "linux"], "category": "workflow",
//!   "url": "https://github.com/.../worktree-v1.tar.gz", "checksum": "sha256:…",
//!   "official": true
//! }
//! ```
//!
//! A plugin built per platform declares one distributable per OS instead of the single `url`/`checksum`
//! (`AMB-D-381`), which is the same entry with an `assets` map in their place:
//!
//! ```json
//! {
//!   "os": ["macos", "linux"],
//!   "assets": {
//!     "macos": { "url": "https://github.com/.../worktree-v1-macos.tar.gz", "checksum": "sha256:…" },
//!     "linux": { "url": "https://github.com/.../worktree-v1-linux.tar.gz", "checksum": "sha256:…" }
//!   }
//! }
//! ```
//!
//! **Lightweight by design** (`AMB-D-347`): an entry carries only what a browse view needs to list and
//! filter — name, description, author, source repo, supported OSes, category, and the official badge —
//! plus the distributable an install needs. Heavy numbers (stars, download counts) are deliberately
//! *not* here: they are fetched lazily for the one plugin a user opens, never for the whole catalog.
//!
//! **`official` is catalog-authoritative, not self-declared** (`AMB-D-347`): the badge means the author is
//! the Amenbo team, decided by catalog curation (the PR review / the manifest's directory), never by a
//! third party ticking a box. The field lives here because the catalog is the shape, but *who may set it
//! true* is enforced upstream by the catalog CI and the validator — the type only supplies the safe
//! default (absent ⇒ `false`).
//!
//! **Validation lives elsewhere** (`AMB-D-354`). The manifest is untrusted third-party input, checked
//! fail-closed at the door (install / catalog intake) by a single validator — `AMB-T-1988`, which also
//! backs `plugin validate` for authors. This module is the type only: it enforces the *shape* (serde
//! rejects a manifest missing a required field), while the *rules* — a well-formed checksum, a name that
//! is not the reserved `registry` ([`config::is_reserved_plugin_name`](crate::config::is_reserved_plugin_name)),
//! a non-empty OS set, and which of the two distributable forms an entry owes — are the validator's, so the
//! one truth about them lives in one place.
//!
//! Unknown keys are ignored rather than rejected: forward compatibility is handled by the version-compat
//! declaration a manifest will carry (`AMB-D-359` — target payload `v` and min Amenbo version), which
//! gates a plugin gracefully instead of failing to parse a manifest a newer Amenbo wrote. Denying unknown
//! fields would preempt that path (the same reasoning as the stored-blob schema in [`blob`](crate::blob)).

use std::collections::BTreeMap;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An operating system a plugin runs on, in wharfy's vocabulary — the same tokens
/// [`update_check`](crate::update_check) uses (`std::env::consts::OS`), so a plugin's OS set and the
/// running platform compare directly.
///
/// Ordered so it can key the [`Manifest::assets`] map: the order is the declaration order above and
/// carries no meaning beyond giving a re-emitted manifest a stable one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

impl Os {
    /// The OS this build is running on, as a manifest spells it. `None` on a platform Amenbo's
    /// vocabulary cannot name — nothing Amenbo ships runs there, so nothing declares an asset for it
    /// either, and every caller's answer is the same: this platform has no distributable.
    pub fn here() -> Option<Os> {
        Os::parse(std::env::consts::OS)
    }

    /// The wire token, matching `std::env::consts::OS`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Os::Macos => "macos",
            Os::Windows => "windows",
            Os::Linux => "linux",
        }
    }

    /// Parse a wire token back to an [`Os`]; `None` for anything else.
    pub fn parse(s: &str) -> Option<Os> {
        match s {
            "macos" => Some(Os::Macos),
            "windows" => Some(Os::Windows),
            "linux" => Some(Os::Linux),
            _ => None,
        }
    }
}

/// A CPU architecture a distributable is built for, in the **same vocabulary
/// [`update_check`](crate::update_check) uses** (`AMB-D-384`): `std::env::consts::ARCH` normalized to
/// wharfy's tokens (`aarch64` → `arm64`, `x86_64` → `x64`), so a plugin's asset keys and the self-updater's
/// keys never spell the same machine two ways.
///
/// What the two ask about is not the same, and deliberately so. A plugin's asset is run **by this
/// build**, so the arch that picks it is this build's own — an arm64 asset is of no use to an x86_64
/// Amenbo, whatever the machine underneath could run. An update replaces the build itself, so it is
/// aimed at the machine ([`native_arch`](crate::update_check::native_arch), `AMB-D-551`). One
/// vocabulary, two questions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    Arm64,
    X64,
}

impl Arch {
    /// The arch this build runs on, or `None` for one Amenbo's vocabulary cannot name — the same honest
    /// gap as [`Os::here`]: an unnameable arch matches only an arch-agnostic asset (the `<os>` key), never
    /// an `<os>-<arch>` one.
    pub fn here() -> Option<Arch> {
        Arch::parse_consts(std::env::consts::ARCH)
    }

    /// The wire token (`arm64` / `x64`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Arch::Arm64 => "arm64",
            Arch::X64 => "x64",
        }
    }

    /// Parse a wire token (`arm64` / `x64`) back to an [`Arch`]; `None` for anything else.
    pub fn parse(s: &str) -> Option<Arch> {
        match s {
            "arm64" => Some(Arch::Arm64),
            "x64" => Some(Arch::X64),
            _ => None,
        }
    }

    /// Normalize a `std::env::consts::ARCH` value onto the wire token — the same mapping the
    /// self-updater's fallback applies, kept here so the two never spell an arch differently.
    fn parse_consts(s: &str) -> Option<Arch> {
        match s {
            "aarch64" => Some(Arch::Arm64),
            "x86_64" => Some(Arch::X64),
            _ => None,
        }
    }
}

/// **What keys an [`assets`](Manifest::assets) map** (`AMB-D-384`): an OS alone — one distributable for
/// **all** of that OS's arches (a universal binary, a script) — or an OS with a specific [`Arch`], the
/// build for exactly that pair.
///
/// The two forms let a manifest be as coarse or as fine as its bytes are: `macos` is one entry for every
/// Mac, while `linux-x64` and `linux-arm64` are two different binaries. Resolution is **exact then
/// OS-wide** ([`Manifest::asset_for`]): the running platform's `<os>-<arch>` is tried first, then its
/// `<os>`, and neither present is a refusal at the door rather than another platform's bytes at run time —
/// the fail-open `AMB-D-384` closes.
///
/// Spelled `<os>` or `<os>-<arch>` on the wire (so it can be a JSON/YAML map key), and ordered by OS then
/// arch — `None` before `Some`, so `macos` sorts ahead of `macos-arm64` — giving a re-emitted manifest a
/// stable key order with no meaning of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Platform {
    pub os: Os,
    /// `None` is the arch-agnostic form (all of `os`'s arches); `Some` is that one arch only.
    pub arch: Option<Arch>,
}

impl Platform {
    /// The platform this build runs on, or `None` when Amenbo's vocabulary cannot name its OS (an arch it
    /// cannot name is still a platform — the arch is simply `None`, matching only an arch-agnostic asset).
    pub fn here() -> Option<Platform> {
        Some(Platform { os: Os::here()?, arch: Arch::here() })
    }

    /// The wire token: `<os>` for the arch-agnostic form, `<os>-<arch>` for a specific pair.
    pub fn token(&self) -> String {
        match self.arch {
            Some(arch) => format!("{}-{}", self.os.as_str(), arch.as_str()),
            None => self.os.as_str().to_string(),
        }
    }

    /// Parse a wire token back to a [`Platform`]: an OS alone, or `<os>-<arch>`. `None` for any token
    /// whose OS or arch is outside the vocabulary — the same fail-to-parse an unknown `os` gets.
    pub fn parse(s: &str) -> Option<Platform> {
        if let Some(os) = Os::parse(s) {
            return Some(Platform { os, arch: None });
        }
        let (os, arch) = s.rsplit_once('-')?;
        Some(Platform { os: Os::parse(os)?, arch: Some(Arch::parse(arch)?) })
    }
}

impl Serialize for Platform {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.token())
    }
}

impl<'de> Deserialize<'de> for Platform {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        Platform::parse(&token)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown platform token '{token}'")))
    }
}

/// **What layer this plugin lives at** (`AMB-D-601`) — declared by the author, because only the author
/// knows which one is meaningful for their plugin.
///
/// A user is never shown two switches for the same plugin (`AMB-D-379`): the layer is settled by this
/// declaration, so `plugin enable <name>` always means exactly one thing. A notifier is answered per
/// project ("do I want this here"), while a plugin that carries the whole device's backlog out has nothing
/// a project could usefully say about it — one server, one pairing, one answer, not one per project.
///
/// The layer decides two things, each its own work: **where the enable, the settings and the secrets are
/// written** (a project's rows, or the device's — `AMB-T-2868`), and **what reach the runner hands the
/// plugin** (that project, or the whole device — `AMB-T-2869`). Both stay in the store either way: a
/// device-wide plugin is not a return to `config.json`, so it rides backups and comes back on a restore
/// (`AMB-D-434`'s three surviving grounds).
///
/// **There is no second scope beside it.** The tier a config *value* was written at is gone (`AMB-D-434`),
/// and it is not coming back: settings do not gain a machine default a project overrides. This declaration
/// picks the one layer the whole plugin — gate, settings, secrets, reach — lives at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// A project's plugin — the default, and what most plugins want. A project that has not enabled it
    /// does not run it, and it sees that project and nothing else.
    #[default]
    Project,
    /// The device's plugin. Enabled once for the machine and reading every project on it, because its work
    /// is the machine's rather than any one project's — a per-project answer would be a switch that looks
    /// like it does something and does not. Enabling one *is* the consent to let it read the whole device
    /// (`AMB-D-601`), so nothing asks a second time.
    Machine,
}

impl Scope {
    /// The wire token.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Machine => "machine",
        }
    }
}

/// **Which face fires a hook** (`AMB-D-383`) — the short-lived CLI a person or their AI drives, or the
/// long-lived GUI.
///
/// A subscription declares the faces it fires on ([`EventSubscription::faces`]); the dispatcher also stamps
/// the face that drove it beside the shared cursor ([`plugin_drive`](crate::plugin_drive), `AMB-D-380`).
/// The two are one vocabulary, so the type lives here with the rest of the manifest shape and
/// `plugin_drive` re-exports it rather than declaring a second enum of the same values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    /// The command face — the CLI a person or their AI runs, and the one face a `reply` hook may fire on,
    /// since it is the only one with a caller waiting to read the reply (`AMB-D-383`).
    Cli,
    /// The long-lived GUI.
    Gui,
}

impl Face {
    /// The wire token, matching how the face is spelled in a manifest and beside the dispatch cursor.
    pub fn as_str(self) -> &'static str {
        match self {
            Face::Cli => "cli",
            Face::Gui => "gui",
        }
    }

    /// The inverse of [`as_str`](Face::as_str) — for reading a face back out of a plain string that never
    /// went through serde, which is how it is stored beside the dispatch cursor (`AMB-D-380`). A token
    /// outside the vocabulary is `None`: a stamp this build cannot read is no answer, not a wrong one.
    pub fn parse(s: &str) -> Option<Face> {
        match s {
            "cli" => Some(Face::Cli),
            "gui" => Some(Face::Gui),
            _ => None,
        }
    }
}

/// The faces a subscription fires on when it declares none: **both** (`AMB-D-383`). A bare event-name
/// string and an object that omits `faces` mean the same thing — a notification-style hook that fires
/// wherever the event happens.
fn default_faces() -> Vec<Face> {
    vec![Face::Cli, Face::Gui]
}

/// **One event a plugin subscribes to, and how its hook is fired** (`AMB-D-383`).
///
/// A subscription carries three things: the `event` name, the [`faces`](EventSubscription::faces) it fires
/// on, and whether its output is a [`reply`](EventSubscription::reply) the caller consumes. The last two
/// have defaults that make the common case a bare string — `"task.done"` is a notification that fires on
/// both faces and replies to no one, equal to `{ event: "task.done", faces: [cli, gui], reply: false }`.
/// An author writes the object form only to narrow the faces or ask for a reply; the worktree advice hook
/// is `{ event: "task.status_changed", faces: [cli], reply: true }`.
///
/// **Shape only, like the rest of this module.** That `reply: true` is allowed only with `faces: [cli]`,
/// and that `faces` is non-empty, are rules the validator enforces (`AMB-D-354`/`AMB-D-383`); serde already
/// refuses a face token outside the vocabulary, the same way an unknown `os` fails to parse. The
/// dispatcher reads `faces` to decide whether the driving face fires this hook and `reply` to decide
/// whether to relay its output — neither is this type's to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventSubscription {
    /// The event name this fires for — one of
    /// [`plugin_payload::V1_EVENTS`](crate::plugin_payload::V1_EVENTS). That it names a real event is the
    /// validator's to enforce; an unrecognised name is inert here, since only catalog events are ever fired.
    pub event: String,
    /// The faces this hook fires on. Default — an omitted key, or the bare-string form — is both.
    pub faces: Vec<Face>,
    /// Whether the hook's output is relayed to the caller (`AMB-D-383`). `true` is only meaningful — and
    /// only valid — with `faces: [cli]`, the one face with a caller to relay to. Default is `false`: a
    /// notification no one waits on.
    pub reply: bool,
}

impl EventSubscription {
    /// A subscription to `event` with the defaults — both faces, no reply. This is the meaning of the
    /// bare-string form, and the shape a re-emitted string subscription round-trips through.
    pub fn new(event: impl Into<String>) -> Self {
        Self { event: event.into(), faces: default_faces(), reply: false }
    }

    /// Whether this is the plain notification default — both faces, no reply — which is exactly what the
    /// bare-string form means. The serializer emits a bare string for it, so an author's `"task.done"`
    /// round-trips verbatim (the same absent-equals-empty spirit the rest of the manifest keeps).
    fn is_bare(&self) -> bool {
        !self.reply && self.faces == default_faces()
    }
}

impl Serialize for EventSubscription {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.is_bare() {
            return s.serialize_str(&self.event);
        }
        let mut st = s.serialize_struct("EventSubscription", 3)?;
        st.serialize_field("event", &self.event)?;
        st.serialize_field("faces", &self.faces)?;
        st.serialize_field("reply", &self.reply)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for EventSubscription {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Two written forms (`AMB-D-383`): a bare event name, or an object that may narrow `faces` and ask
        // for `reply`. `untagged` tries the string first, then the object; an object missing `event`, or
        // carrying a face token outside the vocabulary, matches neither and is refused — the shape door the
        // rest of the manifest passes through.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Name(String),
            Full {
                event: String,
                #[serde(default = "default_faces")]
                faces: Vec<Face>,
                #[serde(default)]
                reply: bool,
            },
        }
        Ok(match Repr::deserialize(d)? {
            Repr::Name(event) => EventSubscription::new(event),
            Repr::Full { event, faces, reply } => EventSubscription { event, faces, reply },
        })
    }
}

/// The payload contract version a manifest targets when it declares none: the v1 baseline (`AMB-D-349`).
/// A fixed literal, deliberately *not* [`crate::plugin_payload::VERSION`] — an omitted `payload_v` means
/// the plugin was written against the original contract, so it must not drift upward as Amenbo bumps its
/// own `v`.
fn default_payload_v() -> u32 {
    1
}

/// One catalog entry: everything a browse view needs to list a plugin, plus what an install needs to
/// fetch it. See the module docs for the design (`AMB-D-347`) and the validation boundary (`AMB-D-354`).
///
/// The core descriptive fields are required — a manifest omitting one does not parse, which is the
/// shape half of the fail-closed door. The rest carry safe defaults for a manifest that omits them:
/// `official` ⇒ `false` (a badge no third party may self-grant), `payload_v` ⇒ the v1 baseline,
/// `min_amenbo` ⇒ no floor, and `config` ⇒ no settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The plugin's name — its identity in the catalog and its directory under `plugins/`. Must not be
    /// the reserved `registry` (the validator enforces this; see the module docs).
    pub name: String,
    /// **What to call the plugin on screen** (`AMB-D-739`) — the product name its author writes it under,
    /// where [`name`](Manifest::name) is the identity everything else is keyed by: the directory under
    /// `plugins/`, the executable, the config keys, the word typed at `plugin run`, and the catalog's
    /// file names. Those cannot read as "Amenbo Viewer", and renaming is not a road that exists, so the
    /// display name is a field of its own rather than a loosening of the id's grammar.
    ///
    /// Absent is the ordinary case, and means a reader sees `name` exactly as they always did. It is not
    /// translated (`AMB-D-739`): a product name is not the kind of text a language has another word for,
    /// so no overlay carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A one-line description, for the list view.
    pub desc: String,
    /// **What the plugin is, in the author's own words** (`AMB-D-638`) — the Markdown a detail view
    /// draws, where `desc`'s one line only says which plugin this is. Absent means the author wrote
    /// none, and the detail view falls back to the repository's README as it always did.
    ///
    /// It is written beside the manifest in the catalog rather than fetched from the plugin's own
    /// repository (`AMB-D-639`): the language-by-language files, the PR check and the delivery are all
    /// already on the catalog road, and GitHub has no language negotiation to fetch a `README.<lang>.md`
    /// over. It rides in the detail document with every language at once (`AMB-D-640`), never in the
    /// list, so a browse view pays for none of it.
    ///
    /// Its length bound and the rule that its links resolve without a base — 2KB per language, absolute
    /// URLs only (`AMB-D-640`) — are the validator's, like every other rule (`AMB-T-2984`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// Who wrote the plugin. For an official plugin this is the Amenbo team; it is display text, not the
    /// authority on the official badge (that is `official`, set by the catalog).
    pub author: String,
    /// The plugin's source repository, `owner/name` — the GitHub coordinates a detail view reads stars
    /// and README from, lazily.
    pub repo: String,
    /// The operating systems the plugin supports. The validator requires this to be non-empty.
    pub os: Vec<Os>,
    /// The plugin's category, for filtering the catalog (e.g. `workflow`). A free label, not a closed
    /// set — the catalog curates the vocabulary.
    pub category: String,
    /// Where the plugin asset is fetched from on install — the **one distributable that serves every OS
    /// the entry lists**, for a plugin that is one file everywhere (a script). A plugin built per platform
    /// declares [`assets`](Manifest::assets) instead and leaves this empty (`AMB-D-381`).
    ///
    /// Empty is therefore a legitimate document, not a parse error: which of the two forms a manifest owes
    /// is a rule, and rules live at the door ([`crate::plugin_validate`]) where every problem is collected
    /// at once. An empty one does not serialize, so a per-OS manifest re-emits without a hollow field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// The single asset's integrity digest (`sha256:<hex>`), verified on download against what `url`
    /// served and re-checked cheaply on every use of the on-disk asset (`AMB-D-351`). See
    /// [`crate::plugin_provenance`]. Empty alongside an empty `url`, for the same reason.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checksum: String,
    /// The asset's minisign signature (the full `.minisig` text), produced by the catalog CI with the
    /// Amenbo **catalog key** when the manifest is aggregated (`AMB-D-371`). Verified once on download
    /// against Amenbo's embedded catalog public key ([`crate::plugin_provenance`]) — the origin half of
    /// provenance, next to `checksum`'s integrity half. Absent means unsigned: a third-party asset with no
    /// signature cannot be installed or enabled (`AMB-D-351`). An official plugin is signed too; its extra
    /// GitHub build-provenance attestation is a separate check, not this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// **One distributable per platform** (`AMB-D-381`, `AMB-D-384`), keyed by a [`Platform`] — an OS
    /// alone, or an OS-arch pair.
    ///
    /// `os` can name three platforms while a single `url` can point at only one set of bytes, which is
    /// fine for a plugin that is one file everywhere and impossible for one built per platform: a native
    /// plugin is different binaries per OS *and* per arch, and the name is the identity, so it cannot be
    /// split into separate entries either (`AMB-D-360`). This map is the join — the platform an entry
    /// claims and the bytes actually served there, one to one. Signature and checksum sit inside each
    /// [`Asset`] because both are claims about *the bytes that will run*, so their grain is the bytes', not
    /// the entry's.
    ///
    /// A key may be arch-agnostic (`macos`, one build for every Mac) or arch-specific (`linux-arm64`); the
    /// door keeps `os` answered by at least one key per OS ([`crate::plugin_validate`]) and
    /// [`asset_for`](Manifest::asset_for) resolves the running platform exact-then-OS-wide.
    ///
    /// **Absent means the single-`url` form**, which stays valid: the two are alternatives, and where both
    /// are written this one answers. That the keys match the declared `os` set is the door's to enforce —
    /// this type only carries the map, and a platform token it does not know fails to parse the same way
    /// every other value outside its vocabulary does.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<Platform, Asset>,
    /// The official badge: the author is the Amenbo team. Catalog-authoritative (`AMB-D-347`), never
    /// self-declared — absent means `false`.
    #[serde(default)]
    pub official: bool,
    /// **Which detail document this entry was published as** (`AMB-D-386`) — the digest of
    /// `plugins/<name>.json`, and the catalog's value rather than the author's.
    ///
    /// A catalog entry travels as two documents ([`crate::plugin_wire`]): the list half everyone fetches,
    /// and the detail half an install reads for the one plugin it is installing. A manifest Amenbo holds
    /// is the two joined, and this is the field that says *which* detail it was joined with. It is the
    /// comparison material update detection is left with once the checksums live a document away
    /// (`AMB-D-386`): an install records it beside the binary, and a later fetch of the list alone reports
    /// a candidate by finding the entry's digest no longer the one recorded.
    ///
    /// Absent means unknown, never "current" — a manifest placed by hand has no digest to compare, so it
    /// is never reported as updatable. [`plugin_wire::split`](crate::plugin_wire::split) empties the slot
    /// on the way out, because the digest is over bytes that do not exist until the catalog publishes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_sum: Option<String>,
    /// **What layer this plugin lives at** — a project's, or the device's ([`Scope`], `AMB-D-601`).
    /// Absent means [`Scope::Project`], which is both the answer that fits most plugins and the safe one:
    /// a plugin that declares nothing sees the one project that turned it on. Declaring `machine` is the
    /// author saying their plugin's work is the machine's, and the faces then say so where a user takes it
    /// on — it reads every project on the device.
    ///
    /// The default is what keeps the entries already published working unchanged: none of them writes this
    /// key, and none of them has to be republished to keep passing the door. A value outside the two is
    /// refused where every other shape error is (`AMB-D-354`): the manifest does not parse, so it never
    /// reaches the rules or the catalog.
    #[serde(default)]
    pub scope: Scope,
    /// The event-payload contract version this plugin reads (`AMB-D-349` — a single integer `v` for the
    /// whole contract, evolving additively). It lets Amenbo notice when its own `v` has moved past what a
    /// plugin understands and warn or refuse rather than silently feed it a payload it cannot parse
    /// (`AMB-D-359`). Absent means the v1 baseline — a manifest written before this field targets the
    /// original contract, not whatever version the reading Amenbo happens to be at. This module only
    /// *carries* the number; the enable/run-time comparison is [`crate::plugin_compat`]'s, not the type's.
    #[serde(default = "default_payload_v")]
    pub payload_v: u32,
    /// The minimum Amenbo version this plugin needs, as a semver string — below it, Amenbo warns or
    /// refuses to enable/run the plugin (`AMB-D-359`). Absent means no floor: the plugin declares no
    /// version requirement. Stored opaquely, like `checksum` — this module neither parses nor compares
    /// it; reading it is [`crate::plugin_compat`]'s, so the one truth about version ordering lives with
    /// the gate that acts on it (a string it cannot parse is a floor Amenbo will not claim to meet).
    /// A value that reads as no version at all is refused earlier, at the manifest door
    /// ([`crate::plugin_validate`]), so it does not reach that gate through a fresh install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_amenbo: Option<String>,
    /// The plugin's configuration schema: a flat list of fields the author declares so Amenbo can
    /// render a form, store the values, and inject them at run time (`AMB-D-356`). Absent means the
    /// plugin takes no configuration — the safe default is an empty schema, so an older manifest with
    /// no `config` key is a plugin with no settings, not a parse error.
    ///
    /// The list is **the whole schema**: no validation rules, and no notion of what a value means (a URL,
    /// an email) — that judgement is the author's at run time. What Amenbo reads here is only which fields
    /// exist, which are secret (so the store never sees them — `AMB-D-356`), which are required (so
    /// `enable` is blocked until they are filled — `AMB-D-351`), and what shape each answer takes:
    /// [`ConfigField::field_type`] with its candidates and default (`AMB-D-415`), which is what a form can
    /// be drawn from and what a written value is admitted against.
    ///
    /// An empty schema does not serialize (`skip_serializing_if`), so a re-emitted manifest for a plugin
    /// with no settings is byte-for-byte what an author who omitted `config` wrote — the absent and the
    /// empty forms stay the same document.
    ///
    /// **Not every entry is a field** (`AMB-D-727`): the list may also carry parts for Amenbo to draw
    /// where they stand — see [`ConfigEntry`]. What takes a value is [`Manifest::fields`], which is what
    /// every layer that stores, injects or judges a value asks for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<ConfigEntry>,
    /// **Where the author's own code is called from the settings face** (`AMB-D-664`) — see [`Settings`].
    /// Absent means nowhere, which is what every manifest written before this block says: the plugin is
    /// enabled on the presence check alone, and its form is fields and a save button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    /// The observation events this plugin subscribes to — each an [`EventSubscription`] naming a v1 event
    /// ([`plugin_payload::V1_EVENTS`](crate::plugin_payload::V1_EVENTS)) and how its hook fires. The
    /// subscription resolver (`AMB-D-367`, `AMB-T-2032`) fires an enabled plugin only for an event whose
    /// name appears here, so a plugin with no `events` observes nothing — a command-only plugin declares an
    /// empty list. A subscription may be a bare event-name string (the notification default: both faces, no
    /// reply) or an object narrowing `faces` / asking for `reply` (`AMB-D-383`) — the two forms round-trip,
    /// a bare string re-emitting as a bare string.
    ///
    /// **This module only carries the subscriptions**; that each names a real v1 event, and that a `reply`
    /// hook declares `faces: [cli]` with a non-empty face set, are the validator's to enforce
    /// (`AMB-D-354` / `AMB-D-383` / `AMB-T-1988`) — the one home for the rules — and an unrecognised event
    /// name is simply inert here, since only catalog events are ever fired. Absent means no subscription,
    /// and an empty list does not serialize, so a re-emitted manifest for a command-only plugin is
    /// byte-for-byte what an author who omitted `events` wrote (the same absent-equals-empty rule as
    /// `config`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventSubscription>,
    /// **What this plugin says for itself at the AI's entry point** (`AMB-D-437`) — see [`AgentGuide`].
    /// Absent means it says nothing there, which is what every manifest written before this field says:
    /// the block is additive, so it moves neither `payload_v` nor `min_amenbo`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentGuide>,
}

impl Manifest {
    /// **The settings this plugin takes**, in the order they are declared — [`config`](Manifest::config)
    /// with the parts Amenbo merely draws left out (`AMB-D-727`).
    ///
    /// This is what every layer behind the screen wants: what is stored, what is injected, what
    /// `required` is read off, what a check may name. A part has no key and no value, so it is not one
    /// of them, and asking for it here would put a caption into the config table.
    ///
    /// It clones because the callers take a slice and there is nowhere to borrow one from — the entries
    /// are interleaved. A schema is capped at
    /// [`MAX_CONFIG_FIELDS`](crate::plugin_validate::MAX_CONFIG_FIELDS) small structs, so the copy is
    /// paid once per run and is not worth an iterator in every signature that reads a field.
    pub fn fields(&self) -> Vec<ConfigField> {
        self.config.iter().filter_map(ConfigEntry::field).cloned().collect()
    }

    /// **The distributable this manifest offers for one OS** (`AMB-D-381`) — the [`assets`](Manifest::assets)
    /// entry when the manifest is the per-OS kind, the single `url`/`checksum`/`signature` when it is the
    /// one-file kind. Every layer that fetches, verifies or compares an asset goes through here, so the
    /// two forms are resolved in exactly one place.
    ///
    /// Resolution is **exact then OS-wide** (`AMB-D-384`): the running platform's `<os>-<arch>` key is
    /// tried first, then its arch-agnostic `<os>` key. A machine whose arch Amenbo cannot name
    /// ([`Arch::here`] is `None`) skips the exact step and can only match an arch-agnostic asset — an
    /// `<os>-<arch>` build cannot be claimed to run on an arch we cannot even name.
    ///
    /// `None` means this manifest publishes nothing for that platform. A declared `assets` map is taken at
    /// its word and never falls back to the single fields: the fallback would hand out bytes built for
    /// another platform, which is a worse answer than none. The door keeps the map and the `os` set in step
    /// ([`crate::plugin_validate`]), so `None` here is either a platform not published for or a manifest
    /// that never went through the door — a hand-placed one, or an entry whose `os` and `assets` disagree.
    pub fn asset_for(&self, here: Platform) -> Option<Asset> {
        if !self.assets.is_empty() {
            if here.arch.is_some() {
                if let Some(asset) = self.assets.get(&here) {
                    return Some(asset.clone());
                }
            }
            return self.assets.get(&Platform { os: here.os, arch: None }).cloned();
        }
        (!self.url.is_empty()).then(|| Asset {
            url: self.url.clone(),
            checksum: self.checksum.clone(),
            signature: self.signature.clone(),
        })
    }
}

/// **One operating system's distributable** (`AMB-D-381`): where the bytes are, what they must hash to,
/// and who signed them.
///
/// The three travel together because all three are about one set of bytes. A native plugin ships a
/// different binary per platform, so a checksum or a signature covering "the plugin" would cover nothing
/// in particular — provenance is only meaningful over the exact file that will run here (`AMB-D-371`).
/// The same three fields appear flat on [`Manifest`] for the one-file form; [`Manifest::asset_for`] is
/// where the two forms become one answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Where this OS's asset is fetched from on install.
    pub url: String,
    /// This asset's integrity digest (`sha256:<hex>`), verified against the exact bytes the url served.
    pub checksum: String,
    /// This asset's minisign signature, produced by the catalog CI over these bytes (`AMB-D-371`).
    /// Absent means unsigned, which the install door refuses (`AMB-D-351`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// **What kind of value a field holds** (`AMB-D-415`) — the one thing about a value Amenbo does read, and
/// it reads it only to know what to draw and what to accept.
///
/// A field is one of three kinds, and two of them live here: a plain line of text, and a choice among
/// candidates the author declares ([`ConfigField::options`]). The third — a secret — is the `secret` flag
/// beside this, since being hidden is about *where the value is stored*, not about what shape it has.
///
/// **The meaning of the value is still the author's** (`AMB-D-356`). A `Text` field is any line at all;
/// Amenbo does not know a URL from an email. What [`Multi`](FieldType::Multi) adds is not a type system but
/// a shorter road to a right answer: a value the author already knows the candidates for should never be
/// something the user has to spell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// One line the user types. The default — a manifest that omits `type` declares a text field, which is
    /// what every field written before the key existed is.
    #[default]
    Text,
    /// Any number of the candidates the field declares. Stored as one string, the chosen
    /// [`value`](ConfigOption::value)s joined by commas, or [`NONE_SELECTED`] for a deliberate empty
    /// choice (`AMB-D-415`).
    Multi,
}

impl FieldType {
    /// Whether this is the default kind, so a field that never declared `type` re-emits without the key —
    /// the absent-equals-default rule the rest of the manifest keeps.
    fn is_text(&self) -> bool {
        matches!(self, FieldType::Text)
    }
}

/// **The value a [`Multi`](FieldType::Multi) field stores when the user chose nothing** (`AMB-D-415`).
///
/// A field has three states, and the empty string cannot hold two of them: it already means *unset*
/// everywhere else (the clear path at the write boundary, the reading `required` takes), and "I looked at
/// the candidates and want none of them" is a different answer from "I have not been here yet" — only the
/// second follows [`ConfigField::default`]. So the deliberate empty choice is stored as this word, which is
/// why the validator refuses it as a candidate's own `value`: one string cannot mean both.
///
/// The word is Amenbo's, not the author's: what a plugin receives is the resolved value (`AMB-D-415`), so
/// nothing a plugin reads has to know the reserved spelling.
pub const NONE_SELECTED: &str = "none";

/// One candidate a [`Multi`](FieldType::Multi) field offers (`AMB-D-415`): the value that is stored, and
/// the words shown beside its checkbox.
///
/// The pair exists because the two audiences differ — the plugin receives `value` and the user reads
/// `label`, so an author can name an event `task.status_changed` on the wire and give it a sentence in
/// their own language on the screen, without either side compromising for the other.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigOption {
    /// What is stored and handed to the plugin when this candidate is chosen. Commas are forbidden (the
    /// validator's rule): the chosen values are joined by one, so a value carrying its own would not
    /// survive the round trip.
    pub value: String,
    /// The human-readable label shown beside this candidate. Display text only.
    pub label: String,
    /// **When this candidate is offered at all** (`AMB-D-727`) — an iCloud transport has nothing to offer
    /// a Windows machine, and a checkbox for it is one more thing to read past. Empty means always, which
    /// is what every candidate written before the key existed says.
    ///
    /// The reading is [`crate::plugin_when`]'s, and it hides the checkbox rather than the answer: a
    /// candidate already chosen stays chosen and still reaches the plugin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<crate::plugin_when::When>,
}

impl ConfigOption {
    /// A candidate offered unconditionally — the pair an author writes when they name a value and the
    /// words beside its checkbox and nothing else. [`when`](ConfigOption::when) is what narrows it.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        ConfigOption { value: value.into(), label: label.into(), when: Vec::new() }
    }
}

/// **One entry in a plugin's `config` list** (`AMB-D-727`) — a setting somebody fills in, or a part
/// Amenbo simply draws.
///
/// The two live in one list because *where* a part sits is what it is for: a way to the page that issues
/// the token belongs above the box that token goes in, and a list of settings with a separate list of
/// captions beside it cannot say that. So the order an author writes is the order a form is drawn in, and
/// a part is written where it should appear.
///
/// **Why static parts at all, when a run can answer with the same ones.** A run answers when it is run,
/// and the reader who most needs the way to that page has not run anything: `mail` declares no operation
/// to press and its check fires at the enable, which is *after* the point where somebody is looking for
/// where to get a password. Before this, both official plugins buried the address in `help` prose, where
/// it is a string to retype rather than a button.
///
/// The parts are [`plugin_show::Part`](crate::plugin_show::Part) — the same seven a run answers with, held
/// to the same rules, `qr` and `link` official-only in both places (`AMB-D-727`).
///
/// It reads untagged because a manifest is written by hand: a setting is an object with a `key` and a
/// `label`, a part is an object named for what it draws, and nothing has to be spelled twice to tell
/// them apart. What that costs is the error message on a malformed entry, which is why
/// [`plugin_validate`](crate::plugin_validate) is where an author is told what is wrong with one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigEntry {
    /// A setting the plugin takes — the whole of what `config` held before this.
    Field(Box<ConfigField>),
    /// Something for Amenbo to draw where it stands, filled in by nobody — see [`ConfigPart`].
    Part(ConfigPart),
}

impl ConfigEntry {
    /// The setting, when this entry is one. `None` is a part, which nobody fills in.
    pub fn field(&self) -> Option<&ConfigField> {
        match self {
            ConfigEntry::Field(field) => Some(field),
            ConfigEntry::Part(_) => None,
        }
    }

    /// The part, when this entry is one. `None` is a setting.
    pub fn part(&self) -> Option<&ConfigPart> {
        match self {
            ConfigEntry::Part(part) => Some(part),
            ConfigEntry::Field(_) => None,
        }
    }

    /// **When this entry is drawn at all** (`AMB-D-727`) — the conditions written on it, whichever kind
    /// it is. Empty is an unconditional entry.
    ///
    /// One accessor because a form walks the list once, in the author's order, and asking a field and a
    /// part in two different ways is how the two end up read by two different rules.
    pub fn when(&self) -> &[crate::plugin_when::When] {
        match self {
            ConfigEntry::Field(field) => &field.when,
            ConfigEntry::Part(part) => &part.when,
        }
    }
}

/// **A part written into a manifest's `config` list, and when it is drawn** (`AMB-D-727`).
///
/// The part itself is [`plugin_show::Part`](crate::plugin_show::Part), flattened, so what an author
/// writes is the same object a run answers with — `{ text: "…" }`, `{ qr: "…" }` — and the condition sits
/// beside it as one more key:
///
/// ```yaml
/// config:
///   - note: "Raise the Cloudflare Worker before filling this in"
///     when:
///       - { field: transport, has: cloudflare }
/// ```
///
/// **Why a part needs one at all.** A field and its caption are a pair: a way to the page that issues a
/// token stands above the box that token goes in, and hiding the box while the caption stays leaves a
/// step nobody can follow — the same reason an operation carries one
/// ([`SettingsAction::when`]). The reading is [`plugin_when`](crate::plugin_when)'s, the one every other
/// condition is read by.
///
/// A run's answer carries no condition and needs none: it is drawn because something was pressed, and
/// what the author knew when they wrote the manifest they know again when their own code runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigPart {
    /// What Amenbo draws — the part, written as the object it is.
    #[serde(flatten)]
    pub part: crate::plugin_show::Part,
    /// **When it is drawn** (`AMB-D-727`). Empty means always, which is what every part written before
    /// the key existed says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<crate::plugin_when::When>,
}

impl ConfigPart {
    /// A part drawn unconditionally — what an author writes when they name one and nothing else.
    /// [`when`](ConfigPart::when) is what narrows it.
    pub fn new(part: crate::plugin_show::Part) -> Self {
        ConfigPart { part, when: Vec::new() }
    }
}

impl From<crate::plugin_show::Part> for ConfigEntry {
    fn from(part: crate::plugin_show::Part) -> Self {
        ConfigEntry::Part(ConfigPart::new(part))
    }
}

impl ConfigEntry {
    /// A `config` list of settings alone, in order — what every manifest written before parts existed is
    /// (`AMB-D-727`), and the shortest way for a caller holding fields to say so.
    pub fn schema(fields: impl IntoIterator<Item = ConfigField>) -> Vec<ConfigEntry> {
        fields.into_iter().map(ConfigEntry::from).collect()
    }
}

impl From<ConfigField> for ConfigEntry {
    fn from(field: ConfigField) -> Self {
        ConfigEntry::Field(Box::new(field))
    }
}

/// One field of a plugin's configuration schema (`AMB-D-356`, `AMB-D-415`). The author declares a flat list
/// of these in the manifest; Amenbo renders each as one form field, routes its value to storage by the
/// `secret` flag, and injects it into the plugin at run time.
///
/// **Amenbo still carries no notion of what a value means** — no pattern, no validation rule. What it
/// reads is only: `secret` (where the value is stored and how it is injected), `required` (whether an
/// unset field blocks `enable`), and — since `AMB-D-415` — [`field_type`](ConfigField::field_type) with
/// its [`options`](ConfigField::options) and [`default`](ConfigField::default), which say what to draw and
/// which answers are admissible. The last three are about the *form of the answer*, never about what the
/// answer signifies. The three `AMB-D-656` adds keep that line: [`help`](ConfigField::help) and
/// [`placeholder`](ConfigField::placeholder) are text Amenbo shows and never reads, and
/// [`readonly`](ConfigField::readonly) says *who writes* the value, not what a written one may be.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigField {
    /// The field's stable key — its identity in storage and the name it is injected under (the env var
    /// for a secret, the JSON key on stdin for the rest). Not shown to the user; `label` is.
    pub key: String,
    /// The human-readable label the form shows beside the field. Display text only.
    pub label: String,
    /// What this field is for, in the author's words (`AMB-D-656`) — the paragraph a one-line `label` has
    /// nowhere to hold, drawn under the input and shown by `plugin config get`. Absent means the label is
    /// the whole of what a reader gets, which is what every schema written before this key says.
    ///
    /// **Plain text, and drawn as plain text** — the validator forbids a control character
    /// ([`crate::plugin_validate`]), and the faces that show it render neither Markdown nor a link: this
    /// is the screen a user types a secret into, and a destination the author chose does not belong on it
    /// (`AMB-D-656`). A newline is text here, not a control character, since this is a body and not a line.
    ///
    /// **It does not travel to where an AI reads** (`AMB-D-575`, `AMB-D-576`): author prose landing in
    /// `agent --json` reads as instruction, so this is a human-face string only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// One example of what to type (`AMB-D-656`), shown greyed inside an empty input and nowhere else —
    /// it is neither a value nor a [`default`](ConfigField::default), and nothing is stored from it.
    ///
    /// The distinction is `AMB-D-474`'s: an example written as a `default` is a value the plugin really
    /// receives, so a user who enables without touching the field sends mail to the example address. An
    /// example written here is only ever read.
    ///
    /// One line, plain text, and — like [`help`](ConfigField::help) — off the AI's face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Whether the value is a secret. **The author declares this; Amenbo does not judge it** (`AMB-D-356`)
    /// — Amenbo cannot know a webhook URL is sensitive, so it trusts the flag. A secret is stored in the
    /// store table an `export` must leave (`AMB-D-434`) and injected as an environment variable (off
    /// argv, off logs); a non-secret is stored in the ordinary table and injected on stdin.
    /// Absent means `false` — the safe-for-storage default is *not* secret only for a field the author
    /// left unmarked, which is a plain-text field by construction.
    #[serde(default)]
    pub secret: bool,
    /// Whether the field must hold a value before the plugin may be enabled (`AMB-D-351`, fail-closed).
    /// Amenbo only checks presence (a non-empty value); it does not check the value is *valid*. Absent
    /// means `false`. Separate from [`default`](ConfigField::default): a field that carries one is never
    /// unanswered, so it does not block `enable` however it is marked here.
    #[serde(default)]
    pub required: bool,
    /// **Who writes this field's value** (`AMB-D-656`): the plugin itself, not the user. Absent means
    /// `false` — a field the user fills in, which is what every field is by default.
    ///
    /// A plugin writes back through `plugin config set` (`AMB-D-406`), and the value it wrote is one the
    /// user has no business editing or clearing — viewer's three fields are generated by its `setup`, and
    /// a form that offers a clear button beside them offers a way to break the plugin. So this binds the
    /// **screen**: the form shows the value without an input or a clear button. It does not bind the
    /// write path, which is the road the plugin's own value arrives by.
    ///
    /// Orthogonal to [`required`](ConfigField::required) — a generated value may still be one the plugin
    /// cannot run without, and declaring both means `enable` stays shut until `setup` has run. It does
    /// not sit with [`default`](ConfigField::default), though, and the validator says so: a value that has
    /// an answer before anyone generates one is not a generated value.
    ///
    /// **An older Amenbo ignores this key** — [`Manifest`] parsing keeps what it does not know rather than
    /// refusing it, so a field declared readonly stays editable until the user's build reads the key. The
    /// declaration is safe to publish early; it is simply not yet in force.
    #[serde(default, skip_serializing_if = "is_false")]
    pub readonly: bool,
    /// What kind of value this field holds (`AMB-D-415`) — spelled `type` in a manifest. Absent means
    /// [`Text`](FieldType::Text), so a schema written before the key existed is a schema of text fields,
    /// and re-emits without the key.
    #[serde(rename = "type", default, skip_serializing_if = "FieldType::is_text")]
    pub field_type: FieldType,
    /// The candidates a [`Multi`](FieldType::Multi) field offers, in the order the form shows them
    /// (`AMB-D-415`). Empty is the only shape a text field may have — declaring candidates for a field
    /// that is not a choice is a mistake the validator names, not a silent ignore.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ConfigOption>,
    /// The value that is in force while the field is unset (`AMB-D-415`). Absent means there is none: an
    /// unset field is simply unanswered.
    ///
    /// For a [`Multi`](FieldType::Multi) field this is a comma-joined subset of the declared
    /// [`options`](ConfigField::options) — the validator keeps it one — and for a text field it is the line
    /// the plugin gets until a user writes another. Either way it is resolved on the way *out*, at
    /// injection: what the store holds for an unset field is still nothing, so a later change to the
    /// manifest's default reaches every project that never answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// **When this field is drawn at all** (`AMB-D-727`) — the Cloudflare credentials belong on the form
    /// of someone who chose Cloudflare, and nowhere else. Empty means always, which is what every field
    /// written before the key existed says.
    ///
    /// **It hides the box, not the value** ([`crate::plugin_when`]). What the store holds is handed to the
    /// plugin whether the field is on screen or not, so a value answered on a Mac is still there when the
    /// store is opened on Windows.
    ///
    /// It does bind one thing besides the drawing: while the field is hidden, an empty
    /// [`required`](ConfigField::required) does not shut the enable gate
    /// ([`crate::plugin_trust::missing_required`]) — a gate held shut over a box nobody can see is one
    /// nobody can open.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<crate::plugin_when::When>,
}

impl ConfigField {
    /// A field with every default: a plain text line the user fills in, no candidates, no default value,
    /// no supporting text, neither secret nor required. The shape an author writes when they declare a
    /// `key` and a `label` and nothing else, and the base the other kinds are written on top of.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        ConfigField {
            key: key.into(),
            label: label.into(),
            help: None,
            placeholder: None,
            secret: false,
            required: false,
            readonly: false,
            field_type: FieldType::Text,
            options: Vec::new(),
            default: None,
            when: Vec::new(),
        }
    }
}

/// Whether a flag is at its absent value, so a manifest that never declared it re-emits without the key —
/// the absent-equals-default rule the keys added since `AMB-D-415` keep. (`secret` and `required` predate
/// it and are always written, which is a document shape that cannot be narrowed now without changing every
/// re-emitted manifest.)
fn is_false(flag: &bool) -> bool {
    !*flag
}

/// **How a plugin names itself where an AI reads how to work here** (`AMB-D-437`): the occasion to reach
/// for it, and the calls it answers.
///
/// `amenbo agent --json` is the one document an AI is pointed at, and a plugin the user installed and
/// enabled is part of the answer that document owes. Amenbo has no words of its own for what a third
/// party's plugin is for, so what rides there is what the author wrote here — read at run time, never
/// written into Amenbo's source (`AMB-D-346`). A plugin renamed or retired takes its own sentences with it.
///
/// ```yaml
/// agent:
///   when: when to reach for this plugin (one line)
///   commands:
///     - cmd: <subcommand and arguments>
///       does: what it does, and what it returns (one line)
///       steps: [<the ids of Amenbo's own steps this call is a tool for>]
/// ```
///
/// **The calling form is Amenbo's to build.** [`cmd`](AgentCommand::cmd) holds the plugin's own command
/// face alone; the entry point puts `amenbo plugin run <name> ` in front of it from the name it just read,
/// so an AI receives a line it can type. An author writing the whole line would be writing their own name
/// into it, which is the one thing this shape keeps out.
///
/// **Where a call belongs is named by id, never in prose** (`AMB-D-571`). An author who wants their
/// call to appear beside the step it serves names that step in [`AgentCommand::steps`]; the step then
/// carries the calling form alone, and every sentence about it stays here.
///
/// **The text is the author's one language.** Amenbo's own entry point carries its wording in more than
/// one, and it holds no translation for a plugin's — what was written is what is relayed.
///
/// **Shape only, like the rest of this module**: that `when` says something, and how long and how many the
/// lines may be, are the validator's (`AMB-D-354`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGuide {
    /// The one line saying when to reach for this plugin — required, since a block naming no occasion
    /// gives a reader nothing to act on.
    pub when: String,
    /// The plugin's command face, one entry per call. Absent means none: a plugin whose whole surface is
    /// observation hooks names its occasion and stops there (`AMB-D-437`). An empty list does not
    /// serialize, so the absent and the empty forms stay one document — the rule `config` and `events`
    /// already follow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<AgentCommand>,
}

/// One call an author puts on the record (`AMB-D-437`): what to type, what comes back, and where in
/// Amenbo's own working cycle it is a tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCommand {
    /// The subcommand and its arguments — without the `amenbo plugin run <name>` the reader prepends.
    pub cmd: String,
    /// One line: what the call does, and what it returns.
    pub does: String,
    /// **The steps of Amenbo's own cycle this call is a tool for** (`AMB-D-571`), each named by id:
    /// `<run>.<step>`, where the run is `agentCycle` for the backbone or a cycle's key
    /// (`cycles.worktree` is written `worktree`), and the step is its id within that run. What travels
    /// to a named step is the calling form and nothing else — the sentences stay in this block, where a
    /// reader meets them as the author's rather than as Amenbo's (`AMB-D-437`/`AMB-D-571`).
    ///
    /// **Naming a step no longer there is not an error.** The steps travel with Amenbo while a manifest
    /// stays where it was installed, and a step can be renamed, retired, or — like the `worktree` cycle
    /// off a git checkout — left out of the run the reader is handed. A ref that resolves to nothing
    /// hangs nowhere and takes nothing with it. Absent means the call is a tool for no step in
    /// particular, which is what a manifest written before this field says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

impl AgentCommand {
    /// A call tied to no step of Amenbo's own cycle — what an author writes who names their command
    /// face and stops there.
    pub fn new(cmd: impl Into<String>, does: impl Into<String>) -> Self {
        AgentCommand { cmd: cmd.into(), does: does.into(), steps: Vec::new() }
    }
}

/// **Where the author's own code is called from the settings face** (`AMB-D-664`): the check that runs when
/// the plugin is enabled, and the operations a user presses.
///
/// ```yaml
/// settings:
///   check: config check          # run when the plugin is enabled — one call
///   actions:                     # what a user may press (ten at most)
///     - cmd: config test
///       label: Send a test message
///       ask:                     # handed to that one run, and never stored
///         - key: api_token
///           label: API token
///           secret: true
/// ```
///
/// **The call is the command face the plugin already has** (`AMB-D-353`). What is written here is the same
/// subcommand-and-arguments line [`AgentCommand::cmd`] holds, and the run puts `plugin run <name>` in front
/// of it — so this block adds no protocol and no second way of speaking to a plugin. It says only *where*
/// one of the calls a plugin already answers is raised, which is the one thing Amenbo was missing: the
/// values are already injected, and the answer already comes back on stdout.
///
/// **Only these two are reachable from the settings face** (`AMB-D-522`). A call taking arguments a caller
/// chooses stays `plugin run`'s, on the CLI — what a form may raise is what the manifest named in advance.
///
/// **The block only says where.** When each is run, what a check's answer must look like, and what a
/// failing one costs, are the run boundary's (`AMB-D-664`, [`crate::plugin_invoke`]) — as is every other
/// question about a plugin's output (`AMB-D-354`). What is here is the declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// **The call that says whether these values are usable** (`AMB-D-664`), run when the plugin is
    /// enabled and again after a save while it is enabled. Absent means none, and the gate is the presence
    /// check it always was — `required` asks whether a field holds something, and nothing more
    /// ([`ConfigField::required`]).
    ///
    /// One call, not a list: the author's code has every value in hand at once, so a second call could only
    /// look at the same values again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// **What a user may press on the settings form** (`AMB-D-664`) — a connectivity test, a `setup` that
    /// writes its result back through `plugin config set` (`AMB-D-406`). Absent means the form is fields and
    /// a save button, as it was.
    ///
    /// An empty list does not serialize, so the absent and the empty forms stay one document — the rule
    /// `config` and `events` already follow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SettingsAction>,
}

/// **One operation a user may press** (`AMB-D-664`): the call it raises, the words on the button, and the
/// input it needs that nothing should keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsAction {
    /// The subcommand and its arguments — the plugin's own command face, without the `amenbo plugin run
    /// <name>` the run prepends, exactly as [`AgentCommand::cmd`] holds one.
    pub cmd: String,
    /// The words on the button, in the author's language. Display text, drawn plain — no Markdown and no
    /// link, like every other author string on this screen (`AMB-D-656`). It is short because a button is:
    /// the cap is the validator's ([`crate::plugin_validate`]).
    ///
    /// **Translated where the form's labels are** (`AMB-D-620`): it is read on the settings screen, so it
    /// travels with the words beside it rather than staying the one language it was written in.
    pub label: String,
    /// **What this run needs and nothing keeps** (`AMB-D-664`) — a token pasted once, a one-time code. The
    /// value is handed to the process this press starts and is stored nowhere: not in the config table, not
    /// in the secret store, not in the form.
    ///
    /// Absent means the press needs nothing beyond the values already saved, which is what most operations
    /// are. An empty list does not serialize, as everywhere else.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ask: Vec<AskField>,
    /// **When this button is offered at all** (`AMB-D-727`) — someone who chose only iCloud has no use for
    /// "raise a Cloudflare tunnel", and hiding that transport's fields while leaving its button turns the
    /// form into a step nobody can follow. Empty means always.
    ///
    /// Read exactly as a field's is ([`crate::plugin_when`]): the same two kinds of clause, judged against
    /// the same layer's answers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<crate::plugin_when::When>,
}

impl SettingsAction {
    /// An operation offered unconditionally, asking for nothing beyond the values already saved — the
    /// button an author writes when they name a call and the words on it.
    pub fn new(cmd: impl Into<String>, label: impl Into<String>) -> Self {
        SettingsAction { cmd: cmd.into(), label: label.into(), ask: Vec::new(), when: Vec::new() }
    }
}

/// **One input an operation asks for, for that run alone** (`AMB-D-664`).
///
/// It looks like a [`ConfigField`] and is the opposite of one: a config field is a value the user answers
/// once and Amenbo keeps, and this is a value Amenbo never has. So it carries the three things a box needs
/// to be drawn and handed over, and none of the ones that only make sense for a value with a life after the
/// press — a `default` is a stored answer to a question that is asked every time, and `required` is a gate
/// on enabling that this is not on either side of.
///
/// **Those two are refused rather than ignored** ([`crate::plugin_validate`]). They are the keys an author
/// carries over when they copy a config field, and a key that is quietly dropped costs them a value they
/// believe is being asked for. Every *other* unknown key is still ignored, as everywhere in this module: a
/// key a later Amenbo adds must not make this one refuse the manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskField {
    /// The name the value is handed over under — a plain identifier, like a [`ConfigField::key`], since it
    /// becomes an environment variable's stem. It may not be a key the form already stores: one name
    /// cannot mean both a saved value and a value that is never saved.
    pub key: String,
    /// The label shown beside the box. Display text, and translated with the rest of the form
    /// (`AMB-D-620`).
    pub label: String,
    /// Whether the box hides what is typed into it. The author's declaration, as on
    /// [`ConfigField::secret`] — Amenbo does not judge which values are sensitive (`AMB-D-356`).
    ///
    /// Unlike a config field's, this decides only what the screen shows: where the value is stored is not a
    /// question here, because it is not stored. Absent means `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub secret: bool,
    /// What this field named that an ask does not have — kept only so the key can be named back to its
    /// author, never read for what it held. The same slot, and the same reason, as
    /// [`ManifestOverlay::extra`].
    #[serde(flatten)]
    pub extra: BTreeMap<String, Ignored>,
}

/// **What one manifest says in one other language**, keyed by the language code it is written in
/// (`AMB-D-621`) — `ja`, `zh-Hans`, one of [`crate::config::LANGUAGES`].
///
/// The author writes each language as a file beside the manifest (`plugins/mail.ja.yaml`); Amenbo
/// publishes them split across the two catalog documents (`AMB-D-622`, [`crate::plugin_wire::split`]).
/// Which languages exist is therefore a map, never a field on [`Manifest`]: the manifest is what the
/// author wrote, and this is what someone else wrote it as.
pub type Translations = BTreeMap<String, ManifestOverlay>;

/// **The translated layer of one manifest, in one language** (`AMB-D-621`).
///
/// It mirrors the manifest's shape and carries only the fields a person reads on a GUI face
/// (`AMB-D-620`, `AMB-D-638`): the one-line `desc`, the description text a detail view draws, and the
/// words of the configuration form — its field labels, and the buttons the settings block puts beside
/// them (`AMB-D-664`). Everything else — the author's name, the category vocabulary, what the plugin says
/// at the AI's entry point — stays the one language it was written in, so there is no key here to write
/// it in another.
///
/// **Every field is optional**, because a translation is a layer and not a replacement: what an author
/// did not translate is not missing, it is the base value (`AMB-D-623`), and Amenbo never fills a gap on
/// their behalf. Selecting between the two is the GUI's, which is why nothing here resolves anything.
///
/// **Unknown keys are kept rather than ignored**, unlike [`Manifest`]'s. A manifest ignores what it does
/// not know so a newer document still parses on an older Amenbo; an overlay's unknown key is the
/// opposite situation — an author translating something Amenbo will never show, whose whole symptom is
/// silence. So the key is carried in [`extra`](ManifestOverlay::extra) and named back to the author by
/// the validator ([`crate::plugin_validate::validate_overlays`]), while the document still parses, which
/// is what lets one run report every mistake in it at once.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestOverlay {
    /// The one-line description, in this language. Absent means the base line is what a reader sees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// The description text a detail view draws, in this language (`AMB-D-638`). Absent means the base
    /// text is what a reader sees — the same field-by-field fallback the rest of the overlay takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// The configuration form's labels, in this language — **keyed by the field's
    /// [`key`](ConfigField::key)**, where the manifest declares an ordered list (`AMB-D-621`). A
    /// translation carries no order of its own, so pairing the two by position would mean an author
    /// re-ordering their form silently re-labels every language.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, ConfigFieldOverlay>,
    /// The words the settings block puts on the same form, in this language (`AMB-D-664`). Absent means
    /// the buttons read as their author wrote them, the same field-by-field fallback everything here
    /// takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<SettingsOverlay>,
    /// What this overlay named that Amenbo does not translate — kept only so the key can be named back
    /// to its author, never read for what it held.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Ignored>,
}

/// **The settings block's words, in one language** (`AMB-D-664`) — the translated half of [`Settings`].
///
/// Only what a person reads is here, which on this block is the buttons: `check` and every `cmd` are
/// calls the plugin answers, not text anyone is shown, so there is no key to write them in another
/// language.
///
/// ```yaml
/// # plugins/mail.de.yaml
/// settings:
///   actions:
///     config test:                     # the base action's `cmd` is the key
///       label: Testnachricht senden
///       ask:
///         api_token: API-Token         # the base ask's `key` is the key
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsOverlay {
    /// The operations' words, **keyed by the operation's [`cmd`](SettingsAction::cmd)**, where the
    /// manifest declares an ordered list — the pairing every translation here takes (`AMB-D-621`), for
    /// the reason the config fields take it: a translation carries no order of its own, so an author
    /// re-ordering their buttons would otherwise silently re-label every language.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub actions: BTreeMap<String, SettingsActionOverlay>,
    /// What this overlay named that Amenbo does not translate, as on [`ManifestOverlay`].
    #[serde(flatten)]
    pub extra: BTreeMap<String, Ignored>,
}

/// **One operation's words, in one language** (`AMB-D-664`) — the translated half of a
/// [`SettingsAction`]. Its `cmd` is the call it raises and is not translated; the button and the boxes a
/// press puts in front of the user are.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsActionOverlay {
    /// The words on the button, in this language. Absent means the base label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The labels beside the boxes this press asks for, **keyed by the ask's [`key`](AskField::key)** —
    /// the name the value is handed over under, which is the plugin's wire vocabulary and so is never
    /// translated.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ask: BTreeMap<String, String>,
    /// What this overlay named that Amenbo does not translate, as on [`ManifestOverlay`].
    #[serde(flatten)]
    pub extra: BTreeMap<String, Ignored>,
}

/// **One configuration field's words, in one language** (`AMB-D-621`) — the translated half of a
/// [`ConfigField`]. The field's `key`, its type, and what it stores are the plugin's wire vocabulary and
/// are not translated; what a person reads on the form is: its label, its supporting text
/// (`AMB-D-656`), and its candidates' labels.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFieldOverlay {
    /// The label shown beside the field, in this language. Absent means the base label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The field's [`help`](ConfigField::help), in this language (`AMB-D-656`). Absent means the base
    /// text is what a reader sees — the field-by-field fallback the whole overlay takes (`AMB-D-623`).
    ///
    /// A label translated while the paragraph under it stays English is half a form: the two are read
    /// together, so they are translated together (`AMB-D-620`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// The field's [`placeholder`](ConfigField::placeholder), in this language (`AMB-D-656`). Absent
    /// means the base example.
    ///
    /// An example is not always the same string in every language — a date, an address, a name written
    /// the way the reader writes one — so it is the author's to restate, not Amenbo's to carry over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The candidates' labels, in this language — **keyed by the candidate's stored
    /// [`value`](ConfigOption::value)**, for the same reason the fields are keyed rather than ordered.
    /// The value itself is what travels to the plugin, so it is never translated.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
    /// What this overlay named that Amenbo does not translate, as on [`ManifestOverlay`].
    #[serde(flatten)]
    pub extra: BTreeMap<String, Ignored>,
}

/// **Which language an overlay file beside a manifest is written in** (`AMB-D-621`) — the naming rule
/// `plugins/<name>.<lang>.yaml`, held here rather than at whichever road walks the directory.
///
/// `manifest_file` and `file` are file names, not paths; the answer is the token between the manifest's
/// own stem and its extension, and `None` for anything that is not an overlay of *that* manifest — the
/// manifest itself included, since its name has no token between the two.
///
/// **The token is not checked against the languages Amenbo reads.** A file named in a code from outside
/// them is still an overlay someone wrote, and the useful answer to it is the validator naming the code
/// ([`crate::plugin_validate::validate_overlays`]) — not this quietly declining to see the file, which is
/// the same silence as never having written it.
pub fn overlay_language<'a>(manifest_file: &str, file: &'a str) -> Option<&'a str> {
    let (stem, ext) = manifest_file.rsplit_once('.')?;
    let rest = file.strip_prefix(stem)?.strip_prefix('.')?;
    let lang = rest.strip_suffix(ext)?.strip_suffix('.')?;
    (!lang.is_empty()).then_some(lang)
}

/// A value Amenbo did not read, standing in for whatever an overlay wrote under a key it does not know.
/// The key is the whole of what anyone needs — it is what the validator names back to the author — so
/// the value is discarded on the way in rather than carried around untyped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ignored;

impl<'de> Deserialize<'de> for Ignored {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(d).map(|_| Ignored)
    }
}

impl Serialize for Ignored {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A running platform with no specific arch — matches only an arch-agnostic `<os>` key.
    fn plat(os: Os) -> Platform {
        Platform { os, arch: None }
    }

    /// A running platform with a specific arch — tries `<os>-<arch>` first, then `<os>`.
    fn plat_arch(os: Os, arch: Arch) -> Platform {
        Platform { os, arch: Some(arch) }
    }

    fn full_json() -> serde_json::Value {
        serde_json::json!({
            "name": "worktree",
            "desc": "Isolate each task in its own git worktree",
            "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo-plugin-worktree",
            "os": ["macos", "linux"],
            "category": "workflow",
            "url": "https://example.com/worktree-v1.tar.gz",
            "checksum": "sha256:deadbeef",
            "official": true,
            "scope": "project",
            "payload_v": 1,
            "min_amenbo": "1.8.0"
        })
    }

    #[test]
    fn a_full_entry_round_trips() {
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert_eq!(m.name, "worktree");
        assert_eq!(m.os, vec![Os::Macos, Os::Linux]);
        assert!(m.official);
        assert_eq!(m.payload_v, 1);
        assert_eq!(m.min_amenbo.as_deref(), Some("1.8.0"));
        // Re-serializing yields the same document.
        assert_eq!(serde_json::to_value(&m).unwrap(), full_json());
    }

    #[test]
    fn os_tokens_are_wharfy_vocabulary() {
        assert_eq!(Os::Macos.as_str(), "macos");
        assert_eq!(Os::Windows.as_str(), "windows");
        assert_eq!(Os::Linux.as_str(), "linux");
        assert_eq!(serde_json::to_value(Os::Macos).unwrap(), serde_json::json!("macos"));
        assert_eq!(Os::parse("linux"), Some(Os::Linux));
        assert_eq!(Os::parse("bsd"), None);
    }

    /// The layer is the author's declaration (`AMB-D-601`): absent means a project's plugin — the safe
    /// answer, since it then sees only what turned it on — and a value outside the two is a manifest that
    /// does not parse, which is where every other shape error is caught. The default is what lets the
    /// three entries already published keep passing the door without being republished.
    #[test]
    fn the_layer_defaults_to_project_and_rejects_anything_else() {
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("scope");
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.scope, Scope::Project, "an undeclared layer is the project's");

        let mut machine = full_json();
        machine["scope"] = serde_json::json!("machine");
        let m: Manifest = serde_json::from_value(machine).unwrap();
        assert_eq!(m.scope, Scope::Machine);
        assert_eq!(serde_json::to_value(&m).unwrap()["scope"], serde_json::json!("machine"));

        for bad in ["global", "workspace", "device", "Project", ""] {
            let mut v = full_json();
            v["scope"] = serde_json::json!(bad);
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "a layer outside the vocabulary must not parse: {bad}"
            );
        }
        assert_eq!(Scope::Project.as_str(), "project");
        assert_eq!(Scope::Machine.as_str(), "machine");
    }

    #[test]
    fn official_defaults_to_false_when_absent() {
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("official");
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert!(!m.official, "a manifest that does not claim official is not official");
    }

    #[test]
    fn a_missing_required_field_does_not_parse() {
        // The shape half of the fail-closed door: drop a required field and it fails to deserialize.
        // `url` and `checksum` are not among them — a manifest may publish per OS instead (`AMB-D-381`),
        // so which of the two forms it owes is a rule, and rules are the validator's.
        for field in ["name", "desc", "author", "repo", "os", "category"] {
            let mut v = full_json();
            v.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "a manifest missing `{field}` must not parse"
            );
        }
    }

    /// The two forms a manifest may take (`AMB-D-381`), and the one place they become one answer.
    #[test]
    fn the_asset_for_an_os_comes_from_the_map_when_there_is_one() {
        // One file for every OS: the single fields answer for whichever platform asks, arch and all.
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        let single = m.asset_for(plat_arch(Os::Macos, Arch::Arm64)).unwrap();
        assert_eq!(single.url, m.url);
        assert_eq!(single.checksum, m.checksum);
        assert_eq!(m.asset_for(plat(Os::Linux)).unwrap().url, m.url, "one url is every OS's");

        // Per OS: each platform gets its own bytes, and one this manifest does not publish for gets
        // nothing rather than another platform's build.
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("url");
        v.as_object_mut().unwrap().remove("checksum");
        v["assets"] = serde_json::json!({
            "macos": { "url": "https://example.com/x-macos.tar.gz", "checksum": "sha256:mac", "signature": "sig-mac" },
            "linux": { "url": "https://example.com/x-linux.tar.gz", "checksum": "sha256:linux" },
        });
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.asset_for(plat_arch(Os::Macos, Arch::Arm64)).unwrap().checksum, "sha256:mac");
        assert_eq!(m.asset_for(plat_arch(Os::Macos, Arch::Arm64)).unwrap().signature.as_deref(), Some("sig-mac"));
        assert!(m.asset_for(plat(Os::Linux)).unwrap().signature.is_none(), "unsigned is a shape, not an error");
        assert!(m.asset_for(plat(Os::Windows)).is_none(), "nothing published here is nothing, not a fallback");

        // And a map never falls back to the single fields, which would hand out another OS's binary.
        let mut v = full_json();
        v["assets"] = serde_json::json!({
            "macos": { "url": "https://example.com/x-macos.tar.gz", "checksum": "sha256:mac" },
        });
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert!(!m.url.is_empty(), "the single url is still in the document");
        assert!(m.asset_for(plat(Os::Windows)).is_none(), "and the map is still the only answer");
    }

    /// Arch resolution is exact-then-OS-wide (`AMB-D-384`): `<os>-<arch>` wins, `<os>` is the fallback, and
    /// a machine whose arch Amenbo cannot name matches only the arch-agnostic key.
    #[test]
    fn asset_for_resolves_arch_exact_then_os_wide() {
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("url");
        v.as_object_mut().unwrap().remove("checksum");
        v["assets"] = serde_json::json!({
            "macos":       { "url": "https://example.com/mac-universal.tar.gz", "checksum": "sha256:mac-uni" },
            "linux-x64":   { "url": "https://example.com/linux-x64.tar.gz",     "checksum": "sha256:lx64" },
            "linux-arm64": { "url": "https://example.com/linux-arm64.tar.gz",   "checksum": "sha256:larm" },
        });
        let mut v2 = v.clone();
        v2["os"] = serde_json::json!(["macos", "linux"]);
        let m: Manifest = serde_json::from_value(v2).unwrap();

        // The exact os-arch key wins where it exists.
        assert_eq!(m.asset_for(plat_arch(Os::Linux, Arch::X64)).unwrap().checksum, "sha256:lx64");
        assert_eq!(m.asset_for(plat_arch(Os::Linux, Arch::Arm64)).unwrap().checksum, "sha256:larm");

        // The arch-agnostic key answers every arch of that OS — and any arch, and none.
        assert_eq!(m.asset_for(plat_arch(Os::Macos, Arch::Arm64)).unwrap().checksum, "sha256:mac-uni");
        assert_eq!(m.asset_for(plat_arch(Os::Macos, Arch::X64)).unwrap().checksum, "sha256:mac-uni");
        assert_eq!(m.asset_for(plat(Os::Macos)).unwrap().checksum, "sha256:mac-uni");

        // An arch this build cannot name (`None`) never matches an os-arch key — only the arch-agnostic one,
        // which linux does not have here, so it is nothing rather than a guess.
        assert!(m.asset_for(plat(Os::Linux)).is_none(), "an unnameable arch matches only an <os> key");
    }

    /// Platform tokens round-trip through the wire form, and one outside the vocabulary does not parse.
    #[test]
    fn platform_tokens_are_the_os_and_os_arch_forms() {
        assert_eq!(plat(Os::Macos).token(), "macos");
        assert_eq!(plat_arch(Os::Macos, Arch::Arm64).token(), "macos-arm64");
        assert_eq!(plat_arch(Os::Linux, Arch::X64).token(), "linux-x64");

        assert_eq!(Platform::parse("windows"), Some(plat(Os::Windows)));
        assert_eq!(Platform::parse("linux-arm64"), Some(plat_arch(Os::Linux, Arch::Arm64)));
        assert_eq!(Platform::parse("macos-x64"), Some(plat_arch(Os::Macos, Arch::X64)));
        assert_eq!(Platform::parse("bsd"), None, "an unknown os does not parse");
        assert_eq!(Platform::parse("macos-riscv"), None, "an unknown arch does not parse");
        assert_eq!(Platform::parse("linux-x64-extra"), None, "a stray segment does not parse");

        // Arch vocabulary matches the self-updater's (`arm64`/`x64` from `aarch64`/`x86_64`).
        assert_eq!(Arch::Arm64.as_str(), "arm64");
        assert_eq!(Arch::X64.as_str(), "x64");
        assert_eq!(Arch::parse("arm64"), Some(Arch::Arm64));
        assert_eq!(Arch::parse("aarch64"), None, "the consts spelling is not the wire token");
    }

    /// A platform-keyed `assets` map round-trips as JSON, keys and all.
    #[test]
    fn an_os_arch_assets_map_round_trips() {
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("url");
        v.as_object_mut().unwrap().remove("checksum");
        v["os"] = serde_json::json!(["linux"]);
        v["assets"] = serde_json::json!({
            "linux-arm64": { "url": "https://example.com/linux-arm64.tar.gz", "checksum": "sha256:larm" },
            "linux-x64":   { "url": "https://example.com/linux-x64.tar.gz",   "checksum": "sha256:lx64" },
        });
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(m.assets.len(), 2);
        assert_eq!(serde_json::to_value(&m).unwrap()["assets"], v["assets"]);
    }

    /// A platform token whose arch is outside the vocabulary is refused at the shape, like an unknown OS.
    #[test]
    fn an_asset_for_an_arch_outside_the_vocabulary_does_not_parse() {
        let mut v = full_json();
        v["assets"] = serde_json::json!({
            "linux-riscv": { "url": "https://example.com/x.tar.gz", "checksum": "sha256:x" },
        });
        assert!(serde_json::from_value::<Manifest>(v).is_err());
    }

    /// An unknown OS key is refused where every other unknown token is — at the shape.
    #[test]
    fn an_asset_for_an_os_outside_the_vocabulary_does_not_parse() {
        let mut v = full_json();
        v["assets"] = serde_json::json!({
            "haiku": { "url": "https://example.com/x.tar.gz", "checksum": "sha256:x" },
        });
        assert!(serde_json::from_value::<Manifest>(v).is_err());
    }

    /// An empty map does not serialize, so a one-file manifest re-emits as its author wrote it — the same
    /// absent-equals-empty rule `config` and `events` follow.
    #[test]
    fn no_assets_map_does_not_serialize() {
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert!(m.assets.is_empty());
        assert!(serde_json::to_value(&m).unwrap().get("assets").is_none());
    }

    #[test]
    fn an_unknown_os_does_not_parse() {
        let mut v = full_json();
        v["os"] = serde_json::json!(["macos", "haiku"]);
        assert!(serde_json::from_value::<Manifest>(v).is_err(), "an OS outside the vocabulary is rejected");
    }

    #[test]
    fn config_defaults_to_an_empty_schema_when_absent() {
        // A manifest with no `config` key is a plugin that takes no settings, not a parse error.
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert!(m.config.is_empty(), "no `config` key ⇒ no configuration schema");
    }

    #[test]
    fn a_config_schema_round_trips() {
        let mut v = full_json();
        v["config"] = serde_json::json!([
            { "key": "webhook_url", "label": "Slack Webhook URL", "secret": true, "required": true },
            { "key": "events", "label": "通知するイベント" }
        ]);
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        let fields = m.fields();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].key, "webhook_url");
        assert!(fields[0].secret && fields[0].required);
        // The second field declares neither flag: both default to false.
        assert_eq!(fields[1].key, "events");
        assert!(!fields[1].secret, "an unmarked field is not a secret");
        assert!(!fields[1].required, "an unmarked field is not required");
        // Re-serializing a schema built from the parsed form yields the same document.
        assert_eq!(serde_json::to_value(&m).unwrap()["config"], serde_json::to_value(&m.config).unwrap());
    }

    /// A part written between the fields is one of them in the list and none of them to fill in
    /// (`AMB-D-727`), and it comes back out where it was written: what a part is *for* is where it sits.
    #[test]
    fn a_part_stands_between_the_fields_and_is_not_one_of_them() {
        let mut v = full_json();
        v["config"] = serde_json::json!([
            { "link": { "url": "https://myaccount.google.com/apppasswords", "label": "Create one" } },
            { "key": "smtp_password", "label": "Password", "secret": true },
            { "note": "One per mailbox." },
        ]);
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(m.config.len(), 3, "the list is what the author wrote");
        assert_eq!(
            m.fields().iter().map(|f| f.key.clone()).collect::<Vec<_>>(),
            vec!["smtp_password"],
            "a part has no key and nothing to store, so it is not a field"
        );
        assert_eq!(
            m.config[2].part().map(|p| &p.part),
            Some(&crate::plugin_show::Part::Note("One per mailbox.".into())),
        );
        // Out as it was written, in the order it was written. The field re-emits with `secret` and
        // `required` spelled out, which is what every field has always done; the parts are verbatim.
        let out = &serde_json::to_value(&m).unwrap()["config"];
        assert_eq!(out[0], v["config"][0], "the way to the page stays above the box it is for");
        assert_eq!(out[1]["key"], serde_json::json!("smtp_password"));
        assert_eq!(out[2], v["config"][2]);
    }

    /// A part carries its condition beside itself, one more key on the object it already was
    /// (`AMB-D-727`) — and a part without one is byte-for-byte what an author who never heard of the key
    /// wrote.
    #[test]
    fn a_part_carries_when_beside_what_it_draws() {
        let mut v = full_json();
        v["config"] = serde_json::json!([
            { "key": "transport", "label": "経路" },
            { "note": "Worker を先に立ててください", "when": [{ "field": "transport", "has": "cloudflare" }] },
            { "text": "どの経路でも読めます" },
        ]);
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        let conditioned = m.config[1].part().expect("a part");
        assert_eq!(conditioned.part, crate::plugin_show::Part::Note("Worker を先に立ててください".into()));
        assert_eq!(conditioned.when, vec![crate::plugin_when::When::field_has("transport", "cloudflare")]);
        assert_eq!(m.config[1].when(), conditioned.when, "one accessor answers for either kind of entry");
        assert!(m.config[2].when().is_empty(), "a part written without one is unconditional");
        assert!(m.config[0].when().is_empty());

        let out = &serde_json::to_value(&m).unwrap()["config"];
        assert_eq!(out[1], v["config"][1], "the condition rides back out beside the part");
        assert_eq!(out[2], v["config"][2], "an empty condition does not appear at all");
    }

    /// The one thing an untagged list has to get right: telling the two apart with nothing spelled twice.
    /// A setting is an object with a `key` and a `label`; a part is an object named for what it draws.
    #[test]
    fn a_field_and_a_part_are_told_apart_by_what_they_are() {
        let read = |one: serde_json::Value| {
            let mut v = full_json();
            v["config"] = serde_json::json!([one]);
            serde_json::from_value::<Manifest>(v)
        };
        assert!(read(serde_json::json!({ "key": "k", "label": "L" })).unwrap().config[0]
            .field()
            .is_some());
        assert!(read(serde_json::json!({ "text": "Read this" })).unwrap().config[0].part().is_some());
        assert!(
            read(serde_json::json!({ "key": "k" })).is_err(),
            "a setting with no label is neither: a manifest that means one is not quietly read as the other"
        );
    }

    /// A field that declares no kind is a text field, and re-emits without the key — the same
    /// absent-equals-default rule the rest of the manifest keeps (`AMB-D-415`).
    #[test]
    fn a_field_with_no_kind_is_a_text_field_and_stays_one_document() {
        let mut v = full_json();
        v["config"] = serde_json::json!([{ "key": "base", "label": "Base branch" }]);
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        let field = &m.fields()[0];
        assert_eq!(*field, ConfigField::new("base", "Base branch"));
        assert_eq!(field.field_type, FieldType::Text, "no `type` ⇒ a line the user types");
        assert!(field.options.is_empty());
        assert!(field.default.is_none());
        let out = &serde_json::to_value(&m).unwrap()["config"][0];
        for absent in ["type", "options", "default"] {
            assert_eq!(out.get(absent), None, "`{absent}` was not written, so it is not carried back out");
        }
    }

    /// The candidates and the default round-trip as written (`AMB-D-415`) — an author's schema is handed
    /// back to them verbatim, `type` included.
    #[test]
    fn a_multi_field_round_trips_with_its_options_and_default() {
        let mut v = full_json();
        v["config"] = serde_json::json!([{
            "key": "events",
            "label": "通知するイベント",
            "type": "multi",
            "options": [
                { "value": "task.done", "label": "完了した" },
                { "value": "task.rejected", "label": "見送った" },
            ],
            "default": "task.done",
        }]);
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        let field = &m.fields()[0];
        assert_eq!(field.field_type, FieldType::Multi);
        assert_eq!(field.options.len(), 2);
        assert_eq!(field.options[0].value, "task.done");
        assert_eq!(field.options[0].label, "完了した");
        assert_eq!(field.default.as_deref(), Some("task.done"));
        let out = &serde_json::to_value(&m).unwrap()["config"][0];
        for declared in ["type", "options", "default"] {
            assert_eq!(out[declared], v["config"][0][declared], "`{declared}` comes back as written");
        }
    }

    /// The supporting text and the readonly flag round-trip as written (`AMB-D-656`), and a schema that
    /// declares none of them re-emits without the keys — the absent-equals-default rule the keys added
    /// since `AMB-D-415` keep.
    #[test]
    fn the_supporting_text_and_the_readonly_flag_round_trip() {
        let mut v = full_json();
        v["config"] = serde_json::json!([
            {
                "key": "webhook_url",
                "label": "Webhook URL",
                "help": "Incoming Webhooks で作る。\n\nチャンネルごとに1本。",
                "placeholder": "https://hooks.example.com/T000/B000",
            },
            { "key": "worker_url", "label": "Worker URL", "readonly": true },
            { "key": "base", "label": "Base branch" },
        ]);
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        let fields = m.fields();
        assert_eq!(fields[0].help.as_deref(), Some("Incoming Webhooks で作る。\n\nチャンネルごとに1本。"));
        assert_eq!(fields[0].placeholder.as_deref(), Some("https://hooks.example.com/T000/B000"));
        assert!(!fields[0].readonly, "an unmarked field is the user's to fill in");
        assert!(fields[1].readonly);
        assert!(fields[1].help.is_none() && fields[1].placeholder.is_none());

        let out = &serde_json::to_value(&m).unwrap()["config"];
        for declared in ["help", "placeholder"] {
            assert_eq!(out[0][declared], v["config"][0][declared], "`{declared}` comes back as written");
        }
        assert_eq!(out[1]["readonly"], serde_json::json!(true));
        for absent in ["help", "placeholder", "readonly"] {
            assert_eq!(out[2].get(absent), None, "`{absent}` was not written, so it is not carried back out");
        }
    }

    /// A kind outside the vocabulary is refused at the shape, where every other unknown token is.
    #[test]
    fn a_field_type_outside_the_vocabulary_does_not_parse() {
        let mut v = full_json();
        v["config"] = serde_json::json!([{ "key": "k", "label": "L", "type": "number" }]);
        assert!(serde_json::from_value::<Manifest>(v).is_err());
    }

    /// A candidate is both halves — what is stored and what is read — so neither may be left out.
    #[test]
    fn a_config_option_missing_value_or_label_does_not_parse() {
        for half in ["value", "label"] {
            let mut option = serde_json::json!({ "value": "task.done", "label": "完了した" });
            option.as_object_mut().unwrap().remove(half);
            let mut v = full_json();
            v["config"] =
                serde_json::json!([{ "key": "k", "label": "L", "type": "multi", "options": [option] }]);
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "an option missing `{half}` must not parse"
            );
        }
    }

    #[test]
    fn a_config_field_missing_key_or_label_does_not_parse() {
        // key and label are the required half of a field's shape (secret/required default).
        for field in ["key", "label"] {
            let full = serde_json::json!({ "key": "k", "label": "L", "secret": true, "required": true });
            let mut one = full.clone();
            one.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ConfigField>(one).is_err(),
                "a config field missing `{field}` must not parse"
            );
        }
    }

    #[test]
    fn events_default_to_no_subscription_when_absent_and_round_trip() {
        // A manifest with no `events` key subscribes to nothing — a command-only plugin, not a parse error.
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert!(m.events.is_empty(), "no `events` key ⇒ no subscription");
        // An empty list does not re-serialize, mirroring `config` (absent equals empty).
        assert!(serde_json::to_value(&m).unwrap().get("events").is_none());

        // A bare-string subscription round-trips verbatim, and means the notification default.
        let mut v = full_json();
        v["events"] = serde_json::json!(["task.created", "comment.added"]);
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(
            m.events,
            vec![EventSubscription::new("task.created"), EventSubscription::new("comment.added")]
        );
        assert_eq!(m.events[0].faces, vec![Face::Cli, Face::Gui], "a bare string fires on both faces");
        assert!(!m.events[0].reply, "a bare string replies to no one");
        assert_eq!(
            serde_json::to_value(&m).unwrap()["events"],
            serde_json::json!(["task.created", "comment.added"]),
            "a bare string re-emits as a bare string, not an object"
        );
    }

    /// The object form (`AMB-D-383`): an author narrows the faces or asks for a reply, and it round-trips as
    /// the object it is — while a bare string beside it stays bare.
    #[test]
    fn a_subscription_object_declares_faces_and_reply_and_round_trips() {
        let mut v = full_json();
        v["events"] = serde_json::json!([
            "task.done",
            { "event": "task.status_changed", "faces": ["cli"], "reply": true },
        ]);
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.events.len(), 2);
        // The bare string keeps the notification default.
        assert_eq!(m.events[0], EventSubscription::new("task.done"));
        // The object carries exactly what it declared.
        assert_eq!(m.events[1].event, "task.status_changed");
        assert_eq!(m.events[1].faces, vec![Face::Cli]);
        assert!(m.events[1].reply, "the worktree advice hook asks for a reply");
        // Re-serializing: the bare one stays bare, the object stays an object.
        assert_eq!(
            serde_json::to_value(&m).unwrap()["events"],
            serde_json::json!([
                "task.done",
                { "event": "task.status_changed", "faces": ["cli"], "reply": true },
            ])
        );
    }

    /// An object that omits `faces` / `reply` is the notification default — the same meaning as the bare
    /// string, so it re-emits as a bare string.
    #[test]
    fn a_subscription_object_with_no_overrides_equals_the_bare_string() {
        let mut v = full_json();
        v["events"] = serde_json::json!([{ "event": "task.created" }]);
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.events, vec![EventSubscription::new("task.created")]);
        assert_eq!(
            serde_json::to_value(&m).unwrap()["events"],
            serde_json::json!(["task.created"]),
            "an object with no overrides collapses back to the bare string it equals"
        );
    }

    /// A face token outside the vocabulary is refused at the shape, the same way an unknown `os`
    /// is — the validator never sees it.
    #[test]
    fn a_face_outside_the_vocabulary_does_not_parse() {
        let mut v = full_json();
        v["events"] = serde_json::json!([{ "event": "task.done", "faces": ["server"] }]);
        assert!(
            serde_json::from_value::<Manifest>(v).is_err(),
            "a face outside {{cli, gui}} must not parse"
        );
    }

    /// A subscription object missing its `event` is refused: it matches neither the string form nor an
    /// object with an event.
    #[test]
    fn a_subscription_object_missing_event_does_not_parse() {
        let mut v = full_json();
        v["events"] = serde_json::json!([{ "faces": ["cli"], "reply": true }]);
        assert!(serde_json::from_value::<Manifest>(v).is_err(), "a subscription needs an event name");
    }

    #[test]
    fn face_tokens_are_lowercase_and_stable() {
        assert_eq!(Face::Cli.as_str(), "cli");
        assert_eq!(Face::Gui.as_str(), "gui");
        assert_eq!(serde_json::to_value(Face::Cli).unwrap(), serde_json::json!("cli"));
        assert_eq!(serde_json::from_value::<Face>(serde_json::json!("gui")).unwrap(), Face::Gui);
    }

    /// The block is optional, and its absence is what every manifest written before it says (`AMB-D-437`):
    /// no key on the way in, no key on the way out.
    #[test]
    fn no_agent_block_is_a_plugin_that_says_nothing_at_the_entry_point() {
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert!(m.agent.is_none(), "no `agent` key ⇒ nothing to relay");
        assert!(serde_json::to_value(&m).unwrap().get("agent").is_none());
    }

    #[test]
    fn an_agent_block_round_trips() {
        let mut v = full_json();
        v["agent"] = serde_json::json!({
            "when": "タスクに着手して、コミットを産む作業を隔離したいとき",
            "commands": [
                { "cmd": "start <task-id>", "does": "リポ外に worktree を切り、cd 行を返す" },
                { "cmd": "finish <task-id>", "does": "worktree を畳む" },
            ],
        });
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        let agent = m.agent.as_ref().expect("the block parsed");
        assert_eq!(agent.commands.len(), 2);
        assert_eq!(agent.commands[0].cmd, "start <task-id>", "the author's own face, with no prefix");
        assert_eq!(serde_json::to_value(&m).unwrap()["agent"], v["agent"]);
    }

    /// A plugin whose whole surface is observation hooks names its occasion and stops there — and an
    /// empty command list re-emits as no key at all, the same absent-equals-empty rule `config` follows.
    #[test]
    fn an_agent_block_may_name_no_commands() {
        let mut v = full_json();
        v["agent"] = serde_json::json!({ "when": "何もしない — 見ているだけ" });
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert!(m.agent.as_ref().unwrap().commands.is_empty());
        assert_eq!(
            serde_json::to_value(&m).unwrap()["agent"],
            serde_json::json!({ "when": "何もしない — 見ているだけ" })
        );
    }

    #[test]
    fn an_agent_block_missing_a_required_line_does_not_parse() {
        // The shape half of the door: `when` is the block, and a command is both halves of a call.
        let mut no_when = full_json();
        no_when["agent"] = serde_json::json!({ "commands": [{ "cmd": "start", "does": "cuts one" }] });
        assert!(serde_json::from_value::<Manifest>(no_when).is_err(), "a block must name its occasion");

        for field in ["cmd", "does"] {
            let mut v = full_json();
            let mut command = serde_json::json!({ "cmd": "start", "does": "cuts one" });
            command.as_object_mut().unwrap().remove(field);
            v["agent"] = serde_json::json!({ "when": "w", "commands": [command] });
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "a command missing `{field}` must not parse"
            );
        }
    }

    #[test]
    fn no_settings_block_is_a_form_that_calls_nothing() {
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert!(m.settings.is_none(), "no `settings` key ⇒ no call raised from the form");
        assert!(serde_json::to_value(&m).unwrap().get("settings").is_none());
    }

    #[test]
    fn a_settings_block_round_trips() {
        let mut v = full_json();
        v["settings"] = serde_json::json!({
            "check": "config check",
            "actions": [
                {
                    "cmd": "config test",
                    "label": "テスト送信",
                    "ask": [{ "key": "api_token", "label": "API トークン", "secret": true }],
                },
                { "cmd": "setup", "label": "セットアップ" },
            ],
        });
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        let settings = m.settings.as_ref().expect("the block parsed");
        assert_eq!(settings.check.as_deref(), Some("config check"));
        assert_eq!(settings.actions[0].cmd, "config test", "the author's own face, with no prefix");
        assert_eq!(settings.actions[0].ask[0].key, "api_token");
        assert!(settings.actions[1].ask.is_empty(), "a press may need nothing beyond what is saved");
        assert_eq!(serde_json::to_value(&m).unwrap()["settings"], v["settings"]);
    }

    /// Each half of the block stands on its own, and an omitted half re-emits as no key at all — the same
    /// absent-equals-empty rule `config` and `events` follow.
    #[test]
    fn a_settings_block_may_declare_a_check_alone_or_an_action_alone() {
        for written in [
            serde_json::json!({ "check": "config check" }),
            serde_json::json!({ "actions": [{ "cmd": "setup", "label": "Set up" }] }),
        ] {
            let mut v = full_json();
            v["settings"] = written.clone();
            let m: Manifest = serde_json::from_value(v).unwrap();
            assert_eq!(serde_json::to_value(&m).unwrap()["settings"], written);
        }
    }

    /// An asked value is not hidden unless its author says so, and a field that says nothing re-emits
    /// without the key — the rule every flag added since `AMB-D-415` keeps.
    #[test]
    fn an_asked_value_is_visible_unless_declared_secret() {
        let mut v = full_json();
        v["settings"] =
            serde_json::json!({ "actions": [{ "cmd": "setup", "label": "Set up", "ask": [
                { "key": "code", "label": "One-time code" },
            ] }] });
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert!(!m.settings.as_ref().unwrap().actions[0].ask[0].secret);
        let out = serde_json::to_value(&m).unwrap();
        assert!(out["settings"]["actions"][0]["ask"][0].get("secret").is_none());
    }

    /// The two keys an author carries over from a config field are kept rather than dropped, so the
    /// validator can name them back (`AMB-D-664`) — while a key from a later Amenbo is ignored, as
    /// everywhere else in this module.
    #[test]
    fn an_ask_keeps_the_keys_it_does_not_have() {
        let mut v = full_json();
        v["settings"] = serde_json::json!({ "actions": [{ "cmd": "setup", "label": "Set up", "ask": [
            { "key": "code", "label": "One-time code", "required": true, "default": "1234" },
        ] }] });
        let m: Manifest = serde_json::from_value(v).unwrap();
        let ask = &m.settings.as_ref().unwrap().actions[0].ask[0];
        assert_eq!(ask.extra.keys().collect::<Vec<_>>(), ["default", "required"]);
    }

    #[test]
    fn a_settings_block_missing_a_required_field_does_not_parse() {
        // The shape half of the door: an operation is a call and the words on its button, and an asked
        // value is a name and the words beside its box.
        for field in ["cmd", "label"] {
            let mut action = serde_json::json!({ "cmd": "setup", "label": "Set up" });
            action.as_object_mut().unwrap().remove(field);
            let mut v = full_json();
            v["settings"] = serde_json::json!({ "actions": [action] });
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "an action missing `{field}` must not parse"
            );
        }
        for field in ["key", "label"] {
            let mut ask = serde_json::json!({ "key": "code", "label": "One-time code" });
            ask.as_object_mut().unwrap().remove(field);
            let mut v = full_json();
            v["settings"] =
                serde_json::json!({ "actions": [{ "cmd": "setup", "label": "S", "ask": [ask] }] });
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "an asked value missing `{field}` must not parse"
            );
        }
    }

    #[test]
    fn an_unknown_key_is_ignored_for_forward_compatibility() {
        // A field a newer Amenbo added must not make an older one refuse the manifest.
        let mut v = full_json();
        v["some_future_field"] = serde_json::json!("whatever a later version wrote");
        let m: Manifest = serde_json::from_value(v).expect("unknown keys are tolerated");
        assert_eq!(m.name, "worktree");
    }

    #[test]
    fn the_compat_declaration_defaults_when_absent() {
        // A manifest written before the compat fields existed still parses: it targets the v1 payload
        // baseline and declares no amenbo-version floor. The default must be the fixed baseline, not the
        // reading Amenbo's current `v` — an old plugin does not silently start claiming a newer contract.
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("payload_v");
        v.as_object_mut().unwrap().remove("min_amenbo");
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.payload_v, 1, "an omitted payload_v is the v1 baseline");
        assert!(m.min_amenbo.is_none(), "an omitted min_amenbo is no version floor");
    }

    #[test]
    fn an_absent_min_amenbo_does_not_serialize() {
        // Absent and present-but-none stay one document: a plugin with no version floor re-emits without
        // the key, mirroring how an empty config schema does not serialize.
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("min_amenbo");
        let m: Manifest = serde_json::from_value(v).unwrap();
        let out = serde_json::to_value(&m).unwrap();
        assert!(out.get("min_amenbo").is_none(), "no floor ⇒ no min_amenbo key on the way out");
    }

    /// An overlay reads as the layer it is: the fields the author translated, keyed the way a
    /// translation has to be keyed, and nothing said about the ones they did not (`AMB-D-621`).
    #[test]
    fn an_overlay_carries_what_was_translated_and_says_nothing_of_the_rest() {
        let o: ManifestOverlay = serde_json::from_value(serde_json::json!({
            "desc": "タスクごとに git worktree を切り分ける",
            "config": {
                "events": { "label": "何を報告するか", "options": { "task.done": "タスクが完了した" } },
                "base": {
                    "label": "基点にするブランチ",
                    "help": "書かなければ、いま居るブランチから切る。",
                    "placeholder": "main",
                },
            },
            "settings": {
                "actions": {
                    "config test": { "label": "テスト送信", "ask": { "api_token": "API トークン" } },
                },
            },
        }))
        .unwrap();

        assert_eq!(o.desc.as_deref(), Some("タスクごとに git worktree を切り分ける"));
        assert_eq!(o.config["base"].label.as_deref(), Some("基点にするブランチ"));
        assert_eq!(o.config["base"].help.as_deref(), Some("書かなければ、いま居るブランチから切る。"));
        assert_eq!(o.config["base"].placeholder.as_deref(), Some("main"));
        assert_eq!(o.config["events"].options["task.done"], "タスクが完了した");
        assert!(o.config["base"].options.is_empty(), "a field with no candidates translates none");
        assert!(
            o.config["events"].help.is_none() && o.config["events"].placeholder.is_none(),
            "a field whose supporting text was left in the base translates none of it",
        );
        let settings = o.settings.as_ref().expect("the buttons on that form are read there too");
        let action = &settings.actions["config test"];
        assert_eq!(action.label.as_deref(), Some("テスト送信"), "keyed by the call it raises, never by position");
        assert_eq!(action.ask["api_token"], "API トークン", "and the boxes by the name they are handed over under");
        assert!(o.extra.is_empty(), "everything it wrote is something Amenbo translates");

        // What was not translated re-emits as absent, never as an empty string standing in for a line
        // the author never wrote.
        let bare: ManifestOverlay = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(serde_json::to_value(&bare).unwrap(), serde_json::json!({}));
    }

    /// **A key Amenbo does not translate is kept, not dropped.** The overlay still parses — so one run
    /// can report every mistake in it — and the key is there for the validator to name back to its
    /// author (`AMB-D-621`), which is the whole difference from a manifest's ignored unknowns.
    #[test]
    fn an_overlay_keeps_the_keys_amenbo_does_not_translate() {
        let o: ManifestOverlay = serde_json::from_value(serde_json::json!({
            "desc": "説明",
            "author": "書いた人",
            "config": { "base": { "label": "ラベル", "default": "main" } },
            "settings": { "check": "検査", "actions": { "config test": { "label": "試す", "cmd": "設定 テスト" } } },
        }))
        .unwrap();

        assert_eq!(o.extra.keys().collect::<Vec<_>>(), ["author"]);
        assert_eq!(o.config["base"].extra.keys().collect::<Vec<_>>(), ["default"]);
        let settings = o.settings.as_ref().unwrap();
        assert_eq!(settings.extra.keys().collect::<Vec<_>>(), ["check"], "a call is not shown to anyone");
        assert_eq!(settings.actions["config test"].extra.keys().collect::<Vec<_>>(), ["cmd"]);
    }

    /// The overlay's file name is what says which language it is, and which manifest it is beside
    /// (`AMB-D-621`).
    #[test]
    fn an_overlay_file_names_the_language_it_translates_into() {
        assert_eq!(overlay_language("mail.yaml", "mail.ja.yaml"), Some("ja"));
        assert_eq!(overlay_language("mail.yaml", "mail.zh-Hans.yaml"), Some("zh-Hans"));
        assert_eq!(overlay_language("mail.json", "mail.pt-BR.json"), Some("pt-BR"));
        // A code Amenbo does not read is still read off the name — the validator is what says so.
        assert_eq!(overlay_language("mail.yaml", "mail.xx.yaml"), Some("xx"));

        // The manifest is not an overlay of itself, and neither is another plugin's anything.
        assert_eq!(overlay_language("mail.yaml", "mail.yaml"), None);
        assert_eq!(overlay_language("mail.yaml", "mailbox.ja.yaml"), None);
        assert_eq!(overlay_language("mail.yaml", "worktree.ja.yaml"), None);
        // The form the manifest was written in is the form its translations are written in.
        assert_eq!(overlay_language("mail.yaml", "mail.ja.json"), None);
        assert_eq!(overlay_language("mail.yaml", "mail.ja.md"), None);
    }
}
