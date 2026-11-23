# D01 六视角守门案件卷：编辑视角的体裁分歧

- 日期：2026-11-23
- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（3,142 行，sha `3e8e03a0…`）
- 规则来源：作者 2026-11-23 读者体验裁决（六视角守门，6/6 才可 CONVERGED）

## 六视角结果

| 视角 | 报告 | 结论 |
|---|---|---|
| beginner-me | `cold-read/d01-beginner-v2.md`（重锚 3e8e03） | `HARD-REJECTIONS: 0 / READY` |
| G18 READER-C | `cold-read/d01-g18-v2.md` | `HARD-REJECTIONS: 0 / READY` |
| 编辑 | `cold-read/editor-perspective-v3.md` | `NOT-READABLE` |
| 导航者 | `cold-read/navigator-perspective-v2.md` | `READABLE` |
| 效率读者 | `cold-read/reference-perspective-v2.md` | `READABLE` |
| 教学设计师 | `cold-read/instructional-design-perspective-v2.md` | `READABLE` |

## 编辑视角的反对意见

编辑认为：文档有一条真实主线，但主线内部仍夹着字段台账、版本账、九动作约束、五步判定、容量表与里程碑矩阵；按顶部路线走到 §3.2 会停在 22 字段退场账，到 §4 要连续重建四次编号坐标系。三项最小可翻盘：

1. 只留一张路线图，删除章表“第一次读”列与残留“可跳过/勿混”指令。
2. 把 §3.2 的 155 行账本移出主线（并入已标参考的 §3.2.2）。
3. 编号元评论收口到 §4.1 一张表，删除其余“这套不是那套”。

其中第 1、3 项已在 `3e8e03a0` 落地；第 2 项是剩余分歧。

## me-agent 裁决

- 不把编辑的 `NOT-READABLE` 当作事实错误或代码缺陷；它是**体裁选择**：当前文档是“重写说明 + 参考手册”的合体，编辑按“技术社区文章”标准判。
- 不在本轮内把 3,142 行文档拆成主文 + 附录：这会改变正式文档的单元、锚点与 review 账户，属于结构性改版，不是 L0.9 微调。
- 采用**主线伴生篇**方案（选项 A）：保留当前完整文档为参考/证据面，新增 `01-boot-shim-bootstrap-mainline.md`，只含 600–800 行主线（三份数据 → I1–I4 → 两张表 → 五条腿 → 接收落地区 → 诊断），所有细节以指针回完整篇。主线篇完成后重新跑六视角；六视角全过才把 D01 标为读者验收通过。
- 在主线篇完成前，D01 的状态写作：`CONVERGED（V3.12 评审门）；六视角守门 5/6，编辑视角待主线篇`。

## 回退

- 若作者否决主线伴生篇，则接受“参考手册”体裁，编辑视角的判据改为“按顶部路线能否查到并读懂”，不再要求“像文章一样读完”；需要作者明确改判据。
