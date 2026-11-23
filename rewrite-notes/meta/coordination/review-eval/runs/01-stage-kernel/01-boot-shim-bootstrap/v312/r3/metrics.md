# D01 R3 metrics（V3.12，结构轮）

## R3（V3.12，full）

- 发现总数: 22
- 唯一根因数: 20
- 本轮新准入: 28
- certify 新准入: 0
- 重复合并: 3
- 自引入: 3
- 处置: ① 28 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 3 / SUGG 37
- 未闭环: 0
- 轮型: full
- 轮结果: ROUND-CLOSED
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.12
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 减法: 提案 1 / 采纳 1
- 成本: agent 8 / token 未统计 / 耗时 未统计

说明：发现 ID 共 30 个——22 个主 FINDING + 2 条 READER-C 附带发现 + 3 条 P-REFUTE 观察 + 3 条独立复验新发现（NEW-01..03）；A-01/B-01、A-04/B-02 两组合并成同一行，故台账 28 行、`处置 ① 28`。`本轮新准入 28` 按台账行数计，不是“新根因数”（后者是 20）。FP 3 是 P-REFUTE 的 CLAIM 01/02/06（反证失败，不改文档）。`SUGG 37` 为人工计数（READER-A 10、READER-B 16、READER-C 9、P-FACT 2）；其中高价值项已随本轮落地，其余为纯风格取舍，不进入 FINDING 口径。`减法 1/1` 指删除 `Serializer` 的三个死计数器。
