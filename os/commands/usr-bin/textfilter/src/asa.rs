//! `asa` — interpret ASA carriage-control for line printers.
//!
//! Ground truth: `minix3/usr.bin/asa/asa.c`. Each input line's first byte is a
//! vertical-motion control code, consumed and replaced by the corresponding
//! printer movement; the rest of the line is the text to print on that line.
//! `asa()` special-cases the *first* line of a stream (it emits no leading
//! motion for a blank/`+` first line, only the `0`/`1` codes act) and always
//! terminates the stream with one newline once at least one line was read.
//!
//! Control codes for subsequent lines (`asa.c:94-109`): `' '` or any unrecognised
//! byte → a single newline; `'0'` → two newlines (double space); `'1'` → a form
//! feed; `'+'` → a carriage return (overstrike the previous line). A line is
//! emitted verbatim from byte 1 onward only when it has more than one byte and
//! neither the control byte nor the first content byte is NUL (`asa.c:88,111`).
//! Empty input produces no output at all (the first `fgetln` returns NULL).

use alloc::vec::Vec;

/// Split `input` into ASA lines the way `fgetln` hands them to `asa()`: a
/// record per newline-delimited chunk, plus a final short record when the input
/// does not end in a newline. A trailing newline yields no empty record (at EOF
/// `fgetln` returns NULL, not a blank line), and empty input yields none.
fn records(input: &[u8]) -> Vec<&[u8]> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&[u8]> = input.split(|&b| b == b'\n').collect();
    if input.ends_with(b"\n") {
        lines.pop();
    }
    lines
}

/// Emit the printable body of one line: bytes after the control code, but only
/// when the line carries a real control byte and a non-NUL first content byte
/// (the `len > 1 && buf[0] && buf[1]` guard).
fn emit_body(line: &[u8], out: &mut Vec<u8>) {
    if line.len() > 1 && line[0] != 0 && line[1] != 0 {
        out.extend_from_slice(&line[1..]);
    }
}

/// Reformat `input` (ASA carriage control) into printer output bytes.
pub fn asa(input: &[u8]) -> Vec<u8> {
    let lines = records(input);
    if lines.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<u8> = Vec::new();

    // First line: only '0' (newline) and '1' (form feed) emit a leading motion;
    // a blank, '+' or other control byte contributes nothing here.
    match lines[0].first() {
        Some(b'0') => out.push(b'\n'),
        Some(b'1') => out.push(0x0c),
        _ => {}
    }
    emit_body(lines[0], &mut out);

    // Subsequent lines: full control-code table.
    for line in &lines[1..] {
        match line.first() {
            Some(b'0') => {
                out.push(b'\n');
                out.push(b'\n');
            }
            Some(b'1') => out.push(0x0c),
            Some(b'+') => out.push(b'\r'),
            _ => out.push(b'\n'),
        }
        emit_body(line, &mut out);
    }

    out.push(b'\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_str(v: &[u8]) -> &str {
        core::str::from_utf8(v).unwrap()
    }

    #[test]
    fn blank_control_single_spaces() {
        assert_eq!(as_str(&asa(b" hello\n world\n")), "hello\nworld\n");
    }

    #[test]
    fn form_feed_first_line() {
        assert_eq!(as_str(&asa(b"1Title\n")), "\x0cTitle\n");
    }

    #[test]
    fn zero_double_spaces_subsequent_line() {
        assert_eq!(as_str(&asa(b" x\n0y\n")), "x\n\ny\n");
    }

    #[test]
    fn plus_overstrikes_previous_line() {
        assert_eq!(as_str(&asa(b" a\n+b\n")), "a\rb\n");
    }

    #[test]
    fn plus_on_first_line_is_inert() {
        // The first line only reacts to '0'/'1'; '+' produces no leading CR.
        // The following bare "x" line contributes a newline (default control)
        // plus the stream's terminating newline.
        assert_eq!(as_str(&asa(b"+over\nx\n")), "over\n\n");
    }

    #[test]
    fn blank_line_between_content() {
        assert_eq!(as_str(&asa(b" a\n\n b\n")), "a\n\nb\n");
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(asa(b"").is_empty());
    }

    #[test]
    fn single_byte_line_has_no_body() {
        // A lone control byte with no following content prints nothing but its
        // motion; the first line prints no motion for a bare 'h'.
        assert_eq!(as_str(&asa(b"h\n")), "\n");
    }

    #[test]
    fn nul_first_content_byte_suppresses_body() {
        assert_eq!(as_str(&asa(b" \x00abc\n")), "\n");
    }

    #[test]
    fn final_line_without_newline_is_still_processed() {
        assert_eq!(as_str(&asa(b" a\n b")), "a\nb\n");
    }
}
