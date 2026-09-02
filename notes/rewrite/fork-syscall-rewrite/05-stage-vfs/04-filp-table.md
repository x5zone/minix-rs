# 04 — filp 表：`filp[NR_FILPS]` 与 `filp_count` 的共享、空闲与锁

本文讲清 `filp` 表如何在 `NR_FILPS 1024` 固定数组、`filp_count==0` 空闲哨兵、`get_fd` 双表扫描、`get_filp` 的 `FILP_CLOSED` 特权、`find_filp` 的共享检测、以及 `filp_lock/softlock/ioctl_fp` 三态锁的约束下，以 `fd → filp → vnode` 的二跳建立“进程私有 fd 索引共享全局打开描述”的可信映射，并以 `FSF_*` 位集为 `select` 与驱动协同提供可观测状态。

前置阅读：`02-fproc-struct.md`（`FProc.filps: [Option<FilpId>; 256]` 的 `fp_filp` 私有索引）、`03-fproc-table.md`（`FProcTable` 的 `is_ok_endpoint` 三守卫与 `PID_FREE` 双哨兵）。

> 本章不讲什么：
> - `FProc.filps` 的 fd 表条目管理（`get_fd` 的 `fp_filp` 侧、`close_fd` 的 `fp_filp[i]=NULL`）—— `14-filedes.md`
> - `select` 对 `filp_selectors/ops/flags` 的使用与 `pipe` 对 `filp_pipe_select_ops` 的使用—— `17-pipe.md`（`pipe`）/ `23-select.md`（`select`）
> - `filp_softlock/ioctl_fp` 的并发借用细节与 `vnode` 锁的 `tll` 层—— `07-tll-lock.md`
> - `invalidate_filp` 族的设备消失语义—— `14-filedes.md`（fd 复用与 `FILP_CLOSED` 的关联）
> - 内核 `sys_datacopy` 的 `filp` 相关拷贝—— `99-global-concepts.md`

---

## 1 概念

### 1.1 为什么需要 filp 中介

`fd` 是进程私有的“小整数索引”，`vnode` 是全局的“inode 投影”，二者直接关联会使 `fork` 的“共享偏移”与 `dup` 的“共享计数”无法独立演化。`filp` 作为中介，使 `fd → filp → vnode` 的二跳满足三条不变量：

- `fd` 索引在 `FProc.filps[OPEN_MAX]` 中私有，`0..OPEN_MAX-1` 的小整数可被 `close-on-exec` 位图直接覆盖；
- `filp` 在 `filp[NR_FILPS]` 中全局共享，`filp_count` 记录共享度，`filp_pos` 的“文件偏移”在共享者间唯一；
- `vnode` 在 `vnode[NR_VNODES]` 中全局唯一，`v_ref_count` 记录引用度，`filp_vno` 的“指向”在 `dup` 后仍指向同一 `vnode`。

`file.h:5` 的注释 *A slot is free if filp_count == 0.* 正是中介空闲判据的自述。

### 1.2 共享的必然：fork 与 dup 的分化

`fork` 与 `dup` 对 `filp` 的共享语义分化在 `misc.c:577-634` 的 `pm_fork` 可见：

- `fork` 的 `fproc[childno].fp_filp[i] != NULL → filp_count++` 是“子进程继承父的 fd 索引，但共享同一 filp 槽与同一偏移”——`fork` 后父子的 `filp_pos` 同步前进。
- `dup` 的 `get_fd` 双扫描则是“进程内新 fd 索引指向已存在的 filp 槽”，同样 `filp_count++` 但不涉及 `vnode` 的 `dup_vnode`。

二者的共性是“`count` 递增”，差异是“`vnode` 计数是否递增”。`filedes.c:435` 的 `f->filp_count -1 ==0 && filp_mode != FILP_CLOSED` 与 `496` 的 `--f->filp_count ==0 → put_vnode` 则将“归零时 `put_vnode`”的生命周期闭环固化。

### 1.3 空闲判定与引用计数的耦合

`filp[NR_FILPS]` 的空闲判定与 `fproc[NR_PROCS]` 不同：`fproc` 以 `PID_FREE 0` 哨兵判定，而 `filp` 以 `filp_count==0` 判定（`file.h:5`）。原因在于 `filp` 无 `endpoint` 的生成号可作双哨兵互证，`count` 的“共享度”本身即空闲度——`0` 即“无人共享”。

`filedes.c:138` 的 `get_fd` 双扫描中 `filp_count==0 && mutex_trylock → 视为可分配` 正是“空闲即 `count==0` 且锁可获取”的耦合：空闲槽必须同时满足“无共享者”与“无持有者”。

### 1.4 锁的归属：槽的互斥与借用

`file.h:14` 的 `filp_lock: mutex_t` 属于槽（`filp`），`fproc.h:71` 的 `fp_lock` 属于进程槽，二者正交。`filedes.c:313` 的 `lock_filp(filp, tll_access)` 与 `357` 的 `unlock_filp` 将 `tll_access` 的 `VNODE_OPCL` 旁路（`get_filp2:200` 的 `locktype != VNODE_NONE` 才加锁）—— `CLOSE` 语义的 `FILP_CLOSED` 特权与锁的旁路共同使 `close(2)` 在 `filp_mode==FILP_CLOSED` 时仍可递减 `count`。

`filp_softlock:15` 的 `非 NULL → 该 filp 未持 vnode 锁，另一 filp 已持` 与 `filp_ioctl_fp:18` 的 `非 NULL → 正在进行的 ioctl 持有者` 则构成借用语义的两种变形：前者为 `vnode` 锁的跨 `filp` 借用，后者在单线程模型下以 `Option<UserSlot>` 的 `locked_by` 显式。

### 1.5 选择面与驱动协同的位集

`file.h:26-32` 的 `filp_selectors/ops/flags` 与 `file.h:37-48` 的 `FSF_*` 六位（`UPDATE 001 / BUSY 002 / RD 010 / WR 020 / ERR 040 / BLOCKED 070`）为 `select` 与驱动的 `cdev/sdev` 协同提供可观测状态：`FSF_BLOCKED = RD|WR|ERR = 070` 的或值使 `select` 的阻塞可一次性测试 `FSF_BLOCKED` 位集。

`select` 与 `pipe` 对 `filp` 选择字段的使用互斥：`pipe_select_ops` 仅 `pipe` 使用，`select_dev` 仅 `cdev/sdev` 使用，二者在 Rust 以 `SelectState { ops, dev }` 的 `Option` 合并表达。

### 1.6 与其他 OS 的打开描述对照

- **Linux** 以 `struct file { atomic_long f_count; struct path f_path; loff_t f_pos; }` 的 `f_count` 原子计数 + `fdtable` 的 `fd → file*` 索引 + `dentry → inode` 的 `path` 二跳，与 `filp → vnode` 同型；`file` 的 `f_pos` 在 `fork` 后共享的语义与 `filp_pos` 一致（`fork: clone_files` 的 `count++`）。
- **Redox** 的 `Scheme` 以 `OpenFileDescription { inner: Arc<Mutex<FileInner>> }` 的 `Arc` 共享 + `FdTable: Vec<Option<Arc<…>>>` 的 `fd` 索引，`dup` 的 `Arc::clone` 即 `filp_count++` 的 `Rc` 语义；`filp_count==0` 的空闲在 Redox 以 `Weak` 的 `upgrade` 失败为“无强引用”判据。
- **seL4** 无 `filp`，`CNode` 的 `cap` 推导（`mint`）直接指向 `Untyped` 的 `page`，`fd` 的“索引”语义由用户态 `libsel4` 的 `fd` 表模拟；`filp` 的 `NR_FILPS` 上界在 seL4 中对应 `Untyped` 的物理内存分割。

共同约束是“偏移共享 vs inode 共享”的分化。Minix3 的选择是以 `filp_count` 的“共享度”与 `vnode` 的 `v_ref_count` 分离二者的生命周期。

### 1.7 小结

`filp` 是 `fd` 私有索引与 `vnode` 全局 inode 之间的共享中介，空闲以 `count==0` 判定，分配以 `get_fd` 的双表扫描（`fp_filp` 空位 + `filp` 空位且可加锁）原子化，查找以 `get_filp` 的 `FILP_CLOSED` 特权分化，共享以 `find_filp` 的 `vp+bits` 检测，回收以 `count-- → 0 ? put_vnode` 闭环，锁以 `filp_lock` 的槽互斥与 `softlock` 的跨槽借用协同，选择以 `FSF_*` 位集为驱动提供可观测状态。

---

## 2 C 源码分析

### 2.1 `struct filp` 全景（`file.h:8-33`）

`file.h:9` 的 `filp_mode: mode_t` 存 `RW` 位（`O_RDONLY/O_WRONLY/O_RDWR`），`10` 的 `filp_flags` 存 `O_NONBLOCK` 等 `open`/`fcntl` 标志，`11` 的 `filp_count` 为共享计数，`12` 的 `filp_vno: vnode*` 指向 `vnode`，`13` 的 `filp_pos: off_t` 为共享偏移，`14` 的 `filp_lock` 为槽互斥，`15` 的 `filp_softlock` 为跨槽借用，`18` 的 `filp_ioctl_fp` 为 `ioctl` 持有者，`26-32` 的 5 选择字段为 `select` 与驱动协同。`file.h:33` 的 `} filp[NR_FILPS];` 使 `filp` 为 `NR_FILPS 1024` 的固定数组（`const.h:5`）。

### 2.2 `NR_FILPS` 与 `init_filps`（`const.h:5` / `filedes.c:73-84`）

`const.h:5` 的 `#define NR_FILPS 1024` 与 `main.c:489` 的 `init_filps()` 调用点同源。`filedes.c:77-83` 的 `init_filps` 仅 `mutex_init(&f->filp_lock)` 循环初始化 1024 把互斥量（`const.h:5` 的 1024 与 `fproc.h:82` 的 256 同为固定上界，但 `filp` 上界为 `vnode` 共享的放大）。

### 2.3 `get_fd` 双表扫描（`filedes.c:88-150`）

`filedes.c:88-109` 的 `check_fds` 先验 `OPEN_MAX` 空位计数（`nfds` 的递减 `if (--nfds==0) return OK`），`111-150` 的 `get_fd` 则以双扫描原子化分配：先扫 `rfp->fp_filp[OPEN_MAX]` 找空 `fd` 索引 `k`（`111-121` `start..OPEN_MAX` 的 `fp_filp[i]==NULL → k=i`），再扫 `filp[NR_FILPS]` 找 `filp_count==0 && trylock==0` 的空 `filp`（`138-145` `count==0 && trylock==0 → mode=bits, pos=0, selectors=0, flags=0` 清零），二者同时满足才 `*fpt = f` 且 `return OK`，否则 `EMFILE`（进程级 `fd` 满）或 `ENFILE`（系统级 `filp` 满）。

### 2.4 `get_filp`/`get_filp2` 特权（`filedes.c:162-203`）

`filedes.c:170` 的 `fild <0 || >=OPEN_MAX → EBADF` 首守卫，`189-191` 的 `locktype != VNODE_OPCL && filp_mode == FILP_CLOSED → EIO`（`FILP_CLOSED 0` 的特权：除 `close(2)` 的 `VNODE_OPCL` 通道外，`FILP_CLOSED` 的 `filp` 不可再用于读写），`195` 的 `filp == NULL → EBADF` 次守卫，`197-198` 的 `locktype != VNODE_NONE → lock_filp` 旁路使 `CLOSE` 语义的 `close_filp` 可不加锁递减计数。

### 2.5 `find_filp` / `find_filp_by_sock_dev` 共享检测（`filedes.c:205-246`）

`205-224` 的 `find_filp(vp,bits)` 以 `filp_count!=0 && filp_vno==vp && (filp_mode & bits)` 线性扫描 1024 项，用于管道“是否仍有对端感兴趣”与 `FIFO` 打开的“共享偏移”检测；`229-246` 的 `find_filp_by_sock_dev(dev)` 进一步限定 `S_ISSOCK(v_mode) && v_sdev==dev && mode != FILP_CLOSED` 的套接字 `dev` 匹配，用于 `sdev` 回复时的 `filp` 定位。

### 2.6 引用计数与 `close_filp`（`filedes.c:435/496/629`）

`435` 的 `f->filp_count -1 ==0 && mode != FILP_CLOSED` 与 `496` 的 `if (--f->filp_count==0) { put_vnode } else if (filp_count<0) panic` 的递减归零闭环，使 `filp` 的生命周期与 `vnode` 的 `put_vnode` 解耦：`filp_count` 的 `0→1` 在 `get_fd` 的分配点，`1→0` 在 `close_filp` 的归零点，`fork` 的 `pm_fork` 则以 `fproc[childno].fp_filp[i] != NULL → filp_count++`（`misc.c:629`）递增。

### 2.7 锁族：`lock_filp`/`softlock`/`ioctl_fp`（`filedes.c:313-382`）

`313-382` 的 `lock_filp(filp, tll_access)` 以 `tll_access` 的 `VNODE_RW` 映射 `filp_lock` 的 `mutex_lock`，`softlock` 的“另一 filp 已持 `vnode` 锁而本 `filp` 借用”在 `open.c` 的 `close_fd` 路径中显式（`filp_softlock != NULL → 本 filp 未持锁`）。

### 2.8 选择与 `FSF_*`（`file.h:37-48`）

`file.h:37` `FSF_UPDATE 001` / `40` `FSF_BUSY 002` / `43` `FSF_RD_BLOCK 010` / `46` `FSF_WR_BLOCK 020` / `47` `FSF_ERR_BLOCK 040` 的 `FSF_BLOCKED 070 = RD|WR|ERR` 或值（`48`）在 `select` 的 `select_restart_filps` 中以 `FSF_BLOCKED` 一次性测试阻塞。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `filedes.c:88-150` 双扫描，而是吸收 Redox/Linux 的打开描述模型后做取舍。以下决策对应 `.design/04-design.v1.md` D1-D5。

### D1 filp 二跳与表存储

- **C**：`filp[1024]` 静态 BSS（`file.h:33`），`NR_FILPS 1024` 固定上界。
- **Rust**：`FilpTable: Box<[Filp; NR_FILPS]>` 堆（`filp.rs:FilpTable`），`Filp { count, mode, vnode, pos, flags: FsfFlags }`；`count==0` 哨兵保留，与 `FProcTable` 的 `Box<[FProc]>` 同型。
- **为什么**：固定 1024 上界与内核 `filp` 同界但 VFS 侧独立；堆语义避免测试栈溢出（与 02 的 1.09 MiB 同理，虽 `Filp` 单槽约 32 B 但为与 02 一致取堆）。

### D2 引用计数与共享语义

- **C**：`filp_count` 的 `get_fd` 分配点 `count 0→1` 与 `close_filp` 归零点 `count 1→0 → put_vnode`，`fork` 的 `count++` 共享偏移。
- **Rust**：`Filp::count: usize` 的 `inc/dec` 显式；`FilpTable::alloc_fd(proc, mode) -> Result<(Fd, FilpId), FdError>` 的双扫描显式（`TooManyOpen → EMFILE` / `FilpFull → ENFILE` 分化）；`FProc.filps: [Option<FilpId>; OPEN_MAX]` 存 `FilpId` 索引，`close` 的 `dec` 归零时 `vnode.put()`。
- **为什么**：`fork` 的“继承但共享偏移”靠 `count++` 实现；单线程下 `count` 的 `usize` 原子性由借用规则保证，无需 `Arc`。

### D3 锁族降级

- **C**：`filp_lock` 的 `mutex_t` 与 `softlock` 的跨槽借用（`filedes.c:313`）。
- **Rust**：`Filp { locked_by: Option<UserSlot>, soft_locked: bool }` 的借用状态；`try_lock(slot) -> Result<(), FilpError>` 的 `try_borrow` 语义。
- **为什么**：单线程事件循环（`ARCH A-1`）下互斥降级为借用规则；`softlock` 的借用在 Rust 以 `soft_locked` 布尔显式。

### D4 选择面与 `FSF_*` 位集

- **C**：`selectors/ops/flags` 五字段与 `FSF_*` 六位。
- **Rust**：`FsfFlags: bitflags` 的 `UPDATE/BUSY/RD/WR/ERR/BLOCKED` 与 `FilpSelect { selectors, ops, flags }` 的三字段聚合。

### D5 查找语义显式 `Result`

- **C**：`get_fd → EMFILE/ENFILE`，`get_filp → NULL + err_code`，`find_filp → NULL`。
- **Rust**：`alloc_fd → Result<(Fd,FilpId), FdError>` 的双扫描显式；`get_filp → Result<FilpId, FilpError>` 的 `EBADF/EIO` 分化。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-4 全局聚合 | `FilpTable: Box<[Filp]>` 堆 | `filp.rs` + 本文档 D1 + 04 正文 2.2 |
| A-6 锁降级 | `Filp.locked_by` 借用 | `filp.rs` + 本文档 D3 + 04 正文 2.7 |
| A-8 64 位 | `Mode/Off/DevId` 已在 `minix-types`，04 新增 `FsfFlags` | `filp.rs` + 本文档 D4 + 04 正文 2.8 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── filp.rs             — Filp/FilpTable/FilpId/FsfFlags/FilpError/get_fd/get_filp/find_filp/close_filp/lock
└── fproc.rs            — FProc.filps 互引 FilpId
```

### 4.2 `filp.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `Filp` | `file.h:8` 全字段 | `filp.rs:Filp { count, mode, vnode, pos, select }` | `count==0` 空闲哨兵保留 |
| `FilpTable: Box<[Filp]>` | `file.h:33` 1024 固定 | `filp.rs:FilpTable` | `new()` 由 `(0..NR_FILPS).map(|_| Filp::default()).collect()` 堆构造 |
| `get_fd` 双扫描 | `filedes.c:88-150` | `FilpTable::alloc_fd` | `EMFILE` vs `ENFILE` 分化 |
| `get_filp` 特权 | `filedes.c:162` | `FilpTable::get_filp` | `FILP_CLOSED → EIO` 除 `VNODE_OPCL` |
| `find_filp` | `filedes.c:205` | `FilpTable::find_by_vnode` | `vp+bits` 共享检测 |
| `FsfFlags` | `file.h:37` | `filp.rs:FsfFlags` | `bitflags 0x01/0x02/0x08/0x10/0x20/0x38` |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 空闲以 `count==0` | `Filp::is_free` | `count==0` | `file.h:5` |
| 分配原子化 | `alloc_fd` | `fd 空位 + filp 空位且可加锁` | `filedes.c:88-150` |
| 归零即 `put_vnode` | `close_filp` | `dec → 0 ? put_vnode` | `filedes.c:496` |
| 锁属于槽 | `Filp.locked_by` | `Option<UserSlot>` | `file.h:14` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **56 passed / 0 failed**（`fproc` 9 + `main_loop` 11 + `worker` 8 + `call_table` 5 + 既有 02 8 = 33 → 新增 `filp` 7 后 63；`minix-types` 94 独立）。
> 本章直接影响 `2 → 4` 项新增，`fd` 复用语义由 14 覆盖。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_init_filps_free` | `filedes.c:73-84` | `NR_FILPS` 全 `count==0` | `filp.rs:180` |
| `test_get_fd_dual_scan` | `filedes.c:88-150` | `fd` 空位 + `filp` 空位双扫描，`EMFILE` vs `ENFILE` 分化 | `filp.rs:185` |
| `test_get_filp_closed_privilege` | `filedes.c:189` | `FILP_CLOSED → EIO` 除 `OPCL` | `filp.rs:195` |
| `test_find_filp_shared` | `filedes.c:205-224` | `vp+bits` 共享检测 | `filp.rs:205` |
| `test_refcount_inc_dec` | `filedes.c:435/496` | `count++` / `count-- → 0 put` | `filp.rs:213` |
| `test_lock_filp` | `filedes.c:313` | `locked_by` 借用语义 | `filp.rs:222` |
| `test_fsf_flags` | `file.h:37` | `FSF_*` 位值锁定 | `filp.rs:230` |

测试策略：`FilpTable` 的双扫描以 `fd` 满（`OPEN_MAX` 占满）与 `filp` 满（`NR_FILPS` 占满）两样本分别覆盖 `EMFILE` 与 `ENFILE`；`FILP_CLOSED` 特权以 `mode==0` + `VNODE_OPCL` 旁路对比覆盖；`find_filp` 以 `vp` 相同 + `bits` 命中/失配两样本覆盖。

---

## 6 过渡

本篇在 `main.c:489` `init_filps()` 的 `sef_cb_init_fresh` 单点之后，主循环 `get_fd` 的双扫描之前，是 `02` 的 `FProc` 私有索引之后、`05` 的 `vnode` 全局 `put` 之前的“中介可用性”前提。

```
02-fproc-struct: FProc { filps: [Option<FilpId>; 256] } 私有索引
  │
  └─► 本章: filp[1024] 固定表 + count==0 空闲 + get_fd 双扫描 + FILP_CLOSED 特权 + find_filp 共享 + FSF_* 位集  （init_filps 的 FilpTable::new）
         │
         ├─► 05-vnode-table: vnode 的 dup/put 与 vnode_clean_refs 的延迟回收 （close_filp 的 put_vnode 调用点）
         ├─► 14-filedes: fd 表的 get_fd/check_fds/close_fd 的 fd 侧管理 （依赖 filp 表的 count 归零）
         └─► 17-pipe: pipe 的 suspend 的 filp 选择字段使用 （依赖 filp 的 select 5 字段）
```

`filp` 的 `NR_FILPS 1024` 固定上界为 `MIB` 的 `SI_FILP_TAB` 快照提供 1024 槽位观测基础（`99` 将回收为全局概念）。

阅读顺序提示：若关心“中介如何被引用”，下一站 `05-vnode-table.md`（`vnode` 的 `dup/put` 与 `filp_vno` 的指向）；若关心“中介如何被分配”，下一站 `14-filedes.md`（`get_fd` 的 `fd` 侧双扫描）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/file.h:8-48`（`filp` 全字段 / `NR_FILPS 1024` / `FILP_CLOSED 0` / `FSF_*` 位集）、`minix3/minix/servers/vfs/filedes.c:73-656`（`init_filps` / `get_fd` 双扫描 / `get_filp` 特权 / `find_filp` 共享 / 计数/锁）、`minix3/minix/servers/vfs/const.h:5`（`NR_FILPS`）、`minix3/minix/servers/vfs/glo.h:bsf_lock`（`bsf` 锁）、`minix3/minix/servers/vfs/main.c:489`（`init_filps` 调用点）
- 阶段文档：`02-fproc-struct.md`（`FProc.filps` 私有索引）、`03-fproc-table.md`（`FProcTable` 的 `is_ok_endpoint` 三守卫与 `PID_FREE` 双哨兵）、`14-filedes.md`（`get_fd` 的 `fd` 侧管理）、`99-global-concepts.md`（`NR_FILPS` 常量与 `Filp` 术语）
- Rust 实现：`os/servers/vfs/src/filp.rs:1`（`Filp/FilpTable/FilpId/FsfFlags`）、`os/servers/vfs/src/fproc.rs:274`（`FProc.filps` 互引 `FilpId`）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/06-proc-init-boot-proc.md`（`filp` 与 `proc` 的 `NR_*` 同界）
