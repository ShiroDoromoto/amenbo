//! The hourly tick: the one plain timer amenbo asks this machine's scheduler to hold (`AMB-D-707`),
//! and whether it may ask at all.
//!
//! **What is registered carries no meaning.** It wakes amenbo once an hour, and amenbo decides once
//! awake what is due. That is the whole reason there is one: a second use never becomes a second
//! timer, so the path that rewrites the OS's own settings every time amenbo grows does not exist. The
//! user sees one row, and switching it off stops everything behind it.
//!
//! Registering writes into the machine's scheduler, which amenbo does not do unasked, so it asks —
//! **once for the tick as a feature, on this device** ([`TickConsent`], kept in
//! [`crate::config::Config::tick_consent`]). The same shape as the lint's [`crate::hooks::HookConsent`],
//! and for the same reason: nobody wants the tick on Tuesdays but not Wednesdays, so asking more than
//! once is repeating a question whose answer is already known. There is no scale below the device here
//! — one machine holds one timer — so the lint's per-repository opt-out has no counterpart, and
//! [`crate::config::Config::tick_consent`] is the only record there is.
//!
//! **The answer and the registration are two independent facts.** The answer says what was consented
//! to and never what the scheduler holds, which is [`probe`]'s answer and is read from the OS every
//! time. The row is the user's to switch off from their own settings, and an answer read as a mirror
//! of the OS would leave amenbo claiming a timer that is not there. The two meet in exactly one place,
//! [`fix_for`].
//!
//! **What goes inside a registration is not here.** A plist, a scheduler task and a systemd unit are
//! not one shape with three spellings, so each OS writes its own, and the build picks one. Three points from
//! `AMB-D-707` are not writing style but whether the premise holds at all, and every door has to meet
//! them: macOS registers through `SMAppService` so the row carries amenbo's name rather than the
//! developer's; all three need the missed-run setting turned on explicitly, none having it by default;
//! and Windows has to have both battery gates turned off, or a laptop off its charger runs no tick at
//! all.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// The answer on record — **one for the device, given once and never asked for again**. There is no
/// `Unanswered` variant: never having answered is the *absence* of an answer (`Option::None`), which is
/// what makes "asked and refused" different from "never asked".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TickConsent {
    /// Hold the timer. It is one registration for the machine, so there is nowhere else for this yes to
    /// reach and nothing further to ask.
    Yes,
    /// Do not hold it, and do not ask again. It answers the question for good and forbids nothing: an
    /// explicit `tick install` asked for later is still honoured.
    No,
}

/// What a pass has to do to bring the answer and the registration back into agreement — the drift
/// table as a value, walked row by row in [`fix_for`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickFix {
    /// They already agree.
    Nothing,
    /// Wanted, and something is registered: write it again, so it names the executable running now. A
    /// registration pointing anywhere else wakes nothing, and says nothing about having failed.
    Rewrite,
    /// Wanted, and nothing is registered: the user switched the row off where they can see it, and the
    /// answer follows rather than putting the timer back behind them.
    TakeTheAnswerBack,
    /// Not wanted, and something is registered: take it away.
    Deregister,
}

/// Read the answer and the scheduler against each other and say what to do.
///
/// Registered-and-current is not told apart from registered-and-stale, and need not be: a scheduler
/// answers whether it holds the registration, not what it points at, so telling the two apart would
/// mean parsing a plist, an XML task and a unit file back. [`TickFix::Rewrite`] covers both — it is
/// what a stale registration needs and what a current one already says — which is how a build that has
/// moved gets the timer pointed at it again. The cost is one write per startup, once a door exists.
///
/// A registration the user removed is **not** written back. The lint makes the opposite call, and the
/// difference is what the two features are: a git hook is amenbo's own file in a repository's plumbing,
/// while the tick is a row in the user's system settings with a switch on it. Undoing that switch from
/// under them would make it not a switch — so the answer follows what they can see
/// ([`TickFix::TakeTheAnswerBack`]), and turning the tick back on is `tick install`.
///
/// Never having been asked leaves everything alone, registration included. Adopting one as a yes would
/// record an answer nobody gave, and taking it away would remove a timer the user may well have asked
/// an older build for; the question is still live either way, and it is the explicit faces that put it.
pub fn fix_for(consent: Option<TickConsent>, registered: bool) -> TickFix {
    match (consent, registered) {
        (Some(TickConsent::Yes), true) => TickFix::Rewrite,
        (Some(TickConsent::Yes), false) => TickFix::TakeTheAnswerBack,
        (Some(TickConsent::No), true) => TickFix::Deregister,
        (Some(TickConsent::No), false) | (None, _) => TickFix::Nothing,
    }
}

/// Whether this build has a door into this machine's scheduler at all.
///
/// False is not a failure and not a refusal to answer: it is the honest state of a target amenbo has
/// not learned to register on, and every face here says so rather than half-doing the work. While it is
/// false [`probe`] is `false`, and [`register`] and [`unregister`] refuse.
///
/// It is a fact about the **build**, not about the machine: a Linux without systemd on it still has a
/// door compiled in, and what it does not have is something behind the door — which is [`probe`]'s
/// answer and the writes' refusal, said where the machine is actually asked.
pub fn available() -> bool {
    platform::AVAILABLE
}

/// Is the scheduler holding amenbo's tick right now? Read from the OS, every time — never from the
/// answer on record (see the module docs).
pub fn probe() -> Result<bool> {
    platform::probe()
}

/// Write the registration, or write it again over one that is already there.
///
/// **Idempotent by contract**: running it twice leaves one registration and no error. It is what makes
/// [`TickFix::Rewrite`] a single move rather than a remove-then-add with a window in between, and what
/// lets an upgrade point the timer at the build running now without first asking what the old one said.
pub fn register() -> Result<()> {
    platform::register()
}

/// Take the registration away. Idempotent in the same way: with nothing registered it succeeds, having
/// left the machine in the state the caller asked for.
pub fn unregister() -> Result<()> {
    platform::unregister()
}

/// The door into this machine's scheduler, chosen for the target being built.
///
/// What goes into a registration is per-OS in a way the rest of this module is deliberately not — the
/// plist macOS wants is written by the app bundle through `SMAppService`, the Windows task has to be
/// built from XML because `schtasks` has no flag for the battery gates, and Linux writes a pair of
/// systemd user units — so each door lands with the OS that needs it (`AMB-T-3253` / `AMB-T-3254`)
/// rather than being guessed at from here. A target with no door yet answers through `nodoor`, which is
/// the honest state and not a half-written one.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(not(target_os = "linux"))]
mod nodoor;
#[cfg(not(target_os = "linux"))]
use nodoor as platform;

#[cfg(test)]
mod tests {
    use super::*;

    /// The drift table in full, so a later reading of it has to move a row on purpose.
    #[test]
    fn the_answer_and_the_registration_settle_the_way_the_user_can_see() {
        // Wanted and held: written again, because a registration naming a place this build has moved
        // out of wakes nothing and says nothing about having failed.
        assert_eq!(fix_for(Some(TickConsent::Yes), true), TickFix::Rewrite);
        // Wanted but gone: the user switched the row off, and the answer follows what they can see.
        assert_eq!(fix_for(Some(TickConsent::Yes), false), TickFix::TakeTheAnswerBack);
        // Refused but held: left over from an answer that has since changed, and it goes.
        assert_eq!(fix_for(Some(TickConsent::No), true), TickFix::Deregister);
        assert_eq!(fix_for(Some(TickConsent::No), false), TickFix::Nothing);
    }

    /// Never asked is left alone, whatever the scheduler holds: neither reading a registration as a
    /// yes nor taking one away is something an unanswered question licenses.
    #[test]
    fn an_unasked_device_is_left_exactly_as_it_is() {
        assert_eq!(fix_for(None, false), TickFix::Nothing);
        assert_eq!(fix_for(None, true), TickFix::Nothing);
    }

    /// With no door on this target, the state is readable and the two writes refuse — the reason a
    /// caller can say so, rather than reporting a registration that was never written.
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn a_target_with_no_door_answers_rather_than_pretending() {
        assert!(!available());
        assert!(!probe().expect("a target with no door still has a state to report"));
        assert!(register().is_err());
        assert!(unregister().is_err());
    }

    /// And on a target that has one, the reading still answers — with nothing held, on a machine that
    /// was never asked to hold anything. What the writes do is the machine's to decide, so they are not
    /// called here: a build box is not a place to register a timer on.
    #[test]
    #[cfg(target_os = "linux")]
    fn a_target_with_a_door_reads_the_scheduler_without_writing_to_it() {
        assert!(available());
        assert!(!probe().expect("the scheduler has a state to report"));
    }
}
