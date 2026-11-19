# D02 指标

- 发现总数: 69
- 处置: ① 24 / ② 42 / ③ 1 / ④ 2 / ⑤ 0
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 15 / 接受 8 / 实施 5 / 登记 10
- 成本: agent 7 / token 未采集 / 耗时 面板约 40min + 修复约 1h

## 说明

- 发现总数 = FACT 12 + DESIGN-D2 15 + READER-A 12 + READER-B 14 + READER-C 16 = 69。P-REFUTE 的 6 条主张中 5 条反证失败（主张成立）、1 条反证成功（aarch64 `isb` 失败推演）；其内容已并入 FACT-01/02、DESIGN-01/02/12 与 §4.3 修复，不单独计为发现。
- ① 24 条：FACT 12、DESIGN 5（01/02/09/12/14）、READER-A 1（A-08）、READER-B 1（B-04）、READER-C 5（C-01/02/03/05/10）。
- ② 42 条：DESIGN 8、READER-A 10、READER-B 13、READER-C 11。
- ③ 1 条：READER-C-15（`_MINIX_MAGIC` 条件定义，文档正确）。
- ④ 2 条：DESIGN-10（Paging/HugePages/BootArch 拒绝项）、DESIGN-15（翻译残留分类项）。
- 误报 0；这不等于精度完美，reader B/A 的转出项可能含少量措辞偏好。
- D02 状态：`NOT CONVERGED`（design-debt 8 条、teaching-debt 若干、test-debt 1 条：`cargo test -p minix-arch` 测试模块编译失败，沿用 D01-F01）。

## meta-review

- 报告：`rewrite-notes/coordination/review-eval/meta/02-higher-half-kernel-review.md`
- 总体：`FLAT`（相对 D01 的 V1 基线）
- 运行质量：`NOT PASS`
- 抽样：26/69（37.7%）；处置一致性 22/26 = 84.6%（最宽松 88.5%，低于 90%）
- 四条不成立：READER-A-08（应 ①，同 C-03）、READER-A-10（应 ②，只应用了起点修法）、READER-B-04（应 ①，同 FACT-09）、READER-C-15（应 ③，`_MINIX_MAGIC` 条件定义，文档正确）。已按复核改正台账。
- 新增盲区：B-17 轨迹门只认 `## FINDING` 形状、B-18 跨角色重复计数、B-19 材料矩阵未覆盖作者前置区。
- 三条 P1 复核：FACT-01 的 113 正确；DESIGN-02 的 aarch64 理由正确；`isb` 修复与 `enable()` 的屏障一致，残留的 §4.3 第 921 行与源码注释已同轮改掉。
- 局限：同 agent 验证；未跑 QEMU/`check-layout.sh`；token 未采集。

## R1 完整 survey

- 发现总数: 53（survey 33：FACT 14、DESIGN-D2 15、READER-ABC 2、REFUTE P2 2；certify 16：FACT 4、READER 12；另 REFUTE 6 条主张中有 2 条 P2 已并入 CLAIM-01/03 行）
- 唯一根因数: 53
- 本轮新准入: 53
- certify 新准入: 0（certify #1 的 16 条已在本轮修完；certify #2 重读为 0）
- 重复合并: 0（同根因合并行仍保留独立根因 ID，避免与门的分档聚合冲突）
- 自引入: 3（RD1-35/36/37 由本轮修复句引入；origin R1；已修）
- 处置: ① 44 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 2 / SUGG 7
- 未闭环: 0
- 轮结果: ROUND-CLOSED
- 代裁决: 0
- ARCH 提案: 2（D-050/D-051，待用户裁决）
- 用户否决: 0
- 轮型: full
- edition: V3.3
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 8 / 接受 8 / 实施 0 / 登记 8（设计批 D2-01..06/13）
- 成本: agent 5 / token 未采集 / 耗时 约 4 小时
- 回读与回归: 六门全绿；`cargo test -p boot-shim --lib -- --list` 125、loader `#[test]` 22、minix-elf `#[test]` 23；无代码行为改动，QEMU 未跑

## R2 确认轮

- 发现总数: 29（survey 21 + certify 8：FACT-A 3、FACT-B 1、DESIGN-D2 9、READER-A 1、READER-B 1、READER-C 6、CERT 8）
- 唯一根因数: 29
- 本轮新准入: 29
- certify 新准入: 0（certify #2/#3 的 8 条已在本轮修完；最终回读为 0）
- 重复合并: 0
- 自引入: 0
- 处置: ① 22 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 7
- 未闭环: 0
- 轮结果: CONVERGED
- 代裁决: 0
- ARCH 提案: 1（D-052，待用户裁决）
- 用户否决: 0
- 轮型: confirmation
- edition: V3.3
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 成本: agent 6 / token 未采集 / 耗时 约 2 小时
- 回读与回归: 六门全绿；certify #1 21 条全修、certify #2 0；无代码行为改动，QEMU 未跑

## R3 确认轮

- 发现总数: 95（FINDING 41 + SUGGESTION 48 + CLAIM 6；与 `panel/round3/INDEX.md` 同源。refute 原用 `RD3-CLAIM-<NN>` 编号，主 agent 在文件末尾追加了机器可读的 `## CLAIM 1..6` 索引行，正文未改动）
- 唯一根因数: 37（RD-R3-01..37，按角色/严重度/处置类聚簇）
- 本轮新准入: 37
- certify 新准入: 0
- 重复合并: 0（聚簇内多发现同行，未跨簇合并）
- 自引入: 0
- 处置: ① 13 / ② 11 / ④ 17 / ⑤ 0 / ⑥ 0 / FP 5 / SUGG 49
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 1（D-061：11 条设计 P1/P2 延期到 R3-design-batch）
- ARCH 提案: 2（RD3-ARCH-01/02，待用户裁决）
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 11 / 接受 11 / 实施 0 / 登记 11
- 成本: agent 8（六角色 + certify + 复核）/ token 未采集 / 耗时 约 4 小时
- 回读与回归: 文档六门绿（anchor 55/55、citation checked=25 failed=0、structure/ref/style error=0/density 通过、doc-code-map 29/29）；独立回执 certify/round3-certify.md（首轮 19 条 14 CLOSED / 5 STILL-BROKEN，5 条按证据改正后复核）
