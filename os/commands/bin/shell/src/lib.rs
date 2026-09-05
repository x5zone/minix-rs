#![cfg_attr(not(test), no_std)]

//! Shell command language core for Minix-RS commands.
//!
//! Covers `notes/rewrite/fork-syscall-rewrite/18-stage-commands/05-shell-family.md`:
//! the Almquist shell command language (`minix3/bin/sh/`: grammar in
//! `parser.c`, evaluation in `eval.c`, expansion in `expand.c`, redirection
//! in `redir.c`, job control in `jobs.c`, startup in `main.c`), the startup
//! files (`minix3/etc/profile`, `minix3/etc/shrc`, `minix3/etc/csh.*`), and
//! the environment face (`env`, `printenv`, `getopt`, `sysenv`,
//! `hostname`, `uname`).
//!
//! # Design
//!
//! A whole interactive shell (line editing, job control, process
//! management) is far beyond a first step, and pretending otherwise would
//! produce stubs. This crate therefore owns the parts that are pure text
//! processing: splitting an input line into words honouring quotes,
//! expanding variables, recognising redirection operators, and deciding
//! which startup files a new shell reads. Parsing, forking, waiting, and
//! terminal handling stay with later stages. The split mirrors how the C
//! shell itself is layered (reading, parsing, expanding, and executing are
//! separate files) and how Redox keeps its user program libraries free of
//! system calls.
//!
//! Everything borrows from the input or writes into caller provided
//! buffers: no heap, `no_std` throughout.
//!
//! # Modules
//!
//! - [`lexer`]: quote aware word splitting.
//! - [`expand`]: variable expansion over the [`expand::Environ`] trait.
//! - [`redir`]: redirection operator recognition.
//! - [`script`]: startup file sequencing.

pub mod expand;
pub mod lexer;
pub mod redir;
pub mod script;

/// Errors produced by this crate, mapped to classic Unix error numbers.
///
/// 22 marks malformed input (`EINVAL`): unterminated quotes, bad variable
/// syntax, unknown redirection shapes. The command layer reports these the
/// same way the C shell prints a syntax error and continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellError {
    /// Malformed input: unterminated quote, bad expansion, bad operator.
    InvalidSyntax,
    /// A value (variable, file, size) that does not fit its buffer.
    TooLong,
}

impl ShellError {
    /// The classic Unix error number for this failure.
    pub fn as_errno(self) -> i32 {
        match self {
            ShellError::InvalidSyntax => 22,
            ShellError::TooLong => 22,
        }
    }
}
