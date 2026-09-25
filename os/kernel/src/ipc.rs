//! Kernel IPC core module.
//!
//! Implements the six IPC primitives (SEND, RECEIVE, SENDREC, NOTIFY, SENDNB, SENDA)
//! and supporting mechanisms (deadlock detection, sender queues, delayed delivery).
//!
//! **Zero-heap contract**: see `lib.rs` for the kernel-wide contract. This
//! module allocates nothing at runtime — sender queues use an intrusive
//! FIFO through the process-table slots (caller_q_head/tail in target slot,
//! send_q_link in sender slot).
//!
//! # Module Organization
//!
//! - Types: `IpcCall`, `IpcOutcome`, `IpcError`, `SendFlags`,
//!   `IpcEngine`, `DeadlockCycle`, `UserCopy`, `DeliverResult`
//! - Sender wait queue: free functions `caller_q_push` / `caller_q_find`
//!   (shared walk, `accept` predicate plugs in at C's CANRECEIVE position) /
//!   `caller_q_remove` / `caller_q_remove_by_nr` (intrusive FIFO through
//!   the process-table slots — no heap)
//! - SENDA flags: `AMF_*` constants
//!
//! Design decisions are documented in `12-ipc-core.md` §3.
//! C source: `minix3/minix/kernel/proc.c:263-294, 479-597, 599-698, 703-768,
//! 870-962, 967-1117, 1122-1167, 1200-1326, 1331-1346`.

use minix_types::{Endpoint, Message, MessNotify, VirBytes};
#[cfg(not(test))]
use minix_arch::PteWalkArch;
use crate::proc::{KProcess, ProcNr, RtsFlagsBits, MiscFlagsBits, NONE_PROC_NR, proc_nr};
use crate::proc_table::PROC_TABLE_SIZE;
use crate::kpriv::{KPriv, PrivTable};
use crate::errno::{OK, EINVAL, EDEADSRCDST, ECALLDENIED};

/// Notification message type. C: `#define NOTIFY_MESSAGE 0x1000` — com.h:90.
///
/// `BuildNotifyMessage` sets `m_type = NOTIFY_MESSAGE` so receivers can
/// distinguish notifications from regular IPC replies. The sender's
/// endpoint is in `m_source` (set by the caller of `BuildNotifyMessage`).
pub(crate) const NOTIFY_MESSAGE: i32 = 0x1000;

// ── IPC status encoding ──
//
// C: `minix3/minix/include/minix/ipcconst.h:21-35` — macros for IPC status
// code manipulation. The IPC status register stores metadata about the
// last IPC delivery (call type + flags) so user-space libraries can
// determine how to reply (e.g., SENDREC needs a reply, NOTIFY does not).

/// Bit shift for the call-type field in the IPC status register.
/// C: `IPC_STATUS_CALL_SHIFT` — ipcconst.h:21
pub const IPC_STATUS_CALL_SHIFT: u32 = 0;

/// Mask for the call-type field (6 bits, values 0-63).
/// C: `IPC_STATUS_CALL_MASK` — ipcconst.h:22
pub const IPC_STATUS_CALL_MASK: u32 = 0x3F;

/// Bit shift for the flags field in the IPC status register.
/// C: `IPC_STATUS_FLAGS_SHIFT` — ipcconst.h:32
pub const IPC_STATUS_FLAGS_SHIFT: u32 = 16;

/// Flag: message originated from the kernel on behalf of a process.
/// Trusted message — user-space must never reply to the sender.
/// C: `IPC_FLG_MSG_FROM_KERNEL` — ipcconst.h:28
pub const IPC_FLG_MSG_FROM_KERNEL: u32 = 1;

/// Encode an `IpcCall` value into the IPC status call field.
/// C: `IPC_STATUS_CALL_TO(call)` — ipcconst.h:25-26
#[inline]
pub const fn ipc_status_call_to(call: IpcCall) -> u64 {
    (call as u64 & IPC_STATUS_CALL_MASK as u64) << IPC_STATUS_CALL_SHIFT
}

/// Encode flags into the IPC status flags field.
/// C: `IPC_STATUS_FLAGS(flags)` — ipcconst.h:33
#[inline]
pub const fn ipc_status_flags(flags: u32) -> u64 {
    (flags as u64) << IPC_STATUS_FLAGS_SHIFT
}

// ── IPC call types ──

/// IPC primitive type. C: `call_nr` in `do_ipc()` — proc.c:599.
///
/// Values align with `minix3/minix/include/minix/ipcconst.h:7-13`:
/// `SEND=1, RECEIVE=2, SENDREC=3, NOTIFY=4, SENDNB=5, MINIX_KERNINFO=6,
/// SENDA=16`.
///
/// `#[repr(u8)]` keeps the encoding compatible with C's `int call_nr` for
/// the values currently defined (1..=16). `MINIX_KERNINFO` (6) is not an
/// IPC primitive — C dispatches it inside the same `do_ipc` switch
/// (proc.c:685-693) and returns the kernel info page address through the
/// secondary IPC return channel instead of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpcCall {
    /// Blocking send. C: `SEND=1` (ipcconst.h:7)
    Send = 1,
    /// Blocking receive. C: `RECEIVE=2` (ipcconst.h:8)
    Receive = 2,
    /// Atomic send + receive. C: `SENDREC=3` (ipcconst.h:9)
    SendRec = 3,
    /// Asynchronous notification. C: `NOTIFY=4` (ipcconst.h:10)
    Notify = 4,
    /// Non-blocking send. C: `SENDNB=5` (ipcconst.h:11)
    SendNb = 5,
    /// Kernel info page query. C: `MINIX_KERNINFO=6` (ipcconst.h:12,
    /// handled at proc.c:685-693). Returns the user-mapped
    /// `minix_kerninfo` page address via the secondary IPC return channel
    /// (x86-64: saved RBX); the caller's message buffer is untouched.
    KernInfo = 6,
    /// Batch async send. C: `SENDA=16` (ipcconst.h:13)
    SendA = 16,
}

impl IpcCall {
    /// Decode from raw `call_nr`. Returns `None` for invalid / unsupported
    /// values. C: `do_ipc` default branch returns `EBADCALL` — caller maps
    /// `None` to `IpcError::BadCall`.
    pub fn from_raw(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::Send),
            2 => Some(Self::Receive),
            3 => Some(Self::SendRec),
            4 => Some(Self::Notify),
            5 => Some(Self::SendNb),
            6 => Some(Self::KernInfo),
            16 => Some(Self::SendA),
            _ => None,
        }
    }
}

/// IPC operation outcome. Replaces C's errno return pattern.
///
/// Design decision §3.1 / AT-1: distinguish "blocked (normal)" from
/// "error (exception)". C returns a single `OK` for both "message
/// delivered" and "caller is now blocked (RTS_SENDING set)" — the
/// dispatcher infers which from side effects. Rust makes the two states
/// explicit so `switch_to_user` can match without inspecting RTS flags.
///
/// Mapping:
/// - C `OK` + (dst woken or caller enqueued) → `Delivered` or `Blocked`
/// - C `ELOCKED`/`ENOTREADY`/`EDEADSRCDST`/`EFAULT`/`ECALLDENIED`/`ETRAPDENIED`
///   → `Error(IpcError)`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcOutcome {
    /// Message delivered to target (or notification recorded in bitmap).
    /// C: `return OK` from `mini_send`/`mini_receive`/`mini_notify` when
    /// the destination was in RECEIVE and the message was copied to
    /// `p_delivermsg`.
    Delivered,
    /// Caller is now blocked (`RTS_SENDING` or `RTS_RECEIVING` set).
    /// C: implicit — `mini_send` returns `OK` but caller is enqueued on
    /// `p_caller_q`. Rust surfaces this as a distinct state so the
    /// scheduler knows to switch without re-checking RTS flags.
    Blocked,
    /// IPC failed with error. The caller remains runnable.
    Error(IpcError),
}

impl IpcOutcome {
    /// Returns `true` if the outcome is `Delivered`.
    pub fn is_delivered(self) -> bool { matches!(self, Self::Delivered) }

    /// Returns `true` if the outcome is `Blocked`.
    pub fn is_blocked(self) -> bool { matches!(self, Self::Blocked) }

    /// Returns `Some(err)` if the outcome is `Error(err)`, else `None`.
    pub fn err(self) -> Option<IpcError> {
        match self { Self::Error(e) => Some(e), _ => None }
    }
}

/// IPC error codes. C: errno values returned by `mini_*` functions.
///
/// Each variant cites the C errno and the source location where it is
/// produced. Values are not encoded as the C errno integers (negative);
/// the kernel↔user boundary converts to `errno` at the syscall return
/// path. See `12-ipc-core.md` §3.5 for the unification rationale.
///
/// Relationship with `minix_types::ipc::IpcError`: the kernel crate is
/// the authoritative definition. The user-space mirror in
/// `os/libs/minix-types/src/ipc/ipc_error.rs` is a separate, smaller
/// enum used by user-space consumers that do not need to distinguish
/// kernel-internal error subtypes. A future refactor should unify them
/// via a shared `minix-ipc` crate (tracked as design follow-up, not a
/// P0 because the two types are not currently mixed at any API boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    /// Deadlock detected. C: `ELOCKED` — proc.c:931
    Deadlock,
    /// Source or destination endpoint invalid. C: `EDEADSRCDST` — proc.c:889
    DeadSrcDst,
    /// Non-blocking send target not ready. C: `ENOTREADY` — proc.c:926
    NotReady,
    /// Invalid IPC call number. C: `EBADCALL` — proc.c:95
    BadCall,
    /// Message copy failed (page fault). C: `EFAULT` — proc.c:902, 937
    Fault,
    /// IPC target not in whitelist. C: `ECALLDENIED` — `do_sync_ipc`
    CallDenied,
    /// System call trap not permitted. C: `ETRAPDENIED` — `do_sync_ipc`
    TrapDenied,
    /// Caller lacks SYS_PROC privilege. C: `EPERM` — `mini_senda`
    /// (proc.c:1336-1339)
    Permission,
    /// Bad argument value. C: `EINVAL` — proc.c:508 (`ANY` passed with a
    /// non-RECEIVE call — ANY is a receive-only wildcard).
    Invalid,
}

// IPC send flags. C: `minix3/minix/kernel/ipc.h:11-12`.
//
// Design decision §3.4 / AT-4: `bitflags!` macro (not bare `u32`).
//
// # P0 FIX (FIX-1 / FIX-2)
//
// Previous code defined `NON_BLOCKING=0x01` and `FROM_KERNEL=0x02`,
// both wrong. The correct values aligned with C source are
// `NON_BLOCKING=0x0080` and `FROM_KERNEL=0x0100`.
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SendFlags: u32 {
        /// Non-blocking mode. C: `NON_BLOCKING=0x0080` — ipc.h:11
        const NON_BLOCKING = 0x0080;
        /// Message from kernel. C: `FROM_KERNEL=0x0100` — ipc.h:12
        const FROM_KERNEL = 0x0100;
        /// Internal: this send originates from SENDA (batch async send).
        /// Not a C flag — Rust uses it to set the correct IPC status call
        /// type (SENDA instead of SEND) when `send` is reused by `senda`.
        /// C's `mini_senda` has its own delivery path; Rust reuses `send`.
        const SENDA = 0x0001;
    }
}

impl SendFlags {
    /// Empty flags (blocking send from user space).
    pub const NONE: Self = Self::empty();
}

/// Deadlock cycle descriptor.
///
/// C: `deadlock()` — proc.c:703-768. Returns `Some` when a cyclic
/// dependency is found along the `P_BLOCKEDON` chain.
///
/// `direction` records which IPC call type produced the cycle so the
/// caller can apply the 2-cycle SEND↔RECEIVE exception (which is NOT a
/// deadlock — it is the normal request/reply pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockCycle {
    /// Process numbers (in walk order) forming the cycle, starting at the caller.
    /// Maximum chain length is bounded by PROC_TABLE_SIZE.
    pub chain: [ProcNr; PROC_TABLE_SIZE],
    /// Number of valid entries in `chain`.
    pub chain_len: usize,
    /// Direction that produced the cycle. C: corresponds to `function`
    /// passed to `deadlock()` (SEND or RECEIVE).
    pub direction: DeadlockDirection,
    /// Number of processes in the cycle. C: `group_size` return value.
    /// `2` triggers the SEND↔RECEIVE check at the caller.
    pub group_size: usize,
}

/// Deadlock direction.
///
/// Kept distinct from `IpcCall` to prevent the deadlock detector from
/// being semantically coupled with the IPC dispatcher: `IpcCall` is the
/// user-facing API call number, `DeadlockDirection` is the engine's
/// detected wait direction. The two are 1:1 today but the type boundary
/// keeps future extensions (e.g. a third wait direction) local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockDirection {
    /// SEND-direction cycle (caller is SENDING, waiting for receiver).
    /// C: `deadlock(SEND, ...)` path.
    Send,
    /// RECEIVE-direction cycle (caller is RECEIVING, waiting for sender).
    /// C: `deadlock(RECEIVE, ...)` path.
    Receive,
}

// ── User-space copy abstraction (FIX-10 / AT-8) ──

/// User-space memory copy abstraction.
///
/// Design decision §3.9 / AT-8: trait + arch implementation. C's
/// `copy_msg_from_user` / `copy_msg_to_user` call `virtual_copy`
/// directly, which violates the hardware abstraction principle (page
/// permission checks are arch-specific). The kernel IPC layer depends
/// only on this trait; arch crates provide the concrete implementation.
///
/// The trait is used by `IpcEngine::send` / `receive` / `deliver_message`
/// / `senda` to abstract away user-space page handling. Arch
/// implementations live in `os/arch/src/*/user_copy.rs` (to be added).
pub trait UserCopy {
    /// Copy a `Message` from user space. C: `copy_msg_from_user` —
    /// proc.c:901, 936. Returns `Err(CopyError::PageFault)` on page
    /// fault, `Err(CopyError::OutOfBounds)` if the address is outside
    /// user space.
    fn copy_msg_from_user(&self, src: VirBytes) -> Result<Message, CopyError>;

    /// Copy a `Message` to user space. C: `copy_msg_to_user` — proc.c:185.
    /// Same error semantics as `copy_msg_from_user`.
    fn copy_msg_to_user(&self, dst: VirBytes, msg: &Message) -> Result<(), CopyError>;

    /// Read one SENDA table entry from user space.
    ///
    /// C: `A_RETR(i)` — proc.c:1244 (per-entry copy-in of `asynmsg_t`).
    /// The kernel never caches the whole table; entries are read one at
    /// a time and results written back one at a time, so no kernel-side
    /// copy of the table exists (C stores only the user-space table
    /// address + size in `priv->s_asyntab`/`s_asynsize`, priv.h:28).
    ///
    /// Returns `(dst_endpoint, message, flags)` — C: `asynmsg_t.dst`,
    /// `.msg`, `.flags`. Flag values are `AMF_*` (ipc.h:2754-2761).
    /// `root` = the SENDER's page-table root (C reads the sender's table
    /// through the sender's segment — A_RETR runs `umap(sender)`); at the
    /// senda syscall the caller IS the sender, at `deliver_async` the
    /// receiver's own root would be wrong.
    fn read_senda_entry(
        &self,
        root: minix_types::PhysBytes,
        table: VirBytes,
        index: usize,
    ) -> Result<(Endpoint, Message, i32), CopyError>;

    /// Write one SENDA table result back to user space.
    ///
    /// C: `A_INSRT(i)` — proc.c:1307 (per-entry copy-out of the result +
    /// `AMF_DONE` flag). The kernel ignores copy errors here, same as C
    /// ("Copy results to caller; ignore errors").
    fn write_senda_result(
        &self,
        root: minix_types::PhysBytes,
        table: VirBytes,
        index: usize,
        result: i32,
        flags: i32,
    ) -> Result<(), CopyError>;
}

/// User-copy error. Distinguishes page fault (retryable via VM) from
/// out-of-bounds (immediately fatal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyError {
    /// Page not mapped or permission denied. Triggers VM suspend on
    /// first occurrence, SIGSEGV on second consecutive failure.
    /// C: `vm_suspend(VMS_PAGEFAULT)` — proc.c:281-282.
    PageFault,
    /// Address outside user space. Always fatal (SIGSEGV).
    OutOfBounds,
}

/// Stub `UserCopy` implementation used by tests and boot-time paths
/// where no real user space exists. Performs a direct bitwise copy with
/// no page-fault handling.
#[derive(Debug, Default, Clone, Copy)]
pub struct KernelUserCopy;

/// Top of the user half of the canonical address space, per architecture
/// (the boundary the higher-half kernel link scripts draw —
/// `02-higher-half-kernel.md` §4.1).
///
/// Every virtual address at or above this limit is kernel or
/// non-canonical — never a message buffer a user process may hand the
/// kernel. C enforced the same boundary structurally: the i386
/// `copy_msg_from_user`/`copy_msg_to_user` run through the user DS
/// segment, whose limit cannot cover kernel linear addresses
/// (usermapped_glo_ipc.S). The flat shared-page-table model has no
/// segment limit, so the bound must be checked explicitly before any
/// user-buffer access.
#[cfg(target_arch = "x86_64")]
const USER_ADDRESS_SPACE_LIMIT: u64 = 0x0000_8000_0000_0000; // PML4[256] base
#[cfg(target_arch = "aarch64")]
const USER_ADDRESS_SPACE_LIMIT: u64 = 0x0000_8000_0000_0000; // TTBR1 region base
#[cfg(target_arch = "riscv64")]
const USER_ADDRESS_SPACE_LIMIT: u64 = 0x0000_0040_0000_0000; // Sv39 VPN[2]=256

/// Validate a user buffer range for a kernel-side user copy, software-walk
/// first so the access below can no longer fault.
///
/// C's user-message copies recover from bad user pointers by fault
/// redirection (`__user_copy_msg_pointer_failure` — mpx.S, fed by the
/// EIP-range check in exception.c:206-219). This kernel expresses the same
/// contract in types instead: the copy path validates before touching the
/// buffer, so a bad pointer produces an `Err` here (EFAULT at the send
/// layer, the two-strike suspend/SIGSEGV policy at delivery) rather than a
/// kernel-mode fault. Same discipline as the cross-space copy paths, which
/// software-walk the page table before their Direct Map window access.
///
/// Checks, in order:
/// 1. The range lies entirely in the user half (`USER_ADDRESS_SPACE_LIMIT`)
///    — otherwise `Err(CopyError::OutOfBounds)`.
/// 2. With an active page-table root, every page in the range is mapped
///    user-accessible (and writable for the write direction) — otherwise
///    `Err(CopyError::PageFault)`. Without a root (no paging yet — boot or
///    hosted tests) there is no translation to validate and no user
///    process to protect, so the check passes.
///
/// `walk` is injected so hosted tests can drive every branch without real
/// page tables; production passes `minix_arch::CurrentPteWalk::walk`.
fn user_copy_range_mapped(
    root: Option<minix_types::PhysBytes>,
    va: VirBytes,
    len: usize,
    need_write: bool,
    walk: impl Fn(
        minix_types::PhysBytes,
        VirBytes,
    ) -> Option<(minix_types::PhysBytes, minix_arch::paging::PageFlags)>,
) -> Result<(), CopyError> {
    use minix_arch::paging::PageFlags;

    // 1. User-half bound (checked_add: a wrap must not read as "fits").
    match va.0.checked_add(len as u64) {
        Some(end) if va.0 < USER_ADDRESS_SPACE_LIMIT && end <= USER_ADDRESS_SPACE_LIMIT => {}
        _ => return Err(CopyError::OutOfBounds),
    }

    // 2. Per-page translation check against the active root.
    let Some(root) = root else {
        return Ok(());
    };
    const PAGE: u64 = 4096;
    let mut page = va.0 & !(PAGE - 1);
    let end = va.0 + len as u64;
    while page < end {
        let Some((_, flags)) = walk(minix_types::PhysBytes(root.0), VirBytes(page)) else {
            return Err(CopyError::PageFault);
        };
        let user_ok = flags.contains(PageFlags::USER_ACCESSIBLE);
        let write_ok = !need_write || flags.contains(PageFlags::WRITABLE);
        if !user_ok || !write_ok {
            return Err(CopyError::PageFault);
        }
        page += PAGE;
    }
    Ok(())
}

/// Copy `buf.len()` bytes between a user VA (translated via `root`'s page
/// tables) and `buf`. Read direction (`to_kernel = false`) fills `buf` from
/// user memory; write direction stores `buf` into user memory.
///
/// NK4-C 1.12：跨地址空间访问的唯一正确姿势——每页经 `walk(root, va)`
/// 翻译为物理地址，再经 Direct Map 窗口访问（`kernel_phys_to_virt`）。
/// 直接解引用用户 VA 只在「current root == 该 root」时成立；deliver_async
/// 等跨进程场景在接收者陷入里运行，同一 VA 落到错误地址空间（s17t
/// vector 13 根因）。C 同形：umap(sender) + phys 拷贝（proc.c:1244）。
#[cfg(not(test))]
fn copy_via_root_pages<D: minix_arch::DirectMapArch>(
    root: minix_types::PhysBytes,
    va: u64,
    buf: &mut [u8],
    to_kernel: bool,
) -> Result<(), CopyError> {
    use minix_arch::paging::PageFlags;

    const PAGE: u64 = 4096;
    let len = buf.len() as u64;
    // User-half bound（同 user_copy_range_mapped：checked_add 防回绕）。
    match va.checked_add(len) {
        Some(end) if va < USER_ADDRESS_SPACE_LIMIT && end <= USER_ADDRESS_SPACE_LIMIT => {}
        _ => return Err(CopyError::OutOfBounds),
    }
    let mut done: u64 = 0;
    while done < len {
        let cur = va + done;
        let page_off = cur & (PAGE - 1);
        let chunk = core::cmp::min(len - done, PAGE - page_off) as usize;
        let Some((pa, flags)) =
            minix_arch::CurrentPteWalk::walk(minix_types::PhysBytes(root.0), VirBytes(cur))
        else {
            return Err(CopyError::PageFault);
        };
        if !flags.contains(PageFlags::USER_ACCESSIBLE) {
            return Err(CopyError::PageFault);
        }
        // walk 返回 cur 的完整物理地址（页内偏移已折入，x86_64
        // walk_translate：pte&ADDR_MASK | vaddr&0xFFF）——DV 即 cur 的
        // DM 窗口映照，不再加页内偏移。
        let dv = D::kernel_phys_to_virt(pa);
        let _ = page_off;
        // SAFETY: DM 窗口 VA 由内核直映，pa 来自页表 walk（该页
        // USER_ACCESSIBLE）；chunk 不越过 cur 所在页。
        unsafe {
            let page = dv.0 as *mut u8;
            // 只保留一次可变再借用（commit 后评审 P1：同时构造
            // as_ptr/as_mut_ptr 按 Stacked Borrows 会使前者标签失效）；
            // 可变指针可作 *const 读源，两臂共用 base。
            let base = buf.as_mut_ptr().add(done as usize);
            // NK4-C 1.12e：方向按 doc 契约——to_kernel=true 把 buf 写入
            // 用户页，false 把用户页读进 buf。此前两臂互换：读腿
            // （read_senda_entry/write_senda_result 的 head 预读传 false）
            // 实际把清零缓冲写进用户槽位（A_RETR 变破坏性写），flags
            // 恒解出 0=AMF_EMPTY → senda 一条不投（s18 系列全零签名）。
            if to_kernel {
                core::ptr::copy_nonoverlapping(base, page, chunk);
            } else {
                core::ptr::copy_nonoverlapping(page as *const u8, base, chunk);
            }
        }
        done += chunk as u64;
    }
    Ok(())
}

impl UserCopy for KernelUserCopy {
    #[cfg(not(test))]
    fn copy_msg_from_user(&self, src: VirBytes) -> Result<Message, CopyError> {
        // Validate-first (see `user_copy_range_mapped`): bound the buffer to
        // the user half and software-walk every page user-accessible in the
        // caller's active address space, then read through the shared page
        // tables (higher-half layout — kernel and user share the CR3). The
        // volatile read prevents the compiler from eliding the access; after
        // validation an unmapped VA is no longer reachable, and a fault here
        // would mean the walk lied (kernel bug), not bad user input.
        //
        // SAFETY: `src` is a user VA from the trap-frame contract (the
        // caller's RDI/m_user), bounded and translation-checked above.
        user_copy_range_mapped(
            crate::current_root_phys(),
            src,
            core::mem::size_of::<Message>(),
            /* need_write = */ false,
            minix_arch::CurrentPteWalk::walk,
        )?;
        let msg = unsafe { core::ptr::read_volatile(src.0 as *const Message) };
        Ok(msg)
    }
    #[cfg(test)]
    fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> {
        // Hosted test build: no real user address space. Return a zeroed
        // message — tests that need specific content use mock UserCopy.
        Ok(Message::default())
    }
    #[cfg(not(test))]
    fn copy_msg_to_user(&self, dst: VirBytes, msg: &Message) -> Result<(), CopyError> {
        // SAFETY: symmetric with copy_msg_from_user — CPL0 write to the
        // caller's user VA through the shared page tables, with the write
        // direction additionally requiring each page writable.
        user_copy_range_mapped(
            crate::current_root_phys(),
            dst,
            core::mem::size_of::<Message>(),
            /* need_write = */ true,
            minix_arch::CurrentPteWalk::walk,
        )?;
        // NK4-C 第 33 轮守卫探针（task1-close 裁决删除）：消息投递写的
        // (VA, 当前 root, walk 得到的物理页)。清零者必在内核侧且本函数
        // 是停车-唤醒窗口里唯一直写用户内存的路径——若 pa 落在 PT 页
        // （与同轮 pf dump 的 lvl1pa 对账），即「消息写错树/错页」实锤。
        {
            use core::sync::atomic::{AtomicU64, Ordering as AtomicOrd};
            static NW: AtomicU64 = AtomicU64::new(0);
            if NW.fetch_add(1, AtomicOrd::Relaxed) < 64 {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                let root = crate::current_root_phys().map(|r| r.0).unwrap_or(0);
                let pa = minix_arch::CurrentPteWalk::walk(
                    minix_types::PhysBytes(root),
                    dst,
                )
                .map(|(pa, _)| pa.0 & !0xFFF)
                .unwrap_or(0);
                C0::write_str("nk4a: msgw va=");
                C0::write_hex(dst.0);
                C0::write_str(" root=");
                C0::write_hex(root);
                C0::write_str(" pa=");
                C0::write_hex(pa);
                C0::write_str("\n");
            }
        }
        unsafe { core::ptr::write_volatile(dst.0 as *mut Message, *msg) };
        Ok(())
    }
    #[cfg(test)]
    fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> {
        // Hosted test: no-op stub.
        Ok(())
    }
    #[cfg(not(test))]
    fn read_senda_entry(
        &self,
        root: minix_types::PhysBytes,
        table: VirBytes,
        index: usize,
    ) -> Result<(Endpoint, Message, i32), CopyError> {
        // NK4-C 1.12：真实现。C A_RETR（proc.c:1244）按发送者的段读
        // `asynmsg_t`。关键：**读走「发送者 root 翻译 → 物理 → Direct Map
        // 窗口」**，绝不直接解引用用户 VA——deliver_async 跑在接收者的
        // receive 陷入里，current CR3 是接收者的，同一用户 VA 落到错误
        // 地址空间（s17t GP fault vector 13 根因）。布局对位 C ipc.h:2745
        // `asynmsg`：flags@0 / dst@4 / result@8 / msg@16，槽距 =
        // `size_of::<WireAsyncSlot>()`（本项目 `Message`=80B → 槽 96B）；
        // WireAsyncSlot 镜像 + offset_of 守卫钉死（同 RS WirePrivUpdate
        // 判例）。C 用 `sizeof(asynmsg_t)` 由类型推导步长（proc.c:1176
        // A_RETR），这里同样必须派生自类型：旧版硬编码 `SLOT=80`（C 32 位
        // 旧布局凑出来的值）会让 slot[i≥1] 按 80 错位读、并用 80 字节缓冲
        // `read_volatile` 一个 96 字节结构（越界 UB）——NK4-C B7 启动死锁
        // 真根因（RS_INIT→VFS 落 slot≥1 永读不出 → VFS 永停 receive(RS)）。
        const SLOT: usize = core::mem::size_of::<WireAsyncSlot>();
        let base: u64 = table.0 + (index as u64) * (SLOT as u64);
        let mut out = [0u8; SLOT];
        copy_via_root_pages::<minix_arch::CurrentDirectMap>(
            root, base, &mut out, false,
        )?;
        // SAFETY: out 是本函数栈上的 `size_of::<WireAsyncSlot>()` 字节缓冲，
        // 由上面的分页拷贝填充；WireAsyncSlot 为 repr(C) 且 size 断言
        // == 16+size_of::<Message>()（与 minix-sys AsyncSlot 逐字节一致）。
        let slot = unsafe { core::ptr::read_volatile(out.as_ptr() as *const WireAsyncSlot) };
        Ok((slot.destination, slot.message, slot.flags as i32))
    }
    #[cfg(test)]
    fn read_senda_entry(
        &self,
        _root: minix_types::PhysBytes,
        _table: VirBytes,
        _index: usize,
    ) -> Result<(Endpoint, Message, i32), CopyError> {
        // Hosted test: no real address space (同 copy_msg_from_user)。
        Err(CopyError::PageFault)
    }
    #[cfg(not(test))]
    fn write_senda_result(
        &self,
        root: minix_types::PhysBytes,
        table: VirBytes,
        index: usize,
        result: i32,
        flags: i32,
    ) -> Result<(), CopyError> {
        // C A_INSRT（proc.c:1307）：回写 result + AMF_DONE——只写槽头
        // flags@0 与 result@8 两域（dst/msg 不动）。同读腿：发送者 root
        // 翻译 → DM 窗口写，不解引用用户 VA。步长同样派生自
        // `size_of::<WireAsyncSlot>()`（旧硬编码 80 会让 slot[i≥1] 写错地址，
        // 见 `read_senda_entry` 注释）。flags/result 偏移不变（flags@0、
        // result@8 仍落在槽头 12 字节内）。
        const SLOT: usize = core::mem::size_of::<WireAsyncSlot>();
        let base: u64 = table.0 + (index as u64) * (SLOT as u64);
        let mut head = [0u8; 12];
        copy_via_root_pages::<minix_arch::CurrentDirectMap>(
            root, base, &mut head, false,
        )?;
        head[0..4].copy_from_slice(&(flags as u32).to_ne_bytes());
        head[8..12].copy_from_slice(&result.to_ne_bytes());
        copy_via_root_pages::<minix_arch::CurrentDirectMap>(
            root, base, &mut head, true,
        )?;
        Ok(())
    }
    #[cfg(test)]
    fn write_senda_result(
        &self,
        _root: minix_types::PhysBytes,
        _table: VirBytes,
        _index: usize,
        _result: i32,
        _flags: i32,
    ) -> Result<(), CopyError> {
        // Hosted test: no-op stub。
        Ok(())
    }
}

/// SENDA 用户表槽位的内核侧镜像（C `asynmsg_t` — ipc.h:2745-2751）。
/// 仅在 `read_senda_entry`/`write_senda_result` 解码用户表用；`repr(C)`
/// + offset_of 守卫保证与 minix-sys `AsyncSlot`（同序字段）逐字节一致。
#[repr(C)]
struct WireAsyncSlot {
    flags: u32,
    destination: Endpoint,
    result: i32,
    message: Message,
}
const _: () = assert!(core::mem::offset_of!(WireAsyncSlot, destination) == 4);
const _: () = assert!(core::mem::offset_of!(WireAsyncSlot, message) == 16);
const _: () = assert!(
    core::mem::size_of::<WireAsyncSlot>()
        == 16 + core::mem::size_of::<Message>()
);

/// Result of `IpcEngine::deliver_message`. C: `delivermsg` is `void`;
/// Rust makes the failure path explicit so `switch_to_user` can route
/// to VM suspend or signal delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverResult {
    /// Message copied to user space successfully. C: clear `MF_DELIVERMSG`.
    Delivered,
    /// First page fault — caller should `vm_suspend(VMS_PAGEFAULT)` and
    /// set `MF_MSGFAILED`. C: proc.c:281-282.
    PageFault,
    /// Second consecutive page fault, or out-of-bounds — caller should
    /// `cause_sig(SIGSEGV)`. C: proc.c:278.
    Segfault,
}

/// Deliver pending message to user space.
///
/// Free function extracted from `IpcEngine::deliver_message` so that
/// `ProcessTable::process_misc_flags` can call it without constructing
/// a full `IpcEngine` (which requires `priv_table` + `procs` slice that
/// `ProcessTable` already owns).
///
/// C: `delivermsg(&p)` — proc.c:263-294. Called by `process_misc_flags`
/// (proc.c:~700) when `MF_DELIVERMSG` is set.
///
/// # Two-failure policy
///
/// - Success → clear `MF_DELIVERMSG` (+ `MF_MSGFAILED` if set).
/// - First `PageFault` → set `MF_MSGFAILED`, return `PageFault`
///   (caller routes to `vm_suspend(VMS_PAGEFAULT)`).
/// - Second consecutive failure (`MF_MSGFAILED` already set) →
///   return `Segfault` (caller routes to `cause_sig(SIGSEGV)`).
/// - `OutOfBounds` → `Segfault` immediately.
pub fn delivermsg(proc: &mut crate::proc::KProcess, user_copy: &dyn UserCopy) -> DeliverResult {
    debug_assert!(
        proc.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG),
        "delivermsg called without MF_DELIVERMSG"
    );

    let msg = proc.p_delivermsg;
    let user_addr = proc.p_delivermsg_vir;

    match user_copy.copy_msg_to_user(user_addr, &msg) {
        Ok(()) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            // C: proc.c:290-292 — the completed IPC returns OK to the user
            // caller. The wake half (mini_notify direct delivery) only sets
            // MF_DELIVERMSG; the return code lands here, when the pending
            // delivery is flushed at dispatch time (C switch_to_user's
            // delivermsg stage). Without it a blocked receiver woken by a
            // notify restores with whatever RAX held at trap entry (real
            // machine NK4-A C-3: VM woken by the SYSTEM SIGKMEM notify saw
            // its src argument, ANY=31744, as the receive result and
            // dropped the wake — RS starved on VMREQUEST).
            // MF_CONTEXT_SET means the kernel deliberately rewrote the
            // context (signal delivery); do not clobber the return reg.
            if !proc.p_misc_flags.is_set(MiscFlagsBits::CONTEXT_SET) {
                crate::proc::set_ipc_return_code(proc, OK as i64);
            }
            DeliverResult::Delivered
        }
        Err(CopyError::PageFault) => {
            if proc.p_misc_flags.is_set(MiscFlagsBits::MSGFAILED) {
                // Second consecutive failure → SIGSEGV. C: proc.c:278.
                proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
                proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
                DeliverResult::Segfault
            } else {
                // First failure → vm_suspend. C: proc.c:281-282.
                proc.p_misc_flags.set(MiscFlagsBits::MSGFAILED);
                DeliverResult::PageFault
            }
        }
        Err(CopyError::OutOfBounds) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            DeliverResult::Segfault
        }
    }
}

// ── Async message passing (SENDA) ──
//
// Design (re-judged from the earlier `AsyncMessageTable` Vec cache):
// C never copies the SENDA table into the kernel. `try_deliver_senda`
// reads entries from user space one at a time (`A_RETR`, proc.c:1244)
// and writes results back one at a time (`A_INSRT`, proc.c:1307). If
// entries remain undelivered, C stores only the table address and size
// in the caller's privilege structure (`s_asyntab`/`s_asynsize`,
// priv.h:28) — retry re-reads the table from user space
// (try_async → try_deliver_senda, proc.c:1348-1410).
//
// The kernel-side Vec cache was a double deviation: it heap-allocated
// (violating the kernel's zero-heap storage model — 06 §2.0 layer 2)
// and changed retry semantics (kernel copy vs C's user-space re-read).
// Delivery state lives where C keeps it: the `AMF_DONE` flag in the
// user table entry, plus the `s_asyn_pending` bitmap on the target
// (set by the delivery pass, cleared by retry delivery).

// ── Sender wait queue (design §2.5 / AT-2 / ARCH-2) ──
//
// C: `struct proc *p_caller_q` (queue head, owned by the TARGET —
// proc.h:73) + `struct proc *p_q_link` (chain link, living on the
// SENDER — proc.h:74) — an intrusive FIFO threaded through the
// process-table slots.
//
// Design (re-judged from the earlier `VecDeque<ProcNr>`): same
// intrusive FIFO, links re-expressed as `Option<ProcNr>` slot indices.
// Index links carry no reference semantics, so Rust's aliasing rules
// are not violated and no heap is involved — enqueue touches only
// `Option<ProcNr>` fields already inside the slots. This restores C's
// properties exactly:
//   1. **Zero allocation** — the kernel has no heap at any point in its
//      lifetime (06-proc-init-boot-proc.md §2.0 layer 2: no malloc/kmalloc
//      in the entire C kernel; minix-rs follows the same storage model).
//   2. **Infallible enqueue** — C's enqueue (two pointer writes,
//      proc.c:960-964) never fails; `VecDeque::push_back` would abort
//      the kernel on allocation failure.
//   3. **Bounded capacity for free** — a process blocks on at most one
//      send (`RTS_SENDING` blocks the whole process, proc.c:948), so
//      each slot appears in at most one queue: total queued entries ≤
//      NR_TASKS + NR_PROCS, statically guaranteed. No overflow path.
//
// Storage (in KProcess, C-isomorphic):
//   - target slot: `caller_q_head` / `caller_q_tail` (C: `p_caller_q`;
//     the tail is an O(1)-append extension — C walks O(n) to the tail,
//     proc.c:960-964; FIFO order identical)
//   - sender slot: `send_q_link` (C: `p_q_link`)
//
// The operations below take `&mut [KProcess]` and resolve links via
// `nr_to_idx` (bounds-checked, same as C's `proc_addr` offset formula).
// C walks find+unlink in one pass (proc.c:1084); Rust's split
// find/remove walks twice — O(n) either way, n ≤ table size.

/// SENDA table entry flags. C: `ipc.h:2754-2761` (octal values).
pub const AMF_EMPTY: i32 = 0o0;
pub const AMF_VALID: i32 = 0o1;
pub const AMF_DONE: i32 = 0o2;
pub const AMF_NOTIFY: i32 = 0o4;
pub const AMF_NOREPLY: i32 = 0o10;
pub const AMF_NOTIFY_ERR: i32 = 0o20;
/// All flag bits the kernel accepts. C: proc.c:1251.
const AMF_ALL: i32 = AMF_VALID | AMF_DONE | AMF_NOTIFY | AMF_NOREPLY | AMF_NOTIFY_ERR;

/// Enqueue `caller_idx`'s process at the tail of `dst_idx`'s sender queue.
///
/// C: `while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller_ptr;`
/// — proc.c:960-964 (walk to tail, link). Rust keeps an explicit tail
/// for O(1) append; FIFO order is identical.
///
/// Invariant (INV-1): a process with `RTS_SENDING` set is in exactly one
/// queue; `send_q_link` is `None` outside a queue.
pub(crate) fn caller_q_push(procs: &mut [KProcess], dst_idx: usize, caller_idx: usize) {
    let caller_nr = procs[caller_idx].p_nr;
    procs[caller_idx].send_q_link = None;
    match procs[dst_idx].caller_q_tail {
        Some(tail_nr) => {
            if let Some(tail_idx) = nr_to_idx(tail_nr) {
                procs[tail_idx].send_q_link = Some(caller_nr);
            }
            procs[dst_idx].caller_q_tail = Some(caller_nr);
        }
        None => {
            procs[dst_idx].caller_q_head = Some(caller_nr);
            procs[dst_idx].caller_q_tail = Some(caller_nr);
        }
    }
}

/// Find the first queued sender on `dst_idx`'s queue matching
/// `src_endpoint` AND satisfying `accept` (head-first scan). Returns the
/// sender's slot index. This is the single walk implementation for the
/// caller queue — production (`IpcEngine::caller_q_find_allowed`, which
/// passes the filter check as `accept`) and tests share it.
///
/// C: `while (*xpp) { if (CANRECEIVE(...)) break; }` — proc.c:1077-1105.
/// `accept` plugs in at exactly the `CANRECEIVE` position of the C loop
/// (in C the filter check lives inside `CANRECEIVE`; in Rust D-16 splits
/// it into `can_receive`). `Endpoint::ANY` matches the head (C: first
/// queue entry).
pub(crate) fn caller_q_find(
    procs: &[KProcess],
    dst_idx: usize,
    src_endpoint: Endpoint,
    mut accept: impl FnMut(usize) -> bool,
) -> Option<usize> {
    let mut cur = procs[dst_idx].caller_q_head;
    while let Some(nr) = cur {
        let idx = nr_to_idx(nr)?;
        let endpoint_match = src_endpoint == Endpoint::ANY || procs[idx].p_endpoint == src_endpoint;
        if endpoint_match && accept(idx) {
            return Some(idx);
        }
        cur = procs[idx].send_q_link;
    }
    None
}

/// Remove the sender at slot index `sender_idx` from `dst_idx`'s queue,
/// unlinking it (predecessor link + head/tail fixup).
///
/// C: `*xpp = (*xpp)->p_q_link` — proc.c:1084 (find + unlink in one walk);
/// same walk shape in `clear_ipc` (system.c:520-531).
pub(crate) fn caller_q_remove(procs: &mut [KProcess], dst_idx: usize, sender_idx: usize) -> bool {
    let sender_nr = procs[sender_idx].p_nr;
    let sender_next = procs[sender_idx].send_q_link;
    let mut cur = procs[dst_idx].caller_q_head;
    let mut prev: Option<ProcNr> = None;
    while let Some(nr) = cur {
        let Some(idx) = nr_to_idx(nr) else { return false };
        if idx == sender_idx {
            match prev {
                Some(prev_nr) => {
                    if let Some(prev_idx) = nr_to_idx(prev_nr) {
                        procs[prev_idx].send_q_link = sender_next;
                    }
                }
                None => procs[dst_idx].caller_q_head = sender_next,
            }
            if procs[dst_idx].caller_q_tail == Some(sender_nr) {
                procs[dst_idx].caller_q_tail = prev;
            }
            procs[sender_idx].send_q_link = None;
            return true;
        }
        prev = Some(nr);
        cur = procs[idx].send_q_link;
    }
    false
}

/// Remove `target_nr` from `dst_idx`'s queue by ProcNr value.
///
/// C: `clear_ipc` walk — system.c:520-531 (dead process unlinked from
/// its send target's queue); `abort_proc_ipc_send` — do_update.c:226-234.
pub(crate) fn caller_q_remove_by_nr(
    procs: &mut [KProcess],
    dst_idx: usize,
    target_nr: ProcNr,
) -> bool {
    match nr_to_idx(target_nr) {
        Some(i) => caller_q_remove(procs, dst_idx, i),
        None => false,
    }
}

// ── Test/convenience helpers (queue length + membership) ──

/// Number of senders queued on `dst_idx`'s queue (walks the chain).
#[cfg(test)]
pub(crate) fn caller_q_len(procs: &[KProcess], dst_idx: usize) -> usize {
    let mut n = 0;
    let mut cur = procs[dst_idx].caller_q_head;
    while let Some(nr) = cur {
        let Some(idx) = nr_to_idx(nr) else { break };
        n += 1;
        cur = procs[idx].send_q_link;
    }
    n
}

/// `true` iff `dst_idx`'s queue is empty.
#[cfg(test)]
pub(crate) fn caller_q_is_empty(procs: &[KProcess], dst_idx: usize) -> bool {
    procs[dst_idx].caller_q_head.is_none()
}

// ── IPC Engine (design §2.6 / AT-3 / ARCH-3) ──

/// IPC core engine. Owns process table reference for in-place modification.
///
/// Design decision §3.3 / ARCH-3: NOT a ZST namespace — holds `&mut [KProcess]`.
///
/// C: free functions (`mini_send`, `mini_receive`, ...) taking `struct proc*`
/// on every call. Rust: `IpcEngine` holds the borrow once, methods don't
/// repeat it — eliminating the per-call `&mut [KProcess]` parameter.
///
/// # BKL (Big Kernel Lock)
///
/// **Precondition**: every method requires the caller to hold the BKL.
/// The `&mut [KProcess]` borrow is the Rust-level proxy for "BKL held":
/// it statically prevents aliasing mutable access across CPUs, matching
/// C's `big_kernel_lock` spinlock guarantee that only one CPU at a time
/// may mutate the process table.
///
/// # Lifetime
///
/// `'a` ties the engine to the borrow of `procs` / `priv_table` /
/// `user_copy`. The engine is short-lived (constructed per syscall
/// dispatch, dropped when the syscall returns).
pub struct IpcEngine<'a> {
    /// Process table slice (mutable — IPC modifies RTS flags, queues, etc.).
    procs: &'a mut [KProcess],
    /// Privilege table (mutable — notify updates `s_notify_pending` bitmap).
    priv_table: &'a mut PrivTable,
    /// User-space copy abstraction (arch-specific impl injected).
    user_copy: &'a dyn UserCopy,
    /// IPC filter pool (D-16, optional — `None` = no filtering available
    /// and every message is allowed, the pre-D-16 behavior; production
    /// dispatch wires the global `IPC_FILTER_POOL` via
    /// [`Self::with_filter_pool`]).
    filter_pool: Option<&'a crate::ipc_filter::IpcFilterPool>,
    /// Sender whose message was just delivered while `MF_SIG_DELAY` was set.
    ///
    /// C: proc.c:1082-1083 — `if (sender->p_misc_flags & MF_SIG_DELAY)
    /// sig_delay_done(sender)`. The delay-end notification
    /// (`sig_delay_done` → `cause_sig(SIGSNDELAY)`) must run at the
    /// `ProcessTable` level because `cause_signal` sets `RTS_SIGNALED`
    /// through the scheduler-aware `rts_set` (dequeue). The engine only has
    /// a slice, so it records the sender here and the `ProcessTable`-level
    /// dispatcher (`dispatch_ipc`) completes the protocol after `do_ipc`
    /// returns. At most one sender is delivered per IPC operation.
    sig_delay_sender: Option<ProcNr>,
    /// The processes the engine just woke by clearing a blocking RTS flag
    /// (`RTS_RECEIVING` / `RTS_SENDING`) with the primitive setter. The
    /// engine holds a procs slice without run-queue access, so the wake's
    /// ENQUEUE half (C `RTS_UNSET` macro — clear + enqueue-when-runnable)
    /// must be completed by the `ProcessTable`-level dispatcher via
    /// [`Self::take_wake_target`] — the wake-direction mirror of
    /// `ProcessTable::dequeue_if_blocked` (the block direction).
    ///
    /// NK4-C 1.12：单槽 Option 会被同一次 syscall 内的第二次唤醒覆盖
    /// （sendrec = send 腿唤醒目的地 + receive 腿 drain 唤醒发送者；
    /// 丢掉的进程 rts 已清却永不入队 → runnable=yes queued=no，F10d
    /// 同族，s17p 实锤 sched 卡死）。C 的 RTS_UNSET 在每个 clear 现场
    /// 立即入队，无此丢失窗口；这里以 4 槽记录 + 调用方 drain 全部。
    wake_targets: [Option<ProcNr>; 4],
}

impl<'a> IpcEngine<'a> {
    /// Construct with process table, privilege table, and user-copy impl.
    pub fn new(
        procs: &'a mut [KProcess],
        priv_table: &'a mut PrivTable,
        user_copy: &'a dyn UserCopy,
    ) -> Self {
        Self {
            procs,
            priv_table,
            user_copy,
            filter_pool: None,
            sig_delay_sender: None,
            wake_targets: [None; 4],
        }
    }

    /// Wire the IPC filter pool (D-16). Production dispatch passes the
    /// global `IPC_FILTER_POOL`; without it no filtering is applied.
    pub(crate) fn with_filter_pool(mut self, pool: &'a crate::ipc_filter::IpcFilterPool) -> Self {
        self.filter_pool = Some(pool);
        self
    }

    /// D-16 (C ipc.h:17-22 CANRECEIVE 的过滤半边): the receiver's filter
    /// chain decides whether a message from `src_e` with `m_type` is
    /// acceptable. C forces `m_source = src_e` (system.c:847) and walks
    /// the chain — see `ipc_filter::chain_allowed`. No pool wired, no
    /// chain head (`s_ipcf == None`), or filters not configured → allow.
    fn can_receive(&self, receiver_idx: usize, src_e: Endpoint, m_type: i32) -> bool {
        let Some(head) = self.procs[receiver_idx]
            .priv_id
            .and_then(|pid| self.priv_table.get(pid))
            .and_then(|p| p.mem.s_ipcf)
        else {
            return true;
        };
        let Some(pool) = self.filter_pool else {
            return true;
        };
        let procs = &*self.procs;
        let priv_table = &*self.priv_table;
        let mut class_of = move |ep: Endpoint| -> Option<crate::ipc_filter::EndpointClass> {
            let idx = procs
                .iter()
                .position(|p| p.p_endpoint == ep && !p.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE))?;
            let p = &procs[idx];
            // C: iskerneln（proc.h）——任务区槽号 <= 0。
            if p.p_nr.0 <= 0 {
                return Some(crate::ipc_filter::EndpointClass::Task);
            }
            let Some(pid) = p.priv_id else {
                return Some(crate::ipc_filter::EndpointClass::Usr);
            };
            match priv_table.get(pid) {
                Some(priv_) if priv_
                    .flags
                    .s_flags
                    .contains(crate::capability::ProcessCapability::SYS_PROC) =>
                {
                    Some(crate::ipc_filter::EndpointClass::Sys)
                }
                _ => Some(crate::ipc_filter::EndpointClass::Usr),
            }
        };
        crate::ipc_filter::chain_allowed(pool, Some(head), src_e, m_type, &mut class_of)
    }

    /// D-16: filter-aware `caller_q_find` — C proc.c:1053-1058. Walks
    /// the caller queue via the shared [`caller_q_find`] walk; a sender
    /// whose cached message (`p_sendmsg`, C: `m_src_p = &sender->p_sendmsg`)
    /// fails the receiver's filter chain stays queued (retried on a later
    /// receive) and the scan continues with the next queued sender — the
    /// filter check rides in the walk's `accept` position, exactly where
    /// C's `CANRECEIVE` sits.
    fn caller_q_find_allowed(&self, caller_idx: usize, src_endpoint: Endpoint) -> Option<usize> {
        caller_q_find(self.procs, caller_idx, src_endpoint, |idx| {
            let sender_ep = self.procs[idx].p_endpoint;
            self.can_receive(caller_idx, sender_ep, self.procs[idx].p_sendmsg.m_type)
        })
    }

    /// Take the sender whose message was delivered while `MF_SIG_DELAY`
    /// was set (and whose delay-end notification is now due), clearing the
    /// record. See [`Self::sig_delay_sender`].
    ///
    /// Called by the `ProcessTable`-level dispatcher after `do_ipc`
    /// returns; `Some(sender_nr)` means "call `sig_delay_done(sender_nr)`".
    pub fn take_sig_delay_sender(&mut self) -> Option<ProcNr> {
        self.sig_delay_sender.take()
    }

    /// Take the wake target recorded by the last IPC operation — the
    /// process whose blocking RTS flag the engine cleared with the
    /// primitive setter. The `ProcessTable`-level caller must complete
    /// the wake by enqueueing it (C: `RTS_UNSET`'s enqueue half,
    /// proc.h:216-224) — see `ProcessTable::enqueue_if_woken`.
    pub fn take_wake_target(&mut self) -> Option<ProcNr> {
        // Drain order: first non-empty slot (all slots get enqueued by the
        // caller's loop — order is irrelevant for run-queue insertion).
        self.wake_targets.iter_mut().find_map(|slot| slot.take())
    }

    /// Record a wake target (engine-internal helper for the primitive
    /// flag clears at the delivery sites).
    fn record_wake_target(&mut self, nr: ProcNr) {
        // 1.12：满了就丢最旧的（4 槽上限远超单次 IPC 的真实唤醒数——
        // sendrec 最坏 2 个；保留防御性容量），不静默覆盖最新。
        if let Some(slot) = self.wake_targets.iter_mut().find(|s| s.is_none()) {
            *slot = Some(nr);
        }
    }

    // ── Helpers ──

    /// Index into process table by `ProcNr`. Returns `None` for invalid nr.
    fn idx_of(&self, nr: ProcNr) -> Option<usize> { nr_to_idx(nr) }

    /// Find index by endpoint.
    fn idx_by_endpoint(&self, ep: Endpoint) -> Option<usize> {
        self.procs.iter().position(|p| p.p_endpoint == ep)
    }

    /// Dynamic field selection for "blocked-on endpoint".
    ///
    /// C: `P_BLOCKEDON(p)` macro — proc.h:187-194. Returns the endpoint
    /// this process is blocked on, or `None` if it is runnable.
    ///
    /// # P0 FIX (FIX-3 / AT-6)
    ///
    /// The previous `detect_deadlock` fixed the field via the `function`
    /// argument (SEND→`p_sendto_e`, RECEIVE→`p_getfrom_e`). That could
    /// not detect mixed-chain deadlocks (A send→B, B receive←C, C
    /// send→A) because the walk broke at B (RECEIVING, not SENDING).
    /// This method mirrors the C macro's dynamic dispatch on RTS flags,
    /// delegating to `KProcess::blocked_on` (proc.rs).
    fn blocked_on(proc_: &KProcess) -> Option<Endpoint> {
        proc_.blocked_on()
    }

    /// Check if `dst` is willing to receive a message from `src`.
    ///
    /// C: `WILLRECEIVE(src_e, dst, m, mp)` macro — ipc.h:14. The IPC
    /// filter extension (`m_src_v` / `m_src_p` arguments) is handled by
    /// `ipc_filter.rs` and not modeled here; this predicate covers only
    /// the RTS-state + source-endpoint match.
    ///
    /// Matches C: `RTS_ISSET(dst, RTS_RECEIVING) && !RTS_ISSET(dst, RTS_SENDING)
    /// && (dst->p_getfrom_e == ANY || dst->p_getfrom_e == src_e)`.
    fn is_willing_to_receive(dst: &KProcess, src: Endpoint) -> bool {
        !dst.p_rts_flags.is_set(RtsFlagsBits::SENDING)
            && dst.p_rts_flags.is_set(RtsFlagsBits::RECEIVING)
            && (dst.p_getfrom_e == Endpoint::ANY || dst.p_getfrom_e == src)
    }

    // ── Deadlock detection ──

    /// Deadlock detection.
    ///
    /// C: `deadlock()` — proc.c:703-768. Follows the `P_BLOCKEDON` chain
    /// starting at `dst_endpoint`. Each step dynamically selects
    /// `p_sendto_e` or `p_getfrom_e` based on the target's RTS flags
    /// (see `blocked_on`). If the chain returns to `caller_nr`, a cycle
    /// is reported.
    ///
    /// # 2-cycle SEND↔RECEIVE exception (proc.c:744-749)
    ///
    /// When the cycle has exactly 2 members (caller ↔ target), C applies
    /// a magic encoding: `RTS_SENDING = 0x04 = 1 << 2`, so
    /// `function << 2` lines up with the SENDING bit. SEND↔RECEIVE
    /// (the normal request/reply pattern) returns "not a deadlock";
    /// SEND↔SEND or RECEIVE↔RECEIVE are deadlocks.
    ///
    /// The caller is responsible for interpreting `direction` and
    /// `group_size` — this function only reports the cycle, mirroring C.
    pub fn detect_deadlock(
        &mut self,
        function: IpcCall,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> Option<DeadlockCycle> {
        // The 2-cycle check uses C's encoding: SEND=1 → 0x04, RECEIVE=2 → 0x08.
        // `function << 2` must align with `RTS_SENDING = 0x04` for the XOR test.
        let direction = match function {
            IpcCall::Send | IpcCall::SendRec | IpcCall::SendNb => DeadlockDirection::Send,
            IpcCall::Receive => DeadlockDirection::Receive,
            _ => return None,
        };

        let caller_idx = self.idx_of(caller_nr)?;
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        let mut chain: [ProcNr; PROC_TABLE_SIZE] = [ProcNr(NONE_PROC_NR); PROC_TABLE_SIZE];
        let mut chain_len: usize = 0;
        chain[chain_len] = caller_nr;
        chain_len += 1;

        let mut current_ep = dst_endpoint;
        let mut group_size: usize = 1;

        loop {
            if current_ep == Endpoint::ANY {
                return None;
            }
            let target_idx = self.idx_by_endpoint(current_ep)?;
            let target_nr = self.procs[target_idx].p_nr;
            group_size += 1;
            chain[chain_len] = target_nr;
            chain_len += 1;

            let next_ep = Self::blocked_on(&self.procs[target_idx])?;

            if next_ep == caller_endpoint {
                if group_size == 2 {
                    // 2-cycle exception. C: `(xp->p_rts_flags ^ (function << 2)) & RTS_SENDING`
                    // — proc.c:746. If the XOR is non-zero on the SENDING bit, this is
                    // SEND↔RECEIVE (normal request/reply) and NOT a deadlock.
                    let xp_rts = self.procs[target_idx].p_rts_flags.get().bits();
                    let function_shifted = (function as u32) << 2;
                    // NK4-C 1.10m 取证探针（task1-close 裁决删除）：2-cycle
                    // 判定现场（caller/fn/xp/xp_rts）——PM send ELOCKED 误报
                    // 定位。
                    #[cfg(not(feature = "mock"))]
                    {
                        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                        static DD_N: AtomicUsize = AtomicUsize::new(0);
                        if DD_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                            Console::write_str("nk4a: dd2 caller=");
                            Console::write_hex(caller_nr.0 as u64);
                            Console::write_str(" fn=");
                            Console::write_hex(function as u64);
                            Console::write_str(" xp=");
                            Console::write_hex(self.procs[target_idx].p_nr.0 as u64);
                            Console::write_str(" xp_rts=0x");
                            Console::write_hex(xp_rts as u64);
                            Console::write_str("\n");
                        }
                    }
                    if (xp_rts ^ function_shifted) & RtsFlagsBits::SENDING.bits() != 0 {
                        return None;
                    }
                }
                // NK4-C 1.10n 逐环仪器化（task1-close 裁决删除）：死锁判定
                // 现场补发双方消息类型——caller 在发的 m_type 与 xp 在发的
                // m_type（p_sendmsg），定位协议互撞的具体消息。
                #[cfg(not(feature = "mock"))]
                {
                    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    static DD2_N: AtomicUsize = AtomicUsize::new(0);
                    if DD2_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                        Console::write_str("nk4a: dd2m caller=");
                        Console::write_hex(caller_nr.0 as u64);
                        Console::write_str(" dst=");
                        Console::write_hex(dst_endpoint.0 as u64);
                        Console::write_str(" cmt=");
                        Console::write_hex(self.procs[caller_idx].p_sendmsg.m_type as u64);
                        Console::write_str(" xp=");
                        Console::write_hex(self.procs[target_idx].p_nr.0 as u64);
                        Console::write_str(" xmt=");
                        Console::write_hex(self.procs[target_idx].p_sendmsg.m_type as u64);
                        Console::write_str("\n");
                    }
                }
                return Some(DeadlockCycle {
                    chain,
                    chain_len,
                    direction,
                    group_size,
                });
            }
            current_ep = next_ep;
        }
    }

    // ── Send ──

    /// Sync send. C: `mini_send()` — proc.c:870-962.
    ///
    /// Flow:
    /// 1. Endpoint validity check (`RTS_NO_ENDPOINT` → `DeadSrcDst`).
    /// 2. If dst is willing to receive → direct delivery (path A).
    /// 3. Else if `NON_BLOCKING` → `NotReady`.
    /// 4. Else deadlock check → `Deadlock` if cycle found.
    /// 5. Else block caller: cache message, set `RTS_SENDING` +
    ///    `p_sendto_e`, enqueue on `dst.caller_q`.
    ///
    /// # BKL
    ///
    /// Caller must hold the BKL. The `&mut self` borrow is the
    /// type-level proxy (see struct doc).
    pub fn send(
        &mut self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
        flags: SendFlags,
    ) -> IpcOutcome {
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        let dst_idx = match self.idx_by_endpoint(dst_endpoint) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };

        // C: `if (RTS_ISSET(dst_ptr, RTS_NO_ENDPOINT)) return EDEADSRCDST;`
        if self.procs[dst_idx].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
            return IpcOutcome::Error(IpcError::DeadSrcDst);
        }

        // Phase 2: WILLRECEIVE check (path A — direct delivery).
        // D-16: C 的 WILLRECEIVE 含 CANRECEIVE（ipc.h:19-22）——被过滤的
        // 消息不投递，发送方落入 Path B（阻塞/排队，proc.c:895→925+），
        // 即被过滤的发送者在队列中等待，与"未命中 receive"同形。
        if Self::is_willing_to_receive(&self.procs[dst_idx], caller_endpoint) {
            // NK4-C 1.10 取证探针（task1-close 裁决删除）：Path A 直投到
            // PM(ep 0) 的现场（src + getfrom + REPLY_PEND）——sched reply
            // 落主循环而非停车 receive 半的定位。
            #[cfg(not(feature = "mock"))]
            if dst_idx == crate::proc_table::nr_to_idx(ProcNr(0)).unwrap_or(usize::MAX) {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                static P4A_N: AtomicUsize = AtomicUsize::new(0);
                if P4A_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                    Console::write_str("nk4a: p4a src=");
                    Console::write_hex(caller_endpoint.0 as u64);
                    Console::write_str(" gf=");
                    Console::write_hex(self.procs[dst_idx].p_getfrom_e.0 as u64);
                    Console::write_str(" rpv=");
                    Console::write_str(
                        if self.procs[dst_idx]
                            .p_misc_flags
                            .is_set(MiscFlagsBits::REPLY_PEND)
                        {
                            "y"
                        } else {
                            "n"
                        },
                    );
                    Console::write_str("\n");
                }
            }
            // C: `copy_msg_from_user` (user path) or direct copy (FROM_KERNEL).
            let m = if !flags.contains(SendFlags::FROM_KERNEL) {
                // User-origin send: route through UserCopy trait.
                // C: proc.c:901-906.
                let user_src = self.procs[caller_idx].p_delivermsg_vir;
                match self.user_copy.copy_msg_from_user(user_src) {
                    Ok(m) => m,
                    Err(_) => return IpcOutcome::Error(IpcError::Fault),
                }
            } else {
                *msg
            };
            if !self.can_receive(dst_idx, caller_endpoint, m.m_type) {
                // Filtered → fall through to Path B (block/queue).
            } else {
            self.procs[dst_idx].p_delivermsg = m;
            self.procs[dst_idx].p_delivermsg.m_source = caller_endpoint;
            self.procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
            if flags.contains(SendFlags::FROM_KERNEL) {
                self.procs[dst_idx].p_misc_flags.set(MiscFlagsBits::SENDING_FROM_KERNEL);
                // C: proc.c:905 — `IPC_STATUS_ADD_FLAGS(dst_ptr, IPC_FLG_MSG_FROM_KERNEL)`
                crate::proc::ipc_status_add_flags(&mut self.procs[dst_idx], IPC_FLG_MSG_FROM_KERNEL);
            }
            // C: proc.c:911-913 — determine call type and add to IPC status.
            //   call = (caller_ptr->p_misc_flags & MF_REPLY_PEND ? SENDREC
            //       : (flags & NON_BLOCKING ? SENDNB : SEND));
            //   IPC_STATUS_ADD_CALL(dst_ptr, call)
            //
            // Rust extension: SENDA flag overrides to IpcCall::SendA, matching
            // C's `mini_senda` direct delivery (proc.c:1287 sets SENDA, not SEND).
            let delivered_call = if flags.contains(SendFlags::SENDA) {
                IpcCall::SendA
            } else if self.procs[caller_idx].p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
                IpcCall::SendRec
            } else if flags.contains(SendFlags::NON_BLOCKING) {
                IpcCall::SendNb
            } else {
                IpcCall::Send
            };
            crate::proc::ipc_status_add_call(&mut self.procs[dst_idx], delivered_call);
            // C: `RTS_UNSET(dst, RTS_RECEIVING)` — wake up target.
            self.procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
            let woken = self.procs[dst_idx].p_nr;
            self.record_wake_target(woken);
            // E1 slice 2: the woken receiver's RECEIVE completes with OK
            // (same completion-path return-code rule as the sender wake).
            crate::proc::set_ipc_return_code(&mut self.procs[dst_idx], OK as i64);
            // NK4-C B12: Path A delivery completing a sendrec's receive half.
            // In C, `mini_sendrec` ends with `MF_CLREPLYPRIV(pr)` (proc.c) that
            // clears MF_REPLY_PEND unconditionally at sendrec completion.  In the
            // Rust async model, the sendrec's receive half completes exactly here
            // when the reply is delivered via Path A to a parked receiver.  Without
            // this clear, a stale REPLY_PEND causes VM drain to mis-park the
            // process on its next page fault (forward_pagefault_to_vm → Path B →
            // VM receive → drain sees REPLY_PEND → parks RECEIVING+getfrom=VM
            // forever).
            self.procs[dst_idx].p_misc_flags.clear(MiscFlagsBits::REPLY_PEND);
            return IpcOutcome::Delivered;
            }
        }

        // Path B: caller must block.
        // C: `if (flags & NON_BLOCKING) return ENOTREADY;` — proc.c:925-927.
        if flags.contains(SendFlags::NON_BLOCKING) {
            return IpcOutcome::Error(IpcError::NotReady);
        }

        // C: `if (deadlock(SEND, caller, dst_e)) return ELOCKED;` — proc.c:930-932.
        if self.detect_deadlock(IpcCall::Send, caller_nr, dst_endpoint).is_some() {
            // NK4-C 1.10n 取证探针（task1-close 裁决删除）：ELOCKED 现场
            // （caller/dst/被拒 send 的 m_type）。
            #[cfg(not(feature = "mock"))]
            {
                use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static EL_N: AtomicUsize = AtomicUsize::new(0);
                if EL_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                    Console::write_str("nk4a: elock caller=");
                    Console::write_hex(caller_nr.0 as u64);
                    Console::write_str(" dst=");
                    Console::write_hex(dst_endpoint.0 as u64);
                    Console::write_str(" mt=");
                    Console::write_hex(msg.m_type as u64);
                    Console::write_str(" src=");
                    Console::write_hex(msg.m_source.0 as u64);
                    // NK4-C 1.10t：互卡双方全量状态——PM 的 rts/getfrom、
                    // sched 的 rts/getfrom/sendto（判定时序错位的直接证据）。
                    if let Some(dp) = self.idx_by_endpoint(dst_endpoint) {
                        Console::write_str(" d_rts=0x");
                        Console::write_hex(self.procs[dp].p_rts_flags.get().bits() as u64);
                        Console::write_str(" d_gf=0x");
                        Console::write_hex(self.procs[dp].p_getfrom_e.0 as u64);
                        Console::write_str(" d_sto=0x");
                        Console::write_hex(self.procs[dp].p_sendto_e.0 as u64);
                    }
                    let c_gf = self.procs[caller_idx].p_getfrom_e.0 as u64;
                    Console::write_str(" c_gf=0x");
                    Console::write_hex(c_gf);
                    let c_rpv = self.procs[caller_idx]
                        .p_misc_flags
                        .is_set(MiscFlagsBits::REPLY_PEND);
                    Console::write_str(" c_rpv=");
                    Console::write_str(if c_rpv { "y" } else { "n" });
                    Console::write_str("\n");
                    // PM 侧栈回溯：定位是 PM 哪个函数在发（cap 2 一次性）。
                    if EL_N.load(AtomicOrd::Relaxed) <= 2
                        && let Some(p) = crate::proc_table_with(
                            &unsafe { crate::smp::BklSection::assume_held() },
                        )
                        .get(caller_nr)
                    {
                        crate::stacktrace::proc_stacktrace(p);
                    }
                }
            }
            return IpcOutcome::Error(IpcError::Deadlock);
        }

        // Cache message + set RTS_SENDING + enqueue.
        // C: proc.c:938-960.
        if !flags.contains(SendFlags::FROM_KERNEL) {
            let user_src = self.procs[caller_idx].p_delivermsg_vir;
            match self.user_copy.copy_msg_from_user(user_src) {
                Ok(m) => self.procs[caller_idx].p_sendmsg = m,
                Err(_) => return IpcOutcome::Error(IpcError::Fault),
            }
        } else {
            self.procs[caller_idx].p_sendmsg = *msg;
            self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::SENDING_FROM_KERNEL);
        }
        self.procs[caller_idx].p_rts_flags.set(RtsFlagsBits::SENDING);
        self.procs[caller_idx].p_sendto_e = dst_endpoint;
        // NK4-C 1.48 取证探针（c39，task1-close 裁决删除）：INIT 发送半
        // 停车现场：dst + pdmv（sendrec 应随后经 SENDING 门停 receive 半）。
        #[cfg(not(feature = "mock"))]
        if self.procs[caller_idx].p_endpoint.0 == 0xb {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: snd-init dst=");
            C0::write_hex(dst_endpoint.0 as u64);
            C0::write_str(" pdmv=");
            C0::write_hex(self.procs[caller_idx].p_delivermsg_vir.0);
            C0::write_str(" mt=");
            C0::write_hex(msg.m_type as u64);
            C0::write_str("\n");
        }
        caller_q_push(self.procs, dst_idx, caller_idx);
        IpcOutcome::Blocked
    }

    // ── Receive ──

    /// Sync receive. C: `mini_receive()` — proc.c:967-1117.
    ///
    /// # P0 FIX (FIX-5) + P0-12-5 (REPLY_PEND semantics)
    ///
    /// Previous implementation was a 30-line stub that skipped all three
    /// message-source checks and unconditionally blocked the caller.
    /// A later attempt added a `MF_REPLY_PEND` shortcut that returned
    /// `Delivered` immediately — but this was a P0 semantic bug: C
    /// (proc.c:999-1005) only skips the *notify* check when
    /// `MF_REPLY_PEND` is set, it still falls through to async + caller_q.
    ///
    /// This implementation follows C's three-tier priority exactly:
    ///
    /// 1. **Pending notifications** (`s_notify_pending` bitmap) — skipped
    ///    when `MF_REPLY_PEND` is set (SENDREC atomicity). C: 1000-1030.
    /// 2. **Pending async messages** (`s_asyn_pending` bitmap) — always
    ///    checked. C: 1031-1070.
    /// 3. **Sync sender queue** (`caller_q`) — first matching sender.
    ///    C: 1071-1095.
    /// 4. None of the above → block caller (`RTS_RECEIVING`).
    ///
    /// Returns:
    /// - `Delivered` — a message was placed in `p_delivermsg`.
    /// - `Blocked` — no matching message; caller is now in `RTS_RECEIVING`.
    pub fn receive(
        &mut self,
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> IpcOutcome {
        // C-3 迭代10 取证：引擎 receive 进入（限 8）。
        #[cfg(not(feature = "mock"))]
        crate::ipc::probe_mark("nk4a: rcv-eng\n");
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };

        // C: proc.c:999-1005 — `MF_REPLY_PEND` only skips the *notify*
        // check. It does NOT skip async or caller_q. The reply for
        // SENDREC arrives via the sender's `mini_send` path, which
        // delivers directly to `p_delivermsg` when the caller is in
        // RECEIVE — that path is handled by Phase 3 (caller_q).
        let reply_pend = self.procs[caller_idx]
            .p_misc_flags
            .is_set(MiscFlagsBits::REPLY_PEND);

        // B24（NK4-C 1.48）：C `mini_receive` 的整段扫描都在
        // `if (!RTS_ISSET(caller_ptr, RTS_SENDING))` 之内（proc.c:999）——
        // 阻塞 SENDREC 的陷入腿（do_ipc SENDREC 臂在 send 半停车后仍
        // fall through 到 mini_receive，proc.c:569-583）带着 SENDING 进来，
        // 跳过 notify/async/caller_q 三面检查直接落到停车臂
        // （getfrom=src_e + RECEIVING）。少了这道门，sendrec 的 receive 半
        // 会在自己请求还在队上时扫 caller_q 抢第三方排队消息冒充 reply
        // （s17j 同型）。真机 c35/c37 实锤的连锁：receive 半从不在陷入腿
        // 停车 → drain 腿代停车但不存 p_delivermsg_vir → 回包投递砸在
        // 从未存值的 0x0 上 → DeliverMsg 挂起 start=0x0 → VM 必失 →
        // 代投者（PM）吃 SIGSEGV 全系统崩。
        if self.procs[caller_idx]
            .p_rts_flags
            .is_set(RtsFlagsBits::SENDING)
        {
            crate::ipc::probe_mark("nk4a: rcv-sndg\n");
            // NK4-C 1.48 取证探针（c39，task1-close 裁决删除）：INIT 经
            // SENDING 门停车（B24 receive 半）的现场：src + pdmv。
            #[cfg(not(feature = "mock"))]
            if self.procs[caller_idx].p_endpoint.0 == 0xb {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                C0::write_str("nk4a: sndg-init src=");
                C0::write_hex(src_endpoint.0 as u64);
                C0::write_str(" pdmv=");
                C0::write_hex(self.procs[caller_idx].p_delivermsg_vir.0);
                C0::write_str("\n");
            }
            self.procs[caller_idx].p_getfrom_e = src_endpoint;
            self.procs[caller_idx]
                .p_rts_flags
                .set(RtsFlagsBits::RECEIVING);
            return IpcOutcome::Blocked;
        }

        // Phase 0: an already-deposited delivery (C: mini_receive 首检
        // MF_DELIVERMSG——proc.c:999 之前，notify/send 先于 receive 到达
        // 时把消息留在 p_delivermsg 并唤醒；重入的 receive 必须先把它
        // 返回，否则接收者带着已投递消息重新阻塞，死等永不到来的唤醒——
        // NK4-A C-3 真机：RS memreq 服务后 VM 唤醒却未返回 SIGKMEM
        // notify，do_memory 永不执行，系统静默，2026-09-22）。
        if self.procs[caller_idx]
            .p_misc_flags
            .is_set(MiscFlagsBits::DELIVERMSG)
        {
            // C-3 迭代10 取证：Phase 0 消费（一次性）。
            #[cfg(not(feature = "mock"))]
            crate::ipc::probe_mark("nk4a: rcv-p0\n");
            self.procs[caller_idx]
                .p_misc_flags
                .clear(MiscFlagsBits::DELIVERMSG);
            return IpcOutcome::Delivered;
        }

        // Phase 1: pending notifications (skipped when MF_REPLY_PEND).
        // C: `has_pending` (NOTIFY) — proc.c:1000-1030.
        if !reply_pend && let Some(notify_src) = self.pick_allowed_notify(caller_nr, src_endpoint) {
            self.build_notify_message(caller_idx, NotifySource::from_caller_nr(notify_src));
            // NK4-C B9（RS boot panic 根因的 notify 同型臂）：沉降 +
            // 同步交付（见 deliver_pending_to_user）。
            self.deliver_pending_to_user(caller_idx);
            // C: proc.c:1033 — `IPC_STATUS_ADD_CALL(caller_ptr, NOTIFY)`
            crate::proc::ipc_status_add_call(&mut self.procs[caller_idx], IpcCall::Notify);
            crate::ipc::probe_mark("nk4a: rcv-p1\n");
            return IpcOutcome::Delivered;
        }

        // Phase 2: pending async messages.
        // C: `has_pending` (ASEND) + `try_async` — proc.c:1031-1070.
        // `try_async` failure (EAGAIN — table empty/endpoint mismatch)
        // falls through to the caller_q check, same as C.
        if let Some(async_src) = self.take_pending_async(caller_nr, src_endpoint)
            && self.deliver_async(caller_idx, async_src, src_endpoint)
        {
            // NK4-C B9（RS boot panic 根因）：沉降 + 同步交付（见
            // deliver_pending_to_user）——deliver_async 只写内核侧
            // p_delivermsg，而 receive 自返腿不经 pick，F14 实锤的
            // 「陈旧缓冲」在异步臂同样成立：VM 的 RS_INIT (mt=0x714)
            // 落到 p_delivermsg，RS 用户缓冲仍是旧内容 (m_type=0) →
            // boot.rs:1254 panic。真机 serial_b8a/b9p2 实锤（b9p2：
            // h2 delivmt=0x714 而 buf64 m_type=0）。
            self.deliver_pending_to_user(caller_idx);
            crate::proc::ipc_status_add_call(&mut self.procs[caller_idx], IpcCall::SendA);
            crate::ipc::probe_mark("nk4a: rcv-p2\n");
            return IpcOutcome::Delivered;
        }

        // Phase 3: sync sender queue. C: proc.c:1071-1095.
        // Intrusive chain walk: `caller_q_find_allowed` (the shared
        // `caller_q_find` walk) scans head-first via
        // `send_q_link`; `caller_q_remove` unlinks (fixing predecessor +
        // head/tail). The old VecDeque split find/remove existed to work
        // around queue-owns-subobject aliasing — with links living in the
        // sender slots, the borrows are ordinary sequential slot accesses.
        // D-16: filtered find — C proc.c:1053-1058 checks CANRECEIVE
        // inside the queue walk (m_src_p = &sender->p_sendmsg, the
        // kernel-cached blocked-send message); a filtered sender stays
        // queued and the scan continues with the next one.
        if let Some(sender_idx) = self.caller_q_find_allowed(caller_idx, src_endpoint) {
            caller_q_remove(self.procs, caller_idx, sender_idx);
            // NK4-C 1.11d（getuid 无回执根因）：同步拷贝前必须盖 m_source。
            // C proc.c:1071-1075 把 sender->p_sendmsg 拷进 p_delivermsg 后
            // `p_delivermsg.m_source = sender->p_endpoint`——发送者的用户
            // 缓冲里 m_source 恒为 0（用户态不填，内核投递时盖章）。F14
            // 同步拷贝绕过 p_delivermsg 直写用户缓冲时漏了这一步：PM 收到
            // init 的 GETUID 但 m_source=0 → 回错槽位 → init 永停 receive
            // 半（s17n `pm 00600` 实锤：mt=6 src=00，而 init 实际端点 0xb）。
            // 拷贝失败回退臂（下方）早已盖章，唯同步臂漏。
            let mut sender_msg = self.procs[sender_idx].p_sendmsg;
            let sender_ep = self.procs[sender_idx].p_endpoint;
            sender_msg.m_source = sender_ep;
            let sender_from_kernel = self.procs[sender_idx]
                .p_misc_flags
                .is_set(MiscFlagsBits::SENDING_FROM_KERNEL);
            // NK4-C F14（449-livelock 根因修复）：排队消息在 drain 时同步
            // 拷入接收者用户缓冲。C proc.c:1071-1095 的语义：receive 入口
            // 已存 m_buff_usr（proc.c:983），队列命中（CANRECEIVE 检查拿的
            // 就是 m_src_p=&sender->p_sendmsg）即把该消息直拷用户缓冲——
            // 一个 drain 恰好交付一条消息。旧实现只做内核侧 p_delivermsg
            // 沉降 + MF_DELIVERMSG，把用户拷贝推迟到接收者下次被挑中时
            // （process_misc_flags DELIVERMSG 臂）；而 receive 是接收者
            // 自己的陷入（完成后直接 restore、不经 pick），沉降的拷贝不会
            // 在返回前发生：接收者拿到 OK 却读到旧缓冲内容，于是立即再收，
            // 连续 drain 把单条 p_delivermsg 反复覆盖，除最后一条外全部
            // 销毁。真机 s14a 实锤：9 台出生服务器的 VM_PAGEFAULT 被
            // 背靠背 drain（p3drain×9）而无一次 dispatch（vm-pf recv 零
            // 新增），9 进程永停 PAGEFAULT(0x400)+to=VM，VM 正常
            // receive(ANY) 空等 → 全系统 449-livelock。
            // 地址空间正确性：Phase 3 只运行在接收者自身的 IPC 陷入里
            // （当前 root 即接收者页表），copy_msg_to_user 走
            // current_root_phys 命中正确地址空间。拷贝失败（目标页未
            // 映射）才退回沉降+DELIVERMSG，由 pick 时 delivermsg 臂
            // （含 VmSuspend 映射重试）兜底——与 C「copy 失败进
            // delivermsg」的分流同形（proc.c:278-282）。
            if self
                .user_copy
                .copy_msg_to_user(self.procs[caller_idx].p_delivermsg_vir, &sender_msg)
                .is_err()
            {
                self.procs[caller_idx].p_delivermsg = sender_msg;
                self.procs[caller_idx].p_delivermsg.m_source = sender_ep;
                self.procs[caller_idx]
                    .p_misc_flags
                    .set(MiscFlagsBits::DELIVERMSG);
            }
            // NK4-C 1.10l（SENDREC 原子性缺口修复）：REPLY_PEND 发送者的
            // send 半被取走后 syscall 未完成——转入 receive 半（RECEIVING +
            // getfrom=ANY 停车，等 REPLY 经 Path A 直投），不写 retreg、
            // 不唤醒。C 对位：blocked sendrec 在 send 半被取走后于自身
            // mini_sendrec 内 `goto receive` 重阻塞等 REPLY（proc.c:
            // 1084-1093）——其 retreg/唤醒只在 REPLY 交付时发生。普通
            // SEND 发送者维持 F15 现状（retreg OK + 唤醒，syscall 完成）。
            let sender_reply_pend = self.procs[sender_idx]
                .p_misc_flags
                .is_set(MiscFlagsBits::REPLY_PEND);
            // Wake up the sender. C: `RTS_UNSET(sender, RTS_SENDING)`.
            self.procs[sender_idx].p_rts_flags.clear(RtsFlagsBits::SENDING);
            let woken_sender = self.procs[sender_idx].p_nr;
            // B24（NK4-C 1.48）：此处曾在 drain 腿代 SENDREC 发送者停
            // receive 半（RECEIVING + getfrom=接收者）—— C 的 receive 半
            // 在发送者自己的陷入腿里早已停完（do_ipc fall-through，
            // proc.c:569-583；上方 B24 修复），drain 腿（proc.c:1069）只
            // `RTS_UNSET(sender, RTS_SENDING)`，不碰 RECEIVING/getfrom。
            // 代停反而制造污染：接收者永远以「自己收过的每个消息源」
            // 覆写停车发送者的 getfrom（真机 c37 实锤：init sendrec(PM)
            // 被 PM drain 停成 gf=0，后又被 VFS drain 覆成 gf=1，PM 真
            // reply 到达时 getfrom 错位）。
            if !sender_reply_pend {
                self.record_wake_target(woken_sender);
            }
            // E1 slice 2 + NK4-C F15 修订：被队列唤醒的发送者的完成码交付
            // 按「发送者是谁」分流。C 的阻塞 send 在停车时即写 retreg=OK
            // （mini_send blocked 臂 return OK，proc.c:960），唤醒处不写
            // （proc.c:1082-1093）；Rust 的 int33 door 对 Blocked「leave
            // RAX untouched」，故真实 IPC 陷入的发送者（SEND/SENDREC）的
            // OK 必须由本 drain 补写——否则恢复后 RAX 是垃圾（F15 首版
            // 删除后 PM↔VFS 握手 NoPerm 回归，s14h）。而 kernel 内部
            // FROM_KERNEL 伪发送者（缺页腿 forward_pagefault_to_vm 的
            // Path B）没有 IPC 陷阱帧——其保存上下文是用户态陷阱现场，
            // 写 OK=0 恰清掉 xchg 的锁地址寄存器：恢复重试读 [0] →
            // cr2=0 → VM noaddr → SIGSEGV（1.9b/c 九进程全灭根因）。
            // ⇒ 门控：SENDING_FROM_KERNEL 的发送者不写（其"syscall"不
            // 经任何返回路径，PF 恢复由 VMCTL_CLEAR_PAGEFAULT 交付）。
            // NK4-C 1.10l：REPLY_PEND（SENDREC）发送者的 syscall 未完成，
            // 不写完成码——其 retreg 由 REPLY 到达时的 Path A 交付写入。
            if !sender_reply_pend
                && !sender_from_kernel
            {
                crate::proc::set_ipc_return_code(&mut self.procs[sender_idx], OK as i64);
            }
            // C: clear `SENDING_FROM_KERNEL` if it was set.
            self.procs[sender_idx].p_misc_flags.clear(MiscFlagsBits::SENDING_FROM_KERNEL);
            // C: proc.c:1070-1071 — determine call type and add to IPC status.
            //   call = (sender->p_misc_flags & MF_REPLY_PEND ? SENDREC : SEND)
            //   IPC_STATUS_ADD_CALL(caller_ptr, call)
            let delivered_call = if self.procs[sender_idx].p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
                IpcCall::SendRec
            } else {
                IpcCall::Send
            };
            crate::proc::ipc_status_add_call(&mut self.procs[caller_idx], delivered_call);
            // C: proc.c:1077-1078 — if message was from kernel, add flag.
            //   if (sender->p_misc_flags & MF_SENDING_FROM_KERNEL)
            //       IPC_STATUS_ADD_FLAGS(caller_ptr, IPC_FLG_MSG_FROM_KERNEL)
            if sender_from_kernel {
                crate::proc::ipc_status_add_flags(&mut self.procs[caller_idx], IPC_FLG_MSG_FROM_KERNEL);
            }
            // Clear MF_REPLY_PEND if this was a SENDREC reply delivery.
            if reply_pend {
                self.procs[caller_idx].p_misc_flags.clear(MiscFlagsBits::REPLY_PEND);
            }
            // C: proc.c:1082-1083 — if (sender->p_misc_flags & MF_SIG_DELAY)
            //   sig_delay_done(sender).
            // The sender is now no longer sending (RTS_SENDING cleared
            // above): if PM had requested a delayed stop on it, this is the
            // quiescent point that ends the delay. We record the sender and
            // let the ProcessTable-level dispatcher run `sig_delay_done`
            // (scheduler-aware `cause_signal`), see
            // `IpcEngine::sig_delay_sender` / `take_sig_delay_sender`.
            if self.procs[sender_idx].p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY) {
                self.sig_delay_sender = Some(self.procs[sender_idx].p_nr);
            }
            crate::ipc::probe_mark("nk4a: rcv-p3\n");
            // NK4-C 449-livelock 取证探针（task1-close 裁决删除）：caller_q
            // drain 命中出生服务器（p_nr 1,3,4,5,6,7,9,10,11）的现场。若
            // 本探针有输出而串口无对应服务器的 vm-pf recv，即 drain 后消息
            // 丢失的直接证据。
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static P3_N: AtomicUsize = AtomicUsize::new(0);
                if matches!(
                    self.procs[sender_idx].p_nr.0,
                    1 | 3 | 4 | 5 | 6 | 7 | 9 | 10 | 11
                ) && P3_N.fetch_add(1, AtomicOrd::Relaxed) < 24
                {
                    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                    C0::write_str("nk4a: p3drain dst=");
                    C0::write_hex(self.procs[caller_idx].p_endpoint.0 as u64);
                    C0::write_str(" snd=");
                    C0::write_hex(self.procs[sender_idx].p_endpoint.0 as u64);
                    C0::write_str(" mt=");
                    C0::write_hex(sender_msg.m_type as u64);
                    C0::write_str("\n");
                }
            }
            return IpcOutcome::Delivered;
        }

        // Phase 4: block. C: proc.c:1096-1110.
        // C-3 迭代10 取证：Phase 4 阻塞到达（一次性）。
        #[cfg(not(feature = "mock"))]
        crate::ipc::probe_mark("nk4a: rcv-p4\n");
        // NK4-C 449-livelock 取证探针（task1-close 裁决删除）：receive 停车
        // 时 caller_q 非空 = Phase 3 刚扫过全队却没取走任何条目（全部被
        // 过滤或幽灵），直接证据。健康系统停车前队列应为空。
        #[cfg(not(feature = "mock"))]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static RBLK_N: AtomicUsize = AtomicUsize::new(0);
            let mut qlen: usize = 0;
            let mut cur = self.procs[caller_idx].caller_q_head;
            while let Some(nr) = cur {
                qlen += 1;
                match nr_to_idx(nr) {
                    Some(i) => cur = self.procs[i].send_q_link,
                    None => break,
                }
            }
            if qlen > 0 && RBLK_N.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                C0::write_str("nk4a: rcvblk ep=");
                C0::write_hex(self.procs[caller_idx].p_endpoint.0 as u64);
                C0::write_str(" qlen=");
                C0::write_hex(qlen as u64);
                C0::write_str("\n");
            }
        }
        self.procs[caller_idx].p_getfrom_e = src_endpoint;
        self.procs[caller_idx]
            .p_rts_flags
            .set(RtsFlagsBits::RECEIVING);
        // NK4-C 1.48 取证探针（c39，task1-close 裁决删除）：INIT 经 Phase 4
        // 普通停车腿的现场：src + pdmv + rts（判「谁把 INIT 停成 gf=0」）。
        #[cfg(not(feature = "mock"))]
        if self.procs[caller_idx].p_endpoint.0 == 0xb {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: p4-init src=");
            C0::write_hex(src_endpoint.0 as u64);
            C0::write_str(" pdmv=");
            C0::write_hex(self.procs[caller_idx].p_delivermsg_vir.0);
            C0::write_str(" rpv=");
            C0::write_str(
                if self.procs[caller_idx]
                    .p_misc_flags
                    .is_set(MiscFlagsBits::REPLY_PEND)
                {
                    "y"
                } else {
                    "n"
                },
            );
            C0::write_str("\n");
        }
        IpcOutcome::Blocked
    }

    /// Pop a pending notification matching `src_endpoint` from the
    /// caller's `s_notify_pending` bitmap.
    ///
    /// C: `has_pending(caller, src_e, ...) -> endpoint_t` — proc.c:967-989.
    /// Returns the source endpoint of the highest-priority pending
    /// notification, or `None` if no match.
    ///
    /// The bit position in `s_notify_pending` is the sender's `priv_id`
    /// (NOT its `proc_nr`). We translate back to endpoint via the
    /// process table.
    fn pick_allowed_notify(
        &mut self,
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Option<ProcNr> {
        let caller_idx = self.idx_of(caller_nr)?;
        let caller_priv_id = self.procs[caller_idx].priv_id?;
        let bitmap = {
            let caller_priv = self.priv_table.get(caller_priv_id)?;
            caller_priv.signals.s_notify_pending
        };
        if bitmap == 0 {
            return None;
        }

        // Scan bits low→high (C order).
        let mut bit = 0u32;
        while bit < 64 {
            if (bitmap & (1u64 << bit)) != 0 {
                // Find the process whose `priv_id` == bit.
                if let Some(sender_idx) = self.procs.iter().position(|p| p.priv_id == Some(bit as u16)) {
                    let sender_ep = self.procs[sender_idx].p_endpoint;
                    if src_endpoint == Endpoint::ANY || src_endpoint == sender_ep {
                        // D-16: CANRECEIVE filter half — C ipc.h:19-22.
                        // Notify messages are kernel-built with
                        // m_type = NOTIFY_MESSAGE (com.h:90).
                        if !self.can_receive(caller_idx, sender_ep, NOTIFY_MESSAGE) {
                            // Filtered → skip this candidate (bit stays
                            // pending), try the next one — C proc.c:1000-1030
                            // iterates bits the same way.
                            bit += 1;
                            continue;
                        }
                        // Clear the bit (C: caller_priv->s_notify_pending &= ~(1<<bit)).
                        if let Some(caller_priv) = self.priv_table.get_mut(caller_priv_id) {
                            caller_priv.signals.s_notify_pending &= !(1u64 << bit);
                        }
                        return Some(self.procs[sender_idx].p_nr);
                    }
                }
            }
            bit += 1;
        }
        None
    }

    /// Pop a pending async message matching `src_endpoint` from the
    /// caller's `s_asyn_pending` bitmap.
    ///
    /// C: `has_pending(caller, src_e, ...) -> endpoint_t` (ASEND branch) —
    /// proc.c:1031-1070. Returns the source endpoint of the highest-priority
    /// pending async message, or `None` if no match.
    ///
    /// The bit position is the sender's `priv_id`, same as notify.
    /// The actual message lives in the sender's user-space SENDA table —
    /// no kernel copy exists; `deliver_async` re-reads it from user
    /// space (C: `try_one`, proc.c:1390-1497).
    fn take_pending_async(
        &mut self,
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Option<Endpoint> {
        let caller_idx = self.idx_of(caller_nr)?;
        let caller_priv_id = self.procs[caller_idx].priv_id?;
        let bitmap = {
            let caller_priv = self.priv_table.get(caller_priv_id)?;
            caller_priv.signals.s_asyn_pending
        };
        // NK4-C 1.12 取证探针（task1-close 裁决删除）：asyn pending 位图
        // 非 0 的每次 receive（caller + 位图 + 请求源）——RS_INIT pending
        // 是否到 VFS、是否被取走。
        #[cfg(not(feature = "mock"))]
        if bitmap != 0 {
            use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static APEND_N: AtomicUsize = AtomicUsize::new(0);
            if APEND_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                Console::write_str("nk4a: apend c=");
                Console::write_hex(caller_nr.0 as u64);
                Console::write_str(" bm=");
                Console::write_hex(bitmap);
                Console::write_str(" src=");
                Console::write_hex(src_endpoint.0 as u64);
                Console::write_str("\n");
            }
        }
        if bitmap == 0 {
            return None;
        }

        // Scan bits low→high (C order).
        let mut bit = 0u32;
        while bit < 64 {
            if (bitmap & (1u64 << bit)) != 0
                && let Some(sender_idx) = self.procs.iter().position(|p| p.priv_id == Some(bit as u16)) {
                    let sender_ep = self.procs[sender_idx].p_endpoint;
                    if src_endpoint == Endpoint::ANY || src_endpoint == sender_ep {
                        // Clear the bit.
                        if let Some(caller_priv) = self.priv_table.get_mut(caller_priv_id) {
                            caller_priv.signals.s_asyn_pending &= !(1u64 << bit);
                        }
                        return Some(sender_ep);
                    }
                }
            bit += 1;
        }
        None
    }

    /// Deliver a pending async message from `sender_ep` to `caller_idx`.
    ///
    /// C: `try_one(ANY, src_ptr, dst_ptr)` — proc.c:1390-1497. Re-reads
    /// the sender's SENDA table from user space (the sender may have
    /// altered entries since `mini_senda` — proc.c:1425-1427), delivers
    /// the first entry aimed at the receiver, and marks that entry
    /// `AMF_DONE` in the user table. When every remaining entry is
    /// done/empty, the sender's table pointer is cleared
    /// (`s_asyntab`/`s_asynsize`, proc.c:1496-1497).
    ///
    /// Returns `true` if a message was delivered. The caller's pending
    /// bit was already cleared by `take_pending_async` (C clears it at
    /// try_one entry, proc.c:1409 — same ordering; C's later early
    /// returns `goto asyn_error` skip the tail re-arm and leave it
    /// cleared — the `size == 0`/endpoint-mismatch guards below mirror
    /// that, proc.c:1411-1412).
    ///
    /// `receive_src` is the source filter of the receive trap that
    /// triggered this call (C: `try_one(src_e, …)` — the `receive_e`
    /// first argument, mini_receive proc.c:1042). NK4-C B8: the
    /// per-entry source check must use it, **not** the caller's
    /// `p_getfrom_e`/`RTS_RECEIVING` — Phase 2 runs inside the
    /// receiver's own receive trap, before the blocking leg (below
    /// `Phase 3`) records those fields.
    fn deliver_async(
        &mut self,
        caller_idx: usize,
        sender_ep: Endpoint,
        receive_src: Endpoint,
    ) -> bool {
        let Some(sender_idx) = self.idx_by_endpoint(sender_ep) else {
            return false;
        };
        let Some(sender_priv_id) = self.procs[sender_idx].priv_id else {
            return false;
        };

        // C: table + size from the sender's privilege structure
        // (proc.c:1405-1406). size == 0 or endpoint mismatch → EAGAIN
        // (proc.c:1411-1412) — nothing to deliver.
        let (table, size, asynendpoint) = {
            let Some(priv_) = self.priv_table.get(sender_priv_id) else {
                return false;
            };
            (priv_.signals.s_asyntab, priv_.signals.s_asynsize, priv_.signals.s_asynendpoint)
        };
        if size == 0 || asynendpoint != sender_ep {
            return false;
        }
        let table = VirBytes(table);
        let sender_ep_final = sender_ep;
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        // C: scan the table (proc.c:1422-1491). Delivery stops at the
        // first entry that matches (break — one message per receive).
        let mut done = true;
        let mut do_notify = false;
        let mut delivered = false;

        // 1.12：A_RETR 按发送者 root 读（deliver_async 跑在接收者的
        // receive 陷入里，current root 是接收者——C 用 umap(sender) 同理）。
        let sender_root = self.procs[sender_idx].p_seg.phys_root;
        for i in 0..size {
            // C: A_RETR(i) — per-entry copy-in; failure skips the entry.
            let (dst_ep, msg, flags) = match self.user_copy.read_senda_entry(sender_root, table, i) {
                Ok(t) => t,
                Err(_) => continue,
            };

            // C: flags == 0 → skip (proc.c:1434).
            if flags == AMF_EMPTY {
                continue;
            }
            // D-16 + NK4-C B8: CANRECEIVE 两半边（C ipc.h:19-22）——
            // 源过滤半边用本次 receive 的 `receive_e` 实参（C try_one
            // proc.c:1457 传的是形参 receive_e，**不是** dst->p_getfrom_e；
            // 读 p_getfrom_e 的是 WILLRECEIVE 宏 ipc.h:14-17，只用于第三方
            // 已阻塞接收者的判定，如 mini_senda 直投臂）。本函数跑在接收
            // 者自己 receive 陷入的 Phase 2，p_getfrom_e/RECEIVING 尚未被
            // 阻塞腿写入（下方 Phase 3 之后才写），旧版误用
            // is_willing_to_receive 读到握手期过期值 → RS_INIT 永拒 →
            // VFS 睡死（B8 boot 死锁）。RECEIVING/SENDING 两谓词在 receive
            // 陷入内部天然成立，不需搬入。
            if !(receive_src == Endpoint::ANY || receive_src == sender_ep_final) {
                continue;
            }
            if !self.can_receive(caller_idx, sender_ep_final, msg.m_type) {
                continue;
            }
            // C: flags validation (proc.c:1436-1441). EINVAL entries
            // fall through to result write-back.
            let invalid = (flags & !AMF_ALL) != 0 || (flags & AMF_VALID) == 0;
            if (flags & AMF_DONE) != 0 {
                // C: already done (proc.c:1441).
                continue;
            }
            // C: done = FALSE — a not-yet-done entry exists (proc.c:1449).
            done = false;

            if invalid {
                // C: goto store_result with r = EINVAL (proc.c:1451-1452).
                let _ = self
                    .user_copy
                    .write_senda_result(sender_root, table, i, EINVAL, flags | AMF_DONE);
                // C: do_notify on NOTIFY / error+NOTIFY_ERR (proc.c:1486-1487).
                if (flags & AMF_NOTIFY) != 0 {
                    do_notify = true;
                }
                break;
            }

            // C: message must be directed at the receiver (proc.c:1455).
            if dst_ep != caller_endpoint {
                continue;
            }
            // C: CANRECEIVE 的源过滤/接收者意愿判定已上移到表扫描入口
            // （receive_src + can_receive 两半，B8）。
            // C: AMF_NOREPLY must not satisfy the receive part of a
            // SENDREC (proc.c:1463-1468).
            let noreply_block = (flags & AMF_NOREPLY) != 0
                && self.procs[caller_idx]
                    .p_misc_flags
                    .is_set(MiscFlagsBits::REPLY_PEND);
            if noreply_block {
                continue;
            }

            // C: deliver (proc.c:1470-1474).
            self.procs[caller_idx].p_delivermsg = msg;
            self.procs[caller_idx].p_delivermsg.m_source = sender_ep_final;
            self.procs[caller_idx]
                .p_misc_flags
                .set(MiscFlagsBits::DELIVERMSG);
            delivered = true;

            // C: store_result (proc.c:1479-1488) — result OK + AMF_DONE.
            let _ = self
                .user_copy
                .write_senda_result(sender_root, table, i, OK, flags | AMF_DONE);
            if (flags & AMF_NOTIFY) != 0 {
                do_notify = true;
            }
            // C: break — one entry per receive (proc.c:1490).
            break;
        }

        if do_notify {
            // C: mini_notify(proc_addr(ASYNCM), src_ptr->p_endpoint)
            // — proc.c:1493-1494. ASYNCM = -5 (com.h:47).
            //
            // Go through `Self::notify` (not the bare `mini_notify_core`)
            // so the ASYNCM-woken sender is recorded via
            // `record_wake_target` and enqueued by the dispatcher's wake
            // drain (NK4-C B6 family: a primitive slice clear leaves it
            // `runnable=yes queued=no`).
            let _ = self.notify(ProcNr(-5), sender_ep_final);
        }

        if done {
            // C: all entries done/empty — clear the table pointer
            // (proc.c:1496-1497).
            if let Some(priv_) = self.priv_table.get_mut(sender_priv_id) {
                priv_.signals.s_asyntab = u64::MAX; // C: (vir_bytes) -1
                priv_.signals.s_asynsize = 0;
            }
        } else {
            // C: try_one 尾部 else 重挂臂（proc.c:1499-1501）：表里仍有
            // 未投递条目（被过滤/被 NOREPLY 拦/单次投递上限未到条目），
            // 把 sender 位在接收者 s_asyn_pending 上重挂回去——
            // take_pending_async 入口已清位（对齐 C proc.c:1409），不重挂
            // 则未投递消息成孤儿（NK4-C B8 必修次缺陷：与 willing 误判
            // 相乘把 RS_INIT 永久吞掉）。
            let receiver_priv_id = self.procs[caller_idx].priv_id;
            if let Some(receiver_priv_id) = receiver_priv_id
                && let Some(cpriv) = self.priv_table.get_mut(receiver_priv_id)
            {
                cpriv.signals.s_asyn_pending |= 1u64 << sender_priv_id;
            }
        }

        delivered
    }

    /// Build a notification message in `dst.p_delivermsg`.
    ///
    /// Delegates to the free function [`build_notify_message`] — see
    /// that function's documentation for the full C reference and
    /// field semantics.
    fn build_notify_message(&mut self, dst_idx: usize, src: NotifySource) {
        build_notify_message(self.procs, self.priv_table, dst_idx, src);
    }

    /// NK4-C B9（RS boot panic 根因修复的共享腿）：把已沉降在
    /// `p_delivermsg` 的消息交付给接收者并清 DELIVERMSG 标志。
    /// receive 是接收者自己的陷入，返回用户态不经 pick，纯沉降的拷贝
    /// （process_misc_flags 的 delivermsg 臂）不会在本次陷阱返回前发生
    /// ——F14 在 Phase 3（caller_q drain）已实锤并修过的「陈旧缓冲」同族
    /// 缺陷，在 Phase 1（notify）/ Phase 2（try_async）继续存在：真机
    /// serial_b8a/b9p2，VM 的 RS_INIT (mt=0x714) 落内核 p_delivermsg，
    /// RS 用户缓冲仍读 m_type=0 → boot.rs:1254 panic。
    /// 先置 MF_DELIVERMSG（pick 兜底路径仍认它，含 VmSuspend 重试），
    /// 同步拷贝成功则清回。C 对位：所有臂统一置 MF_DELIVERMSG，出口
    /// `switch_to_user` 的 `check_misc_flags` 总消费（proc.c:356-365）；
    /// Rust 自返腿需臂内自行交付。地址空间正确性同 Phase 3：本腿只在
    /// 接收者自身的 IPC 陷入里运行（当前 root 即接收者页表）。
    fn deliver_pending_to_user(&mut self, caller_idx: usize) {
        self.procs[caller_idx]
            .p_misc_flags
            .set(MiscFlagsBits::DELIVERMSG);
        let msg = self.procs[caller_idx].p_delivermsg;
        let dst = self.procs[caller_idx].p_delivermsg_vir;
        if self.user_copy.copy_msg_to_user(dst, &msg).is_ok() {
            self.procs[caller_idx]
                .p_misc_flags
                .clear(MiscFlagsBits::DELIVERMSG);
        }
    }

    // ── Notify ──

    /// Async notification. C: `mini_notify()` — proc.c:1122-1167.
    ///
    /// Delegates to [`mini_notify_core`]. See that function for the
    /// full C reference and implementation notes.
    pub fn notify(
        &mut self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> IpcOutcome {
        let outcome = mini_notify_core(self.procs, self.priv_table, caller_nr, dst_endpoint);
        if matches!(outcome, IpcOutcome::Delivered)
            && let Some(dst_idx) = self.idx_by_endpoint(dst_endpoint)
        {
            // Direct notify delivery woke a RECEIVE-blocked dst — record
            // the wake so the ProcessTable-level dispatcher enqueues it
            // (same primitive-clear reason as the send/receive sites).
            let woken = self.procs[dst_idx].p_nr;
            self.record_wake_target(woken);
        }
        outcome
    }

    // ── SendRec ──

    /// Atomic SEND + RECEIVE. C: `do_ipc` 的 SENDREC 臂 — proc.c:569-583
    /// （`mini_send` 后无论送达与否都 fall through 到 `mini_receive`）。
    ///
    /// 1. `send(caller, dst, msg, NONE)`。
    /// 2. If send delivered synchronously (dst was in RECEIVE), proceed
    ///    to `receive(caller, dst)`。
    /// 3. If send blocked caller (SENDING set), still proceed to
    ///    `receive(caller, dst)`：mini_receive 顶部的 SENDING 门（B24）
    ///    让它只落停车臂（getfrom=dst + RECEIVING），与 C 同形。
    ///    `MF_REPLY_PEND` 保留至 reply 到达（跳过 notify 检查）。
    ///
    /// # MF_REPLY_PEND semantics (P0-12-5 fix)
    ///
    /// `MF_REPLY_PEND` only skips the *notify* check in `receive`; it
    /// does NOT skip async or caller_q. The reply arrives via the
    /// sender's `mini_send` path, which delivers directly to
    /// `p_delivermsg` when the caller is in RECEIVE — that path is
    /// handled by `receive` Phase 3 (caller_q).
    pub fn sendrec(
        &mut self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
    ) -> IpcOutcome {
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };

        let send_outcome = self.send(caller_nr, dst_endpoint, msg, SendFlags::NONE);

        match send_outcome {
            IpcOutcome::Delivered => {
                // SEND phase delivered; now receive the reply. C: the
                // SENDREC arm falls through to RECEIVE with the SAME
                // src_dst_e（proc.c:569-583——send 半与 receive 半共用
                // src_dst_e，reply 只能来自 sendrec 目的地）。ANY 会把
                // caller_q 上其它进程的排队请求当 reply 消费（真机 s17j
                // 实锤：PM sendrec(sched) 秒达后 receive(ANY) 抢走 init
                // 的排队请求，sched 真 reply 落进 PM 主循环成野请求，
                // ENOSYS ping-pong livelock）。
                self.receive(caller_nr, dst_endpoint)
            }
            IpcOutcome::Blocked => {
                // B24（NK4-C 1.48）：C do_ipc 的 SENDREC 臂在 mini_send 停车
                // 后仍 fall through 到 mini_receive（proc.c:569-583）——
                // receive 半在发送者自己的陷入腿里就地停车：存
                // p_delivermsg_vir（int33 入口已存 m_ptr）、置
                // getfrom=dst（停车臂）。mini_receive 顶部的 SENDING 门
                // （proc.c:999，上方 B24 注释）保证此处只落停车臂，不会
                // 扫到自己还在队列上的发送半。此前这里直接 return
                // Blocked，把 receive 半推迟到 drain 腿代停——而代停腿
                // 无法存 p_delivermsg_vir（消息指针在发送者陷入帧里），
                // 回包投递因此砸在从未存值的 0x0 上（真机 c35/c37）。
                self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::REPLY_PEND);
                self.receive(caller_nr, dst_endpoint)
            }
            IpcOutcome::Error(e) => IpcOutcome::Error(e),
        }
    }

    // ── SENDA ──

    /// Batch async send. C: `mini_senda()` — proc.c:1331-1342, delegating
    /// to `try_deliver_senda` — proc.c:1200-1326.
    ///
    /// The kernel never caches the table: entries are read from user
    /// space one at a time (`UserCopy::read_senda_entry`, C: `A_RETR` —
    /// proc.c:1244) and per-entry results written back one at a time
    /// (`UserCopy::write_senda_result`, C: `A_INSRT` — proc.c:1307).
    /// Undelivered entries are retried by re-reading the user table
    /// when the target next receives (`deliver_async`, C: `try_one` —
    /// proc.c:1390-1497).
    ///
    /// The caller never blocks. Per-entry errors go into the table's
    /// `result` field (with `AMF_DONE`), not the return value; the
    /// function returns `Delivered` (C: always `OK`) unless a
    /// pre-check fails: non-`SYS_PROC` caller (C: `EPERM`, proc.c:1336)
    /// or the duplicated size sanity check (C: `EDOM`, proc.c:1233;
    /// primary check is the SENDA syscall path — proc.c:681).
    pub fn senda(&mut self, caller_nr: ProcNr, table: VirBytes, size: usize) -> IpcOutcome {
        // NK4-C 1.12 取证探针（task1-close 裁决删除）：senda 入口前 4 次
        // （caller/count）——s18f/g 零 saent：入口未达还是循环前早退。
        #[cfg(not(feature = "mock"))]
        {
            use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static SA_N: AtomicUsize = AtomicUsize::new(0);
            if SA_N.fetch_add(1, AtomicOrd::Relaxed) < 4 {
                Console::write_str("nk4a: sa-in c=");
                Console::write_hex(caller_nr.0 as u64);
                Console::write_str(" n=");
                Console::write_hex(size as u64);
                Console::write_str("\n");
            }
        }
        // C: mini_senda — SYS_PROC check (proc.c:1331-1342).
        macro_rules! sa_exit {
            ($tag:expr, $val:expr) => {{
                #[cfg(not(feature = "mock"))]
                {
                    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    static SAX_N: AtomicUsize = AtomicUsize::new(0);
                    if SAX_N.fetch_add(1, AtomicOrd::Relaxed) < 4 {
                        Console::write_str("nk4a: sa-out ");
                        Console::write_str($tag);
                        Console::write_str("\n");
                    }
                }
                $val
            }};
        }
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return sa_exit!("e1-idx", IpcOutcome::Error(IpcError::DeadSrcDst)),
        };
        let caller_priv_id = match self.procs[caller_idx].priv_id {
            Some(id) => id,
            // C: "caller has no privilege structure" → EPERM (proc.c:1337).
            None => return sa_exit!("e2-priv", IpcOutcome::Error(IpcError::Permission)),
        };
        let caller_is_sys = self
            .priv_table
            .get(caller_priv_id)
            .map(KPriv::is_sys_proc)
            .unwrap_or(false);
        if !caller_is_sys {
            return sa_exit!("e3-nosys", IpcOutcome::Error(IpcError::Permission));
        }
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        // C: clear table first (proc.c:1217-1219); restored only if
        // entries remain undelivered (proc.c:1320-1323).
        {
            let Some(priv_) = self.priv_table.get_mut(caller_priv_id) else {
                return sa_exit!("e4-clr", IpcOutcome::Error(IpcError::Permission));
            };
            priv_.signals.s_asyntab = u64::MAX; // C: (vir_bytes) -1
            priv_.signals.s_asynsize = 0;
            priv_.signals.s_asynendpoint = caller_endpoint;
        }

        // C: size == 0 — nothing to do (proc.c:1221).
        if size == 0 {
            return IpcOutcome::Delivered;
        }

        // C: duplicated size sanity check (proc.c:1233). Same EDOM →
        // BadCall mapping as the syscall path.
        if size > 16 * PROC_TABLE_SIZE {
            return IpcOutcome::Error(IpcError::BadCall);
        }

        let mut done = true;
        let mut do_notify = false;

        for i in 0..size {
            // C: A_RETR(i) — per-entry copy-in. On copy failure C
            // complains and skips the entry (asyn_error has no result
            // write-back); the entry stays un-DONE for retry.
            // 1.12：senda 的 caller 即发送者，root 取其进程表值（与
            // current root 一致，显式传递防漂移）。
            let (mut dst_ep, msg, flags) =
                match self
                    .user_copy
                    .read_senda_entry(self.procs[caller_idx].p_seg.phys_root, table, i)
                {
                Ok(t) => t,
                Err(_) => {
                    // NK4-C 1.12 取证探针：表读失败（errno root/walk 类别）。
                    #[cfg(not(feature = "mock"))]
                    {
                        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                        static SARE_N: AtomicUsize = AtomicUsize::new(0);
                        if SARE_N.fetch_add(1, AtomicOrd::Relaxed) < 4 {
                            Console::write_str("nk4a: sa-readfail root=");
                            Console::write_hex(self.procs[caller_idx].p_seg.phys_root.0);
                            Console::write_str(" tbl=");
                            Console::write_hex(table.0);
                            Console::write_str("\n");
                        }
                    }
                    continue;
                }
            };

            // C: flags == 0 → skip empty entries (proc.c:1248).
            if flags == AMF_EMPTY {
                continue;
            }
            // C: flags must contain only valid bits (proc.c:1251) and
            // must contain a message (proc.c:1255-1258). C's asyn_error
            // path prints and moves on without write-back.
            if (flags & !AMF_ALL) != 0 || (flags & AMF_VALID) == 0 {
                continue;
            }
            // C: AMF_DONE → already processed (proc.c:1259).
            if (flags & AMF_DONE) != 0 {
                continue;
            }

            // C: A_RETR SELF replacement (proc.c:1183-1185).
            if dst_ep == Endpoint::SELF {
                dst_ep = caller_endpoint;
            }

            // C: destination checks (proc.c:1261-1274).
            //   isokendpt fail            → EDEADSRCDST
            //   iskerneln(dst_p)          → ECALLDENIED (no asyn to kernel)
            //   !may_asynsend_to          → ECALLDENIED (IPC mask; self
            //                               always allowed — priv.h:87)
            //   RTS_NO_ENDPOINT on target → EDEADSRCDST
            let mut r: i32 = OK;
            let mut dst_idx_opt: Option<usize> = None;
            if let Some(di) = self.idx_by_endpoint(dst_ep) {
                if self.procs[di].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
                    r = EDEADSRCDST;
                } else if self.procs[di].p_nr.0 <= 0 {
                    // iskerneln: slot number in the task region (C:
                    // `dst_p <= 0` — proc.h iskerneln).
                    r = ECALLDENIED;
                } else {
                    let may = self.procs[di]
                        .priv_id
                        .map(|pid| {
                            self.priv_table
                                .get(caller_priv_id)
                                .map(|cp| cp.may_send_to(pid))
                                .unwrap_or(false)
                        })
                        .unwrap_or(false)
                        || self.procs[di].p_nr == caller_nr;
                    if !may {
                        r = ECALLDENIED;
                    } else {
                        dst_idx_opt = Some(di);
                    }
                }
            } else {
                r = EDEADSRCDST;
            }

            // NK4-C 1.12 取证探针（task1-close 裁决删除）：senda 逐条去向
            // （cap 16，含错误臂）——RS_INIT 落 pending 位还是被错臂吞掉。
            #[cfg(not(feature = "mock"))]
            {
                use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static SAENT_N: AtomicUsize = AtomicUsize::new(0);
                if SAENT_N.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                    Console::write_str("nk4a: saent dst=");
                    match dst_idx_opt {
                        Some(di) => Console::write_hex(self.procs[di].p_endpoint.0 as u64),
                        None => Console::write_str("none"),
                    }
                    Console::write_str(" r=");
                    Console::write_hex(r as u32 as u64);
                    Console::write_str(" mt=");
                    Console::write_hex(msg.m_type as u32 as u64);
                    Console::write_str("\n");
                }
            }

            // C: check if dst is blocked waiting for this message
            // (proc.c:1276-1291). AMF_NOREPLY must not satisfy the
            // receive part of a SENDREC (MF_REPLY_PEND).
            let delivered = match dst_idx_opt {
                Some(di) if r == OK => {
                    // D-16: WILLRECEIVE includes CANRECEIVE (C ipc.h:14-16)
                    // — the entry's message is in hand, so the filter check
                    // is free. Filtered → treated as not-willing → pending
                    // path (no AMF_DONE, retried later).
                    let willing = Self::is_willing_to_receive(&self.procs[di], caller_endpoint)
                        && self.can_receive(di, caller_endpoint, msg.m_type);
                    let noreply_block = (flags & AMF_NOREPLY) != 0
                        && self.procs[di]
                            .p_misc_flags
                            .is_set(MiscFlagsBits::REPLY_PEND);
                    if willing && !noreply_block {
                        // Direct delivery: C: proc.c:1284-1288.
                        self.procs[di].p_delivermsg = msg;
                        self.procs[di].p_delivermsg.m_source = caller_endpoint;
                        self.procs[di]
                            .p_misc_flags
                            .set(MiscFlagsBits::DELIVERMSG);
                        crate::proc::ipc_status_add_call(&mut self.procs[di], IpcCall::SendA);
                        self.procs[di].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
                        let woken_async = self.procs[di].p_nr;
                        self.record_wake_target(woken_async);
                        true
                    } else {
                        // C: set_sys_bit(priv(dst)->s_asyn_pending,
                        // priv(caller)->s_id) — proc.c:1293-1297. The
                        // bit index is the sender's sys_id (priv_id).
                        if let Some(dst_pid) = self.procs[di].priv_id
                            && let Some(dst_priv) = self.priv_table.get_mut(dst_pid)
                        {
                            dst_priv.signals.s_asyn_pending |= 1u64 << caller_priv_id;
                        }
                        done = false;
                        false
                    }
                }
                _ => false,
            };
            if !delivered && r == OK && dst_idx_opt.is_some() {
                // Pending (not delivered, no error) — C: `continue`
                // without result write-back (proc.c:1292-1298).
                continue;
            }
            if delivered {
                // fall through to result write-back with r == OK
            }

            // C: store results (proc.c:1300-1307).
            //   tabent.result = r; tabent.flags = flags | AMF_DONE;
            //   A_INSRT ignores copy errors.
            let _ = self
                .user_copy
                .write_senda_result(
                    self.procs[caller_idx].p_seg.phys_root,
                    table,
                    i,
                    r,
                    flags | AMF_DONE,
                );
            // C: proc.c:1301-1305 — AMF_NOTIFY 恒通知；AMF_NOTIFY_ERR 仅
            // 失败项通知。C 原文两分支同为 `do_notify = TRUE`（宏展开遗留），
            // Rust 重写合并为单一布尔表达式（V12-A3）。
            do_notify |= (flags & AMF_NOTIFY) != 0
                || (r != OK && (flags & AMF_NOTIFY_ERR) != 0);
        }

        if do_notify {
            // C: mini_notify(proc_addr(ASYNCM), caller_ptr->p_endpoint)
            // — proc.c:1317-1318. ASYNCM = -5 (com.h:47).
            //
            // Route through `Self::notify` so the ASYNCM-woken caller is
            // recorded and enqueued by the dispatcher's wake drain
            // (NK4-C B6 family — a bare primitive clear leaves it
            // `runnable=yes queued=no`).
            let _ = self.notify(ProcNr(-5), caller_endpoint);
        }

        if !done {
            // C: proc.c:1320-1323 — remember table for retry.
            if let Some(priv_) = self.priv_table.get_mut(caller_priv_id) {
                priv_.signals.s_asyntab = table.0;
                priv_.signals.s_asynsize = size;
            }
        }

        IpcOutcome::Delivered
    }

    // ── Deliver (delayed copy to user space) ──

    /// Deliver pending message to user space.
    ///
    /// C: `delivermsg()` — proc.c:263-294. Called by `switch_to_user`
    /// before restoring user context.
    ///
    /// # P0 FIX (FIX-7)
    ///
    /// Previous implementation was missing entirely. This version
    /// implements the two-failure policy:
    /// - Success → clear `MF_DELIVERMSG` (+ `MF_MSGFAILED` if set).
    /// - First `PageFault` → set `MF_MSGFAILED`, return `PageFault`
    ///   (caller routes to `vm_suspend(VMS_PAGEFAULT)`).
    /// - Second consecutive failure (`MF_MSGFAILED` already set) →
    ///   return `Segfault` (caller routes to `cause_sig(SIGSEGV)`).
    /// - `OutOfBounds` → `Segfault` immediately.
    ///
    /// # UserCopy injection
    ///
    /// The actual copy is delegated to the injected `UserCopy` impl.
    /// Tests use `KernelUserCopy` (no-op); production code injects the
    /// arch-specific implementation.
    pub fn deliver_message(&mut self, nr: ProcNr) -> DeliverResult {
        let idx = match self.idx_of(nr) {
            Some(i) => i,
            // No process — shouldn't happen (caller checked). Treat as
            // segfault to surface the bug.
            None => return DeliverResult::Segfault,
        };

        // Delegate to the free function `delivermsg` (FIX-20, Phase 1B).
        // This shares the two-failure policy logic with
        // `ProcessTable::process_misc_flags`, which also calls `delivermsg`.
        delivermsg(&mut self.procs[idx], self.user_copy)
    }

    // ── Permission check ──

    /// Check IPC permission. C: `do_sync_ipc` permission layers —
    /// proc.c:479-597.
    ///
    /// # P0 FIX (FIX-9)
    ///
    /// Previous implementation was missing. This version implements
    /// four layers aligned with C:
    /// 1. Endpoint validity (`RTS_NO_ENDPOINT` → `DeadSrcDst`).
    /// 2. IPC whitelist (`s_ipc_to` bitmap → `CallDenied`).
    /// 3. Trap mask (`s_trap_mask` bit for `call` → `TrapDenied`).
    /// 4. Kernel task restriction (only `SendRec` allowed → `TrapDenied`).
    pub fn check_ipc_permission(
        &self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        call: IpcCall,
    ) -> Result<(), IpcError> {
        let caller_idx = self.idx_of(caller_nr).ok_or(IpcError::DeadSrcDst)?;
        let caller_priv_id = self.procs[caller_idx].priv_id.ok_or(IpcError::CallDenied)?;
        let caller_priv = self.priv_table.get(caller_priv_id).ok_or(IpcError::CallDenied)?;

        // C: proc.c:503-541 — ANY is a wildcard ONLY for RECEIVE: it passes
        // through untouched (no endpoint validity check — there is no
        // process behind it to validate); for any other call it is EINVAL.
        // A real endpoint must resolve (`isokendpt` → EDEADSRCDST on miss,
        // NO_ENDPOINT slots included). Without the ANY special case every
        // `receive(ANY)` — the standard server receive form — died here
        // with EDEADSRCDST (observed on real machine: test-sysboot C-27
        // carrier's first blocking receive).
        let is_receive = matches!(call, IpcCall::Receive);
        let dst_idx = if dst_endpoint == minix_types::Endpoint::ANY {
            if !is_receive {
                // C: proc.c:508 — `return EINVAL`.
                return Err(IpcError::Invalid);
            }
            None
        } else {
            let i = self.idx_by_endpoint(dst_endpoint).ok_or(IpcError::DeadSrcDst)?;
            if self.procs[i].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
                return Err(IpcError::DeadSrcDst);
            }
            Some(i)
        };

        // Layer 2: IPC whitelist. C: `may_send_to` — ipc.h — send-family
        // calls only (C: proc.c:527-533, gated on `call_nr != RECEIVE`;
        // a RECEIVE from a specific source is not a "send to" and skips
        // the whitelist).
        if !is_receive
            && let Some(i) = dst_idx
            && let Some(dst_pid) = self.procs[i].priv_id
                && !caller_priv.may_send_to(dst_pid) {
                    return Err(IpcError::CallDenied);
                }

        // Layer 3: trap mask. C: `priv(caller)->s_trap_mask & (1 << call_nr)`
        // — proc.c:552. C's `short s_trap_mask` sign-extends to int for the
        // AND, so `SRV_T = ~0` (stored 0xFFFF) allows SENDA (call_nr=16).
        // The sign extension happens once at the wire boundary
        // (`TrapMask::from_wire` in kpriv.rs), so the stored mask is already
        // in the effective form C checks against. `call as u32` covers
        // SENDA=16 (out of `u16` bit range).
        let call_bit = crate::capability::TrapMask::from_bits(1u32 << (call as u32));
        if !caller_priv.ipc.s_trap_mask.contains(call_bit) {
            return Err(IpcError::TrapDenied);
        }

        // Layer 4: kernel task restriction. C: proc.c:560-566.
        // Calls TO kernel tasks (p_nr < 0) may only be SENDREC or RECEIVE
        // — kernel tasks always reply and may not block if the caller
        // doesn't receive. C checks the TARGET (`iskerneln(src_dst_p)`),
        // not the caller: `call_nr != SENDREC && call_nr != RECEIVE &&
        // iskerneln(src_dst_p)`.
        if call != IpcCall::SendRec
            && call != IpcCall::Receive
            && let Some(i) = dst_idx
            && self.procs[i].is_kernel_task()
        {
            return Err(IpcError::TrapDenied);
        }

        Ok(())
    }

    // ── Top-level dispatch ──

    /// IPC entry point. C: `do_ipc(r1, r2, r3)` — proc.c:599-698.
    ///
    /// Dispatches to `send`/`receive`/`sendrec`/`notify`/`senda` based
    /// on `call`. Performs permission check first (FIX-9).
    ///
    /// # SENDA table passing (P0-12-3 fix)
    ///
    /// C passes the SENDA table pointer and size via trap-frame registers
    /// `r3` and `r2` (proc.c:673, 683), NOT via message fields. The Rust
    /// API mirrors this: `senda_table` is a separate `Option<(ptr, count)>`
    /// parameter, only used when `call == SendA`. The syscall dispatcher
    /// extracts `r2`/`r3` from the trap frame and passes them here.
    ///
    /// # Skeleton
    ///
    /// The full `do_ipc` also handles `MINIX_KERNINFO` (not yet implemented).
    /// IPC status encoding (`IPC_STATUS_ADD_CALL`) is implemented — see
    /// `proc::ipc_status_add_call` / `ipc_status_add_flags`, wired into
    /// all delivery paths (send, receive, mini_notify, senda).
    pub fn do_ipc(
        &mut self,
        caller_nr: ProcNr,
        call: IpcCall,
        dst_endpoint: Endpoint,
        msg: &Message,
        flags: SendFlags,
        senda_table: Option<(VirBytes, usize)>,
    ) -> IpcOutcome {
        // C: do_ipc 的 SENDA 臂（proc.c:673-684）在同步端点/权限校验层
        // 之前拦截——r2 对 SENDA 是表元素个数而非端点，走
        // check_ipc_permission 会对 ANY 伪端点报 EINVAL（C: proc.c:508）。
        // mini_senda 自带 SYS_PROC 与逐条目检查，权限语义自洽。真机
        // NK4-A C-3 c13a 轮：RS boot step2 的 RS_INIT asynsend 到 VM
        // 死于此处（Errno 22 → RS boot failed）。
        if let IpcCall::SendA = call {
            let (table_ptr, count) = match senda_table {
                Some(tc) => tc,
                None => return IpcOutcome::Error(IpcError::BadCall),
            };
            // C: limit size to 16*(NR_TASKS + NR_PROCS) — proc.c:681.
            let max_count = 16 * PROC_TABLE_SIZE;
            if count > max_count {
                // C: returns EDOM — mapped to BadCall (out-of-domain).
                return IpcOutcome::Error(IpcError::BadCall);
            }
            return self.senda(caller_nr, table_ptr, count);
        }

        // Permission check (NOTIFY has relaxed rules in C — the kernel's
        // mini_notify is a kernel-internal function that skips permission
        // checks; user-space SYS_NOTIFY goes through do_ipc's general
        // permission path). Design decision: NOTIFY treated as a kernel-
        // internal signal, not a permission-checked user operation.
        if let Err(e) = self.check_ipc_permission(caller_nr, dst_endpoint, call) {
            return IpcOutcome::Error(e);
        }

        match call {
            IpcCall::Send | IpcCall::SendNb => {
                // NK4-C 1.10 取证探针（task1-close 裁决删除）：进入 sched
                // (ep 4) 的每条 send 前 8 条（caller + m_type）——m_type
                // 0x4e(ENOSYS) 泛滥的发送方定位。
                #[cfg(not(feature = "mock"))]
                if dst_endpoint.0 == 4 {
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                    static S4IN_N: AtomicUsize = AtomicUsize::new(0);
                    if S4IN_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                        Console::write_str("nk4a: s4in c=");
                        Console::write_hex(caller_nr.0 as u64);
                        Console::write_str(" mt=");
                        Console::write_hex(msg.m_type as u32 as u64);
                        Console::write_str("\n");
                    }
                }
                self.send(
                    caller_nr, dst_endpoint, msg,
                    if call == IpcCall::SendNb {
                        flags | SendFlags::NON_BLOCKING
                    } else {
                        flags
                    },
                )
            }
            IpcCall::Receive => {
                // C: proc.c:578-583 — the plain-RECEIVE prologue in
                // `sys_call`: clear MF_REPLY_PEND (SENDREC's protection,
                // already consumed if we got here as a fresh RECEIVE) and
                // IPC_STATUS_CLEAR the status register. The register still
                // carries the user message pointer from trap entry, so
                // without the clear the completion-time OR-merge lands on
                // a pointer and the user-side `is_ipc_notify` check
                // (com.h:92, 6-bit mask) fails — real machine NK4-A C-3
                // 迭代11: VM's SYSTEM SIGKMEM notify surfaced with
                // status = ptr|NOTIFY and was never routed to
                // handle_signal, starving RS on VMREQUEST.
                if let Some(ci) = nr_to_idx(caller_nr) {
                    self.procs[ci].p_misc_flags.clear(MiscFlagsBits::REPLY_PEND);
                    <minix_arch::CurrentCpuContextArch as minix_arch::CpuContextArch>::clear_ipc_status_reg(
                        &mut self.procs[ci].cpu_context,
                    );
                    // NK4-A Task C 第 6 轮判别（task1-close 裁决删除）：
                    // RECEIVE prologue 是「把 0 写进 ctx.rbx」的显式站点，
                    // 只关心 RS（endpoint 2，c23a `pf-save ep=0x2` 实证），
                    // 其余服务器每收一条都打会耗尽上限。
                    #[cfg(not(feature = "mock"))]
                    if self.procs[ci].p_endpoint.0 as u64 == 2 {
                        crate::trap_dispatch::nk4a_rbx_probe(
                            "recv-clear",
                            self.procs[ci].p_endpoint.0 as u64,
                            0,
                            0,
                        );
                    }
                }
                self.receive(caller_nr, dst_endpoint)
            }
            IpcCall::SendRec => {
                // NK4-C 1.10 取证探针：同 s4in，SENDREC 腿（boot taskcall
                // 走这条）。
                #[cfg(not(feature = "mock"))]
                if dst_endpoint.0 == 4 {
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                    static S4R_N: AtomicUsize = AtomicUsize::new(0);
                    if S4R_N.fetch_add(1, AtomicOrd::Relaxed) < 8 {
                        Console::write_str("nk4a: s4r c=");
                        Console::write_hex(caller_nr.0 as u64);
                        Console::write_str(" mt=");
                        Console::write_hex(msg.m_type as u32 as u64);
                        Console::write_str("\n");
                    }
                }
                self.sendrec(caller_nr, dst_endpoint, msg)
            }
            IpcCall::Notify => self.notify(caller_nr, dst_endpoint),
            // SendA was intercepted above the permission layer (C do_ipc
            // switch order, proc.c:673-684 before proc.c:503-541); this arm
            // is unreachable and keeps the match exhaustive.
            IpcCall::SendA => IpcOutcome::Error(IpcError::BadCall),
            // `MINIX_KERNINFO` never reaches the engine: `dispatch_ipc`
            // (syscall.rs) handles it before engine construction, mirroring
            // C where the arm sits in the same outer `do_ipc` switch but
            // returns via the secondary IPC register instead of a message
            // (proc.c:685-693). Defensive arm keeps the match exhaustive.
            IpcCall::KernInfo => IpcOutcome::Error(IpcError::BadCall),
        }
    }
}

/// Convert a logical process number to an array index.
/// Same as `proc_table::nr_to_idx`.
#[inline]
fn nr_to_idx(nr: ProcNr) -> Option<usize> {
    use crate::proc_table::NR_TASKS;
    let offset = nr.0 as isize + NR_TASKS as isize;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}

// ── Free-function notification core (shared by syscall + IRQ paths) ──

/// Notification source type — determines which `MessNotify` fields to fill.
///
/// C: `BuildNotifyMessage` switches on `src` (a ProcNr) and checks for
/// `HARDWARE` (com.h:52, `= KERNEL = -1`) or `SYSTEM` (com.h:50, `= -2`).
/// Rust uses a typed enum instead of magic-number ProcNr comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifySource {
    /// Regular process source — only `timestamp` is filled.
    Process(ProcNr),
    /// Hardware interrupt source (`HARDWARE = KERNEL = -1`).
    /// Fills `interrupts` from `s_int_pending`, then clears it.
    Hardware,
    /// System event source (`SYSTEM = -2`).
    /// Fills `sigset` from `s_sig_pending`, then clears it.
    System,
}

impl NotifySource {
    /// Classify a caller ProcNr into a [`NotifySource`].
    ///
    /// C: `switch (src) { case HARDWARE: ... case SYSTEM: ... }` — proc.c:102-114.
    fn from_caller_nr(caller_nr: ProcNr) -> Self {
        if caller_nr == proc_nr::KERNEL {
            NotifySource::Hardware
        } else if caller_nr == proc_nr::SYSTEM {
            NotifySource::System
        } else {
            NotifySource::Process(caller_nr)
        }
    }
}

/// Build a notification message in `dst.p_delivermsg`.
///
/// C: `BuildNotifyMessage(&dst->p_delivermsg, src_proc_nr, dst_ptr)` —
/// proc.c:98-114 (macro) + proc.c:1029-1030 / 1150-1151 (call sites).
///
/// # C behavior
///
/// 1. `memset(m_ptr, 0, sizeof(*m_ptr))` — zero the message.
/// 2. `m_type = NOTIFY_MESSAGE` (= 0x1000, com.h:90).
/// 3. `m_notify.timestamp = get_monotonic()`.
/// 4. If `src == HARDWARE`: copy `priv(dst)->s_int_pending` →
///    `m_notify.interrupts`, clear `s_int_pending`.
/// 5. If `src == SYSTEM`: copy `priv(dst)->s_sig_pending` →
///    `m_notify.sigset`, clear `s_sig_pending`.
/// 6. For regular process sources: only `timestamp` + `m_type` are set.
///
/// The caller then sets `m_source = sender_endpoint` (proc.c:1030, 1151).
fn build_notify_message(
    procs: &mut [KProcess],
    priv_table: &mut PrivTable,
    dst_idx: usize,
    src: NotifySource,
) {
    // C: memset(m_ptr, 0, ...) — zero the entire message.
    procs[dst_idx].p_delivermsg = Message::default();
    // C: m_type = NOTIFY_MESSAGE — com.h:90.
    procs[dst_idx].p_delivermsg.m_type = NOTIFY_MESSAGE;

    // Fill m_notify payload. Safe because MessNotify is Copy + zeroable.
    // C: m_notify.timestamp = get_monotonic() — I-15: single source, read
    // ClockState directly (the atomic mirrors are deleted). Dispatch-path
    // callers hold the BKL; hosted notify tests are single-threaded.
    let timestamp = unsafe { crate::clock_state_boot_unchecked() }.uptime();
    let mut interrupts = 0u64;
    let mut sigset = 0u64;

    match src {
        NotifySource::Hardware => {
            // C: m_notify.interrupts = priv(dst)->s_int_pending; clear it.
            if let Some(dst_priv_id) = procs[dst_idx].priv_id
                && let Some(dst_priv) = priv_table.get_mut(dst_priv_id) {
                    interrupts = dst_priv.signals.s_int_pending as u64;
                    dst_priv.signals.s_int_pending = 0;
                }
        }
        NotifySource::System => {
            // C: m_notify.sigset = priv(dst)->s_sig_pending; clear it.
            if let Some(dst_priv_id) = procs[dst_idx].priv_id
                && let Some(dst_priv) = priv_table.get_mut(dst_priv_id) {
                    sigset = dst_priv.signals.s_sig_pending.get();
                    dst_priv.signals.s_sig_pending = crate::proc::SigSet::empty();
                }
        }
        NotifySource::Process(_) => {
            // Regular process: only timestamp is set (C falls through switch).
        }
    }

    // Write the MessNotify payload into the message union.
    // SAFETY: MessNotify is #[repr(C)] and fits within MESSAGE_PAYLOAD_SIZE
    // (compile-time asserted in notify.rs). We're writing to a zeroed
    // MessageUnion, so all fields are valid.
    procs[dst_idx].p_delivermsg.m_u.m_notify = MessNotify::new(timestamp, interrupts, sigset);
}

/// Core notification logic — shared by `IpcEngine::notify` (syscall path)
/// and `kernel_mini_notify` (IRQ path).
///
/// C: `mini_notify()` — proc.c:1122-1167.
///
/// Never blocks, never fails (the only error path is invalid dst
/// endpoint, which maps to `Error(DeadSrcDst)`).
///
/// - If dst is in RECEIVE matching caller → deliver directly to
///   `p_delivermsg` + wake dst.
/// - Else set bit in `priv(dst).s_notify_pending` for later delivery.

/// NK4-A C-3 迭代10 取证（task1-close 裁决删除）：限次一次性串口标记。
pub(crate) fn probe_mark(msg: &str) {
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        use minix_plat::EarlyConsole as _;
        static N: AtomicUsize = AtomicUsize::new(0);
        if N.fetch_add(1, AtomicOrd::Relaxed) < 64 {
            minix_plat::CurrentEarlyConsole::write_str(msg);
        }
    }
    #[cfg(feature = "mock")]
    let _ = msg;
}

pub fn mini_notify_core(
    procs: &mut [KProcess],
    priv_table: &mut PrivTable,
    caller_nr: ProcNr,
    dst_endpoint: Endpoint,
) -> IpcOutcome {
    let caller_idx = match nr_to_idx(caller_nr) {
        Some(i) => i,
        None => return IpcOutcome::Error(IpcError::DeadSrcDst),
    };
    let caller_endpoint = procs[caller_idx].p_endpoint;
    let caller_priv_id = procs[caller_idx].priv_id;

    let dst_idx = match procs.iter().position(|p| p.p_endpoint == dst_endpoint) {
        Some(i) => i,
        None => return IpcOutcome::Error(IpcError::DeadSrcDst),
    };

    // Direct delivery if dst is willing to receive from caller.
    if IpcEngine::is_willing_to_receive(&procs[dst_idx], caller_endpoint) {
        let src = NotifySource::from_caller_nr(caller_nr);
        build_notify_message(procs, priv_table, dst_idx, src);
        // C mini_notify（proc.c:1147 对位）：m_source = 调用者端点。
        // build_notify_message 置 Message::default() 后未补 m_source，
        // 通知以 src=NONE(0x7bff) 投递——sef 的 SYSTEM 信号路由
        // （source == SYSTEM_ENDPOINT → on_signal）失配，VM 的
        // SIGKMEM 唤醒被当普通消息丢弃（NK4-A C-3 迭代9 真机：
        // RS 挂起后 VM 收到 src=NONE/type=0 空消息无限自旋、RS 饿死）。
        procs[dst_idx].p_delivermsg.m_source = caller_endpoint;
        procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        // C: proc.c:1154 — `IPC_STATUS_ADD_CALL(dst_ptr, NOTIFY)`
        crate::proc::ipc_status_add_call(&mut procs[dst_idx], IpcCall::Notify);
        procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        return IpcOutcome::Delivered;
    }

    // C: `priv(dst)->s_notify_pending |= (1 << priv_id(caller))` — proc.c:1165.
    // The bit position is the CALLER's priv_id, not proc_nr.
    if let Some(caller_pid) = caller_priv_id
        && (caller_pid as u32) < 64
            && let Some(dst_priv_id) = procs[dst_idx].priv_id
                && let Some(dst_priv) = priv_table.get_mut(dst_priv_id) {
                    dst_priv.signals.s_notify_pending |= 1u64 << caller_pid;
                }
    // If the caller has no priv_id (user process), the notification
    // is silently dropped — matching Minix3 behavior where only
    // system processes have privilege entries. User-process
    // notifications route through PM.
    IpcOutcome::Delivered
}

/// Kernel-level `mini_notify` using global process/privilege tables.
///
/// This is the IRQ-context entry point — called by [`crate::irq_manager`]
/// after a hardware interrupt handler returns. It uses the global
/// `PROC_TABLE` / `PRIV_TABLE` (both `static mut`, BKL-protected).
///
/// C: `mini_notify(proc_addr(HARDWARE), hook->proc_nr_e)` — do_irqctl.c:170.
///
/// # Safety
///
/// Caller must hold the BKL (Big Kernel Lock). This is satisfied by
/// the trap entry path, which acquires the BKL before dispatching.
pub fn kernel_mini_notify(caller_nr: ProcNr, dst_endpoint: Endpoint) -> IpcOutcome {
    // A1 chain root: the trap entry path acquires the BKL before
    // exception/IRQ dispatch and the syscall path holds it throughout
    // (S-8/S-9 will thread real witnesses). Debug builds assert the lock.
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let procs = crate::proc_table_with(&section).procs_slice_mut();
    let priv_table = crate::priv_table_with(&section);
    mini_notify_core(procs, priv_table, caller_nr, dst_endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use crate::proc::RtsFlags;
    use crate::proc_table::NR_TASKS;

    // ── Test helpers ──

    /// ProcNr for a test process placed at `array_idx` in a small slice.
    ///
    /// `nr_to_idx(nr) = nr + NR_TASKS`, so `nr = array_idx - NR_TASKS`.
    /// Using this helper ensures tests exercise the real `nr_to_idx`
    /// mapping rather than bypassing it with unrealistic ProcNr values
    /// (e.g. `KProcess::new(1, ...)` would map to index 6, panicking on
    /// a 2-element slice).
    fn test_nr(array_idx: usize) -> ProcNr {
        ProcNr(array_idx as i32 - NR_TASKS as i32)
    }

    /// Create a test process at `array_idx` with the given endpoint.
    /// The process's `p_nr` is set so that `nr_to_idx(p_nr) == array_idx`.
    fn make_test_proc(array_idx: usize, ep: Endpoint) -> KProcess {
        KProcess::new(test_nr(array_idx), ep)
    }

    // ── Type / encoding tests ──

    #[test]
    fn test_ipc_call_variants() {
        assert_eq!(IpcCall::Send as u8, 1);
        assert_eq!(IpcCall::Receive as u8, 2);
        assert_eq!(IpcCall::SendRec as u8, 3);
        assert_eq!(IpcCall::Notify as u8, 4);
        assert_eq!(IpcCall::SendNb as u8, 5);
        assert_eq!(IpcCall::SendA as u8, 16);
    }

    #[test]
    fn test_ipc_call_from_raw() {
        assert_eq!(IpcCall::from_raw(1), Some(IpcCall::Send));
        assert_eq!(IpcCall::from_raw(16), Some(IpcCall::SendA));
        assert_eq!(IpcCall::from_raw(6), Some(IpcCall::KernInfo)); // ipcconst.h:12
        assert_eq!(IpcCall::from_raw(99), None);
    }

    #[test]
    fn test_ipc_error_variants() {
        let _ = IpcError::Deadlock;
        let _ = IpcError::DeadSrcDst;
        let _ = IpcError::NotReady;
        let _ = IpcError::BadCall;
        let _ = IpcError::Fault;
        let _ = IpcError::CallDenied;
        let _ = IpcError::TrapDenied;
    }

    #[test]
    fn test_ipc_outcome_predicates() {
        assert!(IpcOutcome::Delivered.is_delivered());
        assert!(!IpcOutcome::Delivered.is_blocked());
        assert!(IpcOutcome::Blocked.is_blocked());
        assert_eq!(IpcOutcome::Error(IpcError::Deadlock).err(), Some(IpcError::Deadlock));
        assert_eq!(IpcOutcome::Delivered.err(), None);
    }

    #[test]
    fn test_send_flags_p0_fix_values() {
        // FIX-1 / FIX-2: values must align with C `ipc.h:11-12`.
        assert_eq!(SendFlags::NON_BLOCKING.bits(), 0x0080);
        assert_eq!(SendFlags::FROM_KERNEL.bits(), 0x0100);
        // The previous wrong values must NOT be present.
        assert_ne!(SendFlags::NON_BLOCKING.bits(), 0x01);
        assert_ne!(SendFlags::FROM_KERNEL.bits(), 0x02);

        // bitflags! API works.
        let both = SendFlags::NON_BLOCKING | SendFlags::FROM_KERNEL;
        assert!(both.contains(SendFlags::NON_BLOCKING));
        assert!(both.contains(SendFlags::FROM_KERNEL));
        assert_eq!(SendFlags::NONE, SendFlags::empty());
    }

    // ── Sender wait queue tests (AT-2 / ARCH-2 — intrusive FIFO) ──

    #[test]
    fn test_caller_q_push_find_remove_fifo() {
        // Three slots; queue on slot 2 (target). FIFO order via
        // caller_q_push; find walks head-first; remove unlinks middle.
        let mut procs = crate::test_helpers::scratch_procs([
            make_test_proc(0, Endpoint(11)),
            make_test_proc(1, Endpoint(22)),
            make_test_proc(2, Endpoint(33)),
        ]);
        assert!(caller_q_is_empty(&procs, 2));
        caller_q_push(&mut procs, 2, 0);
        caller_q_push(&mut procs, 2, 1);
        assert_eq!(caller_q_len(&procs, 2), 2);
        // Chain: head=nr(0) → nr(1); tail=nr(1).
        assert_eq!(procs[2].caller_q_head, Some(test_nr(0)));
        assert_eq!(procs[2].caller_q_tail, Some(test_nr(1)));
        assert_eq!(procs[0].send_q_link, Some(test_nr(1)));
        assert_eq!(procs[1].send_q_link, None);
        // ANY matches the head.
        assert_eq!(caller_q_find(&procs, 2, Endpoint::ANY, |_| true), Some(0));
        // Specific endpoint match walks the chain.
        assert_eq!(caller_q_find(&procs, 2, Endpoint(22), |_| true), Some(1));
        // Remove the middle sender: head link must be fixed.
        assert!(caller_q_remove(&mut procs, 2, 1));
        assert_eq!(caller_q_len(&procs, 2), 1);
        assert_eq!(procs[2].caller_q_head, Some(test_nr(0)));
        assert_eq!(procs[2].caller_q_tail, Some(test_nr(0)));
        assert!(procs[1].send_q_link.is_none());
        // Remove the last sender: queue becomes empty.
        assert!(caller_q_remove(&mut procs, 2, 0));
        assert!(caller_q_is_empty(&procs, 2));
        // Removing a non-member returns false.
        assert!(!caller_q_remove_by_nr(&mut procs, 2, test_nr(1)));
    }

    #[test]
    fn test_caller_q_remove_by_nr_unlinks() {
        // remove_by_nr resolves the ProcNr → slot index itself (same
        // walk as C's clear_ipc / abort_proc_ipc_send loops).
        let mut procs = crate::test_helpers::scratch_procs([
            make_test_proc(0, Endpoint(11)),
            make_test_proc(1, Endpoint(22)),
        ]);
        caller_q_push(&mut procs, 1, 0);
        assert!(caller_q_remove_by_nr(&mut procs, 1, test_nr(0)));
        assert!(caller_q_is_empty(&procs, 1));
        assert!(procs[0].send_q_link.is_none());
    }

    // ── Deadlock detection (P0) ──

    #[test]
    fn test_deadlock_no_cycle_empty_table() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, ProcNr(0), Endpoint(1));
        assert!(result.is_none());
    }

    /// Build a 2-proc scenario: caller A, dst B. B is in SENDING or
    /// RECEIVING state and waiting on `b_chain_target`.
    fn build_two_proc_scenario(
        a_ep: Endpoint,
        b_ep: Endpoint,
        b_state: RtsFlagsBits,
        b_chain_target: Endpoint,
    ) -> crate::test_helpers::TestProcArray<2> {
        let mut a = make_test_proc(0, a_ep);
        let mut b = make_test_proc(1, b_ep);
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(b_state);
        match b_state {
            RtsFlagsBits::SENDING => b.p_sendto_e = b_chain_target,
            RtsFlagsBits::RECEIVING => b.p_getfrom_e = b_chain_target,
            _ => panic!("test setup: b_state must be SENDING or RECEIVING"),
        }
        crate::test_helpers::scratch_procs([a, b])
    }

    #[test]
    fn test_deadlock_send_send_two_cycle_is_deadlock() {
        // A sends to B, B is SENDING to A — classic SEND↔SEND deadlock.
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(result.is_some(), "SEND↔SEND 2-cycle must be a deadlock");
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 2);
        assert_eq!(cycle.direction, DeadlockDirection::Send);
    }

    #[test]
    fn test_deadlock_send_receive_two_cycle_not_deadlock() {
        // A sends to B, B is RECEIVING from A — request/reply pattern,
        // NOT a deadlock.
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(
            result.is_none(),
            "SEND↔RECEIVE 2-cycle must NOT be a deadlock (request/reply pattern)"
        );
    }

    #[test]
    fn test_deadlock_receive_send_two_cycle_not_deadlock() {
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Receive, test_nr(0), Endpoint(2));
        assert!(
            result.is_none(),
            "RECEIVE↔SEND 2-cycle must NOT be a deadlock"
        );
    }

    #[test]
    fn test_deadlock_receive_receive_two_cycle_is_deadlock() {
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Receive, test_nr(0), Endpoint(2));
        assert!(
            result.is_some(),
            "RECEIVE↔RECEIVE 2-cycle must be a deadlock"
        );
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 2);
        assert_eq!(cycle.direction, DeadlockDirection::Receive);
    }

    #[test]
    fn test_deadlock_no_cycle_when_target_runnable() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new();
        b.p_getfrom_e = Endpoint::NONE;
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Receive, test_nr(0), Endpoint(2));
        assert!(result.is_none());
    }

    #[test]
    fn test_deadlock_send_state_mismatch() {
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(99),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(result.is_none());
    }

    #[test]
    fn test_deadlock_three_proc_send_cycle() {
        // 3-proc SEND cycle: A → B → C → A.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        let mut c = make_test_proc(2, Endpoint(3));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        b.p_sendto_e = Endpoint(3);
        c.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        c.p_sendto_e = Endpoint(1);
        let mut procs = crate::test_helpers::scratch_procs([a, b, c]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(result.is_some(), "3-proc SEND cycle must be detected");
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 3);
        assert!(cycle.chain[..cycle.chain_len].contains(&test_nr(0)));
        assert!(cycle.chain[..cycle.chain_len].contains(&test_nr(1)));
        assert!(cycle.chain[..cycle.chain_len].contains(&test_nr(2)));
    }

    #[test]
    fn test_deadlock_mixed_chain_cycle() {
        // P0 FIX-3 regression test: mixed-chain deadlock.
        // A send→B, B receive←C, C send→A.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        let mut c = make_test_proc(2, Endpoint(3));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        b.p_getfrom_e = Endpoint(3);
        c.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        c.p_sendto_e = Endpoint(1);
        let mut procs = crate::test_helpers::scratch_procs([a, b, c]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(
            result.is_some(),
            "Mixed-chain deadlock (A send→B, B receive←C, C send→A) must be detected \
             by dynamic blocked_on() — FIX-3 regression"
        );
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 3);
    }

    // ── Send tests (P0-12-1) ──

    #[test]
    fn test_send_when_target_receiving() {
        // Path A: dst is in RECEIVE matching caller → direct delivery.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        b.p_getfrom_e = Endpoint(1);
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert!(outcome.is_delivered(), "send to receiving target must deliver");
        // dst.p_delivermsg should be set + MF_DELIVERMSG.
        assert!(procs[1].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        // dst should be woken (RTS_RECEIVING cleared).
        assert!(!procs[1].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_send_when_target_not_receiving() {
        // Path B: dst not in RECEIVE → caller blocks, enqueued.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert!(outcome.is_blocked(), "send to non-receiving target must block caller");
        // caller should have RTS_SENDING set.
        assert!(procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
        // caller should be enqueued on dst's caller_q (intrusive chain).
        assert_eq!(caller_q_len(&procs, 1), 1);
        assert_eq!(procs[1].caller_q_head, Some(test_nr(0)));
    }

    #[test]
    fn test_send_non_blocking_returns_not_ready() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::NON_BLOCKING);
        assert_eq!(outcome.err(), Some(IpcError::NotReady));
        // caller should NOT be blocked.
        assert!(!procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
    }

    #[test]
    fn test_send_detects_deadlock() {
        // A sends to B, B is SENDING to A → SEND↔SEND deadlock.
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert_eq!(outcome.err(), Some(IpcError::Deadlock));
    }

    // ── Receive tests (P0-12-1 + P0-12-5) ──

    #[test]
    fn test_receive_picks_notify_first() {
        // Phase 1: pending notify is delivered before async/caller_q.
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // Assign priv slots so notify bitmap can be set.
        priv_table.assign_static(ProcNr(-4)).unwrap();
        priv_table.assign_static(ProcNr(-3)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(0).unwrap(); // nr=-4 (assuming NR_TASKS=4)
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(0);
            let b = procs.get_mut(1).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(1);
        }
        let _b_ep = pt.procs_slice()[1].p_endpoint;
        let b_nr = pt.procs_slice()[1].p_nr;
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        // Set notify pending bit 0 (caller A's priv_id) on B.
        {
            let b_priv = engine.priv_table.get_mut(1).unwrap();
            b_priv.signals.s_notify_pending |= 1u64 << 0;
        }
        let outcome = engine.receive(b_nr, Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick pending notify first");
    }

    #[test]
    fn test_receive_skips_notify_when_reply_pend() {
        // MF_REPLY_PEND set → notify check skipped, falls through to caller_q.
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        priv_table.assign_static(ProcNr(-4)).unwrap();
        priv_table.assign_static(ProcNr(-3)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(0).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(0);
            let b = procs.get_mut(1).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(1);
            // Set MF_REPLY_PEND — should skip notify.
            b.p_misc_flags.set(MiscFlagsBits::REPLY_PEND);
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        // Set notify pending bit on B.
        {
            let b_priv = engine.priv_table.get_mut(1).unwrap();
            b_priv.signals.s_notify_pending |= 1u64 << 0;
        }
        let b_nr = engine.procs[1].p_nr;
        let outcome = engine.receive(b_nr, Endpoint::ANY);
        // No caller_q entries → should block (notify was skipped).
        assert!(outcome.is_blocked(), "MF_REPLY_PEND must skip notify");
    }

    #[test]
    fn test_do_ipc_receive_prologue_clears_reply_pend_and_status() {
        // C: proc.c:578-583 — the plain-RECEIVE prologue clears
        // MF_REPLY_PEND and IPC_STATUS_CLEARs the status register (which
        // still carries the user message pointer from trap entry). The
        // completion-time OR-merge must land on a clean word or the
        // user-side is_ipc_notify check fails.
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_rts_flags = RtsFlags::new();
        a.priv_id = Some(0);
        a.p_misc_flags.set(MiscFlagsBits::REPLY_PEND);
        // Poison the status register the way the wrapper leaves it: the
        // message pointer is still in there at trap entry.
        <minix_arch::CurrentCpuContextArch as minix_arch::CpuContextArch>::or_ipc_status_reg(
            &mut a.cpu_context,
            0x7fff_0000,
        );
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &SuccessCopy);
        // Admit the RECEIVE trap through the permission layer (fresh test
        // priv slots have an empty trap mask; C user servers boot with
        // SRV_T = ~0).
        engine.priv_table.get_mut(0).unwrap().ipc.s_trap_mask = crate::capability::TrapMask::ALL;
        let outcome = engine.do_ipc(
            test_nr(0),
            IpcCall::Receive,
            Endpoint::ANY,
            &Message::default(),
            SendFlags::NONE,
            None,
        );
        assert!(outcome.is_blocked());
        // SENDREC's protection is consumed by a fresh plain RECEIVE.
        assert!(
            !engine.procs[0]
                .p_misc_flags
                .is_set(MiscFlagsBits::REPLY_PEND)
        );
        // The status register was cleared: a fresh completion-time merge of
        // NOTIFY (4) reads back as exactly 4 (no pointer bits).
        <minix_arch::CurrentCpuContextArch as minix_arch::CpuContextArch>::or_ipc_status_reg(
            &mut engine.procs[0].cpu_context,
            4,
        );
        assert_eq!(
            minix_arch::ipc_status_register(&engine.procs[0].cpu_context),
            4
        );
    }

    #[test]
    fn test_receive_picks_caller_q_last() {
        // Phase 3: when no notify/async, pick from caller_q.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = Message::default();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        // Enqueue a on b's queue (intrusive link in a's slot).
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick caller_q sender");
        // Sender should be woken (RTS_SENDING cleared).
        assert!(!procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
        // caller_q should be empty after removal.
        assert!(caller_q_is_empty(&procs, 1));
    }

    /// C: proc.c:1082-1083 — when the receiver takes a message from a
    /// sender with `MF_SIG_DELAY` set, the engine records the sender so the
    /// `ProcessTable`-level dispatcher can run `sig_delay_done` (which
    /// needs the scheduler-aware `rts_set`). The flag itself is left for
    /// `sig_delay_done` to clear.
    #[test]
    fn test_receive_sig_delay_sender_records_pending_delay() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = Message::default();
        // PM requested a delayed stop (RC_DELAY) while `a` was sending.
        a.p_misc_flags.set(MiscFlagsBits::SIG_DELAY);
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);

        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick caller_q sender");

        // Sender's delay-end is now due: recorded for the ProcessTable-level
        // dispatcher (dispatch_ipc), exactly once.
        assert_eq!(
            engine.take_sig_delay_sender(),
            Some(test_nr(0)),
            "MF_SIG_DELAY sender must be reported"
        );
        assert_eq!(engine.take_sig_delay_sender(), None, "record is one-shot");
        // The flag stays set — sig_delay_done (ProcessTable level) clears it.
        assert!(procs[0].p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY));
    }

    /// NK4-C F14（449-livelock 根因修复）判别测试：Phase 3 drain 必须把
    /// 排队消息同步拷入接收者用户缓冲（C proc.c:1071-1095，m_buff_usr 直
    /// 拷），且成功时**不**沉降 p_delivermsg/DELIVERMSG——沉降路径把用户
    /// 拷贝推迟到接收者下次被挑中（process_misc_flags DELIVERMSG 臂），
    /// 而 receive 是接收者自己的陷入、返回不经 pick：连续 drain 会把单条
    /// p_delivermsg 反复覆盖，除最后一条外全部销毁（s14a 实锤：9 台服务
    /// 器的 VM_PAGEFAULT 被 drain 后零 dispatch，永停 PAGEFAULT）。
    /// 拷贝失败（目标页未映射）才允许沉降+DELIVERMSG 兜底。
    #[test]
    fn test_receive_caller_q_drain_copies_to_user_buffer_not_deposit() {
        use core::cell::Cell as CoreCell;
        struct SyncCopyOk {
            copied: CoreCell<i32>,
        }
        impl UserCopy for SyncCopyOk {
            fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> {
                Ok(Message::default())
            }
            fn copy_msg_to_user(&self, _dst: VirBytes, msg: &Message) -> Result<(), CopyError> {
                self.copied.set(msg.m_type);
                Ok(())
            }
            fn read_senda_entry(
                &self,
                _root: minix_types::PhysBytes,
                _table: VirBytes,
                _index: usize,
            ) -> Result<(Endpoint, Message, i32), CopyError> {
                Err(CopyError::PageFault)
            }
            fn write_senda_result(
                &self,
                _root: minix_types::PhysBytes,
                _t: VirBytes,
                _i: usize,
                _r: i32,
                _f: i32,
            ) -> Result<(), CopyError> {
                Ok(())
            }
        }
        struct SyncCopyFault;
        impl UserCopy for SyncCopyFault {
            fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> {
                Ok(Message::default())
            }
            fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> {
                Err(CopyError::PageFault)
            }
            fn read_senda_entry(
                &self,
                _root: minix_types::PhysBytes,
                _table: VirBytes,
                _index: usize,
            ) -> Result<(Endpoint, Message, i32), CopyError> {
                Err(CopyError::PageFault)
            }
            fn write_senda_result(
                &self,
                _root: minix_types::PhysBytes,
                _t: VirBytes,
                _i: usize,
                _r: i32,
                _f: i32,
            ) -> Result<(), CopyError> {
                Ok(())
            }
        }

        // 成功拷贝形态：消息走用户缓冲，接收者无 DELIVERMSG 沉降。
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = {
            let mut m = Message::default();
            m.m_type = 0xCFF;
            m
        };
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let copy = SyncCopyOk {
            copied: CoreCell::new(0),
        };
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &copy);
        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered());
        assert_eq!(
            copy.copied.get(),
            0xCFF,
            "queued message must be copied to the user buffer at drain time"
        );
        assert!(
            !procs[1].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG),
            "successful sync copy must not deposit into p_delivermsg"
        );

        // 拷贝失败形态：退回沉降 + DELIVERMSG（delivermsg 臂兜底）。
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = Message::default();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &SyncCopyFault);
        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered());
        assert!(
            procs[1].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG),
            "copy failure must fall back to the deposit + DELIVERMSG arm"
        );
    }

    /// C: proc.c:1082-1083 — a sender *without* `MF_SIG_DELAY` must not be
    /// reported (no delayed stop to end).
    #[test]
    fn test_receive_plain_sender_has_no_pending_delay() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = Message::default();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);

        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick caller_q sender");
        assert_eq!(
            engine.take_sig_delay_sender(),
            None,
            "plain sender has no stop-delay to end"
        );
    }

    #[test]
    fn test_receive_blocks_when_no_match() {
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.receive(test_nr(0), Endpoint::ANY);
        assert!(outcome.is_blocked(), "receive with no match must block");
        assert!(procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_receive_picks_async_second() {
        // Phase 2: pending async message delivered after notify check.
        //
        // Realistic state (C: proc.c:1039-1050 → try_one, 1390-1497): the
        // sender holds a live SENDA table (s_asyntab/s_asynsize on its
        // priv) and the receiver carries the sender's bit in
        // s_asyn_pending. receive → deliver_async re-reads the table from
        // "user space" (SuccessCopy: one VALID entry aimed at Endpoint(2))
        // and delivers.
        //
        // Targets must be user-region slots (p_nr > 0): SENDA to task
        // slots is ECALLDENIED in C (iskerneln, proc.c:1266).
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        let a_endpoint = pt.get(ProcNr(1)).unwrap().p_endpoint; // Endpoint(1)
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(b_priv);
        }
        // Sender's pending SENDA table (C: proc.c:1405-1406 re-reads these).
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.signals.s_asyntab = 0x1000;
            p.signals.s_asynsize = 1;
            p.signals.s_asynendpoint = a_endpoint;
        }
        // Receiver's async-pending bitmap carries the sender's priv bit.
        priv_table.get_mut(b_priv).unwrap().signals.s_asyn_pending |= 1u64 << a_priv;

        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SuccessCopy);
        let outcome = engine.receive(ProcNr(2), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick pending async");
        let b_idx = nr_to_idx(ProcNr(2)).unwrap();
        // NK4-C B9：receive 自返腿同步拷入用户缓冲成功（SuccessCopy）后
        // 必须清 DELIVERMSG——若仍沉降，接收者用户态读到旧缓冲内容
        // （真机 RS boot.rs:1254 panic 根因）。消息内容留在
        // p_delivermsg（供失败回退腿/pick 兜底），m_source 已盖章。
        assert!(
            !engine.procs[b_idx]
                .p_misc_flags
                .is_set(MiscFlagsBits::DELIVERMSG),
            "successful sync copy must not leave the message deposited"
        );
        assert_eq!(engine.procs[b_idx].p_delivermsg.m_source, a_endpoint);
    }

    // ── Notify tests (P0-12-1) ──

    #[test]
    fn test_notify_delivers_when_target_receiving() {
        let mut pt = crate::test_helpers::test_proc_table();
        {
            let a = pt.get_mut(ProcNr(-4)).unwrap();
            a.p_rts_flags = RtsFlags::new();
        }
        {
            let b = pt.get_mut(ProcNr(-3)).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
        }
        let b_endpoint = pt.get(ProcNr(-3)).unwrap().p_endpoint;
        let procs = pt.procs_slice_mut();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),b_endpoint);
        assert!(result.is_delivered(), "notify should deliver");
        let b_idx = nr_to_idx(ProcNr(-3)).unwrap();
        assert!(engine.procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!engine.procs[b_idx].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    /// E1 slice 2: when a send completes a blocked receiver, the woken
    /// caller's saved-context RAX must carry the syscall return code (OK).
    /// C: the completion path sets the return before marking runnable —
    /// without this a resumed caller observes stale trap-entry RAX.
    #[test]
    fn test_send_to_blocked_receiver_writes_ok_return_code() {
        let mut pt = crate::test_helpers::test_proc_table();
        {
            let a = pt.get_mut(ProcNr(-4)).unwrap();
            a.p_rts_flags = RtsFlags::new();
        }
        {
            let b = pt.get_mut(ProcNr(-3)).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
        }
        let b_endpoint = pt.get(ProcNr(-3)).unwrap().p_endpoint;
        let procs = pt.procs_slice_mut();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4), b_endpoint);
        assert!(result.is_delivered(), "notify should deliver");
        let b_idx = nr_to_idx(ProcNr(-3)).unwrap();
        let code = minix_arch::ipc_return_code(&engine.procs[b_idx].cpu_context);
        assert_eq!(code as i32, OK, "woken receiver must see return code OK");
    }

    #[test]
    fn test_notify_records_bitmap_when_not_receiving() {
        let mut pt = crate::test_helpers::test_proc_table();
        {
            let a = pt.get_mut(ProcNr(-4)).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(0);
        }
        {
            let b = pt.get_mut(ProcNr(-3)).unwrap();
            b.p_rts_flags = RtsFlags::new();
            b.priv_id = Some(1);
        }
        let b_endpoint = pt.get(ProcNr(-3)).unwrap().p_endpoint;
        let procs = pt.procs_slice_mut();
        let mut priv_table = crate::test_helpers::test_priv_table();
        priv_table.assign_static(ProcNr(-4)).unwrap();
        priv_table.assign_static(ProcNr(-3)).unwrap();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),b_endpoint);
        assert!(result.is_delivered());
        let dst_priv = engine.priv_table.get(1).unwrap();
        assert_ne!(
            dst_priv.signals.s_notify_pending & (1u64 << 0), 0,
            "s_notify_pending bit 0 (caller's priv_id) should be set"
        );
    }

    #[test]
    fn test_notify_never_blocks() {
        // Notify to a non-existent endpoint returns Error, not Blocked.
        let mut pt = crate::test_helpers::test_proc_table();
        let procs = pt.procs_slice_mut();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),Endpoint(99999));
        assert!(!result.is_blocked(), "notify must never block");
    }

    // ── Deliver message tests (P0-12-1) ──

    /// A UserCopy impl that always succeeds. SENDA reads return a
    /// single VALID entry aimed at `Endpoint(2)`.
    struct SuccessCopy;
    impl UserCopy for SuccessCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
        fn read_senda_entry(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> {
            Ok((Endpoint(2), Message::default(), AMF_VALID))
        }
        fn write_senda_result(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize, _result: i32, _flags: i32) -> Result<(), CopyError> { Ok(()) }
    }

    // ── D-16: receive 侧过滤集成测试 ─────────────────────────────────

    /// D-16: whitelist filter（只允许 A）挂在接收方 R 上——A 的消息照常
    /// 直投（Path A 放行），B 的消息被过滤 → B 阻塞入队且**留在队列中**
    /// （C proc.c:1053-1058 的 CANRECEIVE 失败 = 扫描继续，非错误）。
    #[test]
    fn test_d16_whitelist_allows_listed_blocks_unlisted() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let (r_priv, a_priv, b_priv) = (
            priv_table.assign_static(ProcNr(4)).unwrap(),
            priv_table.assign_static(ProcNr(5)).unwrap(),
            priv_table.assign_static(ProcNr(6)).unwrap(),
        );
        let (r_ep, a_ep, b_ep) = (Endpoint(0x30), Endpoint(0x31), Endpoint(0x32));
        {
            let procs = pt.procs_slice_mut();
            for (nr, ep) in [(ProcNr(4), r_ep), (ProcNr(5), a_ep), (ProcNr(6), b_ep)] {
                let p = procs.get_mut(nr_to_idx(nr).unwrap()).unwrap();
                p.p_rts_flags = RtsFlags::new();
                p.p_endpoint = ep;
            }
            procs[nr_to_idx(ProcNr(4)).unwrap()].priv_id = Some(r_priv);
            procs[nr_to_idx(ProcNr(5)).unwrap()].priv_id = Some(a_priv);
            procs[nr_to_idx(ProcNr(6)).unwrap()].priv_id = Some(b_priv);
            let r = &mut procs[nr_to_idx(ProcNr(4)).unwrap()];
            r.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            r.p_getfrom_e = Endpoint::ANY;
        }
        let wl = pool.allocate(crate::ipc_filter::IpcFilterType::Whitelist).unwrap();
        {
            let slot = pool.get_mut(wl).unwrap();
            slot.num_elements = 1;
            slot.elements[0] = crate::ipc_filter::IpcFilterElement {
                flags: crate::ipc_filter::IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: a_ep.0,
                m_type: 0,
            };
        }
        priv_table.get_mut(r_priv).unwrap().mem.s_ipcf = Some(wl);

        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy)
            .with_filter_pool(&pool);

        let outcome = engine.send(ProcNr(5), r_ep, &Message::default(), SendFlags::empty());
        assert!(outcome.is_delivered(), "whitelisted sender must deliver");
        assert_eq!(engine.procs[nr_to_idx(ProcNr(4)).unwrap()].p_delivermsg.m_source, a_ep);
        {
            let r = &mut engine.procs[nr_to_idx(ProcNr(4)).unwrap()];
            r.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            r.p_rts_flags.set(RtsFlagsBits::RECEIVING);
        }

        let outcome = engine.send(ProcNr(6), r_ep, &Message::default(), SendFlags::empty());
        assert!(outcome.is_blocked(), "filtered sender blocks like an unmatched send");
        let outcome = engine.receive(ProcNr(4), Endpoint::ANY);
        assert!(outcome.is_blocked());
        assert!(!engine.procs[nr_to_idx(ProcNr(4)).unwrap()]
            .p_misc_flags
            .is_set(MiscFlagsBits::DELIVERMSG));
        assert!(engine.procs[nr_to_idx(ProcNr(6)).unwrap()]
            .p_rts_flags
            .is_set(RtsFlagsBits::SENDING));
    }

    /// D-16: 被过滤的 notify 保持 pending（位不清除）——C proc.c:1013 的
    /// CANRECEIVE 逐位检查：不过滤的候选照常投递，过滤的候选留待 filter
    /// 变化后的下一轮 receive。
    #[test]
    fn test_d16_notify_filtered_stays_pending() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let (r_priv, n_priv) = (
            priv_table.assign_static(ProcNr(4)).unwrap(),
            priv_table.assign_static(ProcNr(5)).unwrap(),
        );
        let (r_ep, n_ep) = (Endpoint(0x30), Endpoint(0x31));
        {
            let procs = pt.procs_slice_mut();
            let r = procs.get_mut(nr_to_idx(ProcNr(4)).unwrap()).unwrap();
            r.p_rts_flags = RtsFlags::new();
            r.p_endpoint = r_ep;
            r.priv_id = Some(r_priv);
            r.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            r.p_getfrom_e = Endpoint::ANY;
            let n = procs.get_mut(nr_to_idx(ProcNr(5)).unwrap()).unwrap();
            n.p_rts_flags = RtsFlags::new();
            n.p_endpoint = n_ep;
            n.priv_id = Some(n_priv);
        }
        let wl = pool.allocate(crate::ipc_filter::IpcFilterType::Whitelist).unwrap();
        {
            let slot = pool.get_mut(wl).unwrap();
            slot.num_elements = 1;
            slot.elements[0] = crate::ipc_filter::IpcFilterElement {
                flags: crate::ipc_filter::IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: 0x99,
                m_type: 0,
            };
        }
        priv_table.get_mut(r_priv).unwrap().signals.s_notify_pending = 1u64 << n_priv;
        priv_table.get_mut(r_priv).unwrap().mem.s_ipcf = Some(wl);

        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy)
            .with_filter_pool(&pool);

        let outcome = engine.receive(ProcNr(4), Endpoint::ANY);
        assert!(outcome.is_blocked(), "filtered notify → no delivery → block");
        assert_eq!(
            engine.priv_table.get(r_priv).unwrap().signals.s_notify_pending,
            1u64 << n_priv
        );
        assert!(!engine.procs[nr_to_idx(ProcNr(4)).unwrap()]
            .p_misc_flags
            .is_set(MiscFlagsBits::DELIVERMSG));
    }

    /// D-16: 被过滤的 SENDA 表项走 pending 路径——无结果写回（非
    /// AMF_DONE，留待重试），目标置 s_asyn_pending 位（C proc.c:1280 的
    /// WILLRECEIVE 含 CANRECEIVE；proc.c:1293-1297 的 pending 分支）。
    #[test]
    fn test_d16_senda_filtered_entry_stays_pending() {
        use core::cell::Cell;
        struct RecordingCopy {
            delivered: Cell<bool>,
        }
        impl UserCopy for RecordingCopy {
            fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
            fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
            fn read_senda_entry(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> {
                Ok((Endpoint(0x30), Message::default(), AMF_VALID))
            }
            fn write_senda_result(&self, _root: minix_types::PhysBytes, _t: VirBytes, _i: usize, _r: i32, _f: i32) -> Result<(), CopyError> {
                self.delivered.set(true);
                Ok(())
            }
        }

        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut pool = crate::ipc_filter::IpcFilterPool::new();
        let (r_priv, b_priv) = (
            priv_table.assign_static(ProcNr(4)).unwrap(),
            priv_table.assign_static(ProcNr(6)).unwrap(),
        );
        let (r_ep, b_ep) = (Endpoint(0x30), Endpoint(0x32));
        {
            let procs = pt.procs_slice_mut();
            let r = procs.get_mut(nr_to_idx(ProcNr(4)).unwrap()).unwrap();
            r.p_rts_flags = RtsFlags::new();
            r.p_endpoint = r_ep;
            r.priv_id = Some(r_priv);
            r.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            r.p_getfrom_e = Endpoint::ANY;
            let b = procs.get_mut(nr_to_idx(ProcNr(6)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::new();
            b.p_endpoint = b_ep;
            b.priv_id = Some(b_priv);
        }
        // L1 掩码层放行（B 的 s_ipc_to 有 R 位——may_asynsend_to 通过；
        // senda 还要求 SYS_PROC），拒绝发生在 L2 filter 层：白名单只允许
        // 来源 0x31（≠ B 的 0x32）。
        priv_table.get_mut(b_priv).unwrap().flags.s_flags.insert(
            crate::capability::ProcessCapability::SYS_PROC,
        );
        priv_table.get_mut(b_priv).unwrap().ipc.s_ipc_to =
            crate::capability::IpcMask::from_bits(1u64 << r_priv);
        let wl = pool.allocate(crate::ipc_filter::IpcFilterType::Whitelist).unwrap();
        {
            let slot = pool.get_mut(wl).unwrap();
            slot.num_elements = 1;
            slot.elements[0] = crate::ipc_filter::IpcFilterElement {
                flags: crate::ipc_filter::IpcFilterElFlags::MATCH_M_SOURCE,
                m_source: 0x31,
                m_type: 0,
            };
        }
        priv_table.get_mut(r_priv).unwrap().mem.s_ipcf = Some(wl);

        let copy = RecordingCopy { delivered: Cell::new(false) };
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &copy)
            .with_filter_pool(&pool);

        let outcome = engine.senda(ProcNr(6), VirBytes::new(0x1000), 1);
        assert!(outcome.is_delivered());
        assert_eq!(
            engine.priv_table.get(r_priv).unwrap().signals.s_asyn_pending,
            1u64 << b_priv
        );
        assert!(!copy.delivered.get(), "filtered entry must not be marked AMF_DONE");
    }

    /// D-17 fixture: the single table entry is addressed to SELF
    /// (resolves to the caller's own endpoint — proc.c:1183-1185).
    struct SelfEntryCopy;
    impl UserCopy for SelfEntryCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
        fn read_senda_entry(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> {
            Ok((Endpoint::SELF, Message::default(), AMF_VALID))
        }
        fn write_senda_result(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize, _result: i32, _flags: i32) -> Result<(), CopyError> { Ok(()) }
    }

    /// D-17 fixture: entry aimed at a fixed endpoint; captures the
    /// per-entry result written back by `write_senda_result`.
    struct CapturingCopy {
        target: Endpoint,
        result: core::cell::Cell<Option<i32>>,
    }
    impl UserCopy for CapturingCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
        fn read_senda_entry(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> {
            Ok((self.target, Message::default(), AMF_VALID))
        }
        fn write_senda_result(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize, result: i32, _flags: i32) -> Result<(), CopyError> {
            self.result.set(Some(result));
            Ok(())
        }
    }

    /// A UserCopy impl that always page-faults.
    struct PageFaultCopy;
    impl UserCopy for PageFaultCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Err(CopyError::PageFault) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Err(CopyError::PageFault) }
        fn read_senda_entry(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> { Err(CopyError::PageFault) }
        fn write_senda_result(&self, _root: minix_types::PhysBytes, _table: VirBytes, _index: usize, _result: i32, _flags: i32) -> Result<(), CopyError> { Err(CopyError::PageFault) }
    }

    #[test]
    fn test_deliver_message_success() {
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        // Poison the return register first: the delivery must overwrite it
        // with OK (C: proc.c:291 — the completed IPC returns OK to the
        // caller). Without the write the resumed caller sees the stale
        // trap-entry value (real machine NK4-A C-3: VM saw its src arg).
        crate::proc::set_ipc_return_code(&mut a, 0x7c00);
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &SuccessCopy);
        let result = engine.deliver_message(test_nr(0));
        assert_eq!(result, DeliverResult::Delivered);
        // MF_DELIVERMSG cleared on success.
        assert!(
            !engine.procs[0]
                .p_misc_flags
                .is_set(MiscFlagsBits::DELIVERMSG)
        );
        // The completed IPC returns OK (C: proc.c:290-292).
        assert_eq!(
            minix_arch::ipc_return_code(&engine.procs[0].cpu_context),
            OK as i64 as u64
        );
    }

    #[test]
    fn test_deliver_message_context_set_preserves_return_code() {
        // C: proc.c:290 — `if(!(rp->p_misc_flags & MF_CONTEXT_SET))`:
        // when the kernel deliberately rewrote the context (signal
        // delivery), delivermsg must not clobber the return register.
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        a.p_misc_flags.set(MiscFlagsBits::CONTEXT_SET);
        crate::proc::set_ipc_return_code(&mut a, 0x1234);
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &SuccessCopy);
        let result = engine.deliver_message(test_nr(0));
        assert_eq!(result, DeliverResult::Delivered);
        assert_eq!(
            minix_arch::ipc_return_code(&engine.procs[0].cpu_context),
            0x1234
        );
    }

    #[test]
    fn test_deliver_message_first_page_fault() {
        let a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &PageFaultCopy);
        let result = engine.deliver_message(test_nr(0));
        assert_eq!(result, DeliverResult::PageFault);
        // MF_MSGFAILED set on first failure.
        assert!(engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::MSGFAILED));
        // MF_DELIVERMSG still set (will retry).
        assert!(engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    #[test]
    fn test_deliver_message_second_consecutive_fault() {
        let a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        a.p_misc_flags.set(MiscFlagsBits::MSGFAILED); // already failed once
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &PageFaultCopy);
        let result = engine.deliver_message(test_nr(0));
        assert_eq!(result, DeliverResult::Segfault, "second consecutive fault → Segfault");
        // Both flags cleared.
        assert!(!engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::MSGFAILED));
    }

    // ── Permission check tests (P0-12-1) ──

    #[test]
    fn test_check_ipc_permission_target_kernel_task_restriction() {
        // Layer 4 (C: proc.c:560-566): calls TO a kernel task target
        // (p_nr < 0) may only be SENDREC or RECEIVE — kernel tasks always
        // reply and may not block if the caller doesn't receive. C checks
        // the TARGET (`iskerneln(src_dst_p)`), not the caller.
        // To reach layer 4, layers 1-3 must pass: endpoint valid,
        // whitelist allows target, trap_mask allows SEND call.
        let task_nr = test_nr(0);              // p_nr = -NR_TASKS (kernel task)
        let user_nr = test_nr(NR_TASKS);       // p_nr = 0 (regular process)
        let mut procs: Vec<KProcess> = (0..=NR_TASKS)
            .map(|i| make_test_proc(i, Endpoint(1000 + i as i32)))
            .collect();
        procs[0] = make_test_proc(0, Endpoint(2));  // kernel task target
        procs[NR_TASKS].p_endpoint = Endpoint(1);   // user's own endpoint
        let mut priv_table = crate::test_helpers::test_priv_table();
        let task_priv = priv_table.assign_static(task_nr).unwrap();
        let user_priv = priv_table.assign_static(user_nr).unwrap();
        procs[0].priv_id = Some(task_priv);
        procs[NR_TASKS].priv_id = Some(user_priv);
        // Configure caller's priv: allow IPC to task (and self), all traps.
        {
            let caller_priv = priv_table.get_mut(user_priv).unwrap();
            caller_priv.ipc.s_ipc_to = caller_priv.ipc.s_ipc_to.union(
                crate::capability::IpcMask::from_bits(
                    (1u64 << task_priv as u32) | (1u64 << user_priv as u32),
                ),
            );
            caller_priv.ipc.s_trap_mask = crate::capability::TrapMask::ALL; // allow all calls including SEND
        }
        let engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        // SEND to a kernel task target is denied at layer 4.
        let r = engine.check_ipc_permission(user_nr, Endpoint(2), IpcCall::Send);
        assert_eq!(r, Err(IpcError::TrapDenied));
        // SENDREC and RECEIVE to a kernel task target pass layer 4.
        // Layer 3 trap_mask also needs SENDREC bit (bit 3); 0xFFFF covers it.
        let r2 = engine.check_ipc_permission(user_nr, Endpoint(2), IpcCall::SendRec);
        assert_eq!(r2, Ok(()), "SENDREC to kernel task must be allowed");
        let r3 = engine.check_ipc_permission(user_nr, Endpoint(2), IpcCall::Receive);
        assert_eq!(r3, Ok(()), "RECEIVE from kernel task must be allowed");
        // SEND to a non-kernel-task target passes layer 4 (restriction
        // applies to kernel task targets only).
        let r4 = engine.check_ipc_permission(user_nr, Endpoint(1), IpcCall::Send);
        assert_eq!(r4, Ok(()), "SEND to regular target must pass layer 4");
    }

    // ── SENDA tests (no kernel-side table cache — A_RETR/A_INSRT per entry) ──

    #[test]
    fn test_senda_requires_sys_proc() {
        // C: mini_senda — proc.c:1336-1339. Caller without SYS_PROC
        // privilege gets EPERM.
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(test_nr(0)).unwrap();
        procs[0].priv_id = Some(a_priv);
        // Leave SYS_PROC unset → EPERM (IpcError::Permission).
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.senda(test_nr(0), VirBytes::new(0x1000), 1);
        assert_eq!(outcome.err(), Some(IpcError::Permission));
    }

    #[test]
    fn test_senda_delivers_to_receiving_target() {
        // SENDA with target in RECEIVE → entry delivered via
        // read_senda_entry/write_senda_result (no kernel table copy).
        //
        // Realistic state (C: mini_senda, proc.c:1231-1323): user-region
        // slots (p_nr > 0 — task slots get ECALLDENIED, iskerneln
        // proc.c:1266), caller privileged (SYS_PROC, proc.c:1336), and
        // the caller's s_ipc_to carrying the target's bit (may_send_to,
        // priv.h:87 — a fresh priv has an empty mask).
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(b_priv);
        }
        // Caller: SYS_PROC + IPC send permission for the target's sys_id.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
            p.ipc.s_ipc_to = p.ipc.s_ipc_to.union(
                crate::capability::IpcMask::from_bits(1u64 << b_priv),
            );
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SuccessCopy);
        // SuccessCopy reads one AMF_VALID entry aimed at Endpoint(2).
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        assert!(outcome.is_delivered(), "SENDA never blocks caller");
        // Fully delivered → caller's table pointer stays cleared.
        {
            let p = engine.priv_table.get(a_priv).unwrap();
            assert_eq!(p.signals.s_asynsize, 0);
        }
        // Target got the message and was woken.
        let b_idx = nr_to_idx(ProcNr(2)).unwrap();
        assert!(engine.procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!engine.procs[b_idx].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert_eq!(engine.procs[b_idx].p_delivermsg.m_source, Endpoint(1));
    }

    /// D-17 (priv.h:87): `may_asynsend_to = may_send_to || self` — a
    /// SENDA entry addressed to SELF (resolving to the caller's own
    /// endpoint) passes the IPC mask gate even though the caller's own
    /// `s_ipc_to` bit is clear (the boot convention deliberately keeps
    /// the self bit clear). The target — the caller itself — is not
    /// RECEIVING while executing senda, so the entry takes the pending
    /// path: `s_asyn_pending` bit set for the caller's sys_id and the
    /// table pointer kept for the next receive (C: proc.c:1293-1298,
    /// 1320-1323).
    #[test]
    fn test_senda_self_target_allowed_without_mask_bit() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
        }
        // Caller: SYS_PROC but an EMPTY s_ipc_to — the self exception
        // must not depend on any mask bit.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SelfEntryCopy);
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        // Permission gate passed (CallDenied would surface as a table
        // result, not the aggregate outcome) → aggregate OK.
        assert!(outcome.is_delivered());
        {
            let p = engine.priv_table.get(a_priv).unwrap();
            // Pending path: s_asyn_pending bit for the caller's own
            // sys_id + table pointer kept for retry.
            assert_eq!(p.signals.s_asyn_pending, 1u64 << a_priv);
            assert_eq!(p.signals.s_asynsize, 1);
        }
    }

    /// D-17 asymmetry counterpart: the same EMPTY `s_ipc_to` DENIES an
    /// entry aimed at a different process (the sync-path rule
    /// `may_send_to` has no self exception — priv.h:86 vs :87). The
    /// denial is per-entry: the aggregate outcome stays OK and the
    /// result is written back to the table entry (C: proc.c:1300-1307).
    #[test]
    fn test_senda_other_without_mask_bit_denied() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(b_priv);
        }
        // Caller: SYS_PROC, EMPTY s_ipc_to — no bit for the target.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
        }
        let copy = CapturingCopy { target: Endpoint(2), result: core::cell::Cell::new(None) };
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &copy);
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        // Aggregate outcome is OK (SENA never fails as a whole); the
        // denial lives in the per-entry result.
        assert!(outcome.is_delivered());
        assert_eq!(copy.result.get(), Some(ECALLDENIED));
    }

    #[test]
    fn test_senda_pending_sets_bitmap_and_retries_via_receive() {
        // Target not receiving → s_asyn_pending bit set on target,
        // table pointer kept in caller's priv; the next receive from
        // the target re-reads the table and delivers (C: try_one).
        //
        // Realistic state: user-region slots (task slots get
        // ECALLDENIED — iskerneln, proc.c:1266), both privs assigned,
        // caller SYS_PROC with the target's bit in s_ipc_to.
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::new();
            b.priv_id = Some(b_priv);
        }
        // Caller: SYS_PROC + IPC send permission for the target's sys_id.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
            p.ipc.s_ipc_to = p.ipc.s_ipc_to.union(
                crate::capability::IpcMask::from_bits(1u64 << b_priv),
            );
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SuccessCopy);
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        assert!(outcome.is_delivered());
        // Target's async-pending bitmap has the sender's bit.
        {
            let p = engine.priv_table.get(b_priv).unwrap();
            assert_ne!(p.signals.s_asyn_pending & (1u64 << a_priv), 0,
                "s_asyn_pending must carry the sender's priv bit");
        }
        // Caller's table pointer retained for retry (C: proc.c:1320-1323).
        {
            let p = engine.priv_table.get(a_priv).unwrap();
            assert_eq!(p.signals.s_asyntab, 0x1000);
            assert_eq!(p.signals.s_asynsize, 1);
        }
        // Target now receives → the still-queued sender is drained and
        // delivered (C: try_one / caller_q retry). Hosted stubs cannot
        // re-read the user SENDA table (`read_senda_entry` is a
        // PageFault stub under cfg(test) — the pure Phase-2 leg is
        // covered on real hardware, serial_b8a/b9p2), so the retry
        // observable here is the pending-bit consumption + delivery via
        // the receive leg with the F14/B9 sync copy.
        let b_idx = nr_to_idx(ProcNr(2)).unwrap();
        let a_idx = nr_to_idx(ProcNr(1)).unwrap();
        {
            let procs = &mut engine.procs[..];
            procs[b_idx].p_rts_flags.set(RtsFlagsBits::RECEIVING);
            procs[b_idx].p_getfrom_e = Endpoint::ANY;
            caller_q_push(procs, b_idx, a_idx);
        }
        let r = engine.receive(ProcNr(2), Endpoint::ANY);
        assert!(r.is_delivered(), "receive must deliver the retried sender");
        // Hosted stub cannot re-read the sender's table (asynendpoint
        // unset → deliver_async guard fails), so the Phase-2 attempt
        // leaves nothing delivered — C try_one's tail re-arm (proc.c:
        // 1499-1501, NK4-C B8) puts the sender's bit back. The bit is
        // still there for the *async* retry to be picked up, while the
        // sender itself was drained from caller_q by the sync leg.
        {
            let p = engine.priv_table.get(b_priv).unwrap();
            assert_ne!(
                p.signals.s_asyn_pending & (1u64 << a_priv),
                0,
                "undelivered async attempt must re-arm the pending bit (C else arm)"
            );
            let sp = engine.priv_table.get(a_priv).unwrap();
            assert_ne!(sp.signals.s_asynsize, 0, "sender table kept for retry");
        }
        // NK4-C B9: sync copy delivered the message (SuccessCopy → no
        // deposit left behind).
        assert!(
            !engine.procs[b_idx]
                .p_misc_flags
                .is_set(MiscFlagsBits::DELIVERMSG),
            "successful sync copy must not leave the message deposited"
        );
    }

    // ── Deliver result + KernelUserCopy stub ──

    #[test]
    fn test_deliver_result_variants() {
        let _ = DeliverResult::Delivered;
        let _ = DeliverResult::PageFault;
        let _ = DeliverResult::Segfault;
    }

    #[test]
    fn test_kernel_user_copy_stub() {
        let copier = KernelUserCopy;
        let _msg = copier.copy_msg_from_user(VirBytes::new(0)).unwrap();
        copier.copy_msg_to_user(VirBytes::new(0), &Message::default()).unwrap();
        // SENDA stub: reads fault (no real user table in tests), writes
        // succeed (C ignores A_INSRT errors). 1.12：root 参数（PhysBytes）。
        let root = minix_types::PhysBytes(0);
        assert!(copier.read_senda_entry(root, VirBytes::new(0), 0).is_err());
        copier.write_senda_result(root, VirBytes::new(0), 0, 0, 0).unwrap();
    }

    // ── user_copy_range_mapped: validate-first user-buffer checks ──
    //
    // The production wiring passes `CurrentPteWalk::walk` (real page
    // tables); these tests drive the injected-walk seam so every branch is
    // exercised without an MMU (C parity anchors per branch below).

    /// Mock walk over a page->flags map (page base address is the key).
    fn walk_table(
        pages: &[(u64, minix_arch::paging::PageFlags)],
    ) -> impl Fn(
        minix_types::PhysBytes,
        VirBytes,
    ) -> Option<(minix_types::PhysBytes, minix_arch::paging::PageFlags)>
    + '_ {
        move |_, va| {
            pages
                .iter()
                .find(|(base, _)| *base == va.0 & !0xFFF)
                .map(|(base, flags)| (minix_types::PhysBytes(*base), *flags))
        }
    }

    #[test]
    fn user_copy_range_rejects_kernel_half_as_out_of_bounds() {
        // C's segment-limited copy could never reach kernel linear
        // addresses; the flat model expresses that as the user-half bound.
        let any_walk = |_: minix_types::PhysBytes, _: VirBytes| None;
        let r = user_copy_range_mapped(
            None,
            VirBytes::new(0xFFFF_8000_0000_0000),
            8,
            false,
            any_walk,
        );
        assert_eq!(r, Err(CopyError::OutOfBounds));
    }

    #[test]
    fn user_copy_range_rejects_range_end_overflow_as_out_of_bounds() {
        // va + len must not wrap past the limit (0x7FFF...FF + 8).
        let any_walk = |_: minix_types::PhysBytes, _: VirBytes| None;
        let r = user_copy_range_mapped(
            None,
            VirBytes::new(0x0000_7FFF_FFFF_FFF8),
            64,
            false,
            any_walk,
        );
        assert_eq!(r, Err(CopyError::OutOfBounds));
    }

    #[test]
    fn user_copy_range_without_active_root_passes_bound_only() {
        // No active root = no paging (boot/hosted posture): the bound is
        // the only checkable contract, translation is vacuously absent.
        let r = user_copy_range_mapped(
            None,
            VirBytes::new(0x1000),
            64,
            false,
            |_: minix_types::PhysBytes, _: VirBytes| {
                panic!("walk must not be called without an active root")
            },
        );
        assert_eq!(r, Ok(()));
    }

    #[test]
    fn user_copy_range_unmapped_page_is_page_fault() {
        let r = user_copy_range_mapped(
            Some(minix_types::PhysBytes::new(0x9000)),
            VirBytes::new(0x5000),
            64,
            false,
            walk_table(&[]),
        );
        assert_eq!(r, Err(CopyError::PageFault));
    }

    #[test]
    fn user_copy_range_second_page_unmapped_is_page_fault() {
        // A 64-byte buffer straddling the page boundary needs BOTH pages.
        let page = minix_arch::paging::PageFlags::PRESENT
            | minix_arch::paging::PageFlags::USER_ACCESSIBLE
            | minix_arch::paging::PageFlags::WRITABLE;
        let r = user_copy_range_mapped(
            Some(minix_types::PhysBytes::new(0x9000)),
            VirBytes::new(0x1FFC),
            64,
            false,
            walk_table(&[(0x1000, page)]), // 0x2000 page missing
        );
        assert_eq!(r, Err(CopyError::PageFault));
    }

    #[test]
    fn user_copy_range_supervisor_page_is_page_fault() {
        // Identity-mapped low pages are supervisor-only (U=0): a user
        // process must not hand them to the kernel as message buffers.
        let kernel_page =
            minix_arch::paging::PageFlags::PRESENT | minix_arch::paging::PageFlags::WRITABLE;
        let r = user_copy_range_mapped(
            Some(minix_types::PhysBytes::new(0x9000)),
            VirBytes::new(0x1000),
            64,
            false,
            walk_table(&[(0x1000, kernel_page)]),
        );
        assert_eq!(r, Err(CopyError::PageFault));
    }

    #[test]
    fn user_copy_range_write_to_readonly_page_is_page_fault() {
        let ro = minix_arch::paging::PageFlags::read_only();
        let r = user_copy_range_mapped(
            Some(minix_types::PhysBytes::new(0x9000)),
            VirBytes::new(0x2000),
            64,
            true,
            walk_table(&[(0x2000, ro)]),
        );
        assert_eq!(r, Err(CopyError::PageFault));
    }

    #[test]
    fn user_copy_range_readonly_read_and_rw_write_pass() {
        let ro = minix_arch::paging::PageFlags::read_only();
        let rw = minix_arch::paging::PageFlags::read_write();
        let table = [(0x2000, ro), (0x3000, rw)];
        let read_ro = user_copy_range_mapped(
            Some(minix_types::PhysBytes::new(0x9000)),
            VirBytes::new(0x2000),
            64,
            false,
            walk_table(&table),
        );
        assert_eq!(read_ro, Ok(()));
        let write_rw = user_copy_range_mapped(
            Some(minix_types::PhysBytes::new(0x9000)),
            VirBytes::new(0x3000),
            64,
            true,
            walk_table(&table),
        );
        assert_eq!(write_rw, Ok(()));
    }

    // ── C-27 multi-process boot exchange (test-sysboot real-machine regression) ──

    #[test]
    fn test_sendrec_to_blocked_receiver_stamps_source_endpoint() {
        // C-27 real machine (test-sysboot): the woken receiver observed
        // m_source = Message::default() poison and failed its sendnb reply
        // with EDEADSRCDST(202). Engine half of the contract: a sendrec
        // into a parked receive(ANY) delivers with the SENDER's boot
        // endpoint stamped over the copied message (C mini_send,
        // proc.c:904 — `dst_ptr->p_delivermsg.m_source = caller endpt`),
        // replayed here on the const-init table where gen-0 endpoints ==
        // proc nrs, the exact boot state the carrier runs.
        let mut table = crate::proc_table::ProcessTable::new();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let vm_idx = 8usize + NR_TASKS; // nr 8
        let rs_idx = 2usize + NR_TASKS; // nr 2
        {
            let procs = table.procs_slice_mut();
            procs[vm_idx].p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
            procs[rs_idx].p_rts_flags.clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        }
        let procs = table.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);

        // rx parks in receive(ANY) — the door-blocked state.
        let blocked = engine.receive(ProcNr(8), Endpoint::ANY);
        assert!(matches!(blocked, IpcOutcome::Blocked), "rx must block");

        // tx sendrec(8) — the send half should deliver into rx's p_delivermsg.
        let mut msg = Message::default();
        msg.m_type = 0x42;
        let outcome = engine.sendrec(ProcNr(2), Endpoint(8), &msg);
        assert!(
            matches!(outcome, IpcOutcome::Blocked),
            "tx parks in the reply receive half"
        );
        let delivered_source = engine.procs[vm_idx].p_delivermsg.m_source;
        assert_eq!(delivered_source, Endpoint(2));
        // Tear down: the const-init table carries occupied kernel-task slots
        // (IDLE is PROC_STOP, not SLOT_FREE) whose Drop guard forbids plain
        // scope exit — release every slot back to SLOT_FREE first. IpcEngine
        // implements no Drop, so the table reborrow below ends its borrow.
        for p in table.procs_slice_mut().iter_mut() {
            p.p_rts_flags.set(crate::proc::RtsFlagsBits::SLOT_FREE);
        }
    }

    /// NK4-C 1.10x：SENDREC 发送半被 drain 后停车的 receive 半必须以
    /// sendrec 目的地为 getfrom（C proc.c:1104-1107，src_e 阻塞），不得
    /// ANY——否则第三方发往该发送者的消息经 Path A 直投冒充 reply，
    /// 完成一个语义上未完成的 sendrec（s16g 真机互卡根因）。
    #[test]
    fn test_sendrec_parked_receive_half_getfrom_locks_to_destination() {
        // pm = idx 0 (ep 10), sched = idx 1 (ep 11), init = idx 2 (ep 12).
        let mut procs = crate::test_helpers::scratch_procs([
            make_test_proc(0, Endpoint(10)),
            make_test_proc(1, Endpoint(11)),
            make_test_proc(2, Endpoint(12)),
        ]);
        for p in procs.iter_mut() {
            p.p_rts_flags = RtsFlags::new();
        }
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);

        // 1. pm sendrec(sched)：sched 未收 → 发送半阻塞，REPLY_PEND。
        let mut req = Message::default();
        req.m_type = 0x21;
        assert!(matches!(
            engine.sendrec(test_nr(0), Endpoint(11), &req),
            IpcOutcome::Blocked
        ));
        assert!(
            engine.procs[0]
                .p_misc_flags
                .is_set(MiscFlagsBits::REPLY_PEND)
        );

        // 2. sched receive(ANY)：Phase 3 drain 取走 pm 的发送半 → pm 转入
        //    receive 半停车，getfrom 锁定为目的地 sched 的端点。
        assert!(matches!(
            engine.receive(test_nr(1), Endpoint::ANY),
            IpcOutcome::Delivered
        ));
        assert!(!engine.procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
        assert!(engine.procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert_eq!(engine.procs[0].p_getfrom_e, Endpoint(11));

        // 3. init 发往 pm：getfrom 锁定 → 不得直投冒充 reply，init 阻塞。
        //    （测试态 user copy 恒返回零消息，判定靠 m_source 盖章。）
        let mut stray = Message::default();
        stray.m_type = 0x33;
        assert!(matches!(
            engine.send(test_nr(2), Endpoint(10), &stray, SendFlags::NONE),
            IpcOutcome::Blocked
        ));
        assert!(engine.procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert_ne!(engine.procs[0].p_delivermsg.m_source, Endpoint(12));

        // 4. sched 回复：getfrom 命中 → Path A 直投完成 pm 的 sendrec。
        let mut reply = Message::default();
        reply.m_type = 0x7c;
        assert!(matches!(
            engine.send(test_nr(1), Endpoint(10), &reply, SendFlags::NONE),
            IpcOutcome::Delivered
        ));
        assert!(!engine.procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert_eq!(engine.procs[0].p_delivermsg.m_source, Endpoint(11));
    }

    /// NK4-C 1.10x b：SENDREC 秒达（发送半 Path A 直投）时，receive 半必须
    /// 只收 sendrec 目的地（C proc.c:571-583 同一 src_dst_e）——不得
    /// receive(ANY) 把 caller_q 上第三方排队请求当 reply 消费（s17j 实锤
    /// 的 PM↔sched ENOSYS ping-pong 根因）。
    #[test]
    fn test_sendrec_fast_path_receive_half_scopes_to_destination() {
        // pm = idx 0 (ep 10), sched = idx 1 (ep 11), init = idx 2 (ep 12).
        let mut procs = crate::test_helpers::scratch_procs([
            make_test_proc(0, Endpoint(10)),
            make_test_proc(1, Endpoint(11)),
            make_test_proc(2, Endpoint(12)),
        ]);
        for p in procs.iter_mut() {
            p.p_rts_flags = RtsFlags::new();
        }
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);

        // 1. sched receive(ANY) 停车（待命收 taskcall）。
        assert!(matches!(
            engine.receive(test_nr(1), Endpoint::ANY),
            IpcOutcome::Blocked
        ));
        // 2. init send(pm)：pm 未收 → init 阻塞入 pm 的 caller_q。
        let mut stray = Message::default();
        stray.m_type = 0x33;
        assert!(matches!(
            engine.send(test_nr(2), Endpoint(10), &stray, SendFlags::NONE),
            IpcOutcome::Blocked
        ));
        // 3. pm sendrec(sched)：发送半秒达（Path A）→ receive 半以目的地
        //    sched 过滤：init 的排队请求（src 12 ≠ 11）不得消费 → 阻塞。
        let mut req = Message::default();
        req.m_type = 0x21;
        assert!(matches!(
            engine.sendrec(test_nr(0), Endpoint(11), &req),
            IpcOutcome::Blocked
        ));
        assert!(engine.procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert_eq!(engine.procs[0].p_getfrom_e, Endpoint(11));
        assert_ne!(engine.procs[0].p_delivermsg.m_source, Endpoint(12));
        // 4. sched 回复 → Path A 命中 getfrom 完成 sendrec。
        let mut reply = Message::default();
        reply.m_type = 0x7c;
        assert!(matches!(
            engine.send(test_nr(1), Endpoint(10), &reply, SendFlags::NONE),
            IpcOutcome::Delivered
        ));
        assert!(!engine.procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert_eq!(engine.procs[0].p_delivermsg.m_source, Endpoint(11));
    }
}
