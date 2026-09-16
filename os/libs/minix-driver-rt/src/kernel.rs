//! Production transport: every verb is a kernel call through `minix-sys`.
//!
//! C correspondence: the direct primitive calls inside
//! `chardriver_task`/`blockdriver_task`/`netdriver_task` and the transfer
//! hooks (`sys_safecopyfrom`/`sys_safecopyto`, e.g. `chardriver.c:355`
//! area). Same shape as the input server's `KernelTransport`
//! (`servers/input/src/serve.rs`), which is the field-proven template.

use alloc::string::String;
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
        // SENDA 表接线前以阻塞 send 承载异步回复（input 先例，登记：
        // C asynsend3 AMF_NOREPLY 的异步语义挂 SENDA 客户端接线，
        // edge E1/E2 轨道）。
        send(dst, msg).map_err(|_| -minix_types::EIO)
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
        let _ = String::from(label);
        // 登记：DsClient<T> 要求 T 同时实现 IpcTransport +
        // KernelCallTransport（ds.rs:44），单一直传载体只能实现其一——
        // 与 input 的 publish_label 同一条裁决待办（edge E-DSWIRE 线）。
        Err(-minix_types::ENOSYS)
    }
}
