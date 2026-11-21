# D04 指标

- 发现总数: 88
- 处置: ① 15 / ② 73 / ③ 0 / ④ 0 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 21 / 接受 0 / 实施 0 / 登记 21
- 成本: agent 7 / token 未采集 / 耗时 面板约 90min + 修复约 1h

## 说明

- 发现总数 = FACT 17 + DESIGN-D2 21 + READER-A 16 + READER-B 12 + READER-C 22 = 88。
- P-REFUTE 的 6 条主张中 3 条 P1（§1.4 例外清单、51→53、4→6）已并入 FACT-01/02 与 §1.4 修复；其余 P2 转出。
- ① 15 条：FACT-01..10、12..16。
- ② 73 条：FACT-11/17、DESIGN 21、READER-A 16、READER-B 12、READER-C 22。
- 误报 0；reader 转出项可能含少量措辞偏好。
- D04 状态：`NOT CONVERGED`（design-debt 21 条、teaching-debt 50 条、test-debt 1 条）。

## meta-review

- 报告：`rewrite-notes/coordination/review-eval/meta/04-platform-discovery-review.md`
- 总体：`FLAT`（检测面 IMPROVED，处置面/轨迹面 REGRESSED）
- 运行质量：`NOT PASS`
- 抽样：33/88（37.5%）；严格一致性 23/33 = 69.7%，宽口径 28/33 = 84.8%
- 15 条 ① 只有 10 条干净；部分成立：FACT-01（§6.2 输出）、FACT-03（块内仍是旧 `MachineStruct`）、FACT-04（§7 仍写 `arch_boot_impl`）、FACT-08（“归四类”与“三类故障”并存）、FACT-13（§2.5 仍写“实现对称”）。已在 meta-review 后补修。
- P-REFUTE 附加表整体错位（B-23）；V2 的 `dup-of` 与 C-23 大桶禁令未落地（B-24）。
- DESIGN-01/02/03 三条 P1 转出合理，文件与验收判据都有；D04-F05 的验收不是机械可判。
- D1 `DEGRADED` 足够、不必作废；D2 的 C 真值是二手的。
- 局限：同 agent 验证；未跑 QEMU；token 未采集。

## R1 完整 survey

- 发现总数: 55（FACT 8、DESIGN-D2 12、READER-ABC 33、CERT 2）
- 唯一根因数: 55
- 本轮新准入: 55
- certify 新准入: 0（certify 2 条已修；同 agent grep 复核）
- 重复合并: 0
- 自引入: 0
- 处置: ① 36 / ② 6 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 13
- 未闭环: 0
- 轮结果: ROUND-CLOSED
- 代裁决: 1（D-054）
- ARCH 提案: 1（D2-06）
- 用户否决: 0
- 轮型: full
- edition: V3.3
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 6 / 接受 6 / 实施 0 / 登记 6
- 成本: agent 4 / token 未采集 / 耗时 约 3 小时
- 回读与回归: 文档六门通过（citation 全跳过，MANUAL_FALLBACK）；anchor 72/72；doc-code-map 52/52

## R2 确认轮

- 发现总数: 43（FACT 10、DESIGN-D2 16、READER-ABC 14、CERT 3）
- 唯一根因数: 43
- 本轮新准入: 43
- certify 新准入: 0（certify 3 条已修；同 agent grep 复核）
- 重复合并: 0
- 自引入: 0
- 处置: ① 18 / ② 6 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 19
- 未闭环: 0
- 轮结果: CONVERGED
- 代裁决: 1（D-054 延期）
- ARCH 提案: 1（D2-R2-16）
- 用户否决: 0
- 轮型: confirmation
- edition: V3.3
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 6 / 接受 6 / 实施 0 / 登记 6
- 成本: agent 3 / token 未采集 / 耗时 约 3 小时
- 回读与回归: 文档门通过；certify 已跑（R2 收口时完成 3 条修复，同 agent grep 复核，见 FREEZE）；无代码行为改动

## R3 确认轮

- 发现总数: 70（FINDING 27 + SUGGESTION 37 + CLAIM 6；与 `panel/round3/INDEX.md` 同源）
- 唯一根因数: 29（RD-R3-01..29，按角色/严重度/处置类聚簇）
- 本轮新准入: 29
- certify 新准入: 0
- 重复合并: 0（聚簇内多发现同行，未跨簇合并）
- 自引入: 0
- 处置: ① 10 / ② 3 / ④ 7 / ⑤ 0 / ⑥ 0 / FP 5 / SUGG 45
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 1（D-070：3 条设计 P2 延期到 R3-design-batch）
- ARCH 提案: 1（RD04-A1：aarch64 ACPI 腿的 PSCI conduit，待用户裁决）
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 3 / 接受 3 / 实施 0 / 登记 3
- 成本: agent 9（六角色 + certify）/ token 未采集 / 耗时 约 3 小时
- 回读与回归: 文档六门绿（anchor 77/77、citation checked=68 failed=0 warned=7、structure/ref/style error=0/density 通过、doc-code-map 54/54）；独立回执 certify/round3-certify.md（12/12 CLOSED 首轮即 PASS）

## R39 确认轮（V3.10）

- 发现总数: 15
- 唯一根因数: 15
- 本轮新准入: 15
- certify 新准入: 0
- 重复合并: 0
- 自引入: 4（4.0/2.0 的教程脚手架与 5.4 标点残迹，均修）
- 处置: ① 15 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- 确认轮修复: 2/2 批次（第一批 11 单元、第二批同缺陷补修）
- 抽样: 11/54 冻结单元（20%）
- 回读与回归: 文档七门绿（anchor 70/70、citation 68 checked/0 failed、structure/ref/style error=0/density、publication/tutorial 0 新增）；code-publication 53 文件 0 命中；cargo：minix-arch 266/0、minix-platform 19/0、minix-kernel mock 866/0/8，minix-plat check 通过；QEMU：test-smp-topo x86_64/aarch64/riscv64 三腿 PASS
