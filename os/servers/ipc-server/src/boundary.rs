//! Production [`IpcBoundary`] and [`EventLoopTransport`] — the trap backend.
//!
//! C correspondence: the libsys call surface of
//! `minix3/minix/servers/ipc/main.c`
//! (`clock_time`/`getnuid`/`proceventmask`/`ipc_sendnb`), the `sys_datacopy`
//! transfers and VM messages of `sem.c`/`shm.c` (`vm_remap`/`vm_getphys`/
//! `vm_getrefcount`/`vm_unmap`), and the anonymous backing of
//! `mmap.c:113-118`.
//!
//! # Error convention
//!
//! The trait's `i32` error is a **positive errno** (the reply's `m_type` is
//! sent back negated, the same domain as the Errno constants); the minix-sys
//! `sys_*` wrappers return negative errnos, and the boundary folds them
//! positive with `-e`. The VM/PM `*_via` calls return `Errno`; take
//! `to_i32()`.
//!
//! # MIB face state
//!
//! The RMIB protocol walker behind [`IpcBoundary::mib_process`] (the server
//! half of libsys `rmib.c`) is not wired yet: the decision layer is ready
//! (`KernIpcChild`/`InfoRoute` in `mib_tree.rs` plus the two sem/shm info
//! assemblers) while the protocol half is pending — for now it answers
//! `EOPNOTSUPP`, failing closed like C's unknown-request exit.

use alloc::vec::Vec;
use minix_sys::ipc::{AsyncSlot, AsyncSlotFlags, DirectTrapTransport, IpcTransport as _};
use minix_sys::syscall::{self, DirectKernelCallTransport};
use minix_sys::vm as sys_vm;
use minix_types::{
    Endpoint, Message, PM_GETEPINFO, PROC_EVENT_EXIT, PROC_EVENT_SIGNAL,
    ipc::{IpcPerm, MessLcVmShmUnmap, MessLsysPmGetepinfo, SemidDs, ShmidDs},
};

use crate::perms::{Identity, SetOptions};
use crate::sem::ctl::{self, SemInfo};
use crate::sem::op::SemOp;
use crate::server::{EventLoopTransport, IpcStatus, TransportError};
use crate::service::{Credentials, IpcBoundary};
use crate::shm::ShmIdView;
use crate::shm::attach::{ShmInfoAgg, ShmSummary};
use crate::shm::segment::Backing;

/// Byte length of one `semop` array element (C `struct sembuf`: three shorts).
const SEMBUF_SIZE: usize = 6;

fn positive(e: i32) -> i32 {
    if e < 0 { -e } else { e }
}

/// Production boundary: the IPC leg (PEER taskcall plus asynchronous sends) and
/// the kernel-call leg (safecopy, time, and credentials). The two legs follow
/// the same shape as rs's `TrapKernelApi` and input's serve.
pub struct SysBoundary {
    ipc: DirectTrapTransport,
    kernel: DirectKernelCallTransport,
}

impl SysBoundary {
    pub const fn new() -> Self {
        Self {
            ipc: DirectTrapTransport,
            kernel: DirectKernelCallTransport,
        }
    }
}

impl Default for SysBoundary {
    fn default() -> Self {
        Self::new()
    }
}

impl SysBoundary {
    /// `sys_datacopy` from the caller into a local buffer (the sem.c:680-682
    /// shape).
    fn copy_in_raw(&self, from: Endpoint, ptr: u32, buf: &mut [u8]) -> Result<(), i32> {
        syscall::sys_datacopy(
            &self.kernel,
            from.0,
            ptr as u64,
            syscall::SELF,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
        .map_err(positive)
    }

    /// `sys_datacopy` from a local buffer into the caller's address space.
    fn copy_out_raw(&self, to: Endpoint, ptr: u32, buf: &[u8]) -> Result<(), i32> {
        syscall::sys_datacopy(
            &self.kernel,
            syscall::SELF,
            buf.as_ptr() as u64,
            to.0,
            ptr as u64,
            buf.len() as u64,
        )
        .map_err(positive)
    }
}

impl IpcBoundary for SysBoundary {
    fn now(&self) -> u64 {
        // C clock_time(NULL) (clock_time.c): wall seconds = boottime +
        // realtime_ticks / hz. On error, 0 (an uninitialized-clock timestamp,
        // the same shape as C's "bad, but what's better" branch — a timestamp
        // must not kill the caller).
        let ticks = syscall::sys_times(&self.kernel, Endpoint::SELF.0);
        let hz = syscall::sys_get_hz(&self.kernel);
        match (ticks, hz) {
            (Ok(t), Ok(h)) => {
                let snapshot = minix_sys::misc::ClockSnapshot {
                    uptime_ticks: t.boot_ticks,
                    realtime_ticks: t.real_ticks,
                    boottime_seconds: t.boot_time,
                    ticks_per_second: h as u64,
                };
                minix_sys::misc::wall_clock_time(snapshot).0
            }
            _ => 0,
        }
    }

    fn credentials_of(&self, endpoint: Endpoint) -> Option<Credentials> {
        // The getnuid/getngid plus getnpid of C main.c:265-268: one
        // PM_GETEPINFO brings them all back (the taskcall return value is the
        // pid). The real uid/gid are in the reply arm's uid/gid fields
        // (minix-sys's GetepInfo exposes only euid/egid, while permission
        // checks need the real identity, so the whole arm is read here).
        let mut msg = Message::default();
        msg.m_u.m_lsys_pm_getepinfo = MessLsysPmGetepinfo {
            endpt: endpoint.0,
            ..MessLsysPmGetepinfo::default()
        };
        let reply =
            syscall::perform_taskcall(&self.ipc, Endpoint::PM, PM_GETEPINFO, &mut msg).ok()?;
        if reply < 0 {
            return None;
        }
        // SAFETY: PM fills the reply arm after a successful taskcall
        // (ipc.h mess_pm_lsys_getepinfo).
        let arm = unsafe { &msg.m_u.m_pm_lsys_getepinfo };
        Some(Credentials {
            identity: Identity {
                uid: arm.uid as u32,
                gid: arm.gid as u32,
            },
            pid: reply,
        })
    }

    fn copy_in_ops(&self, from: Endpoint, ptr: u32, count: usize) -> Result<Vec<SemOp>, i32> {
        let mut raw = alloc::vec![0u8; count * SEMBUF_SIZE];
        self.copy_in_raw(from, ptr, &mut raw)?;
        Ok(raw
            .chunks_exact(SEMBUF_SIZE)
            .map(|b| SemOp {
                num: u16::from_le_bytes([b[0], b[1]]),
                op: i16::from_le_bytes([b[2], b[3]]),
                flag: u16::from_le_bytes([b[4], b[5]]),
            })
            .collect())
    }

    fn copy_in_setall(&self, from: Endpoint, ptr: u32, count: usize) -> Result<Vec<u16>, i32> {
        let mut raw = alloc::vec![0u8; count * 2];
        self.copy_in_raw(from, ptr, &mut raw)?;
        Ok(raw
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect())
    }

    fn copy_out_getall(
        &self,
        to: Endpoint,
        ptr: u32,
        values: &[u16],
        count: usize,
    ) -> Result<(), i32> {
        let n = values.len().min(count);
        let mut raw = alloc::vec![0u8; count * 2];
        for (i, v) in values[..n].iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn copy_out_sem_stat(&self, to: Endpoint, ptr: u32, view: &ctl::SemIdView) -> Result<(), i32> {
        let ds = SemidDs {
            sem_perm: IpcPerm {
                uid: view.perm.uid,
                gid: view.perm.gid,
                cuid: view.perm.creator_uid,
                cgid: view.perm.creator_gid,
                mode: view.perm.mode,
                ..IpcPerm::default()
            },
            sem_nsems: view.count,
            sem_otime: view.op_time,
            sem_ctime: view.change_time,
            ..SemidDs::default()
        };
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const ds) as *const u8,
                core::mem::size_of::<SemidDs>(),
            )
        };
        self.copy_out_raw(to, ptr, bytes)
    }

    fn copy_out_seminfo(&self, to: Endpoint, ptr: u32, info: &SemInfo) -> Result<(), i32> {
        // C `struct seminfo` (sys/sem.h:69-79): ten ints in C field order.
        let fields = [
            info.map,
            info.identifiers,
            info.total,
            info.undo_structures,
            info.per_id,
            info.max_ops,
            info.undo_entries,
            info.live_or_undo,
            info.max_value,
            info.allocated_or_exit,
        ];
        let mut raw = alloc::vec![0u8; fields.len() * 4];
        for (i, v) in fields.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn copy_in_set_options(&self, from: Endpoint, ptr: u32) -> Result<SetOptions, i32> {
        // C sem.c:551-553: IPC_SET copies in the permission draft half
        // (uid/gid/mode, i.e. the first three fields of the 24-byte
        // `ipc_perm`).
        let mut raw = [0u8; core::mem::size_of::<IpcPerm>()];
        self.copy_in_raw(from, ptr, &mut raw)?;
        Ok(SetOptions {
            uid: u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            gid: u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]),
            mode: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
        })
    }

    fn copy_out_shm_stat(&self, to: Endpoint, ptr: u32, view: &ShmIdView) -> Result<(), i32> {
        let ds = ShmidDs {
            shm_perm: IpcPerm {
                uid: view.perm.uid,
                gid: view.perm.gid,
                cuid: view.perm.creator_uid,
                cgid: view.perm.creator_gid,
                mode: view.perm.mode,
                ..IpcPerm::default()
            },
            shm_segsz: view.size,
            shm_lpid: view.last_pid,
            shm_cpid: view.creator_pid,
            shm_nattch: view.attached as u32,
            shm_atime: view.attach_time,
            shm_dtime: view.detach_time,
            shm_ctime: view.change_time,
            ..ShmidDs::default()
        };
        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&raw const ds) as *const u8,
                core::mem::size_of::<ShmidDs>(),
            )
        };
        self.copy_out_raw(to, ptr, bytes)
    }

    fn copy_out_shminfo(&self, to: Endpoint, ptr: u32, summary: &ShmSummary) -> Result<(), i32> {
        // C shm.c:349-352: the all-constant shminfo block (the summarize
        // decision face). sys/shm.h shminfo field order:
        // shmmax/shmmin/shmmni/shmseg/shmall.
        let fields: [u32; 5] = [
            summary.max as u32,
            summary.min,
            summary.identifiers,
            summary.identifiers,
            (summary.pages as u32) * (summary.max as u32 / 4096).max(1),
        ];
        let mut raw = alloc::vec![0u8; fields.len() * 4];
        for (i, v) in fields.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn copy_out_shmagg(&self, to: Endpoint, ptr: u32, agg: &ShmInfoAgg) -> Result<(), i32> {
        // C shm.c:354-364: the shm_info aggregate block (sys/shm.h:208-216
        // field order).
        let fields = [
            agg.used_ids,
            (agg.total_pages & 0x7fff_ffff) as i32,
            agg.resident_pages as i32,
            agg.swapped_pages as i32,
            agg.swap_attempts as i32,
            agg.swap_successes as i32,
        ];
        let mut raw = alloc::vec![0u8; fields.len() * 4];
        for (i, v) in fields.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn set_proceventmask(&self, subscribe: bool) {
        // C main.c:163-168: subscribe = EXIT|SIGNAL, unsubscribe = 0. Best
        // effort (C does not check proceventmask's failure).
        let mask = if subscribe {
            PROC_EVENT_EXIT | PROC_EVENT_SIGNAL
        } else {
            0
        };
        //[SD19-gate] C kernel.c exit sets the event mask; the process is gone,
        // so a failure has no surface
        #[allow(clippy::let_underscore_must_use)]
        let _ = minix_sys::pm::proceventmask_via(&self.ipc, mask);
    }

    fn send_wakeup(&self, endpoint: Endpoint, code: i32) {
        // C send_reply (sem.c:207-215): NO_REPLY is swallowed; otherwise clear
        // the message and `ipc_sendnb` it with m_type = code.
        if code == crate::sem::NO_REPLY {
            return;
        }
        let msg = Message {
            m_type: code,
            ..Message::default()
        };
        //[SD19-gate] sendnb delivers a kill/signal without blocking; a failure
        // drops one message
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.ipc.sendnb(endpoint, &msg);
    }

    fn rmib_copyin(
        &self,
        from: Endpoint,
        grant: i32,
        off: usize,
        dst: &mut [u8],
    ) -> Result<(), i32> {
        // `sys_safecopyfrom` of the C rmib framework (grant owner = the MIB
        // service).
        syscall::sys_safecopyfrom(
            &self.kernel,
            from.0,
            grant,
            off as u64,
            dst.as_mut_ptr() as u64,
            dst.len() as u64,
        )
        .map_err(positive)
    }

    fn rmib_copyout(&self, to: Endpoint, grant: i32, off: usize, src: &[u8]) -> Result<(), i32> {
        syscall::sys_safecopyto(
            &self.kernel,
            to.0,
            grant,
            off as u64,
            src.as_ptr() as u64,
            src.len() as u64,
        )
        .map_err(positive)
    }

    fn send_mib_reply(&mut self, to: Endpoint, msg: &Message) {
        // C rmib.c:1067-1068 — a request that arrived via SENDREC →
        // ipc_sendnb.
        //[SD19-gate] C rmib.c:1067 ipc_sendnb has no reply
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.ipc.sendnb(to, msg);
    }

    fn send_mib_reply_async(&mut self, to: Endpoint, msg: &Message) {
        // C rmib.c:1069-1070 — an asynchronous arrival →
        // asynsend3(AMF_NOREPLY), carried by a single-slot senda (the same
        // shape as EventLoopTransport::send_async).
        let slot = minix_sys::ipc::AsyncSlot {
            flags: minix_sys::ipc::AsyncSlotFlags(
                minix_sys::ipc::AsyncSlotFlags::VALID.0
                    | minix_sys::ipc::AsyncSlotFlags::NO_REPLY.0,
            ),
            destination: to,
            message: *msg,
            result: 0,
        };
        //[SD19-gate] an asynchronous senda reply is fire-and-forget; nothing
        // waits for it
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.ipc.senda(&[slot]);
    }

    fn back_segment(&self, bytes: u64) -> Result<Backing, i32> {
        // C shm.c:113-118: an anonymous mmap(MAP_ANON) segment plus
        // vm_getphys for the physical snapshot.
        use minix_sys::vm::MapRequest;
        let request = MapRequest {
            beneficiary: Endpoint::SELF,
            address: minix_types::VirBytes(0),
            length: minix_types::VirBytes(bytes),
            protection: sys_vm::MAP_PROTECTION_READ | sys_vm::MAP_PROTECTION_WRITE,
            flags: 0,
            file: -1,
            offset: 0,
        };
        // mmap_via(caller=SELF): the beneficiary is the caller itself (the
        // MapRequest already carries `beneficiary`; the caller field is the
        // protocol's "who initiates" half).
        let local = sys_vm::mmap_via(&self.ipc, Endpoint::SELF, request).map_err(|e| e.to_i32())?;
        let phys = sys_vm::physical_address_via(&self.ipc, Endpoint::SELF, local)
            .map_err(|e| e.to_i32())?;
        Ok(Backing {
            local: local.0,
            phys: phys.0,
        })
    }

    fn remap(&self, caller: Endpoint, addr: u64, local: u64, bytes: u64) -> Option<u64> {
        // C shm.c:158-160: vm_remap(caller, addr, local, bytes) maps the
        // segment into the caller's space; MAP_FAILED = None.
        let placed = sys_vm::remap_via(
            &self.ipc,
            sys_vm::VM_CALL_REMAP,
            caller,
            Endpoint::SELF,
            minix_types::VirBytes(addr),
            minix_types::VirBytes(local),
            minix_types::VirBytes(bytes),
        )
        .ok()?;
        Some(placed.0)
    }

    fn phys_of(&self, caller: Endpoint, addr: u64) -> Option<u64> {
        // C shm.c:215-216: vm_getphys; the zero sentinel = EINVAL (as the
        // caller reads it).
        let phys =
            sys_vm::physical_address_via(&self.ipc, caller, minix_types::VirBytes(addr)).ok()?;
        (phys.0 != 0).then_some(phys.0)
    }

    fn unmap(&self, caller: Endpoint, addr: u64) {
        // C shm.c:232: vm_unmap(caller, addr) — VM_SHM_UNMAP
        // (libc mmap.c:131-142: the forwhom + addr fields).
        let mut msg = Message::default();
        msg.m_u.m_lc_vm_shm_unmap = MessLcVmShmUnmap {
            forwhom: caller.0,
            _pad: 0,
            addr,
            _padding: [0; 40],
        };
        //[SD19-gate] the segment-destroy taskcall is best effort; a failure
        // has no recovery surface
        #[allow(clippy::let_underscore_must_use)]
        let _ = syscall::perform_taskcall(
            &self.ipc,
            Endpoint::VM,
            minix_types::VM_SHM_UNMAP as i32,
            &mut msg,
        );
    }

    fn refcount_of(&self, local: u64) -> Option<u8> {
        // The segment is in our own space: vm_getrefcount(SELF, local).
        sys_vm::reference_count_via(&self.ipc, Endpoint::SELF, minix_types::VirBytes(local)).ok()
    }

    fn release_mapping(&self, addr: u64, len: u64) {
        // Local half of segment destruction: drop our own anonymous mapping
        // (C munmap semantics); an error here has nowhere to recover, so it is
        // silent (the same shape as C ignoring munmap's return).
        //[SD19-gate] C ignores munmap's return; an error has nowhere to
        // recover
        #[allow(clippy::let_underscore_must_use)]
        let _ = sys_vm::munmap_via(
            &self.ipc,
            minix_types::VirBytes(addr),
            minix_types::VirBytes(len),
        );
    }

    fn warn(&self, line: &str) {
        // C printf → console; the console face in this tree is the
        // SYS_DIAGCTL bridge (the same as IS's DiagWriter), and a failure is
        // silent (C's printf likewise has nowhere to recover).
        //[SD19-gate] C's console printf fails silently; nowhere to recover
        #[allow(clippy::let_underscore_must_use)]
        let _ = syscall::sys_diagctl_write(&DirectKernelCallTransport, line);
    }

    fn statectl(&self, request: i32, address: u64, length: i32) -> Result<i32, i32> {
        // The real wiring of C's `sys_statectl` (a direct trap) — the birth
        // protocol.
        let r = syscall::sys_statectl(&DirectKernelCallTransport, request, address, length);
        if r == minix_types::OK { Ok(r) } else { Err(r) }
    }
}

// ── production event-loop transport ────────────────────────────────

/// Production `EventLoopTransport`: receive = the kernel receive prefixed by the
/// SEF classifier (ANY, `minix_sef::sef_receive_status` — pings answered and
/// signal requests intercepted inside the library and handed to the callback,
/// the same contract as C `sef.c:149-260`), reply = `ipc_sendnb` (main.c:273),
/// async = single-slot `senda` with `AMF_NOREPLY` (the asynsend3 shape of
/// main.c:207-208, following the input serve.rs precedent).
pub struct SysEventLoopTransport {
    inner: DirectTrapTransport,
}

/// `DirectTrapTransport` → [`minix_sef::SefIpc`] adapter (one line of the same
/// family as devman's `SefBridge`).
struct TrapSefBridge<'a>(&'a mut DirectTrapTransport);

impl minix_sef::SefIpc for TrapSefBridge<'_> {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        // minix-sys's `IpcStatus` is a tuple struct over the status word; the
        // error side `TrapStatus(pub i32)` expands directly into an errno.
        self.0
            .receive(src, msg)
            .map(|sts| sts.0 as i32)
            .map_err(|t| t.0)
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.0.notify(dest).map_err(|t| t.0)
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        // The birth reply leg (C `ipc_sendrec(RS, &m)`, sef_init.c:458-466).
        self.0.sendrec(dest, msg).map(|_| 0).map_err(|t| t.0)
    }
}

impl SysEventLoopTransport {
    pub const fn new() -> Self {
        Self {
            inner: DirectTrapTransport,
        }
    }
}

impl Default for SysEventLoopTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl EventLoopTransport for SysEventLoopTransport {
    fn receive(
        &mut self,
        on_signal: &mut dyn FnMut(i32),
        cancel: &minix_sef::SefCancel,
    ) -> Result<(Message, IpcStatus), TransportError> {
        // SEF classifier in front (the same contract as C `sef.c:149-260`):
        // pings are answered inside the library, and signal requests are
        // intercepted inside the library, handing the number to the callback and
        // swallowing the frame (notify form in ascending bit order,
        // sef_signal.c:94-113; manager form a single number, :115-128). The thin
        // TrapSefBridge layer is required: the library entry takes
        // `impl SefIpc` generically while `inner` is a field borrow.
        let mut msg = Message::default();
        let mut bridge = TrapSefBridge(&mut self.inner);
        // The one-line closure is required: the library entry monomorphizes
        // over `impl FnMut` while the trait side can only take an unsized
        // `dyn FnMut`, and passing it straight through hits E0277 (the same trap
        // as IS).
        let mut handoff = |signo: i32| on_signal(signo);
        let recv = minix_sef::sef_receive_status(
            &mut bridge,
            Endpoint::ANY,
            &mut msg,
            &mut handoff,
            cancel,
        )
        .map_err(|_| TransportError)?;
        let call = (recv.status & 0x3F) as u32;
        let notify = call == 4; // NOTIFY(com.h:92)
        Ok((recv.message, IpcStatus { notify, call }))
    }

    fn send_reply(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError> {
        self.inner.sendnb(dest, msg).map_err(|_| TransportError)
    }

    fn send_rec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), TransportError> {
        // The birth-reply leg is C's `ipc_sendrec` (sef_init.c:458-466).
        self.inner.sendrec(dest, msg).map_err(|_| TransportError)
    }

    fn send_async(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError> {
        let slot = AsyncSlot {
            flags: AsyncSlotFlags(AsyncSlotFlags::VALID.0 | AsyncSlotFlags::NO_REPLY.0),
            destination: dest,
            message: *msg,
            result: 0,
        };
        self.inner.senda(&[slot]).map_err(|_| TransportError)
    }
}
