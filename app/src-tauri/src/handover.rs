//! Giving an agent its opening instruction after the pane is already open (`AMB-D-793`).
//!
//! The instruction normally rides in on the command line, which is the certain route: it is handed
//! over before the program starts, it cannot be eaten by anything the program draws first, and it is
//! done in one move ([`amenbo_core::harness::opening`]). This is the other route — for a launch line
//! Amenbo did not compose and therefore has nowhere to put an argument in (`AMB-D-794`), and for a
//! program that takes no such argument at all.
//!
//! **The two are not equals, and this one is not a fallback to reach for.** What follows is a loop
//! that reads the pane while it writes into it, and it is a loop precisely because the thing it is
//! waiting for cannot be asked about.
//!
//! **There is no signal that says the agent is ready.** `ESC[?2004h` — the program turning bracketed
//! paste on — arrives 0.22s into Claude Code and 0.04s into Codex CLI, long before either has drawn
//! an input box, and alt-screen, mouse and focus-report all arrive just as early (`AMB-T-3819`). So
//! the readiness is not asked about; it is read off the screen afterwards.
//!
//! **The newline is the dangerous half, so it is the half that is conditional.** What a fresh folder
//! draws first is not always a trust prompt: Codex CLI opens on an `Update available!` whose first
//! choice runs `curl … | sh`, and Cline, Plandex and Continue each open on a choice of their own
//! (`AMB-T-3819`). A newline sent before the pane is holding this instruction is a newline answering
//! whatever *is* being asked. So it is sent on one condition and no other — that the instruction's
//! own opening words are on the screen.
//!
//! **The words are the test, not the wording of anything else.** Nothing here looks for "trust" or
//! any other of a program's own sentences: those differ per product and per version. What is looked
//! for is the text this loop itself pasted.
//!
//! **Every paste follows a screen that moved, the first one included.** Text pasted before a program
//! starts reading is not always thrown away — Crush kept it in the kernel's buffer and drew it later
//! — so a loop that pasted on a timer would put the sentence in twice. Pasting is therefore tied to
//! the screen having changed since the paste before it, measured from a screen with nothing on it:
//! a pane no program has written to yet is not pasted into at all.
//!
//! **Running out of patience leaves the sentence where it is.** It is sitting in the agent's input
//! box needing one keypress, and taking it away again to apologise would be worse than saying
//! nothing.

/// The bytes that open and close a bracketed paste. A program that has turned bracketed paste on
/// reads what is between them as text and never as keys; one that has not sees the markers as
/// characters, which is the cost of not being able to ask which kind it is.
const PASTE_OPEN: &[u8] = b"\x1b[200~";
const PASTE_CLOSE: &[u8] = b"\x1b[201~";

/// What submits the pasted text, once — and only once — the screen shows it.
const SUBMIT: &[u8] = b"\r";

/// How much of the instruction is looked for on the screen.
///
/// Short, because the screen is not a transcript: a pane narrow enough wraps the sentence, and a TUI
/// draws its own escape sequences between the characters it lays down, so the longer the run looked
/// for the likelier it is broken up by something that is not text. Fourteen characters is well
/// inside any pane a person works in and is still the instruction's own opening rather than a phrase
/// a program could write by itself.
const HEAD: usize = 14;

/// How this ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handover {
    /// The screen showed the instruction and the newline that submits it was sent.
    Sent,
    /// It never showed. The sentence is left in the agent's input box for the person to send.
    LeftForTheReader,
    /// The terminal ended, or would not take what was written to it, before either of those.
    Gone,
}

/// The leading run of `instruction` that a screen is searched for — the longest prefix of at most
/// [`HEAD`] characters. Cut on a character boundary, since the search is over the bytes a screen was
/// drawn with and half a character matches nothing.
fn head(instruction: &str) -> &str {
    match instruction.char_indices().nth(HEAD) {
        Some((at, _)) => &instruction[..at],
        None => instruction,
    }
}

/// Whether what the pane has drawn holds that run of text.
///
/// A plain search over the bytes, which is what the screen is: the pane keeps the terminal's output
/// as it arrived, escape sequences and all, and the agent draws the text it was given as text.
fn echoed(screen: &[u8], head: &str) -> bool {
    let head = head.as_bytes();
    !head.is_empty() && screen.windows(head.len()).any(|run| run == head)
}

/// The instruction as it is safe to paste: its control characters dropped.
///
/// A newline inside the paste is the thing this whole module exists to withhold — a program that
/// does not honour the brackets would read one as the keypress that submits — and an escape is a way
/// out of the brackets altogether. The launch instruction carries neither; a line a person registered
/// might (`AMB-D-794`), and the guard belongs where the bytes are written rather than at whichever
/// door they came in by.
fn pasteable(instruction: &str) -> String {
    instruction.chars().filter(|c| !c.is_control()).collect()
}

/// The bytes one attempt writes: the instruction, bracketed, with nothing that submits it.
fn paste(instruction: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(instruction.len() + PASTE_OPEN.len() + PASTE_CLOSE.len());
    out.extend_from_slice(PASTE_OPEN);
    out.extend_from_slice(instruction.as_bytes());
    out.extend_from_slice(PASTE_CLOSE);
    out
}

/// One number that changes when the screen does. FNV-1a over the tail the pane is holding — cheap
/// enough to take on every pass, and all that is asked of it is inequality.
fn moved(screen: &[u8]) -> u64 {
    screen.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x1000_0000_01b3)
    })
}

/// Hand `instruction` to whatever is running in the pane: paste it, watch for it to be drawn, and
/// submit it when it is.
///
/// `screen` answers with what the pane has drawn so far, or `None` once the terminal is gone.
/// `send` writes to the terminal and answers whether it could. `wait` is the pause between passes —
/// the caller's, so that what this does can be walked without a clock. `tries` bounds the whole of
/// it: the patience is passes × the length of `wait`.
pub fn hand_over(
    instruction: &str,
    tries: usize,
    mut screen: impl FnMut() -> Option<Vec<u8>>,
    mut send: impl FnMut(&[u8]) -> bool,
    wait: impl Fn(),
) -> Handover {
    let text = pasteable(instruction);
    let head = head(&text);
    let bytes = paste(&text);
    // The screen as it stood when the sentence was last pasted, starting from a screen with nothing
    // on it — so the first paste waits for a program to have drawn something, the same rule as every
    // paste after it.
    let mut pasted_at = moved(&[]);

    for pass in 0..tries {
        let Some(now) = screen() else { return Handover::Gone };
        if echoed(&now, head) {
            return if send(SUBMIT) { Handover::Sent } else { Handover::Gone };
        }
        let now = moved(&now);
        if pasted_at != now {
            if !send(&bytes) {
                return Handover::Gone;
            }
            pasted_at = now;
        }
        // No pause after the last pass: what follows it is the answer, not another look.
        if pass + 1 < tries {
            wait();
        }
    }
    Handover::LeftForTheReader
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// A stand-in for the program in a pane: what it has drawn, what was written at it, and how many
    /// pastes it swallows before it starts drawing them.
    ///
    /// It begins with its own first screen already on it — a pane holding nothing is one the loop
    /// leaves alone, which is its own test below.
    struct Agent {
        drawn: RefCell<Vec<u8>>,
        writes: RefCell<Vec<Vec<u8>>>,
        swallows: usize,
    }

    impl Agent {
        fn new(swallows: usize) -> Self {
            Self { drawn: RefCell::new(b"> ".to_vec()), writes: RefCell::new(Vec::new()), swallows }
        }

        /// What was written at it, in order.
        fn writes(&self) -> Vec<Vec<u8>> {
            self.writes.borrow().clone()
        }
    }

    /// Drive [`hand_over`] against one of those, with no clock.
    fn walk(agent: &Agent, instruction: &str, tries: usize) -> Handover {
        let pastes = Cell::new(0usize);
        hand_over(
            instruction,
            tries,
            || Some(agent.drawn.borrow().clone()),
            |bytes| {
                agent.writes.borrow_mut().push(bytes.to_vec());
                if bytes != SUBMIT {
                    pastes.set(pastes.get() + 1);
                    let mut drawn = agent.drawn.borrow_mut();
                    if pastes.get() > agent.swallows {
                        // Reading now, and what it draws is the text rather than the brackets round it.
                        drawn.extend_from_slice(instruction.as_bytes());
                    } else {
                        // Swallowed, and the screen moved for a reason of the program's own — the
                        // question a fresh folder is asked before anything else.
                        drawn.extend_from_slice(b"Do you trust this folder?");
                    }
                }
                true
            },
            || {},
        )
    }

    #[test]
    fn the_newline_follows_the_instruction_onto_the_screen_and_never_precedes_it() {
        let agent = Agent::new(1);
        assert_eq!(walk(&agent, "Before you act on any request", 10), Handover::Sent);
        let writes = agent.writes();
        let submit = writes.iter().position(|w| w == SUBMIT).expect("it was submitted");
        assert_eq!(submit, writes.len() - 1, "the newline is the last thing written");
        assert!(submit > 0, "something was pasted before it");
    }

    #[test]
    fn a_screen_that_never_draws_it_is_left_with_the_sentence_and_no_newline() {
        let agent = Agent::new(usize::MAX);
        assert_eq!(walk(&agent, "Before you act on any request", 5), Handover::LeftForTheReader);
        let writes = agent.writes();
        assert!(!writes.iter().any(|w| w == SUBMIT), "nothing was submitted");
        assert!(!writes.is_empty(), "and the sentence was pasted");
    }

    #[test]
    fn a_screen_that_keeps_moving_is_pasted_into_again() {
        // Every paste is swallowed and every one moves the screen, which is the one shape that earns
        // a second attempt.
        let agent = Agent::new(usize::MAX);
        assert_eq!(walk(&agent, "Before you act on any request", 4), Handover::LeftForTheReader);
        assert!(agent.writes().len() > 1, "a moving screen was tried again");
    }

    #[test]
    fn a_screen_standing_still_is_not_pasted_into_twice() {
        // A program that drew its interface and then waited: pass one has something to paste into,
        // and every pass after it is looking at the same screen.
        let drawn = b"crush> ".to_vec();
        let writes = Cell::new(0usize);
        let verdict = hand_over(
            "Before you act on any request",
            8,
            || Some(drawn.clone()),
            |_| {
                writes.set(writes.get() + 1);
                true
            },
            || {},
        );
        assert_eq!(verdict, Handover::LeftForTheReader);
        assert_eq!(writes.get(), 1, "the sentence went in once");
    }

    #[test]
    fn a_pane_no_program_has_written_to_is_not_pasted_into_at_all() {
        // Nothing has drawn, so there is no input box for the text to land in — and text that lands
        // in a program not yet reading is text that turns up twice later (`AMB-T-3819`).
        let writes = Cell::new(0usize);
        let verdict = hand_over(
            "Before you act on any request",
            8,
            || Some(Vec::new()),
            |_| {
                writes.set(writes.get() + 1);
                true
            },
            || {},
        );
        assert_eq!(verdict, Handover::LeftForTheReader);
        assert_eq!(writes.get(), 0, "nothing was written into a pane holding nothing");
    }

    #[test]
    fn a_terminal_that_has_gone_ends_it() {
        assert_eq!(hand_over("Before you act", 4, || None, |_| true, || {}), Handover::Gone);
    }

    #[test]
    fn a_write_that_fails_ends_it() {
        assert_eq!(
            hand_over("Before you act", 4, || Some(b"> ".to_vec()), |_| false, || {}),
            Handover::Gone
        );
    }

    #[test]
    fn what_is_pasted_carries_the_brackets_and_no_newline() {
        let bytes = paste("Before you act");
        assert!(bytes.starts_with(PASTE_OPEN));
        assert!(bytes.ends_with(PASTE_CLOSE));
        assert!(!bytes.contains(&b'\r') && !bytes.contains(&b'\n'));
    }

    #[test]
    fn a_line_carrying_a_newline_is_pasted_without_one() {
        // A registered launch line is a person's own text (`AMB-D-794`), and a newline in it would be
        // the keypress this module exists to hold back.
        let text = pasteable("say this\nand this\r\x1b[201~");
        assert_eq!(text, "say thisand this[201~");
        assert!(!paste(&text).windows(2).any(|w| w == b"\r\n"));
    }

    #[test]
    fn the_run_looked_for_is_short_enough_to_survive_a_narrow_pane() {
        let instruction = "Before you act on any request in this directory";
        assert_eq!(head(instruction), "Before you act");
        assert!(echoed(b"\x1b[2J\x1b[H> Before you act on any request", head(instruction)));
        assert!(!echoed(b"\x1b[2J\x1b[H> Do you trust this folder?", head(instruction)));
    }

    #[test]
    fn an_instruction_shorter_than_the_run_is_looked_for_whole() {
        assert_eq!(head("hello"), "hello");
        assert!(echoed(b"say hello there", head("hello")));
    }
}
