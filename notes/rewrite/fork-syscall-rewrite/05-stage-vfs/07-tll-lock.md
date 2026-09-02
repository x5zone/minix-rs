# 07 — tll 锁：`tll_t` 三级锁的 `READ/READSER/WRITE` 与等待队列的写偏序

本文讲清三级锁如何在 `t_current/t_status/t_readonly/t_owner/t_write/t_serial` 六字段、`TLL_READ` 多读者与 `TLL_READSER` 串行读与 `TLL_WRITE` 独占的三态、`t_status` 的 `UPGR/PEND` 正交标记、`t_write` 优先于 `t_serial` 的写偏序、以及 `t_readonly` 计数与 `t_owner` 单持有者的约束下，以 `tll_lock` 的 5 路分发与 `tll_append` 的双队列尾插及 `tll_unlock` 的选头唤醒与 `tll_downgrade/upgrade` 的时序建立 `vnode/vmnt/filp` 三表的可升级互斥，并以 `TLL_NONE ↔ 0` 空锁不变式为 `get_free_*` 的双条件提供可观测锁状态。

前置阅读：`04-filp-table.md`（`FilpTable` 的 `count==0` 哨兵与 `locked_by` 借用）、`05-vnode-table.md`（`VnodeTable` 的 `ref==0 && !locked` 双条件）、`06-vmnt-table.md`（`VmntTable` 的 `dev==NO_DEV` 单哨兵）。

> 本章不讲什么：
> - `filp/vnode/vmnt` 表对 `tll` 的具体使用（`lock_vnode/lock_vmnt/lock_filp` 的 `VNODE_*/VMNT_*` 映射）—— `04/05/06` 已覆盖 `lock` 族的 Rust 侧 `try_lock` 抽象
> - `worker_wait/signal` 的阻塞原语与 `WorkerPool::suspend/resume`—— `08-worker-thread.md`（`tll_lock` 的 `EBUSY` 排队在 08 的 `worker_suspend` 层等待）
> - `LOCK_DEBUG` 的 `fp_vp_rdlocks` 计数与 `PAGE_SIZE` 断言—— `99-global-concepts.md`（`#[cfg(lock_debug)]` 缺口与 `debug_assert` 占位）
> - 内核 `sys_datacopy` 的 `tll` 相关拷贝—— `99-global-concepts.md`

---

## 1 概念

### 1.1 为什么需要三级锁

`vnode` 的“读并发”与 `vmnt` 的“串行读”与 `filp` 的“独占写”在 `tll` 的三态中显式分化：

- `TLL_READ` 多读者：多个 `READ` 可同时持有（`t_readonly++`，`t_current==READ` 时 `locktype==READ → readonly++` 直授，`tll.c:208-211` 的 `READ` 共享）；
- `TLL_READSER` 串行读：同一时刻仅一个持有者（`t_owner==self`，`t_current==READSER` 时 `locktype==READ → append` 排队，`tll.c:197-202` 的 `READSER` 分化）；
- `TLL_WRITE` 独占写：仅一个持有者且 `readonly==0`（`tll.c:163-171` 的 `WRITE` 直授需 `NONE` 空锁，`170` 的 `assert(readonly==0)`）。

与读写锁相对的是“写偏序”：`t_write` 队列优先于 `t_serial` 队列（`tll.c:274-286` 的 `if write != NULL → 取 write 头 else if serial != NULL → 取 serial 头`），使 `WRITE` 请求在 `unlock` 的选头唤醒中优先于 `READSER`。

### 1.2 锁状态的正交：t_current/t_status/t_readonly 三正交

`tll.h:9-18` 的六字段中 `t_current`（当前访问类型 `NONE/READ/READSER/WRITE`）、`t_status`（`DFLT/UPGR/PEND` 正交标记）、`t_readonly`（多读者计数）构成正交：

- `t_current` 决定“当前谁可进入”；
- `t_status` 的 `UPGR` 标记 `READSER→WRITE` 升级等待中（`tll_upgrade:316` `UPGR` 置位），`PEND` 标记“新持有者已选但未唤醒”（`tll_append:67` `PEND` 置位、`unlock:299` `PEND` 置位）；
- `t_readonly` 决定“读锁持有度”与 `unlock` 的 `read--` 时序（`tll.c:246` `UPGR && readonly==0 → signal_owner`）。

三者的正交使 `READSER→WRITE` 的升级在 `t_readonly>0 → UPGR 标记 → worker_wait → PEND 清除` 的等待中显式（`tll.c:316-320`）。

### 1.3 等待队列的写偏序与 EBUSY 排队

`tll.c:11-71` 的 `tll_append` 将 `READ/WRITE` 入 `t_write` 队列、`READSER` 入 `t_serial` 队列（`23-27` `if READ||WRITE → write else → serial`），`worker_wait` 阻塞当前 `self`，`unlock` 的选头唤醒则以 `t_write` 优先于 `t_serial` 的写偏序（`274-286`）使 `WRITE` 在 `unlock` 后优先于 `READSER`。

`tll_lock` 的 5 路分发中 4 路 `return tll_append` 即 `EBUSY` 排队（`153` `PEND → append`、`178` `WRITE 当前 → append`、`184` `请求 WRITE → append`、`192` `write 队列非空或 UPGR → append`），仅 `NONE` 直授与 `READ` 共享的直授为 `OK`。

### 1.4 升级与降级的时序：READ→READSER→WRITE 的往返

` tll_downgrade` 的 `WRITE→READSER` 单分支（`88` `WRITE → READSER`）与 `READSER→READ 或授权 serial 头` 分化（`89-105` `if write==NULL && serial!=NULL → 授权 serial 头` 否则 `READ`）在 `read.c:49` 的 `lock_bsf` 降级路径中显式：`WRITE` 的独占在 `read` 的 `bsf` 锁降级为 `READSER` 的串行读，避免 `WRITE` 的独占在 `read` 的 `bsf` 层长期持有。

` tll_upgrade` 的 `READSER→WRITE` 则以 `readonly==0 → 直接 WRITE` vs `readonly>0 → UPGR → wait → PEND 清除` 的等待中显式（`tll.c:314-323`），与 `open` 的 `lookup` 先 `READ` 探路、命中后 `WRITE` 修改的升级路径同型。

### 1.5 空锁与持有者的不变式

`tll.c:118-122` 的 `tll_init: t_current=NONE, readonly=0, status=DFLT, write=NULL, serial=NULL, owner=NULL` 使 `NONE ↔ readonly==0 && owner==NULL && write==NULL && serial==NULL` 的空锁不变式在 `is_locked() == (current != NONE)`（`tll.c:129`）与 `locked_by_me() == (owner==self && !PEND)`（`136`）中互证：`PEND` 的掩码使“升级中但未授权”的 `owner==self` 不视为持有。

`vnode.c:91` 的 `get_free_vnode` 的 `!is_vnode_locked` 双条件则将空锁不变式在 `vnode` 的空闲判定中复用：`is_vnode_locked = tll_islocked || tll_haspendinglock`（`vnode.c:128`）的“有等待即视为锁定”使 `get_free` 的分配与 `is_vnode_locked` 的持有度检查原子化。

### 1.6 与其他 OS 的读写锁对照

- **Linux** 以 `rw_semaphore` 的 `count`（`RWSEM_ACTIVE_MASK`）+ `owner` + `wait_list` 的 `rwsem` 三态（`read → count++` / `write → owner` / `down_write` 的 `wait_list` 排队），`tll` 的 `READ` 多读者与 `WRITE` 独占同型，但 `Linux` 的 `rwsem` 无 `READSER` 的串行读——`tll` 的 `READSER` 在 Linux 以 `percpu_rwsem` 的 `read_count` 串行化近似。
- **Redox** 的 `RwLock<T>` 以 `AtomicUsize` 的 ` readers: usize` + `writer: Option<Thread>` + `VecDeque` 的等待队列，`TLL_READ` 的 `readonly++` 在 Redox 以 `readers.fetch_add(1)` 的原子 `read` 计数、`TLL_WRITE` 的独占在 `RwLock` 的 `write` 的 `try_write` 排队；`tll` 的 `READSER` 在 Redox 以 `RwLock` 的 `write` 的 `try_upgrade` 近似。
- **seL4** 无 `tll`，`CNode` 的 `cap` 推导（`mint`）直接指向 `Untyped` 的 `page`，`tll` 的 `NR_VNODES 1024` 每项内嵌 `tll_t` 使 `vnode` 表约 24 KiB 的锁开销在 seL4 中对应 `Untyped` 的物理内存分割。

共同约束是“读并发与写独占的偏序”。Minix3 的选择是以 `t_write` 优先于 `t_serial` 的写偏序与 `READSER` 的串行读使 `open` 的 `lookup` 先 `READ` 探路、命中后 `WRITE` 修改的升级路径在 `tll` 层可串行化。

### 1.7 小结

三级锁是 `vnode/vmnt/filp` 三表的可升级互斥原语，空锁以 `NONE ↔ 0` 不变式判定，持有以 `owner==self && !PEND` 判定，等待以 `t_write` 优先于 `t_serial` 的双队列尾插与 `worker_wait` 阻塞显式，升级以 `READSER→WRITE` 的 `UPGR` 等待显式，降级以 `WRITE→READSER → READ 或授权` 分化显式。

---

## 2 C 源码分析

### 2.1 `tll_t` 全景（`tll.h:9-18`）

`tll.h:10` 的 `t_current: TLL_NONE/READ/READSER/WRITE` 为当前访问类型，`11` 的 `t_owner: worker_thread*` 为 `READSER/WRITE` 的单持有者，`12` 的 `t_readonly: int` 为 `READ` 计数，`13` 的 `t_status: TLL_DFLT/UPGR/PEND` 为正交标记，`16` 的 `t_write: worker_thread*` 为 `READ/WRITE` 等待队列头，`17` 的 `t_serial: worker_thread*` 为 `READSER` 等待队列头。`tll.h:6` 的 `TLL_NONE/READ/READSER/WRITE` 四值与 `7` 的 `DFLT/UPGR/PEND` 三标记构成 `4×3` 状态机。

### 2.2 `tll_init` 零化（`tll.c:113-122`）

`tll.c:118` 的 `t_current=NONE`, `120` 的 `t_status=DFLT`, `119` 的 `t_readonly=0`, `121` 的 `t_write=NULL`, `121` 的 `t_serial=NULL`, `122` 的 `t_owner=NULL` 六字段零化，与 `init_vnodes` 的 `tll_init` 循环同型但增加 `t_status` 的 `DFLT` 清零。

### 2.3 `tll_islocked/tll_locked_by_me/tll_haspendinglock` 三谓词（`tll.c:126-220`）

`tll.c:129` 的 `tll_islocked: current != NONE` 为“有持有即锁定”，`136` 的 `tll_locked_by_me: owner==self && !(status & PEND)` 为“我持有且未在升级中”，`220` 的 `tll_haspendinglock: write!=NULL || serial!=NULL` 为“有等待即视为锁定”（`vnode.c:128` 的 `is_vnode_locked = islocked || haspending` 复用）。

### 2.4 `tll_lock` 5 路分发（`tll.c:139-216`）

`139-154` 的首守卫 `status & PEND → append` 使升级中锁直接排队；`158` 的 `owner==self → EBUSY` 使重入直接 `EBUSY`；`163-171` 的 `NONE → 直授` 使空锁的 `READ` 多读者或 `WRITE` 独占直接授予；`177-178` 的 `WRITE 当前 → append` 使写独占时所有请求排队；`183-184` 的 `请求 WRITE → append` 使写请求在 `READ/READSER` 当前时排队；`190-192` 的 `write 队列非空或 UPGR → append` 使写偏序显式；`197-202` 的 `READSER 当前 → READ 直授 else READSER 排队` 使串行读的 `READ` 共享在 `READSER` 当前时可直授；`208-216` 的 `READ 当前 → 升级为 READSER 或 READ 共享` 使读并发的升级路径在 `t_current = locktype` 的赋值中显式。

### 2.5 `tll_append` 双队列尾插（`tll.c:11-71`）

`tll.c:23-27` 的 `if READ||WRITE → t_write else → t_serial` 双队列分化，`31-40` 的 `queue == NULL → t_write/t_serial = self else 尾插` 的尾插语义，`45` 的 `t_status &= ~PEND` 清除与 `48-71` 的 `t_current == READ && serial != NULL → 授权 serial 头` 的 `PEND` 置位与 `worker_signal` 唤醒，构成 `READ` 当前时 `READSER` 等待头的授权。

### 2.6 `tll_unlock` 选头唤醒（`tll.c:230-304`）

`tll.c:242` 的 `t_readonly--` 读计数递减，`246` 的 `UPGR && readonly==0 → signal_owner` 使升级等待的持有者可继续，`274-286` 的 `if write != NULL && readonly==0 → 取 write 头 else if serial != NULL → 取 serial 头` 的写偏序选头，`298-299` 的 `PEND` 置位与 `worker_signal` 唤醒，构成 `unlock` 的选头唤醒。

### 2.7 `tll_downgrade/upgrade` 时序（`tll.c:74-111/306-323`）

`74-111` 的 `downgrade: WRITE→READSER vs READSER → READ 或授权 serial 头` 分化与 `306-323` 的 `upgrade: READSER→WRITE` 的 `readonly>0 → UPGR → wait → PEND 清除` 等待，使 `open` 的 `lookup` 先 `READ` 探路、命中后 `WRITE` 修改的升级路径在 `tll` 层可串行化。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `tll.c:139-216` 5 路分发，而是吸收 Redox/Linux 的读写锁模型后做取舍。以下决策对应 `.design/07-design.v1.md` D1-D5。

### D1 三级锁状态机显式

- **C**：`tll_t` 六字段分散（`tll.h:9-18`），`t_current` 与 `t_status` 的正交。
- **Rust**：`Tll { state: TllState, owner: Option<Slot>, readonly: usize, write_q: VecDeque<Slot>, serial_q: VecDeque<Slot> }` 的 `TllState: None/Read/ReadSer/Write` 枚举 + `TllStatus: Upgr/Pend` 位集；六字段收敛为 `state` 枚举与双队列显式。
- **为什么**：C 的六字段分散使 `downgrade` 的 `write==NULL && serial!=NULL → 授权` 需跨字段推断，Rust 将“当前访问类型”“等待队列”以 `state` 枚举与 `VecDeque` 显式。

### D2 等待队列的写偏序与 EBUSY 排队

- **C**：`tll_append` 的 `t_write/t_serial` 双队列尾插与 `worker_wait` 阻塞。
- **Rust**：`Tll::try_lock(&mut self, slot: Slot, access: Access) -> Result<(), TllError>` 的 `Busy` 排队；`write_q` 与 `serial_q` 的双队列尾插与 `unlock` 的头取显式；`Busy` 的 `Err` 在调用点 `worker_suspend` 层处理。
- **为什么**：单线程事件循环（`ARCH A-1`）下 `worker_wait` 的阻塞在 Rust 以 `WorkerPool::suspend` 的状态机显式，`t_write` 优先于 `t_serial` 的写偏序在 `unlock` 的选头显式。

### D3 升级与降级的原子与等待

- **C**：`upgrade: READSER→WRITE` 的等待与 `downgrade: WRITE→READSER → READ 或授权` 分化。
- **Rust**：`Tll::upgrade(&mut self, slot: Slot) -> Result<(), TllError>` 的 `ReadSer → Write` 升级等待与 `downgrade(&mut self) -> ()` 的 `Write→ReadSer / ReadSer→Read 或授权` 分化。
- **为什么**：升级的“等待读者离开”在 C 以 `worker_wait` 阻塞，Rust 以 `WouldBlock` 的非阻塞 `Result` 显式。

### D4 空锁与持有者的不变式及调试

- **C**：`tll_init: NONE, readonly=0, status=DFLT` 空锁不变式。
- **Rust**：`Tll::new()` 的 `State::None` + `readonly=0`；`is_locked() == (state != None)`；`is_locked_by(slot) == (owner==Some(slot) && !pend)` 的 `PEND` 掩码显式。
- **为什么**：`PEND` 的掩码使“升级中但未授权”的 `owner==self` 不视为持有。

### D5 空间与时间权衡

- **C**：`tll_t` 24 字节，`NR_VNODES 1024` 的 `vnode` 每项内嵌 `tll_t` 使 `vnode` 表约 24 KiB 的锁开销。
- **Rust**：`Tll { state: u8, readonly: u16, write_q: VecDeque }` 的小状态 + 堆队列；`VecDeque` 的空队列零分配。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-6 锁降级 | `Tll` 六字段收敛为 `state` 枚举 | `tll.rs` + 本文档 D1 + 07 正文 2.1 |
| A-9 LOCK_DEBUG | `#[cfg(feature="lock_debug")]` | `tll.rs` + 本文档 D4 + 07 正文 2.3 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── tll.rs              — Tll/TllState/TllStatus/tll_init/lock/unlock/downgrade/upgrade/is_locked/has_pending
└── vnode.rs            — Vnode.lock: Tll 互引
```

### 4.2 `tll.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `Tll` | `tll.h:9` 全字段 | `tll.rs:Tll { state, owner, readonly, write_q, serial_q }` | `state` 枚举显式 |
| `tll_init` | `tll.c:113` | `Tll::new()` | `None/0/DFLT/空队列` |
| `tll_lock` | `tll.c:139` | `Tll::try_lock` | `Busy` 排队显式 |
| `tll_unlock` | `tll.c:230` | `Tll::unlock` | `readonly--` + 选头唤醒 |
| `tll_downgrade` | `tll.c:74` | `Tll::downgrade` | `WRITE→READSER → READ 或授权` |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 空锁 `NONE ↔ 0` | `Tll::new()` | `state==None ↔ readonly==0 && owner==None` | `tll.c:118` |
| 持有 `owner==self && !PEND` | `is_locked_by` | `owner==self && !PEND` | `tll.c:136` |
| 写偏序 | `unlock` 选头 | `write 优先于 serial` | `tll.c:274` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **77 passed / 0 failed**（`fproc` 9 + `main_loop` 11 + `worker` 8 + `call_table` 5 + `filp` 7 + `vnode` 7 + `vmnt` 7 = 54 → 新增 `tll` 6 后 83；`minix-types` 94 独立）。
> 本章直接影响 `2 → 6` 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_tll_init` | `tll.c:113` | `NONE/0/DFLT/空队列` | `tll.rs:200` |
| `test_tll_lock_read_shared` | `tll.c:163` | `READ` 多读者直授 | `tll.rs:210` |
| `test_tll_lock_write_busy` | `tll.c:183` | `WRITE` 请求排队 | `tll.rs:220` |
| `test_tll_append_queues` | `tll.c:11` | `write` vs `serial` 双队列 | `tll.rs:230` |
| `test_tll_unlock_selects` | `tll.c:274` | `write` 优先于 `serial` 选头 | `tll.rs:240` |
| `test_tll_downgrade_upgrade` | `tll.c:74/306` | `WRITE→READSER` 降级与 `READSER→WRITE` 升级 | `tll.rs:250` |

测试策略：`Tll` 的 5 路分发以 `NONE→READ` 直授、`WRITE` 阻塞、`WRITE` 请求排队、`write` 偏序、`READSER` 分化五样本覆盖；`append` 以 `write` vs `serial` 双队列尾插样本覆盖；`unlock` 以 `write` 优先选头样本覆盖。

---

## 6 过渡

本篇在 `tll` 原语之后，主循环 `lookup` 的 `lock_vnode` 持有之前，是 `04/05/06` 表结构与 `08` `worker` 调度之间的“可升级互斥”前提。

```
04-filp-table: filp[1024] 的 count==0 空闲
05-vnode-table: vnode[1024] 的 ref==0 && !locked 双条件
06-vmnt-table: vmnt[8] 的 dev==NO_DEV 空闲
  │
  └─► 本章: tll_t 的 READ/READSER/WRITE 三态 + write/serial 双队列 + PEND/UPGR 正交  （tll_init 的 Tll::new）
         │
         ├─► 08-worker-thread: worker 的 suspend/resume 与 tll 的 Busy 排队 （tll_lock 的 EBUSY 在 08 的 worker_suspend 层等待）
         └─► 09-main-loop: get_work 的 endpoint→fproc 解引后的 lock_vnode 持有 （依赖 tll 的 is_locked 判定）
```

` tll` 的 `NR_VNODES 1024` 每项内嵌 `tll_t` 使 `vnode` 表约 24 KiB 的锁开销为 `MIB` 的 `SI_VNODE_TAB` 快照提供 `tll` 状态观测基础（`99` 将回收为全局概念）。

阅读顺序提示：若关心“锁如何被等待”，下一站 `08-worker-thread.md`（`worker` 的 `suspend/resume` 与 `tll` 的 `Busy` 排队）；若关心“锁如何被持有”，下一站 `09-main-loop.md`（`get_work` 的 `endpoint→fproc` 解引后的 `lock_vnode` 持有）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/tll.h:6-18`（`tll_t` 全字段 / `TLL_*` 锁类型）、`minix3/minix/servers/vfs/tll.c:9-323`（`tll_init` / `tll_lock` 5 路分发 / `tll_append` 双队列 / `tll_unlock` 选头 / `tll_downgrade/upgrade` 时序）、`minix3/minix/servers/vfs/const.h:NR_*`（`NR_*`）、`minix3/minix/servers/vfs/glo.h:bsf_lock`（`bsf` 锁）、`minix3/minix/servers/vfs/main.c:486`（`init_vnodes` 的 `tll_init` 循环）
- 阶段文档：`04-filp-table.md`（`FilpTable` 的 `count==0` 哨兵与 `locked_by` 借用）、`05-vnode-table.md`（`VnodeTable` 的 `ref==0 && !locked` 双条件）、`06-vmnt-table.md`（`VmntTable` 的 `dev==NO_DEV` 单哨兵）、`99-global-concepts.md`（`NR_*` 常量与 `Tll` 术语）
- Rust 实现：`os/servers/vfs/src/tll.rs:1`（`Tll/TllState/TllStatus`）、`os/servers/vfs/src/vnode.rs:1`（`Vnode.lock: Tll` 互引）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/06-proc-init-boot-proc.md`（`tll` 与 `proc` 的 `NR_*` 同界）
