# D02 VERIFY-CHECK（R5）

> 同 agent 验证局限：主 agent 与面板/certify 均为 deepseek-v4.1-flash 冷启动实例；无跨模型复核。本文只汇总可重放证据。

## 1. 机械门（最终工作树）

| 门 | 命令 | 读数 | 判定 |
|---|---|---|---|
| 引文 | `python3 tools/doc-citation-check.py <doc>` | blocks=41 checked=27 failed=0 skipped=9 warned=5 | PASS |
| 锚点 | `bash tools/anchor-resolve.sh --check <doc>` | anchors=66 resolved=66 zero-def=0 multi-def=0 | PASS |
| 结构 | `python3 tools/doc-structure-check.py <doc>` | 表与内部 .md 链接均可解析 | PASS |
| 引用 | `AWK=gawk bash tools/doc-ref-check.sh --check <doc>` | 内部章节引用均可解析 | PASS |
| 文风 | `bash tools/doc-style-lint.sh <doc>` | error 级 0 | PASS |
| 承载 | `bash tools/doc-code-density-check.sh --check <doc>` | 4 个可判章均有码/命令 | PASS |
| 关联代码 | `bash tools/doc-code-map.sh <doc> --check` | 30 个文件，30/30 存在 | PASS |
| 出版 | `python3 tools/doc-publication-check.py <doc>` | 无新增违规 | PASS |
| TODO | `python3 tools/todo-ledger.py check --doc <path> --fail-on-open` | 路径过滤 OK；按 doc 基名仍见外部哨兵 | PASS（带限制） |

## 2. 独立回读

| 回执 | 范围 | 初读 | 修复后 |
|---|---|---|---|
| `certify/round5-fact.md` | FACT-01..10 + CLAIM-01..06 + 附注 1..4 | CLOSED 17 / STILL-BROKEN 3 | 3/3 CLOSED（复读） |
| `certify/round5-reader.md` | READER-A/B/C 26 条 | CLOSED 23 / STILL-BROKEN 3 | 3/3 复读 CLOSED；26/26 |

## 3. 一致性

- 机械门一致性：9/9 PASS（TODO 带限制）。
- 面板发现覆盖率：42/42 有处置行（`eval-trace-check` coverage PASS）。
- 独立回读一致性：42/42（fact 16/16 + reader 26/26）CLOSED；`certify 新准入: 0`。

## 4. 修复后回读

- FACT 面：CLAIM-03、附注 2、附注 4 复读 CLOSED（3/3）；FACT/CLAIM/附注全部 CLOSED。
- READER 面：初读 3 条 STILL-BROKEN（B-02、B-08、B-15）全部修复，复读 3/3 CLOSED；最终 26/26。
- 结论：`certify 新准入: 0`；本轮可关闭。

## 5. 残余风险

- 面板与 certify 同模型；无跨模型复核。
- QEMU 未重跑；本轮只做文档修复。
- G11 code-publication（73 处存量）、G13 tutorial-rubric（6/6 章缺项）、`D02-EXTERNAL-D107` 外部哨兵不在本轮可修面。
