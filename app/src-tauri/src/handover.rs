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
//! whatever *is* being asked — and Claude Code 2.1.251 answers its trust prompt `No, exit`, so the
//! blind newline is the one that closes the program (`AMB-T-4008`).
//!
//! **The words are the test wherever there are words to test.** Nothing here looks for "trust" or
//! any other of a program's own sentences: those differ per product and per version. What is looked
//! for is the text this loop itself pasted.
//!
//! **But a pane does not always show it, so there is a second test: the pane answered.** OpenCode
//! 1.18.23 folds a bracketed paste into a `[Pasted ~1 lines]` chip and never draws the body. Against
//! that program the words never arrive however long they are waited for — six pastes went in and all
//! six were read as "not there" (`AMB-T-4008`) — so the sentence would sit in a box nobody submits.
//!
//! **"The pane answered" means it moved when nothing else was going to move it.** The sentence goes
//! into a screen that has held the same bytes across a run of looks (`STILL` of them), and the
//! answer is owed on the very next look and no later. A program still drawing its interface is not
//! standing still; one that has drawn it and is waiting is. Movement any later than that next look
//! is a state the program went into by itself — a person answering the prompt it opened on — and is
//! never read as an answer to the paste: the pane is waited on until it stands still again, and
//! pasted into once more. What this cannot tell apart is a person answering inside that one look,
//! which is why the stillness has to be held rather than caught in a single frame.
//!
//! **The sentence goes into a given screen once.** A screen already pasted into is not pasted into
//! again, and a pane no program has written to yet is not pasted into at all — text that lands in a
//! program not yet reading is text that turns up twice later, as Crush's did by keeping it in the
//! kernel's buffer and drawing it once it started (`AMB-T-3819`).
//!
//! **Running out of patience leaves the sentence where it is.** It is sitting in the agent's input
//! box needing one keypress, and taking it away again to apologise would be worse than saying
//! nothing. What is said instead is said outside the pane: the row above it carries that the
//! sentence is unsent and that Enter sends it (`crate::pty`, `app/src/talk/nameplate.ts`). Writing
//! it into the pane is the one place it cannot go — that is the screen this loop is reading, and a
//! line of Amenbo's own there is a pane answering itself.
//!
//! **The fact ends this before the patience does.** Whether the agent has the canon is settled by
//! whether it ran `amenbo agent`, which it says itself (`AMB-D-805`) — and against that, everything
//! read off a screen here is a guess. So the loop is asked before every pass and gets off the moment
//! the fact arrives: there is nothing left to hand over, and a paste into a pane whose agent is
//! already working is one the person has to clear out of the box themselves.
//!
//! **It is asked, never answered.** [`crate::handover::Handover::Sent`] says a newline went out, not
//! that anything came of it — the screen it was read off is the very thing this is not trusting — so
//! nothing here ever tells the pane it has been briefed.

/// The bytes that open and close a bracketed paste. A program that has turned bracketed paste on
/// reads what is between them as text and never as keys; one that has not sees the markers as
/// characters, which is the cost of not being able to ask which kind it is.
const PASTE_OPEN: &[u8] = b"\x1b[200~";
const PASTE_CLOSE: &[u8] = b"\x1b[201~";

/// What submits the pasted text, once — and only once — the pane has shown it or answered for it.
const SUBMIT: &[u8] = b"\r";

/// How much of the instruction is looked for on the screen.
///
/// Short, because the screen is not a transcript: a pane narrow enough wraps the sentence, and a TUI
/// draws its own escape sequences between the characters it lays down, so the longer the run looked
/// for the likelier it is broken up by something that is not text. Fourteen characters is well
/// inside any pane a person works in and is still the instruction's own opening rather than a phrase
/// a program could write by itself.
const HEAD: usize = 14;

/// How many looks in a row must find the pane holding exactly what it held before, for it to count
/// as standing still.
///
/// One look is not enough for what stillness is being asked to prove. A program working through its
/// own startup goes quiet between the pieces it draws, and a paste dropped into one of those gaps is
/// followed by the next piece — which is movement right after a paste, and reads as an answer. A
/// pause this long is one a program mid-startup does not take, while a program waiting for input
/// takes it forever.
const STILL: usize = 3;

/// How this ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handover {
    /// The pane showed the instruction, or answered the paste of it, and the newline that submits it
    /// was sent.
    Sent,
    /// Neither ever happened. The sentence is left in the agent's input box for the person to send.
    LeftForTheReader,
    /// The terminal ended, or would not take what was written to it, before either of those.
    Gone,
    /// The agent said it had run `amenbo agent` in this pane, so the canon is already where this was
    /// carrying it (`AMB-D-805`). Whether it arrived by the last pass's newline or by a person, the
    /// question this loop was asking is answered.
    Briefed,
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

/// Hand `instruction` to whatever is running in the pane: paste it into a pane standing still, watch
/// for it to be drawn or answered for, and submit it when either happens.
///
/// `briefed` answers whether the fact has arrived that the agent ran `amenbo agent` here; while it
/// says no this goes on, and the pass it says yes on is the last. `screen` answers with what the pane
/// has drawn so far, or `None` once the terminal is gone. `send` writes to the terminal and answers
/// whether it could. `wait` is the pause between passes — the caller's, so that what this does can be
/// walked without a clock. `tries` bounds the whole of it: the patience is passes × the length of
/// `wait`.
pub fn hand_over(
    instruction: &str,
    tries: usize,
    mut briefed: impl FnMut() -> bool,
    mut screen: impl FnMut() -> Option<Vec<u8>>,
    mut send: impl FnMut(&[u8]) -> bool,
    wait: impl Fn(),
) -> Handover {
    let text = pasteable(instruction);
    let head = head(&text);
    let bytes = paste(&text);
    // A pane no program has written to: the one screen that is never pasted into, however still it
    // is.
    let nothing = moved(&[]);
    // The pane as the look before this one found it, and how many looks running have found it that
    // way.
    let mut held = nothing;
    let mut stood = 1usize;
    // The screen the sentence last went into, and whether that paste's answer falls due this look.
    let mut pasted_into: Option<u64> = None;
    let mut answer_due = false;

    for pass in 0..tries {
        // First, and before the screen is looked at: the fact outranks anything read off one, so a
        // pane whose agent has the canon is never pasted into on the strength of how it looks.
        if briefed() {
            return Handover::Briefed;
        }
        let Some(drawn) = screen() else { return Handover::Gone };
        if echoed(&drawn, head) {
            return if send(SUBMIT) { Handover::Sent } else { Handover::Gone };
        }
        let now = moved(&drawn);
        stood = if now == held { stood + 1 } else { 1 };
        held = now;

        if std::mem::take(&mut answer_due) {
            // Owed on this look and no later: the pane was standing still when the sentence went in,
            // so nothing but the sentence was going to move it.
            if Some(now) != pasted_into {
                return if send(SUBMIT) { Handover::Sent } else { Handover::Gone };
            }
        } else if stood >= STILL && now != nothing && Some(now) != pasted_into {
            if !send(&bytes) {
                return Handover::Gone;
            }
            pasted_into = Some(now);
            answer_due = true;
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
    use std::collections::VecDeque;

    /// How the program in a pane takes a bracketed paste.
    #[derive(Clone, Copy)]
    enum Takes {
        /// It draws the text, the way a plain input box does.
        Echoes,
        /// It draws something back but not the text — OpenCode's chip (`AMB-T-4008`).
        Acknowledges,
        /// It draws nothing at all: the paste reached something reading keys, not text.
        Swallows,
    }

    /// A stand-in for the program in a pane.
    ///
    /// Its screen moves for two reasons, and telling those apart is the whole of what this module
    /// does: what it draws of its own accord — one entry of `own` per look, on a schedule owing
    /// nothing to what was written at it — and what it draws back when pasted into (`takes`).
    struct Agent {
        drawn: RefCell<Vec<u8>>,
        writes: RefCell<Vec<Vec<u8>>>,
        own: RefCell<VecDeque<&'static [u8]>>,
        takes: Takes,
        /// The look the fact arrives on, counting from one — the pass on which this agent has run
        /// `amenbo agent`. `None` for one that never does.
        briefed_on: Cell<Option<usize>>,
        /// How many looks have been taken, which is what `briefed_on` is measured against.
        looks: Cell<usize>,
    }

    impl Agent {
        fn new(takes: Takes, own: impl IntoIterator<Item = &'static [u8]>) -> Self {
            Self {
                drawn: RefCell::new(Vec::new()),
                writes: RefCell::new(Vec::new()),
                own: RefCell::new(own.into_iter().collect()),
                takes,
                briefed_on: Cell::new(None),
                looks: Cell::new(0),
            }
        }

        /// One that runs `amenbo agent` on the given look, whatever else it is drawing.
        fn runs_agent_on(self, look: usize) -> Self {
            self.briefed_on.set(Some(look));
            self
        }

        /// One that draws its prompt on the first look and then waits.
        fn waiting(takes: Takes) -> Self {
            Self::new(takes, [&b"> "[..]])
        }

        /// What was written at it, in order.
        fn writes(&self) -> Vec<Vec<u8>> {
            self.writes.borrow().clone()
        }

        /// How many of those were the sentence rather than the newline.
        fn pastes(&self) -> usize {
            self.writes.borrow().iter().filter(|w| w.as_slice() != SUBMIT).count()
        }

        fn submitted(&self) -> bool {
            self.writes.borrow().iter().any(|w| w.as_slice() == SUBMIT)
        }
    }

    /// Drive [`hand_over`] against one of those, with no clock.
    fn walk(agent: &Agent, instruction: &str, tries: usize) -> Handover {
        hand_over(
            instruction,
            tries,
            || {
                agent.looks.set(agent.looks.get() + 1);
                agent.briefed_on.get().is_some_and(|on| agent.looks.get() >= on)
            },
            || {
                if let Some(next) = agent.own.borrow_mut().pop_front() {
                    agent.drawn.borrow_mut().extend_from_slice(next);
                }
                Some(agent.drawn.borrow().clone())
            },
            |bytes| {
                agent.writes.borrow_mut().push(bytes.to_vec());
                if bytes != SUBMIT {
                    let mut drawn = agent.drawn.borrow_mut();
                    match agent.takes {
                        Takes::Echoes => drawn.extend_from_slice(instruction.as_bytes()),
                        Takes::Acknowledges => drawn.extend_from_slice(b"[Pasted ~1 lines]"),
                        Takes::Swallows => {}
                    }
                }
                true
            },
            || {},
        )
    }

    #[test]
    fn the_newline_follows_the_instruction_onto_the_screen_and_never_precedes_it() {
        let agent = Agent::waiting(Takes::Echoes);
        assert_eq!(walk(&agent, "Before you act on any request", 10), Handover::Sent);
        let writes = agent.writes();
        let submit = writes.iter().position(|w| w == SUBMIT).expect("it was submitted");
        assert_eq!(submit, writes.len() - 1, "the newline is the last thing written");
        assert!(submit > 0, "something was pasted before it");
    }

    #[test]
    fn a_pane_that_answers_without_ever_showing_the_words_is_submitted_into() {
        // OpenCode folds the paste into a chip, so the words never arrive; what does arrive is a
        // screen that moved on the look after the sentence went into a pane standing still.
        let agent = Agent::waiting(Takes::Acknowledges);
        assert_eq!(walk(&agent, "Before you act on any request", 10), Handover::Sent);
        assert_eq!(agent.pastes(), 1, "the sentence went in once");
        assert!(agent.submitted());
    }

    #[test]
    fn a_pane_that_never_answers_is_pasted_into_once_and_left_with_the_sentence() {
        let agent = Agent::waiting(Takes::Swallows);
        assert_eq!(walk(&agent, "Before you act on any request", 12), Handover::LeftForTheReader);
        assert!(!agent.submitted(), "nothing was submitted");
        assert_eq!(agent.pastes(), 1, "and it was not collected one copy per look");
    }

    #[test]
    fn a_prompt_answered_long_after_the_paste_is_not_read_as_an_answer_to_it() {
        // The shape that closes Claude Code if it is got wrong: the paste is swallowed by whatever
        // the program opened on, and the screen moves later because a person answered that — not
        // because the sentence landed. A newline here would be the answer to the next question.
        let agent = Agent::new(
            Takes::Swallows,
            [&b"claude"[..], b"", b"", b"", b"", b"Do you trust the files in this folder?"],
        );
        assert_eq!(walk(&agent, "Before you act on any request", 12), Handover::LeftForTheReader);
        assert!(!agent.submitted(), "no newline was sent at a prompt the sentence is not in");
        assert_eq!(agent.pastes(), 2, "the settled screen was pasted into, and so was the next one");
    }

    #[test]
    fn a_screen_still_drawing_itself_is_not_pasted_into() {
        // Startup, one piece per look. There is no still screen to paste into, so the sentence waits
        // rather than landing in a program that is not reading yet.
        let agent = Agent::new(
            Takes::Echoes,
            [&b"one"[..], b"two", b"three", b"four", b"five", b"six"],
        );
        assert_eq!(walk(&agent, "Before you act on any request", 6), Handover::LeftForTheReader);
        assert_eq!(agent.pastes(), 0);
    }

    #[test]
    fn a_pane_no_program_has_written_to_is_not_pasted_into_at_all() {
        // Nothing has drawn, so there is no input box for the text to land in — and text that lands
        // in a program not yet reading is text that turns up twice later (`AMB-T-3819`).
        let writes = Cell::new(0usize);
        let verdict = hand_over(
            "Before you act on any request",
            8,
            || false,
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
        assert_eq!(
            hand_over("Before you act", 4, || false, || None, |_| true, || {}),
            Handover::Gone
        );
    }

    #[test]
    fn a_write_that_fails_ends_it() {
        assert_eq!(
            hand_over("Before you act", 4, || false, || Some(b"> ".to_vec()), |_| false, || {}),
            Handover::Gone
        );
    }

    #[test]
    fn an_agent_that_already_has_the_canon_is_never_pasted_into() {
        // It came in on the command line, or a person had already got it there: either way the fact is
        // in before the first look, and a sentence pasted now is one somebody has to clear out.
        let agent = Agent::waiting(Takes::Swallows).runs_agent_on(1);
        assert_eq!(walk(&agent, "Before you act on any request", 12), Handover::Briefed);
        assert_eq!(agent.pastes(), 0, "nothing went into a pane that did not need it");
        assert!(!agent.submitted());
    }

    #[test]
    fn the_fact_arriving_mid_loop_gets_it_off() {
        // The pane swallows the paste, so nothing on the screen will ever end this — but the agent
        // read the sentence all the same and ran the command. That settles it, and the loop stops
        // rather than pasting into every still screen for the rest of its patience.
        let agent = Agent::waiting(Takes::Swallows).runs_agent_on(6);
        assert_eq!(walk(&agent, "Before you act on any request", 60), Handover::Briefed);
        assert_eq!(agent.pastes(), 1, "the one paste it had already made, and no more");
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
