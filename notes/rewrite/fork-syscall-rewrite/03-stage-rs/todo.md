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
| D1 | `ServiceSlot` god struct（~35 全公开字段）无封装、无不变式 | P2 | ☐ |
| D2 | `SlotId` 无世代/代数，free→reuse 后旧索引悬垂；`get()` 越界 panic | P2 | ☐ |
| D3 | 公共函数 totality：`caller_can_control`/`lookup_by_domain` 对越界计数可 panic | P1 |✅ |
| D4 | 常量双定义（`RS_MAX_LABEL_LEN` 两处等） | P2 |✅ |
| D5 | `RsStart::default` 与 C 调用方默认不一致（sigmgr=SELF/scheduler=KERNEL/quantum=1 vs RS/SCHED/200） | P2 |✅ |
| D6 | `Privilege`/`ServiceSlot` 双份 io/irq 表（C 同构，可改进为单一权威） | P2 | ☐ |
| S1 | `build_cmd_dep` 不遇 NUL 停止 + `Label` 16 字节截断参数（潜在代码缺陷） | P1 |✅ |
| S2 | `activate_boot_slot` 缺失 cmd/script/argc/vm_call_mask/scheduler/priority/quantum/alive_tm | P1 |✅ |
| S3 | `sched_init_proc` 对 `NONE` 调度器不跳过（C sched_start.c:57-58） | P1 |✅ |
| S4 | `KernelApi::sched_init_proc` 签名丢弃调度参数（scheduler/priority/quantum/cpu） | P1 |✅ |
| E1 | MockKernelApi 四份复制、无共享测试工具模块 | P2 | ☐ |
| E2 | 解析函数（IpcListIterator/parse_label/build_cmd_dep）无 fuzz/property 测试 | P2 | ☐ |
| E3 | 无集成级 boot 顺序/消息交换测试（receive 不可用） | P2 | ☐ |

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

## 10. Fix-Status（2026-08-15 修复轮次）

> 按 fix-guard.md：每条修复前读目标行 ±5、grep 确认现状、单条修复、修后 grep 验证 + 记录。
> 修复后基线：`cargo test -p minix-rs` = **193 passed**；`cargo check` 无警告（minix-rs 本体）；
> `cargo clippy -p minix-rs --lib` 无告警；`cargo fmt --check` 干净；`tools/check-rs-unwired.sh` PASS。

### Fix #1 — T3 落地：Errno newtype + 全 crate 迁移
- **Before**：`os/servers/rs/src/*.rs` 全量 `Result<_, i32>`；`minix-sys` 的 `Errno` enum 缺
  `EDONTREPLY`/`EDEADEPT`；ARCH A-12 与代码不一致。
- **After**：`os/libs/minix-types/src/types/errno.rs` 新增 `pub struct Errno(i32)`（12 个关联常量 +
  `from_i32`/`to_i32`）；rs crate 全量 `Result<_, Errno>`；`ready.rs` 的
  `normalize_init_response`/`normalize_lu_response` 改配 `EDONTREPLY/EGENERIC` + `.to_i32()`；
  `sef.rs` 5 个 placeholder 改 `Err(Errno::ENOSYS)`（DEFERRED 注释），`signal_handler` 保留
  `unimplemented!`（无 Result 通道，注释说明）；测试同步。
- **Verified**：`cargo test -p minix-rs` 181→189（含新 `test_errno_values_match_c`/
  `test_errno_roundtrip`/`test_unimplemented_kernel_api_fails_closed`）；文档 99-rs-global-concepts.md §4。

### Fix #2 — S1：build_cmd_dep 遇 NUL 停止 + 参数不截断
- **Before**：`slot.rs::build_cmd_dep` token 循环不检查 NUL；`Label::from_bytes` 16 字节截断
  路径/参数（manager.c:289-323 不截断）。
- **After**：返回 `Vec<&[u8]>`（借用 cmd 切片 = C argv 指针进 r_args 的等价）；循环遇 NUL 停止
  （manager.c:306）；`service_create.rs::rebuild_args` 同步改为完整字节写入 + NUL 分隔。
- **Verified**：新增 `test_build_cmd_dep_stops_at_nul`/`test_build_cmd_dep_long_token_not_truncated`；
  `rg -n "build_cmd_dep" servers/rs/src/slot.rs` 确认签名；文档 08-rs-slot-config.md §2.5/§3.3/§5。

### Fix #3 — D3：越界计数 fail-closed
- **Before**：`access.rs::caller_can_control` 直接 `control[..nr_control]`、`process_table.rs::
  lookup_by_domain` 直接 `domain[i]` —— `nr_* > 容量` 即 panic。
- **After**：`.get(..n)` + `is_some_and(...)`，越界返回 false/None。
- **Verified**：新增 `test_caller_can_control_corrupt_count_fails_closed`/
  `test_lookup_by_domain_corrupt_count_fails_closed`。

### Fix #4 — S3/S4：sched NONE 跳过 + KernelApi::sched_init_proc 携带参数
- **Before**：`sched_init_proc` 对 NONE 调度器仍调 KernelApi（测试注释承认偏差）；
  `KernelApi::sched_init_proc(proc: Endpoint)` 丢 scheduler/priority/quantum/cpu。
- **After**：`cfg.scheduler == NONE → Ok(NONE)` 短路不触内核（sched_start.c:45-47）；
  签名 `fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno>`
  （回传 `*newscheduler_e`）；新增 `SchedulerConfig::boot_defaults`（SRV_SCH=KERNEL/
  SRV_Q=USER_Q=7/SRV_QT=USER_QUANTUM=200/cpu=0，main.c:320-322 + priv.h:88,93,98）；
  `USER_Q`/`MIN_USER_Q`/`MAX_USER_Q`/`USER_QUANTUM` 常量入 sched.rs。
- **Verified**：`test_sched_init_proc_user_proc_none` 断言 mock 零调用；新增
  `test_sched_init_proc_kernel_passes_full_config`/`test_boot_defaults_match_c`；文档
  01-rs-boot-init.md §3.5、03-rs-privilege.md §3.6/§4.4/§5.5。

### Fix #5 — D5：RsStart::default 对齐 C 调用方默认
- **Before**：`sigmgr=SELF`/`scheduler=KERNEL`/`quantum=1`（"未设置"哨兵，与 C 差 200 倍）。
- **After**：`sigmgr=RS`/`scheduler=SCHED`/`priority=USER_Q=7`/`quantum=USER_QUANTUM=200`/
  `cpu=-1`（minix-service parse.c:1164-1169：DSRV_SM/DSRV_SCH/DSRV_Q/DSRV_QT/DSRV_CPU）。
- **Verified**：新增 `test_rs_start_default_matches_c_caller`；文档 08-rs-slot-config.md §2.1。

### Fix #6 — D4：常量收敛
- **Before**：`RS_MAX_LABEL_LEN` 双定义（service_slot.rs:20 + state_data.rs:22）；`MAX_BACKOFF`
  双定义（monitor.rs:28 u32 + recovery.rs:22 i64）；`SEF_LU_STATE_EVAL` 与 NULL/UNREACHABLE 分居
  两模块。
- **After**：`RS_MAX_LABEL_LEN` 唯一在 service_slot.rs；`SEF_LU_STATE_EVAL` 移入 live_update.rs
  （与 NULL/UNREACHABLE 同族）；monitor.rs 未用的 `MAX_BACKOFF` 删除（recovery.rs 保留 i64 版）。
- **Verified**：`rg -n "RS_MAX_LABEL_LEN" servers/rs/src/state_data.rs` 仅 import；
  `rg -c "pub const MAX_BACKOFF" servers/rs/src/` = 1。

### Fix #7 — T2：KernelApi 删默认 impl + UnimplementedKernelApi fail-closed
- **Before**：trait 4 个方法带 `unimplemented!()` 默认实现；`UnimplementedKernelApi` 8 方法全部
  `unimplemented!()` panic（RS 是 root system process，panic = 整机不可用）；`main.rs` 吞掉 init 错误。
- **After**：删全部默认实现（编译期强制全量实现）；`UnimplementedKernelApi` 12 方法全部
  `Err(Errno::ENOSYS)`；`main.rs` boot 失败显式 panic（C boot 错误同样 panic，main.c:226）。
- **Verified**：新增 `test_unimplemented_kernel_api_fails_closed`；`rg -c "unimplemented!" servers/rs/src/boot.rs`
  生产代码 = 0（仅 `self_update` feature 门控 1 处带 18 文档契约，T7 gate 放行）。

### Fix #8 — T6：Step 2/3 计数修正 + fail-closed
- **Before**：SYNCH_BOOT 服务被计入 `nr_uncaught_init_srvs`（C 是同步 catch，main.c:390-392）；
  Step 3 `while counter>0 { counter-=1 }` 不接收（fail-open，main.c:401-407 是阻塞接收）。
- **After**：SYNCH_BOOT → Step 2 显式 `Err(ENOSYS)`（12 落地前 fail-closed）；Step 3 计数 > 0 时
  `Err(ENOSYS)`。VM 计数（main.c:369-370）此前已正确。
- **Verified**：`test_step3_fails_closed_when_init_ready_pending`/
  `test_step2_synch_boot_fails_closed`；boot 顺序测试改走私有 step 方法驱动（step3 不再假装成功）。

### Fix #9 — T1：状态 handover
- **Before**：`RProcTable`/`system_hz`/`shutting_down` 封在 `BootInit` 内，主循环无法访问（06 需
  do_period/RS_DOWN sweep）。
- **After**：`BootInit::into_state(self) -> ServerState<'a>`（消费 self）；`RsServer` 持
  `boot: Option<BootInit>` + `state: Option<ServerState>`，`init(Fresh)` 成功移交；
  `RsServer::state()` 单一访问器；`run()` 对未完成 boot fail-closed。
- **Verified**：新增 `test_rs_server_handover_after_fresh_init`（RS-only 表 → step3 零计数通过 →
  移交后 `state.system_hz/table/shutting_down` 可读、`boot` 已消费）；文档 01-rs-boot-init.md §3.4/§4.2。

### Fix #10 — T4：get_work 不再伪造 IpcStatus
- **Before**：`get_work` 在 receive stub 后伪造 `IpcStatus{flags:0}`（notify 会被错分为请求）。
- **After**：`get_work` 显式 `todo!("... 06 ...")` fail-closed，不伪造状态字；ipc_status 与 receive
  原语一同落地（06）。
- **Verified**：`rg -n "IpcStatus \\{ flags: 0 \\}" servers/rs/src/lib.rs` = 0；文档 06-rs-main-loop.md §3 契约不变。

### Fix #11 — T7：接线占位符 CI 门禁
- **Before**：20 处 `unimplemented!()`/`todo!()` 无门禁。
- **After**：`tools/check-rs-unwired.sh`——排除 `#[cfg(test)]` 后，生产代码每个占位符必须带
  `{NN}-rs-*.md` 文档契约（前 5 行注释），否则 exit 1。当前 3 处生产占位（boot.rs:624 self_update
  feature 门控、lib.rs get_work、sef.rs signal_handler）均带契约 → PASS。
- **Verified**：`tools/check-rs-unwired.sh` = PASS；负例（裸 `todo!`）exit 1。

### Fix #12 — S2：activate_boot_slot 补全 boot 字段
- **Before**：缺失 cmd/script/argc/vm_call_mask/scheduler/priority/quantum/alive_tm
  （alive_tm=0 使心跳判定从启动起偏差，scheduler=NONE 触发 debug_assert 崩溃）。
- **After**：新增 `KernelApi::get_ticks`（C getticks，main.c:333）；`activate_boot_slot(..., ticks)`
  按 main.c:308-333 补全：`cmd` strlcpy、`script[0]='\0'`、`rebuild_args`（build_cmd_dep）、
  `vm_call_mask=CallMask::all()`（SRV_VC=ALL_C）、`scheduler/priority/quantum/cpu= boot_defaults`、
  `alive_tm=ticks`。
- **Verified**：新增 `test_boot_slot_populates_s2_fields`（cmd/argc/script/vm_call_mask/scheduler/
  quantum/alive_tm 全断言）；文档 02-rs-process-table.md §4.2。

### Fix #13 — §8.3 工具链清理
- `ready.rs:19` unused `EINVAL`（T3 迁移时消除）；`query.rs:63` manual_range_contains →
  `!(2..NAME_BUF_LEN).contains(&len)`；`ready.rs:68` init_message 8 参数加
  `#[allow(clippy::too_many_arguments)]`（1:1 对应 C init_service，payload 结构延迟到 12 调用点）；
  `privilege.rs:303`/`dispatch.rs` fmt；`boot.rs` 重复 doc comment 删除；`sched.rs` USER_Q 用
  MIN/MAX 命名常量表达（clippy no-effect 消除）。
- **Verified**：`cargo fmt --check` 干净；`cargo clippy -p minix-rs --lib` 零告警。

### 遗留（未修，属后续轮次/接线期）
- E1/E3：四份测试 mock 未合并为共享 `testutil`（P2，19 接线前做）。
- E2：proptest property 测试未补（P2，接线期）。
- T5：`&mut dyn KernelApi` 注入边界未统一为 monitor 模式（P2，19 前定）。
- D1/D2/D6：`ServiceSlot` god struct / `SlotId` 世代 / io-irq 双表（P2，结构性，另行立项）。
- T3 残留：`boot.rs` lookup 失败与 getnpid 负值共用 `ENOSYS`（内部不变式域，19 接线时区分
  `BootError`）。

---

## 11. 第二轮深度 review 追加发现（2026-08-15 R2 — 独立重读 + C 逐条对照）

> **方式**：本轮**不增量叠加**，而是对 `os/servers/rs/` 全部 22 模块（~9.6k 行）重新独立通读，
> 逐条对照 `minix3/minix/servers/rs/` C 源（每项附 grep/sed 证据），并对照 Redox OS 与
> Rust 社区最佳实践（tokio slab 世代计数、redox_syscall 单一错误 ABI、typed-state 等）。
> **本轮新增：1 个 P0（潜在）+ 4 个 P1 + 6 个 P2**，与 §1 的 T/D/S/E 系列互补，多为
> "上一轮未覆盖的 C 语义细节"与"接线期（19/07/12）双类型/状态归属决策"。
> **基线不变**：`cargo test -p minix-rs` = 193 passed；`cargo check`/`clippy`/`fmt` 干净。
> **证据约定**：每项给出 `rg`/`sed` 复现命令，复核时直接重跑。

### N1. [P0-潜在] table.rs 五个 priv flags 常量全部与 C priv.h 不符 → boot 特权装配错误

**现状**：`table.rs:66-74` 自定义了一套 u32 priv flags 常量，与 C `priv.h:45-49` +
`const.h:143-154` 的位值**全部不符**：

| 常量 | C 真值（priv.h + const.h） | table.rs 现值 | 偏差 |
|------|---------------------------|--------------|------|
| `SRV_F`  | `SYS_PROC\|PREEMPTIBLE` = **0x012** | `SYS_PROC\|0x004` = **0x014** | 0x004 是 BILLABLE 不是 PREEMPTIBLE(0x002) |
| `DSRV_F` | `SRV_F\|DYN_PRIV_ID` = **0x01A** | `SRV_F\|0x002` = **0x016** | 位错 |
| `RSYS_F` | `SRV_F\|ROOT_SYS_PROC` = **0x112** | `SRV_F\|0x008` = **0x01C** | 缺 ROOT_SYS_PROC(0x100)，多了 DYN_PRIV_ID(0x008) |
| `VM_F`   | `SYS_PROC\|VM_SYS_PROC` = **0x210** | `SYS_PROC\|0x020` = **0x030** | 0x020 是 CHECK_IO_PORT，缺 VM_SYS_PROC(0x200) |
| `USR_F`  | `BILLABLE\|PREEMPTIBLE` = **0x006** | `0x004\|0x001` = **0x005** | 0x001 未定义，缺 PREEMPTIBLE(0x002) |

**影响链**：`BOOT_IMAGE_PRIV_TABLE`（table.rs:90-158，RS→RSYS_F/VM→VM_F/PM…→SRV_F/INIT→USR_F）
→ boot.rs:493 `PrivFlags::from_bits_truncate(priv_.flags as u16)` → `Privilege::boot_priv` →
19 接线后 `sys_privctl(SYS_PRIV_SET_SYS, &priv)` 提交给内核的 s_flags 错误：
RS 丢 `ROOT_SYS_PROC`、VM 丢 `VM_SYS_PROC`、全部系统服务丢 `PREEMPTIBLE` 且误得 `BILLABLE`、
INIT 丢 `PREEMPTIBLE`。**这是"接线后必然触发"的确定性缺陷**。
为何测试全绿：`table.rs` 的 `test_priv_table_matches_c`（table.rs:217-252）只查
"flags class"（SYS_PROC 位在/不在），不查位值；design doc 03-rs-privilege.md:205-209
明确写了正确语义（`SRV_F = SYS_PROC | PREEMPTIBLE` 等），**代码偏离了 design** →
按 review-core 归类为 P0-design-deviation（潜在，接线前必修）。

**改进思考**：
- 删除 `table.rs:64-74` 的私有 u32 常量，直接复用 `privilege.rs` 的 `PrivFlags` 关联常量
  （privilege.rs:112-124 是正确且经测试的版本）；`BootImagePriv.flags` 类型从 `u32` 改为
  `PrivFlags`（u16），根除双轨制（见 N10）。
- 补位值断言：`test_priv_table_matches_c` 增加对每个表项 flags 的精确值断言
  （`assert_eq!(table[i].flags, PrivFlags::SRV_F)` 等），杜绝"class 级"弱断言。
- 对照 Redox：Redox 的 init.rc 配置是纯数据、无 bit 编码；Minix 的 s_flags 位编码必须
  有单一权威（privilege.rs）+ 逐位测试（现在 privilege.rs 有、table.rs 没有）。

### N2. [P1] period_decision 的 ping 间隔误用有效 period（07 心跳节律与 C 不同）

**现状**：`monitor.rs:127` —— `if now.saturating_sub(rp.check_tm) > period { return
PingRequest }` 用的是 `effective_period()`（monitor.rs:66-75：INITIALIZING → `RS_INIT_T`
或 `UPD_INIT_MAXTIME`；否则 `r_period`）。C `do_period` 的最后分支
（`request.c:1035-1037`）用的是**原始 `rp->r_period`**：
`else if (now - rp->r_check_tm > rp->r_period) { ipc_notify(...); rp->r_check_tm = now; }`。

**问题分析**：对**正在初始化**的服务（boot 槽 `r_period=0`，main.c:338）：
- C：`now - r_check_tm > 0` 恒真 → **每个 RS_DELTA_T 都发一次状态 ping**（check_tm 更新后下轮再发）；
- Rust：`now - check_tm > RS_INIT_T`（= 10×RS_DELTA_T）→ **每 10 秒才 ping 一次**。
两者对初始化中服务的 ping 节律差 10 倍；若服务 `r_period>0`，C 按原始 period、Rust 按
`RS_INIT_T`，同样分歧。07（period-heartbeat）落地时按 Rust 版实现，行为将与 C 明显不同。

**改进思考**：
- 07 落地时逐分支对照 `request.c:965-1038`：`effective_period` 只用于
  "answer pending 超时"（2×period，request.c:1004-1006）与 `period==0` 门；
  "no answer pending → 请求状态"分支必须用 `r_period`（request.c:1035）。
- 或者显式声明 ARCH 偏离（Rust 版节律更保守、ping 更少），但必须在 07 doc + 代码注释
  双处标注 `[ARCH: ...]`，不能默默不同。
- 补测试：INITIALIZING + r_period=0 的槽，断言 C 语义（每 tick ping）与 Rust 现状的差异，
  让决策显性化。

### N3. [P1] Machine 状态未纳入 ServerState（get_machine 生产路径零调用）

**现状**：`KernelApi::get_machine`（boot.rs:70）存在，但 **crate 内生产路径零调用**
（`rg "get_machine" os/servers/rs/src/` 仅 trait 定义/测试 mock）。C `main()` 在进入主循环前
`sys_getmachine(&machine)`（main.c:53）填充全局 `machine`（glo.h）。`ServerState`
（lib.rs:140-150）只有 tables/rinit/table/shutting_down/system_hz/nr_uncaught_init_srvs 六字段，
**没有 machine**。

**问题分析**：T1 handover 修复把运行期状态移交给了 `ServerState`，但漏了 `machine`。
13 落地时 `do_up` → `check_request`（slot.rs:170）需要 `bsp_id`/`processors_count`
（`RS_CPU_BSP`/越界→BSP 解析，request.c:1286-1296）——届时要么在主循环内临时
`get_machine()`（C 是一次性启动查询，语义不符），要么再次补 handover。

**改进思考**：
- `ServerState` 增加 `machine: Machine`；`init(Fresh)` 的 step0 或 `RsServer::init` 里
  `sys.get_machine()?`（对齐 C main.c:53 的位置——在 SEF startup 之后、主循环之前）。
- 注意 C 里 `sys_getmachine` 失败是 `panic`（main.c:54）；Rust 侧沿用 fail-closed
  （`Err(ENOSYS)` 上抛，main.rs 已 panic）。
- 对照 Redox：Redox daemon 的 CPU 拓扑经 scheme 查询（如 `sysinfo` scheme），无启动期
  全局快照；Minix RS 是启动期快照语义，Rust 应显式建模为 ServerState 字段而非全局。

### N4. [P1] minix-types::Errno 与 minix-sys::Errno 双类型并存（19 桥接决策）

**现状**：`minix_types::Errno`（minix-types/src/types/errno.rs:123，`struct Errno(i32)`
newtype，12 个常量）是 RS 全 crate 使用的错误类型（T3 修复已落地）；但 `minix-sys`
（Cargo.toml 已依赖，`get_work` 调用 `minix_sys::receive`）有自己的
`pub enum Errno`（minix-sys/src/lib.rs:106-128，22 个变体，`Eperm`/`Enoent`… 命名）。

**问题分析**：
1. 19 接线时 `KernelApi` 的实现在 `minix-sys` 上，两种 Errno 必须桥接
   （`minix_sys::Errno` → `minix_types::Errno`），任何漏转都是编译/运行期错误面。
2. `minix_sys::Errno` **缺 RS 必需值**：`ENOSYS`/`EDEADEPT`/`EDONTREPLY`/`EGENERIC`/
   `ERESTART`（errno.h:78/211/199/200/196）都不在其中；`Errno` 枚举缺值会导致
   `KernelApi` 实现无法表达这些 errno。
3. 命名风格分裂：`Eperm` vs 全项目 `EPERM`（与 Minix 常量、review 习惯一致）。
4. Redox 的做法是**单一共享 ABI crate**（`redox_syscall::Error{errno}`），内核与用户态
   用同一个类型——T3 的 A-12 定位也是"共享 ABI"，但两个 crate 各造一个违反了该定位。

**改进思考**：
- 19 接线前定单一权威：要么 `minix-sys` 复用 `minix_types::Errno`（推荐，把
  `minix-sys::Errno` 删除/改为 re-export），要么反向迁移全 RS。禁止两类型长期并存。
- 若保留枚举式（可穷举匹配），需补全 RS 需要的 5 个值并统一命名（`EPERM` 风格）。
- 加编译期桥接测试：`Errno::from_i32(e.to_i32())` 对全值域往返一致。

### N5. [P1] SefCallbacks 函数指针回调无法访问服务器状态（12/18 落地前需重构）

**现状**：`SefCallbacks`（sef.rs:72-85）7 个回调全是**裸函数指针**
（`SefInitCb = fn(SefInitType, &SefInitInfo) -> Result<i32, Errno>`、
`SefMsgCb = fn(&Message) -> Result<i32, Errno>`、`SefSignalCb = fn(i32)`），
`RsServer.callbacks` 字段标 `#[allow(dead_code)]`（lib.rs:119）。

**问题分析**：
1. C 的回调靠全局变量访问 `rproc[]`/`rupdate`；Rust 的状态在 `RsServer`/`ServerState`
   里，而函数指针签名**没有任何状态参数**——12 的 `init_response`/`lu_response`、
   18 的 `init_restart`/`init_lu` 落地时，回调体无法拿到 `&mut ServerState`。
2. 当前 `RsServer::init(Fresh)` 是"绕过回调表直接调 `BootInit::init_fresh`"（lib.rs:169-174），
   与 C `sef_startup()` → 回调派发的结构不一致（doc 01 §3.3 声明"主循环 RS_INIT 分支合并"，
   但合并点没有任何可携带状态的机制）。
3. 这是"把 C 全局函数指针表搬进 Rust 而未利用类型系统"的典型例子；
   Rust 社区对应物是 trait 对象 / 闭包（`Box<dyn FnMut(&mut ServerState, …)>`）。

**改进思考**：
- 12/18 落地时把回调表重构为 trait（如 `trait SefCallbacks { fn init_response(&mut self,
  state: &mut ServerState, m: &Message) -> Result<i32, Errno>; ... }`），由 `RsServer`
  实现；或回调签名统一携带 `&mut ServerState`（单线程下 `&mut` 合法，AGENTS.md 用户态模型）。
- 至少现在就把签名定下来（design-first）：函数指针 `fn` 无捕获是编译期最强约束，但它
  逼不出"回调需要状态"这一事实——`SefCallbacks` 应从 struct 升格为 trait 或方法集。
- 对照 Redox：Redox daemon 无 SEF 等价物（init 配置驱动、无回调表）；Minix SEF 的
  消息驱动回调本质是**状态机方法**，trait 化才是 Rust 的忠实建模。

### N6. [P2] Label 相等语义分裂（派生 vs strcmp）+ 满 16 字节无 NUL

**现状**：`Label` 派生 `PartialEq`（service_slot.rs:192-193，16 字节数组整体比较）；
另实现 `PartialEq<&str>`（service_slot.rs:226-229，`as_str() == Some(*other)`，strcmp 式）。
使用点分裂：
- `caller_can_control`（access.rs:57 `list.iter().any(|c| c == proc_name)`）、
  `add_backward_ipc`（ipc_mask.rs:141 `name == *target_name`）用**派生**语义；
- `lookup_by_label`（process_table.rs:232 `rp.pub_.label == label`）用 `&str` 语义。

**问题分析**：
1. 派生语义不是 strcmp 语义：若 label 数据含**嵌入 NUL + 后续字节**（来自 `Label::from_bytes`
   对含 NUL 切片的复制），`Label==Label` 会因后续字节不同而判不等，C `strcmp` 在首个 NUL 停
   判相等 → 两处判定与 C 分歧（当前方向是 fail-closed：Rust 拒了 C 会放行的控制/发信权限）。
2. `PartialEq<&str>` 依赖 `as_str()`（UTF-8 校验）：非 UTF-8 label → `None` → 恒不等，
   与 strcmp 的字节语义不符（如 `check_duplicates` 的 `unwrap_or("")` 会把非 UTF-8 label
   当空串查重 → 重复检测 fail-open，request.rs:64）。
3. **满 16 字节边界**：`from_bytes` 对 ≥16 字节输入复制 16 字节、**无 NUL 终止符**；
   C `strlcpy(label, src, 16)` 截到 15 字节 + NUL。16 字符 label 在 Rust 是 16 字节串、
   在 C 是 15 字节串，比较/回显结果不同。

**改进思考**：
- 统一相等语义：为 `Label` 实现单一 `PartialEq`（遇 NUL 截断比较，与 strcmp 一致），
  删除或显式化 `PartialEq<&str>`；或把 `as_str()` 改为字节视图 `as_cstr()` 供比较。
- `from_bytes` 对齐 strlcpy：`n = bytes.len().min(RS_MAX_LABEL_LEN - 1)` + 强制尾 NUL
  （C 语义），并加 16 字节边界测试（当前测试只覆盖 <16 与截断为 16 无 NUL 的 as_str 行为）。
- 对照 Redox：label 等价物是 `str` 路径（UTF-8 原生）；Minix label 是字节串，
  Rust 侧应显式声明"label 是 NUL 终止字节数组"并统一比较实现。

### N7. [P2] CallMask::set_bit / SysMap::set 在 release 构建下移位溢出静默错位

**现状**：`CallMask::set_bit`（privilege.rs:192 `1u64 << offset`）、`test_bit`（:198）、
`SysMap::set`（privilege.rs:275）与 `from_calls` 的 `(1u64 << tot_nr_calls) - 1`
（privilege.rs:220）——`offset >= 64` 时 debug 构建 panic、**release 构建按 x86 shl 语义
mask 成 `offset & 63`**，静默设置错误位。当前唯一的越界守卫是 `from_calls` 里的
`debug_assert!`（privilege.rs:230，仅 debug 生效）。

**问题分析**：`from_calls` 的调用数据来自 boot 表/RS_UP 请求（攻击者可构造
`calls[i] - call_base` 越界）；release 构建下越界不再 panic 而是写错位——对内核调用掩码
意味着"允许了错误的系统调用"，是安全相关的静默错误面。C 原版 `SET_BIT` 同样 UB（更糟），
但 Rust 本可做得更好。

**改进思考**：
- `set_bit`/`set` 改为 `checked_shl` + `?`（或返回 `Result`），`from_calls` 对越界
  `return Err(Errno::EINVAL)`（fail-closed），并补 release 语义测试
  （`#[cfg(not(debug_assertions))]` 下断言越界返回 Err）。
- `from_calls` 的 `(1u64 << tot_nr_calls) - 1` 改用 `mask = if tot_nr_calls >= 64 { u64::MAX }
  else { (1u64 << tot_nr_calls) - 1 }`，消除 64 边界 UB 隐患（当前调用点 58/49 安全，但
  常量一旦上调即触发）。

### N8. [P2] RupdateDescriptor 死代码 + 与 UpdateChain 双建模（双源真相）

**现状**：`RupdateDescriptor`（process_table.rs:46-103，含 num_rpupds/curr/first/last/vm/rs
rpupd 字段）**全 crate 无生产使用者**（`rg "RupdateDescriptor" os/servers/rs/src/` 仅
lib.rs:71 的 re-export + 自身测试）；实际建模在 `UpdateChain`（live_update.rs:295-429，
entries + first/curr/last/vm/rs 索引）。C 只有**一个** `struct rupdate`（type.h:43-54）。

**问题分析**：两个结构并行描述同一状态（C 的 rupdate），无任何编译期约束保证同步；
16 落地时如果两端都写（`RupdateDescriptor.num_rpupds` 与 `UpdateChain.len()`），必然漂移。
另外 `UpdateChain` 内部用裸 `usize` 索引 entries（live_update.rs:322-341），与 `SlotId`
混用，类型上可误传。

**改进思考**：
- 删 `RupdateDescriptor`，`UpdateChain` 升格为唯一的 rupdate 模型（字段对齐 type.h:43-54）；
  或让 `RupdateDescriptor` 成为 `UpdateChain` 的薄视图（`fn num_rpupds(&self) -> usize`）。
- `UpdateChain` 的索引类型改为 `SlotId`（或新建 `ChainIdx` newtype），消除 usize/SlotId 混淆。
- 对照 Redox：无等价物；Rust 社区惯例是"一种状态一个权威类型"（single source of truth），
  双建模是 16 落地前的负债，应先删后写。

### N9. [P2] IoRange 双类型 + NR_SCHED_QUEUES 双定义（配置单一权威缺失）

**现状**：`IoRange` 在 `service_slot.rs:241`（r_io_tab 备份）与 `privilege.rs:285`
（s_io_tab）各定义一个**同形异构** struct（base/len）；`NR_SCHED_QUEUES` 在 `slot.rs:31`
与 `sched.rs:25` 各定义一次（注释"same value as slot.rs"，无编译期联动）。

**问题分析**：同形异构类型需要显式转换（备份↔priv 同步，D6 已提数据层，这里是类型层）；
常量双定义一旦一方上调，`MIN_USER_Q`（sched.rs:18）与 `check_request`（slot.rs:178）
的判定就悄悄分裂。D4 已列 `RS_MAX_LABEL_LEN` 双定义，本项是同类模式的类型级扩展。

**改进思考**：
- `service_slot::IoRange` 删除，统一用 `privilege::IoRange`（D6 的单一权威方案一起落地）；
- `NR_SCHED_QUEUES` 移到单一权威模块（建议 sched.rs，slot.rs 引用之），
  并加编译期一致性测试（`assert_eq!(slot::NR_SCHED_QUEUES, sched::NR_SCHED_QUEUES)` 或
  直接删除一个）。

### N10. [P2] PrivFlags u16/u32 双轨 + from_bits_truncate 静默丢位（N1 根因之一）

**现状**：boot 表用 `u32`（`BootImagePriv.flags: u32`，table.rs:38），privilege 用
`PrivFlags(u16)`（privilege.rs:83-97）；boot.rs:493 `PrivFlags::from_bits_truncate(priv_.flags
as u16)` 静默截断未知高位。C 是 `s_flags = boot_image_priv->flags`（main.c:269）直接赋值、
**保留所有位**。

**问题分析**：`from_bits_truncate` 会把未来新增的 s_flags 位（或误填的高位）静默丢弃，
且 u32→u16 截断本身丢位；N1 的位值错误正是借这条静默路径溜进 `Privilege` 的
（无 panic、无测试暴露）。这是"双轨制 + 静默转换"的组合缺陷。

**改进思考**：
- `BootImagePriv.flags` 直接改为 `PrivFlags`（与 N1 的修复合并），消除 u32/u16 转换面；
- 若必须保留 u32 wire 形态，用 `PrivFlags::from_bits` + `map_err`（fail-closed）替代
  `from_bits_truncate`，未知位显式报错而非丢弃。

### N11. [P2] build_cmd_dep/rebuild_args 空命令的 argc 边界（C argv[0]="" vs Rust argc=0）

**现状**：C `build_cmd_dep`（manager.c:291-321）无条件 `r_argv[arg_count++] = r_args`
（argv[0] 恒存在），空 `r_cmd` 时 `r_argc = 1`（argv[0] 指向空串）；Rust `build_cmd_dep`
（slot.rs:202-228）对空输入返回空 Vec，`rebuild_args`（service_create.rs:80-100）→
`argc = 0`，`args` 缓冲全零。

**问题分析**：09/10 的 exec 接线要从 `args` 缓冲 + `argc` 重建 argv：C 语义是
`argv = [""]`（argc=1，exec 必然 ENOENT 失败），Rust 语义是 `argv = []`（argc=0）。
空命令是配置错误，但"失败形态"不同（C 走 exec 失败路径、Rust 走 argc=0 路径），
接线时若不注意会出现难排查的差异。

**改进思考**：
- 09/10 接线时对齐 C：`rebuild_args` 恒写 `args[0] = 0`（argv[0] 位置），
  `argc >= 1`；或显式声明 ARCH 偏离（argc=0 更诚实）并在 08/09 doc 标注。
- 补测试：空 cmd、纯空格 cmd（C 会得到 argc=1 的空串 argv[0]）两种边界。

### N12. Redox / OS 最佳实践对照补充（本轮新增视角）

| 维度 | Redox 做法 | rs server 现状 | 对照结论 |
|------|-----------|---------------|---------|
| 错误类型 ABI | `redox_syscall::Error{errno}` **单一共享类型**，内核/用户态同 crate | `minix_types::Errno(i32)` 与 `minix-sys::Errno` 枚举**双类型** | N4：19 前统一为单一权威，正是 Redox 的共享 ABI crate 定位 |
| 启动期状态快照 | daemon 按需查询（`sysinfo` scheme），无启动期全局快照 | C 在 main.c:53 一次性 `sys_getmachine`；Rust 无对应状态 | N3：`Machine` 应进 `ServerState`（对齐 C 启动快照语义） |
| 回调/事件模型 | 无 SEF 等价物；init 配置驱动 | 函数指针回调表无法携带状态 | N5：SEF 回调 trait 化（状态机方法），才是 Rust 忠实建模 |
| 槽表句柄 | `Arc<dyn Resource>`，无悬垂 | `SlotId` 索引，无世代（D2） | 维持 D2 结论；tokio slab 的世代计数是 Rust 社区标准答案 |
| 配置/状态分离 | init.rc 纯配置 vs daemon 运行态 | `ServiceSlot` 混合配置+实例状态（D1） | 维持 D1；`table.rs` 静态表（纯配置）可 `Copy`，实例状态应分层 |
| ping/心跳 | 无心跳协议（内核不监控 daemon） | `period_decision` 需逐分支对照 C | N2：ping 间隔分支必须用 `r_period`（request.c:1035） |
| 位编码常量 | 无（配置是文本） | s_flags 位编码需单一权威+逐位测试 | N1/N10：privilege.rs 正确、table.rs 错误——权威必须唯一 |
| 事件循环 | 单线程 `SYS_EVENT` 循环，无锁 | 单线程 receive+classify，无锁（✅） | 结构一致，保持 |

### N13. 本轮关键验证证据（复现命令）

```bash
# N1：C 真值 vs table.rs 现值
sed -n '143,154p' minix3/minix/include/minix/const.h   # PREEMPTIBLE=0x002 BILLABLE=0x004 ... ROOT_SYS_PROC=0x100 VM_SYS_PROC=0x200
sed -n '45,49p'  minix3/minix/include/minix/priv.h     # SRV_F/DSRV_F/RSYS_F/VM_F/USR_F 组合
sed -n '66,74p'  os/servers/rs/src/table.rs             # 现值（全部不符）
sed -n '205,209p' notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md  # design 语义正确
# N2：ping 间隔分支
sed -n '1030,1038p' minix3/minix/servers/rs/request.c   # now - rp->r_period（原始值）
sed -n '112,129p'  os/servers/rs/src/monitor.rs          # now - period（有效值）→ 偏差
# N3：get_machine 生产路径零调用
rg -n "get_machine" os/servers/rs/src/*.rs | grep -v "#\[test\]" | grep -v "mod tests"
# N4：双 Errno
rg -n "pub enum Errno|pub struct Errno" os/libs/minix-sys/src/lib.rs os/libs/minix-types/src/types/errno.rs
# N5：回调签名无状态
sed -n '40,90p' os/servers/rs/src/sef.rs
# N8：RupdateDescriptor 死代码
rg -n "RupdateDescriptor" os/servers/rs/src/ | grep -v process_table.rs
```

**修复优先级建议**：N1（P0，位值错误）→ N2/N3/N4/N5（P1，接线期决策）→ N6-N11（P2）。
N1/N10 与 D4 的"常量双定义"是同一根因的两个后果（无单一权威 + 无位值测试），
建议在 03-rs-privilege.md 的 §4.2 一并收口。

---

## 12. 第三轮修复记录（2026-08-16 — 按 §11 逐项修复）

> 本轮按 §11 优先级修复 N1-N11（**文档与代码一并**），遵守 fix-guard.md：每处修复前读
> 目标 ±5 行 + `rg` 确认现状，单条修复，修后 grep/cargo 验证。
> **修复后基线**：`cargo test -p minix-rs` = **191 passed**（基线 193：-5 sef 旧测试
> +1 deferred fail-closed +1 N2 心跳 +1 N6 Label +1 N11 argv0 -1 N8 死代码测试）；
> `cargo check -p minix-rs` / `cargo clippy -p minix-rs --lib` / `cargo fmt -p minix-rs -- --check`
> 全部干净；`tools/check-rs-unwired.sh` PASS（3 个生产占位符均带文档契约）。
> 注：workspace 级 `cargo check --workspace` 的 riscv64 qemu 测试内核报
> `minix_plat::riscv64` 特性门控错误、`cargo fmt --check` 有 arch/boot.rs 历史差异——均非本轮文件。

| ID | 严重度 | 处置 | 代码改动 | 文档同步 |
|----|--------|------|---------|---------|
| N1 | P0-潜在 | ✅ 已修 | `table.rs` + `boot.rs` + `process_table.rs`/`access.rs` 调用点 | 03-rs-privilege.md §4.2 |
| N2 | P1 | ✅ 已修 | `monitor.rs` | 07-rs-period-heartbeat.md §3.1 |
| N3 | P1 | ✅ 已修 | `boot.rs` + `lib.rs`（`ServerState.machine`） | 01-rs-boot-init.md §3.4 |
| N4 | P1 | ✅ 已修 | `os/libs/minix-sys/src/lib.rs`（re-export） | 99-rs-global-concepts.md §4 |
| N5 | P1 | ✅ 已修（trait 化） | `sef.rs` + `lib.rs` + `main.rs` + `boot.rs` 测试 | 01-rs-boot-init.md §3.3/§4.1 |
| N6 | P2 | ✅ 已修 | `service_slot.rs` + `process_table.rs` + `request.rs` | 02-rs-process-table.md §3.8/§5.2 |
| N7 | P2 | ✅ 已修 | `privilege.rs`（from_calls→Result + 全构建 assert） | 03-rs-privilege.md §3.5 |
| N8 | P2 | ✅ 已修（删死代码） | `process_table.rs` + `lib.rs` re-export | 02-rs-process-table.md §3.9 + 16/14/17 交叉引用 |
| N9 | P2 | ✅ 已修 | `service_slot.rs` + `slot.rs` + `sched.rs` | 08-rs-slot-config.md §4 |
| N10 | P2 | ✅ 已修（并入 N1） | `table.rs` + `boot.rs`（u32/u16 双轨消除） | 03-rs-privilege.md §4.2 |
| N11 | P2 | ✅ 已修 | `slot.rs`（build_cmd_dep argv[0] 恒存在） | 08-rs-slot-config.md §2.5/§5 |

### Fix #14 — N1+N10：boot 表 flags 单一权威 + 精确位值测试（P0）
- **Before**：`table.rs:64-74` 自定义 u32 常量位值**全部错误**（`SRV_F=0x014`/`DSRV_F=0x016`/
  `RSYS_F=0x01C`/`VM_F=0x030`/`USR_F=0x005` vs C 真值 `0x012`/`0x01A`/`0x112`/`0x210`/`0x006`，
  priv.h:45-49 + const.h:143-154）；`BootImagePriv.flags: u32` → `boot.rs:493`
  `PrivFlags::from_bits_truncate(flags as u16)` 静默截断高位，错误位值无测试暴露。
- **After**：`BootImagePriv.flags: PrivFlags`、`BootImageSys.flags: SysFlags`（单一权威）；
  删除 table.rs u32 副本与 `SF_*` 重复常量；表项直接用 `privilege::RSYS_F/VM_F/SRV_F/USR_F` 与
  `service_slot::SRVR_SF/VM_SF`；boot.rs/process_table.rs 去掉 `as u16`/`from_bits_truncate`；
  `boot.rs:552` 改 `flags.contains(PrivFlags::SYS_PROC)`、`SYNCH_BOOT` 判定改
  `SysFlags::SYNCH_BOOT`；`table.rs::test_priv_table_matches_c` 加 12 表项**精确位值断言**
  （0x112/0x210/0x012×9/0x006）。
- **Verified**：`cargo test -p minix-rs` 通过；`rg "from_bits_truncate" servers/rs/src/` 仅剩
  `TrapMask::SRV_T`（测试）与 `SysFlags` 无生产消费；文档 03-rs-privilege.md §4.2。

### Fix #15 — N2：ping 间隔分支改用原始 r_period（P1）
- **Before**：`monitor.rs:127` 末分支用 `effective_period()`（INITIALIZING → `RS_INIT_T`）；
  对 boot 槽（`r_period=0`，main.c:338）C 每 tick ping（request.c:1035 `now - r_check_tm >
  r_period`），Rust 每 10 秒才 ping，节律差 10 倍。
- **After**：末分支改 `now.saturating_sub(rp.check_tm) > rp.period`（与 C 逐分支对齐）；
  `effective_period` 只用于 `period==0` 门与 answer-pending 超时（request.c:972/1004-1006）；
  新增 `test_initializing_zero_period_pings_every_tick`（INITIALIZING+r_period=0 → 每 tick
  `PingRequest`；非初始化 period-0 → `Nothing`）。
- **Verified**：`cargo test -p minix-rs period_` 3 passed；文档 07-rs-period-heartbeat.md §3.1。

### Fix #16 — N3：Machine 纳入 ServerState（P1）
- **Before**：`ServerState` 无 machine；`KernelApi::get_machine` 生产路径零调用
  （`rg "get_machine" servers/rs/src/` 仅 trait/mock）；13 的 `check_request` CPU 解析
  （request.c:1286-1296）届时无快照可读。
- **After**：`BootInit` 增 `machine: Machine` 字段，`step0_prepare` 一次性
  `sys.get_machine()?`（对齐 C main.c:53 位置——SEF startup 后、主循环前），`into_state`
  移交 `ServerState.machine`；handover 测试断言 `state.machine` 存活。
- **Verified**：`cargo test -p minix-rs handover` 通过；文档 01-rs-boot-init.md §3.4。

### Fix #17 — N4：minix-sys Errno 统一为 minix-types（P1）
- **Before**：`minix-sys` 自有 `pub enum Errno`（22 变体 `Eperm` 命名），缺
  `ENOSYS/EDEADEPT/EDONTREPLY/EGENERIC/ERESTART`（errno.h:78/211/199/200/196）；
  与 `minix_types::Errno` 双类型并存，19 接线每处 `KernelApi` 边界都要桥接。
- **After**：`pub use minix_types::Errno;`（Redox `redox_syscall::Error` 单 ABI 定位）；
  删本地 enum + 旧测试，新 `test_errno_values` 断言 `EPERM/EINVAL/ENOSYS` 经共享类型可达；
  命令 crate（`use minix_sys::*`）无旧变体使用，不受影响。
- **Verified**：`cargo test -p minix-sys` 1 passed；`cargo check -p minix-cat -p minix-init
  -p minix-sh` 通过；文档 99-rs-global-concepts.md §4。

### Fix #18 — N5：SefCallbacks 从 fn 指针结构体 trait 化（P1）
- **Before**：7 个裸函数指针回调无法携带状态；`RsServer.callbacks` 标
  `#[allow(dead_code)]`；`init(Fresh)` 绕过回调表直接调 `BootInit::init_fresh`（与 C
  `sef_startup()` → 回调派发结构不一致）。
- **After**：`pub trait SefCallbacks`（7 方法，`&mut self` 携带状态，单线程用户态模型）；
  `impl SefCallbacks for RsServer`——`init_fresh` = 四步 boot + handover，其余 6 个 fail-closed
  （`Err(ENOSYS)`/`ENOSYS.to_i32()`，`signal_handler` 保留带 06 契约的 `unimplemented!`）；
  `RsServer::new/with_kernel` 删除 callbacks 参数；`init()` 按 `SefInitType` 经 trait 分派
  （= C `sef_startup()`）；`main.rs`/`boot.rs` 测试同步。新测试
  `test_deferred_callbacks_fail_closed` 锁死 12/18/06 未落地前的失败形态。
- **Verified**：`cargo test -p minix-rs` 通过；`tools/check-rs-unwired.sh` PASS；
  文档 01-rs-boot-init.md §3.3/§4.1。

### Fix #19 — N6：Label 相等语义 strcmp 化 + from_bytes 尾 NUL（P2）
- **Before**：派生 `PartialEq` 16 字节整体比较（嵌入 NUL + 填充字节 → 误判不等）；
  `PartialEq<&str>` 依赖 `as_str()` UTF-8（非 UTF-8 label 恒不等，`check_duplicates` 的
  `unwrap_or("")` fail-open）；`from_bytes` 满 16 字节无 NUL（C strlcpy 截 15+NUL）。
- **After**：`from_bytes` = strlcpy（≤15 字节 + 强制尾 NUL）；手写 `PartialEq`（遇 NUL 截断）
  + `Eq` + 与 strcmp 一致的 `Hash`；`PartialEq<&str>` 字节级；`lookup_by_label` 改收
  `&Label`（`check_duplicates` 直接传 label，删 `unwrap_or("")`）。新增
  `test_label_eq_strcmp_semantics` + 16 字节边界断言。
- **Verified**：`cargo test -p minix-rs` 通过；`rg "unwrap_or\(\"\"\)" servers/rs/src/request.rs`
  = 0；文档 02-rs-process-table.md §3.8/§5.2。

### Fix #20 — N7：CallMask/SysMap 移位越界 fail-closed（P2）
- **Before**：`set_bit`/`test_bit`/`set`/`test` 用 `1u64 << offset`——`offset >= 64` 时 debug
  panic、**release 按 x86 shl 语义静默 mask 成 `offset & 63` 写错位**（调用掩码 = 允许错误
  syscall）；唯一守卫是 `from_calls` 的 `debug_assert!`（仅 debug）。
- **After**：`from_calls -> Result<CallMask, Errno>`，越界调用号（负偏移或
  `>= tot_nr_calls`，可来自 RS_UP 消息）→ `Err(EINVAL)`；`(1u64 << tot_nr_calls) - 1` 在
  `>= 64` 时用 `u64::MAX`；`set_bit/test_bit/set/test` 加**全构建** `assert!(offset < 64)`。
  测试：越界正/负调用号 → `Err(EINVAL)`；`tot_nr_calls=64` 全 1 不溢出。
- **Verified**：`cargo test -p minix-rs` 通过；文档 03-rs-privilege.md §3.5。

### Fix #21 — N8：删除 RupdateDescriptor 死代码（P2）
- **Before**：`RupdateDescriptor`（process_table.rs）全 crate 无生产使用者，与 `UpdateChain`
  （live_update.rs）双建模同一 C `struct rupdate`（type.h:43-54）——16 落地时必然漂移。
- **After**：删 `RupdateDescriptor` + `test_rupdate_descriptor_new` + lib.rs re-export；
  `UpdateChain` 升格为唯一 rupdate 模型（`len()` = `num_rpupds`）；`RupdateFlags` 保留
  （live_update.rs/self_lifecycle.rs 消费）。`UpdateChain` 的 usize 索引是链内位置而非
  `SlotId`，16 落地如需 `ChainIdx` newtype 记为 P2 后续。
- **Verified**：`rg "RupdateDescriptor" servers/rs/src/` = 0；文档 02-rs-process-table.md
  §3.9 + 16/14/17 交叉引用。

### Fix #22 — N9：IoRange/NR_SCHED_QUEUES 单一权威（P2）
- **Before**：`IoRange` 在 service_slot.rs 与 privilege.rs 各定义一份同形异构 struct；
  `NR_SCHED_QUEUES` 在 slot.rs 与 sched.rs 各定义一次（一方上调即悄悄分裂）。
- **After**：service_slot.rs `pub use crate::privilege::IoRange;`（删本地副本，D6 类型层收口）；
  `NR_SCHED_QUEUES` 唯一在 sched.rs（slot.rs `use crate::sched::NR_SCHED_QUEUES;`）。
- **Verified**：`rg "pub struct IoRange" servers/rs/src/` = 1；`rg "pub const NR_SCHED_QUEUES"
  servers/rs/src/` = 1；文档 08-rs-slot-config.md §4。

### Fix #23 — N11：build_cmd_dep argv[0] 恒存在（P2）
- **Before**：空/纯空格命令 → Rust `argc=0`；C 无条件 `r_argv[0] = r_args`
  （manager.c:299-300）→ `argv=[""]`（`argc=1`，exec 空路径 ENOENT）。
- **After**：`build_cmd_dep` 无 token 时返回 `vec![&[]]`；`rebuild_args` 因此恒写
  `args[0]=0`、`argc>=1`，与 C 失败形态对齐。新增
  `test_build_cmd_dep_empty_cmd_keeps_argv0`（空串/纯空格/NUL 开头三边界）。
- **Verified**：`cargo test -p minix-rs build_cmd_dep` 通过；文档 08-rs-slot-config.md
  §2.5/§5。

### 遗留（未修，属后续轮次/接线期）
- E1/E3：四份测试 mock 未合并共享 `testutil`（P2，19 前）。→ ✅ §13（T5 后仅剩 boot shell mock，已迁 `testutil.rs`）
- E2：proptest property 测试未补（P2，接线期）。
- T5：`&mut dyn KernelApi` 注入边界未统一（P2，19 前）。→ ✅ §13（monitor 模式统一，纯模块不再 import `KernelApi`）
- D1/D2/D6：`ServiceSlot` god struct / `SlotId` 世代 / io-irq 双表（P2，结构性）。
- N8 子项：`UpdateChain` 的 `ChainIdx` newtype（P2，16 落地时）。
- T3 残留：`boot.rs` lookup 失败与 getnpid 负值共用 `ENOSYS`（19 接线时区分 `BootError`）。

---

## 13. 第四轮修复记录（2026-08-16 — T5 注入边界 + E1 testutil 收敛）

> 本轮按 §12 遗留项中的"19 前"优先级修复 **T5**（注入边界统一为 monitor 模式）与 **E1**
> （测试 mock 收敛），文档与代码一并（fix-guard.md：每处修复前读目标 ±5 行 + `rg` 确认）。
> **修复后基线**：`cargo test -p minix-rs` = **191 passed**（测试数不变——access/ipc/sched
> 测试从 mock 驱动改为纯数据/闭包驱动，数量与语义等价）；
> `cargo fmt -p minix-rs -- --check` 干净；`cargo clippy -p minix-rs --lib` 零告警；
> `rg "dyn KernelApi" os/servers/rs/src/` 仅剩 boot shell（BootInit 步骤/self_update）
> 与 `RsServer` 持有者（lib.rs）——纯决策模块零 `KernelApi` import。
> 注：`cargo test --no-run` 的 minix-rs 告警仅剩 exec.rs 历史两项（未在本轮 scope，见下）。

### Fix #24 — T5：`&mut dyn KernelApi` 注入边界统一为 monitor 模式（P2，19 前定）
- **File**：`os/servers/rs/src/access.rs`、`ipc_mask.rs`、`sched.rs` + 04/05/03/01/99/18 文档
- **Before**：三种注入风格并存——access/ipc_mask/sched 把 `&mut dyn KernelApi` 作为纯函数参数
  （`check_call_permission(..., sys)`、`add_forward_ipc(rp, table, sys)`、`sched_init_proc(cfg, is_sys, sys)`、
  `update_sig_mgrs(priv_, sys, ...)`），monitor/ready/recovery 走"结果枚举 + 调用方执行副作用"。
- **After**（monitor / functional-core-imperative-shell 模式，Redox "用户态逻辑与 syscall 分离"同款）：
  - 查询注入：`access::caller_is_root(euid: Result<u32, Errno>)`、`check_call_permission(..., caller_euid)`
    ——shell（19 接线层）执行 `sys.getnuid(caller)` 一次并传入；`ipc_mask::add_forward_ipc/init_privs/
    update_ipc_mask` 改收 `priv_id_of: impl FnMut(Endpoint) -> Option<PrivId>` 闭包（惰性——仅
    SYSTEM/USER 伪名出现时查询，C 的按需 `sys_getpriv` 等价）。
  - 命令效果：`sched::sched_decision(cfg, is_sys) -> SchedAction::{Skip, Start(&cfg)}`（shell 执行
    `Start` 并取 `*newscheduler_e`）；`sched::set_sig_mgrs(priv_, synced, ep, sig_mgr, bak) ->
    SigMgrCommit`（shell 执行 `privctl(UpdateSys, commit.priv_)`）。
  - **边界清单**：`KernelApi` 只出现在 boot 四步（shell）、`RsServer.kernel`（lib.rs）、19 接线层。
- **Verified**：`rg -l "KernelApi" servers/rs/src/access.rs ipc_mask.rs sched.rs` = 仅注释；
  `cargo test -p minix-rs access:: ipc_mask:: sched::` 22 passed；文档 §3 同步。

### Fix #25 — E1：测试 mock 收敛为单一共享 `testutil::MockKernelApi`（P2，19 前）
- **File**：`os/servers/rs/src/testutil.rs`（新建）、`boot.rs`、`lib.rs`
- **Before**：四份同形 mock——boot.rs `MockKernelApi`（记录调用 + hz/ticks/pids）、sched.rs
  `MockSys`（记录 privctl/getpriv/sched）、access.rs `MockSys`（getnuid 三态）、ipc_mask.rs
  `MockSys`（getpriv 固定 id）；每份 15 方法 `impl KernelApi` 重复。
- **After**：T5 后 access/ipc_mask/sched 为纯决策函数，测试改纯数据/闭包驱动、**零 mock**；
  仅 boot shell 测试仍需记录型 mock——收敛为 `testutil::MockKernelApi`（`Call` 枚举含
  `GetNuid(Endpoint)` 记录，补齐未来 19 shell 测试需要），`boot.rs` 测试 `use crate::testutil::{Call, MockKernelApi}`。
- **Verified**：`rg "struct Mock" servers/rs/src/` = 1（仅 `testutil::MockKernelApi`，原四份收敛为一）；
  `rg "impl KernelApi" servers/rs/src/` 仅 testutil.rs + boot.rs 生产 impl（`UnimplementedKernelApi`）；
  `rg "MockSys" servers/rs/src/` 仅 ipc_mask.rs 一处注释（提及已删 mock）；191 passed。

### 遗留（未修，属后续轮次/接线期）
- E2：proptest property 测试未补（P2，接线期）。
- D1/D2/D6：`ServiceSlot` god struct / `SlotId` 世代 / io-irq 双表（P2，结构性，另行立项）。
- N8 子项：`UpdateChain` 的 `ChainIdx` newtype（P2，16 落地时）。
- T3 残留：`boot.rs` lookup 失败与 getnpid 负值共用 `ENOSYS`（19 接线时区分 `BootError`）。
- exec.rs 测试模块历史告警（unused `Endpoint` import + 未用 `slot_with_image`，`cargo test --no-run`）——
  非本轮 scope，18/10 接线时随手清理。

---

## 14. 深度 review（2026-08-16 — 自顶向下 + Redox/OS/Rust 最佳实践对照）

> **scope**：仅 `os/servers/rs/src/` Rust 实现（25 模块 ~9.5k 行），不做文档 review。Ground truth 按
> 优先级链 `minix3/` C 源码 > design > Rust。对照对象：Redox（init 声明式服务管理、redox_syscall 边界、
> 无全局进程表）、OS 最佳实践（看门狗/心跳、显式状态机、恢复策略）、Rust 社区最佳实践
> （functional-core/imperative-shell、newtype、fail-closed、property-based 测试）。
> **基线**：`cargo build -p minix-rs` OK；`cargo test -p minix-rs` = **191 passed**；
> `cargo clippy -p minix-rs --lib` 零告警。生产路径全部 fail-closed
> （`UnimplementedKernelApi` 全 ENOSYS、`get_work()` 为 `todo!()`、`main()` 未接线即 panic），
> 19-rs-external-interfaces.md 接线前**无 P0 运行时缺陷**。
> **结论**：架构方向（pure-functional-core / imperative-shell，T5 已定）正确且是加分项；本轮发现
> **1 个 P1 语义偏移（R1，monitor 超时期计算，含错误测试断言与错误注释）**、若干 19 前须定的
> P2 决策（R2/R6/R11）与结构性立项（R7-R10，衔接既有 D1/D2/D6）。R3 为已核实正确项（防误判记录）。

### 14.1 问题列表（R1..Rn）

- **R1（P1 语义偏移 + P0-fact 注释）— `monitor::effective_period` 对 LU 初始化态返回错误的超时期**
  - C 证据：`UPD_INIT_MAXTIME(RPUPD)` — const.h:116 =
    `prepare_maxtime != RS_DEFAULT_PREPARE_MAXTIME ? prepare_maxtime : RS_INIT_T`；
    `RS_INIT_T = system_hz*10`（const.h:48）、`RS_DEFAULT_PREPARE_MAXTIME = 2*RS_DELTA_T`（const.h:58）；
    使用点 request.c:965-969：`period = SRV_IS_UPDATING(rp) ? UPD_INIT_MAXTIME(&rp->r_upd) : RS_INIT_T;`
  - Rust 现状：`monitor.rs:62-74` `effective_period` 对 `INITIALIZING|UPDATING` 直接返回
    `default_prepare_maxtime(hz)` = 2×RS_DELTA_T（@hz60=120）；`monitor.rs:67` 注释称
    "the default is `RS_DEFAULT_PREPARE_MAXTIME`"（漏掉 C 的替代分支）；`monitor.rs:208` 测试
    `assert_eq!(effective_period(&s, 60), 120)` 断言了错误语义。
  - 偏差：**C 的默认分支返回 `RS_INIT_T` = hz×10（@hz60=600）**，只有 prepare_maxtime 被显式
    设置为非默认值时才用 prepare_maxtime。Rust 未建模 `prepare_maxtime`（16 文档 update descriptor），
    默认分支应回落 `init_timeout(hz)`。影响：LU 初始化心跳超时窗口 600→120（缩小 5 倍），
    `2*period` 超时判定（request.c:1004-1028）随之偏移。
  - 改进思考：`effective_period` 增参 `prepare_maxtime: Option<i64>`，实现
    `upd_init_maxtime(hz, pm)`：`pm` 存在且 `!= default_prepare_maxtime(hz)` 时返回 `pm`，否则返回
    `init_timeout(hz)`——忠实 C 的"不等于默认值才用覆盖值"语义（不是"有值就用"）；同步修正注释；
    测试补 `UPDATING` 默认分支 600@hz60 + 显式覆盖分支。19 接线前必修。

- **R2（P2 类型/ABI，19 前定）— `Privilege` 计数域 `u16` vs C `int`**
  - C 证据：`kernel/priv.h:53/56/59` `int s_nr_io_range / s_nr_mem_range / s_nr_irq`；
    `servers/rs/type.h:101-103` `int r_nr_io_range / r_nr_irq`（注意无 `r_nr_mem_range`，不对称）；
    `kernel/system/do_privctl.c:309` 校验 `s_nr_io_range < 0 || > NR_IO_RANGE`。
  - Rust 现状：`privilege.rs:359/363/367` `pub nr_io_range/nr_mem_range/nr_irq: u16`。
  - 影响：`NR_IO_RANGE=64 / NR_MEM_RANGE=20 / NR_IRQ=16`（include/minix/config.h:52/55/58）均
    < 65536，无截断风险；但 C 的 `int` 含"校验前可负"状态（do_privctl 用负值判非法），`u16` 丢失该
    状态，19 接线 `sys_getpriv` 时若直接赋值会把非法负值包成巨大正数。
  - 改进思考：`i32` + 构造时校验 `0..=NR_*`（fail-closed），或 newtype
    `RangeCount` + `TryFrom<i32>`；与 `s_io_tab/s_mem_tab/s_irq_tab` 数组长度约束绑定，消除
    "计数与表长不一致"的非法状态。

- **R3（已核实正确，防误判记录）— `normalize_lu_response` 的 `EDONTREPLY→EGENERIC` 与 C 一致**
  - C 证据：`sef_cb_lu_response` — main.c:613-621：`r = do_upd_ready(m_ptr); if (r == EDONTREPLY) r = EGENERIC;`
    ——与 Rust `ready.rs:223-227` 完全一致。注意与 `sef_cb_init_response`（main.c:591-607，
    `EDONTREPLY→OK`）**相反**：同一哨兵在两个回调中语义不同（init=成功、lu=失败）。
  - Rust 已正确拆分 `normalize_init_response`（`ready.rs:214`，EDONTREPLY→0）与
    `normalize_lu_response`（`ready.rs:223`，EDONTREPLY→EGENERIC），`ready.rs:387-402` 测试断言
    均正确。无需改动；19 接线时不要"统一"这两个归一化。

- **R4（P2 健壮性）— `do_init_ready` 的 pending 计数 `saturating_sub(1)` 掩蔽下溢**
  - C 证据：request.c:506-513 `rupdate.num_init_ready_pending--`；main.c:586
    `assert(rupdate.num_init_ready_pending > 0)`——C 把 0→underflow 视为程序错误。
  - Rust 现状：`ready.rs:141` `pending_remaining: pending.saturating_sub(1)` 静默吞掉非法状态。
  - 改进思考：`debug_assert!(pending > 0)`（fail-fast，测试期暴露）或返回 `Result`；更优是语义类型
    `NonZeroUsize` 计数器，把"0 时不可递减"编进类型。

- **R5（P2 健壮性）— `RProcTable::get/get_mut` 越界直接 panic**
  - Rust 现状：`process_table.rs:123/128` `&self.slots[id.0]`——越界即 panic，且 `SlotId` 无世代
    （见 R8），free→reuse 后旧 id 可指向**别的** slot 而不越界（更隐蔽）。
  - C 对照：C 用 `rproc_ptr[_ENDPOINT_P(ep)]` 指针，交换后始终指向有效内存（但内容可能错位，
    swap_slot 的第三方链即如此，见 R7）。
  - 改进思考：把链遍历/索引访问收敛为带不变量校验的私有方法（越界→panic 可接受，但必须同时校验
    世代/占用位）；对外 API 保持 fail-closed 或返回 `Option`。

- **R6（P2 测试基建，19 前定）— `testutil::MockKernelApi` 四个 `unimplemented!()`**
  - Rust 现状：`testutil.rs:101/105/115/124`——`srv_fork/getprocnr/vm_memctl/vm_set_priv` 为
    `unimplemented!()`，注释"Not reached by current tests; wired at 19"。19 接线后 boot shell 测试
    一旦命中即 panic（且 panic 在测试中难以定位调用栈意图）。
  - 改进思考：改为 `Err(ENOSYS)`（fail-closed，与生产 `UnimplementedKernelApi` 一致），并提供
    `expect_call` 辅助（断言某调用发生且返回指定值）；与生产 impl 行为对齐，panic 只留给真正
    "不可能发生"的路径。

- **R7（P2 结构性，衔接 D1）— `ServiceSlot` god struct + 链/索引一致性无校验**
  - Rust 现状：`service_slot.rs` 35+ 全 `pub` 字段；四链 `old_rp/new_rp/prev_rp/next_rp` +
    endpoint 快索引分散在 `process_table.rs`；`service_create.rs:198` `swap_slot` 只重定向
    src/dst 两行的自身链与 endpoint 索引，**第三方行指向 src/dst 的引用保持悬垂**——这是 C 的忠实
    再现（manager.c:1870-1932 同样只改 src/dst 行字段），测试明确断言了该行为。
  - 问题：指针式 C 中第三方 `rproc_ptr` 仍指向有效内存（内容错位但可读）；索引式 Rust 中悬垂引用
    是"旧行号"，配合 R8 无世代可指向完全无关的 slot——**忠实再现 C 的同时放大了 C 的潜在错误后果**。
  - 改进思考：拆 `ServiceConfig`（不可变配置）/`ServiceRuntime`（可变运行态）/`ServiceIdentity`
    （endpoint+label）；链操作收敛为私有方法；提供 `debug_assert_consistent()`（每行四链互相一致、
    endpoint 索引双向往返一致），swap 后全表校验（测试黄金路径用）。第三方链语义属 C 遗产，16 接线
    时以显式验证函数暴露，不要静默。

- **R8（P2 结构性，承接 D2）— `SlotId` 无世代**
  - 现状：`service_slot.rs:164` `pub struct SlotId(pub usize)`；free→reuse 后旧索引悬垂
    （配合 R5/R7 可指错 slot）。C 中 `rproc` 数组行从不复用（slot 有 `RS_IN_USE` 生命周期管理），
    Rust 的 `vacant()` 复用是设计选择，但无世代使复用不安全。
  - 改进思考：`SlotId { idx: u16, gen: u16 }`，`vacant()` 分配时 `gen += 1`；所有链/索引访问校验
    世代（O(1) 成本）。对照 Redox：句柄为 `Arc<dyn Scheme>`，所有权即有效性，无世代问题——若 19 后
    slot 生命周期仍复杂，可评估把链引用改为共享句柄（但需注意 no_std + 单线程下 `Rc` 即可）。

- **R9（P2 结构性）— `KernelApi` 15 方法单体 trait**
  - 现状：`Box<dyn KernelApi>`（lib.rs 持有），15 方法横跨 priv/sched/pm/vm/ipc 五域；测试 mock
    被迫实现全部（testutil.rs 即因此有 4 个 `unimplemented!()`，见 R6）。
  - 对照：Redox 无单体 syscall trait——`redox_syscall` 每个 syscall 独立函数，用户态逻辑与 syscall
    边界分离；Rust 社区惯例按域拆 trait（Interface Segregation）。
  - 改进思考：拆 `PrivApi/SchedApi/PmApi/VmApi/IpcApi` 五 trait + `dyn` 组合（或 19 接线层直接用
    具体类型）；测试 mock 按域实现，`unimplemented!()` 范围缩小到真正未接线的方法。

- **R10（P2 结构性）— pure/imperative 二分的"kernel 调用顺序"只存在于注释**
  - 现状：T5 后纯模块返回枚举/提交，但 shell（boot.rs、lib.rs、19 接线层）里 ~15 步调用序列
    （如 create_service 的 init/sched/privctl 顺序、`set_sig_mgrs` 的提交顺序）靠注释手抄 C 固定顺序，
    无类型化保证，改一个提交顺序编译器不报错。
  - 改进思考：引入 `KernelEffect` 有序列表（shell 逐条执行）或 step 状态机（每步返回下一步）；
    配 golden 顺序测试（把 C manager.c 的固定顺序固化为测试断言）。对照 Redox init：命令序列
    数据驱动（init.toml 声明），若 19 想声明式化需按 [ARCH] 三处标注。

- **R11（P2 决策，19 前定）— `SourceIpcFilterEl.m_label: &'a str` vs C 原始字节 `m_label[16]`**
  - C 证据：`struct ipc_filter_el`（include/minix/ipc.h）`char m_label[16]`——字节数组，比较用
    strcmp 语义；标签可含非 UTF-8 字节（RS_MAX_LABEL_LEN 语义）。
  - Rust 现状：`&'a str`（UTF-8 先验），非法 UTF-8 label 在解析边界被拒 = fail-closed（安全方向），
    但若内核侧 label 非 UTF-8，服务会不可匹配/不可见。
  - 改进思考：定策略二选一并在 19 文档固化：(a) 保持 `&str` + 文档声明"label 必须 UTF-8，
    非 UTF-8 拒绝"（fail-closed）；(b) 改 `&[u8]` + 字节比较（与 C 完全同构）。RS 场景 label 来自
    boot 配置（RS 自己控制），(a) 可接受，但需显式决策而非隐式。

### 14.2 Redox / OS 最佳实践对照

- **Redox init（声明式服务管理）**：Redox 用 initfs 配置声明服务（name/deps/args/uid/restart 策略），
  启动顺序数据驱动；Minix RS 的 boot 序列是 C 硬编码（manager.c），Rust 已把决策纯函数化但顺序仍
  手抄（R10）。建议 19 时对照评估：恢复策略类型化为 `RestartPolicy` 枚举（Always/OnExit/Manual +
  backoff 参数），替代散落的 `r_backoff/restart_service/crash_service` if-else。
- **Redox syscall 边界**：`redox_syscall` 每 syscall 独立函数；对应 R9 拆域 trait。另：minix-types
  已引入 `Errno` newtype（A-12）——建议边界全部转 `Errno` 而非裸 `i32`（现 `dispatch.rs:88`
  `DispatchResult(pub i32)` 仍裸 i32 + `is_reply_suppressed()` 特判；可改为 `Result<(), Errno>` +
  `ReplyPolicy` 分离，把"是否回复"从错误值中解耦）。
- **OS 看门狗/心跳**：C RS 的 `period + 2×period` 超时是看门狗模式；Rust `PeriodAction` 枚举已把
  判定类型化（加分项）。建议把"超时窗口 = 2×period"与 `MAX_BACKOFF=30` 作为命名常量/策略类型
  显式化，避免散落 magic。monitor.rs:129 已注意到 `RS_INIT_T` 期间逐 tick ping 的 C 怪癖，保持。
- **显式生命周期状态机**：C 用 RFlags 组合表达状态（INITIALIZING|UPDATING 等，合法组合是子集）；
  对照 systemd/Redox 的显式状态机，建议评估 `Phase` enum + 合法转换表（Booting→Initializing→
  Active→Stopping→Updating→Dead），在纯模块内拒绝非法组合（如 INITIALIZING+DEAD）。属 [ARCH] 讨论，
  需三处标注，不急。
- **查找结构**：label/endpoint 查找为 O(n) 线性扫 64 slot（`endpoint_slot` O(1) 快索引已有）。
  64 规模线性可接受（C 亦如此），不建议过度工程；若 19 后 label 查找成为热点，加
  `HashMap<Label, SlotId>` + 一致性校验（与 R7 的不变量一起）。

### 14.3 Rust 社区最佳实践对照

- **functional-core / imperative-shell（已落地，加分项）**：T5 后纯模块零 `KernelApi` import，
  副作用收敛到 shell。继续坚持：纯函数只返回数据/枚举，Kernel 调用顺序由 19 层显式编排（R10）。
- **newtype 化**：已有 `CallMask(u64)`、`Label`（strcmp 语义）、`SlotId`、`SefInitType`；
  建议补齐 `Period(i64 tick)`/`Hz(u32)`（现 monitor 裸 `i32/u32` 参数，@hz60 魔法散落测试）、
  `PendingCount(NonZeroUsize)`（R4）。no_std 下零成本。
- **fail-closed 一致性**：生产 impl 全 ENOSYS（好）；但测试 mock 用 `unimplemented!()`（R6）、
  `saturating_sub`（R4）、裸 panic（R5）是"fail-open 或静默"缺口——统一为 Err/断言。
- **property-based 测试（E2 遗留）**：191 单测覆盖良好，但纯函数密集区适合补 proptest：
  `parse_label` 边界与幂等、`IpcListIterator` 覆盖性（与 C 位域表对照）、`check_request` 权限矩阵
  （对拍 C do_checkperms 逻辑）、`build_cmd_dep` 往返；R10 的 golden 顺序测试也归此类。
- **错误上下文**：no_std 无 anyhow；当前 `Result<(), Errno>` 丢失"哪个 slot/哪步"上下文。建议 19
  接线层包装 `RsError { errno, slot: SlotId, op: &'static str }`（仅 shell 层，纯模块保持裸 Errno）。

### 14.4 验证证据

- `cargo build -p minix-rs` OK；`cargo test -p minix-rs` = 191 passed（基线，未改动代码）。
- `cargo clippy -p minix-rs --lib` 零告警（minix-sys 的 stub warning 不在 scope）。
- 关键 C 证据复核：`sed -n '955,995p' minix3/minix/servers/rs/request.c`（period 计算）、
  `sed -n '110,120p' minix3/minix/servers/rs/const.h`（UPD_INIT_MAXTIME）、
  `sed -n '585,630p' minix3/minix/servers/rs/main.c`（sef_cb_init/lu_response）。
- 既有遗留衔接：R1 为**新发现 P1**；R7/R8 与 D1/D2 合并立项；E2（proptest）在 R10/R7 处获得
  具体测试点。建议下一轮优先：R1 修复（19 前）→ R6/R11 决策 → R7-R10 结构性立项。

---

## 15. 第五轮修复记录（2026-08-16 — §14 深度 review R1/R2/R4/R5/R6/R11）

> 按 §14 问题列表修复。fix-guard.md 全流程：每处修复前读目标 ±5 行 + `rg` 确认现状，单条修复，
> 修后 `grep`/测试验证。**修复后基线**：`cargo test -p minix-rs` = **195 passed**（191 + 4 新测试：
> R1×1、R2×1、R4×1、R5×1）；`cargo build -p minix-rs` OK；`cargo clippy -p minix-rs --lib`
> 对 `servers/rs/src/` 零告警（依赖 crate minix-types/minix-sys 的历史 stub 告警不在 scope）；
> `cargo fmt -p minix-rs -- --check` 干净。
> **结构性立项 R7-R10（与 D1/D2/D6 合并）本轮未动代码**：R7 拆 god struct、R8 SlotId 世代、
> R9 KernelApi 拆域 trait、R10 KernelEffect 顺序类型化——均需 19 接线层定形后单独立项
> （R5 已先做范围断言，R8 世代落地后补"界内但过期"校验）。

### Fix #26 — R1（P1 语义偏移）：`monitor::effective_period` LU 初始化超时期错误
- **File**：`os/servers/rs/src/monitor.rs` + 07 文档 + 07-design
- **Before**：`effective_period` 对 `INITIALIZING|UPDATING` 返回 `default_prepare_maxtime(hz)`（@hz60=120）；
  注释称 "the default is `RS_DEFAULT_PREPARE_MAXTIME`"；测试断言 120——C `UPD_INIT_MAXTIME`
  （const.h:116）默认分支应返回 `RS_INIT_T`（@hz60=600）。
- **After**：新增 `upd_init_maxtime(hz, prepare_maxtime: Option<i64>)`——覆盖值 ≠ 默认才生效，
  否则 `init_timeout(hz)`（忠实 C 的"不等于默认值才用覆盖"语义）；`effective_period` UPDATING 分支
  调 `upd_init_maxtime(hz, None)`（prepare_maxtime 未建模，16 DEFERRED）；注释修正；测试
  `test_effective_period_initializing` 改 600 + 新增 `test_upd_init_maxtime`（None/显式覆盖/==默认 三态）。
- **Verified**：`cargo test -p minix-rs monitor::` = 11 passed；`rg "upd_init_maxtime" os/servers/rs/src/monitor.rs`
  → 定义 + 调用 + 测试 4 处一致；文档 07 §2.1/§4 + 07-design 常量表/effective_period 同步。
- **设计注**：`Option<i64>` 表达"未建模/未设置"（None = C 默认分支），16 落地时接线层传入 descriptor 值，
  不破坏 `period_decision` 签名——避免机械 translate C 三元表达式。

### Fix #27 — R2（P2 类型/ABI，19 前定）：`Privilege` 计数域 `u16` → `i32` + fail-closed 校验
- **File**：`os/servers/rs/src/privilege.rs` + 03 文档
- **Before**：`nr_io_range/nr_mem_range/nr_irq: u16`——C 是 `int`（priv.h:53/56/59）且 `sys_privctl`
  拒绝负值（do_privctl.c:308-309/319-320/330-331），`u16` 丢失"校验前可负"状态，且与同 crate
  `ServiceSlot.r_nr_io_range: i32`（type.h:101-103）不一致。
- **After**：三字段 `i32`（与 C 宽度一致）；新增 `Privilege::validate() -> Result<(), Errno>`——
  `0..=NR_IO_RANGE/NR_MEM_RANGE/NR_IRQ` 门（do_privctl.c 同语义 EINVAL），19 接线 `data_copy` 前调用；
  测试 `test_validate_range_counts`（零值/上限合法；超限/负值 EINVAL）。
- **Verified**：`cargo test -p minix-rs privilege::` = 12 passed；`rg "nr_io_range" os/servers/rs/src/privilege.rs`
  → 3 处 `i32` + validate；03 文档 §3.2 代码块 + R2 注记同步。
- **设计注**：保留定长数组 + 计数（与 kernel ABI 对齐，sys_privctl 整块拷贝），计数合法性用边界
  validate 表达而非 newtype（三个上限不同，newtype 需三份，收益低——务实选择）。

### Fix #28 — R4（P2 健壮性）：`do_init_ready` pending 下溢从静默饱和改为 fail-fast
- **File**：`os/servers/rs/src/ready.rs` + 12 文档
- **Before**：`pending.saturating_sub(1)` 静默吞掉 0→underflow（C 调用方保持
  `num_init_ready_pending > 0`，main.c:586 assert）。
- **After**：`debug_assert!(pending > 0, "do_init_ready: pending underflow")` + `pending - 1`；
  测试 `test_do_init_ready_pending_underflow_panics`（#[should_panic] 锁死语义）。
- **Verified**：`cargo test -p minix-rs ready::` = 13 passed；`rg "pending - 1" os/servers/rs/src/ready.rs`
  → 1 处 + debug_assert；12 文档测试清单第 5 项补充 R4 注记。
- **设计注**：Rust 惯用 fail-fast（非法状态不静默）；与 C release 宏递减行为一致（debug 断言，
  性能零成本）。

### Fix #29 — R5（P2 健壮性）：`RProcTable::get/get_mut` 越界 panic 带上下文断言
- **File**：`os/servers/rs/src/process_table.rs` + 02 文档
- **Before**：裸切片索引 `&self.slots[id.0]`（越界 panic 无上下文）。
- **After**：显式 `assert!(id.0 < len, "...out of range...")`——panic 点带槽号/表长上下文；
  注释说明 R8 世代落地后补"界内但过期"校验；测试 `test_get_rejects_out_of_range_id`。
- **Verified**：`cargo test -p minix-rs process_table::` = 17 passed；`rg "out of range" os/servers/rs/src/process_table.rs`
  → get/get_mut 两处 + 测试；02 文档 §2.2 悬垂注记 + §3.4 表格同步。
- **设计注**：保留索引语义（调用点多为链/索引操作，Option 化会扩散错误处理）；范围断言 + 未来世代
  是 Rust 的"调试期暴露 + release 低开销"平衡。

### Fix #30 — R6（P2 测试基建，19 前定）：`testutil::MockKernelApi` 四个 `unimplemented!()` → `Err(ENOSYS)`
- **File**：`os/servers/rs/src/testutil.rs`
- **Before**：`srv_fork/getprocnr/vm_memctl/vm_set_priv` 为 `unimplemented!()`——19 接线后 shell 测试
  命中即 panic。
- **After**：四方法返回 `Err(Errno::ENOSYS)`（fail-closed，与生产 `UnimplementedKernelApi` 一致）；
  注释说明 19 接线 shell 可在落地后 stub canned 结果。
- **Verified**：全量 `cargo test -p minix-rs` = 195 passed（无测试依赖这四方法，行为不变）；
  `rg "unimplemented!" os/servers/rs/src/` 仅剩 boot.rs:642（self-upgrade，18 文档化占位）。
- **设计注**：mock 与生产 fail-closed 行为对齐，panic 只留给真正不可能路径（review-core 检查点 4）。

### Fix #31 — R11（P2 决策，19 前定）：label UTF-8 契约——保持 `&str` + 边界 fail-closed
- **File**：`os/servers/rs/src/state_data.rs` + 17 文档
- **Before**：`SourceIpcFilterEl.m_label: &'a str` 无契约说明（C `m_label[16]` 是原始字节，rs.h:90）。
- **After**：**决策 = 保持 `&str`**；`SourceIpcFilterEl`/`parse_label` 注释声明 UTF-8 契约（19 消息边界
  `from_utf8` 校验，非 UTF-8 拒绝）；17 文档 §3.2 追加 R11 决策注记（DS label 为服务名、ASCII 实践、
  `str::parse::<i32>()` 保持类型安全十进制解析、拒绝方向安全）。
- **Verified**：`cargo test -p minix-rs state_data::` = 9 passed；`rg "UTF-8" os/servers/rs/src/state_data.rs`
  → m_label + parse_label 两处注释。
- **设计注**：改 `&[u8]` 需手写 strtol 字节解析（translate 反模式）且无真实非 UTF-8 场景；
  fail-closed 拒绝不静默错配——Rust 类型系统表达 C 字节语义的务实边界。

### 遗留（未修，属后续轮次/立项）
- R7-R10：结构性立项，与 D1/D2/D6 合并（19 接线层定形后另行推进；R5 范围断言已先行）。
- E2：proptest property 测试未补（P2，接线期；§14.3 已给出具体测试点）。
- T3 残留：`boot.rs` lookup 失败与 getnpid 负值共用 `ENOSYS`（19 接线时区分 `BootError`）。
- 附带观察：`slot.rs` `RsStart` 缺 `rss_nr_io`（C rs.h:124 有 `int rss_nr_io`）、
  `nr_irq: usize` vs C `int`（rs.h:122）——08-rs-slot-config 接线时核对。→ ✅ §17（Fix #39）

---

## 16. 独立深度 review（2026-08-16 第三轮 — 自顶向下 + C 逐条对照 + Redox/OS/Rust 实践追加）

> **scope**：仅 `os/servers/rs/src/` Rust 实现（25 个 .rs 文件 9,642 行：24 模块 + main.rs），全量通读 + 与 `minix3/` C 源码
> 逐条对照；不 review 文档。对照对象：Redox（查找全量性、scheme 消息枚举、panic 纪律）、OS 实践
> （看门狗副作用归属、启动编排状态迁移）、Rust 社区（functional-core 一致性、totality、
> 类型化副作用载荷）。
> **基线（本轮）**：`cargo check -p minix-rs` 通过；`cargo test -p minix-rs` = **195 passed**；
> 生产占位符仅 3 处（boot.rs:642 self-upgrade / lib.rs:243 get_work / lib.rs:286 signal_handler，
> 均带 fail-closed 文档契约，`tools/check-rs-unwired.sh` PASS）。
> **结论**：前几轮（T/D/S/E/N/R1-R11）已覆盖大部分结构性问题；本轮新增 **1 个 P1 totality（R12）**、
> **2 个 P1 设计缺口（R13 副作用载荷、R14 init_service 变异未建模）**、**5 个 P2（R15-R19）** +
> Redox/OS/Rust 实践对照（R20）。R12-R14 应在 19 接线前定；R16 可立即修。

### 16.1 问题列表（R12..R19）

- **R12（P1 健壮性/totality）— `RProcTable::endpoint_slot`/`set_endpoint_index` 对越界 endpoint 直接索引 → 越界 panic**
  - C 证据：主循环在触碰 `rproc_ptr[who_p]` 前先 `rs_isokendpt`（main.c:63-66，bogus source → panic）；
    `caller_can_control` 是**有界线性扫描**（manager.c:52-60），任意 endpoint 值安全。
  - Rust 现状：`process_table.rs:150-156` `endpoint_slot` 直接 `self.by_endpoint[slot as usize]`，
    `:163-169` `set_endpoint_index` 同——`by_endpoint` 长 `NR_PROCS=256`，而 `Endpoint::NONE` 的
    `slot()` = 31743（endpoint.rs：`ENDPOINT_SLOT_TOP-2`）、`ANY`=31744、`SELF`=31741，全部越界 panic。
    可达面：`access.rs:46` `caller_can_control(caller=NONE/ANY/…)`（pub、消息处理可达）；
    `service_create.rs:224/227` `swap_slot` step 5 对含 `NONE` endpoint 的行（`clone_slot` 产物，
    service_create.rs:106 置 `Endpoint::NONE`）→ `set_endpoint_index(NONE,…)` panic。
  - 影响：A-4 快索引把 C 的全量安全线性扫换成 O(1) 但**非全量**；D3 修了计数越界，本项是同一
    totality 主题在索引层的遗漏。C 主循环靠 isokendpt 前置保证；Rust `run()` 骨架无此前置。
  - 改进思考：`endpoint_slot`/`set_endpoint_index` 增加 `(0..NR_PROCS).contains(&slot)` 门（越界
    → `None`/忽略，fail-closed，与 D3 同风格）；06 主循环接线时在 `classify` 前补 `isokendpt`
    校验（C main.c:63-66 语义）；补 `test_endpoint_slot_none_fails_closed` +
    `test_swap_slot_with_vacant_row_no_panic`。对照 Redox：scheme 消息源由内核保证，但用户态
    解析层仍对任意输入全量（Resource 查找永不 panic）。

- **R13（P1 设计缺口）— 决策枚举不携带槽位副作用：monitor/ready 与 recovery/service_create 模式不一致**
  - C 证据（逐条）：
    | C 位置 | C 副作用 | Rust 决策 | 载荷 |
    |---|---|---|---|
    | request.c:1022-1028 PingTimeoutCrash | `r_flags \|= RS_NOPINGREPLY`；INITIALIZING 时 `r_init_err = EINTR` | `PeriodAction::PingTimeoutCrash`（monitor.rs:135） | 无 |
    | request.c:985-989 StopTimeoutCrash | `r_stop_tm = 0` | `StopTimeoutCrash`（monitor.rs:115） | 无 |
    | request.c:975-978 BackoffTick | `r_backoff -= 1` | `BackoffTick`（monitor.rs:109） | 无（靠 shell 记得减 1） |
    | request.c:1035-1037 PingRequest | `r_check_tm = now` | `PingRequest`（monitor.rs:146） | 无 |
    | request.c:1015-1021 FreePass | `r_alive_tm = now; r_check_tm = now+1` | `FreePass`（monitor.rs:133） | 无 |
    | request.c:506-513 UpdateInitDone | `r_flags \|= RS_INIT_DONE`；pending-- | `ReadyOutcome::UpdateInitDone{pending_remaining}`（ready.rs:144） | 只带 pending |
    | request.c:488-497 InitFailed | `r_init_err = result`；ERESTART 时 `|= RS_REINCARNATE` | `InitFailed{result, reincarnate}`（ready.rs:134） | 只带 bool，不带 init_err |
    | request.c:514-525 FreshInitDone | `&= ~RS_INITIALIZING; check_tm=0; alive_tm=getticks()` | `FreshInitDone`（ready.rs:148） | 无 |
    | manager.c:1071-1076 InitUpdateRollback | `r_init_err = ERESTART` | `TerminateAction::InitUpdateRollback`（recovery.rs） | `set_flags: empty`，无 init_err |
    | manager.c:1146-1151 reincarnate | `&= ~RS_REINCARNATE` | `CleanupAll{reincarnate}`（recovery.rs） | set_flags 只能 SET 不能 CLEAR |
  - 对照：`recovery.rs::TerminateDecision::set_flags` 是"部分显式"；`service_create.rs::mark_child_created`
    是"直接变异"。三种风格并存——T5 的 monitor 模式只统一了**注入边界**，没统一**变异载荷**。
    另有一处同族实例：`check_request`（slot.rs:169-200）C 会**就地改写** `rs_start.rss_cpu`
    （request.c:1286-1296，BSP→bsp_id），Rust 返回 `Ok(cpu)`——12/08 接线若丢弃返回值，
    `slot.cpu` 保留 `RS_CPU_BSP`(-2) 导致调度参数错误；返回值必须被消费。
  - 影响：06/12/15 接线时漏一条变异 = 静默行为漂移——漏 `NOPINGREPLY` → 每轮 ping 超时都 crash
    循环；漏 `check_tm = now` → 每 tick 重复 ping；漏 `INIT_DONE` → 更新永不结束。副作用埋在
    注释里，类型系统不保护。
  - 改进思考：统一为**变异载荷**模式：决策变体携带 `SlotMutations { set: RFlags, clear: RFlags,
    init_err: Option<i32>, check_tm/alive_tm/stop_tm/backoff: Option<…> }`（recovery 的 set_flags
    扩展为可 SET+CLEAR+字段），或"纯函数作用于 slot 副本、返回 `(Slot, Decision)`、shell 只提交"
    ——后者更 Rust（一次借用结束、无散落 setter），并可顺带解 R14。对照 Redox：kernel 侧
    `CleanLockToken` 全链传递变异权；Rust 侧 type-state/载荷是最接近的等价物。

- **R14（P1 设计缺口）— `init_service`/`start_service` 的槽位副作用无任何建模**
  - C 证据：`init_service`（utility.c:18-64）设置 `r_flags |= RS_INITIALIZING`（:19）、
    `r_alive_tm = getticks()`（:20）、`r_check_tm = r_alive_tm + 1`（:21）；`start_service`
    （manager.c:974-987）先 `r_priv.s_init_flags |= init_flags`（:978）再 create/publish/run。
  - Rust 现状：`ready.rs` 只有 `init_message`（消息面组装）与 `init_flags`（SF_USE_SCRIPT 位），
    **全 crate 生产代码无一处写入槽位 `RFlags::INITIALIZING`**（rg 证实：仅测试直接构造与位定义）；
    `s_init_flags |= init_flags` 亦无建模。`do_init_ready` 的 gate 依赖 `INITIALIZING`
    （request.c:477-483）——12 接线若不先设置该位，所有 ready 消息都会被 `Unexpected` 拒绝。
  - 影响：12 落地必须从 C 重抄三行变异 + 一条 |=，无类型/测试保护（对比 `mark_child_created` 有
    独立函数+测试）。这是"纯决策切片"在**消息发送前状态迁移**上的覆盖缺口。
  - 改进思考：`ready.rs` 增加 `mark_initializing(slot: &mut ServiceSlot, ticks: Clock)`（对齐
    utility.c:19-21，测试断言三字段）；`start_service` 决策（或 12 design）显式携带
    `s_init_flags |= init_flags` 变异。与 R13 的变异载荷方案合并落地。

- **R15（P2 边界语义）— `rebuild_args` 的 `argc` 计入被截断出 args 缓冲的 token**
  - C 证据：`build_cmd_dep`（manager.c:289-323）`strcpy(r_args, r_cmd)` 保证 argv 全在缓冲内、
    `r_argv[argc]=NULL`；512 字节满缓冲无 NUL 的 cmd 在 C 是 strcpy 溢出（UB，C 的缺陷）。
  - Rust 现状：`service_create.rs:80-100` `rebuild_args`——`argc = tokens.len()`（全量，:82），写
    args 时按 `remaining.saturating_sub(1)` 截断（:88-95）；cmd 满 512 字节时后部 token 被丢但
    argc 照计 → `argc > args 实际内容`，09/10 重建 argv 会错位。
  - 影响：仅畸形 RS_UP（cmdlen=512 无 NUL）可达；Rust 无内存风险但 argc 语义与 C 分裂，接线后
    难排查。
  - 改进思考：截断后 `argc` 只计实际写入 token（或写满时在缓冲尾写终止 NUL 再统计）；补
    `test_rebuild_args_full_buffer_argc`（512 字节 cmd）；08 接线时与 `rss_cmdlen` 上限校验联动。

- **R16（P2 健壮性）— `compute_backoff` 对负 restarts 是 debug panic / release UB**
  - C 证据：manager.c:1164 `1 << MIN(r_restarts, BACKOFF_BITS-2)`——r_restarts 负值时 C 同样 UB；
    但 C 只在 `r_restarts > 0` 分支调用（:1160），负值不可达。
  - Rust 现状：`recovery.rs:160-166` `compute_backoff` 是 `pub fn`，
    `restarts.min(62) as u32`——负值 → `1i64 << 4294967295`：debug 构建 panic、release 构建
    未定义行为（Rust shift ≥ 位宽）。当前唯一调用点（terminate_decision 的 `restarts > 0` 分支）
    安全，但 pub API 直接调用可触发。
  - 改进思考：`let shift = restarts.max(0).min((BACKOFF_BITS - 2) as i32) as u32;`（或入参改
    `u32`）；补 `test_compute_backoff_negative_returns_1`。Rust 对 C 的 UB 应 fail-closed 而非继承。

- **R17（P2 配置一致性）— boot 表 `proc_nr` 与 `endpoint.slot()` 无一致性校验 + `boot_img` const 越界**
  - C 证据：内核 `boot_image[]` 中 boot 服务 endpoint 由 proc_nr 派生（`_ENDPOINT(0, proc_nr)`），
    RS 侧 `sys_getimage` 拿到的就是这对值（main.c:196）。
  - Rust 现状：`boot.rs:296-325` `BOOT_IMAGE_PLACEHOLDER` 手写 12 组 `boot_img(proc_nr, ep, …)`；
    `validate_tables`（boot.rs:249-263）只比对 image/priv 计数，**不校验
    `proc_nr == endpoint.slot()`**——某行手误（proc_nr 与 endpoint 错位）测试全绿但 boot 语义错。
    `boot_img`（boot.rs:268-278）`while i < bytes.len()` 无 16 上限：名称 >16 字节在 const 求值期
    越界（晦涩编译错误）。
  - 改进思考：`validate_tables` 加 `debug_assert!(ip.endpoint.slot() == ip.proc_nr)`（+ 测试）；
    `boot_img` 用 `let n = bytes.len().min(16)`。对照 Rust 实践：静态配置应让"非法状态不可表达"
    （此处退化为"越界可表达但难看地失败"）。

- **R18（P2 资源语义）— `free_slot` 不释放 exec 镜像，`free_exec` 生产零调用**
  - C 证据：`free_slot`（manager.c:2088-2109）对 `SF_USE_COPY` 服务调 `free_exec(rp)`（:2100-2102）。
  - Rust 现状：`process_table.rs:324-336` `free_slot` 注释声明 deferral（exec 留给 15）；`exec.rs:74`
    `free_exec` 只有测试调用（rg 证实生产路径零调用）。效果：被 free 的行保留
    `exec: Option<Arc<[u8]>>` 直至行被重用，与 C"free 即释放"语义偏离（64 行上限内的小内存滞留）。
  - 改进思考：把 C 语义收敛进表原语——`free_slot` 内 `if slot.pub_.sys_flags.contains(USE_COPY) {
    crate::exec::free_exec(table, id) }`（注意借用顺序：先取 flags 再 free）；或保持 hook 但在
    15 design 加契约测试（`test_free_slot_releases_use_copy_exec`）。

- **R19（P2 语义精确性）— `pid.is_some()` vs C `r_pid > 0`**
  - C 证据：request.c:987/1007 `rp->r_pid > 0`（严格正）。
  - Rust 现状：monitor.rs:114/126 `rp.pid.is_some()`——`Some(0)` 时 Rust 判"有 pid"、C 判"无"。
    真实服务 pid ≥ 1 不可达，但 `getnpid` 结果直接落槽，语义上应 fail-closed。
  - 改进思考：`pid.is_some_and(|p| p > 0)`，或 step4_finish（boot.rs）把 `pid < 0 → Err` 扩为
    `pid <= 0 → Err`，19 接线时约束 `getnpid > 0` 才写槽。

### 16.2 Redox / OS / Rust 实践对照（本轮新增视角，不重复 §7/§14.2/§14.3）

| 维度 | Redox 做法 | rs server 现状 | 对照结论 |
|------|-----------|---------------|---------|
| 消息分类 | scheme 消息 → `enum` dispatch，消息体解析进类型 | `classify` → `DispatchKind` enum（✅）；但 `dispatch_request(i32) -> DispatchResult(i32)` 仍裸 i32 匹配 | 续 A-2 的 typed `RsRequest`：把 `copy_rs_start` 解析纳入类型，非法组合不可表达（R20a） |
| 用户态解析 totality | 内核保证消息源合法；用户态 Resource 查找永不 panic | `endpoint_slot` 对 NONE/ANY/SELF 越界 panic（R12） | R12：索引层补界，向 Redox"查找全量"看齐 |
| 状态迁移副作用 | kernel 锁 token（L0-L5）显式传递变异权 | monitor/ready 的槽位变异靠注释传递（R13/R14） | R13/R14：变异载荷/type-state 是 Rust 等价物，接线前定 |
| 配置-状态分离 | init.rc 纯配置 vs daemon 运行态 | `ServiceSlot` 混合（D1）；boot 表 proc_nr↔endpoint 一致性缺失（R17） | R17：静态配置加编译期/测试期校验 |
| 失败策略 | panic 仅 unrecoverable | `compute_backoff` 负值 UB（R16）、`rebuild_args` argc 分裂（R15） | R15/R16：C 的 UB 不继承，fail-closed |

### 16.3 验证证据（复现命令）

```bash
cargo test -p minix-rs              # 195 passed（本轮基线，未改动代码）
rg -n "unimplemented!|todo!" os/servers/rs/src/   # 3 处生产占位符（均带 doc 契约）
sed -n '63,66p'   minix3/minix/servers/rs/main.c       # rs_isokendpt 前置（R12）
sed -n '1004,1028p' minix3/minix/servers/rs/request.c  # NOPINGREPLY/init_err 变异（R13）
sed -n '19,21p'    minix3/minix/servers/rs/utility.c    # init_service 三行变异（R14）
sed -n '2100,2102p' minix3/minix/servers/rs/manager.c   # free_slot→free_exec（R18）
rg -n "INITIALIZING" os/servers/rs/src/*.rs | grep -v test   # 生产无 INITIALIZING 写入（R14）
```

### 16.4 修复建议路线

- **接线前必修**：R12（索引 totality，5 分钟级）→ R13/R14（变异载荷设计决策，与 T5/R10 合并立项）。
- **可立即修**：R16（1 行 clamp + 测试）。
- **接线期**：R15（08 落地时）、R18（15 落地时）、R19（step4 收口）、R17（表编辑时顺手）。
- 修复遵循 fix-guard.md：每条修复前读目标行 ±5 行、grep 确认、单条修复、修后 grep 验证 + 记录
  Fix-Status（追加到 §17）。

---

## 17. Fix-Status（2026-08-16 第六轮修复 — §16 R12-R19 逐项落地）

> 修复遵循 fix-guard.md：每条先读目标行 ±5 行 → grep 确认现状 → 单条修复 → 修后 grep 验证。
> 代码与文档同步；R13/R14 变异载荷设计是本轮核心决策（与 T5 的注入边界统一为
> "决策返回 + shell 提交"）。验证：`cargo test -p minix-rs` = **207 passed**；
> `cargo clippy -p minix-rs --lib` 零告警；`cargo fmt` 干净。

### Fix #32 — R16（P2 健壮性）：`compute_backoff` 负 restarts clamp，不继承 C 的 UB
- **File**：`os/servers/rs/src/recovery.rs` + 15-rs-terminate-restart.md §5
- **Before**：`let shift = restarts.min((BACKOFF_BITS - 2) as i32) as u32;`——负 restarts 的
  `1i64 << 4294967295` 是 debug panic / release UB（C manager.c:1164 同样 UB，但只在 `restarts > 0`
  分支可达）。
- **After**：`let shift = restarts.max(0).min((BACKOFF_BITS - 2) as i32) as u32;` +
  `test_compute_backoff_negative_returns_1`（-1/-100 × no_bin_exp/use_copy 四组合 → 1）。
- **Verified**：`cargo test -p minix-rs recovery::` = 13 passed；`rg "max(0).min" os/servers/rs/src/recovery.rs` → 1 处。
- **设计注**：Rust 对 C 的 UB 不继承——pub API 全量 fail-closed（入参保持 `i32` 与 `r_restarts`
  语义一致，clamp 而非改签名）。

### Fix #33 — R12（P1 totality）：`endpoint_slot`/`set_endpoint_index` 越界门
- **File**：`os/servers/rs/src/process_table.rs` + 02-rs-process-table.md §3.1/§3.6、10-rs-service-create.md §3.4、
  06-rs-main-loop.md §2.3、dispatch.rs 注释
- **Before**：`if slot < 0 { return None; }` 后裸索引 `self.by_endpoint[slot as usize]`——`Endpoint::NONE`
  （slot=31743）/`ANY`(31744)/`SELF`(31741) 越界 panic（可达面：`access::caller_can_control`、
  `swap_slot` step 5 对 clone_slot 产物的 `NONE` 行）。
- **After**：`if !(0..NR_PROCS as i32).contains(&slot) { return None; }`（写侧同门忽略）；补
  `test_endpoint_slot_none_fails_closed`（process_table.rs）+ `test_swap_slot_with_vacant_row_no_panic`
  （service_create.rs）；06/dispatch 标注"接线时 `classify` 前补 `isokendpt`"（main.c:63-66）。
- **Verified**：`cargo test -p minix-rs process_table::` = 19 passed、`service_create::` = 13 passed；
  `rg "0..NR_PROCS" os/servers/rs/src/process_table.rs` → endpoint_slot/set_endpoint_index 两处。
- **设计注**：A-4 快索引向 Redox"用户态查找全量"看齐——O(1) 不牺牲 totality；C 靠主循环
  isokendpt 前置，Rust 在索引层补界（fail-closed 与 D3 同风格）。

### Fix #34 — R15（P2 边界语义）：`rebuild_args` argc 只计完整写入的 token
- **File**：`os/servers/rs/src/service_create.rs` + 10-rs-service-create.md §3.4
- **Before**：`slot.argc = tokens.len()`（全量），写 args 时按 `remaining.saturating_sub(1)` 截断——
  512 字节满 cmd 时后部 token 被丢但 argc 照计，09/10 重建 argv 错位（C 的 strcpy 溢出是 UB，不继承）。
- **After**：循环内 `off + need > args.len()` → break（尾 token 整体丢弃）；每完整写入
  （token + NUL）`argc += 1`；缓冲尾保持 NUL 终止；补 `test_rebuild_args_full_buffer_argc`
  （满 512 字节 cmd → argc=1；恰好放得下 → 全计）。
- **Verified**：`cargo test -p minix-rs service_create::` = 13 passed；
  `rg "argc = 0" os/servers/rs/src/service_create.rs` → rebuild_args 内。
- **设计注**：argv 是"完整 NUL 终止条目序列"——argc 必须与缓冲内容自洽；C 的 UB 场景在 Rust
  里定义为"截断 + 计数一致"的确定行为（fail-closed 不猜）。

### Fix #35 — R18（P2 资源语义）：`free_slot` 内建 `SF_USE_COPY → free_exec`
- **File**：`os/servers/rs/src/process_table.rs` + 02-rs-process-table.md §3.6、09-rs-exec.md §2.2、
  15-rs-terminate-restart.md §2.2
- **Before**：`free_slot` 注释声明 exec 留给 09/15；`free_exec` 生产路径零调用——被 free 的行滞留
  exec `Arc` 直到行被重用，与 C"free 即释放"（manager.c:2100-2102）偏离。
- **After**：先取 `sys_flags`（结束借用）再 `crate::exec::free_exec(self, id)`；补
  `test_free_slot_releases_use_copy_exec`（USE_COPY → exec None；非 USE_COPY 保留，execve 路径 09
  释放 manager.c:643-644）。
- **Verified**：`cargo test -p minix-rs process_table::` = 19 passed；
  `rg "free_exec" os/servers/rs/src/process_table.rs` → free_slot 内 1 处调用。
- **设计注**：把 C 语义收敛进表原语（单一权威），`Arc` 的 drop 即 free 语义（ARCH A-5）由测试锁定。

### Fix #36 — R19（P2 语义精确性）：`pid.is_some()` → `pid.is_some_and(|p| p > 0)`
- **File**：`os/servers/rs/src/monitor.rs` + 07-rs-period-heartbeat.md §3.2/§5
- **Before**：`rp.pid.is_some()`——`Some(0)` 判"有 pid"，C 是严格的 `r_pid > 0`（request.c:987/1007）。
- **After**：两处 `pid.is_some_and(|p| p > 0)`（stop 超时 + ping 超时）；补 `test_zero_pid_is_no_process`
  （`Some(0)` 不触发 crash 路径，fail-closed）。
- **Verified**：`cargo test -p minix-rs monitor::` = 12 passed；`rg "is_some_and" os/servers/rs/src/monitor.rs` → 2 处。
- **设计注**：`getnpid` 结果直接落槽，语义上"0 进程不存在"；19 接线时 step4 可把 `pid <= 0 → Err`
  收口（本项先行 fail-closed）。

### Fix #37 — R17（P2 配置一致性）：boot 表 `proc_nr ↔ endpoint.slot()` 校验 + `boot_img` 长度钳制
- **File**：`os/servers/rs/src/boot.rs` + 01-rs-boot-init.md §2.3.2/§5.2
- **Before**：`validate_tables` 只比 image/priv 计数，不校验 `proc_nr == endpoint.slot()`
  （手写 placeholder 错位 → 测试全绿但 boot 语义错）；`boot_img` `while i < bytes.len()` 无 16 上限
  （>16 字节名 const 求值越界，晦涩编译错误）。
- **After**：`validate_tables` 加 `any(|ip| ip.endpoint.slot() != ip.proc_nr) → Err(ENOSYS)`（fail-closed，
  比 todo 建议的 debug_assert 更强：release 也拦）；`boot_img` 用 const 兼容的三元钳制
  `let n = if bytes.len() > 16 { 16 } else { bytes.len() };`（`Ord::min` 非 const stable）；
  补 `test_validate_tables_rejects_proc_nr_endpoint_mismatch` + `test_boot_img_truncates_long_name`。
- **Verified**：`cargo test -p minix-rs boot::` = 18 passed；`rg "proc_nr" os/servers/rs/src/boot.rs` → 校验 1 处 + placeholder 12 组。
- **设计注**：静态配置的"非法状态不可表达"理想退化为"越界可表达但校验期失败"——校验放
  `validate_tables`（与 C main.c:225-227 计数核对同层）。

### Fix #38 — R13/R14（P1 设计缺口）：决策变异载荷统一为 `SlotMutations`
- **File**：`os/servers/rs/src/service_slot.rs`（新增 `SlotMutations` + `apply`）、`monitor.rs`
  （`PeriodDecision { action, mutations }`）、`ready.rs`（`ReadyDecision { outcome, mutations }` +
  `mark_initializing` + `fold_init_flags`）、`recovery.rs`（`TerminateDecision.mutations` 替代
  `set_flags`）、`lib.rs` re-exports、`slot.rs`（check_request 返回值消费注记）；文档 07/12/15/16/
  18/99/03/08 同步。
- **Before**：monitor/ready 的槽位变异埋在注释里靠 shell 记得；`recovery::set_flags` 只能 SET 不能
  CLEAR/写字段；`RFlags::INITIALIZING` 生产路径零写入（R14）；`InitUpdateRollback` 的
  `r_init_err = ERESTART`（manager.c:1075）、`REINCARNATE` 清除（manager.c:1147）、backoff 写入
  （manager.c:1163-1174）均未建模。
- **After**：`SlotMutations { set, clear, init_err, check_tm, alive_tm, stop_tm, backoff }` +
  `apply()`（set/clear/字段一次提交）；monitor 各分支携带 C 逐条变异（backoff 递减为绝对值、
  `stop_tm=0`、`check_tm=now`、free pass 的 `alive/check`、`NOPINGREPLY`+`init_err=EINTR`）；ready
  `InitFailed{set:REINCARNATE, init_err}`/`UpdateInitDone{set:INIT_DONE}`/`FreshInitDone{clear:
  INITIALIZING, check_tm:0, alive_tm:ticks}`；recovery `mutations` 含 set/clear/init_err/backoff；
  `mark_initializing(slot, ticks)`（utility.c:19-21）+ `fold_init_flags`（manager.c:953，OR 语义）。
  测试：`test_slot_mutations_apply`、`test_mark_initializing`、`test_fold_init_flags`、ready/monitor/
  recovery 各决策测试扩展载荷断言、`test_terminate_backoff_writes_slot`。
- **Verified**：`cargo test -p minix-rs` = 207 passed；`rg "SlotMutations" os/servers/rs/src/` →
  service_slot 定义 + monitor/ready/recovery 使用 + lib.rs 导出；`rg "INITIALIZING" os/servers/rs/src/ready.rs`
  → `mark_initializing` 写入 1 处（R14 关闭）。
- **设计注**：对照 Redox 变异权 token——副作用随决策显式传递、不埋在注释；三种决策风格
  （recovery 部分显式 / monitor 裸枚举 / service_create 直接变异）收敛为一种"载荷 + apply"模式，
  与 T5 的 functional-core 边界正交。漏一条变异从"接线时静默漂移"变成"类型上不存在"。

### Fix #39 — §15 遗留（P2 附带观察）：`RsStart` 补 `rss_nr_io`/`rss_io` + 计数域对齐 C `int`
- **File**：`os/servers/rs/src/slot.rs` + 08-rs-slot-config.md §3.1/§5
- **Before**：`RsStart` 缺 `rss_nr_io`（C rs.h:124 `int rss_nr_io`）与 `rss_io` 表（rs.h:125
  `struct { unsigned base; unsigned len; } rss_io[RSS_NR_IO]`）；`nr_irq`/`nr_control: usize` vs
  C `int`（rs.h:122/136）——`usize` 丢失 `RSS_IRQ_ALL`/`RSS_IO_ALL` 哨兵之外"校验前负值非法"状态，
  与 `edit_slot` 的 `> NR_IRQ`/`> NR_IO_RANGE` 检查（manager.c:1492/1510）错位。
- **After**：补 `nr_io: i32` + `io: [IoRange; RSS_NR_IO]`（引用 `privilege::IoRange`——N9 单一权威：
  C 的 `rss_io` 与内核 `struct io_range`（priv.h:13-16）同构，`edit_slot` 逐项拷入 `s_io_tab`
  （manager.c:1516-1518），D6 收敛方向）；`nr_irq`/`nr_control: usize → i32`（与 R2/Fix #27 的
  Privilege/ServiceSlot 计数域一致）；Default 补 `nr_io: 0`/`io: [IoRange::default(); RSS_NR_IO]`
  （C 调用方 `memset(rs_config, 0, sizeof)` 语义，parse.c:1160）；测试扩 Default 断言
  （`test_rs_start_default_matches_c_caller`）+ 新增 `test_rs_start_resource_counts_are_i32_like_c_int`
  （哨兵 17 与负值可表示，锁定 C `int` 校验前语义）。
- **Verified**：`cargo test -p minix-rs slot::` = 24 passed、全量 = **208 passed**；
  `rg "nr_io|IoRange" os/servers/rs/src/slot.rs` → 字段/Default/测试三处；08 文档 §3.1 模型块 +
  设计差异两条（i32 计数域 / IoRange 单一权威）+ §5 测试表同步。
- **设计注**：计数域 `i32` 是 C `int` 的"校验前状态"表达（R2 同理由）而非 ABI 搬运——`edit_slot`
  的哨兵归一/越界拒绝（manager.c:1486-1521）仍属 19 接线，本项只保证类型可表达；`io` 表收敛到
  `privilege::IoRange` 避免 C 双表同构（D6）。Redox 视角：资源声明（`rs_start`）与资源授权
  （priv `s_io_tab`）共用同一类型，声明→授权无转换面，非法组合（计数超表长/负值）在类型上
  可表达但由 19 校验拒绝（fail-closed）。

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

### 18.4 死代码候选清单（本轮只列不删；每条含判定依据与消除影响）

**第一档：立即可删（2 项）**
1. `RS_FI_CRASH` 双定义——query.rs:115 与 `libs/minix-types/src/ipc/rs.rs:61` 各一份（值相同，
   各带一份重复测试 query.rs:175-178 与 minix-types:260）；dispatch.rs:24-26 已声明 minix-types
   是 RS 消息常量唯一权威。消除影响：零（query.rs 版零生产调用）。
2. `RProcTable::set_endpoint_mapping`（process_table.rs:225-233）——被 `set_endpoint_index`
   （process_table.rs:168，带 R12 fail-closed 断言）完全取代，全仓唯一出现处即定义。消除
   影响：零。

**第二档：同一语义双实现点（1 项，OQ-3 上交）**
3. `share_exec`（exec.rs:47）与 service_create.rs:129 的内联 `exec.clone()` 做同一件事（后者
   注释自辩 "explicit for faithfulness"）——未来改 exec 表示时是分叉风险。留一删一。

**第三档：接线期复活（不删，建议统一标注）**
- 零调用根因是 dispatch 全 ENOSYS（A4）：`dispatch_request`（dispatch.rs:106）、request.rs
  五件套（`up_init_flags` :34、`check_duplicates` :55、`mark_late_reply` :83、`stop_service`
  :113、`shutdown_apply` :132）、query.rs 全部函数、service_create 大部分纯切片。
- 连 lib.rs 导出都没有的孤儿（cargo bin 目标会报 dead_code，建议补 `#[allow(dead_code)]` +
  归属标注）：`should_reply_ready`（ready.rs:275）、`normalize_init_response`（ready.rs:283）、
  `normalize_lu_response`（ready.rs:298）。
- 为未建编排预留的原料（R23 落地时消费）：`UpdateEntry::is_preparing_only`（live_update.rs:284，
  C 消费点 update.c:515/:552/:827/:891/:904 五处编排尚不存在）、`UpdateChain::last_lu_flags`
  （live_update.rs:339，唯一消费者 `rupdate_set_new_upd_flags` 缺失）。
- 18/19 号预支：self_lifecycle.rs 全部 13 个导出（唯一引用是 lib.rs:90-94 转导出）、sched.rs
  三件套（`sched_decision` :119、`on_stop_result` :173、`set_sig_mgrs` :207）、KernelApi 5 个
  预支方法（`getnuid`/`srv_fork`/`getprocnr`/`vm_memctl`/`vm_set_priv`，boot.rs:81/:91/:97/
  :103/:114——R9 拆 trait 时归位）、`VmRsMemReq` 三个 LU 变体（boot.rs:134-138）、
  `ipc_mask::update_ipc_mask`（ipc_mask.rs:212）、`exec::has_shared_exec`/`validate_image`
  （exec.rs:57/:27，RSS_REUSE 与 19 号路径）、`lookup_by_flags`（process_table.rs:295，
  A5-06 步喂参 `period_decision`）。
- **机制建议**：对照 `tools/check-rs-unwired.sh` 对 panic 标记的 doc-contract 门禁，给上述
  "预期零调用"项在模块头或 lib.rs 导出清单统一加 `// awaiting-wiring: NN-rs-xxx.md` 标注
  ——把"有意等待"显式化，防止后续轮次误判为死代码误删，也防止接线者漏认领。

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
| R20 | edit_slot/init_slot 整体缺失 + RsStart 载体字段不全 | P1-design-missing | ☐ | 08 落地期 |
| R21 | from_calls 无法表达 is_init=false 组合语义 | P1 | ☐ | 08 落地期（可先改 API） |
| R22 | 生命周期编排层缺失（create 15 步只有 2 步 + cleanup 第一相） | P1-design-missing | ☐ | 13 落地期 |
| R23 | LU 中段编排缺失 + rupdate 全局碎片化 + r_upd 载体未建 | P1-design-missing | ☐ | 16 落地期 |
| R24 | do_upd_ready 缺载荷，RS_PREPARE_DONE 无处落地 | P1 | ☐ | 可立即 todo-fix |
| R25 | HeartbeatNotify 缺 timestamp 字段 | P1 | ☐ | 可立即 todo-fix（06 先行） |
| R26 | signal_manager 签名偏差（sef.h:270 对照） | P1 | ✅ | 已修（Fix #40，2026-09-06） |
| R27 | rollback 心跳重发扫 + end_update 自毁短路 + abort 时序注释错 | P1 | ☐ | 16 落地期（注释修正可立即） |
| R28 | clone_service 两分支漏标 DEFERRED | P2 | ☐ | 可立即 todo-fix |
| R29 | TrapMask 宽度分歧（0x3E vs 0xFFFF）+ DSRV_T/DSRV_I 缺失 | P2/OQ-2 | ☐ | 19 接线期决策 |
| R30 | caller_can_control 丢 IN_USE 复核，索引不变式无声明 | P2 | ☐ | 可立即 todo-fix |
| R31 | error.c 错误表 + 诊断字符串化 4 函数缺失 | P2 | ☐ | 19/IS 阶段 |
| R32 | 一致性杂项 7 小项（死存储/双份字段/同名函数/清零无执行者等） | P2 | ☐ | 逐项标注（见条目） |
| R33 | ServiceSlot 派生 PartialEq 的深比较风险 | P2 | ☐ | D1 同轮 |
| R34 | 测试盲区清单 24 条 | P2 | ☐ | 1-13 可立即补；14-17 随 13/14；18-23=E3 具体化 |
| A1 | 编排层引入形态（三方案对比，推荐忠实编排函数） | 建议 | ☐ | 13/16 落地期 |
| A2 | UpdateState 挂 ServerState + r_upd 入 ServiceSlot | 建议 | ☐ | 16 落地期 |
| A3 | SEF 回调重绑建模（restart_cb 枚举） | 建议 | ☐ | 18 落地期 |
| A4 | 控制请求域统一决策载荷模式 | 建议 | ☐ | 13 落地期 |
| A5 | 接线路线图（06→12→13→08→16→19 依赖序） | 建议 | ☐ | 全局 |

**OQ 清单（上交用户）**
- OQ-1：self_lifecycle.rs 13 个导出保持"纯切片等 18 号接线"形态，还是先内联进 recovery/
  live_update？（推荐保持 + awaiting-wiring 标注）
- OQ-2：TrapMask 收窄到 4 位是否有意？`MINIX_KERNINFO`（bit 6）trap 允许面要不要对齐 C？
  （R29 方向决策）
- OQ-3：`share_exec` 与内联 clone 留一删一，留哪个？（18.4 第二档）
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
