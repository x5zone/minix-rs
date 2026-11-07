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
