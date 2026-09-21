# new_todo_muse.md — Remaining work to boot minix-rs in QEMU on three architectures and run all 18-stage-commands programs (muse independent scan)

> **Author tag**: muse. This file is one of two independently produced scans. It does not reference, merge with, or depend on any other `new_todo_*.md`. It is self-contained and isolated by the `muse` suffix.
> **Date**: 2026-09-20. Working tree HEAD is around `f5fcef73f` (S42 batch five era).
> **Goal (user-given)**: in QEMU on three architectures (x86_64, aarch64, riscv64), boot minix-rs and run the programs of 18-stage-commands. First priority is existence ("first ensure there is something"), second priority is quality (non-rewrite translate drift and code defects, "then ensure it is good").
> **Method**: full read of `edge_todo.md` (1096 lines), `notes/TODO.md`, the 18-stage-commands `plan.md` / `todo.md` open batches, plus four parallel code-scan passes (boot/kernel/arch, servers/drivers/FS/IPC, commands/libc, edge-ledger status) followed by first-person re-verification of every load-bearing anchor with `grep` / `sed` before writing. Anchors below are snapshots and will drift; re-read target lines plus/minus 5 lines under fix-guard before acting on any item.
> **Anchor legend**: every fact carries a `file:line` anchor. All anchors in section 2-5 were re-verified by muse on 2026-09-20 unless marked `(subagent-reported)`, meaning a scan agent reported it and muse has not yet re-read the exact lines.
> **Priority convention**: P0 = hard blocker on the boot-to-commands chain (nothing downstream is reachable without it). P1 = needed for the stated goal but a shorter path can boot first without it. P2 = second priority (fidelity defects, translate drift, hygiene).
> **Rewrite boundary**: the project goal is rewrite, not translate. C source under `minix3/` is ground truth for externally visible behavior; internal structure must use Rust's type system. Fixes below follow todo-fix discipline (explain what and why, compare at least two designs against Linux/Redox/OS theory, then implement) and never change observable behavior silently.

---

## 1. Executive summary: where the chain breaks

The boot-to-commands chain breaks at five cuts. Each cut is independently fatal:

| Cut | Content | Current state | Break point |
|---|---|---|---|
| 1. Firmware to kernel | boot-shim loads modules, kernel builds the process table, loads VM | x86_64 single-image carriers pass (test-user-trap, test-rt-birth, test-sysboot single image) | **Production boot-shim declares 6 modules in non-C order while the kernel demands exactly 12 in C order and maps purely by position** (M-P0-1). A production image panics at the length assert, and even past the assert every endpoint would be wrong. |
| 2. VM to RS to services | RS sends RS_INIT to every boot-table SYS_PROC, step 3 blocks until all answer | RS send leg and step-3 wait exist (`servers/rs/src/boot.rs:1031-1138`) | **Only sched, devman, fs-rt, and VM answer anything; PM, VFS, DS, MIB, IS never answer, and the shared `minix-sef` layer never even constructs its own `SefEvent::Init`** (M-P0-3, M-P0-4). Step 3 can never drain. VM additionally consumes RS_INIT without replying and fails the handshake because the rproctab grant is always -1 (M-P0-5). |
| 3. Service runnable form | every service needs a freestanding ELF the guest loader can run | Only test carriers (rt-birth, sysboot) produce guest ELFs | **PM and VFS libraries are std-only (no `no_std` attribute at all); no server `main.rs` has a freestanding target clause; 65 of 66 command bins are hosted-std** (M-P0-6, M-P0-12). There is nothing to put into a boot image except the init template. |
| 4. Root filesystem and console | root mounts, commands read/write stdin/stdout | VFS mount decision pieces exist but never execute; memory driver is a 7-line stub | **VFS `do_init_root` flips phase flags and mounts nothing** (M-P0-7), **MFS block source answers EIO to every block** (M-P0-8), **the memory driver process does not exist** (M-P0-9), **the console chain is broken at all four hops** (M-P0-10). |
| 5. Command face | command binaries build for the guest and execute | 65 bins exist with real hosted logic; C tree has about 247 programs | **No `sh` binary or executor, 14 key commands missing as bins, no `/etc` content, image and qemu assembly are skeletons** (M-P0-12 through M-P0-16). PM additionally rejects every real `execve` with EPERM because the caller gate sits on the wrong function (M-P0-11). |

One sentence: **hosted decision logic is far ahead of guest runnable reality**. Host tests exercise deciding halves; the four engineering faces that turn deciding halves into running guest processes (bootable binaries, boot image assembly, root filesystem chain, console byte path) are largely absent. Three-architecture coverage (T5) adds one more layer on top: aarch64 and riscv64 have no production user trap entry at all (M-P0-2).

Relationship to the existing ledger: items already registered in `edge_todo.md` (E1, E5, E-FSBDEV, E-FSRUNTIME, E-FSCMDS, E-INITSYS, E-CMDSYSFACE, E-SYSCALL-SIGN, E-BOOTFRAME, E-VMTLB and others) are referenced, not restated. Items marked **NEW** below were named as actionable boot blockers for the first time in this scan, or were previously buried inside a stage note and are now pinned to the boot chain with an anchor.

---

## 2. P0 hard blockers, in dependency order

### W1. Kernel runnable face (precondition of any user program)

#### M-P0-1 (NEW) Production boot-shim module list disagrees with the kernel boot-module contract in count, order, and mapping rule

- **What**: the UEFI-side loader declares 6 modules in its own order; the kernel requires exactly 12 modules in C `image[]` order and maps them strictly by index, never by name.
- **Evidence**:
  - `os/boot-shim/src/loader.rs:54`: `MODULE_NAMES = ["vm", "pm", "vfs", "rs", "ds", "inet"]` (6 entries).
  - `os/kernel/src/proc.rs:81`: `NR_BOOT_MODULES = 12`; `os/kernel/src/proc.rs:106-119`: `BOOT_MODULE_PROC_NRS` is the C `image[]` order DS, RS, PM, SCHED, VFS, MEM, TTY, MIB, VM, PFS, MFS, INIT.
  - `os/kernel/src/lib.rs:1131-1137`: `init_proc_and_boot` asserts the module slice length equals `NR_BOOT_MODULES`; inequality panics.
  - `os/kernel/src/lib.rs:1185-1188`: slot assignment is `BOOT_MODULE_PROC_NRS[i]` positional; `os/kernel/src/lib.rs:1194` copies the blob name into the slot without validating it belongs there.
  - `os/boot-shim/src/loader.rs:259-264`: missing modules are skipped silently, which then trips the exact-12 assert above.
  - Carriers hide this: `os/qemu-tests/test-kernels/kernel/bootstrap/test-sysboot/src/main.rs:88-101` hand-builds a 12-entry C-order table.
- **Why it blocks**: a production image dies in `kmain` at the assert. Past the assert, index 0 (`vm` ELF) would land in the DS slot (`ProcNr(6)`), index 1 (`pm`) in RS, and so on; `inet` has no C slot at all. Every endpoint would be wrong.
- **Design comparison**:
  - A (recommended): loader lists the 12 C-order module names, loads by name, errors loudly on a missing module, and drops the "kernel reorders by flag" comment. Change is confined to boot-shim; matches C `kinfo.module_list` semantics (`minix3/minix/kernel/main.c:171`).
  - B (rejected): kernel looks modules up by name. This rewrites the kernel contract; the positional `BOOT_MODULE_PROC_NRS` semantics are C behavior, and changing them is architectural evolution, not rewrite.
  - C (rejected): relax the 12-item assert and build a partial table. TTY, MEM, MFS, INIT absences then break RS step-2/step-3 counts; the failure moves downstream instead of being fixed.
- **Acceptance**: a QEMU smoke that boots a genuine boot-shim product (kernel.elf plus 12 modules) and reaches the VM/RS handshake; the sysboot carrier stops hand-forging its module table and derives it from the loader list.

#### M-P0-2 (NEW in this pinned form) No production user trap entry on aarch64 or riscv64; x86_64 exception leg still panics on every user-origin outcome

- **What**: both non-x86 ports install diagnostic halt stubs. The arch facade functions are empty or zero off x86_64. On x86_64 the IPC and SYSCALL legs exist but the exception leg panics on any user-origin outcome, and the fast-syscall return path panics unconditionally.
- **Evidence**:
  - riscv64: `os/arch/src/riscv64/trap_entry.rs:66-78` (`trap_vector` reads scause/stval/sepc, calls diag, `wfi` loop); the file's own comment assigns full register save, `sscratch` stack swap, and scause dispatch to the future bring-up lane.
  - arm64: `os/arch/src/arm64/trap_entry.rs:88-100` (all four lower-EL slots are `exc_bad_mode`); `:103-138` diagnose and `wfe` loop.
  - Facades: `os/arch/src/lib.rs:262-263` (non-x86 `install_trap_stubs` is empty), `:324-327` (non-x86 `syscall_entry_va()` returns 0), `:346-351` (non-x86 `register_trap_dispatchers` is a no-op).
  - Kernel side: `os/kernel/src/lib.rs:72-73` compiles `trap_dispatch` only on x86_64; `:895-898` documents that other architectures register nothing yet.
  - x86_64 exception leg: `os/kernel/src/trap_dispatch.rs:196-251` — the `KernelPanic` arm panics by design; the catch-all `other` arm panics with "needs per-CPU process context (S-6/S-7)". Classification exists but is unconsumed (`os/arch/src/arch/exception_dispatcher.rs:210` `ForwardToVm`, `:243` `Signal`).
  - Return path: `os/kernel/src/lib.rs:3018-3023` — `FastSyscall` panics, `None` panics; only `FullContext` returns.
- **Why it blocks**: on aarch64/riscv64 any trap (SVC/ecall, timer, IPI, fault) halts the hart. The rt-birth carriers on those architectures pass only because each carrier ships its own vector. On x86_64 any user page fault, exception, signal, or lazy-FPU trap kills the kernel, and any SYSCALL-entered process cannot return. Real programs that fault or use signals cannot survive; rt-birth passes only because `load_vm_elf` pre-materializes every page.
- **Design comparison**:
  - A (recommended): one leg per architecture first (aarch64 `svc` plus `VBAR_EL1` lower-EL slots saving x0-x30/ELR/SPSR/SP_EL0 into `CpuContext`; riscv64 unified `trap_vector` plus sscratch swap plus scause dispatch), reusing the existing `CpuContext`/`TrapFrame` abstraction; then the fault/IRQ legs. Mirrors the C per-arch shape.
  - B (rejected): syscall leg only, no hardware interrupts. Servers need timer and device interrupts to run; half a leg does not boot.
  - C (rejected): keep the carrier pattern of per-process self-supplied handlers. That makes the test double the production model and contradicts C.
- **Acceptance**: per-architecture `test-user-trap` equivalents (trap round-trip, not just birth) on production vectors; then the full-system carrier in section 7.

#### M-P0-3 (NEW as a production-path statement) `minix-sef` never constructs `SefEvent::Init`, so RS_INIT falls into generic call handling everywhere

- **What**: the shared SEF library defines the birth event but never produces it. `sef_receive_status` returns only `Signal` or `Call(m_type)`.
- **Evidence**:
  - `os/libs/minix-sef/src/lib.rs:63-71` defines `SefEvent::Init(i32)`.
  - `os/libs/minix-sef/src/lib.rs:130-167` (re-read 2026-09-20): the loop returns `Signal` at `:151`, answers ping inline, and returns `Call(m_type)` at `:167`. No `Init` construction.
  - Consequence: every `SefEvent::Init` arm in every server is dead code today (for example `os/servers/vfs/src/main_loop.rs:7153`).
- **Why it blocks**: C puts birth interception in libsys (`minix3/minix/lib/libsys/sef.c:118-137`, `sef_init.c:193`): block on RS before the main loop, answer `RS_INIT` plus result, reload tables. The Rust tree has no equivalent library behavior, so each server must hand-roll interception, and most have not.
- **Design comparison**:
  - A (recommended): add the birth face to `minix-sef` — recognize RS_INIT from the RS endpoint, yield `SefEvent::Init`, and provide one shared reply helper (source gate plus fresh versus live-update versus restart split). One fix serves every consumer, isomorphic with C's libsys layering.
  - B (rejected): per-service interception copied seven times. This is the status quo extended; new services will keep missing it (input and ipc-server already have).
- **Acceptance**: unit test per server: RS_INIT from RS yields the Init event and never reaches the business dispatch table; host regression across the affected crates stays green.

#### M-P0-4 (NEW in consolidated form) Birth-face answer coverage: PM, VFS, DS, MIB, and IS never answer RS_INIT

- **What**: C's mechanism is RS step 2 sending RS_INIT to every SYS_PROC boot-table entry and step 3 blocking until all answer (`minix3/minix/servers/rs/main.c:375-407`). The Rust RS side faithfully implements steps 2 and 3 (`os/servers/rs/src/boot.rs:1031-1138`), but the answer side exists in only four places.
- **Evidence**:
  - Answer today (subagent-reported, spot-verified for sched): sched `os/servers/sched/src/kernel_api/transport.rs:173-188` (type plus source gate, replies `RS_INIT` plus OK); devman `os/servers/devman/src/ipc/minix.rs:278-289`; fs-rt `os/fs/fs-rt/src/transport.rs:139-162`; VM `os/servers/vm/src/vm_server.rs:1314-1336` (consumes but returns `Suspend`, never sends `RS_INIT` plus result back — see M-P0-5).
  - Never answer (verified zero `RS_INIT` matches outside comments, 2026-09-20): `os/servers/pm/src/ipc/dispatcher.rs` (0 hits), `os/servers/vfs/src/main_loop.rs` (0 hits), `os/servers/ds/src/server.rs` (0 hits), `os/servers/mib/src/dispatch.rs` (0 hits). PM's own `os/servers/pm/src/main.rs:11` documents "no RS_INIT handshake".
- **Why it blocks**: RS step 3 waits on `nr_uncaught_init_srvs` (`boot.rs:1128-1136`); unanswered or wrongly-typed answers hang or panic the boot. C ground truth is that every server runs `sef_startup` first (for example `minix3/minix/servers/pm/main.c:115-125`, `vfs/main.c:387`).
- **Acceptance**: per-service seam test "RS_INIT from RS yields RS_INIT plus OK and business dispatch is untouched"; RS step 3 drains against all 12 boot-table members in a hosted simulation.

#### M-P0-5 (NEW in consolidated form) rproctab grant has no production creation point, so VM cannot complete the birth handshake

- **What**: the RS_INIT message carries `rproctab_gid`, a one-shot read grant C creates at boot (`minix3/minix/servers/rs/main.c:185`). The Rust field stays `None` on the production path and encodes as -1.
- **Evidence**:
  - `os/servers/rs/src/boot.rs:951-954` (field stays `None`, deferred to grant wiring); `:857-865` (documents the C creation point); `:1044` (copies the `None` into the outgoing message).
  - VM side: `os/servers/vm/src/vm_server.rs:1320-1340` (re-read 2026-09-20): handshake failure drops the message, counts it, audits, and returns `NoReply` — RS waits exactly as it would against a dead VM.
- **Why it blocks**: even with every other service fixed, VM never learns the boot ACLs and RS step 3 never sees VM ready.
- **Fix direction**: create the grant at boot step 0 (`cpf_grant_direct`-equivalent through a new `KernelApi` grant verb), land it in `rinit.rproctab_gid`, and check the kernel `dispatch_setgrant` gate for this use. Pin both directions with seam tests (RS asserts `is_some` after step 0; VM asserts a clean failure reply on an illegal gid).

#### M-P0-6 (NEW, systemic) Server binaries have no freestanding build face; PM and VFS libraries are std-only

- **What**: a bootable system needs one bare-metal ELF per guest process. Only test carriers produce guest ELFs today. Of the eleven servers, nine libraries build for the bare-metal target, but PM and VFS do not even declare `no_std`, and no server `main.rs` carries a freestanding target clause.
- **Evidence**:
  - Missing attribute (verified 2026-09-20): `os/servers/pm/src/lib.rs` and `os/servers/vfs/src/lib.rs` contain no `cfg_attr(not(test), no_std)` (the other nine server libs do).
  - `os/libs/minix-sockdriver/src/lib.rs` likewise has no `no_std` declaration (P1, network only).
  - All `os/servers/*/src/main.rs` lack a `target_os = "none"` clause; the only template is init (`os/commands/sbin/init/Cargo.toml:33-37`, `src/main.rs:19`) plus the rt-birth carrier precedent.
  - Workspace flags are ready (`os/.cargo/config.toml:33-34`, static relocation plus large code model for the kernel ELF loader).
- **Why it blocks**: `cargo check -p minix-pm --lib --target x86_64-unknown-none` cannot pass while the library links std. The same holds for VFS. Without freestanding servers there is no boot image content.
- **Fix direction**: copy the init template per crate (target-gated dependency split, `cfg_attr` triple of `no_std`/`no_main`/panic handler, link script in the rt-birth shape), crate by crate: PM first (the only library that fails), then VFS (via the sockdriver fix), then the eleven mains. Keep hosted tests green at every step and never flip workspace-wide defaults (the Cargo feature-unification trap bites when `-p minix-arch` shares one invocation with boot-shim or test kernels; build them in separate invocations).
- **Acceptance**: `cargo check -p minix-<each-server> --target x86_64-unknown-none` clean for all eleven, and a linked ELF per server.

### W2. Boot image and install face

#### M-P0-7 (NEW in executable form) `xtask image` and `xtask qemu` are skeletons; no proto files, no install manifest, no freestanding command products

- **What**: there is no mechanized path from built ELFs to a bootable image to a QEMU invocation, even for init alone.
- **Evidence**:
  - `os/xtask/src/main.rs:123-140` (re-read 2026-09-20): both entries print `[skeleton]` and return Ok; the comments list collecting ELFs, packing, rootfs via mkfs plus fstab, and output under `os/target/image/`.
  - `find -name "*.proto"` is empty; the only fstab hit is the *library* (`mountinfo/src/fstab.rs`), not a mapping of the 66 binaries onto `bin`/`sbin`/`usr.bin` layers.
  - Command Cargo files pin `minix-rt` with `features = ["std"]` and no `target_os = "none"` split (for example fileops, textfilter).
- **Why it blocks**: "run on booted minix-rs in QEMU" has no assembly route. The image must contain kernel.elf, the 12 module ELFs (M-P0-1, M-P0-6), the root image, and the command set (M-P0-12+).
- **Acceptance**: `cargo run -p xtask -- image` yields a complete image directory, and the hosted fsck engine reads it back with every manifest entry and device node present.

#### M-P0-8 Production boot loads exactly one user ELF; every other boot module is DEFERRED and all images share one stack window

- **What**: only the VM branch really loads; RS plus ten others are deferred; `load_vm_elf` maps every image's stack at the same global `user_sp`-derived window.
- **Evidence**:
  - `os/kernel/src/lib.rs:1314` region (previously-DEFERRED branch) and `:1454` (`EntrySpec::DEFERRED`, "RS will load them at runtime").
  - `os/arch/src/arch/boot.rs:471-494` (re-read 2026-09-20): stack mapped at `[stack_high - 64K, stack_high)` from the global `kernel_info.user_sp()`; ps_strings at `high - 32`, SP at `high - 52` for every image.
  - The sysboot carrier works around this by linking RX/TX at disjoint bases and reusing the same stack geometry.
- **Why it blocks**: production boot yields exactly one runnable user ELF. A second image collides on stack virtual addresses and ps_strings (and on segment addresses unless hand-linked disjointly) inside a single shared root. There is no per-process root creation or VM address-space switch at boot yet, and no RS exec server to consume the deferred slots (M-P0-13).
- **Fix direction**: needs a design decision first — per-process `stack_high` parameter (fast but leaves shared-root collisions) versus per-process roots at the boot loader (C's `arch_boot_proc` shape, touches kernel and arch). Decide, then prove with the dual-image sysboot carrier before touching the production loader.
- **Acceptance**: `test-sysboot.sh` dual-image PASS, then inclusion in `run_all.sh` (it is absent today: `grep -c sysboot run_all.sh` returns 0, verified).

#### M-P0-9 VFS never mounts root: `do_init_root` sets flags and returns

- **What**: the mount decision pieces are ready but have no production caller.
- **Evidence**: `os/servers/vfs/src/main_loop.rs:517-539` (re-read 2026-09-20): sets `BootPhase::Mounting`, disables request acceptance, notes that `mount_pfs`/`mount_fs` IPC is deferred, re-enables acceptance, sets `Running`. Zero `vmnt_table` or vnode mutation, zero `FsClient` send. The reader exists unused: `mount.rs:199-255` (`FsSuperblock` production reader via `send_with_retry`). The device map holds only CTTY: `device_map.rs:135-140`; full `map_service` is deferred at `main_loop.rs:493-496`.
- **Why it blocks**: with no `/` vnode and no superblock (`req_readsuper` never reaches MFS), every pathname syscall fails: `sh` startup, `exec /bin/echo`, reading `/etc/rc`.
- **Fix direction**: wire the startup segment (`sys_safecopyfrom(RS, rproctab)` plus `map_service`, whose RS half `mapdriver` is likewise unwired at `os/servers/rs/src/publish.rs:9-13` — the two halves must land together), then make `do_init_root` really send `req_readsuper` to MFS and seat the vmnt (C: `minix3/minix/servers/vfs/main.c:501-522`).
- **Acceptance**: seam test with a stub FS answering readsuper (vmnt plus vnode seated), then true-machine root mount.

#### M-P0-10 MFS block source answers EIO to every block

- **What**: the MFS process runs the fs-rt serve loop, but its block source is the always-EIO placeholder.
- **Evidence**: `os/fs/mfs/src/main.rs:11-12,32` (47-line file); `os/fs/fs-rt/src/source.rs:19-31` (every read and write returns EIO, verified). A real `BdevBlockSource` exists at `os/libs/minix-fs/src/bdev_bridge.rs:44-165` with only test consumers.
- **Why it blocks**: superblock, `/etc/rc`, and `/bin/sh` reads all fail. Even a mounted VFS cannot serve bytes.
- **Fix direction**: connect `BdevBlockSource` to the memory-driver endpoint (M-P0-11) for the first milestone; the true block-driver half stays under the existing E-FSBDEV entry and is not on this critical path. Land M-P0-9, M-P0-10, and M-P0-11 as one batch against the same mount graph.
- **Acceptance**: hosted MFS tests plus true-machine `REQ_GETDENTS` round-trip on the root directory.

#### M-P0-11 memory driver process does not exist; root device has no server

- **What**: C's root device is the memory driver (major 1, `DEV_IMGRD`), with the image linked into its binary. The Rust policy library is complete but the process shell is a placeholder and the image extents are empty.
- **Evidence**: `os/drivers/storage/memory/src/main.rs` is 7 lines (`init(); loop {}` plus TODO, verified); `device.rs:127-141` leaves `ImageDisk` extents zeroed with a comment saying the service crate fills them at startup — no such startup logic exists. Line-count survey (verified): 55 driver mains total, 53 under 300 bytes; only tty and pckbd run real serve loops.
- **Why it blocks**: no `DEV_IMGRD` backing for the root mount; MFS has no block device to talk to; no RS birth, no announce, no grant copies.
- **Fix direction**: give `main.rs` the tty-shaped process (`DriverRuntime::new` plus `serve`), fill `ImageDisk` from the linked-in image interval at startup, then announce and answer RS_INIT in the same batch as M-P0-4. Minimum boot set needs only tty and memory as processes; the other 53 stubs are P1 (section 4).
- **Acceptance**: hosted driver tests plus true-machine RS_INIT answer and VFS major-1 mapping.

#### M-P0-12 Console output chain is broken at all four hops

- **What**: `write(1, ...)` must traverse VFS character recognition, device-map routing, tty process handling, and hardware backend. All four hops are down.
- **Evidence**:
  - VFS only serves regular files: `os/servers/vfs/src/syscalls.rs:252` ("ENOSYS until wired"); the read arm serves regular files only with `S_ISFIFO`/`S_ISCHR`/`S_ISBLK` explicitly unwired; both the read-type and write-type dispatches return `Nosys` for non-regular modes (verified at `:300-310` and `:410-422`).
  - Device map has no tty slot (`device_map.rs:135-140` maps only CTTY major 5); tty major 4 is recorded in the RS boot device table but `mapdriver` is unwired.
  - TTY process shell exists (`os/drivers/tty/tty/src/main.rs:1-30`, verified) but its self endpoint is `Endpoint::NONE` pending RS assignment; the service wires `NullBackend` (`os/drivers/tty/tty/src/lib.rs:46-47`, verified).
  - Backend is a null object (`backend.rs:70-72` no-op; header `:10-30` documents serial unreachable and video needing a live memory server).
- **Why it blocks**: bytes never leave VFS, and even routed bytes would be accepted as zero. Shell output is invisible and input likewise.
- **Design comparison**:
  - A (recommended): serial first. The driver process writes COM1 by polled port I/O (x86_64 via `SYS_DEVIO`/`SYS_VOUTB`, per-architecture MMIO equivalents elsewhere); QEMU `-serial stdio` shows output immediately on all three architectures. Matches C's `ser_putc` shape with the shortest chain (no VM video-memory mapping, no live memory server).
  - B (later): video-memory text buffer via a VM physical mapping, matching C's default VGA channel but coupling two unwired chains together.
  - C (rejected): diagnostics channel (`SYS_DIAGCTL`) as stdout. That seam is the server diagnostic drain, not the terminal data path; user data would pollute the diagnostic stream.
- **Acceptance**: true-machine `write(1, "hello\n")` through VFS to tty to serial appears in QEMU output, with `tcgetattr`/`tcsetattr` still passing.

### W3. exec chain (the file-execution path)

#### M-P0-13 PM `do_exec` carries the caller gate that C puts on `do_newexec` (correctness bug, boot-fatal)

- **What**: the VFS-or-RS-only gate sits on the forwarding function instead of the acquire function.
- **Evidence** (both sides re-read 2026-09-20):
  - Rust `os/servers/pm/src/exec.rs:99-112`: `do_exec` returns `Perm` unless the caller is VFS or RS; the comment at `:106-108` attributes `exec.c:70-71` to `do_exec`.
  - C `minix3/minix/servers/pm/exec.c:38-56`: `do_exec` has no gate (unconditional `tell_vfs` forward plus SUSPEND). The gate lives in `do_newexec` at `:70-71` (`who_e != VFS && != RS` yields EPERM). The Rust `do_newexec` at `exec.rs:169-177` already has its own gate, so the gate is duplicated in the wrong place and missing nowhere.
  - Dispatch `os/servers/pm/src/ipc/calls.rs:611-625` passes the user caller slot through, so a real `execve` from INIT or a shell always presents a non-VFS, non-RS endpoint.
- **Why it blocks**: every real `do_exec` returns EPERM. Init and shell can never exec; the `/etc/rc` chain dies at its first step. Existing tests cement the wrong behavior (`exec.rs:332-350`) and must be updated with the fix.
- **Fix**: remove the gate from `do_exec` (unconditional forward plus SUSPEND), keep the gate on the ExecNew and ExecRestart arms (the `do_newexec`/`do_execrestart` counterparts). This is a correctness fix under todo-fix, not a drive-by edit.
- **Acceptance**: seam tests for both placements plus true-machine `execve("/bin/echo")`.

#### M-P0-14 VFS exec worker is a stub returning zeroed fields

- **What**: the endpoint check is real; everything after it is not.
- **Evidence**: `os/servers/vfs/src/ipc/dispatcher.rs:200-215` (re-read 2026-09-20): validates the endpoint, then returns `Exec { status: 0, pc: 0, newsp: 0, newps_str: 0 }` with a comment deferring the worker. The wire exists (`os/libs/minix-types/src/ipc/vfs.rs:351` `Exec`, `:521` reply). The VM-to-kernel half (ELF walk plus `sys_exec`, boot stack ABI) is real, so genuine entry and stack values from VFS would complete the chain.
- **Why it blocks**: no file execution: open, read header (including the `#!` script branch, which may lag), load ELF segments (via the VM exec face or datacopy), and emit the PM_EXEC_NEW payload are all missing (C: `minix3/minix/servers/vfs/exec.c`).
- **Acceptance**: hosted worker test plus true-machine `execve` of a filesystem binary.

#### M-P0-15 RS `read_exec` is a no-op seam on every production path

- **What**: service bring-up and crash restart fork a process with no image.
- **Evidence**: `os/servers/rs/src/exec.rs:8-30` (validator without a production caller); no-op injections at `os/servers/rs/src/lib.rs:427-436`, `:756-764`, `shell_request.rs:919-928`, `:953-961`, `recovery.rs:924`.
- **Why it blocks**: boot-table services arrive via kernel loading plus VM exec and do not need this path, but every `service up` start (IS, MIB, devman, input, ipc-server are not boot-table members) and every crash restart does. The system cannot sustain itself past first boot.
- **Fix direction**: C implements it as `srv_fork` plus `srv_execve` (`manager.c:1372-1420`); the Rust side needs the VFS `read_exec` request face ready first, so this lands after M-P0-9.
- **Acceptance**: hosted seam test bringing up a real binary to RS_INIT, then true-machine restart coverage.

### W4. init and commands runnable form

#### M-P0-16 Commands: 65 hosted-std bins, no `sh`, 14 key commands missing, init birth-signature mismatch

- **What**: the command layer has real hosted logic but no guest runnable form and no shell.
- **Evidence** (all verified 2026-09-20):
  - Count: `find os/commands -path "*src/bin/*.rs" | wc -l` returns 65; C has about 247 programs (per-tree `ls | wc -l` sums).
  - Missing as bins: `sh, cat, ls, cp, mv, rm, mkdir, pwd, test, chmod, ps, mount, date, sleep` (each confirmed MISSING by filename search). Present: `echo, stty, kill, ed`. File-operation deciding halves exist in libraries without thin shells.
  - Shell: `os/commands/bin/shell` is library-only (no `[[bin]]`; modules are `lexer, expand, redir, script`); executor comments defer fork, exec, pipe, and waiting.
  - Freestanding: only init gates `no_std`/`no_main` on `target_os = "none"` (`os/commands/sbin/init/src/main.rs:19`, `Cargo.toml:33-37`); every other bin uses `std::env::args` plus `std::process::exit` (some plus `std::fs`).
  - Birth-signature mismatch: `os/libs/minix-rt/src/crt0.rs:293-294` declares `unsafe extern "Rust" { fn main() -> i32; }` and calls it for the exit code; `os/commands/sbin/init/src/main.rs:64` defines private `fn main()` returning `()`. Even the one freestanding binary does not satisfy the birth chain symbol today.
  - Runtime seam exists and only init uses it: `crt0.rs:141` `argv_count`, `:163` `argv_bytes`, `:191` `args`, `:199` `envs`, plus `minix_sys::exit`; consumer `init/src/main.rs:97-100`.
- **Why it blocks**: init's runcom execs `/bin/sh /etc/rc` (`runcom.rs:24-26`) — a binary that does not exist, running a script that does not exist (M-P0-17). On the bare-metal target the 65 bins cannot link; on hosted targets they read Linux argv and exit via Linux, never through the birth chain.
- **Fix direction**: repair init's `main` signature and link script first (it is PID 1 and the template for everything else), then convert bins domain by domain to the echo-style dual seam (std hosted today, minix-rt birth accessors plus `minix_sys::exit` for the guest), starting with the smoke set in section 7. `mkfs_mfs`/`fsck_mfs` host halves stay hosted image tools; their target halves are P1.
- **Acceptance**: `cargo build -p minix-fileops --target x86_64-unknown-none --bin echo` and `cargo build -p minix-init --target x86_64-unknown-none` each yield an ELF.

#### M-P0-17 `/etc` on-disk content is entirely absent

- **What**: the directory holds a placeholder README and nothing else.
- **Evidence**: `ls os/etc` returns `README.md` only (verified); the README lists `rc`, `fstab`, `passwd`, `TTYS` as future work. The init consumer side is complete (`runcom.rs:26` `RUNCOM_SCRIPT = "/etc/rc"`, fork/exec/waitpid chain `:129-170`).
- **Why it blocks**: runcom, password, terminal-line, and mount-table inputs do not exist. Boot reaches runcom and falls through to single-user or reboot paths by construction.
- **Fix direction**: minimal content first (smallest viable `rc` service-start subset against `minix3/etc/rc`, `/dev/console` node, TTY table, passwd), installed via the mkfs prototype-fill path or disk-image install; full C `rc` equivalence is explicitly out of scope for the first milestone.
- **Acceptance**: true-machine init enters the multi-user shape (section 7, T3 line).

#### M-P0-18 `minix-sys` top-level lacks the file and shell essentials

- **What**: the transport-level `*_via` functions exist, but the plain top-level verbs commands program against are missing for the whole file and shell family.
- **Evidence** (verified 2026-09-20):
  - Present top-level (`os/libs/minix-sys/src/lib.rs`): `send, receive, sendrec, notify, fork, exec, exit, sigreturn, waitpid, kill, open, tcgetattr, tcsetattr, close, read, write, stat, lstat, fstat, ioctl, fcntl, getdents, mmap` (plus `dup2_via, chroot_via, lseek_via, open_via, open_existing_via` at the transport level).
  - Absent top-level (no `pub fn`): `pipe, pipe2, mkdir, rmdir, unlink, rename, link, symlink, chmod, chown, chdir, getcwd, access, umask, isatty, truncate, sync, mount, umount, mkfifo, uname, reboot, shutdown`, and the socket family (`socket.rs:80-94` holds only two pure-function helpers).
  - Already closed elsewhere: errno re-export (`lib.rs:55`), argv and env accessors (crt0, above), `perform_syscall`/`perform_taskcall` transport-failure short-circuit (E-SYSCALL-SIGN closure).
- **Why it blocks**: shell needs `chdir/getcwd/access/umask/isatty/pipe`; file tools need `mkdir/rmdir/unlink/rename/link/symlink/chmod/chown/truncate`; init needs `mount/reboot/sync`; network commands need the socket family. No command beyond open/read/write/stat can be issued.
- **Fix direction**: add per command-wave order (shell and smoke-set group first: `pipe2` with existing `dup2`, then `mkdir/unlink/rmdir/chmod/chdir/getcwd/access/isatty/fstat`), each with wire-shape plus hosted-EIO-contract tests; new `minix-types` wire layouts go through the established shared-contract discipline. Ownership sits with the 14-stage-runtime lane (E-CMDSYSFACE precedent); the 18-stage side must not freelance the wire.
- **Acceptance**: per-group wrapper tests; smoke-set commands compile against the new top level.

---

## 3. P1 needed soon (boot-adjacent, ordered by wave)

- **M-P1-1 TTY and MEM must be the first two driver processes; the remaining 53 stubs follow.** Minimum boot needs tty (M-P0-12) plus memory (M-P0-11). Log, random, readclock, fb, pty, and the rest are P1 each; pckbd already runs a real loop but carries a NONE endpoint so its DS and device-map wiring stays open. Non-boot-table drivers start via `service up`, which waits on M-P0-15.
- **M-P1-2 Services need a `whoami` wrapper and must stop hard-coding `Endpoint::NONE`.** C services ask their endpoint first (`minix3/minix/lib/libsys/sef.c:78-82`). The kernel side exists (`GET_WHOAMI = 19` at `os/kernel/src/misc.rs:94-95`, handler at `:640-645`), but `minix-sys` has no wrapper (verified zero hits) and tty (`main.rs:13-16`) plus input (`main.rs:36`, `serve.rs:79-82`) bake in NONE. Misaddressed replies and unanswerable RS pings follow.
- **M-P1-3 minix-rt heap is a fixed 64 KiB pool with no VM page supplier.** `GLOBAL_POOL_BYTES = 16 * PAGE_BYTES` (`os/libs/minix-rt/src/alloc.rs:62`, verified); growth is documented as later via VM mapping. Service process tables and most commands need real growth. The VM client channel exists (`break_via`, mmap family); the design choice (brk taskcall versus mmap channel, following the E-BOOTFRAME precedent) comes first. P1 rather than P0 because first programs fit.
- **M-P1-4 Panic spins instead of exiting through PM; no sigreturn trampoline exists.** `os/libs/minix-rt/src/lib.rs:275-310` (verified): format to a 256-byte stack buffer, run the diagnostic hook or spin sink, then spin forever; staged evolution names PM-exit termination as a later step. No `sigreturn` or trampoline string exists under `os/libs/minix-rt/src/` (verified). A panicked or finished program never releases, so the scheduler cannot reclaim it; any delivered signal has nowhere to return through. The five init handlers stay nominal until this lands.
- **M-P1-5 PM exec and misc batches plus the Reboot arm.** Main loop, fork, exit, wait, and signals are real; the exec wire batch, the misc batch, and `PmCall::Reboot` (catch-all ENOSYS today; init's reboot and powerdown paths need it, C counterpart in the misc-queries domain) remain. Minimum viable reboot may be a `sys_abort` equivalent, decided at implementation time.
- **M-P1-6 Init residual ENOSYS face.** Main loop, signal registration, session, and fork-exec-wait are real. Residual: live security-level half (`host.rs:288` ENOSYS), `init_root` (`host.rs:292` ENOSYS, unlocks with M-P0-9), trampoline address zero (M-P1-4).
- **M-P1-7 kerninfo replication into future per-process roots.** The bootstrap mapping is landed (`KERNINFO_USER_VA = 0x2_0000_0000`, fill-map-publish at `kerninfo.rs:164-167`, called at `lib.rs:502`); the file itself documents the deferred piece (`kerninfo.rs:28-33`): when VM takes over per-process address spaces, each new root needs its own mapping. P1 because bootstrap boot is complete without it.
- **M-P1-8 `test-sysboot` dual-image PASS then CI membership; per-architecture timer-IRQ carriers for riscv64 and aarch64.** The sysboot carrier plus RX/TX payloads plus verdict script are checked in and runnable but outside `run_all.sh`. Timer drivers (~150 lines each for CLINT and generic timer) are written but have no true-machine interrupt-delivery proof; x86 already has `test-timer-irq`. The `notes/TODO.md` QEMU backlog (L4 exception delivery, L5 interrupt delivery: unit yes, QEMU end-to-end no) closes through these carriers.
- **M-P1-9 Cross-build matrix.** After M-P0-6 and M-P0-16, run `cargo check --target {x86_64-unknown-none, aarch64-unknown-none, riscv64gc-unknown-none-elf}` per crate as a script and take it into CI. riscv64 PMP stays entry-0 allow-all until then (declared known limit, single-core boot unaffected).

---

## 4. P2 second priority: fidelity defects, translate drift, hygiene (ensure good, after ensure present)

> None of the items below blocks first boot. Each is independently actionable under full-review or todo-fix. No behavioral fix ships inside boot-chain work (fix-guard discipline).

| ID | Severity | Item | Evidence (re-verified 2026-09-20 unless noted) |
|---|---|---|---|
| M-P2-1 | P0-code-bug filed under P0 too | PM `do_exec` gate on the wrong function (see M-P0-13) | `os/servers/pm/src/exec.rs:99-112` vs `minix3/minix/servers/pm/exec.c:38-56` and `:70-71`. Listed in both tables so neither scan pass drops it. |
| M-P2-2 | P1 | Command-boundary guard regressed: `tools/check-command-boundary.sh` FAILs | Live run: `init/src/execve.rs:203,211` (direct `minix_types` dereference of kerninfo), `termctl/src/stty.rs:131-146,155,163` (direct `Termios` and flag constants). The stty hits sit in test code; whether the guard exempts tests needs a ruling before 30-plus call sites copy the pattern. |
| M-P2-3 | P1 | Workspace `--bins` build breaks on cross-target leakage | `cargo build --workspace --bins` dies in `test-shutdown-riscv64` (riscv64 symbols plus duplicate panic impl leaking into the host graph); per-crate `--bins` is unaffected. Same root cause family as the Cargo feature-unification trap: keep cross-target crates out of shared invocations. (subagent-reported) |
| M-P2-4 | P1 | `static mut` plus unsafe density needs per-site rulings, not bulk cleanup | 81 `static mut` outside tests (mostly kernel startup and interrupt statics, acceptable); 21 files above 3 unsafe per 100 lines, headed by `minix-usb/src/wire.rs` (10/134), `pm/src/ipc/decode.rs` (31/461), `x86_64/trap_stub.rs` (58/846). Kernel and arch density is inherent (page tables, trap frames); PM `decode.rs` at 6 percent deserves its own ruling now that decode is supposedly centralized. Register for a code-excellence pass; do not bulk-clean. (subagent-reported with counts) |
| M-P2-5 | P2 | TEMP-DEBUG probes on the DIAGCTL console path | `os/kernel/src/syscall.rs:2659,2662,2718,2733,2740` — five writes on the exact channel every rt-birth and sysboot payload uses for console output. Pollutes serial PASS markers and perturbs trap-heavy timing. Delete directly. |
| M-P2-6 | P2 | RS main loop return value discarded | `os/servers/rs/src/main.rs:67` (`let _ = server.run();`). The root server exits silently; C panics. |
| M-P2-7 | P2 | Stale boot-shim comment asserts a nonexistent kernel behavior | `os/boot-shim/src/loader.rs:44-53` ("kernel reorders by SYSTEM flag"). Same root as M-P0-1; fix with it. |
| M-P2-8 | P2 | Test carrier forges a 12-entry zero-length module table | `test-sysboot/src/main.rs:88-100`. Masks M-P0-1; derive from the loader list once M-P0-1 lands. |
| M-P2-9 | P2 | Unknown VMCTL parameter returns ENOSYS where C returns EINVAL | `os/kernel/src/syscall.rs:2219` area, self-described as intentional. Under the ground-truth chain (C above design above code) an intentional deviation needs a design-side reason with three-place `[ARCH:]` marking, else converge to C. (subagent-reported) |
| M-P2-10 | P2 | Fork parent FPU unload is a no-op; non-BSP TSS `sp0` is a placeholder | `os/kernel/src/proc.rs:1670` (per-CPU FPU ownership unwired; SMP parent-child FPU state may cross-pollute); `os/arch/src/x86_64/protection.rs:370` (AP interrupt path uses the wrong kernel stack until per-AP setup). Both are SMP-correctness items for the T5 wave. (subagent-reported) |
| M-P2-11 | P2 | Known semantic deviations on record (do not fix, remember) | VM warm restart panics (`os/servers/vm/src/main.rs:41-49`, verified; C keeps BSS state across restart, minix-rs keeps instance state); `KernPhysMap`/`KernMapReply` answer ENOSYS (32-bit-only, standing WONTFIX); `SYS_IOPENABLE`/port I/O answer BadCall off x86 (non-x86 drivers take MMIO, acceptably different-shaped); `sys_statectl` has no wrapper (live-update-only, first boot unaffected). |
| M-P2-12 | P2 | Doc drift: file-ops doc still blames `open` for cat | `06-file-ops.md:182` (subagent-reported) still says cat blocks on `open_existing` ENOSYS; `open_existing_via` is real since the 99-series path layout work. Cat's true blocks are guest-binary form and the exec delivery chain. |

**Checked and deliberately not filed** (so the parallel scan can cross-verify instead of duplicating): E-SYSCALL-SIGN transport-failure short-circuit is landed on both legs (`syscall.rs:100-112`, `:135-139`); E-CMDSYSFACE errno re-export (`minix-sys/src/lib.rs:36`, verified as `pub use minix_types::types::errno::*` at `:55`) and argv/env accessors (crt0, above) are landed; PM signal and uid-family match arms are all present (`calls.rs:390-603`); the 18-stage C-3 claim about missing accessors is therefore half-closed.

---

## 5. 18-stage-commands view: what "all programs runnable" still needs

- **Baseline (verified)**: 24 domain crates, 65 thin bins, about 45k lines plus about 1025 tests. Against roughly 247 C programs the binary coverage is about one quarter.
- **Build layer**: guest-target switch per M-P0-16 (echo dual-seam pattern is the template) plus M-P1-9 matrix.
- **Libc layer**: M-P0-18 group order (pipe2 first for sh, then the file family, then mount/reboot/sync for init, then sockets for net commands). Owned by the 14-stage-runtime lane; the 18-stage side consumes.
- **Server layer**: file commands wait on M-P0-9/M-P0-10/M-P0-14; terminal commands wait on M-P0-12; process commands (`ps`, `kill`) wait on the MIB process-table producer (existing E-MIBPROD entry).
- **Wiring tail**: sh executor unstarted (lexer plus parser plus 36 tests only; needs pipe2 plus M-P0-14); file-op bins unwired (`cat`, `cp`, `mv`, `rm`, `ln`, `ls`, `mkdir`); terminal games waiting on raw terminal mode. Smoke set first: **echo (wired), ls, cat, sh** — these four prove fork, exec, pipe, redirect, file read/write, and terminal. Everything after them is deciding-half wiring once the six primitives work.
- **Acceptance**: QEMU command-smoke script executing the smoke set with output comparison (shape follows `test-sysboot.sh` serial-marker verdicts, no gdbstub mailbox dependency), runnable only after the install face (M-P0-7) and the output channel (M-P0-12) exist.

---

## 6. Suggested execution waves (muse view, one item at a time)

| Wave | Content | Depends on | Exit criterion |
|---|---|---|---|
| W1 Contracts and hard blocks | M-P0-1 module list, M-P0-3 SEF birth face, M-P0-5 rproctab grant, M-P0-13 PM gate fix, M-P2-5 TEMP-DEBUG removal | None | Hosted regression green; RS step 3 drains against all 12 boot-table members in hosted simulation |
| W2 Freestanding binaries | M-P0-6 servers, M-P0-16 init plus echo first, M-P1-2 whoami | None (safe in parallel with W1, disjoint touch surface) | Per-crate bare-metal check clean; one ELF per server plus init and echo |
| W3 Boot image | M-P0-7 xtask image plus manifest, M-P0-11 memory process, M-P0-8 multi-image design decision plus sysboot dual PASS | W1, W2 | Complete image directory: kernel plus 12 modules plus root image; sysboot in `run_all.sh` |
| W4 Filesystem chain | M-P0-9 root mount plus map_service both halves, M-P0-10 true block source | W3 | True-machine root mount; MFS serves root directory reads |
| W5 Console | M-P0-12 serial backend first, then character read/write routing | W3, W4 | True-machine stdout on serial; termios round-trip |
| W6 Command smoke | M-P0-18 wrapper groups for the smoke set, M-P0-14 VFS exec worker, M-P0-17 /etc minimum, smoke-set bin conversions | W5 | True-machine smoke set executes with output comparison |
| W7 Init and rc | M-P0-15 read_exec, runcom true run, M-P1-6 residuals | W6 | True-machine init reaches multi-user shape |
| W8 Three architectures | M-P0-2 non-x86 trap entries, M-P1-8 carriers, M-P1-9 matrix, M-P2-10 SMP items | W6 on x86_64 first | T2 through T4 re-run per architecture plus SMP correctness |

Correspondence to the existing books: W1 covers the two unregistered blocks outside the edge4 status board (module list, birth-face answer coverage) plus the two misfiled correctness bugs; W3/W4/W5 concretize the S32 install face and the L17 driver wave; W8 is T5.

---

## 7. Acceptance and carrier notes

1. **Production boot smoke** (W1 exit): boot a genuine boot-shim product and assert VM reaches the RS handshake. No test covers the production load path today, which is why M-P0-1 survived this long.
2. **Birth-face coverage** (W1 exit): per boot-table member, "RS_INIT from RS yields RS_INIT plus result". Hosted seam tests suffice; no guest needed.
3. **True-machine multiprocess carrier** (W3/W4 exit): `test-sysboot.sh` is the ready-made shape; admit it to `run_all.sh` once M-P0-8 lands (deliberately excluded from CI today).
4. **Command smoke script** (W6 exit): sequential exec inside QEMU with per-command exit-code and stdout comparison, serial-marker verdicts.
5. **Three-architecture matrix** (W8 exit): build-matrix job plus per-architecture command smoke in `run_all.sh`.

---

## 8. Scan method and boundary statement (muse)

- **Inputs**: `edge_todo.md` in full, `edge4.md` ladder, `notes/TODO.md`, the 01-stage-kernel / 09-stage-init / 18-stage-commands stage todos and plans, `os/` full-tree directed grep (kernel, arch, plat, boot-shim, 11 servers, libs, fs, drivers, commands, qemu-tests, xtask), and `minix3/` cross-checks for the PM exec gate, RS birth sequence, VFS mount sequence, and console driver shape.
- **Subagent passes**: four read-only scans (boot/kernel/arch, servers/drivers/FS/IPC, commands/libc, edge-ledger status). Anchors echoed from those passes without a personal re-read are marked; everything in section 2 tables without that mark was re-read by muse.
- **Isolation**: this file modifies nothing else. It does not edit `edge_todo.md` or any stage todo (concurrent-write ban holds). Of the newly pinned facts, M-P0-1, M-P0-3, M-P0-4, M-P0-5, M-P0-6, M-P0-13, and M-P0-18 deserve ledger adoption under the edge4 section-1 rule 7 when executed; this file only registers them.
- **Coverage deliberately excluded** (stated so the parallel scan can judge overlap honestly): per-batch `doc_rerank` blueprint contents, the 05-stage-vfs 64-arm residual list arm by arm, the net family follow-up batches, and 16-stage NIC wave details. They enter this file only where they block the boot chain.
- **Code-excellence lens** (the `/goal` skill): each P0 carries a design comparison with at least two options and a Redox/Linux/C-theory reference where the choice matters (loader list, trap-leg shape, SEF-layered birth, serial-first console, per-process roots); the dead-code candidates are the `SefEvent::Init` arms (dead until M-P0-3), the forged sysboot module table (dead-test-double until M-P0-1), and the duplicated `do_exec` gate (dead-wrong until M-P0-13, with tests to update). Illegal-state blocking that must survive implementation review: RS step 3 must stay fail-closed on wrong-type answers; VM must stay no-reply on grant failure; VFS mount must stay accept-closed until the vmnt seats.
