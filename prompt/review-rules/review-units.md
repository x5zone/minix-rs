# 单元级 review：每篇独立账户与单元子键（V3.11）

> **解决的问题**：整篇 review 的每一轮都重读全文，稳定章节随轮次反复消耗面板；更严重的是，
> 跨篇复用轮次号或裁决会让 D02 花掉 D01 的账。V3.11 把 review 账本改成
> **每篇一个独立账户，每个 `##`/`###` 单元一个子键**，任何轮次、裁决、证据都必须在
> 该篇账户内闭合。
>
> **不改变的东西**：G1–G14、四态模型、确认轮、代码冻结、零欠账、3 edition 上限全部保留；
> 本文件另加 G15/G16/G17。V3.11 只改变“账怎么记、每篇至少审几轮”，不改变“什么算缺陷”。

## 一、账户模型

- 每篇文档一个状态文件：`rewrite-notes/meta/review/state/<stage>/<doc-stem>.yaml`。
- 账户键 = 文档路径；子键 = 单元 ID（`##` 章、`###` 节）。
- 账户内字段：
  - `edition`：本篇当前规则版本（V3.11 起独立记录，不得从别篇继承）；
  - `round`：本篇本地轮次，从 0 开始，只在本篇递增；
  - `full_rounds`：本篇已完成的 full 轮列表（每项带 `n`、`evidence`、`evidence_sha`、`commit`）；
  - `trajectory_reviews`：每轮结束后的轨迹复核（`round`、`evidence`、`verdict`、`commit`）；
  - `units[]`：每个单元的 `review_rounds` 与 `post_freeze_rounds`。
- **跨篇引用禁止**：D01 的 `R39` 只能写在 D01 账户；D02 的轮次必须从自己的 `round` 取号。
  引用裁决时必须带裁决的适用范围（doc / stage / repo），D-114 这类 D01 专用裁决不得给 D02 背书。
- **状态文件也绑定 doc**：`review-units.py` 校验状态里的 `doc` 字段与当前文档一致；复制状态文件
  到另一篇即拒绝。证据文件必须恰好一条 `DOC <当前篇>`，写两条 `DOC` 也拒绝。

## 二、单元与轨道

| 单元 | 取哪一级标题 | 轨道 | 检查什么 |
|---|---|---|---|
| 章单元 | `##` | **T1 结构/语义** | 论证链、C 源锚点、设计语义、跨节承诺、章内小节边界 |
| 节单元 | `###` | **T2 行文/教学** | TR-1..TR-6（在父章级判定）、文风、锚点句意、术语一致 |
| 篇 | 整篇 | **T3 出版/代码同步** | DP-1..DP-6、CP-1..CP-6、符号覆盖、TODO 账本、测试/真机门 |

- 没有 `###` 的 `##` 同时是 T1 与 T2 单元。
- `参见` / `附录` / `索引` / `参考文献` 章不建单元；正文中的空节不建单元。
- 单元 ID 优先取标题编号（`1.2`），无编号时取 `u-<顺序>-<slug>`；稳定 ID 写入状态文件，禁止自动重编号。

## 三、硬门槛（V3.11）

### 3.1 每篇至少 2 个 full 轮

- 第 1 个 full 轮用 `baseline --evidence`；第 2 个及以后用 `full-round --evidence`。
- full 轮必须覆盖**全部单元**，包括已冻结单元；证据文件必须含 `FULL-ROUND` 标记、
  `DOC <当前篇>` 账户绑定，并为每个单元写 `UNIT <id>`。缺一个单元或缺 DOC 即 FAIL。
- full 轮不得用确认轮抽样、冻结单元豁免或“上轮已读”替代；每个 full 轮都是独立产物，
  同一份 evidence 不得复用于两个 full 轮。
- `gate --pass` 要求 `len(full_rounds) >= 2`；不足时最多 `ROUND-CLOSED`。

### 3.2 每个单元至少 2 次 review

- 第一次：冻结前的 T1/T2 review。`mark --pass --evidence <unit-file>`，证据文件必须含
  `DOC <当前篇>` 并**只点名**该单元（`UNIT <id>`），且含可重放 token。
- 第二次：冻结后的确认 review。确认轮内 `mark --confirm-review --evidence <unit-file>`，
  证据同样含 `DOC` 且只点名该单元；不改单元正文，只记录复核。
- `mark --freeze` 要求当前轮已有该单元的 `review_rounds` 记录；不允许“先冻结、后补读”。
- `end-confirmation` 要求每个单元 `len(review_rounds) >= 2` 且 `post_freeze_rounds` 非空。
- **禁止批量冻结**：一份含多个 `UNIT` 的通用证据只能用于 full 轮，不能用于任何单元的
  `--pass/--freeze/--confirm-review`。`review-units.py` 对多单元证据直接 FAIL。

### 3.3 每轮结束后必须 trajectory review

- 每个 full 轮和确认轮结束后，必须写独立的轨迹复核文件，含 `TRAJECTORY-REVIEW`、
  被复核轮次 `R<N>`、`DOC <当前篇>`、证据与结论；用
  `trajectory-review --round N --evidence <file> --verdict PASS|FAIL` 入账。
  一份 evidence 只能点名一个轮次；同一 hash 不得复用于另一轮。
- 开下一个 full 轮前，上一 full 轮必须有 `PASS`；开确认轮前，最后一个 full 轮必须有 `PASS`；
  `gate --pass` 前，所有 full 轮与确认轮都必须有 `PASS`。
- `FAIL` 的轨迹复核会阻断下一轮/收敛；修复后重新复核，不允许覆盖成 `PASS` 而不留原因。

### 3.4 轮次与版本的独立性

- `new-edition` 只在本篇账户内归档旧计数；不得用另一篇的 edition、round 或 full 轮数。
- 同一篇的 edition 上限仍是 3（`review-evals.md` §7.5）；跨篇不累计，也不互相抵扣。
- 任何“本轮已经审过”的声明，必须能在本篇账户的 `review_rounds`/`full_rounds` 找到对应记录；
  找不到 = 没审过，不能凑数。

## 四、状态机与冻结

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

篇级：全部章 `frozen`、至少 2 个 full 轮、每个单元至少 2 次 review、T3 通过，才允许进入确认轮；
确认轮通过后才是 `CONVERGED`。`CONVERGED` / `ROUND-CLOSED` 不写进单元状态。

- 冻结条件：`audit` 显示该单元 `open = 0`；`mark --pass/--freeze` 带**只点名该单元**的 evidence；
  正文 hash 与冻结时一致；锚点状态不是 `missing`/`impl`。
- **锚点凭据（V3.10 保留）**：冻结时抽取直锚（`path:fn NAME` / `struct` / `enum` / `trait` /
  `const` / `static` / `type`），对符号体算 sha256 写入 `anchor_hashes`；`anchor_status` 记
  `direct` / `fallback` / `none`。缺失符号或 `impl Type::method` 锚点拒绝冻结；C 锚点算 `fallback`。
- 正文变化：`sync` 比较 hash；节正文变化只解冻该节，标题/顺序/层级变化解冻该章与受影响子节；
  `deps` 变化把下游标 `reopened`。
- **锚点正文变化**：`sync` 重算冻结单元的 `anchor_hashes`；任一符号体变化、缺失或变成 `impl`
  → 解冻并置 `reopened: true` + `stale: true`。旧状态没有 `anchor_hashes` 的冻结单元强制重开。
- **批量失效**：`invalidate --from <commit>` 对比 `git diff --name-only`，冻结单元的锚点文件变化
  → 解冻并置 `reopened`/`stale`。
- 任何对冻结节的修改都必须先 `sync --unfreeze` 或 `--confirm-fix` 留痕，禁止手改状态文件绕过。
  **正文/锚点变化导致解冻时，该单元的 `review_rounds` 与 `post_freeze_rounds` 必须清空**；
  旧确认不能给新正文背书。确认轮内的 `--confirm-fix` 只清当前确认轮的记录，随后必须重新
  `--pass --freeze` 并再做一次 `--confirm-review`。

## 五、预算

- 每个单元每轨最多 **2 个修改轮**（`t1_rounds` / `t2_rounds`），按 edition 计；干净通过不耗预算。
- 第 3 个修改轮只有两个合法出口：用户明确批准，或该节整节重写（`rewritten: true`，一个 edition
  只允许一次）。
- 篇级仍受 3 个修改轮与 3 个 edition 上限约束；单元预算不替代也不放宽它。
- 同一根因跨单元复发时，仍按 `review-evals.md` §4.0b 的“第二次出现必须结构性重写”处理。

## 六、每轮读什么

| 轮种 | 覆盖范围 |
|---|---|
| full（第 1 或第 2 个） | 整篇全部单元，包括已冻结单元；不许抽样 |
| confirmation | 未冻结单元 + **每个冻结单元一次冻结后确认 review** + 骨架；不做全文逐句通读，但每个单元必须留下 `UNIT <id>` 证据 |
| 回退条件 | 未冻结单元 > 30%，或 T1 发现跨章结构问题，或用户要求 → 回到 full |

确认轮仍由未参与本轮修复的冷启动 agent 执行；冻结后确认 review 是硬要求，不是抽样替代品。

## 七、与既有规则的关系

- **G1–G14 不变**；新增：
  - **G15**：`review-units.py check --doc <当前篇>` 与 `audit --doc <当前篇>` 退出 0，全部单元
    `frozen`、`reopened=0`、`stale=0`、T3 通过，且每个单元 `review_rounds >= 2`、
    `post_freeze_rounds` 非空。
  - **G16**：本篇 `full_rounds >= 2`；每个 full 轮的 evidence 覆盖全部单元；同一 evidence
    不得跨轮复用。
  - **G17**：本篇每个 full 轮与确认轮都有 `PASS` 的 `trajectory_reviews`；缺一个不得 CONVERGED。
- **四态模型不变**：修改轮无收敛声明权；`CONVERGED` 只由确认轮宣告。
- **代码冻结不变**：面板 survey 与 certify 阶段不得写 `os/`；本轮台账内的修复按 L3 实施后
  重设基线，设计批级改动回 Manuscript（`review-publication.md` §四.6）。
- **零欠账不变**：每个单元的 `open` 与 TODO 账本一致；blocked 待裁项存在时不得冻结该单元所在章。
- **面板配置**：整篇 full 用 `review-panel.md` §七表 A；确认轮用表 B，但冻结单元的
  `--confirm-review` 必须逐单元留证，不能用抽样结果替代。

## 八、工具

```bash
# 首次：按当前标题树建账户
python3 tools/review-units.py init rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md

# 第 1 个 full 轮：整篇 survey，evidence 必须含 FULL-ROUND 与全部 UNIT
python3 tools/review-units.py baseline <doc> --evidence runs/<stage>/<doc>/metrics-full-r1.md

# 第 2 个 full 轮：先复核 R1 轨迹，再记录独立 full 产物
python3 tools/review-units.py trajectory-review <doc> --round 1 --evidence runs/<stage>/<doc>/trajectory-r1.md --verdict PASS
python3 tools/review-units.py full-round <doc> --evidence runs/<stage>/<doc>/metrics-full-r2.md

# 单元 review：evidence 只能点名该单元
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --pass --evidence runs/<stage>/<doc>/unit-2.3.md
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --pass --freeze --evidence runs/<stage>/<doc>/unit-2.3.md

# 确认轮：每个冻结单元都要一次 confirm-review
python3 tools/review-units.py begin-confirmation <doc>
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --confirm-review --evidence runs/<stage>/<doc>/unit-2.3-confirm.md
python3 tools/review-units.py end-confirmation <doc>

# 确认轮修复：解冻、修复、重新冻结、重新 confirm-review
python3 tools/review-units.py mark <doc> --unit 2.3 --track t2 --confirm-fix

# T3 与收敛
python3 tools/review-units.py gate <doc> --pass --evidence runs/<stage>/<doc>/gates-final.md
python3 tools/review-units.py trajectory-review <doc> --round 3 --evidence runs/<stage>/<doc>/trajectory-r3.md --verdict PASS
```

状态文件：`rewrite-notes/meta/review/state/<stage>/<doc-stem>.yaml`。
`stats` 会输出 `round`、`full_rounds`、`trajectory_reviews`、`units_review_lt2`、`units_post_freeze_0`。

## 九、失败对照

- **跨篇借账**：D02 用了 D01 的 R39 或 D-114。账户模型下这是越界；工具按 `doc` 定位状态文件，
  引用裁决必须校验适用范围。
- **只按节读、不读骨架**：两节各自自洽，合起来却重复或矛盾。full 轮必须读标题树与跨节承诺；
  确认轮的读者 C 读整篇。
- **冻结即遗忘**：冻结单元被后续修改悄然破坏。有 hash 校验与逐单元 confirm-review；发现即解冻。
- **一份证据冻结全篇**：通用 evidence 没有 `UNIT <id>` 或含多个 `UNIT`，工具直接 FAIL。
- **用抽样代替确认轮**：抽样是确认轮的读法，不是取消确认轮；修改轮仍不得宣告 `CONVERGED`。
- **一轮 full 就收敛**：V3.11 至少 2 个 full 轮；不足时最多 `ROUND-CLOSED`。
