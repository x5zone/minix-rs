//! Production transport: every verb is a kernel call through `minix-sys`.
//!
//! C correspondence: the direct primitive calls inside
//! `chardriver_task`/`blockdriver_task`/`netdriver_task` and the transfer
//! hooks (`sys_safecopyfrom`/`sys_safecopyto`, e.g. `chardriver.c:355`
//! area). Same shape as the input server's `KernelTransport`
//! (`servers/input/src/serve.rs`), which is the field-proven template.

use minix_sys::ds::DsClient;
use minix_sys::ipc::{AsyncSlot, AsyncSlotFlags, DirectTrapTransport, IpcTransport as _};
use minix_sys::syscall::{sys_safecopyfrom, sys_safecopyto, DirectKernelCallTransport};
use minix_sys::{receive, send};
use minix_types::{Endpoint, Message};

use crate::transport::DriverTransport;

/// The kernel transport behind real driver binaries.
pub struct KernelTransport {
    /// Endpoint of this process, stamped on outgoing messages
    /// (`m_source`; the kernel fills it on delivery, replies need it).
    pub self_endpoint: Endpoint,
}

impl KernelTransport {
    /// Transport bound to this process's endpoint.
    pub const fn new(self_endpoint: Endpoint) -> Self {
        KernelTransport { self_endpoint }
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
        // C: `asynsend3(endpt, &m, AMF_NOREPLY)` — a one-slot SENDA table
        // with the no-reply flag set (`ipc.rs:202`, flag value 8). SENDA
        // 客户端接线落地（input 794d3eb91 同型）：宿主构建诚实 -EIO，
        // real-trap 通电即走内核 SENDA。
        let slot = AsyncSlot {
            flags: AsyncSlotFlags(AsyncSlotFlags::VALID.0 | AsyncSlotFlags::NO_REPLY.0),
            destination: dst,
            result: 0,
            message: *msg,
        };
        DirectTrapTransport.senda(&[slot]).map_err(|_| -minix_types::EIO)
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

    /// DS announce 腿经双载体 DsClient：宿主下 grant 生命线首跳即
    /// -EIO 并诚实上浮（publish_label 从 ENOSYS 登记位变真实路径后的
    /// fail-closed 语义）。
    #[test]
    fn test_publish_label_hosted_is_eio() {
        let mut t = KernelTransport::new(Endpoint::PM);
        assert_eq!(t.publish_label("drv.chr.t7"), Err(-minix_types::EIO));
    }
}
