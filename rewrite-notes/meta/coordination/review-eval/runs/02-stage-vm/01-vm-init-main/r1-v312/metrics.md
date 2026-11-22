# 02-stage 01 metrics

## R0 账户初始化

- 发现总数: 0
- 唯一根因数: 0
- 未闭环: 0
- 轮结果: R0

## R1 full（V3.12）

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
- 代码: `cargo test -p minix-vm --lib` 549 passed / 0 failed
- 文档门: publication 0、rubric 0、style 0、ref 通过、density 通过
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- 减法: 提案 0 / 采纳 0
