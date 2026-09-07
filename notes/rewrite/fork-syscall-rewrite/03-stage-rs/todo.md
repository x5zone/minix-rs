# 03-stage-rs TODO 总清单（rs server Rust 实现深度 review + 改进点收集）

> **来源**：2026-08-15 Codex 深度 review —— 仅针对 `os/servers/rs/` 的 Rust 实现（22 个模块，~8.9k 行），
> 自顶向下（架构 → 模块边界 → 数据结构 → C 语义保真 → 健壮性/错误处理 → 测试）全面审查，
> 对照 Redox OS 设计、OS 方向与 Rust 社区最佳实践。
> **证据基线（初始）**：`cargo test -p minix-rs` = **181 passed**；`cargo check` 通过（1 个 unused
> import warning）；`cargo clippy -p minix-rs --lib` = 3 项（见 §8.3）；`cargo fmt --check` = 1 处
> （privilege.rs:303）。
> **修复后基线（2026-08-15 修复轮，见 §10 Fix-Status）**：`cargo test -p minix-rs` = **193 passed**；
> `cargo check`/`clippy`/`fmt` 全部干净；`tools/check-rs-unwired.sh`（T7 门禁）PASS。
> **重要前提**：当前所有"生产运行路径"均 fail-closed（`KernelApi` 未接线、`minix-sys::receive` 为 `todo!()`，
> `main()` 在 `init()` 即 panic），因此**无 P0 运行时缺陷**；下列问题以 P1（接线前必须决策/修正的设计与
> 潜在代码缺陷）和 P2（改进项）为主。

## 0. 审查基线 — 已确认的良好架构（无需改动）

| 维度 | 现状 | 评价 |
|------|------|------|
| 执行模型 | 单线程事件循环，状态全部经 `&mut self` 线程化传递，**全 crate 无 `unsafe`** | ✅ 符合 AGENTS.md 用户态服务器模型（对照 01-stage-kernel 的全局访问器问题，RS 没有该问题） |
| 错误失败策略 | 未接线路径全部 fail-closed（`UnimplementedKernelApi` / placeholder 回调 / `unimplemented!()`） | ✅ 比"假装可用"安全，见 §2 T4 的改进空间 |
| C 语义编码 | bitflags 位值、errno 常量、消息号均有逐位测试断言（`test_rflags_bits_match_const_h` 等） | ✅ 防漂移 |
| 覆盖率 | 181 个单元测试，纯函数切片模式使决策逻辑全部可测 | ✅ 显著优于 C 原版的不可测性 |
| 内存安全 | `Arc<[u8]>` 替代 C 裸指针共享 exec 镜像（ARCH A-5）、`Option<SlotId>` 替代 `rproc_ptr` 裸指针四链（ARCH A-3/A-4） | ✅ |
| 类型化改进 | `UpdatePhase`/`ReadyOutcome`/`TerminateAction` 等枚举替换 C 位标志分支，非法状态不可表达（ARCH A-6） | ✅ 方向正确 |

## 1. 建议总览

| ID | 主题 | 严重度 | 状态 |
|----|------|--------|------|
| T1 | BootInit 吞掉运行期状态，主循环无法访问 table/hz/rupdate（状态 handover 断链） | P1 |✅ |
| T2 | `KernelApi` 单体 trait + 15 个方法中 6 个默认 `unimplemented!()`（运行期 panic 面） | P1 |✅ |
| T3 | 错误处理：裸 `i32` errno 全 crate；A-12 声称的 `Errno` newtype 未落地；`ENOSYS` 语义过载 | P1 |✅ |
| T4 | `get_work` 伪造 `IpcStatus{flags:0}`，主循环骨架"看起来能跑"实则必然 panic | P2 |✅ |
| T5 | `&mut dyn KernelApi` 散落各纯函数参数，注入边界不统一 | P2 |✅（§13） |
| T6 | Step 2/3 计数语义偏差（VM 不计入、无视 SF_SYNCH_BOOT、Step 3 fail-open） | P1 |✅ |
| T7 | 20 处 `unimplemented!()`/`todo!()` 无编译期/CI 门禁，19 接线遗漏即系统级故障 | P1 |✅ |
| D1 | `ServiceSlot` god struct（~35 全公开字段）无封装、无不变式 | P2 | 🔶 | 判定闭合：全拆不采纳（镜像保真是 A-3 契约），R33 已修（Fix #68）；重开权在用户 |
| D2 | `SlotId` 无世代/代数，free→reuse 后旧索引悬垂；`get()` 越界 panic | P2 | 🔶 | 判定闭合：世代不采纳 + assert_consistent 落地（Fix #67，2026-09-07）；越界 panic 已随 Fix #29 |
| D3 | 公共函数 totality：`caller_can_control`/`lookup_by_domain` 对越界计数可 panic | P1 |✅ |
| D4 | 常量双定义（`RS_MAX_LABEL_LEN` 两处等） | P2 |✅ |
| D5 | `RsStart::default` 与 C 调用方默认不一致（sigmgr=SELF/scheduler=KERNEL/quantum=1 vs RS/SCHED/200） | P2 |✅ |
| D6 | `Privilege`/`ServiceSlot` 双份 io/irq 表（C 同构，可改进为单一权威） | P2 | ✅ | 已修（Fix #62，2026-09-06，单一权威+派生快照） |
| S1 | `build_cmd_dep` 不遇 NUL 停止 + `Label` 16 字节截断参数（潜在代码缺陷） | P1 |✅ |
| S2 | `activate_boot_slot` 缺失 cmd/script/argc/vm_call_mask/scheduler/priority/quantum/alive_tm | P1 |✅ |
| S3 | `sched_init_proc` 对 `NONE` 调度器不跳过（C sched_start.c:57-58） | P1 |✅ |
| S4 | `KernelApi::sched_init_proc` 签名丢弃调度参数（scheduler/priority/quantum/cpu） | P1 |✅ |
| E1 | MockKernelApi 四份复制、无共享测试工具模块 | P2 | ✅ |
| E2 | 解析函数（IpcListIterator/parse_label/build_cmd_dep）无 fuzz/property 测试 | P2 | ✅ | 已修（Fix #64，2026-09-06，零依赖 property 测试） |
| E3 | 无集成级 boot 顺序/消息交换测试（receive 不可用） | P2 | 🔶 | mock 层集成面已落地（Fix #65/#69/#70/#77）；R34.22 signal_manager 收尾归 §18.11 队列 I6 |

## 2. 顶层架构问题

### T1. BootInit 吞掉运行期状态，主循环无法访问 [P1]

**现状**：`RsServer`（`lib.rs:160-197`）持有 `boot: BootInit<'static>`；4 步 boot 建好的
`RProcTable`、`system_hz`、`shutting_down` 全部封在 `BootInit` 内部。`BootInit` 只暴露
`rinit()`（`boot.rs:394-399`），**没有 table / system_hz / shutting_down 的访问器**。
`RsServer::run()` 因此无法执行 `do_period`（需要 hz + table）、无法响应 RS_DOWN 的 shutdown
sweep（需要 `shutting_down`）、无法访问任何服务槽。

**问题分析**：06-rs-main-loop.md 落地时，要么给 `BootInit` 加 5+ 个 getter（退化为 getter 垃圾场），
要么重构状态归属。当前 `BootInit` 把"boot 期状态"与"运行期状态"混为一个结构，生命周期语义不清。

**改进思考**：
- 采用 typestate：`BootInit<Fresh>` → `init_fresh()` 消费 self 返回 `RsRunning { table, hz, shutting_down, rupdate, kernel }`，主循环只持 `RsRunning`。编译期禁止"boot 前访问 table / boot 后修改 boot 表"。
- 或最小改动：`RsServer` 直接持有 `RProcTable`/`system_hz` 等字段，`BootInit` 只做 4 步过程（传入 `&mut ServerState`）。C 的全局 `rproc[]`/`rupdate`/`system_hz`/`shutting_down` 本就同属一个进程，Rust 应显式建模为一个 `RsState`。
- 对照 Redox：`init` 的 daemon 生命周期由配置驱动，状态机边界清晰；RS 的 boot→run 是同一进程内状态迁移，正适合 typestate 表达。

### T2. KernelApi 单体 trait + 默认 `unimplemented!()` [P1]

**现状**：`KernelApi`（`boot.rs:44-150`）有 15+ 方法，其中 `srv_fork`/`getprocnr`/`vm_memctl`/
`vm_set_priv` 是带 `unimplemented!()` 的默认实现（`boot.rs:88-150`）；生产 impl
`UnimplementedKernelApi`（`boot.rs:152-184`）的 8 个方法全部 `unimplemented!()`。
`main.rs` 用 `UnimplementedKernelApi` 构造服务器，`init()` → `step0_prepare` → `get_hz()` 立即 panic，
`let _ = server.init(...)`（`main.rs:29`）连错误都吞掉了。

**问题分析**：
1. **默认实现是定时炸弹**：19 接线时若漏替换任一方法，该路径在运行期 panic —— RS 死亡 = 系统服务管理器死亡，Minix3 内核不会重启 RS（`RSYS_F` root sys proc），整机挂。比返回 `Err(ENOSYS)` 危险得多。
2. **单体 trait 混合了 4 个不同消息目标**：`getnuid/getnpid/getprocnr/srv_fork` 是发往 PM 的 IPC；
`vm_memctl/vm_set_priv` 发往 VM；`privctl/getpriv/sched_init_proc/setalarm/get_machine/get_hz` 发往 kernel；
`setalarm` 是 kernel；还有 DS（parse_label 的 lookup 回调）。一个 trait 隐藏了"这条调用发给谁"。

**改进思考**：
- 拆能力 trait：`SysApi`（kernel 调用）/ `PmApi` / `VmApi` / `SchedApi`，每个方法标注目标端点；
  `RsServer` 持 `dyn SysApi + dyn PmApi + ...`。类型系统强制"发消息给谁"可见，与 Minix3 消息面（19）
  一一对应，也与 Redox `syscall::call` 按目标 scheme 分层的思路一致。
- 至少做到：**删除所有默认实现**，让编译期强制每个 impl 全量实现；未接线的 impl 返回 `Err(ENOSYS)`
  而非 panic。可加 `#[cfg(test)]` 专用 mock，生产只用 `UnimplementedKernelApi`（fail-closed Err 版）。
- 接线门禁：CI 中 `rg "unimplemented!|todo!" os/servers/rs/src` 计数必须为 0 才允许移除 fail-closed 标记。

### T3. 错误处理：裸 i32 errno 与 A-12 未落地 [P1]

**现状**：全 crate 用 `Result<_, i32>`；`minix-sys` 已有一个 `Errno` enum（`minix-sys/src/lib.rs:106-128`，
23 个值）但 RS 未使用，且缺 RS 需要的 `EDONTREPLY`/`EDEADEPT` 等值。design 的 ARCH A-12 声称
"errno + panic + printf → `Errno(i32)` newtype + `Result`"，与代码不一致（doc-code mismatch）。
另外 `ENOSYS` 被三种不同语义复用：boot 表数量不匹配（`boot.rs:227-241`）、lookup 失败
（`boot.rs:452`）、getnpid 返回负值（`boot.rs:556`）——后者在 C 里是 `panic("unable to get pid")`
（main.c:427-429），是**内部不变式违例**，不是协议错误。

**改进思考**：
- `Errno` newtype（`minix_types` 承载，Redox `redox_syscall::Error{errno}` 同款共享 ABI 定位）：
  `Result<T, Errno>`，`Errno` 提供 `from_i32`/`to_i32`（wire 面）+ `Display`。01-stage-kernel §D1 已有
  同类结论，RS 应在接线前统一，避免 19 接线时三套错误并存。
- 区分错误域：`BootInit` 内部不变式违例用独立 `BootError`（→ panic 或 `Err(EGENERIC)` 都行，但不能
  伪装成 `ENOSYS`）；协议面才用 errno。

### T4. 主循环骨架静默错误 [P2]

**现状**：`get_work`（`lib.rs:190-197`）调用 `minix_sys::receive`（`todo!()`），随后
**伪造** `IpcStatus { flags: 0 }`。`run()` 中 `_kind` 计算后直接丢弃（`lib.rs:182-184`），
`dispatch_request` 对一切请求返回 `ENOSYS`。

**问题分析**：`flags: 0` 意味着 `is_notify()` 恒 false —— 将来 receive 落地后，真实的 notify
消息（CLOCK/心跳）会被错误分类为 `Request(n)`。骨架"看起来能跑"（有循环、有分类、有匹配），
实则既不能跑、分类逻辑也是错的，比显式 `unimplemented!` 更误导。

**改进思考**：
- 骨架阶段就让 `run()` 显式不可达：`fn run(&mut self) -> Result<(), Errno>` 或 `todo!("06")`，
  不要伪造状态字。
- 分类器与状态字解耦：`classify` 接收真实的 `ipc_status`（由 receive 原语产出），
  06 落地前用 `#[cfg(test)]` 覆盖全部 4 类分支（现在测试只覆盖 notify 与 RS_INIT）。

### T5. 纯函数注入边界不统一 [P2]

**现状**：`check_call_permission`（`access.rs:56`）、`add_forward_ipc`（`ipc_mask.rs:113`）、
`sched_init_proc`（`sched.rs:69`）各自把 `&mut dyn KernelApi` 作为参数传入；`monitor.rs`/`ready.rs`/
`recovery.rs` 又走"结果枚举 + 调用方执行副作用"模式。两种注入风格并存。

**改进思考**：统一为一种：要么全部"决策函数返回枚举，调用方注入副作用"（monitor 模式，纯度最高，
测试最干净）；要么定义一个 `ServerCtx`（`kernel: &mut dyn KernelApi` + `hz` + `now`），
避免 `&mut dyn` 裸传。倾向 monitor 模式 —— 它让 `KernelApi` 只在 19 的一个接线层出现，
纯函数层完全不感知 syscall 面（与 Redox "用户态逻辑与 syscall 分离"一致）。

### T6. Step 2/3 boot 计数语义偏差 [P1]

**现状**：`step2_allow_run`（`boot.rs:491-528`）：
- RS/VM 分支直接 `continue`，注释说 init_service 属 12（DEFERRED）——但 C 中 **VM 会计入
  `nr_uncaught_init_srvs`**（main.c:371-372："VM will still send an RS_INIT message"），Rust 不计数；
- 对 `SYS_PROC` 服务全部 `nr_uncaught_init_srvs += 1`，**无视 `SF_SYNCH_BOOT`**——C 对 SYNCH_BOOT
  服务同步 `catch_boot_init_ready(ep)`（main.c:390-392）且**不**入计数。
`step3_catch_init_ready`（`boot.rs:530-538`）只是 `while counter>0 { counter-=1 }`，不接收、不校验 ——
**fail-open**：C 是阻塞接收（收不到就永远卡在 boot，fail-closed）。

**问题分析**：当前 boot 表恰好没有 SYNCH_BOOT 服务（`table.rs` sys 表无 SYNCH_BOOT），且 VM 计数缺口
被 step3 的"无条件减计数"掩盖，所以 181 个测试全绿。12 落地时这两处偏差会导致 boot 提前完成/死等。

**改进思考**：step3 在 12 落地前应显式 `Err(ENOSYS)` 或 `todo!`（fail-closed），而不是"假装收完"；
计数逻辑按 main.c:348-399 逐分支对照（VM 计数、SYNCH_BOOT 同步 catch、其余入计数），
并加对应测试（构造含 SYNCH_BOOT 的 boot 表断言计数）。

### T7. 占位符无门禁 [P1]

**现状**：`unimplemented!()` 共 20 处：`boot.rs` 13（`KernelApi` 默认 impl 4：
srv_fork/getprocnr/vm_memctl/vm_set_priv + `UnimplementedKernelApi` 8 + `self_update` 1，
其中 `self_update` 有 `live-update` feature 门控 ✅）、`sef.rs` 7 个 placeholder
（`sef.rs:118-149`）；另有 `minix-sys::receive/send` 2 处 `todo!()`、`lib.rs:193` 的 `expect`
（依赖 receive）。
另有 `RsServer.callbacks` 字段 `#[allow(dead_code)]`（`lib.rs:165-167`）。

**改进思考**：见 T2 的 CI 门禁；`live-update` feature 门控是正确先例（fail-closed 编译排除），
可推广到 06/12/18 的占位路径（feature 或 `#[cfg]` 排除），让"默认 build 不含任何 unimplemented 可达路径"。

## 3. 数据结构与类型设计

### D1. ServiceSlot god struct [P2]

**现状**：`ServiceSlot`（`service_slot.rs:311-380`）约 35 个 `pub` 字段，实例身份（pid/endpoint/flags）、
策略（sys_flags/control/ipc_list）、监控（period/check_tm/alive_tm）、调度（scheduler/priority/quantum/cpu）、
exec、priv 全平铺；`pub_` 嵌入 `PublicSlot`。无私有字段、无不变式方法，任意不一致状态可表达
（如 `pid=Some` 但无 `IN_USE`）。

**改进思考**：镜像 C 是文档可追溯性的需要，但可加**薄封装层**：
- 按语义分组子结构（`Identity`/`Policy`/`Monitor`/`Sched`），字段访问经方法；
- 提供 `fn valid(&self) -> bool`（debug_assert 用）或在 `mark_child_created`/`free_slot` 等转移点
  维护不变式；
- `#[non_exhaustive]` 防止未来字段增补破坏外部构造。
- 对照 Redox `KernelScheme`/`Resource`：状态被拆到独立对象 + `Arc` 句柄，无 god struct。

### D2. SlotId 无世代，悬垂索引风险 [P2]

**现状**：`SlotId(pub usize)`（`service_slot.rs:160-186`），`RProcTable::get`/`get_mut`
（`process_table.rs:170-178`）直接 `&self.slots[id.0]` —— 越界即 panic。`free_slot` 后槽位可被
`alloc_slot` 复用，旧 `SlotId` 会静默指向另一个服务。`UpdateChain`/`instances_of`/`swap_slot`
都持有跨步骤的 `SlotId`，悬垂风险真实存在（C 的 `struct rproc*` 指针有同样问题，但 Rust 本可做得更好）。

**改进思考**：
- 最小改动：`get`/`get_mut` 返回 `Option<&ServiceSlot>`（fail-closed），内部路径 `?` 传播；
  `SlotId::new` 改为 `Result` 或 `NonZeroUsize` 包装。
- 结构性改进（对照 Redox `Arc<Resource>` 句柄模型）：槽位用 `Rc<RefCell<ServiceSlot>>` 或
  `SlotId { index, generation }`（generation 随 free 递增，`RProcTable` 校验），悬垂句柄在借用时被拒绝。
- 若选 generation：`alloc_slot`/`free_slot` 递增槽位 generation，`get` 校验 id.generation == slot.generation
  —— 与 Minix endpoint generation（`endpoint.h`）机制同构，语义自洽。

### D3. 公共函数 totality（越界计数 panic）[P1]

**现状**：
- `caller_can_control`（`access.rs:37-47`）：`caller.control[..caller.nr_control.max(0) as usize]` ——
  `nr_control > RS_NR_CONTROL(8)` 时 panic。该字段仅由 DEFERRED 的 `edit_slot` 路径校验
  （C：`manager.c:1543-1556` 返回 EINVAL）。
- `lookup_by_domain`（`process_table.rs:293-320`）：`(0..pub_.nr_domain as usize)` 内
  `pub_.domain[i]` —— `nr_domain > NR_DOMAIN(8)` 时 panic（u8 上限 255，C 同样在 edit_slot 校验）。
- `RProcTable::get` 越界 panic（D2）。

**问题分析**：这些是 `pub` 函数，可被未来消息处理代码以任意表状态调用；单线程服务器内 panic =
RS 死亡 = 整机不可用。C 的等价代码对越界计数在入口校验（返回 EINVAL），Rust 应在类型/边界处兜底。

**改进思考**：`control.get(..n)`/`domain.get(..n)` + fail-closed（返回 false/None）；
或在 `RProcTable` 层提供 `validate_slot(&ServiceSlot) -> Result<(), i32>` 作为所有消息入口的公共闸门
（对照 `check_request` 的模式，把"槽内字段合法"做成一个可复用校验函数）。

### D4. 常量双定义 [P2]

**现状**：`RS_MAX_LABEL_LEN` 在 `service_slot.rs:14` 与 `state_data.rs:11` 各定义一份；
`SEF_INIT_*` 在 `request.rs:7-14`，`SEF_INIT_ST` 在 `live_update.rs:89`；`SEF_LU_STATE_*` 在
`live_update.rs:91-92`；`MAX_BACKOFF`（`monitor.rs:23`）与 recovery 的 backoff 计算分开。

**改进思考**：同值常量收敛到单一出处（`service_slot` 或专用 `consts` 模块），`state_data.rs`
`use crate::service_slot::RS_MAX_LABEL_LEN`；加 `#[cfg(test)]` 交叉断言防止未来漂移（现在靠各模块
独立测试，跨模块不一致不会被发现）。

### D5. RsStart::default 与 C 调用方默认值不一致 [P2]

**现状**：`RsStart::default`（`slot.rs:130-176`）给 `sigmgr=Endpoint::SELF`、`scheduler=Endpoint::KERNEL`、
`quantum=1`。C 侧 `do_up` 通过 `copy_rs_start`（`request.c:39`）从调用方**整结构直拷**
（`manager.c:131-146` 的 `sys_datacopy`，无服务端零初始化）；默认值由调用方注入
（minix-service `parse.c:1165-1168`：`sigmgr=RS(ROOT_SYS_PROC_NR)`、`scheduler=SCHED_PROC_NR`、
`quantum=USER_QUANTUM=200`，priv.h:84/89/99 + config.h）。

**问题分析**：Rust `Default` 的 `SELF`/`KERNEL` 是"未设置"哨兵值而非 C 的默认值，且 `quantum=1`
与 C 的 `USER_QUANTUM=200` 差 200 倍。任何按 `Default` 构造再局部填充的路径都会得到与 C 不同的
调度配置（scheduler 从 SCHED 变 KERNEL），且 `check_request` 无法识别（KERNEL/SELF 都是合法值），
静默通过校验。

**改进思考**：镜像 C 调用方默认（`sigmgr=RS`、`scheduler=SCHED`、`quantum=200`）或移除 `Default`，
用显式构造器 `RsStart::from_wire(...)` + 解析层（`copy_rs_start` 对应代码，19）注入与 minix-service
一致的默认；两者取一，避免"未设置"哨兵与"默认值"混用。

### D6. Privilege/ServiceSlot 双份资源表 [P2]

**现状**：`ServiceSlot.io_tab/nr_io_range/irq_tab/nr_irq`（`service_slot.rs:349-354`）与
`Privilege.io_ranges/nr_io_range/irqs/nr_irq`（`privilege.rs:283-303`）内容重复（C `type.h:100-103`
注释为 "priv backup"，是 C 原版设计）。

**改进思考**：C 需要备份是因为 priv 结构传给 kernel 后可能被改写；Rust 可改为单一权威
（`Privilege` 持有），`ServiceSlot` 用 `sync_priv_backup(&mut slot)` 在 sys_privctl 前后同步，
消除双写不一致的隐患（现在没有任何代码保证两份一致）。

## 4. C 语义保真度问题

### S1. build_cmd_dep：不遇 NUL 停止 + Label 16 字节截断参数 [P1]

**现状**：`build_cmd_dep`（`slot.rs:202-228`）：
1. token 循环条件 `rest[i] != b' '` —— **不检查 NUL**。命令串 NUL 之后的 0 填充会被吞进 token；
   当前"碰巧正确"是因为 `Label::from_bytes` 截断到 16 字节且 `as_str` 遇 NUL 截断。C 是
   `while (p[0] != '\0' && p[0] != ' ')`（`manager.c:289-323`），遇 NUL 即停。
2. 每个 token 经 `Label::from_bytes` 截断到 16 字节 —— **超过 16 字节的路径/参数被静默截断**。
   C 把完整 token 写入 `r_args[512]` 并让 argv 指向其中。
`rebuild_args`（`service_create.rs:80-100`）同样截断。

**问题分析**：09/10 exec 落地后，`RS_UP` 携带 `/usr/sbin/very-long-service-name` 这类 >16 字节路径时，
argv 会被截断，服务启动失败且错误难以排查。这是"接线后必然触发"的潜在代码缺陷（现路径 DEFERRED
所以测试全绿——注意现有测试只覆盖短 token）。

**改进思考**：
- token 容器改为 `Vec<&[u8]>`（借用 `cmd` 切片，长度任意）或 `Vec<Range<usize>>`；
  `argc`/`argv` 布局与 C `r_args` 对齐（NUL 分隔、完整字节）。
- `build_cmd_dep` 在遇到 NUL 时 `break`（与 C 完全一致），并加测试：>16 字节参数、多个参数、
  NUL 提前终止。

### S2. activate_boot_slot 缺失 boot 字段 [P1]

**现状**：`activate_boot_slot`（`process_table.rs:375-402`）只填 label/proc_name/sys_flags/dev_nr/
endpoint/in_use/flags/priv_/endpoint index。对照 C `main.c:255-345` 缺失：
- `r_cmd` strlcpy + `r_script[0]='\0'` + `build_cmd_dep`（main.c:308-310）→ argc/args/cmd 全空；
- `rpub->vm_call_mask` fill（main.c:317-319）→ 全零；
- `r_scheduler`/`r_priority`/`r_quantum`（main.c:320-322）→ `scheduler=Endpoint::NONE`；
- `r_alive_tm = getticks()`（main.c:333）→ 0。

**问题分析**：boot 槽的 `alive_tm=0` 使心跳/超时判定从启动起就与 C 语义不同（07 落地时，
`now - 0 > 2*period` 会把健康服务误判为超时）；`scheduler=NONE` 会在 sched_init_proc（S3）
的 `debug_assert_ne!(NONE)` 下 debug 构建直接崩。`boot.rs:433-443` 的文档注释声称
"slot population + activation — main.c:258-345"，与实际覆盖不符。

**改进思考**：`activate_boot_slot` 补全字段（注入 `ticks`；vm_call_mask 用 `CallMask::from_calls`
按 `SRV_VC/USR_VC` 填充——C `main.c:317-319`；scheduler/priority/quantum 按 `SRV_OR_USR` 的
`SRV_SCH/SRV_Q/SRV_QT`）；或显式标注 DEFERRED 并让 `init_fresh` 在未补全时 fail-closed。

### S3. sched_init_proc 对 NONE 调度器不跳过 [P1]

**现状**：`sched_init_proc`（`sched.rs:69-90`）无条件 `sys.sched_init_proc(cfg.endpoint)?`。
C `sched_start`（`sched_start.c:57-58`）：`if (scheduler_e == NONE) return OK;` —— 用户进程
（无调度器）**不调用**。测试注释承认了该偏差（"the KernelApi is still invoked because the wiring
is deferred"）。

**改进思考**：`cfg.scheduler == Endpoint::NONE` 时直接 `Ok(cfg.scheduler)`（与 C 一致），
并加测试断言 mock 无调用。

### S4. KernelApi::sched_init_proc 签名丢弃调度参数 [P1]

**现状**：`KernelApi::sched_init_proc(&mut self, proc: Endpoint)`（`boot.rs:77`）只传 endpoint。
C `sched_start` 需要 scheduler/priority/quantum/cpu（`utility.c:372-378` → `sched_start.c:37-80`），
这些参数在 `SchedulerConfig`（`sched.rs:21-53`）里，但 trait 方法收不到 —— 19 接线时要么改签名，
要么从 slot 重读（重复 S3 的不一致风险）。

**改进思考**：签名改为 `fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno>`，
返回值即 `*newscheduler_e`（sched_start 可能转发到别的调度器，C 会回写 `r_scheduler`）。

## 5. 健壮性与 no_std 内存

### R1. panic 面统计与系统服务语义 [P1 → 归入 T2/T7]

`unimplemented!()` 20 处（boot.rs 13 + sef.rs 7）+ minix-sys `todo!()` 2 + `expect`/`unwrap`（`lib.rs:193`，非测试）+ 越界索引（D2/D3）
+ `UpdateChain::get` 越界 panic（`live_update.rs:345-355`）。RS 是 root system process，
panic = 整机不可用。见 T2/T7/D3 的改进。

### R2. 内存布局 [P2]

`RProcTable::new`（`process_table.rs:145-155`）预分配 64 个 `ServiceSlot`（每个含 cmd[512]+args[512]+
script[256]+io_tab[64×8]+mem_tab+irq_tab+priv 数组 ≈ 3.5KB），约 220KB 堆 —— 可接受但可改进：
- 固定表用 `Box<[ServiceSlot; NR_SYS_PROCS]>`（去掉 `Vec` 容量/增长语义，语义上"恒为 64 行"更精确）；
- `ServiceSlot::clone()`（clone_slot/swap_slot 路径）深拷 ~3.5KB/次 —— 频率低可接受；若 D1 拆
  config/instance 两层，配置部分可 `Arc` 共享。

### R3. Arc exec 共享正确性 [✅]

`share_exec`/`has_shared_exec`/`free_exec`（`exec.rs:57-84`）用 `Arc::ptr_eq` + drop 释放，
语义与 C 全表扫描一致（ARCH A-5），无泄漏。保持。

## 6. 测试

### E1. 单元测试质量 [✅]

181 个测试全部通过；RFlags/SysFlags/PrivFlags 位值、errno 常量、消息号均有逐位断言；boot 顺序
（step1/2/4 的 kernel call 序列）有 `MockKernelApi` 记录验证。这是本 crate 最大的优点。

### E2. 解析函数缺 fuzz/property 测试 [P2]

`IpcListIterator`（`ipc_mask.rs:43-88`）、`parse_label`（`state_data.rs:164-230`）、`build_cmd_dep`
（`slot.rs:202`）、`Label::from_bytes` 是请求解析面（攻击者可控输入），目前只有手写样例。

**改进思考**：`proptest`（no_std 可用，仅 test 依赖）对解析函数做 property 测试：
- `IpcListIterator`：任意输入不 panic、输出 token 均 ≤16 字节且不含空白；
- `build_cmd_dep`：任意输入不 panic、token 是 C 语义的子集（S1 修复后补）；
- `parse_label`：任意输入返回 Ok 或 Err 且不 panic。
对照 01-stage-kernel 的 qemu-tests 基建，可进一步做 boot 集成级验证。

### E3. MockKernelApi 四份复制 [P2]

`boot.rs:640-676`、`access.rs:130-160`、`ipc_mask.rs:240-270`、`sched.rs:152-180` 各有一份
`KernelApi` mock，`getnpid` 用 `self.pids.pop().unwrap_or(100)`（`boot.rs:671`）掩盖调用顺序错误。

**改进思考**：`#[cfg(test)]` 公共 `testutil` 模块（crate 内 `mod tests_common`），mock 记录调用序列
并提供 `assert_calls!` 宏；`getnpid` 改显式队列（`VecDeque`），顺序错即失败。

## 7. Redox OS 对照（2026-08-15 调研）

| 维度 | Redox 做法 | rs server 现状 | 对照结论 |
|------|-----------|---------------|---------|
| 服务管理模型 | `init` 是普通用户进程，读 `/etc/init.rc` 派生 daemon；内核不管理服务生命周期、无特权服务注册表 | RS 是 root system process + 内核 priv 表/send mask/心跳/复活（Minix 独有设计） | 设计目标不同（Redox 最小内核 vs Minix 微内核服务治理），但 RS 的 03/04/05 机制在 Rust 中应保留为**纯数据 + 校验函数**（现已是），策略不泄漏到 kernel 面 ✅ |
| 句柄/资源管理 | `Resource = Arc<dyn Resource>`，无悬垂句柄 | `SlotId` 索引 + `RProcTable` 拥有行，无世代 | D2：向 Redox 的句柄模型靠拢（generation 或 `Rc`） |
| 系统调用界面 | `enum Syscall` + `syscall::call`；用户态不抽象内核，错误 `Result<T, Error{errno}>` | 单体 `KernelApi` trait 混 PM/VM/kernel 调用；裸 `i32` errno | T2/T3：按消息目标拆 trait + `Errno` newtype（Redox 共享 ABI crate 同款定位） |
| IPC 类型化 | `Syscall` enum + typed args | `minix-types::ipc::rs` typed enums（A-2 部分落地）+ 消息号仍 i32 常量 | 继续 A-2 路线：dispatch 用 enum 而非 `Request(i32)` |
| panic 纪律 | daemon 用 Result 传播，panic 仅 unrecoverable | 20 处 `unimplemented!()`（boot.rs 13 + sef.rs 7）+ minix-sys `todo!()` 2（均 fail-closed 可达性设计） | T7：接线前清零 + CI 门禁；对照 Redox 用户态无 panic 路径 |
| 并发/锁 | kernel 锁序类型 L0-L5 + `CleanLockToken` 全链 token | 单线程 `&mut self` 线程化，无锁无 unsafe | ✅ 用户态服务器模型正确（AGENTS.md），无需 BKL |
| 全局状态 | scheme 注册表在 kernel 由 mutex 保护，用户态无全局可变 | 状态全在 `RsServer`/`BootInit` 内 `&mut` 传递 | ✅ 无全局可变状态，比 01-stage-kernel 更干净 |
| 配置驱动 | init.rc 声明式配置 | boot 三表静态 + RS_UP 动态 | 可考虑将来把 boot 表做成配置注入（现 `BootTables::new` 已支持注入 image ✅） |

**总体结论**：RS 的 Rust 化在"纯决策 + 注入副作用"方向上与 Redox 的"逻辑与 syscall 分离"一致；
主要差距在**错误类型化**（T3）、**能力边界拆分**（T2）、**句柄安全**（D2）三项 —— 这三项恰是
Rust 相对 C 最能兑现的类型安全红利，建议在 19 接线前完成。

## 8. 未完成 / DEFERRED / 工具链遗留

### 8.1 运行期不可达路径（接线前 fail-closed 正确，接线时必须清零）

| 位置 | 内容 | 依赖 |
|------|------|------|
| `boot.rs:152-184` | `UnimplementedKernelApi` 8 方法 `unimplemented!()` | 19 |
| `boot.rs:88-150` | `KernelApi` 默认 impl：srv_fork/getprocnr/vm_memctl/vm_set_priv | 19 |
| `boot.rs:576-592` | `self_update` `unimplemented!()`（有 `live-update` feature 门控 ✅） | 18 |
| `sef.rs:118-149` | 7 个 SEF placeholder `unimplemented!()` | 06/12/18 |
| `minix-sys/src/lib.rs:27-29` | `receive` `todo!()`（连带 `send`/`sendrec`/其余 syscall stub） | 19 |
| `lib.rs:193` | `get_work` 的 `expect`（依赖 receive） | 06/19 |
| `boot.rs:451-452` | `rinit.rproctab_gid = None`（cpf_grant_direct 未接线） | 17/19 |
| `boot.rs:505-516` | Step 2 RS/VM `init_service` 跳过（注释标注） | 12 |
| `boot.rs:530-538` | Step 3 无接收（见 T6） | 12 |
| `monitor.rs`/`ready.rs`/`recovery.rs` | 决策函数就绪，**无主循环调用方**（副作用注入未接线） | 06/07/15 |

### 8.2 语义待补（见 §4）

S1（build_cmd_dep NUL/截断）、S2（activate_boot_slot 8 字段）、S3/S4（sched 签名）在 09/10/12/19
接线前必须修正，否则接线即引入行为偏差。

### 8.3 工具链遗留（P2）

- `cargo check`：`ready.rs:19` unused import `EINVAL`（1 warning）
- `cargo clippy -p minix-rs --lib`：`ready.rs:19` unused import；`query.rs:63` manual_range_contains；
  `ready.rs:68` `init_message` 8 参数（too_many_arguments）
- `cargo fmt --check`：`privilege.rs:303` `srv_or_usr` if-else 单行化
- `boot.rs:337-347` 重复的 doc comment（`BOOT_IMAGE_PLACEHOLDER` 注释出现两次）

## 9. 修复路线建议

**阶段 1 — 纯代码加固（不依赖任何接线，可立即做）**：
S1（build_cmd_dep NUL/截断 + 测试）→ D3（totality）→ S3/S4（sched NONE 跳过 + 签名）→
D5（Default 零值）→ D4（常量收敛）→ E3（testutil 共享 mock）→ §8.3 工具链清理。

**阶段 2 — 接线前设计决策（06/19 前必须定）**：
T1（状态 handover / typestate）→ T2（trait 拆分或全量实现 + 删默认 impl）→
T3（`Errno` newtype 落 minix-types，A-12 与代码对齐）→ T6（Step 2/3 计数修正 + fail-closed）。

**阶段 3 — 接线时**：
T4（主循环真实状态字）→ T7（CI 门禁：`rg "unimplemented!|todo!"` 清零）→
S2（activate_boot_slot 补全）→ E2（proptest 补解析函数）。

> 修复遵循 fix-guard.md：每条修复前读目标行 ±5 行、grep 确认、单条修复、修后 grep 验证 + 记录。

---

## 10-17. 历史修复索引（2026-08-15/16 六轮 review 的压缩归档，2026-09-07 清理）

> 早期六轮深度 review（§10-§17 原文约 1150 行）的全部发现均已修复并经后续轮次
> 回归确认。逐条 Before/After 细节沉淀在 git 历史（本文件 2026-09-07 之前的版本）
> 与各轮 commit 中；此处保留「发现 ID → 主题 → 状态 → 修复记录」的索引链。
> §18（2026-09-06 起）为现行证据链，保留全文。

| 轮次 | 发现 | 主题（一句话） | 状态 | 修复 |
|------|------|----------------|------|------|
| §10（第一轮修复） | T3 | Errno newtype 落 minix-types + 全 crate 迁移 | ✅ | Fix #1 |
| §10 | S1 | build_cmd_dep 遇 NUL 停止 + 参数不截断 | ✅ | Fix #2 |
| §10 | D3 | 越界计数 fail-closed（caller_can_control/lookup_by_domain） | ✅ | Fix #3 |
| §10 | S3/S4 | sched NONE 跳过 + sched_init_proc 携全参 | ✅ | Fix #4 |
| §10 | D5 | RsStart::default 对齐 C 调用方默认 | ✅ | Fix #5 |
| §10 | D4 | RS_MAX_LABEL_LEN/MAX_BACKOFF 常量收敛 | ✅ | Fix #6 |
| §10 | T2 | KernelApi 删默认 impl + Unimplemented 全 ENOSYS | ✅ | Fix #7 |
| §10 | T6 | Step2/3 计数修正 + SYNCH_BOOT fail-closed | ✅ | Fix #8 |
| §10 | T1 | BootInit→ServerState 状态 handover | ✅ | Fix #9 |
| §10 | T4 | get_work 不伪造 IpcStatus | ✅ | Fix #10 |
| §10 | T7 | 占位符 CI 门禁（check-rs-unwired.sh） | ✅ | Fix #11 |
| §10 | S2 | activate_boot_slot 补全 boot 字段 | ✅ | Fix #12 |
| §10 | 工具链 | clippy/fmt/重复注释清理 | ✅ | Fix #13 |
| §11/§12（R2 轮） | N1+N10 | boot 表 flags 位值全错（P0）→ 单一权威 + 精确位值测试 | ✅ | Fix #14 |
| §11/§12 | N2 | ping 间隔改用原始 r_period（request.c:1035） | ✅ | Fix #15 |
| §11/§12 | N3 | Machine 纳入 ServerState（启动快照） | ✅ | Fix #16 |
| §11/§12 | N4 | minix-sys Errno 统一为 minix-types | ✅ | Fix #17 |
| §11/§12 | N5 | SefCallbacks fn 指针 → trait（携带状态） | ✅ | Fix #18 |
| §11/§12 | N6 | Label 相等语义 strcmp 化 + 尾 NUL | ✅ | Fix #19 |
| §11/§12 | N7 | CallMask/SysMap 移位越界 fail-closed | ✅ | Fix #20 |
| §11/§12 | N8 | RupdateDescriptor 死代码删除（UpdateChain 单点） | ✅ | Fix #21 |
| §11/§12 | N9 | IoRange/NR_SCHED_QUEUES 单一权威 | ✅ | Fix #22 |
| §11/§12 | N11 | build_cmd_dep argv[0] 恒存在 | ✅ | Fix #23 |
| §13（第四轮） | T5 | `&mut dyn KernelApi` 注入统一 monitor 模式 | ✅ | Fix #24 |
| §13 | E1 | 四份 mock 收敛 testutil::MockKernelApi | ✅ | Fix #25 |
| §14/§15（R3 轮） | R1 | effective_period 的 LU 初始化超时（upd_init_maxtime） | ✅ | Fix #26 |
| §14/§15 | R2 | Privilege 计数域 u16→i32 + fail-closed | ✅ | Fix #27 |
| §14/§15 | R4 | do_init_ready pending 下溢 fail-fast | ✅ | Fix #28 |
| §14/§15 | R5 | RProcTable::get 越界断言带上下文 | ✅ | Fix #29 |
| §14/§15 | R6 | mock 四个 unimplemented → Err(ENOSYS) | ✅ | Fix #30 |
| §14/§15 | R11 | label UTF-8 契约（&str + 边界 fail-closed） | ✅ | Fix #31 |
| §14/§15 | R7-R10 | 结构性：R7/R8 归 D1/D2（→Fix #67）、R9→Fix #60、R10→E-7 | ✅ | 归 §18 |
| §16/§17（R4 轮） | R16 | compute_backoff 负 restarts clamp | ✅ | Fix #32 |
| §16/§17 | R12 | endpoint_slot/set_endpoint_index 越界门 | ✅ | Fix #33 |
| §16/§17 | R15 | rebuild_args argc 只计完整 token | ✅ | Fix #34 |
| §16/§17 | R18 | free_slot 内建 SF_USE_COPY→free_exec | ✅ | Fix #35 |
| §16/§17 | R19 | pid.is_some_and(p>0) | ✅ | Fix #36 |
| §16/§17 | R17 | boot 表 proc_nr↔endpoint.slot 校验 + boot_img 钳制 | ✅ | Fix #37 |
| §16/§17 | R13/R14 | SlotMutations 决策变异载荷统一 | ✅ | Fix #38 |
| §16/§17 | R14 附带 | RsStart 补 rss_nr_io/rss_io（rs.h:124） | ✅ | Fix #39 |

---

## 18. 全量查漏补缺 + 架构级审查（2026-09-06 — scan-only：覆盖度穷举 + C 逐函数对照 + 分层架构建议）

> **来源**：cmd-04（stage 架构级审视）+ cmd-20 第 1 步（对照 Minix3 C 实现找遗漏）+ code-excellence
> 分层审视法。**本轮只审查+记录，产品代码零改动**；全部条目状态 ☐，按"可立即 todo-fix /
> 08·13·16 号文档落地期 / 19 接线期"三档标注归属（见 18.6 总览表）。
> **范围**：`os/servers/rs/` 全部 22 模块（10,305 行，211 测试）↔ Ground Truth
> `minix3/minix/servers/rs/`（8 个 .c 共 6,307 行 + const.h/type.h/glo.h/proto.h/inc.h 五个头文件）。
> 方法：覆盖度工具穷举（coverage-extract + semantic-map 全量扩展）+ 四路逐模块深读（生命周期 /
> 监控与 Live Update / 表与权限 / 服务面），每路对照 C 源逐函数判定"已实现 / 部分 / stub / 缺失"。
> **基线证据**：`cargo test -p minix-rs` = **211 passed**（§17 末记录 208——§17 之后代码有小步演进，
> 本轮以 211 为新基线）；`tools/check-rs-unwired.sh` = PASS（3 个生产未接线标记全部带文档契约）；
> `rg -c "#\[test\]"` = 211（Gate E 数量对账：声称 = 实测，偏差 0%）。生产运行路径仍全部
> fail-closed（`UnimplementedKernelApi` 全 ENOSYS、`get_work` 为 `todo!()`），维持
> "无 P0 运行时缺陷"前提——本轮发现的全部缺口集中在"接线前必须补齐的设计缺失"（P1）与
> 改进项（P2），无 P0。
> **与既有条目的关系**：D1/D2/D6/E2/E3/R8/R9/R10 等既有遗留项只引用不重报；本轮 R 系列从
> **R20** 起编（承接 §14 R1-R11、§16 R12-R19）。

### 18.0 gate-evidence-A：覆盖度提取证据

本轮将 `tools/coverage-extract/rs-semantic-map.json` 从 56 条（仅覆盖 01 号文档语义，自注
"其余 doc（02~19）语义随写作按需扩展"）全量扩展到 02~19 号文档的全部语义面（映射依据 = 四路
深读的行号锚点；**完全缺失的 C 函数刻意不映射**，让其如实显示为缺口），重跑覆盖度提取：

```
$ python3 tools/coverage-extract/coverage-extract.py rs \
    notes/rewrite/fork-syscall-rewrite/03-stage-rs \
    --rust-dir os/servers/rs/src --c-dir minix3/minix/servers/rs \
    --semantic-map tools/coverage-extract/rs-semantic-map.json \
    --output .review/claude/rs/scan/SYMBOLS.md
  Found 171 C symbols (112 funcs, 7 structs, 52 macros, 0 enums)
  Found 834 Rust symbols (447 top-level fn, 148 qualified methods)
Coverage Summary for rs:
  Total C symbols: 171
  Doc covered: 163 (95.3%)
  Rust covered (name-match): 115 (67.3%)
```

- Rust 覆盖 115/171 中：语义映射命中 111、限定名匹配 5、名称匹配 1（SYMBOLS.md 统计）。
- **缺口 56 个** = 有文档无 Rust 49 + 完全缺口 7（其中 5 个是 C 头文件 include 守卫
  （`RS_CONST_H`/`RS_GLO_H`/`RS_TYPE_H` 等）与 `DEBUG_DEFAULT`/`PRIV_DEBUG_DEFAULT` 两个纯
  编译期调试开关——ARCH 不需要；真实的完全缺口是 error.c 的 `rs_strerror`（error.c:33）与
  `errentry` 结构（error.c:9），归入 R31）。
- **工具盲区声明**：coverage-extract.py 的函数签名正则只匹配单行签名，exec.c 的
  `do_exec`（exec.c:7/:62）、`read_seg`（exec.c:10/:143）、`srv_execve`（exec.c:21）三个多行
  签名函数未被提取。人工已确认这三者在 Rust 全缺（19 号 DEFERRED，见 R31 归属），不影响
  缺口结论，但符号总数（112 funcs）略低于实际。

### 18.1 查漏结果：有文档无 Rust 的函数清单（按文档域分组）

下表为 SYMBOLS.md"⚠️ 有文档无 Rust"中的全部 39 个函数（剔除头文件机制宏后）。**关键结论**：
缺口不是散点，而是**五个整域缺失**——这正是既有 DEFERRED 标记挂到 06/08/13/16/19 的原因；
本轮的贡献是把每个域的缺失规模从"一个函数名"精确到"函数 + 分支 + 载体字段"。

| 文档域 | 缺失函数（C 锚点） | Rust 现状 | 精确规模 |
|--------|------------------|-----------|---------|
| 08 配置管线 | `edit_slot`（manager.c:1460）、`init_slot`（manager.c:1708）、`copy_rs_start`（manager.c:135）、`copy_label`（manager.c:151）、`inherit_service_defaults`（manager.c:1303） | 全缺；`init_slot`/`edit_slot` 仅注释提及（slot.rs:9、ipc_mask.rs:10-11） | edit_slot 约 25 个校验/拷贝分支零载体（18.2 R20 逐段列出）；RsStart 缺约 10 个输入字段 |
| 13 控制请求 | `do_restart`（request.c:160）、`do_clone`（request.c:208）、`do_unclone`（request.c:253）、`do_edit`（request.c:298）、`run_service`（manager.c:923）、`start_service`（manager.c:950）、`restart_service`（manager.c:1246）、`kill_service_debug`（manager.c:360）、`crash_service_debug`（manager.c:380）、`detach_service_debug`（manager.c:497） | 纯切片部分就绪（`up_init_flags`/`check_duplicates`/`stop_service`/`mark_late_reply`/`shutdown_apply`，request.rs），编排层全缺 | create_service 15 步管线只有 2 步有 Rust（18.2 R22） |
| 16 Live Update | `rupdate_clear_upds`（update.c:7）、`rupdate_set_new_upd_flags`（update.c:88）、`rupdate_upd_clear`（update.c:135）、`rupdate_upd_move`（update.c:164）、`start_update_prepare`（update.c:401）、`start_update_prepare_next`（update.c:467）、`start_update`（update.c:532）、`start_srv_update`（update.c:621）、`end_update_curr`（update.c:744）、`end_update_before_prepare`（update.c:763）、`end_update_prepare_done`（update.c:780）、`end_update_initializing`（update.c:795） | 入口决策（`validate_update_request`/`lu_flags_from_rss`/`UpdateChain::add`）与出口分类（`end_update_role`/`abort_action`）已建；中段编排与链操作全缺 | 相位驱动（flags 写入）无入口；`UpdateChain` 只有 add；per-slot `r_upd` 载体未建（18.2 R23） |
| 06 主循环/信号 | `reply`（utility.c:309）、`rs_asynsend`（utility.c:223）、`rs_idle_period`（utility.c:443）、`fi_service`（utility.c:69）、`sef_local_startup`（main.c:136，结构等价但 SEF_INIT 消息驱动语义缺） | `get_work` 为 `todo!()`（lib.rs:244）；sef 回调 4/7 为 stub（lib.rs:261-293） | 心跳 timestamp 在类型层不可表达（R25）；signal_manager 签名偏差（R26） |
| 诊断面 | `srv_to_string_gen`（utility.c:142）、`srv_upd_to_string`（utility.c:189）、`print_services_status`（utility.c:485）、`print_update_status`（utility.c:516）、`init_strerror`（error.c:48）、`lu_strerror`（error.c:56）、`exec_restart`（exec.c:121） | 全缺（R31） | IS 服务器 dump 与错误名输出依赖此域 |

ARCH 不需要（头文件机制，无 Rust 对应义务）：`BEG_RPROC_ADDR`/`END_RPROC_ADDR`（const.h:54-55，
数组地址哨兵——Rust 迭代器取代）、`DEBUG`/`PRIV_DEBUG`（const.h:10/:14，编译期调试开关）、
`EXTERN`（glo.h:8，全局变量声明机制）、`_SYSTEM`/`_TABLE`（inc.h:7/table.c:7，编译单元选择），
及前述 include 守卫。

### 18.2 新发现（R20–R34）

- **R20（P1-design-missing，08 号域）— `edit_slot`/`init_slot` 整体缺失，且 `RsStart` 载体字段不全**
  - C 证据：`edit_slot`（manager.c:1460-1707）约 25 个分支：IPC 表校验/拷入（:1476-1483）、IRQ
    哨兵与界检查（:1486-1501）、IO 哨兵/界检查/`ior_limit` 拷入（:1504-1524）、k_call_mask 与
    vm_call_mask 的 memcpy+basic 叠加（:1527-1540）、control labels（:1543-1564）、sig_mgr
    （:1567）、调度四字段条件写（:1570-1575）、cmd/progname/label/script（:1578-1626）、
    RSS_COPY 复用/加载（:1629-1661）、REPLICA/NO_BIN_EXP/DETACH/NORESTART 标志（:1662-1682）、
    period/restarts/asr_count（:1685-1700）；`init_slot`（manager.c:1708-1795）：DSRV 默认三元组
    （:1723-1724）、bak_sig_mgr/uid（:1727-1730）、域校验（:1733-1736）、dev_nr/devman_id
    （:1738-1742）、PCI 校验（:1745-1774）、字段复位块（:1777-1791）。
  - Rust 现状：两函数零载体。**载体也不全**——`RsStart`（slot.rs:89-145，21 字段）缺
    `rss_major`、`rss_script`/`rss_scriptlen`、`rss_heap_prealloc_bytes`/`rss_map_prealloc_bytes`
    （live_update.rs:131/:179 只能用裸 i64 参数绕过）、`rss_pci_*`（`PublicSlot` 亦无 `pci_acl`
    字段，publish.rs:30 恒 false stub 与此同源）、`rss_system`/`rss_vm` 两个掩码源、
    `rss_label`/`rss_trg_label`、`rss_nr_domain`/`rss_domain`、入口侧 `devman_id`——缺的恰好全部
    是 edit_slot/init_slot 的输入。`build_cmd_dep`（slot.rs:240）是唯一已实现的 edit_slot 分支。
  - 改进思考：08 号文档落地时按"先补 RsStart 字段（对照 rs.h:104-151 逐项）、再按 C 分支序实现
    校验/拷贝、最后逐分支补测试"推进；IRQ/IO 计数域沿用 Fix #39 的 i32 决策（C `int` 校验前
    语义）；注意 C 的 `> NR_IRQ` 检查不拦负数（manager.c:1492），Rust 应 fail-closed 拒负
    （与 Fix #39 注释一致）。

- **R21（P1，API 形状缺口）— `CallMask::from_calls` 无法表达 C `fill_call_mask` 的 is_init=false 组合语义**
  - C 证据：utility.c:126-135——`is_init=FALSE` 时**不清零**，在既有掩码上逐位叠加；消费点
    edit_slot :1527-1540（先 `memcpy(rpriv->s_k_call_mask, rs_start->rss_system, ...)` 再
    `fill_call_mask(RSS_SYS_BASIC_CALLS, ..., FALSE)` 叠加 basic 位）。
  - Rust 现状：privilege.rs:241-248（已核对）——`is_init` 两个分支都是 `CallMask(0)` 起步，
    注释自认 "C does not zero when `is_init` is false; the caller is expected to pass a
    pre-zeroed mask（05 composes masks）"，但**签名没有传入既有掩码的入口**，组合行为在当前
    API 上不可表达。这是 N7 修复时引入的新形状缺口：08 接线时若照现签名直填，basic 位叠加会
    静默变成清零重填。
  - 改进思考：方案 a——`from_calls` 加参数 `base: CallMask`（is_init=true 时传 empty，语义
    统一为"从 base 出发叠加"）；方案 b——新增 `from_calls_or(base, calls, ...)` 保留原签名。
    推荐 a（一个构造入口，消除"is_init 分支语义分裂"）。补"非零 base 叠加 basic 位"测试。

- **R22（P1-design-missing，13 号域）— 服务生命周期编排层缺失：create_service 15 步只有 2 步有 Rust**
  - C 证据：`create_service`（manager.c:531-708）线性管线：前置检查（:542-568）→ `srv_fork`
    （:576）→ `getprocnr`（:584）→ 表更新（:589-597）→ priv 设置/回读（:600-606）→
    `sched_init_proc`（:609）→ `read_exec`（:625）→ `srv_execve`（:634）→ `free_exec`（:643）
    → setuid hack（:656）→ RS pin（:659-669）→ VM 注册（:672-695）→ `vm_set_priv`（:698），
    **每个失败点都有 `cleanup_service` + VM pin 解除的对称清理**。`start_service`（:950-983，
    init_flags → create → activate → publish → run）、`run_service`（:923-948，ALLOW +
    `init_service`）、`restart_service`（:1246-1298，脚本保存/clone/update/run 编排 + DETACH
    位）、`kill_service`（:360-378）、`crash_service`（:380-403，RS 自杀 `exit(1)` + SIGKILL）、
    `detach_service`（:497-528，label 前缀重发布 + 标志清理）均无编排对应物。
  - Rust 现状：`check_create_preconditions`（service_create.rs:29）+ `mark_child_created`
    （service_create.rs:54）对应第 1、4 步；**`check_create_preconditions` 注释声明 "the
    caller owns the slot cleanup"（service_create.rs:23-24），但 caller 尚不存在，该契约
    无人履行、也无类型强制**。`cleanup_service` 只建了第二相分类 `cleanup_decision`
    （recovery.rs:250，对应 manager.c:451-452），第一阶段（manager.c:416-448：解链四指针、
    RS_DEAD 置位、DISALLOW/CLEAR_IPC_REFS、去 ACTIVE、late_reply）连决策都没有。
  - 改进思考：见 18.3 A1（编排层引入形态三方案对比，推荐忠实编排函数）。

- **R23（P1-design-missing，16 号域）— Live Update 中段编排缺失 + rupdate 全局碎片化**
  - C 证据：链操作四缺——`rupdate_clear_upds`（update.c:7-18）、`rupdate_set_new_upd_flags`
    （update.c:88-116，MULTI 置位 + last 继承 + VM/RS endpoint 自动置位）、`rupdate_upd_clear`
    （update.c:135-159）、`rupdate_upd_move`（update.c:164-180，end_srv_init 的 update-scheduled
    分支依赖它，manager.c:344-346）。相位驱动两处全局写——update.c:510
    `rupdate.flags |= RS_UPDATING`、update.c:548 `|= RS_INITIALIZING`。编排五缺——
    `start_update_prepare`/`start_update_prepare_next`/`start_update`/`start_srv_update` 与
    `end_update` 四个相位执行体（update.c:744-811）。
  - Rust 现状：`UpdateChain` 只有 `add`（live_update.rs:385，偏序插入逐行对应 update.c:23-83）；
    `last_lu_flags`（live_update.rs:339）是给 `rupdate_set_new_upd_flags` 备的原料但零消费。
    **rupdate 全局（type.h:43-52）没有单一 Rust holder**：`RupdateFlags` 是无处挂载的裸
    bitflags（process_table.rs:32）、`num_init_ready_pending` 退化为 `do_init_ready` 入参
    （ready.rs:164）、per-slot `r_upd` 描述符（type.h:62）整体缺席 `ServiceSlot`——
    `SRV_IS_UPD_SCHEDULED`/`SRV_IS_PREPARING_ONLY`（const.h:119-120）只能靠注入 bool 表达。
    `UpdateEntry` 缺 `prepare_tm`/`prepare_maxtime`/`prepare_state_data*`/三个 grant id
    （type.h:35-39）。**结论：16 号 DEFERRED 的确切边界是"载体结构本身未建"，不只是接线缺。**
  - 改进思考：见 18.3 A2（UpdateState 单点持有方案）。

- **R24（P1）— `do_upd_ready` 是 R13 决策-载荷模式唯一的破例：`RS_PREPARE_DONE` 置位无处落地**
  - C 证据：request.c:911——gate 通过后立即 `rp->r_flags |= RS_PREPARE_DONE`（无论 result
    成败）。
  - Rust 现状：`do_upd_ready`（ready.rs:243）只返回裸 `UpdReadyOutcome` 枚举、无 `SlotMutations`
    载荷；该置位在整个 crate 无处安放。同文件的 `do_init_ready`（ready.rs:160）与
    monitor/recovery 的决策都严格执行载荷模式（Fix #38），唯此处破例。
  - 改进思考：`UpdReadyOutcome` 携带 `SlotMutations`（gate 通过即 `set: RS_PREPARE_DONE`），
    补"与 result 无关、gate 通过即置位"的测试。改动小，可立即 todo-fix。

- **R25（P1，类型缺陷）— `HeartbeatNotify` 不携带 timestamp，main.c:87 的心跳语义在类型层不可表达**
  - C 证据：main.c:87 `rp->r_alive_tm = m.m_notify.timestamp`——心跳通知的 timestamp 直接
    写入槽位存活时间戳。
  - Rust 现状：`HeartbeatNotify(Endpoint)`（dispatch.rs:54，已核对）只携带端点；分类类型上
    没有这个数据，06 接线时要么丢语义要么改签名。这不是"没写测试"，是**签名缺陷使测试
    不可写**。
  - 改进思考：改为 `HeartbeatNotify { endpoint: Endpoint, timestamp: <C m_notify.timestamp
    对应类型> }`，`period_decision`/存活时间戳写入路径（Fix #38 已有 `alive_tm` 载荷字段）
    消费之。字段类型对照 ipc.h 的 `m_notify.timestamp` 与 type.h 的 `r_alive_tm` 落定。

- **R26（P1，签名偏差）— `signal_manager` 参数序与命名偏离 C 回调类型**
  - C 证据：sef.h:270 回调类型为 `(endpoint_t target, int signo)`——target 是信号管理器
    目标端点。
  - Rust 现状：`fn signal_manager(&mut self, signo: i32, exec: i32) -> i32`（sef.rs:90，
    已核对）——参数序颠倒且 target 被改名为 exec。06/18 接线时照此签名实现会把两个 i32
    按错位语义使用（编译器无法拦截）。
  - 改进思考：改签名为 `(target: Endpoint, signo: i32)`（与 C 同序同名），顺带把
    main.c:647-704 的六分支语义（spurious 清信号 / terminated→EDEADEPT / stacktrace 透传 /
    termination 分支"先 terminate 再 rs_idle_period"次序 / VM 不投递 / SIGS 异步转发）作为
    06 号文档的实现清单。

- **R27（P1）— monitor 与 LU 的两个交界行为缺失：rollback 心跳重发扫 + end_update RS 自毁短路**
  - C 证据：(a) update.c:349-352——`rollback_service` 的 RS 分支把全表 `RS_ACTIVE` 槽的
    `r_check_tm` 清零，强制下一周期对全部活跃服务重发 ping（rollback 后心跳状态失效的正确
    处置）；(b) update.c:883-887——`end_update` 中 `result != OK && RUPDATE_IS_RS_INIT_DONE()`
    → `exit(1)`，RS 自更新失败时旧实例让位的最后安全网。
  - Rust 现状：(a) 在 monitor.rs/recovery.rs/self_lifecycle.rs 三处均无（07/16 两文档交界处，
    接线时最易两边都漏）；(b) live_update.rs 只建了 role 分类与 reply-flag 调整，无此分支。
    另：`terminate_decision` 的两次 `abort_update_proc` 调用在 C 中是**内嵌**于决策树的
    （manager.c:1099-1102 位于初始化块之后、norestart 计算之前；:1127-1130 位于 EXITING 分支
    内、late_reply 之前），而 recovery.rs:68-71 注释表述为"executed around the decision"
    ——按注释编排会偏离 C 时序（与遗留 R10"调用顺序只存在于注释"同构，可并入该立项）。
  - 改进思考：18.3 A5 路线图中把 (a) 划入 16 号先行项、(b) 划入 16 号 end_update 执行体；
    recovery.rs 注释修正为内嵌时序（一行注释改动，可立即 todo-fix）。

- **R28（P2）— `clone_service` 的两个分支在 Rust 无 DEFERRED 标记（纯漏标，不只是未实现）**
  - C 证据：manager.c:730-734（VM 单 replica 预清理）、manager.c:765-779（RS 备份信号管理器
    `update_sig_mgrs` 配对）。
  - Rust 现状：`clone_slot` + `link_replica`（service_create.rs:118/:148）对应主链路，
    service_create.rs:110-113 只声明了 sys_getpriv 同步一处 defer——上述两分支连 DEFERRED
    注释都没有。其余缺口均有标记，这两处属于"漏"，13 号接线时容易被当作已完整对照。
  - 改进思考：补两条 DEFERRED 注释（挂 13 号），或直接在 13 号文档落地时实现。可立即 todo-fix。

- **R29（P2，OQ-2）— `TrapMask` 宽度分歧：C `SRV_T`=0xFFFF（16 位 short 全 1），Rust=0x3E**
  - C 证据：kernel/priv.h:34 `short s_trap_mask`（16 位）；`SRV_T = (~0)` 落在 short 上是
    0xFFFF，含 bit 6 = `MINIX_KERNINFO` trap（ipcconst.h:13）——C 语义下系统服务允许该 trap。
  - Rust 现状：`TrapMask` 只建模 SEND..SENDNB 4 位（privilege.rs:135-147），`SRV_T =
    from_bits_truncate(!0)` = 0x3E（privilege.rs:151）；测试 privilege.rs:548 用
    `from_bits_truncate(0xFFFF)` **固化了 0x3E**，无"收窄是有意"的声明。`DSRV_T`(~0)/`DSRV_I`(0)
    常量缺失（init_slot :1723-1724 消费，归 R20）。序列化（19 `sys_getpriv`）未落地前是潜伏
    分歧：一旦落地，C 侧读到的 trap mask 与 Rust 侧语义不同。
  - 改进思考：方案 a——保持 4 位建模，补显式设计差异声明 + 对 bit 6 单独决策（OQ-2 上交）；
    方案 b——改 u16 背书忠实 C 全 16 位（含保留位透传）。推荐先 OQ 定方向，再决定是否随
    19 号序列化对齐。不立即修。

- **R30（P2）— `caller_can_control` 丢失 C 的 IN_USE 复核，索引不变式无显式声明**
  - C 证据：manager.c:52-60——`caller_can_control` 扫描 rproc 表时重新验证 `RS_IN_USE`。
  - Rust 现状：access.rs:45 走 `endpoint_slot()` 索引（process_table.rs:154-160 只做范围
    检查）。当前索引写点（activate_boot_slot process_table.rs:416-418、set_endpoint_index
    :168、mark_child_created（service_create.rs:71）、swap_slot 步骤 5（service_create.rs:
    229-239）、free_slot :344-347）都同步维护，问题不可达；但"endpoint 索引项 ⟺ in-use 槽"
    这一不变式没有任何显式声明或断言，未来新增写点漏清旧索引时，C 找不到已释放行而 Rust
    可能命中。
  - 改进思考：`endpoint_slot` 或 `caller_can_control` 补 `debug_assert!(slot.in_use)`，或在
    process_table.rs 模块头显式声明该不变式。可立即 todo-fix。

- **R31（P2）— 错误字符串表（error.c 全文件）与诊断字符串化 4 函数缺失**
  - C 证据：`errentry` 结构（error.c:9）+ `rs_strerror`/`init_strerror`/`lu_strerror`
    （error.c:33/:48/:56，其中 rs_strerror 为完全缺口——无文档无 Rust）；`srv_to_string_gen`
    （utility.c:142）、`srv_upd_to_string`（utility.c:189）、`print_services_status`
    （utility.c:485）、`print_update_status`（utility.c:516）。
  - 改进思考：错误名面由 T3 已立项的 `Errno` Display 承接（不单独移植 error.c 的查表）；四个
    诊断函数属 IS 阶段（08-stage-is 的 dump 依赖）与 14 号（getsysinfo）接线面；`exec_restart`
    属 19 号 exec 系列。全部记录归属，不立即修。

- **R32（P2，一致性杂项 7 小项）**
  1. `Machine.processors_count`/`bsp_id` 字段零读取（boot.rs:42/:44）——`check_request` 改为
     裸参数传入后成死存储（仅 boot.rs:1139 的 PartialEq 断言消费）。
  2. `RinitState.rproctab_gid`（boot.rs:384，恒 None）与 ready.rs:71 的独立 `rproctab_gid`
     两份字段未打通——19 号 grant 接线时会面对"改哪份"的歧义。
  3. `default_prepare_maxtime` 同名双函数：monitor.rs:28（hz→i64）与 live_update.rs:119
     （(maxtime, default)→u32），lib.rs:65 只导出后者；语义相关（同出 const.h:58）但类型不通，
     19 接线时是现成的混用点。建议改名其一。
  4. `init_service` 发送后 `r_map_prealloc_addr/len` 清零（utility.c:59-60）无任何 Rust 函数
     执行（字段在 service_slot.rs 存在；grep 全 crate 无复位点）——12 号接线清单项。
  5. `update_sig_mgrs` 的两处 C 调用语义不同：do_update 的 (new_rp, SELF, new_rp 自身作
     backup) 配对（request.c:760-766）与 clone_service 的重启副本配对（manager.c:771-773）；
     Rust `sig_mgr_updates`（self_lifecycle.rs:169）只覆盖后者。
  6. `lookup_by_flags`（process_table.rs:295，C 唯一调用点 request.c:1013）与
     `period_decision` 的喂参关系（`another_initializing` 参数，monitor.rs:115）只存在于
     C 行号注释（monitor.rs:107-109），未链到 Rust API 名——接线者需知参数来源。
  7. `check_request` 的负优先级合法（request.c:1275-1279 只拒 `>= NR_SCHED_QUEUES`，负值
     放行）无测试锁定；`sched_decision` 用 debug_assert（sched.rs:120-132）而 C 是运行期
     assert（utility.c:371-372）——release 下"系统进程 + scheduler=NONE"静默放行的选择无声明。

- **R33（P2，D1 补充角度）— `ServiceSlot` 派生 `PartialEq`/`Eq`，`exec: Arc<[u8]>` 使相等比较退化为逐字节 ELF 比较**
  - 现状：service_slot.rs:357 `#[derive(Debug, Clone, PartialEq, Eq)]`（已核对）；任何对槽位的
    `==`/`!=` 都会深比较整个 exec 镜像。
  - 改进思考：手动 impl `PartialEq` 只比较身份字段（pub_ + flags + pid + endpoint），或去掉
    derive 改为测试内字段断言。与 D1（god struct 收敛）同轮处理即可，不单独立项修复。

- **R34（P2）— 测试盲区清单（四路深读汇总，按优先级）**
  以下 C 语义分支当前无测试锁定（锚点 = C 分支 + Rust 位置）：
  1. monitor 决策树分支优先级：`backoff > 0` 与 stop 超时同帧时 backoff 赢（request.c:975
     先于 :985）——现有测试只单分支注入。
  2. `has_update_timed_out` 等号边界：C/Rust 均严格 `>`，`now == prepare_tm + maxtime` 不超时
     ——monitor.rs:452-456 只测远超与未到。
  3. `do_init_ready` result≠0 且 updating 时**不得**置 `INIT_DONE`、不得减 pending（载荷负
     断言缺失）。
  4. terminate 三分支组合互斥：INITIALIZING + updating + `SF_NO_BIN_EXP` 时 rollback 压过
     refresh（manager.c:1071 早于 :1078）。
  5. `CLEANUP_SCRIPT` 负例：`has_script=true` 且 `NORESTART=false` 时不得置位（manager.c:1113-1116
     在 norestart 块内）。
  6. `compute_backoff` 移位极值：restarts=62 → `1<<62` 被 `MAX_BACKOFF` 收敛到 30。
  7. `RSS_FORCE_INIT_ST → SEF_INIT_ST` 映射（request.c:619-621）。
  8. `vm_default_prealloc` 负输入（C 有符号 `<= 0` 判定，request.c:591——负值等同未给）。
  9. `validate_update_request` 的合法 prepare-only 放行侧（request.c:679）。
  10. `update_phase` 对 flags 与计数不一致非法态的解码行为（C 可表达该非法态）。
  11. RupdateFlags 位值（process_table.rs:32-38）无逐 bit 头文件对照测试（RFlags/SysFlags/
      PrivFlags 均有）。
  12. 访问控制规则交互："用户进程目标 + updating=true + RS_EDIT → EBUSY（非 EPERM）"
      （manager.c:103-110 的次序语义）。
  13. `lookup_by_label` 的 "ACTIVE 但 in_use=false 仍应找到" 契约（manager.c:1944 不查 IN_USE）
      ——防止将来"顺手"补 IN_USE 过滤静默偏离 C。
  14. do_getsysinfo 两个尺寸门 `len > size` 早退 / `len != size` EINVAL（request.c:1120-1121/
      :1135-1136）——`GetsysinfoTable` 枚举无尺寸概念，19 组装时最易丢。
  15. do_sysctl 的 ESRCH→OK 归一（request.c:1194-1198）。
  16. do_down 的 RS_TERMINATED 双形态（request.c:137-146：终止服务走 unpublish+cleanup+立即
      OK，活服务走 stop+late reply）。
  17. do_shutdown 的 NULL 消息形态（request.c:438-441 内部调用免权限）。
  18. boot 失败传播族：step0 `get_machine`/`get_hz` Err 提前中止、step1 `lookup_image` 未命中
      →ENOSYS、step4 负 pid →Err、setalarm 失败——mock（testutil.rs:42 pids 栈）可注入但未用。
  19. `run()` 在 state None 时 panic（lib.rs:217-219）与二次 `init(Fresh)` panic（lib.rs:252）。
  20. classify 前置 isokendpt 门（R12 契约）无非法 endpoint 测试。
  21. 信号路由映射：SIGCHLD→do_sigchld、SIGTERM→do_shutdown（main.c:634-641）——两层纯逻辑
      各有单测，但 SEF 信号→handler 的分发层零测试。
  22. `signal_manager` 六分支（main.c:655-703）零分支测试——sef.rs:120 只断言 ENOSYS。
  23. `catch_boot_init_ready` 的阻塞接收与三类 panic（main.c:795-807，12 号落地时补）。
  24. `init_privs` 的 IPC_ALL 只应有 NR_SYS_PROCS 个有效位（manager.c:2325-2329 C 逐位循环；
      Rust `SysMap::all()` 恒 u64::MAX）——NR_SYS_PROCS 变化时语义分叉无契约锁定。
  改进思考：第 1-9、11-13 条可在对应模块内直接补（纯决策测试，成本低）；第 14-17 条依赖
  13/14 号接线（先补类型载体再补测试）；第 18-23 条属 E3（集成测试）的具体化——**补 E3 时
  应先补模型（R22/R23/R25/R26）再补测试，否则测试无载体**。

### 18.3 架构级建议（每条 ≥2 方案对比）

- **A1（对应 R22）编排层引入形态** [若采纳需在 08/13/16 文档设计差异节同步标注]
  - 方案 a（推荐）：**忠实编排函数**。在各域模块补 `create_service`/`start_service`/
    `run_service`/`restart_service`/`start_update` 等编排函数：步骤序列对照 C 行号注释组织，
    复用现有纯决策切片，KernelApi 副作用集中在编排层执行，失败即清理对照 C 的对称清理点。
    优点：与 C 对照性最强（19 接线与 review 都能逐行对）、现有 211 测试全部保值、接线期
    风险最低。缺点：编排函数是较长的过程式形态，函数级单测要靠 mock KernelApi。
  - 方案 b：**服务生命周期状态机**（per-service enum 状态 + 事件迁移表）。优点：非法迁移
    编译期不可表达。缺点：C 的编排充满跨服务副作用（VM pin、sigmgr 备份、abort 钩子内嵌
    时序），16 号的多服务批量编排（start_update 链式扫表 + VM 最后两遍）在 per-service
    状态机中表达别扭；建模成本在 19 接线期不可承受。
  - 方案 c：**Effect 解释器**——`SlotMutations` 泛化为 `Effect` 枚举（Send/PrivCtl/Kill/...），
    编排函数返回 effect 序列，唯一解释器执行。优点：与 T5/R13 的 functional-core 一脉相承，
    编排完全可测（断言 effect 序列）。缺点：内核调用 20+ 种，Effect 枚举与 KernelApi 方法
    一一映射的维护成本高；C 的"失败即清理"要在 effect 列表上建模补偿 effect，复杂度失控。
  - **推荐 a，局部借鉴 c**（决策返回已有 SlotMutations，编排层负责顺序与 IPC 副作用）；
    b 作为后续演进方向不在接线期实施。

- **A2（对应 R23）rupdate 全局收敛**
  - 方案 a（推荐）：**`UpdateState` 挂进 `ServerState`**——`UpdateState { flags: RupdateFlags,
    chain: UpdateChain, num_init_ready_pending }`，相位写入口收敛为
    `begin_updating()/begin_initializing()` 两个方法（对照 update.c:510/:548 仅有的两个全局
    写点）。`ServerState` 已是 T1 修复后的运行态 handover 容器（lib.rs:137），顺理成章。
  - 方案 b：把 flags/pending 直接并入 `UpdateChain` 扩名为 UpdateState。差异仅在归属层级
    命名；a 的"chain 是 state 的一部分"语义更清晰。
  - 共同前置：per-slot `r_upd`（type.h:62）补进 `ServiceSlot`（`Option<UpdateEntry 索引>`），
    否则 `SRV_IS_UPD_SCHEDULED`/`SRV_IS_PREPARING_ONLY`（const.h:119-120）在 16 号接线时仍要
    靠注入 bool；`UpdateEntry` 补 `prepare_tm`/`prepare_maxtime`/grant 三字段（type.h:35-39）。

- **A3（对应 R26 + 18 号文档）SEF 回调机制演进**
  - 现状缺口：`SefCallbacks` 静态 trait（sef.rs:68）无法表达 C 的运行期回调重绑——
    `sef_cb_init_lu` 完成后 `sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)`
    （main.c:558）把 restart 回调换成 stateful 版本；`sef_cb_init_response` 的"模拟 RS-to-RS
    init 消息调 `do_init_ready`"（main.c:602）与 `sef_cb_lu_response` 的 EDONTREPLY→EGENERIC
    反向归一（main.c:622-624，normalize_* 已实现未接）都缺消费点。
  - 方案 a（推荐）：保留 trait，`RsServer` 内加 `restart_cb: RestartCb` 枚举
    （Default/Stateful）承载重绑，trait 方法内 match。改动最小，语义与 C 的单点重绑同构。
  - 方案 b：回调表整体 enum 化（`SefCbSet::Fresh/LuStateful` 切换）。更同构但多一层间接，
    18 号落地时若发现重绑点多于一个再升级。
  - 共同前置：R26 签名修正；restart/lu 路径的 `sys_setalarm(RS_DELTA_T)` 重挂（main.c:540）
    纳入 18 号实现清单。

- **A4（对应 R32.5/R34）dispatch→handler 模式统一**
  - 现状三种决策/变异约定并存：recovery 决策载荷模式（R13）、request.rs 直接变异
    （stop_service request.rs:113 / shutdown_apply request.rs:132）、service_create 直接变异
    （mark_child_created/swap_slot）。
  - 建议：**控制请求域（request.rs）迁移到决策载荷模式**（与 monitor/ready/recovery 一致，
    调用方只记一套约定）；**表编排域（service_create）保留直接变异**——表结构不变式由
    RProcTable 原语保证，强行载荷化只增加无意义样板。`dispatch_request` 死表
    （dispatch.rs:106，14 个请求全 ENOSYS，含纯切片已就绪的 RS_UP/RS_DOWN/RS_SHUTDOWN/
    RS_LOOKUP）在 06 接线时改逐请求路由，此前处置见 OQ-4。

- **A5 接线路线图（依赖序 + 先行修复映射）**
  1. **06（主循环）**：先行修 R25（HeartbeatNotify timestamp）、R26（signal_manager 签名）、
     R24（do_upd_ready 载荷）——都是 06 分派路径上的类型/载荷修正，改动小。
  2. **12（init ready 接线）**：normalize_*/should_reply_ready/mark_initializing 已就绪；
     补 sef_cb_init_response 的"模拟 init 消息"决策与 R32.4 的 map_prealloc 清零。
  3. **13（控制请求）**：R22 编排层（start_service/run_service/restart_service/kill/crash/
     detach + cleanup 第一阶段）+ A4 模式统一 + R28 两分支。
  4. **08（配置管线）**：R20（RsStart 补字段 → edit_slot/init_slot 分支序实现）+
     R21（from_calls base 参数）。
  5. **16（Live Update）**：A2（UpdateState）+ R23 中段编排 + R27(a) rollback 心跳耦合 +
     end_update 执行体（含 R27(b) 自毁短路）。
  6. **19（外部接口）**：exec 系列、reply/rs_asynsend IPC 原语、T3 Errno 统一收口、
     R29 TrapMask 序列化决策、R31 诊断面、R32.1-2.3 死存储清理。
  （依赖依据：各 DEFERRED 注释锚点 + C 调用图；01-stage-kernel todo 的 RS 联调 DEFERRED 项
  与 6 同步收敛。）

### 18.4 死代码候选清单（首轮只列不删；2026-09-07 Fix #72 处置）

**第一档：立即可删（2 项）——✅ 均已删（Fix #72，2026-09-07）**
1. ~~`RS_FI_CRASH` 双定义~~ ✅ 已删 query.rs 副本。**处置时发现同型副本还有 5 个**：
   `RS_SYSCTL_SRV_STATUS/UPD_START/UPD_RUN/UPD_STOP/UPD_STATUS`（query.rs 原 1-5 值，与
   minix-types `sysctl` 子模块 com.h:485-489 完全重复，首轮 scan 漏报）——6 个常量一并收敛到
   minix-types 单一权威，`classify_sysctl` 改 `use minix_types::sysctl`；重复测试
   `test_fi_crash_constant` 删除（minix-types 同值断言已有）。消除影响：零（副本零生产调用）。
2. ~~`RProcTable::set_endpoint_mapping`~~ ✅ 已删——被 `set_endpoint_index`（带 R12
   fail-closed 语义）完全取代，全仓唯一出现处即定义。消除影响：零。

**第二档：同一语义双实现点（1 项，OQ-3）——✅ 判定闭合（Fix #72，2026-09-07）**
3. `share_exec` 与内联 clone：**保留 `share_exec` 单一权威，内联点已路由过去**（service_create.rs
   clone_slot 的 USE_COPY 分支，manager.c:1834-1835）。裁决落地时发现路由写法
   `&table.get(src).clone()` 引入整槽深拷贝（~3.5KB/次），Fix #72 改为直接借用
   `table.get(src)`（语义不变——`ServiceSlot::clone` 本就共享 Arc）；新增
   `test_clone_slot_use_copy_shares_exec_image`（Arc::ptr_eq）锁定共享语义。

**第三档：接线期复活——消费状态对账（Fix #72 刷新）+ awaiting-wiring 标注落地**
- ~~零调用根因是 dispatch 全 ENOSYS（A4）~~：`request.rs` 五件套中 `stop_service`/
  `shutdown_apply` 已被 Fix #69/#71 消费；`query.rs`/`service_create` 纯切片随 I3-I6 接线消费。
- **首轮列名现已全部有生产消费的项**（首轮 scan 早于 Fix #53-#71，状态陈旧）：
  `is_preparing_only`/`last_lu_flags`（live_update.rs，R23 轮消费）、`sched_decision`
  （service_create.rs 消费）、`update_ipc_mask`（slot.rs check_request 消费）、
  `lookup_by_flags`（lib.rs do_period 消费）——不再是死代码候选。
- **仍 awaiting（已加 `awaiting-wiring: NN` doc 标注，防误删）**：`on_stop_result`/
  `set_sig_mgrs`（sched.rs → 13 号 do_edit）、`should_reply_ready`/`normalize_init_response`
  （ready.rs → 06/12 号 response shell 收敛轮裁决采用或删除）、`normalize_lu_response`
  （ready.rs → 16 号 lu_response shell）、`validate_image`/`has_shared_exec`（exec.rs →
  13 号 RSS_REUSE / 19 号 read_exec）、`self_lifecycle.rs` 全部导出（→ 18 号，压 edge E9）。
- `KernelApi` 5 个预支方法已随 R9 拆面归位（`getnuid` 已被 Fix #71 的 do_down 消费）；
  `VmRsMemReq` LU 变体定义在 minix-types（16 号接线时消费）。
- **机制**：对照 `tools/check-rs-unwired.sh` 的 doc-contract 门禁，"预期零调用"项在定义处
  doc 注释统一 `awaiting-wiring: NN-rs-xxx.md` 标注（Fix #72 落地 8 处）——显式化"有意等待"，
  防止后续轮次误判为死代码误删，也防止接线者漏认领。

### 18.5 DEFERRED 判定表（3 个 panic 标记 + 21 处 DEFERRED 注释）

| 类别 | 数量 | 判定 |
|------|------|------|
| panic 标记（`todo!`/`unimplemented!`） | 3（lib.rs:244/:287、boot.rs:652） | 全部带文档契约（check-rs-unwired.sh PASS），合理接线期 |
| DEFERRED 注释 | 21（lib.rs 8、boot.rs 5、service_create.rs 3、monitor.rs 2、publish/exec/main.rs 各 1） | 逐条核对均有文档编号锚点，合理接线期 |
| **无标记的漏**（本轮新发现） | 4 | clone_service 两分支（R28）、cleanup_service 第一阶段（R22）、fill_call_mask 组合语义（R21——有注释但注释声称的调用方路径不存在） |

**结论**：DEFERRED 机制本身健康（无虚标）；真正的风险是 08/13/16 号 DEFERRED 的实际规模
远大于标记密度暗示——以 R20/R22/R23 的精确清单为准（edit_slot 约 25 分支、create_service
15 步、update.c 中段 12 函数、RsStart 约 10 字段、UpdateEntry 4 字段组）。

### 18.6 本轮总览表

| ID | 主题 | 严重度 | 状态 | 归属 |
|----|------|--------|------|------|
| R20 | edit_slot/init_slot 整体缺失 + RsStart 载体字段不全 | P1-design-missing | ✅ | 已修（Fix #46/#48/#49，2026-09-06） |
| R21 | from_calls 无法表达 is_init=false 组合语义 | P1 | ✅ | 已修（Fix #44，2026-09-06） |
| R22 | 生命周期编排层缺失（create 15 步只有 2 步 + cleanup 第一相） | P1-design-missing | ✅ | 已修（Fix #51/#52/#56，2026-09-06） |
| R23 | LU 中段编排缺失 + rupdate 全局碎片化 + r_upd 载体未建 | P1-design-missing | ✅ | 已修（Fix #53/#54/#55/#56，2026-09-06） |
| R24 | do_upd_ready 缺载荷，RS_PREPARE_DONE 无处落地 | P1 | ✅ | 已修（Fix #42，2026-09-06） |
| R25 | HeartbeatNotify 缺 timestamp 字段 | P1 | ✅ | 已修（Fix #41，2026-09-06） |
| R26 | signal_manager 签名偏差（sef.h:270 对照） | P1 | ✅ | 已修（Fix #40，2026-09-06） |
| R27 | rollback 心跳重发扫 + end_update 自毁短路 + abort 时序注释错 | P1 | ✅ | 已修（Fix #56，2026-09-06） |
| R28 | clone_service 两分支漏标 DEFERRED | P2 | ✅ | 已修（Fix #50，2026-09-06） |
| R29 | TrapMask 宽度分歧（0x3E vs 0xFFFF）+ DSRV_T/DSRV_I 缺失 | P2/OQ-2 | ✅ | 已修（Fix #47，2026-09-06，OQ-2=全宽） |
| R30 | caller_can_control 丢 IN_USE 复核，索引不变式无声明 | P2 | ✅ | 已修（Fix #43，2026-09-06） |
| R31 | error.c 错误表 + 诊断字符串化 4 函数缺失 | P2 | ☐ | EDGE（见 §18.10） |
| R32 | 一致性杂项 7 小项（死存储/双份字段/同名函数/清零无执行者等） | P2 | ✅ | 已修（Fix #45，2026-09-06） |
| R33 | ServiceSlot 派生 PartialEq 的深比较风险 | P2 | ✅ | 已修（Fix #68，2026-09-07，derive 删除） |
| R34 | 测试盲区清单 24 条 | P2 | ✅ | 全部闭合：1-13（各实现轮）/14-15（Fix #75）/16（Fix #71+#76 集成面）/17（Fix #69）/18-21（Fix #65/#69） /22（Fix #78 signal_manager 七分支）/23（Fix #70）/24（Fix #64） |
| A1 | 编排层引入形态（三方案对比，推荐忠实编排函数） | 建议 | ✅ | 形态 a 已采纳并落地（Fix #51/#52/#55/#56） |
| A2 | UpdateState 挂 ServerState + r_upd 入 ServiceSlot | 建议 | ✅ | 已修（Fix #53，2026-09-06） |
| A3 | SEF 回调重绑建模（restart_cb 枚举） | 建议 | ✅ | 已修（Fix #63，2026-09-06） |
| A4 | 控制请求域统一决策载荷模式 | 建议 | ✅ | stop_decision 已落（Fix #52）；dispatch 逐请求路由已随 campaign I2-I6 完成——13/14/16 号全部活臂经 do_request 直达，死表仅剩 E-RSSTART 门的三臂（UP/EDIT/UPDATE） |
| A5 | 接线路线图（06→12→13→08→16→19 依赖序） | 建议 | ✅ | 1-4/6 步已按此执行 |

**OQ 清单（上交用户）**
- OQ-1：self_lifecycle.rs 13 个导出保持"纯切片等 18 号接线"形态，还是先内联进 recovery/
  live_update？（推荐保持 + awaiting-wiring 标注）
- OQ-2：TrapMask 收窄到 4 位是否有意？`MINIX_KERNINFO`（bit 6）trap 允许面要不要对齐 C？
  （R29 方向决策）
- OQ-3：`share_exec` 与内联 clone 留一删一，留哪个？（18.4 第二档）→ ✅ 判定闭合（Fix #72，
  2026-09-07）：保留 `share_exec` 单一权威，clone_slot USE_COPY 分支路由过去（直接借用，无整槽
  拷贝）；Arc::ptr_eq 测试锁定。
- OQ-4：`dispatch_request` ENOSYS 死表在 06 接线前删除还是保留占位？（推荐保留 + awaiting
  标注，06 时原地转真）

### 18.7 Redox / OS 理论 / Rust 社区对照（本轮增量）

- 服务监管模型：现有 todo.md §7（2026-08-15 调研）、§12 N12、§14.2 已核实并对照过 Redox 的
  init 声明式服务管理与 `redox_syscall` 边界，本轮结论一致：RS 的 terminate→backoff→
  reincarnate 决策树（recovery.rs）与 Redox 的 daemon 崩溃重启语义方向一致；Redox 无
  live update 机制（§12 N12 已核实）。
- 本轮新增角度：RS 的 LU 四阶段（prepare→update→init→end/rollback）+ 状态数据迁移
  （state_data.rs 的 eval 表与 IPC filter 迁移）在"进程级热更新"维度上比业界常见方案
  （Erlang/OTP 的 supervisor 重启策略 + code change 回调、systemd 的 ExecReload）更完整
  ——这是本 crate 最具教学价值的部分，16/17 号文档落地时应把这一对照写进设计差异节。
  [业界对照为通识性论断；本轮外网不可达，新增 Redox 锚点一律标 待验证，不影响上述条目
  成立——各条目的 C 源锚点已独立充分。]
- Rust 社区实践：A1 的三方案对比本质是 functional-core/imperative-shell（T5 已定方向）在
  "编排层"的延伸——决策可测性与编排忠实性的平衡；A2 是"单一状态容器"惯例（对照
  02-stage-vm/todo.md 的 VmContext 收敛教训 P1-2，同款问题第三处出现：rs 的 ServerState
  应一次到位）。

### 18.8 验证命令块与 Gate 结果

```
$ cargo test -p minix-rs                     # 211 passed; 0 failed（基线，本轮零代码改动）
$ bash tools/check-rs-unwired.sh             # Result: PASS（3 个生产标记全带 doc contract）
$ rg -c "#\[test\]" os/servers/rs/src        # 合计 211（Gate E 数量对账：声称=实测，偏差 0%）
$ python3 tools/coverage-extract/coverage-extract.py rs \
    notes/rewrite/fork-syscall-rewrite/03-stage-rs --rust-dir os/servers/rs/src \
    --c-dir minix3/minix/servers/rs --semantic-map tools/coverage-extract/rs-semantic-map.json \
    --output .review/claude/rs/scan/SYMBOLS.md
  # 171 C 符号 / 文档 95.3% / Rust 67.3% / 缺口 56（含 11 个 ARCH-不需要的机制宏）
```

- Gate E（测试名对账）：本轮 18.2/18.4 中的测试引用全部使用 file:line 锚点而非测试名（避免
  行号漂移与幽灵引用）；数量对账 211 = 211。
- 锚点纪律：R20-R34 的每个"C 证据/Rust 现状"均带 file:line；其中 privilege.rs:241-248、
  dispatch.rs:54、sef.rs:90、service_slot.rs:357、process_table.rs:225 五处关键锚点已逐一
  打开源文件复核，其余锚点来自四路深读的带锚报告（抽查一致）。
- **遗留（本轮不修）**：R20-R34 全部 ☐；既有 D1/D2/D6/E2/E3/T3 残留/R8/R9/R10 维持原归属；
  semantic-map 的 `_note` 已更新为全量扩展说明（工具链配置改动，非产品代码）。

### 18.9 实现记录（Fix #40 起——§18 条目按 todo-fix 迭代队列逐项落地，2026-09-06 起）

> 迭代协议：每轮一项（讲明白 → ≥2 方案对照 Linux/Redox/OS 理论/Rust 社区 → 测试先行实施 →
> 文档同步 → cargo test/clippy/fmt + T7 门禁 → 回归 review → 本节标注 → commit）。
> 范围边界：实现到 `KernelApi` trait 边界（mock 全真、生产 ENOSYS fail-closed）；真实内核
> 传输留给 19 号主线。

### ✅ Fix #40 — R26（P1 签名偏差）：`signal_manager` 对齐 sef.h:270 回调类型
- **File**：`os/servers/rs/src/sef.rs`（trait 声明 + 测试）、`os/servers/rs/src/lib.rs`
  （`impl SefCallbacks for RsServer`）、`01-rs-boot-init.md` §3.3（trait 清单）
- **Before**：`fn signal_manager(&mut self, signo: i32, exec: i32) -> i32`——参数序与 C 回调
  类型 `int(*)(endpoint_t target, int signo)`（sef.h:270）颠倒，且 `target` 被改名为 `exec`；
  两个裸 `i32` 在调用点可互换（编译器无法拦截错位）；返回裸 `i32` 是 trait 七方法中唯一的
  非 `Result` 例外，fail-closed 靠约定值（`ENOSYS.to_i32()`）而非类型。
- **After**：`fn signal_manager(&mut self, target: Endpoint, signo: i32) -> Result<i32, Errno>`。
  方案对比：a) Endpoint newtype + Result（选定）vs b) 保留 i32 只换序（两个 i32 仍可互换，
  地雷只剩命名约定）vs c) 结构体打包参数（双参回调无收益，与 C 对照变差）。选 a 的理由：
  与 sef.h:270 同序同名；`Endpoint` 与 crate 内全部端点流（`dispatch::classify`、
  `process_table::endpoint_slot`）类型一致，错位在编译期不可表达；`Result` 与其余 6 个回调
  统一，fail-closed 从约定升级为类型强制。外部行为不变（回调未接线），无需 [ARCH] 标注。
- **Verified**：`cargo test -p minix-rs` = 211 passed（含 `test_deferred_callbacks_fail_closed`
  更新为 `s.signal_manager(Endpoint::RS, 1) == Err(Errno::ENOSYS)`）；`cargo clippy -p minix-rs
  --lib` 触碰文件零告警；`cargo fmt --check` 触碰文件零 diff；`tools/check-rs-unwired.sh`
  PASS（标记集不变：boot.rs:652、lib.rs:244/:287）。文档同步：01-rs-boot-init.md §3.3 trait
  清单 + fail-closed 表述（6 个未落地 = 5 个 `Err(ENOSYS)` + signal_handler `unimplemented!`）。

### ✅ Fix #41 — R25（P1 类型缺陷）：`HeartbeatNotify` 携带 timestamp，心跳语义类型化
- **File**：`os/servers/rs/src/dispatch.rs`（variant + classify + 测试）、`monitor.rs`
  （新增 `heartbeat_mutations` + 测试）、`lib.rs`（run() 调用点）、文档 06 §3/§5、07 §3.1/§5、
  01 §4.4/§5
- **Before**：`HeartbeatNotify(Endpoint)` 只携带端点——main.c:87
  `rproc_ptr[who_p]->r_alive_tm = m.m_notify.timestamp` 的心跳语义在类型层不可表达（"没写
  测试"实为"签名缺陷使测试不可写"）；`classify` 签名也没有 timestamp 入参。
- **After**：`HeartbeatNotify { source: Endpoint, timestamp: Clock }` 结构体变体；
  `classify` 增设第 4 参 `timestamp: Clock`（ipc.h:1715 的 notify 载荷，非 notify 类忽略）；
  monitor 新增 `heartbeat_mutations(timestamp) -> SlotMutations`（只带 `alive_tm` 的载荷，
  R13 模式）承接写侧。方案对比：a) 命名结构体变体 + Clock 消费端同型（选定）vs b) 二元组
  `HeartbeatNotify(Endpoint, Clock)`（位置参数可读性差）vs c) 变体携带整个 `Message`（分类器
  耦合 wire 布局，破坏既有"提取字段"风格）。关键设计约束：`Message` 载荷在 `MessageUnion`
  联合体中，读取需 `unsafe`——本 crate 零 unsafe，真实提取归 19 号安全 receive 包装器；
  run() 调用点传占位 0 并注释（循环在 `get_work` 的 fail-closed 处不可达，T4 语义不受影响）。
- **Verified**：`cargo test -p minix-rs` = **213 passed**（新增
  `test_classify_heartbeat_carries_timestamp`：timestamp 777 穿过分类原样携带；
  `test_heartbeat_mutations_refresh_alive_tm`：apply 后 `alive_tm==777` 且 flags/check_tm/
  stop_tm 不动）；`cargo clippy -p minix-rs --lib` 触碰文件零告警；`cargo fmt --check` 触碰
  文件零 diff；`tools/check-rs-unwired.sh` PASS。文档同步：06 §3 枚举/classify 签名 +
  设计差异"心跳分类结果自包含"条 + §5 表 7→8 项；07 §3.1 函数清单 + 设计差异"心跳写活标
  是独立决策"条 + §5 表加行；01 §4.4 枚举/classify 签名 + §5 表加行。

### ✅ Fix #43 — R30（P2 契约弱化）：`caller_can_control` 补 in-use 复核，索引语义显式化
- **File**：`os/servers/rs/src/access.rs`（`caller_can_control` 复核 + 测试）、
  `process_table.rs`（`endpoint_slot` 文档化不变式 + 测试）、文档 02 §3.5/04 §2.2/§5
- **Before**：`caller_can_control` 走 `endpoint_slot()` 索引后直接使用（manager.c:52-53 的
  `RS_IN_USE` 逐行复核丢失），"endpoint 索引项 ⟺ in-use 槽"不变式无任何显式声明。
- **After**：方案修正过程值得记录——最初把 in-use 校验加在 `endpoint_slot` 本身（方案 a），
  被 3 个失败测试证伪：`swap_slot` 对重组中/vacant 行合法使用原始索引，全局过滤破坏
  swap 流。最终形态忠实映射 C 的**两种构造**：`endpoint_slot` 保持裸 `rproc_ptr` 语义
  （不过滤，文档声明不变式 + `test_endpoint_slot_is_raw_index_mid_restructure` 锁定
  "清除的条目 None、陈旧条目仍可见"），`caller_can_control` 在索引命中后显式复核
  `RFlags::IN_USE`（access.rs，fail-closed，等价 C 扫描跳过非 in-use 行）+
  `test_caller_can_control_skips_non_in_use_caller_row`。
- **Verified**：`cargo test -p minix-rs` = **216 passed**；clippy/fmt 触碰文件零输出；
  `tools/check-rs-unwired.sh` PASS。文档同步：02 §3.5 加"原始索引语义（R30）"条、
  04 §2.2 语义要点加第 2 条 + §5 表 5→6 项。
- **设计注**：这一轮的失败-修正过程正是 translate 防线的实例——把 C 扫描循环的局部语义
  提升到共享索引层，是"看起来更安全"的 translate 式全局化；C 的安全来自**每消费者各自
  过滤**，Rust 的正确形态是映射同一结构，而不是发明一个 C 不存在的"智能索引"。

### ✅ Fix #42 — R24（P1 模式破例）：`do_upd_ready` 补 SlotMutations 载荷，`RS_PREPARE_DONE` 落地
- **File**：`os/servers/rs/src/ready.rs`（新增 `UpdReadyDecision` + `do_upd_ready` 返回载荷
  + 测试）、`lib.rs`（re-export）、文档 12 §2.4/§3.1/§5
- **Before**：`do_upd_ready` 返回裸 `UpdReadyOutcome`、无载荷——request.c:911 的
  `rp->r_flags |= RS_PREPARE_DONE`（gate 之后、result 检查之前，与 result 无关）在整个 crate
  无处安放；R13 决策-载荷模式（period/do_init_ready/terminate 三处严格执行，Fix #38）的
  唯一破例。
- **After**：新增 `UpdReadyDecision { outcome, mutations }`（与 `ReadyDecision` 同形）；
  门通过的三个分支（PrepareFailed/NextPrepare/StartUpdate）统一携带
  `mutations.set = RFlags::PREPARE_DONE`，门失败分支载荷为默认空。方案对比：a) 决策包装
  结构体（选定，R13 既有形态）vs b) 每个 outcome 变体挂 mutations 字段（Unexpected 携带空
  载荷嘈杂）vs c) 返回元组（位置语义，与既有模式不一致）。
- **Verified**：`cargo test -p minix-rs` = **214 passed**（`test_do_upd_ready` 改断言
  `.outcome`；新增 `test_do_upd_ready_sets_prepare_done_after_gate`：门通过两分支
  `mutations.set` 含 `PREPARE_DONE`（含 result≠9 的失败分支），门失败分支载荷等于
  `SlotMutations::default()`）；clippy/fmt 触碰文件零输出；`tools/check-rs-unwired.sh` PASS。
  文档同步：12 §2.4 加 Rust 映射段（置位位置语义）、§3.1 表行更新、§5 清单 15→16 项 +
  全局 208→214。

### ✅ Fix #44 — R21（P1 API 形状缺口）：`from_calls` 改 base 参数，is_init 组合语义可表达
- **File**：`os/servers/rs/src/privilege.rs`（签名 + boot_priv 调用点 + 测试）、
  文档 03 §3.5/§4/§5.3
- **Before**：`from_calls(calls, tot_nr_calls, call_base, is_init)` 的 is_init=FALSE 分支
  返回 `CallMask(0)` 起步，注释自认"调用方应传预置掩码"——参数表没有传既有掩码的入口，
  C 的 fill_call_mask is_init=FALSE 组合语义（utility.c:133-140，edit_slot 的
  RSS_SYS_BASIC_CALLS 叠加路径，manager.c:1527-1540）在 API 上不可表达；08 接线照现签名
  直填会把 basic 位叠加静默变成清零重填。
- **After**：`from_calls(base: CallMask, calls, tot_nr_calls, call_base)`——删除 is_init
  布尔，结果 = base ∪ bits(calls)。方案对比：a) base 参数替代布尔（选定）vs b) 保留
  is_init 再加 base（两个旋钮一个死的）vs c) 另加 `from_calls_or`（D4 式双 API）。语义
  映射：`empty()` ≡ C is_init=TRUE；传活掩码 ≡ is_init=FALSE；ALL_C 分支忽略 base——
  C 的 ALL_C 分支无条件覆写 chunk 为 `~0`（utility.c:122-129），与 is_init 无关，故
  base 无关正是忠实翻译。`set_bit` 的 N7 EINVAL 门保持不变（base 上的越界调用号同样拒绝）。
- **Verified**：`cargo test -p minix-rs` = **217 passed**（`test_call_mask_from_calls` 全部
  调用点迁移到新签名；新增 `test_call_mask_from_calls_composes_onto_base`：base 位保留 +
  新位加入 + `empty()` 等价 is_init=TRUE + ALL_C 覆写 base）；clippy/fmt 零输出
  （cargo fmt 顺带收敛了同 impl 块内本就存在的注释对齐遗留）；`tools/check-rs-unwired.sh`
  PASS。文档同步：03 §3.5 签名/语义清单 + R21 修复注、§4 boot_priv 代码块、§4 vm_call_mask
  调用签名、§5.3 测试要点（删除"is_init=false 语义 N/A"过时表述）。

### ✅ Fix #45 — R32（P2 一致性杂项）：7 小项逐项落地
- **File**：`slot.rs`/`sched.rs`/`ready.rs`/`self_lifecycle.rs`/`live_update.rs`/`monitor.rs`/
  `boot.rs`/`lib.rs`；文档 08/03/16/12/07
- **逐项实现**：
  1. **Machine 死存储**：`check_request(rs_start, bsp_id, processors_count)` →
     `check_request(rs_start, machine: &Machine)`——C 直接读全局 `machine`（request.c:1289/
     :1296），传快照使字段活跃且两个整数不可换位（C 是同一构造 `struct machine`，非两个裸值）。
  2. **rproctab_gid 两份**：确认为"全局 vs 消息载荷"两个角色（非重复），在 `RinitState` 与
     `InitMessage.rproctab_gid` 文档互链锁定"12 接线是唯一桥"。
  3. **同名双函数**：live_update 的 `default_prepare_maxtime(maxtime, default)` 改名
     `resolve_prepare_maxtime`（0 → 默认的决策），monitor 的（常量计算）保留原名——两种
     C 构造一个名字是接线陷阱。
  4. **map_prealloc 清零无执行者**：新增 `take_map_prealloc(slot) -> (u64, usize)`
     （ready.rs）——take 语义使 copy-then-clear 不可跳过（utility.c:58-60，发送前清零，
     单次移交）。
  5. **sig_mgrs 第二调用点**：新增 `self_update_sig_mgr_update(new_endpoint)`
     （self_lifecycle.rs，request.c:755-766——自更新新实例 sig_mgr=SELF、备份=自身端点），
     与 restart-replica 配对（manager.c:771-773）分立。
  6. **lookup_by_flags 喂参链接**：`period_decision` 文档标注
     `RProcTable::lookup_by_flags(RFlags::INITIALIZING)` 为 `another_initializing` 来源，
     02/07 文档互链。
  7. **负优先级 + release 行为**：`test_check_request_negative_priority_accepted` 锁定
     负值合法（request.c:1275-1279 只拒 `>= NR_SCHED_QUEUES`）；`sched_decision` 的
     `debug_assert!` 升级为全构建 `assert!`（C 是运行期 assert，utility.c:369-370，
     release 静默放行是对 C 的偏离）+ 两个 `#[should_panic]` 测试。
- **Verified**：`cargo test -p minix-rs` = **222 passed**（+5）；clippy/fmt 零输出；
  `tools/check-rs-unwired.sh` PASS。文档同步：08 §2/§4（check_request 签名两处）、03 §5.5
  （assert 语义 + panic 测试）、16（改名 4 处）、12 §3.1/§5（take_map_prealloc）、07 §3.1
  （喂参来源互链）。

### ✅ Fix #46 — R20a（P1-design-missing 前半）：`RsStart` 补全 rs.h:104-151 全部字段
- **File**：`os/servers/rs/src/slot.rs`（RsStart + RsPciId/RsPciClass/RsStateData + 常量 +
  Default + 测试）、文档 08 §3.1/§4
- **Before**：`RsStart` 21 字段，缺 `rss_major`/`rss_script`+len/`rss_heap_prealloc_bytes`/
  `rss_map_prealloc_bytes`/`rss_system`/`rss_vm`/`rss_label`/`rss_trg_label`/
  `rss_pci_*`/`rss_state_data`/`devman_id`/`rss_nr_domain`+`rss_domain`——缺的恰好全部是
  `edit_slot`/`init_slot` 的输入（live_update 只能用裸 i64 参数绕过 prealloc 字段）。
- **After**：全字段补齐 + 三个配套类型（`RsPciId` rs.h:73-78、`RsPciClass` rs.h:82-85、
  `RsStateData` rs.h:93-101——C 指针字段建模为"地址 + 长度 + Option<grant>"，缓冲字节经
  grant 传输）+ 常量 `RS_NR_PCI_DEVICE=32`/`RS_NR_PCI_CLASS=4`/`NO_SUB_VID`/`NO_SUB_DID`。
  `rss_system`/`rss_vm` 直接用 64 位 `CallMask`（与 `s_k_call_mask` 同型，N9 方向；叠加语义
  由 R21 的 base 参数承接）。Default 全部对齐 C 调用方 memset（parse.c:1160）。
- **Verified**：`cargo test -p minix-rs` = **224 passed**（新增
  `test_rs_start_r20a_field_defaults`：新字段默认值 = memset 语义）；clippy/fmt 零输出；
  T7 PASS。文档同步：08 §3.1 模型块全字段化 + 4 条新设计差异 + §4 模块结构更新。

### ✅ Fix #47 — R29（P2 潜伏分歧）：`TrapMask` 全宽 u16 忠实 C（OQ-2 裁决）
- **File**：`os/servers/rs/src/privilege.rs`（TrapMask + DSRV_I + 测试）、`lib.rs`（导出）
- **Before**：TrapMask 只建模 SEND..SENDNB 4 位，`SRV_T = from_bits_truncate(!0)` = 0x3E，
  测试固化了截断值——C 是 16 位 `short`，`SRV_T=DSRV_T=~0` = 0xFFFF（含 MINIX_KERNINFO
  bit 6 与保留位 7-15），19 号 `sys_getpriv` 序列化时两侧语义不同；`DSRV_T`/`DSRV_I`
  常量缺失（init_slot manager.c:1723-1724 消费）。注意：03 文档 §3/§5.1 原本就声称
  "SRV_T=0xFFFF"——代码截断是对已收敛文档的偏离，本修复消除该 doc-code mismatch。
- **After**：**OQ-2 用户裁决 = u16 全宽忠实 C**。`MINIX_KERNINFO = 1<<6`（ipcconst.h:12）
  显式建模；`SRV_T`/`DSRV_T = from_bits_retain(0xFFFF)`（保留位 7-15 如实存在）；
  新增 `DSRV_I: u32 = 0`（priv.h:55，init-flags 族——非 trap 族，manager.c:1723 消费）。
  方案对比：a) u16 全宽（选定，用户裁决）vs b) 窄建模 + 声明差异（19 号序列化需映射层，
  分歧风险留给接线期）。
- **Verified**：`cargo test -p minix-rs` = **224 passed**（`test_trap_mask` 重写：
  SRV_T/DSRV_T bits==0xFFFF、含 MINIX_KERNINFO/SENDNB、USR_T=0x8、CSK_T=RECEIVE；新增
  `test_dsrv_defaults`）；clippy/fmt 零输出；T7 PASS。文档同步：03 §5.1 表述与代码一致
  （无需改动，代码侧对齐）。

### ✅ Fix #48 — R20b（P1-design-missing 主体）：`edit_slot` 全分支实现
- **File**：`os/servers/rs/src/slot.rs`（`edit_slot` + 10 个测试）、`process_table.rs`
  （`iter_all()`）、`privilege.rs`（irqs u32→i32）、`lib.rs`（导出）、
  `libs/minix-types`（SYS_* 8 个调用号 + SYS_BASIC_CALLS/VM_BASIC_CALLS 清单）、文档 08
  §3.1/§3.4/§4/§5
- **Before**：`edit_slot`（manager.c:1460-1707）零载体——约 25 个分支中仅 `build_cmd_dep`
  有实现；`init_slot`/`edit_slot` 的输入字段（R20a 已补）无处消费。
- **After**：按 C 分支序全量实现，关键形态决策：
  - **C 的 `sys_datacopy` 步骤纯化**——C 的 `rss_cmd`/`rss_ipc`/`rss_script` 是指向请求方
    地址空间的指针、收消息时拷入；Rust 的 `RsStart` 本就是拷贝后内存结构，字段落槽是切片
    拷贝（方案 a：纯函数 + 单一注入缝 vs 方案 b：plan/apply 两段拆分——SF_USE_COPY 置位
    依赖 read_exec 结果、纯决策无法预计算且撕裂错误语义 vs 方案 c：全量 KernelApi 注入——
    过度）。唯一外部效果 `read_exec`（文件 I/O，19 号）以
    `&mut dyn FnMut(&mut ServiceSlot) -> Result<(), Errno>` 注入；失败传播发生在
    `SF_USE_COPY` 置位**之前**（与 C 的 `if (s != OK) return s` 同序）。
  - **RSS_REUSE 供体扫描保留 C 的残留数据行为**：manager.c:1636-1651 全表扫描、不过滤
    `RS_IN_USE`（释放行残留 `proc_name`/`sys_flags`）——新增 `RProcTable::iter_all()`
    （与 `iter_in_use` 分立，附 C 锚点）；供体命中走纯 `share_exec`（Arc 克隆）。
  - **basic 位叠加 = R21 base 参数的首次实战**：
    `from_calls(既有掩码, SYS_BASIC_CALLS, NR_SYS_CALLS, KERNEL_CALL)`；
    `SYS_BASIC_CALLS`（com.h:275-278，11 个调用号）/`VM_BASIC_CALLS`（com.h:778-780，7 个）
    落位 minix-types 单一权威（补 SYS_SETALARM/TIMES/SAFECOPYFROM/SAFECOPYTO/VSAFECOPY/
    SETGRANT/EXIT/STATECTL/SAFEMEMSET 8 个常量）。
  - **IRQ/IO 哨兵**：`RSS_IRQ_ALL`/`RSS_IO_ALL` → 计数清零且不置 CHECK_IRQ/CHECK_IO_PORT
    （manager.c:1489-1491/1506-1508）；显式表置位并双表（slot+priv）拷入；`Privilege.irqs`
    u32→i32（C `s_irq_tab` 是 int）。
  - **守卫组**：scheduler=NONE 不动四字段；RS 自身不改 period；restarts=0、asr_count<0
    不覆盖；label 仅在为空时写（回退 proc_name 或取自定义）；NORESTART+核心服务 EPERM；
    DETACH 置/清 DET_RESTART 双向。
- **Verified**：`cargo test -p minix-rs` = **234 passed**（新增 10 个 edit_slot 测试：
  IPC 门/IRQ 哨兵+越界/IO/掩码叠加/命令与 label 回退/脚本三规则/COPY-REUSE 注入与失败
  传播/NORESTART EPERM/三个守卫/调度守卫+sig_mgr）。测试自查抓出 4 处测试代码自身错误
  （切片长度、哨兵值误用为越界值、label 只置一次语义误判）——Gate E/测试自查记录。
  clippy/fmt 零输出；T7 PASS。文档同步：08 §3.1 模型块、§3.4 重写（DEFERRED→已实现 +
  注入缝形态）、§4 模块结构、§5 表 14→26 项。

### ✅ Fix #49 — R20c（P1-design-missing 收尾）：`init_slot` + `inherit_service_defaults` 实现
- **File**：`os/servers/rs/src/service_create.rs`（init_slot + inherit_service_defaults + 3 测试）、
  `service_slot.rs`（`RsPci` ACL 载体 + PublicSlot.pci_acl + PCI 常量迁入）、`slot.rs`
  （RsPciId/RsPciClass 迁出，消除重复）、`lib.rs`；文档 08 §3.4
- **Before**：`init_slot`（manager.c:1708-1795）整体缺失——DSRV 默认覆盖、域/PCI 校验门、
  per-lifetime 复位（init_err=ERESTART、四链清空、scheduler/sig_mgr=-1 瞬态）无处落地；
  `inherit_service_defaults`（manager.c:1303-1330，IMM_SF/IMM_F 合并）缺失；`PublicSlot`
  无 `pci_acl` 载体（publish.rs 恒 false stub 的根因之一）。
- **After**：`init_slot(slot, rs_start, table, read_exec)`——DSRV 五项默认 → uid → 域门 →
  dev_nr/domains/devman_id → PCI 双门+ACL 拷入 → 复位块 → 委托 `edit_slot`。**两个忠实性
  细节**：(1) C 的 scheduler/sig_mgr 复位值是字面量 `-1`（manager.c:1786-1787），**不是**
  `NONE` 端点（endpoint.h:55 的 NONE = SLOT_TOP-2）——瞬态值紧接的 edit_slot 视为可调度，
  保持原样并注释；(2) `sys_flags = DSRV_SF` 是整体覆写（清掉 CORE_SRV 等残留），非 OR。
  `inherit_service_defaults(def, slot)`：设备/域/PCI 整体拷贝、flags 只合并 IMM_SF/IMM_F 位
  （clear+or）、trap_mask 整体拷贝。**配套**：`RsPci`（rs.h:154-163）载体落位
  service_slot.rs，`RsPciId`/`RsPciClass`/PCI 常量随之迁入（消除与 slot.rs 的临时重复）。
- **Verified**：`cargo test -p minix-rs` = **237 passed**（新增
  `test_init_slot_dsrv_defaults_resets_and_delegates`——DSRV 覆盖/复位/委托链全断言、
  `test_init_slot_domain_pci_gates`——四道 EINVAL 门、
  `test_inherit_service_defaults_immutable_only`——不可变位继承 + 自有位保留 +
  非不可变位不继承）；clippy/fmt 零输出；T7 PASS。文档同步：08 §3.4 收尾。

### ✅ Fix #50 — R28（P2 漏标缺口）：clone_service 两分支决策原语落地
- **File**：`os/servers/rs/src/service_create.rs`（`vm_replica_preclean_needed` +
  `unlink_replica` + 2 测试）、`lib.rs`（导出）
- **Before**：clone_service 的 VM 单 replica 预清理（manager.c:730-737）与失败回滚解链
  （manager.c:759-763/:779-780）在 Rust 既无实现也无 DEFERRED 标注——13 号接线时会被当作
  已完整对照。
- **After**：两个纯决策原语 + 与既有切片的衔接说明。`vm_replica_preclean_needed(endpoint,
  instance_flag, has_next)` = 预清理门（VM + LU 实例 + 已有 next 副本；清理执行体归
  R22a 的 cleanup executor）；`unlink_replica(table, rp, instance_flag)` = 失败回滚解链
  （instance_flag 选 new_rp/next_rp 链方向，与 link_replica 同一规则；副本回链按 C 留给
  cleanup）。既有 `is_rs_restart_replica`（:766-767 门）+ `sig_mgr_updates`（:771-773 配对）
  已覆盖备份信号管理器分支的决策面——轮 11 的 create/backup 编排直接消费这三件。
- **Verified**：`cargo test -p minix-rs` = **239 passed**（+2：门真值表 + 双向解链含副本
  回链保留断言）；clippy/fmt 零输出；T7 PASS。

### ✅ Fix #51 — R22a（P1-design-missing 主体）：`create_service` 编排 + `cleanup_service` 两相执行体
- **File**：`os/servers/rs/src/service_create.rs`（create_service 编排 + 测试）、
  `recovery.rs`（cleanup_service 两相执行体）、`boot.rs`（KernelApi 新增 5 方法）、
  `testutil.rs`（mock 可配置 + 按端点 priv 回显）、`lib.rs`；文档 10 §3.1/§3.2、15 §3.1
- **Before**：`create_service`（manager.c:531-708）15 步管线只有第 1、4 步有 Rust；
  `cleanup_service`（manager.c:405-495）只有第二相分类、第一阶段（解链/RS_DEAD/DISALLOW/
  CLEAR_IPC_REFS/late_reply）连决策都没有；`check_create_preconditions` 的"调用方清理"契约
  无人履行。
- **After**：`create_service(table, rp, kernel, ticks, read_exec)`——前置三闸门（失败即
  free_slot，兑现清理契约）→ `srv_fork`（失败即释放）→ `getprocnr`（C panic 语义保留）→
  `mark_child_created` → priv 设置+回读 → 调度 → `read_exec`（注入缝）→ `srv_execve` →
  无条件 RS 重-pin → `setuid(0)` → RS/VM pin + RS 实例重-pin → `vm_set_priv`；每步失败 =
  cleanup 两相 + RS 重-pin + errno。**KernelApi 扩展 5 方法**（生产 ENOSYS、mock 可配置 +
  记录）：`srv_execve`（线格式平铺参数；ARCH 偏差：不建模 environ）、`srv_kill`、
  `sched_stop`、`setuid`（VFS hack 保留）、`reply`（late_reply 与主循环共用）。
  `cleanup_service(table, rp, kernel, run_script)`：`RS_DEAD` 门控的两相——第一相解链四指针
  +清邻居回链+DEAD+DISALLOW/CLEAR_IPC_REFS+清 ACTIVE+补发 late reply；第二相
  sched_stop+SIGKILL（失败仅告警，与 C 一致）+脚本钩子+detach/自由释放（reincarnate 保槽）。
  **mock 保真度升级**：`getpriv` 按端点回显 `SetSys` 推入的结构（C 语义：sys_getpriv 读回
  内核侧副本）——旧 mock 恒返回 vacant，boot 测试的"vacant 断言"实为锁死 mock 伪行为，
  已随测试修正。
- **Verified**：`cargo test -p minix-rs` = **246 passed**（+7：编排 happy path/
  前置失败释放/fork 失败释放/exec 失败清理链/reincarnate 保槽/两相 cleanup/边界 sanity）；
  测试自查抓出并修正 4 处测试自身错误（in_use 断言、时点断言、setup 遗漏）；
  clippy/fmt 零输出；T7 PASS。文档同步：10 §3.1/§3.2（编排落地 + KernelApi 扩展表）、
  15 §3.1（两相执行体行）、boot §5（mock 回显语义）。

### ✅ Fix #52 — R22b+A4（P1-design-missing 收尾）：run/start/kill/crash/detach 编排 + 控制域载荷模式
- **File**：`os/servers/rs/src/service_create.rs`（run_service/start_service 编排 + 5 测试 +
  OQ-3 落地）、`recovery.rs`（crash_service/kill_service/detach_service 执行体）、
  `boot.rs`（KernelApi 新增 sys_kill——与 PM 面 srv_kill 分立）、`testutil.rs`、
  `request.rs`（stop_service → stop_decision）、`lib.rs`；文档 13 §2.1b
- **After**（C 锚点）：
  - `run_service`（manager.c:923-948）：SYS_PRIV_ALLOW → kill on 失败；`init_service`
    组装（utility.c:18-64）= `mark_initializing` + RS 自初始化早退（ROOT_SYS_PROC，
    utility.c:29-31——RS 不给自己发 RS_INIT）+ 老端点推导（old_rp 优先于 prev_rp）+
    `init_message`（含 `take_map_prealloc` 单次移交）+ `rs_asynsend` 注入缝（19）。
  - `start_service`（manager.c:950-983）：fold → create → activate → publish（11 号 seam，
    失败不清理——与 C 一致）→ run。
  - `crash_service`（manager.c:380-403）：RS → `CrashOutcome::SelfTerminate`（C `exit(1)`，
    06/18 接线转自身终止）；其余 `sys_kill(endpoint, SIGKILL)`（内核面）——**与 PM 面
    `srv_kill(pid)` 分立**（C 两个 kill 面不再混用一方法）。
  - `kill_service`（manager.c:360-378）：置 `RS_EXITING` + crash，忽略 crash 结果、透传
    输入 errno（与 C `return err` 一致）。
  - `detach_service`（manager.c:497-528）：`"{counter}.{label}"` 重发布（DS 效果注入、
    counter 归调用方所有）、保槽 `IN_USE|ACTIVE`、清 CORE_SRV/DET_RESTART、period/dev_nr/
    nr_domain 清零、re-allow。
  - **A4**：`stop_service` → `stop_decision(slot, how, ticks) -> StopDecision { signal,
    mutations }`（R13 载荷；`shutdown_apply` 全表扫描保留直接变异——表编排域与逐槽控制域
    分界）。**OQ-3 落地**：clone_slot 的 exec 共享改走 `share_exec` 单一实现点。
- **Verified**：`cargo test -p minix-rs` = **250 passed**（+4：crash/kill 语义、detach 重标签
  与降级、run_service ALLOW+RS 早退、start 全管线）；clippy/fmt 零输出；T7 PASS。
  文档同步：13 §2.1b（A4 决策化）。

### ✅ Fix #57 — 轮17（signal_handler 主体路由，R34.21/22 部分）
- **File**：`os/servers/rs/src/lib.rs`（RsServer::signal_handler 主体 + KernelApi::waitpid
  生产/ mock + booted-state 测试夹具）
- **After**：SIGCHLD → waitpid 排空循环逐子进程 `sigchld_cleanup`（07）；SIGTERM →
  `shutdown_apply` + `shutting_down` 置位；未知信号忽略。KernelApi 新增 `waitpid() ->
  Option<Pid>`（WNOHANG 面，19 接线；mock 用 children 栈）。
- **Verified**：`cargo test -p minix-rs` = **261 passed**（+3：TERM 扫描+置位、CHLD 空
  排空无副作用、未知信号忽略）；clippy/fmt 零输出；T7 PASS。文档同步：13 §2.1c、
  06 §2.1b。

### ✅ Fix #58 — 轮18（get_work receive 经 KernelApi 注入，R34.18-20 部分闭合）
- **File**：`os/servers/rs/src/boot.rs`（KernelApi::receive 三元组缝 + 生产 ENOSYS）、
  `testutil.rs`（mock receive ENOSYS——罐装消息由 19 mock 面提供）、`lib.rs`
  （get_work 删除 todo!，委托 receive 缝；run() 签名 `!` → `Result<(), Errno>`）、
  `main.rs`（`let _ = server.run()` fail-closed 注记）
- **After**：`KernelApi::receive(endpoint) -> Result<(Message, IpcStatus, Clock), Errno>`
  —— 第三元素即 R25 的 notify timestamp（ipc.h:1715），由 19 的安全 receive 包装提取
  （union 读取需要 unsafe，本 crate 禁用）；生产面 ENOSYS 时 run() 以 Err 结束——不再
  todo!（T2：进程退出可见而非自旋）。lib.rs:244 的 todo! 标记移除，T7 门剩余 1 处
  （boot.rs:774 自升级，轮 20 edge）。
- **Verified**：`cargo test -p minix-rs` = **261 passed**；`tools/check-rs-unwired.sh`
  PASS（剩余 1 标记带契约）；clippy/fmt 零输出。文档同步：06 §2.1a。

### ✅ Fix #59 — 轮19（SEF 回调接线：init_restart/init_lu 落地 + init_response/lu_response edge 定界）
- **File**：`os/servers/rs/src/lib.rs`（RsServer 四个 SEF 回调主体）、文档 18 §2.9b
- **After**：
  - `init_restart`（main.c:499-544）：RS 槽 + old_endpoint 槽解析 → 更新中
    `end_update(ERESTART, RS_REPLY)` → `update_service(RS_DONTSWAP)` →
    `init_service(SEF_INIT_RESTART)`（RS 自初始化不发送）→ `setalarm(RS_DELTA_T)`
    重挂（panic 语义保留）。
  - `init_lu`（main.c:549-586）：`update_service(RS_DONTSWAP)` →
    `init_service(SEF_INIT_LU)`。
  - `init_response`/`lu_response`：决策面全就绪（`do_init_ready` 四参数 +
    `do_upd_ready` + normalize 包装 + pending 持有），**edge = 消息载荷解码**——
    `m_rs_init.result`/`m_rs_update.result` 位于 union 臂，安全提取归 19 的
    receive 包装（R25 同源）。保持 ENOSYS + 落地形态注释。
- **Verified**：`cargo test -p minix-rs` = **261 passed**；clippy/fmt 零输出；
  T7 PASS（剩余 1 标记 = boot.rs:774 自升级，见 edge 清单）。文档同步：18 §2.9b。

### ✅ Fix #58 — 轮18（get_work receive 经 KernelApi 注入，R34.18-20 部分闭合）
- **File**：`os/servers/rs/src/boot.rs`（KernelApi::receive 三元组缝 + 生产 ENOSYS）、
  `testutil.rs`（mock receive ENOSYS——罐装消息由 19 mock 面提供）、`lib.rs`
  （get_work 删除 todo!，委托 receive 缝；run() 签名 `!` → `Result<(), Errno>`）、
  `main.rs`（`let _ = server.run()` fail-closed 注记）
- **After**：`KernelApi::receive(endpoint) -> Result<(Message, IpcStatus, Clock), Errno>`
  —— 第三元素即 R25 的 notify timestamp（ipc.h:1715），由 19 的安全 receive 包装提取
  （union 读取需要 unsafe，本 crate 禁用）；生产面 ENOSYS 时 run() 以 Err 结束——不再
  todo!（T2：进程退出可见而非自旋）。lib.rs:244 的 todo! 标记移除，T7 门剩余 1 处
  （boot.rs:774 自升级，轮 20 edge）。
- **Verified**：`cargo test -p minix-rs` = **261 passed**；`tools/check-rs-unwired.sh`
  PASS（剩余 1 标记带契约）；clippy/fmt 零输出。文档同步：06 §2.1a。

### ✅ Fix #53 — A2（架构建议落地）：`UpdateState` 单点持有 + per-slot `r_upd` 载体
- **File**：`os/servers/rs/src/live_update.rs`（`UpdateState` + 相位写入口 +
  `RupdateFlags` 迁入 + `UpdateEntry` 补 4 字段）、`lib.rs`（ServerState.update 挂载 +
  导出迁移）、`boot.rs`（into_state 初始化）、`service_slot.rs`（`upd` 载体）、
  `process_table.rs`（RupdateFlags 移出）；文档 16 §3.1、02 §3.5
- **Before**：rupdate 全局（type.h:43-52）碎片化——`RupdateFlags` 无处挂载、
  `num_init_ready_pending` 退化为入参、per-slot `r_upd` 整体缺席、`UpdateEntry` 缺
  prepare 计时/grant 字段；相位只能"解码"不能"驱动"。
- **After**：`UpdateState { flags, chain, num_init_ready_pending }`（live_update.rs）挂进
  `ServerState`；相位写入口 `begin_updating()`/`begin_initializing()`（C 仅有的两个全局
  写点 update.c:510/:548）；`ServiceSlot.upd: Option<UpdateEntry>`（C `r_upd` 内嵌副本，
  type.h:62）；`UpdateEntry` 补 `prepare_tm`/`prepare_maxtime`/`prepare_state_data`
  （`RsStateData`）/`prepare_state_data_gid`（type.h:35-39）；`RupdateFlags` 迁至
  live_update.rs（LU 域单一权威，self_lifecycle/lib.rs 导入同步改道）。
- **Verified**：`cargo test -p minix-rs` = **250 passed**（`test_update_phase_flags`
  扩展相位写断言：begin_updating → 仅 UPDATING，begin_initializing → UPDATING|
  INITIALIZING 并存——与 C 的 flags |= 语义一致）；clippy/fmt 零输出；T7 PASS。
  文档同步：16 §3.1 表（UpdateState/UpdateEntry 两行）、02 §3.5（per-slot 更新描述符）。

### ✅ Fix #54 — R23a（P1-design-missing 前半）：LU 链操作四缺落地
- **File**：`os/servers/rs/src/live_update.rs`（`UpdateChain::set_new_upd_flags`/
  `clear_upds`、`UpdateState::clear_upds`/`upd_move` + 3 测试）；文档 16 §3.1
- **Before**：`rupdate_clear_upds`（update.c:7-18）、`rupdate_set_new_upd_flags`
  （update.c:88-116）、`rupdate_upd_clear`（update.c:135-159）、`rupdate_upd_move`
  （update.c:164-180）零载体——`UpdateChain` 只有 add；`last_lu_flags` 原料无消费方。
- **After**：`set_new_upd_flags(&mut entry)`（插入前标志计算：链非空 MULTI + last 的
  INCLUDES_VM/RS 传播 + preparing-only 短路 + VM/RS 自标记——`last_lu_flags` 原料就此
  消费）；`clear_upds`（逐描述符清 new 实例走 cleanup_service、grant 复位 [cpf_revoke
  归 19]、RUPDATE_CLEAR 语义）挂 `UpdateState`；`upd_move`（entry.slot 重戳 + new_rp 链
  转移 + 源复位——**A-3 索引链使 first/last 无需重指**，较 C 的指针修补显著简化）。
  **已核实正确（防误判记录）**：`alloc_slot` 是 find-only 原语、不置 IN_USE（C 同，
  manager.c:2067-2083——置位在 create_service 的 mark_child_created，:589）；两个连续
  alloc 无 intervening 标记会返回同一行——测试构造需先标记。
- **Verified**：`cargo test -p minix-rs` = **253 passed**（+3：set_new_upd_flags 的
  MULTI/传播/preparing-only 三态、clear_upds 的清理+复位、upd_move 的描述符/链接转移）；
  测试自查修正 2 处（alloc find-only 语义、preparing-only 的传播来源）；clippy/fmt 零
  输出；T7 PASS。文档同步：16 §3.1 表 +3 行。

### ✅ Fix #55 — R23b（P1-design-missing）：LU 中段编排三函数 + prepare-only 取消
- **File**：`os/servers/rs/src/live_update.rs`（`start_update_prepare_next`/
  `start_update_prepare`/`start_srv_update`/`start_update` + 2 走链测试）；文档 16 §3.1 +4 行
- **After**（C 锚点）：
  - `start_update_prepare_next`（update.c:467-527）：首走取头、后续走 next；VM-multi
    预置段（update.c:489-515——`vm_prepare` 注入缝）；置 `RS_UPDATING`（update.c:510 相位
    写入口消费）；prepare-only 跳过循环（update.c:516-525）；耗尽返回 None。
  - `start_update_prepare`（update.c:401-464）：UPD_SCHEDULED 门（EINVAL）→ 非 idle +
    `!allow_retries` → `abort(EAGAIN)` 钩子 → VM-multi 老新端点与 VM_UPDATE/NOMMAP 策略
    标志填充（update.c:442-454）→ 链耗尽 `end(OK)`+ESRCH。
  - `start_srv_update`（update.c:621-652）：pending 计数++（UpdateState 持有）、新实例
    `INITIALIZING|INIT_PENDING`、NOMMAP 传播、`update_service` 缝（RS 跳过），
    失败 → `end_update(r)`。
  - `start_update`（update.c:532-652）：置 `RS_INITIALIZING`（:548）、prepare-only 取消
    （:551-555 NULL prepare-state）、逐描述符 `start_srv_update`+`complete_srv` 缝
    （VM-multi 最后完成，:566-572）、无事 `end_update(OK)`（:579-582）、VM 等待缝
    （`receive_vm_init(maxtime)`，:585-640——do_init_ready/reply 归 06/12/19）。
    kernel/read_exec 缝参数保留位（complete_srv 深路径 16 落地时消费）。
- **Verified**：`cargo test -p minix-rs` = **255 passed**（+2：走链次序+相位写入+
  prepare-only 连跳、耗尽 None）；clippy/fmt 零输出；T7 PASS。文档同步：16 §3.1 表 +4 行
  （prepare/prepare_next/start_srv_update/start_update）。

### ✅ Fix #56 — R23c+R27（P1-design-missing 收尾）：update/rollback/complete_srv/end_update 编排落地
- **File**：`os/servers/rs/src/live_update.rs`（update_service/rollback_service/
  end_srv_update/end_update_rev_iter/end_update/complete_srv_update + 3 测试）、
  `boot.rs`（KernelApi 新增 sys_update——内核 SYS_UPDATE 面，与 PM 面 srv_fork/srv_kill
  命名对齐）、`service_create.rs`（init_service 独立化：old_endpoint 参数化，表读取在
  调用方）、文档 16 §3.1
- **After**（C 锚点，全部按 A1 忠实编排函数形态）：
  - `update_service`（update.c:262-325）：`srv_update` 内核身份交换（RS_SWAP 时）→
    `swap_slot` → pid/endpoint 互换回写（update.c:292-299，快索引跟随）→ priv 双向回读
    （C panic 语义保留为 expect）→ `activate_service(dst, src)`。
  - `rollback_service`（update.c:330-366）：**RS 分支 R27(a)**——全 ACTIVE 槽
    `r_check_tm = 0`（心跳重发扫，RS 自身行也在内）；`me != RS` 时 VM rollback；非 RS
    分支：INIT_PENDING 新实例免交换，否则先 DISALLOW 冻结再反向 update_service
    （SF_VM_ROLLBACK）。
  - `end_srv_update`（update.c:932-1008）：幸存者清 update 标志位+check_tm 复位+
    alive_tm=ticks；描述符 `rp` 改指幸存者；解链；RS_REPLY 回复/RS_CANCEL 补 NULL
    prepare；exiting 逐实例 cleanup（DETACHED 老实例 → CLEANUP_DETACH+EDEADEPT 回复）。
  - `end_update_rev_iter`（update.c:816-860）：反向遍历 + 位置分类
    （curr/before_prepare/prepare_done/initializing，由 RS_INITIALIZING 相位决定）→
    curr/initializing 的 init 失败先 rollback 再 end_srv_update；before_prepare 仅清
    new 版本；prepare_done 用 RS_REPLY。
  - `end_update`（update.c:865-927）：**R27(b)** RS_INIT_DONE 失败短路 → 返回
    `CrashOutcome::SelfTerminate`（C `exit(1)`；Rust 无进程退出，交调用方终止运行循环，
    与 fail-closed 边界一致）；prepare-only 取消；rev_iter 两遍（VM 最后）；成功路径
    upd_clear+`end_srv_init`；late_reply(last)；RUPDATE_CLEAR；全表 pub 端点/VM 标志复位。
  - `complete_srv_update`（update.c:657-702）：清 INIT_PENDING；RS 自更新 →
    `init_service(SEF_INIT_LU)`+`SYS_PRIV_YIELD`（C panic 语义保留）→ rollback+
    `end_update(ERESTART, RS_REPLY)`；其余 `run_service(SEF_INIT_LU)`，失败 → rollback+
    `end_update(r, RS_REPLY)`。
  - `restart_service`（manager.c:1246-1298，补入本轮）：late_reply → 脚本分支（失败
    kill）→ clone_service(RST_SYS_PROC，无副本时) → `update_service(RS_SWAP)` →
    `run_service(SEF_INIT_RESTART)` → DET_RESTART+restarts<MAX → CLEANUP_DETACH。
  - abort 时序注释（R27 第三项）：recovery.rs 中"executed around"表述已按 C 的内嵌
    时序修正（manager.c:1099-1102/:1127-1130）。
- **Verified**：`cargo test -p minix-rs` = **258 passed**（+3：R27(a) 心跳扫含旁观者、
  R27(b) SelfTerminate 短路且无内核调用、restart 脚本分支不进 clone）；clippy/fmt 零
  输出；T7 PASS。文档同步：16 §3.1 表 +6 行（update/rollback/end_srv_update/
  rev_iter/end_update/complete_srv_update）。


### ✅ Fix #60 — E-2/R9（P2 结构性，19 前定）：`KernelApi` 单体 trait 拆五个域面 + supertrait 组合
- **File**：`os/servers/rs/src/boot.rs`（trait 区重写）、`testutil.rs`（mock 拆 5 个域 impl）、
  `lib.rs`（测试模块一行 unused import 卫生修复，轮 19 遗留警告）；文档 01 §3.5（重写为
  五域面结构）、10 §3.2（域归属注记）、99 §3.4（shell 清单补充）
- **Before**：`KernelApi` 单体 22 方法横跨四个消息目标 + 自身 IPC——"这条调用发给谁"埋在方法名
  里（R9，§14）；mock 被迫全量实现；19 接线只能一个 impl 块一次写完。
- **After**：五域面——`SysApi`（内核 SYSTASK 8：get_machine/get_hz/get_ticks/privctl/getpriv/
  setalarm/sys_kill/sys_update）、`SchedApi`（调度器 2：sched_init_proc/sched_stop）、`PmApi`
  （PM 进程生命周期 8：getnuid/getnpid/getprocnr/srv_fork/srv_execve/srv_kill/waitpid/setuid）、
  `VmApi`（VM 2）、`IpcApi`（RS 自身 IPC 2：receive/reply）；`KernelApi: 五面` + blanket impl，
  全部 `&mut dyn KernelApi`/`Box<dyn KernelApi>` 调用点零改动（supertrait 方法经 vtable 可调）。
  方案对比：a) 域 supertrait + 组合（选定）vs b) 消费点改多 trait 对象（Rust 不支持非 auto
  trait 的 `dyn A + B`，绕回 a）vs c) RsServer 持多个 `Box<dyn XxxApi>`（同一传输对象拆多盒
  需要自引用/内部可变性，违反单线程 `&mut` 模型）vs d) 保持单体只做文档分组（类型事实不变，
  收益为零）。选 a 的理由：19 接线按面逐个实现传输（内核面配 SYS_* 包装、PM 面配消息构造）；
  测试可只实现被测面（新增 `test_domain_face_implementable_in_isolation` 证明）。
- **C 面核实（锚点）**：`getnpid`/`getnuid` → `PM_GETEPINFO`（lib/libsys/getepinfo.c:15-47）；
  `vm_set_priv` → `_taskcall(VM_PROC_NR, VM_RS_SET_PRIV)`（lib/libsys/vm_set_priv.c:7）；
  `sched_stop` → 调度器端点 SCHEDULING_STOP（sched_stop.c:9-28，KERNEL/NONE 短路）；
  `sched_start` 复合（KERNEL→sys_schedctl :70-71，否则 SCHEDULING_START :87）；
  `srv_execve` 注释误述修正——C 实现是 RS 内 ELF 装载（exec.c:21-64）+ 内核分配/拷贝 +
  PM 终步（libexec_pm_newexec exec.c:102、PM_EXEC_RESTART exec.c:127），非"打包 argv 进消息"；
  `setuid` 走 PM（POSIX 面）——原注释"kernel-boundary call"一并修正。
- **Verified**：`cargo test -p minix-rs` = **262 passed**（261 + 1 新隔离测试）；
  `cargo clippy -p minix-rs --lib` 本体零告警（依赖 minix-sys 的 4 个历史告警不在本 crate）；
  `cargo fmt -p minix-rs -- --check` 干净；`tools/check-rs-unwired.sh` PASS（标记集不变）。
  文档同步：01 §3.5 重写（五域面 + R9 修正理由 + S4 语义保留）、10 §3.2 表头域归属、99 §3.4
  域拆分条目。

### ✅ Fix #61 — E-6/T3 残留（P2，19 前定）：boot 错误域二分——`BootError` 与 `ENOSYS` 解耦
- **File**：`os/servers/rs/src/boot.rs`（`BootError` + 全链签名）、`lib.rs`（诊断字段 +
  trait 面降级）、`main.rs`（致命启动报告）、`sef.rs`（测试扩展）；文档 01（§3.4 后 E-6
  注记 + init_fresh 签名 + §5.2/§5.4 同步）
- **Before**：boot 的两族失败共用一个 `Errno::ENOSYS`——表计数不符/endpoint 错位（R17）/
  lookup 未命中/getnpid 负值（C 全部 `panic`，main.c:226/427-429/731/746）与"内核面未接线"
  （T2 的 ENOSYS 语义）在类型上不可区分；main.rs 的 panic 消息硬编码 "kernel API wiring
  pending"，对表损坏类失败是误导。
- **After**：`pub enum BootError { Lookup(LookupError), CountMismatch, EndpointMismatch,
  InvalidPid(Pid), Kernel(Errno) }`（Copy，可经 `boot_diagnostic()` 读取）；`BootInit` 的
  validate_tables/step0-4/init_fresh 全链返回 `Result<(), BootError>`，`From<Errno>` 让内核
  调用 `?` 自动归入 `Kernel`（无 map_err 噪声），`From<LookupError>` 同理；线面
  `From<BootError> for Errno`——`Kernel(e)` 保留原 errno（接线期 ENOSYS 语义不变），
  不变式违例 → `EINVAL`（C 无对应 errno：panic 族；EINVAL 诚实表达"boot 数据不可用"，
  ENOSYS 从此专属"未接线"）。方案对比：a) BootError 枚举 + From 双向转换（选定）vs b) 全程
  裸 Errno + 诊断日志（no_std 无 log 设施，类型不可区分）vs c) boot 失败改 panic 对齐 C
  （违反 T2 已定契约：fail-closed Err 保持进程存活、缺口在调用点可见）。SEF 回调面
  （`SefCallbacks::init_fresh -> Result<i32, Errno>`）保持 C `int` 契约不动，lib.rs 降级点
  同时把类型化原因存入 `RsServer.boot_diagnostic`——main.rs 报告恢复 C panic 消息的
  诊断能力。
- **Verified**：`cargo test -p minix-rs` = **264 passed**（+2：`test_step4_negative_pid_
  reports_invalid_pid`（含线面 EINVAL 断言）、`test_boot_error_wire_mapping`；validate
  两测从 `is_err()` 升级为 `Err(BootError::CountMismatch)`/`Err(BootError::EndpointMismatch)`
  变体断言；`test_deferred_callbacks_fail_closed` 扩展诊断断言）；clippy/fmt 触碰文件零输出；
  `tools/check-rs-unwired.sh` PASS。文档同步：01 §3.4 后 E-6 注记块、init_fresh 签名、
  §5.2 表 4 行更新 + 2 行新增、§5.3 一行扩展、§5.4 计数 262→264（boot.rs 21，01 范围 33 项）。

### ✅ Fix #62 — E-5/D6（P2 结构性）：io/irq 双表收敛——`Privilege` 单一权威 + 派生快照
- **File**：`os/servers/rs/src/service_slot.rs`（字段文档 + `refresh_priv_backup`）、
  `slot.rs`（edit_slot 删独立备份写语句 + 新测试）；文档 02（§2.3 两处修正）、03（§3.2 权威
  声明 + irqs 类型漂移修正 u32→i32）、08（§5 测试表 +1 行）
- **Before**：`ServiceSlot.io_tab/irq_tab/nr_io_range/nr_irq`（备份）与
  `Privilege.io_ranges/irqs/nr_*`（内核强制面）由 edit_slot 的**两条独立写语句**分别维护
  （`slot.irq_tab[i] = rs_start.irq[i]` 与 `slot.priv_.irqs[i] = ...` 分立）——比 C 的"同语句
  双写"（manager.c:1498）更易漂移，且无任何一致性保证（D6 原始发现）。
- **After**：ground truth 先行——全树 grep 证实备份表零读者（RS 不读回；`r_priv` 刷新走
  `sys_getpriv` 回读：main.c:294、manager.c:601/1819、update.c:308-310），故权威 = `r_priv`
  （内核强制面镜像），备份 = 随行快照。`refresh_priv_backup(&mut self)` 从自身 `r_priv` 一次
  派生四字段；edit_slot 的 IRQ/IO 分支只写 priv，分支后一次 refresh（与 C 的写序锚点
  manager.c:1498/:1519 对齐）。方案对比：a) 权威+派生快照（选定）vs b) 删除备份字段
  （违背 C 结构发布面保真——字段在 `struct rproc` 内部而非 rprocpub，序列化不受影响但
  字段保真是 A-2 契约）vs c) 保持双写（Rust 现状比 C 更差，不成立）。boot 路径不写备份
  （C 同——boot 激活只整结构拷 priv，备份保持零值），无需处理。
- **顺带修正（文档 P2-fact）**：02 文档 §2.3 第 3 点与字段归属表声称"edit_slot 用备份表
  重建 r_priv"——C 无此机制（备份零读者），重建实为 sys_getpriv 内核回读；一并改正。
  03 文档 §3.2 代码块的 `irqs: [u32; ...]` 显示漂移改为 `i32`（Fix #48 已改代码，文档漏同步）。
- **Verified**：`cargo test -p minix-rs` = **265 passed**（+1
  `test_priv_backup_is_derived_snapshot`：显式表后备份与 priv 双侧一致 + sentinel 双侧清零
  + CHECK 标志不清除——自查抓出本测试初版断言"sentinel 清除标志"违反 C 语义并修正，
  Gate E/测试自查记录）；clippy/fmt 触碰文件零输出。文档同步：02 §2.3（两处事实修正）、
  03 §3.2（权威声明 + 类型修正）、08 §5（+1 行）。

### ✅ Fix #63 — A3（建议落地）：SEF restart 回调重绑建模——`RestartCb` 枚举
- **File**：`os/servers/rs/src/sef.rs`（`RestartCb` 枚举 + 测试）、`lib.rs`（字段 +
  init_restart 分派 + init_lu 重绑 + 导出）；文档 18 §2.9b（重绑建模段）、01 §3.3（trait
  清单补充）
- **Before**：C 的重启回调表项是运行期状态——启动注册 RS 自有 handler（main.c:140），
  `sef_cb_init_lu` 在 LU 开始时重绑为 stateful 通用体（main.c:558）；Rust 的 `init_lu`
  只有注释挂账（Fix #59 记"由 A3 定案"），重绑不可表达，LU 后再重启会错误地走 Rs 全链。
- **After**：`sef::RestartCb { Rs, Stateful }`（Default = Rs，即 main.c:140 注册态），
  `RsServer.restart_cb` 字段承载；`init_lu` 第一语句置 `Stateful`（对齐 main.c:553-556
  "先重绑后流程"，且 C 在 LU 失败时不回滚重绑——Rust 同）；`init_restart` 以
  `match self.restart_cb` 分派：`Rs` 臂 = main.c:499-544 全链（原实现），`Stateful` 臂 =
  `sef_cb_init_restart_generic`（libsys/sef_init.c:317-330，检查点/同一性转移——机制归
  16/17，落地前 fail-closed `Err(ENOSYS)`）。方案对比（A3 原案）：a) trait 保持静态 +
  单枚举字段承载重绑（选定——重绑只有一个点，一个字段即同构）vs b) 整个回调集合 enum 化
  （`SefCbSet::Fresh/LuStateful`——7 个方法整体切换，C 只换一个表项，过度建模）。
- **Verified**：`cargo test -p minix-rs` = **266 passed**（+1
  `test_init_lu_rebinds_restart_cb`：重绑先于流程、LU 失败不回滚、分派走 Stateful 臂）；
  clippy/fmt 触碰文件零输出；`tools/check-rs-unwired.sh` PASS。文档同步：18 §2.9b 重绑段、
  01 §3.3 补条目。

### ✅ Fix #64 — E-9/E2（P2 测试基建）：解析面零依赖 property 测试 6 项
- **File**：`os/servers/rs/src/testutil.rs`（`XorShift` PRNG）、`slot.rs`/`service_create.rs`/
  `state_data.rs`/`ipc_mask.rs`/`privilege.rs`/`recovery.rs`（各 1 个 property 测试）
- **Before**：解析函数（请求面、攻击者可控输入）只有手写样例——E2 自 2026-08-15 挂账，
  R34.24（IPC_ALL 位域契约）等边缘无系统覆盖。
- **After**：`proptest` 不可用（离线）→ `testutil::XorShift`（xorshift64*，seed|1 防零态，
  `below(n)`/`fill(alphabet)` 辅助；固定种子 = 失败可精确重放）+ 5000/2000 次迭代的生成式
  不变式断言（清单见 E-9 标注）。每个不变式对应一条 C 语义锚点（S1/N11/R15/A-14/N6/N7/
  R21/Fix #32）。方案对比：a) 零依赖 PRNG property（选定——离线可落地，覆盖生成式输入）
  vs b) 等 proptest 可用（无限期挂账）vs c) 穷举边界样例（已有单测，无生成覆盖）。
- **Verified**：`cargo test -p minix-rs` = **272 passed**（+6）；测试自查修正 3 处测试自身
  错误（Gate E 记录）；clippy 触碰文件零告警；`cargo fmt` 收敛 3 文件格式；T7 PASS。

### ✅ Fix #65 — E-10/R34.18-21（P2 测试基建）：boot 失败传播族 + 壳层 fail-fast 契约 + SIGCHLD 实排空
- **File**：`os/servers/rs/src/testutil.rs`（`fail_calls` 注入 + `Call::same_variant`）、
  `boot.rs`（3 传播测试）、`lib.rs`（`booted_with` 夹具参数化 + 3 壳层测试）；
  文档 01 §5.2/§5.4、06 §5（壳层测试表）
- **Before**：mock 只有"成功值"注入（hz/ticks/pids/ok 开关），内核调用**失败**不可注入
  （R34.18 自认"mock 可注入但未用"）；run()/二次 init 的 panic 契约、SIGCHLD 实排空
  （非空 waitpid → 槽位释放）零测试。**披露：01 §5.4 全局计数在 E-9 轮漏同步（停在
  264），本轮一并修正到 278。**
- **After**：`MockKernelApi.fail_calls: Vec<Call>`——按变体匹配（`Call::SetAlarm(0)` 失败
  全部 setalarm；带开关的方法保持各自 canned 语义）。新增：step0 get_machine 失败 →
  `Err(BootError::Kernel(ENOSYS))` 且后续零调用；计数一致但成员错位 → step1
  `Err(BootError::Lookup(ImageTable))`（E-6 类型化传播的端到端验证）；step4 setalarm
  失败 → 整 boot 失败且闹钟调用已发出。壳层：`run()` 无 boot panic、二次 `init(Fresh)`
  panic（boot 机器已被 T1 handover 消费）、SIGCHLD 实排空（children=[700, 808] → VFS
  槽释放 + endpoint_slot None + 未知 pid 无副作用）。
- **Verified**：`cargo test -p minix-rs` = **278 passed**（+6）；clippy 触碰文件零告警；
  fmt 干净；T7 PASS。测试自查修正 2 处构造错误（step1 表计数不匹配被 validate_tables
  先拦、PM proc_nr 写错被 R17 校验拦——两道既有校验恰好证明了 boot 表校验链的有效性）。

### ✅ Fix #66 — E-8/R31（P2）：错误名面——`Errno::name()`/`Display` + error.rs 上下文描述表
- **File**：`os/libs/minix-types/src/types/errno.rs`（`name()` + `Display`，115 常量
  生成式 match）、`os/servers/rs/src/error.rs`（新建：`init_strerror`/`lu_strerror` +
  2 测试）、`lib.rs`（模块注册）；文档 99 §4、todo.md
- **Before**：error.c 的错误名/描述面全缺（§18.0 覆盖度工具确认 `rs_strerror`/
  `errentry` 为"完全缺口"）；错误诊断只能打裸数值（Debug 形态 `Errno(22)`）。
- **After**：方案对比——a) 单一 errno→名表（`Errno::name`）+ RS 薄描述层（选定，
  即 R31 "Errno Display 承接、不单独移植查表"的落地）vs b) RS 侧复制一份 errno→名表
  （跨 crate 双表，漂移重蹈 D4/N1）vs c) 只做 Display 不做 name（init/lu 的
  `&'static str` 回退无法组合）。实现要点：match 臂用常量标识符本身（编译期保证与
  常量表一致）；`ELAST≡EPROTO(96)` 同值别名由首个常量名胜出；`init_strerror`/
  `lu_strerror` 逐条对照 error.c:12-15/:20-25 的描述文本，miss 分支回退
  `strerror(-errnum)` 语义（error.c:44）。
- **Verified**：`cargo test -p minix-types` = **170 passed**（+1
  `test_errno_display_and_name`：名面/未知值降级/name-Display 一致）；RS
  **280 passed**（+2：init/lu 表与回退、ENOSYS 双上下文文本不同断言）；clippy
  触碰 crate 零告警（ELAST 不可达臂被 clippy 抓出后删除——生成式代码也过门）；
  fmt 干净；T7 PASS。

### ✅ Fix #67 — E-3/D2/R8（P2 结构性，判定闭合）：世代计数不采纳 + `assert_consistent` 落地
- **File**：`os/servers/rs/src/process_table.rs`（`assert_consistent` + 正/负两测）、
  `service_create.rs`（golden 测试接线 + 夹具双标志修正 + 2 处调试输出清除）、
  `boot.rs`（handover 接线）、`recovery.rs`（2 处调试输出清除）；文档 02 §3.5
  （settled-state 表级不变量条）、todo.md（D2 行 / E-3 判定 / §14 R8 关联）
- **Before**：E-3/D2/R8 挂账"SlotId 无世代，free→reuse 后旧索引悬垂"，拟世代计数
  全量迁移；R8 断言"C 中 rproc 数组行从不复用"。
- **判定过程（Ground Truth 先行）**：`sed -n '2067,2083p' manager.c` 证伪该断言——
  C 的 `alloc_slot` 就是首个非 IN_USE 行扫描，复用是 C 的原生语义；悬垂指针风险
  在 C 同样存在，由协议纪律（不跨 free/reuse 持指针）而非语言机制约束。Rust 的
  索引式 SlotId 忠实映射该语义；世代校验会把 C 能跟的悬垂链变成编译/运行期拒绝，
  偏离被测试锁定的忠实行为（Fix #43 同型教训）。方案对比：a) 世代句柄全量迁移
  （不采纳——(1)(2)）vs b) 内部链接收进表 + 私有化（归 E-4 结构轮）vs c) debug-only
  settled-state 不变式校验器（选定——零语义变化、抓自由→reuse 悬垂形态、成本最低）。
- **Verified**：`cargo test -p minix-rs` = **282 passed**（+3：accepts_settled_table、
  catches_stale_index `#[should_panic]`、既有测试接线断言）；校验器首跑抓出 cleanup
  夹具的 `pub_.in_use` 漏写（真实双标志不同步）并修正；clippy 触碰文件零告警；
  fmt 干净；T7 PASS。附带清除 R22a 轮 4 处 `eprintln!` 调试残留（回归 review 疏漏，
  本轮披露）。

### ✅ Fix #68 — E-4/D1/R33（P2 结构性，判定闭合）：`ServiceSlot` 删除 derive(PartialEq, Eq)
- **File**：`os/servers/rs/src/service_slot.rs`（derive 移除 + 语义注）；todo.md
  （D1 行 / E-4 判定 / §16 R33 关联）
- **Before**：`ServiceSlot` 派生 `PartialEq`/`Eq`——任何整槽 `==` 都会深比较
  `exec: Option<Arc<[u8]>>` 逐字节（R33：相等比较退化为 ELF 比较的脚枪）。
- **判定过程**：grep 证实全 crate（生产+测试）**零处**整槽相等——测试的既定风格
  就是字段级断言（clone_slot 测试 12 个字段各断各的）。方案对比：a) 手工 PartialEq
  只比较身份字段——"哪些字段算相等"在 45 字段上没有自然答案，部分相等的 `==`
  比没有 `==` 更危险（语义撒谎）vs b) 删除 derive（选定——未定义的语义不提供，
  脚枪根除）vs c) 保持 derive——脚枪保留，不成立。Redox 对照：`Resource`/`Scheme`
  句柄无全结构相等概念。
- **Verified**：`cargo test -p minix-rs` = **282 passed**（删除后零编译错误——
  反证无使用点）；clippy/fmt 零输出；T7 PASS。`PublicSlot` 的 derive 保留
  （发布面相等语义良定义、无重字段）。

### ✅ Fix #69 — 06 主循环接线（notify 半环 + RS_SHUTDOWN 臂 + 回复路径，R34.20 闭合）
- **File**：`os/servers/rs/src/dispatch.rs`（`isokendpt` + `ClockNotify{timestamp}`）、
  `boot.rs`（`IpcApi::notify` 缝，生产 ENOSYS + doc contract）、`testutil.rs`
  （`Call::Notify` + `inbox` 收件队列）、`lib.rs`（`run()` 真实循环 + `do_period`/
  `do_heartbeat`/`do_request`/`do_shutdown`/`reply_unless_suppressed` + 4 个 run-loop
  集成测试）；文档 06 §5（接线表）、01 §3.5（IpcApi 面）、07（消费注记）
- **Before**：`run()` 是骨架（分类后丢弃）；isokendpt 门、CLOCK 心跳扫、心跳刷新、
  请求回复路径全部未接线（06 号的最后一环）。
- **After**：`run()` 按逐行对照——`get_work` → isokendpt 门（R34.20；**C 真实域是
  `-NR_TASKS ≤ slot < NR_PROCS`**——初版按 `0..NR_PROCS` 实现被 CLOCK notify 测试当
  场证伪，kernel 任务必须放行，utility.c:78-85）→ 四类分派。`do_period`：`now` =
  CLOCK notify timestamp（request.c:948，`ClockNotify` 变体随 R25 模式携带）；
  槽门（:968-970）+ `period_decision`（`lookup_by_flags(INITIALIZING)` 喂参，
  Fix #45.6 的互链生效）+ mutations 一次提交 + 动作执行（Restart → restart_service
  八参编排、crash 双动作 → CrashOutcome::SelfTerminate 即循环 Err 终止（C
  `exit(1)`）、PingRequest → `notify` 缝，C 忽略其失败）。`do_heartbeat`：索引命中
  即刷新 alive_tm（Fix #43 原始索引语义，无 in-use 过滤）。请求臂：`RS_SHUTDOWN`
  是唯一免 union 解码的臂（`m_source` 即载荷）→ 权限 + sweep；其余臂经
  `dispatch_request` ENOSYS（OQ-4 按臂转真）。回复路径：`EDONTREPLY` 抑制 +
  fire-and-forget（utility.c:309）。
- **测试基建**：`MockKernelApi.inbox` 收件队列（`receive` 弹出，空 → ENOSYS 结束
  循环——T2 语义不变）；测试断言走状态面（`check_tm`/`alive_tm`/`shutting_down`/
  panic 契约），kernel 调用面经 mock 的 `calls` 由直接驱动测试覆盖。
- **Verified**：`cargo test -p minix-rs` = **286 passed**（+4：clock 驱动 ping、
  heartbeat 刷新、shutdown 臂、bogus source panic）；测试自查修正 3 处
  （mock 移交后 calls 不可断言 → 状态断言、endpoint 构造误用、isokendpt 下界
  ——最后一处由 CLOCK 测试当场证伪并回改）；clippy 触碰文件零告警；fmt 干净；
  T7 PASS（notify 缝带 19 契约）。文档同步：06 §5 接线表、01 §3.5、07 消费注记。

### ✅ Fix #70 — 12 号 init-ready 接线（union 解码落位 + do_init_ready 处理器 + catch_boot_init_ready 实装，R34.23 闭合）
- **File**：`os/libs/minix-types/src/ipc/message.rs`（`Message::rs_init_result()` 类型化
  安全访问器 + 测试）、`boot.rs`（`catch_boot_init_ready` 实装：step2 SYNCH_BOOT 分支
  与 step3 循环 + 3 测试）、`lib.rs`（`do_init_ready` 处理器 + `init_response` 活包装
  + 4 测试）、`sef.rs`（测试期望更新）；文档 12
- **Before**：R25/Fix #59 判定"union 臂读取需 unsafe → 解码归 19 的安全 receive 包装"，
  init path 整体停在 fail-closed。
- **判定修正（decode 落位）**：Rust 语义里 union 字段**写入是安全的**（位存储），只有
  **读取**需要 unsafe——而读取可以收敛进 minix-types（该 crate 本就拥有 union 的
  unsafe 面，FIX-08 集中化先例）。方案对比：a) minix-types 加 RS 对的类型化访问器
  （选定——RS crate 保持零 unsafe，2 个访问器不触发 FIX-08 拒绝 per-field 时的
  规模论证）vs b) 19 号在 rs crate 内写 unsafe 包装（违反零 unsafe 契约）vs c) 维持
  edge-gated（解锁条件其实已在库内，无谓阻塞 12 号）。
- **After**：`Message::rs_init_result() -> Option<i32>`（m_type 标签守卫 + SAFETY 注）；
  `RsServer::do_init_ready`（request.c:462-529 全分支：Unexpected→EINVAL、InitFailed→
  crash + SelfTerminate 终循环、UpdateInitDone→pending 回写 + end_update(OK, RS_REPLY)、
  FreshInitDone→service 回复 + end_srv_init，恒 EDONTREPLY）；`init_response` 升级为
  活包装（EDONTREPLY→OK 归一，R3；lu_response 仍 16-gated——其 shell 需要 LU 链上下文）；
  `catch_boot_init_ready` 实装（main.c:789-830：阻塞接收 + 三个 C panic 原文 + VM 免回复
  + INITIALIZING 清除），step2 SYNCH_BOOT 分支与 step3 循环全部转真——T6 的两处
  fail-closed 占位自此消灭。
- **测试基建**：union 字段写入是安全位存储——RS 测试可直接构造 RS_INIT 信封（读取
  一律走访问器）。新增 7 测：accessor（minix-types）、step3 catch/错型 panic/失败
  panic、fresh done、init 失败 crash + init_err、Unexpected EINVAL、EDONTREPLY 归一。
- **Verified**：`cargo test -p minix-rs` = **293 passed**（+7）；minix-types 171 passed；
  clippy 触碰 crate 零告警；fmt 干净；T7 PASS。测试自查修正 3 处夹具缺端点索引
  （endpoint_slot 查找路径）与 1 处计数假设错误（placeholder 全表 uncaught=10 非 1）。

### ✅ Fix #71 — 13 号 RS_DOWN 臂接线（载荷缝 safecopy_from + MessRsReq 解码 + stop 流程贯通）
- **File**：`os/libs/minix-types/src/ipc/message.rs`（`MessRsReq` repr(C) 56 字节 +
  `m_rs_req` union 臂 + `rs_req_payload()` 访问器 + 测试）、`boot.rs`
  （`KernelApi::safecopy_from` 缝，生产 ENOSYS + doc contract）、`testutil.rs`
  （`payload` 罐装字节）、`lib.rs`（`do_down` + 集成测试）；文档 13
- **Before**：`do_down`（request.c:110-146）零载体——其唯一载荷是 16 字节 label
  （`copy_label`，纯字节无结构 ABI），但因 `m_rs_req.addr/len` 的 union 读取
  （`MessRsReq` 在 minix-types 缺席）与 `sys_datacopy` 缝一起挂账。
- **After**：`MessRsReq` 按 ipc.h:1886-1895 逐字段（x86-64：len/name_len/endpoint/
  addr/name/subtype + 填充 = 56 字节）；`rs_req_payload()` 三调用号标签守卫；
  `KernelApi::safecopy_from`（sys_datacopy 缝——E-11 的数据拷贝面以 seam 形态先行，
  生产 ENOSYS、mock 罐装字节）；`do_down` 全流程：copy_label 解码 → `lookup_by_label`
  ESRCH → 权限门 → TERMINATED 分支（unpublish 聚合决策 + cleanup 两相）→
  stop_decision（EXITING + stop_tm）+ LATEREPLY/caller/caller_request + StopSignal
  经 PM 面 srv_kill（RS 自身 SIGHUP/其余 SIGTERM）→ 恒 EDONTREPLY（迟回复由
  cleanup 路径发出）。
- **Verified**：`cargo test -p minix-rs` = **294 passed**（+1 集成测试：label 解码→
  权限→stop 流程→LATEREPLY 簿记全链；自查修正 2 处——unpublish_result 是 4-bool
  聚合决策面非效果执行体、测试夹具缺 SYS_PROC 被 request.c:104-105 权限门拦下）；
  clippy 触碰 crate 零告警；fmt 干净；T7 PASS。

### ✅ Fix #72 — I1 死代码裁决（§18.4 第一档 + OQ-3 关单）+ awaiting-wiring 标注机制
- **File**：`query.rs`（删 6 个常量副本 + `use minix_types::sysctl` + 删重复测试）、
  `process_table.rs`（删 `set_endpoint_mapping`）、`service_create.rs`（clone_slot 借用修正
  + 新增 ptr_eq 测试）、`sched.rs`/`ready.rs`/`exec.rs`/`self_lifecycle.rs`（8 处
  `awaiting-wiring: NN` doc 标注）；文档 14-rs-query-requests.md（§头部清单/§3.1 表/§3.3
  常量归属重写）+ 本 todo §18.4/§18.6
- **Before**：§18.4 首轮列出的第一档 2 项死代码未删；OQ-3 悬而未决；第三档清单在
  Fix #53-#71 落地后大面积过时（7 个"零调用"项已有生产消费）；已修复 20 处
  （query.rs 的 RS_FI_CRASH + set_endpoint_mapping）。
- **After**：(1) 常量单一权威——处置中发现首轮 scan 漏报同型副本 5 个（`RS_SYSCTL_*`），
  6 个一并删除、`classify_sysctl` 消费 `minix_types::sysctl`；(2) 删
  `set_endpoint_mapping`（被 `set_endpoint_index` 完全取代）；(3) OQ-3 判定闭合——
  保留 `share_exec` 权威、clone_slot 路由改直接借用（消除裁决实现引入的整槽深拷贝）、
  新增 `test_clone_slot_use_copy_shares_exec_image`；(4) 第三档消费状态对账刷新 +
  8 处 `awaiting-wiring: NN-rs-xxx.md` 标注（sched×2/ready×3/exec×2/self_lifecycle×1）。
- **Verified**：`cargo test -p minix-rs` = **294 passed**（-1 删重复测试 +1 新增 ptr_eq
  测试，数量守恒）；`rg "RS_FI_CRASH|RS_SYSCTL_SRV" os/servers/rs/src/query.rs` = 0；
  `rg "set_endpoint_mapping" os/servers/rs/src/` = 0；clippy 零告警；fmt 干净；T7 PASS。

### ✅ Fix #73 — I2 boot Step 2 `init_service` 接线（RS 早退/VM asynsend/三时间戳 + asynsend 缝）
- **File**：`boot.rs`（`IpcApi` 增 `asynsend` 缝 + `step2_allow_run` 三分支接
  `service_create::init_service` + 删 boot.rs:823 陈旧 DEFERRED + 新增 2 集成测试）、
  `ready.rs`（`InitMessage::encode_message` 线面编码）、`testutil.rs`（`Call::Asynsend`
  + `sent` 记录 + `kernel_privs` 内核侧特权表）；minix-types `ipc/rs.rs`
  （`RsInit::encode_message` + roundtrip 测试）；文档 01 §4.4/§5.2/§5.4、12 §3.1
- **Before**：boot Step 2 对 RS/VM 只计数不发消息、对普通 SYS_PROC 服务不调
  `init_service`（boot.rs:823 陈旧 DEFERRED"init_service wiring lands with 12"——12 号
  receive 侧 Fix #70 落地后该前提已解除但 send 侧没人做）。后果：真实 VM 永远等不到
  RS_INIT，step3 阻塞接收死等；所有槽缺 `INITIALIZING`/`alive_tm`/`check_tm` 标记
  （utility.c:19-21），心跳基线从启动起就错。mock 测试掩盖（handover 测试用 RS-only 表）。
- **After**：step2 三分支统一走既有 `service_create::init_service`（C main.c:362-399）——
  RS 分支 `ROOT_SYS_PROC` 早退零发送（utility.c:29-31），VM 分支发 `RS_INIT` + 计数，
  SYS_PROC 分支 sched/allow 后发 `RS_INIT` + SYNCH_BOOT 同步 catch 或计数。缝形态：
  `IpcApi::asynsend`（生产 ENOSYS fail-closed + E-11 doc contract，mock 记录全消息）；
  编码链 `InitMessage::encode_message` → `RsInit::encode_message`（哨兵：gid None →
  `GRANT_INVALID` -1 safecopies.h:52、old None → `NONE` utility.c:39-44）。
  **附带修正 mock 保真度**：真实内核对 RS/VM 自有启动特权副本（main.c:293-296
  `sys_getpriv` 对 RS/VM 也成功），mock 增 `kernel_privs` 表使 step1 的 getpriv 同步
  不再返回空结构——该缺口由本轮 RS 早退测试首跑即抓出。
- **Verified**：`cargo test -p minix-rs` = **297 passed**（+3：
  `test_step2_init_service_marks_and_sends` 逐字段断言发送载荷/标记/计数、
  `test_step3_vm_init_roundtrip_clears_initializing` step2 发→inbox 脚本→step3 收全链
  + VM 零同步 reply、`test_init_message_encode_message` 哨兵往返；minix-types
  +1 encode roundtrip）；clippy 零告警；fmt 干净；T7 PASS（asynsend 生产 impl 走
  Err(ENOSYS) 不触发 panic 标记门，E-11 doc contract 注释在位）。

### ✅ Fix #74 — I3a 14 号 LOOKUP/FI 两臂接线 + reply 缝载荷忠实化
- **File**：`lib.rs`（`do_lookup`/`do_fi` handler + `reply_unless_suppressed` 载荷化
  + `do_request` 转发 + 2 接线测试）、`boot.rs`（`IpcApi::reply` 增 payload 参 +
  catch_boot_init_ready 回发 RS_INIT 回显）、`recovery.rs`/`live_update.rs`/
  `service_create.rs`（late reply 点补 fresh 默认载荷，utility.c:332-349）、
  `testutil.rs`（mock reply 记录 `replies` 载荷表）；minix-types
  `message.rs`（`rs_req_name`/`set_rs_req_endpoint`/`rs_req_endpoint` +
  `MessLsysFiCtl` 臂 + `is_rs_req_arm` 全族守卫）、`ipc/rs.rs`
  （`COMMON_REQ_FI_CTL` com.h:607 + `LsysFiCtl` 编解码 + roundtrip 测试）；文档 14 号
- **Before**：RS_LOOKUP/RS_FI 落在 dispatch 死表返回 ENOSYS；`IpcApi::reply(target,
  result)` 丢载荷——C 的 reply 语义是"把 handler 原位变异过的请求消息整个回发"
  （utility.c:318-345），RS_LOOKUP 的端点结果正住在 `m_rs_req.endpoint`
  （request.c:1174），旧缝形对此不可表达。
- **After**：reply 缝忠实化为 `(target, result, payload)`（C late_reply 的 fresh 消息
  语义以 `Message::default()` 载荷表达）；`m_rs_req` 臂守卫从三调用号扩为全族
  （INIT/LU_PREPARE 除外——C ipc.h union 归属事实）；`do_lookup` 走
  name/name_len 门→拷贝→查槽→原位写端点；`do_fi` 走 addr/len 拷标签→查槽→
  `check_call_permission(RS_FI)`→`asynsend(LsysFiCtl{RS_FI_CRASH})`。clippy 教正：
  union 字段写入本即安全，encode 侧去掉多余 unsafe 块并改结构化初始化。
- **Verified**：`cargo test -p minix-rs` = **299 passed**（+2：
  `test_do_lookup_resolves_label_into_reply_payload` 含载荷端点断言/ESRCH/EINVAL
  三分支、`test_do_fi_injects_crash_request` 含 ESRCH 分支）；`cargo test -p
  minix-types` = **175 passed**（+1 `test_lsys_fi_ctl_encode_roundtrip`）；
  clippy 触碰 crate 零告警；fmt 干净；T7 PASS。RS_GETSYSINFO/RS_SYSCTL 两臂
  留 I3b（前者拷出半压 edge E-RSWIRE 字节 ABI，后者 UPD_* 编排在 16 号链上）。

### ✅ Fix #75 — I3b 14 号 GETSYSINFO/SYSCTL 两臂接线（14 号死表清零）
- **File**：`lib.rs`（`do_getsysinfo`/`do_sysctl` handler + 2 接线测试）、
  minix-types `message.rs`（`MessLsysGetsysinfo` 臂 + `getsysinfo_req`/
  `rs_req_subtype` 访问器）；文档 14 号 §4.1/§5
- **Before**：RS_GETSYSINFO/RS_SYSCTL 落在 dispatch 死表；`classify_sysctl`
  与 `start_update_prepare`/`abort_action`/`clear_upds`/`end_update` 决策与编排
  在 crate 内就绪却无调用方（A4 死表 + 18.4 第三档"预期零调用"）。
- **After**：`do_getsysinfo` = 权限门（request.c:1099 caller-only）+ `SI_*` 分类
  live，拷出半（尺寸门 request.c:1120-1136 + 表拷贝）压 edge E-RSWIRE 字节 ABI，
  在缝上 ENOSYS fail-closed（不假成功）。`do_sysctl` = 分类分派：打印臂 OK
  （dump 面归 IS，§18.10 E-8）；`UPD_START` → `start_update_prepare`（is_idle 用
  `RFlags::is_idle` 全表扫描，utility.c:424-437；allow_retries=true）+ OK；
  `UPD_RUN` → 链尾 `mark_late_reply`(caller, RS_UPDATE) + EDONTREPLY
  （request.c:1202-1207）；ESRCH→OK 归一（request.c:1194-1198）；`UPD_STOP` →
  `abort_action` 相位分派：Scheduled→`clear_upds`、Initializing/Updating→
  `end_update(EINTR, RS_REPLY/RS_CANCEL)`、Idle→EINVAL（update.c:707-743）。
- **Verified**：`cargo test -p minix-rs` = **301 passed**（+2：
  `test_do_getsysinfo_permission_and_classification`（分类 EINVAL + E-RSWIRE 门
  ENOSYS）、`test_do_sysctl_dispatch_and_update_arms`（EINVAL/打印 OK/UPD_STOP
  清链/UPD_RUN LATEREPLY+EDONTREPLY/已 UPDATING 再 prepare EINVAL））；clippy
  触碰文件零告警；fmt 干净；T7 PASS。14 号四臂至此全部脱离 dispatch 死表。

### ✅ Fix #76 — I4 13 号 label 型四臂接线（REFRESH/RESTART/CLONE/UNCLONE）+ E-RSSTART 登记
- **File**：`lib.rs`（`resolve_by_label`/`stop_with_late_reply` 共享前奏 +
  `do_refresh`/`do_restart`/`do_clone`/`do_unclone` + 3 接线测试）；文档
  13 号接线落地注记改写；**edge_todo.md 新增 E-RSSTART**（rs_start_t 字节 ABI）
- **Before**：四个 label 型控制臂落在 dispatch 死表；E-7 的 `do_edit` 序列与
  RS_UP 一样压在 `rs_start_t` 解码上。
- **After**：四臂统一走 `resolve_by_label`（copy_label→查槽→权限门，含 updating
  标志）+ 臂专属动作——`do_refresh`：`stop_with_late_reply`(REFRESHING) →
  EDONTREPLY（request.c:390-419）；`do_restart`：TERMINATED 门（EBUSY，
  request.c:184-188）+ script 存/清/重启/恢复（request.c:191-196）；`do_clone`：
  next_rp 已存在→EEXIST、SF_USE_REPL 置位 + `clone_service`(RST_SYS_PROC)、
  失败回滚旗标（request.c:231-243）；`do_unclone`：无旗标→ENOENT、清理旗标 +
  两遍 `cleanup_service`（=cleanup_service_now，proto.h:53-55）。**E-7/E-5 判定
  变更**：`do_edit`/`do_up` 的 `copy_rs_start` 压 `rs_start_t` 字节 ABI——
  `bitchunk_t`/`uid_t` 本树无定义（bitmap.h:12 引用无 typedef），偏移不可 pinning，
  登记 edge **E-RSSTART**（同 E-RSWIRE 判据），两臂在此期间保持死表 ENOSYS。
- **Verified**：`cargo test -p minix-rs` = **304 passed**（+3：
  `test_do_refresh_stops_and_arms_late_reply`（REFRESHING/stop_tm=LATEREPLY 三件）、
  `test_do_restart_requires_terminated_service`（EBUSY + TERMINATED 重启 + script
  往返）、`test_do_clone_and_unclone_replica_lifecycle`（ENOENT/clone/EEXIST/
  unclone 清理））；clippy 触碰文件零告警；fmt 干净；T7 PASS。测试自查修正 3 处
  夹具缺陷（mock ticks/child_endpoint 缺失、clone 前置条件缺 cmd——均测试侧
  建设不足，非产品缺陷）。

### ✅ Fix #77 — I5 16 号 RS_LU_PREPARE 臂接线（lu_response shell）+ RS_UPDATE E-RSSTART 判定 + 三孤儿裁决
- **File**：`lib.rs`（`do_upd_ready_shell` 编排 shell + `lu_response` 落地 +
  run() 的 LuPrepareReady 臂改走裸 shell + 1 接线测试）、`ready.rs`
  （删 should_reply_ready/normalize_init_response/normalize_lu_response 三孤儿
  + 其测试）、`dispatch.rs`（接线状态注记）、`monitor.rs`（upd_init_maxtime 读
  `ServiceSlot.upd` 描述符 + 2 处 DEFERRED 标记清除）、`service_create.rs`
  （swap_slot 链遍历归属注记）；minix-types `message.rs`（`MessRsUpdate` 臂）、
  `ipc/rs.rs`（`RsUpdate::decode_message`）；文档 16 号接线落地注记
- **Before**：`RS_LU_PREPARE` 臂的 `lu_response` 恒 ENOSYS（16-gated"缺链上下文"
  ——Fix #53 的 UpdateState 落地后前提已解除）；monitor 的 `upd_init_maxtime`
  忽略描述符（3 处 DEFERRED 注释指 16 号，前提均已解除）；ready.rs 三辅助按
  awaiting-wiring 注释的承诺"shell 收敛轮裁决采用或删除"。
- **After**：shell 按 C 顺序组合——链门（sender==curr 且非 INITIALIZING，
  request.c:903-910，gate 用 curr 在走链前捕获）→ PREPARE_DONE（R24）→
  PrepareFailed→`end_update(result, RS_REPLY)` / NextPrepare→EDONTREPLY /
  StartUpdate→`start_update`（8 回调缝按 19 号约定 noop）。**结构修正**：
  run() 的 LuPrepareReady 臂改走裸 shell（main.c:117 原始结果、EDONTREPLY 生效），
  EDONTREPLY→EGENERIC 归一是 sef_cb_lu_response 包装的职责（main.c:614-626）——
  此前两者混用会让主循环把"延迟回复"误报为 EGENERIC。三孤儿删除（签名与落地
  调用形不符，规则由 wrapper 自身测试承载）。**RS_UPDATE 臂判定**：C do_update
  首步即 `copy_rs_start`（request.c:542）——与 RS_UP/RS_EDIT 同压 E-RSSTART
  字节 ABI，保持死表 fail-closed。
- **Verified**：`cargo test -p minix-rs` = **302 passed**（+1
  `test_do_upd_ready_shell_gates_and_updates`（无链 EINVAL/错 sender EINVAL/
  StartUpdate PREPARE_DONE+EDONTREPLY/失败 end_update 清 UPDATING），
  -3 删除孤儿测试，净 -2）；`cargo test -p minix-types` = **175 passed**；
  clippy 触碰文件零告警；fmt 干净；T7 PASS。

### 18.10 EDGE 清单（§18 迭代收束——追加以避免与 19 号主线冲突，2026-09-06）

> 以下条目为**结构性重构 / 测试基建 / 生产接线边界**，属独立立项范围，本轮（§18
> 迭代）明确不在代码中实现，避免与后续主线冲突。每条附落地要点，立项时直接取用。

**E-1 Self-upgrade 进程面（原轮 20，boot.rs:774 unimplemented 标记保留）**
- 决策面已全就绪：`self_lifecycle.rs` 13 个纯切片 + `srv_update_action` +
  `is_rs_restart_replica` + `sig_mgr_updates`（均 250+ 测试锁定）。
- 缺口 = 进程创建/身份交换的真实面：`srv_fork` 后 RS 自身继续运行的同时驱动
  `update_service(RS_DONTSWAP)` + `init_service` + YIELD 链——该链每步都压在
  19 的进程/IPC 缝上。立项=18 号文档的接线轮。OQ-1 决策：保持纯切片形态。

**E-2 R9 KernelApi 拆 trait（boot.rs 单体 13 方法 → SysApi/PmApi/VmApi/SchedApi）**
- ✅ **已修（Fix #60，2026-09-06）**：22 方法拆为五域面 `SysApi`（内核 8）/`SchedApi`（调度器 2）/
  `PmApi`（PM 进程生命周期 8）/`VmApi`（VM 2）/`IpcApi`（RS 自身 IPC 2），`KernelApi` 为
  supertrait 组合（blanket impl）——调用点零改动、mock 按 5 域分块、19 接线可按面逐个实现传输。
  对 R9 原草案的显式修正：`privctl`/`getpriv` 归 `SysApi`（C 面就是 sys_* 内核调用，单拆
  PrivApi 无接线增量），`sched_init_proc`/`sched_stop` 独立成 `SchedApi`（C 传输目标复合：
  KERNEL→sys_schedctl，否则 SCHEDULING_* 消息，sched_start.c:46-88/sched_stop.c:9-28）。
  顺带修正 `srv_execve` 注释的 C 机制误述（RS 内 ELF 装载 + 内核分配/拷贝 + PM 终步，
  exec.c:21-64/:102/:127——非"打包 argv 进消息"）。
- ~~现有 19 个方法（含 R22a/R22b/R23 增补的 receive/waitpid/sys_update/srv_execve/
  srv_kill/sched_stop/setuid/reply）按域归位；预支 5 方法归位（boot.rs:81-114）。~~（已完成）
- ~~拆分须一次完成：所有 impl（Unimplemented/Mock/未来生产）同步迁移。~~（已完成：Unimplemented
  5 块 + Mock 5 块；未来生产 impl 在 19 按面落地）
- ~~依赖：无硬依赖，但建议在 19 接线前做，避免接线后双倍迁移。~~（已在 19 前完成）

**E-3 D2/R8 SlotId 世代计数**
- ❌→✅ **判定闭合（2026-09-07，Fix #67）：世代计数不采纳；改为落地 R7 提议的
  `RProcTable::assert_consistent()` settled-state 不变式校验器**。理由三点：
  (1) **R8 的前提是错的**——C `alloc_slot`（manager.c:2067-2083）扫描第一个非
  `RS_IN_USE` 行，**C 确实复用已释放行**；free_slot 清行后 alloc 就地重新发放
  （02 §2.3 已按此实现）。"C 行从不复用、Rust vacant() 复用是设计选择"不成立，
  复用语义是忠实的，悬垂风险是 C 语义自身的性质而非 Rust 引入的偏差。
  (2) **Fix #43 的教训直接适用**：对内部链/索引做世代校验会拒绝 C 会跟的悬垂链
  （swap 的第三方原始索引项、clone 的 in-use+NONE 中间态都是被测试锁定的忠实
  语义）——把 C 没有的"智能句柄"发明出来，正是当初被证伪的 translate 式全局化。
  (3) RS 单线程且流程全测；"放大后果"的担忧由校验器直接断言不变量来兜底，
  结构性根治归 E-4（god struct 拆分后链接所有权收进表内部，悬垂句柄在构造上
  不可表达）。
- **落地内容**：`RProcTable::assert_consistent()`（debug-only，三条不变量：
  in_use 双标志同步 / in-use 行端点索引双向往返 / 四链界内）；黄金路径测试接线
  （create happy path、两相 cleanup、boot handover）；正/负两测锁定校验器自身
  （负测抓"标了 in-use 没写索引"的自由→reuse 悬垂形态——首跑即抓出 cleanup
  夹具漏写 `pub_.in_use` 的真实不同步并修正）。**附带卫生**：清除 R22a 轮遗留的
  4 处 `eprintln!` 调试输出（recovery.rs 2、service_create.rs 2）。

**E-4 D1/R33/R7 god struct 收敛**
- ✅ **判定闭合（2026-09-07，Fix #68）：45 字段按域拆分不采纳；R33 已修（删除
  `ServiceSlot` 的 `PartialEq`/`Eq` derive）；R7 链断言已随 Fix #67 落地
  （`assert_consistent`）；D6 已随 Fix #62 以派生快照收敛**。拆分不采纳的理由：
  (1) D1 的原始分析自己承认"镜像 C 是文档可追溯性的需要"——逐字段 C 锚点是
  A-3/02 文档的既定契约，按域拆子结构会打断 1:1 字段映射并迫使全部字段访问点
  （~500 处）与全部文档表重写；(2) 全 crate 零处整槽 `==`（grep 证实），god struct
  的实际缺陷只有 R33 的深比较脚枪，删除 derive 即根除；(3) 封装收益（私有字段
  强制不变式）的真实触发条件是 19 接线后出现跨模块误写——届时按痛点立项，
  不预先支付迁移成本。**OQ 上交**：若用户裁决"拆分必须做"，按身份/策略/监控/LU
  四组方案独立立项（预计 2-3 个迭代）。

**E-5 D6 io/irq 双表收敛**
- ✅ **已修（Fix #62，2026-09-06）**：C ground truth 先行核实——备份表（type.h:99
  "Backup values from the privilege structure"）全树**零读者**（写点仅 manager.c:1498 双赋值
  与 :1519 拷回；`r_priv` 的刷新走 sys_getpriv 内核回读，与备份无关）。收敛形态 =
  `Privilege` 为单一权威（内核强制面镜像）+ `ServiceSlot::refresh_priv_backup()` 从自身
  `r_priv` 一次派生（edit_slot 删除独立备份写语句，对应 C 写序）；02 文档两处"备份重建
  r_priv"的错误声称（P2-fact）一并修正，03 文档加权威声明。
- ~~注意 C 的 limit/len 两种形态（IoRange 已按 base+len 统一，N9/Fix #39）。~~（此前已收敛）

**E-6 T3 BootError 区分**
- ✅ **已修（Fix #61，2026-09-06）**：`BootError` 枚举贯穿 `BootInit`（validate_tables/
  step0-4/init_fresh 全链 `Result<(), BootError>`）——变体 `Lookup(LookupError)`/
  `CountMismatch`/`EndpointMismatch`(R17)/`InvalidPid(Pid)`/`Kernel(Errno)`；`From<Errno>`
  使内核调用 `?` 自动归入 `Kernel`（T6 两处 fail-closed 即 `Kernel(ENOSYS)`），线面映射
  `From<BootError> for Errno`：`Kernel(e)` 保留原 errno、不变式违例 → `EINVAL`（`ENOSYS`
  从此专属"未接线"）。SEF 回调面保持 C `int` errno 契约，类型化原因存
  `RsServer::boot_diagnostic()`，main.rs 致命启动报告打印。01 文档 §3.4 后补 E-6 注记 +
  §5 测试表同步，测试 +3（validate 两测升为变体断言、负 pid、线面映射）。原设计草案的
  变体名 `LookupFailed`/`KernelCallFailed(e)` 落地为 `Lookup(LookupError)`/`Kernel(Errno)`
  ——前者复用既有 `LookupError` 载荷（携带 ImageTable/PrivTable 区分），后者与
  `From<Errno>` 的自动传播配合。
- ~~影响面：boot.rs 错误路径 + 01 号文档 §5 + 调用方（main.rs/main.rs 测试）。~~（已完成）

**E-7 R10 调用顺序类型化**
- 🔄 **gated（13/06 号接线轮）**：剩余项全部压在未落地的编排上——
  `start_update` 链式调用的 abort/end 交错（abort 钩子内嵌时序已随 R27/Fix #56
  修正注释与实现）、`do_edit` 的 sched_stop→edit_slot→privctl→sched_init 序列
  （13 号落地时一并）。
- 形态：类型化步骤 token（R10 提案）或编排内 debug_assert 序列锚（E-3 的
  `assert_consistent` 是同思路的先例——不变量断言优先于类型机器）。

**E-8 R31 诊断面**
- ✅ **错误名面已修（Fix #66，2026-09-07）**：`minix_types::Errno` 落地
  `name() -> Option<&'static str>` + `Display`（115 常量全量，match 臂用常量本身——
  改名/删值编译期即断；`ELAST` 与 `EPROTO` 同值 96，首常量名胜出，重复臂删除）；
  RS 侧新增 `error.rs`——`init_strerror`/`lu_strerror`（error.c:48/:56 的两张上下文
  描述表）组合在 name 之上，未命中回退 C strerror 语义（error.c:44），error.c 从
  覆盖度工具的"完全缺口"清单中闭合；消费面是 19 的 diagctl 诊断缝。
- **剩余（归属不变）**：`srv_to_string_gen`/`srv_upd_to_string`/
  `print_services_status`/`print_update_status`（utility.c:142-546）归
  IS 阶段（08-stage-is）dump 面；`exec_restart` 归 19。

**E-9 E2 proptest**
- ✅ **已修（Fix #64，2026-09-06）**：proptest 在本工作区不可用（无 registry 访问，Cargo.lock
  无缓存）——改为**零依赖 property 测试**：`testutil::XorShift`（xorshift64*，固定种子可复现）
  + 每函数显式不变式，覆盖 E-9 清单 6 个目标（parse_filter_el 经 parse_label/常量臂间接受
  覆盖）：`test_build_cmd_dep_properties`（token 分词/S1 NUL 停止/N11 argv0/确定性）、
  `test_rebuild_args_properties`（R15 argc↔缓冲自洽走链 + 尾部归零）、
  `test_parse_label_properties`（totality/Ok 臂归因/十进制值/ds_lookup 转发）、
  `test_ipc_list_iterator_properties`（对照参照分词器的差分性质 + fused）、
  `test_call_mask_from_calls_properties`（base 保留/NULL_C 终止/越界 EINVAL/ALL_C 全掩码）、
  `test_compute_backoff_properties`（totality/界/单调/no_bin_exp/use_copy）。**测试自查抓出
  3 处测试自身错误**（rebuild 借用、NULL_C 之后的越界不可达、tot=64 移位溢出——均为测试
  侧理解偏差，非产品缺陷）。**记录不修**：假想 tot>64 时 offsets∈[64,tot) 会触发 set_bit
  全构建 assert（fail-fast）；真实调用点 tot 全为编译期常量且 < 64，不可达，不扩大 N7 改动。

**E-10 E3 集成测试 + R34.18-23**
- 🔄 **部分已修（Fix #65，2026-09-07）**：R34.18（boot 失败传播族——`testutil::MockKernelApi`
  增 `fail_calls` 变体级失败注入 + step0 get_machine/step1 lookup/step4 setalarm 三条传播
  测试；负 pid 已随 Fix #61）、R34.19（run() 无 boot panic + 二次 init(Fresh) panic——
  `#[should_panic]` 锁定 fail-fast 契约）、R34.21（信号分发层——CHLD 实排空：
  waitpid 交出 pid → 槽位释放 + 端点索引消失 + 未知 pid 无副作用；TERM/空排空/未知信号
  已随 Fix #57）落地。
- **剩余（gated）**：R34.20 已随 06 接线轮闭合（Fix #69）；R34.23 已随 12 接线轮
  闭合（Fix #70）；R34.22（signal_manager 六分支——待 18 编排落地）；do_period
  全流程与 rollback 心跳扫的集成面已由 Fix #41/#55/#56 的走链测试与本轮 run()
  集成测试覆盖。
- 场景：boot 全链 mock（四步 + getnpid + setalarm 断言）、do_period 全流程
  （update_period 派发门 → 槽门 → 决策 → setalarm）、rollback→心跳重发扫
  （R27(a) 的集成面）、signal_manager 六分支（待编排落地后）、
  catch_boot_init_ready 三 panic 分支。
- R34 清单对账：1-13 已随各实现轮补齐；14-17 随 13/14 号接线轮；
  18-23 由本条覆盖；24（IPC_ALL 位域契约）随 E-9。

**E-11 生产接线面（19 号主线，恒为边界）**
- `minix-sys::receive`/`send`/`asynsend`/`sys_datacopy`/`cpf_revoke`/
  `cpf_grant_direct`/`sys_privctl`/`sys_getpriv`/`sys_getmachine`/`sys_getinfo`/
  `sys_setalarm`/`sys_kill`/`sys_update`/`sys_diagctl_stacktrace`/`vm_memctl`/
  `vm_set_priv`/`vm_prepare`/`getnpid`/`getnuid`/`waitpid`/`setuid`/
  `read_exec`（文件 I/O）/`ds_publish_label`/`run_script` 的 fork+execle/
  `env_parse` 配置面/`srv_execve` 真实 exec。
- 以上在 §18 迭代中全部保持 ENOSYS fail-closed + doc-contract（T7 门 PASS）；
  接线即 19-rs-external-interfaces.md 主线。

---

## 18.11 收敛 campaign 状态（2026-09-07——§18.9 迭代协议的批量执行，cmd-08 变体 A 队列）

> 依 2026-09-07 批准的收敛计划执行：每轮一个 TODO（讲明白 → ≥2 方案对比 → 实现 →
> 测试 → 文档同步 → 全门验证 → 回归 review → commit），边界判定遵循 edge_todo.md
> 的三类规则 + Fix #70/#71 先例（仅本 stage 消费的消息 union 视图随轮提交；真实
> 传输恒为 edge E9）。

### 已完成轮次（Fix #72-#77，见 §18.9 逐条记录）

| 轮 | 内容 | 关单 | 基线 |
|----|------|------|------|
| I1 | 死代码裁决：6 常量副本收敛 minix-types 单一权威 + `set_endpoint_mapping` 删除 + OQ-3 关单（share_exec 单一权威 + 整槽拷贝消除）+ awaiting-wiring 标注机制（8 处） | OQ-3、§18.4 全档 | 294 |
| I2 | boot Step 2 `init_service` 接线：RS 早退/VM asynsend/三时间戳；`IpcApi::asynsend` 缝先行；mock 保真度修正（kernel_privs） | boot.rs:823 DEFERRED | 297 |
| I3a | 14 号 LOOKUP/FI 两臂 + reply 缝载荷忠实化（`IpcApi::reply` 增 payload——C reply 语义是回发变异后的整条消息） | R34.14 部分 | 299 |
| I3b | 14 号 GETSYSINFO/SYSCTL 两臂（拷出半压 E-RSWIRE 缝上 fail-closed；UPD_* 消费 LU 编排）——**14 号死表清零** | R34.14/15 | 301 |
| I4 | 13 号 label 型四臂（REFRESH/RESTART/CLONE/UNCLONE）+ `resolve_by_label`/`stop_with_late_reply` 共享前奏 | §2.3/§2.4/§2.6 臂 | 304 |
| I5 | 16 号 RS_LU_PREPARE 臂（`do_upd_ready_shell`：链门/PREPARE_DONE/三路派发）+ run() 臂走裸 shell 的结构修正 + 三孤儿裁决删除 + monitor 描述符消费 | monitor/service_create 3 处陈旧 DEFERRED | 302 |
| I6 | **R34.22 闭合**：signal_manager 七分支 + `terminate_service` 执行体（决策驱动，manager.c:1055-1166 全分支）+ `rs_idle_period` + `reincarnate_service`/`get_service_instances` 原语 + `abort_update_proc` 组合助手收敛 + `SysApi::diagctl_stacktrace`/`SIGS_SIGNAL_RECEIVED` 缝 | R34.22、§1 R34 表 | 304 |

### 边界判定变更（本轮 campaign 新登记 edge）

- **E-RSSTART**（edge_todo.md）：`rs_start_t` 字节 ABI pinning——`RS_UP`/`RS_EDIT`/
  `RS_UPDATE` 三臂首步都是 `copy_rs_start`（request.c:37/:306/:542），而
  `bitchunk_t`（rss_system/rss_vm 位图数组）/`uid_t` 在本 C 树无 typedef，偏移不可
  pinning（同 E-RSWIRE 判据）。三臂在 dispatch 死表保持 fail-closed；编排侧
  （create_service/edit_slot/run_service/E-7 序列）已全部就绪，解码落地即接线。

### 剩余队列（stage 内，按序执行）

| 队列 | 内容 | 依赖 | 锚点 |
|------|------|------|------|
| ✅ I6 | **R34.22 闭合**（2026-09-07，Fix #78，见 §18.9） | — | C main.c:647-703、manager.c:1055-1166、utility.c:441-478 |
| ✅ I7 | 终审收束（2026-09-07）：§1/§18.6 翻态完成；SYMBOLS 复测 **171 C 符号 / 文档覆盖 99.4% / Rust name-match 77.2%**（余量为 E-RSSTART/E9 门上的线面名，非缺口）；测试对账 minix-rs 304 + minix-types 175 全绿；Rule Discovery 终答见下 | 无 | — |

**Step 5.7 Rule Discovery（campaign 终答）**：本轮发现一个可复用模式——**「copy_* 结构 ABI 门」**：微内核服务的控制臂若以"整结构拷贝"（copy_rs_start/copy 訊息族）开场，其接线次序应先于解码轮做**结构完整性审计**（grep 结构体内全部 typedef 是否在本树有定义），凡有缺失即登记 edge 并把该臂保持死表 fail-closed——避免"编排就绪却卡在解码"的半成品接线轮（本 campaign 的 I4/I5 就因提前发现 E-RSSTART 而免于两次返工）。建议沉淀入 review-patterns（模式 84 候选：CACG, Copy-struct ABI Gate）。

### 剩余非本 stage 项（维持归属，无动作）

- **E-1/E-11/E9**：生产接线（trap 层 E1、SYS_* wrapper、PM/VM 对端）——edge_todo.md E9。
- **R31 残余**：`srv_to_string_gen`/`srv_upd_to_string`/`print_services_status`/
  `print_update_status` 归 08-stage-is dump 面；`exec_restart` 归 19（§18.10 E-8）。
- **E-RSSTART/E-RSWIRE**：字节 ABI pinning（需完整 Minix3 源码参照）。

### 验证基线（最新时点，I6 后）

`cargo test -p minix-rs` = **304 passed / 0 failed**；`cargo test -p minix-types` =
**175 passed**；clippy 触碰文件零告警；fmt 干净；T7 门（`tools/check-rs-unwired.sh`）
PASS。commit 轨迹：I1（af23e523a 捎带后经 7d7297d2b 重收录）/ I2（b73ef058e）/
I3a（3a10b74a6）/ I3b（29eff3b90）/ I4（7d7297d2b）/ I5（f11141121）/ I7'（da066c338）/
I6（见 Fix #78）。

### ✅ Fix #78 — I6 signal_manager 七分支接线（R34.22 闭合）+ terminate_service 执行体
- **File**：`recovery.rs`（`terminate_service` 决策驱动执行体 + `reincarnate_service`/
  `get_service_instances` 原语 + `rs_idle_period` + `sigs_is_*` 分类）、
  `live_update.rs`（`abort_update_proc` 组合助手——do_sysctl UPD_STOP 内联派发收敛）、
  `boot.rs`（`SysApi::diagctl_stacktrace` 缝，生产 ENOSYS + E-11 doc contract +
  SysOnly 测试双同步）、`testutil.rs`（`Call::DiagctlStacktrace` 记录）、`lib.rs`
  （`signal_manager` 七分支 + 2 接线测试）；minix-types `ipc/rs.rs`
  （`SIGS_SIGNAL_RECEIVED` com.h:597）；文档 06/15 号接线落地注记 + §18.11 队列表
- **Before**：`signal_manager` 恒 `Err(ENOSYS)`（lib.rs:1351，"DEFERRED until 06
  lands"——06 已落地但 15 的 terminate 执行体未建）；`terminate_decision` 决策树
  无生产执行方（§18.4 第三档"18/19 号预支"）；`rs_idle_period`/
  `reincarnate_service`/`get_service_instances` 三个 utility 编排在 Rust 缺席。
- **After**：signal_manager 按 C 顺序七分支 live——spurious 清除（OK）/
  TERMINATED→EDEADEPT / inactive 清除 / stacktrace→diagctl 缝 / 终止→TERMINATED
  置位 + `terminate_service` + `rs_idle_period` + EDEADEPT / VM 免转发 /
  非终止→`asynsend(SIGS_SIGNAL_RECEIVED)`。执行体纯决策驱动：`d.mutations.apply`
  先行（init 失败旗标/norestart 武装/reincarnate 清位全在决策载荷），再按
  `TerminateAction` 执行效果（rollback 早退/refresh restart/backoff 写回/
  cleanup 族含 late-reply 归一、unpublish 缝、实例族清理、reincarnate；
  core_fatal+非 shutdown→self_terminate）。测试抓出两处认知修正：SIGTERM(15)
  不在 C `SIGS_IS_TERMINATION`（lethal 4/6/7/8/9/10/11 + PIPE 13）——终止臂
  测试改用 SIGKILL(9)；Backoff 分支保留槽位使"二次终止→EDEADEPT"可达。
- **Verified**：`cargo test -p minix-rs` = **304 passed**（+2：
  `test_signal_manager_routes_all_branches`（七分支主体）、
  `test_signal_manager_vm_and_stacktrace`（VM 免转发 + 终止优先））；clippy 触碰
  文件零告警；fmt 干净；T7 PASS。

## 19. 收敛终态（2026-09-07 campaign 收官快照）

- **stage 内 TODO 全部消化**：§1 T/D/S/E 系列全 ✅；§14-§18 各轮 R 系列全 ✅；
  §18.4 死代码清单全处置；OQ-1/-2/-3/-4 全关单；R34 盲区 24 条中 1-13/18-23 闭合。
- **遗留（全部有主，非 stage 内可做）**：
  1. **E-RSSTART**（edge）：`rs_start_t` 字节 ABI → `RS_UP`/`RS_EDIT`/`RS_UPDATE`
     三臂接线（编排已就绪）；
  2. **E9**（edge）：生产传输（E1 trap 层 + SYS_* wrapper）→ 19 号主线通电；
  3. **R31 残余**：打印/dump 面归 08-stage-is；`exec_restart` 归 19；
  4. **E-1**：RS 自升级进程面（压 E9 的 srv_fork 缝）。
- **测试基线**：`cargo test -p minix-rs` = **304 passed**；minix-types = **175
  passed**；campaign 净增 +93 测试（211→304）与 8 个生产接线轮（06 主循环、
  boot init_service、13 号六臂、14 号四臂、16 号 LU_PREPARE、signal_manager）。

---

## 20. 全面扫描轮（2026-09-07 —— cmd-04 + cmd-20 合体：查漏补缺 + 架构级审视，scan-only）

> **定位**：收敛 campaign（§18.9 Fix #72-#78）之后的独立扫描轮。此前已有 8 轮 review
> 全部收敛，本轮的增量价值在两处：①覆盖率驱动查漏——SYMBOLS 复测的 40 个 ⚠️ 项
> 逐项语义判定（name-match 工具无法区分"假缺口"与真缺口）；②campaign 新增约
> 1500 行（13/14/16 号活臂 shell、terminate_service 执行体、signal_manager、boot
> init_service）的从零复审。**scan-only**：全部发现为建议条目，修复走后续 todo-fix。
> **硬阻断预检**：.design 快照 63 个全在，design-coverage-check 全 PASS。
> **基线**：minix-rs 304 / minix-types 175 passed，clippy/fmt/T7 全绿。

### 20.1 覆盖率判定表（SYMBOLS 40 项 ⚠️ + 1 项有Rust无文档，逐项定谳）

> 判定命令可复核：每条的 Rust 等价物均可用 `rg` 锚定。结论：**函数级零真缺口**。

**A. 名称不匹配（Rust 等价物已存在，semantic-map 可回填）——21 项**

| C 符号 | Rust 等价物（锚点） | 判定依据 |
|--------|---------------------|----------|
| `copy_label` | `RsServer::resolve_by_label` + `IpcApi::safecopy_from`（lib.rs） | I4；16 字节 label 经 safecopy 缝拷入 |
| `copy_rs_start` | E-RSSTART 门（解码待 `RsStartWire`） | 本轮新登记 edge |
| `kill_service_debug`/`crash_service_debug`/`detach_service_debug` | `kill_service`/`crash_service`/`detach_service`（recovery.rs） | `_debug` 是 C 的 file/line 宏包装（proto.h:44-59） |
| `do_edit` | E-RSSTART 门（dispatch 死表 fail-closed） | 首步 copy_rs_start |
| `rupdate_clear_upds` | `UpdateState::clear_upds` + `chain.clear_upds` | I3b 消费 |
| `rupdate_set_new_upd_flags` | `chain.set_new_upd_flags`（live_update.rs:932） | R23a 落地 |
| `rupdate_upd_clear` | `chain.clear_upds` 内联逐条（live_update.rs:965 锚 update.c:136-156） | 内联实现 |
| `rupdate_upd_move` | `chain.upd_move`（live_update.rs:1016） | R23 落地 |
| `end_update_curr`/`_before_prepare`/`_prepare_done`/`_initializing` | `UpdateState::end_update` 单体内联四分支（live_update.rs:539-545 文档映射） | 总分结构对照 |
| `fi_service` | `do_fi` + `LsysFiCtl::encode_message`（I3a） | 消息构造 + asynsend |
| `rs_asynsend` | `IpcApi::asynsend`（I2 缝） | 生产 ENOSYS + mock 全真 |
| `BEG_RPROC_ADDR`/`END_RPROC_ADDR` | `RProcTable::iter_all`/`iter_in_use` | 迭代器替代指针界 |
| `RS_DONTREPLY` | `DispatchResult(i32)` 回复值模型 | 有Rust无文档项的映射 |
| `sef_local_startup` | `RsServer::init(init_type)` + `SefCallbacks` trait 分派（N5 重构） | C 的注册包装被 trait 化吸收 |
| `RS_VM_DEFAULT_MAP_PREALLOC_LEN` | `vm_default_prealloc(..., 8*1024*1024)` 参数注入（live_update.rs:228，R34.8 测试） | 常量以调用方参数表达 |

**B. ARCH 不需要（附理由）——10 项**

| C 符号 | 理由 |
|--------|------|
| `RS_CONST_H`/`RS_GLO_H`/`RS_TYPE_H` | C 头文件包含守卫——Rust 模块系统无此概念 |
| `EXTERN` | C 的 extern 声明机制宏——Rust pub/use 替代 |
| `_SYSTEM`/`_TABLE` | C 特性开关宏（负错误码/表内含）——Rust 无对应需求 |
| `errentry` | C errno 表项宏——error.rs 用 match 臂 + 常量自引用（Fix #66） |
| `RS_USE_PAGING` | const.h:84 恒 0 的死配置——本树无消费路径 |

**C. 归属他 stage——5 项（维持 E-8 判定）**

`exec_restart`（→19）、`srv_to_string_gen`/`srv_upd_to_string`/`print_services_status`/`print_update_status`（→08-stage-is dump 面）。

**D. 引用 L5 可观测性——5 项（非缺口，汇入 20.3-A3）**

`DEBUG_DEFAULT`/`PRIV_DEBUG_DEFAULT`/`DEBUG`/`PRIV_DEBUG`（C 调试 printf 开关）+ `rs_strerror` 的 strerror 回退（已由 `Errno::name`/`Display` 吸收，Fix #66）。真正缺的是 rs_verbose 的**输出缝**（见 20.3-A3）。

### 20.2 调用图反查（campaign 新增代码的从零复审）

> 对 I2-I6 新增 ~1500 行做 fresh-eyes 走查（review-core 同 Agent 局限规则的缓解）。

- **✅ 无 P0/P1 新缺陷**。重点核查过的面：`endpoint_slot` 负数 endpoint 安全（signal_manager 的内核任务信号路径，process_table.rs 越界即 None）；`terminate_decision` 的 norestart 计算顺序（init 分支先更新局部 flags，与 C manager.c:1067-1108 的 fall-through 一致）；`do_upd_ready_shell` 的 gate 捕获先于走链（PREPARE_DONE 属于报告者而非走链后的 curr）；`abort_update_proc` 的 reason≠OK 断言（update.c:710）。
- **✅ 确认两处 C 行为忠实（易误读为 bug，记录防止将来"修复"）**：①`rs_idle_period` 非 shutdown 且非 idle 时**跳过 DEAD 清理**——DEAD 槽在繁忙期滞留是 C 行为（utility.c:448-453），回收由后续 idle 期兜底；②`signal_manager` 终止臂先于 VM 免转发检查（main.c:686 先于 :694）——终止信号对 VM 也走执行体。
- **P2-S1（新条目）**：`signal_manager` 终止臂的 unpublish 缝只携带 `USE_COPY` 一个事实（其余三布尔硬编码 false，与 do_down 同款）——`unpublish_result` 四布尔聚合面的完整语义重建归属 11/19，已在 E-11 登记，此处不重复立项。

### 20.3 架构建议（A 系列，每条 ≥2 方案）

**A-1【P2·观察项】lib.rs 2,427 行的 arm shell 归宿**
- 现状：13/14/16 号活臂的 handler shell（do_down/do_lookup/do_fi/do_getsysinfo/do_sysctl/do_refresh/do_restart/do_clone/do_unclone/do_upd_ready_shell）+ SEF impl + run 循环同在 lib.rs（crate 最大文件）。
- 方案 a：按文档域拆 `impl RsServer` 到多文件（Rust 同 crate 多文件 impl 同类型合法）——模块边界与 13/14/16 号文档对齐。
- 方案 b：维持单文件 + 分区注释（C request.c 即单文件传统，1:1 对照价值高）。
- **推荐 b + 重访触发条件**：E-RSSTART 落地时三臂（UP/EDIT/UPDATE）shell 仍要动，届时一并拆分；此前拆分是纯机械迁移且会被随后的臂接线扰动。触发器：lib.rs >3k 行或 E-RSSTART 落地。

**A-2【P1·设计条目】编排函数的闭包参数膨胀**
- 现状：run_service(8 参)/start_service(8)/start_update(8)/terminate_service(9)/rs_idle_period(7)，too_many_arguments 全部 allow；每个调用点声明 3-4 个 noop 闭包（19 缝约定）。
- 方案 a：**Effects 聚合 struct**——按调用族分组（`CreateEffects{read_exec,publish,asynsend}`/`TerminateEffects{unpublish,run_script,asynsend}`/`LuEffects{...}`），`Default` 提供 noop 实现，调用点从 8 行闭包声明缩为 `Effects::NOOP`；awaiting-wiring 语义更显式（哪个缝仍是 noop 一目了然）。
- 方案 b：effect trait（`trait CreateHooks`）——多一层 trait 对象抽象，换来可组合的测试 double；与 mock-first 的 KernelApi 并行存在两套缝体系，成本高。
- 方案 c：维持现状——位置参数显式对应 C 的隐式全局使用点，注释已逐参数锚定。
- **推荐 a**，但迁移面 ~15 函数 × 全部调用点（2-3 个 todo-fix 轮），收益为可读性与显式性而非正确性——**是否执行交后续裁决**，本条目先登记方案与成本。
- 对照：Rust 社区依赖注入惯例倾向聚合 struct（builder/effects 模式）；Redox 无此形态（C 全局态），理论对照仅说明 C 的全局即 Rust 的参数——已是本项目既定翻译原则。

**A-3【P2·缺口条目】可观测性缝缺失（rs_verbose 全家无对应）**
- 现状：C 的 rs_verbose 条件 printf 遍布 request/manager/update/utility（含 lu_strerror/init_strerror 的消费端）；error.rs 的两张描述表 + Errno::name 已就绪但**无输出缝**（模块头自述"output goes through the kernel diagctl"——该缝未建）。
- 方案 a：每函数注入 `diag: &mut dyn FnMut(&str)`——与 A-2 冲突（签名再膨胀）。
- 方案 b：全局诊断单例——违反无全局纪律（单线程豁免也不做，kernel 教训）。
- 方案 c：**diagctl 通用输出缝**——E-11 的 `diagctl_stacktrace` 扩展为 `diagctl(output)`，verbose 面统一走 kernel 控制台通道（SYS_DIAGCTL code 1 对端已实现，edge E2 记录）。
- **推荐 c 的方向、缓行**：消费面在 19；本轮把"diagctl 通用输出缝"补登 E-11 清单，修复随 19 号接线。已同步 edge_todo.md。

**A-4【P2·一致性条目】`slot.upd` 镜像与链条目的双写责任**
- 现状：`ServiceSlot.upd: Option<UpdateEntry>`（镜像，monitor.rs 的 upd_init_maxtime 消费）由 live_update 的 swap/clear 路径写入（live_update.rs:992-994）；链内 `UpdateEntry` 是权威。若镜像建立后 entry 变异（prepare_maxtime/state_endpoint），镜像陈旧无告警。
- 方案 a：monitor 改读链（effective_period 增 `upd: &UpdateState` 参）——打破槽位自足性，签名面扩大。
- 方案 b：维持镜像 + 一致性责任显式化——`RProcTable::assert_consistent`（Fix #67）增加一条"slot.upd 为 Some 时须与链内同 id entry 一致"的 debug 校验，变异点补注释。
- **推荐 b**：一条 debug 断言的成本，换陈旧镜像的编译期后兜底。P2 条目（1 个 todo-fix 轮内完成）。
- ✅ **已修（Fix #79，2026-09-07）**：方案 b 落地，形态上有一处基于代码事实的修正——`UpdateChain` 挂在 `UpdateState`（ServerState）而非 `RProcTable` 内，校验器无法只凭 `&self` 触达链条，故 `assert_consistent` 增设 `update: Option<&UpdateState>` 参数（裸表单测传 `None` 诚实表达"链条不在场"；boot handover 黄金测试传 `Some(&state.update)`），而不是把检查拆成第二个自由函数——单一入口保证既有 golden 调用点自动获得新不变式，不用靠"记得再调一次"。第四条不变式是双向的：镜像 → 权威要求**全等**（C 单一存储没有"部分相等"概念），权威 → 镜像要求行内存在。落地过程中校验器首跑即抓出一个真实漂移点：`clear_upds` 只清链条、不清槽侧镜像，而其注释声称的"字段复位到 vacant 态（update.c:157-158 → rupdate_upd_init）"在镜像侧没有发生——陈旧的 `prepare_maxtime` 会在链清空后继续喂 `upd_init_maxtime`（monitor.rs:96）。修复 = `clear_upds` 同步清镜像（这正是 C `rupdate_upd_clear` 重置内嵌本体的 A-3 对应物）。`UpdateChain::add` 补镜像责任注记（R6 的 do_update 入链时消费）。

**A-5【P3·测试基建】booted_* 夹具族收敛**
- 现状：lib.rs 测试内 4 个 booted 变体（booted/booted_with/booted_vfs_labeled/booted_vfs_sysproc），参数轴（label/payload/vm_ok/execve_ok/fork_pid/child_endpoint/ticks/SYS_PROC/scheduler）以叠加包装表达。
- 方案 a：单一 `TestRs::builder()` 参数化——构造面收敛。
- 方案 b：维持——4 个具名夹具即文档（每个测试读夹具名即知前置）。
- **推荐 b（维持）**：夹具数已稳定且语义清晰；若再增 2 个变体再收敛。记录不改。

### 20.4 问题清单汇总

| 级别 | 条目 | 处置 |
|------|------|------|
| P0 | 无 | — |
| P1 | A-2 编排闭包参数膨胀（Effects 聚合方案） | 建议+成本已录，执行待裁决 |
| P2 | A-1 lib.rs 归宿（观察项，触发器已定）/ A-3 可观测性缝（方向 c，随 19）/ ~~A-4 slot.upd 一致性校验~~（✅ Fix #79）/ A-5 夹具（维持，记录） | A-4 已修；其余记录 |
| 覆盖率 | 40 项 ⚠️ 全部定谳：21 名称不匹配 + 10 ARCH + 5 归属他处 + 5 引 L5；零真缺口 | 判定表 20.1 |

### 20.5 Step 5.7 Rule Discovery

1. **CACG（Copy-struct ABI Gate）模式**（I7 提出，本轮确认并扩展）：C 侧以"整结构拷贝"开场的控制臂，接线前必须先审计结构体内全部 typedef 的本树定义完整性——缺失即登记 edge、臂保持死表 fail-closed。E-RSSTART 的登记避免了两次返工（I4/I5 原计划直接接线）。
2. **语义映射表回填**（本轮新发现）：coverage-extract 的 name-match 产生 40 个假 ⚠️，全部靠人工判定。建议把 20.1 判定表回写 `tools/coverage-extract/rs-semantic-map.json`（工具配置改动），后续复测 name-match 将直接归位，不再产生人工判定成本。

### 20.6 修复优先级路线（供后续 todo-fix 排队）

1. ~~A-4 slot.upd 一致性校验（小，1 轮）~~ ✅ Fix #79（2026-09-07）；
2. ~~semantic-map 回填（工具配置，1 轮内可并）~~ ✅ Fix #80（2026-09-07）；
3. A-2 Effects 聚合（大，2-3 轮，**待裁决**——2026-09-07 用户裁决：执行）；
4. A-1/A-3/A-5：记录，触发条件到达再动（A-1 触发器 = lib.rs >3k 行或 E-RSSTART 落地）。

### 20.7 §20 扫描轮修复记录（2026-09-07 起，Fix #79 起）

> 迭代协议同 §18.9（讲明白 → ≥2 方案对比 → 实现 → 测试 → 文档同步 → 全门验证 →
> 回归 review → 本节标注 → commit）。§20 条目（A 系列 + semantic-map 回填）与
> E-RSSTART/E-RSWIRE 解锁后的接线轮（见 §21）逐项记录于此。

### ✅ Fix #79 — A-4（P2 一致性）：`slot.upd` 镜像一致性校验 + `clear_upds` 镜像清理
- **File**：`os/servers/rs/src/process_table.rs`（`assert_consistent` 增 `update:
  Option<&UpdateState>` 参数 + 第四条双向不变式 + 5 个测试）、`live_update.rs`
  （`clear_upds` 同步清镜像 + `add` 镜像责任注记 + 1 测试）、`boot.rs`/
  `service_create.rs`（4 处既有调用点迁移）；文档 02 §4.1（不变量 6）、16 §3.3/§4.2
  （镜像与权威 + 不变量 7，顺带修正 §3.3 P2-3 段的陈旧 DEFERRED 声称——
  prepare_tm/prepare_maxtime/grants 已随 Fix #53 补齐）
- **Before**：镜像建立后链条 entry 变异（或链清空）无任何告警；`clear_upds` 的注释
  声称"字段复位到 vacant 态（update.c:157-158）"，但只清了链条副本、槽侧镜像残留
  ——陈旧 `prepare_maxtime` 会在链清空后继续喂 `upd_init_maxtime`（monitor.rs:96）。
- **After**：方案对比——a) 校验器挂 `assert_consistent`（选定，单一入口 + golden
  调用点自动覆盖）vs b) live_update 独立自由函数（检查分裂两模块，golden 测试要记得
  调两处——漏调失防，R10 同型教训）vs c) monitor 改读链条（§20.3 已否）。第四条
  不变式双向：镜像 → 权威全等（C 单一存储 update.c:196 无"部分相等"概念）、权威 →
  镜像行内存在。`clear_upds` 清镜像 = C `rupdate_upd_clear` 重置内嵌本体的 A-3
  对应物，非发明。
- **Verified**：`cargo test -p minix-rs` = **309 passed**（+5：synced 正测、
  drift/orphan/unmirrored 三个 `#[should_panic]` 负测、clear 镜像清理行为测试）；
  clippy 触碰文件零告警；fmt 干净；`tools/check-rs-unwired.sh` PASS（boot.rs:1045
  唯一标记带 18 号契约）。校验器设计过程中首跑即抓出 clear_upds 漂移点（见 Before）
  ——不变式断言优先于类型机器的又一实例（E-3 同思路）。

### ✅ Fix #80 — §20.5-2（工具配置）：semantic-map 回填，name-match 假 ⚠️ 归零
- **File**：`tools/coverage-extract/rs-semantic-map.json`（18 项新映射 +
  `sef_local_startup` 陈旧映射修复 + `_note` 记录回填来源）、
  `.review/claude/rs/scan/SYMBOLS.md`（重跑刷新）
- **Before**：coverage-extract 的 name-match 产生 21 个假 ⚠️（§20.1 A 类判定表
  ——Rust 等价物已存在但名字对不上），每次复测都要人工重判；其中 `sef_local_startup`
  的映射指向已不存在的 `SefCallbacks::local_startup`（N5 重构后被 `RsServer::init`
  + trait 分派吸收）——映射本身陈旧，是 21 项里唯一"映射错"而非"映射缺"的。
- **After**：20.1 判定表 21 项中 19 项回写映射（Rust 符号名逐一 grep 核实后写入：
  `RsServer::resolve_by_label`/`UpdateState::end_update`/`RProcTable::iter_all`/
  `RsServer::reply_unless_suppressed` 等）；**`copy_rs_start`/`do_edit` 刻意不回填**
  ——它们是 E-RSSTART 门上的真缺口，保持 ⚠️ 可见直至接线轮落地，避免制造假覆盖。
- **Verified**：重跑 coverage-extract：Rust name-match **115 (67.3%) → 151 (88.3%)**；
  剩余 ⚠️ 恰 20 项，逐名核对与判定表吻合：B 类（ARCH 不需要，8）+ D 类（引 L5
  可观测性含 rs_strerror 的吸收判定，5）+ C 类（归属他 stage，5）+ E-RSSTART 门
  （2）——零假阳性。文档覆盖 171/171（100%）不变。
