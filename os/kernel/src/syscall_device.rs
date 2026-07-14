//! Device I/O system calls: irqctl, devio, vdevio, iopenable.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_irqctl.c` — SYS_IRQCTL
//! - `do_devio.c` — SYS_DEVIO (x86-only)
//! - `do_vdevio.c` — SYS_VDEVIO (x86-only)
//! - `do_iopenable.c` — SYS_IOPENABLE (x86-only)
//!
//! # Design Decisions (19-syscall-device.md §3)
//!
//! - **D1**: `IrqctlRequest` enum for IRQ sub-requests
//! - **D2**: `trait PortIo` for architecture-specific I/O
//! - **D6**: x86-only calls return BadCall on other architectures

use minix_plat::{IrqPolicy, IrqVector, IrqNotifyId, NR_IRQ_VECTORS};
use minix_types::{Endpoint, Message, MessageM1, MessLsysKrnReadbios, MessLsysKrnSysSdevio};

use crate::irq_manager::IrqManager;
use crate::kpriv::{KPriv, PrivFlagsBits, PrivTable};
use crate::proc::KProcess;
use crate::syscall::KcallResult;
use minix_plat::InterruptController;

// ── Minix3 error codes ──

const OK: i32 = 0;
const EINVAL: i32 = 22;
const EPERM: i32 = 1;
const ENOSPC: i32 = 28;
const ENOSYS: i32 = 38;

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

/// Maximum VDEVIO buffer size. C: `VDEVIO_BUF_SIZE` — do_vdevio.c
pub const VDEVIO_BUF_SIZE: usize = 1024;

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
/// Design: Doc 19-syscall-device.md §3 D2.
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
    let m1 = msg_m1(msg);
    // C: do_irqctl.c:24-25 — extract parameters
    let request = m1.m1i1;       // m_lsys_krn_sys_irqctl.request
    let irq_vec = m1.m1i2;       // m_lsys_krn_sys_irqctl.vector
    let policy = m1.m1i3 as u32; // m_lsys_krn_sys_irqctl.policy
    let hook_id = m1.m1p1 as i32; // m_lsys_krn_sys_irqctl.hook_id

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
                    // Write 1-based hook_id back into the message
                    // SAFETY: We have &mut Message; writing to the m1 variant
                    // of the union is safe since we just read from it above.
                    msg.m_u.m_m1.m1p1 = new_hook_id as u64;
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
            if let Err(_) = irq_mgr.remove_hook_by_slot(slot_idx) {
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
    let m1 = msg_m1(msg);
    // C: do_devio.c:22-24 — extract parameters
    let request = m1.m1i1;     // m_lsys_krn_sys_devio.request
    let port = m1.m1i2 as u16; // m_lsys_krn_sys_devio.port
    let value = m1.m1p1 as u32; // m_lsys_krn_sys_devio.value

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
    if let Some(priv_) = caller_priv {
        if priv_.capability.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
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
    }
    // C: do_devio.c:33-36 — if no priv structure, goto doit (allow)
    // Rust: no priv → caller_priv is None → skip check (same as C "goto doit")

    // C: do_devio.c:60-65 — alignment check
    if port % size as u16 != 0 {
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
            // SAFETY: We have &mut Message; writing to the m1 variant
            // of the union is safe since we just read from it above.
            msg.m_u.m_m1.m1p1 = result as u64;
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
/// Perform a batch of I/O port operations.
///
/// # Implementation Status
///
/// Parameter extraction and type/direction parsing are implemented.
/// The actual batch I/O execution requires `data_copy_vmcheck` to
/// copy the (port, value) pair array from user space, which is
/// deferred until the cross-space copy subsystem is available.
/// Permission checks (CHECK_IO_PORT) are also deferred for the
/// batch case since they need to iterate over the user-space array.
pub fn dispatch_vdevio<PI: PortIo>(
    _caller: &mut KProcess,
    msg: &Message,
    _port_io: &PI,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_vdevio.c:44-52 — extract parameters
    let request = m1.m1i1;     // m_lsys_krn_sys_vdevio.request
    let _vec_addr = m1.m1p1;   // m_lsys_krn_sys_vdevio.vec_addr
    let vec_size = m1.m1i2;    // m_lsys_krn_sys_vdevio.vec_size

    // C: do_vdevio.c:54-72 — parse type/direction, validate size
    let io_type = request & 0x0F0;   // _DIO_TYPEMASK
    let io_dir = request & 0x00F;    // _DIO_DIRMASK

    let _size = match IoSize::from_request_mask(io_type) {
        Some(s) => s,
        None => return KcallResult::Ok(EINVAL),
    };

    let _dir = match IoDirection::from_request_mask(io_dir) {
        Some(d) => d,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_vdevio.c:56-58 — validate vec_size
    if vec_size <= 0 || vec_size as usize > VDEVIO_BUF_SIZE {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_vdevio.c:74-77 — copy (port,value) pairs from user
    // DEFERRED: requires data_copy_vmcheck (cross-space copy subsystem)

    // C: do_vdevio.c:79-100 — batch permission check
    // DEFERRED: requires the copied (port, value) array

    // C: do_vdevio.c:102-139 — batch I/O execution
    // DEFERRED: requires the copied (port, value) array

    // C: do_vdevio.c:141-146 — copy back results for input
    // DEFERRED: requires data_copy_vmcheck

    // Parameter validation passed but batch I/O is not yet implemented.
    // Return ENOSYS (function not implemented) rather than OK — returning OK
    // without performing the actual I/O would be a semantic drift from C,
    // where do_vdevio either executes the batch or returns an error.
    // Same pattern as misc.rs dispatch_getinfo for unimplemented sub-requests.
    KcallResult::Ok(ENOSYS)
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
/// (06-design-final.md §3.5).
///
/// For already-running processes, the trap frame on the kernel stack
/// also needs updating — this is deferred until the scheduler/context-switch
/// path provides arch-layer access to the saved exception frame.
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
    // (06-design-final.md §3.5): the kernel only knows the OS concept
    // "enable user I/O"; the arch decides how to encode it (x86-64:
    // RFLAGS |= 0x3000; aarch64/riscv64: no-op).
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.enable_user_io();
    }

    // DEFERRED: For already-running processes, also update the RFLAGS
    // field in the exception frame on the process's kernel stack.
    // This requires arch-layer support for accessing the saved
    // exception frame, which will be added with the scheduler.

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
/// Parameter extraction, endpoint validation, type/direction parsing,
/// permission check (CHECK_IO_PORT), and alignment check are implemented.
/// The actual batch I/O transfer (`phys_insb`/`phys_outsb`/`phys_insw`/
/// `phys_outsw`) requires:
/// 1. `verify_grant` for safe variants (grant → physical address mapping)
/// 2. `switch_address_space` + `virtual_copy_vmcheck` for unsafe variants
///
/// These are deferred until the cross-space copy subsystem is available.
/// Returning ENOSYS (rather than OK) avoids semantic drift: C's `do_sdevio`
/// either performs the batch I/O or returns an error — never silently
/// succeeds without doing the work.
pub fn dispatch_sdevio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &Message,
    _port_io: &PI,
    priv_table: &PrivTable,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    // C: do_sdevio.c:42-46 — extract parameters via dedicated struct
    // SAFETY: `m_type` has been validated by the dispatcher to be SYS_SDEVIO.
    // Using the dedicated `MessLsysKrnSysSdevio` overlay ensures correct
    // field offsets (the generic `m1` overlay would misparse fields).
    let sdevio = unsafe { msg.m_u.m_lsys_krn_sys_sdevio };
    let request = sdevio.request;
    let port = sdevio.port;
    let vec_endpt = sdevio.vec_endpt;
    let _vec_addr = sdevio.vec_addr;
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
    // DEFERRED: requires verify_grant (grant → physical address mapping).
    // For unsafe variants, C requires the target to be the caller itself
    // (do_sdevio.c:84-90). We enforce this check here.
    let is_safe = (request & DIO_SAFEMASK) == DIO_SAFE;
    if !is_safe {
        // C: do_sdevio.c:84-90 — unsafe sdevio only allowed if target == caller
        if target_nr != caller.p_nr {
            return KcallResult::Ok(EPERM);
        }
    }
    // DEFERRED: safe variant verify_grant + address space switch

    // C: do_sdevio.c:95-100 — determine element size
    // SDEVIO only supports byte and word (long is not supported).
    let size = match req_type {
        0x010 => 1usize,  // _DIO_BYTE
        0x020 => 2usize,  // _DIO_WORD
        _ => return KcallResult::Ok(EINVAL),  // _DIO_LONG not supported
    };

    // C: do_sdevio.c:102-122 — CHECK_IO_PORT permission
    let caller_priv = caller.priv_id.and_then(|pid| priv_table.get(pid));
    if let Some(priv_) = caller_priv {
        if priv_.capability.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
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
    }

    // C: do_sdevio.c:124-129 — alignment check
    if port % (size as i64) != 0 {
        return KcallResult::Ok(EPERM);
    }

    // C: do_sdevio.c:131-153 — perform batch I/O
    // DEFERRED: requires switch_address_space + phys_insb/phys_outsb/
    // phys_insw/phys_outsw (cross-space batch I/O primitives).
    //
    // Validate direction: C only accepts _DIO_INPUT (0x001) and _DIO_OUTPUT
    // (0x002); any other value returns EINVAL (do_sdevio.c:148-152).
    match req_dir {
        0x001 | 0x002 => {}
        _ => return KcallResult::Ok(EINVAL),
    }

    // Validate vec_size: C uses `vir_bytes count` and passes it to phys_*;
    // a zero count would be a no-op, but we still require the cross-space
    // copy primitive to proceed. Return ENOSYS until that is available.
    let _ = vec_size;
    KcallResult::Ok(ENOSYS)
}

// ── SYS_READBIOS ──

/// BIOS memory range constants. C: memory.h (i386)
///
/// `do_readbios` allows reading from two BIOS memory regions:
/// 1. `BIOS_MEM_BEGIN..=BIOS_MEM_END` (0x00000..=0x004FF) — IVT + BIOS data
/// 2. `BASE_MEM_TOP..=UPPER_MEM_END` (0x090000..=0x0FFFFF) — upper memory area
const BIOS_MEM_BEGIN: u64 = 0x00000;
const BIOS_MEM_END: u64 = 0x004FF;
const BASE_MEM_TOP: u64 = 0x090000;
const UPPER_MEM_END: u64 = 0x0FFFFF;

/// Dispatch SYS_READBIOS (x86-only).
///
/// C: `do_readbios()` — arch/i386/do_readbios.c
///
/// Copy data from the BIOS memory area to a user-space buffer.
///
/// # Implementation Status
///
/// Parameter extraction and BIOS memory range validation are implemented.
/// The actual data copy (`virtual_copy_vmcheck`) is deferred until the
/// cross-space copy subsystem is available. Returning ENOSYS (rather than
/// OK) avoids semantic drift: C's `do_readbios` either copies the data or
/// returns an error.
pub fn dispatch_readbios(
    _caller: &mut KProcess,
    msg: &Message,
) -> KcallResult {
    // C: do_readbios.c:19-22 — extract parameters via dedicated struct
    // SAFETY: `m_type` has been validated by the dispatcher to be SYS_READBIOS.
    // Using the dedicated `MessLsysKrnReadbios` overlay ensures correct
    // field offsets (the generic `m1` overlay would misparse fields).
    let readbios = unsafe { msg.m_u.m_lsys_krn_readbios };
    let size = readbios.size;
    let addr = readbios.addr;
    let _buf = readbios.buf;

    // C: do_readbios.c:26 — limit = addr + size - 1
    // Guard against size == 0 (would underflow) and overflow.
    if size == 0 {
        return KcallResult::Ok(EINVAL);
    }
    let limit = match addr.checked_add(size - 1) {
        Some(l) => l,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_readbios.c:31-33 — BIOS memory range check
    // USERRANGE(a, b) = SUBRANGE(src.offset, limit, a, b)
    //                 = VINRANGE(src.offset, a, b) && VINRANGE(limit, a, b)
    // The request is allowed if it fits entirely within EITHER:
    //   (BIOS_MEM_BEGIN..=BIOS_MEM_END) OR (BASE_MEM_TOP..=UPPER_MEM_END)
    let in_bios = addr >= BIOS_MEM_BEGIN && limit <= BIOS_MEM_END;
    let in_upper = addr >= BASE_MEM_TOP && limit <= UPPER_MEM_END;
    if !in_bios && !in_upper {
        return KcallResult::Ok(EPERM);
    }

    // C: do_readbios.c:35 — virtual_copy_vmcheck(caller, &src, &dst, size)
    // DEFERRED: requires virtual_copy_vmcheck (cross-space copy with VM
    // assistance for fault handling). The src is physical (NONE endpoint),
    // dst is the caller's buffer.
    KcallResult::Ok(ENOSYS)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;
    use crate::proc::RtsFlagsBits;

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
        msg.m_type = 0;
        // SAFETY: writing to m_m1 variant of the union for test setup
        unsafe {
            msg.m_u.m_m1.m1i1 = 0x011; // request
            msg.m_u.m_m1.m1i2 = 0x60;  // port (aligned for byte)
            msg.m_u.m_m1.m1p1 = 0;     // value (unused for input)
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
        // No priv_id → no CHECK_IO_PORT → allowed (C "goto doit")
        caller.priv_id = None;

        let pio = MockPortIo::new(0xAB);
        let priv_table = PrivTable::new();
        let result = dispatch_devio(&mut caller, &mut msg, &pio, &priv_table);
        assert_eq!(result, KcallResult::Ok(OK));
        // Result written to m1p1
        assert_eq!(unsafe { msg.m_u.m_m1.m1p1 }, 0xAB);
    }

    #[test]
    fn test_devio_output_word_no_check() {
        // DIO_OUTPUT_WORD = 0x022
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = 0x022; // request
            msg.m_u.m_m1.m1i2 = 0x60;  // port (aligned for word)
            msg.m_u.m_m1.m1p1 = 0x1234; // value to write
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
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
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = 0x021; // request
            msg.m_u.m_m1.m1i2 = 0x61;  // port (not word-aligned)
            msg.m_u.m_m1.m1p1 = 0;
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
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
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = 0x011;
            msg.m_u.m_m1.m1i2 = 0x60;
            msg.m_u.m_m1.m1p1 = 0;
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
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
        assert_eq!(unsafe { msg.m_u.m_m1.m1p1 }, 0xFF);
    }

    #[test]
    fn test_devio_check_io_port_denied() {
        // DIO_INPUT_BYTE = 0x011, port=0x80 NOT in range [0x60, 0x6F]
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = 0x011;
            msg.m_u.m_m1.m1i2 = 0x80;
            msg.m_u.m_m1.m1p1 = 0;
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
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
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = 0x050; // invalid _DIO_TYPEMASK
            msg.m_u.m_m1.m1i2 = 0x60;
            msg.m_u.m_m1.m1p1 = 0;
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
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
        let mut caller = KProcess::new(0_i32, caller_ep);
        caller.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        // Also mark the process in the table as not-free so endpoint_to_nr finds it
        {
            let proc = proc_table.get_mut(0_i32).unwrap();
            proc.p_endpoint = caller_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = Endpoint::SELF.0; // endpt = SELF
        }

        let result = dispatch_iopenable(&mut caller, &msg, &mut proc_table);
        // Should succeed (not EINVAL) — SELF resolved to caller's endpoint
        assert_eq!(result, KcallResult::Ok(0));
        // IOPL enable is verified in minix-arch (x86_64::boot::tests::
        // enable_user_io_sets_iopl) — kernel layer no longer inspects
        // the arch-private cpu_context.psw field.
        let _target = proc_table.get(0_i32).unwrap();
    }

    #[test]
    fn test_iopenable_explicit_endpoint_sets_iopl() {
        // C: do_iopenable.c:26-28 — isokendpt + enable_iop
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let target_ep = Endpoint::from_generation_slot(1, 5);
        {
            let proc = proc_table.get_mut(5_i32).unwrap();
            proc.p_endpoint = target_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(0_i32, Endpoint::from_generation_slot(1, 0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = target_ep.0; // endpt = explicit endpoint
        }

        let result = dispatch_iopenable(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(0));
        // IOPL enable verified in minix-arch layer (kernel layer no
        // longer reads the arch-private cpu_context).
        let _target = proc_table.get(5_i32).unwrap();
    }

    #[test]
    fn test_iopenable_invalid_endpoint_returns_einval() {
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let mut caller = KProcess::new(0_i32, Endpoint(0));

        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = 9999; // nonexistent endpoint
        }

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
            let proc = proc_table.get_mut(-1_i32).unwrap();
            proc.p_endpoint = kernel_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = kernel_ep.0;
        }

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
            let proc = proc_table.get_mut(3_i32).unwrap();
            proc.p_endpoint = target_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_m1.m1i1 = target_ep.0;
        }

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
        let proc = proc_table.get_mut(slot).unwrap();
        proc.p_endpoint = ep;
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        ep
    }

    #[test]
    fn test_sdevio_invalid_endpoint_returns_einval() {
        // C: do_sdevio.c:56 — isokendpt fails → EINVAL
        let proc_table = crate::proc_table::ProcessTable::new();
        let mut caller = KProcess::new(0_i32, Endpoint::from_generation_slot(1, 0));

        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x011, // DIO_INPUT_BYTE (unsafe)
                vec_endpt: 9999, // nonexistent endpoint
                ..Default::default()
            };
        }

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
            let proc = proc_table.get_mut(-1_i32).unwrap();
            proc.p_endpoint = kernel_ep;
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }

        let mut caller = KProcess::new(0_i32, Endpoint::from_generation_slot(1, 0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x011, // DIO_INPUT_BYTE (unsafe)
                vec_endpt: kernel_ep.0,
                ..Default::default()
            };
        }

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

        let mut caller = KProcess::new(0_i32, Endpoint::from_generation_slot(1, 0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x011, // DIO_INPUT_BYTE (unsafe, no _DIO_SAFE)
                vec_endpt: target_ep.0,
                port: 0x60,
                vec_size: 4,
                ..Default::default()
            };
        }

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

        let mut caller = KProcess::new(0_i32, caller_ep);
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x031, // DIO_INPUT_LONG (unsafe)
                vec_endpt: Endpoint::SELF.0,
                port: 0x60,
                vec_size: 4,
                ..Default::default()
            };
        }

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

        let mut caller = KProcess::new(0_i32, caller_ep);
        caller.priv_id = None;
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x021, // DIO_INPUT_WORD (unsafe)
                vec_endpt: Endpoint::SELF.0,
                port: 0x61, // not word-aligned
                vec_size: 4,
                ..Default::default()
            };
        }

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

        let mut caller = KProcess::new(0_i32, caller_ep);
        let mut priv_table = PrivTable::new();
        let priv_id: crate::kpriv::PrivId = 0;
        caller.priv_id = Some(priv_id);
        if let Some(priv_) = priv_table.get_mut(priv_id) {
            priv_.capability.s_flags = PrivFlagsBits::CHECK_IO_PORT;
            priv_.io.s_nr_io_range = 1;
            priv_.io.s_io_tab[0] = crate::kpriv::IoRange { base: 0x60, limit: 0x6F };
        }

        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x011, // DIO_INPUT_BYTE (unsafe)
                vec_endpt: Endpoint::SELF.0,
                port: 0x80, // out of range [0x60, 0x6F]
                vec_size: 4,
                ..Default::default()
            };
        }

        let pio = MockPortIo::new(0);
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_sdevio_valid_returns_enosys() {
        // Valid parameters pass all checks; actual I/O deferred → ENOSYS.
        // C: do_sdevio.c:131-153 — batch I/O (deferred)
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);

        let mut caller = KProcess::new(0_i32, caller_ep);
        caller.priv_id = None;
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x011, // DIO_INPUT_BYTE (unsafe)
                vec_endpt: Endpoint::SELF.0,
                port: 0x60, // aligned for byte
                vec_size: 4,
                ..Default::default()
            };
        }

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_sdevio_invalid_direction_returns_einval() {
        // C: do_sdevio.c:148-152 — direction other than INPUT/OUTPUT → EINVAL
        let mut proc_table = crate::proc_table::ProcessTable::new();
        let caller_ep = setup_sdevio_proc(&mut proc_table, 0, 1);

        let mut caller = KProcess::new(0_i32, caller_ep);
        caller.priv_id = None;
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_sys_sdevio = MessLsysKrnSysSdevio {
                request: 0x010, // _DIO_BYTE with direction=0 (invalid)
                vec_endpt: Endpoint::SELF.0,
                port: 0x60,
                vec_size: 4,
                ..Default::default()
            };
        }

        let pio = MockPortIo::new(0);
        let priv_table = PrivTable::new();
        let result = dispatch_sdevio(&mut caller, &msg, &pio, &priv_table, &proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    // ── dispatch_readbios tests ──

    #[test]
    fn test_readbios_zero_size_returns_einval() {
        // size == 0 would underflow `limit = addr + size - 1`
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
                size: 0,
                addr: 0x100,
                buf: 0x1000,
                ..Default::default()
            };
        }

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_readbios_outside_bios_range_returns_eperm() {
        // C: do_readbios.c:31-33 — neither BIOS_MEM nor UPPER_MEM range
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
                size: 16,
                addr: 0x10000, // between BIOS_MEM_END and BASE_MEM_TOP
                buf: 0x1000,
                ..Default::default()
            };
        }

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_readbios_in_bios_mem_range_returns_enosys() {
        // C: do_readbios.c:31 — USERRANGE(BIOS_MEM_BEGIN, BIOS_MEM_END) passes
        // addr=0x100, size=16 → limit=0x10F, within [0x0, 0x4FF]
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
                size: 16,
                addr: 0x100,
                buf: 0x1000,
                ..Default::default()
            };
        }

        let result = dispatch_readbios(&mut caller, &msg);
        // Actual copy deferred → ENOSYS
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_readbios_in_upper_mem_range_returns_enosys() {
        // C: do_readbios.c:32 — USERRANGE(BASE_MEM_TOP, UPPER_MEM_END) passes
        // addr=0x0F0000, size=16 → limit=0x0F000F, within [0x90000, 0xFFFFF]
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
                size: 16,
                addr: 0x0F0000,
                buf: 0x1000,
                ..Default::default()
            };
        }

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(ENOSYS));
    }

    #[test]
    fn test_readbios_straddling_ranges_returns_eperm() {
        // Range must fit ENTIRELY within one region.
        // addr=0x4F0, size=32 → limit=0x50F, exceeds BIOS_MEM_END (0x4FF)
        // and is below BASE_MEM_TOP (0x90000) → EPERM
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
                size: 32,
                addr: 0x4F0,
                buf: 0x1000,
                ..Default::default()
            };
        }

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_readbios_overflow_returns_einval() {
        // addr + size - 1 overflows u64
        let mut caller = KProcess::new(0_i32, Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = 0;
        unsafe {
            msg.m_u.m_lsys_krn_readbios = MessLsysKrnReadbios {
                size: 2,
                addr: u64::MAX,
                buf: 0x1000,
                ..Default::default()
            };
        }

        let result = dispatch_readbios(&mut caller, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }
}
