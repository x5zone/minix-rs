# 05 — vnode 表：`vnode[NR_VNODES]` 与 `v_ref_count/v_fs_count` 双层引用及 `vnode_clean_refs` 的 256 阈值

本文讲清 `vnode` 表如何在 `NR_VNODES 1024` 固定数组、`v_ref_count==0 && !locked` 空闲双条件、`find_vnode` 的 `fs_e+ino` 命中、`dup_vnode` 的 `ref++` 与 `put_vnode` 的 `ref>1` 快速路径 vs `ref==1` 慢路径 `req_putnode` 以及 `v_fs_count>256` 时 `clean_refs` 的延迟同步约束下，以 `v_ref_count` (VFS 层) 与 `v_fs_count` (FS 层) 的双层计数建立 `filp → vnode → vmnt` 的中介-缓存双链路，并以 `VNODE_READ/OPCL/WRITE` 映射 `TLL` 的三级锁为路径解析与文件 I/O 提供可升级的互斥。

前置阅读：`04-filp-table.md`（`FilpTable` 的 `count==0` 哨兵与 `alloc_filp` 双扫描）、`03-fproc-table.md`（`FProcTable` 的 `is_ok_endpoint` 三守卫）。

> 本章不讲什么：
> - `vmnt` 表结构与 `v_vmnt` 的挂载点穿越—— `06-vmnt-table.md`
> - 路径解析对 `vnode` 的 `lock_vnode` 使用与 `get_name` 的名字拷贝—— `13-path-lookup.md`
> - `vnode` 锁的 `tll` 三级原语（`tll_lock/upgrade/downgrade`）—— `07-tll-lock.md`
> - `req_putnode` 的 FS 通信原语与 `VFS_TRANSID` 编码—— `11-fs-comm.md`
> - 内核 `sys_datacopy` 的 `vnode` 相关拷贝—— `99-global-concepts.md`

---

## 1 概念

### 1.1 为什么需要 vnode 缓存

`vnode` 是 VFS 对“底层 FS 的 `inode`”在内存中的投影：`filp.vno → vnode` 指向它，`vnode.v_fs_e + v_inode_nr` 唯一标识它，`v_mode/v_size/v_uid/v_gid` 缓存它的元数据。`vnode[NR_VNODES]` 的 1024 固定数组使 `open` 的“是否已缓存”可通过 `find_vnode(fs, ino)` 的 `O(1024)` 线性扫描判定，无需哈希——这与 `filp[NR_FILPS]` 的 1024 同界（`const.h:5/8` 同值 1024）构成 `filp` 与 `vnode` 的双 `1024` 固定缓存对偶。

与缓存相对的是直通：若无 `vnode`，每次 `read` 都需 `req_lookup` 往返 FS；`vnode` 的存在使 `dup_vnode` 的 `ref++` 在 `fork` 的 `dup_vnode(fp_rd)` 与 `dup_vnode(fp_wd)` 场景下共享同一 `vnode` 的 `v_ref_count`，而 `v_fs_count` 的延迟同步则避免每次 `dup/put` 都 `req_putnode`。

`vnode[NR_VNODES]` 的 1024 上界与 `filp[NR_FILPS]` 同值并非巧合：`NR_VNODES` 的 1024 使 `vnode` 的 `find_vnode` 线性扫描与 `filp` 的 `find_filp` 同为 `O(1024)` 的可预测上界，而 `open` 的“缓存命中”在 `find_vnode` 命中时直接 `dup_vnode` 复用，无需 `get_free_vnode` 分配。

### 1.2 双层引用的经济学

`vnode.h:13` 的 `v_ref_count` 与 `14` 的 `v_fs_count` 构成双层：

- `v_ref_count`：VFS 层的“引用计数”，`dup_vnode` 时 `++`（`vnode.c:233`），`put_vnode` 时 `>1 → --`（`260` 快速路径）或 `==1 → req_putnode + 0`（`282-295` 慢路径）；
- `v_fs_count`：底层 FS 的“打开计数”，`clean_refs` 的 `>256 → req_putnode(fs_count-1), fs_count=1`（`vnode.c:263` `>256` 阈值与 `313` `fs_count-1`）防止 `int` 环绕。

二者的延迟同步是性能优化：`put_vnode` 的 `ref>1` 时仅 `ref--` 而不立即 `req_putnode`，`fs_count` 的 `>256` 阈值才 `clean_refs` 的 `req_putnode(fs_count-1)` 将多余的 FS 引用一次性回收（`vnode.c:263` 的 `if fs_count>256 → clean_refs` 与 `305-314` 的 `if fs_count>1 → put(fs_count-1), fs_count=1` 阈值闭环）。`fs_count` 的 256 阈值并非 255 或 257，而是 `int` 的 `2^31-1` 环绕前的批量回收点——`ref_count` 的 `int` 上界与 `fs_count` 的 `int` 上界同为 `2^31-1`，但 `fs_count` 的 `req_putnode` 往返 FS 成本使“每 `put` 都同步”不可接受。

`vnode.c:263` 的 `if (v_fs_count > 256) vnode_clean_refs(vp);` 在 `put_vnode` 的 `ref>1 → ref--` 快速路径后插入 `clean_if_needed`，使 `dup` 的 `ref++` 在 `ref>1` 时仅 `ref--` 而不立即 `req_putnode` 的延迟同步在 `256` 处批量回收，与 `filp` 的 `count-- → 0 ? put_vnode` 的单层闭环分化（`04` 的单层 vs `05` 的双层）。

### 1.3 空闲与锁的耦合

`vnode` 的空闲判定与 `filp` 相同但增加锁耦合：`vnode.c:91` 的 `get_free_vnode` 以 `ref==0 && !is_vnode_locked(vp)` 双条件判断空闲——`ref==0` 无引用且未被 `tll` 持有方可分配。这与 `filp` 的 `count==0 && trylock==0` 同型（`04` 的 `alloc_filp` 双条件），均以“无共享者且无持有者”作为空闲的充分条件。

`vnode.c:92-99` 的清零 `v_uid=-1, v_gid=-1, sdev=NO_DEV, mapfs_e=NONE, mapfs_count=0, mapinode=0` 则将“分配即清零”的语义与 `get_free_vnode` 的返回前 5 字段原子化。与 `filp` 的 `alloc_filp` 清零 `selectors/ops` 同型，但 `vnode` 的清零包含 `sdev` 的设备哨兵（`NO_DEV` 0）与 `mapfs` 的映射端点（`NONE`）。

`is_vnode_locked` 的 `tll_islocked || tll_haspendinglock`（`vnode.c:128` `tll_islocked(&vp->v_lock) || tll_haspendinglock(&vp->v_lock)`）使“持有中”与“等待中”均视为“非空闲”——与 `filp` 的 `trylock==0` 的“可加锁”判据同为“锁可获取即空闲”的互斥语义。

### 1.4 锁的升级与借用

`vnode.h:26-29` 的 `VNODE_NONE/TLL_NONE` / `READ/TLL_READ` / `OPCL/TLL_READSER` / `WRITE/TLL_WRITE` 映射使 `lock_vnode(vp, VNODE_READ/OPCL/WRITE)` 的 `tll_lock` 可升级：`vnode.c:218-224` 的 `upgrade_vnode_lock` 以 `tll_upgrade` 将 `READ` 提升为 `WRITE`（`open` 的 `lookup` 先 `READ` 探路，命中后 `WRITE` 修改）。

`vnode.c:156-165` 的 `lock_vnode` 在 `VNODE_READ` 时 `fp->fp_vp_rdlocks++` 的 `LOCK_DEBUG` 计数（`fproc.h:79`）则将读锁持有度暴露给 `check_vnode_locks_by_me` 的调试路径——与 `fproc` 的 `fp_lock` 正交，`vnode` 锁属于 `vnode` 槽而非进程槽。`VNODE_OPCL` 的 `TLL_READSER` 串行读（`TLL_READSER` 的 `S` 为 `Serial`）使 `open` 的 `O_EXCL` 语义在 `vnode` 锁层可串行化。

### 1.5 挂载点穿越的承上

`vnode.h:21` 的 `v_vmnt: vmnt*` 指向 `vmnt` 表的挂载实例，`v_dev:19` 为 `inode` 所在设备，`v_sdev:20` 为特殊文件的设备号。三者的分化在 `path.c:advance` 的挂载点穿越中显式：`v_vmnt != vmnt_of(v_dev)` 即“此 vnode 的设备与所处 vmnt 的设备不一致” → 穿越。

`v_bfs_e:16` 的块特殊文件端点（`v_bfs_e` 为 `bdev` 的 FS 端点）则将 `bdev` 的 `bsf` 缓存语义与 `vnode` 的设备号分离——`bdev` 的 `bsf` 锁在 `read.c:49` 的 `lock_bsf` 层，与 `vnode` 的 `v_lock` 正交。

### 1.6 与其他 OS 的 inode 缓存对照

- **Linux** 以 `inode_hashtable` 的 `hlist_bl` 哈希 + `inode->i_count` 的 `atomic_t` 引用计数 + `inode->i_state` 的 `I_NEW/I_FREEING` 状态机，`iput` 的 `atomic_dec_and_test → evict` 与 `vnode` 的 `put_vnode` 的 `ref>1 → ref--` 快速路径同型；`Linux` 的 `i_count` 与 `v_ref_count` 同为 VFS 层计数，但 `i_nlink` 的持久引用在 `vnode` 中由 `v_fs_count` 的延迟同步近似。`Linux` 的 `inode->i_sb` 指向 `super_block` 与 `vnode.v_vmnt` 同为挂载实例指针。
- **Redox** 的 `Scheme` 以 `Arc<Inode>` 的 `strong_count` 共享 + `Slab<Inode>` 的 arena + `RwLock` 的 `read/write` 升级，`dup` 的 `Arc::clone` 即 `dup_vnode` 的 `ref++` 的 `Arc` 语义；`vnode_clean_refs` 的 `256` 阈值在 Redox 以 `Arc::strong_count() > 256` 的 `drop` 批量回收近似。`Redox` 的 `Inode` 的 `Arc` 共享与 `vnode` 的 `ref_count` 分离 `fs_count` 同为双层，但 `Arc` 的 `strong_count` 与 `v_fs_count` 的“FS 打开计数”在 Redox 中由 `Scheme` 的 `open` 句柄显式。
- **seL4** 无 `vnode`，`CNode` 的 `cap` 推导（`mint`）直接指向 `Untyped` 的 `page`，`vnode` 的 `NR_VNODES` 上界在 seL4 中对应 `Untyped` 的物理内存分割；`vnode` 的 `NR_VNODES 1024` 固定上界在 seL4 中由 `Untyped` 的 `freeIndex` 线性分配近似。

共同约束是“引用计数与锁的耦合”。Minix3 的选择是以 `ref==0 && !locked` 的双条件作为空闲判据，使 `get_free_vnode` 的分配与 `is_vnode_locked` 的持有度检查原子化，与 `filp` 的 `count==0 && trylock==0` 同型但增加 `tll_haspendinglock` 的等待中判定。

### 1.7 小结

`vnode` 是 `filp` 与 `vmnt` 之间的文件对象缓存，空闲以 `ref==0 && !locked` 双条件判定，命中以 `fs+ino` 的 `find_vnode` 判定，生命周期以 `dup` 的 `ref++` 与 `put` 的 `ref>1 → ref--` 快速路径 vs `ref==1 → req_putnode` 慢路径分化，双层计数以 `fs_count>256 → clean_refs` 的延迟同步防止环绕，锁以 `VNODE_READ/OPCL/WRITE` 映射 `TLL` 的三级可升级互斥。

---

## 2 C 源码分析

### 2.1 `struct vnode` 全景（`vnode.h:4-23`）

`vnode.h:5` 的 `v_fs_e: endpoint_t` 为 FS 进程端点，`7` 的 `v_inode_nr: ino_t` 为 minor 设备上的 inode 号，`9` 的 `v_mode: mode_t` 为类型与权限，`10` 的 `v_uid:11` 的 `v_gid` 为属主，`12` 的 `v_size: off_t` 为大小，`13` 的 `v_ref_count` 为 VFS 引用，`14` 的 `v_fs_count` 为 FS 打开计数，`15` 的 `v_mapfs_count` 为映射 FS 计数，`16` 的 `v_bfs_e` 为块特殊文件的 FS 端点，`18` 的 `v_dev` 为 inode 所在设备，`20` 的 `v_sdev` 为特殊设备号，`21` 的 `v_vmnt: vmnt*` 为挂载实例，`22` 的 `v_lock: tll_t` 为三级锁。`vnode.h:23` 的 `} vnode[NR_VNODES];` 使 `vnode` 为 `1024` 固定数组，与 `filp[NR_FILPS]` 的 1024 同界。

### 2.2 `NR_VNODES` 与 `init_vnodes`（`const.h:8` / `vnode.c:138-154`）

`const.h:8` 的 `#define NR_VNODES 1024` 与 `main.c:486` 的 `init_vnodes()` 调用点同源。`vnode.c:142-152` 的 `init_vnodes` 循环 1024 次 `v_fs_e=NONE, v_mapfs_e=NONE, inode=0, ref=0, fs_count=0, mapfs_count=0, tll_init` 零化，与 `init_filps` 的 `mutex_init` 同型但增加 `tll_init` 的三级锁初始化。`vnode.c:140` 的 `struct vnode *vp` 指针与 `142` 的 `for (vp=&vnode[0]; vp<&vnode[NR_VNODES]; ++vp)` 的指针算术在 Rust 以 `VnodeTable: Box<[Vnode]>` 的 `iter_mut` 替代。

### 2.3 `get_free_vnode` 双条件（`vnode.c:84-104`）

`vnode.c:90-99` 的 `get_free_vnode` 以 `ref==0 && !is_vnode_locked(vp)` 双条件扫描 1024 项，命中后 `v_uid=-1, v_gid=-1, sdev=NO_DEV, mapfs_e=NONE, mapfs_count=0, mapinode=0` 的 5 字段清零（与 `filp` 的 `alloc_filp` 清零同型），否则 `err_code=ENFILE` 的 `NULL`。`vnode.c:91` 的 `if (vp->v_ref_count==0 && !is_vnode_locked(vp))` 的双条件与 `filedes.c:138` 的 `filp_count==0 && trylock==0` 同型，但增加 `is_vnode_locked` 的 `tll_islocked || tll_haspendinglock` 等待中判定。

### 2.4 `find_vnode` 命中（`vnode.c:110-124`）

`vnode.c:116-118` 的 `find_vnode(fs_e, ino)` 以 `ref>0 && v_inode_nr==ino && v_fs_e==fs_e` 线性扫描 1024 项的命中判定，与 `filp` 的 `find_filp(vp,bits)` 的 `count!=0 && vno==vp && mode&bits` 同型（`04` 的 `vp+bits` 共享检测）。`vnode.c:117` 的 `if (vp->v_ref_count>0 && vp->v_inode_nr==ino && vp->v_fs_e==fs_e) return(vp);` 的 `ref>0` 守卫使空闲槽的 `ino==0` 不命中。

### 2.5 锁族：`lock_vnode`/`unlock_vnode`/`upgrade`（`vnode.c:156-224`）

`vnode.c:159` 的 `tll_lock(&vp->v_lock, locktype)` 与 `177` 的 `tll_unlock` 及 `221` 的 `tll_upgrade` 构成 `VNODE_READ`（`TLL_READ` 多读者）/ `OPCL`（`TLL_READSER` 串行读）/ `WRITE`（`TLL_WRITE` 独占）的三级可升级互斥。与 `filp` 的 `filp_lock` 单级互斥分化（`04` 的 `locked_by` 单状态 vs `vnode` 的三态）。`vnode.c:165` 的 `if (locktype==VNODE_READ) fp->fp_vp_rdlocks++` 的 `LOCK_DEBUG` 计数在 Rust 以 `VnodeLockState::Read(n)` 的 `n` 显式。

### 2.6 引用计数：`dup_vnode`/`put_vnode`/`clean_refs`（`vnode.c:225-316`）

`vnode.c:233` 的 `dup_vnode: vp->v_ref_count++` 单分支递增，与 `vnode.c:260` 的 `put_vnode: ref>1 → ref--` 快速路径及 `263` 的 `fs_count>256 → clean_refs` 阈值及 `274-295` 的 `ref==1 → req_putnode` 慢路径分化：`put_vnode` 的 `lock_vnode(VNODE_OPCL)` 守门 + `ref>1 → ref--, clean_if_needed, unlock, return` vs `ref==1 → upgrade → assert(ref>0 && fs_count>0) → req_putnode(fs_count) → fs_count=0, ref=0, mapfs_count=0, unlock` 的慢路径。`vnode.c:310-314` 的 `clean_refs: fs_count<=1 → return; put(fs_count-1), fs_count=1` 的阈值回收使 `fs_count` 的延迟同步在 `256` 处批量回收。

`vnode.c:282` 的 `req_putnode(vp->v_fs_e, vp->v_inode_nr, vp->v_fs_count)` 的 `v_fs_count` 全量 `put` 与 `313` 的 `req_putnode(vp->v_fs_e, vp->v_inode_nr, vp->v_fs_count-1)` 的 `fs_count-1` 增量 `put` 的分化，使 `clean_refs` 的批量回收在 `256` 阈值处 `put(fs_count-1)` 后 `fs_count=1` 保留一个打开引用。

### 2.7 设备与挂载关联

`vnode.h:18` 的 `v_dev` 与 `21` 的 `v_vmnt` 的 `vmnt*` 互证：`v_dev` 为 `v_vmnt->m_dev` 的设备号投影，`v_sdev` 为特殊文件的设备号（`NO_DEV` 哨兵保留）。`v_bfs_e:16` 的块特殊文件端点则将 `bdev` 的 `bsf` 缓存语义与 `vnode` 的设备号分离——`bdev` 的 `bsf` 锁在 `read.c:49` 的 `lock_bsf` 层，与 `vnode` 的 `v_lock` 正交。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `vnode.c:84-104` 双条件，而是吸收 Redox/Linux 的 inode 缓存模型后做取舍。以下决策对应 `.design/05-design.v1.md` D1-D5。

### D1 vnode 中介与表存储

- **C**：`vnode[1024]` 静态 BSS（`vnode.h:23`），`NR_VNODES 1024` 固定上界。
- **Rust**：`VnodeTable: Box<[Vnode; NR_VNODES]>` 堆（`vnode.rs:VnodeTable`），`Vnode { fs: Endpoint, ino: u64, mode: Mode, size: u64, ref_count: usize, fs_count: usize, dev: DevId, sdev: DevId, vmnt: Option<VmntId> }`；`ref==0` 哨兵保留，与 `FilpTable` 的 `Box<[Filp]>` 同型。
- **为什么**：固定 1024 上界与 `filp` 同界但 VFS 侧独立；堆语义避免栈溢出，与 02 的 1.09 MiB 同理。

### D2 双层引用计数与 256 阈值

- **C**：`v_ref_count` 的 `dup: ++ref` 与 `put: >1 → ref--` 快速路径 vs `==1 → req_putnode` 慢路径，`v_fs_count>256 → clean_refs` 阈值。
- **Rust**：`Vnode { ref_count, fs_count }` 显式双层；`VnodeTable::dup(id) -> ()` 的 `ref++` 与 `put(id) -> Result<bool, VnodeError>` 的 `ref>1 → ref--` 快速路径 vs `ref==1 → fs_put` 慢路径（`FsCtl::put_node(fs, ino, fs_count)` 抽象）；`clean_if_needed(id) -> bool` 的 `fs_count>256 → put(fs_count-1)` 阈值。
- **为什么**：`fs_count` 的延迟同步是性能优化，256 阈值防止 `fs_count` 环绕，`ref` 与 `fs_count` 分离使 VFS 的“引用”与 FS 的“打开”语义分层。

### D3 锁族降级

- **C**：`v_lock: tll_t` 的 `VNODE_READ/OPCL/WRITE` 映射 `TLL`。
- **Rust**：`VnodeLock { state: LockState }` 的 `None/Read/Write` 借用状态；`try_lock(id, access) -> Result<(), VnodeError>` 的 `try_borrow` 语义；`VNODE_OPCL` 的 `READSER` 在 Rust 以 `ReadSer` 显式。
- **为什么**：单线程事件循环（`ARCH A-1`）下三级锁降级为借用计数；`OPCL` 的串行读在 Rust 以 `ReadSer` 单写者多读者语义保留。

### D4 查找语义显式 `Option`

- **C**：`get_free_vnode() → NULL + ENFILE`，`find_vnode(fs,ino) → NULL`。
- **Rust**：`VnodeTable::alloc() -> Result<VnodeId, VnodeError>` 的 `ENFILE` 分化与 `find_by_ino(fs, ino) -> Option<VnodeId>` 的 `None` 语义；`get_free` 的 `ref==0 && !locked` 双条件在 Rust 以 `is_free()` 显式。
- **为什么**：`ENFILE`（系统级 vnode 满）与 `None`（未命中）的错误码分化在 `VnodeError` 枚举穷尽。

### D5 挂载关联与设备语义

- **C**：`v_vmnt: vmnt*` 与 `v_dev/v_sdev/v_bfs_e` 设备号。
- **Rust**：`Vnode { vmnt: Option<VmntId>, dev: DevId, sdev: DevId }` 的三设备语义；`vmnt` 的 `Option` 与 `dev` 的 `NO_DEV` 哨兵分离。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-4 全局聚合 | `VnodeTable: Box<[Vnode]>` 堆 | `vnode.rs` + 本文档 D1 + 05 正文 2.2 |
| A-6 锁降级 | `Vnode.lock: VnodeLock` 借用 | `vnode.rs` + 本文档 D3 + 05 正文 2.5 |
| A-8 64 位 | `Mode/DevId/Ino` 已在 `minix-types`，05 新增 `VnodeId` | `vnode.rs` + 本文档 D1 + 05 正文 2.1 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── vnode.rs            — Vnode/VnodeTable/VnodeId/VnodeLock/VnodeError/get_free/find/dup/put/clean_refs
└── fproc.rs            — FProc.filps 互引 VnodeId
```

### 4.2 `vnode.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `Vnode` | `vnode.h:4` 全字段 | `vnode.rs:Vnode { fs, ino, mode, size, ref/fs_count, dev, sdev, vmnt, lock }` | `ref==0` 空闲哨兵保留 |
| `VnodeTable: Box<[Vnode]>` | `vnode.h:23` 1024 固定 | `vnode.rs:VnodeTable` | `new()` 由 `(0..NR_VNODES).map(|_| Vnode::default()).collect()` 堆构造 |
| `get_free_vnode` | `vnode.c:84` | `VnodeTable::alloc` | `ref==0 && !locked` 双条件 |
| `find_vnode` | `vnode.c:110` | `VnodeTable::find_by_ino` | `ref>0 && fs==fs && ino==ino` |
| `dup_vnode` | `vnode.c:225` | `VnodeTable::dup` | `ref++` |
| `put_vnode` | `vnode.c:238` | `VnodeTable::put` | `ref>1 → ref--` 快速 vs `ref==1 → fs_put` 慢路径 + 256 阈值 |
| `VnodeLock` | `vnode.h:22` | `vnode.rs:VnodeLock` | `None/Read/ReadSer/Write` 四态 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 空闲以 `ref==0 && !locked` | `Vnode::is_free` | `ref==0 && !locked` | `vnode.c:91` |
| 命中以 `fs+ino` | `find_by_ino` | `ref>0 && fs==fs && ino==ino` | `vnode.c:116` |
| 归零即 `req_putnode` | `put` 慢路径 | `ref==1 → put(fs_count)` | `vnode.c:282` |
| 256 阈值回收 | `clean_if_needed` | `fs_count>256 → put(fs_count-1)` | `vnode.c:263` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **63 passed / 0 failed**（`fproc` 9 + `main_loop` 11 + `worker` 8 + `call_table` 5 + `filp` 7 = 40 → 新增 `vnode` 7 后 70；`minix-types` 94 独立）。
> 本章直接影响 `2 → 7` 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_init_vnodes_zero` | `vnode.c:138-154` | `NR_VNODES` 全 `ref==0 && !locked` | `vnode.rs:200` |
| `test_get_free_vnode_double` | `vnode.c:84-104` | `ref==0 && !locked` 双条件 | `vnode.rs:210` |
| `test_find_vnode_hit` | `vnode.c:110-124` | `fs+ino` 命中 | `vnode.rs:220` |
| `test_dup_put_fast` | `vnode.c:225-264` | `dup → ref++` / `put → ref--` 快速路径 | `vnode.rs:230` |
| `test_put_slow_req_putnode` | `vnode.c:274-295` | `ref==1 → req_putnode` 慢路径 | `vnode.rs:240` |
| `test_clean_refs_threshold` | `vnode.c:263/305` | `fs_count>256 → put(fs_count-1)` | `vnode.rs:250` |
| `test_vnode_lock` | `vnode.c:156` | `VNODE_READ/WRITE` 借用语义 | `vnode.rs:260` |

测试策略：`VnodeTable` 的双条件以 `ref==0` 但 `locked` 的“不可分配”样本覆盖；`find` 以 `fs+ino` 命中/失配两样本覆盖；`dup/put` 以 `ref>1` 快速路径与 `ref==1` 慢路径两样本覆盖；`clean_refs` 以 `fs_count=257` 的阈值触发样本覆盖。

---

## 6 过渡

本篇在 `main.c:486` `init_vnodes()` 的 `sef_cb_init_fresh` 单点之后，主循环 `lookup` 的 `find_vnode` 命中之前，是 `04` 的 `filp` 中介之后、`06` 的 `vmnt` 挂载关联之前的“文件对象缓存”前提。

```
04-filp-table: filp[1024] 的 count==0 空闲 + get_fd 双扫描 + find_filp 共享
  │
  └─► 本章: vnode[1024] 的 ref==0 && !locked 双条件 + find_by_ino 命中 + dup/put 双层计数 + vnode_clean_refs 256 阈值  （init_vnodes 的 VnodeTable::new）
         │
         ├─► 06-vmnt-table: vmnt 的 get_free/mark_free 与 vmnt_unmap_by_endpt 的端点验证 （v_vmnt 的挂载实例指向）
         ├─► 13-path-lookup: 路径的 advance 的挂载点穿越 （v_vmnt != vmnt_of(dev) 即穿越）
         └─► 16-read-write: 读写的 lock_vnode 的 VNODE_READ 持有 （依赖 vnode 锁的三级可升级）
```

`vnode` 的 `NR_VNODES 1024` 固定上界为 `MIB` 的 `SI_VNODE_TAB` 快照提供 1024 槽位观测基础（`99` 将回收为全局概念）。

阅读顺序提示：若关心“缓存如何被挂载关联”，下一站 `06-vmnt-table.md`（`vmnt` 的 `get_free/mark_free` 与 `vnode` 的 `v_vmnt` 互证）；若关心“缓存如何被路径命中”，下一站 `13-path-lookup.md`（`lookup` 的 `find_vnode` 复用）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/vnode.h:4-30`（`vnode` 全字段 / `NR_VNODES 1024` / `VNODE_*` 锁映射）、`minix3/minix/servers/vfs/vnode.c:84-316`（`init_vnodes` / `get_free_vnode` 双条件 / `find_vnode` 命中 / `dup/put/clean_refs` 双层计数）、`minix3/minix/servers/vfs/const.h:8`（`NR_VNODES`）、`minix3/minix/servers/vfs/glo.h:bsf_lock`（`bsf` 锁）、`minix3/minix/servers/vfs/main.c:486`（`init_vnodes` 调用点）
- 阶段文档：`04-filp-table.md`（`FilpTable` 的 `count==0` 哨兵与 `alloc_filp` 双扫描）、`03-fproc-table.md`（`FProcTable` 的 `is_ok_endpoint` 三守卫与 `PID_FREE` 双哨兵）、`06-vmnt-table.md`（`vmnt` 表的 `get_free/mark_free` 与 `vnode` 的 `v_vmnt` 互证）、`99-global-concepts.md`（`NR_VNODES` 常量与 `Vnode` 术语）
- Rust 实现：`os/servers/vfs/src/vnode.rs:1`（`Vnode/VnodeTable/VnodeId/VnodeLock`）、`os/servers/vfs/src/filp.rs:1`（`Filp/FilpTable` 中介）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/06-proc-init-boot-proc.md`（`vnode` 与 `inode` 的 `NR_*` 同界）
