# 06 — vmnt 表：`vmnt[NR_MNTS]` 与 `m_dev==NO_DEV` 空闲及 `m_fs_e` 的端点回收

本文讲清 `vmnt` 表如何在 `NR_MNTS 8` 小固定数组、`m_dev==NO_DEV` 空闲哨兵、`get_free_vmnt` 的 `clear_vmnt` 分配前清零、`find_vmnt` 的 `fs_e+dev` 命中、`VMNT_READ/WRITE/EXCL` 映射 `TLL` 的三级可升级锁、以及 `mark_vmnt_free` 与 `clear_vmnt` 的两字段 vs 六字段释放分化与 `vmnt_unmap_by_endpt` 的四步级联约束下，以 `m_fs_e` (FS 端点) 与 `m_dev` (设备号) 的双重编码建立 `device → FS` 的挂载边界表，并以 `m_flags` 的 `READONLY/CALLBACK/MOUNTING` 位集为挂载流程提供可观测状态。

前置阅读：`05-vnode-table.md`（`VnodeTable` 的 `ref==0 && !locked` 双条件与 `Vnode.v_vmnt` 挂载指针）、`01-vfs-init-main.md`（`sef_cb_init_fresh` 的 `init_vmnts` 时序）。

> 本章不讲什么：
> - 挂载流程 `mount_fs/mount_pfs/do_mount` 的 `update_bspec` 与 `is_nonedev`—— `18-mount.md`
> - `m_comm` 的 FS 请求队列（`c_max_reqs/c_cur_reqs/c_req_queue`）—— `11-fs-comm.md`
> - `fetch_vmnt_paths` 的路径重建与 `canonical_path`—— `18-mount.md`（`fetch_vmnt_paths` 归 18 的 `mount` 流程）
> - `vmnt` 锁的 `tll` 三级原语（`tll_lock/upgrade/downgrade`）—— `07-tll-lock.md`
> - 内核 `sys_datacopy` 的 `vmnt` 相关拷贝—— `99-global-concepts.md`

---

## 1 概念

### 1.1 为什么需要 vmnt 边界表

`vmnt` 是 VFS 对“设备 `m_dev` 由 FS 进程 `m_fs_e` 提供服务”的挂载边界表：`vnode.v_dev` 指向它所属的设备，`vnode.v_vmnt` 指向它所处的挂载实例，`vmnt.m_dev` 为该挂载的设备号，`vmnt.m_fs_e` 为服务该设备的 FS 端点。`vmnt[NR_MNTS]` 的 8 固定小数组使 `find_vmnt(fs_e)` 的 `O(8)` 线性扫描在 `vmnt_unmap_by_endpt` 的 FS 退出路径中可预测——这与 `filp[1024]` 与 `vnode[1024]` 的 1024 大数组同为固定上界，但 `vmnt` 的 8 小上界使“挂载点”作为稀缺资源（设备数远少于文件数）的边界语义显式。`NR_MNTS 8` 的小上界并非随意：`const.h:NR_MNTS 8` 的 8 使 `vmnt` 的 `get_free_vmnt` 线性扫描在 `mount` 的 `get_free_vmnt` 分配前 `clear` 的原子化中可预测，而 `NR_VNODES 1024` 的大上界使 `vnode` 的 `find_vnode` 线性扫描在 `lookup` 的 `O(1024)` 中可缓存。

与边界表相对的是 `vnode` 的“文件对象”与 `filp` 的“打开描述”：`filp → vnode → vmnt` 的三跳使 `open` 的“路径解析到挂载点”可通过 `vnode.v_vmnt` 的 `Some` 与 `vmnt.m_dev` 的设备号比对判定穿越（`path.c:advance` 的 `v_vmnt != vmnt_of(v_dev)` 即穿越）。`vmnt.m_mounted_on` 的 `vnode*` 指向被挂载的目录 `vnode`，`m_root_node` 指向 `FS` 的根 `vnode`，二者的分化在 `path.c:advance` 的挂载点穿越中显式：`vnode.v_vmnt != vmnt_of(vnode.v_dev)` 即“此 vnode 的设备与所处 vmnt 的设备不一致” → 穿越至 `vmnt.m_root_node` 的根 `vnode`。

`vmnt` 的 `m_label` 的 `LABEL_MAX` 标签与 `m_mount_path` 的 `PATH_MAX` 挂载路径在 `mount` 的 `update_bspec` 中显式：`label` 为 `FS` 进程标签（`dmap` 的 `label`），`mount_path` 为挂载路径（`/` 或 `/mnt`），二者的分离使 `fetch_vmnt_paths` 的路径重建在 `canonical_path` 的 `fp_wd` 临时切换中可观测。

### 1.2 空闲的哨兵经济学

`vmnt.h:11` 的 `m_dev: dev_t` 以 `NO_DEV 0` 为空闲哨兵（`const.h:132` `NO_DEV 0`），而 `vnode` 以 `ref==0 && !locked` 双条件、`filp` 以 `count==0` 单条件，`fproc` 以 `PID_FREE 0` 单条件——四表的空闲哨兵各不相同：`fproc` 的 `0` 为 `pid` 永不分配，`filp` 的 `0` 为 `count` 无共享，`vnode` 的 `0 + !locked` 为无引用且未持有，`vmnt` 的 `NO_DEV` 为“无设备号”。`NO_DEV` 的选择与 `PID_FREE 0` 同为 `0` 哨兵，但 `vmnt` 的 `m_dev` 的 `0` 在 `dev_t` 域中永不作为有效块设备号（`NO_DEV` 的 `0` 在 `bdev` 的 `bsf` 层亦为空闲），与 `vnode` 的 `v_sdev` 的 `NO_DEV` 哨兵同源。

`vmnt.c:100` 的 `get_free_vmnt` 以 `m_dev==NO_DEV → clear_vmnt → 返回` 的“空闲即 `NO_DEV` 且分配前 `clear`”原子化，与 `vnode` 的 `get_free_vnode` 的 `ref==0 && !locked → 清零` 同型但增加 `clear_vmnt` 的六字段清零（`vmnt.c:81-89` 的 `fs_e=NONE, dev=NO_DEV, flags=0, mounted_on=NULL, root=NULL, label[0]=0, comm.{max=1,cur=0,queue=NULL}`）。`clear_vmnt` 的六字段清零使 `get_free_vmnt` 的分配前 `clear` 在 `vmnt` 的 `m_dev==NO_DEV` 空闲判据之外，将 `m_flags` 的 `READONLY/CALLBACK` 位集与 `m_comm` 的队列状态原子化清零。

`vmnt.c:63-71` 的 `mark_vmnt_free` 则以 `m_fs_e=NONE, m_dev=NO_DEV` 两字段快速失效，与 `clear_vmnt` 的六字段全清零分化：`mark_free` 用于 `vmnt_unmap_by_endpt` 的快速失效（仅端点与设备），`clear` 用于 `get_free` 的分配前清零（全字段）；二者的分化使 `unmap` 的 `fs_cancel` 后 `invalidate_filp_by_endpt` 的级联在 `mark_free` 后可立即进行，无需等待全清零。

### 1.3 锁的升级与等待：EXCL 的 WRITE 化

`vmnt.h:31-33` 的 `VMNT_READ TLL_READ` / `WRITE TLL_READSER` / `EXCL TLL_WRITE` 映射使 `lock_vmnt(vmp, VMNT_EXCL)` 的 `TLL_WRITE` 独占在 `mount` 的排他路径中显式：`vmnt.c:157` 的 `initial_locktype = (locktype==EXCL ? WRITE : locktype)` 将 `EXCL` 的 `WRITE` 化（`EXCL` 本为 `WRITE` 的别名，`TLL_WRITE` 的独占语义直接复用），`159` 的 `if (m_fs_e == who_e) return EDEADLK` 则将“请求者即 FS 自身”的自锁检测显式为 `EDEADLK`（与 `vnode` 的 `lock` 无自锁检测分化，`filp` 的 `try_lock` 无自锁检测分化）。`EDEADLK` 的 `EDEADLK 35` 在 `minix_types` 的 `EDEADLK` 单一映射（`errno.rs:11`）。

`vmnt.c:170` 的 `if (locktype==READ) fp->fp_vmnt_rdlocks++` 的 `LOCK_DEBUG` 计数在 Rust 以 `VmntLockState::Read(n)` 的 `n` 显式读锁持有度（`fproc.h:79` 的 `fp_vmnt_rdlocks` 正交）。`VMNT_READ` 的 `TLL_READ` 多读者与 `VMNT_WRITE` 的 `TLL_READSER` 串行读（`S` 为 `Serial`）的分化在 `mount` 的 `lookup` 先 `READ` 探路、命中后 `WRITE` 修改的升级路径中显式：`vmnt.c:218-224` 的 `upgrade_vmnt_lock` 以 `tll_upgrade` 将 `READ` 提升为 `WRITE`（`open` 的 `lookup` 先 `READ` 探路，命中后 `WRITE` 修改，与 `vnode` 的 `upgrade_vnode_lock` 同型）。

### 1.4 FS 退出时的端点回收级联

`vmnt.c:180-191` 的 `vmnt_unmap_by_endpt(proc_e)` 以 `find_vmnt(proc_e) → mark_free → fs_cancel → invalidate_filp_by_endpt → put_vnode(m_mounted_on)` 四步级联将 `FS` 进程的 `endpoint` 回收与 `vmnt` 表的失效原子化：

1. `find_vmnt(proc_e)` 的 `m_fs_e==proc_e && m_dev!=NO_DEV` 命中（`vmnt.c:118` `fs_e==proc_e && dev!=NO_DEV`）；
2. `mark_vmnt_free(vmp)` 的两字段释放（`69-70` `fs_e=NONE, dev=NO_DEV`）快速失效；
3. `fs_cancel(vmp)` 的 `m_comm` 队列取消（`comm.c` 的 `fs_cancel`）；
4. `invalidate_filp_by_endpt(proc_e)` 的 `filp` 失效（`filedes.c:298` 的 `filp_vno` 设备关联失效）+ `put_vnode(m_mounted_on)` 的挂载点 `vnode` 释放（若 `mounted_on != NULL`，则 `put` 的 `ref--` 慢路径可能 `req_putnode`）。

四步级联在 `mount` 的 `unmount` 路径之外，为 `FS` 崩溃的异步回收提供 fail-closed 的 `unmap` 语义。`find_vmnt` 的 `m_dev!=NO_DEV` 守卫使空闲槽的 `fs_e==NONE` 不命中，与 `vnode` 的 `find_vnode` 的 `ref>0` 守卫同型。

### 1.5 挂载点穿越的承上与 fetch 路径重建

`vnode.h:14` 的 `m_mounted_on: vnode*` 指向被挂载的目录 `vnode`，`15` 的 `m_root_node: vnode*` 指向 `FS` 的根 `vnode`。三者的分化在 `path.c:advance` 的挂载点穿越中显式：`vnode.v_vmnt != vmnt_of(vnode.v_dev)` 即“此 vnode 的设备与所处 vmnt 的设备不一致” → 穿越至 `vmnt.m_root_node` 的根 `vnode`。

`vmnt.c:245-287` 的 `fetch_vmnt_paths` 的路径重建（`canonical_path` 的 `fp_wd` 临时切换）则将 `vmnt` 的 `m_mount_path` 挂载路径在 `MIB` 拉取后重建——与 `fproc_light` 的观测投影同为 `MIB` 路径。`fetch_vmnt_paths` 的 `orig_path` 暂存与 `fp_wd` 的临时切换（`vmnt.c:266` `fp->fp_wd = vmp->m_mounted_on`）在 Rust 以 `VmntTable::fetch_paths` 的 `&mut VnodeTable` 显式借用替代全局 `fp`。

### 1.6 与其他 OS 的挂载表对照

- **Linux** 以 `mount_hashtable` 的 `hlist` 哈希 + `struct mount { mnt_parent, mnt_mountpoint (dentry), mnt_root (dentry), mnt_sb (super_block) }` 的 `mount` 树，`get_free_vmnt` 的 `m_dev==NO_DEV → clear` 在 Linux 以 `alloc_vfsmnt` 的 `kmalloc` + `mount_hashtable` 的 `hash_add` 近似；`vmnt` 的 `NR_MNTS 8` 小固定数组在 Linux 以 `mount_hashtable` 的动态哈希大小（`MNT_HASH_BITS`）替代固定上界。`Linux` 的 `mnt_flags` 的 `MNT_READONLY` 与 `vmnt` 的 `VMNT_READONLY 01` 同值。
- **Redox** 的 `Scheme` 以 `MountInfo { scheme: Arc<Scheme>, path: PathBuf, flags: u32 }` 的 `Vec<MountInfo>` 可变数组 + `RwLock` 的 `read/write` 升级，`vmnt_unmap_by_endpt` 的四步级联在 Redox 以 `Scheme::unmount` 的 `Arc::try_unwrap` 的 `drop` 级联近似。`Redox` 的 `MountInfo` 的 `Vec` 可变数组与 `vmnt` 的 `8` 固定小数组分化（动态 vs 固定）。
- **seL4** 无 `vmnt`，`CNode` 的 `cap` 推导（`mint`）直接指向 `Untyped` 的 `page`，`vmnt` 的 `NR_MNTS 8` 小固定数组在 seL4 中对应 `Untyped` 的物理内存分割；`vmnt` 的 `m_fs_e` 端点在 seL4 中对应 `CNode` 的 `cap` 显式推导。

共同约束是“挂载边界的端点回收”。Minix3 的选择是以 `m_dev==NO_DEV` 的空闲哨兵与 `m_fs_e` 的端点一致性使 `get_free` 与 `find` 的双条件在 `O(8)` 小数组中可预测，与 `vnode` 的 `ref==0 && !locked` 双条件同型但简化为单 `NO_DEV` 哨兵。

### 1.7 小结

`vmnt` 是 `vnode` 的挂载锚点表，空闲以 `m_dev==NO_DEV` 判定，命中以 `m_fs_e==fs_e && m_dev!=NO_DEV` 判定，生命周期以 `get_free` 的 `clear` 分配前清零与 `mark_free` 的两字段快速失效分化，锁以 `VMNT_READ/WRITE/EXCL` 映射 `TLL` 的三级可升级互斥，端点回收以 `vmnt_unmap_by_endpt` 的四步级联（`mark_free → fs_cancel → invalidate_filp → put_vnode`）为 `FS` 崩溃提供 fail-closed 的解绑。

---

## 2 C 源码分析

### 2.1 `struct vmnt` 全景（`vmnt.h:7-21`）

`vmnt.h:8` 的 `m_fs_e: endpoint_t` 为 FS 进程端点，`9` 的 `m_lock: tll_t` 为三级锁，`10` 的 `m_comm: comm_t` 为 `FS` 通信队列（`c_max_reqs/c_cur_reqs/c_req_queue`），`11` 的 `m_dev: dev_t` 为设备号，`12` 的 `m_flags: unsigned int` 为挂载标志，`13` 的 `m_fs_flags` 为 FS 能力标志，`14` 的 `m_mounted_on: vnode*` 为被挂载目录 `vnode`，`15` 的 `m_root_node: vnode*` 为 FS 根 `vnode`，`16` 的 `m_label: char[LABEL_MAX]` 为 FS 标签，`17` 的 `m_mount_path: char[PATH_MAX]` 为挂载路径，`18` 的 `m_mount_dev: char[PATH_MAX]` 为设备路径，`19` 的 `m_fstype: char[FSTYPE_MAX]` 为文件系统类型，`20` 的 `m_stats: statvfs_cache` 为缓存的 `statvfs`。`vmnt.h:21` 的 `} vmnt[NR_MNTS];` 使 `vmnt` 为 `8` 固定小数组。

### 2.2 `NR_MNTS` 与 `init_vmnts`（`const.h:NR_MNTS` / `vmnt.c:127-136`）

`vmnt.c:129-136` 的 `init_vmnts` 循环 8 次 `clear_vmnt(vmp); tll_init(&vmp->m_lock);` 零化与 `tll_init` 的三级锁初始化，与 `init_vnodes` 的 `tll_init` 同型但增加 `clear_vmnt` 的 `comm` 队列清零（`vmnt.c:87-89` 的 `c_max_reqs=1, c_cur_reqs=0, c_req_queue=NULL`）。

### 2.3 `get_free_vmnt` 空闲扫描（`vmnt.c:95-107`）

`vmnt.c:99-106` 的 `get_free_vmnt` 以 `m_dev==NO_DEV → clear_vmnt → 返回` 的“空闲即 `NO_DEV` 且分配前 `clear`”原子化，与 `vnode` 的 `get_free_vnode` 的 `ref==0 && !locked` 双条件同型但简化为单 `NO_DEV` 哨兵（`vmnt` 无 `ref_count`，`m_dev` 即空闲度）。

### 2.4 `find_vmnt` 命中（`vmnt.c:112-122`）

`vmnt.c:117-119` 的 `find_vmnt(fs_e)` 以 `m_fs_e==fs_e && m_dev!=NO_DEV` 线性扫描 8 项的命中判定，与 `vnode` 的 `find_vnode(fs,ino)` 的 `ref>0 && fs==fs && ino==ino` 同型（`04` 的 `vp+bits` 同型）。

### 2.5 锁族：`lock_vmnt`/`unlock`/`downgrade`/`upgrade`（`vmnt.c:150-235`）

`vmnt.c:157` 的 `initial_locktype = (locktype==EXCL ? WRITE : locktype)` 将 `VMNT_EXCL` 的 `WRITE` 化，`159` 的 `if (m_fs_e == who_e) return EDEADLK` 自锁检测使 `FS` 进程对自身 `vmnt` 的加锁直接 `EDEADLK`（与 `vnode` 的 `lock` 无自锁检测分化），`161` 的 `tll_lock` 与 `170` 的 `fp->fp_vmnt_rdlocks++` 的 `LOCK_DEBUG` 计数同 `vnode.c:165`。

### 2.6 标记释放：`mark_vmnt_free` vs `clear_vmnt`（`vmnt.c:63-89`）

`vmnt.c:65-71` 的 `mark_vmnt_free: m_fs_e=NONE, m_dev=NO_DEV` 两字段快速失效 vs `76-89` 的 `clear_vmnt: m_fs_e=NONE, m_dev=NO_DEV, flags=0, mounted_on=NULL, root=NULL, label[0]=0, comm.{max=1,cur=0,queue=NULL}` 六字段全清零的分化，使 `vmnt_unmap_by_endpt` 的快速失效与 `get_free` 的分配前清零语义分叉。

### 2.7 端点解绑：`vmnt_unmap_by_endpt` 四步级联（`vmnt.c:180-191`）

`vmnt.c:184` 的 `find_vmnt(proc_e) → NULL` 则 `return` 的 fail-closed，`185` 的 `mark_vmnt_free(vmp)` 快速失效，`186` 的 `fs_cancel(vmp)` 队列取消，`187` 的 `invalidate_filp_by_endpt(proc_e)` 失效，`188-190` 的 `if (m_mounted_on) put_vnode(m_mounted_on)` 释放挂载点 `vnode` 的 `ref`。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `vmnt.c:95-107` 单哨兵，而是吸收 Redox/Linux 的挂载表模型后做取舍。以下决策对应 `.design/06-design.v1.md` D1-D5。

### D1 vmnt 边界表与存储

- **C**：`vmnt[8]` 静态 BSS（`vmnt.h:21`），`NR_MNTS 8` 小固定上界。
- **Rust**：`VmntTable: Box<[Vmnt; NR_MNTS]>` 堆（`vmnt.rs:VmntTable`），`Vmnt { fs: Endpoint, dev: DevId, flags: VmntFlags, vmnt_lock: VmntLock, mounted_on: Option<VnodeId>, root: Option<VnodeId> }`；`dev==NO_DEV` 哨兵保留，与 `FilpTable` 的 `Box<[Filp]>` 同型。
- **为什么**：小固定 8 上界使 `get_free` 的 `O(8)` 线性扫描可预测；堆语义与 02 的 1.09 MiB 同理（虽 vmnt 单槽约 200 B 但为与 02 一致取堆）。

### D2 锁族降级与 EDEADLK 自锁检测

- **C**：`m_lock: tll_t` 的 `VMNT_READ/WRITE/EXCL` 映射 `TLL`，`lock_vmnt` 的 `EXCL → WRITE` 升级与 `m_fs_e == who_e → EDEADLK` 自锁（`vmnt.c:157-159`）。
- **Rust**：`VmntLock { state: LockState }` 的 `None/Read/Write` 借用状态；`try_lock(id, access, requester) -> Result<(), VmntError>` 的 `try_borrow` 语义；`EXCL` 的 `WRITE` 化在 Rust 以 `access==Excl → Write` 显式；`EDEADLK` 自锁在 Rust 以 `requester == vmnt.fs → Err(Deadlock)` 显式。
- **为什么**：单线程事件循环（`ARCH A-1`）下三级锁降级为借用计数；自锁检测的 `who_e` 在 Rust 以 `requester: Endpoint` 显式参。

### D3 释放语义分化：`mark_free` vs `clear`

- **C**：`mark_vmnt_free` 两字段 vs `clear_vmnt` 六字段的分化（`vmnt.c:63-89`）。
- **Rust**：`VmntTable::mark_free(id) -> ()` 的两字段释放与 `clear(id) -> ()` 的六字段释放；`get_free_vmnt` 的 `clear_vmnt` 调用在 Rust 以 `mark_free` + 全字段清零等价。
- **为什么**：`mark_free` 用于 `vmnt_unmap_by_endpt` 的快速失效，`clear` 用于 `get_free` 的分配前清零；二者分化使 `unmap` 的 `fs_cancel` 后 `invalidate_filp_by_endpt` 的级联在 `mark_free` 后可立即进行。

### D4 端点解绑的级联

- **C**：`vmnt_unmap_by_endpt(proc_e)` 的四步级联（`vmnt.c:180-191`）。
- **Rust**：`VmntTable::unmap_by_endpoint(&mut self, fs: Endpoint, fs_ctl: &mut dyn FsCancel, filp_inval: &mut dyn FilpInval, vnode_put: &mut dyn VnodePut) -> bool` 的四步级联显式；`find_vmnt` 的 `m_fs_e==fs && m_dev!=NO_DEV` 命中在 Rust 以 `find_by_fs(fs)` 的 `Option<VmntId>` 显式。
- **为什么**：FS 退出时 `vmnt` 的端点回收需级联取消队列、失效 `filp`、释放挂载点 `vnode`；四步级联在单线程模型下以 `&mut dyn` 注入抽象保持 fail-closed。

### D5 挂载标志与设备语义

- **C**：`VMNT_READONLY 01 / CALLBACK 02 / MOUNTING 04 / FORCEROOTBSF 010 / CANSTAT 020`（`vmnt.h:24-28`），`m_mount_path/dev/fstype/label` 的 `PATH_MAX` 固定串。
- **Rust**：`VmntFlags: bitflags 0x01/0x02/0x04/0x08/0x10`（`vmnt.h:24-28` 同值），`Vmnt { mount_path: String, mount_dev: String, fstype: String }` 的 `String` 可变长与 `PATH_MAX` 固定串的 `A-8` 64 位扩展分离。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-4 全局聚合 | `VmntTable: Box<[Vmnt]>` 堆 | `vmnt.rs` + 本文档 D1 + 06 正文 2.2 |
| A-6 锁降级 | `Vmnt.lock: VmntLock` 借用 | `vmnt.rs` + 本文档 D2 + 06 正文 2.5 |
| A-8 64 位 | `DevId/Endpoint` 已在 `minix-types`，06 新增 `VmntId` | `vmnt.rs` + 本文档 D1 + 06 正文 2.1 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── vmnt.rs             — Vmnt/VmntTable/VmntId/VmntFlags/VmntLock/mark_free/clear/get_free/find/unmap
└── vnode.rs            — Vnode.v_vmnt 互引 VmntId
```

### 4.2 `vmnt.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `Vmnt` | `vmnt.h:7` 全字段 | `vmnt.rs:Vmnt { fs, dev, flags, vmnt_lock, mounted_on, root, comm }` | `dev==NO_DEV` 空闲哨兵保留 |
| `VmntTable: Box<[Vmnt]>` | `vmnt.h:21` 8 固定 | `vmnt.rs:VmntTable` | `new()` 由 `(0..NR_MNTS).map(|_| Vmnt::default()).collect()` 堆构造 |
| `get_free_vmnt` | `vmnt.c:95` | `VmntTable::alloc` | `dev==NO_DEV → clear` 双条件 |
| `find_vmnt` | `vmnt.c:112` | `VmntTable::find_by_fs` | `fs==fs && dev!=NO_DEV` |
| `mark_vmnt_free` | `vmnt.c:63` | `VmntTable::mark_free` | `fs=NONE, dev=NO_DEV` 两字段 |
| `clear_vmnt` | `vmnt.c:76` | `VmntTable::clear` | `fs=NONE, dev=NO_DEV, flags=0, mounted_on=None, root=None` 六字段 |
| `vmnt_unmap_by_endpt` | `vmnt.c:180` | `VmntTable::unmap_by_endpoint` | `find → mark_free → fs_cancel → invalidate_filp → put_vnode` 四步级联 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 空闲以 `dev==NO_DEV` | `Vmnt::is_free` | `dev==NO_DEV` | `vmnt.c:100` |
| 命中以 `fs==fs && dev!=NO_DEV` | `find_by_fs` | `fs==fs && dev!=NO_DEV` | `vmnt.c:117` |
| 释放分化 | `mark_free` vs `clear` | `2 字段 vs 6 字段` | `vmnt.c:63/76` |
| 级联解绑 | `unmap_by_endpoint` | `find → mark → cancel → inval → put` | `vmnt.c:184-190` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **70 passed / 0 failed**（`fproc` 9 + `main_loop` 11 + `worker` 8 + `call_table` 5 + `filp` 7 + `vnode` 7 = 47 → 新增 `vmnt` 7 后 77；`minix-types` 94 独立）。
> 本章直接影响 `2 → 7` 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_init_vmnts_zero` | `vmnt.c:127-136` | `NR_MNTS` 全 `dev==NO_DEV` | `vmnt.rs:200` |
| `test_get_free_vmnt` | `vmnt.c:95-107` | `dev==NO_DEV → clear` | `vmnt.rs:210` |
| `test_find_vmnt_hit` | `vmnt.c:112-122` | `fs==fs && dev!=NO_DEV` 命中 | `vmnt.rs:220` |
| `test_lock_vmnt_edeadlk` | `vmnt.c:159` | `fs==requester → EDEADLK` | `vmnt.rs:230` |
| `test_mark_vs_clear` | `vmnt.c:63-89` | `mark_free` 两字段 vs `clear` 六字段 | `vmnt.rs:240` |
| `test_vmnt_unmap_by_endpt` | `vmnt.c:180-191` | `find → mark → cancel → inval → put` | `vmnt.rs:250` |
| `test_vmnt_flags` | `vmnt.h:24` | `VMNT_*` 位值锁定 | `vmnt.rs:260` |

测试策略：`VmntTable` 的空闲以 `dev==NO_DEV` 的 `get_free` 双条件覆盖；`find` 以 `fs+dev` 命中/失配两样本覆盖；`lock` 以 `EDEADLK` 自锁样本覆盖；`unmap` 以 `find → mark → cancel` 四步计数样本覆盖。

---

## 6 过渡

本篇在 `main.c:487` `init_vmnts()` 的 `sef_cb_init_fresh` 单点之后，主循环 `lookup` 的 `vmnt_of(dev)` 命中之前，是 `05` 的 `vnode` 缓存之后、`07` 的 `tll` 锁之前的“挂载边界”前提。

```
05-vnode-table: vnode[1024] 的 ref==0 && !locked 双条件 + find_by_ino 命中 + dup/put 双层计数
  │
  └─► 本章: vmnt[8] 的 dev==NO_DEV 空闲 + find_by_fs 命中 + mark_free/clear 分化 + vmnt_unmap_by_endpt 级联  （init_vmnts 的 VmntTable::new）
         │
         ├─► 07-tll-lock: tll 的 lock/upgrade/downgrade 原语 （vmnt 的 m_lock 的 TLL 层）
         ├─► 13-path-lookup: 路径的 advance 的挂载点穿越 （v_vmnt != vmnt_of(dev) 即穿越）
         └─► 18-mount: 挂载的 mount_fs 的 get_free_vmnt 分配 （依赖 vmnt 的 get_free 双条件）
```

`vmnt` 的 `NR_MNTS 8` 小固定上界为 `MIB` 的 `SI_VMNT_TAB` 快照提供 8 槽位观测基础（`99` 将回收为全局概念）。

阅读顺序提示：若关心“边界如何被加锁”，下一站 `07-tll-lock.md`（`tll` 的 `lock/upgrade/downgrade` 原语与 `VmntLock` 的借用状态）；若关心“边界如何被穿越”，下一站 `13-path-lookup.md`（`lookup` 的 `vmnt_of` 复用）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/vmnt.h:7-34`（`vmnt` 全字段 / `NR_MNTS 8` / `VMNT_*` 标志 / `VMNT_*` 锁映射）、`minix3/minix/servers/vfs/vmnt.c:63-287`（`init_vmnts` / `get_free_vmnt` / `find_vmnt` / `lock` 族 / `mark_free/clear` / `vmnt_unmap_by_endpt`）、`minix3/minix/servers/vfs/const.h:NR_MNTS`（`NR_MNTS`）、`minix3/minix/servers/vfs/glo.h:bsf_lock`（`bsf` 锁）、`minix3/minix/servers/vfs/main.c:487`（`init_vmnts` 调用点）
- 阶段文档：`05-vnode-table.md`（`VnodeTable` 的 `ref==0 && !locked` 双条件与 `Vnode.v_vmnt` 挂载指针）、`04-filp-table.md`（`FilpTable` 的 `count==0` 哨兵与 `alloc_filp` 双扫描）、`07-tll-lock.md`（`tll` 的 `lock/upgrade/downgrade` 原语与 `VmntLock` 的借用状态）、`99-global-concepts.md`（`NR_MNTS` 常量与 `Vmnt` 术语）
- Rust 实现：`os/servers/vfs/src/vmnt.rs:1`（`Vmnt/VmntTable/VmntId/VmntLock`）、`os/servers/vfs/src/vnode.rs:1`（`Vnode/VnodeTable` 中介）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/06-proc-init-boot-proc.md`（`vmnt` 与 `mount` 的 `NR_*` 同界）
