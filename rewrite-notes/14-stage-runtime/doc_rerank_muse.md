# 14-stage-runtime Document Rebuild Blueprint (muse)

## 0. Metadata

- Executor: muse
- Date: 2026-09-19
- Target directory: `rewrite-notes/14-stage-runtime/`
- Repository root: `/home/xzhao/github/minix-rs`
- Current commit: `bcfa02514` (short hash, verified via `git rev-parse --short HEAD` on 2026-09-19)
- Task: R-phase rebuild blueprint. Output is `target_dir/doc_rerank_muse.md`. No body text is modified.
- Constraints honored: no reference to `.design/` or `tmp_design_and_todo/`; no reading of other AIs' `doc_rerank_*` products (the three existing `doc_rerank_deepseek/glm/qwen` files in this directory were listed but never opened); the only new file written is this one, carrying the `_muse` suffix.
- Output language: English (per muse-specific requirement).

### 0.1 Scope

In scope (numbered docs + global concepts):

| # | File | Title (short) | Lines |
|---|------|---------------|-------|
| 00 | `00-runtime-overview.md` | Runtime overview and navigation | 76 |
| 01 | `01-kernel-handoff.md` | Kernel handoff (kerninfo, stack, ps_strings) | 173 |
| 02 | `02-crt0-start.md` | Program entry (_start to main) | 150 |
| 03 | `03-runtime-init.md` | Runtime init (__minix_init, IPC vecs) | 148 |
| 04 | `04-ipc-primitives.md` | IPC primitives (6 traps + SENDA queue) | 157 |
| 05 | `05-syscall-mechanism.md` | Syscall protocol (_syscall/_kernel_call/_loadname) | 141 |
| 06 | `06-allocator.md` | Allocator (brk + slab) | 141 |
| 07 | `07-panic-output.md` | Panic and diagnostic output | 147 |
| 08 | `08-pm-syscalls.md` | PM call group | 150 |
| 09 | `09-vfs-syscalls.md` | VFS call group | 173 |
| 10 | `10-vm-syscalls.md` | VM call group | 149 |
| 11 | `11-misc-syscalls.md` | Misc group (sleep/control/clocks/TSC) | 138 |
| 12 | `12-rs-query.md` | RS discovery and info routing | 120 |
| 13 | `13-constants-abi.md` | Constants ABI audit | 110 |
| 99 | `99-global-concepts.md` | Global concepts (endpoint/message/service numbers) | 70 |
| — | Total numbered + 99 | 15 docs | 2043 |

Reference material (not rebuilt, used as input): `plan.md` (373 lines, in force since 2026-08-16, with §5.1/§5.2 coverage contract and §5.3 exclusion table), `todo.md` (V1 architecture review findings V1-P0-1..V1-P3-2, most fixed 2026-09-16), `draft/README.md` (placeholder README, archived scope statement).

Out of scope: `draft/` contents beyond the README placeholder; any `.design/` or `tmp_design_and_to

do/` material (not referenced, per project rule); other stages' docs except as boundary contracts (§0.4).

### 0.2 Reading list (what was actually read)

1. All 15 docs above: header declarations read in full (audience, lifecycle position, prerequisites, non-coverage); bodies skimmed for knowledge extraction (concept models, C anchors, Rust anchors, boundary handoffs).
2. `plan.md` in full (§1 lifecycle backbone, §2 numbering, §3.3 boundary table, §4 ARCH A-1..A-10, §5.1/§5.2 coverage contract, §5.3 exclusion table, §7 review record).
3. `todo.md` §§0–2 (V1 findings, coverage matrices PM/VFS/VM/misc/RS/constants) plus fix records for V1-P0-1..V1-P1-4.
4. C ground truth (spot-verified, not fully re-read): `minix3/lib/csu/arch/x86_64/crt0.S` (:44-49 six-instruction entry), `minix3/lib/csu/common/crt0-common.c` (:144-192 `___start`), `minix3/minix/lib/libc/sys/init.c` (:8 constructor, :20-26 init with non-fatal NULL fallback), `minix3/minix/lib/libc/sys/syscall.c` (:9 `_syscall`), `minix3/minix/lib/libsys/kernel_call.c` (:7 `_kernel_call`), `minix3/minix/include/minix/type.h` (:214-244 kerninfo, :229 magic), `minix3/minix/lib/libc/arch/i386/sys/_ipc.S` (six ENTRY points), `minix3/minix/lib/libc/sys/stack_utils.c` (:66 STACK_MIN_SZ).
5. Rust entries: `os/libs/minix-rt/src/` (6 files, lib.rs 348 lines), `os/libs/minix-sys/src/` core domain files (`ipc.rs` 926, `syscall.rs` 1893, `pm.rs` 1153, `vfs.rs` 1155, `vm.rs` 1341, `misc.rs` 531, `rs.rs` 452, `stack.rs` 372, `grant.rs` 347, `arch_trap.rs` 246 lines), `os/libs/minix-types/src/` (errno/kerninfo/ipc wire families).
6. Boundary materials: `rewrite-notes/coordination/edge_todo.md` header (E-MINTYPES-RUNTIME, E-MINSYS-SCOPE, E-CMDSYSFACE, E-INITSYS, E-SYSCALL-SIGN registrations), `draft/README.md` (placeholder scope), `13-stage-ipc/00-ipc-overview.md` header (previous-stage boundary: SysV IPC objects live in ipc-server, not here).
7. Non-C artifacts: `minix3/lib/csu/` layout (Makefile, arch/x86_64, common), `minix3/minix/lib/libc/arch/` (i386 + arm), libc/sys file count (133 .c, verified via `ls | wc -l`), libsys file count (116 .c), libminc file list.

### 0.3 Commands used and key outputs (evidence excerpts)

```bash
wc -l rewrite-notes/14-stage-runtime/0*.md 1*.md 99*.md
# 15 docs, 2043 lines total; thinnest 99 (70), thickest 01/09 (173)
ls minix3/minix/lib/libc/sys/*.c | wc -l          # 133 (matches plan §5.1)
ls minix3/minix/lib/libsys/*.c | wc -l            # 116 (shared+server subset)
sed -n '40,55p' minix3/lib/csu/arch/x86_64/crt0.S # 6 instructions confirmed
find . -name "test-rt-birth*" -not -path "./target/*"  # NO .sh script found (see §3 gap G-NEW-06)
rg -n "14-stage-runtime|08-pm-syscalls|..." os/libs/minix-{rt,sys,types}/src/
# 2 hits: minix-sys/src/lib.rs:21, :28 (reference migration inventory)
git rev-parse --short HEAD                        # bcfa02514
```

---

## 1. C Truth Sequence

Stage type判定: **library/framework type** (not a boot chain, not a service event loop). There is a linear birth prefix (kernel handoff → crt0 → constructor init) followed by a steady-state library regime (mechanism + resources + per-service wrapper families + constants). The telling order below therefore has two parts: (A) the birth prefix in strict execution order, (B) the steady-state families organized by first-use order with trigger timing (per §5.2 parallel-body rule: framework first, then groups, representative member in depth, rest by difference table).

### 1.1 Part A — birth prefix (strict runtime order)

| Step | Action | C anchor | Notes |
|------|--------|----------|-------|
| T-01 | Kernel finishes exec, builds initial stack image, selects user stack top | `minix3/minix/lib/libc/sys/stack_utils.c:66-130` (STACK_MIN_SZ, min_size, fill) | Consumer side; producer is kernel/VM exec path (01-stage-kernel / 02-stage-vm) |
| T-02 | Kernel maps kerninfo page; process wakes with kerninfo pointer + SP | `minix3/minix/include/minix/type.h:214-244` (struct), `:229` (KERNINFO_MAGIC 0xfc3b84bf) | Two-item delivery: page pointer + initial stack top |
| T-03 | Assembly stub aligns stack, shuffles registers, jumps to C entry | `minix3/lib/csu/arch/x86_64/crt0.S:44-49` (andq/subq/3x movq/jmp, 6 instructions) | rdx→rdi, rcx→rsi, rbx→rdx; RBX carries ps_strings |
| T-04 | C entry checks ps_strings, publishes environ/progname/ps_strings globals | `minix3/lib/csu/common/crt0-common.c:144-192` (`___start`) | NULL ps_strings → `_FATAL`; argv[0] basename scan for progname |
| T-05 | Constructor queries kerninfo pointer from kernel | `minix3/minix/lib/libc/sys/init.c:20-26` (`__minix_init`, constructor at :8) | Query-then-validate-then-install, fixed order |
| T-06 | Validate magic; on failure clear pointer and CONTINUE (non-fatal) | `init.c:22-26` (`_minix_kerninfo = NULL` fallback) | C contract: "init has no fatal failure, only diagnosable state" (todo Fix #5) |
| T-07 | If magic + MINIX_KIF_IPCVECS flag + non-NULL vecs, install global IPC table | `init.c:10-18` (default table), `:17` (do_kernel_call slot) | Double condition: flag AND pointer |
| T-08 | Run preinit/init_array, register atexit(_fini), call main, pass return to exit | `crt0-common.c:144-192` (tail of `___start`) | fini_array/atexit currently unregistered gap (see §3) |

### 1.2 Part B — steady-state families (framework first, then groups by first use)

| Step | Action | C anchor | Notes |
|------|--------|----------|-------|
| T-09 | Trap ABI: 6 primitives share one convention (endpoint/msg/status registers, vector 33) | `minix3/minix/lib/libc/arch/i386/sys/_ipc.S` (ENTRY _ipc_send/receive/sendrec/notify/sendnb/senda_intr); `minix/include/minix/ipcconst.h` (SEND=1..SENDA=16); `arch/i386/include/ipcconst.h` (vectors 32/33) | ARCH A-6: x86-64 `syscall` insn replaces int 33 |
| T-10 | MINIX_KERNINFO query trap (vector-33 family member 6) | `ipc_minix_kerninfo.S` | Owned by 04, referenced by 03 |
| T-11 | SENDA batch table submit (async, poll for results) | `minix3/minix/lib/libsys/asynsend.c` | &mut self replaces C `inside` flag (reasonable Rust-ization) |
| T-12 | `_syscall` protocol: m_type=callnr; transport fail overwrites m_type; negative m_type → errno | `minix3/minix/lib/libc/sys/syscall.c:9-25` | Two-layer failure on one field |
| T-13 | `_kernel_call`: ENOTREADY retry with linear backoff tickdelay(t++) | `minix3/minix/lib/libsys/kernel_call.c:7-21` | Server-internal only; never blocks a service |
| T-14 | `_loadname`: short names (≤40 incl. NUL) inline, long names by pointer+len | `minix3/minix/lib/libc/sys/loadname.c` (M_PATH_STRING_MAX=40) | 40 = inline buffer size, no more no less |
| T-15 | brk/sbrk: compare-and-request boundary moves via VM_BRK | `minix3/minix/lib/libc/sys/brk.c`, `sbrk.c`; `_brksize` asm | Cache boundary, request only on change |
| T-16 | malloc family (NetBSD, brk/mmap hybrid) supplies heap blocks | `minix3/minix/lib/libc/stdlib/malloc.c` | ARCH A-3 replaces with slab-over-mmap |
| T-17 | kputc staging buffer → sys_diagctl direct-to-kernel channel | `minix3/minix/lib/libsys/kputc.c:17`, `sys_diagctl.c` | Bypasses VFS by design |
| T-18 | panic/assert ladder (identity → message → stacktrace → hooks → abort steps → spin) | `minix3/minix/lib/libsys/panic.c:34-66`, `assert.c` | Single-track convergence required (todo Fix #2) |
| T-19 | PM family: fork/exit/exec/wait4/kill/signal/creds/time/itimer/reboot/srv_fork/srv_kill (47 callnrs) | `minix3/minix/include/minix/callnr.h:14-60`; `libc/sys/fork.c`, `_exit.c`, `execve.c`, `wait4.c`, `kill.c`, … | Representative + difference-table shape |
| T-20 | VFS family: read/write/open/close/lseek/stat/ioctl/fcntl/pipe/dup/select/… (64 callnrs) | `callnr.h:72-135`; `libc/sys/read.c`, `write.c`, `open.c`, `vectorio.c`, … | 4-shape model (read/write, open/close, seek, scatter) |
| T-21 | VM family: mmap/munmap/brk/exit/fork/map_phys/info/procctl/cache/getrusage (49 callnrs + client lib) | `minix3/minix/include/minix/com.h:627-780`; `libc/sys/mmap.c`; `libsys/vm_*.c` | For-self vs for-other flag; sentinel→Result (todo Fix #6) |
| T-22 | Sleep-via-select composition; svrctl group-char dispatch; kerninfo clock direct reads; TSC splice | `libc/sys/nanosleep.c:28-92`; `svrctl.c`; `libsys/getticks.c`, `getuptime.c`, `clock_time.c`; `libc/gen/read_tsc_64.c` | Four heterogeneous mechanisms (split proposed §4) |
| T-23 | RS lookup (name→endpoint, always fresh) + getepinfo/getprocnr/getsysinfo routing | `libc/sys/minix_rs.c`; `libsys/getepinfo.c`, `getprocnr.c`, `getsysinfo.c`; `minix/include/minix/rs.h` | Never cache: reboot visibility beats round trips |
| T-24 | Exit path: atexit/fini_array drain → PM_EXIT message → no return | `libc/sys/_exit.c`; `crt0-common.c` atexit(_fini) registration | Currently homeless terminus (new doc §4) |

Stage-type statement: birth prefix (T-01..T-08) is a linear startup chain and is told in runtime order; steady state (T-09..T-24) is a parallel set of call families told framework-first (04/05) then by first-use groups (06/07 resources, 08..12 service families, 13/99 contracts). No sequence-difference table is needed because the teaching order already matches runtime order for the linear part; the grouping of the parallel part is declared in §4 reading paths.

---

## 2. Knowledge Pool

Types: C=concept, M=mechanism, D=data structure, I=interface/protocol, K=constraint/invariant, A=architectural evolution, T=tooling/engineering, E=test property. Source: S=stock (from existing docs), N=new (from C/non-C/Rust truth, absent in docs). Alignment key for bagging merge: name + anchor.

| ID | Name | Type | Src | Current location | Anchor | Reader benefit (answers…) |
|----|------|------|-----|------------------|--------|---------------------------|
| K-001 | Runtime is a shared userland library layer, not a server | C | S | 00 Ch1 | plan.md §1.1; draft/README.md scope | "Where do I look for birth/call/exit mechanics shared by all userland?" |
| K-002 | Lifecycle backbone: delivery → crt0 → init → service → terminus | C | S | 00 Ch2 | plan.md §1.1 diagram | "Which doc owns which point of a process lifetime?" |
| K-003 | ARCH A-2: no_std + core/alloc replaces libc (libminc composition as exclusion baseline) | A | S | 00 Ch1/Ch4 | `minix3/minix/lib/libminc/Makefile` | "Why is there no stdio/string/ctype port?" |
| K-004 | kerninfo page struct + magic + flags word | D | S | 01 Ch1 | `type.h:214-244`, `:229` magic | "How does a newborn process verify the kernel's delivery?" |
| K-005 | kuserinfo struct + KUSERINFO_HAS_FIELD length rule | M | S | 01 | `type.h` kuserinfo; `minix_kerninfo` Outer + legacy offset-2440 fallback | "How do old binaries detect new fields?" |
| K-006 | Initial stack image + user_sp selection (new field first, legacy fallback) | M | S | 01 | `stack_utils.c:66-130`; `minix/param.h` | "Where does the initial stack top come from?" |
| K-007 | ps_strings descriptor (argc/argv/env packed in one pointer) | D | S | 01/02 | `crt0-common.c:144-160` | "Why does Minix pass one pointer where Linux passes three?" |
| K-008 | x86-64 entry: 6 instructions, RBX=ps_strings, 16B align + shadow | M | S | 02 §1.1 | `crt0.S:44-49` | "What exactly executes before the first C line?" |
| K-009 | ___start pipeline: check → publish → progname → _libc_init → preinit → atexit → main → exit | M | S | 02 | `crt0-common.c:144-192` | "What stands between _start and main?" |
| K-010 | Static-link branch death (_DYNAMIC/loader NULL path) | M | S | 02 (thin) | `crt0-common.c` weak `_DYNAMIC` refs | "Why can the dynamic branch never trigger here?" |
| K-011 | __minix_init constructor: query → validate → conditionally install | M | S | 03 §1.1 | `init.c:8, :20-26` | "In what order does global communication state become ready?" |
| K-012 | Non-fatal init: failure clears pointer, execution continues | K | S | 03 §4 (post-Fix #5) | `init.c:22-26` "not fatal" | "What may a program assume when kerninfo is absent?" |
| K-013 | IPC vecs double condition (flag AND non-NULL pointer) | K | S | 03 §1.2 | `init.c:10-18` | "When is the kernel's function table safe to install?" |
| K-014 | errno slot model (C global + __errno()) vs Result<_, Errno> | A | S | 03 | `libc/gen/_errno.c`; ARCH A-5 | "Where did the global errno go?" |
| K-015 | Six trap primitives sharing one register convention | I | S | 04 §1.1 | `_ipc.S` ENTRY points; `ipcconst.h` 1..5,16; vectors 32/33 | "Which trap does what, and what do they share?" |
| K-016 | Status-word two-segment encoding + SENDA table producer/consumer | M | S | 04 | `asynsend.c`; `asynmsg_t`/AMF_* | "How are async results collected?" |
| K-017 | x86-64 trap evolution (int 33 → syscall insn + SyscallArch trait) | A | S | 04 | ARCH A-6; `arch_trap.rs` 246 lines | "How is the trap ABI abstracted over architectures?" |
| K-018 | _syscall two-layer failure on m_type (transport vs service) | M | S | 05 §1.1 | `syscall.c:9-25` | "How do I tell a lost message from a refused request?" |
| K-019 | _kernel_call ENOTREADY linear-backoff retry | M | S | 05 | `kernel_call.c:7-21` | "Why do in-service calls spin instead of blocking?" |
| K-020 | _loadname 40-byte inline/pointer dual path | M | S | 05 | `loadname.c`; M_PATH_STRING_MAX=40 | "How do variable-length paths fit a 56-byte message?" |
| K-021 | brk/sbrk boundary cache (compare address, request only on change) | M | S | 06 §1.1 | `brk.c`, `sbrk.c` | "Why doesn't every sbrk trap to VM?" |
| K-022 | Slab-over-mmap allocator replacing NetBSD malloc | A | S | 06 | ARCH A-3; `alloc.rs` 654 lines | "How are small/large requests routed differently?" |
| K-023 | kputc staging buffer (NUL flushes, full flushes, empty+NUL no-op) | M | S | 07 §1.1 | `kputc.c:17`; DIAG_BUFSIZE | "Why can't panic output use the filesystem?" |
| K-024 | Panic ladder (report → hooks → exit attempts → spin), single track | M | S | 07 §1.2 (post-Fix #2) | `panic.c:34-66` | "What happens when every fallback also fails?" |
| K-025 | Panic hook single registry in minix-types (kernel writes, rt reads) | M | S | 07 implic
...[truncated 17919 chars]