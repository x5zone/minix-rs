//! DS server assembly: the loop, the two seams, the seven arms.
//!
//! Mirrors `main()` and the handler glue (`minix3/minix/servers/ds/
//! main.c:28-131`, `store.c:287-678` commit halves). 01-ds-init-main.md
//! owns the loop shape; 07~11 own the arms.
//!
//! Two seams, because C talks on two channels: messages with peers
//! (`sef_receive`/`ipc_send`/`ipc_notify`) versus kernel calls
//! (`sys_safecopyfrom`/`sys_safecopyto`/`sys_datacopy`) — the SCHED
//! house split (`os/servers/sched/src/kernel_api/transport.rs`), and
//! a test double for one need not script the other.
//!
//! - [`DsIpc`]: receive any, send a reply, notify a subscriber. The
//!   real end delegates to minix-sys's `IpcTransport` — final shape,
//!   powered when the user-space trap layer lands (edge E1).
//! - [`DsKernel`]: the three copy verbs. The real end reports `EIO`
//!   until minix-sys grows the SYS_SAFECOPY*/SYS_DATACOPY wrappers
//!   (edge E2/E-DSWIRE) — the shape is committed, the power is not.
//!
//! Single-threaded event loop, one owner: the server holds both tables
//! and the pool; the loop borrows it plus the seams.

use minix_types::{
    DS_MAX_KEYLEN, DSF_MASK_INTERNAL, DSF_MASK_TYPE, DsFlags, DsVal, EINVAL, EPERM, ESRCH,
    Endpoint, GrantId, MessDsReq, Message, OK, RS_PROC_NR,
};

use crate::boot::{BootService, apply_boot_map};
use crate::check::{apply_check, plan_check};
use crate::delete::apply_delete;
use crate::dispatch::{DsCall, Incoming, should_reply, triage};
use crate::getsysinfo::{image_bytes, plan_getsysinfo};
use crate::heap::DsPool;
use crate::identity::resolve_name;
use crate::notify::{apply_update, initial_scan};
use crate::publish::{PublishTarget, check_key_len, plan_publish};
use crate::retrieve::{RetrieveHit, plan_retrieve, plan_retrieve_label};
use crate::slots::EntrySlot;
use crate::store::{DataBody, DataEntry, DsStore, MemBody, NR_DS_KEYS};
use crate::subscribe::{SubscribeArgs, apply_subscribe, plan_subscribe};
use crate::subscription::NR_DS_SUBS;
use minix_sys::ipc::IpcTransport as SysIpcTransport;

/// The message seam (C: `get_work`/`reply`/`ipc_notify`).
pub trait DsIpc {
    /// Receive the next message from anyone (C: `sef_receive(ANY)`,
    /// main.c:109-121). The legacy notify test (`is_notify`) works on
    /// the call number, so no status word crosses here.
    fn receive(&mut self) -> Result<Message, i32>;
    /// Send a message (C: `reply`'s `ipc_send`, main.c:123-131).
    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32>;
    /// Wake a subscriber (C: `ipc_notify`, store.c:222).
    fn notify(&self, who: Endpoint) -> Result<(), i32>;
}

/// The kernel-call seam (C: `sys_safecopyfrom`/`sys_safecopyto`/
/// `sys_datacopy`).
pub trait DsKernel {
    /// Copy `buf.len()` bytes from the caller's grant (store.c:167-172 —
    /// the key ferry; :337-349 — MEM publishes).
    fn safecopy_from(
        &mut self,
        caller: Endpoint,
        grant: GrantId,
        buf: &mut [u8],
    ) -> Result<(), i32>;
    /// Copy `buf.len()` bytes into the caller's grant (store.c:409-416,
    /// :441-444, :561-563 — the three read-back roads).
    fn safecopy_to(&mut self, caller: Endpoint, grant: GrantId, buf: &[u8]) -> Result<(), i32>;
    /// Copy server bytes into caller space (store.c:672-675 — the whole
    /// table image rides one call, SELF as source).
    fn data_copy_to(&mut self, caller: Endpoint, buf: &[u8]) -> Result<(), i32>;
}

/// Receive-failure bound (`store.c:39-40` panics on the first one; the
/// bound is the SCHED-documented honest deviation — a transient EIO
/// loses a turn, a broken transport dies at 64).
pub const MAX_CONSECUTIVE_RECV_FAILURES: u32 = 64;

/// One turn's outcome (`Handled` clears the failure counter,
/// `ReceiveFailed` counts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Handled,
    ReceiveFailed,
}

/// The DS server: both tables, the pool, the loop (SchedServer's single
/// owner, [ARCH S-11] shape).
///
/// The pool pins its bytes; `MemBody.data` points into this value, so a
/// server is **created once and never moved** (`new` → run). Tests build
/// it in place; the binary builds it in `main` and keeps it for life.
pub struct DsServer {
    /// The entry table (`ds_store`, store.c:5).
    pub store: DsStore,
    /// The subscription table (`ds_subs`, store.c:6).
    pub subs: [Option<crate::subscription::Subscription>; NR_DS_SUBS],
    /// STR/MEM buffer pool (A-3).
    pub heap: DsPool,
    /// Teardown scratch: buffers handed back by a delete, released into
    /// the pool after the sweep (A-3's "交还" half, 09 D3).
    teardown: [Option<MemBody>; NR_DS_KEYS],
    /// Image scratch for `do_getsysinfo` (24 KiB of BSS, not stack).
    image: [u8; image_bytes()],
}

impl Default for DsServer {
    fn default() -> Self {
        Self::new()
    }
}

impl DsServer {
    /// An empty server: both tables vacant, the pool whole.
    pub fn new() -> Self {
        Self {
            store: [None; NR_DS_KEYS],
            subs: [None; NR_DS_SUBS],
            heap: DsPool::new(),
            teardown: [None; NR_DS_KEYS],
            image: [0u8; image_bytes()],
        }
    }

    /// Forever (C: `while (TRUE)`, main.c:42). A broken transport dies
    /// at [`MAX_CONSECUTIVE_RECV_FAILURES`] — the honest deviation from
    /// C's panic-on-first-failure, shared with SCHED.
    pub fn run(&mut self, ipc: &mut impl DsIpc, kernel: &mut impl DsKernel) -> ! {
        let mut consecutive_failures: u32 = 0;
        loop {
            match self.run_once(ipc, kernel) {
                Step::Handled => consecutive_failures = 0,
                Step::ReceiveFailed => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if consecutive_failures >= MAX_CONSECUTIVE_RECV_FAILURES {
                        panic!(
                            "DS: IPC transport broken after {consecutive_failures} \
                             consecutive receive failures"
                        );
                    }
                }
            }
        }
    }

    /// One turn: receive, triage, dispatch, settle (C: `main.c:42-88`).
    ///
    /// The turn is the unit the loop semantics are tested against.
    /// Notifications and wild numbers refuse with `EINVAL` (C prints a
    /// warning beside it; the reply is the observable half); known
    /// letters dispatch; the answer rides back in the same message with
    /// `m_type` = result unless the handler held it (`EDONTREPLY` — no
    /// DS arm holds today, C main.c:82).
    pub fn run_once(&mut self, ipc: &mut impl DsIpc, kernel: &mut impl DsKernel) -> Step {
        let mut message = match ipc.receive() {
            Ok(m) => m,
            Err(_) => return Step::ReceiveFailed,
        };
        let caller = message.m_source;

        let result = match triage(message.m_type) {
            // C 47-51: a notification has nothing to dispatch.
            Incoming::NotifyRefusal => EINVAL,
            // C 75-77: a wild number has no road.
            Incoming::Unknown => EINVAL,
            Incoming::Dispatch(call) => self.dispatch(call, caller, &mut message, ipc, kernel),
        };

        // C 80-86: the answer rides back in the same message.
        if should_reply(result) {
            message.m_type = result;
            // C 127-129: a failed reply is logged and dropped — the loop
            // moves on; the drop is the observable half.
            let _ = ipc.send(caller, &message);
        }
        Step::Handled
    }

    /// Dispatch one known letter to its arm (C's `switch`, main.c:54-72).
    fn dispatch(
        &mut self,
        call: DsCall,
        caller: Endpoint,
        message: &mut Message,
        ipc: &mut impl DsIpc,
        kernel: &mut impl DsKernel,
    ) -> i32 {
        match call {
            DsCall::Publish => self.arm_publish(caller, ds_req(message), ipc, kernel),
            DsCall::Retrieve => self.arm_retrieve(caller, message, kernel),
            DsCall::RetrieveLabel => self.arm_retrieve_label(caller, ds_req_ref(message), kernel),
            DsCall::Delete => self.arm_delete(caller, ds_req_ref(message), ipc, kernel),
            DsCall::Subscribe => self.arm_subscribe(caller, ds_req_ref(message), ipc, kernel),
            DsCall::Check => self.arm_check(caller, message, kernel),
            DsCall::Getsysinfo => {
                // C reads the getsysinfo payload (main.c:72 → store.c:655)
                // — `mess_lsys_getsysinfo`, a different shape than the
                // DS request arm.
                let (what, size) = unsafe {
                    let g = &message.m_u.m_lsys_getsysinfo;
                    (g.what, g.size as usize)
                };
                match plan_getsysinfo(what, size) {
                    Ok(len) => {
                        // C 672-675: one `sys_datacopy(SELF → caller)`,
                        // the whole image.
                        self.render_image();
                        match kernel.data_copy_to(caller, &self.image[..len]) {
                            Ok(()) => OK,
                            Err(r) => r,
                        }
                    }
                    Err(reject) => reject.errno(),
                }
            }
        }
    }

    // ── DS_PUBLISH (store.c:287-378 verdict + commit) ──

    fn arm_publish(
        &mut self,
        caller: Endpoint,
        req: &mut MessDsReq,
        ipc: &mut impl DsIpc,
        kernel: &mut impl DsKernel,
    ) -> i32 {
        // C 297-299: the caller must have a name. The name snapshots
        // into a local lane: the verdict half reads the table, the
        // commit half mutates it — one borrow cannot hold both.
        let Some(source_lane) = resolve_name(&self.store, caller) else {
            return EPERM;
        };
        let mut source = [0u8; DS_MAX_KEYLEN];
        let source_len = {
            let n = strlen80(source_lane).min(DS_MAX_KEYLEN - 1);
            source[..n].copy_from_slice(&source_lane[..n]);
            n
        };
        let is_rs = caller == RS_PROC_NR;
        // C 306 + 158-181: bounds, grant copy, tail pin.
        let (key, key_len) = match Self::ferry_key(kernel, caller, req) {
            Ok(f) => f,
            Err(e) => return e,
        };
        let flags = DsFlags::from_bits_truncate(req.flags as u32);
        let label_ep = req.val_in.as_endpoint().0 as u32;
        let slot = match plan_publish(
            &self.store,
            Some(&source[..source_len]),
            is_rs,
            &key[..key_len],
            flags,
            label_ep,
        ) {
            Ok(target) => match target {
                PublishTarget::Create { slot } => slot,
                PublishTarget::Overwrite { slot } => slot,
            },
            Err(reject) => return reject.errno(),
        };
        let ty = flags.intersection(DsFlags::from_bits_truncate(DSF_MASK_TYPE));

        // ── commit: take the seat, patch it, put it back ──
        // A Create's seat is vacant (`None`) — the plan only names it;
        // the commit half builds the whole entry here (C allocates the
        // slot then writes fields: store.c:315-318 + 329-362).
        let was_wide = self.store[slot.index()]
            .as_ref()
            .is_some_and(|e| e.flags.intersects(DsFlags::TYPE_STR | DsFlags::TYPE_MEM));
        let mut entry = self.store[slot.index()].take().unwrap_or(DataEntry {
            flags: DsFlags::empty(),
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        });

        match ty {
            t if t == DsFlags::TYPE_U32 => entry.body.u32 = req.val_in.as_number(),
            t if t == DsFlags::TYPE_LABEL => entry.body.u32 = label_ep,
            t if t == DsFlags::TYPE_STR || t == DsFlags::TYPE_MEM => {
                let length = req.val_len.max(0) as usize;
                // C 331-352: reuse the buffer when it is big enough;
                // otherwise (re)allocate — C frees first, this pool
                // allocates the replacement first and releases the old
                // only on success (a superset: C leaves a dangling
                // pointer when the growth malloc fails — MINIX3 BUG,
                // store.c:341-345). A same-name publish that switches
                // arms allocates fresh, where C would read a stale
                // `reallen` out of the wrong union arm (MINIX3 BUG:
                // store.c:335 vs the type switch at :329).
                let old = if was_wide {
                    // SAFETY: wide-arm lanes only (03's documented arm).
                    Some(unsafe { entry.body.mem })
                } else {
                    None
                };
                let reuse = matches!(&old, Some(o) if length <= o.reallen);
                let mut work = if reuse {
                    old.unwrap()
                } else {
                    match self.heap.alloc(length) {
                        Ok((_, body)) => body,
                        Err(e) => {
                            self.store[slot.index()] = Some(entry);
                            return e;
                        }
                    }
                };
                let grant = req.val_in.as_grant();
                let copied =
                    kernel.safecopy_from(caller, grant, &mut self.heap.slice_mut(&work)[..length]);
                if let Err(r) = copied {
                    // C 350-352 frees on the failure road too (and
                    // dangles); the pool only retires a slot this call
                    // freshly minted — a reused buffer keeps its old
                    // bytes and the entry stays consistent.
                    if !reuse {
                        self.heap.release(&work);
                    }
                    self.store[slot.index()] = Some(entry);
                    return r;
                }
                work.length = length;
                if t == DsFlags::TYPE_STR {
                    // C 355: the server pins the terminator over the
                    // last byte moved (the server half of the NUL
                    // discipline; client.rs's `terminate` is the other).
                    let last = length.saturating_sub(1);
                    self.heap.slice_mut(&work)[last] = 0;
                }
                if !reuse && let Some(o) = old {
                    // C 341 frees the replaced buffer; this pool
                    // reclaims it only after the copy succeeded (no
                    // dangling on the ENOMEM road).
                    self.heap.release(&o);
                }
                entry.body.mem = work;
            }
            _ => {
                self.store[slot.index()] = Some(entry);
                return EINVAL; // plan_publish refuses first; unreachable
            }
        }

        // ── attributes (store.c:359-362) ──
        entry.key[..key_len].copy_from_slice(&key[..key_len]);
        if key_len < DS_MAX_KEYLEN {
            entry.key[key_len] = 0;
        }
        entry.owner[..source_len].copy_from_slice(&source[..source_len]);
        if source_len < DS_MAX_KEYLEN {
            entry.owner[source_len] = 0;
        }
        entry.flags = DsFlags::IN_USE | (flags & DsFlags::from_bits_truncate(DSF_MASK_INTERNAL));
        self.store[slot.index()] = Some(entry);

        // ── the ring (store.c:364 → update_subscribers(dsp, 1)) ──
        self.ring(slot, true, ipc);
        OK
    }

    // ── DS_RETRIEVE / DS_RETRIEVE_LABEL (store.c:383-454) ──

    fn arm_retrieve(
        &mut self,
        caller: Endpoint,
        message: &mut Message,
        kernel: &mut impl DsKernel,
    ) -> i32 {
        let (key_grant, key_len_raw, flags_raw, val_in, requested) = {
            let r = ds_req_ref(message);
            (
                r.key_grant,
                r.key_len,
                r.flags,
                r.val_in,
                r.val_len.max(0) as usize,
            )
        };
        let flags = DsFlags::from_bits_truncate(flags_raw as u32);
        // C 393: the key ferries first (bounds, grant copy, tail pin).
        let (key, key_len) = {
            let len = match check_key_len(key_len_raw.max(0) as usize) {
                Ok(l) => l,
                Err(e) => return e,
            };
            let mut key = [0u8; DS_MAX_KEYLEN];
            if let Err(r) = kernel.safecopy_from(caller, key_grant, &mut key[..len]) {
                return r;
            }
            key[DS_MAX_KEYLEN - 1] = 0;
            (key, len)
        };
        // C 399: the gate reads the caller through `check_auth`'s
        // selective protection; the tendered name keeps it pure (05).
        let caller_name = resolve_name(&self.store, caller).map(|lane| &lane[..]);
        let (slot, hit) =
            match plan_retrieve(&self.store, &key, key_len, flags, caller_name, requested) {
                Ok(hit) => hit,
                Err(reject) => return reject.errno(),
            };
        match hit {
            RetrieveHit::Number(v) => unsafe { message.m_u.m_ds_reply.val_out = DsVal::number(v) },
            RetrieveHit::Label(ep) => unsafe {
                message.m_u.m_ds_reply.val_out = DsVal::endpoint(Endpoint(ep as i32))
            },
            RetrieveHit::Bytes { len } => {
                // SAFETY: wide-arm descriptor lanes only (03); the bytes
                // live in this server's pool.
                let body: MemBody = unsafe { self.store[slot.index()].unwrap().body.mem };
                // C 409-416: copy MIN(requested, stored); a failed copy
                // returns the errno with no reply lanes touched.
                if let Err(r) =
                    kernel.safecopy_to(caller, val_in.as_grant(), &self.heap.slice(&body)[..len])
                {
                    return r;
                }
                // C 420: the moved length rides `m_ds_reply.val_len`.
                message.m_u.m_ds_reply.val_len = len as i32
            }
        }
        OK
    }

    fn arm_retrieve_label(
        &mut self,
        caller: Endpoint,
        req: &MessDsReq,
        kernel: &mut impl DsKernel,
    ) -> i32 {
        let slot = match plan_retrieve_label(&self.store, req.val_in.as_endpoint().0 as u32) {
            Ok(slot) => slot,
            Err(reject) => return reject.errno(),
        };
        let entry = slot.get(&self.store).expect("lookup hit seats a body");
        let n = strlen80(&entry.key) + 1; // with terminator (store.c:442-444)
        match kernel.safecopy_to(caller, req.key_grant, &entry.key[..n]) {
            Ok(()) => OK,
            Err(r) => r,
        }
    }

    // ── DS_DELETE (store.c:583-651) ──

    fn arm_delete(
        &mut self,
        caller: Endpoint,
        req: &MessDsReq,
        ipc: &mut impl DsIpc,
        kernel: &mut impl DsKernel,
    ) -> i32 {
        let Some(source) = resolve_name(&self.store, caller) else {
            return EPERM;
        };
        let (key, key_len) = match Self::ferry_key(kernel, caller, req) {
            Ok(f) => f,
            Err(e) => return e,
        };
        let flags = DsFlags::from_bits_truncate(req.flags as u32);
        let plan = match crate::delete::plan_delete(
            &self.store,
            &key,
            key_len,
            flags,
            Some(&source[..]),
        ) {
            Ok(plan) => plan,
            Err(reject) => return reject.errno(),
        };
        // Sweep-and-clear (09): each victim notifies through the ring
        // before its seat clears; wakes ride the transport (store.c:628,
        // :642 → ipc_notify :222).
        let effect = {
            let Self {
                store,
                subs,
                teardown,
                ..
            } = self;
            let woken: &mut dyn FnMut(Endpoint) = &mut |ep: Endpoint| {
                let _ = ipc.notify(ep);
            };
            apply_delete(store, subs, plan, woken, teardown)
        };
        // Buffers handed back retire into the pool (A-3: C frees, this
        // pool reclaims the slot).
        for i in 0..effect.heap_buffers {
            if let Some(body) = self.teardown[i] {
                self.heap.release(&body);
            }
            self.teardown[i] = None;
        }
        OK
    }

    // ── DS_SUBSCRIBE (store.c:456-534) ──

    fn arm_subscribe(
        &mut self,
        caller: Endpoint,
        req: &MessDsReq,
        ipc: &mut impl DsIpc,
        kernel: &mut impl DsKernel,
    ) -> i32 {
        let Some(owner) = resolve_name(&self.store, caller) else {
            return ESRCH; // ESRCH, not EPERM — a subscriber needs a name (10)
        };
        // C 488: the pattern ferries through the key grant. Order note:
        // C frees an overwritten seat *before* the ferry, so a failed
        // copy destroys the caller's old subscription; here the freed
        // seat only lands on success (apply_subscribe) — the failure
        // road keeps the old seat, a stated superset (10).
        let (key, key_len) = match Self::ferry_key(kernel, caller, req) {
            Ok(f) => f,
            Err(e) => return e,
        };
        let flags = DsFlags::from_bits_truncate(req.flags as u32);
        let args = SubscribeArgs {
            owner: Some(owner),
            key: &key,
            key_len,
            flags,
            overwrite: flags.contains(DsFlags::OVERWRITE),
            initial: flags.contains(DsFlags::INITIAL),
        };
        let (plan, freed) = match plan_subscribe(&self.subs, args) {
            Ok(plan) => plan,
            Err(reject) => return reject.errno(),
        };
        let mut pattern = [0u8; DS_MAX_KEYLEN];
        let copy_len = key.len().min(DS_MAX_KEYLEN - 1);
        pattern[..copy_len].copy_from_slice(&key[..copy_len]);
        apply_subscribe(&mut self.subs, owner, &pattern, plan, freed);

        // The immediate scan (C 511-528, DSF_INITIAL): bits set, and the
        // source wakes once when anything matched (:526).
        if plan.initial_scan
            && let Some(source_ep) =
                initial_scan(&self.store, &mut self.subs, plan.slot, caller, owner)
        {
            let _ = ipc.notify(source_ep);
        }
        OK
    }

    // ── DS_CHECK (store.c:536-578) ──

    fn arm_check(
        &mut self,
        caller: Endpoint,
        message: &mut Message,
        kernel: &mut impl DsKernel,
    ) -> i32 {
        let Some(owner) = resolve_name(&self.store, caller) else {
            return ESRCH;
        };
        let hit = match plan_check(&self.subs, Some(&owner[..])) {
            Ok(hit) => hit,
            Err(reject) => return reject.errno(),
        };
        // C 561-563: the key copies first; a failed copy must not consume
        // the update (`apply_check` only runs on the success road).
        let entry = hit.entry.get(&self.store).expect("told bit seats an entry");
        let n = strlen80(&entry.key) + 1;
        let (grant, bytes) = (ds_req_ref(message).key_grant, &entry.key[..n]);
        if let Err(r) = kernel.safecopy_to(caller, grant, bytes) {
            return r;
        }
        // C 570-572: the reply rides the request lanes (type mask +
        // publisher endpoint — client.rs's `CheckReply` names the reuse).
        let publisher = hit.reply_owner(&self.store);
        let entry_type = hit.reply_type(&self.store);
        let req = ds_req(message);
        req.flags = entry_type.bits() as i32;
        // C :570 would panic on an unresolvable publisher (`ds_getprocep`,
        // store.c:137); the deferred verdict lands here as endpoint 0 —
        // the registry stays up, the reply says "no name" (05 D2, 10).
        req.owner = publisher.unwrap_or(Endpoint(0)).0;
        apply_check(&mut self.subs, hit);
        OK
    }

    // ── shared machinery ──

    /// The key ferry (C `get_key_name`, store.c:158-181): bounds refuse,
    /// `key_len` bytes copy through the grant, and the lane's last byte
    /// pins to NUL (:178 — the server half of the terminator discipline).
    fn ferry_key(
        kernel: &mut impl DsKernel,
        caller: Endpoint,
        req: &MessDsReq,
    ) -> Result<([u8; DS_MAX_KEYLEN], usize), i32> {
        let len = check_key_len(req.key_len.max(0) as usize)?;
        let mut key = [0u8; DS_MAX_KEYLEN];
        kernel.safecopy_from(caller, req.key_grant, &mut key[..len])?;
        key[DS_MAX_KEYLEN - 1] = 0;
        Ok((key, len))
    }

    /// Ring one entry's subscribers (C `update_subscribers`' notify half,
    /// store.c:222): the sweep decides whom, the transport wakes.
    fn ring(&mut self, slot: EntrySlot, set: bool, ipc: &mut impl DsIpc) {
        let mut out = [Endpoint(0); NR_DS_SUBS];
        let stats = apply_update(&self.store, &mut self.subs, slot, set, &mut out);
        for endpoint in out.iter().take(stats.notified) {
            // C 222: `ipc_notify` answers to nobody; a failed wake drops.
            let _ = ipc.notify(*endpoint);
        }
    }

    /// Render the getsysinfo image (`ds_store` as C would copy it).
    ///
    /// [ARCH A-10/11]: entries serialize field-by-field into the exact
    /// C layout (192 bytes: flags, key, owner, 4 pad, union). Padding
    /// and the union's inactive bytes render as canonical zeros where C
    /// leaks stale memory — IS `dmp_ds` reads only live fields, so the
    /// visible contract holds; the byte image is simply honest about
    /// what is defined.
    fn render_image(&mut self) {
        for (index, seat) in self.store.iter().enumerate() {
            let at = index * 192;
            let out = &mut self.image[at..at + 192];
            match seat {
                Some(entry) => {
                    out[..4].copy_from_slice(&entry.flags.bits().to_ne_bytes());
                    out[4..84].copy_from_slice(&entry.key);
                    out[84..164].copy_from_slice(&entry.owner);
                    // 4 pad bytes at 164; union at 168: the narrow arm
                    // lives in the low 4 bytes, the wide arm fills 24.
                    let body = &mut out[168..192];
                    if entry
                        .flags
                        .intersects(DsFlags::TYPE_STR | DsFlags::TYPE_MEM)
                    {
                        // SAFETY: wide-arm entries are written through
                        // the wide arm only (publish path, 07).
                        let mem = unsafe { entry.body.mem };
                        body[..8].copy_from_slice(&(mem.data as usize).to_ne_bytes());
                        body[8..16].copy_from_slice(&mem.length.to_ne_bytes());
                        body[16..24].copy_from_slice(&mem.reallen.to_ne_bytes());
                    } else {
                        // SAFETY: narrow arm (U32/LABEL), the documented
                        // arm for these flags (03 D2).
                        let value = unsafe { entry.body.u32 };
                        body[..4].copy_from_slice(&value.to_ne_bytes());
                    }
                }
                None => {
                    // A vacant seat: flags clear, everything else zero —
                    // C leaks stale bytes here (flags-only clear, :645);
                    // the canonical zeros are the stated superset.
                    for b in out.iter_mut() {
                        *b = 0;
                    }
                }
            }
        }
    }

    /// The fresh anchor as the server sees it (`sef_cb_init_fresh`,
    /// store.c:254-282): reset, shadow every boot service, ring each
    /// shadow (map_service's `update_subscribers(dsp, 1)` at :246 —
    /// batched after the loop, 06 D4: the subscription table is empty
    /// at fresh boot, so the ring is quiet; the semantics stay unified).
    ///
    /// The rproctab grant fetch itself (store.c:267-269) rides the RS
    /// wire — the raw `rprocpub` byte ABI is pinned at edge E-RSWIRE;
    /// this seam takes the decoded `BootService` list. A failed shadow
    /// is fatal at the SEF owner, matching C's panic (store.c:275-277).
    pub fn init_fresh(
        &mut self,
        ipc: &mut impl DsIpc,
        services: &[BootService],
    ) -> Result<usize, i32> {
        let mapped = apply_boot_map(&mut self.store, &mut self.subs, services)?;
        for index in 0..NR_DS_KEYS {
            if let Some(slot) = EntrySlot::from_index(index) {
                if self.store[index].as_ref().is_some_and(|e| !e.is_vacant()) {
                    self.ring(slot, true, ipc);
                }
            }
        }
        Ok(mapped)
    }
}

/// Is the seated entry live and wide-armed (a pool descriptor holder)?
fn was_live_wide(store: &DsStore, slot: EntrySlot) -> bool {
    store[slot.index()].as_ref().is_some_and(|e| {
        e.flags.contains(DsFlags::IN_USE)
            && e.flags.intersects(DsFlags::TYPE_STR | DsFlags::TYPE_MEM)
    })
}

/// The real message end: minix-sys's trap transport (final shape; the
/// traps themselves power up with edge E1 — until then the transport
/// reports `EIO`, which the loop's failure bound turns into an honest
/// death instead of a silent spin).
pub struct SysIpc<T: SysIpcTransport> {
    inner: T,
}

impl<T: SysIpcTransport> SysIpc<T> {
    /// Wrap a minix-sys transport (the binary passes
    /// `DirectTrapTransport`).
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T: SysIpcTransport> DsIpc for SysIpc<T> {
    fn receive(&mut self) -> Result<Message, i32> {
        let mut message = Message::default();
        self.inner
            .receive(Endpoint::ANY, &mut message)
            .map(|_| message)
            .map_err(|t| t.0)
    }

    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        self.inner.send(to, message).map_err(|t| t.0)
    }

    fn notify(&self, who: Endpoint) -> Result<(), i32> {
        self.inner.notify(who).map_err(|t| t.0)
    }
}

/// The real kernel-call end (C: `sys_safecopyfrom`/`sys_safecopyto`/
/// `sys_datacopy`). minix-sys has no SYS_SAFECOPY*/SYS_DATACOPY wrapper
/// yet (edge E2/E-DSWIRE) — the stub reports `EIO`, which the arms'
/// error roads handle like any failed copy; when the wrappers land,
/// each body packs the kernel call exactly as production will send it
/// (the SCHED transport precedent).
#[derive(Debug, Clone, Copy, Default)]
pub struct SysKernel;

impl DsKernel for SysKernel {
    fn safecopy_from(&mut self, _: Endpoint, _: GrantId, _: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }
    fn safecopy_to(&mut self, _: Endpoint, _: GrantId, _: &[u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }
    fn data_copy_to(&mut self, _: Endpoint, _: &[u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }
}

/// Read the DS request arm (C: handlers cast `m_ptr` per call number —
/// `m_type` is the discriminator; DS_PUBLISH..DS_CHECK all speak
/// `m_ds_req`, main.c:54-72).
fn ds_req(message: &mut Message) -> &mut MessDsReq {
    // SAFETY: the union arm is selected by `m_type` (triage passed a
    // known DS letter); writing the reply lanes back into the same arm
    // is exactly C's overlay (do_check reuses the request lanes,
    // do_retrieve overlays `m_ds_reply`).
    unsafe { &mut message.m_u.m_ds_req }
}

/// Read-only view of the request arm.
fn ds_req_ref(message: &Message) -> &MessDsReq {
    // SAFETY: same arm discipline as `ds_req`, read side.
    unsafe { &message.m_u.m_ds_req }
}

/// Length up to the first NUL of an 80-byte lane (the `key_eq` stop
/// rule, 04).
fn strlen80(lane: &[u8; DS_MAX_KEYLEN]) -> usize {
    lane.iter().position(|&b| b == 0).unwrap_or(DS_MAX_KEYLEN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{DataBody, DataEntry};
    use core::cell::RefCell;
    use minix_types::EIO;

    const G_KEY: GrantId = 11;
    const G_VAL: GrantId = 12;
    const G_ROOM: GrantId = 13;

    /// Scripted message seam: canned receives, reply/wake ledgers.
    struct MockIpc {
        inbox: RefCell<Vec<Message>>,
        sent: RefCell<Vec<(Endpoint, Message)>>,
        notified: RefCell<Vec<Endpoint>>,
    }

    impl MockIpc {
        fn new(messages: Vec<Message>) -> Self {
            Self {
                inbox: RefCell::new(messages),
                sent: RefCell::new(Vec::new()),
                notified: RefCell::new(Vec::new()),
            }
        }

        fn replies(&self) -> Vec<(Endpoint, i32)> {
            self.sent
                .borrow()
                .iter()
                .map(|(to, m)| (*to, m.m_type))
                .collect()
        }
    }

    impl DsIpc for MockIpc {
        fn receive(&mut self) -> Result<Message, i32> {
            // FIFO: the scripted order is the arrival order.
            if self.inbox.borrow().is_empty() {
                return Err(EIO);
            }
            Ok(self.inbox.borrow_mut().remove(0))
        }
        fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
            self.sent.borrow_mut().push((to, message.clone()));
            Ok(())
        }
        fn notify(&self, who: Endpoint) -> Result<(), i32> {
            self.notified.borrow_mut().push(who);
            Ok(())
        }
    }

    /// Scripted kernel seam: readable grants, captured copy-backs.
    struct MockKernel {
        grants: RefCell<Vec<(Endpoint, GrantId, Vec<u8>)>>,
        written: RefCell<Vec<(Endpoint, GrantId, Vec<u8>)>>,
        copied: RefCell<Vec<(Endpoint, Vec<u8>)>>,
    }

    impl MockKernel {
        fn new() -> Self {
            Self {
                grants: RefCell::new(Vec::new()),
                written: RefCell::new(Vec::new()),
                copied: RefCell::new(Vec::new()),
            }
        }

        fn lend(&self, ep: Endpoint, grant: GrantId, bytes: &[u8]) {
            self.grants.borrow_mut().push((ep, grant, bytes.to_vec()));
        }

        fn last_write(&self) -> Vec<u8> {
            self.written.borrow().last().unwrap().2.clone()
        }
    }

    impl DsKernel for MockKernel {
        fn safecopy_from(
            &mut self,
            caller: Endpoint,
            grant: GrantId,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            for (ep, g, data) in self.grants.borrow().iter() {
                if *ep == caller && *g == grant {
                    let n = buf.len().min(data.len());
                    buf[..n].copy_from_slice(&data[..n]);
                    return Ok(());
                }
            }
            Err(EIO)
        }

        fn safecopy_to(&mut self, caller: Endpoint, grant: GrantId, buf: &[u8]) -> Result<(), i32> {
            self.written
                .borrow_mut()
                .push((caller, grant, buf.to_vec()));
            Ok(())
        }

        fn data_copy_to(&mut self, caller: Endpoint, buf: &[u8]) -> Result<(), i32> {
            self.copied.borrow_mut().push((caller, buf.to_vec()));
            Ok(())
        }
    }

    fn letter(caller: Endpoint, call: i32) -> Message {
        let mut m = Message::default();
        m.m_source = caller;
        m.m_type = call;
        m.m_u.m_ds_req = MessDsReq::default();
        m
    }

    fn with_req(m: &mut Message, f: impl FnOnce(&mut MessDsReq)) {
        f(ds_req(m));
    }

    fn seed_label(server: &mut DsServer, name: &[u8], ep: u32) {
        for seat in server.store.iter_mut() {
            if seat.is_none() {
                let mut e = DataEntry {
                    flags: DsFlags::IN_USE | DsFlags::TYPE_LABEL,
                    key: [0u8; DS_MAX_KEYLEN],
                    owner: [0u8; DS_MAX_KEYLEN],
                    body: DataBody { u32: ep },
                };
                e.key[..name.len()].copy_from_slice(name);
                e.owner[..2].copy_from_slice(b"rs");
                *seat = Some(e);
                return;
            }
        }
        panic!("no vacant seat for test label");
    }

    fn u32_letter(caller: Endpoint, call: i32, key: &[u8], value: u32, extra: DsFlags) -> Message {
        let mut m = letter(caller, call);
        with_req(&mut m, |r| {
            r.key_grant = G_KEY;
            r.key_len = key.len() as i32;
            r.flags = (DsFlags::TYPE_U32 | extra).bits() as i32;
            r.val_in = DsVal::number(value);
        });
        m
    }

    /// Run every scripted letter through the loop.
    fn drain(server: &mut DsServer, ipc: &mut MockIpc, kernel: &mut MockKernel) {
        while !ipc.inbox.borrow().is_empty() {
            let _ = server.run_once(ipc, kernel);
        }
    }

    fn wired() -> (DsServer, MockKernel) {
        let mut server = DsServer::new();
        seed_label(&mut server, b"rs", 2);
        seed_label(&mut server, b"vfs", 9);
        let mut kernel = MockKernel::new();
        kernel.lend(Endpoint(2), G_KEY, b"cfg");
        (server, kernel)
    }

    #[test]
    fn test_publish_check_roundtrip_through_transport() {
        // The registry's whole story in three letters: vfs subscribes to
        // "cfg", rs publishes cfg=7, vfs checks and reads the key back —
        // the wake rides the transport (E5(f)'s middle chain, at the
        // seam level).
        let (mut server, mut kernel) = wired();
        let mut subscribe = letter(Endpoint(9), minix_types::DS_SUBSCRIBE);
        with_req(&mut subscribe, |r| {
            r.key_grant = G_KEY;
            r.key_len = 3;
            r.flags = DsFlags::TYPE_U32.bits() as i32;
        });
        kernel.lend(Endpoint(9), G_KEY, b"cfg");
        let publish = u32_letter(
            Endpoint(2),
            minix_types::DS_PUBLISH,
            b"cfg",
            7,
            DsFlags::empty(),
        );
        let mut check = letter(Endpoint(9), minix_types::DS_CHECK);
        with_req(&mut check, |r| {
            r.key_grant = G_ROOM;
        });
        let mut ipc = MockIpc::new(vec![subscribe, publish, check]);

        drain(&mut server, &mut ipc, &mut kernel);

        // The publish woke the subscriber (store.c:222 through the seam).
        assert_eq!(ipc.notified.borrow().clone(), vec![Endpoint(9)]);
        // The check copied the key with its terminator and answered OK,
        // reply lanes naming the type and the publisher.
        let last = ipc.sent.borrow().last().unwrap().clone();
        assert_eq!((last.0, last.1.m_type), (Endpoint(9), OK));
        assert_eq!(kernel.last_write(), b"cfg\0");
        // SAFETY: the check reply rides the request lanes (store.c:570-572
        // through the arm) — the union's ds_req arm is the live one.
        let (reply_flags, reply_owner) =
            unsafe { (last.1.m_u.m_ds_req.flags, last.1.m_u.m_ds_req.owner) };
        assert_eq!(reply_flags, DsFlags::TYPE_U32.bits() as i32);
        assert_eq!(reply_owner, 2);
    }

    #[test]
    fn test_retrieve_moves_value_through_reply_lanes() {
        let (mut server, mut kernel) = wired();
        let publish = u32_letter(
            Endpoint(2),
            minix_types::DS_PUBLISH,
            b"cfg",
            7,
            DsFlags::empty(),
        );
        let retrieve = u32_letter(
            Endpoint(9),
            minix_types::DS_RETRIEVE,
            b"cfg",
            0,
            DsFlags::empty(),
        );
        kernel.lend(Endpoint(9), G_KEY, b"cfg"); // vfs ferries its own key copy
        let mut ipc = MockIpc::new(vec![publish, retrieve]);
        drain(&mut server, &mut ipc, &mut kernel);
        let (to, reply) = ipc.sent.borrow().last().unwrap().clone();
        assert_eq!((to, reply.m_type), (Endpoint(9), OK));
        assert_eq!(unsafe { reply.m_u.m_ds_reply.val_out.as_number() }, 7);
    }

    #[test]
    fn test_delete_wakes_through_transport() {
        // Deletion notifies (P1-3's contract) — the wake surfaces in the
        // transport ledger, and the entry's seat clears.
        let (mut server, mut kernel) = wired();
        let publish = u32_letter(
            Endpoint(9),
            minix_types::DS_PUBLISH,
            b"cfg",
            7,
            DsFlags::empty(),
        );
        // vfs subscribes to "cfg" before deleting it: C's do_delete rings
        // update_subscribers(…, 0) for matching subscribers (store.c:642)
        // — the deleter itself is the one woken here.
        let mut subscribe = letter(Endpoint(9), minix_types::DS_SUBSCRIBE);
        with_req(&mut subscribe, |r| {
            r.key_grant = G_KEY;
            r.key_len = 3;
            r.flags = DsFlags::TYPE_U32.bits() as i32;
        });
        let mut delete = letter(Endpoint(9), minix_types::DS_DELETE);
        with_req(&mut delete, |r| {
            r.key_grant = G_KEY;
            r.key_len = 3;
            r.flags = DsFlags::TYPE_U32.bits() as i32;
        });
        kernel.lend(Endpoint(9), G_KEY, b"cfg"); // vfs ferries its own key copy
        let mut ipc = MockIpc::new(vec![publish, subscribe, delete]);
        drain(&mut server, &mut ipc, &mut kernel);
        assert_eq!(ipc.notified.borrow().clone(), vec![Endpoint(9)]);
        assert!(server.store[2].is_none());
    }

    #[test]
    fn test_getsysinfo_copies_whole_image() {
        let (mut server, mut kernel) = wired();
        let publish = u32_letter(
            Endpoint(2),
            minix_types::DS_PUBLISH,
            b"cfg",
            7,
            DsFlags::empty(),
        );
        let mut ask = Message::default();
        ask.m_source = Endpoint(9);
        ask.m_type = minix_types::DS_GETSYSINFO;
        let mut getsysinfo = minix_types::MessLsysGetsysinfo::default();
        getsysinfo.what = crate::getsysinfo::SI_DATA_STORE;
        getsysinfo.where_ = 0x4000;
        getsysinfo.size = image_bytes() as u64;
        ask.m_u.m_lsys_getsysinfo = getsysinfo;
        let mut wrong = ask.clone();
        wrong.m_u.m_lsys_getsysinfo.size = image_bytes() as u64 - 1;
        let mut ipc = MockIpc::new(vec![publish, wrong, ask]);
        drain(&mut server, &mut ipc, &mut kernel);

        // The short ask refuses (store.c:668); the exact ask copies the
        // whole image (C 672-675), first byte = live entry's flags.
        let replies = ipc.replies();
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[1].1, EINVAL);
        assert_eq!(replies[2].1, OK);
        let copies = kernel.copied.borrow();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].0, Endpoint(9));
        assert_eq!(copies[0].1.len(), image_bytes());
        // Slot 0 holds the seeded "rs" label (IN_USE|TYPE_LABEL = 0x101);
        // the published "cfg" (U32) took slot 2 — first-fit order.
        assert_eq!(
            u32::from_ne_bytes(copies[0].1[0..4].try_into().unwrap()),
            (DsFlags::IN_USE | DsFlags::TYPE_LABEL).bits()
        );
        assert_eq!(
            u32::from_ne_bytes(copies[0].1[2 * 192..2 * 192 + 4].try_into().unwrap()),
            (DsFlags::IN_USE | DsFlags::TYPE_U32).bits()
        );
    }

    #[test]
    fn test_notify_and_wild_numbers_refuse_einval() {
        let (mut server, mut kernel) = wired();
        let notify = letter(Endpoint(4), 0x1000);
        let wild = letter(Endpoint(4), 0x4242);
        let mut ipc = MockIpc::new(vec![notify, wild]);
        drain(&mut server, &mut ipc, &mut kernel);
        assert_eq!(
            ipc.replies(),
            vec![(Endpoint(4), EINVAL), (Endpoint(4), EINVAL)]
        );
    }

    #[test]
    fn test_initial_scan_wakes_source_on_match() {
        // DSF_INITIAL: an already-live entry wakes the new subscriber
        // once (store.c:511-528 → :526).
        let (mut server, mut kernel) = wired();
        let mut publish = u32_letter(
            Endpoint(2),
            minix_types::DS_PUBLISH,
            b"clk",
            1,
            DsFlags::empty(),
        );
        // A dedicated grant: wired()'s (2, G_KEY) already carries "cfg",
        // and the mock's first-match rule would win.
        with_req(&mut publish, |r| r.key_grant = G_VAL);
        kernel.lend(Endpoint(2), G_VAL, b"clk"); // rs publishes "clk" (not wired's "cfg")
        let mut subscribe = letter(Endpoint(9), minix_types::DS_SUBSCRIBE);
        with_req(&mut subscribe, |r| {
            r.key_grant = G_KEY;
            r.key_len = 3;
            r.flags = (DsFlags::TYPE_U32 | DsFlags::INITIAL).bits() as i32;
        });
        kernel.lend(Endpoint(9), G_KEY, b"clk");
        let mut ipc = MockIpc::new(vec![publish, subscribe]);
        drain(&mut server, &mut ipc, &mut kernel);
        assert_eq!(ipc.notified.borrow().clone(), vec![Endpoint(9)]);
    }

    #[test]
    fn test_run_dies_at_the_failure_bound() {
        let mut server = DsServer::new();
        let mut ipc = MockIpc::new(Vec::new()); // empty inbox = broken wire
        let mut kernel = MockKernel::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            server.run(&mut ipc, &mut kernel)
        }));
        assert!(
            result.is_err(),
            "the bound must turn a dead wire into a death"
        );
    }
}
