# 03 — fproc 表：`fproc[NR_PROCS]` 与 `isokendpt` 的三守卫

本文讲清 VFS 的进程表如何在 `NR_PROCS` 固定大小、`PID_FREE 0` 空闲哨兵、`isokendpt` 三守卫、`okendpt/isokendpt` 致命分化的约束下，以 `fproc[NR_PROCS]` 的槽位索引与 `endpoint = generation·slot` 的双重编码建立 `slot → fproc` 的可信映射，并以 `fproc_light[NR_PROCS]` 的只读投影为 `MIB` 观测提供低成本快照。

前置阅读：`02-fproc-struct.md`（`FProc` 字段与 `BlockedOn` 枚举）、`01-vfs-init-main.md`（`sef_cb_init_fresh` 的两遍初始化时序）。

> 本章不讲什么：
> - `FProc` 各字段含义（`fp_flags` 六位、`fp_blocked_on` 七态、`fp_u` 五 union、凭证五字段）—— `02-fproc-struct.md`
> - `FProc` 的 `filp` 数组与 `cloexec` 位图—— `04-filp-table.md`
> - `MIB` 拉取轻表的 `sys_datacopy` 两段与 `vm_vfs_procctl_handlemem` 重试—— `99-global-concepts.md`（`sys_datacopy_wrapper` 归 99）
> - 内核 `d_endpoint → slot` 的 `ENDPOINT_P` 生成与 `generation` 回绕—— `../01-stage-kernel/06-proc-init-boot-proc.md`

---

## 1 概念

### 1.1 为什么需要固定大小的进程表

VFS 保留 `fproc[NR_PROCS]`，每个用户进程恰好占据一个槽位，槽位号与内核 `proc` 的槽位号严格同下标。`NR_PROCS` 在 Minix3 的 `sys/config.h` 与内核 `proc.h` 同源（256），而 `fproc.h:11` 注释 *NR_PROCS must be the same as in the kernel. It is not possible or even necessary to tell when a slot is free here.* 正是此同界约束的自述。

与数组相对的是哈希或树。VFS 选择数组的三个理由在 `glo.h:26-28` 的两宏中显式：

- `fproc_addr(e)  (&fproc[_ENDPOINT_P(e)])` —— 只要拿到端点就能 `O(1)` 拿到 `fproc*`，无需搜索；
- `who_p  ((int)(fp - fproc))` —— 拿到 `fproc*` 后反向算槽位亦 `O(1)`；
- `NR_PROCS` 作为 `mproc`/`fproc`/`vmproc` 三表的外部进程上界，使 `endpoint` 的低 15 位槽位号在三服务间统一。

代价是“空闲如何表示”。VFS 用两个相互印证的哨兵而非单个布尔。

### 1.2 空闲哨兵的经济学

`fproc.h:103` 定义 `#define PID_FREE 0`——`pid 0` 永不分配给用户进程，因此 `fp_pid == 0` 即“此槽空闲”。`glo.h:27` 的 `fproc_addr` 与 `utility.c:108-109` 的 `fproc[*proc].fp_endpoint == NONE` 检查则构成第二哨兵：`Endpoint::NONE` 同样永不分配。

`FProc::new_unused()` 同时清零两者（`pid=PID_FREE, endpoint=NONE`），与 `main.c:405-408` 的第一遍初始化 `rfp->fp_pid=PID_FREE; rfp->fp_endpoint=NONE;` 原子化。`utility.c:108-109` 的 `ke==NONE → assert(pid==PID_FREE)` 与 `111-114` 的 `ke!=endpoint → assert(pid!=PID_FREE)` 正是双哨兵互证的运行时检查：若端点为 `NONE`，则 pid 必须为 `0`；若端点已知但与传入端点不一致，则 pid 必须非 `0`（槽正被占用）。

`PID 0` 的选择不是偶然：它与 `Endpoint::NONE` 的位模式不同源（`Endpoint` 的低 15 位槽位 + 高位 generation），但“0 即空闲”的约定使 `FProc::is_in_use() = pid != PID_FREE` 的单分支即可判定存活性（`02` 已在 `fproc.rs:359` 实现）。

### 1.3 端点验证的信任边界

用户通过系统调用传入的 `endpoint_t endpoint` 来自 `m_in.m_source` 或消息负载，其低 15 位声称“我来自槽位 N”。VFS 不能信任它——槽位可能越界，或槽位虽合法但存储的端点 `ke = fproc[N].fp_endpoint` 与传入端点 `endpoint` 不一致（进程已退出、槽位已复用、或消息伪造）。

`utility.c:92-123` 的 `isokendpt_f(file,line,endpoint,proc,fatal)` 以三守卫顺序显式处理：

1. `endpoint == NONE` → 失败：`NONE` 永不作为合法发端点（`main.c:622` 的 `fproc[who_p].fp_endpoint == NONE` 守卫同型）。
2. `*proc = _ENDPOINT_P(endpoint); *proc <0 || *proc >= NR_PROCS` → 失败：槽位越界（`NR_PROCS` 外的位模式）。
3. `ke = fproc[*proc].fp_endpoint; ke != endpoint` → 失败：槽位端点与传入端点不一致。

第三守卫的诊断进一步区分“空闲槽”与“占用但失配”：前者断言 `pid==PID_FREE`，后者断言 `pid!=PID_FREE`。这正是双哨兵在验证路径上的收益。

### 1.4 致命 vs 非致命：调用点意图决定错误处理

`proto.h:357-358` 的两宏将同一函数按 `fatal` 分流：

- `#define okendpt(e,p) isokendpt_f(__FILE__,__LINE__,e,p,1)` —— 致命，失败即 `panic("isokendpt_f failed")`（`utility.c:119-120`）。
- `#define isokendpt(e,p) isokendpt_f(__FILE__,__LINE__,e,p,0)` —— 非致命，失败返回 `EDEADEPT`。

调用点统计显示分岔意图：

- `cdev.c:81/444`、 `dmap.c:155/209`、 `mount.c:123` 的 `isokendpt → EINVAL` 将验证失败转为面向用户的 `EINVAL/EDEADEPT`；
- `main.c:901` 的 `worker_start(fproc_addr(PM_PROC_NR), pm_reboot, …)` 前的 `fproc_addr` 则以 `okendpt` 保证 `PM_PROC_NR` 必合法，否则直接 `panic`；
- `misc.c:405` 的 `isokendpt(ep) ? rfp=NULL : rfp=&fproc[slot]` 则将验证失败转为 `NULL` 分支（观测路径）。

因此 Rust 将两者分化为 `is_ok_endpoint(&self, ep) -> Result<UserSlot, FprocError>` 与 `ok_endpoint(&self, ep) -> UserSlot`（后者 `expect` 含 `file!():line!()` 等价 `__FILE__:__LINE__` 诊断）。

### 1.5 fproc_light：观测的低成本投影

`fproc.h:111-115` 的 `fproc_light[NR_PROCS]` 为 `MIB` 服务设计的只读投影：

```c
EXTERN struct fproc_light { dev_t fpl_tty; int fpl_blocked_on; endpoint_t fpl_task; } fproc_light[NR_PROCS];
```

`misc.c:55-96` 的 `do_getsysinfo` 展示其用途：`SI_PROC_TAB` 先拷全表 `fproc`，再拷 `fproc_light`（`95-96` `fproc_light` 的 `sizeof` 拷贝）。投影的动机是“观测成本”：全表约 1.1 MiB，轻表仅 `3×8×256 ≈ 6 KiB`，一次 `sys_datacopy` 即可满足 `MIB` 的轮询。

在 minix-rs 中 `fproc_light` 的同步未实现（`ARCH A-7` 缺口）。`01` 的 `fproc::reset_all` 阶段已将 `fproc_light` 的生成标注为 `#[cfg(feature = "fproc_light")]` 的 `snapshot_light()` 占位——结构契约保留，`sys_datacopy` 的复制机制延后（`utility.c:142-186` 的 `sys_datacopy_wrapper` 的 VM 重试归 99）。

### 1.6 与其他 OS 的进程表对照

- **Linux** 以 `pid_hashtable`（`PIDTYPE_PID` 哈希）+ `task_struct` 的 `pid` 树组织进程，`find_get_pid(pid)` 需哈希查找，`struct pid` 的 `generation` 隐式于 `nsproxy`；VFS 的 `fproc[256]` 则以端点低位直接索引，`O(1)` 但上限固定。
- **Redox** 的 `Scheme` 以 `Slab<Process>` 的 arena 组织，`Pid` 为 `Slab` 索引 + `generation`，分配为 `insert()` 返回 `Pid`，验证为 `get(pid)` 的 `Option`；Minix3 的 `fproc[NR_PROCS]` 则是静态 BSS 数组，分配为 `fp_pid=PID_FREE` 扫描，验证为 `isokendpt` 三守卫。
- **seL4** 无固定进程表，`TCB` 为显式能力，验证为 `cap` 推导（`mint`）而非槽位范围检查；`fproc` 的 `NR_PROCS` 上界在 seL4 中对应 `Untyped` 的物理内存分割。

共同约束是“验证必须在解引前完成”。Minix3 的选择是以固定大小与双哨兵换取“索引即权威”的零分支解引（`fproc_addr` 宏的 `&fproc[slot]` 直接索引）。

### 1.7 小结

VFS 的进程表是 `NR_PROCS` 固定数组，空闲以 `PID_FREE 0` 与 `NONE` 端点双哨兵标记，验证以 `isokendpt` 三守卫（`NONE`、越界、端点不一致）守门，调用点按致命性分 `okendpt`（panic）与 `isokendpt`（`EDEADEPT`）二路，观测以 `fproc_light` 轻投影低成本快照。下一节以 `utility.c:92` 三守卫为主线逐行核对。

---

## 2 C 源码分析

### 2.1 `fproc[NR_PROCS]` 与 `fproc_light[NR_PROCS]` 的声明（`fproc.h:82/111`）

`fproc.h:82` 的 `EXTERN struct fproc fproc[NR_PROCS];` 与 `111` 的 `EXTERN struct fproc_light fproc_light[NR_PROCS];` 在 `glo.h` 的 `_TABLE` 宏展开后为 `extern`，在 `table.c` 的 `#define _TABLE` 后为定义（`EXTERN` → 空的技巧）。`NR_PROCS` 来自 `sys/config.h` 的 `256`，与内核 `proc.h` 的同值宏互证（`02` 已在 `fproc.h:11` 注释 *NR_PROCS must be the same as in the kernel*）。

`02` 已解释 `FProcTable.slots: Box<[FProc]>` 的堆语义（1.09 MiB 栈溢出修复），本节复用：`new()` 由 `(0..NR_PROCS).map(|_| FProc::new_unused()).collect()` 直接堆构造，`fproc_light` 同为 `Box<[FprocLight]>` 的占位。

### 2.2 `PID_FREE` 与 `REVIVING` 哨兵（`fproc.h:101-103`）

`fproc.h:101` `#define NOT_REVIVING 0xC0FFEEE` 与 `102` `#define REVIVING 0xDEEAD` 并非 `fp_pid` 的哨兵，而是 `fp_reviving` 的管道唤醒状态（`pipe.c:342` 的 `rp->fp_pid != PID_FREE && fp_is_blocked(rp)` 与 `593` 的 `rp->fp_pid != PID_FREE && reviving==REVIVING` 分支）。`103` 的 `PID_FREE 0` 才是空闲判据。`02` 的 `FProc::is_in_use() = pid != PID_FREE` 单分支与 `main.c:405-408` 的 `rfp->fp_pid == PID_FREE` 扫描同语义。

### 2.3 `isokendpt_f` 三守卫（`utility.c:92-123`）

`utility.c:99` 的 `*proc = _ENDPOINT_P(endpoint)` 提取槽位（`minix/com.h` 的 `endpoint` 低 15 位槽位 + 高位 generation），随后三守卫顺序与 `01` 的 `main.c:606-613` 的 `proc_p <0 || >=NR_PROCS → fp=NULL` 同型，但 `isokendpt_f` 进一步以 `ke = fproc[*proc].fp_endpoint` 的端点一致性守门。`utility.c:108-114` 的双 `assert` 将“空闲则 `pid==0`、占用则 `pid!=0`”的双哨兵互证固化为调试断言（`NDEBUG` 为 `0` 时触发）。

`utility.c:100-101` 的 `endpoint==NONE → failed` 在 Rust 以 `ep == Endpoint::NONE → Err(BadEndpoint)` 首守卫对应；`103-106` 的越界 `printf` 在 Rust 以 `Err(BadSlot)` 的 `EDEADEPT` 无 `printf` 路径对应（`log::warn!` 可选）。

### 2.4 致命分化与 `panic`（`proto.h:357-358` / `utility.c:119-120`）

`utility.c:119` 的 `if(failed && fatal) panic("isokendpt_f failed")` 使 `okendpt` 的失败直接终止服务（`VFS` 为 `RS` 的受控服务，`panic` 将触发 `RS` 重启），而 `isokendpt` 的失败仅返回 `EDEADEPT` 供调用点转为 `EINVAL/EDEADEPT`。`lock.c:186` 的 `for (fptr=&fproc[0]; fptr<&fproc[NR_PROCS]; fptr++) if (fptr->fp_pid==PID_FREE) continue` 则是非致命路径的“跳过空闲”示例——验证失败的槽位不应 `panic`，而应跳过。

Rust 将两者分化为 `call_table.rs: is_ok_endpoint`（`Result`）与 `ok_endpoint`（`unwrap_or_else(|_| panic!(…))` 的 `file!():line!()` 诊断，等价 `__FILE__:__LINE__`）。

### 2.5 `who_p` 与 `fproc_addr` 宏（`glo.h:26-27`）

`glo.h:26` `#define fproc_addr(e) (&fproc[_ENDPOINT_P(e)])` 与 `27` `#define who_p ((int)(fp - fproc))` 的指针算术在 Rust 以 `FProcTable::at(ep) -> Option<&FProc>` 与 `fproc_table.slot_of(fp_ptr)` 封装（`Endpoint::to_user_slot() -> Option<UserSlot>` 已在 `minix_types` 实现，`UserSlot` 为 `0..255` 的 newtype）。`main.c:607` 的 `fp = &fproc[proc_p]` 与 `main.c:622` 的 `fproc[who_p].fp_endpoint == NONE` 则展示宏与哨兵的联动。

### 2.6 两遍初始化（`main.c:405-408` / `468-483`）

*已在 `02` 展开，本节引用以保证可独立阅读*：

- 第一遍（`sef_cb_init_fresh` 的 `405-408`）：`for (rfp=&fproc[0]; rfp<&fproc[NR_PROCS]; rfp++) { rfp->fp_pid=PID_FREE; rfp->fp_endpoint=NONE; }`——空闲语义原子化。
- 第二遍（`sef_cb_init_fresh` 的 `468-483`）：`for (rfp=&fproc[0]; rfp<&fproc[NR_PROCS]; rfp++) { for (i=0;i<OPEN_MAX;i++) rfp->fp_filp[i]=NULL; rfp->fp_rd=NULL; rfp->fp_wd=NULL; }`——目录锚点与 fd 表清零，`FProcTable::init_phase2()` 直接对应。

### 2.7 `fproc_light` 的轻观测（`fproc.h:111-115` / `misc.c:55-96`）

`fproc.h:111` 的 `EXTERN struct fproc_light { dev_t tty; int blocked_on; endpoint_t task; }` 为 `MIB` 投影的三字段（`fpl_tty`、`fpl_blocked_on`、`fpl_task`）。`misc.c:75` 的 `len = sizeof fproc * NR_PROCS` 与 `96` 的 `len = sizeof fproc_light` 的两段 `sys_datacopy` 在 Rust 以 `FprocLight { tty: DevId, blocked_on: BlockedOn, task: Endpoint }` 的 `CopyToUser` 抽象占位（`A-7` 缺口，`#[cfg(feature="fproc_light")]`）。

### 2.8 调用点举证（10+ `isokendpt` 用例）

`cdev.c:81/444` 的 `isokendpt(dp->dmap_driver) → EDEADEPT` 与 `isokendpt(proc_e) → EDEADEPT` 为驱动与用户两路；`dmap.c:155/209` 的 `isokendpt(endpoint) → EDEADEPT` 为驱动映射；`path.c:831` 的 `isokendpt(ep) → EINVAL` 为路径解析；`pipe.c:445` 的 `proc_e==NONE || isokendpt → return` 为管道唤醒的 `NONE` 守卫；`sdev.c:1035` 的 `isokendpt(req_id) → EDEADEPT` 为套接字。共同点是“先验证再解引”，与 `fproc.h:11` 的 *It is not possible or even necessary to tell when a slot is free here.* 的“通过端点验证而非 pid 扫描”一致。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `utility.c:92-123` 三守卫，而是吸收 Redox/Unix 的验证模型后做取舍。以下决策对应 `.design/03-design.v1.md` D1-D5。

### D1 三守卫验证显式分层

- **C**：`endpoint==NONE / slot 越界 / ke != endpoint` 三守卫 + `ke==NONE → pid==0` 互证。
- **Rust**：`FProcTable::is_ok_endpoint(&self, ep: Endpoint) -> Result<UserSlot, FprocError>` 的三守卫显式；`FprocError::BadEndpoint` → `EDEADEPT 78`（`minix_types::EDEADEPT`）；`Endpoint::to_user_slot() -> Option<UserSlot>` 先验越界；`fproc[slot].endpoint == ep` 再验一致；`is_in_use()` 加强空闲互证。
- **为什么**：`_ENDPOINT_P` 的位截断在 Rust 以 `to_user_slot()` 的 `Option` 类型化越界，`ke != ep` 的不一致在 Rust 以 `endpoint` 相等性直接表达，无需 `printf`。
- **备选**：单函数返回 `Option<&FProc>`；否决——无法回传 `UserSlot` 的槽位号供调用点复用（如 `mount.c:261` 的 `rfp = &fproc[slot]` 后续存 `slot`）

### D2 致命分化与错误码

- **C**：`okendpt → panic` vs `isokendpt → EDEADEPT` 的 `fatal` 分流（`proto.h:357-358`）。
- **Rust**：`ok_endpoint(&self, ep) -> UserSlot` 的 `expect` 含 `file!():line!()` 诊断，等价 `__FILE__:__LINE__`；`is_ok_endpoint` 的 `Err(FprocError::BadEndpoint)` → `to_errno() = EDEADEPT`；`misc.c:405` 的 `isokendpt → NULL` 观测路径则以 `Option` 而非 `Result` 表达。
- **为什么**：调用点意图决定 `panic` 与否；`bdev/cdev` 的 `EINVAL` 与 `PM` 的 `EDEADEPT` 分岔保留（`VFS_PM` 的 `ESRCH` 为例，验证失败≠致命）。
- **备选**：统一 `Result`；否决——`okendpt` 误用 `unwrap` 会丢失诊断，`panic` 是契约。

### D3 空闲语义与双哨兵

- **C**：`PID_FREE 0` + `Endpoint::NONE` 双哨兵（`main.c:405-408` 同时清零）。
- **Rust**：沿用 `FProc::is_in_use() = pid != PID_FREE`（`fproc.rs:359` 已有）与 `FProc::new_unused()` 双清零；`is_ok_endpoint` 的第三守卫以 `is_in_use()` 加强 `ke==NONE → pid==0` 互证的调试断言（`debug_assert!`）。
- **为什么**：`PID 0` 永不分配（与 PM 的 `NR_PIDS` 30000 分离），`Endpoint::NONE` 的位模式与 `PID_FREE` 不同源但“0 即空闲”约定在 Rust 以 `new_unused()` 原子化。

### D4 fproc_light 缺口与只读快照

- **C**：`fproc_light[NR_PROCS]` 三字段投影（`fproc.h:111-115`），`MIB` 拉取两段 `sys_datacopy`（`misc.c:75/96`）。
- **Rust**：`FprocLight { tty: DevId, blocked_on: BlockedOn, task: Endpoint }` + `FprocLightTable: Box<[FprocLight]>` 占位；`FProcTable::snapshot_light() -> Vec<FprocLight>` 标注 `#[cfg(feature="fproc_light")]` 缺口；`CopyToUser` trait 抽象 `sys_datacopy`。
- **为什么**：观测面非主路径，`MIB` 未在 minix-rs 推出；保留数据结构契约，同步机制延后（`ARCH A-7`）。

### D5 宏封装与表拥有

- **C**：`fproc_addr(e)` 宏直接索引 `fproc` 全局（`glo.h:27`）。
- **Rust**：`FProcTable::at(ep) -> Option<&FProc>`（`to_user_slot` 已在 `minix_types` 实现）与 `slot_of(fp: &FProc) -> UserSlot` 的指针算术封装（`fp as *const _ as usize` 差值/槽大小，非 `fp - fproc` 的裸指针减法）；`who_p` 宏在 Rust 为 `VfsState::caller_slot: Option<UserSlot>` 显式状态（`main_loop.rs:140` `current_fp_slot`）。
- **为什么**：全局 `fproc[]` 在 Rust 以 `VfsState.fproc_table` 聚合拥有（`ARCH A-4`），宏的裸索引在 Rust 以 `Option` 的 fail-closed 替代越界。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-8 64 位类型 | `Pid/Uid/Gid/Mode` 已在 02 落地，03 新增 `FprocError::BadEndpoint→EDEADEPT` | `minix_types::EDEADEPT` + 本文档 D1 + `fproc.rs` 注释 |
| A-3 阻塞枚举 | `BlockedOn` 已在 02 落地，03 复用 `is_in_use` 互证 | `fproc.rs:BlockOn` + 本文档 D1 + 03 正文 1.5 |
| A-7 轻表缺口 | `FprocLight` 占位 + `#[cfg(feature)]` | `fproc.rs` 注释 + 本文档 D4 + 03 正文 1.5 |
| A-4 全局聚合 | `FProcTable: Box<[FProc]>` 已在 02 落地，03 新增 `FprocLightTable: Box<[FprocLight]>` | `fproc.rs` + 本文档 D5 + 03 正文 2.1 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── fproc.rs            — FProc/FProcTable/FprocLight/FprocError/is_ok/ok/fproc_addr/slot_of/snapshot_light
└── main_loop.rs        — VfsState.fproc_table 聚合拥有，current_fp_slot 显式
```

### 4.2 `fproc.rs:773` 后的扩展（03 新增）

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `is_ok_endpoint` | `utility.c:92` 三守卫 | `fproc.rs:773` `FProcTable::is_ok_endpoint(&self, ep) -> Result<UserSlot, FprocError>` | 三守卫显式：`NONE→BadEndpoint`、`to_user_slot→BadSlot`、`endpoint != ke → BadEndpoint` |
| `ok_endpoint` | `proto.h:357` 致命 | `fproc.rs:793` `FProcTable::ok_endpoint(&self, ep) -> UserSlot` | `is_ok_endpoint(...).unwrap_or_else(|_| panic!("ok_endpoint failed at {}:{}", file!(), line!()))` |
| `fproc_addr` | `glo.h:27` 宏 | `fproc.rs:805` `FProcTable::at(&self, ep) -> Option<&FProc>` | `is_ok_endpoint` 的 `Option` 封装，`&self.slots[slot.get()]` |
| `who_p` | `glo.h:26` 宏 | `VfsState::caller_slot: Option<UserSlot>` | `fp - fproc` 的指针算术以 `UserSlot` 显式状态替代 |
| `FprocLight` | `fproc.h:111` | `fproc.rs:820` `FprocLight { tty, blocked_on, task }` | 投影结构，`tty: DevId` / `blocked_on: BlockedOn` / `task: Endpoint` |
| `FprocLightTable` | `fproc.h:115` | `fproc.rs:830` `FprocLightTable(Box<[FprocLight]>)` | `snapshot_light(&FProcTable) -> Vec<FprocLight>` 占位，`#[cfg(feature="fproc_light")]` |
| `FprocError` | `utility.c:122` `EDEADEPT` | `fproc.rs:860` `enum FprocError { BadEndpoint }` → `EDEADEPT 78` | `errno.rs:68` `EDEADEPT=78` 单一映射 |

### 4.3 两遍初始化的 Rust 位置

`FProcTable::new()` 已由 `(0..NR_PROCS).map(|_| FProc::new_unused()).collect()` 覆盖第一遍（`pid=0, endpoint=NONE`）；`init_phase2()` 已覆盖第二遍（`filps=[None; OPEN_MAX], rd/wd=None`）。03 不新增 `init` 方法，仅在 `main_loop.rs:191` 的 `init_fresh()` 保留两遍调用的显式时序。

### 4.4 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 空槽双哨兵清零 | `FProc::new_unused()` | `pid==0 && endpoint==NONE` | `fproc.rs:320` |
| 三守卫验证顺序 | `is_ok_endpoint` | `NONE → BadSlot → BadEndpoint` | `utility.c:92` 三分支 |
| 致命分化 | `ok_endpoint` | `fatal=1 → panic` | `proto.h:357` |
| 槽位同界 | `FProcTable: Box<[FProc;256]>` | `NR_PROCS` 与内核同值 | `com.rs:NR_PROCS=256` |
| 轻表只读 | `FprocLight` | `snapshot_light` 无写路径 | `fproc.h:111` 注释 |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **53 passed / 0 failed**（`fproc` 8 + `main_loop` 11 + `worker` 8 + `call_table` 5 + 既有 `02` 8 = 40 → 新增 13 后 53；`minix-types` 94 独立）。
> 本章直接影响 `2 → 4` 项新增，`fproc_light` 为 `cfg` 缺口不计入 53，`who_p` 由 `VfsState` 显式状态覆盖。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_is_ok_endpoint_none` | `utility.c:100` | `NONE → EDEADEPT` | `fproc.rs:900` |
| `test_is_ok_endpoint_out_of_range` | `utility.c:103` | `slot>=256 → EDEADEPT` | `fproc.rs:907` |
| `test_is_ok_endpoint_mismatch` | `utility.c:107` | `ke != ep → EDEADEPT`（空闲 vs 占用两条） | `fproc.rs:915` |
| `test_ok_endpoint_panic` | `utility.c:119` | `fatal=1 → panic`（`#[should_panic]`） | `fproc.rs:925` |
| `test_fproc_addr_none` | `glo.h:27` | `NONE → None` 的 fail-closed | `fproc.rs:932` |
| `test_is_in_use_pid_free` | `fproc.h:103` | `pid==0 → !is_in_use()` | `fproc.rs:940` |
| `test_fproc_light_snapshot` | `fproc.h:111` | `snapshot_light` 的 `#[cfg(feature)]` 缺口（ `cfg_attr` 跳过） | `fproc.rs:950` |

测试策略：`FProcTable` 的 `is_ok` 三守卫以 `NONE` 端点、越界端点（`Endpoint::from_generation_slot(0,999)`）、失配端点（同槽位不同 generation）三样本覆盖；`ok` 的 `panic` 以 `#[should_panic(expected="ok_endpoint")]` 显式；`fproc_light` 的快照以 `#[cfg(feature="fproc_light")]` 在默认 `cargo test` 下跳过（defer 契约），开 `feature` 后 `assert_eq!(light.tty, NO_DEV)`。

---

## 6 过渡

本篇在 `main.c:406-408` 第一遍清零与 `468-483` 第二遍目录/文件清零之间，是 `02` 的 `FProc` 字段定义之后、`04` 的 `filp` 表初始化之前的“表可用性”前提。

```
02-fproc-struct: FProc { pid, endpoint, wd/rd, filp[256], cloexec, tty, blocked_on + u, 凭证… } 定义
  │
  └─► 本章: fproc[256] 固定表 + isokendpt 三守卫 + PID_FREE/NONE 双哨兵 + fproc_light 投影  （两遍初始化的 Rust 侧 FProcTable::new/init_phase2）
         │
         ├─► 04-filp-table: filp 256 项的 init_filps/get_filp/引用计数  （依赖 fproc/filp 双表的 slot 隔离）
         ├─► 05-vnode-table: vnode 的 dup/put 与 vnode_clean_refs 的延迟回收 （与 fproc 的 wd/rd 互证）
         ├─► 06-vmnt-table: vmnt 的 get_free/mark_free 与 vmnt_unmap_by_endpt 的端点验证复用 isokendpt
         └─► 09-main-loop: get_work 的 endpoint→fproc 解引（main.c:606-613 的 fp=NULL 分支即 isokendpt 失败路径）
```

`fproc_light` 的观测成本优势为 `MIB` 服务的 `SI_PROC_TAB` 快照提供低成本替身（`99` 将回收为全局概念）。

阅读顺序提示：若关心“表条目如何被查找与分配”，下一站 `04-filp-table.md`（`get_filp` 的槽位查找与 `tll` 锁）；若关心“表条目如何参与分发”，下一站 `09-main-loop.md`（`fproc_addr` 与 `who_p` 的宏消解）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/fproc.h:82/101-115`（`fproc[NR_PROCS]` / `PID_FREE` / `fproc_light`）、`minix3/minix/servers/vfs/utility.c:92-123`（`isokendpt_f` 三守卫）、`minix3/minix/servers/vfs/glo.h:26-28`（`fproc_addr`/`who_p`）、`minix3/minix/servers/vfs/main.c:405-408`（第一遍）、`468-483`（第二遍）、`minix3/minix/servers/vfs/proto.h:352-358`（`okendpt/isokendpt` 宏）、`minix3/minix/servers/vfs/misc.c:55-96`（`fproc_light` 的 `sys_datacopy` 两段）
- 阶段文档：`02-fproc-struct.md`（`FProc` 字段与 `BlockedOn` 枚举）、`04-filp-table.md`（`init_filps` 的 `get_filp` 族）、`09-main-loop.md`（`fproc_addr` 的 `transid` 分发）、`99-global-concepts.md`（`PID_FREE`/`NR_PROCS` 常量与 `Endpoint` 术语）
- Rust 实现：`os/servers/vfs/src/fproc.rs:383`（`FProcTable: Box<[FProc]>`）、`os/servers/vfs/src/main_loop.rs:130`（`VfsState.fproc_table` 聚合）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/06-proc-init-boot-proc.md`（`endpoint` 的 `generation` 回绕与 `proc` 表同界）
