//! 13/14 号控制与查询请求的 handler shell（A-1 拆分——§20.3 触发器：E-RSSTART
//! 落地）。`impl RsServer` 的本块方法按 request.c 的臂序组织；取数与序列化
//! 助手随行。主循环（run/get_work）、SEF 回调与信号面仍在 lib.rs
//! （06/15/18 域），12/16 号壳在 shell_update.rs。

use super::*;

/// VM's default mmapped-region preallocation on a non-identity update.
/// C: `RS_VM_DEFAULT_MAP_PREALLOC_LEN` — const.h:83 (8 MiB).
const RS_VM_DEFAULT_MAP_PREALLOC_LEN: i64 = 1024 * 1024 * 8;

/// Serializes one [`crate::service_slot::ServiceSlot`] row into C
/// `struct rprocpub` wire bytes (rs.h:165-183; offsets pinned by
/// `minix_types::rprocpub_off`). Field mappings:
/// - `old_endpoint`/`new_endpoint`: `None` → `Endpoint::NONE` — the
///   crate's established "unset" sentinel (the `InitMessage` encode,
///   Fix #73, maps the same way).
/// - `vm_call_mask`: the `CallMask(u64)` splits back into C's
///   `bitchunk_t[2]` little-endian chunks — bit *i* of the u64 is call *i*
///   exactly as C's chunk layout defines.
/// - `devman_id`: `None` → 0 (C's memset-zero vacancy, rs.h:182).
pub(crate) fn serialize_rprocpub_row(slot: &crate::service_slot::ServiceSlot, out: &mut [u8]) {
    use minix_types::rprocpub_off as off;
    fn put16(out: &mut [u8], o: usize, v: u16) {
        out[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(out: &mut [u8], o: usize, v: u32) {
        out[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    let pub_ = &slot.pub_;
    out[off::IN_USE..off::IN_USE + 2].copy_from_slice(&(pub_.in_use as i16).to_le_bytes());
    put32(out, off::SYS_FLAGS, pub_.sys_flags.bits() as u32);
    put32(out, off::ENDPOINT, pub_.endpoint.get() as u32);
    put32(
        out,
        off::OLD_ENDPOINT,
        pub_.old_endpoint.unwrap_or(Endpoint::NONE).get() as u32,
    );
    put32(
        out,
        off::NEW_ENDPOINT,
        pub_.new_endpoint.unwrap_or(Endpoint::NONE).get() as u32,
    );
    put32(out, off::DEV_NR, pub_.dev_nr);
    put32(out, off::NR_DOMAIN, pub_.nr_domain as i32 as u32);
    for (i, d) in pub_.domain.iter().enumerate() {
        put32(out, off::DOMAIN + i * 4, *d as u32);
    }
    out[off::LABEL..off::LABEL + 16].copy_from_slice(pub_.label.as_bytes());
    out[off::PROC_NAME..off::PROC_NAME + 16].copy_from_slice(pub_.proc_name.as_bytes());
    put32(
        out,
        off::VM_CALL_MASK,
        (pub_.vm_call_mask.0 & 0xffff_ffff) as u32,
    );
    put32(
        out,
        off::VM_CALL_MASK + 4,
        (pub_.vm_call_mask.0 >> 32) as u32,
    );
    let pci = off::PCI_ACL;
    out[pci + off::PCI_LABEL..pci + off::PCI_LABEL + 16]
        .copy_from_slice(pub_.pci_acl.label.as_bytes());
    put32(out, pci + off::PCI_ENDPOINT, pub_.pci_acl.endpoint as u32);
    put32(out, pci + off::PCI_NR_DEVICE, pub_.pci_acl.nr_device as u32);
    for (i, d) in pub_.pci_acl.device.iter().enumerate() {
        let o = pci + off::PCI_DEVICE + i * 8;
        put16(out, o, d.vid);
        put16(out, o + 2, d.did);
        put16(out, o + 4, d.sub_vid);
        put16(out, o + 6, d.sub_did);
    }
    put32(out, pci + off::PCI_NR_CLASS, pub_.pci_acl.nr_class as u32);
    for (i, c) in pub_.pci_acl.class.iter().enumerate() {
        put32(out, pci + off::PCI_CLASS + i * 8, c.pciclass);
        put32(out, pci + off::PCI_CLASS + i * 8 + 4, c.mask);
    }
    put32(out, off::DEVMAN_ID, pub_.devman_id.unwrap_or(0) as u32);
}

/// The `SI_PROCPUB_TAB` copy-out (request.c:1119-1121 + :1134-1136):
/// serialize every row of the public table (vacant rows included — C
/// copies the raw array), gate on the caller-declared size, and hand the
/// bytes to the requester through the safecopy seam. Free function so the
/// direct-drive tests can retain the mock and assert the served bytes.
pub(crate) fn copy_out_procpub_table(
    kernel: &mut dyn crate::boot::KernelApi,
    table: &crate::process_table::RProcTable,
    dest: Endpoint,
    addr: usize,
    size: u64,
) -> Result<(), Errno> {
    let row_len = minix_types::rprocpub_off::SIZE;
    let rows = table.len();
    let mut img = alloc::vec![0u8; rows * row_len];
    for (i, (_, slot)) in table.iter_all().enumerate() {
        serialize_rprocpub_row(slot, &mut img[i * row_len..(i + 1) * row_len]);
    }
    // C: request.c:1134-1136 — `len != size` → EINVAL.
    if img.len() as u64 != size {
        return Err(Errno::EINVAL);
    }
    kernel.safecopy_to(dest, addr, &img)
}

/// Serializes one [`crate::service_slot::ServiceSlot`] row into C
/// `struct rproc` wire bytes (type.h:56-108; offsets derived from the
/// repr(C) witness in `minix_types::rproc_off`). Field mapping:
/// - the four chain fields and the `r_upd`/`r_argv`/`r_exec` pointer fields
///   serialize as 0 — they are RS-address-space pointers in C (opaque to
///   any receiver) and the A-3 index model holds no raw addresses;
/// - `struct priv` is written from the authority copy (`slot.priv_`,
///   Fix #62); kernel-runtime fields RS never models (async table, pending
///   maps, timers, grant/state tables) serialize as zero — their C in-RS
///   copies are equally zero or stale;
/// - `r_io_tab`/`s_io_tab` write C's `ior_base`/`ior_limit` =
///   `base + len - 1` (type.h:135-136, edit_slot manager.c:1516-1518);
/// - `r_upd` writes the live descriptor (`slot.upd`) minus its pointers.
///
/// Unmodeled-field zeros are design-documented, not accidents: everything a
/// dump-face consumer (08-stage-is) reads — labels, names, flags, endpoints,
/// period/heartbeat timestamps, restart counters — is byte-exact.
pub(crate) fn serialize_rproc_row(slot: &crate::service_slot::ServiceSlot, out: &mut [u8]) {
    use minix_types::rproc_off as off;
    use minix_types::rproc_off::{priv_off, upd};

    fn put32(out: &mut [u8], o: usize, v: u32) {
        out[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put64(out: &mut [u8], o: usize, v: u64) {
        out[o..o + 8].copy_from_slice(&v.to_le_bytes());
    }
    fn put_i32(out: &mut [u8], o: usize, v: i32) {
        put32(out, o, v as u32);
    }
    fn put_i64(out: &mut [u8], o: usize, v: i64) {
        put64(out, o, v as u64);
    }

    let priv_ = &slot.priv_;

    // r_upd descriptor (type.h:31-45) — pointers stay 0.
    if let Some(u) = &slot.upd {
        let b = off::R_UPD;
        put_i32(out, b + upd::LU_FLAGS, u.lu_flags.bits() as i32);
        put32(out, b + upd::INIT_FLAGS, u.init_flags);
        put_i32(out, b + upd::PREPARE_STATE, u.prepare_state);
        put_i32(out, b + upd::STATE_ENDPOINT, u.state_endpoint.get());
        put_i64(out, b + upd::PREPARE_TM, u.prepare_tm);
        put_i64(out, b + upd::PREPARE_MAXTIME, u.prepare_maxtime);
        let sd = b + upd::PREPARE_STATE_DATA;
        put64(out, sd, u.prepare_state_data.size as u64);
        put64(out, sd + 8, u.prepare_state_data.ipcf_els_addr);
        put64(out, sd + 16, u.prepare_state_data.ipcf_els_size as u64);
        put_i32(
            out,
            sd + 24,
            u.prepare_state_data
                .ipcf_els_gid
                .map(|g| g as i32)
                .unwrap_or(-1),
        );
        put64(out, sd + 32, u.prepare_state_data.eval_addr);
        put64(out, sd + 40, u.prepare_state_data.eval_len as u64);
        put_i32(
            out,
            sd + 48,
            u.prepare_state_data
                .eval_gid
                .map(|g| g as i32)
                .unwrap_or(-1),
        );
    }

    put_i32(out, off::R_PID, slot.pid.unwrap_or(-1));
    put_i32(out, off::R_ASR_COUNT, slot.asr_count);
    put_i32(out, off::R_RESTARTS, slot.restarts);
    put_i64(out, off::R_BACKOFF, slot.backoff);
    put32(out, off::R_FLAGS, slot.flags.bits() as u32);
    put_i32(out, off::R_INIT_ERR, slot.init_err);
    put_i64(out, off::R_PERIOD, slot.period);
    put_i64(out, off::R_CHECK_TM, slot.check_tm);
    put_i64(out, off::R_ALIVE_TM, slot.alive_tm);
    put_i64(out, off::R_STOP_TM, slot.stop_tm);
    put_i32(out, off::R_CALLER, slot.caller.get());
    put_i32(out, off::R_CALLER_REQUEST, slot.caller_request);
    out[off::R_CMD..off::R_CMD + 512].copy_from_slice(&slot.cmd);
    out[off::R_ARGS..off::R_ARGS + 512].copy_from_slice(&slot.args);
    // r_argv: 12 opaque pointers — zero (unmodeled).
    put_i32(out, off::R_ARGC, slot.argc);
    out[off::R_SCRIPT..off::R_SCRIPT + 256].copy_from_slice(&slot.script);
    // r_exec: 0 (opaque); r_exec_len carries the image length.
    put64(
        out,
        off::R_EXEC_LEN,
        slot.exec.as_ref().map(|b| b.len()).unwrap_or(0) as u64,
    );

    // struct priv — authority copy.
    let p = off::R_PRIV;
    put_i32(out, p + priv_off::S_PROC_NR, slot.pub_.endpoint.slot());
    put_i32(out, p + priv_off::S_ID, priv_.id.0);
    let sf = priv_.flags.bits() as i16;
    out[p + priv_off::S_FLAGS..p + priv_off::S_FLAGS + 2].copy_from_slice(&sf.to_le_bytes());
    put32(out, p + priv_off::S_INIT_FLAGS, priv_.init_flags);
    let tm = priv_.trap_mask.bits() as i16;
    out[p + priv_off::S_TRAP_MASK..p + priv_off::S_TRAP_MASK + 2]
        .copy_from_slice(&tm.to_le_bytes());
    put32(
        out,
        p + priv_off::S_IPC_TO,
        (priv_.ipc_to.0 & 0xffff_ffff) as u32,
    );
    put32(
        out,
        p + priv_off::S_IPC_TO + 4,
        (priv_.ipc_to.0 >> 32) as u32,
    );
    put32(
        out,
        p + priv_off::S_K_CALL_MASK,
        (priv_.k_call_mask.0 & 0xffff_ffff) as u32,
    );
    put32(
        out,
        p + priv_off::S_K_CALL_MASK + 4,
        (priv_.k_call_mask.0 >> 32) as u32,
    );
    put_i32(out, p + priv_off::S_SIG_MGR, priv_.sig_mgr.get());
    put_i32(out, p + priv_off::S_BAK_SIG_MGR, priv_.bak_sig_mgr.get());
    put_i32(out, p + priv_off::S_NR_IO_RANGE, priv_.nr_io_range);
    for (i, r) in priv_.io_ranges.iter().enumerate() {
        let o = p + priv_off::S_IO_TAB + i * 8;
        // type.h:135-136 — limit is INCLUSIVE: base + len - 1.
        let limit = if r.len == 0 { 0 } else { r.base + r.len - 1 };
        put32(out, o, r.base);
        put32(out, o + 4, limit);
    }
    put_i32(out, p + priv_off::S_NR_MEM_RANGE, priv_.nr_mem_range);
    for (i, m) in priv_.mem_ranges.iter().enumerate() {
        let o = p + priv_off::S_MEM_TAB + i * 16;
        put64(out, o, m.base);
        put64(out, o + 8, m.len);
    }
    put_i32(out, p + priv_off::S_NR_IRQ, priv_.nr_irq);
    for (i, q) in priv_.irqs.iter().enumerate() {
        put_i32(out, p + priv_off::S_IRQ_TAB + i * 4, *q);
    }

    put32(out, off::R_UID, slot.uid);
    put_i32(out, off::R_SCHEDULER, slot.scheduler.get());
    put_i32(out, off::R_PRIORITY, slot.priority);
    put_i32(out, off::R_QUANTUM, slot.quantum);
    put_i32(out, off::R_CPU, slot.cpu);
    put64(out, off::R_MAP_PREALLOC_ADDR, slot.map_prealloc_addr);
    put64(out, off::R_MAP_PREALLOC_LEN, slot.map_prealloc_len as u64);
    for (i, q) in slot.irq_tab.iter().enumerate() {
        put_i32(out, off::R_IRQ_TAB + i * 4, *q);
    }
    put_i32(out, off::R_NR_IRQ, slot.nr_irq);
    out[off::R_IPC_LIST..off::R_IPC_LIST + 256].copy_from_slice(&slot.ipc_list);
    put_i32(out, off::R_NR_CONTROL, slot.nr_control);
    for (i, l) in slot.control.iter().enumerate() {
        let o = off::R_CONTROL + i * 16;
        out[o..o + 16].copy_from_slice(l.as_bytes());
    }
}

/// The `SI_PROC_TAB` copy-out (request.c:1113-1115): the whole internal
/// table (`sizeof(struct rproc) * NR_SYS_PROCS` raw bytes) through the
/// exact-size gate. Free function for direct-drive tests.
pub(crate) fn copy_out_procall_table(
    kernel: &mut dyn crate::boot::KernelApi,
    table: &crate::process_table::RProcTable,
    dest: Endpoint,
    addr: usize,
    size: u64,
) -> Result<(), Errno> {
    let proc_len = table.len() * minix_types::rproc_off::SIZE;
    let pub_len = table.len() * minix_types::rprocpub_off::SIZE;
    // C: request.c:1116-1118 — the rproc half alone must fit.
    if proc_len as u64 > size {
        return Err(Errno::EINVAL);
    }
    // C: request.c:1134-1136 — rproc + rprocpub must fill the request.
    if (proc_len + pub_len) as u64 != size {
        return Err(Errno::EINVAL);
    }
    let mut img = alloc::vec![0u8; proc_len + pub_len];
    let row_rproc = minix_types::rproc_off::SIZE;
    let row_pub = minix_types::rprocpub_off::SIZE;
    for (i, (_, slot)) in table.iter_all().enumerate() {
        serialize_rproc_row(slot, &mut img[i * row_rproc..(i + 1) * row_rproc]);
        serialize_rprocpub_row(
            slot,
            &mut img[proc_len + i * row_pub..proc_len + (i + 1) * row_pub],
        );
    }
    kernel.safecopy_to(dest, addr, &img)
}

pub(crate) fn copy_out_rproc_table(
    kernel: &mut dyn crate::boot::KernelApi,
    table: &crate::process_table::RProcTable,
    dest: Endpoint,
    addr: usize,
    size: u64,
) -> Result<(), Errno> {
    let row_len = minix_types::rproc_off::SIZE;
    let rows = table.len();
    let mut img = alloc::vec![0u8; rows * row_len];
    for (i, (_, slot)) in table.iter_all().enumerate() {
        serialize_rproc_row(slot, &mut img[i * row_len..(i + 1) * row_len]);
    }
    if img.len() as u64 != size {
        return Err(Errno::EINVAL);
    }
    kernel.safecopy_to(dest, addr, &img)
}

/// Assembles the post-copy [`crate::slot::RsStart`] from the decoded wire
/// view: fetches the caller-space buffers through the safecopy seam — the
/// second half of C's two-phase design (rs.h:63 "Labels are copied over
/// separately"; in C the byte copies live *inside* `edit_slot`'s branches,
/// manager.c:1475-1483/:1578-1626, and the Rust pure-`edit_slot` split them
/// out to the request handler — Fix #48's "RsStart 本就是拷贝后内存结构").
///
/// Copy shapes follow the C per-branch semantics exactly:
/// - cmd / IPC list / script: raw claimed lengths ride in `RsStart`
///   (`cmdlen`/`ipclen`/`scriptlen`), so `edit_slot`'s gates fire on the
///   claim (E2BIG manager.c:1578/:1618, EINVAL manager.c:1475-1479) — the
///   read is clamped to the buffer only so a hostile length cannot overrun
///   it (C never reads past the gate; the extra read of caller memory is
///   unobservable, see the design note below).
/// - progname: the claimed `rss_prognamelen` (manager.c:1593) rides in
///   `RsStart.progname_len` for the same reason; `Label` carries the bytes.
/// - labels (service/target/control): `copy_label`'s clamp — C
///   manager.c:151-169 copies `min(dst_len-1, src_len)` bytes and
///   NUL-terminates, no E2BIG.
/// - script/label reads C skips (`script_addr == NULL`, `l_len == 0`) are
///   skipped here too; `trg_label` is *not* fetched — C consumes it in
///   `do_update`'s own body (request.c:627-629), not in `edit_slot`, and
///   that arm fetches it itself.
///
/// Design note (fetch/check split): C interleaves each copy with its gate;
/// the Rust split runs all copies first and all gates in `edit_slot`. For
/// a request that is invalid in exactly one field the observable errno is
/// identical; for a compound-invalid request whose unreadable buffer would
/// fail the copy, the errno can differ (C's gate errno vs. the copy's
/// EFAULT) — both reject with no slot damage, and the raw-length gates in
/// `edit_slot` fire on the claim before content matters.
pub(crate) fn fetch_rs_start(
    kernel: &mut dyn crate::boot::KernelApi,
    src: Endpoint,
    wire: &minix_types::RsStartWire,
) -> Result<crate::slot::RsStart, Errno> {
    use crate::service_slot::{
        Label, MAX_COMMAND_LEN, MAX_IPC_LIST, MAX_SCRIPT_LEN, RS_MAX_LABEL_LEN,
    };
    use crate::service_slot::{RS_NR_PCI_CLASS, RS_NR_PCI_DEVICE, RsPciClass, RsPciId};
    use crate::slot::{RSS_NR_IO, RsStart};

    let mut fetch = |buf: &mut [u8], addr: u64, len: usize| -> Result<usize, Errno> {
        let n = len.min(buf.len());
        if n > 0 {
            kernel.safecopy_from(src, addr as usize, &mut buf[..n])?;
        }
        Ok(n)
    };
    let label_of = |kernel: &mut dyn crate::boot::KernelApi,
                    l: minix_types::RsLabelWire|
     -> Result<Label, Errno> {
        let mut buf = [0u8; RS_MAX_LABEL_LEN];
        if l.len > 0 {
            let n = (l.len as usize).min(RS_MAX_LABEL_LEN - 1);
            kernel.safecopy_from(src, l.addr as usize, &mut buf[..n])?;
            buf[n] = 0;
        }
        Ok(Label::from_bytes(&buf[..]))
    };

    let mut cmd = [0u8; MAX_COMMAND_LEN];
    fetch(&mut cmd, wire.cmd_addr, wire.cmd_len as usize)?;
    let mut ipc_list = [0u8; MAX_IPC_LIST];
    fetch(&mut ipc_list, wire.ipc_addr, wire.ipc_len as usize)?;
    let mut script = [0u8; MAX_SCRIPT_LEN];
    if wire.script_addr != 0 && wire.script_len > 0 {
        fetch(&mut script, wire.script_addr, wire.script_len as usize)?;
    }
    let progname_len = wire.progname_len as usize;
    let mut progname_buf = [0u8; RS_MAX_LABEL_LEN];
    if progname_len > 0 {
        let n = progname_len.min(RS_MAX_LABEL_LEN - 1);
        kernel.safecopy_from(src, wire.progname_addr as usize, &mut progname_buf[..n])?;
        progname_buf[n] = 0;
    }
    let progname = Label::from_bytes(&progname_buf[..]);

    let mut control = [Label::empty(); crate::service_slot::RS_NR_CONTROL];
    for (i, slot_label) in control.iter_mut().enumerate() {
        if i >= wire.nr_control.max(0) as usize {
            break;
        }
        *slot_label = label_of(kernel, wire.control[i])?;
    }

    Ok(RsStart {
        flags: crate::slot::RssFlags::from_bits_retain(wire.flags),
        uid: wire.uid,
        sigmgr: Endpoint(wire.sigmgr),
        scheduler: Endpoint(wire.scheduler),
        priority: wire.priority,
        quantum: wire.quantum,
        cpu: wire.cpu,
        period: wire.period,
        restarts: wire.restarts,
        asr_count: wire.asr_count,
        cmd,
        cmdlen: wire.cmd_len as usize,
        ipc_list,
        ipclen: wire.ipc_len as usize,
        progname,
        progname_len,
        nr_control: wire.nr_control,
        control,
        nr_irq: wire.nr_irq,
        irq: wire.irq,
        nr_io: wire.nr_io,
        io: {
            let mut io = [crate::privilege::IoRange::default(); RSS_NR_IO];
            for (i, r) in io.iter_mut().enumerate() {
                r.base = wire.io[i].base;
                r.len = wire.io[i].len;
            }
            io
        },
        major: wire.major,
        script,
        scriptlen: wire.script_len as usize,
        heap_prealloc_bytes: wire.heap_prealloc_bytes,
        map_prealloc_bytes: wire.map_prealloc_bytes,
        system: crate::privilege::CallMask(wire.system),
        vm: crate::privilege::CallMask(wire.vm),
        label: label_of(kernel, wire.label)?,
        trg_label: Label::empty(), // do_update's own copy (request.c:627-629)
        nr_pci_id: wire.nr_pci_id,
        pci_id: {
            let mut pci = [RsPciId::default(); RS_NR_PCI_DEVICE];
            for (i, p) in pci.iter_mut().enumerate() {
                p.vid = wire.pci_id[i].vid;
                p.did = wire.pci_id[i].did;
                p.sub_vid = wire.pci_id[i].sub_vid;
                p.sub_did = wire.pci_id[i].sub_did;
            }
            pci
        },
        nr_pci_class: wire.nr_pci_class,
        pci_class: {
            let mut pci = [RsPciClass::default(); RS_NR_PCI_CLASS];
            for (i, p) in pci.iter_mut().enumerate() {
                p.pciclass = wire.pci_class[i].pciclass;
                p.mask = wire.pci_class[i].mask;
            }
            pci
        },
        state_data: crate::slot::RsStateData {
            size: wire.state_data.size as usize,
            ipcf_els_addr: wire.state_data.ipcf_els_addr,
            ipcf_els_size: wire.state_data.ipcf_els_size as usize,
            ipcf_els_gid: (wire.state_data.ipcf_els_gid >= 0)
                .then_some(wire.state_data.ipcf_els_gid as u32),
            eval_addr: wire.state_data.eval_addr,
            eval_len: wire.state_data.eval_len as usize,
            eval_gid: (wire.state_data.eval_gid >= 0).then_some(wire.state_data.eval_gid as u32),
        },
        devman_id: wire.devman_id,
        nr_domain: wire.nr_domain,
        domain: wire.domain,
    })
}

impl RsServer {
    /// C: `do_update` — request.c:534-889: schedule a live update for a
    /// service. The `rs_start_t` round-trip (decode + fetch, Fix #81/#82)
    /// opens the arm; the target label comes from `rss_label`, the target
    /// state endpoint from `rss_trg_label` (request.c:625-640). The flag
    /// mapping (`lu_flags_from_rss`), the phase gates
    /// (`validate_update_request`), the VM-default preallocation
    /// (`vm_default_prealloc`), the descriptor (with the A-4 mirror
    /// responsibility) and the prepare walk (`start_update_prepare`) are
    /// the reviewed 16 slices this handler composes. The state-data segment
    /// (request.c:792-836: the `init_state_data` composition is 17's; the
    /// three `cpf_grant_direct` calls are the 19 grant face, E-11) fails
    /// closed: a request that actually carries state data is rejected
    /// ENOSYS instead of silently scheduling an update without its state
    /// transfer.
    pub(crate) fn do_update(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let Some((addr, _)) = m.rs_req_payload() else {
            return Err(Errno::EINVAL);
        };

        // Copy the request structure (request.c:542-546) and its buffers.
        let mut buf = [0u8; minix_types::rs_start_off::SIZE];
        self.kernel
            .safecopy_from(m.m_source, addr as usize, &mut buf)?;
        let wire = minix_types::decode_rs_start(&buf)?;
        let mut rs_start = fetch_rs_start(self.kernel.as_mut(), m.m_source, &wire)?;

        // Copy label + lookup (request.c:548-556).
        let Some(id) = state.table.lookup_by_label(&rs_start.label) else {
            return Err(Errno::ESRCH);
        };
        let endpoint = state.table.get(id).pub_.endpoint;

        // Check flags (request.c:568-623). The VM-default preallocation
        // decision (request.c:591-599) sits between the C flag writes but
        // only reads the SELF|ASR bits — which do not depend on the
        // preallocation value — so a first mapping pass feeds the default
        // decision and the second sees the defaulted value (its NOMMAP
        // test, request.c:601-605, must observe the default exactly as C's
        // in-place rewrite does).
        let prepare_only = rs_start
            .flags
            .contains(crate::slot::RssFlags::PREPARE_ONLY_LU);
        let force_init_st = rs_start
            .flags
            .contains(crate::slot::RssFlags::FORCE_INIT_ST);
        let (lu_probe, _) =
            crate::live_update::lu_flags_from_rss(rs_start.flags, rs_start.map_prealloc_bytes);
        let defaulted = crate::live_update::vm_default_prealloc(
            rs_start.map_prealloc_bytes,
            endpoint,
            lu_probe,
            force_init_st,
            RS_VM_DEFAULT_MAP_PREALLOC_LEN,
        );
        rs_start.map_prealloc_bytes = defaulted;
        let (lu_flags, init_flags) =
            crate::live_update::lu_flags_from_rss(rs_start.flags, rs_start.map_prealloc_bytes);
        let do_self_update = rs_start.flags.contains(crate::slot::RssFlags::SELF_LU);
        let noblock = rs_start.flags.contains(crate::slot::RssFlags::NOBLOCK);
        let batch_mode = rs_start.flags.contains(crate::slot::RssFlags::BATCH);

        // Lookup target label (request.c:625-640) — the state endpoint for
        // a stateful transfer; copy_label's clamp shapes the bytes.
        let mut state_endpoint = Endpoint::NONE;
        if wire.trg_label.len > 0 {
            let n = (wire.trg_label.len as usize).min(crate::service_slot::RS_MAX_LABEL_LEN - 1);
            let mut label_buf = [0u8; crate::service_slot::RS_MAX_LABEL_LEN];
            self.kernel.safecopy_from(
                m.m_source,
                wire.trg_label.addr as usize,
                &mut label_buf[..n],
            )?;
            label_buf[n] = 0;
            let trg_label = crate::service_slot::Label::from_bytes(&label_buf[..]);
            let Some(trg) = state.table.lookup_by_label(&trg_label) else {
                return Err(Errno::ESRCH);
            };
            state_endpoint = state.table.get(trg).pub_.endpoint;
        }

        // Permission (request.c:642-644).
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let caller_euid = self.kernel.getnuid(m.m_source);
        crate::access::check_call_permission(
            m.m_source,
            minix_types::RS_UPDATE,
            Some(state.table.get(id)),
            &state.table,
            updating,
            caller_euid,
        )?;

        // Prepare state / max time (request.c:646-657) and the phase gates
        // (request.c:659-686): updating → EBUSY, scheduled-without-batch →
        // EBUSY, already in the chain → EINVAL, prepare-only endpoint rules.
        // The default max time is 2*RS_DELTA_T (const.h:58, hz-scaled).
        let upd = minix_types::RsUpdate::decode_message(m);
        let prepare_state = upd.state;
        let prepare_maxtime = crate::live_update::resolve_prepare_maxtime(
            u32::try_from(upd.prepare_maxtime.max(0)).unwrap_or(0),
            (2 * crate::monitor::delta_t(state.system_hz)) as u32,
        );
        crate::live_update::validate_update_request(
            crate::live_update::update_phase(state.update.flags, state.update.chain.len()),
            batch_mode,
            state.table.get(id).upd.is_some(),
            prepare_only,
            endpoint,
            prepare_state,
        )?;

        // Initialize the update descriptor (request.c:689-695) — the A-4
        // mirror write rides on the add below.
        let mut entry = crate::live_update::UpdateEntry::new(id, endpoint);
        entry.lu_flags = lu_flags;
        entry.init_flags = init_flags;
        state.update.chain.set_new_upd_flags(&mut entry);

        // The new instance (request.c:697-760): a self update clones the
        // running service into a replica; a regular update allocates and
        // initializes a fresh slot that inherits the old instance's
        // immutable defaults, links to it, and is created without running.
        let ticks = self.kernel.get_ticks()?;
        let mut new_id: Option<crate::service_slot::SlotId> = None;
        if !prepare_only {
            if do_self_update {
                crate::service_create::clone_service(
                    &mut state.table,
                    id,
                    self.kernel.as_mut(),
                    crate::privilege::PrivFlags::LU_SYS_PROC,
                    entry.init_flags,
                    ticks,
                    &mut |_| Ok(()),
                )?;
                new_id = state.table.get(id).new_rp;
            } else {
                let nid = state.table.alloc_slot()?;
                // Row out/in (Fix #49's slot-first signature) — the row is a
                // fresh vacant one, so the donor scan sees exactly what C's
                // loop would.
                let mut slot = core::mem::replace(
                    state.table.get_mut(nid),
                    crate::service_slot::ServiceSlot::vacant(),
                );
                let init_r = crate::service_create::init_slot(
                    &mut slot,
                    &rs_start,
                    &state.table,
                    &mut |_| Ok(()),
                );
                // Inherit the old instance's immutable defaults while the
                // row is still out of the table (the def borrow and the
                // local row do not alias).
                if init_r.is_ok() {
                    crate::service_create::inherit_service_defaults(state.table.get(id), &mut slot);
                }
                *state.table.get_mut(nid) = slot;
                init_r?;
                // Link the two versions (request.c:732-734).
                state.table.get_mut(nid).old_rp = Some(id);
                state.table.get_mut(id).new_rp = Some(nid);
                // Create the new version but don't let it run
                // (request.c:736-745).
                {
                    let s = state.table.get_mut(nid);
                    s.priv_
                        .flags
                        .insert(crate::privilege::PrivFlags::LU_SYS_PROC);
                    s.priv_.init_flags |= entry.init_flags;
                }
                crate::service_create::create_service(
                    &mut state.table,
                    nid,
                    self.kernel.as_mut(),
                    ticks,
                    &mut |_| Ok(()),
                )?;
                new_id = Some(nid);
            }
        }

        // Default state endpoint (request.c:762-766).
        if state_endpoint == Endpoint::NONE
            && let Some(nid) = new_id
        {
            state_endpoint = state.table.get(nid).pub_.endpoint;
        }

        // RS's backup signal manager for rollback during initialization
        // (request.c:768-777) — the composed update (Fix #45.5) executes as
        // one UpdateSys privctl; failure cleans the new instance.
        if state
            .table
            .get(id)
            .priv_
            .flags
            .contains(crate::privilege::PrivFlags::ROOT_SYS_PROC)
            && let Some(nid) = new_id
        {
            // C: update_sig_mgrs(new_rp, SELF, new_rp->r_pub->endpoint) —
            // utility.c:387-422: sync the new instance's priv from the
            // kernel, set sig_mgr (SELF expanded to the new endpoint) and
            // the backup, then push with UpdateSys. Failure cleans the new
            // instance (request.c:771-776).
            let new_ep = state.table.get(nid).pub_.endpoint;
            let synced = self.kernel.getpriv(new_ep)?;
            let mut p = synced;
            let u = crate::self_lifecycle::self_update_sig_mgr_update(new_ep);
            p.sig_mgr = u.sig_mgr;
            p.bak_sig_mgr = u.bak_sig_mgr;
            let r = self
                .kernel
                .privctl(new_ep, crate::privilege::PrivCtlOp::UpdateSys, Some(&p));
            match r {
                Ok(()) => {
                    state.table.get_mut(nid).priv_ = p;
                }
                Err(e) => {
                    let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                    crate::recovery::cleanup_service(
                        &mut state.table,
                        nid,
                        self.kernel.as_mut(),
                        &mut noop_script,
                    );
                    return Err(e);
                }
            }
        }

        // Preallocate heap / mmapped regions if requested
        // (request.c:779-811). Negative means "not requested" and zeroes in
        // place (request.c:781-783/:789-791). The vm_memctl seam carries
        // (proc, req, a, b) with no out-params — the mapped address of
        // MAP_PREALLOC arrives with the 19 transport (E-11), so the
        // recorded address stays 0 here while the length is live.
        if !prepare_only && let Some(nid) = new_id {
            if rs_start.heap_prealloc_bytes < 0 {
                rs_start.heap_prealloc_bytes = 0;
            }
            if rs_start.heap_prealloc_bytes != 0 {
                self.kernel.vm_memctl(
                    state.table.get(nid).pub_.endpoint,
                    crate::boot::VmRsMemReq::HeapPrealloc,
                    0,
                    rs_start.heap_prealloc_bytes as usize,
                )?;
                if state
                    .table
                    .get(id)
                    .priv_
                    .flags
                    .contains(crate::privilege::PrivFlags::ROOT_SYS_PROC)
                {
                    let _ = self.kernel.vm_memctl(
                        state.table.get(nid).pub_.endpoint,
                        crate::boot::VmRsMemReq::Pin,
                        0,
                        0,
                    );
                }
            }
            if rs_start.map_prealloc_bytes < 0 {
                rs_start.map_prealloc_bytes = 0;
            }
            if rs_start.map_prealloc_bytes != 0 {
                self.kernel.vm_memctl(
                    state.table.get(nid).pub_.endpoint,
                    crate::boot::VmRsMemReq::MapPrealloc,
                    0,
                    rs_start.map_prealloc_bytes as usize,
                )?;
                state.table.get_mut(nid).map_prealloc_len = rs_start.map_prealloc_bytes as usize;
            }
        }

        // State data (request.c:788-836) — the `init_state_data`
        // composition (manager.c:172-285, 17 号): spec validation, the eval
        // bytes and the filter blocks through the fetch seam, labels
        // resolved through the DS seam (19: noop here). The three
        // cpf_grant_direct calls (request.c:798-835) are the 19 grant face
        // (E-11) — the gid fields stay `None` until it lands. Failure
        // cleans the new instance (C: rupdate_upd_clear,
        // request.c:788-796/:830-835).
        let mut fetch = |a: usize, buf: &mut [u8]| -> Result<(), Errno> {
            self.kernel.safecopy_from(m.m_source, a, buf)
        };
        let ds_lookup = |_label: &str| -> Option<Endpoint> { None };
        let state_out = crate::state_data::init_state_data(
            prepare_state,
            &rs_start.state_data,
            &mut fetch,
            &ds_lookup,
            m.m_source == Endpoint::VM,
        );
        match state_out {
            Ok(out) => {
                entry.prepare_state_data.size = out.size;
                entry.eval_buff = out.eval;
                entry.ipcf_els_buff = Some(out.ipcf_els_buff);
            }
            Err(e) => {
                if let Some(nid) = new_id {
                    let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                    crate::recovery::cleanup_service(
                        &mut state.table,
                        nid,
                        self.kernel.as_mut(),
                        &mut noop_script,
                    );
                }
                return Err(e);
            }
        }

        // Fill the descriptor and schedule it (request.c:838-845) — the
        // mirror write is `chain.add`'s documented caller responsibility.
        entry.prepare_state = prepare_state;
        entry.state_endpoint = state_endpoint;
        entry.prepare_tm = ticks;
        entry.prepare_maxtime = prepare_maxtime as i64;
        let mirror = entry.clone();
        state.update.chain.add(entry);
        state.table.get_mut(id).upd = Some(mirror);

        // Batch mode replies immediately (request.c:847-850).
        if batch_mode {
            return Ok(0);
        }

        // Start preparing (request.c:852-861) — allow_retries = 0. The
        // prepare walk's abort/end callbacks cannot capture the update state
        // and table their caller already holds (the A-2 aliasing wall), so
        // the two failure exits resolve post-return with exactly the calls C
        // makes inside: EAGAIN → abort_update_proc(EAGAIN)
        // (update.c:408-417), ESRCH → end_update(OK, RS_REPLY)
        // (request.c:853-858 — nothing left to prepare).
        let is_idle = state.table.iter_in_use().all(|(_, s)| s.flags.is_idle());
        let mut update = core::mem::take(&mut state.update);
        let mut noop_abort = |_: i32| {};
        let mut noop_end = |_: i32| {};
        let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
        let mut noop_vm = |_: Endpoint, _: Endpoint, _: crate::service_slot::SysFlags| {};
        let prepared = update.start_update_prepare(
            &mut state.table,
            is_idle,
            false,
            &mut noop_abort,
            &mut noop_end,
            &mut crate::live_update::PrepareEffects {
                request_prepare: &mut noop_req,
                vm_prepare: &mut noop_vm,
            },
        );
        match prepared {
            Err(Errno::EAGAIN) => {
                // C ignores the abort's internals here (update.c:411 — the
                // call statement's value is unused).
                let _ = crate::live_update::abort_update_proc(
                    &mut update,
                    &mut state.table,
                    self.kernel.as_mut(),
                    Errno::EAGAIN.to_i32(),
                    ticks,
                    &mut |_| Ok(()),
                );
                state.update = update;
                return Err(Errno::EAGAIN);
            }
            Err(Errno::ESRCH) => {
                let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                let _ = update.end_update(
                    &mut state.table,
                    self.kernel.as_mut(),
                    0,
                    crate::live_update::RS_REPLY,
                    ticks,
                    &mut crate::live_update::EndEffects {
                        request_prepare: &mut noop_req,
                        run_script: &mut noop_script,
                    },
                );
                state.update = update;
                return Ok(0);
            }
            Err(e) => {
                state.update = update;
                return Err(e);
            }
            Ok(_) => {}
        }
        state.update = update;

        // Noblock (request.c:863-866) or the late reply on the last
        // descriptor's service (request.c:868-874).
        if noblock {
            return Ok(0);
        }
        if let Some(last) = state.update.chain.rev_iter().next() {
            let last_id = last.slot;
            crate::request::mark_late_reply(
                state.table.get_mut(last_id),
                m.m_source,
                minix_types::RS_UPDATE,
            );
        }
        Ok(minix_types::EDONTREPLY)
    }

    /// Dispatches one `RS_*` request (C: main.c:102-114 switch).
    ///
    /// Arms turn live in dependency order as their message-payload decode
    /// lands (the union-arm reads are the 19 safe-receive seam — OQ-4):
    /// `RS_SHUTDOWN` needs only `m_source` and is live; everything else
    /// falls through to [`dispatch::dispatch_request`]'s fail-closed table.
    pub(crate) fn do_request(
        &mut self,
        caller: Endpoint,
        call_nr: i32,
        msg: &mut minix_types::Message,
    ) -> Result<i32, Errno> {
        match call_nr {
            minix_types::RS_SHUTDOWN => self.do_shutdown(caller),
            minix_types::RS_UP => self.do_up(msg),
            minix_types::RS_EDIT => self.do_edit(msg),
            minix_types::RS_UPDATE => self.do_update(msg),
            minix_types::RS_DOWN => self.do_down(msg),
            minix_types::RS_LOOKUP => self.do_lookup(msg),
            minix_types::RS_FI => self.do_fi(msg),
            minix_types::RS_GETSYSINFO => self.do_getsysinfo(msg),
            minix_types::RS_SYSCTL => self.do_sysctl(msg),
            minix_types::RS_REFRESH => self.do_refresh(msg),
            minix_types::RS_RESTART => self.do_restart(msg),
            minix_types::RS_CLONE => self.do_clone(msg),
            minix_types::RS_UNCLONE => self.do_unclone(msg),
            n => Ok(dispatch::dispatch_request(n).0),
        }
    }

    /// The shared label-request preamble (13/14 arms): copy the 16-byte
    /// label (`copy_label` — request.c:121-123 shape), resolve the slot
    /// (`ESRCH`), and run the permission gate with the target's updating
    /// flag (manager.c:103-110). Returns the resolved slot.
    pub(crate) fn resolve_by_label(
        &mut self,
        m: &minix_types::Message,
        call: i32,
    ) -> Result<crate::service_slot::SlotId, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let Some((addr, len)) = m.rs_req_payload() else {
            return Err(Errno::EINVAL);
        };
        let mut label_buf = [0u8; crate::service_slot::RS_MAX_LABEL_LEN];
        let n = (len as usize).min(label_buf.len() - 1);
        self.kernel
            .safecopy_from(m.m_source, addr as usize, &mut label_buf[..n])?;
        label_buf[n] = 0;
        let label = crate::service_slot::Label::from_bytes(&label_buf[..n]);

        let Some(id) = state.table.lookup_by_label(&label) else {
            return Err(Errno::ESRCH);
        };
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let caller_euid = self.kernel.getnuid(m.m_source);
        crate::access::check_call_permission(
            m.m_source,
            call,
            Some(state.table.get(id)),
            &state.table,
            updating,
            caller_euid,
        )?;
        Ok(id)
    }

    /// The stop half of the label arms: `stop_service(rp, how)` — decision
    /// (manager.c:988-1008), slot mutations, late-reply bookkeeping, and
    /// the friendly signal via the PM face (`srv_kill`). Returns the
    /// handler result — the caller answers `EDONTREPLY`.
    pub(crate) fn stop_with_late_reply(
        &mut self,
        id: crate::service_slot::SlotId,
        how: crate::service_slot::RFlags,
        caller: Endpoint,
        request: i32,
    ) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let ticks = self.kernel.get_ticks()?;
        let decision = crate::request::stop_decision(state.table.get(id), how, ticks);
        decision.mutations.apply(state.table.get_mut(id));
        crate::request::mark_late_reply(state.table.get_mut(id), caller, request);
        let pid = state.table.get(id).pid.unwrap_or(0);
        match decision.signal {
            crate::request::StopSignal::Hangup => {
                // RS itself (manager.c:1003) — SIGHUP via the PM face.
                let _ = self.kernel.srv_kill(pid, 1);
            }
            crate::request::StopSignal::Term => {
                let _ = self.kernel.srv_kill(pid, 15);
            }
        }
        Ok(minix_types::EDONTREPLY)
    }

    /// C: `do_refresh` — request.c:390-419: label resolve, permission, then
    /// `stop_service(rp, RS_REFRESHING)` with the late reply armed — the
    /// caller is unblocked when the refresh completes (cleanup path).
    pub(crate) fn do_refresh(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let caller = m.m_source;
        let id = self.resolve_by_label(m, minix_types::RS_REFRESH)?;
        self.stop_with_late_reply(
            id,
            crate::service_slot::RFlags::REFRESHING,
            caller,
            minix_types::RS_REFRESH,
        )
    }

    /// C: `do_restart` — request.c:160-203: only a TERMINATED service can
    /// be restarted (EBUSY otherwise); the recovery script is suppressed
    /// for this one restart (saved, cleared, restored around
    /// `restart_service`).
    pub(crate) fn do_restart(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let id = self.resolve_by_label(m, minix_types::RS_RESTART)?;
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        if !state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::TERMINATED)
        {
            return Err(Errno::EBUSY);
        }
        // Restart the service, but make sure we don't call the script again
        // (request.c:191-196): save, clear, restart, restore.
        let script = state.table.get(id).script;
        state.table.get_mut(id).script[0] = 0;
        let ticks = self.kernel.get_ticks()?;
        let kernel = self.kernel.as_mut();
        let mut noop_exec = |_: &mut crate::service_slot::ServiceSlot| Ok(());
        let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
        let mut noop_asynsend = |_: Endpoint, _: &crate::ready::InitMessage| Ok(());
        crate::service_create::restart_service(
            &mut state.table,
            id,
            kernel,
            ticks,
            &mut crate::service_create::RestartEffects {
                read_exec: &mut noop_exec,
                run_script: &mut noop_script,
                asynsend: &mut noop_asynsend,
            },
        );
        state.table.get_mut(id).script = script;
        Ok(0)
    }

    /// C: `do_clone` — request.c:208-249: an existing replica → `EEXIST`;
    /// arm `SF_USE_REPL` and clone the service as an `RST_SYS_PROC`
    /// instance (the exec-read callback is the 19 seam).
    pub(crate) fn do_clone(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let id = self.resolve_by_label(m, minix_types::RS_CLONE)?;
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        if state.table.get(id).next_rp.is_some() {
            return Err(Errno::EEXIST);
        }
        state
            .table
            .get_mut(id)
            .pub_
            .sys_flags
            .insert(crate::service_slot::SysFlags::USE_REPL);
        let ticks = self.kernel.get_ticks()?;
        let mut noop_read_exec = |_: &mut crate::service_slot::ServiceSlot| Ok(());
        match crate::service_create::clone_service(
            &mut state.table,
            id,
            self.kernel.as_mut(),
            crate::privilege::PrivFlags::RST_SYS_PROC,
            0,
            ticks,
            &mut noop_read_exec,
        ) {
            Ok(_) => Ok(0),
            Err(e) => {
                state
                    .table
                    .get_mut(id)
                    .pub_
                    .sys_flags
                    .remove(crate::service_slot::SysFlags::USE_REPL);
                Err(e)
            }
        }
    }

    /// C: `do_unclone` — request.c:253-293: no replica → `ENOENT`; clear
    /// `SF_USE_REPL` and clean up the replica immediately
    /// (`cleanup_service_now` = both cleanup phases back-to-back,
    /// proto.h:53-55).
    pub(crate) fn do_unclone(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let id = self.resolve_by_label(m, minix_types::RS_UNCLONE)?;
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        if !state
            .table
            .get(id)
            .pub_
            .sys_flags
            .contains(crate::service_slot::SysFlags::USE_REPL)
        {
            return Err(Errno::ENOENT);
        }
        state
            .table
            .get_mut(id)
            .pub_
            .sys_flags
            .remove(crate::service_slot::SysFlags::USE_REPL);
        if let Some(next) = state.table.get(id).next_rp {
            let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
            crate::recovery::cleanup_service(
                &mut state.table,
                next,
                self.kernel.as_mut(),
                &mut noop_script,
            );
            crate::recovery::cleanup_service(
                &mut state.table,
                next,
                self.kernel.as_mut(),
                &mut noop_script,
            );
            state.table.get_mut(id).next_rp = None;
        }
        Ok(0)
    }

    /// C: `do_getsysinfo` — request.c:1095-1142. The permission gate and
    /// the `SI_*` classification run live; the copy-out half is gated on
    /// the rproctab byte ABI (edge E-RSWIRE: `sizeof(struct rproc)` cannot
    /// be pinned from this source tree, and the size gates
    /// `len > size`/`len != size` — request.c:1120-1121/:1135-1136 — need
    /// it), so it stays fail-closed until that landing.
    pub(crate) fn do_getsysinfo(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:1099-1101 — caller-only permission (no target slot).
        let caller_euid = self.kernel.getnuid(m.m_source);
        crate::access::check_call_permission(
            m.m_source,
            0,
            None,
            &state.table,
            false,
            caller_euid,
        )?;

        // C: request.c:1102-1105 + 1107-1133 — decode the request triple and
        // classify the table; unknown `what` → EINVAL (request.c:1131-1132).
        let Some((what, where_, size)) = m.getsysinfo_req() else {
            return Err(Errno::EINVAL);
        };
        crate::query::getsysinfo_table(what)?;

        // C: request.c:1107-1136 — `SI_PROCPUB_TAB` copies the whole public
        // table (`sizeof(struct rprocpub) * NR_SYS_PROCS` raw bytes, vacant
        // rows included — C copies the array, not the live rows) through
        // the exact-size gate at :1134-1136. `SI_PROC_TAB`/`SI_PROCALL_TAB`
        // need the *internal* `struct rproc` byte ABI (type.h:56-108), which
        // transitively pins `struct priv` (kernel/priv.h:21-72) and its
        // `minix_timer_t`/`sys_map_t`/`sigset_t` fields — no in-tree
        // consumer yet (the IS dump face is 08-stage-is); that pinning
        // stays the E-RSWIRE remainder and these arms fail closed.
        if what == crate::query::SI_PROCPUB_TAB {
            copy_out_procpub_table(
                self.kernel.as_mut(),
                &state.table,
                m.m_source,
                where_ as usize,
                size,
            )?;
            return Ok(0);
        }
        if what == crate::query::SI_PROC_TAB {
            copy_out_rproc_table(
                self.kernel.as_mut(),
                &state.table,
                m.m_source,
                where_ as usize,
                size,
            )?;
            return Ok(0);
        }
        if what == crate::query::SI_PROCALL_TAB {
            // C: request.c:1113-1121 — both tables back to back: rproc rows
            // first (early `len > size` gate at :1116-1118), then rprocpub
            // at dst_addr + proc_len, and the exact-size gate at :1134-1136.
            copy_out_procall_table(
                self.kernel.as_mut(),
                &state.table,
                m.m_source,
                where_ as usize,
                size,
            )?;
            return Ok(0);
        }
        Err(Errno::ENOSYS)
    }

    /// C: `do_sysctl` — request.c:1181-1228. Sub-type classification lives
    /// in query.rs; this shell owns the action dispatch. The console dump
    /// face (`print_services_status`/`print_update_status` — utility.c:
    /// 485-546) is the IS-stage assignment (todo §18.10 E-8); the request
    /// results below are RS's observable behavior.
    pub(crate) fn do_sysctl(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let subtype = m.rs_req_subtype().ok_or(Errno::EINVAL)?;
        match crate::query::classify_sysctl(subtype)? {
            crate::query::SysctlAction::PrintServices => Ok(0),
            crate::query::SysctlAction::UpdateStatus => Ok(0),
            crate::query::SysctlAction::UpdateStart | crate::query::SysctlAction::UpdateRun => {
                // C: request.c:1189-1211 — start_update_prepare(1): one
                // retry on a busy RS (request.c:1190 allow_retries = 1);
                // the prepare request/VM callbacks are the 19 asynsend seam
                // (noop here, same convention as do_period's restart).
                let is_idle = state.table.iter_in_use().all(|(_, s)| s.flags.is_idle());
                let mut noop_abort = |_: i32| {};
                let mut noop_end = |_: i32| {};
                let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                let mut noop_vm = |_: Endpoint, _: Endpoint, _: crate::service_slot::SysFlags| {};
                match state.update.start_update_prepare(
                    &mut state.table,
                    is_idle,
                    true,
                    &mut noop_abort,
                    &mut noop_end,
                    &mut crate::live_update::PrepareEffects {
                        request_prepare: &mut noop_req,
                        vm_prepare: &mut noop_vm,
                    },
                ) {
                    // C: request.c:1194-1198 — ESRCH means "done already" → OK.
                    Err(Errno::ESRCH) => Ok(0),
                    Err(e) => Err(e),
                    Ok(last) => {
                        if subtype == minix_types::sysctl::UPD_RUN {
                            // C: request.c:1202-1207 — the reply comes when
                            // the update completes (LATEREPLY + caller +
                            // RS_UPDATE) → EDONTREPLY.
                            crate::request::mark_late_reply(
                                state.table.get_mut(last),
                                m.m_source,
                                minix_types::RS_UPDATE,
                            );
                            Ok(minix_types::EDONTREPLY)
                        } else {
                            Ok(0) // UPD_START: prepare only, reply OK now.
                        }
                    }
                }
            }
            crate::query::SysctlAction::UpdateStop => {
                // C: request.c:1212-1215 — abort_update_proc(EINTR) composed
                // from the phase dispatch (live_update::abort_action,
                // update.c:707-743).
                let phase =
                    crate::live_update::update_phase(state.update.flags, state.update.chain.len());
                match crate::live_update::abort_action(phase) {
                    crate::live_update::AbortAction::Nothing => Err(Errno::EINVAL),
                    crate::live_update::AbortAction::ClearScheduled => {
                        let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                        state.update.clear_upds(
                            &mut state.table,
                            self.kernel.as_mut(),
                            &mut noop_script,
                        );
                        Ok(0)
                    }
                    crate::live_update::AbortAction::EndWithReply
                    | crate::live_update::AbortAction::EndWithCancel => {
                        // update.c:727-733 — pretend the current service
                        // failed to initialize (RS_REPLY) / prepare
                        // (RS_CANCEL); end_update owns the walk.
                        let reply_flag = if phase == crate::live_update::UpdatePhase::Initializing {
                            crate::live_update::RS_REPLY
                        } else {
                            crate::live_update::RS_CANCEL
                        };
                        let ticks = self.kernel.get_ticks()?;
                        let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
                        let mut noop_script = |_: &mut crate::service_slot::ServiceSlot| Ok(());
                        state.update.end_update(
                            &mut state.table,
                            self.kernel.as_mut(),
                            minix_types::EINTR,
                            reply_flag,
                            ticks,
                            &mut crate::live_update::EndEffects {
                                request_prepare: &mut noop_req,
                                run_script: &mut noop_script,
                            },
                        );
                        Ok(0)
                    }
                }
            }
        }
    }

    /// C: `do_lookup` — request.c:1144-1176: name-length gate, copy the
    /// label from the caller (`m_rs_req.name`/`name_len`), look the service
    /// up, and write the endpoint into the request payload — `reply` echoes
    /// the mutated message back (request.c:1174).
    pub(crate) fn do_lookup(&mut self, m: &mut minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:1151-1157 — `len < 2 || len >= 100` → EINVAL, then
        // copy the label bytes (sys_datacopy, request.c:1158-1162).
        let Some((name, name_len)) = m.rs_req_name() else {
            return Err(Errno::EINVAL);
        };
        crate::query::lookup_name_len(name_len as usize)?;

        let mut namebuf = [0u8; crate::query::NAME_BUF_LEN];
        let n = (name_len as usize).min(namebuf.len() - 1);
        self.kernel
            .safecopy_from(m.m_source, name as usize, &mut namebuf[..n])?;
        namebuf[n] = 0;

        let label = crate::service_slot::Label::from_bytes(&namebuf[..n]);
        let Some(id) = state.table.lookup_by_label(&label) else {
            return Err(Errno::ESRCH);
        };
        // C: request.c:1174 — m_rs_req.endpoint = rrpub->endpoint; the main
        // loop's reply (m_type = OK) carries it back to the caller.
        let endpoint = state.table.get(id).pub_.endpoint;
        m.set_rs_req_endpoint(endpoint);
        Ok(0)
    }

    /// C: `do_fi` — request.c:1229-1263: copy the target label, resolve the
    /// slot, check permission against `RS_FI`, then inject the fault
    /// (`fi_service` — an asynchronous `COMMON_REQ_FI_CTL` crash request,
    /// utility.c:69-77).
    pub(crate) fn do_fi(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:1244-1246 — copy_label(source, m_rs_req.addr, len).
        let Some((addr, len)) = m.rs_req_payload() else {
            return Err(Errno::EINVAL);
        };
        let mut label_buf = [0u8; crate::service_slot::RS_MAX_LABEL_LEN];
        let n = (len as usize).min(label_buf.len() - 1);
        self.kernel
            .safecopy_from(m.m_source, addr as usize, &mut label_buf[..n])?;
        label_buf[n] = 0;
        let label = crate::service_slot::Label::from_bytes(&label_buf[..n]);

        // C: request.c:1249-1255 — lookup + permission against RS_FI.
        let Some(id) = state.table.lookup_by_label(&label) else {
            return Err(Errno::ESRCH);
        };
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let caller_euid = self.kernel.getnuid(m.m_source);
        crate::access::check_call_permission(
            m.m_source,
            minix_types::RS_FI,
            Some(state.table.get(id)),
            &state.table,
            updating,
            caller_euid,
        )?;

        // C: fi_service — utility.c:69-77: COMMON_REQ_FI_CTL + RS_FI_CRASH,
        // asynchronous send (the seam is IpcApi::asynsend).
        let fi = minix_types::LsysFiCtl {
            gid: 0,
            size: 0,
            subtype: minix_types::RS_FI_CRASH,
        }
        .encode_message();
        let target = state.table.get(id).pub_.endpoint;
        self.kernel.asynsend(target, &fi)?;
        Ok(0)
    }

    /// C: `do_down` — request.c:110-146: decode the target label
    /// (`copy_label` — a 16-byte payload, no structure ABI), resolve the
    /// slot, check permission, then either clean up an already-terminated
    /// service or run the stop flow. The reply is deferred until the service
    /// dies (`RS_LATEREPLY` + late_reply via the sigchld/cleanup path), so
    /// the handler always answers `EDONTREPLY`.
    pub(crate) fn do_down(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        // C: request.c:121-123 — copy_label(source, m_rs_req.addr, len).
        let Some((addr, len)) = m.rs_req_payload() else {
            return Err(Errno::EINVAL);
        };
        let mut label_buf = [0u8; crate::service_slot::RS_MAX_LABEL_LEN];
        let n = (len as usize).min(label_buf.len() - 1);
        self.kernel
            .safecopy_from(m.m_source, addr as usize, &mut label_buf[..n])?;
        label_buf[n] = 0;
        let label = crate::service_slot::Label::from_bytes(&label_buf[..n]);

        // C: request.c:126-134 — lookup + permission.
        let Some(id) = state.table.lookup_by_label(&label) else {
            return Err(Errno::ESRCH);
        };
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let caller_euid = self.kernel.getnuid(m.m_source);
        crate::access::check_call_permission(
            m.m_source,
            minix_types::RS_DOWN,
            Some(state.table.get(id)),
            &state.table,
            updating,
            caller_euid,
        )?;

        let ticks = self.kernel.get_ticks()?;
        if state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::TERMINATED)
        {
            // C: request.c:136-141 — a recovery script is bringing down an
            // already-gone service: unpublish + cleanup, reply OK now.
            // C: unpublish_service(rp) — manager.c:864-920 (the DS effect
            // seam is 19; the aggregate decision face is publish.rs, R32).
            let _ = crate::publish::unpublish_result(false, false, false, false);
            crate::recovery::cleanup_service(
                &mut state.table,
                id,
                self.kernel.as_mut(),
                &mut |_| Ok(()),
            );
            return Ok(0);
        }
        // C: request.c:142-145 — stop_service(rp, RS_EXITING) + late reply.
        let decision = crate::request::stop_decision(
            state.table.get(id),
            crate::service_slot::RFlags::EXITING,
            ticks,
        );
        decision.mutations.apply(state.table.get_mut(id));
        state
            .table
            .get_mut(id)
            .flags
            .insert(crate::service_slot::RFlags::LATEREPLY);
        state.table.get_mut(id).caller = m.m_source;
        state.table.get_mut(id).caller_request = minix_types::RS_DOWN;
        match decision.signal {
            crate::request::StopSignal::Hangup => {
                // RS itself (manager.c:1003) — SIGHUP via the PM face.
                let _ = self
                    .kernel
                    .srv_kill(state.table.get(id).pid.unwrap_or(0), 1);
            }
            crate::request::StopSignal::Term => {
                let _ = self
                    .kernel
                    .srv_kill(state.table.get(id).pid.unwrap_or(0), 15);
            }
        }
        Ok(minix_types::EDONTREPLY)
    }

    /// C: `do_up` — request.c:15-106: start a new system service from a
    /// full `rs_start_t` the caller holds in its own address space.
    /// Permission (request.c:21-23) → slot allocation (:25-31) →
    /// `copy_rs_start` (:33-37, the byte-ABI decode via
    /// `minix_types::decode_rs_start` + the buffer fetches of
    /// [`fetch_rs_start`]) → `check_request` (:38-41) → init-flags
    /// (:43-60) → `init_slot` (:62-68) → duplicate gates (:70-85) →
    /// `start_service` (:87-91) → noblock reply or late-reply arming
    /// (:93-106).
    pub(crate) fn do_up(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let Some((addr, _name_len)) = m.rs_req_payload() else {
            return Err(Errno::EINVAL);
        };
        crate::access::check_call_permission(
            m.m_source,
            minix_types::RS_UP,
            None,
            &state.table,
            false,
            self.kernel.getnuid(m.m_source),
        )?;

        // Allocate a new system service slot (request.c:25-31). The row is
        // *not* IN_USE yet — `create_service` marks it; a failure anywhere
        // below leaves it dirty-but-vacant exactly like C (alloc_slot is
        // find-only, Fix #54).
        let id = state.table.alloc_slot()?;

        // Copy the request structure (request.c:33-37 → manager.c:135-147):
        // the struct bytes, then the pointed-to buffers (fetch_rs_start).
        let mut buf = [0u8; minix_types::rs_start_off::SIZE];
        self.kernel
            .safecopy_from(m.m_source, addr as usize, &mut buf)?;
        let wire = minix_types::decode_rs_start(&buf)?;
        let kernel = self.kernel.as_mut();
        let rs_start = fetch_rs_start(kernel, m.m_source, &wire)?;

        crate::slot::check_request(&rs_start, &state.machine)?;

        // Check flags (request.c:43-60).
        let noblock = rs_start.flags.contains(crate::slot::RssFlags::NOBLOCK);
        let mut init_flags = 0u32;
        if rs_start
            .flags
            .contains(crate::slot::RssFlags::FORCE_INIT_CRASH)
        {
            init_flags |= crate::request::SEF_INIT_CRASH;
        }
        if rs_start
            .flags
            .contains(crate::slot::RssFlags::FORCE_INIT_FAIL)
        {
            init_flags |= crate::request::SEF_INIT_FAIL;
        }
        if rs_start
            .flags
            .contains(crate::slot::RssFlags::FORCE_INIT_TIMEOUT)
        {
            init_flags |= crate::request::SEF_INIT_TIMEOUT;
        }
        if rs_start
            .flags
            .contains(crate::slot::RssFlags::FORCE_INIT_DEFCB)
        {
            init_flags |= crate::request::SEF_INIT_DEFCB;
        }

        // Initialize the slot as requested (request.c:62-68). read_exec is
        // the 19 file-I/O seam (exec.c read_seg — E-11): the noop keeps the
        // orchestration observable without faking a real image load.
        // init_slot's reviewed shape (Fix #49) takes the row and the table
        // separately, so the row is taken out for the call and put back —
        // for a fresh allocation the donor scan (RSS_REUSE, edit_slot)
        // sees the same vacant row C's loop skips, and a failed init_slot
        // leaves the dirty-but-vacant row in place exactly like C.
        let ticks = self.kernel.get_ticks()?;
        let mut slot = core::mem::replace(
            state.table.get_mut(id),
            crate::service_slot::ServiceSlot::vacant(),
        );
        crate::service_create::init_slot(&mut slot, &rs_start, &state.table, &mut |_| Ok(()))?;
        *state.table.get_mut(id) = slot;

        // Duplicate gates (request.c:70-85): label, device number, domains.
        if state
            .table
            .lookup_by_label(&state.table.get(id).pub_.label)
            .is_some()
        {
            return Err(Errno::EBUSY);
        }
        let dev_nr = state.table.get(id).pub_.dev_nr;
        if dev_nr > 0 && state.table.lookup_by_dev_nr(dev_nr).is_some() {
            return Err(Errno::EBUSY);
        }
        for i in 0..usize::from(state.table.get(id).pub_.nr_domain) {
            let domain = state.table.get(id).pub_.domain[i];
            if state.table.lookup_by_domain(domain).is_some() {
                return Err(Errno::EBUSY);
            }
        }

        // Start the service (request.c:87-91): create → activate → publish
        // → run. read_exec/publish are the 19 file-I/O and DS seams (same
        // noop convention as do_down's script closure); asynsend collects
        // the RS_INIT sends and replays them through the real IpcApi seam
        // right after — `rs_asynsend` is asynchronous in C too
        // (utility.c:223-240, failures ignored), so the deferred send keeps
        // the observable order.
        let mut sent: alloc::vec::Vec<(Endpoint, minix_types::Message)> = alloc::vec::Vec::new();
        {
            let mut effects = crate::service_create::CreateEffects {
                asynsend: alloc::boxed::Box::new(
                    |ep: Endpoint, msg: &crate::ready::InitMessage| {
                        sent.push((ep, msg.encode_message()));
                        Ok(())
                    },
                ),
                ..Default::default()
            };
            crate::service_create::start_service(
                &mut state.table,
                id,
                self.kernel.as_mut(),
                init_flags,
                ticks,
                &mut effects,
            )?;
        }
        for (ep, out) in sent {
            let _ = self.kernel.asynsend(ep, &out);
        }

        // Unblock the caller immediately if requested (request.c:93-96);
        // otherwise arm the late reply (request.c:98-105) — the reply is
        // sent when the service completes initialization (12).
        if noblock {
            return Ok(0);
        }
        let slot = state.table.get_mut(id);
        slot.flags.insert(crate::service_slot::RFlags::LATEREPLY);
        slot.caller = m.m_source;
        slot.caller_request = minix_types::RS_UP;
        Ok(minix_types::EDONTREPLY)
    }

    /// C: `do_edit` — request.c:298-385: re-configure an existing service.
    /// The label comes from `rss_label` *inside* the rs_start struct (not
    /// from `m_rs_req.name` like the label arms), so the struct round-trip
    /// runs first. E-7's typed sequence — getpriv sync → sched_stop →
    /// edit_slot → privctl(UpdateSys) → vm_set_priv → sched_init_proc →
    /// replica refresh — is this handler body itself: the ordering is real
    /// sequentially-composed code with typed seams, closing R10's
    /// "call order only in comments" concern for this arm.
    pub(crate) fn do_edit(&mut self, m: &minix_types::Message) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let Some((addr, _)) = m.rs_req_payload() else {
            return Err(Errno::EINVAL);
        };

        // Copy the request structure (request.c:303-307) and its buffers.
        let mut buf = [0u8; minix_types::rs_start_off::SIZE];
        self.kernel
            .safecopy_from(m.m_source, addr as usize, &mut buf)?;
        let wire = minix_types::decode_rs_start(&buf)?;
        let rs_start = fetch_rs_start(self.kernel.as_mut(), m.m_source, &wire)?;

        // Copy label + lookup (request.c:309-322).
        let Some(id) = state.table.lookup_by_label(&rs_start.label) else {
            return Err(Errno::ESRCH);
        };
        // Permission (request.c:324-326) — the updating→EBUSY rule lives in
        // check_call_permission (manager.c:108-110).
        let updating = state
            .table
            .get(id)
            .flags
            .contains(crate::service_slot::RFlags::UPDATING);
        let caller_euid = self.kernel.getnuid(m.m_source);
        crate::access::check_call_permission(
            m.m_source,
            minix_types::RS_EDIT,
            Some(state.table.get(id)),
            &state.table,
            updating,
            caller_euid,
        )?;

        let endpoint = state.table.get(id).pub_.endpoint;

        // Synch the privilege structure with the kernel (request.c:329-334):
        // the kernel copy overwrites the slot's.
        let synced = self.kernel.getpriv(endpoint)?;
        state.table.get_mut(id).priv_ = synced;

        // Tell the scheduler this process is finished (request.c:336-341).
        // E-7: the stop gate routes by site — an edit aborts on failure
        // (the slot is untouched so far), a cleanup would continue.
        let scheduler = state.table.get(id).scheduler;
        let stop_result = match self.kernel.sched_stop(scheduler, endpoint) {
            Ok(()) => 0,
            Err(e) => e.to_i32(),
        };
        if let crate::sched::StopOutcome::Abort(e) =
            crate::sched::on_stop_result(crate::sched::StopSite::EditSlot, stop_result)
        {
            return Err(Errno::from_i32(e));
        }

        // Edit the slot as requested (request.c:343-347) — row out/in like
        // do_up (the reviewed slot-first signature; a failed edit leaves the
        // row dirty-but-vacant-free: it was in-use before and stays so, the
        // take/put only hides it from the donor scan).
        let ticks = self.kernel.get_ticks()?;
        let mut slot = core::mem::replace(
            state.table.get_mut(id),
            crate::service_slot::ServiceSlot::vacant(),
        );
        let edit_r = crate::slot::edit_slot(&mut slot, &rs_start, &state.table, &mut |_| Ok(()));
        *state.table.get_mut(id) = slot;
        edit_r?;

        // Update the privilege structure (request.c:349-355).
        self.kernel.privctl(
            endpoint,
            crate::privilege::PrivCtlOp::UpdateSys,
            Some(&state.table.get(id).priv_),
        )?;

        // Update VM calls (request.c:357-363).
        let (mask, is_sys) = {
            let s = state.table.get(id);
            (
                s.pub_.vm_call_mask,
                s.priv_
                    .flags
                    .contains(crate::privilege::PrivFlags::SYS_PROC),
            )
        };
        self.kernel.vm_set_priv(endpoint, mask, is_sys)?;

        // Reinitialize scheduling (request.c:365-370 → utility.c:364-382):
        // the pure decision decides skip vs kernel call.
        let (cfg, is_sys) = {
            let s = state.table.get(id);
            (
                crate::sched::SchedulerConfig::from_slot(
                    s.scheduler,
                    s.pub_.endpoint,
                    s.priority,
                    s.quantum,
                    s.cpu,
                ),
                s.priv_
                    .flags
                    .contains(crate::privilege::PrivFlags::SYS_PROC),
            )
        };
        if let crate::sched::SchedAction::Start(cfg) = crate::sched::sched_decision(&cfg, is_sys) {
            self.kernel.sched_init_proc(cfg)?;
        }

        // Cleanup old replicas and create a new one, if necessary
        // (request.c:372-382) — a clone failure only warns in C (the printf
        // is the 19 diag face), so the result is ignored here.
        if state
            .table
            .get(id)
            .pub_
            .sys_flags
            .contains(crate::service_slot::SysFlags::USE_REPL)
        {
            if let Some(next) = state.table.get(id).next_rp {
                crate::recovery::cleanup_service(
                    &mut state.table,
                    next,
                    self.kernel.as_mut(),
                    &mut |_| Ok(()),
                );
                state.table.get_mut(id).next_rp = None;
            }
            let _ = crate::service_create::clone_service(
                &mut state.table,
                id,
                self.kernel.as_mut(),
                crate::privilege::PrivFlags::RST_SYS_PROC,
                0,
                ticks,
                &mut |_| Ok(()),
            );
        }

        Ok(0)
    }

    /// no-restart sweep (`shutting_down` + `RS_EXITING` over the table —
    /// [`request::shutdown_apply`]). The NULL-message *internal* form
    /// (request.c:436 `m_ptr != NULL` gate) is the SIGTERM arm of
    /// `signal_handler` (Fix #57); the message form checks the caller here.
    pub(crate) fn do_shutdown(&mut self, caller: Endpoint) -> Result<i32, Errno> {
        let state = self.state.as_mut().ok_or(Errno::ENOSYS)?;
        let updating = state
            .update
            .flags
            .contains(live_update::RupdateFlags::UPDATING);
        // C: request.c:435-437 — check_call_permission(source, RS_SHUTDOWN,
        // NULL); the euid query is the T5 shell injection (04).
        let caller_euid = self.kernel.getnuid(caller);
        crate::access::check_call_permission(
            caller,
            minix_types::RS_SHUTDOWN,
            None,
            &state.table,
            updating,
            caller_euid,
        )?;
        state.shutting_down = crate::request::shutdown_apply(&mut state.table);
        Ok(0) // C: request.c:454 — return(OK)
    }
}
