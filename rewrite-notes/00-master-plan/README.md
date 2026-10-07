# 总规划说明

> **状态（本目录的定位）**：本 README 本身仍在维护（最近一次内容修正是 `32538a6ca` 的启动顺序语义纠错），
> 下面的目录表与启动因果链可作依据。但**本目录其余 15 个文件是规划期档案**：它们的最后一次内容维护
> 停在阶段目录重排那次提交 `e1d2c4977`，此后文档主线已换、阶段编号已改，文中仍写着「PM 优先的五阶段」
> 与重排前的目录名。实测本目录内有 **21 条链接指向不存在的目标**（如 `../01-stage-pm/…`、
> `../deep-analysis/…`、`./06-phase1-pm-guide.md`——实际文件叫 `05-phase1-pm-guide.md`）。
>
> 因此后续会话与工具**不要把本目录当成现状来源**。各文件的可用范围见下表。
>
> 查现状该去哪里：某个服务的实现语义 → 同级的 `NN-stage-*` 阶段目录；待拍板事项 →
> `../coordination/PENDING-DECISIONS-3ARCH-PARITY.md` 与三架构对齐台账；已定事实 → `os/` 代码与
> `minix3/` 下的 C 源（真源优先次序见项目根的规范文档）。

## 逐文件可用性

| 文件 | 现在还能用来做什么 | 不能用来做什么 |
|---|---|---|
| 本 `README.md` | 阶段目录与启动顺序的对应关系、启动因果链 | 不描述实现进度 |
| `01-project-overview.md`、`02-architecture-analysis.md`、`04-implementation-roadmap.md` | 了解当初为什么这样切分、纵向切片策略的取舍 | 项目范围与推进顺序都已超出这些描述（现已覆盖内核与全部用户态服务、三架构与真机门禁） |
| `05-phase1-pm-guide.md` … `10-phase5-sched-guide.md`（6 篇） | 只能当「按 fork 主线分五阶段」这一被放弃的组织方式的史料 | 篇号与服务对应关系已失效；服务语义请去对应阶段目录查 |
| `03-minix3-source-audit.md`、`13-source-mapping.md` | C 源码盘点的线索 | 其中引用的路径与篇号有失效项，逐条要复核 |
| `11-decision-records.md` | 决策史（当时的取舍与否决理由） | 不等于当前决策；已定案事项以 `../coordination/` 下的裁决清单与台账为准 |
| `12-constants-reference.md` | 常量对照的线索 | 数值以 `os/` 代码与 `minix3/` 头文件为准 |
| `14-verification-checklist.md`、`15-todo-fixes.md` | 知道当时打算怎么验收、有哪些待办 | **待办状态从未与现状对账**，不可当未完成项清单使用 |

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
| 02 | `02-stage-vm/` | VM | 执行顺序上最先运行的用户服务（ptproc，`minix3/minix/kernel/main.c:kmain` 的 `schedulable_proc` 判定让它立即可调度；它自己是 boot 期唯一由内核解析装载的 ELF，`minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc`）；反过来给其余 boot 成员建页表并解析装载它们的 ELF（`minix3/minix/servers/vm/main.c:init_vm` 遍历 + `exec_bootproc` 装载） |
| 03 | `03-stage-rs/` | RS | root system process（`minix3/minix/include/minix/com.h:ROOT_SYS_PROC_NR` 就等 `RS_PROC_NR`，`minix3/minix/kernel/proc.h:isrootsysn` 靠它判定），VM 之后第二个运行。职责分两面：对 boot 成员只**授权 + 放行**（一次也不读它们的镜像）；对非 boot 服务才**从磁盘读 ELF 装载** |
| 04 | `04-stage-pm/` | PM | boot_image 成员（`minix3/minix/kernel/table.c:image` 的 PM 条目）：镜像由 **VM** 装载、由 **RS** 授权放行；进程管理，fork 次主线核心 |
| 05 | `05-stage-vfs/` | VFS | boot_image 成员（`table.c:image` 的 VFS 条目）：镜像由 **VM** 装载、由 **RS** 授权放行；虚拟文件系统，fork 次主线涉及 fd 复制 |
| 06 | `06-stage-sched/` | SCHED | boot_image 成员（`table.c:image` 的 SCHED 条目）：镜像由 **VM** 装载、由 **RS** 授权放行；调度参数继承 |
| 07 | `07-stage-ds/` | DS | Data Store，系统服务注册与查询（boot_image 登记顺序第一，`table.c:image` 的 DS 条目） |
| 08 | `08-stage-is/` | IS | Information Server（不在 boot_image，RS 运行时加载） |
| 09 | `09-stage-init/` | INIT | 用户态 init，启动登录/用户进程（boot_image 最后一项，`table.c:image` 的 INIT 条目） |
| 10 | `10-stage-mib/` | MIB | 系统信息库（**boot_image 直接登记**，`table.c:image` 的 MIB 条目） |
| 11 | `11-stage-devman/` | DEVMAN | 设备管理（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 12 | `12-stage-input/` | INPUT | 输入服务器（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 13 | `13-stage-ipc/` | IPC | 用户态 IPC 服务（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 14 | `14-stage-runtime/` | RUNTIME | 用户态运行时/标准库（minix-rt + minix-sys 实装；2026-08-16 新建占位） |
| 15 | `15-stage-fs/` | FS | 文件系统服务器 mfs/pfs/procfs/ptyfs/ext2/isofs/vbfs/hgfs + 框架库 libfsdriver/libminixfs/libvtreefs/libsffs（2026-08-16 plan.md 定稿 + 26 篇最小骨架，占位 README 移入 draft/） |
| 16 | `16-stage-drivers/` | DRIVERS | 57 个驱动占位，tty/memory/log 为 boot 关键路径（2026-08-16 新建占位） |
| 17 | `17-stage-net/` | NET | lwip + uds 双 server + libsockdriver/libsockevent/liblwip 框架 + libc socket 封装（2026-08-16 plan.md 定稿 + 26 篇最小骨架，占位 README 移入 draft/） |
| 18 | `18-stage-commands/` | COMMANDS | games 补齐 + bin/sbin + /etc + 登录链路（2026-08-16 新建占位） |
| 19 | `19-stage-integration/` | — | 跨服务集成、状态机、端到端测试（原 10，2026-08-14 后移；2026-08-16 顺延） |
| 20 | — | — | 设计重构记录两篇（`rs-cross-layer-pollution.md`、`tocutou-and-distributed-consistency.md`）已随本次目录迁移移到仓库根的 `redesign-notes/architecture/`，本树不再占用 20 号编号 |

> **boot 两层语义**：登记顺序（`minix3/minix/kernel/table.c:image` 数组）= 模块槽位顺序（ds→rs→pm→sched→vfs→memory→tty→mib→vm→pfs→mfs→init）；执行顺序（`minix3/minix/kernel/main.c:kmain` 仅 kernel 任务 + RS + VM 立即可调度，非 VM 者挂上 RTS_VMINHIBIT）= kernel 任务 → VM → RS → 其余。目录编号采用执行语义 + 阅读理解顺序。
>
> **还有第三个维度：谁把它的可执行映像装进内存。** 上面两个顺序都不回答"PM 的可执行文件是谁搬进来的"，而这个问题很容易答成 RS。下面那张四层归因表就是用来回答它的：先把"加载"拆成四层，再谈谁做了哪一层。

### 启动顺序因果链

```
Kernel (boot)
    │  ① 12 个用户模块的字节已由引导加载器（GRUB）当 multiboot module 搬进物理
    │     内存，内核只记下位置（kernel/main.c:kmain 填 start_addr/len）
    │  ② 为它们在进程表里预置固定槽位与 endpoint（kernel/table.c:image 数组）
    │  ③ 只解析装载 **VM 自己** 的 ELF（arch_boot_proc 的
    │     if(rp->p_nr == VM_PROC_NR) 分支）
    │  ④ 其余全员挂抑制位：非 kernel 任务/RS/VM 者 RTS_NO_PRIV 与 RTS_NO_QUANTUM，
    │     非 VM 者再挂 RTS_VMINHIBIT 与 RTS_BOOTINHIBIT（均在 kernel/main.c:kmain）
    ▼
VM (ptproc)   ← 由内核亲自装载，所以它一睁眼就有页表能力
    │  反过来遍历 boot 表（servers/vm/main.c:init_vm），为其余 11 个成员建页表 +
    │  解析装载 ELF + sys_exec 置入口（exec_bootproc）；C 注释原话
    │  "Any other boot process is already in memory and is set up here"；装载完
    │  逐个发 sys_vmctl(VMCTL_BOOTINHIBIT_CLEAR) 解除 BOOTINHIBIT
    ▼
RS (root sysproc)  ← 也是 boot 成员，镜像同样由 VM 装载，不是自己人载自己
    │  对 boot 成员：只发特权 + 给调度参数 + 放行，不读任何镜像
    │  对非 boot 服务：才亲自读盘装载（stat/open/read → 造槽 → srv_execve）
    ├─► PM / SCHED / VFS / DS / TTY / memory / MIB / PFS / MFS
    │     （boot_image 成员：GRUB 送字节 → 内核给槽位 → VM 解析装载 → RS 放行）
    ├─► IS / DEVMAN / INPUT / IPC / lwip / uds / procfs / ptyfs / log ...
    │     （非 boot 成员：rc 脚本发 up → RS 读盘装载全套；endpoint 动态分配）
    └─► INIT（boot_image 最后一项，table.c:image 的 INIT 条目；根用户进程）
```

**boot 链四层归因表**

中文"加载"是个含糊词，至少混着下面四件互不相干的事。谈"谁加载了 PM"必须先问清是哪一层。

| 层 | 具体在干什么 | boot 成员（DS/RS/PM/SCHED/VFS/memory/TTY/MIB/VM/PFS/MFS/INIT） | 非 boot 服务（IS/DEVMAN/INPUT/IPC/lwip/uds/procfs/ptyfs/log/…） |
|---|---|---|---|
| ① 字节进内存 | 把可执行文件读进物理内存 | **引导加载器**：GRUB 按 multiboot module 搬入，内核 `minix3/minix/kernel/main.c:kmain` 只把模块位置抄进 `boot_image.start_addr/len` | **RS**：`minix3/minix/servers/rs/manager.c:read_exec` 的 `stat`/`open`/`read` 经 VFS 读磁盘 |
| ② 槽位 + endpoint | 在进程表里给它一个身份证号 | **内核**：`minix3/minix/kernel/table.c:image` 预置固定 `proc_nr`；boot 成员世代号 gen=0，故 endpoint 就等于 proc 号（服务器间就靠这个号寻址） | **RS**：造槽（`minix3/minix/servers/rs/manager.c:clone_slot` 与 fork 路径），endpoint 由内核现场分配，所以不存在 `IS_PROC_NR` 这种固定号 |
| ③ 解析 ELF + 建地址空间 | 读 ELF 头与程序头表、映射段、置入口地址 | **VM**：`minix3/minix/servers/vm/main.c:exec_bootproc`（`sys_physcopy` 读头 → `libexec_load_elf` 解析 → `sys_exec` 置入口）。**唯一例外**：VM 自己由内核装载，见 `minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc` 里 `if(rp->p_nr == VM_PROC_NR)` 那条分支 | **RS 解析、VM 建映射、PM 收尾**：`minix3/minix/servers/rs/exec.c:exec_loaders` 挂着 `libexec_load_elf`，`do_exec` 把段映射请求交给 `libexec_alloc_mmap_*`（最终落到 VM），再 `libexec_pm_newexec` + `exec_restart` 经 PM 收尾 |
| ④ 授权 + 准许被调度 | 清 `RTS_NO_PRIV`、发时间片 | **RS**：`minix3/minix/servers/rs/main.c:sef_cb_init_fresh` 的 Step 1 发 `sys_privctl(SYS_PRIV_SET_SYS)`，Step 2 发 `sched_init_proc` + `sys_privctl(SYS_PRIV_ALLOW)` | **RS**：同一套，紧接在 ①②③ 之后 |

**机制锚点**

- `minix3/minix/servers/vm/main.c:init_vm` 遍历 `kernel_boot_info.boot_procs[]`，对每个非负 `proc_nr`（VM 自己除外）建页表并调 `exec_bootproc`；源码注释原话："Any other boot process is already in memory and is set up here"。
- 抑制位的两道解除腿：BOOTINHIBIT 在 `minix3/minix/kernel/system/do_vmctl.c:do_vmctl`，VMINHIBIT 在 `minix3/minix/kernel/arch/i386/arch_do_vmctl.c:arch_do_vmctl`（随 SETADDRSPACE 一并清）；两者都由 VM 发 `sys_vmctl` 触发。内核 boot 循环里对 `RTS_VMINHIBIT`/`RTS_BOOTINHIBIT` 的赋值是**设置**而不是解除，引用时不要弄反方向。
- `minix3/minix/servers/rs/main.c:sef_cb_init_fresh` 在同一个循环里把 `rp->r_exec = NULL`——RS 手里没有 boot 成员的镜像副本，它对这些服务只执行第 ④ 层。
- `minix3/minix/servers/rs/table.c:boot_image_priv_table` 的表头注释自陈顺序含义："the order boot system services are made runnable and initialized at boot time"；表内 12 项恰好是 boot 成员，不含 is/devman/input/ipc/lwip。
- Rust 侧同形态：内核 boot 期只装载 VM（`os/arch/src/arch/boot.rs:load_vm_elf`），其余 boot 成员由 VM 装载（`os/servers/vm/src/vm_server.rs:init_boot_procs` + `exec_bootproc`），RS 的放行顺序照 C 那张表对位（`os/servers/rs/src/table.rs:BOOT_IMAGE_PRIV_TABLE`）。

**为什么两条路必须不同**：读磁盘要 VFS、VFS 要存储驱动、驱动的出生又要 RS——链条起点必须有一批进程在文件系统可用之前就已经在内存里，这一批就是 boot_image，它们的字节只能由链条之外的引导加载器送进来。

**为什么 ③ 里 RS 那条路多一个 PM、VM 那条路没有**：VM 装载 boot 成员时 PM 自己也还没出生，无从通知；等 RS 装载非 boot 服务时 PM 早已运行，新进程的 pid 与凭证必须归它记账（`minix3/minix/servers/rs/exec.c:do_exec` 里的 `libexec_pm_newexec`，源码注释 "Inform PM"）。同一个"装载"动作在不同启动阶段参与者不同，正是依赖关系在时刻上的投影。

### boot 顺序的六张不同次序

①②③④ 说的是"一件事由谁做"；而一句"X 是 boot 第 N 个"里的序，则可能是下面六张表中的任一张。读到这种说法，先问是哪张表：

| 次序 | 由谁决定 | 内容（只列用户服务） | 代码是否强制 |
|---|---|---|---|
| 模块摆放序 | `minix3/minix/kernel/table.c:image` 数组顺序 = GRUB 交来的模块顺序 | ds rs pm sched vfs memory tty mib vm pfs mfs init | **强制**。`minix3/minix/kernel/main.c:kmain` 先断言模块数量相符，再按 `i - NR_TASKS` 到 `kinfo.module_list[]` 取字节——摆错位置就是张冠李戴 |
| 槽位填充序 | 同一次 boot 循环的遍历顺序 | 同上 | 只决定"谁先在进程表里就位"；全员随后被 `RTS_SET(rp, RTS_PROC_STOP)` 按停，不影响谁先跑 |
| 可调度序 | `minix3/minix/kernel/main.c:kmain` 的 `schedulable_proc` 判定（内核任务、`isrootsysn`、VM 三类） | 内核任务 + RS + VM 立即拿到特权与时间片；**第一个能跑的用户服务是 VM** | 强制。其余挂 `RTS_NO_PRIV` 与 `RTS_NO_QUANTUM` |
| 装载序 | `minix3/minix/servers/vm/main.c:init_vm` 的遍历顺序（沿用模块摆放序） | **ds 第一个被铺好、rs 第二个**，跳过内核任务与 VM 自己 | 强制（遍历即数组顺序）。Rust 同：`os/servers/vm/src/vm_server.rs:init_boot_procs` |
| 授权放行序 | `minix3/minix/servers/rs/table.c:boot_image_priv_table` 的表内顺序 | rs 与 vm 已在跑被跳过 → pm sched vfs **ds** tty memory mib pfs mfs init；**ds 第四** | 强制。Rust 同：`os/servers/rs/src/table.rs:BOOT_IMAGE_PRIV_TABLE` |
| NOTIFY 挑选序 | `minix3/minix/kernel/proc.c:has_pending` 从低位往高位扫 `s_notify_pending` 位图；priv id 由 `minix3/minix/include/minix/priv.h:static_priv_id`（`NR_TASKS + proc_nr`）给出 | 按 **proc 号升序**：pm(0) vfs(1) rs(2) memory(3) sched(4) tty(5) **ds(6)** mib(7) vm(8) pfs(9) mfs(10) init(11) | 强制，且**与模块摆放序无关** |

于是两个看似矛盾、其实各自都真的说法可以同时成立：**"ds 是 boot 映像里排第一的用户服务"**（模块摆放序 + 装载序）与 **"ds 是 RS 放行的第四个"**（授权放行序）。

**NOTIFY 挑选序这一张要单独提防**：`minix3/minix/kernel/table.c` 的表头注释声称"数组顺序 = NOTIFY 投递优先级，ds 必须排在最前以保证系统事件可靠异步发布"。内核里没有任何代码执行这句话：`minix3/minix/kernel/proc.c:mini_receive` 收通知时走的是上面那张按 priv id 升序的位图扫描，ds 在那里按 proc 号 6 排在第七位。Rust 侧忠实复刻位图升序（`os/kernel/src/ipc.rs:pick_allowed_notify`，注释 "Scan bits low→high (C order)"）。所以这句话只能当作者的意图声明引用，不能当机制引用：如果按它去实现"ds 优先投递"，等于给 `receive(ANY)` 引入一条原本不存在的消息偏序规则，而这条规则不会给出任何错误码，只会把现有服务的时序打乱。

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

> **注意**：上级目录 [`fork-syscall-rewrite/README.md`](../misc/legacy-fork-syscall-index.md) 同样基于旧主线编写，列出的目录结构（`01-stage-pm`、`03-stage-kernel` 等）、阶段映射、项目进度表均已过时，**暂不修改**（defer）。新主线目录结构以本文档"新目录结构"章节为准。
