# 03-stage 01 metrics

## R0 账户初始化

- 发现总数: 0
- 唯一根因数: 0
- 未闭环: 0
- 轮结果: R0

## R1 full（V3.12）

- 发现总数: 87
- 唯一根因数: 48
- 本轮新准入: 48
- certify 新准入: 0
- 重复合并: 39（87 条 FINDING 归并为 48 个根因行）
- 自引入: 0
- 轮结果: ROUND-OPEN
- 治理: ARCH 提案 0 / 用户否决 0
- ARCH 提案: 0
- 用户否决: 0
- 原始发现: 87（P-FACT 13 + READER-A 26 + READER-B 13 + READER-C 32 + DESIGN-D2 3）
- 归并后唯一根因: 48（defect-ledger R1-01..R1-48；R1-25..R1-48 为对账新增）
- P0: 4（FACT-01、FACT-02、READER-C-02、D2-01 均已修并复验）
- 处置（按根因行）: ① 13 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 35 / FP 0 / SUGG 0
- 说明: 87 条 FINDING 归并为 48 个根因行，处置计数以根因行为单位（混合行含已修子项）
- 未闭环: 35（根因行）
- edition: V3.12
- 轮型: full
- 结果: ROUND-OPEN（未收敛，不宣告 CONVERGED）
- full 轮: 1（R1）
- 单元: 29/39 frozen（本次代码/正文同步后 10 个单元解冻，R2 待审）
- trajectory: R1 OPEN
- 面板核对: panel=87 ledger_ids=87 roots=48 missing=0
- 基线: R1 面板针对 1116 行 / md5 `5f44908078b793e7dc68d252f141cfa2`；本次同步修改了 §1.1/§3.5/§3.6/§5 的部分描述，未修 45 条面板发现
- 事件: `event verify --fresh` 39/39 completed，其中 6 条因正文同步暂不新鲜（R2 复核后刷新）
- 测试: `cargo test -p minix-rs --lib` 359 passed / 0 failed；`--features live-update` 360 passed / 0 failed
- 文档门: publication 0、rubric 0、style 0、ref 通过、density 通过
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 5 / token 未统计 / 耗时 未统计
