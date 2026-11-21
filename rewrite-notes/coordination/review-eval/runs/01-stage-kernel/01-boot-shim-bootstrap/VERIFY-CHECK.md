# D01 VERIFY-CHECK（R37）

> 同 agent 验证局限：主 agent 与面板/ certify 均为 deepseek-v4.1-flash 冷启动实例；没有跨模型复核。本文只汇总可重放证据，不把“一致”写成“无缺陷保证”。

## 1. 机械门（最终工作树）

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 引文 | `python3 tools/doc-citation-check.py <doc>` | blocks=48 checked=21 failed=0 skipped=23 warned=4 | PASS |
| 锚点 | `bash tools/anchor-resolve.sh --check <doc>` | anchors=89 resolved=89 zero-def=0 multi-def=0 | PASS |
| 结构 | `python3 tools/doc-structure-check.py <doc>` | 表与内部 .md 链接均可解析 | PASS |
| 引用 | `AWK=gawk bash tools/doc-ref-check.sh --check <doc>` | 内部章节引用均可解析 | PASS |
| 文风 | `bash tools/doc-style-lint.sh <doc>` | error 级 0 | PASS |
| 承载 | `bash tools/doc-code-density-check.sh --check <doc>` | 4 个可判章均有码/命令 | PASS |
| 关联代码 | `bash tools/doc-code-map.sh <doc> --check` | 48 个文件，48/48 存在 | PASS |
| 出版 | `python3 tools/doc-publication-check.py <doc>` | 无新增违规 | PASS |
| TODO | `python3 tools/todo-ledger.py check --doc <path> --fail-on-open` | 路径过滤 OK；按 doc 基名仍见外部哨兵 | PASS（带限制） |

## 2. 独立回读

| 回执 | 范围 | 初读 | 修复后 |
|---|---|---|---|
| `certify/round37-fact.md` | FACT-01..12 + X1..X6 | CLOSED 12 / STILL-BROKEN 6 | 6/6 复读 CLOSED |
| `certify/round37-reader.md` | READER-A/B/C 28 条 | CLOSED 25 / STILL-BROKEN 3 | 3/3 复读 CLOSED；28/28 |

## 3. 一致性

- 机械门一致性：9/9 PASS（TODO 带限制）。
- 面板发现覆盖率：46/46 有处置行（`eval-trace-check` coverage PASS）。
- 独立回读一致性：40 个 FINDING 全部 CLOSED（fact 12/12 + reader 28/28）；6 个 CLAIM 由 refute 原文裁决并 spot check；另 X1–X6 复核 6/6。`certify 新准入: 0`。

## 4. 修复后回读

- FACT 面：初读 6 条 STILL-BROKEN 全部修复，复读 6/6 CLOSED。
- READER 面：初读 3 条 STILL-BROKEN（B-03、B-10、C-07）全部修复，复读 3/3 CLOSED；最终 28/28。
- 结论：`certify 新准入: 0`；本轮可关闭。

## 5. 残余风险

- 面板与 certify 同模型；无跨模型复核。
- QEMU 未重跑；本轮只做文档修复。
- G11 code-publication（126 处存量）、G13 tutorial-rubric（6/6 章缺项）、`D01-EXTERNAL-D107` 外部哨兵不在本轮可修面。
