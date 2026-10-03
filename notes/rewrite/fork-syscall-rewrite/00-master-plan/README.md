# 总规划说明

## 文档主线变更

本目录（`fork-syscall-rewrite/`）的文档主线已调整：

- **旧主线**：以 `fork` 系统调用为主线，按 fork 在各服务中的执行路径分阶段（PM→VM→Kernel→VFS→SCHED）。
- **新主线**：以**服务的启动执行顺序**为主线。`fork` 系统调用降为次主线，在相关服务章节内部展开。

### 变更原因

旧主线以 fork 为线索，实践发现：fork 路径会牵涉大量尚未实现的服务逻辑，导致文档顺序被迫频繁跳跃、补述未实现内容，打乱阅读流。改用服务启动执行顺序为主线后，文档流与系统实际启动因果链一致，fork 作为次主线在 PM/VFS 等章节内自然展开即可。

## 新目录结构（按启动执行顺序）

| 编号 | 目录 | 服务 | 启动顺序说明 |
|------|------|------|-------------|
| 00 | `00-master-plan/` | — | 顶层规划（本目录） |
| 01 | `01-stage-kernel/` | Kernel | boot 期最先运行，含 boot-shim、kmain、proc init、IPC、syscall 等 |
| 02 | `02-stage-vm/` | VM | 执行顺序上最先运行的用户服务（ptproc，`kernel/main.c:196` 立即可调度；它自己是 boot 期唯一由内核解析装载的 ELF，`kernel/arch/i386/protect.c:402-445`）；反过来给其余 boot 成员建页表并解析装载它们的 ELF（`minix3/minix/servers/vm/main.c:497-512` 遍历 + `:331-417` `exec_bootproc`） |
| 03 | `03-stage-rs/` | RS | root system process（`minix3/minix/include/minix/com.h:77` `ROOT_SYS_PROC_NR = RS_PROC_NR`，`minix3/minix/kernel/proc.h:279` `isrootsysn`），VM 之后第二个运行。职责分两面：对 boot 成员只**授权 + 放行**（一次也不读它们的镜像）；对非 boot 服务才**从磁盘读 ELF 装载** |
| 04 | `04-stage-pm/` | PM | boot_image 成员（`kernel/table.c:55`）：镜像由 **VM** 装载、由 **RS** 授权放行；进程管理，fork 次主线核心 |
| 05 | `05-stage-vfs/` | VFS | boot_image 成员（`kernel/table.c:57`）：镜像由 **VM** 装载、由 **RS** 授权放行；虚拟文件系统，fork 次主线涉及 fd 复制 |
| 06 | `06-stage-sched/` | SCHED | boot_image 成员（`kernel/table.c:56`）：镜像由 **VM** 装载、由 **RS** 授权放行；调度参数继承 |
| 07 | `07-stage-ds/` | DS | Data Store，系统服务注册与查询（boot_image 登记顺序第一，`table.c:52`） |
| 08 | `08-stage-is/` | IS | Information Server（不在 boot_image，RS 运行时加载） |
| 09 | `09-stage-init/` | INIT | 用户态 init，启动登录/用户进程（boot_image 最后一项，`table.c:64`） |
| 10 | `10-stage-mib/` | MIB | 系统信息库（**boot_image 直接登记**，`table.c:60`；2026-08-14 补建） |
| 11 | `11-stage-devman/` | DEVMAN | 设备管理（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 12 | `12-stage-input/` | INPUT | 输入服务器（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 13 | `13-stage-ipc/` | IPC | 用户态 IPC 服务（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 14 | `14-stage-runtime/` | RUNTIME | 用户态运行时/标准库（minix-rt + minix-sys 实装；2026-08-16 新建占位） |
| 15 | `15-stage-fs/` | FS | 文件系统服务器 mfs/pfs/procfs/ptyfs/ext2/isofs/vbfs/hgfs + 框架库 libfsdriver/libminixfs/libvtreefs/libsffs（2026-08-16 plan.md 定稿 + 26 篇最小骨架，占位 README 移入 draft/） |
| 16 | `16-stage-drivers/` | DRIVERS | 57 个驱动占位，tty/memory/log 为 boot 关键路径（2026-08-16 新建占位） |
| 17 | `17-stage-net/` | NET | lwip + uds 双 server + libsockdriver/libsockevent/liblwip 框架 + libc socket 封装（2026-08-16 plan.md 定稿 + 26 篇最小骨架，占位 README 移入 draft/） |
| 18 | `18-stage-commands/` | COMMANDS | games 补齐 + bin/sbin + /etc + 登录链路（2026-08-16 新建占位） |
| 19 | `19-stage-integration/` | — | 跨服务集成、状态机、端到端测试（原 10，2026-08-14 后移；2026-08-16 顺延） |
| 20 | `20-redesign/` | — | 设计重构记录（原 11，2026-08-14 后移；2026-08-16 顺延） |

> **boot 两层语义**：登记顺序（`kernel/table.c:44-64` boot_image 数组）= 模块槽位顺序（ds→rs→pm→sched→vfs→memory→tty→mib→vm→pfs→mfs→init）；执行顺序（`kernel/main.c:196` 仅 kernel 任务 + RS + VM 立即可调度；`:265-267` 非 VM 挂 RTS_VMINHIBIT）= kernel 任务 → VM → RS → 其余。目录编号采用执行语义 + 阅读理解顺序。
>
> **还有第三个维度：谁把它装载进内存。** 登记顺序与执行顺序都不回答"PM 的可执行文件是谁搬进来的"，而这个问题在项目文档里被反复答错（答成 RS）。答案见下方"启动顺序因果链"与其后的四层归因表——**那张表是全项目唯一口径源**，任何 stage 文档谈"谁加载了谁"都指向它，不要各写一份。

### 启动顺序因果链

```
Kernel (boot)
    │  ① 12 个用户模块的字节已由引导加载器（GRUB）当 multiboot module 搬进物理
    │     内存，内核只记下位置（kernel/main.c:179-184 填 start_addr/len）
    │  ② 为它们在进程表里预置固定槽位与 endpoint（kernel/table.c:44-64 顺序）
    │  ③ 只解析装载 **VM 自己** 的 ELF（arch_boot_proc 的
    │     `if(rp->p_nr == VM_PROC_NR)` 分支，protect.c:402-445）
    │  ④ 其余全员挂抑制位：非 kernel 任务/RS/VM 者 RTS_NO_PRIV|RTS_NO_QUANTUM
    │     （main.c:253），非 VM 者再挂 RTS_VMINHIBIT|RTS_BOOTINHIBIT（:264-267）
    ▼
VM (ptproc)   ← 由内核亲自装载，所以它一睁眼就有页表能力
    │  反过来遍历 boot 表，为其余 11 个成员建页表 + 解析装载 ELF + sys_exec 置
    │  入口（servers/vm/main.c:497-512 循环 + :331-417 exec_bootproc；C 注释原话
    │  "Any other boot process is already in memory and is set up here"），装载完
    │  逐个 sys_vmctl(VMCTL_BOOTINHIBIT_CLEAR)（:415）解除 BOOTINHIBIT
    ▼
RS (root sysproc)  ← 也是 boot 成员，镜像同样由 VM 装载，不是自己人载自己
    │  对 boot 成员：只发特权 + 给调度参数 + 放行，不读任何镜像
    │  对非 boot 服务：才亲自读盘装载（stat/open/read → 造槽 → srv_execve）
    ├─► PM / SCHED / VFS / DS / TTY / memory / MIB / PFS / MFS
    │     （boot_image 成员：GRUB 送字节 → 内核给槽位 → VM 解析装载 → RS 放行）
    ├─► IS / DEVMAN / INPUT / IPC / lwip / uds / procfs / ptyfs / log ...
    │     （非 boot 成员：rc 脚本发 `up` → RS 读盘装载全套；endpoint 动态分配）
    └─► INIT（boot_image 最后一项 kernel/table.c:64，根用户进程）
```

**boot 链四层归因表（全项目唯一口径源，2026-10-03 依 C 真源复核定稿）**

中文"加载"是个含糊词，至少混着下面四件互不相干的事。谈"谁加载了 PM"必须先问清是哪一层。

| 层 | 具体在干什么 | boot 成员（DS/RS/PM/SCHED/VFS/memory/TTY/MIB/VM/PFS/MFS/INIT） | 非 boot 服务（IS/DEVMAN/INPUT/IPC/lwip/uds/procfs/ptyfs/log/…） |
|---|---|---|---|
| ① 字节进内存 | 把可执行文件读进物理内存 | **引导加载器**（GRUB 按 multiboot module 搬入）；内核只登记位置 `kernel/main.c:179-184` | **RS**：`stat`/`open`/`read` 经 VFS 读磁盘（`servers/rs/manager.c:1372-1419`） |
| ② 槽位 + endpoint | 在进程表里给它一个身份证号 | **内核**：按 `kernel/table.c:44-64` 预置固定 `proc_nr`；boot 成员世代号 gen=0，故 endpoint 就等于 proc 号（服务器间就靠这个号寻址） | **RS**：造槽（`clone_slot`/fork），endpoint 由内核现分配（所以不存 `IS_PROC_NR` 之类的固定号） |
| ③ 解析 ELF + 建地址空间 | 读 ELF 头与程序头表、映射 PT_LOAD 段、置入口地址 | **VM**：`servers/vm/main.c:331-417` `exec_bootproc` → `libexec_load_elf`（:383）+ `sys_exec`（:408）。**唯一例外**：VM 自己由内核在 boot 期装载（protect.c:402-445） | **RS 解析、VM 建映射、PM 收尾**：`servers/rs/exec.c:14-18` 挂 `libexec_load_elf`（`do_exec` 在 `:89-93` 循环调用它），`:85-87` `libexec_alloc_mmap_*` 请求 VM 建映射，`:101-102` `libexec_pm_newexec`（注释原话 "Inform PM"）+ `:115` `exec_restart` 走 PM 收尾 |
| ④ 授权 + 准许被调度 | 清 `RTS_NO_PRIV`、发时间片 | **RS**：`servers/rs/main.c:287`（`SYS_PRIV_SET_SYS`，RS/VM 跳过）、`:376`（`sched_init_proc`）、`:379`（`SYS_PRIV_ALLOW`） | **RS**：同一套，紧接在 ①②③ 之后 |

**为什么两条路必须不同（一句话机制）**：读磁盘要 VFS、VFS 要存储驱动、驱动的出生又要 RS——所以"链条起点必须有一批进程在文件系统可用之前就已经在内存里"，这一批就是 boot_image，它们的字节只能由链条之外的引导加载器送进来。

**为什么 ③ 里 RS 那条路多一个 PM、VM 那条路没有**：VM 装载 boot 成员时 PM 自己也还没出生，无从通知；等 RS 装载非 boot 服务时 PM 早已运行，新进程的 pid 与凭证必须归它记账（`servers/rs/exec.c:101-102` 注释 "Inform PM"）。同一个"装载"动作在不同启动阶段参与者不同，正是依赖关系在时刻上的投影。

> 依据（2026-10-03 依 C 真源逐锚复核）：
> `minix3/minix/servers/vm/main.c:497-512` + `:331-417`——除 VM 自身外的全部 boot 成员由 **VM** 解析装载；`:505-507` 注释原文即"其余 boot 进程已在内存中，在这里被建立"。
> `minix3/minix/servers/rs/main.c:339`——boot 成员槽位的 `rp->r_exec` 初值为 `NULL`（RS 手里没有它们的镜像副本）；`:285-291/376/379`——RS 对它们做的只有授权、调度参数、放行。
> `minix3/minix/servers/rs/table.c:15-29`——RS 启动期逐个授权放行的名单恰好是 12 个 boot 成员，不含 is/devman/input/ipc/lwip。
> **本次更正的错误口径**：(a) 原文"其余进程跳过，由 RS 运行时加载"把 VM 的装载工作记到了 RS 名下；(b) 原文"`main.c:265-267` 解除抑制"方向反了，那三行是**设置** VMINHIBIT/BOOTINHIBIT；解除另有其人——VMINHIBIT 在 `minix3/minix/kernel/system/do_vmctl.c:137-143`（显式 CLEAR）或在 `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:32`（boot 路径实际走这条：VM 发 SETADDRSPACE 时顺手清），BOOTINHIBIT 在 `do_vmctl.c:166-168`，两者都由 VM 发 `sys_vmctl` 触发；(c) 因果链第一分支原只列 PM/SCHED/VFS/DS/MIB 五项，实际 boot_image 用户成员有 12 项，漏项易被读成"只有这五个走这条路"。

## 旧→新目录映射

| 旧目录 | 新目录 |
|--------|--------|
| `01-stage-pm/` | `04-stage-pm/` |
| `02-stage-vm/` | `02-stage-vm/`（不变） |
| `03-stage-kernel/` | `01-stage-kernel/` |
| `04-stage-vfs/` | `05-stage-vfs/` |
| `05-stage-sched/` | `06-stage-sched/` |
| `06-stage-integration/` | `19-stage-integration/`（2026-08-14 后移，2026-08-16 顺延） |
| `07-redesign/` | `20-redesign/`（2026-08-14 后移，2026-08-16 顺延） |
| —（新建） | `03-stage-rs/` |
| —（新建） | `07-stage-ds/` |
| —（新建） | `08-stage-is/` |
| —（新建） | `09-stage-init/` |
| —（新建） | `10-stage-mib/`（2026-08-14 补建，boot_image 直接登记） |
| —（新建） | `11-stage-devman/`（2026-08-14 补建，RS 加载） |
| —（新建） | `12-stage-input/`（2026-08-14 补建，RS 加载） |
| —（新建） | `13-stage-ipc/`（2026-08-14 补建，RS 加载） |

## 本目录现有文档状态（defer）

> **注意**：本目录（`00-master-plan/`）下 `01-*.md` ~ `15-*.md` 等文档均基于**旧主线**（fork 为主线）编写，内容已过时：
> - 阶段编号与命名基于旧的 fork 分阶段方案（如 `phase1-pm`、`phase2-vm`、`phase3-kernel`）；
> - 路线图、里程碑、依赖关系图均以 fork 执行路径组织，与新主线（服务启动顺序）不一致。
>
> 这些文档**暂不修改**（defer）。新主线规划以上文本节为准。待新主线文档稳定后，再决定是增量更新还是整体重写本目录文档。

## 内部引用说明

目录重命名后，文档间的相对路径引用（如 `../03-stage-kernel/...`）会失效。这些引用暂不修复，后续通过 `grep` 批量定位并修正。修正时可参照上表"旧→新目录映射"。

## 顶层 README.md 状态（defer）

> **注意**：上级目录 [`fork-syscall-rewrite/README.md`](../README.md) 同样基于旧主线编写，列出的目录结构（`01-stage-pm`、`03-stage-kernel` 等）、阶段映射、项目进度表均已过时，**暂不修改**（defer）。新主线目录结构以本文档"新目录结构"章节为准。
