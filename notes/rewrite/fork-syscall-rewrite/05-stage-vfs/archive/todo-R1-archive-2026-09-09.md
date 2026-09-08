# 05-stage-vfs todo.md 第一轮全卷存档（R1，2026-09-06）

> **存档时间**: 2026-09-09（第二轮追加时按 02-stage-vm 的 archive 模式整体迁移）
> **存档范围**: 首轮 §1～§8 与附录 A/B 原文；§0 速览表被主文件 0.1 历史一览取代后一并保留于此。
> **状态**: 首轮 26 条（C-1～C-10、P0×3、P1×5、P2×6、P3×2）全部 open；2026-09-09 第二轮逐条复核结论见主文件 §1（24 条锚点原样维持、C-5/P2-1/P2-4 三处漂移修正）。修复时以本文档条目正文 + 主文件 §1 重校锚点为准。
> **轮次交叉引用**: 主文件 §9（第二轮）引用"首轮 §N"均指本文档章节。

---

## 0. 审查结论速览（首轮原文）

总体判断：VFS 是当前重写完成度最高的服务器——C 语义被系统性地分解为纯决策函数与类型化状态机，测试密度高（334 个测试全部通过）。最大的问题不在单个函数，而在**两层**：其一，`run()` 生产入口驱动的分发契约与忠实的 `route_message` 契约是两套并行的东西，后者没有生产消费者（P1-1）；其二，"最后一公里"（内核 IPC、SEF 生命周期、真实挂载）整体未接线，且 mfs 侧 31 个分发条目只有 8 个能真实应答（P1-2）。另有三条已实现代码中自认偏离 C 语义的 P0（§2）。

| 级别 | 条目 | 一句话 |
|------|------|--------|
| C | C-1～C-10 | 查漏清单：SEF 生命周期、clo_exec、invalidate 失效族、vmnt 锁升降级等（见 §1） |
| P0 | P0-1 | `close_fd` 对已关闭 filp 返回 EINVAL，C 语义是 EIO（注释自认） |
| P0 | P0-2 | `copy_fd` 的 CLOSE 分支缺 `filp_count > 1` 闸门，无条件清除 |
| P0 | P0-3 | `invalidate_by_endpoint` 忽略端点参数，超范围失效所有打开 filp |
| P1 | P1-1 | 双分发契约：`run()` 用 legacy `dispatch()`，忠实的 `route_message()` 无生产消费者，含 Read 占位符陷阱 |
| P1 | P1-2 | "最后一公里"矩阵：SEF、内核 IPC、根挂载、path 循环——依赖链与关闭条件未收敛 |
| P1 | P1-3 | `handle_setgroups` 丢弃补充组内容，组权限判定空转 |
| P1 | P1-4 | `filter_step` 的 Query 结果未编码"发送前清 UPDATE 置 BUSY"义务 |
| P1 | P1-5 | `get_filp` 三类锁语义（OPCL/NONE/读写）塌缩成一个 bool |
| P2 | P2-1 | 20 个模块级错误枚举 + 29 个 `to_errno`，含两个同名 `FdError` |
| P2 | P2-2 | `try_from_raw` 64 臂同构 match；`CallTable` 与 `from_raw` 信息量重复 |
| P2 | P2-3 | 测试替身类型全部裸露在生产编译单元 |
| P2 | P2-4 | `NextFit` 是无 C 来源的自造第二策略（Gate D 双实现压力的产物） |
| P2 | P2-5 | 类型化欠账：`device_map.rs` 裸 `i32` 端点、`filp` 内 vnode 用 `usize` |
| P2 | P2-6 | `TransIdCodec` trait 在 fs_comm 与 main_loop 双重定义 |
| P3 | P3-1 | clippy 真实 lint 约 23 条（collapsible_if 8 条为主） |
| P3 | P3-2 | 简化删除未文档化：lock_proc 族、check_*_locks debug 族、unlock_filps |

（首轮验证命令：`cargo check -p minix-vfs` 通过 lib 7 warnings；`cargo test -p minix-vfs --lib` 334 passed；`cargo clippy -p minix-vfs --lib` 59 条行级告警含 21 条 profile 噪音。）

---

## 1. 查漏补缺（覆盖率缺口清单）

覆盖率基线：415 个 C 符号（311 函数、10 结构体、92 宏、2 枚举）；文档覆盖 387（93.3%）；Rust 机器匹配 262（63.1%，语义映射表 v1 后；裸名基线仅 59/14.2%）。机器数字是下限——枚举变体映射（如 `req_inhibread` → `FsReq::InhibRead`）对提取器不可见（变体不被提取为符号），本项目"一个 C 函数分解为一组纯函数"的风格也天然压低同名率。以下为人工判定的权威缺口表，每条已逐符号 grep 验证（反幻觉）。

### 缺口表

| 编号 | C 符号 | C 锚点 | 判定 | 说明 |
|------|--------|--------|------|------|
| C-1 | `sef_local_startup` / `sef_cb_init_lu` / `sef_cb_lu_prepare` / `sef_cb_lu_state_changed` | main.c:303-391 | **核心，依赖未解除** | SEF 生命周期整体缺失；`os/libs/minix-sef/src/lib.rs` 仅 5 行。`VfsState::init_fresh`（main_loop.rs:327）只覆盖 fresh 初始化，live-update/restart 无任何建模。VFS 作为可被 RS 重启的服务，这是接线前提。归入 P1-2 矩阵 |
| C-2 | `clo_exec` | exec.c:721 | **核心** | exec 收尾扫描并关闭 CLOEXEC fd：Rust 只有存储位图 `fproc.cloexec_set`（fproc.rs:291），`exec.rs` 十一阶段流水线无收尾扫描。接线后 exec 会向新程序泄漏 CLOEXEC fd，属行为级缺口 |
| C-3 | `invalidate_filp_by_char_major` / `invalidate_filp_by_sock_drv` | filedes.c:260 / :277 | 核心 | 驱动死亡时按字符主设备号/socket 驱动失效 filp 的两个扫描；Rust 仅有单条规则 `invalidate_filp`（filedes.rs:187）与弱化的 by_endpoint（见 P0-3）。dmap/smap unmap 级联（device_map.rs `unmap_by_endpt`/`smap_by_endpt`）落地时必需 |
| C-4 | `downgrade_vmnt_lock` / `upgrade_vmnt_lock` | vmnt.c:221 / :237 | 核心 | `VmntLock` 只有 `try_lock`/`unlock`（vmnt.rs:72/:90）；对比 tll.rs 与 vnode.rs 都有 `downgrade`/`upgrade`。unmount 与跨挂载操作需要 |
| C-5 | `fetch_vmnt_paths` | vmnt.c:246 | 核心 | `do_getvfsstat` 要返回每个挂载点的根路径与挂载路径；`vmnt.rs` 的 `Vmnt` 无路径字段，stadir.rs 的 `walk_plan`（stadir.rs，`do_getvfsstat:351-413` 锚）不含路径产出 |
| C-6 | `lookup` 跨 FS 往返循环 / `last_dir` / `get_name` / `canonical_path` | path.c:384 / :146 / :594 / :648 | 核心，依赖 IPC | 状态与结构已建（`path::Lookup`/`LookupRes`/`check_symloop`/`consume_prefix`），但 REQ_LOOKUP 往返推进循环未落地；`last_dir`/`get_name`/`canonical_path` 目前仅存在于 path.rs 测试演示（如 path.rs:383 附近）。归入 P1-2 矩阵 |
| C-7 | `mount_pfs` / `do_socketpath` | mount.c:391 / path.c:803 | IPC 依赖，有标注 | main_loop.rs:431 与 socket.rs:23 已注明 DEFERRED 归 18/13。合法 DEFERRED，收敛进 P1-2 |
| C-8 | `pm_reboot` / `unmount_all` | misc.c:504 / mount.c:552 | 核心 | 重启前全量卸载序（`unmount_all` 的 `verify_empty`/`sweep_passes` 决策件已在 mount.rs，但重启触发链无）。misc.rs:19 scope note 声明归 10，需关闭条件 |
| C-9 | `ds_event` / `panic_hook` | misc.c:949 / :989 | 辅助，部分有标注 | DS 驱动上线事件在 main_loop.rs:395 注明 DEFERRED 归 19/24；panic 清理钩子无建模。Redox 的 root scheme 注册模型可作参照（见 §6） |
| C-10 | `lock_proc` / `unlock_proc` / `thread_cleanup` / `check_filp_locks(_by_me)` / `check_vnode_locks(_by_me)` / `check_vmnt_locks(_by_me)` / `unlock_filps` / `init_select` / `select_forget` / `wipe_select` / `select_timeout_check` / `select_dump` | main.c:528-578、filedes.c:26-71、vnode.c:43-83、vmnt.c:24-62、filedes.c:383、select.c:821-1370 | 简化删除，**未文档化** | 单线程事件循环下进程锁与多线程死锁断言确实可以删除（ARCH A-1 合理推论），但删除决定散落在代码注释里（如 filedes.rs:193-199），没有任何一处集中声明"哪些 C 函数被有意删除、为什么"。建议在 99-global-concepts.md 或本文件 §7 集中登记。select 侧的 `select_timeout_check` 到期语义已由 `plan_timeout` 建模，`select_dump` 由 misc.rs 的 `HostAction::RunSelectDump` 命名 |

### REQ 协议三方对账（"最后一公里"能力矩阵）

- C 侧：request.c 共 36 个 `req_*` 函数（含 `req_breadwrite_actual` 等重试后半），消息码在 vfsif.h。
- VFS 侧：`FsReq` 32 变体（request.rs:122 起）**全部对上**（`Read`/`Write` 合并了 `req_readwrite`，重试由 `send_with_retry`（request.rs:453）承接；`BRead`/`BWrite` 对 `req_breadwrite`）。
- FS 驱动侧：`os/fs/mfs/src/table.rs` 的 `MFS_TABLE` 31 条中 **8 条 Live**（`fs_mount`/`fs_unmount` 为 `LiveInCrate`，5 条块传输为 `LiveViaBlockTransfer`），**23 条 `PendingDocument`**——包括 `fs_lookup`。
- 结论：即使 VFS↔FS 消息回路今天接通，VFS 也只能完成根挂载与块级读写，无法执行路径解析。查漏的真正瓶颈在 mfs 侧而非 VFS 侧。

### DEFERRED 审计

生产代码 23 处 DEFERRED/TODO 标记（main_loop.rs 13 处、ipc/dispatcher.rs 6 处、其余零星）。逐条核对结论：**全部带"归 NN"文档指向，无被遗忘的孤儿标记**。集中的依赖束有三条——真实挂载（归 18）、dmap/smap 初始化与 DS 订阅（归 19）、内核 IPC 原语（归 99）——全部收敛进 P1-2 的矩阵，不需要新增条目。

---

## 2. P0：真实 bug（必须修复）

### P0-1 `close_fd` 对已关闭 filp 返回 EINVAL，C 语义是 EIO

> **修正（2026-09-09，Fix #3）**：本条对 C 行为的转述有误。`get_filp2`（filedes.c:186-188）的 `FILP_CLOSED→EIO` 门带 `locktype != VNODE_OPCL` 前置——**close(2) 走 `OPCL`，穿过 `CLOSED` 继续关闭**（open.c:696-704 清 fd 后走 `close_filp`）；EIO 是给 read/write 等非 `OPCL` 访问的。因此 bug 本身成立（Rust 在 C 会成功的地方返回 EINVAL），但修法不是本条"建议 1"的 `FdError::Closed→EIO`，而是**删除早退、放行关闭**。EIO 归属非 `OPCL` 访问路径（P1-5 的 `FilpLockMode` 接缝）。以下原文保留存档。

**问题**：`filedes::close_fd`（os/servers/vfs/src/filedes.rs:174-176）对 `filp.mode == FILP_CLOSED` 返回 `FdError::Inval`，行内注释自认 "EIO mapped to Inval for test"。C 的 close 路径经 `get_filp2(..., VNODE_OPCL)`（open.c:700），对已关闭 filp 返回 **EIO**（filedes.c:183-190）。
**证据**：注释自我承认；且 `filp.rs:183` 的 `FilpError::Closed → EIO` 证明 crate 内两条路径互相矛盾——同一"已关闭"事实在 filp 层是 EIO、在 fd 层是 EINVAL。
**影响**：接线后 `close()` 一个已被驱动失效（`FILP_CLOSED`）的 fd 会向应用返回 EINVAL 而非 EIO，违反 C 外部行为。
**建议**：
1. 首选：新增 `FdError::Closed` 并映射到 `minix_types::EIO`，`close_fd` 返回它。
2. 同步修正任何断言 EINVAL 的既有测试（先 grep `close` + `Inval` 的测试再改，勿删测试）。
3. 参考：`filp.rs:183` 已有正确的 EIO 映射可复用。

### P0-2 `copy_fd` 的 CLOSE 分支缺 `filp_count > 1` 闸门

**问题**：`filedes::copy_fd` 的 `CopyKind::Close` 分支（filedes.rs:264-273）无条件清除目标 fd 并返回成功，注释自认 "COPYFD_CLOSE expects count>1 to revert; we just clear"。C 的 `COPYFD_CLOSE`（filedes.c:638-646）只在 `rfilp->filp_count > 1` 时执行（递减计数 + 清 fd），否则返回 **EBADF**——因为 COPYFD_CLOSE 语义是"撤销一次成功的 dup，调用者自己还持有一份"。
**证据**：两侧代码逐行对照如上；C 锚点 filedes.c:638-646。
**影响**：接线后对最后一个引用执行 COPYFD_CLOSE 会绕过引用计数递减，造成 filp 泄漏或悬垂。
**建议**：
1. 首选：补 `FilpTable::get(id).count > 1` 检查，不足时返回 `FdError::BadFd`（EBADF），并执行 `dec_count`。
2. 次选：若暂不建 count 联动，至少返回错误并留 `DEFERRED` 注释挂 P1-2。

### P0-3 `invalidate_by_endpoint` 忽略端点参数，超范围失效

**问题**：`filedes::invalidate_by_endpoint`（filedes.rs:200-222）参数名为 `_proc_e`（未使用），函数失效**所有**非关闭且有 vnode 的 filp，注释自认 "Here we invalidate all non-closed for test determinism"。C `invalidate_filp_by_endpt`（filedes.c:298-306）只失效 `filp_vno->v_fs_e == proc_e` 的。本条由两条独立审查通道（本会话横切扫描 + 系统调用面对位 agent）同时发现，交叉验证一致。
**影响**：接线后一个文件系统驱动死亡会关掉**所有**文件系统中所有进程的打开文件，而非仅该 FS 的——灾难面放大一个数量级。
**建议**：
1. 首选：filp 存 `VnodeId`，`vnode` 表查 `v_fs_e` 后比对端点（`VnodeTable::get`，vnode.rs:180）。
2. 次选：`Filp` 直接冗余存 `fs_endpoint` 字段（空间换解耦，挂 `[ARCH]` 标注）。
3. 顺带修同族的 C-3 两个缺口（by_char_major/by_sock_drv），一次设计到位。

---

## 3. P1：架构级问题（建议尽快规划）

### P1-1 双分发契约：`run()` 驱动 legacy，忠实契约无生产消费者

**问题**：生产入口 `run()`（main_loop.rs:788-808）是 mock 自旋——每轮以 `Message::default()` 为输入调 `dispatch()`；而 `dispatch()`（main_loop.rs:480-499）是三路简化（PM/用户槽/门控），`route_message` 的文档注释（main_loop.rs:578-581）明确称其为 "legacy"。忠实的八级优先路由 `route_message()`（main_loop.rs:573-634，含 FS transid 应答、reviving、bdev/cdev/sdev 应答）在整个 crate 内只有测试段消费。
**附带陷阱**：`route_message` 对无法解析的调用号替换为 `VfsCallNum::Read` 继续路由（main_loop.rs:626-634），注释自称 "Real dispatch will ENOSYS. Use Read as placeholder"——声明意图与代码行为相反。C 语义是 ENOSYS（main.c:283-294，`call_index >= NR_VFS_CALLS` → `error = ENOSYS`）。接线者若按注释理解会在无效系统调用上执行 read 语义。
**影响**：C main.c:80-138 的完整优先序在可运行入口上不存在；两套契约在接线时必须二选一收敛，Read 占位符是被埋下的静默错误执行点。
**建议**：
1. 首选：接线时以 `route_message` 为唯一契约，删除 `dispatch()` 与 legacy 标注；占位符分支改为显式 `Route::Enosys` 变体（消费端回复 ENOSYS），与 C 对齐。
2. 次选：若保留两套（教学演示 vs 契约），在 lib.rs 模块文档声明各自地位与"接线时替换"路径。
3. 参考 Redox：scheme 分发按 URL 前缀路由到提供者，分发逻辑单一且在内核一处（doc.redox-os.org/book/schemes.html）——"一个分发点"是微内核服务器的共同实践。

### P1-2 "最后一公里"依赖矩阵：SEF、内核 IPC、根挂载、path 循环

**问题**：四条依赖束散落在 23 处 DEFERRED 标记与两个阶段（mfs 侧 23 条 PendingDocument）中，没有统一的收敛视图：
1. 内核 IPC 原语（`sys_safecopyfrom`、`ipc_sendnb`，main_loop.rs:399-400/:735）→ `FsCaller`/transport trait 的生产实现；
2. SEF 生命周期（C-1，minix-sef 库 5 行）→ 服务可重启性；
3. 根挂载链（`mount_pfs`/`mount_fs`，main_loop.rs:431-433）→ mount.rs 判定件已有、执行件无；
4. path 跨 FS 往返循环（C-6）→ 依赖 1。
另外 mfs 侧 `MFS_TABLE` 31 条只有 8 条 Live（fs/mfs/src/table.rs:53-247），`fs_lookup` 等 23 条 Pending——**VFS 侧消息协议（FsReq 32 变体）反而已齐**。
**影响**：这是 VFS 从"语义库"变成"可运行服务器"的全部剩余工作；依赖不清会导致接线顺序错误（例如先接挂载而后 SEF，重启即挂）。
**建议**：
1. 首选：在 plan.md 增补"接线顺序"小节（或本文件 §8 追踪），显式排序：IPC 原语 → SEF → dmap/smap 初始化 + DS 订阅 → 根挂载 → path 循环 → 驱动级联（C-3）→ mfs lookup 落地。
2. 每一项给关闭条件（哪条 C 函数有真实消息回路测试）。
3. 参考 Redox：文件系统作为用户态 daemon 通过 root scheme 注册（github.com/redox-os/book scheme-operation.md）——注册/生命周期与 IO 路径分离，与 SEF→挂载的顺序要求同构。

### P1-3 `handle_setgroups` 丢弃补充组内容，组权限判定空转

**问题**：生产消息路径 `VfsCall::SetGroups`（ipc/dispatcher.rs:120-131）丢弃 `group_addr`（`let _ = group_addr;`），以空切片调用 `handle_setgroups(endpoint, group_no as usize, &[])`。C `pm_setgroups`（misc.c:743-753）经 `sys_datacopy_wrapper` 把组列表拷入 `fp_sgroups`。数据模型与判定函数都正确（`FProc::supplemental_groups`，fproc.rs:304-307；`in_supplementary`，protect.rs:188-190），坏的只是生产写入路径——只有测试直调 `handle_setgroups` 传真列表才能填充。
**影响**：接线后所有补充组权限判定（`forbidden` 的组档）恒为否定；`NGROUPS_MAX` 边界检查（C misc.c:749-750 的 panic）也未建模。
**建议**：
1. 首选：`SetGroups` 变体改为携带 `GrantScope`（复用 request.rs:97 的授权抽象）驱动 `sys_datacopy` 等价拷贝；接线前至少把 `ngroups > NGROUPS_MAX` 的拒绝语义补上。
2. 在 dispatcher.rs 现有注释处升级为显式 `DEFERRED(P1-3)` 标记，注明当前为语义占位。

### P1-4 `filter_step` 的 Query 结果未编码"发送前清 UPDATE、置 BUSY"义务

**问题**：C 在 select_filter 置 FSF_UPDATE 后，向驱动发送查询前会清除 UPDATE 并置 FSF_BUSY（select.c:509/:525 与 :518/:554）。Rust `FilterOutcome::Query`（select.rs:191-198，发射点 248-252）只有 `set_update: true`，没有对应的清除义务字段。照字段字面执行的调用方会在 BUSY 期间残留 UPDATE，恰好翻转 `reply1_step` 的 ops 清零规则（select.rs:360）。
**影响**：select 状态机的 UPDATE/BUSY 交互是两波收场的正确性前提；当前决策件把义务留在调用方"自己知道"，是隐性契约。
**建议**：`FilterOutcome::Query` 增加 `clear_update: bool`（或改为 `Query { must_clear_update }`），`reply1_step` 文档注明依赖。系统调用面对位报告确认其余对位点（pipe 读写矩阵、plan_timeout 截断进位、forbidden 五段裁决等）逐行核对无漂移，可作回归基线。

### P1-5 `get_filp` 三类锁语义塌缩成一个 bool

**问题**：C `get_filp2`（filedes.c:178-201）的三类锁请求中，"FILP_CLOSED 拒绝"适用于除 `VNODE_OPCL` 外的全部（OPCL 正是为 close 已关闭文件而设）。Rust `FilpTable::get_filp`（filp.rs:173-192）以 `need_lock: bool` 表达，`need_lock=false`（≈VNODE_NONE）路径不再拒绝已关闭 filp，比 C 宽。
**影响**：接线后非 OPCL 调用者（read/write 路径）拿到已失效 filp 不报 EIO，与 P0-1 同根——建议一并修。
**建议**：把 `need_lock: bool` 换成三态 `FilpLockMode { Opcl, None, ReadWrite }`，拒绝规则内聚到 `get_filp`。这也是 P2-1 错误枚举收敛的前置示例（`FilpError::Closed` 在此入口统一出口）。

---

## 4. P2：结构性改进（正确性 gate 通过后规划）

### P2-1 错误枚举收敛：20 个模块级 Error + 29 个 `to_errno`

**问题**：`pub enum *Error` 共 20 个（grep 全 crate；bdev/cdev/sdev/socket/fs_comm/filp/filedes/link/exec/coredump/read_write/request/protect/tll/path/stadir/fproc/device_map/main_loop 各一），`to_errno` 实现 29 个；且 filp.rs 与 filedes.rs **各有一个同名 `FdError`**。VM 侧 todo（02-stage-vm P2-2）有同款问题，属项目级模式。
**影响**：同一 errno（如 EIO）的映射逻辑散在近 30 处；P0-1 正是这种分裂的直接产物（filp 层 EIO、fd 层 EINVAL）。
**建议**：
1. 首选：crate 级单一 `VfsError`（`#[non_exhaustive]`）+ 各模块 `From<ModuleError>`；errno 映射集中一处。
2. 次选：保留模块枚举，但抽 `trait ErrnoMap` 并以宏统一 `to_errno` 样板；重名 `FdError` 至少改一个。
3. 参考 Rust 社区：no_std 下 thiserror 不可用时手写 `From` 链是常规做法；关键是映射点唯一。

### P2-2 `try_from_raw` 64 臂同构 match 与 `CallTable` 冗余

**问题**：`VfsCallNum::try_from_raw`（call_table.rs:111 起）用 64 个 `v if v == Self::X as u32` 同构臂穷举。且 `CallTable`（call_table.rs:206）把全部 64 个调用号都装配为 `Some`——与 C 一致（call_vec 64 项全满），但其信息量因此与 `VfsCallNum::from_raw` 完全等价，`CallResolver` trait 的 `NullResolver` 实现是 Gate D 双实现要求的产物（call_table.rs:330-345 注释自述）。
**影响**：三层机制（enum/CallTable/CallResolver）表达一个范围检查；维护时 64 臂是纯噪音。
**建议**：
1. `try_from_raw` 改为 `match raw - VFS_BASE { 0 => Some(Read), ... }` 或派生宏生成。
2. 评估删除 `CallTable`，`lookup` 直接 `VfsCallNum::from_raw(call_nr).is_some()`；若 Gate D 需要双实现，用 `from_raw`（真实）与 `deny-all`（测试）两个函数级实现即可，不必背一个 struct。
3. 若保留，给 `CallTable` 一条存在性注释说明与 `from_raw` 的关系。

### P2-3 测试替身裸露在生产编译单元

**问题**：`MockFsClient`（request.rs:492）、`ScriptedTransport`（bdev.rs:87）、`ScriptedChannel`/`SilentChannel`（sdev.rs:230/:276）、`EmptyTable`/`ScriptedAlloc`/`FailingAlloc`（socket.rs:232/:285/:345）、`MockTransport`（fs_comm.rs:438）、`NoTty`（cdev.rs:60）等都在 `#[cfg(test)] mod tests` 之外以 `pub` 定义——各文件的 `#[cfg(test)]` 都在数百行之后。
**影响**：no_std 生产库的公共 API 被测试脚手架污染；`cargo bloat` 层面的死重量与误用面。
**建议**：统一移入 `#[cfg(test)]`，或建 `pub mod test_support` 并 `#[cfg(test)]` 圈定。这是纯机械项，可与 P3-1 clippy 清理同一轮做。

### P2-4 `NextFit` 是无 C 来源的自造策略

**问题**：`filedes.rs:93` 的 `NextFit` fd 分配策略无任何 C 对应——C `get_fd`（filedes.c:121 起）是 start→OPEN_MAX 最低空闲线性扫，O_DUPFD 亦然。它是 Gate D"trait ≥2 个行为不同的 impl"压力下的虚构第二策略（系统调用面对位报告的 Rust 添加表发现）。
**影响**：若被误用为生产策略，fd 分配行为偏离 C；至少是教学误导。
**建议**：改名 `NextFitDemo` 并注释"非 C 语义，仅 Gate D 演示"，或改用真实存在的第二语义（如 `start` 从 O_DUPFD 语义推导）。

### P2-5 类型化欠账：裸 `i32` 端点与 `usize` vnode 引用

**问题**：crate 主流已类型化（`Endpoint`/`UserSlot`/`FilpId`/`VnodeId`/`VmntId`/generation·slot 编码），但 `device_map.rs` 全线用裸 `i32` 端点与 `u32` 主设备号（如 `driver_match(table: &DmapTable, proc: i32, major: u32)`，device_map.rs:152）；`filp.rs:195` 的 `find_by_vnode(vnode: usize, ...)` 用裸 usize 指向 vnode 表。
**影响**：端点与 errno/0 哨兵混用的 C 病得以延续；`usize` vnode 引用可指向已回收槽位（无 generation 保护——对比 `FilpId`/`UserSlot` 自身的 generation 设计）。
**建议**：device_map 改用 `Endpoint`；filp→vnode 引用改 `VnodeId`（若 vnode 表有回收复用，评估带 generation 的引用，参照 `generational-arena` crate 思路）。

### P2-6 `TransIdCodec` trait 双重定义

**问题**：transid 编解码 trait 在两处独立定义——`fs_comm.rs:77-79`（`encode`/`decode`/`is_fs_transid`，附 `VfsTransIdCodec` 与 `TestTransIdCodec`）与 `main_loop.rs:134-137`（同名三方法，附两个 impl）。实现逻辑相同（0xB00 基、0xB01 标记、低 16 位槽位），彼此无关联。
**影响**：协议常量改动要改两处；`route_message` 与 `fs_comm` 对同一 wire 格式可能漂移。
**建议**：收敛到一处（建议 minix-types 或 fs_comm），main_loop 复用；`ARCH A-4` 标注迁移。

---

## 5. P3：代码卫生

### P3-1 clippy 真实 lint 清理

59 条行级告警中 21 条是 workspace profile 配置噪音；真实项以 `collapsible_if` 8 条、`needless_range_loop` 3 条、unused import 2 条、doc 格式 6 条、`Default` impl 缺失 2 条为主。全部机械项，一轮清完；`needless_range_loop` 涉及表遍历改迭代器时注意保持索引语义（C 对位注释里的行号锚点）。

### P3-2 简化删除集中登记

C-10 表所列"单线程下有意删除"的 C 函数（进程锁族、check_*_locks debug 断言族、unlock_filps 等）目前删除决定散落各处注释。建议在本文件 §7 或 99-global-concepts.md 建一张"有意省略表"（函数名 + C 锚点 + 删除理由），防止后续轮次被当缺口重复报告，也让 ARCH A-1 的推论可审计。

---

## 6. 对照 Redox 的架构参考

以下事实来自 2026-09-06 对 Redox 官方文档、docs.rs 与月报的联网调研（每条附来源）；标注 [INFERRED] 的是由事实到本项目的推断。源码级类型名（`KernelSchemes` 等）因源站抓取失败未验证，不作断言。

1. **packet 模型与双侧句柄映射**：Redox 内核把调用者的文件操作翻译成 packet 发给 scheme 提供者，提供者看到的是内核分配的不透明 handle 而非调用者的 fd，(pid, fd) ↔ (pid, handle) 映射由内核维护，handle 区间顶部 4096 个保留作错误码；每个请求携带 `CallerCtx`（调用者身份）（[Scheme Operation](https://doc.redox-os.org/book/scheme-operation.html)、[redox-scheme crate](https://docs.rs/redox-scheme/latest/redox_scheme/)）。**对照**：这是 minix-rs IPC transport trait 需要内建的不变量——"提供者句柄 ≠ 调用者 fd"，`CallerCtx` 对应 worker slot 记录的 `m_source`。[INFERRED]
2. **协议演进的警示**：Redox 为统一接口把整个 packet 协议推倒换代过一次，并为扩展调用预留了 `call`/`std_fs_call` 通道（2025-12 月报确认新 packet protocol 迁移完成，[this-month-251231](https://www.redox-os.org/news/this-month-251231/)）。**对照**：minix-rs 的 `FsCaller`/transport trait 一旦随接线固化，演进成本很高——trait 应预留扩展调用口子。[INFERRED]
3. **取消与对端死亡是后补的教训**：Redox 2025 年才为 scheme 补齐 cancellation，且"Redox does not currently restart daemons automatically"——daemon 死后其资源请求悬死（[this-month-250430](https://www.redox-os.org/news/this-month-250430/)、Scheme Operation 页）。**对照**：minix-rs 的 suspend/revive 状态机在接线后必须显式处理两个分支——调用者先死（在途 slot 取消）与对端驱动消失（C 已有 `unsuspend_by_endpt` 对应，pipe.c:334）；测试替身不暴露这两条路径，P1-2 的关闭条件应包含它们。[INFERRED]
4. **路由与后续操作解耦**：nsmgr 模型中每次 open 做一次 scheme 路由，之后内核"only needs to dispatch…based on the dir_fd"，且内核不再保存任何 scheme 名（[Namespace and CWD as capabilities](https://www.redox-os.org/news/nlnet-cap-nsmgr-cwd/)）。**对照**：Minix3 的 vmnt 查找同样只发生在 open/解析期，vnode 句柄接管后续操作——重写保持这一性质即可，不需要为吞吐引入新机制。[INFERRED]
5. **redoxfs 验证了单进程事件循环的可行性，也暴露了它的上限**：`file` scheme 的提供者就是 redoxfs——一个拥有全部文件系统状态的用户态 daemon，COW + 事务保证一致性（[RedoxFS](https://doc.redox-os.org/book/redoxfs.html)、[schemes.html](https://doc.redox-os.org/book/schemes.html)）；其 `Disk` trait（含 `DiskCache`）与本项目"硬件藏在 trait 后"约束同构；但官方路线图把 improved concurrency 列为 NLnet 待办，说明单 daemon 是已知吞吐瓶颈（[this-month-260630](https://www.redox-os.org/news/this-month-260630/)）。**对照**：minix-rs 单线程 VFS + slot 池选型可行；接真实块设备时读路径批量化应尽早设计（Redox 用 `SYS_CALL` 合并 dup+read/write+close 的方向相同）。[INFERRED]
6. **批量 fd 转移**：Redox 的 `CallFlags::FD` 让内核一次原子搬运一组 fd（"the kernel removes FDs from the sender's table, the scheme queues them, and the kernel adds them to the receiver's table"），并把 fd 分配整体移到了用户态（[RSoC 2025 fdtbl](https://www.redox-os.org/news/rsoc-2025-fdtbl/)、[this-month-260630](https://www.redox-os.org/news/this-month-260630/)）。**对照**："fork 时逐个 fd 走一轮 IPC"是 Minix3 模型里最贵的序列之一；minix-rs 忠实还原 `pm_fork` 时若消息协议允许，值得为批量 filp 拷贝留口。另 Redox 经验指出 free_proc 级联中"每步都可能触发 revive"应显式建模（ipc/dispatcher.rs 的 `FreeKind::Exiting` 路径目前是弱化模型）。[INFERRED]
7. **微内核分工**：文件系统调用由 scheme 处理、驱动是独立用户态程序，内核约 5 万行（[How Redox Compares](https://doc.redox-os.org/book/how-redox-compares.html)）。**对照**：与本项目 servers/drivers 拆分一致；Redox 阻塞语义为"调用者阻塞在内核、daemon 经 event scheme 多路复用在途请求"（Scheme Operation 页），单线程事件循环 + 挂起复活达成的正是同一性质，无需向 mthread 级多线程演化。

---

## 7. 模块级观察（一行式，供后续轮次参考）

- **main_loop.rs**：病灶集中地——双分发契约（P1-1）、Read 占位符、SEF 空、`run()` mock 自旋；`route_message`/`unblock`/`next_reviving_slot` 本身质量好。
- **filedes.rs**：P0 密集区（P0-1/2/3 全在此），"for test determinism"/"Simplified" 注释是系统性风险信号，建议全文件清点这类注释。
- **filp.rs**：结构与 generation·slot 编码好；`find_by_vnode` 的 usize 引用待类型化（P2-5）。
- **tll.rs / vmnt.rs / vnode.rs**：三表状态机完整、测试密；vmnt 缺升降级（C-4）是唯一不一致。
- **pipe.rs**：`SuspCount`/`susp_delta`/`WakePlan`/`rollback_for` 分解是全 crate 最佳实践样本。
- **select.rs**：决策函数纯化到位；P1-4 是唯一的语义义务缺口。
- **call_table.rs**：三层冗余（P2-2），但 `Route`/`SyscallResult` 的类型设计本身值得保留。
- **request.rs**：FsReq 32 变体对 C 全对上、`send_with_retry` 结构好；是 REQ 协议的权威定义处。
- **device_map.rs**：决策件丰富（`register_plan`/`RecoverVerdict`/`VanishNotice`），类型化欠账（P2-5）。
- **socket.rs**：`BuildStep`/`compensate` 失败补偿表设计好（C check_sock_fds 失败清理的忠实建模）。
- **coredump.rs / misc.rs / stadir.rs**：纯函数化彻底；misc.rs 的 scope note 注释诚实，是 DEFERRED 标注的范本。
- **fs_comm.rs**：GlobalComm 窗口模型对 C comm.c 语义完整；与 main_loop 的 TransIdCodec 重复待收敛（P2-6）。
- **ipc/dispatcher.rs**：PM 面覆盖全（fork/exit/身份四操作/setgroups），P1-3 是唯一语义洞。

---

## 8. 建议的推进顺序

1. **P0-1 / P0-2 / P0-3**（filedes.rs 的三处，一次设计：`FdError::Closed` + count 闸 + 端点匹配），顺手补 C-3 的 by_char_major/by_sock_drv。
2. **P1-3 / P1-4 / P1-5**：三处"义务未编码"类语义洞，量小且独立。
3. **P1-1**：收敛双分发契约 + Route::Enosys——这是接线的先决设计决定。
4. **P1-2**：按建议顺序推进接线矩阵（IPC → SEF → dmap/DS → 根挂载 → path 循环 → 驱动级联），并与 mfs 侧协调 `fs_lookup` 落地（C-6 的另一半）。
5. **P2 全部**：P2-1 错误收敛建议在第 1 步之后做（P0-1 已为其铺路）；P2-2/P2-3/P2-4 可与 P3-1 同轮机械清理。
6. **P3-2**：在 99-global-concepts.md 落"有意省略表"，防重复报告。

---

## 附录 A：覆盖率穷举证据（gate-evidence-A 等价物）

命令与输出（2026-09-06 实测，工作目录仓库根）：

```
$ python3 tools/coverage-extract/coverage-extract.py vfs \
    notes/rewrite/fork-syscall-rewrite/05-stage-vfs \
    --rust-dir os --c-dir minix3/minix/servers/vfs \
    --semantic-map tools/coverage-extract/vfs-semantic-map.json \
    --output .review/claude/vfs/scans/SYMBOLS.md
Module: vfs / C source: minix3/minix/servers/vfs
Found 415 C symbols (311 funcs, 10 structs, 92 macros, 2 enums)
Coverage Summary for vfs:
  Total C symbols: 415
  Doc covered: 387 (93.3%)
  Rust covered (name-match): 262 (63.1%)
```

- 语义映射表 `tools/coverage-extract/vfs-semantic-map.json` 为首轮审查新建（此前只有 15-stage-fs 驱动侧的 `fs-semantic-map.json`），映射条目由首轮会话与对位 agent 逐条验证。
- 完整符号表落盘：`.review/claude/vfs/scans/SYMBOLS.md`（含逐符号 C 锚点、文档覆盖、Rust 匹配三列）。
- 机器数字为下限的三个原因：枚举变体不被提取为符号；"一 C 函数 → 一组纯函数"分解风格；注释提及不计入匹配。权威缺口判定见 §1 人工表。

## 附录 B：基线验证命令

```
$ cargo check -p minix-vfs          # 通过，lib 7 warnings
$ cargo test -p minix-vfs --lib     # 334 passed / 0 failed
$ cargo clippy -p minix-vfs --lib   # 59 行级告警（21 条 workspace profile 噪音）
```
