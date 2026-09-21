//! Initial-stack frame construction for execve clients.
//!
//! The doing half of `sh` starts other programs, which on this wire means
//! building the child's initial stack image and handing PM its five fields
//! (C: `execve` + `minix_stack_params`/`minix_stack_fill`,
//! `minix3/minix/lib/libc/sys/execve.c` + `stack_utils.c`). The layout and
//! every constant here follow the verified init precedent
//! (`os/commands/sbin/init/src/execve.rs`), lifted unchanged so the two
//! clients cannot drift; the eventual single home for this code is beside
//! the consumer half it feeds (`minix-rt/src/crt0.rs`, the birth chain that
//! reads the frame back).
//!
//! # Layout (C: `stack_utils.c:159-176`)
//!
//! One argc word, the argv pointer slots with a NULL terminator, the env
//! pointer slots with a NULL terminator, the strings, then the
//! `ps_strings` descriptor. Slot values are absolute addresses in the NEW
//! address space (`vsp` = stack top minus frame size). LP64 correction from
//! the init precedent: C seeds `ps_argvstr = *vsp + sizeof(argc)` with a
//! 4-byte `sizeof`, but the argc word is written through an 8-byte store —
//! so this code writes `ps_argvstr = vsp + 8`, one full pointer word.
//!
//! The C `STACK_MIN_SZ` rtld/auxv reservation is dropped exactly as init
//! documents it: these images are statically linked and the birth chain
//! consumes only the ps_strings descriptor, so the minimum frame is the
//! argc slot, the two NULL terminators, and the descriptor.

/// Minimum frame: argc slot (8) + argv/env NULL terminators (16) +
/// `struct ps_strings` (32). C: `stack_utils.c:56-63`, reduced as above.
pub const STACK_MIN_BYTES: usize = 8 + 2 * 8 + 32;

/// Size of one pointer slot / alignment of the frame (LP64).
const SLOT: usize = 8;

/// Sizes the initial stack for one exec (C: `minix_stack_params`,
/// `stack_utils.c:74-109`): the minimum frame plus, per argv/env string,
/// one pointer slot and the NUL-terminated bytes, rounded up to a slot.
/// Overflow anywhere answers E2BIG — the error C's execve hands back for a
/// frame that will not fit (`execve.c:27-31`).
pub fn stack_params(argv: &[&str], envp: &[&str]) -> Result<usize, i32> {
    const E2BIG: i32 = 7; // errno.h: E2BIG — matches minix-sys Errno::E2BIG
    let mut total = STACK_MIN_BYTES;
    let mut count = |items: &[&str]| -> Result<(), i32> {
        for item in items {
            let n = SLOT
                .checked_add(item.len())
                .and_then(|n| n.checked_add(1))
                .ok_or(E2BIG)?;
            total = total.checked_add(n).ok_or(E2BIG)?;
        }
        Ok(())
    };
    count(argv)?;
    count(envp)?;
    Ok(total.div_ceil(SLOT) * SLOT)
}

/// Fills the frame and returns the ps_strings offset within it (C:
/// `minix_stack_fill`, `stack_utils.c:115-176`). `frame` must be at least
/// [`stack_params`] bytes. Slot values are absolute addresses in the new
/// address space (`vsp` + offset), the arrays are NULL-terminated, and the
/// descriptor lands after the final alignment pad.
pub fn stack_fill(argv: &[&str], envp: &[&str], vsp: u64, frame: &mut [u8]) -> u64 {
    let (argc, envc) = (argv.len(), envp.len());
    let pointer_end = SLOT * (argc + envc + 3);
    let strings_off = pointer_end as u64;

    // argc word — one full pointer slot on LP64 (the init precedent's
    // correction to C's `sizeof(argc)`).
    frame[0..SLOT].copy_from_slice(&(argc as u64).to_ne_bytes());

    // argv slots + NULL, then env slots + NULL; each value is the new-space
    // address of its string (`stack_utils.c:160`), each array closed by its
    // NULL terminator even when empty (`stack_utils.c:165`/`170`).
    let mut string_cursor = strings_off;
    let mut slot = SLOT;
    for item in argv {
        frame[slot..slot + SLOT].copy_from_slice(&(vsp + string_cursor).to_ne_bytes());
        slot += SLOT;
        string_cursor += (item.len() + 1) as u64;
    }
    frame[slot..slot + SLOT].copy_from_slice(&0u64.to_ne_bytes());
    slot += SLOT;
    for item in envp {
        frame[slot..slot + SLOT].copy_from_slice(&(vsp + string_cursor).to_ne_bytes());
        slot += SLOT;
        string_cursor += (item.len() + 1) as u64;
    }
    frame[slot..slot + SLOT].copy_from_slice(&0u64.to_ne_bytes());
    debug_assert_eq!(slot + SLOT, pointer_end);

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

    // The ps_strings descriptor (C: `stack_utils.c:172-176`). `argv_str =
    // vsp + 8` is the LP64 correction; `env_str` is `argv_str + argc + 1`
    // slots. The field order mirrors the consumer contract in
    // `minix-rt/src/crt0.rs:49-54` (`ps_argvstr`@0, `ps_nargvstr`@8,
    // `ps_envstr`@16, `ps_nenvstr`@24, padding to 32).
    frame[tail..tail + 8].copy_from_slice(&(vsp + SLOT as u64).to_ne_bytes());
    frame[tail + 8..tail + 12].copy_from_slice(&(argc as i32).to_ne_bytes());
    frame[tail + 12..tail + 16].copy_from_slice(&[0; 4]);
    frame[tail + 16..tail + 24].copy_from_slice(&(vsp + (SLOT * (argc + 1)) as u64).to_ne_bytes());
    frame[tail + 24..tail + 28].copy_from_slice(&(envc as i32).to_ne_bytes());
    frame[tail + 28..tail + 32].copy_from_slice(&[0; 4]);
    tail as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Size formula against the init precedent's worked example (C:
    /// `minix_stack_params`, `stack_utils.c:74-109`, reduced STACK_MIN):
    /// argv `["sh", "/etc/rc"]` and envp `["PATH=/sbin"]` carry 3+8+11 = 22
    /// string bytes and 3 pointer slots over the 56-byte minimum → 102 →
    /// slot-aligned 104.
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
    /// vsp = 0x10000, argv `["sh"]`, envp empty → `[0]=argc=1`,
    /// `[8]=vsp+32` (argv[0]), `[16]=NULL` (argv end), `[24]=NULL` (empty
    /// env end), strings "sh\0" at 32, pad to 40, ps_strings at 40:
    /// argv_str=vsp+8, n_argv=1, env_str=vsp+16, n_env=0.
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
        assert_eq!(
            u64::from_ne_bytes(frame[40..48].try_into().unwrap()),
            vsp + 8
        );
        assert_eq!(i32::from_ne_bytes(frame[48..52].try_into().unwrap()), 1);
        assert_eq!(
            u64::from_ne_bytes(frame[56..64].try_into().unwrap()),
            vsp + 16
        );
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
        assert_eq!(
            u64::from_ne_bytes(frame[ps..ps + 8].try_into().unwrap()),
            vsp + 8
        );
        assert_eq!(
            u64::from_ne_bytes(frame[ps + 16..ps + 24].try_into().unwrap()),
            vsp + 8 * 3
        );
        assert_eq!(&frame[strings..strings + 3], b"sh\0");
        assert_eq!(&frame[strings + 11..strings + 22], b"PATH=/sbin\0");
    }
}
