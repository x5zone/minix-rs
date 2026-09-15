//! Kernel information ABI — the `MINIX_KERNINFO` shared page contract.
//!
//! C ground truth: `minix3/minix/include/minix/type.h:214-245` (struct
//! `minix_kerninfo` + `kuserinfo`) and `minix3/minix/include/minix/config.h:5-9`
//! (OS name / release / version strings). A user process obtains the
//! user-mapped address of a `MinixKerninfo` page by issuing the
//! `MINIX_KERNINFO` IPC call (= 6, `ipcconst.h:12`); the kernel returns the
//! address through the secondary IPC return channel (C i386: `p_reg.bx`,
//! `arch_system.c:184-186`).
//!
//! The struct comment in C is explicit that binaries depend on these
//! offsets — it is ABI-restricted and "may only ever be extended with new
//! fields". The Rust mirror therefore pins field order, sizes, and total
//! size with compile-visible layout tests instead of re-deriving offsets
//! at each use.
//!
//! LP64 adaptation: on i386 the six flag words plus eight pointers total
//! 56 bytes; with 8-byte pointers the natural size is 88 bytes (the six
//! `u32` flags happen to occupy exactly 24 bytes, which is 8-aligned, so
//! no new padding word is needed). The kernel for this rewrite is x86-64
//! only, so the 88-byte layout is the wire truth here.

/// Magic number stamped into a genuine kernel information page.
///
/// C: `KERNINFO_MAGIC` — type.h:229. libc's init validates this before
/// publishing the page pointer (`minix3/minix/lib/libc/sys/init.c:22-26`).
pub const KERNINFO_MAGIC: u32 = 0xfc3b_84bf;

/// `ki_flags` bit: the `minix_ipcvecs` pointer is valid.
///
/// C: `MINIX_KIF_IPCVECS` — type.h:244.
pub const MINIX_KIF_IPCVECS: u32 = 1 << 0;

/// `ki_flags` bit: the `kuserinfo` pointer is valid.
///
/// C: `MINIX_KIF_USERINFO` — type.h:245.
pub const MINIX_KIF_USERINFO: u32 = 1 << 1;

/// OS name. C: `OS_NAME` — config.h:5.
pub const OS_NAME: &str = "Minix";

/// OS release ("3.m.p"). C: `OS_RELEASE` — config.h:6.
pub const OS_RELEASE: &str = "3.4.0";

/// OS revision number (NetBSD `3mm00pp00` form). C: `OS_REV` — config.h:7.
pub const OS_REV: i64 = 304_000_000;

/// OS config flavor. C: `OS_CONFIG` — config.h:8.
pub const OS_CONFIG: &str = "GENERIC";

/// Composed version banner. C: `OS_VERSION` — config.h:9
/// (`OS_NAME " " OS_RELEASE " (" OS_CONFIG ")"`).
pub const OS_VERSION: &str = "Minix 3.4.0 (GENERIC)";

/// Size of the `kinfo.release` / `kinfo.version` character arrays.
///
/// C: `char release[6]` / `char version[6]` — param.h:42-43. Both are
/// filled with `strlcpy` (main.c:431-432), so `OS_RELEASE` fits exactly
/// (`"3.4.0\0"`) while `OS_VERSION` truncates to its first five bytes.
pub const KINFO_RELEASE_LEN: usize = 6;

/// `strlcpy`-semantics pack into a fixed `char[N]` buffer: copy up to
/// `N - 1` bytes, then NUL-terminate. Mirrors how the C kernel fills
/// `kinfo.release` / `kinfo.version` (main.c:431-432).
pub fn strlcpy_fixed<const N: usize>(src: &str) -> [u8; N] {
    let mut out = [0u8; N];
    let bytes = src.as_bytes();
    let copy_len = bytes.len().min(N - 1);
    out[..copy_len].copy_from_slice(&bytes[..copy_len]);
    out
}

/// Per-process user info the kernel publishes for exec'd processes.
///
/// C: `struct kuserinfo` — type.h:203-208. i386 size 16 = LP64 size 16
/// (two pointer/size_t words grow 4→8, count halves). The struct records
/// its own size so future extensions can be feature-tested:
/// `KUSERINFO_HAS_FIELD` (type.h:210-211) checks
/// `kui_size >= offsetof(field) + sizeof(field)`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KuserInfo {
    /// Size of this structure, for ABI testing. C: `size_t kui_size`.
    pub kui_size: u64,
    /// Initial stack pointer for an exec'd process. C: `vir_bytes kui_user_sp`.
    pub kui_user_sp: u64,
}

/// The user-visible kernel information page.
///
/// C: `struct minix_kerninfo` — type.h:214-243. Field order is ABI:
/// six `u32` words (magic, feature flags, presence flags, three unused)
/// followed by eight pointers to the named sub-structures.
///
/// Pointer targets marked "NOT userland ABI" in C (`kinfo`, `machine`,
/// `kmessages`, `loadinfo`, `arm_frclock`, `kclockinfo`) keep their slots
/// here so the wire shape matches, but this rewrite does not yet publish
/// those sub-structures — the kernel fills them with 0 (NULL) and the
/// corresponding `ki_flags` bits stay clear, which is exactly how C
/// treats not-yet-initialized content.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinixKerninfo {
    /// Must equal [`KERNINFO_MAGIC`]. C: `u32_t kerninfo_magic` — type.h:230.
    pub kerninfo_magic: u32,
    /// Features present in the kernel. C: `u32_t minix_feature_flags` — type.h:231.
    pub minix_feature_flags: u32,
    /// What is present in this struct (`MINIX_KIF_*`). C: `u32_t ki_flags` — type.h:232.
    pub ki_flags: u32,
    /// Reserved. C: `u32_t flags_unused2` — type.h:233.
    pub flags_unused2: u32,
    /// Reserved. C: `u32_t flags_unused3` — type.h:234.
    pub flags_unused3: u32,
    /// Reserved. C: `u32_t flags_unused4` — type.h:235.
    pub flags_unused4: u32,
    /// Kernel `kinfo` table. C: `struct kinfo *kinfo` — type.h:236
    /// (NOT userland ABI; `user_sp` legacy offset lives in [`KuserInfo`] now).
    pub kinfo: u64,
    /// Machine description. C: `struct machine *machine` — type.h:237 (NOT userland ABI).
    pub machine: u64,
    /// Kernel message ring. C: `struct kmessages *kmessages` — type.h:238 (NOT userland ABI).
    pub kmessages: u64,
    /// Load information. C: `struct loadinfo *loadinfo` — type.h:239 (NOT userland ABI).
    pub loadinfo: u64,
    /// IPC vector table. C: `struct minix_ipcvecs *minix_ipcvecs` — type.h:240 (userland ABI).
    pub minix_ipcvecs: u64,
    /// Per-process user info. C: `struct kuserinfo *kuserinfo` — type.h:241 (userland ABI).
    pub kuserinfo: u64,
    /// ARM free-running timer. C: `struct arm_frclock *arm_frclock` — type.h:242 (NOT userland ABI).
    pub arm_frclock: u64,
    /// Kernel clock info. C: `volatile struct kclockinfo *kclockinfo` — type.h:243 (NOT userland ABI).
    pub kclockinfo: u64,
}

impl MinixKerninfo {
    /// A zeroed page with only the magic number stamped.
    ///
    /// Mirrors the boot-time state before arch init fills the feature
    /// flags and sub-structure pointers: everything reads as absent, so a
    /// consumer that validates `kerninfo_magic` and then checks `ki_flags`
    /// sees no advertised capability.
    pub const fn new_uninitialized() -> Self {
        Self {
            kerninfo_magic: KERNINFO_MAGIC,
            minix_feature_flags: 0,
            ki_flags: 0,
            flags_unused2: 0,
            flags_unused3: 0,
            flags_unused4: 0,
            kinfo: 0,
            machine: 0,
            kmessages: 0,
            loadinfo: 0,
            minix_ipcvecs: 0,
            kuserinfo: 0,
            arm_frclock: 0,
            kclockinfo: 0,
        }
    }
}

#[cfg(test)]
mod kerninfo_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C absolute values pin: magic + presence flags.
    #[test]
    fn test_kerninfo_magic_and_flags_match_c() {
        assert_eq!(KERNINFO_MAGIC, 0xfc3b_84bf); // type.h:229
        assert_eq!(MINIX_KIF_IPCVECS, 1); // type.h:244
        assert_eq!(MINIX_KIF_USERINFO, 2); // type.h:245
        assert_eq!(OS_RELEASE, "3.4.0"); // config.h:6
        assert_eq!(OS_VERSION, "Minix 3.4.0 (GENERIC)"); // config.h:9 composed
    }

    /// Layout witness: `MinixKerninfo` 88 bytes on LP64 — six `u32` words
    /// (24 bytes, naturally 8-aligned) then eight 8-byte pointers
    /// (type.h:230-243).
    #[test]
    fn test_minix_kerninfo_layout() {
        assert_eq!(size_of::<MinixKerninfo>(), 88);
        assert_eq!(offset_of!(MinixKerninfo, kerninfo_magic), 0);
        assert_eq!(offset_of!(MinixKerninfo, minix_feature_flags), 4);
        assert_eq!(offset_of!(MinixKerninfo, ki_flags), 8);
        assert_eq!(offset_of!(MinixKerninfo, flags_unused2), 12);
        assert_eq!(offset_of!(MinixKerninfo, flags_unused3), 16);
        assert_eq!(offset_of!(MinixKerninfo, flags_unused4), 20);
        assert_eq!(offset_of!(MinixKerninfo, kinfo), 24);
        assert_eq!(offset_of!(MinixKerninfo, machine), 32);
        assert_eq!(offset_of!(MinixKerninfo, kmessages), 40);
        assert_eq!(offset_of!(MinixKerninfo, loadinfo), 48);
        assert_eq!(offset_of!(MinixKerninfo, minix_ipcvecs), 56);
        assert_eq!(offset_of!(MinixKerninfo, kuserinfo), 64);
        assert_eq!(offset_of!(MinixKerninfo, arm_frclock), 72);
        assert_eq!(offset_of!(MinixKerninfo, kclockinfo), 80);
    }

    /// Layout witness: `KuserInfo` 16 bytes (`kui_size`@0, `kui_user_sp`@8)
    /// — type.h:203-208.
    #[test]
    fn test_kuser_info_layout() {
        assert_eq!(size_of::<KuserInfo>(), 16);
        assert_eq!(offset_of!(KuserInfo, kui_size), 0);
        assert_eq!(offset_of!(KuserInfo, kui_user_sp), 8);
    }

    /// `strlcpy` pack semantics: `OS_RELEASE` fits `char[6]` exactly;
    /// `OS_VERSION` truncates to five bytes + NUL (main.c:431-432 fill).
    #[test]
    fn test_strlcpy_fixed_truncates_like_c() {
        let release: [u8; 6] = strlcpy_fixed(OS_RELEASE);
        assert_eq!(&release, b"3.4.0\0");
        let version: [u8; 6] = strlcpy_fixed(OS_VERSION);
        assert_eq!(&version, b"Minix\0");
    }
}
