//! The execve client face: initial-stack frame construction and PM_EXEC.
//!
//! C ground truth is the libc pair — `minix3/minix/lib/libc/sys/execve.c`
//! (overflow check, frame buffer, five-field message) and
//! `minix3/minix/lib/libc/sys/stack_utils.c` (`minix_stack_params` /
//! `minix_stack_fill`, the frame layout). The consumer side of the frame is
//! the birth chain in `os/libs/minix-rt/src/crt0.rs` (ps_strings descriptor
//! at RBX, NUL-terminated pointer arrays), and the shared truth for "where
//! the new stack lives" is the kernel information page
//! (`minix3/minix/lib/libc/sys/kernel_utils.c:40-49`, `minix_get_user_sp`).
//!
//! # The frame layout, and the LP64 correction
//!
//! C builds the frame as: one argc word, the argv pointer slots (with a
//! NULL terminator), the env pointer slots (with a NULL terminator), the
//! strings themselves, then the ps_strings descriptor
//! (`stack_utils.c:159-176`). Slot values are absolute addresses in the NEW
//! address space — `*vsp + (fp - frame)` (`stack_utils.c:160`) — where
//! `vsp = minix_get_user_sp() - stack_size` (`stack_utils.c:133`).
//!
//! One C detail does not survive the move to LP64: C seeds the argv array
//! address with `ps_argvstr = (char **)(*vsp + sizeof(argc))`
//! (`stack_utils.c:174`), where `argc` is an `int`. That is exact on i386
//! (4-byte slots: the argc word occupies frame+0, argv[0] at frame+4). On
//! LP64 the argc word is *written* through a `char **` (`stack_utils.c:162`,
//! an 8-byte store), so argv[0] lives at frame+8 while `sizeof(argc)` is
//! still 4 — the C line hands the birth chain an address 4 bytes into the
//! argc word. This module writes `ps_argvstr = vsp + 8`: the argc slot is
//! one pointer word wide, exactly where the 8-byte store put it.
//!
//! # What is deliberately absent
//!
//! C's `STACK_MIN_SZ` (`stack_utils.c:56-63`) reserves space for the ELF
//! auxiliary vector and the resolved executable name — room the dynamic
//! linker (`_rtld`) consumes. This rewrite is statically linked and the
//! birth chain consumes only the ps_strings descriptor
//! (`crt0.rs:8-16`: RBX carries ps_strings, RDX/RCX are always zero), so
//! the minimum frame is argc slot + two NULL slots + ps_strings = 56 bytes
//! and the auxv/execname reservation is dropped. Registering the difference
//! here is the honest form of "no rtld": a consumer that one day needs auxv
//! grows `STACK_MIN_BYTES` and the fill order.

use crate::session::ParsedCommand;
use alloc::string::String;
use alloc::vec::Vec;
use minix_sys::ipc::{DirectTrapTransport, IpcTransport as _};
use minix_sys::pm;
use minix_sys::Errno;

/// Minimum frame: argc slot (8) + argv/env NULL terminators (16) +
/// `struct ps_strings` (32). The reduced C `STACK_MIN_SZ`
/// (`stack_utils.c:56-63`); see the module docs for the dropped rtld room.
pub(crate) const STACK_MIN_BYTES: usize = 8 + 2 * 8 + 32;

/// Size of one pointer slot / alignment of the frame (LP64).
const SLOT: usize = 8;

/// `struct ps_strings` image written at the tail of the frame.
///
/// C: `sys/exec.h:111-116` via the consumer contract in
/// `minix-rt/src/crt0.rs:49-54` (`ps_argvstr`@0, `ps_nargvstr`@8,
/// `ps_envstr`@16, `ps_nenvstr`@24, tail padding to 32).
#[repr(C)]
struct PsStrings {
    argv_str: u64,
    n_argv: i32,
    _pad0: [u8; 4],
    env_str: u64,
    n_env: i32,
    _pad1: [u8; 4],
}

/// Writes one little-endian-native slot into the frame.
fn write_u64(frame: &mut [u8], at: usize, value: u64) {
    frame[at..at + SLOT].copy_from_slice(&value.to_ne_bytes());
}

/// Sizes the initial stack for this exec (C: `minix_stack_params`,
/// `stack_utils.c:74-109`).
///
/// The total is the minimum frame plus, for every argv and env string, one
/// pointer slot and the NUL-terminated bytes; the sum is rounded up to a
/// slot boundary (`stack_utils.c:104-107`). Overflow anywhere answers
/// E2BIG — the error C's execve hands back for a frame that will not fit
/// (`execve.c:27-31`), detected there by wrap-around and here by
/// `checked_add`.
pub(crate) fn stack_params(argv: &[&str], envp: &[&str]) -> Result<usize, Errno> {
    let mut total = STACK_MIN_BYTES;
    let mut count = |items: &[&str]| -> Result<usize, Errno> {
        let mut slots = 0usize;
        for item in items {
            let n = SLOT
                .checked_add(item.len())
                .and_then(|n| n.checked_add(1))
                .ok_or(Errno::E2BIG)?;
            total = total.checked_add(n).ok_or(Errno::E2BIG)?;
            slots += 1;
        }
        Ok(slots)
    };
    let _ = count(argv)?;
    let _ = count(envp)?;
    Ok(total.div_ceil(SLOT) * SLOT)
}

/// Fills the frame and returns the ps_strings offset within it (C:
/// `minix_stack_fill`, `stack_utils.c:115-176`).
///
/// `frame` must be at least `stack_params` bytes. Slot values are absolute
/// addresses in the new address space (`vsp` + offset), the arrays are
/// NULL-terminated, and the descriptor lands after a final alignment pad —
/// in this reduced layout exactly where the pointer area ends, because the
/// dropped auxv/execname room was the only slack (`stack_utils.c:143`
/// minus `STACK_MIN_BYTES`).
pub(crate) fn stack_fill(argv: &[&str], envp: &[&str], vsp: u64, frame: &mut [u8]) -> u64 {
    let (argc, envc) = (argv.len(), envp.len());
    let pointer_end = SLOT * (argc + envc + 3);
    let strings_off = pointer_end as u64;

    // argc word — one full pointer slot on LP64 (see the module docs on
    // C's `sizeof(argc)`).
    write_u64(frame, 0, argc as u64);

    // argv slots + NULL, then env slots + NULL; each value is the new-space
    // address of its string (C: `*fpw++ = (char *)(*vsp + (fp - frame))`,
    // `stack_utils.c:160`), each array closed by its NULL terminator
    // (`stack_utils.c:165`/`170`) — the terminator exists even when the
    // array is empty, which is why the two loops stay separate.
    let mut string_cursor = strings_off;
    let mut slot = SLOT;
    for item in argv {
        write_u64(frame, slot, vsp + string_cursor);
        slot += SLOT;
        string_cursor += (item.len() + 1) as u64;
    }
    write_u64(frame, slot, 0);
    slot += SLOT;
    for item in envp {
        write_u64(frame, slot, vsp + string_cursor);
        slot += SLOT;
        string_cursor += (item.len() + 1) as u64;
    }
    write_u64(frame, slot, 0);
    slot += SLOT;
    debug_assert_eq!(slot, pointer_end);

    // The strings themselves (C: `memcpy(fp, *p, n)`, `stack_utils.c:163`).
    let mut tail = pointer_end;
    for item in argv.iter().chain(envp.iter()) {
        frame[tail..tail + item.len()].copy_from_slice(item.as_bytes());
        frame[tail + item.len()] = 0;
        tail += item.len() + 1;
    }
    while !tail.is_multiple_of(SLOT) {
        frame[tail] = 0;
        tail += 1;
    }

    // The descriptor (C: `stack_utils.c:172-176`). `argv_str = vsp + 8`:
    // the LP64 correction — the argc slot is one pointer word. `env_str`
    // is `argv_str + (argc + 1)` slots, C's `ps_argvstr + argc + 1`.
    let ps = PsStrings {
        argv_str: vsp + SLOT as u64,
        n_argv: argc as i32,
        _pad0: [0; 4],
        env_str: vsp + (SLOT * (argc + 1)) as u64,
        n_env: envc as i32,
        _pad1: [0; 4],
    };
    // SAFETY: PsStrings is a repr(C) plain-old-data value; the byte copy is
    // its exact representation (same idiom as minix-sys `exec_via`'s
    // payload pack).
    let ps_bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const ps) as *const u8,
            core::mem::size_of::<PsStrings>(),
        )
    };
    frame[tail..tail + ps_bytes.len()].copy_from_slice(ps_bytes);
    tail as u64
}

/// The initial stack pointer the kernel will give the new image (C:
/// `minix_get_user_sp`, `kernel_utils.c:40-49` — read from the kernel
/// information page).
///
/// The page address comes from the kerninfo query through the direct trap
/// transport (hosted builds have no kernel behind the trap and get the
/// honest EIO fallback, E1 slice 5). The `kui_user_sp` field is only read
/// after the size check `kui_size >= offsetof + sizeof` — the C
/// `KUSERINFO_HAS_FIELD` rule (`minix3/minix/include/minix/type.h:210-211`).
pub(crate) fn new_image_stack_top() -> Result<u64, Errno> {
    let page = DirectTrapTransport
        .query_kerninfo_page()
        .map_err(|_| Errno::EIO)?;
    if page == 0 {
        return Err(Errno::EIO);
    }
    // SAFETY: the kernel published and user-mapped this page before handing
    // out its address (the boot handoff owns the mapping); user mode treats
    // it as read-only. Zero would have returned above, so the pointer here
    // is the kernel's.
    let info = unsafe { &*(page as *const minix_types::types::MinixKerninfo) };
    if info.kuserinfo == 0 {
        return Err(Errno::EIO);
    }
    // SAFETY: same kernel-published page family; `kuserinfo` points at the
    // leading `KuserInfo` (two u64 fields, layout witness in
    // minix-types/src/types/kerninfo.rs:199-205), and the size field gates
    // the field read below exactly as KUSERINFO_HAS_FIELD does in C.
    let user = unsafe { &*(info.kuserinfo as *const minix_types::types::KuserInfo) };
    const FIELD_OFFSET: u64 = 8; // offsetof(KuserInfo, kui_user_sp)
    const FIELD_SIZE: u64 = 8;
    if user.kui_size < FIELD_OFFSET + FIELD_SIZE {
        return Err(Errno::EIO);
    }
    Ok(user.kui_user_sp)
}

/// Composes the child's environment: the inherited (birth) strings with the
/// host's `setenv`-style overrides applied — an override replaces the first
/// matching `KEY=` entry in place or appends (C: `setenv(name, value, 1)`,
/// init.c:801 — init sets PATH once; `execv` then inherits `environ`).
pub(crate) fn compose_envp(inherited: &[String], overrides: &[(String, String)]) -> Vec<String> {
    let mut envp = inherited.to_vec();
    for (key, value) in overrides {
        let needle = format!("{key}=");
        let line = format!("{key}={value}");
        match envp.iter().position(|e| e.starts_with(&needle)) {
            Some(at) => envp[at] = line,
            None => envp.push(line),
        }
    }
    envp
}

/// Runs the whole exec: frame, descriptor, five-field PM_EXEC message.
///
/// C shape: `execve.c:33-58` — size the stack, take a buffer (there sbrk,
/// here the slab allocator via `Vec`; allocation failure is E2BIG the way
/// C answers sbrk failure), fill it, clear-and-fill the message, call.
/// A returned call is failure — a successful exec never comes back
/// (`execve.c:53-58`).
///
/// The envp half is `compose_envp` over the birth environment
/// (`minix_rt::crt0::envs()`) and the host's override list — the rewrite's
/// form of C's `environ`, which C hands to `execv` implicitly
/// (`execv(shell, argv)`, init.c:803).
pub(crate) fn exec_command(env_overrides: &[(String, String)], cmd: &ParsedCommand) -> Errno {
    let argv: Vec<&str> = cmd.argv.iter().map(String::as_str).collect();
    let inherited: Vec<String> = minix_rt::crt0::envs()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect();
    let envp_owned = compose_envp(&inherited, env_overrides);
    let envp: Vec<&str> = envp_owned.iter().map(String::as_str).collect();

    // The path travels as a C string: pointer plus length including the NUL
    // (C: `m.name = path; m.namelen = strlen(path) + 1`, execve.c:48-49).
    let mut path = Vec::with_capacity(cmd.exec_path.len() + 1);
    path.extend_from_slice(cmd.exec_path.as_bytes());
    path.push(0);

    let frame_size = match stack_params(&argv, &envp) {
        Ok(size) => size,
        Err(e) => return e,
    };
    let stack_top = match new_image_stack_top() {
        Ok(top) => top,
        Err(e) => return e,
    };
    let vsp = match stack_top.checked_sub(frame_size as u64) {
        Some(vsp) => vsp,
        None => return Errno::E2BIG,
    };

    let mut frame: Vec<u8> = Vec::new();
    if frame.try_reserve_exact(frame_size).is_err() {
        return Errno::E2BIG;
    }
    frame.resize(frame_size, 0);
    let ps_offset = stack_fill(&argv, &envp, vsp, &mut frame);

    let prepared = match pm::prepare_exec(
        path.as_ptr() as u64,
        path.len() as u64 as usize,
        frame.as_ptr() as u64,
        frame.len(),
        vsp + ps_offset,
    ) {
        Ok(prepared) => prepared,
        Err(e) => return e,
    };
    pm::exec_via(&DirectTrapTransport, prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Size formula against a worked example (C: `minix_stack_params`,
    /// `stack_utils.c:74-109`, reduced STACK_MIN): argv `["sh", "/etc/rc"]`
    /// and envp `["PATH=/sbin"]` carry 3+8+11 = 22 string bytes and 3
    /// pointer slots over the 56-byte minimum → 102 → slot-aligned 104.
    #[test]
    fn test_stack_params_worked_example() {
        let argv = ["sh", "/etc/rc"];
        let envp = ["PATH=/sbin"];
        assert_eq!(stack_params(&argv, &envp), Ok(104));
    }

    /// Empty vectors size to the bare minimum frame (already slot-aligned).
    #[test]
    fn test_stack_params_minimum_frame() {
        assert_eq!(stack_params(&[], &[]), Ok(STACK_MIN_BYTES));
        assert_eq!(STACK_MIN_BYTES % 8, 0);
    }

    /// The fill against a fully hand-laid frame (C: `minix_stack_fill`,
    /// `stack_utils.c:115-176`, with the LP64 ps_argvstr correction):
    /// vsp = 0x10000, argv `["sh"]`, envp empty →
    /// `[0]=argc=1`, `[8]=vsp+32` (argv[0]), `[16]=NULL` (argv end),
    /// `[24]=NULL` (empty env end), strings "sh\0" at 32, pad to 40,
    /// ps_strings at 40: argv_str=vsp+8, n_argv=1, env_str=vsp+16, n_env=0.
    #[test]
    fn test_stack_fill_layout() {
        let argv = ["sh"];
        let envp: [&str; 0] = [];
        let vsp: u64 = 0x1_0000;
        let size = stack_params(&argv, &envp).unwrap();
        let mut frame = vec![0xA5u8; size];
        let ps_offset = stack_fill(&argv, &envp, vsp, &mut frame);
        assert_eq!(ps_offset, 40);

        let slot = |i: usize| u64::from_ne_bytes(frame[i * 8..i * 8 + 8].try_into().unwrap());
        assert_eq!(slot(0), 1); // argc
        assert_eq!(slot(1), vsp + 32); // argv[0] → "sh"
        assert_eq!(slot(2), 0); // argv NULL
        assert_eq!(slot(3), 0); // env NULL (empty array keeps its terminator)
        assert_eq!(&frame[32..35], b"sh\0");
        // ps_strings at 40 (LP64: argv_str = vsp + 8 — the argc slot is a
        // full pointer word).
        assert_eq!(u64::from_ne_bytes(frame[40..48].try_into().unwrap()), vsp + 8);
        assert_eq!(i32::from_ne_bytes(frame[48..52].try_into().unwrap()), 1);
        assert_eq!(u64::from_ne_bytes(frame[56..64].try_into().unwrap()), vsp + 16);
        assert_eq!(i32::from_ne_bytes(frame[64..68].try_into().unwrap()), 0);
        assert!(frame[68..72].iter().all(|&b| b == 0)); // C tail padding
    }

    /// Two-string env: slots point at successive strings, descriptor's
    /// env_str sits after the argv NULL (`ps_argvstr + argc + 1`,
    /// `stack_utils.c:176`).
    #[test]
    fn test_stack_fill_env_slots_and_descriptor() {
        let argv = ["sh", "/etc/rc"];
        let envp = ["PATH=/sbin"];
        let vsp: u64 = 0x2_0000;
        let size = stack_params(&argv, &envp).unwrap();
        let mut frame = vec![0u8; size];
        let ps_offset = stack_fill(&argv, &envp, vsp, &mut frame);
        let slot = |i: usize| u64::from_ne_bytes(frame[i * 8..i * 8 + 8].try_into().unwrap());
        let strings = 8 * (2 + 1 + 3); // pointer area: argc + 2 argv + NULL + 1 env + NULL
        assert_eq!(slot(1), vsp + strings as u64); // "sh"
        assert_eq!(slot(2), vsp + (strings + 3) as u64); // "/etc/rc"
        assert_eq!(slot(4), vsp + (strings + 11) as u64); // "PATH=/sbin"
        let ps = ps_offset as usize;
        assert_eq!(u64::from_ne_bytes(frame[ps..ps + 8].try_into().unwrap()), vsp + 8);
        assert_eq!(u64::from_ne_bytes(frame[ps + 16..ps + 24].try_into().unwrap()), vsp + 8 * 3);
        assert_eq!(&frame[strings..strings + 3], b"sh\0");
        assert_eq!(&frame[strings + 11..strings + 22], b"PATH=/sbin\0");
    }

    /// The child environment: overrides replace a matching `KEY=` in place
    /// and append new keys — C `setenv(name, value, 1)` shape
    /// (init.c:801), keeping C's "replace, don't duplicate" rule.
    #[test]
    fn test_compose_envp_replaces_in_place_and_appends() {
        let inherited: Vec<String> = vec!["HOME=/".into(), "PATH=/bin".into()];
        let overrides = vec![
            ("PATH".to_string(), "/sbin".to_string()),
            ("TERM".to_string(), "console".to_string()),
        ];
        let composed = compose_envp(&inherited, &overrides);
        assert_eq!(
            composed,
            vec!["HOME=/".to_string(), "PATH=/sbin".to_string(), "TERM=console".to_string()]
        );
    }

    /// The wired exec on a machine without a kernel: the kerninfo query
    /// fails honest (EIO, E1 slice 5 fallback) — no fake success, no
    /// panic (the C call would go to a real PM; here the machine is
    /// absent, which is a state, not a crash).
    #[test]
    fn test_exec_command_hosted_is_honest_eio() {
        let cmd = ParsedCommand {
            exec_path: "/bin/sh".into(),
            argv: vec!["sh".into(), "/etc/rc".into()],
        };
        assert_eq!(exec_command(&[], &cmd), Errno::EIO);
    }
}
