# R24 meta-review（01-boot-shim-bootstrap）

> 角色：冷启动 P-META。只读审计，唯一写入本文件。
> 审计时点：`2026-11-09`；HEAD `bd0ca319c`（R24 五项裁决落账），R24 基线 `bd7c53cdb`。
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
- 提交层是红的：审计时 `git status --porcelain` 有 33 个已跟踪修改，其中 31 个是 R24 的修复文件（26 个 `os/` 文件、文档、台账、loop、metrics、related-code），全部未提交；另 2 个是 `.codex/.trae` 的 V2.2 派生文件，另有 `01-design-learn.md` 未跟踪；HEAD `bd0ca319c` 只是“R24 五项裁决落账”，不是修复提交；`BASELINE.txt` 最后一条是 BOOT-SCOPE，没有 R24 的 `r24_fix_head`/`r24_fixed_doc_sha256`。这意味着：R24 的 116 条 ① 没有一条能从提交复现，R-1 的“commit 级回归 review”没有对象，R-2 的“轮转闸”证据只指向工作树。B-61 从“收口后的未提交修复”升级为“整轮修复未提交”。
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
| R-1 commit 级回归 review 分级 + 当场修 + 复验终止（递归上限 2 层） | `review-evals.md` §4e、`review-loop.md` L4.6 | R24 没有任何修复提交；HEAD `bd0ca319c` 只是裁决落账；31 个 R24 修复文件全在工作树；`loop.md:1292-1306` 没有逐 commit 的 diff/复验记录；自然也没有“当场修完才下一个 commit”的对象 | **FAIL（未落地）** |
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
