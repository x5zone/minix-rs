//! `ed` command execution: one session, one command line at a time.
//!
//! Ground truth: `minix3/bin/ed/main.c` — `exec_command` (line 465, cases
//! from 481), `extract_addr_range` (285), `check_addr_range` (864),
//! `GET_THIRD_ADDR` (391), `GET_COMMAND_SUFFIX` (428), `display_lines`
//! (1242), `put_tty_line` (`io.c:307`), `read_file`/`write_file`
//! (`io.c`), `get_filename` (941), and the main loop (198-280). The parse
//! halves live in [`crate::addr`] and [`crate::cmd`]; this module owns
//! the session state (current line, modified flag, marks, default file
//! name) and the effect of every command letter on it.
//!
//! # Design
//!
//! Execution is a step function over lines: the caller feeds one command
//! line (or one text-input line while an `a`/`i`/`c` is collecting) and
//! receives either a flow decision ([`Flow::Continue`]/[`Flow::Quit`]/
//! [`Flow::QuitModified`]) or an error carrying the C editor's `errmsg`
//! string. All output and file traffic goes through the [`EditorIo`] seam,
//! so the decision logic stays pure (scripted tests drive it without a
//! kernel) and the binary is a thin shell over `minix_sys`.
//!
//! Faithfulness notes (each anchored at its command below): display
//! advances the current line to the last displayed line
//! (`put_tty_line`'s `current_addr = from++`), `d` re-advances with
//! `INC_MOD` (`ed.h:101`), `m`/`t` take a third address and only `m`
//! rejects a destination inside the range, and `wq`/`wQ` quit after a
//! whole-buffer write. Deliberate gaps (substitute `s`, global
//! `g`/`v`/`G`/`V`, undo `u`, shell `!`) answer through the same `?`
//! channel with an explicit "not wired" message instead of pretending.

use crate::addr::{evaluate, evaluate_range, parse_range, AddressRange, Context, MAX_MARKS};
use crate::cmd::{parse_command, Command, Modifiers};
use crate::store::{TextStore, MAX_TEXT};
use crate::EditorError;

/// `GPR`: print the (new) current line after the command (`ed.h:65`).
const GPR: u8 = 0o2;
/// `GLS`: list with visible control characters (`ed.h:66`).
const GLS: u8 = 0o4;
/// `GNP`: enumerate lines as they print (`ed.h:67`).
const GNP: u8 = 0o10;

/// `l` wrap column default (`main.c:1409`; the C editor narrows it from
/// `TIOCGWINSZ` when one answers — hosted runs keep the default).
pub const COLS: usize = 72;
/// Scroll length default (`main.c:460`, `ws_row - 2` on a real tty).
pub const ROWS: i32 = 22;
/// File-name capacity: C `MAXPATHLEN` (`get_filename`: `n - 1 >
/// MAXPATHLEN` is "filename too long", `main.c:958-960`).
pub const MAX_FILENAME: usize = 1024;

/// Why a step could not run. The C editor funnels every failure through
/// one `?` channel plus a saved `errmsg` string (`seterrmsg`); the string
/// is what `h` prints and what `H` mode repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecError {
    /// The saved `errmsg` for this failure.
    pub message: &'static str,
}

/// What the caller should do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Read the next command line.
    Continue,
    /// `q`/`Q`/`wq` accepted: exit 0.
    Quit,
    /// `q` (or plain `e`) with unsaved changes: the main loop prints `?`
    /// plus "warning: file modified" and, in script mode, exits 2
    /// (`main.c:239-249`); interactively it clears the flag and goes on.
    QuitModified,
}

/// Output and file effects of one step, injected so the decision logic
/// stays testable. `minix_sys` provides the production half; tests script
/// this trait.
pub trait EditorIo {
    /// Standard output (`put_tty_line`'s `putchar` stream, `io.c:307`).
    fn emit(&mut self, bytes: &[u8]);
    /// Standard error (`?`, warning messages, byte counts).
    fn emit_err(&mut self, bytes: &[u8]);
    /// Read a whole file into `out`; returns the byte count. A file that
    /// does not fit is [`EditorError::TooLong`] (the store could not hold
    /// it either — same capacity discipline).
    fn read_file(&mut self, name: &str, out: &mut [u8]) -> Result<usize, EditorError>;
    /// Write `data` to a file (append when asked); returns bytes written.
    fn write_file(&mut self, name: &str, data: &[u8], append: bool)
    -> Result<usize, EditorError>;
}

/// The prompt text (`-p string`, default `*`): a fixed buffer because the
/// session is a fixed-memory type like the rest of the crate.
#[derive(Debug, Clone, Copy)]
struct Prompt {
    len: usize,
    bytes: [u8; 32],
}

impl Prompt {
    fn star() -> Self {
        let mut bytes = [0; 32];
        bytes[0] = b'*';
        Prompt { len: 1, bytes }
    }

    fn from(text: &str) -> Result<Self, EditorError> {
        let bytes = text.as_bytes();
        if bytes.len() > 32 {
            return Err(EditorError::TooLong);
        }
        let mut buf = [0; 32];
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(Prompt { len: bytes.len(), bytes: buf })
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Session state: the C globals of `main.c:90-110` plus the pending
/// text-input position for `a`/`i`/`c`.
pub struct Session {
    /// `current_addr`: 0 only while the buffer is empty.
    pub current: usize,
    /// `modified`: unsaved changes (`q` refuses, `e` refuses softly).
    pub modified: bool,
    /// `scripted` (`-s`): suppress prompts and byte counts, quit on
    /// warnings instead of continuing.
    pub scripted: bool,
    /// `secure` (`-S`): shell access rejected (`main.c:141`).
    pub secure: bool,
    /// `red`: invoked under a name whose third-to-last byte is `r`
    /// (`main.c:118`) — file names may not contain `/` or `..`
    /// (`is_legal_filename`).
    pub restricted: bool,
    /// `garrulous` (`H`): explain every error.
    pub garrulous: bool,
    /// Whether the prompt prints before each command (`P` toggles).
    pub prompt_on: bool,
    /// `old_filename`: the default file name (`main.c:100`).
    pub filename_len: usize,
    pub filename: [u8; MAX_FILENAME + 1],
    /// Line marks (`'a`..`'z`). Numbers follow lines across inserts and
    /// deletes above them; a mark inside a deleted range is dropped. C
    /// hangs marks on line nodes, so marks there also survive moves —
    /// declared deviation, see 09-editors.md §5.
    pub marks: [Option<usize>; MAX_MARKS],
    /// The saved `errmsg` (`seterrmsg`), for `h`/`H`.
    pub error_msg: Option<&'static str>,
    /// While collecting text for `a`/`i`/`c`: the insert-before position
    /// of the next input line.
    pending_input: Option<usize>,
    /// The suffix `gflag` of the command that opened text-input mode; the
    /// post-input display uses it (`exec_command` returns it, and the
    /// input lines are consumed inside the same call in C).
    pending_gflag: u8,
    prompt: Prompt,
    /// `-p` operand (`main.c:124`); `P` re-arms from it.
    opt_prompt: Option<Prompt>,
}

impl Session {
    /// A fresh session with the command-line flags applied (`-s`, `-S`,
    /// `-p string`); `restricted` mirrors invocation as `red`.
    pub fn new(scripted: bool, secure: bool, restricted: bool, opt_prompt: Option<&str>) -> Self {
        Session {
            current: 0,
            modified: false,
            scripted,
            secure,
            restricted,
            garrulous: false,
            prompt_on: false,
            filename_len: 0,
            filename: [0; MAX_FILENAME + 1],
            marks: [None; MAX_MARKS],
            error_msg: None,
            pending_input: None,
            pending_gflag: 0,
            prompt: Prompt::star(),
            opt_prompt: opt_prompt.and_then(|text| Prompt::from(text).ok()),
        }
    }

    /// The default file name as text, if one is set.
    pub fn filename_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.filename[..self.filename_len]).ok()
    }

    /// The `old_filename` setter (`strlcpy(old_filename, fnp, ...)` at
    /// `main.c:189`, `:548`, `:794`): `e`/`f`/`r`/`w` maintain it and the
    /// binary's initial-file block sets it once.
    pub fn set_filename(&mut self, text: &str) -> Result<(), ExecError> {
        let bytes = text.as_bytes();
        if bytes.len() > MAX_FILENAME {
            return Err(err("filename too long"));
        }
        self.filename_len = bytes.len();
        self.filename[..bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    /// The prompt bytes to print (`P` re-arms from the `-p` operand, or
    /// `*` when there was none — `main.c:668-673`).
    pub fn prompt_bytes(&self) -> &[u8] {
        self.prompt.as_slice()
    }

    fn toggle_prompt(&mut self) {
        if self.prompt_on {
            self.prompt_on = false;
        } else {
            self.prompt = self.opt_prompt.unwrap_or(Prompt::star());
            self.prompt_on = true;
        }
    }
}

fn err(message: &'static str) -> ExecError {
    ExecError { message }
}

fn context_of<S: TextStore>(store: &S, sess: &Session) -> Context {
    Context { line_count: store.line_count(), current: sess.current, marks: sess.marks }
}

/// The suffix bits (`GPR`/`GLS`/`GNP`) of a parsed modifier set. A `!`
/// never survives: the C suffix macro only accepts `p`/`l`/`n` and calls
/// anything else an invalid suffix (`main.c:428-448`).
fn suffix_bits(modifiers: &Modifiers) -> Result<u8, ExecError> {
    if modifiers.force {
        return Err(err("invalid command suffix"));
    }
    let mut g = 0;
    if modifiers.print {
        g |= GPR;
    }
    if modifiers.list {
        g |= GLS;
    }
    if modifiers.number {
        g |= GNP;
    }
    Ok(g)
}

/// Scan a trailing `pln` suffix by hand (for the commands whose letter is
/// followed by a parameter first: `k`, `m`/`t`, `z`). Same grammar as
/// `GET_COMMAND_SUFFIX` (`main.c:428-448`): the run, then end of line.
fn suffix_scan(rest: &str) -> Result<u8, ExecError> {
    let bytes = rest.as_bytes();
    let mut at = 0;
    let mut g = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'p' => g |= GPR,
            b'l' => g |= GLS,
            b'n' => g |= GNP,
            _ => break,
        }
        at += 1;
    }
    if at != bytes.len() {
        return Err(err("invalid command suffix"));
    }
    Ok(g)
}

/// `w`/`e`/`E`/`r`/`f` read a file name right after the letter, so any
/// glued `p`/`l`/`n`/`!` (beyond `wq`) is a character the C parser would
/// have fed to its space check — always "unexpected command suffix"
/// (`main.c:508`, `:545`, `:780`, `:946`).
fn reject_glued(modifiers: &Modifiers, allow_quit: bool) -> Result<(), ExecError> {
    let noisy = modifiers.force || modifiers.print || modifiers.list || modifiers.number;
    if noisy || (modifiers.quit_after && !allow_quit) {
        return Err(err("unexpected command suffix"));
    }
    Ok(())
}

fn marks_shift_up(sess: &mut Session, at: usize, lines: usize) {
    for mark in sess.marks.iter_mut() {
        if let Some(line) = *mark
            && line >= at
        {
            *mark = Some(line + lines);
        }
    }
}

fn marks_delete(sess: &mut Session, from: usize, to: usize) {
    let span = to - from + 1;
    for mark in sess.marks.iter_mut() {
        *mark = match *mark {
            Some(line) if line < from => Some(line),
            Some(line) if line <= to => None,
            Some(line) => Some(line - span),
            None => None,
        };
    }
}

/// One inserted line: the store call plus the session bookkeeping C does
/// inside `put_sbuf_line` (`main.c:1129` — every insert marks the buffer
/// modified).
fn insert_line<S: TextStore>(
    store: &mut S,
    sess: &mut Session,
    pos: usize,
    text: &str,
) -> Result<(), ExecError> {
    store.insert(pos, text).map_err(map_store_error)?;
    marks_shift_up(sess, pos, 1);
    sess.modified = true;
    Ok(())
}

/// A deleted range: the store call plus `delete_lines`' bookkeeping
/// (`main.c:1233-1235` — current drops to `from - 1`, buffer modified).
fn delete_range<S: TextStore>(
    store: &mut S,
    sess: &mut Session,
    from: usize,
    to: usize,
) -> Result<(), ExecError> {
    store.delete(from, to).map_err(map_store_error)?;
    marks_delete(sess, from, to);
    sess.current = from - 1;
    sess.modified = true;
    Ok(())
}

fn map_store_error(e: EditorError) -> ExecError {
    match e {
        // The C editor's buffer-exhaustion face (`io.c` sbuf full).
        EditorError::TooLong => err("out of memory"),
        EditorError::InvalidArgument => err("invalid address"),
    }
}

/// `display_lines` + `put_tty_line` (`main.c:1242`, `io.c:307`). Display
/// advances the current line: `put_tty_line` is called with
/// `current_addr = from++`, so the last displayed line stays current.
fn display<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    from: usize,
    to: usize,
    gflag: u8,
) -> Result<(), ExecError> {
    if from == 0 {
        return Err(err("invalid address"));
    }
    let mut buf = [0u8; MAX_TEXT];
    let mut n = from;
    while n <= to {
        let len = store.read_line(n, &mut buf).map_err(map_store_error)?;
        put_tty_line(io, &buf[..len], n, gflag);
        sess.current = n;
        n += 1;
    }
    Ok(())
}

fn put_tty_line<I: EditorIo>(io: &mut I, line: &[u8], number: usize, gflag: u8) {
    let listing = gflag & GLS != 0;
    let mut col = 0usize;
    if gflag & GNP != 0 {
        // C `printf("%ld\t", n)` (`io.c:315`).
        let mut digits = [0u8; 20];
        let used = write_decimal(number as u64, &mut digits);
        io.emit(&digits[..used]);
        io.emit(b"\t");
        col = 8;
    }
    for &byte in line {
        if listing {
            col += 1;
            if col > COLS {
                io.emit(b"\\\n");
                col = 1;
            }
        }
        if !listing || ((0x20..0x7f).contains(&byte) && byte != b'\\') {
            io.emit(&[byte]);
        } else {
            // `ESCAPES "\a\b\f\n\r\t\v\\"` maps to `ESCCHARS "abfnrtv\\"`
            // (`io.c:302-303`); anything else is three octal digits
            // (`io.c:342-345`).
            io.emit(b"\\");
            let named = match byte {
                0x07 => Some(b'a'),
                0x08 => Some(b'b'),
                0x0c => Some(b'f'),
                0x0a => Some(b'n'),
                0x0d => Some(b'r'),
                0x09 => Some(b't'),
                0x0b => Some(b'v'),
                b'\\' => Some(b'\\'),
                _ => None,
            };
            match named {
                Some(c) => io.emit(&[c]),
                None => {
                    let oct = [
                        ((byte & 0o300) >> 6) + b'0',
                        ((byte & 0o070) >> 3) + b'0',
                        (byte & 0o007) + b'0',
                    ];
                    io.emit(&oct);
                    col += 2;
                }
            }
        }
    }
    if listing {
        io.emit(b"$");
    }
    io.emit(b"\n");
}

/// Decimal writer for the `%ld` faces (line numbers, byte counts).
fn write_decimal(mut value: u64, out: &mut [u8]) -> usize {
    if value == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut at = out.len();
    while value > 0 && at > 0 {
        at -= 1;
        out[at] = (value % 10) as u8 + b'0';
        value /= 10;
    }
    let used = out.len() - at;
    out.copy_within(at.., 0);
    used
}

/// The main-loop tail: a nonzero suffix `gflag` prints the (new) current
/// line (`main.c:228-233`).
fn finish<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    gflag: u8,
) -> Result<Flow, ExecError> {
    if gflag != 0 {
        display(store, sess, io, sess.current, sess.current, gflag)?;
    }
    Ok(Flow::Continue)
}

/// A file-name operand, unescaped into the caller's buffer.
enum FileName {
    /// Nothing typed: use `old_filename` (error when unset).
    Default,
    /// Bytes `taken[..len]` hold the operand.
    Fresh(usize),
}

/// `get_filename` minus the shell branch (`main.c:941-984`): a glued
/// non-space is "unexpected command suffix", blanks then end of line is
/// "invalid filename", backslashes escape the next byte, `!` reaches for
/// a shell. The whole tail after the blanks is the name (the C reader
/// runs to end of line), so no suffix can follow a file name.
fn take_filename(rest: &str, sess: &Session, taken: &mut [u8]) -> Result<FileName, ExecError> {
    let bytes = rest.as_bytes();
    if bytes.is_empty() {
        return Ok(FileName::Default);
    }
    // `if (!isspace(*ibufp)) → unexpected command suffix` — the caller
    // strips the trailing newline, so blanks-then-end is the only blank
    // shape that reaches here.
    if bytes[0] != b' ' && bytes[0] != b'\t' {
        return Err(err("unexpected command suffix"));
    }
    let mut at = 0;
    while at < bytes.len() && (bytes[at] == b' ' || bytes[at] == b'\t') {
        at += 1;
    }
    if at >= bytes.len() {
        // `SKIP_BLANKS` ran into the newline (`main.c:947-950`).
        return Err(err("invalid filename"));
    }
    if bytes[at] == b'!' {
        if sess.secure || sess.restricted {
            return Err(err("shell access restricted"));
        }
        return Err(err("shell access not wired"));
    }
    let mut len = 0;
    let mut scan = at;
    while scan < bytes.len() {
        let b = bytes[scan];
        if b == b'\\' {
            // `strip_escapes` (`main.c`): a backslash quotes one byte.
            scan += 1;
            if scan >= bytes.len() {
                break;
            }
        }
        if len >= MAX_FILENAME || len >= taken.len() {
            return Err(err("filename too long"));
        }
        taken[len] = bytes[scan];
        len += 1;
        scan += 1;
    }
    // `is_legal_filename` under `red`: no leading `!`, no `..`, no `/`.
    if sess.restricted
        && (len == 0 || out_starts_bang(taken, len) || &taken[..len] == b".." || taken[..len].contains(&b'/'))
    {
        return Err(err("shell access restricted"));
    }
    if len == 0 {
        return Ok(FileName::Default);
    }
    Ok(FileName::Fresh(len))
}

fn out_starts_bang(taken: &[u8], len: usize) -> bool {
    len > 0 && taken[0] == b'!'
}

/// Resolve an operand (or the default name) into `name_out`, so the
/// borrow does not pin the session while the store mutates. The store
/// world is `&str`: non-UTF-8 names are rejected through the same `?`
/// channel as every other failure.
fn resolve_name<'a>(
    sess: &Session,
    kind: FileName,
    taken: &[u8],
    name_out: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let source: &[u8] = match kind {
        FileName::Default => {
            if sess.filename_len == 0 {
                return Err(err("no current filename"));
            }
            &sess.filename[..sess.filename_len]
        }
        FileName::Fresh(len) => &taken[..len],
    };
    if source.len() > name_out.len() {
        return Err(err("filename too long"));
    }
    name_out[..source.len()].copy_from_slice(source);
    core::str::from_utf8(&name_out[..source.len()]).map_err(|_| err("invalid content"))
}

/// The `r`/`e` read target: the typed name, or the just-maintained
/// default (`*fnp ? fnp : old_filename`, `main.c:534`/`:797`).
fn target_name<'a>(
    sess: &Session,
    typed: &str,
    buf: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let source: &[u8] = if typed.is_empty() {
        &sess.filename[..sess.filename_len]
    } else {
        typed.as_bytes()
    };
    if source.len() > buf.len() {
        return Err(err("filename too long"));
    }
    buf[..source.len()].copy_from_slice(source);
    core::str::from_utf8(&buf[..source.len()]).map_err(|_| err("invalid content"))
}

/// Split `text` into lines the way the store holds them (newline
/// terminated; a final piece without one still counts).
struct LinesOf<'a> {
    rest: &'a str,
}

impl<'a> Iterator for LinesOf<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        if self.rest.is_empty() {
            return None;
        }
        match self.rest.find('\n') {
            Some(at) => {
                let piece = &self.rest[..at];
                self.rest = &self.rest[at + 1..];
                Some(piece)
            }
            None => {
                let piece = self.rest;
                self.rest = "";
                Some(piece)
            }
        }
    }
}

/// Read lines `from..=to` into `buf` with no separators at all — the `j`
/// face (`join_lines`' newline glue would just re-insert the range).
fn concat_lines<'a, S: TextStore>(
    store: &S,
    from: usize,
    to: usize,
    buf: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let mut len = 0;
    let mut n = from;
    while n <= to {
        let mut line = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut line).map_err(map_store_error)?;
        if len + used > buf.len() {
            return Err(err("out of memory"));
        }
        buf[len..len + used].copy_from_slice(&line[..used]);
        len += used;
        n += 1;
    }
    core::str::from_utf8(&buf[..len]).map_err(|_| err("invalid content"))
}

/// Read lines `from..=to` into `buf`, newline separated, as text.
fn join_lines<'a, S: TextStore>(
    store: &S,
    from: usize,
    to: usize,
    buf: &'a mut [u8],
) -> Result<&'a str, ExecError> {
    let mut len = 0;
    let mut n = from;
    while n <= to {
        let mut line = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut line).map_err(map_store_error)?;
        if len + used + 1 > buf.len() {
            return Err(err("out of memory"));
        }
        buf[len..len + used].copy_from_slice(&line[..used]);
        len += used;
        buf[len] = b'\n';
        len += 1;
        n += 1;
    }
    core::str::from_utf8(&buf[..len]).map_err(|_| err("invalid content"))
}

fn ensure_line_end(rest: &str) -> Result<(), ExecError> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(err("invalid command suffix"))
    }
}

fn parse_range_err(line: &str) -> Result<(AddressRange, usize), ExecError> {
    parse_range(line).map_err(|_| err("invalid address"))
}

/// Execute one command line (or one text-input line while collecting).
///
/// `line` carries no trailing newline (the caller strips it, matching the
/// parse halves). Errors set the session's saved message (`seterrmsg`) —
/// `h` and `H` read it back — and the caller prints `?`.
pub fn step<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    line: &str,
    io: &mut I,
) -> Result<Flow, ExecError> {
    let result = step_inner(store, sess, line, io);
    if let Err(e) = &result {
        sess.error_msg = Some(e.message);
    }
    result
}

fn step_inner<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    line: &str,
    io: &mut I,
) -> Result<Flow, ExecError> {
    // Text-input mode: every line is literal text until a lone `.`.
    if let Some(pos) = sess.pending_input {
        if line == "." {
            sess.pending_input = None;
            let g = sess.pending_gflag;
            sess.pending_gflag = 0;
            return finish(store, sess, io, g);
        }
        insert_line(store, sess, pos, line)?;
        sess.current = pos;
        sess.pending_input = Some(pos + 1);
        return Ok(Flow::Continue);
    }

    let (range, consumed) = parse_range_err(line)?;
    let rest = &line[consumed..];
    if rest.is_empty() {
        // A bare address (or bare newline): display the second address,
        // defaulting to the line after current (`main.c:884-890`,
        // `check_addr_range(1, current_addr + 1)`). Display moves current
        // to that line, which is how a lone `3` navigates.
        let (_, to) = evaluate_range(&range, &context_of(store, sess), (1, sess.current + 1))
            .map_err(|_| err("invalid address"))?;
        display(store, sess, io, to, to, 0)?;
        return Ok(Flow::Continue);
    }

    let (command, modifiers, cursor) =
        parse_command(line, consumed).map_err(|_| err("unknown command"))?;
    let ctx = context_of(store, sess);
    let count = store.line_count();
    // Position right after the command letter, for the commands whose
    // parameter starts there (`k`, `m`/`t`, `z`, file names).
    let letter_end = consumed + 1;

    match command {
        Command::Append => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            let (_, second) = evaluate_range(&range, &ctx, (sess.current, sess.current))
                .map_err(|_| err("invalid address"))?;
            sess.pending_input = Some(second + 1);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Insert => {
            let (_, second) = evaluate_range(&range, &ctx, (sess.current, sess.current))
                .map_err(|_| err("invalid address"))?;
            if second == 0 {
                return Err(err("invalid address"));
            }
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            sess.pending_input = Some(second);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Change => {
            let (from, to) = evaluate_range(&range, &ctx, (sess.current, sess.current))
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            delete_range(store, sess, from, to)?;
            sess.pending_input = Some(from);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Delete => {
            let (from, to) = evaluate_range(&range, &ctx, (sess.current, sess.current))
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            delete_range(store, sess, from, to)?;
            // `INC_MOD(current_addr, addr_last)` (`ed.h:101`) then
            // `if (addr != 0)`: slide to the line after the deleted block
            // while one exists (`main.c:498-502`).
            let next = sess.current + 1;
            if next <= store.line_count() {
                sess.current = next;
            }
            finish(store, sess, io, g)
        }
        Command::Print | Command::List | Command::Number => {
            let (from, to) = evaluate_range(&range, &ctx, (sess.current, sess.current))
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            let bits = match command {
                Command::Print => GPR,
                Command::List => GLS,
                _ => GNP,
            } | g;
            display(store, sess, io, from, to, bits)?;
            Ok(Flow::Continue)
        }
        Command::LineNumber => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            // `printf("%ld\n", addr_cnt ? second_addr : addr_last)`
            // (`main.c:866-871`).
            let number = if range.first.is_some() || range.second.is_some() {
                evaluate(range.second.or(range.first).ok_or_else(|| err("invalid address"))?, &ctx)
                    .map_err(|_| err("invalid address"))?
            } else {
                store.line_count()
            };
            let mut digits = [0u8; 20];
            let used = write_decimal(number as u64, &mut digits);
            io.emit(&digits[..used]);
            io.emit(b"\n");
            finish(store, sess, io, g)
        }
        Command::Quit => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if sess.modified && !sess.scripted {
                // C returns EMOD; the main loop prints `?` plus the
                // warning, quits in script mode, and otherwise clears the
                // flag and carries on (`main.c:239-249`).
                sess.error_msg = Some("warning: file modified");
                return Ok(Flow::QuitModified);
            }
            let _ = g;
            Ok(Flow::Quit)
        }
        Command::Edit | Command::EditForce => {
            if consumed > 0 {
                return Err(err("unexpected address"));
            }
            reject_glued(&modifiers, false)?;
            // Plain `e` refuses unsaved changes softly (`main.c:506-509`);
            // the main loop's EMOD branch clears the flag interactively.
            if command == Command::Edit && sess.modified && !sess.scripted {
                sess.error_msg = Some("warning: file modified");
                return Ok(Flow::QuitModified);
            }
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            let typed_len = typed.len();
            if typed_len > 0 {
                sess.set_filename(typed)?;
            }
            let mut target_buf = [0u8; MAX_FILENAME + 1];
            let target = target_name(sess, typed, &mut target_buf)?;
            if count >= 1 {
                delete_range(store, sess, 1, count)?;
            }
            sess.marks = [None; MAX_MARKS];
            let last = read_into_store(store, sess, io, target, 0)?;
            sess.current = last;
            sess.modified = false;
            Ok(Flow::Continue)
        }
        Command::Read => {
            reject_glued(&modifiers, false)?;
            let (_, second) = evaluate_range(&range, &ctx, (sess.current, count))
                .map_err(|_| err("invalid address"))?;
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            if !typed.is_empty() && sess.filename_len == 0 {
                // `if (*old_filename == '\0' && *fnp != '!')`
                // (`main.c:792-794`): the first `r` names the file.
                sess.set_filename(typed)?;
            }
            let mut target_buf = [0u8; MAX_FILENAME + 1];
            let target = target_name(sess, typed, &mut target_buf)?;
            // C inserts after `second_addr` and leaves current on the last
            // inserted line (`read_stream`'s add loop).
            let last = read_into_store(store, sess, io, target, second)?;
            sess.current = last;
            Ok(Flow::Continue)
        }
        Command::Write | Command::WriteAppend => {
            // `wq`/`wQ` glue is legal (`main.c:804-807`); any other glued
            // modifier would have been a file-name character in C.
            reject_glued(&modifiers, true)?;
            let (from, to) = if count == 0 {
                (0, 0)
            } else {
                evaluate_range(&range, &ctx, (1, count)).map_err(|_| err("invalid address"))?
            };
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            if !typed.is_empty() && sess.filename_len == 0 {
                sess.set_filename(typed)?;
            }
            let mut target_buf = [0u8; MAX_FILENAME + 1];
            let target = target_name(sess, typed, &mut target_buf)?;
            let mut data = [0u8; MAX_TEXT];
            let mut len = 0;
            let mut n = from;
            while n <= to {
                let mut line = [0u8; MAX_TEXT];
                let used = store.read_line(n, &mut line).map_err(map_store_error)?;
                if len + used + 1 > data.len() {
                    return Err(err("out of memory"));
                }
                data[len..len + used].copy_from_slice(&line[..used]);
                len += used;
                data[len] = b'\n';
                len += 1;
                n += 1;
            }
            let written = io
                .write_file(target, &data[..len], command == Command::WriteAppend)
                .map_err(|e| match e {
                    EditorError::TooLong => err("out of memory"),
                    EditorError::InvalidArgument => err("cannot open output file"),
                })?;
            if !sess.scripted {
                let mut digits = [0u8; 20];
                let used = write_decimal(written as u64, &mut digits);
                io.emit_err(&digits[..used]);
                io.emit_err(b"\n");
            }
            // `else if (addr == addr_last) modified = 0` (`main.c:811-812`):
            // a whole-buffer write clears the flag. `addr` there is the
            // line count written (`m - n + 1`, `io.c` write_file tail).
            let lines_written = to.saturating_sub(from) + 1;
            let whole = count == 0 || lines_written == count;
            if whole {
                sess.modified = false;
            }
            if modifiers.quit_after {
                // `wq` on a partial write keeps the EMOD dance
                // (`main.c:813-815`): warn interactively, quit in script.
                if sess.modified && !sess.scripted {
                    sess.error_msg = Some("warning: file modified");
                    return Ok(Flow::QuitModified);
                }
                return Ok(Flow::Quit);
            }
            Ok(Flow::Continue)
        }
        Command::Filename => {
            // `main.c:541-556`: an address is rejected, a glued modifier
            // would have been the name's first character, and the current
            // name prints whether or not a new one was typed.
            if consumed > 0 {
                return Err(err("unexpected address"));
            }
            reject_glued(&modifiers, false)?;
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            if !typed.is_empty() {
                sess.set_filename(typed)?;
            }
            let name = sess.filename_str().unwrap_or("");
            io.emit(name.as_bytes());
            io.emit(b"\n");
            Ok(Flow::Continue)
        }
        Command::Mark => {
            // `k` reads its mark letter unconditionally (`main.c:618-620`),
            // then the usual suffix check applies.
            let mark = line.as_bytes().get(letter_end).copied();
            let g = suffix_scan(&line[letter_end + mark.is_some() as usize..])?;
            let (_, second) = evaluate_range(&range, &ctx, (sess.current, sess.current))
                .map_err(|_| err("invalid address"))?;
            if second == 0 {
                return Err(err("invalid address"));
            }
            if let Some(c @ b'a'..=b'z') = mark {
                sess.marks[(c - b'a') as usize] = Some(second);
            }
            finish(store, sess, io, g)
        }
        Command::Move | Command::Transfer => {
            let (from, to) = evaluate_range(&range, &ctx, (sess.current, sess.current))
                .map_err(|_| err("invalid address"))?;
            // `GET_THIRD_ADDR` (`main.c:391-407`): the destination is the
            // second address of a fresh extraction; none is "destination
            // expected", beyond the last line is "invalid address".
            let (third, used) = parse_range_err(&line[letter_end..])?;
            let Some(dest_spec) = third.second.or(third.first) else {
                return Err(err("destination expected"));
            };
            // `GET_THIRD_ADDR` (`main.c:401-404`) rejects only negative
            // and beyond-last, so `m0`/`t0` (move to the front) are legal
            // even though a command address may not be zero.
            let dest = match dest_spec.base {
                crate::addr::Base::Number(0) if dest_spec.offset == 0 => 0,
                _ => evaluate(dest_spec, &ctx).map_err(|_| err("invalid address"))?,
            };
            if dest > count {
                return Err(err("invalid address"));
            }
            let g = suffix_scan(&line[letter_end + used..])?;
            let len = to - from + 1;
            let mut buf = [0u8; MAX_TEXT];
            let joined = join_lines(store, from, to, &mut buf)?;
            if command == Command::Move {
                if dest + 1 == from || dest == to {
                    // `move_lines`' no-op shape (`main.c:1141`): the block
                    // already sits where it is asked to go; current moves
                    // to the second address (`main.c:1144`).
                    sess.current = to;
                    return finish(store, sess, io, g);
                }
                if from <= dest && dest < to {
                    return Err(err("invalid destination"));
                }
                delete_range(store, sess, from, to)?;
                let before = if dest < from { dest + 1 } else { dest + 1 - len };
                store.insert(before, joined).map_err(map_store_error)?;
                marks_shift_up(sess, before, len);
                sess.modified = true;
                // `current_addr = addr + (addr < first ? len : 0)`
                // (`main.c:1167-1169`): the last line of the moved block.
                sess.current = before - 1 + len;
            } else {
                store.insert(dest + 1, joined).map_err(map_store_error)?;
                marks_shift_up(sess, dest + 1, len);
                sess.modified = true;
                // The copies end at `dest + len` (copy_lines walks its
                // duplicates to the end, `main.c:1180-1215`).
                sess.current = dest + len;
            }
            finish(store, sess, io, g)
        }
        Command::Join => {
            let (from, to) = evaluate_range(&range, &ctx, (sess.current, sess.current + 1))
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if from != to {
                // `join_lines` glues with newlines (`add_line_node` per
                // line), but `j` needs ONE line: no separators at all
                // (`main.c:602-610`), then a single insert.
                let mut buf = [0u8; MAX_TEXT];
                let joined = concat_lines(store, from, to, &mut buf)?;
                delete_range(store, sess, from, to)?;
                store.insert(from, joined).map_err(map_store_error)?;
                marks_shift_up(sess, from, 1);
                sess.modified = true;
                sess.current = from;
            }
            finish(store, sess, io, g)
        }
        Command::Help => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if let Some(message) = sess.error_msg {
                io.emit_err(message.as_bytes());
                io.emit_err(b"\n");
            }
            finish(store, sess, io, g)
        }
        Command::HelpMode => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            sess.garrulous = !sess.garrulous;
            if sess.garrulous
                && let Some(message) = sess.error_msg
            {
                io.emit_err(message.as_bytes());
                io.emit_err(b"\n");
            }
            finish(store, sess, io, g)
        }
        Command::PromptToggle => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            sess.toggle_prompt();
            finish(store, sess, io, g)
        }
        Command::Crypt => {
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            let _ = g;
            // The non-DES build's answer, verbatim (`main.c:843-845`).
            Err(err("crypt unavailable"))
        }
        Command::Scroll => {
            // `main.c:848-864`: an optional row count, then the window
            // from the addressed line (second defaults to current + 1).
            let bytes = &line[letter_end..];
            let mut at = 0;
            let mut rows = ROWS;
            while at < bytes.len() && bytes.as_bytes()[at].is_ascii_digit() {
                rows = rows
                    .saturating_mul(10)
                    .saturating_add((bytes.as_bytes()[at] - b'0') as i32);
                at += 1;
            }
            let (_, second) = evaluate_range(&range, &ctx, (1, sess.current + 1))
                .map_err(|_| err("invalid address"))?;
            let g = suffix_scan(&bytes[at..])?;
            let to = count.min(second.saturating_add(rows.max(0) as usize));
            display(store, sess, io, second, to, g)?;
            Ok(Flow::Continue)
        }
        Command::Substitute | Command::Global | Command::GlobalInteractive | Command::Undo
        | Command::Shell => match command {
            Command::Shell if sess.secure || sess.restricted => Err(err("shell access restricted")),
            Command::Substitute | Command::Global | Command::GlobalInteractive => {
                Err(err("search commands not wired"))
            }
            Command::Undo => Err(err("undo not wired")),
            _ => Err(err("shell access not wired")),
        },
    }
}

/// `read_file`'s store half (`io.c`): read `name`, insert its lines after
/// line `after`, report the byte count (unless scripted), warn about an
/// appended final newline. Returns the last line number now in the
/// buffer (`after` when the file is empty).
fn read_into_store<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    name: &str,
    after: usize,
) -> Result<usize, ExecError> {
    let mut scratch = [0u8; MAX_TEXT];
    let size = io.read_file(name, &mut scratch).map_err(|e| match e {
        EditorError::TooLong => err("out of memory"),
        EditorError::InvalidArgument => err("cannot open input file"),
    })?;
    let text = core::str::from_utf8(&scratch[..size]).map_err(|_| err("invalid content"))?;
    let mut at = after;
    let lines = LinesOf { rest: text };
    for piece in lines {
        at += 1;
        insert_line(store, sess, at, piece)?;
    }
    if size > 0 && !text.as_bytes().ends_with(b"\n") && !sess.scripted {
        // `read_stream`'s "newline appended" notice (`io.c`), simplified
        // to the text-file branch (binary files are rejected above).
        io.emit_err(b"newline appended\n");
    }
    if !sess.scripted {
        // `read_file` reports the byte count on stderr (`io.c`).
        let mut digits = [0u8; 20];
        let used = write_decimal(size as u64, &mut digits);
        io.emit_err(&digits[..used]);
        io.emit_err(b"\n");
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{GapStore, LineTable};
    use std::collections::BTreeMap;

    /// Scripted I/O: captures output, serves an in-memory file table.
    struct ScriptIo {
        out: Vec<u8>,
        err_out: Vec<u8>,
        files: BTreeMap<String, Vec<u8>>,
    }

    impl ScriptIo {
        fn new() -> Self {
            ScriptIo { out: Vec::new(), err_out: Vec::new(), files: BTreeMap::new() }
        }

        fn with_file(name: &str, content: &str) -> Self {
            let mut io = Self::new();
            io.files.insert(name.to_string(), content.as_bytes().to_vec());
            io
        }

        fn out_text(&self) -> String {
            String::from_utf8(self.out.clone()).unwrap()
        }

        fn out_lines(&self) -> Vec<String> {
            self.out_text()
                .split('\n')
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        }
    }

    impl EditorIo for ScriptIo {
        fn emit(&mut self, bytes: &[u8]) {
            self.out.extend_from_slice(bytes);
        }
        fn emit_err(&mut self, bytes: &[u8]) {
            self.err_out.extend_from_slice(bytes);
        }
        fn read_file(&mut self, name: &str, out: &mut [u8]) -> Result<usize, EditorError> {
            let data = self.files.get(name).ok_or(EditorError::InvalidArgument)?;
            if data.len() > out.len() {
                return Err(EditorError::TooLong);
            }
            out[..data.len()].copy_from_slice(data);
            Ok(data.len())
        }
        fn write_file(
            &mut self,
            name: &str,
            data: &[u8],
            append: bool,
        ) -> Result<usize, EditorError> {
            let entry = self.files.entry(name.to_string()).or_default();
            if append {
                entry.extend_from_slice(data);
            } else {
                *entry = data.to_vec();
            }
            Ok(data.len())
        }
    }

    fn seeded(lines: &[&str]) -> (GapStore, Session) {
        let mut store = GapStore::new();
        for (i, line) in lines.iter().enumerate() {
            store.insert(i + 1, line).unwrap();
        }
        (store, Session::new(false, false, false, None))
    }

    fn feed<S: TextStore>(store: &mut S, sess: &mut Session, io: &mut ScriptIo, lines: &[&str]) {
        for line in lines {
            step(store, sess, line, io)
                .unwrap_or_else(|e| panic!("step {line:?} failed: {e:?}"));
        }
    }

    #[test]
    fn test_append_collects_until_dot_and_prints() {
        let (mut store, mut sess) = seeded(&[]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["a", "first", "second", ".", "1,$n"]);
        assert_eq!(io.out_lines(), ["1\tfirst", "2\tsecond"]);
        assert_eq!(sess.current, 2, "display moves current to the last line");
        assert!(sess.modified, "inserts mark the buffer modified");
    }

    #[test]
    fn test_print_list_formats_match_put_tty_line() {
        let (mut store, mut sess) = seeded(&["plain", "a\tb", "hi"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,$l"]);
        // `l`: tabs named, line-end dollar (`io.c:302-347`).
        assert_eq!(io.out_lines(), ["plain$", "a\tb$".replace('\t', "\\t").as_str(), "hi$"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2n"]);
        // `n`: `%ld\t` prefix (`io.c:315`) and RAW bytes after it — only
        // `l` escapes; current moves to line 2.
        assert_eq!(io.out_lines(), ["2\ta\tb"]);
        assert_eq!(sess.current, 2);
    }

    #[test]
    fn test_delete_readvances_with_inc_mod() {
        let (mut store, mut sess) = seeded(&["one", "two", "three"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1d"]);
        // `INC_MOD(0, 2) = 1`: the line after the deleted block.
        assert_eq!(sess.current, 1);
        assert_eq!(store.line_count(), 2);
        feed(&mut store, &mut sess, &mut io, &["$d"]);
        // Deleting the last line: no next line, current stays at from - 1.
        assert_eq!(sess.current, 1);
    }

    #[test]
    fn test_change_replaces_range_in_place() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2c", "B1", "B2", ".", "1,$p"]);
        assert_eq!(io.out_lines(), ["a", "B1", "B2", "c"]);
    }

    #[test]
    fn test_insert_before_line_one_of_empty_is_rejected() {
        let (mut store, mut sess) = seeded(&["x"]);
        let mut io = ScriptIo::new();
        assert_eq!(
            step(&mut store, &mut sess, "0i", &mut io).unwrap_err().message,
            "invalid address"
        );
    }

    #[test]
    fn test_move_reorders_and_rejects_inside_destination() {
        let (mut store, mut sess) = seeded(&["a", "b", "c", "d"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,2m4", "1,$p"]);
        assert_eq!(io.out_lines(), ["c", "d", "a", "b"]);
        assert_eq!(sess.current, 4, "current rides the last moved line");
        // `1,3m2` lands inside the range → "invalid destination"
        // (`main.c:638-641`).
        assert_eq!(
            step(&mut store, &mut sess, "1,3m2", &mut io).unwrap_err().message,
            "invalid destination"
        );
        // `1,2m2` is the no-op shape (`addr == second_addr`,
        // `main.c:1141`) — legal, current parks on the second address.
        feed(&mut store, &mut sess, &mut io, &["1,2m2"]);
        assert_eq!(sess.current, 2);
    }

    #[test]
    fn test_transfer_duplicates_block() {
        let (mut store, mut sess) = seeded(&["a", "b"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,2t0", "1,$n"]);
        // `t0` copies the block to the front (`main.c:757-763` allows 0).
        assert_eq!(io.out_lines(), ["1\ta", "2\tb", "3\ta", "4\tb"]);
        assert_eq!(sess.current, 4);
    }

    #[test]
    fn test_join_merges_range_into_one_line() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1,3j", ".,p"]);
        assert_eq!(io.out_lines(), ["abc"]);
        assert_eq!(sess.current, 1);
    }

    #[test]
    fn test_marks_survive_and_die_with_their_line() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2kx"]);
        // `'x` resolves through the mark table (`addr.rs` Context).
        feed(&mut store, &mut sess, &mut io, &["'xp"]);
        assert_eq!(io.out_lines(), ["b"]);
        // Deleting the marked line drops the mark; `'x` then fails.
        feed(&mut store, &mut sess, &mut io, &["2d"]);
        assert_eq!(
            step(&mut store, &mut sess, "'xp", &mut io).unwrap_err().message,
            "invalid address"
        );
    }

    #[test]
    fn test_line_number_prints_second_or_last() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["="]);
        assert_eq!(io.out_lines(), ["3"], "no address prints addr_last");
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["2="]);
        assert_eq!(io.out_lines(), ["2"], "an explicit address prints it");
    }

    #[test]
    fn test_quit_modified_then_quiet_quit() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["1d"]);
        assert_eq!(step(&mut store, &mut sess, "q", &mut io).unwrap(), Flow::QuitModified);
        // The main loop clears the flag on the interactive warning
        // (`main.c:243`); the next `q` succeeds.
        sess.modified = false;
        assert_eq!(step(&mut store, &mut sess, "q", &mut io).unwrap(), Flow::Quit);
    }

    #[test]
    fn test_wq_quits_after_whole_write_only() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::with_file("out.txt", "");
        feed(&mut store, &mut sess, &mut io, &["1,2c", "X", "Y", "."]);
        // Partial write + modified: EMOD, not quit (`main.c:813-815`).
        assert_eq!(
            step(&mut store, &mut sess, "2,3wq out.txt", &mut io).unwrap(),
            Flow::QuitModified
        );
        // Whole-buffer write clears modified and quits.
        assert_eq!(step(&mut store, &mut sess, "wq out.txt", &mut io).unwrap(), Flow::Quit);
        assert_eq!(io.files["out.txt"], b"X\nY\nc\n".to_vec());
    }

    #[test]
    fn test_write_reports_and_clears_modified() {
        // Scripted: `-s` suppresses the byte-count notice (`io.c`).
        let mut store = GapStore::new();
        for (i, line) in ["a", "b"].iter().enumerate() {
            store.insert(i + 1, line).unwrap();
        }
        let mut sess = Session::new(true, false, false, None);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["w out.txt"]);
        assert_eq!(io.files["out.txt"], b"a\nb\n".to_vec());
        assert!(!sess.modified, "whole-buffer write clears the flag");
        assert!(io.err_out.is_empty(), "scripted sessions skip byte counts");
    }

    #[test]
    fn test_read_inserts_after_address_and_names_the_file() {
        let mut io = ScriptIo::with_file("part.txt", "p\nq\n");
        let (mut store, mut sess) = seeded(&["a", "b"]);
        feed(&mut store, &mut sess, &mut io, &["1r part.txt", "1,$n"]);
        assert_eq!(io.out_lines(), ["1\ta", "2\tp", "3\tq", "4\tb"]);
        assert_eq!(sess.filename_str(), Some("part.txt"), "first `r` names the file");
    }

    #[test]
    fn test_edit_swaps_buffer_and_reports_newlines_added() {
        let mut io = ScriptIo::with_file("f.txt", "l1\nl2");
        let (mut store, mut sess) = seeded(&["old"]);
        feed(&mut store, &mut sess, &mut io, &["e f.txt", "1,$p"]);
        assert_eq!(io.out_lines(), ["l1", "l2"]);
        assert!(!sess.modified, "`e` ends with a clean buffer");
        // Non-scripted sessions get the newline notice AND the byte count
        // (`read_stream` tail, then `read_file`'s `%lu`).
        assert_eq!(io.err_out, b"newline appended\n5\n".to_vec());
        assert_eq!(sess.filename_str(), Some("f.txt"));
    }

    #[test]
    fn test_edit_refuses_modified_softly() {
        let mut io = ScriptIo::with_file("f.txt", "x\n");
        let (mut store, mut sess) = seeded(&["old1", "old2"]);
        feed(&mut store, &mut sess, &mut io, &["1d"]);
        assert_eq!(store.line_count(), 1);
        assert_eq!(step(&mut store, &mut sess, "e f.txt", &mut io).unwrap(), Flow::QuitModified);
        assert_eq!(store.line_count(), 1, "the buffer is untouched by the refused `e`");
        // `E` discards without asking (`main.c:510`).
        feed(&mut store, &mut sess, &mut io, &["E f.txt"]);
        assert_eq!(store.line_count(), 1);
        assert!(!sess.modified);
    }

    #[test]
    fn test_filename_prints_and_sets() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["f a.txt"]);
        assert_eq!(io.out_lines(), ["a.txt"]);
        assert_eq!(sess.filename_str(), Some("a.txt"));
        // Bare `f` prints the current name (`main.c:548-552`).
        feed(&mut store, &mut sess, &mut io, &["f"]);
        assert_eq!(io.out_lines(), ["a.txt", "a.txt"]);
    }

    #[test]
    fn test_help_reads_the_saved_message() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        let _ = step(&mut store, &mut sess, "0a", &mut io).unwrap_err();
        feed(&mut store, &mut sess, &mut io, &["h"]);
        assert_eq!(io.err_out, b"invalid address\n".to_vec());
    }

    #[test]
    fn test_declared_gaps_answer_through_the_question_channel() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        assert_eq!(
            step(&mut store, &mut sess, "1s/a/b/", &mut io).unwrap_err().message,
            "search commands not wired"
        );
        assert_eq!(
            step(&mut store, &mut sess, "1,2g/p/d", &mut io).unwrap_err().message,
            "search commands not wired"
        );
        assert_eq!(step(&mut store, &mut sess, "u", &mut io).unwrap_err().message, "undo not wired");
        assert_eq!(
            step(&mut store, &mut sess, "!ls", &mut io).unwrap_err().message,
            "shell access not wired"
        );
        let mut secure = Session::new(false, true, false, None);
        assert_eq!(
            step(&mut store, &mut secure, "!ls", &mut io).unwrap_err().message,
            "shell access restricted"
        );
        // The non-DES build's crypt answer, verbatim (`main.c:843-845`).
        assert_eq!(
            step(&mut store, &mut sess, "x", &mut io).unwrap_err().message,
            "crypt unavailable"
        );
    }

    #[test]
    fn test_suffix_rules_follow_get_command_suffix() {
        let (mut store, mut sess) = seeded(&["a", "b"]);
        let mut io = ScriptIo::new();
        // A trailing `!` is an invalid suffix everywhere (`main.c:448`).
        assert_eq!(
            step(&mut store, &mut sess, "d!", &mut io).unwrap_err().message,
            "invalid command suffix"
        );
        // `dp` deletes and then prints the new current line
        // (`main.c:228-233`).
        feed(&mut store, &mut sess, &mut io, &["1dp"]);
        assert_eq!(io.out_lines(), ["b"]);
    }

    #[test]
    fn test_percent_and_bare_addresses_navigate() {
        let (mut store, mut sess) = seeded(&["a", "b", "c"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["3"]);
        assert_eq!(sess.current, 3, "a bare address displays and moves current");
        feed(&mut store, &mut sess, &mut io, &["%d"]);
        assert_eq!(store.line_count(), 0, "`%` names the whole buffer");
    }

    #[test]
    fn test_scroll_walks_a_window() {
        let mut store = GapStore::new();
        let mut sess = Session::new(false, false, false, None);
        for i in 0..30usize {
            store.insert(i + 1, "x").unwrap();
        }
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["z"]);
        assert_eq!(io.out_lines().len(), 23, "second + ROWS lines inclusive (`main.c:859`)");
        assert_eq!(sess.current, 23);
    }

    #[test]
    fn test_double_backend_agreement_through_exec() {
        // The exec layer only speaks `TextStore`, so both backends must
        // produce the same transcript (store.rs `test_stores_agree` at the
        // execution level).
        let script = ["1,2m4", "3d", "1,3j", "1,$n"];
        let mut gaps = GapStore::new();
        for (i, line) in ["a", "b", "c", "d"].iter().enumerate() {
            gaps.insert(i + 1, line).unwrap();
        }
        let mut table = LineTable::new();
        table.load("a\nb\nc\nd\n").unwrap();
        let mut sess_a = Session::new(true, false, false, None);
        let mut sess_b = Session::new(true, false, false, None);
        let mut io_a = ScriptIo::new();
        let mut io_b = ScriptIo::new();
        for line in script {
            step(&mut gaps, &mut sess_a, line, &mut io_a).unwrap();
            step(&mut table, &mut sess_b, line, &mut io_b).unwrap();
        }
        assert_eq!(io_a.out_text(), io_b.out_text());
        assert_eq!(sess_a.current, sess_b.current);
    }
}
