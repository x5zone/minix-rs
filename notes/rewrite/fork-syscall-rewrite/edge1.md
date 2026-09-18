# edge1 — 内核 · 架构 · QEMU bring-up 线（三线并行之一）

> **定位**：本文件是把 [edge_todo.md](edge_todo.md) 与 01~18 各 stage todo 中的未闭合条目按依赖关系拆成三个可并发工作线之一的**临时分工索引**。条目的权威描述仍在原 todo 文件（每条附链接），本文件只做范围圈定、前置标注与状态记账。全部完成后的归档规则见 [edge4.md](edge4.md) §8。
>
> **所有权（本线可独占修改，其他线触碰须走 edge4 认领板）**：`os/kernel/`、`os/arch/`、`os/plat/`、`os/boot-shim/`、`os/qemu-tests/`、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/`、`notes/rewrite/fork-syscall-rewrite/00-master-plan/`。
>
> **并发规则**：见 [edge4.md](edge4.md) §1。核心三条：①只改所有权内文件；②前置属其他线时先读对方文件状态，未 ✅ 就等待或先做别条；③进度只记本文件 + edge4 状态板，**不直接回写 edge_todo.md / 各 stage todo.md**（由 edge4 批量收敛，避免并发写冲突）。

状态图例：☐ 未开工 ｜ 🔄 进行中 ｜ ⏸ 等待（注明等谁）｜ ✅ 完成（日期+commit）｜ 🚫 维持登记不排期

---

## K 组条目（全部可开工，无跨线硬前置）

| 编号 | 条目 | 来源 | 要点 | 前置 | 状态 |
|---|---|---|---|---|---|
| K1 | E-PREEMPTFLAG 余项：enqueue Phase 3 抢占门激活 | [edge_todo.md](edge_todo.md) E-PREEMPTFLAG ｜ [06-stage-sched/todo.md §2](06-stage-sched/todo.md) | 抢占分支生产不可达（调用方全传 `current_nr=None`）。(a) current 来源改读 `CpuLocal.proc_ptr`；(b) 抢占门消费 `KPrivFlags::is_preemptible`（C proc.c:1638）。SCHED 服务器零改动 | 无（CpuLocal 基建 S-6 已就位） | ✅ 2026-09-18（本提交；生产壳读 `try_smp_state`+特权表，参数化核心 `sched_enqueue_with` 消费真标志；requeue 的硬编码 BSP 改读进程自身 CPU；4 新测试；doc 11 §2.2/§3.5 同步并修正 C 引用失真） |
| K2 | E-SCHEDSMP：SCHED cpu 下发链三环 | [edge_todo.md](edge_todo.md) E-SCHEDSMP ｜ [06-stage-sched/todo.md §3](06-stage-sched/todo.md) | (1) per-CPU Scheduler 入 CpuLocal，`sched_for_cpu` 按 cpu_id 分发；(2) `sched_proc` 补 cpu_is_ready 校验（EBADCPU）；(3) 跨 CPU 迁移接线（`schedule_migrate_proc` 已有本体，缺调用方）。SCHED 侧零改动 | 无硬前置；验收挂 edge4 E5(e) | ✅ 2026-09-18（本提交；环① 按 S-6.3 §3.5.3 冻结决策记账取代——BKL 下就绪队列共享串行化，per-CPU 队列行为退化为单队列；环② validate_cpu_param 纯函数 + EBADCPU 接线；环③ 迁移守卫按 system.c:672-677 内联 sched_proc（非 dispatch_schedule，修正文档漂移）；3 新测试；doc 11 §4.6 同步） |
| K3 | E-VMTLB 余件：SMP IPI 旗标设置完备性 | [edge_todo.md](edge_todo.md) E-VMTLB ｜ [02-stage-vm/todo.md V13-P2-1(c)](02-stage-vm/todo.md) | 机制三件套已落（pick 点判定/switch_to_user 消费/设置点）。余件 = `schedule_vminhibit` IPI 路径的旗标设置完备性 + E5 SMP 冒烟用例「fork 后父子并发写 CoW 页」设计（用例执行归 edge4） | 无 | ✅（本提交；完备性缺口实证并修复——本地臂设 VMINHIBIT+FLUSH_TLB 而 IPI 臂只设 VMINHIBIT，C do_vmctl.c:133-135 的 FLUSH_TLB 是 if/else 外无条件设置，已重构为 C 形 + 拆两个可宿主测试助手 + 2 新测试；冒烟用例设计落 smp_todo.md §27，断言清单草案 a1-a5 随附） |
| K4 | T-13 dm_coverage 测试族共享 mock 无同步 | [01-stage-kernel/todo.md](01-stage-kernel/todo.md) L411 | `os/arch` dm_coverage driver 测试共享全局 mock 无锁，并行调度确定性失败。加 `BKL_TEST_LOCK` 同型互斥（misc.rs `SPROF_TEST_LOCK` 先例）或各测试用不重叠窗口基址 | 无 | ✅ 2026-09-18（本提交；std 语境取 RAII `Mutex<()>` 序列锁，4 测试各持 guard；并行 8 线程 5/5 轮绿，自旋锁先例的 panic 泄挂死风险已记录 fix-status） |
| K5 | profiling trap 入口传 PC | [01-stage-kernel/todo.md](01-stage-kernel/todo.md) §12.3 L332 | `dispatch_sprofile` 已全实现，缺 trap 入口接线传采样 PC。原依赖 S-8/S-9 已于 2026-09-14/15 完成，**当前即可做** | 无 | ✅ 2026-09-18（本提交；C-parity 全接线：`PROFILE_CLOCK_IRQ` 常量 + `IrqManager` hook 注册/摘除 + trap 入口单发 PC 槽 + `profile_clock_hook` 采样；kernel 764 绿含 3 新 tick 测试，plat pin ×3；doc 30 同步） |
| K6 | x86 AP LAPIC local timer | [01-stage-kernel/smp_todo.md](01-stage-kernel/smp_todo.md) L848 | per-CPU TSC 校准 + `lapic_set_timer_one_shot`（C arch_clock.c:131-139 对应）。不接则 AP 上 quantum 递减停摆——SMP 真分片的实际缺口 | 无（S-8 IDT 已就位） | ☐ |
| K7 | S-8 B 链 disjoint-API 重构 | [01-stage-kernel/smp_todo.md](01-stage-kernel/smp_todo.md) L1676 | syscall 入口 caller-in-table 别名当前以 SAFETY 注释裸指针分裂表达，重构收敛 | 无 | ✅（本提交，第一阶段收敛）——两处分散裸指针逃逸收敛为单一具名构造器 `ProcessTable::caller_slot_mut<'a>`（C 真实语义的完整 SAFETY 证明集中一处 + SLOT_FREE debug 断言；两处已漂移的重复注释删除；IPC 臂前段改走安全 `get_mut`）。**完整 disjoint-API（caller-by-nr，10 文件 139 个 `caller: &mut KProcess` 签名）为战役级重构，登记 K20 独立排期** |
| K8 | OQ-13a：GIC/PLIC claim 返回 IrqVector 演进 | [01-stage-kernel/todo.md](01-stage-kernel/todo.md) L186 | I-13 主体已闭合，余 claim→`IrqVector` 返回值演进，随 per-CPU 分发 lane（与 K1/K2 同窗顺带） | 无 | ✅ 2026-09-18（本提交；`claim() -> Option<u32>` + `complete(Option<u32>)` 类型化配对——比 OQ 原案更进一步的 `u32` 原始 claim 身份：GIC INTID 超 u8 会截断配对完成；假 signal 1023/PLIC 0 完成跳过；`last_iar`/`last_claimed` 暂存字段删除；三架构+mock+IrqManager 适配） |
| K9 | arm64/riscv64 trap 向量表链接地雷 | [01-stage-kernel/smp_todo.md](01-stage-kernel/smp_todo.md) L1705 | 两架构 `load()` 引用的 `exc_vector_table`（arm64）/`trap_vector`（riscv64）只有 extern 声明、无 global_asm 定义，靠死代码消除掩盖。**三架构用户态目标的硬前置** | 无 | ☐ |
| K10 | riscv64 SSIE 软件中断 IPI 路径 | [01-stage-kernel/smp_todo.md](01-stage-kernel/smp_todo.md) L871 | S-10 IPI 往返仅 x86 LAPIC 实测；riscv64 走 SSIE 路径（S-4 仅做能力准备） | K9 同批为宜 | ☐ |
| K11 | arm64/riscv64 shutdown L7 三架构验证 | [01-stage-kernel/smp_todo.md](01-stage-kernel/smp_todo.md) L1879 | 后端代码已就位（semihosting SYS_EXIT / sifive_test FINISHER_PASS），缺真机验证。**K5 期间发现**：`os/plat/src/arm64/shutdown.rs` 在 aarch64-unknown-none target 下本身编译不过（:26 noreturn asm 带 inout 输出 + :18 E0308，HEAD 存量），真机验证前先修编译 | 无 | ☐ |
| K12 | test-user-trap / test-rt-birth 纳入 run_all.sh 主线 | [01-stage-kernel/smp_todo.md §26](01-stage-kernel/smp_todo.md) | 两脚本目前独立运行未入 x86 一键回归/CI。纳入即验收阶梯 T1 的 x86 半收口 | 无 | ✅ 2026-09-18（本提交；特殊协议脚本区——gdbstub 信箱/串口标记 PASS 判定 + SKIP(exit 2) 计数；test-user-trap 入构建清单；test-rt-birth 因 `include_bytes!(env!)` 由脚本自建不入普通清单；edge4 §2 登记销账；本地真机两脚本 PASS） |
| K12b | minix-rt 诞生链 + trap 腿的 aarch64/riscv64 真机化 | 本文件新增（三架构目标推导） | test-rt-birth 目前仅 x86。将 rt-birth 测试内核移植到 AAVMF/OpenSBI 载体，验证三架构 CPL3 诞生链 + int/syscall 腿——依赖 K9 的向量表落地 | K9、K12 | ☐ |
| K13 | T-10 riscv64 U-Boot 启动链（CI 环境） | [01-stage-kernel/todo.md](01-stage-kernel/todo.md) L170 | U-Boot fatload→bootelf 集成测试；需 u-boot-qemu/mkimage 工具链，本地缺 sudo 则在 CI workflow 做 | CI 环境 | ☐ |
| K17 | E5(d) QEMU VM paging 冒烟的测试内核载体 | [edge_todo.md](edge_todo.md) E5 验收面增补 | boot shim 拉起 VM → `init_vm_self_pt` → map/query/unmap → **缺页完整回路**（VM 写进程硬件 PTE → 恢复 → 指令重执行）的 qemu-tests 载体与实现。断言清单与编排归 edge4 E5(d)。触碰 `os/qemu-tests/`（本线所有） | VM 参战（edge3 S20 可宿主先行，真机联调挂 edge4） | ☐ |
| K19 | os/kernel/tests/boot_integration.rs 编译破坏（K5 期间发现） | 本文件新增（edge4 §1.7 规则 7） | 该集成测试 3 处引用已被重构删除的 `CpuLocal.scheduler` 字段（:227/:259/:290，`cargo check -p minix-kernel --all-targets` HEAD 即 4 error），拖累 kernel 全目标构建与 CI。修法：改走现行 CpuLocal API（enqueue 经 `Scheduler` 实例）。HEAD 存量，非任何在制改动引入 | 无 | ✅ 2026-09-18（本提交；入队改走 `ProcessTable::sched_for_cpu_mut(BSP)`——S-6.3 共享队列决策后 Scheduler 在表上；all-targets 4 error 清零；boot_integration 3 passed/3 ignored（ignore 为既有 COM1 SIGSEGV 理由）） |
| K20 | syscall 层 disjoint-API 战役：caller-by-nr（K7 第一阶段衍生的完整版） | 本文件新增（K7 处置拆分） | `caller: &mut KProcess` 签名遍布 10 文件 139 处（syscall.rs/cross_space/vm/misc/grant/syscall_{clock,process,signal,copy,device}），完整 caller-by-nr 重构需先出设计轮（outline→design→分批实施），涉及 kernel_call 三件套 + 46 dispatch 臂。触发时机：S-6 CpuLocal 语义演进或 unsafe 审计要求时立项；现状由 `caller_slot_mut` 单点契约承载 | 设计轮 | ☐ |

## D 组（文档与登记类）

| 编号 | 条目 | 来源 | 要点 | 状态 |
|---|---|---|---|---|
| K14 | I-1 kernel 独立 ELF 构建（DEFERRED） | [01-stage-kernel/todo.md](01-stage-kernel/todo.md) L183 | 需入口交接 ABI 设计 + boot-shim 终局跳转。三架构 QEMU 目标走 boot-shim 链即可达成，**维持登记不排期**，出现真实需求再立项 | 🚫 |
| K15 | 15-todo-fixes 状态对账回写 | [00-master-plan/15-todo-fixes.md](00-master-plan/15-todo-fixes.md) | 阶段 2/3（VM/kernel 半）实际已按 PFN 模型实现，文档标"❌"过时——回写事实。**本文件归 edge1 独占**；阶段 4/5/6（VFS/PM）的状态结论由 edge3 经 edge4 状态板传递后由本线统一回写 | ☐ |
| K16 | 06 文档 v2.2 瘦身 + review-line-check.sh 工具 | [01-stage-kernel/06-todo.md](01-stage-kernel/06-todo.md) L579/L690 | 纯文档/工具债，两轮体检清单已给出（2028→约 1550 行）。低优先 | ☐ |
| K18 | 维持登记三件（不排期） | [01-stage-kernel/todo.md](01-stage-kernel/todo.md) | I-5 ACPI（物理机前提）、D-65② boot 旋钮通道（条件立项）、L1 内存序学习 backlog（doc 11/16 review 触发） | 🚫 |

---

## 已闭单勿领（防止并行线重复劳动；以 edge_todo.md 最新进度注记为准）

- E1 trap 桥 / E2 SYS_* wrapper / E6 PM wrapper / E9 wrapper 五域面 / E-REQWIRE / E-FORKMSG / E-BOOTFRAME / E-MIBGRANT / E-MINTYPES-SYS / E-MINSYS-HYGIENE / E-VMMOCK / E4 register_free / E-KERNINFO（内核半+用户态半均闭环，余归档注记）/ E-VFSWIRE / E-VMTLB 机制半 / E-PREEMPTFLAG live 半 / E-SCHEDNICED / E-RSWIRE 两侧半 / E-ISKMESS。
- 01-stage 的 D-36/D-37/D-38① 已随 S-5/S-9 闭合（smp_todo §23/§25），todo.md §7.1 表未刷新，勿重做。
- smp_todo S-0~S-13 主线全部封存，勿重开。

## 本线在最终验收阶梯中的位置（全文见 [edge4.md](edge4.md) §7）

T1（三架构用户态门槛）：K12 → K9 → K12b → K10/K11。T5 的 SMP 正确性面：K1/K2/K3（用例执行在 edge4 E5）。
