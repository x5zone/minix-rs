# review-eval —— review 工作流演进循环的评测与轨迹

> 本目录是 `prompt/skill/review-loop.md` 的配套数据目录，不是规则源。
> 规则源在 `prompt/review-rules/`；本目录只存语料、盲区、冻结决策、每轮轨迹和 meta-review 报告。

## 这个目录解决什么问题

W-16 记录过一次十轮评审仍未发现同一文档三处跨节矛盾的事故；W-6 记录过人读一遍发现八处“读不懂”、工具零拦截。两次事故的共同点不是“规则写得不严”，而是三件基础设施缺失：

1. 没有固定评测语料，换一篇容易的文档就会显得“工作流进步了”；
2. 没有独立读者，审查者和作者共用同一个上下文；
3. 没有把“流程本身是否进步”当成可验证对象。

本目录把这三件事固定下来：

- `CORPUS.md`：每轮跑同一批案例；
- `BLIND-SPOTS.md`：历史上漏过的问题逐条入账，工作流改版后必须重新出现；
- `runs/` 与 `meta/`：每篇一轮轨迹加一份独立评估，回答“这一轮到底改进了什么”。

## 当前状态

- 基线提交：`e364d5f92`（2026-11-07，review 演进循环开始前的 WIP 快照）
- 当前工作流版本：V1（脚手架，见 `WORKFLOW-VERSIONS.md`）
- 试点批次：`rewrite-notes/01-stage-kernel/` 的 01–10 篇；之后是 `02-stage-vm/` 与 `03-stage-rs/` 的 01–10 篇
- 执行入口：`prompt/skill/review-loop.md`

## 目录

| 路径 | 内容 | 是否入库 |
|---|---|---|
| `CORPUS.md` | 固定评测语料：真实篇目、回归切片、对抗样本 | 是 |
| `BLIND-SPOTS.md` | 历史盲区账本 | 是 |
| `DECISIONS.md` | 已裁决或已冻结的决策，防止重复提 | 是 |
| `WORKFLOW-VERSIONS.md` | 版本链、减法账本、同步门读数 | 是 |
| `doc-set/` | 02/03 阶段的文档集对账简报 | 是 |
| `runs/<stage>/<doc>/` | 每篇的 `loop.md`、`metrics.md`、`defect-ledger.md`、`follow-ups.md` | 是（最小集） |
| `meta/` | 每篇与每个版本边界的 meta-review 报告 | 是 |
| `traps/` | 八个对抗样本 | 是 |
| `expected/` | 对抗样本的期望判定，与 `traps/` 分离 | 是 |
| `tmp/review-eval/` | 面板原始输出、命令日志、diff、token 读数 | 否 |

`tmp/review-eval/` 是相对于仓库根目录的路径，整域不入库。入库的是可恢复的最小集；原始取证留在 `tmp/`，需要长期保存时再摘录进 `runs/`。

## 如何跑一篇

1. 读 `WORKFLOW-VERSIONS.md` 确认当前版本。
2. 读 `CORPUS.md` 中该篇条目与 `BLIND-SPOTS.md` 的相关条目。
3. 读 `DECISIONS.md` 中与本篇相关的冻结决策。
4. 按 `prompt/skill/review-loop.md` 的 L0–L7 执行。
5. 把轨迹写进 `runs/<stage>/<doc>/`，把 meta-review 写进 `meta/`。

## 恢复现场

新会话恢复时按以下顺序读，不要凭记忆：

1. 本文件；
2. `WORKFLOW-VERSIONS.md` 的最后一条；
3. `runs/<stage>/<doc>/loop.md` 的 §0、§7、§8；
4. `meta/` 中最近一份报告；
5. `BLIND-SPOTS.md` 的 OPEN 项。

## 纪律

- 每一条面板发现都必须有处置行；没有处置行的轨迹由 `tools/eval-trace-check.py` 判 FAIL。
- 不把“评审又提了建议”当成重开工作流的理由；重开条件见 `prompt/review-rules/review-evals.md` 的收敛标准。
- 不把 `prompt/resources/` 的外部文本直接抄进本目录或规则源；只借原则，按各自 license 取舍。
- 同一篇的轨迹、台账和指标必须能互相复算；跨层数值不一致时，必须在同一处写明口径差。
