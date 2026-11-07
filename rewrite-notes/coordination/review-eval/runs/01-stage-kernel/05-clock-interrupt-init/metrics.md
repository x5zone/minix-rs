# D05 指标

- 发现总数: 83
- 处置: ① 11 / ② 72 / ③ 0 / ④ 0 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 18 / 接受 0 / 实施 0 / 登记 18
- 成本: agent 6（读者 A/B 合并）/ token 未采集 / 耗时 面板约 3h（含批量 inactive 重试）+ 修复约 30min

## 说明

- 发现总数 = FACT 18 + DESIGN-D2 18 + READER-A 12 + READER-B 14 + READER-C 21 = 83。
- ① 11 条：FACT-01/02/05/06/11/17；DESIGN-04（文档半边）；READER-B-03/B-04、READER-C-08/C-09（dup-of FACT-05/06）。
- ② 72 条：FACT 12、DESIGN 17、READER-A 12、READER-B 12、READER-C 19。
- P-REFUTE 的 CLAIM 04/05 是 P1，已在 §2.6/§4.4 修复；其内容不计入 FINDING 行。
- D05 状态：`NOT CONVERGED`（design-debt 18 条、teaching-debt 47 条、test-debt 1 条）。

## meta-review

- 报告：`rewrite-notes/coordination/review-eval/meta/05-clock-interrupt-init-review.md`
- 总体：`FLAT`；运行质量 `NOT PASS`。
- 抽样：22/83（26.5%）；严格一致性 16/22 = 72.7%（宽口径 77.3%，剔除 4 条重复项 88.9%）。
- FACT-01 首修只改了 §5.3 M4，§5.1 表仍写 9；已补修为 10。
- CLAIM 04 补写源码准确但不完整：APIC 支路 LVT 门在 `init_local_timer` 编程段打开、四处无条件结论未限定；已补写。
- 台账 DESIGN 编号与面板错位：面板 DESIGN-05/17/18 是 P0，台账曾重编号成 01/02/03；已按面板 ID 修正，并把 4 条重复 reader 项标 `dup-of`。
- 新增盲区：B-25 台账 ID 重编号、B-26 非 FINDING 面板豁免、B-27 平台回收角色缩编、B-28 暂存 diff 无反向对账。
- 四条 P0 设计项中 3 条 follow-up 有文件与验收判据；DESIGN-04 的代码行为无文件/验收，转用户裁决。
- 局限：同 agent 验证；未跑 QEMU 与完整测试；token 未采集。
