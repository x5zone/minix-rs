# 05-stage-vfs Rust 实现架构级 Review TODO

> 来源：2026-09-06 架构级代码审查（先查漏补缺，后整体/分层架构审视；非逐函数 review）。
> 范围：`os/servers/vfs/` 全部 Rust 代码（33 个文件约 23000 行，与 05-stage-vfs 文档对应）；`os/fs/` 八个文件系统驱动 crate 与 `minix-types`/`minix-sys` 仅做接口对账。
> 方法：覆盖率穷举（`tools/coverage-extract/coverage-extract.py` + `vfs-semantic-map.json`）→ P0 横切正确性扫描 → 四层架构审视（整体 → 模块边界 → 类型设计 → 函数/测试面），对照 Redox scheme 模型与 Rust/OS 社区实践。每条事实断言附 `file:line` 锚点。
> 定位：本文档是查漏清单与架构改进建议，**不同于** `plan.md`（文档重组计划）与 `draft/`（旧 fork 主线素材存档）。修复遵循 fix-guard（每轮读目标行 ±5 行、一次修一条、修后验证）。
> **历史归档**：第一轮全卷（§0～§8 + 附录 A/B，2026-09-06）→ [`archive/todo-R1-archive-2026-09-09.md`](archive/todo-R1-archive-2026-09-09.md)。首轮 26 条全部 open，逐条复核结论见本文 §1。
> 状态（2026-09-09，R2 轮 = 第二轮，见 §9）：**首轮后复扫**。四条工作线——① 存量 26 条逐条 staleness 复核（24 条锚点原样维持，C-5/P2-1/P2-4 三处漂移修正）；② Gate A 覆盖穷举重跑（415/93.3%/63.1% 与首轮完全一致）+ 补扫首轮未下钻的面（device.c/gcov.c、sdev 逐函数对位、64 调用号三层矩阵、VFS↔mfs 协议双侧对账）；③ 执行绑定层与抽象质量深查——发现 **REQ 消息基址 0x600 ≠ C/minix-fs 0xA00 的 wire 级 P0** 与 path.rs 生产单元的 Gate D 虚构抽象族；④ Redox 对照增补（cancellation 已成 redox-scheme crate 一级 API）。本轮新发现：**1 项 P0 + 4 项 P1 + 3 项 P2 + 2 项 P3**（R2-P1-4 系修复期 fix-guard grep 新登记），跨 stage 一条新登记 edge（E-REQWIRE）+ 一条增补（E-VFSWIRE）。修复进度见 §9.9。本轮扫描未修改生产代码，修复自 §9.9 起。

---

## 0. 审查结论速览

### 0.1 历史轮次一览

| 轮次 | 代表条目 | 状态 |
|------|---------|------|
| 第一轮 R1（2026-09-06） | C-1～C-10 缺口表 + P0×3 + P1×5 + P2×6 + P3×2 | 全部 open（正文见 R1 存档；复核见 §1） |
| **第二轮 R2（2026-09-09）** | **R2-P0-1 + R2-P1-1..3 + R2-P2-1..3 + R2-P3-1..2** | **open（§9）** |

### 0.2 R2 轮速览（本轮新增）

| 级别 | 条目 | 一句话 |
|------|------|--------|
| **P0** | **R2-P0-1** | REQ 消息基址 `FS_BASE=0x600` 与 C（com.h:589）/minix-fs（0xA00）不符——VFS↔FS wire 绝对值错误，注释的 C 锚点系伪造（§9.2；**✅ 已修复** 2026-09-09，§9.9 Fix #1） |
| P1 | R2-P1-1 | path.rs 生产单元的 Gate D 虚构抽象族：`DirectFetcher::fetch` 伪造返回 `"a".repeat`、`eat_path` 签名吃 `TestFproc`、`StrictResolver` 恒返回 vnode 99（§9.2） |
| P1 | R2-P1-2 | 执行绑定层缺失：64 个路由臂与决策函数之间不存在任何 match；`CallTable` 全 Some 制造"已实现"假象（§9.2） |
| P1 | R2-P1-3 | `sdev_stop` 驱动死亡级联缺失（sdev.c:912）：socket 驱动死亡时挂起进程永久悬挂（§9.2；**✅ 已修复** 2026-09-09，§10 Fix #11——sdev 侧停尸决策闭合，编排归 P1-2） |
| P1 | R2-P1-4 | route_message 的 BDEV/CDEV/SDEV RS 前缀判定用自认虚构值（0x500/0x600/0x700 + 0xFF00 掩码），C 真值为 `~0x7f` + 0x580/0x480/0x1980（§9.2） |
| **P0** | **R2-P0-2** ✅ | `copy_fd` 的 From/To 方向建模偏离 C 且 EDEADLK/CLOEXEC/`filp_ioctl_fp` 守门未建模——已修复 2026-09-09（§10 Fix #10：`CopyFdCtx` 注入 + kind 决定方向 + 三守门齐） |
| P2 | R2-P2-1 | ToErrno 统一映射通道未接入：30 个错误枚举 0 个 impl（P2-1 的修订方案）（§9.2；**✅ 已修复** 2026-09-09，§10 Fix #19） |
| P2 | R2-P2-2 | 00/99 骨架文档待按快照契约改写（本轮 Step 0.3 已生成 6 份 v1 快照）（§9.2） |
| P2 | R2-P2-3 | `do_gcov_flush` 缺 super_user 特权门（gcov.c:31；misc.rs 决策组四门齐、独缺此门）（§9.2；**✅ 已修复** 2026-09-09，§10 Fix #12） |
| P3 | R2-P3-1 | request.rs 计数注释漂移：33 常量 = 32 活 + 1 死，FsReq 32 变体与活类型双射（§9.2；**✅ 已修复** 2026-09-09，§10 Fix #2） |
| P3 | R2-P3-2 | device_map.rs 四源合一（dmap+smap+device.c ioctl 决策+mapdriver）的职责注记（§9.2） |
| edge | E-REQWIRE（新） | REQ_* VFS↔FS 共享契约双侧独立定义（vfs request.rs vs minix-fs protocol.rs）——收敛 minix-types 或建全量对账测试 |
| edge | E-VFSWIRE 增补 | VFS 侧 `VmVfsReq` 消息级解码未建；wire 定稿须以 C 绝对值断言（FS_BASE 0x600 教训） |

验证命令（2026-09-09 实测基线，与首轮 334 passed 一致，无回归；本轮零代码修改）：
- `cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib`：**334 passed / 0 failed**
- `cargo clippy --manifest-path os/Cargo.toml -p minix-vfs --lib`：40 条 warning 行，其中约 30 条落在 servers/vfs 自身文件（与首轮"59 行级告警中 21 条 profile 噪音"量级一致）

---

## 1. 存量 open 条目（首轮 26 条，2026-09-09 逐条复核）

> 复核方法（Step 0.7 staleness check）：每条 grep 现状 + 重读关键行；前置事实——`git log -- os/servers/vfs/` 最后一次提交为 2026-09-05（首轮之前），`git status --short os/servers/vfs/` 为空，即首轮后 VFS 代码零改动。结论：**24 条锚点原样维持；C-5、P2-1、P2-4 三处漂移修正如下；0 条失效**。条目正文与判定过程见 R1 存档。

### C-1～C-10（缺口表，R1 存档 §1）

逐条复核 ✅ 维持开口：SEF（minix-sef 仍 5 行）、clo_exec（exec.rs 对 cloexec 零消费）、invalidate 失效族（✅ by_char_major/by_sock_drv 已补，§10 Fix #6；by_endpoint 已修，Fix #5）、vmnt 锁升降级（仍只有 try_lock/unlock，vmnt.rs:72/:90）、fetch_vmnt_paths、path 循环（REQ_LOOKUP 于 request.rs:50 在 src 内无 request.rs 之外消费者）、mount_pfs/do_socketpath（DEFERRED 注释原样）、pm_reboot/unmount_all、ds_event/panic_hook、有意省略表未建。

**C-5 漂移修正**：`Vmnt` 已有 `mount_path: String` 字段（os/servers/vfs/src/vmnt.rs:111，对应 C vmnt.h:17 `m_mount_path`）——首轮"Vmnt 无路径字段"表述失实（该字段早于首轮存在）。缺口收窄为：**stadir.rs 的 `walk_plan`（stadir.rs:203 一带）与 getvfsstat 响应不产出路径**（`MountView` 仍只有 `in_use`/`canstat`）。修复时以本条为准，勿再扩表结构。

### P0-1 / P0-2 / P0-3（filedes.rs 三连，R1 存档 §2）

- ✅ P0-1 已修复 2026-09-09（§10 Fix #3）：filedes.rs:175 的 `Inval` 早退删除；**修复时修正首轮前提**——C 的 `get_filp2` 门（filedes.c:186-188）只对非 `OPCL` 访问返回 EIO（"disallow all use except close(2)"），close(2) 走 `VNODE_OPCL` 应**穿过 CLOSED 继续关闭**，故正确行为是放行而非返回 EIO（首轮"B 方案 FdError::Closed→EIO"被否决，详见归档条目的修正注）。测试 `test_close_eio` → `test_close_after_invalidate_proceeds`。
- ✅ P0-2 已修复 2026-09-09（§10 Fix #4）：`CopyKind::Close` 补 `filp_count > 1` 闸门——满足则 `dec_count` + 清 fd，否则 `EBADF`（filedes.c:636-646）；`copy_fd` 签名引入 `&mut FilpTable` 使计数操作可达；`From/To` 补 `inc_count`（filedes.c:652，count 配对是 Close 闸门的前提）。测试 `test_copy_close_last_reference_ebadf` 新增。修复期新登记 **R2-P0-2**（From/To 方向建模与 count 之外的 C 守门缺口）。
- ✅ P0-3 已修复 2026-09-09（§10 Fix #5）：`invalidate_by_endpoint` 引入 `&VnodeTable` 参数做 `v_fs_e` 探针——只失效属于该端点的 filp（filedes.c:298-306 逐字对应）；按 C 去掉 `mode != FILP_CLOSED` 排除（幂等重置合法）、双遍收敛单遍。测试改为三 filp 双端点矩阵（endpoint 5 失效 1 个、endpoint 6 失效 2 个、其余不动）。首轮"次选方案（Filp 冗余存 fs_endpoint）"否决：引入第二真相源。
- **附加清点**（首轮 §7 建议的全文件清点，仍未执行）：filedes.rs"自认偏离"注释共 6 处——:168（may_suspend）/ :180（dec_count simplified）/ :199/:207（invalidate 全失效）/ :234（cred.is_super）/ :269（Close 分支 just clear）。修 P0 三连时逐一消除，不留"修了行为留了假注释"。

### P1-1～P1-5（架构级，R1 存档 §3）

- P1-1 复核 ✅：run()（main_loop.rs:788 起）:786 "Currently a mock implementation"；dispatch()（:480）legacy 三路；:627 "Real dispatch will ENOSYS. Use Read as placeholder"；`route_message` 消费方仍只有 main_loop.rs 自身与测试。本轮下钻出新条目 R2-P1-2（绑定层），两者同点收敛。
- P1-2 复核 ✅（数字微漂）：DEFERRED 束原样；mfs 侧状态标记现为 37 个 = 12 Live（5 LiveInCrate + 7 LiveViaBlockTransfer）+ 25 PendingDocument（首轮口径 8 Live/23 Pending——mfs 不在 vfs 目录，确有演进），`fs_lookup` 仍 Pending（os/fs/mfs/src/table.rs:57）。"真瓶颈在 mfs 侧"结论维持。
- ✅ P1-3 已修复 2026-09-09（§10 Fix #8）：`PmHandler::fetch_group_list` 数据搬运口（`sys_datacopy_wrapper` 接缝，默认 fail-closed `ENOSYS`，通电挂 P1-2/E1）；`SETGROUPS` 臂补 `NGROUPS_MAX → EINVAL` 门（C 为 panic，fail-closed 偏差与 10-pm-protocol.md D4 的 EFAULT 决策同向）+ `group_no==0` 直清 + 正数路径经栈缓冲送真实列表。`PmError::NotImplemented` 变体新增。测试 +3（ENOSYS 预通电态/超限拒绝/零组直清）。
- ✅ P1-4 已修复 2026-09-09（§10 Fix #9）：`FilterOutcome::Query` 增 `clear_update`/`set_busy` 义务字段（select.c:517 清 UPDATE 在发送前、:522 置 BUSY 在成功后，socket 对位 :525-538），`filter_step` 恒置 true——义务进数据而非调用方记忆；23-select.md D3 同步并登记"只给 rops"的否决理由。
- ✅ P1-5 已修复 2026-09-09（§10 Fix #7）：`need_lock: bool` → `FilpLockMode { Opcl, None, ReadWrite }`——`Opcl` 过 `CLOSED` 门（close(2) 特权）、`None` 探测仍拒（C 的门覆盖一切非 `OPCL`，原 bool=false 比 C 宽的缺口闭合）、`ReadWrite` 拒 `CLOSED` 且取锁（filedes.c:186-193）。测试升级为三态矩阵。

### P2-1～P2-6 / P3-1 / P3-2（R1 存档 §4/§5）

- P2-1 **数字漂移修正**：现为 **30 个 `pub enum *Error` + 30 个 `fn to_errno`**（首轮 20/29，全 crate grep 实测）；两同名 `FdError` 仍在（filp.rs:263、filedes.rs:45）。方案已被本轮修订：否决"crate 级单一 VfsError"大收敛，改为接入 minix-types 的 `ToErrno` 通道——见 **R2-P2-1**。**✅ 已修复** 2026-09-09（§10 Fix #19）。
- P2-2 复核 ✅：call_table.rs 64 臂同构 match、`CallTable`:206、`NullResolver`:350 原样；本轮 R2-P1-2 给出它的终局（随绑定层落地删除）。
- P2-3 复核 ✅：9 个测试替身全部仍在 `#[cfg(test)]` 之前的生产单元（request.rs:492/bdev.rs:87/cdev.rs:60/sdev.rs:230/:276/socket.rs:232/:285/:345/fs_comm.rs:438，各文件 cfg(test) 起点在 :539/:321/:311/:599/:627/:493）。path.rs 的同族问题更严重，单列 R2-P1-1。**✅ 已修复** 2026-09-09（§10 Fix #17：11 个替身定义与 impl 全部 `#[cfg(test)]` 圈定，含 fcntl.rs:752 的 `ScriptedFcntl` 与 fs_comm 的 `TestTransIdCodec`——后者连带把 main_loop 的 re-export 拆为条件导出）。
- P2-4 **锚点漂移修正**：`NextFit` 现于 filedes.rs:91（首轮 :93），:70 新增 "O_DUPFD arg lower-bound variant" 辩护注释——仍无 C 来源，判定不变（Gate D 虚构第二实现，同族累积见 §9.6 Rule Discovery）。**✅ 已修复** 2026-09-09（§10 Fix #14：移 cfg(test) 更名 `NextFitDemo`，辩护注释的假语义一并修正——C 与 Linux 的 fd 分配都是 start 起最低空闲，不存在第二真实策略）。
- ✅ P2-5 已修复 2026-09-09（§10 Fix #15）：`device_map` 全线 `Option<i32>` 端点 → `Option<Endpoint>`（DmapEntry/SmapEntry 字段、driver_match/get_by_endpt/unmap_by_endpt/map_driver/check_mapper/EndpointDirectory/smap_by_endpt/smap_endpt_by_dev/RegisterPlan、CTTY_ENDPT/RS_PROC_NR 常量）；`filp::find_by_vnode(usize) → VnodeId`。bdev/cdev 各自决策函数的 i32 参数为 wire 边界，保持并注明。
- P2-6 复核 ✅：`trait TransIdCodec` 双定义仍在（fs_comm.rs:76 与 main_loop.rs:130）。**✅ 已修复** 2026-09-09（§10 Fix #13：收敛到 fs_comm 协议属主，main_loop re-export，-63 行重复）。
- ✅ P3-1 已修复 2026-09-09（§10 Fix #18）：vfs 自身 clippy 归零（lib + tests 双构建），并补修 Fix #17 遗留的三个未门控 impl（非 test 构建断裂）。
- P3-2 复核 ✅："有意省略表"仍未建立（99-global-concepts.md 零命中）；落点已随 R2-P2-2（99 改写）合并推进。

---

## 9. 第二轮（R2 轮，2026-09-09）：存量复核 + 协议面下钻 + 执行绑定层与抽象质量深查

### 9.0 Step 0 预检与 Gate A（证据摘录）

- **Step 0 硬阻断预检**：`.design/` 四类快照计数 = outline 31 / outline-review 31 / design 31 / design-final 0（本 stage 无 design-final 属正常，coverage-check 判据为前三类）；`tools/design-coverage-check.sh fork-syscall-rewrite --stage 05-stage-vfs` 首跑报 00-vfs-overview 与 99-global-concepts 三件套缺失（CRITICAL），已按 **Step 0.3 嵌入生成 6 份 v1 快照**（`.design/00|99-{outline,outline-review,design}.v1.md`，含"正文仍为骨架"的诚实声明），复跑输出 **ALL DOCS COMPLETE（33/33）**。
- **Gate A 覆盖穷举重跑**：`coverage-extract.py vfs ... --semantic-map tools/coverage-extract/vfs-semantic-map.json --output .review/claude/vfs/scans/SYMBOLS-r2.md` → 415 C 符号 / doc 387（93.3%）/ Rust name-match 262（63.1%），与首轮完全一致（与"首轮后零代码改动"互为佐证）。
- 基线命令与输出见 §0.2。

### 9.1 查漏结论（存量 26 条之外的新增缺口判定）

首轮缺口表之外，本轮对四个未下钻的面补扫，结论：

1. **device.c（95 行）——首轮未列缺口是对的，但理由要修正**：决策层全部有对应——`ioctl_route`（device_map.rs:575，对 do_ioctl 的块/字符/socket/ENOTTY 四分流，device.c:34-54）、`ioctl_access`（:590）与 `ioctl_size`（:603，对 make_ioctl_grant 的 IOR/IOW 方向与缓冲大小解码，device.c:76-82）；缺的只是 `cpf_grant_magic` 授权创建与真实驱动往返（属 P1-2 内核 IPC 束），不新立条目。
2. **gcov.c（73 行）——一条小缺口**：misc.rs 的 gcov 决策组四门齐（`gcov_label_gate` :555、`gcov_endpt_ok` :566、`gcov_grant_outcome` :575、`gcov_target` :593），独缺 C gcov.c:31 的 `super_user` 特权门 → 新立 **R2-P2-3**。
3. **sdev.c（1114 行）↔ sdev.rs（838 行）逐函数对位**：25 个 C 函数中 24 个有决策/编码半对应（多为合并建模，如 `SdevOp` 枚举吸收 bind/connect/listen/accept 族），唯一 ❌ 是 **`sdev_stop`（sdev.c:912）驱动死亡级联** → 新立 **R2-P1-3**。其余差距是执行半（真实传输/等待/复活），归 P1-2 矩阵，不重复登记。
4. **64 个 VFS 调用号三层矩阵**：C handler（table.c:18-82）↔ 枚举/路由臂（call_table.rs:28-93 + main_loop.rs:622-631）64/64 齐；决策函数层 62 个 ✅、2 个 ⚠️（GcovFlush 缺特权门 → R2-P2-3；Mapdriver 只有 dmap 标签决策无重启执行 → P1-2 束）+ Socketpath 文档 pending（C-7 已登记）。**但发现系统性断点：路由臂与决策函数之间不存在绑定 match** → 新立 **R2-P1-2**。
5. **VFS↔mfs 协议双侧对账**：REQ_* 消息在 VFS 侧（request.rs:15/:25-57，FS_BASE=0x600）与 FS 侧（os/libs/minix-fs/src/protocol.rs:25，FS_BASE=0xA00）**独立定义、无编译期或测试期联动**；且 VFS 侧基址与 C 不符 → 新立 **R2-P0-1**（数值面，stage 内修）+ **edge E-REQWIRE**（结构面，跨 crate 收敛）。mfs 现状：37 状态标记 = 12 Live + 25 Pending，`fs_lookup` 仍 Pending（table.rs:57），P1-2 结论维持。
6. **VFS 对内核调用依赖**：`sys_hz`/`sys_safecopy*`/`sys_datacopy*`/`sys_getregs` 等在 src 内全部为注释级 defer（main_loop.rs:393-400、select.rs:24、path.rs:171/:205、coredump.rs:20、ipc/dispatcher.rs:125），全部可归 P1-2"内核 IPC 原语束"，无束外新依赖。

### 9.2 本轮新条目

#### ✅ R2-P0-1（P0-code-bug）REQ 消息基址 `FS_BASE=0x600` 与 C/minix-fs 的 `0xA00` 不符——wire 绝对值错误，C 锚点系伪造——已修复 2026-09-09（§9.9 Fix #1）

- **Rust 现状**：`pub const FS_BASE: u32 = 0x600;`（os/servers/vfs/src/request.rs:15），注释自称 "`FS_BASE 0x600` — `vfsif.h:40`"；全部 REQ_* 常量（request.rs:25-57）与 `is_fs_rq`（:20-22，`(raw & !0xff) == FS_BASE`）以 0x600 为基。request.rs:854-856 的测试断言 `msg_type == REQ_LOOKUP` 且 `is_fs_rq` 通过——**测试与错误常量自洽**，属"测试自身正确性"问题。
- **C 行为**（Ground Truth）：FS_BASE 定义在 `minix3/minix/include/minix/com.h:589` = **0xA00**（注释"Requests sent by VFS to filesystem"）；REQ_* 定义在 `minix3/minix/include/minix/vfsif.h:41-73`（该文件**不含** FS_BASE 定义，:40 亦非——Rust 注释的锚点与数值双双失实）。REQ_LOOKUP 绝对值 = 0xA00+26 = **0xA1A**。
- **对端证据**：`os/libs/minix-fs/src/protocol.rs:25` `pub const FS_BASE: i32 = 0xA00;`（注释带正确 C 锚点 com.h:589）；mfs 经 `minix_fs::protocol::RequestNumber` 分发（os/fs/mfs/src/table.rs:12）。两侧偏移对齐（26=Lookup）掩盖了基址分歧。
- **后果与可达性**：当前不可达（run() mock、无真实传输）；接线后 VFS 发出的每个 REQ 消息（0x600+n）都不会被按 C 常量实现的 FS 服务器识别，FS 通信全断；反向亦然。同型先例：edge T32（VM 侧 transid clean_type 拒真消息，Fix #52）、E-VMMCPWIRE（vmmcp 字宽截断）——wire 常量必须与 C 绝对值对齐，项目已两度付学费。
- **修改方案**（≥2 候选）：
  - **A（选定）**：request.rs:15 改 `0xA00`，注释锚点改 com.h:589；测试升级为 C 绝对值断言（`assert_eq!(REQ_LOOKUP, 0xA1A)` 型），杜绝再次自洽式回归。一处常量 + 注释 + 测试，立即止血。
  - B：REQ 常量整体迁 minix-types 共享契约（与 edge E-REQWIRE 合流）。结构更优但属跨 crate 收敛，按 edge 单线程执行；A 先行与 B 不冲突。
- **验证**：`grep -rn "0x600" os/servers/vfs/src` 归零；新绝对值断言入测；`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` 全绿；对端 protocol.rs 不动。
- **边界**：与 P1-2（REQ 协议面）、edge E-REQWIRE 交叉；修 A 时勿动 FsReq 变体结构（那是 E-REQWIRE 范围）；同文件注释漂移顺带修 R2-P3-1（fix-guard 一次一条，分两批）。

#### ✅ R2-P0-2（P0-design-deviation）`copy_fd` 的 From/To 方向建模偏离 C，EDEADLK/CLOEXEC/`filp_ioctl_fp` 守门缺失——已修复 2026-09-09（§10 Fix #10）

- **Rust 现状**（Fix #4 后）：`From`/`To` 两分支行为仍相同——都从 `src` 读 filp、向 `dst` 分配（copy_fd，filedes.rs:237-290 一带），而 C 的方向由 `what` 决定：`COPYFD_FROM` 从**远端**读、写入**调用者**表（filedes.c:600-602 `rfp = fp` 重定向），`COPYFD_TO` 从**调用者**读、写入**远端**表（filedes.c:568 `get_filp2((what == COPYFD_TO) ? fp : rfp, ...)`）——即同一对 `(src, dst)` 参数在两种 kind 下语义应互换，当前不互换。count 配对已由 Fix #4 补上（`inc_count`/Close 闸门），但 `COPYFD_CLOEXEC` 剥离（filedes.c:600）、`S_ISSOCK` 自复制 `EDEADLK`（filedes.c:606-613）、`filp_ioctl_fp == rfp → EBADF`（filedes.c:582-585，VND IOCTL 死锁防护）均未建模（注释自认 DEFERRED）。
- **C 行为**（Ground Truth）：filedes.c:524-650 如上；`COPYFD_CLOSE` 的注释明言"只用于撤销一次成功的 copy-to，且假定调用者自己仍持有引用"。
- **后果与可达性**：接线后 UDS 的 fd 传递（SCM_RIGHTS 型）与 VND 的 fd 注入方向会接错——把 fd 复制到错误的进程表；VND IOCTL 自引用场景缺 EBADF 防护会死锁。
- **修改方案**：
  - **A（选定）**：签名按角色重排为 `copy_fd(caller: &mut FProc, remote: &mut FProc, fd: Fd, kind, ...)`，方向由 `kind` 决定（From：读 remote 写 caller；To：读 caller 写 remote），补齐 CLOEXEC 剥离、EDEADLK 决策件（`S_ISSOCK && smap_endpt == caller_endpoint`）、`filp_ioctl_fp` 探针。
  - B：保留 src/dst 语义、文档声明"调用方按 kind 交换参数"。否决：把 C 的方向正确性外包给每个调用点的纪律，是埋雷。
- **验证**：方向矩阵测试（From/To × 断言读侧不变、写侧获得 filp + count 增长）；EDEADLK 决策函数对 `S_ISSOCK` filp 返回 EDEADLK 的单测。
- **边界**：Fix #4（count 配对已落地，本条完成后 COPYFD 族闭合）；14-filedes.md D5 已按本条登记改写；与 sdev/uds 的 `smap_by_endpt`（device_map.rs:496）联动。

#### R2-P1-1（P1-design-wrong）path.rs 生产单元的 Gate D 虚构抽象族：伪造数据的 trait impl 与以测试类型命名的签名

- **Rust 现状**（全部位于 `#[cfg(test)]`（path.rs:300 起）之外的生产单元）：
  - `PathFetcher`（path.rs:172-175）的生产 impl `DirectFetcher::fetch` 返回 `Ok("a".repeat(len - 1))`（path.rs:186）、`SafecopyFetcher::fetch` 返回 `"b".repeat(...)`（:207，注释自认 "always succeed for test"）——**生产编译单元内伪造用户路径数据**。
  - `PathResolver` trait（:250-253）的 `eat_path` 签名直接吃 `TestFproc` 类型（:255-260，pub 于生产单元）；`StrictResolver::advance` 恒返回 vnode 99、`PermissiveResolver` 恒返回 42（:271/:288）。
  - `SlashHandler` trait（:221-223）注释自认 "Gate D requires 2 behaviourally different impls"。
- **C 行为**：path.c 的 advance/eat_path/last_dir/get_name/canonical_path 是操作 vnode/vmnt 表与 REQ_LOOKUP 往返的实函数（path.c:384/:146/:594/:648）；C 无 resolver/slash-handler 抽象。
- **后果与可达性**：与首轮 P2-3 的替身不同，这批类型名字不带 Mock/Scripted、**伪装成生产抽象**，且 fetch 伪造数据——C-6 接线若误选 DirectFetcher 作生产 fetcher（trait 多 impl 编译不报错），用户路径静默变成 "aaa…"。同族先例：P2-4（NextFit）、P2-2（NullResolver），系统性结论见 §9.6。
- **修改方案**：
  - **A（选定）**：C-6 重设计时删除 `PathResolver` 与 `SlashHandler`——advance/eat_path/last_dir/get_name/canonical_path 以实函数实现（吃 `&VfsState` 表 + FsComm + transport）；`PathFetcher` 保留 seam（Direct/Safecopy 是 C 的 cpf_grant 语义二分，真实存在），但生产 impl 走 transport 的 sys_safecopy 等价物，现伪造 impl 移入 cfg(test) 并更名（如 `FakeFetcher`）；`TestFproc` 并入测试模块。
  - B：仅把伪造 impl 移 cfg(test)、保留全部 trait。否决：为 Gate D 保留无生产语义的空壳抽象是模式 80（为 mock 预建抽象）。
- **验证**：`grep -n "Gate D" os/servers/vfs/src/path.rs` 归零；`repeat(` 只出现在 cfg(test) 内；C-6 落地后的路径往返集成测试（含跨挂载 EnterMount/LeaveMount 转移）。
- **边界**：C-6（同文件、同次设计）、P2-3（替身批处理）、R2-P1-2（决策函数签名是 64 臂绑定的前置）；一次打开 path.rs 设计到位。

#### R2-P1-2（P1-design-missing）执行绑定层缺失：64 个路由臂与决策函数之间不存在任何 match

- **Rust 现状**：`VfsCallNum` 64 变体齐全（call_table.rs:28-93），`route_message` 把解析成功的调用号统一送 `Route::Syscall{call}`（main_loop.rs:622-631）；但全 crate 不存在 `VfsCallNum → 决策函数` 的分发 match（`VfsCallNum::` 的消费只有枚举定义、from_raw/try_from_raw 与 CallTable 装配）；worker.rs:51 `WorkerFunc::DoWork` 是唯一 syscall 执行臂占位（注释 "Normal syscall path (do_work, table.c:call_vec)" 无实现）。`CallTable::new` 把 64 个全部装配为 `Some`（call_table.rs:217-282），制造"已实现"外观——lookup 全 Some ≠ 能执行。
- **C 行为**：table.c:18-82 `call_vec` 64 项函数指针，`do_work` 经 `(*call_vec[call_index])()` 直达 handler——C 的绑定即表本身。
- **后果与可达性**：首轮 P1-1 说"两套分发契约"，本轮下钻一层：即使 `route_message` 胜出，它到 64 个决策函数之间仍是断的。`CallTable` 的 `Option<VfsCallNum>` 数组信息量等于 `from_raw`（P2-2 已判冗余），其存在掩盖绑定缺失。
- **修改方案**：
  - **A（选定）**：接线时落单一 `dispatch_syscall(state: &mut VfsState, call: VfsCallNum, msg: &Message) -> SyscallResult` 的**穷举 match**（64 臂直达各模块决策函数，无通配臂）；`CallTable`/`CallResolver`/`NullResolver` 随之删除（Gate D 双实现由穷举 match + 测试替身函数满足）；route_message 的 Read 占位符（P1-1）同点收敛为 `Route::Enosys`。C 函数指针表在 Rust 的自然对应就是穷举 match（ARCH A-2 完成态）。
  - B：保留 CallTable 作"合法性预检"、match 只处理 Some 分支。否决：预检与穷举 match 重复，两层机制表达一件事。
- **验证**：`grep -rn "struct CallTable" os/servers/vfs/src` 归零；match 无 `_` 通配臂（编译期穷举）；`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` 全绿。
- **边界**：P1-1（同点收敛）、P2-2（本条闭合它）、R2-P1-1（path 决策函数签名前置）；`SyscallResult`/`Route` 类型设计本身保留（首轮 §7 已肯定）。

#### ✅ R2-P1-3（P1-design-missing）`sdev_stop` 驱动死亡级联缺失——已修复 2026-09-09（§10 Fix #11；sdev 侧决策闭合，运行时编排归 P1-2）

- **Rust 现状**：sdev.c 25 函数对位中唯一 ❌。Rust 侧 `ChannelEvent::Dead`（sdev.rs:217）只是测试脚本事件；select 维度的死亡唤醒有 `unsuspend_hit`（select.rs:719），sdev 维度（挂起在 SDEV_CANCEL/读写/accept 上的 slot）无级联；上游触发点 `smap_by_endpt`/`unmap_by_endpt`（device_map.rs:496 一带）本身也未接线。
- **C 行为**：sdev.c:912 `sdev_stop`——驱动死亡时遍历挂起 socket 请求，回 EIO 并复活；与 C-3 的 `invalidate_filp_by_char_major`/`by_sock_drv`（filedes.c:260/:277）同属"驱动死亡级联"族。
- **后果与可达性**：接线后 socket 驱动崩溃 → 所有挂起在 socket 系统调用上的进程永久悬挂（无超时、无唤醒）。Redox 同题教训（daemon 死后请求悬死）见 R1 存档 §6.3。
- **修改方案**：
  - **A（选定）**：并入 C-3 + P0-3 的"失效族"一次设计——按 C 语义建统一的 `driver_death_cascade(endpoint)`：filedes 失效（by_char_major/by_sock_drv）+ sdev stop（挂起 slot 回 EIO 复活）+ select 唤醒三面共享同一触发事件，入口挂 dmap/smap unmap。
  - B：只补 sdev_stop 单函数。否决：死亡级联三面共享触发序，分开设计必然漂移。
- **验证**：驱动死亡注入测试（ScriptedChannel 发 Dead 事件 → 断言挂起 slot 收 EIO 并复活、select 维度同步唤醒）。
- **边界**：C-3、P0-3、P1-2 接线矩阵；与 select.rs `unsuspend_hit` 语义对齐，勿两处各写一份唤醒。

#### R2-P1-4（P1-design-wrong）route_message 的 BDEV/CDEV/SDEV RS 前缀判定使用自认虚构值——通电后驱动回复全部失路由

- **Rust 现状**：`is_bdev_rs`/`is_cdev_rs`/`is_sdev_rs`（main_loop.rs:547-559）用 `(raw & 0xFF00) == 0x500/0x600/0x700` 判定，:548-551 注释自认 "the exact base values are not needed for the routing priority test — we model them as distinct high-byte prefixes"；测试 main_loop.rs:1212/:1218 按假值断言（"matches is_bdev_rs stub"）。
- **C 行为**（Ground Truth）：`CDEV_RS_BASE 0x480`（com.h:919）、`BDEV_RS_BASE 0x580`（com.h:963）、`SDEV_RS_BASE 0x1980`（com.h:1038），掩码为 `~0x7f` 而非 `0xFF00`（com.h:922-923 等 `IS_*_RS(type) (((type) & ~0x7f) == *_RS_BASE)`）。
- **后果与可达性**：`route_message` 是 P1-1 选定的唯一生产契约（§9.8 第 4 步），E1 通电后真实的 `CDEV_REPLY`（0x480 起）不会命中 Cdev 臂、`BDEV_REPLY`（0x580 起）不会命中 Bdev 臂——设备回复全部失路由；当前仅测试自洽不可达。发现渠道：R2-P0-1 修复时的 fix-guard 残留 grep。
- **修改方案**：
  - **A（选定）**：三判定改真值——`(raw & !0x7f) == 0x580/0x480/0x1980`，测试用真 CDEV_REPLY/BDEV_REPLY 消息断言路由臂（与 R2-P1-2 分发收敛同轮做，route_message 转正时一并落）。
  - B：保留教学占位 + DEFERRED 标注。否决：route_message 已被选为唯一契约，契约上的占位判定就是错误契约。
- **验证**：真值消息（如 `m_type = BDEV_RS_BASE + BDEV_REPLY`）路由到 Bdev 臂；`grep -n "0x700\|0x500" os/servers/vfs/src/main_loop.rs` 在路由判定处归零。
- **边界**：R2-P1-2（同轮）、P1-1；与 request.rs 的 FS_BASE 无数值冲突（0xA00 & !0x7f = 0xA00，与三个 RS 基址互异）。

#### ✅ R2-P2-1（P2）ToErrno 统一映射通道未接入——已修复 2026-09-09（§10 Fix #19）

- **Rust 现状**：`pub enum *Error` 30 个、固有 `fn to_errno` 30 个（P2-1 复核更新后的数字）；minix-types 已落 `ToErrno` trait（`os/libs/minix-types/src/types/errno.rs:507`，返回 `Errno` newtype；PmError/KernelError 已 impl——commit 893386cd8 "D1/D2 落地"）；os/servers/vfs 对 `ToErrno` **零匹配**。两同名 `FdError` 仍在（filp.rs:263、filedes.rs:45）。
- **方案修订**（取代 R1 存档 §4 P2-1 的"crate 级单一 VfsError"首选）：02-stage-vm 同题判定先例（edge T8，Fix #27）= 不做大收敛、"From 集中表即最优"。VFS 对应动作：① 30 个枚举逐一 `impl ToErrno`（新 trait 方法委托既有固有方法，机械）；② 消费端统一 `ToErrno::to_errno(&e).to_i32()`；③ filp.rs 的 `FdError` 改名或并入 `FilpError`，消除重名。
- **验证**：`grep -rn "impl ToErrno" os/servers/vfs/src | wc -l` ≥ 30；`grep -rn "enum FdError" os/servers/vfs/src | wc -l` = 1。
- **边界**：P0-1（`FdError::Closed` 新变体直接落在这套通道上）、P1-5（`FilpLockMode` 三态化同文件先行）。

#### R2-P2-2（P2 doc）00/99 骨架文档待按快照契约改写

- **现状**：`00-vfs-overview.md` 22 行（:3 状态 pending 最小骨架）、`99-global-concepts.md` 骨架；本轮 Step 0.3 已生成 `.design/00|99-{outline,outline-review,design}.v1.md` 六份目标契约（含"正文仍为骨架"诚实声明）。02-stage-vm 同型条目 G-V12-13 先例。
- **改写要求**：00 按快照 Ch1-Ch7 展开启动主线叙事（mthread→A-1 的"演进而非退化"论证须带 R1 存档 §6.5/6.7 的 Redox 事实锚点）；99 定稿时一并落 P3-2/C-10 的"有意省略表"与引用计数双层不变量（filp_count ↔ v_ref_count ↔ v_fs_count——它是 C-3/P0-3 失效族的正确性基础）。正文改写后快照升 v2 复审。
- **验证**：plan.md §6 实施路线两行"骨架"状态翻转；coverage-check 复跑仍 ALL PASS。

#### ✅ R2-P2-3（P2）`do_gcov_flush` 缺 super_user 特权门——已修复 2026-09-09（§10 Fix #12）

- **Rust 现状**：misc.rs gcov 决策组四门齐——`gcov_label_gate`（:555，顺带修复并注释了 gcov.c:39-44 的 labellen==0 越界 bug）、`gcov_endpt_ok`（:566）、`gcov_grant_outcome`（:575）、`gcov_target`（:593）；独缺 C gcov.c:31 的 `super_user` → EPERM 门。
- **C 行为**：gcov.c:10-73 `do_gcov_flush` 第一步特权检查。
- **后果与可达性**：接线后非 root 进程可触发 gcov flush（信息面/干扰面）。
- **修改方案**：**A（选定）**——加 `gcov_privilege_gate(caller) -> Result<(), GcovError>` 决策函数（照 protect.rs `in_group` 的决策模式，测试直调）；B——并入调度层统一特权检查。否决 B：C 是 per-call 门，位置语义要保真。
- **验证**：非特权 caller 决策函数返回 EPERM 的单测；`grep -n "super_user\|EPERM" os/servers/vfs/src/misc.rs` 命中新函数。

#### ✅ R2-P3-1（P3）request.rs 计数注释漂移——已修复 2026-09-09（§10 Fix #2，真相比登记更深一层：33 常量 = 32 活 + 1 死，32 变体与活类型双射）

模块头（request.rs:6）与 `FsReq` 定义处（:120）注释称 "33 variants / 33 live variants"，实际 32 变体（本枚举 awk 计数）；`NREQS=34`（:17）对照 C `minix3/minix/include/minix/vfsif.h:75`（NREQS 34，含死 REQ_GETNODE）正确。随 R2-P0-1 同文件分两批顺带修。

#### ✅ R2-P3-2（P3）device_map.rs 四源合一的职责注记——已修复 2026-09-09（§10 Fix #16）

device_map.rs（906 行）聚合 dmap.c（:69-230）、smap.c（:307-560）、device.c 的 ioctl 决策（:561-613）、mapdriver 服务分类（:276-305）四个 C 来源。聚合不违反语义，但 ioctl 决策的家与 C 的 device.c 文件错位，按 C 索引找不到。方案：拆 `device.rs`（ioctl_route/ioctl_access/ioctl_size 三函数）或模块头加"来源映射注记"。P2-5 类型化落地时顺带定夺，不单开一轮。

### 9.3 复核认定无缺口的面（防重复扫描）

1. device.c 决策层完整（§9.1 第 1 条）——后续轮次勿再登记 device.c 缺口；其执行半归 P1-2。
2. 64 调用号：枚举/路由/决策三层 64/64/62+2⚠️（§9.1 第 4 条）；无空决策函数；唯一结构性断点 = R2-P1-2。
3. 12 个 VFS_PM 请求覆盖维持（ipc/dispatcher.rs），唯一语义洞仍是 P1-3。
4. FsReq 32 变体**名字面**与 C REQ_* 对齐维持（绝对值问题 = R2-P0-1，结构收敛 = E-REQWIRE）。
5. 内核调用依赖全部归 P1-2 束，无束外新依赖（§9.1 第 6 条）。
6. 首轮正面评价维持：worker.rs 的 ARCH A-1 论证（Linux workqueue/Redox async/seL4 对照，worker.rs:1-33）、fs_comm GlobalComm 窗口、pipe.rs 的 SuspCount/WakePlan 分解、select.rs 决策纯化、socket.rs BuildStep/compensate。

### 9.4 Redox/Linux 对照增补（更新 R1 存档 §6）

1. **cancellation 升格为 crate API**：redox-scheme 现把 `CallerCtx` 与 `CancellationRequest` 作为一级 API 类型（docs.rs/redox-scheme，2026-09-09 查证；crate 2024 edition、持续更新）。R1 §6.3 的"取消是后补教训"由此获得 API 层佐证——P1-2 与 R2-P1-3 的关闭条件必须含"调用者先死"分支（在途 slot 取消），不止"对端驱动死亡"。
2. **Linux 跨挂载单点收口**（[外部参照] fs/namei.c 的 follow_automount/step_into 机制，不给行号）：跨挂载切换收在路径行走的一处状态转移。C-6 设计对照：`LookupRes::EnterMount/LeaveMount`（path.rs:120-127）已是正确方向——REQ_LOOKUP 循环实现时跨挂载判断只允许出现在这一处状态转移，勿散落多处。

### 9.5 edge 增补指针

- **E-REQWIRE（新登记）**：REQ_* VFS↔FS 共享契约双侧独立定义（vfs `request.rs` FS_BASE=0x600 错值 + 32 变体枚举 ↔ minix-fs `protocol.rs` 0xA00 + `RequestNumber`）——收敛方案 A：REQ face 迁 minix-types（PM face 先例 `ipc/vfs.rs`），VFS 与 minix-fs 共消本地常量；方案 B：最低限度全量对账测试（`FsReq::m_type()` ↔ `RequestNumber` 逐项相等）。R2-P0-1 的常量止血不依赖本条。正文见 edge_todo.md。
- **E-VFSWIRE（增补）**：VFS 侧 `VmVfsReq` 仅有决策原语（misc.rs:262-281），消息级解码未建；wire 定稿时以 C 绝对值断言（R2-P0-1 教训）；VM 侧发送半状态不变（vfs_queue.rs:142 注释仍挂本条）。

### 9.6 Rule Discovery（Step 5.7）

**✅ 发现新模式**：**Gate D 双实现压力产物（虚构第二实现族）**。

- 案例累积 4 组：`NextFit`（filedes.rs:91，R1-P2-4）、`NullResolver`（call_table.rs:350，R1-P2-2）、`StrictResolver`/`PermissiveResolver`+`TestFproc`（path.rs:262-298，本轮 R2-P1-1）、`SlashHandler`+`TestTransIdCodec`（path.rs:221、fs_comm.rs:100，本轮）。
- 共同特征：trait 文档注释自认 "Gate D requires/satisfies ≥2 behaviourally different impls"；第二 impl 无 C 来源或伪造行为；位于生产编译单元。
- 严重度：P2（普通冗余）～P1（伪装成生产抽象且有伪造数据 impl 时，如 R2-P1-1）。
- 归类建议：作为 review-patterns 模式 80/81（为 mock 预建抽象/生产 mock 态）的具名子型登记，或新立模式 84。
- 规则草案：Gate D 的"trait ≥2 行为不同 impl"必须以 C 语义真实存在的行为差异为准；检查清单新增机械检法 `grep -rn "Gate D" os/servers/*/src`——命中的 trait 逐个复查第二 impl 的 C 来源，无来源 → 移 cfg(test) 或删除。规则文件修订留待规则维护会话，本轮先以本条登记。

### 9.7 gate-evidence 与收敛评估

```
gate-evidence-Step0:
$ ls notes/rewrite/fork-syscall-rewrite/05-stage-vfs/.design/*-outline.v*.md | wc -l   → 31
$ ls .../*-outline-review.v*.md | wc -l  → 31
$ ls .../*-design.v*.md | wc -l          → 31
$ ls .../*-design-final.v*.md | wc -l    → 0（本 stage 无此件，判据为前三类）
$ tools/design-coverage-check.sh fork-syscall-rewrite --stage 05-stage-vfs
  首跑：00/99 三件套缺失 CRITICAL → Step 0.3 生成 6 份 v1 快照 → 复跑：ALL DOCS COMPLETE (33/33)
gate-evidence-A:
$ python3 tools/coverage-extract/coverage-extract.py vfs notes/rewrite/fork-syscall-rewrite/05-stage-vfs \
    --rust-dir os --c-dir minix3/minix/servers/vfs \
    --semantic-map tools/coverage-extract/vfs-semantic-map.json \
    --output .review/claude/vfs/scans/SYMBOLS-r2.md
  Total C symbols: 415 / Doc covered: 387 (93.3%) / Rust covered: 262 (63.1%)  ← 与首轮完全一致
gate-evidence-baseline:
$ cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib    → 334 passed / 0 failed
$ cargo clippy --manifest-path os/Cargo.toml -p minix-vfs --lib  → 40 条 warning 行（约 30 条落 servers/vfs）
```

- staleness 通道：26 条逐条 grep/sed（并行探查 agent 执行 + 主会话对承重锚点抽验：P0-1/P1-1 的 filedes/main_loop 锚点、path.rs 全文、FS_BASE 三方链条均主会话亲验）。
- **收敛评估**：本轮为 VFS 第 2 轮。新发现加权（P0×10 + P1×3 + P2×1 = 10+12+3 = 25，R2-P1-4 为修复期新登记），相对首轮（3 P0 + 5 P1 + 6 P2 = 30+15+6 = 51）约 49%，远超"新发现 <20% 停止"阈值；且首轮后尚无修复轮。**不触发收敛停止，继续轮次有充分空间**。

### 9.8 建议的推进顺序（合并首轮 §8 修订）

1. **R2-P0-1**（一处常量 + 锚点 + 绝对值断言，止血 wire）→ 顺带 **R2-P3-1** 注释（同文件分批）。
2. **P0-1/P0-2/P0-3 + C-3 + R2-P1-3 + R2-P0-2**：失效/死亡级联族与 COPYFD 方向建模一次设计（filedes.rs + sdev.rs 两文件，filedes 的 6 处自认偏离注释逐一消除）。
3. **R2-P1-1 + C-6**：path.rs 真实设计（删 Gate D 虚构抽象 + 跨 FS 往返循环落地 + PathFetcher 生产 impl 接 transport）。
4. **P1-1 + R2-P1-2 + R2-P1-4**：分发收敛（route_message 唯一契约 + `dispatch_syscall` 穷举 64 臂 + `Route::Enosys` + RS 前缀真值化 + 删 CallTable/CallResolver/dispatch legacy + run_once 可注入入口，对标 PM 的 run_once_integration 测试形态）。
5. **P1-3/P1-4/P1-5 + R2-P2-1**：语义洞修复与 ToErrno 通道接入。
6. **P2-2/P2-3/P2-5/P2-6 + P3-1 + R2-P3-2**：机械清理批；**R2-P2-2**（00/99 改写，落省略表）。
7. **edge 单线程执行**：E-REQWIRE（REQ 契约收敛 minix-types 或对账测试）→ 随 E1 通电后 E-VFSWIRE 定稿。

---

## 10. 修复记录（Fix #N campaign，一次一条，修前 fix-guard、修后回归 review）

### ✅ Fix #1: R2-P0-1 — REQ 消息基址 0x600→0xA00 对齐 C/minix-fs（2026-09-09）

- **File**：`os/servers/vfs/src/request.rs`（:6 模块注释 / :14-19 FS_BASE 常量与文档 / 测试区）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/12-request-wrappers.md`（13 处数值与锚点）。
- **Before**：`pub const FS_BASE: u32 = 0x600;`，注释锚 `vfsif.h:40`（该行是注释行，非定义；FS_BASE 实定义于 com.h:589=0xA00）；测试以 0x600 自洽断言（request.rs 原 ：854-856 一带）；`test_nreqs_getnode_dead` 用错值 0x601；12-request-wrappers.md 十余处 0x601/0x60B/0x621/0x600 及 `vfsif.h:40` 伪锚点。
- **After**：`FS_BASE = 0xA00`，注释锚 `com.h:589` 并注明 vfsif.h 仅引用、FS 侧（minix-fs protocol.rs）按 0xA00 分发；新增 `test_fs_wire_values_match_c_absolute`（绝对值 pin：REQ_GETNODE=0xA01/REQ_READ=0xA13/REQ_LOOKUP=0xA1A/REQ_BPEEK=0xA21 + 0x600 旧基址回归拒绝 + 与 CDEV_RS/BDEV_RS 命名空间互异）；文档全部数值与锚点同步为 0xA 系。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **335 passed / 0 failed**（334→335，+1 新测试）；`grep -rn "0x600" os/servers/vfs/src` 仅余回归守卫断言（request.rs:568-569）与已登记的 is_cdev_rs 占位（main_loop.rs:555/:1218，新条目 R2-P1-4）；文档 grep 0x6 系旧值零残留。
- **回归 review**：测试名对账——12-request-wrappers.md §5 测试表已增补新测试行；fix-guard 残留清查牵出 R2-P1-4（RS 前缀虚构值），已登记不顺手修。

### ✅ Fix #2: R2-P3-1 — REQ 计数三重校准：33 常量 = 32 活 + 1 死（2026-09-09）

- **File**：`os/servers/vfs/src/request.rs`（:6/:30/:125 三处注释）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/12-request-wrappers.md`（12 处计数与 1 处虚构测试名）。
- **Before**：代码注释称 "33 variants / 33 live variants / 33 live + 1 dead"；文档称 "33 变体 / 33 有效请求 / 33 包装 / 33 函数"，且 §3 D6 引用不存在的测试名 `test_dead_getnode_is_unknown`（Gate E 违规）。
- **After（真相）**：`vfsif.h:41-73` 定义 **33 个常量**（`FS_BASE+1..+33`），其中 `REQ_GETNODE` 死 → **32 个活类型**；`FsReq` **32 变体与活类型一一对应**（`NREQS 34` 是表容量冗余，非活类型数）；`request.c` 的 `req_*` 函数实为 **36 个**（含 `_actual` 重试后半，`grep -oE "\breq_[a-z_0-9]+\(" | sort -u` 实测）——文档三处"33 函数"一并校准；虚构测试名改为真实存在的 `test_nreqs_getnode_dead`。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **335 passed / 0 failed**（注释级修改，无行为变化）；文档 grep 无残留错误计数。
- **回归 review 与 workspace 事件**：修复期间并行会话对 `os/libs/minix-types/src/ipc/vm.rs` 的半成品删除（`VmExecNewmemOut` 类型已删、:938 impl 与 ：1608 测试引用未删）卡死全 workspace 编译 E0425 约 7 分钟——本次按其删除意图补完收尾（仅删悬空代码，不改其它语义），该文件**不并入本提交**（留给其所属会话）；这是共享工作树的已知风险，用户约定跨 stage 条目走 edge_todo 单线程执行正是为规避此类冲突。

### ✅ Fix #3: P0-1 — close_fd 删除 FILP_CLOSED 早退，OPCL 放行继续关闭（2026-09-09）

- **File**：`os/servers/vfs/src/filedes.rs`（close_fd :166-190 + 模块注释 + 测试）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/14-filedes.md`（标题/intro/§1.4/小结/D4/测试表 7 处）。
- **Before**：`close_fd` 对 `filp.mode == FILP_CLOSED` 返回 `FdError::Inval`，注释自认 "EIO mapped to Inval for test"。
- **Ground Truth 复核（关键）**：C `get_filp2`（filedes.c:186-188）的 EIO 门带 `locktype != VNODE_OPCL` 前置——close(2) 的 `OPCL` 路径**穿过** `FILP_CLOSED`（`open.c:696-704`：清 fd → `close_filp` → FD_CLR）；EIO 属 read/write 等非 `OPCL` 访问。首轮条目"建议 1（FdError::Closed→EIO）"会引入新偏离（C 成功处返回 EIO、泄漏描述符），按反查原则以 C 为准否决。
- **After**：close_fd 无条件放行（EBADF 探针保留：fd 未开或 filp 槽失效仍拒），清 fd + cloexec + `dec_count`；EIO 语义归属 P1-5 的 `FilpLockMode` 接缝（`filp.rs` 已有 `Closed → EIO`）。测试 `test_close_eio`（断言 EINVAL）→ `test_close_after_invalidate_proceeds`（断言 Ok + fd 清空 + count 归零）；14-filedes.md 的标题、intro、§1.4、小结、D4、测试表同步（D4 原描述的 `allow_closed=true` 分支设计一并修正为"无条件放行"）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **335 passed / 0 failed**；`grep -n "Inval" os/servers/vfs/src/filedes.rs` 在 close_fd 无命中；文档 `mode==FILP_CLOSED→EIO` 仅存于描述 C 的 `get_filp2` 门处（合法）。
- **边界**：P1-5（`FilpLockMode` 三态化是 EIO 语义的正式落点，下一步）、P0-2/P0-3（同文件，下一轮 fix-guard 各自读行）。

### ✅ Fix #4: P0-2 — copy_fd 的 CLOSE 装上 count>1 闸门 + From/To 补 inc_count（2026-09-09）

- **File**：`os/servers/vfs/src/filedes.rs`（copy_fd 签名与三分支 + 测试）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/14-filedes.md`（D5/测试表）。
- **Before**：`CopyKind::Close` 无条件清 fd（注释自认 "COPYFD_CLOSE expects count>1 to revert; we just clear"）；`From`/`To` 不做 `inc_count`（注释 "caller does"——但 count 配对是 Close 闸门的前提，调用方并不存在）；`copy_fd` 无 `FilpTable` 参数，计数操作根本不可达。
- **After**：签名加 `filp_table: &mut FilpTable`；`Close` = `count > 1` → `dec_count` + 清 fd + `Ok(src_fd)`，否则 `EBADF` 且 fd 不动（filedes.c:636-646 逐字对应）；`From`/`To` 安装后 `inc_count`（filedes.c:652）。测试：`test_copy_close` 增断言 count 2→1；新增 `test_copy_close_last_reference_ebadf`（count==1 → `EBADF`、fd 保留）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **336 passed / 0 failed**（335→336）。
- **边界与期发现**：From/To 方向建模与 EDEADLK/CLOEXEC/`filp_ioctl_fp` 守门缺口登记为 **R2-P0-2**（下一轮，与失效族同批）；14-filedes.md D5 原描述的 `cred: &Credentials` 签名与实现本就不符，已按现状改写并挂新条目指针。

### ✅ Fix #5: P0-3 — invalidate_by_endpoint 装上 v_fs_e 端点匹配（2026-09-09）

- **File**：`os/servers/vfs/src/filedes.rs`（invalidate_by_endpoint 重写 + 测试）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/14-filedes.md`（D6/实现表）。
- **Before**：参数名 `_proc_e` 未使用，函数失效**所有**非关闭且有 vnode 的 filp（注释自认 "for test determinism"）；双遍结构（一遍计数一遍失效）。
- **After**：签名 `(filp_table, vnode_table: &VnodeTable, proc_e)`——`count != 0 && vnode 表探针 fs == proc_e → CLOSED`，单遍完成（filedes.c:298-306 逐字对应）；按 C 去掉 `mode != FILP_CLOSED` 排除（对已关闭 filp 重置幂等合法）。设计取舍：C 的 `f->filp_vno->v_fs_e` 二跳解引在表分离模型下变成显式 `&VnodeTable` 参数（单一事实源），否决首轮"次选：Filp 冗余存 fs_endpoint"——两个真相源会在 vnode 回收复用时失同步（与 P2-5 的 generation 教训同向）。对照：Linux superblock 死亡的 `invalidate_inodes` 同样按 sb 归属遍历，不冗余存归属字段。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **336 passed / 0 failed**；测试升级为三 filp × 双端点矩阵（endpoint 5 → 仅 fid1；endpoint 6 → fid2+fid3；其余 filp 不动）。
- **边界**：C-3 的 by_char_major/by_sock_drv 是同族缺口（下一轮 Fix #6，复用本轮的"显式表参数"模式）；P2-5（`filp.vnode` 裸 `usize` → `VnodeId`）落地时本函数与测试同步改型。

### ✅ Fix #6: C-3 — invalidate_filp_by_char_major / by_sock_drv 补齐（2026-09-09）

- **File**：`os/servers/vfs/src/filedes.rs`（家族助手 + 两新函数 + 两新测试）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/14-filedes.md`（实现表/测试表）。
- **Before**：`by_char_major`/`by_sock_drv` 全 crate 零匹配（首轮 C-3）；驱动死亡级联只有 by_endpoint 一条规则。
- **After**：家族共享私有助手 `invalidate_filps_where`（`count != 0 && vnode 谓词 → CLOSED`，一个扫描三个谓词），`invalidate_by_endpoint` 重构复用（行为不变，Fix #5 的测试原样通过）；`invalidate_by_char_major` = `S_ISCHR && DevCodec::major(v_sdev) == major`（filedes.c:254-267），`invalidate_by_sock_drv` = `S_ISSOCK && split_smap_dev(v_sdev).num == num`（filedes.c:269-295；smap 行活性归表所有者，与 device_map 的注释契约一致）。
- **测试踩坑与 C 锚点**：初版测试用元组连续 `alloc_filp` 后才 `inc_count`——`alloc_filp` 不预留槽位（C 的分配点 `open.c:134` 立即 `filp_count = 1`，Rust 延迟给调用方，get_fd 注释已声明），三次分配全落槽位 0。测试改为交错分配并在注释标注该契约；`alloc_filp` 的延迟置位语义保持现状（属 get_fd 组合层设计，不自作主张改签名），后续轮次若发现第三个踩坑点再评估立条。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **338 passed / 0 failed**（336→338）。
- **边界**：R2-P1-3（sdev_stop 驱动死亡级联——本族 + smap unmap + select 唤醒的触发序设计，下一步）；P2-5（`filp.vnode` 裸 usize 类型化时三函数同步改型）。

### ✅ Fix #10: R2-P0-2 — copy_fd 方向建模 + 三守门齐装（2026-09-09）

- **File**：`os/servers/vfs/src/filedes.rs`（`CopyFdCtx` + copy_fd 重写 + 测试群重写）、`os/servers/vfs/src/device_map.rs`（`smap_endpt_by_dev` 助手）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/14-filedes.md`（D5/测试表）。
- **Before**：`From`/`To` 行为相同（都读 src 写 dst，方向建模缺失）；`S_ISSOCK` 自复制 EDEADLK、`COPYFD_CLOEXEC` 剥离/置位、`filp_ioctl_holder` VND 死锁探针全部缺席。
- **After**：`CopyFdCtx { filp_table, vnode_table, smap_table, policy, caller_endpoint(who_e), remote_slot, is_super, cloexec }` 显式注入 C 的隐式环境（`FreeCtx` 同型）；方向由 `kind` 决定（From 取 remote 装 caller + CLOEXEC 剥离；To 取 caller 装 remote + CLOEXEC 置位；Close 清 remote fd）；三守门齐（EPERM/`ioctl_holder` EBADF/自复制 EDEADLK）。`device_map` 新增 `smap_endpt_by_dev`（C `get_smap_by_dev`，smap.c:216-237）。设计要点：否决"调用方按 kind 交换 src/dst"的旧签名——方向错配必须在函数内不可表达；取用侧/安装侧的分化与 C 的 `get_filp2(COPYFD_TO?fp:rfp)` 一处分化同构。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **343 passed / 0 failed**（341→343：+EDEADLK、+ioctl_holder EBADF；既有 5 测试全部迁移到新签名并保持语义断言）。回归 review 曾逮到区间替换吞掉 `test_copy_close` 的 `#[test]` 属性（342≠343 账目不符），已补回。
- **边界**：close 路径的 `nr_locks` 释放与 `lock_filp(READ)` 真锁仍在 P1-2 接线矩阵；`SmapEntry.endpt` 的裸 `i32` 属 P2-5 类型化批。

### ✅ Fix #11: R2-P1-3 — sdev_stop 停尸决策补齐（2026-09-09）

- **File**：`os/servers/vfs/src/sdev.rs`（`StopPlan`/`stop`/`stop_matches` + 2 测试）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/22-sdev.md`（测试表）。
- **Before**：sdev.c 25 函数对位中唯一 ❌——`sdev_stop`（sdev.c:910-925）无任何 Rust 对应；`ChannelEvent::Dead` 只是测试脚本事件。
- **After**：`stop(call) -> StopPlan { group: finish_kind(call), reply: EIO }`——驱动死亡时挂起调用以 EIO 负类型按原有复活组收尾（C "统一口径"：:918 清挂起 → :921-923 EIO 续办）；`stop_matches(dev, smap_table, dead)` 判定挂起槽位是否属于死驱动（`pipe.c:347-350` 的 smap 行端点比对）。至此死亡级联的三面决策函数全部就位：filedes 失效族（Fix #5/#6）+ select 死亡唤醒（`DeathKind`/`unsuspend_hit`，既有）+ sdev 停尸（本条）；编排分类器 `classify_driver_waiter`/`DriverWake::StopSdev` pipe.rs 既有。
- **诚实边界**：运行时编排（fproc 扫描循环 + dmap/smap unmap 触发序）无法在任何纯决策层落地——它就是事件循环本身，归 P1-2 接线矩阵（该条建议 1 已含"驱动级联"步）。本条闭合的是语义缺口（❌ 对位缺失），非接线缺口。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**（343→345）。

### ✅ Fix #12: R2-P2-3 — gcov 五门之首的 root 门补齐（2026-09-09）

- **File**：`os/servers/vfs/src/misc.rs`（`gcov_privilege_gate` + 测试断言）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/31-misc-queries.md`（实现清单 + 守门表，行号按插入后实测重校）。
- **Before**：misc.rs gcov 决策组四门齐（label/endpt/grant/target），独缺 C gcov.c:31-34 的 `super_user → EPERM` 门；31 号文档 §2.7/:52 早已描述"root 检查"——文档对、代码缺。
- **After**：`gcov_privilege_gate(is_super: bool) -> Result<(), MiscError>`（`Perm → EPERM`），五门之首；测试并入 `test_probe_and_obsolete`（super 过 / 非 root Perm）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**（断言并入既有 gcov 测试，无新增 fn）。

### ✅ Fix #15: P2-5 — device_map/filp 的端点与 vnode 引用类型化（2026-09-09）

- **File**：`os/servers/vfs/src/device_map.rs`（约 20 处签名/字段）、`os/servers/vfs/src/filp.rs`（find_by_vnode）、`os/servers/vfs/src/filedes.rs`、`os/servers/vfs/src/sdev.rs`（消费方）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/19-device-map.md`（字段表）。
- **Before**：`DmapEntry.driver`/`SmapEntry.endpt`/`RegisterPlan.unsuspend` 等全线 `Option<i32>`；`driver_match(table, proc: i32, ...)` 等 ~10 个签名吃裸 i32；`find_by_vnode(vnode: usize)` 裸 usize 指向 vnode 表（无 generation 保护）。
- **After**：全线 `Option<Endpoint>`/`Endpoint`（`CTTY_ENDPT = Endpoint::VFS`、`RS_PROC_NR = Endpoint::RS` 常量同型化）；`find_by_vnode(vnode: VnodeId, bits)`。设计取舍：域号（`check_domain`/`smap_by_domain` 的 `domain: i32`，PF_* 族）与 sockid（`split_smap_dev`）不是端点，保持原类型——类型化的边界是"是不是进程身份"，不做无差别替换；`bdev::resolve_driver`/`cdev::resolve_gate` 的 `Option<i32>` 参数是 wire 边界（消费方在路由层接线时转换），本条不改，待 R2-P1-2 分发收敛时以 Endpoint 贯通。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**；`grep -n "i32" device_map.rs` 剩余仅为 domain/sockid/errno 等合法非端点量。回归 review 曾修复三处误转（register_plan 槽位号 u8、check_domain 的 self_idx u8、smap 行号断言）——宽泛正则替换后必须以编译错误清单逐条回溯。

### ✅ Fix #16: R2-P3-2 — device_map.rs 补 C 文件 → 函数级来源地图（2026-09-09）

- **File**：`os/servers/vfs/src/device_map.rs`（模块头 source map）。
- **Before**：模块头只列四个 C 来源文件名，未说各自落点；按 C `device.c` 索引的读者找不到 ioctl 决策三函数。
- **After**：模块头增函数级 source map（dmap.c / smap.c / device.c / mapdriver 四行，各列对应 Rust 符号；device.c 行注明授权创建半属内核 IPC 束）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**（注释级）。

### ✅ Fix #17: P2-3 — 11 个测试替身移出生产编译单元（2026-09-09）

- **File**：request.rs（MockFsClient）/ bdev.rs（ScriptedTransport）/ cdev.rs（NoTty）/ sdev.rs（ScriptedChannel、SilentChannel）/ socket.rs（EmptyTable、ScriptedAlloc、FailingAlloc）/ fs_comm.rs（MockTransport、TestTransIdCodec）/ fcntl.rs（ScriptedFcntl），共 21 个项（struct + impl 块）加 `#[cfg(test)]`；main_loop 的 re-export 拆为 `TestTransIdCodec` 条件导出。
- **Before**：11 个替身以 `pub` 定义在各文件 `#[cfg(test)] mod tests` 之前的生产编译单元——no_std 生产库的公共 API 被测试脚手架污染。
- **After**：逐项 `#[cfg(test)]` 圈定（保留源位置，不物理搬移——同文件 tests 经 `use super::*` 照常可见）；path.rs 的伪造抽象族不在本条（R2-P1-1 的重设计范围）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**；`cargo check` 无生产单元替身残留警告。

### ✅ Fix #7: P1-5 — get_filp 的 need_lock bool 三态化为 FilpLockMode（2026-09-09）

- **File**：`os/servers/vfs/src/filp.rs`（`FilpLockMode` 枚举 + get_filp 重写 + 测试矩阵）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/04-filp-table.md`（实现表/测试表）。
- **Before**：`get_filp(&mut self, id, need_lock: bool)`——`need_lock=false`（≈`VNODE_NONE`）不拒 `FILP_CLOSED`，比 C 宽；且 bool 无法表达"OPCL 过门但取锁、NONE 不取锁也不过门"的组合。
- **After**：`FilpLockMode { Opcl, None, ReadWrite }`——门规则 `f.mode == FILP_CLOSED && !matches!(Opcl) → Closed(EIO)`（filedes.c:186-188 逐字对应，注释 "disallow all use except close(2)"）；锁规则 `Opcl|ReadWrite` 且 `locked_by` 已占 → `Busy`（filedes.c:191-193 `locktype != VNODE_NONE` → lock_filp）。设计取舍：`VNODE_READ`/`VNODE_WRITE` 在门与锁两个维度行为相同，合并为一个 `ReadWrite` 变体（不为枚举完整性造无行为差异的变体——Gate D 虚构第二实现的教训，§9.6）；P0-1 的 EIO 语义至此有了正式落点（非 `OPCL` 访问路径在 `get_filp` 处被拒，close 路径在 `close_fd` 直通）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **338 passed / 0 failed**；`grep -rn "need_lock" os/servers/vfs/src` 仅余 stadir.rs 的无关同名决策函数。
- **边界**：get_filp 生产消费者随 R2-P1-2 的分发绑定落地（read/write 臂以 `ReadWrite`、close 臂不经此函数）；04-filp-table.md 实现表与测试表已同步三态语义。

### ✅ Fix #8: P1-3 — SETGROUPS 的数据搬运口与尺寸门（2026-09-09）

- **File**：`os/servers/vfs/src/ipc/dispatcher.rs`（PmError/SetGroups 臂/trait 口/3 测试）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/10-pm-protocol.md`（D4 落地说明）。
- **Before**：`handle` 的 `SETGROUPS` 臂 `let _ = group_addr;` + 空切片调用——`handle_setgroups` 内置的短切片拒绝使一切 `ngroups>0` 的生产请求 `Err(TooManyGroups)`（fail-closed 但数据通路断，且 ENOSYS/EFAULT 语义不可见）。
- **After**：`PmHandler::fetch_group_list` 作为 `sys_datacopy_wrapper`（misc.c:752）的接缝口，**默认实现 fail-closed `ENOSYS`**（模式 60 诚实契约：通电挂 P1-2/edge E1，不算 DEFERRED 充数——语义入口、门与数据流已全部就位，唯余 transport）；臂内 `ngroups > NGROUPS_MAX → TooManyGroups(EINVAL)`（C panic misc.c:748-750 → fail-closed Err，与 10 号文档 D4 的 EFAULT 决策同向并登记）、`group_no==0` 免拷贝直清（`setgroups(0)` 合法语义）、正数路径 `fetch → 栈缓冲 → handle_setgroups`。`PmError::NotImplemented(ENOSYS)` 变体新增（对齐 minix-types ToErrno 的 D1/D2 方向）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **341 passed / 0 failed**（338→341：ENOSYS 预通电态且 fproc 不动 / 超限 EINVAL / 零组直清三测试）。
- **边界**：`fetch_group_list` 的生产实现（真实 sys_datacopy）随 P1-2 接线矩阵第一束（内核 IPC 原语）落地，届时拷贝失败映射 EFAULT；10-pm-protocol.md D4 已登记完整决策链。

### ✅ Fix #13: P2-6 — TransIdCodec 收敛到 fs_comm 单一定义（2026-09-09）

- **File**：`os/servers/vfs/src/main_loop.rs`（删 63 行重复定义 → `pub use` re-export）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/11-fs-comm.md`（三处引用同步）。
- **Before**：`TransIdCodec` trait + `VfsTransIdCodec` + `TestTransIdCodec` 在 fs_comm.rs:76-117 与 main_loop.rs:130-180 双重定义，实现逻辑相同（0xB00 基），协议常量改动需改两处。
- **After**：唯一定义在 fs_comm.rs（协议属主——`TransId`/`TRANSACTION_BASE` 的家），main_loop `pub use` re-export 供 route_message 泛型与测试消费（`ARCH A-4` 迁移标注）；11-fs-comm.md 的模块图/实现清单/测试表三处同步为"唯一定义 + re-export"。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**（纯收敛，两侧测试原样通过）。

### ✅ Fix #14: P2-4 — NextFit 移 cfg(test) 更名 NextFitDemo（2026-09-09）

- **File**：`os/servers/vfs/src/filedes.rs`（定义/文档/trait 文档/测试 6 处）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/14-filedes.md`（D2/模块图/实现表/测试表/策略行 7 处）。
- **Before**：`pub struct NextFit` 在生产编译单元，:70 的 trait 文档以 "O_DUPFD arg lower-bound variant" 辩护其真实语义——该辩护不成立：O_DUPFD 复用的是 `get_fd(start=arg)` 的同一 `LowestFree` 策略，并非第二种分配策略。
- **After**：`#[cfg(test)] pub struct NextFitDemo`——对照实现限定测试；trait 文档如实声明"Minix3（filedes.c:121）与 Linux（`alloc_fd(start, end)`）都是 start 起最低空闲，不存在第二真实策略；trait 建模的是 `start` 参数化（O_DUPFD 的 arg），Demo 仅证多态"。落实 §9.6 Rule Discovery 规则草案（"无 C 来源第二实现 → 移 cfg(test) 或删除"）的首个适用案例。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**；`grep -rn "NextFit" os/servers/vfs/src` 仅 cfg(test) 域与文档注释命中。

### ✅ Fix #18: P3-1 — vfs clippy 归零 + Fix #17 构建断裂补修（2026-09-09）

- **File**：filedes.rs（map_or→is_some_and ×2、LowestFree/NextFitDemo 迭代器化）、main_loop.rs（嵌套 if 折叠 + doc quote 改写 + cast 移除）、call_table.rs（`VFS_BASE + 0` → `VFS_BASE`）、vnode.rs（let-chain 折叠 + unit 比较断言移除 + SmapTable 同型 is_empty 不适用）、vmnt.rs、device_map.rs（字面量分组 + 常量断言转注释 + SmapTable::is_empty）、fproc.rs（FprocLightTable Default）、fs_comm/dispatcher（多余 mut）、filp/path/request（未用导入与 bitflags doc 归位）、Cargo.toml（声明 `fproc_light` feature——ARCH A-7 占位的 cfg 有着落）。
- **重要回归补修**：Fix #17 的 cfg(test) 圈定漏掉了三个 impl 块（cdev `TtySource for NoTty`、sdev `SockChannel for SilentChannel`、socket `SockLookup for EmptyTable`），导致 **`cargo build`（非 test）自 Fix #17 起断裂**——当时回归只跑了 `cargo test`（cfg test 激活掩盖断裂）。本条补齐三个 impl 的门控，`cargo check` 与 `cargo test` 双绿。教训入档：替身门控类修改的回归必须包含非 test 构建。
- **死产夹具判定**：`AltFilpTable`/`AltFprocTable`/`AltTll`/`AltFs`(×2)/`AltVnodeTable`/`AltVmntTable` 七个 Gate D 死产测试夹具删除——"never constructed/never used" 实证死代码，属 §9.6 规则草案的收编范围（为何死：注释自认 Gate D 产物且零构造点；消除影响：无生产引用、无测试断言引用）。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **345 passed / 0 failed**；`cargo check` 与 `cargo clippy --lib --tests` 对 servers/vfs 的警告计数归零（minix-sys/minix-types 的 5+1 条不在本 stage 范围）。

### ✅ Fix #19: R2-P2-1 — 30 个错误枚举接入 ToErrno 统一映射通道（2026-09-09）

- **File**：29 个模块文件（每个错误枚举一个 trait impl）+ `filp.rs`（`FdError` → `FdScanError` 改名）。
- **Before**：30 个 `pub enum *Error` 各带固有 `fn to_errno(self) -> i32`，对 minix-types 已落地的 `ToErrno` trait（commit 893386cd8 的 D1/D2 通道）零接入；filp.rs/filedes.rs 双同名 `FdError`。
- **After**：30 个枚举逐一 `impl minix_types::ToErrno`（`Errno::from_i32((*self).to_errno())` 委托既有固有方法——兼容设计按 trait 文档"new trait impls delegate to them"）；`filp.rs` 的 fd 双扫描错误更名 `FdScanError`（`EMFILE`/`ENFILE` 两变体，与 filedes 的 fd 表操作 `FdError` 不再同名）。宏 vs 手写：30 个 impl 以脚本生成、逐文件编译验证——比宏抽象少一层间接，且 diff 可逐条审。
- **Verified**：`grep -rn "impl minix_types::ToErrno" os/servers/vfs/src | wc -l` = **30**；`grep -rn "enum FdError" | wc -l` = **1**；`cargo test` = **345 passed / 0 failed**。
- **边界**：固有 `to_errno(self) -> i32` 保留（ToErrno 文档声明的兼容通道）；消费端随各模块后续触改逐步切 `ToErrno::to_errno(&e).to_i32()`，不做一次性全量重写（VM 侧 T8 判定先例：映射点唯一即可，调用形态不强制）。

### ✅ Fix #21: C-2 — exec 收尾的 clo_exec 扫描补齐（2026-09-09）

- **File**：`os/servers/vfs/src/exec.rs`（`clo_exec` + 测试 + 模块 scope note 修正）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/25-exec.md`（D7）。
- **Before**：`fproc.cloexec_set` 只有存储位图，exec.rs 十一阶段流水线无收尾扫描——接线后 exec 会向新程序泄漏 CLOEXEC fd。
- **After**：`clo_exec(rfp, filp_table)`——`0..OPEN_MAX` 逐 fd 查 `cloexec_set`，命中即 `close_fd`（C 以 `(void)` 忽略关闭错误：扫描必须跑完、失败不回滚，exec.c:721-731 逐字对应）。模块 scope note 的"fd-table execution stays with 14"修正为"close_fd 原语归 14，exec 尾扫描归本篇"——C 的 clo_exec 本就在 exec.c。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **347 passed / 0 failed**（346→347：`test_clo_exec_tail_scan`——非 CLOEXEC 幸存、CLOEXEC 关闭、引用释放）。

### ✅ Fix #20: C-4 — VmntLock 补 downgrade/upgrade（2026-09-09）

- **File**：`os/servers/vfs/src/vmnt.rs`（VmntLock 两方法 + 测试）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/06-vmnt-table.md`（§2.5 落地说明）。
- **Before**：`VmntLock` 只有 `try_lock`/`unlock`（对比 tll.rs 与 vnode.rs 都有 downgrade/upgrade）；unmount 与跨挂载"lookup 先 READ 探路、命中后 WRITE 修改"的升级路径无锁原语可用。
- **After**：`downgrade`（Write→Read(1)，Unlocked 幂等——同 tll 对自由锁的 no-op）与 `upgrade`（Read(1)→Write；多读者 `Busy`——C 的写侧永等在此硬化为拒绝并登记；已 Write 幂等 ok，同 tll.rs 契约）。测试矩阵：降级后共享加入、双读者拒绝晋升、单读者晋升、幂等。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **346 passed / 0 failed**（345→346）。

### ✅ Fix #9: P1-4 — FilterOutcome::Query 编码清 UPDATE/置 BUSY 义务（2026-09-09）

- **File**：`os/servers/vfs/src/select.rs`（Query 变体 + filter_step 构造 + 测试矩阵）、`notes/rewrite/fork-syscall-rewrite/05-stage-vfs/23-select.md`（D3）。
- **Before**：`Query { rops, set_update, set_block }` 只编码"置"侧义务；C 的发送序三步（select.c:517 清 UPDATE → cdev/sdev_select → :522 置 BUSY + `dmap/smap_sel_busy`）中"清"侧完全留在调用方记忆——照字段字面执行的消费者会在 BUSY 期间残留 UPDATE，翻转 `reply1_step` 的 ops 清零规则（select.rs:360）。
- **After**：`Query` 增 `clear_update: bool` 与 `set_busy: bool`（`filter_step` 恒置 true，附 C 行号锚点）——义务从隐性契约变为随决策输出的数据；`reply1_step`/`reply2` 不变（它们消费的是应答侧状态）。测试矩阵扩断言两新字段。
- **Verified**：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib` = **341 passed / 0 failed**；`grep FilterOutcome::Query` 全 crate 仅 select.rs（无外部消费者需迁移）。
- **边界**：Query 的真正消费者（发送 + dmap busy 置位 + SUSPEND）随 P1-2 接线矩阵落地；届时以本义务字段驱动 flag 转换，勿再手写。
