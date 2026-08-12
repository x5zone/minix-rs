# 17-syscall-process-outline-review.v1.md — Outline 评审

> **评审对象**: `design/17-outline.md`（亦对照 `.design/17-outline.v1.md` 语义）
> **评审方法**: 4 维自审（教学性 / 本质深度 / 概念覆盖 / 组织合理性）
> **判定标准**: P0=0 → 自动批准；P0>0 → 修订后重新评审
> **依据**: `17-syscall-process-glm-structure.md`（知识点全集 + 诊断）
> **创建**: 2026-08-01

---

## 一、评审结论

| 维度 | 判定 | 说明 |
|------|------|------|
| 教学性 | ✅ PASS | Ch1 concept-driven，进程状态机视角，每节有"灵魂本质"+ WHY→WHAT→HOW 弧线 |
| 本质深度 | ✅ PASS | Ch3 hypothesis-driven，D1-D6 均有"如果 X 会有 Y 问题所以用 Z"+ ≥2 被否决选项 |
| 概念覆盖 | ✅ PASS | A-K 十一组知识点全覆盖；断裂修复表完整（9 处）；DEFERRED 10 项诚实标注 |
| 组织合理性 | ✅ PASS | 章节顺序符合认知弧线；Ch2 锚定 C 源码；Ch4 真实代码+DEFERRED 表；Ch5 可 grep |

**P0 计数**: 0
**P1 计数**: 3（非阻断，design 阶段关注）
**判定**: ✅ **自动批准**，可进入 design 生成阶段

---

## 二、详细评审

### 2.1 教学性（Teaching Quality）

**检查项**:
- [x] Ch1 主语是进程（状态机视角），非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1 生命周期 / §1.2 同步 fork / §1.3 权限降级 / §1.4 SMP 停止 均有）
- [x] 新概念首次出现有定义（如同步 fork / endpoint 代际 / 权限降级 / SMP IPI 同步）
- [x] 概念之间有因果链（生命周期→同步 fork→权限降级→SMP 停止）

**优秀点**:
- §1.1 把 7 条系统调用收束为"生命周期状态机"4 条核心弧（fork/exec/exit/clear）+ 3 条控制弧（runctl/schedctl/statectl），避免 feature-listing，建立了进程作为状态机引擎的心智模型
- §1.2 同步 fork 的 WHY 推理：从"子进程返回值=0 需父的消息缓冲"推出"父必须 RECEIVING"，因果严密——这是 C 源码 do_fork.c:51 注释"needs to be receiving so we know where the message buffer is"的本质提炼，非表面翻译
- §1.3 权限降级用安全视角（"任何用户进程都能通过 fork 提权"）解释为何不继承，比 C 注释更本质
- §1.4 SMP 停止用竞态视角（"目标 CPU 仍可能用陈旧上下文执行一个时钟周期"）解释为何不能直接改标志，呼应 16-smp 的 BKL/IPI 体系

**P1 改进建议**（非阻断）:
1. §1.1 的状态机图用 ASCII 描绘"未出生→就绪→停止→回收"的转换，但未显式标注每条弧对应的系统调用编号（SYS_FORK 等）——可在图中弧上补 `SYS_FORK`/`SYS_EXEC` 标签，便于读者映射到 Ch2。但这是可读性优化，非本质缺失
2. §1.2 endpoint 代际的"防复活"概念可补一句"旧代 endpoint 的 IPC 消息若投递到新代进程会错乱"——但这是 IPC 模块职责，读者可从 22-privilege / IPC 文档补全，可不补

### 2.2 本质深度（Conceptual Depth）

**检查项**:
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] 每个决策有 ≥2 个被否决的选项 + 否决理由
- [x] 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] 决策之间有逻辑关系（D1 fork_from→D2 Endpoint→D5 KcallResult 都围绕"用类型系统表达 C 语义"）

**优秀点**:
- D1 fork_from 的推理：从"`KProcess` 含 AtomicU8 非 Copy"+"clone 后改 8+ 字段易遗漏"推出"专门构造函数 + 编译器保证字段不漏"——抓住了 C `*rpc = *rpp` 在 Rust 下不可直接翻译的本质（所有权 + 非 Copy 字段）
- D2 Endpoint newtype 的推理覆盖了 `type` 别名（仅文档作用）与 newtype（编译期防护）的区别，且点明 `repr(transparent)` 的 ABI 兼容理由——这是 anti-translate 的正确表达
- D3 StatectlRequest enum 的推理点出"编译器强制穷尽检查"，比 C switch/case 的 default 更安全
- D4 Option 替代 -1 sentinel 的推理保留了 C 的 -1 语义兼容（消息层仍 i32），同时类型层用 Option——平衡了 anti-translate 与 C 对齐
- D5 KcallResult enum 把 EDONTREPLY/errno/返回值区分为不同变体，比 C 的范围判断更本质
- D6 诚实指出 ProcNr 当前是类型别名（弱 anti-translate），建议升级——这是对 ground truth 的忠实反映，非粉饰

**P1 改进建议**（非阻断）:
1. D6 ProcNr 升级为 newtype 是"建议"而非"已落地"，design 需明确给出升级方案（struct 定义 + From 转换 + 影响点清单）+ 决策"是否本轮升级"。若不升级需说明理由（如改动面过大推迟）。design 阶段需给出明确结论
2. D5 KcallResult 的推理中"用 Result<i32, Errno>"被否决的理由"EDONTREPLY 不是错误"可补一句"EDONTREPLY 是合法的控制流（exit 故意不回复），非异常"——但当前表述已足够，可不补

### 2.3 概念覆盖（Concept Coverage）

**检查项**:
- [x] 知识点覆盖矩阵完整（A-K 十一组 × Ch1-Ch5）
- [x] C 源码符号全部列出（7 个 do_* + 辅助符号，全部 file:line）
- [x] 断裂修复表完整（9 处断裂 + 修复方案）
- [x] DEFERRED 函数诚实标注（10 项 + 理由）

**覆盖验证**（对照 structure.md 知识点）:
- A. LC 生命周期: ✅ §1.1 + §2.1-2.4 + D1 + §4.1-4.4 + test_dispatch_clear_*
- B. SF 同步 fork: ✅ §1.2 + §2.1 + D1 + §4.1 + (§5.2 待补)
- C. EG endpoint 代际: ✅ §1.2 + §2.1 + D2 + §4.1 + (§5.2 待补)
- D. PD 权限降级: ✅ §1.3 + §2.1 + §4.1 + (§5.2 待补)
- E. IR 映像替换: ✅ §1.1 + §2.2 + §4.2 + test_dispatch_exec_*
- F. SS 自杀信号: ✅ §1.1 + §2.3 + D5 + §4.3 + test_dispatch_exit_*
- G. SR 槽位回收: ✅ §1.1 + §2.4 + §4.4 + test_dispatch_clear_*
- H. RF 停止/恢复: ✅ §1.4 + §2.5 + §4.5 + test_dispatch_runctl_*
- I. SC 调度权移交: ✅ §1.1 + §2.6 + D4 + §4.6 + test_dispatch_schedctl_*
- J. ST IPC 状态控制: ✅ §1.1 + §2.7 + D3 + §4.7 + test_dispatch_statectl_*
- K. ProcNr newtype: ✅ D6（建议）+ design 落地

**无遗漏**: structure.md 列出的 14 处知识点遗漏全部在 outline 中有对应章节处理（实现或 DEFERRED 或 design 建议）。

**P1 改进建议**（非阻断）:
1. fork 完全无测试（structure.md §5 已标注 P1 测试缺口），outline §5.2 列了 3 个待补测试，但 design 需明确"是否本轮补 fork 测试"——若 fork 实现已完整（dispatch_fork ✅），建议本轮补测试。design 阶段确认

### 2.4 组织合理性（Organizational Soundness）

**检查项**:
- [x] 章节顺序符合认知弧线（概念→源码→决策→实现→测试）
- [x] Ch2 每个符号带 file:line
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED
- [x] Ch5 测试函数可 grep 验证（`fn test_*`，22 个）
- [x] 参见形成闭环（11/16/06/10/14/22）

**优秀点**:
- Ch2 按 do_fork→do_exec→do_exit→do_clear→do_runctl→do_schedctl→do_statectl 顺序，与 Ch1 §1.1 生命周期弧顺序一致（创建→替换→退出→回收→控制），认知连贯
- Ch2 §2.8 调用关系图用 ASCII 时序展示 fork 同步点 + clear 幂等回收，直观
- Ch4 §4.8 DEFERRED 表列出 10 项 + 理由，诚实标注（不写 stub 隐藏）
- Ch5 §5.1 现有 22 个测试 + §5.2 待补充 5 个测试，对应关系明确
- 断裂修复表把 6 类诊断问题（迭代叙事/tmp/stub/测试/Ch1/Ch3）+ ProcNr 弱 anti-translate + fork 无测试 全部映射到修复方案

**无 P0 组织问题**。

**P1 改进建议**（非阻断）:
1. Ch6 参见中 [22-privilege.md] 尚未存在（USER_PRIV_ID / SYS_PROC 归属），但 outline 引用合理——这是前向引用，待 22 文档创建后补全，非本轮阻断
2. Ch1 §1.4 SMP 停止与 16-smp 的 IPI 协议有重叠——outline 用"详见 16-smp §1.3"引用，避免重复，组织合理

---

## 三、执行注意事项（design 生成阶段关注）

1. **D6 ProcNr newtype 升级决策**: design 需明确给出结论——本轮升级或推迟。若升级：提供 `#[repr(transparent)] pub struct ProcNr(pub i32)` 定义 + `From<i32>`/`Into<i32>` 转换 + 影响点清单（grep `ProcNr` 使用点）。若推迟：说明改动面理由，并在 design §4 限制中记录为已知技术债
2. **DEFERRED 10 项的实现方案**: design 需为每个 DEFERRED 项提供实现路径（依赖哪个 trait/模块），即使当前不实现。特别是：exec 的 `ArchProcInit` trait 签名、clear 的 VM/IRQ/IPC/timer 释放接口、runctl 的 `SmpArch::schedule_stop_proc` 接入、statectl 的 `data_copy_vmcheck` + IPC engine
3. **fork 测试补全**: design 需明确是否本轮补 3 个 fork 测试（§5.2）。dispatch_fork 已完整实现，建议补测试覆盖：创建子+新 endpoint / 父非 RECEIVING→EINVAL / SYS_PROC 降级
4. **no_std 约束**: design §1.2 约束需明确 `#![no_std]`（除 `#[cfg(test)]`），所有数据结构无 `alloc` 依赖（KProcess 含 AtomicU8/AtomicU32，无需堆分配）
5. **redox 对照**: design 附录可补 redox `context::Context` + `scheme::proc` 与 Minix-RS `KProcess` + 显式 syscall 的设计对照，作为 anti-translate 的外部参照
6. **anti-translate 一致性**: design §2 数据结构需对齐 outline D1-D6 决策——`KProcess::fork_from`、`Endpoint` newtype、`StatectlRequest` enum、`SchedParams` Option 字段、`KcallResult` enum 全部在 design 中给出真实 Rust 定义

---

## 四、批准

✅ **批准**，进入 design 生成阶段（`design/17-design.md`）。

- P0: 0
- P1: 3（非阻断，design 阶段关注：D6 ProcNr 决策 / fork 测试补全 / DEFERRED 实现路径）
- 评审人: Trae (GLM-5.2)
- 评审日期: 2026-08-01
