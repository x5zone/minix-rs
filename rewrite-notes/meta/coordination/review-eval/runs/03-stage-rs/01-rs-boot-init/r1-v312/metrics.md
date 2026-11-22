# 03-stage 01 metrics

## R0 账户初始化

- 发现总数: 0
- 唯一根因数: 0
- 未闭环: 0
- 轮结果: R0

## R1 full（V3.12）

- 发现总数: 84
- 唯一根因数: 24
- 本轮新准入: 24
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 轮结果: ROUND-CLOSED
- 治理: ARCH 提案 0 / 用户否决 0
- ARCH 提案: 0
- 用户否决: 0
- 原始发现: 84（P-FACT 13 + READER-A 17 + READER-B 26 + READER-C 26 + DESIGN-D2 2）
- 归并后唯一根因: 24（defect-ledger R1-01..R1-24）
- P0: 4（Step 1 内核提交写反、生产接线状态过期、测试基线自相矛盾、RS 的 SEF 入口路径写错）
- 处置: ① 24 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- edition: V3.12
- 轮型: full
- 结果: ROUND-CLOSED（不宣告 CONVERGED）
- full 轮: 1（R1）
- 单元: 39/39 frozen
- trajectory: R1 PASS
- 事件: `event verify --fresh` 39/39 OK
- 测试: `cargo test -p minix-rs --lib` 359 passed / 0 failed
- 文档门: publication 0、rubric 0、style 0、ref 通过、density 通过
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 5 / token 未统计 / 耗时 未统计
