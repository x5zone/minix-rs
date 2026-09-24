//! Production transport: every verb is a kernel call through `minix-sys`.
//!
//! C correspondence: the direct primitive calls inside
//! `chardriver_task`/`blockdriver_task`/`netdriver_task` and the transfer
//! hooks (`sys_safecopyfrom`/`sys_safecopyto`, e.g. `chardriver.c:355`
//! area). Same shape as the input server's `KernelTransport`
//! (`servers/input/src/serve.rs`), which is the field-proven template.

use minix_sys::ds::DsClient;
use minix_sys::ipc::{AsyncSendQueue, AsyncSlotFlags, DirectTrapTransport, IpcTransport as _};
use minix_sys::syscall::{sys_safecopyfrom, sys_safecopyto, DirectKernelCallTransport};
use minix_sys::{receive, send};
use minix_types::{Endpoint, Message};

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
    /// flushed by the next `senda` re-registration (NK4-C B7 same-shape
    /// defect; on RS it was the confirmed boot-deadlock root cause). The
    /// `DriverRuntime<T>` owns this transport for the driver's whole
    /// lifetime, so the slots outlive each `asynsend` call.
    senda_queue: AsyncSendQueue<16>,
}

impl KernelTransport {
    /// Transport bound to this process's endpoint.
    pub const fn new(self_endpoint: Endpoint) -> Self {
        KernelTransport {
            self_endpoint,
            senda_queue: AsyncSendQueue::new(),
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

    fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        msg.m_source = self.self_endpoint;
        // C: driver asynsend → asynsend3 uses the **process-global persistent**
        // multi-entry `static msgtable[ASYN_NR]` (asynsend.c:18) — appends,
        // never flushes. The prior stack-local single slot got overwritten by
        // the next `senda` (stale `s_asyntab` + dead frame → lost pending
        // message; NK4-C B7 same-shape defect). Route through the persistent
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

    fn publish_label(&mut self, label: &str) -> Result<(), i32> {
        // C: `chardriver_announce` 的 DS 发布半（chardriver.c:99-127 →
        // ds_publish_label）。DsClient 双载体参数化（ds.rs，T7 约束放宽）
        // 后单一直传载体可直接装配：DS 对话 = int-33 IPC 腿，grant 生
        // 命线 = SYSCALL 腿；宿主构建诚实 -EIO。grant 链（grant.rs
        // register）上浮正 errno、taskcall 链上浮负状态——此处归一为本
        // trait 其余动词的负状态约定。
        let mut ds = DsClient::new(DirectTrapTransport, DirectKernelCallTransport, Endpoint::DS);
        ds.publish_label(label, Endpoint::NONE, minix_types::DsFlags::empty())
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
        ds.retrieve_label_endpt(label).map(|(endpoint, _)| endpoint).ok()
    }
}

#[cfg(test)]
mod kernel_transport_tests {
    use super::*;
    use crate::transport::DriverTransport;

    /// 宿主构建（未开 real-trap）下 SENDA 腿诚实 -EIO（SENDA 表接线
    /// 落地后的门控验证，input `KernelTransport::asynsend` 同款）。
    #[test]
    fn test_asynsend_hosted_is_eio() {
        let mut t = KernelTransport::new(Endpoint::PM);
        let mut msg = Message::default();
        assert_eq!(t.asynsend(Endpoint::DS, &mut msg), Err(-minix_types::EIO));
    }

    /// NK4-C B7 regression: two async sends must BOTH stay pending in the
    /// persistent ring — the second `senda` must not flush the first. The old
    /// stack-local single-slot form could only ever hold one entry (the
    /// confirmed boot-deadlock root cause on RS); this pins the fix at the
    /// wiring level. Host builds answer EIO on the trap, but `enqueue`
    /// succeeds for both, so `pending_count` reaches 2.
    #[test]
    fn test_asynsend_accumulates_without_flush() {
        let mut t = KernelTransport::new(Endpoint::PM);
        let mut m1 = Message::default();
        let mut m2 = Message::default();
        m2.m_type = 7;
        let _ = t.asynsend(Endpoint::DS, &mut m1);
        let _ = t.asynsend(Endpoint::VM, &mut m2);
        assert_eq!(
            t.senda_queue.pending_count(),
            2,
            "second asynsend must append, not flush the first (persistent ring)"
        );
    }

    /// DS announce 腿经双载体 DsClient：宿主下 grant 生命线首跳即
    /// -EIO 并诚实上浮（publish_label 从 ENOSYS 登记位变真实路径后的
    /// fail-closed 语义）。
    #[test]
    fn test_publish_label_hosted_is_eio() {
        let mut t = KernelTransport::new(Endpoint::PM);
        assert_eq!(t.publish_label("drv.chr.t7"), Err(-minix_types::EIO));
    }

    /// DS label 查表腿经双载体 DsClient：宿主下查表不可达诚实返回
    /// `None`（调用方读作 C 的"忽略该消息"，与 publish 的 fail-closed
    /// 上浮同源，但查表失败在 C 里是静默忽略而非错误码）。
    #[test]
    fn test_lookup_label_hosted_is_none() {
        let mut t = KernelTransport::new(Endpoint::PM);
        assert_eq!(t.lookup_label("input"), None);
    }
}
