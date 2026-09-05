//! Signal number constants.
//!
//! Corresponds to Minix3's `<sys/signal.h>` user-space numbering (signals
//! 1 through 32; real-time signals 33 through 63 are kernel-only and not
//! exposed to userland). Values follow the NetBSD-derived table Minix3
//! ships, verified line by line against `minix3/sys/sys/signal.h`.
//!
//! The range rule lives here as well, so every caller shares one check:
//! signal numbers below zero or at/above [`MAX_SIGNAL_NUMBER`] are rejected
//! before any round trip. Zero is allowed (existence check without
//! delivery), matching the C library's range test.

/// Highest signal number plus one (exclusive upper bound).
///
/// C: `_NSIG 64` (`minix3/sys/sys/signal.h:45`).
pub const MAX_SIGNAL_NUMBER: i32 = 64;

/// Reports whether a number passes the shared range check.
///
/// Mirrors the C library test (`sig < 0 || sig >= _NSIG` rejected): zero is
/// allowed, negatives and 64-plus are not.
pub const fn is_valid_signal_number(number: i32) -> bool {
    number >= 0 && number < MAX_SIGNAL_NUMBER
}

/// Hangup. C: `SIGHUP 1` (`signal.h:52`).
pub const SIGNAL_HANGUP: i32 = 1;
/// Interrupt. C: `SIGINT 2` (`signal.h:53`).
pub const SIGNAL_INTERRUPT: i32 = 2;
/// Quit. C: `SIGQUIT 3` (`signal.h:54`).
pub const SIGNAL_QUIT: i32 = 3;
/// Illegal instruction. C: `SIGILL 4` (`signal.h:55`).
pub const SIGNAL_ILLEGAL_INSTRUCTION: i32 = 4;
/// Trace trap. C: `SIGTRAP 5` (`signal.h:56`).
pub const SIGNAL_TRACE_TRAP: i32 = 5;
/// Abort. C: `SIGABRT 6` (`signal.h:57`).
pub const SIGNAL_ABORT: i32 = 6;
/// EMT instruction. C: `SIGEMT 7` (`signal.h:59`).
pub const SIGNAL_EMT: i32 = 7;
/// Floating point exception. C: `SIGFPE 8` (`signal.h:60`).
pub const SIGNAL_FLOATING_POINT: i32 = 8;
/// Kill, uncatchable. C: `SIGKILL 9` (`signal.h:61`).
pub const SIGNAL_KILL: i32 = 9;
/// Bus error. C: `SIGBUS 10` (`signal.h:62`).
pub const SIGNAL_BUS_ERROR: i32 = 10;
/// Segmentation violation. C: `SIGSEGV 11` (`signal.h:63`).
pub const SIGNAL_SEGMENT_VIOLATION: i32 = 11;
/// Bad system call argument. C: `SIGSYS 12` (`signal.h:64`).
pub const SIGNAL_BAD_SYSCALL: i32 = 12;
/// Write on a pipe with no reader. C: `SIGPIPE 13` (`signal.h:65`).
pub const SIGNAL_BROKEN_PIPE: i32 = 13;
/// Alarm clock. C: `SIGALRM 14` (`signal.h:66`).
pub const SIGNAL_ALARM: i32 = 14;
/// Software termination. C: `SIGTERM 15` (`signal.h:67`).
pub const SIGNAL_TERMINATE: i32 = 15;
/// Urgent I/O condition. C: `SIGURG 16` (`signal.h:68`).
pub const SIGNAL_URGENT: i32 = 16;
/// Sendable stop. C: `SIGSTOP 17` (`signal.h:69`).
pub const SIGNAL_STOP: i32 = 17;
/// Terminal stop. C: `SIGTSTP 18` (`signal.h:70`).
pub const SIGNAL_TERMINAL_STOP: i32 = 18;
/// Continue. C: `SIGCONT 19` (`signal.h:71`).
pub const SIGNAL_CONTINUE: i32 = 19;
/// Child stopped or exited. C: `SIGCHLD 20` (`signal.h:72`).
pub const SIGNAL_CHILD: i32 = 20;
/// Background terminal read. C: `SIGTTIN 21` (`signal.h:73`).
pub const SIGNAL_TERMINAL_INPUT: i32 = 21;
/// Background terminal write. C: `SIGTTOU 22` (`signal.h:74`).
pub const SIGNAL_TERMINAL_OUTPUT: i32 = 22;
/// I/O possible. C: `SIGIO 23` (`signal.h:75`).
pub const SIGNAL_IO_READY: i32 = 23;
/// CPU time limit exceeded. C: `SIGXCPU 24` (`signal.h:76`).
pub const SIGNAL_CPU_LIMIT: i32 = 24;
/// File size limit exceeded. C: `SIGXFSZ 25` (`signal.h:77`).
pub const SIGNAL_FILE_SIZE_LIMIT: i32 = 25;
/// Virtual time alarm. C: `SIGVTALRM 26` (`signal.h:78`).
pub const SIGNAL_VIRTUAL_ALARM: i32 = 26;
/// Profiling alarm. C: `SIGPROF 27` (`signal.h:79`).
pub const SIGNAL_PROFILE_ALARM: i32 = 27;
/// Window size changed. C: `SIGWINCH 28` (`signal.h:80`).
pub const SIGNAL_WINDOW_CHANGE: i32 = 28;
/// Information request. C: `SIGINFO 29` (`signal.h:81`).
pub const SIGNAL_INFO: i32 = 29;
/// User signal 1. C: `SIGUSR1 30` (`signal.h:82`).
pub const SIGNAL_USER_1: i32 = 30;
/// User signal 2. C: `SIGUSR2 31` (`signal.h:83`).
pub const SIGNAL_USER_2: i32 = 31;
/// Power fail or restart. C: `SIGPWR 32` (`signal.h:84`).
pub const SIGNAL_POWER: i32 = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_numbers_match_c_header() {
        assert_eq!(SIGNAL_HANGUP, 1);
        assert_eq!(SIGNAL_KILL, 9);
        assert_eq!(SIGNAL_TERMINATE, 15);
        assert_eq!(SIGNAL_CHILD, 20);
        assert_eq!(SIGNAL_USER_1, 30);
        assert_eq!(SIGNAL_POWER, 32);
        assert_eq!(MAX_SIGNAL_NUMBER, 64);
    }

    #[test]
    fn test_range_check_matches_c_library_rule() {
        assert!(is_valid_signal_number(0));
        assert!(is_valid_signal_number(31));
        assert!(!is_valid_signal_number(-1));
        assert!(!is_valid_signal_number(64));
    }
}
