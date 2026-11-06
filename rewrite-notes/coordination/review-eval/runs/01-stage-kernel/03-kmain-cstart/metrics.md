# D03 指标

- 发现总数: 81
- 处置: ① 24 / ② 57 / ③ 0 / ④ 0 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 19 / 接受 6 / 实施 2 / 登记 17
- 成本: agent 7 / token 未采集 / 耗时 面板约 60min + 修复约 1.5h

## 说明

- 发现总数 = FACT 16 + DESIGN-D2 19 + READER-A 12 + READER-B 17 + READER-C 17 = 81。P-REFUTE 的 6 条主张中 1 条反证成功（CLAIM 02：旧 memmap 链 + `arch_init` APIC 分支）、其余反证失败；其 P0/P1 已并入 FACT-01/07。
- ① 24 条：FACT 16、DESIGN 2（10/11）、READER-A 2（A-09/A-10）、READER-B 2（B-01/B-13）、READER-C 2（C-02/C-03）。
- ② 57 条：DESIGN 17、READER-A 10、READER-B 15、READER-C 15。
- 误报 0；这不等于精度完美，reader B/C 的转出项可能含少量措辞偏好。
- D03 状态：`NOT CONVERGED`（design-debt 17 条、teaching-debt 若干、test-debt 1 条：`minix-arch` 测试模块编译失败）。

## meta-review

- 报告：`rewrite-notes/coordination/review-eval/meta/03-kmain-cstart-review.md`
- 总体：`FLAT`（检测面 IMPROVED，处置面 REGRESSED）
- 运行质量：`NOT PASS`
- 抽样：25/81（30.9%）；处置一致性 17/25 = 68.0%（宽口径 22/25 = 88.0%）
- 24 条 ① 里只有 17 条把发现点名的位置全部改到（70.8%）；7 条部分或错位：FACT-02 §4.7、FACT-05 §2.5/§4.7、FACT-06 Phase 清单、FACT-13 两处路径、READER-C-03 §5.4 M10、B-13/B-14 台账互换。已在 meta-review 后补修。
- 新增盲区：B-20 修复语句自身事实未复算、B-21 文档修正与被引用代码注释脱钩、B-22 处置类按来源角色分配。
- 四条 P1 设计转出（DESIGN-01/02/04/19）按“代码改动不属于文档轮”口径保留；D03-F02/F11 已补具体文件与验收判据。
- 局限：同 agent 验证；未跑 QEMU；token 未采集。
