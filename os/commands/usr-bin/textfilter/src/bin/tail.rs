//! Minix-RS tail — last lines of a stream.
//!
//! Ground truth: `minix3/usr.bin/tail/tail.c` (NetBSD), `tail [-n count |
//! -count] [file ...]` with a default of ten lines. The ring holds as many
//! borrowed lines as the count asks for and no more than the input offered, so
//! any count is honoured the way C's backward walk through the mapped file
//! honours one. With no operand tail reads standard input; with
//! operands it prints the last `count` lines of each file, prefixing a
//! `==> name <==` banner when there is more than one file (this NetBSD `tail`
//! has no `-q`/`-v` to change that) with a blank line between files, and an
//! unopenable/unstatable file warns and is skipped (exit status stays
//! nonzero). Operands open with `fopen`, so `-` is a literal file name.
//!
//! Follow mode is modelled on the shape the C actually takes on Minix. The
//! kqueue arm of `forward()`'s loop is excluded by `#if !defined(__minix)`
//! (`forward.c:207-246`), so what is left is: copy from the handle's position to
//! the end, `fflush`, `sleep(1)` (`:191-200`, `:253`), and — only for `-F` on a
//! named operand (`:255`) — compare `st_ino`/`st_dev`/`st_rdev`/`st_nlink` and
//! reopen when they differ (`:257-266`). Because the position belongs to the open
//! handle, a file that shrinks does not rewind it: repositioning to the end lives
//! only in the excluded kqueue branch (`:238-243`), and the reference run of the
//! C confirms the surviving shape is a mid-line fragment, not a re-sent line.
//!
//! Two legs never reach that loop: a follow flag with more than one operand exits
//! before anything is opened (`tail.c:135-137`), and `-r` with a follow flag is a
//! usage error (`:143-145`). A follow flag on a pipe standard input is dropped
//! outright (`:197-201`, "POSIX.2 requires this").
//!
//! Byte mode `-c N`, block mode `-b N`, reverse mode `-r` and the forward
//! offsets `-n +N` / `-c +N` / `-b +N` are all modelled — C
//! `tail.c`/`forward.c`/`reverse.c`: `-r` prints lines bottom-up (`REVERSE`
//! reverses every line, `RLINES` = an explicit `-n` reverses the last `count`),
//! `-r` combined with `-c`/`-b` is the `RBYTES` byte-window line reverse
//! (`reverse_bytes`), and a `+N` value takes the forward arm of the `ARG` macro
//! so `tail -n +20` starts at line 20 rather than taking the last twenty lines.
//!
//! One declared simplification, registered rather than silently accepted: when a
//! read fails this shell reports the operand as failed and prints nothing, while
//! C's forward arms (`FBYTES`/`FLINES`) have already copied the bytes they read
//! before the failure (`forward.c:112-134`). The difference shows only on a read
//! that fails mid-stream, which neither the host gates nor the current machine
//! gates can produce deterministically.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;
use alloc::format;
use alloc::vec;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::window::{
    Follow, LineWindow, StatIdentity, TailError, TailStyle, TailWindow, parse_tail_args,
    reopen_needed,
};

// ---------------------------------------------------------------------------
// The twin seam of the follow loop
//
// Why this lives at the shell layer rather than in `bin_support`: these seams
// serve only the one shape "hold an already-opened handle and keep reading", and
// in this crate only `tail` uses it today. `bin_support` is the shared face that
// about thirty binaries each pull in via `#[path]`; adding an interface for one
// user would spread the change over the whole family. Promote it once a second
// follower appears.
// ---------------------------------------------------------------------------

/// The `fp` of C `forward(FILE *fp, ...)`. It must stay open for the whole
/// follow: "how far we have read" is a property of the handle, not arithmetic on
/// the file size (`forward.c:191-195` copies from the current position to EOF on
/// every round, and does not rewind when the file shrinks — repositioning exists
/// only in the excluded kqueue arm `:238-243`).
#[cfg(any(test, not(target_os = "none")))]
pub struct FollowSource {
    /// The handle is wrapped uniformly in `ManuallyDrop`, with the `Drop` below
    /// deciding whether to close it: C's `fclose` applies only to operands
    /// (`tail.c:183`), and the standard-input leg is never closed.
    file: std::mem::ManuallyDrop<std::fs::File>,
    is_stdin: bool,
}

#[cfg(any(test, not(target_os = "none")))]
impl Drop for FollowSource {
    fn drop(&mut self) {
        if !self.is_stdin {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.file) };
        }
    }
}

#[cfg(all(not(test), target_os = "none"))]
pub struct FollowSource {
    fd: minix_sys::Fd,
    is_stdin: bool,
}

// The host-errno → C `errno` conversion lives in `support::errno_of` (the same
// convention as `read_file` and `open_input`), and this file does not write a
// second copy: C's `ierr()` goes through the numbered `xerr`/`vwarn`
// (`misc.c:53-57`, `:65-76`), and two conversions would drift apart eventually.

impl FollowSource {
    /// C `fopen(fname, "r")` (`tail.c:167`).
    #[cfg(any(test, not(target_os = "none")))]
    fn open(path: &str) -> Result<Self, minix_sys::Errno> {
        std::fs::File::open(path)
            .map(|file| FollowSource {
                file: std::mem::ManuallyDrop::new(file),
                is_stdin: false,
            })
            .map_err(|e| support::errno_of(&e))
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn open(path: &str) -> Result<Self, minix_sys::Errno> {
        // `O_RDONLY` is zero — the same argument set the read seam in
        // `bin_support` uses.
        minix_sys::open(path, 0, 0).map(|fd| FollowSource {
            fd,
            is_stdin: false,
        })
    }

    /// The stdin leg when there is no operand. C writes `fname` as the literal
    /// `"stdin"` at `tail.c:186`, so the diagnostic line can pronounce that name
    /// too.
    #[cfg(any(test, not(target_os = "none")))]
    fn stdin() -> Self {
        use std::os::fd::FromRawFd as _;
        FollowSource {
            file: std::mem::ManuallyDrop::new(unsafe {
                std::fs::File::from_raw_fd(support::STDIN)
            }),
            is_stdin: true,
        }
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn stdin() -> Self {
        FollowSource {
            fd: support::STDIN,
            is_stdin: true,
        }
    }

    /// C `fstat(fileno(fp), &sb)` (`tail.c:168`, `:188`) — failing to open the
    /// operand **or** failing to get its identity is `ierr(); continue`, so this
    /// mouth is not optional.
    #[cfg(any(test, not(target_os = "none")))]
    fn identity(&self) -> Result<StatIdentity, minix_sys::Errno> {
        self.file
            .metadata()
            .map(host_identity)
            .map_err(|e| support::errno_of(&e))
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn identity(&self) -> Result<StatIdentity, minix_sys::Errno> {
        // Every field of `struct stat` is an integer or an integer timespec, so
        // all zeros is a legal instance — the same construction this crate uses
        // in `probe_path` and in `fileops`' `ls`/`test`.
        let mut stat: minix_sys::Stat = unsafe { core::mem::zeroed() };
        minix_sys::fstat(self.fd, &mut stat)?;
        Ok(target_identity(&stat))
    }

    /// C's positioning probe: `lseek(fileno(stdin), 0, SEEK_CUR) == -1 && errno
    /// == ESPIPE` (`tail.c:197-201`). Only this one number makes C give up
    /// following; other errors (say `EBADF`) still go on, so "any error means a
    /// pipe" is not allowed here either.
    #[cfg(any(test, not(target_os = "none")))]
    fn is_pipe_by_position(&mut self) -> bool {
        use std::io::Seek as _;
        // `SeekFrom::Current(0)` is exactly C's `lseek(fd, 0, SEEK_CUR)`: it asks
        // for the position without moving it, so the probe itself cannot affect
        // where the next round reads from.
        match self.file.seek(std::io::SeekFrom::Current(0)) {
            Err(error) => error.raw_os_error() == Some(minix_sys::ESPIPE),
            _ => false,
        }
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn is_pipe_by_position(&mut self) -> bool {
        // `SEEK_CUR` is 1 (`minix3/sys/sys/unistd.h:175`).
        match minix_sys::lseek(self.fd, 0, 1) {
            Err(error) => error.to_i32() == minix_sys::ESPIPE,
            _ => false,
        }
    }

    /// The initial whole block: collect the bytes after the current position
    /// (C's positioning and first byte-copying round both live in `forward()`'s
    /// switch and its first `getc` loop, `forward.c:101-178`, `:191-195`). It runs
    /// until EOF or an error, and the handle is not closed.
    #[cfg(any(test, not(target_os = "none")))]
    fn collect(&mut self) -> Result<Vec<u8>, minix_sys::Errno> {
        use std::io::Read as _;
        let mut data = Vec::new();
        self.file
            .read_to_end(&mut data)
            .map_err(|e| support::errno_of(&e))?;
        Ok(data)
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn collect(&mut self) -> Result<Vec<u8>, minix_sys::Errno> {
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match minix_sys::read(self.fd, &mut chunk) {
                Ok(0) => break,
                Ok(count) => data.extend_from_slice(&chunk[..count]),
                Err(error) => return Err(error),
            }
        }
        Ok(data)
    }

    /// One round of the follow loop: read and write as you go (C's `putchar`
    /// happens synchronously inside `getc`, `forward.c:192-195`), so bytes copied
    /// before a read failure have already gone out.
    #[cfg(any(test, not(target_os = "none")))]
    fn pump(&mut self) -> Result<(), minix_sys::Errno> {
        use std::io::Read as _;
        let mut chunk = [0u8; 4096];
        loop {
            match self.file.read(&mut chunk) {
                Ok(0) => return Ok(()),
                Ok(count) => support::emit(&chunk[..count]),
                Err(error) => return Err(support::errno_of(&error)),
            }
        }
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn pump(&mut self) -> Result<(), minix_sys::Errno> {
        let mut chunk = [0u8; 4096];
        loop {
            match minix_sys::read(self.fd, &mut chunk) {
                Ok(0) => return Ok(()),
                Ok(count) => support::emit(&chunk[..count]),
                Err(error) => return Err(error),
            }
        }
    }

    /// C `freopen(fname, "r", fp)` (`forward.c:261`): reopen the same operand by
    /// name, with the read position back at the start of the new file. The target
    /// side has no `freopen`, so the semantics are "drop the old one and open by
    /// name", and standard input never reaches here (`probes_identity` blocks it
    /// one layer up).
    #[cfg(any(test, not(target_os = "none")))]
    fn reopen(&mut self, path: &str) -> Result<(), minix_sys::Errno> {
        if self.is_stdin {
            return Err(minix_sys::Errno::EBADF);
        }
        let fresh = std::fs::File::open(path).map_err(|e| support::errno_of(&e))?;
        // C's `freopen` swaps the stream out and does not keep the old handle;
        // this object owns that release itself (`Drop` only runs on a handle that
        // was never reopened).
        unsafe { std::mem::ManuallyDrop::drop(&mut self.file) };
        self.file = std::mem::ManuallyDrop::new(fresh);
        Ok(())
    }

    #[cfg(all(not(test), target_os = "none"))]
    fn reopen(&mut self, path: &str) -> Result<(), minix_sys::Errno> {
        if self.is_stdin {
            return Err(minix_sys::Errno::EBADF);
        }
        let fresh = minix_sys::open(path, 0, 0)?;
        let _ = minix_sys::close(self.fd);
        self.fd = fresh;
        Ok(())
    }
}

/// The target-side release matches C `fclose(fp)` (`tail.c:183`): operands are
/// closed once read, standard input is not (C's leg does not `fclose(stdin)`
/// either). On the host side `File`'s destructor already has these semantics, and
/// the `ManuallyDrop` arm by construction never closes fd 0.
#[cfg(all(not(test), target_os = "none"))]
impl Drop for FollowSource {
    fn drop(&mut self) {
        if !self.is_stdin {
            let _ = minix_sys::close(self.fd);
        }
    }
}

/// The three things C `forward()` holds: the open handle, its identity (`sbp`),
/// and the operand name (the global `fname`, argv's raw bytes). Standard input has
/// no name, hence `Option<&[u8]>` here — C expresses the same thing with
/// `fp != stdin` (`forward.c:255`), and the deciding half's
/// [`Follow::probes_identity`] is the model of that test.
struct FollowTarget<'a> {
    source: FollowSource,
    opened: StatIdentity,
    path: Option<&'a [u8]>,
}

impl<'a> FollowTarget<'a> {
    fn is_stdin(&self) -> bool {
        self.path.is_none()
    }

    /// The name the diagnostic line pronounces: C's `ierr()` uses the global
    /// `fname`, and with no operand `tail.c:186` writes it as the literal
    /// `"stdin"`. The name is argv's raw bytes, so this hands back bytes rather
    /// than `&str`.
    fn label(&self) -> &'a [u8] {
        self.path.unwrap_or(b"stdin".as_slice())
    }
}

/// The four compared fields of the host `struct stat`
/// (`std::os::unix::fs::MetadataExt`).
#[cfg(any(test, not(target_os = "none")))]
fn host_identity(metadata: std::fs::Metadata) -> StatIdentity {
    use std::os::unix::fs::MetadataExt as _;
    StatIdentity {
        ino: metadata.ino(),
        dev: metadata.dev(),
        rdev: metadata.rdev(),
        nlink: metadata.nlink(),
    }
}

/// The same four fields on the target-side `Stat`.
#[cfg(all(not(test), target_os = "none"))]
fn target_identity(stat: &minix_sys::Stat) -> StatIdentity {
    StatIdentity {
        ino: stat.st_ino,
        dev: stat.st_dev,
        rdev: stat.st_rdev,
        nlink: u64::from(stat.st_nlink),
    }
}

/// C `stat(fname, &statbuf)` (`forward.c:256`): look up the operand's identity by
/// name and compare it with the identity of the handle in hand. A name that no
/// longer opens (deleted after the swap) is caught by C's `!= -1` test, and that
/// round does nothing while the next keeps sleeping.
#[cfg(any(test, not(target_os = "none")))]
fn stat_by_path(path: &str) -> Result<StatIdentity, minix_sys::Errno> {
    std::fs::metadata(path)
        .map(host_identity)
        .map_err(|e| support::errno_of(&e))
}

#[cfg(all(not(test), target_os = "none"))]
fn stat_by_path(path: &str) -> Result<StatIdentity, minix_sys::Errno> {
    let mut stat: minix_sys::Stat = unsafe { core::mem::zeroed() };
    minix_sys::stat(path, &mut stat)?;
    Ok(target_identity(&stat))
}

/// C's `(void) sleep(1)` (`forward.c:253`): the return value is discarded, so
/// being interrupted early by a signal simply interrupts.
#[cfg(any(test, not(target_os = "none")))]
fn sleep_one_tick() {
    std::thread::sleep(std::time::Duration::from_secs(1));
}

/// The target side's wait channel is `minix_sys::nanosleep`, whose implementation
/// is VFS's empty-set `select` with a timeout (`misc::nanosleep_via` →
/// `select_empty_via`, from the same source as C `nanosleep.c`). Its return value
/// is discarded too, matching C's `(void)`.
#[cfg(all(not(test), target_os = "none"))]
fn sleep_one_tick() {
    let _ = minix_sys::nanosleep(1, 0);
}

/// C `(void) fflush(stdout)` (`forward.c:200`): a round's data must be handed out
/// before sleeping. The host's standard output is line buffered, so without this
/// mouth half a line would be held back; on the target side `emit` calls
/// `write(2)` directly and there is no user-space buffer to flush.
#[cfg(any(test, not(target_os = "none")))]
fn flush_output() {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
}

#[cfg(all(not(test), target_os = "none"))]
fn flush_output() {}

fn run() -> ! {
    let argv: Vec<Vec<u8>> = support::args_bytes();
    // Every diagnostic sentence of C starts with `getprogname()`
    // (`misc.c:66-91`'s vwarn/vwarnx), so nothing below hardcodes "tail". The
    // program name is folded once here (the two `&str` mouths `line_bytes` and
    // `err_line`, registered as `ARGV-BYTE-PATH-IN-LIBS`); operands, banners and
    // the three echoed sentences have byte-wise legs of their own and do not go
    // through it.
    let prog = String::from_utf8_lossy(support::prog_name_bytes(&argv)).into_owned();
    let args: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    // argv folding lives in the deciding half (`parse_tail_args`): BSD
    // `obsolete()` + `getopt("Fb:c:fn:r")`, with attached-or-next counts and
    // option clustering. The old inline whole-token matcher leaked attached
    // values (`tail -n10 f`, `tail -c100 f`) and unknown flags (`tail -x f`)
    // into the operand list, silently treating them as file names.
    let parsed = match parse_tail_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(e) => arg_error(e, &prog),
    };
    let style = parsed.style;
    let follow = parsed.follow;
    let operands: Vec<Vec<u8>> = parsed.operands;

    // C `for each file: fopen+fstat (ierr on failure), banner when argc>1,
    // forward()`. No operands read stdin. The banner comes after the successful
    // open because C `continue`s past a file it cannot open without printing one.
    let show_banner = operands.len() > 1;
    let mut first = true;
    let mut status = 0;

    if operands.is_empty() {
        // C `tail.c:185-207`: `fname = "stdin"`, `ierr(); exit(1)` when fstat
        // fails, then one probe for whether the stream can be positioned — a pipe
        // clears `fflag` (POSIX.2).
        let source = FollowSource::stdin();
        let opened = match source.identity() {
            Ok(opened) => opened,
            Err(code) => {
                // C's `ierr()` prints the global `fname`, and the standard-input
                // form at `tail.c:186` writes it as the literal `"stdin"` — five
                // bytes, not something folded.
                warn_bytes(&prog, b"stdin", code);
                support::terminate(1);
            }
        };
        let mut target = FollowTarget {
            source,
            opened,
            path: None,
        };
        let follow = follow.kept_for_stdin(target.source.is_pipe_by_position());
        let data = match target.source.collect() {
            Ok(data) => data,
            Err(code) => {
                warn_bytes(&prog, target.label(), code);
                support::terminate(1);
            }
        };
        tail_stream(&data, style);
        if follow.active() && !follow_rounds(&mut target, follow, &prog) {
            support::terminate(1);
        }
        support::terminate(0);
    }
    for operand in &operands {
        // C `tail.c:167-171`: a failed `fopen` **or** a failed `fstat` is both
        // `ierr(); continue`, and that diagnostic is `tail: <name>: <sentence>`
        // (`misc.c:53-57` goes through the errno-carrying `xerr`/`vwarn`, not
        // `warnx`).
        // The `&str` mouth for opening (`ARGV-BYTE-PATH-IN-LIBS`): only the name
        // handed to `open` is folded; the diagnostic slot and the banner slot
        // still use `operand`'s raw bytes.
        let shown = String::from_utf8_lossy(operand);
        let source = match FollowSource::open(&shown) {
            Ok(source) => source,
            Err(code) => {
                warn_bytes(&prog, operand, code);
                status = 1;
                continue;
            }
        };
        let opened = match source.identity() {
            Ok(opened) => opened,
            Err(code) => {
                warn_bytes(&prog, operand, code);
                status = 1;
                continue;
            }
        };
        let mut target = FollowTarget {
            source,
            opened,
            path: Some(operand.as_slice()),
        };
        let data = match target.source.collect() {
            Ok(data) => data,
            Err(code) => {
                warn_bytes(&prog, operand, code);
                status = 1;
                continue;
            }
        };
        if show_banner {
            support::emit(&banner(first, operand));
        }
        first = false;
        tail_stream(&data, style);
        if follow.active() {
            // C's `forward()` never returns while following, so the only way back
            // here is the `ierr()` leg; later operands were already blocked by the
            // single-file gate at `tail.c:135-137`, so the wrap-up simply exits.
            if !follow_rounds(&mut target, follow, &prog) {
                status = 1;
            }
            break;
        }
    }
    support::terminate(status)
}

/// What C `forward.c:191-274`'s `for(;;)` has left on Minix (the kqueue half is
/// excluded by `#if !defined(__minix)` at `:207-246`): sleep one round → for a
/// `-F` real file only, look up the identity by name and reopen once the swap is
/// recognised → read and write the bytes after the current position as you go.
///
/// The first round's output is not here: the caller has already read the whole
/// block and printed the range, so the position is at EOF; C's loop is "copy
/// first, then sleep", so starting from a sleep here keeps the same order as the
/// original.
///
/// Returning `false` matches C's `ierr(); return;` and `ierr(); goto out;`
/// (`:196-199`, `:262-265`): non-fatal, but the exit code has already become 1.
fn follow_rounds(target: &mut FollowTarget, follow: Follow, prog: &str) -> bool {
    loop {
        sleep_one_tick();
        if follow.probes_identity(target.is_stdin()) {
            // Reaching here means `path` must be `Some`: `probes_identity` is
            // always false for standard input. No `unwrap` — that unreachable
            // state is not worth a panic.
            if let Some(name) = target.path {
                // Both the name-based identity lookup and the reopen take the
                // `&str` path mouth (registered as `ARGV-BYTE-PATH-IN-LIBS`);
                // `freopen(fname, …)` (`forward.c:261`) still reports using
                // `fname`'s raw bytes, so the diagnostic slot is not folded along.
                let shown = String::from_utf8_lossy(name);
                match stat_by_path(&shown) {
                    // C's `stat(fname, &statbuf) != -1`: an unrecognisable name
                    // (deleted after the swap) means this round does nothing and
                    // the next keeps sleeping — no error, no exit.
                    Err(_) => {}
                    Ok(current) => {
                        if reopen_needed(&target.opened, &current) {
                            if let Err(code) = target.source.reopen(&shown) {
                                warn_bytes(prog, name, code);
                                return false;
                            }
                            // C's `*sbp = statbuf`: the new file's identity takes
                            // over, and the next round compares against it.
                            target.opened = current;
                        }
                    }
                }
            }
        }
        let pumped = target.source.pump();
        if let Err(code) = pumped {
            let label = target.label();
            warn_bytes(prog, label, code);
            return false;
        }
        flush_output();
    }
}

/// Emit the tail of one open descriptor for the resolved [`TailStyle`]. The
/// forward shapes print raw bytes from an offset (C `forward()`'s `getc` loop
/// copies what is left of the file, newlines and all, so no line is invented
/// and a trailing unterminated line stays unterminated); the reverse shapes
/// print whole lines bottom-up (C `reverse()`).
fn tail_stream(input: &[u8], style: TailStyle) {
    // C's zero arms come back with nothing written and a successful exit:
    // `forward.c` seeks to the end for `RLINES`/`RBYTES` with `off == 0`, and
    // `reverse.c` returns before touching the input for any other style with
    // `off == 0`. Only `REVERSE` (bare `-r`) prints the whole file.
    if style.selects_nothing() {
        return;
    }
    match style {
        TailStyle::SkipBytes { off } => support::emit(bytes_from(input, off)),
        TailStyle::SkipLines { off } => support::emit(bytes_after_lines(input, off)),
        TailStyle::LastBytes { off } => support::emit(last_bytes_of(input, off)),
        TailStyle::LastLines { off } => {
            let mut window = match TailWindow::new(off) {
                Ok(window) => window,
                Err(_) => support::terminate(1),
            };
            for line in support::lines_of(input) {
                window.push(line);
            }
            // Size the output by what the input actually offered, never by the
            // requested count: `tail -n 1000000` on ten lines holds ten.
            let want = (window.seen() as usize).min(off);
            let mut survivors: Vec<minix_textfilter::window::Line> =
                vec![minix_textfilter::window::Line::new(b""); want];
            let drained = window.drain(&mut survivors);
            for line in &survivors[..drained] {
                emit_line(*line);
            }
        }
        TailStyle::ReverseAll => {
            for line in reversed_lines(&support::lines_of(input), None) {
                emit_line(line);
            }
        }
        // `off == 0` prints nothing, which is C's early return at
        // `reverse.c:79-80` (`style != REVERSE && off == 0`).
        TailStyle::ReverseLines { off } => {
            for line in reversed_lines(&support::lines_of(input), Some(off)) {
                emit_line(line);
            }
        }
        TailStyle::ReverseBytes { off } => {
            for line in reverse_bytes(input, off) {
                emit_line(line);
            }
        }
    }
}

fn emit_line(line: minix_textfilter::window::Line<'_>) {
    let mut out = line.bytes().to_vec();
    // The line range C `reverse.c:133-144` records already contains that newline,
    // and is not patched when the last line lacks one; `tail -r` moves an
    // unterminated last line to the **front** of the output, so this fact can only
    // be recorded on the line, not asked by line number.
    out.extend_from_slice(line.eol());
    support::emit(&out);
}

/// The line order C `reverse()` prints: `RLINES` (`Some(off)`) takes the last
/// `off` lines then reverses them (`reverse.c:138-141` stops the backward walk
/// after `off` lines); `REVERSE` (`None`) reverses every line. `Some(0)` selects
/// no line at all, matching C's early return.
fn reversed_lines<'a>(
    lines: &[minix_textfilter::window::Line<'a>],
    take: Option<usize>,
) -> Vec<minix_textfilter::window::Line<'a>> {
    let selected: &[minix_textfilter::window::Line<'a>] = match take {
        Some(off) => &lines[lines.len().saturating_sub(off)..],
        None => lines,
    };
    selected.iter().rev().copied().collect()
}

/// C `tail -r` with `-c`/`-b` (`RBYTES`, `reverse.c` `r_reg`): the lines of the
/// trailing `off`-byte window, printed bottom-up. The window can begin mid-line;
/// r_reg still emits that leading fragment (its leftover `WR`), which is
/// precisely what a byte-window line split yields, so the reverse is exact.
fn reverse_bytes(input: &[u8], off: usize) -> Vec<minix_textfilter::window::Line<'_>> {
    let window = last_bytes_of(input, off);
    let lines = support::lines_of(window);
    reversed_lines(&lines, None)
}

/// The last `off` bytes of `input`, or all of it when shorter — C `tail -c N`
/// prints the final N bytes of each file.
fn last_bytes_of(input: &[u8], off: usize) -> &[u8] {
    &input[input.len().saturating_sub(off)..]
}

/// C `forward()` + `FBYTES` (`forward.c:100-113`): print from byte `off + 1` on.
/// C clamps the seek to the file size for regular files and stops at EOF for
/// pipes, either way an offset past the end prints nothing.
fn bytes_from(input: &[u8], off: usize) -> &[u8] {
    &input[off.min(input.len())..]
}

/// C `forward()` + `FLINES` (`forward.c:115-127`): consume input through the
/// `off`-th newline, then print everything after it. `off == 0` prints the whole
/// input (that is `tail -n +1`), and an input with fewer than `off` newlines
/// leaves C at EOF, which prints nothing.
fn bytes_after_lines(input: &[u8], off: usize) -> &[u8] {
    if off == 0 {
        return input;
    }
    let mut seen = 0usize;
    for (index, byte) in input.iter().enumerate() {
        if *byte == b'\n' {
            seen += 1;
            if seen == off {
                return &input[index + 1..];
            }
        }
    }
    &[]
}

/// Map a [`TailError`] from `parse_tail_args` to its diagnostic and exit status,
/// then terminate.
fn arg_error(e: TailError, prog: &str) -> ! {
    match e {
        TailError::Usage => usage(prog),
        TailError::Getopt(report) => {
            // libc's sentence (`getopt.c:101-104` or `:132-136`) comes first, the
            // usage line after. The sentence has a single home in `GetoptReport`,
            // so the other commands in this family receive the same text. The
            // letter slot goes through `line_bytes`: `getopt.c:79`'s
            // `optopt = *place++` takes one byte at a time and `:103`/`:134` write
            // it back with `%c`, so `-é` (`c3 a9`) reports `c3`; the take in
            // `src/window.rs`'s ring is `char::from(bytes[pos])`, and the two sides
            // agree only when both are bytewise.
            support::warn(&report.line_bytes(prog));
            usage(prog)
        }
        TailError::FollowMultiFile => {
            // C `tail.c:135-137`'s `xerrx(1, ...)`: `xerrx` goes through `vwarnx`
            // (`misc.c:78-89`), so **no `errno` sentence is appended** to that line.
            support::warn(
                format!("{prog}: -f and -F options only appropriate for a single file\n")
                    .as_bytes(),
            );
            support::terminate(1);
        }
        TailError::IllegalCount(v) => {
            // C `xerrx(1, "illegal offset -- %s", optarg)` (`tail.c:90`): `%s`
            // prints every byte of `optarg`. The value slot is argv's raw bytes (for
            // the legacy form it is the string **after** rewriting, see
            // `obsolete_step` in `src/window.rs`), so this line is assembled here.
            support::warn(
                &[
                    format!("{prog}: illegal offset -- ").as_bytes(),
                    v.as_slice(),
                    b"\n".as_slice(),
                ]
                .concat(),
            );
            support::terminate(1);
        }
        TailError::IllegalOption(v) => {
            // C `obsolete()`'s fatal arm: `xerrx(1, "illegal option -- %s", *argv)`
            // (`tail.c:269`) — it carries the **whole token**, likewise as raw bytes.
            support::warn(
                &[
                    format!("{prog}: illegal option -- ").as_bytes(),
                    v.as_slice(),
                    b"\n".as_slice(),
                ]
                .concat(),
            );
            support::terminate(1);
        }
    }
}

/// C `tail.c:173-174`'s `printf("%s==> %s <==\n", first ? "" : "\n", fname)`:
/// with several operands every file gets a banner line before it, with no blank
/// line before the first. `fname` is that byte of argv, and this line goes to
/// **standard output** (not the diagnostic leg). It is a pure function so that
/// "a real file whose name carries a raw byte" has a gate it can pin: the banner
/// path cannot open such a file on the host (the `&str` mouth for opening,
/// registered as `ARGV-BYTE-PATH-IN-LIBS`), and end to end only the failure
/// sentence is visible, so "the name enters the banner as bytes" can only be
/// pinned at this assembly step.
fn banner(first: bool, name: &[u8]) -> Vec<u8> {
    let mut line: Vec<u8> = Vec::new();
    if !first {
        line.extend_from_slice(b"\n");
    }
    line.extend_from_slice(b"==> ");
    line.extend_from_slice(name);
    line.extend_from_slice(b" <==\n");
    line
}

/// C's `warn(3)` line is rendered **bytewise**: `prog: label: <strerror
/// sentence>` plus a newline, with no exit after writing (matching C's `ierr()`:
/// `tail.c:169` and `forward.c:196` only report without terminating, and the exit
/// code turns 1 only at the end). The shape and the sentences still follow the
/// authority in `minix-types` (`verrc.c`'s three parts: program name, `": "`, the
/// caller's string, `": "`, `strerror(code)`). The reason for assembling once more
/// here instead of using `support::warn_line`: that mouth's label is `&str`
/// (`minix_sys::err_line` still has only the text form, registered as
/// `ARGV-BYTE-PATH-IN-LIBS`), while all of `tail`'s labels are operand names
/// coming in from argv — going through `err_line` would fold a bare `ff` into the
/// three bytes `ef bf bd`, which cannot match the reference byte for byte (both
/// the `split` and `sort` rounds measured this shape). Once `err_line` gains a
/// byte form, this copy merges back into it.
fn warn_bytes(prog: &str, label: &[u8], code: minix_sys::Errno) {
    let mut line = Vec::new();
    line.extend_from_slice(prog.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(label);
    line.extend_from_slice(b": ");
    line.extend_from_slice(minix_sys::Strerror(code).to_string().as_bytes());
    line.push(b'\n');
    support::warn(&line);
}

/// C `tail.c:299-305`: that line spells `Usage:` in capitals and carries
/// `getprogname()`, and its option list includes `-f | -F` (the shell's previous
/// lowercase line listing only `-r` was an invention).
fn usage(prog: &str) -> ! {
    support::warn(
        format!("Usage: {prog} [-f | -F | -r] [-b # | -c # | -n #] [file ...]\n").as_bytes(),
    );
    support::terminate(1);
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}

#[cfg(test)]
mod tests {
    use super::{
        banner, bytes_after_lines, bytes_from, last_bytes_of, reverse_bytes, reversed_lines,
    };
    use minix_textfilter::window::Line;

    #[test]
    fn last_bytes_truncates_to_count() {
        assert_eq!(last_bytes_of(b"abcdef", 3), b"def");
    }

    #[test]
    fn last_bytes_short_input_returns_all() {
        assert_eq!(last_bytes_of(b"ab", 10), b"ab");
    }

    #[test]
    fn last_bytes_of_empty_is_empty() {
        assert_eq!(last_bytes_of(b"", 5), b"");
    }

    #[test]
    fn bytes_from_skips_the_offset() {
        // `-c +3` is `off = 2` (`ARG` gives back one unit), i.e. byte three on.
        assert_eq!(bytes_from(b"abcdef", 2), b"cdef");
    }

    #[test]
    fn bytes_from_zero_is_whole_input() {
        // `-c +1` folds to `off = 0`: C breaks out of the `FBYTES` case without
        // seeking, so the whole file prints.
        assert_eq!(bytes_from(b"abcdef", 0), b"abcdef");
    }

    #[test]
    fn bytes_from_past_end_prints_nothing() {
        // C clamps the seek to `st_size` (`forward.c`), and a pipe stops at EOF —
        // either way nothing is left to print.
        assert_eq!(bytes_from(b"abc", 9), b"");
    }

    #[test]
    fn bytes_after_lines_lands_on_the_nth_line() {
        let input = b"one\ntwo\nthree\nfour\n";
        // `-n +1` → off 0 → whole file; `-n +2` → off 1 → past the first newline.
        assert_eq!(bytes_after_lines(input, 0), input);
        assert_eq!(bytes_after_lines(input, 1), b"two\nthree\nfour\n");
        assert_eq!(bytes_after_lines(input, 3), b"four\n");
    }

    #[test]
    fn bytes_after_lines_keeps_a_final_unterminated_line() {
        // C copies the remaining bytes character by character, so a last line
        // without a newline keeps its shape instead of gaining one.
        let input = b"a\nb world";
        assert_eq!(bytes_after_lines(input, 1), b"b world");
    }

    #[test]
    fn bytes_after_lines_short_input_prints_nothing() {
        // Fewer newlines than `off`: C's `getc` loop reaches EOF and prints
        // nothing.
        assert_eq!(bytes_after_lines(b"only\n", 5), b"");
    }

    /// These unit tests compare "which lines, in what order"; the newline state of
    /// the last line is pinned bytewise by the face gate and not carried again
    /// here. Lines are bytes now, and this file's fixtures are all ASCII: failing
    /// to decode means the fixture is wrong, not a case for silent replacement.
    fn texts<'a>(lines: &[minix_textfilter::window::Line<'a>]) -> Vec<&'a str> {
        lines
            .iter()
            .map(|line| core::str::from_utf8(line.bytes()).expect("测试夹具都是 ASCII"))
            .collect()
    }

    #[test]
    fn reversed_lines_all_reverses_every_line() {
        let lines = [Line::from("1"), Line::from("2"), Line::from("3")];
        assert_eq!(texts(&reversed_lines(&lines, None)), ["3", "2", "1"]);
    }

    #[test]
    fn reversed_lines_last_n_reversed() {
        // -r -n 2 on [1,2,3,4] -> last 2 (3,4) reversed -> 4,3.
        let lines = [
            Line::from("1"),
            Line::from("2"),
            Line::from("3"),
            Line::from("4"),
        ];
        assert_eq!(texts(&reversed_lines(&lines, Some(2))), ["4", "3"]);
    }

    #[test]
    fn reversed_lines_count_exceeding_input_is_all() {
        let lines = [Line::from("a"), Line::from("b")];
        assert_eq!(texts(&reversed_lines(&lines, Some(5))), ["b", "a"]);
    }

    #[test]
    fn reversed_lines_zero_takes_nothing() {
        // C `reverse.c:79-80`: a non-`REVERSE` style with `off == 0` returns
        // before printing, so `tail -r -n +1` prints nothing rather than the
        // whole file.
        let lines = [Line::from("a"), Line::from("b")];
        assert!(reversed_lines(&lines, Some(0)).is_empty());
    }

    #[test]
    fn reverse_bytes_reverses_lines_of_trailing_window() {
        // -r -c on "aaa\nbbb\nccc\n": last 8 bytes = "bbb\nccc\n" -> ccc,bbb.
        let input = b"aaa\nbbb\nccc\n";
        assert_eq!(texts(&reverse_bytes(input, 8)), ["ccc", "bbb"]);
        // window starts mid-line (last 7 = "bb\nccc\n"); r_reg emits the leading
        // fragment too, so "bb" survives as its own line.
        assert_eq!(texts(&reverse_bytes(input, 7)), ["ccc", "bb"]);
        // byte offset past the input reverses the whole file's lines.
        assert_eq!(texts(&reverse_bytes(input, 99)), ["ccc", "bbb", "aaa"]);
        // -r -c +1 folds to RBYTES with off 0, whose empty window holds no line.
        assert!(reverse_bytes(input, 0).is_empty());
    }

    /// The banner line goes to standard output and its name slot is argv's raw
    /// bytes (`tail.c:173-174` hands `fname` straight to `%s`). This test pins the
    /// **assembly** leg: a bare `ff` neither folds into the three bytes `ef bf bd`
    /// nor is stopped before assembly by "must be text". End to end this shape is
    /// invisible — a real file whose name carries a raw byte cannot be opened by
    /// this shell (the `&str` mouth for opening, registered as
    /// `ARGV-BYTE-PATH-IN-LIBS`), so the banner can only be pinned on the pure
    /// function, while the name inside the failure sentence belongs to
    /// `tests/tail_face.rs`. The two `first` shapes pin C's blank-line rule: none
    /// before the first file, one blank line before every file after it
    /// (`first ? "" : "\n"`).
    #[test]
    fn the_banner_names_the_operand_bytes_and_only_the_second_one_gets_a_blank_line() {
        assert_eq!(
            banner(true, b"a\xff.txt"),
            [
                b"==> ".as_slice(),
                b"a\xff.txt".as_slice(),
                b" <==\n".as_slice()
            ]
            .concat(),
            "第一档前面不空行，而档名里那枚 ff 还是 ff"
        );
        assert_eq!(
            banner(false, b"a\xff.txt"),
            [
                b"\n==> ".as_slice(),
                b"a\xff.txt".as_slice(),
                b" <==\n".as_slice()
            ]
            .concat()
        );
        assert_eq!(banner(true, b"in.txt"), b"==> in.txt <==\n".to_vec());
    }
}
