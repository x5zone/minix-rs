# 14-stage-runtime 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/14-stage-runtime/`
> **目标**: 以**进程运行时生命周期为主线**重组 userland runtime 全部文档；syscall 封装按服务分组为次主线；最终覆盖 Minix3 userland runtime 全部语义，支撑 `os/libs/minix-rt` + `os/libs/minix-sys` 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/plan.md`（plan 结构参照）、`minix3/lib/csu/` + `minix3/minix/lib/libc/` + `minix3/minix/lib/libminc/` + `minix3/minix/lib/libsys/`（ground truth）、`os/libs/minix-rt/` + `os/libs/minix-sys/` + `os/libs/minix-types/`（Rust 实现）

---

## 1. 背景与动机

### 1.1 与 VM 的差异：runtime 不是 server，主线重新定义

`02-stage-vm` 以 **VM server 启动顺序为主线**——VM 是一个有明确 `init_vm()` 启动链 + 主循环的用户态服务。**14-stage-runtime 不是 server**（无主循环、无 IPC 分发），它是**一切 userland（server/fs/driver/命令）共享的运行时库层**，对应 C 源 `lib/csu/`（crt0）+ `minix/lib/libc/`（syscall 封装与运行时初始化）+ `minix/lib/libminc/`（服务/驱动最小 libc）+ `minix/lib/libsys/` 的共享部分（IPC 原语、`_kernel_call`、诊断输出）。

因此主线从"server 启动顺序"改为 **进程运行时生命周期**（读者学习顺序 = 一个进程从内核交付到退出的实际执行顺序）：

```
内核交付新进程（exec 完成，内核映射 kerninfo + 构建初始栈）
  │
  ▼  01-kernel-handoff：kerninfo/kuserinfo ABI、栈布局、ps_strings
  ├─ __minix_init()（constructor，init.c:15）← 03：kerninfo 获取 + IPC vecs 安装
  │
  ▼  02-crt0-start：___start（crt0-common.c）← 02：argc/argv/environ、init_array
  ├─ init_array → main()
  │
  ▼  主逻辑运行期间（08~12 次主线：syscall 封装按服务分组）
  ├─ PM（fork/exit/exec/wait/kill/signal）    ← 08
  ├─ VFS（read/write/open/close/stat/ioctl）   ← 09
  ├─ VM（mmap/munmap/brk/sbrk）                ← 10
  ├─ 时钟/系统信息/杂项                        ← 11
  └─ RS（服务发现/查询）                       ← 12
  │
  ▼  终局
  ├─ panic 路径（诊断输出 → 终止）              ← 07
  └─ exit 路径（atexit → PM_EXIT）             ← 08（PM 组）
```

**每篇文档必须能回答一个问题：它位于进程运行时生命周期的哪个位置**（诞生/初始化/机制/资源/服务/终局）。这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则，由 `02-stage-vm/plan.md §1.2` 迁移而来。

### 1.2 旧内容的问题与归档

14-stage-runtime 原仅有占位 `README.md`（已移入 `draft/README.md`），无旧主线文档。占位 README 的 scope 定义（minix-rt/minix-sys 实装 + errno/termios 常量 + 静态链接 ARCH）保留为素材，本 plan 将其扩展为完整语义覆盖契约。

### 1.3 syscall 封装次主线

syscall 封装不充当概念引入的驱动，而是按**服务分组**展开（每组一篇），其调用路径图在各组文档内部绘制：

```
进程运行中 → 调用 libc/RT 函数
  ├─ 05 机制：_syscall(endpoint, callnr, &m) → ipc_sendrec → 内核陷阱
  ├─ 08 PM_PROC_NR（callnr.h PM_BASE+1~47）
  ├─ 09 VFS_PROC_NR（callnr.h VFS_BASE+0~63）
  ├─ 10 VM_PROC_NR（com.h VM_RQ_BASE+0~48）
  ├─ 11 杂项/系统信息（MIB/svrctl/kerninfo 直读/select 组合）
  └─ 12 RS_PROC_NR（RS_LOOKUP 等服务查询）
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel`/`02-stage-vm` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。draft 素材保留原样。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | 变更 |
|------|------|------|---------|--------|-----------|------|
| 0 总览 | 00 | `00-runtime-overview.md` | runtime 是什么、生命周期主线图、文档导航 | 全部 | 全部 | 新建 |
| 1 内核交付与程序启动 | 01 | `01-kernel-handoff.md` | minix_kerninfo/kuserinfo、userland ABI、exec 初始栈、ps_strings | `minix/include/minix/type.h`、`minix/lib/libc/sys/kernel_utils.c`、`stack_utils.c`、`lib/csu/` 栈约定 | `minix-rt`（kerninfo 访问）、`minix-boot`（user_sp 提供方） | 新建 |
| 1 | 02 | `02-crt0-start.md` | `_start`/`___start`、argc/argv/environ、init_array/fini_array、静态链接（ARCH A-1） | `lib/csu/common/crt0-common.c`、`lib/csu/arch/x86_64/crt0.S`、`lib/csu/common/crtbegin.c` | `minix-rt`（`_start`） | 新建 |
| 2 运行时初始化 | 03 | `03-runtime-init.md` | `__minix_init` constructor、kerninfo 校验、IPC vecs 安装、TLS（ARCH A-4）、environ/`__progname`、errno 槽（ARCH A-5） | `minix/lib/libc/sys/init.c`、`environ.c`、`libc/gen/_errno.c`、`getprogname.c`/`setprogname.c` | `minix-rt`（`init`）、`minix-types`（Errno） | 新建 |
| 3 系统调用机制 | 04 | `04-ipc-primitives.md` | SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/SENDA、陷阱 ABI（ARCH A-6）、IPC 状态编码、asynsend | `minix/include/minix/ipcconst.h`、`arch/i386/include/ipcconst.h`、`libc/arch/i386/sys/_ipc.S`、`ipc_minix_kerninfo.S`、`libsys/asynsend.c` | `minix-sys`（send/receive/sendrec/notify）、`minix-types`（Notify/ipc） | 新建 |
| 3 | 05 | `05-syscall-mechanism.md` | `_syscall` 协议（m_type=callnr、负值→errno）、`_kernel_call`（ENOTREADY 重试）、`_loadname` | `minix/lib/libc/sys/syscall.c`、`loadname.c`、`libsys/kernel_call.c` | `minix-sys`（`sendrec` 之上的 `syscall` 族） | 新建 |
| 4 内存分配与终局 | 06 | `06-allocator.md` | 分配器：brk/sbrk 与 VM 关系、NetBSD malloc → slab over VM mmap（ARCH A-3）、`_brksize` | `minix/lib/libc/sys/brk.c`、`sbrk.c`、`stdlib/malloc.c`（NetBSD）、`libc/arch/.../brksize.S` | `minix-rt`（`alloc`/`free`） | 新建 |
| 4 | 07 | `07-panic-output.md` | 诊断输出路径（libsa printf → kputc → sys_diagctl）、minix-rt panic handler、诊断通道（ARCH A-8） | `minix/lib/libsys/kputc.c`、`sys_diagctl.c`、`panic.c`、`assert.c`、`libminc`（`_snprintf.c`/`fputs.c`）、`libc/gen/itoa.c`、`stderr.c` | `minix-rt`（panic handler）、`minix-sys`（write/诊断） | 新建 |
| 5 syscall 封装（次主线·按服务分组） | 08 | `08-pm-syscalls.md` | PM 组：fork/exit/wait4/execve/kill/signal/uid/gid/rusage/times/itimer/reboot/srv_fork/srv_kill | `minix/lib/libc/sys/fork.c` 等 PM 相关、`libsys/srv_fork.c`、`srv_kill.c`、`libc/gen/raise.c`、`libc/gen/clock.c` | `minix-sys`（pm 模块）、`minix-types`（ipc/pm.rs） | 新建 |
| 5 | 09 | `09-vfs-syscalls.md` | VFS 组：read/write/open/close/lseek/stat/ioctl/fcntl/pipe/dup/chdir/mount/getdents/select/poll/readv/writev | `minix/lib/libc/sys/read.c` 等 VFS 相关、`vectorio.c`（readv/writev） | `minix-sys`（vfs 模块）、`minix-types`（ipc/vfs.rs） | 新建 |
| 5 | 10 | `10-vm-syscalls.md` | VM 组：mmap/munmap/brk/sbrk + VM 客户端库（vm_fork/vm_exit/vm_map_phys/vm_info/vm_cache...） | `minix/lib/libc/sys/mmap.c`、`brk.c`、`sbrk.c`、`libsys/vm_*.c`（用户态 ABI 子集）、`minix/include/minix/vm.h` | `minix-sys`（vm 模块）、`minix-types`（ipc/vm.rs） | 新建 |
| 5 | 11 | `11-misc-syscalls.md` | 杂项/系统信息：nanosleep（select 组合）/sysctl（MIB_PROC_NR）/svrctl/getticks/getuptime/clock_time（kerninfo 直读）/read_tsc_64 | `minix/lib/libc/sys/nanosleep.c`、`__sysctl.c`、`svrctl.c`、`libsys/getticks.c`、`getuptime.c`、`clock_time.c`、`libc/gen/read_tsc_64.c` | `minix-sys`（misc 模块） | 新建 |
| 5 | 12 | `12-rs-query.md` | RS 服务发现：minix_rs_lookup（RS_LOOKUP）、getepinfo/getprocnr/getsysinfo | `minix/lib/libc/sys/minix_rs.c`、`libsys/getepinfo.c`、`getprocnr.c`、`getsysinfo.c`、`minix/include/minix/rs.h` | `minix-sys`（rs 模块） | 新建 |
| 6 常量 ABI | 13 | `13-constants-abi.md` | errno/termios/signal/fcntl/stat/ioctl/wait/resource/times/utsname/callnr 常量对齐 | `include/errno.h`、`sys/sys/errno.h`、`sys/sys/termios.h`、`minix/include/minix/callnr.h`、`com.h` 等 | `minix-types`（errno/endpoint/com 等） | 新建 |
| 99 全局概念 | 99 | `99-global-concepts.md` | endpoint/generation、message 布局、服务号常量、全局状态 | `minix/include/minix/type.h`、`ipc.h`、`com.h`、`endpoint.h`、`const.h`、`config.h` | `minix-types` | 新建 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在生命周期中的位置与下一阶段的入口：

```
01/02（诞生）→ 03（初始化）→ 04/05（机制）→ 06（资源）→ 07（终局骨架）
→ 08~12（服务，次主线）→ 13（常量 ABI）→ 99（全局概念）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel / 02-stage-vm plan §3）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel`/`02-stage-vm` 一致：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的对应、`[ARCH]` 标注
4. **错误处理**——errno 映射（P0：错误类型必须映射 Minix3 errno 值）
5. **测试**——该文档语义模块的 Rust 单测清单与统计
6. **过渡**——本阶段在生命周期中的位置 + 下一阶段入口
7. **参见**——绝对路径引用（doc/code/C 源），绝不引用 `.design/`/`tmp_design_and_todo/`

### 3.2 引用规则

- 各文档之间用新编号交叉引用（如 `08-pm-syscalls.md` §PM_EXIT）
- 与 kernel/VM 文档交叉引用时用 `../01-stage-kernel/NN-*.md`、`../02-stage-vm/NN-*.md`
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"

### 3.3 每篇文档的边界声明

每篇必须含"前置依赖 / 本篇不覆盖什么"声明，写作时禁止内容交叉。关键边界：

| 文档 | 前置依赖 | 职责 | 不覆盖（移交） |
|------|---------|------|---------------|
| 01 | 00、`../01-stage-kernel/09-vm-boot-protocol.md` | kerninfo/kuserinfo ABI、exec 栈构建 | `_start` 内部（02）、syscall 机制（04/05） |
| 02 | 01 | `_start`/`___start`、argv/environ、init_array | kerninfo 获取细节（01）、`__minix_init`（03） |
| 03 | 01/02 | `__minix_init`、IPC vecs、TLS、errno 槽 | 分配器（06）、panic（07） |
| 04 | 03（IPC vecs） | 内核陷阱 ABI、IPC 状态编码 | `_syscall` 消息协议（05）、各服务消息布局（99） |
| 05 | 04 | `_syscall`/`_kernel_call`/`_loadname` | IPC 原语本身（04）、errno 常量值（13） |
| 06 | 03/10（mmap/brk 接口约定） | 分配策略、brk/sbrk 用户态视图 | mmap/brk 消息封装细节（10）、VM 服务端语义（02-stage-vm） |
| 07 | 03、05（write/诊断） | 诊断输出、panic handler | exit 消息语义（08）、printf 全量实现（[ARCH] core 替代） |
| 08 | 05 | PM 全部调用封装 | 消息布局细节（99）、常量值（13） |
| 09 | 05 | VFS 全部调用封装 | 网络 socket 族（排除→17-stage-net）、消息布局（99） |
| 10 | 05 | VM 全部调用封装、VM 客户端库 | VM 服务端实现（02-stage-vm）、RS 交互 vm_*（边界） |
| 11 | 05 | 时钟/系统信息/杂项 | 时钟服务端（06-stage-sched 等） |
| 12 | 05 | RS 查询/服务发现 | RS 服务端（03-stage-rs） |
| 13 | 无 | 常量值全集 | 常量如何被使用（各文档） |
| 99 | 无 | 全局概念 | 一切机制（00~13） |

### 3.4 测试基线（截至 2026-08-16）

- `os/libs/minix-rt`、`os/libs/minix-sys` 当前为 stub，`cargo test -p minix-rt` / `-p minix-sys` 无实质测试
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.5 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/runtime/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 14-stage-runtime 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **用户态静态链接** | 动态链接：`libexec/ld.elf_so` + `libc.so`（crt0-common.c 的 `_DYNAMIC`/`rtld_DYNAMIC` 弱引用路径） | 全部静态链接，无 ld.elf_so；`_start` 直接调用 `main` | 02 | 设计决策（draft README 已声明） |
| A-2 | **no_std + Rust core/alloc 替代 libc** | libminc/libc 提供 atoi/strtol/printf/string/ctype/regex/compiler-rt 等 | 不移植，用 `core`/`alloc` 语义替代（`libminc/Makefile` 组成作为排除基线） | 00/02/05/07/13 | 设计决策 |
| A-3 | **分配器** | NetBSD malloc（brk/mmap 混合、magic 节、sectionify 插桩） | slab 分配器（VM mmap 供给小块缓存 + 大块直接 mmap），`minix_rt::alloc/free` | 06 | 已声明（draft README），需设计 |
| A-4 | **用户态 TLS** | i386 无用户态 TLS（NetBSD i386 不用 FS/GS 段） | x86-64 FS 段 + 架构 trait 抽象（`minix_arch`） | 03 | 新增（x86-64 必需） |
| A-5 | **errno 模型** | C 全局 `errno` + `__errno()` 函数槽（`libc/gen/_errno.c`） | Rust `Result<_, Errno>`（`minix-types::Errno`），无全局可变状态；syscall 负值回传协议保留 | 03/05/13 | 已实现（minix-types） |
| A-6 | **syscall 陷阱 ABI** | i386 `int 32`（KERVEC_INTR）/`int 33`（IPCVEC_INTR）+ 寄存器约定（eax/ebx/ecx） | x86-64 `syscall` 指令 + `SyscallArch` trait（多架构），`minix_arch::syscall` | 04 | 设计决策 |
| A-7 | **64 位地址空间** | 32 位 `vir_bytes`/`off_t`/指针、message 56 字节负载 | 64 位指针 + `u64` 字段（`minix-types` 已适配） | 01/10/13 | 已实现 |
| A-8 | **诊断输出通道** | `kputc` → `sys_diagctl`（内核日志缓冲） | 待定：直接 `write(STDERR)`（VFS）或保留内核诊断通道；panic handler 先 spin 后输出（draft README 已声明演进顺序） | 07 | 设计决策（见 §7.3） |
| A-9 | **信号上下文 ABI** | NetBSD `_mcontext`/`_ucontext`/`sigreturn` 兼容层 | Rust 类型建模信号语义（08），不移植 C 上下文结构 | 08 | 设计差异（排除表对应项） |
| A-10 | **服务端 syscall 面** | `libsys/sys_*.c` 全套内核调用封装（server 专用） | 归各 server stage（01-stage-kernel syscall 文档 + 各 server crate），不进入 minix-sys | 排除 | 边界决策 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射

**`lib/csu/`（crt0）**

| C 文件 | 新文档 | 核对 |
|--------|--------|------|
| `lib/csu/common/crt0-common.c` | 02 | 已核对（`___start`、argv/environ、init_array/fini_array） |
| `lib/csu/arch/x86_64/crt0.S` | 02 | 已核对（`__start` 栈对齐、参数搬运） |
| `lib/csu/common/crtbegin.c`、`crti.S`/`crtn.S`/`crtend.S`、`compident.S`、`sysident.S` | 02（构造/析构框架，静态链接语义） | 已核对 |
| `lib/csu/README` | 00（参考） | 已核对 |

**`minix/lib/libc/sys/`（syscall 封装与运行时，133 个 .c + MISSING_SYSCALLS/Makefile.inc = 135 条目）**

| C 文件（代表性分组） | 新文档 | 核对 |
|--------|--------|------|
| `init.c`、`environ.c` | 03 | 已核对 |
| `kernel_utils.c`、`stack_utils.c` | 01 | 已核对 |
| `syscall.c`、`loadname.c` | 05 | 已核对 |
| `brk.c`、`sbrk.c` | 06 | 已核对 |
| `mmap.c` | 10 | 已核对（含 minix_mmap_for/minix_vfs_mmap/vm_remap/vm_unmap/vm_getphys/vm_getrefcount） |
| `fork.c`、`_exit.c`、`execve.c`、`wait4.c`、`kill.c`、`getpid.c`、`getppid.c`、`getuid.c`、`geteuid.c`、`getgid.c`、`getegid.c`、`setuid.c`、`setgid.c`、`setgroups.c`、`getgroups.c`、`getpgrp.c`、`setpgid.c`、`getsid.c`、`setsid.c`、`getitimer.c`、`setitimer.c`、`getrlimit.c`、`setrlimit.c`、`getrusage.c`、`gettimeofday.c`、`settimeofday.c`（clock_settime 组合）、`stime.c`、`clock_getres.c`、`clock_gettime.c`、`clock_settime.c`（均 PM_CLOCK_*）、`sigaction.c`、`sigpending.c`、`sigprocmask.c`、`sigsuspend.c`、`sigreturn.c`、`ptrace.c`、`reboot.c`、`issetugid.c`、`vfork.c`（PM_FORK）、`priority.c`、`libc/gen/raise.c`（kill 组合）、`libc/gen/clock.c`（times→getrusage 组合）、`lib/libc/gen/times.c`（getrusage 组合）、`libc/gen/waitpid.c`（wait4 组合）、`libc/gen/execle.c`、`libc/gen/sleep.c`/`usleep.c`（nanosleep→select 组合）、`libc/gen/sigsetops.c` | 08 | 已核对（PM 组，含 gen 组合面） |
| `read.c`、`write.c`、`open.c`、`close.c`、`lseek.c`、`stat.c`（stat/fstat/lstat）、`ioctl.c`、`fcntl.c`、`pipe.c`（VFS_PIPE2）、`dup.c`/`dup2.c`（fcntl F_DUPFD 组合）、`chdir.c`/`fchdir.c`、`chmod.c`、`fchmod.c`、`chown.c`、`fchown.c`、`access.c`、`umask.c`、`link.c`、`unlink.c`、`rename.c`、`mkdir.c`、`mkfifo.c`（mknod 组合）、`mknod.c`、`rmdir.c`、`symlink.c`、`readlink.c`、`truncate.c`、`ftruncate.c`、`fsync.c`、`sync.c`、`mount.c`（mount/umount）、`getdents.c`、`select.c`、`poll.c`（select 组合）、`getvfsstat.c`、`statvfs.c`、`fstatvfs.c`、`fstatfs.c`（fstatvfs 组合）、`vectorio.c`（readv/writev）、`pread.c`/`pwrite.c`（lseek+read/write 组合） | 09 | 已核对（VFS 组，含组合面标注） |
| `nanosleep.c`（select 组合）、`__sysctl.c`（MIB_PROC_NR，10-stage-mib）、`libc/gen/read_tsc_64.c`、`libsys/getticks.c`/`getuptime.c`/`clock_time.c`（kerninfo 直读，01 消费面） | 11 | 已核对（杂项/系统信息；时钟 syscall 面已并入 08 PM 组） |
| `minix_rs.c` | 12 | 已核对（RS_LOOKUP） |
| `svrctl.c` | 12（边界声明） | 已核对（服务控制，`service` 命令面） |
| `_mcontext.c`、`_ucontext.c` | 08（信号语义）| 已核对（NetBSD 兼容层，A-9） |
| `socket*.c`（accept/bind/connect/listen/recvfrom/recvmsg/sendmsg/sendto/setsockopt/getsockopt/getsockname/getpeername/shutdown/socket/socketpair） | 排除（17-stage-net） | 已核对 |
| `shmat.c`/`shmctl.c`/`shmget.c` | 排除（IPC 服务，13-stage-ipc 边界） | 已核对 |
| `gcov_flush_sys.c`、`sprofile.c` | 排除（profiling，A-2） | 已核对 |
| `posix_spawn.c`、`m_closefrom.c`、`sizeup.c` | 排除（命令层，18-stage-commands） | 已核对 |
| `MISSING_SYSCALLS`（lwp_*/acct/mprotect/msync/...） | 排除表（NetBSD 未移植面，WONTFIX 标注） | 已核对 |

**`minix/lib/libc/gen/` + `minix/lib/libminc/`**

| C 文件 | 新文档 | 核对 |
|--------|--------|------|
| `libc/gen/read_tsc_64.c` | 11 | 已核对 |
| `libc/gen/itoa.c`、`stderr.c` | 07 | 已核对（打印/诊断辅助） |
| `libc/gen/raise.c`、`clock.c` | 08（kill/times 组合面） | 已核对 |
| `libc/gen/_errno.c`、`getprogname.c`、`setprogname.c` | 03（errno 槽 A-5、progname） | 已核对 |
| `libc/gen/execle.c`、`sleep.c`、`usleep.c`、`waitpid.c`、`sigsetops.c` | 08（PM 组组合面） | 已核对 |
| `libminc/Makefile` | 00/§5（libminc 组成 = 覆盖契约基线） | 已核对 |
| `libminc/atoi.c`、`strtol.c`、`fputs.c`、`_snprintf.c` | [ARCH] A-2（core 替代） | 已核对 |
| `libc/gen/fslib.c` | 排除（FS 专用 → 15-stage-fs） | 已核对 |
| `libc/gen/configfile.c`、`mtab.c`、`fsversion.c`、`getpass.c`、`gcov*.c` | 排除（命令层/工具） | 已核对 |

**`minix/lib/libsys/`（共享部分）**

| C 文件 | 新文档 | 核对 |
|--------|--------|------|
| `kernel_call.c` | 05（`_kernel_call`、ENOTREADY 重试） | 已核对 |
| `kputc.c`、`sys_diagctl.c`、`panic.c`、`assert.c` | 07 | 已核对 |
| `asynsend.c` | 04（SENDA 辅助） | 已核对 |
| `getticks.c`、`getuptime.c`、`clock_time.c` | 11 | 已核对 |
| `getsysinfo.c`、`getepinfo.c`、`getprocnr.c` | 12 | 已核对 |
| `srv_fork.c`、`srv_kill.c` | 08（PM_SRV_FORK/PM_SRV_KILL） | 已核对 |
| `vm_fork.c`、`vm_exit.c`、`vm_willexit`（vm_exit.c 内）、`vm_map_phys.c`、`vm_unmap_phys`（vm_map_phys.c 内）、`vm_info.c`、`vm_procctl.c`、`vm_cache.c`、`vm_getrusage.c` | 10（VM 客户端库·用户态 ABI 子集） | 已核对 |
| `vm_set_priv.c`、`vm_update.c`、`vm_memctl.c`、`vm_prepare.c` | 边界（RS/集成 stage，对应 02-stage-vm 25-rs-services） | 已核对 |
| `env_parse.c`、`optset.c`、`sef*.c`、`ds.c`、`taskcall.c`、`sys_*.c`、`get_randomness.c`、`copyfd.c`、`closenb.c`、`mapdriver.c`、`pci_*.c`、`socketpath.c`、`sffs*.c` 等 | 排除（server 专用库，归各 server stage / minix-sef / 07-stage-ds） | 已核对 |

**`minix/include/`（头文件）**

| 头文件 | 归属 | 核对 |
|--------|------|------|
| `minix/type.h`（minix_kerninfo/kuserinfo/endpoint） | 01/99 | 已核对 |
| `minix/ipc.h`、`ipcconst.h` | 04/05/99 | 已核对 |
| `minix/com.h`、`const.h`、`config.h`、`endpoint.h` | 99 | 已核对 |
| `minix/callnr.h` | 08/09/10/11/12 | 已核对 |
| `minix/vm.h` | 10 | 已核对 |
| `minix/param.h`（user_sp/stack） | 01 | 已核对 |
| `minix/syslib.h`、`sysutil.h` | 05/07/11/12 | 已核对 |
| `minix/rs.h` | 12 | 已核对 |
| `minix/u64.h` | 11 | 已核对 |
| `minix/sys_config.h`、`minix/sysinfo.h` | 11/12 | 已核对 |
| `include/errno.h`、`sys/sys/errno.h` | 13 | 已核对 |
| `sys/sys/termios.h`、`signal.h`、`fcntl.h`、`stat.h`、`ioctl.h`、`wait.h`、`resource.h`、`times.h`、`utsname.h`、`types.h` | 13 | 已核对 |
| `arch/i386/include/ipcconst.h`（KERVEC_INTR=32/IPCVEC_INTR=33） | 04 | 已核对 |

### 5.2 语义模块覆盖清单（函数级）

> 每篇文档必须覆盖的函数清单以 §5.1 映射 + libminc/Makefile 组成为基线。以下列出跨文件的关键语义，防止"文件有映射但函数漏掉"：

- **01**：`get_minix_kerninfo`、`minix_get_user_sp`、`minix_stack_params`、`minix_stack_fill`、`struct minix_kerninfo` 全字段、`MINIX_KIF_*` 标志、`struct kuserinfo`、`KUSERINFO_HAS_FIELD`、`struct ps_strings`
- **02**：`_start`/`__start`/`___start`、`environ`、`__progname`、`__ps_strings`、`_preinit`/`init_array`/`fini_array` 处理、`_libc_init` 调用、main 调用与返回值 → exit
- **03**：`__minix_init`（constructor）、`_minix_kerninfo`、`_minix_ipcvecs`、`ipc_minix_kerninfo`、`KERNINFO_MAGIC`（0xfc3b84bf）、`MINIX_KIF_IPCVECS`、`_libc_init`、errno 槽（A-5）、TLS（A-4）
- **04**：`_ipc_send_intr`/`_ipc_receive_intr`/`_ipc_sendrec_intr`/`_ipc_notify_intr`/`_ipc_sendnb_intr`/`_ipc_senda_intr`、`_do_kernel_call_intr`、`ipc_minix_kerninfo.S`、`SEND=1`/`RECEIVE=2`/`SENDREC=3`/`NOTIFY=4`/`SENDNB=5`/`MINIX_KERNINFO=6`/`SENDA=16`、`IPC_STATUS_*` 编码、`asynmsg_t`/`AMF_*` 标志
- **05**：`_syscall`（m_type=callnr、ipc_sendrec 失败→m_type=status、负值→errno=-m_type）、`_kernel_call`（ENOTREADY 重试 + tickdelay）、`_loadname`（M_PATH_STRING_MAX=40 内联/指针）
- **06**：`brk`（`_brksize` 比较 + VM_BRK）、`sbrk`（溢出检测）、`brksize.S`、malloc 族（[ARCH] A-3：slab + 大块 mmap）
- **07**：`kputc`（DIAG_BUFSIZE 缓冲）、`sys_diagctl`（DIAGCTL_CODE_*）、`panic`、`assert`、`_snprintf`/`itoa`（打印辅助）、panic handler（[ARCH] A-8）
- **08**：PM 调用全清单（callnr.h PM_BASE+1~47）：PM_FORK/EXIT/WAIT4/GETPID/SETUID/.../KILL/EXEC/SIGACTION/GETTIMEOFDAY/GETRUSAGE/GETPRIORITY/ITIMER/CLOCK_GETTIME/CLOCK_SETTIME/STIME/PTRACE/REBOOT/SRV_FORK/SRV_KILL/GETEPINFO/GETPROCNR/GETSYSINFO 等 + 每封装的 message 字段 + gen 组合面（times/waitpid/raise/sleep/execle）
- **09**：VFS 调用全清单（callnr.h VFS_BASE+0~63）：VFS_READ/WRITE/LSEEK/OPEN/CREAT/CLOSE/.../IOCTL/FCNTL/PIPE2/SELECT/GETDENTS/SOCKET 族（排除标注）等
- **10**：VM 调用（com.h VM_RQ_BASE+0~48）：VM_EXIT/FORK/BRK/MMAP/MUNMAP/MAP_PHYS/UNMAP_PHYS/REMAP/SHM_UNMAP/GETPHYS/GETREF/INFO/PROCCTL/VFS_MMAP/GETRUSAGE + VM 客户端库函数（mmap/munmap/minix_mmap_for/minix_vfs_mmap/vm_remap/vm_remap_ro/vm_unmap/vm_getphys/vm_getrefcount/vm_fork/vm_exit/vm_willexit/vm_map_phys/vm_unmap_phys/vm_info_*/vm_procctl_*/vm_set_cacheblock/vm_map_cacheblock/vm_forget_cacheblock/vm_clear_cache/vm_getrusage）
- **11**：nanosleep（select 组合）/__sysctl（MIB_SYSCTL）/svrctl（PM_SVRCTL+VFS_SVRCTL）/getticks/getuptime/clock_time（kerninfo 直读）/read_tsc_64
- **12**：`minix_rs_lookup`（RS_LOOKUP、m_rs_req.name/name_len）、`getepinfo`、`getprocnr`、`getsysinfo`（who/what/where 协议）
- **13**：errno 全值表（`sys/sys/errno.h`）、termios（`sys/sys/termios.h`）、signal 编号、fcntl/stat/ioctl/wait/resource/times/utsname 常量、callnr.h 全表、com.h endpoint 常量

### 5.3 排除表（WONTFIX / 移交）

| 排除项 | 理由 | 去向 |
|--------|------|------|
| libc 全量（stdio/string/ctype/regex/malloc 实现/compiler-rt） | [ARCH] A-2：Rust core/alloc 语义替代 | — |
| socket 族封装（17 个文件） | 网络服务依赖 net stage | 17-stage-net |
| shmget/shmat/shctl | 用户态 IPC 服务 | 13-stage-ipc |
| `sys_*.c` 内核调用封装（~50 文件） | server 专用，A-10 | 各 server stage |
| `sef*.c`（2210 行） | 独立 crate `minix-sef` | minix-sef stage |
| `ds.c` | DS 客户端 | 07-stage-ds |
| `env_parse.c`/`optset.c` | server 启动参数解析 | 各 server stage |
| `fslib.c` | FS server 专用 | 15-stage-fs |
| `vm_set_priv/vm_update/vm_memctl/vm_prepare` | RS↔VM 特权/活更新协议 | 集成 stage（02-stage-vm 25 对应） |
| profiling（gcov/sprofile） | 工具链面 | 19-stage-integration |
| `posix_spawn.c`/`m_closefrom.c`/`sizeup.c`/`__getcwd.c`/`__getlogin.c`/`configfile.c` 等 | 命令层（含纯用户态组合函数） | 18-stage-commands |
| `pathconf.c`/`fpathconf.c` | 纯用户态（limits.h 常量表，无 syscall 面） | [ARCH] A-2 排除 |
| `MISSING_SYSCALLS` 全部 | NetBSD 未移植面 | WONTFIX 标注 |

---

## 6. 实施顺序

1. 00-runtime-overview → 01~03（生命周期骨架）→ 04/05（机制）→ 06/07（资源与终局）→ 08~12（服务分组）→ 13（常量）→ 99（全局）
2. 每篇文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Gate H.6）
3. 每篇改写后按 §3.5 跑 Blocker Gates，产物写入 `.review/codex/runtime/{NN}-{name}/`
4. checklist.md（顶层，后续补建）编号/路径更新

### 6.1 文档改写状态跟踪

| 编号 | 状态 | 日期 | 说明 |
|------|------|------|------|
| 00~13、99 | pending | 2026-08-16 | plan.md 定稿后创建最小骨架（本计划第 8 步） |

---

## 7. Review 记录

> 本节记录 plan.md 自身的 review 过程（深度 review + minix3 回归 review），与最终 plan.md 同文档交付，保证"覆盖完整性核对"可追溯。

### 7.1 深度 review（2026-08-16）

**范围**：语义全覆盖 + 语义模块合理拆分 + 叙事顺序无前向引用 + 与 VM plan 结构对齐。

| # | 发现 | 等级 | 修复 |
|---|------|------|------|
| R-1 | 初稿主线沿用"server 启动顺序"表述，与 1.1 已声明的"runtime 非 server"矛盾 | P1 | 主线改为"进程运行时生命周期"，时序图重画 |
| R-2 | 初稿 08~12 与 04/05 顺序（服务分组在机制前）导致 mmap/brk 前向引用 | P1 | 机制（04/05）前移至资源（06）与服务（08~12）之前 |
| R-3 | 分配器（06）依赖 VM mmap/brk 消息语义（10），存在前向引用 | P1 | 06 边界声明"mmap/brk 消息封装细节移交 10"，06 只讲分配策略与用户态视图 |
| R-4 | 初稿 exit 路径散落（07 讲 panic 又讲 exit） | P1 | 07 只覆盖诊断输出与 panic handler；exit 消息语义归 08（PM 组） |
| R-5 | libsys 覆盖面未明确边界，`sys_*.c`/`sef*.c`/`ds.c` 等是否入 scope 无声明 | P1 | §5.1 libsys 分表 + §5.3 排除表建立（A-10 边界决策） |
| R-6 | TLS 仅存在于 draft README 一句话，未入 ARCH 清单 | P1 | §4 A-4 建立（x86-64 FS 段 + 架构 trait） |
| R-7 | `ipc_minix_kerninfo.S`（MINIX_KERNINFO=6 系统调用）归属未定位 | P1 | 划入 04（IPC 陷阱 ABI），03 引用 |
| R-8 | `_kernel_call` 的 ENOTREADY 重试语义（kernel_call.c）未覆盖 | P2 | §5.2 05 清单补入 |
| R-9 | `_loadname`（M_PATH_STRING_MAX=40 内联/指针双路径）未覆盖 | P2 | §5.2 05 清单补入 |
| R-10 | srv_fork/srv_kill（libsys，PM_SRV_FORK/KILL）未定位 | P2 | 划入 08 |
| R-11 | `getvfsstat`/`fstatfs`/`statvfs` 等 VFS 统计面未入 09 清单 | P2 | §5.1 09 分组补入 |
| R-12 | `priority.c`（getpriority/setpriority）与 `clock.c`（times）归属 PM 未定位 | P2 | 补入 08 |
| R-13 | `read_tsc_64.c`、`u64.h` 归属未定位 | P2 | 补入 11 |
| R-14 | 每篇"前置依赖/职责/不覆盖"边界表缺失 | P1 | 新增 §3.3 边界表（15 篇全列） |
| R-15 | 无测试基线声明 | P2 | 新增 §3.4 |
| R-16 | 未声明每篇改写接入 review gate（outline/design 快照） | P1 | 新增 §3.5 |
| R-17 | 初稿 libc/sys 计数误写 135 文件（实为 133 .c + 2 非 .c） | P2 | §5.1 修正为 135 条目表述 |
| R-18 | `clock_getres/gettime/gettime/settimeofday` 初稿归 11 杂项，grep 实证均为 PM_CLOCK_*（PM 组）；`settimeofday` 为 clock_settime 组合 | P1 | 并入 08 PM 组，11 相应瘦身 |
| R-19 | `__sysctl.c` 实证走 `MIB_PROC_NR`（com.h:66，10-stage-mib），非"系统信息杂项" | P1 | 11 标注 MIB 归属（10-stage-mib） |
| R-20 | `getticks/getuptime/clock_time`（libsys）实证为 kerninfo 直读（无 syscall），初稿未标注 | P2 | 11 标注"01 kernel-handoff 消费面" |
| R-21 | 组合函数归属未标注（dup=fcntl F_DUPFD、fstatfs=fstatvfs、mkfifo=mknod、poll=select、pread/pwrite=lseek+rw、times=rusage、raise=kill、nanosleep=select、pathconf 纯用户态） | P1 | §5.1 逐项标注组合面 + §5.3 排除表补 pathconf |

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `lib/csu/`、`minix/lib/libc/sys/`（135 文件）、`minix/lib/libminc/`、`minix/lib/libsys/` 共享部分、`minix/include/` 相关头文件逐一 grep 核对 §5.1 映射表，并抽查 §5.2 函数级清单。

**证据**：

```bash
ls minix/lib/libc/sys/*.c | wc -l            # 135，与 §5.1 表一致
cat minix/lib/libminc/Makefile               # libminc 组成（覆盖契约基线）
rg -n '^int _syscall|^int _kernel_call' minix/lib/libc/sys/syscall.c minix/lib/libsys/kernel_call.c
rg -n '^pid_t fork|^int execve|^ssize_t read|^void \*mmap|^int brk' minix/lib/libc/sys/*.c
rg -n 'PM_EXIT|PM_FORK|VFS_READ|VFS_OPEN|VM_MMAP|VM_BRK' minix/include/minix/callnr.h minix/include/minix/com.h
rg -n 'MINIX_KERNINFO|SENDREC|IPCVEC_INTR|KERVEC_INTR' minix/include/minix/ipcconst.h minix/include/arch/i386/include/ipcconst.h
rg -n 'struct minix_kerninfo|KERNINFO_MAGIC|MINIX_KIF' minix/include/minix/type.h
rg -n 'kputc|sys_diagctl' minix/lib/libsys/kputc.c minix/lib/libsys/sys_diagctl.c
rg -n '_syscall' minix/lib/libc/sys/clock_gettime.c minix/lib/libc/sys/clock_settime.c minix/lib/libc/sys/gettimeofday.c minix/lib/libc/sys/__sysctl.c minix/lib/libc/sys/svrctl.c   # PM/MIB 归属实证
rg -n 'fcntl\(fd, F_DUPFD|mknod\(name, mode \| S_IFIFO|fstatvfs\(fd' minix/lib/libc/sys/dup.c minix/lib/libc/sys/mkfifo.c minix/lib/libc/sys/fstatfs.c   # 组合函数实证
rg -n 'get_minix_kerninfo' minix/lib/libsys/getticks.c minix/lib/libsys/getuptime.c minix/lib/libsys/clock_time.c   # kerninfo 直读实证
```

**结论**：libc/sys 133 个 .c + MISSING_SYSCALLS/Makefile.inc 全部映射（分组/排除/边界），libminc 组成全部定位，libsys 共享部分（kernel_call/kputc/sys_diagctl/asynsend/getticks/getuptime/clock_time/getsysinfo/getepinfo/getprocnr/srv_fork/srv_kill/vm_*）全部进入覆盖契约，csu 与 kerninfo ABI 全部定位；ARCH 项（A-1~A-10）与 minix3 现状对照成立。**覆盖完整性通过**。

### 7.3 诊断输出通道设计决策（2026-08-16，待 07 写作前定稿）

**候选**：A) 直接 `write(STDERR)`（VFS 依赖）；B) 保留内核诊断通道（`sys_diagctl` 对应物）；C) 先内核通道后 VFS（draft README 声明的演进顺序）。

**现状约束**：minix-rs kernel stage 的 syscall 文档（01-stage-kernel）尚未定稿诊断 ABI；panic 必须不依赖 VFS（VFS 本身也可能 panic）。**倾向 C**：panic 路径用内核诊断通道（无依赖），正常日志走 VFS。写作 07 时按 design 快照定稿，三处一致标注。

---

## 8. 参见

- `draft/` — 原占位 README（素材）
- `../02-stage-vm/plan.md` — plan 结构参照（§1-§8 骨架）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则/过渡章节）
- `../00-master-plan/README.md` — 目录重排与新主线说明
- `minix3/lib/csu/`、`minix3/minix/lib/libc/`、`minix3/minix/lib/libminc/`、`minix3/minix/lib/libsys/` — C 源码（ground truth）
- `os/libs/minix-rt/`、`os/libs/minix-sys/`、`os/libs/minix-types/` — Rust 实现
