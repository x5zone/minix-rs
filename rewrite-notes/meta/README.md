# meta/ — 过程元数据区

本目录存放产生正式稿所需的全部过程产物，与 `rewrite-notes/<stage>/NN-*.md` 的读者正文分开。

## 边界

- **正式文档不得引用本目录**。`tools/doc-publication-check.py` 的 DP-3 把 `meta/` 与 `.design/`、`runs/` 同等列为中间产物引用；命中即 FAIL。
- 本目录的内容可以长期保留；`<stage>/draft/` 与 `<stage>/archive/` 不在此列——它们是过渡稿与旧稿，待稿件稳定后删除。
- 本文的“meta 根”与 review run 内部的 `meta/` 子目录（轮级 meta-review 报告）不是同一概念。

## 子目录

| 路径 | 内容 |
|---|---|
| `coordination/` | 跨会话协作记录：决策、案卷、worklog、历史台账；其中 `todo-ledger/` 是 TODO 单一账本，`review-eval/` 是 review 轨迹与评测数据 |
| `coordination/todo-ledger/` | 每 stage 一本 `<stage>.yaml`，由 `tools/todo-ledger.py` 校验 |
| `coordination/review-eval/` | review-loop 的 runs、面板制品、指标与 `DECISIONS.md` |
| `design/<stage>/` | 各篇的设计快照（outline / design / design-final）。除 01/02 的 4 个冻结快照入库外均为本地态；`.gitignore` 排除整目录，缺失时由 Step 0.3 重新生成 |
| `review/state/<stage>/<doc>.yaml` | 单元级 review 状态：`###` 节的冻结状态、轮次预算、T1/T2/T3 轨道（规则见 `prompt/review-rules/review-units.md`） |

## 迁移来源

2026-11-21 之前，过程产物散在 `rewrite-notes/coordination/` 与各 stage 的 `.design/`。逐路径对照见 `rewrite-notes/MIGRATION.md` 的「2026-11-21 meta 迁移」节。
