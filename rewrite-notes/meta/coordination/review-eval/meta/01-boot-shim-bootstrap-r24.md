# R24 meta-review（01-boot-shim-bootstrap）

> 角色：冷启动 P-META。只读审计，唯一写入本文件。
> 审计时点：`2026-11-09`；HEAD `7e71c601a`（R24 五项裁决落账），R24 基线 `76e646d97`。
> 审计对象：`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/` 的 R24 轨迹、`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round24/` 的 7 份面板原始输出与 `round24-rb.md`。
> 方法：读工作流规范、版本链、语料、盲区账本、决策账本、R24 轨迹、面板原文、RB24、前三次 meta；用 `git status`、`git log`、`sha256sum`、`grep`、`awk`、`sed`、`python3 tools/eval-trace-check.py` 重放关键读数；另跑一次 `cargo check -p test-paging-faultloop --features fw-x86-uefi` 与一次 `cargo test -p boot-shim --features test-all --lib -- --list`。未重跑 QEMU，未重跑全量宿主测试。
> 局限：同 agent 验证 + 已重放命令；未做跨工具第二人复核；面板读者与 RB 都是同一模型。按 `review-evals.md` §9，这不等于“工作流已被独立证明”。

## 问卷

### 1. Detection：CONCERN

六个 P0 里，四个由工作流结构接住，一个由一个角色的核心主张检查接住，一个带机会性；本轮最大的两个盲区不在文档/代码层，而在“用户裁决到代码”的落地层。

- 结构接住：`FACT-A-01`（EBS 成功分支仍有 `uefi::println!`）由 P-FACT-A 的“阴性声明 + 整模块审计面”直接覆盖；`FACT-A-02`/`FACT-B-03`（§3.7 引文停留在 BOOT-SCOPE 前）由引文块逐块对账覆盖；`FACT-A-03`/`FACT-B-01`（`pt_alloc` 旧 release 语义）与 `FACT-A-04`/`FACT-B-02`（`ident` 容量残留）由合同、字段、常量对账覆盖。证据：`panel/round24/fact-a.md:7-63`、`fact-b.md:94-132`、`loop.md:1279`。
- 结构但只由一个角色接住：`CLAIM-05`（`DEVICE` 未接进 `device_window_flags()`）由 P-REFUTE 的“核心主张反证”接住；事实角色没有对“文档声称已接线的标志”逐个追消费点。证据：`refute.md:110-136`；修复后 `os/arch/src/arch/device_window.rs:250-252` 与测试 `:357-364`，`os/arch/src/arch/paging.rs:709-721` 的 `map_kernel` 段 5 也使用该函数。
- 机会性：`CLAIM-01` 第 4 点（`map_huge` 对同一 2 MiB 叶的第二次写入不会返回 `AlreadyMapped`）是反证者在追“窗口表决定范围”时顺手发现的旁支，不是任何角色被要求系统检查“三架构 `map_huge` 叶写语义”。修复后文档 `:2849` 与源码 `os/kernel/src/lib.rs:1199-1203` 的注释都与三架构实现一致；但工作流仍没有专门的“跨架构实现语义对照”探测器。D1 已按 D-024 停跑，D2 只做文档-实现差异，这个类还会再漏。
- 仍漏：两项用户裁决没有落地（见 §5 与 §8）；全轮 31 个 R24 修复文件未提交（另有 `.codex/.trae` 的 V2.2 派生文件与未跟踪文档；见 §8）；RB 二次 100% 只复读 23 条（见 §4）；B-33 的“面板期间文档未改”仍只有自述，没有面板前后 `git status`/sha 证据；B-64 的“修复后冷读者复读”仍无装置；B-63 的“G8 唯一实质新增”仍不可计算。
- 判定理由：文档/代码事实层的 P0 检测是真实的，不是纯运气；但“用户裁决未实施”和“全轮未提交”是本轮最该被看见却没被任何角色看见的两件事，因此不能给 PASS。

### 2. Precision：CONCERN

- 原始 118 条，唯一实质估计约 85–90 条。跨角色同根重复至少二十余组：`FACT-A-02 = FACT-B-03`、`FACT-A-03 = FACT-B-01 = DESIGN-01 = CLAIM-04`、`FACT-A-04 = FACT-B-02 = DESIGN-02 = READER-A-17`、`FACT-A-05 = FACT-B-07 = DESIGN-09`、`FACT-A-07 = FACT-B-05`、`FACT-A-08 = FACT-B-09 = READER-A-02 = READER-B-08 = READER-C-01`、`FACT-A-09 = FACT-B-06`、`READER-A-03 = READER-B-03 = READER-C-02`、`READER-A-05 = READER-B-09`、`READER-A-12 = READER-C-05`、`READER-A-13 = READER-C-12` 等。`loop.md:1281` 说“按同源并案、逐 ID 留行处理，未计为独立缺陷”，但台账没有 `dup-of` 字段，`metrics.md:74-75` 仍按 118 计；叙事上并案，计数上没有并案。
- 处置错两条：`CLAIM-02` 与 `CLAIM-03` 在 `refute.md:42`、`:69` 的结果是“反证失败（主张成立）”，不是缺陷；台账 `defect-ledger.md:1666` 的 `CLAIM-01..06` 行却统一记 `①`。这把 2 条“无缺陷”计进了 116 条修复。R23 的 `CLAIM-01/03` 已经出过同一形状，本轮复发。
- 误报一例：`FACT-B-21` 的第 1 小项说 `Makefile.inc` 的 `OBJS.kernel += ${UNPAGED_OBJS}` 在 82 行、文档写 83 行是错；实际 `awk 'NR==83' minix3/minix/kernel/arch/i386/Makefile.inc` 就是该行。修复按面板错误读数把文档从正确的 83 改成 82，由 RB24 的 N2 抓回，第二次回读又改回 83。台账仍把 `FACT-B-11..21` 整行记 `①`，没有标注这一小项是面板误报。
- `③` 两条：`DESIGN-13`（bootface 整段合并不做）是“显式延后的设计选择”，用 `③` 表示“不是缺陷”可以接受；`READER-B-20`（批次号噪声）是低置信配额项，台账 `③` 的理由是“图例已声明可跳过”，这是“作者接受该取舍”，不是“问题不成立”的反证。按 `review-evals.md` §4.1，`③` 应给源码、测试或文档原文的反证；这一条更像 `④/接受风险` 的边界项。两条 `③` 里一条干净，一条不干净。
- 结论：清晰误报约 1/118（低于 1%），但处置类错误 3 条、重复计数未并案、`③` 有一条边界项；Precision 不能判 PASS。

### 3. Design quality：PASS

- 设计角色给的是结构性意见，不是命名/注释级：`DESIGN-03` 指出 `KernelLayout::ident_regions()` 恒空、`map_kernel` 只为断言为空而收参数、`KernelLayout` 文档还列不存在的 `ident` 字段，形成“方法 → 参数 → 断言”的三层空通道；本轮删除，`grep -rn 'ident_regions' os --include=*.rs` 零命中，`map_kernel` 现为 7 个参数（`os/arch/src/arch/paging.rs:630-638`）。
- `DESIGN-06` 给 `MAX_BOOT_WINDOWS` 补 const 断言与带 `len/cap` 的消息；`DESIGN-08` 把 `BootWindows::push` 的静默丢弃改成 fail-fast（`os/kernel/src/lib.rs:1086-1100`）；`DESIGN-09` 把低 2 MiB 窗口收进 `#[cfg(target_arch = "x86_64")]`；`DESIGN-11` 抽公共窗口 helper；`DESIGN-12` 用 `PlacementPool` 收拢三个可互换的裸 `u64`；`DESIGN-14` 加 `BootReport.records_in_wire`；`DESIGN-15` 把 `ValidationLevel::Strict { authority }` 改成无载荷；`DESIGN-16` 改掉 `repr(C)` 的跨镜像理由。证据：`design-d2.md:47-227`，代码 `os/kernel/src/lib.rs:1080-1110`、`os/kernel-image/src/bootface_common.rs:150-175`、`os/libs/minix-boot/src/boot_storage.rs:355-385`、`os/libs/minix-boot/src/kernel_info.rs:190-200`。
- 落地计数：提出 4 / 接受 3 / 实施 3 / 登记 0（`metrics.md:78`）；`ident_regions` 删除、公共窗口 helper、`PlacementPool` 都已落码；bootface 整段合并按文件头显式延后拒绝（`DESIGN-13`），理由可复核。
- 但两项用户裁决的设计项没有按裁决实施：待裁决文件的 `DESIGN-02`（补平台描述符窗口长度）对应面板 `DESIGN-07`，现状仍是 `PlatformDescSource` 只有 `kind` 与 `phys_addr`（`os/libs/minix-boot/src/platform.rs:123-126`），`push_platform_windows` 仍只 `push_2m`（`os/kernel/src/lib.rs:1123-1130`）；待裁决文件的 `DESIGN-04`（按窗口表推导 Step 0 上限）对应面板 `DESIGN-10`，现状仍是 `assert!(support.phys_addr_limit() >= IDENTITY_MAP_END)`（`os/kernel/src/lib.rs:916-920`）。设计检测 PASS，落地见 §5。

### 4. Independence：PASS（面板与 RB 首轮）/ CONCERN（二次 100% 的解读）

- 冷读独立性成立：READER A/B/C 只读正文与读者画像，三份报告只引文档原句，没有引用 `os/` 或 `minix3/` 源码；60 条里有大量独有发现（计数、TODO 权威、§3.7 引文、`reslen`、Step 1/2 块尺寸、`check_boot_postconditions`、C 的两套页表池等）。
- RB24 首轮是有效的独立回读：抽样 80/118 = 67.8%，判真修 71、半修 8、未修 1，一致率 71/80 = 88.8%，NOT PASS；它还抓出两处本轮修复新引入的矛盾：N1（§1.2 新句“箭头不编码依赖”与图中“▲ 依赖 A”冲突）与 N2（`Makefile.inc` 从正确的 83 改成 82）。这两条都是主 agent 自评“已修”之后才被发现的，说明 RB 不是复述台账。
- 二次 100% 是“定向复核”，不是“整体一致率”：它由 13 项定向核对（B1、B2、N1–N11）加 10 条上轮真修项重放组成，23/23；没有重新抽样其余 38 条未被首轮覆盖的面板发现，也没有换一个冷读者。它可以证明“这 23 条在写报告时是真修且未被破坏”，不能证明“118 条整体 100%”。`metrics.md:80` 把它写成“11 项补修后二次 23/23 = 100%”，项数也与 13 不符（B1/B2 + N1–N11 = 13）。
- 结论：独立发现是真实的；二次 100% 可信的范围仅限于定向项与重放项，不能当作整体质量的证据。

### 5. Scope discipline：PASS（相邻范围与冻结决策）/ 裁决落地 FAIL（另列）

- `F-R14-04` 仍被正确挡在 02 篇：`follow-ups.md:49` 写明“用户批准延期至 D02（02-higher-half-kernel）循环（2026-11-09）”，并注明这是唯一获准跨篇延期项；没有把它写成当前阻塞。
- `adjacent 阻塞 = 0`、`re-litigation = 0`（`metrics.md:77-78`）；D-013..D-029 没有被重提；D-027 的窗口边界、D-028 的 `pt_alloc` 语义、D-029 的不做诊断打印都在本轮按裁决执行。范围门通过。
- 裁决落地另列 FAIL：`DECISIONS-PENDING-R24.md:9-13` 五项全部写“待实施”，但 `DESIGN-01`（删 `ident_regions`）、`DESIGN-03`（`push` fail-fast）、`DESIGN-05`（`records_in_wire`）已经实施，`DESIGN-02`（补平台窗口长度）与 `DESIGN-04`（按窗口表推导上限）没有实施；`loop.md:1287` 却写“① 116 条：全部本轮修复”，`round24-rb.md:59-69` 也把面板 `DESIGN-07`/`DESIGN-10` 判“真修”。同一批裁决在三处有三种状态，且 `DECISIONS.md` 没有 R24 的八字段判决行。
- 这不是“范围蔓延”，而是“用户裁决被降级成文档假设”：面板 `DESIGN-07` 的建议本来就是“先写清假设”，主 agent 照做了；但用户裁决明确改成“补长度”，实现仍停在面板建议，台账却记 `①`。Scope 的窄口径通过，裁决口径失败。

### 6. Teaching quality：CONCERN

- 60 条读者发现里，约 50 条是真实理解障碍（约 83%）：计数与 TODO 权威（A-02/03/04、B-03/B-08、C-01/02）、§3.7 引文与 `reslen`（A-13、C-12）、§1.2 箭头/地图（A-01、B-01）、Step 1 固定 2 MiB 与 Step 2 `kern_huge`（A-12、C-05）、接收面 `check_boot_postconditions`（C-09）、C 的两套页表池（C-17）、`head.o → unpaged_*.o` 改名（C-10）、`mpx.S`（A-16）、`pg_map` 的 `mapped_pde` 前提（A-20）。这些会让读者推错对象或找不到证据，不是“可以写得更顺”。
- 纯文风/打磨约 8–10 条（13–17%，低于 20% 的护栏）：A-05（重复险情）、A-07（五个 `.long` 的位门控）、A-08（页对齐引文）、B-20（批次号）、C-13（“不在表内”措辞）、C-14（按值/按指针）、C-19（`fast-fail` 命名）、C-20（“接线”两义）。其中 A-05、A-08、C-13、C-14、C-19 都已按读者建议改写，不是纯噪声。
- 采纳率：台账按 `①` 计 59/60（98.3%），高于 50%；但这是“④ 必修”政策强制的结果，不是质量信号。RB 首轮在 22 条抽样读者项里发现 5 条半修/未修（A-01、B-01、B-03、B-12、B-19），说明“改了字”不等于“障碍消失”；第二次定向补修后，当前文本在我抽查的样本里已经修好（A-01 图例、B-12 A/B 标签、B-19 索引、B-03 TODO 权威），但全量 60 条没有冷读者复读。
- 结论：真实障碍占多数、采纳率高；缺的是修复后的理解复读，且 RB 首轮证明过半读者项第一次并没有修干净，因此 CONCERN。

### 7. Cost：CONCERN

- 本轮 7 个面板角色、118 条原始发现、RB 两轮、6 个 QEMU 门；`metrics.md:79` 记“约 2 小时”。R23 是 110 条、约 4 小时。按墙钟计，R24 的单位发现成本明显下降；按唯一实质约 85–90 条计，也与 R23 的约 89 条相当，不是收益递减。
- 但成本仪表仍不完整：`token 未采集`（B-36 仍 OPEN）；`agent 7` 只算面板，不含 RB 两轮与 meta；118 是原始条数，去重后约 85–90；QEMU 六门是真正的运行期证据，但日志是在 RB 首轮 N10 指出缺失之后才生成的（`logs/round24-*.log` 时间 22:43–22:44，RB 首轮报告 22:48 落盘但 N10 已写“无日志”）。这不是造假，是“先声明全绿、后被要求补证据”的顺序问题。
- 结论：相对 R23 没有收益递减，检测与设计侧仍有新发现；成本测量不足以判 PASS，给 CONCERN。

### 8. Trace integrity：FAIL

- 形状层是绿的：`python3 tools/eval-trace-check.py <run> --panel-dir tmp/review-eval/.../panel/round24` 输出 `PASS 8 / WARN 0 / FAIL 0`，解析到 118 条面板发现，台账无孤儿、无缺行；`metrics.md` 的 118 = ① 116 + ③ 2 自洽；`--self-test` PASS。
- 提交层是红的：审计时 `git status --porcelain` 有 33 个已跟踪修改，其中 31 个是 R24 的修复文件（26 个 `os/` 文件、文档、台账、loop、metrics、related-code），全部未提交；另 2 个是 `.codex/.trae` 的 V2.2 派生文件，另有 `01-design-learn.md` 未跟踪；HEAD `7e71c601a` 只是“R24 五项裁决落账”，不是修复提交；`BASELINE.txt` 最后一条是 BOOT-SCOPE，没有 R24 的 `r24_fix_head`/`r24_fixed_doc_sha256`。这意味着：R24 的 116 条 ① 没有一条能从提交复现，R-1 的“commit 级回归 review”没有对象，R-2 的“轮转闸”证据只指向工作树。B-61 从“收口后的未提交修复”升级为“整轮修复未提交”。
- 台账层：`defect-ledger.md:1655` 的表头是 `| 发现 ID | 角色 | 严重度 | 类别 | 处置类 | 处置 | 复验 |`，缺 `review-evals.md` §3.3 要求的 `证据` 列，且 `角色` 不是 `来源角色`；R24 只有 13 个范围行承载 118 个 ID，`①` 的复验列是“doc 五门 PASS”“cargo 测试通过”“QEMU 六门 PASS”这类聚合句，没有逐 ID 的“旧串 → 新串”grep 或测试名。B-54/B-56 原样复发。
- 处置层：`CLAIM-01..06` 一行全记 `①`，但 `refute.md` 的 6 条主张里只有 4 条阻断（`loop.md:1281` 自己也写“4 条阻断”），`CLAIM-02/03` 是“反证失败（主张成立）”；`FACT-B-11..21` 一行全记 `①`，其中第 1 小项是面板误报并被 RB N2 反转。`metrics.md` 的 `① 116` 因此至少含 2 条非缺陷与 1 条误报子项。
- RB 层：`round24-rb.md` 有 B1、B2 与 N1–N11 共 13 项，`loop.md:1304` 只写“B1/B2 与 N1–N11 已按最小补修逐项处理”，没有逐项处置行；`metrics.md:80` 写“11 项补修后二次 23/23”，13 与 11 不一致。二次 23/23 是 13 定向 + 10 重放，不是对 118 条的独立抽样；首轮 71/80 = 88.8% 才是本轮的独立抽样读数。
- 证据层：QEMU 六份日志都在，且都含 PASS（`round24-x86-shim.log`、`round24-aarch64-shim.log`、`round24-riscv-uboot-shim.log`、`round24-riscv-bootface.log`、`round24-riscv-kernel-image.log`、`round24-aarch64-smp-aps.log`）；文档门 `/tmp/r24-{anchor,map,style,ref,struct,density}.log` 可重放（anchor 105/105、map 52/52、style error=0、ref/structure/density PASS）。宿主侧我实跑了 `cargo test -p boot-shim --features test-all --lib -- --list`（136 tests）与 `cargo check -p test-paging-faultloop --features fw-x86-uefi`（exit 0，7 条 warning），其余宿主计数没有归档日志，只有 `loop.md:1301` 的自述。
- 裁决层：`DECISIONS-PENDING-R24.md` 五项全部“待实施”；`DECISIONS.md` 没有 R24 的八字段行（最后是 D-029）。R-4 的“reviewer_verdict 与 user_decision 分列、不得覆盖 NOT PASS”在 R24 的五项上没有落地。
- 结论：形状与算术自洽，但提交、逐 ID 证据、处置类、RB 入账、裁决入账五项都不成立；Trace integrity 判 FAIL。

### 9. 本轮最值得但工作流仍看不见的一件事

**用户裁决到代码的落地状态没有核对门。**

面板发现的是“文档/代码哪里不一致”，用户裁决的是“这个问题按哪个方案改”；两者在 R24 分居 `DECISIONS-PENDING-R24.md` 与 `defect-ledger.md`，没有任何角色或工具把“裁决文本”与“代码/文档现状”对照。结果是：用户裁决 `DESIGN-02`“补平台描述符窗口长度”、`DESIGN-04`“按窗口表推导 Step 0 上限”，实现却按面板的“写清假设/改注释”落地；`loop.md` 记 `①`，RB 判“真修”，`DECISIONS-PENDING-R24.md` 仍写“待实施”。同一批裁决在三处有三种状态，而所有门都是绿的。这是本轮最值得、但工作流仍看不见的一件事；它比再增加一个面板角色更接近“裁决档流程”（V2.2 R-5）的本意。

### V2.2 R-1..R-5 落地核对

| 规则 | 声称的落地物 | R24 实际证据 | 判定 |
|---|---|---|---|
| R-1 commit 级回归 review 分级 + 当场修 + 复验终止（递归上限 2 层） | `review-evals.md` §4e、`review-loop.md` L4.6 | R24 没有任何修复提交；HEAD `7e71c601a` 只是裁决落账；31 个 R24 修复文件全在工作树；`loop.md:1292-1306` 没有逐 commit 的 diff/复验记录；自然也没有“当场修完才下一个 commit”的对象 | **FAIL（未落地）** |
| R-2 轮转闸：无证据不进下一轮；①③④ 全部已修且附证据 | `review-evals.md` §4f、`eval-trace-check.py` 的 ①/④ 复验非空检查 | 工具形状 PASS（`PASS 8 / WARN 0 / FAIL 0`），但 ① 的复验是“doc 五门 PASS”“cargo 测试通过”这类聚合句；RB 首轮仍找到 8 半修 + 1 未修；没有“旧串全篇 grep 归零”的命令与输出 | **CONCERN（门在，证据不成立）** |
| R-3 拖欠须人工批准；未经批准的 ② 按未修计 | `review-evals.md` §4g | R24 没有 ②；`OPEN-DEBT.md` 为空；`follow-ups.md:49` 的 F-R14-04 有 2026-11-09 用户批准、目标 D02、写明“唯一获准跨篇延期项” | **PASS（窄口径）** |
| R-4 判决字段 8 项，`reviewer_verdict` 与 `user_decision` 分列 | `review-evals.md` §3.5、`DECISIONS.md` 八列表 | R24 五项用户裁决没有进 `DECISIONS.md`（最后一行是 D-029）；`DECISIONS-PENDING-R24.md` 只有四列（编号/裁决/状态/实施要点），没有 `reviewer_verdict`、`scope`、`implementation_status`、`evidence`、`reopen_condition`；五项全部“待实施”且未回写 | **FAIL（未落地）** |
| R-5 裁决档流程：问题域锚 = Minix3 语义；不设轮次上限；第三方为完整共同设计者；域外即出界 | `DECISIONS-PENDING-*.md` 模板 | 模板存在，五项都有详细理由与代码约束；但文件自述“五项均按主 agent 建议裁决”“主对话正在并发实现中”，没有第三方共同设计的证据；裁决与实现状态未对账；`DESIGN-02/04` 被降级成面板建议 | **CONCERN（模板在，流程未闭环）** |

结论：V2.2 的五条里，R-3 的窄口径落地；R-1、R-4 没有落地；R-2 只有工具形状；R-5 只有模板。不能因为版本表写“已晋升生效”就认为 R24 已执行。

## 抽样复判

抽样 41/118 = 34.7%，覆盖 FACT-A、FACT-B、READER-A/B/C、REFUTE、DESIGN 全部角色，并覆盖五项用户裁决对应的面板项。判据：台账处置类与最终制品状态是否一致；文档项看“缺陷条件是否消失”，代码项看源码/测试；用户裁决项按裁决文本判，不按面板的“最小修复”判。

| ID | 台账/轮次声称 | 我的判断 | 证据 |
|---|---|---|---|
| FACT-A-01 | ① 真修 | 一致 | `os/boot-shim/src/ebs_pair.rs:207-215` 已改 `crate::raw_serial`；`ebs_boundary_audit.rs` 6 条 `#[test]` |
| FACT-A-02 | ① 真修 | 一致 | 文档 `:1959-1999` 已是 `prepare_kernel_image_v1` + `push_common_boot_windows` 形态，与 `os/kernel/src/lib.rs:431-504` 同形 |
| FACT-A-03 | ① 真修 | 一致 | `os/arch/src/arch/pt_alloc.rs:99-104` 为全构建 `assert!`；`pt_alloc_write_once.rs` 头注释写“all build profiles” |
| FACT-A-04 | ① 真修 | 一致 | `grep -rn 'ident_regions' os --include=*.rs` 零命中；`VmBootHandoff` 无 `ident` 字段 |
| FACT-A-05 | ① 真修 | 一致 | 源码低 2 MiB 两处均 `#[cfg(target_arch = "x86_64")]`（`lib.rs:495-500`、`:990-993`）；文档 `:2836-2841` 分腿写明 |
| FACT-A-08 | ① 真修 | 一致 | `grep -c '#[test]' ebs_map.rs` = 45；文档 `:3109` 45、`:3129` 45，和式 136；`--list` 136 |
| FACT-A-10 | ① 真修 | 一致 | `grep -c '#[test]' os/arch/src/x86_64/paging.rs` = 17；文档 `:3131` 写 17 |
| FACT-A-11 | ① 真修 | 一致 | 文档 `:1496` 改为“差异只落在 `PlacementPool` 与回调”，与 `bootface_common.rs:150-175` 的 9 实参同形 |
| FACT-A-12 | ① 真修 | 一致 | 文档 `:2838` 写 U-Boot 单窗口 `[SHIM_LINK_BASE, __bss_end)`；`uboot_main.rs:500-505` 单窗口 |
| FACT-B-01 | ① 真修 | 一致 | 同 FACT-A-03；文档 `:2081` 已是 `assert!`，`:2942` 写“两种构建档都生效” |
| FACT-B-03 | ① 真修 | 一致 | 同 FACT-A-02；§3.7 引文已改成当前函数体 |
| FACT-B-04 | ① 真修 | 一致 | 文档 `:2348` 末行调用含第 4 实参 `&shim_windows[..shim_window_n]`；`uefi_helpers.rs:1198-1204` 同形 |
| FACT-B-06 | ① 真修 | 一致 | 文档 `:2663` 直核序改为 `4(strict) → 2 → 3 → 1 → 4`；`bootface.rs:434`、`bootface_a64.rs:408` 在 `arch_boot` 前调 strict |
| FACT-B-09 | ① 真修 | 一致 | 同 FACT-A-08/A-10；45/136/17/6 均可 `grep -c` 复核 |
| FACT-B-14 | ① 真修 | 一致 | 文档 `:712` 写 `GDT_SIZE = 6 + CONFIG_MAX_CPUS`、当前 56 B；`archconst.h:21`、`kernel.h:8` 一致 |
| FACT-B-21 | ① 整修 | 不一致（第 1 小项误报） | 该小项称 Makefile 在 82 行；`awk 'NR==83'` 才是 `OBJS.kernel+= ${UNPAGED_OBJS}`；修复先把正确的 83 改成 82，RB N2 又改回 83；台账未标 |
| READER-A-01 | ① 真修 | 一致 | 文档 `:42` “竖线位置表示阅读顺序，箭头方向表示依赖”；`:74` 同义；RB N1 的第一版矛盾已消 |
| READER-A-05 | ① 真修 | 一致 | `grep -n '另一处险情'` 零命中；只剩 `:1792` 一次“这不是纯假设” |
| READER-A-13 | ① 真修 | 一致 | 文档 `:3267` 明确“日志字段是 `res`/`zeroed`，不是 `reslen`”；`:1725` 的读数对得上 |
| READER-A-17 | ① 真修 | 一致 | 文档 `:723` 改为两级容量，点名 `free_regions` 64 与 `deducted` 47，并写明 v9 删除 `ident` 上限 |
| READER-A-18 | ① 真修 | 一致 | 文档 `:2191` 改为“字节数变化见 §3.5；完整偏移表在 `handoff_v1.rs` 的 `envelope_offsets`，本篇不复制” |
| READER-B-01 | ① 真修 | 一致 | 文档 `:42`、`:49`、`:74` 三处箭头语义同向；`grep -n '不编码依赖'` 零命中 |
| READER-B-03 | ① 真修 | 一致 | 文档 `:3051` M5、`:3059` M13 都写“已关闭”；`:3064` 给 TODO-08/09/10 非机制项行；`§5.1 唯一权威` 句与 §5.3 不再冲突 |
| READER-B-12 | ① 真修 | 一致 | 文档 `:2446`、`:3021`、`:3057`、`:3470` 四处均为 A=`first_source_destination_overlap`、B=`first_overlapping_pair` |
| READER-B-19 | ① 真修 | 一致 | 文档 `:132` 标题改“主要编号索引”，`:143-145` 补 A–D、L0–L9、六入口；生产腿定义指向 §3.2 |
| READER-B-20 | ③ 不改 | 部分（③ 理由不是反证） | 批次号仍在；`§1.4` 图例只声明“可跳过”，没有消除目标读者的认知成本；低置信配额项，宜记 ④/接受风险而不是干净 ③ |
| READER-C-03 | ① 真修 | 一致 | 文档 `:2415` 摘要改为“余项按 Step 5 类型表落”，与 `:2394-2397` 五步表一致 |
| READER-C-04 | ① 真修 | 一致 | 文档 `:1429` 写明 riscv64 直核门只验 DTB 表址与模块定位失败，主线证据是 U-Boot shim 六路标门 |
| READER-C-09 | ① 真修 | 一致 | 文档 `:2663` 写明接收面 `arch_boot_resume_v1` 在 `validate_requiring_sources` 之后、`resume_boot_pt_alloc` 之前调 `check_boot_postconditions()` |
| READER-C-17 | ① 真修 | 一致 | 文档 `:2138` 明确 C 两套池：6 页静态池不归还，VM 侧 `pt_free`/`alloc_mem` 对应 Rust 注册槽 |
| READER-C-19 | ① 真修 | 一致 | 文档 `:2844` 已把“fast-fail 的削弱”改成“缓存兜底的削弱”，并说明窗口外更早页错误 |
| CLAIM-01 | ① 真修 | 一致 | 文档 `:2849` 已写“第二次 `map_huge` 不会返回 `AlreadyMapped`，而是重写同一叶”；源码 `lib.rs:1199-1203` 注释同义；低 2 MiB 分腿已写 |
| CLAIM-02 | ① 整修 | 处置错（主张成立，不是缺陷） | `refute.md:42` 结果“反证失败（主张成立）”；`ident_regions` 删除是设计清理，不是该主张的缺陷修复；台账一行全记 ① |
| CLAIM-03 | ① 整修 | 处置错（主张成立，不是缺陷） | `refute.md:69` 结果“反证失败（主张成立）”；台账一行全记 ① |
| CLAIM-05 | ① 真修 | 一致 | `device_window_flags()` 现返回 `kernel_read_write() | DEVICE`（`device_window.rs:250-252`）；测试 `:357-364` 断言 `DEVICE`；`lib.rs:1349` 与 `paging.rs:718` 使用该函数 |
| DESIGN-03 | ① 真修 | 一致 | `ident_regions` 零命中；`map_kernel` 7 参；VM 调用点 `vmproc_handle.rs:386-394` 已同步 |
| DESIGN-07（待裁决 DESIGN-02） | ① 整修 | 不一致（用户裁决“补长度”未实施） | `PlatformDescSource` 仍只有 `kind`/`phys_addr`（`platform.rs:123-126`）；`push_platform_windows` 仍 `push_2m`（`lib.rs:1123-1130`）；只加了文档假设与单片测试 |
| DESIGN-10（待裁决 DESIGN-04） | ① 整修 | 不一致（用户裁决“按窗口表推导”未实施） | `assert!(support.phys_addr_limit() >= IDENTITY_MAP_END)` 仍在（`lib.rs:916-920`）；只改注释与文档措辞 |
| DESIGN-14 | ① 真修 | 一致 | `BootReport.records_in_wire` 在 `boot_storage.rs:379`；UEFI 发送侧 `uefi_helpers.rs:1172` 置 true |
| DESIGN-15 | ① 真修 | 一致 | `ValidationLevel::Strict` 已无载荷（`kernel_info.rs:190-200`）；唯一构造点直接走 `Strict` |
| DESIGN-16 | ① 真修 | 一致 | `kernel_info.rs:14-21` 注释改为“repr(C) 只钉字段顺序、不是跨镜像 ABI” |

抽样统计：

- 真修/一致 35 条；不一致 6 条（`FACT-B-21`、`CLAIM-02`、`CLAIM-03`、`READER-B-20`、`DESIGN-07`、`DESIGN-10`）。
- 严格一致率 35/41 = 85.4%，低于 90% 门槛，按 `review-evals.md` §5.4 记 NOT PASS。
- 敏感性：若把 `DESIGN-07`/`DESIGN-10` 按面板自己的“最小修复”（写清假设/改注释）判，则 37/41 = 90.2%；若再把 `READER-B-20` 的 ③ 视为可接受，则 38/41 = 92.7%。但这两种口径都忽略了用户裁决文本；本轮的主失败恰恰在裁决口径，因此以 85.4% 为准。
- RB 首轮的独立抽样是 80/118 = 67.8%，71/80 = 88.8%；它与我抽到的半修/未修项一致（A-01、B-01、B-03、B-12、B-19），说明不是 meta 单方面的判断。

## VERDICT

**总体：FLAT（检测与设计 IMPROVED；轨迹与裁决落地 REGRESSED；净 FLAT）；轮次运行质量 NOT PASS。**

- 相对 R23：检测面更强（6 个 P0 条目，R23 是 2 个唯一 P0 根；16 条设计发现，R23 也是 16 条；耗时约 2 小时，R23 约 4 小时）；但 R24 的收口质量更差（31 个 R24 修复文件未提交、2 项用户裁决未实施、台账缺 `证据` 列、RB 13 项只做摘要、二次 100% 是定向复核）。因此不能判 IMPROVED；也不是全面 REGRESSED，因为检测与设计确实有新增，判 FLAT 更准确。
- 文档状态：`NOT CONVERGED`（`loop.md:1314` 自述，meta 同意）。G8 不成立：118 条原始发现、约 85–90 条唯一实质、6 个 P0 条目；即使去掉重复与误报，也不是“连续零新增”。
- 抽样一致性：35/41 = 85.4%（裁决口径；面板口径 37/41 = 90.2%，敏感性见上），低于 90%，按规则 **NOT PASS**。
- 同 agent 验证 + 已重放命令：`git status`、`git log`、`sha256sum`、`grep`、`awk`、`python3 tools/eval-trace-check.py`、`cargo test -p boot-shim --features test-all --lib -- --list`、`cargo check -p test-paging-faultloop --features fw-x86-uefi`、`/tmp/r24-*.log` 与 `logs/round24-*.log` 均已重放；未重跑 QEMU，未重跑全量宿主测试；未做跨工具复核。
- 收口前必须做（按优先级）：
  1. 提交全部 R24 修复，登记 `r24_fix_head`/`r24_fixed_doc_sha256`/`r24_fixed_doc_lines`，让 116 条 ① 的复验指向提交而不是工作树（B-61）。
  2. 实施用户裁决 `DESIGN-02`（平台窗口补长度）与 `DESIGN-04`（Step 0 按窗口表推导上限），或由用户重新裁决并落八字段行；不得以“文档已写清假设”代替（B-65）。
  3. 在 `DECISIONS.md` 为 R24 五项建八字段行，更新 `DECISIONS-PENDING-R24.md` 的状态（已实施/不实施/待实施）与证据；RB 必须读裁决文件，不只读面板（R-4/R-5）。
  4. 把 R24 台账的范围行拆成逐 ID 行（至少对 ① 给旧串→新串或测试名），补 `证据` 列；`CLAIM-02/03` 改“主张成立/无缺陷”，`FACT-B-21` 第 1 小项标误报；`READER-B-20` 改 ④ 或接受风险（B-54/B-56/B-26）。
  5. 把 RB24 的 13 项逐项写进台账（B1、B2、N1–N11），修正 `metrics.md` 的“11 项”为 13；在 `metrics.md` 保留首轮 80 条抽样的 88.8%，把二次 23/23 标成“定向复核”，不写成整体 100%（B-66）。
  6. 在提交后的 HEAD 上重跑一次独立 RB（至少 20% 随机抽样），再做 meta 收口；当前 100% 不能替代这次回读。

## BLIND-SPOT-DELTA

### 新增

- **B-65 用户裁决到代码的落地状态无核对门**（流程 / 裁决）
  - 证据：`DECISIONS-PENDING-R24.md:9-13` 五项均“待实施”；`DESIGN-01/03/05` 已实施，`DESIGN-02/04` 未实施；`loop.md:1287` 写“① 116 条全部本轮修复”；`round24-rb.md:59-69` 判 `DESIGN-07`/`DESIGN-10` 真修；`DECISIONS.md` 无 R24 八字段行。
  - 为什么没看见：面板与 RB 读的是“文档/代码是否一致”，裁决文件是另一条输入；没有角色被要求把裁决文本与实现状态对照。
  - 期望探测器：每条用户裁决在 `DECISIONS.md` 有八字段行；`implementation_status` 必须由实现者填 commit/测试证据，由 RB 或 meta 复核；`待实施` 阻断收敛（R-3/R-5 的扩展）。
- **B-66 定向复核冒充整体一致率**（度量 / 回读）
  - 证据：RB 首轮 80/118 = 67.8%、71/80 = 88.8%；二次 23/23 是 13 项定向（B1/B2/N1–N11）加 10 条重放；`metrics.md:80` 写“11 项补修后二次 23/23 = 100%”；其余 38 条面板发现没有被二次抽样。
  - 为什么没看见：`metrics.md` 只有一个“回读”字段，没有“首轮随机抽样率/二次定向复核范围”两个字段；读者会把 100% 当成整体质量。
  - 期望探测器：`metrics.md` 分列 `首轮抽样率/一致率` 与 `二次定向范围/通过数`；禁止把定向复核写成“整体一致率”。

### 复发（不是新增，但状态要改）

- **B-61 收口后的未提交修复：REGRESSED（全轮规模）**。`git status --porcelain` 33 个已跟踪修改，31 个 R24 修复文件未提交；HEAD 只有裁决文件提交；`BASELINE.txt` 无 R24 条目。比 R23 的“一处残项未提交”严重一个量级。
- **B-50 半修记整修：REGRESSED**。`DESIGN-07`/`DESIGN-10` 只做文档假设/注释，台账记 ①；`FACT-B-21` 的误报子项也记 ①。
- **B-54 范围行压成单 token：OPEN/REGRESSED**。R24 用 13 个范围行承载 118 个 ID；`①` 的复验是聚合句，没有逐 ID 证据。
- **B-56 声称修复无 diff 回指：OPEN/REGRESSED**。没有一条 ① 带 commit sha 或 `git show` hunk；复验列是“doc 五门 PASS”。
- **B-26 非 FINDING 面板豁免/主张入账：OPEN**。`CLAIM-02/03` 的“主张成立”被计入 ①，说明“无缺陷/主张成立”仍没有独立处置类。
- **B-44 判决不落账：DETECTED（未关闭）**。RB24 的 NOT PASS→补修→PASS 已写进 `loop.md:1304` 与 `metrics.md:80`，但 R24 五项用户裁决没有八字段行，`DECISIONS-PENDING-R24` 仍是旧状态。
- **B-63 G8 计数不区分同根/文风/已判：OPEN**。118 原始 / 约 85–90 唯一；`dup-of`/`origin` 仍未落码。
- **B-64 ④ 修复后无冷读者复读：OPEN**。本轮 58 条读者项按 ① 记，RB 首轮已发现 5 条半修/未修；没有第二次冷读。
- **B-33 冻结基线无机械门：OPEN**。`loop.md:1277` 只写“面板运行前后 `git status --porcelain` 无面板侧写入”，没有贴命令输出或 sha 对比。
- **B-09 门绿但事错：OPEN**。五门全绿时 RB 仍找到 B1（§3.7 引文旧体）与 B2（A/B 标签反置）；修复后才绿。
- **B-34 代码改动后文档引文无人回读：OPEN**。BOOT-SCOPE/`push`/`PlacementPool` 改动使 §3.7、§4.2、§4.5 引文过期，靠 FACT/RB 抓；没有机械回读门。
- **B-36 成本无仪表：OPEN**。token 仍未采集；`agent 7` 不含 RB 两轮与 meta。

### 关闭

- 无。R24 没有把任何 OPEN 盲区稳定关闭；`B-61`、`B-50` 反而回退。

## 给 L7 的候选

1. **C-29 裁决实施门**（案例：R24 `DESIGN-02`/`DESIGN-04`；目标：`review-evals.md` §3.5、`review-panel.md` P-META/RB、`eval-trace-check.py`）。每条用户裁决必须有八字段行；`implementation_status` 从“待实施”改“已实施”必须同时给 commit/测试/grep 证据；RB 必须读 `DECISIONS-PENDING-*` 与 `DECISIONS.md`，不能只读面板。期望效果：堵住 B-65。回归风险：每轮多一次裁决对账；只对用户裁决项执行。
2. **C-30 全轮提交门**（案例：R24 31 个 R24 修复文件未提交；目标：`review-evals.md` §4f/§5.4、`eval-trace-check.py --require-clean`）。L6/meta 入口 `git status --porcelain` 对文档/代码/轨迹必须干净；每条 ① 带 commit sha；不干净即 FAIL。期望效果：堵住 B-61。回归风险：工作中间态不能跑 meta；只在 L6 执行。
3. **C-31 二次回读口径**（案例：R24 88.8% → 100%、11 vs 13；目标：`review-evals.md` §5.4、`metrics.md` schema）。`metrics.md` 分列首轮随机抽样率/一致率与二次定向范围/通过数；禁止把定向复核写成整体 100%。期望效果：堵住 B-66。回归风险：报告不如现在好看；不改修复行为。
4. **C-32 台账逐 ID + 证据列**（案例：R24 13 行/118、缺 `证据` 列；目标：`review-evals.md` §3.3、`eval-trace-check.py`）。范围行覆盖 >1 个 ID 时必须逐 ID 给“旧串→新串”或测试名；缺 `证据` 列即 FAIL；批量行允许带 commit hash，但逐行至少给位置。期望效果：堵住 B-54/B-56。回归风险：台账变长；可用生成脚本降低手工成本。
5. **C-33 REFUTE 非阻断处置类**（案例：R24 `CLAIM-02/03`、R23 `CLAIM-01/03`；目标：`review-evals.md` §4.1、台账 schema）。新增“主张成立/无缺陷”档（或 ⑥ 的保留档），不把反证失败计入 ①/③；`metrics.md` 单列阻断主张数与非阻断主张数。期望效果：堵住 B-26。回归风险：多一个处置类；只影响反证报告。

---

## 第二次审计（2026-11-10）

> 审计对象：HEAD `162a86478`；修复链 `90428654f`（40 files）→ `d4be59d7b`（基线）→ `99dac96b4`（RB-post 收口）→ `d41261cb6`（基线指向收口）→ `162a86478`（裁决证据/计数）。文档 3477 行 / `8aaac0d3f3064d20e6a23240ef085478db8b5edfc99d66cd7d59cae6794edbe4`，与 `BASELINE.txt` 的 `r24_fixed_doc_sha256` 一致。
> 本次重点：上次的 B-65、B-61、B-54、B-56、CLAIM 处置类、二次 100% 口径、工作树干净；D-030..D-034 是否真落码；118 行台账与 metrics 是否自洽。
> 只读；未改代码/文档；同 agent 验证 + 已重放命令。

### 1. 上次问题逐项复核

| 上次项 | 本次判定 | 证据 |
|---|---|---|
| B-65 裁决落地无门 | **实例关闭；机制仍开** | `DECISIONS.md:108-112` 的 D-030..D-034 均为八字段行；`DECISIONS-PENDING-R24.md:9-13` 五项均“已实施”；D-031 的 `PlatformDescSource.len`、wire `off+4`、`push_platform_windows` 跨度与 `platform_window_uses_the_recorded_blob_length` 均存在；D-033 的 `assert_boot_windows_addressable` 与两枚测试均存在。我实跑 `cargo test -p minix-kernel --features mock --lib platform_window_uses_the_recorded_blob_length`、`...boot_windows_accept_ranges_below_the_phys_limit`、`...boot_windows_reject_ranges_above_the_phys_limit`，各 1 passed。仍开的是“机制”：没有工具/角色自动把 `implementation_status` 与代码现状对照，C-29 仍需要。 |
| B-61 未提交 | **主修复关闭；轨迹两文件仍开** | `git show --stat 90428654f` 为 40 files、+1139/−440；`git show 99dac96b4` 补跨度测试；`BASELINE.txt` 的 `r24_fix_head=99dac96b4`、`r24_fixed_doc_sha256=8aaac0d3…` 与当前文档一致。审计开始时 `git status --porcelain` 仍有 `M loop.md`、`M metrics.md`（内容是新增 RB-post 与提交链两行），另有用户侧 `.codex/.trae` 两项与未跟踪 `01-design-learn.md`；所以“工作树只剩用户侧两项”不成立，但未提交的已不是代码/文档修复，而是两行轨迹记录。 |
| B-54 范围行 | **关闭** | `defect-ledger.md:1655` 表头含 `发现 ID | 角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验`；R24 段解析到 118 条逐 ID 行，没有范围行；`eval-trace-check.py` 覆盖 PASS。 |
| B-56 逐 ID 证据 | **仍开（收窄）** | 118 行虽逐 ID，但 `处置` 列几乎全是“按面板最小修复落地：文档改写/代码接线/测试补齐”，`证据` 列只给“panel/round24/*.md:行；修复见 loop.md §4”，`复验` 列只给“doc 五门 PASS；宿主/QEMU 读数见 loop.md §5”。没有逐 ID 的“旧串→新串”、commit hunk 或点名测试；B-32 的“处置列被大桶压缩”原样存在。 |
| CLAIM 处置类 | **关闭** | `CLAIM-02`、`CLAIM-03` 在台账为 ③（`defect-ledger.md:1754-1755`），理由“反证/复核判定不成立，不计缺陷”；`metrics.md:75` 为 ① 114 / ③ 4，与台账逐 ID 聚合一致。 |
| 二次 100% 口径 | **关闭（口径已澄清）；新开半分制 concern** | `metrics.md:80` 明写“二次定向回读 23/23=100% PASS（13 定向 + 10 重放；不是 118 条整体抽样）”，并补了 RB-post 29 条 26/29=89.7%。但 RB-post 第二次 13/14=92.9% PASS 是把两项部分通过按 0.5 计；只按完全通过是 12/14=85.7%，低于 90%。这不是代码缺陷，是评分口径变化，见 B-67。 |
| 工作树干净 | **未干净** | 审计开始时 `git status --porcelain`：`M .codex/skills/review-loop/SKILL.md`、`M .trae/skills/review-loop/SKILL.md`、`M loop.md`、`M metrics.md`、`?? rewrite-notes/01-stage-kernel/01-design-learn.md`。meta 文件审计开始时已跟踪且干净（`90428654f` 已含第一版），本次写入后为 `M`，属于本次审计输出，不计入 R24 残项。 |

### 2. 九项问卷（第二次）

1. **Detection：PASS。** 六个 P0 均有当前源码/测试支撑：EBS 后 `uefi::` 已改 `raw_serial`（`ebs_pair.rs:207-216`，审计 6 条）；§3.7 引文与 `lib.rs:431-501` 同形；`pt_alloc` 全构建 `assert!`；`ident` 已删；`DEVICE` 接进 `device_window_flags()` 并有测试；`map_huge` 重叠语义与三架构源码一致。D-031/D-033 两项用户裁决也由我实跑测试确认。剩余盲区是轨迹层的逐 ID 证据与 loop §3 计数，不是检测能力。
2. **Precision：CONCERN。** 118 原始 / 约 85–90 唯一；无 `dup-of`，同根重复仍在。`FACT-B-21` 的第 1 小项是面板误报（当前文档 83 正确，`awk 'NR==83'` 命中），但台账整行仍记 ①，没有标出该子项；`READER-B-20` 的 ③ 是“作者接受取舍”而非反证，仍属 ③/④ 边界。`CLAIM-02/03` 改 ③ 后，清晰误报率低，但“③=误报/主张成立”混类的问题仍在。
3. **Design quality：PASS。** D-030..D-034 都落码：删 `ident_regions`、`PlatformDescSource.len` + wire `off+4` + 跨度建窗、`push` fail-fast、`assert_boot_windows_addressable` 逐窗上限、`records_in_wire`；另有 `PlacementPool`、`Strict` 无载荷、`repr(C)` 注释。不是命名/注释级；测试覆盖 D-031/D-033 的正反两向。
4. **Independence：PASS。** RB-post 是提交后随机 29 条，首轮 26/29=89.7% NOT PASS，唯一半修正是 DESIGN-07（平台跨度测试缺失）；补测试后二次 13/14=92.9% PASS（8 条重放）。它抓住了代码真落但测试未落这类残差，不是复述台账。二次 13/14 含 2 项轨迹部分通过并按 0.5 计，严格完全通过是 12/14=85.7%；这削弱了“92.9%”作为整体一致率的可比性，但独立性本身成立。
5. **Scope discipline：PASS。** `F-R14-04` 仍在 `follow-ups.md:49`，用户批准 2026-11-09 延至 D02，写明唯一跨篇延期项；`adjacent 阻塞=0`、`re-litigation=0`；R24 无 ②；D-030..D-034 已实施，没有把裁决项转成相邻项。
6. **Teaching quality：CONCERN。** 60 条读者项里 59 条记 ①（98.3%），抽查的计数、TODO 权威、箭头图例、`reslen`、`mapped_pde`、`head.o` 改名、`mpx.S`、两套页表池等都在当前文档修好；真实障碍约 50 条，纯文风/打磨约 8–10 条（13–17%）。但 B-64 仍未关闭：没有第二个冷读者只读改动节；“修了字”与“理解恢复”仍是两件事。RB 首轮曾在 22 条读者抽样里发现 5 条半修/未修，第二次定向补修后才通过。
7. **Cost：CONCERN。** 7 个面板角色、118 条、RB 两轮加 RB-post、6 个 QEMU 门、约 2 小时；R23 是 110 条/约 4 小时，单位发现成本下降，不是收益递减。但 token 未采集（B-36），`agent 7` 不含 RB/meta；118 是原始条数，唯一实质约 85–90；修复提交 40 files 的一次性成本没有单独记。
8. **Trace integrity：FAIL。** 形状层 PASS：118 条逐 ID、`证据` 列、metrics 118/114/4、trace check `PASS 8/0/0`、RB24 13 项逐项表、D-030..D-034 八字段。内容层有三处硬伤：其一，`loop.md` 的 R24 §3 仍写“① 116 条 / ③ 2 条”（`:1288-1289`），同文件 §6 与 `metrics.md` 写“① 114 / ③ 4”（`:1333`、`metrics.md:75`），这是 B-55 的复发；其二，`loop.md`/`metrics.md` 两行 RB-post 记录未提交；其三，118 行的处置/证据是同一句模板，B-56/B-32 仍开。工具只校验 metrics 与台账，不校验 loop §3，所以“trace check PASS”不能覆盖这三处。
9. **本轮最值得但工作流仍看不见的一件事：逐 ID 行只有“行数”，没有“内容”。** 118 行解决了 B-54 的形状问题，但每行的处置仍是“按面板最小修复落地”，读者无法从台账判断某一条到底改了哪一句、哪一个测试、哪一个 commit；这正是 B-56/B-32 的合并形态。比再加一个面板角色更值得做的是：台账行强制带 `old → new` 或测试名/commit hunk，或者由 `loop.md` §4 的修复清单自动生成 118 行的证据列。

### 3. V2.2 R-1..R-5 落地状态

| 规则 | 上次 | 本次 | 依据 |
|---|---|---|---|
| R-1 commit 级回归 review + 当场修 + 递归上限 2 层 | FAIL | **CONCERN** | 已有修复链 `90428654f`→`99dac96b4`，RB-post 在提交后回读；但 `90428654f` 是一个 40 files 的大提交，没有按“每个逻辑 commit 后派小型 review”逐段记录；`loop.md` §4 也没有逐 commit 的 diff/回归。R-1 的“有提交可审”成立，“逐逻辑 commit”仍未完全落地。 |
| R-2 轮转闸：①③④ 全部已修且附证据 | CONCERN | **CONCERN** | 118 行逐 ID 与 `证据` 列已落；RB-post 首轮仍抓到 DESIGN-07 半修，说明“无证据不进下一轮”在代码层有效；但证据列仍是聚合句，没有“旧串 grep 归零”/测试名，门的形状大于内容。 |
| R-3 拖欠须人工批准 | PASS | **PASS** | R24 无 ②；F-R14-04 有用户批准、目标 D02；`OPEN-DEBT.md` 为空。 |
| R-4 判决字段 8 项 | FAIL | **PASS** | D-030..D-034 八字段行（`awk -F'|'` 每行 8 列）；`DECISIONS-PENDING-R24.md` 五项均“已实施”并附证据；`reviewer_verdict` 与 `user_decision` 分列。唯一小缺口：PENDING 的 DESIGN-02 证据仍只列 `with_len`/wire/`dtb_total_size`，没有点名 `platform_window_uses_the_recorded_blob_length`；`DECISIONS.md` D-031 已点名。 |
| R-5 裁决档流程 | CONCERN | **CONCERN** | 模板、详细理由、代码约束、实施状态都在；但“第三方为完整共同设计者”仍没有证据（文件自述“按主 agent 建议裁决”）；裁决与实现的对照现在一致，流程是否闭环仍靠人。 |

### 4. 抽样复判（第二次）

抽样 30/118 = 25.4%，覆盖全部角色、6 个 P0、两项用户裁决（D-031/D-033）与 `CLAIM-02/03`。判据：台账处置类与当前制品是否一致；代码项看源码与测试；用户裁决项按裁决文本判。

| ID | 台账 | 我的判断 | 证据 |
|---|---|---|---|
| FACT-A-01 | ① | 真修 | `ebs_pair.rs:207-216` 为 `crate::raw_serial`；`ebs_boundary_audit.rs` 6 条 |
| FACT-A-02 | ① | 真修 | 文档 `:1949-1995` 四参 `enter_kernel_image_v1` + `push_common_boot_windows` + arena `checked_add`；与 `lib.rs:431-501` 同形 |
| FACT-A-03 | ① | 真修 | `pt_alloc.rs:100-101` 全构建 `assert!`；文档 `:2081` 无 `debug_assert!`/`if claimed` |
| FACT-A-04 | ① | 真修 | 文档 `:723/:1594/:2783` 两级容量 + v9 删除；`boot.rs` 无 `ident` 字段 |
| FACT-A-05 | ① | 真修 | `lib.rs:489-495`、`:970-975` 低 2 MiB 均 `#[cfg(target_arch = "x86_64")]`；文档 `:2838` 分腿 |
| FACT-A-06 | ① | 真修 | 文档 `:2229`、`:2258`、`:2368` 改为“EBS 后提交分类计数/复核”；`ebs_pair.rs:201-216` 同义 |
| FACT-A-08 | ① | 真修 | `grep -c '#[test]' ebs_map.rs`=45；文档 `:3107` 45、`:3127` 加式 136；boot-shim `--list` 136 |
| FACT-A-13 | ① | 真修 | 文档 `:3051` M7、`:3129`、`:3211` 登记 `pt_alloc_concurrent.rs` 1 条；该文件 `:23-45` 有 barrier 测试 |
| FACT-B-14 | ① | 真修 | 文档 `:712` 写 `GDT_SIZE = 6 + CONFIG_MAX_CPUS`、当前 56 B；`archconst.h:21`、`kernel.h:8` 一致 |
| FACT-B-21 | ① | 部分 | 当前文档 `:282` 的 83 正确，`awk 'NR==83'` 命中；但第 1 小项是面板误报，台账整行 ① 未标该子项（B-32/B-56 残留） |
| READER-A-01 | ① | 真修 | 文档 `:42` 图例“竖向位置=读序、箭头=依赖”，`:74` 同义；`grep '不编码依赖'` 零命中 |
| READER-A-02 | ① | 真修 | 同 FACT-A-08；`136 passed`、45、加式 136 三处一致 |
| READER-A-11 | ① | 真修 | 文档 `:2358` 补“按时间读是 2 的读/预订 → 1 → 2 的拷贝/模块 → 3 → … → 9” |
| READER-B-03 | ① | 真修 | 文档 `:3049` M5、`:3057` M13 与 §5.3 一致；`:3062` 非机制项行 |
| READER-B-12 | ① | 真修 | 文档 `:2446/:3019/:3055/:3468` 四处 A=源×目标、B=目标 LMA 间 |
| READER-B-20 | ③ | 部分 | 批次号仍在；`§1.4` 图例只声明“可跳过”，这是接受风险，不是反证；低置信配额项 |
| READER-C-09 | ① | 真修 | 文档 `:2661` 写接收面 `arch_boot_resume_v1` 在 `validate_requiring_sources` 后、`resume_boot_pt_alloc` 前调 `check_boot_postconditions()` |
| READER-C-11 | ① | 真修 | 文档 `:146` 编号图例改为“删除 `BootShim` trait（唯一实现是 U-Boot 腿）”，与 `:1440` 一致 |
| READER-C-19 | ① | 真修 | 文档 `:2842` “缓存兜底的削弱”，并说明窗口外更早页错误 |
| CLAIM-01 | ① | 真修 | 文档 `:2849` 写“第二次 `map_huge` 不会返回 `AlreadyMapped`，而是重写同一叶”；`lib.rs:1199-1203` 注释同义；低 2 MiB 分腿已写 |
| CLAIM-02 | ③ | 正确 | `refute.md:31` 主张成立；台账 `:1754` ③ 正确 |
| CLAIM-03 | ③ | 正确 | `refute.md:57` 主张成立；台账 `:1755` ③ 正确 |
| CLAIM-05 | ① | 真修 | `device_window.rs:250-252` 返回 `kernel_read_write() | DEVICE`；测试 `:357-364` 断言 `DEVICE`；`lib.rs:1349`、`paging.rs:718` 使用 |
| DESIGN-03 | ① | 真修 | `grep -rn 'ident_regions' os --include=*.rs` 零命中；`map_kernel` 7 参（`paging.rs:630-638`） |
| DESIGN-07 / D-031 | ① | 真修 | `PlatformDescSource.len/with_len/has_len`（`platform.rs:123-168`）；wire `off+4`（`handoff_v1_write.rs:251-256`、`handoff_v1_read.rs:312-317`）；`push_platform_windows` 跨度（`lib.rs:1118-1136`）；`platform_window_uses_the_recorded_blob_length` 实跑 1 passed |
| DESIGN-10 / D-033 | ① | 真修 | `assert_boot_windows_addressable`（`lib.rs:1186-1193`），两入口 `:496/:975` 调用；两枚上限测试实跑各 1 passed；旧全局断言已移除 |
| DESIGN-13 | ③ | 正确 | `bootface_a64.rs:36-43` 文件头显式延后；文档 `:1494` 只承诺入口壳各一份；台账 `:1771` ③ |
| DESIGN-14 | ① | 真修 | `boot_storage.rs:379` 的 `records_in_wire`；`uefi_helpers.rs:1184` 置 true；文档 `:1734` |
| DESIGN-15 | ① | 真修 | `kernel_info.rs:190-200` 的 `Strict` 无载荷，唯一构造点直接走 `Strict` |
| DESIGN-16 | ① | 真修 | `kernel_info.rs:14-21` 注释改为“repr(C) 只钉字段顺序、不是跨镜像 ABI” |

统计：真修/正确 28 条；部分 2 条（`FACT-B-21` 第 1 小项误报残留、`READER-B-20` 的 ③ 边界）。严格一致率 28/30 = **93.3%**，达到 90% 门槛；若把两条部分按 0.5 计为 29/30 = 96.7%，若把两条都按不一致计为 28/30。抽样本身判 PASS；运行质量的 NOT PASS 来自下面的轨迹层，不来自抽样。

### 5. 最终 VERDICT

**IMPROVED（内容、裁决、检测相对第一次审计与 R23 有实质进步）/ 运行质量 NOT PASS（仅轨迹收口）。**

- 内容面：六个 P0 全部有当前源码/测试支撑；D-030..D-034 五项用户裁决全部落码，D-031/D-033 的测试我实跑通过；`cargo test -p minix-kernel --features mock --lib` 为 865 passed + 8 ignored；boot-shim `--list` 为 136；QEMU 六份日志均含 PASS。抽样 28/30 = 93.3%，PASS。
- 运行质量 NOT PASS 的原因不是文档/代码，而是三处轨迹硬伤：其一，`loop.md` R24 §3 仍写 ① 116 / ③ 2，与同文件 §6 和 `metrics.md` 的 ① 114 / ③ 4 矛盾（B-55 复发，且该矛盾已在 HEAD 提交里）；其二，`loop.md`/`metrics.md` 的 RB-post 两行仍未提交（B-61 轨迹残项；审计开始时工作树并非“只剩用户侧两项”）；其三，118 行的处置/证据仍是同一句模板，B-56/B-32 未关。另有 B-67：RB-post 二次 92.9% 依赖部分通过按 0.5 计，严格完全通过是 12/14=85.7%。
- 收口条件（做完即可把运行质量改为 PASS）：把 `loop.md` §3 的“① 116 / ③ 2”改为“① 114 / ③ 4”，并在 §3 列出 `CLAIM-02/03` 两条主张成立；提交 `loop.md` 与 `metrics.md` 的两行审计记录；在 `DECISIONS-PENDING-R24.md` 的 DESIGN-02 证据补 `platform_window_uses_the_recorded_blob_length`；在台账逐 ID 行的 `证据`/`复验` 列至少给一条该 ID 的旧串→新串或测试名。
- 文档仍 `NOT CONVERGED`（`loop.md:1337` 自述；G8 连续零新增未满足）。本轮可以收口为“轮次”，不能宣告文档收敛。
- 局限：同 agent 验证 + 已重放命令；未重跑 QEMU，只读六份日志；未重跑 types/boot/vm/arch 全量，只实跑 minix-kernel 865+8、boot-shim 136 与 D-031/D-033 定向测试；未做跨工具复核。

### 6. BLIND-SPOT-DELTA（第二次）

**关闭/降级：**

- **B-54 范围行压成单 token：CLOSED（形状）**。118 行逐 ID、`证据` 列、trace check 覆盖 PASS。
- **B-65 裁决落地无门：实例 CLOSED，机制仍 OPEN**。D-030..D-034 八字段 + 五项已实施 + D-031/D-033 测试；但没有自动门把 `implementation_status` 与代码对照，C-29 仍成立。
- **B-66 定向复核冒充整体一致率：CLOSED（100% 口径已澄清）**。`metrics.md:80` 明写“不是 118 条整体抽样”，并补 RB-post 29 条 89.7%。
- **B-44 判决不落账：CLOSED**。RB24 13 项逐项表、RB-post 29 条与二次 13/14、D-030..D-034 均已进 run 轨迹/决策账。

**仍开/部分开：**

- **B-61 未提交：收窄为“轨迹两文件”**。代码/文档修复已提交（`90428654f`、`99dac96b4`），但 `loop.md`、`metrics.md` 两行审计记录未提交；审计开始时 `git status` 并非只剩用户侧两项。B-61 不能记 CLOSED，记 **DETECTED（残项）**。
- **B-56 声称修复无 diff 回指：OPEN**。118 行的 `证据` 列是“panel 行号；修复见 loop §4”，`复验` 是“doc 五门/QEMU”，没有逐 ID 的 commit hunk 或旧串 grep。
- **B-32 处置列被大桶压缩：OPEN**。118 行有 118 个 ID，但处置文本几乎相同；B-56 的合并形态。
- **B-55 分档指标不可复算：REGRESSED（复发）**。HEAD 的 `loop.md:1288-1289` 为 ① 116 / ③ 2，`:1333` 与 `metrics.md:75` 为 ① 114 / ③ 4；工具只校验 metrics 与台账，未拦住 loop §3。
- **B-31 台账无轮次作用域：OPEN**。R24 行仍用 `FACT-A-01` 等短 ID，没有 `R24/` 前缀；同文件其他轮次有同名 ID，靠 `## R24` 段隔离。
- **B-64 ④ 修复后无冷读者复读：OPEN**。59 条读者 ① 没有第二读者复读；RB-post 只做抽样回读，不是理解复读。
- **B-33 冻结基线无机械门：OPEN**。`loop.md:1278` 只写“面板运行前后 git status 无面板侧写入”，没有贴输出或 sha 对比。
- **B-09 门绿但事错：OPEN**。五门全绿时 RB 仍找到 B1；本轮 trace check 绿时 loop §3 仍与 metrics 矛盾。
- **B-34 代码改动后文档引文无人回读：OPEN**。D-031/D-033/`push` 改动后的引文回读仍靠 FACT/RB，没有机械清单。
- **B-36 成本无仪表：OPEN**。token 仍未采集；`agent 7` 不含 RB/meta。
- **B-50 半修记整修：DETECTED（未关）**。DESIGN-07 已由跨度测试补成真修；但 FACT-B-21 的误报子项仍按 ① 整行记，逐 ID 证据仍是模板。
- **B-60 抽样驱动修复：OPEN**。RB-post 抽到 DESIGN-07 后才补跨度测试；这轮是“抽样驱动”的正例，但机制未变。
- **B-62 低置信配额噪声：OPEN**。`READER-B-20` 仍是配额项 ③；③ 率 1/4 = 25%，未超过 50%，继续观察。
- **B-63 G8 计数不区分同根/文风/已判：OPEN**。118 原始 / 约 85–90 唯一；`dup-of`/`origin` 仍未落码。

**新增：**

- **B-67 二审半分制**（度量 / 回读）：RB-post 第二次 13/14 = 92.9% PASS 把两项部分通过按 0.5 计；只按完全通过是 12/14 = 85.7%，低于 90%。案例：RB-post 第 5、6 项（PENDING 证据、工作树干净）都是轨迹项，不是代码项。期望探测器：`metrics.md` 分列“完全通过/部分通过/未通过”，二次复核不得用半分制跨越 90% 门槛；若用半分制，必须同时给出严格读数。
- **B-68 逐 ID 证据同文**（记账 / 修复质量）：118 行逐 ID 解决了 B-54 的形状，但 `处置`/`证据` 列几乎逐字相同，无法回答“这条改了哪一句/哪个测试”。这不是新机制，是 B-56/B-32 的合并形态；给新编号是为了让 L7 不再把它当“已关闭”。期望探测器：每行至少一条 `old → new` 或测试名/commit hunk；`eval-trace-check.py` 对“同文行占比 > 50%”报 WARN/FAIL。

### 7. 给 L7 的候选（第二次，仅保留仍成立）

1. **C-29 裁决实施门**（继续）：案例 D-031/D-033；目标 `review-evals.md` §3.5、`review-panel.md` P-META/RB、`eval-trace-check.py`。每条用户裁决必须有八字段行；`implementation_status` 改“已实施”必须同时给 commit/测试/grep；RB 必须读 `DECISIONS-PENDING-*`，不只读面板。期望：B-65 机制关闭。风险：每轮多一次对账；只对用户裁决项执行。
2. **C-30 全轮提交门**（继续，缩小案例）：案例 `loop.md`/`metrics.md` 两行未提交；目标 `review-evals.md` §4f/§5.4、`eval-trace-check.py --require-clean`。L6/meta 入口对文档/代码/轨迹做 `git status` 硬门；不干净即 FAIL。期望：B-61 残项关闭。风险：工作中间态不能跑 meta；只在 L6 执行。
3. **C-31 二次回读口径**（继续，改为半分制案例）：案例 RB-post 13/14=92.9% vs 严格 12/14=85.7%；目标 `review-evals.md` §5.4、`metrics.md` schema。分列“完全通过/部分通过/未通过”，二次复核不得用半分制跨越 90%；首轮随机抽样率与二次定向范围分开。期望：B-67 关闭。风险：报告不如现在好看；不改修复行为。
4. **C-32 台账逐 ID + 证据列**（继续，改为内容层）：案例 118 行同文；目标 `review-evals.md` §3.3、`eval-trace-check.py`。每行至少一条 `old → new` 或测试名/commit hunk；同文行占比 >50% 报 WARN/FAIL。期望：B-56/B-32/B-68 关闭。风险：台账变长；可用生成脚本。
5. **C-34 三源计数门**（新增，替代 C-33 的位置）：案例 `loop.md` §3 116/2 vs §6/metrics 114/4；目标 `eval-trace-check.py`、`metrics.md` schema。metrics、loop §3、loop §6 与台账逐 ID 聚合必须同数；`loop.md` §3 不参与自动生成就禁止写分档数。期望：B-55 复发关闭。风险：loop §3 的叙述性文字要改成引用 metrics；不改内容。

C-33（REFUTE 非阻断处置类）本轮已由 `CLAIM-02/03` ③ 部分满足；保留为低优先，不列入前五。

---

## 第三次审计（2026-11-10，收口复核）

> 审计对象：HEAD `1b35706e2`；文档 3477 行 / `8aaac0d3f3064d20e6a23240ef085478db8b5edfc99d66cd7d59cae6794edbe4`；工作树只剩用户侧 `.codex/.trae` 两项与未跟踪 `01-design-learn.md`。
> 本次只核对第二次审计的收口条件，不重做文档评审。只读；同 agent 验证 + 已重放命令。

### 1. 收口条件逐项复核

| 条件 | 结果 | 证据 |
|---|---|---|
| `loop.md` §3 与 §6/metrics 同数 | **PASS** | `loop.md:1288-1289` 为 ① 114 / ③ 4，并列出 `CLAIM-02/03`“反证失败、主张成立”；`loop.md:1333` 与 `metrics.md:75` 同为 ① 114 / ② 0 / ③ 4 / ④ 0 / ⑤ 0。三处同数。 |
| 工作树干净 | **PASS** | `git status --porcelain` 只剩 `M .codex/skills/review-loop/SKILL.md`、`M .trae/skills/review-loop/SKILL.md`、`?? rewrite-notes/01-stage-kernel/01-design-learn.md`；`loop.md`/`metrics.md` 已随 `1b35706e2` 提交，meta 文件本身也已在该提交内且工作树干净。 |
| `DECISIONS-PENDING-R24.md` DESIGN-02 证据 | **PASS** | `DECISIONS-PENDING-R24.md:10` 已列 `platform_window_uses_the_recorded_blob_length`；D-031 的八字段行也列同一测试；我重放该测试为 `1 passed; 0 failed`。 |
| 118 行逐 ID 的 `证据` 列 | **PASS（结构性）** | 表头 8 列含 `发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验`；R24 段解析到 118 行；118 行都有 `panel/round24/*.md:<行>；问题：<具体标题>`，118 个 `问题：` 串中 85 个不同（33 个是跨角色同根重复），不再是纯模板。 |
| `CLAIM-02/03`/`READER-B-20`/`DESIGN-13` 为 ③ | **PASS** | 台账 `:1754-1755`、`:1732`、`:1771` 均为 ③；`metrics.md:75` 的 ③ 4 与逐 ID 聚合一致。 |
| trace check | **PASS** | `python3 tools/eval-trace-check.py ...` 输出 `PASS 8 / WARN 0 / FAIL 0`；覆盖、无孤儿、metrics 同数均绿。 |
| 文档与基线 | **PASS** | 3477 行 / `8aaac0d3…`，与 `BASELINE.txt` 的 `r24_fixed_doc_sha256` 一致；`r24_fix_head=99dac96b4`，HEAD 是后续的 `162a86478`→`1b35706e2` 文档/轨迹提交。 |

### 2. B-56 是否阻断轮次收口

**不阻断。**

- B-56 的严格诉求是“每条 ① 有逐 ID 的旧串→新串或 commit hunk”。当前 `证据` 列给的是“panel 行号 + 该 ID 的具体问题标题”，`处置`/`复验` 仍是“按面板最小修复落地（文档/代码/测试）/ doc 五门 PASS；测试/QEMU 见 loop §5”。它已经能回答“这条是什么问题、在哪一段面板里”，但不能回答“这条具体改了哪一句、哪个 commit hunk”。
- 这不改变任何处置的真伪：修复提交 `90428654f`/`99dac96b4` 存在，D-031/D-033 测试我实跑通过，RB-post 随机 29 条 26/29=89.7%、补测试后 13/14 定向 92.9%，本次 meta 抽样 28/30=93.3%。B-56 是**证据粒度**问题，不是修复内容问题。
- 因此 B-56 保留为 OPEN（L7 的 C-32），不阻断 R24 轮次收口；若下一轮要求“台账可直接当审计证据”，再按 C-32 升级。
- B-67（RB-post 二审半分制）同样是报告口径问题：首轮随机 89.7% 与本 meta 的 93.3% 是主读数，92.9% 只能标“定向复核（部分通过按 0.5 计）”。它不影响代码/文档收口，保留为 L7 的 C-31。
- B-61、B-55、B-54、B-66（100% 口径）、B-44 已关闭；B-65 的 R24 实例关闭、机制仍开；B-31/B-64/B-33/B-09/B-34/B-36/B-50/B-60/B-62/B-63 仍是开放项，均不阻断本次轮次收口。

### 3. 最终 VERDICT

**IMPROVED / 运行质量 PASS。文档仍 `NOT CONVERGED`（G8 连续零新增未满足），本轮只作为轮次收口。**

- 相对第一次审计：第一次的 B-65 实例、B-61 主修复、B-54、CLAIM-02/03 处置、二次 100% 口径都已关闭；D-030..D-034 五项裁决全部落码，D-031/D-033 测试实跑通过；118 行逐 ID 与 `证据` 列、三源同数、trace check PASS、工作树干净。
- 相对 R23：R24 的检测面（6 个 P0 条目）、设计面（16 条，D-030..D-034 实施）与可复跑证据更强；R23 的最终一致率 90.0%，R24 本次抽样 28/30=93.3%，且修复已提交。判 IMPROVED。
- 抽样一致性：**28/30 = 93.3%**（30/118 = 25.4%，覆盖 6 个 P0、D-031/D-033、CLAIM-02/03）。本次只改了轨迹与证据列，文档/代码未变，第二次审计的 30 条样本仍成立；我重放了其中受影响的计数、PENDING、台账、trace 项。严格一致 28 条，部分 2 条（FACT-B-21 第 1 小项误报残留、READER-B-20 的 ③ 边界），没有未修或新错。
- 保留项：B-56/B-32/B-68（逐 ID 证据是“finding-specific”而非“fix-specific”）、B-67（二审半分制）、B-31/B-64/B-33/B-09/B-34/B-36/B-50/B-60/B-62/B-63。它们不阻断轮次收口，进 L7 候选。
- 局限：同 agent 验证 + 已重放命令；未重跑 QEMU（只读六份 PASS 日志）；未重跑 types/boot/vm/arch 全量；minix-kernel 865+8、boot-shim 136、D-031/D-033 定向测试已实跑；未做跨工具复核。

### 4. BLIND-SPOT-DELTA（第三次）

- **B-61 收口后的未提交修复：CLOSED**。`loop.md`/`metrics.md` 已随 `1b35706e2` 提交；工作树只剩用户侧 `.codex/.trae` 与未跟踪 `01-design-learn.md`。
- **B-55 分档指标不可复算：CLOSED**。`loop.md:1288-1289`、`loop.md:1333`、`metrics.md:75` 三处同为 ① 114 / ③ 4；trace check PASS。
- **B-54 范围行：CLOSED**（118 行逐 ID，8 列表头）。
- **B-66 定向复核冒充整体一致率：CLOSED**（100% 已标注为定向复核）。
- **B-44 判决不落账：CLOSED**（RB24 13 项、RB-post 29 条与二次 13/14、D-030..D-034 均已入轨迹/决策账）。
- **B-65 裁决落地无门：实例 CLOSED，机制 OPEN**（C-29 仍成立）。
- **B-56 声称修复无 diff 回指：OPEN，但不阻断轮次收口**。证据列已是 finding-specific（panel 行号 + 问题标题），仍非 fix-specific（旧串→新串/commit hunk）；保留给 C-32。
- **B-32 处置列被大桶压缩：OPEN（不阻断）**。118 行的 `处置`/`复验` 仍同文；与 B-56 同源。
- **B-68 逐 ID 证据同文：OPEN（不阻断）**。118 行有 85 个不同问题串，33 个是跨角色同根重复；已脱离纯模板，但未到逐条修复证据。
- **B-67 二审半分制：OPEN（不阻断）**。RB-post 第二次 13/14=92.9% 依赖部分通过按 0.5 计；严格 12/14=85.7%；主读数应保留首轮 89.7% 与本 meta 93.3%。
- **B-31/B-64/B-33/B-09/B-34/B-36/B-50/B-60/B-62/B-63：OPEN，均不阻断本次轮次收口**。其中 B-64（冷读者复读）是下一轮教学面的首选装置。

### 5. 给 L7 的候选（第三次，只保留仍成立）

1. **C-29 裁决实施门**：B-65 机制仍开；D-031/D-033 实例已闭，但还没有工具在 `implementation_status` 与代码之间做自动对账。
2. **C-31 二次回读口径**：B-67 仍开；`metrics.md` 应分列“首轮随机抽样/二次定向复核/完全通过/部分通过”，禁止用半分制跨越 90%。
3. **C-32 台账逐 ID + 证据列**：B-56/B-32/B-68 仍开；当前 evidence 是 finding-specific，下一步要求 fix-specific（旧串→新串或 commit hunk），但不阻断本轮。
4. **C-34 三源计数门**：B-55 已由人工修正关闭；仍建议把 loop §3/§6/metrics/台账的同数检查做进 `eval-trace-check.py`，防止下一次复发。
5. **C-30 全轮提交门**：B-61 已关闭；保留为 L6 的 `git status` 硬门，防止下一次“只提交代码、忘记轨迹”。

