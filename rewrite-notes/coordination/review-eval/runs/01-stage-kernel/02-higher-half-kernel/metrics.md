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

- 发现总数: 95（FINDING 41 + SUGGESTION 48 + CLAIM 6；与 `panel/round3/INDEX.md` 同源。refute 原用 `RD3-CLAIM-<NN>` 编号，主 agent 在文件末尾追加了机器可读的 `## CLAIM 11..16` 索引行，正文未改动）
- 唯一根因数: 37（RD-R3-01..37，按角色/严重度/处置类聚簇）
- 本轮新准入: 37
- certify 新准入: 0
- 重复合并: 0（聚簇内多发现同行，未跨簇合并）
- 自引入: 0
- 处置: ① 13 / ② 11 / ④ 17 / ⑤ 0 / ⑥ 0 / FP 5 / SUGG 49
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 1（D-063：11 条设计 P1/P2 延期到 R3-design-batch）
- ARCH 提案: 2（RD3-A01/02，待用户裁决）
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 11 / 接受 11 / 实施 0 / 登记 11
- 成本: agent 8（六角色 + certify + 复核）/ token 未采集 / 耗时 约 4 小时
- 回读与回归: 文档六门绿（anchor 55/55、citation checked=25 failed=0、structure/ref/style error=0/density 通过、doc-code-map 29/29）；独立回执 certify/round3-certify.md（首轮 19 条 14 CLOSED / 5 STILL-BROKEN，5 条按证据改正后复核）

## R4 确认轮

- 发现总数: 79（FINDING 36 + 编号 SUGGESTION 37 + CLAIM 6；另有 5 条 fact 未编号建议，单列）
- 唯一根因数: 79（ID 口径；RD-R4-001..079，逐 ID 立根因；工具按根因 ID 对账）
- 语义根因数: 47（跨角色/同根聚簇后；含 4 条主张维持为 51 个主题；聚簇见下）
- 本轮新准入: 79
- certify 新准入: 0（round4-fact/round4-reader 回执全 CLOSED 后复读；同根残项不计新根）
- 重复合并: 28（语义口径；ID 未合并，旧口径的 `0` 仅指台账没有合并 ID）
- 自引入: 2（均轮内修复：§3.2 题句由 `d9ceba0cb` 修；§5.1「三类→四类」为 post-certify P3）
- 严重度: P0 4 / P1 5 / P2 20 / P3 7（FINDING 36；5 条 P3 FINDING 按 G3 分流 SUGG：DESIGN-D2-07/08/09、READER-B-05/B-06）
- 处置: ① 19 / ② 2 / ④ 12 / ⑤ 0 / ⑥ 0 / FP 4 / SUGG 42
- 未闭环: 0
- 轮型: confirmation
- 轮结果: CONVERGED
- 代裁决: 2（D-105 D2-04、D-106 D2-06 延期 R4-design-batch）
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.4
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 2 / 接受 2 / 实施 0 / 登记 2
- 减法: 提案 10 / 采纳 3（D2 SUBTRACTION-TABLE）
- 成本: agent 8（6 面板 + 2 certify）/ token 未采集 / 耗时 约 5 小时
- 回读与回归: 文档六门绿（anchor 54/54、citation checked=24 failed=0、structure/ref/style error=0/density、doc-code-map 30/30）；check-layout all PASS（x86 PT_LOAD 5→4）；test-sysboot PASS、test-riscv64-uboot-shim 6/6 PASS；minix-elf 27/0、boot-shim 146/0、minix-kernel mock 866/0/8；`xtask image --arch x86_64 -r` 重建后镜像内 kernel 与 staging 同哈希 `fca5b85f…`、4 LOAD / 0 `.lrodata`

### R4 指标口径说明

- **FP 4 的语义**：对应 `CLAIM-01..04` 的「反证失败、主张成立」，不是误报。本轮真正的误报 FINDING = 0；`FP` 是 `eval-trace-check.py` 的既有桶名，改桶名会破坏工具契约，因此在指标里并列写出语义，避免把「主张维持」读成「精度差」。
- **唯一根因数 vs 语义根因数**：`唯一根因数: 79` 是工具要求的根因 ID 数（RD-R4-001..079），不是语义缺陷数；聚簇后约 47 个缺陷根（含 4 条主张维持为 51 个主题），至少 28 个 ID 是跨角色或同根重复。R3 曾用「95 ID / 37 根因」的聚簇口径，R4 没有沿用，故两个口径并列。
- **5 条过期 SUGG**：`READER-A-S03`（第 5 行指针）已由 `READER-B-01/C-02` 修掉；`READER-A-S06`、`READER-B-S11`、`READER-C-S10(a)`（§5.2 重复 `kern_size`）已由 `FACT-07/DESIGN-D2-10` 修掉；`READER-B-S14`（ARM「待补」）已由 `FACT-09` 修掉。台账处置行已加 `dup-of` 说明，不再写成「待登记」。
- **未编号建议**：P-FACT 的 5 条 `## SUGGESTION` 没有 ASCII ID，`panel-index.py` 不计入 INDEX；已在 `suggestions.md` 以 `FACT-S1..S5` 事后编号登记，工具链缺口记 B-72。
- **post-certify P3**：§5.1 的「三类畸形」是批一引入、`round4-fact.md` 观察段抓到但未入判定集的精度问题；本轮改为「四类畸形」并列出四个 `ElfError` 变体，文档 sha 从 `9f9ec2e7…` 变为 `48630c90…`。这是判定集外修复，18/12 个判定点的结论不变。

### R4 语义聚簇

| 簇 | 成员 | ID 数 |
|---|---|---:|
| §3.2 ELF 迭代器 fail-open / Err 模型 | FACT-01/02/06、DESIGN-D2-01、READER-A-01、READER-B-03、READER-C-01、CLAIM-05 | 8 |
| 测试账与测试载体计数 | FACT-05/07、DESIGN-D2-10、READER-A-S06、READER-B-S11、READER-C-S10(a) | 6 |
| v1 handoff 载荷 | FACT-04、DESIGN-D2-03、CLAIM-06 | 3 |
| `kern_stack_top` 字段残留 | FACT-12、READER-B-04、READER-C-03 | 3 |
| §3.1 表格行号 | READER-B-01、READER-C-02、READER-A-S03 | 3 |
| 其余两两同根 | A-02/C-04、D2-05/C-05、FACT-03/D2-02、FACT-09/B-S14、A-S10/C-S05、A-S07/C-S08、B-06/A-S01、C-06/A-S02、B-S05/C-S06、B-S09/C-S10(e)、B-S03/B-S04 | 11 对 |

> 聚簇只用于指标说明，不改台账的逐 ID 行；工具仍按 79 个根因 ID 对账。
