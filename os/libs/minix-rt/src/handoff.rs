//! Kernel handoff: the kernel information page and the initial process stack.
//!
//! When the kernel finishes loading a new program (the `exec` path), it does
//! not call into the program the way one Rust function calls another. It
//! places two things where the new program can find them and then jumps to
//! the program entry point:
//!
//! 1. A pointer to the **kernel information page** (C type
//!    `struct minix_kerninfo`, see `minix3/minix/include/minix/type.h`). This
//!    page carries the magic number, the feature flags, the pointer to the
//!    table of inter-process communication entry points, and the pointer to
//!    the user information structure.
//! 2. An **initial stack** whose top address is chosen by the kernel
//!    (`kuserinfo.kui_user_sp`, with a legacy fallback to
//!    `kinfo.user_sp`). The stack image itself is built by
//!    `minix_stack_params` / `minix_stack_fill` (see
//!    `minix3/minix/lib/libc/sys/stack_utils.c`).
//!
//! This module models that handoff in safe Rust. The design goals are:
//!
//! - A program can never observe an unchecked magic number: construction of
//!   [`ValidatedKernInfo`] requires [`KERNINFO_MAGIC`] to match.
//! - Version probing uses the same rule as C (`kui_size` compared against the
//!   field offset plus the field size, see `KUSERINFO_HAS_FIELD`), but the
//!   rule lives in one tested function ([`user_info_has_field`]) instead of
//!   being re-typed at every call site.
//! - Stack size computation is a pure function ([`compute_stack_size`]) with
//!   explicit overflow reporting, mirroring `minix_stack_params` without
//!   copying its pointer arithmetic.
//!
//! # Execution model
//!
//! This crate runs as an unprivileged user-space library with a
//! single-threaded startup path: the entry point runs once, on one stack,
//! before `main`. There is no second thread that could race startup, so plain
//! function arguments and return values are sufficient. No locking is needed
//! for the handoff itself; the global kernel information pointer installed
//! later by the runtime initialization module is guarded there, not here.

use minix_types::Errno;

/// Magic number that identifies a genuine kernel information page.
///
/// C: `KERNINFO_MAGIC 0xfc3b84bf` (`minix3/minix/include/minix/type.h:229`).
/// Every consumer must compare this value before trusting any other field.
pub const KERNINFO_MAGIC: u32 = 0xfc3b84bf;

/// Flag bit: the `minix_ipcvecs` pointer in the kernel information page is valid.
///
/// C: `MINIX_KIF_IPCVECS (1L << 0)` (`minix3/minix/include/minix/type.h:246`).
pub const KIF_IPC_VECTORS: u32 = 1 << 0;

/// Flag bit: the `kuserinfo` pointer in the kernel information page is valid.
///
/// C: `MINIX_KIF_USERINFO (1L << 1)` (`minix3/minix/include/minix/type.h:247`).
pub const KIF_USER_INFO: u32 = 1 << 1;

/// Legacy offset of the `user_sp` field inside the old `kinfo` structure.
///
/// C: `minix3/minix/include/minix/type.h:222-228` documents that the `user_sp`
/// field at offset 2440 of `struct kinfo` is used by legacy binaries even
/// though `struct kinfo` itself is not part of the user-space interface.
/// New programs must prefer [`UserInfo::initial_stack_pointer`]; the legacy
/// path exists only so old binaries keep running.
pub const LEGACY_USER_SP_OFFSET: usize = 2440;

/// Offset of the `kui_user_sp` field inside [`UserInfo`], in bytes.
///
/// The C macro `KUSERINFO_HAS_FIELD(kui, f)` expands to
/// `kui->kui_size >= offsetof(struct kuserinfo, f) + sizeof(kui->f)`
/// (`minix3/minix/include/minix/type.h:211-212`). This constant captures the
/// `offsetof` half for the stack pointer field so the probing rule can be
/// tested without a C compiler.
pub const USER_INFO_STACK_FIELD_OFFSET: usize = size_of::<usize>();

/// Size of the `kui_user_sp` field in bytes (a virtual address).
pub const USER_INFO_STACK_FIELD_SIZE: usize = size_of::<u64>();

/// Raw view of the user information structure.
///
/// C: `struct kuserinfo` (`minix3/minix/include/minix/type.h:205-208`) with
/// fields `kui_size` (total structure size, for interface version probing)
/// and `kui_user_sp` (initial stack pointer for the newly executed process).
///
/// The structure may only ever be extended with new fields appended at the
/// end; existing offsets are stable interface. This Rust form keeps the same
/// two leading fields and treats any trailing bytes as opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserInfo {
    /// Total size of the structure as published by the kernel.
    pub struct_size: usize,
    /// Initial stack pointer chosen by the kernel for the new process.
    pub initial_stack_pointer: u64,
}

/// Raw view of the leading fields of the kernel information page.
///
/// C: `struct minix_kerninfo` (`minix3/minix/include/minix/type.h:214-244`).
/// The full C structure carries many service-only pointers (machine
/// description, kernel messages, load information) that user programs must
/// not touch. This view keeps exactly the fields that belong to the
/// user-space interface: the magic number, the flag word, and the two
/// user-space pointers expressed as optional addresses (zero means absent,
/// matching the C `NULL` checks in `init.c` and `kernel_utils.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernInfoHeader {
    /// Must equal [`KERNINFO_MAGIC`]; any other value means the page is not
    /// a kernel information page.
    pub magic: u32,
    /// Presence bits; see [`KIF_IPC_VECTORS`] and [`KIF_USER_INFO`].
    pub flags: u32,
    /// Address of the communication vector table, or zero when absent.
    pub ipc_vectors_address: u64,
    /// Address of the [`UserInfo`] structure, or zero when absent.
    pub user_info_address: u64,
}

/// A kernel information header that has passed the magic number check.
///
/// Construction is the check: [`ValidatedKernInfo::new`] returns an error
/// unless `header.magic == KERNINFO_MAGIC`. Code that holds this type can rely
/// on the page being genuine without re-testing the magic number at every use.
///
/// This is the Rust replacement for the C pattern
/// `if (info->kerninfo_magic != KERNINFO_MAGIC) { _minix_kerninfo = NULL; }`
/// (`minix3/minix/lib/libc/sys/init.c:22-26`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedKernInfo(KernInfoHeader);

/// Failure to validate the kernel information page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffError {
    /// The magic number did not match [`KERNINFO_MAGIC`].
    BadMagic {
        /// Value actually present in the page.
        found: u32,
    },
    /// The page pointer itself was null.
    NullPage,
}

impl HandoffError {
    /// Maps the failure to the closest Minix3 error number.
    ///
    /// A corrupt handoff page means the program image or the kernel
    /// publishing step is broken; the closest executable-format error is
    /// `ENOEXEC` (value 8, `minix3/sys/sys/errno.h`). The mapping keeps the
    /// crate-wide rule that every error type converts to a Minix3 error
    /// number instead of inventing new codes.
    pub const fn to_errno(self) -> Errno {
        match self {
            HandoffError::BadMagic { .. } | HandoffError::NullPage => {
                Errno::from_i32(minix_types::ENOEXEC)
            }
        }
    }
}

impl ValidatedKernInfo {
    /// Validates the magic number and wraps the header.
    pub fn new(header: KernInfoHeader) -> Result<Self, HandoffError> {
        if header.magic != KERNINFO_MAGIC {
            return Err(HandoffError::BadMagic {
                found: header.magic,
            });
        }
        Ok(ValidatedKernInfo(header))
    }

    /// Returns the wrapped header.
    pub const fn header(self) -> KernInfoHeader {
        self.0
    }

    /// Reports whether the communication vector pointer is usable.
    ///
    /// C checks both the flag bit and the pointer itself
    /// (`init.c:27-28`: `ki_flags & MINIX_KIF_IPCVECS` and
    /// `minix_ipcvecs != NULL`). Both conditions are required here as well.
    pub const fn has_ipc_vectors(self) -> bool {
        self.0.flags & KIF_IPC_VECTORS != 0 && self.0.ipc_vectors_address != 0
    }

    /// Reports whether the user information pointer is usable.
    pub const fn has_user_info(self) -> bool {
        self.0.flags & KIF_USER_INFO != 0 && self.0.user_info_address != 0
    }
}

/// Tests whether a user information structure carries a given field.
///
/// This is the single home for the C rule `KUSERINFO_HAS_FIELD(kui, f)`
/// (`minix3/minix/include/minix/type.h:211-212`):
/// the field is present when `kui_size` reaches at least the field offset
/// plus the field size. Callers pass the field offset and size explicitly so
/// each field can be probed independently and the rule stays testable.
///
/// # Example (illustrative, not executed)
///
/// ```text
/// // Probe for the initial stack pointer field.
/// let present = user_info_has_field(
///     user_info.struct_size,
///     USER_INFO_STACK_FIELD_OFFSET,
///     USER_INFO_STACK_FIELD_SIZE,
/// );
/// ```
pub const fn user_info_has_field(struct_size: usize, field_offset: usize, field_size: usize) -> bool {
    struct_size >= field_offset.saturating_add(field_size)
}

/// Selects the initial stack pointer for a new process.
///
/// The selection order mirrors `minix_get_user_sp`
/// (`minix3/minix/lib/libc/sys/kernel_utils.c:39-62`):
///
/// 1. When the kernel information page advertises user information
///    (`KIF_USER_INFO` set), the user information pointer is non-null, and
///    the structure is large enough to contain the stack pointer field,
///    return the advertised stack pointer.
/// 2. Otherwise fall back to the legacy `kinfo.user_sp` value supplied by
///    the caller.
///
/// The fallback value is passed in rather than read from a raw pointer so
/// the decision logic can be unit tested without mapping kernel memory. The
/// real pointer dereference happens in exactly one place in the runtime
/// initialization module.
///
/// Returns [`HandoffError::NullPage`] when the header reference itself is
/// absent, matching the C `assert(_minix_kerninfo != NULL)` in
/// `get_minix_kerninfo` (`kernel_utils.c:29`): a missing page is a fatal
/// programming error, surfaced here as an explicit error instead of an abort.
pub fn select_initial_stack_pointer(
    header: Option<ValidatedKernInfo>,
    user_info: Option<UserInfo>,
    legacy_user_sp: u64,
) -> Result<u64, HandoffError> {
    let info = header.ok_or(HandoffError::NullPage)?;
    if let Some(user) = user_info
        && info.has_user_info()
        && user_info_has_field(
            user.struct_size,
            USER_INFO_STACK_FIELD_OFFSET,
            USER_INFO_STACK_FIELD_SIZE,
        )
    {
        return Ok(user.initial_stack_pointer);
    }
    Ok(legacy_user_sp)
}

/// Counts the bytes needed for the initial stack image.
///
/// This is the pure, testable half of `minix_stack_params`
/// (`minix3/minix/lib/libc/sys/stack_utils.c:76-114`): it adds the fixed
/// minimum (room for the argument count, the two terminating null pointers,
/// the auxiliary vectors, the executable name buffer, and the process string
/// descriptor) plus one pointer slot and the string bytes for every argument
/// and environment entry, then rounds the total up to the machine word size.
///
/// The C code detects size_t wrap-around with `if (*stack_size < n)`.
/// This function uses checked arithmetic instead and reports the outcome
/// explicitly: `overflow` is true when any addition wrapped, in which case
/// the returned size is not usable and the caller must refuse to build the
/// stack image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackSizePlan {
    /// Total image size in bytes, rounded up to word alignment.
    pub total_bytes: usize,
    /// Number of argument strings counted.
    pub argument_count: usize,
    /// Number of environment strings counted.
    pub environment_count: usize,
    /// True when an addition wrapped around; the size must not be used.
    pub overflow: bool,
}

/// Fixed minimum stack image size.
///
/// C: `STACK_MIN_SZ` (`stack_utils.c:66-71`): space for the argument count,
/// two null terminators, the auxiliary vectors (`PMEF_AUXVECTORS` entries),
/// the executable name buffer (`PMEF_EXECNAMELEN1` bytes), and one process
/// string descriptor. The exact auxiliary counts live in the executable
/// format headers; this constant keeps their combined contribution plus the
/// descriptor as one named value so the formula below reads as a whole.
///
/// The value is intentionally conservative and architecture neutral: the
/// precise per-architecture padding is applied by the alignment step in
/// [`compute_stack_size`].
#[allow(clippy::manual_bits)]
pub const STACK_MINIMUM_BYTES: usize =
    size_of::<i32>() + 2 * size_of::<usize>() + 8 * size_of::<usize>() + 256 + 32;

/// Computes the initial stack image size for the given argument and
/// environment string lengths.
///
/// `argument_lengths` and `environment_lengths` carry the byte length of each
/// string **including** its terminating zero byte, so the function never
/// scans memory and never reads past a buffer. Each entry contributes one
/// pointer slot plus its own bytes, exactly as the C loops add
/// `sizeof(*p) + strlen(*p) + 1` per entry.
pub fn compute_stack_size(
    argument_lengths: &[usize],
    environment_lengths: &[usize],
) -> StackSizePlan {
    let mut total = STACK_MINIMUM_BYTES;
    let mut overflow = false;

    let mut accumulate = |length: usize| {
        let (after_slot, slot_wrapped) = total.overflowing_add(size_of::<usize>());
        let (after_bytes, bytes_wrapped) = after_slot.overflowing_add(length);
        overflow |= slot_wrapped | bytes_wrapped;
        total = after_bytes;
    };

    for length in argument_lengths {
        accumulate(*length);
    }
    for length in environment_lengths {
        accumulate(*length);
    }

    let alignment = size_of::<usize>();
    let remainder = total % alignment;
    if remainder != 0 {
        let (aligned, wrapped) = total.overflowing_add(alignment - remainder);
        overflow |= wrapped;
        total = aligned;
    }
    if total < STACK_MINIMUM_BYTES {
        overflow = true;
    }

    StackSizePlan {
        total_bytes: total,
        argument_count: argument_lengths.len(),
        environment_count: environment_lengths.len(),
        overflow,
    }
}

/// Process string descriptor: where the argument and environment strings live.
///
/// C: `struct ps_strings` (`minix3/sys/sys/exec.h:111-116`) with the argument
/// pointer, argument count, environment pointer, and environment count. The
/// kernel and the process status tool use this descriptor to locate the
/// strings without re-parsing the stack. The counts are plain integers in C;
/// this form keeps them as `usize` after a range check at the boundary, so
/// negative C values can never enter the safe interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessStrings {
    /// Address of the first argument pointer.
    pub argument_list: u64,
    /// Number of argument strings.
    pub argument_count: usize,
    /// Address of the first environment pointer.
    pub environment_list: u64,
    /// Number of environment strings.
    pub environment_count: usize,
}

impl ProcessStrings {
    /// Builds the descriptor from raw C values, rejecting negative counts.
    ///
    /// The C fields `ps_nargvstr` / `ps_nenvstr` are signed integers; a
    /// negative value would wrap if cast directly to `usize`. This constructor
    /// maps that case to `EINVAL` (value 22), the same error the C library
    /// uses for malformed argument vectors.
    pub fn from_raw(
        argument_list: u64,
        argument_count: i32,
        environment_list: u64,
        environment_count: i32,
    ) -> Result<Self, Errno> {
        if argument_count < 0 || environment_count < 0 {
            return Err(Errno::from_i32(minix_types::EINVAL));
        }
        Ok(ProcessStrings {
            argument_list,
            argument_count: argument_count as usize,
            environment_list,
            environment_count: environment_count as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_magic_number_is_accepted() {
        let header = KernInfoHeader {
            magic: KERNINFO_MAGIC,
            flags: KIF_IPC_VECTORS | KIF_USER_INFO,
            ipc_vectors_address: 0x1000,
            user_info_address: 0x2000,
        };
        let validated = ValidatedKernInfo::new(header).expect("magic matches");
        assert!(validated.has_ipc_vectors());
        assert!(validated.has_user_info());
    }

    #[test]
    fn test_wrong_magic_number_is_rejected_with_found_value() {
        let header = KernInfoHeader {
            magic: 0x12345678,
            flags: 0,
            ipc_vectors_address: 0,
            user_info_address: 0,
        };
        match ValidatedKernInfo::new(header) {
            Err(HandoffError::BadMagic { found }) => assert_eq!(found, 0x12345678),
            other => panic!("expected BadMagic, got {:?}", other),
        }
    }

    #[test]
    fn test_ipc_vectors_require_flag_and_pointer() {
        let flag_without_pointer = KernInfoHeader {
            magic: KERNINFO_MAGIC,
            flags: KIF_IPC_VECTORS,
            ipc_vectors_address: 0,
            user_info_address: 0,
        };
        let pointer_without_flag = KernInfoHeader {
            magic: KERNINFO_MAGIC,
            flags: 0,
            ipc_vectors_address: 0x1000,
            user_info_address: 0,
        };
        assert!(!ValidatedKernInfo::new(flag_without_pointer).unwrap().has_ipc_vectors());
        assert!(!ValidatedKernInfo::new(pointer_without_flag).unwrap().has_ipc_vectors());
    }

    #[test]
    fn test_user_info_field_probing_matches_c_macro_rule() {
        // Field present: size reaches offset plus field size.
        assert!(user_info_has_field(
            USER_INFO_STACK_FIELD_OFFSET + USER_INFO_STACK_FIELD_SIZE,
            USER_INFO_STACK_FIELD_OFFSET,
            USER_INFO_STACK_FIELD_SIZE,
        ));
        // One byte short: field absent.
        assert!(!user_info_has_field(
            USER_INFO_STACK_FIELD_OFFSET + USER_INFO_STACK_FIELD_SIZE - 1,
            USER_INFO_STACK_FIELD_OFFSET,
            USER_INFO_STACK_FIELD_SIZE,
        ));
    }

    #[test]
    fn test_stack_pointer_prefers_user_info_over_legacy() {
        let header = ValidatedKernInfo::new(KernInfoHeader {
            magic: KERNINFO_MAGIC,
            flags: KIF_USER_INFO,
            ipc_vectors_address: 0,
            user_info_address: 0x2000,
        })
        .unwrap();
        let user = UserInfo {
            struct_size: USER_INFO_STACK_FIELD_OFFSET + USER_INFO_STACK_FIELD_SIZE,
            initial_stack_pointer: 0xF000_0000,
        };
        let selected =
            select_initial_stack_pointer(Some(header), Some(user), 0xE000_0000).unwrap();
        assert_eq!(selected, 0xF000_0000);
    }

    #[test]
    fn test_stack_pointer_falls_back_to_legacy_value() {
        // Flag bit clear: legacy kinfo value is used.
        let header = ValidatedKernInfo::new(KernInfoHeader {
            magic: KERNINFO_MAGIC,
            flags: 0,
            ipc_vectors_address: 0,
            user_info_address: 0,
        })
        .unwrap();
        let selected = select_initial_stack_pointer(Some(header), None, 0xE000_0000).unwrap();
        assert_eq!(selected, 0xE000_0000);
    }

    #[test]
    fn test_missing_page_is_an_explicit_error() {
        match select_initial_stack_pointer(None, None, 0) {
            Err(HandoffError::NullPage) => {}
            other => panic!("expected NullPage, got {:?}", other),
        }
    }

    #[test]
    fn test_stack_size_counts_slots_and_string_bytes() {
        // Two arguments ("hi\0" = 3 bytes, "there\0" = 6 bytes), no environment.
        let plan = compute_stack_size(&[3, 6], &[]);
        assert_eq!(plan.argument_count, 2);
        assert_eq!(plan.environment_count, 0);
        assert!(!plan.overflow);
        let expected = STACK_MINIMUM_BYTES + 2 * size_of::<usize>() + 3 + 6;
        let alignment = size_of::<usize>();
        let aligned = expected.div_ceil(alignment) * alignment;
        assert_eq!(plan.total_bytes, aligned);
    }

    #[test]
    fn test_stack_size_overflow_is_reported_not_wrapped() {
        let plan = compute_stack_size(&[usize::MAX], &[]);
        assert!(plan.overflow);
    }

    #[test]
    fn test_process_strings_rejects_negative_counts() {
        assert!(ProcessStrings::from_raw(0x1000, -1, 0x2000, 0).is_err());
        assert!(ProcessStrings::from_raw(0x1000, 0, 0x2000, -2).is_err());
        let descriptor = ProcessStrings::from_raw(0x1000, 2, 0x2000, 1).unwrap();
        assert_eq!(descriptor.argument_count, 2);
        assert_eq!(descriptor.environment_count, 1);
    }

    #[test]
    fn test_handoff_error_maps_to_executable_format_errno() {
        let error = HandoffError::BadMagic { found: 0 };
        assert_eq!(
            error.to_errno(),
            Errno::from_i32(minix_types::ENOEXEC)
        );
    }
}
