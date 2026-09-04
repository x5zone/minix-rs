# 18 — mount：设备编解码、挂载提交、卸载拆除与根换装

本文讲清挂载调用如何在“编解码→查忙占槽→挂载点粘合→读超块→落定”的五段提交中，以设备号、槽位、引用数为三组开关，把一棵外部文件系统嫁接到单树之上，并在卸载时按“查忙→清统计→放引用→调执行→归还”的七步拆除原样拆走——根是唯一的例外，它可以被挂两次，全员换家。

前置阅读：`06-vmnt-table.md`（`vmnt` 槽的占位与锁）、`13-path-lookup.md`（`eat_path` 的挂载点解析）、`01-vfs-init-main.md`（启动时的根挂载调用点）。

> 本章不讲什么：
> - `vmnt` 槽的存储与锁实现（占位/查找/加锁语义）—— `06-vmnt-table.md`
> - `req_readsuper/req_mountpoint/req_unmount` 的 FS 协议执行—— `12-request-wrappers.md`
> - `dmap` 驱动表的存储与映射—— `19-device-map.md`
> - 块 I/O 改道的驱动投递执行—— `20-bdev.md`
> - `update_statvfs/statvfs` 的统计语义—— `28-stadir.md`
> - 用户缓冲拷贝（`sys_datacopy_wrapper`）—— `99-global-concepts.md`

---

## 1 概念

### 1.1 为什么需要嫁接

一个 VFS 面对多台文件服务器（MFS、PFS、各路 ext2/isofs），而用户只想要一棵树。挂载就是嫁接：把某台 FS 的根 inode 缝到本树的一个目录（挂载点）上，此后经过该目录的解析自动穿越到另一台 FS（13 的穿越机读的正是此处缝下的 `m_root_node/m_mounted_on`）。

嫁接之前，VFS 只有启动时缝上的根；嫁接之后，任意目录都可以成为别树的入口。卸载是逆操作：剪断缝线，把目录还给原树。提交与拆除的对称性是本篇的主线——提交时按什么顺序拿的资源，拆除时按逆序还回去。

### 1.2 设备号的编解码

设备号是“哪块盘”的紧凑编码：12 位 major（哪类驱动）+ 20 位 minor（该类第几块）挤进 64 位。`major/minor/makedev` 三宏是编码的全部知识——major 取中段右移，minor 取高段与低段拼接，`makedev` 是其逆。三者的往返律（拆了再组回去不变）是设备号正确性的唯一判据。

伪设备是编码的例外：major 为 0（`NONE_MAJOR`）、minor 在 1~16 的号不代表任何硬件，只为无盘 FS（PFS、ramdisk 之后的二挂根）占位。`is_nonedev` 的三元合取（major==0 且 minor 非零且在界内）正是“占位符”的定义——读懂它，就读懂了为什么无盘挂载不需要驱动。

### 1.3 无设备的占位符与位图

16 个伪设备号由一张 16 位图管理：空闲扫描取首个清位，用后置位，卸载清位。位图的索引是 `minor-1`（minor 从 1 起，位从 0 起）——偏置收敛在一处，调用点只见 minor。满位时 `find_free_nonedev` 报 `EMFILE`：文件太多打不开与伪设备不够用是同一种“没槽了”，共用一个错误码恰如其分。

### 1.4 挂载的提交序

提交是五段：查标签问驱动（无盘跳过）→ 查忙（同设备不重挂）占槽（无槽 `ENOMEM`）→ 粘合挂载点（引用必须恰为 1）→ 读超块（失败则拆槽）→ 落定（根与常规分两路）。每段失败都只拆本段及之前拿的——读超块失败不碰挂载点（它还没粘），粘合失败要放挂载点引用。回滚的边界就是拿取的边界，这是提交序设计的全部技巧。

`EBUSY` 在此出现两次，含义不同：查忙的 `EBUSY` 是“这设备已嫁过”，粘合的 `EBUSY` 是“这目录正忙着”。同码不同义——错误码的字面（忙）相同，忙的主体（设备 vs 目录）不同。

### 1.5 卸载的拆除序

拆除是提交的逆序：先查忙（整树引用超 1、有锁、vmnt 有等待，三者任一即 `EBUSY`）→ 清统计（不再列入报表）→ 放挂载点引用 → 限流（`c_max_reqs=1`，不再发新请求）→ 调 FS 执行卸载（失败忽略——人都走了，错也追不回）→ 还伪设备号 → 清根节点 → 释槽 → 块改道回根。`req_unmount` 失败忽略是全文件最务实的一行：不可恢复的错误，打印一句继续走。

PFS 是拆除唯一的例外：它没有根节点可清（`530-535` 的条件）。例外以分支显式，而不是注释一句——分支可测，注释不可。

### 1.6 根的特殊待遇

根可以被挂两次：先 ramdisk（启动即有），再 boot 盘（真根），`have_root` 从 0 数到 2。第三次挂 `/` 就是普通挂载（`have_root<2` 不成立，走粘合路）。换根时全员换家：每个活进程的根目录与工作目录统统换成新根（`MAKEROOT` 宏：放旧、取新、指新）——旧根的引用逐个归还，新根的引用逐个递增，没有进程会被落在旧树上。

`ROOT_DEV/ROOT_FS_E` 的落定（`327-328`）是换根的句号：此后块改道的默认目标就是新根的 FS。卸载任意设备后改道回根（`542`）则是同一机制的反向——树可以来来去去，根永远是改道的终点。

### 1.7 与其他 OS 的挂载对照

- **Linux** 以 `vfsmount`（挂载实例）+ `super_block`（超块）+ `dentry` 嫁接点的三元组实现同构机制：`do_mount` 的查忙占槽对应 `do_add_mount` 的锁与查重，`have_root` 两次对应 `init_mount_tree` 先 rootfs 再真根的两次覆盖，`MAKEROOT` 全员换家对应 `pivot_root` 的进程根迁移语义。
- **Redox** 以 scheme 桶（`scheme.rs` 的命名空间挂载点）扁平化嫁接：路径前缀分发代替目录缝合；VFS 的 `classify_name`（块直取 vs 挂载根回退）对应 Redox 的 scheme 解析（精确匹配 vs 前缀回退）。
- **seL4** 无挂载原语，命名空间由用户态文件服务自行拼接；VFS 的内核侧（服务侧）槽表在 seL4 中对应各服务的私有挂载表——差异的根因仍是状态服务器 vs 无状态内核。

### 1.8 小结

挂载是五段提交（编解码→查忙占槽→粘合→读超块→落定），卸载是七步拆除（查忙→清统计→放引用→限流→调执行→归还→改道），根是两次特例加全员换家。三组开关（设备号/槽位/引用数）决定成败，一条不变量贯穿始终：拿什么，回什么——PFS 的无根例外是唯一的分支。

---

## 2 C 源码分析

### 2.1 设备编解码三宏（`types.h:290-295`）

`major(x) = (x & 0xfff00) >> 8` 取 12 位，`minor(x)` 拼高 20 位段与低 8 位，`makedev` 为其逆。定义见 `minix3/sys/sys/types.h:290-295`。伪设备以 `NONE_MAJOR 0`（见 `minix3/minix/include/minix/dmap.h:21`）为 major，minor 从 1 起（`dmap.h:17-18` 的 `NO_DEV` 对照注释）。

### 2.2 `nonedev` 位图机（`mount.c:33-37,641-653`）

`bitchunk_t nonedev[BITMAP_CHUNKS(NR_NONEDEVS)]` 全零（`34`，`NR_NONEDEVS=NR_MNTS=16` 见 `minix3/minix/servers/vfs/const.h:7-12`），`alloc/free_nonedev` 即 `minor-1` 位的置/清（`36-37`）。`find_free_nonedev` 首清位分配（`648-649`，`makedev(NONE_MAJOR, i+1)`），满位设 `EMFILE` 返 `NO_DEV`（`651-652`）。

### 2.3 `is_nonedev` 判定（`mount.c:628-635`）

`major == NONE_MAJOR && minor > 0 && minor <= NR_NONEDEVS` 三元合取——零号 minor 排除在外（`NO_DEV` 本身 major/minor 全零，不可误判为伪设备）。

### 2.4 `update_bspec` 路由改道（`mount.c:46-80`）

扫全 vnode 表（`56`）：引用有效且块类型且设备命中即改 `v_bfs_e`（`57-58`）。投递驱动标签时越界 major 跳过（`61-64`）、消失驱动跳过（`66-70`，打印一句）、投递失败打印继续（`72-77`）——三处“跳过并继续”都是“改道尽力而为”的同一语义：个别块节点改不动，不阻断整树挂载。

### 2.5 `do_mount` 入口机（`mount.c:85-150`）

超管门（`108`）→ 标签长度守门（`111-112`）→ 拷贝标签并 DS 查端点（`113-120`）→ 端点合法性（`123`）→ 空设备名走伪设备（`126,133-138`，名填 `"none"`）否则取块名解设备（`127-132`，`name_to_dev(FALSE)`）→ 取挂载路径（`141`）→ 类型长度守门（`144`，`ENAMETOOLONG`）→ 取类型 → `mount_fs`（`148-149`）。

### 2.6 `mount_fs` 提交机（`mount.c:156-385`）

驱动标签（`176-188`：伪设备跳过，驱动缺席 `EINVAL`，空标签断言）→ 查忙（`191-196`，重挂 `EBUSY`）占槽（`197-200`，无槽 `ENOMEM`，加 `VMNT_EXCL`）→ 记路径与类型（`202-204`）→ 根判定（`205-209`）→ 非根粘合（`211-238`：`eat_path` 取挂载点，引用恰 1 才 `req_mountpoint`，余 `EBUSY`；提前解锁父 vmnt 以容 FUSE 回调，`224-228`）→ 取根 vnode（`241-248`）→ 端点复核（`252-260`，坏端点 `EINVAL`）→ 标服务进程（`261-262`，`FP_SRV_PROC`）→ 存端点设备与只读位（`265-268`：`MNT_RDONLY` 置 `VMNT_READONLY`，见 `minix3/minix/servers/vfs/vmnt.h:24`）→ 置 `MOUNTING` 读超块清标志（`271-274`）→ 存 FS 标志（`276`）→ 刷 statvfs 缓存（`279-280`）→ 失败拆槽（`282-291`）→ 加 `bsf`（`293`）→ 填根节点九字段（`296-304`）→ 绑挂载（`307-308`）→ 线程数（`309-313`：`RES_THREADED` 见 `minix3/minix/include/minix/vfsif.h:21`，线程 FS 用 `NR_WTHREADS=9`，余 1）→ 置 `CANSTAT`（`316`）→ 根提交（`318-349`：落槽、双标签、无盘占位、改道不投递、落 `ROOT_*`、全员 `MAKEROOT`、`have_root++`）或常规提交（`352-384`：类型冲突 `EISDIR`、落槽、无盘占位、非强制改道、解锁）。

### 2.7 `mount_pfs` 管道服机（`mount.c:391-425`）

取伪设备（`404-405`，无则 panic——启动不变式）→ 取槽（`407-408`，无则 panic）→ 占位（`410`）→ 填槽五字段（`412-417`：设备、端点 `PFS_PROC_NR`、标志清零、`"pfs"`/`"pipe"`/`"none"` 三标签）→ `req_readsuper` 握手（`420`，失败只打印——PFS 缺席不阻断启动）。

### 2.8 `do_umount/unmount` 拆除机（`mount.c:430-546`）

`do_umount`：超管门（`447`）→ 取名解设备（`450-453`，`allow_mountpt=TRUE`）→ `unmount`（`455`）→ 标签 overlong 截断（`460-461`）→ 回传标签（`462-463`，供调用者停服）。`unmount`：按设备定位槽（`480-485`，双挂 panic——挂载门保证不可能），无槽 `EINVAL`（`488`），加 `EXCL`（`490`）→ 忙检查（`494-504`：引用和>1、有锁超 1、vmnt 有等待，三者任一 `EBUSY`）→ 清统计（`507`）→ 清根引用（`510`）→ 放挂载点（`512-515`）→ 限流 1（`517-519`）→ 调 FS 卸载（`522-524`，失败打印忽略）→ 还伪设备号（`526`）→ 回传标签（`528`）→ 清根节点（`530-535`，PFS 跳过）→ 释槽（`536`）→ 解锁（`538`）→ 加 `bsf` 改道回根并投递（`541-543`）。

### 2.9 `unmount_all` 扫荡机（`mount.c:552-585`）

`NR_MNTS` 轮 × 全表扫荡（`563-567`：每轮至少卸一个，嵌套挂载自外向内剥落）→ 非 force 直返（`570`）→ 三锁断言（`573-576`：vnode/vmnt/filp/bsf，04/06/07 管辖）→ 残留 panic（`580-584`）。

### 2.10 `name_to_dev` 名字机（`mount.c:590-622`）

解析（`602-607`，`VMNT_READ/VNODE_READ`）→ 块直取 `v_sdev`（`609-610`）→ 许可且为挂载根则回 `v_dev`（`611-612`）→ 余 `ENOTBLK`（`613-616`）→ 解锁归还返回（`618-621`）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `mount.c` 的直线代码，而是吸收 Linux/Redox 的挂载模型后做取舍。以下决策对应 `.design/18-design.v1.md` D1-D7。

### D1 设备编解码纯函数

- **C**：`major/minor/makedev` 三宏（`types.h:290-295`）。
- **Rust**：`DevCodec::{major, minor, make, is_nonedev}` 纯函数（`os/servers/vfs/src/mount.rs:37`）。
- **为什么**：宏不可测往返律；函数使 `make→major/minor` 恒等一测即知。替代方案（宏直译 `macro_rules!`）被否决：编解码是函数语义，用宏只是把 C 的拼写带进 Rust。

### D2 伪设备位图新型

- **C**：`bitchunk_t[]` + 置位宏 + `minor-1` 散落调用点（`mount.c:34-37`）。
- **Rust**：`NonedevBitmap(u16)` + `alloc/free/contains/find_free` + `alloc_nonedev/free_nonedev`（`os/servers/vfs/src/mount.rs:71,122`）；满位 `EMFILE`。
- **为什么**：16 位定长使“满”可表达（全 1 即满）；偏置收敛进类型，调用点只见 minor。`u16` 的位宽选择即文档：`NR_NONEDEVS=16`，不多一位。

### D3 提交分阶段与超块 trait

- **C**：`mount_fs` 230 行直线 + 五处回滚（`mount.c:195-365`）。
- **Rust**：`SuperblockReader` trait（`MemSuperblock` 常成功 vs `FailSuperblock` 常 `EIO`）+ `MountPhase` 五值 + `check_dev_free/glue_decision/type_clash/thread_allowance` 四纯判定（`os/servers/vfs/src/mount.rs:152,188,203`）。
- **为什么**：FS 往返是唯一的不可测点，trait 隔离后提交序可单测。`MNT_RDONLY` 以 `readonly: bool` 入参——同步树内无该宏定义可验证，不编造位值（诚实标注，见 design D3）。

### D4 粘合与忙判定纯谓词

- **C**：`ref==1` 粘合（`mount.c:218`）与三元忙检查（`501`）分处两函数。
- **Rust**：`glue_decision(ref_count)` + `busy_check(refs, locks, pending)`（`os/servers/vfs/src/mount.rs:212,389`）。
- **为什么**：同一“独占”知识的两面；边界值（1 vs 2）各一测。`pending` 把 07 的“等待即锁定”语义显式为参数。

### D5 根状态机

- **C**：`have_root` 0/1/2 裸计数（`mount.c:31`）。
- **Rust**：`RootStage::{Absent, Ramdisk, Bootdisk}` + `mount_takes_root_path + advance`（`os/servers/vfs/src/mount.rs:243`）。
- **为什么**：裸计数的“2”无名；枚举使 ramdisk→boot disk 的两次可读，饱和语义保留（第三次自动转普通挂载）。`MAKEROOT` 全员换家留调用点（proc 表迭代归 03）。

### D6 拆除计划

- **C**：七步直线 + PFS 例外分支（`mount.c:506-545`）。
- **Rust**：`UnmountPlan{free_nonedev, clear_root, send_driver}` + `plan_unmount`（`os/servers/vfs/src/mount.rs:398,408`）。
- **为什么**：顺序知识结构化后 PFS 例外可测；`req_unmount` 失败忽略留调用点注释（不可恢复，打印继续）。

### D7 扫荡验证类型化

- **C**：残留 panic（`mount.c:581-584`）。
- **Rust**：`verify_empty(&[bool])` + `sweep_passes`（`os/servers/vfs/src/mount.rs:420,431`）。
- **为什么**：panic→错误是 ARCH 加固——关机路径崩溃即最坏情况；三锁断言归 04/06/07，此处只判残留。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-4 全局聚合（glo.h→状态聚合） | `have_root`→`RootStage`，`nonedev`→`NonedevBitmap` | `mount.rs:71,243` + 本文档 D2/D5 + 18 正文 §1.3/§1.6 |
| 不可达 panic→错误（启动/关机加固） | `PFS` 无槽、`verify_empty` 残留 | `mount.rs:122,420` + 本文档 D3/D7 + 18 正文 §2.7/§2.9 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── mount.rs              — 本篇：编解码/位图/提交判定/拆除计划
├── vmnt.rs               — VmntTable 槽存储（06，不重复实现）
├── path.rs               — eat_path 执行（13）
└── request.rs            — req_* 执行（12）
```

> 设计决策：§3 D1（编解码纯函数）/ D2（位图新型）/ D5（根状态机）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `major/minor/makedev` | `types.h:290-295` | `mount.rs:37 DevCodec` | 位布局纯函数 |
| `is_nonedev` | `mount.c:628` | `mount.rs:59 is_nonedev` | 三元合取 |
| `nonedev` 位图 | `mount.c:33-37` | `mount.rs:71 NonedevBitmap` | `u16` 位集 |
| `find_free_nonedev` | `mount.c:641` | `mount.rs:104,122` | 首清位 + EMFILE |
| 超块读取 | `mount.c:272` | `mount.rs:152 SuperblockReader` | Mem/Fail 双实现 |
| 提交阶段 | `mount.c:156-385` | `mount.rs:188 MountPhase` | 五值 |
| 查忙/粘合/冲突/线程 | `mount.c:191,218,352,309` | `mount.rs:203,212,222,230` | 四纯判定 |
| `have_root` | `mount.c:31` | `mount.rs:243 RootStage` | 三态饱和 |
| 改道判定 | `mount.c:46-80` | `mount.rs:271,287 bspec_update` | 四值表 |
| 入口守门 | `mount.c:107-144` | `mount.rs:303,316` | 门 + 来源二值 |
| 名字分类 | `mount.c:590-622` | `mount.rs:342,355 classify_name` | 三分类 |
| 忙检查 | `mount.c:492-504` | `mount.rs:389 busy_check` | 三元合取 |
| 拆除计划 | `mount.c:506-545` | `mount.rs:398,408` | 计划三位 |
| 扫荡验证 | `mount.c:552-585` | `mount.rs:420,431` | 残留判定 |
| 错误族 | `mount.c` 全文件 | `mount.rs:439,462 MountError::to_errno` | 9 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 槽位独占 | `check_dev_free` | 重挂 EBUSY | `mount.c:191-196` |
| 位图配对 | `free_nonedev` + 拆除计划 | 分配必有归还 | `mount.c:373,526` |
| 根至多两次 | `RootStage` 饱和 | 第三次转普通 | `mount.c:206-209` |
| 拆除忙查先行 | `busy_check` 门 | 三元任一即忙 | `mount.c:501-504` |
| 编解码往返 | `make→major/minor` | 恒等测试 | `types.h:290-295` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **238 passed / 0 failed**（既有 228 + 本篇新增 10；`minix-types` 独立）。
> 本章直接影响 10 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_dev_codec_roundtrip` | `types.h:290-295` + `mount.c:628-635` | 往返恒等 + 伪设备判定 | `mount.rs:482` |
| `test_nonedev_pool_pairing` | `mount.c:33-37,641-653` | 全分配 + EMFILE + 释放复用 | `mount.rs:502` |
| `test_superblock_factories_differ` | design D3 + `mount.c:309-313` | 双工厂分化 + 线程数 | `mount.rs:530` |
| `test_commit_guards` | `mount.c:191-222,352-353` | 查忙 + 粘合 + 类型冲突 | `mount.rs:548` |
| `test_root_stage_machine` | `mount.c:31,206-209` | 两次根 + 饱和 | `mount.rs:566` |
| `test_bspec_routing_table` | `mount.c:61-77` | 跳过表 + 投递表 | `mount.rs:579` |
| `test_mount_head_gate` | `mount.c:107-144` | 超管门 + 长度门 + 来源二值 | `mount.rs:590` |
| `test_name_classification` | `mount.c:607-616` | 三分类 + 查找失败 | `mount.rs:619` |
| `test_teardown_plan` | `mount.c:492-535,552-585` | 忙三元 + 计划三位 + 扫荡 | `mount.rs:641` |
| `test_errno_map_covers_mount_c` | `mount.c` 全文件 | 9 变体→errno 全映射 | `mount.rs:670` |

测试策略：编解码以往返恒等覆盖；位图以全分配/满/释放复用覆盖；提交以边界值（1 vs 2）覆盖；根以状态机全转移覆盖；改道以跳过/投递表覆盖；拆除以忙三元 + 计划三位 + PFS 例外覆盖；错误以 9 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**238 passed / 0 failed**
- 本节列出与本模块直接相关的 10 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/mount.rs`

---

## 6 过渡

本篇在 01（启动根挂载）之后、19（设备表）之前，是“树”的装配层：01 只管把根缝上，本篇管以后的一切嫁接与拆除；没有本篇的槽位判定，19 的设备映射不知为谁服务，20 的块改道不知默认去哪。

```
01-vfs-init-main: do_init_root + mount_pfs（根与管子先行）
   │
   └─► 本篇：编解码 → 查忙占槽 → 粘合 → 读超块 → 落定 / 查忙 → 拆除 → 改道回根
          │                                              │
          ├─► 19-device-map：dmap 驱动标签的查询与映射（提交第 2 段的标签来源）
          ├─► 20-bdev：块改道的默认目标 ROOT_FS_E（落定与拆除的终点）
          └─► 12-request-wrappers：req_readsuper/mountpoint/unmount 的协议执行
```

阅读顺序提示：若关心“标签从哪来”，下一站 `19-device-map.md`（`dmap` 的驱动映射）；若关心“改道之后块请求走哪”，下一站 `20-bdev.md`（`bdev_sendrec` 与 `bdev_reply`）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/mount.c:1-653`（`update_bspec/do_mount/mount_fs/mount_pfs/do_umount/unmount/unmount_all/name_to_dev/is_nonedev/find_free_nonedev`）、`minix3/sys/sys/types.h:290-295`（`major/minor/makedev`）、`minix3/minix/servers/vfs/const.h:7-12`（`NR_MNTS/NR_NONEDEVS`）、`minix3/minix/servers/vfs/vmnt.h:24-33`（`VMNT_*` 标志与锁映射）、`minix3/minix/include/minix/com.h:68`（`PFS_PROC_NR`）、`minix3/minix/include/minix/vfsif.h:21`（`RES_THREADED`）、`minix3/minix/include/minix/dmap.h:21`（`NONE_MAJOR`）
- 阶段文档：`06-vmnt-table.md`（槽存储）、`01-vfs-init-main.md`（启动根挂载）、`13-path-lookup.md`（挂载点解析）、`12-request-wrappers.md`（FS 协议执行）、`19-device-map.md`（驱动标签）、`20-bdev.md`（块改道终点）
- Rust 实现：`os/servers/vfs/src/mount.rs:1`（本篇判定层）、`os/servers/vfs/src/vmnt.rs:1`（槽存储，06）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（标签与路径拷贝语义）
