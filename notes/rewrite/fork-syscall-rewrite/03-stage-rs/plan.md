# 03-stage-rs 文档重组计划（plan.md）

> **状态**: **定稿**（2026-08-15 首版 + 深度 review + minix3 源码回归 review，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/`
> **目标**: 以 **RS server 启动顺序为主线**定义 RS 全部文档；服务生命周期为次主线；最终覆盖 Minix3 RS server（`servers/rs/`，8 个 .c，6307 行）全部语义，支撑 RS server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/`（同流程先例）、`minix3/minix/servers/rs/`（ground truth）、`os/servers/rs/`（Rust 实现，当前为空壳 stub）

---

## 1. 背景与动机

### 1.1 现状问题

`03-stage-rs/` 目录自 2026-08-13 创建以来仅有**占位 README**（已移入 `draft/`），没有任何正式文档。与它的 boot 地位严重不匹配：

1. **boot 位置关键**——RS 是 boot 执行顺序中第 2 个运行的用户服务（root system process：`include/minix/com.h:61` 定义 `RS_PROC_NR=2`、`:77` 定义 `ROOT_SYS_PROC_NR=RS_PROC_NR`；boot 映像顺序见 `kernel/table.c:36-48`：DS 第一、RS 紧随其后），是"加载并启动其余用户服务"的角色（`00-master-plan/README.md` 因果链：Kernel → VM → **RS** → 其余）。它的语义理解是 PM/VFS/SCHED/DS 等后续 stage 的前置。
2. **语义复杂度高**——`servers/rs/` 共 6307 行 C，是**用户态服务中状态机最密集的服务器**：boot 初始化 4 步状态机、服务生命周期状态机（create→publish→run→monitor→terminate→restart→cleanup）、心跳监控状态机（alive/check/backoff）、Live Update 状态机（prepare→update→init→end/rollback）。此外还依赖 7 个外部服务（PM/SCHED/VM/DS/VFS/PCI/devman）+ 内核 syslib 面。
3. **与 02-stage-vm 的差异**——02-stage-vm 有旧 fork 主线文档可迁移；03-stage-rs **无旧文档可迁移**（只有占位 README）。因此本计划从零定义文档集，`draft/` 仅保留占位素材；这也意味着本计划的覆盖契约（§5）是后续写作的**唯一权威基线**，必须一次到位。

### 1.2 新主线：RS server 启动顺序 + 运行时主循环

与 `01-stage-kernel` / `02-stage-vm` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。RS 的启动与运行是严格线性的：

```
kernel 启动 RS（root sysproc，boot_image 中 RS_PROC_NR 紧随 DS）
  │
  ▼  main.c:38  main()
  ├─ sef_local_startup()                  ← 01：SEF 回调注册
  ├─ sys_getmachine()                     ← 01/19：机器信息
  ▼  sef_cb_init_fresh()（main.c:158，boot 锚点）
  ├─ env_parse(rs_verbose) + GET_HZ       ← 01/19：配置与系统频率
  ├─ rinit.rproctab_gid grant             ← 12：rprocpub 表 grant
  ├─ RUPDATE_INIT() + shutting_down=FALSE ← 16：update 全局复位
  ├─ sys_getimage() → boot_image 表       ← 01：boot 映像
  ├─ 重置 rproc/rprocpub 表               ← 02：进程表
  ├─ Step 1: 逐服务设置 priv/sys/dev 属性  ← 03（priv）/05（send mask）/02（slot）
  ├─ Step 2: 允许运行（RS/VM 例外；init_service + catch_boot_init_ready）
  │                                        ← 12（ready 消息）/03（sched+ALLOW）
  ├─ Step 3: catch 剩余 init ready        ← 12
  ├─ Step 4: getnpid                      ← 02/19
  ├─ sys_setalarm(RS_DELTA_T)             ← 07：周期检查
  └─ USE_LIVEUPDATE: clone_slot + srv_fork（RS 自升级）← 18
  │
  ▼  main.c:50-120  主循环（运行时）
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

**每篇文档必须能回答一个问题：它位于 RS 启动时序（sef_cb_init_fresh 4 步）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 与 `02-stage-vm/plan.md §1.2` 继承的组织原则。

### 1.3 次主线：服务生命周期

RS 的全部工作本质是**系统服务的生命周期管理**。次主线以"单个服务从诞生到消亡的旅程"贯穿阶段 4~6，其路径图在 `13-rs-control-requests` 与 `15-rs-terminate-restart` 内部绘制：

```
RS_UP 到达（主循环 dispatch）               ← 13
  ├─ 04 访问控制：check_call_permission    ← 调用者权限
  ├─ 08 slot 配置：alloc_slot + init_slot  ← 请求参数 → rproc
  ├─ 10 创建：create_service（fork）       ← 03 priv / 05 sendmask / 09 exec / 19 外部面
  ├─ 11 发布：publish_service              ← DS label / VFS mapdriver / PCI / devman
  ├─ 12 运行：run_service → RS_INIT 握手    ← 12 ready 消息
  ├─ 07 监控：do_period 心跳/超时           ← 周期检查
  ├─ 13 停止：RS_DOWN → stop_service       ← SIGTERM → SIGKILL 升级
  ├─ 15 终止：terminate_service            ← restart/backoff/cleanup 分支
  └─ 16 热升级：RS_UPDATE → Live Update    ← prepare→update→init→end
```

**位置可回答性双锚点**：每篇文档同时回答它在（a）boot 序列/主循环、（b）服务生命周期中的位置。

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。`draft/` 仅保留占位 README（素材），新编号在顶层重新建立。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块（规划） | draft 来源 | 变更 |
|------|------|------|---------|--------|------------------|-----------|------|
| 0 总览 | 00 | `00-rs-overview.md` | RS 是什么、boot 链位置、启动主线图、文档导航 | `servers/rs/` 全部 | 全部 | `draft/README` | **新建**（导航按启动主线叙事） |
| 1 启动入口与进程表 | 01 | `01-rs-boot-init.md` | main/sef_local_startup/sef_cb_init_fresh 四步 boot/sef_cb_init_restart·lu 流程/自升级流程（USE_LIVEUPDATE）、boot_image 表读取 | `main.c`、`table.c` | `main.rs`、`lib.rs` | — | **新建**（骨架，映射 02~18 锚点） |
| 1 | 02 | `02-rs-process-table.md` | rproc/rprocpub/rproc_ptr、rinit、rupdate、r_flags/sys_flags 全表、lookup_slot_by_*/alloc_slot/free_slot/rs_isokendpt | `type.h`、`glo.h`、`rs.h`、`manager.c:1935-2109` | `process_table.rs`、`service_slot.rs` | — | **新建** |
| 2 权限与隔离 | 03 | `03-rs-privilege.md` | priv 结构建模、boot priv 初始化（static_priv_id/flags/trap mask/call mask/sig mgr）、privctl 操作面、sched_init_proc、update_sig_mgrs、vm_call_mask | `kernel/priv.h`、`main.c:240-345`、`utility.c:82-141,364-422` | `privilege.rs`、`sched.rs` | — | **新建**（boot Step 1 的权限机制） |
| 2 | 04 | `04-rs-access-control.md` | check_call_permission/caller_is_root/caller_can_control、isolation policy（r_control） | `manager.c:21-130` | `access.rs` | — | **新建** |
| 2 | 05 | `05-rs-ipc-sendmask.md` | r_ipc_list、get_next_name、init_privs/add_forward_ipc/add_backward_ipc、IPC_ALL/IPC_ALL_SYS 语义 | `manager.c:2112-2331` | `ipc_mask.rs` | — | **新建** |
| 3 主循环与监控 | 06 | `06-rs-main-loop.md` | 主循环、四类消息分类、reply/late_reply/EDONTREPLY、rs_idle_period/rs_is_idle | `main.c:38-131`、`utility.c:223-233,309-341,351-359,424-479` | `main.rs`、`dispatcher.rs` | — | **新建**（RS 运行时心脏） |
| 3 | 07 | `07-rs-period-heartbeat.md` | do_period、心跳/alive_tm/check_tm/stop_tm、init 超时、backoff、SIGTERM→SIGKILL、do_sigchld、update_period、sys_setalarm | `request.c:943-1049,1051-1092`、`update.c:371-397` | `monitor.rs` | — | **新建** |
| 4 服务创建与配置 | 08 | `08-rs-slot-config.md` | check_request、copy_rs_start/copy_label、init_slot/edit_slot、build_cmd_dep、inherit_service_defaults | `manager.c:135-173,289-324,1303-1327,1460-1701,1710-1797`、`request.c:1265-1309` | `slot.rs`、`rs_start.rs` | — | **新建** |
| 4 | 09 | `09-rs-exec.md` | srv_execve/do_exec/exec_restart/read_seg、read_exec/share_exec/free_exec、SF_USE_COPY/SF_NEED_COPY | `exec.c`、`manager.c:1354-1455` | `exec.rs` | — | **新建**（libexec → minix-elf，ARCH A-8） |
| 4 | 10 | `10-rs-service-create.md` | create_service（fork/priv/sched/exec/VM 交互/pin）、clone_service、activate_service、clone_slot、swap_slot | `manager.c:531-786,1013-1033,1800-1932` | `service_create.rs` | — | **新建** |
| 4 | 11 | `11-rs-publish.md` | publish/unpublish：DS label、mapdriver、PCI ACL、devman bind/unbind | `manager.c:787-918` | `publish.rs` | — | **新建** |
| 4 | 12 | `12-rs-init-run.md` | 服务启动与初始化协议：start_service（编排）、run_service（原语）、init_service（RS_INIT 消息构造）、do_init_ready、do_upd_ready、catch_boot_init_ready、end_srv_init、sef_cb_init_response/lu_response（RS 自模拟 ready） | `manager.c:328-356,923-987`、`utility.c:18-68`、`request.c:462-533,890-942`、`main.c:591-626,784-825` | `ready.rs`、`run.rs` | — | **新建**（boot Step 2/3 + 运行时共享） |
| 4 | 13 | `13-rs-control-requests.md` | do_up/do_down/do_restart/do_refresh/do_shutdown/do_clone/do_unclone/do_edit（不含 RS_UPDATE） | `request.c:15-460`、`manager.c:988-1012` | `request.rs` | — | **新建** + 绘制服务生命周期次主线路径图 |
| 4 | 14 | `14-rs-query-requests.md` | do_lookup/do_getsysinfo/do_sysctl/do_fi、fi_service、print_services_status/print_update_status | `request.c:1095-1264`、`utility.c:69-81,142-222,485-546` | `query.rs` | — | **新建** |
| 5 终止与恢复 | 15 | `15-rs-terminate-restart.md` | terminate_service/restart_service/run_script/reincarnate_service、kill/crash/cleanup/detach_service、backoff 恢复、RS_DEAD 清理、get_service_instances | `manager.c:360-530,1033-1052,1055-1353` | `recovery.rs` | — | **新建** + 绘制终止/恢复状态机图 |
| 6 Live Update | 16 | `16-rs-live-update.md` | do_update、rupdate 链（add/iter/排序）、prepare/update/init 阶段、end_update/end_srv_update/abort/rollback、VM multi-component | `update.c` 全部、`request.c:534-889`、`utility.c:247-304` | `live_update.rs` | — | **新建**（核心状态机） |
| 6 | 17 | `17-rs-state-data.md` | init_state_data、rs_state_data/rs_ipc_filter_el、IPC filter 解析（ANY_USR/SYS/TSK）、state grants（cpf_*） | `manager.c:174-284`、`type.h:30-42`、`ipc_filter.h` | `state_data.rs` | — | **新建** |
| 6 | 18 | `18-rs-self-lifecycle.md` | sef_cb_init_restart/lu 细节、boot 自升级流程、RS 信号管理器备份、RS rollback 特例（sys_whoami/vm_update ROLLBACK） | `main.c:436-590`、`update.c:230-366`、`utility.c:387-412`、`manager.c:760-780` | `self_lifecycle.rs` | — | **新建** |
| 7 外部接口 | 19 | `19-rs-external-interfaces.md` | syslib 面、srv_*（PM）、vm_*、ds_*、sched_*、libexec、mapdriver、PCI/devman 消息契约 | `lib/libsys/*`、`include/minix/*.h`、`lib/libexec/*` | `minix-sys`、`minix-types`（新增 rs 消息模块） | — | **新建**（Rust 依赖面契约） |
| 99 全局概念 | 99 | `99-rs-global-concepts.md` | 常量、RS_* 消息类型、RSS_*/SF_* 标志、SEF_INIT_*/SEF_LU_*、endpoint、错误表（error.c） | `const.h`、`com.h:463-492`、`rs.h`、`sef.h`、`error.c` | `minix-types` | — | **新建** |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"、`02-stage-vm/plan.md §2.1`），说明本阶段在启动时序中的位置与下一阶段的入口：

```
01（boot 骨架）→ 02（进程表）→ 03/04/05（权限·访问控制·IPC 掩码：boot Step 1 的机制）
→ 06/07（主循环 + 周期监控：boot 完成后的运行时心脏）
→ 08~11（服务创建机制：slot 配置/exec/创建/发布）
→ 12（启动与初始化协议：boot Step 2/3 与运行时 RS_INIT/RS_LU_PREPARE 处理）
→ 13/14（控制与查询请求）
→ 15（终止与恢复状态机）
→ 16~18（Live Update 全链 + RS 自身生命周期）
→ 19（外部接口契约）→ 99（全局概念）
```

### 2.2 前向引用豁免（导航性引用）

`01-rs-boot-init` 是**导航骨架**：它按 boot 顺序映射到 02~18 的锚点（如 Step 1 → 03/05），这是继承自 `02-stage-vm/plan.md` 的约定（其 01 同样映射 init_vm 各步）。**概念解释禁止前向引用**——01 只标注"某步的机制见某篇"，不展开机制。同理，`06-rs-main-loop` 的 dispatch 表只列 handler 名称，机制在 12~16 展开；`04`/`07` 中出现的 `RUPDATE_IS_UPDATING()` 只陈述规则（"update 进行中 → EBUSY/跳过"），状态机在 16 展开。另有 4 处单函数引用豁免：`12` 的 `end_srv_init` 内部调用 `cleanup_service`（机制在 15）与 `rupdate_upd_move`（机制在 16）；`15` 的 `terminate_service` 调用 `abort_update_proc`（机制在 16）；`07` 的 `update_period` 调用 `end_update`（机制在 16）——均只陈述调用点，不展开机制。

---

## 3. 讲述结构规范（参照 01-stage-kernel / 02-stage-vm）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel`（`03-kmain-cstart.md`、`04-platform-discovery.md`）一致：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在启动时序/主循环/服务生命周期中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3 / 02-stage-vm §3.2）

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：priv 在 03、send mask 在 05、ready 消息在 12、update 状态机在 16，均在引用方之前或作为导航骨架）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 boot 序列/主循环/服务生命周期的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `12-rs-init-run.md` §do_init_ready）
- 与 kernel 文档交叉引用用 `../01-stage-kernel/NN-*.md`（如 privctl → `22-privilege.md`、IPC filter → `23-ipc-filter.md`）
- 与 VM 文档交叉引用用 `../02-stage-vm/NN-*.md`（如 `vm_memctl` → `25-rs-services.md`）
- 对 draft 素材的引用一律指向 `draft/NN-*.md` 并标注"素材"；正式文档绝不引用 review 产物
- 新增外部依赖文档（`19-rs-external-interfaces.md`）是唯一允许引用 `lib/libsys/*.c` 的文档，其余文档引用外部函数时指向 19

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 03-stage-rs 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **服务创建 fork 语义** | `srv_fork(0,0)` 走 `PM_SRV_FORK`（libsys/srv_fork.c），PM 在 VFS 注册后 fork 出子进程；`run_script` 直接 `fork()+execle(sh)` | no_std 无 libc fork。外部行为保持：create_service 仍经 PM 创建子进程；`run_script` 的 fork+execle 需设计决策（委托 INIT/受限执行器，或标注 defer） | 10/15 | 设计决策 + 缺口标注 |
| A-2 | **消息类型** | `mess_rs_*` 裸 C 联合体（ipc.h:1866-1906）+ `m_rs_req` 通用槽 | `minix-types` 新增 rs 消息模块：`RsRequest`/`RsInit`/`RsUpdate` typed enums（参照 `ipc/vm.rs` 既有模式） | 99/19 | **需新增**（当前 minix-types 无 RS 类型） |
| A-3 | **服务实例指针链** | `r_prev_rp/r_next_rp/r_old_rp/r_new_rp` 裸指针四链 + `get_service_instances` 遍历 | `Vec<ServiceSlot>` + `Option<SlotId>` 索引链，杜绝裸指针悬垂；`ServiceInstances` 迭代器替代静态数组 | 02/10/16 | 设计决策 |
| A-4 | **快速索引** | `rproc_ptr[NR_PROCS]` 端点→slot 指针数组 | 数组 + `Option<SlotId>`（端点号 → slot id 映射保持 O(1)） | 02 | 设计决策 |
| A-5 | **exec image 内存副本** | `malloc` + `share_exec` 指针共享（free_exec 引用计数扫描） | `Arc<[u8]>`（共享副本语义）或 `Box<[u8]>`（独占），`Arc::strong_count` 替代全表扫描 | 09 | 设计决策 |
| A-6 | **Live Update 状态机** | C flags（`RS_UPDATING/RS_INITIALIZING/...`）+ `RUPDATE_ITER`/`REV_ITER` 宏遍历双向链 | 类型化状态机：`UpdatePhase` enum（Scheduled/Preparing/Updating/Initializing/Ended）+ 显式链迭代器；非法状态不可表达 | 16 | 设计决策 |
| A-7 | **SEF 框架** | libsys `sef.c` 回调注册/init/lu/restart/signal 拦截 | minix-rs 需建立用户态 SEF 抽象（init 消息拦截、回调表），RS 先行实现并复用至 PM/VFS 等后续服务器 | 01/12/18 | **需新增**（01-stage-kernel 仅提供 IPC 基础） |
| A-8 | **exec 加载器** | libexec（`libexec_load_elf` + `minix_stack_params/fill` + PM_EXEC_RESTART） | `minix-elf` crate（已有 `parse_ehdr/segment_iter/entry_point`）+ Rust 栈帧构建 + PM_EXEC_RESTART 消息 | 09 | 已实现基础，需接线 |
| A-9 | **超时接收** | `rs_receive_ticks`：IPC filter + `sys_setalarm2` 双源接收（唯一调用点 update.c:590） | 超时接收原语：状态过滤（src + CLOCK）+ 定时器；Rust 侧封装为 `receive_timeout` | 16 | 设计决策 |
| A-10 | **PCI 支持** | `USE_PCI`（默认 yes）：`pci_set_acl/pci_del_acl` | minix-rs 无 PCI 驱动面 → 标注 defer，保留 `PciAcl` 数据结构占位（fail-closed） | 11/99 | **缺口**：defer + 语义契约 |
| A-11 | **构建配置宏** | `USE_LIVEUPDATE`/`USE_PCI`/`DEBUG`/`PRIV_DEBUG` 编译宏 | cargo feature（如 `feature = "live-update"`）或无条件实现核心语义；调试打印 → `#[cfg(feature)]` | 01/99 | 设计差异 |
| A-12 | **错误与日志** | errno + `panic()` + `printf` | `Errno(i32)` newtype（minix-types）+ `Result`；panic 保留 fail-closed 语义（对照 `02-stage-vm` 同款约束） | 99/各文档 §3 | 设计差异 |
| A-13 | **boot 表来源** | `sys_getimage()` 运行期拷贝 kernel `boot_image[]` + RS 自身 `table.c` 三张表 | boot 信息经 01-stage-kernel 的 `sys_getimage` 语义传入；`boot_image_priv/sys/dev` 三表在 minix-rs 侧可静态化（同 kernel 表） | 01/02 | 设计决策 |
| A-14 | **状态数据协议常量与安全默认** | C 字节布局随架构：`sizeof(struct rs_state_data)` 本树 i386 = 28，x86-64 目标 = 56（manager.c:190 的 E2BIG 门）；`m_source` 未设 MATCH 门时 C 写 0（PM 端点）；`strtol("")` 空串返回 0 无错误 → PM | x86-64 目标布局常量建模（`RS_STATE_DATA_SIZE=56`，wire 拷贝在 19）；门控字段默认 `Endpoint::NONE`；空 label fail-closed `ESRCH`（不沿袭 C 的 strtol 空串→0） | 17 | 设计差异 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射（8 个 .c）

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `main.c` | 834 | 01/06/12/18 | 已核对 |
| `manager.c` | 2332 | 02/04/05/08/09/10/11/12/15/17 | 已核对 |
| `request.c` | 1309 | 07/12/13/14/16 | 已核对 |
| `update.c` | 1011 | 16/18 | 已核对 |
| `utility.c` | 547 | 03/06/12/14/16 | 已核对 |
| `exec.c` | 165 | 09 | 已核对 |
| `error.c` | 59 | 99 | 已核对 |
| `table.c` | 50 | 01/02 | 已核对 |

### 5.2 头文件覆盖

| 头文件 | 归属 | 核对 |
|--------|------|------|
| `servers/rs/type.h`（boot_image_priv/sys/dev、rprocupd、rupdate、rproc） | 02/16/17 | 已核对 |
| `servers/rs/glo.h`（rprocpub/rproc/rproc_ptr/rinit/rupdate/rs_verbose/shutting_down/system_hz/machine） | 02/99 | 已核对 |
| `servers/rs/const.h`（r_flags 全表、周期常量、backoff、LU 常量、更新宏） | 02/07/16/99 | 已核对 |
| `servers/rs/proto.h` | 全部分布（函数签名来源） | 已核对 |
| `servers/rs/inc.h` | 00（依赖面声明） | 已核对 |
| `include/minix/rs.h`（rprocpub 定义、SF_* 全表、RSS_* 全表、rs_start、rs_state_data、rs_pci） | 02/08/17/99 | 已核对 |
| `include/minix/com.h` RS 段（RS_RQ_BASE + 0..24、RS_SYSCTL_*、RS_FI_CRASH、VM_RS_*、SYS_PRIV_*、SYS_STATE_*） | 19/99 | 已核对 |
| `include/minix/ipc.h`（mess_rs_*、mess_lsys_pm_srv_fork、mess_lsys_getsysinfo、mess_lsys_fi_ctl、mess_lsys_vfs_mapdriver） | 19/99 | 已核对 |
| `include/minix/sef.h`（SEF_INIT_*/SEF_LU_*/sef_init_info_t/SEF_CB_*） | 01/12/16/18/99 | 已核对 |
| `include/minix/ipc_filter.h`（IPCF_*、ANY_USR/SYS/TSK、ipc_filter_el_t） | 17 | 已核对 |
| `include/minix/ds.h`（ds_publish_label/delete/retrieve） | 11/19 | 已核对 |
| `include/minix/sched.h`（sched_start/sched_stop） | 03/19 | 已核对 |
| `include/minix/vm.h`（vm_memctl/vm_set_priv/vm_update/vm_prepare） | 10/16/19 | 已核对 |
| `include/minix/minlib.h`/`libsys/srv_*.c`（srv_fork/srv_kill/getnpid/getnuid/getprocnr/mapdriver） | 10/15/19 | 已核对 |
| `kernel/priv.h`（priv 结构、priv 表、may_send_to 宏） | 03/99 | 已核对 |

### 5.3 语义模块覆盖清单（函数级）

> 每篇文档必须覆盖的函数清单以 §7 回归 review 为准，逐篇核对。以下列出跨文件的关键语义，防止"文件有映射但函数漏掉"：

- **01**：`main`、`sef_local_startup`、`sef_cb_init_fresh`（四步 boot 全流程）、`boot_image_info_lookup`（四表查找）、`get_work`、`sys_getimage` 调用点、`rinit.rproctab_gid` 创建点、`USE_LIVEUPDATE` 自升级流程（clone_slot + srv_fork + update_service + cpf_reload + cleanup_service + vm_memctl pin / privctl SET_SYS + sched_init_proc + YIELD）
- **02**：`rproc` 全字段、`rprocpub` 全字段、`rproc_ptr`、`rinit`、`rupdate`、`boot_image_priv/sys/dev` 三表、`r_flags` 全 16 位（`RS_IN_USE`~`RS_REINCARNATE`）、`sys_flags` 全 13 位（`SF_CORE_SRV`~`SF_NO_BIN_EXP`）、`lookup_slot_by_label/pid/dev_nr/domain/flags`、`alloc_slot`、`free_slot`、`rs_isokendpt`、`RS_SRV_IS_IDLE` 宏
- **03**：`priv` 结构全字段（kernel/priv.h）、boot priv 初始化（`static_priv_id`、`s_flags/s_init_flags/s_trap_mask/s_sig_mgr/s_bak_sig_mgr`、`fill_call_mask` kernel/VM 两侧、RS/VM 例外跳过 SET_SYS）、`sys_privctl` 全操作面（SET_SYS/ALLOW/DISALLOW/UPDATE_SYS/YIELD/SET_USER/CLEAR_IPC_REFS）、`sys_getpriv` 同步、`sched_init_proc`、`update_sig_mgrs`、`fill_call_mask`/`fill_send_mask`（utility.c）
- **04**：`check_call_permission`（root/control 双通道、SYS_PROC 限定、RUPDATE EBUSY、RS_LATEREPLY/RS_INITIALIZING EBUSY、RS_TERMINATED 例外、SF_CORE_SRV 禁 RS_DOWN）、`caller_is_root`、`caller_can_control`（isolation policy、`r_nr_control`/`r_control`）
- **05**：`get_next_name`（IPC 列表解析）、`add_forward_ipc`（SYSTEM/USER 例外 + proc_name 匹配）、`add_backward_ipc`（反向补位）、`init_privs`（IPC_ALL/IPC_ALL_SYS 全置位）、`fill_send_mask`
- **06**：主循环消息分类（`is_ipc_notify` → CLOCK/默认心跳）、`reply`（RS 自身免回）、`late_reply`（`RS_LATEREPLY`）、`rs_asynsend`（AMF_NOREPLY）、`rs_isokendpt`、`rs_is_idle`、`rs_idle_period`（RS_DEAD 清理 + SF_USE_REPL 补 replica + VM 单 replica 限制）、`EDONTREPLY` 协议、`sef_cb_signal_handler`（SIGCHLD→do_sigchld、SIGTERM→do_shutdown）、`sef_cb_signal_manager`（系统信号转发、终止信号→terminate_service、VM 免转发、SIGS_SIGNAL_RECEIVED）
- **07**：`do_period`（update 状态检查 + 全表周期检查）、`r_alive_tm/r_check_tm/r_stop_tm/r_backoff/r_period` 字段语义、`RS_DELTA_T/RS_INIT_T/MAX_BACKOFF`、backoff 递减→`restart_service`、SIGTERM 超时→`crash_service`、`ipc_notify` 心跳请求、free pass 逻辑、`do_sigchld`（waitpid 清理 + rupdate_clear_upds）、`update_period`（prepare 超时→end_update EINTR/RS_CANCEL）、`sys_setalarm` 重排
- **08**：`check_request`（scheduler/priority/quantum/cpu/sigmgr 参数验证、RS_CPU_BSP/DEFAULT）、`copy_rs_start`、`copy_label`、`init_slot`（DSRV_* 默认、uid/dev/domain/pci/devman 初始化）、`edit_slot`（ipc 列表、IRQ、I/O 范围、kernel/VM call mask、control labels、sigmgr、sched 参数、cmd/progname/label、**脚本 SF_USE_SCRIPT 仅非 core**、**RSS_COPY→read_exec/share_exec+SF_USE_COPY（含 RSS_REUSE 同名扫描）**、**RSS_REPLICA→SF_USE_REPL**、**RSS_NO_BIN_EXP→SF_NO_BIN_EXP**、**RSS_DETACH→SF_DET_RESTART**、**RSS_NORESTART→SF_NORESTART（core 服务 EPERM）**、**r_period（RS 自身除外）/r_restarts/r_asr_count 覆盖**、**init_privs 重跑**）、`build_cmd_dep`（argv 解析）、`inherit_service_defaults`（IMM_SF/IMM_F、trap mask）
- **09**：`srv_execve`（minix_stack_params/fill、sbrk frame）、`do_exec`（exec_info、exec_loaders、libexec_load_elf、libexec_pm_newexec、stack copy、exec_restart）、`exec_restart`（PM_EXEC_RESTART）、`read_seg`、`read_exec`（stat/open/read）、`share_exec`、`free_exec`（共享扫描）、`SF_USE_COPY/SF_NEED_COPY`
- **10**：`create_service`（srv_fork、getprocnr、privctl SET_SYS+GETPRIV、sched_init_proc、read_exec、srv_execve、setuid(0) hack、RS pin、VM MAKE_VM + 全实例 pin、vm_set_priv）、`clone_service`（replica 链接、RS 备份 sig mgr）、`activate_service`、`clone_slot`（浅拷贝+深拷贝语义）、`swap_slot`/`swap_slot_pointer`（四链+update 链+全局索引交换）
- **11**：`publish_service`（ds_publish_label、mapdriver、pci_set_acl、DEVMAN_BIND）、`unpublish_service`（ds_delete_label、pci_del_acl、DEVMAN_UNBIND）、`setuid(0)` VFS hack 注释
- **12**：`start_service`（编排：create+activate+publish+run，do_up/reincarnate 共用）、`run_service`（原语：privctl ALLOW + init_service）、`init_service`（RS_INIT 消息构造：type/flags/rproctab_gid/old_endpoint/restarts/buff_addr/prepare_state、ROOT_SYS_PROC 免发）、`do_init_ready`（非 update 分支：清 RS_INITIALIZING + reply + end_srv_init；update 分支标注→16）、`do_upd_ready`（校验 curr_rpupd、RS_PREPARE_DONE、失败→end_update；后续 start_update_prepare_next/start_update 标注→16）、`catch_boot_init_ready`（VM 异步例外）、`end_srv_init`（late_reply + prev replica 清理 + rupdate_upd_move + r_restarts++）、`sef_cb_init_response`/`sef_cb_lu_response`（RS 自模拟 ready）
- **13**：`do_up`（RSS_NOBLOCK/FORCE_INIT_* 标志、init_slot、重复检查 label/dev_nr/domain、start_service、RS_LATEREPLY）、`do_down`（stop_service + RS_LATEREPLY、RS_TERMINATED 直接 cleanup）、`do_restart`（仅 recovery script 场景）、`do_refresh`、`do_shutdown`（shutting_down + 全表 RS_EXITING）、`do_clone`/`do_unclone`（SF_USE_REPL）、`do_edit`（privctl UPDATE_SYS + vm_set_priv + sched 重初始化 + replica 重建）、`stop_service`（停止原语：SIGTERM/SIGHUP-for-RS + r_stop_tm 记录，被 do_down/do_refresh 共用）
- **14**：`do_lookup`（namebuf 拷贝 + label 查找）、`do_getsysinfo`（SI_PROC_TAB/PROCALL_TAB/PROCPUB_TAB 拷贝）、`do_sysctl`（SRV_STATUS/UPD_START/UPD_RUN/UPD_STOP/UPD_STATUS）、`do_fi`、`fi_service`（COMMON_REQ_FI_CTL）、`print_services_status`、`print_update_status`、`srv_upd_to_string`（update 描述串，print_update_status 使用）
- **15**：`terminate_service`（初始化失败分支：update rollback / SF_NO_BIN_EXP refresh / 否则 exiting；RUPDATE abort；SF_NORESTART → CLEANUP_DETACH/SCRIPT；core 服务死亡 _exit(1)；RS_EXITING 全清理 + reincarnate）、`restart_service`（脚本或 clone+update+run 路径）、`run_script`（fork+execle sh、reason/incarnation、脚本 priv 设置）、`reincarnate_service`（RS_REINCARNATE 恢复路径，仅 terminate_service 调用）、`kill_service`/`crash_service`（RS 自身 exit(1)）、`cleanup_service`（两段式：标记 RS_DEAD + disallow + late_reply；真清理 sched_stop + srv_kill + 脚本 + detach/free_slot）、`detach_service`（唯一 label 重发布 + 降权）、`get_service_instances`、`MAX_DET_RESTART`
- **16**：`do_update`（RSS_* LU 标志全表、prepare_only/self/force/batch、VM 默认 mmap prealloc、target label、prepare_state/maxtime、EBUSY/EINVAL 校验、clone/init_slot + inherit + create、信号管理器、heap/map prealloc、init_state_data、cpf grants、rupdate_add_upd、start_update_prepare、RS_LATEREPLY）、`rupdate_clear_upds/add_upd/set_new_upd_flags/upd_init/upd_clear/upd_move`、`request_prepare_update_service`、`srv_update`（vm_update 分支）、`update_service`（swap_slot + pid/endpoint 重排 + activate）、`rollback_service`（RS 特例 sys_whoami/vm_update ROLLBACK）、`start_update_prepare`/`start_update_prepare_next`（VM multi 预分配）、`start_update`（VM multi 时序、`rs_receive_ticks` 超时等待 VM 初始化，update.c:590）、`start_srv_update`/`complete_srv_update`（RS yield 特例）、`end_update`（prepare-only 取消、rev_iter、成功 end_srv_init、标志清理）、`end_srv_update`（surviving/exiting 选择、reply/cancel、cleanup/detach）、`end_update_curr/before_prepare/prepare_done/initializing/rev_iter`、`abort_update_proc`
- **17**：`init_state_data`（rs_ipc_filter_el 解析、ds_retrieve_label_endpt + ANY_* + strtol、VM 保底 RS 通信条目）、`rs_state_data`/`rs_ipc_filter_el` 结构、`cpf_grant_direct/cpf_revoke` 语义、`IPCF_*` 标志
- **18**：`sef_cb_init_restart`（SEF_CB_INIT_RESTART_STATEFUL、end_update 若进行中、update_service RS_DONTSWAP、init_service、alarm）、`sef_cb_init_lu`（SEF_CB_INIT_LU_DEFAULT、update_service、断言链）、boot 自升级流程细节（见 01）、RS 备份信号管理器、RS rollback 特例（`sys_whoami` + `vm_update(SF_VM_ROLLBACK)`）
- **19**：`sys_getinfo(GET_HZ)`、`sys_getimage`、`sys_getmachine`、`sys_privctl` 全操作、`sys_getpriv`、`sys_setalarm/sys_setalarm2`、`sys_kill`、`sys_datacopy`、`sys_statectl`、`sys_diagctl_stacktrace`、`sys_whoami`、`srv_fork/srv_kill/srv_execve`、`getnpid/getnuid/getprocnr`、`waitpid`、`vm_memctl(VM_RS_MEM_*)`、`vm_set_priv`、`vm_update/vm_prepare`、`ds_publish_label/ds_delete_label/ds_retrieve_label_endpt`、`sched_start/sched_stop`、`mapdriver`、`pci_set_acl/pci_del_acl`、`libexec_*`、`minix_stack_*`、DEVMAN_BIND/UNBIND 消息
- **99**：`const.h` 常量全表、`RS_RQ_BASE+0..24` 消息类型、`RS_SYSCTL_*`/`RS_FI_CRASH`、`RSS_*` 标志（rs.h）、`SF_*` 标志、`SEF_INIT_*/SEF_LU_*/SEF_CB_*`（sef.h）、`IPCF_*`、`SYS_PRIV_*`/`SYS_STATE_*`、endpoint 常量（`RS_PROC_NR` 等）、`init_strerror/lu_strerror` 错误表

### 5.4 明确排除 / 跳过的项

| 项 | 处理 | 依据 |
|----|------|------|
| `RS_USE_PAGING 0` | 死配置（RS 不使用分页特性），标注跳过 | const.h:84，全文件无引用 |
| `DEBUG`/`PRIV_DEBUG` | 调试宏 → `#[cfg(feature)]` 替代（A-11） | 设计差异 |
| `USE_PCI` 条件编译段 | minix-rs 无 PCI 驱动面 → defer + `PciAcl` 占位（A-10），fail-closed | 设计差异 + 缺口 |
| `srv_to_string_gen` 的 static 池（3 槽轮换） | Rust `Display` 替代，无共享可变静态 | 设计差异（A-12） |
| `_ASSERT_MSG_SIZE` 编译期断言 | Rust 结构体布局测试替代 | 设计差异 |
| `panic()` 路径 | 保留 fail-closed 语义；Rust 侧 `Errno` + panic 映射 | 语义保持（A-12） |
| `inc.h` 的 libc 头依赖面（stdio/stdlib/signal 等） | 归入 19 的 Rust 依赖面契约 | 边界声明 |
| `run_script` 的 `fork()+execle("sh")` | no_std 无 fork → A-1 设计决策（委托 INIT/受限执行器 或 defer） | 缺口标注 |
| `Makefile`/`.include bsd.own.mk` | 构建系统，非语义 | WONTFIX |

---

## 6. 实施步骤

> 文档写作顺序 = 编号顺序。每篇完成后按 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约自查。P0 修复完成后才可推进下一篇。`checklist.md`（函数级覆盖基线，格式参照 `02-stage-vm/checklist.md`）在 01~05 完成后建立，作为 §5.3 的机器可核对基线。
> **进度（2026-08-15）**：21 篇最小骨架（核心点 + 边界）已按 §2 编号全部创建，与 §5.3 函数级清单交叉核对通过（见 §7.2）；下一步按编号顺序逐篇扩写。

1. **00-rs-overview 新建**（导航按启动主线叙事，含 §2 启动时序图 + 服务生命周期次主线图）
2. **01-rs-boot-init 新建**（骨架：四步 boot 映射 02~18 锚点；SEF 回调注册表）
3. **02~05 按 boot Step 1 机制顺序写作**（进程表 → priv → 访问控制 → IPC 掩码）
4. **06~07 主循环与监控**（RS 运行时心脏 + 心跳状态机）
5. **08~11 服务创建机制**（slot 配置 → exec → 创建 → 发布）
6. **12~14 启动/初始化协议 + 控制/查询请求**（13 内绘制服务生命周期次主线路径图）
7. **15 终止与恢复状态机**（含终止/恢复状态机图）
8. **16~18 Live Update 全链 + RS 自身生命周期**
9. **19 外部接口契约**（Rust 依赖面：minix-sys 需补哪些 syslib、minix-types 需补哪些消息类型）+ **99 全局概念**
10. **checklist.md 建立** + `00-master-plan/README.md` 状态同步 + 与 `02-stage-vm/25-rs-services.md`（VM 侧 RS 服务）交叉核对

---

## 7. Review 记录

> 本节记录 plan.md 自身的 review 过程（深度 review + minix3 回归 review），与最终 plan.md 同文档交付，保证"覆盖完整性核对"可追溯。

### 7.1 深度 review（2026-08-15）

**范围**：语义全覆盖 + 语义模块合理拆分 + 叙事顺序无前向引用。

深度 review 结论与修复：

| # | 发现 | 等级 | 修复 |
|---|------|------|------|
| D-1 | 首版将 `check_request`（参数验证）与权限检查同放 04，语义混杂（一个是 rs_start 参数校验、一个是调用者权限） | P1 | `check_request` 移入 08-slot-config，04 只保留 check_call_permission/caller_is_root/caller_can_control |
| D-2 | 首版将 `do_init_ready`/`do_upd_ready` 与 `do_period` 同放"主循环"阶段，但 ready 消息与 update 状态机强耦合 | P1 | 拆分：07 只含 do_period/do_sigchld/update_period；12 独立为 ready 消息语义单元 |
| D-3 | `sef_cb_init_response`/`sef_cb_lu_response`（RS 自模拟 ready 消息，main.c:591-630）首版遗漏 | P1 | 补入 12 清单 |
| D-4 | `end_srv_init`（manager.c:328，late_reply + prev replica 清理）首版遗漏 | P1 | 补入 12 清单 |
| D-5 | 首版未区分 `catch_boot_init_ready` 的 VM 异步例外（VM 回复不 reply，防死锁） | P1 | 12 明确 VM 例外语义 |
| D-6 | `update.c` 的 5 个 static `end_update_*` 辅助函数（curr/before_prepare/prepare_done/initializing/rev_iter）首版只列了端到端函数 | P2 | 补入 16 清单 |
| D-7 | `srv_update`（update.c:230，VM 多组件时调 vm_update）首版遗漏 | P1 | 补入 16 清单 |
| D-8 | `inherit_service_defaults`（manager.c:1303，IMM_SF/IMM_F 不可变继承）首版遗漏 | P1 | 补入 08 清单 |
| D-9 | boot Step 1 中 RS/VM 跳过 `sys_privctl(SET_SYS)`、Step 2 中 RS/VM 走 `init_service` 的例外语义，首版时序图未标注 | P2 | 01 骨架 + 03 显式标注两处例外 |
| D-10 | `r_flags` 16 位中 `RS_CLEANUP_DETACH`/`RS_CLEANUP_SCRIPT`/`RS_REINCARNATE` 三位的语义（仅 15 使用）首版未归位 | P2 | 02 全表 + 15 语义展开 |
| D-11 | `fill_send_mask` 同时被 03（boot）与 05（init_privs）使用，首版只在 03 列出 | P2 | 03 定义、05 引用，引用规则补充 |
| D-12 | `setuid(0)` VFS 阻塞 hack（create_service/publish_service 注释）是外部行为的一部分（mapdriver 时序依赖），首版未归位 | P2 | 10/11 标注为"行为保持点"（Rust 侧需等效处理） |
| D-13 | `RS_SRV_IS_IDLE` 宏（rs_is_idle 的核心判定）首版遗漏 | P2 | 补入 02 清单 + 06 语义 |
| D-14 | 19 与 03 的 `sys_privctl` 操作面重复，首版未定边界 | P2 | 03 覆盖"priv 结构语义 + privctl 每操作的效果"，19 只列"外部函数签名与消息映射" |
| D-15 | 首版无 `checklist.md` 建立步骤，§5.3 的函数基线缺少落地载体 | P1 | §6 步骤 10 增加 checklist.md 建立（格式参照 02-stage-vm） |
| D-16 | `rinit.rproctab_gid` 的创建（cpf_grant_direct）在 boot 流程、消费在 init_service（RS_INIT 消息），首版只归 01 | P1 | 01 标注创建点、12 标注消费点 |
| D-17 | `start_service`/`run_service`/`stop_service`（manager.c:923-1012）首版未在 §5.3 归位，且 run_service→init_service 造成 10→12 前向引用 | P1 | 12 扩展为"服务启动与初始化协议"（start_service/run_service 归 12），stop_service 归 13；10 只保留创建机制 |
| D-18 | `reincarnate_service` 首版归 10，但其唯一调用点是 terminate_service（manager.c:1152），且调用 start_service 造成 10→12 前向引用 | P1 | 移入 15（终止与恢复），消除前向引用 |
| D-19 | `rs_receive_ticks`（utility.c:247）首版仅在 A-9 提及，§5.3 未归位（唯一调用点 update.c:590，VM 超时等待） | P1 | 补入 16 清单 |
| D-20 | `sef_cb_signal_handler`/`sef_cb_signal_manager`（main.c:631-708）首版遗漏 | P1 | 补入 06 清单（运行时信号处理） |
| D-21 | §2 表 03 行 C 源码列误含 `manager.c:2297-2332`（实为 `init_privs`，归 05）；§5.1 manager.c 映射随之含 03 | P2 | 03 行改为 `kernel/priv.h`+`main.c:250-330`+`utility.c:100-140,364-422`；§5.1 移除 03 |
| D-22 | `srv_upd_to_string`（print_update_status 使用）首版未归位 | P2 | 补入 14 清单 |
| D-23 | `12` 文档名 `ready-messages` 过窄（现含 start_service/run_service），且 `end_srv_init` 内部调用 `cleanup_service`（15）/`rupdate_upd_move`（16）、`terminate_service` 调用 `abort_update_proc`（16）、`update_period` 调用 `end_update`（16）三处单函数引用未声明豁免 | P1 | 12 更名 `12-rs-init-run.md`（服务启动与初始化协议）；§2.2 增补 4 处导航性引用豁免声明 |
| D-24 | `edit_slot` 尾部（manager.c:1618-1705）的 sys_flags 更新段（SF_USE_SCRIPT/SF_USE_COPY/RSS_REUSE 扫描/SF_USE_REPL/SF_NO_BIN_EXP/SF_DET_RESTART/SF_NORESTART+core EPERM）与 r_period/r_restarts/r_asr_count 覆盖、init_privs 重跑，首版 §5.3 08 清单遗漏 | P1 | 补入 08 清单（含 RSS_* → SF_* 映射与 core EPERM 例外） |
| D-25 | §2 表多行 C 源码行号区间不精确：06 误含 rs_receive_ticks（247-307，归 16）、07 的 do_sigchld 止于 1094 而非 1050、08 缺 build_cmd_dep（289-324）、12 缺 end_srv_init（328-356）且误含 stop_service（988-1012，归 13）、14 缺 fi_service（69-81）与 srv_upd_to_string（142-222）、03 缺 fill_send_mask（82-99） | P2 | 逐行修正 §2 表区间（见 D-21 同款处理） |

### 7.2 minix3 源码回归 review（2026-08-15）

**方法**：对 `minix3/minix/servers/rs/` 全部 8 个 .c + 头文件逐一 grep 核对 §5.1/§5.2 映射表，并抽查 §5.3 函数级清单。

**证据**：

```bash
ls minix3/minix/servers/rs/*.c          # 8 个文件，与 §5.1 表一致（6307 行）
rg -n '^(int|void|char\*|struct rproc|static int|static void) [a-z_]+\(|^[a-z_]+\(' \
  minix3/minix/servers/rs/{main,manager,request,update,utility,exec,error}.c   # 函数面核对
grep -cE '#define RS_(UP|DOWN|REFRESH|RESTART|SHUTDOWN|UPDATE|CLONE|UNCLONE|EDIT|SYSCTL|FI|GETSYSINFO|LOOKUP|INIT|LU_PREPARE)[[:space:]]' \
  minix3/minix/include/minix/com.h       # 15 个 IPC 消息类型全部进入分发表
rg -n 'mess_rs_|mess_lsys_pm_srv_fork|mess_lsys_getsysinfo|mess_lsys_fi_ctl' \
  minix3/minix/include/minix/ipc.h       # 消息槽核对（§5.2）
grep -cE '#define SF_(CORE_SRV|SYNCH_BOOT|NEED_COPY|USE_COPY|NEED_REPL|USE_REPL|VM_UPDATE|VM_ROLLBACK|VM_NOMMAP|USE_SCRIPT|DET_RESTART|NORESTART|NO_BIN_EXP)[[:space:]]' \
  minix3/minix/include/minix/rs.h        # 13 个 SF_* 标志全表
grep -cE '#define RS_(IN_USE|EXITING|REFRESHING|NOPINGREPLY|TERMINATED|LATEREPLY|INITIALIZING|UPDATING|PREPARE_DONE|INIT_DONE|INIT_PENDING|ACTIVE|DEAD|CLEANUP_DETACH|CLEANUP_SCRIPT|REINCARNATE)[[:space:]]' \
  minix3/minix/servers/rs/const.h        # 16 个 r_flags 全表
rg -n 'SEF_INIT_|SEF_LU_STATE_|SEF_LU_SELF|SEF_LU_ASR|SEF_LU_MULTI|SEF_LU_PREPARE_ONLY|SEF_LU_NOMMAP|SEF_LU_DETACHED|SEF_LU_INCLUDES' \
  minix3/minix/include/minix/sef.h       # SEF 标志面核对
```

**结论**：8 个 .c 文件全部映射到新文档，无遗漏；15 个 IPC 消息类型、16 个 `r_flags`、13 个 `SF_*`、`RSS_*`/`SEF_*` 标志面全部进入覆盖契约；ARCH 项（A-1~A-13）与 minix3 现状对照成立（`USE_LIVEUPDATE`/`USE_PCI` 在 `share/mk/bsd.own.mk:1499` 默认 yes，LU 按核心语义处理、PCI 标注 defer）。**覆盖完整性通过**。

**回归 review 发现与修复**：

| # | 发现 | 等级 | 修复 |
|---|------|------|------|
| R-1 | 计划引用行数 6808 与实际不符（`wc -l` 实测 6307） | P1 | 全文修正为 6307 |
| R-2 | `r_flags` 声称 15 位，实测 const.h 为 16 位（含 `RS_CLEANUP_DETACH/RS_CLEANUP_SCRIPT/RS_REINCARNATE`） | P1 | §5.3/§7.1 D-10/§7.2 结论统一为 16 |
| R-3 | `SF_*` 声称 12 位，实测 rs.h 为 13 位 | P1 | 统一为 13 |
| R-4 | 证据命令对 15 个 IPC 消息类型的 grep 会命中 `RS_SYSCTL_*` 等子功能宏（粗匹配 23 处（含 RS_RQ_BASE/RS_PROC_NR 与 RS_SYSCTL_*/RS_FI_CRASH 子功能宏）），不可作为精确证据 | P2 | 改为 `grep -cE '#define RS_(UP|...|LU_PREPARE)[[:space:]]'` 精确匹配（实测 15） |
| R-5 | §5.3 逐函数核对（`rg` 函数面 + 分节统计：main 12 / manager 45 / request 18 / update 24 / utility 17 / exec 2 / error 3 / table 0，共 121 个函数）确认全部归位，无遗漏 | — | 覆盖完整性通过（见上） |
| R-6 | §1.1 引用 `proc.h:279` 无法在源码复现（`kernel/proc.h` 无 RS 引用；`RS_PROC_NR` 实定义于 `include/minix/com.h:61`、`ROOT_SYS_PROC_NR=RS_PROC_NR` 于 `:77`、boot 映像顺序注释与条目在 `kernel/table.c:36-48`） | P1 | §1.1 引用修正为 com.h:61,77 + kernel/table.c:36-48 |
| R-7 | §5.2 ipc.h 消息槽 `mess_lsys_vfs_ln` 不存在（RS 经 `mapdriver()` 实际使用 `mess_lsys_vfs_mapdriver`，ipc.h:1463-1474） | P1 | §5.2 修正为 `mess_lsys_vfs_mapdriver` |
| R-8 | §2 表 10 行 C 源码区间缺 `activate_service`（manager.c:1013-1033，夹在 13 的 stop_service 988-1012 与 15 的 reincarnate_service 1033 之间） | P2 | §2 表 10 行补 `1013-1033` |
| R-9 | R-4 声称粗匹配 24 处，实测 `grep -cE '#define RS_' com.h` 为 23 处（15 主类型 + RS_RQ_BASE + RS_PROC_NR + 5×RS_SYSCTL_* + RS_FI_CRASH） | P2 | R-4 行修正为 23 处并注明构成 |

---

## 8. 参见

- `draft/` — 占位素材（README.md）
- `../02-stage-vm/plan.md` — 同流程先例（文档重组计划 + review 记录结构）
- `../02-stage-vm/25-rs-services.md` — VM 侧 RS 服务（SET_PRIV/PREPARE/UPDATE/MEMCTL），需与本 stage 双向核对
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则/过渡章节）
- `../01-stage-kernel/22-privilege.md`、`23-ipc-filter.md` — kernel 侧 priv/IPC filter 语义（RS 是主要消费者）
- `../00-master-plan/README.md` — 目录重排与新主线说明
- `minix3/minix/servers/rs/` — C 源码（ground truth）
- `minix3/minix/include/minix/rs.h`、`sef.h`、`ipc_filter.h`、`com.h`（RS 段）— 协议定义
- `os/servers/rs/` — Rust 实现（当前 stub，文档写作后按 plan 实现）
