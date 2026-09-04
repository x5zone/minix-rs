# 07-stage-ds 文档重组计划（plan.md）

> **状态**: 定稿（2026-08-16 首版 + 深度 review + minix3 源码回归 review，见 §7）
> **范围**: `notes/rewrite/fork-syscall-rewrite/07-stage-ds/`
> **目标**: 以 **DS server 启动顺序为主线**定义 DS 全部文档；发布/订阅数据流为次主线；最终覆盖 Minix3 DS server（`servers/ds/`，2 个 .c，811 行）+ 协议面（`com.h`/`ipc.h`/`ds.h`/`sysinfo.h`）+ 客户端契约（`libsys/ds.c`，219 行）全部语义，支撑 DS server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/` 与 `03-stage-rs/`（同流程先例）、`minix3/minix/servers/ds/`（ground truth）、`os/servers/ds/`（Rust 实现，当前为 stub）

---

## 1. 背景与动机

### 1.1 现状问题

`07-stage-ds/` 目录自 2026-08-13 创建以来仅有**占位 README**（2026-08-16 移入 `draft/`），没有任何正式文档。历史逐行讲解素材（`tmp/ds/tmp_main.c.md`、`tmp/ds/tmp_store.c.md`，2026-08-14 产出）已一并移入 `draft/` 作素材。现状与 DS 的 boot 地位不匹配：

1. **boot 位置关键**——DS 是 boot_image 中**第一个用户服务**（`kernel/table.c:52`，`{DS_PROC_NR, "ds"}` 紧跟 5 个 kernel task 之后、RS 之前），是系统服务的"动态注册中心"：服务 label→endpoint 映射、驱动状态（`DS_DRIVER_UP`）、PM/VFS/RS 等核心服务的状态发布/订阅全部依赖 DS。`00-master-plan/README.md` 因果链：Kernel → VM → RS → 其余，DS 是 RS 加载后最先可用的注册面。
2. **语义小而精**——`servers/ds/` 仅 811 行 C（2 个 .c），是**用户态服务中最小的一个**，但协议面横跨 4 个头文件 + 客户端库 + 2 个外部布局消费者（IS `dmp_ds.c`、libmagicrt `magic_ds.c`）。文档必须一次到位地把"服务器内部语义"与"跨服务协议契约"讲清。
3. **与 03-stage-rs 相同**——无旧主线文档可迁移（只有占位 README），本计划从零定义文档集，`draft/` 仅保留占位与素材；§5 覆盖契约是后续写作的**唯一权威基线**，必须一次到位。

### 1.2 新主线：DS server 启动顺序 + 运行时主循环

与 `01-stage-kernel` / `02-stage-vm` / `03-stage-rs` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。DS 的启动与运行是严格线性的：

```
boot_image 登记（table.c:52：DS 第一用户服务）→ boot 映像随系统加载
  → kernel/main.c:265-267 RTS_VMINHIBIT（VM 建页表前禁止调度，:196 仅 kernel task/RS/VM 立即可调度）
  → VM 解除抑制后由 RS 调度启动（SEF 初始化）
  │
  ▼  main.c:28  main()
  ├─ env_setargs(argc, argv)                  ← 01：启动参数
  ├─ sef_local_startup()                      ← 01：SEF 生命周期
  │    ├─ sef_setcb_init_fresh(sef_cb_init_fresh)   ← 06：boot 映射回调
  │    ├─ sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL) ← 01/06：重启保留状态语义
  │    └─ sef_llvm_ds_st_init()               ← 01/12：Live Update 状态转移 hook
  ▼  sef_cb_init_fresh()（store.c:254，boot 锚点）
  ├─ 清空 ds_store / ds_subs 全部 flags       ← 03：数据结构复位
  ├─ sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, rprocpub) ← 06：rprocpub 表 grant
  └─ 循环 map_service（label→endpoint 登记，owner="rs"）← 06：boot 服务映射
  │
  ▼  main.c:45-88  主循环（运行时）
  ├─ get_work() → sef_receive(ANY)            ← 01：收消息（callnr/who_e）
  ├─ is_notify(callnr) → 打印告警 + EINVAL    ← 01：DS 不处理 notify 消息体
  ├─ switch(callnr)：
  │   ├─ DS_PUBLISH        → do_publish       ← 07
  │   ├─ DS_RETRIEVE       → do_retrieve      ← 08
  │   ├─ DS_RETRIEVE_LABEL → do_retrieve_label← 08
  │   ├─ DS_DELETE         → do_delete        ← 09
  │   ├─ DS_SUBSCRIBE      → do_subscribe     ← 10
  │   ├─ DS_CHECK          → do_check         ← 10
  │   ├─ DS_GETSYSINFO     → do_getsysinfo    ← 11
  │   └─ default           → 打印告警 + EINVAL← 01
  └─ reply()：m_type = result；EDONTREPLY 除外 ← 01
```

**每篇文档必须能回答一个问题：它位于 DS 启动时序（sef_cb_init_fresh）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 次主线：一条数据项的发布/订阅旅程

DS 的全部工作本质是**发布/订阅数据流**。次主线以"一条数据项从发布到被订阅者消费再到删除"的旅程贯穿阶段 5，其路径图在 `10-ds-subscribe-check` 内部绘制：

```
发布者 ds_publish_*(DS_PUBLISH)                  ← 12 客户端 / 07 服务器
  ├─ 05 身份：ds_getprocname（label→名字）
  ├─ 03/04 槽位：lookup_entry / alloc_data_slot
  ├─ 07 写入：u32 / label / str / mem + overwrite 语义
  └─ 07 update_subscribers(dsp, 1)：
       ├─ 10 check_sub_match（auth + regexec）
       └─ SET_BIT(old_subs) + ipc_notify(ep)     ← 订阅者收 notify
  ▼ 订阅者主循环收到 notify → DS_CHECK            ← 10
  ├─ do_check：取第一个置位项 → key 拷贝 → UNSET_BIT
  └─ ds_retrieve_*(DS_RETRIEVE)                  ← 08：拉取数据
  ▼ 删除 ds_delete_*(DS_DELETE)                  ← 09
  └─ do_delete：update_subscribers(dsp, 0) + notify（LABEL 级联清理）
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。`draft/` 保留旧占位与逐行素材（旧编号 `tmp_*.md` 不改名，作素材引用）。

### 阶段总览（14 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | draft 素材 | 变更 |
|------|------|------|---------|--------|-----------|-----------|------|
| 0 总览 | 00 | `00-ds-overview.md` | DS 是什么、启动主线图、文档导航、boot 两层语义 | `servers/ds/` 全部 | `os/servers/ds/` 全部 | `draft/README.md` | **重写**为导航（原占位信息并入） |
| 1 启动入口 | 01 | `01-ds-init-main.md` | `main`/`sef_local_startup`/`get_work`/`reply`/主循环骨架/notify 拒绝/EDONTREPLY | `main.c`（132 行） | `main.rs`、`lib.rs` | `draft/tmp_main.c.md` | 沿用（主循环分发表移入各 handler 篇） |
| 2 协议面 | 02 | `02-ds-message-contract.md` | call numbers（com.h）、`mess_ds_req`/`mess_ds_reply`/`union ds_val`（ipc.h）、DSF 标志全集（ds.h）、key grant 约定、getsysinfo 消息 | `com.h:498-507`、`ipc.h`、`ds.h`、`sysinfo.h:13` | `minix-types`（**缺 DsReq/DsReply，A-1**） | 无（协议面无旧素材） | **新增**：所有 handler 篇的前置协议文档 |
| 3 数据模型 | 03 | `03-ds-data-structures.md` | `struct data_store`/`struct subscription`、`ds_store`/`ds_subs` 表、`NR_DS_KEYS`/`NR_DS_SUBS`、flags 位语义、**SI_DATA_STORE 布局 ABI（A-10）** | `store.h` | `store.rs`、`subscription.rs` | `draft/tmp_store.c.md` | 沿用 |
| 3 | 04 | `04-ds-slot-management.md` | `alloc_data_slot`/`alloc_sub_slot`/`free_sub_slot`/`lookup_entry`/`lookup_label_entry`/`lookup_sub` | `store.c:11-107` | `store.rs` | `draft/tmp_store.c.md` | 沿用 |
| 3 | 05 | `05-ds-identity-auth.md` | `ds_getprocname`/`ds_getprocep`/`check_auth`、owner 语义、DS 自身身份、panic 路径 | `store.c:110-156` | `identity.rs`、`auth.rs` | `draft/tmp_store.c.md` | 沿用 |
| 4 启动映射 | 06 | `06-ds-boot-mapping.md` | boot 两层语义、`sef_cb_init_fresh`、rproctab grant、`map_service`、RS 握手、STATEFUL 重启语义 | `store.c:229-285`、`kernel/table.c:44-64`、`kernel/main.c:196,265-267`、`rs.h:165-183`、`sef.h:44-53,85` | `boot.rs`、`sef.rs` | `draft/tmp_main.c.md` + `draft/tmp_store.c.md` | 沿用 + 补 boot 时序 |
| 5 数据面 handler | 07 | `07-ds-publish.md` | `do_publish` 全类型（u32/label/str/mem）、overwrite/EEXIST、LABEL 仅 RS、通知接线 | `store.c:287-381` | `publish.rs` | `draft/tmp_store.c.md` | 沿用 |
| 5 | 08 | `08-ds-retrieve.md` | `do_retrieve` + `do_retrieve_label`、`DSF_PRIV_RETRIEVE`、MIN 长度语义 | `store.c:383-454` | `retrieve.rs` | `draft/tmp_store.c.md` | 沿用 |
| 5 | 09 | `09-ds-delete.md` | `do_delete` 全类型、LABEL 级联清理（subs+entries）、`free(data)` | `store.c:583-651` | `delete.rs` | `draft/tmp_store.c.md` | 沿用 |
| 5 | 10 | `10-ds-subscribe-check.md` | `do_subscribe` + `do_check` + `update_subscribers`/`check_sub_match` + `DSF_INITIAL` + notify 流、**次主线路径图** | `store.c:456-581,186-227` | `subscribe.rs`、`check.rs`、`notify.rs` | `draft/tmp_store.c.md` | 沿用 |
| 5 | 11 | `11-ds-getsysinfo.md` | `do_getsysinfo`、`SI_DATA_STORE`、**布局 ABI 消费者（IS dmp_ds.c）** | `store.c:653-678`、`servers/is/dmp_ds.c` | `getsysinfo.rs` | `draft/tmp_store.c.md` | 沿用 + 补 IS 消费者 |
| 6 客户端契约 | 12 | `12-ds-client-library.md` | `libsys/ds.c` 全部 API、grant 生命周期（`do_invoke_ds`）、`_taskcall`、死 map/snapshot API（A-7） | `libsys/ds.c`（219 行）、`ds.h:50-58` | `minix-sys`（**DS 客户端模块，A-8**） | 无 | **新增**：服务器→客户端双向协议契约 |
| 99 全局概念 | 99 | `99-ds-global-concepts.md` | `DS_PROC_NR`、常量表（DSF/DS_*）、错误码汇总、与 kernel/RS/IS/驱动库的交叉引用 | `com.h:65,498-507`、`ds.h` | `minix-types` | `draft/README.md` | 沿用 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序中的位置与下一阶段的入口：

```
01（启动骨架）→ 02（协议面）→ 03~05（数据模型/槽位/身份）
→ 06（boot 映射：sef_cb_init_fresh 锚点）
→ 07~11（主循环 dispatch 的服务 handler，次主线数据流）
→ 12（客户端契约，协议闭环）→ 99（全局概念收口）
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
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：协议面 02 先于 handler 07~11，数据模型 03~05 先于 boot 映射 06，客户端契约 12 殿后）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 sef_cb_init_fresh / 主循环 dispatch 的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `07-ds-publish.md` §通知接线 → `10-ds-subscribe-check.md`）
- 与 kernel 文档交叉引用用 `../01-stage-kernel/NN-*.md`（boot 抑制机制 → `09-vm-boot-protocol.md`；IPC 原语 → `12-ipc-core.md`/`13-syscall-dispatch.md`；safecopy → `18-syscall-copy.md`）
- 与 RS 文档交叉引用用 `../03-stage-rs/NN-*.md`（rproctab → `02-rs-process-table.md`/`12-rs-init-run.md`；label 发布 → `11-rs-publish.md`）
- 对 draft 素材的引用一律指向 `draft/`，并标注"素材"

### 3.4 每篇文档边界声明（执行级）

> 写作每篇文档前，先按本表声明**前置依赖 / 本篇职责 / 不覆盖（移交）**，防止内容交叉与遗漏。边界以 §5.3 函数清单为准绳。

| 编号 | 前置依赖 | 本篇职责（核心语义） | 不覆盖（移交） |
|------|---------|---------------------|--------------|
| 00 | 无 | 总览、启动主线图、boot 两层语义、文档导航、设计原则 | 一切机制细节（01~12、99） |
| 01 | 00 + kernel `12-ipc-core`/`09-vm-boot-protocol` | `main`/`sef_local_startup`/`get_work`/`reply`、主循环分类骨架、notify 拒绝、EDONTREPLY | SEF boot 回调体（06）、各 handler 实现（07~11） |
| 02 | 01 | call numbers、`mess_ds_req`/`mess_ds_reply`/`union ds_val` 字段级语义、DSF 标志全集与掩码、key grant 约定、A-1（消息类型缺口） | 服务器存储语义（03~05）、handler 流程（07~11） |
| 03 | 02 | `struct data_store`/`struct subscription` 全字段、表容量、flags 位语义、**SI_DATA_STORE 布局 ABI（A-10）** | 槽位查找/分配原语（04）、身份/权限（05） |
| 04 | 03 | `alloc_data_slot`/`alloc_sub_slot`/`free_sub_slot`、三个 lookup 的线性扫描语义 | 身份映射（05）、handler 调用面（07~11） |
| 05 | 03 | `ds_getprocname`/`ds_getprocep`（label 双向映射）、`check_auth` 权限位语义、DS 自身身份、panic 路径 | boot 映射的 owner="rs" 来源（06） |
| 06 | 01 + 03/05 + RS `02`/`11` | `sef_cb_init_fresh`、rproctab grant 拷贝、`map_service`、boot 两层语义、STATEFUL 重启 | 客户端 `ds_publish_label` 调用面（12） |
| 07 | 02/03/04/05 + 10（通知接线） | `do_publish` 全类型分支、overwrite/EEXIST、LABEL 仅 RS、写入后 `update_subscribers(dsp,1)` | 检索（08）、删除（09）、订阅机制（10） |
| 08 | 02/03/05 | `do_retrieve` + `do_retrieve_label`、`DSF_PRIV_RETRIEVE`、MIN 截断语义、回复字段写回 | 发布（07）、订阅（10） |
| 09 | 02/03/04/05 + 10（通知接线） | `do_delete` 全类型、LABEL 级联（subs 先于 entries）、`free` 释放 | 订阅机制内部（10） |
| 10 | 02/03/04/05 | `do_subscribe`（regex 锚定/DSF_INITIAL/overwrite）、`do_check`（bitmap 消费）、`update_subscribers`/`check_sub_match`/`ipc_notify`、次主线路径图 | 客户端 `ds_subscribe`/`ds_check`（12） |
| 11 | 02/03 + IS `dmp_ds.c` | `do_getsysinfo`、`SI_DATA_STORE`、size 精确匹配、**布局 ABI 消费者契约（A-10）** | 存储内部语义（03~05） |
| 12 | 02 + 服务器 handler 篇 | `libsys/ds.c` 全部 API、grant 生命周期、`_taskcall`、`DS_DRIVER_UP` 事件、死 map/snapshot API（A-7） | 服务器内部实现（03~11） |
| 99 | 全部 | `DS_PROC_NR`、DSF/DS_* 常量表、错误码汇总、跨服务引用（RS/IS/驱动库/PM/VFS） | 各机制细节 |

### 3.5 测试基线

> `os/servers/ds/` 当前为 stub（`lib.rs: pub fn init() {}`，`main.rs: minix_ds::init(); loop {}`）。`cargo check -p minix-ds` 通过（2026-08-16，0 errors / 28 pre-existing warnings 来自 `minix-sys` 依赖），**无任何测试**。每篇文档 §测试 的"测试总数"声明以此为基线。

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 07-stage-ds 范围内的 ARCH 项，写文档时必须逐项落实。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **DS 消息类型** | `mess_ds_req`/`mess_ds_reply`/`union ds_val`（`ipc.h`，字段全 32 位，56 字节 payload 内可容纳，布局无歧义） | `minix-types` **尚无 DsReq/DsReply 类型**，需新增（消息字段全 i32/u32，64 位下布局兼容） | 02 | **缺口**：minix-types 新增类型 |
| A-2 | **正则引擎** | POSIX `regcomp`/`regexec`（`store.c:456-534`，`REG_EXTENDED` + `^...$` 锚定） | Rust `no_std` 无 POSIX regex：决策（a）`regex` crate（alloc feature）；（b）自研最小匹配器（仅需完整锚定匹配）；（c）defer | 10 | **待决策**（行为契约：锚定语义必须保留） |
| A-3 | **动态内存** | `malloc`/`free`（STR/MEM 数据缓冲，`store.c:330-350,608-609`） | `no_std` 分配策略：全局分配器或专用 `DsAllocator`（cap 上限，`ds_store` 内联缓冲 or 堆） | 07/09/03 | 待设计 |
| A-4 | **静态数组表** | `ds_store[NR_DS_KEYS]`/`ds_subs[NR_DS_SUBS]` 静态数组 + 线性扫描 | Rust 类型化表：`[Option<DataEntry>; NR_DS_KEYS]`（或 free-list 索引），`EntryIndex<Data>` newtype 索引 | 03/04 | 待设计 |
| A-5 | **订阅位图** | `bitchunk_t old_subs[BITMAP_CHUNKS(NR_DS_KEYS)]`（`store.h`，`SET_BIT`/`UNSET_BIT`/`GET_BIT`） | `minix-types::bitmap`（已存在）固定容量位图；`NR_DS_KEYS=128` → 2×u64 | 03/10 | 可复用 |
| A-6 | **SEF / Live Update 状态转移** | `sef_llvm_ds_st_init()` → weak `_magic_ds_st_init`（`libmagicrt/magic_ds.c`）：magic 插桩**直接遍历 DS 静态内存**（`ds_store`/`ds_subs`），`dsi_u` union 按类型分派转移（U32/LABEL identity、STR/MEM 类型转换）；`sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)` 重启保留状态 | minix-rs 无 LLVM magic 插桩：状态转移改为**显式序列化/反序列化**（参照 `03-stage-rs` 的 `state_data.rs`/`sef.rs`）；STATEFUL 重启语义 = 数据结构不重置 | 01/06/12 | **缺口**：需显式转移设计（先例见 RS） |
| A-7 | **死 API 排除契约** | `do_snapshot`（proto.h:16，**无定义**）；`DS_SNAPSHOT`（com.h:505，主循环无 case → default EINVAL）；`ds_publish_map`/`ds_snapshot_map`/`ds_retrieve_map`/`ds_delete_map`（ds.h:53-58，**无实现**）；`DSF_PRIV_SNAPSHOT`（=0x004，别名 DSF_PRIV_OVERWRITE，无使用） | 不实现，标注排除 + 语义契约（若未来需要 snapshot，需重开设计） | 01/02/12 | 排除（grep 实证，见 §5.4） |
| A-8 | **客户端库归属** | `libsys/ds.c` 全部 `ds_*` API（`_taskcall(DS_PROC_NR, ...)` + grant） | `minix-sys` 新增 DS 客户端模块（当前 `minix-sys` 是 stub，`sendrec`/`notify` 为 `todo!()`）；或先交付纯函数层（消息构造/解析） | 12 | 待实施（依赖 IPC 落地） |
| A-9 | **错误码** | errno 全集：`EPERM`/`EINVAL`/`ESRCH`/`ENOENT`/`EEXIST`/`EAGAIN`/`ENOMEM`/`EDONTREPLY`（+ `EFATAL` panic 路径） | `minix_types::Errno`（已存在，含 `EDONTREPLY`），禁止自创错误码 | 各 handler 篇 + 99 | 已具备 |
| A-10 | **SI_DATA_STORE 布局 ABI** | `do_getsysinfo` 原样拷贝 `ds_store`（`sizeof(struct data_store)*NR_DS_KEYS`，x86-64 = 192B×128）；**IS `dmp_ds.c` 按 `store.h` 布局直接解释**（`servers/is/dmp_ds.c`，`getsysinfo(DS_PROC_NR, SI_DATA_STORE, ...)`） | Rust 侧若保持 `#[repr(C)]` 等价布局则可兼容 IS 不变；否则**声明 ARCH 偏离 + IS 侧同步改造**（`dmp_ds.c` 是 debug 输出，可随 IS 重写演进） | 03/11 | **待决策**（兼容 vs 偏离，两方案都要三处一致标注） |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

### 5.1 C 源文件 → 新文档映射（2 个 .c）

| C 源文件 | 行数 | 覆盖文档 | 核对 |
|---------|------|---------|------|
| `servers/ds/main.c` | 132 | 01（全部）、06（SEF 注册调用点） | 已核对 |
| `servers/ds/store.c` | 679 | 03~11（按函数分片，见 §5.3） | 已核对 |

### 5.2 头文件覆盖

| 头文件 | 覆盖文档 | 核对 |
|--------|---------|------|
| `servers/ds/inc.h` | 01（包含面，无独立语义） | 已核对 |
| `servers/ds/proto.h` | 各 handler 篇 + §5.4 排除（`do_snapshot`） | 已核对 |
| `servers/ds/store.h` | 03（struct/常量） | 已核对 |
| `include/minix/com.h`（DS_PROC_NR:65、DS_RQ_BASE:498、DS_*:500-507、is_notify:93） | 02、99 | 已核对 |
| `include/minix/ipc.h`（mess_ds_req:107-115、mess_ds_reply:100-105、union ds_val:94-99、mess_lsys_getsysinfo:1065-1071） | 02、11 | 已核对 |
| `include/minix/ds.h`（DSF_*:17-32、DS_MAX_KEYLEN:35、DS_DRIVER_UP:37、客户端 API:40-68） | 02、12、99 | 已核对 |
| `include/minix/sysinfo.h`（SI_DATA_STORE:13） | 11 | 已核对 |
| `include/minix/sef.h`（sef_init_info_t:44-53、SEF_CB_INIT_RESTART_STATEFUL:85） | 06 | 已核对 |
| `include/minix/rs.h`（struct rprocpub:165-183） | 06 | 已核对 |

### 5.3 语义模块覆盖清单（函数级）

> 以下为跨文件的关键语义，防止"文件有映射但函数漏掉"。写作时以 `draft/` 逐行素材 + 本清单双重核对。

- **01**：`main`（主循环骨架、notify 拒绝、default EINVAL、EDONTREPLY 跳过）、`sef_local_startup`（3 个注册 + sef_startup）、`get_work`（sef_receive + who_e/callnr）、`reply`（ipc_send + 失败打印）、全局 `who_e`/`callnr` 单线程状态
- **02**：`DS_PUBLISH`~`DS_GETSYSINFO`（com.h:500-507，含 `DS_SNAPSHOT` A-7）、`mess_ds_req`（key_grant/key_len/flags/val_in/val_len/owner 字段级）、`mess_ds_reply`（val_out/val_len）、`union ds_val`（grant/u32/ep）、DSF 标志全集（IN_USE/PRIV_RETRIEVE/PRIV_OVERWRITE/PRIV_SNAPSHOT/PRIV_SUBSCRIBE/TYPE_U32/STR/MEM/LABEL/OVERWRITE/INITIAL + MASK_TYPE/MASK_INTERNAL）、`DS_MAX_KEYLEN`、`DS_DRIVER_UP`
- **03**：`struct data_store`（flags/key[80]/owner[80]/union u）、`struct subscription`（flags/owner/regex/old_subs bitmap）、`NR_DS_KEYS=2*NR_SYS_PROCS=128`、`NR_DS_SUBS=4*NR_SYS_PROCS=256`、`ds_store`/`ds_subs` 静态表
- **04**：`alloc_data_slot`、`alloc_sub_slot`、`free_sub_slot`（regfree + memset）、`lookup_entry`（key+type 双条件）、`lookup_label_entry`（type LABEL + num）、`lookup_sub`（owner）
- **05**：`ds_getprocname`（DS 自身 → "ds"；label 反查）、`ds_getprocep`（label 正查 + panic）、`check_auth`（权限位未置 → 放行；置位 → owner 名相等）
- **06**：`sef_cb_init_fresh`（复位 + rproctab grant 拷贝 + 循环 map_service）、`map_service`（owner="rs"、flags=IN_USE\|LABEL、update_subscribers(dsp,1)）、boot 两层语义（table.c:44-64 登记序 vs kernel/main.c:196,265-267 执行序）
- **07**：`do_publish`（source 身份 → LABEL 仅 RS → get_key_name → lookup_entry/lookup_label_entry → alloc/overwrite/EEXIST → 4 类型写入 → 属性设置 → update_subscribers(dsp,1)）
- **08**：`do_retrieve`（get_key_name → lookup → check_auth(PRIV_RETRIEVE) → 4 类型输出、STR/MEM MIN 截断 + val_len）、`do_retrieve_label`（按 ep 反查 key）
- **09**：`do_delete`（身份 → key → lookup → owner 检查 → 4 类型分支，LABEL 级联：subs 先清再 entries、STR/MEM free(data) → update_subscribers(dsp,0) → flags=0）
- **10**：`do_subscribe`（owner → 已有订阅 EEXIST/OVERWRITE 释放 → regex `^...$` 锚定 + REG_EXTENDED → type_set 掩码 → DSF_INITIAL 即时扫描 + notify）、`do_check`（owner/sub 查找 → 首置位扫描 ENOENT → key 拷贝 → 回复 type/owner → UNSET_BIT）、`check_sub_match`（auth + regexec）、`update_subscribers`（类型匹配 → SET/UNSET_BIT(nr) → ipc_notify(ep)）
- **11**：`do_getsysinfo`（SI_DATA_STORE → 原样拷贝 ds_store、size 精确匹配 → sys_datacopy(SELF→caller)）、客户端入口 `libsys/getsysinfo.c:22`（DS_PROC_NR → DS_GETSYSINFO，IS 的 `getsysinfo()` 路径）
- **12**：`do_invoke_ds`（key grant 生命周期：CHECK/RETRIEVE_LABEL 写 grant 80B，其余读 grant strlen+1）、`ds_publish_u32/str/mem/label`（`ds_publish_raw`）、`ds_retrieve_u32/str/mem/label_name/label_endpt`（`ds_retrieve_raw`）、`ds_delete_u32/str/mem/label`、`ds_subscribe`、`ds_check`（回复字段复用 m_ds_req.flags/owner）
- **99**：`DS_PROC_NR`、DSF/DS_* 常量表、errno 汇总（§A-9）、跨服务引用（RS manager.c:513,800 ds_publish_label；VFS main.c:441 ds_subscribe + misc.c:960-985 ds_check/ds_retrieve_u32；PM misc.c；驱动库 DS_DRIVER_UP；IS dmp_ds.c）

### 5.4 明确排除 / 跳过的项

| 项 | 处理 | 依据 |
|----|------|------|
| `proto.h:do_snapshot` | **死声明**（无定义），标注跳过 | grep 全 `servers/ds/` 无实现 |
| `DS_SNAPSHOT`（com.h:505） | 主循环无 case → default EINVAL，标注为 A-7 排除 | grep main.c switch 无 case |
| `ds_publish_map`/`ds_snapshot_map`/`ds_retrieve_map`/`ds_delete_map`（ds.h:53-58） | **死声明**（全 minix3 无实现），标注跳过 | grep 全 `minix3/` 仅声明 |
| `DSF_PRIV_SNAPSHOT`（ds.h:21） | 与 `DSF_PRIV_OVERWRITE` 同值别名，无使用，标注 | grep 全 `minix3/` 仅定义 |
| `libmagicrt/magic_ds.c` 的 magic 插桩机制 | **不移植**：minix-rs 无 LLVM magic；语义并入 A-6（显式状态转移） | 架构演进 |
| `Makefile`（`-Dregcomp=_regcomp` 等） | 构建/链接脚本，非语义 | WONTFIX |
| `tests/ds/dstest.c`/`subs.c` | 不建独立文档，作为**行为契约**并入各 handler 篇 §测试要点 | 组织原则 |

---

## 6. 实施路线

> 每篇新文档 = 基于对应 draft 素材改写（新增/重写处除外），并遵守 §3 讲述结构 + §4 ARCH 标注 + §5 覆盖契约。所有 P0 修复完成后才可推进下一篇。

1. **00-ds-overview 新建**（导航重写，含 §1.2 启动时序图 + boot 两层语义）
2. **01-ds-init-main 改写**（draft/tmp_main.c.md 素材；主循环分发表移入各 handler 篇）
3. **02-ds-message-contract 新建**（协议面，A-1 minix-types 缺口标注）
4. **03~05 数据模型三篇改写**（draft/tmp_store.c.md 素材；A-4/A-5/A-10 标注）
5. **06-ds-boot-mapping 改写**（SEF 锚点；RS 交叉引用）
6. **07~11 数据面五篇改写**（次主线路径图入 10；每篇含测试要点引用 tests/ds/）
7. **12-ds-client-library 新建**（A-7/A-8 标注）
8. **99-ds-global-concepts 新建**（常量/错误码/跨服务引用收口）
9. **kernel/RS 侧交叉引用同步**（`01-stage-kernel`/`03-stage-rs` 对 DS 的引用指向新编号）

### 6.1 文档改写状态跟踪

> 每完成一篇，将状态改为 `reviewed`（附 scan 日期）。全部 `reviewed` 且无 P0 遗留 = 阶段完成。

| 编号 | 状态 | 首轮 review 日期 | 备注 |
|------|------|-----------------|------|
| 00 | pending | — | 新建导航 |
| 01 | pending | — | draft/tmp_main.c.md 素材 |
| 02 | pending | — | 新建协议面 |
| 03 | pending | — | draft/tmp_store.c.md 素材 |
| 04 | reviewed（2026-09-04，CONVERGED，0/0/0） | 2026-09-04 | draft/tmp_store.c.md 素材 + slots.rs 新实现 |
| 05 | pending | — | draft/tmp_store.c.md 素材 |
| 06 | pending | — | 素材 + boot 时序补充 |
| 07 | pending | — | 素材 + 通知接线 |
| 08 | pending | — | 素材 |
| 09 | pending | — | 素材 + LABEL 级联 |
| 10 | pending | — | 素材 + 次主线路径图 |
| 11 | pending | — | 素材 + IS 消费者 |
| 12 | pending | — | 新建客户端契约 |
| 99 | pending | — | 新建全局概念 |

---

## 7. Review 记录

### 7.1 深度 review（2026-08-16）

**方法**：按 review-process Step 1-5 对计划本身做语义全覆盖审计——逐函数/逐消息/逐标志/逐消费者核对 §2 文档拆分能否承载全部语义，检查模块边界是否重叠或漏项。

**发现的问题与修复**：

| # | 问题 | 级别 | 修复 |
|---|------|------|------|
| D-1 | 初版将 `update_subscribers`/`check_sub_match` 散落在 07/09/10 三篇，订阅通知机制无单一归属 | P1 | 并入 10 为"订阅机制"语义单元（§5.3 已归位） |
| D-2 | 初版漏掉 `mess_lsys_getsysinfo` 消息格式与 `sys_datacopy(SELF→caller)` 拷贝语义 | P1 | 补入 11（§5.2 ipc.h 行 + §5.3） |
| D-3 | 初版未识别 **SI_DATA_STORE 布局 ABI**：IS `dmp_ds.c` 按 `store.h` 布局直接解释 DS 内存，跨服务器 ABI | P1 | 新增 A-10 + 03/11 职责（§4/§3.4） |
| D-4 | 初版未识别 **libmagicrt/magic_ds.c**（LU 状态转移直接遍历 ds_store 静态内存 + dsi_u union 分派） | P1 | 新增 A-6 缺口契约 + 01/06/12 职责 |
| D-5 | 初版漏掉 `ds_publish_str` 的客户端 `value[length-1]='\0'` 原地改写与 `ds_retrieve_str` 的 `value[length-1]='\0'` 收尾 | P2 | 补入 12 行为契约（§5.3） |
| D-6 | 初版漏掉 `do_check` 回复**复用请求字段**（m_ds_req.flags/owner 写回） | P2 | 补入 02/10（§5.3） |
| D-7 | 初版未明确 `check_auth` 的"权限位未置 → 放行"语义（DSF_PRIV_* 是**选择性保护**，非默认保护） | P1 | 补入 05 职责声明（§5.3） |
| D-8 | 初版漏掉 boot 两层语义（登记序 vs 执行序）在文档中的定位 | P2 | 00 总览 + 06 职责（§1.2/§2） |
| D-9 | 初版漏掉 `DSF_MASK_TYPE` 的 `0x80` 空位与 `DSF_MASK_INTERNAL` 掩码语义 | P2 | 补入 02（§5.3） |
| D-10 | 测试基线未声明（stub 无测试，`cargo check -p minix-ds` 通过） | P2 | 新增 §3.5 |

**结论**：修复后按 §5.3 函数清单反向核对——`servers/ds/` 全部 27 个顶层符号（25 函数 + 2 数据定义：4 main.c + 21 store.c 函数 + ds_store/ds_subs）逐一落入 01/03~11；协议面 4 头文件 + 客户端库全部落入 02/12/99；外部消费者（IS、libmagicrt）落入 11/06。**语义全覆盖，无遗漏**。

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/ds/` 全部 .c/.h + 协议头文件 + 客户端库逐一 grep 核对 §5.1/§5.2/§5.3 映射，并抽查主循环 dispatch 面与死声明。

**证据**：

```bash
wc -l minix3/minix/servers/ds/*.c          # main.c 132 + store.c 679 = 811，与 §1 一致
grep -n '^static \|^int do_\|^int sef_\|^void sef_\|^int main' minix3/minix/servers/ds/*.c   # 命中 32 行（main.c 9 + store.c 23，含 5 个声明/原型），其中 27 个为顶层定义（25 函数 + 2 数据定义），全部落入 §5.3
grep -n 'DS_PUBLISH\|DS_RETRIEVE\|DS_SUBSCRIBE\|DS_CHECK\|DS_DELETE\|DS_GETSYSINFO' minix3/minix/servers/ds/main.c   # switch 面 7 case + default，与 §1.2 一致
grep -rn "do_snapshot\|ds_publish_map\|ds_snapshot_map" minix3/minix --include=*.c | grep -v tests/   # 仅 proto.h/ds.h 声明，无定义 → A-7 排除成立
grep -rn "SI_DATA_STORE" minix3/minix --include=*.c   # servers/is/dmp_ds.c:15 消费者 → A-10 成立
grep -rn "ds_store\|data_store" minix3/minix/lib/libmagicrt/magic_ds.c   # LU 状态转移直接遍历 → A-6 成立
grep -rn "ds_publish_label" minix3/minix/servers/rs/manager.c   # :513,:800 → 12/99 交叉引用成立
sed -n '44,64p' minix3/minix/kernel/table.c + sed -n '196,205p;255,270p' minix3/minix/kernel/main.c   # boot 两层语义证据
```

**结论**：2 个 .c 文件全部映射到新文档，无遗漏；27 个顶层符号逐一定位；4 个协议头文件 + 客户端库 + 2 个外部布局消费者全部进入覆盖契约；A-1~A-10 与 minix3 现状对照成立。**覆盖完整性通过**。

---

## 8. 参见

- `draft/` — 旧占位 README + 逐行讲解素材（`tmp_main.c.md`/`tmp_store.c.md`）
- `../00-master-plan/README.md` — 目录重排与新主线说明（DS boot 地位）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则）
- `../02-stage-vm/plan.md` 与 `../03-stage-rs/plan.md` — 同流程先例（03-stage-rs 亦为从零定义文档集）
- `minix3/minix/servers/ds/` — C 源码（ground truth）
- `minix3/minix/lib/libsys/ds.c` — 客户端契约（libsys）
- `os/servers/ds/` — Rust 实现（当前 stub）
