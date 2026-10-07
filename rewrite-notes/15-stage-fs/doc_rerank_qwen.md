# 15-stage-fs 文档重建蓝图（qwen）

## 0. 元数据

```text
执行者 = qwen
日期 = 2026-09-19
目标目录 = notes/rewrite/fork-syscall-rewrite/15-stage-fs/
仓库根目录 = /home/xzhao/github/minix-rs
当前提交号 = 4c99bc2e7
任务 = R 相·重建蓝图：只产出本蓝图，不修改任何正文
```

### 0.1 审查范围

- **算文档**：`00-fs-overview.md`、`01`~`24` 编号文档、`99-global-concepts.md`（共 26 篇）。
- **算参考材料（不重建，只读取线索）**：`plan.md`、`todo.md`、`draft/README.md`。
- **范围外**：`.design/`（72 个中间产物文件，项目规范禁止正式引用）、其它 AI 的
  `doc_rerank_deepseek.md` / `doc_rerank_glm.md`（未读取，见 §0.4 排他证据）、`00-outline`
  之类不存在文件；VFS 服务侧（05-stage-vfs）、块驱动（16-stage-drivers）、进程启动握手
  （E-FSRUNTIME → 14-stage-runtime）等按 plan §5.4 排除表移交。

### 0.2 读取清单

- **文档**：26 篇全部读取头部声明（分类/源码/Rust 模块/前置/不讲什么/一句话问题）与
  小节骨架（`grep -n "^## \|^### "` 全量），并抽读 01/04/05/06/08/09/13/14/15/16/18/19/24 正文段落。
- **C 源码（ground truth）**：`minix3/minix/lib/libfsdriver/`（fsdriver.c、table.c 全文；
  call.c/utility.c/dentry.c/lookup.c 按锚点抽对）、`libminixfs/`（cache.c、bio.c）、
  `fs/mfs/`（main.c 全文、super.c/const.h 魔数段）、`fs/pfs/pfs.c`（函数行号全列）、
  `fs/procfs/main.c`（run_vtreefs 入口）、`fs/ptyfs`、`fs/ext2`、`fs/isofs`、`fs/vbfs`、`fs/hgfs`
  （main/入口行号）、`lib/libvtreefs/vtreefs.c`、`lib/libsffs/main.c`、
  `include/minix/vfsif.h`（REQ 全集/NREQS/错误码）、`include/minix/com.h`（FS_BASE 0xA00）、
  `servers/vfs/main.c:499-524`（do_init_root）、`kernel/table.c:44-64`（boot_image）。
- **非 C 制品**：`minix3/minix/fs/Makefile.inc`（BINDIR=/service）、`os/qemu-tests/`（FS 用例缺位核查）、
  `os/xtask/`、`minix3/minix/lib/libfsdriver/Makefile` 等构建文件、消息布局（`include/minix/ipc.h` 系）。
- **边界材料**：`00-master-plan/README.md`（全文）、`edge_todo.md`（E-FSRUNTIME/E-FSBDEV/E-FSVMCACHE/E-FSCMDS
  条目位置 :897/:914/:931/:948）、`14-stage-runtime/00-runtime-overview.md`（全文，确认前置边界）、
  `15-stage-fs/plan.md` + `todo.md`（全文）。
- **Rust 实现入口**：`os/libs/minix-fs/src/`（13 模块）、`os/fs/{mfs,pfs,procfs,ptyfs,ext2,isofs,vbfs,hgfs}/src/`、
  `os/libs/minix-vtreefs/src/`、`os/libs/minix-sffs/src/`；`os/libs/minix-fs/src/lib.rs:1-30` 模块→文档映射注释全文。

### 0.3 使用的命令与关键输出（证据摘录）

| 命令 | 关键输出 |
|------|---------|
| `wc -l *.md` | 01=358、04=310、13/14=241/244、其余 171~284；00/99=21（pending 骨架）；plan=396、todo=265 |
| `cat libfsdriver/fsdriver.c` | fsdriver_process L17-62、fsdriver_terminate L64-71、fsdriver_task L76-97（全文见 §1） |
| `cat libfsdriver/table.c` | callvec 32 项（REQ_GETNODE 无槽位） |
| `sed -n '499,524p' servers/vfs/main.c` | do_init_root：`mount_pfs()` L510、`mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, …)` L517 |
| `grep kernel/table.c` | `{PFS_PROC_NR,"pfs"}` L62、`{MFS_PROC_NR,"mfs"}` L63 |
| `grep vfsif.h` | REQ_GETNODE L41（"Should be removed"）、NREQS=34 L75、EENTERMOUNT/-301 L26-28、RES_HASPEEK L22 |
| `grep com.h` | `FS_BASE 0xA00` L589 |
| `grep fs/mfs/main.c` | init_fresh：`lmfs_may_use_vmcache(1)` → inode 清零 → `init_inode_cache()` → `lmfs_buf_pool(DEFAULT_NR_BUFS)`（L47-65）；SIGTERM→`fs_sync()`→`fsdriver_terminate()`（L70-78） |
| `grep mfs/const.h super.c` | 魔数五种：SUPER_MAGIC 0x137F L22、SUPER_REV L23、SUPER_V2 0x2468 L24、SUPER_V2_REV L25、SUPER_V3 0x4d5a L26；read_super 判定 `magic==SUPER_V2\|\|SUPER_MAGIC` → 拒（super.c:252）、`!=SUPER_V3` → 拒（:258）——文档 08 §1.2 "三魔数两命运" 与代码一致 |
| `grep -c 工具生成 *.md` | **47 处**工具生成锚点（正式 26 篇内：01=16、04=1、05=8、06=14、17=1、18=1、19=4、20=1、21=1），符号名系统性滞后（§3.5） |
| `sed -n '215,220p' pfs.c` | pfs.c L217 实际是 `pfs_read` 签名段，而文档 06 §2.5 锚点写 `pfs_putnode（L217，工具生成）`——锚点符号名错、行号近 |
| `grep 15-stage-fs/[0-9] 引用统计` | stage 内全路径引用 127 处；"第 NN 篇"式引用 141 处；外部文档（16-stage-drivers plan/两篇、18-stage-commands、edge_todo、14/17/05-stage、master-plan README、xtask）约 15 处；Rust 注释 `os/libs/minix-fs/src/lib.rs` 13 处 |
| `cargo test`（todo.md §7/§9 转述 + 基线核对） | FS 域 11 包：289→338 passed（2026-09-17 终验） |

### 0.4 排他声明

未读取任何 `doc_rerank_*`（他人产物）与 `.design/`、`tmp_design_and_todo/` 内容。
`26-ptyfs.md`、`24-vtreefs.md` 之类的失配引用经 grep 确认只存在于他人蓝图中，
正式 26 篇文档内无此引用（grep 输出为空）。

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

本 stage 是**混合型**：`libfsdriver`/`libminixfs` 是**库与框架型**（先抽象层与契约，再骨架，
后实现族）；8 个 server 各自是**服务事件循环型**（诞生与初始化 → 消息接口 → 核心数据结构 →
按场景分组的请求处理 → 与邻接服务的协议）；ext2/isofs/vbfs-hgfs 相对 mfs 是**集合变体**
（参考实现 + 差异展开）。判定依据：所有 server 的 main 最终都落到同一个
`fsdriver_task(&table)`（fsdriver.c:76），server 之间只有接线表与回调实现不同
（mfs table.c 31 项、pfs pfs.c:437 附近 9 项、ptyfs table 8 项、vtreefs/sffs 各一表）。
因此主线取"**框架 → boot 因果序 → 参考实现（一次请求的生命周期 + 数据结构分层）→ 变体差异**"，
这与 prompt §9 "服务事件循环型以一次请求生命周期作主线" 一致。

### 1.2 真序表（逐条可核对，全部来自 C 源码，不转述文档）

| # | 动作 | 锚点 | 说明 |
|---|------|------|------|
| T1 | boot image 登记 pfs、mfs | `minix3/minix/kernel/table.c:62-63` | FS server 是 boot image 成员；pfs 在 mfs 之前 |
| T2 | server 进程诞生：`main → env_setargs → sef_local_startup → sef_startup` | `fs/mfs/main.c:14-42`；`fs/ext2/main.c:29-61`；`fs/isofs/main.c:43-62`；`fs/pfs/pfs.c:419`；`fs/ptyfs/ptyfs.c:408`；`lib/libvtreefs/vtreefs.c:52-59`（procfs 经 `run_vtreefs` 走此路，`fs/procfs/main.c:77-93`）；`fs/vbfs/vbfs.c:98-132`、`fs/hgfs/hgfs.c:64-99`（经 `sffs_init`，`lib/libsffs/main.c:17,58`） | 每个 FS server 同构：SEF 启动 + 注册 init/signal 回调 |
| T3 | 初装（mfs 为例）：`lmfs_may_use_vmcache(1)` → inode 表清零 → `init_inode_cache()` → `lmfs_buf_pool(DEFAULT_NR_BUFS)` | `fs/mfs/main.c:47-65` | 顺序即依赖；其余 server 的 init_fresh 各自装配 |
| T4 | VFS 根装配：`do_init_root` → `mount_pfs()` → `mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, 0, "mfs", "fs_imgrd")` | `servers/vfs/main.c:501-524`（mount_pfs L510、mount_fs L517） | boot 期 pfs 先于 mfs 挂载；期间 `worker_allow(FALSE)` 挡住外部请求 |
| T5 | 主循环：`while (fsdriver_running \|\| fsdriver_mounted) { sef_receive_status(ANY,…); fsdriver_process(fdp,…); }` | `lib/libfsdriver/fsdriver.c:76-97` | 循环条件保证"已承诺退出但未卸载"时收完尾账 |
| T6 | 分发五步：①非 VFS 来源/通知 → `fdr_other` 旁路且不回复（L26-31）②`TRNS_GET_ID`/`TRNS_DEL_ID` 拆事务号（L34-35）③挂载门禁：`fsdriver_mounted \|\| call_nr==REQ_READSUPER` 否则 EINVAL（L39-46）④`call_nr -= FS_BASE` 查 `fsdriver_callvec`（回绕有意，L40-45；表 `table.c:6-40` 32 槽）⑤回复 `TRNS_ADD_ID(r, transid)` + `fdr_postcall`（L50-60） | `lib/libfsdriver/fsdriver.c:17-62` | 全部 33 个 REQ（`vfsif.h:41-73`，NREQS=34 L75，`FS_BASE 0xA00` `com.h:589`）中 REQ_GETNODE 声明但无适配器（`vfsif.h:41` "Should be removed"）——32 槽 |
| T7 | 请求适配：每个 `fsdriver_*` 适配器做"取参校验 → fdr_* 回调 → 装回复"三步 | `lib/libfsdriver/call.c`（1013 行，31 个适配器）；辅助在 `utility.c`（copyin/copyout/zero/getname）、`dentry.c`（getdents 组装）、`lookup.c`（整路径漫步：挂载点跨越 EENTERMOUNT/ELEAVEMOUNT/ESYMLINK，`vfsif.h:26-28`） | 单请求生命周期的"翻译层" |
| T8 | 挂载落地（mfs）：`fs_mount`（记设备 → `read_super` → unclean 降级只读 → 块大小核对 → 数已用区 → `get_inode` 根 → 写脏超级块） | `fs/mfs/mount.c:10-98`；`super.c:241-355`（魔数判定 L252/L258）；`inode.c:118-174` | 一次 REQ_READSUPER 的纵深 |
| T9 | 数据通路：`fs_readwrite` → `rw_chunk` → `read_map`（直接 7 + 单重 1 + 双重 1，`mfs/inode.h` z_zones[10]）→ `rd_indir`/`get_block_map` → 缓存 `get_block_ino`（`libminixfs/cache.c:298-498`）→ 驱动 `lmfs_goto`/读写（`libminixfs/bio.c`） | `fs/mfs/read.c:23-111,117-204,210-279,281-325`；`write.c:28-185,254-305` | 读侧与写侧共用翻译；写侧多分配/回收（`write_map` WMAP_FREE，`write.c:28-185`） |
| T10 | 终止：SIGTERM → `fs_sync()`（先 inode 后块，`fs/mfs/misc.c:8-23`）→ `fsdriver_terminate()`（`running=FALSE; sef_cancel()`） | `fs/mfs/main.c:70-78`；`fsdriver.c:64-71` | 与 T5 循环条件呼应 |
| T11 | 运行时加载路径：ptyfs/procfs 由 RS 在系统启动后拉起（boot_image 无条目）；ext2/isofs/vbfs/hgfs 按需挂载 | `kernel/table.c:44-64`（boot_image 全表无 procfs/ptyfs/ext2/isofs）+ `00-master-plan/README.md` boot 两层语义 | 决定文档把 18~24 放在 boot 主线之后 |

### 1.3 序差表（运行时序 vs 教学序，逐条记录）

| # | 运行时事实（锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---|---|---|---|
| D1 | 框架代码与 server 代码在同一个进程内链接执行，不存在"框架先运行"（T2/T5 每个 server 都经历） | 框架 01~05 前置于一切 server（06 起） | 8 个 server 共享同一骨架，先讲骨架则 server 只讲差异（变体 diff 原则） | 06/07 开篇回指"你已经见过消息循环" |
| D2 | 运行时 mount 一次调用贯通 T8：`fs_mount` 先发生，其中调 `read_super` 与 `get_inode` | 文档序 08（super）→09（inode）→10（mount 纵贯） | 数据结构先于消费者，否则 mount 一节里"身份证/根节点"全是悬空词 | 10 §2.1 按调用序纵贯一次，把 08/09 的结论按步引用 |
| D3 | 运行时一次写文件触发的调用链是 `write_map → alloc_zone → bitmap`（自顶向下），位图在 `super.c`，分配策略在 `cache.c` | 14（读）→15（写）紧邻；位图/分配在 08/07 先讲 | 位图是"账本"概念独立于读写路径 | 15 §前置声明列出 07/08 消费点 |
| D4 | pfs 的懒时间与 mfs 的懒时间是同一机制的两个实例（`pfs.c` 内联；`mfs/inode.c:349-370 update_times`） | 06（pfs，boot 第一个挂载）先讲透，09 §1.6 只讲 mfs 增量并回指"第 06 篇" | boot 顺序 + 最小完整样例优先 | 09 §1.6 带跨篇引用（已存在，合规） |
| D5 | mfs `main.c` 初装第 1 步就打开 vmcache（`lmfs_may_use_vmcache(1)`），而 VM 二级缓存机制在文档 04 内是后半节 | 保持现状（04 §1.5 概念 + §2.5/§3.3 展开），07 §1.1 只回指"第 04 篇第 1.5 节讲这条通道" | 二级缓存与块缓存本体不可分（同一 get_block 路径的两级）；拆篇会切断一次拿块 | 07 §1.1 回指（已存在） |

**结论**：现目录 00→01~05→06→07~17→18~20→21~24→99 的骨架与真序（T1~T11）及
依赖序一致，未发现需要移动整篇位置的前向引用违例。本蓝图的重建对象不是"顺序"，
而是第 3 节列出的**内容性缺陷**（骨架 pending 篇、锚点系统性错误、篇内混杂与漂移声明过期）。

---

## 2. 知识点全集（存量池 + 新增池）

### 2.1 编号与图例

- 编号 `K-0xx`，stage 内唯一；汇总对齐键 = 名称 + 锚点。
- 类型：`概念` / `机制` / `数据结构` / `接口协议` / `约束不变量` / `架构演进` / `工具工程` / `测试`。
- 来源：`存量`（现有文档承载，去向受步骤 4 规则约束）/ `新增`（现有文档没有，
  由覆盖审计追加，须带证据锚点）。
- 主讲述点：重复概念的**唯一**完整讲述位置，其余改为一句话 + 引用。

### 2.2 框架层（01~05）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主讲述点 |
|------|------|------|------|----------|------|----------|
| K-001 | 文件服务器=被驱动的订单厨房（单线程事件循环心智） | 概念 | 存量 | 01 §1.1 | fsdriver.c:76-97 | 01 |
| K-002 | REQ 请求编号全集（FS_BASE+1..33）与"三十三张标准小票" | 接口协议 | 存量 | 01 §1.2/§2.5 | vfsif.h:41-75; com.h:589 | 01（语义）/99（表） |
| K-003 | 事务号编码 TRNS_GET_ID/ADD_ID/DEL_ID（低 16 位） | 接口协议 | 存量 | 01 §2.5 | fsdriver.c:34-35 | 01 |
| K-004 | callvec 32 槽与 REQ_GETNODE 死槽（声明无适配器） | 约束不变量 | 存量 | 01 §1.3/§2.4 | table.c:6-40; vfsif.h:41 | 01 |
| K-005 | 挂载门禁：未挂载只受理 REQ_READSUPER，其余 EINVAL | 机制 | 存量 | 01 §1.4 | fsdriver.c:39-46 | 01 |
| K-006 | 非 FS 消息旁路 fdr_other（通知不回复） | 机制 | 存量 | 01 §1.5 | fsdriver.c:26-31 | 01 |
| K-007 | 能力协商 RES_THREADED/RES_HASPEEK/RES_64BIT（挂载回复携带） | 接口协议 | 存量 | 01 §1.6 | vfsif.h:20-23; call.c:48-51 | 01 |
| K-008 | 终止路径：fsdriver_terminate + sef_cancel + 循环条件（running‖mounted） | 机制 | 存量 | 01 §2.3 | fsdriver.c:64-97 | 01 |
| K-009 | 请求编号/事务号/标志/回调表/挂载态/分类的 Rust 类型化（enum/trait/newtype） | 架构演进 | 存量 | 01 §3.1~3.6 | protocol.rs; driver.rs; task.rs | 01 |
| K-010 | 单线程假设写进文档不做类型（!Send/!Sync 合理性） | 约束不变量 | 存量 | 01 §3.7 | AGENTS 执行模型; fsdriver.c:76 | 01 |
| K-011 | 空服务器 NullDriver=协议测试对端 | 工具工程 | 存量 | 01 §3.8 | driver.rs:633-643 | 01 |
| K-012 | Linux/Redox 对照（异步/同步请求模型差异） | 概念 | 存量 | 01 §1.7 | todo.md §7 对照来源 | 01（标注支线） |
| K-013 | 适配器三步：先校验、再回调、后装回复；失败回调零调用 | 机制 | 存量 | 02 §1.1 | call.c:21-34 | 02 |
| K-014 | 31 适配器五组编队（挂载 5/数据 8/命名空间 9/元数 5/块 4） | 接口协议 | 存量 | 02 §1.2/§2.1~2.5 | call.c 全文件 | 02 |
| K-015 | 校验规则：位置（seek 边界）、长度、计数（grant/offset/EOF） | 约束不变量 | 存量 | 02 §1.3 | fsdriver_check(call.c 内) | 02 |
| K-016 | 点名字符全网统一规则（`.`/`..` 拒绝进服务器） | 约束不变量 | 存量 | 02 §1.4 | fsdriver_getdotname/validate | 02 |
| K-017 | 窥视模拟：无 peek 服务器以 read 仿真（默认能力协商） | 机制 | 存量 | 02 §1.5 | call.c:294-306 | 02 |
| K-018 | 回复形状：位置推进（offset）与字节计数两类 | 接口协议 | 存量 | 02 §1.6 | call.c 各适配器 | 02 |
| K-019 | 空回调默认值三类（成功类 7/ENOSYS 类 25/peek 条件类） | 约束不变量 | 存量 | 02 §2.6 差异 | call.rs 保真注释; todo.md §2.2 | 02 |
| K-020 | MountInput 携带驱动标签（label 经 getname 到 fdr_driver） | 接口协议 | 存量(修复后) | 02（Fix #7 同步） | call.c:36-41; call.rs adapt_mount | 02 |
| K-021 | 数据通道两形态：远端授权 grant vs 本地缓冲（Rust 枚举+trait） | 机制 | 存量 | 03 §1.1/§2.1 | utility.c:4-78 | 03 |
| K-022 | 边界检查走廊护栏（越界先拒，宕机改错误） | 约束不变量 | 存量 | 03 §1.2 | utility.c fsdriver_copy* | 03 |
| K-023 | 名字获取四道关卡（grant→长度→NUL 终结→本地点名检查） | 机制 | 存量 | 03 §1.3/§2.2 | utility.c:83-105 | 03 |
| K-024 | 目录项编码器三件套（init/add/finish，staging 摆盘） | 机制 | 存量 | 03 §1.4/§2.3 | dentry.c:1-99 | 03 |
| K-025 | 整路径漫步：挂载点起步/根自环/出逃 offset/符号链接 7 次上限/putnode 纪律 | 机制 | 存量 | 03 §1.5/§2.5 | lookup.c:117-333 | 03 |
| K-026 | EENTERMOUNT/ELEAVEMOUNT/ESYMLINK 三协议错误码语义 | 接口协议 | 存量 | 03 §2.5 | vfsif.h:26-28 | 03（语义）/99（值） |
| K-027 | 凭据获取 PATH_GET_UCRED（grant 搬运+尺寸校验；Rust 声明值前校验归传输解码） | 约束不变量 | 存量(Fix #10) | 03 §4.3 偏差记录 | lookup.c:147-157; lookup.rs | 03 |
| K-028 | buf 缓冲头与取值模式（正常/免读/窥视三态） | 数据结构 | 存量 | 04 §1.4/§2.1 | libminixfs.h struct buf; cache.c | 04 |
| K-029 | 池与哈希+LRU（front/rear 双向链，MINBUFS 下限） | 数据结构 | 存量 | 04 §1.2/§2.2 | cache.c:38-71 | 04 |
| K-030 | 拿块主流程 get_block_ino（命中/逐出/读入/引用计数） | 机制 | 存量 | 04 §2.5 | cache.c:298-498 | 04 |
| K-031 | 归还与腾空（put_block/freeblock，逐出失败恢复原样） | 机制 | 存量 | 04 §2.6 | cache.c:512-608,252-272 | 04 |
| K-032 | 读写盘与散射聚集（read_block/rw_scattered，短末块归轨道） | 机制 | 存量 | 04 §2.7 | cache.c:723-777,840-982 | 04 |
| K-033 | 缓存级预读三函数（readahead/readahead_limit/prefetch 位图选段） | 机制 | 存量 | 04 §2.8 | cache.c:987-1131 | 04 |
| K-034 | 刷盘与失效（bflush 设备/全部、lmfs_invalidate 卸载路径） | 机制 | 存量 | 04 §2.9 | cache.c:1136-1166,1295-1321 | 04 |
| K-035 | 容量启发式与用量变更（fs_bufs_heuristic/change_blockusage/池调整 resize） | 机制 | 存量 | 04 §1.6/§2.3/§2.10 | cache.c:73-162,1192-1293 | 04 |
| K-036 | VM 二级缓存（旗标字/块标签/页内存/四线上调用；池内存来源与开关回落） | 机制 | 存量 | 04 §1.5/§3.3/§5.5 | cache.c:443-451,1236-1239; vm_cache.rs | 04（通道此端）；对端归 02-stage-vm |
| K-037 | 单线程无锁缓存（根因=消息循环串行） | 约束不变量 | 存量 | 04 §前置/§3 | fsdriver.c:76 推论 | 04 |
| K-038 | bio 四要素取书单（设备/位置/长度/方向；窥视=收货缺席的读） | 概念 | 存量 | 05 §1.1 | bio.c lmfs_bio data==NULL 分支 | 05 |
| K-039 | 三道关卡核对单据（设备存在/范围合法/分区裁剪） | 约束不变量 | 存量 | 05 §1.2 | bio.c:80-115 区段 | 05 |
| K-040 | 逐块行走（首尾不满中间整块）与 NO_READ 整块覆写免读 | 机制 | 存量 | 05 §1.3/§1.5 | bio.c:158-230（含 :194 last_size） | 05 |
| K-041 | 读预取 block_prefetch（顺路暖缓存） | 机制 | 存量 | 05 §1.4 | bio.c block_prefetch | 05 |
| K-042 | 驱动绑定 lmfs_driver（label→端点）与 NEW_DRIVER 消息 | 接口协议 | 存量 | 05 §1.7/§2.2 | bio.c:48-53; call.c fsdriver_newdriver | 05 |
| K-043 | 刷后失效 lmfs_bflush（关门前清场） | 机制 | 存量 | 05 §1.6/§2.5 | bio.c:236-263 | 05 |
| K-044 | 内存盘 RamDisk（boot 镜像载体，生产代码待消费） | 工具工程 | 存量 | 05 §3.5 | bio.rs:345-349（E-FSBDEV） | 05 |
| K-045 | 块层 DeviceInfo/BlockSource trait 化 + 真驱动桥接归轨道 | 架构演进 | 存量 | 05 §3.1/§3.6 | bio.rs; bdev_bridge.rs | 05 |

### 2.3 pfs（06）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主讲述点 |
|------|------|------|------|----------|------|----------|
| K-046 | boot 第一个挂载的因果（管道不依赖磁盘，永远就绪） | 概念 | 存量 | 06 §1.1 | servers/vfs/main.c:510; kernel/table.c:62 | 06 |
| K-047 | 512 节点表 + 空闲链（PFS_NR_INODES） | 数据结构 | 存量 | 06 §1.2/§2.1 | pfs.c:49 区 | 06 |
| K-048 | 无根挂载（pfs_mount 不做设备核对） | 机制 | 存量 | 06 §2.2 | pfs.c:50-86 | 06 |
| K-049 | newnode/putnode/findnode 借还最小闭环（putnode 批量 count-1） | 机制 | 存量 | 06 §1.3/§2.4/§2.5 | pfs.c:105,126,184 | 06 |
| K-050 | 管道读写（环形缓冲、头尾、异步唤醒在 VFS 侧） | 机制 | 存量 | 06 §1.4/§2.6 | pfs.c:217-378 | 06 |
| K-051 | 懒时间完整机制（三标记、查询时一次时钟结清） | 机制 | 存量 | 06 §1.5 | pfs.c put_stat/update 逻辑 | **06（首次完整）** |
| K-052 | 截断/状态/改模式（p 半部、chmod 只动权限位） | 机制 | 存量 | 06 §2.7 | pfs.c:298-378 | 06（chmod 细节主讲述在 16 §1.1，06 实例引用） |
| K-053 | 信号退出先 sync 再下班 | 机制 | 存量 | 06 §1.6/§2.8 | pfs.c:380-451 | 06 |
| K-054 | 九回调接线表 + main（唯一已接 FsDriver 的服务器） | 数据结构 | 存量 | 06 §2.8/§4 | pfs.c table 段; os/fs/pfs/src/lib.rs:274 | 06 |
| K-055 | pfs Rust 建模：编号栈/变长缓冲/Clock trait/状态布局本地化 | 架构演进 | 存量 | 06 §3 | pfs/src/lib.rs | 06 |

### 2.4 mfs 参考实现（07~17）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主讲述点 |
|------|------|------|------|----------|------|----------|
| K-056 | 初装四步顺序即依赖（vmcache 开关→inode 清零→init_inode_cache→buf_pool） | 机制 | 存量 | 07 §1.1/§2.2 | mfs/main.c:47-65 | 07 |
| K-057 | 信号语义：SIGTERM→fs_sync→terminate；其余信号忽略 | 机制 | 存量 | 07 §1.2/§2.3 | mfs/main.c:70-78 | 07 |
| K-058 | 接线表 mfs_table 31 行（通/待两态账本 EntryStatus） | 数据结构 | 存量 | 07 §1.3/§2.4 | mfs/table.c:13-45; table.rs:40 | 07 |
| K-059 | 拿块包装 get_block（块错误宕机→Rust 报错） | 机制 | 存量 | 07 §1.4/§2.5 | mfs/cache.c:22-37 | 07 |
| K-060 | 区分配策略 alloc_zone（hint s_isearch/s_zsearch、回绕、断粮 ENOSPC）+ free_zone | 机制 | 存量 | 07 §1.5/§2.6/§2.7 | mfs/cache.c:42-109; super.c:29-156 | 07 |
| K-061 | mfs 服务装配终态：MfsServer+Parts+impl FsDriver+冒烟链（31/0 账本） | 架构演进 | 存量(todo V1-P0-1 闭环) | 07 §4.5 | todo.md §3.1; server.rs | 07 |
| K-062 | 磁盘七段布局（boot/super/位图×2/inode 区/填充/数据区） | 数据结构 | 存量 | 08 §1.1/§2.1 | super.h:4-60 | 08 |
| K-063 | 三魔数两命运（V1 0x137F/V2 0x2468 拒、V3 0x4d5a 放行；版本检查先于字段解释） | 约束不变量 | 存量 | 08 §1.2 | const.h:22-26; super.c:252,258 | 08 |
| K-064 | 块大小五连验 + 几何 sanity（数字要像话） | 约束不变量 | 存量 | 08 §1.3/§1.5 | super.c:241-355 | 08 |
| K-065 | 首区计算 z_one_zone（装不下现算）与 s_max_size 立方封顶 | 机制 | 存量 | 08 §1.4 | super.c:283-330 区 | 08 |
| K-066 | 位图账本 alloc_bit/free_bit（origin hint、重复释放拒绝、位 0 保留不变量） | 机制 | 存量 | 08 §1.6/§2.3/§2.4 | super.c:29-156 | 08 |
| K-067 | 干净位与降级（上次关机体面吗；MFSFLAG_CLEAN） | 约束不变量 | 存量 | 08 §1.7 | mount.c:13-21; clean.h | 08 |
| K-068 | rw_super 读写与 write_super 只读守卫 | 机制 | 存量 | 08 §2.6/§2.8 | super.c:173-236,360-364 | 08 |
| K-069 | 内存 inode=磁盘 64B 格式+工作状态字段（i_count/dirty/search/mount/updated） | 数据结构 | 存量 | 09 §1.1/§2.1/§2.2 | inode.h:20-50; type.h:8-17 | 09 |
| K-070 | 128 桶哈希 + 512 槽 + 空闲队列（addhash/unhash/init_inode_cache） | 数据结构 | 存量 | 09 §1.2/§2.4/§2.5 | inode.c:70-112 | 09 |
| K-071 | get_inode 三条路（命中/冷命中/缺席）与 find_inode 纯找 | 机制 | 存量 | 09 §1.4/§2.6/§2.7 | inode.c:118-200 | 09 |
| K-072 | fs_putnode 批量递减 count-1（Rust put_count，超量按协议违反拒绝） | 机制 | 存量(Fix #3) | 09 §2.3/§4.2 | inode.c:38-64; inode.rs put_count | 09 |
| K-073 | 分配/释放两步舞 alloc_inode/free_inode/wipe_inode | 机制 | 存量 | 09 §1.5/§2.9~2.11 | inode.c:252-343 | 09 |
| K-074 | 懒时间 mfs 侧（update_times stamp + 结清；只读挂载跳过） | 机制 | 存量 | 09 §1.6/§2.12 | inode.c:349-370 | 引用 K-051，仅记 mfs 增量 |
| K-075 | rw_inode/new_icopy 磁盘转换（d2_inode 六十四字节；只读护栏下沉 write_to_disk） | 机制 | 存量(P2-9) | 09 §2.13 | inode.c:375-449; inode.rs | 09 |
| K-076 | 无主 inode 清算：ReclaimZones 上报 + 执行体三出口（unlink/remove_dir/put_node） | 机制 | 存量(Fix #4) | 09 §3.2 + 13/15 | inode.c:206-246; todo V1-P1-4 | 09（上报）/15（执行） |
| K-077 | 挂载七步纵贯（记设备→读身份证→脏盘降级→认块大小→数已用区→领根→脏标落盘） | 机制 | 存量 | 10 §1.1/§2.1 | mount.c:10-98 | 10 |
| K-078 | 卸载六步（busy 检查→sync→clean 标记→invalidate→表释放→设备归还） | 机制 | 存量 | 10 §1.3/§2.3 | mount.c:134-172 | 10 |
| K-079 | 挂载点检查三问（门口挂牌） | 机制 | 存量 | 10 §1.4/§2.2 | mount.c:104-128 | 10 |
| K-080 | "一个值拥有全部"挂载形态优选（三方案比较） | 架构演进 | 存量 | 10 §1.5/§3.1 | mount.rs MountedFs/Parts | 10 |
| K-081 | 64B 定长目录项花名册（ino==0 空坑；名 60B；全系统唯一截断点） | 数据结构 | 存量 | 11 §1.1/§2.1 | mfsdir.h:13-18 | 11 |
| K-082 | 四种查法一 walk（LOOK_UP/ENTER/DELETE/IS_EMPTY）与 ENTER hint | 机制 | 存量 | 11 §1.2/§1.3/§2.4 | path.c:92-240 | 11 |
| K-083 | advance 与 fs_lookup 单步三段式 | 机制 | 存量 | 11 §1.5/§2.2/§2.3 | path.c:16-86 | 11 |
| K-084 | 只读挂载写操作全拒 | 约束不变量 | 存量 | 11 §1.4 | path.c 写臂 | 11 |
| K-085 | 新节点四拍（查空/分配/落盘/进入）+ 逐拍回滚（孤儿节点优于悬空名字） | 机制 | 存量 | 12 §1.1/§1.2/§2.5 | open.c:192-257 | 12 |
| K-086 | mkdir 点点舞（`.`/`..` + nlinks=2）与链接数上限 | 机制 | 存量 | 12 §1.3/§2.3 | open.c:77-122 | 12 |
| K-087 | 符号链接建立：目标住单间（直写缓存块） | 机制 | 存量 | 12 §1.4/§2.4 | open.c:128-187 | 12 |
| K-088 | mknod 设备节点与 fs_seek 寻位标记 | 机制 | 存量 | 12 §1.5/§2.2/§2.6 | open.c:56-71,263-270 | 12 |
| K-089 | 硬链接：计数+1，两名字同物 | 机制 | 存量 | 13 §1.1/§2.1 | link.c:32-97 | 13 |
| K-090 | unlink/rmdir 共用路径与判断条件差异 | 机制 | 存量 | 13 §1.2/§2.2~2.5 | link.c:103-249 | 13 |
| K-091 | rmdir 三步记账（父名-1、子`.`-1、父`..`-1 → 子归 NO_LINK 触发释放） | 约束不变量 | 存量(Fix #5) | 13 §2.4 | link.c:205-211; link.rs remove_directory | 13 |
| K-092 | rename 决策树（SAME 收敛、superdir 环走查、同目录先删后进、跨父 `..` 改链+父 nlinks+1） | 机制 | 存量(Fix #8) | 13 §1.3/§2.6 | link.c:255-422 | 13 |
| K-093 | 截断判断与执行分离：释放计划纯计算（truncate_inode/freesp_inode/nextblock/zerozone_*） | 机制 | 存量 | 13 §1.5/§2.7/§2.8 + 15 §1.4 | link.c:428-637 | 13（判断）/15（执行） |
| K-094 | 块号翻译：直接 7 + 单重 1 + 双重 1（read_map/rd_indir/get_block_map） | 机制 | 存量 | 14 §1.1/§2.3/§2.4 | read.c:210-325 | 14 |
| K-095 | 空洞读零（未映射区返回零不分配） | 机制 | 存量 | 14 §1.2 | read.c:236-260 区 | 14 |
| K-096 | 分块循环 rw_chunk（每次一块内一段；写臂 NO_READ 免读优化在 15） | 机制 | 存量 | 14 §1.3/§2.2 | read.c:117-204 | 14 |
| K-097 | 缓存窥视 FSC_PEEK（只查有无、不清零不读盘；peek=直通+能力位） | 机制 | 存量(Fix #9) | 14 §1.4/§2.2 | read.c:156-159,89-111; table.c:20 | 14 |
| K-098 | 文件级预读 rahead（EOF 截断、间接块加窗、洞穿行） | 机制 | 存量(Fix #17) | 14 §1.5/§2.5 | read.c:341-435 | 14 |
| K-099 | 目录内容枚举 fs_getdents（位置分批、IFTODT 类型、冷目录走盘） | 机制 | 存量(Fix #16) | 14 §1.6/§2.6 | read.c:437-556 | 14 |
| K-100 | 目录镜像桥 load/store_dir_blocks（镜像↔缓存，追加块经 alloc_zone+write_map） | 机制 | 存量(Fix #1 新增实现) | 14 §4.4 | dir_io.rs; todo V1-P1-5 | 14 |
| K-101 | 写映射 write_map（存储/释放双臂：WMAP_FREE 先释放旧区再清槽） | 机制 | 存量 | 15 §1.1/§2.1 | write.c:28-185 | 15 |
| K-102 | 间接块生长与回收（wr_indir/empty_indir 逐级建拆） | 机制 | 存量 | 15 §2.2 | write.c:191-227 | 15 |
| K-103 | new_block（分配+清零；间接块内容校验加固项） | 机制 | 存量 | 15 §1.2/§2.4 | write.c:254-305; read.rs:148-152 | 15 |
| K-104 | clear_zone/zero_block 与条件编译级 no-op | 机制 | 存量 | 15 §1.5/§2.3 | write.c:233-248,311-318 | 15 |
| K-105 | 文件写入四层（顶层只读/超长拒绝→分块循环→块保障→映射写入）+ 整块免读 | 机制 | 存量(Fix #14) | 15 §1.3/§2.6 | read.c:48-61,89-111; write.rs ensure_block | 15 |
| K-106 | 元数据六操作：chmod 低 12 位/chown 收回特权位+只读不对称/utime 三选择器 | 机制 | 存量 | 16 §1.1~1.3/§2.1/§2.2/§2.6 | protect.c:9-58; time.c:11-48 | 16 |
| K-107 | stat 先结算时间再逐字段上报 + estimate_blocks 块用量估计 | 机制 | 存量 | 16 §1.4/§2.3/§2.4 | stadir.c:11-76 | 16 |
| K-108 | statvfs：总数/空闲现算/名字长度上限 | 机制 | 存量 | 16 §1.5/§2.5 | stadir.c:82-104 | 16 |
| K-109 | conv2/conv4 字节序转换=历史兼容残留 | 约束不变量 | 存量 | 16 §1.6/§2.7 | utility.c:10-36 | 16 |
| K-110 | typed Stat/StatVfs（88B/80B）收敛到 minix-types（消灭各服私有布局） | 架构演进 | 存量(Fix #12) | 16 §3.1 | todo V1-P2-5; minix-types | 16 |
| K-111 | 同步顺序：先脏 inode 后脏块（顺序至关重要） | 约束不变量 | 存量 | 17 §1.1/§2.1 | misc.c:8-23 | 17 |
| K-112 | 空闲统计 count_free_bits（逐块计位不越界；Rust 收敛单点权威从零计满图） | 机制 | 存量(Fix #18) | 17 §1.2/§2.2 | stats.c:13-89; superblock.rs count_clear_bits_in_image | 17 |
| K-113 | 脏标记红线：只读盘出脏=守卫报错 | 约束不变量 | 存量 | 17 §1.3/§2.3 | clean.h:1-14 | 17 |
| K-114 | 常量目录与全局状态（const.h 六十八行权威位置、glo.h 五名字去向） | 数据结构 | 存量 | 17 §1.4/§1.5/§2.4/§2.5 | const.h; glo.h:10-20 | 17（使用点索引归 99） |

### 2.5 虚拟树与变体（18~24）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主讲述点 |
|------|------|------|------|----------|------|----------|
| K-115 | 为什么抽框架：三服务同构（树/钩子/分发） | 概念 | 存量 | 18 §1.1 | libvtreefs 依赖图 | 18 |
| K-116 | 节点四组字段（身份/元数据/树位置/哈希链）+ index_t | 数据结构 | 存量 | 18 §1.2/§2.2 | vtreefs/inode.h:30-626 | 18 |
| K-117 | 两套哈希（按名/按号）与 sdbm | 数据结构 | 存量 | 18 §1.3/§2.9 | sdbm.c:21-30 | 18 |
| K-118 | 删除两阶段（先摘牌后拆房）与批量引用 | 机制 | 存量 | 18 §1.4/§1.5 | inode.c putnode 区 | 18 |
| K-119 | 查找四步（点/点点/钩子/哈希）与枚举三段 | 机制 | 存量 | 18 §1.6/§1.7/§2.4/§2.8 | path.c:8-59; file.c getdents | 18 |
| K-120 | 文件读写循环：框架管循环、钩子填内容（buf 暂存窗口） | 机制 | 存量 | 18 §1.8/§2.8 | file.c:14-295; extra.c | 18 |
| K-121 | 挂载：拒绝当根+钩子开场；状态语义（链接数是布尔） | 机制 | 存量 | 18 §1.9/§1.10/§2.6/§2.7 | mount.c:8-56; stadir.c | 18 |
| K-122 | run_vtreefs 启动链与 vtreefs_table（fsdriver 复用） | 机制 | 存量 | 18 §2.1/§2.3 | vtreefs.c:52-110; table.c:6-23 | 18 |
| K-123 | TreeServer 接框架（impl FsDriver，Fix #21 补） | 架构演进 | 存量(todo) | 18 §4 | minix-vtreefs/src/driver.rs | 18 |
| K-124 | procfs 静态树构造（root_files+版本兼容 init_tree） | 机制 | 存量 | 19 §1/§2.1 | procfs/main.c:77-93; root.c | 19 |
| K-125 | pid 槽位算术（NR_PROCS+NR_TASKS 索引）与两遍刷新 | 机制 | 存量 | 19 §1/§2.3 | pid.c; tree.c | 19 |
| K-126 | 内容生成族：uptime/meminfo/loadavg/kinfo/mount/dmap/psinfo | 机制 | 存量 | 19 §1/§2.2 | root.c; util.c:8-65 | 19 |
| K-127 | 内容面扩展（cpuinfo/PCI/IPC 向量/cmdline/environ 渲染；真实数据源归轨道） | 机制 | 存量(Fix #23) | 19 §4.6 | content.rs; service.rs | 19 |
| K-128 | 服务子目录策略表（service.c RS 对话，只讲机制） | 机制 | 存量 | 19 §2.5 | service.c:1-348 | 19 |
| K-129 | 暂存窗口语义（buf.c 输出缓冲）与负载算法 | 数据结构 | 存量 | 19 §2.7/§2.8 | buf.c:17-125; util.c | 19 |
| K-130 | ptyfs 固定表 32 节点位图预分配 | 数据结构 | 存量 | 20 §1.1/§2.1 | ptyfs.c 表; node.c:20-83 | 20 |
| K-131 | 数字名字双向转换（一一对应、空串拒绝） | 机制 | 存量 | 20 §1.2/§2.2 | ptyfs.c:54-100 | 20 |
| K-132 | 查找只认根与点、枚举走满格、stat 链接数语义 | 机制 | 存量 | 20 §1.3~1.5 | ptyfs.c:27-293 | 20 |
| K-133 | 控制消息 PTYFS_SET/DEL + ds 标签鉴权（先验后办再回） | 接口协议 | 存量 | 20 §1.6/§2.5 | ptyfs.c:299-372; com.h:899-901 | 20 |
| K-134 | 直连 fsdriver（不用 vtreefs）+ 拒根挂载 + fdr_other 旁路消费 | 机制 | 存量 | 20 §1.7/§2.6 | ptyfs.c:377-434; fsdriver.c:26 | 20 |
| K-135 | ext2 块组自治布局（组 descriptor/位图/inode 表自带） | 数据结构 | 存量 | 21 §1.1 | ext2/super.h; super.c:69-459 | 21 |
| K-136 | 四组放置策略：目录散开（orlov 三变体）、文件跟父、窗口与保留、预分配丢弃 | 机制 | 存量 | 21 §1.2/§1.3/§2.5/§2.6 | balloc.c:1-362; ialloc.c:1-476 | 21 |
| K-137 | ext2 超级块三道门（魔数/尺寸/特性集门）与挂载六开关 | 约束不变量 | 存量 | 21 §1.4/§1.5/§2.3/§2.4 | mount.c:17-221; super.c | 21 |
| K-138 | 变长目录项（rec_len 自带、删除合并不留洞、解码先验长度） | 数据结构 | 存量 | 22 §1.1/§2.1 | ext2/dir 段 path.c:20-314 | 22 |
| K-139 | 128B inode 记录 + 15 指针 + 三级间接分解（12+1+1+1，立方封顶） | 数据结构 | 存量 | 22 §1.2/§1.3/§2.5 | ext2/type.h; inode.c; mapping.rs | 22 |
| K-140 | 全盘小端总开关（le_CPU 断言）与语义引用形状差异 | 约束不变量 | 存量 | 22 §1.4/§1.5 | ext2/super.h 断言 | 22 |
| K-141 | ISO 卷发现扫描（固定偏移起步、≤20 扇区、主描述符+终止符齐备） | 机制 | 存量 | 23 §1.1/§1.2/§2.2 | isofs/super.c:24-121 | 23 |
| K-142 | 目录记录双端数字段（位置/长度各存两遍读小端）与名字解码 | 数据结构 | 存量 | 23 §1.3/§2.3 | isofs/inode.c; utility.c | 23 |
| K-143 | 区间跑道 extent（文件块连续段走查累减） | 数据结构 | 存量 | 23 §1.4 | isofs/inode.c 区段 | 23 |
| K-144 | Rock Ridge 尾巴（NM 长名/SL 链接组装/norock 可关） | 机制 | 存量 | 23 §1.5/§2.6 | susp.c; susp_rock_ridge.c | 23 |
| K-145 | 只读子集回调表（两处关掉+读钳制+date7 转换） | 机制 | 存量 | 23 §1.6/§1.7/§2.4/§2.5 | isofs/table.c:1-32; read.c | 23 |
| K-146 | SFFS 操作表：15 个宿主 round trip 窄接口 | 接口协议 | 存量 | 24 §1.1/§2.7 | sffs/proto.h 操作表 | 24 |
| K-147 | 选项四件套（身份/掩码/前缀/大小写折叠；掩码减法保留） | 机制 | 存量 | 24 §1.2/§1.4 | sffs/name.c:1-52; params | 24 |
| K-148 | 路径拼接（前缀+链，右到左等价实现；同串异写） | 机制 | 存量 | 24 §1.3/§2.5 | sffs/path.c:1-108 | 24 |
| K-149 | 验鲜三判决（有罪推定：确定在/确定不在/待查） | 机制 | 存量 | 24 §1.5/§2.4 | sffs/verify.c:1-118 | 24 |
| K-150 | 句柄懒开静关 + inode 缓存 | 机制 | 存量 | 24 §1.6 | sffs/handle.c:1-77; inode.c | 24 |
| K-151 | 查找三步（验父/分点/验鲜建表）+ 挂载拒根验根六十四位 | 机制 | 存量 | 24 §1.7/§1.8/§2.2/§2.3 | sffs/lookup.c:1-150; mount.c:1-89 | 24 |
| K-152 | 两桥装配（备选项→两初始化→进循环）与 sffs_init 链 | 机制 | 存量 | 24 §1.9/§2.1/§2.8 | libsffs/main.c:17,58; vbfs.c:98-132; hgfs.c:64-99 | 24 |

### 2.6 全局/边界/非 C（99、00 与新增项）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 主讲述点 |
|------|------|------|------|----------|------|----------|
| K-153 | FS 子系统=8 server+4 框架库全景与语义主线图 | 概念 | 存量(骨架) | 00 | plan §1.1; §1 真序表 | 00 |
| K-154 | boot 挂载因果链（do_init_root→mount_pfs→mount_fs MFS） | 机制 | 存量(骨架) | 00 | servers/vfs/main.c:501-524 | 00 |
| K-155 | FS server 构建与登记（minix3 fs/Makefile BINDIR=/service；boot_image vs RS 加载） | 工具工程 | **新增** | 无（00 未展开） | minix3/minix/fs/Makefile.inc:1-5; kernel/table.c:44-64 | 00 |
| K-156 | 镜像与内存布局：root image（bootramdisk imgrd）承载 mfs 根 | 工具工程 | **新增** | 无 | servers/vfs/main.c:517（DEV_IMGRD 参数） | 00（本 stage 半；制作归 18/19 轨道） |
| K-157 | bin 接线现状与 E-FSRUNTIME 边界（8 个 main `loop {}` 占位声明） | 约束不变量 | **新增** | 06/07 头部分散声明 | todo.md §2.5; edge_todo.md:897 | 00 |
| K-158 | 跨 stage 边界契约四条（E-FSRUNTIME/E-FSBDEV/E-FSVMCACHE/E-FSCMDS） | 约束不变量 | **新增** | todo §5 有指针、00 无 | edge_todo.md:897,914,931,948 | 00 |
| K-159 | REQ 常量权威表（1..33+FS_BASE 0xA00+NREQS 34）与 IS_FS_RQ 死宏 | 接口协议 | 存量(骨架) | 99 列了要点未成文 | vfsif.h:41-77; com.h:589; todo V1-P3-7 | 99 |
| K-160 | 消息布局权威（m_vfs_fs_*/m_fs_vfs_*、fsdriver_node/data/dentry、vfs_ucred_t） | 接口协议 | 存量(骨架) | 99 | minix/ipc.h; fsdriver.h | 99 |
| K-161 | 错误码映射表（errno + EENTERMOUNT 族 -301..-303） | 接口协议 | 存量(骨架) | 99 | vfsif.h:26-28 | 99 |
| K-162 | 服务号常量（MFS/PFS/PROC/PTY endpoint，com.h） | 接口协议 | 存量(骨架) | 99 | com.h | 99 |
| K-163 | 测试矩阵汇总（338 测试分布、单元 vs 装配 vs 真机空位） | 测试 | **新增** | 各篇 §5 分散 | todo §7/§9; 各篇 §5 计数 | 00（汇总索引）；单篇明细归各篇 §5 |
| K-164 | 文档-代码账本同步纪律（EntryStatus 31/0 终态；防"空壳声明"残留） | 工具工程 | **新增** | 无集中处 | table.rs:40; todo §2.1 | 00 |
| K-165 | 变体 diff 原则（参考实现语义只写一次，变体只写差异矩阵） | 约束不变量 | 存量 | plan §3.6 有、正文无 | plan §3.6（线索）+ 21/22/23/24 头声明 | 00 |
| K-166 | 汇编入口/陷阱进入、链接脚本（本 stage 无此类制品） | — | 新增-排除 | — | os/fs/*/main.rs 无 asm；链接归 14-stage-runtime | 不立篇（见 §7） |

**统计摘要**：166 条（存量 158、新增 8：K-155~158、163~166）。按层分布：框架 45、pfs 10、
mfs 59、虚拟树/变体 38、全局/边界 14。按类型分布：机制 78、数据结构 22、概念 10、
接口协议 18、约束不变量 22、架构演进 9、工具工程 5、测试 2。

### 2.7 重复与主讲述点标记（池内合并项）

| 概念 | 出现位置 | 主讲述点 | 其余处理 |
|------|----------|----------|----------|
| 懒时间 | 06 §1.5、09 §1.6、16 §1.3/1.4 | 06（K-051） | 09 记 mfs 增量（只读跳过/结清时机），16 记修改侧语义 |
| 预读/预取三层 | 04 §2.8（缓存级）、05 §1.4（bio 级）、14 §1.5（文件级） | 各层本层讲透 | 每处开头一句话声明"这是第几层的预读，另两层见 0X" |
| 挂载门禁 vs 挂载流程 | 01 §1.4（框架门禁）、02 挂载组、10 §1.1（fs_mount）、18/20/24（拒根变体） | 01（门禁语义）/10（磁盘流程） | 变体只写差异一句话 |
| 窥视 peek | 02 §1.5（适配器仿真）、04 §2.8（缓存 peek 拿法）、05 §1.1（窥视=缺席读）、14 §1.4（FSC_PEEK） | 各层概念首现处 | 14 声明与 02 协商关系 |
| 目录项两种形态 | 03 §1.4（getdents 输出编码）、11 §1.1（磁盘目录项） | 03（协议侧）/11（磁盘侧） | 双向引用，已有，保持 |
| 符号链接 | 03（框架漫步重写）、12（建立）、13（读取+rename 中语义）、18（钩子）、22（差异） | 03/12/13 分层 | 边界已在"不讲什么"声明 |
| 常量权威 | 01 §2.5/2.6、17 §1.4/2.5、99 | 99（表）；01/17（语义与使用） | 99 建成后各篇数值改引用（见 §3.3 重复主题表） |
| 单线程假设 | 01 §3.7、04 前置 | 01 | 04 一句话回指 |

---

## 3. 覆盖审计

### 3.1 主题全集与来源（四路）

| 路 | 来源 | 规模与核对方式 | 结果 |
|----|------|----------------|------|
| 1. C 源码符号 | `minix3/minix/minix/fs/`（8 server 目录）+ `lib/libfsdriver`（6 文件）+ `lib/libminixfs`（cache.c/bio.c 等）+ `lib/libvtreefs` + `lib/libsffs` + 头文件（vfsif.h/com.h/fsdriver.h/libminixfs.h/mfsdir.h/super.h/inode.h/glo.h） | plan §5.1/§5.2 给出 92 个 C 文件 → 逐文件对照现有文档映射表再核对（`ls minix3/minix/minix/fs/*/`），未发现映射表之外的文件 | 全部落入 K-001~K-152 或 plan §5.4 排除表（VFS 服务侧、工具链程序、测试程序） |
| 2. 操作系统通用概念 | 不依赖具体代码：缓存淘汰（LRU）、位图分配器、inode 缓存与引用计数、间接块映射、定长/变长目录项、只读降级、有罪推定式缓存验鲜、懒初始化（时间戳）、单线程事件循环服务器 | 每条在第 2 节池内有对应条目（K-029/K-066/K-069/K-094/K-081/K-138/K-149/K-051/K-001） | 无悬空概念 |
| 3. 非 C 制品 | `minix3/minix/fs/Makefile.inc`（BINDIR=/service）、`kernel/table.c:44-64`（boot_image）、`servers/vfs/main.c:501-524`（imgrd 挂载）、消息线格式（ipc.h 系 `m_vfs_fs_*`）、`os/qemu-tests/`、`os/xtask/` | 逐项检查（§0.2 清单）；qemu-tests 内无 FS 域专项真实机用例（`ls os/qemu-tests/` 核对） | 发现 6 个未承载主题 → §3.2 缺口 G-03~G-08 |
| 4. 阶段边界契约 | `edge_todo.md`：E-FSRUNTIME(:897)、E-FSBDEV(:914)、E-FSVMCACHE(:931)、E-FSCMDS(:948)；`00-master-plan/README.md` 阶段划分 | 逐条与本 stage 声明边界对账 | 四条都只有 todo 指针、无正文承载 → 缺口 G-06 |

### 3.2 覆盖缺口表

每条缺口给出：证据、裁决、以及落实为新增知识点的入池编号（K-155~K-166，见 §2.6）。

| # | 缺口主题 | 证据（为什么确认没讲） | 裁决 | 落实 |
|---|----------|------------------------|------|------|
| G-01 | 00 总览未完成：只有 21 行骨架（`wc -l 00-fs-overview.md` = 21，头部标 `状态: pending`） | 文件实测；核心点全部是"素材指针"（plan §1.1/§2/§3.6），无正文 | 重建 00（内容级扩建，编号不变） | K-153/K-154 扩建 + 吸收 G-03~G-09 全部 |
| G-02 | 99 全局概念未完成：21 行骨架，常量表/消息布局/错误码表只列要点未成文 | 文件实测 `状态: pending`；REQ 表、TRNS、EENTERMOUNT 族数值散落在 01/02/03 正文重复出现 | 重建 99（成表 + 使用点索引） | K-159~K-162 |
| G-03 | FS server 的构建与登记：谁把 pfs/mfs 放进 boot image、`BINDIR=/service` 装配规则 | 全 stage grep 无"Makefile/boot_image 装配"主题正文；只有本蓝图 §1 T1 证据 | 新建小节入 00 | K-155（锚点 `minix3/minix/fs/Makefile.inc`、`kernel/table.c:44-64`） |
| G-04 | boot 镜像载体：根文件系统镜像 `fs_imgrd` 挂在 DEV_IMGRD 上这一事实 | `servers/vfs/main.c:517` 参数 `"bootramdisk", …, "fs_imgrd"` 在任何文档正文无解释 | 00 讲本 stage 半（消费端）；镜像制作归 18/19 轨道 | K-156 |
| G-05 | bin 接线现状：8 个 Rust server 的 `main.rs` 尚为占位、真实装配在库层 | todo.md §2.5 有声明；06/07 头部零散提及，无集中交代 | 00 集中声明 + 指向 E-FSRUNTIME | K-157 |
| G-06 | 跨 stage 边界契约四条（E-FSRUNTIME/E-FSBDEV/E-FSVMCACHE/E-FSCMDS）无正文归属 | edge_todo.md:897,914,931,948；本 stage 文档只在各自"不讲什么"零散指涉 | 00 建"边界契约"一节统一挂账 | K-158 |
| G-07 | 测试矩阵总览：338 个测试的分布（各篇 §5 分散计数）、单元/装配/真机三层空位 | 各篇 §5 只报本篇计数（如 07 §5.6"截至 2026-09-17"）；`os/qemu-tests/` 无 FS 专项用例 | 00 建汇总索引；明细留各篇 | K-163 |
| G-08 | 文档-代码账本同步纪律：接线表 EntryStatus 31/0 终态如何维持、防空壳声明残留 | todo.md §2.1/§3.1 有过程记录，正文无处沉淀为规则 | 00 一节 | K-164 |
| G-09 | 变体 diff 原则正文化：21~24 的"只写差异"写法约束目前只在 plan §3.6 | 正文 grep"变体"仅出现在各篇头部声明 | 00 阅读指南一节写明 | K-165 |
| G-10 | 链接脚本 / 汇编入口 / 陷阱进入 | 实测本 stage 无此类制品：`os/fs/*/src/` 无 `asm!`/`global_asm!`（grep 验证），入口是普通 `fn main`；运行时启动与链接归 14-stage-runtime | 不立篇、不入池正文，00 边界一句话 | K-166（排除项） |
| G-11 | 真机/QEMU 端到端 FS 验证空位 | `os/qemu-tests/` 目录核对无 FS 用例；端到端集成按 master-plan 归 19-stage-integration | 判定属于其它 stage，00 测试矩阵标注空位 | 并入 K-163 的"空位"列 |

### 3.3 重复主题表

新目录中的主讲述点裁决（与 §2.7 一致，此处补充"其余篇目的具体改写动作"）：

| 主题 | 现状重复位置 | 主讲述点（唯一完整讲述） | 其余改写动作 |
|------|--------------|--------------------------|--------------|
| 懒时间 | 06 §1.5、09 §1.6、16 §1.3/1.4 | **06**（K-051） | 09 保留 mfs 增量（只读跳过、结清时机），首句改"机制已在第 06 篇讲透，这里只讲 mfs 的三个差异"；16 只讲修改侧（utime 三选择器）语义 |
| 预读/预取 | 04 §2.8（缓存级）、05 §1.4（bio 级）、14 §1.5（文件级） | 各层在本层讲透（三层物理上不可合并：作用对象不同） | 每处首句声明"这是第 N 层预读，另两层见 0X/0Y"，并给一张三层对照小表（放 14，因为文件级最易与前两层混淆） |
| 窥视 peek | 02 §1.5（适配器仿真）、04 §1.4（缓存拿法）、05 §1.1（bio 缺席读）、14 §1.4（FSC_PEEK） | 各层概念首现处（四次首现分别属于四个抽象层，非真重复） | 99 增加一行"peek 在四层中的含义"索引；02/14 互相声明协商关系（能力位 RES_HASPEEK） |
| 挂载 | 01 §1.4（框架门禁）、02 §2.1（挂载组适配器）、10 §1.1（mfs 七步）、18/20/24（拒根变体） | **01**（门禁语义）+ **10**（磁盘流程） | 变体篇各只留一句"与 mfs 的差异：拒绝当根"（K-121/K-134/K-151 现状已接近，B 相复核） |
| 目录项两种形态 | 03 §1.4（协议侧编码）、11 §1.1（磁盘侧定长槽） | 03 / 11 分层 | 双向引用已存在，保持 |
| 符号链接 | 03（框架漫步）、12（建立）、13（rename 中语义）、18（钩子）、22（ext2 差异） | 03/12/13 分层 | 不改；22/18 已是差异句式 |
| 常量数值 | 01 §1.2/§2.5、03 §2.5、17 §1.4/§2.5、99 | **99**（成表）；01/03/17 讲语义与使用 | 99 建成后，各篇正文的具体数值保留"权威出处指向 99 + 源码锚点"双标；B 相不强制回改所有数值（避免大面积扰动），新增表格以 99 为准 |
| 单线程假设 | 01 §3.7、04 前置、各 server 篇开头 | **01**（K-010） | 其余全部一句话回指"第 01 篇 §3.7" |
| 测试计数 | 各篇 §5 + 00 | **00 汇总索引**（K-163）；单篇明细留各篇 §5 | 00 只列表不重述测试内容，防两处维护同一清单 |

### 3.4 越界主题表

| # | 位置 | 内容 | 判定 | 裁决 |
|---|------|------|------|------|
| O-1 | 06 §1.4/§2.6（管道读写） | "异步唤醒在 VFS 侧"——select/poll 唤醒机制 | 半越界：VFS 内部机制归 05-stage-vfs | 06 止步于"pfs 把数据放好后回复，唤醒由 VFS 完成"一句；不展开 VFS 队列 |
| O-2 | 19 §2.5（service.c 策略表） | procfs `/service/` 子目录与 RS 的对话细节 | 半越界：RS 协议归 03-stage-rs | 保持头部声明的"只讲机制"：讲清"从 RS 查询服务列表渲染成目录"，RS 消息格式指向 03-stage-rs（K-128） |
| O-3 | 04 §2.3（容量启发式） | `fs_bufs_heuristic` 需要系统总内存数 | 越界风险点：内存查询走 RS | 只讲启发式公式与下限（cache.c:73-162），"内存数从哪来"一句话指向 E-FSBDEV/RS 轨道（K-035 边界注） |
| O-4 | 05 §3.5（RamDisk）与 bdev_bridge | 真实块驱动桥接 | 已声明移交（E-FSBDEV → 16-stage-drivers） | 保持：讲接口与线格式，不讲驱动实现（K-044/K-045 现状合规） |
| O-5 | 04 §1.5/§5.5（VM 二级缓存） | VM 侧页缓存管理 | 已声明移交（E-FSVMCACHE → 02-stage-vm） | 保持 D5 裁决：通道 FS 端在 04 讲透，VM 端不展开（K-036） |
| O-6 | 01 §1.7（Linux/Redox 对照） | 对照性内容 | 不越界（支线），但需标注 | 保留并显式标"支线对照，可跳读"（K-012） |

### 3.5 锚点系统性错误与文档-代码漂移（本蓝图最重要的内容性缺陷）

**(a) 47 处"工具生成"锚点，符号名系统性滞后。** 实测分布：01=16、04=1、05=8、06=14、17=1、18=1、19=4、20=1、21=1（合计 47）。两处抽验确认同一模式——**行号大致正确、符号名滞后约一个函数**：

| 实例 | 文档写法 | 实测 |
|------|----------|------|
| 01 §2.3 | `fsdriver_process（L64，工具生成）` 处实际讲 `fsdriver_terminate` | `fsdriver_terminate` 在 fsdriver.c:64-71；`fsdriver_process` 在 L17-62 |
| 06 §2.5 | `pfs_putnode（L217，工具生成）` | pfs.c:217 实测是 `pfs_read` 签名段（`sed -n '215,220p'`） |

裁决：B 相重建时**逐条替换为人工核对锚点**（本蓝图 §1/§2 已给出经核对的框架层锚点样例；server 层逐函数锚点在 B 相用 `grep -n "^static.*函数名"` 现场复核）。不允许保留"工具生成"字样或沿用未核对符号名。这与项目已有记忆一致：工具生成锚点必须复核符号名。

**(b) 文档-代码漂移（V1 架构评审修复后的同步）。** todo.md 记录 2026-09-17 完成的 Fix #1~#24 改变了 Rust 侧终态，涉及正文的有：02（Fix #7 MountInput 标签）、03（Fix #10 凭据校验位置）、07（Fix #1 mfs 服务装配 server.rs，31/0 账本终态）、09（Fix #3 putnode 批量递减）、13（Fix #5 rmdir 三记账、Fix #8 rename 决策树）、14（Fix #9 窥视直通、Fix #16 getdents、Fix #17 rahead、dir 桥 load/store）、16（Fix #12 typed Stat 收敛 minix-types）、17（Fix #18 空闲统计单点权威）、18（Fix #21 TreeServer impl FsDriver）、19（Fix #23 内容面扩展）。池中相应条目的"现有位置"列已标注（存量(Fix #N)）。B 相每篇动工前按 todo.md 对应 Fix 项 diff 复核，不凭本蓝图转述。

**(c) 00 骨架内锚点精度。** 00 现有文本写 `do_init_root（servers/vfs/main.c:491）`；实测 491 行是 `worker_start(…, do_init_root, …)` 调用处，函数体在 :501-524。重建 00 时使用 §1 T4 的锚点。

### 3.6 非 C 主题逐项回答（固定十项清单）

| 项 | 在哪里讲 / 为什么不在本 stage |
|----|-------------------------------|
| 链接与加载 | 不在本 stage：FS server 是普通用户态 ELF，链接脚本与加载器归 14-stage-runtime（00 边界一句话，K-166 排除项） |
| 镜像与内存布局 | 00 新建小节：bootramdisk/`fs_imgrd` → DEV_IMGRD → mfs 根（K-156，锚点 main.c:517）；镜像制作工具链归 18/19 轨道 |
| 汇编入口与陷阱进入 | 本 stage 无此类制品（grep 验证 `os/fs/*/src` 无 `asm!`）；不立篇 |
| 启动装配 | 双层：boot 装配（登记+挂载）在 00（K-155/K-154）；server 进程内初装在 07（K-056~K-058）与 06/18/24 各篇入口节 |
| 构建与工具链 | 00 一节：minix3 `fs/Makefile.inc` BINDIR 规则 + Rust workspace `os/fs/*` crate 组织（K-155） |
| 跨模块接口与线格式 | 99 成表（K-159~K-162：REQ 表、消息布局、错误码、服务号）；语义讲述在 01/02/03 |
| 错误路径 | 各篇事实底线内含（如 K-005 EINVAL 门禁、K-063 魔数拒绝、K-112 统计越界防护）；映射表在 99（K-161） |
| 关闭与退出 | 01（框架 terminate/循环条件 K-008）+ 07（SIGTERM→sync→terminate K-057）+ 10（卸载六步 K-078）+ 17（sync 顺序 K-111） |
| 并发与同步 | 01 §3.7 单线程假设（K-010）+ 04 无锁缓存推论（K-037）；跨 CPU 问题不存在于本 stage 模型 |
| 测试基建 | 00 测试矩阵（K-163）+ 各篇 §5；真机端到端空位显式标注归 19-stage-integration（G-11） |

---

## 4. 新目录

### 4.1 总体裁决：保留编号，内容级重建

现目录 00→24→99 的骨架经 §1 真序对账**因果成立**：框架（01~05）前置于 server（变体 diff 原则，D1）、boot 因果（06 pfs 最先挂载）、数据结构先于消费者（08/09 先于 10，D2 已在序差表记录并给出回指补偿）。因此新目录**不重排、不拆分、不合并、不归档、不重编号**；重建对象是内容：00/99 两篇从零扩建，其余 24 篇按契约重写正文（叙述结构推倒重来，编号与文件名不变），并清除 §3.5 的三类缺陷。

为什么不重编号（成本证据）：全量重编号需迁移 stage 内 127 处路径引用 + 141 处"第 NN 篇"引用 + 约 15 处外部文档引用 + 13 处 Rust 源码注释映射（`os/libs/minix-fs/src/lib.rs:1-30`），合计约 296 处；而真序对账未发现任何"必须整篇移动"的违例。历史上项目正是在"重编号断链风险大于收益"的裁决下止步（R-prompt §二 所述教训）。保留编号使迁移成本降为**纯锚点文本修复 47 处 + 契约内内容重写**，收益/成本比明确占优。此裁决列为 §9 待用户确认项 U-1。

### 4.2 新篇章总表（26 篇，编号不变）

| 编号 | 文件名（不变） | 一句话定位 | 分组 |
|------|----------------|-----------|------|
| 00 | 00-fs-overview | 全景导航 + boot 挂载因果 + 边界契约/构建登记/测试矩阵总账 | A 总览与框架 |
| 01 | 01-fsdriver-task | 框架心脏：消息循环、分发门禁、终止路径与 Rust 类型化 | A |
| 02 | 02-fsdriver-call | 31 个请求适配器：校验→回调→装回复的翻译层 | A |
| 03 | 03-fsdriver-utility | 数据通道、目录项编码器、整路径漫步 | A |
| 04 | 04-block-cache | 块缓存：池、LRU、脏块、三级预读的缓存层、VM 二级缓存 FS 端 | A |
| 05 | 05-block-io | bio 块 IO 与驱动绑定：取书单、逐块行走、刷后失效 | A |
| 06 | 06-pfs | 最小完整 server：boot 第一个挂载的样例篇（懒时间主讲述点） | B 样例 |
| 07 | 07-mfs-init-main | mfs 启动装配：初装四步、信号、31 行接线表、服务装配终态 | C mfs 参考实现 |
| 08 | 08-mfs-super | 磁盘全景与账本：布局、魔数、位图、干净位 | C |
| 09 | 09-mfs-inode | inode 缓存：结构、哈希、借还、清算上报 | C |
| 10 | 10-mfs-mount | 挂载/卸载纵贯：一次 REQ_READSUPER 的完整纵深 | C |
| 11 | 11-mfs-path | 目录花名册：64B 定长目录项与四种查法 | C |
| 12 | 12-mfs-open | 新节点诞生：creat/mkdir/symlink/mknod 与逐拍回滚 | C |
| 13 | 13-mfs-link | 名字与链接：hardlink/unlink/rmdir/rename 记账 | C |
| 14 | 14-mfs-read | 读路径：块号翻译、空洞读零、窥视、文件级预读、getdents | C |
| 15 | 15-mfs-write | 写路径：写映射、间接块生长、整块覆写免读、截断执行 | C |
| 16 | 16-mfs-metadata | 元数据操作：chmod/chown/utime/stat/statvfs 与 typed Stat | C |
| 17 | 17-mfs-maint | 维护面：sync 顺序、空闲统计、脏标红线、常量目录 | C |
| 18 | 18-vtreefs | 虚拟树框架：节点、双哈希、两阶段删除、钩子协议 | D 虚拟树族 |
| 19 | 19-procfs | 进程观察树：静态树 + pid 槽位 + 内容生成族 | D |
| 20 | 20-ptyfs | 伪终端表：直连 fsdriver 的固定表变体 | D |
| 21 | 21-ext2-init-mount | ext2 差异（上）：块组自治、四组放置策略、三道门 | E 磁盘格式变体 |
| 22 | 22-ext2-namespace-data | ext2 差异（下）：变长目录项、128B inode、三级间接、小端 | E |
| 23 | 23-isofs | 只读光盘：卷发现、双端数字段、Rock Ridge、只读子集 | E |
| 24 | 24-vbfs-hgfs | 宿主桥：SFFS 操作表、验鲜三判决、两桥装配 | F 宿主变体 |
| 99 | 99-global-concepts | 权威附表：REQ/消息布局/错误码/服务号成表 + 四层 peek 索引 | G 附录 |

### 4.3 阅读路径与并行体组织

- **主线**（顺序读完即掌握整个子系统）：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → … → 17。
- **支线一（虚拟树族）**：18 → 19 → 20。只依赖 01~03（框架）与 10/11 的概念对照；可在读完 06 后 anytime 读。
- **支线二（磁盘变体）**：21 → 22、23。强依赖主线 07~17 的 mfs 参考实现（变体 diff 原则：差异只有相对参考实现才有意义）。
- **支线三（宿主变体）**：24。依赖 01~05 + 与 18/20 的"不用 vtreefs"对照。
- **可跳读**：01 §1.7 Linux/Redox 对照；99 全部（字典篇，按需查）；各篇 §测试小节。
- **并行体组织**（prompt §5.2）：8 个 server 是并行体，不强行排线——统一框架篇（01~05）先行，代表性成员 mfs 讲透（07~17），其余按差异表收束（06 是"最小样例"另有教学职责故前置，18~24 全为差异篇）。31 个适配器同为并行体：02 按五组编队，组内代表讲透（挂载组/读写组），其余差异收束。

---

## 5. 每篇契约

**契约的读法**：每篇七要素齐全（定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单与验收标准）。知识点编号的名称、类型、来源、现有位置、锚点**一律见 §2 池表，此处不重复誊写**——契约表只写两列增量信息：为什么归本篇、B 相动工前的现场复核点。这是对 R-prompt 契约模板的压缩，压缩不减少任何一条知识点（166 条全覆盖，见 §9 G5）。所有契约共同遵守的通用条款（写一次，不再逐篇重复）：

- **通用条款 C1**：正文禁止出现"工具生成"字样锚点；全部锚点 B 相现场 `grep -n` 复核（§3.5a）。
- **通用条款 C2**：涉及 todo.md Fix #1~#24 的段落，动工前先 diff 对应 Fix 记录（§3.5b）。
- **通用条款 C3**：重复主题按 §3.3 裁决执行"主讲述 + 一句话回指"。
- **通用条款 C4**：每篇维持七章模板（概念 / C 源码分析 / Rust 设计决策 / Rust 实现详解 / 测试 / 过渡 / 参见），但节级结构由 B 相按本篇契约重排，不保留旧节序。
- **通用条款 C5**：头部声明块（分类/源码/Rust 模块/前置/不讲什么/一句话问题）必须与契约逐字段一致。

### 00-fs-overview（重建扩建：21 行骨架 → 全文）

- 一句话定位：读者进入或离开本 stage 时，靠这一篇回答"FS 子系统有哪些部件、按什么因果顺序启动、26 篇各讲什么、哪些事明确不在本 stage 讲"。
- 讲什么：K-153（全景与主线图）、K-154（boot 挂载因果链）、K-155（构建与登记）、K-156（imgrd 镜像载体）、K-157（bin 接线现状）、K-158（边界契约四条）、K-163（测试矩阵）、K-164（账本同步纪律）、K-165（变体 diff 原则）。
- 不讲什么：一切机制细节 → 01~24；镜像制作工具链 → 18/19 轨道；真机端到端 → 19-stage-integration；链接与加载、汇编入口 → 14-stage-runtime（K-166 排除说明放这里）。
- 前置：无。
- 后置：全部 26 篇（导航唯一入口）。
- 事实底线：`kernel/table.c:44-64`（boot_image 全表）；`servers/vfs/main.c:501-524`（do_init_root，mount_pfs :510、mount_fs :517，**不用旧骨架的 :491**）；`minix3/minix/fs/Makefile.inc:1-5`（BINDIR=/service）；`edge_todo.md:897,914,931,948`；todo.md §2.1/§2.5/§3.1/§7/§9；§1 真序表全部锚点。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-153/154 | 总览职责本体 | 主线图与 §4.2 总表逐篇一致 |
  | K-155 | 登记是 boot 因果的第一环 | `cargo` workspace 侧 os/fs/* 成员清单现拉 |
  | K-156 | DEV_IMGRD 参数唯一解释点 | main.c:517 参数字符串现核 |
  | K-157 | 8 个 main.rs 占位现状需集中交代 | `grep -n "loop {}" os/fs/*/src/main.rs` 重跑 |
  | K-158 | 边界账本只有总览篇有资格挂账 | edge_todo 四条目内容现读 |
  | K-163 | 测试计数两处维护必漂移，索引唯一 | `cargo test` 各包计数当日重跑 |
  | K-164/165 | 工程纪律无机制归属 | table.rs:40 终态现核 |

- 验收标准：不看任何正文能画出"8 server + 4 库 + VFS/驱动/VM 三方"关系图；boot 因果链每步有锚点；边界契约四条各有"我方职责/对方职责"一句话；测试矩阵总数与当日 `cargo test` 一致；篇幅目标 400~600 行。

### 01-fsdriver-task

- 一句话定位：读懂 FS server 的"心跳"——一条消息从到达至回复走过的五个闸门，以及服务器如何承诺退出。
- 讲什么：K-001~K-012。
- 不讲什么：单个适配器取参细节 → 02；copyin/copyout/lookup 工具 → 03；块层 → 04/05；mfs/pfs 具体接线内容 → 06/07；SEF 启动库内部 → 14-stage-runtime（一句话）。
- 前置：无（声明的先修知识：IPC 收发概念，指向 01-stage-kernel）。
- 后置：02~05（框架其余）、06~24（每个 server 都复用本篇结论）。
- 事实底线：`fsdriver.c:17-62`（process 五步）、`:64-71`（terminate）、`:76-97`（task 循环）；`table.c:6-40`（callvec 32 槽）；`vfsif.h:20-23,26-28,41-75`；`com.h:589`；Rust：protocol.rs/driver.rs/task.rs、driver.rs:633-643（NullDriver）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-001/003/004/005/006/008 | 全部是 fsdriver.c/table.c 本体的机制 | 16 处"工具生成"锚点全替换（本篇最多） |
  | K-002/007 | 请求编号与能力协商是循环的输入契约 | 99 成表后数值双标 |
  | K-009/010/011 | 框架骨架的 Rust 类型化与测试对端 | driver.rs 现状现核 |
  | K-012 | 对照支线，标注可跳读 | — |

- 验收标准：能默写分发五步顺序并解释两处"为什么不能换序"（门禁先于查表：未挂载无表可查；事务号拆分先于减 FS_BASE）；能回答"为什么 callvec 只有 32 槽而 NREQS=34"；循环条件 `running‖mounted` 的收尾账语义讲清。

### 02-fsdriver-call

- 一句话定位：31 个翻译官的通用三步（校验→回调→装回复）与五组编队，一张消息进出服务器的海关。
- 讲什么：K-013~K-020。
- 不讲什么：三步里用到的传输工具与漫步 → 03；回调在 mfs 的实现 → 08~17；pfs 回调 → 06；REQ 数值总表 → 99。
- 前置：01。
- 后置：06/07/18/20/24（接线表篇）、99。
- 事实底线：`call.c:21-34`（校验）、`:36-41`（mount 标签）、`:294-306`（peek 仿真）；Rust call.rs 对应小节；空回调 7/25 分类对照 todo.md §2.2。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-013~K-018 | call.c 本体机制与编队 | 五组计数 5+8+9+5+4=31 与符号覆盖矩阵对账 |
  | K-019/020 | 默认值与 MountInput 标签是实现约束 | Fix #7/#12 后 call.rs 现状（C2） |

- 验收标准：给任一 REQ 编号能说出它属于哪组、适配器三步各做什么、失败时回调是否被调用；空回调三类数目与 Rust 源码一致。

### 03-fsdriver-utility

- 一句话定位：适配器的工具箱——数据怎么搬、名字怎么取、目录项怎么编码、整条路径怎么漫步。
- 讲什么：K-021~K-027。
- 不讲什么：mfs 磁盘目录项格式 → 11；挂载点跨越在 VFS 侧的后续动作 → 05-stage-vfs；符号链接的建立 → 12。
- 前置：01、02。
- 后置：11、18（漫步对照）、99。
- 事实底线：`utility.c:4-78`、`:83-105`；`dentry.c:1-99`；`lookup.c:1-112`、`:117-333`；`vfsif.h:26-28`；Rust data.rs/dentry.rs/lookup.rs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-021~K-026 | 全部是本文件机制；漫步三错误码语义在此首完 | 1~2 处锚点复核 |
  | K-027 | 凭据校验位置的 Rust 偏差记录属传输层 | Fix #10 后 lookup.rs 现状（C2） |

- 验收标准：四道关卡顺序可复述并各自给出拒绝例；漫步循环的 putnode 纪律能解释"为什么每个返回路径都要放"；符号链接 7 次上限与 ESYMLINK 的关系讲清。

### 04-block-cache

- 一句话定位：书架系统本体：池、索引、淘汰、脏块、三种拿法，以及通向 VM 的二级缓存（FS 端）。
- 讲什么：K-028~K-037。
- 不讲什么：bio 逐块行走 → 05；文件级预读 → 14；VM 端页缓存管理 → 02-stage-vm（E-FSVMCACHE）；真实块驱动 → 16-stage-drivers。
- 前置：01（单线程假设回指，C3）。
- 后置：05、07、14、15。
- 事实底线：`cache.c:38-71,73-162,164-177,252-272,298-498,512-608,723-777,782-808,840-982,987-1131,1136-1166,1192-1293,1295-1321`；`libminixfs.h` struct buf；`cache.c:443-451,1236-1239`（vmcache 通道）；Rust cache.rs/vm_cache.rs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-028~K-035 | cache.c 本体 | 8+1 处锚点替换；拿块主流程行段现核 |
  | K-036 | 二级缓存与拿块同路径不可拆（序差 D5） | 27 个 vm_cache 测试计数现核 |
  | K-037 | 无锁是 K-010 的直接推论 | — |

- 验收标准：get_block_ino 分支树（命中/逐出/读入 × 三种取值模式）可默画；逐出失败恢复原样的路径讲清；预读分层声明句（C3）就位；本篇 310 行不拆分（若拆，见 §9 U-5）。

### 05-block-io

- 一句话定位：向驱动开出的取书单：四要素、三道关卡、逐块行走，以及给设备挂牌子的 NEW_DRIVER。
- 讲什么：K-038~K-045。
- 不讲什么：块驱动本体 → 16-stage-drivers（E-FSBDEV）；缓存内部 → 04；mfs 侧 get_block 包装宕机差异 → 07。
- 前置：04。
- 后置：07、14、15。
- 事实底线：`bio.c:48-53,80-115,158-230（:194 last_size）,236-263`；`call.c`（fsdriver_newdriver）；Rust bio.rs（:345-349 RamDisk）、bdev_bridge.rs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-038~K-043 | bio.c 本体 | 8 处锚点替换 |
  | K-044/045 | 内存盘与 trait 化桥接是 Rust 侧特有 | bdev_bridge 接线现状 |

- 验收标准：一张取书单画全四要素与三道关卡；NO_READ 免读在 bio 层的语义（整块覆写才允许）说清；"窥视 = 收货缺席的读"一句话成立且与 02/04/14 分层不冲突（C3）。

### 06-pfs

- 一句话定位：最小的完整服务器——用 512 个内存节点看清每个回调该长什么样，并把"懒时间"一次讲透。
- 讲什么：K-046~K-055（K-051 懒时间主讲述点）。
- 不讲什么：VFS 侧管道唤醒队列 → 05-stage-vfs（O-1）；mfs 懒时间增量 → 09 §1.6；块层（pfs 没有块层，这正是它小的原因）→ 一句话。
- 前置：01~05。
- 后置：07~17（对照"完整磁盘服务器多了什么"）、09（懒时间回指）、20（fdr_other 消费对照）。
- 事实底线：`pfs.c` 各区段（:49 节点表、:50-86 mount、:105/:126/:184 节点借还、:217-378 读写/截断/状态、:380-451 信号与表）；`servers/vfs/main.c:510`；`kernel/table.c:62`；Rust os/fs/pfs/src/lib.rs（:274 FsDriver 接线）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-046~K-054 | pfs.c 本体 + boot 因果首位 | **14 处锚点替换**（本篇与 01 并列重灾区），逐函数现场 grep |
  | K-055 | Rust 建模决策属本篇实现章 | lib.rs 现状 |

- 验收标准：懒时间"三标记 + 查询时一次时钟结清"完整讲述（首次出现即完整）；九回调清单与 pfs.c 表逐行对上；能回答"pfs 挂载为什么可以不碰设备"。

### 07-mfs-init-main

- 一句话定位：参考实现的接线板：进程从 main 走到消息循环之间发生的四步、一个信号、一张 31 行的账本。
- 讲什么：K-056~K-061。
- 不讲什么：31 行账本每行的回调内容 → 08~17；SEF 库内部 → 14-stage-runtime；pfs 对照放一句话。
- 前置：01~05（06 强烈建议先读，作"完整 vs 最小"对照）。
- 后置：08~17 全部。
- 事实底线：`mfs/main.c:14-42,47-65,70-78`；`mfs/table.c:13-45`；`mfs/cache.c:22-37,42-109`；Rust startup.rs/mfs_cache.rs/table.rs（:40 EntryStatus）/server.rs/lib.rs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-056~K-060 | main.c/table.c/cache.c 启动段本体 | 初装四步顺序论证 |
  | K-061 | 服务装配终态（Fix #1）是文档落后代码最重的一节 | **C2 必查**：todo §3.1 + server.rs diff；§4.5 新增小节 |

- 验收标准：初装四步"顺序即依赖"逐对解释（为什么 vmcache 开关最先）；31/0 账本与 table.rs EntryStatus 一致；宕机改报错的三处（get_block/池满/resize）列全。

### 08-mfs-super

- 一句话定位：磁盘的户口本与账本：布局七段、魔数三道、位图一进一出、干净位一体面。
- 讲什么：K-062~K-068。
- 不讲什么：inode 区内容 → 09；挂载纵贯 → 10；alloc_zone 的区粒度策略 → 07（位图位粒度在此）；ext2 位图 → 21 差异。
- 前置：04、05、07。
- 后置：09、10、14、15、17。
- 事实底线：`super.h:4-60`；`const.h:4-66`（魔数 :22-26）；`super.c:29-106,111-156,161-168,173-236,241-355（:252,:258）,360-364`；`mount.c:13-21`（降级消费点预告）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-062~K-068 | super.c/super.h 本体 | 魔数五常量 vs 文档"三魔数"口径：以 §0.3 实测（REV 变体同值判定）写清 |

- 验收标准：布局图七段带字节区间；三魔数两命运表与 super.c:252/258 逐行一致；"版本检查先于字段解释"论证独立成立；位 0 保留不变量在 alloc/free 两侧的体现。

### 09-mfs-inode

- 一句话定位：内存 inode 的旅馆手册：64 字节户口、128 桶门牌、三条入住路、退房批量递减、无主清算上报。
- 讲什么：K-069~K-076（K-074 为对 06 的回指+增量）。
- 不讲什么：懒时间机制本体 → 06（C3）；rw_inode 的时间结清调用点 → 14/16；清算执行体 → 13/15。
- 前置：04、06、08。
- 后置：10~17。
- 事实底线：`inode.h:20-50`；`type.h:8-17`；`inode.c:38-64,70-112,118-200,206-246,252-343,349-370,375-449`；Rust inode.rs（put_count）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-069~K-071/073/075 | inode.c/inode.h 本体 | K-075 只读护栏下沉位置（P2-9）现核 |
  | K-072 | putnode 批量递减（Fix #3） | C2：link.rs/inode.rs 现状 |
  | K-076 | ReclaimZones 上报侧在本文件 | 执行体三出口在 13/15 的分工句 |
  | K-074 | 只记 mfs 增量（只读跳过/结清时机） | 首句回指 06（C3） |

- 验收标准：get_inode 三条路流程图；"超量 put_count 按协议违反拒绝"与 C 侧行为差异表；64B 磁盘格式与内存结构字段对照表完整。

### 10-mfs-mount

- 一句话定位：纵贯线——一次 REQ_READSUPER 从门禁进来到根节点到手的全部七步，以及反向的卸载六步。
- 讲什么：K-077~K-080。
- 不讲什么：read_super 字段细节 → 08（引用）；get_inode → 09（引用）；VFS 侧 mount 记账 → 05-stage-vfs；框架门禁 → 回指 01。
- 前置：01（门禁回指）、08、09。
- 后置：18/20/24（拒根变体）、21（六开关差异）。
- 事实底线：`mount.c:10-98,104-128,134-172`；Rust mount.rs（MountedFs/Parts）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-077/078/079 | mount.c 本体 | 七步每步引用 08/09 结论而非重讲（D2 回指补偿位置） |
  | K-080 | 三方案比较是 Rust 架构决策 | mount.rs 现状 |

- 验收标准：按 T8 顺序能把七步各落到一个 C 行段；卸载六步与 01 循环条件、17 sync 顺序两处呼应句齐全；"门口挂牌"三问可复述。

### 11-mfs-path

- 一句话定位：目录花名册：64 字节定长槽、四种查法一次走、`..` 的父目录规矩。
- 讲什么：K-081~K-084。
- 不讲什么：getdents 协议侧编码 → 03；名字变更 → 13；变长目录项 → 22 差异；03 的漫步框架不再重讲。
- 前置：09、10。
- 后置：12、13、14（getdents 走此盘的冷目录路径）。
- 事实底线：`mfsdir.h:13-18`；`path.c:16-86,92-240`。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-081~K-084 | path.c/mfsdir.h 本体 | "全系统唯一截断点"grep 复核（是否仍唯一） |

- 验收标准：四种查法状态机图（含 ENTER hint）；ino==0 空坑与"唯一截断点"两条不变量各配一个失败例。

### 12-mfs-open

- 一句话定位：名字的诞生：新节点四拍、mkdir 点点舞、符号链接住单间、mknod 挂号。
- 讲什么：K-085~K-088。
- 不讲什么：链接的断开与改名 → 13；chmod 等事后修改 → 16；框架侧符号链接解析 → 回指 03。
- 前置：11。
- 后置：13、16。
- 事实底线：`open.c:56-71,77-122,128-187,192-257,263-270`。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-085~K-088 | open.c 本体 | 四拍回滚表逐拍核对 |

- 验收标准："孤儿节点优于悬空名字"能独立论证（回滚顺序为什么不对称）；nlinks=2 与 13 的 rmdir 记账衔接句。

### 13-mfs-link

- 一句话定位：名字与实体的账本：硬链接加一、unlink/rmdir 减三处、rename 四岔路、截断只出计划。
- 讲什么：K-089~K-093（K-093 判断侧；执行侧归 15）。
- 不讲什么：截断执行（freesp 消费）→ 15；空 inode 的最终释放 → 09 上报 + 本篇/15 触发；软链内容读取 → 03/回指。
- 前置：11、12。
- 后置：15、17。
- 事实底线：`link.c:32-97,103-249（:205-211）,255-422,428-637`；Rust link.rs（remove_directory）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-089/090 | link.c 前段本体 | unlink 与 rmdir 共用路径的行级差异 |
  | K-091/092 | 记账与决策树（Fix #5/#8） | **C2**：link.rs 三处 diff |
  | K-093 | 判断与执行分离的判断侧 | 与 15 的分工线一句写死 |

- 验收标准：rmdir 三分支记账图（父名-1/子`.`-1/父`..`-1→NO_LINK）可默画；rename 四岔路决策树与 link.c:255-422 段段对应；截断计划（truncate_inode/nextblock）作为纯函数讲解、零副作用论证成立。

### 14-mfs-read

- 一句话定位：读路径全景：块号翻译 7+1+1、空洞读零、窥视直通、文件级预读、目录枚举。
- 讲什么：K-094~K-100。
- 不讲什么：写臂 → 15；缓存内部命中细节 → 回指 04；bio 单据 → 回指 05；三层预读对照表放本篇（§3.3 裁决）。
- 前置：08、09、10、11（目录枚举）、13（无则不引）——实际最小集：04、05、08~11。
- 后置：15、16、21~23（变体读路径对照）。
- 事实底线：`read.c:23-111（:89-111,:156-159）,117-204,210-279,281-325,341-435,437-556`；`inode.h` z_zones[10]；`table.c:20`（FSC_PEEK）；Rust read.rs、dir_io.rs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-094~K-096 | read.c 翻译与分块本体 | 间接块行段现核 |
  | K-097~K-099 | 窥视/预读/枚举三专项（Fix #9/#16/#17） | **C2** 逐条 diff |
  | K-100 | dir 桥是 Rust 新增实现（Fix #1） | dir_io.rs 现状；"设计→实施"追踪记入 §4 |

- 验收标准：7+1+1 翻译图 + 一个双重间接实例走全；"空洞读零不分配"与写路径分配的对称性说明；预读三层对照表就位（C3）。

### 15-mfs-write

- 一句话定位：写路径全景：写映射双臂、间接块生长与回收、整块覆写免读、截断执行收口。
- 讲什么：K-101~K-105 + K-076/K-093 的执行侧（ReclaimZones 三出口、truncate 执行）。
- 不讲什么：分配位图本体 → 08；alloc_zone 策略 → 07；bio NO_READ 底层实现 → 回指 05。
- 前置：14（与读共用翻译，差异只在写臂）。
- 后置：17。
- 事实底线：`write.c:28-185,191-227,233-248,254-305,311-318`；`read.c:48-61,89-111`；Rust write.rs（ensure_block）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-101~K-104 | write.c 本体 | WMAP_FREE 先释放后清槽的顺序 |
  | K-105 | 四层写栈（Fix #14） | ensure_block 现状（C2） |
  | K-076/K-093 执行侧 | 出口与消费者在写路径 | 与 09/13 的引用句 |

- 验收标准：四层写栈每层的拒绝条件列全；整块免读从 REQ_WRITE 标志到 bio NO_READ 的贯通链；截断执行的三种出口（直接/间接/位图）与 13 计划一一对应。

### 16-mfs-metadata

- 一句话定位：文件的小卡片维护：权限三步、时间三选择器、stat 先结清再上报、statvfs 现算。
- 讲什么：K-106~K-110。
- 不讲什么：懒时间机制 → 回指 06（C3，只留 utime 修改侧语义）；时间结清的读路径触发点 → 回指 14；Stat 结构体字节表 → 99。
- 前置：09、14。
- 后置：99（typed Stat 表行）。
- 事实底线：`protect.c:9-58`；`time.c:11-48`；`stadir.c:11-76,82-104`；`utility.c:10-36`（conv2/conv4）；Rust minix-types Stat/StatVfs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-106/107/108/109 | 四文件本体 | chown 只读不对称、stat 结算时序现核 |
  | K-110 | typed Stat 收敛（Fix #12） | **C2**：minix-types 88B/80B 现核；消灭了哪些私有布局逐名列 |

- 验收标准：chmod 低 12 位/chown 特权位/utime 选择器各自给出"拒绝例"；estimate_blocks 的估计语义与 stat 上报衔接；历史残留 conv2/conv4 为什么保留（行为契约）说清。

### 17-mfs-maint

- 一句话定位：收尾三件套：关店先清账的顺序、空闲统计怎么数不越界、脏标记红线，外加 mfs 常量目录。
- 讲什么：K-111~K-114。
- 不讲什么：sync 触发（SIGTERM 链）→ 回指 07；脏块缓存本体 → 回指 04；常量数值权威表 → 99（本篇只留使用点索引，C3）。
- 前置：08、09、14、15。
- 后置：99。
- 事实底线：`misc.c:8-23`；`stats.c:13-89`；`clean.h:1-14`；`const.h` 全文件；`glo.h:10-20`；Rust superblock.rs（count_clear_bits_in_image）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-111/113 | misc.c/clean.h 本体 | 先 inode 后块的"为什么"独立论证（崩在中间会怎样） |
  | K-112 | 空闲统计（Fix #18） | **C2**：单点权威函数现状 |
  | K-114 | const.h/glo.h 归属梳理天然落维护篇 | glo 五名字逐一去向表 |

- 验收标准：sync 顺序反例推演成立；count_free_bits 与 Rust 收敛的语义差表；glo.h 五全局各自"搬去哪个模块/为何留在装配层"结论与代码一致。

### 18-vtreefs

- 一句话定位：把"一棵活树"做成框架：节点四组字段、双哈希、两阶段删除、钩子协议，三服务共用。
- 讲什么：K-115~K-123。
- 不讲什么：fsdriver 框架本体 → 回指 01~03；procfs 内容生成 → 19；ptyfs 对照（它**不**用本篇）→ 20；sffs 宿主桥 → 24。
- 前置：01、02、03（+建议读 06；10/11 仅概念对照非硬前置）。
- 后置：19、20（对照）、24（对照）。
- 事实底线：`vtreefs.c:52-110`；`vtreefs/inode.h:30-626`；`sdbm.c:21-30`；`vtreefs/path.c:8-59`；`vtreefs/file.c:14-295`；`vtreefs/mount.c:8-56`；`vtreefs/table.c:6-23`；Rust minix-vtreefs/src/driver.rs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-115~K-122 | libvtreefs 本体 | 1 处锚点替换；钩子全集表逐名列 |
  | K-123 | TreeServer impl FsDriver（Fix #21） | **C2**：driver.rs 现状 |

- 验收标准：钩子清单与 vt_hook 字段一一对上；"框架管循环、钩子填内容"配一个 read 全栈走查；两阶段删除的顺序理由（先摘牌防再入）论证独立成立。

### 19-procfs

- 一句话定位：vtreefs 最大用户：一棵按 pid 排开的观察树，内容全是现读现拼。
- 讲什么：K-124~K-129。
- 不讲什么：RS 消息协议 → 03-stage-rs（O-2）；真实数据源（PCI/内存数）跨 stage 供数 → 各轨道；vtreefs 框架 → 回指 18。
- 前置：18。
- 后置：E-FSCMDS 相关（命令侧读 /proc 归 18-stage-commands，指针）。
- 事实底线：`procfs/main.c:77-93`；`root.c`；`pid.c`；`tree.c`；`util.c:8-65`；`service.c:1-348`；`buf.c:17-125`；Rust content.rs/service.rs。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-124~K-126/028 复用 | procfs 本体 | 4 处锚点替换（本篇=4） |
  | K-127 | 内容面扩展（Fix #23） | **C2**：content.rs 现状 |
  | K-128/129 | service 策略表与暂存窗口 | "只讲机制"边界句（O-2） |

- 验收标准：pid 槽位算式（NR_PROCS/NR_TASKS 索引域，K-125）配两遍刷新时序图；内容生成族至少 7 个文件各一句"数据从哪来、什么时候刷新"。

### 20-ptyfs

- 一句话定位：反例变体：同一个伪终端服务器**不**用 vtreefs，直连 fsdriver 的 32 格固定表。
- 讲什么：K-130~K-134。
- 不讲什么：pty 主从配对语义（INPUT/终端栈）→ 12-stage-input/05-stage-vfs 轨道；vtreefs 本体 → 回指 18（只作对照）。
- 前置：01、18（对照"不用它"必须先知道它）。
- 后置：24（同为"拒根+直连"对照）。
- 事实底线：`ptyfs.c:27-293,299-372,377-434（main :408）`；`node.c:20-83`；`com.h:899-901`；`fsdriver.c:26`（fdr_other 消费点回指）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-130~K-134 | ptyfs.c 本体 | 1 处锚点替换；PTYFS_* 消息号现核 |

- 验收标准："直连 vs vtreefs"一张差异表；控制消息"先验（ds 标签）后办再回"顺序论证；数字名字一一对应为什么可以抛弃树框架说清。

### 21-ext2-init-mount

- 一句话定位：ext2 差异（上）：从"全盘一本账"到"块组各自记账"，以及为 locality 服务的四组放置策略。
- 讲什么：K-135~K-137。
- 不讲什么：namespace/数据格式差异 → 22；mfs 对照语义不再重讲（变体 diff 原则 K-165）；ext2 工具程序（e2fsck/mkfs）→ plan §5.4 排除。
- 前置：07、08、09、10（全部以 mfs 为对照基准）。
- 后置：22。
- 事实底线：`ext2/super.h`；`ext2/super.c:69-459`；`balloc.c:1-362`；`ialloc.c:1-476`；`ext2/mount.c:17-221`。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-135~K-137 | ext2 布局/放置/挂载差异 | 1 处锚点替换；orlov 三变体分支现核 |

- 验收标准：与 mfs 的账本对照表（单本位图 vs 每组位图）；四组放置策略各配一个"为什么这样放"的场景；三道门+六开关表与 mount.c 段段对应。

### 22-ext2-namespace-data

- 一句话定位：ext2 差异（下）：变长目录项、128B inode、12+1+1+1 三级间接、全盘小端。
- 讲什么：K-138~K-140。
- 不讲什么：放置与挂载 → 21；mfs 定长目录项本体 → 回指 11；通用间接映射 → 回指 14。
- 前置：11、14、21。
- 后置：无（ext2 组收尾）。
- 事实底线：`ext2/path.c:20-314`；`ext2/type.h`；`ext2/inode.c`；Rust ext2 mapping.rs；`ext2/super.h`（le 断言）。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-138~K-140 | ext2 namespace/数据段本体 | 删除合并"不留洞"与 mfs 空坑策略对照表 |

- 验收标准：rec_len 走查一个含空洞历史目录的实例；两代容量算式对比（mfs 7+1+1 vs ext2 12+1+1+1，各在多大块下够用）；小端总开关为什么在本 stage 是断言而不是转换。

### 23-isofs

- 一句话定位：只读世界：卷在扇区海里怎么被找到、双端数字段怎么读、Rock Ridge 怎么把 Unix 名补回来。
- 讲什么：K-141~K-145。
- 不讲什么：写路径全部（不存在）；通用缓存/bio → 回指 04/05；mkisofs 工具 → 排除表。
- 前置：10（挂载对照）、14（读路径对照）。
- 后置：无。
- 事实底线：`isofs/super.c:24-121`；`isofs/inode.c`；`isofs/utility.c`；`susp.c`；`susp_rock_ridge.c`；`isofs/table.c:1-32`；`isofs/read.c`。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-141~K-145 | isofs 本体 | 只读子集"关掉的回调"计数与 table.c 对齐 |

- 验收标准：卷发现扫描窗口图（固定偏移起、≤20 扇区、终止符）；extent 跑道一个累减实例；date7 转换与 mfs 时间戳差异一句。

### 24-vbfs-hgfs

- 一句话定位：宿主桥：把自己的文件系统回调换成一串"问宿主"的 round trip，验鲜三判决是灵魂。
- 讲什么：K-146~K-152。
- 不讲什么：宿主协议本体（VMware/VBox 侧）→ 范围外；fsdriver 框架 → 回指 01；vtreefs → 回指 18（只作"另一条复用路线"对照）。
- 前置：01~05、18、20。
- 后置：19-stage-integration（端到端挂载验证指针）。
- 事实底线：`sffs/proto.h`；`sffs/name.c:1-52`；`sffs/path.c:1-108`；`sffs/verify.c:1-118`；`sffs/handle.c:1-77`；`sffs/inode.c`；`sffs/lookup.c:1-150`；`sffs/mount.c:1-89`；`libsffs/main.c:17,58`；`vbfs.c:98-132`；`hgfs.c:64-99`。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-146~K-152 | libsffs 与两桥本体 | 15 操作表逐行列举与 proto.h 对齐 |

- 验收标准：15 个 round trip 全列；验鲜三判决各配一个场景（确定在/确定不在/待查）；"两桥"（备选项→两初始化→进循环）流程图与 vbfs/hgfs 两个 main 对得上。

### 99-global-concepts（重建扩建：21 行骨架 → 全文）

- 一句话定位：本 stage 的字典：REQ 表、消息布局、错误码、服务号一次成表，正文各处数值以此为准。
- 讲什么：K-159（REQ 权威表 + REQ_GETNODE 死槽与 IS_FS_RQ 死宏标注）、K-160（m_vfs_fs_*/m_fs_vfs_*/fsdriver_node/data/dentry/vfs_ucred_t 布局表）、K-161（errno + EENTERMOUNT 族 -301..-303 映射表）、K-162（服务号常量）；附"各篇常量使用点索引"（K-114 的索引半）与"peek 四层含义索引"（§3.3 裁决）。
- 不讲什么：任何"为什么"（语义全在各机制篇，本篇只给数值 + 锚点 + 一行语义摘要）；minix3 全部其它子系统常量（越界）。
- 前置：无硬性（建议在读过 01 后查阅；编号最后，不产生前向依赖）。
- 后置：01/02/03/17 及所有引用数值的篇章（改双标：99 + 源码锚点）。
- 事实底线：`vfsif.h:20-28,41-77`；`com.h:589` 及 FS 服务号段；`include/minix/ipc.h` 系消息声明；`fsdriver.h`；Rust minix-types/protocol.rs 对应物。
- 知识点清单：

  | 编号 | 为什么归本篇 | B 相复核点 |
  |------|--------------|-----------|
  | K-159~K-162 | 字典职责本体 | 33 行 REQ 表逐行对照 vfsif.h；死槽/死宏标注（todo V1-P3-7 结论） |

- 验收标准：REQ 表行数=33 且注明"callvec 32 槽 + 1 死槽"；错误码表含 -301..-303；每张表每行带源码行号；grep 全 stage 正文中出现在 99 表内的数值与本表零冲突（机械比对脚本见 §9）。

---

## 6. 变更表

统一一张表。操作类型仅出现三类：**扩建**（00/99 从骨架成文）、**内容重建**（24 篇编号不变、叙述推倒重写）、**排除**（判定不入本 stage）。无重排、无拆分、无合并、无归档、无重编号（裁决与成本见 §4.1）。

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 存量去向 |
|------|------|--------|--------|------|-----------|----------|
| OP-01 | 扩建 | 00 骨架（21 行） | 00 全文（8 节，见契约） | G-01~G-09 全部缺口由此吸收 | K-153~158、K-163~165 | 骨架核心点全部保留并展开，无丢弃 |
| OP-02 | 扩建 | 99 骨架（21 行） | 99 全文（4 表 + 2 索引） | G-02；常量数值散写各处需权威收口 | K-159~162 | 骨架列出的要点全部成表，无丢弃 |
| OP-03 | 内容重建 | 01~24 每篇 | 同编号同名 | §3.5 三类缺陷（47 锚点、Fix 后同步、混杂漂移）+ C4 节级重排 | K-001~K-152 | 池内每条"现有位置"→契约主讲述点，166 条零丢失（§9 G5） |
| OP-04 | 排除 | —（本 stage 不存在） | 不立篇 | 链接脚本/汇编入口/陷阱进入无制品（G-10/K-166）；真机端到端归 19-stage-integration（G-11） | K-166 | 排除项在 00 边界节各留一句去向 |

**双方向规则核对**（prompt 步骤 4）：
- 存量方向：§2 池 158 条存量每条在 §5 契约中恰好出现一次为"讲什么"或标注"回指"（重复项按 §3.3 收敛到主讲述点，其余篇降级为一句话引用——去向仍是该新篇章，只是形态改变，不算丢弃）。**无任何一条写不出去向。**
- 新增方向：8 条新增（K-155~158、163~166）全部带证据锚点（源码/制品/edge_todo 行号），落于 00/99 契约的"讲什么"。

---

## 7. 缺漏新篇（非 C 主题固定清单逐项落实）

本节不允许留空。每项：主题 → 是否立篇 → 归属 → 原料 → 验收。详细"在哪讲"见 §3.6，此处落实为可执行任务。

| # | 主题 | 落实 | 原料在哪里 | 验收 |
|---|------|------|-----------|------|
| N-1 | 链接与加载 | **否决立篇**：FS server 是普通用户态 ELF，无本 stage 特有链接知识；归 14-stage-runtime | 14-stage-runtime 文档 | 00 边界节一句话 + 指向，grep 确认存在 |
| N-2 | 镜像与内存布局 | **并入 00**（K-156）：bootramdisk/`fs_imgrd`→DEV_IMGRD→mfs 根消费端半段 | `servers/vfs/main.c:517` 参数；master-plan boot 两层语义 | 00 内该节每步带锚点；制作侧明确移交 |
| N-3 | 汇编入口与陷阱进入 | **否决立篇**：实测无制品（`os/fs/*/src` 无 `asm!`/`global_asm!`，入口为普通 main） | —（排除证据） | 00 边界节一句"本 stage 无汇编制品"，附 grep 命令 |
| N-4 | 启动装配 | **双层已有归属**：boot 登记+挂载并入 00（K-154/155）；进程内初装 07 既有职责（K-056~058），重建时补 §4.5 装配终态 | §1 真序表 T1~T5；mfs/main.c；server.rs | 00 与 07 两处各自契约验收；T2 八 server 入口全列 |
| N-5 | 构建与工具链 | **并入 00**（K-155）：minix3 `fs/Makefile.inc` BINDIR=/service + Rust workspace `os/fs/*` 组织 | Makefile.inc；os/Cargo.toml members | 00 内能回答"新增一个 FS server 要改哪几处"（两侧各列） |
| N-6 | 跨模块接口与线格式 | **并入 99**（K-159~162 成表）+ 01/02/03 语义讲述维持 | vfsif.h/com.h/ipc.h/fsdriver.h；minix-types/protocol.rs | 99 契约验收（33 行表逐行对源）|
| N-7 | 错误路径 | **否决单列**：错误处理分散但完备——门禁 EINVAL（01）、协议错误码（03/99）、魔数拒绝（08）、只读拒绝（11）、超量 putnode（09）、池满报错（07）；99 收口映射表（K-161） | 各篇契约"事实底线"行 | 99 错误码表 + 每条错误在机制篇有讲述点（抽查 5 条） |
| N-8 | 关闭与退出 | **否决单列**：链路已闭环分散四篇且互为引用：01（K-008 框架）→07（K-057 信号）→17（K-111 清账）→10（K-078 卸载） | §1 T10 | 00 导航图画出这条链，四篇互相回指齐 |
| N-9 | 并发与同步 | **否决单列**：本 stage 模型是单线程事件循环；两处主讲述（01 K-010、04 K-037）+ 其余篇回指 | fsdriver.c:76 推论；AGENTS 执行模型 | §3.3 收敛执行后，grep"单线程"无第二处完整讲述 |
| N-10 | 测试基建 | **并入 00**（K-163 矩阵）+ 各篇 §5 明细；真机空位标注归 19-stage-integration（G-11） | todo §7/§9；各篇 §5；os/qemu-tests/ 核对 | 00 矩阵总数=当日 `cargo test` 实测；空位显式写"无"而非留白 |

**结论**：无一篇需要全新立号；8 项缺口由 00/99 两篇扩建吸收，2 项否决给理由，与 §4 编号保留裁决自洽。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（逐节去向，覆盖所有变化文档）

24 篇编号文档的节级骨架按 C4 重排，节的**内容去向**由 §2 池的"现有位置"列 → §5 契约"讲什么"完全决定（每条 K 的旧位置→新主讲述点一一在案，此处不重复誊 166 行）。本节只列**非平凡迁移**——旧位置与新位置不是同一篇、或形态改变的节：

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 备注（断链风险） |
|--------|------------------|--------|----------|------------------|
| 00 §核心点/§边界（全部 21 行） | 素材指针式要点 | 00 新 8 节各自展开 | 改写 | 无外部引用指向 00 具体小节，零风险 |
| 99 §核心点（6 个要点行） | 常量表要点清单 | 99 四张成表 + 两索引 | 改写 | 同上 |
| 06 §1.5 完整懒时间 → 09 §1.6 | mfs 侧时间戳曾近完整重讲 | 09 §懒时间增量（新节） | 合并（收敛到 06） | 09 旧 §1.6 读者预期在：保留节位、改回指+增量，不断链 |
| 06 §1.5 → 16 §1.3/1.4 | utime 处第三次重讲机制 | 16 只留修改侧语义 | 合并 | 同上 |
| 14 §1.5 新增三层预读对照表 | 对照表原先不存在 | 14（§3.3 裁决落点） | 新增 | 需从 04 §2.8/05 §1.4/14 §1.5 现场摘出层级参数，三处锚点 |
| 01/02/03/17 内散落常量数值 | 正文直接写数值 | 保留数值 + 双标"99 + 源码锚点" | 改写（轻量） | 不强制删除正文数值，防大面积扰动（§3.3 行 7 裁决） |
| 各篇 §5 测试计数段 | 各报各数 | 各篇 §5 保留明细 + 00 矩阵索引 | 合并（索引化） | "截至日期"字样统一为重建日重测日 |
| 09/13/14/16/17/18/19 §2 各节中 Fix 前描述 | 与终态代码不符的行为句 | 同篇新对应节，按 todo Fix 记录改写 | 改写 | 断链风险=0（编号不变），风险在事实：C2 条款强制 diff |
| 47 处"工具生成"锚点（01=16、04=1、05=8、06=14、17=1、18=1、19=4、20=1、21=1） | 行号近似、符号名滞后 | 原地替换为现场核对锚点 | 改写 | 修复本身不断链；防复发靠 §9 机械门 M4 |

### 8.2 引用迁移表（谁引用了旧编号/旧文件名，迁移到什么）

| 引用类别 | 数量（实测） | 载体与热点 | 裁决 | 验证方式 |
|----------|-------------|-----------|------|----------|
| stage 内全路径引用（`15-stage-fs/NN-*.md`） | 127 | 分布于各篇"参见"章与正文交叉引用 | **零迁移**（编号与文件名不变，§4.1） | 重建后 `grep -rn "15-stage-fs/[0-9]" notes/` 计数不减、死链为零 |
| stage 内"第 NN 篇"式引用 | 141 | 各篇正文 | **零迁移** | 同上按篇号白名单核对 |
| 外部文档引用 | ~15 | `16-stage-drivers/plan.md`（最重，两处点名具体文档）、16-stage 两篇正文、18-stage-commands、edge_todo、14/17/05-stage、master-plan README、xtask | **零迁移**；16-stage-drivers 引用恰好是我们移交 E-FSBDEV 的对象，重建后须保持被引用性 | `grep -rn "15-stage-fs" --exclude-dir=15-stage-fs` 前后对比 |
| Rust 源码注释映射 | 13 | `os/libs/minix-fs/src/lib.rs:1-30`（模块→文档 01~05 映射） | **零迁移**（指向的编号语义不变） | `cargo check` 无关；人工复读一次 |
| 锚点文本内"工具生成"字样 | 47 | §8.1 末行列分布 | 全部替换（文档内改动，不涉链接） | `grep -c "工具生成" *.md` 归零（本蓝图自述除外） |
| 他人蓝图中的失配引用（`26-ptyfs.md`、`24-vtreefs.md`） | 不处理 | 只存在于 doc_rerank_glm/deepseek（本蓝图未读取其内容，仅文件名 grep） | 非本 stage 正式文档，越权不碰 | — |

### 8.3 断链成本摘要

- 受影响引用总量：**296 处**（127+141+15+13），本方案下**需要改动的为 0 处**——这是 §4.1 保留编号裁决的直接收益。
- 实际改动量：锚点文本修复 47 处 + 26 篇正文重写（其中 2 篇从零扩建）+ 8 处非平凡节迁移（§8.1）。
- 热点文件：01（16 锚点）、06（14 锚点）两篇是 B 相第一优先质检对象；外部热点 `16-stage-drivers/plan.md`（改动前需对账 E-FSBDEV 表述仍成立）。
- 批量方式建议：B 相按篇提交（一篇一 commit），每篇完成即跑 §9 M1~M4 四道机械门；不做大爆炸式全量重写后统一验证。

---

## 9. 验证与自检门

### 9.1 四种机械检查（prompt §5.4）

| # | 检查 | 结果 | 证据 |
|---|------|------|------|
| M1 | 前向引用扫描（逐篇"前置"只指更早编号） | **通过** | 前置边集：00/01/99←∅；02←01；03←01,02；04←01；05←04；06←01~05；07←01~05；08←04,05,07；09←04,06,08；10←01,08,09；11←09,10；12←11；13←11,12；14←04,05,08~11；15←14；16←09,14；17←08,09,14,15；18←01~03；19←18；20←01,18；21←07~10；22←11,14,21；23←10,14；24←01~05,18,20。全部指向更小编号 |
| M2 | 依赖图无环 | **通过** | 所有边从小到大，编号序即拓扑序，无环（特例 99 被引用为"数值双标"而非语义前置，不构成图边；00 是导航非知识前置） |
| M3 | 知识点池 100% 有去向 | **通过** | 机械验证已跑：契约区 K 编号按区间/斜杠展开后 166/166 全覆盖，零遗漏；另有 K-166 显式排除给理由；无“写不出去向”条目（§6 双方向核对） |
| M4 | 断链成本统计 | **通过** | §8 三表齐：296 处引用、0 处需迁移、47 处锚点文本修复，热点与批量方式已列 |

B 相完成后的机械命令（写进 00 重建的验收，也在此固化）：
```bash
grep -c "工具生成" *.md                       # 期望：全 0
grep -rn "15-stage-fs/[0-9]" --include="*.md" # 死链为零（可接 tools/check_references.sh）
# 99 表数值 vs 正文数值一致性：从 99 提取数值集合，grep 各篇比对（B 相写一次性脚本，产物带 _qwen 后缀）
```

### 9.2 自检门 G1~G9

| 门 | 结果 | 依据 |
|----|------|------|
| G1 C 真序逐条可核对 | **通过** | T1~T11 全带锚点；随机抽 10 条复核：table.c:62-63（boot grep）、fsdriver.c:17-62/64-71/76-97（全文 cat）、vfsif.h:41/75、com.h:589、mfs/main.c:47-65/70-78、const.h:22-26、super.c:252/258、main.c:510/517、vtreefs.c:52-59——输出全部记录于 §0.3 |
| G2 知识点池完整（每文件有归属或排除） | **通过** | 92 个 C 文件对 plan §5.1 映射再核（§3.1 路 1）：全落 K-001~152 或 plan §5.4 排除表；非 C 制品逐项（§3.1 路 3）落 K-155/156/159~163 或 N-1/N-3 排除 |
| G3 前向引用为零 | **通过** | M1 边集 |
| G4 依赖图无环 | **通过** | M2（无环，无需拆解方案） |
| G5 覆盖率 100% + 新增有锚点 + 删除单列 | **通过** | M3；8 条新增全带锚点；明确删除项：**无**（0 条知识点被删除，K-166 是"排除"非"删除"，单列于 §2.6/§6 OP-04） |
| G6 拆合写清去向、新建写清来源（抽查 10 处） | **通过** | 抽 OP-01/02 全表 + 契约 00/99/14/16/17 的合并行 + §8.1 前两行：存量去向齐（池"现有位置"列）、新增来源齐（锚点列） |
| G7 契约七要素齐全 | **通过** | 26 篇契约 × 7 要素逐篇自查：定位/讲/不讲（带去向）/前置（只指更早）/后置/事实底线（带锚点）/清单+验收——其中清单采用"§2 池 + 契约增量表"的压缩写法，全部 166 条编号可寻（§5 前言声明） |
| G8 迁移表覆盖所有变化文档与代码注释引用 | **通过** | 26 篇全部变化（24 重建+2 扩建），§8.1 规则行 + 非平凡行覆盖；§8.2 覆盖文档引用与 lib.rs 代码注释 |
| G9 事实断言有锚点（抽 10 + 推测标注） | **通过** | 本蓝图机制/代码/制品类断言均带锚点；已显式标注的待验证/推测项：G-11 的 qemu-tests 判定为"目录核对"级证据（若 B 相发现隐藏用例以实测为准）、14 处 K 条目行段为"区段"精度（B 相现场 grep -n 收紧）、§3.4 半越界判定属裁决级而非事实级 |

### 9.3 结论

**蓝图完成**，可交付汇总收敛。核心裁决一句话：**顺序骨架是对的，坏的是内容**——保留 00→24→99 全部编号，重建两块骨架（00/99 从零成文），按 26 份契约重写 24 篇正文，清剿 47 处工具生成锚点与 V1 修复后的文档-代码漂移，把 8 条边界/工程知识点并入 00、4 条常量表并入 99；断链成本从全量重编号的约 296 处降为 0 处链接迁移 + 47 处文本修复。

### 9.4 待用户裁决的问题

| # | 问题 | 本蓝图立场 | 备选 |
|---|------|-----------|------|
| U-1 | 是否保留编号（§4.1） | 保留：真序对账未发现整篇移动违例，断链账 296→0 | 全量重编号（需先解决约 296 处引用迁移与历史"I-14 止步"教训） |
| U-2 | 构建/登记细节（K-155）放 00 还是 19-stage-integration | 放 00：它是 boot 因果第一环，读者需要 | 下放集成篇，00 留指针 |
| U-3 | 47 锚点修复时机 | 随 B 相逐篇修（反正正文重写） | 独立批量 pass 先行（适合 B 相排期靠后时） |
| U-4 | 21/22 ext2 两篇分界（上=布局放置挂载 / 下=namespace 数据） | 维持现分界，契约已写死各自对照基准 | 若觉两篇体量失衡可并一篇（22 无后置，合并零断链） |
| U-5 | 04（310 行、含 27 个二级缓存测试）是否拆分 | 不拆：二级缓存与拿块是同一条路径的两级（序差 D5 论证），拆篇切断一次拿块 | 拆 04a 缓存本体 + 04b VM 通道（代价：新增编号、E-FSVMCACHE 表述重挂） |

> 本报告由 qwen 独立产出；未读取任何其它 AI 的 `doc_rerank_*` 产物，未引用 `.design/` 与 `tmp_design_and_todo/`。
