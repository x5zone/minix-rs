# 执行指令：D01–D06 全量收敛验证（V3.5 制品链）

> 将此文件整体作为 prompt 交给新会话的 agent 执行。所有规则源、工具、账本均在仓库内，无需外部上下文。

## 你是什么

你是 review-loop 流程的执行者。你的任务是把 D01–D06 六篇文档**依次序**跑完完整的 review 循环，确认或达成收敛。

## 第零步：环境确认

```bash
cd /home/xzhao/github/minix-rs
git log --oneline -1
git status --porcelain | grep -v '^??' | wc -l  # 期望 0
```

按顺序读以下文件（这是你的唯一规则入口，不要凭记忆）：

1. `AGENTS.md` — 项目指令入口（只看 Build & Test 与 Review Workflow 两节）
2. `prompt/skill/review-loop.md` — 主流程 L0–L7
3. `prompt/review-rules/review-evals.md` — 轨迹/指标/处置/收敛契约
4. `prompt/review-rules/review-panel.md` — 面板协议（档位表 §七）
5. `prompt/review-rules/fix-guard.md` — 修复守则
6. `rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md` — 版本链（末尾三节：V3.4/V3.4-r26/V3.5）
7. `rewrite-notes/coordination/review-eval/DECISIONS.md` — 判决账（D-055/D-056/D-057/D-058）
8. `rewrite-notes/coordination/review-eval/CORPUS.md` — 语料条目

**当前流程版本**：V3.5（= V3.3 + D-055 两墙 + V3.4 最小内核 + r7–r27 修正 + 制品链）。

## 核心规则速记

| 规则 | 要点 |
|---|---|
| schema 标记 | **新 review 轮写 `schema: V3.4 from R<N>`**（旧标记不追溯不改动） |
| 制品链 | 面板/回执入库 `runs/<stage>/<doc>/panel/round<N>/` + `certify/round<N>-*.md`；用 `python3 tools/panel-index.py <dir>` 生成 INDEX.md |
| 读者 A/B/C | ≥500 行文档必须独立（D-055 墙 1） |
| certify 独立冷读 | grep 归零不算回读；回执必须来自独立子 agent（D-055 墙 2） |
| 修改轮 | 只能出 ROUND-CLOSED / CONVERGENCE-CANDIDATE |
| 确认轮 | 全量 survey + certify 新准入=0 → CONVERGED；每个 CANDIDATE 最多 2 次 |
| FREEZE 后 | 独立回归审计（§4h）→ REGRESSION.md 入账 → 通过后进下一篇 |
| BLOCKED | 第 3 次 certify → STUCK-REPORT → 两级分流解除 |
| 面板档位 | 唯一源 = review-panel.md §七；≥500 行读者独立；≥2000 行代码跑 D1+D2 |
| 形状契约 | `##/### SUGGESTION <ASCII-ID>`（与 FINDING 同规则，必须带 ID） |

## 执行序列

### D01（01-boot-shim-bootstrap.md，3579 行，已 FREEZE——确认轮）

已有 FREEZE（V3.3 口径）。当前任务：**在 V3.5 下做一次完整确认轮**，确认真实收敛。

1. `loop.md` 追加新确认轮节（`schema: V3.4`，`轮型: confirmation`）
2. 全量六角色 survey（独立读者 A/B/C + FACT + DESIGN-D2 + REFUTE）
3. certify 独立冷读 + 回执入库
4. `--round R<new>` PASS → CONVERGED → 写 FREEZE 追加节 → 进 D02

### D02–D04（已 FREEZE——确认轮）

同 D01 流程。每篇一个确认轮。

### D05（05-clock-interrupt-init.md，2198 行，未 FREEZE——完整循环）

V3.3 时代 R1 基线已立但面板未跑。当前任务：**从 R1 开始完整循环直到收敛**。

1. `loop.md §0` 更正 schema 为 `V3.4 from R1`，记 VARIANCE
2. L0 → survey → inventory → fix → certify → 确认轮 → FREEZE → 回归审计
3. 预期 ≤5 轮收敛；超过则按 §7.5 判 NOT CONVERGING

### D06（06-proc-init-boot-proc.md，2976 行，未开始）

全新文档。同 D05 流程从零开始。

## 每轮必做

```bash
# 面板入库
python3 tools/panel-index.py runs/01-stage-kernel/<doc>/panel/round<N>/
# 轨迹门
python3 tools/eval-trace-check.py runs/01-stage-kernel/<doc> --round R<N>
```

## 制品入库要求（V3.5）

每轮关闭前，以下文件必须在 `runs/<stage>/<doc>/` 下：
- `panel/round<N>/*.md` — 面板原始输出（逐角色一份）
- `panel/round<N>/INDEX.md` — 哈希索引（panel-index.py 生成）
- `certify/round<N>-*.md` — certify 独立冷读回执原文
- `defect-ledger.md` — 该轮台账行（在 `## R<N>` 编号节下）
- `metrics.md` — 该轮指标块（在 `## R<N>` 编号节下）
- `loop.md` — 该轮九节结构（§0–§8 + L6 + L6.5）

## 收敛判据

一篇文档 CONVERGED 需要**同时满足**：
1. `--round R<N>` exit 0
2. 确认轮 `certify 新准入 = 0`
3. OPEN-DEBT 无未销账行
4. FREEZE 后回归审计（§4h）干净
5. 机械门全绿（anchor/citation/doc-code-map/structure/ref/style/density）

一篇文档的循环在**确认轮 certify 新准入 > 0** 或 **FREEZE 后回归审计出 P0/P1** 时不收敛——继续修改轮，直到收敛或达 edition 上限（3 修改轮 + 2 确认轮）。

## 收敛后

写 FREEZE.md → 回归审计 → 进下一篇。

## 全部完成后

写汇总报告到 `rewrite-notes/coordination/review-eval/meta/V3.5-execution-summary.md`，内容包括：
- 六篇的轮数、发现数、P0 数、FP 数、收敛状态
- 制品链实战表现（INDEX 哈希是否一致、certify 回执质量）
- 门误报/漏报统计
- 流程收敛性最终判定
