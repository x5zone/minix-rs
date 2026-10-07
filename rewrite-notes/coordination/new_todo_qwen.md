# new_todo_qwen.md — 启动 minix-rs 并跑通 18-stage-commands 全部命令：待实现逻辑清单

> **产出者**：qwen（本次扫描由 qwen 独立完成）
> **日期**：2026-09-20
> **目标**：三架构（x86-64 / aarch64 / riscv64）QEMU 环境启动 minix-rs，运行 `rewrite-notes/18-stage-commands/` 覆盖的全部命令程序。
> **隔离声明**：本文与所有中间结果均带 `_qwen` 后缀，与另一 AI 的产出（同目录下已有 `new_todo_deepseek.md`、`new_todo_glm.md`，qwen 未读取、未参考，保证两路独立）互不覆盖。任何 qwen 产出的计划、报告、验证日志一律以 `_qwen` 结尾。
> **定位**：本文只做"扫描 + 登记待继续实现的逻辑"，不改生产代码。逐条实现走 `todo-fix`（一次一条）。
> **优先级口径（用户明确要求）**：**先确保有，再确保好**。P0 = 让系统能真启动、命令能真在目标机上跑起来的关键路径缺口；P1 = 把"少数命令能跑"扩到"全部命令能跑"的覆盖长尾；P2 = 第二优先级，非 rewrite 的 translate 味道与代码缺陷（项目目标始终是 rewrite 而非 translate）。

---

## 0. 现状快照（每条都给了可复核的锚点）

结论先行：**从"内核在 QEMU 里启动"到"`ls` 这类命令在目标机上跑起来"这条端到端链路，目前根本没有接起来。** 各个环节单独看发育得很好（生产 `kmain` 能进真正的调度循环、服务器有真事件循环、init 有真状态机、命令有薄壳），但它们之间缺了三样东西：一个能装出可启动镜像的打包工具、一套把命令按目标机（no_std + real-trap）编译的构建、以及让命令够用的系统调用面。

| 环节 | 现状 | 锚点 |
|------|------|------|
| QEMU 现有测试 | 只有 bootstrap 级：`hello-boot`（三架构串口）、`test-memmap`/`test-paging-enable`/`test-kernel-map`（仅 x86-64）。没有任何"启动到 shell 跑命令"的整机测试 | `os/qemu-tests/README.md:92-100` |
| 生产内核入口 | 真 `kmain`（Phase A→F）存在，末尾 `switch_to_user()` 进真调度循环；但 kernel 默认 feature 是 `mock`，会把生产 `kmain` 编译掉，只有 `boot-shim`（`default-features = false`）才构建它 | `os/kernel/src/lib.rs:409`、`:2298`、`:3114`；`os/kernel/Cargo.toml:25`；`os/boot-shim/Cargo.toml:18` |
| 可启动镜像打包 | **不存在**。`xtask` 的 `image()` 与 `qemu()` 是打印 `[skeleton]` 的空壳；`build()` 只编 库 + `minix-kernel` + `minix-pm`，不含任何服务器、命令、rootfs | `os/xtask/src/main.rs:123-142`、`:63-83` |
| boot 模块供给 | boot-shim 只喂 6 个模块 `["vm","pm","vfs","rs","ds","inet"]`，而内核要求 `NR_BOOT_MODULES = 12`（DS/RS/PM/SCHED/VFS/MEM/TTY/MIB/VM/PFS/MFS/INIT）并 `assert_eq!` 对齐；`inet` 还不在内核表里 → 一开机就 panic | `os/boot-shim/src/loader.rs:54`；`os/kernel/src/proc.rs:81`、`:106-119`；`os/kernel/src/lib.rs:1131` |
| 内核实际装载的进程 | `init_proc_and_boot` 只把 VM 的 ELF 真装载（`load_vm_elf`），其余全部 `EntrySpec::DEFERRED`（"等 RS 运行期再加载"）——而 RS 的加载能力今天没有整机验证 | `os/kernel/src/lib.rs:1372-1381`、`:1449-1455` |
| 服务器事件循环 | RS/PM/VFS/MFS/PFS 的 `main` 都有真事件循环（不是 `loop{}` 停车），但全部落在传输层：没有 `real-trap` 时对内核的每一条腿诚实返回 `EIO` | RS `os/servers/rs/src/lib.rs:260-314`；PM `os/servers/pm/src/init.rs:396-441`；VFS `os/servers/vfs/src/main_loop.rs:7079-7156`；MFS/PFS `os/fs/mfs/src/main.rs:9-45`、`os/fs/pfs/src/main.rs:21-38` |
| 命令按目标机构建 | **一个都没有**。65 个命令薄壳（`find os/commands -path '*/src/bin/*.rs'` 实测 65 个）全部宿主 `std` 构建；`real-trap` 只在 qemu-tests 的 `test-kernels` 里开，命令与服务器都不开；minix-rt 的三架构裸 `_start` 已有但命令没用起来 | `os/commands/bin/fileops/Cargo.toml:16`（`features=["std"]`）；`os/commands/bin/fileops/src/bin/echo.rs:11-15,31-34`（宿主接缝）；`os/libs/minix-rt/src/crt0.rs:335-381`（裸 `_start`）；`os/.cargo/config.toml:36-37`（`x86_64-unknown-none` 配好但无人调用） |
| init | 是真 init（runlevel 状态机、fork/exec/waitpid 都接到 `minix_sys`），但不启 `real-trap`，宿主下第一次 fork/exec 就在 kerninfo 查询处 `EIO` | `os/commands/sbin/init/src/main.rs:64-156`；`host.rs:200-210`；`os/commands/sbin/init/Cargo.toml:31-32` |
| minix-sys 顶层 API | 比 2026-09-17/18 的 todo/edge 记载**更完整**：`errno` 常量再导出、`argv/env` 访问器、`open`（含 open-existing 路径）、`stat`/`lstat`/`fstat`、`ioctl`/`fcntl`/`getdents`、`tcgetattr`/`tcsetattr` 均已落顶层。仍缺：`pipe`、顶层 `dup2`/`lseek`/`getpid`/`nanosleep` 再导出，以及 `unlink`/`rename`/`mkdir`/`chmod`/`chdir` 等一族 | `os/libs/minix-sys/src/lib.rs:229-418`；`crt0.rs:141-199`；见 §2 P0-4 |

> **文档时效提醒**：`18-stage-commands/todo.md §6`、`edge_todo.md` 中多条"缺口"（errno 再导出、argv/env 访问器、getdents/stat/ioctl/fcntl、open-existing 路径）在实测代码里已闭合。以代码为准，逐条实现前按 fix-guard 重读目标行。

---

## 1. 关键路径分解：两条轨，先打通哪条

用户目标"启动 + 跑所有命令"天然是两条轨（见 `00-commands-overview.md §1.2`）：

- **装载轨（系统能不能起来）**：boot → 打包出含 kernel+服务器+rootfs 的可启动镜像 → init 起来 → 挂载根文件系统 → 拉起 shell/命令。**这条今天完全没通**，是 P0 的主战场，且大多跨 stage（归内核 / xtask / boot-shim / RS / VFS / FS 各轨道，不属 18-stage-commands 独占）。
- **依赖轨（命令能不能跑）**：命令 → minix-rt/minix-sys → 服务器 → 内核。命令本体已写了不少（65 薄壳 + 大量决定半库），但都停在"宿主能跑、目标跑不了"这一步。

**先确保有** = 用最小代价让装载轨打通一次，让**一小批代表性命令**（比如 `echo`、`cat`、`ls`、`sh`）能在目标机 QEMU 上跑出来——哪怕其余命令还缺。有了这个"能启动能跑命令"的样板，再按 P1 把覆盖扩到全部 328 命令。

---

## 2. P0 — 关键路径：让系统启动、让命令在目标机跑起来（确保"有"）

### P0-1　可启动镜像打包工具（xtask image/qemu 从空壳变实装）

- **缺什么**：`cargo xtask image` 要把内核 ELF + 12 个 boot 模块 ELF + 一个含命令二进制的 rootfs 镜像，按 minix3 boot image 格式打进一张 ESP/FAT 盘映像；`cargo xtask qemu` 要按架构拼 QEMU 参数启动。现在两者都只打印 `[skeleton]`（`os/xtask/src/main.rs:123-142`），`build()` 也只编到 `minix-pm`（`:63-83`）。
- **要做**：实现 xtask 空壳注释里已列的四步（收集 boot image 进程 ELF，对照 `minix3/minix/kernel/table.c:36` 的 `boot_image` 数组 → 按 `minix3/releasetools/mkboot` + `distrib/common/bootimage` 格式打包 → 用 `mkfs`（Rust 侧 `sbin/diskfmt`/`mountinfo` 的 mfs 工具，见 §3）生成 rootfs → 输出到 `os/target/image/`）。三架构各自的固件/内核镜像布局（x86-64 与 aarch64 走 UEFI/OVMF，riscv64 走 OpenSBI）分别参数化，参照 `os/qemu-tests/run_qemu.sh`。
- **归属/依赖**：xtask（构建面）+ 15-stage-fs（rootfs 镜像）+ 16-stage-drivers；与 `edge_todo.md` E-FSCMDS（mkfs/fsck 命令）、E-FSRUNTIME（fs server 启动）相关。这是"启动"的**第一硬门槛**。

### P0-2　boot 模块清单对齐（消除开机 assert panic）

- **缺什么**：boot-shim 的 `MODULE_NAMES`（6 个，含内核不认的 `inet`）与内核 `NR_BOOT_MODULES = 12` 的 `assert_eq!` 直接冲突，一开机即 panic。
- **要做**：二选一并三处一致——(a) 让 boot-shim 供齐内核要求的 12 个模块；或 (b) 若现阶段只想先跑通最小集，同步下调内核的模块数断言与 `proc_table` 构建，把未就绪的模块显式标 DEFERRED 且不计入断言。倾向 (a)（贴近 C 的 boot_image 表），但需与 P0-1 的镜像内容、P0-3 的 RS 运行期加载裁决一致。
- **归属**：boot-shim + 01-stage-kernel（`proc.rs`/`lib.rs` 断言）。裁决类，走 `[ARCH]` 三处一致。
- **锚点**：`os/boot-shim/src/loader.rs:38-54`（注释自己描述的 12 项 C 顺序与实际 6 项列表矛盾，属 doc-code 漂移，见 §4）；`os/kernel/src/proc.rs:81,106-119`；`os/kernel/src/lib.rs:1131`。

### P0-3　命令与服务器按目标机构建（no_std + real-trap 的装配面落地）

- **缺什么**：所有命令薄壳是宿主 `std` 构建，`real-trap` 只有 qemu-tests 的手工测试载体开（grep 实测）。没有任何构建/镜像流程把 `os/commands/**/src/bin/*.rs` 编成目标机 ELF。minix-rt 有真 `_start`（`crt0.rs:335-381`），但命令没走它。
- **要做**：
  1. 建一条"目标命令"构建配置（`x86_64-unknown-none` 已在 `os/.cargo/config.toml:36-37` 配好；aarch64/riscv64 同批），命令 crate 以 `no_std` + `real-trap` feature 编译，链接 minix-rt 的 `_start`。
  2. 把 echo 模板里预留的两个宿主接缝（`terminate()`→`minix_sys::exit`；`std::env::args`→`minix_rt::crt0::argv_*`）在目标构建一刀切换（`echo.rs:11-15,31-34`、`bin_support.rs:5-8`）。这一步 init 已趟过样板（`init/src/main.rs:97-100` 走 birth 链 argv）。
  3. 与 RS 运行期加载（内核把非 VM 模块 DEFERRED 给 RS，`lib.rs:1449-1455`）打通：命令 ELF 要能被 RS 装进 VFS 提供的镜像并被 fork+exec 起来。
- **归属**：18-stage-commands（命令侧接缝）+ 14-stage-runtime（minix-rt no_std 姿态）+ xtask（P0-1）+ E1 real-trap 门控。
- **判据**：一个真命令（建议先 `echo`）在 QEMU 目标机上被执行、串口看到它的输出、退出码正确。

### P0-4　补齐目标机跑通冒烟命令集所需的系统调用面

分两档（便宜的先做）：

- **只差顶层再导出（`*_via` 已存在，命令却不许直接碰——99 §1 硬规则）**：`dup2`（`vfs.rs:464` 有 `dup2_via`）、`lseek`（`vfs.rs:562` 有 `lseek_via`）、`getpid`（`pm.rs:222` 有 `getpid_via`）、`nanosleep`/`sleep`（`misc.rs:276` 有 `nanosleep_via`）。在 `os/libs/minix-sys/src/lib.rs` 顶层加对应薄封装即可（对齐现有 `open`/`read`/`write` 形状）。
- **整条调用未建（客户端封装 + VFS/PM 服务器对端臂都要写）**：
  - `pipe` / `pipe2`：`grep VFS_PIPE|pub fn pipe.*_via` 零命中——`sh` 管道、命令重定向的前置。
  - `unlink` / `rename` / `mkdir` / `rmdir` / `chmod` / `chown` / `link` / `symlink` / `chdir` / `umask`：均无 `*_via`——一批文件命令（`rm`/`mv`/`mkdir`/`ln`/`chmod` 等）的前置。
  - `getppid` / `times` / `pathconf` / `isatty` 等进程与属性调用：进程类命令（`ps`/`date` 部分面）前置。
- **归属**：14-stage-runtime（minix-sys 顶层 + 消息布局）+ 05-stage-vfs / 04-stage-pm（服务器对端臂）。跨 stage，按 `edge_todo.md` 的 edge 判定属共享契约层；相关既有 edge：E-CMDSYSFACE、E-INITSYS、E-REQWIRE。
- **锚点**：顶层现有 API 全表 `os/libs/minix-sys/src/lib.rs:123-418`；缺项以 §0 表末行 grep 结果为准。

### P0-5　shell 执行器 + 一组文件命令接线到目标可跑（端到端冒烟样板）

- **缺什么**：
  - `sh`（`os/commands/bin/shell`，文字层 36 测试已备）只有前端、没有执行器——落点决策已写（`todo.md §批次二十六`：[ARCH] A-2 下缺 `execve` 运行时面、`pipe2` 封装、`F_DUPFD`），三者今天都未齐（P0-4）。没有 shell，"运行命令"只能靠 init 一条条硬编码拉起。
  - `os/commands/bin/fileops` 现有薄壳只有 `basename/dirname/echo/expr/false/pathchk/printf/true`——**`cat`/`cp`/`ls`/`mv`/`rm`/`test`/`chmod` 都没有 `src/bin`**（`find` 实测 65 个 bin 里没有它们）。而 open-existing 路径今天已可用（`vfs.rs:671-729`），`ls` 需要的 `getdents`/`stat` 顶层也已就位（`lib.rs:318,365`）——即"决定半 + 薄壳"可以现在接。
- **要做**：先接 `cat`/`ls`/`echo` 三枚到目标可跑（依赖 P0-3 构建面 + P0-4 的 dup2/lseek），作为"多命令能在目标跑"的最小证据；再排 `sh` 执行批（依赖 P0-4 的 pipe + execve 面）。
- **归属**：18-stage-commands（命令接线，stage 内生产代码，不属 edge）。

---

## 3. P1 — 覆盖长尾：从"少数命令"到"全部 328 命令"（确保"多"）

> 命令总量口径：328 命令 / 865 .c / 425,130 行（`18-stage-commands/plan.md:14`）。今天有薄壳的只有 65 枚，其余为"决定半已写、薄壳待接"或"决定半也未写"。逐域接线每一次走一遍 `todo-fix`。

- **P1-1　逐命令接线（按域批次）**。已知有整批缺薄壳/缺决定半的域：
  - 06 文件：`cat`/`cp`/`ls`/`mv`/`rm`/`test`/`chmod`/`chgrp`/`touch`/`ln`/`readlink` 等（依赖 P0-4 的 `unlink`/`rename`/`mkdir`/`chmod`/`link`）。
  - 05 shell：`sh` 执行器（fork/exec/waitpid/pipe/dup2 齐后组装）。
  - 12 进程：`ps`（等内核表数据源，`todo.md 批次二十三`）、`date`/`sleep`/`nice`/`finger`/`from`。
  - 13 终端：`stty -g`、rows/columns、`tput`/`clear`（terminfo 装船随 A-2）。
  - 14~17 存储：`mount`/`umount`/`fdisk`/`mkfs.*`/`fsck.*`（受 E-FSCMDS + 盘上结构层约束）。
  - 18/19 网络：`ifconfig`/`ping`/`tcpdump`/`inet*` 等（等 socket 面 + 17-stage-net，见 edge E-NETSTART）。
  - 20/21/23/24：系统信息与游戏族的剩余留白面（`todo.md` 各批次"声明性留白"已逐条点名，如 adventure/monop 完整游玩循环、fortune 索引表版）。
- **P1-2　契约表 Requires 列逐命令回填**（`todo.md P1-1` 仍开放）。每接一域就回填该篇 Requires，并统计同一 API 的消费者数，作为 14-stage-runtime 补封装的排序依据。
- **P1-3　POSIX 基准引用落到每篇契约表头**（`todo.md P1-2` 仍开放）。
- **P1-4　装机面**：哪些 bin 进 rootfs、装到 bin/sbin/usr.bin/usr.sbin 哪层、PATH 语义——随 P0-1 镜像清单登记（`00-commands-overview.md §1.3` 四层目录）。

---

## 4. P2 — 第二优先级：代码缺陷与 translate 味道（确保"好"；发现即登记，修走 full-review/todo-fix）

> 项目强调 rewrite 不是 translate。以下是扫描中发现的"事实/正确性/一致性"问题，优先级低于 P0/P1，但会影响"跑起来后可信验收"。

- **P2-1（真缺陷，建议尽早修）　宿主系统调用假成功（E-SYSCALL-SIGN 未闭）**。`perform_syscall`（`os/libs/minix-sys/src/syscall.rs:94-101`）按 `m_type` 负值判错，而 `DirectTrapTransport` 的 `Err` 携带正 errno（`ipc.rs:549`；errno 全系正值如 `EEXIST=17`）——宿主下每条 `*_via` 假成功（echo 冒烟 `write` 返 `Ok(5)`、`exit 0`、无输出）。`perform_taskcall`（`syscall.rs:112-125`）同型。修法采 edge 方案 A：transport 级失败直接短路 `Err`，不再走 m_type 往返。**影响 P0/P1 的所有宿主冒烟验收，建议提到 P0 同批。**
- **P2-2（doc-code 漂移）　RS main.rs 过时注释**。`os/servers/rs/src/main.rs:48-67` 声称"KernelApi 生产 impl DEFERRED、fail-closed ENOSYS"，但实际 `rs/src/lib.rs:189-190` 已用 `TrapKernelApi`（`trap_api.rs`）。注释按文档-代码同步门更正。
- **P2-3（doc-code 漂移，涉 P0-2）　boot-shim loader.rs 注释自相矛盾**。`loader.rs:38-53` 一边描述按内核 12 模块 IPC 顺序消费、一边给出与其自身 6 项 `MODULE_NAMES` 与内核断言都不符的"字母分类序"。P0-2 对齐时一并消除。
- **P2-4（潜伏 wire 错值）　`CDEV_REPLY_BASE = 0x500`**。`os/libs/minix-chardriver/src/protocol.rs:22` 引 `com.h:934` 但真值应为 `CDEV_RS_BASE 0x480`（`com.h:920`；0x500 是 `BDEV_RQ_BASE`）。当前无消费者故未爆（edge E-CDRCONV 问题二）。收敛 chardriver 双实现时消除。
- **P2-5（设计归属悬空）　线程模型与 futex 无归属（E-THREAD-MODEL）**。命令层单线程（Minix3 命令皆单线程，`minix3/lib/libc/thread-stub/`），但若目标命令要上 Rust `std`（多线程/sync）则需线程模型决策；今天既无线程模型条目也无 futex。归属建议 14-stage-runtime 或 04-stage-pm，按 `[ARCH]` 立项。
- **P2-6（translate 味道持续盯防）**。逐命令接线时警惕"把 C 逐行译成 Rust"而非按语义重写（模式 16/17/65）；共享常量务必走 minix-types 单一权威，不在命令里手搓消息（99 §1 硬规则 + `tools/check-command-boundary.sh`，`todo.md §6.1.1`）。

---

## 5. 建议执行顺序与停止判据

1. **先打通一次端到端（P0-1 → P0-2 → P0-3）**：目标 = QEMU 里 `cargo xtask image && cargo xtask qemu` 起得来、init 跑到、`echo` 在目标机输出一次。这是"确保有"的最低线。
2. **同批修 P2-1**（宿主假成功），否则一切宿主冒烟验收不可信。
3. **P0-4 廉价再导出（dup2/lseek/getpid/sleep）** → 接 `cat`/`ls` 目标可跑（P0-5）。
4. **P0-4 整条未建族（pipe、文件操作族）** + **sh 执行器**（P0-5）→ 交互式跑命令。
5. 之后按 P1 逐域扩覆盖，每域一次 `todo-fix`，同步回填 Requires（P1-2）与 POSIX 基准（P1-3）。
6. 停止判据：三架构各自 QEMU 启动到 shell、`18-stage-commands` 全部 328 命令逐个跑通并按 POSIX/Minix3 真值验收。在此之前不停（覆盖与正确性都还有大量工作）。

> **收敛纪律提醒**：本清单不声称任何项已闭环；每条实现前按 fix-guard 重读目标行、grep 复核现状（本文锚点可能随并行轨道漂移，尤其 §0 已列出多处"文档滞后于代码"）；P0 未清不得对应用域标 CONVERGED。

---

## 6. 扫描方法与实证（便于另一路 AI / 用户复核）

本次结论来自实际代码扫描，非仅读文档。主要实证命令：

- `find notes/.../18-stage-commands` / 读 `edge_todo.md`（1096 行）、`todo.md`、`00-commands-overview.md`、`99-global-concepts.md`。
- `os/qemu-tests/README.md:92-100`（现有 QEMU 测试面）。
- `grep "pub fn" os/libs/minix-sys/src/lib.rs`（顶层 API 全表）；`grep "pub fn .*_via"`（`dup2_via:464`/`lseek_via:562`/`getpid_via:222`/`nanosleep_via:276` 在，pipe 一族零命中）。
- `grep "pub fn (argv|args|env|envs|progname)" os/libs/minix-rt/src/crt0.rs`（argv/env 访问器已在 `:141-199`）。
- `find os/commands -path '*/src/bin/*.rs'` → 65 个薄壳；fileops 无 cat/cp/ls/mv/rm。
- `os/xtask/src/main.rs:63-142`（build 只到 pm、image/qemu 是 skeleton）、`os/boot-shim/src/loader.rs:54`（6 模块）、`os/kernel/src/proc.rs:81`（NR_BOOT_MODULES=12）。
- 端到端 boot→exec→run-command 链路经交叉阅读（kmain/服务器 main/命令构建面）确认"未连接"。

*（本文档为 qwen 独立产出，中间与最终产物均以 `_qwen` 后缀隔离。）*
