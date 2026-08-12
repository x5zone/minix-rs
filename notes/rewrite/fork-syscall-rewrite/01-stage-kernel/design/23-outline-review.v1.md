# 23-ipc-filter Outline Review v1

> AI 自审 outline.v1.md 的 4 维评估。P0=0 自动批准。

---

## 维度 1: 教学性（教学深度）

| Ch | 教学目标明确? | 动机说明? | 概念骨架? | 评 |
|----|-------------|----------|----------|-----|
| Ch1 | ✅ "三层过滤模型" | ✅ "为什么需要过滤（最小特权）" | ✅ 三层模型作为统一框架 | A |
| Ch2 | ✅ "C 源码分析" | ✅ Ch1 引入概念，本章分析 C 实现 | ✅ 按文件分层 | A |
| Ch3 | ✅ "Rust 设计决策" | ✅ 9 决策 hypothesis-driven | ✅ 决策表 | A |
| Ch4 | ✅ "实现要点" | ✅ Ch3 决策落地 | ✅ 5 实现要点 | A |
| Ch5 | ✅ "测试" | ✅ L1/L2/边界 | ✅ 三重覆盖 | A |
| Ch6 | ✅ "已知缺口" | ✅ 诚实标注 | ✅ 4 缺口 | A |
| Ch7 | ✅ "参见" | ✅ 4 引用闭环 | ✅ | A |

**判定**：✅ PASS — 全章教学目标明确，动机说明充分，概念骨架（三层模型）贯穿全文。

---

## 维度 2: 本质深度（design 视角）

| Ch3 决策 | 替代方案? | 拒绝理由? | ARCH 标注? |
|---------|----------|----------|----------|
| D1 内联函数 | 宏 | 类型安全 | N/A |
| D2 独立函数 | 内联 | 可测试性 | N/A |
| D3 u64 | [u32;2] | 58 syscall 足够 | N/A |
| D4 EPERM | panic | 对齐 C | N/A |
| D5 Option | type==IPCF_NONE | illegal states unrepresentable | N/A |
| D6 Option<usize> | 裸指针 | 避免 unsafe | N/A |
| D7 bitflags | 裸 u32 | 类型安全 + 可组合 | N/A |
| D8 enum | int | 穷尽匹配 | N/A |
| D9 DEFERRED IPC_STATUS | 实现 | 当前无 RECEIVE 路径 | N/A |

**判定**：✅ PASS — 每决策含替代方案 + 拒绝理由。无架构演进标注需求（不涉及 arch 差异）。

---

## 维度 3: 概念覆盖（Minix3 对齐）

| Minix3 概念 | outline 覆盖? | 位置 |
|------------|-------------|------|
| s_ipc_to 位图 | ✅ | Ch1.3, Ch2, Ch3.D3, Ch4.1 |
| s_k_call_mask | ✅ | Ch1.3, Ch2, Ch3.D3, Ch4.1 |
| s_ipcf 过滤链 | ✅ | Ch1.3, Ch2, Ch3.D5-D7, Ch4.3 |
| may_send_to | ✅ | Ch1.4, Ch2 |
| may_asynsend_to | ✅ | Ch1.4, Ch6 |
| allow_ipc_filtered_msg | ✅ | Ch2, Ch6 |
| CANRECEIVE/WILLRECEIVE | ✅ | Ch2 |
| IPC_STATUS | ✅ | Ch1.3, Ch3.D9, Ch6 |
| IPCF_POOL_* | ✅ | Ch2, Ch3.D5, Ch4.3 |
| IPCF_MATCH_M_SOURCE/M_TYPE | ✅ | Ch2, Ch3.D7 |
| ANY_USR/SYS/TSK | ✅ | Ch2, Ch6 |
| get_sys_bit/set_sys_bit | ✅ | Ch2, Ch4.2 |

**判定**：✅ PASS — 12 个 Minix3 概念全部覆盖。

---

## 维度 4: 组织合理性（13 项检查矩阵）

| # | 检查项 | 判定 |
|---|--------|------|
| 1 | 核心概念完整性 | ✅ 三层模型覆盖 |
| 2 | Ch1&2/Ch3&4 平衡 | ✅ 3:4（Ch1-2=11 知识点 / Ch3-4=14 知识点） |
| 3 | 设计决策集中度 | ✅ Ch3 集中 9 决策 |
| 4 | 章节依赖单向性 | ✅ Ch1→Ch2→Ch3→Ch4→Ch5 |
| 5 | 文档拆分判定 | ✅ 预计 < 500 行 |
| 6 | 跨文档引用一致性 | ✅ 22/12/17/13 引用闭环 |
| 7 | 索引完整性 | ✅ Ch7 参见 |
| 8 | 术语首次定义 | ✅ Ch1 引入 |
| 9 | 摘要/总结 | ✅ Ch1 概念 + Ch6 缺口 |
| 10 | 代码示例最小化 | ✅ Ch4 仅关键签名 |
| 11 | 引用 Minix3 源码 | ✅ Ch2/Ch4/Ch6 全 file:line |
| 12 | Design 引用 | ✅ design.v1.md 配套 |
| 13 | 架构演进标注 | N/A（无 arch 差异） |

**判定**：✅ PASS — 12/12 通过（1 项 N/A）。

---

## 总判定

**✅ PASS — P0=0，自动批准**。

outline.v1.md 可作为 23-ipc-filter.md 重写的结构契约。

### 改进建议（不阻断）

1. Ch1 可考虑用 Mermaid 流程图展示三层过滤时序
2. Ch3 D9 DEFERRED 应明确触发条件（RECEIVE 路径实现时）
3. Ch6 缺口表可加 "依赖" 列（如 allow_ipc_filtered_msg 依赖 12-ipc-core RECEIVE）
