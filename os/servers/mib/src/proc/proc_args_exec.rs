//! KERN_PROC_ARGS assembly: ps_strings decode and the page walk.
//!
//! Mirrors the *effect* half of `mib_kern_proc_args` (`proc.c:918-1176`).
//! The gates, size estimate, copy budget, and page-walk math are decided in
//! 19 ([`super::proc_args`]); the slot lookup and tables come from 16; the
//! state chain is not involved. Reading the target process's memory runs
//! through [`MibKernel::datacopy_from`] — the same verb C spells
//! `sys_datacopy(target, addr, SELF, buf, n)`.
//!
//! Truncation semantics differ from every other sysctl call, on purpose:
//! the walk returns the truncated size instead of ENOMEM + the real size
//! (libkvm relies on it, proc.c:1032-1035), and truncation needs no NUL
//! termination — userland copes (proc.c:1036-1038).
//!
//! 19-mib-proc-args.md.

use minix_types::{Endpoint, MProcSnap, PsStrings, PS_STRINGS_SIZE, ARG_MAX, ESRCH, PAGE_SIZE};

use super::proc_args::{
    cap_fragment, cap_oldlen, check_args_namelen, copy_budget, decode_req, fragment_split,
    is_count_estimate, locate_page, max_estimate, walk_can_start, ArgsReq, PageHome,
};
use super::proc2::check_pid_slot;
use super::rows::PmRows;
use super::tables::{chain_lookup, hash_slots, NO_SLOT};
use crate::dispatch::SysctlOutcome;
use crate::io::copy::Oldp;
use crate::transport::{MibKernel, MibServices};
use crate::walker::MibCtx;

/// Walk the argument/environment strings for one request. C:
/// `mib_kern_proc_args` — proc.c:918-1176.
///
/// `args` is the name tail past the `kern.proc_args` function node:
/// `[pid, req]`.
pub fn proc_args_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    args: &[i32],
    oldp: Option<&Oldp>,
) -> SysctlOutcome {
    let (pid, req) = match args {
        [pid, req] => (*pid, *req),
        _ => return SysctlOutcome::err(minix_types::EINVAL),
    };
    let kind = match decode_req(req) {
        Ok(k) => k,
        Err(code) => return SysctlOutcome::err(code),
    };
    if check_args_namelen(args.len() as u32).is_err() {
        return SysctlOutcome::err(minix_types::EINVAL);
    }

    // update_tables + slot lookup + zombie rule — proc.c:949-956.
    let now = match ctx.kernel.getticks() {
        Ok(t) => t,
        Err(code) => return SysctlOutcome::err(code),
    };
    if !ctx.tables.update(now, ctx.kernel, ctx.svc) {
        return SysctlOutcome::err(minix_types::EINVAL);
    }
    let pm = PmRows::new(&ctx.tables.pm_tab);
    let (hash, next, pids) = super::rows::build_pid_hash(&pm);
    let mslot = chain_lookup(
        &hash,
        &next,
        &pids,
        pid,
        hash_slots(minix_types::NR_PROCS as u32),
    );
    let zombie = mslot != NO_SLOT
        && pm
            .row(mslot as usize)
            .map(|r| PmRows::is_zombie(&r))
            .unwrap_or(false);
    if check_pid_slot(mslot == NO_SLOT, zombie).is_err() {
        return SysctlOutcome::err(ESRCH);
    }
    let row: MProcSnap = match pm.row(mslot as usize) {
        Some(r) => r,
        None => return SysctlOutcome::err(ESRCH),
    };

    // Count-only estimates skip the target entirely — proc.c:958-960.
    if is_count_estimate(oldp.is_none(), kind) {
        return SysctlOutcome::Done(4);
    }

    // The ps_strings block rides the exec frame tail — proc.c:963-965.
    let target = Endpoint(row.mp_endpoint);
    let frame_addr = row.mp_frame_addr;
    let frame_len = row.mp_frame_len;
    let mut pss_buf = [0u8; PS_STRINGS_SIZE];
    if ctx
        .kernel
        .datacopy_from(
            target,
            frame_addr + frame_len - PS_STRINGS_SIZE as u64,
            &mut pss_buf,
        )
        .is_err()
    {
        return SysctlOutcome::err(minix_types::EINVAL);
    }
    // SAFETY: `PsStrings` is `#[repr(C)]` Copy POD; the buffer is exactly
    // `PS_STRINGS_SIZE` long.
    let pss: PsStrings = unsafe { core::ptr::read_unaligned(pss_buf.as_ptr() as *const _) };

    // Estimate: capped by the frame length and ARG_MAX, rounded up to
    // whole pages — proc.c:967-981.
    let max = max_estimate(frame_len, ARG_MAX, PAGE_SIZE);

    match kind {
        ArgsReq::Nargv => {
            return copy_count(ctx, oldp, pss.ps_nargvstr);
        }
        ArgsReq::Nenv => {
            return copy_count(ctx, oldp, pss.ps_nenvstr);
        }
        ArgsReq::Argv | ArgsReq::Env => {}
    }

    let Some(oldp) = oldp else {
        // Size estimate for the string queries — proc.c:973/:977.
        return SysctlOutcome::Done(max);
    };
    let (vaddr, count) = match kind {
        ArgsReq::Argv => (pss.ps_argvstr, pss.ps_nargvstr),
        _ => (pss.ps_envstr, pss.ps_nenvstr),
    };

    // A null-page vector or a misaligned one: empty answer — proc.c:1040.
    if !walk_can_start(vaddr, PAGE_SIZE, 8) {
        return SysctlOutcome::Done(0);
    }

    let oldlen = cap_oldlen(oldp.left, max);
    let mut budget = copy_budget(ARG_MAX, PAGE_SIZE);

    // The three pages C keeps on the thread stack (proc.c:922-924); boxed
    // here so server stacks stay small in the hosted loop.
    let mut vbuf = alloc::boxed::Box::new([0u8; PAGE_SIZE as usize]);
    let mut sbuf = alloc::boxed::Box::new([0u8; PAGE_SIZE as usize]);
    let mut obuf = alloc::boxed::Box::new([0u8; PAGE_SIZE as usize]);

    let mut vaddr = vaddr;
    let mut count = count;
    let mut vpage: u64 = 0;
    let mut spage: u64 = 0;
    let mut off: u64 = 0;
    let mut olen: usize = 0;
    let mut aborted = false;

    // `while (count > 0 && off + olen < oldlen && !aborted)` — proc.c:1056.
    while count > 0 && (off + olen as u64) < oldlen && !aborted {
        // Fetch the page holding the current vector element if needed —
        // proc.c:1062-1068. Vector copies do not spend budget (:1027-1029).
        if vaddr / PAGE_SIZE * PAGE_SIZE != vpage {
            vpage = vaddr / PAGE_SIZE * PAGE_SIZE;
            if ctx
                .kernel
                .datacopy_from(target, vpage, &mut vbuf[..])
                .is_err()
            {
                break;
            }
        }
        // The current vector element points at the string. C reads a
        // `char *` — LP64, 8 bytes (proc.c:1070-1073).
        let at = (vaddr - vpage) as usize;
        let mut paddr = u64::from_le_bytes(vbuf[at..at + 8].try_into().unwrap());
        let mut ppage = paddr / PAGE_SIZE * PAGE_SIZE;
        if ppage == 0 {
            break;
        }

        // The string itself, one page at a time at most — proc.c:1076-1146.
        loop {
            // Which kept page holds it, if either (19's `locate_page`).
            let page: &[u8] = match locate_page(ppage, vpage, spage) {
                PageHome::Vector => &vbuf[..],
                PageHome::KeptString => &sbuf[..],
                PageHome::Fetch => {
                    if budget == 1 {
                        aborted = true;
                        break;
                    }
                    budget -= 1;
                    spage = ppage;
                    if ctx
                        .kernel
                        .datacopy_from(target, spage, &mut sbuf[..])
                        .is_err()
                    {
                        aborted = true;
                        break;
                    }
                    &sbuf[..]
                }
            };
            let base = (paddr - ppage) as usize;
            let frag = &page[base..PAGE_SIZE as usize];

            // Split at the terminator; without one, the whole fragment is
            // string and it continues on the next page (proc.c:1114-1121).
            let nul_at = frag.iter().position(|b| *b == 0).map(|p| p as u64);
            let (mut bytes, ended) = fragment_split(frag.len() as u64, nul_at);

            // Cap to the caller's length (proc.c:1124-1125).
            bytes = cap_fragment(off, olen as u64, bytes, oldlen);

            // Batch through obuf, flushing whole pages to the sink
            // (proc.c:1128-1141).
            if olen as u64 + bytes > PAGE_SIZE {
                let oleft = PAGE_SIZE as usize - olen;
                obuf[olen..].copy_from_slice(&frag[..oleft]);
                if let Err(code) = oldp.copyout(ctx.kernel, off, &obuf[..]) {
                    return SysctlOutcome::err(code);
                }
                off += PAGE_SIZE;
                olen = 0;
                bytes -= oleft as u64;
            }
            if bytes > 0 {
                let b = bytes as usize;
                obuf[olen..olen + b].copy_from_slice(&frag[..b]);
                olen += b;
            }

            // Next page of the same string (proc.c:1144-1146).
            paddr += frag.len() as u64;
            ppage = paddr;
            if ended || off + olen as u64 >= oldlen {
                break;
            }
        }

        vaddr += 8; // sizeof(char *)
        count -= 1;
    }

    // Trailing partial page (proc.c:1150-1156).
    if olen > 0 {
        if let Err(code) = oldp.copyout(ctx.kernel, off, &obuf[..olen]) {
            return SysctlOutcome::err(code);
        }
        off += olen as u64;
    }

    SysctlOutcome::Done(off)
}

/// `mib_copyout(oldp, 0, &count, sizeof(count))` — proc.c:969/:972.
fn copy_count<K: MibKernel>(
    ctx: &mut MibCtx<K, impl MibServices>,
    oldp: Option<&Oldp>,
    count: i32,
) -> SysctlOutcome {
    let Some(oldp) = oldp else {
        return SysctlOutcome::Done(4);
    };
    match oldp.copyout(ctx.kernel, 0, &count.to_le_bytes()) {
        Ok(_) => SysctlOutcome::Done(4),
        Err(code) => SysctlOutcome::err(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::CallAuth;
    use crate::heap::MibBudget;
    use crate::io::copy::Oldp;
    use crate::proc::test_mocks::{FakeKernel, FakeServices};
    use crate::proc::Tables;
    use crate::tree::arena::MibTree;
    use crate::walker::{self, Request};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use minix_types::{
        mp_flags, ProcInfoStruct, KERN_PROC_ARGV, KERN_PROC_ENV, KERN_PROC_NARGV,
        KERN_PROC_NENV,
    };

    fn name_bytes(name: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        let b = name.as_bytes();
        n[..b.len()].copy_from_slice(b);
        n
    }

    /// 目标进程内存像：帧 0x10000..0x12000（2 页），ps_strings 在帧尾；
    /// 向量在 0x11100（"hello"/"world"/NULL），串体在 0x11400 与 0x11808
    /// ——一个跨在向量页，一个落在独立的串页，恰好走到三种 PageHome。
    const FRAME: u64 = 0x1_0000;
    const FRAME_LEN: u64 = 0x2000;
    const TARGET_EP: i32 = 7;

    fn target_image() -> Vec<u8> {
        let mut mem = vec![0u8; FRAME_LEN as usize];
        // 像内索引一律用**帧相对偏移**（mock 的 target_base 才是绝对基址）。
        let base = 0usize;
        // ps_strings 落帧尾。
        let pss_at = base + FRAME_LEN as usize - 32;
        // SAFETY(test): repr(C) POD 写进像内窗口，索引在界内。
        unsafe {
            core::ptr::write_unaligned(
                mem.as_mut_ptr().add(pss_at) as *mut minix_types::PsStrings,
                minix_types::PsStrings {
                    ps_argvstr: FRAME + 0x100,
                    ps_nargvstr: 2,
                    ps_envstr: FRAME + 0x140,
                    ps_nenvstr: 1,
                },
            );
        }
        // ARGV 向量 @+0x100：[hello, world, NULL]。
        // 向量里存的是**进程绝对地址**（FRAME + 帧内偏移）——C 的 walk 按
        // 绝对地址找页，0 页指针即终止（proc.c:1075-1077）。
        let put_ptr = |mem: &mut Vec<u8>, at: usize, p: u64| {
            mem[at..at + 8].copy_from_slice(&p.to_le_bytes());
        };
        put_ptr(&mut mem, base + 0x100, FRAME + 0x400);
        put_ptr(&mut mem, base + 0x108, FRAME + 0x808);
        put_ptr(&mut mem, base + 0x110, 0);
        // 串体："hello" 在向量页内（+0x400），"world" 跨到下一页的页首
        // 附近（+0x808），NULL 终止符都在串内。
        mem[base + 0x400..base + 0x406].copy_from_slice(b"hello\0");
        mem[base + 0x808..base + 0x80e].copy_from_slice(b"world\0");
        // ENV 向量 @+0x140：["A=1", NULL]。
        put_ptr(&mut mem, base + 0x140, FRAME + 0x600);
        put_ptr(&mut mem, base + 0x148, 0);
        mem[base + 0x600..base + 0x604].copy_from_slice(b"A=1\0");
        mem
    }

    fn pm_table() -> Vec<u8> {
        let prow = core::mem::size_of::<MProcSnap>();
        let mut v = vec![0u8; minix_types::NR_PROCS * prow];
        // SAFETY(test): repr(C) POD 写进槽窗口，索引在界内。
        unsafe {
            core::ptr::write_unaligned(
                v.as_mut_ptr() as *mut MProcSnap,
                MProcSnap {
                    mp_pid: 42,
                    mp_flags: mp_flags::IN_USE,
                    mp_endpoint: TARGET_EP,
                    mp_frame_addr: FRAME,
                    mp_frame_len: FRAME_LEN,
                    mp_name: name_bytes("worker"),
                    ..MProcSnap::default()
                },
            );
        }
        v
    }

    fn fixture() -> (FakeKernel, FakeServices) {
        let mut k = FakeKernel {
            proctab: vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * core::mem::size_of::<ProcInfoStruct>()],
            ticks: 1000,
            hz: 50,
            boot: 172_800,
            sink: RefCell::new(Vec::new()),
            target_base: FRAME,
            target_mem: target_image(),
        };
        // getproctab 只需前几行有效（拉取按整表拷贝）。
        k.proctab.truncate(0);
        k.proctab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * core::mem::size_of::<ProcInfoStruct>()];
        (k, FakeServices { pm_tab: pm_table() })
    }

    fn e2e_ctx<'a>(
        kernel: &'a mut FakeKernel,
        svc: &'a mut FakeServices,
        tables: &'a mut Tables,
    ) -> MibCtx<'a, FakeKernel, FakeServices> {
        let tree = Box::leak(Box::new(MibTree::init()));
        let budget = Box::leak(Box::new(MibBudget::new()));
        MibCtx {
            tree,
            budget,
            kernel,
            svc,
            tables,
            self_endpt: Endpoint::NONE,
            caller: Endpoint::PM,
            auth: CallAuth::Yes,
        }
    }

    fn query(
        c: &mut MibCtx<FakeKernel, FakeServices>,
        pid: i32,
        req: i32,
        oldp: Option<Oldp>,
    ) -> SysctlOutcome {
        walker::sysctl(
            c,
            &mut Request {
                name: &[minix_types::CTL_KERN, minix_types::KERN_PROC_ARGS, pid, req],
                oldp,
                newp: None,
            },
        )
    }

    /// ARGV 全量：两根串按序拷出，NULL 终止符在内（C 的串边界含 '\0'，
    /// proc.c:1114-1121）。
    #[test]
    fn test_proc_args_argv_end_to_end() {
        let (mut k, mut s) = fixture();
        let mut tables = Tables::new();
        {
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(
                &mut c,
                42,
                KERN_PROC_ARGV,
                Some(Oldp { endpt: Endpoint::PM, addr: 0, left: 1 << 20 }),
            );
            assert_eq!(out, SysctlOutcome::Done(12)); // "hello\0" + "world\0"
            let sink = c.kernel.sink.borrow();
            assert_eq!(&sink[..12], b"hello\0world\0");
        }
    }

    #[test]
    fn test_proc_args_env_and_counts() {
        // ENV 全量：一根 "A=1\0"。
        {
            let (mut k, mut s) = fixture();
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(
                &mut c,
                42,
                KERN_PROC_ENV,
                Some(Oldp { endpt: Endpoint::PM, addr: 0, left: 1 << 20 }),
            );
            assert_eq!(out, SysctlOutcome::Done(4));
            let sink = c.kernel.sink.borrow();
            assert_eq!(&sink[..4], b"A=1\0");
        }

        // NARGV/NENV：计数走 4 字节回复（proc.c:969/:972）。
        {
            let (mut k, mut s) = fixture();
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(
                &mut c,
                42,
                KERN_PROC_NARGV,
                Some(Oldp { endpt: Endpoint::PM, addr: 0, left: 64 }),
            );
            assert_eq!(out, SysctlOutcome::Done(4));
            assert_eq!(c.kernel.sink.borrow()[..4], 2i32.to_le_bytes());
        }

        {
            let (mut k, mut s) = fixture();
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(
                &mut c,
                42,
                KERN_PROC_NENV,
                Some(Oldp { endpt: Endpoint::PM, addr: 0, left: 64 }),
            );
            assert_eq!(out, SysctlOutcome::Done(4));
            assert_eq!(c.kernel.sink.borrow()[..4], 1i32.to_le_bytes());
        }

        // NARGV 的 oldp=NULL 估算：sizeof(count)，不打扰目标（:958-960）。
        {
            let (mut k, mut s) = fixture();
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(&mut c, 42, KERN_PROC_NARGV, None);
            assert_eq!(out, SysctlOutcome::Done(4));
        }
    }

    #[test]
    fn test_proc_args_estimate_and_gates() {
        let (mut k, mut s) = fixture();

        // ARGV 估算：roundup(min(0x2000, ARG_MAX), 4096) = 0x2000。
        {
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(&mut c, 42, KERN_PROC_ARGV, None);
            assert_eq!(out, SysctlOutcome::Done(0x2000));
        }

        // 未知请求号 → EOPNOTSUPP（不是 EINVAL——形状对、操作不存在）。
        {
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(&mut c, 42, 99, None);
            assert_eq!(out, SysctlOutcome::err(minix_types::EOPNOTSUPP));
        }

        // 未知 pid → ESRCH。
        {
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(&mut c, 999, KERN_PROC_ARGV, None);
            assert_eq!(out, SysctlOutcome::err(ESRCH));
        }

        // 拷出截断：left=5 → 只回 5 字节且返回 5（C 的截断语义，
        // proc.c:1032-1038——不回 ENOMEM）。
        {
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = query(
                &mut c,
                42,
                KERN_PROC_ARGV,
                Some(Oldp { endpt: Endpoint::PM, addr: 0, left: 5 }),
            );
            assert_eq!(out, SysctlOutcome::Done(5));
            let sink = c.kernel.sink.borrow();
            assert_eq!(&sink[..5], b"hello");
        }
    }
}
