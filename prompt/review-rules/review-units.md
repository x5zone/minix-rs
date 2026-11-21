# 单元级 review：按节冻结与三轨分工（V3.9）

> **解决的问题**：整篇 review 的每一轮都重读全文，稳定章节随轮次反复消耗面板；D01 的 28 轮里，
> 后段轮次的大部分输入是已经不再变化的章节。本文件把 review 的**覆盖单位**从「整篇」降到
> 「`###` 节」，并允许已稳定的节退出循环。
>
> **不改变的东西**：G1–G13、四态模型、确认轮、代码冻结、零欠账、3 edition 上限全部保留；
> 本文件只改变「每一轮读什么」，不改变「什么算收敛」。

## 一、单元与轨道

| 单元 | 取哪一级标题 | 轨道 | 检查什么 |
|---|---|---|---|
| 章单元 | `##` | **T1 结构/语义** | 论证链、C 源锚点、设计语义、跨节承诺、章内小节边界 |
| 节单元 | `###` | **T2 行文/教学** | TR-1..TR-6（在父章级判定）、文风、锚点句意、术语一致 |
| 篇 | 整篇 | **T3 出版/代码同步** | DP-1..DP-6、CP-1..CP-6、符号覆盖、TODO 账本、测试/真机门 |

- 没有 `###` 的 `##` 同时是 T1 与 T2 单元。
- `参见` / `附录` / `索引` / `参考文献` 章不建单元；正文中的空节（只有标题与分隔线）不建单元。
- 单元 ID 优先取标题编号（`1.2`），无编号时取 `u-<顺序>-<slug>`；稳定 ID 写入状态文件，禁止自动重编号。

## 二、状态机

节单元：

```
pending → (T2 通过) → passed → (父章 T1 通过) → frozen
   ↑                                      │
   └────────── 正文 hash 变化 ────────────┘
```

章单元：

```
pending → (T1 通过) → passed → (全部子节 frozen) → frozen
```

篇级：全部章 `frozen` 且 `T3` 通过，才允许进入确认轮；确认轮通过后才是 `CONVERGED`。
`CONVERGED` / `ROUND-CLOSED` 不写进单元状态——单元状态只描述稿件，不描述轮次结论。

## 三、预算

- 每个单元每轨最多 **2 个修改轮**（`t1_rounds` / `t2_rounds`）。第 3 个修改轮只有两个合法出口：
  用户明确批准，或该节**整节重写**（`rewritten: true`，计数重置，但一个 edition 内只允许一次）。
- **干净通过不耗预算**：只有产生并实施了修改的轮次才 `+1`。
- 篇级仍受 3 edition 上限约束；单元预算不替代也不放宽它。
- 同一根因跨单元复发时，仍按 `review-evals.md` §4.0b 的「第二次出现必须结构性重写」处理，
  不因为换了单元就重新计第一次。

## 四、冻结与解冻

- 冻结条件：该单元 `open = 0`、`certify 新准入 = 0`、机械门通过、正文 hash 与冻结时一致。
- 正文变化：`tools/review-units.py sync` 比较 hash。节正文变化只解冻该节（T2）；
  标题、顺序、层级变化解冻该章（T1）与受影响子节；`deps` 变化把下游单元标为 `reopened`。
- 冻结单元不再进入 L1 面板；只进入确认轮的抽样。冻结不是「永远不读」——确认轮仍会抽 20%。
- 任何对冻结节的修改都必须先 `sync --unfreeze` 留痕，禁止手改状态文件绕过。

## 五、每轮读什么

| 轮种 | 覆盖范围 |
|---|---|
| 首个 edition 的 full survey | 整篇（沿用现有面板档位），建立基线 |
| 后续 full / structural | 全部未冻结单元 + 依赖闭包；冻结单元不重读 |
| confirmation | 全部未冻结单元 + **≥20% 冻结单元**（seed = 最终 commit）+ 骨架（标题树与跨节承诺）；不做全文逐句通读 |
| 回退条件 | 未冻结单元 > 30%，或 T1 发现跨章结构问题，或用户要求 → 回到整篇 survey |

确认轮仍由未参与本轮修复的冷启动 agent 执行；抽样只减少阅读量，不降低独立性。

## 六、与既有规则的关系

- **G1–G13 不变**；新增 **G14**：`tools/review-units.py check --doc <当前篇>` 退出 0，
  且全部单元 `frozen`、T3 通过。
- **四态模型不变**：修改轮仍无收敛声明权；`CONVERGED` 只由确认轮宣告。
- **代码冻结不变**：进入 L1 后任何 `os/` 改动 = 中止本轮。
- **零欠账不变**：每个单元的 `open` 与 TODO 账本一致；blocked 待裁项存在时不得冻结该单元所在的章。
- **面板阈值**：`review-panel.md` §七 的档位按**本次实际派发的单元**行数判断；首 edition 的整篇
  survey 仍按整篇行数判断。读者 C 在 T2 中仍读整篇（防止跨节矛盾被切碎），A/B 只读被派单元。

## 七、工具

```bash
# 首次：按当前标题树建状态
python3 tools/review-units.py init rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
# 整篇 survey 完成后记录基线（之后才允许确认轮抽样）
python3 tools/review-units.py baseline rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md

# 每轮开始：校验状态与正文一致，列出本轮该审的单元
python3 tools/review-units.py check <doc>
python3 tools/review-units.py next <doc>

# 修复/通过：只动被审单元
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --fix
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --pass --freeze

# 第 3 轮的两种合法出口
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --approve   # 用户批准
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --rewrite   # 整节重写

# 跨节依赖：上游改动时下游标 reopened
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --deps 2.1,2.2

# 删除/合并小节后：先解冻再 prune（不静默丢单元）
python3 tools/review-units.py sync <doc> --unfreeze --prune

# T3 通过（全部单元冻结后）
python3 tools/review-units.py gate <doc> --pass --evidence runs/<stage>/<doc>/certify/round<N>-gates.md

# 确认轮抽样（首 edition 拒绝；ratio ≥0.2；同 seed 同集合）
python3 tools/review-units.py sample <doc> --ratio 0.2 --seed $(git rev-parse HEAD)
```

状态文件：`rewrite-notes/meta/review/state/<stage>/<doc-stem>.yaml`。

## 八、失败对照

- **只按节读、不读骨架**：两节各自自洽，合起来却重复或矛盾。所以每轮必须读标题树与跨节承诺；
  T2 的读者 C 读整篇。
- **冻结即遗忘**：冻结单元被后续修改悄然破坏。所以有 hash 校验与 20% 确认抽样；发现即解冻，
  不许用「它已经冻结」驳回证据。
- **单元预算当 edition 预算**：一个单元 2 轮用完就以为整篇可以收口。篇级 3 edition 与 G1–G14
  仍要全过。
- **用抽样代替确认轮**：抽样是确认轮的读法，不是取消确认轮。修改轮仍不得宣告 `CONVERGED`。
