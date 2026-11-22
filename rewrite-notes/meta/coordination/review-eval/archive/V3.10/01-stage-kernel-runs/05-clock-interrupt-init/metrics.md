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

## R1 完整修改轮

- 发现总数: 70（FINDING 25 + SUGGESTION 39 + CLAIM 6；与 `panel/round1/INDEX.md` 同源）
- 唯一根因数: 29（RD-R1-01..29，按角色/严重度/处置类聚簇；D2-03 与 FACT-11 同根按 dup-of 合并、D2-06 记 SUGG）
- 本轮新准入: 29
- certify 新准入: 0
- 重复合并: 0（聚簇内多发现同行，未跨簇合并）
- 自引入: 0
- 处置: ① 15 / ② 0 / ④ 7 / ⑤ 0 / ⑥ 4 / FP 5 / SUGG 39
- 未闭环: 0
- 轮型: full
- 轮结果: ROUND-CLOSED
- 代裁决: 1（D-075：6 条设计项按 [MODERN]/[DEL] 当轮 ⑥ 实施；ARCH 提案 RD05-A1 待用户亲裁）
- ARCH 提案: 1（RD05-A1：设备窗口校验与 VA 注入收进控制器接口）
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 6 / 接受 6 / 实施 4（D2-01 注释、D2-04 删方法、D2-05 re-export、D2-06 debug_assert）/ 登记 2（D2-02/03 的文档半边随 fact 批修）
- 成本: agent 10（六角色，reader-b/refute 各超时重试一次）/ token 未采集 / 耗时 约 5 小时
- 回读与回归: 文档六门绿（anchor 105/105、citation checked=43 failed=0、structure/ref/style error=0/density 通过、doc-code-map 33/33）；`cargo check -p minix-arch` 绿、`cargo test -p minix-kernel --features mock --lib` 866/0；独立回执 certify/round1-certify.md（首轮 12/3，其中 2 条为任务书笔误混入的 D04 条目；残留 5 处按证据改正后以复核为准）

## R2 确认轮

- 发现总数: 69（FINDING 21 + SUGGESTION 42 + CLAIM 6；与 `panel/round2/INDEX.md` 同源）
- 唯一根因数: 28（RD-R2-01..28，按角色/严重度/处置类聚簇）
- 本轮新准入: 28
- certify 新准入: 0
- 重复合并: 0（聚簇内多发现同行）
- 自引入: 0
- 处置: ① 17 / ② 0 / ④ 6 / ⑤ 0 / ⑥ 0 / FP 3 / SUGG 43
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 0
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 9（六角色 + certify）/ token 未采集 / 耗时 约 3 小时
- 回读与回归: 文档六门绿（anchor 105/105、citation checked=43 failed=0、structure/ref/style error=0/density 通过、doc-code-map 33/33）；`cargo check -p minix-arch` 绿；独立回执 certify/round2-certify.md（8/8 CLOSED 首轮即 PASS）

## R39 确认轮（V3.10）

> **INVALIDATED（2026-11-22）**：本文件中的 R39 是 D01 的轮次，不是本篇轮次；D-114 仅适用于 D01。原“CONVERGED”宣告作废，状态为 CONVERGENCE-CANDIDATE。纠正记录见 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/ROUND-IDENTITY-CORRECTION.md`。

- 发现总数: 4（3 面板 + 1 测试载体）
- 唯一根因数: 4
- 本轮新准入: 4
- certify 新准入: 0
- 重复合并: 0
- 自引入: 1（D1 的第四处残留，P1，已修）
- 处置: ① 4 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- 轮型: confirmation
- 轮结果: INVALIDATED（原写 CONVERGED）
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- 确认轮修复: 2/2 批次（第一批 3.0、第二批同缺陷补修）
- 抽样: 10/47 冻结单元（21%）
- 回读与回归: 文档七门绿（anchor 100/100、citation 43 checked/0 failed、structure/ref/style error=0/density、publication/tutorial 0 新增）；code-publication 33 文件 0 命中；cargo：minix-arch 266/0、minix-kernel mock 866/0/8；QEMU：test-timer-irq 三架构 PASS，riscv64 连跑 4 次 PASS
