//! Progress display arithmetic.
//!
//! Ground truth: `minix3/minix/commands/progressbar/progressbar.c`. The tool
//! reads one total file count from the command line, then reads file names
//! from the standard input stream and redraws one line per name: the remaining
//! file count (`max minus done`), an opening bracket, `=` fill cells for the
//! finished share, one `|` cursor cell, `-` empty cells for the remaining
//! share, and a closing bracket. The bar is `WIDTH 77` columns wide in total.
//! Each input line is cut to 78 visible characters before display.

use crate::MaintError;

/// Total bar width in columns, including the surrounding text.
pub const BAR_WIDTH: usize = 77;

/// Parse the total file count from the command line word.
pub fn parse_total(word: &str) -> Result<u64, MaintError> {
    if word.is_empty() {
        return Err(MaintError::InvalidArgument);
    }
    let mut value: u64 = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_digit() {
            return Err(MaintError::InvalidArgument);
        }
        value = value
            .checked_mul(10)
            .and_then(|scaled| scaled.checked_add((byte - b'0') as u64))
            .ok_or(MaintError::InvalidArgument)?;
    }
    Ok(value)
}

/// Remaining file count (total minus done, saturating at zero).
pub fn remaining(total: u64, done: u64) -> u64 {
    total.saturating_sub(done)
}

/// Render one progress line into `out`, returning the used byte count.
///
/// The line holds `Remaining: {remaining} files. [` then the bar cells then
/// `]`. The bar cells share the columns left after the prefix: finished cells
/// print `=`, one cursor cell prints `|`, the rest print `-`. When the total
/// is zero the bar is all empty cells (division by zero must never panic).
pub fn render_progress(
    done: u64,
    total: u64,
    out: &mut [u8],
) -> Result<usize, MaintError> {
    let rest = remaining(total, done);
    let mut written: usize = 0;
    let put_byte = |out: &mut [u8], written: &mut usize, byte: u8| -> Result<(), MaintError> {
        if *written >= out.len() {
            return Err(MaintError::InvalidArgument);
        }
        out[*written] = byte;
        *written += 1;
        Ok(())
    };
    let put_text =
        |out: &mut [u8], written: &mut usize, text: &[u8]| -> Result<(), MaintError> {
            for byte in text {
                put_byte(out, written, *byte)?;
            }
            Ok(())
        };
    put_text(out, &mut written, b"Remaining: ")?;
    put_unsigned(out, &mut written, rest)?;
    put_text(out, &mut written, b" files. [")?;
    let prefix_len = written;
    let bar_cells = BAR_WIDTH.saturating_sub(prefix_len + 1);
    let filled = if total == 0 || bar_cells == 0 {
        0
    } else {
        ((done as u128 * (bar_cells as u128).saturating_sub(1)) / total as u128) as usize
    };
    let filled = filled.min(bar_cells.saturating_sub(1));
    for _ in 0..filled {
        put_byte(out, &mut written, b'=')?;
    }
    if bar_cells > 0 {
        put_byte(out, &mut written, b'|')?;
        for _ in 0..bar_cells.saturating_sub(filled + 1) {
            put_byte(out, &mut written, b'-')?;
        }
    }
    put_byte(out, &mut written, b']')?;
    Ok(written)
}

fn put_unsigned(out: &mut [u8], written: &mut usize, mut value: u64) -> Result<(), MaintError> {
    if value == 0 {
        if *written >= out.len() {
            return Err(MaintError::InvalidArgument);
        }
        out[*written] = b'0';
        *written += 1;
        return Ok(());
    }
    let mut digits = [0u8; 20];
    let mut count = 0;
    while value > 0 {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }
    while count > 0 {
        count -= 1;
        if *written >= out.len() {
            return Err(MaintError::InvalidArgument);
        }
        out[*written] = digits[count];
        *written += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(done: u64, total: u64) -> String {
        let mut out = [0u8; 160];
        let len = render_progress(done, total, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_total_parses() {
        assert_eq!(parse_total("120"), Ok(120));
        assert_eq!(parse_total("0"), Ok(0));
    }

    #[test]
    fn test_total_rejects_words() {
        assert_eq!(parse_total(""), Err(MaintError::InvalidArgument));
        assert_eq!(parse_total("-5"), Err(MaintError::InvalidArgument));
        assert_eq!(parse_total("12a"), Err(MaintError::InvalidArgument));
    }

    #[test]
    fn test_remaining_saturates() {
        assert_eq!(remaining(10, 3), 7);
        assert_eq!(remaining(3, 10), 0);
    }

    #[test]
    fn test_line_starts_with_remaining() {
        assert!(render(3, 10).starts_with("Remaining: 7 files. ["));
        assert!(render(10, 10).starts_with("Remaining: 0 files. ["));
    }

    #[test]
    fn test_bar_ends_with_bracket() {
        assert!(render(3, 10).ends_with(']'));
    }

    #[test]
    fn test_finished_bar_has_no_dashes() {
        let line = render(10, 10);
        let body = line.split('[').nth(1).unwrap();
        assert!(!body.contains('-'));
        assert!(body.contains('|'));
    }

    #[test]
    fn test_zero_total_never_panics() {
        let line = render(0, 0);
        assert!(line.starts_with("Remaining: 0 files. ["));
    }

    #[test]
    fn test_small_buffer_rejected() {
        let mut out = [0u8; 4];
        assert_eq!(
            render_progress(1, 10, &mut out),
            Err(MaintError::InvalidArgument)
        );
    }
}
