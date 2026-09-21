//! Initial-stack frame construction — the `minix_stack_params` /
//! `minix_stack_fill` pair.
//!
//! C ground truth: `minix3/minix/lib/libc/sys/stack_utils.c:83-176`. The
//! VM server builds this frame for every boot process (`exec_bootproc`,
//! `minix3/minix/servers/vm/main.c:391-403`); the frame is copied to the
//! new address space's stack top and the process starts with its stack
//! pointer at `vsp`.
//!
//! # Frame layout (what `_start` expects)
//!
//! ```text
//! frame + 0            argc (one pointer-sized word, C: *fpw++ = argc)
//! frame + WORD         argv[0] .. argv[argc-1], then NULL
//!                      envp[0] .. envp[envc-1], then NULL
//! frame + strings_off  the strings themselves, NUL-terminated, packed
//! ...pad to WORD...    alignment padding
//! frame + ps_off       PsStrings { argv, argc, envp, envc }
//! ```
//!
//! Every slot holds an *absolute* address in the new address space
//! (`vsp + offset-in-frame`), so the frame works as-is once copied to
//! `vsp` — the same trick C uses (stack_utils.c:41-44).
//!
//! # LP64 adaptation
//!
//! C's budget constant `STACK_MIN_SZ` counts the argc slot as
//! `sizeof(int)` (4) while `minix_stack_fill` actually writes it as a
//! full pointer word (`*fpw++ = (char *)argc`). On i386 the two coincide;
//! on LP64 they differ by 4. The C library was never shipped on 64-bit,
//! so this rewrite resolves the ambiguity in favor of what `fill`
//! actually writes: the argc slot is one full word (8). Likewise
//! `ps_argvstr = vsp + sizeof(argc)` (stack_utils.c:169) becomes
//! `vsp + WORD` — the first argv slot sits one word after the argc word.
//!
//! The `user_sp` input is an explicit parameter here; C's `minix_stack_fill`
//! reads it from the kernel information page (`minix_get_user_sp`,
//! kernel_utils.c:40-52). The boot path has `KernelInfo.user_sp` at hand,
//! and the pure function stays testable without a mapped kernel page.

use minix_types::{PMEF_AUXVECTORS, PMEF_EXECNAMELEN1, PsStrings};

/// Pointer/word size in the frame — one slot per argv/envp entry.
const WORD: usize = core::mem::size_of::<usize>();

/// Size of the ELF auxiliary vector entry type (`AuxInfo`: two words).
const AUXINFO_SIZE: usize = 2 * WORD;

/// Minimum frame size: argc word, argv/envp NULL terminators, the
/// auxiliary-vector budget, the executable-name budget, and the
/// `PsStrings` block.
///
/// C: `STACK_MIN_SZ` — stack_utils.c:66-73, with the LP64 argc-slot
/// adaptation documented at the module level. On i386 the same formula
/// yields 1212; the LP64 value is 1400.
pub const STACK_MIN_SZ: usize = WORD
    + WORD * 2
    + AUXINFO_SIZE * PMEF_AUXVECTORS
    + PMEF_EXECNAMELEN1
    + core::mem::size_of::<PsStrings>();

/// Outcome of [`stack_params`] — the numbers C reports through
/// `size_t *stack_size, char *overflow, int *argc, int *envc`
/// (stack_utils.c:84).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackParams {
    /// Frame size in bytes, rounded up to a word.
    pub frame_size: usize,
    /// `argc` — number of argv strings.
    pub argc: usize,
    /// `envc` — number of envp strings.
    pub envc: usize,
}

/// Outcome of [`stack_fill`] — where the frame lands and where the
/// `PsStrings` block ended up, as absolute new-address-space values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillPlacement {
    /// Initial stack pointer: `user_sp - frame_size` (stack_utils.c:133).
    pub vsp: u64,
    /// Absolute address of the `PsStrings` block in the new address
    /// space — the value handed to `sys_exec` (main.c:409-411).
    pub ps_str: u64,
}

/// Why [`stack_fill`] refused to build a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackFillError {
    /// The strings or the ps_strings block would run past the frame end.
    /// C detects this family of failures in `minix_stack_params` via the
    /// overflow flag and the caller panics (main.c:395-397).
    FrameTooSmall,
    /// `frame_size` does not match the buffer length; the C pair
    /// implicitly assumes the caller keeps them consistent.
    SizeMismatch,
}

/// Compute the frame size and argument counts for an `(argv, envp)` pair
/// — C `minix_stack_params` (stack_utils.c:83-116).
///
/// The C signature also takes `path` but never reads it (the size loop
/// walks argv/envp only, stack_utils.c:91-108) — the program name is
/// counted through its argv[0] entry, which is the convention the boot
/// caller sets up (`main.c:347`, `argv = {ip->proc_name, NULL}`). The
/// dead parameter is dropped here rather than carried.
pub fn stack_params(argv: &[&str], envp: &[&str]) -> StackParams {
    // C: *stack_size = STACK_MIN_SZ, then += sizeof(ptr) + strlen+1 per
    // string (stack_utils.c:88-110).
    let mut size = STACK_MIN_SZ;
    for s in argv {
        size += WORD + s.len() + 1;
    }
    for s in envp {
        size += WORD + s.len() + 1;
    }
    // C: round up to pointer alignment (stack_utils.c:112-114).
    size = (size + WORD - 1) & !(WORD - 1);
    StackParams {
        frame_size: size,
        argc: argv.len(),
        envc: envp.len(),
    }
}

/// Fill `frame` with the initial stack image — C `minix_stack_fill`
/// (stack_utils.c:119-176).
///
/// `user_sp` plays the role of `minix_get_user_sp()`: the returned `vsp`
/// is `user_sp - frame_size`, and every pointer slot in the frame is
/// `vsp + offset` so the image is position-correct the moment it is
/// copied to `vsp` in the new address space.
pub fn stack_fill(
    argv: &[&str],
    envp: &[&str],
    frame_size: usize,
    user_sp: u64,
    frame: &mut [u8],
) -> Result<FillPlacement, StackFillError> {
    if frame.len() != frame_size {
        return Err(StackFillError::SizeMismatch);
    }
    if frame_size < STACK_MIN_SZ {
        return Err(StackFillError::FrameTooSmall);
    }
    let argc = argv.len();
    let envc = envp.len();

    let vsp = user_sp - frame_size as u64;

    // ── Pointer array (grows from frame+0) ──
    // C: *fpw++ = argc — the argc word, then argv/envp slots, then NULLs.
    let words_needed = 1 + argc + 1 + envc + 1;
    if words_needed * WORD > frame.len() {
        return Err(StackFillError::FrameTooSmall);
    }
    put_word(frame, 0, argc as u64);
    let mut w = 1;
    let mut string_off = min_strings_off(argc, envc);
    for s in argv {
        put_word(frame, w * WORD, vsp + string_off as u64);
        w += 1;
        write_string(frame, &mut string_off, s)?;
    }
    put_word(frame, w * WORD, 0); // argv NULL — C: *fpw++ = NULL
    w += 1;
    for s in envp {
        put_word(frame, w * WORD, vsp + string_off as u64);
        w += 1;
        write_string(frame, &mut string_off, s)?;
    }
    put_word(frame, w * WORD, 0); // envp NULL
    let _ = w;

    // ── PsStrings block ──
    // C: *psp = fp (after padding); ps_argvstr = vsp + sizeof(argc) →
    // LP64: vsp + WORD (the argv[0] slot sits one word past the argc
    // word); ps_envstr = ps_argvstr + argc + 1 (char** arithmetic —
    // (argc+1) slots of WORD bytes). The ps_strings block lands after
    // the strings, padded to a word (stack_utils.c:160-175).
    let ps_off = round_up_word(string_off);
    if ps_off + core::mem::size_of::<PsStrings>() > frame.len() {
        return Err(StackFillError::FrameTooSmall);
    }
    let ps = PsStrings {
        ps_argvstr: vsp + WORD as u64,
        ps_nargvstr: argc as i32,
        ps_envstr: vsp + WORD as u64 + ((argc + 1) * WORD) as u64,
        ps_nenvstr: envc as i32,
    };
    write_ps_strings(frame, ps_off, &ps);

    Ok(FillPlacement {
        vsp,
        ps_str: vsp + ps_off as u64,
    })
}

/// Offset where the strings area starts: the aux/execname budget sits
/// between the pointer array and the strings.
///
/// C: `frame + (min_size - sizeof(struct ps_strings)) +
/// (envc + argc) * sizeof(char *)` — stack_utils.c:140-141. Note the
/// pointer slots the fill loop actually writes (argc word + argc + envc
/// slots + two NULLs) extend into the aux/execname reserve; the formula
/// is reproduced as-is because the reserve is budget, not layout.
fn min_strings_off(argc: usize, envc: usize) -> usize {
    STACK_MIN_SZ - core::mem::size_of::<PsStrings>() + (envc + argc) * WORD
}

fn round_up_word(v: usize) -> usize {
    (v + WORD - 1) & !(WORD - 1)
}

/// Store one little-endian word (C writes native-endian pointer words;
/// the supported targets are little-endian x86-64/aarch64/riscv64).
fn put_word(frame: &mut [u8], off: usize, v: u64) {
    frame[off..off + WORD].copy_from_slice(&v.to_ne_bytes());
}

/// Copy a string plus its NUL terminator into the strings area, advancing
/// the cursor. Fails when it would cross the frame end.
fn write_string(frame: &mut [u8], cursor: &mut usize, s: &str) -> Result<(), StackFillError> {
    let end = *cursor + s.len() + 1;
    if end > frame.len() {
        return Err(StackFillError::FrameTooSmall);
    }
    frame[*cursor..end - 1].copy_from_slice(s.as_bytes());
    frame[end - 1] = 0;
    *cursor = end;
    Ok(())
}

/// Serialize the ps_strings block at `off` (fields in exec.h:111-116
/// order; i32 counts at their natural 4-byte width).
/// Serialize the ps_strings block at `off` (fields in exec.h:111-116
/// order; i32 counts at their natural 4-byte width).
///
/// NK4-A C-3（2026-09-22）：布局对位 C `struct ps_strings` 的 LP64 自然
/// 对齐——ps_envstr@16（i32 计数后有 4 字节填充）、ps_nenvstr@24、
/// size 32（与 `size_of::<PsStrings>()` 一致）。旧实现按紧凑 24 字节写
/// （envstr@12/nenvstr@20），与 minix-rt `PsStringsRaw`（repr(C) 自然
/// 对齐读侧）错位——幸而帧缓冲清零使 rt 恰好读到空 envp，属于潜伏
/// C 保真缺陷（F13，stack_frame_tests 有布局 pin 测试）。
fn write_ps_strings(frame: &mut [u8], off: usize, ps: &PsStrings) {
    put_word(frame, off, ps.ps_argvstr);
    frame[off + WORD..off + WORD + 4].copy_from_slice(&ps.ps_nargvstr.to_ne_bytes());
    put_word(frame, off + WORD + 8, ps.ps_envstr);
    frame[off + 2 * WORD + 8..off + 2 * WORD + 12]
        .copy_from_slice(&ps.ps_nenvstr.to_ne_bytes());
}

#[cfg(test)]
mod stack_frame_tests {
    use super::*;
    use alloc::vec;

    /// C absolute value pin: the LP64 budget (i386 would be 1212 — see
    /// the STACK_MIN_SZ docs; com.h:356-357, syslimits.h:64).
    #[test]
    fn test_stack_min_sz_lp64() {
        assert_eq!(AUXINFO_SIZE, 16);
        assert_eq!(STACK_MIN_SZ, 8 + 16 + 320 + 1024 + 32);
        assert_eq!(STACK_MIN_SZ, 1400);
    }

    /// Boot-path shape: `argv = {"/sbin/rs", NULL}`, `envp = {NULL}`
    /// (main.c:347-348). Verifies vsp placement, argc word, the argv
    /// slot pointing at the string, the NULL terminators, and the
    /// ps_strings content — the whole `_start` contract.
    #[test]
    fn test_stack_fill_boot_path_bytes() {
        const USER_SP: u64 = 0x7fff_ffff_f000;
        let path = "/sbin/rs";
        let params = stack_params(&[path], &[]);
        assert_eq!(params.argc, 1);
        assert_eq!(params.envc, 0);

        let mut frame = vec![0u8; params.frame_size];
        let placement = stack_fill(&[path], &[], params.frame_size, USER_SP, &mut frame)
            .expect("boot frame fits");
        let vsp = placement.vsp;
        assert_eq!(vsp, USER_SP - params.frame_size as u64);

        // argc word @0 (C: *fpw++ = argc — a full pointer word).
        assert_eq!(get_word(&frame, 0), 1);
        // argv[0] slot @8 points at the string; string bytes follow.
        let argv0 = get_word(&frame, WORD);
        let string_off = min_strings_off(1, 0);
        assert_eq!(argv0, vsp + string_off as u64);
        assert_eq!(&frame[string_off..string_off + path.len()], path.as_bytes());
        assert_eq!(frame[string_off + path.len()], 0); // NUL
        // argv NULL @16; envp NULL @24 (envc = 0 → envp slot list empty).
        assert_eq!(get_word(&frame, 2 * WORD), 0);
        assert_eq!(get_word(&frame, 3 * WORD), 0);

        // ps_strings block content at ps_str. NK4-A C-3：布局对位 C LP64
        // 自然对齐（envstr@+16、nenvstr@+24——i32 计数后有填充）。
        let ps_off = placement.ps_str as usize - vsp as usize;
        assert_eq!(ps_off, round_up_word(string_off + path.len() + 1));
        assert_eq!(get_word(&frame, ps_off), vsp + WORD as u64); // ps_argvstr
        assert_eq!(read_i32(&frame, ps_off + WORD), 1); // ps_nargvstr
        assert_eq!(
            get_word(&frame, ps_off + WORD + 8),
            vsp + WORD as u64 + 2 * WORD as u64, // ps_envstr = argv0 + argc+1 slots
        );
        assert_eq!(read_i32(&frame, ps_off + 2 * WORD + 8), 0); // ps_nenvstr
        assert!(placement.ps_str < USER_SP); // below stack top
    }

    /// `ps_argvstr` points *at the argv pointer array*, not at the first
    /// string: C sets it to `vsp + sizeof(argc)` (stack_utils.c:169) —
    /// the argv[0] slot's address — while the slot's content is the
    /// string address. Same for `ps_envstr` versus the envp NULL slot.
    #[test]
    fn test_ps_strs_point_at_pointer_array() {
        const USER_SP: u64 = 0x0000_4000_0000;
        let name = "vm";
        let params = stack_params(&[name], &[]);
        let mut frame = vec![0u8; params.frame_size];
        let placement =
            stack_fill(&[name], &[], params.frame_size, USER_SP, &mut frame).unwrap();
        let ps_off = placement.ps_str as usize - placement.vsp as usize;
        let vsp = placement.vsp;
        // ps_argvstr = address of the argv[0] slot (frame+WORD).
        assert_eq!(get_word(&frame, ps_off), vsp + WORD as u64);
        // …and the slot AT that address holds the string address.
        let slot: usize = (get_word(&frame, ps_off) - vsp) as usize;
        assert_eq!(get_word(&frame, slot), get_word(&frame, WORD));
        // ps_envstr = argv[0] slot + (argc+1) words = envp NULL slot;
        // that slot exists in the frame and holds 0.
        // NK4-A C-3：ps_envstr 字段在 C LP64 自然布局下位于 ps块+16
        // （i32 计数后有填充）。
        let env_slot: usize =
            (get_word(&frame, ps_off + WORD + 8) - vsp) as usize;
        assert_eq!(env_slot, WORD + 2 * WORD); // argc=1 → NULL @ frame+24
        assert_eq!(get_word(&frame, env_slot), 0);
    }

    /// A frame smaller than the budget is refused (C: overflow flag →
    /// caller panic, main.c:395-397).
    #[test]
    fn test_stack_fill_rejects_undersized_frame() {
        let mut frame = vec![0u8; STACK_MIN_SZ - 1];
        assert_eq!(
            stack_fill(&["x"], &[], frame.len(), 0x1000, &mut frame),
            Err(StackFillError::FrameTooSmall)
        );
        let mut frame2 = vec![0u8; STACK_MIN_SZ];
        assert_eq!(
            stack_fill(&["x"], &[], frame2.len() - 1, 0x1000, &mut frame2),
            Err(StackFillError::SizeMismatch)
        );
    }

    /// Two arguments exercise multi-string packing: strings are adjacent
    /// (first string ends where the second begins).
    #[test]
    fn test_stack_fill_two_args_pack_adjacent() {
        const USER_SP: u64 = 0x7fff_0000_0000;
        let args = ["/sbin/pm", "-d"];
        let params = stack_params(&args, &[]);
        let mut frame = vec![0u8; params.frame_size];
        let placement =
            stack_fill(&args, &[], params.frame_size, USER_SP, &mut frame).unwrap();
        let off0 = min_strings_off(2, 0);
        assert_eq!(get_word(&frame, WORD), placement.vsp + off0 as u64);
        assert_eq!(get_word(&frame, 2 * WORD), placement.vsp + (off0 + 9) as u64);
        assert_eq!(&frame[off0..off0 + 8], b"/sbin/pm");
        assert_eq!(&frame[off0 + 9..off0 + 11], b"-d");
        assert_eq!(read_i32(&frame, ps_off_of(&placement) + WORD), 2);
    }

    fn ps_off_of(p: &FillPlacement) -> usize {
        p.ps_str as usize - p.vsp as usize
    }

    fn get_word(frame: &[u8], off: usize) -> u64 {
        u64::from_ne_bytes(frame[off..off + 8].try_into().unwrap())
    }

    fn read_i32(frame: &[u8], off: usize) -> i32 {
        i32::from_ne_bytes(frame[off..off + 4].try_into().unwrap())
    }
}
