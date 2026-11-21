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

## R1 完整 survey

- 发现总数: 49（survey 34 + certify 15：FACT 8、DESIGN-D2 16、READER 4、REFUTE 6、CERT 15）
- 唯一根因数: 49
- 本轮新准入: 49
- certify 新准入: 0（certify 的 9 条已修；二次回读 0）
- 重复合并: 0
- 自引入: 0
- 处置: ① 35 / ② 4 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 2 / SUGG 8
- 未闭环: 0
- 轮结果: ROUND-CLOSED
- 代裁决: 1（D-053）
- ARCH 提案: 1（D2-16）
- 用户否决: 0
- 轮型: full
- edition: V3.3
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 3 / 接受 3 / 实施 0 / 登记 3
- 成本: agent 4 / token 未采集 / 耗时 约 3 小时
- 回读与回归: 文档六门通过（citation 42 块全跳过，见 §8）；anchor 75/75；doc-code-map 21/21；无代码行为改动

## R2 确认轮

- 发现总数: 48（FACT 6、DESIGN-D2 10、READER-ABC 27、CERT 5）
- 唯一根因数: 48
- 本轮新准入: 48
- certify 新准入: 0（certify 的 5 条已修；最终同 agent grep 复核）
- 重复合并: 0
- 自引入: 0
- 处置: ① 41 / ② 2 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 5
- 未闭环: 0
- 轮结果: CONVERGED
- 代裁决: 1（D-053 延期）
- ARCH 提案: 0
- 用户否决: 0
- 轮型: confirmation
- edition: V3.3
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 2 / 接受 2 / 实施 0 / 登记 2
- 成本: agent 3 / token 未采集 / 耗时 约 3 小时
- 回读与回归: 文档六门通过；certify 已跑（R2 收口时完成，见 R2 节回执指针）；无代码行为改动

## R3 确认轮

- 发现总数: 74（FINDING 22 + SUGGESTION 46 + CLAIM 6；与 `panel/round3/INDEX.md` 同源）
- 唯一根因数: 29（RD-R3-01..29，按角色/严重度/处置类聚簇）
- 本轮新准入: 29
- certify 新准入: 0
- 重复合并: 0（聚簇内多发现同行，未跨簇合并）
- 自引入: 0
- 处置: ① 5 / ② 5 / ④ 3 / ⑤ 0 / ⑥ 0 / FP 5 / SUGG 56
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 1（D-066：5 条设计 P2 延期到 R3-design-batch）
- ARCH 提案: 2（RD03-A1/A2，待用户裁决）
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 5 / 接受 5 / 实施 0 / 登记 5
- 成本: agent 9（六角色 + certify 首轮 + 复核）/ token 未采集 / 耗时 约 3.5 小时
- 回读与回归: 文档六门绿（anchor 75/75、citation checked=32 failed=0、structure/ref/style error=0/density 通过、doc-code-map 21/21）；独立回执 certify/round3-certify.md（首轮 6/2，改 3 处后 8/0 PASS）

## R39 确认轮（V3.10）

- 发现总数: 13
- 唯一根因数: 13
- 本轮新准入: 13
- certify 新准入: 0
- 重复合并: 0
- 自引入: 2（D2 的 GDB 配方分两次补齐、D4 的组标题，均 P1/P3，已修）
- 处置: ① 13 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 0
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- 确认轮修复: 2/2 批次（第一批 8 单元、第二批 6.2；含 D2 同缺陷补修）
- 抽样: 9/44 冻结单元（20%；含 fallback/none 锚点）
- 回读与回归: 文档七门绿（anchor 75/75、citation 32 checked/0 failed、structure/ref/style error=0/density、publication/tutorial 0 新增）；code-publication 22 文件 0 命中；cargo：minix-arch 266/0、minix-kernel mock 866/0/8、minix-elf 27/0；QEMU：test-protection x86_64/aarch64/riscv64 三腿 PASS
