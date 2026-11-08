# D01 开放欠账（open-debt）

> 本表只收**跨 crate / 需要独立项目**的 ② 项（负责人 + 截止轮次），按 `prompt/review-rules/review-evals.md` §4.1 与 G9 的口径管理。
> `adjacent` 项不进本表：它们在 `follow-ups.md` 登记目标轮次，按 §4.2 的“adjacent 阻塞 = 0”规则**不阻塞本篇收敛**。
> 本表为空是“没有跨 crate 欠账”的充分信号，但不是收敛的必要条件；收敛只看 G8（连续两轮无新 in-scope 发现）与 G9（⑥ 项清零）。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| F-R17-02 | P2 | UEFI 测试辅助三件套的 feature 门（跨 16 个测试内核）；截止 R19 |
| F-R17-04 | P2 | `KernelInfo` 增 `ram_authority` 字段（跨 minix-boot/内核/文档 ABI）；截止 R19 前裁决 |
| — | — | F-R18-01 已实施；R14/READER-A-16 为 adjacent，见 `follow-ups.md` |
