# 28 — stadir：工作目录切换、stat 查询与 statvfs 遍历

本文讲清目录状态层如何处理四类操作：切换工作目录与根目录（chdir/fchdir/chroot）、stat 查询文件属性（stat/fstat/lstat）、statvfs 查询文件系统状态、getvfsstat 遍历所有挂载点：工作目录与根目录决定相对路径解析的起点与边界，切换时有三项检查（相同则跳过、须是目录、须有执行权限），chroot 仅 root 可调用；stat 有按路径与按 fd 双入口（区别仅在于是否跟随符号链接）；statvfs 分实时查询与缓存读取（实时失败返回 EIO，缓存先零填再复制）；getvfsstat 遍历填充（无缓冲时只计数，填充失败即返回）——工作目录变动则相对路径解析结果随之变化，遍历结果在遍历过程中逐项填充。

前置阅读：`13-path-lookup.md`（路径解析）、`14-filedes.md`（按 fd 取 vnode）、`02-fproc-struct.md`（工作目录与根目录字段）、`06-vmnt-table.md`（挂载表）。

> 本章不讲什么：
> - 寻路的执行—— `13-path-lookup.md`（本篇只给路径查询的调用点）
> - 权限判定的执行—— `29-protect.md`（本篇只给执行权限检查位）
> - FS 下发的执行—— `12-request-wrappers.md`（本篇只给 FS 请求 trait）
> - 锁执行的细节—— `06-vmnt-table.md`（本篇只给加锁条件）
> - 用户缓冲拷贝的执行—— 内核侧（本篇只给填充规则）

---

## 1 概念

### 1.0 引言与前置

三类问题：当前在何处（工作目录与根目录）、文件属性是什么（stat 查询）、文件系统状态如何（statvfs 查询与 getvfsstat 遍历）。本章假设读者已了解路径解析（见 13），只讲"工作目录切换与属性查询"。

### 1.1 为什么工作目录是相对路径的起点

相对路径没有起点就无法解析：工作目录决定 `.` 指向何处，根目录决定 `..` 到何处停止。切换工作目录即更换起点——起点一变，所有相对路径的解析结果都变。因此切换有三项检查：相同则跳过（已在此目录无需切换）、须是目录（非目录返回 `ENOTDIR`）、须有执行权限（无执行权限的目录无法作为起点，由 29 章判定）。引用计数更新顺序是先加新引用再放旧引用、最后交换指针——顺序颠倒会导致旧引用泄漏。

### 1.2 切换工作目录的三项检查

三项检查即"工作目录必须可用"，顺序即优先级。相同则跳过排第一（无变化不打扰）；类型检查第二（非目录直接报错，不查权限）；权限检查第三（X 位决定是否可进入）。按 fd 切换（fchdir）与按路径切换（chdir）检查相同、路径不同：前者手持 vnode 直接检查，后者需先解析路径再打开 vnode——检查相同、来源不同，共用 `change_into` 函数。

### 1.3 chroot 的 root 检查

切换根目录影响全局：根一变，`..` 的终止点、`/` 的起点全变——一次调用改变整个路径解析边界，非 root 返回 `EPERM`，没有例外。其余流程与按路径切换相同（复制路径、解析、打开 vnode、替换根目录），root 检查是唯一的增量。切换之后，旧根之外的路径全部不可见——根检查通过后没有其他分支。

### 1.4 stat 的双入口

stat 查询有两路：按路径（解析路径取 vnode）与按 fd（从 fd 取 vnode）。两路最终都下发同一个 FS 请求（`req_stat` 查询文件所属 FS），区别只有两处：是否需要路径解析、符号链接尾是否保留——lstat 保留符号链接尾（查询链接本身），stat 展开符号链接（查询链接指向）。按路径与按 fd 只是来源不同，查询逻辑同一套。

### 1.5 statvfs 的实时与缓存

statvfs 查询分实时与缓存：实时（fresh）直接询问 FS，失败即返回 `EIO`；缓存（`ST_NOWAIT`）复制挂载表中的旧数据，先零填再复制。只读标志叠加：挂载点只读时，结果标志或入 `ST_RDONLY`（叠加而非覆盖——挂载表原有标志保留）。文件系统标识：fsid 取设备号（同时按 POSIX 与 NetBSD 格式各写一份），类型名、挂载点、来源三项直接复制——挂载表中的名称复制即结果。

### 1.6 getvfsstat 的遍历填充

getvfsstat 遍历挂载表并填充结果数组：无缓冲时只计数（只返回数量，不加锁）；有缓冲时加锁遍历（`do_lock` 取反于 NOWAIT）——缓冲写满即停（`bufsize` 耗尽即返回）、跳过不可上报的挂载（无设备或不可上报）、填充失败即返回（先解锁后返回错误）。遍历即填充：每走一个挂载点，计数并填充一项；某项失败时整个调用停止——已填充的部分保留计数。

### 1.7 与其他 OS 的目录状态对照

- **Linux** 以 `vfs_statfs`（实时/缓存分流 + 只读标志叠加 `ST_RDONLY`）与 `getvfsstat` 式遍历（`iterate_supers` + 有名额即填）实现同构语义：`fill_statvfs` 的零填缓存对应 `statfs` 的 `ST_NOWAIT` 快路，`walk_plan` 的失败即返对应 `statfs_by_dentry` 的错误短路；`change_into` 的三项检查对应 `chdir_common` 的目录类型与执行权限检查。
- **Redox** 的 stat 方案以句柄属性（目录类型 + 权限检查）对应工作目录切换与 stat 查询；Redox 以方案内缓存对应 statvfs 缓存。
- **seL4** 无目录状态原语，工作目录与挂载表都是各服务的内部状态——有状态服务器把遍历做成循环（本篇的 walk），无状态内核把它留给持有能力的客户端自行遍历。

### 1.8 小结

目录状态层处理四类操作（切换工作目录→stat 查询→statvfs 查询→getvfsstat 遍历），三组条件（目录是否相同/来源是路径还是 fd/查询是实时还是缓存）决定流程走向，一条不变量贯穿始终：每次返回前必有依据——切换依据三项检查，stat 依据双入口，statvfs 依据实时或缓存，遍历依据逐项填充。每步都有依据覆盖，是本篇的核心要求。

---

## 2 C 源码分析

### 2.1 头注释契约（`stadir.c:1-13`）

四类调用涉及目录与文件状态（`1-2`）→ 八个入口（`5-12`：fchdir 实现在 32 行但头注释未列名——以实现为准共九个，见 §3 D1）。

### 2.2 `do_fchdir` 流程（`stadir.c:32-45`）

`do_fchdir`：取 fd（`38`）→ 按 fd 取 vnode（`41`，失败返回 `err_code`）→ 切换工作目录（`42`，公共函数）→ 解锁（`43`）。

### 2.3 `do_chdir` 流程（`stadir.c:50-78`）

`do_chdir`：复制路径（`62-63`）→ 配置路径解析（`66-68`，读锁）→ 打开 vnode（`69`）→ 切换工作目录（`71`，公共函数）→ 释放并归还（`73-75`）。

### 2.4 `do_chroot` 流程（`stadir.c:83-112`）

`do_chroot`：root 检查（`94`，非超级用户返回 `EPERM`）→ 复制路径（`96-97`）→ 配置路径解析（`100-102`）→ 打开 vnode（`103`）→ 切换根目录（`105`，公共函数）→ 释放并归还（`107-109`）。

### 2.5 `change_into` 公共函数（`stadir.c:117-135`）

`change_into`：相同则跳过（`121`）→ 类型检查（`124-125`，非目录返回 `ENOTDIR`）→ 权限检查（`127`，X 位，由 29 章判定）→ 释放旧引用并安装新引用（`131-133`：释放旧 vnode/增加新引用/交换指针）→ `OK`（`134`）。

### 2.6 stat 查询流程（`stadir.c:140-192`）

`do_stat`（`140-168`）：取路径名（`151-153`）→ 配置路径解析（`155-157`）→ 打开 vnode（`159-160`）→ 下发 FS（`161`，结果写入请求者缓冲）→ 释放并归还（`163-166`）。`do_fstat`（`173-192`）：取 fd（`181`）→ 按 fd 取 vnode（`184`）→ 下发 FS（`186-187`，按 vnode 的 FS 与 inode 号）→ 解锁（`189`）。

### 2.7 `update_statvfs` 流程（`stadir.c:197-229`）

`update_statvfs`：询问 FS（`202-203`）→ 十七个字段回填（`205-226`：标志/块数/文件数/同步/名称长度五组）→ `OK`（`228`）。

### 2.8 `fill_statvfs` 流程（`stadir.c:234-289`）

`fill_statvfs`：注释说明三段（`237-241`）→ 实时与缓存分流（`244-274`：实时失败返回 `EIO`，缓存零填后复制）→ 只读标志叠加（`276-277`）→ 文件系统标识（`279-285`：fsid 双格式 + 三项名称复制）→ 复制到用户缓冲（`287-288`）。

### 2.9 statvfs 查询流程（`stadir.c:294-346`）

`do_statvfs`（`294-323`）：取路径名（`305-308`）→ 配置路径解析（`310-312`）→ 打开 vnode（`314-315`）→ 填充结果（`316`，按 vnode 所属挂载点）→ 释放并归还（`318-321`）。`do_fstatvfs`（`328-346`）：取 fd（`335`）→ 按 fd 取 vnode（`340`）→ 填充结果（`341`，按 vnode 所属挂载点）→ 解锁（`343`）。

### 2.10 `do_getvfsstat` 流程（`stadir.c:351-413`）

`do_getvfsstat`：取参数（`359-363`）→ 无缓冲时只计数（`404-410`，不加锁）→ 加锁条件判定（`372`，`do_lock` 即非 NOWAIT，注释说明 procfs 自调用 `366-371`）→ 遍历（`374-403`：缓冲满即停 `376-377` → 加锁 `380-381` → 锁后复验 `388`（使用中且可上报，(un)mount 过程中跳过）→ 填充失败即返回 `389-394` → 计数递增 `396-398` → 解锁 `401-402`）→ 返回计数（`412`）。

### 2.11 `do_lstat` 流程（`stadir.c:418-446`）

`do_lstat`（`418-446`）：取路径名（`429-431`）→ 配置路径解析（`433-435`，保留符号链接尾 `PATH_RET_SYMLINK`）→ 打开 vnode（`437-438`）→ 下发 FS（`439`）→ 释放并归还（`441-444`）。与 stat 仅差一个标志（`433` vs `155`）。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `stadir.c` 的打开 vnode 与填充流程，而是吸收 Linux/Redox 的状态模型后做取舍。以下决策对应 `.design/28-design.v1.md` D1-D7。

### D1 调用枚举

- **C**：头注释八个入口，fchdir 实现在 32 行（`stadir.c:1-13,32-45`）。
- **Rust**：`StadirCall` 九个变体 + `by_fd()` + `keeps_symlink()`（`os/servers/vfs/src/stadir.rs:45,66`）。
- **为什么**：头注释漏列是历史事实，枚举以实现为准（九个变体更准确）。替代方案（八值 + 注释）被否决：调用面的事实不应靠注释遮掩（24-D1 同源惯例：调用号复用处亦以实现为准）。

### D2 工作目录切换检查

- **C**：`change_into` 相同跳过/类型检查/权限检查/引用替换（`117-135`）。
- **Rust**：`change_dir()` + `AnchorOut::{Keep, Switch}`（`os/servers/vfs/src/stadir.rs:80,92`）。
- **为什么**：三项检查各有不同的错误返回；引用替换顺序单独列出——顺序颠倒会导致旧引用泄漏。替代方案（三个布尔与）被否决：三种错误原因各不同（OK/ENOTDIR/上游 EACCES）。

### D3 chroot 的 root 检查

- **C**：`do_chroot` 首检非超级用户即 EPERM（`94`），其余与 chdir 相同。
- **Rust**：`chroot_gate()`（`os/servers/vfs/src/stadir.rs:106`）。
- **为什么**：切换根目录影响全局路径解析边界。替代方案（并入 D2）被否决：root 检查是调用级检查，工作目录切换检查是 vnode 级检查，层级不同。

### D4 stat 双入口

- **C**：stat 按路径/fstat 按 fd/lstat 保留符号链接尾（`140-192,418-446`）。
- **Rust**：`StatSrc::{ByPath{retain_symlink}, ByFd}`（`os/servers/vfs/src/stadir.rs:115`）。
- **为什么**：三个 stat 查询仅差路径解析与终止标志；保留符号链接尾差一个标志位即可表达。替代方案（三个函数分立）被否决：同一逻辑三份拷贝容易漂移。

### D5 statvfs 的实时与缓存

- **C**：十七字段回填 + 实时缓存分流 + 只读叠加 + 文件系统标识（`197-289`）。
- **Rust**：`fill_plan()` + `apply_readonly_overlay()` + `fs_identity()`（`os/servers/vfs/src/stadir.rs:127,135,145,151,161`）。
- **为什么**：实时查询失败即 EIO、缓存先零填再复制，分两路处理；只读标志叠加单独列出。十七字段逐一建模被否决：字段搬运无判定逻辑，注释"整体复制"即可（26-D6 同源惯例）。

### D6 getvfsstat 遍历

- **C**：无缓冲计数/加锁遍历/跳过/失败即返（`351-413`）。
- **Rust**：`MountView` + `WalkOut` + `need_lock()` + `walk_plan()`（`os/servers/vfs/src/stadir.rs:171,180,191,203`）。
- **为什么**：遍历填充时锁后复验与失败即返分别是并发与错误处理的关键；无缓冲计数不加锁单独列出。替代方案（遍历直译）被否决：拆成可测试的矩阵更易验证。

### D7 FS 对话 trait 化

- **C**：`req_stat` + `req_statvfs` 两个下发点（`161,203`）。
- **Rust**：`StatFs{stat, statvfs}`（`ScriptedStat` 按脚本应答 vs `RefusingStat` 常拒）+ `stat_then_statvfs()` 契约探针（`os/servers/vfs/src/stadir.rs:239,248,284,296`）。
- **为什么**：FS 是唯一的不可测点；两法一 trait 足矣（27-D7 同源惯例）。替代方案（两 trait 分立）被否决：知识同源不分立。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-1 单线程事件循环（mthread→状态机） | 锁配对转借用注释；遍历加锁留到执行层 | `stadir.rs:191` 模块注释 + 本文档 D2/D6 + 28 正文 §1.1 |
| A-5 SUSPEND/revive 显式化 | 本篇无 SUSPEND（查询与下发皆同步）；verdict 只有 Done | `stadir.rs:303` + 本文档 D1 + 28 正文 §1.1 |
| A-19 statvfs 缓存类型化（十七字段→两态分支） | 整体复制注释 + 实时缓存枚举 | `stadir.rs:127` + 本文档 D5 + 28 正文 §1.5 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── stadir.rs               — 本篇：调用枚举/目录切换/root 检查/stat 查询/statvfs/遍历/FS 判定
├── path.rs                 — 寻路执行对照（13，路径解析层）
├── filedes.rs              — get_filp 执行对照（14，按 fd 取 vnode 层）
├── fproc.rs                — 工作目录与根目录宿主对照（02，工作/根目录层）
└── vmnt.rs                 — 挂载表结构对照（06，挂载层）

> 设计决策：§3 D1（调用枚举）/ D2（工作目录切换检查）/ D6（getvfsstat 遍历）。

### 4.2 核心符号表

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| 标志位 | `fstypes.h:88,283` | `stadir.rs:31` | RDONLY/NOWAIT |
| 挂载标志/表容量 | `vmnt.h:24,28`/`const.h:7` | `stadir.rs:35,39` | 只读/可上报/16 槽 |
| 九个调用 | `stadir.c:1-13,32` | `stadir.rs:45,66` | 按 fd 族 + 保留符号链接尾 |
| 目录切换检查 | `stadir.c:117-135` | `stadir.rs:80,92` | 三项检查 + 引用替换顺序 |
| chroot 的 root 检查 | `stadir.c:94` | `stadir.rs:106` | 仅 root |
| stat 双入口 | `stadir.c:140-192,418-446` | `stadir.rs:115` | 按路径/按 fd |
| statvfs 实时与缓存 | `stadir.c:197-289` | `stadir.rs:127,135,145,151,161` | 实时缓存/只读叠加/标识 |
| getvfsstat 遍历 | `stadir.c:351-413` | `stadir.rs:171,180,191,203` | 计数/加锁/跳过/失败返回 |
| FS 对话 | `request.h` req 族 | `stadir.rs:239,248,284,296` | 脚本/常拒双实现 + 探针 |
| 错误族 | `stadir.c` 全文件 | `stadir.rs:313,324 StadirError::to_errno` | 4 变体→errno，无自创 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 工作目录必须可用（相同跳过/类型/权限） | `change_dir` 三项检查 | 顺序即优先级 | `stadir.c:121-128` |
| chroot 仅 root | `chroot_gate` | 非 root 返回 EPERM | `stadir.c:94` |
| 实时缓存分流（失败 EIO/零填复制） | `fill_plan` | 两路分明 | `stadir.c:244-274` |
| 失败即返（填充失败则解锁返回） | `walk_plan` | 已填充部分保留 | `stadir.c:389-394` |
| 无缓冲只计数（无缓冲不查询） | `WalkOut` 分支 | 只计数不查询 | `stadir.c:404-410` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **318 passed / 0 failed**（既有 313 + 本篇新增 5；`minix-types` 独立）。
> 本章直接影响 5 项新增。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_calls_and_anchor` | `stadir.c:1-13,32,94,117-135` | 九个调用 + 三项检查 + root 检查 | `stadir.rs:341` |
| `test_asking_roads_and_mountain_form` | `stadir.c:140-192,197-289,418-446` | 双入口 + 实时缓存 + 只读叠加 + 标识 | `stadir.rs:373` |
| `test_tally_walk` | `stadir.c:351-413` | 计数/遍历/缓冲满/失败返回 + 加锁判定 | `stadir.rs:411` |
| `test_fs_dialogue` | `request.h` req 族 | 脚本/常拒 + 契约探针 | `stadir.rs:460` |
| `test_errno_map_covers_stadir_c` | `stadir.c` 全文件 | 4 变体→errno + 旗域 | `stadir.rs:478` |

测试策略：调用以九个变体全枚举锁定；目录切换以三项检查 + root 检查覆盖；stat 以双入口 + 符号链接尾差异覆盖；statvfs 以实时缓存/只读叠加/标识覆盖；getvfsstat 以计数/遍历/缓冲满/失败返回矩阵覆盖；FS 以脚本应答/固定拒绝 + 探针覆盖；错误以 4 变体全映射覆盖。

### 5.1 测试统计（截至 2026-09-03）

- `cargo test -p minix-vfs --lib`：**318 passed / 0 failed**
- 本节列出与本模块直接相关的 5 个（子集）
- 完整测试清单：`rg "fn test_" os/servers/vfs/src/stadir.rs`

---

## 6 过渡

本篇在 13（寻路）与 27（链接）之后、29（权限）之前，是目录状态查询的归属层：13 只管路径解析到 vnode，27 只管目录项变更，本篇管解析之后的状态查询（工作目录切换/stat/statvfs/getvfsstat）；没有本篇，29 的权限检查不知工作目录在何处，30 的锁不知文件系统容量。

```
13-path-lookup: eat_path 寻路（路径解析到 vnode）
   │
27-link: 目录项变更 ───────────────┐
                                  ├─► 本篇：change_dir 切换目录 → StatSrc 查询属性
02-fproc-struct: 工作目录宿主 ─────┘   → fill_plan 查询文件系统 → walk_plan 遍历挂载
           │                              │
           ├─► 29-protect：权限检查（工作目录的下一站）
           └─► 30-fcntl-lock：记录锁（文件系统容量的下一站）
```

阅读顺序提示：若关心"权限检查的下文"，下一站 `29-protect.md`（权限检查）；若关心"路径如何解析到 vnode"，回看 `13-path-lookup.md`（寻路全程）。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/stadir.c:1-446`（十二函数全族）、`minix3/sys/sys/fstypes.h:88,283`（`MNT_RDONLY/NOWAIT`）、`minix3/minix/servers/vfs/vmnt.h:24,28`（`VMNT_READONLY/CANSTAT`）、`minix3/minix/servers/vfs/const.h:7`（`NR_MNTS`）
- 阶段文档：`13-path-lookup.md`（寻路执行）、`14-filedes.md`（取卷执行）、`02-fproc-struct.md`（锚位）、`06-vmnt-table.md`（山表）、`09-main-loop.md`（调用分发）、`29-protect.md`（下一站）、`30-fcntl-lock.md`（下下一站）
- Rust 实现：`os/servers/vfs/src/stadir.rs:1`（本篇判定层）、`os/libs/minix-types/src/types/errno.rs:15`（errno 值）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（填表拷出语义）
