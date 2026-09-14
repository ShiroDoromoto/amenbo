//! **A code a camera can read, drawn where the person is looking** (`AMB-D-884`, ported from the
//! `viewer` plugin the retreat retired).
//!
//! **The screen and the camera are the one path with no network on it**, which is why pairing goes this
//! way round. What the code carries is the encryption key, and a key routed through the Worker would let
//! the Worker read everything it is storing.
//!
//! What is here is the drawing and not the deciding: what goes on a code is [`super::pairing`]'s.

use qrcode::{EcLevel, QrCode};

use crate::error::{Error, Result};

/// How much of a code may be damaged and still read. Medium is what a screen wants: a camera pointed at
/// glass loses modules to reflection and to its own focus, and the levels above it buy that back by
/// making the code denser — which is the thing a camera was already struggling with.
const HOW_MUCH_MAY_BE_LOST: EcLevel = EcLevel::M;

/// The white margin every side of a code needs. A reader finds a code by its edges, and a code drawn
/// flush against text has none to find.
const QUIET_ZONE: usize = 4;

/// Draw what a phone is to read, in half-blocks.
///
/// **Two rows of the code share one line of characters** — the upper half of the character and the lower
/// half — because a terminal cell is about twice as tall as it is wide, and one character per module
/// would come out stretched to the point a camera reads it as something else.
pub fn in_blocks(carried: &str) -> Result<String> {
    let code = QrCode::with_error_correction_level(carried.as_bytes(), HOW_MUCH_MAY_BE_LOST)
        .map_err(|err| Error::invalid(format!("this will not fit on a code a camera can read: {err}")))?;
    let width = code.width();
    let dark = code.into_colors();
    let black = |x: usize, y: usize| {
        let (x, y) = (x.wrapping_sub(QUIET_ZONE), y.wrapping_sub(QUIET_ZONE));
        x < width && y < width && dark[y * width + x] == qrcode::Color::Dark
    };

    const BOTH: char = '█';
    const UPPER: char = '▀';
    const LOWER: char = '▄';
    const NEITHER: char = ' ';

    let side = width + QUIET_ZONE * 2;
    let mut drawn = String::new();
    for y in (0..side).step_by(2) {
        for x in 0..side {
            drawn.push(match (black(x, y), black(x, y + 1)) {
                (true, true) => BOTH,
                (true, false) => UPPER,
                (false, true) => LOWER,
                (false, false) => NEITHER,
            });
        }
        drawn.push('\n');
    }
    Ok(drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The drawing is square, margined on every side, and made of the four characters a terminal has for
    /// two rows in one line.
    #[test]
    fn a_code_is_drawn_square_and_inside_a_margin() {
        let drawn = in_blocks("https://example.invalid/x").unwrap();
        let lines: Vec<&str> = drawn.lines().collect();
        let across = lines[0].chars().count();

        assert!(lines.iter().all(|line| line.chars().count() == across), "every line is the same width");
        // Two rows of the code to one line, so the drawing is half as tall as it is wide — give or take
        // the odd row a code of odd height leaves.
        assert!(lines.len() * 2 >= across && (lines.len() - 1) * 2 < across, "{} × {across}", lines.len());
        assert!(
            drawn.chars().all(|c| matches!(c, '█' | '▀' | '▄' | ' ' | '\n')),
            "nothing but the four characters and the line breaks"
        );

        // The margin: the first two lines and the first four columns are white on every row.
        assert!(lines[0].trim().is_empty() && lines[1].trim().is_empty(), "a margin above");
        assert!(
            lines.iter().all(|line| line.starts_with("    ")),
            "and one at the side — four modules of it"
        );
    }

    /// A longer code is a bigger one. The key travels on it, so what has to hold is that it grows rather
    /// than being cut to fit.
    #[test]
    fn more_to_carry_is_a_bigger_code() {
        let small = in_blocks("x").unwrap();
        let large = in_blocks(&"x".repeat(300)).unwrap();
        assert!(large.lines().count() > small.lines().count());
    }

    /// More than a code can hold is refused rather than drawn short. Nothing pairing puts on one comes
    /// near it, and a code that quietly lost half the key would pair a phone that reads nothing.
    #[test]
    fn more_than_a_code_can_hold_is_refused() {
        assert!(in_blocks(&"x".repeat(8_000)).is_err());
    }
}
