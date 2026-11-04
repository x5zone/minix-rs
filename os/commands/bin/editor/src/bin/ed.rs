//! Minix-RS ed — the doing half over `minix_editor::exec`.
//!
//! Ground truth: `minix3/bin/ed/main.c` (`main`, line 117: option loop,
//! script flag, initial file read at 179-193, and the read-eval loop at
//! 198-280). The deciding halves live in the library (address parsing in
//! `addr`, command letters in `cmd`, execution in `exec`); this program
//! gathers argv, reads standard input line by line, prints the `?` error
//! channel, and applies the flow decisions.
//!
//! Seams (same hosted-versus-target split as the other commands, swapped
//! in one sweep when no_std program images land): argv from
//! `std::env::args`, `terminate` through the host runtime (a hosted
//! `minix_sys::exit` would spin), bytes through `minix_sys::read`/`write`
//! so a hosted run without a kernel fails honestly.


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_editor::exec::{self, EditorIo, Flow, Session};
use minix_editor::store::{GapStore, TextStore};
use minix_shell::exec_run::exec_one;
use minix_sys::{Fd, close, dup2, fork, pipe2, read, waitpid, write};

/// POSIX standard descriptors.
const STDIN: Fd = 0;
const STDOUT: Fd = 1;
const STDERR: Fd = 2;

/// `open` flags per `minix3/sys/sys/fcntl.h` (the Minix values, not the
/// Linux ones): O_WRONLY 0x1 (:65), O_APPEND 0x8 (:82), O_CREAT 0x200 (:99),
/// O_TRUNC 0x400 (:100).
const O_WRONLY: i32 = 0x1;
const O_CREAT: i32 = 0x200;
const O_TRUNC: i32 = 0x400;
const O_APPEND: i32 = 0x8;

/// The create-mode flags for an ed write: `w` truncates (`main.c:828`
/// `fopen("w")`), `W` appends (`fopen("a")`); both create. A pure function so
/// the flag composition is unit-testable without a filesystem.
fn write_flags(append: bool) -> i32 {
    if append {
        O_WRONLY | O_CREAT | O_APPEND
    } else {
        O_WRONLY | O_CREAT | O_TRUNC
    }
}

/// The production I/O half: bytes in and out through `minix_sys`, files
/// through the open/read/write/close family (the wrappers, one call each —
/// `io.c`'s `fopen`/`fclose` faces). A name starting with `!` is a pipe:
/// `popen` = `pipe2` + `fork` + exec `/bin/sh -c` (`io.c:50`/`:157`).
struct TtyIo {
    scripted: bool,
    /// The inherited `KEY=VALUE` strings handed to the shell child (C
    /// `system`/`popen` pass `environ` through).
    envp: Vec<String>,
}

impl EditorIo for TtyIo {
    fn emit(&mut self, bytes: &[u8]) {
        //[SD19-gate] write STDOUT/STDERR is best-effort
        #[allow(clippy::let_underscore_must_use)]
        let _ = write(STDOUT, bytes);
    }

    fn emit_err(&mut self, bytes: &[u8]) {
        //[SD19-gate] write STDOUT/STDERR is best-effort
        #[allow(clippy::let_underscore_must_use)]
        let _ = write(STDERR, bytes);
    }

    /// C `system()` (`stdlib.h`, called at `main.c:876`): fork, exec
    /// `/bin/sh -c <cmd>`, wait; the caller discards the exit status. The
    /// exec-failure child exits 127 (POSIX `system`'s answer when the
    /// shell itself cannot be run).
    fn run_shell(&mut self, cmd: &str) {
        match fork() {
            Ok(0) => child_exec_shell(cmd, &self.envp),
            Ok(pid) => {
                //[SD19-gate] waitpid reap; C system reads the status only to
                // return it and ed discards that value
                #[allow(clippy::let_underscore_must_use)]
                let mut raw = 0;
                let _ = waitpid(pid, &mut raw, 0);
            }
            Err(_) => {}
        }
    }

    fn read_file(&mut self, name: &str, out: &mut [u8]) -> Result<usize, minix_editor::EditorError> {
        if let Some(cmd) = name.strip_prefix('!') {
            return self.popen_read(cmd, out);
        }
        let fd = minix_sys::open(name, 0, 0).map_err(|_| minix_editor::EditorError::InvalidArgument)?;
        let mut total = 0;
        while total < out.len() {
            match read(fd, &mut out[total..]) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => {
                    //[SD19-gate] close fd is best-effort cleanup
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = minix_sys::close(fd);
                    return Err(minix_editor::EditorError::InvalidArgument);
                }
            }
        }
        //[SD19-gate] close fd is best-effort cleanup
        #[allow(clippy::let_underscore_must_use)]
        let _ = minix_sys::close(fd);
        let _ = self.scripted;
        Ok(total)
    }

    fn write_file(
        &mut self,
        name: &str,
        data: &[u8],
        append: bool,
    ) -> Result<usize, minix_editor::EditorError> {
        if let Some(cmd) = name.strip_prefix('!') {
            // C `popen(fn + 1, "w")` (`io.c:157`): the mode is "w"
            // whatever the caller asked for.
            return self.popen_write(cmd, data);
        }
        // `main.c:828` maps ed `w` to fopen("w") (truncate) and `W` to
        // fopen("a") (append); the flags come from `write_flags` (Minix
        // `fcntl.h` values). mode 0o644 per the existing face.
        let flags = write_flags(append);
        let fd = minix_sys::open(name, flags, 0o644)
            .map_err(|_| minix_editor::EditorError::InvalidArgument)?;
        let mut done = 0;
        while done < data.len() {
            match write(fd, &data[done..]) {
                Ok(0) => break,
                Ok(n) => done += n,
                Err(_) => {
                    //[SD19-gate] close fd is best-effort cleanup
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = minix_sys::close(fd);
                    return Err(minix_editor::EditorError::InvalidArgument);
                }
            }
        }
        //[SD19-gate] close fd is best-effort cleanup
        #[allow(clippy::let_underscore_must_use)]
        let _ = minix_sys::close(fd);
        Ok(done)
    }
}

/// The popen/`system` child: exec `/bin/sh -c <cmd>` with the inherited
/// environment. A successful exec never returns; a failed one exits 127
/// (POSIX `system`/`popen`'s answer when the shell cannot be run).
fn child_exec_shell(cmd: &str, envp: &[String]) -> ! {
    let argv: [&str; 3] = ["/bin/sh", "-c", cmd];
    let env: Vec<&str> = envp.iter().map(|s| s.as_str()).collect();
    let _ = exec_one("/bin/sh", &argv, &env);
    support::terminate(127);
}

impl TtyIo {
    /// C `popen(cmd, "r")` (`io.c:50`): the child's stdout feeds the read;
    /// a `pipe2`/`fork` failure is popen's NULL answer. `pclose` is the
    /// trailing `waitpid`.
    fn popen_read(
        &mut self,
        cmd: &str,
        out: &mut [u8],
    ) -> Result<usize, minix_editor::EditorError> {
        let (read_fd, write_fd) =
            pipe2(0).map_err(|_| minix_editor::EditorError::InvalidArgument)?;
        match fork() {
            Ok(0) => {
                //[SD19-gate] dup2/close redirect setup; popen's child treats
                // the pipe face as best effort
                #[allow(clippy::let_underscore_must_use)]
                let _ = dup2(write_fd, STDOUT);
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(read_fd);
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(write_fd);
                child_exec_shell(cmd, &self.envp)
            }
            Ok(pid) => self.pipe_read(pid, read_fd, write_fd, out),
            Err(_) => {
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(read_fd);
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(write_fd);
                Err(minix_editor::EditorError::InvalidArgument)
            }
        }
    }

    /// C `popen(cmd, "w")` (`io.c:157`): the buffer feeds the child's
    /// stdin; the mode is "w" whatever the caller asked for.
    fn popen_write(&mut self, cmd: &str, data: &[u8]) -> Result<usize, minix_editor::EditorError> {
        let (read_fd, write_fd) =
            pipe2(0).map_err(|_| minix_editor::EditorError::InvalidArgument)?;
        match fork() {
            Ok(0) => {
                #[allow(clippy::let_underscore_must_use)]
                let _ = dup2(read_fd, STDIN);
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(read_fd);
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(write_fd);
                child_exec_shell(cmd, &self.envp)
            }
            Ok(pid) => {
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(read_fd);
                let mut done = 0;
                while done < data.len() {
                    match write(write_fd, &data[done..]) {
                        Ok(0) => break,
                        Ok(n) => done += n,
                        Err(_) => {
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = close(write_fd);
                            self.reap(pid);
                            return Err(minix_editor::EditorError::InvalidArgument);
                        }
                    }
                }
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(write_fd);
                self.reap(pid);
                Ok(done)
            }
            Err(_) => {
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(read_fd);
                #[allow(clippy::let_underscore_must_use)]
                let _ = close(write_fd);
                Err(minix_editor::EditorError::InvalidArgument)
            }
        }
    }

    /// The read half of popen: drain the child's stdout to EOF (or the
    /// caller's capacity), then `pclose`.
    fn pipe_read(
        &mut self,
        pid: i32,
        read_fd: Fd,
        write_fd: Fd,
        out: &mut [u8],
    ) -> Result<usize, minix_editor::EditorError> {
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(write_fd);
        let mut total = 0;
        while total < out.len() {
            match read(read_fd, &mut out[total..]) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => {
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = close(read_fd);
                    self.reap(pid);
                    return Err(minix_editor::EditorError::InvalidArgument);
                }
            }
        }
        #[allow(clippy::let_underscore_must_use)]
        let _ = close(read_fd);
        self.reap(pid);
        Ok(total)
    }

    /// `pclose`: reap the child. C compares the return against `< 0`; the
    /// failure carries no errno here, so it stays best effort (the lib
    /// maps every io failure to the same "cannot open" pair).
    fn reap(&mut self, pid: i32) {
        #[allow(clippy::let_underscore_must_use)]
        let mut raw = 0;
        let _ = waitpid(pid, &mut raw, 0);
    }
}

/// One line from standard input, newline stripped; `None` at end of
/// input. Non-UTF-8 input is rejected (the store is a text store —
/// `rewrite-notes/18-stage-commands/09-editors.md` declares the boundary).
fn read_line(buf: &mut [u8; 8192]) -> Option<String> {
    let mut len = 0;
    loop {
        // Refill: read returns whatever is ready; a full buffer without a
        // newline is the recorded capacity boundary.
        if len == buf.len() {
            return None;
        }
        match read(STDIN, &mut buf[len..]) {
            Ok(0) => {
                if len == 0 {
                    return None;
                }
                // Final line without a newline: the C editor discards it
                // (`main.c:218-223`, "unexpected end-of-file").
                return None;
            }
            Ok(n) => {
                if let Some(pos) = buf[..len + n].iter().position(|&b| b == b'\n') {
                    let line = String::from_utf8(buf[..pos].to_vec()).ok()?;
                    return Some(line);
                }
                len += n;
            }
            Err(_) => return None,
        }
    }
}

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let program = args.first().copied().unwrap_or("ed");
    // `red = strlen > 2 && argv[0][n-3] == 'r'` (`main.c:118`).
    let restricted = program.len() > 2 && program.as_bytes()[program.len() - 3] == b'r';

    let mut scripted = false;
    let mut secure = false;
    let mut opt_prompt: Option<&str> = None;
    let mut names: Vec<&str> = Vec::new();
    let mut at = 1;
    while at < args.len() {
        let arg = args[at];
        if arg == "--" {
            at += 1;
            names.extend(args[at..].iter());
            break;
        }
        if let Some(rest) = arg.strip_prefix('-') {
            if rest.is_empty() {
                // A lone `-` is script mode (`main.c:150-153`).
                scripted = true;
                at += 1;
                continue;
            }
            let mut chars = rest.chars();
            while let Some(c) = chars.next() {
                match c {
                    's' => scripted = true,
                    'S' => secure = true,
                    // Extended regexes: `s` is unwired, so the flag has no
                    // one to serve yet; accepted like C accepts it.
                    'E' => {}
                    // No DES in this build: the C editor prints the notice
                    // and carries on (`main.c:131-137`).
                    'x' => {
                        //[SD19-gate] write STDOUT/STDERR is best-effort
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = write(STDERR, b"crypt unavailable\n?\n");
                    }
                    'p' => {
                        opt_prompt = Some(chars.as_str());
                        if opt_prompt.is_some() && chars.as_str().is_empty() {
                            // `-p string` as two argv items.
                            at += 1;
                            opt_prompt = args.get(at).copied();
                        }
                        break;
                    }
                    _ => {
                        //[SD19-gate] write STDOUT/STDERR is best-effort
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = write(STDERR, b"Usage: ed [-] [-ESsx] [-p string] [name]\n");
                        support::terminate(1);
                    }
                }
            }
            at += 1;
        } else {
            names.push(arg);
            at += 1;
        }
    }

    let mut store = GapStore::new();
    let mut sess = Session::new(scripted, secure, restricted, opt_prompt);
    let mut io = TtyIo {
        scripted,
        envp: support::envs(),
    };

    // The initial file (`main.c:179-193`): read it, name it, quit(2) in
    // script mode when the read fails.
    if let Some(name) = names.first() {
        if name.is_empty() {
            //[SD19-gate] write STDOUT/STDERR is best-effort
            #[allow(clippy::let_underscore_must_use)]
            let _ = write(STDERR, b"?\n");
            sess.error_msg = Some("invalid filename");
            if scripted {
                support::terminate(2);
            }
        } else {
            let mut scratch = [0u8; minix_editor::store::MAX_TEXT];
            match io.read_file(name, &mut scratch) {
                Ok(size) => {
                    if let Ok(text) = core::str::from_utf8(&scratch[..size]) {
                        let mut pos = 0usize;
                        for piece in text.split('\n').collect::<Vec<_>>() {
                            // `split` yields a trailing empty piece for a
                            // newline-terminated file; the store adds the
                            // newline back on its own.
                            if piece.is_empty() && pos == text.len() {
                                break;
                            }
                            let at_line = pos + 1;
                            if store.insert(at_line, piece).is_ok() {
                                pos += 1;
                            }
                        }
                        sess.current = pos;
                        sess.modified = false;
                    }
                    sess.set_filename(name).unwrap_or_else(|_| support::terminate(2));
                }
                Err(_) => {
                    //[SD19-gate] write STDOUT/STDERR is best-effort
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = write(STDERR, b"?\n");
                    if scripted {
                        support::terminate(2);
                    }
                }
            }
        }
    }

    let mut buf = [0u8; 8192];
    loop {
        if sess.prompt_on && !sess.scripted {
            //[SD19-gate] write STDOUT/STDERR is best-effort
            #[allow(clippy::let_underscore_must_use)]
            let _ = write(STDOUT, sess.prompt_bytes());
        }
        let Some(line) = read_line(&mut buf) else {
            // End of input: `q` rules apply (`main.c:206-221`) — modified
            // warns once interactively and quits in script mode.
            if sess.modified && !sess.scripted {
                //[SD19-gate] write STDOUT/STDERR is best-effort
                #[allow(clippy::let_underscore_must_use)]
                let _ = write(STDERR, b"?\n");
                sess.error_msg = Some("warning: file modified");
                if sess.scripted {
                    support::terminate(2);
                }
                sess.modified = false;
                continue;
            }
            support::terminate(0);
        };
        if line.is_empty() && !sess.scripted {
            // C distinguishes an immediate EOF (n == 0) from an empty
            // line; `read` returning a lone newline lands here as "".
        }
        match exec::step(&mut store, &mut sess, &line, &mut io) {
            Ok(Flow::Continue) => {}
            Ok(Flow::Quit) => support::terminate(0),
            Ok(Flow::QuitModified) => {
                //[SD19-gate] write STDOUT/STDERR is best-effort
                #[allow(clippy::let_underscore_must_use)]
                let _ = write(STDERR, b"?\n");
                let message = sess.error_msg.unwrap_or("warning: file modified");
                //[SD19-gate] write STDOUT/STDERR is best-effort
                #[allow(clippy::let_underscore_must_use)]
                let _ = write(STDERR, message.as_bytes());
                //[SD19-gate] write STDOUT/STDERR is best-effort
                #[allow(clippy::let_underscore_must_use)]
                let _ = write(STDERR, b"\n");
                if sess.scripted {
                    support::terminate(2);
                }
                sess.modified = false;
            }
            Err(e) => {
                sess.error_msg = Some(e.message);
                //[SD19-gate] write STDOUT/STDERR is best-effort
                #[allow(clippy::let_underscore_must_use)]
                let _ = write(STDERR, b"?\n");
                if sess.garrulous {
                    //[SD19-gate] write STDOUT/STDERR is best-effort
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = write(STDERR, e.message.as_bytes());
                    //[SD19-gate] write STDOUT/STDERR is best-effort
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = write(STDERR, b"\n");
                }
                if sess.scripted {
                    // Script mode quits on the first error
                    // (`main.c:268-275`).
                    support::terminate(2);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_truncates_and_creates() {
        // ed `w`: fopen("w") => O_WRONLY|O_CREAT|O_TRUNC (Minix values).
        assert_eq!(write_flags(false), O_WRONLY | O_CREAT | O_TRUNC);
        // Regression guard: O_CREAT is the Minix 0x200, not Linux's 0x40.
        assert_eq!(O_CREAT, 0x200);
        assert_eq!(O_TRUNC, 0x400);
    }

    #[test]
    fn write_append_appends_not_truncates() {
        // ed `W`: fopen("a") => O_WRONLY|O_CREAT|O_APPEND (0x8), never O_TRUNC.
        let flags = write_flags(true);
        assert_eq!(flags, O_WRONLY | O_CREAT | O_APPEND);
        assert_eq!(flags & O_TRUNC, 0);
        assert_ne!(flags & O_APPEND, 0);
    }
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
