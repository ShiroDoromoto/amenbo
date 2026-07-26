//! The **read-back path** — what a plugin needs in its environment to call `amenbo` back (`AMB-D-406`).
//!
//! A payload carries an id and a kind, and nothing of the record itself (`AMB-D-348`). Without a way to
//! read, that id names nothing a plugin can act on. The way is the one every author already has: **run
//! `amenbo <read command> --json`**. There is no second protocol, and no language binding — a plugin is any
//! executable (`AMB-D-346`), so the only route that works in every language is the binary itself.
//!
//! What a child process cannot work out on its own is *which* store and *how far*, so amenbo hands it both
//! when it launches it, the same way git hands a hook `GIT_DIR`:
//!
//! | variable | what it says |
//! |---|---|
//! | [`STORE_ENV`] (`AMENBO_HOME`) | the store to open — the base directory the run's own store sits in |
//! | [`REACH_ENV`] (`AMENBO_PLUGIN_REACH`) | how far it may read — [`all`](ALL_REACH), or a project's ref |
//!
//! **The store is named, never resolved.** A plugin's working directory is whatever its launcher happened
//! to be in, so the `.amenbo` walk would answer about a folder nobody consulted — and a runner works the
//! store its parent drove, not whichever one its own environment would find. Naming it is the same choice
//! [`SelfRunner`](crate::plugin_runner::SelfRunner) makes one level up.
//!
//! **The reach is the gate, read back.** A plugin fires through exactly one gate (`AMB-D-379`), and that
//! gate is also the window: a `machine` plugin observes the device, a `project` plugin observes one
//! project — so what it may read is what it may observe, and [`reach_of`] is that identity spelled out.
//! Both faces resolve the gate before anything runs ([`plugin_trust::gate_for`](crate::plugin_trust::gate_for)),
//! so neither has to decide this twice.
//!
//! **This is not isolation** (`AMB-D-406`). A plugin has a shell: it can rewrite these variables, and it can
//! open the store file directly. The trust boundary is the explicit enable (`AMB-D-351`), not a sandbox —
//! what this door is for is that an author who needs the content has a documented way in, instead of being
//! pushed into reading the store behind amenbo's back.

use std::path::Path;

use crate::error::{Error, Result};
use crate::idref::{self, RefKind};
use crate::plugin_trust::Gate;
use crate::reach::Reach;

/// The variable naming the store a plugin reads back from. It is amenbo's own `AMENBO_HOME` — the root that
/// already means "the user layer lives here" — rather than a second spelling of the same fact, so a plugin's
/// `amenbo` call resolves its paths exactly as the process that launched it did.
pub const STORE_ENV: &str = crate::env::HOME_VAR;

/// The variable declaring how far a plugin may read: [`ALL_REACH`], or a project's `AMB-P-<n>` ref.
pub const REACH_ENV: &str = crate::env::PLUGIN_REACH_VAR;

/// The value of [`REACH_ENV`] that means the whole device — what a `scope: machine` plugin is handed. Spelled
/// out rather than left as the empty value, so "amenbo said everything" and "nobody set this" stay apart.
pub const ALL_REACH: &str = "all";

/// The window a plugin reads through, from the gate it fires through (`AMB-D-406`). The device's gate is the
/// device's window; one project's gate is that project's.
pub fn reach_of(gate: Gate) -> Reach {
    match gate {
        Gate::Machine => Reach::All,
        Gate::Project(id) => Reach::Project(id),
    }
}

/// The variables to set on a plugin's process: the store at `base_dir`, and `reach` as its window. In the
/// `(name, value)` shape [`PluginInvocation::env`](crate::plugin_exec::PluginInvocation::env) takes, so a
/// caller sets each verbatim beside the config injection (`AMB-D-356`) it already builds.
pub fn env(base_dir: &Path, reach: Reach) -> Vec<(String, String)> {
    vec![
        (STORE_ENV.to_string(), base_dir.to_string_lossy().into_owned()),
        (REACH_ENV.to_string(), encode_reach(reach).to_string()),
    ]
}

/// How a reach is written into [`REACH_ENV`]: the whole device as [`ALL_REACH`], one project as its
/// `AMB-P-<n>` ref — the same spelling every other surface shows a project by, so an author who prints the
/// variable reads a ref they can look up.
pub fn encode_reach(reach: Reach) -> std::borrow::Cow<'static, str> {
    match reach {
        Reach::All => std::borrow::Cow::Borrowed(ALL_REACH),
        Reach::Project(id) => std::borrow::Cow::Owned(idref::project(id)),
    }
}

/// Read [`REACH_ENV`] back — the other half of [`encode_reach`], and the CLI's door into the plugin face.
///
/// A bare number is accepted beside the `AMB-P-<n>` ref, on the same "reading is looser than writing" footing
/// as every other ref amenbo takes. A value that is neither is an **error**: the variable is set only by
/// amenbo launching a plugin, so a value it cannot read means the window was declared and lost — falling back
/// to a default would open one the launcher never named.
pub fn decode_reach(value: &str) -> Result<Reach> {
    let value = value.trim();
    if value.eq_ignore_ascii_case(ALL_REACH) {
        return Ok(Reach::All);
    }
    idref::strip(RefKind::Project, value)
        .parse::<i64>()
        .map(Reach::Project)
        .map_err(|_| unreadable(value))
}

/// This process's plugin window, if it is running as one: [`REACH_ENV`] decoded. `None` is the ordinary case
/// — nothing launched this as a plugin, so the facet and the binding decide the reach as they always have.
pub fn reach_from_env() -> Result<Option<Reach>> {
    match crate::env::plugin_reach() {
        // Set-but-empty reads as unset: an exported-and-cleared variable is a shell's way of saying nothing.
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => decode_reach(&value).map(Some),
        None => Ok(None),
    }
}

/// A [`REACH_ENV`] value amenbo did not write, in both languages.
fn unreadable(value: &str) -> Error {
    // The ref's shape, spelled by the module that owns every ref's spelling rather than by a literal here.
    let project_ref = format!("{}-{}-<n>", idref::NAMESPACE, RefKind::Project.code());
    Error::invalid(
        format!(
            "{REACH_ENV}='{value}' is not a reach amenbo wrote — it is set when amenbo launches a plugin, \
             and holds '{ALL_REACH}' or a project's {project_ref} ref. Unset it to run outside a plugin."
        ),
        format!(
            "{REACH_ENV}='{value}' は amenbo が書いた見える範囲ではありません——これは amenbo が \
             プラグインを起動するときに設定され、'{ALL_REACH}' かプロジェクトの {project_ref} 参照が \
             入ります。プラグイン以外で実行するなら、この変数を外してください。"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gate_is_the_window_it_fires_through() {
        assert_eq!(reach_of(Gate::Machine), Reach::All);
        assert_eq!(reach_of(Gate::Project(7)), Reach::Project(7));
    }

    #[test]
    fn a_reach_survives_the_round_trip_through_the_environment() {
        for reach in [Reach::All, Reach::Project(12)] {
            assert_eq!(decode_reach(&encode_reach(reach)).unwrap(), reach);
        }
        // The project side is written as the ref every other surface shows.
        assert_eq!(encode_reach(Reach::Project(12)), "AMB-P-12");
    }

    #[test]
    fn a_bare_number_reads_as_a_project_and_another_spaces_ref_does_not() {
        assert_eq!(decode_reach("12").unwrap(), Reach::Project(12));
        // A task's ref names another number space, so it is refused rather than resolving project 12.
        assert!(decode_reach("AMB-T-12").is_err());
        assert!(decode_reach("").is_err());
        assert!(decode_reach("everything").is_err());
    }

    #[test]
    fn the_env_names_the_store_and_the_window() {
        let vars = env(Path::new("/data/amenbo"), Reach::Project(3));
        assert_eq!(
            vars,
            vec![
                ("AMENBO_HOME".to_string(), "/data/amenbo".to_string()),
                ("AMENBO_PLUGIN_REACH".to_string(), "AMB-P-3".to_string()),
            ]
        );
    }
}
