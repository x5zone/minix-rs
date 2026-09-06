//! Kernel-entry trap style: how a process most recently entered the kernel.
//!
//! C records `p_kern_trap_style` in the per-process save frame (i386
//! archtypes.h:36) at every kernel entry and consumes it at the single
//! return-to-user dispatch point `restore_user_context()`
//! (arch_system.c:577-610): the recorded style selects the register
//! restore sequence, because entry mechanisms differ in how much state
//! they save — a full interrupt frame carries everything the mode switch
//! needs, while the skinny SYSCALL entry relies on the user-side
//! trampoline to restore the rest. The value also rides the signal
//! round-trip: `do_sigsend` stamps it into the sigframe (do_sigsend.c:77)
//! and `do_sigreturn` re-records it via `arch_proc_setcontext`
//! (do_sigreturn.c:81), so a signal handler returns through the same path
//! the interrupted code entered by.
//!
//! minix-rs models the style as an enum instead of C's bare `int` so the
//! return path dispatches on named variants and unknown raw values are
//! rejected at the sigreturn boundary rather than panicking the kernel
//! later (C: `panic("unknown trap style recorded")` — arch_system.c:604,
//! reachable from a user-crafted sigcontext; see the sigreturn call site
//! for the `MINIX3 BUG` note).

/// How the process most recently entered the kernel.
///
/// Discriminants mirror C's `KTS_*` numbering (i386 archconst.h:167-172)
/// because the raw value is user-visible: it is stored in the sigcontext
/// and handed back by `sigreturn`, so keeping C's numbering makes the
/// sigframe byte-for-byte comparable with the C implementation.
///
/// `KTS_INT_UM` (4, entry from usermapped kernel code) and `KTS_SYSENTER`
/// (6, i386-only mechanism) are deliberately not modeled: minix-rs has no
/// usermapped kernel region on 64-bit (the W-2 exclusion in the kernel
/// todo) and no SYSENTER entry, so no entry path can ever produce them.
///
/// The recording half (entry paths stamping the style) lands with the asm
/// trap-entry work — smp_todo S-8. Until then every process carries
/// [`TrapStyle::NoEntry`], which the return path refuses to dispatch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrapStyle {
    /// No kernel entry recorded — the process has never entered, or the
    /// previous return consumed the record. C: `KTS_NONE` — archconst.h:167.
    #[default]
    NoEntry = 1,
    /// Exception or hardware interrupt: the entry saved a full frame.
    /// C: `KTS_INT_HARD` — archconst.h:168.
    IntHard = 2,
    /// Soft entry from user code (trap instruction). C: `KTS_INT_ORIG` —
    /// archconst.h:169.
    IntOrig = 3,
    /// Explicit full-context restore request. C: `KTS_FULLCONTEXT` —
    /// archconst.h:171.
    FullContext = 5,
    /// Fast syscall entry (x86-64 `syscall`, paired with a `sysret`-class
    /// return). C: `KTS_SYSCALL` — archconst.h:172.
    Syscall = 7,
}

impl TrapStyle {
    /// Parse the raw sigcontext value; `None` for values no entry path can
    /// produce (including the two unmodeled C styles 4 and 6).
    pub fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            1 => Some(Self::NoEntry),
            2 => Some(Self::IntHard),
            3 => Some(Self::IntOrig),
            5 => Some(Self::FullContext),
            7 => Some(Self::Syscall),
            _ => None,
        }
    }

    /// Raw sigcontext value (C `KTS_*` numbering).
    pub fn raw(self) -> i32 {
        self as i32
    }

    /// The register-restore sequence this style requires at the
    /// return-to-user dispatch.
    ///
    /// `None` = no entry recorded: dispatching to user would restore an
    /// arbitrary register file, which C answers with
    /// `panic("no entry trap style known")` (arch_system.c:597-598).
    pub fn return_sequence(self) -> Option<ReturnSequence> {
        match self {
            Self::IntHard | Self::IntOrig | Self::FullContext => Some(ReturnSequence::FullContext),
            Self::Syscall => Some(ReturnSequence::FastSyscall),
            Self::NoEntry => None,
        }
    }
}

/// The register-restore sequence a recorded style requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnSequence {
    /// The mode-switch frame carries all user state (iretq/eret/sret
    /// class). C: `restore_user_context_int` — arch_system.c:600-603.
    FullContext,
    /// The entry saved a skinny frame; the return must pair it
    /// (sysret-class). C: `restore_user_context_syscall` —
    /// arch_system.c:592-595. Lands with the asm entry work (S-8).
    FastSyscall,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trap_style_raw_matches_c_kts_numbering() {
        // C: i386 archconst.h:167-172 — the sigcontext ABI carries these
        // raw values, so the numbering is contract, not implementation.
        assert_eq!(TrapStyle::NoEntry.raw(), 1);
        assert_eq!(TrapStyle::IntHard.raw(), 2);
        assert_eq!(TrapStyle::IntOrig.raw(), 3);
        assert_eq!(TrapStyle::FullContext.raw(), 5);
        assert_eq!(TrapStyle::Syscall.raw(), 7);
    }

    #[test]
    fn test_trap_style_from_raw_roundtrip() {
        for style in [
            TrapStyle::NoEntry,
            TrapStyle::IntHard,
            TrapStyle::IntOrig,
            TrapStyle::FullContext,
            TrapStyle::Syscall,
        ] {
            assert_eq!(TrapStyle::from_raw(style.raw()), Some(style));
        }
    }

    #[test]
    fn test_trap_style_from_raw_rejects_unknown_and_unmodeled() {
        // 0 is not a C style; 4 (KTS_INT_UM) and 6 (KTS_SYSENTER) are
        // deliberately unmodeled — no 64-bit entry path produces them.
        for raw in [0, 4, 6, 8, -1, i32::MAX] {
            assert_eq!(TrapStyle::from_raw(raw), None, "raw={raw}");
        }
    }

    #[test]
    fn test_trap_style_return_sequence_classification() {
        // C arch_system.c:592-603 — int-class styles share the full-context
        // restore; the syscall style pairs the fast path; none dispatches.
        assert_eq!(TrapStyle::IntHard.return_sequence(), Some(ReturnSequence::FullContext));
        assert_eq!(TrapStyle::IntOrig.return_sequence(), Some(ReturnSequence::FullContext));
        assert_eq!(TrapStyle::FullContext.return_sequence(), Some(ReturnSequence::FullContext));
        assert_eq!(TrapStyle::Syscall.return_sequence(), Some(ReturnSequence::FastSyscall));
        assert_eq!(TrapStyle::NoEntry.return_sequence(), None);
    }

    #[test]
    fn test_trap_style_default_is_no_entry() {
        assert_eq!(TrapStyle::default(), TrapStyle::NoEntry);
    }
}
