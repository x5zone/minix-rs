# 19-syscall-signal-outline-review.v1.md — Outline 评审

> **评审对象**: `design/19-outline.v1.md`
> **评审方法**: 4 维自审（教学性 / 本质深度 / 概念覆盖 / 组织合理性）
> **判定标准**: P0=0 → 自动批准；P0>0 → 修订后重新评审
> **创建**: 2026-08-01
> **评审人**: Trae (GLM-5.2)

---

## 一、评审结论

| 维度 | 判定 | 说明 |
|------|------|------|
| 教学性 | ✅ PASS | Ch1 concept-driven，主语"信号/流程"，每节有"灵魂本质"+ WHY→WHAT→HOW 弧线 |
| 本质深度 | ✅ PASS | Ch3 hypothesis-driven，7 个决策均有"如果 X 会有 Y 问题所以用 Z"，含 `#[cfg(target_arch)]` 反例 |
| 概念覆盖 | ✅ PASS | A-G 七组知识点全覆盖；16 处断裂修复表完整 |
| 组织合理性 | ✅ PASS | 章节顺序符合认知弧线；Ch2 锚定 C 源码 + 8 字段行为契约；Ch4 真实代码 + DEFERRED 诚实标注；Ch5 测试可 grep |

**P0 计数**: 0
**P1 计数**: 3（非阻断，design 阶段关注）
**判定**: ✅ **自动批准**，可进入 design 生成阶段

---

## 二、详细评审

### 2.1 教学性（Teaching Quality）

**检查项**:
- [x] Ch1 主语是信号/流程，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1 内核信号路径为典型）
- [x] 新概念首次出现有定义（双路径/RTS_SIGNALED/RTS_SIG_PENDING/VMSUSPEND/s_sig_mgr）
- [x] 概念之间有因果链（双路径→时序约束→信号管理器角色）
- [x] 双向闭环完整性（内核信号路径三步闭环 + POSIX 信号路径 SIGSEND/SIGRETURN 双向）
- [x] 跨架构统一抽象先行（SignalContext trait 在 §1.2 引入概念，Ch3 D2/D6 展开设计）

**优秀点**:
- §1.1 把"内核通知 + SM 拉取 + SM 确认"抽象为三步闭环，建立心智模型清晰
- §1.3 VMSUSPEND 时序约束的幂等性分析（前 5 步幂等 / 第 6 步非幂等）有技术深度，非简单复述 WARNING 注释
- §1.4 SELF 路径 + 致命信号 panic 路径的因果链严密：从"SM 故障需 fallback"到"backup 切换 + RTS_NO_PRIV 清除"到"无 backup panic"

**P1 改进建议**（非阻断）:
1. §1.2 "POSIX 路径与内核信号路径的关系"段已补充，但可加一句"GETKSIG 是分叉点——SM 拉取信号后决定走 SIGSEND 还是直接处理"——但这是行为细节，可在 Ch2 §2.4 调用关系图体现
2. §1.3 VMSUSPEND 的"恢复后从入口重新执行"语义可补充一句"这是 Minix3 系统调用的通用重入语义，非 SIGSEND 独有"——但读者可从 Ch2 推断，可不补
3. §1.4 "SIGKSIGSM vs SIGKSIG" 区分可加一句"SIGKSIG 是外部 SM 通知（值=74，超出 _NSIG），SIGKSIGSM 是自通知（值不同）"——design 阶段需明确 SIGKSIGSM 数值

### 2.2 本质深度（Conceptual Depth）

**检查项**:
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] 每个决策有 ≥2 个被否决的选项 + 否决理由
- [x] 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] 决策之间有逻辑关系（D2 trait → D6 静态分发 → D3 SAFETY 约束）
- [x] 含 `#[cfg(target_arch)]` 反例推理（D6，模式 14）

**优秀点**:
- D1 SigSet newtype 的推理覆盖了 `bitflags!` 的"标志位集合 vs 信号集合"语义区分——这是 Rust 类型设计深度
- D3 VMSUSPEND 时序约束的"假设不保留"推理：从"VMSUSPEND 恢复后重新执行"推出"寄存器被多次修改"，再推出"完全避免 VMSUSPEND 不可行"——逻辑严密
- D4 cause_signal KProcess 方法的推理覆盖了"完全封装不可能"（需访问 PrivTable）的现实约束
- D6 `#[cfg(target_arch)]` 反例 + trait object 反例 + trait 静态分发正例的完整对比

**P1 改进建议**（非阻断）:
1. D5 GETKSIG 线性扫描的推理可补充"信号队列方案的状态管理复杂度：cause_sig 入队 + ENDKSIG 出队 + 队列一致性维护"——但这是性能 vs 复杂度的通用权衡，可在 design §3.5 展开
2. D7 信号常量 const vs enum 的推理可补充"Rust enum 的 `as i32` 在 FFI 边界的开销"——但这是 Rust 通用知识，可不补
3. D3 可补充"幂等性分析在 design §3.4 通过 SAFETY 注释体现"——这是 outline → design 的链路说明

### 2.3 概念覆盖（Concept Coverage）

**检查项**:
- [x] 知识点覆盖矩阵完整（A-G 七组 × Ch1-Ch5）
- [x] C 源码符号全部列出（5 个 do_*.c + system.c:386-449 cause_sig + sig_delay_done）
- [x] 断裂修复表完整（16 处断裂 + 修复方案）
- [x] DEFERRED 函数诚实标注（14 个 + 理由）
- [x] 8 字段行为契约完整（6 个核心函数 × 8 字段）

**覆盖验证**（对照 structure.md 知识点）:
- A.0-A.9 内核信号路径: ✅ §1.1 + §2.3 (do_kill/cause_sig/getksig/endksig) + D4/D5 + §4.4/§4.5 + test_getksig_*/endksig_*
- B.0-B.10 POSIX 信号路径: ✅ §1.2 + §2.3 (do_sigsend/sigreturn) + §2.2 + D2/D6 + §4.3/§4.6 (DEFERRED) + test_sigsend_*/sigreturn_*
- C.0-C.4 SIGSEND 时序: ✅ §1.3 + §2.3 (do_sigsend 注释) + §2.4 + D3 + §4.3 (真实代码)
- D.0-D.8 信号管理器: ✅ §1.4 + §2.1 (常量) + §2.3 (SELF 路径) + §4.4 (DEFERRED SELF) + (待补充测试)
- E.0-E.9 消息字段与常量: ✅ §2.1 + D7 + §4.1/§4.2 + test_sig_mask/test_nsig/test_signal_constants
- F.0-F.7 跨架构差异: ✅ §1.2 + §2.2 (sigcontext 字段) + D2/D6 + §4.6 (DEFERRED impl) + (arch 层测试)
- G.0-G.4 redox 对比: ✅ 附录（design 阶段补 redox 对比表）

**无遗漏**: structure.md 列出的 16 处知识点断裂全部在 outline 中有对应章节处理（实现或 DEFERRED）。

**8 字段行为契约验证**（6 个核心函数）:
- do_kill: ✅ 输入/输出/副作用/错误码/时序/状态前置/状态后置/竞争条件
- cause_sig: ✅ 完整 8 字段
- do_getksig: ✅ 完整 8 字段
- do_endksig: ✅ 完整 8 字段
- do_sigsend: ✅ 完整 8 字段（含 VMSUSPEND 重入语义）
- do_sigreturn: ✅ 完整 8 字段（含不可重入说明）

### 2.4 组织合理性（Organizational Soundness）

**检查项**:
- [x] 章节顺序符合认知弧线（概念→源码→决策→实现→测试）
- [x] Ch2 每个符号带 file:line
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED
- [x] Ch5 测试函数可 grep 验证（7 个 `fn test_*` + grep 命令）
- [x] 参见形成闭环（11/14/15/16/17/22）
- [x] §4.3 SIGSEND 时序用真实代码（非伪代码）—— 关键修复点
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无迭代叙事日期

**优秀点**:
- Ch2 §2.4 调用关系图用 ASCII 时序图展示双路径，清晰直观
- Ch4 §4.3 SIGSEND 时序约束用真实 syscall_signal.rs 代码（含 DEFERRED 标注）+ 完整实现方案对照——既诚实标注当前状态，又展示目标设计
- Ch4 §4.7 DEFERRED 表列出 14 个缺失功能 + C 位置 + Rust 位置 + DEFERRED 理由，诚实标注
- Ch5 §5.1 现有 7 个测试 + §5.2 待补充 9 个测试，对应关系明确 + grep 命令可验

**无 P0 组织问题**。

---

## 三、执行注意事项（design 生成阶段关注）

1. **D3 VMSUSPEND 时序约束**: design §3.4 需给出 `dispatch_sigsend<A: SignalContext>` 完整实现，含 `setup_handler_entry` 的 SAFETY 注释——明确约束"必须在最后一次 data_copy_vmcheck 之后"。SAFETY 注释需引用 do_sigsend.c:126-131 WARNING。

2. **D4 cause_signal 重构为 KProcess 方法**: design §3.2 需给出 `KProcess::cause_signal(&mut self, sig_nr, &mut PrivTable)` 方法签名 + 实现。当前 free function 保留为 dispatch_kill 的内部调用入口，或直接迁移到 KProcess。

3. **D6 SignalContext arch impl**: design §3.5 需定义 x86_64/aarch64 impl 方案（riscv64 标 DEFERRED，因 C 源码未实现）。x86_64 impl 需覆盖 `build_sigcontext`（gs/fs/es/ds/edi/.../eflags/esp/ss 字段）+ `setup_handler_entry`（sp=sigframe, pc=sighandler, fp=new_fp）+ `restore_sigcontext`（psw 用户位合并）。aarch64 impl 需覆盖 r0-r12/sp/lr/pc/spsr + lr=sigreturn + r0=signo + r2=sigctx + MF_CONTEXT_SET。

4. **cause_sig 缺失功能**: design §3.3 需给出三个缺失功能的实现方案：
   - 去重检查：`if !target.p_pending.contains(sig_nr) { ... }` (依赖 SigSet::contains 方法)
   - SELF 路径：`if sig_mgr == target.p_endpoint { sig_mgr_priv.s_sig_pending.add(sig_nr); send_sig(target.p_endpoint, SIGKSIGSM); return; }`
   - 致命信号 panic：`if sig_mgr == target.p_endpoint && is_lethal(sig_nr) { if let Some(backup) = backup_sig_mgr { ... } else { panic!(...); } }`

5. **SIGKSIGSM 常量**: design §3.1 需补充 `pub const SIGKSIGSM: u32 = ?`（需 grep minix3 signal.h 确认数值）。

6. **SIGS_IS_LETHAL 实现**: design §3.1 需给出 `pub fn is_lethal(sig_nr: u32) -> bool` 实现——列出致命信号集合（SIGKILL=9, SIGABRT=6, 等）。

7. **SC_MAGIC 常量**: design §3.1 需补充 `pub const SC_MAGIC: u32 = ?`（需 grep sigcontext.h 确认数值）。

8. **redox 对比表**: design 附录 B 需补充 redox 信号设计对比——`signal::SignalData` 结构 vs minix-rs SigSet 拆分 + `context::signal_handler` 直接修改 context vs SignalContext trait。

---

## 四、6 维反查矩阵（Step 0.5.6）

| # | 维度 | 反查对象 | 偏离类型 | 判定 |
|---|------|---------|---------|------|
| 1 | outline ↔ 文档正文 | 19-outline.v1.md vs 19-syscall-signal.md 当前正文 | 遗漏（Ch3 hypothesis 缺）/ 多余（§补充 tmp-15）/ 顺序错位（§1.3 实现驱动） | 🔴 P1（修复方案在断裂修复表） |
| 2 | design ↔ Rust 代码 | 待 design 生成后查 | — | 🟢 待 design 生成 |
| 3 | design ↔ Minix3 C 源码 | outline 已锚定 5 个 do_*.c + system.c:386-449 | 🟢 一致 | 🟢 共识一致 |
| 4 | outline ↔ design | 待 design 生成后查 | — | 🟢 待 design 生成 |
| 5 | 文档 ↔ 代码（横向）| 19-syscall-signal.md vs syscall_signal.rs | Ch4 §4.3 伪代码 vs 真实 DEFERRED 状态 | 🔴 P1（修复方案在断裂修复表） |
| 6 | 元层反查 | 19-outline.v1.md（首次生成）| — | 🟢 首次无对比 |

**反查覆盖率**: 6/6 维度全部输出 ✅
**通过门槛**: 🔴 直接判定中 P0 = 0；🟢 共识一致项 ≥ 50% ✅

---

## 五、章节意图分析（Step 0.5.7）

### 多余章节：当前文档 §补充（来源：tmp-15-syscall-exit-signal.md）

- **试图强调什么**（教学意图）: 补充信号管理器、致命信号、消息字段、信号处理帧等细节
- **试图解释什么**（内容意图）: 把 tmp-15 中的"信号处理详细分析"内容并入 19 文档
- **是否与 outline 教学目标对齐**: ⚠️ 偏离——outline 已将这些内容拆解到 Ch1.4（信号管理器）/ Ch2.1（常量）/ Ch2.2（数据结构）/ Ch4（实现）；§补充作为"补充章节"违反组织合理性
- **判定**: ⚠️ 偏离 → 删除 §补充，内容按 outline 拆解到对应章节

---

## 六、批准

✅ **批准**，进入 design 生成阶段（`design/19-design.md`）。

- P0: 0
- P1: 3（非阻断，design 阶段关注）
- 评审人: Trae (GLM-5.2)
- 评审日期: 2026-08-01
