//! Data Store client — the Rust rewrite of C libsys `ds.c` (219 lines).
//!
//! E-DSWIRE transport half: every DS consumer (RS manager.c:513/800, VFS
//! main.c:441, input, storage/filter, IS dmp_ds) goes through these
//! entry points instead of hand-building `m_ds_req` messages. The
//! protocol shape: one key grant (READ for outgoing names, WRITE for
//! incoming names), optional value grant (publish/retrieve of memory),
//! one `_taskcall(DS_PROC_NR, letter, &m)`, revoke.
//!
//! C anchors: `do_invoke_ds` (ds.c:7-34) is the shared skeleton; the
//! per-call entry points are ds.c:36-219. Reply-value lanes live in
//! `m_ds_reply` (`val_out`), except `ds_check` whose answer is written
//! back into the *request* lanes (`m_ds_req.flags`/`owner`, ds.c:215-216)
//! — [`DsCheckReply`] names that reuse.
//!
//! Transport: pre-E1 the minix-sys IPC transport answers `-EIO`, so every
//! method fails honestly; the table itself (grants) is real user memory
//! and its lifecycle (grant → taskcall → revoke) is fully exercised.

use minix_types::{
    DS_CHECK, DS_DELETE, DS_MAX_KEYLEN, DS_PUBLISH, DS_RETRIEVE, DS_RETRIEVE_LABEL, DS_SUBSCRIBE,
    DsFlags, DsVal, Endpoint, Message,
};

use crate::grant::GrantTable;
use crate::ipc::IpcTransport;
use crate::syscall::{KernelCallTransport, perform_taskcall};

/// Reply of [`DsClient::check`]: the answer comes back in the *request*
/// lanes (C ds.c:215-216).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DsCheckReply {
    /// Entry type mask. C: `*type = m_ds_req.flags` — ds.c:215.
    pub entry_type: DsFlags,
    /// Publisher endpoint. C: `*owner_e = m_ds_req.owner` — ds.c:216.
    pub owner: i32,
}

/// One DS client bound to two transport legs and the store's endpoint.
///
/// C 的一个进程自带两条硬件腿：DS 对话走 int-33 IPC（`_taskcall`），
/// grant 生命线走 SYSCALL（`cpf_*` = SYS_SAFECOPY 家族）。`IpcTransport`
/// 与 `KernelCallTransport` 分立成两个载体参数（T7 约束放宽，2026-09-18）：
/// 单一直传载体不再被迫聚合双 trait（input 聚合 workaround 与
/// minix-driver-rt 的 ENOSYS 登记随此解除）。
///
/// The grant table is owned by the *process*, not the client: C keeps one
/// global libc `grants` array (safecopies.c) shared by every `cpf_*` user
/// in the address space, and the kernel stores a single `s_grant_table`
/// per privilege (`SYS_SETGRANT` is last-writer-wins). A client therefore
/// borrows the caller's table on each call — constructing several
/// `DsClient`s in one process must reuse the same `GrantTable`, never own
/// a private one, or the divergent tables hijack the kernel's view and
/// every safecopy read hits a stale slot (NK4-C 1.24 B13 root cause).
pub struct DsClient<I: IpcTransport, K: KernelCallTransport> {
    /// DS 对话腿：publish/retrieve/check 的 `_taskcall(DS, ...)`。
    ipc: I,
    /// grant 生命线腿：`grant_direct`/`revoke` 的内核调用。
    kernel: K,
    ds_endpoint: Endpoint,
}

impl<I: IpcTransport, K: KernelCallTransport> DsClient<I, K> {
    /// Bind to the store at `ds_endpoint`. Grant operations borrow the
    /// process's `GrantTable` via each method's `grants` argument.
    pub fn new(ipc: I, kernel: K, ds_endpoint: Endpoint) -> Self {
        Self {
            ipc,
            kernel,
            ds_endpoint,
        }
    }

    /// The store endpoint this client talks to.
    pub fn ds_endpoint(&self) -> Endpoint {
        self.ds_endpoint
    }

    /// Shared skeleton (C `do_invoke_ds`, ds.c:7-34): grant the key
    /// (WRITE roomy buffer for CHECK/RETRIEVE_LABEL, READ `name+NUL`
    /// otherwise), stamp `key_grant`/`key_len`, taskcall, revoke.
    fn invoke(
        &mut self,
        grants: &mut GrantTable,
        call: i32,
        name: &[u8],
        flags: i32,
        val_in: Option<DsVal>,
        val_len: i32,
    ) -> Result<Message, i32> {
        // C: ds.c:13-19 — CHECK/RETRIEVE_LABEL receive INTO the key buffer
        // (an 80-byte roomy WRITE grant); everything else sends the name out.
        let (key_len, write_key) = if call == DS_CHECK || call == DS_RETRIEVE_LABEL {
            (DS_MAX_KEYLEN, true)
        } else {
            (name.len(), false)
        };

        // C: ds.c:13-19 — key grant direction/length per letter. Rust keeps
        // the granted buffer alive in `key_buf` until after the taskcall
        // (the grant points at real memory for the whole round trip).
        let (key_grant, _key_buf) = if write_key {
            // CHECK/RETRIEVE_LABEL: C grants WRITE over the caller's key
            // buffer — in ds.c that buffer doubles as the name source
            // (ds_key). Rust splits roles: the caller's `name` bytes seed
            // the buffer contents; the store writes its answer into it.
            let mut room = name.to_vec();
            room.resize(DS_MAX_KEYLEN, 0);
            let gid = grants.grant_direct(
                &self.kernel,
                self.ds_endpoint.0,
                room.as_ptr() as u64,
                key_len as u64,
                minix_types::CpFlags::WRITE,
            )?;
            (gid, room)
        } else {
            let mut buf = name.to_vec();
            buf.push(0); // C: strlen(ds_name) + 1 — the terminator travels
            let gid = grants.grant_direct(
                &self.kernel,
                self.ds_endpoint.0,
                buf.as_ptr() as u64,
                key_len as u64 + 1,
                minix_types::CpFlags::READ,
            )?;
            (gid, buf)
        };

        let mut msg = Message::default();
        {
            // SAFETY: m_ds_req is the documented DS request arm
            // (kernel-side: servers/ds reads it in dispatch::triage;
            // C ipc.h mess_ds_req).
            let req = unsafe { &mut msg.m_u.m_ds_req };
            req.key_grant = key_grant;
            req.key_len = if write_key {
                key_len as i32
            } else {
                name.len() as i32 + 1
            };
            req.flags = flags;
            if let Some(v) = val_in {
                req.val_in = v;
            }
            req.val_len = val_len;
        }

        // C: `_taskcall(DS_PROC_NR, type, m)` — ds.c:30. The outcome is
        // held unlifted until after the revoke below: the grant must stay
        // alive across the call, and C revokes unconditionally (ds.c:32),
        // on the transport-failure path too.
        let outcome = perform_taskcall(&self.ipc, self.ds_endpoint, call, &mut msg);
        let _ = grants.revoke(key_grant);
        let reply = outcome.map_err(|e| e.to_i32())?;

        if reply < 0 {
            return Err(-reply);
        }
        Ok(msg)
    }

    /// Publish an endpoint under a label name (C: `ds_publish_label`,
    /// ds.c:36-43).
    pub fn publish_label(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
        endpoint: Endpoint,
        extra: DsFlags,
    ) -> Result<(), i32> {
        let flags = (DsFlags::TYPE_LABEL.bits() | extra.bits()) as i32;
        self.invoke(
            grants,
            DS_PUBLISH,
            name.as_bytes(),
            flags,
            Some(DsVal::endpoint(endpoint)),
            0,
        )
        .map(|_| ())
    }

    /// Publish a u32 value (C: `ds_publish_u32`, ds.c:46-53).
    pub fn publish_u32(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
        value: u32,
        extra: DsFlags,
    ) -> Result<(), i32> {
        let flags = (DsFlags::TYPE_U32.bits() | extra.bits()) as i32;
        self.invoke(
            grants,
            DS_PUBLISH,
            name.as_bytes(),
            flags,
            Some(DsVal::number(value)),
            0,
        )
        .map(|_| ())
    }

    /// Publish a memory range (C: `ds_publish_mem`, ds.c:87-90). The
    /// buffer is granted READ for the duration of the call; the store
    /// keeps the grant ID (not a copy) — the buffer must outlive the
    /// entry, exactly as in C.
    pub fn publish_mem(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
        buffer: &[u8],
        extra: DsFlags,
    ) -> Result<(), i32> {
        let flags = (DsFlags::TYPE_MEM.bits() | extra.bits()) as i32;
        let val_grant = grants.grant_direct(
            &self.kernel,
            self.ds_endpoint.0,
            buffer.as_ptr() as u64,
            buffer.len() as u64,
            minix_types::CpFlags::READ,
        )?;
        // C ds_publish_raw: val_in.grant = gid, val_len = length, type arm
        // NOT ORed here — publish_mem's flags arrive with TYPE_MEM set
        // (ds.c:89), and ds_publish_raw passes `flags` verbatim (ds.c:71).
        let r = self.invoke(
            grants,
            DS_PUBLISH,
            name.as_bytes(),
            flags,
            Some(DsVal::grant(val_grant)),
            buffer.len() as i32,
        );
        let _ = grants.revoke(val_grant);
        r.map(|_| ())
    }

    /// Publish a string (C: `ds_publish_str`, ds.c:79-85). The terminator
    /// travels (strlen + 1 bytes granted).
    pub fn publish_str(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
        value: &str,
        extra: DsFlags,
    ) -> Result<(), i32> {
        let flags = (DsFlags::TYPE_STR.bits() | extra.bits()) as i32;
        let mut buf = value.as_bytes().to_vec();
        buf.push(0); // C ds.c:83-84 — value[length-1] = '\0'
        let val_grant = grants.grant_direct(
            &self.kernel,
            self.ds_endpoint.0,
            buf.as_ptr() as u64,
            buf.len() as u64,
            minix_types::CpFlags::READ,
        )?;
        let r = self.invoke(
            grants,
            DS_PUBLISH,
            name.as_bytes(),
            flags,
            Some(DsVal::grant(val_grant)),
            buf.len() as i32,
        );
        let _ = grants.revoke(val_grant);
        r.map(|_| ())
    }

    /// Retrieve a u32 value (C: `ds_retrieve_u32`, ds.c:115-125).
    pub fn retrieve_u32(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
    ) -> Result<(u32, DsFlags), i32> {
        let flags = DsFlags::TYPE_U32.bits() as i32;
        let msg = self.invoke(grants, DS_RETRIEVE, name.as_bytes(), flags, None, 0)?;
        // SAFETY: the store fills m_ds_reply on success (ds.c:123).
        let arm = unsafe { &msg.m_u.m_ds_reply };
        Ok((
            arm.val_out.as_number(),
            DsFlags::from_bits_truncate(arm.val_len as u32),
        ))
    }

    /// Retrieve the label name of an endpoint (C: `ds_retrieve_label_name`,
    /// ds.c:92-101): `DS_RETRIEVE_LABEL` carries the endpoint in
    /// `val_in.ep`; the store writes the label name (NUL-pinned) into the
    /// WRITE-granted key buffer. Returns the label length in bytes.
    pub fn retrieve_label_name(
        &mut self,
        grants: &mut GrantTable,
        endpoint: Endpoint,
        buf: &mut [u8],
    ) -> Result<usize, i32> {
        let room = buf.len().min(DS_MAX_KEYLEN);
        let mut seed = alloc::vec![0u8; DS_MAX_KEYLEN];
        // WRITE grant 必须源自 *mut(as_ptr 派生的写别名是 UB);store
        // 的写入经 grant 直达这块内存,mut 借用覆盖整个 taskcall。
        let key_grant = grants.grant_direct(
            &self.kernel,
            self.ds_endpoint.0,
            seed.as_mut_ptr() as u64,
            DS_MAX_KEYLEN as u64,
            minix_types::CpFlags::WRITE,
        )?;

        let mut msg = Message::default();
        {
            // SAFETY: m_ds_req request arm (C mess_ds_req; the store reads
            // val_in.ep and writes the answer through the key grant).
            let req = unsafe { &mut msg.m_u.m_ds_req };
            req.key_grant = key_grant;
            req.key_len = DS_MAX_KEYLEN as i32;
            req.flags = DsFlags::TYPE_LABEL.bits() as i32;
            req.val_in = DsVal::endpoint(endpoint);
        }
        // Same unlifted-outcome shape as `check`: the WRITE grant stays
        // alive across the taskcall; revoke runs on every path (ds.c:32).
        let outcome = perform_taskcall(&self.ipc, self.ds_endpoint, DS_RETRIEVE_LABEL, &mut msg);
        let _ = grants.revoke(key_grant);
        let reply = outcome.map_err(|e| e.to_i32())?;
        if reply < 0 {
            return Err(-reply);
        }
        // The store wrote the name through the grant into `seed` — copy it
        // back (NUL-pinned, C ds.c:155's terminator discipline).
        let len = seed.iter().position(|&b| b == 0).unwrap_or(room).min(room);
        buf[..len].copy_from_slice(&seed[..len]);
        Ok(len)
    }

    /// Retrieve the endpoint behind a label (C: `ds_retrieve_label_endpt`,
    /// ds.c:103-113).
    pub fn retrieve_label_endpt(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
    ) -> Result<(Endpoint, DsFlags), i32> {
        let flags = DsFlags::TYPE_LABEL.bits() as i32;
        let msg = self.invoke(grants, DS_RETRIEVE, name.as_bytes(), flags, None, 0)?;
        // SAFETY: reply arm on success (ds.c:111).
        let arm = unsafe { &msg.m_u.m_ds_reply };
        Ok((
            arm.val_out.as_endpoint(),
            DsFlags::from_bits_truncate(arm.val_len as u32),
        ))
    }

    /// Retrieve a string. The caller offers `len_str` text bytes; one more
    /// moves for the terminator (C ds.c:152-153), which this method pins
    /// on return (ds.c:155). Returns the entry's flags.
    pub fn retrieve_str(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
        value: &mut [u8],
    ) -> Result<(usize, DsFlags), i32> {
        let flags = DsFlags::TYPE_STR.bits() as i32;
        let grant_len = value.len();
        let val_grant = grants.grant_direct(
            &self.kernel,
            self.ds_endpoint.0,
            value.as_mut_ptr() as u64,
            grant_len as u64,
            minix_types::CpFlags::WRITE,
        )?;
        let r = self.invoke(
            grants,
            DS_RETRIEVE,
            name.as_bytes(),
            flags,
            Some(DsVal::grant(val_grant)),
            grant_len as i32,
        );
        let _ = grants.revoke(val_grant);
        let msg = r?;
        // SAFETY: reply lane carries the moved length (ds.c:144).
        let arm = unsafe { &msg.m_u.m_ds_reply };
        let moved = (arm.val_len as usize).min(value.len());
        if moved > 0 {
            value[moved - 1] = 0; // C ds.c:155 — pin the terminator
        }
        Ok((moved, DsFlags::from_bits_truncate(0)))
    }

    /// Retrieve a memory range (C: `ds_retrieve_mem`, ds.c:159-162). The
    /// offered length travels in, the moved length comes back.
    pub fn retrieve_mem(
        &mut self,
        grants: &mut GrantTable,
        name: &str,
        buffer: &mut [u8],
    ) -> Result<(usize, DsFlags), i32> {
        let flags = DsFlags::TYPE_MEM.bits() as i32;
        let val_grant = grants.grant_direct(
            &self.kernel,
            self.ds_endpoint.0,
            buffer.as_mut_ptr() as u64,
            buffer.len() as u64,
            minix_types::CpFlags::WRITE,
        )?;
        let r = self.invoke(
            grants,
            DS_RETRIEVE,
            name.as_bytes(),
            flags,
            Some(DsVal::grant(val_grant)),
            buffer.len() as i32,
        );
        let _ = grants.revoke(val_grant);
        let msg = r?;
        // SAFETY: reply lane carries the moved length (ds.c:144).
        let arm = unsafe { &msg.m_u.m_ds_reply };
        let moved = (arm.val_len as usize).min(buffer.len());
        Ok((moved, DsFlags::from_bits_truncate(0)))
    }

    /// Delete an entry of the given type arm (C: `ds_delete_u32/str/mem/
    /// label`, ds.c:164-198 — four entry points, one shape).
    pub fn delete(&mut self, grants: &mut GrantTable, name: &str, arm: DsFlags) -> Result<(), i32> {
        self.invoke(
            grants,
            DS_DELETE,
            name.as_bytes(),
            arm.bits() as i32,
            None,
            0,
        )
        .map(|_| ())
    }

    /// Subscribe with a regexp (C: `ds_subscribe`, ds.c:200-207 — the key
    /// grant carries the pattern, flags verbatim).
    pub fn subscribe(
        &mut self,
        grants: &mut GrantTable,
        regexp: &str,
        flags: i32,
    ) -> Result<(), i32> {
        self.invoke(grants, DS_SUBSCRIBE, regexp.as_bytes(), flags, None, 0)
            .map(|_| ())
    }

    /// Check for a pending update (C: `ds_check`, ds.c:209-219). The key
    /// buffer is granted WRITE (the store writes the updated key into it,
    /// up to `DS_MAX_KEYLEN`); the answer rides the request lanes.
    pub fn check(
        &mut self,
        grants: &mut GrantTable,
        key: &mut [u8],
    ) -> Result<Option<DsCheckReply>, i32> {
        let room = (key.len()).min(DS_MAX_KEYLEN);
        let mut seed = key[..room].to_vec();
        seed.resize(DS_MAX_KEYLEN, 0);
        let key_grant = grants.grant_direct(
            &self.kernel,
            self.ds_endpoint.0,
            seed.as_ptr() as u64,
            DS_MAX_KEYLEN as u64,
            minix_types::CpFlags::WRITE,
        )?;

        let mut msg = Message::default();
        {
            // SAFETY: m_ds_req is the request arm; ds_check's answer rides
            // these lanes (ds.c:215-216).
            let req = unsafe { &mut msg.m_u.m_ds_req };
            req.key_grant = key_grant;
            req.key_len = DS_MAX_KEYLEN as i32;
        }
        // Same unlifted-outcome shape as `invoke`: the WRITE grant stays
        // alive across the call, and the revoke runs on every path — C's
        // `ds_check` (ds.c:209-219) rides `do_invoke_ds`, whose
        // unconditional revoke (ds.c:32) covers the failure case too.
        let outcome = perform_taskcall(&self.ipc, self.ds_endpoint, DS_CHECK, &mut msg);
        let reply = outcome.map_err(|e| e.to_i32())?;
        // SAFETY: the store wrote flags/owner back into the request lanes.
        let req = unsafe { &msg.m_u.m_ds_req };
        let has_event = reply >= 0 && req.flags != 0;
        // C ds_check 的 key 参数是 WRITE grant 登记的本进程内存：DS 把
        // 事件 key 写进去，revoke 后内容仍在（ds.c:209-219）。回拷给
        // 调用者——VFS 的 ds_event 靠 key 前缀分类（misc.c:952-970）。
        if has_event {
            let n = room.min(key.len());
            key[..n].copy_from_slice(&seed[..n]);
        }
        let _ = grants.revoke(key_grant);
        if reply < 0 {
            return Err(-reply);
        }
        if !has_event {
            return Ok(None); // No pending update for this subscriber.
        }
        Ok(Some(DsCheckReply {
            entry_type: DsFlags::from_bits_truncate(req.flags as u32),
            owner: req.owner,
        }))
    }
}

#[cfg(test)]
mod ds_client_tests {
    use super::*;
    use crate::ipc::CannedTransport;
    use crate::syscall::CannedKernelCallTransport;

    /// 双载体装配契约（T7 约束放宽，2026-09-18）：grant 生命线载体
    /// （`KernelCallTransport`，SYSCALL 腿）与 DS 对话载体
    /// （`IpcTransport`，int-33 腿）分立成两个参数——单一直传载体
    /// （如只实现其一的 CannedKernelCallTransport）不再被迫聚合双 trait
    /// （input 的聚合 workaround 与 minix-driver-rt 的 ENOSYS 登记随此
    /// 解除）。
    #[test]
    fn test_dual_carrier_assembly() {
        let ds = DsClient::new(
            CannedTransport::new(),
            CannedKernelCallTransport::new(),
            Endpoint::DS,
        );
        assert_eq!(ds.ds_endpoint(), Endpoint::DS);
    }

    /// publish 的两类动词各走各腿：grant 生命线落 kernel 载体（空脚本
    /// 内核调用即成功），`_taskcall(DS, DS_PUBLISH)` 落 ipc 载体（脚本
    /// OK 应答）——整链成功，无人再要求同一类型实现双 trait。
    #[test]
    fn test_publish_label_over_split_carriers() {
        let mut ipc = CannedTransport::new();
        let mut reply = Message::default();
        reply.m_type = 0; // do_invoke_ds 成功
        ipc.reply_sendrec(Ok(reply));
        let mut ds = DsClient::new(ipc, CannedKernelCallTransport::new(), Endpoint::DS);
        let mut grants = GrantTable::new();
        assert!(
            ds.publish_label(
                &mut grants,
                "drv.chr.t7",
                Endpoint::NONE,
                minix_types::DsFlags::empty()
            )
            .is_ok()
        );
    }
}
