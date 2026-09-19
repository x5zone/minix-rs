//! `ps_strings` — the process argument-vector summary block a process's
//! initial stack carries.
//!
//! C ground truth: `minix3/sys/sys/exec.h:111-116`. The kernel reports the
//! block's user-space address to the freshly exec'd process (C i386 passes
//! it through `sys_exec`'s fifth argument; this rewrite's x86-64 kernel
//! parks it in the saved RBX via the exec context — see `boot.rs`), and
//! userland's `ps`-family tooling reads argv/envp locations from it.
//!
//! It sits in the initial stack frame that the VM server builds for boot
//! processes (C `exec_bootproc` → `minix_stack_fill`,
//! `minix3/minix/servers/vm/main.c:391-403`) — the byte layout around it is
//! defined by `minix_sys::stack` (the `stack_utils.c` analog).

/// Argument/environment summary written into every initial stack frame.
///
/// C: `struct ps_strings` — sys/sys/exec.h:111-116. i386 size 16 (four
/// 4-byte words); LP64 size 32 (two 8-byte pointers interleaved with two
/// 4-byte counts, rounded to pointer alignment).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PsStrings {
    /// Address of `argv[0]` in the process's address space.
    /// C: `char **ps_argvstr` — exec.h:112.
    pub ps_argvstr: u64,
    /// Number of argument strings. C: `int ps_nargvstr` — exec.h:113.
    pub ps_nargvstr: i32,
    /// Address of `envp[0]` in the process's address space.
    /// C: `char **ps_envstr` — exec.h:114.
    pub ps_envstr: u64,
    /// Number of environment strings. C: `int ps_nenvstr` — exec.h:115.
    pub ps_nenvstr: i32,
}

/// Exec frame size cap: the strings live in the exec frame, whose total
/// never exceeds this. C: `ARG_MAX 262144`（NetBSD limits.h；MIB 的
/// proc.c:978 注记 "current ARG_MAX value of 256K"）——`ps` 家族的大小
/// 估算与页行走预算都按它封顶（C-22）。
pub const ARG_MAX: u64 = 262_144;

/// Page size the MIB arg/env walk fetches and rounds by. C: `PAGE_SIZE
/// 4096`（i386/LP64 页宽；proc.c:981 的 roundup 与 :1042 的 trunc_page）。
pub const PAGE_SIZE: u64 = 4_096;

/// `sizeof(struct ps_strings)` — the MIB reads this many bytes from the
/// exec frame tail (proc.c:964).
pub const PS_STRINGS_SIZE: usize = core::mem::size_of::<PsStrings>();

#[cfg(test)]
mod ps_strings_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// Layout witness: 32 bytes on LP64 — `ps_argvstr`@0, `ps_nargvstr`@8,
    /// `ps_envstr`@16, `ps_nenvstr`@24 (exec.h:111-116, pointer 4→8).
    #[test]
    fn test_ps_strings_layout() {
        assert_eq!(size_of::<PsStrings>(), 32);
        assert_eq!(offset_of!(PsStrings, ps_argvstr), 0);
        assert_eq!(offset_of!(PsStrings, ps_nargvstr), 8);
        assert_eq!(offset_of!(PsStrings, ps_envstr), 16);
        assert_eq!(offset_of!(PsStrings, ps_nenvstr), 24);
        assert_eq!(PS_STRINGS_SIZE, 32);
    }

    /// 取表半的极限常量钉值（C-22）：ARG_MAX 封顶估算、PAGE_SIZE 定页。
    #[test]
    fn test_arg_max_and_page_size() {
        assert_eq!(ARG_MAX, 262_144);
        assert_eq!(PAGE_SIZE, 4_096);
    }
}
