//! **One turn at a time, on this whole machine** (`AMB-D-884`, ported from the `viewer` plugin the retreat
//! retired).
//!
//! A turn reads a stretch of the backlog out, places it, and writes down where it got to. Two turns doing
//! that at once put an older picture of a record on top of a newer one, and the phone is left holding the
//! older one with nothing to say so. So there is one hold, and a run that cannot take it does nothing.
//!
//! **It does not wait for its turn.** Waiting would be the wrong answer to the shape this is for: a write
//! sets a carrier off, and writes come in bursts, so the second one of a burst finds the first still
//! carrying the very stretch it would have carried. It has nothing of its own to do, so what it should do
//! is stop — which is what "no, and do not queue" makes it do. Waiting would hold a process open for a turn
//! that has already been taken.
//!
//! **Setup is the one that waits** ([`wait_for_the_turn`]). It has a thing of its own to write — the build
//! of the Worker it placed, or a memory thrown away for a server the carrier has not sent to — and a turn
//! running under it writes back the row it read at its start, over whatever setup put there. Stopping would
//! leave that unwritten, so setup waits for the turn to end and holds it while it writes.
//!
//! **The other reason is the one that started this.** A machine that sleeps hands the same stretch to two
//! runs at once, because a timer counts a wall clock that runs while the machine is asleep and a heartbeat
//! counts one that does not.
//!
//! **Nothing is ever written into the file.** What means "a turn is running" is the kernel's hold on it,
//! which a process that dies drops on the way out with no code of ours running. Bytes could not say that:
//! whatever a dead run left behind would go on claiming the turn for ever.

use std::fs::{OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::config::Paths;
use crate::error::{Error, Result};

/// What the hold is taken on, `<base>/viewer-sending.lock`. It is a file for want of anything else to name
/// — a hold has to be on something — and its content is deliberately nothing.
const FILE_NAME: &str = "viewer-sending.lock";

/// How long [`wait_at`] leaves between one try at the hold and the next.
const WAIT_PAUSE: Duration = Duration::from_millis(100);

/// The hold, for as long as it is kept. Letting go of this lets go of the turn: the file is closed, and
/// closing it is what the kernel reads as the hold being dropped.
///
/// **It is dropped rather than released by name.** A turn ends in several places — a refusal, a server that
/// asked to be left alone, a question the store would not answer — and a hold that had to be given back by
/// hand would be kept by whichever of those was written last.
#[derive(Debug)]
pub struct TheTurn(std::fs::File);

impl Drop for TheTurn {
    fn drop(&mut self) {
        // The close would do this on its own. Saying it is for the reader, so both ends of the pair are
        // visible; a hold this cannot drop is one the close drops a line later.
        let _ = self.0.unlock();
    }
}

/// Take the turn, or say that somebody else has it.
///
/// `None` is not a failure — it is this working. An `Err` is: a base directory that cannot be written in is
/// a device where nothing can be carried at all, and saying so is better than carrying without the hold.
pub fn take_the_turn(paths: &Paths) -> Result<Option<TheTurn>> {
    at(&paths.base_dir.join(FILE_NAME))
}

/// [`take_the_turn`], on a named file — what a test drives, two holds at a time.
pub fn at(path: &Path) -> Result<Option<TheTurn>> {
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(
        |err| Error::invalid(format!("the Viewer's sending lock cannot be opened: {err}")),
    )?;
    match file.try_lock() {
        Ok(()) => Ok(Some(TheTurn(file))),
        // A hold that is already taken is the answer this asks for rather than a fault, so it is read as
        // one; what is left over — a directory that went away, a filesystem with no locking at all — is
        // what comes back as an error. `std::fs`'s advisory locking separates the two in
        // `TryLockError`, as the store's swap lock reads them, so no platform's own number is compared
        // here. Each platform answers a taken hold with a number of its own — Windows with
        // `ERROR_LOCK_VIOLATION`, nothing like the `EWOULDBLOCK` a Unix gives — and comparing against one
        // of those alone left every Windows carrier but the first of a burst erroring where it should
        // have stopped.
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(err)) => Err(Error::invalid(format!(
            "the Viewer's sending lock cannot be taken: {err}"
        ))),
    }
}

/// Take the turn, waiting for whoever holds it to let go — for `up_to` at the most.
///
/// What [`take_the_turn`] answers with `None` this answers with an `Err` once `up_to` has passed: the one
/// who waits has something to write, and not writing it is a failure to say.
pub fn wait_for_the_turn(paths: &Paths, up_to: Duration) -> Result<TheTurn> {
    wait_at(&paths.base_dir.join(FILE_NAME), up_to)
}

/// [`wait_for_the_turn`], on a named file.
pub fn wait_at(path: &Path, up_to: Duration) -> Result<TheTurn> {
    let until = Instant::now() + up_to;
    loop {
        if let Some(turn) = at(path)? {
            return Ok(turn);
        }
        if Instant::now() >= until {
            return Err(Error::invalid(format!(
                "a send to the Viewer's server held its turn for longer than {}s",
                up_to.as_secs()
            )));
        }
        std::thread::sleep(WAIT_PAUSE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One run holds it and the next is told so, rather than waiting — and the moment the first lets go,
    /// the turn is there to be taken.
    #[test]
    fn a_second_run_is_told_the_turn_is_taken_rather_than_made_to_wait() {
        let dir = amenbo_scratch::scratch("viewer-lock-taken");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        let first = at(&path).unwrap().expect("nobody holds it yet");
        assert!(at(&path).unwrap().is_none(), "a turn that is taken is answered, not queued for");

        drop(first);
        assert!(at(&path).unwrap().is_some(), "letting go hands the turn on");
    }

    /// The one who waits is handed the turn the moment its holder lets go, and not before.
    #[test]
    fn a_waiter_takes_the_turn_once_it_is_let_go() {
        let dir = amenbo_scratch::scratch("viewer-lock-waited");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        let held = at(&path).unwrap().expect("nobody holds it yet");
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(held);
        });
        let waited = wait_at(&path, Duration::from_secs(30)).expect("the turn is let go within the wait");
        letting_go.join().unwrap();
        assert!(at(&path).unwrap().is_none(), "and the waiter holds it now");
        drop(waited);
    }

    /// A turn that is not let go within the wait is an error, not a hold taken anyway.
    #[test]
    fn a_waiter_that_waits_too_long_is_told_so() {
        let dir = amenbo_scratch::scratch("viewer-lock-waited-out");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        let _held = at(&path).unwrap().expect("nobody holds it yet");
        assert!(wait_at(&path, Duration::from_millis(250)).is_err());
    }

    /// The file says nothing, before or after. What means "a turn is running" is the kernel's hold, so a
    /// run that died leaves nothing behind that goes on claiming it.
    #[test]
    fn nothing_is_ever_written_into_it() {
        let dir = amenbo_scratch::scratch("viewer-lock-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        // The length is asked of the directory entry rather than by reading the file: Windows holds the
        // stretch a lock covers against readers too, so a read taken while the turn is held would fail
        // there for a reason that has nothing to do with what this is checking.
        let held = at(&path).unwrap().expect("nobody holds it yet");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        drop(held);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    }
}
