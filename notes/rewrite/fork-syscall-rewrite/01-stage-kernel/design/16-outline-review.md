# 16-outline-review.v1.md — Outline 评审

> **评审对象**: `.design/16-outline.v1.md`
> **评审方法**: 4 维自审（教学性 / 本质深度 / 概念覆盖 / 组织合理性）
> **判定标准**: P0=0 → 自动批准；P0>0 → 修订后重新评审
> **创建**: 2026-08-01

---

## 一、评审结论

| 维度 | 判定 | 说明 |
|------|------|------|
| 教学性 | ✅ PASS | Ch1 concept-driven，每节有"灵魂本质"+ WHY→WHAT→HOW 弧线 |
| 本质深度 | ✅ PASS | Ch3 hypothesis-driven，9 个决策均有"如果 X 会有 Y 问题所以用 Z" |
| 概念覆盖 | ✅ PASS | A-F 六组知识点全覆盖；断裂修复表完整 |
| 组织合理性 | ✅ PASS | 章节顺序符合认知弧线；Ch2 锚定 C 源码；Ch4 真实代码+DEFERRED 诚实标注 |

**P0 计数**: 0
**P1 计数**: 2（非阻断，执行中注意）
**判定**: ✅ **自动批准**，可进入 design 生成阶段

---

## 二、详细评审

### 2.1 教学性（Teaching Quality）

**检查项**:
- [x] Ch1 主语是 CPU/OS/矛盾，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1 BKL 为典型）
- [x] 新概念首次出现有定义（如 BKL/IPI/per-CPU/CPU 亲和性）
- [x] 概念之间有因果链（BKL→per-CPU 免锁→IPI 跨 CPU→亲和性迁移）

**优秀点**:
- §1.1 BKL 的 WHY→WHAT→HOW 弧线清晰：从"细粒度锁三大难题"到"BKL 串行化"再到"C 实现"
- §1.3 IPI 的异步/同步分类直观，重入处理有专门说明
- §1.5 AP 启动握手用时序图展示 BSP/AP 交互

**P1 改进建议**（非阻断）:
1. §1.2 per-CPU 数据的"cache 局部性"概念可补充一句"CPU 访问自己的数据命中 L1/L2 cache，访问其他 CPU 的数据需跨 NUMA 节点"——但这属于 OS 通用知识，读者应已具备，可不补
2. §1.6 硬件抽象的跨架构差异表已清晰，但可补充"为何 AP 启动是 x86-only"的理由（x86 的实模式→保护模式切换是历史包袱）——可在 Ch2 arch 部分补充

### 2.2 本质深度（Conceptual Depth）

**检查项**:
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] 每个决策有 ≥2 个被否决的选项 + 否决理由
- [x] 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] 决策之间有逻辑关系（D1 BKL→D3 释放模式→D4 类型见证）

**优秀点**:
- D3 BklGuard 非 RAII 的推理：从"RAII Drop 只在作用域结束触发"推出"无法在作用域中间释放"，逻辑严密
- D4 BklSection witness 是 Rust 独有设计（Capability pattern），非 C translate
- D7 SmpArch trait 的推理覆盖了 `#[cfg(target_arch)]` 的违反原则问题

**P1 改进建议**（非阻断）:
1. D8 per-CPU 索引的 ProcNr vs 裸指针推理中，可补充"ProcNr 是 newtype 包装的 u32，编译期防止与其他 u32 混淆"——但这是 Rust 通用模式，可在 Ch4 代码展示时体现
2. D9 单 CPU 退化的推理可补充"运行时 CAS 在单 CPU 时的实际开销：一次 compare_exchange 成功，约 10-20 cycles，可忽略"——但这是性能细节，非本质

### 2.3 概念覆盖（Concept Coverage）

**检查项**:
- [x] 知识点覆盖矩阵完整（A-F 六组 × Ch1-Ch5）
- [x] C 源码符号全部列出（smp.c 204 行 + smp.h + cpulocals.h）
- [x] 断裂修复表完整（9 处断裂 + 修复方案）
- [x] DEFERRED 函数诚实标注（10 个函数 + 理由）

**覆盖验证**（对照 structure.md 知识点）:
- A.0-A.7 BKL: ✅ §1.1 + §2.2 + D1/D3/D4 + §4.5 + test_bkl_*
- B.0-B.10 per-CPU: ✅ §1.2 + §2.3 + D2/D8 + §4.1 + test_cpu_local_*
- C.0-C.10 IPI: ✅ §1.3 + §2.4/§2.5 + D5/D6 + §4.4 + test_sched_ipi_*
- D.0-D.6 CPU 状态: ✅ §1.5 + §2.1 + D9 + §4.2/§4.3 + test_smp_state_*
- E.0-E.4 跨 CPU 调度: ✅ §1.4 + §2.5 + D7 + §4.7(DEFERRED) + §5.2
- F.0-F.4 arch 抽象: ✅ §1.6 + §2.5 + D7 + §4.6

**无遗漏**: structure.md 列出的 12 处知识点遗漏全部在 outline 中有对应章节处理（实现或 DEFERRED）。

### 2.4 组织合理性（Organizational Soundness）

**检查项**:
- [x] 章节顺序符合认知弧线（概念→源码→决策→实现→测试）
- [x] Ch2 每个符号带 file:line
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED
- [x] Ch5 测试函数可 grep 验证（`fn test_*`）
- [x] 参见形成闭环（11/14/15/10/22/17）

**优秀点**:
- Ch2 §2.6 调用关系图用时序图展示 BSP/AP 启动 + 运行时跨 CPU 操作，清晰直观
- Ch4 §4.7 DEFERRED 表格列出 10 个缺失函数 + 理由，诚实标注
- Ch5 §5.1 现有 18 个测试 + §5.2 待补充 5 个测试，对应关系明确

**无 P0 组织问题**。

---

## 三、执行注意事项（design 生成阶段关注）

1. **D3 BklGuard RAII 化**: design 需明确提供两个版本——`BklGuard`（非 RAII，用于 sendrecv 阻塞路径）+ `BklGuardRaii`（RAII，用于普通临界区）。参考 redox 的 `WaitContext` 模式。
2. **D7 SmpArch trait**: design 需定义完整 trait 签名 + 各 arch 实现方案。x86_64 的 `boot_ap` 用 INIT+SIPI，需引用 `arch/i386/smp.c`。
3. **§4.7 DEFERRED 函数**: design 需为每个 DEFERRED 函数提供实现方案（数据结构 + 算法 + 依赖），即使当前不实现。特别是 `smp_schedule_sync` 的 BKL release/reacquire + 重入处理逻辑。
4. **BKL 接入点**: design 需明确列出所有需接入 BKL 的访问点（proc_table/ipc/sched/irq_manager/clock），标注当前状态与待接入。

---

## 四、批准

✅ **批准**，进入 design 生成阶段（`design/16-design.md`）。

- P0: 0
- P1: 2（非阻断，design 阶段关注）
- 评审人: Trae (GLM-5.2)
- 评审日期: 2026-08-01
