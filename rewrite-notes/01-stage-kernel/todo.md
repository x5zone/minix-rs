# 01-stage-kernel TODO 总清单（V13，2026-09-09 压缩归档版）

> **台账状态摘要**（对照代码逐条核实于 `2026-09`–`10` 本轮，快照 `9f752834c`，口径：全量）：`open` 3 ｜ `stopgap landed` 4 ｜ `partially fixed` 3 ｜ `adjudicated` 6 ｜ 需真机或需逐臂对 C 真源判读 2。
> **本文件内部口径的自相矛盾已按代码刷新（`2026-10-09` 回勾，claim/SK-FREEZE-REFRESH-qoder）**：§7.1、§10、§12.2/§12.3 曾把 `D-36`（多核初始化）、`D-37`（启动锁）、`D-38①`（异常入口取锁）列为开放并注"阻塞于多核"、把调度循环记为占位；§23/§24 与代码事实是四条均已闭合（`os/kernel/src/smp.rs:fn smp_init`、`os/kernel/src/lib.rs:fn scheduler_loop`、`os/arch/src/x86_64/protection.rs:fn init_ap`，逐条凭据见 `../coordination/STAGE-KERNEL-FREEZE-READY-20261008.md` §2 与其核实命令）。真正没做的是**次级核承载进程**（`TODO-LEDGER-OPEN.md` §1 钳位行），那是另一件事，不能用"多核未做"盖住四条已完成的账。
> 另有 `T-10`（通用引导器对照腿）已按裁决 `PD-15` 放弃，不再属于"保持推迟"；`I-5`（物理机平台发现）与 `I-1`（内核独立镜像构建）维持不排期。
> 跨阶段联动项的状态权威在 `../coordination/TODO-LEDGER-OPEN.md`（§1 是内核与多核）；本文件的条目描述与修法仍是权威，本轮只加本摘要不改条目。

> 本文件是 01-stage-kernel stage 的 TODO 权威清单。**2026-09-09 压缩归档**：已完成条目
> 压缩为一行结论（条目号 + 一句话 + 日期），完整细节可从 git 历史找回——压缩前最后
> 全量版为 commit `893386cd8`（2897 行）；更早的原始 18 节历史见 `97df4e58d^`。
> 压缩只删"已闭环的论证过程"，不删任何 open 条目、不删防重查备忘（§21.3 / §7.6 / 非缺口表）。
>
> 历史结构（压缩后去向）：
> - §0.1 处理状态总览 → §0.1（压缩为批次表）
> - §0 审查基线 / §0.5 学习 backlog / §0.6 硬件抽象化 backlog → 原样保留
> - §1-§6 架构建议（A/B/C/D/E/F/G/M/R 系）→ §1 总览 + §1.1 一行解决记录
> - §7 未完成项汇总（D/W/I/T 系）→ §7（open 保留全文，resolved 压缩）
> - §H 历史归档（968 行原始 18 节 + 清空归档）→ 删，见 git `97df4e58d^`；转移清单结论并入 §7.7
> - §18.5 / §19 / §20（GPT 评论分析）→ §8（结论压缩，含 第一项先决 未修核实）
> - Step 1.0f 漂移表 → 并入 §23（V13 复核仍未修）
> - OQ-15-1 / OQ-D49-1 / D19-1 → §9
> - Edge Items 表 → §10
> - §21 V12 轮 → §11（结果表 + 非缺口备忘保留）
> - §22 收敛执行计划 → §12（Phase 3 SMP 与 open 项保留，done 压缩）
> - §23 V13 轮（本轮新增，扫描进行中）

## 0.1 处理状态总览（压缩为批次表；行内日期 = 修复落地日）

| 批次 | 处理 | 一行结论 |
|------|------|---------|
| 2026-08-14 修复会话 | 5 项 | F1/F2（pt_alloc 论证模型 + register 同步）、D3（命名错误类型 6/7）、D-51（PrivUpdateRequest 字段序冻结）、D-53（cpu_identify + GET_CPUINFO 全记录） |
| 2026-09-05 三批 | 5 项 | D-6（dispatch_exit 全 cause_sig）+ D-11（cause_sig 致命 SELF 路径）+ D-57（proc_stacktrace 接入）+ D-13（sig_delay_done 双路接线）+ D-43/D-45（SC_TRACE→SIGTRAP、Segfault→SIGSEGV） |
| 2026-09-05 第四/五批 | 2 项 | D-49（set_sendto_bit 全链 + P0 顺带 p_priv 回链）+ D-58（boot 模板 fill_sendto_mask，自位恒清） |
| 2026-09-06 第六批 | 12 项 | D-8 kernel_call wrapper、D-14 no-op、D-15 CLOCK 通知、D-20 vm_enqueue_and_notify_vm、D-24/D-25 no-op、D-33 read_tsc 熵采样 [ARCH]、D-34 no-op、D-35 远程 IPI、D-44 vm_suspend(DeliverMsg)、D-46 软件半环、D-52 sched_proc 完全对齐 C、D-17/D-16/D-18（IPC 过滤三件套） |
| 2026-08-14 二次核实 | 18 项 | D-1~D-5/D-7/D-10/D-12/D-19/D-21/D-22/D-23/D-26~D-29/D-31/D-32 实为已实现（原记录过时，grep 证据在案） |
| 2026-09-08 Phase 4/5 | 8 项 | E1（Arch 关联类型聚合）、G1（ProcNr 上移 minix-types）、R1（核实闭合 CTOS）、C1（4 dispatcher 拆分）、M1（gen-test-kernel.sh）、U-1（alarm_timer 保全）、U-2（swap_memreq 真实现）、I-7/I-9/I-10（评估闭合维持现状）+ SYS_PADCONF（奇偶闭合） |
| 2026-09-08 Phase 6 | 7 项 | T-1（10/10）、T-6（10 测试）、T-7（5/5）、T-12（5/5 + dispatch_clear 幂等修正）、T-9（3 测试）、T-11（runqueues_ok 集成）、T-5（QEMU CI workflow） |
| 2026-09-07~08 §22 Phase 1/2 | 12 项 | V12-A1 trap_style（含 MINIX3 BUG 修复）、V12-B1 panic 审计、V12-A2/A3/A4、V12-B2（IoBatchBuf 对齐 UB 修复）、V12-B3（Message 72B 实测）、V12-B4、A1（assume_held + 24 处迁移）、A2（globals.rs）、B1（transfer API）、B-X 第一批、S-2b/S-3a/S-3b/S-3c |
| 2026-09-09 D1/D2 + Phase 7 | 4 项 | D1/D2（Errno newtype + ToErrno，架构建议清零）、I-14（B 层完成）、I-3（核实闭合） |
| 2026-10-12 01 篇 CH1-CH2 人审 | 3 项登记 | I-17（§2.6 拆节）/I-18（章头与巨节体检）/I-19（讲解条续扫）；同轮正文修正另见台账 `prompt/WORKFLOW-OPEN-ITEMS.md` 第四节第三轮 |
| 2026-10-13 01 篇 §2.3–§3.2 人审续 | 1 项登记 + 读数刷新 | I-20（boot-shim 再导出注释与调用点不符）；I-17/I-18 体量读数现算刷新（提交 `8dc20636b`）；同轮正文修正见提交 `2eac4586e` |
| `2026-10-13` SD-20/SD-42 落地（代码侧） | 文档同步 2 篇 | 装载器尾的指令缓存同步契约（`[ARCH: cache-sync]`）与 aarch64 设备内存属性（`[ARCH: mmio-device]`）已进代码并带形状钉；文档半待补：`06-proc-init-boot-proc.md` §2.7（`arch_boot_proc` 与 VM ELF 装载）需补「装载器尾部做指令缓存同步」一段与标注，`04-platform-discovery.md` 的 MMIO 高半窗口/§1.114 段需补「设备窗口按 2 MiB 分片映射为 Device、恒等与高点两别名同属性」一段与标注（三处一致纪律的文档半；代码半见 `coordination/TODO-LEDGER-OPEN.md` §1 同日行） |
| 2026-10-20 01 篇 `reserved_regions` 答疑派生 | 2 项登记 | D-59（`supports_1gb_page` 双 trait 入口收口：`HugePages` 的 latent 方法 vs `DmCoverageArch` 的生产路径）/ D-60（`IDENTITY_MAP_END` 与 `BOOT_IDENTITY_MAP_END` 同值双定义）；两项均为抽象收口、不改行为，详见 §7.1 |
| 2026-10-18 生产链 memmap 硬编码议题 → 同日架构裁决升级 | 1 项登记 + 同日刷新 | I-22（初登"候选甲/乙/丙三案" → 同日二轮升为"丁案：抽 `MemoryMapSource` trait 收口三链取内存图"最佳设计版；用户明示「不计成本、专期完成」；连带 01 篇 L1265/L1945/L2288 + opensbi_helpers 顶注 + bootface 头注 + 04 篇平台发现契约一处需同批动；opensbi 侧常量与单区断言测试删除；与 coordination 两链合并议题脱钩） |
| 2026-10-18 直核形态 fw_payload 文档缺口 | 1 项登记 | I-23（01 篇 `fw_payload` 术语零命中，§3.7 L1695 列三形态但读者易误判直核=QEMU 测试专用；补一小段介绍 OpenSBI `FW_PAYLOAD` 规范形态 + 三架构落位矩阵，明写「x86 无对应形态因平台规范强制 BIOS/UEFI、aarch64 覆盖测试为主、riscv64 兼具测试与真板嵌入式」；`[ARCH]` 标注三处一致纪律要打上；建议与 I-22 同批落地避免 §3.7 diff 冲突） |
| 2026-10-18 §3.1 「内核自带 stub」与 bootface 高度重合 | 1 项登记 | I-24（L1310 主句把「内核自带 stub、自己读文件再自举」列为被否，括号补丁又承认 opensbi 形态不在否决之列——主句描述与今天 bootface 直核形态动作一致、只差固件形态一字；且「同一个二进制的两个入口」这句与 L1410 依赖方向段对不上。候选三案推荐甲拆两行「UEFI 形态被否」/「opensbi 形态采纳」；触发窗口与 I-22 同批动、可并入 I-23） |
| 2026-10-15 W-10/W-11 登记与重构轮（台账 `prompt/WORKFLOW-OPEN-ITEMS.md`） | 1 项登记 | I-21（篇头「路径约定」本 stage 13 篇重复、「标注约定」仅 01 篇独有但 `[ARCH]` 解释 4 篇两套标签）；标 📌 延后决策，当前不做，冻结后再议；连带耦合已写明：W-10 工序表两处措辞需同批改 |
| 2026-10-20 01 篇 param_buf / §3.4 repr(C) 边界 / 三结构体答疑派生 | 2 项登记 | D-66（`param_buf` 生产侧接入：UEFI Load Option / SBI 参数 → key=value，代码注释标记 P9-1，消费侧已对齐 C 零改动）/ D-67（boot 信封合一 + 跨镜像布局全钉死：`BootPrepareResult` 并入 `BootHandoff` 成 `#[repr(C)]` 单信封，切片 `(ptr,len)` 与 `Option` 表示 repr(C) 化，"同批构建"从正确性前提降级为部署细节；建议随 NK1/OQ-N6 同批；`VmBootHandoff` 明确不在射程）；详见 §7.1 |
| 2026-10-20 01 篇关联代码设计审查（五分片） | 6 项登记 + 2 项回勾 | **新登记** D-68（bootface_a64 第四条 memmap 链未迁 `DtbMemoryMap`，I-22 漏网）/ D-69（`build_kernel_info` 双版本漂移面，随 D-67 同批）/ D-70（`store_kernel_info` 落地区缺 `platform_sources`/`param_buf` 两块，param_buf 半为 D-66 硬前置）/ D-71（`BootForm::source_path` 悬空路径 + 形态测试无存在性检查）/ I-26（opensbi_helpers 孤儿注释 + "hardcoded" 顶注与 I-22 实况矛盾）/ I-27（01 篇 L1422/L2624「还没裁决」与 OQ-N6 已裁口径不一致）；**回勾** I-22/D-60 代码已落地（台账 §7.1 读数待刷）。工单全文与证据命令见 `01-todo.md`；设计裁决载体 `01-code-design.md`（自包含简报 T1–T5，供网页端 GPT 多轮评审）；既有条目 D-59/D-66/D-67/I-1 的本轮增量亦摘于 `01-todo.md` §3。**2026-10-29 回勾（01TODO-B1）**：D-71 / I-26 / I-27 已闭合，D-68/D-69/D-70 仍待各自批次；台账 §7.1 读数随本批刷新，TODO-02/TODO-03 测试缺口同批补齐。**2026-10-29 回勾（01TODO-B3）**：D-68 已闭合（bootface_a64 迁 `DtbMemoryMap`，Phase A.2 按真实 totalsize 永久扣除 DTB，aarch64 直核冒烟 PASS），D-69/D-70 仍待各自批；**2026-10-29 回补**：D-69/T5-D-70 已随 T1-5a/T5 闭合（单一 builder；五块落地区 845/0/8） |
| 📌 保持 DEFERRED（依赖仍成立） | — | **I-1**（kernel ELF 构建无引导路径）、**I-5**（ACPI RSDP 无消费方）、**T-10**（U-Boot 工具链，`PD-15` 已裁放弃）、**profiling deferred 体**（等 S-8/S-9）；Edge 表旧 12 项中 SMP 块的 `D-36`/`D-37`/`D-38①`/`D-39` 四条已于 S-4/S-5/S-7 收官时闭合（§23/§24 在案，`2026-10-09` 按代码回勾本行与 §7.1/§10——真正仍开的 SMP 残项是**次级核承载进程**，那是另一件事，不能用"多核未做"盖住四条已完成的账） |
| 2026-10-28 01 篇全量 review（09 轮评审终态后补录） | 5 项新登记 + 3 处文档漂移 | **新登记** D-72（bump 池耗尽结构债：kerninfo 注入链每新根抽 1–3 页、池无回收、bn9m 真机 ~40 次绑定死亡、4 MiB 每次启动扣除、结构修复两方向待裁）+ N-1~N-5（`BootAlloc::resume` 区间自洽校验 / `vm_handoff` 头注释与实现不一致 / 零消费者包装 `load_kernel_elf`·`load_boot_modules` / bootface 双份名字池与常量独立清理增量 / 池页零化机制归因调查）；**文档漂移**：01 篇 §5.2.1 测试计数（opensbi 13→12、paging 14→15、算式 29→28）、§4.6 `IDENTITY_MAP_END` 措辞（D-60 后已是别名）、`globals.rs` 陈旧 `allow(dead_code)` 注释；工单全文与证据命令见 `01-todo.md` §2.7/§3/§5 |

## 0. 审查基线 — 已确认的良好架构（无需改动）

| 维度 | 现状 | 评价 |
|------|------|------|
| 分层与依赖方向 | `kernel → arch → plat/platform → types/boot/elf`，依赖单向无环 | ✅ 干净 |
| no_std | 全链 `#![no_std]`（boot-shim 部分 cfg 后 std），无 std 泄漏 | ✅ |
| 全局状态安全 | `SyncUnsafeCell<T>` + sealed trait `BklProtected`（`bkl_protected_impls!` 宏）+ `BklSection` witness，声明集中于 `os/kernel/src/globals.rs` | ✅ typestate 水准（R-01/R-03/A2） |
| syscall 分派 | `enum Syscall` + `try_from` + match；`CurrentArchSyscall` ZST trait 化 | ✅ 优于 C call_vec[] |
| 硬件抽象 | 16+ 细粒度 trait（Paging/Protection/TrapEntry/Exception/Clock/Fpu/Smp/…）+ 每 arch Mock + `Arch` 关联类型聚合（E1） | ✅ |
| 跨空间拷贝 | `cross_space_copy<D: DirectMapArch>` 泛型核心 + `cross_space.rs` vmcheck 封装 | ✅ |
| 依赖注入 | syscall dispatch 显式传表；kernel_call_finish 已去全局化（A1） | ✅ 混合模式 |
| boot 契约 | `minix-boot::KernelInfo`（含 `validate()`）为 kernel/boot-shim 唯一契约 | ✅ |
| 占位符 | `todo!`/`unimplemented!` = 0；`unsafe impl Sync/Send` 全库 4 处真实实现均有论证 | ✅ 基线 |
| 平台发现 | `PlatformContext` write-once（AssumeSyncCell）+ `&'static dyn PlatformDesc` | ✅ |
| 错误通道 | `Errno(i32)` newtype + `ToErrno` trait 于 minix-types 单一权威（D1/D2，2026-09-09） | ✅ |

## 0.5 学习 / 概念理解 backlog（与代码改动分离）

### L1. `happens-before` 与内存序（C `volatile` 背景 → Rust 内存模型） [学习]

**盲点自述**（2026-08-31）：对 `Acquire`/`Release`/`Relaxed` 的形式化含义理解停留在现象层；
`06-proc-init-boot-proc.md` §3.3 的内存序论证只给了现象未给"如果去 BKL 会怎样"的完整推理。

**待深入**：① happens-before 的形式化定义（相对 C volatile 的差异）；② 单字段防撕裂
（所有 Ordering 都有）vs 跨字段次序（只有 Acquire/Release 建立）两层保证；③ Acquire 防的是
"load 之后的访问被重排到 load 之前"，不是 load 本身被重排；④ 读侧 Acquire 必须配写侧
Release 才形成 happens-before 链。

**递进路径**：06 §3.3"关键反推"段（已写）→ 16-smp BklSection 实现（`xchg` + 屏障）对照 →
11-scheduling-primitives 写作时推演"去 BKL 后 `p_nextready` 的 Relaxed 是否要升 Acquire"。

**触发条件**：启动 doc 11 或 doc 16 review；启动去 BKL 重构。是 backlog 不是 bug——
当前论证对已修目标足够，强行深挖阻塞主线。

## 0.6 硬件抽象化加固 backlog（B-X）

原则（CLAUDE.md）：`#[cfg(target_arch)]` 仅允许"定义 current* 类型 / trait 实现点 /
`naked_asm!`+`asm!` 指令字面量（语法硬约束）"；**禁止**在行为逻辑中用 cfg 选行为。

**✅ 第一批已落地（2026-09-07，§22 Phase 2 迭代 4）**：新增 `minix-platform::test_support`
（unit_test_irq_desc + ARCH_NAME/SP_LABEL/PC_LABEL/FP_LABEL/REACHED_BANNER 常量族）；
kernel 侧 13 处行为选择 cfg 消除（kmain_verify 9 常量块 + 1 打印行 + `new_test_interrupt_controller`
三变体合一）；kernel 侧行为选择 cfg 清零。验证：kernel 694 + platform 13 全绿，三架构
production-target check 全过。

**剩余 open（✅ 全部收口，2026-09-09 V13 终批）**：
- `os/kernel/src/lib.rs` DirectMap 三 cfg 分支 → 改用既有根别名
  `minix_arch::CurrentDirectMap::default()`（三分支 ZST 加 `#[derive(Default)]`；
  别名早已存在于 arch lib.rs 根，kernel 未用而已）。kernel 侧行为选择 cfg 归零复验通过。
- `minix-platform/src/global.rs` 其余 cfg：逐处核实为"定义 current/trait 实现点"，
  合法（PlatformDescEnum 裁定沿用）。**B-X 关闭**。
- `os/libs/minix-platform/src/global.rs` 约 10 处 `#[cfg(target_arch = "x86_64")]`
  （global.rs:95-175 `PlatformDescEnum` 9 处经裁定为"枚举变体按架构存在性裁剪"= 合法；
  其余待逐处核实是否属行为选择）。

**触发条件**：doc 16 review 重启 / 新增 arch 平台时 cfg 重复暴增 / 再次发现行为选择 cfg。

## 1. 架构建议（原 §1-§6；全部闭环，保留总览与一行解决记录）

| ID | 层级 | 主题 | 结论（日期） |
|----|------|------|------|
| A1 | kernel | 全局访问器 witness 全覆盖 | ✅ `BklSection::assume_held()` 链根 + 调度/IRQ/boot 链 24 处迁移 + kernel_call_finish 去全局化；debug 断言当场揪出 8 个测试不持锁隐患（2026-09-07） |
| A2 | kernel | 全局静态收敛 | ✅ `os/kernel/src/globals.rs`（396 行：13 静态 + 包装器 + BklProtected 审批清单）；访问器族零改动（审计点单一化已足够，搬运无收益——记录防重查）（2026-09-07） |
| A3 | kernel | KProcess/KPriv Drop 防御 | ✅ 占用槽隐式销毁 panic fail-fast；测试豁免夹具三层；详见 `panic-in-drop.md`（2026-09-02） |
| B1 | kernel | BKL guard 跨函数传递 | ✅ `BklGuard::transfer(self)`（ManuallyDrop，全库唯一 `mem::forget` 收敛点）+ kernel_call_finish 入口 `debug_assert!(bkl_is_locked())`；11 处散点收敛（2026-09-07） |
| C1 | kernel | dispatch 大函数拆分 | ✅ 4 函数语义拆分：getinfo 468→35 / privctl 446→111 / vmctl 349→157 / trace 294→139（statectl 62 行不再 qualify）；verbatim 提取 + 46 处文档锚点同步（2026-09-08） |
| D1 | 全层 | errno newtype 单一来源 | ✅ `Errno(i32)` 于 `os/libs/minix-types/src/types/errno.rs`（唯一权威），kernel errno.rs 再导出（2026-09-09） |
| D2 | kernel | ToErrno 映射 trait | ✅ trait 于 errno.rs；impl：SchedProcError（sched.rs）/ PmError / KernelError（minix-types）；2 调用点迁移，手写映射删除；范围收窄说明：kernel 侧手写映射实测仅 1 处（2026-09-09） |
| D3 | kernel/arch | `Result<(), ()>` 清理 | ✅ ProfileClockError + WriteUserRegError（trait 签名级，三 arch + mock）；`vm::enqueue_and_notify` 按 R-18 保留（单一失败模式，全库仅剩 allow）（2026-08-14） |
| E1 | arch | 聚合 `Arch` trait | ✅ **落地形态与草图偏差（load-bearing 设计事实）**：supertrait 形式不可实现——Paging 是有状态句柄族（`from_active_root`），ZST 架构体无法伪造；改为全关联类型组合（17 关联类型覆盖 20 个 Current* 别名）+ `os/arch/src/arch/current.rs` 单点 cfg + MockArch 矩阵镜像（TypeId pin 测试）；既有别名不动、新代码优先 `CurrentArch`（OQ 并存裁决）（2026-09-08） |
| F1 | arch | pt_alloc 论证模型 | ✅ 改写为 SMP+BKL write-once-then-read-only 论证（os/arch/src/arch/pt_alloc.rs）（2026-08-14） |
| F2 | arch | register 同步 | ✅ debug_assert 防重复 + Release/Acquire；**trait 化演进已否决**——kernel 不做长期 PMM，FrameAllocator trait 无 owner（`os/arch/src/arch/frame.rs` 模块 doc "What this module does not own"），防 premature abstraction（2026-08-14） |
| G1 | 跨层 | ProcNr 双定义 | ✅ newtype 迁 `os/libs/minix-types/src/types/proc_nr.rs`（Redox ContextId 于 redox-syscall 同款）；arch 删别名改 `pub use`；kernel re-export ~600 使用点零 diff；边界拆包 3 处归零（2026-09-08） |
| M1 | qemu-tests | test-kernel 配置模板化 | ✅ `tools/gen-test-kernel.sh`（--new/--apply/--check）+ 23 文件规范化 + 死 profile 块删除；每架构聚合 crate 方案否决（feature unification 破坏 test-proc-init 无 alloc 约束）（2026-09-08） |
| R1 | arch | PTE 位权威位置 | ✅ 核实闭合（登记前提过时，Pattern #70）：`PageFlags` 语义位图（`os/arch/src/arch/paging.rs:28`）+ 三架构私有 raw 翻译层 + kernel 全消费点语义化早已达成；**关联常量形式被否决**——把 raw 位值以统一名暴露给共享层弱于现状封装（CLAUDE.md 硬件约束按更强者执行）（2026-09-08） |

### 1.1 Redox 对照结论（原 §6，2026-08-14 调研 + 后续落地）

同构且方向已正确：syscall 中心 match、usercopy 独立模块、共享 ABI crate 承载错误类型
（D1 落地）、全局锁 + 无 static mut（我们 sealed trait 更严格）、edition 2024 + panic=abort。
我们占优：细粒度 arch trait（Redox 用 cfg 换模块）、宿主单元测试密度（743 vs Redox ~19）。
Redox 领先且已对标：`CleanLockToken` 全链传递（我们 A1 assume_held 是雏形，S-8 trap entry
witness 根位补齐后可全链化）；rmm 独立内存管理 crate（F2 已论证不建——无 owner 抽象不建）。

## 7. 文档 01-30 未完成项汇总

### 7.1 DEFERRED 代码任务（D 系）

**✅ 已解决（44 项，一行备忘）**：D-1~D-13、D-15~D-35、D-42~D-58 全部闭环（落地批次见 §0.1；
逐条解决记录含 C 锚点与测试证据，见 git `893386cd8` 版 §7.1 表）。其中 🟢 设计 no-op 六项：
D-14（SIGKMESS 随 W-7 演进不可达）、D-24（VMSTYPE_MAP **C 有定义也有消费臂**（`proc.h:101`、
`do_vmctl.c:102-104`），但全树**无生产者**——`vm_suspend` 只用 `VMSTYPE_KERNELCALL`
（`arch/i386/memory.c:444,567,661`）与 `VMSTYPE_DELIVERMSG`（`proc.c:282`），`VMSTYPE_SYS_NONE`
是原始 `type` 的零值；本树枚举因此正好覆盖有生产者的两值，no-op 结论不变、理由改正为「无生产者」而非「无此变体」）、D-25（无部分进度
消费方）、D-30（swap 链恒空前提；**后被 U-2 推翻改真实现**，见 §12 Phase 5）、D-34
（mmap_size/mem_high_phys 无读者，write-only 状态不维护）、W-8 同 D-30。

**✅ 旧"仍 open"表——`2026-10-09` 全部按代码回勾闭合（与 §10 Edge 表同源）**：

| # | 项 | 依赖 |
|---|-----|------|
| D-36 | ~~`smp_init`（ACPI/MADT 表解析 + `SmpArch::boot_ap`）~~ ✅ S-5（`2026-09-15` 收官，见 §12.2；硬件清单实际走 KernelInfo+DTB 平台描述符而非 ACPI/MADT——I-5 自陈该设计走向，非缺口） | S-4/S-5 ✅ |
| D-37 | ~~`boot_lock`（smp.c:28）~~ ✅ S-5（`os/kernel/src/smp.rs:855` 起的 boot_lock 单例与 AP finish handshake） | 随 D-36 ✅ |
| D-38① | ~~exception_dispatcher::handle 需 bkl_lock~~ ✅（`os/kernel/src/trap_dispatch.rs:105` 入口 `bkl_lock_or_inherit()`，②③④ 早已闭合） | S-9 ✅ |
| D-39 | ~~x86_64 `init_ap` panic 占位~~ ✅ S-4（2026-09-14 真实现：AP 侧装载 + GS/MSR；16 字节 TSS 描述符布局 bug 同步修复——见 smp_todo §22） | S-4 ✅ |
| D-40 | ~~ptproc per-CPU（现由 `CURRENT_PTPROC_NR` 全局单核承担）~~ ✅ S-6.1（全局原子退役 → CpuLocal.ptproc；身份锚 gs:0x10） | S-6a ✅ |
| D-41 | ~~`PLATFORM` AssumeSyncCell SMP 替换~~ ✅ S-6.2（Frozen<T> 冻结原语，minix-types；PLATFORM 已迁移） | S-6b ✅ |

**⬜ `2026-10-28` 新登记（01 篇全量 review 补录；全文与证据命令见 `01-todo.md` §2.7/§3 N-1~N-5）**：

| # | 项 | 状态与修法 |
|---|-----|-----------|
| D-72 | ✅ 已实施（01TODO-D72VM，2026-10-30）：`VmBootHandoff` v7 加 `kerninfo_pa`，VM 在 `init_page_table` 统一映射 `KERNINFO_USER_VA`，内核 `vmctl_set_addr_space` 注入块删除，源码序钉 2/0，kernel mock 845/0/8、VM 543/0、run_all 39/0/0。原登记：**bump 池耗尽结构债**：`prepare_boot(1024)` 的池无回收路径——kerninfo 注入链每个新根绑定（exec 重建根）抽 1–3 页/次，真机（bn9m）在 ~40 次绑定后 `AllocationFailed` 死亡；1024 页 = 4 MiB 每次启动从 VM 可选内存整段扣除（`vm_handoff` 按 `boot_alloc_region()` 全段保留）。代码注释自陈这是 runway 不是终态（"only postpones the deterministic panic"），结构修复两方向未裁：**可回收页表页** / **kerninfo 映射责任移交 VM**（`kerninfo.rs` doc 的 `.usermapped` 口径方向）。**性质**：01 篇射程内唯一"已跑通的临时方案被当作终态"的结构债；不影响当前启动、exec 密集负载下是确定性上限。**修法**：**2026-10-28 已裁（八轮评审，通过候选 A）**——映射责任移交 VM，所有权三分（内核持有 `KERNINFO_PAGE`、VM 持有映射与中间页表页），`VmBootHandoff` 加字段并升载荷版本（VM 拒绝不支持版本），`fork`/`exec` 分别验收，4 MiB 启动池预算在候选 A 后按实测峰值复核；仓库侧前置已核实（`init_kerninfo` Phase B.5 早于 `build_vm_handoff` Phase C；fork/exec 都在 VM 新建根、不继承映射）。**现算命令**：`sed -n '38,62p' os/boot-shim/src/main.rs`；`grep -n "init_kerninfo\|new root\|replicating" os/kernel/src/kerninfo.rs`；`grep -n "boot_alloc_region" os/kernel/src/vm_handoff.rs` |
| N-1 | `BootAlloc::resume`/`region` 缺区间自洽校验：三值裸 store，`alloc` 只判 `pa >= end`、不判 `pa < base`——跨镜像 wire 字段 `bump_base/next/end` 未验证即信任，倒置/越界会让页表页落进未登记区域或产出倒置区间。修法：T1 wire 验证清单加「bump 三件套区间自洽」（必做）；`resume`/`region` 入口断言为可选加固 + 宿主三类倒置输入测试 | ✅ 已修（01TODO-B1，2026-10-29）：两入口断言 + 五枚宿主测试（1 合法 resume / 3 类倒置越界 / 1 region 侧倒置 init）；T1 wire 侧检查仍在 T1 清单 |
| N-2 | `vm_handoff.rs` 模块 doc 扣除表行 "reserved module blobs … **minus the VM module**" 与实现不一致（`classify` 为含 VM 全量保留，成修理由：VM 启动前其段帧从本 free list 取出；"除 VM 外"只属交接模块清单 `VmBootHandoff.modules`）。01 篇 §3.8 已指出旧口径，代码注释待改：拆两行分述。验收 `grep -n "minus the VM module" os/kernel/src/vm_handoff.rs` 归零 | ✅ 已修（01TODO-B1）：模块头与 `classify` doc 两处均改两清单分述，01 篇 §3.8 同批 |
| N-3 | 零消费者便利包装 `uefi_helpers::load_kernel_elf`/`load_boot_modules`（`pub fn` 文档称供集成测试，全仓零调用方）→ 删除；`build_memmap()`（单数）保留（两个 qemu 测试内核在用）。验收：`grep -rn "load_kernel_elf\|load_boot_modules\b" os --include="*.rs"` 归零（`_with_loader` 保留） | ✅ 已修（01TODO-B1）：两函数已删，minix-elf doc 改指 `loader::load_kernel_with_loader`，全仓 grep 归零 |
| N-4 | `bootface.rs` ↔ `bootface_a64.rs` 名字池与常量重复收敛：两处头注都写"留给独立清理增量"但台账无独立条目；与 I-22 削界双份同型。修法：列全重复面（`NAME_POOL`/`MODULE_NAMES`/`DTB_PA`/`KERN_SIZE` 等）→ 能下沉 `boot-shim` 库的下沉、架构真差异的留两份并互指注释；不合并引导形态层。随 D-68 落地后做 | ✅ 已修（01TODO-B4）：名字池/纯函数闸/编译期对账下沉 `kernel-image/src/bootface_common.rs`；`say`/`say_hex`、`place_modules`、布局常量留两份互指；两架构直核冒烟 PASS |
| N-5 | 池页零化机制归因未定案（"页表不重放" vs "复用清零"）：01 篇 §3.4 两条候选各带可检验推论；T1 受控复制不依赖该归因，但证据链缺最后一环。修法：按两条推论各做一次定向读数（切根前后同址回读 / 推迟消费探针）→ 结论落盘 → 01 篇相应句改定案或标明判别结果 | ✅ 已取证（01TODO-N5）：快照后 LOADER_DATA 页三时点校验和一致（verdict=replayed）、in_snapshot_free=0——映射重放成立、复用前提未出现；剩余解释为深拷前入口引用链（NK4-A），01 篇 §3.4 已改写 |
| D-73 | **riscv64 独立 boot-shim 腿（三架构统一为主线）**：主线 = shim + 独立内核镜像 + 交接载荷；自引导面（`bootface`/`bootface_a64`）降为特定嵌入式形态（FW_PAYLOAD / `-kernel` 测试），保留但不作主产线。shim 腿主体已在（`OpenSbiBootShim`/`UbootFileLoader`/`BootFileTable`/`bump_alloc_pages`）但零接线（bin 只按 `fw-uefi-image` 构建、`prepare_boot` 无调用方）；已知缺口五项（bin/feature 词表、U-Boot 下设备树通道、`bootstrap_to_kernel_image` 泛化、内核镜像两形分道判据、池基去 QEMU 硬编码）。**2026-10-28 已裁（八轮评审，有条件通过）**：先闭合入口契约（输入契约）再接线（U-Boot 版本与 `bootelf` 文档化参数协议 —— `bootelf [-p|-s] [-d <fdt_addr>] [<image_addr> [<arg>]...]`，参数以 `argc/argv` 交应用；入口适配层解析校验，不依赖 `a0/a1`；装载责任与地址/长度/内存范围进契约；页池等 QEMU 常量改 DTB 推导；两内核产物 `kernel-direct.elf`/`kernel-shim.elf` 入口契约）。直核自引导面称「辅助启动路径」；QEMU ≠ 真板。裁决回填 `01-code-design.md` §0.3/§2.3；详见 `01-design-note-riscv-unify-d72.md` §五 | ⏳ open，待实施（入口契约闭合先行） |

**⬜ `2026-10-20` 新登记（待修，抽象收口；均不改行为）**：

| # | 项 | 状态与修法 |
|---|-----|-----------|
| D-59 | 1 GiB 页能力查询双 trait 入口收口（`HugePages::supports_1gb_page` vs `DmCoverageArch::supports_1gb_page`） | ✅ 五阶段全部实施（D59PAGE/D59SIZE/D59POLICY/D59DEFEND/D59RENAME；合并 4dc6a797f/8d99bb774/f9bab95c2/5479b8083，末轮 run_all 39/0/0）。原登记：⏳ open（2026-10-20 设计修订完成，待实施）：**设计文档**：`rewrite-notes/01-stage-kernel/boot-dm-hugepage-design.md`（2249 行；已按代码与五轮评审重写；`tools/doc-style-lint.sh --strict` 0 error，`tools/doc-code-density-check.sh --check` 通过）。**第五轮收尾**：`KernelMapPolicy` 保存规划时验证的 `KernelLayout`，`DmPagePolicy` 保存 `DirectMapLayout`，执行入口从策略取布局；`build_bootstrap_root_and_enable` 只接收 `root_page` 与 `KernelMapPolicy`，`establish_boot_dm` 只接收策略并返回安装错误；`install_boot_dm_range` 拒绝不属于策略的窗口，并在第一次写表前预检范围端点、所有叶子 VA/PA、窗口和溢出；底层 `install_leaf` 中途失败后必须终止启动，不回滚、不继续使用部分映射；`BootPageSupport::from_parts` 仅 test-support，`validate()` 统一检查基础页/回退/首选为架构声明值且在页大小集合内；`PageSize` 二次幂已保证整除，删除整除检查和不可能失败的测试；`plan_range_units` 自检基础页、范围对齐和终点溢出，空区间是对齐起点上的合法空操作；`plan_kernel_mapping` 在首选可用时仍先检查回退支持；`DirectMapLayout` 容量按基础页对齐；§1 摘要编号统一为 1–12；`PageSize` 编译期比较改用 `.bytes()`，避免非 const `PartialEq`；`DirectMapLayout::validate` 在 `plan_dm_policy` 前检查容量/终点/上界；`Paging` 只暴露 `phys_addr_limit()` 和 `validate_virtual_range()`；PTE 格式上限留在架构内部（x86 52、ARM64 当前 48、RISC-V 56）。`phys_addr_limit()`：x86 先查最大扩展叶再读 `CPUID.80000008H`，缺叶则启动失败；ARM64 读当前 `TCR_EL1.IPS`（当前 IPS=5，即 48 位）；RISC-V 采用平台配置的 48 位保守上界而非内存图最高地址；`BootPageSupport` 保存有效上界，策略执行只检查语义上界；`supports_huge_page` 契约要求尺寸在 `HUGE_PAGE_SIZES` 内；Direct Map 和内核 VA 区间通过 `Paging::validate_virtual_range()` 验证；同构 SMP 的支持边界包含页大小、PA 上界和翻译配置；ARM64 52 位描述符需在 LPA2 条件下单独验证；异构组合明确不支持；`install_prevalidated_boot_dm_range` 避免全局预检重复；`CurrentPaging`/`CurrentDirectMap`/`CurrentBootDmInstaller` 全部投影自 `CurrentBootArch`；`DirectMapArch::KERNEL_DM_VA_END_LIMIT` 与 `IDENTITY_MAP_END` 编译期断言，确认 x86-64 4 级、ARM64 48 位、RISC-V Sv39；`KernelLayout` 对应连续、统一权限映射，未来段级权限/空洞需改为按段规划；`KERNEL_DIRECT_MAP_SIZE`/`kernel_pa_limit` 是窗口覆盖跨度，不是系统 PA 上界；`install_leaf` 的安全契约包含合法、对齐、可寻址的 root；所有安装错误路径不得恢复继续。**保留裁决**：`install_boot_dm_range`/`establish_boot_dm` 标 `unsafe`；`BootArch { Paging, DirectMap, DmInstaller }` 统一架构身份；`KERNEL_DIRECT_MAP_SIZE` 给 Kernel DM 有限 VA 窗口（x86/ARM `0x0000_7F7F_FFFF_F000`、RISC-V `0x0000_003F_BFFF_F000`，止于 `0xFFFF_FFFF_FFFF_F000`）；`BootPageSupport<A>` 只存 `preferred`/`fallback` 事实；`PageSize::try_as_usize`/`try_from_usize`；策略手写 `Copy`/`Clone`；`plan_dm_policy` 区分 `FallbackUnsupported` 与静态配置错误；CPUID 先查扩展叶 `0x80000001`。`validate_virtual_range` 负责内核/DM 的 VA 合法性；ARM64 当前格式按 48 位声明，52 位需 LPA2 实施与测试；同构 SMP 契约覆盖页大小、PA 上界和翻译配置；x86 缺 `CPUID.80000008H` 时启动失败。**源码取证**：x86/ARM/RISC-V 当前 `map_huge` 均未在入口检查能力/对齐/VA/PA 区间（`os/arch/src/{x86_64,arm64,riscv64}/paging.rs:fn map_huge`）；ARM `TCR_EL1` 在 `AArch64Paging::enable` 写入、RISC-V `satp.MODE` 在 `Riscv64Paging::enable` 写入，目标检测必须使用目标模式并在 enable 后断言；`boot_validate_and_prepare<P>` 与 `establish_boot_dm` 当前仍使用 `CurrentDirectMap`/`CurrentDmCoverage`，迁移到 `A` 必须替换。**迁移**：五阶段，先修启动选页，再 `PageSize`/能力 API，再事实/规划拆分与策略，再 `map_huge`/SMP/预检验证，最后机械重命名。**现算读数**：`os/arch/src/arch/paging_ext.rs:125`、`os/arch/src/arch/dm_coverage.rs:151`、`establish_dm_range`（`:209`）、x86-64 DM 实现（`os/arch/src/x86_64/paging.rs:1148`）。**DoD**：文档评审收敛；实施后 `rg` 旧入口归零；启动选页先能力后对齐；`plan_*` 纯函数；布局随策略传播；写表前预检；Kernel DM 有限窗口/`KERNEL_DM_VA_END_LIMIT`；`-smp4`。**现算命令**：`rg -n 'supports_1gb_page' os --include='*.rs'`；`rg -n 'plan_boot_pages\(|one_gib|one_gb' os --include='*.rs'`。 |
| D-60 | 恒等映射上界常量双定义（`IDENTITY_MAP_END` vs `BOOT_IDENTITY_MAP_END`） | ⏳ open（2026-10-20 同上，低优）：**现状读数**——`os/kernel/src/lib.rs:59` 私有 `const IDENTITY_MAP_END: u64 = 0x1_0000_0000`；`os/arch/src/lib.rs:231` 公开 `pub const BOOT_IDENTITY_MAP_END: u64 = 0x1_0000_0000`，`boot_dm_pa_limit()`（`:246`）用它；两处同值、跨 crate 手工同步。**风险**：改一处不改另一处会让“内核恒等映射范围”与“boot DM 分配上界”脱节，表现为 bump 页越过恒等窗口，或反向浪费可分配空间。**修法（推荐）**：kernel 侧改为 `use minix_arch::BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END;`（依赖方向 kernel → arch 已存在），删除本地字面量；boot-shim 继续引用 arch 的公开常量。**备选**：若认为 arch 不应拥有 kernel 的恒等上界，把常量下沉到 `minix-types`/`minix-boot` 后两处 re-export，但这会扩大改动面，需先证明 arch 的值不属于 boot 契约。**解除条件（DoD）**：全仓该字面量只剩一处定义；`rg -n '0x1_0000_0000|IDENTITY_MAP_END|BOOT_IDENTITY_MAP_END' os --include='*.rs'` 中无第二个 `= 0x1_0000_0000`；`cargo test -p minix-kernel --features mock` 保持通过。**现算命令**：`rg -n '0x1_0000_0000|IDENTITY_MAP_END|BOOT_IDENTITY_MAP_END' os --include='*.rs'` |
| D-66 | `KernelInfo.param_buf` 生产侧接入（UEFI Load Option / SBI 启动参数 → key=value，代码注释标记 P9-1，现传空切片） | ✅ 已实施（01TODO-T3PARAM，2026-10-30）：`minix_boot::normalize_param_buf` 统一规则；UEFI 读 `LoadOptions`，OpenSBI/直核读 DTB `/chosen/bootargs`；ARCH 由编译目标生成、BOARD 无来源即省略；aarch64 直核 `bytes=0x15 first=ARCH=aarch64`、UEFI 腿 `bytes=13`；minix-boot 65/0、boot-shim 53/0、kernel mock 845/0/8、三架构 check 0 error。原登记：⏳ open（2026-10-20 01 篇 param_buf 答疑登记，低优）：**现状读数**——生产侧全部传空切片：`os/boot-shim/src/uefi_helpers.rs:527`、`os/boot-shim/src/opensbi_helpers.rs:498`（两处注释均标 P9-1）；消费侧已完整对齐 C：`os/kernel/src/misc.rs` `getinfo_mon_params`（`:1330`）把切片零填充到 `MULTIBOOT_PARAM_BUF_SIZE=1024`（`:1344`）再整体拷贝（C do_getinfo.c:143-146 `length = sizeof(kinfo.param_buf)` 定长语义），空切片进出等价 C `param_buf[0]=='\0'`；长度护栏 `os/libs/minix-boot/src/kernel_info.rs:482-493`。C 生产侧参照 `minix3/minix/kernel/arch/i386/pre_init.c:144-150`（`mb_set_param` 写 `param_buf`，ARCH/BOARD 两个变量必定写入）。**语义定位**：字段通道已冻结、内容暂缺，接入只动生产侧（boot-shim/bootface），`KernelInfo` 结构与内核零改动（01 篇 §3.4 param_buf 条目已写明此定位）。**修法（落地时细化）**：① UEFI 腿——解析 Load Option 取参数正文、拆 key=value（正文段语义需对 UEFI 规范现场核实）；② SBI/直核腿——**来源需先裁决**：a1 现递 DTB、无命令行通道，QEMU `-kernel` 直核形态没有现成固件参数入口，可能诚实结论就是"该腿长期空切片"而非欠账；③ 结果过既有 1024 护栏。**连带同步**：`draft/25-misc-unported.md:311/:432/:465/:775/:808` 的「GET_MONPARAMS → EINVAL」读数已过时（现行为 = 零填充拷贝，仅 `kernel_info()` 为 None 的单测路径 EINVAL；draft 不作事实基线，落地时顺手刷新防误读）。**解除条件（DoD）**：UEFI 腿 Load Option 正文可进 `param_buf` 且 `GET_MONPARAMS` 返回与 C 同形（1024 定长零填充）；内核侧零改动；01 篇 §3.4 param_buf 条目"空切片"表述同步更新。**现算命令**：`rg -n 'param_buf: &' os --include='*.rs'`（当前生产侧全为 `&[]`）；`rg -n 'P9-1' os --include='*.rs'`（落地后应删除或改写） |
| D-67 | boot 信封合一 + 跨镜像布局全钉死——`BootPrepareResult` 并入 `BootHandoff`（`#[repr(C)]` 单信封），切片 `(ptr,len)` 与 `Option` 表示显式 repr(C) 化，使「同一 `xtask image` run 构建」从正确性前提降级为部署细节 | ✅ 已实施（01TODO-T1CUT，2026-10-30）：2c/3b 联合切根落地，旧一页 `BootHandoff` blob 与 `bootstrap_to_kernel_image` 删除，`BootHandoffV1` arena 成为唯一跨镜像协议；`BootPrepareResult` 保留为 x86/riscv 进程内返回类型；minix-boot 57/0、boot-shim 53/0 + 边界审计 5/0、kernel mock 845/0/8、run_all 39/0/0。原登记：⏳ open（2026-10-20 01 篇 §3.4 repr(C) 边界段答疑登记、同日三结构体（KernelInfo/BootPrepareResult/BootHandoff）答疑扩容，低优；**建议随 NK1/OQ-N6 跳转协议定案同批落地**——届时 BootHandoff 本来就要动）：**现状读数**——① 两张信封 90% 重叠：`os/libs/minix-boot/src/boot_shim.rs:20` `BootPrepareResult { kernel_info, root_page, bump_base, bump_end }`（无 `#[repr(C)]`，仅 `prepare_boot` 进程内返回值、从不跨二进制）vs `os/libs/minix-boot/src/handoff.rs:50` `BootHandoff { kernel_info, root_page, bump_base, bump_next, bump_end }`（`#[repr(C)]`，跨镜像 blob）；② 实质差异只两处——跨镜像路径接收方链接自己的 `minix_kernel` 副本、bump 游标不在共享 `.bss`，`bump_next` 必须进载荷且**必须在 blob 页分配之后**才能读（`os/kernel/src/lib.rs:302-306` 先 `boot_pt_alloc()` 分配 blob 页再取 `boot_alloc_next()`），这一时序使 `prepare_boot` 返回值天然缺该字段；跨二进制按裸指针解引用要求 `#[repr(C)]`；③ 缝合痕迹：`bootstrap_to_kernel_image`（`os/kernel/src/lib.rs:289`）收散参 `(&KernelInfo, root_page)` 而非 trait 返回值，只因 `BootPrepareResult` 缺 `bump_next` 且未钉布局；④ `kernel_info.rs:12-19` 承认 "the `&'static [T]` fields' internal (ptr, len) order is still toolchain-defined; the cross-image contract therefore relies on both images being produced by one `xtask image` run"，`handoff.rs` Layout stability 段同口径。**未钉死的类型自由度两处**：5 个切片字段（`memmap`/`reserved_regions`/`boot_modules`/`platform_sources`/`param_buf`）的内部 `(ptr, len)` 顺序；`free_upper_idx: Option<usize>`（`kernel_info.rs:70`）的判别式/niche 布局——比切片更隐蔽，"全部定住"必须连它一起处理。**性质判定**：不是疏忽——`BootHandoff` 是 aarch64 方案 A 跑通途中的正确救火（bump_next 时序 + repr(C) 两个实质需求），留下的债是没顺手收编 `BootPrepareResult`；同批构建当部署模型事实成立（shim 与 kernel.elf 是同一 `xtask image` 产物），但正确性靠**工序纪律** inferior 于靠**结构保证**。**修法（方向，落地时细化；⚡ `2026-10-21` 外部首轮裁决改判——主问题改题：先冻结跨镜像 wire 协议的单一权威定义（`BootSlice<T>` 显式 `(ptr,len)`、`free_upper_idx` 用 `u64::MAX` 哨兵、`MemoryRegion.len`→`u64`、协议头 magic/version/size、各受支持架构交叉编译布局构建门、wire 数据先验证再解码为语义对象），**信封合一降为次级**——`BootPrepareResult` 去留待 wire 冻结后按语义边界裁决、不以减少结构体数量为独立目标；评审倾向「保留进程内 `KernelInfo` + 专门 wire 类型（`BootHandoffV1`）镜像字段、解码边界收敛在 minix-boot」，详见 `01-code-design.md` T1 v2–v4（v3 二轮增补：三层解码模型 `BootHandoffV1`→`KernelInfoView`→内核自有 `KernelInfo`、wire 递归闭合 `Wire*` 元素类型、UEFI 池上界机制；v4 三轮增补：非泛型 `WireSlice{addr,byte_len}` + 溢出检查区间公式、地址语义逐字段声明 `WirePhysAddr`/`WireVirtAddr`、可读区间语义接口 `BootReadPhase` 替代共享解码器硬编码恒等映射（机制归腿；`minix-boot` 只依赖 `minix-types` 的依赖方向佐证）、wire 序列化/UEFI 快照/EBS/回收闭环；v5 四轮增补（2026-10-28，补证 `01-forensics-round5.md`）：最终内存图改方案甲（EBS 返回 `MemoryMapOwned` 唯一权威 + 快照 A 取消 + 四验收条件）、`translate` 草图否决改三路径读取 API、模块 blob MaxAddress 上界、x86 red-zone 撤销（两目标规格均 `disable-redzone: true`，无旗标待注入）、T3「宁可占位」废除为真实值或显式省略、T4 `cut_memmap` 自动页化核实完毕；v6 五轮增补（2026-10-28，取证 `01-forensics-round6.md`，最后设计闭环）：序列化时点裁四步（EBS 前分配 / EBS 后切根前分类即序列化 / 切根 / 切根后只补运行期信封）、`KernelInfoView`/`BootReadPhase` 退役改受控复制五步序列（`unsafe` 边界仅两处）、采单一 handoff arena（一至三页一次 MaxAddress 分配、`WireSlice.addr` 维持物理地址、全部载荷 ⊆ arena 验证）、x86 栈二段式 + `rbx` 传 blob 指针、`load_elf_into` 前提归边界；**T1/T2 概念层全部定向，设计冻结线已到**；v7 六轮增补（2026-10-28，取证 `01-forensics-round7.md`，实施前定稿）：arena 补语义工作分区（方案 A：分类循环双写 `MemoryRegion` 与 `WireMemoryRegion`，分开存储无共享布局；shim 侧准备函数对内存图内容零消费已经取证——语义分区供类型正确性而非内容消费）、arena 兼任信封宿主（`BootHandoffV1` 居 arena 首部、切根后原地补运行期字段、`x0 = arena 基址`、独立 bump blob 页移除、容量精确 4 页编译期常量）、方案乙（EBS 配对循环内分类）评估后不采（锁定 uefi 0.33.0 无公开 API 路径、「EBS 后固件页表有效」前提两案共有；维持方案甲，升乙为三项条件触发）；状态收紧表 §10；v8 七轮增补（2026-10-28，取证 `01-forensics-round8.md`，协议定稿）：`WireSlice{addr,byte_len}` **退役**（推翻 v5 已裁条目——「载荷 ⊆ arena」无法区分指向正确分区，固定分区布局下自报地址是冗余自由度；描述符改仅 `byte_len: u64` + 信封 `arena_len` 字段，验证收敛为容量 + 编码守恒，溢出/对齐/⊆ 升格为布局构造保证）、内存图权威**改裁方案乙**（round-7 否定性取证有漏：`uefi::table::system_table_raw()` 在 0.33.0 公开存在，探针证实配对式 GetMemoryMap/EBS 保留 `uefi::entry` 零新依赖类型可达、boot-shim 未开 logger feature 故 crate 状态补丁为零；分类移入配对循环 = Boot Services 活着的规范明文窗口 + MapKey 显式配对，成本 ≈ 50–80 行规范 ABI 循环；甲降级为回退路径）；**T1/T2 设计冻结，转入实施**（实施首批：分区常量与信封字段钉死随布局门禁、乙路径真机验证）；round-9 八轮增补（2026-10-28，`01-forensics-round9.md`）：乙三条硬性实施契约（EBS 成功后三零纪律——零分配/零释放/零 BS 调用，crate 防线依赖固件置 NULL、`prepare_boot` 链 Drop 面盘点为零；重试缓冲三段处置——EBS 前可扩容/失败后只复用/复用不足 fail-closed、总尝试预算 2；描述符遍历——desc_size 步长 + version=1/desc_size≥size_of/边界/整除四检查）+ 更正 round-5 台账 #7（快照 A 图缓冲实际被 FreePool）；v9 九轮终审（2026-10-28）：**设计冻结**（架构与协议不再开设计轮次），补四条终版实施约束——重试只对 `EFI_INVALID_PARAMETER` 触发（其他状态立即失败、超限走不依赖 BS 的终止路径且不沿用旧分类结果）、描述符对齐前置于引用建立（含九轮新增的「`descriptor_size` 为对齐整数倍」）、区间 `checked_mul/add` + 页数非零 + PA 范围验证、失败路径零析构零释放；实施第一批 = arena 布局常量 + raw EBS 配对循环 + 九面失败测试族）与其 §1.3 已裁表/§1.4 待裁清单；实施次序 T4→T1→T5→T3→T2）**：a. **信封合一**——`BootShim::prepare_boot` 改返回 `BootHandoff`（返回时 `bump_next` = 当时光标快照，进程内路径读它无害），提供 `with_current_cursor()` 一类 mutator 供跨镜像路径在 blob 页分配后补刀；删除 `BootPrepareResult`（`boot_shim.rs:20`、`boot-shim/src/lib.rs:30` re-export、uefi/opensbi helpers 构造点）；`bootstrap_to_kernel_image` 改收 trait 返回值，缝合痕迹消失；x86_64 内联与 riscv64 直核解构出 `(&kernel_info, root_page)` 传 `arch_boot`，**内核侧入口 ABI 零改动**；b. 切片字段换 `#[repr(C)]` 显式 `(ptr, len)` newtype，构造/`as_slice()` 转换收敛在 boundary；c. `Option<usize>` 换显式表示（`u64`+哨兵或 repr(C) 小 enum），同步 §3.9 语义叙述；d. 编译期断言补齐 size/align/关键字段 offset（先例：`handoff.rs` 末 `size_of::<BootHandoff>() <= 4096`）；e. 注释改写为 "layout fully pinned; same-build remains a deployment detail"。**影响面**：定义侧 `minix-boot`；构造侧 boot-shim（uefi/opensbi helpers）、kernel-image（bootface/bootface_a64）、内核与 qemu-tests 构造点约 50 处（`rg -c 'param_buf: &' os --include='*.rs'` 现算）；消费侧内核读切片处各加一次转换；qemu 测试内核手工构造 `BootPrepareResult` 的十余处同步换名。机械但广，故与 NK1/OQ-N6 同批最省。**明确不并入**：`VmBootHandoff`（`os/libs/minix-types/src/types/boot.rs:229`）是内核→VM 的运行期上层交接、带 magic/version，属另一层契约，不在本条射程。**解除条件（DoD）**：a–e 落地；`rg -n 'BootPrepareResult' os --include='*.rs'` 归零；`rg -n 'toolchain-defined|versioned ABI' os/libs/minix-boot/src/` 归零或按 e 改写；01 篇 §3.4 "前提是两个镜像由同一次构建产出"让步句与两信封叙述同步改写；`cargo test -p minix-kernel --features mock` 与 qemu 引导测试全绿。**现算命令**：`rg -n 'BootPrepareResult' os --include='*.rs' | wc -l`（当前约 30 处）；`rg -n 'toolchain-defined|versioned ABI|xtask image' os/libs/minix-boot/src/`；`rg -n 'Option<usize>' os/libs/minix-boot/src/kernel_info.rs` |

### 7.2 WONTFIX 设计排除清单（有 rationale，非缺口，防重查）

| # | 排除内容 | 理由 |
|---|---------|------|
| W-1 | NMI watchdog 全族（watchdog.c/arch_watchdog.c） | 架构覆盖不全 + 默认关闭 + 依赖 PMU；sprofile 替代（doc 26 §3.1） |
| W-2 | usermapped_data 全部 | 64 位无 `.usermapped` 段机制（doc 28 §4.2） |
| W-3 | IPC 消息跟踪/统计 hook 族（debug.c 12 函数 + 3 宏） | panic 路径风险；log crate + 外部工具替代（doc 29 §3.2-3.4） |
| W-4 | `nmi_sprofile_handler` | 无 NMI 子系统（doc 30 §4.4，同 W-1） |
| W-5 | SPROF `PROF_NMI` 路径 | 同上 |
| W-6 | `ClearMapCache` no-op | 64 位 Direct Map 无 cache table，匹配 C（doc 09 §4.5） |
| W-7 | kmess 缓冲体系（双缓冲/END_OF_KMESS/send_diag_sig/SIGKMESS） | EarlyConsole 直出 + log 后端替代（doc 27 §6.2，ARCH 演进） |
| W-8 | ~~swap_memreq no-op~~ → **已被 U-2 推翻并真实现**（2026-09-08，proc_table.rs `vm_swap_requestor`） | 见 §12 Phase 5 |

### 7.3 测试债（T 系）

**✅ 已解决**：T-1（10/10 行为矩阵）、T-6（10 测试）、T-7（5/5 队列）、T-9（IRQCTL 3 + 撤回项论证）、
T-11（runqueues_ok boot_integration）、T-12（5/5，附带修正 dispatch_clear 幂等对齐 C do_clear.c:38）、
T-5（QEMU CI：`.github/workflows/qemu-tests.yml`，23 测试 22 PASS，唯一 FAIL=S-3d WIP，CI 置
`QEMU_TESTS_SKIP_AP_ALIVE=1`）（2026-09-08~09）。

**⬜ 仍 open**：

| # | 项 | 依赖 |
|---|-----|------|
| T-2 | ~~7 个 SMP 测试~~ ✅ S-12（smp.rs hosted 覆盖 ≥7：masks/boot_lock/ipi handlers/wait/timeout/finish/entered） | S-12 ✅ |
| T-3 | ~~init/load 顺序约束测试~~ ✅ S-12（gate↔stub 覆盖互查 + 未注册分发 assert 机制 + BKL 已持 debug_assert；extern "C" 边界 panic-abort 使专门的违序 hosted 测试不可行——机制由 assert 本体承载） | S-12 ✅ |
| T-4 | ~~init_ap 路径验证~~ ✅ S-4/S-12（test-smp-aps 硬件 L2/L4 + hosted 布局测试族） | S-12 ✅ |
| T-8 | E2E grant 剩余 QEMU 半 5 个（间接链 ELOOP/magic 重定向/跨进程拷贝/vm_lookup/vm_memset——宿主 PTE walk 必然 Suspended 无法到达断言） | T-5 QEMU CI 承接 |
| T-10 | riscv64 真实启动链（U-Boot fatload→bootelf）集成 | 允许开放（`PD-15` 已裁放弃；本机 U-Boot 2025.10 `bootelf` 无 `-d`/argv，固定槽需独立 LMA 产物——实测见 D-73 §2.8） |

### 7.4 文档任务 / 改进方向（I 系）

**✅ 已解决/闭合**：I-3（riscv64 高半核，QEMU 全 PASS 证实）、I-4（测试表去重）、I-8（=D1）、
I-11（BIOS legacy 范围声明非缺口）、I-12（RAII 拒绝维持，B1 承接）、I-14（B 层完成：09/16 前置
声明对齐实时序，06/08/10 核销，C 层重编号不做——风险>收益，2026-09-09）、SYS_PADCONF
（奇偶闭合：C 仅 `__arm__` map，BadCall→EBADREQUEST 即正确应答）。

**⏸/⬜ 仍 open**：

| # | 项 | 状态与解除条件 |
|---|-----|------|
| I-22 | 抽 `MemoryMapSource` trait 收口三链取内存图（现在 riscv64 生产链 opensbi 腿硬编码、UEFI 与 bootface 各写一份）——把「(free, reserved) 划分」上升为跨固件的 OS 语义层 | ⚠ **2026-11-03 被 I-28 正式 supersede**（终审裁决：trait 无多态消费者，删除方向冻结；正式 supersede 记录随 I-29 原型批落码，落地前本条的代码形态维持现状）。落地历史：✅ 已落地（2026-10-29，01TODO-B2 回勾：代码丁案 A–F 在位、文档连带 ①–⑦ 逐项核过、⑤ bootface 头注已修；遗留仅 D-68 随 T4）。原登记读数保留：⏳ open（2026-10-18 用户问询触发登记、同日二轮架构裁决刷新；用户明示「不计成本、专期完成」）：**现状读数（现算）**——① `os/boot-shim/src/opensbi_helpers.rs:61` `DRAM_BASE = 0x8000_0000`、`:64` `DEFAULT_RAM_SIZE = 128MB`、`:421` `build_memmap()` 返回**单区硬编码**（模块顶注释 `:188` 原句承认 "Memory map → hardcoded"），配套单区断言测试 `:654`、`:668-670`；② `os/boot-shim/src/uefi_helpers.rs:295` `build_memmaps()` 走 UEFI `GetMemoryMap` 分流 conventional/non-conventional；③ `os/kernel-image/src/bootface.rs:290` `fill_memmap()` 解 DTB `/memory`/`/memory@…`（`MEMMAP_MAX=8`）+ `clip_firmware` 扣 OpenSBI 占用。三处平行、语义相同、**没抽公共 trait**。**违反公理**（`prompt/review-rules/review-core-semantics.md` §1.6 四公理）——违反公理 1（QEMU 硬件形状泄露进 OS 语义层产物）；违反公理 3 的正面用法（三条链对 OS 暴露的语义完全相同：给一份 (free, reserved) 划分，正**该**共享 trait，现状是没抽）。AGENTS.md 核心约束「硬件通过 trait 抽象」同款违反。**主方案（丁案，唯一目标形态）**：A. `os/libs/minix-boot/` 新增 `pub trait MemoryMapSource { fn collect(&self) -> (&'static [MemoryRegion], &'static [MemoryRegion]); }`，语义：conventional（PMM 可直分）+ reserved（运行期经低址/Direct Map 会访问但不得重分配）；B. 三个 impl——`UefiMemoryMap`（现 `uefi_helpers::build_memmaps` 迁入）/ `DtbMemoryMap<'a>`（现 `bootface::fill_memmap` + `clip_firmware` + 未来 `/reserved-memory` 节点解析迁入，从 `dtb_pa` 解）/ `MultibootMemoryMap` 空位留给 BIOS 腿出现时补（不为漂亮提前改契约）；C. 三链消费点接线——`UefiBootShim::prepare_boot` 换 `UefiMemoryMap::collect()`（行为等价、语义显式化）；`OpenSbiBootShim::prepare_boot` 换 `DtbMemoryMap::collect(dtb_ptr().expect("OpenSBI 递 a1=DTB"))`（`install_dtb_ptr(a1)` 已在位，当前只作转发供内核平台发现，本方案把它同时用作 memmap 源）；`bootface::bootface` 调同一 `DtbMemoryMap::collect(dtb_pa)`；D. 删除 opensbi 侧三件常量 `DRAM_BASE`/`DEFAULT_RAM_SIZE`/`build_memmap()` 与单区断言测试（`:654`/`:668-670`）；bootface 里的 `DRAM_BASE`（`:111`）**保留**——它是 DTB/表界闸下界防漂移，不是 memmap 源；E. 新增 DTB 解析测试——mock FDT 覆盖 `/memory` 单段/多段/unit-address 变体 `/memory@80000000`/缺节点走诚实停机臂；F. **不合并引导形态层**——`BootShim`（独立可执行体由固件加载）与 `bootface`（kernel-image 自举、`satp=0` 恒等视图、无固件服务）对应不同机器状态，按公理 3 不造伪统一；本方案只**下沉 memmap 源这一层**到 trait。**连带耦合（改代码必同批动的文档六处 + 测试一组）**——① 01 篇 L1265表格「内存布局」列：「boot-shim 用硬编码的 QEMU virt DRAM 布局」→「UEFI/OpenSBI 分别走 `UefiMemoryMap` / `DtbMemoryMap`，三链共享 `MemoryMapSource` trait」；② L1945表格「内存与参数准备」列：「生产链：硬编码的 QEMU virt DRAM 布局；直核：DTB 的内存节点」→「生产链与直核共用 `DtbMemoryMap`；UEFI 链走 `UefiMemoryMap`」；③ L2288字段来源表「`memmap` 谁填」行：「UEFI：步骤 6 快照的 conventional 列表；OpenSBI：硬编码单条 DRAM」→「UEFI：`UefiMemoryMap::collect()`；OpenSBI：`DtbMemoryMap::collect(dtb_ptr())`」；④ `os/boot-shim/src/opensbi_helpers.rs` 模块顶注释 L186-189（"Memory map → hardcoded"）→「Memory map → `DtbMemoryMap::collect(dtb_ptr())`」；⑤ `os/kernel-image/src/bootface.rs:9-11` 头注「不再抄 `opensbi_helpers::build_memmap` 的单区硬编码」在硬编码删除后失去指涉对象 →「与 opensbi 腿共享同一 `DtbMemoryMap`（trait 在 `minix-boot`）」；⑥ 01 篇 §3.7 / §4.8 两链关系处酌情补一句「两链 memmap 层通过同一 `MemoryMapSource` 合流，引导形态层不合并」（改动量现场判）；⑦ 04 篇平台发现契约：本方案让 boot-shim 侧也解 DTB 的 `/memory` 子集，与「内核平台发现层解 DTB 其余节点」的分工要重写清楚（同批对账）。**门依赖**：无（CI 与 `tools/` 不校验 opensbi 侧常量；`check-layout.sh` L0/A1 只钉 bootface `DRAM_BASE` 下界闸，与本 trait 抽取正交）。**行业对照**（作设计合理性旁证，不作裁决依据）：Linux `memblock_add`/`memblock_reserve` 同名抽象，上游 source 分 `devicetree.c`（`/memory`+`/reserved-memory`）/ `efi.c`（`GetMemoryMap`）/ `acpi.c`（MADT memory affinity）；Redox `BootInfo` memory region 列表由 multiboot2 与 UEFI loader 各自报，内核消费不 per-board 硬编码。本方案与两家同层。**与 coordination 台账关系**：若存在「riscv64 两链合并」议题，本条与之**脱钩**——本方案不合并两链，只统一下沉 memmap 源；两链合并属引导形态层的另一项裁决。**解除条件（DoD）**：A–F 全部落地 + 文档①–⑦ 同步 + DTB 解析测试新建 + 单区断言测试删除；只改代码不改文档 → Doc Code Sync 七项 FAIL；trait 抽出但三链未接线 → 视为半成品不计入收敛。**登记纪律说明**：初版候选甲/乙/丙已被本轮架构裁决替换为丁案（用户明示「追求最佳设计」、成本不计、专期完成）；甲/乙/丙不再列为候选，旧版本存 git 历史可回查。**现算命令（复核用）**：`grep -n 'DRAM_BASE\|DEFAULT_RAM_SIZE\|fn build_memmap' os/boot-shim/src/opensbi_helpers.rs`（当前常量三处 + 函数一处，方案落地后 opensbi 侧应为**零命中**，bootface 侧保留一处）；`grep -rn 'trait MemoryMapSource\|fn collect' os/libs/minix-boot/`（当前零命中，方案落地后至少一处）；`grep -cE '硬编码的 QEMU virt DRAM 布局\|单条 DRAM' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（当前读数 **3**，方案落地后按新口径应为 **0**） |
| I-28 | `MemoryMapSource` trait 无多态消费者——无泛型约束、无 `dyn`，三个 impl 中两个无活消费者；正式 **supersede I-22** | ✅ **closed（本批落码，基线 acd1302d3，落点 本笔合入后的 rewrite 新尖子（基线 `acd1302d3`，claim 分支 `claim/I-28-I-29-qoder`））**。trait 与 BIOS 空位实现、UEFI 包装类型一并删除；取表方法降为设备树收集器的固有方法；「唯一出口」类措辞改写为「结果形状承载语义 + 为何不值得接口」；I-22 的六项取代记录已落本文件上一条。**核实三件套**：① `grep -rn "MemoryMapSource" os --include="*.rs" | wc -l` → 0；② `cargo test -p boot-shim -p minix-boot` = 64 通过 / 0 失败 / 0 忽略（另 5 通过；minix-boot 65 通过 / 0 失败）；③ 三架构真机门 `cd os/qemu-tests && bash run_all.sh` → 37 passed / 2 failed / 0 skipped / 0 build failures（本树现跑；两条红 `test-smp-aps`/`test-smp-ipi` 在同一台机器上用**未改动的 rewrite 尖子 `2b72a1e62` 重新构建同一载体**复现同一 `### PANIC … arch/src/x86_64/protection.rs:0x1c3`，判为既有、非本批引入）；④ 权威 mock 门 docker run --rm -m 2g -v <本树>:/work -w /work/os minix-ci:1.94 cargo test -j 1 -p minix-kernel --features mock → 865 通过 / 0 失败 / 12 忽略（本批；记录基线 857/0/8，计数差异未归因——本批未新增 minix-kernel 用例，失败数 0 为判据）。**重新引入判据**见 I-22 记录。以下裁决文字保留不改。裁决：删除 trait 与空占位（`MultibootMemoryMap`、`UefiMemoryMap`——后者删除范围随 I-29 对 A1 与 `build_memmaps` 的处理），`collect` 降为 `DtbMemoryMap` 固有方法，保留「可用表/保留表」语义文档（搬进 `KernelInfo` 字段文档或共享结果类型），模块头注不再宣称「唯一出口」。**实施顺序**：随 I-29 原型批执行（I-22 的正式 supersede 记录在此批落地）；重新引入判据 = 第二种真实来源出现且有消费者依赖共同语义。**DoD**：`grep -rn "MemoryMapSource" os --include="*.rs"` 与裁决方向一致（删除则零命中）；三架构 `cargo build` + `run_all` 39/0/0；代码侧无「统一出口」类失真注释。**详情**：`01-todo.md` §3 I-28 条 + 设计笔记 v12 §1/§5.1（`rewrite-notes/01-stage-kernel/01-design-note-memmap-consolidation.md`） |
>
> 上表 `I-22` 条的取代记录（六项）：（本批，基线 acd1302d3，本批落点 本笔合入后的 rewrite 新尖子（基线 `acd1302d3`，claim 分支 `claim/I-28-I-29-qoder`））**。正式记录六项：**① 当初为何引入**——三条引导链各自取得内存图，其中 opensbi 引导腿把宿主单块 DRAM 区间直接交给内核，需要一层跨固件的共同出口来阻止硬件形状进入 OS 语义层；**② 当时假定的消费者与实现**——生产 UEFI 腿与两条裸机引导面各自消费同一个出口，实现三份（UEFI 取固件内存图、设备树取 `/memory`、BIOS 腿留空位待补）；**③ 实际具有的能力**——所谓出口只返回两张静态区间表；生产 UEFI 腿在引导服务仍存活时把分类结果写进交接区，拿不到「两张静态表」这个形状，因此从来没有接进该出口；BIOS 空位实现只能返回两张空表；**④ 未兑现的假设**——没有任何调用点以 trait 对象或泛型约束消费它，「来源可替换」这一能力从未被使用，而空表把「没有答案」伪装成「答案是空」，让缺陷推迟到内核深处才暴露；**⑤ 本次为何撤销**——共同语义其实在结果的两张表里，不在接口形状里；保留一个无消费者的接口会让读者误以为接入新来源就是再实现一个 trait，并让两处无生产调用点的实现长期冒充第三种来源；**⑥ 语义文档去向**——两张表的定义、「可用」与「保留」的边界、以及为何不值得一个接口，写入 `os/libs/minix-boot/src/memmap_source.rs` 模块头；`KernelInfo` 的两个字段仍按同一形状被消费。**重新引入判据**：出现第二种真实来源，且有消费者依赖与来源无关的语义时，才重新设计接口。**核实命令与读数**：`grep -rn "MemoryMapSource" os --include="*.rs" | wc -l` → 0（本批，基线 acd1302d3）。本条当年落地的硬编码清除与设备树解析迁移全部保留、未被回退；被取代的只是「以 trait 收口」这一层手段。
| I-29 | UEFI 内存图分类双份实现（生产 `classify_map` vs 测试 `build_memmaps`）；测试图不反映快照后分配；类型政策缺位（判据用地址不用类型）；`Attribute` 未读；三架构缓存属性机制各异 | 🟡 **partially fixed（本批：已冻结的内核全部落码；暂缓项各自仍等验收门，非降级充数。基线 acd1302d3，落点 本笔合入后的 rewrite 新尖子（基线 `acd1302d3`，claim 分支 `claim/I-28-I-29-qoder`））**。**已坐实**：唯一生产解析器与唯一分类规则（新增 `os/boot-shim/src/ebs_map.rs` 的解析 + 裁决函数，原 `uefi_helpers::build_memmaps` 里的第二份分流规则删除并改接同一政策，两条消费腿——写交接区的配对循环与构造内核镜像表的辅助函数——走同一实现）；`ram_top` 地址启发式从分类器删除（不留旧行为特征测试，按裁决）；描述符引用带原始访问属性，起点对齐与几何边界由解析器负责；分类零分配、输出用调用方预分配存储；预检可失败、写入以计划凭证表达不可失败；第三份输出（side list）落为 shim 内部诊断，容量不足即在预检拒绝而非截断；五步裁决顺序含四个具名含糊组合；`bootstrap_alloc_limit` 改为界内最后一字节；§2.5.9 两处失真注释（池释放不回固件、快照缓冲与可用表的因果）已修正。**仍开放**：属性政策矩阵逐行批准（等真机样本与三架构证据）；三架构普通写回映射合同；退出后最终内存图的端到端验收；不可映射节点的 L3/L2 禁止集合编译与 `FirmwareRuntimeMmioRanges` / `DeviceWindowRanges` 的来源接线（简报 §12.4 两栏）；**第二十九轮已收尾（文档提交 `a15fcccac`）**：主文档改为 as-built 全量重写（1669→137 行），旧文冻结为 `04-design-backlog-round28.md`；第二十九轮九项意见三分结果全部为「成立、未设计」，登记为 O-01～O-13，不再写成正文公式；`effective_pa_limit` 改为只表示可表示上界；核出代码今天只有一棵项目根、DM 为追加。**同日按评审人反馈把主文档改写为自包含简报（364 行）**：补背景、失败形态、候选方案与判据；137 行状态备忘仅作过渡版，已被替换。**第三十轮定点语义修订（主文档 384 行）**：O-01 改生命周期、O-02 拆授权/禁令、O-05/O-06/O-08/O-09/O-10 采用候选 A、O-11 改首次访问授权、O-12 拆五子项；启动入口改四条并入 §1.1/§2.5。**第三十一轮（代码 `2209fb3e9` + 文档 `77b0e3c7b`）**：`/reserved-memory` 增空 `ranges` 校验与 `no-map`/`reusable` 互斥；修正 aarch64 UEFI 磁盘 DTB 定位（只作自检闸，不是内存政策输入）；O-01/O-02a/O-02b/O-03/O-05/O-06/O-07/O-10/O-11/O-12 按裁决修订。**第三十二轮（代码 `431bf8461` + 文档 `46cbef986`）**：标记属性空值校验、cells 与根一致、UEFI 磁盘 DTB 自检降为诊断（并修复 aarch64-uefi 编译回归）；文档分出 `RamSourceDomain`/`RamBacking`、补 UEFI 保留叠加与 O-11 交叉验证（含 MRB）、登记采集器重构 O-12g。**第三十三轮（代码 `413ff9265` + 文档）**：MRB 条目重叠拒绝与扫描上限；O-11 扩到四条 DTB 入口；O-06 加 `RamSourceDomain` 列与属性处置；O-12c 设备窗口来源矩阵；O-12g 原始范围保留；新增 O-12h；§6 证据基线。**第三十四轮（代码 `f0c518179` + 文档）**：`/memory` 的 status/hotpluggable 成为准入条件；O-06 裁定 RESERVED 域外、UNUSABLE 在域内但排除；O-12 改列表并拆设备窗口三概念；O-12h 两级校验；§7 五阻塞。**第三十五轮（代码 `53755840b` + 文档）**：零长度 MRB 拒绝、热插拔属性位负控；§3 来源域叙述与 O-06 对齐；O-12i 量化合同；O-12h 重叠/顶端；O-12c 实际静态窗口清单；O-12j 登记 MRB 对齐缺口。**第三十六轮（代码 `f98d4edff` + 文档）**：PLIC 窗口扩到 0x24_0000 覆盖 context；热插拔位修正为 0x0010_0000；O-12i 页域公式；O-03 冲突组合表；O-12c 版本绑定。**第三十七轮（代码 `9b2b09f1c` + 文档）**：PLIC 授权按 2 MiB 包络 0x40_0000；O-12i 补四类排除原因；O-03 拆 no-map 三项与 MRB 部分页；O-12j 写明不签证据；§6 基线改四条命令。**第三十八轮（代码 `a52cfee54` + 文档）**：DTB 指针 8 字节对齐与 totalsize 上限；O-12c 四层范围与 PLIC 配置绑定；O-12i 页集合类型；O-12j 写入 fdt 核对结果；§6 基线 86/67；§7 六组。**第三十九轮（代码 `1a9e2a14f` + 文档）**：FDT 容器校验层落地并接入五处；O-12j 关闭；O-12c 五列授权范围与 UART 256 B；O-06 四类型拆行与 Reserved 身份；§4 权限合同改正；§6 96/67。**第四十轮（代码 `3333d428c` + 文档）**：FDT 版本与块顺序入校验；O-12j 拆 a/b/c；O-11 地址域与可达性改名；O-12c Expected/Observed 与 APIC/PLIC profile；O-12i 类型先裁决再取整页。**第四十一轮（代码 `23feb9bc0` + 文档）**：FDT 版本子集 17/16、结构块对齐/END/完整顺序；UART 改平台窗口授权；PLIC context 推导链；PlatformMemoryProfile；五个调用点表；§6 102/67。**第四十二轮（代码 `51b784cb3` + 文档）**：MRB 上限与 strings 非对齐测试；O-12j-b 预扫在 `-C jump-tables=no`（xtask 注入 + config）后落地，27 条测试；新增 EarlyExecutionClosure 合同；O-12c 资源清单/政策字段；O-06 属性三分与 CONVENTIONAL 条件；§6 104/67。**第四十三轮（代码 `0464a1e5c` + 文档）**：结构块预扫在 `-C jump-tables=no` 下落地；EarlyExecutionClosure；§6 113/67。**第四十四轮（代码 `d615d0190` + 文档）**：预扫拒绝「子节点后属性」并把深度收到 63（对齐 fdt 0.1.5 的消费能力，撤回上一轮的固件形态判断）；O-11 补自有副本第三段；O-12g 原始记录层列为前置；§6 114/67。**第四十五轮（代码 `28e277d5b` + 文档）**：采集边界交出五类 `SourceRecord`（`collect_with_sources`，容量 40），no-map/MRB/RAM 外声明可追溯；O-11 拆provenance/reachability 并给复制算法；O-12h 改判平台准入；O-06 兼容矩阵与第一版处置列；RamBacking 更名 OrdinaryRamEligibility；§6 118/67。**第四十六轮（代码 `9a4d0eb51` + 文档）**：`SourceRecord` 去页范围、补 `id`/`origin`/`reusable`，MRB 更名 `MrbEntry`；容量改由输入上限论证并加 RAM 外 no-map 负控；`collect`/`collect_with_sources` 一致性测试；§2.5 三层登记启动路径未接入；aarch64 QEMU 被 `2d292a06b`（sendrec）外部红阻断。§6 120/67。**第四十七轮（代码 `b49afa97d` + 文档）**：`SourceKind::class()` 分输入声明与派生 `FirmwarePrefix`；origin 补节点/reg 序号与 `memory_id` 派生链；新增共用声明上限、40 条恰好可达、派生链三条测试；`OrdinaryRamEligibility` 收窄为映射资格；`uefi-raw` 行展开常量；§6 增证据分层表。§6 123/67。**第四十八轮（代码 `09e56a729` + 文档）**：`raw_*` 改中性 `range_*`；前缀留 `cut` 派生参数；精确重复声明投影去重、来源各留一条（部分重叠拒绝、同范围异语义冲突）；补反向完整性测试；§6 127/67。**第四十九轮（代码 `b3c93b83f` + 文档）**：MRB 精确重复改回拒绝（DTSpec 5.3.2），`/reserved-memory` 重复去重保留为项目策略；不变量 1 补四层投影合同与量化顺序；O-11 四项条件（含全范围可达与源目标不相交）；DeviceAccessUse 补 VA；RESERVED/UNUSABLE 语义分开；WP 归物理保护能力。§6 127/67。**第五十轮（代码 `59aa94cf3` + 文档）**：保留语义三分（普通/reusable/no-map）与跨语义冲突；`/memory` 部分重叠拒绝、精确重复去重；UEFI↔DTB 改覆盖合同（未实现）；`EfiReservedMemoryType` 规范/项目两层；O-11 MRB 措辞与页粒度临时保留；§6 131/67。**第五十一轮（代码 `c5f1789b8` + 文档）**：O-11 读取措辞修正；UEFI↔DTB 页级类型裁决（项目强化路径，must/should 分开）；启动读取用途矩阵（no-map/EfiReservedMemoryType 不得默认放行）；CONVENTIONAL 条件资格；/memory 重叠改述为项目策略；O-12j-c 状态表拆分；新增单节点多 reg 重复测试。§6 132/67。 aarch64 仍阻断（`servers/sched` 的 `SefIpcAdapter` 缺 `sendrec`）。**第五十二轮（纯文档）**：普通静态+MRB 同页改判拒绝（must vs 项目强化冲突）；PMM 公式补 `CacheAttributeEligiblePages`；`no-map`/`EfiReservedMemoryType` 内的 DTB 读取第一版拒绝；reusable owner 措辞修正；`collect()` 的 MRB 归因缺失入「不保证什么」。**第五十三轮（代码 `694b74386` + 文档）**：`0x0001_0000` 歧义位改 fail-closed（`AmbiguousAttribute`）；删除「Reserved 覆盖 BootServicesData」表述，同页冲突一律拒绝；新增 O-12k MapKey 快照合同；`EfiReservedMemoryType`+`no-map` 第一版绝对禁止；合法/拒绝诊断记录分开；aarch64 UEFI 无配置 DTB 的启动规则；riscv QEMU 覆盖范围标注。§6 132/68。**第五十四轮（纯文档）**：O-12k 重试执行环境与五条负控；O-11 补 UEFI 配置表存放类型与页粒度类型冲突；x2APIC 独立 MSR 访问域；PA/VA 分开与端到端可达；QEMU 读数标注历史范围。**第五十五轮（代码 `cdfa1637c` + 文档）**：属性冲突域 `0x1_F000` 整体 fail-closed；EFI DTB 专属类型收紧为 `EfiACPIReclaimMemory` 并进入页级主张（与 MRB 同页走例外）；x2APIC 分层（MSR 不进共享层）；O-11 地址三概念；`ReservedReusable+MRB` 冲突与页级生命周期；§6 132/69。**第五十六轮（代码 `9ce248cf7` + 文档）**：高位属性 RO/SP/CPU_CRYPTO 钉住为仓库分支选择；**aarch64 UEFI 腿首次 PASS**（E0046 三处修复后）；EFI DTB 类型验证拆两时点；MRB×EFI DTB 页集合差分与最终表拆分；NV 分支不可达写明；O-12k 静态准备前移；x2APIC 入 O-10 审计。§6 132/70。**第五十七轮（代码 `5566105c0` + 文档）**：高位属性改 profile 门控（`ProfileAttribute`）；O-11×O-12k 双快照七步时序；属性证据史修正；ACPI 不启用时 ACPIReclaim 视为 Permanent；SourceRef 定型登记；术语统一 `OrdinaryRamMappingEligibility`；`reg+size` 行为写明；aarch64 再次 PASS（门控后）。§6 132/70。**第五十八轮（纯文档）**：属性证据史更正（2.10 原始版含高位）；NV 解锁两步条件；§2.2 70 条与独立属性可达性修正；O-12k 缓冲失败三分；UEFI 快照级来源身份；O-11 端点与分配结果相交；术语统一。**第五十九轮（纯文档）**：O-06 可达性旧句修正；属性位三列完整矩阵；PMM 补 `ArchMemoryAccessEligiblePages`并把 WB 改为能力项；`/reserved-memory` 拒绝规则标注为输入子集。**第六十轮（代码 `f52730bef` + 文档）**：`/reserved-memory` 错误四拆（缺失/不支持、cells 缺失/不一致）；O-12i 架构资格三分（能力/物理访问 ReqProfile/逐叶映射兑现）；O-12k 扩容即失效与快照 ID 生成规则。§6 134/70。**第六十一轮（纯文档）**：`EfiReservedMemoryType` 拆规范事实/项目政策三层（强禁令说法标待核）；RISC-V 页表准入补 RsrvEventual 与 PMP 证据提供者；`OrdinaryCachePolicyFeasiblePages` 取代 WB 命名；`BootInputObjectId` 首读前建立；`PhysicalRange` 统一 u128 端点；设备完整性三主张分开。**第六十二轮（纯文档）**：PMM 用途化 `PmmEligible(page, role, p)`；缓存可行性与实际叶子拆分；RISC-V PTE 更新方案条件化（Svade 不要求 RsrvEventual）+ hart 方案一致；`EfiReservedMemoryType` 补回 `DenyOsUse` 三层；AArch64 64 KiB 分组属性规则。**第六十三轮（代码+文档）**：PMM 拆基底层与用途层（`PmmBaseAvailable`/`PmmBaseExcluded` 补集、`PmmRoleEligible(page, U, p)`/`PmmRoleRejected`，用途可组合且单调、未知即 `AllRoles`）；`MemoryOrderingCompatiblePages` 入基底；A/D 方案绑 `menvcfg.ADUE` 有效值、hart 一致性降为项目要求、缺证据默认 `SupervisorFaultManagedAd`，SMP 段无条件 `RsrvEventual` 撤销；整页合格 = 逐 hart 求交 → 按用途求交 → 向内取整页；AArch64 64 KiB 按物理地址分组且只比 Table 2.5 字段、限定 UEFI profile、未映射子页记 `GroupCoverage`；代码落 `UnsupportedDynamicReservation`（size-only 拒绝、`reg`+`size` 按 `reg`），三条专测；minix-boot **137**、boot-shim **70**，riscv bootface 与 aarch64/x86_64 shim 三条 QEMU 腿 PASS（x86 腿 `no dumpdtb.dtb on ESP`，未进入 DTB 采集路径）。**第六十四轮（纯文档）**：撤销两处自设规则——hart 方案一致是**规范逐字要求**（`norm:pte_ad_scheme_uniform`），「缺证据默认软件方案」不安全；`ADUE=0`/未实现 Svadu 都推不出软件更新，补 `EffectiveSvade` 四行真值表与 `Unknown`（不可证→拒绝启动），`PteUpdateEvidence` 扩字段；`H` 进签名 → `MemoryAdmissionContext`（hart_set_id 非数量、换代即失效）；SMP 拆 `HartRuntimeAdmission`/`CommonCpuAccessEligiblePages`/`RoleAdmit(PageTable)`；原子用途补 `AtomicAlgorithmRequirement` 与 Rsrv 三级；幂等术语更正（`Idempotent`/`Non-idempotent`，撤销杜撰的 `Non-resident`）；AArch64 比较对象改为**有效属性**而非 `AttrIndx`；RISC-V 与 DTSpec 改为本方直接取证（主源文件+行号+锚点写入 §6/03），UEFI 侧 403 未取证仍标待核；动态保留从「原文未取到」改为「规范允许、本项目不支持」。主简报 1183 行，lint error 0。**第六十五轮（代码+文档）**：撤销上一轮自写的 `EffectiveSvade` 布尔式（与同段真值表矛盾，`Svadu=1 ∧ ADUE=1 ∧ Svade=1` 会被算成软件方案），改为分支式 `PteUpdateScheme(hart)` 并加反例负控；`SystemHartSet`（全系统方案一致性）与 `ParticipatingHartSet`（`ctx.H` 逐 hart 交集）分开，运行时禁止单 hart 换方案、第一版初始化后冻结；入场审计扩到既存存活对象并给四行处置表；一致性/排序继续拆清并作出「第一版不支持软件维护一致性的非一致主存」的项目选择；`fallback` 改凭证、补 `Ziccamoa`/`Ziccamoc`/`Ziccrse` 的 Profile 全局义务取交集；代码落 `physical_end`+`UnsupportedPhysicalAddressEnd`（`==2^64` 与 `>2^64` 分名，`checked_add` 读数 0），四测试；规范证据钉 commit（RISC-V `418bdc49…`、DTSpec `8314f1e1…`、release 标签 `v20260120`），RVA 与 UEFI 仍未取证。minix-boot **141**、boot-shim **70**，三条 QEMU 腿 PASS。**第六十六轮（纯文档）**：回写版本状态——评审对照的是 `fffaa3e5f`，三个 P0 在 `2eca58006` 已落地并给锚点；修掉自造误读源（正文不再逐字保留被撤销的布尔式）；`SystemHartSet` 补清单来源与「永不启用≠不属于系统」政策；`HartRuntimeAdmission` 增映射/访问路径维度并换用更锋利的处置表；`ProfileMemoryObligations(isa_profile)` 与 `PlatformMemoryProfile` 分离、补 `Ziccif`；`fallback_plan_id`+`AtomicFallbackPlan` 五字段；顶端页口径改为「记录层应接受 `==2^64`，今天拒绝是 u64 临时限制」；§6 记 DTSpec revision 串与 UEFI 双侧取证状态。主简报 1285 行，lint error 0。**第六十七轮（纯文档）**：第二次按旧文本评审——四项被记缺失者均在 `7aeea8964`；对策是主文档加**版本横幅**（本版提交号 + `git rev-parse` 核对），并把旧符号原文清零（`fallback_available`/`HartAdmittedBytes`/布尔式 计数均为 0）。实质新增：PMP 证据改按访问类别索引（`AccessRequirement`/`HartAccessEligibleBytes`/`RoleRequirements`）；Profile 义务补 `Ssccptr` 并定「第一版不声明 RVA ⇒ 空集≠已满足」；`AtomicFallbackPlan` 补 `required_memory` 与锁/状态路径；O-11 增第三条硬约束（`FinalMap` 与复制之间重证全范围可达）；AArch64 补 `EfiReserved`↔`EfiReservedMemoryType` 名称对应与双侧取证；§7 增逐项签字视图；登记 `SourceRecord` 变体拆分候选（非阻断）。主简报 1318 行，lint error 0。**第六十八轮（纯文档）**：第三次按旧文本评审（其自述依据 `f52730bef`/137-70，当前 `9cf7f574d`/141-70）；版本横幅扩为「提交号 + 内容轮次」，离线材料也有判据。实质新增：`PageTableRoleAdmission(page, ctx)` 独立成谓词，PMM 与对象审计共用（引导期自举页表/静态页表页不经 PMM，只写分配路径会漏审）；页域公式补裁决步 `FullPages(Union(ResolveCompatibleClaims(RamSourceDomain)))`；Profile 义务清单标注非全集（`Za128rs`/`Zicclsm`）；§7 增「页表用途准入入口」行。主简报 1331 行，lint error 0。**第六十九轮（代码+文档）**：`SourceRecord` 改 `range: PhysicalRange{start:u64,end_exclusive:u128}`；`physical_end` 只拒 `>2^64`，`==2^64` 进记录层，拒绝移到投影层 `projectable_end()`（`UnsupportedPhysicalAddressEnd`）；三条输入路径共用 u128 求和；新增 `physical_range_expresses_the_top_page`。**行为不变**（最高页仍不交付），变的是类型与拒绝点。`§7` 删掉过早闭合句；`ProfileMemoryObligations(profile, region)`；依赖最终成功的 LR/SC 第一版只收 `RsrvEventual`，`AlgorithmFallback` 登记为未启用扩展；RISC-V 证据改钉 `418bdc49…`。minix-boot **142**、boot-shim **70**，三条 QEMU 腿 PASS，三目标交叉编译通过。主简报 1331 行。**第七十轮（纯文档）**：PMP 判定模型纠错——规范是「编号最低、命中任意字节的条目决定整次操作，且须覆盖全部字节」，上一版「逐字节合成」写错；公式改 `PmpPermits`/`AdmittedRanges`（引 `norm:pmp_entry_priority`/`norm:pmp_full_match_required`/`norm:pmp_rwx_check`/`norm:pmp_no_entry_match`）。补 `ArchitectureAdmissionRejected` 为第五类排除原因并区分`DefinitelyIneligible`/`InsufficientEvidence`；不变量 3 与 O-03 统一；设备窗口三分 + xAPIC 只映射 4 KiB 规则；RVA 补 `Zicclsm`；`ADUE=0` 记来源、真值表第四行条件改「两者均被证据证明未实现」。主简报 1359 行，lint error 0。**第七十一轮（代码+文档）**：新增 `audit_source_coverage`（free⊆Memory 记录链、reserved⊆保留类记录链，允许相邻接续），riscv bootface 改 `collect_with_sources()` 并在启动路径调用审计（失败即停）——记录层首个真实消费者；`grep -rn collect_with_sources` 由 0 处变 1 处。更正我方算术错误：xAPIC 授权是 **2 MiB** 不是 18 MiB，问题改为「4 KiB 需求为何用 2 MiB 叶子」，并把该授权标为待证平台假设。补「hart 一致性比较有效方案而非扩展位图」、`SystemHartInventoryId`/`ParticipatingHartSetId` 两个身份、no-map 两个子禁令不可压平。minix-boot **146**、boot-shim **70**，三条 QEMU 腿 PASS，三目标交叉编译通过；主简报 1363 行。**第七十二轮（材料清理，纯文档）**：撤回此前「评审按旧文本」的判定——主文档无该公式，但撤销掉的布尔式逐字留在 02/03 档案里，送审时被当成现行规则；本轮把三处逐字旧公式删除并原位注明，02/03 标题加「历史档案，不是现行规则」，主文档横幅加**本版时间**并声明唯一现行来源。**第七十三轮（纯文档）**：评审首次对上现行文本并确认七项闭合。修正三处：① 页表准入自相矛盾（`PmmBaseAvailable` 扣了存活对象，既有页表永远通不过）→ 拆 backing/分配/既有对象三个谓词；② 公式只用了 PMP、PMA 停在文字里 → `PhysicalAccessPermits = PmpPermits ∧ PmaPermits`，并区分操作起始地址与整次访问 footprint、页合格用 `PagesFullyInside`；③ 补 `PmpImplementationState`（零条 PMP 合法）与有效特权级（隐式页表访问按 S-mode）。另：`GeneralData` 宽度按实际代码、Profile 义务按来源三分（`Ziccamoc` 属 Development Options、`Ssccptr` 属 S64）、`HartInventoryEvidence` 含 `completeness_claim`、CPU 节点 `disabled/fail` 语义按 DTSpec、旧字段名与 §7 状态表清理。主简报 1421 行。第七十四轮：整页合格改成 `RolePageEligible`（∀ 合法起始 × 整次访问 × 每个 hart），`LegalStarts` 第一版锁页级保守合同，跨页对象另用 `ObjectAccessEligibility`，第一版只准入自然对齐访问；PMP 操作→许可位表按 `norm:pmp_load_fault`/`norm:pmp_store_fault`/`norm:pmp_exec_fault` 逐行取证，Smepmp 第一版只接受 `MML=0 ∧ MMWP=0`，`Absent` 须附平台支持限制；O-11 补 EFI DTB × MRB × `no-map` 三方组合表与「类型按页 / 访问按原始字节」粒度分工；`EfiReservedMemoryType` 首层回到 UEFI 字面「Not usable」；APIC 核对改逐逻辑处理器；RVA23 profile 义务改为本方直取。主简报 1530 行，仍无代码改动。第七十五轮：`Footprint` 顶端改为只拒绝 `a + width > 2^64`（允许 `end == 2^64`）；`PageTable` 需求拆五项，显式 PTE 写在硬件与软件 A/D 方案下都必需；新增 `InstructionFetchPattern`/`FetchFootprints`，明确取指对齐 ≠ 宽度且可跨页（RISC-V IALIGN=16 与「border a protection boundary」原文已取），页级用保守片段集、对象级按真实片段判两页；`ObjectAccessEligibility` 拆成物理准入与 VA→PA/权限/属性/别名两谓词，`pa_start` 须来自独立输入；MRB 表拆「原始字节相交」与「仅页级投影相交」；新增 `PhysicalProtectionInventory`/`PlatformProtectionPermits`，`RLB` 进证据稳定性证明；profile 五档校验表与 `ReservationSetRequirement`；`RoleRequirements`/`LegalStarts` 非空前置与 `GeneralDataCompleteness`；`CacheBlockZero` 进枚举并声明不支持。主简报 1617 行，仍无代码改动。第七十六轮：把 `RoleAdmit` 与 `RolePageEligible` 收成单一展开式并把 `U ≠ ∅` 写进 `PmmRoleEligible`；拆 `BaseAccessRequirements`（普通数据页基底，含 `GeneralDataCompleteness`）与 `RoleSpecificRequirements`（用途额外类别，`GeneralData → ∅` 为定义），消除 `GeneralData` 的双层定义冲突；新增 `ImplicitHardwarePteUpdate`（含 `atomicity = AtomicPteUpdate` 与 `RsrvEventual`）并把 `atomicity` 加进 `AccessRequirement`，`PageTableBackingEligible` 补「显式 PTE 读写与 A/D 方案无关」；对象访问改为 `ObjectAccessUse { va_range, expected_pa_range, … }` + `ActualFootprints`（Fetch 走 `FetchFootprints` 全片段）+ 区间级 `Resolve`；合同 A 补「用途规定的完整 footprint 约束」；稳定性改「不可变承诺（第一版）/ 变更前协调」二选一；`ReservationSetGeometryPermits` 进原子准入；`CacheBlockManagement` 入枚举并补 `norm:pmp_same`/`norm:pma_same` 块级一致性规则（CBM 是 load **或** store 许可，不是按读权限）。另更正横幅时间为实测值。主简报 1663 行，仍无代码改动。第七十七轮：`ObjectMappedAccessEligible` 补政策授权层 `ObjectPolicyAccessEligible`（来源授权 + MRB/`no-map`/`EfiReservedMemoryType` 各自的负向解除）；`PageTable` 补回始终必需的隐式页表读并给显式读写绑 `PteSize`/`PteAlignment`；`RoleRequirementsComplete`/`Applicable`、非 `GeneralData` 用途非空、`ctx.ParticipatingCpuSet ≠ ∅` 进前置；`AtomicRoleEligible` 四项合取并把第一版统一成「LR/SC 一律要 `RsrvEventual`」；`ProfileAdmissionValid`/`ProfileMemoryEligiblePages` 给出 ISA 义务执行位置；`MemoryAdmissionContext` 身份三分（平台内存 / ISA / UEFI 属性）；`expected_pa_range` 改成分段 `ExpectedTranslationSegments` 并让取指实例自带宽度；共享层与架构 adapter 三条硬规则；`MappingAudit` 补正向授权与 `DenyMapping` 不相交。主简报 1696 行，仍无代码改动。第七十八轮：把只定义未消费的谓词接进最终公式——`RoleAdmit(AtomicSharedState)` 派发到 `AtomicRoleEligible`（逐 CPU、逐实际访问范围判预约集几何），`AdmissionContextValid` 进 `BasePageEligible`，`ProfileMemoryEligiblePages` 进 `PmmBaseAvailable`；对象访问改为 `ActualAccessInstance`（每实例自带宽度/对齐/片段）+ `AccessAlignmentSatisfied` + `RequiredCpus ≠ ∅`；`GeneralDataCompleteness` 进 `BasePageEligible`；按 `norm:lr_sc_rsrv_set_page` 把 LR/SC 的基础页内约束与 `Za64rs` 的 profile 几何分成两层；`operation` 改用语义名（`NormalRead`/`NormalWrite`/`InstructionFetch`/`AtomicUpdate`/`CacheMaintenance`），`pte_update_scheme_id` 移入 `adapter_evidence` 槽位；`CacheMaintenance{Prefetch}` 入模型；新增 `PolicyProjectionKey`；O-12g 与 §7 状态同步到第七十八轮。为守 1700 行删了 5 处被正式定义取代的散文。主简报 1697 行，仍无代码改动。第七十九轮：`MemoryAdmissionContext` 补 `legal_starts_contract`（进 `AdmissionContextId`）与 `ParticipatingCpuSet = Resolve(id, inventory_snapshot)`（不可变、绑定 `evidence_generation`，共享公式不再用 `ctx.H`），`SystemHartInventoryId` 归入 `adapter_evidence`；`ProfileAdmissionState` 四态（`NotDeclared` 不得返回「通过」，`ProfileMemoryEligiblePages` 取中性语义而非空集，`SystemPteUpdateSchemeUniform` 非 RISC-V 须 `NotApplicable`）；`ObjectAccessUse` 补 `access_use_id`/`object_id` 并消除未定义字段，新增 `ObjectAccessInventoryState`（`Incomplete` 必须失败）与 `RequiredCpuScope`；原子拆 `AtomicPageRoleRequirement`（页级，用 `LegalStarts`）与 `AtomicAccessInstance`（对象级实际访问）；`MappingIntent` 进 `MappingAudit`（标准/驱动受控/其它显式，推测访问禁令独立审计）；O-05 改 `MutableMappingMetadata`；`ArchitectureAdmissionRejected` 补 `profile_memory_obligation_evidence`；`PhysicalProtectionInventory` 补 `scope`。主简报 1698 行，仍无代码改动。第八十轮：`SystemHartSet` 内方案不一致改为「拒绝平台配置」（`disabled`/暂不调度不再作为排除依据）；原子用途合并为唯一入口 `AtomicSharedStateEligible`（由 `RolePageEligible` 显式调用，含 family、进展与几何，AMO-only 几何为 `NotApplicable`），`AtomicRoleEligible` 撤销；`ReservationSetGeometrySatisfied` 改页级签名；对象访问补 `ActualAccesses ≠ ∅`、`physical_footprints ≠ ∅`、分段无空洞、`AccessAlignmentSatisfied` 进合取，`RequiredCpuScope` 改显式二分支，`physical_footprints` 定义为实际硬件访问操作；`ProfileAdmissionUsable` 与 `ProfileConformanceSatisfied` 分离，`AdmissionContextValid` 改用前者；`rsrv_eventual_required` 改为语义级 `progress_guarantee`；写明 `AdmissionContextId` 构造字段；`UNUSABLE` 增独立 TypePolicy 禁止项。主简报 1698 行，仍无代码改动。第八十一轮：撤销 `NotApplicable` 逃逸口（第一版只建模 `CpuAccess(cpus)`，非 CPU 主体 `UnsupportedAccessActor`；`NoAccess` 需机器可检查证明）；`ActualAccessInstance` 带虚拟/物理范围与 `reservation_set_evidence`，判定式显式加 `TranslateExpected` 绑定与逐实例 `AccessAlignmentSatisfied`；撤销 `AtomicAccessInstance`，页级与对象级共用 `ReservationSetGeometryCore`；`AtomicSharedStateEligible` 去掉与通用展开重复的 `AccessRequirementsSatisfied`；共享层 `ExecutionPrivilege` 取代 `{S,U}`、量词改用 `ParticipatingCpuSet`、`AtomicUpdate` 只留 `family`；CPU 状态五行表并选定 `SystemHartSet` 语义；`PageTableBackingEligible` 补排序模型/Profile 区域义务/内存类别，幂等约束改标项目政策；Profile 拆三层并由 `ProfileConformanceSatisfied` 聚合；O-12k 插入 `RevalidateBootInputObjectsAgainstFinalMap`。按纪律 8 合并稳定重复陈述，主简报 1695 行，仍无代码改动。第八十二轮：`RolePageEligible(page, PageTable, ctx) = PageTableBackingEligible(page, ctx)`（幂等/排序/Profile 区域义务都在 backing 内，两条路径共用）；`RoleSpecificRequirements(AtomicSharedState) = ExpandAtomic(RegisteredAtomicAlgorithms)` 等式与「每个算法完整展开」的完整性定义；`AtomicProgressRequirement` 三分（AMO-only 不要求 `RsrvEventual`）；页级几何加 `∀ a ∈ LegalStarts` 量词；对象访问拆 `VirtualAccessInstance → TranslatedAccessFragments → ActualMemoryOperations`，逐操作使用各自 requirement，对齐改 `ArchitecturalAlignmentSatisfied`；`ProfileContextState` 改名并引入 `AllRegionObligationsSatisfied`；`ArchitectureAdmissionDenial` 逐页归因；`NoAccess` 蕴含空清单且证明有范围；`RUNTIME_SERVICES_CODE/DATA` 定为第一版永久排除。主简报 1692 行，仍无代码改动。本批为结构重构批（非评审轮）：主简报按纪律 8 收缩到 119,988 字符（-15.1%）、1636 行，新增 §0 现行模型速览，取证引文与用例穷举逐字移入 `03` 第五十四节、被删改原文移入 `02` 第七节，符号名/测试名/规范锚点对账 lost 0，一条规则未撤销。

**第二十八轮已收尾（文档提交 `154aed914`）**：非 UEFI `SourceDomain` 只由 `/memory` 构造、保留声明在交集内生效；`ReservedMapped` 两步派生（声明向外、backing 向内）与 `DenyOnly`；R21 补齐启动/设备两类义务并加 T2 前审计；R18 定义 `RequiredPhysicalRanges`；R13-A 白名单收紧到函数级；T1 审计完成点与 S1 访问清单；输入表把 `ReservedPrefixEnd` 划为政策标量；类型 12 改按项目支持边界；§4.4.1b 按解析/转换/接入三层事实重写。

**第二十七轮已收尾（文档提交 `cdf6ba20a`）**：三输入域/记录所有权对齐、AArch64 4 KiB granule 锁定、归一化角色表、R13-A 改 `AccessUseId`（调用点完备性列为实现先决）、保留映射区进普通主存准入、PERSISTENT 归诊断成员、39 条计数与 24 组对照；**设备树侧保留来源已落码**（提交 `38668f843`：`/reserved-memory` 静态 `reg` + 文件头保留映射块接入，声明按交集扣除、`no-map` 与保留块不进两表，容量/回绕/重叠/不可读形状拒绝；10 条采集测试，riscv bootface QEMU PASS）；**被排除描述符的访问属性留痕通道**——设计 §7 要求未知位与被类型排除的描述符「记录进诊断、不静默忽略」，现实现的第三条输出记录的是区间（起点+长度）而非属性字，因此属性维度的「不静默」只到「区间可见」这一层，逐条属性值仍未落诊断；本项等政策矩阵给出要留哪几个字段后一并落码，避免先建一条无人读的旁路。**核实三件套**：① `cargo test -p boot-shim -p minix-boot` = 64 通过 / 0 失败 / 0 忽略（另 5 通过；minix-boot 65 通过 / 0 失败）（本模块 32 枚宿主用例，含五个政策维度交叉与四个具名裁决顺序组合）；② 三架构真机门 `cd os/qemu-tests && bash run_all.sh` → 37 passed / 2 failed / 0 skipped / 0 build failures（本树现跑；两条红 `test-smp-aps`/`test-smp-ipi` 在同一台机器上用**未改动的 rewrite 尖子 `2b72a1e62` 重新构建同一载体**复现同一 `### PANIC … arch/src/x86_64/protection.rs:0x1c3`，判为既有、非本批引入）；③ 权威 mock 门 docker run --rm -m 2g -v <本树>:/work -w /work/os minix-ci:1.94 cargo test -j 1 -p minix-kernel --features mock → 865 通过 / 0 失败 / 12 忽略（本批；记录基线 857/0/8，计数差异未归因——本批未新增 minix-kernel 用例，失败数 0 为判据）。**已冻结**：唯一生产解析器 + 唯一分类规则；零分配分类 + 预分配输出；`ram_top` 从分类器删除（不留 characterization test）；属性处理方案 A（保留原始 `Attribute`，UEFI 适配器内准入，不进公共交接协议）；`PA_LIMIT` 与 DM 覆盖两合同分离；`DescriptorRef` 含 attribute + 唯一解析器负责字段边界与 4 KiB 几何校验；side list = shim 内部诊断输出（不构成跨引导边界保留协议）；五步裁决顺序（未知/ISA fail-closed → `UNACCEPTED`/`PAL_CODE` 拒绝启动 → 类型强制处置（类型胜过属性）→ 普通内存候选 × `RUNTIME`/`NV`/`SP` → WB 准入；`RESERVED+NV` 等四组合落定）。**暂缓冻结（各有验收门）**：属性政策矩阵逐行批准（凭原型 + 三架构证据）、三架构普通 WB 合同（x86 PAT/MTRR、aarch64 AttrIndx、riscv64 PMA——I-29-16）、Pass 1/2 具体 API（归原型 ①）、路径 A 端到端验收（归原型 ②）。**执行序列**：原型 ① 生产解析器接预分配 sink（穷测矩阵含裁决顺序四具名用例）→ 原型 ② ∥ 三架构合同 → 政策矩阵批准 → 冻结实现规格。**DoD**：设计笔记 v12 §7 五步全过 + `classify_map` 落地类型政策与属性准入 + `bootstrap_alloc_limit` 改 `bound-1` + §2.5.9 注释修正 + I-22 supersede 记录。**详情**：`01-todo.md` §3 I-29 条（v2–v12 全演进）+ 设计笔记 v12（`rewrite-notes/01-stage-kernel/01-design-note-memmap-consolidation.md`） |
> 上表 `I-29` 条仍开放的项已整理为一份**不依赖代码即可评审**的自包含简报：`rewrite-notes/01-stage-kernel/01-adjudication-brief-memmap-open-items.md`。两轮外部评审已回填，简报现为六组待裁决——三架构普通主存合同、类型 × 属性政策矩阵逐行批准、退出引导服务之后的端到端验收、设备树来源的保留区与不可映射节点政策、被排除描述符的留痕通道，以及新增的「不可映射范围由哪棵页表树兑现」——外加一个前置阻断项（属性位常量的规范来源与固件报告的规范修订号）。简报同时记下两轮的结论与本方回应（四处补强、三处不同意），以及七处按仓库读数与规范原文修正的事实错误；它不改判任何已冻结条目，也不替任何一行填结论。第三方的逐行批准读数回到本行与本表的核实三件套登记。第二轮外部评审已回填：六组待裁决维持，裁决六的「只限定措辞」一案被否，改为「诊断样本」与「启动映射禁止清单」两个对象（后者跨界且不可截断）；本方按规范原文、EDK2 与 Linux 三处读数推翻了上一轮「ISA 属性掩码比规范宽」的断言，属性位的前置阻断项因此改写为配置档四项。第三轮评审已回填：版本门控改为「按固件声明的版本整体选档或拒绝」（同一位在两个配置档里含义相反，按位容错不安全，测试哨兵取值也需替换）；x86 选属性表项的位数由本方写错并已改正；`/memory` 出界闸不再宣称符合「忽略」要求，改为显式的三案选择；设备树保留来源拆成三个名字并补一条本仓局限（现有模型是上界而非区间列表）；裁决六补三条页表级实现合同（最小页粒度兑现、启动对象冲突检查、第四种输出的硬预检）。评审人已撤回与本方的掩码分歧，并建议五项优先阻断改完即冻结结构、不再润色第四轮。第四轮评审已回填：双方各自撤回一次关于 2.10 的结论——2.10 有 2022 年原始版与 2024 年 Errata A 两份正式文档且属性常量不同，表头修订字段区分不了它们，因此属性配置档的选档机制改成「已核实的固件组合白名单」，逐位表改为按配置档标识键控；另把「页级归一化只能有一份集合」「禁止清单由唯一生成阶段产出、合并不得丢来源」「预备阶段对象需独占有效长度而非依赖类型清零」三项写入实现合同。本文档仍有优先阻断项未清，不具备冻结条件。第五轮评审已回填（两份意见，一份挑缺口、一份通读全文，判据一致：架构方向不重开，缺的是数据合同本身）：属性位表不再是笼统的「2.10 配置档」，改为有身份的实现兼容档 `MEMATTR_PROFILE_UEFI_RAW_0_9`，白名单补齐身份维度与「唯一 / 零 / 多命中 / 样本违约」四路匹配算法并明确样本只作验证证据；「版本」拆成表头修订、描述符版本、配置档标识三个互不替代的字段；「页级归一化只能有一份集合」由本方改写为「一份坐标 + 多个语义集合」并给出五条集合等式（第六轮扩到七条），取整规则扩到全部 PMM 排除范围；§0.3 数据流里那个第三个区间输入经本方核实为本文画错（交接结构里的 `reserved_regions` 是分类器输出，不是输入）已删；`/memory` 出界闸的 A/C 之争由本方让步关闭（选 A）；本方另主动删除 `PERSISTENT` 的完整范围承诺与 §6.1 的「被排除区域不许访问」两句兑现不了的表述，并补出三条本仓现存缺陷（设备树收集器 8 格上限用满后静默丢弃区间、运行时服务类型的类型政策只靠兜底分支隐式成立、重试循环对固件服务的依赖无类型或断言保证）。第五轮之后本文档没有遗留的事实性分歧，只剩两处设计粒度待定；结构可冻结，完整实现合同仍等材料包一的固件样本与三架构读数。第六轮评审已回填（八项问题：一项是本文档自己的内部矛盾、一项是数据流缺失输入、余为关系不完备与合同不闭合）。本方核对后七条采纳、一条反驳：反驳的是评审人自己提出的「类型与运行时属性双向等价」，本方按规范读数改为单向校验（只查运行时服务类型缺运行期属性位）——规范把运行期内存的身份落在属性位上、把「退出引导服务后必须保留」落在类型上，双向判据会误杀设备窗口类型带该位的形态。为此本文第一次改写冻结条目：冻-6 由「五步裁决顺序」改为六步（在属性裁决之前插入「类型与生命周期属性的一致性」一步，各步政策内容不变），并在 §0.4 单独说明而不是悄悄换数字。新增 §6.8 `StartupObjectInventory`（启动对象登记表）作为规划阶段的第四个输入，它不是第三种内存来源也不是另一份保留区间表；本方按读数补上支持该缺口的现场证据——直接映射覆盖验收的注释记录过一次真实缺页故障（内核镜像与栈不在那三个来源里、别名从未被装过，留有具体故障地址与每轮复现读数）。集合等式由五条扩到七条（评审人指出漏了「可映射保留区」与「诊断成员」同样不许分配，本方核实为真漏），并把 PMM 排除集合定义为「不许分配」的完整集合。区间溢出合同与既有实现对照后改写：那条 C 复刻函数确实用饱和加法，与「溢出即拒绝」不天然一致；本方另核出分类器已是正确的两步 checked 形状并改以它为样板，顺带查出一处现存脆弱性（一条相交断言用裸加法算末端，今天被上游校验挡住所以不可达）。白名单身份拆成「运行时可读（参与匹配）」与「流水线证据（不参与匹配）」两层，并承认第五轮写出了循环定义（配置档标识同时是输入与结果）；按读数补一条现实约束：厂商字段本仓零读取点，故第一版只能登记一条已核实组合。RISC-V 清单补排序与一致性两个维度（支持原子指令不等于多核看到一致顺序），准入收紧为三条同时成立。评审人点名的四处旧决策文字（§4.4.1 的先 C 后 B、§4.7 两问、§8 三输入名单、§13 出界闸改造项）已全部清理，并按读数修正其中一条判据（那道闸门在本仓只有 1 个生产调用点，两条裸机腿从不叫它）。重试阶段按裁定定为三档、运行时类型与属性不一致按项目加强政策 fail-closed 并挂固件样本前提门，第五轮遗留的两处设计粒度据此关闭。轮后遗留当时的判断是「全部是取值问题而非结构问题」，八项待裁定集中在 8 KiB 判据处置分级、登记表最小完备集与阶段档数、集合等式是否收敛为最小无关集、白名单起步方式四处——**这个判断被第七轮撤销**，见下一段。第七轮评审已回填（首条即一条规范事实纠错，另含三个结构性缺口的暴露）。**本方第六轮写进正文的「带运行期属性位的描述符必须 8 KiB 对齐、长度为 8 KiB 整数倍」不是本项目三个目标架构的规范要求**：评审人核对 UEFI 2.10 官方文本与其 Errata A 页面都是 4 KiB，2.9A 里 x64/AArch64/RISC-V 各架构章节同为 4 KiB，8 KiB 那一支出自 Itanium 架构章节。本方撤销该判据本身（不再在「严格报错」与「记录后继续」之间选择），现有解析层的 4 KiB 起点对齐检查与要求一致、无需改动；本方自己的检索读数与另一份公开翻译在同一位置给出两种数值，冲突如实登记成表，并承认这违反了本文 §7.1 自定的纪律——本方当时没核对那句话属于哪个架构分支，这是本方第三次栽在出处问题上、且第一次错在「章节归属」这一维（§7.1 的要求因此从出版物标识+日期扩到三维）。结构性修正一：启动对象登记表`StartupObjectInventory`第六轮只接了「与禁止清单冲突检查」一个消费者，而本方自己在 §6.8 记录的历史故障起因是内核镜像与栈**没进入最终直接映射的构造来源**——只接冲突检查等于没解决那次故障、还会让读者以为已解决。第七轮按字段派生三种结果：分配排除、阶段访问要求、**页表映射要求**，第三种写成合同 StartupMappingRequirements(树,阶段) ⊆ MappedRanges(树,阶段)，按两棵树与阶段两个轴分别验收，并接到本仓「最终直接映射由四份来源取并集」那个具体落点上（那段来源说明里正留着一次真实缺页与一条需要区间减法才能闭合的未竟项）。结构性修正二：新增集合关系 R4「仍然存活的启动对象汇入 PMM 排除集合」与 R7，「阶段」同时加到排除集合与可用集合上；评审人给的错误形状（一段加载器数据进可用表、其中一页放着栈或页表页、内核起来后被重分配、而最后访问还没发生）本方核对成立。本仓没有任何一处定义过释放点，因此第一版一律保持排除、不自动归还。集合等式由七条扩到十条。结构性修正三：物理地址上界单独立节 §4.4.4，本方核对结果比评审人的前提更微妙——**本仓不是缺这个概念，而是同一概念有两份实现且内存图准入用了粗的那一份**：内核侧三架构各有一份「能力与格式取小」的访问器（x86-64 读 CPUID 扩展叶子的物理地址位数并与 52 取小、AArch64 读 PARANGE 且低于 48 位直接判不支持、RISC-V 取平台位数与分页模式编码位数之小，Sv39 下为 48），引导壳那个编译期常量的注释自己写着「计划由分页层同名访问器取代」。本方据此修正评审人的一条前提：RISC-V 是有取值的，缺的是那个平台位数按平台登记的证据、不是数值本身。另按他的 4.1 给出越界处置的三档来源表（UEFI 描述符拒绝 / 设备树保留范围按其政策 / 本项目对象拒绝），并规定不得存在未区分来源的默认裁剪。白名单：本方**撤销第六轮「第一版只登记一条已核实组合」作为解决方案**——评审人给的两种情况本方核对都致命：那些字段列为必需则本仓读不到→零命中→本文的算法拒绝本项目自己的三条腿；声明不约束则唯一命中是虚假的唯一性；样本补不上，因为本方自己已规定样本不是选择器。改为必须在方案 A（来宾可读身份闭合）与方案 B（把限制写成外部支持前提并声明这不是引导程序自己完成的识别）之间显式二选一，本方选 B，并在身份键闭合前保留 fail-closed、不开放泛化匹配。另外三处：重试阶段的受限能力面禁止再读配置表（本方补一条准确性说明——直接读系统表内存不等于调用引导服务，规范没把只读访问一律禁止，因此这是项目为可验证性选的加强约束而非合规要求；并核出 aarch64 那条腿的配置表快照与 §4.3 的复制规则必须落在同一个动作上）；登记表完备性改用可追溯性判据（跨内存图改变/页表切换/分配器启用三类边界的对象要能追溯范围、最后使用阶段与每棵树上的访问方式），本方按此自查出两类漏项：引导壳自身映像与栈（那段不用堆、把诊断写进栈上定长缓冲的中止处理路径正跨边界）、以及原始内存图缓冲与规划工作区（核实与分类器输出存储是两块不同存储）；计数纠错——本方「七条里有两条冗余」数错了（只有一条可推出），补入「可用 ∩ 可映射保留区」后十条里三条冗余成立，并规定编号写在等式左边、推出依据写在右边，数量由表自己决定不由散文决定。**第七轮之后本文不再建议冻结**：撤销第六轮「四项先决做完即可冻结」与「遗留全是取值问题」两句判断——后者是本方把三个结构性缺口错分进取值类。§8 里登记表与排除集合的「已闭合」标注改为「结构已定、接线待核验」，身份键从取值类升为先决。本方对「什么算闭合」的标准收紧为：一个抽象只有点名了每一个下游消费者并写明接不上时会怎样失败，才允许标为已闭合。下一轮范围按评审人的话限定为这三项补合同 + 一次完整交叉审计，不重开六组讨论。第八轮评审已回填（本轮没有新原则，八项全部核对成立并采纳，无新增不同意项；重心从「补合同」移到「让合同能被代码与测试证明」）。**先决项一：PMM 排除集合的「完整」以前是空的。**第七轮把排除集合定义成所有不许分配的页的完整集合，但 §2.1 第 3 步判为排除与设备路径的两类去向**在本仓根本不产生区间**——本方核到裁决函数对固件保留与不可用类型直接返回长度零，三处空分支（预检计数、写入分发、提交落盘）都不发区间。于是即使十条关系全通过也证明不了覆盖，因为那些范围从未进入任何集合。补三样：`PmmPolicyUniverse` 统计范围（取进入本项目内存政策的范围而非全部地址空间）、含 `TypeExcludedRanges` 的**构造式**（按评审人说法它不必成为另一份跨边界协议，但必须有完整生成过程）、以及把验收拆成关系测试与**来源完整性测试**两组，并写一条反例用例：故意漏收不可用类型的实现必须仍通不过这一组，而它能通过全部十条等式。**先决项二：映射子集合同过弱。**评审人给的例子一针见血——某内核代码页物理上确有映射但映射到别的虚拟地址且权限可写不可执行，纯物理覆盖检查通过而访问合同一点没兑现；本方承认这与本文从第三轮起反复纠正的是同一个毛病（用有映射压了四件事）。合同按物理覆盖 / 虚拟访问路径 / 访问权限 / 属性与共享性四维重写，映射要求字段从四选一全局枚举改为按 `(树, 阶段)` 索引；并按他裁定**撤销本方第七轮的倾向**：登记表不是第五份映射候选来源，而是**映射义务的权威来源**，义务满足不了时才补带完整映射语义的对象候选或拒绝启动（否则引导壳自身映像会因被登记而自动成为最终直接映射候选）。**先决项三：方案 B 落成形状并关闭。**配置档改为编译期绑定到具体引导壳构建产物，固件镜像身份与模拟器的版本 / 机器 / 处理器参数由测试流水线记录固定，样本只作符合性检查，引导壳第一版不承诺运行时身份识别；采纳他那条边界——**不能把外部支持前提写成运行时 fail-closed 已保证**，引导壳能因属性组合违反绑定档而失败，但不能判断一个身份未知的固件是否就是清单里那份。§7.1 标题、身份表、§8 依赖图第 0 步、§13 两条取证项同步改，登记项上限为 1 随运行时选择器作废。另有五项：三类生命周期补入 §6.8（复制后的设备树缓冲区、登记表自身存储、中止路径的可达阶段）；栈的物理范围改为从分配点或已有映射取、**禁止由描述符类型反推**（一条加载器数据描述符可能覆盖多个对象，虚拟栈顶推不出物理栈底），本方核交接结构确实只有虚拟栈顶、无物理栈底字段，结论与第七轮相同但理由完全不同；来源收集缓冲超限一律拒绝启动（诊断样本可截断、来源缓冲不可，因为它决定政策结果），并把设备树两条硬性判据标注为项目支持子集而非规范强制；x86 增条款⑦逐核核对 `CR0.CD / CR0.NW`（本方核到全仓 5 处写 `CR0` 全是只改自己关心的位、无一处读或清这两位的现量命令见简报 §10，即缓存禁用状态被原样透传；且分页处已有「固件不保证 WP」的同形先例）；R1–R10 补显式阶段量词。**本方第七轮的一处说法被控制流读数修正**：该路径跨的是**引导根启用**而非切到最终根，因此不要求最终根映射；而本方当时的理由（注释自称驻留）是从注释推行为，是本文批评过三次的做法、本方第四次踩在同一类上。§0.2 把属性整体定义成能力位图也被指出过宽——属性位是异质的四类（硬件能力、内存性质、操作系统必须遵守的映射要求、政策提示），与本文对运行期位自己的用法矛盾，已改写。旧文清理四处：§12.3 里复活的 8 KiB 句子**整句删除不加旁注**、§11 标题轮次与 §12 不一致、§12.4 一行重复两次、§13 两处早期「做完即冻结」口径标注为历史。**第八轮之后仍不建议冻结**：本方接受，并把范围收窄为把这两份合同变成能被代码与测试证明的东西加一次完整交叉审计，不推翻任何已签署的架构原则。本方对闭合的标准再收紧一次：第七轮立了「写完字段与生产者不算闭合」的纪律，第八轮仍需评审人来兑现——说明把一句采纳进来的话写进正文不等于按它自检。**送审文档已拆成两份（同批）**：正文过长无法整份发给外部评审人，故把八轮的逐轮变化清单、结论与签字状态表、本方逐轮的采纳/补强/不同意与错误登记、以及被后续轮次替代的阻断表与取证表移入同目录的 `02-review-history-memmap-open-items.md`（逐字移出，未改写未删减，正文只留指针与「编号不连续是故意的」说明，以免 §11.x / §12.3 这类交叉引用与档案原文对不上）。正文由 38.3 万字节降到 25.9 万，档案 12.9 万；拆分后做过逐行核对——原文 1399 个非空行在两份文件里合计命中 0 缺失。**被移出的条目不因移出而失效**：正文里凡写「第 N 轮已改 / 已撤销」之处，原文与撤销理由都能在档案查到，这正是正文 §7.1 要求「改动需要被看见」对文档自身结构的应用。12.4「本轮分歧的关闭状态」与 §0.5 的待裁问题**留在正文**，因为那是评审人要答的东西，不是历史。第九轮评审已回填（十五条，本方逐条核对后**全部接受、本轮零反驳**；评审人同时规定下一轮不再做泛泛审阅，改为围绕材料包做「哪些已实现、哪些只有文档声明、哪些数据流根本没接通」的逐条对照）。结构性修正六处：一，统计范围补不相交并集分区式（新 R5b，集合关系由十条扩到十三条）并按 UEFI / 非 UEFI 分别给构造式，去掉兜底桶、各项先与统计范围求交、登记对象越界即报错；二，一个同时表达「这一阶段仍需访问」与「第一版仍不归还」的集合被拆成两个派生集合（分别交给 R4/R7 与 R10/R13），评审人举的分叉例子正是本仓的原始设备树缓冲区；三，L2 层此前没有完整禁止集合，本文实际是靠最终直接映射那四份候选「恰好没包含」来满足它——补 `FinalMapDenyRanges` 与 R11、R12；四，阶段边界改按体系结构状态真正切换的时点定义（**撤销本方第八轮刚写下的「按函数完成时刻归前一阶段」**），四档落成五列转换表；五，`EfiMemoryMappedIO` 与 `EfiMemoryMappedIOPortSpace` 拆开（本方记一条自检：**一周内第二次被 Itanium 专属条款咬到**，规范断言的标注要求由两维扩到第三维「通用条款还是某架构专属条款」）；六，映射义务的跨交接形式定为「只传固定布局的派生记录、登记表本体不过界、禁止双重权威」，权限默认精确 R/W/X 并新增页级冲突与别名权限审计，方案 B 的运行时选档残留整段撤下，§6.5 的 C′ 由「下一批换乙案」的备选降为**不是当前政策**。**本方按调用链核出一条比评审人怀疑的更宽的事实**：aarch64 的 UEFI 腿上确有**两份不同的设备树**（磁盘镜像那份只解析 `/memory`，配置表那份引导壳只登记地址、从不解析），且全仓所有 Rust 源文件里字符串 `/reserved-memory` **零命中**——所以缺的不是某条腿的接线，而是本项目从未解析过设备树保留子树，整套 §4.2–§4.6 政策的定性由「等接线」改为「等实现」（并列为本轮新提出、请评审人裁的两问之一）。**本方撤销自己本轮写下的一条不成立缺陷登记**：评审人要的退出状态码三路分类（成功 / 版本戳失效可重试 / 其它立即终止）本仓配对循环**早已实现**、重试上限为 2、且已有两枚宿主用例锁住两个方向，本仓的设计笔记与阶段台账也早写明它——本文档落后代码五轮，而本方写下「本仓不区分」之前没有读那段代码。真实缺陷在另一条腿：路径 A 那个退出封装把返回值整体丢弃，失败与成功对调用方不可区分，它被 `prepare_boot` 与 22 个引导期测试内核入口调用——**这才是本仓现存缺陷第五条**（简报 §8 本轮新增一张「本仓现存缺陷登记表」把五条汇总，此前它们散在 §4.3、§2.1、§6.4 与 §10）。本方据此给简报加两条硬规则：**凡写「本仓某处是缺陷」必须与一条本方跑过的现量命令同段出现**；**零命中类断言不得只看计数**（本方用 `grep 'no-map'` 想证明零实现，读数却是 2，两条都是 `errno-mapping` 的子串）。另按评审人裁定落笔四项：`CR0.CD/NW` 检查失败 fail-closed 且不得顺手清位、x86 普通页与每种大页分别做「索引位 + 真实树遍历」两层验收、RISC-V 平台清单以代码常量表为准并细化原子登记（操作类别 / 宽度 / 对齐 / 排序强度 / 证据）、物理地址上界统一记号为 `effective_pa_limit = min(平台位数, 格式位数)` 并纠正本方「Sv39 只有 48 位」的写法（两个常量分别是 48 与 56）。状态同步六处（§0.5 / §7.2 / §8 / §12.4 / §13 / 文首），本方承认其中三处是同一轮里改了正文忘了表。**第九轮之后本文仍不建议冻结**；本轮没有新增任何顶层抽象，只拆已有集合之间的边界。两份文件当前规模：正文 373361 字节、档案 185048 字节（本行由脚本在合入前刷新一次，避免登记旧读数）。第十轮评审已回填（评审人这份标题写「第九轮」，审阅的却是本方第九轮回填后的正文；本方按收到次数记为第十轮并在文首登记编号差异）。十五条意见与本方七个待裁问题**逐条核对后全部接受，本轮仍零反驳**；他同时明确政策方向已收敛，阻止冻结的换成数据模型与实现合同，并规定下一轮只围绕四条数据流逐条对照（谁生产、保存在哪、谁消费、容量如何预检、超限怎样失败、用什么测试证明完整）。结构性修正七处：一，统计范围改成 `SourceDomain` → `PmmPolicyUniverse` 两层构造——第九轮那版在建立统计域时就把待统计的排除项减掉（UEFI 腿先取「普通内存候选」、非 UEFI 腿先减设备树保留范围、还先减前缀阈值），于是「范围 ∩ 统计域」可能是空集，**分区式照样通过而被类型强制排除的范围从未被统计**：这是本方本轮承认最严重的一处，因为它是**假闭合**（会通过全部测试）；现在只减设备地址类，其余一律作为域内具名成员，R5b 补 `∀p`，`AvailableRanges` 定死为「政策上有资格」而非「此刻可分配」。二，`DeviceTreeReservedRanges` 此前只在正文宣布过、没进构造式——本轮补齐三层（两路原始来源不合并且带来源标签）。三，`FinalMapDenyRanges` 此前只有一条子集关系、**等于没定义**；本轮给出五项具名成员构造式、点名哪些排除范围仍允许最终直接映射（`ReservedMappedRanges` 与启动对象页必须不在其中），新增 R15 验收关系与 R14 及其前提 R16，**并撤回本方第九轮那句「不必新增交接协议」**——内核拿不到诊断成员与类型强制排除，无从重建，故改为独立跨界集合。四，R13 从物理覆盖子集关系改回四维合同谓词 `MappingSatisfies`，拆五个可独立失败的子判据；上位概念改 `StartupAccessRequirements(树,阶段)`，「不要求映射」不再是映射义务里的魔法值；R10 删掉第一版实际不存在的例外通道并按树选禁止集合。五，对象目录按固定镜像 / 启动期分配 / 派生协议三类逐块补齐（含 DTB 复制目标、两块静态数组、映射义务 ABI 与 L2 禁止集合自身存储）；中止路径要写的串口寄存器窗口按二选一归**独立设备映射义务**，不塞进登记表；S1 的固件页表裁定为外部平台前提，不作第三棵树纳入四维验收，代价是 S1 的访问需求另进平台假设清单。六，阶段边界由「根寄存器写入那条指令」推进到「新根写入且该架构要求的地址转换同步与指令同步完成」，过渡窗口内双侧覆盖或证明不访问，四档不增加；本方自加两次切换各一条跨架构用例。七，类型 12 由本方误写的「按未知类型处理」改判「已知但本项目不支持的标准类型 → 拒绝启动」，并附一条样本取证门：前提「三个架构实际固件不输出它」本仓无证据，类型 11 因其在三架构常规出现而**故意不对称**地走域外完整保留，并给 `DeviceAddressRanges` 补齐来源、存储、消费者与超限即拒绝——这同时补上第九轮欠的域外出口。另有四项按裁定落笔：完整诊断成员必须一次性贡献给排除结果与适用的最终禁止结果、不得靠可截断样本恢复（本方并区分开「不跨界」与「不需要完整存储」）；x86 `CR0.CD/NW` 标为**项目准入加强**而非跨架构必要条件；RISC-V 原子清单主键换成**真实指令实现路径 × 宽度 × 对齐 × 区域 PMA 等级 × 排序要求**（比较交换要分 LR/SC 序列与 Zacas 的 `AMOCAS`，8/16 位要分 Zabha 原生 / 宽操作模拟 / 锁回退）；映射义务 ABI 存 **OS 内存语义**（`MemoryClass` / `Shareability`）而非 `PAT 索引 / AttrIdx / PBMT` 这类架构编码值，§8 那格由「已改」如实降级为「分工已定、字段合同待定」。**本方本轮按调用链核出两条决定性的仓库事实**：① aarch64 的 UEFI 腿上那**份设备树复制从未发生**——引导壳只把配置表条目的物理地址登记成平台描述来源，内核 `parse_by_kind` 至今按那个**原始地址**解引用解析，所以本文第七轮起写的「原始缓冲区最后使用点＝复制完成」是在描述一个不存在的动作，他担心的「禁止映射原始 DTB 而内核仍持有原始地址」是现在时的冲突；本轮据此定下五步数据流，并**第一次承认「不动固定布局」这一倾向对某个对象失效**（栈那一格仍成立，DTB 复制目标那一格必须变更交接记录语义）。② 仓库注释里有一条**待复现**的实测读数：AAVMF 配置表「8 条目、ACPI2 在、两个设备树 GUID 都不在」——若成立，他的裁定「采用配置表 DTB」在本项目 aarch64 UEFI 测试环境上等于「没有设备树保留输入」，本方把这条与取证项一起登记，不做选择性沉默。**并且本方登记一条比既往更严重的自身错误**：第九轮本方在本档案与阶段台账里写着「已把设备树两条判据标注为项目支持子集」，而正文 §4.3 从来没有那半句——这是**在一处记录里宣称已在另一处完成、而两处都不能自证**；本轮已补进正文，并新增一条硬自检：**凡登记「正文本轮已改 X」必须带一条能在正文检索到 X 的关键词，收口前回读一次**（已写进档案 §六 第 5 条）。评审人点名的十一处旧口径全部清理（含 §7.1 那句与它自己表格标题冲突的「只有第一层参与匹配」）。他裁掉的七个开放问题（设备树来源、§4 状态定性、L3 升级登记形状、RISC-V 清单归属、区间数学落 `minix-boot`、保留冗余关系、诊断配额分配）全部关闭，本方本轮新提出两问（受控环境是否真有配置表 DTB 条目、L2 禁止集合与义务 ABI 的容量与布局版本）。**第十轮之后本文仍不建议冻结，本轮没有改写任何冻结条目、没有新增顶层抽象**。两份文件当前规模：正文 373355 字节、档案 185042 字节。【送审文件结构再拆一次（第十轮同批）】正文回填到 1770 行 / 320494 字节之后，用户给出可粘贴上限「1500~1800 行」，本方把正文的**第十节（复核锚点）与第十三节「材料包与逐轮取证项」**逐字移入新建的材料册 `03-evidence-and-materials-memmap-open-items.md`（188 行），正文相应位置留编号占位与指针（编号不连续仍是故意的，交叉引用不变）；§8 的第六轮历史阻断表与 §7.1 残留的五段运行时选档文字移入档案 §四 / §五（档案现 640 行）。**移出不等于降级**：拆分后做了逐行核对——被移出的 190 个非空行在目标文件里全部命中，只有两行是两节自己的标题（材料册用新标题重写，属预期差异，不是内容丢失）。三份文件的分工写进正文文首的文档结构说明。第十一轮评审已回填（他这份标题写「第十轮回填版审阅意见」，审的正是本方第十轮回填后的正文；本方按收到次数记为第十一轮并在文首登记编号差异）。29 条意见与他七处直接裁定**逐条核对后全部接受，连续第三轮零反驳**。他这一轮的判断变了性质：不再问「还缺哪个抽象」，而是指出**本方第十轮新补的形式化关系自己又把不同语义压回了一个集合关系**，并点名两处最危险。结构性修正八处：一，排除集合公式漏掉了自己新加的 `ExplicitPolicyDeniedRanges`（R16 因此不成立）——按他的方案一补进公式，并接受他更狠的那句「不得成为换名的兜底桶」：给出 E-1 / E-2 封闭登记表、**第一版规定它为空**、完整性测试要断言每个元素可追溯到一条具名登记；R16 补 `∀p`。二，**集合差本身成了新的掩盖器**：`SourceDomain − DeviceAddressRanges` 会把「同一页既是 RAM 候选又是设备寄存器」这种来源矛盾直接抹掉，公式照样闭合——新增关系 R17「先判来源自洽，再算域外减法」，非空即拒绝启动。三，`ReservedPrefixEnd`（标量输入）与 `PrefixRangeView`（派生视图）分开命名，并明确**后者无独立存储**、不进容量清单；本方第十轮同时在正文写「阈值不进任何区间集合」又在公式里放 `ReservedPrefixRanges`，属同一节内两句话打脸的第三次。四，**R10 把 L2 当成了绝对禁令**：L2 的定义是「不得进入最终直接映射」，不等于「这页在任何虚拟地址都不能映射」，而本方自己给映射义务留了三种访问路径——R10 升级为按树**与访问路径**选判据；第一版**不提供可绕过 L2 的专用映射例外**，§2.2 行 3 那半句「可经审查提供专用路径」限缩为**未来政策**；实际负向验收拆成 R15a（引导根直接映射 ∩ L3）与 R15b（`FinalDirectMapRanges` ∩ L2），被测集合按映射种类分开。五，**刚拆开的类型 11 又被并回去**：本方第十轮在 §2.1 分开 11 与 12，转头在 `DeviceAddressRanges` 里让 11 与架构设备窗口共用集合与消费者——按他给的拆成 `FirmwareRuntimeMmioRanges`（第一版**无消费者**）与 `DeviceWindowRanges`；并撤回本方「样本一旦输出类型 12 即改判域外」那句自动规则：**样本只证明固件输出，不证明本项目有消费者与合同**。六，设备映射义务从一句名字变成一份合同：六个问题逐条落地（语义字段含**设备内存语义**、生产者与**两棵树都消费**、跨界与独立容量、参与页表验收、与两个来源集合的关系），并新增关系 R18 让它也过禁止冲突检查——否则本方第十轮那句「串口不属于登记表」恰好让最该拒绝启动的冲突无人检查；存储清单本轮定为**唯一权威**（§5.3，§3.3 问题 D 只引用），并裁定 `PmmExcludedRanges` **不跨界**、`TypeExcludedRanges` **流式贡献不独立存储**。七，设备树那条链补齐跨架构闭合点：UEFI 腿两路保留声明**必须先证明被固件内存图完整覆盖**再谈求交（否则求交为空、R4b 与 R5b 双双通过而声明哪儿也没进——第十轮那个假闭合换了位置），覆盖对照拆成范围覆盖（规范强制）与类型符合（建议级 ⇒ 本项目升级为严格，标项目加强）；非 UEFI 腿补 `AuthoritativeDtbSource(启动方式)` 与「DTB 落在保留映射块内时读取先于禁止生效」；复制前置补**源可读性五问**（他的判据：区间算术正确不等于源内存真的可读）；退化规则从三态并格拆成**四态**（absent / 单一有效 / 重复冲突 / 损坏，损坏不得降格成没有输入），并删掉本方上一版留在 §4.4.1b 的「是否解析配置表那份」备选口径（他已裁定，本文不能同时写「合同已定」与「请裁合同是否存在」）。八，R13d / R13e 落到跨架构语义：R13d 不再假定各架构都有「叶子属性索引与共享性字段」（只有 AArch64 两者皆有，x86 要 PAT/MTRR 合成、RISC-V Sv39 普通页表项根本没有缓存属性索引），改为核对**实际映射与平台属性共同兑现的 OS 内存语义**；`MemoryClass = NormalWriteBack` 与本仓 §1.1「不要求平台有物理缓存」顶牛，拆成主存类别 / 一致性与共享性 / 排序 / 原子能力四维，具体缓存策略只留在架构证明层；R13e 从不可测的「整体兼容」改成按（物理页, 阶段或转换窗口）归并的可判定关系（`RX`+`R` 允许、`RX`+`RW` 拒绝、跨阶段分别生效不算冲突、切换窗口内合并判定）。阶段模型按他的第 16、17 项拆成**两个正交状态机**（`BootServicesState` 与 `MemoryLifecyclePhase`，真实存在 `BootServicesExited` × `S1` 这种组合，压进一个枚举就会出现无法表达的状态），S4 改名「最终根生效阶段」、服务器就绪降为独立生命周期事件、表头改「物理页分配器是否已启用」。**本方本轮登记四处自身错误**：① 术语表落后正文一整轮（`PmmPolicyUniverse` 词条仍是第九轮旧公式、`归一化集合族` 还挂着已拆掉的旧名）——本方第十轮改正文时没改词条，而词条是第三方读者的第一入口；② 又把 `grep -rc` 的**行匹配数**写成「122 处使用」，本方第十轮刚立「计数类断言不能只看计数」，那条规则当时只覆盖了零命中、没覆盖非零计数；③ 同一节内两句话打脸第三次（前缀阈值三处口径）；④ 第十轮那次「拆开」只做到命名，没做到消费者与后果不同。四处的共同成因是本方的「已改」停在被点名的那一段，没有扩到引用它的词条、总表与相邻集合，本文此后要求每次改名或先列一张「谁引用了这个名字」的清单。**行数控制**：用户本轮给出硬上限「主文档不超过 1800 行」，第十一轮回填一度把正文推到 1986 行，本方按第八、第十轮同一套纪律再移一次——原 §0.2、§1.5、§2.4、§3.1、§5.1、§6.1、§7.2、§9 八段移入材料册（现第三至第八节），§4.4.1 的三案细节、§6.5 的 C′ 完整形状与 §8 的三层状态框架移入档案；正文现为 1792 行，档案 753 行、材料册 462 行；逐行核对被移出的 180 个非空行在目标文件里全部命中（0 缺失），移出的都是形状事实、机制背景、取证面与词表，无一条政策判断或待裁问题被移出。他裁掉的七处问题全部关闭，本方本轮新向他提出的问题：`SourceDomain` 的类型 11 与设备窗口对应规则要不要第一版就写成合同。**第十一轮之后本文仍不建议冻结，本轮未改写任何冻结条目、未新增顶层抽象**；下一轮按他规定只对公式、集合边界、跨界载体与实际页表结果四类做逐条审计。第十二轮评审已回填（他这份标题写「第十一轮重写稿」，审的是第十一轮回填后的正文；本方记为第十二轮）。29 条意见含七项先决，**逐条核对后全部接受，连续第四轮零反驳**；本轮他第一次给出冻结路线：**先修完 先决 再冻结政策结构，硬件证据、QEMU 验收与实现缺陷作为独立的实现门槛**。结构性修正六处：一，统计域公式漏掉刚拆出的 `FirmwareRuntimeMmioRanges`（只减了 `DeviceWindowRanges`），类型 11 描述符两边都不落、R5b 不成立——按他的建议**两个都减**，并写死该集合的职责是「来源完整性记录 + 交叉核验件」而不是义务；二，R17 从一句「RAM ∩ 设备 = ∅」升级成**按来源身份的封闭兼容矩阵**（三个来源集合定义 + 五条类别对 + 类型 11 与设备窗口「具名可验证对应关系」的五条可执行规则：只允许相等或包含、部分交叉不允许、只按页区间判定、重复取并、不匹配即拒绝）；三，**设备义务接进 R13-D**、R18 补全量词与路径维度（`DeniedFor` 只对两条路径有定义，其它路径必须显式登记），`R15a` 改名 `BootIdentityMappedRanges`，`MappingPath` 四值、**普通启动对象的 `DedicatedPath` 第一版为未支持值**；四，**设备义务表自身的 ABI 存储进对象目录**（补暂存区包含关系检查、并用「符号地址取自链接期静态范围」解决登记表自身的递归）；五，两条腿的 DTB 消费点闭合：每条 `AuthoritativeDtbSource` 补 `PostShimConsumer` 三值、可读性拆「来源合法」与「当前可达」（后者在 S1 属固件页表下的外部前提，进平台假设清单）、**无 DTB 的准入条件**写成「平台证据证明无额外 no-map / 保留映射块语义或已由其它来源承载，否则拒绝，证据不能是「QEMU 能启动」」；六，计数改成四行并列（编号区间 / **21 行具名关系式** / R13 五个验收面 / 实际测试另计）并同步 §8，`MemoryClass = NormalWriteBack` 与 `CoherentShareable` 两个示例值拆成 `AddressSpaceClass` / `CoherenceContract` / `SharingScope` / `OrderingRequirement` / `AtomicOperationRequirements` 五字段（可缓存性降为平台能力与页表证明资料），RISC-V 补 `SupportedAccess` 四项（取指、普通读写、硬件页表读、硬件页表写）。另按他的 9.1 / 9.2 / 11.2 / 11.3 / 11.4 补齐：`TypeExcludedRanges` 的流式贡献改成**两个下游**（PMM 排除与 L2 各自计数、两遍扫描）、§3.3 问题 D 改为直接引用 §5.3 唯一清单、R13e 的 `W^X` 别名规则标为**项目映射安全合同**并强调跨对象跨页归并、`PrefixRangeView` 补「不保证页对齐 ⇒ 走统一外向归一化」与「超过 `effective_pa_limit` 即拒绝启动、不静默钳制」、**E-1 登记表的七个必填字段写进正文**。**本方登记两处自身错误**：① 又一次「宣布已加、其实没加」——第十一轮写「只能经 E-1 那种具名登记增长」而正文从没有 E-1 表，被引用的依据不在正文里等于没有依据；此后规则扩成「引用即自证」与「改名先列引用清单」；② 公式与名词分解不同步的第二次——第十一轮拆了两份域外集合并只减一份，与第十轮「拆了 11/12 又并回去」同类，拆的做完了、依赖它的公式没跟上。**本轮首次给出冻结边界**（正文新增声明）：**政策与合同结构建议冻结**（有哪些集合、每个集合回答什么问题、集合之间有哪些关系、哪些对象必须登记、交接什么），**不冻结实现形态**；实现门槛——现存缺陷、DTB 两条腿的解析、四份禁止与义务集合落地、三硬件与 QEMU 读数，以及材料册里逐轮列出的那些取证项独立跟踪、不阻断概念冻结；并请他确认这个边界。**行数**：主文档上限 1800，回填一度到 1857，把 §0.5 的两张历史裁定表移入档案、§2.5 逐位处置表与 §2.3 矩阵完备性判据移入材料册后，正文 1797 行、档案 834 行、材料册 526 行。第十三轮评审已回填（他审的是第十二轮回填后的正文）。他这一轮的落点全在「关系式自身是否闭合」，并第一次把冻结拆成两层。本轮修正：**§0.3 整体重写为自包含核心**（集合定义、构造式、22 行关系式、三态访问义务、五个验收面、计数口径全在正文，不必翻材料册），并彻底清掉 `DeviceAddressRanges`、`StartupRequiredObjectRanges` 与 `MappingPath = NoMapping` 三处旧名/旧值；**R17 补合法来源叠加并扩成封闭矩阵**（新增 RAM×设备树保留、类型 11×设备树保留、设备窗口×类型强制排除，以及「同一 MRB 内条目重叠必须在归一化之前拒绝」）；**访问义务拆成三态**（`MustMap` / `NoRequirement` / `MustNotMap(范围)`），新增 `StartupNoMappingRequirements` 与关系 R19；**路径模型统一**（`DeniedFor` 逐条定义两条已支持路径与 `DeviceWindowAccess`，三条义务对象各对应自己的关系）；**设备树两路保留语义分开写合同**（`/reserved-memory` 的 `no-map` 与 MRB 的「未授权不得访问」不同，MRB 与设备窗口相交直接拒绝）；**DTB 前置补格式检查点**（基址与各区段对齐、`version` / `last_comp_version` 兼容范围，并写明谁负责）；**状态机补允许组合表**、非 UEFI 腿记 `NotApplicable`；**§8 依赖图改成四段流水线**解除裁决二 ↔ 裁决六的循环；**RISC-V 访问能力补宽度与对齐粒度并按区域限定范围**。冻结门按他的建议改成**两层**：语义结构与方向可签；每份集合的精确定义、每条关系式、交接 ABI 字段合同本轮已逐条落笔、请他核验后同上签署；实现门槛独立跟踪。**行数**：用户本轮把上限从 1800 收紧到 1780 并要求主文档自包含，重写 §0.3 后正文 1609 行（档案 852 行、材料册 539 行）。第十四轮评审已回填：评审人**签署第一层**（语义结构与设计方向）、拒绝第二层，并给出八项下一轮清单。本轮修正：`BootMapDenyRanges` 补出成员来源×四层归属表（类型 11 显式禁止进两棵普通映射树）；`DeviceWindowAccess` 虚拟关系定死为恒等（`VA=PA`），套用所在树禁止集合是定义结果；访问义务扩成四态（`MustNotMap` / `MustNotAccess`），新增 R20；区间量化改双向（RAM 向内、排除向外）；引导根 `RWX` 裁决为按对象拆权限、例外须具名；`AvailableRanges` 只交接 S3 并由内核逐项释放；R4b 拆 `R4b-s`/`R4b-p`；R17 加「先保留来源身份再归一化」硬约束并补三类同源重叠；计数改为 **22 关系组 + 23 原子断言**；`effective_pa_limit` 在 §8 如实降级；RISC-V 取保守口径；权威 DTB 在 MRB 内为**窄用途授权**；§0.5 冻结门改三档（一层可签、二层待核验、三层 ABI 不可签）；§1 状态改为方向已定、硬件证据属实现门槛。正文 1656 行（上限 1780），档案 869 行。第十五轮评审已回填（第一层可签、第二层暂不签）。本轮修正六处：类型 11 与真实设备窗口改为**相交即拒绝**（两条政策不能同时成立）；显式禁令拆成 `ExplicitBootMapDenyRanges` / `ExplicitFinalMapDenyRanges`（E-1 从此能表达只升 L2）；`SourceDomain` 改分语义归一化并删除通用向外取整旧条款；RISC-V 取方案 A 删掉「只对特定区域要求」的例外；状态机乘积表补 `NotApplicable`、S1 改名「项目页表启用前的外部根」；四态义务与命名统一，新增 R21（普通主存映射正向验收），计数改为 22 组 + **25 条原子断言**并建立追溯表。规范分级修正两处（节点内 `reg` 重叠属项目支持子集；类型 12 不引申为跨架构禁令）。正文 1663 行（上限 1750）。第十六轮评审已回填（第一层下调为「方向认可、条件待闭合」，第二三层不签）。本轮修正：R3 加定义域（类型 11 是域外成员）；R4b-s 恒真问题用新关系 R4b-c（非 UEFI 保留必须被 /memory 覆盖）补上；R19 落成三个具名 scope；新增 R22（设备义务必须来自已登记窗口）；R21 补齐五面定义；类型 11 设备路径格按 R17 改口；MustNotAccess 证明边界收紧并写全设备路径套用 L2/L3 的后果；RISC-V 删掉「按区域限定」准入例外并补跨 hart 共同能力；设备义务改用设备 I/O 语义；R13e 审计全部真实叶子且大页不得跨界；S3 后回收补范围来源二选一与 NeverReclaim；NoRequirement 定为不跨界；对象目录补四类 per-hart 冷读对象；计数重算为 **26 条原子断言**。正文 1676 行（上限 1750）。第十七轮评审已回填（第一层「架构中立语义方向」可冻结、完整合同条件未闭合；第二三层不签）。本轮修正：R16 限定到 `L1RequiredDenyRanges` 并补「只升 L2 的登记项范围不得在可用表内」；R4b-c 拆成原始字节覆盖与页级政策两条；R22 补 `AuthorityId` 四条判据；新增 R9b（保留区 ∩ 最终 L2 = ∅）；R21 给出页类别的生成规则；类型 11 残留句改为相交即拒；设备页改 `DeviceIoContract`、R13-D 覆盖设备专属字段；RISC-V 补硬件 A/D 与统一 PTE 方案；四态义务的跨界转换写清；S3 回收写成三值字段并默认 `NeverReclaim`；计数重算为 28 条原子断言 / 22 个关系组。正文 1693 行（上限 1750）。第十八至十九轮评审已回填。第十八轮：R17 去「按具名政策判定」、R14 改独立守卫、R4b-s 分腿、计数 30、输出表改四份记录、R21 权限表填满 + VA 可表示性、MustNotMap 声明有意扩大、S3ReleaseDisposition 进必填字段、NoRequirement 完整性矩阵、DeviceIoContract 读写幂等分开、DTB 先分配后取 MapKey。第十九轮：ACPI_RECLAIM 定 NeverReclaim、R21 五字段填满、R17 五步顺序 + 同源重叠、类型 11 拒绝标项目限制、S1 改外部地址转换状态（per-hart + SMP 停驻）、T1/T2 成验收键、设备路径 VA 布局冲突检查。第二十轮评审已回填。P0 是设备访问模型按腿拆分：MemoryMappedDevice 走 DeviceMappingRequirements + 页表验收；x86 端口 I/O 控制台（COM1 0x3F8，in/out）走 DeviceAccessRequirements，无页表叶子、不进设备窗口、不参与恒等 VA 检查；LAPIC/IO-APIC 不入第一版窗口清单。原子要求改回 OS 语义（不写 LR/SC/AMO，宽度按实际依赖）；新增 DeviceWindowPolicy 承载 AuthorityId；R13-D 拆成 DeviceMappingSatisfies 与 DeviceAccessSatisfies；ACPI_RECLAIM 表格改为「未来版本条件」；R17 补非 UEFI MRB 组合与「完全重复=范围+类型+属性全同」；DTB 源可读性按腿分支；断言口径分成 30 条记录 + 独立不变量。

| I-23 | 01 篇补一小段：三形态引导矩阵与 `fw_payload` 的真板用途（读者易将直核 bootface 误判为 QEMU 测试专用） | ✅ 已修（01TODO-DOC：§3.7 落位矩阵 + `[ARCH]` 三处一致，设计侧镜像在主树工作区未代提交）。原登记：⏳ open（2026-10-18 用户议题登记）：**现状读数**——`grep -cE 'fw_payload\|FW_PAYLOAD\|fw_jump\|fw_dynamic' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 得 **0**；§3.7 L1695 已列三形态（同镜像内调用 / 跨镜像跳转 / 直核自引导）并带 `[ARCH: 内核从内联执行改为独立镜像 + 交接载荷]` 标注；L1771 riscv64 那段只写“在没有 U-Boot 与文件系统的 `-kernel` 场景里”——读者易把直核判成“QEMU 测试专用”，不知道它在真板上对应 OpenSBI 规范里的 `FW_PAYLOAD` 模式（与 `fw_jump`、`fw_dynamic` 并列）。**教学缺口**：一小段（~10 行）的位置——不扩章。**目标（§3.7 三形态列表下方一小段 ~10 行）**：① `FW_PAYLOAD` 是 OpenSBI 规范的正式固件形态之一（next stage 打包进 OpenSBI 镜像本身、boot 时直跳 S-mode 目标、省 U-Boot），不是 QEMU 编造的测试通道；② 三架构落位矩阵——x86_64 **无此形态**（PC 平台规范强制 BIOS/UEFI 存在，`-kernel` 只在 QEMU 侧模拟；真板上没等价形态，coreboot+depthcharge 勉强类比但不是主流），aarch64 有 `bootface_a64.rs`、主要作 QEMU `-kernel` 测试通道（真板走 UEFI/EDK2 或 U-Boot，**无规范化**的省 bootloader 形态），riscv64 **兼具**测试与真板 `FW_PAYLOAD` 场景（嵌入式 flash 紧张、启动时序固定的产品形态）；③ 与 minix3 C 原始形态的偏离度——minix3 只走 multiboot / U-Boot 两条，本项目的“直核”是 **Architectural Evolution**，为跨架构引导兼容性加的（三档术语纪律要求 `[ARCH: ...]` 三处一致，落地时在正文与 §5.2 检查清单同步）；④ 与 I-22 丁案的分工——I-22 统一 memmap 源、明确**不合并引导形态层**（公理 3）；本段作为那段裁决的**读者侧背景**，让读者先看本段再看 I-22 才不觉得“两链不合并”是浪费；与 I-1 分工——I-1 补 x86 跨镜像跳转协议，本段解释“为何 x86 不补直核”（平台规范使然）。**判据三问**（`prompt/review-rules/review-core-semantics.md` §1.6）——讲“直核形态存在”是引导形态层（OS 语义层）问题、不是硬件形状泄漏；`fw_payload`/`fw_jump`/`fw_dynamic` 是固件规范正式术语，允许在正文出现；一段讲完不扩章。**连带耦合（改时必须同批动）**：① §3.7 L1695 节标题的 `[ARCH: ...]` 标注射程评估——本段落地后或将标注扩为“三形态引导矩阵”、或另加一条独立 `[ARCH]`，现场判；② §3 章头导读 L1253 “三种形态”一句话与新段保持口径一致；③ L1771 riscv64 那段“在没有 U-Boot 与文件系统的 `-kernel` 场景里”需与新段互指，避免两处描述脱节；④ §4.8 bootface 甲案实现段——若本段落在 §3.7 而 §4.8 只讲实现，需给一个“读者先读 §3.7 新段再看 §4.8”的指针；⑤ §5.2 检查清单——`[ARCH]` 三处一致标注同步；⑥ §6 故障定位章——若涉及“直核与生产链症状差异”（例如 `fw_payload` 场景 DTB 递空、U-Boot 场景 boot-file-table magic 校验失败），可能要加一小格。**门依赖**：无（`tools/doc-code-density-check.sh` 不卡这一小段；`[ARCH]` 三处一致纪律由 `review-doc-checklist.md` 把经）。**解除条件（DoD）**：一小段落地 + `[ARCH]` 三处一致 + 与 I-1/I-22 相互引用不打架；只讲 fw_payload 术语不举三架构落位矩阵 = 教学半成品不计入收敛；篇幅扩至一整节 = 违反“不占太大篇幅”约束、评审时会被要求缩回。**现算命令（复核用）**：`grep -cE 'fw_payload\|FW_PAYLOAD\|fw_jump\|fw_dynamic' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（当前 **0**，落地后至少 **1**）；`grep -cE '\[ARCH:.*(三形态\|引导矩阵\|fw_payload)' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（当前 **0**，落地后视标注射程而定）。**触发窗口**：与 I-22 同批落地最省——一次 §3.7 结构改动带两处；分两批会撞 §3.7 标题与表格的 diff 冲突 |
| I-24 | §3.1 L1310 「内核自带 stub」这条被否理由与今天 bootface 直核形态高度重合（现有「半否半留」括号补丁使读者绕），重写为「UEFI 形态被否 / opensbi 形态采纳」 | ✅ 已修（01TODO-DOC：甲案拆两行「UEFI 形态被否 / opensbi 形态采纳」，§3.1 衔接段与 L1410 依赖方向句同批收敛）。原登记：⏳ open（2026-10-18 用户议题登记）：**现状读数**——L1310 主句把「内核自带 stub、内核自己读文件再自举」列为被否，理由「boot-shim 与内核变成同一个二进制的两个入口」；同一句括号里承认「riscv64/aarch64 的 `kernel-image` 自引导面复用 opensbi 取件库、不引入 `uefi` 依赖，**不在否决之列**——方向差异见 §3.3」。主句描述与今天 §3.7 L1771 riscv64 bootface 直核段在**动作层面完全同构**（内核自带引导面 + 复用取件库 + 自己读文件再自举），差别只在固件形态 uefi vs opensbi。另外主句「同一个二进制的两个入口」与 L1410 §3.3 依赖方向段（`kernel-image` 与 `minix-kernel` 是两个 crate 两个产物，按目标反向依赖 boot-shim 库）已对不上。**候选三案**：甲·拆两行——「内核自带 **UEFI 形态** stub」被否（uefi target 在 riscv64 不存在，x86/aarch64 会污染 `minix-kernel` crate 依赖方向）；「内核自带 **opensbi 形态** stub」→ **采纳**，落点 §3.7 直核自引导；乙·整行删除——从「被否方案」表里挪走，改到 §3.3 依赖方向那一节讲「uefi crate 不进 `minix-kernel`」；丙·保留一行收紧否决面——主句改为「内核直接 `use uefi::*` 把 UEFI 引导链编进内核」，将否决点收窄到「内核 crate 里出现 uefi 依赖」这一层，明示「opensbi 侧走独立 bootface 不在否决之列，见 §3.7/§3.3」。**推荐甲**（表格保持「引导协议选路」三行到四行的完整性，读者一眼看清「内核自带引导面」一族的两个变体——一个被否、一个被采纳；乙会问「为何只剩两条」、丙虽保守但被否方案名字与实际被否动作已不太对应）。**连带耦合（改时必须同批动的四处）**：① §3.2 尾巴 L1370「boot-shim 是一个独立可执行体…不能把它说成‘新板子只需写 boot-shim’」——这条正好是「两产物 + 反向依赖」的分界，L1310 改后要引到位；② §3.3 L1384-1410 依赖方向段——`minix-kernel` 不依赖 boot-shim vs `kernel-image` 按目标反向依赖 boot-shim 库（`cfg(target_arch = "riscv64")` / `"aarch64"` 下各加 boot-shim opensbi feature + `fdt`），甲案两行分界就落在这段；③ §3.7 L1695 直核自引导段与 §4.8 L2564 跨镜像交接段——bootface 落点，甲案的「采纳」行指向这里；④ §5.2 检查清单或 §6 若引用「三种被否路径」（L1304 当前「三种」字样）需联动，甲案后改为「四种」或删数量词。**门依赖**：无（`tools/` 不卡这一小段）。**触发窗口**：**与 I-22 同批落地最省**——I-22 会把 memmap 源抽到 `minix-boot::MemoryMapSource` trait、把 boot-shim/kernel-image 依赖方向钉死，届时重写这段一句话说得清；分两批会撞 §3.1 表格与 §3.3 依赖方向段的 diff 冲突。与 I-23 也可并行——I-23 讲 §3.7 三形态引导矩阵，与本条讲 §3.1 被否表的两行分界同属「三形态引导」叙事族，同批落地共用一小段讨论最省。**解除条件（DoD）**：三案任一落地 + ①②③④ 四位置同步 + 与 I-22/I-23 相互引用不打架；主句仍留「同一个二进制的两个入口」与 L1410 依赖方向段冲突 = 视为未修；甲案拆行但② §3.3 未给到「新行直接可逆期」的指针 = 教学半成品。**现算命令（复核用）**：`grep -nE '内核自带 stub\|不在否决之列\|同一个二进制的两个入口' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（当前 1 处 = L1310；方案落地后应重新读，甲案会有 2 行独立描述）；`grep -nE '被"否决"的三种替代路径\|被"否决"的替代路径' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（当前 1 处 L1304 “三种”字样，甲案后需同步为“四种”或删数量词） |
| I-21 | 篇头「路径约定」/「标注约定」两段是否上收为只说一次 | ✅ **已上收（2026-11-01，I21HOIST 批，用户授权关闭）**：等 W-10/W-11 的七篇串行迭代冻结后再议——那一轮本身就在动头部，同一批里再动这两段会让 diff 分不清是谁改的。**现状读数（2026-10-15 现算）**：`> **路径约定**` 本 stage **13 篇各写一遍**（01/02/03/04/05/06/08/09/10/11/12/13/14），全 `rewrite-notes/` 树 14 篇 → **真冗余**。`> **标注约定**` **仅 01 篇独有**，所以"标注约定跨篇重复"这句**不成立**；但同一件事（`[ARCH: …]` 三档术语的解释）在另 3 篇以 `> **三档术语**` 标签出现，即**4 篇说同一件事、用了两套标签名**——真问题是"重复 + 命名漂移"，不是单纯重复。**候选**：① 两段上收到 `rewrite-notes/README.md`（或 stage 级说明），各篇头部留一句指针；② 只上收「路径约定」，`[ARCH]` 解释统一到一套标签名（两名并存必择一）；③ 全删——**不采**，因新读者首次遇到 `[ARCH: …]` 与锚点约定时无解码处，与"记号首次出现处要给定义"的既有裁决相冲。**连带耦合（改时必须一起动）**：`prompt/WORKFLOW-OPEN-ITEMS.md` 里 W-10 的《目标形态与预演读数》头部结果句、与《联合落地工序表》S5 子步 6 都写着"引用块从现有的 `> **路径约定**` 起"——若上收，这两处措辞必须同批改，否则工序表与现实脱节。**门依赖：无**（`tools/` 与 `prompt/review-rules/` 里的"路径约定"指的是 §2.4c/模式 74 那条 `os/` 前缀一致性检查，不读这段声明）。现算命令：`grep -lE '^> \*\*路径约定\*\*' rewrite-notes/01-stage-kernel/*.md \| wc -l` 得 13；`grep -lE '^> \*\*(标注约定\|三档术语)\*\*' rewrite-notes/01-stage-kernel/*.md \| wc -l` 得 4。**上收落地（I21HOIST）**：两约定全局化入 `rewrite-notes/README.md`「全局阅读约定」节（含 `三档术语` 一词的解码指向）；13 篇篇头改一句指针（解码处指向 README，候选①形态）；`三档术语` 逐篇块为**本篇三档应用陈述**（本篇属 Rewrite/哪些 [ARCH] 标签），非全局约定重复，保留；W-10 两处措辞连带同批改（WORKFLOW-OPEN-ITEMS.md）。`grep -lE '^> \*\*路径约定\*\*'` 现算 = 0 |
| I-25 | os 全仓代码注释语言与自包含性整改（中文混杂、协作锚点无落点） | ✅ 全仓完成（注释 CJK=0、广义锚点=0，1,549 个 `.rs`；654 个纯注释文件非注释 token 不变 + `SimPaging` 按 D-59 补齐）；审计 `tmp/i25-final-audit.md`，workspace 8,284 passed/0 failed、`run_all` 39/0/0。原登记：⏳ open（2026-10-19 用户登记，**低优**，不阻塞当前文档建设与代码 todo-fix 主线）：**问题（示例 `os/boot-shim/src/main.rs:74-78`）**——①中英文混杂：os 下代码注释按 review 规则应为全英文，现存在成段中文注释（同型还有 `uefi_helpers.rs` D-64②/F8 段、`bootface.rs` 头注、`opensbi_helpers.rs` 部分段等，仅已知样例、未穷举）；②注释不自包含：注释带 `NK4-A`、`D-64②`、`fix27`、`bn9m`、`甲案` 这类协作/worklog 锚点，但锚点本体的落点（哪个工作日志、哪份裁决文件）在注释里或仓内均不可达——读代码的人无法顺着锚点找到背景，注释给出的"知识或注意点"缺了证据链。**整改口径（两选一，落地时裁决）**：a. 锚点处补齐仓内可达路径（如 `rewrite-notes/.../worklog-xxx.md#nk4-a`）；b. 删锚点、把必要背景写进注释本身，使注释自包含自解释。**范围**：落地时对 `os/` 全仓扫描（不止 main.rs 一处），逐处按 ①② 判定；中文注释改英文，锚点按 a/b 处理。**触发窗口**：后续 todo-fix 轮；不与 I-22/I-23/I-24 的在制文件抢同批。**解除条件（DoD）**：全仓扫描记录落盘（每处：文件：行号、问题分类①或②、处理结果）；os 下代码注释无成段中文、协作锚点全部仓内可达或已内联为自包含背景。**现算命令（复核用）**：`grep -rnE '[一-龥]{4,}' os --include='*.rs' \| grep -v '#\[cfg(test)\]' \| wc -l`（当前未知，落地前现算基线）；锚点样例：`grep -rn 'NK4-A\|D-64\|fix27\|甲案' os --include='*.rs'` |
| I-20 | `os/boot-shim/src/lib.rs` 的再导出注释与实际调用点不符 | ✅ 已闭合（提交 b0da9074e 采修法一：注释按实况改写「Re-exports the active firmware implementation's type — callers still write the concrete type」；2026-10-19 复核：清掉该笔遗留的旧句残段与 emit_byte 行尾误粘的重复注释。原发现 2026-10-13：注释写"allows callers to write `boot_shim::prepare_boot(...)` without knowing which firmware is active"，但它再导出的是**类型**（`pub use uefi_helpers::{find_platform_sources, UefiBootShim}`），实况是调用方仍写具体类型（`os/boot-shim/src/main.rs` 的 `UefiBootShim::prepare_boot(1024)`）。修法二选一：改注释按实况（"再导出活动实现的类型"），或真加一层 `pub fn prepare_boot(...)` 转发让注释成立。重跑：`grep -n "cfg(feature\|pub use" os/boot-shim/src/lib.rs` 与 `grep -n "prepare_boot(" os/boot-shim/src/main.rs` |
| I-17 | 01 篇 §2.6 拆节（`alloc_lowest` 缺陷分析、静态页表池两段独立成 §2.6.3） | ✅ 已修（01TODO-DOC：§2.6 内部拆五节，最大节 176 行；顶层号不动以保外部历史锚点；`alloc_lowest` 段现居 §2.4.1 不随移）。原登记：⏳ open（2026-10-12 01 篇人审轮登记）：纯移动不删内容；触发窗口＝下轮 structure 复核或审到 §2.6。现量（2026-10-13 现算）：§2.6 347 行占本章 31%（重跑：`awk '/^### 2\.6 /,/^### 2\.7 /' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \| wc -l`；同法得 §2 总行 1125） |
| I-18 | 01 篇 §1、§3–§6 的章头与巨节结构体检 | ✅ 已修（01TODO-DOC：§4/§6 补章头导语；≥300 行巨节仅 §2.6，已由 I-17 拆；§2.4 287 行未达阈值记录不改）。原登记：⏳ open（同上轮登记）：查两件事——章头是否同样零导语、是否有 ≥300 行的单节。§2 的基线（2026-10-13 现算）：补导语前章头仅 2 行＝无导语；补后全章 1125 行，最大三节占 70%（2.6 计 347 行、2.4 计 286 行、2.3 计 158 行）；已按建议补章级导语与 2.4/2.6 的"本节结算"句，其余章未动 |
| I-19 | §3–§6 讲解条目续扫"只报事实不报道理" | ✅ 已修（01TODO-DOC：§3–§6 讲解条 ~90 条续扫、修 4 条纯事实条目）。原登记：⏳ open（同上轮登记）：判据＝事实陈述后必须回答"为何必须这样、不这样会怎样"，纯细节堆叠（构建改名、符号别名、常量罗列）改为一句话指向 owning 节＋补回道理。§2 已扫 40 条、修 4 条（§2.5/§2.6 三条＋§2.8 拟人句）；§3–§6 约 90 条待过 |
| I-1 | kernel 独立 ELF 构建接入（build.rs + link.ld → xtask） | ✅ 已闭合（2026-10-30，01TODO-T2X86）：入口交接 ABI 已裁并落地（v1 arena；x86 `rbx`、aarch64 `x0`），boot-shim 终局跳转与 `kernel-image` bin/build.rs 均在树，x86 与 aarch64 两条 UEFI 链已走独立镜像；`run_all` 39/0/0。原登记：⏸ 诚实 DEFERRED（2026-09-09）：三架构 link.ld 就绪（`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`）但 artifact 无引导路径——`KernelLoadResult.entry_point`（`os/boot-shim/src/loader.rs:66`）零消费者，boot-shim→kernel.elf 终局跳转未设计（入口 ABI：e_entry 处接收什么参数/寄存器态无定义）。解除前置：(a) 入口交接 ABI 设计；(b) boot-shim 终局跳转（S-4/S-8 相邻）；(c) bin 目标 + build.rs（先例：hello-boot-riscv64/build.rs）。单独做 (c) = 不可引导 artifact（frame.rs 先例） |
| I-5 | ACPI RSDP 搜索与表解析 | ⏸ 诚实 DEFERRED（2026-09-09）：三家族 QEMU virt 均不依赖 ACPI（平台发现走 KernelInfo+DTB），内核无 RSDP 消费方；MADT 常量骨架在位（`os/arch/src/acpi.rs`）。解除条件：物理机 x86_64 支持（真实 MADT/HPET 消费方）时实现 RSDP 扫描（EBDA + 0xE0000-0xFFFFF） |
| I-6 | `bill_ptr` 完整联动 + 真实调度主循环 | ✅ 全条闭合（§23.3 ①②③ 于 `2026-09-15` 落地；旧记"调度循环为 placeholder"过期——`os/kernel/src/lib.rs:fn scheduler_loop` 为真实循环，kmain Phase F 直入（`lib.rs:3909`），`bill_ptr` 联动 `set_bill_to_idle`/`pick_and_bill` 到位；端到端实证 test-smp-aps（四核 idle/pick 多轮）+ test-smp-ipi。核实：`grep -n "fn scheduler_loop" os/kernel/src/lib.rs` |
| I-13 | ~~`InterruptController` trait 拆分~~ ✅ 已闭合（2026-09-15，code-excellence 轮）：拆为 `InterruptRouter`（new/init/mask/unmask/mask_all，全局路由面）+ `PerCpuInterruptUnit`（claim/complete，去死参 irq，per-CPU 语义），IrqManager 约束 IC: Router + PerCpuUnit；GIC/PLIC 的 `claim → IrqVector` 返回值演进记 OQ-13a（随 per-CPU 分发 lane） | ✅ |
| I-7 / I-9 / I-10 | MF_REPLY_PEND typestate / NonNull / PrivId newtype | ✅ 评估闭合（2026-09-08）：均维持现状——typestate 增益在持久化边界消失；`s_stack_guard` 是地址值无 deref 目标、filter 链本体未实现（虚构抽象不建）；PrivId newtype 需改 81 处构造点且无混用 bug 史例，**跟进顺序：D1 先例建立后批量评估**（若未来评估，从 I-10 重启） |
| I-2 | `release[]`/`version[]` + MINIX_KERNINFO | ✅ 内核臂闭环（2026-09-16，edge E-KERNINFO，commit 207e30644）：minix-types kerninfo wire（88B MinixKerninfo/KuserInfo + OS_* 常量）+ `IpcCall::KernInfo=6` dispatch 臂（未发布→EBADCALL/已发布→二次返回通道 RBX + OK）+ `set_secondary_ipc_return`（C arch_system.c:184-186 等价物）；页映射初始化与用户态消费挂 E1 通电。**✅ 内核半闭环（2026-09-17，edge E-KERNINFO）**：`kerninfo::init_kerninfo`（kmain Phase B.5）构建内核镜像静态页（magic + kuserinfo=kui_size/user_sp，main.c:438-440 对应）→ 活动根 query 翻译 PA → `Paging::map` 用户只读映射到固定 `KERNINFO_USER_VA=0x2_0000_0000` → 发布 `MINIX_KERNINFO_USER`；同轮修复 `walk_alloc` 中间层不传播 USER 位的 arch 级缺陷（U/S 逐层 AND，CPL3 必 #PF——VmBootHandoff 同路径潜伏缺陷一并消除）；test-user-trap 真机验收：round-2 返回 OK + RBX=页 VA + CPL3 读到 KERNINFO_MAGIC，脚本五断言 PASS。用户态消费半（minix-rt query_kerninfo 翻转）仍挂 E1/T2 | ✅ |

### 7.5 文档条目 ↔ 架构建议关联（保留防重查）

13 §6.5=§D1；13 §6.2 BklGuard RAII 已拒绝→§B1 transfer 承接；16 §4.13 BKL 接入→§A1/§B1
（D-38①余）；22 D9 裸指针→I-9 维持论证；24 D-25 部分进度→无消费方不建。

### 7.6 已解决排除表（grep 验证后不入清单，防重查）

11 `notify_scheduler`（proc_table.rs 完整实现）；12 `copy_msg_from_user`（UserCopy trait）；
07 syscall_copy 6 stub 全实现；20 §4.8 DEFERRED 全实现；13 §6.1 kernel_call_resume 时机修复；
05 Timer queue 测试已覆盖；16 cache 对齐 FIXME=C 同款非缺口；23 IPC_STATUS_* 已实现；
06 `EntrySpec::DEFERRED`=设计语义；08 三处 C 步骤差异=ARCH 演进；27 kmess=W-7。

### 7.7 跨阶段转移清单（原 §H 结论，均已落地）

页表页分配器 VM 接入 + minix-vm clippy → 02-stage-vm；init/load 测试 + init_ap + QEMU GDB CI +
ptproc per-CPU → 16-smp（=本表 D-40/T-2~T-5）；C-D-1~5 → rewrite-notes/misc/legacy-notes-todo.md（其中 C-D-5 已随 D3 闭合）；
QEMU E2E → rewrite-notes/misc/legacy-notes-todo.md QEMU backlog。原始 18 节全文见 git `97df4e58d^`。

## 8. GPT 评论分析结论（原 §18.5/§19/§20，压缩为结论）

- **§18.5 cause_sig backlog**：BL-1/BL-2/BL-3 全部闭合（D-57/D-13 落地、D-14 no-op）。
- **§19 doc 05 时钟评论（GPT 8/10）**：P1 ack API 与 P2 mask_all 改名并入 I-13 待 S-6；
  P2-3 DEFAULT_HZ 并入 OQ-15-1；P3 ClockArch 拆三 trait 不做（C-Rust 1:1 对应优先）；
  **第一项先决（`ClockArch::init_timer` 过早使能 timer IRQ）——V13 复核仍未修**（`os/arch/src/arm64/clock.rs:69`
  仍 `msr cntp_ctl_el0, 1` Enable=1/IMASK=0；`os/arch/src/riscv64/clock.rs:84` 仍 `csrs sie, 0x20`），
  C ground truth 是 `bsp_finish_booting`（main.c:324）才经 `boot_cpu_init_timer` 使能；Rust 在
  `init_clock_and_interrupts` 即 live 而 handler 链（D-46 硬件半环/S-8）未接 → **升格为 §23 正式条目 D-59**。
- **§20 doc 06 评论（GPT 8/10）**：P1 两项措辞修正已落（FIX-06-GPT-1/1b/2/2b/2c，"唯一组合"与
  "BSS"表述，三轮穷尽扫描）；5 项设计债保留如下（原编号 D-52~56 与 §7.1 D 系冲突，重编号 GD）：

| ID | 设计债 | 触发条件 |
|----|--------|---------|
| GD-1 | ProcKind 拆分（ProcessRole × ExecutionDomain × PrivilegePolicy） | KernelTask 下出现 interrupt-driven / kernel coroutine / schedulable kernel thread 三情形 |
| GD-2 | EntrySpec 演进为 state enum（Kernel/Deferred/Ready） | 出现 Option 表达不了的第三种 entry 状态 |
| GD-3 | enable_user_io 从 CpuContextArch 拆出（ArchPrivilege trait） | trait 方法 >8 或边界频繁调整（现 6 方法） |
| GD-4 | Process vs ExecutionContext 概念拆分 | 引入 coroutine/微线程等新内核执行流类型 |
| GD-5 | CapabilityTemplate 重命名 | 引入运行时 capability 机制 |

## 9. Open Questions 存档

### OQ-15-1 DEFAULT_HZ 三方不一致 [✅ 已裁决（2026-09-09 V13 终批）：维持 100]

C（ground truth）i386=60 / earm=1000（`minix3/minix/include/arch/i386/include/archconst.h:4`）；
Rust=100（`os/kernel/src/clock.rs` + `os/arch/src/arch/clock.rs`）；doc 15 §2.1/§3.7 D7 已标注。
影响：无 `hz` 覆盖参数时 tick 频率/时间片与 C i386 不同（行为差异）。已做：差异三处标注。
选项：A. 保留 100（现状，boot 时可 `with_hz(60)` 对齐）；B. 改 60（影响依赖 100 的测试与
时间片计算）。关联：跨 crate 双定义的"权威位置"问题（与 G1 同型，若合并建议随 minix-types
config 层收敛）。

**裁决：选 A（保留 100）**——C 自身即两值并存（i386=60 / earm=1000），不存在唯一
要对齐的 C 值；差异已按 [ARCH: K-3] 三层标注（doc 05 + design + code，V13 复核一致）；
切 60 将作废全部按 100 调优的测试与负载采样窗；需要 C i386 行为时可 boot 传 `with_hz(60)`。
跨 crate 权威位置：维持现状（`clock.rs` 常量 + doc 标注），不新增 config 层。

### OQ-D49-1（已关闭）/ D19-1（已实现）

D-58 方案 B 落地后关闭（2026-09-06）；D19-1（cause_signal 致命 SELF 子路径）随 D-11 落地（2026-09-05）。

## 10. Edge Items：SMP/硬件阻塞残余（2026-09-06 全面扫描，V13 复核仍成立）

> 全部需要 SMP bring-up 或裸机/QEMU 基建，hosted CI 无法诚实验证。步骤编号见 smp_todo.md §5。

| ID | 项 | 阻塞 |
|---|---|---|
| D-36 | ~~smp_init（ACPI/MADT + boot_ap）~~ ✅ S-5（`2026-09-15`，见 §12.2；硬件清单走 KernelInfo+DTB 非 ACPI） | S-4/S-5 ✅ |
| D-37 | ~~boot_lock~~ ✅ S-5（smp.rs boot_lock 单例 + AP finish handshake） | 随 D-36 ✅ |
| D-38① | ~~exception_dispatcher BKL~~ ✅（trap_dispatch.rs 入口 `bkl_lock_or_inherit()`，与 §7.1 同形回勾） | S-9 ✅ |
| D-39 | ~~x86_64 init_ap panic 占位~~ ✅ S-4（`2026-09-14` 真实现，见 §7.1/smp_todo §22） | S-4 ✅ |
| D-40 | ~~ptproc per-CPU（CpuLocal）~~ ✅ S-6.1 | S-6a ✅ |
| D-41 | ~~PLATFORM SMP 容器语义~~ ✅ S-6.2（Frozen<T>） | S-6b ✅ |
| sched-1 | ~~per-CPU scheduler running 指针~~ ✅ S-6.3（冻结决策：队列共享+BKL；per-CPU 只 proc_ptr） | S-6c ✅ |
| tick-1 | ~~per-CPU kernel-tick 统计~~ ✅ S-6.4（CpuLocal.tsc_per_state 五桶 + GET_CPU_TICKS 转真） | S-6d ✅ |
| hw-1 | ~~D-46 硬件半环（asm IRQ stub + IDT load + 入口分流）~~ ✅ S-8（2026-09-14，test-timer-irq PASS 全链实证） | S-8 ✅ |
| hw-2 | ~~D-48 shutdown(0)~~ ✅ S-11（minix_shutdown 语义层 + 三架构后端；test-smp-shutdown PASS） | S-11 ✅ |
| trap-1 | ~~trap 入口接线 kernel_call_dispatch~~ ✅ S-8（B 体 asm→kernel_call 已接；disjoint-API 重构登记为 S-6 阻塞项） | S-8 ✅ |

非 SMP 的代码级 TODO 残余：ipc.rs NOTIFY 权限路径已改设计决策注释；lib.rs TODO-01-1 为
boot-shim 历史项已有 doc。

## 11. V12 轮 Rust 全量扫描结果（2026-09-07；8/8 条目全部闭环）

| ID | 严重度 | 结论 |
|----|--------|------|
| V12-A1 | P1 | ✅ `TrapStyle` 枚举（`os/arch/src/arch/trap_style.rs`，保留 C KTS_* 编号）+ `KProcess.trap_style` 字段 + sigsend 前置 EINVAL 校验 + **MINIX3 BUG 修复**（C arch_system.c:597 用户可控 trap_style 可 panic，Rust 校验先行）+ finish_and_restore 返回闸门分流；KernTrapStyle 并入（2026-09-07） |
| V12-A2 | P2 | ✅ `caller_q_find` 泛型化（accept 闭包插在 C CANRECEIVE 位）+ `el_match` 归一——单一真相源，测试测的就是生产路径（2026-09-07） |
| V12-A3 | P2 | ✅ senda notify 同分支合并为单布尔表达式（C proc.c:1301-1305 锚点）（2026-09-07） |
| V12-A4 | P2 | ✅ boot 回收重验：add_memmap 启动链接线本已落地（lib.rs bootstrap 守卫段），旧表 5 行"❌ 缺失"断言全部改写（Pattern #70 实例）（2026-09-07） |
| V12-B1 | P1 | ✅ proc_table.rs 17 处 panic 审计：16 处内部不变量维持 panic（C assert parity，14 处已有 R-15 注释）+ 2 处补契约注释 + 1 处分类纠偏（SYSTEM→VM 唤醒吞错改 C-parity panic）；无用户可达 panic（2026-09-07） |
| V12-B2 | P2 | ✅ unsafe 审计抓到真 UB：`do_vdevio`/`do_sdevio` 批量 I/O 缓冲对齐 1 → `slice::from_raw_parts` 转型对齐 4/2 失配（12 处 cast）——`IoBatchBuf`（`#[repr(C, align(8))]`）构造性修复 + paging.rs 两处形态修复（arch clippy 2→0）+ misc.rs union 读取补论证（2026-09-07） |
| V12-B3 | P3 | ✅ 实测关闭：`Message` = 72 字节（union 64 + 头 8，pin 进测试），每 syscall 至多 ~144B 瞬时栈占用，距 512B 阈值余量 3.5 倍（2026-09-07） |
| V12-B4 | P3 | ✅ clippy kernel 16→2（10 处机械修复；余 2 处为已登记不修：push_exclusion 特性门控误报、too_many_arguments=C-D-2）；arch 2→0；MADT 跨架构误报记录防重开（2026-09-07） |

**§21.3 非缺口备忘（防重查，保留）**：① `announce`/`prepare_shutdown`/`is_fpu`/`env_get("hz")`/
`cut_memmap`/`add_memmap` 均有实现（接线仅 V12-A4 一条且已闭合）；② debug.c 打印族已实现
（runqueues_ok_cpu/rtsflagstr/miscflagstr/print_proc），IPC 统计 hook 属 W-3；③ profile.c 经
dispatch_profile 覆盖（nmi 臂 W-4），watchdog.c 全族 W-1，usermapped_data.c W-2；④ 跨 stage
判定：V12 轮未发现新的跨 stage 生产代码缺口（微内核边界下 stage 内消费稳定契约即可独立实现）。

## 12. 全量收敛执行计划（§22 压缩版）

> 逐迭代串行：讲明白（grep 证据）→ 多方案对比 → 实施（代码+文档+测试）→ 测试全绿 →
> 8 公共门回归 → 打勾 + commit。EDGE 判定裁定（2026-09-07）：仅三类进 edge_todo.md——
> ①共享契约层的契约性变更（minix-types wire ABI / minix-sys trap 层 / os/arch 跨 stage 基建）、
> ②其他 stage 目录内生产代码、③多进程联调集成测试；共享库中纯增量、仅内核消费的类型改进
> （D1/G1）不属 edge。

### 12.0 环境基线（2026-09-07 实测；V13 待复测）

`cargo test -p minix-kernel` 691→729+3（含迭代增量）；QEMU 四模拟器齐备；三 production targets
就绪；rustc 1.94.1；Phase 边界回归 = 全 workspace cargo test + `os/qemu-tests/run_all.sh`。

### 12.1 Phase 0/1/2/4/6（✅ 全部完成，压缩为 §0.1 批次表 + §1/§11 条目行）

跨会话教训（保留）：并行会话共享工作区——提交前 `git status` 核查暂存区；在途编译错误阻塞
依赖链验证；minix-types Message union 64→72B 增长被 V12-B3 pin 测试当场拦截（pin 随实测更新）；
`RUST_MIN_STACK=8388608` 已入 `.cargo/config.toml`（深帧链击穿 2MiB 默认栈）。

### 12.2 Phase 3 — SMP 主线（smp_todo.md §5 为权威，此处只留状态索引）

S-0/S-1/S-2/S-2b/S-3a ✅；**S-3b ✅（2026-09-08，commit e41929cf2——本表原 ⬜ 为陈旧状态，
smp_todo.md §5 为准）**；S-3c ✅（PSCI 0xC4000003 / riscv IPI EID 0x735049 / x86 mfence 三真
bug 修复）；**S-3d ✅（2026-09-14 第四会话收官：test-smp-ap-alive-mb PASS、-d int 0 异常——
真因 lgdt 16 位寻址 modrm 0x15=[DI] 读 IVT 垃圾，修复为 0x16=disp16，另修阶梯尾部三处
记录字段绝对读字面量笔误；见 smp_todo.md 第十~十二轮记录 + 本文件 §25 终局段）。**

| 步骤 | 内容 | 依赖 |
|------|------|------|
| S-8 | ✅ asm trap stub + SYSCALL 入口（2026-09-14 收官——A/B 路径 + Win64 ABI/ISA override/LVT 屏蔽三真 bug + test-timer-irq PASS；见 smp_todo.md §21 完成记录） | S-3d ✅ |
| S-4 | ✅ init_ap 真实现（2026-09-14——16 字节 TSS 描述符入 GDT + IST 栈 + GS 双 MSR + per-CPU SYSCALL MSR + L2 读回 PASS；x86 PIT 全局无 per-AP 时钟工作；见 smp_todo §22） | S-8 ✅（D-39 ✅） |
| S-5 | ✅ smp_init 编排 + boot_lock（2026-09-15 全步收官——内核层 + 硬件 L4 四核全在线；含 PROTECTION/TRAP_ENTRY 驻留生命周期修复与三真 bug；见 smp_todo §23） | S-4 ✅ |
| S-6a~d | ✅ per-CPU ptproc / PLATFORM 冻结语义 / 调度 running 指针 / tick 统计（2026-09-15 四件独立 commit；见 smp_todo §24） | S-5 ✅ |
| S-7 | ✅ AP 主循环（共享 `scheduler_loop`，前置由 L5 测试覆盖——S-10/S-12 行早已引用 S-7 ✅，本行漏翻，`2026-10-09` 回勾） | S-6 ✅ |
| S-9 | ✅ 异常入口 BKL（D-38①，`trap_dispatch.rs:105` 入口 `bkl_lock_or_inherit()`——§7.1 同条目已记闭合，本行漏翻，`2026-10-09` 回勾） | S-8 ✅ |
| S-10 | ✅ IPI 往返（2026-09-15 test-smp-ipi PASS——0xF0 路由 + per-CPU LAPIC 使能 + load_ap；riscv SSIE 路径随 lane） | S-7 ✅ |
| S-11 | ✅ shutdown(0)（2026-09-15 minix_shutdown 语义层 + 三架构后端 + isa-debug-exit 设备；test-smp-shutdown PASS） | S-10 ✅ |
| S-12 | ✅ 测试债 + CI 化（T-2 hosted ≥7 行为测试/T-3 覆盖互查+assert/T-4 硬件路径验证/T-5 qemu-tests.yml + run_all 一键 27 测试） | S-7 ✅ |
| S-13 | ✅ 收尾 sweep（Edge 表 SMP 块迁移完成、smp_todo §21-26 记录、全阶梯重跑） | 全部 ✅ |

### 12.3 Phase 5/7 — 仍 open 的主线项

- ✅ **I-6** bill_ptr + 真实调度主循环——已全条闭合（§23.3 ①②③，旧记"替换 placeholder"系过期；核实：`grep -n "fn scheduler_loop" os/kernel/src/lib.rs`）
- ⬜ **I-13** InterruptController trait 拆分（依赖 S-6）
- 🔄 **profiling/sprofiling deferred 体**（misc.rs dispatch_sprofile 已全实现；剩余 = trap 入口接线
  传 PC，依赖 S-8/S-9；NMI 臂 ENOSYS 与 idle 轮询变体两处 documented 豁免）
- ⏸ I-1 / I-5 / T-10（解除条件见 §7.3/§7.4）
- ✅ SYS_PADCONF / syscall.rs:1013 / dispatch_clear 缺口盘点全部闭合（详见 git 893386cd8 版 Phase 5）

## 23. V13 轮：查漏补缺 + 架构深审（2026-09-09 扫描完成）

> 范围：`os/kernel` + `os/arch` + `os/plat` + `os/boot-shim` + 共享层内核消费面，方法与
> 定位：① Gate A 机器覆盖（SYMBOLS.md 已落 `.review/claude/fork-syscall-rewrite/stage01-rescan-20260909/`）
> ＋ 2026-08-12 后 91 个增量提交的语义对账；② 分层架构深审（共享层 → HAL → kernel 核心 → boot 链），
> 联网对照 Redox/Linux/OS 理论；③ 设计深度检查（translate 残留、BKL 纪律、借用结构）。
> 本轮只登记不改代码。详细证据链见同目录 scan.md。

### 23.1 覆盖结论（先说查漏结果）

- **38/38 个 `do_*.c` 全部有语义对应实现**（syscall.rs 分派 46 项覆盖 C call_vec 全部条目，
  11 个空洞与 C 同为 WONTFIX），本轮增量（U-1/U-2/do_sprofile）均对齐；**无疑似缺口**。
- 顶层内核文件（proc/system/clock/main/interrupt/table/smp/debug/utility）函数级对账：
  仅 2 条轻微项（入 D-65）；send_diag_sig/get_value 等落在 W-7/设计演进范围。
- include 契约抽查 12 项（端点常量/SYS 调用号/VMSUSPEND/NR_BOOT_PROCS/VmBootHandoff）：
  **零值漂移**。
- 标注体系反向覆盖：MINIX3 BUG 全库 7 处（6 处锚点精确，1 处 C 行号 ~5 行漂移→D-60）；
  `[ARCH]` 6 处三层一致（krandom D-33 的 design 快照层滞后一行→D-65）。
- **无新增 edge 条目**：本轮未发现跨 stage 生产代码缺口，V12 §21.3④ 结论维持
  （微内核边界下 stage 内消费稳定契约即可独立实现）。

### 23.2 新发现 P1（两项均为 S-8 前置必修的休眠缺口——现状无实害是因为中断从未真正开启）

#### D-59 timer IRQ 链：过早使能 + 使能抽象零调用 + 路由错线（复合） [P1] — ✅ 已修复（2026-09-09，V13 首个 todo-fix）

**问题**（五点，均实测锚点）：
1. **配置与使能合一步且提前**：arm64 `os/arch/src/arm64/clock.rs:69` 在 `init_timer` 内直接
   `msr cntp_ctl_el0, 1`（Enable=1/IMASK=0）；riscv64 `os/arch/src/riscv64/clock.rs:84` 直接
   `csrs sie, 0x20`——均在 Phase B `init_clock_and_interrupts`（`os/kernel/src/lib.rs:808` 调用点）
   即 armed；x86_64 `os/arch/src/x86_64/clock.rs:57-81` 是纯配置（只编程 PIT，LVT 保持 masked）。
   C ground truth：`minix3/minix/kernel/clock.c:48-66` init_clock 纯软件，硬件编程+handler 注册
   相邻且在 boot 最后一刻（`main.c:73` bsp_finish_booting 末段 → clock.c:294-304）。
2. **使能抽象已建但零调用**：`TimerIrqGate` trait（`os/arch/src/arch/timer_irq_gate.rs:62-78`）
   三架构 enable/disable 实现齐备，但生产 kernel **零调用**（`os/kernel/src/lib.rs:1902-1917`
   仅注释提及）——"配置→使能分离"的正确抽象落地后被旧通道（init_timer 内联使能）绕过。
3. **"no-op safety net" 名不副实**：`os/kernel/src/lib.rs:1875-1882` 在 bsp_finish_booting 里
   再调一次 init_timer，注释称 no-op；实际 arm64/riscv64 语义是重置比较器推后首个 deadline
   （重 armed，非 no-op）。
4. **riscv64 双重使能**：`os/arch/src/riscv64/arch_init.rs:54` 再写 `sie=0x22`，STIE 被置两次。
5. **IrqVector(0) 路由三架构不同**：`os/kernel/src/lib.rs:1893` 的 `register_hook(IrqVector(0))`
   + unmask 在 x86_64 正确（IOAPIC IRQ0=PIT）；arm64 上 unmask(0) 使能的是 SGI 0 而非 timer
   PPI 29（`os/plat/src/arm64/interrupt.rs:162-179` 写 GICR_ISENABLER0 bit 0）；riscv64 上
   unmask(0) 直接 return（`os/plat/src/riscv64/interrupt.rs:113-116`，PLIC 无 0 号源）——
   即 arm64/riscv64 的 timer IRQ 投递路径根本没接对。

**影响**：当前休眠（三架构 CPU 级中断掩码全程关闭，无 `sti`/daifclr/sstatus.SIE 生产路径，
S-8 前不会触发）。S-8 trap 入口落地、首次开中断的瞬间：Phase B 埋下的 deadline 早已过期
（首个 unmask 即收到积压 tick）；arm64/riscv64 则连投递线都没接对（D-46 时钟中断在这两个
架构上静默失效）。

**修法方向（多方案对比）**：
- 方案 A（推荐，对齐 C 时序）：init_timer 全架构改 configure-only（arm64 写 CNTP_CTL=2
  即 Enable=0/IMASK=1；riscv 写 mtimecmp 不写 sie.STIE；x86_64 维持现状）；TimerIrqGate
  成为唯一使能入口；bsp_finish_booting 的 re-call 删除或改语义化命名；hook 注册改为
  按架构正确的 IRQ 号（x86 IRQ0 / arm64 PPI 29 / riscv 将 timer 直连 sie 不经 PLIC——
  需 irq hook 层支持"非控制器路由"的中断源）。
- 方案 B（最小改动）：仅修 arm64/riscv64 使能时点后移到 bsp_finish_booting，IrqVector(0)
  路由修对；TimerIrqGate 维持零调用并删除（避免双通道）。
- 方案 C（保守）：全部推迟到 S-8 批次一并做（与 asm trap 入口同批验收）。
- 对照：Linux clockevent 把 clockevent device 的 set_state_* 与 irq enable 分离，
  enable 在 late_time_init 阶段（`start_kernel` 末尾）——与方案 A 同型；C Minix3 的
  "编程+注册相邻且最后" 是同一原则。
**裁决建议**：随 S-8 批次实施（方案 A 为目标形态）；S-8 前任何"开中断"代码不得合入。

**✅ 已修复（2026-09-09，方案 A 落地）**：
- **Phase B 纯软件化**：`init_clock_and_interrupts` 删除 `init_timer` 调用（C init_clock clock.c:48-66 纯软件 parity）；"Order matters" 注释重写（删除"arch_init 使能 LVT"虚构声称）。
- **Step 6 重建为 C boot_cpu_init_timer 完整序列**（lib.rs）：`init_timer` 只编程（arm64 `CNTP_CTL=0x2` 保持门闭；riscv64 不碰 `sie.STIE`）→ `register_hook(minix_plat::TIMER_IRQ, ...)`（首 handler 自动 unmask = C interrupt.c:65 parity）→ `<CurrentTimerIrqGate>::enable_timer_irq()` 开本地门；"no-op safety net" 重复调用与误导性 Behavior-change 块删除。
- **`minix_plat::TIMER_IRQ`**（新常量，per-arch 定义 + 根 cfg 选择）：x86_64=0（PIT→IOAPIC 0）、aarch64=30（CNTP NS PPI，CNTPNSIRQ）、riscv64=0（伪向量，PLIC mask/unmask 对 0 恒 no-op 即"无控制器线"的语义化）；三架构各带硬件事实 pin 测试。
- **x86_64 gate 语义修正 `[ARCH: gate-semantics]`**：enable/disable 改文档化 no-op（PIT 无本地门；旧实现清未编程 LVT = 打开无 handler 中断源的陷阱），LVT 门随未来 LAPIC 时钟源批次回归；trait 模块文档同步（调用时序 = 晚于编程与注册）。
- **riscv64 arch_init** 删除提前 `csrs sie, 0x22`（STIE 归门控、SSIE 归 S-7/S-10 IPI）。
- 文档同步：doc 05 §3.7（行为变更声明表标注已修复 + D-59 解决记录 5 条 + 调用时序约束改写 + 调用顺序段改写）、doc 08（Step 6 流程行/不变量/代码块三处）。
- 验证：kernel 729 passed / 0 failed / 8 ignored；arch 223 passed；plat 3 passed（含 3 个新 pin 测试）；三架构 production-target check exit 0；clippy 零新增。QEMU 回归见 commit 前记录。
- **附带发现（不属本条顺手修，登记为 T-13）**：`arch/src/arch/dm_coverage.rs` driver 测试族共享全局 mock 注册表无同步，并行调度下确定性失败（单线程 10/10 全过）——预存在的测试隔离缺陷，与本次改动无关。

#### T-13 [P2] dm_coverage driver 测试族共享全局 mock 注册表无同步，并行运行确定性失败

`arch/src/arch/dm_coverage.rs` 的 `tests::driver` 子模块各测试共享 mock 全局注册表（`mock_dm_clear`/`mock_dm_leaves`），无锁。默认并行调度下 `test_establish_window_pa_limit` 与 `test_establish_skips_hole_and_offsets_va` 稳定失败（断言读到他人测试的叶项：期望 `min_pa=0x3000_0000` 实得 `0x1C000`）；`--test-threads=1` 下 10/10 全过。hosted 测试通常经 `os/.cargo/config.toml` 的 `RUST_TEST_THREADS=1` 单线程运行故未暴露（D-59 验证时从仓库根目录调用 cargo 漏读 config 才现形——调用目录教训已录 §12.1）。修法：mock 注册表加 `BKL_TEST_LOCK` 同型互斥（kernel misc.rs SPROF_TEST_LOCK 先例），或 driver 各测试用互不重叠的窗口基址。

#### D-61 InterruptController：生产 dispatch 从不 ack，arm64/riscv64 eoi 将以 0 值完成中断 [P1] — ✅ 已修复（2026-09-09，V13 第二个 todo-fix）

**问题**：
1. **ack 全仓库无生产调用方**：`os/kernel/src/irq_manager.rs:442-507` 的 dispatch 只调
   mask(:452)/handler/unmask(:501)/eoi(:504)；arm64 的 eoi 依赖 ack 捕获的 `last_iar`
   （`os/plat/src/arm64/interrupt.rs:182-194`）、riscv64 依赖 `last_claimed`
   （`os/plat/src/riscv64/interrupt.rs:127-141`）——真中断到来时这两个字段是初值 0，
   eoi 将以 0 写 ICC_EOIR1_EL1 / PLIC complete。GIC/PLIC 语义：ack（读 IAR/claim）取得
   INTID → 处理 → 用**同一 INTID** 写 EOIR/complete；以 0 完成不是合法完成
   （Redox aarch64 / Linux generic IRQ 层 / GIC 规范同此协议，2026-09-09 联网核实）——
   后果是活动中断永不关闭 → 中断风暴。x86_64 的 ack/eoi 忽略 `_irq` 直接 `lapic_eoi()`
   （`os/plat/src/x86_64/interrupt.rs:218-224`）语义正确。
2. **arm64 mask_all/init 只盖 SPI**：`os/plat/src/arm64/interrupt.rs:196-203` 循环
   `32..nr_irqs`，SGI/PPI（<32）从未被 disable（init :83-90 同）；"init 后全屏蔽"不变量
   对 SGI/PPI 不成立（timer PPI 29 保留 firmware 遗留状态）。trait 文档表
   `os/plat/src/interrupt.rs:126` 声称 "GICD_ICENABLER=all" 与实现不符。
3. 关联但独立于 I-13：I-13（trait 拆分 Router/Ack）是抽象债；本条是**正确性缺口**——
   S-8/S-9 接通中断入口时若沿用即引暴。

**修法方向**：dispatch 路径补 ack（向量分发前读 IAR/claim 取 INTID，再按 INTID 派发
hook——这同时修正 mask/unmask 的 IRQ 号来源）；或 eoi 改为携带 handler 层回传的 INTID。
arm64 init/mask_all 补 GICR_ICENABLER0 处理（SGI/PPI 全屏蔽 + 按需 unmask PPI）。
与 I-13 的 trait 拆分合并设计（ack 归 per-CPU Ack 面、mask/unmask 归 Router 面），
**但修复不等待 trait 重构**——S-8 批次内先以现 trait 修正确性。

**✅ 已修复（2026-09-09，claim/complete 语义方案落地）**：
- **dispatch 接入 claim**（`os/kernel/src/irq_manager.rs`）：控制器调用序列定型为
  **ack（claim）→ mask → handler 链 → unmask → eoi（complete）**——claim 在最前，因为
  eoi 写回的 INTID 必须来自本次 claim 的捕获；C parity：claim 在 C i386 位于 asm 入口
  （`irq_handle` 之前），EOI 在 asm 尾部。
- **x86 ack 修正为文档化 no-op**：旧实现写 LAPIC EOI——把"完成"塞进"取号"，dispatch 头部
  调用即提前放行同级中断；x86 无 claim 寄存器，EOI 是完整握手（trait 文档表与 trait 方法
  doc 同步，`os/plat/src/interrupt.rs`）。
- **arm64 mask_all 补 SGI/PPI**（`os/plat/src/arm64/interrupt.rs`）：GICD 只达 SPI（≥32），
  追加 banked `GICR_ICENABLER0 = all`；`init_redistributor` 在 WAKER 唤醒后同样屏蔽
  INTID 0-31（firmware 遗留的 timer PPI 使能态被清除，timer 由 D-59 链的 register_hook
  unmask 按需打开）——"init 后全屏蔽"不变量对 SGI/PPI 成立。
- **顺序钉住测试**：`dispatch_claims_before_mask_and_completes_last`（mock 增加跨方法
  `call_log`，钉住完整序列 unmask[register]→ack→mask→unmask→eoi；mock 的 ack 不再
  混入 eoi_log）。
- 文档同步：doc 14 §2.6（C `hw_intr_ack=*_eoi` 是 i386 事实；Rust ack=claim 语义差异注）、
  doc 05（ack/eoi 握手精确语义 + 分发序列）。
- 验证：kernel 730 passed / 0 failed / 8 ignored；plat 3 passed；clippy 零新增；QEMU 全量
  回归（aarch64 GICR init 写经实测）随 commit 记录。
- **I-13 关系**：trait 拆分（Router + per-CPU Ack）仍待 S-6 per-CPU 基建；本条只修正确性，
  现 trait 内完成，I-13 维持 open。

### 23.3 新发现 P2

#### D-60 注释准确性批（4 处漂移 + 1 处虚构 + 1 处失真） [P2] — ✅ 已修复/核实闭合（2026-09-09）

`os/kernel/src/lib.rs:1807`（`see smp.rs:200`）、`os/kernel/src/lib.rs:1823`
（`see proc_table.rs:276`）、`os/kernel/src/syscall.rs:2858`（`see vm.rs:323`——
`os/servers/vm/src/` 无 vm.rs，文件级失效，应指向实际文件或删除）；Step 1.0f 原始
5 项清单见 git 893386cd8 版。另有：`os/kernel/src/lib.rs:777-780` 声称 "x86-64 LAPIC
LVT timer entry 在 X86_64ArchInit::init() 使能"——实际 `os/arch/src/x86_64/arch_init.rs:46-62`
是空壳（虚构使能点）；`os/kernel/src/syscall_copy.rs:561` 注释 "Failure is logged"
实际只是丢弃返回值（C 同样不检查，注释失真非行为缺口）；`os/arch/src/x86_64/cpu_identity.rs:57`
MINIX3 BUG 标注引 `arch_system.c:239`，实际合并语句在 :243-245（~5 行漂移）。
修法：fix-guard 逐处 grep 目标符号现址后重写。

**✅ 解决记录（2026-09-09，fix-guard 逐处 grep 复核后执行）**：
- **已修 3 处**（行内行号引用改为符号引用——`CpuLocal::set_running` / `ProcessTable::rts_unset`
  / `ipc::delivermsg`，消灭行号腐烂这一类）：lib.rs `set_running(IDLE)` 注释（:200→符号）、
  lib.rs `rts_unset auto-enqueues` 注释（:276→符号；现址 418）、syscall.rs `copy_msg_to_user`
  doc（vm.rs:323 文件级失效→`ipc::delivermsg` 消费点，grep 核实在 ipc.rs:402）。
- **已随 D-59 闭合**：lib.rs "LVT 使能点" 虚构声称（该注释块在 D-59 Step 6 重写中删除）。
- **复核剔除 2 条（防重查）**：① cpu_identity.rs 的 C 锚点 `:239` **正确**——C 源
  `arch_system.c:239-240` 正是 model 合语句，V13 扫描代理报的 ":243-245 漂移" 为误报；
  ② "Failure is logged" 全库 grep 零命中（已被此前批次修复或扫描误读），无需动作。
- 验证：kernel 730/0/8 全绿（注释改动无行为面）。

#### D-62 A2 收尾：globals.rs 之外 4 组静态漏网 [P2] — ✅ 已修复（2026-09-09，③ 契约显式化 + 收敛路径登记）

`os/kernel/src/globals.rs:1-6` 的"单一审计点"声明对核心状态成立，但：①
`os/kernel/src/krandom.rs:172/:177`（`KRANDOM` SyncUnsafeCell + `KRANDOM_INIT` AtomicBool）；
② `os/kernel/src/smp.rs:837`（`CPU_INFO` SyncUnsafeCell）——类型在审批清单、静态本体
不在 globals.rs；③ `os/kernel/src/clock.rs:58-79` 四个 AtomicU64 镜像
（CLOCK_UPTIME/REALTIME/BOOTTIME/TSC_PER_MS，与 CLOCK_STATE 字段并存，clock.rs:50 注释
自辩"免穿引用读"——属 [ARCH] 级双真相源，需裁决：镜像豁免登记 or 收敛到单一来源）；
④ **最尖锐**：`os/kernel/src/misc.rs:2227/:2242` 两个 `static mut`（SPROF_INFO +
256KB SPROF_SAMPLE_BUFFER）连 SyncUnsafeCell 模式都绕过（仅 addr_of_mut! 纪律，
misc.rs:2219-2222 自述）。修法：迁 globals.rs 或按 A2 反例条款论证豁免并登记；
`static mut` 至少升 SyncUnsafeCell + BklProtected 审批。

**✅ 解决记录（2026-09-09，11 个静态全部收编 globals.rs）**：
- **①②④ 迁移**：`KRANDOM`/`KRANDOM_INIT`、`CPU_INFO`、`SPROF_*` 六件 + `SPROF_INFO`/
  `SPROF_SAMPLE_BUFFER` 两个 `static mut` 升级 `SyncUnsafeCell`（`get()` 取代
  `addr_of_mut!`，19 个访问点机械替换；misc.rs 声明块删除改 import）；`SprofInfo` 与
  `u8`（`[u8;N]` 复合 impl 的元素类型）加入 `bkl_protected_impls!` 审批清单（即 A2
  设计的"审批摩擦点"动作）；globals.rs 头部清单 13 → 24。
- **③ 裁决：双源契约显式化，完全收敛登记为独立路径**。穷举消费方（sched/proc_table/
  ipc/syscall_clock 共 6 点）证实**全部持 BKL**——镜像注释"免穿引用读"的存在理由已
  失效；但完全收敛（删镜像、读侧走 `clock_state_with`）需给 `IpcEngine::
  build_notify_message` 等引擎内部函数穿透 `&BklSection` 签名，属 IPC 引擎级重构，
  不属"A2 收尾"边界。本批落地：4 个 Atomic 迁 globals.rs + 写侧 Release 双写/读侧
  Acquire/两源差 ≤1 tick 的契约写入 clock.rs 与 doc 15（C 单源 `kclockinfo` 差异如实
  记录）；收敛重构作为独立条目跟进（见下）。
- ✅ **I-15（已闭合，2026-09-15，code-excellence 轮）**：三个镜像原子
  （CLOCK_UPTIME/REALTIME/BOOTTIME）删除，ClockState 为唯一源。实现与原设想的
  section 穿透不同：①tick/时间系统路径（smp_init deadline、build_notify_
  message、proc_table.get_monotonic、syscall_clock GET_TIME）改 `clock_state_
  boot_unchecked().uptime()` 直读——生产调用方持 BKL 直读无竞态，hosted 测试
  单线程同享该性质，避免 ~15 处 rts_set/notify 调用链的 section 级联；
  ②notify 链 timestamp 由 build_notify_message 内部直读（签名不变）。
  消费方 grep 归零。语义与 D-62 的"单源"目标一致，机制选择见该条与
  clock.rs get_* 文档。验证：kernel 742 hosted 全绿；test-smp-aps/proc-init/
  shutdown 硬件 PASS。
- 验证：kernel 730/0/8 全绿；clippy 零新增；doc 15/25 声称同步（SPROF 访问描述、
  镜像契约）。

#### D-63 A1 约定破例两处（witness 通道） [P2] — ✅ 已修复（2026-09-09，两处同批）

**✅ 解决记录**：① `clock_irq_handler`（`os/kernel/src/clock.rs`）迁移到 `_with` 访问器族 +
`BklSection::assume_held()` 链根取证（`IrqHandler` fn 指针签名无法携带 witness，与
`KernelNotifier::notify_hardware` 同constraint；"boot context" 误导注释删除）——"运行期 IRQ
路径用 boot_unchecked"的契约违反消除；② `ipc_filter_pool` 裸访问器消灭：`notify_scheduler`
（proc_table.rs，`section` 参数本就在作用域，纯疏漏）、`kernel_call_dispatch_inner` 的
Statectl 臂（syscall.rs，同上）、`dispatch_ipc`（syscall.rs，assume_held 链根 + kernel_call
dispatch 契约注释）三处全部迁 `_with`，`lib.rs` 删除裸访问器与 `#[allow(dead_code)]`。
测试侧：`clock_irq_handler` 直调测试与 4 个触及 assume_held 链根的测试补 RAII 取锁
（`bkl_lock_reset_for_test` + `bkl_lock()` 不 transfer，作用域尾自动解锁——首版用 transfer
遗留锁定态导致字母序后续测试自旋挂死，回归 review 拦截后修正）。验证：kernel 730/0/8 全绿，
clippy 零新增。

① `os/kernel/src/clock.rs:1271-1275` `clock_irq_handler`（D-46 新代码）在**运行期定时器
IRQ 路径**用 `*_boot_unchecked` 访问器族——"boot context" 注释名不副实，`_with` 访问器
可用而未用；② `os/kernel/src/syscall.rs:781-783` `dispatch_ipc` 用裸 `ipc_filter_pool()`
（`os/kernel/src/lib.rs:1740`，无 witness 参数），而带 witness 的 `ipc_filter_pool_with`
（lib.rs:1748）挂着 `#[allow(dead_code)]`——同文件新代码走旧通道。修法：迁移到 `_with`/
`assume_held` 链根取证（同 A1 已迁 24 处的形态）。

#### D-64 boot 链窄项三件 [P2] — ✅ 已修复（2026-09-09，三件同批）

① **OpenSBI 路径 FREE_MEMMAP 含内核镜像**：`os/boot-shim/src/opensbi_helpers.rs:396-403`
把整段 DRAM 报成单一 CONVENTIONAL 区（含内核/模块/shim）；kernel Phase A.2 只 cut 模块
（`os/kernel/src/lib.rs:439-445`），内核镜像只在 VmBootRegion 排除与 handoff 快照中扣除
——GET_MEMINFO 出口与 C `pre_init.c:196-199`"内核按额外模块 cut"语义在 OpenSBI/U-Boot
路径不一致（UEFI 路径因 LOADER_DATA 天然无害）。修法：kernel 侧 Phase A.2 补 cut 内核
镜像区间（stage 内闭合，非 edge）。② `os/boot-shim/src/uefi_helpers.rs:245-255`
`assert_bootstrap_outside_memmap` 定义后零调用——shim 侧防线是死代码，wire 或删除
（删除需说明"为何死"：kernel 侧 vm_handoff 扣除已兜底）。③ Phase E 注释壳
（`os/kernel/src/lib.rs:491-496` 无可执行语句，system_init 已被 enum match 取代）——
kmain 头注/流程注释与实际不符，注释级同步。

**✅ 解决记录（2026-09-09，三件同批）**：
- **① 内核镜像自切**：Phase A.2 新增 Step 3——`cut_memmap(kern_phys_base, kern_size)`
  （C parity：`pre_init.c:196-216` 把内核当额外 module `kern_mod` 一并 cut）。UEFI 路径
  no-op（内核 LOADER_DATA 不在 conventional memmap，cut 返 Err 被忽略=预期）；OpenSBI
  路径修复 GET_MEMINFO 把内核镜像报成可用内存的偏差。
- **② 死防护代码接线**（选择 wire 而非删除——它是真实的 fail-fast 防线）：
  `prepare_boot` 在 root page/bump region 两笔 LOADER_DATA 分配后调用
  `assert_bootstrap_outside_memmap`，固件若把 conventional 页分给引导分配即刻 panic
  （此时 boot-shim 尚能打印诊断）。
- **③ Phase E 注释**：kmain 头注 Phase E 行改为"无 boot 期动作"（C system_init 已被
  enum Syscall 分派吸收）+ Phase B/F 行补 D-59 后状态；Phase E 体注改写为准确叙述。
- 文档同步：doc 01 回收逻辑分布表补第 4 行（内核镜像自切 + 两路径差异）。
- 验证：kernel 730/0/8 全绿（memmap 18/18）；boot-shim x86_64-unknown-uefi check 通过。

#### I-6 改判：bill_ptr 链路已闭环，主循环真实可达（原"保持 DEFERRED"降级为三个小余项）

**V13 实测推翻旧登记**：五阶段调度循环（`os/kernel/src/lib.rs:2583` switch_to_user，
真 `loop` :2606-2664）生产可达（真 kmain lib.rs:382 → bsp_finish_booting :1773 →
:1959）；"placeholder 占位循环"只存在于 qemu_test/mock 的 cfg 分支（lib.rs:714-726/:191-196），
生产构建没有占位循环。bill_ptr 完整链路已在：写点 set_bill_to_idle
（proc_table.rs:520）/pick_and_bill（lib.rs:2148-2153，含 is_billable 判定）/idle
（lib.rs:2266-2270），读点 clock_irq_handler→tick_with(billp)（clock.rs:1277-1327，
对齐 C clock.c:114-152 的 billp 三用途）。**生产单跳限制的根因是 S-8 缺席**（首个
restore_to_user 交出 CPU 后无向量回内核），非调度器缺陷。
**剩余三个小余项**（并入本条跟踪）：① idle 的 kernel_ticks 未接线；②
`CpuLocal::set_running` 无 BILLABLE 判定；③ S-8 落地后循环多轮行为端到端验证。

**✅ 余项解决记录（2026-09-09，①② 落地；③ 维持 S-8 依赖）**：
- **① kernel_ticks 裁决为 no-op（C write-only 实证）**：全 C 树 grep 显示
  `kernel_ticks[CONFIG_MAX_CPUS]`（glo.h:86）只有声明与 arch_clock.c:231 的累加、
  **无任何读方**（do_getinfo/debug 均不触）——接线即 write-only 状态（D-34 标准）。
  idle Step 4 的缺口 TODO 注释改写为该裁决 + 证据。随带补上 **p_cycles 的 KERNEL
  分支累加**（C arch_clock.c:232，`p_cycles` 经 GET_PROC 可观察，是真缺口）：
  `idle` 中 `kernel.p_cycles.add_cycles(tsc_delta)`。
- **② set_running 收紧为 proc_ptr 原语**：bill_ptr 写是调用点策略（C 的三个写点
  两个 BILLABLE 门控、boot 点目标是自带 BILLABLE 的 idle；Rust 门控路径 =
  set_bill_to_idle / pick_and_bill / idle）。原语不再无条件写 bill_ptr，
  新增反例测试 `test_cpu_local_set_running_does_not_bill`（原测试断言的正是
  要消除的行为，已同步改写）。
- ✅ **I-16（已闭合，2026-09-15，code-excellence 轮）**：盘点 C 调用点全集后发现
  记账目标只有 KERNEL/idle 伪进程（C 的四个 context_stop 位点全部传 KERNEL 或
  idle_proc，用户进程的 p_cycles 在 C 中同样不经 context_stop）——Rust 侧此前的
  真缺口是：①finish_and_restore step 2（C:440 对应位）漏接 p_cycles 与 per-state
  两路记账；②idle 位点 CP_INTR 入桶代码重复出现两次（tick-1 两次编辑叠加，per-state
  通胀 ×2）。修复：提取共享帮手 `account_kernel_stop`（CP_INTR 入桶 + KERNEL
  p_cycles 累加，两处 context_stop 等价位点共用），双记账去重 + 漏接补齐；
  3 个 hosted 测试（累加/零 delta no-op/隔离）。FPU-exit 位点（proc.c:1956）
  与 context_stop_idle 的 TSC-delta 记账随用户帧/校准 lane（见 smp_todo §24
  deadline 注）。验证：kernel 742 hosted 全绿；test-smp-aps 硬件 PASS。
- **③ ✅ 已闭合（2026-09-15，S-7/S-10 证据）**：调度循环多轮端到端行为已由
    test-smp-aps（四核 entered + PIT 驱动多轮 idle/pick 循环）与 test-smp-ipi
    （IPI 中断打断 idle halt→处理→返回循环）实证。I-6 全条闭合。

### 23.4 新发现 P3（D-65 轻微项批） — ✅ 已处置（2026-09-09：①③ 落地，② 维持登记，④ 误报纠正）

① idle 命名：C `proc.c:66-90` set_idle_name 产出 "idle<n>"，Rust 硬编码 "IDLE"
（`os/kernel/src/proc_table.rs:85-88`，测试 :1298 钉住）——诊断输出漂移，取舍登记
（对齐 C 或保留类型化名，二选一即可）；② `get_value`/env_get boot 旋钮通道缺失
（C `main.c:481-508`；Rust 结构化 handoff 未留字符串通道——设计演进合理，将来需要
"boot 参数改内核策略"时再立项）；③ krandom D-33 的 `[ARCH: deviation]` doc+code 两层
一致、design 快照层（25-design.v1.md）未回写一行；④ `os/kernel/src/debug.rs:302/:313`
两个 `#[ignore]` 实为测试基建缺口（mock logger 未初始化）而非硬件依赖，可初始化后转
常规测试。

**✅ 处置记录（2026-09-09）**：
- **① idle 命名对齐 C**：`"IDLE"` → `"idle0"`（C `set_idle_name(name, cpu)` =
  "idle"+十进制 CPU 号，proc.c:66-90；GET_PROC 外部可观察，Ground Truth 链强制）。
  const 声明与 pin 测试同步（`[..5] == b"idle0"`）。
- **② env_get 旋钮通道**：维持登记（设计演进方向非缺口，需 boot 参数改内核策略时立项）。
- **③ krandom D-33 design 层回写**：`.design/25-design.v1.md` 追加 D33 补记，
  [ARCH: deviation] 三层一致达成（.design 不入库，本地生效）。
- **④ 纠正（V13 扫描代理误报）**：debug.rs 两个 `#[ignore]` 是**真硬件依赖**——
  hosted x86_64 经 plat cfg 选真 COM1 控制台，无 iopl 的 port I/O 即 SIGSEGV
  （实测确认，与 T-11/stacktrace 同类），并非"mock logger 未初始化"；已恢复 ignore
  并改写为准确理由。教训：ignore 理由注释的错误比 ignore 本身更误导。
- 验证：kernel 731/0/8 全绿。

### 23.5 Rule Discovery（Step 5.7）

✅ **Pattern #84 候选："抽象落地未收口"（Built-but-not-wired）**——正确的新抽象/API 落地
后，生产调用路径未迁移、继续走旧通道，新旧并存且旧通道往往是错的那个。本轮三实例：
`TimerIrqGate` 三架构实现齐备但生产零调用（D-59）；`ipc_filter_pool_with` 挂
`#[allow(dead_code)]` 而新代码走裸访问器（D-63）；`InterruptController::ack` 有实现无
生产调用方、dispatch 直接 eoi（D-61）。检测命令：`rg "fn xxx" + 生产调用点 grep == 0`
或 `#[allow(dead_code)]` 清单复核。**待用户确认后落地**到 review-patterns（同 #78 先例，
本 scan 先登记）。

### 23.6 处置建议（供 todo-fix 排序）

1. **S-8 批次前置**：D-59 + D-61（+ D-63① 因同一 IRQ 路径）——两项 P1 在中断真正开启
   （S-8 trap 入口）时从休眠转实害，建议作为 S-8 的显式前置子步骤或同批验收项；
   smp_todo.md FROZEN 不重开，但 S-8 的验收标准应包含"timer IRQ 链三架构端到端"。
2. **独立可修**（不依赖 SMP）：D-60、D-62、D-63②、D-64、D-65、I-6 余项①②。
3. **需裁决**：D-62③（clock.rs 四 AtomicU64 镜像的双真相源：豁免登记 vs 收敛）；
   D-65①（idle 命名取舍）。
4. 收敛评估：本轮新发现 2 P1 + 4 P2 + 1 P3 批 + 1 改判，占增量窗口（91 提交）的可
   发现面比例合理；对已收敛面（30 轮 doc review + V12）零重复发现，符合增量策略预期。

## 24. V13 执行 campaign 终局对账（2026-09-09）

**已修复（本 campaign，一 TODO 一 commit）**：D-59（1297da86c）、D-61（15e76355e）、
D-63①②（9cf8788bb）、D-60（8cf00a60e）、D-62（ae951c71a）、D-64（81d873818）、
I-6 余项①②（a22ae791a）、D-65（54d5403ea）、B-X 收口 + OQ-15-1 裁决（终批）。
派生新条目：T-13（dm_coverage 并行 flake）、I-15（时钟镜像收敛）、I-16（p_cycles 累加）。

**诚实 DEFERRED 维持（依赖未解除，逐条复核）**：
- **SMP 链**（S-3d 起全部步骤、Edge 表 12 项、T-2/T-3/T-4、I-13 trait 拆分、profiling
  deferred 体、trap-1/hw-1/hw-2）：依赖 = QEMU gdb 硬件调试 AP bring-up（smp_todo.md
  S-3d WIP ⏸，此前 4 轮尝试后主动暂停）——本会话未解除，维持 FROZEN 设计 + 暂停态。
- **T-10**：sudo/工具链不可得（解除条件已具体化），维持 DEFERRED。
- **T-8 QEMU 半**：归 T-5 CI 承接（需新 QEMU 测试内核工作，独立批次）。
- **I-1**：入口 ABI 设计 + boot-shim 终局跳转未做（单独构建 = 不可引导 artifact），
  维持 DEFERRED；**I-5**：无 RSDP 消费方，维持 DEFERRED。
- **I-15/I-16**（本 campaign 新登记）：触发窗口 = S-8 IPC 入口重构 / 记账专项，open。

**stage 状态总结**：本 campaign 后，01-stage-kernel 全部"可诚实完成"的 stub/deferred/TODO
清零；剩余 open 项的依赖全部在 SMP bring-up（S-3d 起的硬件调试链）或外部环境（sudo），
无一项可在当前环境下进一步推进。

## 25. S-3d 续诊（2026-09-09 第二会话：三修复 + 环境级根因 + 下一步）

**真实修复（已入库，见 smp_todo.md S-3d V13 续诊记录）**：
1. **FJ16/FJ32 远跳偏移脱钩修复**：手写 0x8040/0x80C0 与实际布局脱节（AP 落进 FJ32
   操作数字节执行垃圾——S-3d 卡死的直接原因之一），改汇编期 label 差值 + AP_BASE
   推导，hosted 签名契约测试（`test_far_jump_targets_hit_real_instruction_boundaries`）
   钉住"目标落点=下一阶段首指令"。
2. **SDM 强制延时补齐**：INIT→SIPI 10ms / SIPI 间 200µs（`tsc_delay`，C arch_smp.c
   udelay parity）——消除 0x0000/0xA2 随机（SIPI 在 INIT 未完成时被丢弃）。
3. **级标诊断设施**：梯子逐级写 AP_STAGE_MARK + #UD 收集器（IVT[6]→handler 记录
   故障 CS:IP）+ 串口级标——本轮全部定位证据的手段即来源于此，S-4/S-5 调试沿用。

**环境级根因（monitor `xp` 实证，修正此前"lgdt 0x67 编码"旧猜测）**：
- 本 QEMU+OVMF(TCG) 组合的 MpInitLib 将 AP 停放跳表/循环置于 **PA 0x8000-0x91xx**
  （每页头 3 字节页号痕迹 + `jmp 82:xx` 跳表 + CS=0x82 停放循环），与 vector 0x08
  选页（PA 0x8000）及 scratch（0x9000）全面冲突；
- 存在 **2/3 号 #UD 游走核**，持续执行/清零低内存（blob 安装后被清零实证）——
  低内存 trampoline 在该引导路径下不可靠。
- 已搬迁 blob/scratch/marks 至 vector 0x05（PA 0x5000/0x6000/0x6F00）并复核
  GDT/record 内容正确落盘，但游走核踩踏仍在——**属 OVMF+TCG 环境级问题**。

**下一步（S-3d 解除的明确路径）**：测试内核改 **multiboot 直启**（QEMU `-kernel`，
绕开 OVMF：AP 天然 wait-for-SIPI、无固件 AP 干扰、无 UEFI 依赖），multiboot 入口
桩 + 复用 kernel 库，随后 S-3d 在干净环境一次跑通，顺路成为 S-8（asm trap 入口）
的裸机测试基建。

**终局（2026-09-14 第四会话，第十~十二轮，详见 smp_todo.md 同名记录）**：mb 直启
内核在第十轮以"先反汇编产物、再运行"的方法排除构建变量后，九轮"fill base 字节
写入未生效"改判为 **WSL 崩溃留下的陈旧 rlib**（干净重建后 fill 序列逐条正确，
desc 回读 27 00 10 61 ✓）；十轮谜团真凶由 `-d int` 故障转储首次给出的 AP 视角
GDTR={0xFF53F000, 0xFF53}=**IVT[0] 字节（F000:FF53）**钉死——手写 LGDT 的
modrm 0x15 沿用 32 位寻址形态（rm=101=disp），16 位寻址下 rm=101=[DI]，lgdt 实际
从 [DI=0] 装载；修复为 0x16（rm=110 mod=00=[disp16]）。连带修复阶梯尾部三处记录
字段绝对读的重复"60"字面量（含 stack 读 +4 与 offset_of! 钉死偏移矛盾——AP 从未
到达尾部故未引爆），并新增
`test_ladder_absolute_reads_match_record_layout` 钉死全部绝对读字节契约。修复后
**`TEST_RESULT: PASS test-smp-ap-alive-mb`、中断日志 0 异常**（AP 一次爬完
16→PE→32→CR3/PAE/LME/PG→64→`ap_early_entry`，BOOT_ACK 观察 ✓），minix-arch
225 hosted 测试全绿。修复 commit aa73eeb5f。**SMP 链自 S-8 起解锁**（S-8 asm
trap stub + SYSCALL 入口 → S-4→S-13 与 Edge 表 12 项顺次解冻）。

**全量回归（同日）**：run_all.sh 600s 截断统计 **19 PASS / 0 FAIL**——x86 9 项
全绿，其中 **canonical OVMF 路线 `test-smp-ap-alive` 亦首次 PASS**（此前"游走核
踩踏低内存"应为本测试三重故障后被 QEMU 复位的 AP 的次生现象，非固件环境问题；
smp_todo §25 环境级根因段相应降级为历史假说）；aarch64 全绿；riscv64 前 3 项绿
（截断段与本修复无关——x86 阶梯不参与 arm/riscv 构建）。CI 计数的 23 测试至此
应全绿，T-5 记录待 CI 复跑刷新。


## 26. doc-gen v3 文档复核发现的源码侧分歧（2026-10-09/10 登记，待代码侧裁决）

> 来源：`07-cross-space-init.md` 与 `06-proc-init-boot-proc.md` 的重写复核（四路：逐节实读/盲态闭卷/旧稿块级对照/独立事实核查）。
> 全部为**注释与现码不一致**或**C/Rust 语义分歧**，无当场可判的代码 bug；文档侧已按现码陈述并在对应节登记，此处汇总供 todo-fix 排期。逐条的证据命令见两篇文档的对应节。

| # | 位置 | 分歧 | 文档登记处 |
|---|---|---|---|
| 1 | `os/kernel/src/lib.rs` Step 2 注释（`init_post_and_memory`） | 注释称 "Record VM as the kernel-level ptproc"，函数体无对应语句；记录点实际在 `switch_address_space`（lib.rs:384/567 附近的调用路径内） | 06 §3.12、07 §3.5 |
| 2 | `os/kernel/src/lib.rs` 同函数 Step 3 注释 | "窗口在 Phase C 建立"与现码偏离：窗口在 `arch_boot` Step 4 建立、Phase C 只记根地址 | 07 §3.2 末、§4.2 |
| 3 | `os/kernel/src/lib.rs` 同函数 Step 1 注释 | 坐标 `(lib.rs:960-963)` 陈旧：`phys_root`/`virt_root` 实际写在 lib.rs:1997/2000 | 07 §3.5 |
| 4 | `os/kernel/src/dm_coverage.rs:81`（`establish_boot_dm` 函数文档注释） | "Called once from arch_boot_impl" 已过时：两条生产路径（lib.rs:384/567） | 07 §4.4、§5.3 |
| 5 | `os/arch/src/arch/direct_map.rs:137-139`（riscv64 模块注释） | 自相矛盾："non-canonical"与"payload decodes to VPN[2] = 256"不能同时成立——旧值 `0xFFFF_FC00_0000_0000` 的 39 位载荷是 0x0（VPN[2]=0） | 07 §4.1（已按算术改写） |
| 6 | `os/plat/src/x86_64/interrupt.rs:26-31`、`os/plat/src/lib.rs:59-60` | 注释写 "PIT 输出是 IOAPIC input 0"，与 `isa_irq_to_pin` 的 IRQ0→input 2 映射矛盾（文档按操作代码陈述） | 07 §4.9 |
| 7 | `os/kernel/src/lib.rs`（`bsp_finish_booting` 相关注释） | 注释称其为 dead-code 分歧路径，与现码矛盾（kmain Phase F 调用它、它再调 `boot_init_timer`） | 06 §5.3（报）|
| 8 | C `do_clear` vs Rust `dispatch_clear` 的 `RTS_NO_ENDPOINT` 清除语义 | C 用 `RTS_SETFLAGS` 整值覆盖（隐式清位）；Rust 用 `rts_set(SLOT_FREE)`（fetch_or 按位置位），SLOT_FREE 与 NO_ENDPOINT 同时留存——`is_empty` 与 fork 子槽检查的精确判据 `p_rts_flags == RTS_SLOT_FREE` 在 Rust 路径下达不到 | 06 §2.2.2（行 581/602 一带） |
| 9 | 用户模块 ELF 装载归属（三处说法打架） | `ProcKind` 注释写 "ELF deferred to RS"；C 的实际装载者是 VM（`arch_boot_proc`）；`load_process_elf` 目前只有测试内核调用 | 06 §2.7 |
| 10 | `os/boot-shim/src/loader.rs` 错误变体测试缺口 | `NoLoadSegments`（loader.rs:144）与 `InvalidSegment`（loader.rs:211）无任何测试构造 | 01 §5.1、07 未涉及 |

> 处置建议：#1–#7、#9 均为**注释回修**（一处 PR 批量可清）；#8 是**真实语义分歧**，改哪一侧需对照 C 真源与 `is_empty` 消费面单独裁决（建议走 todo-fix 单条流程）；#10 属测试完备性缺口，归 test-audit 排期。

> **处置（`2026-10-11`，`claim/KBOOT-DIVERG-78-qoder` @ 基线 `ff00b80fd`）——#7、#8 已闭环**：
> - **#8（清槽语义，对齐 C 整值赋值）**：新增 `ProcessTable::rts_setflags`（整值赋值 + C `RTS_SETFLAGS` 同款 `proc_is_runnable`→出队判定），`dispatch_clear` 清槽处由 `rts_set(SLOT_FREE)` 改调它，与 C `do_clear.c:57` 逐位对齐，残留 `NO_ENDPOINT` 隐式清除、`is_empty`（精确 `== SLOT_FREE`）恢复可达。核实：`cargo test -p minix-kernel --features mock --lib`→831/0（`test_rts_setflags_wipes_other_bits` 钉整值清位、`test_dispatch_clear_sets_target_slot_free` 加 `is_empty`+`NO_ENDPOINT` 断言）；`grep -rn fn rts_setflags os/kernel/src/proc_table.rs`→定义与调用两处。
> - **#7（`bsp_finish_booting` 注释与现码矛盾，并消除一处重复初始化）**：生产 `kmain`（`lib.rs:804`，`#[cfg(not(mock), not(qemu_test))]`）Phase B 曾在 `lib.rs:956` 直调 `boot_init_timer()`，Phase F（`lib.rs:1117`）现调 `bsp_finish_booting`、其内 Step 6（`lib.rs:2864`）又调一次——二者并存为启动重复初始化时钟源。删 `lib.rs:956` 冗余调用，保留 C `boot_cpu_init_timer` 真位（Step 6）单点；注释改写与现码一致并去掉“dead-code”误导。核实：`cargo build -p minix-kernel --target x86_64-unknown-none --no-default-features` Finished、`boot_init_timer` 无 unused 告警；真机 x86_64 release/QEMU（快照 `os/target/image/x86_64/serial.log`）→ 0 panic、进 `entering scheduler`、init runcom 执行 `#!/bin/sh`、`e5-ds: label init = 11`，证 tick 与量子抢占仍正常。改动在共享 arch-无关 `kmain`。
>
> **处置（`2026-10-11`，`claim/KBOOT-DIVERG-T2-qoder` @ 基线 `cfbf27463`）——#9 已闭环**（= `08-todo.md` T-2）：
> - **#9（ELF 装载归属三处说法打架）**：`os/arch/src/arch/boot.rs` 的 `ProcKind::RootService`/`UserService` 两条注释原写 "ELF deferred to RS"，与 C 真源不符——`protect.c:388` 的 `arch_boot_proc` 仅在 `p_nr == VM_PROC_NR` 分支把 VM 装进 bootstrap 页表，其余 boot 镜像的页表由 VM 起来后建（`load_process_elf` 文档头 :709-720 早已正确写“生产模块装载是 VM 的活”）。改两条注释为“由 VM 装载”，与 C 及同文件 `load_process_elf` 现有注一致（采 08-todo 案 1，纯注释回修、不动代码语义）。核实：`grep -rn 'deferred to RS' os/`→零命中；`cargo check -p minix-arch` Finished（无新增告警）；`load_process_elf` 仍无生产调用者（`grep -rn load_process_elf os/ \| grep -v test` 仅定义+lib 再导出）。06 篇 §2.7 的偏离登记句（文档侧）待文档 agent 据本修复删除。
>
> **处置（`2026-10-11`，`claim/KBOOT-COMMENTS-347-qoder` @ 基线 `bd27af71a`）——#4 已闭环**（= `08-todo.md` T-4；另 `08-todo.md` T-7 的 proc_arch 陈旧引用一并回修）：
> - **#4（`dm_coverage.rs` “Called once” 过时）**：`establish_boot_dm` 函数注释原写“Called once from `arch_boot_impl`”，实有两条生产路径（`lib.rs:384` = `arch_boot_resume_high_half`、`lib.rs:567` = `arch_boot_impl` Step 4）。注释改为列出两条路径。核实：`grep -rn 'Called once' os/kernel/src/dm_coverage.rs`→零命中。
> - **proc_arch 陈旧引用（`08-todo.md` T-7）**：`os/arch/src/arch/boot.rs` `load_vm_elf` 注释引用了重构中已删除的 `os/arch/src/{x86_64,arm64,riscv64}/proc_arch.rs`；改指 `arch/mod.rs` 删除记录，不再指向不存在的路径。核实：`grep -rn 'proc_arch' os/arch/src/`→只剩 `arch/mod.rs` 的删除记录行。纯注释回修、无代码语义变更。
>
> **处置（`2026-10-11`，`claim/KBOOT-COMMENT-T5-qoder` @ 基线 `9cb843065`）——#5 已闭环**（= `08-todo.md` T-5）：
> - **#5（riscv64 `direct_map.rs` 注释算术自相矛盾）**：旧基址 `0xFFFF_FC00_0000_0000` 的注释称“39 位载荷 decodes to VPN[2] = 256，撞镜像自身槽”，但 python 核算 `(v>>30)&0x1FF` = 0、payload = 0x0——它是非规范地址、VPN[2]=0，不落任何合法顶层槽（与 x86-64 撞镜像 L2 槽是不同型的错）；256 是内核镜像基址 `0xFFFF_FFC0_0000_0000` 的位置。注释按算术改写。核实：`python3 -c "v=0xFFFF_FC00_0000_0000; print((v>>30)&0x1FF)"`→0；`grep -rn 'decodes to VPN\[2\] = 256' os/`→零命中。纯注释回修。
>
> **处置（`2026-10-11`，`claim/KBOOT-COMMENT-T3-qoder` @ 基线 `b8c29df30`）——#1/#2/#3 已闭环**（= `08-todo.md` T-3）：`init_post_and_memory` 三条失真注释按现码回修——（a）Step 1 坐标 `(lib.rs:960-963)` 陈旧（该处现为 kmain rdmsr 探针），改为符号指路“在 `init_proc_and_boot` 内赋值”（phys_root/virt_root 实写点已漂至 :2007/:2010，用符号避免再漂）；（b）Step 2 “Record VM as the kernel-level ptproc” 函数体实无语句，`set_current_ptproc_nr` 真写在 `switch_address_space`（:3495/:3499）、CpuLocal 槽在 `init_smp_state` 安装，改为“Confirm …（此步自身不记录）”；（c）Step 3 “VM Direct Map window … established in Phase C” 失真，窗口由 `establish_boot_dm` 在 arch-boot Step 4 建立（:384 “step4 DM coverage”），Phase C 只记根地址。核实：`grep -rn 'lib.rs:960-963\|established in Phase C when the kernel' os/kernel/src/lib.rs`→零命中。root 在 Phase C 安装属正确，未动。纯注释回修、无代码语义变更。
>
> **处置（`2026-10-11`，`claim/BOOTSHIM-ERRTESTS-qoder` @ 基线 `16534735c`）——#10 已闭环**（= `08-todo.md` T-9）：`os/boot-shim/src/loader.rs` 两个错误变体无测试构造——现补两枚宿主测直接产出该变体：`test_compute_kernel_layout_rejects_image_without_load_segments`（将唯一 phdr 由 PT_LOAD 改 PT_NULL→`compute_kernel_layout` 返 `Err(NoLoadSegments)`）与 `test_load_segments_into_phys_memory_rejects_overlapping_elf`（duplicate phdr + e_phnum=2 造两段同物理地址重叠→`load_segments_into_phys_memory` 在拷贝循环前的重叠守卫处返 `Err(InvalidSegment)`，不解引用野地址、宿主安全）。`first_overlapping_pair` 谓词既有 5 测覆盖，本两条补的是变体产出。核实：`cargo test -p boot-shim --lib`→16/0（+2）；clippy 触及新测零告警。test-audit 自身红：初版曾加一枚 overlap 谓词测，发现与已有 5 枚冗余→改接真返 InvalidSegment 的产出测。
>
> **处置（`2026-10-11`，`claim/KBOOT-COMMENT-T6-qoder` @ 基线 `48c40baad`）——#6 已闭环**（= `08-todo.md` T-6）：`os/plat/src/x86_64/interrupt.rs` 与 `os/plat/src/lib.rs:73` 四处 present-tense 注释将 PIT 时钟误称“IOAPIC input 0”——混淆了 ISA IRQ 号与 IOAPIC 输入引脚：PIT 驱 ISA IRQ 0，virtual-wire 模式下经 `isa_irq_to_pin` 落到 **pin 2**（input 0 实从机级联/ISA IRQ2），而内核按 ISA IRQ 号寻址故 `TIMER_IRQ=0` 正确。四处改为“ISA IRQ 0 →经 `isa_irq_to_pin` 落 pin 2、内核按 ISA 号寻址”的准确口径（:27 / :36历史叙述 / :456 测试注释 / lib.rs:73）。核实：`grep -rn 'input 0' os/plat/src/ \| grep -iv 'irq\|swap'`→零命中；`cargo check -p minix-plat` Finished。**未改测试名 `test_timer_irq_is_ioapic_input_zero`**：该名字被 `05-clock-interrupt-init.md:1763` 引用，而 05 篇正由文档线程重写，改名会断引用（碰撞面）；测试断言 `TIMER_IRQ.get()==0` 本身正确，已在注释内澄清真实引脚路由，改名待 05 篇定稿后另处理。纯注释回修、无代码语义变更。
>
> **处置（`2026-10-11`，`claim/KBOOT-COMMENT-T10-qoder` @ 基线 `4def1333e`）——`08-todo.md` T-10 已闭环**（§26 无对应行，追加登记）：`os/kernel/src/proc.rs` 的 `Quantum::allocate` 无生产调用点——生产量子续期在 `sched.rs:497` 内联写 `cpu_time_left`（C `sched.c` `p_cpu_time_left = ms_2_cpu_time(quantum)` 对位），与 `allocate` 行为等价；而时钟递减侧真调 `Quantum::consume`——两方法调用面不对称。采 08-todo 案 1（非删、非强接线）：给 `allocate` 补文档注释写明不对称现状、生产内联点与本方法作为 `consume` 对称接口/未来集中化归宿的定位（非死代码）。纯注释回修、无代码语义变更。
>
> **处置（`2026-10-11`，`claim/KBOOT-MFCOMMENTS-qoder` @ 基线 `a7bae2ff1`）——`08-todo.md` T-8 已按正解闭环**（§26 无对应行）：工单 T-8 称 `EXT_REG_INITIALIZED` 是 `MiscFlagsBits` 里唯一无注释成员——实测不属实：整个 `MiscFlagsBits` 块十七个位均无逐位注释。按正解一次性补齐整块（逐位照 `minix3/minix/kernel/proc.h:234-262` 的 `MF_*` 语义写 `///` 注释），并注明 Rust `EXT_REG_INITIALIZED` 对应 C `MF_FPU_INITIALIZED`（arch-中性命名）、`MSGFAILED` 在 C 定义处亦无注释。纯注释回修、无代码语义变更。核实：`cargo test -p minix-kernel --features mock --lib`→831/0（bitflags 成员 `///` 正常编译）；clippy/rustfmt 触及区零告警零漂移。

> **处置（`2026-10-11`，`claim/CMD-K08-T11-qoder` @ 基线 `f89e02da8`）——`08-todo.md` T-11 已闭环（状态：已裁决——登记不移植，L2）**：工单 T-11 指 C `struct proc` 的 `p_stale_tlb[BITMAP_CHUNKS(CONFIG_MAX_CPUS)]`（“哪些 CPU 上可能有本进程的陈旧 TLB 项”）在 Rust `KProcess` 无对位字段。前提已核：`grep -rniE 'stale_tlb' os/kernel os/arch os/libs`→零命中（确无对位）；C 写点在 `do_vmctl` 的 `VMINHIBIT` 解除分支（`bits_fill`），本树无 SMP 写路径消费它。按工单自身建议**登记不移植**（非代码缺口、非降级：当前无消费者，强接会引入无读者的死状态），本条只在 `06-proc-init-boot-proc.md` §4.4 的 `p_stale_tlb`“无对位”登记行补“归 `08-todo.md` T-11”指向与“待 `16-smp` 设计时定夺”的口径。无代码变更，仅文档登记。核实：`grep -n 'T-11' rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md`→§4.4 登记行命中；本轮未动 `os/` 代码，三架构编译门不受影响。
