//! The **read-back path** — what a plugin needs in its environment to call `amenbo` back (`AMB-D-406`).
//!
//! A payload carries an id and a kind, and nothing of the record itself (`AMB-D-348`). Without a way to
//! read, that id names nothing a plugin can act on. The way is the one every author already has: **run
//! `amenbo <read command> --json`**. There is no second protocol, and no language binding — a plugin is any
//! executable (`AMB-D-346`), so the only route that works in every language is the binary itself.
//!
//! What a child process cannot work out on its own is *which* store and *how far*, so Amenbo hands it both
//! when it launches it, the same way git hands a hook `GIT_DIR`:
//!
//! | variable | what it says |
//! |---|---|
//! | [`STORE_ENV`] (`AMENBO_HOME`) | the store to open — the base directory the run's own store sits in |
//! | [`REACH_ENV`] (`AMENBO_PLUGIN_REACH`) | how far it may read — the project it fires for, or the device |
//!
//! **`amenbo` itself is put within reach, not assumed to be.** A plugin is told to run the binary by name,
//! so it has to be findable by name wherever a plugin is started from — and a plugin fired by the hourly
//! tick inherits the scheduler's `PATH`, not a shell's. Amenbo puts its own directory in front of the
//! child's `PATH` for exactly that reason (`AMB-D-716`, carried out in
//! [`crate::plugin_exec::PluginInvocation::spawn`]), which is why nothing here has to hand the author a
//! path to call and no plugin has to read one.
//!
//! **The store is named, never resolved.** A plugin's working directory is whatever its launcher happened
//! to be in, so the `.amenbo` walk would answer about a folder nobody consulted — and a runner works the
//! store its parent drove, not whichever one its own environment would find. Naming it is the same choice
//! [`SelfRunner`](crate::plugin_runner::SelfRunner) makes one level up.
//!
//! **The reach is the gate, read back.** A plugin fires through its own gate, and that gate is also the
//! window: it observes what it observes, so what it may read is what it may observe, and [`reach_of`] is
//! that identity spelled out. Which gate that is comes from the layer the author declared (`AMB-D-601`) —
//! one project for a `project` plugin, the whole device for a `machine` one. Both faces have already
//! resolved that layer ([`Layer::of`](crate::plugin_layer::Layer::of)) before anything runs, so neither has
//! to decide it twice, and neither needs a project id a device-wide run would only throw away.
//!
//! **This is not isolation** (`AMB-D-406`). A plugin has a shell: it can rewrite these variables, and it can
//! open the store file directly. The trust boundary is the explicit enable (`AMB-D-351`), not a sandbox —
//! what this door is for is that an author who needs the content has a documented way in, instead of being
//! pushed into reading the store behind Amenbo's back.

use std::path::Path;

use crate::error::{Error, Result};
use crate::idref::{self, RefKind};
use crate::plugin_layer::Layer;
use crate::reach::Reach;

/// The variable naming the store a plugin reads back from. It is Amenbo's own `AMENBO_HOME` — the root that
/// already means "the user layer lives here" — rather than a second spelling of the same fact, so a plugin's
/// `amenbo` call resolves its paths exactly as the process that launched it did.
pub const STORE_ENV: &str = crate::env::HOME_VAR;

/// The variable declaring how far a plugin may read: [`ALL_REACH`], or a project's `AMB-P-<n>` ref.
pub const REACH_ENV: &str = crate::env::PLUGIN_REACH_VAR;

/// The value of [`REACH_ENV`] that means the whole device — what a plugin declaring `scope: machine` is
/// handed (`AMB-D-601`). Its work is the machine's, so one project's window would hide most of what it was
/// enabled to carry; enabling it is the consent to let it read the whole device, and nothing asks again.
pub const ALL_REACH: &str = "all";

/// The window a plugin reads through, from the gate it fires through (`AMB-D-406`): the [`Layer`] its
/// author declared (`AMB-D-601`) **is** that gate, so it is the whole of what this takes.
///
/// A `project` plugin — the default, and what a manifest saying nothing means — reaches the one project
/// whose gate let it fire, which is the id its layer carries. A `machine` plugin's gate is the device's, so
/// its window is the device: it is handed [`Reach::All`], and it needs no project to say so — asking for one
/// would make a device-wide run demand a value it then discards, and refuse the runs that have none. That is
/// not a wider grant than the gate — it *is* the gate, read back, exactly as the project case is.
pub fn reach_of(layer: Layer) -> Reach {
    match layer {
        Layer::Project(id) => Reach::window(id),
        Layer::Device => Reach::All,
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
        Reach::Project { id, .. } => std::borrow::Cow::Owned(idref::project(id)),
    }
}

/// Read [`REACH_ENV`] back — the other half of [`encode_reach`], and the CLI's door into the plugin face.
///
/// A bare number is accepted beside the `AMB-P-<n>` ref, on the same "reading is looser than writing" footing
/// as every other ref Amenbo takes. A value that is neither is an **error**: the variable is set only by
/// Amenbo launching a plugin, so a value it cannot read means the window was declared and lost — falling back
/// to a default would open one the launcher never named.
pub fn decode_reach(value: &str) -> Result<Reach> {
    let value = value.trim();
    if value.eq_ignore_ascii_case(ALL_REACH) {
        return Ok(Reach::All);
    }
    idref::strip(RefKind::Project, value)
        .parse::<i64>()
        .map(Reach::window)
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

/// Was this process launched as a plugin at all? The question asked ahead of decoding, by a caller that
/// needs to know only *that* a window was handed over — the CLI's facet door, which stops asking for a facet
/// to draw a reach the window already fixed.
///
/// A value this build cannot read still declares a window (and loses it): [`reach_from_env`] fails on it a
/// moment later with a message naming the variable, so answering `false` here would replace that with a
/// complaint about something else entirely.
pub fn window_declared() -> bool {
    reach_from_env().map_or(true, |window| window.is_some())
}

/// A [`REACH_ENV`] value Amenbo did not write.
fn unreadable(value: &str) -> Error {
    // The ref's shape, spelled by the module that owns every ref's spelling rather than by a literal here.
    let project_ref = format!("{}-{}-<n>", idref::NAMESPACE, RefKind::Project.code());
    Error::invalid(
        format!(
            "{REACH_ENV}='{value}' is not a reach Amenbo wrote — it is set when Amenbo launches a plugin, \
             and holds '{ALL_REACH}' or a project's {project_ref} ref. Unset it to run outside a plugin."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::Scope;

    #[test]
    fn a_gate_is_the_window_it_fires_through() {
        assert_eq!(reach_of(Layer::Project(7)), Reach::window(7));
    }

    #[test]
    fn a_device_wide_plugin_reads_the_device_without_a_project_to_narrow_it() {
        assert_eq!(reach_of(Layer::Device), Reach::All);
        // The layer comes from the declaration, so a face standing in a project and one standing nowhere
        // hand a `machine` plugin the same window (`AMB-D-601`).
        assert_eq!(reach_of(Layer::of(Scope::Machine, Some(7)).unwrap()), Reach::All);
        assert_eq!(reach_of(Layer::of(Scope::Machine, None).unwrap()), Reach::All);
        // An undeclared layer is the project's (`AMB-D-601`), so a manifest written before `scope` existed
        // keeps the window it has always had.
        assert_eq!(reach_of(Layer::of(Scope::default(), Some(7)).unwrap()), Reach::window(7));
    }

    #[test]
    fn a_reach_survives_the_round_trip_through_the_environment() {
        for reach in [Reach::All, Reach::window(12)] {
            assert_eq!(decode_reach(&encode_reach(reach)).unwrap(), reach);
        }
        // The project side is written as the ref every other surface shows.
        assert_eq!(encode_reach(Reach::window(12)), "AMB-P-12");
    }

    #[test]
    fn a_bare_number_reads_as_a_project_and_another_spaces_ref_does_not() {
        assert_eq!(decode_reach("12").unwrap(), Reach::window(12));
        // A task's ref names another number space, so it is refused rather than resolving project 12.
        assert!(decode_reach("AMB-T-12").is_err());
        assert!(decode_reach("").is_err());
        assert!(decode_reach("everything").is_err());
    }

    #[test]
    fn the_env_names_the_store_and_the_window() {
        let vars = env(Path::new("/data/amenbo"), Reach::window(3));
        assert_eq!(
            vars,
            vec![
                ("AMENBO_HOME".to_string(), "/data/amenbo".to_string()),
                ("AMENBO_PLUGIN_REACH".to_string(), "AMB-P-3".to_string()),
            ]
        );
        // And the device-wide half, which is the same two variables with the window opened.
        assert_eq!(
            env(Path::new("/data/amenbo"), reach_of(Layer::Device))[1],
            ("AMENBO_PLUGIN_REACH".to_string(), ALL_REACH.to_string())
        );
    }
}
