# 15-stage-fs 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/15-stage-fs/`
> **目标**: 以 **FS 子系统语义为主线**重组 FS 全部文档；VFS→FS 请求协议为次主线；最终覆盖 Minix3 FS 子系统全部语义（libfsdriver + libminixfs + libvtreefs + libsffs + 8 个 FS server），支撑 `os/fs/*`（8 crate）+ `os/libs/minix-fs` 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/plan.md` + `14-stage-runtime/plan.md`（plan 结构参照；14 为非 server 主线重定义先例）、`minix3/minix/fs/` + `minix3/minix/lib/libfsdriver/` + `minix3/minix/lib/libminixfs/` + `minix3/minix/lib/libvtreefs/` + `minix3/minix/lib/libsffs/`（ground truth）、`os/fs/*` + `os/libs/minix-fs/`（Rust 实现）

---

## 1. 背景与动机

### 1.1 与 VM 的差异：FS 不是单个 server，主线重新定义

`02-stage-vm` 以 **VM server 启动顺序为主线**——VM 是一个有明确 `init_vm()` 启动链 + 主循环的用户态服务。**15-stage-fs 不是单个 server**：它覆盖 8 个 FS server（`minix3/minix/fs/`）+ 4 个共享框架库（`libfsdriver`/`libminixfs`/`libvtreefs`/`libsffs`）：

| 层 | 内容 | C 源码 | 行数 | 性质 |
|----|------|--------|------|------|
| 框架 | libfsdriver | `lib/libfsdriver/`（fsdriver.c/table.c/call.c/lookup.c/dentry.c/utility.c） | 1771 | 所有 FS server 的消息循环 + 请求适配（mfs/pfs/ptyfs/ext2/isofs 直接使用；VTreeFS/SFFS 内部复用） |
| 框架 | libminixfs | `lib/libminixfs/`（bio.c/cache.c） | 1594 | 磁盘 FS 的块缓存 + 块 I/O（mfs/ext2/isofs 使用） |
| 框架 | libvtreefs | `lib/libvtreefs/`（10 个 .c + 4 个 .h） | 3562 | 虚拟树 FS 框架（procfs 使用；devman/gpio 也使用，跨 stage） |
| 框架 | libsffs | `lib/libsffs/`（15 个 .c + 6 个 .h） | 2055 | 宿主共享文件夹 FS 框架（vbfs/hgfs 使用） |
| server | pfs | `fs/pfs/pfs.c` | 451 | **boot 第一个挂载的 FS**（`servers/vfs/main.c:491` mount_pfs）+ 最小完整 server 样例 |
| server | mfs | `fs/mfs/`（17 .c + 10 .h） | 4096 | **参考实现**：V2/V3 磁盘格式、inode/块缓存、完整读写语义（boot 根 FS，`kernel/table.c:63`） |
| server | procfs | `fs/procfs/`（8 .c） | 1703 | 进程信息虚拟 FS（VTreeFS 之上，RS 运行时加载） |
| server | ptyfs | `fs/ptyfs/`（ptyfs.c/node.c） | 518 | `/dev/pts` 伪终端树（fsdriver 之上，RS 运行时加载） |
| server | ext2 | `fs/ext2/`（17 .c） | ~3900 | ext2 磁盘 FS（libminixfs 变体，完整实现） |
| server | isofs | `fs/isofs/`（12 .c） | 1509 | ISO9660 + Rock Ridge 只读 FS（libminixfs 变体） |
| server | vbfs/hgfs | `fs/vbfs/vbfs.c` + `fs/hgfs/hgfs.c` | 247 | 宿主共享文件夹桥接（libsffs 之上，后置） |

旧内容仅有占位 `README.md`（已移入 `draft/README.md`），其 scope 定义（8 server + minix-fs 框架 + boot 关键路径）保留为素材，本 plan 将其扩展为完整语义覆盖契约。

### 1.2 新主线：FS 子系统语义（框架 → boot 顺序 → 参考实现 → 变体）

与 `01-stage-kernel` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。FS 子系统没有单一 server 启动链，但有清晰的**boot 执行顺序 + 语义依赖顺序**：

```
VFS boot（servers/vfs/main.c:491 do_init_root — 05-stage-vfs 已覆盖）
  │
  ├─ mount_pfs()                      ← 06：PFS 先挂载（管道/clone 设备，boot 第一个 FS）
  │
  ▼  mount_fs(DEV_IMGRD, "/", MFS)    ← 07~17：MFS 挂载 boot ramdisk 根 FS（参考实现）
  │
  ▼  RS 运行时加载                     ← 18~20：VTreeFS/procfs/ptyfs
  │
  ▼  按需挂载                          ← 21~24：ext2/isofs/vbfs/hgfs
```

每个 FS server 的执行骨架相同（SEF 启动 → `fsdriver_task(&table)` 主循环），因此**框架语义前置**：

```
阶段 1  libfsdriver（01~03）：消息循环 + REQ_* 协议 + 请求适配 + 查找/目录项辅助
阶段 2  libminixfs（04/05）：块缓存 + 块 I/O
阶段 3  pfs（06）：boot 第一个挂载的 FS，最小完整 server 样例（fsdriver 框架的首次完整消费）
阶段 4  mfs（07~17）：参考实现——启动/挂载 → 元数据(inode/super) → 路径 → 数据通路
阶段 5  虚拟树变体（18~20）：VTreeFS 框架 → procfs → ptyfs
阶段 6  磁盘/宿主变体（21~24）：ext2 → isofs → vbfs/hgfs(SFFS)
```

**每篇文档必须能回答一个问题：它位于 FS 子系统的哪个语义层（框架/参考实现/变体），以及在该层中处于哪个执行位置（启动/挂载/主循环/服务）。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则，由 `14-stage-runtime/plan.md §1.1`（非 server 主线重定义先例）迁移而来。

### 1.3 VFS→FS 请求协议次主线

FS server 的全部工作由 VFS 驱动（`minix3/minix/include/minix/vfsif.h`，34 个 REQ）。协议不作为概念引入的驱动，而是作为**次主线**在框架文档（01/02）中完整定义，在各 server 文档中按请求展开：

```
VFS 请求到达（m_type = TRNS_ADD_ID(call_nr, transid)，m_source = VFS_PROC_NR）
  │
  ▼  fsdriver_process（fsdriver.c:29）
  ├─ 非 FS 请求/通知 → fdr_other 回调        ← 01/20（ptyfs_other 等）
  ├─ 未挂载且非 READSUPER → EINVAL
  ▼  call_nr -= FS_BASE，查 fsdriver_callvec  ← 01（table.c，32 项）
  ├─ REQ_READSUPER  → fsdriver_readsuper     ← 02（挂载组：dev/grant/flags → fdr_mount → 根节点回复）
  ├─ REQ_LOOKUP     → fsdriver_lookup        ← 03（fdr_lookup：单组件查找 + EENTERMOUNT/ELEAVEMOUNT/ESYMLINK）
  ├─ REQ_READ/WRITE/PEEK/GETDENTS/FTRUNC     ← 02 数据组 → mfs 15/16
  ├─ REQ_CREATE/MKDIR/MKNOD/LINK/UNLINK/
  │    RMDIR/RENAME/SLINK/RDLINK             ← 02 命名空间组 → mfs 12/13/14
  ├─ REQ_STAT/CHOWN/CHMOD/UTIME/STATVFS      ← 02 元数据组 → mfs 17
  ├─ REQ_BREAD/BWRITE/BPEEK/BFLUSH/
  │    NEW_DRIVER/FLUSH                      ← 02 块组 → 04/05
  └─ 回复：m_out.m_type = TRNS_ADD_ID(r, transid)
```

**关键协议点（vfsif.h）**：`REQ_* = FS_BASE + 1..33`；`FS_BASE` 为负数基址；`TRNS_GET_ID`/`TRNS_ADD_ID` 事务号（低位 16 位）；`IS_FS_RQ` 判定；`EENTERMOUNT(-301)`/`ELEAVEMOUNT(-302)`/`ESYMLINK(-303)` 特殊错误码（挂载点/符号链接遍历）；`RES_THREADED`/`RES_HASPEEK`/`RES_64BIT` 能力位；`REQ_RDONLY`/`REQ_ISROOT` 挂载标志；`vfs_ucred_t` 凭据结构。

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel`/`02-stage-vm`/`14-stage-runtime` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。draft 素材保留原样（仅 `draft/README.md`）。

### 阶段总览（24 篇 + 00 + 99 = 26 篇）

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | 变更 |
|------|------|------|---------|--------|-----------|------|
| 0 总览 | 00 | `00-fs-overview.md` | FS 子系统是什么、语义主线图、文档导航、boot 挂载因果链 | `fs/` 全部 + `lib/fsdriver` 等 | 全部 | 新建 |
| 1 框架·libfsdriver | 01 | `01-fsdriver-task.md` | fsdriver_task 主循环、fsdriver_process 分发、transid、mounted 状态、terminate、REQ_* 协议全集（vfsif.h）、callvec 分发表（table.c） | `libfsdriver/fsdriver.c`、`table.c`、`minix/include/minix/vfsif.h` | `minix-fs`（driver 框架） | 新建 |
| 1 | 02 | `02-fsdriver-call.md` | call.c 31 个请求适配器：参数提取/校验 → fdr_* 回调 → 回复构造；按 6 组组织（挂载/节点/数据/命名空间/元数据/块） | `libfsdriver/call.c` | `minix-fs`（请求适配） | 新建 |
| 1 | 03 | `03-fsdriver-utility.md` | fsdriver_copyin/copyout/zero、fsdriver_getname（grant→name）、fsdriver_dentry_init/add/finish（getdents 组装）、fsdriver_lookup（挂载点/符号链接语义） | `libfsdriver/utility.c`、`dentry.c`、`lookup.c` | `minix-fs`（copy/dentry/lookup 辅助） | 新建 |
| 2 框架·libminixfs | 04 | `04-block-cache.md` | 缓冲区池（lmfs_buf_pool）、hash 表、LRU（front/rear）、count 引用、dirty 标记、写回策略、readahead、VM 二级缓存（lmfs_may_use_vmcache）、block usage 统计、cache_resize/heuristic | `libminixfs/cache.c` | `minix-fs`（block cache） | 新建 |
| 2 | 05 | `05-block-io.md` | lmfs_driver（label→块驱动端点）、lmfs_bio（bread/bwrite/bpeek）、lmfs_bflush、NEW_DRIVER 处理、与 minix-bdev 交互 | `libminixfs/bio.c` | `minix-fs`（bio）、`minix-bdev` | 新建 |
| 3 变体·boot 第一个挂载 | 06 | `06-pfs.md` | PFS：512 inode 表、pfs_mount（无根节点）、newnode/putnode/read/write/trunc/stat/chmod 9 个回调、管道/clone 设备语义、与 VFS pipe.c/cdev.c 交互；**最小完整 server 样例** | `fs/pfs/pfs.c` | `os/fs/pfs` | 新建 |
| 4 mfs·启动与挂载 | 07 | `07-mfs-init-main.md` | mfs 启动链：SEF init_fresh → inode 表初始化 → init_inode_cache → lmfs_buf_pool → fsdriver_task；mfs_table 回调表（31 项）；cache.c 包装层（get_block/alloc_zone/free_zone） | `mfs/main.c`、`table.c`、`cache.c` | `os/fs/mfs`（main） | 新建 |
| 4 | 08 | `08-mfs-super.md` | super_block 结构、磁盘布局（boot/super/位图/inode/数据区）、read_super/write_super、alloc_bit/free_bit、V2/V3 magic（0x2468/0x4d5a）、clean 标志、字节序（s_native） | `mfs/super.c`、`super.h`、`const.h`（部分） | `os/fs/mfs`（super） | 新建 |
| 4 mfs·核心数据结构 | 09 | `09-mfs-inode.md` | inode 表、hash、unused 链表、get/put/find/dup/alloc_inode、rw_inode（d2_inode 磁盘格式）、update_times、i_count 引用计数 | `mfs/inode.c`、`inode.h`、`type.h` | `os/fs/mfs`（inode） | 新建 |
| 4 | 10 | `10-mfs-mount.md` | fs_mount（bdev_open → read_super → clean 检查 → blocksize → root inode）、fs_unmount（busy 检查 → sync → clean 标记 → lmfs_invalidate）、fs_mountpt | `mfs/mount.c` | `os/fs/mfs`（mount） | 新建 |
| 4 mfs·命名空间 | 11 | `11-mfs-path.md` | fs_lookup（单组件）、advance、search_dir（LOOK_UP/ENTER/DELETE/IS_EMPTY）、struct direct 目录项格式（mfsdir.h） | `mfs/path.c`、`mfsdir.h` | `os/fs/mfs`（path） | 新建 |
| 4 | 12 | `12-mfs-open.md` | fs_create/fs_mkdir/fs_mknod/fs_slink/fs_seek、new_node（inode 分配 + 目录项 ENTER） | `mfs/open.c` | `os/fs/mfs`（open） | 新建 |
| 4 | 13 | `13-mfs-link.md` | fs_link/fs_unlink/fs_rmdir/fs_rename/fs_slink/fs_rdlink、remove_dir/unlink_file、truncate_inode/freesp_inode、nextblock/zerozone_* | `mfs/link.c` | `os/fs/mfs`（link） | 新建 |
| 4 mfs·数据通路 | 14 | `14-mfs-read.md` | fs_readwrite（读路径）、read_map（直接/间接/双重间接块寻址）、rd_indir、get_block_map、rahead、fs_getdents、rw_chunk | `mfs/read.c` | `os/fs/mfs`（read） | 新建 |
| 4 | 15 | `15-mfs-write.md` | write_map（WMAP_FREE）、new_block、clear_zone、zero_block、wr_indir/empty_indir、空洞语义 | `mfs/write.c` | `os/fs/mfs`（write） | 新建 |
| 4 mfs·元数据与维护 | 16 | `16-mfs-metadata.md` | fs_chmod/fs_chown（protect）、fs_stat/fs_statvfs（stadir）、fs_utime（time）、conv2/conv4（utility） | `mfs/protect.c`、`stadir.c`、`time.c`、`utility.c` | `os/fs/mfs`（metadata） | 新建 |
| 4 | 17 | `17-mfs-maint.md` | fs_sync（misc）、count_free_bits（stats）、常量表（const.h：NR_INODES/SUPER_MAGIC/区格式）、clean.h 宏、glo.h 全局状态 | `mfs/misc.c`、`stats.c`、`const.h`、`clean.h`、`glo.h` | `os/fs/mfs`（misc） | 新建 |
| 5 变体·虚拟树 | 18 | `18-vtreefs.md` | VTreeFS 框架：inode 树（index_t 索引）、fs_hooks 回调、fsdriver 复用（vtreefs_table）、init_inodes/init_extra/init_buf、path/link/mount/stadir/file 语义、sdbm 哈希；**跨 stage 框架（devman/gpio 复用）** | `lib/libvtreefs/` 全部 | `os/fs/procfs`（框架部分，或独立 `minix-vtreefs`） | 新建 |
| 5 | 19 | `19-procfs.md` | procfs：root/tree/pid/service/cpuinfo/buf/util、静态树构造、NR_PROCS+NR_TASKS 索引、动态 pid 子树、hooks 实现 | `fs/procfs/` 全部 | `os/fs/procfs` | 新建 |
| 5 | 20 | `20-ptyfs.md` | ptyfs：挂载（非 root）、lookup（make_name/parse_name）、getdents、stat/chown/chmod/statvfs、fdr_other（PTYFS_SET/PTYFS_DEL，ds label 'pty' 校验）、node.c | `fs/ptyfs/ptyfs.c`、`node.c` | `os/fs/ptyfs` | 新建 |
| 6 变体·磁盘/宿主 | 21 | `21-ext2-init-mount.md` | ext2 启动/挂载/分配：main/table/mount/super/balloc（块分配+prealloc）/ialloc（Orlov 分配器）/misc、与 mfs 的差异 | `fs/ext2/main.c`、`table.c`、`mount.c`、`super.c`、`balloc.c`、`ialloc.c`、`misc.c` | `os/fs/ext2` | 新建 |
| 6 | 22 | `22-ext2-namespace-data.md` | ext2 命名空间/数据/元数据：path/open/link/read/write/protect/stadir/time/utility、inode.c（ext2 格式 inode） | `fs/ext2/path.c`、`open.c`、`link.c`、`read.c`、`write.c`、`protect.c`、`stadir.c`、`time.c`、`utility.c`、`inode.c` | `os/fs/ext2` | 新建 |
| 6 | 23 | `23-isofs.md` | ISO9660 只读 FS：mount/super/inode/path/read/stadir/link（empty）、Rock Ridge（susp.c/susp_rock_ridge.c）、norock 选项 | `fs/isofs/` 全部 | `os/fs/isofs` | 新建 |
| 6 | 24 | `24-vbfs-hgfs.md` | SFFS 框架（sffs_init/loop、sffs_table 回调、path/name/verify/dentry 语义）+ vbfs（VBoxFS 桥）+ hgfs（VMware HGFS 桥）、optset 参数 | `lib/libsffs/` 全部 + `fs/vbfs/vbfs.c` + `fs/hgfs/hgfs.c` | `os/fs/vbfs`、`os/fs/hgfs` | 新建 |
| 99 全局概念 | 99 | `99-global-concepts.md` | FS_BASE/REQ 常量表、fsdriver_node/data/dentry 结构、endpoint 约定、服务号（MFS/PFS/PROC/PTY 等）、errno 映射表 | `vfsif.h`、`fsdriver.h`、`libminixfs.h`、`sffs.h`、`vtreefs.h`、`com.h` | `minix-types` | 新建 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在语义层中的位置与下一阶段的入口：

```
00（总览）→ 01~03（fsdriver 框架）→ 04/05（块缓存/IO）→ 06（pfs：最小 server 样例）
→ 07~17（mfs 参考实现：启动→super/inode→路径→数据→元数据/维护）
→ 18~20（VTreeFS/procfs/ptyfs 虚拟树变体）→ 21/22（ext2）→ 23（isofs）→ 24（vbfs/hgfs）
→ 99（全局概念）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel / 02-stage-vm plan §3 / 14-stage-runtime plan §3）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel`/`02-stage-vm`/`14-stage-runtime` 一致：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的对应、`[ARCH]` 标注
4. **错误处理**——errno 映射（P0：错误类型必须映射 Minix3 errno 值；含 EENTERMOUNT/ELEAVEMOUNT/ESYMLINK 协议错误码）
5. **测试**——该文档语义模块的 Rust 单测清单与统计
6. **过渡**——本阶段在子系统语义层中的位置 + 下一阶段入口
7. **参见**——绝对路径引用（doc/code/C 源），绝不引用 `.design/`/`tmp_design_and_todo/`

### 3.2 引用规则

- 各文档之间用新编号交叉引用（如 `13-mfs-link.md` §fs_rename）
- 与 kernel/VM/VFS 文档交叉引用时用 `../01-stage-kernel/NN-*.md`、`../02-stage-vm/NN-*.md`、`../05-stage-vfs/NN-*.md`
- VFS 侧对 FS 的引用（`05-stage-vfs/11-fs-comm.md` 等）需同步更新为新编号
- 对 draft 素材的引用一律指向 `draft/README.md`，并标注"素材"
- VTreeFS 跨 stage（devman 11-stage 也使用）：`18-vtreefs.md` 声明框架语义，`../11-stage-devman/` 引用之

### 3.3 每篇文档的边界声明

每篇必须含"前置依赖 / 本篇不覆盖什么"声明，写作时禁止内容交叉。关键边界：

| 文档 | 前置依赖 | 职责 | 不覆盖（移交） |
|------|---------|------|---------------|
| 00 | 无 | FS 子系统全景、主线图、导航 | 一切机制细节（01~24） |
| 01 | 00、`../05-stage-vfs/09-main-loop.md` | 主循环/分发/REQ 协议 | 请求适配细节（02）、协议常量值（99） |
| 02 | 01 | 31 个请求适配器 | fdr_* 服务端实现（06~24）、copy/dentry 辅助（03） |
| 03 | 01/02 | copy/copyout/zero、getname、dentry 组装、lookup 挂载点语义 | 各 FS 的 fs_lookup 实现（11/19/20/22/23） |
| 04 | 01 | 块缓存池/LRU/dirty/写回/VM 二级缓存 | 块驱动通信（05）、inode 缓存（09） |
| 05 | 04 | 块 I/O 协议、driver 绑定 | 块设备驱动实现（16-stage-drivers）、缓存策略（04） |
| 06 | 01/02 | PFS inode 表/管道语义/最小 server 样例 | 管道 VFS 侧实现（05-stage-vfs/17-pipe）、磁盘 FS 语义（07~17） |
| 07 | 01/04/05 | mfs 启动链、回调表、get_block/alloc_zone 包装 | super/inode 细节（08/09）、主循环（01） |
| 08 | 07 | superblock/位图/磁盘布局/版本 | inode 生命周期（09）、挂载流程（10） |
| 09 | 07/08 | inode 表/缓存/磁盘格式转换 | 路径查找（11）、数据通路（14/15） |
| 10 | 07~09 | 挂载/卸载/挂载点检查 | super 读写细节（08）、VFS 侧挂载（05-stage-vfs/18-mount） |
| 11 | 09 | 单组件查找/目录项格式 | 多组件路径（VFS 侧 05-stage-vfs/13-path-lookup）、数据读取（14） |
| 12 | 09/11 | create/mkdir/mknod/slink/seek | 链接语义（13）、数据写入（15） |
| 13 | 09/11 | link/unlink/rmdir/rename/rdlink/trunc | open 语义（12）、块释放细节（15 write_map WMAP_FREE） |
| 14 | 09/11 | 读路径/块寻址/getdents | 写路径（15）、请求适配（02） |
| 15 | 09/14 | 写路径/块分配/空洞 | 读路径（14）、截断（13） |
| 16 | 09 | chmod/chown/stat/statvfs/utime | 权限检查在 VFS（05-stage-vfs/29-protect）、磁盘格式（08） |
| 17 | 08 | sync/统计/常量 | 各常量使用点（各文档） |
| 18 | 01/02 | VTreeFS 框架（inode 树/hooks/表） | procfs 具体内容（19）、devman 用法（11-stage-devman） |
| 19 | 18 | procfs 树/静态/动态节点/进程信息 | VTreeFS 框架（18）、内核进程表（01-stage-kernel） |
| 20 | 01/02 | ptyfs 树/挂载/other 消息 | TTY 驱动（16-stage-drivers）、VFS 侧 /dev/pts 挂载 |
| 21 | 04~10 | ext2 启动/挂载/分配器 | ext2 命名空间/数据（22）、mfs 对照语义（07~17） |
| 22 | 21 | ext2 路径/数据/元数据 | ext2 分配器（21） |
| 23 | 04~10 | ISO9660 + Rock Ridge | mfs 对照语义（07~17）、光盘驱动（16-stage-drivers） |
| 24 | 01 | SFFS 框架 + vbfs/hgfs 桥 | VBox/VMware 驱动（16-stage-drivers） |
| 99 | 无 | 常量值全集/结构定义 | 常量如何被使用（各文档） |

### 3.4 测试基线（截至 2026-08-16）

- `os/fs/*` 8 个 crate 为 stub（仅 `init()` 空实现），`os/libs/minix-fs` 为空 lib；`cargo test` 无实质测试
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.5 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/fs/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

### 3.6 变体文档写作原则（参考实现 + 差异展开）

mfs（07~17）定义磁盘 FS 的完整语义（super/inode/路径/数据/元数据）；变体 server（pfs/procfs/ptyfs/ext2/isofs/vbfs/hgfs）**不重复这些语义**，每篇变体文档按以下结构写作：

1. **启动/挂载差异**——SEF init 内容、mount 回调差异（如 pfs 无根节点、ptyfs 拒绝 root 挂载、isofs 只读）
2. **fdr 回调差异矩阵**——该 server 实现/未实现的回调表（如 pfs 仅 9 项、ptyfs 8 项），未实现回调 → ENOSYS 语义
3. **特有数据结构**——如 pfs 内存 inode 表、ext2 inode/组描述符、isofs Rock Ridge 属性
4. **特有协议/交互**——如 ptyfs PTYFS_SET/PTYFS_DEL、procfs 内核进程表查询、SFFS 宿主回调
5. **C 文件 API 面映射表**——该 server 全部 .c 文件逐一对齐到文档小节（覆盖契约，§5.1 核对列）

此原则保证：mfs 语义只写一次，变体只写差异，总文档数可控（26 篇），且每个 server 的 C 文件全部有归属（§5.1 无遗漏）。

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。**FS crate 当前为 stub，以下为设计期候选 ARCH 项**，写文档时必须逐项确认/更新状态；minix-rs 侧已实现的 ARCH（如 02-stage-vm 的 Direct Map）不属于本 stage。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进（候选） | 涉及新文档 | 状态 |
|---|---------|------------|---------------------|-----------|------|
| A-1 | fsdriver 回调表 | `struct fsdriver` 函数指针表（fsdriver.h） | Rust trait（如 `FsDriver`）+ 枚举/分发器，类型化回调 | 01/02 | 设计期（stub，待确认） |
| A-2 | 块缓存 | 全局 buf 数组 + hash + LRU（cache.c 1321 行） | `minix-fs` crate 泛型缓存（`BlockCache` trait），no_std + 分配器（依赖 02-stage-vm HeapArena/VM mmap） | 04 | 设计期 |
| A-3 | 磁盘 inode/目录项 | packed struct + 隐式字节序（d2_inode/struct direct） | `repr(packed)`/zerocopy + 显式字节序（little-endian 目标），类型安全解析 | 09/11 | 设计期 |
| A-4 | 64 位文件语义 | `i32_t i_size`、`zone_t`（32 位）、`RES_64BIT` 能力位 | `u64` 全程文件大小/块号；RES_64BIT 语义保留 | 14/15/99 | 设计期 |
| A-5 | 单线程执行模型 | C fsdriver 单线程消息循环 | Rust 单线程事件循环（`!Send`/`!Sync`，与执行模型约束一致）；`RES_THREADED` 不支持 | 01/00 | 设计期 |
| A-6 | SEF/Live Update | `SEF_CB_INIT_RESTART_STATEFUL` + SIGTERM → fs_sync → fsdriver_terminate | 信号 → sync → 优雅终止；Live Update 与 02-stage-vm A-8 同步 fail-closed | 07/17 | 设计期 |
| A-7 | VM 二级缓存 | `lmfs_may_use_vmcache` + VM 缓存交互（lmfs_get_block_ino/lmfs_zero_block_ino） | 依赖 `02-stage-vm/24-page-cache.md` 语义；minix-rs 缓存层集成 | 04 | 设计期 |
| A-8 | 只读/脏标志 | `MFSFLAG_CLEAN` + unclean 时自动降级只读（mount.c） | 状态机建模（Clean/Dirty/ReadOnly） | 08/10 | 设计期 |
| A-9 | 字节序/多架构 | `s_native` + `le_CPU` assert（ext2） | x86-64 目标，Rust 端消除运行时字节序判断（编译期保证） | 08/21 | 设计期 |
| A-10 | 错误路径 | mfs 块错误 panic（cache.c:19-24）、`err_code` 全局 | `Result<T, FsError>` 类型化错误，errno 映射 | 07/17/99 | 设计期 |
| A-11 | 磁盘格式兼容 | Minix V2/V3（mfs）、ext2、ISO9660 原生格式 | **兼容策略决策**：读旧磁盘镜像 vs 仅自举格式（blockdev 工具链面） | 00/08/21/23 | 设计期（重大决策） |
| A-12 | VTreeFS/SFFS | C 回调 hooks 框架（vtreefs.h/sffs.h） | Rust trait 化；VTreeFS 为 procfs 前置（18），SFFS **可能 defer**（vbfs/hgfs 非 boot 关键路径） | 18/24 | 设计期（SFFS defer 候选） |
| A-13 | 块大小多样性 | isofs 子页块大小（FIXME 注释）、`lmfs_set_blocksize` 动态 | `BlockSize` 泛型/动态块大小 trait | 04/23 | 设计期 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7。

### 5.1 C 源文件 → 新文档映射（.c 全量）

**`minix3/minix/lib/libfsdriver/`（框架）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `fsdriver.c` | 97 | 01 | 已核对（主循环/process/terminate） |
| `table.c` | 40 | 01 | 已核对（callvec 分发表 32 项） |
| `call.c` | 1013 | 02 | 已核对（31 个适配器，6 组） |
| `lookup.c` | 333 | 03 | 已核对（fsdriver_lookup 挂载点/符号链接） |
| `dentry.c` | 99 | 03 | 已核对（dentry_init/add/finish） |
| `utility.c` | 105 | 03 | 已核对（copyin/copyout/zero/getname） |

**`minix3/minix/lib/libminixfs/`（框架）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `bio.c` | 263 | 05 | 已核对（lmfs_driver/bio/bflush） |
| `cache.c` | 1321 | 04 | 已核对（缓冲池/LRU/hash/dirty/写回/readahead/VM 缓存） |
| `inc.h` | 10 | 04 | 已核对（get_partial_block/readahead 声明） |

**`minix3/minix/fs/pfs/`（boot 第一个挂载）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `pfs.c` | 451 | 06 | 已核对（7 个 fdr 回调 + inode 表） |

**`minix3/minix/fs/mfs/`（参考实现）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `main.c` | 102 | 07 | 已核对（SEF 启动链） |
| `table.c` | 45 | 07 | 已核对（mfs_table 29 项回调） |
| `cache.c` | 109 | 07 | 已核对（get_block/alloc_zone/free_zone 包装） |
| `super.c` | 365 | 08 | 已核对 |
| `inode.c` | 464 | 09 | 已核对 |
| `mount.c` | 173 | 10 | 已核对 |
| `path.c` | 241 | 11 | 已核对 |
| `open.c` | 270 | 12 | 已核对 |
| `link.c` | 638 | 13 | 已核对 |
| `read.c` | 556 | 14 | 已核对 |
| `write.c` | 319 | 15 | 已核对 |
| `protect.c` | 58 | 16 | 已核对 |
| `stadir.c` | 104 | 16 | 已核对 |
| `time.c` | 49 | 16 | 已核对（fs_utime） |
| `utility.c` | 36 | 16 | 已核对（conv2/conv4） |
| `misc.c` | 23 | 17 | 已核对（fs_sync） |
| `stats.c` | 89 | 17 | 已核对（count_free_bits） |

**`minix3/minix/fs/`（变体 server）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `procfs/main.c` + `root.c` + `tree.c` + `pid.c` + `service.c` + `cpuinfo.c` + `buf.c` + `util.c` | 1703 | 19 | 已核对（VTreeFS hooks） |
| `ptyfs/ptyfs.c` + `node.c` | 518 | 20 | 已核对 |
| `ext2/main.c` + `table.c` + `mount.c` + `super.c` + `balloc.c` + `ialloc.c` + `misc.c` | 1711 | 21 | 已核对 |
| `ext2/path.c` + `open.c` + `link.c` + `read.c` + `write.c` + `protect.c` + `stadir.c` + `time.c` + `utility.c` + `inode.c` | 2994 | 22 | 已核对 |
| `isofs/*.c`（12 个，含 susp.c/susp_rock_ridge.c） | 1509 | 23 | 已核对 |
| `vbfs/vbfs.c` + `hgfs/hgfs.c` | 247 | 24 | 已核对 |

**`minix3/minix/lib/`（变体框架）**

| C 文件 | 行数 | 新文档 | 核对 |
|--------|------|--------|------|
| `libvtreefs/vtreefs.c` + `table.c` + `inode.c` + `path.c` + `link.c` + `mount.c` + `stadir.c` + `file.c` + `extra.c` + `sdbm.c`（10 个） | 3562 | 18 | 已核对 |
| `libsffs/main.c` + `table.c` + `dentry.c` + `handle.c` + `inode.c` + `link.c` + `lookup.c` + `misc.c` + `mount.c` + `name.c` + `path.c` + `read.c` + `stat.c` + `verify.c` + `write.c`（15 个） | 2055 | 24 | 已核对 |

### 5.2 头文件覆盖

| 头文件 | 归属 | 核对 |
|--------|------|------|
| `minix/include/minix/vfsif.h`（REQ_*/flags/错误码/ucred） | 01/99 | 已核对 |
| `minix/include/minix/fsdriver.h`（struct fsdriver/fsdriver_node/data/dentry） | 01/02/99 | 已核对 |
| `minix/include/minix/libminixfs.h`（lmfs_* API + struct buf） | 04/99 | 已核对 |
| `minix/include/minix/vtreefs.h`（struct fs_hooks） | 18 | 已核对 |
| `minix/include/minix/sffs.h`（sffs_table/params/attr） | 24 | 已核对 |
| `mfs/const.h`（V2/V3 常量/区格式） | 08/17 | 已核对 |
| `mfs/super.h`（super_block 结构/磁盘布局） | 08 | 已核对 |
| `mfs/inode.h` + `type.h`（内存/磁盘 inode） | 09 | 已核对 |
| `mfs/buf.h`（union ixfer_fsdata_u 视图） | 04/07 | 已核对 |
| `mfs/mfsdir.h`（struct direct） | 11 | 已核对 |
| `mfs/clean.h` + `glo.h` + `fs.h` + `proto.h`（函数原型全表） | 17/07/00 | 已核对 |
| `sys/dirent.h`（getdents 输出格式） | 03/14 | 已核对 |

### 5.3 协议与消息布局

- VFS→FS 请求协议全集：`vfsif.h` REQ_GETNODE(1)~REQ_BPEEK(33)，`NREQS=34`，`IS_FS_RQ` 判定 → 01/99
- 消息布局（m_vfs_fs_* / m_fs_vfs_*）：`minix/ipc.h` 各请求/回复结构体 → 02/99
- 事务号（transid）编码：`TRNS_GET_ID`/`TRNS_ADD_ID`/`TRNS_DEL_ID` → 01
- 特殊错误码：EENTERMOUNT/ELEAVEMOUNT/ESYMLINK（-301/-302/-303）→ 03/99
- 注意：REQ_GETNODE 在 vfsif.h 中标注 "Should be removed"，`fsdriver_callvec` 无对应项（table.c 32 项）→ 01 说明

### 5.4 排除表（非本 stage 语义）

| 内容 | 归属 | 说明 |
|------|------|------|
| VFS 服务端（servers/vfs/） | 05-stage-vfs | 已独立 stage；本 stage 只消费 REQ 协议 |
| 路径多组件解析（VFS path lookup） | 05-stage-vfs/13-path-lookup | FS 侧只做单组件 lookup |
| 管道 VFS 侧（servers/vfs/pipe.c、cdev.c） | 05-stage-vfs/17-pipe | PFS 只提供节点/读写 |
| 块设备驱动（memory/ramdisk 等） | 16-stage-drivers | 本 stage 只经 minix-bdev 接口消费 |
| 终端/PTY 驱动 | 16-stage-drivers | ptyfs 只提供 /dev/pts 树 |
| devman/gpio 的 VTreeFS 用法 | 11-stage-devman / 16-stage-drivers | 框架语义在本 stage 18 定义，跨引用 |
| libbdev（bdev 客户端） | minix-bdev（库层） | 语义归 16-stage-drivers 或库文档，本 stage 引用接口 |
| libhgfs/libvboxfs（宿主协议） | 库层/驱动 | 本 stage 24 只覆盖 SFFS 桥接层 |
| 挂载命令面（mount/umount 命令） | 18-stage-commands | 本 stage 覆盖 FS 服务端 mount 回调 |

---

## 6. 实施顺序（自下而上，每批可独立验收）

1. **框架批**：01 → 02 → 03（libfsdriver）；04 → 05（libminixfs）——对应 `minix-fs` crate 骨架
2. **pfs 批**：06（最小完整 server 样例，验证框架可用）
3. **mfs 参考批**：07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17（按 §2 阶段序）
4. **虚拟树批**：18（VTreeFS）→ 19（procfs）→ 20（ptyfs）
5. **磁盘/宿主批**：21/22（ext2）→ 23（isofs）→ 24（vbfs/hgfs，可 defer）
6. **收尾**：00 总览定稿（吸收各批成果）+ 99 全局概念 + checklist 更新

---

## 7. Review 记录

### 7.1 深度 review（语义全覆盖 + 模块拆分合理性）

> 首轮深度 review 结论（2026-08-16）：
> - **结构修正 R1**：pfs 从"变体尾段"前移至 06（boot 第一个挂载 + 最小 server 样例），mfs 文档顺延为 07~17；VTreeFS 从 procfs 拆分独立成 18（框架语义模块 + devman 跨引用）；procfs→19、ptyfs→20、ext2→21/22、isofs→23、vbfs/hgfs→24。
> - **事实修正 R2**：call.c 为 31 个适配器（fsdriver_lookup 位于 lookup.c），callvec 分发表 32 项（REQ_GETNODE 无对应项，vfsif.h 标注 "Should be removed"）。
> - P0/P1/P2 全部闭环后本 plan 方可进入实施。

### 7.2 minix3 源码回归 review

> 逐文件 grep 实证（2026-08-16），见 §5 核对列；发现遗漏/错配时在此记录并回填 §5。

**首轮回归 review 事实修正（已回填 §5/§2）：**

| # | 修正项 | 原值 | 实证值（grep/wc） |
|---|--------|------|------------------|
| F1 | pfs 回调数 | 7 | 9（`rg -c "fdr_" fs/pfs/pfs.c` → 9） |
| F2 | mfs_table 回调数 | 29 | 31（`rg -c "\.fdr_" fs/mfs/table.c` → 31） |
| F3 | libvtreefs 文件数 | 14 个 .c | 10 个 .c + 4 个 .h |
| F4 | libsffs 文件数 | 17 个 .c | 15 个 .c + 6 个 .h（含 handle.c 显式列出） |
| F5 | mfs 文件构成 | 16 .c + 11 .h | 17 .c + 10 .h |
| F6 | procfs 行数 | ~1600 | 1703（`wc -l`） |
| F7 | isofs 行数 | ~1400 | 1509（`wc -l`） |
| F8 | ext2 拆分行数 | ~1900/~2000 | 1711/2994（`wc -l`） |
| F9 | proto.h | §5.2 遗漏 | 已补（函数原型全表 → 07） |

**自动化覆盖检查**：92 个 .c 文件全量映射，0 遗漏（脚本比对 plan §5.1 显式清单）。

### 7.3 自检清单

- [ ] 8 个 server + 4 个框架库全部映射（§5.1 无遗漏）
- [ ] vfsif.h 34 个 REQ 全部覆盖（01/02/99）
- [ ] 每个语义模块恰好一篇，无内容交叉（§3.3 边界表）
- [ ] ARCH 项全部标注三处一致（§4）
- [ ] 排除表明确（§5.4），无越界
- [ ] 文档数可实施（26 篇），每篇一个语义单元
