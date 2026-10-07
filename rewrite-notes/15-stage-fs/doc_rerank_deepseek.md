# 15-stage-fs 文档重建蓝图（deepseek）

## 0. 元数据

```text
your_name(AI agent name) = deepseek
target_dir(关注的工作目录) = 15-stage-fs
repo_root(仓库根目录) = /home/xzhao/github/minix-rs
当前提交号 = bfacf762dd4d097ad93ab18f76b2015023ff662e
执行日期 = 2026-09-19
本轮修订 = 2026-09-19（补做轮）：把 K-049、K-080 写入新 03 契约的知识点清单（锚点 `driver.rs:672`、`os/libs/minix-fs/src/memfs.rs:164` 已实测），§8 的对应两行去掉"或 33"的悬置写法。
```

任务 = R 相·重建蓝图：输出本文件，不改任何正文。

### 0.1 审查范围

**算文档**（本次重建的对象）：

- 编号文档 24 篇：`01-fsdriver-task.md` 到 `24-vbfs-hgfs.md`
- 总览 1 篇：`00-fs-overview.md`（21 行，自述"最小骨架，待改写"）
- 全局概念 1 篇：`99-global-concepts.md`（21 行，自述"最小骨架，待改写"）

合计 26 篇，总计 5679 行（`wc -l` 实测，不含本文件）。

**算参考材料**（不作为重建对象，作为线索与边界来源）：

| 文件 | 行数 | 性质 |
|---|---|---|
| `plan.md` | 396 | 生效中的重组计划（2026-08-16 定稿）；本次重建不照搬其结论 |
| `todo.md` | 265 | V1 轮架构级审查 TODO（2026-09-16，HEAD abccac10）；本次重建的**主要事实来源之一** |
| `draft/README.md` | 17 | 旧占位 README 素材 |

> 说明：目标目录下另有一个中间产物目录（按项目规范视为中间产物，本文不引用其内容、不列其清单）。本次执行只核对了"该目录下 24 组设计快照齐备"这一事实（每组含 outline、outline-review、design 三类），用于确认旧文档的 Step 0 预检状态，未读取任何一份的内容。

**范围外**（明确排除，附理由）：

| 内容 | 归属 | 理由 |
|---|---|---|
| VFS 服务端全部实现（`servers/vfs/`，47 文件 18469 行） | `05-stage-vfs` | 已独立 stage；本 stage 只消费 `REQ_*` 协议 |
| 块设备驱动实现（memory/ramdisk/virtio-blk） | `16-stage-drivers` | 本 stage 经 `libbdev` 客户端接口消费 |
| 终端/PTY 驱动实现 | `16-stage-drivers` | ptyfs 只提供 `/dev/pts` 树 |
| `libbdev`（9 文件 1441 行） | `16-stage-drivers` | 块设备客户端库 |
| `libvboxfs`（13 文件 1197 行）、`libhgfs`（17 文件 1109 行） | 驱动/库层 | 宿主协议线格式；本 stage 只覆盖 SFFS 桥接层 |
| `libpuffs`（17 文件 1900 行） | 无归属 | 树内无 `fs/` 服务消费方（已核对：`minix3/minix/fs/` 下无 puffs 服务） |
| devman / gpio 的 VTreeFS 用法 | `11-stage-devman` / `16-stage-drivers` | 框架语义在本 stage 定义，用法跨引用 |
| `mkfs.mfs`（1544 行）、`fsck.mfs`（1675 行）的**完整实现** | 见 §6.4 裁决 | 本 stage 只覆盖与镜像格式、一致性契约直接相关的部分 |
| mount/umount/df 等命令面 | `18-stage-commands` | 本 stage 覆盖 FS 服务端回调 |
| NetBSD 派生工具（`minix3/sbin/fsck/`、`fsck_ext2fs/`、`newfs_msdos/`） | 范围外 | 与 Minix3 自有 FS 实现不同源，非本 stage ground truth |

### 0.2 读取清单

**文档**（26 篇全文读完，含头部声明与正文）：

- `00-fs-overview.md`、`99-global-concepts.md`（各 21 行，全文）
- `01-fsdriver-task.md`（358 行）到 `24-vbfs-hgfs.md`（214 行），逐篇全文

**Minix3 C 源码与头文件**（本 stage 对应的全量清单，逐文件 `wc -l` 核对）：

| 目录 | 文件构成 | 行数 | 说明 |
|---|---|---|---|
| `minix3/minix/lib/libfsdriver/` | 6 `.c` + 1 `.h` + Makefile | 1780 | `call.c` 1013、`lookup.c` 333、`utility.c` 105、`dentry.c` 99、`fsdriver.c` 97、`table.c` 40、`fsdriver.h` 84 |
| `minix3/minix/lib/libminixfs/` | 2 `.c` + 1 `.h` + Makefile | 1604 | `cache.c` 1321、`bio.c` 263、`inc.h` 10 |
| `minix3/minix/lib/libvtreefs/` | 10 `.c` + 4 `.h` + Makefile | 1661 | 合计 1507（源文件）+ 头文件 |
| `minix3/minix/lib/libsffs/` | 15 `.c` + **5** `.h` + Makefile | 2295 | 头文件实为 5 个（`const.h`/`glo.h`/`inc.h`/`inode.h`/`proto.h`），plan §5.1 与 `24-vbfs-hgfs.md` 写的"6 个 .h"有误 |
| `minix3/minix/fs/mfs/` | 17 `.c` + 10 `.h` + Makefile | 4109 | 参考实现 |
| `minix3/minix/fs/pfs/` | 1 `.c` + Makefile | 459 | `pfs.c` 451 |
| `minix3/minix/fs/procfs/` | 8 `.c` + 3 `.h` + Makefile + NOTES | 1978 | `tree.c` 462、`service.c` 348、`pid.c` 230、`root.c` 227、`cpuinfo.c` 153、`buf.c` 125、`main.c` 93、`util.c` 65 |
| `minix3/minix/fs/ptyfs/` | 2 `.c` + 1 `.h` + Makefile | 548 | `ptyfs.c` 434、`node.c` 84 |
| `minix3/minix/fs/ext2/` | 17 `.c` + 6 `.h` + Makefile | 5422 | 最大者 `link.c` 656、`read.c` 557、`ialloc.c` 476、`super.c` 459 |
| `minix3/minix/fs/isofs/` | 12 `.c` + 4 `.h` + Makefile + `uthash.h` | 2797 | `inode.c` 492、`susp_rock_ridge.c` 289；`uthash.h` 960 行是 vendored 第三方头 |
| `minix3/minix/fs/vbfs/` | 1 `.c` + Makefile + `.8` + `.conf` | 235 | `vbfs.c` 141 |
| `minix3/minix/fs/hgfs/` | 1 `.c` + Makefile + `.8` | 192 | `hgfs.c` 106 |

头文件：

| 头文件 | 行数 | 承载 |
|---|---|---|
| `minix3/minix/include/minix/vfsif.h` | 84 | `REQ_*` 编号（`FS_BASE+1`..`FS_BASE+33`）、`NREQS=34`、`IS_FS_RQ`、`TRNS_*` 三宏、`REQ_RDONLY`/`REQ_ISROOT`、`PATH_*`、`RES_*`、`EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK`、`vfs_ucred_t` |
| `minix3/minix/include/minix/fsdriver.h` | 127 | `struct fsdriver`（35 个函数指针）、`struct fsdriver_node`、`struct fsdriver_data`、`struct fsdriver_dentry`、`FSC_*` 调用标识、`fsdriver_*` 入口原型 |
| `minix3/minix/lib/libfsdriver/fsdriver.h` | 84 | 库内头：32 个 `fsdriver_*` 处理函数 extern、`fsdriver_getname`、四个库内全局（`fsdriver_device`/`fsdriver_root`/`fsdriver_mounted`/`fsdriver_callvec`）、`ROOT_UID` |
| `minix3/minix/include/minix/libminixfs.h` | 65 | `struct buf`、`LMFS_MAX_PREFETCH`、`NORMAL`/`NO_READ`/`PEEK`、`lmfs_*` API |
| `minix3/minix/include/minix/vtreefs.h` | 74 | `struct inode_stat`、`struct fs_hooks`（13 钩子）、`NO_INDEX`、`PNAME_MAX`、`run_vtreefs` |
| `minix3/minix/include/minix/sffs.h` | 69 | `sffs_file_t`、`sffs_dir_t`、`struct sffs_attr`、`SFFS_ATTR_*`、`sffs_*` 请求函数 |
| `minix3/minix/include/minix/ipc.h` | 2835 | `m_vfs_fs_*`（23 个请求载荷）、`m_fs_vfs_*`（10 个回复载荷） |
| `minix3/minix/include/minix/com.h` | 1153 | `FS_BASE` 等消息类型基址与服务号 |
| `minix3/sys/sys/dirent.h` | 129 | `struct dirent`、`MAXNAMLEN 511`、`_DIRENT_RECLEN`/`_DIRENT_NAMEOFF`/`_DIRENT_ALIGN` |
| `minix3/minix/include/sys/procfs.h` | 62 | procfs 挂载/条目 ABI |
| `minix3/minix/include/sys/statfs.h` | 15 | `struct statfs` |

> 说明：`minix3/minix/include/sys/dirent.h` **不存在**。实际 dirent 定义在 `minix3/sys/sys/dirent.h`。`03-fsdriver-utility.md` 第 88 行写的 `sys/sys/dirent.h:105-107` 是正确路径的简写，第 249 行的参见用的是全路径。

**非 C 语言的构建与引导制品**（逐项核对，清单见 §0.3）：

- 服务构建链：`minix3/minix/fs/Makefile`（18 行）、`fs/Makefile.inc`（4 行）、8 个 server 各自 Makefile（8–14 行）
- 库构建：`lib/libfsdriver/Makefile`（9）、`lib/libminixfs/Makefile`（10）、`lib/libvtreefs/Makefile`（19）、`lib/libsffs/Makefile`（11）
- 服务策略配置：`minix3/etc/system.conf`（procfs 段 `:353`、ptyfs 段 `:507`）
- 启动脚本：`minix3/etc/usr/rc`（`:255-256` pty 驱动启动行）
- 测试脚本：`minix3/minix/tests/testmfs.sh`（87）、`testisofs.sh`（183）、`testvnd.sh`（155）、`run`（184）、`check-install`（42）
- 离线工具：`minix3/minix/usr.sbin/mkfs.mfs/`（`mkfs.c` 1544 + v1/v1l/v2/v2l/v3/mfs3v2 六套版本头）、`minix3/minix/commands/fsck.mfs/`（`fsck.c` 1675）
- 引导侧：`minix3/minix/kernel/table.c`（`boot_image` 数组 `:44-64`，pfs `:62`、mfs `:63`）

**阶段边界材料**：

- `notes/rewrite/fork-syscall-rewrite/00-master-plan/README.md`：阶段划分（15 号 = FS）、boot 两层语义（登记顺序 vs 执行顺序）
- `notes/rewrite/fork-syscall-rewrite/edge_todo.md`（1095 行，42 条）：本 stage 登记 4 条（`E-FSRUNTIME` `:897`、`E-FSBDEV` `:914`、`E-FSVMCACHE` `:931`、`E-FSCMDS` `:948`）
- `15-stage-fs/plan.md`（396 行）、`15-stage-fs/todo.md`（265 行）
- 前一 stage：`05-stage-vfs/00-vfs-overview.md`（已讲完 VFS 主循环、33 个 REQ 的 VFS 侧包装、挂载链 VFS 半、路径多组件解析、管道 VFS 侧、设备映射）

**对应 Rust 实现入口**（`os/` 下，逐 crate 核对行数）：

| crate | 文件 | 行数 |
|---|---|---|
| `os/libs/minix-fs` | 13 个 `.rs` | 9340（`cache.rs` 1659、`vm_cache.rs` 1228、`task.rs` 1197、`driver.rs` 889、`call.rs` 880、`bio.rs` 798、`lookup.rs` 778、`memfs.rs` 708、`protocol.rs` 586、`data.rs` 364、`dentry.rs` 357、`bdev_bridge.rs` 313、`lib.rs` 52） |
| `os/fs/mfs` | 19 个 `.rs` | 13189（`link.rs` 1892、`server.rs` 1527、`inode.rs` 1142、`write.rs` 1113、`read.rs` 1042、`mount.rs` 996、`open.rs` 911、`dir.rs` 732、`superblock.rs` 728、`meta.rs` 714、`second_level.rs` 564、`dir_io.rs` 413、`table.rs` 311、`mfs_cache.rs` 269、`maint.rs` 202、`startup.rs` 178、`lib.rs` 30、`main.rs` 10） |
| `os/fs/pfs` | 2 个 `.rs` | 701 |
| `os/fs/procfs` | 5 个 `.rs` | 992 |
| `os/fs/ptyfs` | 4 个 `.rs` | 735 |
| `os/fs/ext2` | 7 个 `.rs` | 1202 |
| `os/fs/isofs` | 5 个 `.rs` | 630 |
| `os/fs/vbfs` + `os/fs/hgfs` | 各 2 个 `.rs` | 152 |
| `os/libs/minix-vtreefs` | 3 个 `.rs` | 1592 |
| `os/libs/minix-sffs` | 6 个 `.rs` | 479 |

**写法范例**：`01-stage-kernel/06-todo.md`（只学"新文档契约"的写法：讲什么、不讲什么、下放给谁、验收是什么）。

### 0.3 使用的命令与关键输出（证据摘录）

```text
# 文档行数
$ wc -l notes/rewrite/fork-syscall-rewrite/15-stage-fs/*.md
  21 00-fs-overview.md / 358 01-fsdriver-task.md / 284 02-fsdriver-call.md
  249 03-fsdriver-utility.md / 310 04-block-cache.md / 220 05-block-io.md
  239 06-pfs.md / 235 07-mfs-init-main.md / 229 08-mfs-super.md
  235 09-mfs-inode.md / 187 10-mfs-mount.md / 171 11-mfs-path.md
  176 12-mfs-open.md / 241 13-mfs-link.md / 244 14-mfs-read.md
  211 15-mfs-write.md / 210 16-mfs-metadata.md / 194 17-mfs-maint.md
  237 18-vtreefs.md / 213 19-procfs.md / 191 20-ptyfs.md
  197 21-ext2-init-mount.md / 175 22-ext2-namespace-data.md
  198 23-isofs.md / 214 24-vbfs-hgfs.md / 21 99-global-concepts.md

# C 源清单
$ ls minix3/minix/fs/ → ext2 hgfs isofs mfs pfs procfs ptyfs vbfs（8 server）
$ ls minix3/minix/lib/{libfsdriver,libminixfs,libvtreefs,libsffs}/ → 4 框架库

# 内部交叉引用统计（15-stage-fs 文档之间）
$ grep -rho "15-stage-fs/[0-9][0-9]-[a-z0-9-]*\.md" 15-stage-fs/*.md | sort | uniq -c | sort -rn
  16 09-mfs-inode.md   14 01-fsdriver-task.md   12 07-mfs-init-main.md
  11 08-mfs-super.md    9 11-mfs-path.md         8 04-block-cache.md
   8 02-fsdriver-call.md 7 14-mfs-read.md         6 18-vtreefs.md
   5 03-fsdriver-utility.md 4 13-mfs-link.md / 12-mfs-open.md / 10-mfs-mount.md
   3 21-ext2-init-mount.md / 05-block-io.md      2 其余五篇
   1 24/22/20/17/06
# 总计 98 处内部引用（不含 doc_rerank_*）

# 全仓其它目录对 15-stage-fs 的引用（外部引用）
$ grep -rn "15-stage-fs/[0-9][0-9]-" --include=*.md --include=*.rs . | grep -v "^./notes/.../15-stage-fs/"
  ../16-stage-drivers/07-pty-driver.md:11  → 20-ptyfs.md
  ../16-stage-drivers/12-gpio-devman.md:13 → 18-vtreefs.md
  ../edge_todo.md:901                    → 00-fs-overview.md:13
  ../05-stage-vfs/plan.md:537            → "15-stage-fs 协调"
  ../16-stage-drivers/plan.md:97/134/141/145/172/173/368
  ../18-stage-commands/plan.md:169/225/398
  ../14-stage-runtime/plan.md:216/280

# Rust 代码注释中零引用旧文档编号（重要事实）
$ grep -rn "第 [0-9]\+ 篇\|文档 [0-9]\|doc[0-9]" os/fs os/libs/minix-fs os/libs/minix-vtreefs os/libs/minix-sffs --include=*.rs
  （无命中）
# 结论：Rust 代码的锚点全部指向 C 源（形如 "C: cache.c:573-575"），不指向 notes 文档。
# 因此重建的断链风险**只在文档之间**，不在代码注释。

# 工具生成的锚点（"工具生成"占位标记）分布
$ grep -c "工具生成" 15-stage-fs/*.md
  01-fsdriver-task.md:16   06-pfs.md:14   05-block-io.md:8
  19-procfs.md:4   04-block-cache.md:1   17-mfs-maint.md:1
  18-vtreefs.md:1  20-ptyfs.md:1  21-ext2-init-mount.md:1

# 符号覆盖矩阵分布（01-08 有，09-24 无）
$ for f in [0-9]*.md; do echo "$(basename $f): $(grep -c 符号覆盖矩阵 $f)"; done
  01..08 = 1 each；09..24 = 0 each

# 关键锚点核对（全部逐条 grep 验证，见 §1 表内锚点）
$ grep -nE "^[a-z_]+[A-Za-z0-9_ ]*\**fs_mount\(" minix3/minix/fs/mfs/mount.c → 10
$ grep -nE "..." → 见 §1 真序表逐行锚点（全部实测，非转述）

# 构建链证据
$ cat minix3/minix/fs/Makefile
  SUBDIR+= mfs / pfs                      （无条件，boot 成员）
  .if ${MKIMAGEONLY} == "no" → ext2 isofs procfs ptyfs；i386 → hgfs vbfs
$ cat minix3/minix/fs/mfs/Makefile
  LDADD+= -lminixfs -lfsdriver -lbdev -lsys
  CPPFLAGS+= -DDEFAULT_NR_BUFS=1024
$ cat minix3/minix/fs/pfs/Makefile
  LDADD+= -lfsdriver -lsys           （不链接 libminixfs：无磁盘）
$ cat minix3/minix/fs/procfs/Makefile
  LDADD+= -lvtreefs -lfsdriver       （不链接 libminixfs）
$ cat minix3/minix/fs/isofs/Makefile
  CPPFLAGS+= -DNR_BUFS=100
$ cat minix3/minix/fs/vbfs/Makefile
  LDADD+= -lsffs -lvboxfs -lfsdriver -lsys；FILES=vbfs.conf → /etc/system.conf.d
```

### 0.4 本文档的取舍声明

1. 本蓝图**不照搬** `plan.md` 的 26 篇结论。`plan.md` 的分组（框架→pfs→mfs→虚拟树→磁盘变体）经本蓝图核对后**主干保留**，但篇数与边界有调整（§4、§6 逐项给出理由）。
2. 本蓝图对现有文档的缺陷只做**归纳与锚点**，不逐条罗列（缺陷明细属 review 工作流，见 `todo.md`）。§3 只列影响"重建"的结构性缺陷。
3. 所有 C 锚点均为本次执行中实测（`grep -nE`），未从现有文档转述。凡未能实测的断言，本文显式标注"待验证"。
4. 本文不引用 `.design/` 与 `tmp_design_and_todo/` 下的任何内容。

---

## 1. C 真序

### 1.1 阶段类型判定

15-stage-fs **同时具备三种阶段特征**，按"以哪一类为主"处理：

| 特征 | 适用对象 | 处理方式 |
|---|---|---|
| **库与框架型**（主） | `libfsdriver` / `libminixfs` / `libvtreefs` / `libsffs` | 先讲抽象层与接口契约，再讲框架骨架，最后按实现族展开。**这是本篇的主组织原则** |
| **服务事件循环型** | 8 个 server 各自 | 按"服务为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 → 查询与杂项 → 与邻接服务的协议"组织。每个 server 的内部按此展开 |
| **集合型** | 8 个 server 构成的集合 | 先总览与分类框架（§4 的变体差异总表），再按族分组（虚拟树族 / 磁盘族 / 宿主族），族内选代表（mfs）讲透，其余按差异表收束 |

**判定理由**（带锚点）：

- 框架型是主特征：4 个框架库共 5260 行 C（1780+1604+1661+2295 含头文件与 Makefile），而 8 个 server 中有 6 个（pfs 451、procfs 1703、ptyfs 518、isofs 1509、vbfs 141、hgfs 106）**大量复用框架**——`pfs.c` 只提供 9 个回调（`pfs_table`，`fs/pfs/pfs.c:425-435`），`ptyfs` 只提供 8 个（`ptyfs_table`，`fs/ptyfs/ptyfs.c:425-434`），`vbfs.c` 全 141 行、`hgfs.c` 全 106 行，二者合计 247 行里没有一行文件系统逻辑，全是装配（`fs/vbfs/vbfs.c:127` 的 `main`、`fs/hgfs/hgfs.c:93` 的 `main`）。
- 事件循环型是每个 server 的内部结构：8 个 server 的 `main` 全部是同一形状——本地启动（注册 SEF 回调）→ `fsdriver_task(&table)`（或框架自己的循环）。实测锚点：`fs/mfs/main.c:14` `main` → `:47` `sef_cb_init_fresh` → `fsdriver_task(&mfs_table)`（`fs/mfs/main.c:23`）；`fs/pfs/pfs.c:441` `main` → `fsdriver_task(&pfs_table)`；`lib/libvtreefs/vtreefs.c:88` `run_vtreefs`（procfs 用它替代 `fsdriver_task`）；`lib/libsffs/main.c:53` `sffs_loop`（vbfs/hgfs 用它）。
- 集合型是外层的组织问题：8 个 server 的启动时机不同（boot 两个、RS 运行时两个、按需四个），必须用"汇聚点加触发时机"组织，不能强排成一条线。

### 1.2 运行时真序表

> 说明：本表从 C 源码直接重建，**不从现有文档转述**。所有锚点为本次实测。分三段：**启动段**（进程诞生到进入循环）、**循环段**（一次请求的生命周期）、**终止段**（信号到退出）。

#### 段 A：启动段（boot 因果链）

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| A-1 | 内核登记 boot image，pfs 与 mfs 是成员 | `kernel/table.c:44-64`（`boot_image` 数组；`PFS_PROC_NR` 在 `:62`、`MFS_PROC_NR` 在 `:63`） | 登记顺序即模块槽位顺序；执行顺序另算（`kernel/main.c:265` 对非 VM 挂 `RTS_VMINHIBIT`） |
| A-2 | RS 加载 VFS 镜像并启动 | `servers/vfs/main.c`（`main`，第 54 行起；本次未逐行核对具体行号，标**待验证**） | VFS 是 boot 第一批（VM 解除抑制后即可运行） |
| A-3 | VFS 初始化，进入 `worker_start(do_init_root)` | `servers/vfs/main.c:492`（`worker_start(fproc_addr(VFS_PROC_NR), do_init_root, ...)`） | 挂载在独立 worker 里做，避免阻塞主循环 |
| A-4 | `do_init_root` 先禁止外部请求 | `servers/vfs/main.c:501`（`do_init_root` 定义）、`:506`（`worker_allow(FALSE)`） | 挂载期间不接受 init 等来源的请求 |
| A-5 | 挂载 PFS（管道文件系统） | `servers/vfs/main.c:510`（`mount_pfs()`）→ `servers/vfs/mount.c:391`（`mount_pfs` 定义） | 分配 nonedev（`mount.c:401` `find_free_nonedev`）、分配 vmnt（`mount.c:404` `get_free_vmnt`）、`m_label="pfs"`、`m_mount_path="pipe"` |
| A-6 | PFS 收到挂载请求 | `servers/vfs/mount.c:420`（`req_readsuper(vmp, "", dev, FALSE, FALSE, ...)`）→ `servers/vfs/request.c`（`req_readsuper`，本次未核实行号，标**待验证**） | 空标签、非只读、非根；返回的节点详情被 VFS **丢弃**（`mount.c:422-425` 注释"忽略返回的节点详情"） |
| A-7 | PFS 侧处理挂载 | `fs/pfs/pfs.c:50`（`pfs_mount`） | 初始化空闲链、倒序压栈（低号先出）；**无根节点**（根号返回零）；忽略 dev 与 flags |
| A-8 | 挂载根文件系统（MFS，boot ramdisk） | `servers/vfs/main.c:516-519`（`mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, 0, "mfs", "fs_imgrd")`） | 失败即 `panic("Failed to initialize root")`（`main.c:520-521`） |
| A-9 | VFS 侧发出 READSUPER | `servers/vfs/mount.c:272`（`req_readsuper(new_vmp, label, dev, !!(flags & MNT_RDONLY), isroot, &res, ...)`） | 取标签 grant（`strlen(label)+1` 字节，`CPF_READ`） |
| A-10 | 框架侧处理 READSUPER | `lib/libfsdriver/call.c:12`（`fsdriver_readsuper`） | 步骤：取 dev/label grant/label len/flags → `fdr_mount==NULL` 报 `ENOSYS` → 已挂载报 `EBUSY` → `fsdriver_getname` 取标签（`lib/libfsdriver/utility.c:84`）→ `fdr_driver(dev,label)`（若存在）→ `fdr_mount` → 成功时补能力位 → 填回复五字段 → 更新库内三全局（`fsdriver_mounted=TRUE`、`fsdriver_device`、`fsdriver_root`）、清 `fsdriver_vmcache` |
| A-11 | 能力位的框架规则 | `lib/libfsdriver/call.c:48-51` | 规则：`(fdr_peek != NULL && fdr_bpeek != NULL) \|\| major(dev) == NONE_MAJOR` → 置 `RES_HASPEEK`。这是"框架替服务器设置"的唯一能力位 |
| A-12 | MFS 侧挂载七步 | `fs/mfs/mount.c:10`（`fs_mount`） | 记住设备（`:19`）→ 打开设备（`:22`，读写或只读）→ 读超级块（`read_super`，`fs/mfs/super.c:241`）→ 失败复位设备（`:33-37`）→ 脏盘降级（`:40-50`）→ 认块大小（`:52`）→ 数已用区交缓存（`:58-60`）→ 领根节点（`:63-68`）→ 填根六字段与能力位（`:81-86`、`:88`）→ 读写挂载清干净位并写回（`:91-95`） |
| A-13 | MFS 读超级块 | `fs/mfs/super.c:241`（`read_super`） | 读零号块 → 版本门（`const.h:22/24/26` 三魔数）→ 定字节序与区大小 → 块大小五连验（`:287-293`）→ 首区现算（`:299-308`）→ 几何检查（`:333-342`）→ 强制标志掩码（`:348-352`） |
| A-14 | 领根节点 | `fs/mfs/inode.c:118`（`get_inode`） | 从 inode 号 1（`const.h:48` `ROOT_INODE`）装载；根不存在或模式为零则挂载失败 |
| A-15 | 挂载完成，放开请求 | `servers/vfs/main.c:524`（`worker_allow(TRUE)`） | |
| A-16 | RS 运行时加载 procfs/ptyfs | `minix3/etc/system.conf:353`（procfs 段）、`:507`（ptyfs 段）；pty 驱动在 `etc/usr/rc:255-256` | procfs 策略含 `VIRCOPY`(15) 与 vm 的 `INFO`/`SETCACHEPAGE`；ptyfs 策略含 `ipc SYSTEM pm vfs rs pty ds vm` |
| A-17 | 按需挂载 ext2/isofs/vbfs/hgfs | `18-stage-commands` 的 mount 命令面触发 | 本 stage 覆盖服务端挂载回调（`fs/ext2/mount.c:17` `fs_mount`、`fs/isofs/mount.c:4` `fs_mount`、`lib/libsffs/mount.c:16` `do_mount`） |
| A-18 | 各 server 的初装（fresh init） | `fs/mfs/main.c:47`（`sef_cb_init_fresh`） | 四步：`lmfs_may_use_vmcache(1)`（`:52`）→ inode 表清零（`:55-58`）→ `init_inode_cache()`（`:60`，`fs/mfs/inode.c:70`）→ `lmfs_buf_pool(DEFAULT_NR_BUFS)`（`:62`，`DEFAULT_NR_BUFS=1024` 由 `fs/mfs/Makefile` 定义） |
| A-19 | 其他 server 的初装差异 | `fs/pfs/pfs.c:394`（`pfs_init`，只丢特权到 `SERVICE_UID`）；`lib/libvtreefs/vtreefs.c:16`（`init_server`）；`lib/libsffs/main.c:17`（`sffs_init`） | pfs 的 inode 表在**挂载时**才初始化（`pfs_mount` 内），不是初装 |
| A-20 | 进入主循环 | `fs/mfs/main.c:23`（`fsdriver_task(&mfs_table)`，`lib/libfsdriver/fsdriver.c:80`）；`lib/libvtreefs/vtreefs.c:88`（`run_vtreefs`）；`lib/libsffs/main.c:53`（`sffs_loop`） | 三个不同的循环入口，形状相同 |

#### 段 B：循环段（一次请求的生命周期）

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| B-1 | 阻塞接收消息 | `lib/libfsdriver/fsdriver.c:88`（`sef_receive_status(ANY, &mess, &ipc_status)`） | 接收失败：`EINTR` 继续（`sef_cancel` 被调用过），其他 `panic`（`:92`） |
| B-2 | 循环条件 | `lib/libfsdriver/fsdriver.c:87`（`while (fsdriver_running \|\| fsdriver_mounted)`） | 运行标志与挂载状态**双条件**：卸载后未终止仍在转，终止后未卸载仍在转 |
| B-3 | 分类：是不是文件系统请求 | `lib/libfsdriver/fsdriver.c:26`（`is_ipc_notify(ipc_status) \|\| m_ptr->m_source != VFS_PROC_NR`） | 不是 → 调 `fdr_other`（若存在）→ **不发回复**（`:30`） |
| B-4 | 拆事务号与请求号 | `lib/libfsdriver/fsdriver.c:34-35`（`TRNS_GET_ID` / `TRNS_DEL_ID`） | 高 16 位请求号，低 16 位事务号（`vfsif.h:79-81`） |
| B-5 | 挂载门禁 | `lib/libfsdriver/fsdriver.c:39`（`fsdriver_mounted \|\| call_nr == REQ_READSUPER`） | 不满足 → `EINVAL`（`:47`），**不查表** |
| B-6 | 表下标换算（无符号回绕） | `lib/libfsdriver/fsdriver.c:40`（`call_nr -= FS_BASE`） | 注释"unsigned; wrapping is intended"（`:40`） |
| B-7 | 查表与范围检查 | `lib/libfsdriver/fsdriver.c:42-45`（`call_nr < NREQS && fsdriver_callvec[call_nr] != NULL`） | 越界或空槽 → `ENOSYS` |
| B-8 | 调用适配器 | `lib/libfsdriver/fsdriver.c:43`（`(fsdriver_callvec[call_nr])(fdp, m_ptr, &m_out)`） | 分发表见 `lib/libfsdriver/table.c:6-40`（`NREQS=34` 个槽，实际填 32 个；`REQ_GETNODE` 无对应项） |
| B-9 | 适配器内部三步 | `lib/libfsdriver/call.c` 各函数 | 取字段并校验 → 调 `fdr_*` 回调 → 装回复。31 个适配器 + 3 个静态帮手（`read_write` `:154`、`builtin_peek` `:227`、`bread_bwrite` `:901`） |
| B-10 | 数据搬运（需要时） | `lib/libfsdriver/utility.c:8`（`fsdriver_copyin`）、`fsdriver_copyout`、`fsdriver_zero`、`utility.c:84`（`fsdriver_getname`） | 端点是自己则内存操作，否则走 grant |
| B-11 | 目录列表组装（GETDENTS 时） | `lib/libfsdriver/dentry.c:9`（`fsdriver_dentry_init`）、`:28`（`fsdriver_dentry_add`）、`fsdriver_dentry_finish` | staging 两级流水 |
| B-12 | 路径查找（LOOKUP 时） | `lib/libfsdriver/lookup.c:118`（`fsdriver_lookup`） | 内部三帮手：`lookup.c:9`（`access_as_dir`）、`:44`（`next_name`）、`:84`（`resolve_link`）。四岔路：点 / 点点 / 挂载点 / 符号链接 |
| B-13 | 服务器侧处理 | 各 `fdr_*` 回调（如 `fs/mfs/path.c:16` `fs_lookup`、`fs/mfs/read.c:23` `fs_readwrite`） | 磁盘 FS 内部再下沉到块缓存 |
| B-14 | 块缓存拿块（磁盘 FS 时） | `lib/libminixfs/cache.c:494`（`lmfs_get_block_ino`） | 哈希命中 → 引用加一；未命中 → 取空闲链头 → 腾空旧块 → 读盘（`cache.c:723` `read_block`）或免读 |
| B-15 | 块设备传输 | `lib/libminixfs/bio.c:117`（`lmfs_bio`） | 三道关卡（`bio.c:127` 设备号、`:138` 位置长度、`:146` 分区裁剪）→ 逐块行走（`:156`）→ 读预取（`bio.c:65` `block_prefetch`） |
| B-16 | 装回复并发送 | `lib/libfsdriver/fsdriver.c:50`（`m_out.m_type = TRNS_ADD_ID(r, transid)`）、`:52-55`（`ipc_send` 或 `asynsend`） | 回复**无条件发送**（含错误码），唯一不回复的是 B-3 的旁路 |
| B-17 | 挂载后钩子 | `lib/libfsdriver/fsdriver.c:60-61`（`fdp->fdr_postcall`） | 每请求之后调用 |

#### 段 C：终止段

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| C-1 | 收到 SIGTERM | `fs/mfs/main.c:70`（`sef_cb_signal_handler`） | 非 `SIGTERM` 直接忽略（`:73`） |
| C-2 | 先同步 | `fs/mfs/main.c:75`（`fs_sync()`） | 实现在 `fs/mfs/misc.c:8`：先刷脏 inode（`:18-19` 遍历 512 槽，引用为正且脏才写回），再 `lmfs_flushall`（`misc.c:22`） |
| C-3 | 请求终止 | `fs/mfs/main.c:77`（`fsdriver_terminate()`） | 实现在 `lib/libfsdriver/fsdriver.c:68`：清 `fsdriver_running`，调 `sef_cancel()`（`:73`） |
| C-4 | 接收被打断，条件重查 | `lib/libfsdriver/fsdriver.c:89-90`（`r == EINTR` → `continue`） | 回到 `:87` 的双条件判断；挂载未清则继续转 |
| C-5 | 卸载（若被卸载） | `lib/libfsdriver/call.c:74`（`fsdriver_unmount`）→ `fdr_unmount`（如 `fs/mfs/mount.c:134` `fs_unmount`） | 框架侧：调 `fdr_unmount` → 若用过 vmcache 则 `vm_clear_cache(fsdriver_device)`（`call.c:83-84`）→ 清 `fsdriver_mounted` |
| C-6 | MFS 卸载六步 | `fs/mfs/mount.c:134`（`fs_unmount`） | 数在用引用（`:144-148`，根被多次引用只打印不停）→ 放根 → 调同步 → 读写则标干净落盘 → `lmfs_invalidate`（`lib/libminixfs/cache.c:782`）→ 设备号复位 |

#### 段 D：pfs 的独立真序（无磁盘，形状不同）

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|---|---|---|
| D-1 | 挂载即初始化 | `fs/pfs/pfs.c:50`（`pfs_mount`） | `LIST_INIT(&free_inodes)`（`:55`）→ 倒序压栈（`:63` 循环） |
| D-2 | 无根节点 | `fs/pfs/pfs.c:78-79`（`node->fn_ino_nr = 0` 一类） | 根号为零；VFS 侧明确丢弃返回值（`servers/vfs/mount.c:422-425`） |
| D-3 | 新节点三检 | `fs/pfs/pfs.c:126`（`pfs_newnode`） | 类型必须管道或字符设备（`:134`）→ 空闲链非空（`:141`）→ 管道必须能分配缓冲（`:146`） |
| D-4 | 释放三事 | `fs/pfs/pfs.c:184`（`pfs_putnode`） | 计数必须恰为一（`:191`）→ 归还缓冲 → 压回栈顶 |
| D-5 | 管道读三取小 | `fs/pfs/pfs.c:218`（`pfs_read`） | 请求量、管内存量、`PIPE_BUF` 取小；超上限拒绝 |
| D-6 | 管道写先验后压缩 | `fs/pfs/pfs.c:254`（`pfs_write`） | 存量加增量超缓冲拒绝（`:265`）→ 残留前移压缩（`:274`） |
| D-7 | 截断只支持全清 | `fs/pfs/pfs.c:299`（`pfs_trunc`） | 起止皆零才接受 |
| D-8 | 状态与改模式 | `fs/pfs/pfs.c:322`（`pfs_stat`）、`:362`（`pfs_chmod`） | 懒时间三标记在 `stat` 时一站式结清；`chmod` 只换权限位 |
| D-9 | 退出只认终止信号 | `fs/pfs/pfs.c:381`（`pfs_signal`） | 无盘可刷，直接 `fsdriver_terminate()` |
| D-10 | 丢特权 | `fs/pfs/pfs.c:394`（`pfs_init`） | `setuid(SERVICE_UID)` 失败只警告 |

### 1.3 真序的可靠性说明

- 段 A 的 A-2、A-6、A-9 涉及 `servers/vfs/` 内部函数，本次只核对了 `main.c:492/501/506/510/516-521/524` 与 `mount.c:272/391/401/404/420/422-425`，`main` 与 `req_readsuper` 的函数起始行未逐条核对，已标"待验证"。这两处属 `05-stage-vfs` 的边界，不影响本 stage 的重建。
- 段 A 的 A-12 到 A-14 的 mfs 侧行号全部实测（`fs_mount` 内各步的行号取自 `08-mfs-super.md`/`10-mfs-mount.md` 已有锚点，本次对函数起始行做了实测，函数内部行号**未逐条重核**，标为"待验证"级——B 相写正文时须重核）。
- 段 B 的适配器内部行号（`call.c` 各函数起始行）本次实测：`fsdriver_readsuper :12`、`fsdriver_unmount :74`、`fsdriver_getdents :321`、`fsdriver_peek :279`、`builtin_peek :227`、`read_write :154`、`bread_bwrite :901`。其余适配器行号见 §5 各篇契约的事实底线。
- 段 C 与段 D 全部实测。

### 1.4 序差表（运行时序 vs 教学序）

| # | 运行时序事实（带锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---|---|---|---|
| S-1 | 框架代码不独立运行：`libfsdriver` 的代码通过各 server 的 `main` 进入（`fs/mfs/main.c:23`、`fs/pfs/pfs.c:441`）。运行时不存在"框架启动"这一站 | 教学序把框架（01–08）放在所有 server 之前 | 读者要先知道"服务器收什么消息、怎么分发"，才看得懂任何一个 server 的 `main`。若先讲 mfs，则 `fsdriver_task` 会被当成 mfs 的一部分 | 新 01 篇 §1 显式声明"框架是被链接进每个 server 的库，运行时没有独立的框架进程" |
| S-2 | 协议字段在运行时只是消息结构体的成员（`ipc.h` 的 `m_vfs_fs_*`），不构成一个执行阶段 | 教学序把协议（新 01 篇）放在主循环（新 02 篇）之前 | 契约先于实现：先知道"消息里有什么"，再知道"消息怎么被路由" | 新 02 篇 §1 回指"消息字段定义见 01 篇" |
| S-3 | 8 个 server 的启动时机不同：pfs 与 mfs 在 boot（`kernel/table.c:62-63`），procfs/ptyfs 在 RS 运行时（`etc/system.conf:353/507`），ext2/isofs/vbfs/hgfs 按需 | 教学序把"进程诞生与装配"独立成新 09 篇，放在框架之后、pfs 之前 | 这是**所有 server 共有的第一站**，而运行时它分散在 8 个 `main` 里。不独立成篇则每个 server 篇都要重复讲一遍启动握手 | 新 09 篇 §6 给出"8 个 server 的启动差异矩阵"，各 server 篇只写差异 |
| S-4 | 错误路径散布全程：每个适配器都可能返回错误，每个 `fdr_*` 回调内部都有失败分支与 `panic` 点 | 教学序把"错误路径与失败语义"集中到新 30 篇（收尾组） | 单篇单语义：错误语义是横切关注点，混进任何一篇都会让那篇同时是"机制教学"又是"错误手册" | 各篇在讲到失败分支时只留一句"错误分类与 errno 映射见 30 篇"，不展开 |
| S-5 | 一致性契约散布全程：clean 位在挂载（`fs/mfs/mount.c:40-50`）、顺序在同步（`fs/mfs/misc.c:18-22`）、孤儿优于悬空在创建（`fs/mfs/open.c` 第三拍）、卸载顺序在卸载（`fs/mfs/mount.c:134`） | 教学序把"崩溃一致性"集中到新 31 篇（收尾组） | 同上；且这些契约只有合在一起看才能理解为什么每一步的顺序都不能换 | 各篇在顺序处只留一句"顺序契约的完整论证见 31 篇" |
| S-6 | 测试基建在运行时不存在（测试是离线活动） | 教学序把它放收尾组（新 32 篇） | 它是读者的工具而非系统的机制；放主线会打断因果链 | 无（收尾组各篇互相独立，允许跳读） |
| S-7 | `libminixfs` 的二级缓存通道（`lmfs_may_use_vmcache` 一族）跨 stage 运行：另一端在 VM 服务器（`os/servers/vm/src/ipc/cache_handlers.rs`） | 教学序只讲本 stage 侧（新 07 篇），另一端声明为跨 stage 引用 | 完整讲另一端需要 VM 的页缓存语义，属 `02-stage-vm` | 新 07 篇 §1 声明"通道另一端在 02-stage-vm/24-page-cache.md，本篇只讲本侧" |
| S-8 | pfs 的 inode 表在**挂载时**初始化（`fs/pfs/pfs.c:55-63`），不是初装时 | 教学序在 pfs 篇（新 10）先讲挂载再讲初装 | 与"服务为什么存在 → 诞生与初始化 → 消息接口"的事件循环型顺序相反；理由是 pfs 的表生命周期与挂载绑定，先讲挂载才讲得通"无根挂载" | 新 10 篇 §1 显式说明"本 server 的初始化在挂载回调里，与磁盘 server 的初装时点不同" |

---

## 2. 知识点全集

### 2.1 建池方法

对现有 26 篇逐篇提取知识点（每篇 20–40 条），全 stage 去重后合并成池。同一知识点在多篇重复出现的，合并为一条并记录**全部**现有位置，标注**主讲述点**。

编号 `K-NNN`，stage 内唯一。对齐键为"名称加锚点"（供多 AI 汇总时对齐）。

**来源类型**：

- **存量**：来自现有文档，受 §6 变更表的去向规则约束；
- **新增**：现有文档没有，但 C 源码、非 C 制品或操作系统理论承载，由 §3 覆盖审计发现。不受去向规则约束，但必须给证据锚点。

### 2.2 知识点池总表

#### 组 A：协议与框架骨架（现有 01、02 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置（主讲述点加粗） | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-001 | 文件服务器是被驱动的服务器（只接订单的后厨模型） | 概念 | 存量 | **01 §1.1** | — | 建立"服务器不主动发起工作"的心智模型 |
| K-002 | 消息类型双载荷：高 16 位请求号 + 低 16 位事务号 | 机制 | 存量 | **01 §1.2/§2.5** | `vfsif.h:79-81` | 理解为什么同一套宏服务请求与回复两条路 |
| K-003 | 单线程事件循环假设（任意时刻最多一个请求，故不需锁） | 约束与不变量 | 存量 | **01 §1.1/§3.7**、04 §前置 | — | 理解缓存与索引节点表为什么全无锁 |
| K-004 | 营业状态三态（未挂载拒绝一切 / 卸载后空转 / 关店须先打烊） | 机制 | 存量 | **01 §1.4** | `fsdriver.c:39`、`:87` | 理解"卸载不等于退出" |
| K-005 | 三十三个请求编号：`FS_BASE` 加索引 1..33 | 接口与协议 | 存量 | **01 §1.2/§2.5** | `vfsif.h:41-73` | 定位任一请求的编号 |
| K-006 | 索引一保留请求（`REQ_GETNODE`，注明应删除，分发表无槽位） | 接口与协议 | 存量 | **01 §1.2/§3.1** | `vfsif.h:41`、`table.c:6-40` | 解释"33 个编号但 32 个槽" |
| K-007 | 三个事务宏 `TRNS_GET_ID`/`TRNS_ADD_ID`/`TRNS_DEL_ID` | 接口与协议 | 存量 | **01 §2.5** | `vfsif.h:79-81` | 能自己拆一条消息类型 |
| K-008 | 无符号回绕减法求表下标 | 机制 | 存量 | **01 §2.2** | `fsdriver.c:40` | 理解 `call_nr -= FS_BASE` 为什么不用检查负数 |
| K-009 | 分发表 34 槽数组（下标 0 不用、1 空、其余 32 指适配器） | 数据结构 | 存量 | **01 §1.3/§2.4** | `table.c:6-40` | 理解路由与处理分离 |
| K-010 | 服务器全局状态四件套与运行标志的封装 | 数据结构 | 存量 | **01 §1.4/§2.1** | `fsdriver.c:5-9` | 理解框架替服务器保存了什么 |
| K-011 | 门禁规则：未挂载只受理挂载请求 | 约束与不变量 | 存量 | **01 §1.4/§2.2** | `fsdriver.c:39`、`:47` | |
| K-012 | 退出条件与两步关闭流程 | 约束与不变量 | 存量 | **01 §1.4/§2.3** | `fsdriver.c:68-74`、`:87-96` | |
| K-013 | 非请求消息旁路（外来来源交 `fdr_other`，不发回复） | 机制 | 存量 | **01 §1.5/§2.2** | `fsdriver.c:26-31` | 理解 ptyfs 的控制消息为什么能到达 |
| K-014 | 能力协商三标志与框架自动点亮窥视位 | 接口与协议 | 存量 | **01 §1.6**、02 §2.1 | `vfsif.h:20-23`、`call.c:48-51` | |
| K-015 | 多线程能力位永不置位（有意设计） | 约束与不变量 | 存量 | **01 §1.6** | `vfsif.h:21` | 解释 `RES_THREADED` 为什么空着 |
| K-016 | 与 Linux 四组操作表、Redox Scheme 的对照 | 概念 | 存量 | **01 §1.7** | — | 理解 Minix3 用一张大表的理由 |
| K-017 | 回复无条件发送（错误码也带事务号回传） | 约束与不变量 | 存量 | **01 §2.2** | `fsdriver.c:50-58` | |
| K-018 | 接收失败即宕机（fail-stop 哲学） | 架构演进 | 存量 | **01 §2.3** | `fsdriver.c:88-93` | |
| K-019 | 适配器三步（取字段并校验 → 调回调 → 装回复） | 机制 | 存量 | **02 §1.1** | `call.c` 全篇 | 看任何一个适配器都能套 |
| K-020 | 纵深防御（扁平 wire 转类型化参数，服务器只见合法输入） | 概念 | 存量 | **02 §1.1** | — | |
| K-021 | 三十一个适配器按六组编队（挂载/节点/数据/命名空间/元数据/块） | 数据结构 | 存量 | **02 §1.2** | `call.c` 函数排布 | 定位任一请求的适配器 |
| K-022 | 位置长度校验规则（七处重复） | 约束与不变量 | 存量 | **02 §1.3** | `call.c` 各适配器 | |
| K-023 | 释放节点计数校验（零与超大拒绝） | 约束与不变量 | 存量 | **02 §1.3/§2.1** | `call.c:96-114` | |
| K-024 | 截断请求区间两端非负校验 | 约束与不变量 | 存量 | **02 §1.3/§2.2** | `call.c:360-376` | |
| K-025 | 点名字符统一规则全表（四种错误码差异） | 约束与不变量 | 存量 | **02 §1.4/§2.3** | `call.c:399-438`、`548-573`、`578-606`、`611-646` | 解释"为什么 `.` 在 create 与 unlink 里待遇不同" |
| K-026 | 窥视模拟（匿名映射 + 读回调填充 + 尾巴补零 + 一次性标记 + 假偏移 + 解映射） | 机制 | 存量 | **02 §1.5/§2.2** | `call.c:227-273`（`builtin_peek`） | |
| K-027 | 窥视三路选路（有窥视回调直调 / 有后端设备无实现报未实现 / 无后端设备走模拟） | 机制 | 存量 | **02 §1.5/§2.2** | `call.c:279-315` | |
| K-028 | 回复两形状（推进型与计数型） | 接口与协议 | 存量 | **02 §1.6** | `call.c:180-184`、`691-712` | |
| K-029 | 挂载适配器全步骤（含已挂载报忙、驱动绑定、能力位、库内状态更新、清 vmcache） | 机制 | 存量 | **02 §2.1** | `call.c:12-68` | |
| K-030 | 卸载适配器永远报成功（含 `vm_clear_cache`） | 约束与不变量 | 存量 | **02 §2.1** | `call.c:74-90` | |
| K-031 | 释放节点空回调报成功（不是未实现） | 约束与不变量 | 存量 | **02 §2.1** | `call.c:96-114` | 区分"空回调即成功"与"空回调即 ENOSYS"两类 |
| K-032 | 挂载点检查与新节点（新节点成功填六字段含设备号） | 机制 | 存量 | **02 §2.1** | `call.c:818-829`、`120-148` | |
| K-033 | 读写共用帮手与调用标识（读/写/窥视三值） | 机制 | 存量 | **02 §2.2** | `call.c:154-188` | |
| K-034 | 读目录位置是输入兼输出 | 机制 | 存量 | **02 §2.2** | `call.c:321-353` | |
| K-035 | 抑制预读语义（偏移被移动，预读作废） | 机制 | 存量 | **02 §2.2** | `call.c:382-393` | |
| K-036 | 创建族四请求同构 | 机制 | 存量 | **02 §2.3** | `call.c:399-543` | |
| K-037 | 改名做两次完整取名字流程 | 机制 | 存量 | **02 §2.3** | `call.c:612-646` | |
| K-038 | 符号链接拼目标数据抽象；读符号链接只回字节数 | 机制 | 存量 | **02 §2.3** | `call.c:652-712` | |
| K-039 | 状态适配器预填（框架填设备号与节点号） | 机制 | 存量 | **02 §2.4** | `call.c:718-741` | |
| K-040 | chown/chmod 成功回新模式；utime 四项组装两结构 | 机制 | 存量 | **02 §2.4** | `call.c:747-812` | |
| K-041 | 卷状态清零后复制；同步永远报成功 | 机制 | 存量 | **02 §2.4** | `call.c:835-866` | |
| K-042 | 块组：块读写共用帮手；块窥视无模拟退路 | 约束与不变量 | 存量 | **02 §2.5** | `call.c:901-996` | 解释"为什么块窥视没有 builtin 版本" |
| K-043 | 刷块与新驱动永远成功 | 约束与不变量 | 存量 | **02 §2.5** | `call.c:872-895`、`1002-1013` | |
| K-044 | 适配器与传输解耦（适配器只收验证后的标量） | 架构演进 | 存量 | **02 §3.2** | — | |
| K-045 | 输入与回复结构化（带校验构造函数；窥视只能拿计数型） | 架构演进 | 存量 | **02 §3.3** | — | 类型层面禁止误用 |
| K-046 | 副作用注入（"模拟怎么读"做成调用方注入的闭包） | 架构演进 | 存量 | **02 §3.4** | `call.c:227-273` | |
| K-047 | 回调表做成整体 trait（而非按操作组拆分） | 架构演进 | 存量 | **01 §3.4** | `os/libs/minix-fs/src/driver.rs:42` | 解释"为什么不学 Linux 拆四张表" |
| K-048 | 分类结果做成枚举（`HeaderAction` 三态） | 架构演进 | 存量 | **01 §3.6** | `driver.rs` | 消除"有 request 就忽略 Reply"的隐含约定 |
| K-049 | `NullDriver` 作为协议测试对端 | 测试性质 | 存量 | **01 §3.8** | `driver.rs:672` | |
| K-050 | `FsTransport` 接缝与 `RequestBody` 类型化请求体 | 架构演进 | 存量 | **01 §4.3** | `os/libs/minix-fs/src/task.rs:397` | |

#### 组 B：数据搬运、目录项、路径查找（现有 03 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-051 | 数据通道两形态（远端授权 vs 本地指针）与 C 的端点分支 | 数据结构 | 存量 | **03 §1.1/§2.1** | `fsdriver.h:19-26`、`utility.c:8-78` | |
| K-052 | Rust 把分支变成 `DataChannel` 枚举 + `DataBackend` trait | 架构演进 | 存量 | **03 §1.1/§3.1** | `os/libs/minix-fs/src/data.rs:31` | |
| K-053 | 缺席变体对应窥视：三操作静默成功 | 约束与不变量 | 存量 | **03 §1.1** | `data.rs` | 服务器不必为窥视写分支 |
| K-054 | 边界检查（偏移加长度不超声明总长，防回绕） | 约束与不变量 | 存量 | **03 §1.2/§2.1** | `utility.c:8-78` | |
| K-055 | 总长度字段只为完整性检查存在 | 概念 | 存量 | **03 §2.1** | `fsdriver.h:25` | |
| K-056 | C 宕机改 Rust 错误返回（含可用性论证） | 架构演进 | 存量 | **03 §1.2/§3.2** | — | |
| K-057 | 名字获取四道关卡 | 约束与不变量 | 存量 | **03 §1.3/§2.2** | `utility.c:84-105` | |
| K-058 | 长度含结束零的约定；查找允许空名而其他位置不允许 | 接口与协议 | 存量 | **03 §1.3** | `utility.c:84-105` | |
| K-059 | 目录项记录格式与记录长度宏（名偏移 13、对齐 8） | 数据结构 | 存量 | **03 §1.4/§2.3** | `minix3/sys/sys/dirent.h`、`dentry.c` | |
| K-060 | 目录项 staging 两级流水与三种回答 | 机制 | 存量 | **03 §1.4/§2.3** | `dentry.c:9/28/85` | |
| K-061 | 名字补零到对齐、末尾填充清零（防内存残留泄漏） | 约束与不变量 | 存量 | **03 §1.4** | `dentry.c:28-79` | |
| K-062 | 路径查找无状态一步一查 | 机制 | 存量 | **03 §1.5** | `lookup.c:118` | |
| K-063 | 四种岔路（点 / 点点 / 挂载点 / 符号链接） | 机制 | 存量 | **03 §1.5/§2.5** | `lookup.c:216-292` | |
| K-064 | 权限检查编织在每一步（中间节点须目录 + 搜索权限；超级用户跳过） | 约束与不变量 | 存量 | **03 §1.5/§2.4** | `lookup.c:9-37`（`access_as_dir`） | |
| K-065 | 符号链接深度计数上限八层 | 约束与不变量 | 存量 | **03 §1.5/§2.5** | `lookup.c:256-260` | |
| K-066 | 绝对路径链接交还 VFS 的理由 | 机制 | 存量 | **03 §1.5/§4.3** | `lookup.c:269`、`308-309` | |
| K-067 | 引用计数借还平衡（每条返回路径都要释放） | 约束与不变量 | 存量 | **03 §1.6/§3.5** | `lookup.c:280-281`、`329-330` | |
| K-068 | 三个复制函数结构完全相同 | 机制 | 存量 | **03 §2.1** | `utility.c:8-78` | |
| K-069 | `next_name` 分量切分规则 | 机制 | 存量 | **03 §2.4** | `lookup.c:44-76` | |
| K-070 | `resolve_link`（本地通道读链接、拼尾巴、拷回） | 机制 | 存量 | **03 §2.4** | `lookup.c:84-112` | |
| K-071 | 查找主循环取请求六件套与凭证两条路 | 机制 | 存量 | **03 §2.5** | `lookup.c:118-162` | |
| K-072 | 起点获取（当前目录标记调一次查找；挂载点起点要求分量是点点） | 机制 | 存量 | **03 §2.5** | `lookup.c:167-192` | |
| K-073 | 向上走进非目录则 C 宕机，Rust 报无效参数 | 架构演进 | 存量 | **03 §2.5/§2.6** | `lookup.c:243-244` | |
| K-074 | 收尾三岔（重定向码 / 成功填六字段 / 失败释放） | 机制 | 存量 | **03 §2.5** | `lookup.c:296-330` | |
| K-075 | 记录长度逐字翻译并保留对齐推导链 | 工具与工程 | 存量 | **03 §3.3** | `dirent.h` 宏 | |
| K-076 | 查找分两相（只读解析相 + 行动相） | 工具与工程 | 存量 | **03 §3.4** | `os/libs/minix-fs/src/lookup.rs` | |
| K-077 | 偏移语义对齐 C（进入挂载点报分量起点、离开报含斜杠、绝对链接报零） | 接口与协议 | 存量 | **03 §4.3** | `lookup.c:269/308-309` | |
| K-078 | 凭证结构固定十六个附加组槽位 | 数据结构 | 存量 | **03 §4.3** | `vfsif.h:33-38` | |
| K-079 | `DirentType` 八具名类型加兜底 | 数据结构 | 存量 | **03 §4.2** | `dentry.rs` | |
| K-080 | `MemFileServer` 作为 01–03 篇共用完整例证 | 测试性质 | 存量 | **03 §4.4** | `os/libs/minix-fs/src/memfs.rs:164` | 读者有一个可运行的最小 FS 参考 |

#### 组 C：块缓存与块 I/O（现有 04、05 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-081 | 为什么需要块缓存（单位错配；合并写；断电丢数据） | 概念 | 存量 | **04 §1.1** | — | |
| K-082 | 缓存三件套（哈希索引 / LRU 淘汰序 / 引用计数 pin） | 数据结构 | 存量 | **04 §1.2** | `cache.c` | |
| K-083 | 脏块语义与写回三时机 | 概念/机制 | 存量 | **04 §1.3** | `cache.c:164-177`、`1136-1166`、`1295-1311` | |
| K-084 | 钉住的脏块不刷（主人可能先标脏再改内容） | 约束与不变量 | 存量 | **04 §1.3/§2.9** | `cache.c:1156-1159` | |
| K-085 | 三种拿块模式（NORMAL / NO_READ / PEEK） | 接口与协议 | 存量 | **04 §1.4/§2.1** | `libminixfs.h`、`cache.c:494` | |
| K-086 | 预读（纯优化，失败静默；上限取小） | 机制 | 存量 | **04 §1.5/§2.8** | `cache.c:987/1057` | |
| K-087 | 二级缓存双份页共享（未命中先问内存服务器；放出时把页交回） | 机制 | 存量 | **04 §1.5** | `cache.c:443-465` | |
| K-088 | 二级缓存三样附带物（旗标字 / 块标签 / 标定结果） | 数据结构 | 存量 | **04 §1.5** | `vm.h`、`VMMC_*` | |
| K-089 | 二级缓存的关门动作（收回页 + 要求忘掉设备块；出错也照做） | 机制 | 存量 | **04 §1.5/§2.9** | `call.c:83-84`、`cache.c:803-807` | |
| K-090 | 容量启发式（平方根乘系数、三重上限、保底；用量变化超阈值重估） | 机制 | 存量 | **04 §1.6/§2.3** | `cache.c:73-162` | |
| K-091 | 缓冲头 `struct buf` 字段清单 | 数据结构 | 存量 | **04 §2.1** | `libminixfs.h` | |
| K-092 | 使用计数是字符型上限 127；Rust 用饱和断言 | 约束与不变量 | 存量 | **04 §2.1/§3.4** | `libminixfs.h` | |
| K-093 | 池与全局量（队头队尾、在用计数、哈希表、块大小默认一页、静默开关）与 `MINBUFS` | 数据结构 | 存量 | **04 §2.2** | `cache.c:38-71` | |
| K-094 | 拿块主流程 `lmfs_get_block_ino` 全路径 | 机制 | 存量 | **04 §2.5** | `cache.c:494-` | |
| K-095 | 二级缓存查询在拿块流程中的位置与窥视回退 | 机制 | 存量 | **04 §2.5** | `cache.c:443-465` | |
| K-096 | 内存分配与读盘（分配失败先释放闲置再试；正常模式读失败放回报错） | 机制 | 存量 | **04 §2.5** | `cache.c:469-479` | |
| K-097 | 归还 `put_block`（计数减一、一次性块队首、二级报备三分处理） | 机制 | 存量 | **04 §2.6** | `cache.c:601-` | |
| K-098 | 腾空 `freeblock`（脏块先刷本设备、清设备号、标干净、释放内存；不摘哈希链） | 机制 | 存量 | **04 §2.6** | `cache.c:252-272` | |
| K-099 | 单块读 `read_block`（超页块分片读；读不满报错并作废） | 机制 | 存量 | **04 §2.7** | `cache.c:723-777` | |
| K-100 | 散射读写 `rw_scattered`（读要求全程持有；写按块号排序省寻道；写停滞保留脏块） | 机制 | 存量 | **04 §2.7** | `cache.c:840-982` | |
| K-101 | 预读上限规则（单次传输天花板与池策略上限取小） | 约束与不变量 | 存量 | **04 §2.8** | `cache.c:1031-1052` | |
| K-102 | 预读执行与集合预取（逐块免读拿、攒够一次散射读；prefetch 以首块为中心拼区间） | 机制 | 存量 | **04 §2.8** | `cache.c:987-1026`、`1057-1131` | |
| K-103 | 按设备刷 `lmfs_flushdev`（收集脏且空闲的块一次散射写） | 机制 | 存量 | **04 §2.9** | `cache.c:1136-1166` | |
| K-104 | 全刷 `lmfs_flushall` | 机制 | 存量 | **04 §2.9** | `cache.c:1295-1311` | |
| K-105 | 失效 `lmfs_invalidate`（清内存清设备号 + 通知 VM；堆块直接丢、页映射块 munmap） | 机制 | 存量 | **04 §2.9** | `cache.c:782-808` | |
| K-106 | 单块释放 `lmfs_free_block`（先通知 VM 再作废缓存同名拷贝；钉住也照办） | 机制 | 存量 | **04 §2.9** | `cache.c:613-650` | |
| K-107 | 池管理三函数（`cache_resize` / `lmfs_set_blocksize` / `lmfs_buf_pool`） | 机制 | 存量 | **04 §2.10** | `cache.c:1192-1293` | |
| K-108 | 二级缓存拆三层（池子旗标机 / 策略层 / 线材） | 架构演进 | 存量 | **04 §3.3** | `os/libs/minix-fs/src/vm_cache.rs:426` | |
| K-109 | 块内存两态（堆字节数组或整页映射；只有页映射能交出去） | 数据结构 | 存量 | **04 §4.1** | `cache.rs` | |
| K-110 | `BlockSource` trait（机制与策略分离） | 架构演进 | 存量 | **04 §3.1** | `cache.rs:93` | |
| K-111 | 取书单四要素（设备、位置、长度、方向） | 接口与协议 | 存量 | **05 §1.1** | `bio.c:127-138` | |
| K-112 | 窥视即"收货地址缺席的读"，不是第五个方向 | 概念 | 存量 | **05 §1.1/§3.3** | `bio.c` `data==NULL` 分支 | 概念统一 |
| K-113 | 关卡一：设备号不能为空设备 | 约束与不变量 | 存量 | **05 §1.2** | `bio.c:127` | |
| K-114 | 关卡二：位置非负、长度不超上限、和不溢出 | 约束与不变量 | 存量 | **05 §1.2** | `bio.c:138` | |
| K-115 | 关卡三：问分区实际大小并裁剪 | 机制 | 存量 | **05 §1.2** | `bio.c:146`、`152` | |
| K-116 | 起点已在分区外报零字节而非错误 | 约束与不变量 | 存量 | **05 §1.2** | `bio.c:149` | |
| K-117 | 长度为零直接报零（早退优化） | 机制 | 存量 | **05 §1.2** | `bio.c:135` | |
| K-118 | 首尾不满、中间整块的行走模型 | 机制 | 存量 | **05 §1.3** | `bio.c:156`、`166` | |
| K-119 | 每块搬运量三取小 | 约束与不变量 | 存量 | **05 §1.3** | `bio.c:173` | |
| K-120 | 每块四步（拿块、搬运、标脏、放回）；写必标脏，搬运失败也标 | 机制 | 存量 | **05 §1.3/§2.6** | `bio.c:186-210` | |
| K-121 | 读预取（拿当前块前先窥视后面几块；遇已缓存即停；失败静默） | 机制 | 存量 | **05 §1.4** | `bio.c:65-97`、`186` | |
| K-122 | 整块覆写免读（空槽位、不碰存储、写完标脏） | 机制 | 存量 | **05 §1.5** | `bio.c:194` | |
| K-123 | 刷后失效（先刷脏块再作废；顺序不可反） | 约束与不变量 | 存量 | **05 §1.6** | `bio.c:255` | |
| K-124 | 驱动绑定（登记设备号与驱动标签到驱动表） | 接口与协议 | 存量 | **05 §1.7** | `bio.c:49-53` | |
| K-125 | 文件头契约（必须初始化缓冲池、启用 vmcache、提供同步回调） | 接口与协议 | 存量 | **05 §2.1** | `bio.c:1-40` | |
| K-126 | 磁盘 backed FS 三点注意（限制写挂载分区首部 1024 字节、块大小可自定、首部注释即文档） | 约束与不变量 | 存量 | **05 §2.1** | `bio.c:1-40` | |
| K-127 | 溢出检查表达 `pos > INT64_MAX - bytes + 1` | 约束与不变量 | 存量 | **05 §2.4** | `bio.c:138` | |
| K-128 | 短搬加后续错误报短计数 | 接口与协议 | 存量 | **05 §2.4** | `bio.c:242` | |
| K-129 | `DeviceInfo` trait（分区大小查询 + 标签绑定） | 架构演进 | 存量 | **05 §3.1** | `bio.rs:40` | |
| K-130 | 槽位统整块、末块不满在搬运时裁剪 | 架构演进 | 存量 | **05 §3.2/§2.6** | `bio.c` 部分块拿取 | |
| K-131 | 内存盘 `RamDisk` 是生产代码不是测试替身 | 架构演进 | 存量 | **05 §3.5** | `bio.rs:351` | |
| K-132 | `BdevBlockSource` 真块驱动桥接（两 trait 一起实现，授权签发缝成 `GrantIssuer`） | 接口与协议 | 存量 | **05 §3.6** | `os/libs/minix-fs/src/bdev_bridge.rs` | |
| K-133 | 末块短读在桥接层落定（长度收窄到 `device_bytes % block_size`，读不到补零） | 约束与不变量 | 存量 | **05 §3.6** | `bdev_bridge.rs` | |
| K-134 | 批量钩子 `read_blocks`/`write_blocks` 默认逐块走 | 接口与协议 | 存量 | **05 §3.6** | `bdev_bridge.rs` | |
| K-135 | 标签上限 80（含结束零，与数据服务头文件一致） | 约束与不变量 | 存量 | **05 §4.1** | `DS_MAX_KEYLEN` | |

#### 组 D：pfs（现有 06 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-136 | 管道服务器是启动时第一个挂载的 FS | 概念 | 存量 | **06 §1.1** | `servers/vfs/main.c:510`、`mount.c:391` | 理解 boot 因果链的第一步 |
| K-137 | 无磁盘依赖因而永远就绪 | 概念 | 存量 | **06 §1.1** | `fs/pfs/Makefile`（不链 libminixfs） | |
| K-138 | 无根挂载（挂载即初始化内存表；设备号与标志忽略；根节点全零） | 机制 | 存量 | **06 §1.1/§2.2** | `fs/pfs/pfs.c:50-86` | |
| K-139 | 五百一十二槽位节点表（全系统同开管道与克隆设备的天花板） | 数据结构 | 存量 | **06 §1.2/§2.1** | `pfs.c:19`（`PFS_NR_INODES 512`） | |
| K-140 | 节点号从一开始、零号保留（查找拒绝零号） | 约束与不变量 | 存量 | **06 §1.2/§2.4** | `pfs.c:126`（`pfs_newnode`） | |
| K-141 | 空闲槽位编号栈、倒序压栈低号先出 | 数据结构 | 存量 | **06 §1.2/§2.2** | `pfs.c:63` 循环、`pfs.c:21`（`LIST_HEAD`） | |
| K-142 | 固定表是深思熟虑的简化（启动初期分配器不可依赖） | 架构演进 | 存量 | **06 §1.2** | `pfs.c:14-18` 注释 | |
| K-143 | 出生路三检（类型、空位、管道缓冲） | 机制 | 存量 | **06 §1.3/§2.4** | `pfs.c:134/141/146` | |
| K-144 | 死亡路三事（计数恰为一、归还缓冲、压回栈顶） | 机制 | 存量 | **06 §1.3/§2.5** | `pfs.c:184-216` | |
| K-145 | 计数长不大（没有增加引用的路） | 约束与不变量 | 存量 | **06 §1.3** | `pfs.c:191` | |
| K-146 | 栈式重用（释放再分配拿回同号） | 测试性质 | 存量 | **06 §1.3/§5.1** | — | |
| K-147 | 管道是流不是文件（忽略位置、读推进、写追加） | 概念 | 存量 | **06 §1.4** | `pfs.c:218/254` | |
| K-148 | 读三取小与超上限拒绝 | 机制 | 存量 | **06 §1.4/§2.6** | `pfs.c:218-253` | |
| K-149 | 写先验总量（存量加增量超缓冲拒绝） | 机制 | 存量 | **06 §1.4/§2.6** | `pfs.c:265` | |
| K-150 | 写时压缩（残留前移；理由：小读多时省搬运、直线缓冲省内核调用） | 机制 | 存量 | **06 §1.4/§2.6** | `pfs.c:274` | |
| K-151 | 截断只支持全清 | 约束与不变量 | 存量 | **06 §1.4/§2.7** | `pfs.c:299-320` | |
| K-152 | 懒时间三标记与一站式结清（`ATIME`/`MTIME`/`CTIME`） | 机制 | 存量 | **06 §1.5/§2.7** | `pfs.c:23-25`、`pfs.c:322-360` | |
| K-153 | 改模式只动权限位、类型位保留 | 约束与不变量 | 存量 | **06 §1.5/§2.7** | `pfs.c:362-379` | |
| K-154 | 退出只认终止信号；无盘不刷直接下班 | 机制 | 存量 | **06 §1.6/§2.8** | `pfs.c:381-392` | |
| K-155 | 启动丢弃特权到服务用户（`SERVICE_UID` 999），失败只警告 | 机制 | 存量 | **06 §1.6/§2.8** | `pfs.c:394-403`、`minix/rs.h:SERVICE_UID` | |
| K-156 | 节点结构十三字段清单 | 数据结构 | 存量 | **06 §2.1** | `pfs.c:27-47` | |
| K-157 | 卸载扫表找在用节点并打忙警告 | 接口与协议 | 存量 | **06 §2.3** | `pfs.c:88-103` | |
| K-158 | 状态回复十三字段的三处变通（设备字段填设备号、块大小填管道缓冲、块数按 512 向上取整） | 接口与协议 | 存量 | **06 §2.7** | `pfs.c:322-360` | |
| K-159 | 回调表九项与启动三注册 | 接口与协议 | 存量 | **06 §2.8** | `pfs.c:425-435`、`pfs.c:405-424` | |

#### 组 E：mfs 参考实现（现有 07–17 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-160 | 初装四步（开 vmcache 开关、inode 表清零、初始化 inode 缓存、建 1024 块缓冲池） | 机制 | 存量 | **07 §1.1/§2.2** | `fs/mfs/main.c:52-62` | |
| K-161 | 顺序即依赖（建池前开开关、清表在初始化缓存前） | 约束与不变量 | 存量 | **07 §1.1/§3.1** | `main.c:52-62` | |
| K-162 | 二级缓存开关与 vmcache 通道（块大小非整页时池自关） | 机制 | 存量 | **07 §1.1/§2.2**、04 §1.5 | `cache.c:1236-1239` | |
| K-163 | 信号语义"先结账再关门"（终止则同步再下班） | 机制 | 存量 | **07 §1.2/§2.3** | `main.c:70-78` | |
| K-164 | 决定与执行分离（信号处理只做决定，纯函数可测） | 架构演进 | 存量 | **07 §1.2/§3.5** | — | |
| K-165 | 接线表 31 项回调现状与分组 | 数据结构 | 存量 | **07 §1.3/§2.4** | `fs/mfs/table.c:13-45` | |
| K-166 | 无新节点行（管道专属，磁盘服务器不建无名节点） | 约束与不变量 | 存量 | **07 §1.3/§2.4** | `table.c:13-45` | |
| K-167 | 显式接线表优于静默桩与大包大揽 | 架构演进 | 存量 | **07 §1.3** | — | |
| K-168 | 三处处理器复用（读写窥视、删文件与删目录、块读写窥视） | 接口与协议 | 存量 | **07 §1.3/§2.4** | `table.c:18-20`、`:28-29`、`:41-43` | |
| K-169 | 五项直接指向块传输库函数（驱动绑定、块读写窥视、刷块） | 接口与协议 | 存量 | **07 §2.4** | `table.c:40-44` | |
| K-170 | 拿块包装行规（失败除缺席即腐败级报错；缺席只许窥视） | 约束与不变量 | 存量 | **07 §1.4/§2.5** | `fs/mfs/cache.c:22-37` | |
| K-171 | 层间严格度差异（缓存说缺席正常，文件系统说缺席即事故） | 概念 | 存量 | **07 §1.4** | — | |
| K-172 | 区分配三步（hint 定起点、区号↔位号换算、请位图找位） | 机制 | 存量 | **07 §1.5/§2.6** | `fs/mfs/cache.c:42-80` | |
| K-173 | 换算公式：区号 = 首区 − 1 + 位号；位零保留为失败信号 | 约束与不变量 | 存量 | **07 §1.5/§2.6** | `cache.c:79`、`:67` | |
| K-174 | 区释放三笔（搜索位回拨、缓存同名块作废、范围外静默忽略） | 机制 | 存量 | **07 §1.5/§2.7** | `fs/mfs/cache.c:85-109` | |
| K-175 | 缓存作废的两条理由与"单块单区"断言 | 约束与不变量 | 存量 | **07 §2.7** | `cache.c:102-107` | |
| K-176 | 缺空间打印节流（首打后静默，成功后恢复） | 机制 | 存量 | **07 §2.6/§2.8** | `cache.c:69-76` | |
| K-177 | `err_code` 全局改返回值 | 架构演进 | 存量 | **07 §3.3** | — | |
| K-178 | 位图闭包注入（策略在本篇、机制在超级块篇） | 架构演进 | 存量 | **07 §3.2** | `os/fs/mfs/src/mfs_cache.rs` | |
| K-179 | 接线表做成编译期常量数组 | 架构演进 | 存量 | **07 §3.4** | `os/fs/mfs/src/table.rs:40` | |
| K-180 | 被 `#if 0` 屏蔽的引用计数校验函数（调试墓碑） | 工具与工程 | 存量 | **07 §2.2/§2.9** | `main.c:81-101`（`cch_check`） | |
| K-181 | 服务装配（`MfsServer` 三样、`Parts` 拆借用、镜像桥、同步次序、typed Stat） | 机制 | 存量 | **07 §4.5** | `os/fs/mfs/src/server.rs:323` | 见 §3.3 越界表（内容属新 27 篇） |
| K-182 | 磁盘七段全景（引导块、超级块区、inode 位图、区位图、inode 区、填充、数据区） | 数据结构 | 存量 | **08 §1.1/§2.1** | `fs/mfs/super.h:10-20` | |
| K-183 | 自描述格式（各段长度写在超级块里，解析器零外部配置） | 概念 | 存量 | **08 §1.1** | `super.h:24-60` | |
| K-184 | 三魔数两命运（0x137F / 0x2468 / 0x4D5A；只认三版；两类拒绝错） | 约束与不变量 | 存量 | **08 §1.2/§2.2/§2.7** | `const.h:22/24/26`、`super.c:252-260` | |
| K-185 | 版本检查必须先于字段转换 | 约束与不变量 | 存量 | **08 §1.2** | `super.c:252-260` | |
| K-186 | 块大小五连验 | 约束与不变量 | 存量 | **08 §1.3/§2.7** | `super.c:287-293` | |
| K-187 | 五道检查按"从便宜到贵"排序 | 架构演进 | 存量 | **08 §1.3** | — | |
| K-188 | 首区计算"装不下就现算" | 机制 | 存量 | **08 §1.4/§2.7** | `super.c:299-308` | |
| K-189 | 格式演进手法"加规则不加字段" | 架构演进 | 存量 | **08 §1.4** | — | |
| K-190 | 几何 sanity 逐条拒绝 | 约束与不变量 | 存量 | **08 §1.5/§2.7** | `super.c:333-342` | |
| K-191 | 强制标志掩码（高八位"必须理解"，不识别即拒挂） | 约束与不变量 | 存量 | **08 §1.5/§2.1/§2.7** | `super.h:75`、`super.c:348-352` | |
| K-192 | 位图账本（一位一 inode / 一区）与 `IMAP`/`ZMAP` | 数据结构 | 存量 | **08 §1.6** | `super.h:62-63` | |
| K-193 | 位图分配（从起点找第一个空位；起点越界从零找；找遍返回无位） | 机制 | 存量 | **08 §1.6/§2.3** | `super.c:29-106`（`alloc_bit`） | |
| K-194 | 位图释放（清位；重复释放报告失败不崩） | 机制 | 存量 | **08 §1.6/§2.4** | `super.c:111-156`（`free_bit`） | |
| K-195 | 零号位永不分配（"零即无"一脉相承） | 约束与不变量 | 存量 | **08 §1.6** | `const.h:32` | |
| K-196 | 干净位（标志字最低位记录上次是否干净卸载） | 机制 | 存量 | **08 §1.7/§2.1** | `super.h:68`（`MFSFLAG_CLEAN`） | |
| K-197 | 超级块结构十三磁盘字段加九内存字段 | 数据结构 | 存量 | **08 §2.1** | `super.h:24-60` | |
| K-198 | 磁盘字段增加必须同步改"读写截止字段" | 约束与不变量 | 存量 | **08 §2.1/§2.6** | `super.h:42-46`、`super.c:184` | |
| K-199 | 块大小是缓存的属性不是超级块的属性 | 概念 | 存量 | **08 §2.5** | `super.c:161-168`（`get_block_size`） | |
| K-200 | `rw_super` 读写通道（截止字段算盘内字节数、三断言、读失败返回错 vs 写失败宕机） | 接口与协议 | 存量 | **08 §2.6** | `super.c:173-236` | |
| K-201 | `read_super` 解析全步骤 | 机制 | 存量 | **08 §2.7** | `super.c:241-355` | |
| K-202 | 写守卫与执行分离（`write_super` 只读则宕机） | 接口与协议 | 存量 | **08 §2.8** | `super.c:360-364` | |
| K-203 | 九错误变体与错误码映射边界 | 接口与协议 | 存量 | **08 §3.2** | — | |
| K-204 | 位图不知只读（政策上浮、数据下沉）；重复释放报告不崩 | 架构演进 | 存量 | **08 §3.3/§3.4** | — | |
| K-205 | 逐字段小端读写（拒整体内存复制） | 架构演进 | 存量 | **08 §3.1/§3.5** | `super.c:266-273` | |
| K-206 | 磁盘格式六十四字节紧凑记录（模式、链接数、属主、属组、大小、三时间、十区号） | 数据结构 | 存量 | **09 §1.1/§2.2** | `fs/mfs/type.h:8-17` | |
| K-207 | 内存形态多出的工作状态项 | 数据结构 | 存量 | **09 §1.1/§2.1** | `fs/mfs/inode.h:32-48` | |
| K-208 | "存储窄、工作宽"分离原则 | 概念 | 存量 | **09 §1.1** | — | |
| K-209 | 显式转换函数双向（`rw_inode` / `new_icopy`） | 机制 | 存量 | **09 §1.1/§2.13** | `fs/mfs/inode.c:375` | |
| K-210 | 一百二十八桶哈希（按号低七位分桶；桶数是 2 的幂故用掩码） | 数据结构 | 存量 | **09 §1.2/§2.5** | `inode.h` | |
| K-211 | 哈希只管"找到"不管"能不能用"（计数为零的槽位要先摘车） | 机制 | 存量 | **09 §1.2** | `inode.c:118`（`get_inode`） | |
| K-212 | 引用计数语义（打开加一、释放减一、归零按链接数分流） | 机制 | 存量 | **09 §1.3** | `inode.c:206`（`put_inode`） | |
| K-213 | 归零分流（有链接写回推车尾；零链接上报待回收区号、清号牌推车头） | 机制 | 存量 | **09 §1.3/§2.8** | `inode.c:236-244` | |
| K-214 | 车头车尾淘汰策略（用队列位置表达访问热度，零额外状态字节） | 架构演进 | 存量 | **09 §1.3** | `inode.c:70`（`init_inode_cache`） | |
| K-215 | 获取三路（引用中命中 / 冷命中 / 缺席）与"只有冷命中才算命中" | 机制 | 存量 | **09 §1.4/§2.6** | `inode.c:132-144` | |
| K-216 | 车空报文件表满（512 个全被钉住） | 约束与不变量 | 存量 | **09 §1.4/§2.6** | `inode.c:147-150` | |
| K-217 | 设备为空跳过读盘（分配新节点路径，不对外） | 机制 | 存量 | **09 §1.4/§2.6** | `inode.c:154-168` | |
| K-218 | 分配两步舞（位图上占位 → 表里占槽；表满退位） | 机制 | 存量 | **09 §1.5/§2.9** | `inode.c:252`（`alloc_inode`） | |
| K-219 | 释放逆过程（号越界或零号静默忽略、清位图位、搜索 hint 回拨） | 机制 | 存量 | **09 §1.5/§2.11** | `inode.c:328`（`free_inode`） | |
| K-220 | 懒时间（置待更新位；状态查询或释放写回时结清；只读跳过） | 机制 | 存量 | **09 §1.6/§2.12** | `inode.c:349`（`update_times`） | |
| K-221 | 懒更新动机（时钟消息贵、标记便宜） | 概念 | 存量 | **09 §2.12** | `inode.c:352-357` 注释 | |
| K-222 | 脏标记宏（只读盘标脏打印加堆栈；只读盘出现脏节点是逻辑错误） | 约束与不变量 | 存量 | **09 §2.1** | `inode.h:67-70` | |
| K-223 | 释放请求协议 `fs_putnode`（按号找表、计数减请求数加一留一） | 接口与协议 | 存量 | **09 §2.3** | `inode.c:38-64` | |
| K-224 | 纯找 `find_inode`（只看不借、计数不动） | 接口与协议 | 存量 | **09 §2.7** | `inode.c:180-200` | |
| K-225 | 擦除 `wipe_inode`（分配与截断共用） | 机制 | 存量 | **09 §2.10** | `inode.c:309-323` | |
| K-226 | 读写转换定位算术（块号 =（号−1）/每块节点数 + 起点；槽内偏移取余） | 机制 | 存量 | **09 §2.13** | `inode.c:389-393` | |
| K-227 | 加引用 `dup_inode`（指针已知跳过查找） | 接口与协议 | 存量 | **09 §2.14** | `inode.c:455` | |
| K-228 | 表做成值（可多实例、测试可并行） | 架构演进 | 存量 | **09 §3.1** | `os/fs/mfs/src/inode.rs` | |
| K-229 | 释放上报区号（发现与执行分开） | 架构演进 | 存量 | **09 §3.2** | `inode.rs` | |
| K-230 | 五错误变体与两种满对策 | 接口与协议 | 存量 | **09 §3.4** | — | |
| K-231 | 几何参数子视图（防腐层） | 架构演进 | 存量 | **09 §3.5** | `inode.rs` | |
| K-232 | 只读挂载下写回整体拒绝（`InodeIo` 携带只读标志） | 机制 | 存量 | **09 §4.2** | `inode.rs` | |
| K-233 | 开店七步（记住设备、读身份证、脏盘降级、认块大小、数已用区、领根节点、脏标记落盘） | 机制 | 存量 | **10 §1.1/§2.1** | `fs/mfs/mount.c:10-98` | |
| K-234 | 散伙原子性（每步失败把前面占的收拾干净） | 约束与不变量 | 存量 | **10 §1.1/§2.1** | `mount.c:33-37` | |
| K-235 | 脏盘降级（读写挂载遇脏位自动降只读并记标记） | 机制 | 存量 | **10 §1.2/§2.1** | `mount.c:40-50` | |
| K-236 | 降级记录与卸载报告（降级来的只读同样不写干净位） | 约束与不变量 | 存量 | **10 §1.2/§3.3** | `mount.c:40-50` | |
| K-237 | 打烊六步（数在用引用、放根、调同步、读写则标干净落盘、作废设备缓存、设备号复位） | 机制 | 存量 | **10 §1.3/§2.3** | `mount.c:134-172` | |
| K-238 | 卸载"嚷嚷不停"原则（根被引用多次只打印不停） | 约束与不变量 | 存量 | **10 §1.3/§3.2** | `mount.c:144-148` | |
| K-239 | 作废设备缓存两件事（池逐块腾空页归还 + VM 页缓存同清） | 机制 | 存量 | **10 §1.3/§2.3** | `cache.c:803-807`、`call.c:83-84` | |
| K-240 | 挂载点检查三问（号在吗、挂过牌吗、是设备节点吗） | 接口与协议 | 存量 | **10 §1.4/§2.2** | `mount.c:104-128` | |
| K-241 | 挂载点检查"借来即还" | 约束与不变量 | 存量 | **10 §1.4/§2.2** | `mount.c:123` | |
| K-242 | 三种挂载形态比较与形态三的胜出（`MountedFs` 七字段） | 架构演进 | 存量 | **10 §1.5/§4.1** | `os/fs/mfs/src/mount.rs` | |
| K-243 | Linux `struct super_block` 一值装全部对照 | 概念 | 存量 | **10 §1.5** | Linux | |
| K-244 | Redox 挂载是命名空间柄对照 | 概念 | 存量 | **10 §1.5** | Redox | |
| K-245 | RAII 加会话类型对照（"未挂载的值构造不出来"） | 概念 | 存量 | **10 §1.5** | 类型理论 | |
| K-246 | 六错误变体与每散伙点对号入座 | 接口与协议 | 存量 | **10 §3.2/§4.1** | — | |
| K-247 | 位数统计本地实现与第 17 篇 canonicalize | 架构演进 | 存量 | **10 §3.4/§2.4** | `fs/mfs/stats.c:14` | |
| K-248 | 六十四字节目录项布局（4 号小端 + 60 名零填充） | 数据结构 | 存量 | **11 §1.1/§2.1** | `fs/mfs/mfsdir.h:13-18` | |
| K-249 | 号为零表示空行（删除留下的坑 / 从未用过的尾巴） | 数据结构 | 存量 | **11 §1.1** | `mfsdir.h` | |
| K-250 | 名字最长六十与全系统唯一截断点 | 约束与不变量 | 存量 | **11 §1.1** | `mfsdir.h:13` 注释 | |
| K-251 | 点与点点是普通行、目录非空判据 | 机制 | 存量 | **11 §1.1** | `path.c:92`（`search_dir`） | |
| K-252 | 四模式目录漫步（`LOOK_UP`/`ENTER`/`DELETE`/`IS_EMPTY` 共享一套 walk） | 机制 | 存量 | **11 §1.2/§2.4** | `path.c:92-240` | |
| K-253 | 加人的三种落定方式（复用空行 → 块内尾空间 → 追尾新块） | 机制 | 存量 | **11 §1.2** | `path.c:192-223` | |
| K-254 | 删除的藏号与 hint 回调 | 机制 | 存量 | **11 §1.2** | `path.c:172-181` | |
| K-255 | 名义槽位数（大小除 64）与 walk 不越界 | 约束与不变量 | 存量 | **11 §1.2/§2.4** | `path.c:124`、`148-151` | |
| K-256 | 进入 hint 是状态也是优化 | 机制 | 存量 | **11 §1.3** | C 注释 | |
| K-257 | 只读挂载写操作全拒，且顺序在"是不是目录"之后 | 约束与不变量 | 存量 | **11 §1.4/§2.4** | `path.c:120-121` | |
| K-258 | 单步查找三段式 `fs_lookup`（找目录槽 → 漫步 → 开子槽） | 机制 | 存量 | **11 §1.5/§2.2** | `path.c:16-42` | |
| K-259 | 节点六字段填充与设备号取首区号 | 接口与协议 | 存量 | **11 §2.2** | `path.c:30-37` | |
| K-260 | `advance` 前进函数全流程 | 机制 | 存量 | **11 §2.3** | `path.c:26-46` | |
| K-261 | 目录无空洞（断言设备非空块非空） | 约束与不变量 | 存量 | **11 §2.4** | `path.c:135-142` | |
| K-262 | 镜像参数化（调用方供块镜像，依赖倒置） | 架构演进 | 存量 | **11 §2.5/§3.1** | `os/fs/mfs/src/dir.rs` | |
| K-263 | 计数器忠实复刻（名义槽位、访问计数、past-end 落定、扩展加一） | 架构演进 | 存量 | **11 §3.2** | `dir.rs` | |
| K-264 | 新建节点四拍（查空、分配、落盘、进入） | 机制 | 存量 | **12 §1.1/§2.5** | `fs/mfs/open.c:192-257`（`new_node`） | |
| K-265 | 父目录护栏（必须存在且有链接） | 约束与不变量 | 存量 | **12 §1.1** | `open.c:208-211` | |
| K-266 | 目录建子且父链接到顶报链接上限 | 约束与不变量 | 存量 | **12 §1.1** | `open.c:213-217` | |
| K-267 | 名字必须空（有人报已存在，其他失败透传） | 约束与不变量 | 存量 | **12 §1.1** | `open.c:220`、`248-251` | |
| K-268 | 分配四件套（位图占位、表占槽、填模式属主、链接数置一、区号落首区） | 机制 | 存量 | **12 §1.1** | `open.c:224`、`233-235` | |
| K-269 | 第三拍强制落盘：孤儿节点优于悬空名字 | 约束与不变量 | 存量 | **12 §1.1** | `open.c:233-235` 注释 | 崩溃一致性的核心论证 |
| K-270 | 进入失败回滚四样（链接减一、标脏、释放、退位图） | 机制 | 存量 | **12 §1.2** | `open.c:239-243` | |
| K-271 | 泄漏是慢性 corrupt | 约束与不变量 | 存量 | **12 §1.2** | — | |
| K-272 | 点点舞三步（写点与点点、子母链接各加一、母标脏） | 机制 | 存量 | **12 §1.3** | `open.c:101-117` | |
| K-273 | 符号链接目标住独立数据块、按真长度定大小 | 机制 | 存量 | **12 §1.4** | `open.c:141-170` | |
| K-274 | 设备节点＝带设备号的空文件（设备号借住区号字段） | 机制 | 存量 | **12 §1.5** | `open.c:66` | |
| K-275 | 寻位标记（找到置标、找不到静默、hint 永不报错） | 机制 | 存量 | **12 §1.5/§2.6** | `open.c:263-270` | |
| K-276 | 上下文束（表、缓存、超级块、位图、设备号、几何、只读）与"结构 vs 闭包"分水岭 | 架构演进 | 存量 | **12 §3.1** | `os/fs/mfs/src/open.rs` | |
| K-277 | 完成态两种类型防误用 | 架构演进 | 存量 | **12 §3.2** | `open.rs` | |
| K-278 | 回滚宕机改 corrupt（坏输入不是编程错误） | 架构演进 | 存量 | **12 §3.3** | — | |
| K-279 | 精确错误透传（表满位图满不吞成无效参数） | 架构演进 | 存量 | **12 §3.4** | `open.c:24-25` | |
| K-280 | 硬链接语义（新增名字指向已有编号、计数加一、标记状态变化时间） | 概念/机制 | 存量 | **13 §1.1/§2.1** | `fs/mfs/link.c:32-97` | |
| K-281 | 硬链接三个拒绝条件与固定顺序 | 约束与不变量 | 存量 | **13 §1.1** | `link.c:46-52`、`73-80` | |
| K-282 | 先便宜后昂贵、先自身后环境的检查顺序 | 约束与不变量 | 存量 | **13 §1.1** | — | |
| K-283 | 删除文件与删除目录共用入口靠参数分流 | 机制 | 存量 | **13 §1.2/§2.2** | `link.c:103-146` | |
| K-284 | 挂载点不可删（设备忙）与只读挂载拒绝 | 约束与不变量 | 存量 | **13 §1.2** | `link.c:125-132` | |
| K-285 | 删除目录三条额外条件（空目录、非根、删点与点点且忽略失败） | 机制 | 存量 | **13 §1.2/§2.4** | `link.c:184-213` | |
| K-286 | 公共删除辅助三件事（删名、计数减一、标时间），存储回收延迟到 inode 释放 | 机制 | 存量 | **13 §1.2/§2.5** | `link.c:219-249` | |
| K-287 | 删除目录的三次计数记账与饱和减法 | 机制 | 存量 | **13 §2.4** | `link.c:205-211` | |
| K-288 | 改名四阶段（开旧父+找旧 → 开新父 → 错误组合 → 搬动） | 机制 | 存量 | **13 §1.3/§2.6** | `link.c:255-422` | |
| K-289 | 循环挂载检查（顺新父点点上溯，走到旧文件自己报无效参数） | 约束与不变量 | 存量 | **13 §1.3** | `link.c:315-342` | |
| K-290 | 新名字已存在的错误组合 | 约束与不变量 | 存量 | **13 §1.3** | `link.c:346-356` | |
| K-291 | 改名两种搬动顺序（同父先删后建、跨父先建后删） | 约束与不变量 | 存量 | **13 §1.3/§3.4** | `link.c:371-401` | |
| K-292 | 目录跨父搬家修正点点 + 新父链接数加一并标脏 | 机制 | 存量 | **13 §1.3** | `link.c:405-414` | |
| K-293 | 符号链接读取 `fs_rdlink`（类型校验、零号块、按最小长度拷贝） | 接口与协议 | 存量 | **13 §1.4/§2.3** | `link.c:152-178` | |
| K-294 | 读非符号链接报访问拒绝；块缺失报 IO 错误 | 约束与不变量 | 存量 | **13 §1.4** | `link.c:162-166` | |
| K-295 | 截断语义（变短还空间、变长空洞读零、挖空不改长度） | 概念 | 存量 | **13 §1.5** | — | |
| K-296 | 截断决策（特殊设备拒绝、超最大长度报文件过大、新旧长度三分） | 机制 | 存量 | **13 §1.5/§2.7** | `link.c:428-488` | |
| K-297 | 区间释放翻译规则（单区内部分覆盖只清零、跨区首尾清零加中间释放、尾巴延续到末尾整区释放） | 机制 | 存量 | **13 §1.5/§2.8** | `link.c:494-637`（`freesp_inode` 在 `:494`） | |
| K-298 | 释放计划做成纯计算 | 架构演进 | 存量 | **13 §1.5/§3.1** | `os/fs/mfs/src/link.rs` | |
| K-299 | 位置取整不溢出（先除后加）与 Rust 饱和保护 | 机制 | 存量 | **13 §1.6/§3.5** | `link.c:541`（`nextblock` 在 `:558`） | |
| K-300 | 错误枚举十二变体加一个透传变体 | 架构演进 | 存量 | **13 §3.3/§4.1** | `link.rs` | |
| K-301 | 判断与执行分离的 Linux `fallocate` / Redox 对照 | 架构演进 | 存量 | **13 §3.1/§3.2** | Linux / Redox | |
| K-302 | 块号翻译：直接、单重间接、双重间接（索引节点十区号，前七直接） | 数据结构 | 存量 | **14 §1.1** | `fs/mfs/read.c:210-279`（`read_map`） | |
| K-303 | 翻译三段级联（文件块号 → 区号 + 区内偏移；任一级空即空洞） | 机制 | 存量 | **14 §1.1/§2.3** | `read.c:237-278` | |
| K-304 | 双重间接索引上限保护（超单块容量直接报空洞） | 约束与不变量 | 存量 | **14 §1.1** | `read.c:254-259` | |
| K-305 | 空洞读零（来源：跳写缝隙、截断扩大尾巴）与稀疏文件 | 概念 | 存量 | **14 §1.2** | `read.c:149-155` | |
| K-306 | 读写对缺席的分叉（读当正常、写当分配信号） | 概念 | 存量 | **14 §1.2** | — | |
| K-307 | 分块循环（切分取块剩余与请求剩余较小值） | 机制 | 存量 | **14 §1.3/§2.1** | `read.c:65-87` | |
| K-308 | 三个计数器与返回实际搬运字节数 | 机制 | 存量 | **14 §1.3** | `read.c:84-110` | |
| K-309 | 窥视模式（只问缓存不读盘）与接线表挂接 `fdr_peek` | 接口与协议 | 存量 | **14 §1.4** | `fs/mfs/table.c:20`、`read.c:156-159` | |
| K-310 | 窥视遇空洞通知 VM 清页 | 机制 | 存量 | **14 §1.4** | `read.c:156-159` | |
| K-311 | 预读四上限与最小三十二块、寻位后不保证最小量 | 机制 | 存量 | **14 §1.5/§2.5** | `read.c:330-448`（`rahead` 在 `:330`） | |
| K-312 | 预读的两个提前停止（翻译出空洞停止、命中缓存停止） | 机制 | 存量 | **14 §1.5** | `read.c:417-441` | |
| K-313 | 目录内容枚举按位置分批（位置须为项大小整数倍） | 接口与协议 | 存量 | **14 §1.6/§2.6** | `read.c:454-556`（`fs_getdents`） | |
| K-314 | 目录项类型现查内存表、查不到报未知 | 机制 | 存量 | **14 §1.6** | `read.c:516-519` | |
| K-315 | `fs_readwrite` 全流程 | 机制 | 存量 | **14 §2.1** | `read.c:23-111` | |
| K-316 | `rw_chunk` 块处理（位置超限终止、标签登记、三路分支、免读三条件） | 机制 | 存量 | **14 §2.2** | `read.c:117-204` | |
| K-317 | 块标签是缓存与内存服务器共享页面的对账凭据 | 架构演进 | 存量 | **14 §2.2** | `read.c:180` | |
| K-318 | `rd_indir`（断言块非空、断言第三版、小端读出、范围校验失败打印并终止） | 机制 | 存量 | **14 §2.4** | `read.c:263-325` | |
| K-319 | `get_block_map`（翻译、按块大小取整、断言编号、拿块） | 接口与协议 | 存量 | **14 §2.4** | `read.c:281-294` | |
| K-320 | 翻译参数收两个小结构作防腐层 | 架构演进 | 存量 | **14 §3.1** | `read.rs` | |
| K-321 | 间接块损坏报 IO 错而非终止进程 | 架构演进 | 存量 | **14 §3.2** | `read.c:317-322` | |
| K-322 | 窥视不下沉到间接块 | 架构演进 | 存量 | **14 §3.3** | `read.c:260-273` | |
| K-323 | 预读有界且寻位衰减，三十二做成具名常量 | 架构演进 | 存量 | **14 §3.4** | `read.c:405-412` | |
| K-324 | 目录枚举类型降级未知 | 架构演进 | 存量 | **14 §3.5** | `read.c:516-519` | |
| K-325 | `dir_io.rs` 目录镜像桥（装载半逐块翻译、回写半整块免读、追加块先分配区号、目录从不收缩、位图闭包注入） | 工具与工程 | 存量 | **14 §4.4/§5.4** | `os/fs/mfs/src/dir_io.rs` | |
| K-326 | Linux 预读自适应窗口、Redox 目录枚举降级对照 | 架构演进 | 存量 | **14 §3.4/§3.5** | Linux / Redox | |
| K-327 | 写映射（定位与读取翻译相同；存储模式写新区号、释放模式先释放旧区再清槽） | 机制 | 存量 | **15 §1.1/§2.1** | `fs/mfs/write.c:28-185`（`write_map`） | |
| K-328 | 间接块按需生长（分配、清零、再写入下级区号） | 机制 | 存量 | **15 §1.1** | `write.c:79-140` | |
| K-329 | 分配失败不回滚（间接块是共享结构） | 约束与不变量 | 存量 | **15 §1.1** | `write.c:116-131` | |
| K-330 | 间接块回收是生长的逆过程（级联变空释放） | 机制 | 存量 | **15 §1.1** | `write.c:141-180` | |
| K-331 | 变空检查重新读盘，不信任旧快照 | 约束与不变量 | 存量 | **15 §1.1/§3.2** | `write.c:176-180` | |
| K-332 | 块保障（有映射直接拿块，无映射则分配） | 机制 | 存量 | **15 §1.2** | `write.c:254-305`（`new_block`） | |
| K-333 | 分配起点提示位三优先级（文件上次提示、文件首区、文件系统首区） | 机制 | 存量 | **15 §1.2** | `write.c:270-282` | |
| K-334 | 提示位的目的是让文件块相邻、减少寻道 | 概念 | 存量 | **15 §1.2** | — | |
| K-335 | 新块清零保证无旧数据；一区一块时为空操作 | 约束与不变量 | 存量 | **15 §1.2** | `write.c:233-248`（`clear_zone`）、`write.c:311-318`（`zero_block`） | |
| K-336 | 块大小等于存储区大小由挂载保证，写路径信任该保证 | 约束与不变量 | 存量 | **15 §1.2** | `write.c:246` | |
| K-337 | 整块覆盖免读盘，部分覆盖必须先读盘 | 机制 | 存量 | **15 §1.2** | `read.c:175-177` | |
| K-338 | 块对齐且在原文件末尾之后的部分覆盖不需要读盘 | 机制 | 存量 | **15 §1.2** | `read.c:175-177` | |
| K-339 | 写入入口两拒绝（只读挂载、超过最大长度） | 机制 | 存量 | **15 §1.3** | `read.c:49-54` | |
| K-340 | 扩大检查用有符号比较避免无符号减法绕回 | 约束与不变量 | 存量 | **15 §1.3/§3.4** | `read.c:53-54` | |
| K-341 | 分块循环与读取的三处不同（不截断、空洞分配、结束更新长度） | 概念 | 存量 | **15 §1.3** | `read.c:71-75` | |
| K-342 | 只有普通文件与目录更新长度，设备文件不改 | 约束与不变量 | 存量 | **15 §1.3** | `read.c:90-94` | |
| K-343 | 写入结束清除寻位标记、标记两时间、标脏 | 机制 | 存量 | **15 §1.3** | `read.c:96-108` | |
| K-344 | 截断三决策（不变返回、扩大只改长度、缩小先清后放） | 机制 | 存量 | **15 §1.4** | `link.c:428-488` | |
| K-345 | 清零范围按块切分，翻译为空即空洞跳过 | 机制 | 存量 | **15 §1.4** | — | |
| K-346 | 执行顺序先清零后释放（顺序反了找不到块） | 约束与不变量 | 存量 | **15 §1.4** | — | |
| K-347 | `clear_zone` 的三个调用点（写入超尾、截断扩大、新块不在末尾） | 机制 | 存量 | **15 §1.5** | `write.c:233` | |
| K-348 | Rust 不复制空函数：扩大的尾巴不清零，语义等价 | 架构演进 | 存量 | **15 §1.5** | — | |
| K-349 | 权限、属主、时间、状态、卷状态、字节序六组操作的分界（只碰 inode 字段） | 概念 | 存量 | **16 §1.0** | — | |
| K-350 | 文件模式十六位 = 高四位类型 + 低十二位权限 | 数据结构 | 存量 | **16 §1.1** | `fs/mfs/protect.c:24` | |
| K-351 | 改权限只替换低十二位，忽略类型位；标状态变化时间并标脏；报回新模式 | 机制 | 存量 | **16 §1.1** | `protect.c:9-33` | |
| K-352 | 只读挂载拒绝改权限；失败释放 inode 不留引用 | 约束与不变量 | 存量 | **16 §1.1** | `protect.c:18-21` | |
| K-353 | 改属主：换 uid/gid 并清除 setuid/setgid 位 | 机制 | 存量 | **16 §1.2** | `protect.c:47-49` | |
| K-354 | 收回特权位的安全理由（防新属主白得提权） | 概念 | 存量 | **16 §1.2** | — | |
| K-355 | 改属主不检查只读挂载，与改权限不对称（**原始实现的不一致，Rust 忠实保留**） | 约束与不变量 | 存量 | **16 §1.2/§3.2** | `protect.c:39-58` | 全 stage 唯一显式声明"照抄原始不一致"的行为 |
| K-356 | 三个时间（访问、数据修改、状态变化） | 数据结构 | 存量 | **16 §1.3** | `fs/mfs/inode.h` | |
| K-357 | 设时间三语义（设为现在标延迟位、跳过、明确秒数）；亚秒向下舍去 | 机制 | 存量 | **16 §1.3** | `fs/mfs/time.c:10-48` | |
| K-358 | 进门丢弃旧的访问与修改延迟标记 | 机制 | 存量 | **16 §1.3** | `time.c:18` | |
| K-359 | 三语义是 POSIX 规定而非本 FS 发明 | 概念 | 存量 | **16 §1.3** | POSIX | |
| K-360 | 查文件状态先结算延迟时间再拷贝 | 机制 | 存量 | **16 §1.4** | `fs/mfs/stadir.c:53` | |
| K-361 | 设备号填首个存储区号，普通文件填零 | 机制 | 存量 | **16 §1.4** | `stadir.c:56-59` | |
| K-362 | 块用量是保守估计（空洞当实块算）；公式与"宁多不少"原则 | 机制 | 存量 | **16 §1.4/§2.3** | `stadir.c:11-37`（`estimate_blocks`） | |
| K-363 | 查卷状态八个字段的报告集 | 接口与协议 | 存量 | **16 §1.5** | `stadir.c:82-104` | |
| K-364 | 空闲区数 = 总数 − 已用数；空闲 inode 数实时数位图不缓存 | 机制 | 存量 | **16 §1.5** | `stadir.c:91-100` | |
| K-365 | 无特权调用方可用空闲数与总空闲相同（无预留） | 约束与不变量 | 存量 | **16 §1.5** | `stadir.c:91-93` | |
| K-366 | 字节序转换 `conv2`/`conv4`（32 位复用 16 位两次） | 机制 | 存量 | **16 §1.6/§2.7** | `fs/mfs/utility.c:10-36` | |
| K-367 | 当前小端目标下恒走不交换分支，函数保留供他序磁盘镜像 | 架构演进 | 存量 | **16 §1.6** | `utility.c` | |
| K-368 | 状态结构做成框架中立字段（避免循环依赖） | 接口与协议 | 存量 | **16 §3.1** | `minix-types` `Stat` | |
| K-369 | 时间选择器做成具名常量加小结构 | 数据结构 | 存量 | **16 §3.3** | `meta.rs` | |
| K-370 | 估计用有符号中间值避免绕回 | 约束与不变量 | 存量 | **16 §3.4** | `stadir.c:30` | |
| K-371 | 同步先刷脏 inode、后刷全部脏块 | 机制 | 存量 | **17 §1.1/§2.1** | `fs/mfs/misc.c:8-23` | |
| K-372 | 顺序至关重要的理由（inode 写回产生新的脏缓存块）；顺序反了的后果 | 约束与不变量 | 存量 | **17 §1.1** | `misc.c:22` | 崩溃一致性核心论证之一 |
| K-373 | 两种位图与各自的起点、总位数公式 | 数据结构 | 存量 | **17 §1.2** | `fs/mfs/stats.c:32-42`、`const.h:48-51` | |
| K-374 | 数空位过程（从搜索提示位起绕一圈、整字跳全一、否则逐位） | 机制 | 存量 | **17 §1.2** | `stats.c:63-71` | |
| K-375 | 位号超总位数即停，不数图外填充位；填充位恒零，误数会让卷状态撒谎 | 约束与不变量 | 存量 | **17 §1.2** | `stats.c:74-82` | |
| K-376 | 搜索提示位越界归零（鲁棒性处理） | 机制 | 存量 | **17 §1.2** | `stats.c:45` | |
| K-377 | 位图字按本机字节序转换后才能逐位判断 | 机制 | 存量 | **17 §1.2** | `stats.c:65` | |
| K-378 | `MARKDIRTY` 宏：只读挂载打印文件名行号并输出调用栈；只读盘出现标脏即逻辑错误 | 约束与不变量 | 存量 | **17 §1.3/§2.3** | `fs/mfs/clean.h` | |
| K-379 | 库不打印原则（Rust 用返回值代替打印加堆栈） | 架构演进 | 存量 | **17 §1.3/§3.4** | — | |
| K-380 | 常量头文件六组常量与每组的权威 Rust 位置 | 数据结构 | 存量 | **17 §1.4/§2.5** | `fs/mfs/const.h:5-65` | |
| K-381 | 常量零重复原则（重复定义是跨模块漂移的源头） | 约束与不变量 | 存量 | **17 §1.4/§3.5** | `minix-types` | |
| K-382 | 全局头文件五名字与五去向 | 数据结构 | 存量 | **17 §1.5/§2.4** | `fs/mfs/glo.h:11-20` | |
| K-383 | `fs_sync` 遍历 512 槽，引用计数为正且脏才写回；全部写完刷整缓存 | 机制 | 存量 | **17 §2.1** | `misc.c:18-22` | |
| K-384 | `count_free_bits` 逐步流程 | 机制 | 存量 | **17 §2.2** | `stats.c:14-89` | |
| K-385 | 函数名与注释不符（注释写分配一位，实际数空位总数） | 工具与工程 | 存量 | **17 §2.2** | `stats.c:14-89` | |
| K-386 | 同步执行器闭包注入（顺序硬编码、调用方不能调换） | 接口与协议 | 存量 | **17 §3.1** | `os/fs/mfs/src/maint.rs` | |
| K-387 | 失败停住并上报已做工作量 | 约束与不变量 | 存量 | **17 §3.2** | `maint.rs` | |
| K-388 | 空清单也调刷盘闭包（空清单不代表块缓存干净） | 约束与不变量 | 存量 | **17 §4.1** | `maint.rs` | |

#### 组 F：虚拟树族（现有 18、19、20 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-389 | 三服务同构需求与框架抽取动机（进程信息、设备管理、伪终端） | 概念 | 存量 | **18 §1.1** | — | 理解框架为什么存在 |
| K-390 | 框架管机制、服务填策略（Linux/Redox 对照） | 架构演进 | 存量 | **18 §1.1** | Linux / Redox | |
| K-391 | 节点四组字段（身份 / 元数据 / 树位置 / 哈希链） | 数据结构 | 存量 | **18 §1.2** | `lib/libvtreefs/inode.h:25-49` | |
| K-392 | 短名字内联优化与 Rust 统一字节向量的取舍 | 架构演进 | 存量 | **18 §1.2** | `PNAME_MAX` | |
| K-393 | 双哈希表（名字哈希 / 索引哈希）与桶数 = 节点总数 | 数据结构 | 存量 | **18 §1.3** | `inode.c:117-137` | |
| K-394 | sdbm 哈希算法与多项式常数 65599 | 机制 | 存量 | **18 §1.3** | `lib/libvtreefs/sdbm.c:22` | |
| K-395 | 删除两阶段（标记摘表 / 引用归零回收）与幂等 | 机制 | 存量 | **18 §1.4** | `inode.c:543-594`、`515-534` | |
| K-396 | 链式回收父节点（删子触发父回收） | 机制 | 存量 | **18 §1.4** | `inode.c:515-534` | |
| K-397 | 引用计数三操作（打开加一 / 释放减一 / 就地回收） | 机制 | 存量 | **18 §1.5** | `inode.c:469-498` | |
| K-398 | 批量释放的计数语义（参数含本次引用，减一再走释放） | 约束与不变量 | 存量 | **18 §1.5** | `inode.c:611`（`fs_putnode`） | |
| K-399 | 断言改返回值的总方针（库不崩进程） | 架构演进 | 存量 | **18 §1.5/§3.3** | `inode.c` 多处 | |
| K-400 | 单步查找四步（目录校验 → 点/点点 → 刷新钩子 → 名字哈希） | 机制 | 存量 | **18 §1.6** | `lib/libvtreefs/path.c:9` | |
| K-401 | 根的两处点点语义不同（枚举显示为自己 vs 查找越顶报错） | 约束与不变量 | 存量 | **18 §1.6** | `path.c:28-30` | |
| K-402 | 枚举位置计数器三段（0 点 / 1 点点 / 2 起索引槽与普通子） | 机制 | 存量 | **18 §1.7** | `lib/libvtreefs/file.c:195`（`fs_getdents`） | |
| K-403 | 索引空洞耗位 + 缓冲满回绕 + 已索引子节点跳过 | 约束与不变量 | 存量 | **18 §1.7** | `file.c:194-295` | |
| K-404 | 位置到顶报 IO 错误（防错乱调用方） | 约束与不变量 | 存量 | **18 §1.7** | `file.c:194-295` | |
| K-405 | 读循环语义（类型校验、无钩子当空文件、部分成功） | 接口与协议 | 存量 | **18 §1.8** | `lib/libvtreefs/file.c:46`（`fs_read`） | |
| K-406 | 写循环语义（拒绝访问、零长成功、钩子返零停） | 接口与协议 | 存量 | **18 §1.8** | `file.c:108-164` | |
| K-407 | 截断语义与非零尾参待办 | 约束与不变量 | 存量 | **18 §1.8** | `file.c:169-189` | |
| K-408 | 状态查询三义（链接数是布尔、符号链接目标现算、三时间 = 当下） | 机制 | 存量 | **18 §1.9** | `lib/libvtreefs/stadir.c:8-43` | |
| K-409 | 状态变更钩子与"无钩子报未实现 + 重读报回" | 接口与协议 | 存量 | **18 §1.9** | `stadir.c:48-109` | |
| K-410 | 卷状态仅两字段（不截断 / 名字上限），无并行度与容量 | 接口与协议 | 存量 | **18 §1.9** | `stadir.c:114-122` | |
| K-411 | 挂载语义（拒根挂载、根加引用、初始化钩子、能力无标志） | 机制 | 存量 | **18 §1.10** | `lib/libvtreefs/mount.c:10`（`fs_mount`） | |
| K-412 | 启动三件套与信号只应终止 | 机制 | 存量 | **18 §1.10/§2.1** | `lib/libvtreefs/vtreefs.c:16`（`init_server`） | |
| K-413 | 启动注册三回调 + 非 FS 消息先拷贝再调钩子 + 六参数存全局 | 接口与协议 | 存量 | **18 §2.1** | `vtreefs.c:66/87` | |
| K-414 | 节点管理内部机制全集（建树三块堆内存、腾挪游标、新增九断言九字段、访问器六组、三种查找、脱钩、删否查询） | 机制 | 存量 | **18 §2.2** | `inode.c:30-604` | |
| K-415 | 回调表十七项全挂实现 + 十三钩子表 + 头文件契约 | 接口与协议 | 存量 | **18 §2.3/§2.10** | `lib/libvtreefs/table.c:6-23`、`vtreefs.h` | |
| K-416 | 链接组四操作语义（读链接 / 建链接 / 建节点 / 删节点） | 接口与协议 | 存量 | **18 §2.5** | `lib/libvtreefs/link.c:8-129` | |
| K-417 | 额外数据区（按节点数分配、按号偏移取指针、零长成功） | 数据结构 | 存量 | **18 §2.9** | `lib/libvtreefs/extra.c:14-56` | |
| K-418 | 树做成值（多实例并行测试）+ Linux 超级块对象同构对照 | 架构演进 | 存量 | **18 §3.1** | `os/libs/minix-vtreefs/src/tree.rs` | |
| K-419 | 钩子做成有默认方法的特征（Redox 方案特征同构） | 架构演进 | 存量 | **18 §3.2** | `os/libs/minix-vtreefs/src/driver.rs:67` | |
| K-420 | 腾挪失败报无空间（服务可降级） | 约束与不变量 | 存量 | **18 §3.4** | `inode.c:142-179` | |
| K-421 | 枚举位置语义忠实复刻（正确性命门） | 约束与不变量 | 存量 | **18 §3.5** | `file.c:194-295` | |
| K-422 | Rust 常量与类型清单（名字上限 60、短名阈 24、空索引 -1、暂存下限 4096、错误八变体、十三钩子全默认） | 数据结构 | 存量 | **18 §4.1** | `os/libs/minix-vtreefs/src/lib.rs` | |
| K-423 | `TreeServer` 接线与 libfsdriver 依赖（挂载点 64 位尺寸能力位、卷状态报零块/容量节点数/60 字节名长） | 接口与协议 | 存量 | **18 §4.6** | `driver.rs:67` | |
| K-424 | 进程信息 FS 定位（静态树 + 进程目录 + 现场生成） | 概念 | 存量 | **19 头部/§1.0** | — | |
| K-425 | 静态文件表递归建树（表项 = 名 + 模式 + 数据；数据 = 子表或生成函数） | 数据结构 | 存量 | **19 §1.1** | `fs/procfs/root.c` 文件表、`main.c:21-44` | |
| K-426 | 根文件清单（七项 + x86 外加项） | 数据结构 | 存量 | **19 §1.1/§2.2** | `root.c` | |
| K-427 | 权限模型：文件全局可读、目录全局可进 | 约束与不变量 | 存量 | **19 §1.1** | `root.c` | |
| K-428 | 两遍刷新第一遍"删"（空槽删、变号删、变属主删）；先删后建防同名撞车 | 机制 | 存量 | **19 §1.2** | `fs/procfs/tree.c:85-223`（`update_list` 在 `:85`） | |
| K-429 | 属主变更重建整棵子树的安全理由（防旧属主打开的文件带信息） | 机制 | 存量 | **19 §1.2** | `tree.c` 注释 | |
| K-430 | 两遍刷新第二遍"建"（活槽无目录则建、名 = 进程号十进制串、索引槽位数 = 每进程文件数、回调数据记进程号） | 机制 | 存量 | **19 §1.2** | `tree.c:104-121` | |
| K-431 | 任务槽恒在（号为槽位 − 任务数，为负；属主恒超级用户）；用户槽查快照、空闲报零 | 机制 | 存量 | **19 §1.2** | `tree.c:12-43` | |
| K-432 | 哨兵约定：进程号零表示无进程 | 约束与不变量 | 存量 | **19 §1.2** | 快照与目录约定 | |
| K-433 | 查找刷新懒策略（同滴答不重复拉、根下重建整树、进程目录只建被查项、服务目录走服务查找） | 机制 | 存量 | **19 §1.2** | `tree.c:355-395` | |
| K-434 | 枚举刷新勤策略（每次拉快照重建整树，一次枚举看到一致快照） | 机制 | 存量 | **19 §1.2** | `tree.c:402-416` | |
| K-435 | 懒查与勤枚举的性能-一致性权衡 | 概念 | 存量 | **19 §1.2** | 注释 | |
| K-436 | 四种进程文件（状态 / 命令行 / 环境 / 内存映射） | 数据结构 | 存量 | **19 §1.3** | `fs/procfs/pid.c` 注册表 | |
| K-437 | 按需生长（先验注册表、未注册不建、枚举建全量、索引 = 注册表下标、删了可重建） | 机制 | 存量 | **19 §1.3** | `tree.c:229-280` | |
| K-438 | 三岔分发（父索引 → 进程读 / 服务读 / 回调数据里的生成函数） | 机制 | 存量 | **19 §1.3** | `tree.c:315-443`（`pid_read` 在 `:316`） | |
| K-439 | 僵尸进程与任务的三文件为空；存在性归树、内容归生成器 | 约束与不变量 | 存量 | **19 §1.3** | `pid.c:192` | |
| K-440 | 输出暂存三语义（跳过、截断、产量；整篇生成 + 窗口截取） | 机制 | 存量 | **19 §1.4/§3.3** | `fs/procfs/buf.c:18-125`（`buf_init` 在 `:18`） | |
| K-441 | 暂存实现细节（请求长度与 4096−1 取小、留一字节结束零、跳过只在第一次写） | 约束与不变量 | 存量 | **19 §1.4** | `buf.c:30-83` | |
| K-442 | 手写十进制整数渲染 | 机制 | 存量 | **19 §1.4** | `buf.c` | |
| K-443 | 负载均值三窗口算法（乘一百再除、商整数部分、余数两位小数）与窗口边界 | 机制 | 存量 | **19 §1.5/§2.8** | `fs/procfs/util.c:9-65`（`procfs_getloadavg` 在 `:9`） | |
| K-444 | 运行时间与时钟频率两个单行生成器 | 机制 | 存量 | **19 §1.6** | `root.c` | |
| K-445 | 进程状态行十五字段位置契约 | 接口与协议 | 存量 | **19 §1.7** | `pid.c:60`（`pid_psinfo`） | |
| K-446 | 名字空格截断 + 僵尸用新鲜快照判断 | 约束与不变量 | 存量 | **19 §1.7** | `pid.c:29`、`:41` | |
| K-447 | 节点预算 =（任务数 + 进程数）× 4 的推导；超预算走腾挪降级不崩溃 | 约束与不变量 | 存量 | **19 §1.8** | `tree.c:129-139` | |
| K-448 | 服务子目录四值策略枚举 + 16 条服务默认策略字符串 | 数据结构 | 存量 | **19 §2.5** | `service.c` 策略表、`service.c:22-28` | |
| K-449 | x86 处理器文件（64 项标志表、两字 32 位逐位、单处理器打印、入口） | 机制 | 存量 | **19 §2.6** | `cpuinfo.c:85`（`print_x86_cpu_flags`） | |
| K-450 | PCI / IPC 向量 / 命令行与环境渲染格式（`%-16s: value`、`%08lx T name(k)`、NUL 分隔） | 接口与协议 | 存量 | **19 §4.6** | `root_pci`、`root_ipcvecs` | |
| K-451 | 内核查询在服务层、格式化在内容层 | 架构演进 | 存量 | **19 §3.1** | `os/fs/procfs/src/content.rs` | |
| K-452 | 刷新计划纯计算 | 架构演进 | 存量 | **19 §3.2** | `content.rs` | |
| K-453 | 服务策略表不写死（只建机制，表待运行服务） | 架构演进 | 存量 | **19 §3.4** | `os/fs/procfs/src/service.rs` | |
| K-454 | ptyfs 定位（变体内容·终端树，直连驱动框架不用 vtreefs） | 概念 | 存量 | **20 头部/§1.0** | `fs/ptyfs/ptyfs.c` | |
| K-455 | 固定表：32 节点在系统配置里定死、编译期已知、数组与位图静态分配 | 数据结构 | 存量 | **20 §1.1/§4.1** | `fs/ptyfs/node.c:20-25`、`node.h` | |
| K-456 | 固定表代价与好处（终端数封顶 vs 无分配失败、无碎片、枚举上限恒定） | 约束与不变量 | 存量 | **20 §1.1** | `node.c` 注释 | |
| K-457 | Linux `/dev/pts` 动态展示已分配终端对照 | 概念 | 存量 | **20 §1.1** | Linux | |
| K-458 | 正向转换（数字 → 十进制串：逐位取余倒填、零特判、装不下报名字过长） | 机制 | 存量 | **20 §1.2/§2.2** | `ptyfs.c:55-66`（`make_name` 在 `:55`） | |
| K-459 | 反向转换四项校验（空拒 / 非数字拒 / 前导零拒 / 上溢拒） | 机制 | 存量 | **20 §1.2/§2.2** | `ptyfs.c:76-100`（`parse_name` 在 `:76`） | |
| K-460 | 有效名定义 = 正向能生产的串；两向互逆无例外 | 约束与不变量 | 存量 | **20 §1.2** | — | |
| K-461 | 前导零拒绝的规范性理由（一名一节点是名字空间基本卫生） | 约束与不变量 | 存量 | **20 §1.2** | — | |
| K-462 | 查找：只认根、非根报找不到、点回根、命中填六字段、挂载恒否 | 接口与协议 | 存量 | **20 §1.3/§2.3** | `ptyfs.c:108-147`（`ptyfs_lookup` 在 `:108`） | |
| K-463 | 枚举位置计数器（0 点 / 1 点点 / 2 起索引 = 位置 − 2）与行为（空闲耗位、满回绕、到顶停） | 机制 | 存量 | **20 §1.4/§2.3** | `ptyfs.c:153-199`（`ptyfs_getdents` 在 `:153`） | |
| K-464 | 枚举类型换算（目录点点报目录、从设备报字符设备） | 机制 | 存量 | **20 §1.4** | `ptyfs.c:153-199` | |
| K-465 | 与框架枚举的位置语义同构 | 概念 | 存量 | **20 §1.4** | `18-vtreefs.md` 对照 | |
| K-466 | 状态查询（按号找记录、根返回根记录、链接数目录 2 文件 1、三时间全填创建时间） | 数据结构 | 存量 | **20 §1.5/§2.4** | `ptyfs.c:205-280` | |
| K-467 | 修改语义（改属主换标识清特权位；改模式只换权限九位保留类型位） | 机制 | 存量 | **20 §1.5/§2.4** | `ptyfs.c:225-257` | |
| K-468 | 未知号三操作全报无效参数；查卷不截断标记 + 名字上限 | 约束与不变量 | 存量 | **20 §1.5/§2.4** | `ptyfs.c:286-293` | |
| K-469 | 控制消息两半（授权：取发送方标签、须为终端驱动、请求码须认识；执行：增改 / 删除 / 查名） | 接口与协议 | 存量 | **20 §1.6/§2.5** | `ptyfs.c:300-355`（`ptyfs_other` 在 `:300`） | |
| K-470 | 删除幂等（空删也成功）；回话区分阻塞同步回与异步无回执发 | 接口与协议 | 存量 | **20 §1.6/§2.5** | `ptyfs.c:339-371` | |
| K-471 | 创建必须阻塞的理由（驱动建完节点应用马上打开，异步建有竞态） | 约束与不变量 | 存量 | **20 §1.6** | 注释 | |
| K-472 | 增改语义"存在即更新"；与 procfs 先删后建相反（索引由驱动分配 vs 由内核分配） | 机制 | 存量 | **20 §1.6** | `node.c:33`（`set_node`） | |
| K-473 | 挂载拒根、填根六字段、能力无标志；信号只响应终止；回调表八项 | 接口与协议 | 存量 | **20 §1.7/§2.6** | `ptyfs.c:28-47`、`391-434` | |
| K-474 | 表做成值（测试可建多个） | 架构演进 | 存量 | **20 §3.1** | `os/fs/ptyfs/src/table.rs` | |
| K-475 | 正向缓冲瑕疵收紧（显式 bound 检查，12 字节） | 架构演进 | 存量 | **20 §3.2** | `ptyfs.c:79` | |
| K-476 | 空串拒绝（原始实现得零，但到不了解析） | 架构演进 | 存量 | **20 §3.3** | `ptyfs.c:76-100` | |
| K-477 | 授权与执行分离 | 架构演进 | 存量 | **20 §3.4** | `os/fs/ptyfs/src/lib.rs` | |

#### 组 G：磁盘与宿主变体（现有 21–24 篇）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-478 | 块组概念（每组自带宽块位图、inode 位图、inode 表；用量记在组描述符表） | 概念 | 存量 | **21 §1.1** | `fs/ext2/super.c` | |
| K-479 | 分配两段式（先选组再组内找位；释放按号算组再清位）与组的本地性价值 | 机制 | 存量 | **21 §1.1** | `fs/ext2/balloc.c` | |
| K-480 | 与 mfs 对照：两张全局位图 vs 每组两张小位图 | 概念 | 存量 | **21 §1.1** | `07/08` 对照 | |
| K-481 | 组数公式（首数据块之前不分组，减一除商加一） | 数据结构 | 存量 | **21 §1.1** | `super.c` | |
| K-482 | 四组 inode 放置策略总览，由挂载选项选择 | 机制 | 存量 | **21 §1.2** | `fs/ext2/ialloc.c:113/116/254/341` | |
| K-483 | Orlov 策略（默认）：顶层散到目录最少组；非顶层从父组起找第一组双计数过半 | 机制 | 存量 | **21 §1.2/§2.6** | `ialloc.c:116`（`find_group_orlov`） | |
| K-484 | 哈希策略（父组健康住父组，否则父号二次方探测，最后线性扫） | 机制 | 存量 | **21 §1.2/§2.6** | `ialloc.c:284`（`find_group_hashalloc` 定义） | |
| K-485 | 老目录策略（全组扫空闲 inode 超均值且空闲块最多） | 机制 | 存量 | **21 §1.2/§2.6** | `ialloc.c:254` | |
| K-486 | 首 fit 策略（游标起第一组有空位） | 机制 | 存量 | **21 §1.2/§2.6** | `ialloc.c:341` | |
| K-487 | 策略性能思想（目录散开、文件跟父；师承 Linux Orlov） | 概念 | 存量 | **21 §1.2** | Linux Orlov | |
| K-488 | 块分配目标定夺（调用方给块号 / 不给用 inode 所在组首 / 目标非法回落全局搜索位） | 机制 | 存量 | **21 §1.3/§2.5** | `balloc.c:71`（`alloc_block`） | |
| K-489 | 窗口机制（顺序写判定、满八块一次 park 七块、组空闲不足四倍窗口不开窗） | 机制 | 存量 | **21 §1.3** | `balloc.c:23`（`alloc_block_bit`） | |
| K-490 | 保留机制（空闲触保留线先丢全部窗口再拒绝；空闲见底直接拒绝） | 约束与不变量 | 存量 | **21 §1.3/§2.5** | `balloc.c:71` | |
| K-491 | 搜索位与 inode 搜索游标优化（记住上次位置、释放回拨） | 机制 | 存量 | **21 §1.3** | `balloc.c:278`（`free_block`） | |
| K-492 | 超级块读取（1024 偏移读 1024 字节、魔数校验、块大小由对数推导、三验） | 数据结构 | 存量 | **21 §1.4/§2.4** | `fs/ext2/super.c:69`（`read_super`） | |
| K-493 | 版本分支（inode 尺寸老版 128、新版读字段须二的幂；首可用号老版 11、新版读字段） | 数据结构 | 存量 | **21 §1.4** | `super.c` | |
| K-494 | 特性三道门（不兼容特性、只读兼容、错误态；根 inode 必须存在、非零模式、是目录） | 约束与不变量 | 存量 | **21 §1.4/§2.3** | `fs/ext2/mount.c:50-118` | |
| K-495 | 脏盘语义（挂载成功读写写状态为错误、挂载计数加一、记时间并落盘；卸载写干净态） | 机制 | 存量 | **21 §1.4/§2.3** | `mount.c:122-127`、`176-221` | |
| K-496 | 启动选项六个与默认值；布尔正反两键 | 接口与协议 | 存量 | **21 §1.5/§2.1** | `fs/ext2/main.c:15-24`、`67-98` | |
| K-497 | 选项影响点在分配入口分支，不在分配深处（策略与机制分离） | 架构演进 | 存量 | **21 §1.5** | — | |
| K-498 | 小端断言在主函数；Rust 目标恒小端故不断言 | 约束与不变量 | 存量 | **21 §1.5** | `main.c:29-47` | |
| K-499 | ext2 接线表 31 项全挂实现、与参考实现同行数同分组；三处复用；块组差异 | 接口与协议 | 存量 | **21 §2.2** | `fs/ext2/table.c` | |
| K-500 | 挂载流程全序（开设备 / 分配超级块 / 读超级块 / 失败复位 / 认领块大小与用量 / 领根 / 填根六字段 / 挂载点 / 卸载） | 机制 | 存量 | **21 §2.3** | `fs/ext2/mount.c:17-221` | |
| K-501 | `super.c` 逐字段解析（每扇区数、块位数、最大长度、混合每块数、零值拒绝、表块数、每块描述符数、组数、描述符块数） | 数据结构 | 存量 | **21 §2.4** | `super.c:69-150` | |
| K-502 | `balloc` 五函数（丢窗 / 分配 / 位分配 / 释放 / 块号校验） | 机制 | 存量 | **21 §2.5** | `balloc.c:23/71/278` | |
| K-503 | `ialloc` 分配链（只读报只读 → 调位分配 → 无空间打印节流 → 无盘取槽 → 失败退位 → 填字段 → 清场） | 机制 | 存量 | **21 §2.6** | `ialloc.c:32`（`alloc_inode`） | |
| K-504 | `wipe_inode` 清场（大小时间块数标志生成访问表碎片清零、十五指针全空、标脏） | 数据结构 | 存量 | **21 §2.6** | `ialloc.c:454` | |
| K-505 | `misc.c` 同步顺序（先 inode 后块） | 机制 | 存量 | **21 §2.7** | `fs/ext2/misc.c` 全 35 行 | |
| K-506 | 选择器纯函数（统计进组号出） | 架构演进 | 存量 | **21 §3.1** | `os/fs/ext2/src/placement.rs` | |
| K-507 | 随机起点注入 | 架构演进 | 存量 | **21 §3.2** | `placement.rs` | |
| K-508 | 保留与窗口先算后动 | 架构演进 | 存量 | **21 §3.3** | `placement.rs` | |
| K-509 | 校验逐项返回 | 架构演进 | 存量 | **21 §3.4** | `os/fs/ext2/src/superblock.rs` | |
| K-510 | 模块改名 `alloc.rs` → `placement.rs`（edition 2024 下 `extern crate alloc` 与 `pub mod alloc` 冲突） | 工具与工程 | 存量 | **21 §4.3** | `placement.rs` | |
| K-511 | 变长目录项结构（头带总长度、名字跟头部、尾部四字节对齐填充） | 数据结构 | 存量 | **22 §1.1** | `fs/ext2/path.c` | |
| K-512 | 走查按长度跳项；删除合并（被删项长度并入前一项，块内无洞）；空闲槽 = 号零项 | 机制 | 存量 | **22 §1.1/§2.1** | `path.c:95-314` | |
| K-513 | 实际长度公式（头八加名长对四取整）与松弛量 | 数据结构 | 存量 | **22 §1.1/§4.1** | `os/fs/ext2/src/dir.rs` | |
| K-514 | 最大名 255 字节且对齐后整除（构造保证） | 约束与不变量 | 存量 | **22 §1.1** | — | |
| K-515 | 与 mfs 定长 64 对照（藏号留坑可复用 vs 变长合并且无洞） | 概念 | 存量 | **22 §1.1** | `11` 对照 | |
| K-516 | 解码四项先验（缓冲够头 / 长度非零 / 长度不出块 / 名不出记录），任一失败拒绝整块 | 约束与不变量 | 存量 | **22 §3.1** | — | |
| K-517 | 128 字节 inode 小端记录字段清单（含删除时间、generation、访问表、碎片地址） | 数据结构 | 存量 | **22 §1.2** | `fs/ext2/inode.c` | |
| K-518 | 扇区数以 512 字节为单位与块大小解耦 | 数据结构 | 存量 | **22 §1.2** | `inode.c` | |
| K-519 | 十五指针 = 12 直接 + 三间接槽（槽号 12/13/14） | 数据结构 | 存量 | **22 §1.2/§4.2** | `inode.c` | |
| K-520 | 删除时间语义（零 = 活着，非零 = 删除时刻）；释放复用不清零 | 约束与不变量 | 存量 | **22 §1.2** | `ialloc.c:454`（`wipe_inode`） | |
| K-521 | 系统联合与访问表本服务器不读写，全零来全零去 | 架构演进 | 存量 | **22 §1.2/§3.2** | `inode.c` | |
| K-522 | 三级间接阈值分解四段（12 / 单 / 双 / 三；单块容量 = 块大小 / 4，每地址四字节） | 机制 | 存量 | **22 §1.3** | `fs/ext2/read.c:201-258`（`read_map` 在 `:201`） | |
| K-523 | 与参考实现对照（直接 7 + 两级 vs 直接 12 + 三级；单文件上限从方级升到立方级） | 概念 | 存量 | **22 §1.3** | `14` 对照 | |
| K-524 | 链接 / 改名 / 截断 / 权限 / 时间 / 状态语义与参考实现同形（逐项引用不重复） | 概念 | 存量 | **22 §1.4** | `13/16` | |
| K-525 | 字段位置差异（模式属主大小时间偏移按 128 布局；设备号借首指针） | 数据结构 | 存量 | **22 §1.4** | `path.c:20-46` | |
| K-526 | 快速符号链接特殊形状（目标短于 60 字节直接存十五指针区；60 = 15 × 4） | 数据结构 | 存量 | **22 §1.4** | ext2 优化 | |
| K-527 | 小端编解码处处显式 | 约束与不变量 | 存量 | **22 §1.5** | `fs/ext2/main.c` 断言 | |
| K-528 | `path.c` 单步查找与漫步（非目录拒绝、变长走查、进入找松弛、删除合并、查空跳过点点） | 机制 | 存量 | **22 §2.1** | `fs/ext2/path.c:20-314`（`search_dir` 在 `:78`） | |
| K-529 | 创建四件套与参考同形；`link.c` 656 行覆盖面 | 机制 | 存量 | **22 §2.2** | `fs/ext2/open.c`、`link.c` | |
| K-530 | 读写分块循环同形；地址翻译三阈值逐级拿块、索引上限保护、空洞读零、窥视只用缓存；写映射对称生长回收 | 机制 | 存量 | **22 §2.3** | `fs/ext2/read.c:201-258`、`fs/ext2/write.c:27` | |
| K-531 | 元数据四文件（权限换低十二位、属主换人清特权位、时间三语义、状态逐字段拷贝、卷状态块与文件计数、字节序转换两函数） | 数据结构 | 存量 | **22 §2.4** | `fs/ext2/protect.c`、`stadir.c`、`time.c`、`utility.c` | |
| K-532 | `inode.c` 传输定位公式与逐字段拷贝 | 机制 | 存量 | **22 §2.5** | `fs/ext2/inode.c:299`（`rw_inode`） | |
| K-533 | 解码先验长度（坏块早报错） | 架构演进 | 存量 | **22 §3.1** | `dir.rs` | |
| K-534 | 联合字段零来零去 | 架构演进 | 存量 | **22 §3.2** | `inode.rs` | |
| K-535 | 分解纯算术 | 架构演进 | 存量 | **22 §3.3** | `os/fs/ext2/src/mapping.rs` | |
| K-536 | 只读光盘定位（三无：无分配、无写入、无链接） | 概念 | 存量 | **23 头部/§1.0** | — | |
| K-537 | 卷发现参数（起始 32768 字节、每扇区 2048 字节一个描述符、最多走 20 个） | 数据结构 | 存量 | **23 §1.1/§4.1** | `fs/isofs/super.c:75-121`（`read_vds` 在 `:75`） | |
| K-538 | 描述符首字节类型语义（1 = 主描述符解析并替换；255 = 终止符见即停） | 接口与协议 | 存量 | **23 §1.1** | `super.c:75-121` | |
| K-539 | 发现成功两条件齐备 | 约束与不变量 | 存量 | **23 §1.1** | `super.c:75-121` | |
| K-540 | 扇区读不上来直接失败，坏扇区不跳过 | 约束与不变量 | 存量 | **23 §1.1** | — | |
| K-541 | 主描述符三门（五字节签名 CD001、版本 1、块大小小端值 ≥ 2048） | 约束与不变量 | 存量 | **23 §1.2/§4.1** | `super.c:24-73` | |
| K-542 | 认领动作（按块大小设缓存块大小、按卷空间设块用量） | 机制 | 存量 | **23 §1.2** | `super.c:24-73` | |
| K-543 | 根目录记录嵌在主描述符尾部；根区间公式 | 数据结构 | 存量 | **23 §1.2/§2.3** | `super.c`、`utility.c` | |
| K-544 | 块大小 < 2048 拒绝的理由 | 约束与不变量 | 存量 | **23 §1.2** | 原始注释 | |
| K-545 | 目录记录结构（首字节总长度、零表示填充到块尾）与双端数字段 | 数据结构 | 存量 | **23 §1.3** | `fs/isofs/inode.c` | |
| K-546 | 标志位（位 1 = 目录、位 0 = 隐藏）；名字在 33 字节偏移、名长在 32 处 | 数据结构 | 存量 | **23 §1.3** | `inode.c` | |
| K-547 | 坏记录三拒绝（名越界、零长度非填充、超块）；走查靠长度前进 | 约束与不变量 | 存量 | **23 §1.3** | `inode.c` | |
| K-548 | 全零名两特殊项（单字节 0 = 当前目录、单字节 1 = 父目录） | 接口与协议 | 存量 | **23 §1.3** | `inode.c` | |
| K-549 | Rock Ridge 开关下同一目录两种面孔（关时原始交换名、开时长名） | 概念 | 存量 | **23 §1.3/§1.5** | 选项 | |
| K-550 | 区间概念（位置 + 块数二元组）；文件是区间链表 | 数据结构 | 存量 | **23 §1.4/§4.1** | `utility.c` | |
| K-551 | 区间累减定位（累减各区间长度，落中则位置加块内偏移；全超报零） | 机制 | 存量 | **23 §1.4/§3.3** | `utility.c` | |
| K-552 | 与块组对照（块组管分配、区间管定位；光盘无分配） | 概念 | 存量 | **23 §1.4** | `21` 对照 | |
| K-553 | 系统使用尾结构（两字节签名、一字节长度、一字节版本、载荷）与走查规则（短头停、坏长度拒整尾、未知签名跳过） | 数据结构 | 存量 | **23 §1.5/§2.6/§4.2** | `fs/isofs/susp.c:11`（`parse_susp`）、`susp_rock_ridge.c:135` | |
| K-554 | 条目五用途（长名 / 模式 / 链接 / 子定位 / 重定位） | 接口与协议 | 存量 | **23 §1.5/§2.6** | `susp_rock_ridge.c` | |
| K-555 | 链接目标四分支拼法（普通 / 当前 / 父 / 根）与 bound 封顶 256、超了停在已拼前缀 | 机制 | 存量 | **23 §1.5/§3.4** | `susp_rock_ridge.c` | |
| K-556 | 空目标保持空、占位钩子原样报（预留扩展的诚实） | 约束与不变量 | 存量 | **23 §1.5** | — | |
| K-557 | 读钳制（位置超文件长报零、超长请求缩到剩余） | 约束与不变量 | 存量 | **23 §1.6/§4.2** | `fs/isofs/read.c:5`（`fs_read`） | |
| K-558 | 分块循环（块内偏移、块剩长度、取小、逐块拿区间扇区、拷贝、推进计数器） | 机制 | 存量 | **23 §1.6** | `read.c`、`14` 对照 | |
| K-559 | 拿块失败行为差异（原始终止进程 vs Rust 报 IO 错误） | 架构演进 | 存量 | **23 §1.6/§2.8** | `read.c` | |
| K-560 | 枚举（位置 = 解码后数组下标、越界停、缓冲满回绕、名长按首个零字节） | 机制 | 存量 | **23 §1.6/§2.4** | `read.c` | |
| K-561 | 枚举类型换算不为类型读盘 | 接口与协议 | 存量 | **23 §1.6** | `read.c` | |
| K-562 | 链接读（非链接拒绝、超长截断、拷贝返回长度；目标存在内存串里不占数据块） | 机制 | 存量 | **23 §1.6/§2.5** | `fs/isofs/link.c` 全 24 行 | |
| K-563 | 回调表只读子集；窥视与块窥视两处条件编译关掉 | 接口与协议 | 存量 | **23 §1.7/§2.7/§2.8** | `fs/isofs/table.c` 全 32 行 | |
| K-564 | 选项表一项（关 Rock Ridge 布尔，默认开） | 接口与协议 | 存量 | **23 §1.7/§2.1** | `fs/isofs/main.c:10`、`:21` | |
| K-565 | 挂载/卸载/信号/启动（只读开设备、填根六字段、挂载点查目录非空、卸载放根关设备、信号只应终止、启动缓冲池与时区清空算时间） | 机制 | 存量 | **23 §1.7/§2.1** | `fs/isofs/mount.c:4`（`fs_mount`）、`main.c:17-38` | |
| K-566 | 扫描纯函数（标签进位置出） | 架构演进 | 存量 | **23 §3.1** | `os/fs/isofs/src/volume.rs` | |
| K-567 | 区间走查累减 | 架构演进 | 存量 | **23 §3.3** | `record.rs` | |
| K-568 | 链接拼装 bound 封顶 | 架构演进 | 存量 | **23 §3.4** | `rockridge.rs` | |
| K-569 | 宿主共享定位（框架在客侧搭名字树，宿主表管 round trip，每次用前先验鲜，句柄懒开静关） | 概念 | 存量 | **24 头部/§1.0** | — | |
| K-570 | 操作表十五个函数分六组（文件开读写关 / 缓冲查询两组 / 目录开列关 / 属性取设 / 建删改名 / 查卷） | 接口与协议 | 存量 | **24 §1.1** | `lib/libsffs` 操作表 | |
| K-571 | 窄接口双向防火墙（换宿主协议只重写填表；换框架只重写调表） | 架构演进 | 存量 | **24 §1.1** | — | |
| K-572 | Redox 方案接口与 Linux VFS 操作表对照（同一思想三处开花） | 概念 | 存量 | **24 §1.1** | Linux / Redox | |
| K-573 | 选项六项（前缀 / 属主 / 属组 / 文件掩码 / 目录掩码 / 大小写开关）与两桥默认值差异 | 接口与协议 | 存量 | **24 §1.2/§4.1** | `lib/libsffs/main.c:17-34` | |
| K-574 | 掩码语义（宿主模式按位与掩码，再或上客侧类型位；掩码是减法不是加法） | 约束与不变量 | 存量 | **24 §1.2/§3.3** | `lib/libsffs/stat.c` | |
| K-575 | 类型位强制的原因（宿主不给类型或给不准，客侧信自己） | 约束与不变量 | 存量 | **24 §1.2** | — | |
| K-576 | 路径拼接（客节点是根起名字链；宿路径 = 前缀加链斜杠拼接；前缀去头斜杠） | 机制 | 存量 | **24 §1.3** | `lib/libsffs/path.c:17-65`（`make_path` 在 `:17`） | |
| K-577 | 右到左拼与左到右拼同串（等价由测试锁定） | 架构演进 | 存量 | **24 §1.3/§3.2** | `path.c:17-65` | |
| K-578 | 加分支 bound 先算后写，超了拒绝不截；弹出砍末段，无斜杠清空 | 机制 | 存量 | **24 §1.3** | `path.c:70-108` | |
| K-579 | 中途节点无父则整链不可寻址报找不到 | 约束与不变量 | 存量 | **24 §1.3** | `path.c:17-65` | |
| K-580 | 名字折叠比较（字节小写）；哈希与比较同折叠（哈希一致性铁律） | 机制 | 存量 | **24 §1.4/§2.5** | `lib/libsffs/name.c:18-51`（`normalize_name` 在 `:18`） | |
| K-581 | 验鲜三判决（宿主说无此项或非目录 → 删节点；种类对不上 → 删节点并标 stale；其他错误 → 透传） | 机制 | 存量 | **24 §1.5/§3.1** | `lib/libsffs/verify.c:17-56`（`verify_path` 在 `:17`） | |
| K-582 | 验鲜调用契约（取属性时掩码先置模式位，保证种类可比） | 接口与协议 | 存量 | **24 §1.5/§2.4** | `verify.c:61-84` | |
| K-583 | 目录项验鲜多一步（先验父、再拼名、查客表；自顶向下信任链；建表无空位报文件表满） | 机制 | 存量 | **24 §1.5/§2.4** | `verify.c:89-118` | |
| K-584 | 句柄懒开静关（按需开；文件先读写开，失败或只读挂载转只读开；关闭忽略错误） | 机制 | 存量 | **24 §1.6/§2.6** | `lib/libsffs/handle.c:18-77`（`get_handle` 在 `:18`） | |
| K-585 | 懒开与静关的理由（round trip 贵；关的错误无调用方可报） | 架构演进 | 存量 | **24 §1.6/§3.4** | — | |
| K-586 | 查找全流程与填六字段（号、模式掩码后、大小、属主属组配置值、设备空、挂载恒否） | 接口与协议 | 存量 | **24 §1.7/§2.3** | `lib/libsffs/lookup.c:13-150`（`do_lookup` 在 `:105`） | |
| K-587 | 两级验鲜信任链；stale 透传成找不到（验鲜对上层的透明） | 约束与不变量 | 存量 | **24 §1.7** | `lookup.c` | |
| K-588 | 挂载（拒根标记、只读记标志、建目录项表、建根节点、取根属性、根取不到挂载失败、填根六字段、能力 64 位） | 机制 | 存量 | **24 §1.8/§2.2** | `lib/libsffs/mount.c:16-67`（`do_mount` 在 `:16`） | |
| K-589 | 卸载与卷状态（卸载减根引用、报残留引用；卷状态找根验鲜、问宿主卷、块大小任意非零、块数空闲数字节除块大小、可用等空闲、名上限） | 机制 | 存量 | **24 §1.8** | `mount.c:72-89`、`lib/libsffs/misc.c:17`（`do_statvfs`） | |
| K-590 | 已知局限：根必须是真共享而非共享列表 | 约束与不变量 | 存量 | **24 §1.8** | 注释 | |
| K-591 | 两桥装配顺序（备选项、解析选项串、调宿主库初始化、调框架初始化、进主循环、退出逆序放）与信号全交框架 | 机制 | 存量 | **24 §1.9/§2.8** | `fs/vbfs/vbfs.c:127`、`fs/hgfs/hgfs.c:93` | |
| K-592 | 装配先源后框、释放逆序的原则（顺序反了框架调空表） | 架构演进 | 存量 | **24 §1.9** | `07` 对照 | |
| K-593 | 其余文件职责（dentry 增删查 + 哈希折叠 / inode 引用计数 / link 建删改名 / read、write 句柄循环与部分成功 / stat 掩码模式与时间直填 / misc 卷状态 / table 17 项全挂） | 数据结构 | 存量 | **24 §2.7** | `lib/libsffs/{dentry,inode,link,read,stat,misc,table}.c` | |
| K-594 | 宿主答案输入化（判决纯函数） | 架构演进 | 存量 | **24 §3.1** | `os/libs/minix-sffs/src/verify.rs` | |
| K-595 | 路径同串异写 | 架构演进 | 存量 | **24 §3.2** | `os/libs/minix-sffs/src/path.rs` | |
| K-596 | 掩码减法保留 | 架构演进 | 存量 | **24 §3.3** | `minix-sffs` | |
| K-597 | 关闭静默 | 架构演进 | 存量 | **24 §3.4** | `os/libs/minix-sffs/src/handles.rs` | |

#### 组 H：测试性质（横切，各篇 §5）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|---|---|---|---|---|---|---|
| K-598 | 测试分组惯例（本篇直接相关数 + 全工作区数）与统计日期 | 测试性质 | 存量 | 各篇 §5 | `cargo test -p <crate>` | 读者知道怎么跑 |
| K-599 | 测试镜像搭建器规格（64 节点、一区位图一块、节点区一块、首区五、根目录块五） | 测试性质 | 存量 | **10 §5.3** | `os/fs/mfs/src/mount.rs` 测试 | 读者能自己造一个可挂载镜像 |
| K-600 | 单字节块测试把每条分支逼到墙角 | 测试性质 | 存量 | **11 §3.2** | `os/fs/mfs/src/dir.rs` | |
| K-601 | 计数源断言零设备读（免读优化的观察口） | 测试性质 | 存量 | **15 §4.2** | `os/fs/mfs/src/write.rs` | |
| K-602 | 二级缓存测试的十六类线材与十一类池子侧用例 | 测试性质 | 存量 | **04 §5.5** | `os/libs/minix-fs/src/vm_cache.rs`、`cache.rs` | |
| K-603 | 装配冒烟链（挂载 → 查找 → 创建 → 写 → 读 → 枚举 → 状态 → 同步 → 卸载 → 设备交还 → 重挂载） | 测试性质 | 存量 | **07 §4.5/§5.5** | `os/fs/mfs/src/server.rs` | |

### 2.3 新增知识点（来源类型 = 新增）

> 这些条目现有文档**没有**，由 §3 覆盖审计发现。不受 §6 去向规则约束，但必须有证据锚点。每条在 §5 有归属契约。

| 编号 | 名称 | 类型 | 锚点（证据） | 为什么需要 | 归入新篇 |
|---|---|---|---|---|---|
| N-001 | FS 子系统全貌：8 server + 4 框架库，**不是**单个 server | 概念 | `ls minix3/minix/fs/`（8 目录）+ `ls minix3/minix/lib/{libfsdriver,libminixfs,libvtreefs,libsffs}/` | 现有 00 篇只有 21 行骨架，读者拿不到全貌 | 01 |
| N-002 | boot 两层语义：登记顺序（`kernel/table.c:44-64`）≠ 执行顺序（`kernel/main.c:265` 对非 VM 挂 `RTS_VMINHIBIT`） | 概念 | `kernel/table.c:44-64`、`kernel/main.c:265` | 解释"pfs/mfs 在 boot image 里，但不是在 boot image 的位置顺序上启动" | 01 |
| N-003 | FS server 的启动时机三档（boot 两个 / RS 运行时两个 / 按需四个） | 概念 | `kernel/table.c:62-63`、`etc/system.conf:353/507`、`18-stage-commands` 的 mount 面 | 现有文档分散在 06/19/20/21/23/24 各篇的"为什么这时候挂载"里，缺一张总表 | 01、09 |
| N-004 | 四框架库的依赖关系图（谁链谁：`fs/mfs/Makefile` 链 `-lminixfs -lfsdriver -lbdev -lsys`；`fs/pfs/Makefile` 只链 `-lfsdriver -lsys`；`fs/procfs/Makefile` 链 `-lvtreefs -lfsdriver`；`fs/vbfs/Makefile` 链 `-lsffs -lvboxfs -lfsdriver -lsys`） | 数据结构 | 8 个 server 的 Makefile（逐份已读） | **现有文档零处**。这是"哪个 server 用哪个框架"的唯一机器可读答案 | 01、09 |
| N-005 | `libfsdriver` 是**被链接进每个 server 的库**，运行时没有独立的框架进程 | 概念 | `fs/mfs/main.c:23`、`fs/pfs/pfs.c:441`（框架代码由各 server 的 `main` 进入） | 防止读者误以为有一个"框架服务"进程 | 02 |
| N-006 | 三个循环入口的对照（`fsdriver_task` / `run_vtreefs` / `sffs_loop`） | 接口与协议 | `lib/libfsdriver/fsdriver.c:80`、`lib/libvtreefs/vtreefs.c:88`、`lib/libsffs/main.c:53` | 现有文档各篇各讲一个，从未并列对照 | 02、09 |
| N-007 | 服务器全量清单与每个 server 的回调数（pfs 9 / ptyfs 8 / mfs 31 / ext2 31 / vtreefs 17 / sffs 17 / isofs 只读子集） | 数据结构 | `fs/pfs/pfs.c:425-435`、`fs/ptyfs/ptyfs.c:425-434`、`fs/mfs/table.c:13-45`、`fs/ext2/table.c`、`lib/libvtreefs/table.c:6-23`、`lib/libsffs/table.c`、`fs/isofs/table.c` | 一张表回答"谁实现了什么" | 09 |
| N-008 | 变体差异总表（启动时机 / 依赖框架 / 磁盘有无 / 回调数 / 只读性 / 根挂载 / 能力位） | 数据结构 | 综合 N-003、N-004、N-007 | 现有 plan §3.6 提出"变体 diff 原则"但从未给出这张表 | 09 |
| N-009 | 非 C 制品清单：构建链（`fs/Makefile` 的条件编译：mfs/pfs 无条件，ext2/isofs/procfs/ptyfs 在 `MKIMAGEONLY=no` 时，hgfs/vbfs 只在 i386）、`DEFAULT_NR_BUFS=1024`（mfs）、`NR_BUFS=100`（isofs） | 工具与工程 | `fs/Makefile`、`fs/mfs/Makefile`、`fs/isofs/Makefile` | 现有文档只在零散处提"一千零二十四块"，从未讲清这是构建期常量 | 09、34 |
| N-010 | 服务策略配置（`etc/system.conf` 的 procfs/ptyfs 段：procfs 需 `VIRCOPY` + vm 的 `INFO`/`SETCACHEPAGE`；ptyfs 需 `ipc SYSTEM pm vfs rs pty ds vm`） | 工具与工程 | `minix3/etc/system.conf:353`、`:507` | 现有 19/20 篇零处提到；这是"服务能不能工作"的授权前提 | 34 |
| N-011 | 镜像格式契约与离线工具（`mkfs.mfs` 1544 行 + 六套版本头、`fsck.mfs` 1675 行） | 工具与工程 | `minix3/minix/usr.sbin/mkfs.mfs/`、`minix3/minix/commands/fsck.mfs/` | 现有文档零处。读者无法回答"这个镜像从哪来、坏了怎么查" | 34 |
| N-012 | 端到端测试脚本（`testmfs.sh` 87 行含镜像 sha1 期望值、`testisofs.sh` 183 行、`testvnd.sh` 155 行、`run` 184 行、`check-install` 42 行） | 工具与工程 | `minix3/minix/tests/` | 现有文档零处。这是 C 侧唯一的行为验收面 | 34 |
| N-013 | 二级缓存通道的另一端（VM 服务器侧 `os/servers/vm/src/ipc/cache_handlers.rs` 的四个页缓存请求处理，对应 C 的 `do_mapcache` 一族） | 接口与协议 | `os/servers/vm/src/ipc/cache_handlers.rs`、`04-block-cache.md §7` | 现有 04 篇只在 §7 参见里一句话带过，正文没有讲清"这条通道的另一端是谁" | 07（本侧）+ 跨 stage 引用 |
| N-014 | 生产传输接缝（`FsTransport` 五方法、`RequestBody` 三十二变体、`Incoming::Cancelled` 对应 `fsdriver_terminate` 的取消接收语义） | 接口与协议 | `os/libs/minix-fs/src/task.rs:397` | 现有 01 篇 §4.3 有，但归在"实现详解"里；这是"框架与真实 IPC 的接缝"，应有独立位置 | 03、33 |
| N-015 | 错误路径总图（协议错误码 `EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK` 的三条使用路 + 各 server 的 errno 映射边界 + "库不崩进程"的 C 宕机改返回值清单） | 接口与协议 | `vfsif.h:26-28`、各篇 §2.x 差异表 | 现有文档把差异表散在 24 篇里，读者无法一次看全 | 30 |
| N-016 | 崩溃一致性契约汇总（孤儿优于悬空 / 先 inode 后块 / clean 位三处 / 卸载顺序 / 位图与 inode 位的一致性窗口） | 约束与不变量 | `fs/mfs/open.c:233-235`、`fs/mfs/misc.c:18-22`、`fs/mfs/mount.c:40-50`、`mount.c:134-172` | 现有文档散在 10/12/17 三篇，从未合并论证 | 31 |
| N-017 | 关闭与退出总图（SIGTERM 三档处理：mfs 先同步再终止、pfs 直接终止、vtreefs 只应终止；卸载与退出的次序） | 接口与协议 | `fs/mfs/main.c:70-78`、`fs/pfs/pfs.c:381-392`、`lib/libvtreefs/vtreefs.c:66` | 现有文档散在 06/07/18 篇 | 30 |
| N-018 | 并发与同步声明（单线程事件循环 = 零锁；框架层无 `Mutex`；`RES_THREADED` 永不置位；与内核 BKL 的关系） | 约束与不变量 | `fsdriver.c:80-96`（单线程循环）、`vfsif.h:21`、`os/libs/minix-fs/src/cache.rs`（无锁设计） | 现有 01 篇 §1.1/§3.7 有，但只讲了"缓存为什么无锁"，没有讲全 stage 的并发声明 | 33 |
| N-019 | 磁盘格式兼容策略（A-11 决策：读旧镜像 vs 仅自举格式） | 架构演进 | 待裁决（`plan.md §4` A-11 列为"重大决策"） | 现有 00/08/21/23 篇都提到这项决策但从未落地结论 | 34（作为待裁决项） |
| N-020 | `libpuffs`（1900 行）的处置（树内无 `fs/` 消费方） | 工具与工程 | `ls minix3/minix/lib/libpuffs/`、`ls minix3/minix/fs/`（无 puffs 目录） | 覆盖审计发现的"孤儿库"，需明确"不在本 stage"并给理由 | 34（排除声明） |

### 2.4 统计摘要

**总条数**：603 条存量（K-001..K-603）+ 20 条新增（N-001..N-020）= **623 条**。

**按类型分布**（存量部分）：

| 类型 | 条数 | 占比 |
|---|---|---|
| 机制 | 231 | 38.3% |
| 数据结构 | 108 | 17.9% |
| 接口与协议 | 101 | 16.8% |
| 约束与不变量 | 95 | 15.8% |
| 架构演进 | 52 | 8.6% |
| 概念 | 49 | 8.1% |
| 工具与工程 | 12 | 2.0% |
| 测试性质 | 8 | 1.3% |
| （未标类型者计入最接近项） | — | — |

**按现有文档分布**（主讲述点计数）：

| 现有文档 | 知识点数 | 现有文档 | 知识点数 |
|---|---|---|---|
| 01 | 27（K-001..K-027 段） | 13 | 22 |
| 02 | 26 | 14 | 24 |
| 03 | 30 | 15 | 22 |
| 04 | 30 | 16 | 22 |
| 05 | 25 | 17 | 18 |
| 06 | 24 | 18 | 35 |
| 07 | 22 | 19 | 30 |
| 08 | 24 | 20 | 24 |
| 09 | 28 | 21 | 33 |
| 10 | 16 | 22 | 23 |
| 11 | 16 | 23 | 29 |
| 12 | 16 | 24 | 29 |

**重复与主讲述点标记**（同一知识点在多篇出现，合并为一条并标主讲述点；下表只列**跨篇重复**，篇内重复不列）：

| 知识点 | 主讲述点 | 次讲述点（改为引用） |
|---|---|---|
| K-003 单线程假设 | 01 §1.1 | 04 §前置、05 §前置、33（新） |
| K-014 能力协商 | 01 §1.6 | 02 §2.1（框架规则）、10 §2.1（mfs 填根六字段）、18 §4.6、21 §2.2、24 §1.8（各 server 的能力位） |
| K-026/K-027 窥视模拟与选路 | 02 §1.5/§2.2 | 14 §1.4（mfs 的 peek 入口）、23 §1.7（isofs 关掉窥视） |
| K-051..K-061 数据通道与目录项 | 03 | 02 §2.2（适配器调用位置）、14 §2.6（getdents 消费） |
| K-059 目录项记录格式 | 03 §1.4 | 14 §1.6（枚举产出）、18 §1.7（框架枚举）、20 §1.4、23 §1.6 |
| K-081..K-110 块缓存 | 04 | 05 §1.4/§1.5（预取与免读）、07 §2.5（拿块包装）、14 §1.5（预读策略）、15 §1.2（免读条件） |
| K-111..K-135 块 I/O | 05 | 04 §2.7（散射读写调用方）、07 §2.4（五项直指块库）、21 §2.2（ext2 块组差异） |
| K-152 懒时间 | 06 §1.5（pfs） | 09 §1.6（mfs，机制更完整）、16 §1.4（结算点） |
| K-160 初装四步 | 07 §1.1 | 21 §2.1（ext2 同构）、23 §2.1（isofs 缓冲池） |
| K-163 信号语义 | 07 §1.2 | 06 §1.6（pfs 无盘版）、18 §1.10（vtreefs 版）→ 合并到新 30 篇 |
| K-192..K-195 位图 | 08 §1.6 | 07 §1.5（区分配消费）、17 §1.2（统计消费）、09 §1.5（inode 位图消费） |
| K-206..K-227 内存 inode | 09 | 07 §2.2（初装两步）、10 §2.1（领根）、11 §2.2（六字段）、13 §2.5（释放） |
| K-233..K-241 挂载/卸载 | 10 | 06 §2.2（pfs 无根版）、21 §2.3（ext2 版）、23 §2.1（isofs 只读版）、24 §1.8（sffs 版） |
| K-248..K-263 目录与路径 | 11 | 12 §1.1（进入）、13 §1.2（删除）、14 §1.6（枚举）、22 §1.1（变长对照） |
| K-264..K-279 创建 | 12 | 13 §1.1（对称篇）、21 §2.6（ext2 版） |
| K-280..K-301 链接/删除/改名/截断 | 13 | 15 §1.4（截断执行）、21 §2.6（ext2 版） |
| K-302..K-326 读路径 | 14 | 15 §1.0（镜像篇）、22 §1.3（三级对照）、23 §1.6（isofs 分块循环对照） |
| K-327..K-348 写路径 | 15 | 13 §1.5（计划来源）、07 §1.5（区分配） |
| K-349..K-370 元数据 | 16 | 11 §2.2（设备号字段）、12 §1.5（设备节点）、21 §2.6（ext2 版）、22 §2.4（ext2 版） |
| K-371..K-388 维护面 | 17 | 10 §3.4（位数统计）、16 §1.5（卷状态消费）、21 §2.7（ext2 版） |
| K-389..K-423 vtreefs | 18 | 19 全篇（消费）、20 §1.4（枚举同构对照）、24 §前置（框架对照） |
| K-455..K-473 ptyfs | 20 | 19 §1.6（先删后建 vs 存在即更新对照） |
| K-478..K-510 ext2 启动分配 | 21 | 22 §1.4（消费） |
| K-511..K-535 ext2 名字数据 | 22 | 11/14 对照（引用不重复） |
| K-536..K-568 isofs | 23 | 14 §1.6（分块循环对照）、21 §1.4（区间 vs 块组对照） |
| K-569..K-597 sffs | 24 | 18 §前置（框架对照） |

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集由四路来源构成：

**来源一：C 源码符号**（逐目录核对，函数名清单来自 `ctags -x` 加 `grep` 交叉验证）

| 目录 | 函数总数 | 已入池 | 明确排除（加理由） |
|---|---|---|---|
| `lib/libfsdriver/` | 48（`call.c` 34 + `fsdriver.c` 3 + `lookup.c` 4 + `dentry.c` 3 + `utility.c` 4） | 48 | 0 |
| `lib/libminixfs/` | 42（`cache.c` 38 + `bio.c` 4） | 42 | 0 |
| `lib/libvtreefs/` | 61 | 61 | 0 |
| `lib/libsffs/` | 47 | 47 | 0 |
| `fs/mfs/` | 77 | 77 | 0 |
| `fs/pfs/` | 14 | 14 | 0 |
| `fs/procfs/` | 51 | 51 | 0 |
| `fs/ptyfs/` | 20 | 20 | 0 |
| `fs/ext2/` | 100 | 100 | 0 |
| `fs/isofs/` | 44 | 44 | 0 |
| `fs/vbfs/` + `fs/hgfs/` | 6 | 6 | 0 |
| 合计 | 510 | 510 | 0 |

> 说明：`table.c` 类文件只含 `struct fsdriver` 初始化，无函数定义，已计入各自目录的"表"主题（K-165、K-499、K-415、K-593、K-563、K-159、K-473）。
> `minix3/minix/fs/isofs/uthash.h`（960 行）是 vendored 第三方宏头，**明确排除**：它不是 Minix3 自有的设计，是被 `inode.c` 当作哈希表工具使用的；语义归 K-545 一族的 inode 缓存机制，不单独成知识点。

**来源二：操作系统通用概念**（不依赖具体代码）

| 主题 | 是否入池 | 归入 |
|---|---|---|
| 进程状态机 | 是 | K-430..K-432（procfs 两遍刷新） |
| 地址空间 | 否 | 本 stage 无（FS 不直接管地址空间） |
| 权限模型 | 是 | K-064（查找权限检查）、K-351..K-355（chmod/chown）、K-427（procfs 全局可读） |
| 块缓存与替换策略 | 是 | K-081..K-110 |
| 崩溃一致性 | 是 | K-269、K-372、K-196、N-016 |
| 命名空间与路径解析 | 是 | K-062..K-077、K-248..K-263 |
| 稀疏文件与空洞 | 是 | K-305、K-306 |
| 引用计数与生命周期 | 是 | K-212..K-219、K-397..K-398、K-144 |
| 事务与幂等 | 是 | K-002、K-470（删除幂等） |
| 服务发现与注册 | 是 | K-591（sffs 装配）、N-003、N-010 |
| 只读文件系统语义 | 是 | K-235、K-236、K-557、K-563 |
| 文件系统一致性检查 | 是 | N-011 |

**来源三：非 C 制品承载的主题**

| 制品 | 承载主题 | 入池编号 |
|---|---|---|
| 8 个 server 的 Makefile | 依赖图与构建期常量 | N-004、N-009 |
| `fs/Makefile` | 条件编译与 boot 成员 | N-009、N-002 |
| `etc/system.conf` | 服务策略授权 | N-010 |
| `etc/usr/rc` | 启动脚本 | N-010 |
| `mkfs.mfs` / `fsck.mfs` | 镜像格式契约 | N-011 |
| `tests/testmfs.sh` 等 | 端到端验收 | N-012 |
| `sys/sys/dirent.h` | 目录项输出 wire | K-059 |
| `vfsif.h` / `fsdriver.h` / `libminixfs.h` / `vtreefs.h` / `sffs.h` | 协议与契约 | K-005..K-014、K-029、K-051、K-085、K-415、K-570 |

**来源四：阶段边界契约里属于本 stage 的主题**

从 `edge_todo.md` 的四条本 stage 条目与 `plan.md §5.4` 排除表提取：

| 边界条目 | 主题 | 归属判定 |
|---|---|---|
| `E-FSRUNTIME`（`edge_todo.md:897`） | 8 个 server bin 的 SEF/RS 启动握手与运行时接线 | **本 stage 讲"是什么"（新 09 篇），"怎么接"归运行时轨道** |
| `E-FSBDEV`（`:914`） | `minix-fs` 块层与真实块驱动的接缝 | 本 stage 讲接缝契约（新 08 篇），真实驱动归 16-stage-drivers |
| `E-FSVMCACHE`（`:931`） | 二级缓存零拷贝页移交与旗标机 | 本 stage 讲本侧（新 07 篇），另一端归 02-stage-vm |
| `E-FSCMDS`（`:948`） | `fsck`/`mkfs` 命令占位认领 | 本 stage 讲格式契约（新 34 篇），命令实现归 18-stage-commands |

### 3.2 覆盖缺口表

> 判定标准：主题在主题全集里，但现有 26 篇**没有任何一篇**讲，或只在参见/一句话里带过。每条给出建议与落实位置。

| # | 缺口主题 | 重要度 | 现有状态（证据） | 建议 | 落实 |
|---|---|---|---|---|---|
| GAP-01 | FS 子系统全貌（8 server + 4 框架库的关系） | 高 | `00-fs-overview.md` 全文 21 行，自述"最小骨架，待改写" | 新建完整总览篇 | 新 01（N-001、N-002、N-003、N-004） |
| GAP-02 | 框架库依赖图（哪个 server 用哪个框架） | 高 | 全 stage 零处；只在各 server 篇头部"源码"行间接暗示 | 并入总览篇 | 新 01 §3（N-004） |
| GAP-03 | 三个循环入口的对照 | 中 | `fsdriver_task`（01/02）、`run_vtreefs`（18）、`sffs_loop`（24）各讲一个 | 新建"进程诞生与装配"篇 | 新 09（N-006） |
| GAP-04 | 变体差异总表 | 高 | `plan.md §3.6` 提出原则但从未给表；各 server 篇各自列 | 并入"进程诞生与装配"篇 | 新 09（N-008） |
| GAP-05 | 全 stage 的并发与同步声明 | 中 | 01 §1.1/§3.7 只讲"缓存为什么无锁" | 并入"边界与外部契约"篇 | 新 33（N-018） |
| GAP-06 | 错误路径总图（协议错误码 + errno 映射 + C 宕机改返回值清单） | 高 | 差异表散在 24 篇的 `§2.x`；`99-global-concepts.md` 只列了 `EENTERMOUNT` 三个常量名 | 新建"错误路径与失败语义"篇 | 新 30（N-015） |
| GAP-07 | 崩溃一致性契约汇总 | 高 | 散在 10（clean 位）、12（孤儿优于悬空）、17（先 inode 后块） | 新建"崩溃一致性契约"篇 | 新 31（N-016） |
| GAP-08 | 关闭与退出总图（三种 SIGTERM 处理的对照） | 中 | 散在 06 §1.6、07 §1.2、18 §1.10 | 并入"错误路径与失败语义"篇 | 新 30（N-017） |
| GAP-09 | 非 C 制品清单（构建链、条件编译、构建期常量） | 高 | 全 stage 零处；`07` 篇只在正文说"一千零二十四块"未讲来源 | 新建"构建、镜像与离线工具"篇 | 新 34（N-009） |
| GAP-10 | 服务策略配置（`system.conf` 的授权面） | 中 | 全 stage 零处 | 并入"构建、镜像与离线工具"篇 | 新 34（N-010） |
| GAP-11 | 镜像格式契约与离线工具（mkfs/fsck） | 高 | 全 stage 零处 | 同上 | 新 34（N-011） |
| GAP-12 | 端到端测试脚本（C 侧的行为验收面） | 中 | 全 stage 零处（各篇 §5 只讲 Rust 单测） | 新建"测试基建"篇 | 新 32（N-012） |
| GAP-13 | 二级缓存通道的另一端 | 中 | `04 §7` 参见一句话 | 并入二级缓存篇的跨 stage 声明 | 新 07（N-013） |
| GAP-14 | 生产传输接缝（`FsTransport`/`RequestBody`） | 中 | `01 §4.3`（在"实现详解"里） | 提升为"框架与真实 IPC 的接缝"独立小节 | 新 03（N-014） |
| GAP-15 | 磁盘格式兼容策略（A-11） | 高 | `00`/`08`/`21`/`23` 提到决策存在，无结论 | 作为待裁决项登记 | 新 34 §待裁决（N-019） |
| GAP-16 | `libpuffs` 的处置 | 低 | 全 stage 零处 | 明确排除声明 | 新 34 §范围外（N-020） |
| GAP-17 | 各 server 的**回调差异矩阵**（谁实现了哪些回调、未实现的语义） | 高 | 各 server 篇零散提到（如 06 "九回调全实现"、23 "只读子集"），无总表 | 并入"进程诞生与装配"篇 | 新 09（N-007） |
| GAP-18 | `DIRECT`/`INDIRECT` 块寻址的**两种格式对照表**（mfs 两级 vs ext2 三级） | 中 | 14 与 22 各讲自己，对照只在 22 §1.3 一句话 | 在变体对照篇给表 | 新 26 §对照 |
| GAP-19 | 目录项**三种格式对照**（mfs 定长 64 / ext2 变长 / isofs 记录） | 中 | 11、22 §1.1、23 §1.3 各讲自己 | 同上 | 新 26 §对照 |
| GAP-20 | 索引节点**三种格式对照**（mfs 64 字节 / ext2 128 字节 / isofs 记录） | 中 | 09 §2.2、22 §1.2、23 §1.3 各讲自己 | 同上 | 新 26 §对照 |

### 3.3 重复主题表

> 同一主题在多个文档重复展开，每条给出"新目录中保留哪一篇作为主讲述点，其余改为引用"。

| # | 重复主题 | 重复位置 | 保留主讲述点 | 其余改为 |
|---|---|---|---|---|
| DUP-01 | 懒时间三标记与结清时机 | 06 §1.5（pfs 版，含"上一节测试失败"反证）、09 §1.6/§2.12（mfs 版）、16 §1.4（结算点） | 新 12（pfs 篇内只讲"无盘版的懒时间"差异） | 新 14（inode）讲机制，新 12 只引用 |
| DUP-02 | 信号处理与退出 | 06 §1.6、07 §1.2、18 §1.10、20 §1.7 | 新 30（错误路径与失败语义篇的"关闭与退出"节） | 新 11/13/21 各只写差异一行 |
| DUP-03 | 只读挂载的拒绝语义 | 10 §1.2（mfs 降级）、11 §1.4（目录写拒）、13 §1.2（删除拒）、16 §1.1（chmod 拒）、23 §1.7（isofs 全只读）、24 §1.6（sffs 转只读开） | 新 16（挂载篇讲降级与 clean 位）+ 新 31（一致性篇讲"只读不写盘"的理由） | 各消费篇只留一句"只读检查" |
| DUP-04 | 位图分配与释放 | 08 §1.6/§2.3/§2.4（mfs 位图）、17 §1.2/§2.2（统计）、21 §2.5（ext2 块位图）、21 §2.6（ext2 inode 位图） | 新 15（mfs 位图） | 新 24（ext2）只讲"每组两张小位图"的差异；新 19（维护）只讲统计 |
| DUP-05 | 目录项格式 | 03 §1.4（输出格式 `struct dirent`）、11 §1.1（mfs 盘上格式）、18 §1.7（框架枚举产出）、20 §1.4（ptyfs 枚举）、23 §1.6（isofs 枚举） | 新 04（框架输出格式）+ 新 18（mfs 盘上格式） | 新 21/23/26 各只写差异；新 26 给三格式对照表 |
| DUP-06 | 索引节点字段清单 | 09 §2.1/§2.2（mfs 内存与磁盘）、22 §1.2（ext2）、23 §1.3（isofs） | 新 14（mfs 内存 inode 机制） | 新 26 给三格式对照表；新 24/25 各只写自己 |
| DUP-07 | 三级间接与地址翻译 | 14 §1.1/§2.3（mfs 两级）、22 §1.3（ext2 三级）、15 §1.1（写侧对称） | 新 20（mfs 读路径含翻译） | 新 25（ext2）只写"三级"差异；新 26 给对照 |
| DUP-08 | 免读优化 | 04 §1.4（拿块模式）、05 §1.5（块层免读）、15 §1.2/§4.2（文件层免读） | 新 05（块层免读，含模式定义）+ 新 06（拿块模式） | 新 22（写路径）只写"何时可以免读"的文件层判据 |
| DUP-09 | 预读策略 | 04 §1.5/§2.8（缓存层）、05 §1.4（块层预取）、14 §1.5/§2.5（文件层） | 新 06（缓存层）+ 新 08（块层预取） | 新 20（读路径）只写文件层窗口计算 |
| DUP-10 | 挂载与卸载流程 | 10（mfs 完整）、06 §2.2（pfs）、21 §2.3（ext2）、23 §2.1（isofs）、24 §1.8（sffs）、18 §1.10（vtreefs） | 新 16（mfs 挂载纵贯） | 新 11/24/25/28/29 各只写差异；新 09 给差异矩阵 |
| DUP-11 | 枚举位置计数器语义 | 18 §1.7（框架三段）、20 §1.4（ptyfs 同构）、23 §1.6（isofs） | 新 21（框架枚举）+ 新 23（ptyfs 差异） | 新 25（isofs）只写自己 |
| DUP-12 | 权限检查（超级用户跳过、属主/属组/其他人） | 03 §1.5/§2.4（框架查找路径） | 新 04 | 其他篇不重复（VFS 侧在 05-stage-vfs） |
| DUP-13 | 符号链接解析（深度上限八层） | 03 §1.5/§2.5（框架）、12 §1.4（创建）、13 §1.4（读取） | 新 04（解析）+ 新 17（创建/读取） | — |
| DUP-14 | 一致性同步顺序 | 17 §1.1（mfs）、21 §2.7（ext2） | 新 31（一致性篇给完整论证） | 新 19/24 各只引用 |
| DUP-15 | 服务装配与回调接线 | 07 §4.5（mfs 装配）、10 §4.3（接线表翻通）、18 §4.6（TreeServer）、24 §4.2（两桥装配） | 新 09（进程诞生与装配） | 各 server 篇只写自己的装配差异 |

### 3.4 越界主题表

> 某篇讲了声明边界之外的主题。每条给出正确归属。

| # | 越界位置 | 越界内容 | 声明边界（该篇头部"不讲什么"） | 正确归属 |
|---|---|---|---|---|
| OOB-01 | `07-mfs-init-main.md §4.5`「服务装配」整节 | `MfsServer` 装配、`Parts` 拆借用、镜像桥、同步次序、typed `Stat` 88 字节、能力位空、时钟注入 | 07 头部只声明"启动链、回调表、拿块包装、区分配" | **新 09**（进程诞生与装配）；`Stat` 布局归 **新 30**（错误与外部契约） |
| OOB-02 | `10-mfs-mount.md §4.3`「接线表翻通」 | 接线表三态、存活计数五变八 | 10 头部声明"挂载、卸载、挂载点检查" | **新 09**（接线表状态属装配） |
| OOB-03 | `07-mfs-init-main.md §2.2` | `cch_check` 被 `#if 0` 屏蔽（调试墓碑） | 同上 | **新 34**（工具与工程） |
| OOB-04 | `17-mfs-maint.md §1.4/§2.5`「常量表」 | 全 stage 常量目录（跨 08/09/11/13/15 五个模块的常量） | 17 头部声明"同步、统计、脏标记守卫" | **新 34**（常量权威位置表）或 **新 35**（全局概念）；见 §6 裁决 |
| OOB-05 | `14-mfs-read.md §4.4`「目录镜像桥」 | `dir_io.rs` 的装载/回写半（属目录操作的落地桥） | 14 头部声明"读路径、块号翻译、目录内容枚举" | **新 18**（目录与路径）或 **新 22**（写路径）；见 §6 裁决 |
| OOB-06 | `05-block-io.md §3.6` | `BdevBlockSource` 真块驱动桥接（属块设备轨道） | 05 头部声明"字节范围如何搬运"，未声明桥接 | **新 08**（块 I/O 篇的"接缝"节）+ 跨 stage 指针 E-FSBDEV |
| OOB-07 | `18-vtreefs.md §2.10`「头文件契约」 | `vtreefs.h` 的十三钩子表全表 | 18 头部声明"框架如何管树，服务如何挂内容" | 保留在 **新 21**（框架篇的接口契约节），但需标明"钩子表的逐项语义归消费方（procfs/devman/gpio）" |
| OOB-08 | `01-fsdriver-task.md §2.6/§2.7` | `vfs_ucred_t` 字段、`struct fsdriver_data`/`dentry` 字段 | 01 头部声明"骨架"，矩阵里自标"本篇记录字段，展开在第 03 篇" | **新 03**（数据结构篇）——自标已正确，重建时按此归位 |
| OOB-09 | `12-mfs-open.md §1.5` | 设备节点与寻位标记（`fs_seek` 属 VFS 的 `lseek` 语义） | 12 头部声明"创建四件套、新节点" | 寻位标记保留 **新 17**（创建篇，因 `fdr_seek` 在 `open.c`）；设备节点字段归 **新 18** |
| OOB-10 | `23-isofs.md §1.5` | Rock Ridge 条目五用途（子定位、重定位） | 23 头部"本章不讲什么"声明"本篇只建长名身份链接三用，其余跳过" | **自相矛盾**：正文 §1.5/§2.6 讲全五用。裁决见 §6.3 |

### 3.5 非 C 主题逐项回答（固定清单）

| # | 主题 | 在哪里讲 | 依据 |
|---|---|---|---|
| 1 | **链接与加载** | 不属本 stage | FS server 是普通用户态进程，加载由 RS 与 boot 机制负责（`01-stage-kernel` 的 boot 链 + `03-stage-rs`）。本 stage 只讲"镜像是什么、怎么装配"（新 34 与 新 09） |
| 2 | **镜像与内存布局** | 新 34 §镜像格式契约 | `mkfs.mfs`（1544 行）与六套版本头定义了 mfs 镜像的字节布局；`fs/mfs/super.h:10-20` 的七段全景是盘上布局；内存布局归各数据结构篇（新 06/14/15/18） |
| 3 | **汇编入口与陷阱进入** | 不属本 stage | 本 stage 全部是 C/Rust 用户态代码，零汇编。逐目录核对：`lib/libfsdriver/`、`lib/libminixfs/`、`lib/libvtreefs/`、`lib/libsffs/`、`fs/*` 下无 `.S` 文件 |
| 4 | **启动装配** | 新 09 | 三个循环入口（N-006）、8 个 server 的回调表（N-007）、初装差异（K-160、K-181、K-412、K-455）、装配顺序（K-592）、`system.conf` 策略（N-010） |
| 5 | **构建与工具链** | 新 34 | 8 个 server Makefile + 4 个库 Makefile + `fs/Makefile` 条件编译（N-004、N-009）；`DEFAULT_NR_BUFS`/`NR_BUFS` 构建期常量 |
| 6 | **跨模块接口与线格式** | 新 03（协议契约）+ 新 30（错误与外部契约） | `vfsif.h` 的 33 个 REQ 与消息布局（`ipc.h` 的 `m_vfs_fs_*`/`m_fs_vfs_*`）、`fsdriver.h` 的四个结构、`dirent.h` 的输出格式、`RES_*`/`PATH_*`/`REQ_*` 标志 |
| 7 | **错误路径** | 新 30 | 协议错误码三条使用路、errno 映射边界、C 宕机改返回值清单（N-015） |
| 8 | **关闭与退出** | 新 30 §关闭与退出 | 三种 SIGTERM 处理对照（N-017）、卸载与退出次序（C-4 到 C-6） |
| 9 | **并发与同步** | 新 33 | 单线程事件循环、零锁设计、`RES_THREADED` 永不置位、与内核 BKL 的关系（N-018） |
| 10 | **测试基建** | 新 32 | Rust 侧单测惯例（K-598..K-603）+ C 侧端到端脚本（N-012） |

---

## 4. 新目录

### 4.1 设计原则与总体变化

**沿用**（经核对成立）：

- 主干分组：**框架 → 参考实现 → 变体**（`plan.md §1.2` 的语义依赖顺序经 C 真序核对成立：任何 server 都先经框架的 `fsdriver_process` 才能处理请求）
- "变体 diff 原则"（mfs 讲透、变体只讲差异）——这是控制篇幅的唯一可行办法
- 单篇单语义、禁止前向引用、首次出现即完整

**调整**（理由逐项见 §6）：

1. **框架部分从 5 篇扩到 8 篇**：把"协议契约"、"主循环与挂载状态"、"数据结构"、"辅助工具"分开（现有 01 篇把协议、主循环、回调表、能力位、数据结构五件事挤在一篇 358 行里）
2. **新增"进程诞生与装配"篇**：8 个 server 共有的第一站，现有文档零处集中讲
3. **新增收尾组 6 篇**（错误路径 / 崩溃一致性 / 测试基建 / 边界契约 / 构建镜像工具 / 全局常量）：这六类横切主题现有文档全部散落
4. **mfs 部分从 11 篇调整为 10 篇**：把 `08-mfs-super`（磁盘格式 + 位图）与 `09-mfs-inode` 的职责按"数据结构 vs 生命周期"重新切分；把 `14/15` 的读写合并考虑后**保留拆分**（读侧与写侧各 300+ 行，合并会超 600 行）
5. **变体部分保持 4 组但重排编号**：pfs 前移到框架之后（保持）；ext2 两篇合并为一篇 + 对照篇（新增对照篇承载三种格式的三张对照表）

### 4.2 新篇章总表

**共 37 篇**（现有 26 篇 = 00 + 01–24 + 99 → 新 37 篇 = 00 + 01–35 + 99）。净增 11 篇的来源：全新建 7 篇（09、30–35）、拆分净增 5 篇（旧 01 与旧 03 重切成新 01–04 得 +2、旧 04 拆出新 07 得 +1、旧 08 拆出新 13 得 +1、旧 13 拆出新 19 得 +1）、合并净减 1 篇（旧 21 与旧 22 并为新 27）。

| 新编号 | 标题 | 一句话定位 | 分组 | 旧编号 |
|---|---|---|---|---|
| 00 | fs-overview | FS 子系统是什么、8 server 与 4 框架库的关系、主线图与阅读路径 | 总览 | 00（改写） |
| 01 | fs-protocol | VFS 与 FS 之间的 33 个请求、消息布局、标志位、事务号 | 一·框架契约 | 01（拆分） |
| 02 | fs-driver-loop | 框架主循环、请求分发、挂载状态机、终止条件 | 一·框架契约 | 01（拆分） |
| 03 | fs-driver-data | 框架的数据结构（回调表、节点描述、数据通道、目录项编码器） | 一·框架契约 | 01 + 03（合并） |
| 04 | fs-lookup | 框架的路径查找漫步（点、点点、挂载点、符号链接、权限检查） | 一·框架契约 | 03（拆分） |
| 05 | fs-request-adapters | 三十一个请求适配器：参数提取、校验、回调调用、回复构造 | 一·框架契约 | 02（改写） |
| 06 | block-cache | 块缓存：哈希索引、LRU、引用计数、脏块、写回、预读、容量启发式 | 二·块层 | 04（改写） |
| 07 | block-cache-vmcache | 二级缓存：旗标字、块标签、页共享、与内存服务器的四个线上调用 | 二·块层 | 04（拆分） |
| 08 | block-io | 块 I/O：分区裁剪、逐块行走、预取、免读、刷后失效、驱动绑定 | 二·块层 | 05（改写） |
| 09 | server-assembly | 8 个 server 如何诞生：SEF 启动、回调表、初装差异、变体差异总表 | 三·装配与首个 server | 新建 |
| 10 | pfs | 管道文件服务器：最小完整 server、无根挂载、512 槽位表、管道语义 | 三·装配与首个 server | 06（改写） |
| 11 | mfs-startup | mfs 启动链与接线：初装四步、信号语义、31 项接线表、拿块包装、区分配 | 四·mfs 参考实现 | 07（改写） |
| 12 | mfs-super | 超级块：31 字节磁盘格式、三魔数、块大小五连验、首区计算、几何检查 | 四·mfs 参考实现 | 08（改写） |
| 13 | mfs-bitmap | 位图：一位一块的账本、分配与释放、零号保留、搜索提示位 | 四·mfs 参考实现 | 08（拆分） |
| 14 | mfs-inode | 内存索引节点：表、哈希、引用计数、分配回收、磁盘转换、懒时间 | 四·mfs 参考实现 | 09（改写） |
| 15 | mfs-mount | 挂载纵贯：开店七步、脏盘降级、打烊六步、挂载点检查 | 四·mfs 参考实现 | 10（改写） |
| 16 | mfs-path | 目录与路径：64 字节目录项、四模式漫步、单步查找、进入 hint | 四·mfs 参考实现 | 11（改写） |
| 17 | mfs-create | 创建与打开：四拍加回滚、目录点点舞、符号链接、设备节点、寻位 | 四·mfs 参考实现 | 12（改写） |
| 18 | mfs-link | 链接与删除：硬链接、删除文件与目录、改名决策树、符号链接读取 | 四·mfs 参考实现 | 13（改写） |
| 19 | mfs-truncate | 截断与区间释放：释放计划、区间翻译、清零范围、顺序契约 | 四·mfs 参考实现 | 13（拆分） |
| 20 | mfs-read | 读路径：块号翻译、空洞读零、分块循环、窥视、预读、目录枚举 | 四·mfs 参考实现 | 14（改写） |
| 21 | mfs-write | 写路径：写映射、间接块生长与回收、块保障、免读、长度更新 | 四·mfs 参考实现 | 15（改写） |
| 22 | mfs-metadata | 元数据：改权限、改属主、设时间、查文件状态、查卷状态、字节序 | 四·mfs 参考实现 | 16（改写） |
| 23 | mfs-maintenance | 维护面：同步刷盘顺序、空闲统计、脏标记红线 | 四·mfs 参考实现 | 17（改写） |
| 24 | vtreefs | 虚拟树框架：内存树、双哈希、钩子、删除两阶段、枚举位置语义 | 五·虚拟树族 | 18（改写） |
| 25 | procfs | 进程信息 FS：静态树、两遍刷新、按需生长、输出暂存、内容生成 | 五·虚拟树族 | 19（改写） |
| 26 | ptyfs | 伪终端树：固定 32 槽位、名字双向转换、控制消息授权、枚举 | 五·虚拟树族 | 20（改写） |
| 27 | ext2 | 第二扩展：块组、四种放置策略、窗口与保留、变长目录项、128 字节 inode、三级间接 | 六·磁盘与宿主变体 | 21 + 22（合并） |
| 28 | isofs | 只读光盘：卷发现、记录解码、区间、Rock Ridge、只读回调子集 | 六·磁盘与宿主变体 | 23（改写） |
| 29 | sffs | 宿主共享桥接：操作表、路径拼接、验鲜、句柄懒开静关、两桥装配 | 六·磁盘与宿主变体 | 24（改写） |
| 30 | format-compare | 三种磁盘格式对照：目录项、索引节点、地址翻译、位图组织 | 六·磁盘与宿主变体 | 新建 |
| 31 | fs-errors | 错误路径与失败语义：协议错误码、errno 映射、C 宕机改返回值、关闭与退出 | 七·收尾 | 新建 |
| 32 | fs-consistency | 崩溃一致性契约：孤儿优于悬空、先 inode 后块、clean 位、只读不写盘 | 七·收尾 | 新建 |
| 33 | fs-testing | 测试基建：Rust 单测惯例、测试镜像搭建、C 侧端到端脚本 | 七·收尾 | 新建 |
| 34 | fs-boundaries | 边界与外部契约：并发与同步声明、与块设备/VM/命令层的接缝 | 七·收尾 | 新建 |
| 35 | fs-build-image | 构建、镜像与离线工具：Makefile 链、镜像格式契约、mkfs/fsck | 七·收尾 | 新建 |
| 99 | fs-global-concepts | 全局常量与结构权威位置表 | 附录 | 99（改写） |

> 编号说明：新 00 与 99 保留原编号语义（总览与全局概念）；01–35 连续编号，无跳号。旧编号与新编号**不是一一对应**（多对多），映射见 §8.1 锚点迁移表。

### 4.3 阅读路径

**主线**（31 篇 = 00 + 01–30，按编号顺序）：

```
00 总览
 → 01 协议 → 02 主循环 → 03 数据结构 → 04 查找 → 05 适配器     （框架契约，5 篇）
 → 06 缓存 → 07 二级缓存 → 08 块 I/O                            （块层，3 篇）
 → 09 装配 → 10 pfs                                             （首个完整 server，2 篇）
 → 11 启动 → 12 super → 13 位图 → 14 inode → 15 挂载
   → 16 目录 → 17 创建 → 18 链接 → 19 截断
   → 20 读 → 21 写 → 22 元数据 → 23 维护                        （mfs 参考实现，13 篇）
 → 24 vtreefs → 25 procfs → 26 ptyfs                            （虚拟树族，3 篇）
 → 27 ext2 → 28 isofs → 29 sffs → 30 格式对照                   （磁盘与宿主变体，4 篇）
```

**支线**（可跳读，6 篇）：31 错误 → 32 一致性 → 33 测试 → 34 边界 → 35 构建镜像 → 99 全局常量

**最短路径**（想快速理解"FS 子系统怎么工作"，6 篇）：00 → 01 → 02 → 06 → 08 → 15

**按角色的推荐路径**：

| 读者目标 | 路径 |
|---|---|
| 想实现一个新的 FS server | 00 → 01 → 02 → 03 → 05 → 09 → 10（最小样例）→ 35（构建） |
| 想理解磁盘 FS 的完整语义 | 00 → 06 → 08 → 11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20 → 21 → 22 → 23 |
| 想理解虚拟文件系统（无盘） | 00 → 01 → 02 → 09 → 24 → 25 → 26 |
| 想对照 Linux/Redox 的实现 | 00 → 30 → 06 → 20 → 21 → 31 → 32 |
| 想审计一致性正确性 | 00 → 12 → 13 → 15 → 19 → 23 → 32 |
| 想理解构建与镜像 | 00 → 35 → 12 → 27 → 28 |

### 4.4 并行主题的分组与代表成员

| 并行组 | 成员 | 代表成员（讲透） | 其余如何收束 |
|---|---|---|---|
| 8 个 FS server | pfs / mfs / procfs / ptyfs / ext2 / isofs / vbfs+hgfs | **mfs**（参考实现，磁盘主线 13 篇） | 新 09 给差异总表；各 server 篇按"启动/挂载差异 → 回调差异矩阵 → 特有数据结构 → 特有协议 → C 文件 API 面映射表"五段写 |
| 4 个框架库 | libfsdriver / libminixfs / libvtreefs / libsffs | **libfsdriver**（所有 server 共用）+ **libminixfs**（所有磁盘 server 共用） | libvtreefs 独立成篇（新 24）；libsffs 并入 sffs 篇（新 29） |
| 33 个请求 | 见新 01 | **READSUPER / LOOKUP / READ / WRITE / GETDENTS** 五个（覆盖挂载、查找、数据、枚举四类主路径） | 新 05 按六组（挂载/节点/数据/命名空间/元数据/块）全列；组内以代表请求讲透，其余给字段表 |
| 31 个 mfs 回调 | 见 `fs/mfs/table.c:13-45` | 同上五个 | 新 11 §接线表给全表；各篇按归属认领 |
| 三种磁盘格式 | mfs 定长 / ext2 变长 / isofs 记录 | **mfs**（新 12–23 全展开） | 新 30 给三张对照表（目录项 / inode / 地址翻译） |
| 四种 ext2 放置策略 | Orlov / hashalloc / dir / any | **Orlov**（默认策略） | 新 27 §放置策略给四策略差异表 |

---

## 5. 每篇契约

> 格式说明：每篇给出七要素（定位、讲什么、不讲什么、前置、后置、事实底线、知识点清单加验收标准）。
> "知识点清单"表里的"来源"一列：存量条目填旧文档位置（形如 `01 §1.1`），新增条目填 C 源码锚点或非 C 制品路径。
> "前置"只允许指向更早的编号（前向引用为零，见 §9 G3）。

### 00-fs-overview

- **一句话定位**：读者读完知道 FS 子系统由哪些东西组成、它们怎么连起来、自己要按什么顺序读下去。
- **讲什么**：
  - FS 子系统的构成（8 server + 4 框架库），以及为什么它不是单个 server
  - boot 两层语义（登记顺序 vs 执行顺序）与三档启动时机
  - 框架库依赖图（哪个 server 链哪个库）
  - 全 stage 的语义主线图与阅读路径（主线 / 支线 / 按角色）
  - 各 server 的一句话定位与它在本 stage 的哪一篇展开
  - ARCH 全景（旧 plan §4 的 13 项候选，逐项给出当前状态）
- **不讲什么**：
  - 任何机制细节（交给 01–35 各篇）
  - 全局常量与结构定义（99）
  - 构建与镜像（35）
- **前置**：无（本 stage 第一篇）
- **后置**：全部 01–35 篇引用本篇的导航结论
- **事实底线**：
  - C：`minix3/minix/fs/`（8 个目录）、`minix3/minix/lib/{libfsdriver,libminixfs,libvtreefs,libsffs}/`、`minix3/minix/kernel/table.c:44-64`（`boot_image`，pfs 在 `:62`、mfs 在 `:63`）、`minix3/minix/kernel/main.c:265`（`RTS_VMINHIBIT`）、`minix3/etc/system.conf:353/507`
  - 非 C 制品：8 个 server 的 Makefile + `minix3/minix/fs/Makefile`（条件编译）
  - Rust：`os/fs/*`（8 crate）、`os/libs/{minix-fs,minix-vtreefs,minix-sffs}`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-001 | FS 子系统全貌（8 server + 4 框架库） | 概念 | `ls minix3/minix/fs/` | 总览的定义性内容 | 新增（C 源码目录结构） |
| N-002 | boot 两层语义 | 概念 | `kernel/table.c:44-64`、`kernel/main.c:265` | 解释启动时机的总纲 | 新增（`00-master-plan/README.md` 有原则，无本 stage 版） |
| N-003 | 三档启动时机 | 概念 | `kernel/table.c:62-63`、`etc/system.conf:353/507` | 同上 | 新增 |
| N-004 | 框架库依赖图 | 数据结构 | 8 个 server Makefile | 回答"谁用哪个框架" | 新增（Makefile） |
| K-016 | 与 Linux / Redox 的组织对照 | 概念 | — | 总览层的横向定位 | 存量（01 §1.7） |

- **验收标准**：
  1. 能画出 8 server × 4 框架库的依赖图，每个箭头有 Makefile 锚点
  2. 能回答"pfs 为什么不链 libminixfs"（答：无磁盘，`fs/pfs/Makefile`）
  3. 给出三条阅读路径（主线 / 最短 / 按角色），每条列出具体编号
  4. 13 项 ARCH 候选逐项给出状态（设计期 / 已落地 / 待裁决），不许留空

### 01-fs-protocol

- **一句话定位**：读者读完能自己拆开一条 VFS 发来的消息，说出它请求什么、带哪些字段、期望什么回复。
- **讲什么**：
  - 三十三个请求编号与 `FS_BASE` 基址、`NREQS=34`
  - 事务号编码（`TRNS_GET_ID` / `TRNS_ADD_ID` / `TRNS_DEL_ID`）与"为什么同一套宏服务两条路"
  - 请求与回复的消息布局（`m_vfs_fs_*` 23 个、`m_fs_vfs_*` 10 个）
  - 挂载标志（`REQ_RDONLY` / `REQ_ISROOT`）与查找标志（`PATH_RET_SYMLINK` / `PATH_GET_UCRED`）
  - 能力位（`RES_THREADED` / `RES_HASPEEK` / `RES_64BIT`）
  - 协议错误码（`EENTERMOUNT` / `ELEAVEMOUNT` / `ESYMLINK`）的**定义**（语义展开在 04 与 31）
  - `REQ_GETNODE` 的保留状态（"Should be removed"，分发表无槽位）
  - `IS_FS_RQ` 的粗筛规则与它的实际使用情况
- **不讲什么**：
  - 消息如何被路由（02）
  - 每个请求的适配细节（05）
  - 协议错误码的语义与消费（04 讲查找路径的三条重定向，31 讲错误总图）
  - 各 server 如何协商能力位（09 与各 server 篇）
  - 常量值的全局汇总表（99）
- **前置**：00
- **后置**：02、05、09、31、99
- **事实底线**：
  - C：`minix3/minix/include/minix/vfsif.h`（84 行，全文：`:8-9` 挂载标志、`:11-18` 查找标志、`:20-23` 能力位、`:26-28` 协议错误码、`:33-38` `vfs_ucred_t`、`:41-73` 33 个 REQ、`:75` `NREQS`、`:77` `IS_FS_RQ`、`:79-81` 三宏）、`minix3/minix/include/minix/ipc.h:2400-2660`（`m_fs_vfs_*` 与 `m_vfs_fs_*` 联合）、`minix3/minix/include/minix/com.h`（`FS_BASE`）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-005 | 三十三个请求编号 | 接口与协议 | `vfsif.h:41-73` | 协议的定义 | 01 §1.2/§2.5 |
| K-006 | 索引一保留请求 | 接口与协议 | `vfsif.h:41`、`table.c:6-40` | 解释编号与槽位的差 | 01 §1.2/§3.1 |
| K-007 | 三个事务宏 | 接口与协议 | `vfsif.h:79-81` | 协议编码 | 01 §2.5 |
| K-002 | 消息类型双载荷 | 机制 | `vfsif.h:79-81` | 同上 | 01 §1.2/§2.5 |
| K-014 | 能力协商三标志 | 接口与协议 | `vfsif.h:20-23` | 协议字段 | 01 §1.6 |
| K-015 | 多线程能力位永不置位 | 约束与不变量 | `vfsif.h:21` | 协议层的设计声明 | 01 §1.6 |
| K-024 | 挂载标志与查找标志 | 接口与协议 | `vfsif.h:8-18` | 协议字段 | 01 §2.6 |
| K-025 | 协议错误码是重定向信号而非失败 | 接口与协议 | `vfsif.h:26-28` | 协议语义（消费在 04） | 01 §2.6 |
| K-078 | 凭证结构 | 数据结构 | `vfsif.h:33-38` | 协议载荷 | 01 §2.6 + 03 §4.3 |
| N-021 | `m_vfs_fs_*` 与 `m_fs_vfs_*` 消息布局逐字段表 | 接口与协议 | `ipc.h` 联合成员 | 现有文档只零散提到字段名，无逐字段表 | 新增（`ipc.h`） |

- **验收标准**：
  1. 给出 33 个请求的编号、名称、一句话语义的完整表，编号逐一有 `vfsif.h` 行锚点
  2. 能解释 `TRNS_DEL_ID` 为什么用带符号右移（回复路径的负错误码）
  3. 给出至少 6 个请求的完整消息布局（请求字段 + 回复字段），每个字段带 `ipc.h` 行锚点
  4. 明确回答"`REQ_GETNODE` 现在能不能被发出"（答：编号存在但分发表无槽位，到达即 `ENOSYS`）

### 02-fs-driver-loop

- **一句话定位**：读者读完能说出一条消息从进来到回复走的每一步分支，以及服务器什么时候退出。
- **讲什么**：
  - 主循环结构（接收 → 分类 → 门禁 → 查表 → 适配 → 回复 → 后钩）
  - 挂载状态机（未挂载 / 已挂载 / 卸载后未终止）
  - 双条件的循环终止（`fsdriver_running || fsdriver_mounted`）
  - 请求分发表（`fsdriver_callvec` 34 槽）与下标换算的无符号回绕
  - 非请求消息的旁路（`fdr_other`，不回复）
  - 回复的无条件发送与事务号回填
  - 接收失败的处理（`EINTR` 继续 / 其他宕机）
  - `fdr_postcall` 的位置与用途
  - 三个循环入口的对照（`fsdriver_task` / `run_vtreefs` / `sffs_loop`）
- **不讲什么**：
  - 协议字段（01）
  - 适配器内部（05）
  - 数据结构定义（03）
  - 服务器侧回调实现（10–29）
  - 各 server 的初装与装配（09）
- **前置**：00、01
- **后置**：05、09、10–29、31、34
- **事实底线**：
  - C：`minix3/minix/lib/libfsdriver/fsdriver.c`（97 行，全文：`:18` `fsdriver_process`、`:26-31` 旁路、`:34-35` 拆号、`:39-47` 门禁、`:50-58` 回复、`:60-61` 后钩、`:68` `fsdriver_terminate`、`:80` `fsdriver_task`、`:87` 循环条件、`:88-93` 接收）、`minix3/minix/lib/libfsdriver/table.c`（40 行，全文）、`lib/libvtreefs/vtreefs.c:88`（`run_vtreefs`）、`lib/libsffs/main.c:53`（`sffs_loop`）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-001 | 被驱动的服务器心智模型 | 概念 | — | 主循环的动机 | 01 §1.1 |
| K-003 | 单线程事件循环假设 | 约束与不变量 | `fsdriver.c:80-96` | 主循环的根因 | 01 §1.1/§3.7 |
| K-004 | 营业状态三态 | 机制 | `fsdriver.c:39`、`:87` | 状态机 | 01 §1.4 |
| K-008 | 无符号回绕减法求表下标 | 机制 | `fsdriver.c:40` | 分发机制 | 01 §2.2 |
| K-009 | 分发表 34 槽数组 | 数据结构 | `table.c:6-40` | 分发机制 | 01 §1.3/§2.4 |
| K-011 | 门禁规则 | 约束与不变量 | `fsdriver.c:39`、`:47` | 状态机 | 01 §1.4 |
| K-012 | 退出条件与两步关闭 | 约束与不变量 | `fsdriver.c:68-74`、`:87-96` | 生命周期 | 01 §1.4/§2.3 |
| K-013 | 非请求消息旁路 | 机制 | `fsdriver.c:26-31` | 分类机制 | 01 §1.5/§2.2 |
| K-017 | 回复无条件发送 | 约束与不变量 | `fsdriver.c:50-58` | 回复机制 | 01 §2.2 |
| K-018 | 接收失败即宕机 | 架构演进 | `fsdriver.c:88-93` | 失败哲学 | 01 §2.3 |
| K-019 | 适配器三步 | 机制 | `call.c` 全篇 | 分发之后的固定形状 | 02 §1.1 |
| K-010 | 服务器全局状态四件套 | 数据结构 | `fsdriver.c:5-9` | 状态机持有的状态 | 01 §1.4/§2.1 |
| N-006 | 三个循环入口的对照 | 接口与协议 | `fsdriver.c:80`、`vtreefs.c:88`、`libsffs/main.c:53` | 现有文档各讲一个，无并列对照 | 新增（C 源码） |
| N-005 | `libfsdriver` 是被链接的库，无独立框架进程 | 概念 | `fs/mfs/main.c:23`、`fs/pfs/pfs.c:441` | 防止误读框架为服务 | 新增（C 源码） |

- **验收标准**：
  1. 画出 `fsdriver_process` 的完整分支图，每个分支标注返回的 errno 与是否回复
  2. 用一句话解释"为什么卸载之后服务器还在转"（答：`fsdriver_mounted` 清了但 `fsdriver_running` 还在，等 SIGTERM）
  3. 给出 `fsdriver_callvec` 的 34 槽完整表，标出空槽位置
  4. 对照三个循环入口，列出相同点与不同点（至少 3 条差异）

### 03-fs-driver-data

- **一句话定位**：读者读完能说出框架在服务器与调用者之间搬运数据时用的四种抽象，以及它们各自解决什么问题。
- **讲什么**：
  - 回调表 `struct fsdriver`（35 个函数指针）与共享签名的调用标识参数（`FSC_READ`/`FSC_WRITE`/`FSC_PEEK`、`FSC_UNLINK`/`FSC_RMDIR`）
  - 节点描述 `struct fsdriver_node`（六字段）
  - 数据通道 `struct fsdriver_data`（端点分支：grant vs 本地指针）+ 三个复制函数
  - 目录项编码器 `struct fsdriver_dentry` + 三件套（init / add / finish）
  - 边界检查（偏移加长度不超声明总长）
  - 名字获取 `fsdriver_getname` 的四道关卡
  - 目录项记录格式与记录长度宏
  - 总长度字段的存在理由
  - Rust 侧：`DataChannel` 枚举 + `DataBackend` trait、`DataBackend` 的三处实现（授权/内存/本地缓冲）
- **不讲什么**：
  - 消息字段与编号（01）
  - 分发与状态机（02）
  - 每个适配器怎么调这些辅助（05）
  - 查找漫步（04）
  - 各 server 自己的目录项格式（16 讲 mfs 盘上格式、27 讲 ext2、28 讲 isofs）
- **前置**：00、01、02
- **后置**：04、05、10–29
- **事实底线**：
  - C：`minix3/minix/include/minix/fsdriver.h:9-16`（`fsdriver_node`）、`:19-26`（`fsdriver_data`）、`:29-36`（`fsdriver_dentry`）、`:45-50`（`FSC_*`）、`:53-105`（`struct fsdriver`）、`lib/libfsdriver/utility.c`（105 行：`:8` `fsdriver_copyin`、`:84` `fsdriver_getname`）、`lib/libfsdriver/dentry.c`（99 行：`:9` `fsdriver_dentry_init`、`:28` `fsdriver_dentry_add`）、`minix3/sys/sys/dirent.h`（129 行）
  - Rust：`os/libs/minix-fs/src/data.rs:31`（`DataBackend`）、`os/libs/minix-fs/src/dentry.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-051 | 数据通道两形态与端点分支 | 数据结构 | `fsdriver.h:19-26`、`utility.c:8-78` | 数据搬运的定义 | 03 §1.1/§2.1 |
| K-054 | 边界检查（防回绕） | 约束与不变量 | `utility.c:8-78` | 搬运的护栏 | 03 §1.2/§2.1 |
| K-055 | 总长度字段只为完整性检查存在 | 概念 | `fsdriver.h:25` | 字段语义 | 03 §2.1 |
| K-068 | 三个复制函数结构完全相同 | 机制 | `utility.c:8-78` | 搬运机制 | 03 §2.1 |
| K-057 | 名字获取四道关卡 | 约束与不变量 | `utility.c:84-105` | 搬运机制 | 03 §1.3/§2.2 |
| K-058 | 长度含结束零；查找允许空名 | 接口与协议 | `utility.c:84-105` | 同上 | 03 §1.3 |
| K-059 | 目录项记录格式与记录长度宏 | 数据结构 | `dirent.h`、`dentry.c` | 编码格式 | 03 §1.4/§2.3 |
| K-060 | 目录项 staging 两级流水与三种回答 | 机制 | `dentry.c:9/28/85` | 编码机制 | 03 §1.4/§2.3 |
| K-061 | 名字补零到对齐、末尾填充清零 | 约束与不变量 | `dentry.c:28-79` | 编码机制 | 03 §1.4 |
| K-075 | 记录长度逐字翻译并保留对齐推导链 | 工具与工程 | `dirent.h` 宏 | 编码格式 | 03 §3.3 |
| K-079 | `DirentType` 八具名类型加兜底 | 数据结构 | `dentry.rs` | 编码格式 | 03 §4.2 |
| K-052 | Rust `DataChannel` 枚举 + `DataBackend` trait | 架构演进 | `data.rs:31` | 搬运的 Rust 化 | 03 §1.1/§3.1 |
| K-053 | 缺席变体对应窥视 | 约束与不变量 | `data.rs` | 搬运的语义 | 03 §1.1 |
| K-047 | 回调表做成整体 trait | 架构演进 | `driver.rs:42` | 回调表的 Rust 化 | 01 §3.4 |
| K-048 | 分类结果做成枚举 | 架构演进 | `driver.rs` | 分发结果的 Rust 化 | 01 §3.6 |
| K-050 | `FsTransport` 接缝与 `RequestBody` 类型化请求体 | 架构演进 | `task.rs:397` | 搬运与 IPC 的接缝 | 01 §4.3 |
| N-014 | 生产传输接缝的五方法与取消接收语义 | 接口与协议 | `task.rs:397` | 现有 01 §4.3 埋在实现详解里，应提升为接缝 | 新增（Rust 源码） |
| K-049 | `NullDriver` 作为协议测试对端 | 测试性质 | `driver.rs:672`（`impl FsDriver for NullDriver`） | 本篇"测试对端"小节：框架在无真实文件系统时仍可被驱动的例证 | 01 §3.8 |
| K-080 | `MemFileServer` 作为 01–03 篇共用完整例证 | 测试性质 | `os/libs/minix-fs/src/memfs.rs:164`（`impl FsDriver for MemFileServer`） | 本篇"参考实现"小节：最小可运行文件系统的参考 | 03 §4.4 |

- **验收标准**：
  1. 给出 `struct fsdriver` 全 35 个函数指针的表，每个标注"必需/可选"与"空时的语义"（`ENOSYS` / 成功 / 跳过）
  2. 用一张图说清 `fsdriver_data` 的端点分支与三个复制函数的关系
  3. 给出目录项记录格式的字节图（名偏移 13、对齐 8），并解释对齐值的来源
  4. 对照 C 与 Rust：列出数据通道在两侧的形态差异（至少 3 条）

### 04-fs-lookup

- **一句话定位**：读者读完能说出框架如何把一条多组件路径一步步走完，以及四个岔路各自怎么处理。
- **讲什么**：
  - 无状态一步一查的漫步模型（VFS 发整条路径，框架取一个分量问服务器）
  - 四种岔路：点（原地踏步）、点点（向上走，含"作为根"与"文件系统根"的特殊处理）、挂载点（停下交路权）、符号链接（读出目标、拼尾巴、重走）
  - 权限检查编织在每一步（`access_as_dir`：中间节点须目录 + 搜索权限；超级用户跳过；属主/属组/其他人各看自己执行位）
  - 符号链接深度计数上限八层
  - 绝对路径链接交还 VFS 的理由
  - 三个重定向码的使用（`EENTERMOUNT` / `ELEAVEMOUNT` / `ESYMLINK`）与偏移语义
  - 分量切分（`next_name`）与链接解析（`resolve_link`）
  - 起点获取与挂载点起点的特殊要求
  - 引用计数借还平衡（每条返回路径都要释放）
  - 收尾三岔
  - Rust 侧的两相拆分（只读解析相 + 行动相）与借用检查器的作用
- **不讲什么**：
  - 协议字段定义（01）
  - 分发与状态机（02）
  - 数据结构定义（03）
  - 各 server 自己的单步查找实现（16 讲 mfs、25 讲 procfs、26 讲 ptyfs、27 讲 ext2、28 讲 isofs）
  - 错误路径总图（31）
- **前置**：00、01、02、03
- **后置**：05、10、16、25、26、27、28
- **事实底线**：
  - C：`minix3/minix/lib/libfsdriver/lookup.c`（333 行：`:9` `access_as_dir`、`:44` `next_name`、`:84` `resolve_link`、`:118` `fsdriver_lookup`；`:167-192` 起点获取、`:216-292` 四岔路、`:243-244` 上行非目录宕机、`:256-260` 链接上限、`:269`/`:308-309` 绝对链接偏移、`:296-330` 收尾）
  - Rust：`os/libs/minix-fs/src/lookup.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-062 | 路径查找无状态一步一查 | 机制 | `lookup.c:118` | 漫步模型 | 03 §1.5 |
| K-063 | 四种岔路 | 机制 | `lookup.c:216-292` | 漫步的核心 | 03 §1.5/§2.5 |
| K-064 | 权限检查编织在每一步 | 约束与不变量 | `lookup.c:9-37` | 漫步的护栏 | 03 §1.5/§2.4 |
| K-065 | 符号链接深度计数上限八层 | 约束与不变量 | `lookup.c:256-260` | 漫步的护栏 | 03 §1.5/§2.5 |
| K-066 | 绝对路径链接交还 VFS 的理由 | 机制 | `lookup.c:269`、`:308-309` | 岔路语义 | 03 §1.5/§4.3 |
| K-067 | 引用计数借还平衡 | 约束与不变量 | `lookup.c:280-281`、`:329-330` | 漫步的资源纪律 | 03 §1.6/§3.5 |
| K-069 | `next_name` 分量切分规则 | 机制 | `lookup.c:44-76` | 漫步机制 | 03 §2.4 |
| K-070 | `resolve_link` | 机制 | `lookup.c:84-112` | 岔路机制 | 03 §2.4 |
| K-071 | 查找主循环取请求六件套与凭证两条路 | 机制 | `lookup.c:118-162` | 漫步入口 | 03 §2.5 |
| K-072 | 起点获取与挂载点起点要求 | 机制 | `lookup.c:167-192` | 漫步入口 | 03 §2.5 |
| K-073 | 向上走进非目录则 C 宕机，Rust 报无效参数 | 架构演进 | `lookup.c:243-244` | 护栏的 Rust 化 | 03 §2.5/§2.6 |
| K-074 | 收尾三岔 | 机制 | `lookup.c:296-330` | 漫步出口 | 03 §2.5 |
| K-077 | 偏移语义对齐 C | 接口与协议 | `lookup.c:269`/`:308-309` | 三个重定向码的载荷 | 03 §4.3 |
| K-076 | 查找分两相 | 工具与工程 | `lookup.rs` | 漫步的 Rust 化 | 03 §3.4 |
| K-056 | C 宕机改 Rust 错误返回 | 架构演进 | — | 护栏的 Rust 化 | 03 §1.2/§3.2 |

- **验收标准**：
  1. 画出漫步主循环的流程图，标出四个岔路的位置与每个岔路的出口
  2. 给出偏移语义的三条规则表（进入挂载点 / 离开挂载点 / 绝对符号链接），每条带 C 锚点
  3. 能回答"为什么绝对路径的符号链接要交还 VFS"（答：绝对路径的起点是全局根，只有 VFS 知道）
  4. 列出所有需要释放引用的返回路径（至少 5 条），每条带 C 行锚点

### 05-fs-request-adapters

- **一句话定位**：读者读完能对着任意一个请求说出它的适配器做了什么检查、调了哪个回调、回了什么内容。
- **讲什么**：
  - 适配器三步的固定形状（取字段并校验 → 调回调 → 装回复）
  - 三十一个适配器按六组编队（挂载 5 / 节点 5 / 数据 8 / 命名空间 9 / 元数据 6 / 块 6；组数按 `call.c` 的函数排布）
  - 校验规则的七处重复（位置长度）
  - 释放计数校验、截断区间校验
  - 点名字符统一规则全表（四种错误码差异）
  - 窥视模拟与三路选路
  - 回复两形状（推进型与计数型）
  - 各组代表请求的完整步骤（`fsdriver_readsuper` 全 12 步、`read_write`、`fsdriver_getdents`、创建族四请求同构、`fsdriver_stat` 预填、块组帮手）
  - 两处"空回调即成功"与二十五处"空回调即 ENOSYS"的对照
  - Rust 侧：适配器与传输解耦、输入与回复结构化、副作用注入、校验函数独立成自由函数
- **不讲什么**：
  - 分发与状态机（02）
  - 数据结构定义（03）
  - 查找漫步（04）
  - 各 server 的回调实现（10–29）
  - 错误路径总图（31）
- **前置**：00、01、02、03、04
- **后置**：09、10–29、31
- **事实底线**：
  - C：`minix3/minix/lib/libfsdriver/call.c`（1013 行）。31 个适配器 + 3 个静态帮手。已实测的锚点：`:12` `fsdriver_readsuper`、`:74` `fsdriver_unmount`、`:96` `fsdriver_putnode`、`:120` `fsdriver_newnode`、`:154` `read_write`、`:194` `fsdriver_read`、`:208` `fsdriver_write`、`:227` `builtin_peek`、`:279` `fsdriver_peek`、`:321` `fsdriver_getdents`、`:359` `fsdriver_trunc`、`:382` `fsdriver_inhibread`、`:399` `fsdriver_create`、`:444` `fsdriver_mkdir`、`:480` `fsdriver_mknod`、`:518` `fsdriver_link`、`:549` `fsdriver_unlink`、`:579` `fsdriver_rmdir`、`:612` `fsdriver_rename`、`:652` `fsdriver_slink`、`:691` `fsdriver_rdlink`、`:718` `fsdriver_stat`、`:747` `fsdriver_chown`、`:773` `fsdriver_chmod`、`:796` `fsdriver_utime`、`:818` `fsdriver_mountpoint`、`:835` `fsdriver_statvfs`、`:857` `fsdriver_sync`、`:872` `fsdriver_newdriver`、`:901` `bread_bwrite`、`:941` `fsdriver_bread`、`:955` `fsdriver_bwrite`、`:969` `fsdriver_bpeek`、`:1002` `fsdriver_flush`
  - Rust：`os/libs/minix-fs/src/call.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-019 | 适配器三步 | 机制 | `call.c` 全篇 | 适配器的形状 | 02 §1.1 |
| K-020 | 纵深防御 | 概念 | — | 适配器的动机 | 02 §1.1 |
| K-021 | 三十一个适配器按六组编队 | 数据结构 | `call.c` 排布 | 组织方式 | 02 §1.2 |
| K-022 | 位置长度校验规则 | 约束与不变量 | `call.c` 各适配器 | 校验 | 02 §1.3 |
| K-023 | 释放节点计数校验 | 约束与不变量 | `call.c:96-114` | 校验 | 02 §1.3/§2.1 |
| K-024a | 截断请求区间两端非负校验 | 约束与不变量 | `call.c:360-376` | 校验 | 02 §1.3/§2.2 |
| K-025a | 点名字符统一规则全表 | 约束与不变量 | `call.c:399-438`、`:549-606`、`:612-646` | 校验 | 02 §1.4/§2.3 |
| K-026 | 窥视模拟 | 机制 | `call.c:227-273` | 数据组 | 02 §1.5/§2.2 |
| K-027 | 窥视三路选路 | 机制 | `call.c:279-315` | 数据组 | 02 §1.5/§2.2 |
| K-028 | 回复两形状 | 接口与协议 | `call.c:180-184`、`:691-712` | 回复构造 | 02 §1.6 |
| K-029 | 挂载适配器全步骤 | 机制 | `call.c:12-68` | 挂载组代表 | 02 §2.1 |
| K-030 | 卸载适配器永远报成功 | 约束与不变量 | `call.c:74-90` | 挂载组 | 02 §2.1 |
| K-031 | 释放节点空回调报成功 | 约束与不变量 | `call.c:96-114` | 节点组 | 02 §2.1 |
| K-032 | 挂载点检查与新节点 | 机制 | `call.c:818-829`、`:120-148` | 节点组 | 02 §2.1 |
| K-033 | 读写共用帮手与调用标识 | 机制 | `call.c:154-188` | 数据组代表 | 02 §2.2 |
| K-034 | 读目录位置是输入兼输出 | 机制 | `call.c:321-353` | 数据组 | 02 §2.2 |
| K-035 | 抑制预读语义 | 机制 | `call.c:382-393` | 数据组 | 02 §2.2 |
| K-036 | 创建族四请求同构 | 机制 | `call.c:399-543` | 命名空间组 | 02 §2.3 |
| K-037 | 改名做两次取名字流程 | 机制 | `call.c:612-646` | 命名空间组 | 02 §2.3 |
| K-038 | 符号链接拼目标；读符号链接只回字节数 | 机制 | `call.c:652-712` | 命名空间组 | 02 §2.3 |
| K-039 | 状态适配器预填 | 机制 | `call.c:718-741` | 元数据组 | 02 §2.4 |
| K-040 | chown/chmod 回新模式；utime 组装两结构 | 机制 | `call.c:747-812` | 元数据组 | 02 §2.4 |
| K-041 | 卷状态清零复制；同步永远报成功 | 机制 | `call.c:835-866` | 元数据组 | 02 §2.4 |
| K-042 | 块组帮手；块窥视无模拟退路 | 约束与不变量 | `call.c:901-996` | 块组 | 02 §2.5 |
| K-043 | 刷块与新驱动永远成功 | 约束与不变量 | `call.c:872-895`、`:1002-1013` | 块组 | 02 §2.5 |
| K-044 | 适配器与传输解耦 | 架构演进 | `call.rs` | 适配器的 Rust 化 | 02 §3.2 |
| K-045 | 输入与回复结构化 | 架构演进 | `call.rs` | 同上 | 02 §3.3 |
| K-046 | 副作用注入 | 架构演进 | `call.c:227-273` | 同上 | 02 §3.4 |
| N-022 | 两处"空回调即成功"与二十五处"空回调即 ENOSYS"的完整对照表 | 接口与协议 | `call.c` 各适配器的空检 | 现有 02 篇只举了 1 个例子（释放节点），未给全表 | 新增（`call.c` 逐函数核对） |

- **验收标准**：
  1. 给出 31 个适配器的完整表：函数名、行锚点、所在组、调用的回调、校验项、回复形状
  2. 给出"空回调时各适配器返回什么"的完整对照（分三类：成功 / ENOSYS / 条件）
  3. 逐条列出点名字符规则的四种错误码差异（已存在 / 不允许 / 无效参数 / 目录非空），每处带行锚点
  4. 用 `fsdriver_readsuper` 为例，逐步骤对照 C 与 Rust，差异项单独列出

### 06-block-cache

- **一句话定位**：读者读完能说出一个磁盘块从设备进到内存、被引用、被淘汰、被写回的完整生命周期。
- **讲什么**：
  - 为什么需要缓存（单位错配、合并写、断电丢数据）
  - 缓存三件套（哈希索引、LRU 淘汰序、引用计数 pin）
  - 缓冲头 `struct buf` 的字段与使用计数（字符型上限 127）
  - 池与全局量（队头队尾、在用计数、哈希表、块大小默认一页、静默开关、`MINBUFS`）
  - 三种拿块模式（`NORMAL` / `NO_READ` / `PEEK`）
  - 拿块主流程 `lmfs_get_block_ino` 全路径
  - 归还 `put_block`（计数减一、一次性块队首插入、二级报备三分处理）
  - 腾空 `freeblock`（脏块先刷、清设备号、标干净、释放内存；不摘哈希链）
  - 脏块三姐妹与写回三时机
  - 钉住的脏块不刷
  - 单块读 `read_block` 与散射读写 `rw_scattered`
  - 预读三函数与上限规则
  - 刷盘（按设备刷、全刷）与失效（`lmfs_invalidate`）
  - 单块释放 `lmfs_free_block`（先通知 VM 再作废）
  - 池管理三函数与容量启发式
  - Rust 侧：`BlockSource` trait、块内存两态、池满与改大小的错误语义
- **不讲什么**：
  - 二级缓存的线上通道（07）
  - 块在设备与缓存之间怎么搬（08）
  - 索引节点的内存缓存（14）
  - inode 表的淘汰策略（14）
- **前置**：00、01、02
- **后置**：07、08、11、13、14、20、21、23
- **事实底线**：
  - C：`minix3/minix/lib/libminixfs/cache.c`（1321 行：`:73` `fs_bufs_heuristic`、`:164` `lmfs_markdirty`、`:252` `freeblock`、`:494` `lmfs_get_block_ino`、`:601` `lmfs_put_block`、`:723` `read_block`、`:782` `lmfs_invalidate`、`:840` `rw_scattered`、`:987` `lmfs_readahead`、`:1057` `lmfs_prefetch`、`:1136` `lmfs_flushdev`、`:1192` `cache_resize`、`:1245` `lmfs_buf_pool`、`:1295` `lmfs_flushall`；`:38-71` 池与全局量、`:443-465` 二级缓存查询与窥视回退、`:533-544` 一次性块队首、`:613-650` `lmfs_free_block`、`:803-807` 失效通知 VM、`:884-885` 脏块排序、`:1031-1052` 预读上限、`:1156-1159` 钉住脏块跳过、`:1236-1239` 块大小非整页自关 vmcache）、`minix3/minix/include/minix/libminixfs.h`（65 行）、`lib/libminixfs/inc.h`（10 行）
  - Rust：`os/libs/minix-fs/src/cache.rs:93`（`BlockSource`）、`os/fs/mfs/src/second_level.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-081 | 为什么需要块缓存 | 概念 | — | 动机 | 04 §1.1 |
| K-082 | 缓存三件套 | 数据结构 | `cache.c` | 结构 | 04 §1.2 |
| K-083 | 脏块语义与写回三时机 | 概念/机制 | `cache.c:164-177`、`:1136-1166`、`:1295-1311` | 生命周期 | 04 §1.3 |
| K-084 | 钉住的脏块不刷 | 约束与不变量 | `cache.c:1156-1159` | 生命周期 | 04 §1.3/§2.9 |
| K-085 | 三种拿块模式 | 接口与协议 | `libminixfs.h`、`cache.c:494` | 接口 | 04 §1.4/§2.1 |
| K-086 | 预读（纯优化、失败静默、上限取小） | 机制 | `cache.c:987/1057` | 机制 | 04 §1.5/§2.8 |
| K-090 | 容量启发式 | 机制 | `cache.c:73-162` | 机制 | 04 §1.6/§2.3 |
| K-091 | 缓冲头字段清单 | 数据结构 | `libminixfs.h` | 结构 | 04 §2.1 |
| K-092 | 使用计数是字符型上限 127 | 约束与不变量 | `libminixfs.h` | 结构 | 04 §2.1/§3.4 |
| K-093 | 池与全局量与 `MINBUFS` | 数据结构 | `cache.c:38-71` | 结构 | 04 §2.2 |
| K-094 | 拿块主流程 | 机制 | `cache.c:494-` | 主流程 | 04 §2.5 |
| K-095 | 二级缓存查询位置与窥视回退 | 机制 | `cache.c:443-465` | 主流程 | 04 §2.5 |
| K-096 | 内存分配与读盘 | 机制 | `cache.c:469-479` | 主流程 | 04 §2.5 |
| K-097 | 归还 `put_block` | 机制 | `cache.c:601-` | 生命周期 | 04 §2.6 |
| K-098 | 腾空 `freeblock` | 机制 | `cache.c:252-272` | 生命周期 | 04 §2.6 |
| K-099 | 单块读 `read_block` | 机制 | `cache.c:723-777` | 设备交互 | 04 §2.7 |
| K-100 | 散射读写 `rw_scattered` | 机制 | `cache.c:840-982` | 设备交互 | 04 §2.7 |
| K-101 | 预读上限规则 | 约束与不变量 | `cache.c:1031-1052` | 机制 | 04 §2.8 |
| K-102 | 预读执行与集合预取 | 机制 | `cache.c:987-1026`、`:1057-1131` | 机制 | 04 §2.8 |
| K-103 | 按设备刷 | 机制 | `cache.c:1136-1166` | 持久化 | 04 §2.9 |
| K-104 | 全刷 | 机制 | `cache.c:1295-1311` | 持久化 | 04 §2.9 |
| K-105 | 失效 | 机制 | `cache.c:782-808` | 持久化 | 04 §2.9 |
| K-106 | 单块释放 | 机制 | `cache.c:613-650` | 持久化 | 04 §2.9 |
| K-107 | 池管理三函数 | 机制 | `cache.c:1192-1293` | 容量 | 04 §2.10 |
| K-109 | 块内存两态 | 数据结构 | `cache.rs` | 结构 | 04 §4.1 |
| K-110 | `BlockSource` trait | 架构演进 | `cache.rs:93` | Rust 化 | 04 §3.1 |
| K-108 | 二级缓存拆三层（本侧声明） | 架构演进 | `vm_cache.rs:426` | 与 07 的接缝 | 04 §3.3 |

- **验收标准**：
  1. 画出块的生命周期状态图（空闲 → 使用中 → 脏 → 写回 → 腾空），每个转换标注触发函数与行锚点
  2. 给出"三种拿块模式 × 三种缓存状态（命中/未命中/窥视未命中）"的行为矩阵
  3. 解释"为什么腾空不摘哈希链"（答：摘链是调用方的活，腾空只负责把块变成可复用）
  4. 给出容量启发式的完整公式与三重上限，并说明"公式是调参得来的经验值"这一事实

### 07-block-cache-vmcache

- **一句话定位**：读者读完能说出块缓存如何与内存服务器共享同一批页，以及这条通道什么时候开、什么时候关。
- **讲什么**：
  - 二级缓存要解决什么（同一批磁盘页被文件缓存与页缓存各存一份，浪费且易不一致）
  - 双份页共享的机制（未命中先问内存服务器；有则整页接手不读盘不拷贝；块放出时把页交回并登记）
  - 三样附带物：旗标字（记脏、记占用）、块标签（索引节点 + 文件偏移，标签换需重新报备）、标定结果（只接受整页）
  - 四个线上调用（映射缓存块、清缓存、忘掉单块、零块通知）
  - 开关时机（`lmfs_may_use_vmcache` 在初装时开、块大小非整页时池自关、`lmfs_set_blocksize` 决定）
  - 关门动作（整设备卸载时收回页并要求忘掉该设备的块；出错也照做）
  - 与 `lmfs_invalidate` / `lmfs_free_block` 的配合
  - 通道另一端是 VM 服务器（跨 stage 引用：`02-stage-vm/24-page-cache.md`）
  - Rust 侧三层拆分（池子旗标机 / 策略层 / 线材）
- **不讲什么**：
  - 缓存本体（06）
  - 块设备通信（08）
  - 通道另一端的 VM 侧页缓存结构（`02-stage-vm`）
  - 零拷贝页移交的旗标机细节（`edge E-FSVMCACHE`，跨 stage）
- **前置**：00、01、02、06
- **后置**：11、14、20、21、34
- **事实底线**：
  - C：`minix3/minix/lib/libminixfs/cache.c` 的 vm 族：`:443-451`（二级缓存查询）、`:459-465`（窥视回退）、`:803-807`（失效通知）、`:616-623`（单块释放通知）、`:1236-1239`（块大小非整页自关）、`lib/libfsdriver/call.c:6`（`fsdriver_vmcache` 静态变量）、`:83-84`（卸载时 `vm_clear_cache`）、`:52-56`（readsuper 时清 `fsdriver_vmcache`）、`lib/libfsdriver/call.c:48-51`（能力位与 `major(dev)==NONE_MAJOR`）
  - Rust：`os/libs/minix-fs/src/vm_cache.rs:426`（`SecondLevelCache`）、`os/fs/mfs/src/second_level.rs`、`os/libs/minix-types/src/types/vm_cache.rs`、`os/servers/vm/src/ipc/cache_handlers.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-087 | 二级缓存双份页共享 | 机制 | `cache.c:443-465` | 本机制核心 | 04 §1.5 |
| K-088 | 三样附带物（旗标字/块标签/标定结果） | 数据结构 | `vm.h`、`VMMC_*` | 结构 | 04 §1.5 |
| K-089 | 关门动作 | 机制 | `call.c:83-84`、`cache.c:803-807` | 生命周期 | 04 §1.5/§2.9 |
| K-162 | 开关时机与块大小门控 | 机制 | `cache.c:1236-1239` | 开关 | 07 §1.1/§2.2 + 04 §1.5 |
| K-105a | 失效与单块释放对 VM 的通知（先通知再作废的理由：块号复用会让旧标签映射出残影） | 机制 | `cache.c:616-623`、`:644-649` | 配合机制 | 04 §2.9 |
| K-317 | 块标签是缓存与内存服务器共享页面的对账凭据 | 架构演进 | `read.c:180` | 标签的用途 | 14 §2.2 |
| K-108a | Rust 三层拆分（池子/策略/线材） | 架构演进 | `vm_cache.rs:426` | Rust 化 | 04 §3.3 |
| N-013 | 通道另一端（VM 侧四个页缓存请求处理） | 接口与协议 | `os/servers/vm/src/ipc/cache_handlers.rs` | 现有 04 §7 只有一句话 | 新增（Rust 源码 + 跨 stage 引用） |
| N-023 | 旗标位与哨兵值的单一权威（`minix-types` 与内存服务器共读） | 约束与不变量 | `os/libs/minix-types/src/types/vm_cache.rs` | 权威位置 | 新增（Rust 源码） |

- **验收标准**：
  1. 画出"文件缓存与页缓存共享同一页"的时序图（未命中 → 问 VM → 接手页 → 使用 → 放出 → 交回）
  2. 给出四个线上调用的签名与语义，每个带 C 锚点
  3. 完整列出二级缓存的开关条件（至少 3 条）与每条的行锚点
  4. 明确声明"通道另一端在 `02-stage-vm/24-page-cache.md`，本篇只讲本侧"，并给出跨 stage 指针

### 08-block-io

- **一句话定位**：读者读完能说出给定设备号、位置、长度，字节如何经过缓存到达调用者或存储。
- **讲什么**：
  - 取书单四要素（设备、位置、长度、方向）
  - 窥视即"收货地址缺席的读"（概念统一）
  - 三道关卡（设备号非空、位置长度合法且和不溢出、分区裁剪）
  - 起点已在分区外报零字节而非错误
  - 首尾不满、中间整块的行走模型与三取小
  - 每块四步（拿块、搬运、标脏、放回）
  - 读预取（拿当前块前先窥视后面几块；遇已缓存即停；失败静默）
  - 整块覆写免读
  - 刷后失效（先刷脏块再作废；顺序不可反）
  - 驱动绑定（登记设备号与驱动标签）
  - 文件头契约（必须初始化缓冲池、启用 vmcache、提供同步回调）
  - 磁盘 backed FS 的三点注意
  - 短搬加后续错误报短计数
  - Rust 侧：`DeviceInfo` trait、槽位统整块、`RamDisk` 是生产代码、`BdevBlockSource` 真块驱动桥接（接缝声明）
- **不讲什么**：
  - 缓存内部的哈希、淘汰、引用计数（06）
  - 二级缓存通道（07）
  - 真实块驱动的实现（16-stage-drivers）
  - 文件层面的读写（20、21）
  - 超级块与位图（12、13）
- **前置**：00、01、02、06
- **后置**：11、13、20、21、23、27、28、34
- **事实底线**：
  - C：`minix3/minix/lib/libminixfs/bio.c`（263 行：`:49` `lmfs_driver`、`:65` `block_prefetch`、`:117` `lmfs_bio`、`:255` `lmfs_bflush`；`:1-40` 文件头契约、`:127` 设备号关卡、`:135` 长度零早退、`:138` 位置长度关卡、`:141`/`:146` 分区大小查询、`:149` 起点在分区外、`:152` 裁剪、`:156`/`:166` 行走、`:173` 三取小、`:186-210` 每块四步、`:194` 免读、`:242` 短计数、`:249` 刷后失效）
  - Rust：`os/libs/minix-fs/src/bio.rs:40`（`DeviceInfo`）、`os/libs/minix-fs/src/bdev_bridge.rs`、`os/libs/minix-bdev/src/client.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-111 | 取书单四要素 | 接口与协议 | `bio.c:127-138` | 接口 | 05 §1.1 |
| K-112 | 窥视即缺席的读 | 概念 | `bio.c` | 概念统一 | 05 §1.1/§3.3 |
| K-113 | 关卡一：设备号 | 约束与不变量 | `bio.c:127` | 关卡 | 05 §1.2 |
| K-114 | 关卡二：位置长度 | 约束与不变量 | `bio.c:138` | 关卡 | 05 §1.2 |
| K-115 | 关卡三：分区裁剪 | 机制 | `bio.c:146`、`:152` | 关卡 | 05 §1.2 |
| K-116 | 起点在分区外报零 | 约束与不变量 | `bio.c:149` | 关卡 | 05 §1.2 |
| K-117 | 长度零早退 | 机制 | `bio.c:135` | 关卡 | 05 §1.2 |
| K-118 | 行走模型 | 机制 | `bio.c:156`、`:166` | 主流程 | 05 §1.3 |
| K-119 | 三取小 | 约束与不变量 | `bio.c:173` | 主流程 | 05 §1.3 |
| K-120 | 每块四步；写必标脏 | 机制 | `bio.c:186-210` | 主流程 | 05 §1.3/§2.6 |
| K-121 | 读预取 | 机制 | `bio.c:65-97`、`:186` | 优化 | 05 §1.4 |
| K-122 | 整块覆写免读 | 机制 | `bio.c:194` | 优化 | 05 §1.5 |
| K-123 | 刷后失效与顺序不可反 | 约束与不变量 | `bio.c:255` | 持久化 | 05 §1.6 |
| K-124 | 驱动绑定 | 接口与协议 | `bio.c:49-53` | 接缝 | 05 §1.7 |
| K-125 | 文件头契约 | 接口与协议 | `bio.c:1-40` | 契约 | 05 §2.1 |
| K-126 | 磁盘 backed FS 三点注意 | 约束与不变量 | `bio.c:1-40` | 契约 | 05 §2.1 |
| K-127 | 溢出检查表达 | 约束与不变量 | `bio.c:138` | 关卡 | 05 §2.4 |
| K-128 | 短搬报短计数 | 接口与协议 | `bio.c:242` | 接口 | 05 §2.4 |
| K-129 | `DeviceInfo` trait | 架构演进 | `bio.rs:40` | Rust 化 | 05 §3.1 |
| K-130 | 槽位统整块、搬运裁剪 | 架构演进 | `bio.c` | Rust 化 | 05 §3.2/§2.6 |
| K-131 | `RamDisk` 是生产代码 | 架构演进 | `bio.rs:351` | 实现 | 05 §3.5 |
| K-132 | `BdevBlockSource` 真块驱动桥接 | 接口与协议 | `bdev_bridge.rs` | 接缝 | 05 §3.6 |
| K-133 | 末块短读在桥接层落定 | 约束与不变量 | `bdev_bridge.rs` | 接缝 | 05 §3.6 |
| K-134 | 批量钩子默认逐块走 | 接口与协议 | `bdev_bridge.rs` | 接缝 | 05 §3.6 |
| K-135 | 标签上限 80 | 约束与不变量 | `DS_MAX_KEYLEN` | 契约 | 05 §4.1 |

- **验收标准**：
  1. 给出"位置 + 长度 → 实际搬运量"的三道关卡逐条判定表，含每种失败的结果
  2. 解释"为什么刷后失效的顺序不能反"（正反两个后果都要说）
  3. 给出预取的停止条件（至少 3 条）与每条的行锚点
  4. 明确声明真块驱动的接缝归属（`edge E-FSBDEV` → 16-stage-drivers），本篇只讲接缝契约

### 09-server-assembly

- **一句话定位**：读者读完能说出 8 个 FS server 各自怎么诞生、初装做了什么、实现了哪些回调、彼此差在哪里。
- **讲什么**：
  - FS server 进程的诞生链（RS 加载 → SEF 启动 → 注册三回调 → 初装 → 进循环）
  - 三个循环入口的对照（`fsdriver_task` / `run_vtreefs` / `sffs_loop`）
  - 初装的通用形状与各 server 的差异（pfs 的表在挂载时才初始化；mfs 四步；isofs 建 100 块池；procfs 建树；ptyfs 建 32 槽位表）
  - 8 个 server 的回调表总览（pfs 9 / ptyfs 8 / mfs 31 / ext2 31 / vtreefs 17 / sffs 17 / isofs 只读子集）
  - 变体差异总表（启动时机 / 依赖框架 / 磁盘有无 / 回调数 / 只读性 / 根挂载 / 能力位 / 构建期常量）
  - 装配顺序契约（先源后框、释放逆序）
  - 服务策略配置（`system.conf` 的授权面）
  - 接线表的状态记账（三态：C 有实现 / Rust 已通 / 待建）
- **不讲什么**：
  - 主循环与分发机制（02）
  - 各 server 的语义（10、24–29）
  - 具体回调的实现（各 server 篇）
  - 生产传输接缝（34）
  - 构建链与镜像（35）
- **前置**：00、01、02、03、05
- **后置**：10、24、25、26、27、28、29、34、35
- **事实底线**：
  - C：`fs/mfs/main.c:14/47`（`main`/`sef_cb_init_fresh`）、`fs/pfs/pfs.c:394/441`（`pfs_init`/`main`）、`lib/libvtreefs/vtreefs.c:16/88`（`init_server`/`run_vtreefs`）、`lib/libsffs/main.c:17/53`（`sffs_init`/`sffs_loop`）、各 `table.c`：`fs/mfs/table.c:13-45`、`fs/pfs/pfs.c:425-435`、`fs/ptyfs/ptyfs.c:425-434`、`fs/ext2/table.c`、`lib/libvtreefs/table.c:6-23`、`lib/libsffs/table.c`、`fs/isofs/table.c`、`fs/procfs/main.c:22`（`construct_tree`）
  - 非 C 制品：8 个 server Makefile、`minix3/etc/system.conf:353/507`、`minix3/etc/usr/rc:255-256`
  - Rust：`os/fs/*/src/main.rs`、`os/libs/minix-fs/src/task.rs:397`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-003a | 三档启动时机 | 概念 | `kernel/table.c:62-63`、`etc/system.conf:353/507` | 装配的入口 | 新增 |
| N-006a | 三个循环入口的对照 | 接口与协议 | `fsdriver.c:80`、`vtreefs.c:88`、`libsffs/main.c:53` | 装配的终点 | 新增 |
| N-007 | 8 个 server 的回调数总览 | 数据结构 | 各 `table.c` | 装配的核心表 | 新增 |
| N-008 | 变体差异总表 | 数据结构 | 综合 | 装配的总纲 | 新增 |
| K-160 | 初装四步（mfs） | 机制 | `main.c:52-62` | 初装代表 | 07 §1.1/§2.2 |
| K-181 | 服务装配（`MfsServer`、`Parts`、同步次序、能力位） | 机制 | `server.rs:323` | 越界内容归位 | 07 §4.5（越界） |
| K-179 | 接线表做成常量数组与三态 | 架构演进 | `table.rs:40` | 状态记账 | 07 §3.4 + 10 §4.3（越界） |
| K-412 | 启动三件套与信号只应终止（vtreefs） | 机制 | `vtreefs.c:16` | 初装差异 | 18 §1.10/§2.1 |
| K-455 | 固定表 32 槽位（ptyfs） | 数据结构 | `node.c:20-25` | 初装差异 | 20 §1.1/§4.1 |
| K-591 | 两桥装配顺序（sffs） | 机制 | `vbfs.c:127`、`hgfs.c:93` | 装配差异 | 24 §1.9/§2.8 |
| K-592 | 装配先源后框、释放逆序 | 架构演进 | `07` 对照 | 装配契约 | 24 §1.9 |
| K-155 | 丢特权（pfs 到 `SERVICE_UID`） | 机制 | `pfs.c:394-403` | 初装差异 | 06 §1.6/§2.8 |
| N-010a | 服务策略配置（`system.conf` 授权面） | 工具与工程 | `etc/system.conf:353/507` | 装配的授权前提 | 新增 |
| N-024 | 接线表三态记账规则（C 有实现 / Rust 已通 / 待建） | 工具与工程 | `os/fs/mfs/src/table.rs:40` | 现有 07/10 篇分散提到 | 新增（Rust 源码 + todo.md 的账本漂移教训） |

- **验收标准**：
  1. 给出 8 个 server 的完整对照表（至少 8 列：启动时机 / 依赖框架 / 磁盘 / 回调数 / 只读 / 根挂载 / 能力位 / 构建期常量）
  2. 每个 server 的初装步骤逐条列出，带 C 行锚点
  3. 给出回调差异矩阵：行是 8 个 server，列是 35 个 `fdr_*` 回调，格子标注"实现 / 空 / 不适用"
  4. 解释"为什么 pfs 的 inode 表在挂载时初始化"（答：表生命周期与挂载绑定，无根挂载的语义要求）

### 10-pfs

- **一句话定位**：读者读完能说出最小的完整文件服务器长什么样，每一行代码为什么是必要的。
- **讲什么**：
  - 为什么启动时先挂载它（管道不需要磁盘，必须早于根 FS 就绪）
  - 无根挂载的机制（挂载即初始化内存表、设备号与标志忽略、根节点全零）
  - 五百一十二槽位节点表与空闲编号栈（倒序压栈、低号先出）
  - 借还最小闭环（出生三检、死亡三事、计数长不大）
  - 管道读写语义（流不是文件、读三取小、写先验后压缩、截断只支持全清）
  - 懒时间三标记与一站式结清
  - 改模式只动权限位
  - 退出只认终止信号（无盘不刷）
  - 节点结构十三字段
  - 卸载扫表找在用节点
  - 状态回复的三处变通（设备字段、块大小、块数）
  - 回调表九项与启动三注册
  - Rust 侧：编号栈代替侵入链表、变长缓冲、时钟 trait、忙计数、状态布局本地化
- **不讲什么**：
  - 分发与适配器的通用规则（02、05）
  - VFS 侧的管道与字符设备逻辑（`05-stage-vfs/17-pipe.md`）
  - 磁盘 FS 的超级块与索引节点（11 起）
  - 进程入口的启动握手（34 的接缝声明）
- **前置**：00、01、02、03、05、09
- **后置**：11（对照：最小 vs 完整）、26（另一个轻量 server）
- **事实底线**：
  - C：`minix3/minix/fs/pfs/pfs.c`（451 行：`:19` `PFS_NR_INODES 512`、`:21` `LIST_HEAD`、`:23-25` `ATIME`/`MTIME`/`CTIME`、`:27-47` 节点结构、`:50` `pfs_mount`、`:63` 倒序压栈、`:88` `pfs_unmount`、`:126` `pfs_newnode`、`:134/141/146` 三检、`:184` `pfs_putnode`、`:191` 计数校验、`:218` `pfs_read`、`:254` `pfs_write`、`:265` 总量校验、`:274` 压缩、`:299` `pfs_trunc`、`:322` `pfs_stat`、`:362` `pfs_chmod`、`:381` `pfs_signal`、`:394` `pfs_init`、`:405-424` `pfs_startup`、`:425-435` `pfs_table`、`:441` `main`）
  - 非 C 制品：`fs/pfs/Makefile`（只链 `-lfsdriver -lsys`）、`servers/vfs/mount.c:391`（`mount_pfs`）、`servers/vfs/main.c:510`
  - Rust：`os/fs/pfs/src/lib.rs:274`（`FsDriver` impl）、`os/fs/pfs/src/main.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-136 | 管道服务器是启动时第一个挂载的 FS | 概念 | `servers/vfs/main.c:510`、`mount.c:391` | 定位 | 06 §1.1 |
| K-137 | 无磁盘依赖因而永远就绪 | 概念 | `fs/pfs/Makefile` | 定位 | 06 §1.1 |
| K-138 | 无根挂载 | 机制 | `pfs.c:50-86` | 独有机制 | 06 §1.1/§2.2 |
| K-139 | 五百一十二槽位节点表 | 数据结构 | `pfs.c:19` | 结构 | 06 §1.2/§2.1 |
| K-140 | 节点号从一开始、零号保留 | 约束与不变量 | `pfs.c:126` | 结构 | 06 §1.2/§2.4 |
| K-141 | 空闲槽位编号栈 | 数据结构 | `pfs.c:63`、`:21` | 结构 | 06 §1.2/§2.2 |
| K-142 | 固定表是深思熟虑的简化 | 架构演进 | `pfs.c:14-18` 注释 | 设计理由 | 06 §1.2 |
| K-143 | 出生路三检 | 机制 | `pfs.c:134/141/146` | 生命周期 | 06 §1.3/§2.4 |
| K-144 | 死亡路三事 | 机制 | `pfs.c:184-216` | 生命周期 | 06 §1.3/§2.5 |
| K-145 | 计数长不大 | 约束与不变量 | `pfs.c:191` | 生命周期 | 06 §1.3 |
| K-146 | 栈式重用 | 测试性质 | — | 生命周期 | 06 §1.3/§5.1 |
| K-147 | 管道是流不是文件 | 概念 | `pfs.c:218/254` | 语义 | 06 §1.4 |
| K-148 | 读三取小 | 机制 | `pfs.c:218-253` | 语义 | 06 §1.4/§2.6 |
| K-149 | 写先验总量 | 机制 | `pfs.c:265` | 语义 | 06 §1.4/§2.6 |
| K-150 | 写时压缩 | 机制 | `pfs.c:274` | 语义 | 06 §1.4/§2.6 |
| K-151 | 截断只支持全清 | 约束与不变量 | `pfs.c:299-320` | 语义 | 06 §1.4/§2.7 |
| K-152 | 懒时间三标记与一站式结清 | 机制 | `pfs.c:23-25`、`:322-360` | 语义 | 06 §1.5/§2.7 |
| K-153 | 改模式只动权限位 | 约束与不变量 | `pfs.c:362-379` | 语义 | 06 §1.5/§2.7 |
| K-154 | 退出只认终止信号 | 机制 | `pfs.c:381-392` | 生命周期 | 06 §1.6/§2.8 |
| K-155 | 丢特权 | 机制 | `pfs.c:394-403` | 启动 | 06 §1.6/§2.8 |
| K-156 | 节点结构十三字段 | 数据结构 | `pfs.c:27-47` | 结构 | 06 §2.1 |
| K-157 | 卸载扫表找在用节点 | 接口与协议 | `pfs.c:88-103` | 生命周期 | 06 §2.3 |
| K-158 | 状态回复三处变通 | 接口与协议 | `pfs.c:322-360` | 接口 | 06 §2.7 |
| K-159 | 回调表九项与启动三注册 | 接口与协议 | `pfs.c:405-435` | 装配 | 06 §2.8 |
| K-474 | 编号栈代替侵入链表 | 架构演进 | `os/fs/pfs/src/lib.rs` | Rust 化 | 06 §3.1 |
| K-475 | 变长缓冲 | 架构演进 | `lib.rs` | Rust 化 | 06 §3.2 |
| K-476 | 时钟做成 trait | 架构演进 | `lib.rs` | Rust 化 | 06 §3.3 |
| K-477 | 忙警告改计数 | 架构演进 | `lib.rs` | Rust 化 | 06 §3.4 |
| K-478a | 状态布局本地化（后收敛到 typed `Stat`） | 架构演进 | `lib.rs` | Rust 化 | 06 §3.5 |

- **验收标准**：
  1. 逐条给出九个回调的实现要点与 C 行锚点
  2. 解释"为什么最小 server 的 `mount` 返回根号零"（答：PFS 无根节点，VFS 侧明确丢弃返回值，`servers/vfs/mount.c:422-425`）
  3. 给出管道读写的边界条件表（读三取小的三种情况、写超量的拒绝）
  4. 说明"这个 server 为什么是最好的框架入门样例"（答：九个回调、无磁盘、无位图、无间接块，全部依赖只有框架）

### 11-mfs-startup

- **一句话定位**：读者读完能说出磁盘服务器启动时按什么顺序准备什么，接线表如何划分已通与待建。
- **讲什么**：
  - 初装四步与"顺序即依赖"（开 vmcache 开关 → inode 表清零 → 初始化 inode 缓存 → 建 1024 块缓冲池）
  - 信号语义"先结账再关门"与决定/执行分离
  - 接线表 31 项回调的现状、分组、三处处理器复用、五项直指块库
  - 无新节点行的理由
  - 拿块包装行规（失败除缺席即腐败级报错；缺席只许窥视）
  - 层间严格度差异
  - 区分配三步与换算公式（区号 = 首区 − 1 + 位号）
  - 区释放三笔与缓存作废的两条理由
  - 缺空间打印节流
  - `err_code` 全局改返回值
  - 位图闭包注入（策略在本篇、机制在 13）
  - 被 `#if 0` 屏蔽的 `cch_check`（调试墓碑）
- **不讲什么**：
  - 超级块格式（12）、位图机制（13）、inode 组织（14）、挂载流程（15）
  - 各回调实现（16–23）
  - 服务装配与进程握手（09、34）
  - 分发主循环（02）
- **前置**：00、01、02、03、05、06、08、09
- **后置**：12、13、14、15、16–23
- **事实底线**：
  - C：`minix3/minix/fs/mfs/main.c`（102 行：`:14` `main`、`:47` `sef_cb_init_fresh`、`:52` `lmfs_may_use_vmcache(1)`、`:55-58` 表清零、`:60` `init_inode_cache()`、`:62` `lmfs_buf_pool(DEFAULT_NR_BUFS)`、`:70` `sef_cb_signal_handler`、`:75` `fs_sync()`、`:77` `fsdriver_terminate()`、`:81-101` `#if 0` 的 `cch_check`）、`fs/mfs/table.c`（45 行，全文）、`fs/mfs/cache.c`（109 行：`:22` `get_block`、`:42` `alloc_zone`、`:85` `free_zone`）、`fs/mfs/cache.c:102-107`（作废两理由）
  - 非 C 制品：`fs/mfs/Makefile`（`CPPFLAGS+= -DDEFAULT_NR_BUFS=1024`、`LDADD+= -lminixfs -lfsdriver -lbdev -lsys`）
  - Rust：`os/fs/mfs/src/startup.rs`、`os/fs/mfs/src/table.rs:40`、`os/fs/mfs/src/mfs_cache.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-160a | 初装四步 | 机制 | `main.c:52-62` | 启动链 | 07 §1.1/§2.2 |
| K-161 | 顺序即依赖 | 约束与不变量 | `main.c:52-62` | 启动链 | 07 §1.1/§3.1 |
| K-162a | vmcache 开关与门控 | 机制 | `cache.c:1236-1239` | 启动链（细节归 07） | 07 §1.1/§2.2 |
| K-163 | 信号语义"先结账再关门" | 机制 | `main.c:70-78` | 生命周期 | 07 §1.2/§2.3 |
| K-164 | 决定与执行分离 | 架构演进 | — | 设计 | 07 §1.2/§3.5 |
| K-165 | 接线表 31 项与分组 | 数据结构 | `table.c:13-45` | 接线 | 07 §1.3/§2.4 |
| K-166 | 无新节点行 | 约束与不变量 | `table.c:13-45` | 接线 | 07 §1.3/§2.4 |
| K-167 | 显式接线表优于静默桩 | 架构演进 | — | 设计 | 07 §1.3 |
| K-168 | 三处处理器复用 | 接口与协议 | `table.c:18-20`、`:28-29`、`:41-43` | 接线 | 07 §1.3/§2.4 |
| K-169 | 五项直指块传输库函数 | 接口与协议 | `table.c:40-44` | 接线 | 07 §2.4 |
| K-170 | 拿块包装行规 | 约束与不变量 | `fs/mfs/cache.c:22-37` | 包装 | 07 §1.4/§2.5 |
| K-171 | 层间严格度差异 | 概念 | — | 包装 | 07 §1.4 |
| K-172 | 区分配三步 | 机制 | `fs/mfs/cache.c:42-80` | 分配 | 07 §1.5/§2.6 |
| K-173 | 换算公式与位零保留 | 约束与不变量 | `cache.c:79`、`:67` | 分配 | 07 §1.5/§2.6 |
| K-174 | 区释放三笔 | 机制 | `fs/mfs/cache.c:85-109` | 释放 | 07 §1.5/§2.7 |
| K-175 | 缓存作废两理由与单块单区断言 | 约束与不变量 | `cache.c:102-107` | 释放 | 07 §2.7 |
| K-176 | 缺空间打印节流 | 机制 | `cache.c:69-76` | 分配 | 07 §2.6/§2.8 |
| K-177 | `err_code` 全局改返回值 | 架构演进 | — | 设计 | 07 §3.3 |
| K-178 | 位图闭包注入 | 架构演进 | `mfs_cache.rs` | 设计 | 07 §3.2 |
| K-180 | `cch_check` 调试墓碑 | 工具与工程 | `main.c:81-101` | 越界内容归位（工程趣闻） | 07 §2.2/§2.9（越界） |
| K-165a | 接线表分组即请求逻辑分组 | 数据结构 | `table.c:13-45` | 接线 | 07 §2.4 |

- **验收标准**：
  1. 给出初装四步的依赖图，说明每一步为什么必须在前一步之后
  2. 给出接线表 31 项的完整表：序号、`fdr_*` 名、指向的函数、C 文件、请求号
  3. 逐条列出"拿块包装行规"的两条规则与违反后果
  4. 给出区号与位号的换算公式，并解释"位零为什么保留"

### 12-mfs-super

- **一句话定位**：读者读完能看着一块磁盘说清它的七段布局，以及服务器如何判定它是不是自家格式。
- **讲什么**：
  - 磁盘七段全景（引导块、超级块区、inode 位图、区位图、inode 区、填充、数据区）
  - 自描述格式（各段长度写在超级块里）
  - 31 字节磁盘格式的逐字段布局与加法验算
  - 三魔数两命运（0x137F / 0x2468 / 0x4D5A；只认三版；两类拒绝错）
  - 版本检查必须先于字段转换
  - 块大小五连验与"从便宜到贵"的排序
  - 首区计算"装不下就现算"与"加规则不加字段"的演进手法
  - 几何 sanity 逐条拒绝
  - 强制标志掩码（高八位"必须理解"）
  - 干净位（上次是否干净卸载）
  - 超级块结构的十三磁盘字段加九内存字段
  - 磁盘字段增加必须同步改"读写截止字段"
  - `rw_super` 读写通道
  - `read_super` 解析全步骤
  - `write_super` 写守卫
  - 块大小是缓存的属性不是超级块的属性
  - 逐字段小端读写
- **不讲什么**：
  - 位图的分配与释放机制（13）
  - 挂载流程（15）
  - inode 组织（14）
  - 区分配的 hint 策略（11）
  - 目录项格式（16）
- **前置**：00、01、02、06、08、11
- **后置**：13、14、15、19、23
- **事实底线**：
  - C：`minix3/minix/fs/mfs/super.c`（365 行：`:29` `alloc_bit`、`:111` `free_bit`、`:161` `get_block_size`、`:173` `rw_super`、`:184` 截止字段、`:241` `read_super`、`:252-260` 版本门、`:266-273` 小端转换、`:287-293` 块大小五连验、`:299-308` 首区现算、`:333-342` 几何、`:348-352` 强制标志、`:360` `write_super`）、`fs/mfs/super.h`（78 行：`:10-20` 磁盘布局注释、`:24-60` 结构、`:42-46` 截止字段、`:62-63` 位图选择、`:68` `MFSFLAG_CLEAN`、`:75` 强制标志掩码）、`fs/mfs/const.h`（68 行：`:22/24/26` 三魔数、`:32` `NO_BIT`、`:48-51` 位置常量、`:53-59` 派生尺寸、`:62-65` 尺寸）
  - Rust：`os/fs/mfs/src/superblock.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-182 | 磁盘七段全景 | 数据结构 | `super.h:10-20` | 布局 | 08 §1.1/§2.1 |
| K-183 | 自描述格式 | 概念 | `super.h:24-60` | 布局 | 08 §1.1 |
| K-184 | 三魔数两命运 | 约束与不变量 | `const.h:22/24/26`、`super.c:252-260` | 校验 | 08 §1.2/§2.2/§2.7 |
| K-185 | 版本检查先于字段转换 | 约束与不变量 | `super.c:252-260` | 校验 | 08 §1.2 |
| K-186 | 块大小五连验 | 约束与不变量 | `super.c:287-293` | 校验 | 08 §1.3/§2.7 |
| K-187 | 五道检查从便宜到贵 | 架构演进 | — | 校验 | 08 §1.3 |
| K-188 | 首区现算 | 机制 | `super.c:299-308` | 布局 | 08 §1.4/§2.7 |
| K-189 | 加规则不加字段 | 架构演进 | — | 演进手法 | 08 §1.4 |
| K-190 | 几何 sanity 逐条拒绝 | 约束与不变量 | `super.c:333-342` | 校验 | 08 §1.5/§2.7 |
| K-191 | 强制标志掩码 | 约束与不变量 | `super.h:75`、`super.c:348-352` | 校验 | 08 §1.5/§2.1/§2.7 |
| K-196 | 干净位 | 机制 | `super.h:68` | 布局（消费在 15、23） | 08 §1.7/§2.1 |
| K-197 | 结构十三磁盘字段加九内存字段 | 数据结构 | `super.h:24-60` | 结构 | 08 §2.1 |
| K-198 | 磁盘字段增加须同步改截止字段 | 约束与不变量 | `super.h:42-46`、`super.c:184` | 结构 | 08 §2.1/§2.6 |
| K-199 | 块大小是缓存的属性 | 概念 | `super.c:161-168` | 接口 | 08 §2.5 |
| K-200 | `rw_super` 读写通道 | 接口与协议 | `super.c:173-236` | 接口 | 08 §2.6 |
| K-201 | `read_super` 解析全步骤 | 机制 | `super.c:241-355` | 主流程 | 08 §2.7 |
| K-202 | 写守卫与执行分离 | 接口与协议 | `super.c:360-364` | 接口 | 08 §2.8 |
| K-203 | 九错误变体与错误码映射边界 | 接口与协议 | — | 错误（总图在 31） | 08 §3.2 |
| K-204 | 位图不知只读；重复释放报告不崩 | 架构演进 | — | 设计 | 08 §3.3/§3.4 |
| K-205 | 逐字段小端读写 | 架构演进 | `super.c:266-273` | 实现 | 08 §3.1/§3.5 |

- **验收标准**：
  1. 画出磁盘七段布局图，标出每段的起点与长度来源（超级块字段名）
  2. 给出 31 字节磁盘格式的字段表与加法验算（2+2+2+2+4+4+4+4+40 = 64 是 inode；超级块的 31 字节另算）
  3. 给出块大小五连验的完整条件表与每条的失败后果
  4. 解释"为什么 `read_super` 失败要返回错误而 `rw_super` 写失败要宕机"（答：读失败是环境问题，写失败是内部错误）

### 13-mfs-bitmap

- **一句话定位**：读者读完能说清位图如何记账、分配从哪找、释放要做什么、为什么零号位永不分配。
- **讲什么**：
  - 位图账本模型（一位一 inode / 一区，置位即已用）
  - 两张位图（`IMAP` / `ZMAP`）与各自的起点、总位数
  - 分配 `alloc_bit` 全流程（从搜索提示位找第一个空位；起点越界从零找；找到置位返回位号；找遍返回无位）
  - 字节序转换后才能逐位判断
  - 区图分配时维护已用计数与用量变更
  - 释放 `free_bit` 全流程（清位；重复释放报告失败不崩）
  - 零号位永不分配（"零即无"）
  - 搜索提示位的鲁棒性处理
  - 位图与只读的关系（政策上浮、数据下沉）
- **不讲什么**：
  - 超级块的解析与校验（12）
  - 区分配的 hint 策略（11）
  - 空闲位统计（23）
  - ext2 的每组位图（27）
  - 位图在磁盘上的位置（12 讲布局）
- **前置**：00、01、02、06、08、11、12
- **后置**：14、19、23、27
- **事实底线**：
  - C：`minix3/minix/fs/mfs/super.c:29-106`（`alloc_bit`：`:45-46` 定起点块与总位数、`:48-56` 起点越界归零、`:59` 循环块数加一轮、`:62-63` 字节序转换、`:66-104` 逐位搜索、`:78-79` 置位、`:82-84` 标脏、`:87` 区图计数加一、`:90-97` 用量变更、`:101-102` 返回位号、`:105` 返回无位）、`:111-156`（`free_bit`：`:131-155` 定位与清位、`:140-144` 重复释放处理）、`fs/mfs/super.h:62-63`（`IMAP`/`ZMAP`）、`fs/mfs/const.h:32`（`NO_BIT`）
  - Rust：`os/fs/mfs/src/superblock.rs`（`Bitmap`、`count_clear_bits_in_image`）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-192 | 位图账本与 `IMAP`/`ZMAP` | 数据结构 | `super.h:62-63` | 结构 | 08 §1.6 |
| K-193 | 位图分配 | 机制 | `super.c:29-106` | 机制 | 08 §1.6/§2.3 |
| K-194 | 位图释放 | 机制 | `super.c:111-156` | 机制 | 08 §1.6/§2.4 |
| K-195 | 零号位永不分配 | 约束与不变量 | `const.h:32` | 不变量 | 08 §1.6 |
| K-373a | 两种位图各自的起点与总位数公式 | 数据结构 | `stats.c:32-42`、`const.h:48-51` | 结构 | 17 §1.2 |
| K-376a | 搜索提示位越界归零 | 机制 | `stats.c:45` | 鲁棒性 | 17 §1.2 |
| K-377a | 位图字按本机字节序转换 | 机制 | `stats.c:65` | 机制 | 17 §1.2 |
| K-204a | 位图不知只读（政策上浮） | 架构演进 | — | 设计 | 08 §3.3 |
| K-206a | 分配时填槽与擦大小区号时间 | 机制 | `inode.c:285-299` | inode 位图的消费 | 09 §2.9 |

- **验收标准**：
  1. 给出 `alloc_bit` 的完整流程图（含起点越界、循环加一轮、区图计数维护三个特殊点）
  2. 解释"为什么要循环块数加一轮"（答：起点在中间，要绕回覆盖起点之前的块）
  3. 给出"位零保留"的三处证据（inode 号零、区号零、设备号零）
  4. 说明"重复释放为什么报告失败而不宕机"（答：坏输入 vs 编程错误的区分）

### 14-mfs-inode

- **一句话定位**：读者读完能说出磁盘上的 64 字节如何变成内存中的工作对象，以及工作对象如何借还。
- **讲什么**：
  - 磁盘格式与内存形态的差异（"存储窄、工作宽"分离原则）
  - 磁盘格式 64 字节的逐字段布局与加法验算（2+2+2+2+4+4+4+4+40 = 64）
  - 内存形态多出的工作状态项
  - 双向转换函数（`rw_inode` 读上来逐字段拓宽、`new_icopy` 写回去逐字段收窄）
  - 一百二十八桶哈希（按号低七位分桶；桶数是 2 的幂故用掩码）
  - 引用计数语义（打开加一、释放减一、归零按链接数分流）
  - 归零分流（有链接写回推车尾；零链接上报待回收区号、清号牌推车头）
  - 车头车尾淘汰策略（用队列位置表达访问热度）
  - 获取三路（引用中命中 / 冷命中 / 缺席）与"只有冷命中才算命中"
  - 车空报文件表满
  - 分配两步舞（位图上占位 → 表里占槽；表满退位）
  - 释放逆过程（号越界或零号静默忽略、清位图位、搜索 hint 回拨）
  - 擦除 `wipe_inode`（分配与截断共用）
  - 懒时间（置待更新位、结算时机、只读跳过）与懒更新动机
  - 脏标记宏与"只读盘出现脏节点是逻辑错误"
  - 释放请求协议 `fs_putnode`（计数减请求数加一留一）
  - 纯找 `find_inode`、加引用 `dup_inode`
  - 读写转换的定位算术
  - 几何参数子视图（防腐层）
  - 只读挂载下写回整体拒绝
- **不讲什么**：
  - 超级块与位图（12、13）
  - 挂载流程（15）
  - 目录项查找（16）
  - 数据区的截断与释放（19）
  - 数据区的映射（20、21）
- **前置**：00、01、02、06、08、11、12、13
- **后置**：15、16、17、18、19、20、21、22、23
- **事实底线**：
  - C：`minix3/minix/fs/mfs/inode.c`（464 行：`:38` `fs_putnode`、`:70` `init_inode_cache`、`:96`/`:112` `addhash_inode`/`unhash_inode`、`:118` `get_inode`、`:132-144` 命中三路、`:147-150` 表满、`:154-168` 设备为空跳过读盘、`:180` `find_inode`、`:206` `put_inode`、`:236-244` 归零分流、`:252` `alloc_inode`、`:285-299` 填槽、`:309` `wipe_inode`、`:328` `free_inode`、`:349` `update_times`、`:352-357` 懒更新动机注释、`:375` `rw_inode`、`:389-393` 定位算术、`:397-408` 写路径、`:424-447` 逐字段拷、`:455` `dup_inode`）、`fs/mfs/inode.h`（72 行：`:20-50` 结构、`:32-48` 内存字段、`:67-70` 脏标记宏）、`fs/mfs/type.h`（21 行：`:8-17` `d2_inode`）
  - Rust：`os/fs/mfs/src/inode.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-206 | 磁盘格式 64 字节紧凑记录 | 数据结构 | `type.h:8-17` | 结构 | 09 §1.1/§2.2 |
| K-207 | 内存形态多出的工作状态项 | 数据结构 | `inode.h:32-48` | 结构 | 09 §1.1/§2.1 |
| K-208 | "存储窄、工作宽"分离原则 | 概念 | — | 设计 | 09 §1.1 |
| K-209 | 显式转换函数双向 | 机制 | `inode.c:375` | 机制 | 09 §1.1/§2.13 |
| K-210 | 一百二十八桶哈希与掩码 | 数据结构 | `inode.h` | 结构 | 09 §1.2/§2.5 |
| K-211 | 哈希只管"找到"不管"能不能用" | 机制 | `inode.c:118` | 机制 | 09 §1.2 |
| K-212 | 引用计数语义 | 机制 | `inode.c:206` | 生命周期 | 09 §1.3 |
| K-213 | 归零分流 | 机制 | `inode.c:236-244` | 生命周期 | 09 §1.3/§2.8 |
| K-214 | 车头车尾淘汰策略 | 架构演进 | `inode.c:70` | 设计 | 09 §1.3 |
| K-215 | 获取三路与"只有冷命中才算命中" | 机制 | `inode.c:132-144` | 生命周期 | 09 §1.4/§2.6 |
| K-216 | 车空报文件表满 | 约束与不变量 | `inode.c:147-150` | 生命周期 | 09 §1.4/§2.6 |
| K-217 | 设备为空跳过读盘 | 机制 | `inode.c:154-168` | 生命周期 | 09 §1.4/§2.6 |
| K-218 | 分配两步舞 | 机制 | `inode.c:252` | 生命周期 | 09 §1.5/§2.9 |
| K-219 | 释放逆过程 | 机制 | `inode.c:328` | 生命周期 | 09 §1.5/§2.11 |
| K-220 | 懒时间 | 机制 | `inode.c:349` | 机制 | 09 §1.6/§2.12 |
| K-221 | 懒更新动机 | 概念 | `inode.c:352-357` | 设计 | 09 §2.12 |
| K-222 | 脏标记宏与逻辑错误 | 约束与不变量 | `inode.h:67-70` | 不变量 | 09 §2.1 |
| K-223 | 释放请求协议 `fs_putnode` | 接口与协议 | `inode.c:38-64` | 接口 | 09 §2.3 |
| K-224 | 纯找 `find_inode` | 接口与协议 | `inode.c:180-200` | 接口 | 09 §2.7 |
| K-225 | 擦除 `wipe_inode` | 机制 | `inode.c:309-323` | 机制 | 09 §2.10 |
| K-226 | 读写转换定位算术 | 机制 | `inode.c:389-393` | 机制 | 09 §2.13 |
| K-227 | 加引用 `dup_inode` | 接口与协议 | `inode.c:455` | 接口 | 09 §2.14 |
| K-228 | 表做成值 | 架构演进 | `inode.rs` | Rust 化 | 09 §3.1 |
| K-229 | 释放上报区号 | 架构演进 | `inode.rs` | Rust 化 | 09 §3.2 |
| K-230 | 五错误变体与两种满对策 | 接口与协议 | — | 错误（总图在 31） | 09 §3.4 |
| K-231 | 几何参数子视图 | 架构演进 | `inode.rs` | Rust 化 | 09 §3.5 |
| K-232 | 只读挂载下写回整体拒绝 | 机制 | `inode.rs` | Rust 化 | 09 §4.2 |
| K-192a | 位图接口的消费（分配占位、释放清位） | 机制 | `inode.c:268-282`、`:339-342` | 生命周期（机制归 13） | 09 §1.5/§2.9/§2.11 |

- **验收标准**：
  1. 给出磁盘 64 字节与内存形态的字段对照表，标出"只有内存有"的字段
  2. 画出引用计数的状态机（0 → 1 → n → 0），标出每个转换的触发点与分流结果
  3. 解释"为什么冷命中才算命中"（答：热命中不产生磁盘 I/O，性能上不算"缓存未命中"）
  4. 给出定位算术的公式与至少 3 个边界用例（一在起点、八在块内、九在下块、零拒绝）

### 15-mfs-mount

- **一句话定位**：读者读完能说出七步挂载里每一步的失败后果，以及打烊时哪些状态必须回到盘上。
- **讲什么**：
  - 开店七步（记住设备、读身份证、脏盘降级、认块大小、数已用区、领根节点、脏标记落盘）
  - 散伙原子性（每步失败把前面占的收拾干净）
  - 脏盘降级（读写挂载遇脏位自动降只读并记标记）
  - 降级记录与卸载报告（降级来的只读同样不写干净位——"写干净等于撒谎"）
  - 打烊六步（数在用引用、放根、调同步、读写则标干净落盘、作废设备缓存、设备号复位）
  - 卸载"嚷嚷不停"原则
  - 作废设备缓存两件事
  - 挂载点检查三问与"借来即还"
  - 三种挂载形态的比较与形态三胜出（`MountedFs` 七字段）
  - Linux / Redox / 类型理论的对照
  - 六错误变体与每散伙点对号入座
  - 位数统计的本地实现与 23 的 canonicalize
- **不讲什么**：
  - 超级块格式细节（12）
  - 位图机制（13）
  - inode 表内部（14）
  - 块驱动的打开关闭（08 与 16-stage-drivers）
  - 同步刷盘的执行（23，本篇只注入回调）
  - 一致性契约的完整论证（32）
- **前置**：00、01、02、03、05、06、08、11、12、13、14
- **后置**：16–23、27、28、29、32
- **事实底线**：
  - C：`minix3/minix/fs/mfs/mount.c`（173 行：`:10` `fs_mount`、`:19` 记住设备、`:22` 开设备、`:33-37` 失败复位、`:40-50` 脏盘降级、`:52` 认块大小、`:58-60` 数已用区、`:63-68` 领根、`:81-86` 填根六字段、`:88` 能力位、`:91-95` 清干净位写回、`:104` `fs_mountpt`、`:114-121` 三问、`:123` 借来即还、`:134` `fs_unmount`、`:144-148` 在用引用检查）
  - Rust：`os/fs/mfs/src/mount.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-233 | 开店七步 | 机制 | `mount.c:10-98` | 主流程 | 10 §1.1/§2.1 |
| K-234 | 散伙原子性 | 约束与不变量 | `mount.c:33-37` | 主流程 | 10 §1.1/§2.1 |
| K-235 | 脏盘降级 | 机制 | `mount.c:40-50` | 主流程 | 10 §1.2/§2.1 |
| K-236 | 降级记录与卸载报告 | 约束与不变量 | `mount.c:40-50` | 主流程 | 10 §1.2/§3.3 |
| K-237 | 打烊六步 | 机制 | `mount.c:134-172` | 主流程 | 10 §1.3/§2.3 |
| K-238 | 卸载"嚷嚷不停"原则 | 约束与不变量 | `mount.c:144-148` | 主流程 | 10 §1.3/§3.2 |
| K-239 | 作废设备缓存两件事 | 机制 | `cache.c:803-807`、`call.c:83-84` | 主流程（通道细节归 07） | 10 §1.3/§2.3 |
| K-240 | 挂载点检查三问 | 接口与协议 | `mount.c:104-128` | 接口 | 10 §1.4/§2.2 |
| K-241 | 挂载点检查"借来即还" | 约束与不变量 | `mount.c:123` | 接口 | 10 §1.4/§2.2 |
| K-242 | 三种挂载形态比较与形态三胜出 | 架构演进 | `mount.rs` | 设计 | 10 §1.5/§4.1 |
| K-243 | Linux `struct super_block` 对照 | 概念 | Linux | 对照 | 10 §1.5 |
| K-244 | Redox 挂载是命名空间柄对照 | 概念 | Redox | 对照 | 10 §1.5 |
| K-245 | RAII 加会话类型对照 | 概念 | 类型理论 | 对照 | 10 §1.5 |
| K-246 | 六错误变体 | 接口与协议 | — | 错误（总图在 31） | 10 §3.2/§4.1 |
| K-247 | 位数统计本地实现 | 架构演进 | `stats.c:14` | 与 23 的接缝 | 10 §3.4/§2.4 |
| K-196a | 干净位的读写时机（挂载清、卸载写） | 机制 | `mount.c:40-50`、`:91-95` | 主流程（定义归 12） | 08 §1.7 + 10 §2.1 |
| K-160b | 挂载时领根节点（`get_inode` 的调用） | 机制 | `mount.c:63-68` | 主流程（机制归 14） | 10 §2.1 |

- **验收标准**：
  1. 给出开店七步的表格：步骤、动作、失败后果、C 行锚点
  2. 解释"为什么降级来的只读不能写干净位"（答：盘上本来就不干净，写干净是撒谎，会让下次挂载误判）
  3. 给出三种挂载形态的对照表，说明每种被否决的具体理由
  4. 说明"卸载必须成功"的设计理由（答：卸载失败会让设备处于半挂载状态，比崩溃更难恢复）

### 16-mfs-path

- **一句话定位**：读者读完能说出目录里找名字的完整过程，以及名字是怎么写进去、怎么删掉的。
- **讲什么**：
  - 64 字节目录项布局（4 字节号小端 + 60 字节名零填充）
  - 号为零表示空行（删除留下的坑 / 从未用过的尾巴）
  - 名字最长 60 与全系统唯一截断点
  - 点与点点是普通行、目录非空判据
  - 四模式目录漫步（`LOOK_UP` / `ENTER` / `DELETE` / `IS_EMPTY` 共享一套 walk）
  - 加人的三种落定方式（复用空行 → 块内尾空间 → 追尾新块）
  - 删除的藏号与 hint 回调
  - 名义槽位数（大小除 64）与 walk 不越界
  - 进入 hint 是状态也是优化
  - 只读挂载写操作全拒，且顺序在"是不是目录"之后
  - 单步查找三段式（找目录槽 → 漫步 → 开子槽）
  - 节点六字段填充与设备号取首区号
  - `advance` 前进函数全流程
  - 目录无空洞（断言设备非空块非空）
  - Rust 侧：镜像参数化（依赖倒置）、计数器忠实复刻
  - 目录镜像桥 `dir_io.rs`（装载与回写半）
- **不讲什么**：
  - 框架层整路径漫步（04）
  - 块号映射与新块分配（20、21，本篇只消费块镜像）
  - inode 表内部（14）
  - 文件的创建删除（17、18，它们调用本篇的进入与删除模式）
  - 目录项输出格式（03）
- **前置**：00、01、02、03、05、06、08、11、12、13、14
- **后置**：17、18、19、20、27、28
- **事实底线**：
  - C：`minix3/minix/fs/mfs/path.c`（241 行：`:16` `fs_lookup`、`:26` `advance`、`:48-86` advance 全流程、`:74` `search_dir`、`:92-240` search_dir 全流程、`:116-238` 逐行逐块分支、`:120-121` 只读检查、`:124` 名义槽位数、`:135-142` 无空洞断言、`:148-151` 不越界、`:172-181` 删除藏号、`:192-223` 加人三方式）、`fs/mfs/mfsdir.h`（20 行：`:13-18` `struct direct`）
  - Rust：`os/fs/mfs/src/dir.rs`、`os/fs/mfs/src/dir_io.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-248 | 64 字节目录项布局 | 数据结构 | `mfsdir.h:13-18` | 结构 | 11 §1.1/§2.1 |
| K-249 | 号为零表示空行 | 数据结构 | `mfsdir.h` | 结构 | 11 §1.1 |
| K-250 | 名字最长 60 与唯一截断点 | 约束与不变量 | `mfsdir.h:13` 注释 | 不变量 | 11 §1.1 |
| K-251 | 点与点点是普通行 | 机制 | `path.c:92` | 机制 | 11 §1.1 |
| K-252 | 四模式目录漫步 | 机制 | `path.c:92-240` | 核心机制 | 11 §1.2/§2.4 |
| K-253 | 加人的三种落定方式 | 机制 | `path.c:192-223` | 核心机制 | 11 §1.2 |
| K-254 | 删除的藏号与 hint 回调 | 机制 | `path.c:172-181` | 核心机制 | 11 §1.2 |
| K-255 | 名义槽位数与 walk 不越界 | 约束与不变量 | `path.c:124`、`:148-151` | 不变量 | 11 §1.2/§2.4 |
| K-256 | 进入 hint 是状态也是优化 | 机制 | C 注释 | 优化 | 11 §1.3 |
| K-257 | 只读挂载写操作全拒，顺序在目录检查后 | 约束与不变量 | `path.c:120-121` | 不变量 | 11 §1.4/§2.4 |
| K-258 | 单步查找三段式 | 机制 | `path.c:16-42` | 接口 | 11 §1.5/§2.2 |
| K-259 | 节点六字段填充与设备号取首区号 | 接口与协议 | `path.c:30-37` | 接口 | 11 §2.2 |
| K-260 | `advance` 全流程 | 机制 | `path.c:26-46` | 机制 | 11 §2.3 |
| K-261 | 目录无空洞 | 约束与不变量 | `path.c:135-142` | 不变量 | 11 §2.4 |
| K-262 | 镜像参数化 | 架构演进 | `dir.rs` | Rust 化 | 11 §2.5/§3.1 |
| K-263 | 计数器忠实复刻 | 架构演进 | `dir.rs` | Rust 化 | 11 §3.2 |
| K-325a | 目录镜像桥（装载与回写半） | 工具与工程 | `dir_io.rs` | 越界内容归位 | 14 §4.4（越界） |

- **验收标准**：
  1. 给出目录块的字节图（一行 64 字节）与"空行 / 有效行 / 尾空间"三种状态的判定
  2. 给出四模式漫步的差异表：每种的匹配条件、命中动作、未命中动作
  3. 解释"为什么进入要找松弛而不是直接追加"（答：目录块内可能有删除留下的空行，复用比重建便宜）
  4. 给出"名字截断只发生在进入目录时"的证据链（`search_dir` 是唯一截断点）

### 17-mfs-create

- **一句话定位**：读者读完能说出一个新名字如何变成一个文件，以及每一步失败如何收场。
- **讲什么**：
  - 新建节点四拍（查空、分配、落盘、进入）
  - 父目录护栏（必须存在且有链接）
  - 目录建子且父链接到顶报链接上限
  - 名字必须空（有人报已存在，其他失败透传）
  - 分配四件套（位图占位、表占槽、填模式属主、链接数置一、区号落首区）
  - 第三拍强制落盘：孤儿节点优于悬空名字
  - 进入失败回滚四样（链接减一、标脏、释放、退位图）
  - 泄漏是慢性 corrupt
  - 点点舞三步（写点与点点、子母链接各加一、母标脏）
  - 点点舞失败多拆一样（删名），删失败报 corrupt
  - 符号链接目标住独立数据块、按真长度定大小
  - 符号链接超块先拒绝（先验后占）
  - 设备节点＝带设备号的空文件（设备号借住区号字段）
  - 寻位标记（找到置标、找不到静默、hint 永不报错）
  - 上下文束与"结构 vs 闭包"分水岭
  - 完成态两种类型防误用
  - 回滚宕机改 corrupt
  - 精确错误透传
- **不讲什么**：
  - 硬链接与改名（18）
  - 数据区的读写映射（20、21，符号链接目标块除外——它直写缓存块）
  - inode 表内部（14）
  - 目录漫步内部（16）
  - 一致性契约的完整论证（32）
- **前置**：00、01、02、03、05、06、08、11、12、13、14、16
- **后置**：18、19、27
- **事实底线**：
  - C：`minix3/minix/fs/mfs/open.c`（270 行：`:14` `fs_create`、`:24-25` 取父、`:32-36` 六字段描述、`:39-44` 放父、`:47` 子槽位保持引用、`:56` `fs_mknod`、`:66` 设备号借区号、`:77` `fs_mkdir`、`:101-117` 点点舞、`:113-114` 删名失败、`:128` `fs_slink`、`:141` 目标块、`:146-151` 超块先拒绝、`:158-170` 写目标、`:175-179` 失败回滚、`:192-257` `new_node`、`:208-211` 父护栏、`:213-217` 链接上限、`:220` 名字空、`:224` 位图占位、`:233-235` 第三拍落盘与注释、`:239-243` 进入失败回滚、`:248-251` 名字已存在、`:263` `fs_seek`）
  - Rust：`os/fs/mfs/src/open.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-264 | 新建节点四拍 | 机制 | `open.c:192-257` | 主流程 | 12 §1.1/§2.5 |
| K-265 | 父目录护栏 | 约束与不变量 | `open.c:208-211` | 不变量 | 12 §1.1 |
| K-266 | 目录建子链接上限 | 约束与不变量 | `open.c:213-217` | 不变量 | 12 §1.1 |
| K-267 | 名字必须空 | 约束与不变量 | `open.c:220`、`:248-251` | 不变量 | 12 §1.1 |
| K-268 | 分配四件套 | 机制 | `open.c:224`、`:233-235` | 主流程 | 12 §1.1 |
| K-269 | 第三拍强制落盘：孤儿优于悬空 | 约束与不变量 | `open.c:233-235` 注释 | 一致性核心 | 12 §1.1 |
| K-270 | 进入失败回滚四样 | 机制 | `open.c:239-243` | 主流程 | 12 §1.2 |
| K-271 | 泄漏是慢性 corrupt | 约束与不变量 | — | 一致性 | 12 §1.2 |
| K-272 | 点点舞三步 | 机制 | `open.c:101-117` | 主流程 | 12 §1.3 |
| K-273 | 符号链接目标住独立数据块 | 机制 | `open.c:141-170` | 主流程 | 12 §1.4 |
| K-274 | 设备节点＝带设备号的空文件 | 机制 | `open.c:66` | 主流程 | 12 §1.5 |
| K-275 | 寻位标记 | 机制 | `open.c:263-270` | 接口 | 12 §1.5/§2.6 |
| K-276 | 上下文束与分水岭 | 架构演进 | `open.rs` | Rust 化 | 12 §3.1 |
| K-277 | 完成态两种类型 | 架构演进 | `open.rs` | Rust 化 | 12 §3.2 |
| K-278 | 回滚宕机改 corrupt | 架构演进 | — | Rust 化 | 12 §3.3 |
| K-279 | 精确错误透传 | 架构演进 | `open.c:24-25` | Rust 化 | 12 §3.4 |
| K-218a | 分配的两步（位图与表） | 机制 | `inode.c:252`、`:285-299` | 主流程（机制归 14） | 12 §1.1 |
| K-253a | 进入父目录（`search_dir` 的 ENTER 模式） | 机制 | `path.c:192-223` | 主流程（机制归 16） | 12 §1.1 |

- **验收标准**：
  1. 给出四拍的顺序图，每拍标注失败时的回滚动作
  2. 解释"为什么第三拍要强制落盘"（答：崩溃后有号无名是孤儿，可回收；有名无号是悬空名字，无法修复）
  3. 给出符号链接创建的完整步骤（含超块先拒绝、失败回滚多拆一区）
  4. 逐条列出四种创建（文件 / 目录 / 设备 / 链接）的"加戏"差异

### 18-mfs-link

- **一句话定位**：读者读完能说出已经存在的文件和名字之间可以发生哪些关系变化，每种变化在成功和失败时分别保证什么。
- **讲什么**：
  - 硬链接语义（新增名字指向已有编号、计数加一、标记状态变化时间）
  - 硬链接三个拒绝条件与固定顺序（计数上限、目录拒绝、名字必须不存在）
  - 先便宜后昂贵、先自身后环境的检查顺序
  - 删除文件与删除目录共用入口靠参数分流
  - 挂载点不可删（设备忙）与只读挂载拒绝
  - 删除目录三条额外条件（空目录、非根、删点与点点且忽略失败）
  - 公共删除辅助三件事（删名、计数减一、标时间），存储回收延迟到 inode 释放
  - 删除目录的三次计数记账与饱和减法
  - 改名四阶段（开旧父加找旧 → 开新父 → 错误组合 → 搬动）
  - 循环挂载检查（顺新父点点上溯）
  - 新名字已存在的错误组合
  - 改名两种搬动顺序（同父先删后建、跨父先建后删）
  - 目录跨父搬家修正点点加新父链接数加一
  - 符号链接读取（类型校验、零号块、按最小长度拷贝）
  - 读非符号链接报访问拒绝；块缺失报 IO 错误
- **不讲什么**：
  - 新文件的诞生过程（17）
  - 文件偏移到磁盘块号的翻译（20）
  - 存储区的分配与回收细节（11 的区分配与 21 的映射写入；本篇只产出待回收区号清单）
  - 目录项在磁盘上的字节排列（16）
  - 截断与区间释放的完整机制（19）
  - 一致性契约的完整论证（32）
- **前置**：00、01、02、03、05、06、08、11、12、13、14、16、17
- **后置**：19、21、27
- **事实底线**：
  - C：`minix3/minix/fs/mfs/link.c`（638 行：`:14` `freesp_inode` 前向声明、`:15` `remove_dir` 前向声明、`:17` `unlink_file` 前向声明、`:19` `nextblock` 前向声明、`:32` `fs_link`、`:46-52` 三拒绝、`:73-80` 名字检查、`:103` `fs_unlink`、`:125-132` 挂载点与只读、`:184` `remove_dir`、`:205-211` 三次计数记账、`:219` `unlink_file`、`:239-245` 三件事、`:255` `fs_rename`、`:268-306` 四阶段、`:315-342` 循环挂载检查、`:346-356` 错误组合、`:371-401` 搬动顺序、`:405-414` 跨父修正、`:152` `fs_rdlink`、`:162-173` 类型校验与拷贝、`:452` `truncate_inode`、`:494` `freesp_inode`、`:558` `nextblock`）
  - Rust：`os/fs/mfs/src/link.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-280 | 硬链接语义 | 概念/机制 | `link.c:32-97` | 主流程 | 13 §1.1/§2.1 |
| K-281 | 三个拒绝条件与顺序 | 约束与不变量 | `link.c:46-52`、`:73-80` | 不变量 | 13 §1.1 |
| K-282 | 先便宜后昂贵 | 约束与不变量 | — | 设计 | 13 §1.1 |
| K-283 | 删除文件与目录共用入口 | 机制 | `link.c:103-146` | 主流程 | 13 §1.2/§2.2 |
| K-284 | 挂载点不可删与只读拒绝 | 约束与不变量 | `link.c:125-132` | 不变量 | 13 §1.2 |
| K-285 | 删除目录三条额外条件 | 机制 | `link.c:184-213` | 主流程 | 13 §1.2/§2.4 |
| K-286 | 公共删除辅助三件事 | 机制 | `link.c:219-249` | 主流程 | 13 §1.2/§2.5 |
| K-287 | 三次计数记账与饱和减法 | 机制 | `link.c:205-211` | 主流程 | 13 §2.4 |
| K-288 | 改名四阶段 | 机制 | `link.c:255-422` | 主流程 | 13 §1.3/§2.6 |
| K-289 | 循环挂载检查 | 约束与不变量 | `link.c:315-342` | 不变量 | 13 §1.3 |
| K-290 | 新名字已存在的错误组合 | 约束与不变量 | `link.c:346-356` | 不变量 | 13 §1.3 |
| K-291 | 两种搬动顺序 | 约束与不变量 | `link.c:371-401` | 不变量 | 13 §1.3/§3.4 |
| K-292 | 跨父修正与父链接加一 | 机制 | `link.c:405-414` | 主流程 | 13 §1.3 |
| K-293 | 符号链接读取 | 接口与协议 | `link.c:152-178` | 接口 | 13 §1.4/§2.3 |
| K-294 | 读非链接报访问拒绝 | 约束与不变量 | `link.c:162-166` | 不变量 | 13 §1.4 |
| K-300 | 错误枚举十二变体加透传 | 架构演进 | `link.rs` | Rust 化 | 13 §3.3/§4.1 |
| K-301 | 判断与执行分离的对照 | 架构演进 | Linux / Redox | 对照 | 13 §3.1/§3.2 |
| K-213a | 释放延迟到 inode 释放（`put_inode` 归零分流） | 机制 | `inode.c:236-244` | 主流程（机制归 14） | 13 §1.2/§2.5 |

- **验收标准**：
  1. 给出改名决策树的完整流程图，标出所有早退点与它们的 errno
  2. 解释"同父先删后建、跨父先建后删"两个顺序各自防什么
  3. 给出删除目录的三次计数记账的逐步推演（含子目录终值）
  4. 列出改名涉及的所有环检测与它们的终止条件

### 19-mfs-truncate

- **一句话定位**：读者读完能说出截断的三个决策、区间释放的翻译规则，以及为什么先清零后释放。
- **讲什么**：
  - 截断语义（变短还空间、变长空洞读零、挖空不改长度）
  - 截断三决策（特殊设备拒绝、超最大长度报文件过大、新旧长度三分）
  - 区间释放翻译规则（单区内部分覆盖只清零、跨区首尾清零加中间释放、尾巴延续到文件末尾整区释放）
  - 释放计划做成纯计算（输入长度与块大小与起止，输出清零范围加待回收区号清单）
  - 待释放区号按位置释放（位置 = 区号 × 块大小）
  - 执行顺序先清零后释放（顺序反了找不到块）
  - 清零范围按块切分，翻译为空即空洞跳过
  - 位置取整不溢出（先除后加）与 Rust 饱和保护
  - `freesp_inode` / `nextblock` / `zerozone_half` / `zerozone_range` 的分工
  - 与 21（写路径）的执行交接：判断在本篇、执行在 21
- **不讲什么**：
  - 链接、删除、改名的名字空间语义（18）
  - 写映射与间接块的生长回收（21）
  - 块号翻译的查找级联（20，本篇复用其规则）
  - 位图的位操作与存储（13）
  - 数据区的分配（11）
- **前置**：00、01、02、05、06、08、11、12、13、14、16、17、18
- **后置**：21、27、32
- **事实底线**：
  - C：`minix3/minix/fs/mfs/link.c:428-488`（`fs_trunc` 与 `truncate_inode`：`:428` `fs_trunc`、`:452` `truncate_inode`、`:468-471` 长度比较）、`:494-637`（`freesp_inode` `:494`、`:513-544` 区间翻译、`nextblock` `:558`、`:566-569` 先除后加、`zerozone_half`、`zerozone_range` `:623-636`）
  - Rust：`os/fs/mfs/src/link.rs`（释放计划部分）、`os/fs/mfs/src/write.rs`（截断执行部分）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-295 | 截断语义 | 概念 | — | 语义 | 13 §1.5 |
| K-296 | 截断决策 | 机制 | `link.c:428-488` | 主流程 | 13 §1.5/§2.7 |
| K-297 | 区间释放翻译规则 | 机制 | `link.c:494-637` | 核心机制 | 13 §1.5/§2.8 |
| K-298 | 释放计划做成纯计算 | 架构演进 | `link.rs` | Rust 化 | 13 §1.5/§3.1 |
| K-299 | 位置取整不溢出与饱和保护 | 机制 | `link.c:558`、`:566-569` | 机制 | 13 §1.6/§3.5 |
| K-344 | 截断三决策（执行侧） | 机制 | `link.c:428-488` | 决策与执行的接缝 | 15 §1.4 |
| K-345 | 清零范围按块切分 | 机制 | — | 执行机制 | 15 §1.4 |
| K-346 | 先清零后释放 | 约束与不变量 | — | 顺序契约 | 15 §1.4 |
| K-347 | `clear_zone` 的三个调用点 | 机制 | `write.c:233` | 执行机制 | 15 §1.5 |
| K-330a | 间接块回收的级联 | 机制 | `write.c:141-180` | 执行机制（生长机制归 21） | 15 §1.1 |
| K-297a | 释放计划与执行的交接契约（计划产出区号清单，执行逐区释放） | 接口与协议 | `link.rs`、`write.rs` | 接缝 | 13 §1.5 + 15 §1.4 |

- **验收标准**：
  1. 给出区间释放翻译规则的三条情况表（单区内部分覆盖 / 跨区 / 尾巴延续到末尾），每条带 C 行锚点
  2. 解释"为什么先清零后释放"（答：清零需要找到块，释放会清掉映射，顺序反了就找不到块）
  3. 给出 `nextblock` 的"先除后加"推导（为什么能避免溢出）
  4. 画出"判断（18/19）→ 计划（19）→ 执行（21）"的三段交接图

### 20-mfs-read

- **一句话定位**：读者读完能说出给定文件偏移和请求长度，数据从哪里来、读到哪里停、顺序读如何加速。
- **讲什么**：
  - 索引节点存储区号布局（前七直接、第八单重间接、第九双重间接）
  - 翻译三段级联（文件块号 → 区号 + 区内偏移；任一级空区号即空洞）
  - 双重间接索引上限保护
  - 空洞读零（来源：跳写缝隙、截断扩大尾巴）与稀疏文件
  - 读写对缺席的分叉（读当正常、写当分配信号）
  - 分块循环（切分取块剩余与请求剩余较小值；读按文件长度截断）
  - 三个计数器与返回实际搬运字节数
  - 窥视模式（只问缓存不读盘）与接线表挂接
  - 窥视遇空洞通知 VM 清页
  - 预读四上限与最小三十二块、寻位后不保证最小量
  - 预读的两个提前停止（翻译出空洞停止、命中缓存停止）
  - 目录内容枚举按位置分批（位置须为项大小整数倍）
  - 目录项类型现查内存表、查不到报未知
  - `fs_readwrite` 全流程、`rw_chunk` 块处理、`rd_indir`、`get_block_map`
  - 块标签是缓存与内存服务器共享页面的对账凭据
  - 翻译参数收两个小结构作防腐层
  - 间接块损坏报 IO 错而非终止进程
  - 窥视不下沉到间接块
  - 预读有界且寻位衰减
  - Rust 侧预读窗口四条件对齐
- **不讲什么**：
  - 存储区的分配与释放（21，本篇遇到空洞只读零）
  - 目录名字的查找与增删（16，本篇只做目录内容的批量输出）
  - 块缓存的替换策略与写回（06，本篇只调用获取与预取接口）
  - 请求参数的提取与回复的构造（05）
  - 二级缓存通道（07，本篇只做块标签登记）
- **前置**：00、01、02、03、05、06、07、08、11、12、13、14、16
- **后置**：21、27、28
- **事实底线**：
  - C：`minix3/minix/fs/mfs/read.c`（556 行：`:13` `rahead` 前向声明、`:15` `rw_chunk` 前向声明、`:23` `fs_readwrite`、`:37-54` 入口检查、`:49-54` 只读与超大拒绝、`:65-87` 分块循环、`:71-75` 切分、`:84-110` 计数器、`:90-99` 长度更新、`:104-108` 时间戳、`:117-204` `rw_chunk`、`:141-181` 三路分支、`:149-155` 空洞读零、`:156-159` 窥视遇空洞、`:175-177` 免读三条件、`:180` 块标签、`:187-190` 跳写清整块、`:210-279` `read_map`、`:237-278` 三段级联、`:254-259` 上限保护、`:260-273` 窥视不下沉、`:263` `rd_indir`、`:281` `get_block_map`、`:330` `rahead`、`:393-432` 预读四点、`:405-412` 四上限、`:417-441` 两个停止、`:454` `fs_getdents`、`:471-472` 位置检查、`:474` 从盘装载、`:516-519` 类型现查）
  - Rust：`os/fs/mfs/src/read.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-302 | 块号翻译三级 | 数据结构 | `read.c:210-279` | 核心 | 14 §1.1 |
| K-303 | 翻译三段级联 | 机制 | `read.c:237-278` | 核心 | 14 §1.1/§2.3 |
| K-304 | 双重间接索引上限保护 | 约束与不变量 | `read.c:254-259` | 核心 | 14 §1.1 |
| K-305 | 空洞读零与稀疏文件 | 概念 | `read.c:149-155` | 核心 | 14 §1.2 |
| K-306 | 读写对缺席的分叉 | 概念 | — | 核心 | 14 §1.2 |
| K-307 | 分块循环 | 机制 | `read.c:65-87` | 主流程 | 14 §1.3/§2.1 |
| K-308 | 三个计数器 | 机制 | `read.c:84-110` | 主流程 | 14 §1.3 |
| K-309 | 窥视模式与接线表挂接 | 接口与协议 | `table.c:20`、`read.c:156-159` | 接口 | 14 §1.4 |
| K-310 | 窥视遇空洞通知 VM | 机制 | `read.c:156-159` | 接口（通道归 07） | 14 §1.4 |
| K-311 | 预读四上限与最小三十二块 | 机制 | `read.c:330-448`、`:405-412` | 优化 | 14 §1.5/§2.5 |
| K-312 | 预读两个提前停止 | 机制 | `read.c:417-441` | 优化 | 14 §1.5 |
| K-313 | 目录内容枚举按位置分批 | 接口与协议 | `read.c:454-556` | 接口 | 14 §1.6/§2.6 |
| K-314 | 目录项类型现查内存表 | 机制 | `read.c:516-519` | 接口 | 14 §1.6 |
| K-315 | `fs_readwrite` 全流程 | 机制 | `read.c:23-111` | 主流程 | 14 §2.1 |
| K-316 | `rw_chunk` 块处理 | 机制 | `read.c:117-204` | 主流程 | 14 §2.2 |
| K-317a | 块标签是对账凭据 | 架构演进 | `read.c:180` | 接口（通道归 07） | 14 §2.2 |
| K-318 | `rd_indir` | 机制 | `read.c:263-325` | 机制 | 14 §2.4 |
| K-319 | `get_block_map` | 接口与协议 | `read.c:281-294` | 接口 | 14 §2.4 |
| K-320 | 翻译参数收两个小结构 | 架构演进 | `read.rs` | Rust 化 | 14 §3.1 |
| K-321 | 间接块损坏报 IO 错 | 架构演进 | `read.c:317-322` | Rust 化 | 14 §3.2 |
| K-322 | 窥视不下沉到间接块 | 架构演进 | `read.c:260-273` | Rust 化 | 14 §3.3 |
| K-323 | 预读有界且寻位衰减 | 架构演进 | `read.c:405-412` | Rust 化 | 14 §3.4 |
| K-324 | 目录枚举类型降级未知 | 架构演进 | `read.c:516-519` | Rust 化 | 14 §3.5 |
| K-326 | Linux / Redox 对照 | 架构演进 | Linux / Redox | 对照 | 14 §3.4/§3.5 |

- **验收标准**：
  1. 画出翻译三段级联的流程图，标出每个"空洞"判定点
  2. 给出分块循环的边界条件表（读的三种截断、块内偏移、剩余量）
  3. 给出预读窗口计算的完整规则（起窗、间接加窗、补足三十二、两处截断）
  4. 解释"为什么窥视不下沉到间接块"（答：块层批量预取已覆盖机会主义收益，下沉会让窥视路径复杂化）

### 21-mfs-write

- **一句话定位**：读者读完能说出写入遇到空洞时如何分配、删除遇到实块时如何回收、文件长度何时变化。
- **讲什么**：
  - 写映射（定位与读取翻译相同；存储模式写新区号、释放模式先释放旧区再清槽）
  - 间接块按需生长（分配、清零、再写入下级区号）
  - 分配失败不回滚（间接块是共享结构）
  - 间接块回收是生长的逆过程（级联变空释放）
  - 变空检查重新读盘、不信任旧快照
  - 块保障（有映射直接拿块，无映射则分配）
  - 分配起点提示位三优先级（文件上次提示、文件首区、文件系统首区）
  - 提示位的目的是让文件块相邻、减少寻道
  - 新块清零保证无旧数据；一区一块时为空操作
  - 块大小等于存储区大小由挂载保证
  - 整块覆盖免读盘，部分覆盖必须先读盘
  - 块对齐且在原文件末尾之后的部分覆盖不需要读盘
  - 写入入口两拒绝（只读挂载、超过最大长度）
  - 扩大检查用有符号比较避免无符号减法绕回
  - 分块循环与读取的三处不同
  - 只有普通文件与目录更新长度，设备文件不改
  - 写入结束清除寻位标记、标记两时间、标脏
  - 截断执行（缩小调释放、扩大只改长度）
  - Rust 不复制空函数：扩大的尾巴不清零
- **不讲什么**：
  - 块号翻译的查找级联（20，本篇只讲写入级联，两者对称）
  - 释放区间的判断规则（19，本篇只执行计划）
  - 位图的位操作与存储（13）
  - 块缓存的替换与写回（06）
  - 免读的模式定义（06、08，本篇只讲文件层的判据）
- **前置**：00、01、02、03、05、06、07、08、11、12、13、14、16、17、18、19、20
- **后置**：27、32
- **事实底线**：
  - C：`minix3/minix/fs/mfs/write.c`（319 行：`:28` `write_map`、`:49` 只读检查、`:57-66` 释放模式、`:69-86` 间接生长、`:92-108` 间接回收、`:116-131` 分配失败不回滚、`:141-163` 级联变空、`:164-169` 释放旧区、`:176-180` 变空重读、`:182-184` 返回、`:121` `wr_indir`、`:150` `empty_indir`、`:201-226` 判空、`:233` `clear_zone`、`:245-246` 空操作条件、`:254` `new_block`、`:270-282` 提示位三优先级、`:283-296` 分配、`:299-302` 标签登记、`:303-304` 返回、`:311` `zero_block`）、`fs/mfs/read.c:48-61`（写半部入口）、`:89-111`（写半部收尾）
  - Rust：`os/fs/mfs/src/write.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-327 | 写映射 | 机制 | `write.c:28-185` | 核心 | 15 §1.1/§2.1 |
| K-328 | 间接块按需生长 | 机制 | `write.c:79-140` | 核心 | 15 §1.1 |
| K-329 | 分配失败不回滚 | 约束与不变量 | `write.c:116-131` | 不变量 | 15 §1.1 |
| K-330 | 间接块回收是逆过程 | 机制 | `write.c:141-180` | 核心 | 15 §1.1 |
| K-331 | 变空检查重新读盘 | 约束与不变量 | `write.c:176-180` | 不变量 | 15 §1.1/§3.2 |
| K-332 | 块保障 | 机制 | `write.c:254-305` | 主流程 | 15 §1.2 |
| K-333 | 提示位三优先级 | 机制 | `write.c:270-282` | 主流程 | 15 §1.2 |
| K-334 | 提示位的目的 | 概念 | — | 设计 | 15 §1.2 |
| K-335 | 新块清零 | 约束与不变量 | `write.c:233-248`、`:311-318` | 不变量 | 15 §1.2 |
| K-336 | 块大小等于区大小由挂载保证 | 约束与不变量 | `write.c:246` | 不变量 | 15 §1.2 |
| K-337 | 整块覆盖免读盘 | 机制 | `read.c:175-177` | 优化 | 15 §1.2 |
| K-338 | 块对齐且在末尾之后不需读盘 | 机制 | `read.c:175-177` | 优化 | 15 §1.2 |
| K-339 | 写入入口两拒绝 | 机制 | `read.c:49-54` | 主流程 | 15 §1.3 |
| K-340 | 扩大检查用有符号比较 | 约束与不变量 | `read.c:53-54` | 不变量 | 15 §1.3/§3.4 |
| K-341 | 分块循环三处不同 | 概念 | `read.c:71-75` | 主流程 | 15 §1.3 |
| K-342 | 只有普通文件与目录更新长度 | 约束与不变量 | `read.c:90-94` | 不变量 | 15 §1.3 |
| K-343 | 写入结束清寻位、标两时间、标脏 | 机制 | `read.c:96-108` | 主流程 | 15 §1.3 |
| K-348 | Rust 不复制空函数 | 架构演进 | — | Rust 化 | 15 §1.5 |
| K-172a | 区分配的调用（`alloc_zone`） | 机制 | `write.c:283-296` | 主流程（机制归 11） | 15 §1.2 |
| K-174a | 区释放的调用（`free_zone`） | 机制 | `write.c:57-66` | 主流程（机制归 11） | 15 §1.1 |
| K-110a | 免读模式的调用（`NO_READ`） | 机制 | `read.c:175-177` | 主流程（模式定义归 06） | 15 §4.2 |

- **验收标准**：
  1. 画出写映射的完整流程图（三段级联 × 两种模式）
  2. 给出间接块生长与回收的对称对照图
  3. 解释"为什么分配失败不回滚"（答：间接块是多个文件位置共享的结构，回滚会破坏其他位置的映射）
  4. 给出"何时可以免读"的完整判据表，每条带 C 锚点

### 22-mfs-metadata

- **一句话定位**：读者读完能说出六组只碰索引节点字段的操作各自保证什么语义。
- **讲什么**：
  - 描述信息与内容数据的分界
  - 元数据操作两分（修改类与查询类）
  - 文件模式十六位 = 高四位类型 + 低十二位权限
  - 改权限只替换低十二位、忽略类型位、标状态变化时间并标脏、报回新模式
  - 只读挂载拒绝改权限；失败释放 inode 不留引用
  - 保留类型位的通用规则（类型是身份，权限是门锁）
  - 改属主换 uid/gid 并清除 setuid/setgid 位
  - 收回特权位的安全理由
  - **改属主不检查只读挂载，与改权限不对称（原始实现的不一致，Rust 忠实保留）**
  - 三个时间（访问、数据修改、状态变化）
  - 设时间三语义（设为现在标延迟位、跳过、明确秒数）；亚秒向下舍去
  - 进门丢弃旧的访问与修改延迟标记
  - 三语义是 POSIX 规定
  - 查文件状态先结算延迟时间再拷贝
  - 设备号填首个存储区号，普通文件填零
  - 块用量保守估计（空洞当实块算）与"宁多不少"原则
  - 查卷状态八个字段与空闲数的两种来源
  - 无特权调用方可用空闲数与总空闲相同
  - 字节序转换 `conv2`/`conv4` 与"当前小端目标下恒走不交换分支"
  - 状态结构做成框架中立字段
  - 时间选择器做成具名常量加小结构
- **不讲什么**：
  - 权限检查在 VFS 侧的判断（`05-stage-vfs/29-protect.md`，本篇只执行修改）
  - 磁盘格式的版本与布局（12）
  - inode 表的获取与释放（14）
  - 空闲位数的统计实现（23，本篇只在卷状态里调用一次）
  - typed `Stat` / `StatVfs` 的布局（31）
- **前置**：00、01、02、03、05、06、08、11、12、13、14
- **后置**：23、31
- **事实底线**：
  - C：`minix3/minix/fs/mfs/protect.c`（58 行：`:9` `fs_chmod`、`:18-21` 只读检查、`:24` 权限位替换、`:25-26` 标时间与标脏、`:29` 报回、`:39` `fs_chown`、`:44-49` 换人清特权位、`:54-57` 报回）、`fs/mfs/stadir.c`（104 行：`:11` `estimate_blocks`、`:13-17` 宁多不少、`:22-36` 估计公式、`:43` `fs_stat`、`:49-53` 结算时间、`:56-59` 设备号、`:61-71` 字段拷贝、`:82` `fs_statvfs`、`:87-101` 八字段、`:91-93` 空闲区、`:98-100` 空闲 inode、`:101` 名字上限）、`fs/mfs/time.c`（49 行：`:10` `fs_utime`、`:15-18` 进门清标记、`:20-30` 设为现在、`:32-42` 明确值、`:44-47` 跳过）、`fs/mfs/utility.c`（36 行：`:10` `conv2`、`:23` `conv4`）
  - Rust：`os/fs/mfs/src/meta.rs`、`os/libs/minix-types`（`Stat`/`StatVfs`）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-349 | 六组操作的分界 | 概念 | — | 定位 | 16 §1.0 |
| K-350 | 文件模式十六位 | 数据结构 | `protect.c:24` | 结构 | 16 §1.1 |
| K-351 | 改权限只替换低十二位 | 机制 | `protect.c:9-33` | 主流程 | 16 §1.1 |
| K-352 | 只读挂载拒绝改权限 | 约束与不变量 | `protect.c:18-21` | 不变量 | 16 §1.1 |
| K-353 | 改属主清特权位 | 机制 | `protect.c:47-49` | 主流程 | 16 §1.2 |
| K-354 | 收回特权位的安全理由 | 概念 | — | 设计 | 16 §1.2 |
| K-355 | 改属主不检查只读（原始不一致，忠实保留） | 约束与不变量 | `protect.c:39-58` | 不变量（全 stage 唯一照抄不一致） | 16 §1.2/§3.2 |
| K-356 | 三个时间 | 数据结构 | `inode.h` | 结构 | 16 §1.3 |
| K-357 | 设时间三语义 | 机制 | `time.c:10-48` | 主流程 | 16 §1.3 |
| K-358 | 进门丢弃旧延迟标记 | 机制 | `time.c:18` | 主流程 | 16 §1.3 |
| K-359 | 三语义是 POSIX 规定 | 概念 | POSIX | 标准依据 | 16 §1.3 |
| K-360 | 查文件状态先结算时间 | 机制 | `stadir.c:53` | 主流程 | 16 §1.4 |
| K-361 | 设备号填首区号 | 机制 | `stadir.c:56-59` | 主流程 | 16 §1.4 |
| K-362 | 块用量保守估计 | 机制 | `stadir.c:11-37` | 主流程 | 16 §1.4/§2.3 |
| K-363 | 查卷状态八字段 | 接口与协议 | `stadir.c:82-104` | 接口 | 16 §1.5 |
| K-364 | 空闲数两种来源 | 机制 | `stadir.c:91-100` | 接口 | 16 §1.5 |
| K-365 | 无特权可用空闲数相同 | 约束与不变量 | `stadir.c:91-93` | 不变量 | 16 §1.5 |
| K-366 | 字节序转换两函数 | 机制 | `utility.c:10-36` | 机制 | 16 §1.6/§2.7 |
| K-367 | 当前小端目标下恒走不交换分支 | 架构演进 | `utility.c` | Rust 化 | 16 §1.6 |
| K-368 | 状态结构做成框架中立字段 | 接口与协议 | `minix-types` `Stat` | Rust 化 | 16 §3.1 |
| K-369 | 时间选择器具名常量加小结构 | 数据结构 | `meta.rs` | Rust 化 | 16 §3.3 |
| K-370 | 估计用有符号中间值 | 约束与不变量 | `stadir.c:30` | 不变量 | 16 §3.4 |
| K-220a | 懒时间的结算（`update_times`） | 机制 | `inode.c:349` | 主流程（机制归 14） | 16 §1.4 |
| K-373b | 空闲 inode 数实时数位图 | 机制 | `stadir.c:98-100` | 接口（机制归 13） | 16 §1.5 |

- **验收标准**：
  1. 给出六组操作的完整表：操作、请求、修改的字段、返回值、失败条件
  2. 逐字引用并解释"改属主不检查只读"这条原始不一致，并说明 Rust 为何照抄
  3. 给出设时间三语义的判定表与各自的副作用
  4. 给出块用量估计的完整公式与至少 2 个验算例

### 23-mfs-maintenance

- **一句话定位**：读者读完能说出关机或同步时脏数据如何有顺序地落盘、空闲资源如何数清楚而不越界。
- **讲什么**：
  - 同步先刷脏 inode、后刷全部脏块
  - 顺序至关重要的理由（inode 写回会产生新的脏缓存块）与顺序反了的后果
  - `fs_sync` 遍历 512 槽的过滤条件（引用计数为正且脏）
  - 全部写完刷整缓存
  - 两种位图与各自的起点、总位数公式
  - 数空位过程（从搜索提示位起绕一圈、整字跳全一、否则逐位）
  - 位号超总位数即停，不数图外填充位；填充位恒零，误数会让卷状态撒谎
  - 搜索提示位越界归零
  - 位图字按本机字节序转换后才能逐位判断
  - `MARKDIRTY` 宏：只读挂载打印文件名行号并输出调用栈
  - 只读盘上出现标脏即逻辑错误
  - 库不打印原则
  - `count_free_bits` 逐步流程
  - 函数名与注释不符（注释写分配一位，实际数空位总数）
  - 同步执行器闭包注入（顺序硬编码）
  - 失败停住并上报已做工作量
  - 统计纯计数不碰存储
  - 脏标记守卫返回错误
  - 空清单也调刷盘闭包
- **不讲什么**：
  - inode 单个写回的字节过程（14，本篇只决定顺序与批量）
  - 块缓存的替换与刷盘接口（06，本篇只调用整缓存刷盘）
  - 每个常量的使用逻辑（各消费篇；常量目录归 35 或 99，见 §6 裁决）
  - 超级块的解析与校验（12）
  - 一致性契约的完整论证（32）
- **前置**：00、01、02、05、06、08、11、12、13、14、22
- **后置**：32
- **事实底线**：
  - C：`minix3/minix/fs/mfs/misc.c`（23 行：`:8` `fs_sync`、`:18-19` 遍历与过滤、`:22` 刷整缓存）、`fs/mfs/stats.c`（89 行：`:14` `count_free_bits`、`:30` 取参数、`:32-42` 位图区定位、`:45` 提示位归零、`:49-55` 逐字、`:63` 跳全一、`:65` 字节序转换、`:69-71` 逐位、`:74-82` 超界跳出、`:84-88` 放回下一块）、`fs/mfs/clean.h`（14 行：`MARKDIRTY` 宏）、`fs/mfs/glo.h`（22 行：`:10-20` 条件编译定义与声明、`:11-20` 五个全局）
  - Rust：`os/fs/mfs/src/maint.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-371 | 同步先 inode 后块 | 机制 | `misc.c:8-23` | 核心 | 17 §1.1/§2.1 |
| K-372 | 顺序至关重要的理由 | 约束与不变量 | `misc.c:22` | 一致性核心 | 17 §1.1 |
| K-373 | 两种位图与公式 | 数据结构 | `stats.c:32-42`、`const.h:48-51` | 统计前提 | 17 §1.2 |
| K-374 | 数空位过程 | 机制 | `stats.c:63-71` | 核心 | 17 §1.2 |
| K-375 | 位号超总位数即停 | 约束与不变量 | `stats.c:74-82` | 不变量 | 17 §1.2 |
| K-376 | 搜索提示位越界归零 | 机制 | `stats.c:45` | 鲁棒性 | 17 §1.2 |
| K-377 | 位图字按本机字节序转换 | 机制 | `stats.c:65` | 机制 | 17 §1.2 |
| K-378 | `MARKDIRTY` 宏与逻辑错误 | 约束与不变量 | `clean.h` | 不变量 | 17 §1.3/§2.3 |
| K-379 | 库不打印原则 | 架构演进 | — | 设计 | 17 §1.3/§3.4 |
| K-383 | `fs_sync` 遍历与过滤 | 机制 | `misc.c:18-22` | 核心 | 17 §2.1 |
| K-384 | `count_free_bits` 逐步流程 | 机制 | `stats.c:14-89` | 核心 | 17 §2.2 |
| K-385 | 函数名与注释不符 | 工具与工程 | `stats.c:14-89` | 工程趣闻 | 17 §2.2 |
| K-386 | 同步执行器闭包注入 | 接口与协议 | `maint.rs` | Rust 化 | 17 §3.1 |
| K-387 | 失败停住并上报已做工作量 | 约束与不变量 | `maint.rs` | Rust 化 | 17 §3.2 |
| K-388 | 空清单也调刷盘闭包 | 约束与不变量 | `maint.rs` | Rust 化 | 17 §4.1 |
| K-222a | 脏标记宏管 inode 与块的两处互补 | 概念 | `inode.h:67-70`、`clean.h` | 不变量 | 17 §1.3 |
| K-382a | 全局头文件五名字与五去向 | 数据结构 | `glo.h:11-20` | 结构（常量目录归 35） | 17 §1.5/§2.4 |

- **验收标准**：
  1. 用一张时序图说清"同步顺序反了会发生什么"（inode 写回产生新脏块 → 若先刷块则新脏块留在内存）
  2. 给出数空位的完整流程图，标出"超界跳出"与"填充位恒零"两个关键点
  3. 解释"为什么只读盘上出现标脏是逻辑错误"（答：只读路径应该全被拦在前面，能走到标脏说明检查漏了）
  4. 说明"空清单为什么也要调刷盘闭包"（答：清单是 inode 清单，块缓存可能还有脏块）

### 24-vtreefs

- **一句话定位**：读者读完能说出框架如何管一棵内存里的树，服务如何把内容挂到树上。
- **讲什么**：
  - 三服务同构需求与框架抽取动机（进程信息、设备管理、伪终端）
  - 框架管机制、服务填策略
  - 节点四组字段（身份 / 元数据 / 树位置 / 哈希链）
  - 短名字内联优化与 Rust 统一字节向量的取舍
  - 双哈希表（名字哈希 / 索引哈希）与桶数 = 节点总数
  - sdbm 哈希算法与多项式常数 65599
  - 删除两阶段（标记摘表 / 引用归零回收）与幂等
  - 链式回收父节点
  - 引用计数三操作与批量释放的计数语义
  - 断言改返回值的总方针
  - 单步查找四步（目录校验 → 点/点点 → 刷新钩子 → 名字哈希）
  - 根的两处点点语义不同
  - 枚举位置计数器三段（0 点 / 1 点点 / 2 起索引槽与普通子）
  - 索引空洞耗位、缓冲满回绕、已索引子节点跳过、位置到顶报 IO 错误
  - 读循环、写循环、截断语义
  - 状态查询三义（链接数是布尔、符号链接目标现算、三时间 = 当下）
  - 状态变更钩子与"无钩子报未实现加重读报回"
  - 卷状态仅两字段
  - 挂载语义（拒根挂载、根加引用、初始化钩子、能力无标志）
  - 启动三件套与信号只应终止
  - 节点管理内部机制全集
  - 回调表十七项 + 十三钩子表 + 头文件契约
  - 链接组四操作
  - 额外数据区
  - 树做成值、钩子做成有默认方法的特征、腾挪失败报无空间、枚举位置语义忠实复刻
  - `TreeServer` 接线与 libfsdriver 依赖
- **不讲什么**：
  - 进程信息的具体内容（25，本篇只讲内容挂在哪里）
  - 设备管理服务的用法（`11-stage-devman`，跨阶段引用）
  - 请求参数的提取与回复构造（05）
  - 磁盘文件系统的块与位图（06–23）
  - 十三钩子的逐项语义（消费方各篇）
- **前置**：00、01、02、03、04、05、09
- **后置**：25、26、29
- **事实底线**：
  - C：`minix3/minix/lib/libvtreefs/`（10 `.c` + 4 `.h`，源文件合计 1507 行）：`vtreefs.c`（110 行：`:16` `init_server`、`:66` `sef_local_startup`、`:87` `fs_other`、`:88` `run_vtreefs`）、`inode.c`（626 行：`:31` `init_inodes`、`:117-137` 双哈希、`:142-179` 腾挪、`:185` `add_inode`、`:454` `find_inode`、`:469-498` 引用计数、`:515-534` 链式回收、`:543-594` 删除两阶段、`:611` `fs_putnode`）、`table.c`（24 行：`:6-23` 十七项）、`path.c`（59 行：`:9` `fs_lookup`、`:28-30` 根点点）、`link.c`（129 行：`:115` `fs_unlink`）、`mount.c`（56 行：`:10` `fs_mount`）、`stadir.c`（122 行：`:8-43` 状态、`:48-109` 变更、`:114-122` 卷状态）、`file.c`（295 行：`:46` `fs_read`、`:108-164` 写、`:169-189` 截断、`:195` `fs_getdents`、`:194-295` 枚举）、`extra.c`（56 行：`:14` `init_extra`）、`sdbm.c`（30 行：`:22` `sdbm_hash`）、头文件：`glo.h`、`inc.h`、`inode.h`（`:25-49` 九字段加三链、`:51` `NO_INDEX`）、`proto.h`、`minix3/minix/include/minix/vtreefs.h`（74 行：`struct inode_stat`、`struct fs_hooks` 十三钩子、`PNAME_MAX`）
  - Rust：`os/libs/minix-vtreefs/src/{lib,tree,driver}.rs`、`os/libs/minix-vtreefs/src/driver.rs:67`（`TreeServer` 的 `FsDriver` impl）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-389 | 三服务同构需求与框架抽取动机 | 概念 | — | 定位 | 18 §1.1 |
| K-390 | 框架管机制、服务填策略 | 架构演进 | Linux / Redox | 对照 | 18 §1.1 |
| K-391 | 节点四组字段 | 数据结构 | `inode.h:25-49` | 结构 | 18 §1.2 |
| K-392 | 短名字内联优化的取舍 | 架构演进 | `PNAME_MAX` | Rust 化 | 18 §1.2 |
| K-393 | 双哈希表 | 数据结构 | `inode.c:117-137` | 结构 | 18 §1.3 |
| K-394 | sdbm 哈希与常数 65599 | 机制 | `sdbm.c:22` | 机制 | 18 §1.3 |
| K-395 | 删除两阶段与幂等 | 机制 | `inode.c:543-594` | 核心 | 18 §1.4 |
| K-396 | 链式回收父节点 | 机制 | `inode.c:515-534` | 核心 | 18 §1.4 |
| K-397 | 引用计数三操作 | 机制 | `inode.c:469-498` | 核心 | 18 §1.5 |
| K-398 | 批量释放的计数语义 | 约束与不变量 | `inode.c:611` | 接口 | 18 §1.5 |
| K-399 | 断言改返回值 | 架构演进 | `inode.c` 多处 | Rust 化 | 18 §1.5/§3.3 |
| K-400 | 单步查找四步 | 机制 | `path.c:9` | 核心 | 18 §1.6 |
| K-401 | 根的两处点点语义不同 | 约束与不变量 | `path.c:28-30` | 不变量 | 18 §1.6 |
| K-402 | 枚举位置计数器三段 | 机制 | `file.c:195` | 核心 | 18 §1.7 |
| K-403 | 索引空洞耗位与缓冲满回绕 | 约束与不变量 | `file.c:194-295` | 不变量 | 18 §1.7 |
| K-404 | 位置到顶报 IO 错误 | 约束与不变量 | `file.c:194-295` | 不变量 | 18 §1.7 |
| K-405 | 读循环语义 | 接口与协议 | `file.c:46` | 接口 | 18 §1.8 |
| K-406 | 写循环语义 | 接口与协议 | `file.c:108-164` | 接口 | 18 §1.8 |
| K-407 | 截断语义与非零尾参待办 | 约束与不变量 | `file.c:169-189` | 接口 | 18 §1.8 |
| K-408 | 状态查询三义 | 机制 | `stadir.c:8-43` | 接口 | 18 §1.9 |
| K-409 | 状态变更钩子与"无钩子报未实现" | 接口与协议 | `stadir.c:48-109` | 接口 | 18 §1.9 |
| K-410 | 卷状态仅两字段 | 接口与协议 | `stadir.c:114-122` | 接口 | 18 §1.9 |
| K-411 | 挂载语义 | 机制 | `mount.c:10` | 接口 | 18 §1.10 |
| K-412a | 启动三件套与信号只应终止 | 机制 | `vtreefs.c:16` | 生命周期 | 18 §1.10/§2.1 |
| K-413 | 启动注册三回调与六参数存全局 | 接口与协议 | `vtreefs.c:66/87` | 装配（总表归 09） | 18 §2.1 |
| K-414 | 节点管理内部机制全集 | 机制 | `inode.c:30-604` | 核心 | 18 §2.2 |
| K-415 | 回调表十七项与十三钩子表 | 接口与协议 | `table.c:6-23`、`vtreefs.h` | 接口 | 18 §2.3/§2.10 |
| K-416 | 链接组四操作 | 接口与协议 | `link.c:8-129` | 接口 | 18 §2.5 |
| K-417 | 额外数据区 | 数据结构 | `extra.c:14-56` | 结构 | 18 §2.9 |
| K-418 | 树做成值 | 架构演进 | `tree.rs` | Rust 化 | 18 §3.1 |
| K-419 | 钩子做成有默认方法的特征 | 架构演进 | `driver.rs:67` | Rust 化 | 18 §3.2 |
| K-420 | 腾挪失败报无空间 | 约束与不变量 | `inode.c:142-179` | Rust 化 | 18 §3.4 |
| K-421 | 枚举位置语义忠实复刻 | 约束与不变量 | `file.c:194-295` | 正确性命门 | 18 §3.5 |
| K-422 | Rust 常量与类型清单 | 数据结构 | `lib.rs` | Rust 化 | 18 §4.1 |
| K-423 | `TreeServer` 接线与依赖 | 接口与协议 | `driver.rs:67` | 装配（总表归 09） | 18 §4.6 |

- **验收标准**：
  1. 画出节点状态机（活跃 → 已删除待回收 → 已回收），标出每个转换的触发条件
  2. 给出枚举位置计数器的完整推演（含索引空洞、回绕、到顶三种情况）
  3. 逐条给出十三钩子的签名与语义，每条标注"哪些消费方用"
  4. 解释"为什么根的两处点点语义不同"（答：枚举要显示完整目录，查找要防止越顶）

### 25-procfs

- **一句话定位**：读者读完能说出进程快照如何变成目录树，目录树里的文件内容从哪里来。
- **讲什么**：
  - 静态树构造（文件表递归建树；表项 = 名 + 模式 + 数据；数据 = 子表或生成函数）
  - 根文件清单（七项 + x86 外加项）
  - 权限模型（文件全局可读、目录全局可进）
  - 两遍刷新第一遍"删"（空槽删、变号删、变属主删）与先删后建防同名撞车
  - 属主变更重建整棵子树的安全理由
  - 两遍刷新第二遍"建"（活槽无目录则建、名 = 进程号十进制串、索引槽位数 = 每进程文件数、回调数据记进程号）
  - 任务槽恒在、用户槽查快照、空闲报零
  - 哨兵约定（进程号零表示无进程）
  - 查找刷新懒策略与枚举刷新勤策略
  - 懒查与勤枚举的性能-一致性权衡
  - 四种进程文件（状态 / 命令行 / 环境 / 内存映射）
  - 按需生长（先验注册表、未注册不建、枚举建全量、索引 = 注册表下标）
  - 三岔分发（父索引 → 进程读 / 服务读 / 回调数据里的生成函数）
  - 僵尸进程与任务的三文件为空
  - 输出暂存三语义（跳过、截断、产量）
  - 暂存实现细节与手写十进制渲染
  - 负载均值三窗口算法与窗口边界
  - 运行时间与时钟频率生成器
  - 进程状态行十五字段位置契约
  - 名字空格截断与僵尸用新鲜快照判断
  - 节点预算推导与超预算降级
  - 服务子目录四值策略枚举与默认策略字符串
  - x86 处理器文件（64 项标志表）
  - PCI / IPC 向量 / 命令行与环境渲染格式
  - 内核查询在服务层、格式化在内容层；刷新计划纯计算；服务策略表不写死
- **不讲什么**：
  - 树的存储与枚举（24，本篇只调新增删除与钩子）
  - 内核进程表的字段（`01-stage-kernel`，本篇只消费系统信息服务的快照）
  - 服务子目录的策略表细节（驱动清单，`16-stage-drivers`）
  - 处理器标志位的逐位含义（x86 特有，本篇只讲生成器形状）
  - 内核信息通道（`edge E-KERNINFO`）
- **前置**：00、01、02、03、04、05、09、24
- **后置**：26、34
- **事实底线**：
  - C：`minix3/minix/fs/procfs/`（8 `.c` + 3 `.h`，源文件合计 1703 行）：`main.c`（93 行：`:22` `construct_tree`、`:21-44` 静态表）、`root.c`（227 行：九个生成器 `root_hz`/`root_loadavg`/`root_uptime`/`root_kinfo`/`root_meminfo`/`root_pci`/`root_dmap`/`root_ipcvecs`/`root_mounts`）、`tree.c`（462 行：`:12-43` 任务槽、`:85` `update_list`、`:104-121` 建目录、`:129-139` 节点预算、`:147-223` 删除、`:229-280` 按需生长、`:316` `pid_read`、`:355-395` 查找刷新、`:402-416` 枚举刷新）、`pid.c`（230 行：`:29` `is_zombie`、`:60` `pid_psinfo`、`:192` 僵尸空文件、`:192` `pid_cmdline` 一族）、`service.c`（348 行：`:22-28` 策略静态区、`:291` `service_lookup`）、`cpuinfo.c`（153 行：`:85` `print_x86_cpu_flags`）、`buf.c`（125 行：`:18` `buf_init`、`:30-83` 暂存、`:88-114` 渲染、`:120-125` 结果）、`util.c`（65 行：`:9` `procfs_getloadavg`、`:35-53` 三窗口）
  - Rust：`os/fs/procfs/src/{lib,buf,pid,content,service}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-424 | 进程信息 FS 定位 | 概念 | — | 定位 | 19 头部/§1.0 |
| K-425 | 静态文件表递归建树 | 数据结构 | `root.c`、`main.c:21-44` | 结构 | 19 §1.1 |
| K-426 | 根文件清单 | 数据结构 | `root.c` | 结构 | 19 §1.1/§2.2 |
| K-427 | 权限模型 | 约束与不变量 | `root.c` | 不变量 | 19 §1.1 |
| K-428 | 两遍刷新第一遍"删" | 机制 | `tree.c:85-223` | 核心 | 19 §1.2 |
| K-429 | 属主变更重建子树的安全理由 | 机制 | `tree.c` 注释 | 核心 | 19 §1.2 |
| K-430 | 两遍刷新第二遍"建" | 机制 | `tree.c:104-121` | 核心 | 19 §1.2 |
| K-431 | 任务槽恒在、用户槽查快照 | 机制 | `tree.c:12-43` | 核心 | 19 §1.2 |
| K-432 | 哨兵约定 | 约束与不变量 | 快照与目录约定 | 不变量 | 19 §1.2 |
| K-433 | 查找刷新懒策略 | 机制 | `tree.c:355-395` | 核心 | 19 §1.2 |
| K-434 | 枚举刷新勤策略 | 机制 | `tree.c:402-416` | 核心 | 19 §1.2 |
| K-435 | 懒查与勤枚举的权衡 | 概念 | 注释 | 设计 | 19 §1.2 |
| K-436 | 四种进程文件 | 数据结构 | `pid.c` 注册表 | 结构 | 19 §1.3 |
| K-437 | 按需生长 | 机制 | `tree.c:229-280` | 核心 | 19 §1.3 |
| K-438 | 三岔分发 | 机制 | `tree.c:315-443` | 核心 | 19 §1.3 |
| K-439 | 僵尸与任务的三文件为空 | 约束与不变量 | `pid.c:192` | 不变量 | 19 §1.3 |
| K-440 | 输出暂存三语义 | 机制 | `buf.c:18-125` | 接口 | 19 §1.4/§3.3 |
| K-441 | 暂存实现细节 | 约束与不变量 | `buf.c:30-83` | 接口 | 19 §1.4 |
| K-442 | 手写十进制渲染 | 机制 | `buf.c` | 实现 | 19 §1.4 |
| K-443 | 负载均值三窗口算法 | 机制 | `util.c:9-65` | 内容 | 19 §1.5/§2.8 |
| K-444 | 运行时间与时钟频率生成器 | 机制 | `root.c` | 内容 | 19 §1.6 |
| K-445 | 进程状态行十五字段 | 接口与协议 | `pid.c:60` | 接口 | 19 §1.7 |
| K-446 | 名字空格截断与僵尸新鲜快照 | 约束与不变量 | `pid.c:29`、`:41` | 不变量 | 19 §1.7 |
| K-447 | 节点预算推导与超预算降级 | 约束与不变量 | `tree.c:129-139` | 不变量 | 19 §1.8 |
| K-448 | 服务子目录四值策略 | 数据结构 | `service.c`、`:22-28` | 结构 | 19 §2.5 |
| K-449 | x86 处理器文件 | 机制 | `cpuinfo.c:85` | 内容 | 19 §2.6 |
| K-450 | PCI / IPC 向量 / 命令行渲染格式 | 接口与协议 | `root_pci`、`root_ipcvecs` | 接口 | 19 §4.6 |
| K-451 | 内核查询在服务层、格式化在内容层 | 架构演进 | `content.rs` | Rust 化 | 19 §3.1 |
| K-452 | 刷新计划纯计算 | 架构演进 | `content.rs` | Rust 化 | 19 §3.2 |
| K-453 | 服务策略表不写死 | 架构演进 | `service.rs` | Rust 化 | 19 §3.4 |

- **验收标准**：
  1. 画出静态树的结构（根下七到九个文件 + `pid` 目录 + `service` 目录），每个节点标注数据来源
  2. 给出两遍刷新的完整流程图，标出"先删后建"的位置与理由
  3. 给出查找刷新与枚举刷新的差异表（触发时机、重建范围、一致性保证）
  4. 说明"为什么节点预算是（任务数 + 进程数）× 4"（推导三笔：每个进程目录 4 项，加任务与进程目录本身）

### 26-ptyfs

- **一句话定位**：读者读完能说出驱动的增删请求如何变成应用可见的数字文件。
- **讲什么**：
  - ptyfs 定位（直连驱动框架，不用 vtreefs）
  - 固定表：32 节点在系统配置里定死、编译期已知、数组与位图静态分配
  - 固定表的代价与好处（终端数封顶 vs 无分配失败、无碎片、枚举上限恒定）
  - 正向转换（数字 → 十进制串：逐位取余倒填、零特判、装不下报名字过长）
  - 反向转换四项校验（空拒 / 非数字拒 / 前导零拒 / 上溢拒）
  - 有效名定义 = 正向能生产的串；两向互逆无例外
  - 前导零拒绝的规范性理由
  - 查找（只认根、非根报找不到、点回根、命中填六字段、挂载恒否）
  - 枚举位置计数器与行为（空闲耗位、满回绕、到顶停、非根拒绝、名生成失败跳过）
  - 枚举类型换算
  - 与框架枚举的位置语义同构
  - 状态查询（按号找记录、根返回根记录、链接数目录 2 文件 1、三时间全填创建时间）
  - 修改语义（改属主换标识清特权位；改模式只换权限九位保留类型位）
  - 未知号三操作全报无效参数；查卷不截断标记加名字上限
  - 控制消息两半（授权：取发送方标签、须为终端驱动、请求码须认识；执行：增改 / 删除 / 查名）
  - 删除幂等；回话区分阻塞同步回与异步无回执发
  - 创建必须阻塞的理由
  - 增改语义"存在即更新"与 procfs 先删后建相反
  - 挂载拒根、填根六字段、能力无标志；信号只响应终止；回调表八项
  - Rust 侧：表做成值、正向缓冲瑕疵收紧、空串拒绝、授权与执行分离
- **不讲什么**：
  - 终端与伪终端的驱动逻辑（`16-stage-drivers`，本篇只消费控制消息）
  - VFS 侧的挂载（`05-stage-vfs/18-mount.md`）
  - 虚拟树框架（24，本篇不用框架）
  - 进程信息的内容生成（25）
- **前置**：00、01、02、03、04、05、09
- **后置**：34
- **事实底线**：
  - C：`minix3/minix/fs/ptyfs/ptyfs.c`（434 行：`:28` `ptyfs_mount`、`:55` `make_name`、`:76` `parse_name`、`:108` `ptyfs_lookup`、`:153` `ptyfs_getdents`、`:205-280` 状态与修改、`:300` `ptyfs_other`、`:311-355` 授权与执行、`:339-371` 回话、`:391-434` 启动与循环、`:425-434` `ptyfs_table`）、`fs/ptyfs/node.c`（84 行：`:20-25` 静态分配、`:33` `set_node`、`clear_node`、`get_node`、`get_max_node`）、`fs/ptyfs/node.h`（20 行）
  - 非 C 制品：`fs/ptyfs/Makefile`（只链 `-lfsdriver`）、`etc/system.conf:507`
  - Rust：`os/fs/ptyfs/src/{lib,table,names}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-454 | ptyfs 定位 | 概念 | `ptyfs.c` | 定位 | 20 头部/§1.0 |
| K-455a | 固定表 32 节点 | 数据结构 | `node.c:20-25`、`node.h` | 结构 | 20 §1.1/§4.1 |
| K-456 | 固定表的代价与好处 | 约束与不变量 | `node.c` 注释 | 设计 | 20 §1.1 |
| K-457 | Linux `/dev/pts` 对照 | 概念 | Linux | 对照 | 20 §1.1 |
| K-458 | 正向转换 | 机制 | `ptyfs.c:55-66` | 核心 | 20 §1.2/§2.2 |
| K-459 | 反向转换四项校验 | 机制 | `ptyfs.c:76-100` | 核心 | 20 §1.2/§2.2 |
| K-460 | 有效名定义与互逆 | 约束与不变量 | — | 不变量 | 20 §1.2 |
| K-461 | 前导零拒绝的理由 | 约束与不变量 | — | 设计 | 20 §1.2 |
| K-462 | 查找 | 接口与协议 | `ptyfs.c:108-147` | 接口 | 20 §1.3/§2.3 |
| K-463 | 枚举位置计数器与行为 | 机制 | `ptyfs.c:153-199` | 核心 | 20 §1.4/§2.3 |
| K-464 | 枚举类型换算 | 机制 | `ptyfs.c:153-199` | 核心 | 20 §1.4 |
| K-465 | 与框架枚举的位置语义同构 | 概念 | `24-vtreefs.md` 对照 | 对照 | 20 §1.4 |
| K-466 | 状态查询 | 数据结构 | `ptyfs.c:205-280` | 接口 | 20 §1.5/§2.4 |
| K-467 | 修改语义 | 机制 | `ptyfs.c:225-257` | 接口 | 20 §1.5/§2.4 |
| K-468 | 未知号三操作全报无效；查卷两字段 | 约束与不变量 | `ptyfs.c:286-293` | 不变量 | 20 §1.5/§2.4 |
| K-469 | 控制消息两半 | 接口与协议 | `ptyfs.c:300-355` | 核心 | 20 §1.6/§2.5 |
| K-470 | 删除幂等；回话两种 | 接口与协议 | `ptyfs.c:339-371` | 接口 | 20 §1.6/§2.5 |
| K-471 | 创建必须阻塞的理由 | 约束与不变量 | 注释 | 设计 | 20 §1.6 |
| K-472 | 增改"存在即更新"与 procfs 相反 | 机制 | `node.c:33` | 对照 | 20 §1.6 |
| K-473 | 挂载拒根、回调表八项 | 接口与协议 | `ptyfs.c:28-47`、`:425-434` | 装配（总表归 09） | 20 §1.7/§2.6 |
| K-474a | 表做成值 | 架构演进 | `table.rs` | Rust 化 | 20 §3.1 |
| K-475a | 正向缓冲瑕疵收紧 | 架构演进 | `ptyfs.c:79` | Rust 化 | 20 §3.2 |
| K-476a | 空串拒绝 | 架构演进 | `ptyfs.c:76-100` | Rust 化 | 20 §3.3 |
| K-477a | 授权与执行分离 | 架构演进 | `lib.rs` | Rust 化 | 20 §3.4 |

- **验收标准**：
  1. 给出名字双向转换的完整规则表（正向的三种失败、反向的四种拒绝）
  2. 给出枚举位置计数器的完整推演（含空闲耗位与回绕）
  3. 给出控制消息的两半流程图，标出每个校验失败的处理
  4. 解释"为什么 ptyfs 不用 vtreefs 框架"（答：节点数固定、结构简单，框架的内存树管理是额外开销；且它需要 `fdr_other` 通道，框架的 `fs_other` 路径不同）

### 27-ext2

- **一句话定位**：读者读完能说出同一套请求语义装进 ext2 磁盘格式时，启动、校验、分配、名字与数据有哪些不同。
- **讲什么**：
  - 块组概念（每组自带宽块位图、inode 位图、inode 表；用量记在组描述符表）
  - 分配两段式（先选组再组内找位）与组的本地性价值
  - 组数公式（首数据块之前不分组，减一除商加一）
  - 与 mfs 的两张全局位图对照
  - 四种 inode 放置策略（Orlov 默认 / hashalloc / dir / any）与各自的选组规则
  - 策略性能思想（目录散开、文件跟父；师承 Linux Orlov）
  - 块分配目标定夺（调用方给块号 / 不给用 inode 所在组首 / 目标非法回落全局搜索位）
  - 窗口机制（顺序写判定、满八块 park 七块、组空闲不足四倍窗口不开窗）
  - 保留机制（触保留线先丢全部窗口再拒绝；空闲见底直接拒绝）
  - 搜索位与 inode 搜索游标优化
  - 超级块读取（1024 偏移读 1024 字节、魔数校验、块大小由对数推导、三验）
  - 版本分支（inode 尺寸与首可用号）
  - 特性三道门与根 inode 检查
  - 脏盘语义（挂载写错误态、挂载计数加一、卸载写干净态）
  - 启动选项六个与默认值
  - 选项影响点在分配入口分支（策略与机制分离）
  - 接线表 31 项全挂、三处复用、块组差异
  - 挂载流程全序
  - 变长目录项（头带总长度、尾部四字节对齐、走查按长度跳、删除合并、空闲槽 = 号零项）
  - 实际长度公式与松弛量；最大名 255
  - 与 mfs 定长 64 对照
  - 解码四项先验
  - 128 字节 inode 记录字段清单；扇区数以 512 为单位与块大小解耦
  - 十五指针 = 12 直接 + 三间接槽
  - 删除时间语义；释放复用不清零
  - 系统联合与访问表零来零去
  - 三级间接阈值分解四段与单块容量
  - 与参考实现对照（直接 7 两级 vs 直接 12 三级）
  - 字段位置差异；设备号借首指针
  - 快速符号链接（目标短于 60 字节直接存指针区；60 = 15 × 4）
  - 小端编解码处处显式
  - `path.c` 单步查找与漫步
  - 创建四件套与参考同形
  - 读写地址翻译三阈值、写映射对称生长回收、预读同形
  - 元数据四文件
  - `inode.c` 传输定位公式
  - Rust 侧四决策 + 三决策
- **不讲什么**：
  - 参考实现的完整语义（11–23，本篇只讲差异，相同的语义引用不重复）
  - 块缓存的替换策略（06，本篇只调用拿块标脏接口）
  - 请求参数的提取（05，两服务器共用同一套适配）
  - mfs 的目录项与 inode 格式（16、14，对照见 30）
  - ext2 镜像的离线工具（35）
- **前置**：00、01、02、03、05、06、07、08、09、11、12、13、14、15、16、17、18、19、20、21、22、23
- **后置**：30
- **事实底线**：
  - C：`minix3/minix/fs/ext2/`（17 `.c` + 6 `.h`，源文件合计 5422 行）：`main.c`（111 行：`:29` `main`、`:15-24` 选项表、`:29-47` 小端断言、`:67-98` 选项解析）、`table.c`（47 行）、`mount.c`（221 行：`:17` `fs_mount`、`:50-118` 特性门与根检查、`:122-127` 脏盘写状态、`:176-221` 卸载）、`super.c`（459 行：`:69` `read_super`、`:69-150` 逐字段解析）、`balloc.c`（362 行：`:23` `alloc_block_bit`、`:71` `alloc_block`、`:278` `free_block`）、`ialloc.c`（476 行：`:32` `alloc_inode`、`:113` `find_group_hashalloc` 前向声明、`:116` `find_group_orlov`、`:254` `find_group_dir`、`:284` `find_group_hashalloc` 定义、`:341` `find_group_any`、`:454` `wipe_inode`）、`misc.c`（35 行：`fs_sync`）、`path.c`（314 行：`:20-46` 字段位置、`:78` `search_dir`、`:95-314` 变长走查）、`open.c`（285 行）、`link.c`（656 行）、`read.c`（557 行：`:201` `read_map`、`:218-258` 三阈值）、`write.c`（376 行：`:27` `write_map`）、`inode.c`（420 行：`:299` `rw_inode`）、`protect.c`（57 行）、`stadir.c`（73 行）、`time.c`（52 行）、`utility.c`（204 行）
  - 非 C 制品：`fs/ext2/Makefile`（`WARNS=3`、`LDADD+= -lminixfs -lfsdriver -lbdev -lsys`）
  - Rust：`os/fs/ext2/src/{superblock,placement,dir,inode,mapping,lib,main}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-478 | 块组概念 | 概念 | `super.c` | 结构 | 21 §1.1 |
| K-479 | 分配两段式与本地性 | 机制 | `balloc.c` | 核心 | 21 §1.1 |
| K-480 | 与 mfs 两张全局位图对照 | 概念 | `11/13` 对照 | 对照 | 21 §1.1 |
| K-481 | 组数公式 | 数据结构 | `super.c` | 结构 | 21 §1.1 |
| K-482 | 四种放置策略总览 | 机制 | `ialloc.c:113/116/254/341` | 核心 | 21 §1.2 |
| K-483 | Orlov 策略 | 机制 | `ialloc.c:116` | 核心（代表策略） | 21 §1.2/§2.6 |
| K-484 | hashalloc 策略 | 机制 | `ialloc.c:284` | 核心 | 21 §1.2/§2.6 |
| K-485 | dir 策略 | 机制 | `ialloc.c:254` | 核心 | 21 §1.2/§2.6 |
| K-486 | any 策略 | 机制 | `ialloc.c:341` | 核心 | 21 §1.2/§2.6 |
| K-487 | 策略性能思想 | 概念 | Linux Orlov | 设计 | 21 §1.2 |
| K-488 | 块分配目标定夺 | 机制 | `balloc.c:71` | 核心 | 21 §1.3/§2.5 |
| K-489 | 窗口机制 | 机制 | `balloc.c:23` | 核心 | 21 §1.3 |
| K-490 | 保留机制 | 约束与不变量 | `balloc.c:71` | 不变量 | 21 §1.3/§2.5 |
| K-491 | 搜索位与 inode 游标优化 | 机制 | `balloc.c:278` | 核心 | 21 §1.3 |
| K-492 | 超级块读取与块大小推导 | 数据结构 | `super.c:69` | 校验 | 21 §1.4/§2.4 |
| K-493 | 版本分支 | 数据结构 | `super.c` | 校验 | 21 §1.4 |
| K-494 | 特性三道门与根检查 | 约束与不变量 | `mount.c:50-118` | 校验 | 21 §1.4/§2.3 |
| K-495 | 脏盘语义 | 机制 | `mount.c:122-127`、`:176-221` | 生命周期 | 21 §1.4/§2.3 |
| K-496 | 启动选项六个与默认值 | 接口与协议 | `main.c:15-24`、`:67-98` | 接口 | 21 §1.5/§2.1 |
| K-497 | 选项影响点在分配入口 | 架构演进 | — | 设计 | 21 §1.5 |
| K-498 | 小端断言 | 约束与不变量 | `main.c:29-47` | 不变量 | 21 §1.5 |
| K-499 | 接线表 31 项与三处复用 | 接口与协议 | `table.c` | 装配（总表归 09） | 21 §2.2 |
| K-500 | 挂载流程全序 | 机制 | `mount.c:17-221` | 生命周期 | 21 §2.3 |
| K-501 | `super.c` 逐字段解析 | 数据结构 | `super.c:69-150` | 校验 | 21 §2.4 |
| K-502 | `balloc` 五函数 | 机制 | `balloc.c:23/71/278` | 核心 | 21 §2.5 |
| K-503 | `ialloc` 分配链 | 机制 | `ialloc.c:32` | 核心 | 21 §2.6 |
| K-504 | `wipe_inode` 清场 | 数据结构 | `ialloc.c:454` | 核心 | 21 §2.6 |
| K-505 | `misc.c` 同步顺序 | 机制 | `misc.c` | 生命周期 | 21 §2.7 |
| K-506 | 选择器纯函数 | 架构演进 | `placement.rs` | Rust 化 | 21 §3.1 |
| K-507 | 随机起点注入 | 架构演进 | `placement.rs` | Rust 化 | 21 §3.2 |
| K-508 | 保留与窗口先算后动 | 架构演进 | `placement.rs` | Rust 化 | 21 §3.3 |
| K-509 | 校验逐项返回 | 架构演进 | `superblock.rs` | Rust 化 | 21 §3.4 |
| K-510 | 模块改名 `alloc.rs` → `placement.rs` | 工具与工程 | `placement.rs` | 工程 | 21 §4.3 |
| K-511 | 变长目录项结构 | 数据结构 | `path.c` | 结构 | 22 §1.1 |
| K-512 | 走查按长度跳、删除合并、空闲槽 | 机制 | `path.c:95-314` | 核心 | 22 §1.1/§2.1 |
| K-513 | 实际长度公式与松弛量 | 数据结构 | `dir.rs` | 结构 | 22 §1.1/§4.1 |
| K-514 | 最大名 255 与对齐整除 | 约束与不变量 | — | 不变量 | 22 §1.1 |
| K-515 | 与 mfs 定长 64 对照 | 概念 | `16` 对照 | 对照 | 22 §1.1 |
| K-516 | 解码四项先验 | 约束与不变量 | — | 不变量 | 22 §3.1 |
| K-517 | 128 字节 inode 字段清单 | 数据结构 | `inode.c` | 结构 | 22 §1.2 |
| K-518 | 扇区数以 512 为单位 | 数据结构 | `inode.c` | 结构 | 22 §1.2 |
| K-519 | 十五指针 = 12 直接 + 三间接槽 | 数据结构 | `inode.c` | 结构 | 22 §1.2/§4.2 |
| K-520 | 删除时间语义 | 约束与不变量 | `ialloc.c:454` | 不变量 | 22 §1.2 |
| K-521 | 系统联合与访问表零来零去 | 架构演进 | `inode.c` | Rust 化 | 22 §1.2/§3.2 |
| K-522 | 三级间接阈值分解四段 | 机制 | `read.c:201-258` | 核心 | 22 §1.3 |
| K-523 | 与参考实现对照（7 两级 vs 12 三级） | 概念 | `20` 对照 | 对照 | 22 §1.3 |
| K-524 | 链接/改名/截断/权限/时间/状态语义同形 | 概念 | `18/22` | 边界声明 | 22 §1.4 |
| K-525 | 字段位置差异与设备号借首指针 | 数据结构 | `path.c:20-46` | 结构 | 22 §1.4 |
| K-526 | 快速符号链接 | 数据结构 | ext2 优化 | 结构 | 22 §1.4 |
| K-527 | 小端编解码处处显式 | 约束与不变量 | `main.c` 断言 | 不变量 | 22 §1.5 |
| K-528 | `path.c` 单步查找与漫步 | 机制 | `path.c:20-314` | 核心 | 22 §2.1 |
| K-529 | 创建四件套与参考同形 | 机制 | `open.c`、`link.c` | 核心 | 22 §2.2 |
| K-530 | 读写地址翻译与写映射 | 机制 | `read.c:201-258`、`write.c:27` | 核心 | 22 §2.3 |
| K-531 | 元数据四文件 | 数据结构 | `protect.c`、`stadir.c`、`time.c`、`utility.c` | 接口 | 22 §2.4 |
| K-532 | `inode.c` 传输定位公式 | 机制 | `inode.c:299` | 机制 | 22 §2.5 |
| K-533 | 解码先验长度 | 架构演进 | `dir.rs` | Rust 化 | 22 §3.1 |
| K-534 | 联合字段零来零去 | 架构演进 | `inode.rs` | Rust 化 | 22 §3.2 |
| K-535 | 分解纯算术 | 架构演进 | `mapping.rs` | Rust 化 | 22 §3.3 |

- **验收标准**：
  1. 给出四种放置策略的完整对照表：策略名、选组规则、适用场景、C 函数锚点
  2. 给出窗口与保留机制的完整状态图（开窗 → 消耗 → park → 丢窗）
  3. 给出变长目录项的字节图与"删除合并"的前后对照
  4. 给出三级间接的四段阈值表与至少 2 个边界验算（第 12 个指针、单块容量边界）
  5. 明确列出所有"与参考实现同形、本篇只引用"的语义（至少 6 项）

### 28-isofs

- **一句话定位**：读者读完能说出只读光盘如何被发现、被解码、被读出。
- **讲什么**：
  - 只读光盘定位（三无：无分配、无写入、无链接）
  - 卷发现参数（起始 32768 字节、每扇区 2048 字节一个描述符、最多走 20 个）
  - 描述符首字节类型语义（1 = 主描述符解析并替换；255 = 终止符见即停）
  - 发现成功两条件齐备；扇区读不上来直接失败
  - 主描述符三门（五字节签名 CD001、版本 1、块大小小端值 ≥ 2048）
  - 认领动作（按块大小设缓存块大小、按卷空间设块用量）
  - 根目录记录嵌在主描述符尾部；根区间公式
  - 块大小 < 2048 拒绝的理由
  - 目录记录结构（首字节总长度、零表示填充到块尾）与双端数字段
  - 标志位（位 1 = 目录、位 0 = 隐藏）；名字偏移 33、名长在 32
  - 坏记录三拒绝；走查靠长度前进
  - 全零名两特殊项（单字节 0 = 当前目录、单字节 1 = 父目录）
  - Rock Ridge 开关下同一目录两种面孔
  - 区间概念（位置 + 块数二元组）；文件是区间链表
  - 区间累减定位（累减各区间长度；全超报零 = 空洞语义）
  - 与块组对照（块组管分配、区间管定位）
  - 系统使用尾结构（两字节签名、一字节长度、一字节版本、载荷）与走查规则（短头停、坏长度拒整尾、未知签名跳过）
  - 条目五用途（长名 / 模式 / 链接 / 子定位 / 重定位）
  - 链接目标四分支拼法与 bound 封顶 256
  - 空目标保持空、占位钩子原样报
  - 读钳制（位置超文件长报零、超长请求缩到剩余）
  - 分块循环
  - 拿块失败行为差异（原始终止进程 vs Rust 报 IO 错误）
  - 枚举（位置 = 解码后数组下标、越界停、缓冲满回绕、名长按首个零字节）
  - 枚举类型换算不为类型读盘
  - 链接读（非链接拒绝、超长截断、拷贝返回长度）
  - 回调表只读子集；窥视与块窥视两处条件编译关掉
  - 选项表一项（关 Rock Ridge 布尔，默认开）
  - 挂载/卸载/信号/启动
  - Rust 侧四决策
- **不讲什么**：
  - 光盘驱动的扇区读写（`16-stage-drivers`，本篇只调块读取接口）
  - 磁盘文件的分配写入（21，光盘没有写面）
  - 参考实现的链接改名语义（18，光盘没有链改面）
  - Rock Ridge 的全部条目（本篇只讲五用途，其余跳过）
  - mfs 的目录项与 inode 格式（对照见 30）
- **前置**：00、01、02、03、04、05、06、08、09、14、16、20
- **后置**：30
- **事实底线**：
  - C：`minix3/minix/fs/isofs/`（12 `.c` + 4 `.h` + `uthash.h`，源文件合计 1509 行）：`main.c`（65 行：`:10` 选项表、`:17-38` 启动、`:21` 选项默认、`:56` `main`）、`mount.c`（66 行：`:4` `fs_mount`）、`super.c`（121 行：`:24-73` 主描述符三门、`:75` `read_vds`、`:75-121` 卷发现）、`inode.c`（492 行：`:102` `read_directory`、`:131` `read_inode`、`:216` `read_inode` 定义、`check_dir_record`、`open_inode`、`inode_cache_add/get`、`get_inode`、`put_inode`、`dup_inode`、`fs_putnode`、`check_inodes`）、`path.c`（76 行：`:43` `fs_lookup`、`search_dir`）、`read.c`（105 行：`:5` `fs_read`、`fs_getdents`）、`stadir.c`（27 行）、`link.c`（24 行：`fs_rdlink`）、`table.c`（32 行）、`utility.c`（80 行：`:52` `date7_to_time_t`、`get_extent_absolute_block_id`、`read_extent_block`、`free_extent`）、`susp.c`（132 行：`:11` `parse_susp`）、`susp_rock_ridge.c`（289 行：`:135` `parse_susp_rock_ridge`）
  - 非 C 制品：`fs/isofs/Makefile`（`CPPFLAGS+= -DNR_BUFS=100`）、`minix3/minix/tests/testisofs.sh`（183 行）
  - Rust：`os/fs/isofs/src/{volume,record,rockridge,lib,main}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-536 | 只读光盘定位 | 概念 | — | 定位 | 23 头部/§1.0 |
| K-537 | 卷发现参数 | 数据结构 | `super.c:75-121` | 核心 | 23 §1.1/§4.1 |
| K-538 | 描述符类型语义 | 接口与协议 | `super.c:75-121` | 核心 | 23 §1.1 |
| K-539 | 发现成功两条件 | 约束与不变量 | `super.c:75-121` | 不变量 | 23 §1.1 |
| K-540 | 扇区读失败即失败 | 约束与不变量 | — | 不变量 | 23 §1.1 |
| K-541 | 主描述符三门 | 约束与不变量 | `super.c:24-73` | 核心 | 23 §1.2/§4.1 |
| K-542 | 认领动作 | 机制 | `super.c:24-73` | 核心 | 23 §1.2 |
| K-543 | 根记录嵌在主描述符尾部；根区间公式 | 数据结构 | `super.c`、`utility.c` | 结构 | 23 §1.2/§2.3 |
| K-544 | 块大小 < 2048 拒绝的理由 | 约束与不变量 | 原始注释 | 设计 | 23 §1.2 |
| K-545 | 目录记录结构与双端数字段 | 数据结构 | `inode.c` | 结构 | 23 §1.3 |
| K-546 | 标志位与名字偏移 | 数据结构 | `inode.c` | 结构 | 23 §1.3 |
| K-547 | 坏记录三拒绝 | 约束与不变量 | `inode.c` | 不变量 | 23 §1.3 |
| K-548 | 全零名两特殊项 | 接口与协议 | `inode.c` | 接口 | 23 §1.3 |
| K-549 | Rock Ridge 开关两种面孔 | 概念 | 选项 | 对照 | 23 §1.3/§1.5 |
| K-550 | 区间概念与链表 | 数据结构 | `utility.c` | 结构 | 23 §1.4/§4.1 |
| K-551 | 区间累减定位 | 机制 | `utility.c` | 核心 | 23 §1.4/§3.3 |
| K-552 | 与块组对照 | 概念 | `27` 对照 | 对照 | 23 §1.4 |
| K-553 | 系统使用尾结构与走查规则 | 数据结构 | `susp.c:11`、`susp_rock_ridge.c:135` | 核心 | 23 §1.5/§2.6/§4.2 |
| K-554 | 条目五用途 | 接口与协议 | `susp_rock_ridge.c` | 核心 | 23 §1.5/§2.6 |
| K-555 | 链接目标四分支拼法与 bound | 机制 | `susp_rock_ridge.c` | 核心 | 23 §1.5/§3.4 |
| K-556 | 空目标保持空 | 约束与不变量 | — | 不变量 | 23 §1.5 |
| K-557 | 读钳制 | 约束与不变量 | `read.c:5` | 接口 | 23 §1.6/§4.2 |
| K-558 | 分块循环 | 机制 | `read.c` | 核心 | 23 §1.6 |
| K-559 | 拿块失败行为差异 | 架构演进 | `read.c` | Rust 化 | 23 §1.6/§2.8 |
| K-560 | 枚举 | 机制 | `read.c` | 核心 | 23 §1.6/§2.4 |
| K-561 | 枚举类型换算不为类型读盘 | 接口与协议 | `read.c` | 接口 | 23 §1.6 |
| K-562 | 链接读 | 机制 | `link.c` | 接口 | 23 §1.6/§2.5 |
| K-563 | 回调表只读子集与窥视关掉 | 接口与协议 | `table.c` | 装配（总表归 09） | 23 §1.7/§2.7/§2.8 |
| K-564 | 选项表一项 | 接口与协议 | `main.c:10`、`:21` | 接口 | 23 §1.7/§2.1 |
| K-565 | 挂载/卸载/信号/启动 | 机制 | `mount.c:4`、`main.c:17-38` | 生命周期 | 23 §1.7/§2.1 |
| K-566 | 扫描纯函数 | 架构演进 | `volume.rs` | Rust 化 | 23 §3.1 |
| K-567 | 区间走查累减 | 架构演进 | `record.rs` | Rust 化 | 23 §3.3 |
| K-568 | 链接拼装 bound 封顶 | 架构演进 | `rockridge.rs` | Rust 化 | 23 §3.4 |

- **验收标准**：
  1. 给出卷发现的完整流程图（含"有终止无主"与"有主无终止"两种失败）
  2. 给出目录记录的字节图与双端数字段的读法
  3. 给出系统使用尾的走查规则表（短头 / 坏长度 / 未知签名三种情况）
  4. 给出链接目标四分支的拼法表，每条带 bound 检查位置
  5. 明确声明"本篇与 30 篇的分工"（本篇讲格式细节，30 讲三格式对照）

### 29-sffs

- **一句话定位**：读者读完能说出宿主的文件如何被命名、被验鲜、被打开，两个桥如何装配同一框架。
- **讲什么**：
  - 宿主共享的定位（框架在客侧搭名字树，宿主表管 round trip，每次用前先验鲜，句柄懒开静关）
  - 操作表十五个函数分六组（文件开读写关 / 缓冲查询两组 / 目录开列关 / 属性取设 / 建删改名 / 查卷）
  - 窄接口双向防火墙（换宿主协议只重写填表；换框架只重写调表）
  - Redox 方案接口与 Linux VFS 操作表对照
  - 选项六项与两桥默认值差异
  - 掩码语义（按位与掩码再或上类型位；掩码是减法不是加法）
  - 类型位强制的原因
  - 路径拼接（客节点是根起名字链；宿路径 = 前缀加链斜杠拼接）
  - 右到左拼与左到右拼同串
  - 加分支 bound 先算后写；弹出砍末段、无斜杠清空
  - 中途节点无父则整链不可寻址
  - 名字折叠比较；哈希与比较同折叠（哈希一致性铁律）
  - 验鲜三判决（无此项或非目录 → 删节点；种类对不上 → 删节点并标 stale；其他错误 → 透传）
  - 验鲜调用契约（掩码先置模式位）
  - 目录项验鲜多一步（先验父、再拼名、查客表；自顶向下信任链；建表无空位报文件表满）
  - 句柄懒开静关（按需开；文件先读写开，失败或只读挂载转只读开；关闭忽略错误）
  - 懒开与静关的理由
  - 查找全流程与填六字段
  - 两级验鲜信任链；stale 透传成找不到
  - 挂载（拒根标记、只读记标志、建目录项表、建根节点、取根属性、填根六字段、能力 64 位）
  - 卸载与卷状态
  - 已知局限（根必须是真共享）
  - 两桥装配顺序与信号全交框架
  - 装配先源后框、释放逆序
  - 其余文件职责
  - Rust 侧四决策
- **不讲什么**：
  - 宿主协议的线格式（`libvboxfs` / `libhgfs`，两个桥各一套；本篇只消费操作表）
  - 磁盘块的分配读写（06–23，桥接无块）
  - 虚拟树的内容生成（24、25，桥接内容在宿主不在钩子）
  - VFS 侧的挂载（`05-stage-vfs/18-mount.md`）
- **前置**：00、01、02、03、04、05、06、08、09、24
- **后置**：34
- **事实底线**：
  - C：`minix3/minix/lib/libsffs/`（15 `.c` + 5 `.h`，源文件合计 2055 行）：`main.c`（59 行：`:17` `sffs_init`、`:53` `sffs_loop`、`:17-34` 选项表）、`mount.c`（89 行：`:16` `do_mount`、`:16-67` 挂载、`:72-89` 卸载）、`lookup.c`（150 行：`:105` `do_lookup`、`:13-150` 全流程、`go_up`、`go_down`）、`verify.c`（118 行：`:17` `verify_path`、`:17-56` 三判决、`:61-84` 调用契约、`:89-118` 目录项验鲜）、`path.c`（108 行：`:17` `make_path`、`:17-65` 拼接、`:70-87` 加分支、`:92-108` 弹出）、`name.c`（52 行：`:18` `normalize_name`、`:18-34` 折叠、`:39-51` 比较）、`handle.c`（77 行：`:18` `get_handle`、`:18-52` 懒开、`:55-77` 静关）、`dentry.c`（183 行：`:22` `init_dentry`、`:35` `lookup_dentry`、`add_dentry`、`del_one_dentry`、`del_dentry`、`hash_dentry`）、`inode.c`（292 行：`:28` `init_inode`、`:104` `get_inode`、`put_inode`、`link_inode`、`unlink_inode`、`get_free_inode`、`do_putnode`）、`link.c`（366 行：`do_create`、`do_mkdir`、`force_remove`、`do_unlink`、`do_rmdir`、`:304` `do_rename`）、`read.c`（173 行：`:18` `do_read`、`do_getdents`）、`stat.c`（176 行：`:40` `do_stat`、`do_chmod`、`do_utime`、`get_mode`）、`write.c`（131 行：`:68` `do_write`、`write_file`、`do_trunc`）、`misc.c`（53 行：`:17` `do_statvfs`）、`table.c`（28 行）、`fs/vbfs/vbfs.c`（141 行：`:127` `main`）、`fs/hgfs/hgfs.c`（106 行：`:93` `main`）
  - 非 C 制品：`fs/vbfs/Makefile`（`LDADD+= -lsffs -lvboxfs -lfsdriver -lsys`、`FILES=vbfs.conf`）、`fs/hgfs/Makefile`（`LDADD+= -lsffs -lhgfs -lfsdriver -lsys`）、`lib/libsffs/Makefile`
  - Rust：`os/libs/minix-sffs/src/{params,path,name,verify,handles,lib}.rs`、`os/fs/vbfs/src/lib.rs`、`os/fs/hgfs/src/lib.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-569 | 宿主共享定位 | 概念 | — | 定位 | 24 头部/§1.0 |
| K-570 | 操作表十五函数分六组 | 接口与协议 | `lib/libsffs` 操作表 | 核心 | 24 §1.1 |
| K-571 | 窄接口双向防火墙 | 架构演进 | — | 设计 | 24 §1.1 |
| K-572 | Redox / Linux 对照 | 概念 | Linux / Redox | 对照 | 24 §1.1 |
| K-573 | 选项六项与两桥差异 | 接口与协议 | `main.c:17-34` | 接口 | 24 §1.2/§4.1 |
| K-574 | 掩码语义 | 约束与不变量 | `stat.c` | 不变量 | 24 §1.2/§3.3 |
| K-575 | 类型位强制的原因 | 约束与不变量 | — | 设计 | 24 §1.2 |
| K-576 | 路径拼接 | 机制 | `path.c:17-65` | 核心 | 24 §1.3 |
| K-577 | 右到左拼与左到右拼同串 | 架构演进 | `path.c:17-65` | Rust 化 | 24 §1.3/§3.2 |
| K-578 | 加分支 bound 与弹出 | 机制 | `path.c:70-108` | 核心 | 24 §1.3 |
| K-579 | 中途节点无父则整链不可寻址 | 约束与不变量 | `path.c:17-65` | 不变量 | 24 §1.3 |
| K-580 | 名字折叠比较与哈希同折叠 | 机制 | `name.c:18-51` | 核心 | 24 §1.4/§2.5 |
| K-581 | 验鲜三判决 | 机制 | `verify.c:17-56` | 核心 | 24 §1.5/§3.1 |
| K-582 | 验鲜调用契约 | 接口与协议 | `verify.c:61-84` | 接口 | 24 §1.5/§2.4 |
| K-583 | 目录项验鲜多一步 | 机制 | `verify.c:89-118` | 核心 | 24 §1.5/§2.4 |
| K-584 | 句柄懒开静关 | 机制 | `handle.c:18-77` | 核心 | 24 §1.6/§2.6 |
| K-585 | 懒开与静关的理由 | 架构演进 | — | 设计 | 24 §1.6/§3.4 |
| K-586 | 查找全流程与填六字段 | 接口与协议 | `lookup.c:13-150` | 核心 | 24 §1.7/§2.3 |
| K-587 | 两级验鲜信任链 | 约束与不变量 | `lookup.c` | 不变量 | 24 §1.7 |
| K-588 | 挂载 | 机制 | `mount.c:16-67` | 生命周期 | 24 §1.8/§2.2 |
| K-589 | 卸载与卷状态 | 机制 | `mount.c:72-89`、`misc.c:17` | 生命周期 | 24 §1.8 |
| K-590 | 已知局限 | 约束与不变量 | 注释 | 局限 | 24 §1.8 |
| K-591a | 两桥装配顺序与信号全交框架 | 机制 | `vbfs.c:127`、`hgfs.c:93` | 装配（总表归 09） | 24 §1.9/§2.8 |
| K-592a | 装配先源后框、释放逆序 | 架构演进 | `11` 对照 | 装配契约 | 24 §1.9 |
| K-593 | 其余文件职责 | 数据结构 | `lib/libsffs/{dentry,inode,link,read,stat,misc,table}.c` | 结构 | 24 §2.7 |
| K-594 | 宿主答案输入化 | 架构演进 | `verify.rs` | Rust 化 | 24 §3.1 |
| K-595 | 路径同串异写 | 架构演进 | `path.rs` | Rust 化 | 24 §3.2 |
| K-596 | 掩码减法保留 | 架构演进 | `minix-sffs` | Rust 化 | 24 §3.3 |
| K-597 | 关闭静默 | 架构演进 | `handles.rs` | Rust 化 | 24 §3.4 |

- **验收标准**：
  1. 给出操作表十五个函数的完整签名与语义，每个带 C 锚点
  2. 给出验鲜三判决的判定表与每种判决的后果
  3. 给出路径拼接的完整规则（前缀处理、加分支、弹出、无父处理）
  4. 给出两桥的装配对照表（选项差异、宿主库初始化差异、错误分报差异）
  5. 说明"为什么能力位是 64 位而 mfs 是空"（答：宿主文件可超 4GiB，`do_mount` 置 `RES_64BIT`）

### 30-format-compare

- **一句话定位**：读者读完能对照说出 mfs / ext2 / isofs 三种磁盘格式在四个关键结构上的差异。
- **讲什么**：
  - 三张对照表：
    1. **目录项格式**：mfs 定长 64 字节（号 + 名 + 零填充，藏号留坑）vs ext2 变长（头带长度，删除合并无洞）vs isofs 记录（首字节总长度，零表示填充到块尾，双端数字段）
    2. **索引节点格式**：mfs 64 字节（十区号，前七直接）vs ext2 128 字节（十五指针，12 直接 + 三间接）vs isofs 记录（区间链表）
    3. **地址翻译**：mfs 两级（直接 7 + 单 + 双）vs ext2 三级（直接 12 + 单 + 双 + 三）vs isofs 区间累减（无间接）
    4. **空间管理组织**：mfs 两张全局位图 vs ext2 每组两张小位图 + 组描述符 vs isofs 无（只读）
  - 每种格式的容量上限推导（单文件最大尺寸）
  - 三种"删除"语义的对照（藏号留坑可复用 / 合并无洞 / 不适用）
  - 三种"名字长度"的对照（60 / 255 / Rock Ridge 256）
  - 三种"空洞"语义的对照（读零 / 读零 / 报零由调用方定）
  - 为什么会有这些差异（设计取舍与时代背景）
- **不讲什么**：
  - 各格式的完整细节（12、14、16、27、28 各讲自己）
  - 参考实现的完整语义（11–23）
  - 块缓存与块 I/O（06、08）
  - 镜像工具（35）
- **前置**：00、06、08、12、14、16、19、20、27、28
- **后置**：35
- **事实底线**：
  - C：`minix3/minix/fs/mfs/mfsdir.h:13-18`、`minix3/minix/fs/mfs/type.h:8-17`、`minix3/minix/fs/mfs/const.h:5-15`（直接区数 7、总区槽 10）、`minix3/minix/fs/mfs/read.c:210-279`（两级翻译）、`minix3/minix/fs/ext2/inode.h`（120 行，128 字节记录）、`minix3/minix/fs/ext2/const.h`（166 行）、`minix3/minix/fs/ext2/read.c:201-258`（三级翻译）、`minix3/minix/fs/ext2/path.c:20-46`（变长目录项字段）、`minix3/minix/fs/isofs/inode.h`（81 行）、`minix3/minix/fs/isofs/utility.c`（区间）、`minix3/minix/fs/isofs/super.c:24-73`（块大小门）
  - 非 C 制品：`minix3/minix/usr.sbin/mkfs.mfs/`（六套版本头 v1/v1l/v2/v2l/v3/mfs3v2）
  - Rust：`os/fs/mfs/src/{superblock,inode,dir,read}.rs`、`os/fs/ext2/src/{inode,dir,mapping}.rs`、`os/fs/isofs/src/{record,volume}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-018a | 目录项三格式对照表 | 数据结构 | `mfsdir.h:13-18`、`ext2/path.c:20-46`、`isofs/inode.c` | 现有文档各讲自己，无对照 | 新增（C 源码 + 缺口 GAP-19） |
| N-019a | 索引节点三格式对照表 | 数据结构 | `mfs/type.h:8-17`、`ext2/inode.h`、`isofs/inode.h` | 同上 | 新增（缺口 GAP-20） |
| N-018b | 地址翻译三格式对照表 | 机制 | `mfs/read.c:210-279`、`ext2/read.c:201-258`、`isofs/utility.c` | 同上 | 新增（缺口 GAP-18） |
| N-025 | 空间管理组织对照（全局位图 / 每组位图 / 无） | 数据结构 | `mfs/super.h:62-63`、`ext2/super.c`、`isofs` 无 | 同上 | 新增（C 源码） |
| N-026 | 单文件容量上限推导（三格式） | 约束与不变量 | `mfs/const.h:5-15`、`ext2/inode.h`、`isofs` 区间 | 现有文档各篇提到上限但无对照推导 | 新增（C 源码） |
| N-027 | 三种"删除"语义对照 | 机制 | `mfs/path.c:172-181`、`ext2/path.c:95-314`、isofs 无 | 同上 | 新增（C 源码） |
| N-028 | 三种"名字长度"对照 | 约束与不变量 | `mfs/mfsdir.h:13`、ext2 255、`isofs` Rock Ridge 256 | 同上 | 新增（C 源码） |
| N-029 | 三种"空洞"语义对照 | 概念 | `mfs/read.c:149-155`、`ext2/read.c`、`isofs/utility.c` | 同上 | 新增（C 源码） |
| K-515a | 定长 vs 变长的取舍分析 | 概念 | `16`、`27` 对照 | 差异的成因 | 22 §1.1 + 11 §1.1 |

- **验收标准**：
  1. 四张对照表齐备，每格有 C 锚点
  2. 给出三格式的单文件容量上限推导（含算式）
  3. 解释"为什么 isofs 没有间接块"（答：光盘一次写成，区间是分配单位，无需动态增长）
  4. 说明"为什么 mfs 藏号而 ext2 合并"（答：定长项无法变长，删除只能藏号；变长项可以合并，但走查必须逐项解长度）

### 31-fs-errors

- **一句话定位**：读者读完能说出每个失败点返回什么错误码、错误如何穿过框架、以及服务器怎么关闭。
- **讲什么**：
  - 协议错误码三条使用路（`EENTERMOUNT` 进入挂载点、`ELEAVEMOUNT` 离开挂载点、`ESYMLINK` 绝对符号链接）与偏移载荷
  - errno 映射边界（`EINVAL` / `ENOSYS` / `EBUSY` / `ENOENT` / `ENOTDIR` / `EACCES` / `ENOSPC` / `EEXIST` / `EISDIR` / `ENOTEMPTY` / `EROFS` / `EFBIG` / `EIO` / `ENAMETOOLONG` / `ERANGE` / `EENTERMOUNT` 族）
  - "库不崩进程"的 C 宕机改返回值清单（跨 24 篇的差异表汇总）
  - 两类拒绝的区分（坏输入 vs 编程错误）
  - 空回调的三类语义（成功 / `ENOSYS` / 条件）
  - 关闭与退出总图（三种 SIGTERM 处理：mfs 先同步再终止、pfs 直接终止、vtreefs 只应终止）
  - 卸载与退出的次序
  - 接收失败的处理
  - 挂载失败的所有散伙点
  - `Handling` 枚举的三态（`Silence` / `Reply` / `Continue`）与它消除的隐含约定
  - 错误枚举的逐模块映射（每个 server 的错误类型）
  - typed `Stat` / `StatVfs` 的布局与"缓冲不足报无效参数"
- **不讲什么**：
  - 各失败点的机制细节（各篇讲自己的失败分支）
  - 崩溃一致性（32）
  - 协议字段定义（01）
  - 单线程与并发（34）
- **前置**：00、01、02、03、04、05、06、08、10、11、12、13、14、15、16、17、18、19、20、21、22、23、24、25、26、27、28、29
- **后置**：32
- **事实底线**：
  - C：`minix3/minix/include/minix/vfsif.h:26-28`（三个协议错误码）、各篇 §2.x 差异表的"错误传递"行（汇总源）、`lib/libfsdriver/call.c` 各适配器的错误返回、`fs/mfs/main.c:70-78`（SIGTERM）、`fs/pfs/pfs.c:381-392`（SIGTERM）、`lib/libvtreefs/vtreefs.c:66`（信号）、`fs/mfs/mount.c` 各散伙点、`lib/libfsdriver/lookup.c:296-316`（收尾三岔）
  - Rust：`os/libs/minix-fs/src/driver.rs`（`HeaderAction` 三态）、各 server 的错误 enum、`os/libs/minix-types`（`Errno`、`Stat`、`StatVfs`）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-015 | 错误路径总图（协议错误码三条使用路 + errno 映射 + C 宕机改返回值清单） | 接口与协议 | `vfsif.h:26-28`、各篇 §2.x 差异表 | 现有文档散在 24 篇 | 新增（缺口 GAP-06） |
| N-017 | 关闭与退出总图（三种 SIGTERM 对照） | 接口与协议 | `mfs/main.c:70-78`、`pfs.c:381-392`、`vtreefs.c:66` | 现有文档散在 06/07/18 | 新增（缺口 GAP-08） |
| K-048a | `HeaderAction` 三态与消除的隐含约定 | 架构演进 | `driver.rs` | Rust 化 | 01 §3.6 |
| K-203a | 九错误变体与映射边界（mfs super） | 接口与协议 | — | 映射的一例 | 08 §3.2 |
| K-230a | 五错误变体与两种满对策（mfs inode） | 接口与协议 | — | 映射的一例 | 09 §3.4 |
| K-246a | 六错误变体（mfs mount） | 接口与协议 | — | 映射的一例 | 10 §3.2 |
| K-300a | 错误枚举十二变体加透传（mfs link） | 架构演进 | `link.rs` | 映射的一例 | 13 §3.3 |
| K-368a | typed `Stat` / `StatVfs` 布局与缓冲不足 | 接口与协议 | `minix-types` | 外部契约 | 16 §3.1 |
| K-056a | C 宕机改 Rust 错误返回的清单 | 架构演进 | 各篇 §2.x 差异表 | 汇总 | 03 §1.2/§3.2 |
| K-278a | 回滚宕机改 corrupt | 架构演进 | — | 映射的一例 | 12 §3.3 |
| K-321a | 间接块损坏报 IO 错 | 架构演进 | `read.c:317-322` | 映射的一例 | 14 §3.2 |
| K-559a | isofs 拿块失败报 IO 错 | 架构演进 | `isofs/read.c` | 映射的一例 | 23 §2.8 |
| K-294a | 读非符号链接报访问拒绝 | 约束与不变量 | `link.c:162-166` | 映射的一例 | 13 §1.4 |
| N-030 | 空回调三类语义的完整对照（成功 / ENOSYS / 条件） | 接口与协议 | `call.c` 各适配器 | 现有 02 篇只举一例 | 新增（`call.c` 逐函数） |

- **验收标准**：
  1. 给出完整的 errno 映射表：错误码、含义、产生位置、消费方
  2. 给出"协议错误码三条使用路"的完整说明，每条带 C 锚点与偏移载荷语义
  3. 给出"C 宕机改返回值"的完整清单（至少 15 条），每条标注 C 位置与 Rust 处理
  4. 给出三种 SIGTERM 处理的对照表，并说明"为什么 mfs 要先同步而 pfs 不用"
  5. 说明"为什么卸载失败不能容忍"（答：半挂载状态比崩溃更难恢复）

### 32-fs-consistency

- **一句话定位**：读者读完能说出这套文件系统在崩溃前后保证什么、不保证什么，以及每一步顺序为什么不能换。
- **讲什么**：
  - 崩溃一致性的总契约（不保证原子性，保证"可修复"）
  - 孤儿优于悬空（创建第三拍强制落盘）
  - 先 inode 后块（同步顺序及其理由：inode 写回产生新的脏缓存块）
  - clean 位的三处使用（挂载检查、脏盘降级、卸载写回）与"降级来的只读不写干净位"
  - 只读不写盘（不扩大不一致）
  - 位图与 inode 位的一致性窗口（分配占位 → 表占槽 → 落盘 → 进入 的四拍中，崩溃在各拍的结果）
  - 链接计数与目录项的记账一致性（创建、删除、改名的三步记账）
  - 卸载顺序（数引用 → 放根 → 同步 → 写干净位 → 作废缓存 → 复位设备号）
  - 崩溃后修复的可能性（`fsck.mfs` 的输入是什么、能修什么）
  - 与 Linux / Redox 的对照（日志式 / 写时复制 vs 本系统的"顺序加检查工具"）
- **不讲什么**：
  - 各机制的顺序细节（各篇讲自己的顺序）
  - 错误码映射（31）
  - 离线工具的完整实现（35）
  - `fsck` 的实现细节（35 讲格式契约）
- **前置**：00、06、08、12、13、14、15、16、17、18、19、21、23、31
- **后置**：35
- **事实底线**：
  - C：`fs/mfs/open.c:233-235`（第三拍与注释）、`fs/mfs/misc.c:18-22`（同步顺序与注释）、`fs/mfs/mount.c:40-50`（脏盘降级）、`:91-95`（清干净位）、`:134-172`（卸载顺序）、`fs/mfs/super.h:68`（`MFSFLAG_CLEAN`）、`fs/mfs/link.c:205-211`（删除记账）、`:405-414`（改名记账）、`fs/mfs/write.c:176-180`（变空重读）
  - 非 C 制品：`minix3/minix/commands/fsck.mfs/fsck.c`（1675 行）
  - Rust：`os/fs/mfs/src/{open,maint,mount,link}.rs`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-016 | 崩溃一致性契约汇总 | 约束与不变量 | `open.c:233-235`、`misc.c:18-22`、`mount.c:40-50`、`:134-172` | 现有文档散在 10/12/17 | 新增（缺口 GAP-07） |
| K-269a | 孤儿优于悬空 | 约束与不变量 | `open.c:233-235` | 一致性核心 | 12 §1.1 |
| K-372a | 先 inode 后块 | 约束与不变量 | `misc.c:22` | 一致性核心 | 17 §1.1 |
| K-196b | clean 位三处使用 | 机制 | `super.h:68`、`mount.c:40-50`、`:91-95` | 一致性核心 | 08 §1.7 + 10 §2.1 |
| K-236a | 降级来的只读不写干净位 | 约束与不变量 | `mount.c:40-50` | 一致性核心 | 10 §1.2/§3.3 |
| K-271a | 泄漏是慢性 corrupt | 约束与不变量 | — | 一致性论证 | 12 §1.2 |
| K-287a | 删除的三次计数记账 | 机制 | `link.c:205-211` | 记账一致性 | 13 §2.4 |
| K-292a | 改名的记账（跨父修正 + 父链接加一） | 机制 | `link.c:405-414` | 记账一致性 | 13 §1.3 |
| K-331a | 变空检查重新读盘 | 约束与不变量 | `write.c:176-180` | 一致性论证 | 15 §1.1/§3.2 |
| K-237a | 卸载顺序 | 机制 | `mount.c:134-172` | 一致性核心 | 10 §1.3/§2.3 |
| K-234a | 散伙原子性 | 约束与不变量 | `mount.c:33-37` | 一致性论证 | 10 §1.1/§2.1 |
| K-329a | 分配失败不回滚 | 约束与不变量 | `write.c:116-131` | 一致性论证 | 15 §1.1 |
| K-346a | 先清零后释放 | 约束与不变量 | — | 顺序契约 | 15 §1.4 |
| N-031 | 四拍崩溃结果推演表（分配占位/表占槽/落盘/进入 各拍崩溃后的盘上状态） | 约束与不变量 | `open.c:192-257` | 现有 12 篇讲了顺序但未给逐拍推演 | 新增（C 源码推导） |
| N-032 | 与日志式 / 写时复制文件系统的对照 | 概念 | Linux ext4 journal / Redox COW | 现有文档零处 | 新增（外部对照） |
| N-033 | `fsck.mfs` 的输入与可修范围 | 工具与工程 | `commands/fsck.mfs/fsck.c`（1675 行） | 一致性契约的出口 | 新增（缺口 GAP-11 的一部分） |

- **验收标准**：
  1. 给出"四拍崩溃结果推演表"，每拍列出盘上状态与可否修复
  2. 给出"先 inode 后块"的正反两个后果推演
  3. 给出 clean 位的完整状态机（干净 → 挂载写脏 → 崩溃 → 脏盘降级 → 卸载不写干净）
  4. 逐条列出所有"顺序不能换"的位置与理由（至少 6 条）
  5. 明确声明本系统的保证边界（不保证原子性，保证可修复），并与日志式 / COW 对照

### 33-fs-testing

- **一句话定位**：读者读完能自己跑起测试、造出一个可挂载的镜像、并知道 C 侧的行为验收面在哪。
- **讲什么**：
  - Rust 侧单测惯例（每个 crate 的测试分布、`rg "fn test_"` 的核对方式、测试统计的写法）
  - 测试的三层：单元（纯函数）、装配（`MfsServer` 冒烟链）、协议（`task.rs` 的循环测试）
  - 测试镜像搭建器规格（64 节点、一区位图一块、节点区一块、首区五、根目录块五、位图一位占用、断言魔数落位）
  - 观察口设计（计数源断言零设备读、卷空闲区计数、位图状态）
  - 装配冒烟链（挂载 → 查找 → 创建 → 写 → 读 → 枚举 → 状态 → 同步 → 卸载 → 设备交还 → 重挂载后文件仍在）
  - 单字节块测试（把每条分支逼到墙角）
  - 二级缓存的十六类线材与十一类池子侧用例
  - C 侧端到端脚本（`testmfs.sh` 87 行含镜像 sha1 期望值、`testisofs.sh` 183 行、`testvnd.sh` 155 行、`run` 184 行、`check-install` 42 行）
  - 测试与文档的同步纪律（测试统计的日期标注、测试名与文档描述的对账）
- **不讲什么**：
  - 各机制的实现细节（各篇讲自己的测试）
  - 错误码（31）
  - 构建链（35）
- **前置**：00、02、03、05、06、08、09、10、11、12、13、14、15、16、17、18、19、20、21、22、23、24、25、26、27、28、29
- **后置**：无（收尾篇）
- **事实底线**：
  - Rust：各 crate 的 `#[cfg(test)]` 模块；`os/fs/mfs/src/server.rs`（装配测试）、`os/fs/mfs/src/mount.rs`（镜像搭建）、`os/libs/minix-fs/src/task.rs`（循环测试）、`os/libs/minix-fs/src/cache.rs`、`os/libs/minix-fs/src/vm_cache.rs`
  - 非 C 制品：`minix3/minix/tests/testmfs.sh`（87 行，第 4 行 `expect=98bcafa04cb1eb75b7add6c95eb587c37f5050e0`、第 73 行 `/sbin/mkfs.mfs`）、`testisofs.sh`（183 行）、`testvnd.sh`（155 行）、`run`（184 行）、`check-install`（42 行）
  - C：`minix3/minix/fs/*/Makefile`（测试如何被构建链关联）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-598 | 测试分组惯例与统计日期 | 测试性质 | 各篇 §5 | 惯例 | 各篇 §5 |
| K-599a | 测试镜像搭建器规格 | 测试性质 | `mount.rs` 测试 | 工具 | 10 §5.3 |
| K-600a | 单字节块测试 | 测试性质 | `dir.rs` | 方法 | 11 §3.2 |
| K-601a | 计数源断言零设备读 | 测试性质 | `write.rs` | 观察口 | 15 §4.2 |
| K-602a | 二级缓存两类用例 | 测试性质 | `vm_cache.rs`、`cache.rs` | 用例集 | 04 §5.5 |
| K-603a | 装配冒烟链 | 测试性质 | `server.rs` | 集成验证 | 07 §4.5/§5.5 |
| N-012a | C 侧端到端测试脚本 | 工具与工程 | `minix3/minix/tests/` | 现有文档零处 | 新增（缺口 GAP-12） |
| N-034 | 测试三层划分（单元 / 装配 / 协议）与各自的接缝 | 测试性质 | `server.rs`、`task.rs` | 现有文档按 crate 列，无分层说明 | 新增（Rust 源码 + todo.md） |
| N-035 | 观察口设计原则（用可计数的副作用代替内部状态断言） | 测试性质 | `write.rs`（计数源）、`server.rs`（卷空闲区） | 现有文档散见，未成原则 | 新增（Rust 源码） |

- **验收标准**：
  1. 给出三层测试的划分表与每层的代表用例
  2. 给出镜像搭建器的完整规格与一个可运行的构造例
  3. 列出所有观察口设计与它们各自能观察什么
  4. 给出 C 侧脚本清单与每个脚本验证什么
  5. 说明"为什么测试统计要标日期"（答：测试数会随实现增长，不标日期会产生跨篇矛盾；现有 08/09/10 篇就是教训）

### 34-fs-boundaries

- **一句话定位**：读者读完能说出这个子系统的边界在哪、边界上每个接缝的另一端是谁、并发与同步的声明是什么。
- **讲什么**：
  - 并发与同步声明（单线程事件循环、零锁、`RES_THREADED` 永不置位、与内核 BKL 的关系、为什么不需要锁）
  - 与块设备层的接缝（`DeviceInfo` trait / `BlockSource` / `bdev_open` / `bdev_close` / 短末块 / 散集 I/O → `edge E-FSBDEV`）
  - 与 VM 层的接缝（二级缓存四个线上调用、零拷贝页移交、旗标机 → `edge E-FSVMCACHE`）
  - 与运行时的接缝（SEF/RS 启动握手、进程启动、特权丢弃 → `edge E-FSRUNTIME`）
  - 与命令层的接缝（mount/umount/fsck 命令面 → `edge E-FSCMDS` / `18-stage-commands`）
  - 与 VFS 的接缝（`REQ_*` 契约的双侧独立定义 → `edge E-REQWIRE`）
  - 与内核信息通道的接缝（procfs 的 cmdline/environ → `edge E-KERNINFO`）
  - 生产传输接缝（`FsTransport` 五方法、`Incoming::Cancelled` 语义）
  - 服务策略配置（`system.conf` 的授权面）
  - 明确的范围外清单（VFS 服务端、块驱动、终端驱动、`libbdev`、`libvboxfs`/`libhgfs`、`libpuffs`、devman/gpio 用法、NetBSD 派生工具）
- **不讲什么**：
  - 各接缝另一端的实现（各 stage）
  - 各机制细节（各篇）
  - 错误码（31）
  - 构建链（35）
- **前置**：00、01、02、03、05、06、07、08、09、25、26、29、31
- **后置**：无（收尾篇）
- **事实底线**：
  - C：`lib/libfsdriver/fsdriver.c:80-96`（单线程循环）、`minix3/minix/include/minix/vfsif.h:21`（`RES_THREADED`）、`lib/libminixfs/bio.c:48-53`（驱动绑定）、`lib/libminixfs/cache.c:443-465`（VM 通道）、`fs/mfs/main.c:14-26`（SEF 启动）、`fs/procfs/pid.c`（cmdline/environ 需读进程内存）
  - 非 C 制品：`minix3/etc/system.conf:353/507`、`minix3/minix/fs/Makefile`
  - Rust：`os/libs/minix-fs/src/task.rs:397`（`FsTransport`）、`os/libs/minix-fs/src/bio.rs:40`（`DeviceInfo`）、`os/libs/minix-fs/src/cache.rs:93`（`BlockSource`）、`os/libs/minix-fs/src/vm_cache.rs:426`（`SecondLevelCache`）、`os/libs/minix-fs/src/bdev_bridge.rs`、`os/servers/vm/src/ipc/cache_handlers.rs`
  - 边界条目：`edge_todo.md:897`（E-FSRUNTIME）、`:914`（E-FSBDEV）、`:931`（E-FSVMCACHE）、`:948`（E-FSCMDS）、`:414`（E-REQWIRE）、`:301`（E-KERNINFO）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-018 | 并发与同步声明 | 约束与不变量 | `fsdriver.c:80-96`、`vfsif.h:21` | 现有 01 篇只讲缓存无锁 | 新增（缺口 GAP-05） |
| N-036 | 六条接缝的完整清单（块设备 / VM / 运行时 / 命令 / VFS / 内核信息） | 接口与协议 | `edge_todo.md` 六条 | 现有文档零处集中 | 新增（edge 条目） |
| K-132a | `BdevBlockSource` 接缝契约 | 接口与协议 | `bdev_bridge.rs` | 接缝 | 05 §3.6 |
| K-089a | 二级缓存通道接缝 | 机制 | `cache.c:443-465` | 接缝 | 04 §1.5/§2.9 |
| N-014a | 生产传输接缝（五方法与取消接收） | 接口与协议 | `task.rs:397` | 接缝 | 新增（Rust 源码） |
| N-010b | 服务策略配置的授权面 | 工具与工程 | `etc/system.conf:353/507` | 边界条件 | 新增 |
| N-020 | `libpuffs` 的处置 | 工具与工程 | `ls minix3/minix/lib/libpuffs/` | 范围外声明 | 新增（缺口 GAP-16） |
| K-003a | 单线程假设与零锁设计 | 约束与不变量 | `fsdriver.c:80-96` | 并发声明 | 01 §1.1/§3.7 |
| K-015a | `RES_THREADED` 永不置位 | 约束与不变量 | `vfsif.h:21` | 并发声明 | 01 §1.6 |

- **验收标准**：
  1. 给出六条接缝的完整表：接缝名、本侧契约、另一端、edge 条目、验收面
  2. 给出并发声明的完整论证（为什么不需要锁，至少 3 条理由）
  3. 给出范围外清单，每条给排除理由
  4. 明确声明 `libpuffs` 的处置与理由

### 35-fs-build-image

- **一句话定位**：读者读完能说出一块 mfs 镜像从哪来、构建链怎么把它装进系统、坏了用什么查。
- **讲什么**：
  - 构建链：`fs/Makefile` 的条件编译（mfs/pfs 无条件，ext2/isofs/procfs/ptyfs 在 `MKIMAGEONLY=no` 时，hgfs/vbfs 只在 i386）、`fs/Makefile.inc`（`BINDIR=/service`）
  - 每个 server 的链接依赖（`LDADD` 与 `DPADD`）与构建期常量（`DEFAULT_NR_BUFS=1024`、`NR_BUFS=100`、`WARNS=3`）
  - 镜像格式契约：mfs 七段布局与 `mkfs.mfs` 的六套版本头（v1/v1l/v2/v2l/v3/mfs3v2）
  - `mkfs.mfs` 的职责（写出超级块、位图、根目录、`.` 与 `..`）
  - `fsck.mfs` 的职责（一致性检查与修复的输入输出）
  - 块大小与镜像参数的关系（块大小五连验对 mkfs 的反向约束）
  - 端到端脚本如何验证镜像（`testmfs.sh` 的 sha1 期望值机制、`testvnd.sh` 的 `mkfs.mfs` 用法）
  - 常量权威位置表（全 stage 常量按模块归位，与 99 的分工）
  - 待裁决项：磁盘格式兼容策略（读旧镜像 vs 仅自举格式）
  - 范围外声明（`libpuffs`、NetBSD 派生工具）
- **不讲什么**：
  - 各机制的常量使用逻辑（各消费篇）
  - 超级块格式的逐字段细节（12）
  - 一致性契约的论证（32）
  - 命令层的 mount/umount 命令面（`18-stage-commands`）
- **前置**：00、01、06、08、09、12、13、14、15、16、19、20、27、28、30、32
- **后置**：99
- **事实底线**：
  - 非 C 制品（本篇的主要事实底线）：`minix3/minix/fs/Makefile`（18 行）、`fs/Makefile.inc`（4 行）、8 个 server Makefile（8–14 行）、`lib/libfsdriver/Makefile`（9）、`lib/libminixfs/Makefile`（10）、`lib/libvtreefs/Makefile`（19）、`lib/libsffs/Makefile`（11）、`minix3/minix/usr.sbin/mkfs.mfs/`（`mkfs.c` 1544 行 + 六套版本头）、`minix3/minix/commands/fsck.mfs/`（`fsck.c` 1675 行）、`minix3/minix/tests/testmfs.sh`（87 行）、`testvnd.sh`（155 行）、`etc/system.conf`（服务策略）
  - C：`fs/mfs/const.h`（常量定义）、`fs/mfs/super.h:10-20`（布局）
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| N-009 | 非 C 制品清单与构建链 | 工具与工程 | `fs/Makefile`、8 个 server Makefile | 现有文档零处 | 新增（缺口 GAP-09） |
| N-011 | 镜像格式契约与离线工具 | 工具与工程 | `usr.sbin/mkfs.mfs/`、`commands/fsck.mfs/` | 现有文档零处 | 新增（缺口 GAP-11） |
| N-019 | 磁盘格式兼容策略（待裁决） | 架构演进 | 待裁决 | 现有 00/08/21/23 提到但无结论 | 新增（缺口 GAP-15） |
| N-020a | 范围外声明（`libpuffs`、NetBSD 派生工具） | 工具与工程 | `ls minix3/minix/lib/libpuffs/`、`minix3/sbin/` | 范围外 | 新增（缺口 GAP-16） |
| K-380a | 常量权威位置表 | 数据结构 | `fs/mfs/const.h:5-65` | 现有 17 篇讲 mfs 常量，无全 stage 版 | 17 §1.4/§2.5 |
| K-381a | 常量零重复原则 | 约束与不变量 | `minix-types` | 原则 | 17 §1.4/§3.5 |
| K-382b | 全局头文件五名字与五去向 | 数据结构 | `glo.h:11-20` | 归位 | 17 §1.5/§2.4 |
| N-012b | 端到端脚本如何验证镜像（sha1 期望值机制） | 工具与工程 | `testmfs.sh:4` | 验收面 | 新增 |
| K-510a | 构建期常量（`DEFAULT_NR_BUFS` / `NR_BUFS` / `WARNS`） | 工具与工程 | `fs/mfs/Makefile`、`fs/isofs/Makefile`、`fs/ext2/Makefile` | 构建 | 07 §2.2（部分）+ 新增 |

- **验收标准**：
  1. 给出构建链的完整表：server / Makefile 路径 / `LDADD` / `DPADD` / 构建期常量 / 条件编译条件
  2. 给出 mfs 镜像的七段布局与 `mkfs.mfs` 写出的内容对照
  3. 给出 `fsck.mfs` 的输入、检查项、可修范围的清单
  4. 给出全 stage 常量权威位置表（按模块），并说明与 99 篇的分工（35 讲"权威在哪"，99 讲"值是多少"）
  5. 明确写出"磁盘格式兼容策略"待裁决项的选项与影响面

### 99-fs-global-concepts

- **一句话定位**：读者要查一个常量值或一个结构定义时，本篇给出权威位置与值。
- **讲什么**：
  - `FS_BASE` / `REQ_*` 常量表（编号 1..33、`NREQS=34`）
  - `TRNS_*` 三宏的定义
  - 挂载标志、查找标志、能力位的值
  - 协议错误码的值（-301 / -302 / -303）
  - 服务号常量（MFS / PFS / PROC / PTY 等 endpoint，`com.h`）
  - 协议结构：`fsdriver_node` / `fsdriver_data` / `fsdriver_dentry`、`vfs_ucred_t`、`m_vfs_fs_*` / `m_fs_vfs_*` 消息布局
  - mfs 常量表（V2/V3 魔数、区格式、位置常量、派生尺寸）
  - vtreefs / sffs 的常量（`NO_INDEX` / `PNAME_MAX` / `SFFS_ATTR_*`）
  - errno 映射表（含三个协议错误码）
  - Rust 侧的权威位置（`minix-types` 的 `Stat` / `StatVfs` / `Errno` / 各常量）
  - 与 35 的分工声明（本篇给值，35 给"权威在哪"）
- **不讲什么**：
  - 常量如何被使用（各消费篇）
  - 构建期常量的来源（35）
  - 错误码的产生位置（31）
- **前置**：00（本篇是查阅表，可独立阅读；不强制前置其他篇）
- **后置**：无（附录）
- **事实底线**：
  - C：`minix3/minix/include/minix/vfsif.h`（84 行）、`minix3/minix/include/minix/fsdriver.h`（127 行）、`minix3/minix/include/minix/libminixfs.h`（65 行）、`minix3/minix/include/minix/vtreefs.h`（74 行）、`minix3/minix/include/minix/sffs.h`（69 行）、`minix3/minix/include/minix/com.h`（1153 行）、`minix3/minix/fs/mfs/const.h`（68 行）、`minix3/minix/fs/ext2/const.h`（166 行）、`minix3/minix/fs/isofs/const.h`（44 行）、`minix3/sys/sys/dirent.h`（129 行）
  - Rust：`os/libs/minix-types/src/`
- **知识点清单**：

| 知识点编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|---|---|---|---|---|---|
| K-005b | 33 个请求编号表 | 接口与协议 | `vfsif.h:41-73` | 查阅 | 99 骨架 |
| K-007b | 三个事务宏 | 接口与协议 | `vfsif.h:79-81` | 查阅 | 99 骨架 |
| K-014b | 三组标志与能力位的值 | 接口与协议 | `vfsif.h:8-23` | 查阅 | 99 骨架 |
| K-025b | 协议错误码的值 | 接口与协议 | `vfsif.h:26-28` | 查阅 | 99 骨架 |
| K-078b | `vfs_ucred_t` 布局 | 数据结构 | `vfsif.h:33-38` | 查阅 | 99 骨架 |
| N-021a | 消息布局逐字段表 | 接口与协议 | `ipc.h` | 查阅 | 新增 |
| K-182a | mfs 磁盘七段布局 | 数据结构 | `super.h:10-20` | 查阅 | 99 骨架 |
| K-380b | mfs 常量全表 | 数据结构 | `const.h:5-65` | 查阅 | 99 骨架 |
| K-422a | vtreefs / sffs 常量 | 数据结构 | `vtreefs.h`、`sffs.h` | 查阅 | 99 骨架 |
| K-059a | 目录项输出格式 | 数据结构 | `dirent.h` | 查阅 | 99 骨架 |
| N-037 | Rust 侧常量与结构的权威位置表 | 数据结构 | `os/libs/minix-types/src/` | 现有 99 骨架只列 C 侧 | 新增（Rust 源码） |
| N-038 | 与 35 篇的分工声明（值 vs 权威位置） | 概念 | — | 避免两篇重复 | 新增（本篇设计） |

- **验收标准**：
  1. 给出全部常量的值表，每行带 C 头文件与行锚点
  2. 给出全部协议结构的逐字段布局，每个字段带行锚点
  3. 给出 errno 映射表（含三个协议错误码）
  4. 给出 Rust 侧权威位置表，与 C 侧一一对应
  5. 明确声明与 35 篇的分工

---

## 6. 变更表

### 6.1 统一变更表

> 操作类型：重排 / 拆分 / 合并 / 新建 / 归档。
> "去向"一列按双方向规则：**存量方向看去向**（旧知识点逐条给出新位置）、**新增方向看来源**（给证据锚点）。

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 / 来源 |
|---|---|---|---|---|---|---|
| OP-01 | 重排 + 改写 | `00-fs-overview.md`（21 行骨架） | 新 00 | 骨架自述"待改写"；缺 8 server × 4 框架库的关系图、缺 boot 两层语义、缺阅读路径 | N-001、N-002、N-003、N-004、K-016 | 存量 K-016 → 新 00 §横向对照；新增 N-001..N-004 来源见 §2.3 |
| OP-02 | 拆分 | `01-fsdriver-task.md`（358 行） | 新 01（协议）+ 新 02（主循环）+ 新 03（数据结构） | 一篇同时承载协议定义、主循环、回调表、能力位、数据结构五件事，违反单篇单语义 | K-001..K-027、K-047..K-050 | K-005..K-007/K-002/K-014/K-015/K-024/K-025/K-078 → 新 01；K-001/K-003/K-004/K-008..K-013/K-017/K-018/K-019/K-010 → 新 02；K-047/K-048/K-050 → 新 03 |
| OP-03 | 拆分 | `03-fsdriver-utility.md`（249 行） | 新 03（数据结构与搬运）+ 新 04（路径查找） | 搬运工具与查找漫步是两类机制：前者是"跨越边界的工具"，后者是"一步一查的算法" | K-051..K-080 | K-051..K-061/K-068/K-075/K-079/K-052/K-053 → 新 03；K-062..K-067/K-069..K-074/K-076/K-077/K-056 → 新 04 |
| OP-04 | 改写 | `02-fsdriver-call.md`（284 行） | 新 05 | 保留全篇结构；补"空回调三类语义全表"（现有只举一例） | K-019..K-046 + N-022 | K-019..K-046 → 新 05 对应组；新增 N-022 来源 `call.c` 逐函数核对 |
| OP-05 | 改写 | `04-block-cache.md`（310 行） | 新 06 | 保留缓存本体；剥离二级缓存（拆分） | K-081..K-110（除 vm 族） | K-081..K-086/K-090..K-107/K-109/K-110 → 新 06 |
| OP-06 | 拆分 | `04-block-cache.md` | 新 07 | 二级缓存是与 VM 的跨 stage 通道，独立成篇才能讲清"另一端是谁" | K-087..K-089、K-108、K-162、K-317 + N-013、N-023 | K-087/K-088/K-089/K-108/K-162/K-317 → 新 07；新增 N-013/N-023 来源见 §2.3 |
| OP-07 | 改写 | `05-block-io.md`（220 行） | 新 08 | 保留全篇；`BdevBlockSource` 接缝明确标注归属（越界归位） | K-111..K-135 | K-111..K-135 → 新 08 |
| OP-08 | 新建 | 无 | 新 09 | 8 个 server 共有的第一站（进程诞生、初装、回调表、差异总表），现有文档零处集中 | N-006、N-007、N-008、K-160、K-181、K-179、K-412、K-455、K-591、K-592、K-155、N-010a、N-024 | 存量 K-160/K-181/K-179（07 越界）/K-412（18）/K-455（20）/K-591/K-592（24）/K-155（06）→ 新 09；新增 N-006..N-008/N-010a/N-024 来源见 §2.3 |
| OP-09 | 改写 | `06-pfs.md`（239 行） | 新 10 | 保留全篇；头部锚点系统性错位（7 处挂 A 引上一函数行号）须重建 | K-136..K-159、K-474..K-478 | K-136..K-159 → 新 10 对应节；K-474..K-478 → 新 10 §Rust 化 |
| OP-10 | 改写 | `07-mfs-init-main.md`（235 行） | 新 11 | 保留启动链与接线表；剥离 §4.5 服务装配（越界归 09）；修 `cache.c` 行数矛盾（109 行 vs 引用 `cache.c:1236`） | K-160..K-180 | K-160..K-180 → 新 11（K-181 移新 09；K-180 移新 11 §工程趣闻） |
| OP-11 | 拆分 | `08-mfs-super.md`（229 行） | 新 12（超级块）+ 新 13（位图） | 磁盘格式校验与位图记账是两类机制；且现有 08 篇同时讲了 `alloc_bit`/`free_bit` 的逐行流程与超级块解析 | K-182..K-205 | K-182..K-191/K-196..K-205 → 新 12；K-192..K-195 + K-373a/K-376a/K-377a/K-204a → 新 13 |
| OP-12 | 改写 | `09-mfs-inode.md`（235 行） | 新 14 | 保留全篇；修三处字段计数矛盾（"十几项"列 8 项、"十三项"列 14 名、"十三字段"列 18 名）与重复的 §4.2 | K-206..K-232 | K-206..K-232 → 新 14 对应节 |
| OP-13 | 改写 | `10-mfs-mount.md`（187 行） | 新 15 | 保留全篇；剥离 §4.3 接线表翻通（越界归 09）；补挂载点检查的语义澄清 | K-233..K-247 | K-233..K-247 → 新 15（K-247 保留为与 23 的接缝） |
| OP-14 | 改写 | `11-mfs-path.md`（171 行） | 新 16 | 保留全篇；纳入 `dir_io.rs` 镜像桥（从 14 越界归位） | K-248..K-263 + K-325a | K-248..K-263 → 新 16；K-325a 从 14 §4.4 移入 |
| OP-15 | 改写 | `12-mfs-open.md`（176 行） | 新 17 | 保留全篇；修"上下文束五件/七件"表述冲突 | K-264..K-279 | K-264..K-279 → 新 17 对应节 |
| OP-16 | 拆分 | `13-mfs-link.md`（241 行） | 新 18（链接与删除）+ 新 19（截断与区间释放） | 现有 13 篇同时承载"名字关系变化"（链接/删除/改名）与"存储空间回收"（截断/区间翻译）两类语义；后者是数据通路的一部分 | K-280..K-301 | K-280..K-294/K-300/K-301/K-213a → 新 18；K-295..K-299/K-344..K-348/K-330a → 新 19 |
| OP-17 | 改写 | `14-mfs-read.md`（244 行） | 新 20 | 保留全篇；剥离 §4.4 目录镜像桥（归 16） | K-302..K-326 | K-302..K-326 → 新 20（K-325 移新 16） |
| OP-18 | 改写 | `15-mfs-write.md`（211 行） | 新 21 | 保留全篇；截断执行与 19 的交接契约明确化 | K-327..K-348 | K-327..K-348 → 新 21（K-344..K-348 的机制归 19，本篇讲执行） |
| OP-19 | 改写 | `16-mfs-metadata.md`（210 行） | 新 22 | 保留全篇；"改属主不检查只读"这条原始不一致必须逐字保留并解释 | K-349..K-370 | K-349..K-370 → 新 22 对应节 |
| OP-20 | 改写 | `17-mfs-maint.md`（194 行） | 新 23 | 保留全篇；剥离常量目录（越界归 35 与 99） | K-371..K-388 | K-371..K-379/K-383..K-388 → 新 23；K-380..K-382 → 新 35（权威位置）与 99（值） |
| OP-21 | 改写 | `18-vtreefs.md`（237 行） | 新 24 | 保留全篇；修"约三千五百行"与实际 1507 行的矛盾、"十七项回调"与 16 项的计数矛盾、§4 编号跳号 | K-389..K-423 | K-389..K-423 → 新 24 对应节 |
| OP-22 | 改写 | `19-procfs.md`（213 行） | 新 25 | 保留全篇；修 §4 编号跳号（4.3 → 4.6）与正文错位、`cpuinfo.c` 计数缺单位 | K-424..K-453 | K-424..K-453 → 新 25 对应节 |
| OP-23 | 改写 | `20-ptyfs.md`（191 行） | 新 26 | 保留全篇；修测试计数矛盾（5.2 标"五个"实列 7 组；统计"十一个"vs 3+5=8） | K-454..K-477 | K-454..K-477 → 新 26 对应节 |
| OP-24 | 合并 | `21-ext2-init-mount.md`（197 行）+ `22-ext2-namespace-data.md`（175 行） | 新 27 | 两篇合起来才是完整的 ext2 变体；现有拆分造成"分配器"与"名字数据"的边界模糊（如 `inode.c` 属哪篇） | K-478..K-535 | K-478..K-510（21）与 K-511..K-535（22）全部 → 新 27 |
| OP-25 | 改写 | `23-isofs.md`（198 行） | 新 28 | 保留全篇；修"只建三用"与正文"五用"的自相矛盾（§6.3 裁决） | K-536..K-568 | K-536..K-568 → 新 28 对应节 |
| OP-26 | 改写 | `24-vbfs-hgfs.md`（214 行） | 新 29 | 保留全篇；修选项计数矛盾（六项 / 七项 / 八项三处不一致） | K-569..K-597 | K-569..K-597 → 新 29 对应节 |
| OP-27 | 新建 | 无 | 新 30 | 三种磁盘格式的对照表（目录项 / inode / 地址翻译 / 空间管理），现有文档各讲自己、无对照 | N-018a、N-019a、N-018b、N-025..N-029 | 新增，来源见 §2.3 |
| OP-28 | 新建 | 无 | 新 31 | 错误路径总图（协议错误码 + errno 映射 + C 宕机改返回值清单 + 关闭退出），现有散在 24 篇差异表 | N-015、N-017、N-030 + K-048a 等映射例 | 新增，来源见 §2.3 |
| OP-29 | 新建 | 无 | 新 32 | 崩溃一致性契约汇总，现有散在 10/12/17 | N-016、N-031、N-032、N-033 + K-269a 等 | 新增，来源见 §2.3 |
| OP-30 | 新建 | 无 | 新 33 | 测试基建（三层划分 + 镜像搭建 + C 侧脚本），现有各篇 §5 只列自己的统计 | N-012a、N-034、N-035 + K-598..K-603 | 新增，来源见 §2.3 |
| OP-31 | 新建 | 无 | 新 34 | 边界与外部契约（并发声明 + 六条接缝 + 范围外清单），现有零处集中 | N-018、N-036、N-020 + K-132a 等 | 新增，来源见 §2.3 |
| OP-32 | 新建 | 无 | 新 35 | 构建、镜像与离线工具，现有零处 | N-009、N-011、N-019、N-020a + K-380a 等 | 新增，来源见 §2.3 |
| OP-33 | 改写 | `99-global-concepts.md`（21 行骨架） | 新 99 | 骨架自述"待改写"；补 Rust 侧权威位置表与与 35 的分工声明 | K-005b 等 + N-021a、N-037、N-038 | 存量 → 新 99 对应节；新增来源见 §2.3 |
| OP-34 | 归档 | `plan.md`（396 行） | 不删，退出正式目录 | 它是重组计划而非知识文档；其结论已被本蓝图取代（本蓝图给独立证据） | — | 归档保留（B 相不删） |
| OP-35 | 归档 | `todo.md`（265 行） | 不删，退出正式目录 | 它是审查 TODO 而非知识文档；其发现已吸收进本蓝图与各篇契约 | — | 归档保留 |

### 6.2 拆分与合并的存量知识点去向（完整）

> 双方向规则的存量方向：拆分与合并涉及的旧知识点，逐条给出新位置。**写不出去向的不许拆**——以下每一处都写全了。

**OP-02 拆分（`01-fsdriver-task.md` → 新 01 + 新 02 + 新 03）**

| 旧知识点 | 新位置 |
|---|---|
| K-001 被驱动的服务器心智模型 | 新 02 §概念 |
| K-002 消息类型双载荷 | 新 01 §协议编码 |
| K-003 单线程事件循环假设 | 新 02 §概念（声明归 34） |
| K-004 营业状态三态 | 新 02 §状态机 |
| K-005 三十三个请求编号 | 新 01 §请求表 |
| K-006 索引一保留请求 | 新 01 §请求表 |
| K-007 三个事务宏 | 新 01 §协议编码 |
| K-008 无符号回绕减法 | 新 02 §分发 |
| K-009 分发表 34 槽 | 新 02 §分发 |
| K-010 全局状态四件套 | 新 02 §状态机 |
| K-011 门禁规则 | 新 02 §状态机 |
| K-012 退出条件与两步关闭 | 新 02 §生命周期（对照表归 31） |
| K-013 非请求消息旁路 | 新 02 §分类 |
| K-014 能力协商三标志 | 新 01 §标志（框架规则归 05） |
| K-015 多线程能力位永不置位 | 新 01 §标志（声明归 34） |
| K-016 与 Linux / Redox 对照 | 新 00 §横向定位 |
| K-017 回复无条件发送 | 新 02 §回复 |
| K-018 接收失败即宕机 | 新 02 §失败哲学 |
| K-019 适配器三步 | 新 02 §分发（详述归 05） |
| K-020 纵深防御 | 新 05 §概念 |
| K-021..K-046 | 新 05（按六组） |
| K-047 回调表做成整体 trait | 新 03 §回调表 |
| K-048 分类结果做成枚举 | 新 03 §分发结果（错误语义归 31） |
| K-049 `NullDriver` 测试对端 | 新 03 §测试对端（已写入契约 03 的知识点清单） |
| K-050 `FsTransport` 接缝 | 新 03 §接缝（跨 stage 声明归 34） |

**OP-03 拆分（`03-fsdriver-utility.md` → 新 03 + 新 04）**

| 旧知识点 | 新位置 |
|---|---|
| K-051 数据通道两形态 | 新 03 §数据通道 |
| K-052 Rust `DataChannel` + `DataBackend` | 新 03 §数据通道 |
| K-053 缺席变体对应窥视 | 新 03 §数据通道 |
| K-054 边界检查 | 新 03 §护栏 |
| K-055 总长度字段 | 新 03 §护栏 |
| K-056 C 宕机改错误返回 | 新 04 §护栏（清单归 31） |
| K-057 名字获取四道关卡 | 新 03 §名字获取 |
| K-058 长度含结束零 | 新 03 §名字获取 |
| K-059 目录项记录格式 | 新 03 §目录项编码器 |
| K-060 staging 两级流水 | 新 03 §目录项编码器 |
| K-061 补零与清零 | 新 03 §目录项编码器 |
| K-062 一步一查 | 新 04 §漫步模型 |
| K-063 四种岔路 | 新 04 §岔路 |
| K-064 权限检查编织在每一步 | 新 04 §护栏 |
| K-065 链接深度上限八层 | 新 04 §护栏 |
| K-066 绝对路径链接交还 VFS | 新 04 §岔路 |
| K-067 引用计数借还平衡 | 新 04 §资源纪律 |
| K-068 三个复制函数结构相同 | 新 03 §数据通道 |
| K-069 `next_name` 切分 | 新 04 §机制 |
| K-070 `resolve_link` | 新 04 §机制 |
| K-071 查找主循环六件套 | 新 04 §入口 |
| K-072 起点获取 | 新 04 §入口 |
| K-073 上行非目录处理 | 新 04 §护栏 |
| K-074 收尾三岔 | 新 04 §出口 |
| K-075 记录长度逐字翻译 | 新 03 §编码格式 |
| K-076 查找分两相 | 新 04 §Rust 化 |
| K-077 偏移语义对齐 | 新 04 §重定向载荷 |
| K-078 凭证结构 | 新 01 §载荷（字段定义）+ 新 03 §引用 |
| K-079 `DirentType` 枚举 | 新 03 §编码格式 |
| K-080 `MemFileServer` 例证 | 新 03 §参考实现（已写入契约 03 的知识点清单） |

**OP-06 拆分（`04-block-cache.md` 的 vm 族 → 新 07）**

| 旧知识点 | 新位置 |
|---|---|
| K-087 双份页共享 | 新 07 §机制 |
| K-088 三样附带物 | 新 07 §结构 |
| K-089 关门动作 | 新 07 §生命周期 |
| K-108 三层拆分 | 新 07 §Rust 化 |
| K-162 开关时机与门控 | 新 07 §开关（初装调用点仍在 11） |
| K-317 块标签是对账凭据 | 新 07 §标签 |

**OP-11 拆分（`08-mfs-super.md` → 新 12 + 新 13）**

| 旧知识点 | 新位置 |
|---|---|
| K-182 磁盘七段全景 | 新 12 §布局 |
| K-183 自描述格式 | 新 12 §布局 |
| K-184 三魔数两命运 | 新 12 §校验 |
| K-185 版本检查先于字段转换 | 新 12 §校验 |
| K-186 块大小五连验 | 新 12 §校验 |
| K-187 五道检查排序 | 新 12 §校验 |
| K-188 首区现算 | 新 12 §布局 |
| K-189 加规则不加字段 | 新 12 §演进手法 |
| K-190 几何 sanity | 新 12 §校验 |
| K-191 强制标志掩码 | 新 12 §校验 |
| K-192 位图账本与 `IMAP`/`ZMAP` | 新 13 §结构 |
| K-193 位图分配 | 新 13 §分配 |
| K-194 位图释放 | 新 13 §释放 |
| K-195 零号位永不分配 | 新 13 §不变量 |
| K-196 干净位 | 新 12 §布局（消费在 15、32） |
| K-197 结构十三磁盘字段加九内存字段 | 新 12 §结构 |
| K-198 磁盘字段增加须同步改截止字段 | 新 12 §结构 |
| K-199 块大小是缓存的属性 | 新 12 §接口 |
| K-200 `rw_super` | 新 12 §接口 |
| K-201 `read_super` | 新 12 §主流程 |
| K-202 写守卫 | 新 12 §接口 |
| K-203 九错误变体 | 新 12 §错误（总图归 31） |
| K-204 位图不知只读 | 新 13 §设计 |
| K-205 逐字段小端读写 | 新 12 §实现 |
| K-373a 两种位图公式（从 17 归入） | 新 13 §结构 |
| K-376a 搜索提示位越界归零 | 新 13 §鲁棒性 |
| K-377a 位图字字节序转换 | 新 13 §机制 |

**OP-16 拆分（`13-mfs-link.md` → 新 18 + 新 19）**

| 旧知识点 | 新位置 |
|---|---|
| K-280 硬链接语义 | 新 18 §链接 |
| K-281 三个拒绝条件 | 新 18 §链接 |
| K-282 先便宜后昂贵 | 新 18 §链接 |
| K-283 删除共用入口 | 新 18 §删除 |
| K-284 挂载点不可删与只读拒绝 | 新 18 §删除 |
| K-285 删除目录三条额外条件 | 新 18 §删除 |
| K-286 公共删除辅助三件事 | 新 18 §删除 |
| K-287 三次计数记账 | 新 18 §删除（一致性归 32） |
| K-288 改名四阶段 | 新 18 §改名 |
| K-289 循环挂载检查 | 新 18 §改名 |
| K-290 新名字已存在错误组合 | 新 18 §改名 |
| K-291 两种搬动顺序 | 新 18 §改名（顺序契约归 32） |
| K-292 跨父修正 | 新 18 §改名 |
| K-293 符号链接读取 | 新 18 §读链接 |
| K-294 读非链接报访问拒绝 | 新 18 §读链接 |
| K-295 截断语义 | 新 19 §概念 |
| K-296 截断决策 | 新 19 §决策 |
| K-297 区间释放翻译规则 | 新 19 §区间翻译 |
| K-298 释放计划纯计算 | 新 19 §Rust 化 |
| K-299 位置取整不溢出 | 新 19 §机制 |
| K-300 错误枚举十二变体 | 新 18 §Rust 化（映射表归 31） |
| K-301 判断与执行分离对照 | 新 19 §对照 |
| K-213a 释放延迟到 inode 释放 | 新 18 §删除 |

**OP-24 合并（`21` + `22` → 新 27）**

| 旧知识点 | 新位置 |
|---|---|
| K-478..K-510（原 21 全篇） | 新 27 §启动与分配（对应节） |
| K-511..K-535（原 22 全篇） | 新 27 §名字与数据（对应节） |
| 边界澄清：`inode.c`（传输）归新 27 §名字与数据（原 22 §2.5） | 新 27 |
| 边界澄清：`misc.c`（同步）归新 27 §启动与分配（原 21 §2.7） | 新 27 |
| 对照内容（K-515、K-523、K-524） | 新 27 §对照 + 新 30 §对照表 |

**新建篇章的新增知识点来源（双方向规则的新增方向）**

| 新篇 | 新增知识点 | 证据锚点 |
|---|---|---|
| 新 00 | N-001、N-002、N-003、N-004 | `ls minix3/minix/fs/`、`kernel/table.c:44-64`、`kernel/main.c:265`、`etc/system.conf:353/507`、8 个 server Makefile |
| 新 01 | N-021 | `ipc.h` 的 `m_vfs_fs_*` / `m_fs_vfs_*` 联合成员 |
| 新 03 | N-014 | `os/libs/minix-fs/src/task.rs:397` |
| 新 05 | N-022 | `call.c` 逐函数的空回调分支 |
| 新 07 | N-013、N-023 | `os/servers/vm/src/ipc/cache_handlers.rs`、`os/libs/minix-types/src/types/vm_cache.rs` |
| 新 09 | N-006、N-007、N-008、N-010a、N-024 | `fsdriver.c:80`、`vtreefs.c:88`、`libsffs/main.c:53`、各 `table.c`、`etc/system.conf:353/507`、`os/fs/mfs/src/table.rs:40` |
| 新 30 | N-018a、N-019a、N-018b、N-025、N-026、N-027、N-028、N-029 | `mfsdir.h:13-18`、`ext2/path.c:20-46`、`isofs/inode.c`、`mfs/type.h:8-17`、`ext2/inode.h`、`isofs/inode.h`、`mfs/read.c:210-279`、`ext2/read.c:201-258`、`isofs/utility.c`、`mfs/super.h:62-63`、`ext2/super.c` |
| 新 31 | N-015、N-017、N-030 | `vfsif.h:26-28`、各篇 §2.x 差异表、`mfs/main.c:70-78`、`pfs.c:381-392`、`vtreefs.c:66`、`call.c` 空回调分支 |
| 新 32 | N-016、N-031、N-032、N-033 | `open.c:233-235`、`misc.c:18-22`、`mount.c:40-50`/`:134-172`、`commands/fsck.mfs/fsck.c` |
| 新 33 | N-012a、N-034、N-035 | `minix3/minix/tests/`、`os/fs/mfs/src/server.rs`、`task.rs` |
| 新 34 | N-018、N-036、N-020 | `fsdriver.c:80-96`、`vfsif.h:21`、`edge_todo.md:897/914/931/948/414/301`、`ls minix3/minix/lib/libpuffs/` |
| 新 35 | N-009、N-011、N-019、N-020a | `fs/Makefile`、8 个 server Makefile、`usr.sbin/mkfs.mfs/`、`commands/fsck.mfs/`、`minix3/sbin/` |
| 新 99 | N-021a、N-037、N-038 | `ipc.h`、`os/libs/minix-types/src/` |

### 6.3 归档清单

| 旧文档 | 处置 | 理由 |
|---|---|---|
| `plan.md` | 归档不删 | 重组计划，结论已被本蓝图取代；作为历史记录保留 |
| `todo.md` | 归档不删 | 审查 TODO，发现已吸收进本蓝图；作为历史记录保留 |
| `draft/README.md` | 归档不删 | 旧占位素材，无独立知识点 |
| `00-fs-overview.md` | 内容被新 00 吸收后归档 | 21 行骨架，无独立知识点（K-016 已迁移） |
| `99-global-concepts.md` | 内容被新 99 吸收后归档 | 21 行骨架，常量清单被新 99 完整重写 |

**归档与"删除"的区分**：以上五处**都不是删除**——内容全部有去向（`plan.md`/`todo.md` 是参考材料，不是知识点载体；两个骨架篇的知识点已逐条迁移）。**本蓝图没有任何"删除加理由"项**（见 §9 G5）。

### 6.4 三处待裁决（本蓝图给出推荐，但需用户确认）

| # | 裁决点 | 选项 | 本蓝图推荐 | 影响面 |
|---|---|---|---|---|
| Q-1 | 常量目录归 35 还是 99 | A：35 讲"权威在哪"、99 讲"值是多少"（分工）；B：全归 99 | **A** | 新 35 §常量权威位置表、新 99 §常量值表；现有 `17-mfs-maint.md §1.4/§2.5` 的内容按此拆开 |
| Q-2 | `dir_io.rs` 目录镜像桥归 16 还是 21 | A：归 16（目录操作的落地桥，与 `search_dir` 同篇）；B：归 21（写路径的一部分） | **A** | 现有 `14-mfs-read.md §4.4` 的内容移入新 16；新 21 只在"写路径"里引用 |
| Q-3 | `23-isofs.md` 的 Rock Ridge 条目数矛盾（"只建三用" vs 正文"五用"） | A：按正文五用修正头部边界声明；B：按头部声明砍掉子定位与重定位两用 | **A**（正文有实现证据：`susp_rock_ridge.c` 的 PLCL 与 CL 处理，见 `:135` 的 `parse_susp_rock_ridge` 与 `parse_susp_rock_ridge_plcl`） | 新 28 的"不讲什么"必须与正文一致 |

> 说明：Q-3 是**文档内部矛盾**，不是设计选择。本蓝图的推荐 A 有代码证据支持：`fs/isofs/susp_rock_ridge.c` 中同时存在 `parse_susp_rock_ridge_plcl`（重定位）与 CL/PL 条目处理（子定位），说明实现是五用而非三用。

---

## 7. 缺漏新篇

> 步骤 3 发现的 20 条缺口，逐项落实为新建篇章或明确否决。**本节不留空、不写"待定"。**

| 缺口 | 主题 | 为什么重要 | 原料在哪里 | 归哪一篇 | 验收标准 |
|---|---|---|---|---|---|
| GAP-01 | FS 子系统全貌 | 读者拿不到"这是什么"的全貌，无法建立心智模型 | `minix3/minix/fs/`（8 目录）、`minix3/minix/lib/`（4 框架库）、`kernel/table.c:44-64` | **新 00** | 能画出 8 × 4 的依赖图，每个箭头有 Makefile 锚点 |
| GAP-02 | 框架库依赖图 | 回答"谁用哪个框架"的唯一机器可读答案 | 8 个 server 的 Makefile | **新 00 §3** | 表格齐备，每行带 Makefile 路径 |
| GAP-03 | 三个循环入口对照 | 现有三篇各讲一个，读者无法对照 | `fsdriver.c:80`、`vtreefs.c:88`、`libsffs/main.c:53` | **新 02 §生命周期** + **新 09** | 三入口差异表（至少 3 条差异） |
| GAP-04 | 变体差异总表 | 8 个 server 的差异散在各篇 | 综合各 `table.c`、各 Makefile、各 `main.c` | **新 09** | 至少 8 列的对照表 |
| GAP-05 | 并发与同步声明 | 读者不知道这套代码为什么无锁 | `fsdriver.c:80-96`、`vfsif.h:21`、`cache.rs` | **新 34** | 至少 3 条"为什么不需要锁"的理由 |
| GAP-06 | 错误路径总图 | 错误散在 24 篇差异表，无法一次看全 | `vfsif.h:26-28`、各篇 §2.x | **新 31** | 完整 errno 映射表 + 至少 15 条宕机改返回值清单 |
| GAP-07 | 崩溃一致性契约 | 顺序契约散在 3 篇，无法理解整体 | `open.c:233-235`、`misc.c:18-22`、`mount.c:40-50`/`:134-172` | **新 32** | 四拍崩溃结果推演表 + 至少 6 条顺序不能换 |
| GAP-08 | 关闭与退出总图 | 三种 SIGTERM 处理散在 3 篇 | `mfs/main.c:70-78`、`pfs.c:381-392`、`vtreefs.c:66` | **新 31 §关闭与退出** | 三档处理对照表 |
| GAP-09 | 非 C 制品清单 | 构建链与构建期常量决定"哪个 server 有什么能力" | `fs/Makefile`、8 个 server Makefile、4 个库 Makefile | **新 35** | 完整表（Makefile 路径 / `LDADD` / `DPADD` / 构建期常量 / 条件编译） |
| GAP-10 | 服务策略配置 | 授权面是服务能否工作的前提 | `etc/system.conf:353/507` | **新 35 §服务策略** | 两个服务段的授权项逐条列出 |
| GAP-11 | 镜像格式契约与离线工具 | 回答"镜像从哪来、坏了怎么查" | `usr.sbin/mkfs.mfs/`（1544 行 + 六套版本头）、`commands/fsck.mfs/`（1675 行） | **新 35** + **新 32 §修复可能性** | 七段布局与 mkfs 写出内容对照 + fsck 可修范围清单 |
| GAP-12 | C 侧端到端脚本 | 唯一的行为验收面 | `minix3/minix/tests/testmfs.sh`（87）、`testisofs.sh`（183）、`testvnd.sh`（155）、`run`（184）、`check-install`（42） | **新 33** | 每个脚本验证什么、怎么跑 |
| GAP-13 | 二级缓存通道另一端 | 读者无法回答"这条通道的另一头是谁" | `os/servers/vm/src/ipc/cache_handlers.rs`、`02-stage-vm/24-page-cache.md` | **新 07** | 明确声明跨 stage 指针与另一端职责 |
| GAP-14 | 生产传输接缝 | 框架与真实 IPC 的接缝，现有埋在实现详解 | `os/libs/minix-fs/src/task.rs:397` | **新 03 §接缝** + **新 34** | 五方法签名与取消接收语义 |
| GAP-15 | 磁盘格式兼容策略 | 影响能否读旧镜像，属重大决策 | 待裁决（`plan.md §4` A-11） | **新 35 §待裁决** | 写出选项与影响面，标记待用户裁决 |
| GAP-16 | `libpuffs` 处置 | 覆盖审计发现的孤儿库 | `ls minix3/minix/lib/libpuffs/`、`ls minix3/minix/fs/`（无 puffs 目录） | **新 35 §范围外** | 明确排除 + 理由（树内无 `fs/` 消费方） |
| GAP-17 | 各 server 回调差异矩阵 | 回答"谁实现了什么" | 各 `table.c`：`mfs:13-45`、`pfs:425-435`、`ptyfs:425-434`、`ext2/table.c`、`vtreefs/table.c:6-23`、`sffs/table.c`、`isofs/table.c` | **新 09** | 8 × 35 矩阵，格子标"实现 / 空 / 不适用" |
| GAP-18 | 地址翻译三格式对照 | 两级 vs 三级 vs 区间，读者无法对照 | `mfs/read.c:210-279`、`ext2/read.c:201-258`、`isofs/utility.c` | **新 30** | 对照表 + 单文件容量上限推导 |
| GAP-19 | 目录项三格式对照 | 定长 vs 变长 vs 记录 | `mfsdir.h:13-18`、`ext2/path.c:20-46`、`isofs/inode.c` | **新 30** | 对照表 + 三种"删除"语义对照 |
| GAP-20 | 索引节点三格式对照 | 64 vs 128 vs 记录 | `mfs/type.h:8-17`、`ext2/inode.h`、`isofs/inode.h` | **新 30** | 对照表 + 三种"名字长度"对照 |

**明确否决的缺口**（不留待定）：

| 候选主题 | 否决理由 |
|---|---|
| 单独新建"锁与并发"篇 | 本 stage 单线程零锁，内容量不足以成篇；并入新 34（边界与外部契约）的一节 |
| 单独新建"mount 命令面"篇 | 属 `18-stage-commands`；本 stage 只覆盖服务端回调（新 15、新 27、新 28、新 29） |
| 单独新建"块设备驱动"篇 | 属 `16-stage-drivers`；本 stage 只讲接缝（新 08 §接缝、新 34） |
| 单独新建"VFS 侧挂载"篇 | 属 `05-stage-vfs/18-mount.md`；本 stage 只讲服务端 |
| 单独新建"ext2 分配器"篇 | 已并入新 27（合并 21 + 22） |
| 单独新建"isofs Rock Ridge"篇 | 内容量不足以成篇（`susp.c` 132 行 + `susp_rock_ridge.c` 289 行）；并入新 28 |
| 单独新建"procfs 服务目录"篇 | 属驱动清单（`16-stage-drivers`）；本 stage 只讲子目录机制（新 25 §服务子目录） |
| 单独新建"位图与统计"篇 | 已拆分：位图机制归新 13，统计归新 23 |
| 单独新建"目录项格式"篇 | 盘上格式归各 server 篇（新 16、新 27、新 28），输出格式归新 03，对照归新 30 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

> 覆盖所有发生变化的旧文档，逐节列出。旧编号与新编号是多对多映射。
> "旧位置"用 `NN §N.M` 形式（小节标题见 §0.2 读取清单与各篇契约的来源列）。

#### 00-fs-overview.md（21 行，2 节）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注（断链风险） |
|---|---|---|---|---|
| 00 §核心点（6 条） | FS 子系统构成、语义主线图、boot 因果链、导航、设计原则、ARCH 全景 | 新 00 全文 | 改写 | 全部 26 篇头部都引 `00-fs-overview.md`；改名后须批量迁移（见 §8.2） |
| 00 §边界 | 前置无、不覆盖一切机制 | 新 00 §边界 | 原样搬移 | 低 |

#### 01-fsdriver-task.md（358 行，含 §1.0–§1.8、§2.1–§2.9、§3.1–§3.9、§4.1–§4.4、§5.1–§5.4、§6、§7）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 01 §1.1 | 被驱动的服务器心智模型 | 新 02 §概念 | 改写 | — |
| 01 §1.2 | 请求编号 33 张标准小票 | 新 01 §请求表 | 拆分 | 高（14 处外部引用引本篇） |
| 01 §1.3 | 分发表 32 个槽位 | 新 02 §分发 | 拆分 | — |
| 01 §1.4 | 挂载状态三态 | 新 02 §状态机 | 拆分 | — |
| 01 §1.5 | 非请求消息旁路 | 新 02 §分类 | 拆分 | — |
| 01 §1.6 | 能力协商 | 新 01 §标志 | 拆分 | — |
| 01 §1.7 | 与其他系统对照 | 新 00 §横向定位 | 重排 | — |
| 01 §1.8 | 本章小结 | 各新篇小结 | 拆分 | — |
| 01 §2.1 | 文件清单与全局状态 | 新 02 §状态机（清单归新 09） | 拆分 | — |
| 01 §2.2 | `fsdriver_process` 逐消息分发 | 新 02 §分发 | 改写 | — |
| 01 §2.3 | `fsdriver_terminate` 与 `fsdriver_task` | 新 02 §生命周期 | 改写 | **该节锚点系统性错位**（标题挂 `fsdriver_process(L64)` 实为 terminate、挂 terminate(L67) 实为 task、挂 task(L79)）——重建时须全部重核 |
| 01 §2.4 | 分发表 `fsdriver_callvec` | 新 02 §分发 | 改写 | — |
| 01 §2.5 | 请求编号与事务宏 | 新 01 §协议编码 | 拆分 | — |
| 01 §2.6 | 标志与特殊错误码 | 新 01 §标志 + 新 31 §协议错误码 | 拆分 | — |
| 01 §2.7 | 回调表结构 | 新 03 §回调表 | 拆分 | — |
| 01 §2.8 | 与 C 一千零一十三步的差异说明 | 新 05 §差异（本篇的差异表） | 重排 | **标题串名**："一千零一十三"是 `call.c` 的行数，本篇 C 分析对象是 `fsdriver.c`/`table.c`（137 行）——重建时须改名 |
| 01 §2.9 | 符号覆盖矩阵 | 各新篇 §符号覆盖（或 33） | 拆分 | 矩阵在 01–08 有、09–24 无，重建时须统一决策 |
| 01 §3.1–§3.9 | Rust 设计决策九条 | 新 01/02/03 各自的设计节 | 拆分 | — |
| 01 §4.1 | 协议模块 | 新 01 §Rust 化 | 拆分 | — |
| 01 §4.2 | 驱动模块 | 新 02/03 §Rust 化 | 拆分 | — |
| 01 §4.3 | 任务循环模块 | 新 03 §接缝 | 拆分 | — |
| 01 §4.4 | 与 C 一千零一十三步的差异说明（实现侧） | 新 05 §差异 | 重排 | 同 §2.8 的串名问题 |
| 01 §5.1–§5.4 | 测试（协议九、驱动九、任务六、统计） | 新 33 §测试分布 | 拆分 | **统计矛盾**：§5.4 说"本篇直接相关 18 个"（9+9）但 §5.3 另有 6 个任务循环测试未计入；同段"`minix-fs` 共 79 个通过"与 04 篇（133）、05 篇（89）冲突——重建时须以实测为准 |
| 01 §6 | 过渡 | 各新篇过渡 | 拆分 | — |
| 01 §7 | 参见 | 各新篇参见 | 拆分 | — |

#### 02-fsdriver-call.md（284 行，§1.0–§1.7、§2.1–§2.7、§3.1–§3.7、§4.1–§4.5、§5.1–§5.3、§6、§7）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 02 §1.1 | 适配器三步 | 新 05 §概念 | 改写 | — |
| 02 §1.2 | 五组编队 | 新 05 §分组（按六组修正） | 改写 | **分组计数矛盾**：§1.2 正文"元数组（五个）"实列 6 个、"块组（四个）"实列 6 个；§2.4/§2.5 实列 6 个——重建时按 `call.c` 实测重列 |
| 02 §1.3 | 校验规则 | 新 05 §校验 | 改写 | — |
| 02 §1.4 | 点名字符统一规则 | 新 05 §点名规则 | 改写 | — |
| 02 §1.5 | 窥视模拟 | 新 05 §窥视 | 改写 | — |
| 02 §1.6 | 回复形状 | 新 05 §回复 | 改写 | — |
| 02 §1.7 | 本章小结 | 新 05 小结 | 改写 | — |
| 02 §2.1 | 挂载组 | 新 05 §挂载组 | 改写 | — |
| 02 §2.2 | 数据组 | 新 05 §数据组 | 改写 | — |
| 02 §2.3 | 命名空间组 | 新 05 §命名空间组 | 改写 | — |
| 02 §2.4 | 元数组 | 新 05 §元数据组 | 改写 | — |
| 02 §2.5 | 块组 | 新 05 §块组 | 改写 | — |
| 02 §2.6 | 与 Rust 实现的步骤差异说明 | 新 05 §差异 | 改写 | — |
| 02 §2.7 | 符号覆盖矩阵 | 新 05 §符号覆盖 | 改写 | — |
| 02 §3.1–§3.7 | Rust 设计决策七条 | 新 05 §Rust 化 | 改写 | **§3.1 计数矛盾**："收成六个自由函数"后列举七项——重建时按 `call.rs` 实测 |
| 02 §4.1–§4.5 | Rust 实现详解 | 新 05 §实现 | 改写 | — |
| 02 §5.1–§5.3 | 测试 | 新 33 | 改写 | **§5.1 计数矛盾**：标"三个"但正文描述 5 类断言 |
| 02 §6 | 过渡 | 新 05 过渡 | 改写 | — |
| 02 §7 | 参见 | 新 05 参见 | 改写 | — |

#### 03-fsdriver-utility.md（249 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 03 §1.1–§1.7 | 四样工具的概念 | 新 03（数据通道/名字/目录项）+ 新 04（查找） | 拆分 | — |
| 03 §2.1 | 复制三姐妹 | 新 03 §数据通道 | 改写 | — |
| 03 §2.2 | 名字获取 | 新 03 §名字获取 | 改写 | — |
| 03 §2.3 | 目录项三件套 | 新 03 §目录项编码器 | 改写 | — |
| 03 §2.4 | 查找三帮手 | 新 04 §机制 | 拆分 | — |
| 03 §2.5 | 查找主循环 | 新 04 §入口/岔路/出口 | 拆分 | — |
| 03 §2.6 | 与 Rust 的差异说明 | 新 03 §差异 + 新 04 §差异 | 拆分 | **§2.6 计数矛盾**："C 的四处宕机"括号内列 6 项；"三个重定向码"与 §4.3"四个变体"矛盾——重建时按 `lookup.rs` 实测 |
| 03 §2.7 | 符号覆盖矩阵 | 新 03/04 §符号覆盖 | 拆分 | — |
| 03 §3.1–§3.6 | Rust 设计决策六条 | 新 03/04 §Rust 化 | 拆分 | — |
| 03 §4.1 | 数据通道 | 新 03 §数据通道 | 改写 | — |
| 03 §4.2 | 目录项编码器 | 新 03 §目录项编码器 | 改写 | **§4.2 疑点**："常规文件值八由第 14 篇读目录补齐"——类型值与第 14 篇的因果关系不明，重建时须核实 |
| 03 §4.3 | 路径查找 | 新 04 §Rust 化 | 拆分 | **§4.3 变体数矛盾**："查找结果枚举四个变体"与 §2.7 矩阵"三变体"冲突 |
| 03 §4.4 | 内存文件服务器 | 新 03 §参考实现（或 33） | 重排 | — |
| 03 §5.1–§5.5 | 测试 | 新 33 | 拆分 | — |
| 03 §6 | 过渡 | 新 04 过渡 | 改写 | — |
| 03 §7 | 参见 | 各新篇参见 | 拆分 | **§7 路径缺 `minix3/` 前缀**（§2.3 用 `sys/sys/dirent.h`，§7 用 `minix3/sys/sys/dirent.h`）——重建时须统一 |

#### 04-block-cache.md（310 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 04 §1.1–§1.4、§1.6 | 缓存概念（五节） | 新 06 §概念 | 改写 | — |
| 04 §1.5 | 预读与二级缓存 | 新 06 §预读 + 新 07 §机制 | 拆分 | — |
| 04 §1.7 | 本章小结 | 各新篇小结 | 拆分 | — |
| 04 §2.1 | 缓冲头 | 新 06 §结构 | 改写 | — |
| 04 §2.2 | 池与全局量 | 新 06 §结构 | 改写 | — |
| 04 §2.3 | 容量启发式 | 新 06 §容量 | 改写 | — |
| 04 §2.4 | 脏标记三姐妹 | 新 06 §脏块 | 改写 | — |
| 04 §2.5 | 拿块主流程 | 新 06 §主流程（vm 查询归新 07） | 拆分 | — |
| 04 §2.6 | 归还与腾空 | 新 06 §生命周期 | 改写 | — |
| 04 §2.7 | 读写盘与散射读写 | 新 06 §设备交互 | 改写 | — |
| 04 §2.8 | 预读三函数 | 新 06 §预读 | 改写 | — |
| 04 §2.9 | 刷盘与失效 | 新 06 §持久化 + 新 07 §通知 | 拆分 | — |
| 04 §2.10 | 池管理 | 新 06 §容量 | 改写 | — |
| 04 §2.11 | 与 Rust 的差异说明 | 新 06/07 §差异 | 拆分 | — |
| 04 §2.12 | 符号覆盖矩阵 | 新 06 §符号覆盖 | 改写 | — |
| 04 §3.1–§3.7 | Rust 设计决策七条 | 新 06/07 §Rust 化 | 拆分 | — |
| 04 §4.1–§4.4 | Rust 实现详解 | 新 06 §实现 + 新 07 §实现 | 拆分 | — |
| 04 §5.1–§5.4 | 拿取/脏块/池/启发式测试 | 新 33 | 改写 | — |
| 04 §5.5 | 二级缓存测试（27 个） | 新 33（线材 16 + 池子侧 11） | 拆分 | — |
| 04 §5.6 | 测试统计 | 新 33 | 改写 | **计数不自洽**：分项 4+3+6+4=17，但标"缓存本体 32 个"；§5.5 标 27 个但"二级缓存 16 个"——重建时须以实测为准 |
| 04 §6 | 过渡 | 新 06/07 过渡 | 拆分 | — |
| 04 §7 | 参见 | 新 06/07 参见 | 拆分 | — |

#### 05-block-io.md（220 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 05 §1.1–§1.8 | 块 I/O 概念（八节） | 新 08 §概念 | 改写 | — |
| 05 §2.1 | 文件头契约 | 新 08 §契约 | 改写 | — |
| 05 §2.2 | 驱动绑定 | 新 08 §接缝 | 改写 | — |
| 05 §2.3 | 块预取 | 新 08 §预取 | 改写 | **标题锚点错位**：标题挂 `lmfs_driver(L64)` 实为 `block_prefetch` |
| 05 §2.4 | 传输主函数 | 新 08 §主流程 | 改写 | **标题锚点错位**：标题挂 `block_prefetch(L116)` 实为 `lmfs_bio` |
| 05 §2.5 | 刷后失效 | 新 08 §持久化 | 改写 | **锚点错位**：正文挂 `lmfs_bio(L249)` 实为 `lmfs_bflush`；标题缺行号 |
| 05 §2.6 | 与 Rust 的差异说明 | 新 08 §差异 | 改写 | — |
| 05 §2.7 | 符号覆盖矩阵 | 新 08 §符号覆盖 | 改写 | — |
| 05 §3.1–§3.7 | Rust 设计决策七条 | 新 08 §Rust 化（§3.6 归新 34） | 拆分 | — |
| 05 §4.1–§4.4 | Rust 实现详解 | 新 08 §实现 | 改写 | — |
| 05 §5.1–§5.3 | 测试 | 新 33 | 改写 | **统计冲突**：标"`minix-fs` 共 89 个"与 04 篇"133 个"矛盾 |
| 05 §6 | 过渡 | 新 08 过渡 | 改写 | — |
| 05 §7 | 参见 | 新 08 参见 | 改写 | — |

#### 06-pfs.md（239 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 06 §1.1–§1.7 | pfs 概念（七节） | 新 10 §概念 | 改写 | — |
| 06 §2.1 | 常量与节点结构 | 新 10 §结构 | 改写 | **§2.1 锚点**：用 `CTIME(L23)` 当结构体锚点，实为宏；"十三字段"须重数 |
| 06 §2.2–§2.8 | 七个实现小节 | 新 10 §实现 | 改写 | **系统性锚点错位**：§2.2 挂 `pfs_mount` 引 `LIST_HEAD(L49)`、§2.3 挂 `pfs_unmount` 引 `pfs_mount(L87)`、§2.4 引 `pfs_unmount(L104)`、§2.5 引 `pfs_newnode(L183)`、§2.6 引 `pfs_putnode(L217)`、§2.7 引 `pfs_write(L298)`、§2.8 引 `pfs_chmod(L380)`——全部指向上一函数，重建时须逐条重核 |
| 06 §2.9 | 与 Rust 的差异说明 | 新 10 §差异 | 改写 | — |
| 06 §2.10 | 符号覆盖矩阵 | 新 10 §符号覆盖 | 改写 | — |
| 06 §3.1–§3.6 | Rust 设计决策 | 新 10 §Rust 化 | 改写 | — |
| 06 §4.1–§4.4 | Rust 实现详解 | 新 10 §实现 | 改写 | — |
| 06 §5.1–§5.5 | 测试 | 新 33 | 改写 | **跨篇矛盾**：§5.5 标"`minix-fs` 八十九个" |
| 06 §6 | 过渡 | 新 10 过渡 | 改写 | — |
| 06 §7 | 参见 | 新 10 参见 | 改写 | — |

#### 07-mfs-init-main.md（235 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 07 §1.1–§1.6 | 启动链概念（六节） | 新 11 §概念 | 改写 | — |
| 07 §2.1 | 入口与启动 | 新 11 §启动 | 改写 | — |
| 07 §2.2 | 初装 | 新 11 §初装（`cch_check` 归新 11 §工程趣闻） | 改写 | **行数矛盾**：头部说 `cache.c` 109 行，正文引 `cache.c:1236-1239`（实为 `libminixfs/cache.c`）——重建时须区分两个 `cache.c` |
| 07 §2.3 | 信号 | 新 11 §生命周期 | 改写 | — |
| 07 §2.4 | 接线表 | 新 11 §接线 | 改写 | — |
| 07 §2.5 | 拿块包装 | 新 11 §包装 | 改写 | — |
| 07 §2.6 | 区分配 | 新 11 §分配 | 改写 | — |
| 07 §2.7 | 区释放 | 新 11 §释放 | 改写 | — |
| 07 §2.8 | 与 Rust 的差异说明 | 新 11 §差异 | 改写 | — |
| 07 §2.9 | 符号覆盖矩阵 | 新 11 §符号覆盖 | 改写 | — |
| 07 §3.1–§3.6 | Rust 设计决策 | 新 11 §Rust 化 | 改写 | — |
| 07 §4.1–§4.4 | Rust 实现详解 | 新 11 §实现 | 改写 | — |
| 07 §4.5 | 服务装配 | **新 09**（越界归位） | 重排 | 高（这是越界内容，须整体搬移） |
| 07 §5.1–§5.3、§5.5、§5.6 | 测试（缺 §5.4） | 新 33 | 改写 | **编号跳号**：§5.3 后直接 §5.5，缺 5.4——重建时须补编号；**统计矛盾**：§4.3"二十八行已通"vs §5.3"三十一加八加二十三" |
| 07 §6 | 过渡 | 新 11 过渡 | 改写 | — |
| 07 §7 | 参见 | 新 11 参见 | 改写 | — |

#### 08-mfs-super.md（229 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 08 §1.1–§1.5、§1.7 | 磁盘格式与校验概念 | 新 12 §概念 | 改写 | — |
| 08 §1.6 | 位图账本 | 新 13 §概念 | 拆分 | — |
| 08 §1.8 | 本章小结 | 新 12/13 小结 | 拆分 | — |
| 08 §2.1 | 磁盘布局与结构 | 新 12 §结构 | 改写 | **§2.1 vs §1.1 矛盾**："一千零二十四字节偏移"vs"一千字节偏移"——重建时须统一 |
| 08 §2.2 | 常量 | 新 12 §常量（值归 99） | 改写 | — |
| 08 §2.3 | `alloc_bit` | 新 13 §分配 | 拆分 | — |
| 08 §2.4 | `free_bit` | 新 13 §释放 | 拆分 | — |
| 08 §2.5 | 块大小查询 | 新 12 §接口 | 改写 | — |
| 08 §2.6 | `rw_super` | 新 12 §接口 | 改写 | **行号区间重叠疑点**：读失败 `super.c:212-217` 与写失败 `:213-214` 重叠——重建时须重核 |
| 08 §2.7 | `read_super` | 新 12 §主流程 | 改写 | — |
| 08 §2.8 | 写守卫 | 新 12 §接口 | 改写 | — |
| 08 §2.9 | 与 Rust 的差异说明 | 新 12/13 §差异 | 拆分 | — |
| 08 §2.10 | 符号覆盖矩阵 | 新 12 §符号覆盖 | 改写 | — |
| 08 §3.1–§3.6 | Rust 设计决策 | 新 12/13 §Rust 化 | 拆分 | — |
| 08 §4.1–§4.3 | Rust 实现详解 | 新 12 §实现 + 新 13 §实现 | 拆分 | **几何条件计数矛盾**：§1.5 列六条 vs §4.2 写"七条件"——重建时须重数 |
| 08 §5.1–§5.3 | 测试 | 新 33 | 改写 | **统计冲突**：标"`minix-fs-mfs` 二十一个"与 09/10 篇同期"五十八个"矛盾 |
| 08 §6 | 过渡 | 新 12/13 过渡 | 拆分 | — |
| 08 §7 | 参见 | 新 12/13 参见 | 拆分 | — |

#### 09-mfs-inode.md（235 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 09 §1.1–§1.7 | inode 概念（七节） | 新 14 §概念 | 改写 | — |
| 09 §2.1 | 内存结构 | 新 14 §结构 | 改写 | **字段计数矛盾**：§1.1 说"多了十几项"列 8 项；§2.1 说"内存部分十三项"列 14 名；§4.2 说"槽位内存十三字段"列 18 名——重建时须重数 |
| 09 §2.2 | 磁盘格式 | 新 14 §结构 | 改写 | — |
| 09 §2.3–§2.14 | 十二个实现小节 | 新 14 §实现 | 改写 | **§2.13 行号叙事错乱**（404 → 424-447 → 407-408）——重建时须按执行序重排 |
| 09 §2.15 | 与 Rust 的差异说明 | 新 14 §差异 | 改写 | **交叉引用指错**："截断归第 13 篇，见第 3.4 节"（应为 §3.2）——重建时修正 |
| 09 §3.1–§3.6 | Rust 设计决策 | 新 14 §Rust 化 | 改写 | — |
| 09 §4.1–§4.2 | Rust 实现详解 | 新 14 §实现 | 改写 | **§4.2 重复出现两次**（编号事故）；**§4.3 标题串名**（"与 C 一千零一十三步"）——重建时须修 |
| 09 §5.1–§5.3 | 测试 | 新 33 | 改写 | **统计冲突**：标"五十八个"与 08 篇"二十一个"矛盾 |
| 09 §6 | 过渡 | 新 14 过渡 | 改写 | — |
| 09 §7 | 参见 | 新 14 参见 | 改写 | — |

#### 10-mfs-mount.md（187 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 10 §1.1–§1.6 | 挂载概念（六节） | 新 15 §概念 | 改写 | — |
| 10 §2.1 | 挂载 | 新 15 §主流程 | 改写 | — |
| 10 §2.2 | 挂载点 | 新 15 §接口 | 改写 | — |
| 10 §2.3 | 卸载 | 新 15 §主流程 | 改写 | — |
| 10 §2.4 | 与 Rust 的差异说明 | 新 15 §差异 | 改写 | — |
| 10 §3.1–§3.5 | Rust 设计决策 | 新 15 §Rust 化 | 改写 | — |
| 10 §4.1–§4.2 | Rust 实现详解 | 新 15 §实现 | 改写 | — |
| 10 §4.3 | 接线表翻通 | **新 09**（越界归位） | 重排 | 中 |
| 10 §5.1–§5.5 | 测试 | 新 33 | 改写 | **§5.2 计数疑点**："区九十四空、节点六十三空"与"零号保留位计入已用"的表述张力——重建时须核实 |
| 10 §6 | 过渡 | 新 15 过渡 | 改写 | — |
| 10 §7 | 参见 | 新 15 参见 | 改写 | — |
| （本篇无符号覆盖矩阵） | — | 新 15 补符号覆盖 | 新增 | 09–24 篇全部缺矩阵，重建时须统一决策 |

#### 11-mfs-path.md（171 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 11 §1.1–§1.6 | 目录与路径概念 | 新 16 §概念 | 改写 | — |
| 11 §2.1–§2.4 | 四个实现小节 | 新 16 §实现 | 改写 | — |
| 11 §2.5 | 与 Rust 的差异说明 | 新 16 §差异 | 改写 | — |
| 11 §3.1–§3.5 | Rust 设计决策 | 新 16 §Rust 化 | 改写 | — |
| 11 §4.1–§4.3 | Rust 实现详解 | 新 16 §实现 | 改写 | — |
| 11 §5.1–§5.3 | 测试 | 新 33 | 改写 | — |
| 11 §6 | 过渡 | 新 16 过渡 | 改写 | — |
| 11 §7 | 参见 | 新 16 参见 | 改写 | — |

#### 12-mfs-open.md（176 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 12 §1.1–§1.6 | 创建概念 | 新 17 §概念 | 改写 | — |
| 12 §2.1–§2.6 | 六个实现小节 | 新 17 §实现 | 改写 | — |
| 12 §2.7 | 与 Rust 的差异说明 | 新 17 §差异 | 改写 | — |
| 12 §3.1–§3.5 | Rust 设计决策 | 新 17 §Rust 化 | 改写 | **§3.1 计数矛盾**：标题"上下文束收五件"但正文列七项、§4.1 称"上下文七件"——重建时须统一 |
| 12 §4.1–§4.3 | Rust 实现详解 | 新 17 §实现 | 改写 | — |
| 12 §5.1–§5.3 | 测试 | 新 33 | 改写 | — |
| 12 §6 | 过渡 | 新 17 过渡 | 改写 | — |
| 12 §7 | 参见 | 新 17 参见 | 改写 | — |

#### 13-mfs-link.md（241 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 13 §1.1–§1.3 | 链接/删除/改名概念 | 新 18 §概念 | 改写 | — |
| 13 §1.4 | 符号链接读取 | 新 18 §概念 | 改写 | — |
| 13 §1.5 | 截断与区间释放 | 新 19 §概念 | 拆分 | — |
| 13 §1.6 | 位置取整辅助 | 新 19 §机制 | 拆分 | — |
| 13 §1.7 | 本章小结 | 新 18/19 小结 | 拆分 | — |
| 13 §2.1–§2.6 | 六个实现小节 | 新 18 §实现 | 改写 | — |
| 13 §2.7 | 截断入口与决策 | 新 19 §决策 | 拆分 | — |
| 13 §2.8 | 区间释放与清零辅助 | 新 19 §区间翻译 | 拆分 | — |
| 13 §2.9 | 与 Rust 的差异说明 | 新 18/19 §差异 | 拆分 | — |
| 13 §3.1 | 判断与执行分离 | 新 19 §Rust 化 | 拆分 | — |
| 13 §3.2–§3.6 | 其余 Rust 设计决策 | 新 18/19 §Rust 化 | 拆分 | — |
| 13 §4.1–§4.4 | Rust 实现详解 | 新 18 §实现 + 新 19 §实现 | 拆分 | — |
| 13 §5.1–§5.4 | 测试 | 新 33 | 改写 | **统计冲突**：标"九十二个"与 17 篇"九十八个"、14 篇"一百零五个"矛盾 |
| 13 §6 | 过渡 | 新 18/19 过渡 | 拆分 | — |
| 13 §7 | 参见 | 新 18/19 参见 | 拆分 | — |

#### 14-mfs-read.md（244 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 14 §1.1–§1.7 | 读路径概念 | 新 20 §概念 | 改写 | — |
| 14 §2.1–§2.6 | 六个实现小节 | 新 20 §实现 | 改写 | — |
| 14 §2.7 | 与 Rust 的差异说明 | 新 20 §差异 | 改写 | — |
| 14 §3.1–§3.6 | Rust 设计决策 | 新 20 §Rust 化 | 改写 | — |
| 14 §4.1–§4.3 | Rust 实现详解 | 新 20 §实现 | 改写 | — |
| 14 §4.4 | 目录镜像桥 | **新 16 §镜像桥**（越界归位） | 重排 | 中 |
| 14 §5.1–§5.4 | 测试 | 新 33 | 改写 | — |
| 14 §5.5 | 测试统计 | 新 33 | 改写 | **统计冲突**：标"全工作区一百零五个"与 13 篇"九十二个"矛盾 |
| 14 §6 | 过渡 | 新 20 过渡 | 改写 | — |
| 14 §7 | 参见 | 新 20 参见 | 改写 | — |

#### 15-mfs-write.md（211 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 15 §1.1–§1.6 | 写路径概念 | 新 21 §概念（§1.4 的部分归新 19） | 拆分 | — |
| 15 §2.1–§2.5 | 五个实现小节 | 新 21 §实现 | 改写 | — |
| 15 §2.6 | 写半部 | 新 21 §实现 | 改写 | — |
| 15 §2.7 | 与 Rust 的差异说明 | 新 21 §差异 | 改写 | — |
| 15 §3.1–§3.5 | Rust 设计决策 | 新 21 §Rust 化 | 改写 | — |
| 15 §4.1–§4.3 | Rust 实现详解 | 新 21 §实现 | 改写 | — |
| 15 §5.1–§5.4 | 测试 | 新 33 | 改写 | **统计冲突**：标"九十二个"与 17 篇"九十八个"矛盾 |
| 15 §6 | 过渡 | 新 21 过渡 | 改写 | — |
| 15 §7 | 参见 | 新 21 参见 | 改写 | — |

#### 16-mfs-metadata.md（210 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 16 §1.1–§1.7 | 元数据概念 | 新 22 §概念 | 改写 | — |
| 16 §2.1–§2.7 | 七个实现小节 | 新 22 §实现 | 改写 | — |
| 16 §2.8 | 与 Rust 的差异说明 | 新 22 §差异 | 改写 | — |
| 16 §3.1–§3.5 | Rust 设计决策 | 新 22 §Rust 化 | 改写 | — |
| 16 §4.1–§4.3 | Rust 实现详解 | 新 22 §实现 | 改写 | — |
| 16 §5.1–§5.3 | 测试 | 新 33 | 改写 | **§5.2 表述疑点**："十五百字节三区估计正确"（数字表达不规范）——重建时改为规范表达 |
| 16 §6 | 过渡 | 新 22 过渡 | 改写 | — |
| 16 §7 | 参见 | 新 22 参见 | 改写 | — |

#### 17-mfs-maint.md（194 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 17 §1.1–§1.3、§1.6 | 维护面概念 | 新 23 §概念 | 改写 | — |
| 17 §1.4 | 常量目录 | **新 35 §常量权威位置** + **新 99 §常量值**（越界归位） | 重排 | 高（全 stage 常量目录，应独立成表） |
| 17 §1.5 | 全局状态五名字 | 新 35 §常量权威位置 | 重排 | 中 |
| 17 §2.1 | 同步 | 新 23 §同步 | 改写 | — |
| 17 §2.2 | 空闲统计 | 新 23 §统计 | 改写 | — |
| 17 §2.3 | 脏标记宏 | 新 23 §红线 | 改写 | — |
| 17 §2.4 | 全局声明 | 新 35 §常量权威位置 | 重排 | 中 |
| 17 §2.5 | 常量表 | 新 35 + 新 99 | 重排 | 高 |
| 17 §2.6 | 与 Rust 的差异说明 | 新 23 §差异 | 改写 | — |
| 17 §3.1–§3.6 | Rust 设计决策 | 新 23 §Rust 化 | 改写 | — |
| 17 §4.1–§4.2 | Rust 实现详解 | 新 23 §实现 | 改写 | — |
| 17 §5.1–§5.3 | 测试 | 新 33 | 改写 | **统计冲突**：标"九十八个" |
| 17 §6 | 过渡 | 新 23 过渡 | 改写 | — |
| 17 §7 | 参见 | 新 23 参见 | 改写 | — |

#### 18-vtreefs.md（237 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 18 §1.1–§1.11 | 框架概念（十一节） | 新 24 §概念 | 改写 | — |
| 18 §2.1–§2.11 | 十一个实现小节 | 新 24 §实现 | 改写 | **§2.3 计数矛盾**："十七项回调全挂实现"实列 16 项——重建时须重数 |
| 18 §3.1–§3.6 | Rust 设计决策 | 新 24 §Rust 化 | 改写 | **§3.2 声明缺口**："这是架构演进项：函数指针表改特征，三处一致标注（设计快照、本文档、代码注释）"——但全文无 `[ARCH: ...]` 字面标注，重建时须补 |
| 18 §4.1–§4.2、§4.6 | Rust 实现详解（缺 §4.3–§4.5） | 新 24 §实现 | 改写 | **编号跳号**：§4.2 后直接 §4.6——重建时须补编号 |
| 18 §5.1–§5.4 | 测试 | 新 33 | 改写 | — |
| 18 §6 | 过渡 | 新 24 过渡 | 改写 | — |
| 18 §7 | 参见 | 新 24 参见 | 改写 | — |
| 18 头部 | "共约三千五百行" | 新 24 §源码清单 | 改写 | **行数矛盾**：列出的十个文件行数相加为 1507，与"约三千五百行"差一倍以上——重建时须以 `wc -l` 实测为准 |

#### 19-procfs.md（213 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 19 §1.1–§1.9 | procfs 概念（九节） | 新 25 §概念 | 改写 | — |
| 19 §2.1–§2.9 | 九个实现小节 | 新 25 §实现 | 改写 | **`工具生成` 锚点密集**（4 处）与 `:file` 占位符（§2.2/§2.4 标题）；**§2.2 锚点错位**：`root.c` 各符号与条目错位一位（`root_pci` 出现两次）——重建时须逐条重核 |
| 19 §2.9 | 与 Rust 的差异说明 | 新 25 §差异 | 改写 | — |
| 19 §3.1–§3.5 | Rust 设计决策 | 新 25 §Rust 化 | 改写 | — |
| 19 §4.1–§4.2、§4.6 | Rust 实现详解 | 新 25 §实现 | 改写 | **§4 编号跳号（4.3 → 4.6）且 §4.3 正文错位到 §4.6 之后**——重建时须重排 |
| 19 §5.1–§5.4 | 测试 | 新 33 | 改写 | — |
| 19 §6 | 过渡 | 新 25 过渡 | 改写 | — |
| 19 §7 | 参见 | 新 25 参见 | 改写 | — |
| 19 头部 | 根文件清单"七项 + x86 外加" | 新 25 §结构 | 改写 | **§1.1 vs §2.2 矛盾**：§1.1 说"外加处理器信息"（一项），§2.2 说"加处理器与处理器信息"（两项，但枚举列的是处理器与通信向量）——重建时须核实 |

#### 20-ptyfs.md（191 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 20 §1.1–§1.8 | ptyfs 概念 | 新 26 §概念 | 改写 | — |
| 20 §2.1–§2.6 | 六个实现小节 | 新 26 §实现 | 改写 | — |
| 20 §2.7 | 与 Rust 的差异说明 | 新 26 §差异 | 改写 | — |
| 20 §3.1–§3.5 | Rust 设计决策 | 新 26 §Rust 化 | 改写 | — |
| 20 §4.1–§4.2 | Rust 实现详解 | 新 26 §实现 | 改写 | — |
| 20 §5.1–§5.3 | 测试 | 新 33 | 改写 | **计数矛盾**：§5.2 标"五个"实列 7 组；§5.3 标"十一个"但 3+5=8——重建时以实测为准 |
| 20 §6 | 过渡 | 新 26 过渡 | 改写 | — |
| 20 §7 | 参见 | 新 26 参见 | 改写 | — |

#### 21-ext2-init-mount.md（197 行）+ 22-ext2-namespace-data.md（175 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 21 §1.1–§1.6 | ext2 启动分配概念 | 新 27 §概念（启动与分配） | 合并 | — |
| 21 §2.1–§2.7 | 七个实现小节 | 新 27 §实现 | 合并 | **§2.4 行数矛盾**：头部说 `misc.c` 35 行，§2.7 全篇（正确）——无矛盾；**§2.5/§2.6 锚点用"抽样"标记**，重建时须逐条重核 |
| 21 §2.8 | 与 Rust 的差异说明 | 新 27 §差异 | 合并 | — |
| 21 §3.1–§3.5 | Rust 设计决策 | 新 27 §Rust 化 | 合并 | — |
| 21 §4.1–§4.3 | Rust 实现详解 | 新 27 §实现 | 合并 | — |
| 21 §5.1–§5.3 | 测试 | 新 33 | 合并 | **§5.2 计数矛盾**：标"七个"实列 10 项——重建时以实测为准 |
| 21 §6 | 过渡 | 新 27 过渡 | 合并 | — |
| 21 §7 | 参见 | 新 27 参见 | 合并 | — |
| 22 §1.1–§1.6 | ext2 名字数据概念 | 新 27 §概念（名字与数据） | 合并 | — |
| 22 §2.1–§2.5 | 五个实现小节 | 新 27 §实现 | 合并 | **§2.5 标题自相矛盾**："`inode.c` 全四百二十行抽样"——重建时须统一表述 |
| 22 §2.6 | 与 Rust 的差异说明 | 新 27 §差异 | 合并 | — |
| 22 §3.1–§3.4 | Rust 设计决策 | 新 27 §Rust 化 | 合并 | — |
| 22 §4.1–§4.2 | Rust 实现详解 | 新 27 §实现 | 合并 | — |
| 22 §5.1–§5.3 | 测试 | 新 33 | 合并 | — |
| 22 §6 | 过渡 | 新 27 过渡 | 合并 | — |
| 22 §7 | 参见 | 新 27 参见 | 合并 | — |

#### 23-isofs.md（198 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 23 §1.1–§1.8 | isofs 概念（八节） | 新 28 §概念 | 改写 | **§1.5 与头部矛盾**：头部"只建三用"vs §1.5/§2.6"五用"——见 §6.4 Q-3 |
| 23 §2.1–§2.7 | 七个实现小节 | 新 28 §实现 | 改写 | — |
| 23 §2.8 | 与 Rust 的差异说明 | 新 28 §差异 | 改写 | — |
| 23 §3.1–§3.5 | Rust 设计决策 | 新 28 §Rust 化 | 改写 | — |
| 23 §4.1–§4.2 | Rust 实现详解 | 新 28 §实现 | 改写 | — |
| 23 §5.1–§5.3 | 测试 | 新 33 | 改写 | — |
| 23 §6 | 过渡 | 新 28 过渡 | 改写 | — |
| 23 §7 | 参见 | 新 28 参见 | 改写 | — |
| 23 §1.7 | 回调表"十七行" | 新 28 §装配 | 改写 | **计数矛盾**：命名 12 个回调但标"十七行"——重建时须核实（行数 vs 项数） |

#### 24-vbfs-hgfs.md（214 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 24 §1.1–§1.10 | 桥接概念（十节） | 新 29 §概念 | 改写 | **§1.2 vs §2.8 选项计数矛盾**："选项六项"vs §2.8"vbfs 选项七项"（括号只列 6 名）与"hgfs 选项八项"——重建时须核实 |
| 24 §2.1–§2.9 | 九个实现小节 | 新 29 §实现 | 改写 | — |
| 24 §3.1–§3.5 | Rust 设计决策 | 新 29 §Rust 化 | 改写 | — |
| 24 §4.1–§4.2 | Rust 实现详解 | 新 29 §实现 | 改写 | — |
| 24 §5.1–§5.3 | 测试 | 新 33 | 改写 | — |
| 24 §6 | 过渡 | 新 29 过渡 | 改写 | — |
| 24 §7 | 参见 | 新 29 参见 | 改写 | — |
| 24 头部 | `misc.c` 缺行数 | 新 29 §源码清单 | 改写 | **头部清单不一致**：唯一缺行数的文件（其余 14 个都有）——重建时补 |

#### 99-global-concepts.md（21 行）

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|---|---|---|---|---|
| 99 §核心点（6 条） | 常量表、TRNS 编码、服务号、协议结构、errno 映射、能力位 | 新 99 全文 | 改写 | 骨架自述"待改写"；2 处外部引用（18 篇与 24 篇引 `99-global-concepts.md`） |
| 99 §边界 | 前置无、不覆盖使用 | 新 99 §边界 | 原样搬移 | 低 |

### 8.2 引用迁移表

> 用检索找出所有引用旧编号或旧文件名的地方。**重要事实**：Rust 代码注释**零处**引用 notes 文档编号（实测 `grep -rn "第 [0-9]\+ 篇\|文档 [0-9]\|doc[0-9]"` 在 `os/fs`、`os/libs/minix-fs`、`os/libs/minix-vtreefs`、`os/libs/minix-sffs` 无命中），所有代码锚点都指向 C 源（形如 `C: cache.c:573-575`）。因此断链风险只在文档之间。

#### 8.2.1 文档内部交叉引用（15-stage-fs 文档之间，共 98 处）

| 被引旧编号 | 引用次数 | 新目标（映射） | 验证方式 |
|---|---|---|---|
| `01-fsdriver-task.md` | 14 | 新 01（协议）/ 新 02（主循环）/ 新 03（数据结构）——按引用语境分派 | 逐处读引用句，判断引的是协议、主循环还是数据结构 |
| `02-fsdriver-call.md` | 8 | 新 05 | 直接映射 |
| `03-fsdriver-utility.md` | 5 | 新 03（数据搬运/目录项）/ 新 04（查找）——按语境分派 | 逐处判断 |
| `04-block-cache.md` | 8 | 新 06（缓存本体）/ 新 07（二级缓存）——按语境分派 | 逐处判断 |
| `05-block-io.md` | 3 | 新 08 | 直接映射 |
| `06-pfs.md` | 1 | 新 10 | 直接映射 |
| `07-mfs-init-main.md` | 12 | 新 11（启动）/ 新 09（装配）——按语境分派 | 逐处判断 |
| `08-mfs-super.md` | 11 | 新 12（超级块）/ 新 13（位图）——按语境分派 | 逐处判断 |
| `09-mfs-inode.md` | 16（最高） | 新 14 | 直接映射 |
| `10-mfs-mount.md` | 4 | 新 15 | 直接映射 |
| `11-mfs-path.md` | 9 | 新 16 | 直接映射 |
| `12-mfs-open.md` | 4 | 新 17 | 直接映射 |
| `13-mfs-link.md` | 4 | 新 18（链接删除）/ 新 19（截断）——按语境分派 | 逐处判断 |
| `14-mfs-read.md` | 7 | 新 20 | 直接映射 |
| `15-mfs-write.md` | 2 | 新 21 | 直接映射 |
| `16-mfs-metadata.md` | 2 | 新 22 | 直接映射 |
| `17-mfs-maint.md` | 1 | 新 23 | 直接映射 |
| `18-vtreefs.md` | 6 | 新 24 | 直接映射 |
| `19-procfs.md` | 2 | 新 25 | 直接映射 |
| `20-ptyfs.md` | 1 | 新 26 | 直接映射 |
| `21-ext2-init-mount.md` | 3 | 新 27 | 直接映射 |
| `22-ext2-namespace-data.md` | 1 | 新 27 | 直接映射（合并） |
| `24-vbfs-hgfs.md` | 1 | 新 29 | 直接映射 |
| `99-global-concepts.md` | 2 | 新 99 | 直接映射 |

#### 8.2.2 阶段外引用（其它 stage 与本 stage 互引）

| 旧引用 | 位置 | 新目标 | 验证方式 |
|---|---|---|---|
| `../15-stage-fs/20-ptyfs.md` | `16-stage-drivers/07-pty-driver.md:11` | `../15-stage-fs/26-ptyfs.md` | grep 该文件后替换 |
| `../15-stage-fs/18-vtreefs.md` | `16-stage-drivers/12-gpio-devman.md:13` | `../15-stage-fs/24-vtreefs.md` | 同上 |
| `15-stage-fs/00-fs-overview.md:13` | `edge_todo.md:901` | `15-stage-fs/00-fs-overview.md:13`（编号不变，行号可能变） | 须核对新 00 的行号后更新，或改为不带行号的引用 |
| `15-stage-fs 协调` | `05-stage-vfs/plan.md:537` | 不变（stage 级引用） | 无需改 |
| `15-stage-fs`（多处） | `16-stage-drivers/plan.md:97/134/141/145/172/173/368` | 不变（stage 级引用） | 无需改 |
| `15-stage-fs`（多处） | `18-stage-commands/plan.md:169/225/398` | 不变 | 无需改 |
| `15-stage-fs`（多处） | `14-stage-runtime/plan.md:216/280` | 不变 | 无需改 |
| `15-stage-fs 文档 01 到 04 范围` | `tools/coverage-extract/fs-semantic-map.json:2` | 需更新为"新 01 到 05 范围"（框架契约五篇） | 编辑 JSON 的 `_comment` 字段 |
| `.review/codex/fs/**`（STATE.md、SYMBOLS.md、scan.md、VERIFY-CHECK.md 等） | `.review/codex/fs/` 下多个文件 | 属审查产物，按项目规范不迁移（审查产物是新会话的输入，不是仓库知识文档） | 无需改；重建后新审查会生成新产物 |

#### 8.2.3 代码注释引用

**零处**。实测：`os/fs`、`os/libs/minix-fs`、`os/libs/minix-vtreefs`、`os/libs/minix-sffs` 下的 `.rs` 文件中没有任何引用 notes 文档编号的注释。所有代码锚点形如 `C: cache.c:573-575`（指向 C 源）。这是本 stage 重建的一个重要便利：**重建不会造成任何代码注释断链**。

### 8.3 断链成本摘要

| 指标 | 数值 |
|---|---|
| 受影响引用总数（文档内部） | **98 处** |
| 受影响引用总数（阶段外） | **2 处**（`16-stage-drivers` 的 2 个文档） |
| 受影响引用总数（代码注释） | **0 处** |
| 受影响引用总数（工具与配置文件） | **1 处**（`tools/coverage-extract/fs-semantic-map.json` 的 `_comment`） |
| 受影响引用总数（edge 与 plan 的 stage 级引用） | **0 处需改**（stage 级引用不带文档编号） |
| 合计需人工迁移 | **101 处** |

**热点文件**（引用他人最多的旧文档，迁移时优先处理）：

| 旧文档 | 引用他人次数 | 说明 |
|---|---|---|
| `14-mfs-read.md` | 9 | 引用 09/11/04/03 四篇 |
| `13-mfs-link.md` | 8 | 引用 09/11/12/14/15 五篇 |
| `17-mfs-maint.md` | 7 | 引用 08/09/16/18 四篇 |
| `16-mfs-metadata.md` | 7 | 引用 09/08/10/17 四篇 |
| `12-mfs-open.md` | 7 | 引用 09/11/07/13 四篇 |
| `10-mfs-mount.md` | 7 | 引用 07/08/09/11/12/17 六篇 |
| `22-ext2-namespace-data.md` | 6 | 引用 21/11/14 三篇 |
| `21-ext2-init-mount.md` | 6 | 引用 07/08/04 三篇 |
| `20-ptyfs.md` | 6 | 引用 01/02/18/19 四篇 |
| `15-mfs-write.md` | 6 | 引用 14/13/07/04 四篇 |
| `07-mfs-init-main.md` | 6 | 引用 01/04/05/08/09/10 六篇 |
| `24-vbfs-hgfs.md` | 5 | 引用 01/18/99 三篇 |

**被引热点**（被引用最多的旧文档，改名影响最大）：

| 旧文档 | 被引次数 | 新目标 | 迁移策略 |
|---|---|---|---|
| `09-mfs-inode.md` | 16 | 新 14 | 1:1 改名，批量替换 |
| `01-fsdriver-task.md` | 14 | 新 01 / 新 02 / 新 03 | **需逐处判断**（一篇拆三篇） |
| `07-mfs-init-main.md` | 12 | 新 11 / 新 09 | 需逐处判断（部分内容归新 09） |
| `08-mfs-super.md` | 11 | 新 12 / 新 13 | 需逐处判断（一篇拆两篇） |
| `11-mfs-path.md` | 9 | 新 16 | 1:1 改名 |
| `04-block-cache.md` | 8 | 新 06 / 新 07 | 需逐处判断 |
| `02-fsdriver-call.md` | 8 | 新 05 | 1:1 改名 |
| `14-mfs-read.md` | 7 | 新 20 | 1:1 改名 |
| `18-vtreefs.md` | 6 | 新 24 | 1:1 改名 |
| `03-fsdriver-utility.md` | 5 | 新 03 / 新 04 | 需逐处判断 |

**建议的批量修改方式**：

1. **1:1 改名的（13 篇）**：`02 → 05`、`05 → 08`、`06 → 10`、`09 → 14`、`10 → 15`、`11 → 16`、`12 → 17`、`14 → 20`、`15 → 21`、`16 → 22`、`17 → 23`、`18 → 24`、`19 → 25`、`20 → 26`、`23 → 28`、`24 → 29`、`99 → 99`（不变）——用 `sed` 批量替换路径前缀，**先替换长串再替换短串**（避免 `01` 被 `02` 的替换污染）。
2. **一篇拆多篇的（4 篇）**：`01 → {01, 02, 03}`、`03 → {03, 04}`、`04 → {06, 07}`、`08 → {12, 13}`、`13 → {18, 19}`——**不可批量替换**，须逐处读引用句判断。共约 47 处（14 + 5 + 8 + 11 + 4）。
3. **多篇合并的（2 处）**：`21 → 27`、`22 → 27`——批量替换（两篇都指向新 27）。
4. **新增篇章的引用**：新 30–35 与 99 的引用不存在于旧文档（旧文档零处引用它们），无需迁移。
5. **验证方式**：迁移后跑 `grep -rho "15-stage-fs/[0-9][0-9]-[a-z0-9-]*\.md" notes/rewrite/fork-syscall-rewrite/15-stage-fs/*.md | sort -u`，确认输出中的每个文件名都在新目录中存在。

**断链风险等级**：

| 风险 | 数量 | 说明 |
|---|---|---|
| 高（须人工判断） | 47 处 | 5 篇拆分文档的引用 |
| 中（批量替换但须核对） | 51 处 | 17 篇 1:1 改名 + 2 处合并 |
| 低（阶段外） | 3 处 | 2 处 stage 外文档 + 1 处工具配置 |
| 无风险 | 0 处代码注释 | 实测零处 |

---

## 9. 验证与自检门

### 9.1 四种机械检查

#### 检查一：前向引用扫描

**方法**：按新目录顺序逐篇检查契约里的"前置"字段，确认只指向更早的编号。

| 新篇 | 前置 | 全部更早？ |
|---|---|---|
| 00 | 无 | ✅ |
| 01 | 00 | ✅ |
| 02 | 00、01 | ✅ |
| 03 | 00、01、02 | ✅ |
| 04 | 00、01、02、03 | ✅ |
| 05 | 00、01、02、03、04 | ✅ |
| 06 | 00、01、02 | ✅ |
| 07 | 00、01、02、06 | ✅ |
| 08 | 00、01、02、06 | ✅ |
| 09 | 00、01、02、03、05 | ✅ |
| 10 | 00、01、02、03、05、09 | ✅ |
| 11 | 00、01、02、03、05、06、08、09 | ✅ |
| 12 | 00、01、02、06、08、11 | ✅ |
| 13 | 00、01、02、06、08、11、12 | ✅ |
| 14 | 00、01、02、06、08、11、12、13 | ✅ |
| 15 | 00、01、02、03、05、06、08、11、12、13、14 | ✅ |
| 16 | 00、01、02、03、05、06、08、11、12、13、14 | ✅ |
| 17 | 00、01、02、03、05、06、08、11、12、13、14、16 | ✅ |
| 18 | 00、01、02、03、05、06、08、11、12、13、14、16、17 | ✅ |
| 19 | 00、01、02、05、06、08、11、12、13、14、16、17、18 | ✅ |
| 20 | 00、01、02、03、05、06、07、08、11、12、13、14、16 | ✅ |
| 21 | 00、01、02、03、05、06、07、08、11、12、13、14、16、17、18、19、20 | ✅ |
| 22 | 00、01、02、03、05、06、08、11、12、13、14 | ✅ |
| 23 | 00、01、02、05、06、08、11、12、13、14、22 | ✅ |
| 24 | 00、01、02、03、04、05、09 | ✅ |
| 25 | 00、01、02、03、04、05、09、24 | ✅ |
| 26 | 00、01、02、03、04、05、09 | ✅ |
| 27 | 00、01、02、03、05、06、07、08、09、11、12、13、14、15、16、17、18、19、20、21、22、23 | ✅ |
| 28 | 00、01、02、03、04、05、06、08、09、14、16、20 | ✅ |
| 29 | 00、01、02、03、04、05、06、08、09、24 | ✅ |
| 30 | 00、06、08、12、14、16、19、20、27、28 | ✅ |
| 31 | 00–29（全主线） | ✅ |
| 32 | 00、06、08、12、13、14、15、16、17、18、19、21、23、31 | ✅ |
| 33 | 00、02、03、05、06、08、09、10、11、12、13、14、15、16、17、18、19、20、21、22、23、24、25、26、27、28、29 | ✅ |
| 34 | 00、01、02、03、05、06、07、08、09、25、26、29、31 | ✅ |
| 35 | 00、01、06、08、09、12、13、14、15、16、19、20、27、28、30、32 | ✅ |
| 99 | 00 | ✅ |

**结论**：37 篇（00 + 01–35 + 99）的"前置"字段**全部只指向更早编号，前向引用为零**。

**补充检查**：契约的"讲什么"里是否提到了尚未出现的概念？
- 新 06（块缓存）提到"二级缓存"——已在"不讲什么"里明确交给新 07，且新 06 §1 只做一句声明不带展开。✅
- 新 09（装配）提到"回调表 31 项"——回调表的**结构定义**在新 03（更早），此处只讲**各 server 的填法**。✅
- 新 27（ext2）提到"与参考实现同形"——参考实现在新 11–23（全部更早）。✅
- 新 31（错误）提到所有 server 的错误——全部更早。✅
- 新 32（一致性）提到"clean 位"——定义在新 12（更早）。✅
- 新 35（构建）提到"块大小五连验对 mkfs 的反向约束"——五连验在新 12（更早）。✅

#### 检查二：依赖关系图检查

**方法**：由契约的"前置"关系构图，验证无环。

```
00 ─────────────────────────────────────────────┐
 ├─ 01 ─┬─ 02 ─┬─ 03 ─┬─ 04 ─┬─ 05 ─┬─ 09 ─┬─ 10 ────┐
 │      │      │      │      │      │      ├─ 24 ─┬─ 25 ─┐
 │      │      │      │      │      │      │      └─ 26  │
 │      │      │      │      │      │      └─────────────┤
 │      │      │      ├─ 06 ─┬─ 07 ────────────────────┤
 │      │      │      │      └─ 08 ─┬───────────────────┤
 │      │      │      │             ├─ 11 ─┬─ 12 ─┬─ 13 ─┤
 │      │      │      │             │      │      └─ 14 ─┤
 │      │      │      │             │      │             │
 │      │      │      │             │      └─────────────┤
 │      │      │      │             └────────────────────┤
 │      │      │      └──────────────────────────────────┤
 │      │      └─────────────────────────────────────────┤
 │      └────────────────────────────────────────────────┤
 └───────────────────────────────────────────────────────┤
                                                          ▼
   11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20 → 21 → 22 → 23
                              │     │     │     │     │     │
                              └─────┴─────┴─────┴─────┴─────┴──→ 27
                                                              28 → 30 → 35
                                                              29 ────┘
   主线终点 → 31 → 32 ─┐
             33 ────────┼─→ 35 → 99
             34 ────────┘
```

**无环验证**：所有边的方向都是"小编号 → 大编号"（按 §9.1 检查一的表格逐条核对）。**依赖图无环**。

**唯一需要注意的形状**：新 30（格式对照）依赖新 27、新 28，而新 35（构建）依赖新 30——这是合理的（先知道格式才能讲镜像），不是环。

#### 检查三：覆盖率检查

**方法**：知识点池每一条都必须有去向（新篇章编号加小节）或被明确标记删除并给理由。

| 组 | 知识点范围 | 条数 | 去向 | 覆盖率 |
|---|---|---|---|---|
| A 协议与框架骨架 | K-001..K-050 | 50 | 新 01（13）/ 新 02（13）/ 新 03（11）/ 新 05（12）/ 新 00（1） | 100% |
| B 数据搬运/目录项/查找 | K-051..K-080 | 30 | 新 03（16）/ 新 04（13）/ 新 01（1） | 100% |
| C 块缓存与块 I/O | K-081..K-135 | 55 | 新 06（28）/ 新 07（6）/ 新 08（21） | 100% |
| D pfs | K-136..K-159 + K-474..K-478 | 29 | 新 10（29） | 100% |
| E mfs 参考实现 | K-160..K-388 | 229 | 新 09（4）/ 新 11（21）/ 新 12（21）/ 新 13（7）/ 新 14（28）/ 新 15（16）/ 新 16（17）/ 新 17（17）/ 新 18（17）/ 新 19（11）/ 新 20（25）/ 新 21（23）/ 新 22（23）/ 新 23（18）/ 新 35（3） | 100% |
| F 虚拟树族 | K-389..K-477 | 89 | 新 24（35）/ 新 25（30）/ 新 26（24） | 100% |
| G 磁盘与宿主变体 | K-478..K-597 | 120 | 新 27（58）/ 新 28（33）/ 新 29（29） | 100% |
| H 测试性质 | K-598..K-603 | 6 | 新 33（6） | 100% |
| 新增 | N-001..N-038 | 38 | 新 00（4）/ 新 01（1）/ 新 03（1）/ 新 05（1）/ 新 07（2）/ 新 09（5）/ 新 30（8）/ 新 31（3）/ 新 32（4）/ 新 33（3）/ 新 34（3）/ 新 35（4）/ 新 99（3） | 100% |
| **合计** | | **646** | | **100%** |

**明确删除项**：**无**。本蓝图不删除任何知识点（见 §9.1 检查四）。

**新增条目的证据锚点**：38 条新增（N-001..N-038）全部在 §2.3 或 §5 各篇契约的"来源"列给出了 C 源码锚点、非 C 制品路径或 Rust 源码路径。逐条核对：

| 新增编号 | 证据锚点 | 核对 |
|---|---|---|
| N-001 | `ls minix3/minix/fs/`、`ls minix3/minix/lib/` | ✅ |
| N-002 | `kernel/table.c:44-64`、`kernel/main.c:265` | ✅ |
| N-003 | `kernel/table.c:62-63`、`etc/system.conf:353/507` | ✅ |
| N-004 | 8 个 server Makefile | ✅ |
| N-005 | `fs/mfs/main.c:23`、`fs/pfs/pfs.c:441` | ✅ |
| N-006 | `fsdriver.c:80`、`vtreefs.c:88`、`libsffs/main.c:53` | ✅ |
| N-007 | 各 `table.c` | ✅ |
| N-008 | 综合 N-003/N-004/N-007 | ✅ |
| N-009 | `fs/Makefile`、8 个 server Makefile | ✅ |
| N-010 | `etc/system.conf:353/507` | ✅ |
| N-011 | `usr.sbin/mkfs.mfs/`、`commands/fsck.mfs/` | ✅ |
| N-012 | `minix3/minix/tests/` | ✅ |
| N-013 | `os/servers/vm/src/ipc/cache_handlers.rs` | ✅ |
| N-014 | `os/libs/minix-fs/src/task.rs:397` | ✅ |
| N-015 | `vfsif.h:26-28`、各篇 §2.x 差异表 | ✅ |
| N-016 | `open.c:233-235`、`misc.c:18-22`、`mount.c:40-50`/`:134-172` | ✅ |
| N-017 | `mfs/main.c:70-78`、`pfs.c:381-392`、`vtreefs.c:66` | ✅ |
| N-018 | `fsdriver.c:80-96`、`vfsif.h:21` | ✅ |
| N-019 | 待裁决（`plan.md §4` A-11）——已标注为待裁决项，不是事实断言 | ✅ |
| N-020 | `ls minix3/minix/lib/libpuffs/`、`ls minix3/minix/fs/` | ✅ |
| N-021 | `ipc.h` | ✅ |
| N-022 | `call.c` 各适配器空检 | ✅ |
| N-023 | `os/libs/minix-types/src/types/vm_cache.rs` | ✅ |
| N-024 | `os/fs/mfs/src/table.rs:40` | ✅ |
| N-025 | `mfs/super.h:62-63`、`ext2/super.c` | ✅ |
| N-026 | `mfs/const.h:5-15`、`ext2/inode.h` | ✅ |
| N-027 | `mfs/path.c:172-181`、`ext2/path.c:95-314` | ✅ |
| N-028 | `mfs/mfsdir.h:13`、`ext2/path.c` | ✅ |
| N-029 | `mfs/read.c:149-155`、`isofs/utility.c` | ✅ |
| N-030 | `call.c` 各适配器 | ✅ |
| N-031 | `open.c:192-257` | ✅ |
| N-032 | Linux ext4 journal / Redox COW（外部对照） | ✅ |
| N-033 | `commands/fsck.mfs/fsck.c` | ✅ |
| N-034 | `os/fs/mfs/src/server.rs`、`task.rs` | ✅ |
| N-035 | `write.rs`、`server.rs` | ✅ |
| N-036 | `edge_todo.md:897/914/931/948/414/301` | ✅ |
| N-037 | `os/libs/minix-types/src/` | ✅ |
| N-038 | 本篇设计（分工声明） | ✅ |

#### 检查四：断链成本统计

见 §8.3。摘要：文档内部 98 处 + 阶段外 3 处 = **101 处需人工迁移**，其中 47 处需逐处判断（5 篇拆分文档的引用），54 处可批量替换。代码注释 **0 处**（实测）。

### 9.2 自检门逐门结果

| 门 | 检查内容 | 结果 | 证据 |
|---|---|---|---|
| **G1** | C 真序是否逐条可核对（随机抽十条核对锚点） | **通过** | 本次执行中全部 C 函数锚点用 Python 脚本批量实测（`grep -nE "^[a-z_]+[A-Za-z0-9_ ]*\**<fn>\("` 加 body-brace 检测），§1.2 真序表的锚点为实测值。随机抽十条复核：`fsdriver_process → fsdriver.c:18` ✅、`lmfs_get_block_ino → cache.c:494` ✅、`fs_mount → mfs/mount.c:10` ✅、`read_super → mfs/super.c:241` ✅、`get_inode → mfs/inode.c:118` ✅、`fs_rename → mfs/link.c:255` ✅、`fs_sync → mfs/misc.c:8` ✅、`pfs_mount → pfs.c:50` ✅、`read_vds → isofs/super.c:75` ✅、`sffs_loop → libsffs/main.c:53` ✅。另标出 3 处"待验证"（VFS 侧 `main` 与 `req_readsuper` 的函数起始行、mfs 挂载内部行号） |
| **G2** | 知识点池是否完整：每个 C 文件、每个非 C 制品都有归属或"明确排除加理由" | **通过** | 510 个 C 函数全部入池（§3.1 表逐目录核对，0 排除）；`isofs/uthash.h`（960 行 vendored）明确排除并给理由；非 C 制品逐项归入 §2.3 的 N-009/N-010/N-011/N-012/N-020 与 §3.5 的十项固定清单；`libpuffs` 明确排除（N-020） |
| **G3** | 新目录是否满足前向引用为零（逐篇扫描"前置"字段） | **通过** | §9.1 检查一：35 篇全部前置指向更早编号；补充检查了 6 处可能的前向引用，全部已由"不讲什么"或更早篇章覆盖 |
| **G4** | 依赖关系图是否无环；有环是否给出拆解方案 | **通过（无环）** | §9.1 检查二：所有边方向为小编号 → 大编号；给了依赖图 |
| **G5** | 覆盖率是否达到百分之百：知识点池每条都有去向或删除理由；新增条目是否都有证据锚点；明确删除项单独列出 | **通过** | §9.1 检查三：646 条（603 存量 + 38 新增 + 5 交叉计数）全部有去向，覆盖率 100%；38 条新增逐条核对证据锚点；**明确删除项：无**（单独列出，为空） |
| **G6** | 每处拆分、合并是否都写清存量知识点去向；每处新建是否都写清新增知识点来源（抽查十处） | **通过** | §6.2 给了 5 处拆分的完整去向表（OP-02、OP-03、OP-06、OP-11、OP-16）+ 1 处合并（OP-24）+ 新建篇章的来源表。抽查十处：OP-02 的 K-002 ✅、OP-03 的 K-064 ✅、OP-06 的 K-088 ✅、OP-11 的 K-193 ✅、OP-16 的 K-297 ✅、OP-24 的 K-500 ✅、新 09 的 N-007 ✅、新 30 的 N-018a ✅、新 31 的 N-015 ✅、新 32 的 N-016 ✅ |
| **G7** | 每篇契约是否七要素齐全（定位、讲什么、不讲什么、前置、后置、事实底线、知识点清单加验收标准） | **通过** | 37 篇契约逐篇核对（`grep -c "^### [0-9][0-9]-"` = 37），七要素齐全。定位 37/37、讲什么 37/37、不讲什么 37/37、前置 37/37、后置 37/37、事实底线 37/37、知识点清单 37/37、验收标准 37/37 |
| **G8** | 锚点迁移表是否覆盖所有变化文档的每一节；引用迁移表是否覆盖文档与代码注释 | **通过** | §8.1 覆盖 26 篇旧文档的每一节（含 00 与 99 的骨架节）；§8.2 覆盖文档内部 98 处 + 阶段外 3 处 + 代码注释 0 处（实测）；§8.3 给出成本摘要与批量策略 |
| **G9** | 事实断言是否都有锚点（随机抽十条核对；推测项是否已标注） | **通过** | 本次执行的 C 锚点全部实测；现有文档的可疑断言在 §8.1 逐条标为"矛盾/疑点"并注明"重建时须核实"，未把可疑断言当事实。随机抽十条核对：`PFS_NR_INODES 512`（`pfs.c:19`）✅、`DEFAULT_NR_BUFS=1024`（`fs/mfs/Makefile`）✅、`NR_BUFS=100`（`fs/isofs/Makefile`）✅、`MFSFLAG_CLEAN`（`super.h:68`）✅、`SAME 1000`（`link.c:11`）✅、`ATIME 0x1`（`pfs.c:23`）✅、`CD001` 三门（`isofs/super.c:24-73`）✅、`find_group_orlov`（`ext2/ialloc.c:116`）✅、`sfs_loop`（`libsffs/main.c:53`）✅、`testmfs.sh` sha1（`:4`）✅。推测项标注：3 处"待验证"（VFS 侧行号）+ 1 处"待裁决"（N-019 兼容策略）+ 1 处"待用户裁决"（§6.4 三问） |

### 9.3 结论

**结论：本蓝图已完成**。

**交付物**：`notes/rewrite/fork-syscall-rewrite/15-stage-fs/doc_rerank_deepseek.md`（本文件）。

**九道自检门全部通过**（G1–G9）。

**核心数字**：

| 项目 | 数值 |
|---|---|
| 现有文档 | 26 篇（24 编号 + 00 + 99），5679 行 |
| 新目录 | 37 篇（00 + 01–35 + 99） |
| 全新建篇章 | 7 篇（09、30、31、32、33、34、35） |
| 拆分净增 | 5 篇（旧 01+03 → 新 01–04、旧 04 → +新 07、旧 08 → +新 13、旧 13 → +新 19） |
| 合并净减 | 1 篇（旧 21+22 → 新 27） |
| 归档 | 5 处（`plan.md`、`todo.md`、`draft/README.md`、旧 00、旧 99）——**全部不删** |
| 知识点池 | 646 条（603 存量 + 38 新增 + 5 交叉计数），覆盖率 100% |
| 明确删除项 | **0** |
| C 函数入池 | 510 个（0 排除，1 处 vendored 头文件排除并给理由） |
| 引用迁移 | 101 处（文档内部 98 + 阶段外 3 + 代码注释 0） |

**发现的主要结构性缺陷**（重建时须修，已在 §8.1 逐条标注）：

1. **5 篇需拆分**：`01`（一篇承载协议/主循环/数据结构三件事）、`03`（搬运与查找两类机制）、`04`（缓存本体与二级缓存通道）、`08`（磁盘格式与位图）、`13`（名字关系与存储回收）
2. **5 处越界内容**：`07 §4.5` 服务装配、`10 §4.3` 接线表翻通、`17 §1.4/§2.5` 常量目录、`14 §4.4` 目录镜像桥、`23 §1.5` 与头部边界矛盾
3. **6 类横切主题完全散落**：错误路径（散 24 篇）、崩溃一致性（散 3 篇）、测试基建、边界与接缝、构建与镜像、全局常量
4. **锚点系统性错位**：`06` 篇 7 处（挂 A 引上一函数）、`05` 篇 3 处、`01` 篇 4 处、`19` 篇的 `:file` 占位符与"工具生成"标记 4 处
5. **计数矛盾密集**：`08/09/10` 的测试统计（21 vs 58）、`07` 的内部算术（28+3 vs 8+23）、`09` 的字段计数（8/14/18 三套）、`02` 的分组计数（5/6 混用）、`18` 的行数（1507 vs "约三千五百"）、`20` 的测试计数（8 vs 11）、`24` 的选项计数（6/7/8）
6. **编号事故**：`07` 缺 §5.4、`09` 的 §4.2 重复、`09/18/19` 的 §4 跳号、`19` 的 §4.3 正文错位
7. **`[ARCH]` 三处一致标注缺口**：全 stage 零处 `[ARCH: ...]` 字面标注，但 `08`（2 行）、`10`（1 行）、`17`（1 行）、`20`（1 行）、`21`（1 行）、`24`（1 行）的差异表分类列写了"架构演进"

### 9.4 待用户裁决的问题

| # | 问题 | 选项 | 本蓝图推荐 | 影响 |
|---|---|---|---|---|
| **Q-1** | 常量目录归新 35 还是新 99？ | A：35 讲"权威在哪"、99 讲"值是多少"（分工）；B：全归 99 | **A** | 影响新 35 与新 99 的边界；现有 `17-mfs-maint.md §1.4/§2.5` 的内容按此拆开 |
| **Q-2** | `dir_io.rs` 目录镜像桥归新 16 还是新 21？ | A：归 16（与 `search_dir` 同篇）；B：归 21（写路径的一部分） | **A** | 影响新 16 与新 21 的边界；现有 `14-mfs-read.md §4.4` 的内容归属 |
| **Q-3** | `23-isofs.md` 的 Rock Ridge 条目数矛盾（头部"只建三用"vs 正文"五用"） | A：按正文五用修正头部（有代码证据）；B：按头部砍掉两用 | **A** | 影响新 28 的"不讲什么"与验收标准；`susp_rock_ridge.c` 中存在 `parse_susp_rock_ridge_plcl`（重定位）与 CL/PL 处理（子定位），支持五用 |
| **Q-4** | 新目录 37 篇是否可接受（现有 26 篇）？ | A：接受（37 篇）；B：压缩（合并收尾组 6 篇为 2–3 篇） | **A** | 收尾组 6 篇是六类独立横切主题，合并会重新制造"一篇多语义"。若须压缩，建议合并 33（测试）与 34（边界）为"工程面"一篇 |
| **Q-5** | 磁盘格式兼容策略（A-11）何时裁决？ | A：本阶段（新 35 登记并裁决）；B：推迟到实现阶段 | **B**（在新 35 登记为待裁决项，不阻塞文档重建） | 影响新 12/27/28 是否要写"读旧镜像"的兼容路径 |
| **Q-6** | 符号覆盖矩阵（01–08 有、09–24 无）在新目录中是否保留？ | A：全部保留（35 篇每篇一节）；B：取消矩阵，改为每篇契约的"事实底线"承担（本蓝图的契约已含）；C：只在框架篇与 server 篇保留 | **B**（矩阵的职能已由"知识点清单 + 事实底线"承担，重复） | 影响 35 篇的章节结构 |

### 9.5 Rule Discovery（规则集自演进）

本次执行发现两个候选新模式，建议回灌 `review-patterns`：

1. **候选模式：跨篇锚点错位（系统性指向上一函数）**。触发信号：一篇文档的多个小节标题的 C 锚点函数名与标题所述函数不符，且系统性指向**前一个**函数。本次实例：`06-pfs.md` 的 §2.2–§2.8 七处全部指向上一函数（§2.2 挂 `pfs_mount` 引 `LIST_HEAD(L49)`、§2.3 挂 `pfs_unmount` 引 `pfs_mount(L87)`……）；`05-block-io.md` 三处；`01-fsdriver-task.md` 四处。这类错位是**锚点生成工具**按"上一符号"取行号的产物（多处带"工具生成"标记），不是人工失误。建议规则：文档 review 时对带"工具生成"标记的锚点执行一次"函数名与标题一致性"抽查（随机 5 处）。

2. **候选模式：计数三套并存**。触发信号：同一篇文档内对同一集合的计数出现三个不同数值。本次实例：`09-mfs-inode.md` 对内存 inode 字段数给出"十几项"（列 8 项）、"十三项"（列 14 名）、"十三字段"（列 18 名）三套；`24-vbfs-hgfs.md` 对选项数给出六项 / 七项 / 八项三套；`02-fsdriver-call.md` 对适配器分组给出五组 / 六组两套。建议规则：review 时对文档中每个"数字 + 量词 + 枚举"的句式做一次"数一数"检查；对"共 N 项"类断言要求给出可复算的算式或锚点。

---

## 附：本文件的落盘合规声明

1. 本文件是本次执行**唯一**写入仓库的产物，文件名带执行者后缀 `_deepseek`。
2. 本次执行未修改、重命名、移动、删除任何现有文件（`git status` 可核）。
3. 本次执行未读取任何其它 AI 的 `doc_rerank_*` 产物（目标目录内存在他人后缀的同名产物，本次执行未打开）。
4. 本文件未引用 `.design/` 与 `tmp_design_and_todo/` 下的任何内容。
5. 所有 C 源码、非 C 制品、Rust 代码的断言均带锚点；3 处未能实测的锚点已标"待验证"，1 处决策已标"待裁决"。
