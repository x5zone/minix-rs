# 00-rs-overview: RS 整体架构概览

> **分类**: 阶段 0 — 总览与导航（全文档入口）
> **源码**: `minix3/minix/servers/rs/`（8 个 .c，6307 行）+ `minix3/minix/include/minix/{com,rs,sef,ipc_filter}.h`（RS 协议面）
> **Rust 模块**: `os/servers/rs/` 全部（`lib.rs`/`main.rs` + `table`/`boot`/`sef`/`dispatch`/`service_slot`/`process_table`/`privilege`/`access`/`ipc_mask`/`monitor`/`slot`/`exec`/`service_create`/`publish`/`ready`/`request`/`query`/`recovery`/`live_update`/`state` 子模块，按本文档导航表逐篇落地）
> **前置**: 无（`01-stage-kernel/09-vm-boot-protocol.md` 提供 boot 链背景）
> **说明**: 本文档是 03-stage-rs 全部 21 篇文档的导航枢纽：回答 RS 是什么、在 boot 链的哪个位置、启动主线如何组织、服务生命周期次主线如何贯穿、21 篇文档如何按位置可回答性编排，以及覆盖契约（8 个 .c / 15 个消息类型 / 16 位 r_flags / 13 个 SF_* / RSS_*·SEF_* 标志面）。

---

## 1. 概念：RS 是什么

### 1.0 章节引言

RS（Reincarnation Server，复活服务器）是 Minix3 的 **root system process**：boot 时由内核直接启动的第二个用户服务，负责**加载、启动、监控、终止与恢复其余全部用户服务**。它的名字 "reincarnation" 来自其核心职责——服务崩溃后按策略重新拉起（复活）它。

> **本章不讲什么**（机制一律移交，只标注归属）:
> - boot 四步状态机的每一步细节（`01-rs-boot-init.md`）
> - 服务登记表 `rproc`/`rprocpub` 字段（`02-rs-process-table.md`）
> - 权限/隔离/发送掩码机制（`03-rs-privilege.md`、`04-rs-access-control.md`、`05-rs-ipc-sendmask.md`）
> - 主循环与心跳（`06-rs-main-loop.md`、`07-rs-period-heartbeat.md`）
> - 服务创建/发布/初始化/控制/查询（`08~14`）
> - 终止与恢复状态机（`15-rs-terminate-restart.md`）
> - Live Update 状态机与 RS 自升级（`16-rs-live-update.md`、`17-rs-state-data.md`、`18-rs-self-lifecycle.md`）
> - 外部接口契约与全局概念（`19-rs-external-interfaces.md`、`99-rs-global-concepts.md`）
>
> 本章只回答三个问题：**RS 是谁、它排在 boot 链哪里、它一辈子在做什么**。

### 1.1 RS 的身份：root system process

在 Minix3 的进程命名空间里，RS 是固定编号的系统服务：

- `RS_PROC_NR = 2`（`minix3/minix/include/minix/com.h:61`，用户进程编号区间 0~11：PM=0、VFS=1、RS=2、MEM=3、SCHED=4、TTY=5、DS=6、MIB=7、VM=8、PFS=9、MFS=10、INIT=11）
- `ROOT_SYS_PROC_NR = RS_PROC_NR`（`com.h:77`）——"root system process"即 RS：系统服务这一族进程的"根"，其余服务都由它派生/管理
- boot 映像登记顺序（`kernel/table.c:44-64`）：kernel 任务 5 个（asyncm/idle/clock/system/kernel）→ **DS 第一（`table.c:52`）→ RS 紧随其后（`table.c:53`）** → PM/SCHED/VFS/memory/tty/mib/vm/pfs/mfs/init

为什么 DS 必须在 RS 之前？`kernel/table.c:36-40` 的注释说明了顺序的语义：**表内顺序 = NOTIFY 消息投递优先级**。DS 是数据存储服务（系统事件异步发布依赖它，先启动保证"发布事件的服务"先于"订阅事件的服务"）；RS 紧随其后，因为它周期性地向系统服务投递 ping（心跳）消息（见 `07-rs-period-heartbeat.md`），需要高通知优先级。

### 1.2 boot 链中的位置：两层顺序语义

"RS 是第 2 个用户服务"要区分**登记顺序**与**执行顺序**两层含义：

| 层 | 顺序 | 依据 |
|----|------|------|
| 登记顺序 | ds → rs → pm → sched → vfs → memory → tty → mib → vm → pfs → mfs → init | `kernel/table.c:44-64` boot_image 数组 |
| 执行顺序 | kernel 任务 → **VM** → **RS** → 其余 | `kernel/main.c` boot 循环 + `RTS_VMINHIBIT` 抑制机制 |

执行顺序为什么是 VM 先于 RS？因为 RS 要加载其余服务，需要 VM 已经建好页表与地址空间。Minix3 的实现是：boot 循环中仅 VM（`VM_PROC_NR`）执行 `arch_boot_proc` 加载 ELF（`01-stage-kernel/06-proc-init-boot-proc.md`），其余进程挂 `RTS_VMINHIBIT` 等 VM 解除抑制（`01-stage-kernel/09-vm-boot-protocol.md`）。因此因果链是：

```
Kernel (boot)
  │  boot 期加载 VM ELF；VM 立即可调度，其余挂 RTS_VMINHIBIT
  ▼
VM (ptproc)
  │  为 PM/VFS/RS 等创建页表，解除抑制
  ▼
RS (root sysproc)      ← 本文档所在位置
  │  运行时加载并启动其余用户服务
  ├─► PM / SCHED / VFS / DS / MIB   （boot_image 直接登记，VM 解除后即可运行）
  ├─► IS / DEVMAN / INPUT / IPC     （RS 运行时加载）
  └─► INIT                          （boot_image 最后一项）
```

这一因果位置决定了 RS 的独特地位：**它是唯一一个"被内核直接启动、但职责是启动别人"的用户服务**。这带来一个自举问题——RS 启动时什么设施都没有，却要在别人依赖它之前先把自己启动好（`01-rs-boot-init.md` §1.1 的"先有鸡还是先有蛋"）。

### 1.3 它一辈子在做什么：两个状态机

RS 的全部工作可以浓缩为两个相互嵌套的状态机：

1. **启动状态机**（一次性）：`sef_cb_init_fresh()` 四步 boot——建 priv 权限 → 填发送掩码 → 等初始化 ready → 启动周期检查（`01-rs-boot-init.md`）
2. **运行时状态机**（永续）：主循环收消息 → 四类消息分类（CLOCK 周期 / 心跳 notify / RS_INIT·RS_LU_PREPARE ready / RS_* 控制请求）→ 分派 → 回复（`06-rs-main-loop.md`）

而运行时状态机的每次分派，落点都是**服务生命周期**（次主线，§3）：一个服务从诞生（RS_UP）到消亡（RS_DOWN/崩溃）再到复活（restart）的旅程。

---

## 2. 启动主线：从 main() 到主循环

### 2.0 主线图

```
kernel 启动 RS（root sysproc，boot_image 中 RS_PROC_NR 紧随 DS，table.c:53）
  │
  ▼  main.c:38  main()
  ├─ sef_local_startup()                  ← 01：SEF 回调注册（main.c:51）
  ├─ sys_getmachine(&machine)             ← 01/19：机器信息（main.c:53）
  ▼  sef_cb_init_fresh()（main.c:158，boot 锚点）
  ├─ env_parse(rs_verbose) + GET_HZ       ← 01/19：配置与系统频率
  ├─ rinit.rproctab_gid grant             ← 12：rprocpub 表 grant（main.c:185）
  ├─ RUPDATE_INIT() + shutting_down=FALSE ← 16：update 全局复位
  ├─ sys_getimage() → boot_image 表       ← 01：boot 映像（main.c:196）
  ├─ 重置 rproc/rprocpub 表               ← 02：进程表（main.c:230-237）
  ├─ Step 1: 逐服务设置 priv/sys/dev 属性 ← 03（priv）/05（send mask）/02（slot）
  ├─ Step 2: 允许运行（RS/VM 例外；init_service + catch_boot_init_ready）
  │                                        ← 12（ready 消息）/03（sched+ALLOW）
  ├─ Step 3: catch 剩余 init ready        ← 12
  ├─ Step 4: getnpid                      ← 02/19
  ├─ sys_setalarm(RS_DELTA_T)             ← 07：周期检查
  └─ USE_LIVEUPDATE: clone_slot + srv_fork（RS 自升级）← 18
  │
  ▼  main.c:56-130  主循环（运行时）
  ├─ rs_idle_period()                     ← 06：清理 RS_DEAD + 补 replica
  ├─ get_work() → sef_receive_status(ANY) ← 06
  ├─ rs_isokendpt()                       ← 02：caller 验证
  ├─ 消息分类：
  │   ├─ CLOCK notify  → do_period()      ← 07：周期检查/心跳
  │   ├─ 其他 notify   → 心跳（r_alive_tm）← 07
  │   ├─ RS_INIT       → do_init_ready()  ← 12：初始化完成
  │   ├─ RS_LU_PREPARE → do_upd_ready()   ← 12（update 分支→16）：update 就绪
  │   └─ RS_* 请求     → do_up/do_down/…  ← 13/14/16：控制请求
  └─ reply() / EDONTREPLY（late reply）   ← 06
```

（本图与 `plan.md §1.2` 同源，每个步骤后的 `NN` 是承载该机制的文档编号。**位置可回答性**原则：每一步必须能在图中找到自己的位置。）

### 2.1 主线的两个阶段

启动主线分两段，对应 RS 生命周期中"把自己启动好"与"开始管理别人"两个阶段：

- **阶段 A（一次性的 boot）**：`main()` 前半段 + `sef_cb_init_fresh()` 四步。产出：RS 自身可用（配置/机器信息/进程表/权限），并给每个 boot 服务建好槽位与权限。文档覆盖：`01`（编排与锚点）、`02`（数据底座）、`03`（权限）、`05`（发送掩码）、`12`（ready 协议）、`16`（update 复位）、`18`（自升级）。
- **阶段 B（永续的主循环）**：`main()` 后半段。产出：对系统服务的持续监控与响应。文档覆盖：`06`（心脏）、`07`（心跳）、`13/14`（控制/查询请求）、`12/16`（ready 消息的运行时分支）。

### 2.2 与 01-stage-kernel / 02-stage-vm 的主线对齐

同系列的组织原则（继承自 `01-stage-kernel/00-kernel-overview.md §3` 与 `02-stage-vm/plan.md §1.2`）：**读者学习顺序 = 系统实际执行顺序**。三条主线的锚点分别是：

| 阶段 | 锚点 | 主线 |
|------|------|------|
| 01-stage-kernel | `kmain()` / boot-shim | 内核启动到多任务 |
| 02-stage-vm | `main()` → `init_vm()` → 主循环 | VM 启动到内存服务 |
| 03-stage-rs | `main()` → `sef_cb_init_fresh()` → 主循环 | RS 启动到服务管理 |

三者在 boot 链上首尾相接：kernel → VM（页表）→ RS（服务加载）→ 其余服务。

---

## 3. 次主线：服务生命周期

### 3.0 生命周期图

启动主线是"一次性的"；次主线是"每服务重复的"。单个服务从诞生到消亡的旅程：

```
                 RS_UP（do_up，13）
                     │
                     ▼
             ┌────────────────┐
             │  create_service │← 10：fork + priv + sched + exec + VM 交互
             └────────────────┘
                     │
                     ▼
             ┌────────────────┐
             │   publish       │← 11：DS label 注册 + mapdriver + PCI ACL + devman bind
             └────────────────┘
                     │
                     ▼
             ┌────────────────┐
             │  init/run      │← 12：RS_INIT ready 协议（catch_boot_init_ready / do_init_ready）
             └────────────────┘
                     │
                     ▼
             ┌────────────────┐
             │  monitor       │← 07：心跳（alive/check/backoff）+ CLOCK 周期检查
             └────────────────┘
             │          │
   心跳正常    │          │ 崩溃/超时（SIGTERM→SIGKILL）
             │          ▼
             │   ┌────────────────┐
             │   │ terminate/restart│← 15：kill/crash/cleanup → backoff → reincarnate
             │   └────────────────┘
             │          │
             │          ▼（restart 决策：立即重试 / backoff / 放弃）
             ▼
   RS_DOWN / shutdown（do_down / do_shutdown，13）
```

（本图与 `plan.md §1.3` 同源。详细路径图分别在 `13-rs-control-requests.md`（控制面）与 `15-rs-terminate-restart.md`（终止/恢复面）内部绘制。）

### 3.1 生命周期的语义单元划分

次主线把 21 篇文档中的 10 篇串成一条因果链，每篇回答"服务旅程的哪一站"：

| 站 | 文档 | 回答的问题 |
|----|------|-----------|
| 准备 | `08-rs-slot-config.md` | 服务启动前的配置（命令串/参数/依赖/默认值）怎么进槽？ |
| 加载 | `09-rs-exec.md` | 可执行映像怎么被加载（ELF/栈帧/exec_restart）？ |
| 诞生 | `10-rs-service-create.md` | 服务进程怎么被创建（fork/priv/sched/VM/pin/clone）？ |
| 登记 | `11-rs-publish.md` | 服务怎么让全世界知道它（DS label/mapdriver/PCI/devman）？ |
| 就绪 | `12-rs-init-run.md` | 初始化完成怎么上报（RS_INIT ready）？ |
| 监控 | `07-rs-period-heartbeat.md` | 存活怎么被确认（心跳/超时/backoff）？ |
| 消亡 | `15-rs-terminate-restart.md` | 终止与恢复的状态机（kill/crash/cleanup/restart）？ |
| 复活 | `15`（同上）+ `10`（clone/replica） | 副本与 reincarnate 怎么补位？ |
| 升华 | `16-rs-live-update.md` | 服务怎么在不重启系统的前提下被替换（Live Update）？ |

### 3.2 为什么 RS 需要"复活"能力

Minix3 的可靠性模型：**系统服务崩溃不应导致整个系统崩溃**。RS 对每个服务维护一组恢复策略（`SF_*` 标志：`SF_DET_RESTART`/`SF_NORESTART`/backoff 时间），崩溃后按策略自动重启。这是 "Reincarnation" 名字的来源，也是 RS 区别于普通服务器（如 VFS 只管文件、PM 只管进程）的本质：**它管理的是"管理者的管理者"**。

---

## 4. 文档导航

### 4.0 21 篇文档总览

按 `plan.md §2` 的编号规则（`NN-短横线语义名.md`；`00-` 总览、`99-` 全局概念），7 个阶段 21 篇：

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 |
|------|------|------|---------|--------|
| 0 总览 | 00 | `00-rs-overview.md` | RS 是什么、boot 链位置、启动主线图、文档导航 | `servers/rs/` 全部 |
| 1 启动入口与进程表 | 01 | `01-rs-boot-init.md` | main/sef_local_startup/sef_cb_init_fresh 四步 boot/自升级流程/boot_image 表读取 | `main.c`、`table.c` |
| 1 | 02 | `02-rs-process-table.md` | rproc/rprocpub/rproc_ptr、rinit、rupdate、r_flags/sys_flags 全表、lookup/alloc/free、rs_isokendpt | `type.h`、`glo.h`、`rs.h`、`manager.c:1935-2109` |
| 2 权限与隔离 | 03 | `03-rs-privilege.md` | priv 结构建模、boot priv 初始化、privctl 操作面、sched_init_proc、update_sig_mgrs | `kernel/priv.h`、`main.c:240-345`、`utility.c:82-141,364-422` |
| 2 | 04 | `04-rs-access-control.md` | check_call_permission/caller_is_root/caller_can_control、isolation policy | `manager.c:21-130` |
| 2 | 05 | `05-rs-ipc-sendmask.md` | r_ipc_list、get_next_name、init_privs/add_forward_ipc/add_backward_ipc、IPC_ALL/IPC_ALL_SYS | `manager.c:2112-2331` |
| 3 主循环与监控 | 06 | `06-rs-main-loop.md` | 主循环、四类消息分类、reply/late_reply/EDONTREPLY、rs_idle_period/rs_is_idle | `main.c:38-131`、`utility.c:223-233,309-341,351-359,424-479` |
| 3 | 07 | `07-rs-period-heartbeat.md` | do_period、心跳/alive_tm/check_tm/stop_tm、init 超时、backoff、SIGTERM→SIGKILL、do_sigchld、update_period | `request.c:943-1049,1051-1092`、`update.c:371-397` |
| 4 服务创建与配置 | 08 | `08-rs-slot-config.md` | check_request、copy_rs_start/copy_label、init_slot/edit_slot、build_cmd_dep、inherit_service_defaults | `manager.c:135-173,289-324,1303-1327,1460-1701,1710-1797`、`request.c:1265-1309` |
| 4 | 09 | `09-rs-exec.md` | srv_execve/do_exec/exec_restart/read_seg、read_exec/share_exec/free_exec、SF_USE_COPY/SF_NEED_COPY | `exec.c`、`manager.c:1354-1455` |
| 4 | 10 | `10-rs-service-create.md` | create_service、clone_service、activate_service、clone_slot、swap_slot | `manager.c:531-786,1013-1033,1800-1932` |
| 4 | 11 | `11-rs-publish.md` | publish/unpublish：DS label、mapdriver、PCI ACL、devman bind/unbind | `manager.c:787-918` |
| 4 | 12 | `12-rs-init-run.md` | start_service、run_service、init_service、do_init_ready、do_upd_ready、catch_boot_init_ready、end_srv_init | `manager.c:328-356,923-987`、`utility.c:18-68`、`request.c:462-533,890-942`、`main.c:591-626,784-825` |
| 4 | 13 | `13-rs-control-requests.md` | do_up/do_down/do_restart/do_refresh/do_shutdown/do_clone/do_unclone/do_edit | `request.c:15-460`、`manager.c:988-1012` |
| 4 | 14 | `14-rs-query-requests.md` | do_lookup/do_getsysinfo/do_sysctl/do_fi、fi_service、print_services_status/print_update_status | `request.c:1095-1264`、`utility.c:69-81,142-222,485-546` |
| 5 终止与恢复 | 15 | `15-rs-terminate-restart.md` | terminate_service/restart_service/run_script/reincarnate_service、kill/crash/cleanup/detach_service、backoff 恢复、get_service_instances | `manager.c:360-530,1033-1052,1055-1353` |
| 6 Live Update | 16 | `16-rs-live-update.md` | do_update、rupdate 链、prepare/update/init 阶段、end_update/end_srv_update/abort/rollback、VM multi-component | `update.c` 全部、`request.c:534-889`、`utility.c:247-304` |
| 6 | 17 | `17-rs-state-data.md` | rupdater/sys_pids/instance/r_prev_rp·r_next_rp 链、update 状态数据结构 | `update.c`、`type.h:30-42`、`glo.h` |
| 6 | 18 | `18-rs-self-lifecycle.md` | RS 自身：SEF LU 回调、SEF_INIT_*、srv_fork 自升级、SEF_RS_UPDATE_SELF、重启链 | `main.c:436-590`、`update.c:230-366`、`utility.c:387-412`、`manager.c:760-780`、`lib/libsys/sef*.c` |
| 7 外部接口与全局 | 19 | `19-rs-external-interfaces.md` | 全部外部 syscall 签名与消息映射（sys_privctl/sys_getimage/sched_*/vm_*/DS/mapdriver/PCI/libexec/SEF） | `lib/libsys/*.c`、`include/minix/{com,ipc,sef}.h` |
| 7 | 99 | `99-rs-global-concepts.md` | const.h 常量全表、RS_RQ_BASE 消息类型、RSS_*/SF_*/SEF_*/IPCF_*/SYS_PRIV_* 标志、错误表 | `const.h`、`rs.h`、`sef.h`、`ipc_filter.h`、`com.h` |

### 4.1 依赖关系（阅读顺序）

21 篇文档的依赖是**严格的线性**（编号即顺序），每篇的前置依赖：

```
00
 │
 ▼
01 ──► 02 ──► 03 ──► 04 ──► 05 ──► 06 ──► 07
                                              │
                                              ▼
                                        08 ──► 09 ──► 10 ──► 11 ──► 12
                                                                      │
                                              ┌───────────────────────┤
                                              ▼                       ▼
                                        13 ──► 14                15
                                                                  │
                                                                  ▼
                                                            16 ──► 17 ──► 18
                                                                              │
                                                                              ▼
                                                                        19 ──► 99
```

例外（导航骨架文档豁免）：`01` 作为"映射 02~18 锚点"的导航骨架，允许前向引用机制归属（只标注不展开，`01` §2.2 声明豁免）；`19`/`99` 允许引用 `lib/libsys` 等外部实现（`plan.md §3.3` 唯一例外）。

### 4.2 与其他 stage 的交叉引用

| 方向 | 文档 | 引用内容 |
|------|------|---------|
| 向上 | `../01-stage-kernel/22-privilege.md` | kernel 侧 priv 结构与 `sys_privctl` 实现（03 的底层） |
| 向上 | `../01-stage-kernel/23-ipc-filter.md` | IPC filter（`IPCF_*`）语义（05 的底层） |
| 向上 | `../01-stage-kernel/09-vm-boot-protocol.md` | boot 链（VM 解除抑制） |
| 向上 | `../01-stage-kernel/06-proc-init-boot-proc.md` | boot 循环中仅 VM 加载 ELF |
| 向下 | `../02-stage-vm/25-rs-services.md` | VM 侧 RS 服务（SET_PRIV/PREPARE/UPDATE/MEMCTL），双向核对 |
| 向下 | `../02-stage-vm/01-vm-init-main.md` 等 | VM 启动主线（主线对齐） |
| 向下 | `../04-stage-pm/`、`../05-stage-vfs/` 等 | RS 加载的服务（消费方） |

---

## 5. 设计原则

### 5.1 位置可回答性

每篇文档必须能回答：**它位于 RS 启动时序（sef_cb_init_fresh 4 步）或主循环（dispatch）的哪个位置**（`plan.md §1.2`）。这是从 `01-stage-kernel` 继承的组织原则，保证文档流与系统实际执行因果链一致。

### 5.2 禁止前向引用

概念首次出现即完整解释，后续只引用不复述。编号顺序已保证依赖机制前置（priv 在 03、send mask 在 05、ready 消息在 12、update 状态机在 16，均在引用方之前或作为导航骨架）。唯一豁免：`01`（导航骨架，只标注不展开）、`19`/`99`（外部依赖面/全局常量表）。

### 5.3 每篇一个语义单元

每篇文档可独立阅读，围绕一个核心问题组织（见 §4.0 表"语义模块"列）。

### 5.4 ARCH 三处一致标注

架构演进（`plan.md §4`，A-1~A-14）必须三处一致标注：**Minix3 行为对照点 + design doc（.design/{NN}-design.v*.md）+ 代码注释**。任何一处缺失即 P0-design-deviation。本 stage 的 ARCH 项概览：

| ARCH | 演进 | 涉及文档 |
|------|------|---------|
| A-1 | 服务创建 fork 语义（no_std 无 libc fork） | 10/15 |
| A-2 | `mess_rs_*` → minix-types 新增 rs 消息模块（typed enums） | 99/19 |
| A-3 | 裸指针四链 → `Vec<ServiceSlot>` + `Option<SlotId>` 索引链 | 02/10/16 |
| A-4 | `rproc_ptr[NR_PROCS]` → 数组 + `Option<SlotId>` | 02 |
| A-5 | exec image 共享 → `Arc<[u8]>` | 09 |
| A-6 | update C flags → 类型化状态机 `UpdatePhase` enum | 16 |
| A-7 | SEF 框架 → minix-rs 用户态 SEF 抽象（RS 先行） | 01/12/18 |
| A-8 | libexec → `minix-elf` crate + Rust 栈帧构建 | 09 |
| A-9 | 超时接收 `rs_receive_ticks` → `receive_timeout` 原语 | 16 |
| A-10 | PCI 支持 → defer + `PciAcl` 占位（fail-closed） | 11/99 |
| A-11 | 编译宏 → cargo feature（`live-update` 默认关） | 01/99 |
| A-12 | errno + panic + printf → `Errno(i32)` newtype + `Result` | 99/各文档 §3 |
| A-13 | boot 三表 → 静态表（`table.rs`） | 01/02 |
| A-14 | 状态数据协议常量与安全默认（x86-64 目标布局 56 vs C i386 28 / `Endpoint::NONE` / 空 label fail-closed） | 17 |

### 5.5 覆盖契约

本 stage 的语义覆盖契约（`plan.md §5`，grep 实证通过，见 `plan.md §7.2`）：

| 覆盖面 | 数量 | 证据 |
|--------|------|------|
| C 源文件 | 8 个 .c（6307 行） | `wc -l servers/rs/*.c` |
| RS IPC 消息类型 | 15 | `grep -cE '#define RS_(UP\|...\|LU_PREPARE)[[:space:]]' com.h` = 15 |
| `r_flags` | 16 位 | `grep -cE '#define RS_(IN_USE\|...\|REINCARNATE)[[:space:]]' const.h` = 16 |
| `SF_*` | 13 位 | `grep -cE '#define SF_(CORE_SRV\|...\|NO_BIN_EXP)[[:space:]]' rs.h` = 13 |
| `RSS_*`/`SEF_*`/`IPCF_*`/`SYS_PRIV_*` | 全表 | `rs.h`/`sef.h`/`ipc_filter.h`/`com.h` |
| C 函数面 | 121 个函数全部归位 | `plan.md §7.2 R-5`（main 12/manager 45/request 18/update 24/utility 17/exec 2/error 3/table 0） |

---

## 6. 阅读路线

按目的选择入口：

- **想理解 RS 是什么** → 本文档 §1 + §2 + §3
- **想理解 boot 怎么完成** → `01` → `02` → `03` → `05`（Step 1 机制）→ `12`（ready 协议）→ `16/18`（update/自升级）
- **想理解运行时怎么工作** → `06` → `07` → `13` → `14`
- **想理解服务怎么被创建** → `08` → `09` → `10` → `11` → `12` → `13`
- **想理解崩溃恢复** → `15`（含终止/恢复状态机图）
- **想理解 Live Update** → `16` → `17` → `18`
- **想核对 Rust 依赖面** → `19`（外部接口契约）→ `99`（全局常量）

---

## 7. 过渡

本文档建立了 RS 的全局心智模型（身份/位置/两个状态机/文档导航/设计原则/覆盖契约）。下一站是启动主线的第一步：`01-rs-boot-init.md` 详细展开 `main()` → `sef_cb_init_fresh()` 四步 boot 的完整时序，并作为阶段 1~7 全部文档的导航锚点。

## 8. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/plan.md` — 文档重组计划（§1.2 启动主线 / §1.3 次主线 / §2 编号 / §3.3 引用规则 / §4 ARCH / §5 覆盖契约 / §7 review 记录）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/draft/README.md` — 占位素材（早期理解，非 ground truth）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（§3 组织原则）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/09-vm-boot-protocol.md` — boot 链（VM 解除抑制）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md` — 同流程先例（§1.2 主线组织）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/25-rs-services.md` — VM 侧 RS 服务（双向核对）
- `minix3/minix/servers/rs/` — C 源码（ground truth）
- `minix3/minix/include/minix/{com,rs,sef,ipc_filter}.h` — 协议定义
- `os/servers/rs/` — Rust 实现
