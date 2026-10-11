//! `soelim` — expand `.so` include directives in n/troff input.
//!
//! Ground truth: `minix3/usr.bin/soelim/soelim.c` (Bill Joy, 1977). The
//! filter reads a character stream and replaces each `.so <name>` line with
//! the contents of `<name>` (searching a directory list), recursively, while
//! passing every other byte through untouched.
//!
//! This module holds the pure byte-level state machine ([`expand`]); the
//! filesystem walk (path search, `fopen`) lives with the caller (the thin
//! bin), injected through the `open` callback. That split mirrors
//! `soelim.c`'s separation of `process()` (the scan) from `openpath()` (the
//! file lookup) and keeps the whole filter host-testable.
//!
//! # Faithful corners of the C scanner
//!
//! `process` reads byte-by-byte. On a `.` it peeks `s` then `o`; a mismatch
//! is not a rewrite — the consumed characters are emitted literally and the
//! scan resumes in "copy to end of line" mode (so `.s` failure prints `.` and
//! the next byte; `.so`-prefix-but-not-`o` prints `.s` and the next byte).
//! After a real `.so`, leading spaces/tabs are skipped, then the filename
//! runs to a space, tab, newline or EOF. That terminating byte is *consumed
//! and dropped* on the recursive path (C `continue`s to a fresh `getc`), but
//! *emitted* on the empty-filename path (C falls through `printf(".so")` into
//! the copy loop). Consequently `.source` is parsed as `.so "urce"` — a real
//! C quirk, pinned by tests.
//!
//! # Deviation: recursion depth cap
//!
//! C has no depth guard, so a `.so` cycle overflows the stack and crashes.
//! [`expand`] bounds recursion at [`MAX_DEPTH`]; on overflow it treats the
//! include as unopenable and emits `.so <name>` instead. This is the one place
//! the port deliberately does not reproduce C (a crash is not behavior worth
//! preserving); the observable output for non-cyclic input is identical.

use alloc::vec::Vec;

/// Maximum `.so` nesting depth before an include is treated as unopenable.
pub const MAX_DEPTH: usize = 32;

/// Copy one line (the byte `c` already read, plus the rest up to and
/// including its newline) to `out`. Models C's `simple:` label.
fn emit_to_eol(input: &[u8], pos: &mut usize, c: u8, out: &mut Vec<u8>) {
    out.push(c);
    if c == b'\n' {
        return;
    }
    let n = input.len();
    while *pos < n {
        let d = input[*pos];
        *pos += 1;
        out.push(d);
        if d == b'\n' {
            break;
        }
    }
}

/// Read the next byte at `pos`, advancing it; `None` at end (C's `EOF`).
fn getc(input: &[u8], pos: &mut usize) -> Option<u8> {
    if *pos < input.len() {
        let x = input[*pos];
        *pos += 1;
        Some(x)
    } else {
        None
    }
}

/// Recursively expand `.so` directives in `input`, appending to `out`.
///
/// `open(name)` returns `Some(bytes)` when `<name>` resolves (its contents)
/// or `None` when it cannot be opened — the caller applies its directory
/// search. On `None` (or depth overflow) the directive is emitted verbatim as
/// `.so <name>\n`, matching C's failure path.
pub fn expand(input: &[u8], out: &mut Vec<u8>, open: &mut dyn FnMut(&str) -> Option<Vec<u8>>) {
    process(input, out, open, 0);
}

fn process(
    input: &[u8],
    out: &mut Vec<u8>,
    open: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    depth: usize,
) {
    let mut pos = 0usize;
    while let Some(c) = getc(input, &mut pos) {
        if c != b'.' {
            emit_to_eol(input, &mut pos, c, out);
            continue;
        }
        // '.' consumed; peek 's'
        match getc(input, &mut pos) {
            Some(b's') => {}
            other => {
                out.push(b'.');
                if let Some(x) = other {
                    emit_to_eol(input, &mut pos, x, out);
                }
                continue;
            }
        }
        // ".s"; peek 'o'
        match getc(input, &mut pos) {
            Some(b'o') => {}
            other => {
                out.extend_from_slice(b".s");
                if let Some(x) = other {
                    emit_to_eol(input, &mut pos, x, out);
                }
                continue;
            }
        }
        // ".so"; skip spaces/tabs
        let mut c = getc(input, &mut pos);
        while let Some(b' ' | b'\t') = c {
            c = getc(input, &mut pos);
        }
        // filename runs to space/tab/newline/EOF (terminator consumed)
        let mut fname: Vec<u8> = Vec::new();
        while let Some(x) = c {
            if x == b' ' || x == b'\t' || x == b'\n' {
                break;
            }
            fname.push(x);
            c = getc(input, &mut pos);
        }
        if fname.is_empty() {
            // ".so" with no name: emit the literal, then copy the terminating
            // byte (the C `goto simple`), if any.
            out.extend_from_slice(b".so");
            if let Some(x) = c {
                emit_to_eol(input, &mut pos, x, out);
            }
            continue;
        }
        // recurse into the named file; on failure emit the directive verbatim
        let resolved = if depth < MAX_DEPTH {
            let name = alloc::string::String::from_utf8_lossy(&fname);
            open(name.as_ref())
        } else {
            None
        };
        match resolved {
            Some(bytes) => process(&bytes, out, open, depth + 1),
            None => {
                out.extend_from_slice(b".so ");
                out.extend_from_slice(&fname);
                out.push(b'\n');
            }
        }
        // The terminator byte (`c`) was consumed and is dropped (C `continue`).
        //
        // If the terminator was a newline we have already crossed it; if it
        // was EOF (c == None) the outer loop's next getc returns None and we
        // break. Either way, fall through to the top.
        continue;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    fn run(input: &[u8], fs: &[(&str, &str)]) -> String {
        let mut table: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (k, v) in fs {
            table.insert((*k).to_string(), v.as_bytes().to_vec());
        }
        let mut out = Vec::new();
        expand(input, &mut out, &mut |name| table.get(name).cloned());
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn passes_through_lines_without_so() {
        assert_eq!(
            run(b"hello\n.world\n", &[]),
            "hello\n.world\n",
            "a '.' not followed by 's' is emitted literally, line copied"
        );
    }

    #[test]
    fn expands_resolved_so() {
        assert_eq!(
            run(b".so a\n", &[("a", "X\n")]),
            "X\n",
            "the .so line is replaced by the included file's bytes"
        );
    }

    #[test]
    fn keeps_directive_when_unopenable() {
        assert_eq!(
            run(b".so missing\n", &[]),
            ".so missing\n",
            "an unopenable include falls back to the literal directive"
        );
    }

    #[test]
    fn so_without_name_keeps_dot_so_plus_terminator() {
        // ".so\n": empty filename -> emit ".so" then copy the newline.
        assert_eq!(run(b".so\n", &[]), ".so\n");
        // ".so" at EOF: emit ".so", no terminator to copy.
        assert_eq!(run(b".so", &[]), ".so");
    }

    #[test]
    fn nested_includes_recursive() {
        assert_eq!(
            run(b".so a\n", &[("a", ".so b\n"), ("b", "B\n")]),
            "B\n",
            "an include whose body is itself a .so recurses"
        );
    }

    #[test]
    fn source_quirk_parses_as_so_with_remainder() {
        // C reads ".so" then the filename runs to whitespace: ".source" ->
        // .so of "urce" (the 'u' immediately follows ".so"). Pinned faithfully.
        assert_eq!(run(b".source\n", &[]), ".so urce\n");
    }

    #[test]
    fn partial_so_prefixes_emit_literally() {
        // "." then non-'s': emit '.' and copy the line from the next byte.
        assert_eq!(run(b".x1\n", &[]), ".x1\n");
        // ".s" then non-'o': emit ".s" and copy from the next byte.
        assert_eq!(run(b".sq\n", &[]), ".sq\n");
    }

    #[test]
    fn leading_whitespace_after_so_is_skipped() {
        assert_eq!(
            run(b".so    a\n", &[("a", "A\n")]),
            "A\n",
            "spaces/tabs between .so and the name are skipped"
        );
    }

    #[test]
    fn depth_cap_breaks_cycles() {
        // a -> .so a forever: the guard stops at MAX_DEPTH and emits the
        // directive rather than overflowing (C would crash).
        let out = run(b".so a\n", &[("a", ".so a\n")]);
        assert!(
            out.contains(".so a\n"),
            "a cycle must terminate by emitting the literal directive"
        );
        assert!(
            out.len() < 64,
            "output must stay bounded, not exponential (len {})",
            out.len()
        );
    }

    #[test]
    fn file_operand_without_trailing_newline() {
        // ".so a" with no newline: name runs to EOF (terminator None).
        assert_eq!(run(b".so a", &[("a", "tail")]), "tail");
    }
}
