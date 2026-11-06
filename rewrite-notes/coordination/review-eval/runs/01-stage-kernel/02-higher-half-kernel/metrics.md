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
