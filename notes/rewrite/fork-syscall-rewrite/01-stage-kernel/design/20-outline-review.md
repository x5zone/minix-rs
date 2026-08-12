# 20-syscall-device-outline-review.md — Outline 评审

> **评审对象**: `design/20-outline.md`
> **评审方法**: 4 维自审（教学性 / 本质深度 / 概念覆盖 / 组织合理性）
> **判定标准**: P0=0 → 自动批准；P0>0 → 修订后重新评审
> **创建**: 2026-08-01

---

## 一、评审结论

| 维度 | 判定 | 说明 |
|------|------|------|
| 教学性 | ✅ PASS | Ch1 concept-driven，主语"硬件+安全"；每节有"灵魂本质"+ WHY→WHAT→HOW 弧线（§1.1 IRQ 控制为典型） |
| 本质深度 | ✅ PASS | Ch3 hypothesis-driven，7 个决策均有"如果 X 会有 Y 问题所以用 Z"；D6 trait+BadCall 推理覆盖 #[cfg] 违反抽象原则 |
| 概念覆盖 | ✅ PASS | A-G 七组知识点全覆盖；6 个 do_* 函数 + generic_handler 全列；断裂修复表 12 项完整 |
| 组织合理性 | ✅ PASS | 章节顺序符合认知弧线；Ch2 锚定 C 源码 file:line；Ch4 真实代码 + DEFERRED 诚实标注；Ch5 33 个可 grep 测试 |

**P0 计数**: 0
**P1 计数**: 3（非阻断，执行中注意）
**判定**: ✅ **自动批准**，可进入 design 生成阶段

---

## 二、详细评审

### 2.1 教学性（Teaching Quality）

**检查项**:
- [x] Ch1 主语是硬件/安全，非函数名/结构体名（开篇"内核如何让用户态驱动安全访问硬件？"）
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1 IRQ 控制为典型：WHY 中断上下文不可阻塞→WHAT 钩子三元组→HOW do_irqctl 4 子请求）
- [x] 新概念首次出现有定义（IRQ 钩子/IOPL/CHECK_IO_PORT/verify_grant/switch_address_space）
- [x] 概念之间有因果链（IRQ 钩子→通知驱动；端口 I/O 三层共享权限；x86-only→trait+BadCall 抽象）

**优秀点**:
- §1.2 端口 I/O 的"单次/批量/跨进程批量"三层抽象分类清晰，shared 解码 + 权限统一讲解，差异点（VDEVIO 静态缓冲区 / SDEVIO grant + switch_space）分别列出
- §1.3 IOPENABLE/READBIOS 的"内核只承担概念，编码下沉 arch 层"准确表达了分层思想
- §1.4 架构抽象用跨架构差异表直观展示 PortIo/InterruptController/IOPL/BIOS 的多架构对照

**P1 改进建议**（非阻断）:
1. §1.1 generic_handler 的"返回 `policy & IRQ_REENABLE` 控制重启用"可补充一句"驱动声明 REENABLE 后无需手动 ENABLE，内核自动重启用中断线"——但这是策略细节，读者从 policy 字段可推断，可不补
2. §1.2 VDEVIO 的"lock()/unlock() 包裹防中断"可补充"Rust 在 BKL 下已串行化，无需额外 lock"——但这是实现细节，属 Ch3/Ch4 范畴
3. §1.3 READBIOS 的"两段 BIOS 内存范围"可补充为何分两段（低段是 IVT+BIOS data，高段是 upper memory area 含 EBDA）——已在 Ch2 §2.6 暗含，可不再重复

### 2.2 本质深度（Conceptual Depth）

**检查项**:
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] 每个决策有 ≥2 个被否决的选项 + 否决理由
- [x] 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] 决策之间有逻辑关系（D2 PortIo trait → D6 trait+BadCall；D3 IrqManager → D4 IrqNotify trait）

**优秀点**:
- D6 trait+BadCall 的推理明确覆盖了 `#[cfg(target_arch)]` 的"分散条件编译 + 架构耦合渗入内核 + 违反硬件抽象原则"三大问题，并链接到 D9 全局决策
- D3 IrqManager 的"类型层完整 vs dispatch 层 BadCall"诚实标注，未掩盖 DEFERRED 状态——这是本次重写的核心修复点（原文档"⚠️ DEFERRED @ dispatch"开发日志味）
- D4 IrqHookContext + IrqNotify trait 是 Rust 独有 anti-translate（C 用函数指针+全局 mini_notify），非 translate
- D5 VDEVIO 栈分配的推理覆盖了 no_std + SMP 双重约束

**P1 改进建议**（非阻断）:
1. D3 的"dispatch 层 BadCall 根因"可补充"修复路径：KernelState 添加 `irq_mgr: IrqManager<ArchIc>` 字段"——但这是 design.md §3.1 的范畴，outline 不需展开
2. D7 IoSize/IoDirection enum 的推理较简短，可补充"裸位掩码的 `request & 0x0F0` 是魔数，type/dir 混在 int 里易写错"——已在推理中，可不再扩展
3. D2 PortIo trait 的"零虚拟开销"可补充"泛型 `<PI: PortIo>` 静态分发，编译期单态化"——但这是 Rust 通用知识，读者应已具备

### 2.3 概念覆盖（Concept Coverage）

**检查项**:
- [x] 知识点覆盖矩阵完整（A-G 七组 × Ch1-Ch5）
- [x] C 源码符号全部列出（do_irqctl.c 174 行 + do_devio.c 107 行 + do_vdevio.c 165 行 + do_sdevio.c 162 行 + do_iopenable.c 34 行 + do_readbios.c 37 行）
- [x] 断裂修复表完整（12 处文档断裂 + 修复方案）
- [x] DEFERRED 函数诚实标注（6 项 + 理由，§4.8 汇总表）

**覆盖验证**（对照 structure.md 知识点）:
- A.0-A.11 IRQ 控制: ✅ §1.1 + §2.1 + D1/D3/D4 + §4.1 + test_irqctl_*/test_check_irq_*
- B.0-B.7 DEVIO: ✅ §1.2 + §2.2 + D2/D7 + §4.2 + test_devio_*/test_io_*
- C.0-C.8 VDEVIO: ✅ §1.2 + §2.3 + D5 + §4.3(DEFERRED) + (待补)
- D.0-D.10 SDEVIO: ✅ §1.2 + §2.4 + D2 + §4.4(DEFERRED) + test_sdevio_*
- E.0-E.9 IOPENABLE/READBIOS: ✅ §1.3 + §2.5/§2.6 + §4.5/§4.6 + test_iopenable_*/test_readbios_*
- F.0-F.7 架构抽象: ✅ §1.4 + §2.7 + D2/D6 + §4.7
- G.0-G.4 redox 对比: ✅ design.md 附录 B（outline 引用）

**无遗漏**: structure.md 列出的 12 处文档断裂 + 8 处实现断裂全部在 outline 中有对应章节处理（实现或 DEFERRED）。

**VMCTL 归属判定**: outline §0 明确 VMCTL 不适用（属内存系统调用），避免范围蔓延。

### 2.4 组织合理性（Organizational Soundness）

**检查项**:
- [x] 章节顺序符合认知弧线（概念→源码→决策→实现→测试）
- [x] Ch2 每个符号带 file:line
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED + §4.8 汇总表
- [x] Ch5 测试函数可 grep 验证（33 个 `fn test_*`）
- [x] 参见形成闭环（14/22/13/18/16）

**优秀点**:
- Ch2 §2.7 调用关系图直观展示 6 个 syscall 的处理路径分支
- Ch4 §4.8 DEFERRED 汇总表统一列出 6 个 DEFERRED 项 + 理由，读者一目了然
- Ch5 §5.1 列出 33 个实际测试函数名 + 对应 C 符号，§5.2 待补充测试标注依赖
- 断裂修复表 12 项与 structure.md 诊断一一对应，修复方案明确

**无 P0 组织问题**。

---

## 三、执行注意事项（design 生成阶段关注）

1. **D3 dispatch_irqctl BadCall**: design 需明确"类型层 `dispatch_irqctl<IC>` 完整 vs dispatch 层 BadCall"两层状态，并给出 KernelState 接入 IrqManager 的修复路径（design.md §3.1）。**禁止**用"⚠️ DEFERRED @ dispatch"开发日志味表述。

2. **D6 trait+BadCall**: design 需定义 `dispatch_arch_*` 函数的 cfg 门控模式 + 非 x86 BadCall 返回。引用 13-syscall-dispatch.md D9 全局决策。

3. **§4.8 DEFERRED 汇总**: design 需为每个 DEFERRED 项提供实现方案（数据结构 + 算法 + 依赖），即使当前不实现。特别是：
   - dispatch_vdevio 的栈缓冲区 `[u8; VDEVIO_BUF_SIZE]` + data_copy_vmcheck 接入
   - dispatch_sdevio 的 verify_grant + switch_address_space + phys_* 接入
   - dispatch_readbios 的 virtual_copy_vmcheck 接入
   - dispatch_irqctl dispatch 层的 KernelState 重构路径

4. **anti-drift ENOSYS 决策**: design 需明确"DEFERRED 函数返回 ENOSYS 而非 OK"的理由（避免 silent 语义漂移），这是 Rust 独有决策，非 C translate。

5. **redox 对比（附录 B）**: design 需对比 redox 的 `scheme::irq::IrqScheme`（用户态 IRQ scheme）+ `scheme::io::Pio`（端口 I/O newtype）与 minix-rs 的 `IrqManager<IC>` + `PortIo` trait，说明设计哲学差异（minix-rs 对齐 C 内核维护钩子；redox 是 userspace-driver 重设计）。

6. **语义偏移文档化**: design 需在附录 A 差异矩阵中文档化两处合理收紧：
   - DEVIO unknown type：C default size=4 → Rust EINVAL
   - VDEVIO 超缓冲区：C E2BIG → Rust EINVAL

7. **VMCTL 边界**: design 不纳入 VMCTL 内容；如需 VMCTL 设计，建议独立文档。

---

## 四、批准

✅ **批准**，进入 design 生成阶段（`design/20-design.md`）。

- P0: 0
- P1: 3（非阻断，design 阶段关注）
- 评审人: Trae (GLM-5.2)
- 评审日期: 2026-08-01
