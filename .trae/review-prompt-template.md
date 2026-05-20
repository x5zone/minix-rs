# Review Prompt Template

> 使用方法：将 `TARGET` 替换为文档名（如 `14-cow-mechanism`），然后复制整段到对话框。
> 本项目文档均 >1000 行，统一使用分阶段版。

---

## 分阶段 Review（5 步，H→I→J→K→Cross）

```
按以下顺序 review TARGET=14-cow-mechanism.md 和它关联的 rust 代码：

第一步：调用 review-profiles skill，按 Profile H 对 TARGET.md 做 Ch1&2 准确性验证。
开始前，先执行 review-doc Step 2（Diff Extraction）：找出文档与 Minix3 源码偏差最大的 3 处，作为后续重点检查区域。
输出所有 P0/P1/P2 问题。

第二步：调用 review-profiles skill，按 Profile I 对 TARGET.md 做 Ch3&4 设计质量验证（基于第一步结论）。
输出所有 P0/P1/P2 问题。

第三步：调用 review-profiles skill，按 Profile J 对 TARGET.md 关联的 Rust 代码做代码质量验证（基于第二步结论）。
输出所有 P0/P1/P2 问题。

第四步：调用 review-profiles skill，按 Profile K 对 TARGET.md 做跨文档+可读性验证（基于前三步结论）。
输出所有 P0/P1/P2 问题。

第五步：调用 review-cross skill，交叉验证 TARGET.md 与关联 Rust 代码的双向一致性（基于前四步结论）。
重点做 Code→Doc 方向：检查 Rust 代码中的关键实现是否在文档中有对应描述。
输出所有 P0/P1/P2 问题。

所有 P0/P1/P2 问题都需要修复，修复后编译验证。
```

---

## 依赖关系

```
Step 1: Profile H + Diff Extraction    ≈ review-doc 前半 (Step 1-5.6)
    │   Profile H 覆盖: 概念准确性、C引用验证、C源覆盖、数据结构、架构演进
    │   额外补充: review-doc Step 2 Diff Extraction（Profile 未包含）
    ▼
Step 2: Profile I                      ≈ review-doc 后半 (Step 6-8, 10)
    │   覆盖: 设计决策质量、章节链接、Doc→Code一致性、教学质量
    │   缺口: 不做 Code→Doc 方向（由 Step 5 补充）
    ▼
Step 3: Profile J                      = review-code 全部 (Step 0-17)
    │   覆盖: 重写质量、硬件抽象、类型安全、C-Rust对齐、no_std、
    │         模块设计、命名、注释、64位、架构演进、执行模型、
    │         内存模型、测试、复杂度、设计-代码一致性
    │   无缺口，甚至多出 J15 (Design-Code Consistency)
    ▼
Step 4: Profile K                      ≈ review-doc Step 11-12
    │   覆盖: 跨文档一致性、可读性
    ▼
Step 5: review-cross                   补充 Profile I 的 Code→Doc 缺口
                                        双向一致性: Doc→Code + Code→Doc
```

---

## 覆盖对照表

### review-doc 覆盖情况

| review-doc 步骤 | 内容 | 本模板覆盖位置 | 差异 |
|---|---|---|---|
| Step 1: Ground Truth | C 源文件定位 | Step 1 (H Preparation) | ✅ |
| Step 2: Diff Extraction | Top 3 偏差定位 | Step 1 额外补充 | ✅ 已补 |
| Step 3: Concept Accuracy | 概念准确性 | Step 1 (H1) | ✅ |
| Step 4: C Ref Verification | C 引用验证 | Step 1 (H2) | ✅ |
| Step 5: C Source Coverage | C 源覆盖 | Step 1 (H8) | ✅ |
| Step 5.5: Data Struct | 数据结构覆盖 | Step 1 (H5) | ✅ |
| Step 5.6: Arch Evolution | 架构演进标注 | Step 1 (H6) | ✅ |
| Step 6: Design Decision | 设计决策质量 | Step 2 (I2) | ✅ |
| Step 7: Chapter Linkage | 章节链接 | Step 2 (I3) | ✅ |
| Step 8: Doc-Code Consistency | 文档↔代码一致性 | Step 2 (I1) + Step 5 | ⚠️ I1 只做 Doc→Code，Step 5 补 Code→Doc |
| Step 9: Diagram Quality | 图表质量 | Step 1 (H7) | ✅ |
| Step 10: Pedagogical Quality | 教学质量 | Step 2 (I4) | ✅ |
| Step 11: Readability | 可读性 | Step 4 (K) | ✅ |
| Step 12: Cross-Document | 跨文档检查 | Step 4 (K1) | ✅ |
| Step 13-15: Output | 输出格式 | 各 Step 输出 | ✅ |

### review-code 覆盖情况

| review-code 步骤 | 内容 | Profile J 维度 | 差异 |
|---|---|---|---|
| Step 0: Scope | 范围声明 | J Preparation | ✅ |
| Step 1: Rewrite Quality | 翻译味检查 | J1 | ✅ |
| Step 2: Hardware Abstraction | 硬件抽象 | J2 | ✅ |
| Step 3: Type Safety | 类型安全 | J3 | ✅ |
| Step 4: C-Rust Alignment | C-Rust语义对齐 | J4 | ✅ |
| Step 5: no_std Compliance | no_std合规 | J5 | ✅ |
| Step 6: Module Design | 模块设计 | J6 | ✅ |
| Step 7: Naming & Traceability | 命名与可追溯 | J7 | ✅ |
| Step 8: Comment Quality | 注释质量 | J8 | ✅ |
| Step 9: 64-bit Assumptions | 64位假设 | J9 | ✅ |
| Step 10: Architecture Evolution | 架构演进合规 | J10 | ✅ |
| Step 11: Execution Model | 执行模型 | J11 | ✅ |
| Step 12: Memory Model | 内存模型 | J12 | ✅ |
| Step 13: Tests | 测试 | J13 | ✅ |
| Step 14: Complexity | 复杂度 | J14 | ✅ |
| Step 15-17: Output | 行动项/自检/输出 | J 输出格式 | ✅ |
| — | 设计-代码一致性 | J15 | ✅ Profile J 额外多出 |

结论：5 步模板完整覆盖 review-doc + review-code + review-cross，无遗漏。
