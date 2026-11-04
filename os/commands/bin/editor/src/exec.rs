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
//! whole-buffer write. The shell escape (`!`, plus `!command` file names)
//! runs through the [`EditorIo::run_shell`] / popen-shaped file legs; the
//! remaining deliberate gap (crypt `x` on a non-DES build) answers through
//! the same `?` channel with an explicit message instead of pretending.

use crate::addr::{
    evaluate_with, evaluate_range_with, parse_range, AddressRange, Context, SearchProbe, MAX_MARKS,
};
use crate::cmd::{parse_command, Command, Modifiers};
use crate::store::{TextStore, MAX_LINES, MAX_TEXT};
use crate::EditorError;
use alloc::borrow::Cow;
use alloc::vec::Vec;
use minix_regex::sed::{apply as subst_apply, Subst, SubstScope};
use minix_regex::pattern::compile_basic;

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
    /// The shell escape's run half: C `system()` (`stdlib.h`) — fork, exec
    /// `/bin/sh -c <cmd>`, wait. The `!` command discards the exit status
    /// (`main.c:876`), so this carries no result; the session keeps going
    /// whatever the child did.
    fn run_shell(&mut self, cmd: &str);
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
    /// The in-progress state of the `G`/`V` interactive global (the interact
    /// half of C's `exec_global`, glbl.c:107-134): each active line is
    /// displayed and then one command is awaited.
    pending_global: Option<PendingGlobal>,
    /// The previous pattern bytes (written by `s/old/...`; the empty pattern
    /// `//` and the bare `s` reuse it -- C's `pat` global plus the `expr`
    /// cache, re.c:59-88). Empty len = none.
    last_pattern: [u8; MAX_TEXT],
    last_pattern_len: usize,
    /// The previous replacement template (used by the `%%<delim>` reuse form
    /// and the bare `s` -- C's `rhbuf`, sub.c:44-46). Empty len = none.
    last_replacement: [u8; MAX_TEXT],
    last_replacement_len: usize,
    /// The previous `s` scope (replayed by bare `s`/`sg`/`sN` -- C's
    /// `sgflag`/`sgnum` globals). `None` = no substitution yet ("no previous
    /// substitution").
    last_scope: Option<SubstScope>,
    /// The previous shell command, `!` prefix included (C's static `shcmd`
    /// buffer with `shcmdi` as its length, `main.c:80-82`): a `!!` repeat
    /// substitutes it and any later command overwrites it, shared by the
    /// `!` command and `!command` file names alike. Empty len = never run
    /// ("no previous command", `main.c:1015-1021`).
    pub last_shell: [u8; MAX_TEXT],
    pub last_shell_len: usize,
    /// The undo stack (C `ustack`/`u_p`, undo.c:41-43). Each entry records one
    /// change; after `u` replays it in reverse the kind flips and the order
    /// reverses, so a second `u` redoes it.
    undo_stack: Vec<UndoEntry>,
    /// `u_current_addr`/`u_addr_last` (C undo.c:64-65): the state before the
    /// change; `None` = not armed yet (`u` answers "nothing to undo").
    undo_current: Option<usize>,
    undo_last: Option<usize>,
    /// `isglobal` (C main.c:91): a global command is running -- clearing the
    /// stack is suppressed (a whole global run is one undo unit), a nested `g`
    /// is refused, an `s` with no match is not an error, and a bare address
    /// defaults to the current line.
    is_global: bool,
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
            pending_global: None,
            pending_gflag: 0,
            last_pattern: [0; MAX_TEXT],
            last_pattern_len: 0,
            last_replacement: [0; MAX_TEXT],
            last_replacement_len: 0,
            last_scope: None,
            last_shell: [0; MAX_TEXT],
            last_shell_len: 0,
            undo_stack: Vec::new(),
            undo_current: None,
            undo_last: None,
            is_global: false,
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

    fn last_pattern_bytes(&self) -> &[u8] {
        &self.last_pattern[..self.last_pattern_len]
    }

    fn last_replacement_bytes(&self) -> &[u8] {
        &self.last_replacement[..self.last_replacement_len]
    }

    /// Remember a freshly compiled pattern (C `pat = tpat`, main.c:739-743 --
    /// it is remembered **whether or not the substitution succeeds**: the
    /// cache is charged at parse time).
    fn remember_pattern(&mut self, bytes: &[u8]) -> Result<(), ExecError> {
        if bytes.len() > MAX_TEXT {
            return Err(err("out of memory"));
        }
        self.last_pattern[..bytes.len()].copy_from_slice(bytes);
        self.last_pattern_len = bytes.len();
        Ok(())
    }

    fn remember_replacement(&mut self, bytes: &[u8]) -> Result<(), ExecError> {
        if bytes.len() > MAX_TEXT {
            return Err(err("out of memory"));
        }
        self.last_replacement[..bytes.len()].copy_from_slice(bytes);
        self.last_replacement_len = bytes.len();
        Ok(())
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

/// The kind of one undo record (C `UADD`/`UDEL`, ed.h:84-85). `u` flips it
/// after replay: Add becomes Delete (the next `u` redoes the deletion), and
/// vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UndoKind {
    /// Lines `from..=to` were inserted (undo = delete them back).
    Add,
    /// Lines `from..=to` were deleted and `text` keeps their contents
    /// (undo = insert them back).
    Delete,
}

/// One undo record (C `undo_t`, ed.h:87-91 -- C keeps identity with line-node
/// pointers; this model uses line numbers plus contents: a Delete entry carries
/// the deleted text and inserts it back verbatim on `u`).
#[derive(Debug, Clone)]
struct UndoEntry {
    kind: UndoKind,
    from: usize,
    to: usize,
    text: Vec<u8>,
}

/// `clear_undo_stack` (undo.c:107-120): clear the stack and snapshot the state
/// into `undo_current`/`undo_last` -- that snapshot is `u`'s "state before the
/// change". Every modifying command (a/i/c/d/e/E/i/j/m/r/s/t) calls it before
/// touching the buffer.
fn clear_undo<S: TextStore>(store: &S, sess: &mut Session) {
    sess.undo_stack.clear();
    sess.undo_current = Some(sess.current);
    sess.undo_last = Some(store.line_count());
}

/// The insert half of `push_undo_stack` (C's UADD, main.c:1085/1125/1203: every
/// inserted line records one entry).
fn push_undo_add(sess: &mut Session, pos: usize) {
    sess.undo_stack.push(UndoEntry {
        kind: UndoKind::Add,
        from: pos,
        to: pos,
        text: Vec::new(),
    });
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
    push_undo_add(sess, pos);
    Ok(())
}

/// A deleted range: the store call plus `delete_lines`' bookkeeping (the UDEL
/// record at `main.c:1222` plus `main.c:1233-1235` -- current becomes
/// `from - 1` and the buffer is dirty). The deleted text travels with the entry
/// (C keeps it in the nodes; here `text` carries it).
fn delete_range<S: TextStore>(
    store: &mut S,
    sess: &mut Session,
    from: usize,
    to: usize,
) -> Result<(), ExecError> {
    let mut text = Vec::new();
    let mut n = from;
    while n <= to {
        let mut buf = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
        text.extend_from_slice(&buf[..used]);
        text.push(b'\n');
        n += 1;
    }
    store.delete(from, to).map_err(map_store_error)?;
    marks_delete(sess, from, to);
    sess.current = from - 1;
    sess.modified = true;
    sess.undo_stack.push(UndoEntry {
        kind: UndoKind::Delete,
        from,
        to,
        text,
    });
    Ok(())
}

/// The active table (C `glbl.c`'s `active_list`, recording content snapshots in
/// ascending order). Fixed-size buffers: active contents are a subset of the
/// buffer contents, so the store's capacity is the table's upper bound.
struct ActiveList {
    text: [u8; MAX_TEXT],
    starts: [u16; MAX_LINES + 1],
    count: usize,
}

impl ActiveList {
    fn new() -> Self {
        ActiveList { text: [0; MAX_TEXT], starts: [0; MAX_LINES + 1], count: 0 }
    }

    fn line(&self, index: usize) -> &[u8] {
        &self.text[self.starts[index] as usize..self.starts[index + 1] as usize]
    }

    fn push(&mut self, bytes: &[u8]) {
        let start = self.starts[self.count] as usize;
        debug_assert!(self.count < MAX_LINES && start + bytes.len() <= MAX_TEXT);
        self.text[start..start + bytes.len()].copy_from_slice(bytes);
        self.count += 1;
        self.starts[self.count] = (start + bytes.len()) as u16;
    }
}

/// The in-progress state of `G`/`V`: the active table plus the cursor, plus the
/// most recent command seen in this `G` run (C's `ocmd` static, the source `&`
/// replays; `seen` corresponds to C's local `cmd != NULL` "no previous command"
/// gate -- it starts over for each `G` run).
struct PendingGlobal {
    active: ActiveList,
    /// The index of the next active line.
    next: usize,
    /// The scan start (line number) for content relocation.
    relocate: usize,
    /// The line number currently awaiting an answer.
    at: usize,
    prev: [u8; MAX_TEXT],
    prev_len: usize,
    seen: bool,
    /// The suffix print bit (`G/.../n` etc., C `main.c:566`'s
    /// GET_COMMAND_SUFFIX).
    gflag: u8,
}

/// The common front half of `g`/`v`/`G`/`V` (C `build_active_list`,
/// glbl.c:43-67): evaluate the range (the whole buffer by default), parse the
/// pattern (an empty pattern reuses the previous one and charges it to the
/// cache), and collect the active table by match or non-match. Returns the
/// table and the start of the remainder after the pattern.
fn build_active<S: TextStore>(
    store: &mut S,
    sess: &mut Session,
    line: &str,
    cursor: usize,
    range: &AddressRange,
    is_match: bool,
) -> Result<(ActiveList, usize), ExecError> {
    let count = store.line_count();
    let mut probe = StoreSearch { store, sess: &*sess };
    let (from, to) =
        evaluate_range_with(range, &context_of(store, sess), (1, count), line, &mut probe)
            .map_err(map_store_error)?;
    // The probe's borrow ends here: building the table below takes a mutable
    // borrow.
    let rest = &line[cursor..];
    let bytes = rest.as_bytes();
    if bytes.is_empty() || bytes[0] == b' ' {
        return Err(err("invalid pattern delimiter"));
    }
    let delim = bytes[0];
    let (pspan, after) =
        crate::addr::scan_pattern(bytes, 1, delim).map_err(map_store_error)?;
    let mut pattern_len = pspan.len as usize;
    let mut pattern_buf = [0u8; MAX_TEXT];
    pattern_buf[..pattern_len].copy_from_slice(&bytes[1..1 + pattern_len]);
    if pattern_len == 0 {
        // An empty pattern reuses the previous one (C
        // `get_compiled_pattern`'s expr cache).
        pattern_len = sess.last_pattern_len;
        pattern_buf[..pattern_len].copy_from_slice(sess.last_pattern_bytes());
    }
    if pattern_len == 0 {
        return Err(err("no previous pattern"));
    }
    sess.remember_pattern(&pattern_buf[..pattern_len])?;
    let pattern_text =
        core::str::from_utf8(&pattern_buf[..pattern_len]).map_err(|_| err("invalid content"))?;
    let compiled = compile_basic(pattern_text).map_err(|_| err("invalid pattern"))?;
    let mut active = ActiveList::new();
    let mut n = from;
    while n <= to {
        let mut buf = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
        let text = core::str::from_utf8(&buf[..used]).map_err(|_| err("invalid content"))?;
        if compiled.is_match(text) == is_match {
            active.push(&buf[..used]);
        }
        n += 1;
    }
    Ok((active, after))
}

/// The execution half of `g`/`v` (all of C `glbl.c` + `main.c:562-574`):
///
/// 1. The range defaults to the whole buffer (`check_addr_range(1, addr_last)`);
///    the tail form is `<delim>pattern<delim>subcommand` (an empty subcommand
///    does not imply `p` -- this build has no BACKWARDS, glbl.c:56-63); an
///    empty pattern reuses the previous one.
/// 2. Build the active table (`build_active_list`, glbl.c:43-67): `g` collects
///    matching lines and `v` non-matching ones, recording content snapshots in
///    ascending order.
/// 3. Clear the undo stack once (exec_global:143) -- a whole global run is
///    **one** undo unit, and stack clearing inside subcommands is suppressed by
///    `is_global`.
/// 4. Run the subcommand for each active line (`exec_global:104-144`): the
///    current line moves to the active line; active lines already deleted or
///    changed are skipped (relocation rule below); a subcommand error aborts
///    the whole global run (changes already made are kept, as in C). A nested
///    `g` is refused at dispatch (main.c:562-564).
///
/// **Registered deviation (locating active lines)**: C uses line-node identity
/// (the slot of a deleted line is emptied and `next_active_node` skips it);
/// this model relocates by the content captured at match time, scanning forward
/// from the last hit -- a decrease in the line count is taken as a deletion and
/// the scan restarts at the hit line; otherwise it starts at the next line.
/// Lines with duplicate contents may therefore land differently from C under
/// subcommands that move or insert lines.
fn global_command<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    cursor: usize,
    range: &AddressRange,
) -> Result<Flow, ExecError> {
    if sess.is_global {
        return Err(err("cannot nest global commands"));
    }
    // The command letter is at `line[cursor - 1]` (`g` or `v`).
    let is_match = line.as_bytes()[cursor - 1] == b'g';
    let (active, after) = build_active(store, sess, line, cursor, range, is_match)?;
    if !sess.is_global {
        clear_undo(store, sess);
    }
    let cmd = &line[cursor + after..];
    let was_global = sess.is_global;
    sess.is_global = true;
    let mut result = Ok(Flow::Continue);
    let mut cursor_pos = 1usize;
    for index in 0..active.count {
        let want = active.line(index).to_vec();
        // Content relocation: scan forward from the previous hit for a line
        // with the same text (see the deviation note in the function header).
        let mut found = None;
        let mut n = cursor_pos;
        while n <= store.line_count() {
            let mut b = [0u8; MAX_TEXT];
            let used = store.read_line(n, &mut b).map_err(map_store_error)?;
            if &b[..used] == want.as_slice() {
                found = Some(n);
                break;
            }
            n += 1;
        }
        let Some(at) = found else { continue };
        sess.current = at;
        if cmd.is_empty() {
            // Empty subcommand: this build does not imply `p` (glbl.c:56-63
            // has no BACKWARDS); only move the current line.
            cursor_pos = at + 1;
            continue;
        }
        let before = store.line_count();
        result = step_inner(store, sess, cmd, io);
        if result.is_err() || matches!(result, Ok(Flow::Quit) | Ok(Flow::QuitModified)) {
            break;
        }
        // Cursor advance: a decrease in the count means a line was deleted, so
        // the next active line may sit on the hit line itself (rescan);
        // otherwise the hit line, if it survived, is consumed (scan starts at
        // the next line).
        let survived = at <= store.line_count() && {
            let mut b = [0u8; MAX_TEXT];
            let used = store.read_line(at, &mut b).map_err(map_store_error)?;
            &b[..used] == want.as_slice()
        };
        cursor_pos = if store.line_count() < before || !survived { at } else { at + 1 };
    }
    sess.is_global = was_global;
    result
}

/// The startup half of `G`/`V` (the G/V branch of C `main.c:560-574`): the same
/// table-building half as `g`/`v` (`build_active_list`), then read the p/l/n
/// suffix, clear the stack once, enter input mode, and display the first active
/// line -- the rest of the interaction spans several [`step`] calls.
fn global_interactive_start<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    cursor: usize,
    range: &AddressRange,
) -> Result<Flow, ExecError> {
    if sess.is_global {
        return Err(err("cannot nest global commands"));
    }
    let is_match = line.as_bytes()[cursor - 1] == b'G';
    let (active, after) = build_active(store, sess, line, cursor, range, is_match)?;
    let gflag = suffix_scan(&line[cursor + after..])?;
    if !sess.is_global {
        clear_undo(store, sess);
    }
    sess.is_global = true;
    let mut pending = PendingGlobal {
        active,
        next: 0,
        relocate: 1,
        at: 0,
        prev: [0; MAX_TEXT],
        prev_len: 0,
        seen: false,
        gflag,
    };
    if global_resume(store, sess, io, &mut pending)? {
        sess.pending_global = Some(pending);
        Ok(Flow::Continue)
    } else {
        sess.is_global = false;
        Ok(Flow::Continue)
    }
}

/// Locate and display the next active line (the `display_lines` at the head of
/// C's `exec_global` loop, glbl.c:107-110). A false return means the active
/// table is exhausted -- the whole `G` run settles.
fn global_resume<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    pending: &mut PendingGlobal,
) -> Result<bool, ExecError> {
    while pending.next < pending.active.count {
        let want = pending.active.line(pending.next).to_vec();
        // Content relocation: scan forward from the previous hit for a line
        // with the same text (the same shape as `g`).
        let mut found = None;
        let mut n = pending.relocate;
        while n <= store.line_count() {
            let mut b = [0u8; MAX_TEXT];
            let used = store.read_line(n, &mut b).map_err(map_store_error)?;
            if &b[..used] == want.as_slice() {
                found = Some(n);
                break;
            }
            n += 1;
        }
        let Some(at) = found else {
            pending.next += 1;
            continue;
        };
        sess.current = at;
        pending.at = at;
        display(store, sess, io, at, at, pending.gflag)?;
        return Ok(true);
    }
    Ok(false)
}

/// One `G`/`V` answer (C glbl.c:111-134): an empty line skips, `&` replays,
/// and anything else runs through the global grammar. `Ok(None)` = keep waiting
/// for the next active line's answer; `Ok(Some)` = the `G` run settles and
/// carries the flow out; `Err` = abort the whole `G` run (changes already made
/// are kept, as in C).
fn global_interact_answer<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    pending: &mut PendingGlobal,
) -> Result<Option<Flow>, ExecError> {
    let at = pending.at;
    let want = pending.active.line(pending.next).to_vec();
    if line.is_empty() {
        // `n == 1 && ibuf == "\n"`: skip this line (glbl.c:119-120).
        pending.next += 1;
        pending.relocate = at + 1;
        return if global_resume(store, sess, io, pending)? {
            Ok(None)
        } else {
            Ok(Some(Flow::Continue))
        };
    }
    let cmd: &str;
    if line == "&" {
        if !pending.seen {
            // C glbl.c:121-125: this run has not supplied a command yet.
            return Err(err("no previous command"));
        }
        cmd = core::str::from_utf8(&pending.prev[..pending.prev_len])
            .map_err(|_| err("invalid content"))?;
    } else {
        if line.len() > MAX_TEXT {
            return Err(err("out of memory"));
        }
        pending.prev[..line.len()].copy_from_slice(line.as_bytes());
        pending.prev_len = line.len();
        pending.seen = true;
        cmd = line;
    }
    let before = store.line_count();
    match step_inner(store, sess, cmd, io) {
        Err(e) => Err(e),
        Ok(flow @ (Flow::Quit | Flow::QuitModified)) => Ok(Some(flow)),
        Ok(Flow::Continue) => {
            // The isglobal branch of C `append_lines` (main.c:1059-1064):
            // text comes from the command string -- one answer cannot receive
            // text lines, so `a`/`i`/`c` fall through empty.
            if sess.pending_input.take().is_some() {
                sess.pending_gflag = 0;
            }
            // Relocation bookkeeping: a decrease in the count means a line was
            // deleted (rescan); otherwise the hit line, if it survived, is
            // consumed (scan starts at the next line).
            let survived = at <= store.line_count() && {
                let mut b = [0u8; MAX_TEXT];
                let used = store.read_line(at, &mut b).map_err(map_store_error)?;
                &b[..used] == want.as_slice()
            };
            pending.relocate = if store.line_count() < before || !survived { at } else { at + 1 };
            pending.next += 1;
            if global_resume(store, sess, io, pending)? {
                Ok(None)
            } else {
                Ok(Some(Flow::Continue))
            }
        }
    }
}

/// `pop_undo_stack` (undo.c:71-105): the body of `u` -- replay the undo stack in
/// reverse, flipping every change back, then **flip the kinds, reverse the stack
/// order, and swap the state snapshots** (a second `u` redoes; C's
/// `type ^= 1` plus USWAP, undo.c:117-123).
///
/// Replay uses raw store calls (nothing is pushed back onto the stack), and
/// marks shift or disappear exactly as under the original command; afterwards
/// the current line and the buffer size come from the pre-change snapshot
/// (`u_current_addr`/`u_addr_last`).
fn pop_undo<S: TextStore>(store: &mut S, sess: &mut Session) -> Result<(), ExecError> {
    // `undo_last` is the buffer size of the redo state: replay itself restores
    // it (C's `addr_last = u_addr_last` swap); it is used here as the replay
    // completeness assertion.
    let (Some(undo_current), Some(undo_last)) = (sess.undo_current, sess.undo_last) else {
        return Err(err("nothing to undo"));
    };
    // C: `else if (u_p) modified = 1;` -- when there is a record to flip, mark
    // the buffer dirty.
    if !sess.undo_stack.is_empty() {
        sess.modified = true;
    }
    let o_current = sess.current;
    let o_last = store.line_count();
    for i in (0..sess.undo_stack.len()).rev() {
        let (kind, from, to) = {
            let e = &sess.undo_stack[i];
            (e.kind, e.from, e.to)
        };
        match kind {
            UndoKind::Add => {
                // Undo an insertion: first collect the current text of
                // `from..=to` into the entry (after flipping it becomes Delete
                // and a redo needs it), then delete it back.
                let mut text = Vec::new();
                let mut n = from;
                while n <= to {
                    let mut buf = [0u8; MAX_TEXT];
                    let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
                    text.extend_from_slice(&buf[..used]);
                    text.push(b'\n');
                    n += 1;
                }
                store.delete(from, to).map_err(map_store_error)?;
                marks_delete(sess, from, to);
                let e = &mut sess.undo_stack[i];
                e.kind = UndoKind::Delete;
                e.text = text;
            }
            UndoKind::Delete => {
                // Undo a deletion: insert the carried text back line by line at
                // `from`, and flip it to Add.
                let text = sess.undo_stack[i].text.clone();
                let line_text =
                    core::str::from_utf8(&text).map_err(|_| err("invalid content"))?;
                let pieces = LinesOf { rest: line_text };
                let mut pos = from;
                let mut k = 0;
                for piece in pieces {
                    store.insert(pos, piece).map_err(map_store_error)?;
                    pos += 1;
                    k += 1;
                }
                marks_shift_up(sess, from, k);
                let e = &mut sess.undo_stack[i];
                e.kind = UndoKind::Add;
                e.from = from;
                e.to = from + k - 1;
            }
        }
    }
    sess.undo_stack.reverse();
    sess.current = undo_current;
    sess.undo_current = Some(o_current);
    sess.undo_last = Some(o_last);
    debug_assert_eq!(store.line_count(), undo_last, "回放应把缓冲还原到改动前规模");
    Ok(())
}

fn map_store_error(e: EditorError) -> ExecError {
    match e {
        // The C editor's buffer-exhaustion face (`io.c` sbuf full).
        EditorError::TooLong => err("out of memory"),
        EditorError::InvalidArgument => err("invalid address"),
        // "no match" from C `get_matching_node_addr`/`search_and_replace`
        // (main.c:938, sub.c:175-179).
        EditorError::NoMatch => err("no match"),
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
fn take_filename<I: EditorIo>(
    rest: &str,
    sess: &mut Session,
    io: &mut I,
    taken: &mut [u8],
) -> Result<FileName, ExecError> {
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
        // `get_filename`'s shell branch (`main.c:956-963`): parse the
        // command, echo the expansion when it came from a substitution,
        // and return the `!`-prefixed text as the name — `read_file` /
        // `write_file` turn that into a pipe (`io.c:50`/`:157`). C runs
        // nothing here; the popen legs own the execution.
        // The tail after the `!` is the command line; it came from a
        // `&str` line, so the slice is UTF-8 by construction.
        let cmd_text =
            core::str::from_utf8(&bytes[at + 1..]).map_err(|_| err("invalid content"))?;
        let (len, substituted) = build_shell_command(cmd_text, sess, taken)?;
        if substituted {
            io.emit(&taken[1..len]);
            io.emit(b"\n");
        }
        return Ok(FileName::Fresh(len));
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

/// `get_shell_command` (`main.c:992-1048`): build one shell command out of
/// the text after the command letter's `!`. The buffer starts with the `!`
/// prefix (C's `buf[0]`), escapes stay verbatim (`/bin/sh` re-reads them),
/// a `!` in first position replays the previous command, and a `%`
/// anywhere inserts the current file name. Returns the full length
/// (prefix included) and the C substitution status.
///
/// The C return (`*s == '!' || *s == '%'`, `main.c:1047`) reduces to
/// "some substitution happened": the substitution arms park `s` on the
/// substituted character itself, and with no substitution `s` never
/// leaves the head, whose `!`/`%` would have substituted.
///
/// Registered deviations: the `\`-newline continuation of
/// `get_extended_line` (`io.c:219-247`) has no counterpart — the step
/// model feeds one line at a time; the buffer is fixed, so C's
/// `REALLOC` growth ends as "out of memory" past the caller's capacity.
fn build_shell_command(
    rest: &str,
    sess: &mut Session,
    out: &mut [u8],
) -> Result<(usize, bool), ExecError> {
    if out.is_empty() {
        return Err(err("out of memory"));
    }
    let bytes = rest.as_bytes();
    let mut len = 0usize;
    out[len] = b'!';
    len += 1;
    let mut substituted = false;
    let mut at = 0usize;
    while at < bytes.len() {
        let b = bytes[at];
        if b == b'!' && at == 0 {
            // Repeat: replay the previous command sans prefix. C answers
            // from a NULL `shcmd` (`main.c:1014-1021`); the non-BACKWARDS
            // build accepts an empty previous command.
            if sess.last_shell_len == 0 {
                return Err(err("no previous command"));
            }
            let prev = sess.last_shell_len;
            for &pb in &sess.last_shell[1..prev] {
                if len >= out.len() {
                    return Err(err("out of memory"));
                }
                out[len] = pb;
                len += 1;
            }
            substituted = true;
            at += 1;
        } else if b == b'%' {
            // `%`: the current file name (`main.c:1022-1036`). C re-strips
            // escapes from `old_filename`; this crate stores the name
            // already stripped, so the bytes go in as they are.
            if sess.filename_len == 0 {
                return Err(err("no current filename"));
            }
            let n = sess.filename_len;
            for &fb in &sess.filename[..n] {
                if len >= out.len() {
                    return Err(err("out of memory"));
                }
                out[len] = fb;
                len += 1;
            }
            substituted = true;
            at += 1;
        } else {
            if len >= out.len() {
                return Err(err("out of memory"));
            }
            out[len] = b;
            len += 1;
            if b == b'\\' {
                // `main.c:1012-1014`: a backslash quotes the next byte —
                // both bytes copy verbatim.
                at += 1;
                if at < bytes.len() {
                    if len >= out.len() {
                        return Err(err("out of memory"));
                    }
                    out[len] = bytes[at];
                    len += 1;
                }
            }
            at += 1;
        }
    }
    // `shcmd` persists across commands (`main.c:1041-1044`): the built
    // buffer becomes the next repeat's replay source, shared by the `!`
    // command and `!command` file names alike.
    if len > sess.last_shell.len() {
        return Err(err("out of memory"));
    }
    sess.last_shell[..len].copy_from_slice(&out[..len]);
    sess.last_shell_len = len;
    Ok((len, substituted))
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

/// The search probe over the buffer (the store half of `addr::SearchProbe`): the
/// pattern goes to the BRE engine of doc 08, and the scan order copies C
/// `get_matching_node_addr` (main.c:919-938) -- starting at the line after the
/// current one (backward: the previous line), `INC_MOD`/`DEC_MOD`
/// (ed.h:101-102) wrap once around the whole buffer, and the current line is
/// visited last; an empty buffer has no line to scan and answers with an
/// invalid address.
struct StoreSearch<'a, S: TextStore> {
    store: &'a S,
    sess: &'a Session,
}

impl<S: TextStore> SearchProbe for StoreSearch<'_, S> {
    fn find_line(
        &mut self,
        pattern: &[u8],
        forward: bool,
    ) -> Result<Option<usize>, EditorError> {
        let count = self.store.line_count();
        if count == 0 {
            return Err(EditorError::InvalidArgument);
        }
        let text = core::str::from_utf8(pattern).map_err(|_| EditorError::InvalidArgument)?;
        let compiled = compile_basic(text).map_err(|_| EditorError::InvalidArgument)?;
        let mut n = self.sess.current;
        loop {
            n = if forward {
                if n + 1 > count {
                    0
                } else {
                    n + 1
                }
            } else if n == 0 {
                count
            } else {
                n - 1
            };
            if n != 0 {
                let mut buf = [0u8; MAX_TEXT];
                let used = self.store.read_line(n, &mut buf)?;
                let line =
                    core::str::from_utf8(&buf[..used]).map_err(|_| EditorError::InvalidArgument)?;
                if compiled.is_match(line) {
                    return Ok(Some(n));
                }
            }
            if n == self.sess.current {
                return Ok(None);
            }
        }
    }
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

    // `G`/`V` interactive global: each input line is the answer for the
    // current active line (the interact half of C's `exec_global` spans several
    // steps -- each matching line is displayed and one command is awaited).
    if sess.pending_global.is_some() {
        let mut pending = sess.pending_global.take().unwrap();
        match global_interact_answer(store, sess, io, line, &mut pending) {
            Ok(None) => {
                sess.pending_global = Some(pending);
                return Ok(Flow::Continue);
            }
            Ok(Some(flow)) => {
                sess.is_global = false;
                return Ok(flow);
            }
            Err(e) => {
                sess.is_global = false;
                return Err(e);
            }
        }
    }

    let (range, consumed) = parse_range_err(line)?;
    let rest = &line[consumed..];
    // Search probe: it borrows store and sess immutably; NLL guarantees each
    // arm can take a mutable borrow once the probe's last use is past (all
    // evaluation happens before any change).
    let mut probe = StoreSearch { store, sess: &*sess };
    if rest.is_empty() {
        // A bare address (or bare newline): display the second address,
        // defaulting to the line after current (`main.c:884-890`,
        // `check_addr_range(1, current_addr + 1)`). Display moves current
        // to that line, which is how a lone `3` navigates.
        let step_past = usize::from(!sess.is_global);
        let (_, to) = evaluate_range_with(
            &range,
            &context_of(store, sess),
            (1, sess.current + step_past),
            line,
            &mut probe,
        )
        .map_err(map_store_error)?;
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
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            sess.pending_input = Some(second + 1);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Insert => {
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            if second == 0 {
                return Err(err("invalid address"));
            }
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            sess.pending_input = Some(second);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Change => {
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            delete_range(store, sess, from, to)?;
            sess.pending_input = Some(from);
            sess.pending_gflag = g;
            Ok(Flow::Continue)
        }
        Command::Delete => {
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
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
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
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
                let which = range.second.or(range.first).ok_or_else(|| err("invalid address"))?;
                evaluate_with(which, &ctx, line, &mut probe).map_err(map_store_error)?
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
            let kind = take_filename(&line[cursor..], sess, io, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            let typed_len = typed.len();
            // `if (*fnp && *fnp != '!')` (`main.c:531`): a `!command` name
            // feeds the pipe leg but never becomes the default.
            if typed_len > 0 && !typed.starts_with('!') {
                sess.set_filename(typed)?;
            }
            let mut target_buf = [0u8; MAX_FILENAME + 1];
            let target = target_name(sess, typed, &mut target_buf)?;
            // C 522/537: e/E clears the undo stack unconditionally (before
            // deleting the old buffer).
            if !sess.is_global {
                clear_undo(store, sess);
            }
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
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, count), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, io, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            if !typed.is_empty() && sess.filename_len == 0 && !typed.starts_with('!') {
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
                evaluate_range_with(&range, &ctx, (1, count), line, &mut probe)
                .map_err(|_| err("invalid address"))?
            };
            let mut taken = [0u8; MAX_FILENAME + 1];
            let kind = take_filename(&line[cursor..], sess, io, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            // `*fnp != '!'` (`main.c:819`): a pipe target never names the file.
            if !typed.is_empty() && sess.filename_len == 0 && !typed.starts_with('!') {
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
                    EditorError::NoMatch => err("no match"),
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
            let kind = take_filename(&line[cursor..], sess, io, &mut taken)?;
            let mut name_buf = [0u8; MAX_FILENAME + 1];
            let typed = resolve_name(sess, kind, &taken, &mut name_buf)?;
            // `else if (*fnp == '!')` (`main.c:546-549`): a shell name is
            // not a file name at all.
            if typed.starts_with('!') {
                return Err(err("invalid redirection"));
            }
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
            let (_, second) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
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
            let (from, to) = evaluate_range_with(&range, &ctx, (sess.current, sess.current), line, &mut probe)
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
                _ => evaluate_with(dest_spec, &ctx, line, &mut probe).map_err(map_store_error)?,
            };
            if dest > count {
                return Err(err("invalid address"));
            }
            let g = suffix_scan(&line[letter_end + used..])?;
            let len = to - from + 1;
            let mut buf = [0u8; MAX_TEXT];
            let joined = join_lines(store, from, to, &mut buf)?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
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
            let (from, to) = evaluate_range_with(
                &range,
                &ctx,
                (sess.current, sess.current + 1),
                line,
                &mut probe,
            )
                .map_err(|_| err("invalid address"))?;
            if !sess.is_global {
                clear_undo(store, sess);
            }
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
            let (_, second) = evaluate_range_with(&range, &ctx, (1, sess.current + 1), line, &mut probe)
                .map_err(|_| err("invalid address"))?;
            let g = suffix_scan(&bytes[at..])?;
            let to = count.min(second.saturating_add(rows.max(0) as usize));
            display(store, sess, io, second, to, g)?;
            Ok(Flow::Continue)
        }
        Command::Substitute => {
            substitute_command(store, sess, io, line, cursor, &modifiers, &range)
        }
        Command::Undo => {
            // C main.c:794-800: an address is "unexpected address", the suffix
            // is still read, and then `pop_undo_stack` runs -- **without
            // clearing the stack** (clearing is the business of modifying
            // commands).
            if range.first.is_some() || range.second.is_some() {
                return Err(err("unexpected address"));
            }
            let g = suffix_bits(&modifiers)?;
            ensure_line_end(&line[cursor..])?;
            pop_undo(store, sess)?;
            finish(store, sess, io, g)
        }
        Command::Global => {
            global_command(store, sess, io, line, cursor, &range)
        }
        Command::GlobalInteractive => {
            global_interactive_start(store, sess, io, line, cursor, &range)
        }
        Command::Shell => {
            // C `main.c:867-877`: an address is "unexpected address", the
            // command runs to end of line (so no suffix can follow), the
            // expansion echoes only when it came from a substitution, and
            // the `!` confirmation prompt prints unless scripted.
            if range.first.is_some() || range.second.is_some() {
                return Err(err("unexpected address"));
            }
            if sess.secure || sess.restricted {
                return Err(err("shell access restricted"));
            }
            // The whole tail after the `!` is the command (C's
            // `get_shell_command` runs to the newline, so the `!pln`
            // modifier scan must not touch it) — read from `letter_end`.
            let mut cmd = [0u8; MAX_TEXT];
            let (len, substituted) = build_shell_command(&line[letter_end..], sess, &mut cmd)?;
            if substituted {
                io.emit(&cmd[1..len]);
                io.emit(b"\n");
            }
            // `system(shcmd + 1)` (`main.c:876`): the exit status is
            // discarded, so the session keeps going whatever the child did.
            io.run_shell(core::str::from_utf8(&cmd[1..len]).map_err(|_| err("invalid content"))?);
            if !sess.scripted {
                io.emit(b"!\n");
            }
            Ok(Flow::Continue)
        }
    }
}

/// The execution half of `s` (C `main.c:698-770` + `sub.c:49-240`):
///
/// 1. **Leading flags** (may be written in sequence; after they start, any
///    non-flag is "invalid command suffix"): `g` global, `p` print the replaced
///    line, `r` reuse the previous replacement, a digit = the N-th match. An
///    empty tail (bare `s`) = replay everything.
/// 2. **Tail form** `<delim>pat<delim>repl<delim?>[g|N]?`: the delimiter may be
///    any character (a space is "invalid pattern delimiter"), `%%<delim>` at
///    `repl` reuses the previous replacement ("no previous substitution"), and
///    the closing delimiter may be omitted at end of line; the following `p`/
///    `l`/`n` belong to the same family as the modifier bits (C's
///    GET_COMMAND_SUFFIX).
/// 3. **Pattern cache**: a non-empty pattern is compiled and remembered at once
///    (C `pat = tpat`, main.c:739); an empty pattern (`//`) reuses the previous
///    one ("no previous pattern", re.c:69). The range defaults to the current
///    line; the last line where a replacement happened becomes the new current
///    line; with no replacement anywhere, the answer is "no match" (C
///    `search_and_replace`, sub.c:141-181).
fn substitute_command<S: TextStore, I: EditorIo>(
    store: &mut S,
    sess: &mut Session,
    io: &mut I,
    line: &str,
    cursor: usize,
    modifiers: &Modifiers,
    range: &AddressRange,
) -> Result<Flow, ExecError> {
    let rest = &line[cursor..];
    let bytes = rest.as_bytes();
    // ── leading flags ──
    let mut scope_override: Option<SubstScope> = None;
    let mut reuse_replacement = false;
    let mut print_replaced = false;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'g' => {
                scope_override = Some(SubstScope::Global);
                at += 1;
            }
            b'p' => {
                print_replaced = true;
                at += 1;
            }
            b'r' => {
                reuse_replacement = true;
                at += 1;
            }
            b'0'..=b'9' => {
                let start = at;
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    at += 1;
                }
                let n: u32 = rest[start..at].parse().map_err(|_| err("invalid command suffix"))?;
                if n == 0 {
                    return Err(err("invalid command suffix"));
                }
                scope_override = Some(SubstScope::Nth(n));
            }
            _ => break,
        }
    }
    if at > 0 && at < bytes.len() {
        // Once flags have started, only flags may follow until end of line (the
        // default arm of C `while (sflags && *ibufp != '\n')`, main.c:719-722).
        return Err(err("invalid command suffix"));
    }
    let flagged = at > 0;
    // ── where pattern and replacement come from ── (this crate has no heap:
    // sources are first copied into local fixed-size buffers)
    let mut pattern_buf = [0u8; MAX_TEXT];
    let mut pattern_len: usize;
    let mut repl_buf = [0u8; MAX_TEXT];
    let repl_len: usize;
    let mut scope;
    if reuse_replacement {
        // `sr` (the SGR half of C main.c:730-737): read a **new pattern** from
        // the tail while the replacement and scope stay as before
        // (`sgflag`/`sgnum` untouched); an empty tail is the same as bare `s`.
        let Some(prev) = sess.last_scope else {
            return Err(err("no previous substitution"));
        };
        scope = prev;
        repl_len = sess.last_replacement_len;
        repl_buf[..repl_len].copy_from_slice(sess.last_replacement_bytes());
        pattern_len = 0;
        if !bytes.is_empty() {
            let delim = bytes[0];
            if delim == b' ' {
                return Err(err("invalid pattern delimiter"));
            }
            let (pspan, _after) =
                crate::addr::scan_pattern(bytes, 1, delim).map_err(map_store_error)?;
            pattern_len = pspan.len as usize;
            pattern_buf[..pattern_len].copy_from_slice(&bytes[1..1 + pattern_len]);
        }
    } else if bytes.is_empty() || flagged {
        // Bare or flagged form: replay everything (C's sflags gate
        // `if (sflags && !pat)` -> "no previous substitution", main.c:725-728).
        let Some(prev_scope) = sess.last_scope else {
            return Err(err("no previous substitution"));
        };
        pattern_len = sess.last_pattern_len;
        pattern_buf[..pattern_len].copy_from_slice(sess.last_pattern_bytes());
        repl_len = sess.last_replacement_len;
        repl_buf[..repl_len].copy_from_slice(sess.last_replacement_bytes());
        scope = scope_override.unwrap_or(prev_scope);
    } else {
        let delim = bytes[0];
        if delim == b' ' {
            return Err(err("invalid pattern delimiter"));
        }
        let (pspan, after_pattern) =
            crate::addr::scan_pattern(bytes, 1, delim).map_err(map_store_error)?;
        pattern_len = pspan.len as usize;
        pattern_buf[..pattern_len].copy_from_slice(&bytes[1..1 + pattern_len]);
        let mut r_at = after_pattern;
        if bytes.get(r_at) == Some(&b'%') && bytes.get(r_at + 1) == Some(&delim) {
            // `%%<delim>`: reuse the previous replacement (the rhbuf branch of
            // C `extract_subst_template`, sub.c:82-89).
            if sess.last_replacement_len == 0 {
                return Err(err("no previous substitution"));
            }
            repl_len = sess.last_replacement_len;
            repl_buf[..repl_len].copy_from_slice(sess.last_replacement_bytes());
            r_at += 2;
        } else {
            let (rspan, r_next) =
                crate::addr::scan_pattern(bytes, r_at, delim).map_err(map_store_error)?;
            repl_len = rspan.len as usize;
            repl_buf[..repl_len].copy_from_slice(&bytes[r_at..r_at + repl_len]);
            r_at = r_next;
        }
        scope = SubstScope::First;
        match bytes.get(r_at) {
            Some(b'g') => {
                scope = SubstScope::Global;
            }
            Some(b'0'..=b'9') => {
                let start = r_at;
                while r_at < bytes.len() && bytes[r_at].is_ascii_digit() {
                    r_at += 1;
                }
                let n: u32 = rest[start..r_at].parse().map_err(|_| err("invalid pattern delimiter"))?;
                if n == 0 {
                    return Err(err("invalid pattern delimiter"));
                }
                scope = SubstScope::Nth(n);
            }
            _ => {}
        }
    }
    // An empty pattern reuses the previous one (C re.c:66-72).
    if pattern_len == 0 {
        pattern_len = sess.last_pattern_len;
        pattern_buf[..pattern_len].copy_from_slice(sess.last_pattern_bytes());
    }
    if pattern_len == 0 {
        return Err(err("no previous pattern"));
    }
    // Remember them (C main.c:739-743 and sub.c's rhbuf: recorded as soon as
    // parsing succeeds, without looking back at whether the substitution
    // succeeded).
    sess.remember_pattern(&pattern_buf[..pattern_len])?;
    sess.remember_replacement(&repl_buf[..repl_len])?;
    sess.last_scope = Some(scope);
    let pattern_text =
        core::str::from_utf8(&pattern_buf[..pattern_len]).map_err(|_| err("invalid content"))?;
    let replacement_text =
        core::str::from_utf8(&repl_buf[..repl_len]).map_err(|_| err("invalid content"))?;
    let compiled = compile_basic(pattern_text).map_err(|_| err("invalid pattern"))?;
    let template = Subst {
        pattern_text: "",
        replacement: Cow::Borrowed(replacement_text),
        scope,
        print: false,
    };
    // The range defaults to the current line (C
    // `check_addr_range(current_addr, current_addr)`, main.c:760-761). With no
    // current line (empty buffer) C scans its line 0 head (the empty string)
    // and reports "no match" on a miss -- this model has no line 0, so it folds
    // to the same error faithfully (registered deviation: a pattern that can
    // match the empty string would really insert a line in C; that behavior is
    // unknown here).
    if sess.current == 0 && range.first.is_none() && range.second.is_none() {
        return Err(err("no match"));
    }
    // C `main.c:684`: inside a global, `s`'s stack clearing is suppressed by
    // isglobal -- a whole global run is one undo unit.
    if !sess.is_global {
        clear_undo(store, sess);
    }
    let mut probe = StoreSearch { store, sess: &*sess };
    let (from, to) = evaluate_range_with(
        range,
        &context_of(store, sess),
        (sess.current, sess.current),
        line,
        &mut probe,
    )
    .map_err(map_store_error)?;
    // The probe's borrow ends here: the insert/delete below takes a mutable
    // borrow (NLL settles it per path).
    let original_current = sess.current;
    let mut last_changed: Option<usize> = None;
    let mut n = from;
    while n <= to {
        let mut buf = [0u8; MAX_TEXT];
        let used = store.read_line(n, &mut buf).map_err(map_store_error)?;
        let line_text = core::str::from_utf8(&buf[..used]).map_err(|_| err("invalid content"))?;
        let mut out = [0u8; MAX_TEXT];
        let (len, replaced) = subst_apply(&compiled, &template, line_text.as_bytes(), &mut out);
        if replaced {
            // C: delete_lines(current, current) + put_sbuf_line per line
            // (sub.c:148-163); the replacement template never introduces a
            // newline inside a line (a backslash-newline in the template is the
            // interactive continuation face, and this model's lines carry no
            // newline), so this is a single-line delete plus insert. Both go
            // through the undo-recording helpers (UDEL + UADD, sub.c:152).
            let text = core::str::from_utf8(&out[..len]).map_err(|_| err("invalid content"))?;
            delete_range(store, sess, n, n)?;
            insert_line(store, sess, n, text)?;
            last_changed = Some(n);
        }
        n += 1;
    }
    sess.current = last_changed.unwrap_or(original_current);
    // C `sub.c:175`'s `!(gflag & GLB)`: inside a global, an `s` with no match
    // is not an error (main.c:755 sets GLB).
    if last_changed.is_none() && !sess.is_global {
        return Err(err("no match"));
    }
    // Suffix printing (C: SGP folds into GPR and clears GLS|GNP,
    // main.c:752-754; the rest belongs to the GET_COMMAND_SUFFIX family).
    let mut bits = suffix_bits(modifiers)?;
    if print_replaced {
        bits = GPR;
    }
    finish(store, sess, io, bits)
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
        EditorError::NoMatch => err("no match"),
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
        /// Commands handed to [`EditorIo::run_shell`], in order.
        shell_cmds: Vec<String>,
    }

    impl ScriptIo {
        fn new() -> Self {
            ScriptIo {
                out: Vec::new(),
                err_out: Vec::new(),
                files: BTreeMap::new(),
                shell_cmds: Vec::new(),
            }
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
        fn run_shell(&mut self, cmd: &str) {
            self.shell_cmds.push(String::from(cmd));
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

    /// The tail forms of `s` (C main.c:698-770 + sub.c): first substitution,
    /// `g` global, `N` the N-th, `p` print; the range defaults to the current
    /// line; the replaced line becomes the new current line; no replacement
    /// answers "no match".
    #[test]
    fn test_substitute_tail_forms() {
        let (mut store, mut sess) = seeded(&["foo boo", "bar"]);
        let mut io = ScriptIo::new();
        // `2s/a/0/`: applies to the second line, first substitution.
        step(&mut store, &mut sess, "2s/a/0/", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..3], b"b0r");
        // `s` on the current line: `1s/o/0/` replaces only the first o.
        step(&mut store, &mut sess, "1s/o/0/", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"f0o boo");
        assert_eq!(sess.current, 1, "替换行成为当前行");
        // `g` global.
        step(&mut store, &mut sess, "1s/o/0/g", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"f00 b00");
        // `N`: the second match.
        let (mut store, mut sess) = seeded(&["foo boo"]);
        sess.current = 1;
        step(&mut store, &mut sess, "s/o/0/2", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"fo0 boo");
        // No match: "no match".
        let (mut store, mut sess) = seeded(&["abc"]);
        assert_eq!(
            step(&mut store, &mut sess, "s/z/q/", &mut io).unwrap_err().message,
            "no match"
        );
    }

    /// Bare `s` and `sg`/`sN` replay the previous substitution (C's sflags
    /// gate: without a previous substitution the answer is "no previous
    /// substitution"; the empty pattern `//` reuses the previous pattern,
    /// re.c:66).
    #[test]
    fn test_substitute_replay_and_pattern_cache() {
        let (mut store, mut sess) = seeded(&["aa", "ab"]);
        let mut io = ScriptIo::new();
        sess.current = 1;
        step(&mut store, &mut sess, "s/a/x/", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..2], b"xa");
        // Bare `s`: replay (the range is still the current line).
        sess.current = 2;
        step(&mut store, &mut sess, "s", &mut io).unwrap();
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..2], b"xb");
        // The empty pattern `//` reuses the previous pattern.
        let (mut store, mut sess) = seeded(&["k1", "k2"]);
        sess.current = 1;
        step(&mut store, &mut sess, "s/k/z/", &mut io).unwrap();
        step(&mut store, &mut sess, "2s//z/", &mut io).unwrap();
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..2], b"z2");
        // Refused without a previous substitution.
        let (mut store, mut sess) = seeded(&["x"]);
        assert_eq!(
            step(&mut store, &mut sess, "s", &mut io).unwrap_err().message,
            "no previous substitution"
        );
        // Refused without a previous pattern.
        assert_eq!(
            step(&mut store, &mut sess, "s//y/", &mut io).unwrap_err().message,
            "no previous pattern"
        );
    }

    /// The replacement template's `&` and group replay (C's regsub semantics,
    /// the same source as the doc 08 engine).
    #[test]
    fn test_substitute_replacement_replay() {
        let (mut store, mut sess) = seeded(&["hello world"]);
        let mut io = ScriptIo::new();
        sess.current = 1;
        // `\+` is the extended-regex quantifier (pattern.rs:305 accepts it only
        // in extended mode); BRE uses literal groups.
        step(&mut store, &mut sess, "s/\\(ll\\)/[&]/", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..13], b"he[ll]o world");
    }

    /// Search-address evaluation: forward, backward, wrapping, a `+N` offset,
    /// and "no match" (the wrap order of C `get_matching_node_addr`,
    /// main.c:919-938).
    #[test]
    fn test_search_addresses_evaluate() {
        let (mut store, mut sess) = seeded(&["alpha", "beta", "gamma", "beta"]);
        let mut io = ScriptIo::new();
        sess.current = 4;
        // Forward: no beta after 4, so it wraps back to 2.
        step(&mut store, &mut sess, "/beta/", &mut io).unwrap();
        assert_eq!(sess.current, 2, "裸搜索地址显示并把当前行移过去");
        // Backward: from 2 the way back is 4.
        sess.current = 2;
        step(&mut store, &mut sess, "?beta?d", &mut io).unwrap();
        assert_eq!(store.line_count(), 3, "反向搜到的行（4）被删除");
        // The offset follows the search base.
        let (mut store, mut sess) = seeded(&["alpha", "mid", "omega"]);
        sess.current = 1;
        step(&mut store, &mut sess, "/alpha/+1", &mut io).unwrap();
        assert_eq!(sess.current, 2);
        // No match.
        sess.current = 1;
        assert_eq!(
            step(&mut store, &mut sess, "/zebra/", &mut io).unwrap_err().message,
            "no match"
        );
        // An empty buffer has nothing to search.
        let (mut store, mut sess) = seeded(&[]);
        assert_eq!(
            step(&mut store, &mut sess, "/x/", &mut io).unwrap_err().message,
            "invalid address"
        );
    }

    /// `u`'s undo and redo (C undo.c:71-105's replay and snapshot swap): after
    /// `d`, `u` restores the lines and the current line, and another `u` redoes
    /// the deletion; `modified` goes dirty on undo.
    #[test]
    fn test_undo_restores_delete_then_redoes() {
        let (mut store, mut sess) = seeded(&["alpha", "beta", "gamma"]);
        sess.current = 1;
        step(&mut store, &mut sess, "2,3d", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 1);
        // Undo: the lines come back and the current line returns to its
        // pre-change value (1).
        step(&mut store, &mut sess, "u", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 3);
        let mut line = [0u8; 64];
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"beta");
        assert_eq!(sess.current, 1, "当前行取改动前快照");
        assert!(sess.modified, "撤销把缓冲标脏（C undo.c:91）");
        // Redo: delete once more.
        step(&mut store, &mut sess, "u", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 1);
        // A third `u` undoes again -- the flipped stack keeps cycling.
        step(&mut store, &mut sess, "u", &mut io_none()).unwrap();
        assert_eq!(store.line_count(), 3);
    }

    /// Undo of an insertion (the UADD entries from `a`) and the "nothing to
    /// undo" start.
    #[test]
    fn test_undo_restores_append_and_reports_empty() {
        let (mut store, mut sess) = seeded(&["one"]);
        sess.current = 1;
        let mut io = io_none();
        assert_eq!(
            step(&mut store, &mut sess, "u", &mut io).unwrap_err().message,
            "nothing to undo",
            "未做任何改动前 `u` 拒绝（C undo.c:76-79）"
        );
        // `a` inserts two lines and one `u` takes both back (one UADD entry per
        // line, replayed in reverse, each deleting its line).
        step(&mut store, &mut sess, "a", &mut io).unwrap();
        step(&mut store, &mut sess, "two", &mut io).unwrap();
        step(&mut store, &mut sess, "three", &mut io).unwrap();
        step(&mut store, &mut sess, ".", &mut io).unwrap();
        assert_eq!(store.line_count(), 3);
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 1, "两行插入被整批撤销");
        // Redo puts the two lines back.
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 3);
        let mut line = [0u8; 64];
        store.read_line(3, &mut line).unwrap();
        assert_eq!(&line[..5], b"three");
    }

    /// Undo of a substitution: the original text is restored (C
    /// `search_and_replace`'s UDEL+UADD pair, sub.c:148-163).
    #[test]
    fn test_undo_restores_substitute() {
        let (mut store, mut sess) = seeded(&["foo boo", "keep"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "1,2s/o/0/g", &mut io).unwrap();
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"f00 b00");
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..7], b"foo boo");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"keep", "未替换的行不受影响");
        // A new change wipes the old undo record (C: every modifying command
        // calls clear_undo_stack first).
        step(&mut store, &mut sess, "2d", &mut io).unwrap();
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 2, "撤销指向最近一次改动（d）");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"keep");
    }

    /// `g/pat/cmd`: run the subcommand for each matching line (exec_global in C
    /// `glbl.c`). The classic three: delete every match, substitute line by
    /// line, print.
    #[test]
    fn test_global_delete_substitute_and_print() {
        // `g/x/d`: delete every matching line.
        let (mut store, mut sess) = seeded(&["x1", "keep", "x2", "also"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "g/x/d", &mut io).unwrap();
        assert_eq!(store.line_count(), 2);
        let mut line = [0u8; 64];
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..4], b"keep");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..4], b"also");
        // `v/x/d`: delete every non-matching line (the inverse).
        let (mut store, mut sess) = seeded(&["x1", "keep", "x2"]);
        sess.current = 1;
        step(&mut store, &mut sess, "v/x/d", &mut io).unwrap();
        assert_eq!(store.line_count(), 2);
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..2], b"x1");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..2], b"x2");
        // `g/o/s/o/0/`: substitute line by line (first match per line); the
        // suffix `p` prints through the subcommand's own channel.
        let (mut store, mut sess) = seeded(&["foo", "boo"]);
        sess.current = 1;
        step(&mut store, &mut sess, "g/o/s/o/0/", &mut io).unwrap();
        store.read_line(1, &mut line).unwrap();
        assert_eq!(&line[..3], b"f0o");
        store.read_line(2, &mut line).unwrap();
        assert_eq!(&line[..3], b"b0o");
    }

    /// Undo of a global: a whole global run is **one** undo unit (C
    /// exec_global:143 clears the stack once, and stack clearing inside
    /// subcommands is suppressed by isglobal).
    #[test]
    fn test_global_undo_is_one_unit() {
        let (mut store, mut sess) = seeded(&["x1", "keep", "x2"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "g/x/d", &mut io).unwrap();
        assert_eq!(store.line_count(), 1);
        step(&mut store, &mut sess, "u", &mut io).unwrap();
        assert_eq!(store.line_count(), 3, "一次 `u` 撤销整段全局");
        // A nested `g` is refused (C main.c:562-564).
        step(&mut store, &mut sess, "g/x/g/x/d", &mut io).unwrap_err();
        // Error abort: a subcommand error aborts the whole run (changes already
        // made are kept, as in C).
        let (mut store, mut sess) = seeded(&["a1", "a2"]);
        sess.current = 1;
        // `y` has no command letter of its own (C's dispatch table has no such
        // arm).
        assert_eq!(
            step(&mut store, &mut sess, "g/a/y", &mut io).unwrap_err().message,
            "unknown command"
        );
    }

    /// An empty subcommand does not imply `p` (this build has no BACKWARDS,
    /// glbl.c:56-63): the current line merely moves to the last active line.
    #[test]
    fn test_global_empty_cmd_moves_current_only() {
        let (mut store, mut sess) = seeded(&["m1", "mid", "m2"]);
        sess.current = 1;
        let mut io = io_none();
        step(&mut store, &mut sess, "g/m/", &mut io).unwrap();
        assert_eq!(sess.current, 3, "当前行落到最后一个活跃行");
        assert_eq!(
            io.out_text(),
            "",
            "空子命令不打印（非 BACKWARDS 构建）"
        );
    }

    fn io_none() -> ScriptIo {
        ScriptIo::new()
    }

    /// Read the whole buffer back (test assertion helper).
    fn buffer_lines<S: TextStore>(store: &S) -> Vec<String> {
        let mut buf = [0u8; MAX_TEXT];
        let mut lines = Vec::new();
        for n in 1..=store.line_count() {
            let used = store.read_line(n, &mut buf).unwrap();
            lines.push(String::from_utf8(buf[..used].to_vec()).unwrap());
        }
        lines
    }

    #[test]
    fn test_interactive_global_edits_each_match_and_replays() {
        let (mut store, mut sess) = seeded(&["one", "two", "three", "four"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["G/o", "s/o/0", "&", ""]);
        // Each active line is displayed and then run; `&` replays the previous
        // one; an empty line skips (glbl.c:111-134).
        assert_eq!(io.out_lines(), ["one", "two", "four"]);
        assert_eq!(buffer_lines(&store), ["0ne", "tw0", "three", "four"]);
        assert_eq!(sess.current, 4, "最后一个活跃行落当前行");
    }

    #[test]
    fn test_interactive_global_v_inverse_and_number_suffix() {
        let (mut store, mut sess) = seeded(&["one", "two", "three", "four"]);
        let mut io = ScriptIo::new();
        // `V` collects non-matching lines, and the `n` suffix numbers the
        // display (C main.c:566's suffix).
        feed(&mut store, &mut sess, &mut io, &["V/e/n", "", ""]);
        assert_eq!(io.out_lines(), ["2\ttwo", "4\tfour"]);
        assert_eq!(buffer_lines(&store), ["one", "two", "three", "four"], "全跳过：缓冲不动");
    }

    #[test]
    fn test_interactive_global_amp_needs_a_previous_command() {
        let (mut store, mut sess) = seeded(&["a", "b"]);
        let mut io = ScriptIo::new();
        // `&` at the start: this run has not supplied a command yet
        // (glbl.c:121-125) -- the whole run aborts.
        assert_eq!(step(&mut store, &mut sess, "G/a", &mut io).unwrap(), Flow::Continue);
        assert_eq!(
            step(&mut store, &mut sess, "&", &mut io).unwrap_err().message,
            "no previous command"
        );
        assert_eq!(store.line_count(), 2, "中止不动缓冲");
        // `seen` starts over for each `G` run: the `&` at the start of a new run
        // is refused the same way.
        feed(&mut store, &mut sess, &mut io, &["G/a", "d"]);
        assert_eq!(store.line_count(), 1);
        assert_eq!(step(&mut store, &mut sess, "G/b", &mut io).unwrap(), Flow::Continue);
        assert_eq!(
            step(&mut store, &mut sess, "&", &mut io).unwrap_err().message,
            "no previous command"
        );
    }

    #[test]
    fn test_interactive_global_delete_relocates_and_rejects_nesting() {
        let (mut store, mut sess) = seeded(&["a", "b", "a", "b"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["G/a", "d", "d"]);
        assert_eq!(buffer_lines(&store), ["b", "b"], "两个活跃行都按内容重定位后删除");
        // A nested g in an answer is refused (the isglobal gate at C
        // main.c:562-564).
        assert_eq!(step(&mut store, &mut sess, "G/b", &mut io).unwrap(), Flow::Continue);
        assert_eq!(
            step(&mut store, &mut sess, "g/x/d", &mut io).unwrap_err().message,
            "cannot nest global commands"
        );
        // After the abort, dispatch is back to normal: the next line is no
        // longer an answer.
        let before = io.out_lines();
        assert_eq!(step(&mut store, &mut sess, "1,$p", &mut io).unwrap(), Flow::Continue);
        assert_eq!(io.out_lines()[before.len()..], ["b", "b"]);
    }

    #[test]
    fn test_interactive_global_undo_is_one_unit() {
        let (mut store, mut sess) = seeded(&["one", "two", "four"]);
        let mut io = ScriptIo::new();
        feed(&mut store, &mut sess, &mut io, &["G/o", "s/o/0", "&", "&"]);
        // The stack is cleared once -- a whole `G` run is one undo unit (C
        // exec_global:143).
        feed(&mut store, &mut sess, &mut io, &["u"]);
        assert_eq!(buffer_lines(&store), ["one", "two", "four"], "一次 u 翻掉整段 G");
        feed(&mut store, &mut sess, &mut io, &["u"]);
        assert_eq!(buffer_lines(&store), ["0ne", "tw0", "f0ur"], "再 u 即重做");
    }

    #[test]
    fn test_interactive_global_unknown_answer_aborts_session() {
        let (mut store, mut sess) = seeded(&["x", "y"]);
        let mut io = ScriptIo::new();
        assert_eq!(step(&mut store, &mut sess, "G/x", &mut io).unwrap(), Flow::Continue);
        // A bad answer (unknown command) aborts the whole `G` run; changes
        // already made are kept (as in C).
        assert_eq!(
            step(&mut store, &mut sess, "~", &mut io).unwrap_err().message,
            "unknown command"
        );
        assert_eq!(
            step(&mut store, &mut sess, "1,$p", &mut io).unwrap(),
            Flow::Continue,
            "G 中止后输入回到正常派发"
        );
    }

    #[test]
    fn test_declared_gaps_answer_through_the_question_channel() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
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

    // ── the shell escape (`!`, `main.c:867-877` + `main.c:992-1048`) ──

    #[test]
    fn test_shell_command_runs_and_prompts() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        // A fresh command echoes nothing (no substitution) and prints the
        // `!` confirmation unless scripted (`main.c:875-877`).
        step(&mut store, &mut sess, "!echo hi", &mut io).unwrap();
        assert_eq!(io.shell_cmds, ["echo hi"]);
        assert_eq!(io.out_text(), "!\n");
        // Scripted mode suppresses the prompt.
        let mut scripted = Session::new(true, false, false, None);
        let mut io2 = ScriptIo::new();
        step(&mut store, &mut scripted, "!echo hi", &mut io2).unwrap();
        assert_eq!(io2.out_text(), "");
    }

    #[test]
    fn test_shell_repeat_substitutes_and_echoes() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        // A repeat with no previous command is an error (`main.c:1015-1021`).
        assert_eq!(
            step(&mut store, &mut sess, "!!", &mut io)
                .unwrap_err()
                .message,
            "no previous command"
        );
        step(&mut store, &mut sess, "!echo hi", &mut io).unwrap();
        // `!!` replays and echoes the expanded command (`main.c:875`).
        step(&mut store, &mut sess, "!!", &mut io).unwrap();
        assert_eq!(io.shell_cmds, ["echo hi", "echo hi"]);
        assert_eq!(io.out_text(), "!\necho hi\n!\n");
    }

    #[test]
    fn test_shell_percent_substitutes_the_filename() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        // No current name yet (`main.c:1027-1030`).
        assert_eq!(
            step(&mut store, &mut sess, "!touch %", &mut io).unwrap_err().message,
            "no current filename"
        );
        sess.set_filename("notes.txt").unwrap();
        step(&mut store, &mut sess, "!touch %", &mut io).unwrap();
        assert_eq!(io.shell_cmds, ["touch notes.txt"]);
        // A `%` substitution echoes the expansion (`main.c:875`).
        assert_eq!(io.out_text(), "touch notes.txt\n!\n");
    }

    #[test]
    fn test_shell_escapes_and_literal_bangs_survive() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        // Escapes copy verbatim (`main.c:1012-1014`): the shell re-reads
        // the backslash.
        step(&mut store, &mut sess, "!echo a\\ b", &mut io).unwrap();
        assert_eq!(io.shell_cmds, ["echo a\\ b"]);
        // A `!` past first position is literal (`main.c:1005-1011`).
        step(&mut store, &mut sess, "!echo one!two", &mut io).unwrap();
        assert_eq!(io.shell_cmds, ["echo a\\ b", "echo one!two"]);
    }

    #[test]
    fn test_shell_refuses_addresses_and_restricted_names() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        // An address is "unexpected address" (`main.c:869-872`).
        assert_eq!(
            step(&mut store, &mut sess, "1!echo hi", &mut io)
                .unwrap_err()
                .message,
            "unexpected address"
        );
        assert!(io.shell_cmds.is_empty());
        // `-S` (`secure`) refuses before reading (`main.c:994-997`).
        let mut secure = Session::new(false, true, false, None);
        assert_eq!(
            step(&mut store, &mut secure, "!echo hi", &mut io).unwrap_err().message,
            "shell access restricted"
        );
    }

    // ── `!command` file names (`get_filename`'s shell branch, `main.c:956-963`) ──

    #[test]
    fn test_read_from_shell_pipe_feeds_the_command_name_through() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        io.files.insert("!echo piped".to_string(), b"piped\n".to_vec());
        step(&mut store, &mut sess, "$r !echo piped", &mut io).unwrap();
        assert_eq!(buffer_lines(&store), ["a", "piped"]);
        // The pipe target never becomes the default name (`main.c:792-794`).
        assert_eq!(sess.filename_len, 0);
        // The name parse runs nothing (`get_filename` only builds the
        // buffer); the popen leg lives in the io seam.
        assert!(io.shell_cmds.is_empty());
    }

    #[test]
    fn test_write_to_shell_pipe_and_invalid_redirection() {
        let (mut store, mut sess) = seeded(&["one", "two"]);
        let mut io = ScriptIo::new();
        step(&mut store, &mut sess, "w !cat", &mut io).unwrap();
        // The buffer text reached the pipe name (`io.c:157`, popen "w").
        assert_eq!(io.files["!cat"], b"one\ntwo\n".to_vec());
        // `f !cmd` is "invalid redirection" (`main.c:546-549`).
        assert_eq!(
            step(&mut store, &mut sess, "f !cat", &mut io).unwrap_err().message,
            "invalid redirection"
        );
    }

    #[test]
    fn test_shell_name_memory_is_shared_between_command_and_filename() {
        let (mut store, mut sess) = seeded(&["a"]);
        let mut io = ScriptIo::new();
        io.files.insert("!echo shared".to_string(), b"x\n".to_vec());
        // The parse half of a `!command` name fills the replay buffer too
        // (C's `shcmd` is one static, `main.c:1041-1044`).
        step(&mut store, &mut sess, "$r !echo shared", &mut io).unwrap();
        step(&mut store, &mut sess, "!!", &mut io).unwrap();
        // The `!!` replayed the name's command — one run, recorded here.
        assert_eq!(io.shell_cmds, ["echo shared"]);
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
