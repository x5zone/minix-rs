# 08-stage-is 文档重组计划（plan.md）

> **状态**: 定稿（2026-08-16 首版 + 深度 review + minix3 源码回归 review，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/08-stage-is/`
> **目标**: 以 **IS server 启动顺序为主线**定义 IS 全部文档；功能键按压→转储输出数据流为次主线；最终覆盖 Minix3 IS server（`servers/is/`，8 个 .c，1151 行）+ 协议面（`com.h`/`ipc.h`/`keymap.h`/`sysutil.h`/`sysinfo.h` + `libsys` 客户端）+ 跨服务数据面（kernel `sys_getinfo`/`DIAGCTL`/kerninfo、PM/VFS/RS/DS `getsysinfo`、VM `vm_info`）全部语义，支撑 IS server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/` 与 `07-stage-ds/`（同流程先例）、`minix3/minix/servers/is/`（ground truth）、`os/servers/is/`（Rust 实现，当前为 stub）

---

## 1. 背景与动机

### 1.1 现状问题

`08-stage-is/` 目录自 2026-08-13 创建以来仅有**占位 README**（2026-08-16 移入 `draft/`），没有任何正式文档。历史逐行讲解素材（`tmp/is/` 8 个 `tmp_*.md`，2026-08-14 产出）已一并移入 `draft/` 作素材。现状与 IS 的语义定位不匹配：

1. **语义小而窄，但协议面横跨 5 组数据面**——`servers/is/` 仅 1151 行 C（8 个 .c），是用户态服务中较小的一个，但 IS 本质是**调试转储聚合器**：kernel（`sys_getinfo`/`SYS_DIAGCTL`/kerninfo）、TTY（fkey 协议）、VM（`VM_INFO`）、PM/VFS/RS/DS（`getsysinfo` 布局 ABI）各是一组独立数据面。文档必须一次到位地把"服务器内部语义"+"5 条数据获取通道（sys_getinfo / sys_diagctl / kerninfo / getsysinfo / vm_info）"+"6 个数据布局契约（kernel/PM/VFS/RS/DS/VM）"讲清。
2. **无固定 endpoint、无 boot_image 登记**——IS 不在 kernel `boot_image`（`kernel/table.c:44-64` 无 `is` 条目），由 RS 运行时加载（`etc/rc.minix:117` `up -n is -period 5HZ`，仅 `sysenv debug_fkeys != 0` 时启动），endpoint 由 RS 动态分配（全 minix3 无 `IS_PROC_NR` 定义，grep 实证见 §5.4）。这是与 DS/VM/PM 等固定 endpoint 服务的关键差异。
3. **与 07-stage-ds 相同**——无旧主线文档可迁移（只有占位 README），本计划从零定义文档集，`draft/` 仅保留占位与素材；§5 覆盖契约是后续写作的**唯一权威基线**，必须一次到位。

### 1.2 新主线：IS server 启动顺序 + 运行时主循环

与 `01-stage-kernel` / `02-stage-vm` / `07-stage-ds` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。IS 的启动与运行是严格线性的：

```
启动条件（etc/rc.minix:117：仅 sysenv debug_fkeys != 0）
  → minix-service up is → RS 运行时加载 ELF + 动态分配 endpoint
  → system.conf:271-277 权限面：service is { vm INFO; uid 0; }（仅 VM_INFO 系统调用 + root）
  │
  ▼  main.c:31  main()
  ├─ env_setargs(argc, argv)                  ← 01：启动参数
  ├─ sef_local_startup()                      ← 01：SEF 生命周期
  │    ├─ sef_setcb_init_fresh(sef_cb_init_fresh)    ← 01：init 回调
  │    ├─ sef_setcb_init_lu(sef_cb_init_fresh)       ← 01：Live Update 复用同一回调
  │    ├─ sef_setcb_init_restart(sef_cb_init_fresh)  ← 01：重启复用同一回调（STATELESS）
  │    └─ sef_setcb_signal_handler(sef_cb_signal_handler) ← 01：SIGTERM 清理
  ▼  sef_cb_init_fresh()（main.c:94，boot 锚点）
  ├─ map_unmap_fkeys(TRUE)                    ← 02：向 TTY 注册 F1-F12/SF1-SF12 观察者
  │    （fkey_map → TTY_FKEY_CONTROL → TTY do_fkey_ctl 登记 fkey_obs[]）
  │
  ▼  main.c:44-69  主循环（运行时）
  ├─ get_work() → sef_receive(ANY)            ← 01：收消息（callnr/who_e）
  ├─ is_notify(callnr)？
  │   ├─ _ENDPOINT_P(who_e) == TTY_PROC_NR → do_fkey_pressed ← 03：转储分派（次主线入口）
  │   └─ default → EDONTREPLY                ← 01
  ├─ else → 打印告警 + EDONTREPLY            ← 01
  └─ reply()（EDONTREPLY 除外）               ← 01
```

**每篇文档必须能回答一个问题：它位于 IS 启动时序（sef_cb_init_fresh）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 次主线：一次功能键按压的旅程

IS 的全部工作本质是"功能键 → 转储输出"。次主线以一次 F-key 按压的旅程贯穿阶段 2~5，其路径图在 `03-is-dump-dispatch` 内部绘制：

```
用户按下 F1（键盘中断）
  ├─ 02 TTY 侧：func_key() → fkey_obs 查观察者 → ipc_notify(IS endpoint)
  ├─ 01 IS 主循环：is_notify → who_e==TTY_PROC_NR → do_fkey_pressed
  ├─ 03 分派：fkey_events() 拉取位图 → hooks 表匹配 → 调用对应 dump 函数
  ├─ 04 数据面：sys_getinfo / getsysinfo / vm_info / kerninfo 获取数据拷贝
  └─ 05~10 格式化：按目标服务器布局解释 + printf 输出（22 行分页）
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。`draft/` 保留旧占位与逐行素材（旧编号 `tmp_*.md` 不改名，作素材引用）。

### 阶段总览（12 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | draft 素材 | 变更 |
|------|------|------|---------|--------|-----------|-----------|------|
| 0 总览 | 00 | `00-is-overview.md` | IS 是什么、启动主线图、无 boot_image 语义、文档导航、设计原则 | `servers/is/` 全部 | `os/servers/is/` 全部 | `draft/README.md` | **重写**为导航（原占位信息并入） |
| 1 启动入口 | 01 | `01-is-init-main.md` | `main`/`sef_local_startup`/`sef_cb_init_fresh`/`sef_cb_signal_handler`/`get_work`/`reply`、主循环骨架、notify 拒绝、SEF ping 拦截、EDONTREPLY、panic 路径 | `main.c`（148 行）、`libsys/sef.c:150,208-214`、`libsys/sef_ping.c:21` | `main.rs`、`lib.rs`、`sef.rs` | `draft/tmp_main.c.md` | 沿用 |
| 2 协议面 | 02 | `02-is-fkey-contract.md` | FKEY_MAP/UNMAP/EVENTS、`TTY_FKEY_CONTROL`、`mess_lsys_tty_fkey_ctl`/`mess_tty_lsys_fkey_ctl`、F1~SF12 常量、`map_unmap_fkeys`、`fkey_ctl` 客户端、TTY 侧 `do_fkey_ctl`/`func_key`/`ipc_notify` 契约 | `com.h:874-877`、`ipc.h:1453,1930`、`keymap.h`、`sysutil.h:43-46`、`libsys/fkey_ctl.c`、`drivers/tty/tty/arch/i386/keyboard.c:401-585` | `minix-types`（**缺 TtyFkeyCtlReq/Reply，A-1**）、`tty_fkey.rs` | `draft/tmp_dmp.c.md`（map_unmap_fkeys 素材） | **新增**：协议面 |
| 3 转储分派 | 03 | `03-is-dump-dispatch.md` | hooks 表（16 项）、`do_fkey_pressed`、`pressed` 宏、`fkey_events` 消费、`key_name`、`mapping_dmp`、**次主线路径图** | `dmp.c:17-131` | `dispatch.rs` | `draft/tmp_dmp.c.md` | 沿用 |
| 4 数据面机制 | 04 | `04-is-data-acquisition.md` | 5 条数据获取通道：`sys_getinfo(GET_*)`、`sys_diagctl(STACKTRACE)`、`get_minix_kerninfo→kmessages`（A-3）、`getsysinfo(SI_*)`（跨服务 IPC + size 精确匹配 + sys_datacopy）、`vm_info_*(VMIW_*)`；消息格式与错误面 | `com.h:252,316-331,412-415,476,507,729-734`、`sysinfo.h:11-17`、`libsys/getsysinfo.c`、`libsys/vm_info.c`、`libsys/sys_diagctl.c`、`callnr.h`（PM_BASE+47/VFS_BASE+48） | `minix-sys`（sys_getinfo/getsysinfo/vm_info 客户端；客户端库归属参照 `07-stage-ds` A-8）、`minix-types` | 无（机制面无旧素材） | **新增**：所有转储域篇的前置数据面文档 |
| 5 转储域 | 05 | `05-is-dump-kernel.md` | `dmp_kernel.c` 全部 8 函数 + 4 helper + `proc_name`、kernel 布局 ABI（`proc`/`priv`/`boot_image`/`kinfo`/`machine`/`kmessages`）、PROCLOOP/PRINTRTS 宏 | `dmp_kernel.c`（396 行） | `dump_kernel.rs` | `draft/tmp_dmp_kernel.c.md` | 沿用 |
| 5 | 06 | `06-is-dump-pm.md` | `mproc_dmp`/`sigaction_dmp`/`flags_str`、`SI_PROC_TAB`、mproc 布局 ABI、getticks 时钟面 | `dmp_pm.c`（109 行） | `dump_pm.rs` | `draft/tmp_dmp_pm.c.md` | 沿用 |
| 5 | 07 | `07-is-dump-vfs.md` | `fproc_dmp`/`dtab_dmp`、`SI_PROC_TAB`/`SI_DMAP_TAB`、fproc/dmap 布局 ABI、FP_* 位语义 | `dmp_fs.c`（83 行） | `dump_vfs.rs` | `draft/tmp_dmp_fs.c.md` | 沿用 |
| 5 | 08 | `08-is-dump-rs.md` | `rproc_dmp`/`s_flags_str`、`SI_PROCPUB_TAB`/`SI_PROC_TAB`、rprocpub/rproc 布局 ABI、RS_* 位语义 | `dmp_rs.c`（74 行） | `dump_rs.rs` | `draft/tmp_dmp_rs.c.md` | 沿用 |
| 5 | 09 | `09-is-dump-ds.md` | `data_store_dmp`、`SI_DATA_STORE`、data_store 布局 ABI（DS A-10 消费者契约） | `dmp_ds.c`（52 行） | `dump_ds.rs` | `draft/tmp_dmp_ds.c.md` | 沿用 |
| 5 | 10 | `10-is-dump-vm.md` | `vm_dmp`/`print_region`、`vm_info_stats/usage/region` 批处理状态机、分页游标（prev_i/prev_base）、连续 region 折叠 | `dmp_vm.c`（157 行） | `dump_vm.rs` | `draft/tmp_dmp_vm.c.md` | 沿用 |
| 99 全局概念 | 99 | `99-is-global-concepts.md` | TTY_PROC_NR、F-key/GET_*/SI_*/VMIW_*/DIAGCTL_CODE_* 常量表、错误码汇总、单线程事件循环执行模型、跨服务引用 | `com.h`、`sysinfo.h`、`keymap.h` | `minix-types` | `draft/README.md` | 沿用 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序中的位置与下一阶段的入口：

```
01（启动骨架）→ 02（fkey 协议面：init 锚点 map_unmap_fkeys）
→ 03（转储分派：主循环 notify 入口，次主线路径图）
→ 04（数据面机制：5 条数据获取通道前置）
→ 05~10（6 个转储域：按 hooks 表顺序展开，每篇一个外部布局契约）
→ 99（全局概念收口）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `03-kmain-cstart.md`、`04-platform-discovery.md` 等）：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明（基线见 §3.5）
6. **过渡**——在启动时序/主循环中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：协议面 02 先于分派 03，数据面 04 先于转储域 05~10）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 sef_cb_init_fresh / 主循环 dispatch 的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `03-is-dump-dispatch.md` 次主线路径图 → `02-is-fkey-contract.md`/`04-is-data-acquisition.md`）
- 与 kernel 文档交叉引用用 `../01-stage-kernel/NN-*.md`（IPC 原语 → `12-ipc-core.md`/`13-syscall-dispatch.md`；`sys_getinfo` → `25-misc-unported.md`；usermapped/kerninfo → `28-usermapped-data.md`；栈回溯 → `32-stack-tracing.md`；SYS_DIAGCTL 接线 → `08-system-init-boot-finish.md`）
- 与 VM/RS/PM/VFS/DS 文档交叉引用用 `../02-stage-vm/`、`../03-stage-rs/`、`../04-stage-pm/`、`../05-stage-vfs/`、`../07-stage-ds/NN-*.md`（VM INFO → `26-vm-queries.md`；DS 布局 → `03-ds-data-structures.md`/`11-ds-getsysinfo.md`）
- 对 draft 素材的引用一律指向 `draft/`，并标注"素材"

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、启动主线图、无 boot_image + RS 运行时加载语义、文档导航、设计原则 | 一切机制细节（01~10、99） |
| 01 | 00 + kernel `12-ipc-core`/`09-vm-boot-protocol` | `main`/`sef_local_startup`（4 个 SEF 注册）/`sef_cb_init_fresh`（boot 锚点）/`sef_cb_signal_handler`/`get_work`/`reply`、主循环分类骨架、notify 拒绝（TTY 例外）、SEF ping 存活检查（sef_receive 拦截，主循环不感知）、EDONTREPLY、panic 路径 | fkey 注册实现（02）、转储分派（03） |
| 02 | 01 + TTY `drivers/tty` + kernel `13-syscall-dispatch` | FKEY 三命令、`TTY_FKEY_CONTROL` 消息格式（请求/回复字段）、F1~SF12 常量、`map_unmap_fkeys`（IS 侧注册逻辑）、`fkey_ctl` 客户端（grant 无、直接消息）、TTY 侧 `do_fkey_ctl` 状态机（覆盖登记/EPERM/位图消费）+ `func_key`/`ipc_notify` 通知语义 | IS 侧分派（03）、dump 内容（05~10） |
| 03 | 02 | hooks 表（16 项 key→函数→描述）、`do_fkey_pressed`、`pressed` 宏、`fkey_events` 消费、`key_name`、`mapping_dmp`、**次主线路径图** | 各 dump 函数体（05~10）、fkey 协议（02） |
| 04 | 01 + kernel `25-misc-unported`/`28-usermapped-data`/`32-stack-tracing` + 各服务阶段 | 5 条数据获取通道：`sys_getinfo(GET_*)`（消息格式 + 拷贝语义）、`sys_diagctl_stacktrace`（DIAGCTL_CODE_STACKTRACE）、`get_minix_kerninfo→kmessages`（A-3 usermapped 移除）、`getsysinfo(SI_*)`（4 服务器 callnr + size 精确匹配 + sys_datacopy + root 检查）、`vm_info_*(VMIW_*)`；错误面（告警不 panic） | 各服务器布局解释（05~10）、输出格式化与分页 |
| 05 | 04 + kernel 布局头文件 | `dmp_kernel.c` 全部：proctab/procstack/privileges/image/irqtab/kmessages/monparams/kenv 8 函数 + `s_flags_str`/`s_traps_str`/`p_rts_flags_str`/`proc_name` + PROCLOOP/PRINTRTS 宏 + kernel 布局 ABI（`proc`/`priv`/`boot_image`/`kinfo`/`machine`/`kmessages`） | PM/VFS/RS/DS/VM 数据面（06~10） |
| 06 | 04 + PM `mproc` 布局（`04-stage-pm`） | `mproc_dmp`/`sigaction_dmp`/`flags_str`、`SI_PROC_TAB`、mproc 布局 ABI、`getticks` 时钟面（alarm 剩余时间） | PM 服务器内部语义（`04-stage-pm`） |
| 07 | 04 + VFS 布局（`05-stage-vfs`） | `fproc_dmp`/`dtab_dmp`、`SI_PROC_TAB`/`SI_DMAP_TAB`、fproc/dmap 布局 ABI、FP_SESLDR/FP_REVIVED/FP_BLOCKED_ON_* 位语义 | VFS 服务器内部语义（`05-stage-vfs`） |
| 08 | 04 + RS 布局（`03-stage-rs`） | `rproc_dmp`/`s_flags_str`、`SI_PROCPUB_TAB`/`SI_PROC_TAB`、rprocpub/rproc 布局 ABI、RS_IN_USE/RS_ACTIVE/RS_UPDATING/... 位语义 | RS 服务器内部语义（`03-stage-rs`） |
| 09 | 04 + DS 布局（`07-stage-ds`） | `data_store_dmp`、`SI_DATA_STORE`、data_store 布局 ABI（**DS A-10 消费者契约**）、DSF_IN_USE/DSF_MASK_TYPE/DSF_TYPE_* 位语义、静态 prev_i 环形翻页游标（跳过未用槽位 + 到尾回绕） | DS 服务器内部语义（`07-stage-ds`） |
| 10 | 04 + VM_INFO 协议（`02-stage-vm` `26-vm-queries`） | `vm_dmp`/`print_region`、`vm_info_stats/usage/region` 批处理状态机（prev_i/prev_base/首屏 header/连续 region 折叠/LINES 边界）、输出分页游标 | VM 服务器内部语义（`02-stage-vm`） |
| 99 | 全部 | TTY_PROC_NR、F-key/GET_*/SI_*/VMIW_*/DIAGCTL_CODE_* 常量表、错误码汇总（EDONTREPLY/EPERM/EINVAL/ENOSYS）、单线程事件循环执行模型声明、跨服务引用（TTY/VM/PM/VFS/RS/DS/kernel） | 各机制细节 |

### 3.5 测试基线

> `os/servers/is/` 当前为 stub（`lib.rs: pub fn init() {}`，`main.rs: minix_is::init(); loop {}`）。`cargo check -p minix-is` 通过（2026-08-16），**无任何测试**。每篇文档 §测试 的"测试总数"声明以此为基线。minix3 侧亦无 IS 专项测试（`minix/tests/` 无 `is` 用例，调试转储无回归测试面；行为契约以 §5.3 为准）。

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 08-stage-is 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **fkey 协议消息类型** | `mess_lsys_tty_fkey_ctl`/`mess_tty_lsys_fkey_ctl`（`ipc.h:1453,1930`，字段全 int 位图，32 位布局无歧义） | `minix-types` **尚无 TtyFkeyCtlReq/Reply 类型**，需新增（request/fkeys/sfkeys 三字段 + 回复位图写回） | 02 | **缺口**：minix-types 新增类型 |
| A-2 | **FKEY 通知消息体** | notify 消息**无 payload**，IS 主循环只读 `_ENDPOINT_P(who_e)`（`main.c:44-48`），消息体未使用 | `minix-types::Notify`（`ipc/notify.rs` 已存在）直接复用；通知语义 = 拉模式（FKEY_EVENTS 消费位图） | 01/02 | 可复用 |
| A-3 | **`.usermapped` 段移除（kmessages 通道）** | `kmessages_dmp` 用 `get_minix_kerninfo()->kmessages`（`type.h:214-232`，usermapped 映射直读，**无 GET_KMESSAGES syscall**，grep 实证） | minix-rs 64-bit **不保留 `.usermapped` 段**（`28-usermapped-data.md` 已文档化，数据改走 `sys_getinfo`）→ 需新增 `GET_KMESSAGES` 子请求或等价机制，三处一致标注 | 04/05 | **缺口**：需显式机制设计 |
| A-4 | **跨服务器布局 ABI（6 个数据源）** | IS 按对方内存布局**原样解释**：kernel `proc`/`priv`/`boot_image`/`kinfo`/`machine`/`kmessages`、PM `mproc`、VFS `fproc`/`dmap`、RS `rprocpub`/`rproc`、DS `data_store`、VM `vm_stats_info`/`vm_usage_info`/`vm_region_info`（`getsysinfo`/`sys_getinfo`/`VM_INFO` 原样拷贝） | Rust 侧：各结构 `#[repr(C)]` 等价布局则兼容不变；否则**声明 ARCH 偏离 + 各服务器同步改造**（IS 是 debug 转储，可随重写演进）；两方案都要三处一致标注。DS 侧先例：`07-stage-ds` A-10 | 04/05~10 | **待决策**（兼容 vs 偏离） |
| A-5 | **分页输出语义** | 22 行/屏（`LINES=22`，VM 用 24）+ `--more--\r` + 静态游标 `prev_i`（`dmp_pm.c:30,69` 等）实现连续翻页 | 转储输出重定向后（日志/串口）分页语义弱化：**保留行为契约**（输出可逐屏消费）还是简化（一次性输出全部）待决策 | 05~10 | 待决策 |
| A-6 | **printf/诊断输出路径** | IS 的 `printf` → libc stdio → 服务 stdout（log 驱动，`etc/usr/rc:290` `up log -dev /dev/klog`） | minix-rs `no_std`：诊断输出机制（内核 diag 通道 / log 驱动 IPC / panic handler）待设计；所有 dump 输出依赖此通道 | 01/03/05~10 | 待设计 |
| A-7 | **proctab_dmp arm 分支** | `dmp_kernel.c:349` `#if defined(__arm__)` 空实现 | x86-64 目标：**排除**，标注不移植 | 05 | 排除 |
| A-8 | **死代码** | `click_to_round_k`（`dmp_kernel.c:42`，定义未使用）；`glo.h` 的 `diag_buf`/`diag_next`/`diag_size`/`sys_panic`/`dont_reply`（extern 声明，全树**无定义无使用**，grep 实证） | 不实现，标注排除 | 05/99 | 排除（grep 实证，见 §5.4） |
| A-9 | **无固定 endpoint** | 全 minix3 **无 `IS_PROC_NR` 定义**（grep 实证）；endpoint 由 RS 运行时分配；`system.conf:271-277` `service is { vm INFO; uid 0; }` 权限面仅 VM_INFO | Rust 侧：不定义 `IsProcNr` 常量；endpoint 作为运行时参数注入（参照 RS 动态加载模型）；权限面 = `VmInfo` 调用白名单 | 00/01/99 | 已具备（先例见 RS） |
| A-10 | **SEF 生命周期** | `sef_cb_init_fresh` 三合一（init_fresh/init_lu/init_restart 同一回调）+ `sef_cb_signal_handler`（SIGTERM → `map_unmap_fkeys(FALSE)` + `exit(0)`）；STATELESS（重启重建 fkey 映射） | 参照 `03-stage-rs` 的 `sef.rs` 显式生命周期；signal handler 注册为 SEF 回调 | 01 | 待实施（依赖 SEF 落地） |
| A-11 | **启动条件与存活检查** | `etc/rc.minix:117` `up -n is -period 5HZ`（`-n` = OPT_NOBLOCK 立即返回调用者，`minix-service.c:87`；`-period 5HZ` = RS 每 5 秒 ping 存活检查），仅 `sysenv debug_fkeys != 0` 时执行；ping 为 NOTIFY 消息，由 `sef_receive` 透明拦截（`sef.h:121-122`、`sef_ping.c:21`） | minix-rs 启动配置同语义：IS 为**条件性 debug 服务**，不参与核心启动因果链；SEF ping 存活应答随 SEF 层落地 | 00/01/99 | 已具备（依赖 SEF） |
| A-12 | **标志字符串编码** | `p_rts_flags_str`/`s_flags_str`/`s_traps_str`/`flags_str`（`dmp_kernel.c:218,236,300`、`dmp_pm.c:21`、`dmp_rs.c:61`）位→字符 固定编码（行为契约） | Rust 用 `Display`/格式化 trait 表达相同输出；位掩码 → `BitFlags` 类型 | 05/06/08 | 待设计 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

### 5.1 C 源文件 → 新文档映射（8 个 .c + 3 个 .h）

| C 源文件 | 行数 | 覆盖文档 | 核对 |
|---------|------|---------|------|
| `servers/is/main.c` | 148 | 01（全部）、02（init 调用点） | 已核对 |
| `servers/is/dmp.c` | 132 | 02（map_unmap_fkeys）、03（hooks/do_fkey_pressed/key_name/mapping_dmp） | 已核对 |
| `servers/is/dmp_kernel.c` | 396 | 05（全部 8 函数 + 4 helper + proc_name） | 已核对 |
| `servers/is/dmp_pm.c` | 109 | 06（全部） | 已核对 |
| `servers/is/dmp_fs.c` | 83 | 07（全部） | 已核对 |
| `servers/is/dmp_rs.c` | 74 | 08（全部） | 已核对 |
| `servers/is/dmp_ds.c` | 52 | 09（全部） | 已核对 |
| `servers/is/dmp_vm.c` | 157 | 10（全部） | 已核对 |
| `servers/is/inc.h` | 32 | 01（包含面，无独立语义） | 已核对 |
| `servers/is/glo.h` | 16 | 99 + §5.4 排除（死 extern） | 已核对 |
| `servers/is/proto.h` | 34 | 各函数归位各篇 | 已核对 |

> 合计 1151 行 .c（与占位 README 声明一致）。

### 5.2 头文件覆盖

| 头文件 | 覆盖文档 | 核对 |
|--------|---------|------|
| `include/minix/com.h`（is_notify:93、TTY_FKEY_CONTROL:874、FKEY_*:875-877、SYS_GETINFO 字段 GET_*:316-331、SYS_DIAGCTL:252、DIAGCTL_CODE_*:412-415、RS_GETSYSINFO:476、DS_GETSYSINFO:507、VM_INFO:729、VMIW_*:732-734） | 02、04、99 | 已核对 |
| `include/minix/ipc.h`（mess_lsys_tty_fkey_ctl:1453、mess_tty_lsys_fkey_ctl:1930） | 02 | 已核对 |
| `include/minix/keymap.h`（F1:93、F12:104、SF1:135、SF12:146） | 02、99 | 已核对 |
| `include/minix/sysutil.h`（fkey_map/unmap/events:43-45、fkey_ctl:46）、`include/minix/syslib.h`（sys_diagctl_stacktrace:166-168、sys_getmonparams:187） | 02、04 | 已核对 |
| `include/minix/sysinfo.h`（SI_*:11-17） | 04、99 | 已核对 |
| `include/minix/callnr.h`（PM_GETSYSINFO=PM_BASE+47、VFS 侧 =VFS_BASE+48） | 04 | 已核对 |
| `include/minix/type.h`（struct minix_kerninfo:214-232） | 04、05 | 已核对 |
| `include/minix/sef.h`（INTERCEPT_SEF_PING_REQUESTS:121、SEF_PING_REQUEST_TYPE=NOTIFY_MESSAGE:122、init/signal 回调注册） | 01、02 | 已核对 |
| `libsys/fkey_ctl.c`（客户端） | 02 | 已核对 |
| `libsys/getsysinfo.c`（客户端） | 04 | 已核对 |
| `libsys/vm_info.c`（客户端） | 04、10 | 已核对 |
| `libsys/sys_diagctl.c`（sys_diagctl 实现，stacktrace 宏在 syslib.h） | 04、05 | 已核对 |
| `drivers/tty/tty/arch/i386/keyboard.c`（do_fkey_ctl:429-527、func_key:532-585、kb_init_once 观察者数组:401-415、debug_fkeys:78,206,406） | 02（协议对方） | 已核对 |

### 5.3 语义模块覆盖清单（函数级）

> 以下为跨文件的关键语义，防止"文件有映射但函数漏掉"。写作时以 `draft/` 逐行素材 + 本清单双重核对。

- **01**：`main`（主循环骨架、is_notify 拒绝 + TTY 例外、default 告警 + EDONTREPLY）、`sef_local_startup`（4 个注册 + sef_startup）、`sef_cb_init_fresh`（boot 锚点：map_unmap_fkeys(TRUE)）、`sef_cb_signal_handler`（SIGTERM → unmap + exit(0)）、`get_work`（sef_receive + who_e/callnr + panic）、`reply`（ipc_send + panic）、全局 `m_in`/`m_out`/`who_e`/`callnr` 单线程状态、启动条件（rc.minix:117 `up -n is -period 5HZ`、system.conf:271-277）、SEF ping 存活检查（sef.c:150,208-214 拦截 + sef_ping.c:21 do_sef_ping_request，NOTIFY 消息，IS 主循环不感知）
- **02**：`map_unmap_fkeys`（dmp.c:46：bit_set 组装 fkeys/sfkeys → fkey_map/fkey_unmap → 失败告警）、`FKEY_MAP`/`FKEY_UNMAP`/`FKEY_EVENTS`（com.h:875-877）、`TTY_FKEY_CONTROL`（com.h:874）、`mess_lsys_tty_fkey_ctl`（request/fkeys/sfkeys，ipc.h:1453）、`mess_tty_lsys_fkey_ctl`（回复位图写回，ipc.h:1930）、`fkey_ctl` 客户端（libsys/fkey_ctl.c：_taskcall(TTY_PROC_NR, TTY_FKEY_CONTROL) + 位图写回）、TTY 侧 `do_fkey_ctl`（keyboard.c:429-527：FKEY_MAP 覆盖登记无 EBUSY（DEAD_CODE 段）、FKEY_UNMAP owner 检查 EPERM、FKEY_EVENTS 消费 events 位图）、`func_key`（keyboard.c:532-585：events++ → ipc_notify）、`kb_init_once` 观察者数组（keyboard.c:401-415）、`debug_fkeys` 开关（keyboard.c:78,206,406：TTY 侧条件拦截）、F1/F12/SF1/SF12（keymap.h:93,104,135,146）
- **03**：hooks 表（dmp.c:17-36：16 项 key→function→name）、`NHOOKS`、`do_fkey_pressed`（dmp.c:73：fkey_events 拉取 → 双位图匹配 → 逐项调用）、`pressed` 宏（dmp.c:70-72）、`key_name`（dmp.c:103）、`mapping_dmp`（dmp.c:120：打印键映射表）
- **04**：`sys_getinfo(GET_*)`（com.h:316-331：GET_KINFO/IMAGE/PROCTAB/MONPARAMS/IRQHOOKS/PRIVTAB/MACHINE/IRQACTIDS，消息格式见 `25-misc-unported`）、`sys_diagctl_stacktrace`（DIAGCTL_CODE_STACKTRACE，com.h:413；kernel 侧接线 forward reference：`32-stack-tracing.md` DIAGCTL 仍 ENOSYS）、`get_minix_kerninfo→kmessages`（type.h:214-232，A-3）、`getsysinfo`（libsys/getsysinfo.c：who→callnr 映射、SI_* 值、size 精确匹配 + sys_datacopy + root 检查，服务侧见 PM misc.c:105/VFS misc.c:61/RS/DS）、`vm_info_stats/usage/region`（libsys/vm_info.c：VM_INFO + VMIW_STATS/USAGE/REGION）、错误面（各调用失败 → 告警 + return，不 panic）
- **05**：`proctab_dmp`（319/349：sys_getproctab → PROCLOOP 22 行分页 + PRINTRTS）、`procstack_dmp`（359：sys_getproctab + sys_diagctl_stacktrace）、`privileges_dmp`（253：sys_getprivtab + sys_getproctab + s_flags_str/s_traps_str + s_ipc_to/s_k_call_mask）、`image_dmp`（169：sys_getimage）、`irqtab_dmp`（122：sys_getirqhooks + sys_getirqactids + IRQ_REENABLE/掩码）、`kmessages_dmp`（63：kerninfo→kmessages 环形缓冲展开，A-3）、`monparams_dmp`（94：sys_getmonparams + 换行展开）、`kenv_dmp`（192：sys_getkinfo + sys_getmachine）、`s_flags_str`（218）、`s_traps_str`（236）、`p_rts_flags_str`（300）、`proc_name`（386）、`proc[]`/`priv[]`/`image[]`（55-57）
- **06**：`mproc_dmp`（41：getsysinfo(PM_PROC_NR, SI_PROC_TAB) + flags_str + 22 行分页）、`sigaction_dmp`（75：同表 + getticks() 计算 alarm 剩余）、`flags_str`（21：WAITING/ZOMBIE/ALARM_ON/EXITING/... 位编码）
- **07**：`fproc_dmp`（25：getsysinfo(VFS_PROC_NR, SI_PROC_TAB) + OPEN_MAX fd 计数 + FP_SESLDR/FP_REVIVED/FP_BLOCKED_ON_CDEV 位语义）、`dtab_dmp`（67：getsysinfo(VFS_PROC_NR, SI_DMAP_TAB) + NONE 跳过）
- **08**：`rproc_dmp`（26：getsysinfo(RS_PROC_NR, SI_PROCPUB_TAB+SI_PROC_TAB) 双表 + RS_IN_USE 过滤 + s_flags_str）、`s_flags_str`（61：RS_ACTIVE/UPDATING/EXITING/NOPINGREPLY + SF_USE_COPY/SF_USE_REPL）
- **09**：`data_store_dmp`（9：getsysinfo(DS_PROC_NR, SI_DATA_STORE) + DSF_IN_USE 过滤 + DSF_MASK_TYPE 四类型输出 + 22 行分页 + 静态 prev_i 环形翻页（跳过未用槽位，到尾回绕））
- **10**：`vm_dmp`（55：vm_info_stats 首屏 → sys_getproctab → vm_info_usage + vm_info_region 批处理状态机）、`print_region`（11：连续相同 region 折叠 + 保护位显示）、分页游标 prev_i/prev_base/LINES=24 边界、错误处理（vm_info 失败 → 告警 continue）
- **99**：TTY_PROC_NR、F-key 常量表（keymap.h）、GET_*/SI_*/VMIW_*/DIAGCTL_CODE_* 常量表、错误码（EDONTREPLY 语义）、单线程事件循环执行模型声明（与 Kernel SMP+BKL 的区别）、跨服务引用（TTY func_key 通知、VM INFO handler、PM/VFS/RS/DS do_getsysinfo、kernel do_getinfo/do_diagctl）

### 5.4 明确排除 / 跳过的项

| 项 | 处理 | 依据 |
|----|------|------|
| `glo.h` 的 `diag_buf`/`diag_next`/`diag_size`/`sys_panic`/`dont_reply` | **死 extern**（全树无定义无使用），标注跳过 | `rg -rn "diag_buf" minix3/minix` 仅 `servers/is/glo.h` 一处声明 |
| `click_to_round_k`（dmp_kernel.c:42） | **死代码**（定义未使用），标注跳过 | `rg -n "click_to_round_k" servers/is/` 仅定义行 |
| `proctab_dmp` arm 分支（dmp_kernel.c:349-355） | **不移植**（x86-64 目标），标注排除 | `#if defined(__arm__)` 条件编译 |
| `IS_PROC_NR` | **不存在**（全树无定义），IS endpoint 动态分配（A-9） | `rg -rn "IS_PROC_NR" minix3/minix` 无命中 |
| `Makefile`（CPPFLAGS include 路径） | 构建/链接脚本，非语义 | WONTFIX |
| `inc.h` 包含面 | 无独立语义，并入 01 | 组织原则 |
| `drivers/tty` 其余部分（tty.c 主循环、键盘扫描） | **不属 IS 语义**：仅 fkey 观察者/通知面属 02 协议契约；TTY 实现细节归 TTY 自身（未 staging） | 范围声明 |

---

## 6. 实施路线

> 每篇新文档 = 基于对应 draft 素材改写（新增/重写处除外），并遵守 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约。所有 P0 修复完成后才可推进下一篇。

1. **00-is-overview 新建**（导航重写，含 §1.2 启动时序图 + 无 boot_image 语义）
2. **01-is-init-main 改写**（draft/tmp_main.c.md 素材；SEF 回调归位）
3. **02-is-fkey-contract 新建**（协议面，A-1/A-2 标注；TTY 侧契约 + 素材 map_unmap_fkeys）
4. **03-is-dump-dispatch 改写**（draft/tmp_dmp.c.md 素材；次主线路径图入 03）
5. **04-is-data-acquisition 新建**（数据面机制，A-3/A-4 标注）
6. **05~10 六个转储域篇改写**（各 draft/tmp_*.c.md 素材；每篇含布局 ABI 契约 + 分页格式契约）
7. **99-is-global-concepts 新建**（常量/错误码/执行模型/跨服务引用收口）
8. **kernel/VM/PM/VFS/RS/DS 侧交叉引用同步**（各 stage 对 IS 的引用指向新编号）

### 6.1 文档改写状态跟踪

> 每完成一篇，将状态改为 `reviewed`（附 scan 日期）。全部 `reviewed` 且无 P0 遗留 = 阶段完成。

| 编号 | 状态 | 首轮 review 日期 | 备注 |
|------|------|-----------------|------|
| 00 | skipped | — | 用户决策跳过（2026-09-04），从 01 开始 |
| 01 | reviewed | 2026-09-04 | draft/tmp_main.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/01-is-init-main/`） |
| 02 | reviewed | 2026-09-04 | 新建协议面；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/02-is-fkey-contract/`；A-1 兑现） |
| 03 | reviewed | 2026-09-04 | draft/tmp_dmp.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/03-is-dump-dispatch/`；次主线收口） |
| 04 | reviewed | 2026-09-04 | 新建数据面；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/04-is-data-acquisition/`；A-3 设计） |
| 05 | reviewed | 2026-09-04 | draft/tmp_dmp_kernel.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/05-is-dump-kernel/`；A-4 快照） |
| 06 | reviewed | 2026-09-04 | draft/tmp_dmp_pm.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/06-is-dump-pm/`） |
| 07 | reviewed | 2026-09-04 | draft/tmp_dmp_fs.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/07-is-dump-vfs/`） |
| 08 | reviewed | 2026-09-04 | draft/tmp_dmp_rs.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/08-is-dump-rs/`） |
| 09 | reviewed | 2026-09-04 | draft/tmp_dmp_ds.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/09-is-dump-ds/`） |
| 10 | reviewed | 2026-09-04 | draft/tmp_dmp_vm.c.md 素材；CONVERGED（scan 见 `.review/claude/fork-syscall-rewrite/10-is-dump-vm/`；阶段收官） |
| 99 | pending | — | 新建全局概念 |

---

## 7. Review 记录

### 7.1 深度 review（2026-08-16）

**方法**：按 review-process Step 1-5 对计划本身做语义全覆盖审计——逐函数/逐消息/逐标志/逐消费者核对 §2 文档拆分能否承载全部语义，检查模块边界是否重叠或漏项。

**发现的问题与修复**：

| # | 问题 | 级别 | 修复 |
|---|------|------|------|
| D-1 | 初版将 `map_unmap_fkeys` 归 03（分派篇），与 02（协议面）的"注册逻辑"职责重叠，且 init 锚点（sef_cb_init_fresh）到注册逻辑的链路断裂 | P1 | 归 02（协议面），03 仅保留按下事件分派（§2/§5.3 已归位） |
| D-2 | 初版漏掉 `kmessages_dmp` 的 `get_minix_kerninfo()->kmessages` 机制与其余 `sys_getinfo` 机制不同——usermapped 移除的 ARCH 影响若散落 04/05 两处会漏标 | P1 | 机制归 04（A-3），05 只讲格式化与布局（§4/§5.3 已归位） |
| D-3 | 初版漏掉 `procstack_dmp` 的 `sys_diagctl_stacktrace`（DIAGCTL_CODE_STACKTRACE）kernel 调用面 + kernel 侧接线仍 ENOSYS 的 forward reference | P1 | 补入 04 机制清单 + 05 函数归位 + 交叉引用（§5.2/§5.3） |
| D-4 | fkey 协议若只覆盖 IS 侧（libsys）会漏掉通知语义的前提——TTY 侧 `do_fkey_ctl` 的观察者状态机（覆盖登记/EPERM/位图消费）是 IS 行为的决定性契约 | P1 | 02 包含 TTY 侧契约（keyboard.c:401-585，§5.2/§5.3） |
| D-5 | 初版沿用"服务有固定 `IS_PROC_NR`"的假设——IS 无固定 endpoint，全树无定义 | P1 | 新增 A-9 + 00/01/99 职责（§4/§5.4） |
| D-6 | 初版漏掉"IS 何时存在"的语义——仅 `debug_fkeys` 启用时由 rc.minix 启动（条件性 debug 服务） | P2 | 补入 00/01/99（§1.2/§5.3） |
| D-7 | 初版将 `vm_dmp` 的批处理状态机（prev_i/prev_base/首屏 header/连续 region 折叠/LINES 边界）与 `print_region` 拆到两处讨论，状态割裂 | P1 | 10 单篇容纳完整状态机（§5.3） |
| D-8 | 初版未识别 `getsysinfo` 的**服务侧 size 精确匹配 + sys_datacopy + root 检查**契约（PM misc.c:105/VFS misc.c:61 实证）——仅当客户端消息面描述 | P1 | 补入 04（§5.3） |
| D-9 | 初版漏掉 `data_store_dmp` 的静态 `prev_i` 环形翻页语义（跳过未用槽位 + 到尾回绕，与其它 dump 的游标行为略异） | P2 | 补入 09 职责声明（§5.3） |
| D-10 | 测试基线未声明（stub 无测试，`cargo check -p minix-is` 通过；minix3 亦无 IS 专项测试） | P2 | 新增 §3.5 |

**结论**：修复后按 §5.3 函数清单反向核对——`servers/is/` 全部 8 个 .c 的顶层符号（33 个函数定义：main.c 6 + dmp.c 4 + dmp_kernel.c 13（含 proctab_dmp 双架构定义，语义 12 函数）+ dmp_pm.c 3 + dmp_fs.c 2 + dmp_rs.c 2 + dmp_ds.c 1 + dmp_vm.c 2 + 各文件全局数据）逐一落入 01~10；协议面 8 个头文件 + 4 个 libsys 客户端全部落入 02/04/99；外部对方（TTY、VM、PM/VFS/RS/DS、kernel）落入 02/04/05~10 契约层。**语义全覆盖，无遗漏**。

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/is/` 全部 .c/.h + 协议头文件 + libsys 客户端 + TTY 对方 + boot 证据逐一 grep 核对 §5.1/§5.2/§5.3 映射，并抽查主循环 dispatch 面与死声明。

**证据**：

```bash
wc -l minix3/minix/servers/is/*.c        # 132+52+83+396+109+74+157+148 = 1151，与 §1/§5.1 一致
rg -c '^[a-z].*\([^;]*$' minix3/minix/servers/is/*.c    # 顶层函数定义计数（main.c 6、dmp.c 4、dmp_kernel.c 13、dmp_pm.c 3、dmp_fs.c 2、dmp_rs.c 2、dmp_ds.c 1、dmp_vm.c 2 = 33），全部落入 §5.3
rg -n "while \(TRUE\)|return\(OK\)" minix3/minix/servers/is/main.c   # 主循环 main.c:44-69 实证
rg -n "struct proc proc|struct priv priv|struct boot_image image" minix3/minix/servers/is/dmp_kernel.c   # 全局表 55-57 实证
sed -n '44,64p' minix3/minix/kernel/table.c              # boot_image 17 项无 is → 无 boot_image 登记成立
sed -n '117p' minix3/etc/rc.minix                        # "up -n is -period 5HZ" → RS 运行时加载成立
sed -n '271,277p' minix3/etc/system.conf                 # "service is { vm INFO; uid 0 }" → 权限面成立
rg -rn "IS_PROC_NR" minix3/minix                         # 无命中 → A-9 成立
rg -rn "diag_buf" minix3/minix                           # 仅 servers/is/glo.h 声明 → 死 extern 成立
rg -n "click_to_round_k" minix3/minix/servers/is/        # 仅定义行 → 死代码成立
rg -n "GET_KMESSAGES" minix3/minix                       # 无命中 → A-3（kmessages 无 syscall 通道）成立
rg -n "FKEY_MAP|FKEY_UNMAP|FKEY_EVENTS|TTY_FKEY_CONTROL" minix3/minix/include/minix/com.h   # :874-877 与 §5.2 一致
rg -n "is_notify" minix3/minix/include/minix/com.h        # :93 与 §5.2 一致
rg -n "do_fkey_ctl|func_key" minix3/minix/drivers/tty/tty/arch/i386/keyboard.c   # :429,:519 与 §5.2 一致
rg -n "m_lsys_getsysinfo" minix3/minix/servers/pm/misc.c minix3/minix/servers/vfs/misc.c   # size 精确匹配 + sys_datacopy 实证
rg -n "CALLMAP\(VM_INFO" minix3/minix/servers/vm/main.c  # :566 → VM INFO handler 实证（交叉引用 02-stage-vm）
```

**结论**：8 个 .c 文件全部映射到新文档，无遗漏；33 个函数定义（含 proctab_dmp 双架构，语义 32 函数）逐一定位；协议面 8 个头文件 + 4 个 libsys 客户端 + TTY 对方 + boot/权限证据全部进入覆盖契约；A-1~A-12 与 minix3 现状对照成立。**覆盖完整性通过**。

---

## 8. 参见

- `draft/` — 旧占位 README + 逐行讲解素材（8 个 `tmp_*.md`）
- `../00-master-plan/README.md` — 目录重排与新主线说明（IS 归类 RS 加载组）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则）
- `../02-stage-vm/plan.md` 与 `../07-stage-ds/plan.md` — 同流程先例（07-stage-ds 亦为从零定义文档集）
- `minix3/minix/servers/is/` — C 源码（ground truth）
- `minix3/minix/drivers/tty/tty/arch/i386/keyboard.c` — fkey 协议对方（TTY 侧）
- `minix3/minix/lib/libsys/` — 客户端契约（fkey_ctl.c/getsysinfo.c/vm_info.c/sys_diagctl.c）
- `os/servers/is/` — Rust 实现（当前 stub）
