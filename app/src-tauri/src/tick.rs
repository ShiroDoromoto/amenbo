//! The hourly tick's startup pass: settle what this device answered against what its scheduler holds
//! (`AMB-D-707`), once, as the app comes up.
//!
//! The judgement is [`amenbo_core::tick::settle`]'s and is shared with the CLI, which makes the same
//! pass before the command a person came for. What is here is the occasion and the writing back.
//!
//! **On macOS this app is the only face that can make the pass.** The registration is written through
//! `SMAppService`, which reads the agent plist out of the calling process's main bundle — and the CLI
//! on `PATH` is a symlink into the bundle rather than the bundle's own executable, so it has none. It
//! can re-launch the copy inside the bundle for a command a person typed, and does; doing that on every
//! command, to settle a state that rarely drifts, is a cost with no occasion. This app is launched from
//! inside the bundle, so the pass costs it nothing.
//!
//! **A development build makes the pass like any other.** The login registration withholds itself from
//! one (`AMB-D-547`) because it would register an executable in a working tree that is rebuilt and
//! thrown away. Nothing here registers anything the user did not ask for: [`amenbo_core::tick::settle`]
//! returns before touching the scheduler unless the answer on record is already yes or no, and only an
//! explicit `tick install` writes one. So a dev build that was asked to hold a timer is exactly the
//! build that should tidy it up afterwards, and withholding the pass would leave that to nobody.

/// Settle the answer and the registration, and write the answer back if it moved.
///
/// Best-effort throughout, like everything else in the startup path: a config that cannot be resolved
/// or written leaves both halves as the last run left them, which is the state that was working. The
/// only thing that ever moves here is a yes going back to no, after the user took the registration away
/// where they can see it.
pub fn reconcile() {
    let Ok(paths) = amenbo_core::config::Paths::resolve() else {
        return;
    };
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    let Some(answer) = amenbo_core::tick::settle(config.tick_consent) else {
        return;
    };
    config.tick_consent = Some(answer);
    if let Err(e) = config.save(&paths.config_file) {
        log::warn!("tick: the registration is gone but the answer could not follow ({e})");
    }
}
