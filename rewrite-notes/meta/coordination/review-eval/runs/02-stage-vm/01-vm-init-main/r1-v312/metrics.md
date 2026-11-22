# 02-stage 01 metrics

## R0 账户初始化

- 发现总数: 0
- 唯一根因数: 0
- 未闭环: 0
- 轮结果: R0

## R1 full（V3.12）

- 发现总数: 83
- 唯一根因数: 34
- 本轮新准入: 34
- certify 新准入: 0
- 重复合并: 0
- 自引入: 0
- 轮结果: ROUND-CLOSED
- 治理: ARCH 提案 0 / 用户否决 0
- ARCH 提案: 0
- 用户否决: 0
- 原始发现: 83（READER-A 22 + READER-B 20 + READER-C 18 + DESIGN-D2 19 + P-FACT 4）
- 归并后唯一根因: 34（defect-ledger R1-01..R1-34）
- P0: 3（extra_pages 模块记账、is_first_time 恒 fresh、栈帧边界写反）
- P1: 31 条原始（含重复根因）；P2/P3 其余
- 处置: ① 34 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- edition: V3.12
- 轮型: full
- 结果: ROUND-CLOSED（不宣告 CONVERGED）
- full 轮: 1（R1）
- 单元: 43/43 frozen；每单元独立 UNIT 证据
- trajectory: R1 PASS
- 事件: `event verify --fresh` events=45 completed=45 fresh_only=1 → OK（23 个重开单元由 round1c 新事件覆盖，旧事件作历史保留）
- 代码: `cargo test -p minix-vm --lib` 549 passed / 0 failed
- 修复复核: T1B 7 + T2B 7 + P-REFUTE 3，全部 RESOLVED；round1c T1/T2 两报告 PASS
- 文档门: publication 0、rubric 0、style 0、ref 通过、density 通过
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- 减法: 提案 0 / 采纳 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 5 / token 未统计 / 耗时 未统计
