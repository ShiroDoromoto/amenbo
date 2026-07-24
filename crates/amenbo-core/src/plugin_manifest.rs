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
//! the amenbo team, decided by catalog curation (the PR review / the manifest's directory), never by a
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
//! declaration a manifest will carry (`AMB-D-359` — target payload `v` and min amenbo version), which
//! gates a plugin gracefully instead of failing to parse a manifest a newer amenbo wrote. Denying unknown
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
    /// The OS this build is running on, as a manifest spells it. `None` on a platform amenbo's
    /// vocabulary cannot name — nothing amenbo ships runs there, so nothing declares an asset for it
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arch {
    Arm64,
    X64,
}

impl Arch {
    /// The arch this build runs on, or `None` for one amenbo's vocabulary cannot name — the same honest
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

    /// Normalize a `std::env::consts::ARCH` value onto the wire token — the same mapping the self-updater's
    /// `current_platform_key` applies, kept here so the two never drift.
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
    /// The platform this build runs on, or `None` when amenbo's vocabulary cannot name its OS (an arch it
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
    /// whose OS or arch is outside the vocabulary — the same fail-to-parse an unknown `os` or `scope` gets.
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

/// **What one switch turns this plugin on** (`AMB-D-379`) — declared by the author, because only the
/// author knows which one is meaningful for their plugin.
///
/// A user is never shown two enable switches for the same plugin. A notifier is answered per project ("do
/// I want this here"), while a plugin that watches the whole device has nothing a project could usefully
/// say about it — so amenbo asks for one answer, at the level this field names, and refuses the other. The
/// per-project *differences* a plugin needs beyond on/off are its **settings**, which have their own tiers
/// (`AMB-D-356`): "notify at all" is one switch, "to which channel" is a value a project may override.
///
/// Not to be confused with [`plugin_config::Scope`](crate::plugin_config::Scope), which names the tier one
/// config *value* is written at. This names what the *gate* is per.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Enabled per project — the default, and what most plugins want. A project that has not enabled it
    /// does not run it, and there is no device-wide answer to inherit.
    #[default]
    Project,
    /// Enabled once for the device. A project cannot override it: for a plugin whose work is not a
    /// project's (it watches the machine, or the store as a whole), a per-project answer would be a switch
    /// that looks like it does something and does not.
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
/// refuses a face token outside the vocabulary, the same way an unknown `scope` fails to parse. The
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
/// the plugin was written against the original contract, so it must not drift upward as amenbo bumps its
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
    /// A one-line description, for the list view.
    pub desc: String,
    /// Who wrote the plugin. For an official plugin this is the amenbo team; it is display text, not the
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
    /// amenbo **catalog key** when the manifest is aggregated (`AMB-D-371`). Verified once on download
    /// against amenbo's embedded catalog public key ([`crate::plugin_provenance`]) — the origin half of
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
    /// this type only carries the map, and a platform token it does not know fails to parse the same way an
    /// unknown `scope` does.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<Platform, Asset>,
    /// The official badge: the author is the amenbo team. Catalog-authoritative (`AMB-D-347`), never
    /// self-declared — absent means `false`.
    #[serde(default)]
    pub official: bool,
    /// Which switch enables this plugin — per project, or once for the device ([`Scope`], `AMB-D-379`).
    /// Absent means [`Scope::Project`], the answer that fits most plugins and the safe one: a project that
    /// has said nothing runs nothing. Declaring `machine` is the author saying a per-project answer would
    /// be meaningless for their plugin, and the faces then offer only the device-wide switch.
    ///
    /// A value outside the two is refused where every other shape error is (`AMB-D-354`): the manifest does
    /// not parse, so it never reaches the rules or the catalog.
    #[serde(default)]
    pub scope: Scope,
    /// The event-payload contract version this plugin reads (`AMB-D-349` — a single integer `v` for the
    /// whole contract, evolving additively). It lets amenbo notice when its own `v` has moved past what a
    /// plugin understands and warn or refuse rather than silently feed it a payload it cannot parse
    /// (`AMB-D-359`). Absent means the v1 baseline — a manifest written before this field targets the
    /// original contract, not whatever version the reading amenbo happens to be at. This module only
    /// *carries* the number; the enable/run-time comparison is [`crate::plugin_compat`]'s, not the type's.
    #[serde(default = "default_payload_v")]
    pub payload_v: u32,
    /// The minimum amenbo version this plugin needs, as a semver string — below it, amenbo warns or
    /// refuses to enable/run the plugin (`AMB-D-359`). Absent means no floor: the plugin declares no
    /// version requirement. Stored opaquely, like `checksum` — this module neither parses nor compares
    /// it; reading it is [`crate::plugin_compat`]'s, so the one truth about version ordering lives with
    /// the gate that acts on it (a string it cannot parse is a floor amenbo will not claim to meet).
    /// A value that reads as no version at all is refused earlier, at the manifest door
    /// ([`crate::plugin_validate`]), so it does not reach that gate through a fresh install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_amenbo: Option<String>,
    /// The plugin's configuration schema: a flat list of fields the author declares so amenbo can
    /// render a form, store the values, and inject them at run time (`AMB-D-356`). Absent means the
    /// plugin takes no configuration — the safe default is an empty schema, so an older manifest with
    /// no `config` key is a plugin with no settings, not a parse error.
    ///
    /// The list is **the whole schema**: no types, no validation rules. amenbo does not judge what a
    /// value means (a URL, an email) — that is the author's job at run time. What amenbo reads here is
    /// only which fields exist, which are secret (so the store never sees them — `AMB-D-356`), and
    /// which are required (so `enable` is blocked until they are filled — `AMB-D-351`).
    ///
    /// An empty schema does not serialize (`skip_serializing_if`), so a re-emitted manifest for a plugin
    /// with no settings is byte-for-byte what an author who omitted `config` wrote — the absent and the
    /// empty forms stay the same document.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<ConfigField>,
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
}

impl Manifest {
    /// **The distributable this manifest offers for one OS** (`AMB-D-381`) — the [`assets`](Manifest::assets)
    /// entry when the manifest is the per-OS kind, the single `url`/`checksum`/`signature` when it is the
    /// one-file kind. Every layer that fetches, verifies or compares an asset goes through here, so the
    /// two forms are resolved in exactly one place.
    ///
    /// Resolution is **exact then OS-wide** (`AMB-D-384`): the running platform's `<os>-<arch>` key is
    /// tried first, then its arch-agnostic `<os>` key. A machine whose arch amenbo cannot name
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

/// One field of a plugin's configuration schema (`AMB-D-356`). The author declares a flat list of these
/// in the manifest; amenbo renders each as one form field, routes its value to storage by the `secret`
/// flag, and injects it into the plugin at run time. **amenbo carries no notion of the field's type or
/// meaning** — there is no `type`, no pattern, no validation rule here. The only semantics amenbo acts on
/// are `secret` (where the value is stored and how it is injected) and `required` (whether an empty value
/// blocks `enable`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigField {
    /// The field's stable key — its identity in storage and the name it is injected under (the env var
    /// for a secret, the JSON key on stdin for the rest). Not shown to the user; `label` is.
    pub key: String,
    /// The human-readable label the form shows beside the field. Display text only.
    pub label: String,
    /// Whether the value is a secret. **The author declares this; amenbo does not judge it** (`AMB-D-356`)
    /// — amenbo cannot know a webhook URL is sensitive, so it trusts the flag. A secret is stored in the
    /// user-area secret file (never the store, never a backup) and injected as an environment variable
    /// (off argv, off logs); a non-secret is stored in the ordinary two tiers and injected on stdin.
    /// Absent means `false` — the safe-for-storage default is *not* secret only for a field the author
    /// left unmarked, which is a plain-text field by construction.
    #[serde(default)]
    pub secret: bool,
    /// Whether the field must hold a value before the plugin may be enabled (`AMB-D-351`, fail-closed).
    /// amenbo only checks presence (a non-empty value); it does not check the value is *valid*. Absent
    /// means `false`.
    #[serde(default)]
    pub required: bool,
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

    /// The enable scope is the author's declaration (`AMB-D-379`): absent means per project — the safe
    /// answer, since a project that has said nothing then runs nothing — and a value outside the two is a
    /// manifest that does not parse, which is where every other shape error is caught.
    #[test]
    fn the_enable_scope_defaults_to_project_and_rejects_anything_else() {
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("scope");
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.scope, Scope::Project, "an undeclared scope is per project");

        let mut machine = full_json();
        machine["scope"] = serde_json::json!("machine");
        let m: Manifest = serde_json::from_value(machine).unwrap();
        assert_eq!(m.scope, Scope::Machine);
        assert_eq!(serde_json::to_value(&m).unwrap()["scope"], serde_json::json!("machine"));

        for bad in ["global", "workspace", "Project", ""] {
            let mut v = full_json();
            v["scope"] = serde_json::json!(bad);
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "a scope outside the vocabulary must not parse: {bad}"
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
    /// a machine whose arch amenbo cannot name matches only the arch-agnostic key.
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
        assert_eq!(m.config.len(), 2);
        assert_eq!(m.config[0].key, "webhook_url");
        assert!(m.config[0].secret && m.config[0].required);
        // The second field declares neither flag: both default to false.
        assert_eq!(m.config[1].key, "events");
        assert!(!m.config[1].secret, "an unmarked field is not a secret");
        assert!(!m.config[1].required, "an unmarked field is not required");
        // Re-serializing a schema built from the parsed form yields the same document.
        assert_eq!(serde_json::to_value(&m).unwrap()["config"], serde_json::to_value(&m.config).unwrap());
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

    /// A face token outside the vocabulary is refused at the shape, the same way an unknown `scope` or `os`
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

    #[test]
    fn an_unknown_key_is_ignored_for_forward_compatibility() {
        // A field a newer amenbo added must not make an older one refuse the manifest.
        let mut v = full_json();
        v["some_future_field"] = serde_json::json!("whatever a later version wrote");
        let m: Manifest = serde_json::from_value(v).expect("unknown keys are tolerated");
        assert_eq!(m.name, "worktree");
    }

    #[test]
    fn the_compat_declaration_defaults_when_absent() {
        // A manifest written before the compat fields existed still parses: it targets the v1 payload
        // baseline and declares no amenbo-version floor. The default must be the fixed baseline, not the
        // reading amenbo's current `v` — an old plugin does not silently start claiming a newer contract.
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
}
