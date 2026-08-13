//! Device I/O system calls: irqctl, devio, vdevio, iopenable.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_irqctl.c` — SYS_IRQCTL
//! - `do_devio.c` — SYS_DEVIO (x86-only)
//! - `do_vdevio.c` — SYS_VDEVIO (x86-only)
//! - `do_iopenable.c` — SYS_IOPENABLE (x86-only)
//!
//! # Design Decisions (20-syscall-device.md §3)
//!
//! - **D1**: `IrqctlRequest` enum for IRQ sub-requests
//! - **D2**: `trait PortIo` for architecture-specific I/O
//! - **D6**: x86-only calls return BadCall on other architectures

use minix_plat::{IrqPolicy, IrqVector, IrqNotifyId, NR_IRQ_VECTORS};
use minix_types::{Endpoint, Message, MessageM1};

use crate::irq_manager::IrqManager;
use crate::kpriv::{KPriv, PrivFlagsBits, PrivTable};
use crate::proc::KProcess;
use crate::syscall::{KcallResult, Syscall};
use minix_plat::InterruptController;

// ── Minix3 error codes ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
// Previously ENOSYS=38 here (should be 78).
use crate::errno::*;

// ── IRQ control requests ──

/// IRQ control request types. C: `IRQ_SETPOLICY` etc. — devio.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum IrqctlRequest {
    /// Register an interrupt hook. C: `IRQ_SETPOLICY`
    SetPolicy = 0,
    /// Remove an interrupt hook. C: `IRQ_RMPOLICY`
    RmPolicy = 1,
    /// Enable an IRQ. C: `IRQ_ENABLE`
    Enable = 2,
    /// Disable an IRQ. C: `IRQ_DISABLE`
    Disable = 3,
}

impl TryFrom<i32> for IrqctlRequest {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::SetPolicy),
            1 => Ok(Self::RmPolicy),
            2 => Ok(Self::Enable),
            3 => Ok(Self::Disable),
            _ => Err(()),
        }
    }
}

// ── IRQ policy flags ──

/// Re-enable IRQ after handling. C: `IRQ_REENABLE`
pub const IRQ_REENABLE: u32 = 0x01;

// ── I/O size types ──

/// I/O operation size. C: `_DIO_BYTE/_DIO_WORD/_DIO_LONG`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoSize {
    Byte = 1,
    Word = 2,
    Long = 4,
}

impl IoSize {
    /// Extract size from I/O request type mask.
    /// C: `_DIO_TYPEMASK` — com.h:287
    ///
    /// C values: `_DIO_BYTE=0x010, _DIO_WORD=0x020, _DIO_LONG=0x030`
    /// The mask extracts the type field; we match on the raw C constants.
    pub fn from_request_mask(mask: i32) -> Option<Self> {
        match mask {
            0x010 => Some(IoSize::Byte),  // _DIO_BYTE
            0x020 => Some(IoSize::Word),  // _DIO_WORD
            0x030 => Some(IoSize::Long),  // _DIO_LONG
            _ => None,
        }
    }
}

// ── I/O direction ──

/// I/O direction. C: `_DIO_INPUT/_DIO_OUTPUT`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoDirection {
    Input,
    Output,
}

impl IoDirection {
    /// Extract direction from I/O request direction mask.
    /// C: `_DIO_DIRMASK` — com.h:283
    ///
    /// C values: `_DIO_INPUT=0x001, _DIO_OUTPUT=0x002`
    pub fn from_request_mask(mask: i32) -> Option<Self> {
        match mask {
            0x001 => Some(IoDirection::Input),   // _DIO_INPUT
            0x002 => Some(IoDirection::Output),  // _DIO_OUTPUT
            _ => None,
        }
    }
}

// ── Constants ──

/// Maximum number of IRQ hooks. C: `NR_IRQ_HOOKS` — system.h
pub const NR_IRQ_HOOKS: usize = 64;

/// Maximum VDEVIO buffer size in bytes. C: `VDEVIO_BUF_SIZE` — do_vdevio.c:17
/// C uses `char vdevio_buf[VDEVIO_BUF_SIZE]` = 64 bytes (not elements).
pub const VDEVIO_BUF_SIZE: usize = 64;

// ── VDEVIO (port, value) pair types ──
// C: devio.h:21-23 — `pvb_pair_t`, `pvw_pair_t`, `pvl_pair_t`
// `#[repr(C)]` matches C struct layout (with padding for alignment).

/// Byte-sized (port, value) pair. C: `pvb_pair_t` — devio.h:21
/// Layout: 2-byte port + 1-byte value + 1-byte padding = 4 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct PvBytePair {
    port: u16,
    value: u8,
}

/// Word-sized (port, value) pair. C: `pvw_pair_t` — devio.h:22
/// Layout: 2-byte port + 2-byte value = 4 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct PvWordPair {
    port: u16,
    value: u16,
}

/// Long-sized (port, value) pair. C: `pvl_pair_t` — devio.h:23
/// Layout: 2-byte port + 2-byte padding + 4-byte value = 8 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct PvLongPair {
    port: u16,
    value: u32,
}

// ── Helper ──

fn msg_m1(msg: &Message) -> MessageM1 {
    // SAFETY: `m_type` has been validated by the caller to select the M1
    // format. All union variants share the same size and `#[repr(C)]`
    // layout, so reading a different variant is sound.
    unsafe { msg.m_u.m_m1 }
}

/// Check if a process is allowed to use a given IRQ vector.
///
/// C: do_irqctl.c:62-76 — `if (privp->s_flags & CHECK_IRQ)` loop.
fn check_irq_permission(caller_priv: &KPriv, irq_vec: i32) -> bool {
    if !caller_priv.capability.s_flags.contains(PrivFlagsBits::CHECK_IRQ) {
        return true; // No CHECK_IRQ flag → unrestricted
    }
    // C: do_irqctl.c:67-72 — scan s_irq_tab for matching vector
    for i in 0..caller_priv.io.s_nr_irq as usize {
        if i < caller_priv.io.s_irq_tab.len() && caller_priv.io.s_irq_tab[i] == irq_vec {
            return true;
        }
    }
    false
}

// ── Port I/O trait (re-exported from minix_plat) ──

/// Architecture-specific port I/O operations.
///
/// Re-exported from `minix_plat::PortIo`. The trait was moved to the
/// platform crate so that x86_64 can provide a real implementation
/// (`X86_64PortIo` using `in/out` instructions) while mock/ARM/RISC-V
/// use `MockPortIo` (no-op). This follows the same pattern as
/// `InterruptController` and `EarlyConsole`.
///
/// Design: Doc 20-syscall-device.md §3 D2.
pub use minix_plat::PortIo;

// ── Dispatch functions ──

/// Dispatch SYS_IRQCTL.
///
/// C: `do_irqctl()` — do_irqctl.c
///
/// Control interrupt hooks: set policy, remove policy, enable, disable.
///
/// **Note**: The `IrqManager` parameter is required for hook operations.
/// The `PrivTable` parameter is required for CHECK_IRQ permission checks.
/// Both are passed from `kernel_call_dispatch` via the syscall dispatch layer.
pub fn dispatch_irqctl<IC: InterruptController>(
    caller: &mut KProcess,
    msg: &mut Message,
    irq_mgr: &mut IrqManager<IC>,
    priv_table: &PrivTable,
) -> KcallResult {
    // C: do_irqctl.c:24-25 — extract parameters from mess_lsys_krn_sys_irqctl.
    //
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   request@0, vector@4, policy@8, hook_id@12
    // while MessageM1 has m1i1@0, m1i2@4, m1i3@8, then 4 bytes padding for
    // 8-byte alignment, then m1p1@16. Reading `m1p1` for `hook_id` would
    // read offset 16 (padding) instead of offset 12 — a field-mapping bug.
    // Always use the dedicated `MessLsysKrnSysIrqctl` variant.
    msg.debug_check_m_type_any(&[Syscall::Irqctl as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let irq = unsafe { msg.m_u.m_lsys_krn_sys_irqctl };
    let request = irq.request;
    let irq_vec = irq.vector;
    let policy = irq.policy as u32;
    let hook_id = irq.hook_id;

    let req = match IrqctlRequest::try_from(request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    match req {
        IrqctlRequest::SetPolicy => {
            // C: do_irqctl.c:55-56 — validate IRQ vector
            if irq_vec < 0 || irq_vec as usize >= NR_IRQ_VECTORS {
                return KcallResult::Ok(EINVAL);
            }

            // C: do_irqctl.c:58-76 — CHECK_IRQ permission
            let caller_priv = caller.priv_id.and_then(|pid| priv_table.get(pid));
            match caller_priv {
                None => return KcallResult::Ok(EPERM),
                Some(priv_) => {
                    if !check_irq_permission(priv_, irq_vec) {
                        return KcallResult::Ok(EPERM);
                    }
                }
            }

            // C: do_irqctl.c:78-79 — validate notify_id
            let notify_id = hook_id;
            // C: `if (notify_id > CHAR_BIT * sizeof(irq_id_t) - 1)`
            // irq_id_t is u32 → 31 bits max
            if notify_id > 31 {
                return KcallResult::Ok(EINVAL);
            }

            // C: do_irqctl.c:82-106 — find existing or free hook, install handler
            let irq = IrqVector::new(irq_vec as u8);
            let nid = IrqNotifyId::new(notify_id as u32);
            let pol = if policy & IRQ_REENABLE != 0 {
                IrqPolicy::REENABLE
            } else {
                IrqPolicy::empty()
            };

            match irq_mgr.irqctl_set_policy(irq, caller.p_endpoint, nid, pol) {
                Ok(new_hook_id) => {
                    // C: do_irqctl.c:108 — return hook_id in reply
                    // Write the 1-based hook_id back into the message using the
                    // dedicated irqctl variant (matches the read path above;
                    // writing to m1p1@16 would land in padding, not hook_id@12).
                    msg.debug_check_m_type_any(&[Syscall::Irqctl as i32]);
                    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
                    msg.m_u.m_lsys_krn_sys_irqctl.hook_id = new_hook_id;
                }
                Err(crate::irq_manager::IrqError::NoFreeSlots) => {
                    return KcallResult::Ok(ENOSPC);
                }
                Err(_) => {
                    return KcallResult::Ok(EINVAL);
                }
            }
        }
        IrqctlRequest::RmPolicy => {
            // C: do_irqctl.c:111-114 — validate hook_id
            // C uses 1-based hook_id; convert to 0-based index
            let slot_idx = (hook_id - 1) as usize;
            if hook_id < 1 || slot_idx >= NR_IRQ_HOOKS {
                return KcallResult::Ok(EINVAL);
            }

            // C: do_irqctl.c:112 — check proc_nr_e == NONE
            match irq_mgr.hook_owner(slot_idx) {
                None => return KcallResult::Ok(EINVAL), // slot empty
                Some(owner) => {
                    // C: do_irqctl.c:113 — check owner == caller
                    if owner != caller.p_endpoint {
                        return KcallResult::Ok(EPERM);
                    }
                }
            }

            // C: do_irqctl.c:118-120 — rm_irq_handler + clear slot
            if irq_mgr.remove_hook_by_slot(slot_idx).is_err() {
                return KcallResult::Ok(EINVAL);
            }
        }
        IrqctlRequest::Enable => {
            // C: do_irqctl.c:31-32 — validate hook_id, check owner, enable
            let slot_idx = (hook_id - 1) as usize;
            if hook_id < 1 || slot_idx >= NR_IRQ_HOOKS {
                return KcallResult::Ok(EINVAL);
            }

            match irq_mgr.hook_owner(slot_idx) {
                None => return KcallResult::Ok(EINVAL),
                Some(owner) => {
                    if owner != caller.p_endpoint {
                        return KcallResult::Ok(EPERM);
                    }
                }
            }

            // Enable the IRQ for this hook
            // C: enable_irq(&irq_hooks[irq_hook_id])
            irq_mgr.enable_irq_by_slot(slot_idx);
        }
        IrqctlRequest::Disable => {
            // C: do_irqctl.c:31-32 — validate hook_id, check owner, disable
            let slot_idx = (hook_id - 1) as usize;
            if hook_id < 1 || slot_idx >= NR_IRQ_HOOKS {
                return KcallResult::Ok(EINVAL);
            }

            match irq_mgr.hook_owner(slot_idx) {
                None => return KcallResult::Ok(EINVAL),
                Some(owner) => {
                    if owner != caller.p_endpoint {
                        return KcallResult::Ok(EPERM);
                    }
                }
            }

            // Disable the IRQ for this hook
            // C: disable_irq(&irq_hooks[irq_hook_id])
            irq_mgr.disable_irq_by_slot(slot_idx);
        }
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_DEVIO (x86-only).
///
/// C: `do_devio()` — do_devio.c
///
/// Perform a single I/O port read or write.
///
/// # C Semantic Alignment (do_devio.c:22-100)
///
/// 1. Parse `request` into `io_type` (`_DIO_TYPEMASK`) and `io_dir` (`_DIO_DIRMASK`)
/// 2. `CHECK_IO_PORT` permission: scan `s_io_tab` for matching range
/// 3. Alignment check: `port & (size-1)` must be zero
/// 4. Execute I/O via `PortIo` trait methods
/// 5. For input: write result back to `m_krn_lsys_sys_devio.value`
pub fn dispatch_devio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &mut Message,
    port_io: &PI,
    priv_table: &PrivTable,
) -> KcallResult {
    // C: do_devio.c:22-24 — extract parameters from mess_lsys_krn_sys_devio.
    //
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   request@0, port@4, value@8
    // while MessageM1 has m1i1@0, m1i2@4, m1i3@8, then 4 bytes padding,
    // then m1p1@16. Reading `m1p1` for `value` would read offset 16
    // (padding) instead of offset 8 — the same field-mapping bug class as
    // SYS_IRQCTL (see dispatch_irqctl above). Always use the dedicated
    // `MessLsysKrnSysDevio` variant.
    msg.debug_check_m_type_any(&[Syscall::Devio as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let devio = unsafe { msg.m_u.m_lsys_krn_sys_devio };
    let request = devio.request;
    let port = devio.port as u16;
    let value = devio.value;

    // C: do_devio.c:26-29 — parse type and direction
    let io_type = request & 0x0F0;   // _DIO_TYPEMASK
    let io_dir = request & 0x00F;    // _DIO_DIRMASK

    let size = match IoSize::from_request_mask(io_type) {
        Some(s) => s,
        None => return KcallResult::Ok(EINVAL),
    };

    let dir = match IoDirection::from_request_mask(io_dir) {
        Some(d) => d,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_devio.c:31-58 — CHECK_IO_PORT permission
    let caller_priv = caller.priv_id.and_then(|pid| priv_table.get(pid));
    if let Some(priv_) = caller_priv
        && priv_.capability.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
            // C: do_devio.c:42-53 — scan s_io_tab for matching range
            let mut allowed = false;
            for i in 0..priv_.io.s_nr_io_range as usize {
                if i < priv_.io.s_io_tab.len() {
                    let ior = &priv_.io.s_io_tab[i];
                    // C: do_devio.c:50 — if (port >= iorp->ior_base && port+size-1 <= iorp->ior_limit)
                    if port as u32 >= ior.base && port as u32 + size as u32 - 1 <= ior.limit {
                        allowed = true;
                        break;
                    }
                }
            }
            if !allowed {
                return KcallResult::Ok(EPERM);
            }
        }
    // C: do_devio.c:33-36 — if no priv structure, goto doit (allow)
    // Rust: no priv → caller_priv is None → skip check (same as C "goto doit")

    // C: do_devio.c:60-65 — alignment check
    if !port.is_multiple_of(size as u16) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_devio.c:68-100 — execute in/out
    match dir {
        IoDirection::Input => {
            let result = match size {
                IoSize::Byte => port_io.inb(port) as u32,
                IoSize::Word => port_io.inw(port) as u32,
                IoSize::Long => port_io.inl(port),
            };
            // C: do_devio.c:71-80 — write result to m_krn_lsys_sys_devio.value
            // Reply value lives at offset 0 (`MessKrnLsysSysDevio`), NOT at
            // M1's `m1p1` @16 — same field-mapping reasoning as above.
            msg.debug_check_m_type_any(&[Syscall::Devio as i32]);
            // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
            msg.m_u.m_krn_lsys_sys_devio.value = result;
        }
        IoDirection::Output => {
            match size {
                IoSize::Byte => port_io.outb(port, value as u8),
                IoSize::Word => port_io.outw(port, value as u16),
                IoSize::Long => port_io.outl(port, value),
            }
        }
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_VDEVIO (x86-only).
///
/// C: `do_vdevio()` — do_vdevio.c
///
/// Perform a batch of I/O port operations. The (port, value) pairs are
/// copied from user space, permission-checked, executed, and (for input)
/// copied back.
///
/// # Anti-translate
///
/// C uses a static `char vdevio_buf[64]` buffer with union casts
/// (`pvb`/`pvw`/`pvl`). Rust uses a stack `[u8; VDEVIO_BUF_SIZE]`
/// buffer and `#[repr(C)]` struct arrays — the type system ensures
/// correct layout without union punning.
///
/// C uses `data_copy(caller, user_addr, KERNEL, buf, bytes)` for the
/// user→kernel copy. Rust uses `pte_walk::copy_from_user` which walks
/// the caller's page table via Direct Map — no magic KERNEL endpoint.
pub fn dispatch_vdevio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &Message,
    port_io: &PI,
    priv_table: &PrivTable,
) -> KcallResult {
    // C: do_vdevio.c:44-52 — extract parameters from mess_lsys_krn_sys_vdevio.
    //
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   request@0, vec_size@4, vec_addr@8 (vir_bytes)
    // while MessageM1 has m1i1@0, m1i2@4, m1i3@8, then 4 bytes padding,
    // then m1p1@16. Reading `m1p1` for `vec_addr` would read offset 16
    // (padding) instead of offset 8 — the same field-mapping bug class as
    // SYS_IRQCTL (see dispatch_irqctl above). `MessageM1` cannot express
    // this layout at all (no u64 field at offset 8). Always use the
    // dedicated `MessLsysKrnSysVdevio` variant.
    msg.debug_check_m_type_any(&[Syscall::Vdevio as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let vdevio = unsafe { msg.m_u.m_lsys_krn_sys_vdevio };
    let request = vdevio.request;
    let vec_addr = vdevio.vec_addr;
    let vec_size = vdevio.vec_size;

    // C: do_vdevio.c:54-72 — parse type/direction, validate size
    let io_type = request & 0x0F0;   // _DIO_TYPEMASK
    let io_dir = request & 0x00F;    // _DIO_DIRMASK

    let size = match IoSize::from_request_mask(io_type) {
        Some(s) => s,
        None => return KcallResult::Ok(EINVAL),
    };

    let dir = match IoDirection::from_request_mask(io_dir) {
        Some(d) => d,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vdevio.c:56-58 — validate vec_size
    if vec_size <= 0 {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_vdevio.c:50-64 — compute bytes = vec_size * sizeof(pair)
    let pair_size = match size {
        IoSize::Byte => core::mem::size_of::<PvBytePair>(),
        IoSize::Word => core::mem::size_of::<PvWordPair>(),
        IoSize::Long => core::mem::size_of::<PvLongPair>(),
    };
    let bytes = match (vec_size as usize).checked_mul(pair_size) {
        Some(b) => b,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vdevio.c:65 — if (bytes > sizeof(vdevio_buf)) return E2BIG
    if bytes > VDEVIO_BUF_SIZE {
        return KcallResult::Ok(E2BIG);
    }

    // C: do_vdevio.c:67-70 — copy (port,value) pairs from user
    //
    // Uses `data_copy_vmcheck` (arch-independent page table walk via
    // `CurrentPteWalk`) instead of the older `copy_from_user` which
    // hardcoded x86_64 page table walking. This also supports VM
    // suspend/resume for lazy-allocated pages, matching C's `data_copy`.
    use crate::cross_space::data_copy_vmcheck;
    use crate::vm::{AddressRef, CrossSpaceResult};
    use minix_arch::{CurrentDirectMap, DirectMapArch};
    use minix_types::{Endpoint, VirBytes};

    let mut buf = [0u8; VDEVIO_BUF_SIZE];
    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;

    let dst_phys = CurrentDirectMap::virt_to_phys(VirBytes(
        buf.as_mut_ptr() as u64,
    ));
    let proc_cr3 = |ep: Endpoint| {
        if ep == caller_endpt { Some(caller_cr3) } else { None }
    };
    let src = AddressRef::Process {
        endpoint: caller_endpt,
        offset: VirBytes(vec_addr),
    };
    let dst = AddressRef::Physical(dst_phys);

    match data_copy_vmcheck(caller, src, dst, bytes, proc_cr3) {
        CrossSpaceResult::Completed(Ok(())) => {}
        CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
    }

    // C: do_vdevio.c:72-100 — batch permission check
    let caller_priv = caller.priv_id.and_then(|pid| priv_table.get(pid));
    if let Some(priv_) = caller_priv
        && priv_.capability.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
            for i in 0..vec_size as usize {
                let port = match size {
                    IoSize::Byte => {
                        let pairs: &[PvBytePair] = unsafe {
                            core::slice::from_raw_parts(buf.as_ptr() as *const PvBytePair, vec_size as usize)
                        };
                        pairs[i].port
                    }
                    IoSize::Word => {
                        let pairs: &[PvWordPair] = unsafe {
                            core::slice::from_raw_parts(buf.as_ptr() as *const PvWordPair, vec_size as usize)
                        };
                        pairs[i].port
                    }
                    IoSize::Long => {
                        let pairs: &[PvLongPair] = unsafe {
                            core::slice::from_raw_parts(buf.as_ptr() as *const PvLongPair, vec_size as usize)
                        };
                        pairs[i].port
                    }
                };
                // C: do_vdevio.c:84-91 — scan s_io_tab for matching range
                let mut allowed = false;
                for j in 0..priv_.io.s_nr_io_range as usize {
                    if j < priv_.io.s_io_tab.len() {
                        let ior = &priv_.io.s_io_tab[j];
                        if port as u32 >= ior.base
                            && port as u32 + size as u32 - 1 <= ior.limit
                        {
                            allowed = true;
                            break;
                        }
                    }
                }
                if !allowed {
                    return KcallResult::Ok(EPERM);
                }
            }
        }

    // C: do_vdevio.c:102-149 — perform batch I/O
    match (dir, size) {
        (IoDirection::Input, IoSize::Byte) => {
            let pairs: &mut [PvBytePair] = unsafe {
                core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut PvBytePair, vec_size as usize)
            };
            for pair in pairs.iter_mut() {
                pair.value = port_io.inb(pair.port);
            }
        }
        (IoDirection::Output, IoSize::Byte) => {
            let pairs: &[PvBytePair] = unsafe {
                core::slice::from_raw_parts(buf.as_ptr() as *const PvBytePair, vec_size as usize)
            };
            for pair in pairs {
                port_io.outb(pair.port, pair.value);
            }
        }
        (IoDirection::Input, IoSize::Word) => {
            let pairs: &mut [PvWordPair] = unsafe {
                core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut PvWordPair, vec_size as usize)
            };
            for pair in pairs.iter_mut() {
                // C: do_vdevio.c:116 — if (port & 1) goto bad (panic)
                if pair.port & 1 != 0 {
                    return KcallResult::Ok(EPERM);
                }
                pair.value = port_io.inw(pair.port);
            }
        }
        (IoDirection::Output, IoSize::Word) => {
            let pairs: &[PvWordPair] = unsafe {
                core::slice::from_raw_parts(buf.as_ptr() as *const PvWordPair, vec_size as usize)
            };
            for pair in pairs {
                if pair.port & 1 != 0 {
                    return KcallResult::Ok(EPERM);
                }
                port_io.outw(pair.port, pair.value);
            }
        }
        (IoDirection::Input, IoSize::Long) => {
            let pairs: &mut [PvLongPair] = unsafe {
                core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut PvLongPair, vec_size as usize)
            };
            for pair in pairs.iter_mut() {
                // C: do_vdevio.c:136 — if (port & 3) goto bad (panic)
                if pair.port & 3 != 0 {
                    return KcallResult::Ok(EPERM);
                }
                pair.value = port_io.inl(pair.port);
            }
        }
        (IoDirection::Output, IoSize::Long) => {
            let pairs: &[PvLongPair] = unsafe {
                core::slice::from_raw_parts(buf.as_ptr() as *const PvLongPair, vec_size as usize)
            };
            for pair in pairs {
                if pair.port & 3 != 0 {
                    return KcallResult::Ok(EPERM);
                }
                port_io.outl(pair.port, pair.value);
            }
        }
    }

    // C: do_vdevio.c:151-156 — copy back results for input
    //
    // Uses `data_copy_vmcheck` (kernel→user direction) for the same
    // reasons as the user→kernel copy above.
    if dir == IoDirection::Input {
        let src_phys = CurrentDirectMap::virt_to_phys(VirBytes(
            buf.as_ptr() as u64,
        ));
        let proc_cr3 = |ep: Endpoint| {
            if ep == caller_endpt { Some(caller_cr3) } else { None }
        };
        let src = AddressRef::Physical(src_phys);
        let dst = AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(vec_addr),
        };
        match data_copy_vmcheck(caller, src, dst, bytes, proc_cr3) {
            CrossSpaceResult::Completed(Ok(())) => {}
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
        }
    }

    KcallResult::Ok(OK)
}

// ── SYS_IOPENABLE ──

/// Dispatch SYS_IOPENABLE (x86-only).
///
/// C: `do_iopenable()` — arch/i386/do_iopenable.c
///
/// Allow a user process to use I/O instructions. On x86-64 this sets
/// RFLAGS.IOPL=3; on aarch64/riscv64 it is a no-op. The kernel layer
/// only knows the OS concept "enable user I/O"; the arch layer
/// (`CpuContextArch::enable_user_io`) decides how to encode it
/// (06-proc-init-boot-proc.md §3.5).
///
/// For already-running processes, the RFLAGS.IOPL change is written into
/// the target's saved `cpu_context.psw` (see body below) — no scheduler
/// hook is needed because the context is re-loaded on return to user mode.
pub fn dispatch_iopenable(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut crate::proc_table::ProcessTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_iopenable.c:25-28 — extract and validate endpoint
    let endpt = m1.m1i1; // m_lsys_krn_sys_iopenable.endpt

    // C: do_iopenable.c:24-25 — SELF → use caller's endpoint
    // C: okendpt(caller->p_endpoint, &proc_nr) maps SELF to caller.
    let target_ep = if endpt == minix_types::Endpoint::SELF.0 {
        caller.p_endpoint.0
    } else {
        endpt
    };

    // C: do_iopenable.c:26 — isokendpt check
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(target_ep)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_iopenable.c:29 — kernel processes are denied
    // (C: #if ENABLE_USERPRIV && ENABLE_USERIOPL; else returns EPERM)
    if crate::proc_table::ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_iopenable.c:28 — enable_iop(proc_addr(proc_nr))
    // C: enable_iop() sets IOPL=3: pp->p_reg.psw |= 0x3000
    // The Rust rewrite sinks the RFLAGS.IOPL write into the arch layer
    // (06-proc-init-boot-proc.md §3.5): the kernel only knows the OS concept
    // "enable user I/O"; the arch decides how to encode it (x86-64:
    // RFLAGS |= 0x3000; aarch64/riscv64: no-op).
    //
    // In C, `p_reg` is the single register save area embedded in
    // `struct proc` — there is no separate "exception frame on the
    // kernel stack". `do_iopenable()` is always called from a syscall
    // handler, so the target's registers are already saved in
    // `p_reg`/`cpu_context`. Modifying `cpu_context.psw` here takes
    // effect when the process returns to user mode. No additional
    // scheduler hook is needed.
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.enable_user_io();
    }

    KcallResult::Ok(0) // OK
}

// ── SYS_SDEVIO ──

/// `_DIO_SAFE` flag mask. C: com.h:288 — `#define _DIO_SAFE 0x100`
const DIO_SAFE: i32 = 0x100;
/// `_DIO_SAFEMASK` mask. C: com.h:289 — `#define _DIO_SAFEMASK 0xf00`
const DIO_SAFEMASK: i32 = 0xf00;

/// Dispatch SYS_SDEVIO (batch I/O, x86-only).
///
/// C: `do_sdevio()` — arch/i386/do_sdevio.c
///
/// Perform a batch of I/O port operations (byte/word) to/from a buffer
/// in another process's address space.
///
/// # Implementation Status
///
/// - **Unsafe path** (non-SAFE, target == caller): fully implemented.
///   Uses `copy_from_user` + `PortIo::insb`/`outsb`/`insw`/`outsw` +
///   `copy_to_user`. No `switch_address_space` needed (already in caller's
///   address space).
/// - **SAFE path** (grant-based): fully implemented. Uses `verify_grant`
///   to resolve the grant → granter's virtual address, then
///   `data_copy_vmcheck` to copy between granter's buffer and a kernel
///   buffer, performing string I/O via `PortIo` trait methods.
pub fn dispatch_sdevio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &Message,
    port_io: &PI,
    priv_table: &PrivTable,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    // C: do_sdevio.c:42-46 — extract parameters via dedicated struct
    msg.debug_check_m_type_any(&[Syscall::Sdevio as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    // Using the dedicated `MessLsysKrnSysSdevio` overlay ensures correct
    // field offsets (the generic `m1` overlay would misparse fields).
    let sdevio = unsafe { msg.m_u.m_lsys_krn_sys_sdevio };
    let request = sdevio.request;
    let port = sdevio.port;
    let vec_endpt = sdevio.vec_endpt;
    let vec_addr = sdevio.vec_addr;
    let vec_size = sdevio.vec_size;
    let _offset = sdevio.offset;

    // C: do_sdevio.c:48-58 — resolve target endpoint
    // SELF → use caller's endpoint; otherwise validate via isokendpt.
    let target_ep = if vec_endpt == Endpoint::SELF.0 {
        caller.p_endpoint
    } else {
        match proc_table.endpoint_to_nr(Endpoint(vec_endpt)) {
            Some(_) => Endpoint(vec_endpt),
            None => return KcallResult::Ok(EINVAL),
        }
    };

    // C: do_sdevio.c:59-60 — kernel processes are denied
    let target_nr = match proc_table.endpoint_to_nr(target_ep) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    if crate::proc_table::ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_sdevio.c:62-63 — extract direction and type
    let req_dir = request & 0x00F;   // _DIO_DIRMASK
    let req_type = request & 0x0F0;  // _DIO_TYPEMASK

    // C: do_sdevio.c:65-93 — safe variant handling (verify_grant)
    // For unsafe variants, C requires the target to be the caller itself
    // (do_sdevio.c:84-90). We enforce this check here.
    let is_safe = (request & DIO_SAFEMASK) == DIO_SAFE;
    if !is_safe {
        // C: do_sdevio.c:84-90 — unsafe sdevio only allowed if target == caller
        if target_nr != caller.p_nr {
            return KcallResult::Ok(EPERM);
        }
    }

    // C: do_sdevio.c:95-100 — determine element size
    // SDEVIO only supports byte and word (long is not supported).
    let size = match req_type {
        0x010 => 1usize,  // _DIO_BYTE
        0x020 => 2usize,  // _DIO_WORD
        _ => return KcallResult::Ok(EINVAL),  // _DIO_LONG not supported
    };

    // C: do_sdevio.c:102-122 — CHECK_IO_PORT permission
    let caller_priv = caller.priv_id.and_then(|pid| priv_table.get(pid));
    if let Some(priv_) = caller_priv
        && priv_.capability.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
            let mut allowed = false;
            for i in 0..priv_.io.s_nr_io_range as usize {
                if i < priv_.io.s_io_tab.len() {
                    let ior = &priv_.io.s_io_tab[i];
                    // C: do_sdevio.c:113 — port range check
                    if port as u32 >= ior.base
                        && port as u32 + (size as u32) - 1 <= ior.limit
                    {
                        allowed = true;
                        break;
                    }
                }
            }
            if !allowed {
                return KcallResult::Ok(EPERM);
            }
        }

    // C: do_sdevio.c:124-129 — alignment check
    if port % (size as i64) != 0 {
        return KcallResult::Ok(EPERM);
    }

    // C: do_sdevio.c:131-153 — perform string I/O
    //
    // For the unsafe (non-SAFE) path where target == caller, we are already
    // in the caller's address space — no `switch_address_space` needed.
    // We use `copy_from_user` to read the user buffer into a kernel buffer,
    // perform string I/O via `PortIo::insb`/`outsb`/`insw`/`outsw`, then
    // `copy_to_user` to write results back for input.
    //
    // For the SAFE path, `verify_grant` resolves the grant to the granter's
    // virtual address, then `data_copy_vmcheck` copies between the granter's
    // buffer and a kernel buffer.

    // Validate direction: C only accepts _DIO_INPUT (0x001) and _DIO_OUTPUT
    // (0x002); any other value returns EINVAL (do_sdevio.c:148-152).
    let is_input = match req_dir {
        0x001 => true,   // _DIO_INPUT
        0x002 => false,  // _DIO_OUTPUT
        _ => return KcallResult::Ok(EINVAL),
    };

    if is_safe {
        // SAFE path: verify_grant resolves grant → granter's virtual address,
        // then data_copy_vmcheck copies between granter's buffer and kernel.
        use crate::grant::{verify_grant, VerifyGrantOutcome, CpFlags};
        use crate::cross_space::data_copy_vmcheck;
        use crate::vm::{AddressRef, CrossSpaceResult};
        use minix_arch::{CurrentDirectMap, DirectMapArch};
        use minix_types::VirBytes;

        let total_bytes = match (vec_size as usize).checked_mul(size) {
            Some(b) => b,
            None => return KcallResult::Ok(EINVAL),
        };
        const SDEVIO_BUF_MAX: usize = 4096;
        if total_bytes > SDEVIO_BUF_MAX {
            return KcallResult::Ok(E2BIG);
        }

        // C: do_sdevio.c:65-72 — verify_grant
        // Input (port→buffer): grantee writes to grant → CPF_WRITE
        // Output (buffer→port): grantee reads from grant → CPF_READ
        let access = if is_input { CpFlags::WRITE } else { CpFlags::READ };

        let caller_endpt = caller.p_endpoint;
        let caller_cr3 = caller.p_seg.phys_root;
        let proc_cr3 = |endpt: Endpoint| {
            if endpt == caller_endpt {
                Some(caller_cr3)
            } else {
                proc_table
                    .endpoint_to_nr(endpt)
                    .and_then(|nr| proc_table.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };

        let outcome = verify_grant(
            caller,
            target_ep,
            caller_endpt,
            vec_addr as i32,
            total_bytes as u64,
            access,
            sdevio.offset,
            proc_table,
            priv_table,
            &proc_cr3,
        );

        let grant_result = match outcome {
            VerifyGrantOutcome::Ok(r) => r,
            VerifyGrantOutcome::Err(e) => return KcallResult::Ok(e),
            VerifyGrantOutcome::Suspended(_) => return KcallResult::VmSuspend,
        };

        // grant_result.offset = virtual address in granter's space.
        // grant_result.effective_granter = endpoint of the granter.
        let granter = grant_result.effective_granter;
        let granter_vaddr = grant_result.offset;

        let mut buf = [0u8; SDEVIO_BUF_MAX];
        let buf_phys = CurrentDirectMap::virt_to_phys(VirBytes(
            buf.as_mut_ptr() as u64,
        ));

        // For output: copy grant buffer → kernel, then write to I/O port.
        if !is_input {
            let src = AddressRef::Process {
                endpoint: granter,
                offset: granter_vaddr,
            };
            let dst = AddressRef::Physical(buf_phys);
            match data_copy_vmcheck(caller, src, dst, total_bytes, proc_cr3) {
                CrossSpaceResult::Completed(Ok(())) => {}
                CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
            }
        }

        // Perform string I/O (shared logic with unsafe path).
        match (is_input, size) {
            (true, 1) => port_io.insb(port as u16, &mut buf[..total_bytes]),
            (false, 1) => port_io.outsb(port as u16, &buf[..total_bytes]),
            (true, 2) => {
                let words: &mut [u16] = unsafe {
                    core::slice::from_raw_parts_mut(
                        buf.as_mut_ptr() as *mut u16,
                        vec_size as usize,
                    )
                };
                port_io.insw(port as u16, words);
            }
            (false, 2) => {
                let words: &[u16] = unsafe {
                    core::slice::from_raw_parts(
                        buf.as_ptr() as *const u16,
                        vec_size as usize,
                    )
                };
                port_io.outsw(port as u16, words);
            }
            _ => return KcallResult::Ok(EINVAL),
        }

        // For input: copy kernel buffer → grant buffer.
        if is_input {
            let src = AddressRef::Physical(buf_phys);
            let dst = AddressRef::Process {
                endpoint: granter,
                offset: granter_vaddr,
            };
            match data_copy_vmcheck(caller, src, dst, total_bytes, proc_cr3) {
                CrossSpaceResult::Completed(Ok(())) => {}
                CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
                CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
            }
        }

        return KcallResult::Ok(OK);
    }

    // Unsafe path: target == caller (already verified above).
    // Perform string I/O using copy_from_user/copy_to_user + PortIo trait.
    use crate::pte_walk::{copy_from_user, copy_to_user};
    use minix_types::VirBytes;

    let root_paddr = caller.p_seg.phys_root;
    let total_bytes = match (vec_size as usize).checked_mul(size) {
        Some(b) => b,
        None => return KcallResult::Ok(EINVAL),
    };

    // Cap buffer size to prevent stack overflow (max 4KB).
    const SDEVIO_BUF_MAX: usize = 4096;
    if total_bytes > SDEVIO_BUF_MAX {
        return KcallResult::Ok(E2BIG);
    }

    let mut buf = [0u8; SDEVIO_BUF_MAX];

    // For output: copy user buffer → kernel, then write to I/O port.
    // For input: read from I/O port → kernel buffer, then copy to user.
    if !is_input {
        // Output: read user data first
        match copy_from_user(root_paddr, VirBytes(vec_addr), &mut buf[..total_bytes]) {
            Ok(()) => {}
            Err(_) => return KcallResult::Ok(EFAULT),
        }
    }

    // Perform string I/O
    match (is_input, size) {
        (true, 1) => port_io.insb(port as u16, &mut buf[..total_bytes]),
        (false, 1) => port_io.outsb(port as u16, &buf[..total_bytes]),
        (true, 2) => {
            let words: &mut [u16] = unsafe {
                core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u16, vec_size as usize)
            };
            port_io.insw(port as u16, words);
        }
        (false, 2) => {
            let words: &[u16] = unsafe {
                core::slice::from_raw_parts(buf.as_ptr() as *const u16, vec_size as usize)
            };
            port_io.outsw(port as u16, words);
        }
        _ => return KcallResult::Ok(EINVAL), // _DIO_LONG not supported
    }

    if is_input {
        // Input: copy results back to user
        match copy_to_user(&buf[..total_bytes], root_paddr, VirBytes(vec_addr)) {
            Ok(()) => {}
            Err(_) => return KcallResult::Ok(EFAULT),
        }
    }

    KcallResult::Ok(OK)
}

// ── SYS_READBIOS ──

/// BIOS memory range constants. C: memory.h (i386)
///
/// `do_readbios` allows reading from two BIOS memory regions:
/// 1. `BIOS_MEM_BEGIN..=BIOS_MEM_END` (0x00000..=0x004FF) — IVT + BIOS data
/// 2. `BASE_MEM_TOP..=UPPER_MEM_END` (0x090000..=0x0FFFFF) — upper memory area
#[allow(dead_code)] // BIOS memory start; not yet wired to all call sites
const BIOS_MEM_BEGIN: u64 = 0x00000;
const BIOS_MEM_END: u64 = 0x004FF;
const BASE_MEM_TOP: u64 = 0x090000;
const UPPER_MEM_END: u64 = 0x0FFFFF;

/// Dispatch SYS_READBIOS (x86-only).
///
/// C: `do_readbios()` — arch/i386/do_readbios.c
///
/// Copy data from the BIOS memory area to a user-space buffer. The source
/// is a physical address (BIOS area), the destination is the caller's
/// virtual buffer.
///
/// # Anti-translate
///
/// C uses `virtual_copy_vmcheck(caller, &src, &dst, size)` with
/// `src.proc_nr_e = NONE` (physical) and `dst.proc_nr_e = caller`.
/// Rust uses Direct Map to read the physical BIOS memory, then
/// `pte_walk::copy_to_user` to write to the caller's buffer —
/// no magic NONE endpoint, the type system distinguishes physical
/// (Direct Map) from user-virtual (PTE walk) addressing.
pub fn dispatch_readbios(
    caller: &mut KProcess,
    msg: &Message,
) -> KcallResult {
    // C: do_readbios.c:19-22 — extract parameters via dedicated struct
    msg.debug_check_m_type_any(&[Syscall::Readbios as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let readbios = unsafe { msg.m_u.m_lsys_krn_readbios };
    let size = readbios.size;
    let addr = readbios.addr;
    let buf = readbios.buf;

    // C: do_readbios.c:26 — limit = addr + size - 1
    if size == 0 {
        return KcallResult::Ok(EINVAL);
    }
    let limit = match addr.checked_add(size - 1) {
        Some(l) => l,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_readbios.c:31-33 — BIOS memory range check.
    // `BIOS_MEM_BEGIN == 0` so `addr >= BIOS_MEM_BEGIN` is always true for
    // u64; only the upper bound needs checking.
    let in_bios = limit <= BIOS_MEM_END;
    let in_upper = addr >= BASE_MEM_TOP && limit <= UPPER_MEM_END;
    if !in_bios && !in_upper {
        return KcallResult::Ok(EPERM);
    }

    // C: do_readbios.c:35 — virtual_copy_vmcheck(caller, &src, &dst, size)
    // src is physical (NONE endpoint = BIOS memory), dst is caller's buffer.
    //
    // Uses `data_copy_vmcheck` (arch-independent page table walk via
    // `CurrentPteWalk`) instead of the older `copy_to_user` which hardcoded
    // x86_64 page table walking. Also supports VM suspend/resume for
    // lazy-allocated destination pages, matching C's `virtual_copy_vmcheck`.
    //
    // The copy is done page-by-page because `cross_space_copy` resolves
    // only the first page's physical address — multi-page copies require
    // iterating to handle non-contiguous physical mappings.
    use crate::cross_space::data_copy_vmcheck;
    use crate::vm::{AddressRef, CrossSpaceResult};
    use minix_types::{Endpoint, PhysBytes, VirBytes};

    let caller_endpt = caller.p_endpoint;
    let caller_cr3 = caller.p_seg.phys_root;
    let mut remaining = size as usize;
    let mut src_phys = addr;
    let mut dst_va = buf;

    while remaining > 0 {
        let chunk = core::cmp::min(remaining, 4096);
        let proc_cr3 = |ep: Endpoint| {
            if ep == caller_endpt { Some(caller_cr3) } else { None }
        };
        let src = AddressRef::Physical(PhysBytes(src_phys));
        let dst = AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(dst_va),
        };
        match data_copy_vmcheck(caller, src, dst, chunk, proc_cr3) {
            CrossSpaceResult::Completed(Ok(())) => {}
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
        }
        src_phys += chunk as u64;
        dst_va += chunk as u64;
        remaining -= chunk;
    }

    KcallResult::Ok(OK)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{Endpoint, MessLsysKrnReadbios, MessLsysKrnSysSdevio};
    use crate::proc::RtsFlagsBits;
    use crate::proc::ProcNr;

    #[test]
    fn test_irqctl_request_try_from() {
        assert_eq!(IrqctlRequest::try_from(0), Ok(IrqctlRequest::SetPolicy));
        assert_eq!(IrqctlRequest::try_from(1), Ok(IrqctlRequest::RmPolicy));
        assert_eq!(IrqctlRequest::try_from(2), Ok(IrqctlRequest::Enable));
        assert_eq!(IrqctlRequest::try_from(3), Ok(IrqctlRequest::Disable));
        assert_eq!(IrqctlRequest::try_from(99), Err(()));
    }

    #[test]
    fn test_io_size_from_mask() {
        // C values: _DIO_BYTE=0x010, _DIO_WORD=0x020, _DIO_LONG=0x030
        assert_eq!(IoSize::from_request_mask(0x010), Some(IoSize::Byte));
        assert_eq!(IoSize::from_request_mask(0x020), Some(IoSize::Word));
        assert_eq!(IoSize::from_request_mask(0x030), Some(IoSize::Long));
        assert_eq!(IoSize::from_request_mask(0x040), None);
        assert_eq!(IoSize::from_request_mask(0), None);
    }

    #[test]
    fn test_io_direction_from_mask() {
        // C values: _DIO_INPUT=0x001, _DIO_OUTPUT=0x002
        assert_eq!(IoDirection::from_request_mask(0x001), Some(IoDirection::Input));
        assert_eq!(IoDirection::from_request_mask(0x002), Some(IoDirection::Output));
        assert_eq!(IoDirection::from_request_mask(0x003), None);
    }

    #[test]
    fn test_dio_combined_request() {
        // C: DIO_INPUT_BYTE = _DIO_INPUT|_DIO_BYTE = 0x011
        let req = 0x011i32;
        let io_type = req & 0x0F0;
        let io_dir = req & 0x00F;
        assert_eq!(IoSize::from_request_mask(io_type), Some(IoSize::Byte));
        assert_eq!(IoDirection::from_request_mask(io_dir), Some(IoDirection::Input));

        // C: DIO_OUTPUT_LONG = _DIO_OUTPUT|_DIO_LONG = 0x032
        let req = 0x032i32;
        let io_type = req & 0x0F0;
        let io_dir = req & 0x00F;
        assert_eq!(IoSize::from_request_mask(io_type), Some(IoSize::Long));
        assert_eq!(IoDirection::from_request_mask(io_dir), Some(IoDirection::Output));
    }

    #[test]
    fn test_irq_reenable_flag() {
        assert_eq!(IRQ_REENABLE, 0x01);
    }

    #[test]
    fn test_check_irq_permission_no_flag() {
        // Without CHECK_IRQ flag, all IRQs are allowed
        let priv_ = KPriv::new(0);
        assert!(check_irq_permission(&priv_, 5));
        assert!(check_irq_permission(&priv_, 255));
    }

    #[test]
    fn test_check_irq_permission_with_flag_allowed() {
        let mut priv_ = KPriv::new(0);
        priv_.capability.s_flags = PrivFlagsBits::CHECK_IRQ;
        priv_.io.s_nr_irq = 2;
        priv_.io.s_irq_tab[0] = 5;
        priv_.io.s_irq_tab[1] = 10;
        assert!(check_irq_permission(&priv_, 5));
        assert!(check_irq_permission(&priv_, 10));
    }

    #[test]
    fn test_check_irq_permission_with_flag_denied() {
        let mut priv_ = KPriv::new(0);
        priv_.capability.s_flags = PrivFlagsBits::CHECK_IRQ;
        priv_.io.s_nr_irq = 1;
        priv_.io.s_irq_tab[0] = 5;
        assert!(!check_irq_permission(&priv_, 3));
        assert!(!check_irq_permission(&priv_, 10));
    }

    // ── PortIo mock for devio tests ──

    struct MockPortIo {
        /// Last (port, value) written via outb/outw/outl
        last_write: core::cell::RefCell<Option<(u16, u32)>>,
        /// Value to return for inb/inw/inl reads
        read_value: u32,
    }

    impl MockPortIo {
        fn new(read_value: u32) -> Self {
            Self {
                last_write: core::cell::RefCell::new(None),
                read_value,
            }
        }
    }

    impl PortIo for MockPortIo {
        fn inb(&self, _port: u16) -> u8 { self.read_value as u8 }
        fn outb(&self, port: u16, value: u8) {
            *self.last_write.borrow_mut() = Some((port, value as u32));
        }
        fn inw(&self, _port: u16) -> u16 { self.read_value as u16 }
        fn outw(&self, port: u16, value: u16) {
            *self.last_write.borrow_mut() = Some((port, value as u32));
        }
        fn inl(&self, _port: u16) -> u32 { self.read_value }
        fn outl(&self, port: u16, value: u32) {
            *self.last_write.borrow_mut() = Some((port, value));
        }
    }

    #[test]
    fn test_devio_input_byte_no_check() {
        // DIO_INPUT_BYTE = 0x011
        let mut msg = Message::default();
        msg.m_type = Syscall::Devio as i32;
        // SAFETY: writing to the dedicated devio variant of the union
        // (matches C `mess_lsys_krn_sys_devio` layout: request@0/port@4/value@8)
        msg.m_u.m_lsys_krn_sys_devio.request = 0x011; // request
        msg.m_u.m_lsys_krn_sys_devio.port = 0x60;     // port (aligned for byte)
        msg.m_u.m_lsys_krn_sys_devio.value = 0;       // value (unused for input)

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        // No priv_id → no CHECK_IO_PORT → allowed (C "goto doit")
        caller.priv_id = None;

        let pio = MockPortIo::new(0xAB);
        let priv_table = PrivTable::new();
        let result = dispatch_devio(&mut caller, &mut msg, &pio, &priv_table);
        assert_eq!(result, KcallResult::Ok(OK));
        // Result written to m_krn_lsys_sys_devio.value (offset 0, C reply)
        assert_eq!(unsafe { msg.m_u.m_krn_lsys_sys_devio.value }, 0xAB);
    }

    #[test]
    fn test_devio_output_word_no_check() {
        // DIO_OUTPUT_WORD = 0x022
        let mut msg = Message::default();
        msg.m_type = Syscall::Devio as i32;
        msg.m_u.m_lsys_krn_sys_devio.request = 0x022; // request
        msg.m_u.m_lsys_krn_sys_devio.port = 0x60;     // port (aligned for word)
        msg.m_u.m_lsys_krn_sys_devio.value = 0x1234;  // value to write

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        caller.priv_id = None;

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_devio(&mut caller, &mut msg, &pio, &priv_table);
        assert_eq!(result, KcallResult::Ok(OK));
        assert_eq!(*pio.last_write.borrow(), Some((0x60, 0x1234)));
    }

    #[test]
    fn test_devio_alignment_check() {
        // DIO_INPUT_WORD = 0x021, port=0x61 (not word-aligned)
        let mut msg = Message::default();
        msg.m_type = Syscall::Devio as i32;
        msg.m_u.m_lsys_krn_sys_devio.request = 0x021; // request
        msg.m_u.m_lsys_krn_sys_devio.port = 0x61;     // port (not word-aligned)
        msg.m_u.m_lsys_krn_sys_devio.value = 0;

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        caller.priv_id = None;

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_devio(&mut caller, &mut msg, &pio, &priv_table);
        // C: do_devio.c:60-65 — unaligned → EPERM
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_devio_check_io_port_allowed() {
        // DIO_INPUT_BYTE = 0x011, port=0x60 in range [0x60, 0x6F]
        let mut msg = Message::default();
        msg.m_type = Syscall::Devio as i32;
        msg.m_u.m_lsys_krn_sys_devio.request = 0x011;
        msg.m_u.m_lsys_krn_sys_devio.port = 0x60;
        msg.m_u.m_lsys_krn_sys_devio.value = 0;

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut priv_table = PrivTable::new();
        let priv_id: crate::kpriv::PrivId = 0;
        caller.priv_id = Some(priv_id);
        if let Some(priv_) = priv_table.get_mut(priv_id) {
            priv_.capability.s_flags = PrivFlagsBits::CHECK_IO_PORT;
            priv_.io.s_nr_io_range = 1;
            priv_.io.s_io_tab[0] = crate::kpriv::IoRange { base: 0x60, limit: 0x6F };
        }

        let pio = MockPortIo::new(0xFF);
        let result = dispatch_devio(&mut caller, &mut msg, &pio, &priv_table);
        assert_eq!(result, KcallResult::Ok(OK));
        assert_eq!(unsafe { msg.m_u.m_krn_lsys_sys_devio.value }, 0xFF);
    }

    #[test]
    fn test_devio_check_io_port_denied() {
        // DIO_INPUT_BYTE = 0x011, port=0x80 NOT in range [0x60, 0x6F]
        let mut msg = Message::default();
        msg.m_type = Syscall::Devio as i32;
        msg.m_u.m_lsys_krn_sys_devio.request = 0x011;
        msg.m_u.m_lsys_krn_sys_devio.port = 0x80;
        msg.m_u.m_lsys_krn_sys_devio.value = 0;

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut priv_table = PrivTable::new();
        let priv_id: crate::kpriv::PrivId = 0;
        caller.priv_id = Some(priv_id);
        if let Some(priv_) = priv_table.get_mut(priv_id) {
            priv_.capability.s_flags = PrivFlagsBits::CHECK_IO_PORT;
            priv_.io.s_nr_io_range = 1;
            priv_.io.s_io_tab[0] = crate::kpriv::IoRange { base: 0x60, limit: 0x6F };
        }

        let pio = MockPortIo::new(0);
        let result = dispatch_devio(&mut caller, &mut msg, &pio, &priv_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_devio_invalid_type() {
        // Invalid io_type mask
        let mut msg = Message::default();
        msg.m_type = Syscall::Devio as i32;
        msg.m_u.m_lsys_krn_sys_devio.request = 0x050; // invalid _DIO_TYPEMASK
        msg.m_u.m_lsys_krn_sys_devio.port = 0x60;
        msg.m_u.m_lsys_krn_sys_devio.value = 0;

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        caller.priv_id = None;

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_devio(&mut caller, &mut msg, &pio, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    // ── dispatch_iopenable tests ──

    #[test]
    fn test_iopenable_self_endpoint_resolves_to_caller() {
        // C: do_iopenable.c:24-25 — SELF → okendpt(caller->p_endpoint, &proc_nr)
        let mut proc_table = crate::proc_table::ProcessTable::new();
        // Set up a user process at slot 0 (nr=0, endpoint=Endpoint(0))
        let caller_ep = Endpoint::from_generation_slot(1, 0);
        let mut caller = KProcess::new(ProcNr(0), caller_ep);
        caller.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // Also mark the process in the table as not-free so endpoint_to_nr finds it
        {
            let proc = proc_table.get_mut(ProcNr(0)).unwrap();
            proc.p_endpoint = caller_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut msg = Message::default();
        msg.m_type = 0;
        msg.m_u.m_m1.m1i1 = Endpoint::SELF.0; // endpt = SELF

        let result = dispatch_iopenable(&mut caller, &msg, &mut proc_table);
        // Should succeed (not EINVAL) — SELF resolved to caller's endpoint
        assert_eq!(result, KcallResult::Ok(0));
        // IOPL enable is verified in minix-arch (x86_64::boot::tests::
        // enable_user_io_sets_iopl) — kernel layer no longer inspects
        // the arch-private cpu_context.psw field.
        let _target = proc_table.get(ProcNr(0)).unwrap();
    }

    #[test]
    fn test_iopenable_explicit_endpoint_sets_iopl() {
        // C: do_iopenable.c:26-28 — isokendpt + enable_iop
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 5);
        {
            let proc = proc_table.get_mut(ProcNr(5)).unwrap();
            proc.p_endpoint = target_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        let mut msg = Message::default();
        msg.m_type = 0;
        msg.m_u.m_m1.m1i1 = target_ep.0; // endpt = explicit endpoint

        let result = dispatch_iopenable(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(0));
        // IOPL enable verified in minix-arch layer (kernel layer no
        // longer reads the arch-private cpu_context).
        let _target = proc_table.get(ProcNr(5)).unwrap();
    }

    #[test]
    fn test_iopenable_invalid_endpoint_returns_einval() {
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));

        let mut msg = Message::default();
        msg.m_type = 0;
        msg.m_u.m_m1.m1i1 = 9999; // nonexistent endpoint

        let result = dispatch_iopenable(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_iopenable_kernel_process_returns_eperm() {
        // C: do_iopenable.c:29 — kernel processes denied
        // Kernel tasks have p_nr < 0
        let mut proc_table = crate::proc_table::ProcessTable::new();
        // Slot for kernel task at nr=-1 (index 0 in the task portion)
        // ProcessTable stores tasks at indices 0..NR_TASKS
        let kernel_ep = Endpoint::from_generation_slot(1, -1_i32);
        {
            let proc = proc_table.get_mut(ProcNr(-1)).unwrap();
            proc.p_endpoint = kernel_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        msg.m_u.m_m1.m1i1 = kernel_ep.0;

        let result = dispatch_iopenable(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_iopenable_iopl_bits_only_affects_bits_12_13() {
        // The kernel-layer contract is "enable user I/O". The arch layer
        // (x86_64) is responsible for setting RFLAGS.IOPL=3; the
        // bit-level assertion is in `x86_64::boot::tests::
        // enable_user_io_sets_iopl`. The kernel-layer test just verifies
        // the syscall succeeds.
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 3);
        {
            let proc = proc_table.get_mut(ProcNr(3)).unwrap();
            proc.p_endpoint = target_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        msg.m_u.m_m1.m1i1 = target_ep.0;

        let result = dispatch_iopenable(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(0));
    }

    // ── dispatch_sdevio tests ──

    /// Helper: set up a user process in the proc table for sdevio tests.
    fn setup_sdevio_proc(
        proc_table: &mut crate::proc_table::ProcessTable,
        slot: i32,
        generation: i32,
    ) -> Endpoint {
        let ep = Endpoint::from_generation_slot(generation, slot);
        let proc = proc_table.get_mut(ProcNr(slot)).unwrap();
        proc.p_endpoint = ep;
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        ep
    }

    #[test]
    fn test_sdevio_invalid_endpoint_returns_einval() {
        // C: do_sdevio.c:56 — isokendpt fails → EINVAL
        let proc_table = crate::proc_table::ProcessTable::new();
        let mut caller = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));

        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x011, // DIO_INPUT_BYTE (unsafe)
            vec_endpt: 9999, // nonexistent endpoint
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sdevio_kernel_target_returns_eperm() {
        // C: do_sdevio.c:60 — iskerneln(proc_nr) → EPERM
        let mut proc_table = crate::proc_table::ProcessTable::new();
        // Kernel task at slot -1
        let kernel_ep = Endpoint::from_generation_slot(1, -1_i32);
        {
            let proc = proc_table.get_mut(ProcNr(-1)).unwrap();
            proc.p_endpoint = kernel_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x011, // DIO_INPUT_BYTE (unsafe)
            vec_endpt: kernel_ep.0,
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_sdevio_unsafe_target_not_caller_returns_eperm() {
        // C: do_sdevio.c:84-90 — unsafe sdevio with target != caller → EPERM
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let _caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);
        let target_ep = setup_sdevio_proc(&mut proc_table, 5, 1);

        let mut caller = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x011, // DIO_INPUT_BYTE (unsafe, no _DIO_SAFE)
            vec_endpt: target_ep.0,
            port: 0x60,
            vec_size: 4,
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_sdevio_long_type_returns_einval() {
        // C: do_sdevio.c:140-152 — _DIO_LONG not supported in batch I/O
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);

        let mut caller = KProcess::new(ProcNr(0), caller_ep);
        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x031, // DIO_INPUT_LONG (unsafe)
            vec_endpt: Endpoint::SELF.0,
            port: 0x60,
            vec_size: 4,
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sdevio_unaligned_port_returns_eperm() {
        // C: do_sdevio.c:124-129 — port & (size-1) != 0 → EPERM
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);

        let mut caller = KProcess::new(ProcNr(0), caller_ep);
        caller.priv_id = None;
        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x021, // DIO_INPUT_WORD (unsafe)
            vec_endpt: Endpoint::SELF.0,
            port: 0x61, // not word-aligned
            vec_size: 4,
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_sdevio_check_io_port_denied() {
        // C: do_sdevio.c:102-122 — CHECK_IO_PORT with port out of range → EPERM
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);

        let mut caller = KProcess::new(ProcNr(0), caller_ep);
        let mut priv_table = PrivTable::new();
        let priv_id: crate::kpriv::PrivId = 0;
        caller.priv_id = Some(priv_id);
        if let Some(priv_) = priv_table.get_mut(priv_id) {
            priv_.capability.s_flags = PrivFlagsBits::CHECK_IO_PORT;
            priv_.io.s_nr_io_range = 1;
            priv_.io.s_io_tab[0] = crate::kpriv::IoRange { base: 0x60, limit: 0x6F };
        }

        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x011, // DIO_INPUT_BYTE (unsafe)
            vec_endpt: Endpoint::SELF.0,
            port: 0x80, // out of range [0x60, 0x6F]
            vec_size: 4,
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_sdevio_safe_path_no_grant_table_returns_eperm() {
        // Valid SAFE request passes all checks; verify_grant is called but
        // the granter has no priv_id (no grant table) → EPERM.
        // C: do_sdevio.c:65-93 — safe variant (verify_grant) wired.
        //
        // Uses _DIO_SAFE (0x100) to select the safe path. verify_grant
        // resolves the grant via the granter's privilege table; with
        // priv_id=None, it returns EPERM (grant.rs:386).
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);

        let mut caller = KProcess::new(ProcNr(0), caller_ep);
        caller.priv_id = None;
        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x111, // DIO_INPUT_BYTE | DIO_SAFE (safe path)
            vec_endpt: Endpoint::SELF.0,
            port: 0x60, // aligned for byte
            vec_size: 4,
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_sdevio_invalid_direction_returns_einval() {
        // C: do_sdevio.c:148-152 — direction other than INPUT/OUTPUT → EINVAL
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);

        let mut caller = KProcess::new(ProcNr(0), caller_ep);
        caller.priv_id = None;
        let mut msg = Message::default();
        msg.m_type = Syscall::Sdevio as i32;
        msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
            request: 0x010, // _DIO_BYTE with direction=0 (invalid)
            vec_endpt: Endpoint::SELF.0,
            port: 0x60,
            vec_size: 4,
            ..Default::default()
        };

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    // ── dispatch_readbios tests ──

    #[test]
    fn test_readbios_zero_size_returns_einval() {
        // size == 0 would underflow `limit = addr + size - 1`
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Readbios as i32;
        msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
            size: 0,
            addr: 0x100,
            buf: 0x1000,
            ..Default::default()
        };

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_readbios_outside_bios_range_returns_eperm() {
        // C: do_readbios.c:31-33 — neither BIOS_MEM nor UPPER_MEM range
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Readbios as i32;
        msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
            size: 16,
            addr: 0x10000, // between BIOS_MEM_END and BASE_MEM_TOP
            buf: 0x1000,
            ..Default::default()
        };

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    // NOTE: Valid-range readbios tests (BIOS_MEM and UPPER_MEM ranges) are
    // omitted because dispatch_readbios calls data_copy_vmcheck →
    // cross_space_copy::<CurrentDirectMap>, which dereferences Direct Map
    // addresses (0xFFFF_8000_0000_0000+) that are unmapped in host-side
    // unit tests → SIGSEGV.
    // Integration tests with QEMU + real page tables are required for the
    // copy path. The validation tests below cover all error paths.

    #[test]
    fn test_readbios_straddling_ranges_returns_eperm() {
        // Range must fit ENTIRELY within one region.
        // addr=0x4F0, size=32 → limit=0x50F, exceeds BIOS_MEM_END (0x4FF)
        // and is below BASE_MEM_TOP (0x90000) → EPERM
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Readbios as i32;
        msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
            size: 32,
            addr: 0x4F0,
            buf: 0x1000,
            ..Default::default()
        };

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_readbios_overflow_returns_einval() {
        // addr + size - 1 overflows u64
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Readbios as i32;
        msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
            size: 2,
            addr: u64::MAX,
            buf: 0x1000,
            ..Default::default()
        };

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    // ── dispatch_irqctl tests (with local IrqManager<MockIrqController>) ──

    /// Minimal no-op `InterruptController` for unit-testing `dispatch_irqctl`
    /// without real hardware. The validation paths under test return before
    /// any controller method is invoked; the SetPolicy success path calls
    /// `unmask` (a no-op here) when the first hook is installed.
    struct MockIrqController;

    impl InterruptController for MockIrqController {
        fn new(_desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
            MockIrqController
        }
        fn init(&mut self) {}
        fn mask(&mut self, _irq: IrqVector) {}
        fn unmask(&mut self, _irq: IrqVector) {}
        fn ack(&mut self, _irq: IrqVector) {}
        fn eoi(&mut self, _irq: IrqVector) {}
        fn mask_all(&mut self) {}
    }

    /// Build a SYS_IRQCTL message using the dedicated struct (not M1).
    fn build_irqctl_msg(request: i32, vector: i32, policy: i32, hook_id: i32) -> Message {
        let mut msg = Message::default();
        msg.m_type = Syscall::Irqctl as i32;
        msg.m_u.m_lsys_krn_sys_irqctl = minix_types::MessLsysKrnSysIrqctl {
            request,
            vector,
            policy,
            hook_id,
            _padding: [0; 40],
        };
        msg
    }

    #[test]
    fn test_dispatch_irqctl_rejects_unknown_request() {
        // C: do_irqctl.c:43 — unknown request → EINVAL.
        // Validation happens before any IrqManager access.
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_irqctl_msg(99, 0, 0, 0);
        let mut irq_mgr = IrqManager::new(MockIrqController);
        let priv_table = PrivTable::new();
        let result = dispatch_irqctl(&mut caller, &mut msg, &mut irq_mgr, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_irqctl_setpolicy_rejects_negative_irq() {
        // C: do_irqctl.c:55-56 — irq_vec < 0 → EINVAL.
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_irqctl_msg(
            IrqctlRequest::SetPolicy as i32,
            -1,
            0,
            0,
        );
        let mut irq_mgr = IrqManager::new(MockIrqController);
        let priv_table = PrivTable::new();
        let result = dispatch_irqctl(&mut caller, &mut msg, &mut irq_mgr, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_irqctl_setpolicy_rejects_too_high_irq() {
        // C: do_irqctl.c:55-56 — irq_vec >= NR_IRQ_VECTORS → EINVAL.
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        let mut msg = build_irqctl_msg(
            IrqctlRequest::SetPolicy as i32,
            9999,
            0,
            0,
        );
        let mut irq_mgr = IrqManager::new(MockIrqController);
        let priv_table = PrivTable::new();
        let result = dispatch_irqctl(&mut caller, &mut msg, &mut irq_mgr, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_irqctl_setpolicy_no_priv_returns_eperm() {
        // C: do_irqctl.c:58-76 — caller without an assigned privilege (priv_id
        // == None) cannot pass CHECK_IRQ → EPERM. Returned before any
        // IrqManager hook operation.
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        // priv_id left as None (KProcess::new default).
        let mut msg = build_irqctl_msg(
            IrqctlRequest::SetPolicy as i32,
            5, // valid vector
            0,
            0,
        );
        let mut irq_mgr = IrqManager::new(MockIrqController);
        let priv_table = PrivTable::new();
        let result = dispatch_irqctl(&mut caller, &mut msg, &mut irq_mgr, &priv_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_irqctl_setpolicy_writes_hook_id_to_dedicated_field() {
        // C: do_irqctl.c:82-108 — successful SETPOLICY installs a hook and
        // writes the 1-based hook_id back into the message.
        //
        // This test verifies the fix for the field-mapping bug: the reply
        // hook_id must land in `m_lsys_krn_sys_irqctl.hook_id` (offset 12),
        // NOT in `m_m1.m1p1` (offset 16, which is padding in the irqctl
        // struct layout).
        let mut caller = KProcess::new(ProcNr(0), Endpoint(100));
        caller.priv_id = Some(0); // PrivTable slot 0 has no CHECK_IRQ flag → all IRQs allowed.
        let mut msg = build_irqctl_msg(
            IrqctlRequest::SetPolicy as i32,
            5,   // valid vector
            0,   // policy (no REENABLE)
            0,   // notify_id (valid: 0..=31)
        );
        let mut irq_mgr = IrqManager::new(MockIrqController);
        let priv_table = PrivTable::new();
        let result = dispatch_irqctl(&mut caller, &mut msg, &mut irq_mgr, &priv_table);
        assert_eq!(result, KcallResult::Ok(OK));
        // The first installed hook gets 1-based id = 1.
        // Read back via the dedicated irqctl variant (not M1).
        msg.debug_check_m_type_any(&[Syscall::Irqctl as i32]);
        // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
        let hook_id = unsafe { msg.m_u.m_lsys_krn_sys_irqctl.hook_id };
        assert_eq!(hook_id, 1, "hook_id must be written to the dedicated irqctl field");
    }
}
