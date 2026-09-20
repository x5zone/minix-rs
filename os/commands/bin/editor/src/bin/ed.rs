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
use minix_sys::{read, write, Fd};

/// POSIX standard descriptors.
const STDIN: Fd = 0;
const STDOUT: Fd = 1;
const STDERR: Fd = 2;


/// The production I/O half: bytes in and out through `minix_sys`, files
/// through the open/read/write/close family (L10's wrappers, one call
/// each — `io.c`'s `fopen`/`fclose` faces).
struct TtyIo {
    scripted: bool,
}

impl EditorIo for TtyIo {
    fn emit(&mut self, bytes: &[u8]) {
        let _ = write(STDOUT, bytes);
    }

    fn emit_err(&mut self, bytes: &[u8]) {
        let _ = write(STDERR, bytes);
    }

    fn read_file(&mut self, name: &str, out: &mut [u8]) -> Result<usize, minix_editor::EditorError> {
        let fd = minix_sys::open(name, 0, 0).map_err(|_| minix_editor::EditorError::InvalidArgument)?;
        let mut total = 0;
        while total < out.len() {
            match read(fd, &mut out[total..]) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(_) => {
                    let _ = minix_sys::close(fd);
                    return Err(minix_editor::EditorError::InvalidArgument);
                }
            }
        }
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
        // O_WRONLY 1 | O_CREAT 0x40 | (O_APPEND 0x400 when appending);
        // mode 0o644 — `io.c`'s fopen("w"/"a") face.
        let mut flags = 1 | 0x40;
        if append {
            flags |= 0x400;
        }
        let fd = minix_sys::open(name, flags, 0o644)
            .map_err(|_| minix_editor::EditorError::InvalidArgument)?;
        let mut done = 0;
        while done < data.len() {
            match write(fd, &data[done..]) {
                Ok(0) => break,
                Ok(n) => done += n,
                Err(_) => {
                    let _ = minix_sys::close(fd);
                    return Err(minix_editor::EditorError::InvalidArgument);
                }
            }
        }
        let _ = minix_sys::close(fd);
        Ok(done)
    }
}

/// One line from standard input, newline stripped; `None` at end of
/// input. Non-UTF-8 input is rejected (the store is a text store —
/// 09-editors.md §5 declares the boundary).
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
    let mut io = TtyIo { scripted };

    // The initial file (`main.c:179-193`): read it, name it, quit(2) in
    // script mode when the read fails.
    if let Some(name) = names.first() {
        if name.is_empty() {
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
            let _ = write(STDOUT, sess.prompt_bytes());
        }
        let Some(line) = read_line(&mut buf) else {
            // End of input: `q` rules apply (`main.c:206-221`) — modified
            // warns once interactively and quits in script mode.
            if sess.modified && !sess.scripted {
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
                let _ = write(STDERR, b"?\n");
                let message = sess.error_msg.unwrap_or("warning: file modified");
                let _ = write(STDERR, message.as_bytes());
                let _ = write(STDERR, b"\n");
                if sess.scripted {
                    support::terminate(2);
                }
                sess.modified = false;
            }
            Err(e) => {
                sess.error_msg = Some(e.message);
                let _ = write(STDERR, b"?\n");
                if sess.garrulous {
                    let _ = write(STDERR, e.message.as_bytes());
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


#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
