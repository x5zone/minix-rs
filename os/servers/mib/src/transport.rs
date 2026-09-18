//! The two seams: kernel verbs and peer-service verbs (A-12).
//!
//! MIB talks on two channels, and the split is the SCHED/DS house
//! shape (`os/servers/sched/src/kernel_api/transport.rs`,
//! `os/servers/ds/src/server.rs`): messages with peers versus calls
//! into the kernel — a test double for one need not script the other.
//!
//! - [`MibKernel`]: the kernel-call family — the two `sys_datacopy`
//!   directions, grant create/revoke over caller memory
//!   (`cpf_grant_magic`/`cpf_revoke`), the process-table pull
//!   (`sys_getproctab`), and the clock (`getticks`/`sys_hz`).
//! - [`MibServices`]: peer messages — `getnuid` (PM, the auth round
//!   trip), `getsysinfo` (PM/VFS tables), `ds_retrieve_label_name`
//!   (DS, remote-mount registration), and the VM statistics pair
//!   (`vm_info_stats`/`vm_info_usage`).
//!
//! Both traits are the **final shape**: every verb MIB's handlers will
//! ever need is declared here, so later campaigns (P1-2 walker, P1-5
//! tables, P1-1 assembly) consume instead of extend.
//!
//! The real end ([`SysTransport`]) reports `-EIO` on every verb until
//! minix-sys grows the trap bodies and `SYS_*` wrappers — an honest
//! failure, never a fake success (edge E1/E2; the DS `SysKernel`
//! precedent). Tests drive the [`Recorder`] double instead.

use minix_sys::ds::DsClient;
use minix_sys::pm::getnuid_via;
use minix_sys::ipc::IpcTransport as _;
use minix_types::{EIO, Endpoint, GrantId};

use crate::io::relay::RelayDir;

/// Kernel calls: copy, grant, tables, clock (A-12's kernel half).
pub trait MibKernel {
    /// Copy from a caller's address space into `buf`.
    /// C: `sys_datacopy(src, src_addr, SELF, buf, n)` — main.c:183-184.
    fn datacopy_from(&mut self, src: Endpoint, src_addr: u64, buf: &mut [u8]) -> Result<(), i32>;

    /// Copy `buf` into a caller's address space.
    /// C: `sys_datacopy(SELF, buf, dest, dest_addr, n)` — main.c:136-138.
    fn datacopy_to(&mut self, dest: Endpoint, dest_addr: u64, buf: &[u8]) -> Result<(), i32>;

    /// Grant `whom` magic access to `[addr, addr+len)` in *our* address
    /// space (remote relay, 12). `dir` picks read/write.
    /// C: `cpf_grant_magic(whom, addr, len, flags)` — remote.c:396-413.
    fn grant_magic(
        &mut self,
        whom: Endpoint,
        addr: u64,
        len: u64,
        dir: RelayDir,
    ) -> Result<GrantId, i32>;

    /// Retire a grant. C: `cpf_revoke(id)` — remote.c:441-446.
    fn grant_revoke(&mut self, grant: GrantId);

    /// Pull the kernel process table (16). C: `sys_getproctab` —
    /// proc.c:75; the kernel chunks the copy (misc.rs:894).
    fn getproctab(&mut self, buf: &mut [u8]) -> Result<(), i32>;

    /// Uptime in clock ticks (16's pull discipline). C: `getticks` — proc.c:66.
    fn getticks(&mut self) -> Result<u64, i32>;

    /// Ticks per second (time conversions, 16). C: `sys_hz` — proc.c:126 一带.
    fn hz(&mut self) -> Result<u32, i32>;
}

/// Peer-service messages (A-12's service half).
pub trait MibServices {
    /// Ask PM for a caller's uid (auth, 07). C: `getnuid` — main.c:265-268.
    fn getnuid(&mut self, who: Endpoint) -> Result<u32, i32>;

    /// Pull a table image from a peer server (16). C: `getsysinfo` —
    /// proc.c:90 (`SI_PROC_TAB`), :106 (`SI_PROCLIGHT_TAB`).
    fn getsysinfo(&mut self, target: Endpoint, what: i32, buf: &mut [u8]) -> Result<(), i32>;

    /// Fetch a service's label name from DS by endpoint (register's
    /// first step, 12). C: `ds_retrieve_label_name` — remote.c:88.
    fn ds_retrieve_label_name(&mut self, who: Endpoint, buf: &mut [u8]) -> Result<usize, i32>;

    /// Fetch a remote subtree root's name and description at mount
    /// (12). C: `mib_remote_info` — remote.c:316-355 (two write
    /// grants, one COMMON_MIB_INFO round trip).
    fn remote_info(
        &mut self,
        peer: Endpoint,
        name_buf: &mut [u8],
        desc_buf: &mut [u8],
    ) -> Result<(), i32>;

    /// Pull VM statistics (14's CTL_VM handlers).
    /// C: `vm_info_stats`/`vm_info_usage` — vm.c:30-50 一带.
    fn vm_info(&mut self, what: i32, buf: &mut [u8]) -> Result<(), i32>;

    /// Relay a remote subtree call to its owning service (12).
    /// C: `ipc_sendrec(peer, &m_mib_lsys_call)` — remote.c:422-436;
    /// the reply's `status` rides back via the same message
    /// (`mess_lsys_mib_reply`, :461-464).
    fn remote_call(
        &mut self,
        peer: Endpoint,
        call: crate::io::relay::RemoteCall,
        reply: &mut crate::io::relay::RemoteReplyWire,
    ) -> Result<(), i32>;

    /// Pull PM boot parameters (13's boottime). C: `svrctl(PMGETPARAM)`.
    fn pm_getparam(&mut self, param: i32, buf: &mut [u8]) -> Result<(), i32>;
}

/// The real end: every verb fails honestly until E1/E2 land.
///
/// The shape is committed (each verb's signature is the wire contract
/// the minix-sys wrapper must satisfy); the power is not. When the
/// trap bodies arrive, each method swaps its `Err(EIO)` for the real
/// `minix-sys` call — call sites do not change.
#[derive(Debug, Clone, Copy, Default)]
pub struct SysTransport;

/// The real peer-service end. DS 动词走 [`DsClient`](`retrieve_label_name`,
/// minix-sys ds.rs 的 C ds.c:92-101 形状;标签查询的 DS 侧在 S22 通电后
/// 为真),getnuid 走 pm.rs `getnuid_via`;其余动词的 wrapper 未落地
/// (对端 producer 面归 S33 对账与 RS relay 域)——维持 `-EIO`
/// fail-closed,wrapper 落地时逐动词换真,调用点不变。
pub struct SysServices {
    ipc: minix_sys::ipc::DirectTrapTransport,
    ds: DsClient<minix_sys::ipc::DirectTrapTransport, minix_sys::syscall::DirectKernelCallTransport>,
}

impl Default for SysServices {
    fn default() -> Self {
        Self {
            ipc: minix_sys::ipc::DirectTrapTransport,
            ds: DsClient::new(
                minix_sys::ipc::DirectTrapTransport,
                minix_sys::syscall::DirectKernelCallTransport,
                Endpoint::DS,
            ),
        }
    }
}

impl MibKernel for SysTransport {
    fn datacopy_from(&mut self, _src: Endpoint, _src_addr: u64, _buf: &mut [u8]) -> Result<(), i32> {
        Err(EIO)
    }

    fn datacopy_to(&mut self, _dest: Endpoint, _dest_addr: u64, _buf: &[u8]) -> Result<(), i32> {
        Err(EIO)
    }

    fn grant_magic(
        &mut self,
        _whom: Endpoint,
        _addr: u64,
        _len: u64,
        _dir: RelayDir,
    ) -> Result<GrantId, i32> {
        Err(EIO)
    }

    fn grant_revoke(&mut self, _grant: GrantId) {
        // Nothing to revoke while nothing can be granted; a real
        // revoke failure is logged, never fatal (C ignores it too,
        // remote.c:441-446 — the grants die with the process anyway).
    }

    fn getproctab(&mut self, _buf: &mut [u8]) -> Result<(), i32> {
        Err(EIO)
    }

    fn getticks(&mut self) -> Result<u64, i32> {
        Err(EIO)
    }

    fn hz(&mut self) -> Result<u32, i32> {
        Err(EIO)
    }
}

impl MibServices for SysServices {
    fn getnuid(&mut self, who: Endpoint) -> Result<u32, i32> {
        // C main.c:265-268 getnuid(who) → PM_GETEPINFO(minix-sys
        // pm.rs getnuid_via;错误已折叠为 Errno,取 i32 透传)。
        getnuid_via(&self.ipc, who).map(|uid| uid as u32).map_err(|e| e.to_i32())
    }

    fn getsysinfo(&mut self, _target: Endpoint, _what: i32, _buf: &mut [u8]) -> Result<(), i32> {
        Err(EIO)
    }

    fn ds_retrieve_label_name(&mut self, who: Endpoint, buf: &mut [u8]) -> Result<usize, i32> {
        // C remote.c:88 ds_retrieve_label_name(who)——register 的第一
        // 步(remote.rs label_fits 的输入)。DS 侧在 S22 通电后为真。
        self.ds.retrieve_label_name(who, buf)
    }

    fn remote_info(
        &mut self,
        _peer: Endpoint,
        _name_buf: &mut [u8],
        _desc_buf: &mut [u8],
    ) -> Result<(), i32> {
        Err(EIO)
    }

    fn vm_info(&mut self, _what: i32, _buf: &mut [u8]) -> Result<(), i32> {
        Err(EIO)
    }

    fn remote_call(
        &mut self,
        _peer: Endpoint,
        _call: crate::io::relay::RemoteCall,
        _reply: &mut crate::io::relay::RemoteReplyWire,
    ) -> Result<(), i32> {
        Err(EIO)
    }

    fn pm_getparam(&mut self, _param: i32, _buf: &mut [u8]) -> Result<(), i32> {
        Err(EIO)
    }
}

#[cfg(test)]
pub(crate) mod recording {
    //! Scripted double: records the verb sequence (grant ordering
    //! tests rely on it) and answers from canned values.
    //!
    //! Not a crate feature — lives behind `cfg(test)` and is visible
    //! only to unit tests in this crate.

    use alloc::vec::Vec;
    use core::cell::RefCell;

    use minix_types::{Endpoint, GrantId};

    use crate::io::relay::RelayDir;

    /// One recorded verb call.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Call {
        DatacopyFrom(Endpoint, u64, usize),
        DatacopyTo(Endpoint, u64, usize),
        GrantMagic(Endpoint, u64, u64, RelayDir),
        Revoke(GrantId),
        Getproctab,
        Getnuid(Endpoint),
    }

    /// The recording transport. `fail` makes the copy/grant verbs fail
    /// (error-path tests); `uids`/`grants` are consumed FIFO;
    /// `canned_from` feeds `datacopy_from` byte by byte; `written`
    /// captures every `datacopy_to` payload so sinks can be asserted;
    /// `remote_status` is what `remote_call` reports.
    #[derive(Default)]
    pub struct Recorder {
        pub calls: RefCell<Vec<Call>>,
        pub fail: bool,
        pub uids: Vec<Result<u32, i32>>,
        pub grants: Vec<Result<GrantId, i32>>,
        pub canned_from: Vec<u8>,
        pub from_off: usize,
        pub written: RefCell<Vec<u8>>,
        pub remote_status: i32,
        /// Scripted DS label for `ds_retrieve_label_name` (None = DS
        /// unreachable).
        pub ds_label: Option<Vec<u8>>,
        /// Scripted tick count for `getticks`.
        pub getticks_result: Option<u64>,
    }

    impl Recorder {
        pub fn log(&self, call: Call) {
            self.calls.borrow_mut().push(call);
        }

        /// Grant ids seen, in creation order (ordering assertions).
        pub fn grant_ids(&self) -> Vec<GrantId> {
            self.calls
                .borrow()
                .iter()
                .filter_map(|c| match c {
                    Call::GrantMagic(_, _, _, _) => None,
                    Call::Revoke(id) => Some(*id),
                    _ => None,
                })
                .collect()
        }
    }

    impl super::MibKernel for Recorder {
        fn datacopy_from(
            &mut self,
            src: Endpoint,
            src_addr: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            self.log(Call::DatacopyFrom(src, src_addr, buf.len()));
            if self.fail {
                return Err(minix_types::EIO);
            }
            let end = (self.from_off + buf.len()).min(self.canned_from.len());
            let take = end - self.from_off;
            buf[..take].copy_from_slice(&self.canned_from[self.from_off..end]);
            buf[take..].fill(0);
            self.from_off += take;
            Ok(())
        }

        fn datacopy_to(&mut self, dest: Endpoint, dest_addr: u64, buf: &[u8]) -> Result<(), i32> {
            self.log(Call::DatacopyTo(dest, dest_addr, buf.len()));
            self.written.borrow_mut().extend_from_slice(buf);
            if self.fail {
                return Err(minix_types::EIO);
            }
            Ok(())
        }

        fn grant_magic(
            &mut self,
            whom: Endpoint,
            addr: u64,
            len: u64,
            dir: RelayDir,
        ) -> Result<GrantId, i32> {
            self.log(Call::GrantMagic(whom, addr, len, dir));
            if self.fail {
                return Err(minix_types::EINVAL);
            }
            // Seeded tests consume FIFO; unseeded tests get a
            // synthetic live grant id.
            match self.grants.first() {
                Some(Ok(id)) => {
                    let id = *id;
                    self.grants.remove(0);
                    Ok(id)
                }
                Some(Err(code)) => {
                    let code = *code;
                    self.grants.remove(0);
                    Err(code)
                }
                None => Ok(41),
            }
        }

        fn grant_revoke(&mut self, grant: GrantId) {
            self.log(Call::Revoke(grant));
        }

        fn getproctab(&mut self, _buf: &mut [u8]) -> Result<(), i32> {
            self.log(Call::Getproctab);
            if self.fail {
                return Err(minix_types::EIO);
            }
            Ok(())
        }

        fn getticks(&mut self) -> Result<u64, i32> {
            Ok(self.getticks_result.unwrap_or(7))
        }

        fn hz(&mut self) -> Result<u32, i32> {
            Ok(60)
        }
    }

    impl super::MibServices for Recorder {
        fn getnuid(&mut self, who: Endpoint) -> Result<u32, i32> {
            self.log(Call::Getnuid(who));
            // Unseeded tests answer superuser; seeded tests consume FIFO.
            match self.uids.first() {
                Some(Ok(uid)) => {
                    let uid = *uid;
                    self.uids.remove(0);
                    Ok(uid)
                }
                Some(Err(code)) => {
                    let code = *code;
                    self.uids.remove(0);
                    Err(code)
                }
                None => Ok(0),
            }
        }

        fn getsysinfo(
            &mut self,
            _target: Endpoint,
            _what: i32,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Err(minix_types::EIO)
        }

        fn ds_retrieve_label_name(&mut self, _who: Endpoint, buf: &mut [u8]) -> Result<usize, i32> {
            match &self.ds_label {
                Some(label) => {
                    buf[..label.len()].copy_from_slice(label);
                    Ok(label.len())
                }
                None => Err(minix_types::EIO),
            }
        }

        fn vm_info(&mut self, _what: i32, _buf: &mut [u8]) -> Result<(), i32> {
            Err(minix_types::EIO)
        }

        fn remote_info(
            &mut self,
            _peer: Endpoint,
            name_buf: &mut [u8],
            _desc_buf: &mut [u8],
        ) -> Result<(), i32> {
            let name = b"ipc";
            name_buf[..name.len()].copy_from_slice(name);
            Ok(())
        }

        fn remote_call(
            &mut self,
            _peer: Endpoint,
            _call: crate::io::relay::RemoteCall,
            reply: &mut crate::io::relay::RemoteReplyWire,
        ) -> Result<(), i32> {
            reply.req_id = 0;
            reply.status = self.remote_status;
            Ok(())
        }

        fn pm_getparam(&mut self, _param: i32, _buf: &mut [u8]) -> Result<(), i32> {
            Err(minix_types::EIO)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Endpoint;

    /// The honest stub: every verb reports EIO, nothing succeeds.
    #[test]
    fn test_sys_transport_fails_closed() {
        let mut t = SysTransport;
        let mut buf = [0u8; 4];
        assert_eq!(t.datacopy_from(Endpoint::PM, 8, &mut buf), Err(EIO));
        assert_eq!(t.datacopy_to(Endpoint::PM, 8, &buf), Err(EIO));
        assert_eq!(
            t.grant_magic(Endpoint::PM, 8, 4, crate::io::relay::RelayDir::Read),
            Err(EIO)
        );
        assert_eq!(t.getproctab(&mut buf), Err(EIO));
        assert_eq!(t.getticks(), Err(EIO));
        assert_eq!(t.hz(), Err(EIO));
        // Revokes stay silent — there is nothing to revoke.
        t.grant_revoke(3);
        // The service half fails closed just as honestly(宿主构建下
        // 真 wrapper 的 trap 后端回答 -EIO,断言不变)。
        let mut services = SysServices::default();
        let mut name_buf = [0u8; 8];
        assert_eq!(services.getnuid(Endpoint::PM), Err(EIO));
        assert_eq!(
            services.remote_info(Endpoint::PM, &mut name_buf, &mut []),
            Err(EIO)
        );
    }
}
