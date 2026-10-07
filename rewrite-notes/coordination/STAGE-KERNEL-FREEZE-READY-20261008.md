> **创建**: `2026-10-08`
> **本文件管什么**: `rewrite-notes/01-stage-kernel/` 对应的 Rust 实现**能不能定稿**。用户接下来会按该目录的重排稿重新生成正式文档，如果实现里还挂着未完成的待办，或者文档写着"未实现"而代码其实早已落地，重生成出来的就是带着假缺口的定稿——返工不可避免。本文件给的就是这两件事：**哪些叙述已经过期**，以及**哪些边界必须写成显式推迟**。
> **核实方式**: 纯静态读码。快照 `87784ca04`。凡结论依赖真机跑通，一律写 `待验证` 并指明解锁需要哪道门；本轮不跑构建、不跑虚拟机、不改 `os/` 代码。
> **配套阅读**: 待办的状态与证据在 `TODO-LEDGER-OPEN.md` §1；已闭项与裁决在 `TODO-LEDGER-DONE.md` §1、§2。

# 01-stage-kernel 代码面冻结就绪度

## 结论先说

**按"次级核存活已冻结 + 次级核承载进程显式推迟"这条边界，`01-stage-kernel` 可以进入文档重生成；但必须先把十五组过期叙述回勾掉，并接受三篇文档暂时不能定稿。** 依据是一个方向非常一致的事实：**这个目录的账面普遍落后于代码，而不是超前于代码**。本轮逐条对账下来，"文档说没做、代码已经做了"共十五组（覆盖十九处叙述），"文档说做了、代码查无此事"只有三组，且三组都属于表述过宽而非结论造假。

正式文档的数量口径先钉一下：`rewrite-notes/README.md` 记 38 篇，`ls rewrite-notes/01-stage-kernel/[0-9]*.md` 实测 38 个编号文件，其中 `06-todo.md`（待办台账）、`07-paging_init_gpt.md` 与 `18-trap-bridge-design.md`（外部评审稿）不属于正式文档，**正式文档为 35 篇**。重生成时按 35 篇计，别再引用 38 这个混合口径。

## §1 逐文档对账

标记命中数来自 `grep -c "待补\|DEFERRED\|未接线\|未实现\|TODO\|已知缺口" <文档>`，是**账面声明**；判据列是对 `os/` 的实测。

| 文档 | 标记命中 | 对应 Rust 落点 | 判据（账面声明 vs 代码事实） | 冻结判定 |
|---|---|---|---|---|
| `00-kernel-overview.md` | 0 | `os/kernel/src/lib.rs:fn kmain` | 分层叙述与代码一致 | 可定稿 |
| `01-boot-shim-bootstrap.md` | 6 | `os/boot-shim/src/`、`os/kernel/src/misc.rs:fn prepare_boot` 的 `OS_RELEASE` 填充 | 记"版本串字段尚未实现"→ **已实现**；记真机集成测试链待补 → 属实 | 回勾后可定稿 |
| `02-higher-half-kernel.md` | 0 | `os/arch/src/{x86_64,arm64,riscv64}/paging.rs` | 一致 | 可定稿 |
| `03-kmain-cstart.md` | 0 | `os/kernel/src/lib.rs:fn kmain` | 一致 | 可定稿 |
| `04-platform-discovery.md` | 14 | `os/libs/minix-platform/src/global.rs:static PLATFORM` | 记"须换成互斥锁或每核一份"→ **已用一次性冻结方案替代，该待办整段作废**；记 riscv64 的固件路径返回空切片 → 属实 | 回勾后可定稿 |
| `05-clock-interrupt-init.md` | 4 | `os/plat/src/interrupt.rs:trait InterruptRouter`、`trait PerCpuInterruptUnit`、`os/arch/src/x86_64/clock.rs:fn init_local_timer` | 记"中断特征按每核局部性拆分推迟"→ **已拆分落地**；记 x86 的 ACPI 表解析待做、该架构初始化文件零测试 → 属实 | 回勾后可定稿 |
| `06-proc-init-boot-proc.md` | 2 | `os/kernel/src/proc_table.rs`、`os/kernel/src/proc.rs:enum ProcKind` | 命中都是概念叙述不是缺口 | 可定稿 |
| `07-cross-space-init.md` | 0 | `os/kernel/src/cross_space.rs:fn data_copy_vmcheck` | 一致 | 可定稿 |
| `08-system-init-boot-finish.md` | 1 | `os/kernel/src/memmap.rs:fn add_memmap` | 记两个信息块字段未实现 → **已裁定为设计上不需要，且举证无读者** | 回勾后可定稿 |
| `09-vm-boot-protocol.md` | 6 | `os/kernel/src/syscall.rs` 的虚存控制臂、`os/kernel/src/vm_handoff.rs` | 同上，两处"待补/推迟"实为已裁定的不需要 | 回勾后可定稿 |
| `10-switch-to-user.md` | 7 | `os/kernel/src/lib.rs:fn finish_and_restore`、`os/kernel/src/proc_table.rs:fn process_misc_flags` | 记"每核内核时长统计未接线"→ **已裁定为不需要**；记投递挂起未接线 → **已接**；记恢复腿为简化版 → 属实；记采样轮询 → 半真 | 回勾后可定稿（简化版与采样两条保留为显式未完成） |
| `11-scheduling-primitives.md` | 1 | `os/kernel/src/lib.rs:fn pick_and_bill`、`os/kernel/src/sched.rs` | 记"计费指针未更新"→ **已更新** | 回勾后可定稿 |
| `12-ipc-core.md` | 3 | `os/kernel/src/ipc.rs:fn copy_msg_from_user`、`os/kernel/src/proc_table.rs:fn vm_enqueue_and_notify_vm` | 记"投递仍未接线"→ **已闭**；记"待特征化"→ **已是特征方法** | 回勾后可定稿 |
| `13-syscall-dispatch.md` | 2 | `os/kernel/src/syscall.rs:fn dispatch_privctl`、`fn kernel_call_resume` | 表体是已完成叙述；分派臂通过数与"一百六十"这一读数静态不可核 | 可定稿（读数改标"待复跑验证"） |
| `14-exception-interrupt.md` | 3 | `os/arch/src/arch/exception_dispatcher.rs`、`os/kernel/src/trap_dispatch.rs` | 记浮点陷入未接线 → **分架构**：riscv64 与 aarch64 的轮转腿已通，x86_64 的归属写入为零、命中即显式停止；记随机性采样未调 → 属实（低优先） | 不能定稿：须先补 x86_64 的归属腿，或把"门控已生效、恢复未接"写成显式边界 |
| `15-clock-timer.md` | 0 | `os/kernel/src/clock.rs:fn local_tick`、`fn rearm_local_tick`、`fn tick_with` | 一致；但缺"次级核本地定时器如何驱动调度"一节 | 可定稿（新增小节要等次级核承载，写成边界） |
| `16-smp.md` | 14 | `os/kernel/src/smp.rs:fn smp_init`、启动锁、`fn smp_ap_tail`、`os/arch/src/x86_64/protection.rs:fn init_ap` | 三处大内核锁推迟 → **全部已落地**；次级核初始化占位 → **已是带断言的真实实现**；待补测试清单 → 属实 | 回勾后可定稿，但必须按 §4 写死两条边界（次级核存活已通、次级核承载显式推迟） |
| `17-syscall-process.md` | 3 | `os/kernel/src/syscall_process.rs` | 命中是历史叙述（删除线条目） | 可定稿 |
| `18-syscall-copy.md` | 8 | `os/kernel/src/syscall_copy.rs`、`os/kernel/src/grant.rs:fn verify_grant`、`os/kernel/src/pte_walk.rs` | 端到端与集成测试缺口属实；记"五十四个测试"实测 56 | 回勾后可定稿 |
| `19-syscall-signal.md` | 9 | `os/kernel/src/syscall_signal.rs`、`os/arch/src/{riscv64,arm64}/signal.rs` | 记诊断控制到信号推迟 → **已实现**，且原文把投递目标也写错了（真实目标是注册者自身） | 回勾后可定稿 |
| `20-syscall-device.md` | 4 | `os/kernel/src/syscall_device.rs`、`os/kernel/src/irq_manager.rs:fn notify_hardware` | 一致；行号锚点需重跑 | 可定稿 |
| `21-syscall-clock.md` | 4 | `os/kernel/src/clock.rs`（定时器递减、时间调整、批量投递三处） | 附录记三条未实现 → **全部已实现**；同篇"行为测试待补"与正文"已补齐"自相矛盾 | 回勾后可定稿 |
| `22-privilege.md` | 2 | `os/kernel/src/kpriv.rs`、`os/kernel/src/capability.rs` | 一致；恢复腿锚点行号漂移严重（记 3056 实为 3782） | 回勾后可定稿（仅锚点重跑） |
| `23-ipc-filter.md` | 7 | `os/kernel/src/ipc_filter.rs:fn chain_allowed` | 一致 | 可定稿 |
| `24-cross-space-runtime.md` | 7 | `os/kernel/src/vm.rs:struct VmRequestQueue`、`fn dequeue_filtered`、`fn enqueue_and_notify` | 记"未接入请求队列"→ **已接入**；记挂起类型的一个变体未被使用 → 该变体已不存在（账面陈旧） | 回勾后可定稿 |
| `25-misc-unported.md` | 15 | `os/kernel/src/misc.rs:fn dispatch_getinfo` | 多数推迟已解除；记未用分派函数为死码 → **属实且应删** | 回勾后可定稿 |
| `26-watchdog.md` | 4 | `os/kernel/src/trap_dispatch.rs:ExceptionOutcome::SpuriousNmi` | 记"不可屏蔽中断子系统未实现"→ **向量已处置**，措辞要改；锁死检测维持不做 → 属实 | 回勾后可定稿 |
| `27-kernel-utility.md` | 12 | `os/kernel/src/kmess.rs:struct KmessRing`、`fn snapshot_ordered`、`os/kernel/src/syscall.rs` 的诊断控制注册臂 | 对照表六行记"未实现"→ **全部已实现**（本目录最集中的一处账面滞后）；另记字符写入口有喂入者 → **无**（只有字节入口被调用） | 回勾后可定稿 |
| `28-usermapped-data.md` | 0 | 各架构陷入入口段的 `#[used]` 与节属性 | 一致 | 可定稿 |
| `29-kernel-debug.md` | 0 | `os/kernel/src/debug.rs:fn runqueues_ok` 等五个公开函数 | 无待办标记，但**这五个函数没有任何生产调用方**，唯一消费者是集成测试；文档把它们当内核调试设施叙述 | 需先定归属再定稿 |
| `30-kernel-profile.md` | 0 | `os/kernel/src/misc.rs` 的采样常量与挂钩 | 一致 | 可定稿 |
| `31-fpu-context-switching.md` | 4 | `os/arch/src/{riscv64,arm64}/fpu.rs`、`os/kernel/src/trap_dispatch.rs` 两臂、`os/kernel/src/lib.rs:fn finish_and_restore` | 记"不实现轮转"→ **riscv64 与 aarch64 已实现**（跨架构表失真）；记信号路径保存浮点 → **确实缺**（两架构信号上下文文件里查无浮点） | 不能定稿（同 `14`，等 x86_64 归属腿） |
| `32-stack-tracing.md` | 2 | `os/kernel/src/stacktrace.rs:fn util_stacktrace`、`os/kernel/src/lib.rs` 的崩溃钩子调用 | 记"内核栈回溯仍未实现"→ **已实现且已进生产崩溃路径** | 回勾后可定稿 |
| `33-syscall-caller-api.md` | 0 | `os/kernel/src/syscall.rs` 的调用者按号传递腿 | 一致 | 可定稿 |
| `99-global-concepts.md` | 0 | 跨模块概念 | 一致 | 可定稿（内容更像概念词条，建议整体并入 `rewrite-notes/concepts/`） |

## §2 必须回勾的过期叙述（文档说没做、代码已做）

这一节直接决定重生成质量。**不回勾就重生成，新文档会把已完成的功能写成待办**，下一轮评审又要推翻重来。

| 文档与位置 | 文档原话 | 代码事实 | 核实命令 |
|---|---|---|---|
| `27-kernel-utility.md` §6.2 对照表 | 消息环、环快照取出、有序快照、诊断注册码等六行标"未实现" | `os/kernel/src/kmess.rs:struct KmessRing`、`fn snapshot_ordered`、`fn copy_snapshot_to_caller` 与 `os/kernel/src/syscall.rs` 的诊断控制注册臂全在，并有生产调用方 | `grep -rn "KmessRing\|snapshot_ordered" os/kernel/src/kmess.rs` |
| `32-stack-tracing.md:376,570` | 内核栈回溯仍未实现，崩溃只有停机 | `os/kernel/src/lib.rs` 的崩溃钩子里直接调用 `os/kernel/src/stacktrace.rs:fn util_stacktrace` | `grep -n "util_stacktrace" os/kernel/src/lib.rs os/kernel/src/stacktrace.rs` |
| `16-smp.md:1053`—`1055`、`1058`、`1088`、`1091`、`1154` | 异常入口取锁、启动早期取锁、循环前释放锁、`smp_init`、启动锁、次级核初始化共七处标推迟或占位 | `os/kernel/src/lib.rs` 有早期取锁与回装前释放；`os/kernel/src/trap_dispatch.rs` 按用户态起源取锁；`os/kernel/src/smp.rs:fn smp_init` 与启动锁成段；`os/arch/src/x86_64/protection.rs:fn init_ap` 是带断言的真实实现 | `grep -n "bkl_lock().transfer()" os/kernel/src/lib.rs; grep -n "pub fn smp_init" os/kernel/src/smp.rs; grep -n "fn init_ap" os/arch/src/x86_64/protection.rs` |
| `10-switch-to-user.md:310,439`、`12-ipc-core.md:977` | 投递消息首次缺页的挂起"仍未接线" | `os/kernel/src/proc_table.rs` 的投递挂起分支已路由到挂起与入队通知 | `grep -n "VmSuspendType::DeliverMsg" os/kernel/src/proc_table.rs` |
| `11-scheduling-primitives.md:498` | 未更新计费指针 | `os/kernel/src/lib.rs:fn pick_and_bill` 写入计费指针，`os/kernel/src/proc_table.rs:fn set_bill_to_idle` 配对 | `grep -n "bill_ptr" os/kernel/src/lib.rs` |
| `10-switch-to-user.md:434` | 每核内核时长统计是"已知缺口" | 已由设计裁定为不需要，理由与举证写在 `os/kernel/src/lib.rs` 对应段 | `sed -n '3325,3345p' os/kernel/src/lib.rs` |
| `04-platform-discovery.md:417`—`421` | 平台描述符的多核就绪后必须替换 | 已替换为一次性冻结方案（写一次、之后只读） | `grep -n "static PLATFORM" os/libs/minix-platform/src/global.rs` |
| `05-clock-interrupt-init.md:464` | 中断特征按每核局部性拆分推迟 | 已拆成路由特征与每核单元特征两个 | `grep -n "trait InterruptRouter\|trait PerCpuInterruptUnit" os/plat/src/interrupt.rs` |
| `24-cross-space-runtime.md:402` | 挂起路径未接入请求队列 | 已接入；且该文档引用的四个行号锚点全部漂移 | `grep -n "struct VmRequestQueue" os/kernel/src/vm.rs` |
| `21-syscall-clock.md` 附录 B 前三行 | 定时器到点发信号、时间调整增量消费、时钟通知未实现 | 三处均已实现 | `grep -n "tick_virt_timer\|n_batch" os/kernel/src/clock.rs` |
| `19-syscall-signal.md:800` | 诊断控制投递到进程管理的信号推迟 | 已实现注册路径，且真实投递目标是注册者自身（文档写错对象） | `grep -n "DIAGCTL_CODE_REGISTER" os/kernel/src/syscall.rs` |
| `08:777`、`09:585`—`596` | 两个信息块字段 Rust 未实现 | 已裁定为设计上不需要并举证无读者 | `sed -n '64,82p' os/kernel/src/memmap.rs` |
| `01-boot-shim-bootstrap.md:734` | 版本串两个字段尚未实现 | 已由 `os/kernel/src/misc.rs` 的字段填充函数落地 | `grep -n "str_to_kinfo_field" os/kernel/src/misc.rs` |
| `26-watchdog.md:275` | 不可屏蔽中断子系统未实现 | 向量已有处置（伪中断归类），只有锁死判据维持不做 | `grep -rn "SpuriousNmi" os/kernel/src os/arch/src` |
| `12-ipc-core.md:659` | 消息取回待特征化 | 已是特征方法并有五处实现 | `grep -n "fn copy_msg_from_user" os/kernel/src/ipc.rs` |

`01-stage-kernel/todo.md` 的**文内自相矛盾**要在同一笔里消解：`:135`—`137`（§7.1）与 `:254`—`256`（§10）仍把 `D-36`、`D-37`、`D-38①` 列为开放并标注"阻塞于多核"，`:185` 仍写"调度循环为占位"，而 `:574`、`:611` 已给出 `I-6` 全条闭合的结论。以代码为准的正确表述是：`D-36`、`D-37`、`D-38①`、`I-6` 四条均已闭合；真正没做的是**次级核承载进程**，那是另一件事，不能用"多核未做"一句话盖住四条已完成的账。`edge1.md` 的"已闭单勿领"一节其实已经写明该表未刷新，本轮只是把这个已知事实落到台账。

## §3 反向风险（文档说做了、代码查无）

三处，都不是造假，但定稿即失真：

| 文档与位置 | 文档表述 | 实测 | 该怎么改 |
|---|---|---|---|
| `29-kernel-debug.md` §5.3 | 把队列不变量检查、标志位写出、进程转储当内核调试设施叙述 | `grep -rn "runqueues_ok" os --include=*.rs \| grep -v debug.rs` → 命中全在集成测试；`print_proc` 与标志写出无生产消费者 | 二选一：接进崩溃路径或诊断通道，或降级表述为"当前仅用于集成测试"并登记死码处置 |
| `27-kernel-utility.md` §6.2 之外的段落 | 崩溃诊断叙述成立，但未写明消息环的字符写入口没有喂入者 | `grep -rn "console_write_str" os --include=*.rs` → 只有定义；生产侧只有字节入口被调用 | 补一句"内核打印路径当前不注入消息环" |
| `14-exception-interrupt.md`、`05-clock-interrupt-init.md` 的"三架构对齐"叙述 | 读起来像三架构中断面等价 | riscv64 的软件中断使能明确留给多核中断下发（`os/arch/src/riscv64/arch_init.rs` 注释自陈此处刻意不开）；两架构到信号的执行腿在源码里写作"随下一波" | 改成**分架构对照表**，每格给"已通 / 登记缺口 / 不适用"，禁止再出现笼统的"三架构对齐" |

## §4 真缺口与冻结裁决

### 不属于"次级核承载"、因而不能靠推迟条款封闭的缺口

| 缺口 | 为什么不是推迟项 | 现在必须怎么办 |
|---|---|---|
| x86_64 的浮点归属从未被写入 | 单核同样触发：每次恢复上下文都把浮点单元关掉，用户态一条浮点指令就进显式停止分支。与次级核无关 | 要么接归属写入与恢复腿，要么把 `14`、`31` 两篇的相关章节写成"门控已生效、恢复未接"的显式边界并标风险面（当前用户态是否使用浮点需真机确认） |
| riscv64 软件中断与外部中断臂、aarch64 生成中断臂缺失；两架构用户同步异常不到信号 | 属三架构中断面完整性，不在次级核链上 | 文档按 §3 的分架构对照表写清；`14` 不能定稿 |
| 指令缓存维护缺失 | 与次级核无关，是"数据转指令"路径的系统性缺口 | 文档写明当前没有承载它的抽象 |
| 用户栈十六字节对齐只有 x86_64 | 属运行时约定 | `10` 里按架构标注缺失 |
| 死码与无消费者设施若干（未用分派函数、消息环字符入口、调试队列检查） | 属卫生与归属决策 | 删除或在文档里注明保留理由 |
| x86 的 ACPI 表解析未做 | 平台发现面真实缺口（设备树路径已完成） | `05`、`04` 保留为真缺口 |

### 裁决（无人值守下按推荐项自决）

**采甲案。** 把"次级核承载进程、解除主核钳位"显式划入本轮不做，文档按两条边界写死：

- **边界一，次级核存活**（已冻结，可以写进定稿）：次级核能被启动、能进内核、有本地定时器与向量、能收投递、调度主循环在次级核上可达。凭据：`os/arch/src/{arm64,riscv64,x86_64}/ap_early_entry.rs` 的完整入口体、`os/arch/src/x86_64/clock.rs:fn init_local_timer` 与 `0xf1` 向量、`os/kernel/src/smp.rs:fn smp_ap_tail` 尾部的调度循环入口。
- **边界二，次级核承载**（显式推迟，文档必须写成推迟而不是缺口）：进程被真正安放到次级核运行。缺的四件都在 `TODO-LEDGER-OPEN.md` §1：中断入口唤醒空闲核的清零臂、空闲标志的清零站点、次级核放行邮箱的生产者、跨核内存腐蚀的根治。钳位在解除上述条件前保留，其**可观测效果对位 C 的非多核形态**（接受请求、进程留在主核、返回成功），这是正当过渡态而不是遗忘。裁决依据 `PD-02`：本轮不承诺完整多核，撤钳子门的前置清单必含 `PD-19` 的投递腿完成屏障与跨核读对账。

采甲案后，`16-smp.md` 可以定稿（写这两条边界），`15-clock-timer.md` 的次级核小节也可以定稿。**被否的乙案**是"先补齐上述四件再冻结"：代价是把文档定稿时间挂在一条需要真机的攻坚线上（跨核腐蚀本身还是 `待验证`），而它并不改变已经落地的次级核存活事实；同时会让 `01-stage-kernel` 的重生成无限期等待，与本目录文档已经明显落后于代码的现状相反。

## §5 重生成前的备忘（只登记，不在本轮实施）

1. **代码注释按文件名引用本目录文档**：`os/` 里提及 `06-proc-init-boot-proc.md` 六十三处、`16-smp.md` 十八处、`11-scheduling-primitives.md` 十一处、`04-platform-discovery.md` 二十四处。重排若改名、拆分或合并，**改名必须与代码注释同一笔提交完成**，否则断的是生产代码里的锚点。复跑判据：`grep -rhoE "[0-9]{2}-[a-z0-9-]+\.md" --include=*.rs os \| sort \| uniq -c \| sort -rn`。
2. **门禁对文档名的硬依赖**：`tools/check-rs-unwired.sh:6-16` 要求生产占位符的同处或前五行注释里出现 `NN-rs-*.md` 形态的文档引用。文档改名会让这道门当场失败。
3. **行号锚点大面积漂移**：本轮扫描发现复刻块里的工具派生行号大面积失配（符号名不漂移、行号漂移）。重生成时应改用符号锚点，或跑 `tools/anchor-resolve.sh --check <文档>` 复核；`tools/doc-style-lint.sh` 的 SL-10 也要求把纯坐标移出正文。
4. **引用热点**：本目录被引最多的五篇（`16-smp`、`06-proc-init-boot-proc`、`14-exception-interrupt`、`11-scheduling-primitives`、`25-misc-unported`）合计占跨文档引用的约四成。这五篇的重排方案要最先定，否则改动会波及整个笔记树。
5. **本文件与重排稿的关系**：`doc_rerank_*.md` 六份是重排提案，本轮**不评判其内容**，只回答"代码能不能定稿"。两件事合起来的执行顺序是：先按 §2 回勾过期叙述 → 再按 §4 写边界 → 然后按重排稿生成 → 生成时按 §5 第 1、2 条同步改代码注释与门禁引用。

## 一句话总结

`01-stage-kernel` 的 Rust 实现**已经比它的文档更接近定稿**：三十五篇里三十二篇可以回勾后直接定稿，三篇（`14-exception-interrupt.md`、`31-fpu-context-switching.md`、`29-kernel-debug.md`）因浮点归属、中断臂与设施归属的真实缺口不能定稿；`16-smp.md` 在写死两条边界的前提下可以定稿。把"次级核承载进程"写成有裁决依据的显式推迟，就能在不阻塞文档重生成的前提下把多核缺口留在台账里。
