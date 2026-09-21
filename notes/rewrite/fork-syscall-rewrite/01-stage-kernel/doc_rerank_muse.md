# 01-stage-kernel Documentation Rebuild Blueprint (muse)

```text
your_name(AI agent name) = muse
target_dir(关注的工作目录) = notes/rewrite/fork-syscall-rewrite/01-stage-kernel
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：output target_dir/doc_rerank_muse.md, modify no body text.
        Besides C source, also read: all doc headers in this dir, os/ entry points,
        00-*-overview.md navigation tables.
约束 = Never cite .design/ nor tmp_design_and_todo/; any landed artifact must carry
        _muse suffix; never read nor copy other AIs' doc_rerank_* products.
```

This report is self-contained: it assumes the reader has not seen any chat history.
All factual claims carry anchors. Anything without an anchor is explicitly marked
as inference or to-be-verified. The report is written in English per muse convention.
It is a directly executable rebuild specification: Phase B can take each new-chapter
contract, fetch the listed material, and write the body without further triage.

Method premise (rebuild, not relocation): the current catalog is a ~35-node web in
which every move re-balances neighbours (boundaries, forward declarations, cross
references, code-comment citations). Relocating chapter by chapter converges to
"everything moved a little, nothing became good". The blueprint therefore defines
a NEW catalog as the coordinate system; each new chapter fetches its contracted
knowledge items from the pool (old docs + C source + non-C artifacts + OS theory)
and is written fresh. Old docs serve as knowledge sources, never as move targets.
Breakage cost is counted BEFORE rebuild (anchor migration + reference migration
tables below); a rebuild whose cost is not visible is not started.

---

## 0. Metadata

- Executor: muse. Date (UTC): 2026-09-19. Target: `notes/rewrite/fork-syscall-rewrite/01-stage-kernel`.
- Repo root: `/home/xzhao/github/minix-rs`. Commit: `338c7a301163b9ce4b1b5cc4ce2a6a354cfaa680`.
- Scope: numbered docs `00`–`33` + `99-global-concepts.md` are IN SCOPE as knowledge
  sources. Reference material (NOT rebuilt, read as evidence): `todo.md` (V13),
  `checklist.md`, `06-todo.md` (writing-style example + 06-rework spec),
  `endpoint_todo.md`, `smp_todo.md`, `panic-in-drop.md`, `smp_gpt*.md`,
  `07-paging_init_gpt.md` (frozen-decision memo, not a reader doc),
  `18-trap-bridge-design.md` (approved design memo 2026-09-16, not a reader doc).
  OUT OF SCOPE: any `doc_rerank_*` product (never read, per task isolation rule);
  `.design/` and `tmp_design_and_todo/` (never cited, per project rule).
- Reading list actually read (headers of ALL in-scope docs, §1 bodies sampled;
  full bodies of 00/06-proc/08/10/13 sampled for contract evidence):
  `00-kernel-overview.md` (388 lines), `01-boot-shim-bootstrap.md` (2019),
  `02-higher-half-kernel.md` (1329), `03-kmain-cstart.md` (1197),
  `04-platform-discovery.md` (1255), `05-clock-interrupt-init.md` (2263),
  `06-proc-init-boot-proc.md` (1375), `07-cross-space-init.md` (691),
  `08-system-init-boot-finish.md` (1019), `09-vm-boot-protocol.md` (594),
  `10-switch-to-user.md` (506), `11-scheduling-primitives.md` (716),
  `12-ipc-core.md` (1209), `13-syscall-dispatch.md` (949),
  `14-exception-interrupt.md` (661), `15-clock-timer.md` (1162),
  `16-smp.md` (1298), `17-syscall-process.md` (924), `18-syscall-copy.md` (809),
  `19-syscall-signal.md` (931), `20-syscall-device.md` (796),
  `21-syscall-clock.md` (829), `22-privilege.md` (622), `23-ipc-filter.md` (523),
  `24-cross-space-runtime.md` (468), `25-misc-unported.md` (819),
  `26-watchdog.md` (293), `27-kernel-utility.md` (392), `28-usermapped-data.md` (448),
  `29-kernel-debug.md` (369), `30-kernel-profile.md` (368),
  `31-fpu-context-switching.md` (602), `32-stack-tracing.md` (575),
  `33-syscall-caller-api.md` (222), `99-global-concepts.md` (55),
  plus C sources `minix3/minix/kernel/{main,proc,system,clock,interrupt,smp,table,
  profile,debug,utility,watchdog,usermapped_data}.c`, `system/do_*.c` (38 files),
  `arch/i386/{head.S,kernel.lds,klib.S,mpx.S,trampoline.S,io_*.S,protect.c,
  exception.c,memory.c,pre_init.c,pg_utils.c,arch_system.c,arch_clock.c,
  arch_smp.c,i8259.c,apic.c,usermapped_glo_ipc.S}`, `include/minix/com.h`
  (KERNEL_CALL=0x600, NR_SYS_CALLS=58), Rust `os/kernel/src/{lib,proc,proc_table,
  kpriv,capability,syscall,syscall_*,ipc,ipc_filter,cross_space,vm,smp,sched,
  clock,irq_manager,misc,grant,stacktrace,debug}.rs` and `os/boot-shim/src/*`,
  boundary `fork-syscall-rewrite/00-master-plan/README.md` + `edge_todo.md`.
- Commands run (read-only) with key outputs:
  - `wc -l notes/rewrite/fork-syscall-rewrite/01-stage-kernel/*.md` → 44,418 total
    incl. other-AI products; in-scope numbered docs ≈ 30 docs, ≈ 24,000 lines.
  - `ls minix3/minix/kernel/` → 34 entries incl. `system/` (38 `do_*.c`),
    `arch/{i386,earm}`; core `.c` ≈ 5,900 lines + headers ≈ 1,000 lines.
  - `grep -n "map(SYS_" minix3/minix/kernel/system.c` → 40 mapped handlers
    (see §1 R4 table); `grep -n KERNEL_CALL minix3/minix/include/minix/com.h` →
    `KERNEL_CALL 0x600`, `NR_SYS_CALLS 58`.
  - `rg -o "[0-3][0-9]-[a-z-]+\.md" 01-stage-kernel/*.md | sort | uniq -c` →
    densest cross-refs: `07-cross-space-init→07-pagetable-struct(02-stage-vm)` ×24
    (cross-stage forward ref), `01↔02` ×24/×10, `06→11` ×21, `03→14` ×20.
  - `rg -n "covered in|see .*\.md|01-stage-kernel" os/kernel/src os/arch/src` →
    ≈ 35 code-comment citations of old doc numbers/sections (migration cost §8).

---

## 1. C True Order (runtime truth, rebuilt from source, not paraphrased)

Stage-type判定: this stage is a **boot-chain type with a runtime tail**.
The boot half (firmware → kmain → bsp_finish_booting → first user process) is a
strict linear startup chain and the narration backbone. The runtime half (trap /
IPC / syscall dispatch / clock tick / VM handshake / SMP / shutdown) is an
event-driven set entered repeatedly through `switch_to_user`. The blueprint
therefore uses **boot order as the main line** (new docs N01–N08 follow it
exactly) and organizes the runtime half as **convergence-point + trigger-time**
groups (framework chapter first, then role groups, one representative member
taught in depth per group, rest closed by difference tables). Mixed-type note:
no fake linear order is imposed on the ~40 syscall handlers; they are grouped
by role (process / memory-copy / signal / time+device), never numbered in
`do_*.c` alphabetical order.

Truth table (each row checkable; anchors are file:symbol):

| Step | Action | C anchor | Note |
|------|--------|----------|------|
| B1 | Firmware loads GRUB; GRUB reads `boot.cfg`, loads kernel ELF + boot modules, jumps to entry | `minix3/etc/boot.cfg.default`; `minix3/minix/kernel/arch/i386/head.S` | GRUB is not Minix source; kernel side is multiboot header + entry |
| B2 | `pre_init()` parses multiboot (mem map, module list) into `kinfo_t` | `minix3/minix/kernel/arch/i386/pre_init.c:pre_init` | Rust equivalent: boot-shim UEFI/OpenSBI → `KernelInfo` (`os/boot-shim/src/*`) |
| B3 | Identity-map page tables built, paging enabled | `minix3/minix/kernel/arch/i386/pg_utils.c` | 32-bit 2-level; 64-bit rewrite uses 4-level (arch evolution, N02) |
| B4 | `head.S` jumps to `kmain(kinfo)` | `minix3/minix/kernel/arch/i386/head.S` | Stack `k_initial_stktop`; higher-half jump on 64-bit (N02) |
| K1 | `kmain` entry: BSS check, `memcpy(kinfo)`, `memcpy(kmess)`, board id, `kernel_may_alloc=1`, copy `image[]` | `minix3/minix/kernel/main.c:kmain` (L~150–200) | `NR_BOOT_MODULES` vs `mi_mods_count` panic check |
| K2 | `cstart()`: `prot_init()` → env parse → `init_clock()` → `intr_init(0)` → `arch_init()` | `minix3/minix/kernel/main.c:cstart` (L~403–481); `arch/i386/protect.c:prot_init`; `kernel/clock.c:init_clock`; `arch/i386/i8259.c:intr_init`; `arch/i386/arch_system.c:arch_init` | GDT/IDT/TSS + 8259/APIC + clock vars + env (`ac_layout`, `no_apic`, `no_smp`, `watchdog`) |
| K3 | `BKL_LOCK()`; `proc_init()` clears proc table; `IPCF_POOL_INIT()` | `minix3/minix/kernel/main.c:kmain`; `kernel/proc.c:proc_init`; `kernel/ipc_filter.h` | Slots marked `RTS_SLOT_FREE` |
| K4 | Boot-image loop over `NR_BOOT_PROCS`: `get_priv` static ids, `fill_sendto_mask`, `s_k_call_mask` fill, `arch_boot_proc(ip,rp)`; non-VM inhibited (`RTS_VMINHIBIT\|RTS_BOOTINHIBIT\|RTS_PROC_STOP`, clear `RTS_SLOT_FREE`); only kernel tasks + RS + VM schedulable | `minix3/minix/kernel/main.c:kmain` loop (L~196–280); `kernel/system.c:get_priv`; `kernel/arch/i386/memory.c:arch_boot_proc` | VM (`VM_PROC_NR`) gets `VM_F/SRV_T/SRV_M/SRV_KC/SRV_Q`; tasks get `TSK_F/TSK_M/TSK_KC`; RS gets `RSYS_F/SRV_*` |
| K5 | `arch_post_init()` (record ptproc + pagetable addr); `IPCNAME()` × 6; `memory_init()` (freepdes); `system_init()` (irq_hooks=NONE, alarm timers, `call_vec[]` 40 maps); `add_memmap()` reclaims bootstrap memory | `minix3/minix/kernel/main.c:kmain`; `arch/i386/protect.c:arch_post_init`; `arch/i386/pg_utils.c:pg_mapkernel/pg_info`; `kernel/system.c:system_init` (L~170–270) | `call_vec` mapping list = R4 table source |
| K6 | SMP branch: `smp_init()` (AP bring-up) or single-CPU fallback → `bsp_finish_booting()` | `minix3/minix/kernel/main.c:kmain` tail; `kernel/smp.c:smp_init`; `kernel/arch/i386/arch_smp.c` | `config_no_apic/no_smp` env gates |
| K7 | `bsp_finish_booting()`: `cpu_identify`, `bill_ptr=proc_ptr=idle`, `announce`, clear `RTS_PROC_STOP` for boot procs, `cycles_accounting_init`, `boot_cpu_init_timer`, `fpu_init`, `kernel_may_alloc=0`, `switch_to_user()` + `NOT_REACHABLE` | `minix3/minix/kernel/main.c:bsp_finish_booting` | Kernel never returns from here |
| R1 | `switch_to_user()` loop: drain delay-slot (`enqueue_head`), pick next (`pick_proc`), `arch_finish_switch_to_user`, `restore_user_context` (iret/eret/sret); idle proc when nothing runnable | `minix3/minix/kernel/proc.c:switch_to_user` (L299–437); `proc.c:pick_proc/enqueue/dequeue` | Single exit funnel for ALL kernel work |
| R2 | Trap entries: sync exceptions (page fault, GP, #NM…), async IRQs (clock, devices, IPI), IPC trap vector 33, kernel-call vector 32; asm stubs save regs → C dispatcher | `arch/i386/exception.c`; `kernel/interrupt.c`; `arch/i386/mpx.S`; `arch/i386/io_intr.S`; `arch/i386/usermapped_glo_ipc.S:IPCFUNC`; `arch/i386/include/hw_intr.h` | x86-64 rewrite: IDT33 DPL=3 trap gate + SYSCALL/MSR_LSTAR leg (N10) |
| R3 | IPC path `do_ipc`: endpoint check → `s_ipc_to` L1 check → SEND/SENDREC/RECEIVE/SENDNB/NOTIFY/SENDA arms → `deadlock()` check → block (`RTS_SENDING/RTS_RECEIVING`) or `delivermsg` copy → `mini_notify` wakeups | `minix3/minix/kernel/proc.c:do_ipc` (L599–698); `mini_send/mini_receive/mini_notify/delivermsg/deadlock` (L703–1346) | 6 primitives; SENDA async batch + `MF_SENDA_VM_MISS` |
| R4 | Syscall path `kernel_call`: `copy_msg_from_user` → `kernel_call_dispatch` (range + `s_k_call_mask` check → `EBADREQUEST/ECALLDENIED`) → `call_vec[call_nr](caller,msg)` → `kernel_call_finish` (`VMSUSPEND` save / `EDONTREPLY` / `copy_msg_to_user`) | `minix3/minix/kernel/system.c:kernel_call/kernel_call_dispatch/kernel_call_finish` (L52–163) | 40 mapped handlers grouped below |
| R5 | Clock tick `timer_int_handler`: jiffies/uptime accounting, per-process times, `set_kernel_timer` alarms, `do_setalarm/do_vtimer` timers, quantum expiry → `RTS_NO_QUANTUM` | `minix3/minix/kernel/clock.c`; `system/do_setalarm.c`; `system/do_vtimer.c`; `arch/i386/arch_clock.c` | 100 Hz base; quantum handled by sched layer |
| R6 | VM handshake `SYS_VMCTL` subfunctions: SETADDRSPACE / KERN_PHYSMAP / MEMREQ / etc. (VM builds real pagetables, kernel switches CR3) | `minix3/minix/kernel/system/do_vmctl.c`; `arch/i386/arch_do_vmctl.c` | Boot-time multi-round negotiation (N13) |
| R7 | Cross-space fault during kernel-borrowed copy: `VMSUSPEND` → `RTS_VMREQUEST` + notify VM (`SIGKMEM`) → VM replies → `kernel_call_resume` | `minix3/minix/kernel/proc.c:vm_suspend`; `kernel/system.c:kernel_call_resume`; `kernel/vm.h:VMSUSPEND/EFAULT_SRC/EFAULT_DST` | Suspend-notify-resume protocol (N13) |
| R8 | Shutdown: `prepare_shutdown` (1-sec timer) → `minix_shutdown` → `smp_shutdown_aps`, `hw_intr_disable_all`, `stop_local_timer`, banner, `arch_shutdown` | `minix3/minix/kernel/main.c:prepare_shutdown/minix_shutdown` | Least-covered path in old docs (gap G-01) |

Syscall handler groups (from `system_init` `map()` list, `system.c:193–268`):

- Process lifecycle (10): FORK, EXEC, CLEAR, EXIT, PRIVCTL, TRACE, UPDATE, RUNCTL, STATECTL, SCHEDULE, SCHEDCTL, GETMCONTEXT, SETMCONTEXT (13 maps; N12).
- Signals (5): KILL, GETKSIG, ENDKSIG, SIGSEND, SIGRETURN (N14).
- Memory/copy/grants (9): MEMSET, UMAP, UMAP_REMOTE, VUMAP, VIRCOPY, PHYSCOPY, SAFECOPYFROM, SAFECOPYTO, VSAFECOPY, SAFEMEMSET, SETGRANT (11 maps; N13).
- Device/IRQ (6, x86-gated): IRQCTL, DEVIO, VDEVIO, SDEVIO, IOPENABLE, READBIOS (N15).
- Clock (5): TIMES, SETALARM, STIME, SETTIME, VTIMER (N15).
- System/misc (6): ABORT, GETINFO, DIAGCTL, SPROF, VMCTL, PADCONF(arm) (N17/N18).

---

## 2. Knowledge Pool (deduplicated, stage-wide)

Legend — Type: C=concept, M=mechanism, D=data structure, P=interface/protocol,
V=constraint/invariant, E=evolution, T=tooling/engineering, S=test property.
Source: S=stock (from old docs) / N=new (from C / non-C / OS theory, added by §3 audit).
Anchor: C file / artifact path / theory. Duplicates merged; main telling point marked ★.

### Boot & loading (K-001–K-012)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit (answers) |
|----|------|------|-----|-----------------|--------|--------------------------|
| K-001 | Firmware→GRUB→multiboot handoff | M | S | 01 §1.2/§2.0 ★ | `minix3/etc/boot.cfg.default`; `arch/i386/head.S` | What does the kernel receive at entry? |
| K-002 | pre_init kinfo_t construction | M | S | 01 §2.3 ★ | `arch/i386/pre_init.c:pre_init` | How do mem map + modules become kinfo? |
| K-003 | Identity-map bootstrap tables | M | S | 01 §2.3, 02 §4.4 | `arch/i386/pg_utils.c` | Why can C code run before paging is "real"? |
| K-004 | boot-shim UEFI/OpenSBI replacement | E | S | 01 §3.1–3.2 ★ | `os/boot-shim/src/uefi_helpers.rs,opensbi_helpers.rs` | Why is there no GRUB in the rewrite? |
| K-005 | KernelInfo boot contract + validate() | P | S | 01 §4.1, 04 §3.7 | `os/libs/minix-boot/src/kernel_info.rs` | What is the single boot handoff type? |
| K-006 | kernel.lds VMA/LMA/AT() layout | M | S | 02 §2.1 ★ | `arch/i386/kernel.lds` | How do link address ≠ load address work? |
| K-007 | ELF load (parse, copy segments, zero BSS) | M | S | 02 §4.2 ★ | `arch/i386/head.S`; `os/kernel/src/lib.rs:arch_boot_impl` | Who loads the kernel ELF in the rewrite? |
| K-008 | Higher-half stack+PC paired jump | M | S | 02 §1.1/§2.3 ★ | `os/kernel/src/lib.rs:HigherHalf::jump_to_kmain` | Why must stack and PC switch together? |
| K-009 | AT() removal + independent kernel ELF | E | S | 02 §3.1–3.2 | `os/kernel/src/lib.rs` | What link simplifications did 64-bit allow? |
| K-010 | k_initial_stktop provenance | D | S | 02 §2.4 | `arch/i386/head.S`; `arch/i386/kernel.lds` | Where does the first kernel stack come from? |
| K-011 | Boot image table.c slot order vs execution order | C | S | 06 §2.3, 00-master-plan README ★ | `minix3/minix/kernel/table.c:44-64`; `main.c:196,265-267` | Why is DS first in the table but VM first to run? |
| K-012 | Boot module ELF span (start_addr/len from multiboot) | D | N | 06 §2.3 (partial) | `minix3/minix/kernel/main.c:kmain` (`mb_mod->mod_start/mod_end`) | How does the kernel find VM's ELF bytes? |

### Protection & kmain (K-013–K-020)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-013 | Ring0/3 trust boundary + CPL | C | S | 03 §1.1–1.2 ★ | OS theory (privilege); `arch/i386/protect.c:prot_init` | Why can't user code touch kernel memory? |
| K-014 | GDT/IDT/TSS setup (prot_init, 3-arch) | M | S | 03 §2.3–2.5 ★ | `arch/i386/protect.c:prot_init`; `arch/earm/protect.c:prot_init` | What does "protection is ready" mean? |
| K-015 | kmain entry sequence (BSS, memcpy, kernel_may_alloc) | M | S | 03 §2.1, 08 §2.3 | `minix3/minix/kernel/main.c:kmain` | What runs before cstart? |
| K-016 | cstart() call order | M | S | 03 §2.2, 05 §1.2/§2.6 | `minix3/minix/kernel/main.c:cstart` | Why this exact init order? |
| K-017 | x86 GDT retention rationale | C | S | 03 §1.7 | `arch/i386/protect.c` + OS theory | Why does x86-64 keep a GDT at all? |
| K-018 | env_get/boot vars (ac_layout, no_apic, no_smp, watchdog) | P | N | 05 §2.6 (partial) | `minix3/minix/kernel/main.c:cstart,get_value,env_get` | Which boot knobs change kernel behavior? |
| K-019 | ProtectionArch/TrapEntryArch traits | E | S | 03 §3–4 | `os/arch/src/arch/*` | How is protection modeled in types? |
| K-020 | arch_boot_impl boundary vs prot_init | V | S | 03 §1.9 | `os/kernel/src/lib.rs:arch_boot_impl` vs `init_protection` | Where does boot paging end and protection begin? |

### Platform, clock, IRQ init (K-021–K-030)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-021 | acpi_init / ACPI table parsing (x86) | M | S | 04 §2.1, 05 §2.4 | `arch/i386/acpi.c:acpi_init`; `arch/i386/arch_system.c:arch_init` | Where do x86 hw parameters come from? |
| K-022 | bsp_init / board constants (ARM) | M | S | 04 §2.2, 05 §2.5 | `arch/earm/arch_system.c:arch_init`; `arch/earm/bsp/ti/omap_intr.c` | Where do ARM hw parameters come from? |
| K-023 | PlatformDesc trait + 3-arch impls | E | S | 04 §3–4 ★ | `os/libs/minix-platform/src/*`; `os/libs/minix-boot/src/platform.rs` | How does the rewrite avoid hardcoded addresses? |
| K-024 | init_clock (jiffies, uptime base) | M | S | 05 §2.1 ★ | `minix3/minix/kernel/clock.c:init_clock` | What is time zero? |
| K-025 | intr_init 8259/APIC vs ARM GIC | M | S | 05 §2.2–2.3 | `arch/i386/i8259.c:intr_init`; `arch/earm/.../omap_intr.c` | How are IRQs routable before procs exist? |
| K-026 | arch_init per-arch tail | M | S | 05 §2.4–2.5 | `arch/i386/arch_system.c`; `arch/earm/arch_system.c` | What is left for each arch? |
| K-027 | EarlyConsole trait | E | S | 05 §3.4 | `os/plat/src/early_console.rs` | How does the kernel print before drivers? |
| K-028 | ClockArch / InterruptController / ArchInit traits | E | S | 05 §3.1–3.5 | `os/arch/src/arch/clock.rs,arch_init.rs` | How is init hardware abstracted? |
| K-029 | Sync vs async (exception vs interrupt) model | C | S | 05 §1.0, 14 §1.1 ★ (dup, main=05) | OS theory | Which events can be retried? |
| K-030 | ClockState software model | D | S | 05 §4.1, 15 §1 | `os/kernel/src/clock.rs:ClockState` | What clock state is arch-independent? |

### Proc, priv, bootstrap (K-031–K-044)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-031 | struct proc field groups (sched/IPC/mem/signal/debug) | D | S | 06 §2.1/§6.1 ★ | `minix3/minix/kernel/proc.h`; `proc.c` | What lives in one proc row? |
| K-032 | Endpoint + generation anti-reuse | M | S | 99 §1 ★ | `minix3/minix/kernel/const.h`; `include/minix/endpoint.h` | Why can't a stale endpoint hit a new proc? |
| K-033 | RTS flag machine (SENDING/RECEIVING/NO_QUANTUM/VMINHIBIT/BOOTINHIBIT/PROC_STOP/SLOT_FREE/SIGNALED/VMREQUEST/NO_PRIV…) | D/V | S | 06 §1.3, 11 §1 ★ (main=11 for runtime, 06 for boot) | `minix3/minix/kernel/proc.h:RTS_*`; `const.h` | What does each blocked state mean? |
| K-034 | Boot inhibition triple (VMINHIBIT/BOOTINHIBIT/PROC_STOP) + VM exception | M | S | 06 §1.5, main.c loop | `minix3/minix/kernel/main.c:kmain` L~265–280 | Why can only VM+RS run first? |
| K-035 | struct priv (s_flags/s_trap_mask/s_ipc_to/s_k_call_mask/s_sig_mgr…) | D | S | 06 §2.2/§6.2, 22 §1.1 ★ (main=22) | `minix3/minix/kernel/priv.h`; `include/minix/priv.h` | How are capabilities stored? |
| K-036 | get_priv static/dynamic allocation | M | S | 06 loop, 22 §1 | `minix3/minix/kernel/system.c:get_priv` | How does a proc gain a priv slot? |
| K-037 | fill_sendto_mask + s_k_call_mask fill | M | S | 06 loop, 22 §1 | `minix3/minix/kernel/system.c:fill_sendto_mask,set_sendto_bit` | How are send/call rights granted at boot? |
| K-038 | CapabilityTemplate / kpriv.rs model | E | S | 06 §3.2, 22 §4 | `os/kernel/src/kpriv.rs`; `capability.rs` | How are 6 bare C params typed? |
| K-039 | ProcessTable fixed array + index (not pointer) | E | S | 06 §3.1 | `os/kernel/src/proc_table.rs` | Why indices instead of `proc_addr` pointers? |
| K-040 | KProcess ownership model | D | S | 06 §3.1.1 | `os/kernel/src/proc_table.rs:KProcess` | Who owns proc resources? |
| K-041 | CpuContextArch trait (arch CPU state) | E | S | 06 §3.5 | `os/arch/src/arch/*` | How is context-switch state abstracted? |
| K-042 | arch_boot_proc ELF load of VM | M | S | 06 §3.7 | `arch/i386/memory.c:arch_boot_proc` | How is the first user image entered? |
| K-043 | Zero-heap boot path | V | S | 06 §3.8 | `os/kernel/src/*` (boot_alloc) | How does boot avoid allocation? |
| K-044 | USER_PRIV sharing for user procs | M | S | 22 §1 | `minix3/minix/kernel/priv.h`; `table.c` | Why do all users share one priv? |

### Address spaces (K-045–K-051)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-045 | Borrowed-pagetable model (kernel has no own tables) | C | S | 07 §1.1 ★ | `arch/i386/memory.c`; `protect.c` + OS theory | How can the kernel touch other procs' memory? |
| K-046 | freepdes/ptproc temp windows (32-bit) | M | S | 07 §1.2/§2.4–2.5 | `arch/i386/memory.c:createpde`; `pg_utils.c:pg_mapkernel` | What did the old trick look like? |
| K-047 | direct_map replacement (64-bit) | E | S | 07 §1.3/§3.1 ★ | `02-stage-vm/07-pagetable-struct.md §3.2`; `os/kernel/src/vm.rs` | Why is temp mapping gone? |
| K-048 | arch_post_init + pg_info/pg_mapkernel/memory_init/add_memmap chain | M | S | 07 §2, 08 §2.3 | `arch/i386/protect.c,pg_utils.c,memory.c`; `main.c` | What builds unmaps what, in which order? |
| K-049 | DirectMapArch trait boundary | E | S | 07 §3.3 | `os/arch/src/arch/*` | Where does kernel mapping come from now? |
| K-050 | map_kernel responsibility evolution | E | S | 07 §3.4 | `02-stage-vm/08-pagetable-ops.md §3.4` | Who builds the kernel window now? |
| K-051 | Stage-D simplification (confirm DM ready) | V | S | 07 §1.4/§3.5 | `os/kernel/src/lib.rs:init_post_and_memory` | What remains of stage D? |

### System init & first schedule (K-052–K-058)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-052 | system_init call_vec 40-map table | P | S | 08 §2.3, 13 §1 ★ (main=13 for dispatch use) | `minix3/minix/kernel/system.c:system_init` | Which handlers exist? |
| K-053 | bsp_finish_booting beat (bill/idle/announce/timers/FPU/alloc-off) | M | S | 08 §1.2 ★ | `minix3/minix/kernel/main.c:bsp_finish_booting`; `os/kernel/src/lib.rs:1948` | What is the last boot function? |
| K-054 | T0–T6 Rust phase split | E | S | 08 §1.1 | `os/kernel/src/lib.rs:744–2757` | How is C's tail redistributed? |
| K-055 | switch_to_user funnel + NOT_REACHABLE/never-return | M | S | 10 §1.1–1.2 ★ | `minix3/minix/kernel/proc.c:switch_to_user`; `os/kernel/src/lib.rs:2757` | How does the kernel leave to user? |
| K-056 | Idle proc + bill_ptr accounting | M | S/N | 10 (partial), main.c | `minix3/minix/kernel/proc.c:176-229`; `main.c:bsp_finish_booting` | Who runs when nobody is runnable? |
| K-057 | enqueue/dequeue/pick_proc + runqueue invariant | M/V | S | 11 §1 ★ | `minix3/minix/kernel/proc.c:enqueue,pick_proc` | How is "in-queue iff runnable" kept? |
| K-058 | proc_no_time / quantum expiry path | M | S | 11, 15 | `minix3/minix/kernel/proc.c:1893-1910` | What happens when quantum hits zero? |

### IPC & trap/dispatch (K-059–K-070)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-059 | 6-primitive blocking matrix (SEND/SENDREC/RECEIVE/SENDNB/NOTIFY/SENDA) | C/M | S | 12 §1.2 ★ | `minix3/minix/kernel/proc.c:do_ipc` | Which primitive blocks when? |
| K-060 | delivermsg deferred-copy path | M | S | 12, 13 | `minix3/minix/kernel/proc.c:delivermsg` | When is the message actually copied? |
| K-061 | deadlock() cycle detection | M | S | 12 | `minix3/minix/kernel/proc.c:deadlock` | How are A→B→A waits rejected? |
| K-062 | mini_notify + HARDWARE source | M | S | 12, 14, 20 | `minix3/minix/kernel/proc.c:mini_notify` | How do IRQs wake drivers? |
| K-063 | SENDA async batch + MF_SENDA_VM_MISS | M | S | 12 | `minix3/minix/kernel/proc.c` L~795–820 | How are async sends batched? |
| K-064 | 3 entry legs + 1 exit (trap model) | C | S | 14 §1.2 | `arch/i386/exception.c`; `kernel/interrupt.c` | How does the CPU enter/leave the kernel? |
| K-065 | IDT/IST/MSR_LSTAR + int32/int33 vectors | M | S | 14, trap-bridge memo | `arch/i386/include/hw_intr.h:31-35`; `protect.c:147`; `usermapped_glo_ipc.S` | How are vectors 32/33/34/35 split? |
| K-066 | int33 trap bridge (regs→context→dispatch→sched-only exit) | E | S | 18-trap-bridge memo ★ | `os/arch/src/x86_64/trap_entry.rs`; `os/kernel/src/trap_dispatch.rs` | How does user IPC reach the kernel now? |
| K-067 | SYSCALL leg (RDI msgptr → kernel_call → RAX reply; NoReply panics) | E | S | trap-bridge memo R2 | `os/kernel/src/trap_dispatch.rs:179-236` | Why can't SYSCALL block? |
| K-068 | kernel_call_dispatch (range + mask → EBADREQUEST/ECALLDENIED) | M | S | 13 §1 ★ | `minix3/minix/kernel/system.c:kernel_call_dispatch` | How are bad/denied calls rejected? |
| K-069 | kernel_call_finish (VMSUSPEND save / EDONTREPLY / copy-back + SIGSEGV on bad ptr) | M | S | 13 §1, 24 §1 | `minix3/minix/kernel/system.c:kernel_call_finish` | How do suspend/noreply/copy-back work? |
| K-070 | caller-by-nr vs borrowed-row (caller_slot_mut) | E | S | 33 ★ | `os/kernel/src/proc_table.rs:caller_slot_mut`; `do_schedule.c:8`; `proc.h:269` | How is aliasing made explicit? |

### Syscall role groups (K-071–K-086)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-071 | fork (sync premise + generation + deprivilege) | M | S | 17 §1 ★ | `system/do_fork.c:do_fork` | What makes fork atomic? |
| K-072 | exec image replacement + arch_proc_init IP/SP | M | S | 17 | `system/do_exec.c:do_exec` | Why is exec not "create"? |
| K-073 | exit via delegated SIGABRT + EDONTREPLY | M | S | 17 | `system/do_exit.c:do_exit` | Why does exit not reply? |
| K-074 | clear idempotent reclaim | M/V | S | 17 | `system/do_clear.c:do_clear` | Why is double-clear safe? |
| K-075 | schedctl dual-mode + sched_proc alignment | M | S | 11/17 | `system.c:sched_proc`; `system/do_schedctl.c` | How are scheduler params set? |
| K-076 | vircopy/physcopy trusting-copy | M | S | 18 §1.1 ★ | `system/do_copy.c:do_copy` | When is copy permission-free? |
| K-077 | safecopy grant-checked copy + grant table | M/P | S | 18 | `system/do_safecopy.c`; `include/minix/safecopies.h` | How do untrusted procs share buffers? |
| K-078 | umap/umap_remote/vumap translation queries | M | S | 18 | `system/do_umap.c,do_umap_remote.c,do_vumap.c` | How are addresses translated for DMA? |
| K-079 | VMCTL negotiation rounds (SETADDRSPACE/KERN_PHYSMAP/…) | P | S | 09 §1 ★ | `system/do_vmctl.c`; `arch/i386/arch_do_vmctl.c` | How does VM take over paging? |
| K-080 | VMREQUEST suspend-notify-resume + MF_KCALL_RESUME | M | S | 24 §1 ★ | `kernel/proc.c:vm_suspend`; `kernel/system.c:kernel_call_resume`; `kernel/vm.h` | What happens on borrow-time page faults? |
| K-081 | Kernel signals path (cause_sig→GETKSIG→ENDKSIG push+pull) | M | S | 19 §1.1 ★ | `system/do_kill.c,do_getksig.c,do_endksig.c`; `system.c:cause_sig` | How are kernel signals delivered? |
| K-082 | POSIX signals path (SIGSEND frame→SIGRETURN restore) | M | S | 19 | `system/do_sigsend.c,do_sigreturn.c` | How are user handlers run? |
| K-083 | Clock tick duties (jiffies, timers, alarms, loadavg) | M | S | 15 §1 ★ | `minix3/minix/kernel/clock.c:timer_int_handler` | What happens every tick? |
| K-084 | Clock syscalls (TIMES/SETALARM/STIME/SETTIME/VTIMER) | P | S | 21 §1 | `system/do_{times,setalarm,stime,settime,vtimer}.c` | How is time queried/set? |
| K-085 | IRQ hooks (register/enable/disable → HARDWARE notify) | M | S | 20 §1.1 ★ | `system/do_irqctl.c`; `kernel/interrupt.c` | How do drivers receive IRQs? |
| K-086 | Port I/O family (devio/vdevio/sdevio/iopenable/readbios) + x86-only BadCall | M/V | S | 20 | `system/do_devio.c,do_vdevio.c`; `arch/i386/do_sdevio.c,do_iopenable.c,do_readbios.c` | How is HW access fenced per arch? |

### Concurrency, FPU, observability, retired (K-087–K-104)

| ID | Name | Type | Src | Old location(s) | Anchor | Reader benefit |
|----|------|------|-----|-----------------|--------|----------------|
| K-087 | BKL coarse serialization + no-sleep rule | V | S | 16 §1.1 ★ | `kernel/smp.c`; `kernel/spinlock.h`; `os/kernel/src/smp.rs` | Why is only one CPU in-kernel? |
| K-088 | Per-CPU data (cpulocals, proc_ptr, bill_ptr, fpu_owner) | D | S | 16 | `kernel/cpulocals.c,h`; `proc.h` | What state is per-CPU? |
| K-089 | AP bring-up handshake + IPI scheduling | M | S | 16 | `kernel/smp.c:smp_init`; `arch/i386/arch_smp.c`; `apic.c` | How do APs join? |
| K-090 | FPU lazy model (#NM trap, TS bit, save/restore) | M | S | 31 §1 ★ | `arch/i386/arch_system.c` fpu family; `proc.c:copr_not_available_handler`; `arch/i386/mpx.S`; `os/arch/src/arch/fpu_arch.rs` | Why is FPU state not switched eagerly? |
| K-091 | debug.c conditional arsenal (runqueues_ok/print_proc/IPC hooks/stats) | T | S | 29 §1 ★ | `minix3/minix/kernel/debug.c` (~563 lines) | How does the kernel check itself? |
| K-092 | profile.c sampling + SPROFILE + NMI sampler | T | S | 30 §1 ★ | `minix3/minix/kernel/profile.c` (157 lines, `#if SPROFILE`) | How is kernel time attributed? |
| K-093 | proc_stacktrace frame-pointer walk + cross-space read | M | S | 32 §1 ★ | `arch/i386/exception.c:proc_stacktrace`; `system/do_diagctl.c`; `utility.c:util_stacktrace` | How are backtraces produced? |
| K-094 | GETINFO/DIAGCTL/ABORT/SPROF misc surface | P | S | 25 | `system/do_getinfo.c,do_trace.c,do_update.c,do_sprofile.c` | Which misc calls stay ported? |
| K-095 | panic/kputc/kmess/_exit utility trio | M | S | 27 §1 ★ | `minix3/minix/kernel/utility.c` (~93 lines) | How does the kernel report its own death? |
| K-096 | NMI watchdog WONTFIX (partial-arch + no unified trait) | V | S | 26 ★ | `minix3/minix/kernel/watchdog.c,h` (~162 lines) | Why is lockup detection not ported? |
| K-097 | .usermapped read-only window WONTFIX (→ syscall fetch) | E/V | S | 28 ★ | `kernel/usermapped_data.c`; `arch/i386/usermapped_data_arch.c,usermapped_glo_ipc.S,kernel.lds` | Why is shared-window retired? |
| K-098 | Shutdown path (prepare_shutdown→minix_shutdown→arch_shutdown) | M | N | (gap; scattered) | `minix3/minix/kernel/main.c:prepare_shutdown,minix_shutdown` | How does the kernel halt/reboot? |
| K-099 | Unported/deferred set (TRACE/UPDATE/MCONTEXT/PADCONF/ACPI-consumer/U-Boot) | V | S/N | 25, todo I-1/I-5/T-10 | `system/do_trace.c,do_update.c,do_mcontext.c`; `arch/earm/*` | What is deliberately left out? |
| K-100 | errno mapping discipline (no invented codes) | V | S | todo D1/D2 | `os/kernel/src/errno.rs`; `minix3` errno values | Which error values are legal? |
| K-101 | no_std + trait-hardware rule (no CR3/PTE bits in OS layer) | V | S | 00 §5, AGENTS.md | `os/kernel/src/*`; `os/arch/src/*` | What are the two hard layering rules? |
| K-102 | Image layout + build/test rig (boot.cfg, Makefiles, qemu-tests, gen-test-kernel.sh) | T | N | checklist (partial) | `minix3/etc/boot.cfg.default`; `minix3/minix/kernel/Makefile`; `os/qemu-tests/*`; `tools/gen-test-kernel.sh` | How is a test kernel built and run? |

Statistics: 102 items; by type C:9 M:47 D:10 P:10 V:15 E:13 T:5 S:0(+test notes inside T);
by source stock:88 / new:14 (K-012, K-018, K-056-part, K-098, K-099-part, K-102,
plus gap-driven additions in §3); by old-doc spread: boot docs (01–03) 20 items,
init docs (04–08) 22, runtime (09–16) 30, syscalls (17–24) 20, infra (25–33/99) 10.
Main-telling-point dups resolved: K-029→05, K-033 split 06(boot)/11(runtime),
K-035→22, K-047→07, K-052→13, K-066→memo distilled into N10 (memo archived, not moved).

---

## 3. Coverage Audit

### 3.1 Theme universe (four sources)

U-C (C symbols, from `system_init` map + core files + arch files, cross-checked
with `checklist.md`'s ~260-symbol count): all 40 `call_vec` handlers; proc.h
(RTS_*, priv, cpulocals); glo.h (kinfo, kmess, krandom, bill_ptr, kernel_may_alloc);
const.h/config.h/type.h; clock.c (init_clock, timer_int_handler, set_kernel_timer);
interrupt.c (intr_handle, generic_handler, irq_hooks); proc.c (all IPC + sched);
system.c (dispatch + get_priv + sendto + cause_sig + sched_proc); profile.c;
debug.c; utility.c; watchdog.c; usermapped_data.c; memory.c (data_copy,
virtual_copy, arch_proc_init, arch_boot_proc); protect.c (prot_init,
arch_post_init); exception.c (handlers + proc_stacktrace + fpu on/off);
pre_init.c; pg_utils.c; arch_system.c (arch_init, fpu family); arch_clock.c;
arch_smp.c; arch_do_vmctl.c; arch_watchdog.c; i8259.c; apic.c + apic_asm.S;
head.S; klib.S; mpx.S; trampoline.S; io_*.S (8); debugreg.S; usermapped_glo_ipc.S;
kernel.lds; breakpoints.c; direct_tty_utils.c; oxpcie.c; acpi.c; debugreg.h;
sconst.h; earm branch (arch_system, protect, bsp/ti/omap_*, padconf).
U-OS (theory concepts): privilege rings, address spaces, scheduling invariants,
IPC blocking semantics, deadlock, signals, time, SMP coherence, lazy FPU,
sampling profilers, shutdown.
U-NONC (artifacts §0): link scripts, asm entries, boot chain+protocol, image
layout, user runtime load, cross-module wire (com.h/ipc.h/message.h), build
scripts+toolchain, test rig+simulator scripts.
U-BOUND (belongs-to-kernel per master plan + edge_todo): kernel boot through
first user schedule; VM handshake kernel side; trap bridge kernel side (E1);
shared errno/types; NOT kernel: VM pagetable policy, RS loading, PM/VFS logic.

### 3.2 Coverage gaps (theme present, no doc owns it) → disposition

| Gap | Evidence | Disposition → new owner |
|-----|----------|-------------------------|
| G-01 shutdown/halt path | `main.c:prepare_shutdown/minix_shutdown`; zero dedicated sections | NEW section in N18 + K-098 added |
| G-02 do_abort/do_update/do_trace/do_mcontext content | `system/do_{abort,update,trace,mcontext}.c`; only named in 25/checklist | Cover in N12 (trace/update/mcontext) + N18 (abort) |
| G-03 earm branch + PADCONF + omap BSP detail | `arch/earm/**`; only checklist rows | Explicit ARCH-note boxes in N04/N15 + K-099; no new chapter (bounded) |
| G-04 spinlock.h + cpulocals + bill_ptr accounting | `spinlock.h`; `cpulocals.c/h`; `main.c:bill_ptr` | Cover in N16 (lock+percpu) + N09 (billing) |
| G-05 glo.h/kinfo/kmess/krandom/kerninfo consumers | `glo.h`; `usermapped_data.c`; only scattered | Cover in N01 (kinfo) + N17 (kmess) + N18 (krandom note) |
| G-06 oxpcie/breakpoints/debugreg/direct_tty/trampoline/klib/io-asm | arch files; only filenames mentioned | Cover in N10 (entry asm) + N17 (breakpoints/debugreg) + N18 (direct_tty/oxpcie note) |
| G-07 grant-table lifecycle (SETGRANT→safecopy verify→revoke) | `do_setgrant.c` + `do_safecopy.c`; split across 18/24 | Unify in N13 + K-077 extended |
| G-08 SENDA batch end-to-end + IPC_STATUS_REG reporting | `proc.c` SENDA arms; `ipc.h:25-48`; L3 only in 23 | Cover in N11 (SENDA) + N10 (status reg) |
| G-09 FPU lazy-RESTORE main path + signal-path save | 31 covers switch-in trap; restore + sigsave thin | Complete in N17(fpu doc keeps both legs) |
| G-10 Image/build/test story (boot.cfg, Makefiles, qemu-tests, gen-test-kernel) | artifacts exist; no chapter | NEW appendix N18-D + K-102 added |
| G-11 VMINHIBIT vs BOOTINHIBIT vs PROC_STOP distinction | main.c loop; 06 lumps them | Split table in N06 + K-034 refined |
| G-12 EFAULT_SRC/DST + copy-failure taxonomy | `vm.h`; `memory.c:data_copy` | Cover in N13 + K-080 |

All 12 gaps become NEW knowledge items or NEW sections; none is declared
out-of-stage, none left "to be determined".

### 3.3 Duplicates (same theme expanded in ≥2 docs) → keep main, others cite

| Theme | Occurrences | Keep (main) | Others become |
|-------|-------------|-------------|---------------|
| panic | 06, 14, 19, 27 | N17 (utility) | one-line refs |
| RTS flags full table | 06, 11, 99 | N09 (runtime) + N06 (boot triple) | 06 keeps boot-only triple |
| BKL story | 14, 16, 20 | N16 | 2-line refs |
| direct_map | 02, 07, 09, 18, 24 | N07 | refs |
| VMCTL rounds | 08, 09, 18, 24 | N13 | refs |
| profiling/sprofile | 25, 30 | N17 | 25-row retired in N18 |
| clock init vs tick | 05, 15, 21 | N05 (init) / N15 (tick+syscalls) | split by phase, not by file |
| protection concept | 03, 14 | N03 | N10 cites |
| PlatformDesc pitch | 04 (×7 sections) | N04 slimmed | cut 60% repetition |
| Higher-half jump | 01, 02 | N02 | 01 cites |

### 3.4 Overreach (doc teaches outside its declared boundary) → correct owner

| Doc | Overreach | Correct owner |
|-----|-----------|---------------|
| 04 | Teaches Rust `PlatformDesc` as if it were C behavior; C `acpi_init/bsp_init` thin | N04: C-first (2 sections), evolution second (1 section) |
| 09 | Teaches Rust `map_kernel` design from `02-stage-vm` | N13 (VMCTL) + N07 (DM); N09 drops it |
| 07 | Normative refs to `02-stage-vm/07-pagetable-struct.md` as authority | N07: C-first, VM docs cited as evolution only |
| 28 | Teaches Rust `KernelInfo/ClockState` as replacement without C mechanism first | N18: C mechanism first, retirement reason second |
| 05/15/21 | Triple-teach tick (init/tick/syscall split by file, not by phase) | N05 init-only; N15 tick+syscalls |
| 06-todo/07-paging memo/18-trap memo/33 | Process memos filed as numbered chapters (collide: dual 07s, dual 18s) | Archived (not rebuilt); content distilled into N07/N10/N11 contracts |

### 3.5 Non-C topics: fixed 10-item answers (none left blank)

| Topic | Answer: where taught or why not in stage |
|-------|------------------------------------------|
| Link & load | N02 (kernel.lds, ELF, HigherHalf) |
| Image & memory layout | N02 (link layout) + N07 (runtime windows) + N18-D (boot.cfg/modules/test image) |
| Asm entry & trap ingress | N10 (vectors, stubs, gates, SYSCALL leg) |
| Boot assembly | N01 (GRUB/multiboot/pre_init) + N02 (head.S) |
| Build & toolchain | N18-D appendix (Makefiles, arch flags, gen-test-kernel.sh) |
| Cross-module iface & wire | N01 (KernelInfo) + N10/N11 (message/com.h/ipc.h, KERNEL_CALL=0x600) |
| Error paths | Per-group (N12–N15: EBADREQUEST/ECALLDENIED/EFAULT/SIGSEGV) + N13 (EFAULT taxonomy) |
| Shutdown & exit | N18-C (prepare_shutdown path) + N12 (do_exit/clear) |
| Concurrency & sync | N16 (BKL, percpu, IPI, APs) |
| Test infrastructure | N18-D (qemu-tests rig, test-*-sh scripts, QEMU monitor assertions) |

---

## 4. New Catalog

19 chapters (00–18). Count change 34 mixed items → 19 reader chapters is
deliberate: collisions removed (dual 07/dual 18), memos archived, WONTFIX
clustered, syscall handlers grouped by role. Length policy: one chapter = one
semantic unit; complex chapters (N06/N11/N13) may run long (soft limit ~3000
lines accepted) but must stay single-semantic; nothing is split by source-file
alphabet.

| New | Title | One-line定位 | Group |
|-----|-------|--------------|-------|
| N00 | kernel-overview | What the kernel is, its lifecycle map, how to read this stage | Front door |
| N01 | firmware-to-shim | From power-on to `kmain(kinfo)`: firmware, GRUB, pre_init, boot-shim, KernelInfo | A·Boot |
| N02 | elf-load-higher-half | Link, load, and the paired jump into the higher half | A·Boot |
| N03 | kmain-protection | kmain/cstart/prot_init: building a trustworthy execution context | A·Boot |
| N04 | platform-discovery | How the kernel learns its hardware (ACPI/DT → descriptors) | A·Boot |
| N05 | clock-irq-init | init_clock + intr_init + arch_init: becoming event-capable | A·Boot |
| N06 | proc-priv-bootstrap | Tables, privileges, and why only VM+RS may run first | B·Birth |
| N07 | address-spaces-boot | Borrowed pagetables: temp windows → direct_map | B·Birth |
| N08 | system-init-first-schedule | system_init, reclaim, bsp_finish_booting, first pick | B·Birth |
| N09 | scheduling | Queues, pick_proc, quantum, idle, run/sched controls | C·Run |
| N10 | trap-entry-dispatch | Entering the kernel: vectors, stubs, SYSCALL leg, dispatch, caller model | C·Run |
| N11 | ipc-core | The six primitives, deadlock, notify, SENDA | C·Run |
| N12 | process-lifecycle-syscalls | fork/exec/exit/clear + trace/update/mcontext/privctl | D·Handlers |
| N13 | memory-copy-vm-protocol | Copy family + grants + VMCTL + VMREQUEST suspend/resume | D·Handlers |
| N14 | signals | Kernel-signal path + POSIX-signal path | D·Handlers |
| N15 | time-irq-device | Tick duties + clock syscalls + IRQ hooks + port I/O | D·Handlers |
| N16 | smp-bkl | BKL, per-CPU, APs, IPIs, no-sleep rule | E·Concurrency |
| N17 | fpu-observability | Lazy FPU + debug + profile + stacktrace + diag + panic/kmess | E·Introspect |
| N18 | retired-appendix | WONTFIX (watchdog/usermapped/deferred) + shutdown + build/test/image | F·Retired/Appx |

Reading paths: main line N00→N08 (boot→birth, must read in order); run line
N09→N11 (scheduler→entry→IPC, in order); handler groups N12–N15 skimmable by
role after N10 (each states its privilege + trigger); E/F skippable
(N16 required for SMP builds; N17 reference-style; N18 appendix/WONTFIX first).
Parallel-theme rule: the ~40 handlers are never listed linearly; N10 gives the
common dispatch frame, N12–N15 group by role with one worked member each
(fork / safecopy / SIGSEND / IRQ-hook) and difference tables for the rest.

Order-difference table (teaching order vs runtime order; every deviation logged):

| # | Runtime fact (anchor) | Teaching choice | Compensation back-ref |
|---|-----------------------|-----------------|-----------------------|
| D-1 | Traps (R2) precede first schedule (R1) causally | N09 (schedule) before N10 (entry) | N09 §1 states "entry legs deferred to N10"; N10 §1 restates the funnel |
| D-2 | Privilege masks (K4) are used by dispatch (R4) | N06 (bootstrap) before N10 (dispatch) | N10 cites N06 masks, never re-explains |
| D-3 | VMCTL (R6) happens before scheduler steady-state | N13 (handlers) after N11 (IPC) | N08 previews the 4-round handshake; N13 gives full protocol |
| D-4 | FPU init (K7) precedes first schedule; #NM fires late | N17 (FPU) after N16 (SMP) | N08 notes fpu_init as a beat; N17 owns the mechanism |

---

## 5. Per-Chapter Contracts (all 19; missing one blocks delivery)

### N00-kernel-overview

- One-line定位: the entry point that tells a new reader what the kernel is,
  its 6-beat lifecycle, and which chapter to open for which question.
- 讲什么: K-011 (table vs execution order), K-013 (trust boundary), K-029
  (sync/async), K-055 (funnel), K-101 (no_std + trait-hardware rules), catalog map.
- 不讲什么: boot-shim internals (→N01); paging levels (→N07); any handler (→N12–N15);
  SMP policy (→N16). No mechanism taught here beyond names.
- 前置: none (first chapter; assumes C + Rust literacy only).
- 后置: every chapter cites N00's lifecycle map; N06/N10/N13 cite the VM-split figure.
- 事实底线: `minix3/minix/kernel/main.c:kmain,bsp_finish_booting`;
  `minix3/minix/kernel/table.c:44-64`; `00-master-plan/README.md` (stage order).
- 知识点清单:
  | K-011 | table vs execution order | C | master-plan + table.c | frames the whole stage | S:00 §3 |
  | K-013 | trust boundary | C | protect.c | motivates N03 | S:00 §1/03 §1.1 |
  | K-101 | layering rules | V | AGENTS.md | gates all code claims | S:00 §5 |
- 验收标准: reader can draw the 6-beat chain from memory; can answer
  "why is the kernel not an event loop like VM?" without opening another chapter.

### N01-firmware-to-shim

- One-line定位: how control reaches `kmain` with a valid `kinfo`.
- 讲什么: K-001, K-002, K-003, K-004, K-005, K-018 (boot vars subset).
- 不讲什么: ELF segment copying (→N02); paging levels (→N07); ACPI parsing (→N04).
- 前置: N00.
- 后置: N02 (takes kinfo + loaded ELF), N04 (consumes descriptors), N06 (consumes modules).
- 事实底线: `arch/i386/pre_init.c`; `arch/i386/pg_utils.c`; `etc/boot.cfg.default`;
  `os/boot-shim/src/{main,loader,uefi_helpers,opensbi_helpers}.rs`.
- 知识点清单: K-001..K-005 + K-018 rows as above; sources S:01 §§1–2, N:boot-shim files.
- 验收标准: reader can trace power-on → `kmain(kinfo)` naming every handoff and
  its data (multiboot struct → kinfo_t → KernelInfo); can state what GRUB is NOT.

### N02-elf-load-higher-half

- One-line定位: how the kernel image is linked, loaded, and entered in the higher half.
- 讲什么: K-006, K-007, K-008, K-009, K-010.
- 不讲什么: module ELFs (→N06/K-012); runtime pagetables (→N07); trap gates (→N10).
- 前置: N01.
- 后置: N03 (starts at kmain), N07 (reuses link layout), N18-D (image appendix cites).
- 事实底线: `arch/i386/kernel.lds`; `arch/i386/head.S`;
  `os/kernel/src/lib.rs:arch_boot_impl,HigherHalf`.
- 知识点清单: K-006..K-010; S:02 §§2/4.
- 验收标准: reader can explain VMA≠LMA, why stack+PC switch as a pair, and what
  "code never moved" means physically.

### N03-kmain-protection

- One-line定位: how the kernel builds a trustworthy CPU context before doing anything else.
- 讲什么: K-013, K-014, K-015, K-016, K-017, K-019, K-020.
- 不讲什么: syscall routing (→N10); trap-frame bytes (→N10); PTE formats (→N07).
- 前置: N02.
- 后置: N04/N05 (build on protection), N10 (reuses gates).
- 事实底线: `main.c:kmain,cstart`; `arch/i386/protect.c:prot_init`;
  `arch/earm/protect.c:prot_init`; `os/arch/src/arch/*ProtectionArch*`.
- 知识点清单: K-013..K-017 + K-019/K-020; S:03.
- 验收标准: reader can list cstart's call order with reasons; can say why x86 keeps a GDT.

### N04-platform-discovery

- One-line定位: where hardware parameters come from, in C first, rewrite second.
- 讲什么: K-021, K-022, K-023 (slimmed to one design section).
- 不讲什么: register-level programming of clocks/IRQs (→N05); timer IRQ gating (→N15).
- 前置: N03.
- 后置: N05 (consumes addresses/frequencies), N16 (consumes topology).
- 事实底线: `arch/i386/acpi.c:acpi_init`; `arch/i386/arch_system.c:arch_init`;
  `arch/earm/arch_system.c,bsp/ti/*`; `os/libs/minix-platform/src/*`.
- 知识点清单: K-021..K-023; S:04 §2 (C) + §3 trimmed.
- 验收标准: reader can name the three arch descriptor sources and the rewrite's
  parse-locate/parse split; no Rust trait taught before its C counterpart.

### N05-clock-irq-init

- One-line定位: how the kernel becomes able to hear hardware before any process exists.
- 讲什么: K-024, K-025, K-026, K-027, K-028, K-030.
- 不讲什么: tick duties/alarms (→N15); IRQ→driver notify path (→N15/N11).
- 前置: N04.
- 后置: N06 (procs born into a ticking world), N15 (owns the tick).
- 事实底线: `kernel/clock.c:init_clock`; `arch/i386/i8259.c:intr_init`;
  `arch/*/arch_system.c:arch_init`; `os/kernel/src/clock.rs:ClockState`.
- 知识点清单: K-024..K-028 + K-030; S:05 §§1–2/4.
- 验收标准: reader can state the three functions' contracts and why they precede proc_init.

### N06-proc-priv-bootstrap

- One-line定位: how processes and privileges come into being, and who may run first.
- 讲什么: K-031, K-032, K-033 (boot triple only), K-034, K-035, K-036, K-037,
  K-038, K-039, K-040, K-041, K-042, K-043, K-044, K-011, K-012.
- 不讲什么: runtime queue mechanics (→N09); mask ENFORCEMENT on trap paths (→N10);
  syscall-by-syscall priv needs (→N12–N15, each states its own).
- 前置: N05.
- 后置: N07/N08 (consume tables), N10/N11 (enforce masks).
- 事实底线: `main.c:kmain` boot loop; `kernel/proc.h`; `kernel/priv.h`;
  `kernel/system.c:get_priv,fill_sendto_mask`; `arch/i386/memory.c:arch_boot_proc`;
  `os/kernel/src/{proc_table,kpriv,capability}.rs`.
- 知识点清单: K-031/K-032/K-034..K-044 + K-011/K-012; S:06 + 22(priv core) + 99(endpoint).
- 验收标准: reader can simulate the boot loop for VM vs a deferred server row by
  row (masks, flags, ELF); can distinguish the three inhibit flags (G-11 closed).

### N07-address-spaces-boot

- One-line定位: how the kernel sees other address spaces, old trick vs new map.
- 讲什么: K-045, K-046, K-047, K-048, K-049, K-050, K-051.
- 不讲什么: per-copy auth (→N13); VM pagetable POLICY (02-stage-vm, cited only).
- 前置: N06.
- 后置: N08 (reclaims windows), N13 (uses direct_map per copy).
- 事实底线: `arch/i386/{protect.c,pg_utils.c,memory.c}` (arch_post_init, pg_info,
  pg_mapkernel, memory_init, createpde); `os/kernel/src/{vm,lib.rs:init_post_and_memory}`.
- 知识点清单: K-045..K-051; S:07-cross-space-init (memo 07-paging distilled, archived).
- 验收标准: reader can contrast freepdes vs direct_map and name what stage D
  still does (confirm-ready + reclaim).

### N08-system-init-first-schedule

- One-line定位: the last boot function and the first pick.
- 讲什么: K-052 (table Gazetteer role only), K-053, K-054, K-055 (first call),
  K-056 (first idle), K-079-preview (4-round handshake summary, full in N13).
- 不讲什么: dispatch semantics (→N10); handler bodies (→N12–N15).
- 前置: N07.
- 后置: N09 (owns the loop), N13 (owns VMCTL).
- 事实底线: `kernel/system.c:system_init`; `arch/i386/pg_utils.c:add_memmap`;
  `main.c:bsp_finish_booting`; `os/kernel/src/lib.rs:744–2757` (T0–T6).
- 知识点清单: K-052..K-056 + K-079-preview; S:08.
- 验收标准: reader can recite bsp_finish_booting's beats and the T0–T6 redistribution.

### N09-scheduling

- One-line定位: who runs next, and what "runnable" means.
- 讲什么: K-033 (runtime machine), K-056, K-057, K-058, K-075 (runctl/schedctl/
  schedule/statectl as queue operations), K-089-part? No (→N16). Billing (K-056 ext).
- 不讲什么: IPC blocking arms (→N11); trap entry (→N10); SMP balancing (→N16).
- 前置: N08.
- 后置: N10 (called at funnel end), N11 (shares RTS machine), N12 (statectl detail cited).
- 事实底线: `proc.c:enqueue,dequeue,pick_proc,proc_no_time,sched_proc`;
  `system/do_{runctl,schedctl,schedule,statectl}.c`; `os/kernel/src/sched.rs`.
- 知识点清单: K-033/K-056/K-057/K-058/K-075; S:10(idle/funnel part)+11+17(schedctl part).
- 验收标准: reader can prove in-queue-iff-runnable maintenance across one block
  and one wakeup; can place every RTS flag on the machine.

### N10-trap-entry-dispatch

- One-line定位: the single door in, the checks at the door, the single funnel out.
- 讲什么: K-064, K-065, K-066, K-067, K-068, K-069, K-070, K-062-part (notify
  wakeups as exit effects), K-086-part (x86-only BadCall rule).
- 不讲什么: handler bodies (→N12–N15); IPC rendezvous (→N11); SENDA batch (→N11).
- 前置: N09.
- 后置: N11–N15 (all start "after N10's checks").
- 事实底线: `arch/i386/exception.c`; `kernel/interrupt.c`; `arch/i386/{mpx.S,
  io_intr.S,usermapped_glo_ipc.S,klib.S,trampoline.S,io_*.S}`;
  `arch/i386/include/hw_intr.h`; `kernel/system.c:kernel_call*`;
  `os/arch/src/x86_64/trap_entry.rs`; `os/kernel/src/{trap_dispatch,syscall}.rs`.
- 知识点清单: K-064..K-070; S:13+14(entry halves)+trap-bridge memo+33 (distilled).
- 验收标准: reader can route one int33 IPC and one SYSCALL kcall end-to-end,
  naming every check, copy, and exit; can explain why SYSCALL never blocks.

### N11-ipc-core

- One-line定位: rendezvous, blocking, deadlock, notification, async batch.
- 讲什么: K-059, K-060, K-061, K-062, K-063, K-080? No (→N13). L1/L2 filter
  USE points only (ownership stays N06/N10).
- 不讲什么: mask DEFINITIONS (→N06); dispatch checks (→N10); grant auth (→N13).
- 前置: N10.
- 后置: N12–N15 (handlers cite rendezvous states), N16 (BKL held across arms).
- 事实底线: `proc.c:do_ipc,mini_send,mini_receive,mini_notify,delivermsg,deadlock`;
  `kernel/ipc.h`; `os/kernel/src/ipc.rs`.
- 知识点清单: K-059..K-063; S:12 + 23-use-points.
- 验收标准: reader can tabulate all six primitives' blocking and walk one
  SENDREC rendezvous + one SENDA batch + one deadlock rejection.

### N12-process-lifecycle-syscalls

- One-line定位: birth, replacement, death, reclaim, and runtime controls.
- 讲什么: K-071, K-072, K-073, K-074, K-075 (handler detail), K-099-part
  (TRACE/UPDATE/MCONTEXT bodies), G-02 bodies.
- 不讲什么: copy mechanics used BY fork/exec (→N13, cited); scheduling queues (→N09).
- 前置: N10 (dispatch), N11 (states).
- 后置: N13 (copies for), N14 (exit→signal handoff cites).
- 事实底线: `system/do_{fork,exec,exit,clear,privctl,trace,update,runctl,statectl,
  schedctl,schedule,mcontext}.c`; `os/kernel/src/syscall_process.rs`.
- 知识点清单: K-071..K-075 + G-02; S:17 + 25(trace/update rows).
- 验收标准: worked FORK + difference tables for the rest; idempotence of CLEAR shown.

### N13-memory-copy-vm-protocol

- One-line定位: moving bytes across spaces safely, and handing paging to VM.
- 讲什么: K-076, K-077, K-078, K-079 (full rounds), K-080 (full protocol),
  K-048-part (reclaim beat cited), G-07/G-08/G-12.
- 不讲什么: VM's pagetable POLICY (02-stage-vm); scheduler use of copies (→N09 cites).
- 前置: N10, N11.
- 后置: N14/N15 (copies cited, never re-explained).
- 事实底线: `system/do_{copy,safecopy,umap,umap_remote,vumap,memset,safememset,
  setgrant,vmctl}.c`; `arch/i386/{memory.c:virtual_copy,arch_do_vmctl.c}`;
  `kernel/{vm.h,system.c:kernel_call_resume,proc.c:vm_suspend}`;
  `os/kernel/src/{syscall_copy,cross_space,vm,grant}.rs`.
- 知识点清单: K-076..K-080; S:18 + 24 + 09 (merged; old 09 archived).
- 验收标准: worked SAFECOPY + VMCTL 4-round trace + one VMSUSPEND resume trace.

### N14-signals

- One-line定位: kernel-mediated signals, both paths.
- 讲什么: K-081, K-082, K-090-part (sigsave cited, owned by N17).
- 不讲什么: exception ORIGINS of cause_sig (→N10 cites); FPU save bytes (→N17).
- 前置: N10, N13 (sigframe copies).
- 后置: N12 (exit handoff), N17 (frame save).
- 事实底线: `system/do_{kill,getksig,endksig,sigsend,sigreturn}.c`;
  `system.c:cause_sig,sig_delay_done`; `os/kernel/src/syscall_signal.rs`.
- 知识点清单: K-081/K-082; S:19.
- 验收标准: worked SIGSEND→SIGRETURN frame trace + kernel-path poll loop diagram.

### N15-time-irq-device

- One-line定位: the hardware-facing syscalls: time, alarms, IRQs, ports.
- 讲什么: K-083, K-084, K-085, K-086, K-025-part (hook storage cited from N05).
- 不讲什么: init of controllers (→N05); scheduler expiry policy (→N09).
- 前置: N05 (init), N10 (dispatch), N11 (notify).
- 后置: N17 (profile sampler cites tick), N18 (deferred ACPI consumer note).
- 事实底线: `kernel/clock.c:timer_int_handler`; `system/do_{times,setalarm,stime,
  settime,vtimer,irqctl,devio,vdevio}.c`; `arch/i386/do_{sdevio,iopenable,
  readbios}.c`; `arch/i386/arch_clock.c`; `os/kernel/src/{clock,syscall_clock,
  syscall_device,irq_manager}.rs`.
- 知识点清单: K-083..K-086; S:15 + 20 + 21 (merged by phase).
- 验收标准: one tick walked (account→alarms→expiry); one IRQ→HARDWARE-notify
  walk; x86-only BadCall table stated.

### N16-smp-bkl

- One-line定位: many CPUs, one kernel at a time.
- 讲什么: K-087, K-088, K-089, K-004? No. AP topology (from N04 cited), no-sleep
  rule with examples, IPI reschedule, G-04 lock+percpu.
- 不讲什么: queue POLICY (→N09); FPU ownership bytes (→N17 cites rule).
- 前置: N09 (queues), N10 (entry per CPU).
- 后置: N17 (FPU owner cites BKL), N18 (SMP-deferred edge items listed).
- 事实底线: `kernel/{smp.c,smp.h,cpulocals.c,spinlock.h}`; `arch/i386/{arch_smp.c,
  apic.c,apic_asm.S}`; `os/kernel/src/smp.rs`.
- 知识点清单: K-087/K-088/K-089; S:16 (+smp_todo open items as Owned-open list).
- 验收标准: reader can state the BKL critical-section rule and give one
  sleep-in-BKL violation example; AP join steps listed.

### N17-fpu-observability

- One-line定位: everything the kernel uses to watch itself (and its FPU).
- 讲什么: K-090 (full lazy model incl. restore + signal legs), K-091, K-092,
  K-093, K-094 (GETINFO/DIAGCTL bodies), K-095, K-062-part? No.
- 不讲什么: retirement verdicts (→N18); WONTFIX advocacy (none; facts first).
- 前置: N10 (entry), N15 (tick), N16 (owner/IPI).
- 后置: N18 (retired items cite these as the live set).
- 事实底线: `arch/i386/arch_system.c` fpu family; `proc.c:copr_not_available_handler`;
  `arch/i386/{mpx.S,exception.c}`; `kernel/{debug.c,profile.c,utility.c}`;
  `arch/i386/exception.c:proc_stacktrace`; `system/do_{getinfo,diagctl,sprofile}.c`;
  `os/{kernel/src/{smp(fpu_owner),stacktrace,debug,misc,clock},arch/src/arch/fpu_arch.rs}`.
- 知识点清单: K-090..K-095; S:27(utility)+29+30+31+32+25(live rows).
- 验收标准: lazy-FPU state diagram (#NM→save→restore); sampling-path diagram;
  stacktrace walk preconditions; panic vs kputc decision stated.

### N18-retired-appendix

- One-line定位: what was deliberately NOT ported, how the kernel stops, and how
  to build and test it. Four fenced sections A–D, each single-semantic internally.
- 讲什么: (A) K-096 watchdog verdict; (B) K-097 usermapped verdict; (C) K-098
  shutdown + K-099 deferred set; (D) K-102 build/test/image appendix.
- 不讲什么: live mechanisms (→N01–N17, cited); advocacy for revival (none; records
  revival preconditions instead).
- 前置: N17 (live set known) — verdicts only make sense after the live set.
- 后置: none (terminal chapter).
- 事实底线: `kernel/{watchdog.c,usermapped_data.c,main.c:shutdown}`;
  `arch/i386/{usermapped_data_arch.c,usermapped_glo_ipc.S,arch_watchdog.c}`;
  `etc/boot.cfg.default`; `kernel/Makefile`; `os/qemu-tests/*`;
  `tools/gen-test-kernel.sh`; `todo.md` I-1/I-5/T-10 + `edge_todo.md` SMP-blocked items.
- 知识点清单: K-096/K-097/K-098/K-099/K-102; S:26+28+25(retired rows)+todo/checklist(open lists).
- 验收标准: each verdict names the C fact + the blocking reason + the revival
  precondition; shutdown traced; a newcomer can build + run one QEMU test from §D alone.

---

## 6. Change Table (old → new; every stock item has a destination or a reasoned delete)

| Op | Type | Old | New | Reason | Items (destination) |
|----|------|-----|-----|--------|---------------------|
| OP-01 | rebuild-slim | 00-kernel-overview (§§1–5) | N00 | Keep entry role; cut handler previews (§3.7–3.9 summarized to map) | K-011,K-013,K-101 → N00 |
| OP-02 | keep-rewrite | 01-boot-shim-bootstrap | N01 | Strong chain; trim HigherHalf overlap (§3.4→N02) | K-001..K-005 → N01 §§1–4 |
| OP-03 | keep-rewrite | 02-higher-half-kernel | N02 | Single semantic already; absorb 01's jump overlap | K-006..K-010 → N02 |
| OP-04 | keep-rewrite | 03-kmain-cstart | N03 | Concept-first works; cut trap/syscall previews | K-013..K-017,K-019,K-020 → N03 |
| OP-05 | slim | 04-platform-discovery | N04 | Cut 60% Rust-trait repetition; C-first reorder | K-021..K-023 → N04 (C §§ then 1 evolution §) |
| OP-06 | split-by-phase | 05-clock-interrupt-init | N05 (init) + N15 (tick use) | Init vs steady-state confused | K-024..K-028,K-030 → N05; tick duties → N15 |
| OP-07 | merge-absorb | 06-proc-init-boot-proc + 22-privilege(core) + 99 (endpoint) | N06 | Proc+priv inseparable at boot; 99 stub dissolved | K-031/K-032/K-034..K-044,K-011,K-012 → N06 |
| OP-08 | distill | 07-paging_init_gpt.md (memo) | N07 §3 box (decision record, not body) | Memo is not a reader chapter | Decision content → 1 box; no sections moved |
| OP-09 | keep-rewrite | 07-cross-space-init | N07 | Core contrast solid; drop 02-stage-vm normative refs | K-045..K-051 → N07 |
| OP-10 | keep-rewrite | 08-system-init-boot-finish | N08 | Beats solid; VMCTL preview fenced to 1 § | K-052..K-056,K-079-prev → N08 |
| OP-11 | archive-merge | 09-vm-boot-protocol | N13 §§ (handshake) + N08 § (preview) | Standalone handshake chapter caused forward-ref (old 09 required 10) | K-079,K-080-use → N13 |
| OP-12 | split | 10-switch-to-user (funnel + idle + queues) | N08 (first call) + N09 (loop+queues+idle) | Funnel vs policy mixed | K-055/K-056 → N08+N09; K-057/K-058 → N09 |
| OP-13 | split | 11-scheduling-primitives (machine + schedctl detail) | N09 (machine+queues) + N12 (schedctl body) | Mechanism vs handler mixed | K-033/K-057/K-058 → N09; K-075 body → N12 |
| OP-14 | keep-rewrite | 12-ipc-core | N11 | Strong; L1/L2 defs fenced to cites | K-059..K-063 → N11 |
| OP-15 | split-distill | 13-syscall-dispatch + 33-syscall-caller-api | N10 (dispatch frame + caller model §) | 33 is a section, not a chapter | K-068/K-069 → N10; K-070 → N10 §caller |
| OP-16 | split-distill | 14-exception-interrupt + 18-trap-bridge-design.md | N10 (entry+vectors+bridge) | Memo archived; entry halves unified | K-064..K-067 → N10 |
| OP-17 | split-by-phase | 15-clock-timer (tick) + 21-syscall-clock | N15 (unified time) | File-split, not phase-split | K-083/K-084 → N15 |
| OP-18 | keep | 16-smp | N16 | Keep; add G-04 lock+percpu(bytes from checklist) | K-087..K-089 → N16 |
| OP-19 | keep-rewrite | 17-syscall-process | N12 | Add G-02 bodies (trace/update/mcontext) | K-071..K-075 → N12 |
| OP-20 | merge | 18-syscall-copy + 24-cross-space-runtime | N13 (with 09) | Copy auth and suspend/resume inseparable | K-076..K-078,K-080 → N13 |
| OP-21 | keep | 19-syscall-signal | N14 | Keep; FPU-save fenced to cite | K-081/K-082 → N14 |
| OP-22 | merge | 20-syscall-device (+21 merged above) | N15 | Hardware-facing syscalls share IRQ/clock priv checks | K-085/K-086 → N15 |
| OP-23 | split | 22-privilege (defs→N06, enforcement→N10, filter→N11) | N06+N10+N11 | One chapter taught struct+use+chain together | K-035..K-037 → N06(defs)+N10(L1 gate)+N11(L2 chain) |
| OP-24 | dissolve | 23-ipc-filter | N10 § + N11 § (use points) | No standalone filter chapter; L3 status → N10 | L1/L2/L3 → use points |
| OP-25 | split | 25-misc-unported (live vs retired rows) | N12/N17/N18 (by row) | Bundle of unrelated fates | K-094 live → N17; retired rows → N18; trace/update → N12 |
| OP-26 | archive-verdict | 26-watchdog | N18-A | WONTFIX memo → verdict section | K-096 → N18-A |
| OP-27 | merge-live | 27-kernel-utility | N17 (live trio) | Panic/kmess are introspection | K-095 → N17 |
| OP-28 | archive-verdict | 28-usermapped-data | N18-B | WONTFIX → verdict section | K-097 → N18-B |
| OP-29 | merge-live | 29-kernel-debug + 30-kernel-profile + 32-stack-tracing | N17 | Observability cluster belongs together | K-091/K-092/K-093 → N17 |
| OP-30 | keep | 31-fpu-context-switching | N17 §§ (FPU leg) | FPU is introspection-adjacent; stays whole inside N17 | K-090 → N17 (full, incl. G-09 restore leg) |
| OP-31 | archive | 06-todo.md, smp_gpt*.md, endpoint_todo.md (process memos) | Contracts cite decisions only | Not reader docs | Cited, not moved |
| OP-32 | dissolve | 99-global-concepts (55-line stub) | N00 (map) + N06 (endpoint) + N09 (RTS) | Stub dissolved into owners | Rows → N00/N06/N09 |
| OP-33 | new | — (G-01/G-10) | N18-C/D | Shutdown + build/test had no owner | K-098/K-102 → N18 |
| OP-34 | renumber | ALL old numbers 00–33/99 | N00–N18 | Dual 07s / dual 18s made old numbers un-citable | Migration tables §8 |

Deletes with reasons: old 09 as standalone (forward-ref defect, content preserved
in N13); old 23 as standalone (single-chain split across three use points,
content preserved); old 99 as stub (content preserved per-row); memos as chapters
(decisions preserved as boxes, prose not moved). No knowledge item deleted.

---

## 7. Missing-New-Chapters落实 (gap06: every §3 gap lands; no "TBD" allowed)

- G-01 shutdown → N18-C full trace (prepare_shutdown timer → minix_shutdown →
  arch_shutdown) with `main.c` anchors; acceptance: reader states halt vs poweroff vs reboot paths.
- G-02 bodies → N12 (TRACE/UPDATE/MCONTEXT per-`do_*.c` semantics + tests) and
  N18-C (ABORT path); acceptance: each names caller privilege + reply behavior.
- G-03 earm → bounded ARCH-note boxes in N04 (bsp), N15 (padconf), N18-C
  (deferred list); acceptance: each box names the C file and the bound ("no new chapter").
- G-04 locks+percpu → N16 (spinlock.h rule, cpulocals fields, bill_ptr beats in N09);
  acceptance: no-sleep example + per-CPU table.
- G-05 globals → N01 (kinfo), N17 (kmess), N18 (krandom note); acceptance: each
  consumer named with anchor.
- G-06 asm misc → N10 (trampoline/klib/io stubs table), N17 (breakpoints/debugreg),
  N18 (direct_tty/oxpcie one-line notes); acceptance: every `.S` has an owner row.
- G-07 grants → N13 unified lifecycle section; acceptance: SETGRANT→verify→revoke trace.
- G-08 SENDA+STATUS → N11 (batch) + N10 (status reg); acceptance: batch walk + status bits table.
- G-09 FPU restore → N17 both legs; acceptance: switch-in AND signal-path saves shown.
- G-10 build/test → N18-D appendix; acceptance: build→run→assert reproducible from §D alone.
- G-11 inhibits → N06 split table; acceptance: three flags distinguishable by test.
- G-12 EFAULT → N13 taxonomy; acceptance: SRC/DST cases + handler behavior each.

---

## 8. Anchor Migration & Breakage Cost

### 8.1 Chapter-level migration (old → new; B-phase expands to section tables)

| Old | New | Type |
|-----|-----|------|
| 00 | N00 | rewrite-slim |
| 01 | N01 | rewrite |
| 02 | N02 | rewrite |
| 03 | N03 | rewrite |
| 04 | N04 | slim-reorder |
| 05 | N05 + N15-part | split-by-phase |
| 06 | N06-part | merge-absorb |
| 07-cross-space-init | N07 | rewrite |
| 07-paging memo | N07 §box | distill-archive |
| 08 | N08 | rewrite |
| 09 | N13 + N08-preview | archive-merge |
| 10 | N08-part + N09 | split |
| 11 | N09 + N12-part | split |
| 12 | N11 | rewrite |
| 13 | N10-part | split-merge |
| 14 | N10-part | split-merge |
| 15 | N15-part | merge |
| 16 | N16 | keep-extend |
| 17 | N12-part | extend (G-02 in) |
| 18 | N13-part | merge |
| 19 | N14 | keep |
| 20 | N15-part | merge |
| 21 | N15-part | merge |
| 22 | N06 + N10 + N11 | 3-way split |
| 23 | N10-use + N11-use | dissolve |
| 24 | N13-part | merge |
| 25 | N12 + N17 + N18 rows | row-split |
| 26 | N18-A | verdict-archive |
| 27 | N17-part | merge-live |
| 28 | N18-B | verdict-archive |
| 29/30/32 | N17 | cluster-merge |
| 31 | N17-FPU-leg | keep-inside |
| 33 | N10 §caller | distill |
| 99 | N00 + N06 + N09 | dissolve |
| 06-todo/smp_todo/endpoint/panic-in-drop/smp_gpt* | cited only | process-archive |
| 18-trap-bridge memo | N10 §§ | distill-archive |
| todo.md/checklist.md | N18-C/D open-lists | cited, not moved |

Section-level rule for B-phase: every old `##` section gets one row
(old § → new § + move-type + risk). Chapter table above is the binding index;
B-phase may not invent destinations outside it without a blueprint amendment.

### 8.2 Reference migration (measured, not guessed)

- Doc→doc cross-refs: densest edges measured in §0 (`01↔02` ~34, `06→11` 21,
  `03→14` 20, `07→02-stage-vm` 24 cross-stage). All old-number citations
  (`NN-name.md` + `§x.y`) must be rewritten to N-numbers per §8.1. Hotspots:
  `01/02` pair, `06`, `08`, `13/14` (now unified in N10), `07` (cross-stage refs
  converted to evolution-cites, not normative).
- Code-comment refs (≈35 sites in `os/kernel/src/*`, `os/arch/src/*` — e.g.
  `lib.rs:494/506/532` "covered in 05/06/07", `cross_space.rs:16` "see 09-…",
  `irq_manager.rs:6`, `exception_dispatcher.rs`, `timer_irq_gate.rs`,
  `proc.rs:1041`/`kpriv.rs:446` "see panic-in-drop.md", `test_helpers.rs:4`):
  each maps per §8.1 (05→N05, 06→N06, 07→N07, 09→N13, 14→N10, 04→N04, 16→N16,
  panic-in-drop→N17). Batch method: `tools/anchor-migrate.sh`-style
  search-replace per mapping row, then `rg` zero-hit verification for old numbers.
- Cost summary: ~35 code sites + hundreds of doc cross-ref hits (exact count
  depends on section expansion; doc-hit total dominated by the 01/02/06/08/13/14
  cluster). No silent links: any unresolved old-number hit after migration fails
  the build gate (G8). Old files are archived, not deleted, so rollback is a move-back.

---

## 9. Verification, Self-Gates & Decisions

### 9.1 Four mechanical checks (§5.4)

1. Forward-ref scan: every contract's 前置 points only to earlier N-numbers —
   PASS by construction (N00→…→N18 chain; deviations D-1..D-4 logged with compensation).
2. Dependency graph: N00→N08 linear; N09→N11 linear; N12–N15 fan out of N10;
   N16/N17/N18 terminal — acyclic. PASS (no chapter cites a later chapter's conclusions).
3. Coverage: all 102 pool items have an owner (§2 rows + §7 gaps); all 12 gaps own
   sections; deletes listed with reasons (§6). PASS (subject to B-phase section-table audit).
4. Breakage count: code sites ≈35 enumerated with batch method; doc edges hotspots
   listed. PASS as estimate; exact totals locked after B-phase section expansion.

### 9.2 Self-gates G1–G9

| Gate | Result |
|------|--------|
| G1 C-truth checkable (10 random rows verified: B2 pre_init, K2 cstart, K4 loop, K5 system_init maps, K7 bsp_finish, R1 switch_to_user, R3 do_ipc, R4 dispatch, R6 do_vmctl, R8 shutdown) | PASS |
| G2 every C file / non-C artifact owned or excluded-with-reason (§3.1 universe + §3.5 ten answers) | PASS (earm bounded per G-03) |
| G3 zero forward refs in new catalog | PASS (D-1..D-4 compensated) |
| G4 acyclic + split plans | PASS |
| G5 100%: pool→owner; new items→anchors; deletes→reasons | PASS |
| G6 10 sampled splits/merges show stock-destinations + new-source anchors (OP-06/07/11/12/13/15/16/20/23/25) | PASS |
| G7 all 19 contracts carry 7 elements | PASS |
| G8 chapter migration covers all changing docs; ref migration covers docs + code comments | PASS (section tables deferred to B-phase per rule) |
| G9 anchors on factual claims; 3 inference marks (K-056 billing detail, G-04 bill_ptr scope, exact doc-hit totals pending expansion) marked to-be-verified | PASS with marks |

### 9.3 Conclusion & questions for the user

Conclusion: the blueprint is COMPLETE and executable. B-phase may start at N00
and proceed in catalog order; each chapter needs no further triage beyond its contract.

Questions requiring user ruling (no guessing):

1. N15 scope: time+IRQ+device merged (shared priv checks, one chapter) vs split
   into two (time vs device)? Blueprint recommends MERGE (fewer forward refs, one
   worked IRQ walk serves both). Overrule cost: +1 chapter, new D-entry.
2. N17 scope: FPU kept inside observability (shared per-CPU owner + trap context)
   vs standalone FPU chapter? Blueprint recommends INSIDE (avoids a 1-mechanism
   chapter and a new forward edge). Overrule cost: N17 shrinks, catalog becomes 20.
3. `06-todo.md`-style contract format is followed; old-number archive dir naming
   (e.g. `archive/00-33/`) is left to B-phase — confirm or prescribe.
4. `todo.md` open items (I-1/I-5/I-6/T-10, SMP-blocked Edge 12) are LISTED in
   N16/N18 as owned-open, not resolved here — confirm this boundary (blueprint
   does not fix code).
