//! Minix-RS sh — the doing half over `minix_shell`.
//!
//! Ground truth: `minix3/bin/sh/` (Almquist shell) — the read loop in
//! `main.c`, word splitting/expansion/redirection recognition in the
//! library, command execution in `eval.c` (`evalcommand`: fork, redirect,
//! execve, wait). The library owns the pure text decisions; this program
//! runs them as a command executor:
//!
//! - `-c command` runs that string; a script operand is read and run line
//!   by line; with neither, standard input is read. `-i` is accepted and
//!   only documented: line editing does not exist here, and the `$ `
//!   prompt is printed for stdin sources regardless.
//! - Pipelines split on a standalone unquoted `|` word, one `pipe2` +
//!   `fork` per non-final stage, every stage waited, the last stage's
//!   status left. A `|` glued between words (`a|b`) is NOT a pipeline
//!   here — the lexer has no operator pass; the deviation is pinned on
//!   this line.
//! - Redirections `< > >> >| <> >& <&` (with optional leading descriptor
//!   digit) are recognised by the library per expanded word and applied
//!   in the child before exec. C removes redirection words before
//!   expansion and expands the target; here the target is expanded first
//!   — pinned simplification.
//! - Builtins in the parent for a single command: `exit [n]`, `cd [dir]`.
//!   A pipeline stage runs externally (the subshell shape), where the
//!   builtins do not exist and report as unknown commands. A builtin with
//!   redirections is rejected rather than half-applied.
//! - External commands search `PATH` (default `/bin:/usr/bin` when unset
//!   — C `_PATH_STDPATH` narrowed to this image); ENOENT/ENOTDIR keep
//!   searching, a found-but-failed exec leaves 126, an exhausted search
//!   leaves 127 (`eval.c:269-273`). The child exec builds the
//!   initial-stack frame through `minix_shell::exec_frame` (the init
//!   `execve.rs` precedent) and hands PM its five fields.
//!
//! Not modelled, and refusing to pretend: control flow (`if`/`while`/
//! `for`/`case`), functions, `FOO=bar cmd` assignments, variable export,
//! command substitution, and startup-file sourcing (`plan_startup`
//! decides them; executing them is the next stage). The stage contract
//! (`99-global-concepts.md §1`) rules the channels: no stdio, writes
//! through `minix_sys::write`.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

#[path = "../bin_support.rs"]
mod support;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use minix_shell::exec_frame::{stack_fill, stack_params};
use minix_shell::expand::{expand_word, Environ};
use minix_shell::lexer::split_words;
use minix_shell::redir::{parse_redir, RedirOp, Redirection};
use minix_sys::ipc::{DirectTrapTransport, IpcTransport as _};
use minix_sys::{dup2, exec, fork, pipe2, waitpid};
use support::{args, emit, envs, terminate, warn};

/// Expansion buffer per word (this layer caps the per-word render at a
/// page; C bounds the whole command by `ARG_MAX`).
const WORD_BUF: usize = 1024;
/// Read chunk for the line source.
const READ_CHUNK: usize = 4096;
/// Fallback search path when `PATH` is unset (C `_PATH_STDPATH`, narrowed
/// to the directories this image installs).
const DEFAULT_PATH: &str = "/bin:/usr/bin";

/// The live environment: inherited `KEY=VALUE` strings plus the positional
/// parameters stored as `"0"`, `"1"`, ... keys, feeding both `$name`
/// expansion and the child env walk.
struct RealEnv {
    entries: Vec<String>,
}

impl Environ for RealEnv {
    fn get(&self, name: &str) -> Option<&str> {
        let needle = format!("{name}=");
        self.entries
            .iter()
            .find(|entry| entry.starts_with(&needle))
            .map(|entry| &entry[needle.len()..])
    }
}

/// Target entry: the `crt0` birth chain resolves the symbol `main` by
/// name (consumer contract, `minix-rt/src/crt0.rs`) and its stage-6
/// `exit(main())` reads the `i32` slot.
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    let argv: Vec<String> = args();
    run_status(&argv)
}

/// Hosted entry: std builds go through rustc's start glue, whose `main`
/// must return a `Termination` type — `i32` is not one (rustc 1.94,
/// E0277), `()` is.
#[cfg(any(test, not(target_os = "none")))]
fn main() {
    let argv: Vec<String> = args();
    let status = run_status(&argv);
    support::terminate(status);
}

/// The whole program: flag parse, environment build, source dispatch.
fn run_status(argv: &[String]) -> i32 {
    let mut command: Option<String> = None;
    let mut script: Option<String> = None;
    let mut operands: Vec<String> = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].as_str();
        match arg {
            "--" => {
                i += 1;
                for item in &argv[i..] {
                    if script.is_none() {
                        script = Some(item.clone());
                    } else {
                        operands.push(item.clone());
                    }
                }
                break;
            }
            "-i" => {}
            "-c" => {
                i += 1;
                match argv.get(i) {
                    Some(cmd) => command = Some(cmd.clone()),
                    None => usage(),
                }
            }
            _ if arg.starts_with('-') && arg.len() > 1 => {
                warn(b"sh: only -c and -i are recognised\n");
                usage();
            }
            _ => {
                if command.is_none() && script.is_none() {
                    script = Some(arg.to_string());
                } else {
                    operands.push(arg.to_string());
                }
            }
        }
        i += 1;
    }

    let mut env = RealEnv {
        entries: envs(),
    };
    // Positional parameters: `$0` is the shell name, `$1..` the operands
    // after the script (C `main.c`'s flags hand-off).
    env.entries
        .push(format!("0={}", argv.first().cloned().unwrap_or_default()));
    for (n, op) in operands.iter().enumerate() {
        env.entries.push(format!("{}={}", n + 1, op));
    }

    if let Some(cmd) = command {
        return eval_text(&cmd, &mut env);
    }
    if let Some(path) = script {
        let bytes = match support::read_file(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                warn(format!("sh: cannot open {path}\n").as_bytes());
                return 127;
            }
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        return eval_text(&text, &mut env);
    }
    eval_stdin(&mut env)
}

fn usage() -> ! {
    warn(b"usage: sh [-i] [-c command] [script [operand ...]]\n");
    terminate(2)
}

/// Evaluates every line of an in-memory text (`-c` string or script).
fn eval_text(text: &str, env: &mut RealEnv) -> i32 {
    let mut status = 0;
    for line in text.split('\n') {
        let trimmed = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = trimmed.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        status = eval_line(trimmed, env, status);
    }
    status
}

/// Reads standard input line by line, prompting before each read (the C
/// interactive `$ `; the prompt is the only interactive face — no line
/// editing, `-i` changes nothing).
fn eval_stdin(env: &mut RealEnv) -> i32 {
    let mut status = 0;
    let mut carried: Vec<u8> = Vec::new();
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        emit(b"$ ");
        let line = loop {
            if let Some(pos) = carried.iter().position(|&b| b == b'\n') {
                let mut raw: Vec<u8> = carried.drain(..=pos).collect();
                raw.pop();
                break String::from_utf8_lossy(&raw).into_owned();
            }
            match minix_sys::read(support::STDIN, &mut chunk) {
                Ok(0) => {
                    if carried.is_empty() {
                        return status;
                    }
                    let line = String::from_utf8_lossy(&carried).into_owned();
                    carried.clear();
                    break line;
                }
                Ok(n) => carried.extend_from_slice(&chunk[..n]),
                Err(_) => return status,
            }
        };
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        status = eval_line(trimmed, env, status);
    }
}

/// Evaluates one command line: split, expand, pipeline-split, run (C
/// `eval.c`'s command chain, one command at a time).
fn eval_line(line: &str, env: &mut RealEnv, last: i32) -> i32 {
    let (words, count) = match split_words(line) {
        Ok(split) => split,
        Err(_) => {
            warn(b"sh: syntax error\n");
            return 2;
        }
    };
    let pid = minix_sys::getpid().unwrap_or(0) as u32;
    let mut expanded: Vec<String> = Vec::new();
    for word in words.iter().take(count) {
        let mut buf = [0u8; WORD_BUF];
        match expand_word(word, env, (last & 0xFF) as u8, pid, &mut buf) {
            Ok(n) => expanded.push(String::from_utf8_lossy(&buf[..n]).into_owned()),
            Err(_) => {
                warn(b"sh: bad substitution\n");
                return 2;
            }
        }
    }

    // Pipeline split on a standalone unquoted `|` word (see the module
    // header for the glued-operator deviation).
    let mut stages: Vec<Vec<String>> = vec![Vec::new()];
    for word in expanded {
        if word == "|" {
            stages.push(Vec::new());
        } else {
            if let Some(last) = stages.last_mut() {
                last.push(word);
            }
        }
    }
    if (stages.len() > 1 && stages.iter().any(|s| s.is_empty()))
        || stages.first().is_some_and(|s| s.is_empty())
    {
        warn(b"sh: syntax error\n");
        return 2;
    }
    run_pipeline(&stages, env)
}

/// Separates redirection words from the command words of one stage.
/// A dangling operator (operator word with no following target word) is a
/// syntax error → `None`.
fn take_redirects(words: &[String]) -> Option<(Vec<String>, Vec<Redirection<'_>>)> {
    const BARE_OPS: [(&str, RedirOp, u8); 8] = [
        ("<", RedirOp::Read, 0),
        (">", RedirOp::Write, 1),
        (">>", RedirOp::Append, 1),
        (">|", RedirOp::Clobber, 1),
        ("<>", RedirOp::ReadWrite, 0),
        (">&", RedirOp::DuplicateWrite, 1),
        ("<&", RedirOp::DuplicateRead, 0),
        ("2>", RedirOp::Write, 2),
    ];
    let mut command = Vec::new();
    let mut redirects = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let word = &words[index];
        match parse_redir(word) {
            Ok(Some(red)) => redirects.push(red),
            other @ (Ok(None) | Err(_)) => {
                // Bare operator form: the whole word IS the operator and
                // the NEXT word carries the target. parse_redir only sees
                // the attached form.
                let bare = BARE_OPS.iter().find(|(op, _, _)| *op == word.as_str());
                if let Some(&(_, kind, fd)) = bare {
                    let target = words.get(index + 1)?;
                    redirects.push(Redirection { op: kind, fd, target });
                    index += 2;
                    continue;
                }
                let _ = other;
                command.push(word.clone());
            }
        }
        index += 1;
    }
    Some((command, redirects))
}

/// Runs one pipeline: a `pipe2` + `fork` per non-final stage, every stage
/// waited, the final stage's status returned (C `evalcommand`'s pipe
/// shape, `eval.c:215-224`).
fn run_pipeline(stages: &[Vec<String>], env: &mut RealEnv) -> i32 {
    let last = stages.len() - 1;
    let mut children: Vec<i32> = Vec::new();
    let mut carry_read: Option<i32> = None;
    for (index, stage) in stages.iter().enumerate() {
        let final_stage = index == last;
        let (command, redirects) = match take_redirects(stage) {
            Some(split) => split,
            None => {
                warn(b"sh: syntax error: missing redirection target\n");
                return 2;
            }
        };
        if command.is_empty() && redirects.is_empty() {
            warn(b"sh: syntax error: empty command\n");
            return 2;
        }

        // Single-command builtins run in the parent (C `evalbuiltin`).
        // A builtin carrying redirections or sitting in a pipeline would
        // need child-side descriptor juggling for an in-parent effect —
        // rejected rather than half-applied.
        if final_stage && stages.len() == 1 && redirects.is_empty() {
            match command.first().map(String::as_str) {
                Some("exit") => {
                    let code = command.get(1).and_then(|n| n.parse::<i32>().ok()).unwrap_or(0);
                    terminate(code);
                }
                Some("cd") => {
                    let target = command
                        .get(1)
                        .cloned()
                        .or_else(|| env.get("HOME").map(|h| h.to_string()));
                    let Some(target) = target else {
                        warn(b"sh: cd: HOME not set\n");
                        return 1;
                    };
                    return match minix_sys::chdir(&target) {
                        Ok(()) => 0,
                        Err(_) => {
                            warn(format!("sh: cd: {target}: cannot change directory\n").as_bytes());
                            1
                        }
                    };
                }
                _ => {}
            }
        }

        let pipe = if final_stage { None } else { pipe2(0).ok() };
        match fork() {
            Ok(0) => {
                // Child: rewire the carried pipe ends, apply redirects,
                // exec (exec never returns; a failure exits below).
                if let Some(read) = carry_read {
                    let _ = dup2(read, 0);
                    let _ = minix_sys::close(read);
                }
                if let Some((_, write)) = pipe {
                    let _ = dup2(write, 1);
                    let _ = minix_sys::close(write);
                }
                if let Some((read, _)) = pipe {
                    let _ = minix_sys::close(read);
                }
                if command.is_empty() {
                    // Redirection-only stage: create/consume the files and
                    // succeed (C `>` with no command still touches files).
                    let _ = apply_redirs(&redirects);
                    terminate(0);
                }
                child_exec(&command, &redirects, env);
            }
            Ok(pid) => {
                if let Some(read) = carry_read {
                    let _ = minix_sys::close(read);
                }
                if let Some((_, write)) = pipe {
                    let _ = minix_sys::close(write);
                }
                carry_read = pipe.map(|(read, _)| read);
                children.push(pid);
            }
            Err(_) => {
                warn(b"sh: cannot fork\n");
                break;
            }
        }
    }
    if let Some(read) = carry_read {
        let _ = minix_sys::close(read);
    }
    let mut status = 0;
    for (index, pid) in children.iter().enumerate() {
        let mut raw = 0;
        if waitpid(*pid, &mut raw, 0).is_err() {
            continue;
        }
        if index + 1 == children.len() {
            status = wait_status(raw);
        }
    }
    status
}

/// Decodes a C wait status the way the shell's status rules read it:
/// exited → the low exit byte; killed → 128 + signal (C
/// `WIFEXITED`/`WEXITSTATUS`, `sys/wait.h`).
fn wait_status(status: i32) -> i32 {
    let signal = status & 0x7F;
    if signal == 0 {
        (status >> 8) & 0xFF
    } else if signal != 0x7F {
        128 + signal
    } else {
        1
    }
}

/// Applies redirections to the current process (child side, C `redirect`).
/// Flag values are the C `fcntl.h` constants the wire carries verbatim.
fn apply_redirs(redirects: &[Redirection<'_>]) -> Result<(), minix_sys::Errno> {
    const O_RDONLY: i32 = 0x0000_0000;
    const O_WRONLY: i32 = 0x0000_0001;
    const O_RDWR: i32 = 0x0000_0002;
    const O_APPEND: i32 = 0x0000_0008; // fcntl.h:76
    const O_CREAT: i32 = 0x0000_0200; // fcntl.h:96
    const O_TRUNC: i32 = 0x0000_0400; // fcntl.h:100
    use minix_types::types::errno as e;
    for red in redirects {
        let outcome = match red.op {
            RedirOp::DuplicateWrite | RedirOp::DuplicateRead => {
                if red.target == "-" {
                    let _ = minix_sys::close(red.fd as i32);
                    Ok(())
                } else {
                    let source = red.target.parse::<i32>().map_err(|_| minix_sys::Errno::from_i32(e::EINVAL))?;
                    dup2(source, red.fd as i32).map(|_| ())
                }
            }
            RedirOp::Read => with_fd(red.fd, minix_sys::open(red.target, O_RDONLY, 0)),
            RedirOp::Write | RedirOp::Clobber => {
                with_fd(red.fd, minix_sys::open(red.target, O_WRONLY | O_CREAT | O_TRUNC, 0o644))
            }
            RedirOp::Append => {
                with_fd(red.fd, minix_sys::open(red.target, O_WRONLY | O_CREAT | O_APPEND, 0o644))
            }
            RedirOp::ReadWrite => {
                with_fd(red.fd, minix_sys::open(red.target, O_RDWR | O_CREAT, 0o644))
            }
        };
        outcome.map_err(|e| if e.to_i32() == 0 { minix_sys::Errno::from_i32(2) } else { e })?;
    }
    Ok(())
}

/// Dups `fd` onto `red.fd` and closes the original when they differ.
fn with_fd(red_fd: u8, opened: Result<i32, minix_sys::Errno>) -> Result<(), minix_sys::Errno> {
    let fd = opened?;
    let _ = dup2(fd, red_fd as i32);
    if fd != red_fd as i32 {
        let _ = minix_sys::close(fd);
    }
    Ok(())
}

/// The fork side every external stage runs: redirect, search `PATH`, exec
/// a prepared frame (C `evalcommand`'s child + the `execvpe` search).
/// Never returns: a successful exec replaces the process, any failure
/// exits 126/127.
fn child_exec(stage: &[String], redirects: &[Redirection<'_>], env: &RealEnv) -> ! {
    if apply_redirs(redirects).is_err() {
        warn(b"sh: cannot redirect\n");
        terminate(2);
    }
    let Some(program) = stage.first() else { terminate(0) };
    let argv: Vec<&str> = stage.iter().map(String::as_str).collect();
    let mut candidates: Vec<String> = Vec::new();
    if program.contains('/') {
        candidates.push(program.clone());
    } else {
        let path = env
            .get("PATH")
            .map(|p| p.to_string())
            .unwrap_or_else(|| DEFAULT_PATH.to_string());
        for dir in path.split(':') {
            let dir = if dir.is_empty() { "." } else { dir };
            candidates.push(format!("{dir}/{program}"));
        }
    }
    for candidate in &candidates {
        let code = exec_candidate(candidate, &argv, env);
        if code == minix_types::types::errno::ENOENT
            || code == minix_types::types::errno::ENOTDIR
        {
            continue;
        }
        let text = if code == minix_types::types::errno::EACCES {
            "Permission denied"
        } else if code == minix_types::types::errno::ENOEXEC {
            "Exec format error"
        } else {
            "cannot execute"
        };
        warn(format!("sh: {program}: {text}\n").as_bytes());
        terminate(126);
    }
    warn(format!("sh: {program}: not found\n").as_bytes());
    terminate(127)
}

/// Builds the exec frame for `candidate` and calls PM (C `execve.c:33-58`:
/// size the stack, take a buffer, fill it, clear-and-fill the message,
/// call). A successful exec never returns, so every return here is the
/// failing attempt's Errno.
fn exec_candidate(candidate: &str, argv: &[&str], env: &RealEnv) -> i32 {
    let envp: Vec<String> = env.entries.clone();
    let envp_refs: Vec<&str> = envp.iter().map(String::as_str).collect();

    // The path travels as a C string: pointer plus length including the
    // NUL (C: `execve.c:48-49`).
    let mut path = Vec::with_capacity(candidate.len() + 1);
    path.extend_from_slice(candidate.as_bytes());
    path.push(0);

    let frame_size = match stack_params(argv, &envp_refs) {
        Ok(size) => size,
        Err(code) => return code,
    };
    let stack_top = match new_image_stack_top() {
        Some(top) => top,
        None => return minix_types::types::errno::EIO,
    };
    let Some(vsp) = stack_top.checked_sub(frame_size as u64) else {
        return minix_types::types::errno::E2BIG;
    };

    let mut frame = vec![0u8; frame_size];
    let ps_offset = stack_fill(argv, &envp_refs, vsp, &mut frame);

    match minix_sys::pm::prepare_exec(
        path.as_ptr() as u64,
        path.len(),
        frame.as_ptr() as u64,
        frame.len(),
        vsp + ps_offset,
    ) {
        Ok(prepared) => exec(prepared).to_i32(),
        Err(e) => e.to_i32(),
    }
}


/// The initial stack pointer the kernel will give the new image (C:
/// `minix_get_user_sp`, `kernel_utils.c:40-49` — the kerninfo page query;
/// the init `execve.rs` precedent verbatim, hosted builds get the honest
/// EIO fallback). `None` maps to EIO at the call site.
fn new_image_stack_top() -> Option<u64> {
    let page = DirectTrapTransport.query_kerninfo_page().ok()?;
    if page == 0 {
        return None;
    }
    // SAFETY: the kernel published and user-mapped this page before handing
    // out its address (the boot handoff owns the mapping); user mode treats
    // it as read-only. Zero returned above, so the pointer is the kernel's.
    let info = unsafe { &*(page as *const minix_types::types::MinixKerninfo) };
    if info.kuserinfo == 0 {
        return None;
    }
    // SAFETY: same kernel-published page family; `kuserinfo` points at the
    // leading `KuserInfo`, and the size field gates the field read exactly
    // as KUSERINFO_HAS_FIELD does in C (`minix3/minix/include/minix/type.h:
    // 210-211`).
    let user = unsafe { &*(info.kuserinfo as *const minix_types::types::KuserInfo) };
    const FIELD_OFFSET: u64 = 8; // offsetof(KuserInfo, kui_user_sp)
    const FIELD_SIZE: u64 = 8;
    if user.kui_size < FIELD_OFFSET + FIELD_SIZE {
        return None;
    }
    Some(user.kui_user_sp)
}
