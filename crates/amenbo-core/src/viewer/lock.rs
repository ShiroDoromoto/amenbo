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
//! **The other reason is the one that started this.** A machine that sleeps hands the same stretch to two
//! runs at once, because a timer counts a wall clock that runs while the machine is asleep and a heartbeat
//! counts one that does not.
//!
//! **Nothing is ever written into the file.** What means "a turn is running" is the kernel's hold on it,
//! which a process that dies drops on the way out with no code of ours running. Bytes could not say that:
//! whatever a dead run left behind would go on claiming the turn for ever.

use std::fs::OpenOptions;
use std::path::Path;

use fs2::FileExt as _;

use crate::config::Paths;
use crate::error::{Error, Result};

/// What the hold is taken on, `<base>/viewer-sending.lock`. It is a file for want of anything else to name
/// — a hold has to be on something — and its content is deliberately nothing.
const FILE_NAME: &str = "viewer-sending.lock";

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
        let _ = fs2::FileExt::unlock(&self.0);
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
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(TheTurn(file))),
        // Every platform answers a hold that is already taken with its own error, and `fs2` hands them
        // over as they are. It is the answer this asks for rather than a fault, so it is read as one:
        // what is left over — a directory that went away, a filesystem with no locking at all — is what
        // comes back as an error.
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(Error::invalid(format!(
            "the Viewer's sending lock cannot be taken: {err}"
        ))),
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

    /// The file says nothing, before or after. What means "a turn is running" is the kernel's hold, so a
    /// run that died leaves nothing behind that goes on claiming it.
    #[test]
    fn nothing_is_ever_written_into_it() {
        let dir = amenbo_scratch::scratch("viewer-lock-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        let held = at(&path).unwrap().expect("nobody holds it yet");
        assert_eq!(std::fs::read(&path).unwrap(), Vec::<u8>::new());
        drop(held);
        assert_eq!(std::fs::read(&path).unwrap(), Vec::<u8>::new());
    }
}
