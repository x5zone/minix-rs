# meta 目录与单元级 review（2026-11-22）

## 一、为什么做

- **token 成本**：review loop 的一轮原本是「整篇 × 面板 × 全量门」。V3.8 的 3 轮硬上限解决了
  「无限开轮」，但没有解决「每轮重读全文」——01 篇 2,673 行、28 轮的重复通读主要耗在这里，
  已经稳定的章节被后续轮次反复读入。
- **过程产物散落**：todo、review 轨迹、设计快照分散在 `coordination/` 与各 stage 的 `.design/`，
  正式文档容易顺手引用；TODO 迁移刚清掉一批死链，需要一个结构性边界。
- 用户裁决：review 按 `###` 节冻结 + 三轨；过程产物一次到位迁入 `rewrite-notes/meta/`；
  `draft/` 与 `archive/` 除外（稳定后要删）。

## 二、目标布局

```
rewrite-notes/
  meta/
    README.md
    coordination/          # 整体下沉；含 todo-ledger/ 与 review-eval/
    design/<stage>/        # 原各 stage/.design（本地快照，gitignore）
    review/state/<stage>/<doc>.yaml
  01-stage-kernel/
    NN-*.md                # 正式稿
    draft/ archive/        # 原样保留，稳定后另行删除
```

选「coordination 整体下沉」而不是把 todo-ledger/review-eval 抽到 meta 根：前者只需一次
前缀/深度修正，避免 51 处 stage 链接与 81 处父级链接二次断裂；todo 与 review 评价仍属
跨会话协作，放在 `meta/coordination/` 下语义成立。

`.design/` 原本被 `.gitignore` 的 `**/.design/` 忽略、基本为本地态；迁移用 `mv`，新增
`rewrite-notes/meta/design/` 忽略规则。4 个曾被强制跟踪的 01 篇快照随目录重命名保留历史。

## 三、单元级 review（V3.9）

- 单元：`##` = T1 章单元；`###` = T2 节单元；无子节的 `##` 两者兼任；`参见/附录/索引/参考文献`
  不建单元。
- 状态：`pending → passed → frozen`。章冻结要求 T1 通过且子节全冻结；篇冻结要求全章冻结 + T3 通过。
- 预算：每单元每轨 2 个修改轮；第 3 轮需用户批准或整节重写；篇级 3 edition 上限不变。
- 冻结：正文 hash 变化只解冻 T2；标题/顺序变化解冻 T1+T2；父章随子节解冻。
- 每轮读什么：首 edition 全篇；后续轮只读未冻结单元；确认轮读未冻结单元 + ≥20% 冻结样本
  + 骨架；脏单元 >30% 回退全篇。读者 C 在 T2 仍读全篇，防跨节矛盾被切碎。
- 出口：G1–G13 不变，新增 G14（`review-units.py check` 通过、全部 frozen、T3 通过）。

状态文件：`rewrite-notes/meta/review/state/<stage>/<doc-stem>.yaml`。
01 篇已初始化 41 个单元（6 章 + 35 节），全部 `pending`，不改变历史 FREEZE 结论。

## 四、改动与提交

| 提交 | 内容 |
|---|---|
| `83124a7f4` | coordination → `meta/coordination/`；`.design` → `meta/design/<stage>/`；链接/路径真源/工具/CI/文档同步 |
| `2c39cb905` | `review-units.md`、`tools/review-units.py`、G14、三轨接入、01 篇状态初始化 |

路径真源：`tools/notes-layout.conf` 增加 `NOTES_META` / `NOTES_COORD` / `NOTES_TODO_LEDGER` /
`NOTES_REVIEW_EVAL` / `NOTES_REVIEW_STATE` / `NOTES_DESIGN_ROOT`；`tools/notes_layout.py` 供
Python 工具使用，并由 `notes_layout.py` 自测与 `check-review-rules.sh` 双向校验。

## 五、验证读数

- `python3 tools/todo-ledger.py check` → OK（01: 39 条 open 19 / blocked 4；02: 28 条 open 17 / blocked 5；coordination: 46 条 open 37）。
- `python3 tools/notes-link-check.py rewrite-notes/` → 151 处断链；正式稿 0 处，全部为 review-eval
  run 制品与历史工作文档的存量；迁移后正式稿与外部 coordination 链接无新增断链。
- `python3 tools/doc-structure-check.py rewrite-notes/meta/coordination/review-eval` → 274 文件、117 处存量表结构问题；路径有效，非空扫描。
- `bash tools/design-coverage-check.sh 01-stage-kernel` → ALL DOCS COMPLETE。
- `python3 tools/review-units.py --self-test` → PASS；`check` 01 篇 → OK（41 单元、0 冻结）。
- `bash tools/check-review-rules.sh`、`lint-review-rules.sh`、`diff-trae-skills.sh --only-diff` → 全绿。
- `python3 tools/eval-trace-check.py ... --round R37` / `R5` → 仍为迁移前的 lifecycle FAIL
  （CONVERGED 后再开轮），run 制品未被本次改动触碰。

## 六、边界与残留

- `draft/` 与 `archive/` 不迁不删；等稿件稳定后另行清理。
- `meta/design/` 是本地态、gitignore；新机器上由 Step 0.3 重新生成，不在 CI 覆盖内。
- 历史 prompt 报告、review-eval run 内部的旧路径叙述不改写；活配置与正式文档已更新。
- 单元模型不改变四态与确认轮：修改轮仍无收敛声明权；单元全 frozen 也不等于篇级 `CONVERGED`。
- 旧 FREEZE 篇目 grandfather，不追溯补单元状态；新 edition 从 01 篇的下一轮开始使用。
