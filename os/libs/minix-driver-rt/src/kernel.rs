//! Production transport: every verb is a kernel call through `minix-sys`.
//!
//! C correspondence: the direct primitive calls inside
//! `chardriver_task`/`blockdriver_task`/`netdriver_task` and the transfer
//! hooks (`sys_safecopyfrom`/`sys_safecopyto`, e.g. `chardriver.c:355`
//! area). Same shape as the input server's `KernelTransport`
//! (`servers/input/src/serve.rs`), which is the field-proven template.

use minix_sys::ds::DsClient;
use minix_sys::grant::GrantTable;
use minix_sys::ipc::{AsyncSendQueue, AsyncSlotFlags, DirectTrapTransport, IpcTransport as _};
use minix_sys::syscall::{DirectKernelCallTransport, sys_datacopy, sys_safecopyfrom, sys_safecopyto};
use minix_sys::{receive, send, sendrec};
use minix_types::{CpFlags, Endpoint, Message};

use crate::transport::DriverTransport;

/// The kernel transport behind real driver binaries.
pub struct KernelTransport {
    /// Endpoint of this process, stamped on outgoing messages
    /// (`m_source`; the kernel fills it on delivery, replies need it).
    pub self_endpoint: Endpoint,
    /// Persistent SENDA ring table — the faithful port of C's
    /// `static asynmsg_t msgtable[ASYN_NR]` (`asynsend.c:18`). The kernel
    /// records only the user address of this table and re-reads it once the
    /// destination enters receive, so a stack-local single slot would be
    /// flushed by the next `senda` re-registration (the same shape as the
    /// defect that was the confirmed boot-deadlock root cause on RS). The
    /// `DriverRuntime<T>` owns this transport for the driver's whole
    /// lifetime, so the slots outlive each `asynsend` call.
    senda_queue: AsyncSendQueue<16>,
    /// The process's grant table (`cpf_grant_direct` / `cpf_revoke` are
    /// pure table edits; `sys_setgrant` registers the table with the
    /// kernel on growth). Drivers that hand their own request block to
    /// another process (the i2c exec request) allocate grants here.
    grants: GrantTable,
}

impl KernelTransport {
    /// Transport bound to this process's endpoint.
    pub fn new(self_endpoint: Endpoint) -> Self {
        KernelTransport {
            self_endpoint,
            senda_queue: AsyncSendQueue::new(),
            grants: GrantTable::new(),
        }
    }
}

impl DriverTransport for KernelTransport {
    fn receive(&mut self, msg: &mut Message) -> Result<(), i32> {
        // minix-sef switch point: swaps to `minix_sef::sef_receive_status`
        // when SEF lifecycle handling lands (IS/MIB precedent).
        receive(Endpoint::ANY, msg).map_err(|_| -minix_types::EIO)
    }

    fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        msg.m_source = self.self_endpoint;
        send(dst, msg).map_err(|_| -minix_types::EIO)
    }

    fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        msg.m_source = self.self_endpoint;
        // Birth-report leg, C `ipc_sendrec` (sef_init.c:458-466): a single-trap
        // send+receive that stops in the receive(RS) half waiting for RS's catch
        // to answer OK.
        sendrec(dst, msg).map_err(|_| -minix_types::EIO)
    }

    fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        msg.m_source = self.self_endpoint;
        // C: driver asynsend → asynsend3 uses the **process-global persistent**
        // multi-entry `static msgtable[ASYN_NR]` (asynsend.c:18) — appends,
        // never flushes. The prior stack-local single slot got overwritten by
        // the next `senda` (stale `s_asyntab` + dead frame -> lost pending
        // message; the same shape as the RS boot-deadlock defect). Route through
        // the persistent
        // `AsyncSendQueue`: `enqueue` (VALID OR-ed last, asynsend.c:124-131)
        // then hand `pending_slice` to `senda` (asynsend.c:154).
        self.senda_queue
            .enqueue(dst, *msg, AsyncSlotFlags::NO_REPLY)
            .map_err(|_| -minix_types::EBUSY)?;
        DirectTrapTransport
            .senda(self.senda_queue.pending_slice())
            .map_err(|_| -minix_types::EIO)
    }

    fn copy_from_grant(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        sys_safecopyfrom(
            &DirectKernelCallTransport,
            granter.get(),
            grant,
            offset,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }

    fn copy_to_grant(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32> {
        sys_safecopyto(
            &DirectKernelCallTransport,
            granter.get(),
            grant,
            offset,
            buf.as_ptr() as u64,
            buf.len() as u64,
        )
    }

    fn copy_to_addr(&mut self, dest: Endpoint, addr: u64, buf: &[u8]) -> Result<(), i32> {
        // C `store_t` (readclock.c:185-189): sys_datacopy(SELF, &t, who_e, addr, n).
        sys_datacopy(
            &DirectKernelCallTransport,
            self.self_endpoint.get(),
            buf.as_ptr() as u64,
            dest.get(),
            addr,
            buf.len() as u64,
        )
    }

    fn copy_from_addr(&mut self, src: Endpoint, addr: u64, buf: &mut [u8]) -> Result<(), i32> {
        // C `fetch_t` (readclock.c:179-183): sys_datacopy(who_e, addr, SELF, &t, n).
        sys_datacopy(
            &DirectKernelCallTransport,
            src.get(),
            addr,
            self.self_endpoint.get(),
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    }

    fn publish_label(&mut self, label: &str) -> Result<(), i32> {
        // C: the DS publish half of `chardriver_announce` (chardriver.c:99-127).
        // The verb is **`ds_publish_u32(key, DS_DRIVER_UP, DSF_OVERWRITE)`**
        // (chardriver.c:119), not publishing a label: only RS may publish
        // TYPE_LABEL (DS's source gate at store.c:302-304, "Only RS can publish
        // labels", hard-codes `m_source == RS_PROC_NR`), so a driver publishing
        // TYPE_LABEL always gets EPERM — on real hardware this killed MEM's and
        // TTY's announce outright (the earlier misuse of `publish_label` was
        // masked into a false success by a DS reply-sign defect).  The key name
        // reuses the caller-supplied `drv.chr.*`; C builds it with
        // `ds_retrieve_label_name(sef_self())`.
        //
        // DsClient is assembled from two carriers (ds.rs): the DS conversation
        // runs on the int-33 IPC leg and the grant lifetime on the SYSCALL leg;
        // hosted builds honestly return -EIO.  The grant chain (grant.rs
        // register) surfaces positive errnos while the taskcall chain surfaces
        // negative statuses — normalized here to the negative-status convention
        // used by the rest of this trait.
        let mut ds = DsClient::new(DirectTrapTransport, DirectKernelCallTransport, Endpoint::DS);
        let mut grants = minix_sys::grant::GrantTable::new();
        ds.publish_u32(
            &mut grants,
            label,
            minix_types::DS_DRIVER_UP as u32,
            minix_types::DsFlags::OVERWRITE,
        )
        .map_err(|e| -(e.unsigned_abs() as i32))
    }

    fn lookup_label(&mut self, label: &str) -> Option<Endpoint> {
        // C: `do_conf` resolves the server label through the data store
        // before comparing it to a sender (`inputdriver.c:82-111`). Same
        // double-carrier client as publish: the lookup is a DS conversation
        // on the IPC leg. A failed or unreachable lookup is `None`, which
        // the caller reads as C's "ignore the message" (`get_service_endpt`
        // failure path) — hosted builds (no DS) honestly get `None`.
        let mut ds = DsClient::new(DirectTrapTransport, DirectKernelCallTransport, Endpoint::DS);
        let mut grants = minix_sys::grant::GrantTable::new();
        ds.retrieve_label_endpt(&mut grants, label)
            .map(|(endpoint, _)| endpoint)
            .ok()
    }

    fn grant_direct(
        &mut self,
        dest: Endpoint,
        address: u64,
        len: usize,
        write: bool,
    ) -> Result<i32, i32> {
        // C `cpf_grant_direct` (safecopies.c:148-172): a pure table edit on
        // this process's own memory, registered with the kernel on growth.
        let access = if write {
            CpFlags::READ | CpFlags::WRITE
        } else {
            CpFlags::READ
        };
        self.grants
            .grant_direct(
                &DirectKernelCallTransport,
                dest.0,
                address,
                len as u64,
                access,
            )
            .map_err(|error| -error)
    }

    fn grant_revoke(&mut self, grant: i32) -> Result<(), i32> {
        // C `cpf_revoke` (safecopies.c:218-263).
        self.grants.revoke(grant).map(|_| ()).map_err(|error| -error)
    }

    fn now_seconds(&mut self) -> Result<i64, i32> {
        // C `time(NULL)` → `gettimeofday` through PM
        // (`libc/sys/gettimeofday.c`); the IPC leg is the same direct trap
        // the DS client uses.
        minix_sys::pm::gettimeofday_via(&DirectTrapTransport)
            .map(|(sec, _nsec)| sec)
            .map_err(|error| -error.to_i32())
    }

    fn label_name(&mut self, endpoint: Endpoint) -> Option<alloc::string::String> {
        // C `ds_retrieve_label_name` (`ds.c:92-101`) through the same
        // double-carrier client as the forward lookup; the name is a
        // NUL-terminated string in the store's answer.
        let mut ds = DsClient::new(DirectTrapTransport, DirectKernelCallTransport, Endpoint::DS);
        let mut grants = minix_sys::grant::GrantTable::new();
        let mut buf = [0u8; 80];
        let len = ds.retrieve_label_name(&mut grants, endpoint, &mut buf).ok()?;
        alloc::string::String::from_utf8(buf[..len].to_vec()).ok()
    }

    fn signal(&mut self, pid: i32, signal: i32) -> Result<(), i32> {
        // C `kill` → PM's kill call (`gettimeofday`'s sibling on the same
        // direct-trap IPC leg).
        minix_sys::pm::kill_via(&DirectTrapTransport, pid, signal)
            .map_err(|error| -error.to_i32())
    }
}

#[cfg(test)]
mod kernel_transport_tests {
    use super::*;
    use crate::transport::DriverTransport;

    /// In a hosted build (real-trap not enabled) the SENDA leg honestly returns
    /// -EIO (the gated check that follows the SENDA table wiring, the same shape
    /// as the input server's `KernelTransport::asynsend`).
    #[test]
    fn test_asynsend_hosted_is_eio() {
        let mut t = KernelTransport::new(Endpoint::PM);
        let mut msg = Message::default();
        assert_eq!(t.asynsend(Endpoint::DS, &mut msg), Err(-minix_types::EIO));
    }

    /// Regression guard: two async sends must BOTH stay pending in the
    /// persistent ring — the second `senda` must not flush the first. The old
    /// stack-local single-slot form could only ever hold one entry (the
    /// confirmed boot-deadlock root cause on RS); this pins the fix at the
    /// wiring level. Host builds answer EIO on the trap, but `enqueue`
    /// succeeds for both, so `pending_count` reaches 2.
    #[test]
    fn test_asynsend_accumulates_without_flush() {
        let mut t = KernelTransport::new(Endpoint::PM);
        let mut m1 = Message::default();
        let mut m2 = Message {
            m_type: 7,
            ..Message::default()
        };
        // [SD19-gate] This test pins that both asynsend calls enqueue and that
        // neither flushes the other (it asserts pending_count == 2); the per-call
        // result is ignored as a fixture.
        #[allow(clippy::let_underscore_must_use)]
        let _ = t.asynsend(Endpoint::DS, &mut m1);
        #[allow(clippy::let_underscore_must_use)]
        let _ = t.asynsend(Endpoint::VM, &mut m2);
        assert_eq!(
            t.senda_queue.pending_count(),
            2,
            "second asynsend must append, not flush the first (persistent ring)"
        );
    }

    /// The DS announce leg goes through the two-carrier DsClient: on the host the
    /// grant lifetime fails at its first hop with -EIO, which is surfaced honestly
    /// (the fail-closed semantics of publish_label now that it is a real path
    /// instead of a registered ENOSYS placeholder).
    #[test]
    fn test_publish_label_hosted_is_eio() {
        let mut t = KernelTransport::new(Endpoint::PM);
        assert_eq!(t.publish_label("drv.chr.t7"), Err(-minix_types::EIO));
    }

    /// The DS label lookup leg goes through the two-carrier DsClient: on the host
    /// the lookup is unreachable and honestly returns `None` (the caller reads it
    /// as C's "ignore this message"; it comes from the same fail-closed surfacing
    /// as publish, except that a failed lookup in C is a silent ignore rather than
    /// an error code).
    #[test]
    fn test_lookup_label_hosted_is_none() {
        let mut t = KernelTransport::new(Endpoint::PM);
        assert_eq!(t.lookup_label("input"), None);
    }
}
