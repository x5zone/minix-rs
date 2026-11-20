# design-d2 — P-DESIGN 第二遍（独立设计审查）

- 审查对象：`rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md` §3/§4 ↔ Rust 实现（related-code.txt 清单）
- 基线：文档主张 ↔ 源码事实（C 源 `minix3/` 作真值仲裁）
- 方法：七维度对照（状态表达、身份、所有权、错误、并发、抽象边界、可测试性）+ 翻译残留 / 死抽象 / 非法状态 / 最小化机会
- 边界：不读作者解释、历史评审、expected/；每条给「接受/拒绝/需用户裁决」

## FINDING RD06-DESIGN-D2-01（P1）`CapabilityTemplate::Vm` 的 s_flags 多出 PREEMPTIBLE，且文档 §3.2 表与代码互相矛盾 [MODERN]

- 文档 §3.2 模板对照表写 `Vm` 的 `s_flags` = `SYS_PROC | VM_SYS_PROC`（C `VM_F`）。
- 代码事实：`os/kernel/src/capability.rs:218` `Self::Vm => ProcessCapability::VM_F | ProcessCapability::SRV_F`，即实际置了 `SYS_PROC | VM_SYS_PROC | PREEMPTIBLE`。
- C 真值：`minix3/minix/kernel/main.c:204` `priv(rp)->s_flags = VM_F;`——C 的 VM **不带** PREEMPTIBLE。
- 行为面：`PREEMPTIBLE` 被 `proc_no_time` 一类路径读取（文档 §3.11 自己引用了该判据），C 的 VM 时间片耗尽后就地续时间片，Rust 的 VM 会走"通知用户态调度器"分支。这是外部可观察行为的静默改变，既未在 §3.14 去向表标 `[ARCH]`，文档表又与代码相反。
- 同函数注释自称 "each arm is the exact flag set C writes to s_flags"（capability.rs:213-215），对 VM 这条臂是假话。
- 附注：`RootService => RSYS_F | SRV_F` 中 `RSYS_F` 已含 `SRV_F`，OR 项冗余但值正确。
- 判定：**需用户裁决**——要么这是有意现代化（则文档表、代码注释、§3.14 档位三处都要补 `[ARCH]` 标注），要么是笔误（则代码改回 `VM_F`）。

## FINDING RD06-DESIGN-D2-02（P2）`CapabilityError` 三变体中两个是死变体，枚举的设计理由未落地 [DEL]

- 文档 §3.2 的核心卖点：`Option<PrivId>` 升级为枚举，以区分"静态槽被占（bug）"与"动态段满（可重试）"。
- 代码事实：
  - `grant_capability`（kpriv.rs:1180-1182）唯一错误来源是 `assign_static(proc_nr).ok_or(CapabilityError::SlotOccupied)`——`assign_static` 返回 `Option<PrivId>`，**invalid proc_nr 与槽被占折叠成同一个 `SlotOccupied`**；
  - 全仓检索：`CapabilityError::InvalidProcNr` 与 `CapabilityError::NoFreeSlots` 除定义处（capability.rs:198,205）外**零构造、零匹配**；
  - 生产调用点（lib.rs:2664,2724）一律 `.expect(...)`，枚举的"瞬时/永久"区分在调用侧没有任何消费者。
- 结论：枚举声称的三分类实际只有一分类在流转，"可重试 vs bug"的区分能力是纸面的。死变体要么删掉（枚举退化为单错误或直接 `Option`，诚实于现实），要么给 `assign_static` 补上 invalid-nr 判定让变体活过来。
- 判定：**接受**（删变体或补判定，二选一；现状最差）。

## FINDING RD06-DESIGN-D2-03（P2）错误码注释与 C 真值不符：代码写 ENONENT，C 实为 EINVAL/EBUSY/ENOSPC [DEL]

- C 真值（`minix3/minix/kernel/system.c:274-299` `get_priv`）：静态路径 invalid id → `EINVAL`；槽已被占 → `EBUSY`；动态段满 → `ENOSPC`。
- 代码事实：`capability.rs:197` 写 "C: `get_priv` returns `ENONENT`"、`:204` 写 "returns `ENONENT`"。两处注释都引错 C。
- 文档 §2.3.1/§3.2 引的是 EINVAL/EBUSY/ENOSPC（与 C 相符），因此这是**代码注释层的翻译残留**，且与项目约束"错误类型必须映射到 Minix3 的 errno 值"直接相关：`CapabilityError` 本身不带 errno，未来接线到 `do_privctl` 时若照抄这组注释会产出错误 errno。
- 判定：**接受**——修注释；若按 RD06-DESIGN-D2-02 补活变体，应顺带在变体文档里写明各自对应的 errno。

## FINDING RD06-DESIGN-D2-04（P2）`IpcMask::set_bit/unset_bit` 静默吞掉越界位，注释自相矛盾 [MODERN]

- 代码事实（capability.rs:341-354）：`idx >= 64` 时 `set_bit` 直接忽略、返回原值；`has_bit` 同样返回 false。
- 注释原文："Indices at or above 64 are ignored: ... and an out-of-range grant is a caller bug the kernel must not mask into silent state corruption."——前半句说"忽略"，后半句说"调用方 bug 不得被掩成静默状态损坏"，而**忽略恰恰就是静默掩掉**。注释与行为互相打脸。
- 对照 C：`set_sys_bit`（kernel/const.h:24）是宏 `(!((map)=0,...))` 系断言域（`assert(id >= 0 && id < NR_SYS_PROCS)` 语义），越界在 C 里是 assert 崩，不是静默忽略。
- 判定：**接受**——改为 `debug_assert!(idx < NR_SYS_PROCS)` 或返回 `Result`；至少把注释改到与行为一致。

## FINDING RD06-DESIGN-D2-05（P2）`grant_capability` 给内核 task/idle 置 `s_sig_mgr = SELF`，C 的终态是 NONE [MODERN]

- C 真值：`proc.c:145` 清表统一 `s_sig_mgr = NONE`；boot 循环只有 VM（`s_sig_mgr = SELF`，main.c:208）与 RS（`s_sig_mgr = SRV_SM = ROOT_SYS_PROC_NR`，即 RS 自身）改写它。内核 task 与 idle 的终态是 NONE。
- 代码事实：`grant_capability`（kpriv.rs:1202-1211）对**所有**模板统一 `sig_mgr = Endpoint::from_generation_slot(0, proc_nr.0)`（= SELF）。内核 task 与 idle 的表状态因此与 C 不同。
- 影响面：信号投递路径按 `s_sig_mgr` 决定去向；内核 task 在 C 中从不经此路径，当前可能无可观察差异，但表状态对账（mib/proc 一类导出）会露出差异。
- 文档 §3.2 的模板表根本没列 `s_sig_mgr` 列，读者无从得知这一偏离。
- 判定：**需用户裁决**——或按 C 把 KernelTask/Idle 的 sig_mgr 置 NONE（在模板上补一列），或确认 SELF 无害并写进文档。

## DIFF-TABLE（首批）

| ID | 维度 | 分类 | P | 一句话 | 判定 |
|---|---|---|---|---|---|
| RD06-DESIGN-D2-01 | 错误/状态 | MODERN | P1 | Vm 模板多 PREEMPTIBLE，文档与代码相反 | 需用户裁决 |
| RD06-DESIGN-D2-02 | 死抽象 | DEL | P2 | CapabilityError 两变体零构造 | 接受 |
| RD06-DESIGN-D2-03 | 错误 | DEL | P2 | errno 注释引错 C | 接受 |
| RD06-DESIGN-D2-04 | 状态表达 | MODERN | P2 | IpcMask 越界静默忽略，注释自相矛盾 | 接受 |
| RD06-DESIGN-D2-05 | 状态表达 | MODERN | P2 | 内核 task 的 s_sig_mgr SELF vs C NONE | 需用户裁决 |

## FINDING RD06-DESIGN-D2-06（P1）内核 task 的初始优先级是 USER_Q(7)，C 的终态是 TASK_Q(0) [MODERN]

- C 真值：`proc.c:131-137` `proc_init` 给**每个槽** `p_priority = 0`（即 `TASK_Q`，config.h:67）；boot 循环只给 VM/RS 改写 `SRV_Q = USER_Q`（main.c:205/225），内核 task 的优先级保持 0——SYSTEM/CLOCK 永远在最高队列。
- 代码事实：`proc.rs` `SchedFields::new()` 给 priority 置 `USER_Q = 7`（proc.rs:605），`KProcess::new_zeroed` 与 `KProcess::new` 都用它；boot 循环（lib.rs:2644-2734）没有任何一处把内核 task 拉回 `TASK_Q`。
- 消费点：`sched_enqueue_with` 的队列号直接取 `p.get_priority()`（proc_table.rs:930），无内核 task 特判。内核 task 被 IPC 唤醒后将入**队列 7**，与 USER_Q 的用户进程同队竞争；C 里它们在队列 0，恒先于全部用户进程。这是调度语义的外部可观察偏离。
- 同根事实（惰性）：`SchedFields::new` 还给每个槽 `quantum size = 200ms`（proc.rs:606），C 的 `proc_init` 是 0、仅 VM/RS 改 `SRV_QT = USER_QUANTUM`。此项**惰性**——C 的 tick 递减按 `p_endpoint >= 0` 跳过内核 task（proc.c:314，Rust clock.rs 注释同形），内核 task 时间片永不清账——但它是同一根因（`SchedFields::new` 给全槽用户级默认值，而 C 是全槽零 + 仅 VM/RS 升级）的第二个表现。
- 文档内部矛盾：§2.5（行 1009）与 §2.1.2（行 323）**明确写出了** C 侧事实——"p_priority = 0 恰好是最高的 TASK_Q""boot 循环只给 VM 与 RS 写优先级"；而 §3.11（Rust 决策章）对 Rust 侧的 USER_Q 初值只字未提，§4.7 的 12 动作对位表里"清时间片"有、"优先级回填"无。模板表（§3.2）也没有 priority 列——C 的 VM/RS `SRV_Q`、内核 task 的隐式 0 都是模板应表达的角色属性。
- 判定：**需用户裁决**——修法明确：boot 循环给内核 task 置 `TASK_Q`（或模板补 priority 列）；无论修不修，文档 §3.11 都要补这条初值语义。

## FINDING RD06-DESIGN-D2-07（P1）表访问器返回 `&'static mut`，witness 形态的证明力被签名自己拆掉 [MODERN]

- 代码事实：`proc_table()` / `proc_table_with(_section: &BklSection)` / `proc_table_boot_unchecked()` **全部返回 `&'static mut ProcessTable`**（lib.rs:3417/3429/3443）。两个 `&'static mut` 并存即别名 UB：调用两次 `proc_table_with`（哪怕同一 section）就产生两个互斥引用。`BklSection` witness 只证明"进入时持锁"，对返回值的独占性与存续期毫无约束。
- 使用面：`proc_table()` 裸 unsafe 版约 318 处调用，`proc_table_with` witness 版约 30 处——"优先 witness"实际是 1:10 的反向比例。
- 文档矛盾：§3.1 宣称"要拿到 `&KProcess` 就必须先经过 `Option` 的解包""每个访问点自己举证"，§4.3 把三族访问器的命名约定当作纪律；但 `&'static mut` 返回值绕过 `Option`、且把"举证"从类型层降为纯口头。同文件 `caller_slot_mut`（proc_table.rs:800-834）为它自己的裸指针拆分写了完整别名契约并自称 "THE single audited escape"，而访问器家族的同类别名面却是无契约的。
- 判定：**需用户裁决**。彻底修法是访问器改为借用 `BklSection` 存续期的 `&mut`（大改，波及全部调用点）；最小修法是返回自制守卫类型。至少文档 §4.3 必须补写"`&'static mut` 的别名契约"一节，否则 §3.1 的存储安全叙事不成立。

## FINDING RD06-DESIGN-D2-08（P1）文档 §3.3/§3.9 的"清位与入队绑一次动作"主张与实际 API 面相反 [MODERN]

- 文档主张：§3.3"操作宏的对位是两个方法（rts_set/rts_unset）"；§3.9 反推表明确"否掉"拆分 API——"拆分会让'清位但忘了入队'成为可写出的 bug"。
- 代码事实：同一文件里存在**成对的拆分原语**——`RtsFlags::clear`（proc.rs:1417，与 `unset` 同体的低层原语）+ `ProcessTable::dequeue_if_blocked` / `enqueue_if_woken`（proc_table.rs:523/546），供只持有 `procs` 切片的 IPC 引擎使用；两条注释各自记录了真机故障旁证（"86k picks of a receiver parked in RTS_RECEIVING"、"runnable but never enqueued"）。代码自己说"This split mirrors C's macro-level decomposition"。
- 也就是说：文档当作"被否掉的失败形态"的写法，恰是代码的现行设计（有正当理由：IPC 引擎拿不到 `&mut ProcessTable`）；文档对这三个名字零提及。读者按 §3.9 的叙事会得出"忘入队不可能发生"的错误结论，而真机旁证表明它发生过。
- 判定：**接受**——不要求改代码；要求文档 §3.3/§3.9 改写为双轨 API 叙事（整表侧 `rts_set`/`rts_unset`、切片侧 `clear`+`dequeue/enqueue_if_*`），并把两条真机旁证收进 §6 诊断。

## FINDING RD06-DESIGN-D2-09（P2）`EntrySpec::KERNEL_TASK` 与 `DEFERRED` 同值双常量：文档声称的区分实际由 `ProcKind` 承载 [DEL]

- 代码事实：两常量值完全相同（三个 `None`，boot.rs:1666/1669）；各只有一个使用点（lib.rs:2678 / lib.rs:3000）；`build_cpu_context` 对三个 `None` 统一 `unwrap_or(0)`（x86-64 boot.rs），arch 层根本收不到"哪个常量"的信息。
- 文档 §3.4 的理由——"如果只有一个 NONE 常量，arch 层无法区分'永远不会有入口'与'暂时没有'，第一次调度时该不该报错也就无从判断"——**不成立**：这个区分由同调用点的 `ProcKind::KernelTask` vs `UserService` 承载（`build_cpu_context(kind, nr, entry)` 的第一参数），`EntrySpec` 对此零贡献。双常量是死区分，留一个即可。
- 判定：**接受**——删 `DEFERRED`（或 `KERNEL_TASK`），§3.4 的理由段改写为"区分由 ProcKind 承载"。

## FINDING RD06-DESIGN-D2-10（P2）`endpoint_to_nr` 线性扫描 O(261)，C 的 `isokendpt` 是 O(1) 算术解析 [MODERN]

- C 真值：`ipc.c:1851-1854`——`_ENDPOINT_P(e)` 一步算出槽号，`proc_addr(*p)->p_endpoint == e` 一次比对。
- 代码事实：`proc_table.rs:840-845` 对整表 `procs.iter().find(...)` 线性扫描。endpoint 编码公式本身与 C 一致（已核对 endpoint.rs:82-95 与 endpoint.h:65-69 逐项同形），slot() 直接给出候选槽——扫描完全可省。
- 位置：IPC 热路径（`m_source` 校验类调用）。261 槽 × 每 IPC 一次是纯浪费；且文档 §4.3 的 C 宏对照表缺 `isokendpt` 行，读者无从发现这条偏离。
- 判定：**接受**——按 `Endpoint::slot()` 直取 + 代次比对，与 C 同形；§4.3 表补 `isokendpt` 行。

## FINDING RD06-DESIGN-D2-11（P2）`CpuMask` 类型注释宣称 "Default = all bits set"，派生 Default 实为空掩码 [MODERN]

- 代码事实：`#[derive(Default)]`（proc.rs:546）给出 `bits = 0`——"不允许任何 CPU"这一合法但无意义的非法状态footgun；"全允许"只来自 `SchedFields::new()` 显式调 `CpuMask::all()`（proc.rs:608）。
- 类型文档注释（proc.rs:540）写 "Default = all bits set (any CPU)"——与派生行为相反。任何一处新代码写 `CpuMask::default()` 都会静默造出一个永不可调度的进程。
- 文档 §3.11 讨论了 C 零值 vs Rust 全允许的初值语义差，但没有揭示类型层这个注释-实现矛盾。
- 判定：**接受**——去掉 `derive(Default)`（空掩码不该有 Default 语义），或手写 `Default = all()`；注释同步。

## FINDING RD06-DESIGN-D2-12（P3）最小化机会：`Quantum::allocate` 零生产调用点；`RtsFlags::unset`/`clear` 同体双名 [DEL]

- `Quantum::allocate`（proc.rs:432）仅 clock.rs 测试使用（clock.rs:2553/2573，均在 `mod tests`）。文档 §3.11 已诚实披露。建议：移入 `#[cfg(test)]` 或删掉，让"没有生产写点"在 API 面可见。
- `RtsFlags` 同时有 `unset`（proc.rs:293）与 `clear`（proc.rs:1417），函数体一字不差；`clear` 是为 IPC 拆分轨道起的名。同体双名让 grep 无法统计"谁在用哪条轨道"。建议合一（保留 `clear`，删 `unset`，或反之）。
- 判定：**接受**（低优先级）。

## DIFF-TABLE（第二批）

| ID | 维度 | 分类 | P | 一句话 | 判定 |
|---|---|---|---|---|---|
| RD06-DESIGN-D2-06 | 状态表达/并发 | MODERN | P1 | 内核 task 优先级 7 vs C 的 0 | 需用户裁决 |
| RD06-DESIGN-D2-07 | 并发/所有权 | MODERN | P1 | 访问器 &'static mut 别名面 | 需用户裁决 |
| RD06-DESIGN-D2-08 | 抽象边界 | MODERN | P1 | 拆分 RTS 原语与文档主张相反 | 接受（改文档） |
| RD06-DESIGN-D2-09 | 死抽象 | DEL | P2 | EntrySpec 双常量死区分 | 接受 |
| RD06-DESIGN-D2-10 | 抽象边界/性能 | MODERN | P2 | endpoint_to_nr O(n) vs C O(1) | 接受 |
| RD06-DESIGN-D2-11 | 非法状态 | MODERN | P2 | CpuMask 注释与派生 Default 相悖 | 接受 |
| RD06-DESIGN-D2-12 | 最小化 | DEL | P3 | allocate 测试专用；unset/clear 双名 | 接受 |

## FINDING RD06-DESIGN-D2-13（P2）内核层编码 RISC-V sstatus.FS 位值，§4.1 的"内核层看不到任何一项"被同 crate 违反 [ARCH]

- 代码事实：`lib.rs:4769-4773` `riscv_lazy_fs_field` 在**内核层**硬编码 `FS = 0b00 << 13 / 0b11 << 13`（sstatus 的 FS 域位 13）；`lib.rs:4862-4864` 内核调度路径调 `p.cpu_context.set_fs_field(fs)`——`set_fs_field` 是 riscv64 `CpuContext` 的**固有方法**（riscv64/boot.rs:59），不是 `CpuContextArch` trait 方法。
- 文档矛盾：§4.1 明文"内核层看不到这张表里的任何一项：它只调用 `build_cpu_context`、`set_boot_cpu_context` 与 `enable_user_io`"；§4.1 的反推段自己写过违反后果——"三个架构的状态字位定义会与内核层字段类型混在一起……破坏 03 篇划定的边界"。现状正是位定义混进内核层。
- 项目第一原则（AGENTS.md）："抽象语义，不描述机制""硬件通过 trait 抽象，不把 CR3/PTE 位之类暴露给 OS 语义层"。`FS_OFF/FS_DIRTY` 是纯机制。
- 影响面：该点属 FPU 轮转路径（05 篇/FPU 专题射程），但 §4.1 的边界主张是全称命题，被同一 crate 内的这两处否定。
- 判定：**接受**（修法见 ARCH-PROPOSAL RD06-A01；文档 §4.1 需补"已知泄漏点"或收敛到 trait）。

## FINDING RD06-DESIGN-D2-14（P2）`CapabilityTemplate::Deferred` 生产零使用：RS 运行期授权走 wire 路径，不走模板 [DEL]

- 代码事实：`Deferred` 变体在 kernel crate 内只有一处构造（kpriv.rs:1572，`mod tests`）。运行期授权的真实路径是 RS 发 `PrivUpdateRequest` → `KPriv::apply_fields_from_request`（kpriv.rs），不经模板。
- 文档 §3.2 模板表把 `Deferred` 列为活跃角色（"用在哪：等 RS 授权的用户模块"），读者会以为 boot 循环给 10 个非调度模块授 Deferred——实际它们只置 `NO_PRIV|NO_QUANTUM`、根本不进授予路径（lib.rs:2729-2734）。
- 判定：**接受**——文档改述（Deferred 当前仅测试/未来 RS 动态授予的预留形状），或删变体。

## ARCH-PROPOSAL RD06-A01：把 FP 门控的 FS 编码收进 `CpuContextArch`

- 现状：内核层 `riscv_lazy_fs_field(is_owner) -> u64` + `CpuContext::set_fs_field(u64)`（均在 riscv64 cfg 下）。
- 提案：trait 增加方法 `fn prepare_fpu_gate(ctx: &mut Self::CpuContext, is_owner: bool)`（或 `fpu_trap_gate`），三架构实现各写自己的门控（x86-64/aarch64 无操作或写各自策略字段，riscv64 写 FS 域）；内核层调用语义化为 `<CurrentCpuContextArch as CpuContextArch>::prepare_fpu_gate(&mut p.cpu_context, fpu_owner == Some(picked))`，`riscv_lazy_fs_field` 移入 arch 层。
- 收益：内核层恢复"owner / non-owner"纯 OS 语义；`set_fs_field` 固有方法消失；§4.1 的全称边界主张重新成立。纯内部重构（Refactor 档），外部行为不变。
- 代价：trait 面加一个方法；05 篇/FPU 文档需同步（后者尚未重生成，正好趁窗口）。

## ARCH-PROPOSAL RD06-A02：表访问器的返回类型收敛为受 `BklSection` 借用约束的引用或守卫

- 现状：三族访问器全部返回 `&'static mut`（FINDING RD06-DESIGN-D2-07）。
- 提案（两档）：
  - A（彻底）：`pub fn proc_table_with<'a>(_section: &'a BklSection<'_>) -> &'a mut ProcessTable`——返回值存续期被 section 钉住，BKL 持有期内才可访问；`boot_unchecked` 收敛为 `&'static mut` 的唯一合法来源并要求 `#[allow]` 注释举证。波及 ~350 调用点，建议分两个 slice（先 witness 版、后裸版）。
  - B（最小）：返回类型改 `TableGuard<'a>`（Deref → ProcessTable），语义同上但调用点改动最小。
- 收益：把 §3.1 的"每个访问点自己举证"从口头纪律升级为类型层事实；两份 `&'static mut` 并存的别名 UB 面关闭。
- 档位：Refactor（外部行为不变）。

## 已核实一致（无 finding）的关键主张抽样

| 文档主张 | 核对点 | 结果 |
|---|---|---|
| §3.1 `PROC_TABLE`/`PRIV_TABLE` 形态、`nr_to_idx` 公式 | globals.rs:452/458、proc_table.rs:1692 | 一致 |
| §3.1 `ProcessTable::new` 镜像 C `proc_init`（nr/endpon/idle 三赋值） | proc_table.rs:86-106、C proc.c:128-157 | 一致（含 idle0 命名契约） |
| §3.2 trap_mask 的 CLOCK/SYSTEM 例外留在授予函数 | kpriv.rs:1194-1200、C main.c:218-219 | 一致 |
| §3.2 `fill_sendto_mask` 自位恒清 + 互惠置位 | kpriv.rs:941-1009、C system.c:305-328 | 一致（先读后借序） |
| §3.3 RTS/MF 位值逐位同 C（含跳过 0x2000） | proc.rs:126-230 | 一致（RTS 16 位、MF 18 位、EXT_REG_INITIALIZED=0x1000） |
| §3.4 `KERNEL_TASKS` 5 项、`BOOT_MODULE_PROC_NRS` 12 项定长 | proc.rs:93-124 | 一致 |
| §3.5 三函数收拢、`load_vm_elf` 为自由函数 | boot.rs 模块文档、mod.rs:35 记录行 | 一致 |
| §3.5 `enable_user_io` 收口（syscall_device.rs 走 trait） | syscall_device.rs:890 | 一致 |
| §3.7 `VmLoadError` 四变体 + ImageInsideSegments 防御分支 | boot.rs、lib.rs panic 调用点 | 一致 |
| §3.8 `SyncUnsafeCell` + sealed `BklProtected`（两处宏调用） | globals.rs:133-191 | 一致（另有 Option/[T;N] 组合 impl，文档未列，轻微） |
| §3.10 `KPriv` 8 子结构、wire 顺序同构、`to_wire` 截断扩展位 | kpriv.rs:81-409、capability.rs:129-131 | 一致 |
| §3.11 Priority u8 值域、CpuMask 单字、`allocate`/`consume` 不对称 | proc.rs:367-435、sched.rs | 一致（文档已披露 allocate） |
| §3.12 `init_post_and_memory` 三断言、注释偏离自报 | lib.rs | 一致（文档自报在先） |
| §3.13 fork FPU 策略继承三架构 + 条件位 | proc.rs fork 分支、三 arch boot.rs | 一致（三份 inherit_fpu_state 测试名齐） |
| §4.4 Drop 守卫、swap_slots 身份恢复、p_found/p_stale_tlb 无对位 | proc.rs:1139、proc_table.rs:139、debug.rs:55 | 一致 |
| §4.8 `bsp_finish_booting` 步骤序、BKL 断言 | lib.rs | 一致 |
| §5.1 测试条数 43/41/39/15/14/14/5/4 | `#[test]` 计数 | 逐一相符 |

## DIFF-TABLE（全量汇总）

| ID | 维度 | 分类 | P | 一句话 | 判定 |
|---|---|---|---|---|---|
| RD06-DESIGN-D2-01 | 状态/错误 | MODERN | P1 | Vm 模板 s_flags 多 PREEMPTIBLE，文档与代码相反 | 需用户裁决 |
| RD06-DESIGN-D2-02 | 死抽象/错误 | DEL | P2 | CapabilityError 两变体零构造，区分理由未落地 | 接受 |
| RD06-DESIGN-D2-03 | 错误 | DEL | P2 | errno 注释引错 C（ENONENT vs EINVAL/EBUSY/ENOSPC） | 接受 |
| RD06-DESIGN-D2-04 | 状态表达 | MODERN | P2 | IpcMask 越界静默忽略，注释自相矛盾 | 接受 |
| RD06-DESIGN-D2-05 | 状态表达 | MODERN | P2 | 内核 task 的 s_sig_mgr SELF vs C NONE | 需用户裁决 |
| RD06-DESIGN-D2-06 | 状态/并发 | MODERN | P1 | 内核 task 初始优先级 USER_Q(7) vs C TASK_Q(0) | 需用户裁决 |
| RD06-DESIGN-D2-07 | 并发/所有权 | MODERN | P1 | 访问器 &'static mut 别名面，witness 证明力空转 | 需用户裁决 |
| RD06-DESIGN-D2-08 | 抽象边界 | MODERN | P1 | 拆分 RTS 原语与文档"绑一次动作"主张相反 | 接受（改文档） |
| RD06-DESIGN-D2-09 | 死抽象 | DEL | P2 | EntrySpec KERNEL_TASK/DEFERRED 死区分 | 接受 |
| RD06-DESIGN-D2-10 | 抽象边界/性能 | MODERN | P2 | endpoint_to_nr O(n) vs C O(1) | 接受 |
| RD06-DESIGN-D2-11 | 非法状态 | MODERN | P2 | CpuMask 注释与派生 Default 相悖 | 接受 |
| RD06-DESIGN-D2-12 | 最小化 | DEL | P3 | allocate 测试专用；unset/clear 同体双名 | 接受 |
| RD06-DESIGN-D2-13 | 抽象边界 | ARCH | P2 | 内核层编码 sstatus.FS 位值 → RD06-A01 | 接受 |
| RD06-DESIGN-D2-14 | 死抽象 | DEL | P2 | Deferred 模板生产零使用 | 接受 |

## VERDICT

- **NOT PASS（作为设计基线）**。§3/§4 的骨架（两张表的存储模型、模板-授予分工、位图类型化、arch/kernel 边界、零堆约束）与源码核对基本扎实，但四处 P1 使"文档=权威设计陈述"的定位不成立：
  1. 两处文档-代码**方向性矛盾**（Vm 模板 s_flags、§3.9 的拆分 API 主张）——读者按文档会得出与代码相反的结论；
  2. 一处未披露的调度语义偏离（内核 task 优先级 7 vs 0）——超出 Refactor 档应走的 `[ARCH]` 标注程序；
  3. 一处并发叙事漏洞（`&'static mut` 访问器让 §3.1 的"每个访问点自己举证"失去类型层支撑）。
- 另有 5 条死抽象/死区分（CapabilityError 双死变体、EntrySpec 双常量、Deferred 模板、双名 unset/clear、allocate）指向同一模式：**枚举/常量的"区分理由"写在文档里，代码没跟上或根本不需要**——这批应整体过一遍最小化。
- 两条 ARCH-PROPOSAL（A01 FS 编码下沉、A02 访问器借用化）均为 Refactor 档，不改变外部行为，建议排入下一轮。
- 本遍未发现新的 P0 六类事实错误；FINDING-01 若裁决为笔误则升格 P0-design-deviation 候选。

- 本次扫描：14 条 FINDING，2 条 ARCH-PROPOSAL。
