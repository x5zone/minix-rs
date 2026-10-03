# 06 — DS 启动映射：第一次启动时 RS 的服务表怎么登记进来

> **分类**: 启动映射 / boot 锚点
> **源码**: `minix3/minix/servers/ds/store.c:map_service`、`kernel/table.c:44-64`、`kernel/main.c:196,265-267`、`minix3/minix/include/minix/rs.h:rprocpub`、`sef.h:44-53,85`
> **说明**: DS 启动后做的第一件实事：清两张表，从 RS 拷来整张服务表，逐个登记为 label 条目。本文讲清这个"清—拷—逐登"三步，以及背后的两层启动顺序。

---

## 1 概念

### 1.1 目标读者与前置知识

面向想理解"DS 表里最初的数据从哪来"的读者。前置知识：03 的条目形状（知道 label 条目长什么样），05 的翻译（知道 owner 名字哪来）。boot 映像和 RS 的概念现场解释。

### 1.2 本章不讲什么

- SEF 回调怎么注册——那是 01 的事（本篇讲注册之后**被调时**发生什么）。
- label 条目发布后怎么被检索——那是 08 的事。
- 客户端怎么调发布——那是 12 的事。

### 1.3 三层启动顺序

DS 的启动分三层，容易混（统一口径见 `../00-master-plan/README.md` 的"boot 链四层归因表"：那里把"字节从哪来"与"槽位谁给"也各算一层，本节把它们合并进了"登记序"）：

1. **登记序**（`kernel/table.c:44-64`）：boot 映像里 DS 排第一个用户模块（`DS_PROC_NR` 紧跟 5 个内核任务之后、RS 之前）。这是"占位"，不是"运行"——模块字节按这个顺序摆放，但谁先跑不由它决定（表头注释 L37-41 说这个顺序影响的是 NOTIFY 投递优先级）。
2. **装载序**（`minix3/minix/servers/vm/main.c:497-512` + `:331-417`）：DS 的 ELF 段由 **VM** 解析并铺进地址空间（DS 自己连页表能力都没有，无法把自己装进去）；**不是 RS 加载的**——RS 自己也是被 VM 装载的那一批。
3. **放行序**（`kernel/main.c:196,253,264-267` 抑 + `minix3/minix/servers/rs/table.c:19-27` 放）：内核先抑制调度（非 RS/VM 者挂 `RTS_NO_PRIV|RTS_NO_QUANTUM`，非 VM 者再挂 `RTS_VMINHIBIT|RTS_BOOTINHIBIT`）；VM 建好地址空间并解除抑制后，RS 按上面那张表的固定顺序逐个授权放行：PM → SCHED → VFS → **DS（第四位）**→ tty → memory → MIB → PFS → MFS。

一句话：table 管"谁在映像里"，VM 管"谁能读到自己的代码"，RS 管"谁先跑"。DS 能做注册中心，不靠"最早放行"（它排在 PM/SCHED/VFS 之后），靠的是"比所有需要向它注册的服务都早"：那些服务多数是 init 跑 rc 脚本后经 RS 读盘拉起来的（`minix3/etc/usr/rc`），时序上晚了一整代。

### 1.4 清—拷—逐登三步（`sef_cb_init_fresh`，`store.c:254-279`）

DS 首次启动只做三件事：

```
清两张表 flags（:261-266）
  → 从 RS 拷服务表 rprocpub[]（sys_safecopyfrom，:269-272，失败 panic）
  → 逐个在用项 map_service 登记（:273-279，失败 panic）
```

`map_service`（`:229-249`）登记一个服务：取槽（满则 `ENOMEM`）→ 填三栏（key=服务名、u32=端点、owner=`"rs"`，flags=`IN_USE|TYPE_LABEL`）→ 顺手跑一轮订阅者通知（`update_subscribers(dsp, 1)`，`:246`——启动时一般没订阅者，这一步是空转，但语义统一：任何新条目都要过通知环）。

`"rs"` 这个 owner 是写死的（`:242`）。含义："这条 label 是 RS 背书的"。后面 07 会讲：只有 RS 能发布 label（`do_publish` 拦非 RS 的 label 发布，`:302`）——登记和发布两处同守一条规则，boot 映射是这条规则的"创世版本"。

### 1.5 重启不清表（STATEFUL）

01 注册过 `SEF_CB_INIT_RESTART_STATEFUL`：DS 重启走的不是 `sef_cb_init_fresh`（不清表），状态保留。这是注册中心的存在意义——RS 重启崩了的服务，靠 DS 里还活着的注册项把它拉起来（A-6 的状态转移设计与此同源：Rust 用显式序列化代替 magic 插桩，但"重启不清"语义不变）。

### 1.6 小结

三层序（table 占位、VM 装载、RS 放行），三步锚（清、拷、逐登），owner 写死 `"rs"`，重启不清表。下一站 07——运行时的第一个 handler：发布。

---

## 2 C 源码分析

### 2.1 `map_service`（`store.c:229-249`）

取槽（`:235`，`alloc_data_slot`，满 `ENOMEM`）→ `strcpy key`（`:240`）→ `u.u32 = endpoint`（`:241`，label 寄数字道）→ `strcpy owner "rs"`（`:242`）→ `flags = IN_USE|TYPE_LABEL`（`:243`）→ `update_subscribers(dsp, 1)`（`:246`）→ `OK`。

### 2.2 `sef_cb_init_fresh`（`store.c:254-279`）

清条目表 flags（`:261-263`）→ 清订阅表 flags（`:264-266`，注意只清旗，不清整个结构——和 `free_sub_slot` 的"旗清"同式）→ `sys_safecopyfrom(RS, rproctab_gid → rprocpub[NR_BOOT_PROCS])`（`:269-272`，grant 跨服拷，失败 `panic`）→ 逐 `in_use` 项 `map_service`（`:273-279`，失败 `panic`）→ `OK`。

两次 `panic` 的含义：启动锚点失败没有"降级"可言——表没建成，后面一切免谈。运行时的 handler 失败回 errno，启动失败 panic，轻重分明（和 01 的 `get_work` panic / `reply` 不 panic 同构）。

### 2.3 两层序证据

`table.c:44-64`（DS 登记第一用户服务）vs `main.c:196`（仅 kernel task/RS/VM 先可调度）+ `:265-267`（`RTS_VMINHIBIT` 抑制）。`rprocpub` 结构见 `minix3/minix/include/minix/rs.h:rprocpub`（`label` / `endpoint` / `in_use` 三栏是本篇用的全部）。

---

## 3 Rust 设计决策

| # | 决策 | C 做法 | Rust 做法 | 为什么 |
|---|------|--------|-----------|--------|
| D1 | 三步拆三函 | 一个 `sef_cb_init_fresh` 内联全做 | `reset_tables` / `map_service` / `apply_boot_map`（`boot.rs`） | 清、登、批是三个可测单元：清是纯置空，登是单条 verdict，批是循环+失败即停；绑一起只能测"全过/全不过"，拆开每步可独立断言 |
| D2 | 清用全清 | 只清 flags（`:261-266`） | `store.fill(None)` / `subs.fill(None)` | fresh 启动时内存本就是零，全清与只清旗同果；且全清与 `free_sub_slot` 的 `None` 语义统一（04 D5），一种"空"胜过两种 |
| D3 | 拷贝显式定界 | `strcpy` 三处（`:240,242`） | `copy_label` 三止（NUL / 16 / 79）+ `RS_OWNER_LANE` 常量 | `strcpy` 信任源端有界（RS 表），显式三止让信任变契约；label 上界 16 来自 `RS_MAX_LABEL_LEN`，不是 80——用错界会把服务名截错地方 |
| D4 | 通知环留钩不定空线 | `map_service` 末尾调 `update_subscribers`（`:246`） | `map_service` **不调**，钩子文档化（`boot.rs` D5） | 通知环（10）在实现顺序上还没落地；调一个空函数等于撒谎，留钩子加文档等于诚实。接线时在 `apply_boot_map` 之后统一扫一轮，语义等价（启动时订阅表恒空，首轮本就是空转） |
| D5 | 传输层剥离 | `sys_safecopyfrom` 内联（`:269`） | `apply_boot_map` 收 `&[BootService]` 切片 | grant 拷贝是传输（02/12），"逐个登记"是业务；切片把"我拿到表了"和"我怎么拿到的"分开，业务可纯测 |
| D6 | 状态迁移显式化（A-6） | `sef_llvm_ds_st_init` 魔法插桩原地遍历静态内存 | `sef.rs::export_state`/`import_state`：条目表按 C 192B 规范布局（宽臂 data 栏在途写**池槽号**，导入侧重钉指针）、订阅表版本化记录（"DSS1"）、池字节随行 | minix-rs 无 LLVM magic；malloc 堆在 C 靠魔法做指针转换，Rust 的池把"字节随行 + 槽号重钉"写进类型——迁移后检索同一字节，订阅 pending 位不丢（2026-09-15 落地，`sef.rs` state_tests 往返锁定） |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/ds/src/
├── boot.rs — 本篇：BootService / reset_tables / map_service / apply_boot_map
└── sef.rs  — 01：启动种类（Fresh 走本篇三步，RestartStateful 跳过）
```

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 单服务登记 | `store.c:229-249` | `boot.rs:89`（`map_service`） | 取槽 → 三栏 → label 旗（不跑通知环，D4） |
| 双表复位 | `store.c:261-266` | `boot.rs:59`（`reset_tables`） | 全置 `None`（D2） |
| 批量映射 | `store.c:273-279` | `boot.rs:120`（`apply_boot_map`） | 逐在用项登记，首错即停（首错码即返回值，与 C 的 panic-on-first-failure 同"停"，不同"崩"——见 §5 注） |
| 服务描述 | `minix3/minix/include/minix/rs.h:rprocpub` | `boot.rs:45`（`BootService`） | 在用位 + 端点 + 名（三栏即 C 所用的全部） |

注：C 在批量映射失败时 `panic`，Rust 停并返回首个错误码。启动锚点失败确实没有降级，但"停并上报"比"崩"给 supervisior（RS）更多信息——RS 才能决定是重试还是 abort。这是运行 Robustness 对启动 panic 的收敛，文档化在此，测试锁定行为（`test_apply_boot_map_stops_at_first_error` 类）。

### 4.3 不变量

| 不变量 | 守卫 | 证据 |
|--------|------|------|
| 登记项恒 label | `flags = IN_USE\|TYPE_LABEL` 定死 | `store.c:243` |
| 登记主恒 rs | `owner` 定死 `"rs"` | `store.c:242` |
| 复位后全空 | `fill(None)` | `store.c:261-266` |

---

## 5 测试要点

> 基线：`cargo test -p minix-ds --lib`，本篇 7 个测试。

| 测试名 | 覆盖 C 位置 | 行为 |
|--------|-------------|------|
| `test_reset_clears_both_tables` 类 | `store.c:261-266` | 复位后双表全空 |
| `test_map_service_lanes` 类 | `store.c:240-243` | 三栏 + label 旗 |
| `test_map_service_full_is_enomem` 类 | `store.c:235` | 满即 `ENOMEM` |
| `test_apply_boot_map_*` 类 | `store.c:273-279` | 批量登记 + 首错即停 |

---

## 6 过渡

启动映射讲完了：DS 表里的第一批数据是 RS 背书的 label。下一站 07——运行时 handler 之首：发布（谁能立、立哪、能不能盖）。

## 7 参见

- C 源：`minix3/minix/servers/ds/store.c:map_service`、`kernel/table.c:44-64`、`include/minix/rs.h:165-183`
- 阶段文档：`05-ds-identity-auth.md`（上一站）、`07-ds-publish.md`（下一站，label 发布规则的运行版）
- Rust 实现：`os/servers/ds/src/boot.rs`
- 对端：`../03-stage-rs/02-rs-process-table.md`（`rprocpub` 的另一面）
