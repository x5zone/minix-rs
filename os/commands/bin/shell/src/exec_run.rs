//! Exec search and frame submission, shared by `sh` and `env`.
//!
//! Ground truth: `minix3/lib/libc/gen/execvp.c` (`execvpe`, lines 59-140)
//! for the search, and `minix3/usr.bin/env/env.c:78-85` /
//! `minix3/commands/...` callers for the error mapping. The initial stack
//! image itself is built by [`crate::exec_frame`] (`minix_stack_params` /
//! `minix_stack_fill`); this module only walks `PATH` and hands each
//! candidate's frame to PM.
//!
//! A successful exec never returns: it replaces the calling process. Every
//! return value here is the failing errno, and the caller owns the
//! diagnostic and the exit code (C `execvp` returns -1 with `errno` set;
//! `env` maps ENOENT to 127 and everything else to 126).
//!
//! Registered deviations from C, both invisible to the common path:
//! - C bounds each candidate at `PATH_MAX` and prints `execvp: <dir>: path
//!   too long` when it overflows; this tree's candidates are heap strings
//!   with no fixed buffer, so the guard has nothing to protect and is not
//!   reproduced.
//! - C sleeps 1-3 s and retries the same candidate on `ETXTBSY`; this tree
//!   has no sleep primitive, so `ETXTBSY` returns to the caller unchanged
//!   (C's retries end in the same errno when the file stays open).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::exec_frame::{stack_fill, stack_params};

/// The search path when the environment carries no `PATH` (C
/// `_PATH_DEFPATH`, narrowed to the directories this image installs).
pub const DEFAULT_PATH: &str = "/bin:/usr/bin";

/// The candidates `execvpe` tries for `name` under `path` (`execvp.c:101-124`):
/// a name containing `/` is used verbatim; otherwise each `PATH` element is
/// joined, and an empty element means the current directory.
pub fn path_candidates(name: &str, path: &str) -> Vec<String> {
    if name.contains('/') {
        return alloc::vec![String::from(name)];
    }
    let mut out = Vec::new();
    for dir in path.split(':') {
        let dir = if dir.is_empty() { "." } else { dir };
        out.push(format!("{dir}/{name}"));
    }
    out
}

/// What one failed `execve` attempt means for the search (`execvp.c:125-139`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// `ENOENT`/`ENOTDIR`: try the next candidate. `EACCES` is a continue
    /// too, but the search remembers it and prefers it over `ENOENT` when
    /// every candidate has been tried.
    Continue {
        /// The attempt failed with `EACCES`.
        denied: bool,
    },
    /// `ENOEXEC`: C retries the candidate through `/bin/sh` before giving up.
    ShellFallback,
    /// Any other errno stops the search and comes back to the caller.
    Return(i32),
}

/// Classify one candidate's errno (`execvp.c:125-139`).
pub fn step(errno: i32) -> Step {
    match errno {
        minix_sys::errno::EACCES => Step::Continue { denied: true },
        minix_sys::errno::ENOENT | minix_sys::errno::ENOTDIR => Step::Continue { denied: false },
        minix_sys::errno::ENOEXEC => Step::ShellFallback,
        other => Step::Return(other),
    }
}

/// Exec one framed candidate (`execve.c:33-58`): size the stack, take a
/// buffer, fill it, hand the C-string path and the frame to PM. A
/// successful exec never returns, so a return value is the failing
/// attempt's errno.
pub fn exec_one(candidate: &str, argv: &[&str], envp: &[&str]) -> i32 {
    // The path travels as a C string: pointer plus length including the
    // NUL (`execve.c:48-49`).
    let mut path = Vec::with_capacity(candidate.len() + 1);
    path.extend_from_slice(candidate.as_bytes());
    path.push(0);

    let frame_size = match stack_params(argv, envp) {
        Ok(size) => size,
        Err(code) => return code,
    };
    let stack_top = match minix_sys::user_stack_top() {
        Ok(top) => top,
        Err(code) => return code.to_i32(),
    };
    let Some(vsp) = stack_top.checked_sub(frame_size as u64) else {
        return minix_sys::errno::E2BIG;
    };

    let mut frame = alloc::vec![0u8; frame_size];
    let ps_offset = stack_fill(argv, envp, vsp, &mut frame);

    match minix_sys::pm::prepare_exec(
        path.as_ptr() as u64,
        path.len(),
        frame.as_ptr() as u64,
        frame.len(),
        vsp + ps_offset,
    ) {
        Ok(prepared) => minix_sys::exec(prepared).to_i32(),
        Err(e) => e.to_i32(),
    }
}

/// `execvpe` (`execvp.c:59-140`): search `path` for `name` and exec its
/// frame. `path` is the value of `PATH` the new program will see — the
/// caller passes the environment it hands to `exec_one` (C reads it back
/// from the mutated `environ`).
///
/// Returns the errno of the failed search; never returns on success.
pub fn exec_search(name: &str, argv: &[&str], envp: &[&str], path: Option<&str>) -> i32 {
    if name.is_empty() {
        // `execvp.c:84-87`: the empty name is not even looked up.
        return minix_sys::errno::ENOENT;
    }
    let path = path.unwrap_or(DEFAULT_PATH);
    let mut denied = false;
    let mut last = minix_sys::errno::ENOENT;
    for candidate in path_candidates(name, path) {
        let errno = exec_one(&candidate, argv, envp);
        last = errno;
        match step(errno) {
            Step::Continue { denied: d } => denied |= d,
            Step::ShellFallback => {
                // `execvp.c:126-137`: re-run the candidate as a shell
                // script, `[/bin/sh, candidate, argv[1..]]`.
                let mut shell_argv: Vec<&str> = Vec::with_capacity(argv.len() + 1);
                shell_argv.push("/bin/sh");
                shell_argv.push(candidate.as_str());
                if argv.len() > 1 {
                    shell_argv.extend_from_slice(&argv[1..]);
                }
                return exec_one("/bin/sh", &shell_argv, envp);
            }
            Step::Return(code) => return code,
        }
    }
    if denied {
        minix_sys::errno::EACCES
    } else {
        last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slashed_name_is_used_verbatim() {
        assert_eq!(path_candidates("/bin/ls", "/bin:/usr/bin"), ["/bin/ls"]);
        assert_eq!(path_candidates("./x", "/bin"), ["./x"]);
    }

    #[test]
    fn each_path_element_is_joined_and_empty_means_here() {
        assert_eq!(
            path_candidates("ls", "/bin:/usr/bin"),
            ["/bin/ls", "/usr/bin/ls"]
        );
        assert_eq!(path_candidates("ls", ":/bin:"), ["./ls", "/bin/ls", "./ls"]);
        assert_eq!(path_candidates("ls", ""), ["./ls"]);
    }

    #[test]
    fn the_step_table_matches_execvpe() {
        assert_eq!(
            step(minix_sys::errno::EACCES),
            Step::Continue { denied: true }
        );
        assert_eq!(
            step(minix_sys::errno::ENOENT),
            Step::Continue { denied: false }
        );
        assert_eq!(
            step(minix_sys::errno::ENOTDIR),
            Step::Continue { denied: false }
        );
        assert_eq!(step(minix_sys::errno::ENOEXEC), Step::ShellFallback);
        assert_eq!(
            step(minix_sys::errno::EPERM),
            Step::Return(minix_sys::errno::EPERM)
        );
    }

    #[test]
    fn the_empty_name_answers_enoent_without_a_lookup() {
        assert_eq!(exec_search("", &[""], &[], None), minix_sys::errno::ENOENT);
    }
}
