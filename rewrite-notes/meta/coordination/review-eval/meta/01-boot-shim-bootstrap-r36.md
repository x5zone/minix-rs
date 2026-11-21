# 01 · boot-shim-bootstrap · R36 流程评估（P-META）

- 审计日期：2026-11-21
- 审计者：P-META（独立冷启动；未参与 R36 面板、修复或 certify；只读，唯一写入是本文件）
- 审计对象：`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/` 的 `loop.md` R36 小节、`metrics.md` R36 块、`defect-ledger.md` R36 表、`OPEN-DEBT.md`、`suggestions.md`、`certify/round36-*.md`、`panel/round36/`
- 基线状态：HEAD `624fcccf158c4c24fc9043e89aaa287856234f1f`（`git rev-parse HEAD` 实测）；文档 sha256 `1c0d9de478c09760a77e4068ac8190c82c0d88c0c36fe372e791d9e524c11660`，与 `49ed0c54a`、`624fcccf1` 两个提交的文档对象逐字节相同
- 规范依据：`prompt/review-rules/review-panel.md` §3/§4.3/§5、`prompt/review-rules/review-evals.md` §4.0b–§4.0f/§5/§6A/§9、`prompt/skill/review-loop.md` L5/L6/L6.5、`WORKFLOW-VERSIONS.md` 的 V3.4–V3.6

**结论摘要：总体 `IMPROVED`；运行质量 `NOT PASS`；抽样全记录一致 40/43 = 93.0%，处置合规 42/43 = 97.7%，抽样项本身高于 90% 阈值。`NOT PASS` 来自 Scope discipline 与 Trace integrity 的硬问题，不是抽样精度不足。**

## 一、执行摘要

R36 的内容质量是这条循环里目前最高的一轮。证据有三组。第一，确认轮抓到了 R35 之后设计批落地的真实 P0 代码缺陷：`BOOT_HANDOFF_V1_VERSION` 在 `52110580f` 的提交说明里写「9 → 10」，常量却仍是 9；FACT-A-01、FACT-B-01、DESIGN-D2-01、READER-A-01、CLAIM-03 五路独立撞到同一根，主 agent 把常量改成 10 并补了版本历史（`d8ce8b29c`），不是只改文档。第二，设计角色不再停在命名与注释：`SHIM_TABLE` 的字节偏移构造与未对齐 `&BootFileTable` 强转（D2-07）、aarch64 直核腿的第二份手写 bump 边界检查（D2-04）、`prepare_boot` 的两个安装型全局（D2-05）、startup 死代码（D2-03）都是结构性发现；V3.6 减法表给出 8 行提案、4 行采纳，全部复用现有类型或纯删除。第三，读者三角色是独立冷读，产出了事实角色没有覆盖的理解障碍（READER-B-03 的「第三条标记」变体、READER-B-04 的五腿五门链对称差、READER-C-03 的 boot2 边界外推、READER-B-07 的过程词算术），并且 reader certify 的 16 个修改点由另一名冷读者逐条复读。

这一轮不能判 `PASS`，有四类硬问题。

第一，**冻结决策被重新提起，而台账记 `re-litigation: 0`**。`DECISIONS.md:147` 的 D-064（2026-11-20 用户裁决）明确否决路线 A（参数化 `prepare_boot` 签名），选择路线 B（`debug_assert!`→`assert!`，两个 `static mut` 与两个 `install_*` 结构保留）。R36 的 DESIGN-D2-05（`panel/round36/design-d2.md:125-139`）再次提出参数化，`D-104`（`DECISIONS.md:187`）把它延期到 `R36-design-batch`，`OPEN-DEBT.md:49` 的 R36 行把「两个安装型全局参数化」写成待办第一项。D-104 与 R36 台账都没有提到 D-064；loop 的冻结决策检查（`loop.md:2173`）只列 D-061/D-062/D-067/D-071/D-086，漏了 D-064。这是对用户裁决的静默重开，不是新发现。D2-05 中唯一的新内容是 `install_dtb_ptr(0)` 时 `DTB_PTR == 0` 使第二次安装仍可通过断言（0 值写一次漏洞），这一子项成立，但它应与「参数化」拆开：前者可以新立 P2，后者应判 `dup-of: D-064` 或由一条明确取代 D-064 的新裁决承接。

第二，**同根修复的射程仍然按「面板句子」而不是「根因触及的制品集合」定义，导致三轮 certify**。`certify/round36-fact-pass1.md` 在 `5ccd49968` 上判 38 CLOSED / 6 STILL-BROKEN；`-pass2.md` 在 `37238e568` 上判 43/1；最终 `round36-fact.md` 在 `49ed0c54a` 上判 44/0，随后又在 `624fcccf1` 上补读 FACT-B-05 的三处源码注释残项。三轮不是重复劳动，而是第一轮只修了「文档句」，没有把同一根的源码 docstring、测试注释、未用 import 一起收掉。最后两批修复提交 `49ed0c54a` 与 `624fcccf1` 不在 R36 台账的任何一行里，`metrics.md` 也没有引用它们；`certify 新准入 = 0` 的口径（`loop.md:2202`）把这类「同根修复不完整」排除在新根因之外，账面自洽，但账本无法回指最终修复。

第三，**认证历史被事后改写，且轨迹产物在审计时未提交**。`round36-fact-pass1.md` 的正文汇总写 `CLOSED 38 / STILL-BROKEN 6`（第 65 行），文件末尾却被追加了「本轮最终结论：CLOSED/PASS（44/44 修改点）」（第 73 行）；`round36-reader-pass1.md` 的正文写 `CLOSED 12 / STILL-BROKEN 4`（第 26 行），末尾同样追加「16/16」（第 29 行）。这两处追加在 `git diff` 里是未提交修改；它们与正文直接矛盾。一个按文件名与末尾结论读取的门会得到 PASS，一个按正文读取的读者会得到 NOT PASS。这是 B-61（收口后的未提交修复）与 B-43（迟交回读读旧快照）的合体，不是措辞问题。

第四，**盲区账本没有本轮增量**。`loop.md:2228` 的 L6.5 写「已记 BLIND-SPOTS 候选（确认轮同根残项需按『修复点集合=文档句+注释+引文』定义）」，但 `BLIND-SPOTS.md` 全文对 `R36`、`裸 ID`、`同根残项` 零命中，最后一条是 R23 时代的 B-64 与 B-67。L6.5 的产物没有落地。与此同时，`suggestions.md` 的 R36 段只按组登记（`READER-A S01–S10`、`READER-B S-B-01–S10`、`READER-C S01–S05`），25 条编号建议没有逐 ID 的处置与证据；8 条 fact 角色未编号建议连 ID 都没有，台账与 `panel-index.py` 都看不见。`loop.md:2160` 还说 `INDEX.md` 是「FINDING 62 / SUGGESTION 33 / CLAIM 6」，实际 `INDEX.md` 的 SUGGESTION 列合计是 25（fact-a 0、fact-b 0、reader-a 10、reader-b 10、reader-c 5）。

`IMPROVED` 的依据是「抓到真 P0、设计发现结构性、读者独立且有独有发现」；`NOT PASS` 的依据是「用户裁决被静默重开、同根射程靠三轮 certify 补、认证历史事后改写、盲区账本未更新」。

## 二、四个流程数

| 流程数 | 记录值 | 独立复判 | 判定 |
|---|---|---|---|
| 唯一根因数 | 93（`metrics.md:285`、`loop.md:2172` 写作 `RD-R36-001..093`） | 台账实际是 93 行，但 ID 最大到 `RD-R36-095`，中间缺 `016` 与 `036`；按根因文本聚类，实质缺陷簇约 35 个（wire 版本、v10 删除、strict/postconditions、调用点、ghost API、所有权、Paging、测试账、build_memmap、aarch64 x0、行号锚点、cfg 计数、land_slice、探针描述、user_sp、step1+2、loader 引文、ceil 不等式、静默跳过、SHIM_TABLE、aarch64 bump、prepare_boot、startup 死代码、静态 scratch、arena 算术、boot2 边界、第三条标记、五腿差集、§3.7 重复、Ch5 重复、§1.4 结构、过程词、TLB 时点、map_huge、术语缺口），另有 30 条建议与 2 条 FP。 | 账面值不是唯一根因数，是 ID 数；指标名与事实不符。CONCERN |
| 自引入数 | 0（`metrics.md:289`） | `land_slice`/`store_kernel_info` 的 docstring 在本轮文档修正后与正文冲突，属「修复暴露既有注释过期」；它由 certify #3 在 `624fcccf1` 修掉，未计入自引入。按「只算新代码缺陷」的口径 0 成立，按「修复必须同步同根注释」的口径应记 1。 | 口径依赖；无新运行期缺陷。CONCERN |
| 重复合并数 | 0（逐 ID 立根因，`loop.md:2172`） | 至少 7 个跨角色同根簇可合并：wire 版本 7 ID（A-01/B-01/D2-01/RA-01/RB-01 部分/RC-01 部分/CLAIM-03）、v10 删除 14 ID、测试账 6 ID、aarch64 x0 5 ID、build_memmap 2 ID、loader 引文 2 ID、ceil 不等式 2 ID。按当前「ID 即根」的约定，0 是账面值；按「同一缺陷不重复计数」的收敛判据，重复率约 25–30/93。 | 指标失去唯一性语义。FAIL（作为度量） |
| edition 稳定 | V3.4（R35→R36 未变；`loop.md:2134` 写 `schema: V3.4 from R35`） | 版次标签稳定，但工作流版本已到 V3.6（`WORKFLOW-VERSIONS.md:15` 与 V3.6 节），R36 的 D2 报告带 V3.6 减法表、`metrics.md:301` 带 `减法: 提案 8 / 采纳 4`，而 `eval-trace-check.py` 的 `_combined_ver` 由 schema 决定（`tools/eval-trace-check.py:2352-2361`），V3.4 下不跑 V3.6 减法门，也不要求 certify 的 `- 遗留 P0: <n> / P1: <n>` 声明。规则被部分执行，门没有跟着执行。 | 标签稳定，规则边界不稳定。CONCERN |

## 三、九问

| 问 | 结论 | 证据与理由 |
|---|---|---|
| 1 Detection | PASS（有保留） | 最强证据是 wire 版本 P0：基线 `52110580f` 的提交说明写 9→10，`handoff_v1.rs:288` 仍是 `= 9`，FACT-A-01、FACT-B-01、DESIGN-D2-01、READER-A-01、CLAIM-03 五路 ID（fact 两路、design、reader、refute）独立撞到。设计侧还抓到 `SHIM_TABLE` 的字节构造（D2-07）、aarch64 bump 第二份边界检查（D2-04）、探针来源写错（A-14/B-17）、`map_huge` 2 MiB 语义写反（CLAIM-05）。仍漏的四类：① 同根的源码注释/测试注释/import（三轮 certify 才清完）；② 冻结决策 D-064（D2 的材料清单没有决策摘要，主 agent 检查也未列 D-064）；③ 8 条未编号 fact 建议与 25 条只按组登记的建议；④ 盲区账本没有本轮增量。 |
| 2 Precision | CONCERN | 台账 `FP 2`（CLAIM-01/02）是正确分类：两条主张在文档自设口径内成立，反证失败不等于文档有缺陷。但抽样发现 3 条记录级问题：FACT-B-19 的「内核侧没有数量断言」与源码相反（`os/kernel/src/lib.rs:2770-2778` 有 `assert_eq!(len, NR_BOOT_MODULES)`；只是静默跳过时 len 仍为 12，断言不会触发）；DESIGN-D2-06 的「25 成员 / 2 常量 / `DM_HUGE`」与源码相反（`Paging` 是 24 成员 = 1 常量 + 23 方法；`DM_HUGE` 是 `paging.rs:770` 的局部常量，不在 trait 里）；DESIGN-D2-05 把 D-064 已否决的参数化当新提案。误报没有明显的角色模式，但设计角色的这两条说明「设计角色对 Rust 类型面的计数需要事实角色复核」。 |
| 3 Design quality | PASS | 9 条设计发现中 7 条是结构性且落到代码：D2-07 类型化 `SHIM_TABLE`、D2-04 复用 `BumpRegion`、D2-05 安装型全局、D2-03 startup 死代码、D2-09 静态 scratch、D2-06 `PteChannel` 文档缺席、D2-02 v10 台账。减法表 8 行、采纳 4 行，全部是「复用现有类型 / 纯删除 / 纯文档」，没有新增类型。局限：D2-06 的成员数写错；D2-05 的一半是重提 D-064。 |
| 4 Independence | PASS（有保留） | 读者三份报告均声明未读源码与其他角色输出；reader certify 是另一名冷读者，逐条复读 16 点。独有发现包括 READER-B-03/04、READER-C-03/06、READER-B-07、CLAIM-05。19 条读者 FINDING 中约 8 条不是事实角色的复述（B-03、B-04、B-05、B-06、B-08、C-03、C-06、B-07）。保留：最大的五个 P0 簇仍是 fact/design/refute 三路重复撞到，读者独有发现集中在结构与教学面，不在 wire 契约面。D2-05 的独立发现被不完整的决策摘要污染（见第五、六节）。 |
| 5 Scope discipline | FAIL | D-064 是同一子系统的用户裁决，明确否决参数化；R36 D2-05 重提，D-104 延期并写入 OPEN-DEBT，loop §3 的冻结决策检查漏列 D-064，`re-litigation: 0` 不成立。除此之外没有相邻阻塞：`adjacent 阻塞 0` 成立，D2-05 的 0 值漏洞子项、D2-03/D2-09 的 P3 登记都在射程内。 |
| 6 Teaching quality | PASS（有保留） | 读者 FINDING 19 条中 16 条本轮修（④），3 条按 §4.0e 降 P3 登记（B-05/B-06/B-08）；另有 25 条建议登记。修后的 reader certify 由冷读者 16/16 CLOSED，B-64（④ 修复后无冷读者复读）在这轮被实质关闭（至少对读者面修改点）。纯文风噪声低：读者建议里 ESP/%esp、悬挂指针、术语缺口属于理解障碍，不是排版偏好。保留：25 条建议只按组登记，读者无法按 ID 回查；3 条结构建议未实施是合理的，但「索引 → 详情」的分工说明与 §5.1 M4 的实际展开仍有张力（READER-B-06 的原始点）。 |
| 7 Cost | CONCERN | `metrics.md:302` 记 `agent 9（7 面板 + 2 certify）/ token 未采集 / 约 6 小时`。实际 certify 是两侧各三轮（`round36-fact-pass1/-pass2/round36-fact.md`、`round36-reader-pass1/-pass2/round36-reader.md`），共 6 次冷启动回执；按派发次数应记 13 个 agent，不是 9。6 小时比 R35 的 5 小时多 20%，发现 ID 从 78 到 93，但实质根因约 35；按「每实质根因」计成本明显高于 R35。收益不是递减：抓到 P0 代码缺陷值这个成本；但三轮 certify 与 93 个 ID 的记账是主要成本，且 token 仍未采集，无法做严格的收益递减判断。 |
| 8 Trace integrity | FAIL | 详见第五节。可核对的正面读数：93/93 个 ID 有处置行；`93 = 44 + 1 + 16 + 0 + 0 + 2 + 30`；严重度按 FINDING 62 条可复算为 P0 10 / P1 21 / P2 25 / P3 6，CLAIM 另有 P0×2、P1×2、FP×2；`--round R36` 门 PASS（12 PASS / 25 WARN / 0 FAIL）；面板 7 个文件的 sha256 与 `INDEX.md` 一致。失败项：根因 ID 范围写作 001..093 而实际到 095；certify pass1 事后追加 PASS；certify 缺 V3.6 遗留声明；最终两批修复提交不入台账；建议只按组登记；BLIND-SPOTS 未更新；L6.5 的「同一最终基线」前提不成立；D-064 状态过期；成本少记 4 个 agent。 |
| 9 开放题 | 回答见下 | 本轮最值得、但工作流仍看不见的一件事：同根修复的「制品类别射程」没有角色或工具负责。 |

第 9 问的回答：三轮 certify 的表面原因是 FACT-B-05 的注释面没收干净，结构原因是工作流的修复单位是「面板 FINDING 的句子」，而根因的单位是「文档句 + 源码 docstring + 测试注释 + 引文块 + 锚点 + 测试名」。台账的 ① 行只写「冷读回执 CLOSED」，没有字段要求修复者枚举这条根因触及的制品类别，也没有工具对「本轮删掉/改名的符号」做跨文件旧串扫描。于是第一轮修了文档，第二、三轮才逐类发现源码注释、未用 import、测试注释。这个盲区不是「再派一个读者」能解决的，它需要一条根因级的 closure manifest：每个 ① 行至少列 `doc / code-comment / test / citation / anchor` 五类，逐类给证据；对删除或改名的标识符，机械门在 `os/` 与文档里同时扫旧串，命中即未闭合。它比再增加一轮 certify 便宜，也比「冷读说 CLOSED」可复核。

## 四、抽样复判

### 4.1 抽样方法

- P0 全部复判：FINDING P0 10 条 + CLAIM P0 2 条 = 12 条。
- P1 抽 16 条：按台账 ID 排序，取 FACT-A-02/04/07/08/09/10/14/15、FACT-B-07/13/16、DESIGN-D2-01/02/07、READER-A-01、CLAIM-05，覆盖 fact/design/reader/refute 四类来源。
- P2/P3 抽 15 条：固定间隔取 FACT-A-06/A-12、FACT-B-15/B-19、DESIGN-D2-04/05/06、READER-A-03、READER-B-03/04、READER-C-03/04/06、CLAIM-01/02。
- 合计 43 条，占 93 个 ID 的 46%；每条都重放了至少一条 grep、源码读数或测试命令。

判定口径：`一致` = 我的独立判断与台账的缺陷成立性、严重度与处置方向一致；`部分` = 缺陷核心成立，但记录中的证据/计数/前提有一处实质错误；`不一致` = 处置方向本身不成立。

### 4.2 抽样表

| # | 发现 | 严重度 | 台账行 | 复判 | 独立读数与理由 |
|---|---|---|---|---|---|
| 1 | FACT-A-01 | P0 | RD-R36-001 | 一致 | `grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs` → `296: ... = 10`；基线 9、修复后 10；文档 `:1625/:1714` 为「9 → 10」「当前 10」。 |
| 2 | FACT-A-03 | P0 | RD-R36-003 | 一致 | `grep -c STARTUP_RECORD_STORE_LEN` 文档 0；`六个落地区` 3 命中；`七个落地区` 0；代码 `globals.rs` 无 startup store。 |
| 3 | FACT-A-05 | P0 | RD-R36-005 | 一致 | `grep -c '#\[test\]' kernel_info.rs` → 24；15/3/5/1；`test_postconditions_reject` 全仓 0。基线文档声称 31 与 4 条 postcondition 负控，属于虚构测试。 |
| 4 | FACT-B-01 | P0 | RD-R36-017 | 一致 | 与 #1 同根；`handoff_v1_read.rs:107` 精确比较；`git show 52110580f` 的 diff 未改常量。 |
| 5 | FACT-B-02 | P0 | RD-R36-018 | 一致 | 与 A-07 同根；`grep -rn 'commit_map_tables\|map_tables_mut' os/` 零命中；`fill_map_tables` 在 `uefi_helpers.rs:1156`。 |
| 6 | FACT-B-03 | P0 | RD-R36-019 | 一致 | 与 A-03 同根；`STARTUP_RECORD_STORE` 只剩 `handoff_v1_read.rs:763` 一条悬空注释；六类落地区。 |
| 7 | FACT-B-04 | P0 | RD-R36-020 | 一致 | 与 A-15 同根；生产 `store_kernel_info` 仅 `lib.rs:225/347`；`kmain` 形参 `_kernel_info`（`:1806`）且不重存。 |
| 8 | FACT-B-05 | P0 | RD-R36-021 | 一致 | `kernel_info.rs:554-571` 只剩 `kern_virt_base > u32::MAX`；文档已统一为「4 GiB 门」。P0 偏重（文档高估防线，无运行期错误），但 P0-fact 类别成立；三轮 certify 的残项说明首修不完整。 |
| 9 | FACT-B-06 | P0 | RD-R36-022 | 一致 | 与 A-04 同根；`cargo test --list` 实测 146/185/27；基线文档 136/195/23。 |
| 10 | FACT-B-12 | P0 | RD-R36-028 | 一致 | `loader.rs:517-518` 有 `elf_file_end` 两行；`opensbi_helpers.rs:209` 注释明确不再跑 `segment_iter`；文档 `:2572` 已补。P0 对「引文决定模块长度语义」成立。 |
| 11 | CLAIM-03 | P0 | RD-R36-092 | 一致 | 与 A-01 同根；`handoff_v1.rs:296` = 10；`handoff_v1_read.rs:627/633` 的 ±1 负控现在是 9/11。 |
| 12 | CLAIM-04 | P0 | RD-R36-093 | 一致 | 与 A-03/A-15 同根；六落地区、两个调用点、一条 postcondition，均有源码读数。 |
| 13 | FACT-A-02 | P1 | RD-R36-002 | 一致 | `Paging` 实测 24 成员 = `const PAGE_SIZE` + 23 方法（`os/arch/src/arch/paging.rs:212-640`）；`adopt_active_root` 只作历史注释；文档 `:1878` 已改 24。 |
| 14 | FACT-A-04 | P1 | RD-R36-004 | 一致 | 146/185/27 三读数；文档逐文件表与实测一致。 |
| 15 | FACT-A-07 | P1 | RD-R36-007 | 一致 | `commit_map_tables`/`map_tables_mut`/`set_*_len`/`push_startup` 全仓零命中；文档改为 `fill_map_tables` 闭包（6 命中）。 |
| 16 | FACT-A-08 | P1 | RD-R36-008 | 一致 | 文档 `只有一个所有者` 1 命中、`两个所有者` 0；`bootface.rs:376` 与 `bootface_a64.rs:348` 调 `fill_map_tables`；DTB 静态量已删。 |
| 17 | FACT-A-09 | P1 | RD-R36-009 | 一致 | `grep -rn 'build_memmap()' os/qemu-tests` 可执行调用 2 处（`test-memmap:165`、`test-memmap-aarch64:29`），另 1 处注释；`exit_boot_services_and_classify` 12 个文件；14 个可执行点 + 1 个注释放 15，口径在 certify #1 校正后自洽。 |
| 18 | FACT-A-10 | P1 | RD-R36-010 | 一致 | 文档 `x0=0 分道` 0 命中；`诚实停机` 在 §3.1.1/§4.8/§5.1 命中；`main.rs` 两个 cfg 臂。与 RA-04/RB-02/RC-02 同根。 |
| 19 | FACT-A-14 | P1 | RD-R36-014 | 一致 | `test-kernel-map/src/main.rs:78` 分配探针、`:91` 取 `probe_pre`、`:96` 才 `exit_boot_services_and_classify`；文档 `:3305-3307` 已写「最终 EBS 内存图之前」与「最终图分类」。 |
| 20 | FACT-A-15 | P1 | RD-R36-015 | 一致 | `lib.rs:225/347` 两个生产调用点；`kmain` 不重存；文档 `:2867` 两个调用点 + one-shot panic。 |
| 21 | FACT-B-07 | P1 | RD-R36-023 | 一致 | `kernel_info.rs` 24 条、15/3/5/1；文档 `:3267` 与实测一致。 |
| 22 | FACT-B-13 | P1 | RD-R36-029 | 一致 | `lib.rs:1429` Step 2b、`:1475` Step 2c、`:1519` `step1+2 mappings ok`、`:1522` `enable()`；文档 `:3468` 已改为「2b/2c 已完成，卡在 Step 3 或跳转」。 |
| 23 | FACT-B-16 | P1 | RD-R36-032 | 一致 | 与 A-08 同根；§3.4.5 的「两个所有者/set_*_len/push_startup」已改。 |
| 24 | DESIGN-D2-01 | P1 | RD-R36-037 | 一致 | 与 A-01 同根。严重度写 P1，而 fact/refute 同一根写 P0；同一缺陷跨角色严重度从 P0 到 P1 分散，台账没有根级归并。 |
| 25 | DESIGN-D2-02 | P1 | RD-R36-038 | 一致 | 七项 v10/D-071/I-33 台账均已逐处修正；与 A-03/A-07/A-08/A-15 大量重叠。 |
| 26 | DESIGN-D2-07 | P1 | RD-R36-043 | 一致 | `uboot_main.rs:293` 为 `static mut SHIM_TABLE: BootFileTable`；`:301` 有 `size_of::<BootFileTable>() == 16 + 16 * 80` 断言；`build_shim_table` 按字段写。 |
| 27 | READER-A-01 | P1 | RD-R36-046 | 一致 | 与 A-01 同根；文档三处版本号已统一为 10，v9 只作历史。 |
| 28 | FACT-A-06 | P2 | RD-R36-006 | 一致 | C `kinfo` 26 字段（含 `bootstrap_start, bootstrap_len` 双声明）；Rust 保留 4，退场 22；文档 `:1655-1672` 的 19 行表 = 22 字段，含 `user_sp` 行。 |
| 29 | FACT-A-12 | P2 | RD-R36-012 | 一致 | `grep -c 'cfg(feature = "qemu-test-kernels")' uefi_helpers.rs` → 10；文档 `:1528` 写 10 处并列 7 个条目。 |
| 30 | FACT-B-15 | P2 | RD-R36-031 | 一致 | 文档 `:2605` 为 `r > 0 && r < 4096 - A`；`grep 'r > 4096 - A'` 0；反例 `4095+4094` 复算正确。 |
| 31 | FACT-B-19 | P2 | RD-R36-035 | 部分 | 缺陷核心成立：文档说错误会推迟到「期望 12、实际 N」的断言，但静默跳过时 len 仍为 12，该断言不会触发（`lib.rs:2770-2778`）。但面板与台账 action 写「内核侧没有这条数量断言」是错的：断言存在，只是不触发。 certify #1 已在复验列补正为「strict 门位置核名」，action 列仍留旧句。 |
| 32 | DESIGN-D2-04 | P2 | RD-R36-040 | 一致 | `bootface_a64.rs:119` `static BUMP_NEXT: AtomicU64`；`:128` `BumpRegion::resumed` / `alloc_pages`；手写 `need` 已删。 |
| 33 | DESIGN-D2-05 | P2 | RD-R36-041 | 不一致（部分成立） | 0 值写一次漏洞成立：`install_dtb_ptr(0)` 后 `DTB_PTR == 0`，第二次安装仍过断言（`opensbi_helpers.rs:509-517`）。但主提案「参数化 `prepare_boot`」是 D-064（`DECISIONS.md:147`）明确否决的路线 A；D-104 未引用 D-064，OPEN-DEBT 把它写成待办。应按 `dup-of: D-064` 关闭参数化分支，只把 0 值漏洞新立一条。 |
| 34 | DESIGN-D2-06 | P2 | RD-R36-042 | 部分 | 核心成立：文档成员表过期、`new_from_page` 缺 `channel`、`PteChannel` 章内缺席。计数错误：面板写「25 成员 / 2 常量 / `DM_HUGE`」，实际 24 成员 = 1 常量 + 23 方法；`DM_HUGE` 是 `paging.rs:770` 的局部常量。台账 action 用了正确的「24 成员」，但没有记录对面板计数的纠正。 |
| 35 | READER-A-03 | P2 | RD-R36-048 | 一致 | 256 + 7616 + 21312 = 29184，32768 − 29184 = 3584；文档 `:1744` 已拆 v9 30624 / v10 29184。 |
| 36 | READER-B-03 | P2 | RD-R36-062 | 一致 | 文档 `:3311` 已写「前两条固定，第三条随变体」，`DTB_CFGTABLE=1` 变体明确没有 memmap source 行。 |
| 37 | READER-B-04 | P2 | RD-R36-063 | 一致 | 文档 `:3317` 已写「差集有两项」：aarch64 直核面缺专用启动门；riscv64 U-Boot 直载测试链在门链里但不是生产腿。 |
| 38 | READER-C-03 | P2 | RD-R36-081 | 一致 | 文档 `:154` 与 `:256` 已把「必须设栈」限定为 Multiboot 合规引导器，对 boot2 只取证代码事实。 |
| 39 | READER-C-04 | P3 | RD-R36-082 | 一致 | 与 B-15 同根；按 §4.0e 降 P3 建议合理；文档已顺手修。 |
| 40 | READER-C-06 | P3 | RD-R36-084 | 一致 | 与「重建活动根」同根；按 P3 建议登记合理；文档 `:1664` 已改成「下一个冷取指页错误（TLB 残留可能先跑几条）」，实际顺手修了，但台账记 SUGG，未计 ④。 |
| 41 | CLAIM-05 | P1 | RD-R36-094 | 一致 | 基线文档 `§4.6` 确写「三个架构的 2 MiB 路径都直接重写同一个叶并返回 Ok(())」；源码三架构在 2 MiB 叶已存在时返回 `AlreadyMapped`（x86 `:1325-1327`、arm64 `:1248-1250`、riscv `:1249-1251`）；现文档 `:2939-2940` 已改。这是 refute 的独有发现。 |
| 42 | CLAIM-01 | P3 | RD-R36-090 | 一致 | 反证失败（主张成立），记 FP 正确；claim 的算术与链接布局我按 `head.S:68-77`/`kernel.lds:17-18` 复核，未发现反例。 |
| 43 | CLAIM-02 | P3 | RD-R36-091 | 一致 | 反证失败（主张成立），记 FP 正确；`alloc_lowest` 唯一调用点 `arch_smp.c:70-81`，调用序与文档四前提一致。 |

### 4.3 一致性读数

- 全记录一致：40/43 = 93.0%（`部分` 2 条 + `不一致` 1 条）。
- 处置合规：42/43 = 97.7%（唯一不合规是 D2-05 的参数化分支）。
- 抽样项高于 90%，按规范不因抽样判 `NOT PASS`；运行质量的 `NOT PASS` 来自 Scope 与 Trace 两问，见三、五节。
- 不一致/部分的 3 条分两类：D2-06 与 B-19 是记录级事实错误（设计角色的成员计数、事实角色的断言前提），处置方向仍然正确；D2-05 是设计角色对冻结决策的无知，处置方向本身不成立。前者指向「设计角色需要事实角色复核计数」，后者指向「设计角色需要完整的决策摘要」。

### 4.4 已重放的关键命令与读数

```
git rev-parse HEAD
  → 624fcccf158c4c24fc9043e89aaa287856234f1f
git log -6 --format='%h %ci %s'
  → 5ccd49968 2026-11-21 07:58 docs(01): R36 轨迹
    37238e568 2026-11-21 08:22 fix(01): R36 certify #1 残项
    49ed0c54a 2026-11-21 08:43 fix(01): R36 certify #2 残项
    624fcccf1 2026-11-21 09:02 fix(01): R36 certify #3 残项
    e4374af7e 2026-11-21 07:48 fix(01): R36 面板归因批一
    773e4bbc5 2026-11-21 07:51 fix(01): R36 面板归因批二
python3 tools/eval-trace-check.py <run-dir> --round R36
  → PASS 12 / WARN 25 / FAIL 0，exit 0
python3 tools/panel-index.py <run-dir>/panel/round36 --check
  → 7 个文件哈希与计数一致，exit 0
cargo test -p boot-shim --features test-all --lib -- --list
  → 146 tests, 0 benchmarks
cargo test -p minix-boot --lib -- --list
  → 185 tests, 0 benchmarks
cargo test -p minix-elf --lib -- --list
  → 27 tests, 0 benchmarks
grep -c '#\[test\]' os/libs/minix-boot/src/kernel_info.rs
  → 24；rejects 15 / accepts 3 / requiring_sources 5 / postconditions 1
grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs
  → 296: pub const BOOT_HANDOFF_V1_VERSION: u32 = 10;
grep -c 'cfg(feature = "qemu-test-kernels")' os/boot-shim/src/uefi_helpers.rs
  → 10
grep -n 'store_kernel_info(' os/kernel/src/lib.rs
  → 生产 225、347；定义 1575；其余为测试
grep -n 'static mut SHIM_TABLE\|size_of::<BootFileTable>' os/boot-shim/src/uboot_main.rs
  → 293: static mut SHIM_TABLE: BootFileTable = ...；301: size_of 断言
grep -n 'BumpRegion\|BUMP_NEXT' os/kernel-image/src/bootface_a64.rs
  → 119/128/134：AtomicU64 + BumpRegion
grep -n 'r > 0 && r < 4096 - A' doc
  → 2605: 1 命中
grep -n '下一条取指页错误\|下一个冷取指' doc
  → 「下一个冷取指」1 命中；「下一条」0
grep -n '24 个成员' doc
  → 1878: 1 命中
awk 'NR>=212 && NR<=640 && /^    (const |fn |unsafe fn )/' os/arch/src/arch/paging.rs | wc -l
  → 24
grep -rn 'build_memmap()' os/qemu-tests/test-kernels --include=*.rs
  → 可执行 2 处；另 1 处注释
grep -rl 'exit_boot_services_and_classify' os/qemu-tests/test-kernels --include=*.rs | wc -l
  → 12
grep -c '遗留 P0' <run-dir>/certify/*.md
  → 0（全部回执缺 V3.6-r2 的机器可读声明）
grep -n 'R36' rewrite-notes/coordination/review-eval/BLIND-SPOTS.md
  → 0
grep -n 'D-064' <run-dir>/loop.md <run-dir>/defect-ledger.md
  → 0（D-064 未进 R36 的冻结决策检查）
git blame -L 509,515 --date=short os/boot-shim/src/opensbi_helpers.rs
  → 289512053d 2026-11-20 已把 assert! 与 D-064 注释落码
git diff -- <run-dir>/certify/round36-fact-pass1.md <run-dir>/certify/round36-reader-pass1.md
  → 两处仅追加「本轮最终结论：CLOSED/PASS」，与正文 38/6、12/4 矛盾；未提交
```

## 五、Trace integrity 专项

### 5.1 通过项

- 93 个 ID 全部有台账行，无孤儿行；`--round R36` 的 `coverage`、`metrics`、`open-debt`、`severity` 全 PASS。
- `metrics.md:284-302` 的恒等式成立：93 = 44（①）+ 1（②）+ 16（④）+ 0（⑤）+ 0（⑥）+ 2（FP）+ 30（SUGG）。
- 严重度可复算：FINDING 62 条按面板标题为 P0 10 / P1 21 / P2 25 / P3 6；CLAIM 另有 P0×2、P1×2、FP×2；台账 93 行的严重度分布 12 P0 / 23 P1 / 25 P2 / 33 P3，与前者在「P3 含 30 条建议 + 5 条降级 FINDING + 2 条 FP」的口径下闭合。
- 面板 7 个文件的 sha256 与 `INDEX.md` 一致；`tmp/review-eval/.../panel/round36/` 与入库 `panel/round36/` 的 7 份文件逐字节相同。
- `OPEN-DEBT.md:49` 的 R36 行有 `D-104` 与「用户批准 2026-11-21」；`DECISIONS.md:187` 的 D-104 有日期、范围、状态与重开条件。就 R36 这一行，日期与判决账对得上。

### 5.2 根因 ID 范围与计数（FAIL）

`metrics.md:285` 与 `loop.md:2172` 都写 `RD-R36-001..093`，暗示 93 个连续编号。台账实际是 93 行，但最大 ID 是 `RD-R36-095`（`CLAIM-06`），且 `RD-R36-016`、`RD-R36-036` 两个编号缺失。也就是说：

- 行数 93 与「93 个根因」对得上；
- 编号范围 001..093 与台账不符；
- 缺号没有登记（不像是 `[D-REF-OK]` 那种有说明的已知空缺）。

这不是内容错误，是轨迹可复核性问题：一个按范围做区间校验的读者会认为 094/095 是越界行，一个按行数校验的读者会认为缺号是丢行。修法很简单：把范围写成 `RD-R36-001..095（93 行；016/036 为空号）`，或在台账头部登记两个空号的原因。

### 5.3 certify 三轮与事后改写（FAIL）

时间线是清楚的：

| 轮次 | 事实面基线 | 结果 | 读者面基线 | 结果 |
|---|---|---|---|---|
| #1 | `5ccd49968` | 38 CLOSED / 6 STILL-BROKEN | `5ccd49968` | 12 CLOSED / 4 STILL-BROKEN |
| #2 | `37238e568` | 43 CLOSED / 1 STILL-BROKEN | `37238e568` | 16 CLOSED / 0 |
| #3 | `49ed0c54a` + `624fcccf1` 残项复读 | 44 CLOSED / 0 | `49ed0c54a` | 16 CLOSED / 0 |

问题不在三轮，而在回执的保存方式：

1. `round36-fact-pass1.md:65` 的正文是 `CLOSED 38 / STILL-BROKEN 6`，`:73` 却被追加了「本轮最终结论：CLOSED/PASS（44/44）」。
2. `round36-reader-pass1.md:26` 的正文是 `CLOSED 12 / STILL-BROKEN 4`，`:29` 被追加了「CLOSED/PASS（16/16）」。
3. 这两处追加是工作树里的未提交修改（`git diff` 只显示这两段新增），不是原始回执的一部分。原始回执是历史证据，追加的结论是后来状态；两者放在同一文件里、没有 `## 事后补记` 这样的分隔，读者无法分辨哪段是当时读数。
4. 轨迹门只看到「CLOSED/PASS」与回执文件存在，看不到正文的 `STILL-BROKEN 6`，所以门 PASS。这是「门读末尾结论，不读正文汇总」的盲区。

正确的做法是保持 pass 文件不可变，把最终结论只放在最终回执；如果要标注历史回执已过期，应另起一节并在标题写「POST-HOC，非当时读数」。这不影响最终结论（44/44 由最终回执支撑），但影响审计链的可信性。

### 5.4 certify 新准入与最终修复提交（CONCERN）

`loop.md:2202` 定义 `certify 新准入 = 0` 的口径是「三轮 certify 的 STILL-BROKEN 均为同根修复不完整，不是新根因」。这个口径可以自洽，但账本没有回指最终两批修复：

- `49ed0c54a` 修了 5 处源码注释（4 GiB 门措辞等）；
- `624fcccf1` 修了 `kernel_info.rs` 的孤儿测试注释、未用 `USER_STACK_TOP` import、`land_slice`/`store_kernel_info` docstring，以及 `handoff_v1_read.rs`/`handoff_v1_write.rs`/`memmap_source.rs` 的 v10 后未用 import；
- 台账 93 行的「证据」列只引 `d8ce8b29c`、`e4374af7e`、`773e4bbc5`，没有一行引 `49ed0c54a` 或 `624fcccf1`；
- `metrics.md` 也没有引用这两个提交。

这不是说 `certify 新准入` 的计数错了（残项确实是同根），而是说「certify 期间发生的代码修改」在台账里没有行级落点。一个后续读者按台账重放「所有 ① 的修复证据」时，不会知道最终 HEAD 上还有两批 certify 驱动的修改。修法：给 `FACT-B-05` 或新增一行 `CERT-R36-01` 加「同根残项：源码注释/测试注释/import；commit `49ed0c54a` + `624fcccf1`；复验见最终回执残项节」。这与「certify 新准入」的根因数口径不冲突，只是把修改点落账。

### 5.5 V3.6 规则边界与遗留声明（CONCERN）

`WORKFLOW-VERSIONS.md` 的版本表把 V3.6 标为「生效（新开篇指标 `schema: V3.6`）」，而 R36 的 schema 是 `V3.4 from R35`。结果是：

- R36 的 D2 报告包含 V3.6 的 `SUBTRACTION-TABLE`（8 行），`metrics.md:301` 也写了 `减法: 提案 8 / 采纳 4`；
- 但 `eval-trace-check.py:2352-2361` 的 `_combined_ver` 取 schema 版本，V3.4 下 `v36_doc_flag` 为假，`check_subtraction` 不跑；`require_decl` 也是 `_combined_ver >= (3, 6)`，所以 certify 不要求 `- 遗留 P0: <n> / P1: <n>`；
- `grep -c '遗留 P0' certify/*.md` = 0，六份回执（含最终两份）都没有这行。

如果 V3.6 只对新开篇生效，那么 R36 的减法表是自愿产物，数字没有门校验，这本身可以接受；但 `loop.md:2202` 把「减法 8/4」当成指标结论，读者会以为它过了 V3.6 门。建议二选一：给 D01 标 `schema: V3.6 from R36` 并补遗留声明、跑减法门；或者明确写「V3.6 规则仅新开篇生效，R36 的减法表是自愿读数，不参与门」。

### 5.6 建议的逐 ID 可追溯性（CONCERN）

- 25 条编号 SUGGESTION 在台账里都有行，但 `suggestions.md` 的 R36 段只按组写摘要（`READER-A S01–S10`、`READER-B S-B-01–S10`、`READER-C S01–S05`），没有逐 ID 的位置、建议、为什么不修。台账的复验列写「`grep -n 'READER-A-S01' suggestions.md` 命中或按组登记」——我实测 `READER-A-S01`、`READER-A-S05`、`READER-B-S01`、`S-B-05`、`READER-C-S05` 全部 0 命中，只有组文本。所谓「命中」靠的是「或按组登记」这个兜底，不是可重放的逐条证据。
- 8 条 fact 未编号建议在台账与 `panel-index.py` 里都不存在；`metrics.md:284` 明确写「另有 8 条 fact 角色未编号建议，单列不参与分档」，但「单列」在哪里没有指针。`loop.md:2160` 把 `INDEX.md` 说成 `SUGGESTION 33`，实际 `INDEX.md` 的 SUGGESTION 列合计 25。这不是大缺陷，但它让「33 条建议」这个数字没有单一权威。

建议：给每条 SUGGESTION 一个 ID（`SUGG-R36-001..033`），在 `suggestions.md` 逐条落位置/建议/不修理由；`panel-index.py` 把未编号的 fact 建议也计入并在 INDEX 里显示。否则下一轮无法判断「这条建议是否已经处理」。

### 5.7 盲区账本（FAIL）

`loop.md:2228` 声称已记 BLIND-SPOTS 候选，但 `BLIND-SPOTS.md` 全文没有 R36 条目。这不是「候选写在 loop 里也算」的问题：规则要求 L6.5 的产物进盲区账本，账本是跨轮复用的；只写在当轮 loop 里，下一轮的面板不会读到。R36 至少有三条值得进账本的候选：

1. **根因射程按制品类别定义**（三轮 certify 才清完注释面）；
2. **决策摘要完整性**（D-064 未进 snippet，设计角色重提被否决的路线）；
3. **认证回执可变性**（pass1 事后追加 PASS）。

这三条在第六节以 `B-R36-01..03` 记录；本文件是只读审计，不能改 `BLIND-SPOTS.md`，主 agent 收口时应把它们追加进账本。

### 5.8 D-064 的状态与重提（FAIL，跨 Scope）

`DECISIONS.md:147` 的 D-064 状态是「已批准待实施（裁决生效，实施未开始）」，但 `git blame` 显示 `289512053d`（2026-11-20）已把两个 `debug_assert!` 改成 `assert!` 并写了「D-064」注释（`opensbi_helpers.rs:459-468`、`:511-517`）。D-064 的实施状态过期；`OPEN-DEBT.md:20` 也仍写「已批准待实施」。这本身是判决账的维护问题，但它与 D2-05 的重提叠加：一个本应显示「路线 B 已实施」的判决，被读成「还可以讨论参数化」。R36 的冻结决策检查没有列 D-064，D-104 也没有引用它。更直接的派发缺项是：`loop.md:2151-2152` 给 FACT-A/FACT-B 的材料含「决策摘要」，而 `loop.md:2153` 给 P-DESIGN-D2 的材料只有「§3/§4 + 关联代码 + `trait-audit.json` + AGENTS.md」，没有决策摘要。设计角色不是疏忽，是没拿到 D-064。

正确的处置：把 D-064 状态改成「路线 B 已实施（`289512053d`）；路线 A 仍被否决」；D2-05 拆成「0 值写一次漏洞（新，P2）」与「参数化（`dup-of: D-064`，关闭）」。这样既保留 D2-05 的新发现，也不重开用户裁决。

### 5.9 裸 ID 冲突 workaround（CONCERN）

`loop.md:2216` 主动披露：R36 的设计行使用裸 `DESIGN-D2-0x`，与 R26–R30 的历史行重名；为了让历史轮的 ② 检查通过，8 行设计行在 action 里补了「用户批准 2026-11-21（R36 代裁授权）」。这是诚实的披露，但仍是规避：这 8 行是 ① 或 SUGG，不是 ②，补一句批准日期并不对应任何具体裁决；历史轮的 ② 检查可能被 R36 的同名行满足。修法是工具层把 ledger 键改成 `(round, id)`，这已在 `loop.md:2216` 的下一版建议里，但本轮没有实现。

### 5.10 成本与 L6.5 前提（CONCERN）

- `metrics.md:302` 记 9 个 agent；实际是 7 个面板角色 + 6 次 certify 回执 = 13 次派发。如果 certify 的同一角色在同一 session 里连续跑三轮，可以争辩「2 个 certify agent」，但文件明确是三次冷启动回读，按派发次数应记 13。
- `loop.md:2227` 的三问②写「R36 面板与三轮 certify 都在同一最终基线（624fcccf1）上」。面板基线是 `b5f8ff65b`（`loop.md:2136`），certify #1/#2 分别是 `5ccd49968`/`37238e568`，最终回执正文是 `49ed0c54a` + `624fcccf1` 残项复读。面板与 certify 不可能在同一基线；正确的表述是「certify 收敛到最终 HEAD，面板在修复前基线」。这个前提错误会让 L6.5 的三问自检失去意义。
- `metrics.md` 的「token 未采集」仍是 B-36 的 OPEN 状态；6 小时的 wall time 是唯一成本读数。

### 5.11 FREEZE 未追加（CONCERN，收口待办）

`FREEZE.md` 最后一条是「R35 确认轮」，没有 R36 的 append-only 节。R36 在 `loop.md` 与 `metrics.md` 已写 `CONVERGED`，按 V3.4-m7 的 append 语义，收口应追加 R36 的冻结提交、文档 sha、未解决事项与重开条件。审计时 HEAD 是 `624fcccf1`，工作树里 loop/pass1 仍未提交；如果这是 meta 收口前的中间态，那么应在主 agent 收口时补；如果直接以当前状态结案，FREEZE 的记录就缺一轮。

## 六、BLIND-SPOT-DELTA

> 说明：`BLIND-SPOTS.md` 没有 R36 条目；以下 delta 由本报告记录，主 agent 收口时应追加进账本。

### 新增

- **B-R36-01 根因射程按「面板句子」定义，不按制品类别定义。** 证据：FACT-B-05 需要三轮 certify 才清完文档 + 源码 docstring + 测试注释 + 未用 import；最终两批修复 `49ed0c54a`/`624fcccf1` 不进台账。期望探测器：每个 ① 行带 `closure:` 五类（doc / code-comment / test / citation / anchor），删除或改名的标识符在 `os/` 与文档里同时扫旧串。
- **B-R36-02 决策摘要不完整，冻结决策检查漏项。** 证据：`tmp/.../panel/round36/decisions-snippet.md` 30 行不含 D-064；`loop.md:2173` 的冻结决策检查也不含 D-064；D2-05 重提 D-064 否决的参数化，`re-litigation: 0`。期望探测器：snippet 由 `DECISIONS.md` 按 related-code 路径生成；设计 FINDING 的提案与判决账做关键词/语义对账，冲突即要求 `dup-of` 或显式 supersede。
- **B-R36-03 认证回执可事后改写，门只读末尾结论。** 证据：pass1 正文 38/6 与 12/4，末尾追加 44/44 与 16/16；`git diff` 显示是未提交追加；轨迹门 PASS。期望探测器：certify 文件 append-only；末尾结论必须与正文汇总一致；不一致即 FAIL。
- **B-R36-04 未编号/按组登记的建议逃出索引与逐条追溯。** 证据：8 条 fact 建议无 ID；25 条编号建议只按组登记；`INDEX.md` 的 SUGGESTION 25 与 loop 的 33 不一致。期望探测器：`panel-index.py` 统计所有 SUGGESTION（含未编号），`suggestions.md` 逐 ID 落位置/建议/不修理由。

### 本轮证据可关闭或部分关闭（账本状态未改，主 agent 需按本节追加）

- **B-64（④ 修复后无冷读者复读）：部分关闭。** R36 的 reader certify 是独立冷读者，16/16 CLOSED；但 pass1 的事后追加削弱了「当时读数」的可信性。
- **B-17（非 FINDING 面板逃出台账）：部分关闭。** 6 条 CLAIM 全部有行；25 条建议有行但只按组证据；8 条 fact 建议仍无行。
- **B-49（严重度地板为零、无运行期验证）：部分改善。** R36 有代码修复（wire 常量、`SHIM_TABLE` 类型化、aarch64 bump）与 `test-sysboot`/`test-riscv64-uboot-shim` 的 QEMU 读数；但 aarch64 `test-rt-birth` 仍是红的（A2 遗留，stash 对照证明非本轮因果），不能算全绿。

### 仍 OPEN 或 DETECTED

- **B-08 可复算读数漂移**：R36 自己又抓到 6 类计数漂移（wire 版本、测试账、build_memmap、cfg 计数、land_slice、arena 合计），说明门仍未前置。
- **B-10 重复提已裁决问题**：D-064 是本轮的直接复发。
- **B-31 台账无轮次作用域**：裸 ID 冲突 workaround 是同一缺口的本轮形态。
- **B-36 成本无仪表**：token 仍未采集；agent 数还少记 4。
- **B-50 半修记整修**：FACT-B-05 三轮才闭合；台账 action 仍留旧句（B-19）。
- **B-61 收口后的未提交修复**：pass1 追加与 loop 的 R36 小节在审计时未提交。
- **B-63 同根/同角色重复计数**：93 个 ID 对约 35 个实质根因。
- **B-47 欠账源自身不收敛**：`--round R36` 有 24 个 open-debt WARN（另有 1 个 edition-trial WARN），工具没有逐行确认判决账，只做了宽松匹配。

## 七、建议变更

1. **案例 `D-064` / `DESIGN-D2-05`。目标文件 `prompt/review-rules/review-panel.md` §3/§4.3 与 `tools/decision-ref-check.py`（或新增 `tools/decision-snippet.py`）。** 期望效果：给面板的冻结决策摘要由 `DECISIONS.md` 按 `related-code.txt` 的路径自动生成，覆盖所有同子系统裁决；设计 FINDING 的提案若与某条已否决路线同形，必须在 `action` 写 `dup-of: D-0xx` 或由一条明确 supersede 的新裁决承接，否则判 `re-litigation`。回归风险：摘要变长、面板 token 上升；用一行摘要 + 路径过滤控制。替代/合并：替代当前手工挑选的 30 行 `decisions-snippet.md` 与主 agent 的手工冻结决策检查。
2. **案例 `FACT-B-05` 三轮 certify。目标文件 `prompt/review-rules/review-panel.md` §4.3/§5 与 `tools/eval-trace-check.py`。** 期望效果：每个 ① 行强制一行 `closure:`，列 `doc / code-comment / test / citation / anchor` 五类及证据；对删除或改名的标识符，门在 `os/` 与文档里同时扫旧串，命中即未闭合。回归风险：每行多 1–2 行文本；用紧凑格式控制。替代/合并：替代「冷读回执 CLOSED」这种不可拆解的整句证据，保留冷读作为最终复核而非唯一证据。
3. **案例 `round36-fact-pass1.md` / `round36-reader-pass1.md` 的事后追加。目标文件 `tools/eval-trace-check.py` 的 certify-artifact 检查与 `review-panel.md` §5。** 期望效果：certify 回执 append-only；最终结论只写进最终回执；如果确实要标注历史回执已过期，必须用 `## POST-HOC（非当时读数）` 标题，且门只认正文汇总。回归风险：历史回执的合法补注需要新格式；用标题标记即可。替代/合并：替代当前「在旧回执末尾加一句最终结论」的做法。
4. **案例 `93 个 ID / 93 个根因`。目标文件 `metrics.md` 的固定键与 `tools/eval-trace-check.py` 的 metrics 检查。** 期望效果：把 `发现总数（ID）` 与 `唯一根因数` 拆成两个数；要求跨角色同根填 `cluster` 或 `dup-of`，门校验 `唯一根因数 = ID 数 − 合并数`。回归风险：聚类有争议；由主 agent 填、meta 抽样复核，争议进 `DECISIONS.md`。替代/合并：在确认轮替代「逐 ID 立根因」的记账约定；修改轮仍可逐 ID 保留。
5. **案例 8 条 fact 未编号建议 + 25 条按组建议。目标文件 `tools/panel-index.py` 与 `review-panel.md` §5。** 期望效果：所有 SUGGESTION 都有 ID 与逐条处置；`panel-index.py` 统计未编号项并在 INDEX 显示；`suggestions.md` 逐 ID 写位置、建议、为什么不修。回归风险：ID 增多；用 `SUGG-R<N>-NNN` 命名空间，不占 FINDING 编号。替代/合并：替代「或按组登记」的兜底。
6. **案例 V3.6 减法表与遗留声明。目标文件 `WORKFLOW-VERSIONS.md` 的 V3.6 节与 `tools/eval-trace-check.py` 的版本判定。** 期望效果：二选一——D01 标 `schema: V3.6 from R36`，补 `- 遗留 P0: 0 / P1: 0` 并跑减法门；或明确写 V3.6 只对新开篇生效，R36 的减法表是自愿读数。回归风险：前者需要补一行声明，成本低；后者需要改文字，不影响门。替代/合并：把「规则生效」与「门生效」的判定统一到同一个 schema 字段。
7. **案例 `BLIND-SPOTS.md` 无 R36 增量。目标文件 `prompt/review-rules/review-evals.md` 的 L6.5 与 `tools/eval-trace-check.py`。** 期望效果：L6.5 的产物必须是 `BLIND-SPOTS.md` 的新 `## R<N>` 节；只有 loop 里的「已记候选」不算。回归风险：低。替代/合并：把盲区 delta 从 loop 的自由文本移到固定产物。
8. **案例成本记录。目标文件 `metrics.md` 的成本行与 `review-loop.md` L5。** 期望效果：记录面板角色数、certify 派发次数、每角色 wall time、token 或等价的文件字节代理；计算「每实质根因成本」与「每 P0/P1 成本」。回归风险：token 数据可能拿不到；用 prompt/response 文件大小作代理并注明。替代/合并：替代当前「agent 9 / token 未采集」的单一读数。

## 八、局限声明

**同 agent 验证 + 已重放命令。** 本次审计由与被审轮次同一模型（opencode-go-chat/deepseek-v4.1-flash）的冷启动子 agent 完成，共享模型级盲区，不能替代异构评审。下列命令在本仓当前工作树上实际执行过，输出已写入第四节；未执行的检查逐项标注。

已重放的命令与读数见 §4.4。补充说明：

- `cargo test --list` 是列表计数，不是完整跑测试；我未重跑 185 条 `minix-boot` 与 146 条 `boot-shim` 的完整套件，也未跑 QEMU。台账里的 `test-sysboot PASS`、`test-riscv64-uboot-shim 6/6`、`test-rt-birth-aarch64` 我只做了源码/脚本级核对，没有运行期复核。
- `tools/anchor-resolve.sh --check`、`tools/doc-citation-check.py`、`tools/doc-code-map.sh --check` 我重放了，读数分别是 anchor 110/110、citation `checked=29 failed=0`、code-map 54/54；`doc-structure-check`、`doc-ref-check`、`doc-style-lint`、`doc-code-density-check` 未逐条重放，按 loop 的记录接受。
- 我没有读 `minix3/` 的 C 源做全量复算（只对 CLAIM-01/02 与字段台账做了点读）；C 侧结论的独立强度低于 Rust 侧。
- 抽样 43/93 = 46%，高于 20% 要求；全记录一致 40/43 = 93.0%，高于 90%，抽样项不写 `NOT PASS`。运行质量的 `NOT PASS` 来自 Scope discipline（D-064）与 Trace integrity（5.2–5.10），不是抽样精度。
- 本次审计在 HEAD `624fcccf1`、工作树含未提交的 `loop.md` 与两份 pass1 回执的状态下完成；如果主 agent 在收口时提交或修正这些文件，本报告的 5.3、5.10、5.11 三节需要按新状态重读。本报告不把「门 PASS」写成「工作流已被独立证明」。
