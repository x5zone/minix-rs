//! Kernel-side trap/syscall dispatch bodies — S-8 (smp_todo.md §3.7).
//!
//! The x86-64 asm stubs (`minix_arch::x86_64::trap_stub`) save context and
//! call the two bodies registered here at `init_protection` time. This
//! module owns the *policy* (exception → `ExceptionDispatcher`, IRQ →
//! `IrqManager::dispatch`, syscall → `kernel_call`) while the arch module
//! owns the *mechanics* (frame save, iretq/sysretq).
//!
//! # C parity (mpx.S / trap.c / system.c)
//!
//! - **Kernel-origin traps** (C `TEST_INT_IN_KERNEL` light path): the
//!   interrupted context already owns the BKL, so handlers run under it via
//!   `BklSection::assume_held()` — the same root contract
//!   `dispatch_hardware_irq` and `clock_irq_handler` already witness. C's
//!   interrupt handlers take no lock for the same reason.
//! - **User-origin traps** (S-6/S-7 wired): the body captures the per-CPU
//!   current process, acquires the BKL (user code runs without it —
//!   `finish_and_restore` released it), and acts on the outcome:
//!   `ForwardToVm` sets RTS_PAGEFAULT and sends the `VM_PAGEFAULT`
//!   message to VM FROM_KERNEL (C: `pagefault()` — exception.c:112-129);
//!   `Signal` calls `cause_signal` (C: `cause_sig` — exception.c:276);
//!   VM's own fault panics (C: "pagefault in VM" — exception.c:101-118).
//!   Both acting arms end in the scheduling loop (C: mpx.S
//!   `jmp switch_to_user`).
//! - **Recovery-class outcomes** (NK2-A ②③④ wired): `SpuriousNmi` prints
//!   and returns (C: exception.c:191-194); `ClearTrapFlag` clears the
//!   trace bit in the frame and returns (C: exception.c:243-245, fed by
//!   the saved-PSW/trap-style legitimacy inputs); `RedirectToRecovery` /
//!   `PhysCopyFault` are invariant panics — the kernel copy paths are
//!   validate-first (ipc.rs / vm.rs), so C's RIP-redirect recovery
//!   (klib.S labels) has no producer here. Only `FpuTrap` remains a
//!   registered gap (lazy-FPU restore, X-8 / T5 wave).
//! - **Syscall body**: reads the per-CPU `proc_ptr` anchor before any lock
//!   (C reads `proc_ptr` at trap entry the same way), then runs the
//!   existing `kernel_call` wrapper — which acquires/releases the BKL
//!   around dispatch+finish itself. The user message pointer travels in
//!   RDI (first SysV argument register); x86-64 is a new port, C's 32-bit
//!   register choice does not transfer (doc 13 §6.3 documents the ABI).

use crate::syscall::KcallResult;
use minix_arch::exception::{ExceptionArch, FaultContext};
use minix_arch::exception_dispatcher::{ExceptionDispatcher, ExceptionOutcome, ExceptionSignal};
use minix_arch::TrapStyle;
#[cfg(target_arch = "x86_64")]
use minix_arch::x86_64::exception::X86_64ExceptionFrame;
#[cfg(target_arch = "x86_64")]
use minix_arch::x86_64::trap_stub::TrapFrame;
use minix_types::VirBytes;

/// The int-33 IPC gate (C: IPC_VECTOR_ORIG = 33, interrupt.h:33).
#[cfg(target_arch = "x86_64")]
const IPC_VECTOR_GATE: u8 = 33;

/// RFLAGS trace bit — single-step flag (C: `TRACEBIT` 0x0100,
/// i386 archconst.h:120). Read from the saved process PSW for the
/// nested-debug legitimacy check; cleared in the frame by the
/// ClearTrapFlag acting arm (exception.c:243).
#[cfg(target_arch = "x86_64")]
const TRACEBIT: u64 = 0x0100;

/// Build the CPU-pushed tail of the frame as an `X86_64ExceptionFrame` for
/// the arch-generic dispatcher (which is implemented over that type).
#[cfg(target_arch = "x86_64")]
fn exception_frame_of(frame: &TrapFrame) -> X86_64ExceptionFrame {
    X86_64ExceptionFrame {
        vector: frame.vector,
        errcode: frame.errcode,
        rip: frame.rip,
        cs: frame.cs,
        rflags: frame.rflags,
        rsp: frame.rsp,
        ss: frame.ss,
    }
}

#[cfg(target_arch = "x86_64")]
/// C: the `SAVE_PROCESS_CTX(0, KTS_INT_HARD)` macro body every IRQ/clock
/// stub in mpx.S runs at trap entry (mpx.S:77/137/355/540; sconst.h:75-89)
/// — `SAVE_GP_REGS` mirrors ALL interrupted user registers into
/// `saved_proc->p_reg` (EBX included; on i386 the IPC status register and
/// a callee-saved GP are the same register, ipcconst.h:10) and records
/// `p_kern_trap_style`, BEFORE any scheduling policy runs.
///
/// NK4-A Task C fix: the IRQ/tick arms of this port never saved the frame
/// (the old comment at the profile-clock handoff admitted it), so a
/// process moved by quantum expiry or the user scheduler woke through
/// `finish_and_restore` (kernel/lib.rs, "rebuild the frame from the
/// arch-private context") with a stale register file — RBX's live
/// callee-saved value rolled back to whatever the last trap-entry save or
/// the IPC-status/ps_strings semantics left there (real machine c19a-c21a:
/// RS step2 saw `&self.table` = 0x0 after a pf/IPC round trip, crashing
/// in `endpoint_slot`). Kernel-origin interrupts skip: the frame holds
/// kernel registers there, and the process's user context was already
/// saved in full by its own syscall/trap entry.
///
/// # Safety
/// Caller is the A-path trap body with the BKL owned per the arm's
/// contract (user-origin IRQs run under the interrupted context's lock —
/// same witness `check_quantum` and `dispatch_hardware_irq` already use).
unsafe fn save_irq_frame_to_context(frame: &TrapFrame) {
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let cur_nr = {
        let smp = crate::smp_state_with(&section);
        smp.cpu_local(crate::current_cpu_id())
            .and_then(|l| l.proc_ptr)
    };
    if let Some(nr) = cur_nr {
        let table = crate::proc_table_with(&section);
        if let Some(proc) = table.get_mut(nr) {
            mirror_irq_frame_into_proc(frame, proc);
        }
    }
}

/// The testable core of [`save_irq_frame_to_context`]: the CPL gate plus
/// the full-context mirror. Returns whether the frame was mirrored (the
/// host-side test pins both branches without boot-global fixtures).
///
/// x86-64 only: the parameter is the x86 `TrapFrame`（`cs`/`rip`/`rbx` 字段）,
/// 本函数与它的调用点都服务于 NK4-A/NK4-B 的 x86_64 取证，
/// 按架构门编译（NK4-B M3.1 补上这道门）。
#[cfg(target_arch = "x86_64")]
fn mirror_irq_frame_into_proc(
    frame: &TrapFrame,
    proc: &mut crate::proc::KProcess,
) -> bool {
    if frame.cs & 3 != 3 {
        // kernel re-entry: the frame holds kernel registers; the process's
        // user context was already saved in full by its own trap entry.
        return false;
    }
    minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
    proc.trap_style = TrapStyle::FullContext;
    #[cfg(not(feature = "mock"))]
    nk4a_rbx_probe(
        "irq-save",
        proc.p_endpoint.0 as u64,
        frame.rbx,
        frame.rip,
    );
    #[cfg(not(feature = "mock"))]
    {
        nk4a_rs_trace_probe("irq", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
        nk4a_rs_anom_probe("irq", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
        nk4a_rs_leak_probe("irq", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
    }
    true
}

#[cfg(not(feature = "mock"))]
/// NK4-A Task C 第 6 轮判别探针（task1-close 裁决删除）：内核每一次改写
/// 某进程保存上下文里的 RBX（IPC 状态寄存器家族）都打一条 —— tag 标站点、
/// `ep` 是目标进程 endpoint、`rbx` 是写入后读回的值、`rip` 是该上下文当前
/// 保存的返回点（无 rip 概念的站点打 0）。
///
/// 判别目标：真机 c23a 实证 `finish_and_restore` 交给 RS 的
/// `rip=0x203bf0`（`asynsend` 首指令，`push rbx` 保护之前）配对
/// `rbx=0x0`，RS 由此永久丢失 LLVM 常驻 RBX 的 `&self.table`（反汇编
/// 0x20d2db `mov %rbx,%rdi` 传 endpoint_slot 的 self）。本探针回答「哪一
/// 站把 0 写进了 RS 的 ctx.rbx」，与 `pf-save`（故障入口采到的 RBX）、
/// `pre-restore rbx=`（恢复交付值）三点闭环。
///
/// 只打 `rbx == 0` 的站点（否则 add-call/add-flags 在 VM 投递上每消息
/// 一条，40 条上限会在启动前段耗尽，看不到崩溃现场）。全局限 40 条。
pub(crate) fn nk4a_rbx_probe(tag: &str, ep: u64, rbx: u64, rip: u64) {
    if rbx != 0 {
        return;
    }
    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
    static RBX_PROBE: AtomicUsize = AtomicUsize::new(0);
    if RBX_PROBE.fetch_add(1, AtomicOrd::Relaxed) < 40 {
        use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
        C0::write_str("nk4a: rbxw ");
        C0::write_str(tag);
        C0::write_str(" ep=");
        C0::write_hex(ep);
        C0::write_str(" rbx=");
        C0::write_hex(rbx);
        C0::write_str(" rip=");
        C0::write_hex(rip);
        C0::write_str("\n");
    }
}

#[cfg(not(feature = "mock"))]
/// NK4-B P1 第 7 轮配对轨迹探针（task1-close 裁决删除）：只看 rs
/// （endpoint 2），在「内核存下用户寄存器组」的每个站点无条件打印
/// `(site, rip, rbx)`，并按三元组去重、全局限 64 条。
///
/// 与 [`nk4a_rbx_probe`] 的分工：后者只打 `rbx == 0`、与 add-call /
/// add-flags 共享 40 条额度；c23a/c24a 证明那套过滤在 rs 崩溃前就
/// 被启动前段同一 refault 循环耗尽（NK4B-WORKLOG T0.4），崩溃现场的
/// 捕获值至今未拿到。本探针靠「只看 rs + 去重」把额度留给真正不
/// 同的 (站点, rip, rbx) 组合，用来回答一个二分：rs 的 live RBX 是在
/// 某个存帧点之**前**就被上一轮交付写坏（则所有站点看到的都是 0），
/// 还是在存帧→恢复之间被内核改写（则入口非 0、交付 0）。
pub(crate) fn nk4a_rs_trace_probe(site: &str, ep: u64, rbx: u64, rip: u64) {
    /// rs 的端点号（c23a `pf-save ep=0x2` 实证；MINIX3 里 RS = 2 号进程）。
    const RS_ENDPOINT: u64 = 2;
    const CAP: usize = 64;
    if ep != RS_ENDPOINT {
        return;
    }
    use core::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering as AtomicOrd};
    static N: AtomicUsize = AtomicUsize::new(0);
    static SEEN_RIP: [AtomicU64; CAP] = [const { AtomicU64::new(0) }; CAP];
    static SEEN_RBX: [AtomicU64; CAP] = [const { AtomicU64::new(0) }; CAP];
    static SEEN_SITE: [AtomicU8; CAP] = [const { AtomicU8::new(0) }; CAP];
    // 站点以首字节区分（pf2 / x33 / irq / sig / vms / rst 首字互不相同；
    // 探针的 site 首字必须唯一，否则去重会合并不同站点）。
    let site_key = site.as_bytes()[0];
    let n = N.load(AtomicOrd::Relaxed);
    let mut i = 0;
    while i < n && i < CAP {
        if SEEN_RIP[i].load(AtomicOrd::Relaxed) == rip
            && SEEN_RBX[i].load(AtomicOrd::Relaxed) == rbx
            && SEEN_SITE[i].load(AtomicOrd::Relaxed) == site_key
        {
            return;
        }
        i += 1;
    }
    if n >= CAP {
        return;
    }
    SEEN_RIP[n].store(rip, AtomicOrd::Relaxed);
    SEEN_RBX[n].store(rbx, AtomicOrd::Relaxed);
    SEEN_SITE[n].store(site_key, AtomicOrd::Relaxed);
    N.fetch_add(1, AtomicOrd::Relaxed);
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: rs-tr ");
    C0::write_str(site);
    C0::write_str(" n=");
    C0::write_hex(n as u64);
    C0::write_str(" rbx=");
    C0::write_hex(rbx);
    C0::write_str(" rip=");
    C0::write_hex(rip);
    C0::write_str("\n");
}

#[cfg(not(feature = "mock"))]
/// NK4-C S1 取证探针（task1-close 裁决删除）：内核未布防写点的
/// 目标三元组 `(tag, va, root→pa)` 去重记录 —— 把每个不同页组合打
/// 一条（上限 96），事后用 §7.3 脚本与同轮 `lvl1pa`（PT 页）对账：
/// 命中即错页写实锤。
///
/// 布防对象是 §4.4 第 1 项穷举后仅剩的两条裸写通路（其余写点已有
/// kdst/msgw）：`finw` = syscall.rs kernel_call_finish 的 errno 回执 DM
/// 直写（p_delivermsg_vir，每次带错误码完成的系统调用必触发，正落在
/// RS 持 CPU 的内核代执行窗口）；`viow` = pte_walk.rs copy_to_user
///（SYS_VDEVIO/SYS_SDEVIO 结果回写）。站点首字互不相同，去重键
/// `(site, va页, pa页)`，避免启动期重复事件吃光额度（prompt 铁律 2）。
pub(crate) fn nk4a_user_write_probe(site: &str, root: u64, va: u64, pa: u64) {
    const CAP: usize = 96;
    use core::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering as AtomicOrd};
    static N: AtomicUsize = AtomicUsize::new(0);
    static SEEN_VA: [AtomicU64; CAP] = [const { AtomicU64::new(0) }; CAP];
    static SEEN_PA: [AtomicU64; CAP] = [const { AtomicU64::new(0) }; CAP];
    static SEEN_SITE: [AtomicU8; CAP] = [const { AtomicU8::new(0) }; CAP];
    let site_key = site.as_bytes()[0];
    let va_page = va & !0xFFF;
    let pa_page = pa & !0xFFF;
    let n = N.load(AtomicOrd::Relaxed);
    let mut i = 0;
    while i < n && i < CAP {
        if SEEN_VA[i].load(AtomicOrd::Relaxed) == va_page
            && SEEN_PA[i].load(AtomicOrd::Relaxed) == pa_page
            && SEEN_SITE[i].load(AtomicOrd::Relaxed) == site_key
        {
            return;
        }
        i += 1;
    }
    if n >= CAP {
        // S2h 评审修复：配额触顶打一条现形标记——否则「没打印」与
        // 「没命中」不可区分，阴性结论不可信。
        if n == CAP {
            N.fetch_add(1, AtomicOrd::Relaxed);
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: w-cap\n");
        }
        return;
    }
    SEEN_VA[n].store(va_page, AtomicOrd::Relaxed);
    SEEN_PA[n].store(pa_page, AtomicOrd::Relaxed);
    SEEN_SITE[n].store(site_key, AtomicOrd::Relaxed);
    N.fetch_add(1, AtomicOrd::Relaxed);
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: w-");
    C0::write_str(site);
    C0::write_str(" n=");
    C0::write_hex(n as u64);
    C0::write_str(" va=");
    C0::write_hex(va);
    C0::write_str(" root=");
    C0::write_hex(root);
    C0::write_str(" pa=");
    C0::write_hex(pa);
    C0::write_str("\n");
}

#[cfg(not(feature = "mock"))]
#[cfg(target_arch = "x86_64")]
/// NK4-C S2c 哨兵探针（task1-close 裁决删除）：在 RS 栈区三页
/// （0x7fffffff9000 / a000 / c000）扫描「值等于 `self`（表地址
/// 0x7fffffffc800）」的 u64 槽。每个内核观测点经 DM 逐页重扫，任一页
/// 的「(条目在否, 命中数, 首命中偏移)」变化就打一条（上限 64）。
///
/// 选型依据：s2c 实证崩溃窗口内 c800 页的 PTE 条目与表首字全程
/// 正常翻动，而 self 从栈 reload 后读出 0 —— 抹的是 self 存放槽的其
/// 它栈页数据，槽偏移未知，故改为按值扫描。观测点在调用方按 RS
/// 上下文门控；本函数只管按传入 root 读表，不做进程判定。
pub(crate) fn nk4a_pte_watch(site: &str, root_pa: u64) {
    const WATCH_SELF: u64 = 0x7fff_ffff_c800;
    const PAGES: [u64; 3] = [0x7fff_ffff_9000, 0x7fff_ffff_a000, 0x7fff_ffff_c000];
    // S2h 评审修复：64→256（s2g 时窗口已到 n=0x31，余量只剩 15 条）。
    const CAP: usize = 256;
    use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrd};
    // 打包状态：bit63=条目不在，bits[12..22)=命中数（0xFFF=NP 哨兵），
    // bits[0..12)=首命中槽字节偏移。
    static LAST: [AtomicU64; 3] = [const { AtomicU64::new(u64::MAX) }; 3];
    static N: AtomicUsize = AtomicUsize::new(0);
    if root_pa == 0 {
        return;
    }
    let rd = |pa: u64| -> u64 {
        let v = <minix_arch::CurrentDirectMap as minix_arch::DirectMapArch>::kernel_phys_to_virt(
            minix_types::PhysBytes(pa),
        );
        // SAFETY: DM 窗口覆盖全部物理内存；逐级表页都是已存在条目
        // 指向的常驻页表页（每级先查 present 位再读）。
        unsafe { core::ptr::read_volatile(v.0 as *const u64) }
    };
    let idx = |va: u64, sh: u64| (va >> sh) & 0x1FF;
    let mut state = [0u64; 3];
    for (si, page) in PAGES.iter().enumerate() {
        let page = *page;
        // x86-64 四级手动 walk（与 PF dump 同构）取该页 L1 条目。
        let pml4e = rd(root_pa + idx(page, 39) * 8);
        let mut frame_pa = 0u64;
        if pml4e & 1 != 0 {
            let pdpte = rd((pml4e & 0x000F_FFFF_FFFF_F000) + idx(page, 30) * 8);
            // S2h 评审修复：补 PDPT 级 PS 位（1GiB 巨页不当表页下钻）。
            if pdpte & 1 != 0 && pdpte & 0x80 == 0 {
                let pde = rd((pdpte & 0x000F_FFFF_FFFF_F000) + idx(page, 21) * 8);
                if pde & 1 != 0 && pde & 0x80 == 0 {
                    let pte = rd((pde & 0x000F_FFFF_FFFF_F000) + idx(page, 12) * 8);
                    if pte & 1 != 0 {
                        frame_pa = pte & 0x000F_FFFF_FFFF_F000;
                    }
                }
            }
        }
        // S2h 评审修复：表项内容已损时 frame_pa 可为任意值，对物理地址
        // 加上界（本环境 RAM ≤ 4GiB，串口日志 memmap 实证）越界时按
        // NP 处理——探针不能把被诊断的系统打死。
        if frame_pa == 0 || frame_pa >= 0x1_0000_0000 {
            state[si] = (1 << 63) | (0xFFF << 12);
            continue;
        }
        let mut cnt = 0u64;
        let mut first = 0xFFF;
        let mut off = 0u64;
        while off < 0x1000 {
            if rd(frame_pa + off) == WATCH_SELF {
                if cnt == 0 {
                    first = off;
                }
                cnt += 1;
            }
            off += 8;
        }
        state[si] = (cnt << 12) | first;
    }
    let mut changed = false;
    for (si, st) in state.iter().enumerate() {
        if LAST[si].load(AtomicOrd::Relaxed) != *st {
            LAST[si].store(*st, AtomicOrd::Relaxed);
            changed = true;
        }
    }
    if !changed {
        return;
    }
    let n = N.fetch_add(1, AtomicOrd::Relaxed);
    if n >= CAP {
        // S2h 评审修复：触顶现形标记（仅打一次，n==CAP 那一刻）。
        if n == CAP {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: pw-cap\n");
        }
        return;
    }
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: pw-");
    C0::write_str(site);
    C0::write_str(" n=");
    C0::write_hex(n as u64);
    C0::write_str(" s9=");
    C0::write_hex(state[0]);
    C0::write_str(" sa=");
    C0::write_hex(state[1]);
    C0::write_str(" sc=");
    C0::write_hex(state[2]);
    C0::write_str("\n");
}

#[cfg(not(feature = "mock"))]
/// NK4-B P1 第 8 轮异常轨迹探针（task1-close 裁决删除）：只看 rs，且只打
/// 「RBX 不可能是用户指针」的存帧/交付现场（`rbx < 0x10000`，涵盖 0 与
/// IPC 状态值两类），按 `(site, rbx, rip)` 去重、限 48 条。
///
/// 为何需要它：c25a 的 [`nk4a_rs_trace_probe`]（ep==2 全量 + 去重 + 64）
/// 仍在崩溃前耗尽（最后一条在第 2194 行，崩溃在第 3800+ 行），因为启动
/// 期 rs 不断引入新的正常 (rip, rbx) 组合。而 c25a 已证：pf2/rst/irq
/// 三站的 (rbx, rip) 逐对相等 → 存帧→恢复回路忠实，0 不是在内回路与
/// 交付之间被改写。本探针把额度全留给异常类（包括 0），以拿到
/// 崩溃那次现场及其上一个异常点。
pub(crate) fn nk4a_rs_anom_probe(site: &str, ep: u64, rbx: u64, rip: u64) {
    const RS_ENDPOINT: u64 = 2;
    const ANOM_CEILING: u64 = 0x10000;
    const CAP: usize = 48;
    if ep != RS_ENDPOINT || rbx >= ANOM_CEILING {
        return;
    }
    use core::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering as AtomicOrd};
    static N: AtomicUsize = AtomicUsize::new(0);
    static SEEN_RIP: [AtomicU64; CAP] = [const { AtomicU64::new(0) }; CAP];
    static SEEN_RBX: [AtomicU64; CAP] = [const { AtomicU64::new(0) }; CAP];
    static SEEN_SITE: [AtomicU8; CAP] = [const { AtomicU8::new(0) }; CAP];
    let site_key = site.as_bytes()[0];
    let n = N.load(AtomicOrd::Relaxed);
    let mut i = 0;
    while i < n && i < CAP {
        if SEEN_RIP[i].load(AtomicOrd::Relaxed) == rip
            && SEEN_RBX[i].load(AtomicOrd::Relaxed) == rbx
            && SEEN_SITE[i].load(AtomicOrd::Relaxed) == site_key
        {
            return;
        }
        i += 1;
    }
    if n >= CAP {
        return;
    }
    SEEN_RIP[n].store(rip, AtomicOrd::Relaxed);
    SEEN_RBX[n].store(rbx, AtomicOrd::Relaxed);
    SEEN_SITE[n].store(site_key, AtomicOrd::Relaxed);
    N.fetch_add(1, AtomicOrd::Relaxed);
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: rs-anom ");
    C0::write_str(site);
    C0::write_str(" n=");
    C0::write_hex(n as u64);
    C0::write_str(" rbx=");
    C0::write_hex(rbx);
    C0::write_str(" rip=");
    C0::write_hex(rip);
    C0::write_str("\n");
}

#[cfg(not(feature = "mock"))]
/// NK4-B P1 第 9 轮「第一次泄露」跃变探针（task1-close 裁决删除）：
/// 只看 rs，与上一个采样点比较——从「指针量级」（`>= 0x10000`）跳到
/// 「状态量级」（`< 0x10000`）时打印跃变前后的 `(site, rbx, rip)`，限 12 条。
///
/// 它把 c25a/c26a 的两类归因分开：若跃变发生在存帧站（pf2/irq/i33/sig/vms），
/// 说明内核保存之前用户态的 RBX 就已经被写坏（上一个交付点才是错点）；
/// 若跃变发生在交付站（rst），则是内核亲手把状态量交付到了一个不在
/// 用户态保存窗口（libc/ipc_trap 的 push-pop 窗口）内的恢复点。两者对
/// 下一步修复方向的含义不同，必须区分。
pub(crate) fn nk4a_rs_leak_probe(site: &str, ep: u64, rbx: u64, rip: u64) {
    const RS_ENDPOINT: u64 = 2;
    const POINTER_FLOOR: u64 = 0x10000;
    const CAP: usize = 12;
    use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrd};
    static PREV_RBX: AtomicU64 = AtomicU64::new(POINTER_FLOOR);
    static PREV_RIP: AtomicU64 = AtomicU64::new(0);
    static PREV_SITE: AtomicU64 = AtomicU64::new(0);
    static N: AtomicUsize = AtomicUsize::new(0);
    if ep != RS_ENDPOINT {
        return;
    }
    let prev = PREV_RBX.swap(rbx, AtomicOrd::Relaxed);
    let prev_rip = PREV_RIP.swap(rip, AtomicOrd::Relaxed);
    let prev_site = PREV_SITE.swap(site.as_bytes()[0] as u64, AtomicOrd::Relaxed) as u8;
    if prev < POINTER_FLOOR || rbx >= POINTER_FLOOR {
        return;
    }
    let n = N.fetch_add(1, AtomicOrd::Relaxed);
    if n >= CAP {
        return;
    }
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: rs-leak ");
    C0::write_str(site);
    C0::write_str(" n=");
    C0::write_hex(n as u64);
    C0::write_str(" from_site=");
    C0::write_str(match prev_site {
        b'i' => "irq",
        b'p' => "pf2",
        b's' => "sig",
        b'v' => "vms",
        b'r' => "rst",
        b'x' => "x33",
        _ => "other",
    });
    C0::write_str(" prev_rbx=");
    C0::write_hex(prev);
    C0::write_str(" prev_rip=");
    C0::write_hex(prev_rip);
    C0::write_str(" rbx=");
    C0::write_hex(rbx);
    C0::write_str(" rip=");
    C0::write_hex(rip);
    C0::write_str("\n");
}

#[cfg(target_arch = "x86_64")]
/// A-path body: exceptions, external IRQs, IPIs, soft-int gates, spurious.
///
/// Registered via `minix_arch::register_trap_dispatchers` before
/// `TrapEntryArch::load()`; reached from `x86_trap_common`.
///
/// # Safety
///
/// `frame` points at the TrapFrame built by the asm stub on the interrupted
/// stack; the stub resumes via iretq when this returns.
pub unsafe extern "C" fn x86_trap_dispatch_body(frame: &mut TrapFrame) {
    let vector = frame.vector as u8;

    // NK4-A fix27e+ 取证路标（task1-close 裁决删除）：cr3 切换后内核在
    // VM 页表上运行到 1dac2b12 空指针 #PF，handler 树又在 1dae6869 二次
    // #PF 递归（int_fix35 CR2=0x10 ×4686）。本探针证明 PF 是否到达
    // kernel body 并打印原始 RIP/err——若真机只见 12 条后静默，则崩溃点
    // 在 asm stub/dispatcher 层（探针前），本身即证词。mock（宿主）下
    // console 是真实端口写，编译掉（同 lib.rs 路标惯例）。
    // C-3 迭代1（评审 F0 续修）：补 CR2（故障 VA——判定是否恒 0x10）与
    // RSP（栈耗尽可见性），上限 12→20；并加 M1-M5 分段路标（各限 4 次）
    // 把递归点二分到 body 前段 / smp 读取 / 表读取 / handle / panic 渲染
    // 之一（每轮最后出现的路标 = 故障段）。
    #[cfg(not(feature = "mock"))]
    if vector == 14 {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static PF_MARK: AtomicUsize = AtomicUsize::new(0);
        let n = PF_MARK.fetch_add(1, AtomicOrd::Relaxed);
        if n < 20 {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            let cr2: u64;
            // SAFETY: reading CR2 has no side effects.
            unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)) };
            C0::write_str("nk4a: pf#");
            C0::write_hex(n as u64);
            C0::write_str(" rip=");
            C0::write_hex(frame.rip);
            C0::write_str(" err=");
            C0::write_hex(frame.errcode);
            C0::write_str(" cr2=");
            C0::write_hex(cr2);
            C0::write_str(" rsp=");
            C0::write_hex(frame.rsp);
            // 第 27 轮：CR3 直读——current_root_phys() 在故障风暴期返回
            // None（整轮 walk= 缺席），打印真实活动根定位「哪个进程的
            // 页表在故障」。
            {
                let cr3: u64;
                // SAFETY: reading CR3 has no side effects.
                unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack)) };
                C0::write_str(" cr3=");
                C0::write_hex(cr3 & 0x000F_FFFF_FFFF_F000);
            }
            // C-3 F0 迭代4：内核独立走当前 root 查 cr2 的 PTE——分辨
            // "VM 的 PTE 写未持久到内存"（walk NP）与"写生效但翻译/TLB
            // 不一致"（walk 命中）。
            if let Some(root) = crate::current_root_phys() {
                match crate::pte_walk::walk_x86_64(root, minix_types::VirBytes(cr2)) {
                    Some((pa, fl)) => {
                        C0::write_str(" walk=0x");
                        C0::write_hex(pa.0);
                        C0::write_str("/0x");
                        C0::write_hex(fl.bits() as u64);
                    }
                    None => C0::write_str(" walk=NP"),
                }
            }
            C0::write_str("\n");
        }
    }

    // C-3 迭代1 分段路标：每个站点独立计数（static 展开在各站点），限 4 次。
    // 用法：nk4a_stage_mark!("pfm2") —— 串口打 "nk4a: pfm2"。
    #[cfg(not(feature = "mock"))]
    macro_rules! nk4a_stage_mark {
        ($tag:literal) => {{
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static STAGE_MARK: AtomicUsize = AtomicUsize::new(0);
            let n = STAGE_MARK.fetch_add(1, AtomicOrd::Relaxed);
            if n < 4 {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                C0::write_str("nk4a: ");
                C0::write_str($tag);
                C0::write_str("\n");
            }
        }};
    }

    // NK4-A C-3 迭代9 取证（task1-close 裁决删除）：全向量采样器（限 40）
    // ——验证 PIT/时钟中断是否触发（tick 活性），并采样被中断用户态
    // （RS）的 rip 推进轨迹。
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static VEC_SAMPLE: AtomicUsize = AtomicUsize::new(0);
        let vn = VEC_SAMPLE.fetch_add(1, AtomicOrd::Relaxed);
        // 迭代10：只采样 PIT（v=0x50）——每个时钟 tick 打印被打断者的
        // rip，即 RS 用户态自旋点的周期采样（RS ELF 可符号化）。
        if vector == 0x50 && vn < 400 {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: vs");
            C0::write_hex(vn as u64);
            C0::write_str(" rip=0x");
            C0::write_hex(frame.rip);
            C0::write_str("\n");
        }
    }

    // E1 trap bridge: vector 33 (IPC_VECTOR) from user mode is the IPC
    // soft-int leg (C: IPC_VECTOR_ORIG, interrupt.h:33; gate DPL=3,
    // trap_entry.rs configure_ipc_entry). The register ABI is the C i386
    // soft-int convention widened to 64 bits — RAX = src/dst endpoint
    // (SENDA: count), RBX = message pointer (SENDA: table pointer),
    // RCX = IPC call number (design doc 18, decision 2; C:
    // usermapped_glo_ipc.S IPCARGS/SENDA_ARGS). errno returns in RAX;
    // IPC status rides the saved-context RBX channel as already wired
    // (or_ipc_status_reg / set_secondary_ipc_return).
    if vector == IPC_VECTOR_GATE {
        let cur_nr = current_ipc_proc_nr();
        x86_ipc_dispatch_body(frame, cur_nr);
        return;
    }

    // LAPIC spurious interrupt (C: apic.c apic_spurious_interrupt): no
    // handler chain, no EOI — the spurious vector never enters IRR/ISR.
    if vector == 0xFF {
        return;
    }

    // S-10: scheduler IPI (C SMP_SCHED_IPI_VECTOR) → the target CPU's own
    // sched_handler_full (loads flags, applies STOP/SAVE_CTX/VM_INHIBIT,
    // clears pending — the schedule_sync waiter's completion signal).
    // BKL: try-or-inherit (depth-1 emulation of C's counting lock — the AP
    // may be in its idle window with the BKL released, or interrupted while
    // the kernel held it).
    if vector == minix_arch::x86_64::smp::SCHED_IPI_VECTOR as u8 {
        let acquired = crate::smp::bkl_try_lock();
        {
            let section = unsafe { crate::smp::BklSection::assume_held() };
            let table = crate::proc_table_with(&section);
            let smp = crate::smp_state_with(&section);
            let cpu = crate::current_cpu_id();
            smp.sched_handler_full(table, cpu);
        }
        if acquired {
            crate::smp::bkl_unlock();
        }
        return;
    }

    // LAPIC local timer (edge1 K6): the per-CPU one-shot tick never
    // enters the hook chains — its handler re-arms the ICR and services
    // per-CPU work directly (C: `lapic_timer_int_handler`, apic.c:913,
    // registered straight in the IDT, with its own EOI). The local-tick
    // body owns the EOI via the arch `local_timer_eoi`.
    //
    // K6-v2 quantum enforcement: after the re-arm + EOI, evaluate
    // quantum expiry for this CPU's running process (C: clock_handler
    // per-CPU arm, proc.c:418-424 — if the quantum is exhausted, call
    // sched_proc_no_time to notify the user scheduler or transition to
    // the kernel-scheduled renewal path). The SSI trap entry cleared
    // SIE; the sret after the handler restores it (SPIE=1), so the
    // preempted process's instruction stream resumes without disruption.
    if vector == 0xF1 {
        // C: every clock/IRQ stub in mpx.S runs SAVE_PROCESS_CTX at entry
        // before any policy code (mpx.S:77/137/355/540) — do it before the
        // quantum check so a preempt-and-wake dispatch never reads a stale
        // register file (NK4-A Task C).
        unsafe { save_irq_frame_to_context(frame) };

        // NK4-A C-3 迭代8 取证（一次性）：LAPIC tick 存活证明——RS 用户态
        // 裸转期间若 tick 从未触发，则无抢占、自旋独占 CPU。
        #[cfg(not(feature = "mock"))]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static TICK1: AtomicUsize = AtomicUsize::new(0);
            if TICK1.fetch_add(1, AtomicOrd::Relaxed) == 0 {
                use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                Console::write_str("nk4a: tick1\n");
            }
        }
        crate::clock::local_tick(crate::current_cpu_id());
        // Quantum enforcement (C proc.c:418-424): the tick CPU evaluates
        // its own running process — if the quantum is exhausted, the
        // process is preempted (NO_QUANTUM set, scheduler notified).
        let section = unsafe { crate::smp::BklSection::assume_held() };
        let cur_nr = {
            let smp = crate::smp_state_with(&section);
            smp.cpu_local(crate::current_cpu_id())
                .and_then(|l| l.proc_ptr)
        };
        if let Some(nr) = cur_nr {
            let table = crate::proc_table_with(&section);
            let priv_table = crate::priv_table_with(&section);
            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                table.check_quantum(nr, priv_table, &section);
            }
        }
        return;
    }

    if let Some(irq) = minix_arch::x86_64::trap_stub::irq_of_vector(vector) {
        // C parity: like the clock stub, every IRQ stub in mpx.S saves the
        // full interrupted user context at entry (SAVE_PROCESS_CTX,
        // sconst.h:75-89) before irq_int's policy code — the PIT clock line
        // lands here, so the save must happen before anything that can lead
        // to a quantum/preemption decision (NK4-A Task C).
        unsafe { save_irq_frame_to_context(frame) };
        // Profile-clock PC handoff (C reads p->p_reg.pc, which the asm
        // entry saved into the process context — the save above mirrors
        // that; the rip still travels to `profile_clock_hook` through the
        // one-shot slot so the hook needs no table access). The
        // SPROFILING guard keeps a line fire outside an active profiling
        // run from leaving a stale value in the slot.
        if irq == minix_plat::PROFILE_CLOCK_IRQ.get()
            && crate::misc::SPROFILING.load(core::sync::atomic::Ordering::Acquire)
        {
            crate::misc::stash_profile_pc(frame.rip);
        }
        // D-46 hardware half-loop: ack → mask → hook chain → unmask → eoi,
        // under the interrupted context's BKL (C: IRQ handlers run with the
        // BKL owned by the interrupted context; no lock is taken here).
        // Spurious lines are logged-and-dropped (C irq_handle's spurious
        // path); an invalid IRQ line is a trap-entry wiring bug.
        match crate::irq_manager::dispatch_hardware_irq(minix_plat::IrqVector::new(irq)) {
            Ok(()) => {}
            Err(crate::irq_manager::IrqError::Spurious(line)) => {
                use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
                Console::write_str("spurious irq ");
                Console::write_hex(line.get() as u64);
                Console::write_str("\n");
            }
            Err(e) => panic!("trap_dispatch: IRQ dispatch error {e:?} (vector {vector:#04x})"),
        }
        return;
    }

    // Exception / user soft-int gate → arch-generic dispatcher.
    //
    // S-9 (D-38①) — BKL ownership at the exception entry, per origin:
    // - **Kernel-origin** (is_user = false): the interrupted context owns
    //   the BKL, so the body INHERITS it via `assume_held` (C parity: trap
    //   handlers run with the BKL owned by the interrupted context; trap.c
    //   takes no lock — acquiring here would deadlock on the non-reentrant
    //   CAS lock for the normal kernel-origin case).
    // - **User-origin** (is_user = true): user code runs WITHOUT the BKL
    //   (`finish_and_restore` releases it before the context restore —
    //   C: context_stop must_bkl_unlock, arch_clock.c:226-233), so the
    //   entry ACQUIRES it (C: trap entry BKL_LOCK from ring 3). Every
    //   acting arm below (page-fault forwarding, signal) mutates shared
    //   tables and must run under the lock; the scheduling loop reuses the
    //   held-BKL convention on exit.
    let mut exc = exception_frame_of(frame);
    let is_user = X86_64ExceptionFrame::is_user_mode(&exc);
    let section = if is_user {
        crate::smp::bkl_lock_section()
    } else {
        unsafe { crate::smp::BklSection::assume_held() }
    };
    #[cfg(not(feature = "mock"))]
    nk4a_stage_mark!("pfm1-gate");
    // Per-CPU current process (C: `saved_proc = get_cpulocal_var(proc_ptr)`
    // — exception.c:186). User-origin outcomes act on it; the kernel-origin
    // path needs it too for the nested-debug legitimacy check (C reads
    // saved_proc->p_reg.psw and p_kern_trap_style unconditionally —
    // exception.c:232-234). The read follows the same dispatch-anchor
    // convention as the SYSCALL body (proc_ptr is read as the anchor
    // itself). User origin + None = no scheduler step ever ran on this CPU,
    // so no CPL3 code could have faulted — wiring bug, same alarm as the
    // SYSCALL arm. Kernel origin tolerates None (early-boot faults have no
    // saved process, C: "no saved_proc yet" — exception.c:173).
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let proc_ptr = smp.cpu_local(cpu).and_then(|l| l.proc_ptr);
    let cur_nr = if is_user {
        Some(proc_ptr.unwrap_or_else(|| {
            panic!(
                "user exception before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        }))
    } else {
        proc_ptr
    };
    // is_vm feeds the page-fault classification (C: `pr->p_endpoint ==
    // VM_PROC_NR` — exception.c:101; VM's own faults cannot be forwarded
    // to VM itself). Kernel-origin faults never reach that check (the
    // nested path panics first), matching C's is_nested ordering.
    let is_vm = cur_nr == Some(crate::proc::proc_nr::VM_PROC_NR);
    #[cfg(not(feature = "mock"))]
    nk4a_stage_mark!("pfm2-smp");
    // Nested-debug legitimacy inputs, read from the saved process state the
    // way C does (exception.c:232-234): the trace bit comes from the saved
    // PSW (`p_reg.psw & TRACEBIT`, archconst.h:120), the entry style from
    // the per-process record (`p_seg.p_kern_trap_style`). A kernel-origin
    // fault with no saved process is not a traced-process debug trap.
    let (is_traced, kern_trap_style) = cur_nr
        .and_then(|nr| {
            crate::proc_table_with(&section).get(nr).map(|p| {
                (
                    (minix_arch::x86_64::trap_stub::saved_psw(&p.cpu_context) & TRACEBIT) != 0,
                    p.trap_style,
                )
            })
        })
        .unwrap_or((false, TrapStyle::NoEntry));
    #[cfg(not(feature = "mock"))]
    nk4a_stage_mark!("pfm3-tbl");
    // The kernel copy paths are validate-first (user-buffer range checks in
    // ipc.rs, Direct Map window checks in vm.rs), so no FaultContext slot is
    // maintained: C's `catch_pagefaults` + context-tracking replacement
    // (doc 14 §3.4) has no producer in this kernel — a fault inside a
    // recoverable copy cannot occur by construction. `FaultContext::Normal`
    // is therefore the truthful context, not a placeholder.
    let outcome = ExceptionDispatcher::<X86_64ExceptionFrame>::handle(
        &mut exc,
        /* is_nested = */ !is_user,
        is_vm,
        FaultContext::Normal,
        is_traced,
        kern_trap_style,
    );
    #[cfg(not(feature = "mock"))]
    nk4a_stage_mark!("pfm4-hand");

    match outcome {
        // Spurious NMI — C prints and returns (exception.c:191-194); the
        // stub's iretq resumes the interrupted context. No state mutated.
        ExceptionOutcome::SpuriousNmi => {
            use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
            Console::write_str("got spurious NMI\n");
        }
        // Traced process entered the kernel (int-gate legs keep TF set) and
        // single-stepped the first kernel instruction before the entry
        // recorded a style. C clears the flag in the frame and resumes
        // (exception.c:243-245); identical here — the stub's iretq delivers
        // the corrected RFLAGS. (The `syscall` entry masks TF in hardware,
        // so on this port only the int-33 gate leg can produce the case.)
        ExceptionOutcome::ClearTrapFlag => {
            frame.rflags &= !TRACEBIT;
        }
        // C recovers copy faults by redirecting RIP into recovery labels
        // (exception.c:206-230, klib.S) — a mechanism this kernel replaces
        // with validate-first copy paths (ipc.rs `user_copy_range_mapped`,
        // vm.rs `physical_range_in_dm_window`): bad caller buffers produce
        // typed Err values and never fault. Reaching these outcomes means a
        // fault escaped validation — an invariant break (kernel bug), not
        // recoverable state. C's own fallback for unrecognized kernel
        // faults is inkernel_disaster (exception.c:280-282).
        ExceptionOutcome::RedirectToRecovery(rp) => panic!(
            "trap_dispatch: RedirectToRecovery({rp:?}) — kernel copy paths \
             are validate-first; a fault here escaped validation (kernel bug)"
        ),
        ExceptionOutcome::PhysCopyFault { fault_addr } => panic!(
            "trap_dispatch: PhysCopyFault at {:#x} — Direct Map window \
             validation escaped a fault (kernel bug)",
            fault_addr.0
        ),
        ExceptionOutcome::KernelPanic(v) => {
            // Console diagnostics before dying: the panic handler cannot
            // render the formatted message (no fmt on early console).
            use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
            Console::write_str("trap: vector ");
            Console::write_hex(v.get() as u64);
            Console::write_str(" err ");
            Console::write_hex(frame.errcode);
            Console::write_str(" rip ");
            Console::write_hex(frame.rip);
            Console::write_str(" cs ");
            Console::write_hex(frame.cs);
            Console::write_str(" rflags ");
            Console::write_hex(frame.rflags);
            Console::write_str(" rsp ");
            Console::write_hex(frame.rsp);
            Console::write_str(" ss ");
            Console::write_hex(frame.ss);
            Console::write_str("\n");
            #[cfg(not(feature = "mock"))]
            nk4a_stage_mark!("pfm5-dump");
            panic!(
                "kernel exception vector {} at rip {:#x} errcode {:#x} [dispatch_body @ 0x{:x}]",
                v.get(),
                frame.rip,
                frame.errcode,
                x86_trap_dispatch_body as *const () as usize
            );
        }
        ExceptionOutcome::VmPageFault => {
            // VM's own user page fault — forwarding to VM would deadlock
            // on itself. C: pagefault() prints the frame summary and
            // panics ("pagefault in VM") — exception.c:101-118.
            use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
            Console::write_str("pagefault for VM on cpu ");
            Console::write_hex(crate::current_cpu_id().raw() as u64);
            Console::write_str(" rip ");
            Console::write_hex(frame.rip);
            // NK4-A 首亮取证（临时）：CR2 = 真实缺页地址。rip 只能说明
            // 执行到了哪，判定"哪个 VA 没映射"必须看 cr2。
            let cr2: u64;
            unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)); }
            Console::write_str(" cr2 ");
            Console::write_hex(cr2);
            Console::write_str(" err ");
            Console::write_hex(frame.errcode);
            Console::write_str("\n");
            panic!("pagefault in VM");
        }
        ExceptionOutcome::ForwardToVm(pf) => {
            let cur_nr = cur_nr.expect("ForwardToVm implies a user-origin fault");
            // Persist the user register file BEFORE any dispatch side
            // effect (design decision 3, same shape as the int-33 IPC
            // arm) — VM resolves the fault while the process is parked on
            // RTS_PAGEFAULT, and the resume must re-execute the faulting
            // instruction against this saved state (C: mpx.S
            // SAVE_PROCESS_CTX already ran by the time exception_handler
            // dispatches).
            {
                let table = crate::proc_table_with(&section);
                let proc = table
                    .get_mut(cur_nr)
                    .unwrap_or_else(|| panic!("pagefault from invalid proc nr {cur_nr:?}"));
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                // NK4-C 第 11 轮金丝雀已于第 31 轮摘除：该探针在 rbx==0 时
                // 改写保存上下文（write_user_register 注入
                // 0xDEAD_BEEF_CAFE_0001），真机 c30b 实证它把哨兵当 RBX
                // 恢复交付给 RS——探针本身成了上下文破坏源，干扰
                // self=0 根因取证。仅保留被动观察探针。
                // NK4-A Task C 第 5 轮判别（task1-close 裁决删除）：pf 转发
                // 入口「保存即采到的 RBX」——与 lib.rs pre-restore 路标的
                // `rbx=`（同一进程本次恢复交付值）对照，三分支裁决：两处同
                // 为 0 → 保存即错（故障点用户 RBX 已是 0）；保存非 0 而恢复
                // 0 → 保存后被内核改写（RBX 的 IPC 状态寄存器语义踩掉用户
                // callee-saved 活值）；两处非 0 而用户仍崩 → 恢复读错源。
                // 上限 8 在 c23a 于启动前 8 次内耗尽（崩溃在第 100+ 次），
                // 第 6 轮提到 48，仅真机。
                // S2h 评审修复（task1-close 裁决删除）：pw-pf 移出 PFRBX 的
                // 48 配额块——旧挂法被全进程 PF 配额在启动期吃光，崩溃
                // 窗口内必然失明（假阴性）；watch 自身只在变化时打印，
                // 无需外部配额。
                #[cfg(not(feature = "mock"))]
                if proc.p_endpoint.0 == 2 {
                    nk4a_pte_watch("pf", proc.p_seg.phys_root.0);
                }
                #[cfg(not(feature = "mock"))]
                {
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    static PFRBX: AtomicUsize = AtomicUsize::new(0);
                    if PFRBX.fetch_add(1, AtomicOrd::Relaxed) < 48 {
                        use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                        let cr2: u64;
                        // SAFETY: reading CR2 has no side effects.
                        unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)) };
                        C0::write_str("nk4a: pf-save ep=");
                        C0::write_hex(proc.p_endpoint.0 as u64);
                        C0::write_str(" rbx=");
                        C0::write_hex(frame.rbx);
                        C0::write_str(" rip=");
                        C0::write_hex(frame.rip);
                        C0::write_str(" cr2=");
                        C0::write_hex(cr2);
                        // NK4-C 第 12 轮：RS 全现场——wrapper 的栈写目标
                        // （rsp/r8=slot/rdi/rsi）与 r10 状态车道。
                        if proc.p_endpoint.0 == 2 {
                            // 第 18 轮：RS 故障 VA 的四级页表层级逐级 dump
                            // （PML4E/PDPTE/PDE/PTE present 位）——直接
                            // 定位映射在哪一级丢失。走 DM 读物理页表页。
                            {
                                use minix_arch::DirectMapArch as _;
                                let c2 = cr2;
                                let i4 = (c2 >> 39) & 0x1FF;
                                let i3 = (c2 >> 30) & 0x1FF;
                                let i2 = (c2 >> 21) & 0x1FF;
                                let i1 = (c2 >> 12) & 0x1FF;
                                let root_pa = crate::proc_table_with(&section)
                                    .get(cur_nr)
                                    .map(|p| p.p_seg.phys_root.0)
                                    .unwrap_or(0);
                                let rd = |pa: u64| -> u64 {
                                    let va = <minix_arch::CurrentDirectMap as minix_arch::DirectMapArch>::kernel_phys_to_virt(minix_types::PhysBytes(pa));
                                    unsafe { core::ptr::read(va.0 as *const u64) }
                                };
                                let pml4e = rd(root_pa + i4 * 8);
                                let mut l3pa = 0u64;
                                if pml4e & 1 != 0 {
                                    l3pa = pml4e & 0x000F_FFFF_FFFF_F000;
                                }
                                let pdpte = if l3pa != 0 { rd(l3pa + i3 * 8) } else { 0 };
                                let mut l2pa = 0u64;
                                if pdpte & 1 != 0 {
                                    l2pa = pdpte & 0x000F_FFFF_FFFF_F000;
                                }
                                let pde = if l2pa != 0 { rd(l2pa + i2 * 8) } else { 0 };
                                let mut l1pa = 0u64;
                                if pde & 1 != 0 && pde & 0x80 == 0 {
                                    l1pa = pde & 0x000F_FFFF_FFFF_F000;
                                }
                                let pte = if l1pa != 0 { rd(l1pa + i1 * 8) } else { 0 };
                                C0::write_str(" lvl4=");
                                C0::write_hex(pml4e);
                                C0::write_str(" lvl3=");
                                C0::write_hex(pdpte);
                                C0::write_str(" lvl2=");
                                C0::write_hex(pde);
                                C0::write_str(" lvl1=");
                                C0::write_hex(pte);
                                C0::write_str(" lvl2pa=");
                                C0::write_hex(l2pa);
                                C0::write_str(" lvl1pa=");
                                C0::write_hex(l1pa);
                            }
                            C0::write_str(" rsp=");
                            C0::write_hex(frame.rsp);
                            C0::write_str(" rax=");
                            C0::write_hex(frame.rax);
                            C0::write_str(" rcx=");
                            C0::write_hex(frame.rcx);
                            C0::write_str(" rdx=");
                            C0::write_hex(frame.rdx);
                            C0::write_str(" r8=");
                            C0::write_hex(frame.r8);
                            C0::write_str(" rdi=");
                            C0::write_hex(frame.rdi);
                            C0::write_str(" rsi=");
                            C0::write_hex(frame.rsi);
                            C0::write_str(" rbp=");
                            C0::write_hex(frame.rbp);
                            C0::write_str(" r10=");
                            C0::write_hex(frame.r10);
                            C0::write_str(" err=");
                            C0::write_hex(frame.errcode);
                            // 第 13 轮：memset(0x227af0..0x227b40) 无 prologue
                            // 压栈，[rsp] 即调用者返回地址。
                            if frame.rip >= 0x227_000 && frame.rip < 0x228_000 {
                                // memset 无 prologue 压栈：[用户 rsp] 即调用
                                // 者返回地址——经 RS 页表翻译后走 DM 读。
                                let root = crate::proc_table_with(&section)
                                    .get(cur_nr)
                                    .map(|p| p.p_seg.phys_root);
                                if let Some(root) = root {
                                    match crate::pte_walk::walk_x86_64(
                                        root,
                                        minix_types::VirBytes(frame.rsp),
                                    ) {
                                        Some((pa, _)) => {
                                            use minix_arch::DirectMapArch as _;
                                            let va = <minix_arch::CurrentDirectMap as minix_arch::DirectMapArch>::kernel_phys_to_virt(minix_types::PhysBytes(pa.0));
                                            let retaddr = unsafe {
                                                core::ptr::read(va.0 as *const u64)
                                            };
                                            C0::write_str(" ret=");
                                            C0::write_hex(retaddr);
                                        }
                                        None => C0::write_str(" ret=?"),
                                    }
                                }
                            }
                        }
                        C0::write_str("\n");
                    }
                }
                #[cfg(not(feature = "mock"))]
                {
                    nk4a_rs_trace_probe("pf2", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                    nk4a_rs_anom_probe("pf2", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                    nk4a_rs_leak_probe("pf2", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                    // 第 16 轮（task1-close 裁决删除）：VA 重复故障检测——
                    // 同一页第 2+ 次故障意味着「填充后内容又丢了」（页被
                    // 二次清零/重映射），直接打印次数与 rbx 现场。
                    #[cfg(target_arch = "x86_64")]
                    if proc.p_endpoint.0 == 2 {
                        use core::sync::atomic::{
                            AtomicU64, AtomicUsize, Ordering as AtomicOrd,
                        };
                        let cr2: u64;
                        // SAFETY: reading CR2 has no side effects.
                        unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)) };
                        static RE_VA: [AtomicU64; 32] =
                            [const { AtomicU64::new(0) }; 32];
                        static RE_N: [AtomicUsize; 32] =
                            [const { AtomicUsize::new(0) }; 32];
                        let mut slot = 32;
                        let mut i = 0;
                        while i < 32 {
                            if RE_VA[i].load(AtomicOrd::Relaxed) == cr2 {
                                slot = i;
                                break;
                            }
                            if RE_VA[i].load(AtomicOrd::Relaxed) == 0 && slot == 32 {
                                slot = i;
                            }
                            i += 1;
                        }
                        if slot < 32 {
                            let k = RE_N[slot].fetch_add(1, AtomicOrd::Relaxed);
                            if k >= 1 && k <= 2 {
                                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                                C0::write_str("nk4a: pf-refault va=");
                                C0::write_hex(cr2);
                                C0::write_str(" k=");
                                C0::write_hex(k as u64);
                                C0::write_str(" rbx=");
                                C0::write_hex(frame.rbx);
                                C0::write_str(" rip=");
                                C0::write_hex(frame.rip);
                                C0::write_str("\n");
                            }
                        }
                    }
                }
                // C 对位：异常入口与 trap 入口同记返回样式（mpx.S 保存半
                // p_kern_trap_style=KTS_FULLCONTEXT；restore 消费）。缺它则
                // 转发后重入队的进程在下次 dispatch 撞 "no entry trap style
                // known"（NK4-A C-3 真机 2026-09-22：RS fault 填充恢复后）。
                proc.trap_style = minix_arch::TrapStyle::FullContext;
            }
            // C: pagefault() tail — RTS_PAGEFAULT, VM_PAGEFAULT message,
            // FROM_KERNEL mini_send (exception.c:112-129). Send errors
            // panic here (C: panic "WARNING: pagefault: mini_send
            // returned %d").
            if let Err(e) = forward_pagefault_to_vm(
                crate::proc_table_with(&section),
                crate::priv_table_with(&section),
                cur_nr,
                pf.vaddr.0,
                frame.errcode as u32,
            ) {
                panic!("pagefault: mini_send returned {e:?}");
            }
            // C: pagefault() returns → mpx.S `jmp switch_to_user`. The
            // process stays non-runnable (RTS_PAGEFAULT) until VM's
            // SYS_VMCTL ClearPageFault re-enqueues it (do_vmctl.c:35);
            // the scheduling loop picks whoever is runnable next.
            crate::scheduler_loop(crate::current_cpu_id())
        }
        ExceptionOutcome::Signal(sig) => {
            let cur_nr = cur_nr.expect("Signal implies a user-origin exception");
            // Console diagnostics for the signal-classified exception —
            // same rationale as the KernelPanic arm's frame dump: which
            // vector became which signal is not otherwise observable on
            // real machine (the panic handler cannot render it).
            {
                use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
                Console::write_str("user exception: vector ");
                Console::write_hex(vector as u64);
                Console::write_str(" err ");
                Console::write_hex(frame.errcode);
                Console::write_str(" rip ");
                Console::write_hex(frame.rip);
                Console::write_str(" rsp ");
                Console::write_hex(frame.rsp);
                Console::write_str(" cs ");
                Console::write_hex(frame.cs);
                Console::write_str(" rflags ");
                Console::write_hex(frame.rflags);
                Console::write_str("\n");
                // NK4-A C-3 迭代5 取证（task1-close 裁决删除）：#GP 时直读
                // rip 处 8 字节（走当前 root 的 PTE + DM 窗口）——对照 ELF
                // 字节判别"特权指令执行态"还是"页内容/视图不一致"。RS 入口
                // ELF 字节应为 48 83 e4 f0（and rsp,-0x10）。
                if vector == 13 {
                    if let Some(root) = crate::current_root_phys() {
                        match crate::pte_walk::walk_x86_64(
                            root,
                            minix_types::VirBytes(frame.rip),
                        ) {
                            Some((pa, fl)) => {
                                Console::write_str("nk4a: gp-byte@0x");
                                Console::write_hex(pa.0);
                                Console::write_str(" fl=0x");
                                Console::write_hex(fl.bits() as u64);
                                Console::write_str(" bytes=");
                                use minix_arch::DirectMapArch;
                                let base = <minix_arch::CurrentDirectMap as DirectMapArch>::kernel_phys_to_virt(pa).0;
                                for i in 0..8u64 {
                                    let b = unsafe {
                                        ((base + i) as *const u8).read_volatile()
                                    };
                                    Console::write_hex(b as u64);
                                }
                                Console::write_str("\n");
                            }
                            None => {
                                Console::write_str("nk4a: gp rip unmapped\n");
                            }
                        }
                    }
                }
            }
            // Persist first: the signal manager's later SIGSEND delivery
            // builds the handler trampoline on top of the fault-time
            // register file (C: sig_proc reshapes p_reg, which holds the
            // trap-saved state).
            {
                let table = crate::proc_table_with(&section);
                let proc = table
                    .get_mut(cur_nr)
                    .unwrap_or_else(|| panic!("exception from invalid proc nr {cur_nr:?}"));
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                // NK4-B P1 第 7 轮（task1-close 裁决删除）：本臂与下面的
                // VmSuspend 臂是第 6 轮静态穷举发现、当时未取证的两个存帧
                // 站点（NK4B-WORKLOG T0.4）。
                #[cfg(not(feature = "mock"))]
                {
                    nk4a_rs_trace_probe("sig", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                    nk4a_rs_anom_probe("sig", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                    nk4a_rs_leak_probe("sig", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                }
                // 同 ForwardToVm 臂：异常入口记返回样式（C mpx.S 对位），
                // 信号暂停后的下次 dispatch 恢复需要它。
                proc.trap_style = minix_arch::TrapStyle::FullContext;
            }
            // C: cause_sig(proc_nr(saved_proc), ep->signum) —
            // exception.c:276. RTS_SIGNALED parks the process until its
            // signal manager resolves the delivery (kernel-side behavior
            // is complete at cause_sig).
            let sig_nr = exception_signal_to_nr(sig);
            crate::syscall_signal::cause_signal(
                cur_nr,
                sig_nr,
                crate::proc_table_with(&section),
                crate::priv_table_with(&section),
            );
            // C: cause_sig returns → mpx.S `jmp switch_to_user`.
            crate::scheduler_loop(crate::current_cpu_id())
        }
        other => {
            // The only remaining outcome: user-mode #NM (vector 7) — the
            // lazy-FPU restore stage (save owner / restore self / clts).
            // Its acting stage lands with the FPU ownership wiring
            // (X-8 / T5 wave, NK2-A ①); reaching it before that wave is a
            // wiring gap, not recoverable state. Every other outcome has an
            // acting arm above. Console diagnostics first: the panic
            // handler cannot render the formatted message.
            use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
            Console::write_str("trap: vector ");
            Console::write_hex(vector as u64);
            Console::write_str(" err ");
            Console::write_hex(frame.errcode);
            Console::write_str(" rip ");
            Console::write_hex(frame.rip);
            Console::write_str(" cs ");
            Console::write_hex(frame.cs);
            Console::write_str(" rflags ");
            Console::write_hex(frame.rflags);
            Console::write_str(" rsp ");
            Console::write_hex(frame.rsp);
            Console::write_str(" ss ");
            Console::write_hex(frame.ss);
            Console::write_str("\n");
            panic!(
                "trap_dispatch: outcome {other:?} (FpuTrap) awaits the \
                 lazy-FPU restore stage (registered gap, T5 wave) — \
                 vector {vector:#04x} rip {:#x}",
                frame.rip
            )
        }
    }
}

#[cfg(target_arch = "x86_64")]
/// Map the arch-generic exception classification to the kernel signal
/// number (C: the `ex_data[].signum` column — exception.c:19-39; constants
/// signal.h:55-63).
fn exception_signal_to_nr(sig: ExceptionSignal) -> u32 {
    match sig {
        ExceptionSignal::Fpe => crate::syscall_signal::SIGFPE,
        ExceptionSignal::Ill => crate::syscall_signal::SIGILL,
        ExceptionSignal::Segv => crate::syscall_signal::SIGSEGV,
        ExceptionSignal::Bus => crate::syscall_signal::SIGBUS,
        ExceptionSignal::Emt => crate::syscall_signal::SIGEMT,
        ExceptionSignal::Trap => crate::syscall_signal::SIGTRAP,
    }
}

/// The page-fault forwarding arm's acting half — C: `pagefault()` tail,
/// exception.c:112-129.
///
/// 1. `RTS_SET(pr, RTS_PAGEFAULT)` (exception.c:115) — via the
///    scheduler-aware `rts_set`, which is the RTS_SET macro's dequeue
///    half; the primitive flag set in `page_fault::set_pagefault_pending`
///    alone would leave the process queued while non-runnable
///    (pick_proc returns queue heads without a runnable re-check).
/// 2. Build the `VM_PAGEFAULT` message (m_source = faulting endpoint,
///    VPF_ADDR = fault address, VPF_FLAGS = raw error code —
///    exception.c:118-122) and `mini_send` it to VM FROM_KERNEL
///    (exception.c:123-125).
///
/// Returns `Err(errno)` when the send fails; the caller panics (C:
/// `panic("WARNING: pagefault: mini_send returned %d")`).
fn forward_pagefault_to_vm(
    proc_table: &mut crate::proc_table::ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    cur_nr: crate::proc::ProcNr,
    fault_addr: u64,
    error_code: u32,
) -> Result<crate::ipc::IpcOutcome, crate::ipc::IpcError> {
    use crate::ipc::{IpcEngine, KernelUserCopy, SendFlags};
    use crate::proc::RtsFlagsBits;

    proc_table.rts_set(cur_nr, RtsFlagsBits::PAGEFAULT);
    // Rust-side diagnostic record (the field `set_pagefault_pending`
    // maintains; C keeps no per-process fault address — the address
    // travels in the message alone).
    if let Some(p) = proc_table.get_mut(cur_nr) {
        p.p_fault_addr = Some(fault_addr);
    }
    let src_endpoint = proc_table
        .get(cur_nr)
        .map(|p| p.p_endpoint)
        .unwrap_or(minix_types::Endpoint::NONE);
    let msg = crate::page_fault::build_vm_pagefault_msg(src_endpoint, fault_addr, error_code);
    // Send target: the VM process's own endpoint (C: mini_send(pr,
    // VM_PROC_NR, ...) resolves the slot through the table; a dead VM
    // slot is the EDEADSRCDST path).
    let dst_endpoint = proc_table
        .get(crate::proc::proc_nr::VM_PROC_NR)
        .map(|p| p.p_endpoint)
        .ok_or(crate::ipc::IpcError::DeadSrcDst)?;
    let mut engine = IpcEngine::new(proc_table.procs_slice_mut(), priv_table, &KernelUserCopy);
    let outcome = engine.send(cur_nr, dst_endpoint, &msg, SendFlags::FROM_KERNEL);
    // C: RTS_UNSET(dst, RTS_RECEIVING) 的入队半——引擎侧只清 RTS 标志
    // （primitive setter，见 ipc.rs 投递臂），入队必须由调用方补齐
    // （syscall.rs dispatch_ipc_entry 的 take_wake_target →
    // enqueue_if_woken 同款）。漏掉这半：VM 被解锁但从未入就绪队列，
    // 调度器无人可选 → idle 停机（NK4-A C-3 真机：fwd Delivered 后全静默，
    // 2026-09-22）。
    let woken_target = engine.take_wake_target();
    if let Some(woken_nr) = woken_target {
        proc_table.enqueue_if_woken(woken_nr);
    }
    // 锚点括值：本函数运行时地址（一次性，供 #GP rip 离线定位）。
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static ANCHOR: AtomicUsize = AtomicUsize::new(0);
        if ANCHOR.fetch_add(1, AtomicOrd::Relaxed) == 0 {
            use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
            Console::write_str("nk4a: anchor fwd=0x");
            Console::write_hex(forward_pagefault_to_vm as *const () as u64);
            Console::write_str(" finish=0x");
            Console::write_hex(crate::finish_and_restore_addr());
            Console::write_str(" sched=0x");
            Console::write_hex(crate::scheduler_loop_addr());
            Console::write_str("\n");
        }
    }
    match outcome {
        crate::ipc::IpcOutcome::Error(e) => Err(e),
        delivered_or_blocked => Ok(delivered_or_blocked),
    }
}

#[cfg(target_arch = "x86_64")]
/// E1 trap bridge — locate the current user process for the vector-33 arm.
/// Mirrors the SYSCALL body's per-CPU anchor read (proc_ptr before any lock).
fn current_ipc_proc_nr() -> crate::proc::ProcNr {
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    smp.cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "int-33 IPC before scheduler bring-up (proc_ptr = None on                  cpu {cpu:?}) — wiring bug"
            )
        })
}

#[cfg(target_arch = "x86_64")]
/// E1 trap bridge body: vector-33 IPC leg.
///
/// Contract (design doc 18, decisions 1-3):
/// - registers: RAX = src/dst endpoint (SENDA: count), RBX = message
///   pointer (SENDA: table pointer), RCX = IPC call number;
/// - the interrupted register file is persisted into the caller's saved
///   context BEFORE dispatch, so delivery-side IPC-status ORs land in the
///   same state the scheduler restores;
/// - a reply-code outcome returns through the stub (errno in RAX, status
///   RBX synced back from the context); a NoReply (Blocked) outcome never
///   returns — the BKL released by `kernel_call_finish` is re-acquired and
///   the CPU enters the scheduling loop, which restores whoever is
///   runnable (the blocked caller resumes only when its IPC completes).
///
/// # Safety
///
/// Same contract as `x86_trap_dispatch_body`: `frame` points at the
/// TrapFrame built by the asm stub on this CPU's kernel stack, and the
/// per-CPU `proc_ptr` anchor names the interrupted process.
unsafe fn x86_ipc_dispatch_body(frame: &mut TrapFrame, cur_nr: crate::proc::ProcNr) {
    let table = unsafe { crate::proc_table_boot_unchecked() };

    // Decision 3: persist the user register file before any dispatch side
    // effect — delivery paths OR IPC status into this saved context.
    {
        let caller = table
            .get_mut(cur_nr)
            .unwrap_or_else(|| panic!("int-33 IPC from invalid proc nr {cur_nr:?}"));
        minix_arch::save_frame_to_context(frame, &mut caller.cpu_context);
        #[cfg(not(feature = "mock"))]
        nk4a_rbx_probe(
            "int33-save",
            caller.p_endpoint.0 as u64,
            frame.rbx,
            frame.rip,
        );
        #[cfg(not(feature = "mock"))]
        {
            nk4a_rs_trace_probe("x33", caller.p_endpoint.0 as u64, frame.rbx, frame.rip);
            nk4a_rs_anom_probe("x33", caller.p_endpoint.0 as u64, frame.rbx, frame.rip);
            nk4a_rs_leak_probe("x33", caller.p_endpoint.0 as u64, frame.rbx, frame.rip);
            // NK4-C S2c 哨兵（task1-close 裁决删除）：RS 进内核代执行入口
            // 重读监视 PTE；与 kernel_call_finish 的出口观测夹住整个 IPC
            // 服务窗口。LAST 状态全局，必须按 ep==2 门控避免错 root 串扰。
            if caller.p_endpoint.0 == 2 {
                nk4a_pte_watch("x33", caller.p_seg.phys_root.0);
            }
        }
        // Record the ENTRY style (C: every mpx.S soft-int entry records
        // `p_kern_trap_style`; arch_system.c:585 consumes it at
        // restore_user_context). A door-parked caller (blocked IPC)
        // resumes through finish_and_restore's entry-style gate — without
        // a fresh record the wake dispatch finds NoEntry and panics
        // ("no entry trap style known", arch_system.c:597-598; observed
        // on real machine waking a parked receiver, test-sysboot C-27).
        // The int-33 gate pushes a full frame, so the style is
        // FullContext — same record the carrier seeds for the first run.
        caller.trap_style = TrapStyle::FullContext;
    }

    // K20 (caller-by-nr): no laundering — the caller travels as its nr and
    // every access re-borrows the slot from `table` at the point of use.

    // Pre-decode: unknown call numbers exit before the BKL is taken —
    // dispatch_ipc_entry's own decode-fail return path also skips the BKL
    // (EBADCALL without acquiring), so kernel_call_finish must not run.
    // C: proc.c:602-606 — do_ipc's default branch, same effect.
    let call_nr = frame.rcx as i32;
    // C-3 迭代10 取证：int-33 IPC 入口（限 16）。
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static IPC_ENTRY: AtomicUsize = AtomicUsize::new(0);
        let n = IPC_ENTRY.fetch_add(1, AtomicOrd::Relaxed);
        if n < 16 {
            use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
            Console::write_str("nk4a: ipc-entry nr=");
            Console::write_hex(call_nr as u64);
            Console::write_str(" caller=");
            Console::write_hex(cur_nr.0 as u64);
            Console::write_str("\n");
        }
    }
    if crate::ipc::IpcCall::from_raw(call_nr).is_none() {
        frame.rax = crate::errno::EBADCALL as i64 as u64;
        return;
    }

    // Register extraction (design decision 2).
    let call_nr = frame.rcx as i32;
    let r1 = frame.rax; // src/dst endpoint, or SENDA count
    let r2 = frame.rbx; // message pointer, or SENDA table pointer
    let is_senda = call_nr == (crate::ipc::IpcCall::SendA as i32);
    // Body-less calls: the stubs trap with rbx = 0 (no message buffer) —
    // C `ipc_minix_kerninfo.S:8-10` zeroes eax/ebx before `int`, and the
    // notify stub passes 0 the same way ("notify without a message body").
    // KernInfo's result returns through the secondary RBX channel; the
    // service arm documents "No message buffer is read or written"
    // (syscall.rs). Copying 64 bytes from VA 0 was the SIGSEGV this door
    // delivered to every birth-time kerninfo query on real machine
    // (test-sysboot C-27 carrier, first real-machine birth through this
    // gate).
    let is_bodyless =
        is_senda
            || call_nr == (crate::ipc::IpcCall::KernInfo as i32)
            || call_nr == (crate::ipc::IpcCall::Notify as i32);
    {
        let caller = table
            .get_mut(cur_nr)
            .expect("int-33 IPC: caller slot must exist");
        caller.p_defer.r2 = r1 as usize;
        caller.p_defer.r3 = if is_senda { r2 as usize } else { 0 };
        // C do_sync_ipc's `(message *) r3` — the caller's message buffer
        // reaches the engine through `p_delivermsg_vir` (C mini_receive
        // stores m_buff_usr there, proc.c:983; the syscall-leg kernel_call
        // stores m_user, system.c:141). The door previously dropped rbx, so
        // a parked receiver's wake delivery copied the message to a stale
        // syscall buffer and the receive buffer kept its pre-call content
        // (test-sysboot C-27: rx read Message::default poison, then its
        // sendnb(m_source) failed with EDEADSRCDST on the garbage source).
        // SENDA carries the table pointer in rbx instead (p_defer.r3 above).
        // NK4-C S2g 现场打印（task1-close 裁决删除）：抹写窗口已钉死
        // 在单次 int33 内核代执行段（s2e），候选抹写者是
        // kernel_call_finish 的 DM 直写。判别 fresh/stale：本次入口的 r2、
        // 调用号、是否 SENDA（SENDA 不存 delivermsg，fx 目标必为旧值）与
        // 旧 p_delivermsg_vir 一并打出（仅 RS，与 fx 记录离线对账）。
        // S2h：移出 !is_senda 块——s2g 零输出本身就是证据（窗口那次
        // int33 疑似 SENDA），加 senda= 字段直接裁决。
        #[cfg(not(feature = "mock"))]
        if caller.p_endpoint.0 == 2 {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static XIN: AtomicUsize = AtomicUsize::new(0);
            // S2h 评审修复：4096→512 + 触顶现形标记（串口是同步 PIO，
            // 最坏量级不能拖穿复跑 timeout）。
            let xn = XIN.fetch_add(1, AtomicOrd::Relaxed);
            if xn == 512 {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                C0::write_str("nk4a: x33in-cap\n");
            }
            if xn < 512 {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                C0::write_str("nk4a: x33in call=");
                C0::write_hex(call_nr as u64);
                C0::write_str(" r2=");
                C0::write_hex(r2);
                C0::write_str(" old=");
                C0::write_hex(caller.p_delivermsg_vir.0);
                C0::write_str(" senda=");
                C0::write_hex(is_senda as u64);
                C0::write_str("\n");
            }
        }
        if !is_senda {
            caller.p_delivermsg_vir = VirBytes(r2);
        }
    }

    // Copy the user message (kernel-side copy, TOCTOU defense — the same
    // shape as kernel_call's own copy). SENDA carries no message buffer:
    // mini_senda reads entries from the user table directly (proc.c:683).
    let mut msg = minix_types::Message::default();
    if !is_bodyless {
        use crate::ipc::UserCopy as _;
        match crate::ipc::KernelUserCopy.copy_msg_from_user(VirBytes(r2)) {
            Ok(m) => msg = m,
            Err(_) => {
                // C system.c:152-155 parity (kernel_call's copy arm):
                // SIGSEGV + EFAULT, without entering IPC dispatch.
                crate::syscall_signal::cause_signal(
                    cur_nr,
                    crate::syscall_signal::SIGSEGV,
                    table,
                    unsafe { crate::priv_table_boot_unchecked() },
                );
                frame.rax = crate::errno::EFAULT as i64 as u64;
                return;
            }
        }
    }
    // NK4-C 第 31 轮判别探针（task1-close 裁决删除）：int-33 入口「保存
    // 即采到的 RBX」——与 pre-restore 的交付 rbx 对照裁决：入口非 0 而
    // 交付 0 ⇒ 停车期间被内核改写；入口即 0 ⇒ stub/帧/用户侧来源。
    #[cfg(not(feature = "mock"))]
    if cur_nr == crate::proc::ProcNr(2) {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static I33N: AtomicUsize = AtomicUsize::new(0);
        if I33N.fetch_add(1, AtomicOrd::Relaxed) < 48 {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: i33-save rip=");
            C0::write_hex(frame.rip);
            C0::write_str(" rbx=");
            C0::write_hex(frame.rbx);
            C0::write_str(" rsp=");
            C0::write_hex(frame.rsp);
            C0::write_str("\n");
        }
    }
    msg.m_type = call_nr;
    msg.m_source = table
        .get(cur_nr)
        .map(|c| c.p_endpoint)
        .expect("int-33 IPC: caller slot must exist");

    // BKL: dispatch_ipc_entry acquires and transfers out (held across
    // kernel_call_finish, which releases it on every non-VmSuspend path).
    // NK4-C S3 门纪律：陷阱腿用 `kernel_call_finish_ipc_door`——C 的
    // proc.c mini_* 从不执行 kernel_call 腿的 eager 回执直写（状态经
    // h_errno/寄存器，真回执经 MF_DELIVERMSG 投递），陈旧
    // p_delivermsg_vir（SENDA 不刷新）因此在结构上不可达。
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    let result = crate::syscall::dispatch_ipc_entry(cur_nr, table, &mut msg, priv_table);
    crate::syscall::kernel_call_finish_ipc_door(cur_nr, table, &msg, result, priv_table);

    // Delivered (reply code): errno rides RAX out through the stub's
    // iretq; the IPC status bits were ORed into the saved context's RBX by
    // the delivery path — pull that register back into the frame so the
    // stub restores the up-to-date value (C: status lives in p_reg.bx).
    // Blocked (NoReply): leave RAX untouched — the caller must not observe
    // a return value; it stays unrunnable until its IPC completes.
    if let Some(code) = result.reply_code() {
        frame.rax = code as i64 as u64;
        let ctx = &table
            .get(cur_nr)
            .expect("int-33 IPC: caller slot must exist")
            .cpu_context;
        minix_arch::sync_status_register_to_frame(ctx, frame);
        #[cfg(not(feature = "mock"))]
        nk4a_rbx_probe("int33-ret", msg.m_source.0 as u64, frame.rbx, frame.rip);
    } else {
        // Enter the scheduling loop; never returns to this frame.
        reenter_scheduler();
    }
}

#[cfg(target_arch = "x86_64")]
/// Re-acquire the BKL (released by `kernel_call_finish`) and enter the
/// scheduling loop — the unified exit for blocked IPC (design decision 3).
fn reenter_scheduler() -> ! {
    // scheduler_loop consumes the held-BKL convention (assume_held witness
    // inside); the guard is forgotten deliberately — ownership passes to
    // the ambient held-BKL scope the same way dispatch_ipc_entry's
    // transfer() hands the lock off.
    let guard = crate::smp::bkl_lock();
    core::mem::forget(guard);
    let cpu = crate::current_cpu_id();
    crate::scheduler_loop(cpu)
}

#[cfg(target_arch = "x86_64")]
/// B-path body: SYSCALL entry → `kernel_call` (trap-1).
///
/// Registered via `minix_arch::register_trap_dispatchers`; reached from
/// `x86_syscall_entry`. Returns the reply code in the frame's `rax`; the
/// stub then restores and executes `sysretq`.
///
/// # Safety
///
/// `frame` points at the TrapFrame built by the LSTAR stub on the per-CPU
/// kernel stack.
pub unsafe extern "C" fn x86_syscall_dispatch_body(frame: &mut TrapFrame) {
    // Per-CPU anchor read before any lock — C parity: `proc_ptr` is read at
    // trap entry without the BKL (it is the dispatch anchor itself). The
    // BKL is acquired inside `kernel_call` → `kernel_call_dispatch`
    // (R-03/R-05 guard transfer) and released by `kernel_call_finish`.
    //
    // Pre-S-6 reality: no scheduler step ever ran, so `proc_ptr` is None and
    // no user code could have executed SYSCALL — panic = wiring bug alarm.
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let bsp = smp.bsp_cpu_id();
    let cur_nr = smp
        .cpu_local(bsp)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "SYSCALL before scheduler bring-up (proc_ptr = None) — the \
                 syscall data path activates with S-6/S-7"
            )
        });

    // User message pointer ABI: RDI (first SysV argument register).
    let m_user = VirBytes::new(frame.rdi);

    let table = unsafe { crate::proc_table_boot_unchecked() };
    // K20 (caller-by-nr): no laundering — the caller travels as its nr.
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );

    // Reply code → RAX for the sysret leg. VmSuspend：调用已通过
    // vm_suspend 停排（RTS_VMREQUEST + memreq 入队 + VM 收 SIGKMEM）——
    // 保存完整上下文（RDI=消息指针经 saved_m_user 另存）、记返回样式、
    // 重入调度器；VM 服务完成后 KCALL_RESUME 阶段重派调用并经
    // set_ipc_return_code 交付 RAX（NK4-A C-3 迭代6，此前此臂 panic）。
    // NoReply 不在此腿出现（receive 型调用走 int-33）。
    match result.reply_code() {
        Some(code) => frame.rax = code as i64 as u64,
        None => {
            if !matches!(result, KcallResult::VmSuspend) {
                panic!("trap_dispatch: syscall returned {result:?} with no reply code");
            }
            {
                let proc = table
                    .get_mut(cur_nr)
                    .expect("syscall suspend: caller slot must exist");
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                #[cfg(not(feature = "mock"))]
                {
                    nk4a_rs_trace_probe("vms", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                    nk4a_rs_anom_probe("vms", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                    nk4a_rs_leak_probe("vms", proc.p_endpoint.0 as u64, frame.rbx, frame.rip);
                }
                proc.trap_style = minix_arch::TrapStyle::FullContext;
                if let Some(ctx) = proc.p_vm_suspend.as_mut() {
                    ctx.saved_m_user = Some(m_user.0);
                }
                // C-3 迭代8 取证：停车时打印挂起态与 VMREQUEST（一次性）——
                // 判别"挂起后未入队/未通知"导致 VM 永不服务的死等。
                #[cfg(not(feature = "mock"))]
                {
                    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                    let (vmreq, state_pending) = table
                        .get(cur_nr)
                        .map(|p| {
                            (
                                p.p_rts_flags
                                    .is_set(crate::proc::RtsFlagsBits::VMREQUEST),
                                p.p_vm_suspend
                                    .as_ref()
                                    .is_some_and(|c| {
                                        matches!(
                                            c.state,
                                            crate::vm::VmSuspendState::Pending
                                        )
                                    }),
                            )
                        })
                        .unwrap_or((false, false));
                    Console::write_str("nk4a: sys-susp vmreq=");
                    Console::write_str(if vmreq { "y" } else { "n" });
                    Console::write_str(" pending=");
                    Console::write_str(if state_pending { "y" } else { "n" });
                    Console::write_str("\n");
                }
                // C RTS_SET 的 dequeue 半：挂起后进程必须移出就绪队列，否则
                // 调度器对已停排进程 spin-pick（dequeue_if_blocked 文档记录
                // 的同症状；NK4-A C-3 真机：RS 挂起后 pick->2 刷屏，
                // 2026-09-22）。
                table.dequeue_if_blocked(cur_nr);
                // C kernel_call_finish 的 VmSuspend 半（system.c:60-63 +
                // D-20 vm_enqueue_and_notify_vm）：挂起上下文入 VM 请求链并
                // 以 SIGKMEM 通知 VM。快路径不走 finish，缺此步则 VM 永不
                // 服务、RS 永久 Pending（真机：sys-susp 后系统静默）。
                table.vm_enqueue_and_notify_vm(
                    cur_nr,
                    unsafe { crate::priv_table_boot_unchecked() },
                );
            }
            crate::scheduler_loop(crate::current_cpu_id());
        }
    }
}

// ── riscv64 / aarch64 production trap bodies (E-3ARCHTRAP) ─────────────
//
// The kernel-side policy half of the production trap legs. The arch asm
// legs (frame save + stack strategy) live in `minix_arch::{riscv64,arm64}
// ::trap_stub`; these bodies receive the frame and decide:
// - **timer arm**: SBI re-arm + per-CPU tick (riscv64) / GIC claim-route
//   (aarch64) → the clock hook chain advances uptime (D-46) → quantum
//   enforcement (C proc.c:418-424, the x86 0xF1 shape).
// - **syscall arm**: the KERNEL_CALL message leg (a7/x8 == 0, message at
//   a0/x0 per `minix_sys::arch_trap`) → `kernel_call`, reply code in
//   a0/x0. The raw IPC legs (a7/x8 1..16) are a registered gap — the
//   int-33 IPC bridge is x86-only so far — and answer -ENOSYS (the same
//   contract the K12b carriers observed).
// - **faults**: panic with the architectural diagnostics. The acting
//   arms (ForwardToVm / cause_signal — NK2, x86-only) are the downstream
//   "页故障回路三架构化" wave; reaching them here is a registered gap,
//   not recoverable state.
//
// Registered by `init_protection` before `TrapEntryArch::load()`; slot
// semantics per arch are documented on `register_trap_dispatchers`.

/// -ENOSYS on the trap ABI return register (minix_types::errno parity;
/// the carriers pin the same value).
#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
const ENOSYS_CODE: u64 = 38;
/// KERNEL_CALL message-leg trap number (minix_sys::arch_trap).
#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
const KERNEL_CALL_TRAP: u64 = 0;
/// riscv64 scause: Supervisor Timer interrupt (privileged spec §5.2.2,
/// interrupt code 5 under the interrupt bit).
#[cfg(target_arch = "riscv64")]
const RISCV64_CAUSE_SUPERVISOR_TIMER: u64 = 5;
/// riscv64 scause: Environment Call from U-mode (exception code 8).
#[cfg(target_arch = "riscv64")]
const RISCV64_CAUSE_ECALL_UMODE: u64 = 8;
/// riscv64 scause interrupt bit (bit 63).
#[cfg(target_arch = "riscv64")]
const RISCV64_SCAUSE_INTERRUPT: u64 = 1 << 63;
/// GIC spurious INTID (nothing claimable — ICC_IAR1_EL1).
#[cfg(target_arch = "aarch64")]
const GIC_SPURIOUS_INTID: u32 = 1023;

#[cfg(target_arch = "riscv64")]
fn riscv64_read_scause() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free CSR read in S-mode; nomem/nostack per the
    // usual CSR-read contract.
    unsafe { core::arch::asm!("csrr {}, scause", out(reg) v, options(nomem, nostack)); }
    v
}

#[cfg(target_arch = "riscv64")]
fn riscv64_read_stval() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free CSR read in S-mode.
    unsafe { core::arch::asm!("csrr {}, stval", out(reg) v, options(nomem, nostack)); }
    v
}

/// Kernel-leg body (S-origin): supervisor timer ticks + kernel-fault
/// diagnostics. The SSI (IPI) and SEI (PLIC external) interrupt arms are
/// registered gaps — S-10/K11 own the riscv64 IPI lane on this leg.
///
/// # Safety
///
/// `frame` points at the live kernel-leg frame (asm contract); the stub
/// resumes via `sret` when this returns.
#[cfg(target_arch = "riscv64")]
pub unsafe extern "C" fn riscv64_kernel_body(frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame) {
    let scause = riscv64_read_scause();
    if scause & RISCV64_SCAUSE_INTERRUPT != 0
        && scause & !RISCV64_SCAUSE_INTERRUPT == RISCV64_CAUSE_SUPERVISOR_TIMER
    {
        riscv64_timer_arm();
        return;
    }
    riscv64_diag_panic(frame, scause, "kernel-leg trap");
}

/// User-leg body (U-origin): ecall kernel calls + user-fault diagnostics.
/// Fault classification (page-fault forwarding / signals) is the
/// downstream three-arch wave — a faulting user process panics here with
/// diagnostics instead of being parked for VM.
///
/// # Safety
///
/// `frame` points at the live user-leg frame at the kernel-stack top
/// (asm contract); the interrupted user sp travels in `frame.gpr[2]`.
#[cfg(target_arch = "riscv64")]
pub unsafe extern "C" fn riscv64_user_body(frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame) {
    let scause = riscv64_read_scause();
    if scause == RISCV64_CAUSE_ECALL_UMODE {
        // `ecall` does not advance sepc — step past the 4-byte
        // instruction or the sret re-executes it and the trap loops
        // forever (minix-sys arch_trap contract).
        frame.sepc = frame.sepc.wrapping_add(4);
        let leg = frame.gpr[17] as u64; // a7 = call number register
        if leg == KERNEL_CALL_TRAP {
            riscv64_kernel_call_leg(frame);
        } else {
            // Raw IPC legs (SEND..SENDA) and MINIX_KERNINFO: the
            // int-33-style IPC bridge is x86-only so far — registered
            // gap, answered -ENOSYS on the a0/a1 return pair.
            frame.gpr[10] = (-(ENOSYS_CODE as i64)) as u64; // a0
            frame.gpr[11] = 0; // a1
        }
        return;
    }
    riscv64_diag_panic(frame, scause, "user-leg trap");
}

/// The riscv64 timer arm — one S-mode timer tick, three jobs in the x86
/// local-tick + PIT-hook order:
/// 1. `local_tick` (per-CPU observability counter + the SBI re-arm via
///    `ClockArch::local_timer_eoi` — the one-shot stops dead unless the
///    body re-arms, apic.c:578 parity),
/// 2. the clock hook chain under `minix_plat::TIMER_IRQ` (advances
///    uptime, expires alarms — D-46; the PLIC claim is a documented
///    no-op for the CPU-local timer, pseudo-vector 0),
/// 3. quantum enforcement for this CPU's running process
///    (C proc.c:418-424).
fn riscv64_timer_arm() {
    crate::clock::local_tick(crate::clock::current_cpuid());
    match crate::irq_manager::dispatch_hardware_irq(minix_plat::TIMER_IRQ) {
        Ok(()) => {}
        Err(crate::irq_manager::IrqError::Spurious(irq)) => {
            use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
            Console::write_str("spurious irq ");
            Console::write_hex(irq.get() as u64);
            Console::write_str("\n");
        }
        Err(e) => panic!("riscv64 timer arm: IRQ dispatch error {e:?}"),
    }
    // Quantum check under try-or-inherit BKL (the SCHED_IPI shape): the
    // tick may interrupt the kernel with the BKL held (inherit) or an
    // idle window without it (acquire → release).
    let acquired = crate::smp::bkl_try_lock();
    {
        let section = unsafe { crate::smp::BklSection::assume_held() };
        let smp = crate::smp_state_with(&section);
        let cur_nr = {
            let cpu = crate::current_cpu_id();
            smp.cpu_local(cpu).and_then(|l| l.proc_ptr)
        };
        if let Some(nr) = cur_nr {
            let table = crate::proc_table_with(&section);
            let priv_table = crate::priv_table_with(&section);
            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                table.check_quantum(nr, priv_table, &section);
            }
        }
    }
    if acquired {
        crate::smp::bkl_unlock();
    }
}

/// The KERNEL_CALL leg: message pointer at a0, call number in
/// `m_type` — `kernel_call` parity with the x86 SYSCALL body.
///
/// # Safety
///
/// Caller guarantees the registered-dispatcher invariants (scheduler
/// brought up, tables initialized); a blocked-call outcome is a wiring
/// bug here (blocking IPC arrives through the IPC bridge, x86 parity).
#[cfg(target_arch = "riscv64")]
unsafe fn riscv64_kernel_call_leg(frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame) {
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "riscv64 kernel call before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
    // User message pointer ABI: a0 (minix_sys::arch_trap kernel_call_trap).
    let m_user = VirBytes::new(frame.gpr[10]);
    let table = unsafe { crate::proc_table_boot_unchecked() };
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );
    // Reply code → a0. NoReply/VmSuspend must not occur on this leg
    // (blocking IPC arrives via the IPC bridge) — x86 B-body parity:
    // panic rather than reply garbage.
    frame.gpr[10] = match result.reply_code() {
        Some(code) => code as i64 as u64,
        None => panic!("riscv64 kernel call returned {result:?} with no reply code"),
    };
}

/// Console diagnostics + panic for unreached causes — the production
/// replacement of the removed print-and-halt diag stub: the panic routes
/// through the kernel diagnostic path instead of halting in the entry
/// leg, so a scheduler can at least be observed around the corpse.
#[cfg(target_arch = "riscv64")]
fn riscv64_diag_panic(
    frame: &minix_arch::riscv64::trap_stub::Riscv64TrapFrame,
    scause: u64,
    origin: &str,
) {
    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
    let stval = riscv64_read_stval();
    Console::write_str(origin);
    Console::write_str(" scause=");
    Console::write_hex(scause);
    Console::write_str(" stval=");
    Console::write_hex(stval);
    Console::write_str(" sepc=");
    Console::write_hex(frame.sepc);
    Console::write_str(" sstatus=");
    Console::write_hex(frame.sstatus);
    Console::write_str("\n");
    panic!(
        "riscv64 {origin}: scause {scause:#x} stval {stval:#x} sepc {:#x}",
        frame.sepc
    );
}

/// Current-EL (kernel) body: GIC claim → route → dispatch, kernel-fault
/// diagnostics. IRQ: the claim read IS the identification register on
/// GIC (D-61), so the INTID routes the chain and travels to
/// `dispatch_claimed_hardware_irq` for the completion half. Sync: a
/// kernel-origin synchronous exception is a kernel bug at this stage —
/// panic with the syndrome (the EL0 page-fault arm lands with the
/// three-arch wave).
///
/// # Safety
///
/// `frame` points at the live kernel-leg frame (asm contract); the stub
/// resumes via `eret` when this returns.
#[cfg(target_arch = "aarch64")]
pub unsafe extern "C" fn aarch64_kernel_body(
    frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame,
    class: u64,
) {
    if class == minix_arch::arm64::trap_stub::TRAP_CLASS_IRQ {
        let claimed = crate::irq_manager::claim_hardware_irq();
        match claimed {
            // Spurious: nothing claimable, nothing to complete (K8).
            None => return,
            Some(id) if id == GIC_SPURIOUS_INTID => return,
            Some(id) => {
                // INTIDs ≥ NR_IRQ_VECTORS cannot be enabled (the
                // controller clamps its line count at the manager bound)
                // — routing one would alias through the u8 conversion.
                if id as usize >= minix_plat::NR_IRQ_VECTORS {
                    panic!(
                        "aarch64 IRQ route: intid {id} beyond NR_IRQ_VECTORS — \
                         controller wiring bug"
                    );
                }
                // The boot clock PPI: per-CPU re-arm + observability tick
                // BEFORE the hook chain (the riscv64 timer-arm shape;
                // CNTP is a one-shot — without the re-arm the tick train
                // stops dead after the first interrupt).
                let is_timer = id as u8 == minix_plat::TIMER_IRQ.get();
                if is_timer {
                    crate::clock::local_tick(crate::clock::current_cpuid());
                }
                let irq = minix_plat::IrqVector::new(id as u8);
                match crate::irq_manager::dispatch_claimed_hardware_irq(irq, claimed) {
                    Ok(()) => {}
                    Err(crate::irq_manager::IrqError::Spurious(line)) => {
                        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
                        Console::write_str("spurious irq ");
                        Console::write_hex(line.get() as u64);
                        Console::write_str("\n");
                    }
                    Err(e) => panic!("aarch64 IRQ route: dispatch error {e:?} (intid {id})"),
                }
                // Quantum enforcement for a timer tick (C proc.c:418-424;
                // try-or-inherit BKL — the SCHED_IPI shape).
                if is_timer {
                    let acquired = crate::smp::bkl_try_lock();
                    {
                        let section = unsafe { crate::smp::BklSection::assume_held() };
                        let smp = crate::smp_state_with(&section);
                        let cur_nr = {
                            let cpu = crate::current_cpu_id();
                            smp.cpu_local(cpu).and_then(|l| l.proc_ptr)
                        };
                        if let Some(nr) = cur_nr {
                            let table = crate::proc_table_with(&section);
                            let priv_table = crate::priv_table_with(&section);
                            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                                table.check_quantum(nr, priv_table, &section);
                            }
                        }
                    }
                    if acquired {
                        crate::smp::bkl_unlock();
                    }
                }
            }
        }
        return;
    }
    aarch64_diag_panic(frame, "kernel sync exception");
}

/// Lower-EL (user) body: SVC kernel calls + user-origin diagnostics.
///
/// # Safety
///
/// `frame` points at the live user-leg frame on the EL1 entry stack
/// (asm contract); the interrupted EL0 sp travels in `frame.sp`.
#[cfg(target_arch = "aarch64")]
pub unsafe extern "C" fn aarch64_user_body(
    frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame,
    class: u64,
) {
    if class == minix_arch::arm64::trap_stub::TRAP_CLASS_IRQ {
        // Interrupts arriving from EL0 route through the same GIC
        // claim/route as kernel-origin ones.
        aarch64_kernel_body(frame, class);
        return;
    }
    // Synchronous: discriminate by ESR_EL1 EC (bits [31:26]).
    // EC 0x15 = SVC from AArch64; `svc` DOES advance ELR — no PC step
    // (minix-sys arch_trap contract).
    const ESR_EC_SHIFT: u64 = 26;
    const ESR_EC_MASK: u64 = 0x3F;
    const EC_SVC_AARCH64: u64 = 0x15;
    let esr = aarch64_read_esr();
    let ec = (esr >> ESR_EC_SHIFT) & ESR_EC_MASK;
    if ec == EC_SVC_AARCH64 {
        let leg = frame.gpr[8] as u64; // x8 = call number register
        if leg == KERNEL_CALL_TRAP {
            aarch64_kernel_call_leg(frame);
        } else {
            // Raw IPC legs / MINIX_KERNINFO: registered gap (x86-only
            // IPC bridge) — -ENOSYS on the x0/x1 return pair.
            frame.gpr[0] = (-(ENOSYS_CODE as i64)) as u64;
            frame.gpr[1] = 0;
        }
        return;
    }
    aarch64_diag_panic(frame, "user sync exception");
}

#[cfg(target_arch = "aarch64")]
fn aarch64_read_esr() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free system-register read at EL1.
    unsafe { core::arch::asm!("mrs {}, esr_el1", out(reg) v, options(nomem, nostack)); }
    v
}

/// The KERNEL_CALL leg: message pointer at x0 — `kernel_call` parity
/// with the x86 SYSCALL body (riscv64 sibling carries the full contract).
///
/// # Safety
///
/// Same invariants as `riscv64_kernel_call_leg`.
#[cfg(target_arch = "aarch64")]
unsafe fn aarch64_kernel_call_leg(frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame) {
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "aarch64 kernel call before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
    let m_user = VirBytes::new(frame.gpr[0]);
    let table = unsafe { crate::proc_table_boot_unchecked() };
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );
    frame.gpr[0] = match result.reply_code() {
        Some(code) => code as i64 as u64,
        None => panic!("aarch64 kernel call returned {result:?} with no reply code"),
    };
}

/// Console diagnostics + panic for unreached causes (riscv64 sibling
/// carries the rationale).
#[cfg(target_arch = "aarch64")]
fn aarch64_diag_panic(frame: &minix_arch::arm64::trap_stub::AArch64TrapFrame, origin: &str) {
    use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
    let far = aarch64_read_far();
    Console::write_str(origin);
    Console::write_str(" esr=");
    Console::write_hex(aarch64_read_esr());
    Console::write_str(" far=");
    Console::write_hex(far);
    Console::write_str(" elr=");
    Console::write_hex(frame.elr);
    Console::write_str(" spsr=");
    Console::write_hex(frame.spsr);
    Console::write_str("\n");
    panic!(
        "aarch64 {origin}: elr {:#x} spsr {:#x} far {far:#x}",
        frame.elr, frame.spsr
    );
}

#[cfg(target_arch = "aarch64")]
fn aarch64_read_far() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free system-register read at EL1.
    unsafe { core::arch::asm!("mrs {}, far_el1", out(reg) v, options(nomem, nostack)); }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_frame_carries_cpu_tail() {
        // The dispatcher's view is exactly the CPU-pushed fields; GPRs stay
        // in the TrapFrame and are restored by the asm leg.
        let frame = TrapFrame {
            rax: 1,
            rbx: 2,
            rcx: 3,
            rdx: 4,
            rsi: 5,
            rdi: 6,
            rbp: 7,
            r8: 8,
            r9: 9,
            r10: 10,
            r11: 11,
            r12: 12,
            r13: 13,
            r14: 14,
            r15: 15,
            vector: 14,
            errcode: 2,
            rip: 0x1000,
            cs: 0x1B,
            rflags: 0x202,
            rsp: 0x7FFF_0000,
            ss: 0x23,
        };
        let exc = exception_frame_of(&frame);
        assert_eq!(X86_64ExceptionFrame::vector(&exc).get(), 14);
        assert_eq!(X86_64ExceptionFrame::error_code(&exc), 2);
        assert_eq!(
            X86_64ExceptionFrame::instruction_pointer(&exc),
            VirBytes::new(0x1000)
        );
        assert!(X86_64ExceptionFrame::is_user_mode(&exc));
        assert!(X86_64ExceptionFrame::is_write_fault(&exc));
    }

    #[test]
    fn spurious_vector_dispatch_completes() {
        // S-9 (D-38①): the LAPIC spurious vector (0xFF) returns WITHOUT
        // dispatching and without panicking — exercises the full entry path
        // (asm→thunk contract aside, the body's early-return arm) hosted,
        // including the BKL witness acquisition.
        let mut frame = TrapFrame {
            rax: 0, rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
            vector: 0xFF,
            errcode: 0,
            rip: 0, cs: 0x08, rflags: 0, rsp: 0, ss: 0,
        };
        // SAFETY: hosted test body; the frame is a local.
        unsafe { x86_trap_dispatch_body(&mut frame) };
    }

    #[test]
    fn irq_entry_mirrors_user_frame_but_skips_kernel_origin() {
        // NK4-A Task C 防回归（C: mpx.S 每个 IRQ/clock stub 入口
        // SAVE_PROCESS_CTX — sconst.h:75-89）：user-origin tick/IRQ 必须在
        // 任何调度决定前把全量寄存器（含 RBX 活值）镜像进进程 ctx，否则
        // 被抢占/挪动后 finish_and_restore 从陈旧 ctx 恢复（真机 c19a-c21a
        // RS step2 &self.table=0x0）；kernel-origin 中断不得镜像（frame 是
        // 内核寄存器）。判别性：删 user 镜像 → 第一组断言失败；删 CPL 门 →
        // 第二组（陈旧 rip 保留）失败。
        let mut table = crate::test_helpers::test_proc_table();
        let proc = table.get_mut(crate::proc::ProcNr(0)).unwrap();
        // Pre-seed the context through the write seam (save with a
        // sentinel frame): the kernel-visible read seams are
        // ipc_status_register（NK4-C Task C A 案后 = R10 状态车道）/
        // ipc_return_code (RAX) / callee_saved_rbx (RBX)。
        let seed = TrapFrame {
            rax: 7,
            rbx: 0xABCD_0000,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0x1111_0000,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            vector: 0x50,
            errcode: 0,
            rip: 0x203bf0,
            cs: 0x1B, // user CS → RPL3, must mirror
            rflags: 0x202,
            rsp: 0x7FFF_0000,
            ss: 0x23,
        };
        assert!(
            mirror_irq_frame_into_proc(&seed, proc),
            "user-origin IRQ must mirror"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::callee_saved_rbx(&proc.cpu_context),
            0xABCD_0000,
            "RBX live value must survive into ctx (callee-saved完整性)"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::ipc_status_register(&proc.cpu_context),
            0x1111_0000,
            "R10 must be mirrored (A 案：R10 = IPC 状态车道)"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::ipc_return_code(&proc.cpu_context),
            7,
            "RAX must be mirrored"
        );
        assert_eq!(proc.trap_style, TrapStyle::FullContext);
        // Kernel CS + different register values → must NOT touch the ctx.
        let mut kern = seed;
        kern.cs = 0x08;
        kern.rbx = 0;
        kern.rax = 99;
        assert!(
            !mirror_irq_frame_into_proc(&kern, proc),
            "kernel-origin IRQ must skip the mirror"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::callee_saved_rbx(&proc.cpu_context),
            0xABCD_0000,
            "kernel-origin must not overwrite RBX"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::ipc_return_code(&proc.cpu_context),
            7,
            "kernel-origin must not overwrite RAX"
        );
    }

    #[test]
    fn reply_code_none_is_a_wiring_bug_marker() {
        // The B leg panics on NoReply/VmSuspend rather than replying
        // garbage; this pins that those variants exist and are distinct
        // from Ok/BadCall/CallDenied (which carry a reply code).
        assert_eq!(KcallResult::Ok(7).reply_code(), Some(7));
        assert!(KcallResult::NoReply.reply_code().is_none());
        assert!(KcallResult::VmSuspend.reply_code().is_none());
    }

    #[test]
    fn exception_signal_maps_to_c_signum() {
        // C: the ex_data[].signum column (exception.c:19-39) — the
        // classification lives in the arch dispatcher, the numeric signal
        // in this kernel signal module; the mapping must hit the C
        // constants exactly (signal.h:55-63).
        use minix_arch::exception_dispatcher::ExceptionSignal;
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Fpe), crate::syscall_signal::SIGFPE);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Ill), crate::syscall_signal::SIGILL);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Segv), crate::syscall_signal::SIGSEGV);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Bus), crate::syscall_signal::SIGBUS);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Emt), crate::syscall_signal::SIGEMT);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Trap), crate::syscall_signal::SIGTRAP);
    }

    /// Shape a test table with a faulting user proc (nr 0) and the VM
    /// slot (VM_PROC_NR), both occupied with distinct endpoints.
    fn pagefault_test_table() -> (crate::test_helpers::TestProcTable, crate::proc::ProcNr) {
        use crate::proc::RtsFlagsBits;
        let mut table = crate::test_helpers::test_proc_table();
        let caller_nr = crate::proc::ProcNr(0);
        {
            let p = table.get_mut(caller_nr).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_endpoint = minix_types::Endpoint(100);
        }
        {
            let p = table.get_mut(crate::proc::proc_nr::VM_PROC_NR).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_endpoint = minix_types::Endpoint(8);
        }
        (table, caller_nr)
    }

    #[test]
    fn forward_pagefault_blocks_when_vm_not_receiving() {
        // C: mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL) with VM
        // busy → the faulting process parks on the VM caller queue,
        // carrying RTS_PAGEFAULT (the scheduler must skip it until VM
        // resolves — exception.c:115 + proc.c:938-960).
        use crate::ipc::IpcOutcome;
        use crate::proc::{proc_nr, RtsFlagsBits};
        let (mut table, caller_nr) = pagefault_test_table();
        let mut priv_table = crate::test_helpers::test_priv_table();

        let outcome = forward_pagefault_to_vm(&mut table, &mut priv_table, caller_nr, 0x4000, 0x4);
        assert!(matches!(outcome, Ok(IpcOutcome::Blocked)));
        {
            let p = table.get(caller_nr).unwrap();
            assert!(p.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT));
            assert!(!p.is_runnable(), "faulting process must not stay schedulable");
            assert!(p.p_rts_flags.is_set(RtsFlagsBits::SENDING));
            assert_eq!(p.p_sendto_e, minix_types::Endpoint(8));
            assert_eq!(p.p_fault_addr, Some(0x4000));
        }
        assert_eq!(
            table.get(proc_nr::VM_PROC_NR).unwrap().caller_q_head,
            Some(caller_nr),
            "caller must be enqueued on VM's caller queue"
        );
    }

    #[test]
    fn forward_pagefault_delivers_to_receiving_vm() {
        // Path A: VM in RECEIVE for the faulting endpoint → direct
        // delivery; the delivered message is the VM_PAGEFAULT wire shape
        // (m_source = faulting endpoint, VPF_ADDR, VPF_FLAGS —
        // exception.c:118-122), and the faulting process is parked on
        // RTS_PAGEFAULT alone (delivery cleared nothing for the sender).
        use crate::ipc::IpcOutcome;
        use crate::proc::{proc_nr, RtsFlagsBits};
        let (mut table, caller_nr) = pagefault_test_table();
        {
            let vm = table.get_mut(proc_nr::VM_PROC_NR).unwrap();
            vm.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            vm.p_getfrom_e = minix_types::Endpoint(100);
        }
        let mut priv_table = crate::test_helpers::test_priv_table();

        let outcome = forward_pagefault_to_vm(&mut table, &mut priv_table, caller_nr, 0xCAFE, 0x6);
        assert!(matches!(outcome, Ok(IpcOutcome::Delivered)));
        {
            let vm = table.get(proc_nr::VM_PROC_NR).unwrap();
            assert!(vm.p_misc_flags.is_set(crate::proc::MiscFlagsBits::DELIVERMSG));
            assert_eq!(vm.p_delivermsg.m_type, minix_types::VM_PAGEFAULT as i32);
            assert_eq!(vm.p_delivermsg.m_source, minix_types::Endpoint(100));
            // SAFETY: m_vm_pagefault is the active arm — the kernel just
            // wrote it via build_vm_pagefault_msg.
            let pf = unsafe { vm.p_delivermsg.m_u.m_vm_pagefault };
            assert_eq!(pf.vpf_addr, 0xCAFE);
            assert_eq!(pf.vpf_flags, 0x6);
        }
        {
            let p = table.get(caller_nr).unwrap();
            assert!(p.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT));
            assert!(
                !p.is_runnable(),
                "delivered != resolved: parked until SYS_VMCTL ClearPageFault"
            );
        }
    }

    #[test]
    fn forward_pagefault_errors_on_dead_vm_slot() {
        // C: mini_send error → the caller panics ("WARNING: pagefault:
        // mini_send returned %d"). The helper surfaces the errno as Err
        // for the trap body's panic; RTS_PAGEFAULT was still set first
        // (exception.c:115 precedes the send).
        use crate::ipc::IpcError;
        use crate::proc::RtsFlagsBits;
        let (mut table, caller_nr) = pagefault_test_table();
        {
            let vm = table.get_mut(crate::proc::proc_nr::VM_PROC_NR).unwrap();
            vm.p_rts_flags.set(RtsFlagsBits::NO_ENDPOINT);
        }
        let mut priv_table = crate::test_helpers::test_priv_table();

        let outcome = forward_pagefault_to_vm(&mut table, &mut priv_table, caller_nr, 0x4000, 0x4);
        assert_eq!(outcome, Err(IpcError::DeadSrcDst));
        let p = table.get(caller_nr).unwrap();
        assert!(p.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT));
    }
}
