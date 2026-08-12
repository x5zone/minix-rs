# 18-outline-review.md — Outline 评审

> **评审对象**: `design/18-outline.md`
> **评审方法**: 4 维自审（教学性 / 本质深度 / 概念覆盖 / 组织合理性）
> **判定标准**: P0=0 → 自动批准；P0>0 → 修订后重新评审
> **创建**: 2026-08-01

---

## 一、评审结论

| 维度 | 判定 | 说明 |
|------|------|------|
| 教学性 | ✅ PASS | Ch1 concept-driven，主语"内存/安全"，每节有"灵魂本质"+ WHY→WHAT→HOW 弧线；信任/不信任矛盾贯穿全篇 |
| 本质深度 | ✅ PASS | Ch3 hypothesis-driven，9 个决策均有"如果 X 会有 Y 问题所以用 Z"；D1 Direct Map 推理从 createpde 临时映射开销推出 Direct Map |
| 概念覆盖 | ✅ PASS | A-H 八组知识点全覆盖；断裂修复表完整（9 处断裂 + 修复方案）；DEFERRED 函数诚实标注（12 个 + 共同 blocker） |
| 组织合理性 | ✅ PASS | 章节顺序符合认知弧线（概念→源码→决策→实现→测试）；Ch2 锚定 C 源码 file:line；Ch4 真实代码+DEFERRED 诚实标注；Ch5 测试函数可 grep |

**P0 计数**: 0
**P1 计数**: 3（非阻断，design 阶段关注）
**判定**: ✅ **自动批准**，可进入 design 生成阶段

---

## 二、详细评审

### 2.1 教学性（Teaching Quality）

**检查项**:
- [x] Ch1 主语是内存/安全，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch1 采用 WHY→WHAT→HOW 弧线（§1.1 vircopy、§1.2 safecopy、§1.5 Direct Map 为典型）
- [x] 新概念首次出现有定义（如 SELF/grant 表/verify_grant/Direct Map/VMSUSPEND）
- [x] 概念之间有因果链（vircopy 信任→safecopy 不信任→grant 表验证→Direct Map 简化拷贝）

**优秀点**:
- §1.1 vircopy 的"信任调用者"定位清晰：从"系统进程需要跨空间拷贝"到"内核仅做最小验证"再到"do_copy 实现"
- §1.2 safecopy 的 grant 验证流程 11 步列举完整，从 endpoint 验证到 magic grant 重定向，逻辑严密
- §1.5 Direct Map 的 WHY→WHAT→HOW 弧线清晰：从"32 位 createpde 临时映射开销"到"64 位 Direct Map 一行加法"再到"kernel_phys_to_virt 实现"
- §1.5 Direct Map 演进表含"当前状态"列，诚实标注 DEFERRED——不假装已实现

**P1 改进建议**（非阻断）:
1. §1.3 umap/vumap 合并为一节，但 vumap 的 DMA 用途可补充一句"批量映射减少逐个 grant 映射的系统调用开销"——但这是性能细节，读者可从批量语义推断，可不补
2. §1.4 memset 的 pattern 截断（int → byte）可在 Ch4 代码展示时体现，Ch1 概念层可不涉及
3. §1.5 VMSUSPEND 协议仅在 Direct Map 限制中提及，可补充一句"缺页时内核挂起源/目标进程，通知 VM 换入页面后恢复拷贝"——但 VMSUSPEND 详细机制属 24-cross-space-runtime，本文档引用即可

### 2.2 本质深度（Conceptual Depth）

**检查项**:
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] 每个决策有 ≥2 个被否决的选项 + 否决理由
- [x] 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] 决策之间有逻辑关系（D1 Direct Map→D7 vm_memset/vm_lookup 保留→D5 栈数组）

**优秀点**:
- D1 Direct Map 的推理：从"createpde 临时映射开销大"和"64 位下不必要"推出"Direct Map 一行加法"——hypothesis-driven 范式典型
- D2 grant 表访问的推理：从"内核直接读用户空间 grant 表可能递归 VMSUSPEND"推出"data_copy 拷入内核缓存"——揭示了递归 VMSUSPEND 的本质问题
- D3 GrantVerifyResult 结构体的推理：从"多个输出参数类型不安全"推出"结构体替代"——Rust 惯用法
- D9 Option<SoftFaultInfo> 的推理：从"sfinfo 总是存在但大部分场景不使用"推出"Option 表达可能不存在"——anti-translate 典型
- D8 合并 do_umap 的推理：从"C 的 #if 条件编译"到"Rust 不需要"——架构演进标记

**P1 改进建议**（非阻断）:
1. D6 CP_FLAG_TRY 保留的推理可补充"VFS 内存映射文件场景的具体死锁路径"——但这属于 VFS 实现细节，本文档引用即可
2. D5 栈数组的推理可补充"MAPVEC_NR 的具体值"——但这是实现细节，Ch4 代码展示时体现

### 2.3 概念覆盖（Concept Coverage）

**检查项**:
- [x] 知识点覆盖矩阵完整（A-H 八组 × Ch1-Ch5）
- [x] C 源码符号全部列出（7 个 .c 文件全量行号）
- [x] 断裂修复表完整（9 处断裂 + 修复方案）
- [x] DEFERRED 函数诚实标注（12 个函数 + 共同 blocker）

**覆盖验证**（对照 structure.md 知识点）:
- A.0-A.6 vircopy/physcopy: ✅ §1.1 + §2.1 + D1/D6/D8 + §4.2 + test_dispatch_copy_*
- B.0-B.11 safecopy: ✅ §1.2 + §2.2 + D3/D4/D6/D9 + §4.3 + test_dispatch_safecopy_*
- C.0-C.6 umap: ✅ §1.3 + §2.3 + D8 + §4.4 + test_dispatch_umap_*
- D.0-D.5 vumap: ✅ §1.3 + §2.4 + D5 + §4.5 + test_dispatch_vumap_*
- E.0-E.3 memset: ✅ §1.4 + §2.5 + D7 + §4.6 + test_dispatch_memset_*
- F.0-F.6 Direct Map: ✅ §1.5 + §2.6 + D1 + §4.7 + test_virtual_copy_vmcheck_*
- G.0-G.2 VMSUSPEND: ✅ §1.5 提及 + §4.8 DEFERRED 表
- H.0-H.2 redox 对比: ✅ 附录（design.md 补充）

**关键修复验证**:
- ✅ §3 D1 巨型 DEFERRED 块已拆分：D1 仅保留设计决策，DEFERRED 状态移至 Ch4 §4.8
- ✅ P1-05/P0-10/P0-02 内部 ID 已删除
- ✅ §6 "来源：tmp-13" 已删除
- ✅ 测试 bullet 改为 55 个可 grep 函数名
- ✅ "补充"节 VMCTL 内容已删除（属 20-syscall-device）
- ✅ Direct Map DEFERRED 诚实标注（演进表含"当前状态"列）

**无遗漏**: structure.md 列出的 13 处知识点遗漏全部在 outline 中有对应章节处理（实现或 DEFERRED）。

### 2.4 组织合理性（Organizational Soundness）

**检查项**:
- [x] 章节顺序符合认知弧线（概念→源码→决策→实现→测试）
- [x] Ch2 每个符号带 file:line
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED
- [x] Ch5 测试函数可 grep 验证（`fn test_*`）
- [x] 参见形成闭环（16/17/20/24）

**优秀点**:
- Ch2 §2.6 调用关系图用时序图展示 vircopy/safecopy 拷贝流程，清晰直观
- Ch4 §4.8 DEFERRED 表格列出 12 个缺失函数 + 理由 + 共同 blocker，诚实标注
- Ch5 §5.1 现有 55 个测试按 9 个类别分组 + §5.2 待补充 8 个测试，对应关系明确
- 断裂修复表 9 处断裂全部对应到具体修复方案

**无 P0 组织问题**。

---

## 三、执行注意事项（design 生成阶段关注）

1. **D1 Direct Map DEFERRED blocker**: design 需明确说明 `virt_to_phys` PTE walk 是 5 处核心路径的共同 blocker。演进表保留为设计目标，但必须诚实标注当前实现状态（primitive 已实现，跨进程 PTE walk DEFERRED）。
2. **D3 GrantVerifyResult 结构体**: design 需贴完整 struct 定义 + 与 C 多输出参数的对照。`sfinfo: Option<SoftFaultInfo>` 体现 anti-translate。
3. **§4.8 DEFERRED 表**: design 需为每个 DEFERRED 函数提供实现方案（数据结构 + 算法 + 依赖），即使当前不实现。特别是 verify_grant 的 11 步验证流程。
4. **Direct Map 演进表**: design 需保留演进表（设计目标），但每行增加"当前状态"列诚实标注。禁止假装 Direct Map 已完全实现。
5. **VMCTL 删除**: design 中不得出现 VMCTL 内容——VMCTL 属 20-syscall-device.md。
6. **redox 对比**: design 需在附录补充 redox 跨空间拷贝对比（redox 用 `paging::map_physical` + `copy_to_user`）。
7. **测试函数名**: design 的测试策略需列出实际可 grep 的 `fn test_*` 函数名，不得用描述性 bullet。

---

## 四、批准

✅ **批准**，进入 design 生成阶段（`design/18-design.md`）。

- P0: 0
- P1: 3（非阻断，design 阶段关注）
- 评审人: Trae (GLM-5.2)
- 评审日期: 2026-08-01
