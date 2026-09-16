# 15-stage-fs Rust 实现架构级 Review TODO（V1 轮）

> **来源**：V1 轮架构级审查（2026-09-16，code-excellence 口径：查漏补缺优先，其次整体/分层架构深审；叠加 cmd-20 四步：找遗漏、找改进、补覆盖、回归）。
> **范围**：26 篇文档 `Rust 模块` 头声明的全部实现——`os/fs/`（8 crate）+ `os/libs/minix-fs` + `os/libs/minix-vtreefs` + `os/libs/minix-sffs`，共 20,876 行 Rust、289 个测试。C ground truth 为 `minix3/minix/fs/`（8 server）、`minix3/minix/lib/{libfsdriver,libminixfs,libvtreefs,libsffs}`、`minix3/minix/include/minix/{fsdriver.h,vfsif.h}`。
> **定位**：本文件只登记发现与修复方向，不复写 plan.md；跨 stage 条目的唯一入口是 `../edge_todo.md`（本文件第 5 节只留指针）。本轮未修改任何生产代码；正确性缺陷只登记，修复走后续 todo-fix 单线程轮。
> **基线（2026-09-16，HEAD abccac10）**：`cargo test` 11 个包 289 passed / 0 failed；`cargo clippy` 约 15 条轻量告警（明细见第 7 节）；`tools/design-coverage-check.sh fork-syscall-rewrite/15-stage-fs` 输出 ALL DOCS COMPLETE（24 组设计快照齐备）。

## 0. 审查结论速览

| 编号 | 级别 | 一句话 | 状态 |
|---|---|---|---|
| V1-P0-1 | P0 | mfs 服务装配断层：31 个分发行全部不可达（无 `impl FsDriver`、无事件循环） | ✅ 2026-09-17 闭环（之 1 循环 + 之 2 装配/冒烟 + P1-8 账本翻转） |
| V1-P0-2 | P0 | rmdir 链接计数与 C 行为不符：子目录 nlinks 终值 1、父目录未递减 | ✅ 2026-09-17 Fix #5（子目录饱和减二、父目录减一，测试断言落账） |
| V1-P1-1 | P1 | `fs_rename` 整体缺失（link.c:255-422 的完整决策树无对应） | ✅ 2026-09-17 Fix #8（决策树全实现 + MfsServer 接线 + 账本 30/1） |
| V1-P1-2 | P1 | `fs_putnode` 缺失：count-1 批量递减语义不存在 | ✅ 2026-09-17 Fix #3（表级 put_count + 装配接线 + 账本 29/2） |
| V1-P1-3 | P1 | Peek 路径缺失：read.c:156-159 的 `FSC_PEEK` 分支无对应 | ✅ 2026-09-17 Fix #9（方案修正：C mfs 有 peek 入口，实现为读路径直通 + HAS_PEEK） |
| V1-P1-4 | P1 | `ReclaimZones` 无执行者：unlink 最后一链后数据区实际不回收 | ✅ 2026-09-17 Fix #4（执行体挂 unlink/remove_dir/put_node 三出口，卷视图可观察验证） |
| V1-P1-5 | P1 | 目录块镜像与缓存之间没有装载与写回的桥接，目录操作落不到磁盘 | ✅ 2026-09-16 Fix #1（先行落地：P0-1 装配的依赖） |
| V1-P1-6 | P1 | 绝对符号链接的 offset 语义与 C 分歧（C 报 0，Rust 报旧路径组件起点） | ✅ 2026-09-17 Fix #6（归零 + 跨组件钉住测试 + 03 篇对齐说明更新） |
| V1-P1-7 | P1 | mount 适配层丢失驱动标签：`adapt_mount` 向 `bound_driver` 传空字符串 | ✅ 2026-09-17 Fix #7（MountInput/请求体携 label，spy 测试锁定） |
| V1-P1-8 | P1 | 完备性账本滞后：23 行 pending 中 20 行的实现已经存在 | ✅ 2026-09-17 Fix #2（随装配翻转，钉住测试 28/3） |
| V1-P1-9 | P1 | vtreefs 与 sffs 脱离框架（只依赖 minix-types），与文档 18 的前置声明及 C 的依赖结构不一致 | 开口项 |
| V1-P1-10 | P1 | `PATH_GET_UCRED` 的凭据 grant 校验无处实现 | ✅ 2026-09-17 Fix #10（声明式落账：值构造前校验归传输解码，结构文档锚定） |
| V1-P1-11 | P1 | procfs 内容面缺口：cmdline、environ、cpuinfo、pci、ipcvecs、service 目录 | 开口项 |
| V1-P2-1 ~ V1-P2-11 | P2 | 写路径免读优化、缓存拷贝清单、getdents 冷目录、预读策略、stat 无类型契约、双语义返回、三份重复实现、块层缺口、写盘只读护栏、ext2/isofs/sffs 覆盖面、测试缺口 | 开口项 |
| V1-P3-1 ~ V1-P3-8 | P3 | 死语句、unused import、位图起点差异、errno 边缘顺序、短末块判定、消费方画像、无调用方的预滤函数、间接块校验粒度 | 开口项 |
| edge×4 | — | E-FSRUNTIME / E-FSBDEV / E-FSVMCACHE / E-FSCMDS（见第 5 节） | 已登记 |

## 1. Step 0 预检与 Gate 证据

- **Step 0 硬阻断预检**：`notes/rewrite/fork-syscall-rewrite/15-stage-fs/.design/` 下 24 组 `NN-outline.v1.md` + `NN-outline-review.v1.md` + `NN-design.v1.md` 齐备（各 24 个）；`bash tools/design-coverage-check.sh fork-syscall-rewrite/15-stage-fs` 输出 `ALL DOCS COMPLETE`，退出码 0。Gate H.6 / H.1 通过。
- **基线**：`cargo test -p minix-fs -p minix-fs-mfs -p minix-fs-pfs -p minix-fs-procfs -p minix-fs-ptyfs -p minix-fs-ext2 -p minix-fs-isofs -p minix-fs-vbfs -p minix-fs-hgfs -p minix-vtreefs -p minix-sffs` 全绿，合计 289 passed（明细：mfs 98、minix-fs 90、ext2 25、procfs 17、pfs 12、sffs 12、ptyfs 11、vtreefs 11、isofs 9、vbfs 2、hgfs 2）。scope 内 `grep -rn "unsafe "` 命中 0。
- **Gate D（trait 实现普查）**：`FsDriver` 有 2 个生产行为不同的实现（`PfsServer`，os/fs/pfs/src/lib.rs:274；`MemFileServer`，os/libs/minix-fs/src/memfs.rs:170），通过。`BlockSource`、`SecondLevelCache`、`DeviceInfo`、`DataBackend` 各只有 1 个生产实现（其余为测试替身）：第二实现分别归属块设备轨道与 VM 轨道（bio.rs:12-16、cache.rs:15-22 有书面归属），按模式 80 记 P2 观察（第 3.3 节 V1-P2-8 与 edge 条目），不判死代码。
- **translate 防线（模式 16/17/65）**：分发用 enum + match（call_table 的 C 函数指针表在 os/libs/minix-fs/src/driver.rs:8-9 有书面替换声明）、错误走 `Errno` + 每模块错误 enum、无 `static mut`、scope 内 unsafe 为 0。判定通过。
- **时点声明**：全部证据采集于 HEAD abccac10；期间并行线程仅在 `../edge_todo.md` 追加内容（859 行 → 860 行），未触碰本范围文件。

## 2. 覆盖对账矩阵（查漏主体）

### 2.1 mfs（os/fs/mfs ↔ minix3/minix/fs/mfs，17 个 C 文件）

函数级对账结论：C 的 31 个入口中，26 个已有 Rust 实现函数（多数达到「决策点与副作用顺序可对齐」的完全对应），3 个整体缺失，2 个部分成形。逐文件要点：

| C 文件 | Rust 对应 | 结论 |
|---|---|---|
| link.c（638 行） | link.rs（1156 行） | link/unlink/rdlink/truncate 决策完全对应；`fs_rename` 缺失；rmdir 的 nlinks 记账与 C 不符（V1-P0-2） |
| read.c（556 行） | read.rs（976 行） | 读半、映射、目录枚举对应；Peek 分支缺失；预读策略差异（V1-P2-4）；getdents 冷目录与类型解析退化（V1-P2-3） |
| write.c（319 行） | write.rs（963 行） | write_map/wr_indir/empty_indir/new_block/zero_block 完全对应；整块免读优化缺失（V1-P2-1） |
| inode.c（464 行） | inode.rs（1039 行） | 缓存/哈希/分配/回收完全对应；`fs_putnode` 缺失；无主 inode 的清算只上报不执行（V1-P1-4） |
| super.c（365 行） | superblock.rs（632 行） | 位图与超级块校验链完全对应；两处刻意偏离见 V1-P3-3 与第 3.4 节 |
| cache.c（109 行） | mfs_cache.rs（268 行） | get_block/alloc_zone/free_zone 完全对应；缓存本体在框架层（2.2 节） |
| mount.c / stadir.c / protect.c / time.c / utility.c / stats.c | mount.rs / meta.rs | mount 生命周期、stat/statvfs/chmod/chown/utime、conv2/conv4 完全对应；stats.c 的空闲位统计有三份重复实现（V1-P2-7） |
| path.c（241 行） | dir.rs（732 行） | advance/search_dir（四模式）完全对应；`fs_lookup` 入口函数未收口（构件齐备） |
| open.c（270 行） | open.rs（910 行） | create/mkdir/mknod/slink/new_node/seek 完全对应 |
| main.c（102 行） | startup.rs（159 行）+ main.rs（10 行） | init 三件套完成两件；信号只移植纯决策；分发主循环缺失（V1-P0-1） |
| misc.c / table.c | maint.rs / table.rs | fs_sync 的「先 inode 后块」契约一致；表结构 31 行一一对应但翻转状态失真（V1-P1-8） |

账本核查两张清单（table.rs:40，`EntryStatus` 三态）：

- **账本滞后（账本写 pending、实现已存在，20 行）**：Read/Write（read.rs:174、write.rs:331）、GetDents（read.rs:334）、Truncate（link.rs:665 + write.rs:417）、InhibitRead（open.rs:555）、Create/MakeDir/MakeNode（open.rs:244/295/265）、Link/Unlink（link.rs:159/278）、RemoveDir（link.rs:317，含 V1-P0-2 缺陷）、SymbolicLink（open.rs:412）、ReadLink（link.rs:476）、Stat/ChangeOwner/ChangeMode/UpdateTimes/StatVfs（meta.rs:226/122/93/152/325）、Sync（maint.rs:64）、Lookup（部分：构件齐备无入口）。
- **真 pending（实现确实缺失，3 行）**：PutNode（inode.c:38-64）、Peek（read.c:156-159）、Rename（link.c:255-422）。

### 2.2 minix-fs 框架（↔ libfsdriver + libminixfs）

- **协议面零差集**：vfsif.h:41-73 的 33 个 REQ_* 与 protocol.rs:66-137 的 33 个变体逐一对应，编号经 33 对编译期对账测试钉住（protocol.rs:553-601）；FS_BASE/NREQS 从 minix-types 单点权威 re-export（E-REQWIRE 收敛成果）。flags 全部对上（REQ_RDONLY/ISROOT、PATH_RET_SYMLINK/GET_UCRED、RES_*、EENTERMOUNT/-301 族）。
- **call.c 31 个适配函数**：全部有对应（call.rs），三类「空回调默认值」保真（成功类 7 处、ENOSYS 类 25 处、peek 条件类）。差异三处：mount 的驱动标签丢失（V1-P1-7）、unmount 不清二级缓存（归 VM 轨道，edge）、getdents 的验证顺序在「回调缺失 + 参数非法」重叠时 errno 不同（V1-P3-4）。
- **libminixfs 块层缺口**（C 有 Rust 无，按归属分两类）：归属本 stage 的有 ONE_SHOT 块的 LRU 队首插入（cache.c:533-544）、`lmfs_prefetch` 的位图选连续区间（cache.c:1089-1130）、脏块按块号排序批量写（cache.c:884-885）、运行期池调整（lmfs_set_blocksize/cache_resize）、`lmfs_change_blockusage` 的重估触发（cache.c:119-161）；归属轨道协作的有短末块部分读写（lmfs_get_partial_block）、gather/scatter 聚散 I/O（rw_scattered + bdev_gather，归 16-stage）、vmcache 零拷贝页移交与旗标机（cache.c:443-451 等，归 VM 轨道）。全部记录于 V1-P2-8 与 edge 条目。
- **lookup.c → lookup.rs**：主解析循环完全对应（mountpoint 起步、root 自环、出逃 offset、EENTERMOUNT 携 inode、符号链接 7 次上限、putnode 纪律）。两处分歧：绝对符号链接 offset（V1-P1-6）、`PATH_GET_UCRED` 凭据校验缺失（V1-P1-10）。
- **框架上半层零生产消费方**：call.rs 适配器、driver.rs 的 classify/dispatch、lookup.rs 的 resolve_path、bio.rs 的 bio_transfer 在 os/ 树内没有任何生产调用方——这是 V1-P0-1 的框架侧镜像。

### 2.3 pfs / procfs / ptyfs

- **pfs**（C 451 行单文件 ↔ Rust lib.rs 690 行）：唯一接入 `FsDriver` 的真实 server（lib.rs:274），mount/unmount/new_node/put_node/read 等带 C 锚点；`main.rs` 构建 server 后 `loop {}` 停放，等运行时轨道。本组无新增开口项。
- **procfs**（C 1703 行 8 文件 ↔ Rust 802 行）：buf/pid 槽位算术/两遍刷新/uptime/meminfo/loadavg/kinfo/mount/dmap/psinfo 已对应。缺口：pid_cmdline 与 pid_environ（pid.c）、root_cpuinfo 与 cpuinfo.c、root_pci、root_ipcvecs、service.c 的 RS 对话族（service_active/get_flags/get_policies）——登记为 V1-P1-11。
- **ptyfs**（C 518 行 2 文件 ↔ Rust 735 行）：查找/枚举/状态/控制/名字双向转换已对应；ptyfs_signal 与 ptyfs_other 属事件循环层（随 V1-P0-1 类装配问题走 edge 轨道）。本组无新增开口项。

### 2.4 ext2 / isofs / vbfs / hgfs / sffs / vtreefs

- **ext2**（C 4705 行 17 文件 ↔ Rust 1202 行）：已实现的是磁盘格式解析层（超级块几何与 feature 门、组分配策略含 orlov 三变体与窗口、目录项编解码、三级间接分解）。未实现的语义块：全部 fs_* 适配入口、rw_chunk/rahead 数据路径、块与 inode 的分配执行（balloc/ialloc）、link.c 的名字空间操作、truncate、 protect/time/stadir。这与文档分期一致（21/22 篇只声称覆盖 init-mount 与 namespace-data），登记为 V1-P2-10 的对账行，不判缺陷。
- **isofs**（C 1509 行 12 文件 ↔ Rust 630 行）：卷发现、记录解析、Rock Ridge 的 SL 组装已实现；fs_lookup/search_dir、fs_stat（date7 转换）、fs_getdents 分发、inode 缓存、挂载生命周期未实现。同上，归 V1-P2-10。
- **vbfs/hgfs + minix-sffs**（C 合计 2202 行 ↔ Rust 合计 700 行）：sffs 已实现名字归一化/路径栈/装配参数/部分 verify 语义；C libsffs 的 handle 管理、inode 与 lookup 核心、do_* 全套分发、read/write、stat、主循环未实现。归 V1-P2-10。
- **minix-vtreefs**（C libvtreefs 1507 行 ↔ Rust tree.rs 1073 行）：树存储、哈希、点项、钩子、释放与清理完整；但 crate 不依赖 minix-fs、没有 fs_* 适配层——C 的 libvtreefs 是构建在 libfsdriver 之上的（vtreefs.c 的 fs_* 函数族），文档 18 第 6 行也把 01 篇列为前置。登记 V1-P1-9。

### 2.5 bin / startup 接线现状

8 个 server 的 bin 全部未接事件循环：mfs 是 `fn main() { loop {} }`（os/fs/mfs/src/main.rs:8-10，注释声明归服务运行时轨道）；ext2/isofs/vbfs/hgfs/procfs/ptyfs 的 main.rs 第 4-6 行是同义 TODO 注释；pfs 构建 server 后停放（os/fs/pfs/src/main.rs:8-10）。C 对应物是 main.c 的 `env_setargs` + `sef_local_startup` + `fsdriver_task(&table)` 三段。其中框架侧事件循环（`fsdriver_task` 的对应物）属本 stage（V1-P0-1），进程启动握手属跨 stage 条目 E-FSRUNTIME。

### 2.6 DEFERRED / stub 记账复核

scope 内 39 处标记复核完毕：无一处 `todo!`/`unimplemented!`/`FIXME`；`#[allow(dead_code)]` 为 0。vfs 的 DEFERRED 群归 05-stage-vfs 轨道不重复登记；本范围内的标记分三类：(a) 有归属轨道的（bio.rs:12-16 归块设备、cache.rs:15-22 归 VM、mount.rs:8-9 的 bdev_open/close、dir.rs:10-14 的镜像桥接、startup.rs:88-94 的 sync 序列）；(b) 描述性 stub（fs_comm.rs、call.rs:730 的 diskless 注释属 vfs 轨道）；(c) 应翻账而未翻的（2.1 节 20 行滞后）。(a) 类全部已挂 edge 或开口项，无游离 DEFERRED。

## 3. 条目正文

每条给出锚点、问题、至少两个候选方案与对照（Redox / Linux / OS 理论）、推荐与 [ARCH] 判定。本轮不实施；实施时按 fix-guard（修前读目标行 ±5 行、grep 确认、一次一条、修后重放验证）执行。

### 3.1 P0

**V1-P0-1 mfs 服务装配断层：31 个分发行全部不可达。**
事实：mfs crate 没有任何 `impl FsDriver`（全仓 grep 仅命中 pfs 与框架自身）；入口是 `loop {}`（main.rs:8-10）；`ServerCore`（startup.rs:49-68）持有缓存与配置但不持有 inode 表，`InodeTable::new`（inode.rs:300）无人组装；框架上半层（call.rs 适配器、driver.rs:472-563 分类与分发）零生产消费方。效果是 table.rs 标记 Live 的 8 行同样不可达，289 个测试全部测的是库函数而非服务器行为。
方案 A：新建 `MfsServer` 结构聚合 `ServerCore` + `InodeTable` + `Superblock`，实现 `FsDriver` 的 26 个已有能力（真 pending 三行走默认 ENOSYS），框架补一个 `run(server, transport)` 事件循环，用 `RamDisk` + 测试镜像写装配冒烟测试（mount → lookup → read → write → sync 全链）。对照 Redox：redoxfs 在同一 crate 内直接实现 `Scheme` trait 并自跑事件循环，没有「框架半成品 + 实现半成品」两截；我们保留 C 的框架/server 两截结构（8 个 server 共享框架，有正当性），但必须让两截尽早相遇。
方案 B：只补文档声明「mfs 当前是库不是服务器」，把装配推迟到运行时轨道。否决理由：装配 mismatches（签名不契合、生命周期错位）只有写装配代码才会暴露，推迟会把 V1-P1-4/5/6 这类缺口全部拖到联调期才炸。
推荐：方案 A，并把 table.rs 翻转纳入同一修复（V1-P1-8）。[ARCH] 判定：属于实现补全（Rewrite 边界内），不改外部行为，无需三处标注；若装配中发现必须改 `FsDriver` 签名，则升级为 Architectural Evolution，按规范三处一致标注。
**🔄 进度（2026-09-16，之 1 落地）**：框架任务循环 `os/libs/minix-fs/src/task.rs` 提交——`FsTransport` 接缝（receive/reply/copy_in/copy_out/scratch 五方法）、`RequestBody` 三十二变体类型化请求体（名字按 `fsdriver_getname` 语义拷贝为自有缓冲）、`FsReply` 状态+事务号+载荷（Transfer/Node/Lookup）、`Incoming::Cancelled` 对应 `fsdriver_terminate` 的取消接收语义、`Server` 增 peek/后端设备三构造期旋钮（对应 C 表 `fdr_peek`/`fdr_bpeek` 与设备全局）。六项循环测试全绿（crate 96 个），clippy 零告警；01 篇同步 §4.3/§5.3。余：`MfsServer` 装配 + `impl FsDriver` + 冒烟链。
**✅ 之 2 部分落地（2026-09-17）**：装配地基与服务器主体提交——(a) 位图随挂载装载、同步回写，`unmount` 的 sync 闭包改为接收 `&mut MountedFs` 并**归还源设备**（重挂载语义，C 进程跨卸载保设备句柄）；(b) `Parts` 拆借用透镜 + `MfsServer`（持 `Option<S>` 源、`MountedFs`、池配置、注入时钟）+ `impl FsDriver`：mount/unmounted/is_mount_point/read/write/get_dents/truncate（整文件与打洞两臂）/sought/lookup_child/create/stat（本地六十四字节布局，P2-5 收敛）/synchronized/flushed 全部接线，`sync_mounted` 按「先 inode、再位图、后冲刷」次序；(c) 冒烟测试全链通过（挂载→查找→创建→写→读→枚举→状态→同步→卸载→设备交还→重挂载后文件仍在），crate 108 测试全绿。**裁决落定**：池随挂载建立、容量来自启动配置（与 C 的池生命周期差异不可观察）；能力位空（C 框架只补窥视位、mfs 从不设 `RES_64BIT`，grep 证据 `call.c:48-51`）。**余下未接线方法（走 trait 默认 ENOSYS，账实相符）**：make_dir/make_node/link/unlink/remove_dir/symbolic_link/read_link（LinkCtx 族，之 3）、change_owner/change_mode/update_times/stat_vfs（meta 族，之 3）、block_read/block_write/peek（bio_transfer 接线，之 3）、rename（P1-1）。**之 3 范围**：上述 LinkCtx 族（load_parent/store_parent 已就位，remove_directory 需子目录镜像）+ meta 四方法 + bio_transfer 三方法的接线与逐方法测试；P1-4 的回收执行体挂进 unlink/remove_dir 的 ReclaimZones 出口，随之 3 同轮自然落地。
**✅ 之 3 落地（2026-09-17）**：名字空间族七方法（make_node/make_dir/link/unlink/remove_dir/symbolic_link/read_link）经 LinkCtx/CreateCtx 与镜像桥接线（make_dir 的子目录点项镜像按旧尺寸零经桥分配区号并建映射）；meta 四方法（change_owner/change_mode/update_times/stat_vfs——卷布局八字节十字段）直通；块传输两方法经 `bio_transfer` 加内存后端暂存（peek 按能力协商保持未实现）；`stat_vfs` 的卷空闲在位图上现算（与 C `stadir.c:783` 同法）。三个装配级测试：名字空间族全链、statvfs/元数据/回收可观察性、原始块读回。crate 一百一十四测试全绿，clippy 零告警。**账本仅余 Peek（P1-3）与 Rename（P1-1）两行待建。**
**之 2 开工前的四个待裁决点（2026-09-16 登记，均已裁决）**：
1. 池归属冲突（已裁决：A 变体落地）——`MfsServer` 持 `Option<S>` 源，挂载取走、卸载经 `BlockCache::into_source` 归还；池容量仍来自启动配置（差异不可观察），`ServerCore`/`prepare` 保留于 startup.rs 待 07 篇后续裁决是否收缩（记入批 7 死代码复核）。
2. `MountedFs` 拆借用（已落地）：`Parts` 透镜（server.rs 定义、mount.rs `parts()` 构造）一次拆出表/缓存/超级块/双位图/区策略 + 几何快照；位图为 `MountedFs` 独立字段（挂载装载、同步回写）。
3. `FsDriver::stat` 的字节填充（已按「server 填全部字段」落地）：`STAT_LAYOUT_SIZE=64` 本地小端布局（server.rs 常量注释），P2-5 收敛到 minix-types typed Stat。
4. 冒烟镜像构造（已落地）：最小镜像按 mount.rs 测试模式在 server.rs 测试内构造；冒烟链经 `FsDriver` 直调完成（task.rs 循环已有六项独立测试），经 run() 的脚本传输全链留待之 3 与名字空间接线同轮补。

**V1-P0-2 rmdir 的链接计数与 C 行为不符。**
事实：C 的 remove_dir 依次调用三次 unlink_file 语义（link.c:205 父名 nlinks-1；link.c:210 子目录 `.` 再-1；link.c:211 `..` 使父目录 nlinks-1），子目录 nlinks 归 0（NO_LINK）触发释放。Rust `remove_directory`（link.rs:317-393）对镜像做父名与点项删除后只做一次 `nlinks -= 1`（link.rs:383-388），父目录 nlinks 完全未动：子目录终值 1 而非 NO_LINK，`put` 的释放分支（inode.rs:450）永不触发。
这是正确性缺陷，本轮只登记。修复方向：按 link.c:205-211 的三次递减重建记账，并补「rmdir 后父目录 nlinks 与位图状态」测试；与 V1-P1-4（回收执行者）联动验收。
**✅ 2026-09-17 闭环（Fix #5）**：`remove_directory` 记账改为子目录饱和减二（父名 + 点）、父目录减一（`..` 指向父），与 link.c:205-211 三步一致；子目录落无主值后 `release_child` 上报区号表，由 V1-P1-4 的回收执行体清位摘副本。测试断言落账（子目录链接数等于无主值），crate 一百一十四测试全绿；13 篇 §2.4/§5.1 同步。交 todo-fix 轮处理。

### 3.2 P1

**V1-P1-1 `fs_rename` 整体缺失。** C link.c:255-422 的完整决策树无对应：SAME=1000 收敛、superdir 环走查（link.c:315-342）、同目录「先 DELETE 旧名腾位再 ENTER」（link.c:391-395 的反直觉顺序）、跨目录 `..` 改链与父目录 nlinks 递增（link.c:405-414）。Rust 仅有 `SAME_NAME` 常量（link.rs:32）且测试只断言其数值（link.rs:1003/1154）。修复走 todo-fix 三段式（先讲清 C 的决策树，再给方案对比，最后实施）。
**✅ 2026-09-17 闭环（Fix #8）**：`link::rename` 按决策树全序实现——旧名/新父打开与三类早退、新名试探（同父搜旧镜像，与 C 的 new_dirp==old_dirp 一致）、挂载点忙、环走查（顺「..」上行走缓存，撞旧文件报无效、到根停止、缺点点按最坏处理）、链接到顶报过多、同文件收敛为成功、类型交错各报其错；搬动按 C 次序（同父先删后进、跨父先进后删），目录跨父改写「..」并使新父链接加一（被搬目录镜像经桥原位覆写）。`MfsServer::rename` 接线（同父单镜像、跨父双装载、双回写、回收清单并入执行体）。六个 rename 测试 + 账本 Rename 行翻转（30/1），13 篇同步。

**V1-P1-2 `fs_putnode` 缺失。** C inode.c:38-64：引用计数按 `count-1` 批量递减、超量为 panic。Rust `put` 只减 1（inode.rs:437-478）；配套的 `duplicate`（inode.rs:365）在生产代码零调用。修复时与 V1-P0-1 的装配一起验收（putnode 只有经由分发才可测）。
**✅ 2026-09-17 闭环（Fix #3）**：`InodeTable::put_count`（inode.rs——减 `count-1` 后交 `put` 消耗最后一个引用；未知号、零计数、超量计数按协议违反拒绝且计数不动，对应 C 的三处宕机换成报错）；`MfsServer::put_node` 接线 + 表级三测试（批量释放余一持有、超量拒绝计数不动、未知号拒绝）+ 装配层用例（释放后读回答无效、超量拒绝、根目录完好）。账本 PutNode 行随实现翻转（29/2）。07 篇 §4.3 与 09 篇 §4.2 同步。ReleaseOutcome::ReclaimZones 的执行者仍归 P1-4。

**V1-P1-3 Peek 路径缺失。** C read.c:156-159 的 `FSC_PEEK` 缺块分支无对应；适配器默认以 read 仿真兜底（call.rs:297-305 引 call.c:294-306），所以 mfs 作为无 peek 能力服务器在协议层可工作，但 `CapabilityFlags::HAS_PEEK` 永不置位。方案 A：维持无 peek（与 C 的无 peek server 同型），登记声明即可；方案 B：实现 peek 并接入 VM 零拷贝（与 E-FSVMCACHE 绑定）。
**✅ 2026-09-17 闭环（Fix #9，方案修正）**：核查发现原方案 A 的前提不成立——C mfs **有** peek 入口（`table.c:20` `.fdr_peek = fs_readwrite`），挂载协商会点亮 HAS_PEEK。实现改为：`MfsServer::peek` 复用读路径暖缓存、丢弃字节、只报字节数（等价 C 的 FSC_PEEK 分支）；挂载能力注释同步修正（原「不加能力位」的注释与 C 不符）。Peek 行翻转为已通，账本 **31/0 全通**；14 篇 §1.4、07 篇 §4.3 同步。缺块时的 `lmfs_zero_block_ino` 通知仍归 E-FSVMCACHE（VM 轨道）。

**V1-P1-4 `ReclaimZones` 无执行者。** C 在最后一个引用释放且 `i_nlinks==NO_LINK` 时同步 `truncate_inode(rip, 0)`（inode.c:220-231）真实释放全部数据区与间接块。Rust 只把非零 zone 号打包进 `ReleaseOutcome::ReclaimZones`（inode.rs:450-462，含 zones[7..9] 的间接块号但调用方无从区分类型），crate 内没有位图释放或缓存逐出的消费者（grep `reclaimed` 仅 link.rs:125-134 收集与测试断言）。效果：unlink 最后一链后 zone 实际泄漏。
**✅ 2026-09-17 闭环（Fix #4）**：执行体 `MfsServer::reclaim_zones` 落地——逐非零区号经 `mfs_cache::free_zone` 清位图（含搜索位回拨）并 `free_block` 摘除缓存副本，挂三个出口：`unlink`、`remove_dir`、`put_node`（引用清零且链接数为无主时区表随释放结果携带，此时数据区与间接块的释放动作相同，无需按类型区分——区号→位号换算是唯一映射）。装配级测试以卷空闲区计数为观察口：创建并写入后空闲减一，unlink 加引用释放加同步后空闲复原。

**V1-P1-5 目录块镜像缺桥接。** dir.rs 的四模式搜索以调用方提供的 `Vec<Vec<u8>>` 块镜像流转（dir.rs:158），crate 内不存在从 `BlockCache` 装载镜像或把镜像写回缓存的代码（dir.rs:10-14 自述归文档 14/15），因此 create/unlink/mkdir/link 的目录修改落不到磁盘。方案 A：装配层（`MfsServer` 的方法）提供装载/写回桥，镜像设计保留；方案 B：删除镜像，目录操作直接经 `BlockCache` 槽进行（更贴近 C，少一次拷贝，但 dir.rs 全部签名要改）。推荐 A 先落地保行为，B 作为 V2 轮重写评估项（镜像使每次目录操作多 O(块数) 拷贝）。
**✅ 2026-09-16 闭环（Fix #1）**：新模块 `os/fs/mfs/src/dir_io.rs`——`load_dir_blocks`（逐文件块翻译 + 缓存读出）与 `store_dir_blocks`（整块免读覆写置脏；旧尺寸外追加块经 `alloc_zone`+`write_map` 建映射；尺寸内空洞按损坏报 EIO，与 `list_dir_entries` 同规则）。位图以闭包注入，模块不依赖超级块结构；inode 元数据写回留给装配层。七个新测试全绿（crate 105 个）；文档同步 14 篇 §4.4/§5.4，dir.rs 过时注释更新。批内顺序调整说明：本条先行于 V1-P0-1，因为装配冒烟链的 lookup 依赖本桥。

**V1-P1-6 绝对符号链接的 offset 分歧。** C 在符号链接改写路径后置 `ptr = path`（lookup.c:269），绝对链接返回 ESYMLINK 时 `m_fs_vfs_lookup.offset = ptr - path = 0`（lookup.c:308-309），VFS 从重写路径的起点重新解析。Rust 返回 `offset: component_start`（lookup.rs:424-433），即旧路径中链接组件的起点——对重写后的新路径该值没有意义。修复方向：改为 0 并加钉住测试；同时在 05-stage-vfs 侧对账 VFS 消费 offset 的语义（本条修复只动 minix-fs，VFS 侧只需读代码验证，不构成跨 stage 改码）。
**✅ 2026-09-17 闭环（Fix #6）**：`AbsoluteSymlink` 臂的 offset 改为 0（lookup.rs，注释锚 lookup.c:269/308-309）；新增跨组件钉住测试（路径 sub/link，断言 offset==0 且改写路径为 /etc/hosts），ELEAVEMOUNT 的分量起点语义经核与 C 一致保持不动；03 篇 §4.3 的偏差记录改为「已对齐」并更新 §5.3/§5.4。minix-fs 九十六测试全绿。

**V1-P1-7 mount 适配层丢失驱动标签。** C fsdriver_readsuper 先经 `fsdriver_getname` 取标签再调 `fdr_driver(dev, label)`（call.c:36-41）；Rust `adapt_mount` 硬编码 `driver.bound_driver(input.device, "")`（call.rs:151）。修复方向：`MountInput` 增加 label 字段，由协议层（ReadSuper 消息的 name 字段经 `fetch_name`，data.rs:125）填充；在装配层验收。
**✅ 2026-09-17 闭环（Fix #7）**：`MountInput` 增 `label`（自有串，与 `fsdriver_getname` 的拷贝语义同），`adapt_mount` 搬运真实标签；`RequestBody::ReadSuper` 同步增 label，由传输接缝的取名半填充。专测以 spy 驱动锁定标签搬运（ram0），minix-fs 九十七测试全绿；02 篇挂载适配小节同步。

**V1-P1-8 完备性账本滞后 20 行。** table.rs:40 的 23 行 `PendingDocument` 中 20 行实现已存在（2.1 节清单 A）。table.rs:229-242 的 `live_entries/pending_entries` 是本 crate 唯一的机读化完备状态，当前给出与代码相反的画像，违反该文件头「nothing is ever silently unimplemented」的自订纪律（table.rs:7-10）。
**✅ 2026-09-17 闭环（Fix #2）**：二十行随装配翻转 `LiveInCrate`（含 Lookup——入口经镜像桥与 `lookup_name` 收口），钉住测试更新为 28/3，`cargo test -p minix-fs-mfs` 一百零八全绿；07 篇 §4.3 同步翻转事实与剩余三行（PutNode/Peek/Rename，分属 P1-2/P1-3/P1-1）。

**V1-P1-9 vtreefs/sffs 脱离框架。** `minix-vtreefs` 与 `minix-sffs` 的 Cargo.toml 只依赖 minix-types；而 C 的 libvtreefs/libsffs 都构建在 libfsdriver 之上（libvtreefs/vtreefs.c 的 fs_* 函数族、libsffs/table.c），文档 18 第 6 行也把 01 篇（框架主循环与回调表）列为前置。结果：procfs/ptyfs 的树有了、挂载分发没有；doc 与代码不一致。方案 A：vtreefs 增加对 minix-fs 的依赖，提供 `TreeServer` 的 `FsDriver` 实现（对应 C 的 fs_* 族），procfs/ptyfs 直接消费；方案 B：维持解耦，在文档 18 增补偏离声明并 [ARCH] 三处一致。推荐 A（与 C 结构一致，且装配是迟早要做的事）；sffs 待语义补齐（V1-P2-10）后同样处理。

**V1-P1-10 `PATH_GET_UCRED` 凭据校验缺失。** C lookup.c:147-157 在该旗标置位时经 grant 搬运 ucred 并校验 `ucred_size == sizeof(ucred)`；Rust 侧凭据由调用方以值传入，grant 搬运与长度校验无处实现。修复方向：协议层（data.rs 的通道抽象）补 grant 形态的凭据读取，或书面声明由 VFS 侧传入值并记录偏差（需与 05-stage-vfs 对账后二选一）。
**✅ 2026-09-17 闭环（Fix #10，声明式落账）**：框架消费的是校验之后的值——`Credentials` 结构文档锚定 C 校验点（lookup.c:147-157），grant 长度对 `struct ucred` 尺寸的校验归生产传输的解码半（E-FSRUNTIME 接线时的契约之一），与 VFS 侧对账由 05 轨道持有。若未来传输实现无法承担校验，再回到框架补 grant 形态读取（重开本条）。

**V1-P1-11 procfs 内容面缺口。** C 有而 Rust 无：pid_cmdline/pid_environ（pid.c，需经 VM 读进程内存，受 E-FSVMCACHE 类轨道约束）、root_cpuinfo 与 cpuinfo.c、root_pci、root_ipcvecs、service.c 全族（经 RS 查询服务目录，依赖 RS 轨道）。修复分两批：不依赖轨道的（cpuinfo/pci/ipcvecs 的渲染，C 侧是读内核信息结构）随内核信息通道（对账 E-KERNINFO）走；依赖 RS/VM 的挂 edge。

### 3.3 P2

**V1-P2-1 写路径整块免读优化缺失。** C rw_chunk 在写满块或「块对齐且越过旧 EOF」时用 NO_READ（read.c:175-177）；Rust 对已映射块一律 `AcquireMode::Normal`（write.rs:280-282），每次整块写多一次设备读。修复：在 `write_file` 的分块判断处引入 NoRead 条件（含短末块语义，与 V1-P3-5 一并对账），用 `CountingSource` 断言零设备读。
**✅ 2026-09-17 闭环（Fix #14）**：`ensure_block` 增 `skip_read` 参数——写入从块首开始且写满一整块（`offset==0 && chunk==block_size`）时，已映射块按免读方式拿块；未映射块照旧分配清零。短末块语义随 V1-P3-5 对账。计数源测试锁定：两次整块写穿透读为零；crate 一百二十三测试全绿；15 篇 §4.2 同步免读优化说明。

**V1-P2-2 缓存与传输热路径的拷贝清单。** 五处可消除或收缩的拷贝：(1) `acquire` 的 Normal 路径先读入临时 `staging` 再 `copy_from_slice`（cache.rs:262-272）——`self.source` 与 `self.slots[slot]` 是不相交字段的借用，Rust 允许直接 `self.source.read_block(key, &mut self.slots[slot].data)`；(2) `evict_one` 为借检克隆整块（cache.rs:479-482）；(3) `flush_device` 每脏块 `data.clone()`（cache.rs:352-355）；(4) bio 写路径 `staging_at` 克隆整块再由 `write_slot` 整块拷回（bio.rs:239-250）——用 `slot_data_mut` + 区间 `copy_from_slice` 可省两次整块拷贝；(5) LRU 命中的 `remove_from_free` 线性扫描 O(池深)（cache.rs:520-524）。对照 Redox：redoxfs 的 `DiskCache` 直接在缓冲上读写，无搬运层；对照 C：lmfs 的驱动直读缓冲。方案 A：逐项就地消除（不改 API，收益立得）；方案 B：Buffer 内嵌 prev/next 把 LRU 改成侵入式双向链（与 C cache.c:46-47 同构），(5) 从 O(n) 归 O(1)。推荐 A 全做、B 只做 (5)（池默认 1024，(5) 的常数不可忽略）。

**V1-P2-3 getdents 的冷目录与类型解析退化。** C 用 `get_inode` 从盘加载目标 inode 并取真实 IFTODT 类型（read.c:474、read.c:516-519）；Rust 用 `table.find`（冷目录直接 Invalid，read.rs:348）且类型只查缓存槽、冷槽降级 DT_UNKNOWN（read.rs:421-424），还要求目标必须是目录（read.rs:349-357）。行为可见：对冷目录的 getdents C 成功、Rust EINVAL；readdir 的 d_type C 稳定、Rust 可能 UNKNOWN。修复：装配后改走 `InodeTable::get` 并补冷目录测试。
**✅ 2026-09-17 闭环（Fix #16）**：`list_dir_entries` 签名增 `io` 并改走 `InodeTable::get`——冷目录从盘装载、非目录拒绝（read.c:474 同法）；每个表项目标经打开-释放对取真实模式推导类型，装载失败降级未知而非中断。冷目录行为有测试锁定（放回引用后枚举成功），mfs 一百二十三测试全绿；14 篇 §4.3 同步。

**V1-P2-4 预读策略差异。** C rahead：EOF 截断（read.c:393-394）、临近一阶间接块加窗（read.c:397-402）、映射失败退化顺序号穿洞续排（read.c:426-432）；Rust readahead_file：固定 32 次、遇第一个洞即停、无 EOF 截断与间接加窗（read.rs:278-308）。顺序大文件近似、稀疏与边界文件预读量不同。方案 A：逐条对齐 C 四点；方案 B：保留现状并在文档 14 增补偏离声明（read.rs:11-14 已声明部分取舍）。推荐 A（预读是纯内部优化，对齐 C 无成本争议）。
**✅ 2026-09-17 闭环（Fix #17，方案 A 落地）**：`readahead_file` 签名改为收位置与本次剩余字节数，窗口计算与 C 逐条对齐——剩余块数起窗、临近一阶间接块窗口与块数加一、不足三十二补足、按文件末尾截断、按块层上限截断；洞不再中断排队，顺序猜测继续。既有有界性测试全绿，14 篇 §4.2 同步。

**V1-P2-5 stat/statvfs 的无类型字节布局契约。** `FsDriver::stat(&mut self, inode: u64, out: &mut [u8])`（driver.rs:277-280）与 stat_vfs（driver.rs:310-313）把布局决定权完全交给调用方，每个 server 自定义字节序与字段序（memfs.rs:33-37 自定义 20 字节布局即是例证）。对照 Redox：redox-scheme 用 typed `Stat` 结构；对照 C：Minix3 用共享 `struct stat` safecopy。方案 A：minix-types 定义 `Stat`/`StatVfs` typed 结构（字段与 C 布局一一对应）+ `to_bytes`，trait 签名改 `out: &mut Stat`；方案 B：维持字节切片。推荐 A：字段错位从「联调期 wire 事故」提前到「编译期」，且不改 wire 行为。[ARCH] 判定：内部类型强化（Refactor 级），无需三处标注。
**✅ 2026-09-17 闭环（Fix #12）**：minix-types 权威落地 `Stat`（八十八字节：模式、链接数、属主、属组各二，设备、节点号、特殊设备号、大小、三个时间、块尺寸、块数各八）与 `StatVfs`（八字节十字段八十字节），带 `zeroed()` 与 `write_to`（缓冲不足报无效参数）；`FsDriver::stat/stat_vfs`、两个适配器、任务循环、memfs/pfs/mfs 三个服务器全部收敛到 typed 签名，memfs 的二十字节与 pfs/mfs 的各自布局两套私有字节序消灭；`FsTransport::scratch` 与 `STATUS_SCRATCH_SIZE` 随之移除（typed 结构无需暂存缓冲）。minix-fs 九十八、pfs 十二、mfs 一百二十一测试全绿。

**V1-P2-6 `handle_header` 的双语义返回。** Dispatch 分支同时返回 `Handling::Reply { status: 0 }` 与 `Some(request)`（driver.rs:552-562），调用方必须遵守「有 request 就忽略 Reply」的隐含约定，误用会发出一个内容为空的成功回复。方案 A：返回 `enum HeaderOutcome { Refuse(Handling), Dispatch { request, transaction } }`；方案 B：维持并加文档警示。推荐 A（调用方只有装配层一处，改动面小）。
**✅ 2026-09-17 闭环（Fix #11）**：`handle_header` 返回类型改为 `HeaderAction` 三态（Silence/Reply{status,transaction}/Continue{request,transaction}），虚构的 `Reply{status:0}` 不再存在；随重构 `Handling` 枚举失去全部使用者，按死代码纪律一并移除。九十七测试全绿。

**V1-P2-7 空闲位统计的三份重复实现。** mount.rs:242、meta.rs:349、maint.rs:92 各有一份逐块计位实现，C 只有 stats.c:14 一份。修复：收敛到 superblock.rs 一处（与位图同址），三处改调用；顺带补 origin 起点语义说明（C 从 s_isearch/s_zsearch 起步会漏计尾部，Rust 从 0 计满图——收敛时统一为计满图并记录）。

**V1-P2-8 块层缺口分组。** 本 stage 内：ONE_SHOT 前插（cache.c:533-544）、`lmfs_prefetch` 位图选区间（cache.c:1089-1130）、脏块排序批量写（cache.c:884-885）、运行期池调整与 usage 重估触发（cache.c:119-161、cache.c:1192-1207）。跨轨道：短末块部分读写与 gather/scatter（E-FSBDEV）、vmcache 零拷贝与旗标机（E-FSVMCACHE）。本 stage 内的四项在 V2 轮按「先测试钉行为、再对齐实现」推进。

**V1-P2-9 `write_to_disk` 无只读护栏。** C 只在 `!s_rd_only` 时置脏写回（inode.c:398）；Rust 无该检查（inode.rs:623-663），靠调用方自律；maint.rs:116 的 `check_dirty_mark` 存在但未接入。修复：把只读判断下沉到 `write_to_disk`（Superblock 在手即可判定）。
**✅ 2026-09-17 闭环（Fix #13）**：`InodeIo` 增 `read_only` 字段（`from_superblock` 从挂载状态带入），`write_back` 在只读时报只读错误——护栏从「调用方自律」变为结构强制；专测锁定（只读参数下写回拒绝、磁盘不动）。09 篇 §4.2 同步。

**V1-P2-10 ext2 / isofs / sffs 覆盖面对账。** 三者目前是格式解析层，与文档分期（21/22/23/24 篇声称范围）一致，非缺陷；本行登记 C 侧未覆盖语义块清单（ext2：适配入口/数据路径/分配/名字空间；isofs：查找/getdents/stat/挂载生命周期；sffs：handle/inode/lookup 核心/do_* 全套），作为后续文档与实现排期的输入。完成前 bin 不可用（走 E-FSRUNTIME）。**裁决（2026-09-16）**：不纳入本轮实施战役——分期设计如此；全量语义留待后续「文档 + 实现」独立波次（届时先补 21~24 篇缺失章节，再随 mfs 装配模式逐 server 复用）。

**V1-P2-11 测试缺口（top 项）。** 按分片 A 的对照（第 4 节引用），优先补：rmdir 记账（随 V1-P0-2）、rename 全分支（随 V1-P1-1）、putnode 批量语义（随 V1-P1-2）、写满块零设备读断言（随 V1-P2-1）、冷目录 getdents（随 V1-P2-3）、双 free 位返回 false 的钉住测试（superblock.rs:594 现只测回绕）、`s_max_size` 截断不可达论证的钉住测试。其余中置信度项（unmount busy 路径、父目录 NO_LINK 守卫、BlockSizeMismatch）在装配冒烟测试中顺带覆盖。

### 3.4 P3

- **V1-P3-1** bio.rs:100 `let _ = info;` 是无操作死语句（下一行 115 就用了 `info`），删除无行为影响。
- **V1-P3-2** mfs 的三条 unused import 告警（`DirScan`、`TYPE_SYMLINK`、`TYPE_DIRECTORY`，出现在 fs/mfs/src/dir.rs、link.rs、open.rs 的 import 行），删除即可。
- **V1-P3-3** `Bitmap::alloc` 在 origin 无效时从位 1 起搜（superblock.rs:419-423），C 的 origin 归零后可返回位 0（super.c:59）；Rust 的「位 0 恒保留」是更强的不变量，保留但补注释与钉住测试。
- **V1-P3-4** getdents 适配在「回调缺失 + 参数非法」重叠时 C 报 ENOSYS（先空检 call.c:334 后校验 call.c:337）、Rust 报 EINVAL（先校验 call.rs:329-339）；仅影响未实现 getdents 的服务器收到畸形请求的 errno，随装配轮统一顺序。
- **V1-P3-5** bio_transfer 的 NoRead 判据是 `chunk == block_size`（bio.rs:158），C 是 `chunk == size`（含短末块 last_size，bio.c:194）；短末块全覆写时 Rust 多一次读。与 V1-P2-1 同修。
- **V1-P3-6** 消费方画像：`MemFileServer`（704 行）外部零消费方，定位是框架的第二实现与参考实现（memfs.rs:1-15），不是死代码；`NullDriver` 外部零引用，定位是协议测试 peer（driver.rs:633-643）；`RamDisk` 是生产代码但当前消费方全部位于 mfs 的 `#[cfg(test)]`（boot 内存盘的真实消费随 E-FSBDEV）。三者均保留，处置建议见第 4 节 OQ。
- **V1-P3-7** `is_file_request`（protocol.rs:262-264）是 C 宏 IS_FS_RQ 的直译，dispatch 实际走 `wrapping_sub` 路线（driver.rs:486），该函数生产零调用、仅测试引用。处置见第 4 节 OQ。
- **V1-P3-8** `read_indirect` 对间接块整块逐项校验（read.rs:148-152），C 只校验被访问项且对越界 panic（read.c:317-322）；Rust 更严：含脏数据但未触碰的镜像 C 能继续工作、Rust 拒绝。判定为接受的加固（把 abort 换成错误码符合本仓库约定），保留，登记语义可见性即可。

## 4. 死代码清单

code-excellence 子目标。scope 内无 `#[allow(dead_code)]`、无注释掉的代码块、无无主 TODO；clippy 的 never-read 字段 2 处（fs/mfs/src/read.rs:487 的 `io`、fs/mfs/src/write.rs:535 的 `freed`，均为测试观察结构体中从未断言的计数器）与 1 处多余 `mut`（fs/mfs/src/meta.rs:423）属卫生项，随 V1-P3-2 一并清理。逐项结论：

| # | 位置 | 内容 | 为何可疑 | 处置建议 |
|---|---|---|---|---|
| 1 | os/libs/minix-fs/src/bio.rs:100 | `let _ = info;` | 无操作语句（info 在 :115 实际使用） | 死语句，直接删除，无行为影响 |
| 2 | os/fs/mfs/src/mount.rs:242、meta.rs:349、maint.rs:92 | 三份逐块计空闲位实现 | 同一语义三份实现互为重复（C 只有一份） | 不是死代码但是重复重量：收敛到 superblock 一处，其余改调用（V1-P2-7） |
| 3 | os/libs/minix-fs/src/memfs.rs 全模块 | `MemFileServer` 704 行 | 外部零消费方，仅本 crate 测试与文档用途 | **已裁决（2026-09-16，按推荐默认）**：保留原位——trait 的第二行为实现（Gate D 要求）+ 新 server 作者的参考实现（memfs.rs:5-10 有书面定位），无编译或维护成本 |
| 4 | os/libs/minix-fs/src/driver.rs:640 | `NullDriver` | 外部零引用 | 保留：框架文档明确的协议测试 peer 与新 server 起点（driver.rs:633-639），Gate D 需要 |
| 5 | os/libs/minix-fs/src/bio.rs:351 | `RamDisk`（生产代码） | 生产消费方为零（9 处全在 mfs 测试） | 保留：boot 内存盘的既定生产实现（bio.rs:345-349），真实消费随 E-FSBDEV 落地 |
| 6 | os/libs/minix-fs/src/protocol.rs:262 | `is_file_request` | 生产零调用（dispatch 走 wrapping_sub） | **已裁决（2026-09-16，按证据）**：删除（含测试）——C 侧 `IS_FS_RQ` 同为死宏（vfsif.h:77 仅定义、minix3 全树零使用），保留即 translate 味。由批 7 执行 |
| 7 | os/fs/mfs/src/inode.rs:365 | `InodeTable::duplicate` | 生产零调用（putnode 缺失导致无人增加引用） | 不是死代码：是 V1-P1-2 的组成部分，putnode 落地即复活。保留并在 V1-P1-2 验收 |

## 5. edge 增补指针

本轮向 `../edge_todo.md` 追加 4 条（追加前已对既有 33 条查重，E-REQWIRE 已闭单、E-MINSYS-HYGIENE 已覆盖 minix-sys 的 clippy 卫生项）：

| edge 条目 | 一句话 | 本 stage 侧关联 |
|---|---|---|
| E-FSRUNTIME | 8 个 fs server bin 的进程启动握手（SEF/RS）与运行时接线 | 2.5 节；V1-P0-1 只做框架内事件循环与装配，不做进程握手 |
| E-FSBDEV | bio 层与真实块驱动的接缝（DeviceInfo 生产实现、短末块、聚散 I/O、bdev_open/close） | V1-P2-8、V1-P3-5、mount.rs:8-9 |
| E-FSVMCACHE | 二级缓存的零拷贝页移交与旗标机（cache.c vm_* 族） | cache.rs:15-22；V1-P1-3 方案 B、V1-P1-11 的 cmdline/environ 依赖 |
| E-FSCMDS | fsck/mkfs 命令占位（os/commands/sbin/，main.rs 各 14/15 行） | ext2/mfs 的离线一致性工具，随对应 server 语义成熟后认领 |

既有条目挂接（不新开）：minix-types 的 1 条 clippy 告警（libs/minix-types/src/ipc/vm.rs:660）建议并入 E-MINSYS-HYGIENE 的执行车辆；REQ_* 契约问题一律走已闭单的 E-REQWIRE 先例（minix-types 单点权威 + 双侧对账测试）。

## 6. Rule Discovery（Step 5.7）

本轮两个候选新模式，建议回灌 review-patterns：

1. **账本漂移（候选模式 84）**：机读化完备账本（如 table.rs 的 live/pending 三态）与实现状态脱节——20 行写着 pending 的行实现已存在。触发信号：账本文件存在且断言固定计数（8/23），而对应实现文件的 pub 函数数远超 live 行数。建议规则：账本翻转必须与实现同一提交；评审时对任何「状态账本」执行一次抽查（随机 3 行核对实现存在性）。
2. **装配断层（候选模式 85）**：框架层与实现层各自测试全绿，但两个方向在同一个调用图中从未相遇（框架骨架零生产消费方 + 实现层零框架接线）。触发信号：对框架 crate 的骨架符号（适配器、分发函数）grep 生产调用方为 0。建议规则：评审框架型 crate 必查「生产消费方计数」；零消费方不是死代码（是未验证契约），但必须登记装配开口项并要求冒烟测试排期。

## 7. gate-evidence 汇总与对照来源

命令与输出（2026-09-16，HEAD abccac10）：

- `bash tools/design-coverage-check.sh fork-syscall-rewrite/15-stage-fs` → `ALL DOCS COMPLETE`，exit 0（outline 24 / outline-review 24 / design 24）。
- `cargo test -p minix-fs -p minix-fs-mfs -p minix-fs-pfs -p minix-fs-procfs -p minix-fs-ptyfs -p minix-fs-ext2 -p minix-fs-isofs -p minix-fs-vbfs -p minix-fs-hgfs -p minix-vtreefs -p minix-sffs`（在 os/ 下）→ 289 passed / 0 failed。
- `cargo clippy`（同 11 包，--all-targets）→ 告警清单：8 条 empty `loop {}`（7 个 server bin 占位 + minix-types 1 处同型）、2 条循环可改迭代器（os/libs/minix-fs/src/bio.rs:273、:284）、3 条 unused import（mfs）、2 条 never-read 字段（read.rs:487、write.rs:535）、1 条多余 mut（meta.rs:423）、1 条 assert_eq 布尔字面量（bio.rs:772）、1 条 variant 尺寸差（minix-sffs/src/handles.rs:50）、minix-types 1 条、minix-sys 2 条（建议并入 E-MINSYS-HYGIENE）。生产代码无 error 级告警。
- unsafe 普查：`grep -rn "unsafe " fs/*/src libs/minix-fs/src libs/minix-vtreefs/src libs/minix-sffs/src` 命中 0。

Redox / 社区对照来源（本轮引用的结论出处）：

- redoxfs 架构（Block trait、DiskCache、Table/TreeNode 写时复制树）：https://github.com/redox-os/redoxfs 与 https://doc.redox-os.org/book/redoxfs.html
- redox-scheme 的请求多路复用模型（Socket/Request/Response/Tag、read_requests/write_responses 批量读写，区别于 Minix3 的同步一问一答）：https://docs.rs/redox-scheme
- 结论使用处：V1-P0-1（框架/服务器两截结构的正当性与装配必要性）、V1-P2-2（缓冲直读无搬运层的对照）、V1-P2-5（typed Stat 对照）。Minix3 的同步协议 + 事务号编码是 Rewrite 边界（外部行为），不因 Redox 的异步模型而改。

## 8. 修复推进顺序建议

1. **第一批（正确性 + 装配，互相解锁）**：V1-P0-1 装配（`MfsServer` + 框架事件循环 + 冒烟测试）→ V1-P1-8 账本翻转随行 → V1-P1-2/V1-P1-4（putnode 与回收执行体，依赖装配才可测）→ V1-P1-5（镜像桥，依赖装配）。
2. **第二批（正确性缺陷修复）**：V1-P0-2（rmdir 记账）、V1-P1-6（offset 语义）、V1-P1-7（mount 标签）——三条都是小修，可与第一批并行但不与装配同提交。
3. **第三批（行为对齐 + 收敛）**：V1-P1-1（rename，独立大件）、V1-P1-3/V1-P1-10（协议面补全）、V1-P2-1/2/3/4/5/6/7/9（性能与 API 强化，互相独立可拆）。
4. **第四批（轨道协作）**：V1-P1-9（vtreefs 接框架）、V1-P1-11（procfs 内容面）、V1-P2-8/10（块层与覆盖排期），edge 四条按单线程队列另执。
5. 每批验收命令：`cargo test -p minix-fs -p minix-fs-mfs`（及受影响包）+ 重放第 7 节基线对比；P0 未清不得标 CONVERGED。

## 9. 存档指引

- 本文件是 15-stage-fs 的首个 todo（此前无历史 todo 需要清理）。轮次编号 V1；后续轮次 V2、V3 递增，闭单条目移动到本文件末尾「已闭单」小节或独立 archive 文件（对齐 `../edge_todo_archive.md` 先例）。
- 修复执行一律走 todo-fix 单线程：先讲明白是什么为什么，再给多方案对比，最后按 fix-guard 实施（修前读目标行 ±5 行、grep 确认、一次一条、修后 grep 重放 + 测试）。
- 跨 stage 条目唯一入口是 `../edge_todo.md`；本文件第 5 节只维护指针，不复制正文。
