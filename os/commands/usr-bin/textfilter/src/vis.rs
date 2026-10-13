//! Visible encoding and decoding for `vis` and `unvis`.
//!
//! Ground truth: `minix3/lib/libc/gen/vis.c` (the vis(3) engine the
//! command drives) and `minix3/usr.bin/unvis/unvis.c` (the decoder).
//! The engine's first act for a byte is a pass-through test
//! (`vis.c:310-312`): a byte escapes only if the extra list names it, and
//! otherwise it survives unchanged when it is graphic, or is whitespace
//! (space, tab, newline), or `VIS_SAFE` is set and it is backspace, bell or
//! carriage return. Everything that fails that test goes through `do_mbyte`:
//! `-c` selects C-style escapes (`\n`, `\r`, `\b`, `\a`, `\v`, `\t`, `\f`, `\s`,
//! `\0`, and `\c` for graphic non-octal bytes, with the C's skip list of letters
//! that must fall through, `vis.c:210-267`); `-o`, an entry on the extra list, or
//! a byte whose low seven bits are a space select three-digit octal; anything
//! else gains a backslash (unless `-b`, `VIS_NOSLASH`) and renders as `^X` for a
//! control (`?` for 0x7F) or `-x` otherwise, with an `M` prefix when the eighth
//! bit is set. Because `makeextralist` puts a backslash on the list unless `-b`
//! removes it (`vis.c:380`), a plain `\` is the one graphic byte the default
//! mode encodes: `\134`. The whitespace and magic-character letters `-s`, `-t`,
//! `-w`, `-S` and `-M` only change that list — see [`VisOptions::extra_set`] —
//! while `-l` ends each line with `\$`. The decoder accepts the same grammar —
//! `M`-meta prefixes, `^X` controls, and backslash-octal runs — which is what the
//! encoder produces.
//!
//! Folding is the other half of the contract: `-f` and `-F` change nothing about how
//! a byte is encoded, only where a line is broken in the text vis has already
//! emitted, which is why [`foldit`] measures encoded bytes and why the command
//! encodes one chunk at a time when folding is on. The two places where
//! `usr.bin/vis/foldit.c` and this shell part company are recorded in
//! `rewrite-notes/18-stage-commands/07-text-filter.md`.
//!
//! Two of the letters select a different encoding instead of a different break:
//! `-h` percent-encodes whatever a URL may not carry verbatim (lowercase hex, RFC
//! 1808), and `-m` quoted-printables whatever RFC 2045 may not (uppercase hex,
//! and a trailing space only when a line ends after it). Both hand the bytes they
//! allow back to the standard engine, so `-e`, `-b` and `-c` keep their meaning
//! under either style; `getvisfun` (`vis.c:338-345`) resolves a request for both
//! styles in favour of HTTP, which the command never lets happen.
//!
//! Two command flags are deliberately not fields of [`VisOptions`]. `-n` is
//! `none` in `minix3/usr.bin/vis/vis.c`, which copies the stream with each
//! backslash doubled and so preserves raw bytes above `0x7F` — bytes a `String`
//! could not carry unchanged; it is its own byte-level entry point, see
//! [`copy_doubling_backslashes`]. `-N` (`VIS_NOLOCALE`) selects the C-locale twin
//! of two engine decisions — `ISGRAPH` using `iscgraph` instead of `iswgraph`
//! (`vis.c:122-124`) and `makeextralist` decoding the `-e` string as multibyte
//! text (`vis.c:361`) — and an encoder that reads bytes and never reassembles
//! them into characters has only the C-locale answer to give. So the flag is
//! accepted and sets nothing; `tests/vis_face.rs` pins that its output equals the
//! default. Bytes above `0x7F` and what that costs a UTF-8 input are recorded in
//! `rewrite-notes/18-stage-commands/07-text-filter.md`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Encoder/decoder configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisOptions {
    /// `-c`: C-style escapes for known characters.
    pub cstyle: bool,
    /// `-o`: three-digit octal for every byte.
    pub octal: bool,
    /// `-b` (`VIS_NOSLASH`): inhibit the backslash prefix. It also keeps the
    /// backslash out of the extra list, which is why `-b` makes a literal
    /// backslash pass through instead of becoming `\134`. C's `-n` is a
    /// different flag; see [`copy_doubling_backslashes`].
    pub no_slash: bool,
    /// `-l` (`markeol`): end each line with `\$`; the backslash itself follows
    /// `-b`, exactly as `usr.bin/vis/vis.c` writes it before the newline.
    pub mark_eol: bool,
    /// `-e`: bytes the user names as backslash-protected. They seed the extra
    /// list that `do_svis` searches (`lib/libc/gen/vis.c:310`:
    /// `iswextra = wcschr(extra, c) != NULL`).
    pub extra: Vec<u8>,
    /// `-s` (`VIS_SAFE`): backspace, bell and carriage return join the
    /// pass-through set, so they are left as they are (`vis.c:312` with
    /// `iswsafe`, whose definition is `vis.c:127`).
    pub safe: bool,
    /// `VIS_SP`: a space goes on the extra list, so it escapes instead of passing
    /// through as whitespace. `vis(1)` has no letter for it alone; `-w` and `-M`
    /// set it (`include/vis.h:49`).
    pub sp: bool,
    /// `-t` (`VIS_TAB`): a tab goes on the extra list.
    pub tab: bool,
    /// `VIS_NL`: a newline goes on the extra list. Like `VIS_SP`, only reachable
    /// through `-w` or `-M` (`include/vis.h:51`).
    pub nl: bool,
    /// `-S` (`VIS_SHELL`): the shell magic characters go on the extra list.
    pub shell: bool,
    /// `VIS_GLOB`: the glob magic characters go on the extra list. `vis(1)` has no
    /// letter for it either — only `-M` (`VIS_META`) pulls it in (`vis.h:67`).
    pub glob: bool,
    /// `-h` (`VIS_HTTPSTYLE`): percent-encoding for everything outside the set RFC
    /// 1808 calls safe (`do_hvis`, `lib/libc/gen/vis.c:161-177`). The C refuses
    /// `-h` together with `-m` (`usr.bin/vis/vis.c:142-144`); if both were ever
    /// handed to the library, `getvisfun` (`vis.c:338-345`) picks HTTP style.
    pub http: bool,
    /// `-m` (`VIS_MIMESTYLE`): quoted-printable, `=` plus *uppercase* hex for the
    /// bytes RFC 2045 will not carry verbatim (`do_mvis`, `vis.c:183-201`). In the
    /// command this flag also moves the fold width from 80 to 76 and swaps the
    /// hidden newline for `=` plus a newline (`vis.c:108-112`, `foldit.c:63`).
    pub mime: bool,
}

/// The shell magic characters C appends to the extra list under `VIS_SHELL`
/// (`lib/libc/gen/vis.c:133`). The backslash in the middle is the same byte the
/// list gets anyway unless `-b` clears that unconditional entry — so a shell mode
/// protects a backslash whether `-b` is set or not.
const CHAR_SHELL: &[u8] = b"'`\";&<>()|{}]\\$!^~";

/// The glob magic characters C appends under `VIS_GLOB` (`vis.c:134`). Only
/// reachable through `-M`, since `vis(1)` has no letter for `VIS_GLOB` alone.
const CHAR_GLOB: &[u8] = b"*?[#";

impl VisOptions {
    /// C's `makeextralist(flags, src)` (`lib/libc/gen/vis.c:351-384`), in its
    /// order: the user's `-e` string, then the glob set for `VIS_GLOB`, the shell
    /// set for `VIS_SHELL`, then the whitespace named one bit at a time by
    /// `VIS_SP`/`VIS_TAB`/`VIS_NL`, and finally the backslash unless
    /// `VIS_NOSLASH` is set (`vis.c:380`).
    pub fn extra_set(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self.extra.clone();
        if self.glob {
            out.extend_from_slice(CHAR_GLOB);
        }
        if self.shell {
            out.extend_from_slice(CHAR_SHELL);
        }
        if self.sp {
            out.push(b' ');
        }
        if self.tab {
            out.push(b'\t');
        }
        if self.nl {
            out.push(b'\n');
        }
        if !self.no_slash {
            out.push(b'\\');
        }
        out
    }
}

/// C's `-n` shape (`vis.c`'s `if (none)` branch): copy every byte through and
/// double each backslash. Kept at the byte level on purpose, because the copy
/// preserves input above 0x7F that a `String` could not carry unchanged.
pub fn copy_doubling_backslashes(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    for &byte in input {
        out.push(byte);
        if byte == b'\\' {
            out.push(b'\\');
        }
    }
    out
}

/// Encodes the input per the options.
pub fn encode(input: &[u8], options: &VisOptions) -> String {
    let mut out = String::new();
    // C builds the extra list once per call (`istrsenvisx:484`) and searches it
    // per character; a membership table is the same test without the scan.
    let extra = extra_membership(&options.extra_set());
    for (index, &byte) in input.iter().enumerate() {
        let next = input.get(index + 1).copied().unwrap_or(0);
        encode_into(byte, next, options, &extra, &mut out);
    }
    out
}

/// The one-byte form of [`encode`]: exactly what `vis(1)` holds in `buff` before
/// it prints, so the folding decision is taken on the same text the C feeds to
/// `foldit()`. The extra list is rebuilt here the way `istrsenvisx:484` rebuilds
/// it per character.
pub fn encode_chunk(byte: u8, next: u8, options: &VisOptions) -> String {
    let extra = extra_membership(&options.extra_set());
    let mut out = String::new();
    encode_into(byte, next, options, &extra, &mut out);
    out
}

/// One slot per byte value, set for the bytes on the extra list — what
/// `wcschr(extra, c) != NULL` asks of `do_svis`.
fn extra_membership(extra: &[u8]) -> [bool; 256] {
    let mut table = [false; 256];
    for &byte in extra {
        table[byte as usize] = true;
    }
    table
}

/// The command's own step for one byte: the end-of-line marker first, because
/// `usr.bin/vis/vis.c:220-227` tests `markeol && c == '\n'` before it calls the
/// library at all, so a marked newline never reaches the pass-through test that
/// would otherwise let it by.
fn encode_into(byte: u8, next: u8, options: &VisOptions, extra: &[bool; 256], out: &mut String) {
    if options.mark_eol && byte == b'\n' {
        if !options.no_slash {
            out.push('\\');
        }
        out.push('$');
        out.push('\n');
        return;
    }
    // `getvisfun` (`lib/libc/gen/vis.c:338-345`) chooses the style in this order:
    // HTTP, then MIME, then the standard test. The command refuses `-h` together
    // with `-m` (`usr.bin/vis/vis.c:142-144`), so at most one of these two branches
    // is live for any run.
    if options.http && !http_carries(byte) {
        push_percent(byte, out);
        return;
    }
    if options.mime && mime_escapes(byte, next) {
        push_mime_hex(byte, out);
        return;
    }
    encode_byte(byte, next, options, extra, out);
}

/// The bytes RFC 1808 lets a URL carry unencoded, as `do_hvis` lists them
/// (`lib/libc/gen/vis.c:164-170`): alphanumerics, then the five characters it
/// calls safe — `$`, `-`, `_`, `.`, `+` — then the six extra ones `!`, `*`, `'`,
/// `(`, `)` and `,`. Everything else goes to percent-encoding, and that set
/// includes space, tab and newline, which the standard style passes through. Bytes
/// that *are* carried still go through `do_svis`, so an `-e` entry protects them
/// here exactly as it does anywhere else.
fn http_carries(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'$' | b'-' | b'_' | b'.' | b'+' | b'!' | b'*' | b'\'' | b'(' | b')' | b','
        )
}

/// Quoted-Printable's escape decision, `do_mvis` (`lib/libc/gen/vis.c:185-197`),
/// read clause by clause: a newline is never encoded (the `c != L'\n'` guard comes
/// first, because the line break is the format's own structure); whitespace is
/// encoded *only* when the next character ends the line, since a trailing space
/// would otherwise be eaten; anything below `!` or above `~` that is not whitespace
/// is out of range and goes; `=` goes, and the C spells that as the range test
/// `c > 60 && c < 62`, which can only ever mean 61; and finally the twelve
/// characters of `vis.c:195` — `#$@[\]^` plus a backtick, brace, vertical bar,
/// brace and tilde — are named outright.
fn mime_escapes(byte: u8, next: u8) -> bool {
    if byte == b'\n' {
        return false;
    }
    let is_space = byte.is_ascii_whitespace();
    (is_space && (next == b'\r' || next == b'\n'))
        || (!is_space && (byte < 33 || byte == b'=' || byte > 126))
        || matches!(
            byte,
            b'#' | b'$' | b'@' | b'[' | b'\\' | b']' | b'^' | b'`' | b'{' | b'|' | b'}' | b'~'
        )
}

/// `vis.c:128-129` keeps two hex tables, `xtoa` lowercase and `XTOA` uppercase,
/// and the two styles do not agree on which one to use: `do_hvis` writes lowercase
/// digits after `%` while `do_mvis` writes uppercase digits after `=`. Preserving
/// that difference is what lets `unvis`-style readers and other implementations
/// recognise the two formats apart, so the tables live side by side here too.
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
const UPPER_HEX: &[u8; 16] = b"0123456789ABCDEF";

fn push_percent(byte: u8, out: &mut String) {
    out.push('%');
    out.push(LOWER_HEX[(byte >> 4) as usize] as char);
    out.push(LOWER_HEX[(byte & 0x0f) as usize] as char);
}

fn push_mime_hex(byte: u8, out: &mut String) {
    out.push('=');
    out.push(UPPER_HEX[(byte >> 4) as usize] as char);
    out.push(UPPER_HEX[(byte & 0x0f) as usize] as char);
}

/// The hidden newline `-f` and `-F` insert, and the same one that closes a partial
/// line at the end of an operand. C picks the pair by flag: `"=\n"` under
/// `VIS_MIMESTYLE`, `"\\\n"` otherwise (`usr.bin/vis/foldit.c:63` and
/// `usr.bin/vis/vis.c:289`).
pub fn fold_marker(mime: bool) -> &'static [u8] {
    if mime { b"=\n" } else { b"\\\n" }
}

/// One column step of C's `foldit()` switch (`usr.bin/vis/foldit.c:47-60`): a
/// newline or carriage return resets the column, a tab advances to the next
/// multiple of eight, a backspace backs up but never below zero, and anything else
/// costs one column. Encoded text is walked, not the input — an escape sequence
/// `\011` occupies three visible columns even though its byte was one.
fn column_step(col: usize, byte: u8) -> usize {
    match byte {
        b'\n' | b'\r' => 0,
        b'\t' => (col + 8) & !7,
        0x08 => col.saturating_sub(1),
        _ => col + 1,
    }
}

/// The column after a whole chunk, with nothing in the way.
fn columns(chunk: &[u8], incoming: usize) -> usize {
    let mut col = incoming;
    for &byte in chunk {
        col = column_step(col, byte);
    }
    col
}

/// The walk alone: `Some(column)` when the chunk stays inside the limit, `None`
/// at the first byte that would pass it.
fn walk(chunk: &[u8], incoming: usize, limit: usize) -> Option<usize> {
    let mut col = incoming;
    for &byte in chunk {
        col = column_step(col, byte);
        if col > limit {
            return None;
        }
    }
    Some(col)
}

/// C's `foldit(chunk, col, max, flags)` (`usr.bin/vis/foldit.c:36-70`), the column
/// bookkeeping behind `-f` and `-F`. `incoming` is the column the previous chunks
/// left — `usr.bin/vis/vis.c` keeps it in a `static int col`, so it survives across
/// chunks *and* across file operands; `max` is 80 under `-f` (76 once `-m` has had
/// its way with it) or the `-F` width. Returns how many [`fold_marker`]s to print
/// *before* the chunk (C calls `printf` inside `foldit`, then the caller emits the
/// chunk) and the column to carry on.
///
/// The break test is `col > max - 2`, two columns of slack so a fold can never
/// leave its own marker hanging past the edge. On a break C prints the marker,
/// zeroes the column and restarts the *same* chunk from its first byte
/// (`goto again`), which is why a chunk that only overflowed because of where the
/// line had got to costs exactly one marker. A chunk wider than `max - 2` can never
/// fit on a line of its own and that restart never terminates: C prints markers
/// forever (`printf '\377' | vis -f -F 5`). This shell stops after the one marker it
/// can actually place and carries the chunk's own width forward; the deviation is
/// recorded in `rewrite-notes/18-stage-commands/07-text-filter.md`.
pub fn foldit(chunk: &[u8], incoming: usize, max: usize) -> (usize, usize) {
    let limit = max.saturating_sub(2);
    if let Some(carried) = walk(chunk, incoming, limit) {
        return (0, carried);
    }
    (
        1,
        walk(chunk, 0, limit).unwrap_or_else(|| columns(chunk, 0)),
    )
}

fn encode_byte(byte: u8, next: u8, options: &VisOptions, extra: &[bool; 256], out: &mut String) {
    // `do_svis`'s one test, extra list and `VIS_SAFE` included
    // (lib/libc/gen/vis.c:310-312). `iswwhite` is space, tab and newline
    // (`vis.c:126`); `iswsafe` is backspace, bell and carriage return (`:127`).
    let iswextra = extra[byte as usize];
    let iswwhite = byte == b' ' || byte == b'\t' || byte == b'\n';
    let iswsafe = byte == 0x08 || byte == 0x07 || byte == 0x0D;
    if !iswextra && (byte.is_ascii_graphic() || iswwhite || (options.safe && iswsafe)) {
        out.push(byte as char);
        return;
    }
    // From here C's order is fixed: the `-c` switch first, then the octal branch
    // that `-e` also feeds, then the M-notation fallback (`vis.c:207-293`). Testing
    // `-o` first instead would flip the answer for any byte that has a named
    // escape and is given both flags: `0x07` would come out `\007` where C writes
    // `\a`.
    if options.cstyle
        && let Some(escape) = cstyle_escape(byte, next)
    {
        out.push_str(&escape);
        return;
    }
    // `(byte & 0o177) == b' '` is C's meta-space case: 0xA0 renders as `\240`
    // rather than as a meta sequence (`vis.c:266-271`).
    if iswextra || (byte & 0o177) == b' ' || options.octal {
        push_octal(byte, out);
        return;
    }
    if !options.no_slash {
        out.push('\\');
    }
    let mut stripped = byte;
    if stripped & 0x80 != 0 {
        stripped &= 0x7F;
        out.push('M');
    }
    if stripped < 0x20 || stripped == 0x7F {
        out.push('^');
        out.push(if stripped == 0x7F {
            '?'
        } else {
            char::from(stripped + 0x40)
        });
    } else {
        out.push('-');
        out.push(char::from(stripped));
    }
}

/// C-style escape for a byte, when the `-c` rules apply to it
/// (vis.c:210-267): named escapes for controls, `\c` for graphic
/// non-octal bytes with the skip list respected, `\0` doubled before
/// an octal digit. `None` means fall through to the default notation.
fn cstyle_escape(byte: u8, next: u8) -> Option<String> {
    let named: Option<&str> = match byte {
        b'\n' => Some("\\n"),
        b'\r' => Some("\\r"),
        0x08 => Some("\\b"),
        0x07 => Some("\\a"),
        0x0B => Some("\\v"),
        b'\t' => Some("\\t"),
        0x0C => Some("\\f"),
        b' ' => Some("\\s"),
        0x00 => Some("\\0"),
        _ => None,
    };
    if let Some(escape) = named {
        // A `\0` before an octal digit doubles the zeros.
        if byte == 0x00 && (0x30..=0x37).contains(&next) {
            return Some("\\000".to_string());
        }
        return Some(escape.to_string());
    }
    // Graphic bytes that are not octal digits take `\c` — except the
    // skip list letters whose escape form would be ambiguous.
    let skip = matches!(
        byte,
        b'n' | b'r' | b'b' | b'a' | b'v' | b't' | b'f' | b's' | b'0' | b'M' | b'^' | b'$'
    );
    if !skip && byte.is_ascii_graphic() && !(b'0'..=b'7').contains(&byte) {
        let mut out = String::new();
        out.push('\\');
        out.push(byte as char);
        return Some(out);
    }
    None
}

fn push_octal(byte: u8, out: &mut String) {
    out.push('\\');
    out.push((b'0' + ((byte >> 6) & 0o3)) as char);
    out.push((b'0' + ((byte >> 3) & 0o7)) as char);
    out.push((b'0' + (byte & 0o7)) as char);
}

/// Decodes visible text back into bytes, mirroring the C `unvis`
/// state machine (lib/libc/gen/unvis.c:217-330). Returns the decoded
/// bytes. Incomplete escapes at the end are dropped (the C's
/// end-of-input flush).
pub fn decode(input: &str) -> Vec<u8> {
    #[derive(Debug, PartialEq, Clone, Copy)]
    enum St {
        Ground,
        Start,
        Meta,
        Meta1,
        Ctrl,
        Octal2,
        Octal3,
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut st = St::Ground;
    let mut value: u8 = 0;
    let mut cursor = 0usize;
    #[cfg(test)]
    eprintln!("DBG decode input={:?} len={}", input, bytes.len());
    while cursor < bytes.len() {
        let c = bytes[cursor];
        cursor += 1;
        #[cfg(test)]
        eprintln!("DBG st={:?} c={:?}", st, c);
        match st {
            St::Ground => {
                if c == b'\\' {
                    st = St::Start;
                } else {
                    out.push(c);
                }
            }
            St::Start => match c {
                b'\\' => {
                    out.push(b'\\');
                    st = St::Ground;
                }
                b'0'..=b'7' => {
                    value = c - b'0';
                    st = St::Octal2;
                }
                b'M' => {
                    value = 0o200;
                    st = St::Meta;
                }
                b'^' => st = St::Ctrl,
                b'n' => {
                    out.push(b'\n');
                    st = St::Ground;
                }
                b'r' => {
                    out.push(b'\r');
                    st = St::Ground;
                }
                b'b' => {
                    out.push(0x08);
                    st = St::Ground;
                }
                b'a' => {
                    out.push(0x07);
                    st = St::Ground;
                }
                b'v' => {
                    out.push(0x0B);
                    st = St::Ground;
                }
                b't' => {
                    out.push(b'\t');
                    st = St::Ground;
                }
                b'f' => {
                    out.push(0x0C);
                    st = St::Ground;
                }
                b's' => {
                    out.push(b' ');
                    st = St::Ground;
                }
                b'E' => {
                    out.push(0x1B);
                    st = St::Ground;
                }
                // A hidden newline or marker emits nothing.
                b'\n' | b'$' => st = St::Ground,
                other if other.is_ascii_graphic() => {
                    out.push(other);
                    st = St::Ground;
                }
                // Non-graphic bytes after a backslash are dropped (the
                // C's default arm requires isgraph).
                _ => st = St::Ground,
            },
            St::Meta => match c {
                b'-' => st = St::Meta1,
                b'^' => st = St::Ctrl,
                _ => {
                    out.push(c | 0o200);
                    st = St::Ground;
                }
            },
            St::Meta1 => {
                out.push(c | 0o200);
                st = St::Ground;
            }
            St::Ctrl => {
                let produced = if c == b'?' { 0x7F } else { c & 0x1F };
                // The pending 0200 from a preceding `M` joins here.
                out.push(value | produced);
                st = St::Ground;
            }
            St::Octal2 => {
                if (b'0'..=b'7').contains(&c) {
                    value = (value << 3) | (c - b'0');
                    st = St::Octal3;
                } else {
                    // Non-digit: the one-digit value emits and the byte
                    // is reprocessed (the C's ungetc arm).
                    out.push(value);
                    cursor -= 1;
                    st = St::Ground;
                }
            }
            St::Octal3 => {
                if (b'0'..=b'7').contains(&c) {
                    value = (value << 3) | (c - b'0');
                    // A completed octal run emits its byte right away
                    // and stays ready for the next one.
                    out.push(value);
                    st = St::Ground;
                } else {
                    out.push(value);
                    cursor -= 1;
                    st = St::Ground;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extra_bytes_skip_the_pass_through_shortcut() {
        // `do_svis` only skips its first test for a byte the `extra` list names
        // (lib/libc/gen/vis.c:310-312), so a printable letter escapes and the
        // others stay as they were.
        let options = VisOptions {
            extra: vec![b'x'],
            ..VisOptions::default()
        };
        assert_eq!(encode(b"axy", &options), "a\\170y");
        // With `-c`, C's switch runs first, so a graphic non-octal non-skip letter
        // takes the short form instead of the octal one.
        let cstyle = VisOptions {
            cstyle: true,
            extra: vec![b'x'],
            ..VisOptions::default()
        };
        assert_eq!(encode(b"xy", &cstyle), "\\xy");
        // C's meta-space rule: `(c & 0177) == ' '` sends 0xA0 to octal, so it
        // never renders as a meta sequence.
        assert_eq!(encode(&[0xA0], &VisOptions::default()), "\\240");
    }

    #[test]
    fn test_mark_eol_writes_a_backslash_dollar_before_the_newline() {
        // `vis.c`'s `else if (markeol && c == '\n')` arm.
        let options = VisOptions {
            mark_eol: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\nb", &options), "a\\$\nb");
        // The marker's own backslash follows `-b` (`VIS_NOSLASH`).
        let without_slash = VisOptions {
            mark_eol: true,
            no_slash: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\nb", &without_slash), "a$\nb");
        // Only newlines are marked. A backslash byte is graphic but sits on the
        // extra list unless `-b` removes it, so the default mode encodes it.
        assert_eq!(encode(b"a\\b", &options), "a\\134b");
    }

    #[test]
    fn test_a_backslash_is_the_one_graphic_byte_the_default_mode_encodes() {
        // `makeextralist` appends `'\\'` whenever `VIS_NOSLASH` is clear
        // (lib/libc/gen/vis.c:380), and `do_svis` consults that list before its
        // graphic test (`vis.c:310-312`), so the byte escapes as octal. Under `-b`
        // it leaves the list and passes through as the graphic byte it is.
        assert_eq!(encode(b"a\\b", &VisOptions::default()), "a\\134b");
        let no_slash = VisOptions {
            no_slash: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\\b", &no_slash), "a\\b");
        // With `-c` the C-style branch runs first and has no case for a graphic
        // byte outside its skip list, so the backslash takes the short `\\` form.
        let cstyle = VisOptions {
            cstyle: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\\b", &cstyle), "a\\\\b");
    }

    #[test]
    fn test_safe_admits_backspace_bell_and_carriage_return() {
        // `VIS_SAFE` only widens the pass-through test (`vis.c:312`), so the three
        // named bytes survive while every other control still escapes.
        let options = VisOptions {
            safe: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"\x08\x07\r", &options), "\u{8}\u{7}\r");
        assert_eq!(encode(b"\x01", &options), "\\^A");
        // Without it the same three bytes are controls, each in its own notation;
        // with `-c` they would have named escapes, which the pass-through
        // preempts because that test runs before the switch.
        assert_eq!(
            encode(b"\x08\x07\r", &VisOptions::default()),
            "\\^H\\^G\\^M"
        );
        let cstyle_safe = VisOptions {
            cstyle: true,
            safe: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"\x08\x07\r", &cstyle_safe), "\u{8}\u{7}\r");
    }

    #[test]
    fn test_whitespace_letters_move_whitespace_onto_the_extra_list() {
        // `-t` sets `VIS_TAB` alone; `-w` sets `VIS_WHITE`, which is the three bits
        // `VIS_SP | VIS_TAB | VIS_NL` (`include/vis.h:49-53`). An extra-list hit
        // goes to the octal branch (`vis.c:266`), so a tab becomes `\011`.
        let tab = VisOptions {
            tab: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\tb", &tab), "a\\011b");
        assert_eq!(encode(b"a b\n", &tab), "a b\n");
        let white = VisOptions {
            sp: true,
            tab: true,
            nl: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b" \t\n", &white), "\\040\\011\\012");
    }

    #[test]
    fn test_shell_and_meta_letters_name_the_magic_characters() {
        // `-S` appends `char_shell` (`vis.c:133`), and `-M` is `VIS_META` =
        // `VIS_WHITE | VIS_GLOB | VIS_SHELL` (`include/vis.h:67`), which also
        // appends `char_glob` (`vis.c:134`). A `$` is shell magic, a `*` is glob
        // magic, and `-S` alone must not touch the glob one.
        let shell = VisOptions {
            shell: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"$", &shell), "\\044");
        assert_eq!(encode(b"*", &shell), "*");
        let meta = VisOptions {
            sp: true,
            tab: true,
            nl: true,
            shell: true,
            glob: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"*$", &meta), "\\052\\044");
        assert_eq!(encode(b" ", &meta), "\\040");
        // `-b` clears only the unconditional entry, and `char_shell` names the same
        // byte, so a shell mode still encodes a backslash.
        let shell_no_slash = VisOptions {
            shell: true,
            no_slash: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\\b", &shell_no_slash), "a\\134b");
        // Every member of the shell set escapes, and nothing outside it does:
        // `vis.c:133` lists eighteen characters, one of them that backslash, and the
        // length is asserted so this enumeration cannot silently lose a member.
        assert_eq!(CHAR_SHELL.len(), 18, "vis.c:133 的点名字母个数");
        assert_eq!(CHAR_GLOB.len(), 4, "vis.c:134 的点名字母个数");
        for byte in CHAR_SHELL {
            assert_eq!(
                encode(&[*byte], &shell),
                format!("\\{byte:03o}"),
                "shell magic {byte:#04x}"
            );
        }
        assert_eq!(encode(b"aZ.", &shell), "aZ.");
    }

    #[test]
    fn test_none_copies_through_and_doubles_each_backslash() {
        // `vis.c`'s `if (none)` arm: the byte is written, and a backslash is
        // followed by a second one so the output still shows exactly one.
        assert_eq!(
            copy_doubling_backslashes(b"a\\b"),
            vec![b'a', b'\\', b'\\', b'b']
        );
        assert_eq!(copy_doubling_backslashes(b"plain"), b"plain".to_vec());
        // Bytes above 0x7F survive unchanged, which is why this path is not a
        // `String` and not a `VisOptions` field.
        assert_eq!(
            copy_doubling_backslashes(&[0x80, 0xff]),
            vec![0x80u8, 0xffu8]
        );
    }

    #[test]
    fn test_graphic_bytes_and_whitespace_pass_through() {
        // do_svis passes graphic bytes and whitespace (space/tab/newline)
        // through before any flag-specific encoding applies
        // (lib/libc/gen/vis.c:311-315).
        assert_eq!(encode(b"abc XYZ\n", &VisOptions::default()), "abc XYZ\n");
    }

    #[test]
    fn test_control_bytes_render_as_caret_notation() {
        assert_eq!(encode(b"\x00", &VisOptions::default()), "\\^@");
        assert_eq!(encode(b"\x1b", &VisOptions::default()), "\\^[");
        assert_eq!(encode(b"\x7f", &VisOptions::default()), "\\^?");
    }

    #[test]
    fn test_high_bit_gains_the_meta_prefix() {
        // 0xFF: M + ^? ; 0xE9 (é): M + - i.
        assert_eq!(encode(b"\xff", &VisOptions::default()), "\\M^?");
        assert_eq!(encode(b"\xe9", &VisOptions::default()), "\\M-i");
    }

    #[test]
    fn test_whitespace_passes_through() {
        // iswwhite = space/tab/newline: all three pass through.
        assert_eq!(encode(b"a b\tc\nd", &VisOptions::default()), "a b\tc\nd");
    }

    #[test]
    fn test_no_slash_drops_the_backslash() {
        let options = VisOptions {
            no_slash: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"\x00", &options), "^@");
    }

    #[test]
    fn test_octal_style_applies_to_encoded_bytes() {
        // Graphic bytes still pass through with `-o`; only encoded bytes
        // (like NUL) take the three-digit octal form.
        let options = VisOptions {
            octal: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a\x00", &options), "a\\000");
    }

    #[test]
    fn test_cstyle_named_escapes() {
        let options = VisOptions {
            cstyle: true,
            ..VisOptions::default()
        };
        // Tab and newline pass through (iswwhite); CR is a control and
        // takes the named escape; NUL takes the zero escape.
        assert_eq!(encode(b"a\tb\nc\rd\0", &options), "a\tb\nc\\rd\\0");
    }

    #[test]
    fn test_cstyle_graphic_passes_through_too() {
        // do_svis passes graphic bytes through BEFORE the cstyle switch
        // runs, so even with `-c` a graphic byte stays itself.
        let options = VisOptions {
            cstyle: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"|", &options), "|");
        assert_eq!(encode(b"t", &options), "t");
    }

    #[test]
    fn test_foldit_breaks_before_the_chunk_that_passes_the_limit() {
        // `usr.bin/vis/foldit.c:61-65` compares against `max - 2`, so with width 6 (limit 4) a
        // chunk is placed while the column stays at four and folds the moment it
        // would become five. The fold costs one marker and the chunk is then
        // counted from column zero again, exactly as C's `goto again` restart does.
        assert_eq!(foldit(b"abcd", 0, 6), (0, 4));
        assert_eq!(foldit(b"a", 3, 6), (0, 4));
        assert_eq!(foldit(b"a", 4, 6), (1, 1));
        // Width 5 is the smallest `-F` accepts, so its limit is three columns.
        assert_eq!(foldit(b"ab", 3, 5), (1, 2));
    }

    #[test]
    fn test_foldit_counts_columns_with_the_tab_backspace_and_newline_rules() {
        // `usr.bin/vis/foldit.c:47-60`: a tab goes to the next multiple of eight, a backspace
        // comes back one column but never below zero, and a newline or carriage
        // return starts the line over.
        assert_eq!(foldit(b"\t", 0, 80), (0, 8));
        assert_eq!(foldit(b"\t", 8, 80), (0, 16));
        assert_eq!(foldit(b"\x08", 5, 80), (0, 4));
        assert_eq!(foldit(b"\x08", 0, 80), (0, 0));
        assert_eq!(foldit(b"abc\ndef", 70, 80), (0, 3));
    }

    #[test]
    fn test_foldit_terminates_where_the_c_would_loop() {
        // A chunk wider than `max - 2` fits on no line, and C's restart-from-zero
        // then repeats forever, printing markers and never the chunk
        // (`printf '\377' | vis -f -F 5`: the chunk is the four characters `\M^?`
        // and the limit is three). This shell places the one marker it can and
        // counts the chunk's own width, so the run finishes.
        assert_eq!(foldit(b"\\M^?", 0, 5), (1, 4));
        assert_eq!(foldit(b"\\M^?", 79, 80), (1, 4));
    }

    #[test]
    fn test_encode_chunk_matches_the_whole_buffer_encode() {
        // The command folds one chunk at a time while the library path encodes the
        // stream in one call; both must give the same bytes, since the chunk form
        // exists only so the fold decision sees the text C would have printed.
        let options = VisOptions {
            mark_eol: true,
            cstyle: true,
            extra: vec![b'x'],
            ..VisOptions::default()
        };
        let input = b"ax\n\x01\xff\\";
        let mut chunks = String::new();
        for (index, &byte) in input.iter().enumerate() {
            let next = input.get(index + 1).copied().unwrap_or(0);
            chunks.push_str(&encode_chunk(byte, next, &options));
        }
        assert_eq!(chunks, encode(input, &options));
    }

    #[test]
    fn test_http_style_percent_encodes_with_lowercase_hex() {
        // `do_hvis` (`lib/libc/gen/vis.c:161-177`) carries alphanumerics, the five
        // safe and the six extra characters, and percent-encodes everything else in
        // *lowercase* hex — including the whitespace the standard style passes
        // through, and the backslash it puts on the extra list.
        let http = VisOptions {
            http: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a b/c?", &http), "a%20b%2fc%3f");
        assert_eq!(encode(b"$-_.+!*'(),", &http), "$-_.+!*'(),");
        assert_eq!(encode(b"\\", &http), "%5c");
        assert_eq!(encode(b"\t\n", &http), "%09%0a");
        assert_eq!(encode(&[0xff], &http), "%ff");
        // A byte it does carry still goes through `do_svis`, so `-e` protects it the
        // same way here as in the default mode.
        let with_extra = VisOptions {
            http: true,
            extra: vec![b'a'],
            ..VisOptions::default()
        };
        assert_eq!(encode(b"ab", &with_extra), "\\141b");
    }

    #[test]
    fn test_mime_style_quoted_printables_with_uppercase_hex() {
        // `do_mvis` (`lib/libc/gen/vis.c:183-201`) is the mirror image with three
        // differences worth naming: the introducer is `=`, the hex digits are
        // *uppercase*, and a space is only escaped when a line ends after it.
        let mime = VisOptions {
            mime: true,
            ..VisOptions::default()
        };
        assert_eq!(encode(b"a b\n", &mime), "a b\n");
        assert_eq!(encode(b"a \nb", &mime), "a=20\nb");
        // Whitespace in the middle of a line is left to the standard engine, so a
        // carriage return that does not end the line comes out as `\^M`; only the
        // one before a line end becomes `=0D`.
        assert_eq!(encode(b"a\rb", &mime), "a\\^Mb");
        assert_eq!(encode(b"a\r\n", &mime), "a=0D\n");
        assert_eq!(encode(&[0xff], &mime), "=FF");
        assert_eq!(encode(b"\x7f", &mime), "=7F");
        assert_eq!(encode(b"\x01", &mime), "=01");
        // A newline is the format's own structure: the `c != L'\n'` guard comes
        // before any of the escape clauses, so it is never encoded. A single newline
        // would pass even without that guard, because the clause behind it only fires
        // on a line end *after* it — two in a row are the case that tells them apart.
        assert_eq!(encode(b"\n", &mime), "\n");
        assert_eq!(encode(b"\n\n", &mime), "\n\n");
        assert_eq!(encode(b"a\r\n\n", &mime), "a=0D\n\n");
        assert_eq!(encode(b"a\t b", &mime), "a\t b");
        // The C spells `=` as the range test `c > 60 && c < 62`, which can only
        // mean 61 — and the twelve named characters follow, with the count asserted
        // so an edit to the table has to be made on purpose.
        assert_eq!(encode(b"=", &mime), "=3D");
        assert_eq!(
            b"#$@[\\]^`{|}~".len(),
            12,
            "do_mvis 的点名字母个数（vis.c:195）"
        );
        for byte in b"#$@[\\]^`{|}~" {
            assert_eq!(
                encode(&[*byte], &mime),
                format!("={byte:02X}"),
                "quoted-printable named {:#04x}",
                byte
            );
        }
    }

    #[test]
    fn test_fold_marker_follows_the_mime_flag() {
        // Two places read the same bit (`usr.bin/vis/foldit.c:63` and
        // `usr.bin/vis/vis.c:289`): MIME breaks a line with `=` and everything else
        // with a backslash.
        assert_eq!(fold_marker(true), b"=\n");
        assert_eq!(fold_marker(false), b"\\\n");
    }

    #[test]
    fn test_decode_reverses_the_encoding() {
        // Every byte round-trips: the backslash that used to break the pair is on
        // the extra list by default, so it encodes as `\134` and the decoder reads
        // the octal run back (lib/libc/gen/unvis.c:246-258).
        for byte in 0x00..=0xFFu8 {
            let options = VisOptions::default();
            let encoded = encode(&[byte], &options);
            let decoded = decode(&encoded);
            assert_eq!(decoded, vec![byte], "byte {:#04x} via {:?}", byte, encoded);
        }
    }

    #[test]
    fn test_decode_octal_runs() {
        assert_eq!(decode("\\101\\010"), vec![0x41, 0x08]);
    }

    #[test]
    fn test_decode_meta_and_caret_need_the_backslash() {
        // unvis only starts an escape on the backslash; a bare "M-^?"
        // is four literal characters (the C ground state emits every
        // non-backslash byte unchanged).
        assert_eq!(decode("M-^?"), b"M-^?");
        // The canonical 0xFF encoding: backslash, M (meta), caret, ?.
        let escaped = "\\M^?".as_bytes();
        assert_eq!(decode(std::str::from_utf8(escaped).unwrap()), vec![0xFF]);
        // The dashed form decodes as two bytes per S_META1.
        assert_eq!(decode("\\M-^?"), vec![0xDE, 0x3F]);
        assert_eq!(decode("\\^I"), vec![0x09]);
    }
}
