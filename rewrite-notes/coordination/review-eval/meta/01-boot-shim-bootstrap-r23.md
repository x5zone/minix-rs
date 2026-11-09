# R23 meta-review

> 角色：冷启动 P-META。只读审计；唯一写入本文件。未改任何仓库文件。
> 审计时点：`2026-11-09`；HEAD `2d67f8723`（R23 制品之后又有 `013e55396`、`2d67f8723` 两个不触碰 R23 文档/台账的提交）。
> R23 基线：`8269805734dfc7781496ccae5e8206562ccbfa02`，文档 3397 行 / `11870c0a…`（`loop.md:1184`）。
> R23 修复：`7f34f8b64`（代码+文档）、`31c63cc27`（轨迹）、`91a010975`（RB23 补修）。
> 面板：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round23/`（7 文件）与 `round23-rb.md`。
> 方法：只读 `git show` / `git status` / `grep` / `sed` / `awk` + 文档门重跑（anchor、structure、ref、doc-code-map）。**未重跑 cargo test 与 QEMU**；宿主计数用 `#[test]` 静态计数核对（minix-boot 196、boot-shim 135，与台账一致）。
> 同 agent 局限：本 meta 是独立会话，但仍由同一模型执行；所有判断均附命令或文件行，未做跨工具第二人复核。

## 抽样复判

抽样 30 条 / 110 条 = 27.3%（≥20%）。判定口径：台账处置类与最终制品状态一致，且台账证据行在**轨迹提交 `31c63cc27` 写入时**已成立，记「一致」；只有经后续 RB 补修 `91a010975` 才成立的，记「后补」并按写入时点计「不一致」；处置类可用但理由未回应发现点的，记「部分」。

| ID | 台账判断 | 我的判断 | 证据 |
|---|---|---|---|
| FACT-A-01 | ① 真修 | 一致 | doc:2523-2555 引文 11 形参，`authority: minix_boot::InputObjectId` 在 2534；`os/boot-shim/src/lib.rs:101-132` 同形 |
| FACT-A-02 | ① 真修 | 一致 | doc:2569 字段表已有 `authority` 行 |
| FACT-A-03 | ① 真修 | 一致 | doc:2295 `enter_kernel_image_v1(arena_pa, ...)`；`os/kernel/src/lib.rs:419-423` 三参为 `PhysBytes` |
| FACT-A-04 | ① 真修 | 一致 | doc:1719 已改「三个类型化地址」；`os/libs/minix-boot/src/boot_prepare.rs:26-35` 四字段 |
| FACT-A-05 | ① 真修 | 一致 | doc:3019/3110「`build_memmaps` 生产不调用；单表包装被 15 个测试内核调用」 |
| FACT-A-06 | ① 真修 | 一致（后补） | 现 `related-code.txt` 48/48 存在；`31c63cc27` 时第 35 行仍为 `boot_shim.rs`（47/48），RB Q1 后修 |
| FACT-A-07 | ① 真修 | 一致 | doc:3219 `collect_param_buf_into` |
| FACT-B-02 | ① 真修 | 一致 | doc:1486「固件选择 7 处 + `qemu-test-kernels` 8 处」；`grep -c 'cfg(feature = "qemu-test-kernels")' os/boot-shim/src/uefi_helpers.rs` = 8 |
| FACT-B-03 | ① 真修 | 一致 | doc:2212 单一后果 `pt_alloc::alloc_pt_page called before register`；`os/arch/src/arch/pt_alloc.rs:134` |
| FACT-B-04 | ① 真修 | 一致 | doc:2808「整表，不做筛选；表内只有 PLIC + UART」；`os/kernel/src/lib.rs:922-925` 无 filter |
| FACT-B-09 | ① 真修 | 一致（后补） | doc:1827「22 个成员」；`awk 'NR>=186&&NR<=620' os/arch/src/arch/paging.rs \| grep -E '^    (unsafe )?(fn\|type\|const) ' \| wc -l` = 22；`31c63cc27` 时为 20，RB Q2 后修 |
| FACT-B-13 | ① 真修 | 一致 | doc:1099-1101 补 `phys_bytes offset;`，块头补省略范围 |
| FACT-B-15 | ① 真修 | 一致 | doc:655「固件通常升序，规范不保证」 |
| FACT-B-16 | ① 真修 | 一致 | doc:2807 MTRR 前提 + doc:3178 TODO-10 |
| READER-A-10 | ④ 真修 | 一致 | doc:1086 首次出现处定义 `k` |
| READER-A-18 | ③ 保留 | 不一致 | doc:668-686 表仍在，每格仍混「断开/不断开/取决于更早切割形态」；台账理由「表已穷举条件组合」没有回应「单格多结论不可消费」；宜 ④ 或写明风险接受（B-42 形态） |
| READER-B-09 | ① 真修 | 一致 | doc:3016「两条固定标记 + 第三个标记的三条变体之一」 |
| READER-B-12 | ③ 保留 | 部分 | doc:1290-1307 的 ARM 步骤 2 单元格只列 `pg_identity`；ARM 实际也有 `pg_clear`/`pg_mapkernel`（`minix3/minix/kernel/arch/earm/pg_utils.c:158/184/243/250`）。表后总结「步骤 2、3 同名」本身成立，但「范围已在表内」不成立 |
| READER-B-17 | ① 真修 | 一致 | doc:1699-1700 求和语序已改「合计 30624 B…余 2144 B」 |
| READER-C-10 | ① 真修 | 一致（后补） | doc:1132 现为「本文对时序的推断，C 源没有成文依据（待源码核实）」；`7f34f8b64` 时写「源码注释…声明」，是该修复提交新增的假归因（RB B2） |
| READER-C-15 | ① 真修 | 一致 | doc:2739「在全部落地区（七个加名字池）里」 |
| READER-C-17 | ③ 保留 | 部分 | doc:350 仍只写「不会死循环」；NUL 之后的行为没有限定，台账「文档已限定扫描范围」是范围判断，不是反证。低置信可保留，但 ③ 理由弱 |
| DESIGN-01 | ① 真修 | 一致 | `os/libs/minix-boot/src/kernel_info.rs:446-490` 三条 `assert_eq!`；负控 998-1016 |
| DESIGN-03 | ① 真修 | 一致 | `os/boot-shim/src/lib.rs:143-173` `take()` + `AtomicBool` 二次 panic；两调用点已 `.take()` |
| DESIGN-07 | ① 真修 | 一致（后补） | `os/boot-shim/src/uefi_helpers.rs:1126-1130` 用 `classification.conventional/reserved`；doc:2283 同步；`7f34f8b64` 的 doc:2282 仍是旧 `decode_descriptors`（RB B1） |
| DESIGN-09 | ① 真修 | 一致 | `os/kernel-image/src/bootface.rs:270-274` 删除 `pt_alloc::register` 并留注释；`bootface_a64.rs` 同 |
| DESIGN-11 | ② 转 R24 | 一致（形式） | 跨 crate wire：`os/kernel/src/vm_handoff.rs:586-595` 写 `count=0`；`os/libs/minix-types/src/types/boot.rs:90-101` 自述只保 v8 大小。但应入 `OPEN-DEBT.md`（带负责人），且 `DECISIONS-PENDING` 无条目 |
| DESIGN-13 | ② 转 R24 | 一致（形式） | `os/arch/src/arch/pt_alloc.rs:91-114` release 静默保留第一个；`os/arch/tests/pt_alloc_write_once.rs:38-46` 锁该行为。属用户裁决，但 `DECISIONS-PENDING` 无条目 |
| DESIGN-16 | ① 真修 | 一致（后补） | `boot_prepare.rs` 存在、`boot_shim.rs` 不存在；doc:1431 已改；`related-code.txt:35` 直到 `91a010975` 才改（RB Q1） |
| CLAIM-06 | ① 真修 | 一致 | doc:3019 M15「只在两条 UEFI 腿…执行」；doc:3309「不在 `prepare_boot` 路径上，也不在两个直核面里」；`uefi_helpers.rs:1131-1133` 为唯一生产调用 |

### 抽样统计与元发现

- 一致 27、部分 2、不一致 1。**最终态严格一致率 27/30 = 90.0%**；按轨迹提交写入时点（扣除 5 条「后补」：FACT-A-06、FACT-B-09、READER-C-10、DESIGN-07、DESIGN-16）为 **22/30 = 73.3%**；若把 ② 的日期形式也算入写入时点，为 20/30 = 66.7%。
- RB 自抽 ① 36 条：真修 30、半修 5、处置错 1（`round23-rb.md:89`），真修率 30/36 = 83.3%。两次独立抽样都指向同一结论：**R23 的自评在 RB 前不可信，RB 后基本可信**。
- 抽样之外，meta 另发现 4 处未被面板/RB 点名的残留（见问 8）：
  1. `build_kernel_info` 引文块头写「略去 `minix_boot::`/`minix_types::` 路径限定符」，但 2534 行保留 `minix_boot::InputObjectId`——修 FACT-A-01 时引入的块头/正文不一致（P3，需一处措辞修）。
  2. `loop.md:1217` 写五门 `anchor 106/106`；`91a010975` 实测 `anchors=105 resolved=105`（基线 106，修复删除一个 `main.c:kmain` 锚点）。读数是冻结基线的，不是修复后制品的。
  3. `loop.md:1200` 写「另 P1 约 35 条」；台账实际 P1 = 49（`awk` 统计 3 P0 / 49 P1 / 53 P2 / 3 P3）。「约」差 14 条，不是近似。
  4. `metrics.md:68` 写「重构 提出 8 / 接受 5 / 实施 5 / 登记 3」，括号里却列 5 个转出项（wire 偏移、bootface 合并、`kern_ident`、landing helper、`pt_alloc` release）——「3」与清单「5」自相矛盾。

## 九问

| 问题 | 结论 | 证据 |
|---|---|---|
| 1 Detection | PASS | 以前漏的接住了：B-08 计数/签名漂移（FACT-A-01/04/07、FACT-B-02/09）、B-12 引文改写（FACT-B-05/08/11/13）、B-15 公共入口调用方（FACT-A-05、DESIGN-09、CLAIM-06）、B-03 跨节矛盾（READER-C-02/03/07/08/18/20）、B-01/B-02 设计残留（DESIGN-03/06/07/08/09/11/16）。仍漏：B-09（五门绿而 RB 找到 P0 文档-代码失配）、B-33（reader-a 18:19:35 交付 vs 修复提交 18:20:09，无冻结基线证据）、B-44（RB 判决未入 run 轨迹）、B-08 状态未从 OPEN 更新 |
| 2 Precision | PASS | ③ 6/110 = 5.5%，但 6 条不是同一种东西：清晰误报 1 条（DESIGN-05，`write_envelope` 被跨 crate 测试使用，属测试接缝）；处置争议 3 条（READER-A-18、B-12、C-17，都是低置信可读性项，归 ③ 消掉了本可 ④ 的修复）；类别误用 2 条（CLAIM-01/03 是「反证失败（主张成立）」，不属「③ 误报」）。角色模式：3 条读者 ③ 全是 §4.4 强制的「低置信高价值」配额项；事实角色 0 误报。按清晰误报计 1/104 = 1.0%，按 ③ 计 5.5% |
| 3 Design quality | PASS | 有可删除抽象/错误建模/多余状态，不是命名级：`BootStorageCell` 从 `&self` 造 `&mut`（DESIGN-03，现 `take()+AtomicBool`）、wire 偏移三份定义（06）、编码后解码往返（07）、`classify` 死参数（08）、bootface 冗余 `register`（09）、`kern_ident` 恒空 1 KiB（11）、`boot_shim.rs` 只剩结果类型（16）。1 条误报（05）；5 条 ② 中 06/12 应本轮修 |
| 4 Independence | CONCERN | 冷读确实独立：40 条 ④ 全为读者独有且全部进修复；但 110 条里约 15 条是跨角色同根重复（A-02=B-04、A-03=B-07、A-12=B-11、A-15=B-18=C-16、A-01=B-14、B-13=C-04、B-10=C-12、B-01=C-01=FACT-B-03、FACT-A-01=B-01=CLAIM-04=DESIGN-14、FACT-A-04=B-14、FACT-A-06=B-17、DESIGN-15=FACT-B-06、C-05=CLAIM-05），唯一根约 95/110（86.4%）。无 `dup-of` 字段，独有量不可机械计算；reader-a 晚交付 4 小时且无面板前后 `git status` 记录（`review-panel.md:323-325` 要求），B-33 复发 |
| 5 Scope discipline | CONCERN | `adjacent 阻塞` 0、`re-litigation` 0（`loop.md:1205`、`metrics.md:66`）。但 D-026 在用户未答复时代裁两条用户裁决项（`DECISIONS.md:100`）；② 的 DESIGN-06/12 是同 crate 范围内可实施重构，按 `review-evals.md:181/194` 必须 ①/⑥ 本轮修；DESIGN-11 是跨 crate 却未进 `OPEN-DEBT.md`；DESIGN-10/11/13 无 `DECISIONS-PENDING` 条目 |
| 6 Teaching quality | CONCERN | 读者发现真实障碍：计数索引（A-01/A-13）、观测常态（A-11）、历史险情顺序（B-08）、风险窗口（C-02）、防线计数（C-07）、M11 两道/三道（C-20）。RB 抽 17 条 ④：16 真修 + 1 半修（B-15，后由 Q4 补成纯指针）= 最终 17/17；但无「修复后冷读者复读」，无法证明障碍消失；40/40 采纳是「④ 必修」政策强制，不是质量信号；纯文风噪声没有分类口径 |
| 7 Cost | CONCERN | 7 agent、约 4 小时、110 条（`metrics.md:64-70`）；R22 为 67 条 / 约 55 分钟，成本约 4.4 倍。token 未采集（`metrics.md:69`），6B.7 的「≤前 3 篇均值 1.2 倍」无法计算（B-36 仍 OPEN）。边际收益：2 个唯一 P0 根 + 8 条结构性设计项 + 40 条读者项；不算纯递减，但 23 轮后仍有 40 条读者措辞/结构项，过度编辑风险真实存在（`loop.md:1231` 自述） |
| 8 Trace integrity | CONCERN | 覆盖 PASS：面板 110 ID 与台账 110 行双向无孤儿/无重复（脚本比对）；110/①59/②5/③6/④40 在 `metrics.md`、`loop.md`、台账三处一致。RB 链真实 PASS：B1 在 `7f34f8b64` 的 doc:2282 确为旧 `decode_descriptors`，`91a010975` 的 doc:2283 与源码 1126 同步；B2 的假归因在 `7f34f8b64` 引入、`91a010975` 删除。但：① 行普遍缺 §4d 要求的反向验证与 fix-scope（全台账无「还原修复→变红」记录）；RB 抽 ① 36 条中 5 半修 + 1 处置错；台账 3 处证据在写入时点为假（48/48、带日期、全仓引用更新）；五门 anchor 106/106 与制品 105/105 不符；P1 35 vs 49；metrics「登记 3」列 5 项；RB23 判决未写入 `loop.md`/`metrics.md`/台账（`loop.md:1219` 仍是「RB23/meta 待跑」）；Q2 残项只在未提交工作树里修（见下） |
| 9 最值得但看不见 | 见下 | **读者修复后的复读缺失**：40 条 ④ 只有「文档 diff」证据，没有第二个冷读者确认「障碍已消失」。RB 是修复者视角，检查的是改动存在，不是理解恢复 |

### 问 1 补充：Detection 的具体得失

- 接住的旧盲区：B-08（`FACT-A-01` 11 参 vs 10 参、`FACT-B-09` Paging 20→22、`FACT-A-04` 两→三个地址、`FACT-B-02` cfg 8 处）、B-12（`FACT-B-05` SAFETY 注释被换、`FACT-B-08` 省略行数、`FACT-B-11` 闭包改名与借用合同截断、`FACT-B-13` `phys_bytes offset;` 缺失）、B-15（`FACT-A-05` `build_memmaps` 无生产调用、`DESIGN-09` bootface 重复注册、`CLAIM-06` 断言只覆盖 2/5 腿）、B-03（`READER-C-02` "任意一次" vs 最高下标、`C-03` 取整因果、`C-07` 三道/四道、`C-08` 全部 DRAM、`C-18` 11 个模块不归还、`C-20` 两道/三道）、B-01/B-02（`DESIGN-03` 可双借的 `&mut`、`DESIGN-07` 编码后解码、`DESIGN-08` 死参数、`DESIGN-11` 恒空字段、`DESIGN-16` 模块名残留）。
- 仍漏：五门在 `loop.md:1217` 全绿的同时，`91a010975` 之前的 doc:2282 有 P0 级引文失配（B-09）；RB23 判决只存在 `tmp/` 的 `round23-rb.md`，run 轨迹没有一行（B-44）；reader-a 交付时间 18:19:35 与修复提交 18:20:09 只差 34 秒，其他角色 14:01–14:23 已交付，没有「面板期间文档未改」的 `git status`/diff 证据（B-33）；B-08 在 `BLIND-SPOTS.md` 仍是 OPEN，尽管本轮至少 6 处计数/签名漂移被接住。

### 问 2 补充：Precision 的角色/类别模式

- 6 条 ③ 的构成：READER-A-18、READER-B-12、READER-C-17、DESIGN-05、CLAIM-01、CLAIM-03。
- 其中 3 条读者 ③ 都是读者自己在面板里标了「低置信但高价值」的配额项（`reader-a.md:179`、`reader-b.md:115`、`reader-c.md:161`）。这不是读者乱报，而是 §4.4「至少给 1–2 条低置信但高价值的发现」的结构性副作用：配额要求产出，产出后归因又倾向 ③。
- CLAIM-01/03 不是误报：`refute.md` 的结果是「反证失败（主张成立）」，处置应为「无缺陷/主张成立」，不应记 ③。这让「③ 误报 6」这个读数虚高，也让 Precision 的类别统计失真。
- 清晰误报只有 DESIGN-05（`write_envelope` 被测试使用，不是死接口面）；READER-A-18/B-12/C-17 是低置信可读性项，问题在「用 ③ 消掉 ④」而不是「读者乱报」。事实角色（FACT-A/B）0 误报；读者角色 3 条 ③ 全是配额项。结论：误报率低（清晰误报 1/104 = 1.0%），但**类别标签、配额机制与 ③/④ 边界**需要修。

### 问 3 补充：Design quality 的落地程度

- 本轮设计发现不是「命名/注释级」：`DESIGN-03` 是 Rust 别名规则下的 UB 面（`&self -> &mut T`），`DESIGN-06` 是 wire 三份偏移源，`DESIGN-07` 是 EBS 后多余的编码→解码失败路径，`DESIGN-08` 是死参数，`DESIGN-09` 是重复注册掩盖顺序错误，`DESIGN-11` 是 1 KiB 恒空 wire 字段，`DESIGN-16` 是删 trait 后的模块名残留。16 条的处置是 10 条 ①（7 条代码项 01/02/03/07/08/09/16 + 3 条文档项 04/14/15）、1 条 ③（05）、5 条 ②（06/10/11/12/13）；其中 06/12 本应本轮做。
- 设计质量的边界：`DESIGN-10`（两个 bootface 约 280 行非注释重复）被识别但没有动；`DESIGN-12`（七段落地区五段同构）也没有动。两者都是「可删除的重复层次」，说明设计角色已经能看见结构问题，但工作流的「全修」没有覆盖到它们。

### 问 4 补充：Independence 的量化

- 面板 7 个角色，读者 3 个只读文档，未读源码（reader 文件无 `os/`/`minix3/` 引用），材料隔离成立。
- 独有发现：40 条 ④ 全部由读者报出，事实/设计角色没有报同名项；其中至少 20 条是真正的理解障碍（计数索引、观测常态、依赖图虚线、M11/M12 计数、历史险情顺序、风险表区间、栈顶对象），不是事实错误。
- 但「独有」不可机械计算：台账没有 `dup-of`，跨角色重复约 15 条；`metrics.md` 只记原始条数。按人工归并，唯一根约 95 条，其中 6 条非缺陷、89 条实质项。
- 基线完整性：reader-a 是唯一 18:19 交付的角色，交付时修复提交 18:20:09 已在即；`loop.md` 没有面板前后 `git status` 记录，无法证明它读的是 13:47 冻结的 3397 行版本。内容看它引用的仍是修复前文本（如「图例在 §3.4.1」），大概率读的是旧快照，但这是推断，不是证据（B-33）。

### 问 5 补充：Scope discipline 的三处具体越界/漏记

1. **D-026 代裁**：`DECISIONS.md:100` 明写「用户未答复结构化提问；agent 按两条推荐项代裁，可回退」。两条都是「待用户裁决」项（F-R17-01 删 trait、F-R17-04 `KernelInfo` 13→14）。按 `review-evals.md:201`，② 裁决项应等用户；代裁把用户决策变成了 agent 决策。虽然记录完整、可回退，仍应记为过程偏差。
2. **② 与 ⑥ 的边界**：`review-evals.md:181` 要求「范围内确认为真缺陷的一律本轮修」；`194` 要求 P-DESIGN 可实施改进标 ⑥ 本轮实施，只有跨 crate 协调才进 `OPEN-DEBT`。DESIGN-06（wire 偏移，全在 `minix-boot`）与 DESIGN-12（`store_kernel_info` 的 `land_slice`，全在 `os/kernel/src/lib.rs`）都不跨 crate、不需要用户裁决，却标 ② 转 R24。这是「应修尽修」的两处实质违反。
3. **登记位置**：DESIGN-11 是跨 crate（`minix-types`/`minix-boot`/`kernel`/VM server）wire v9，应进 `OPEN-DEBT.md`（负责人 + 截止轮次），实际只在 `follow-ups.md:115`；DESIGN-10/11/13 需要用户/架构 owner 裁决，`DECISIONS-PENDING*.md` 均无对应条目。`OPEN-DEBT.md` 当前只有 F-R17/F-R20 旧项，没有 F-R23-*。

### 问 6 补充：Teaching quality 的采纳率与噪声

- 采纳率：RB 抽 17 条 ④，16 真修 + 1 半修（READER-B-15），Q4 补修后 17/17 = 100%。全量 40 条只有台账行，未逐条独立复判；按台账 40/40 计。
- 真实障碍的证据：A-01/A-13（六套/八套计数）、A-11（观测常态 vs 无条件常态）、B-08（历史险情放在「若」之后）、C-02（「任意一次」vs 最高下标）、C-07（三道/四道防线）、C-20（M11 两道/三道）、C-19（`kernel_boot_stack_top` vs `kern_stack_top`）都是会让读者推错的对象，不是纯文风。
- 噪声：3 条低置信配额项被 ③；`READER-A-24` 的修复只加了「理论路径」标记，没有按读者要求把「现行 Step 1 全部是 2 MiB 叶」前移；`READER-B-15` 第一次只加了前向括注，仍下「§3.7 禁止」结论，RB Q4 才改成纯指针。说明「改了字」不等于「障碍消失」，而工作流没有复读门。

### 问 7 补充：Cost

- 7 agent（FACT-A/B、READER A/B/C、REFUTE、DESIGN-D2），约 4 小时，110 条；R22 为 7 agent、约 55 分钟、67 条；R21 约 50 分钟、63 条；R20 约 40 分钟、64 条。R23 的时间是 R22 的 4.4 倍，条数是 1.64 倍。
- token 未采集，`metrics.md:69` 明写「token 未采集」。因此 G-workflow 第 7 条（成本不超过前 3 篇均值 1.2 倍）无法判定；B-36 仍 OPEN。
- 收益侧：2 个唯一 P0 根（`authority` 引文、cfg 断言）+ 8 条设计结构项 + 40 条读者项 + 6 条 ③。读者项占 36%，其中多数是措辞/结构；这不像纯递减（设计/事实仍有新发现），但也不是高性价比轮次。`loop.md:1231` 自己提出「是否存在为修而修的过度编辑」，meta 认为有这个风险，但缺 token/复读数据，不能判 FAIL。

### 问 8 补充：Trace integrity 的四层读数

| 层 | 读数 | 判定 |
|---|---|---|
| 每条发现是否有处置行 | 面板 110 ID（104 条 + 6 主张）与台账 110 数据行双向 `comm` 无孤儿/无重复 | PASS |
| 指标是否自洽 | `metrics.md:64-66`、`loop.md:1223`、台账 `awk` 均为 110 / ①59 / ②5 / ③6 / ④40；宿主 minix-boot 196、boot-shim 135 与静态 `#[test]` 计数一致，862+8/287 采信 RB 与台账 | 主计数 PASS；严重度与重构计数 FAIL：`loop.md:1200` P1「约 35」vs 实际 49；`metrics.md:68`「登记 3」vs 括注 5 项；`loop.md:1217` anchor 106/106 vs 制品 105/105 |
| RB 的 NOT PASS→补修→PASS 是否真实 | B1：`7f34f8b64` doc:2282 仍是 `decode_descriptors`，源码 1126 已改 `classification`，`91a010975` doc:2283 同步；B2：`7f34f8b64` doc:1132 的「源码注释…声明」在 `protect.c:352-368` 无依据，`91a010975` 改为「本文推断…待源码核实」；Q1 related-code 47/48→48/48；Q3 五个 ② 加 `2026-11-16`；Q4 READER-B-15 改纯前向指针 | PASS（真实） |
| 修复证据强度 | ① 行只有「doc diff」「code diff; 测试通过」，没有 §4d 的「还原修复→测试变红→恢复→变绿」记录；没有 C-11 的「旧串→新串」grep 零命中；RB 抽 36 条 ① 中 5 半修 + 1 处置错 | FAIL（§4d 未执行） |

**关键残留（收口前必须处理）**：

1. `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 在工作树中仍有一处未提交修改（`git status --porcelain` = ` M ...`），内容是把 `const _: () = assert!(... == NR_BOOT_MODULES)` 改成「数组长度仍是 `[&str; NR_BOOT_MODULES]`」——即 RB Q2 的点名残项。该修改发生在 `91a010975`（18:32:37）与 RB PASS 之后（文档 mtime 18:34:15），不在任何提交、台账行或 RB 复核里。它本身是对的，但**收口状态不可从提交复现**。
2. RB23 的判决没有回写：`loop.md:1219` 仍写「RB23/meta 待跑」，`metrics.md:70` 只写面板与三门，台账没有 RB23 段。RB 的 PASS 只存在 `tmp/review-eval/.../round23-rb.md`；按 C-16/D-024，判决必须入 run 轨迹。
3. `BASELINE.txt` 没有 R23 条目（最后一条是 R22），R23 基线只写在 `loop.md:1184`。R22 meta 的基线与读数核对依赖 `BASELINE.txt`，R23 缺这一层。
4. 五门读数 `anchor 106/106` 是基线读数；修复后为 105/105。应改成 105/105 并注明「基线 106，修复删除一个 `main.c:kmain` 锚点」。
5. `build_kernel_info` 引文块头与 2534 行的 `minix_boot::InputObjectId` 限定符不一致（块头声明略去路径限定符）。

### 问 9：本轮最值得但工作流仍看不见的一件事

**读者修复后的复读缺失。** 本轮 40 条 ④ 全部按「改了文档」结案，RB 抽 17 条也只验证「改动存在」。工作流看不见的是：改完以后，目标读者在原来的卡点是否真的不再卡。证据：READER-B-15 第一次只加前向括注、仍下「§3.7 禁止」结论，RB Q4 才改成纯指针；READER-A-24 只加了「理论路径」标记，没有按读者要求把「现行 Step 1 全部是 2 MiB 叶」前移；READER-A-18 的表格单元格仍混四种结论。三例都说明「文本改动」与「理解恢复」是两件事。

期望装置（低成本）：每批 ④ 修复后，派一个**新的冷读者**只读改动节（不读源码、不读 diff），带原 FINDING ID 与读者画像，回答「障碍是否消失 / 仍卡在哪」；`metrics.md` 增「④ 复读通过率」，低于 90% 的批次不得结案。这比再跑一轮全面板便宜，也比 RB 的 diff 复核更接近教学目标。

## ② 转出裁决

| 项 | 转出理由（follow-ups） | 是否合理 | 裁决与应本轮修的理由 |
|---|---|---|---|
| DESIGN-06 wire 偏移三份定义 | adjacent（R24，截止 2026-11-16） | **不合理** | 全部在 `os/libs/minix-boot`（`handoff_v1.rs` / `handoff_v1_write.rs` / `handoff_v1_read.rs`），不跨 crate、不需用户裁决；`offset_of!` 已在同文件使用。按 `review-evals.md:181/194` 应 ⑥ 本轮实施。应本轮修 |
| DESIGN-12 `store_kernel_info` landing helper | adjacent（R24） | **不合理** | 只改 `os/kernel/src/lib.rs`（七段落地区五段同构），无跨 crate、无行为变更；泛型 `land_slice<T: Copy>` 是纯重构。按同条应 ⑥ 本轮实施。应本轮修 |
| DESIGN-11 `kern_ident` 恒空 1 KiB | 需用户裁决（R24） | **合理但登记不合规** | wire v8→v9 + VM server 同步是跨 crate 协调，转出本身合理；但按 `review-evals.md:181` 应进 `OPEN-DEBT.md`（带负责人/截止），不是 `follow-ups.md`；且应有一条 `DECISIONS-PENDING` 记录，当前没有 |
| DESIGN-13 `pt_alloc` release 二次注册 | 需用户裁决（R24） | **合理但登记不合规** | 改 release 语义/公开 API 是行为变更，必须用户裁决；转出合理。但 `DECISIONS-PENDING*.md` 无条目，只有 `follow-ups.md:117`，用户看不到「等什么裁决」的正式入口 |
| DESIGN-10 两个 bootface 合并 | 需架构 owner 裁决（R24） | **合理** | 约 280 行流程重复，但合并触碰 riscv `satp=0` 物理视图纪律与 boot 关键路径，需 QEMU 直核门逐条验证；设计角色自己给了「继续下沉 helper」的低风险替代。可转出，但同样应有 `DECISIONS-PENDING` 条目与 owner |

结论：**5 条中 2 条违反「应修尽修」（DESIGN-06、DESIGN-12），应本轮修；3 条转出方向合理但登记位置不合规（DESIGN-10/11/13 需 `DECISIONS-PENDING`；DESIGN-11 需 `OPEN-DEBT`）**。`loop.md:1207` 说「带日期与理由」在 `31c63cc27` 时只兑现「理由」，日期是 `91a010975` 才补的（RB Q3）；`metrics.md:68` 的「登记 3」与 5 项清单不符。

## 盲区增量

### 新增（建议 ID，接现有最大号 B-60）

**B-61 收口后的未提交修复（类别：修复质量 / 轨迹）**
- 为什么本轮没看见：所有门读的是已提交制品或面板目录；RB 的复核对象是 `7f34f8b64`/`31c63cc27`；没有门在 L6/meta 入口比对 HEAD 与工作树。
- 本轮实例：RB Q2 残项在 `91a010975` 之后、RB PASS 之后被改到工作树（`git status` = ` M rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`；doc mtime 18:34:15 > 提交 18:32:37），无提交、无台账行、无 RB 复核。
- 期望探测器：L6 入口 `git status --porcelain` 硬门（被审文档/台账/轨迹路径必须干净）；每条 ① 必须带 commit sha；`eval-trace-check.py --require-clean`。

**B-62 低置信配额噪声（类别：面板设计 / 精度）**
- 为什么本轮没看见：③ 表只记处置类，不记「配额项」来源；`metrics.md` 不按来源分档。
- 本轮实例：6 条 ③ 中 3 条（READER-A-18、B-12、C-17）正是 §4.4 强制的「低置信但高价值」项；三条的理由都是范围判断而非反证。
- 期望探测器：面板输出给配额项打 `[QUOTA]`；meta 统计「配额项 ③ 率」；连续两轮 >50% 就改 `review-panel.md:228` 的配额措辞（改成「可以给」而非「至少给」）。

**B-63 G8 计数不区分同根/文风/已判（类别：度量 / 收敛）**
- 为什么本轮没看见：台账无 `dup-of`/`origin`（B-24/B-52 未落码），`metrics.md` 只汇总原始条数；G8 直接拿 110 当「新增」。
- 本轮实例：110 条中约 15 条跨角色同根重复、6 条非缺陷、3 条 P0（2 个根）；唯一实质项约 89。按原始条数，G8 在任何一轮有读者 Top-20 的评审里都不可能为 0。
- 期望探测器：`dup-of` + `origin` 字段（C-14/B-52）；G8 改为「unique substantive new findings = 0 且 new P0/P1 = 0」；原始条数只作成本指标。

**B-64 ④ 修复后无冷读者复读（类别：教学 / 验证）**
- 为什么本轮没看见：读者角色只在修复前跑；RB 是修复者视角，检查 diff 是否存在，不检查理解是否恢复。
- 本轮实例：READER-B-15 第一次半修、READER-A-24 只加标记、READER-A-18 表格仍混条件；40 条 ④ 无一经过第二读者复读。
- 期望探测器：每批 ④ 后派新冷读者只读改动节，回答「障碍是否消失」；`metrics.md` 增「④ 复读通过率」，<90% 不结案。

### 复发与状态更新（本轮应改而未改）

- **B-08**（计数/签名漂移）本轮至少 6 处被接住，但 `BLIND-SPOTS.md` 仍是 OPEN；应改为 DETECTED 并附 R23 证据。
- **B-09**（门绿但事错）五门在 `loop.md:1217` 全绿，`91a010975` 之前 doc:2282 有 P0 引文失配；仍 OPEN，且本轮复发。
- **B-33**（冻结基线无机械门）reader-a 18:19:35 交付、修复提交 18:20:09，无面板前后 `git status`；仍 OPEN，本轮复发。
- **B-34**（代码改动后文档引文无人回读）DESIGN-07 的代码改动使 §4.2 引文过期；RB 接住；仍 OPEN。
- **B-44**（判决不落账）RB23 判决只在 `tmp/`，run 轨迹没有；仍 OPEN，本轮复发。
- **B-50**（半修记整修）RB 抽 36 条 ① 中 5 半修 + 1 处置错；DETECTED 状态合理，但机制未落码。
- **B-36**（成本无仪表）token 仍未采集；OPEN。
- **B-42**（转出三件套）DESIGN-06/12 是本轮「换类规避 ⑥」的实例；OPEN。
- 关闭：无。`BLIND-SPOTS.md` 全文没有 R23 字样（最后更新 12:33，早于面板 13:49），本轮 BLIND-SPOT-DELTA 没有落账。

## 结论

### 总体

**IMPROVED（相对 R22）/ 运行质量 NOT PASS（按写入时点严格一致率 73.3%）**。Detection 与 Design 是实质进步：旧盲区 B-08/B-12/B-15/B-03/B-01/B-02 都被接住，设计角色给出 16 条结构项（10 条 ① 实施/同步、1 条 ③、5 条 ②），事实角色抓到 2 个唯一 P0 根。Precision 清晰误报 1/104 = 1.0%（③ 率 5.5%，其中 3 条是读者配额项的 ③/④ 边界问题、2 条是反证失败被误记 ③）。问题集中在**处置与轨迹**：① 证据没有反向验证/fix-scope，RB 前自评 83.3% 真修，RB 后仍有未提交修复与未回写判决；② 有 2 条应本轮修；读者 40 条没有复读验证。

### 收口判断

R23 作为**轮次**可以有条件收口，但**不能宣告文档 CONVERGED**（G8 新增 110 条；按唯一实质项约 89 条，仍远大于 0；面板 2 个唯一 P0 根 + RB 找到的 1 个 P0 回归均已修）。收口前必须补齐 5 项：

1. 提交工作树中的 Q2 残项修改（doc 的 `const assert` 句），使收口状态可从提交复现（B-61）。
2. 把 RB23 的 `RE-READ: PASS` 与 Q2 残项写入 `loop.md` §5/§7、`metrics.md` R23、`defect-ledger.md` 的 RB23 段（B-44）。
3. 把 DESIGN-06、DESIGN-12 改判 ①/⑥ 本轮实施；若用户明确豁免，写 `DECISIONS.md` 裁决条目（不是 `follow-ups.md`）。
4. 把 DESIGN-11 移入 `OPEN-DEBT.md`（负责人 + 截止轮次）；为 DESIGN-10/11/13 建 `DECISIONS-PENDING` 条目。
5. 更正 `loop.md:1217` anchor 106→105；`loop.md:1200` P1 约 35→49；`metrics.md:68`「登记 3」与 5 项清单二选一；修 `build_kernel_info` 引文块头的路径限定符声明。

### 一致率与局限

- 抽样 30/110 = 27.3%，覆盖 FACT-A/B、READER-A/B/C、REFUTE、DESIGN 全部角色。
- 严格一致率（按台账写入时点、扣除 5 条 RB 后补）= 22/30 = 73.3%；最终态 27/30 = 90.0%（部分 2、不一致 1）；RB 自抽 ① 36 条真修 30/36 = 83.3%。
- 同 agent 局限：本 meta 为独立冷启动会话，但仍由同一模型执行；未做跨工具第二人复核；已重放 `git show`/`git status`/`grep`/`sed`/`awk` 与四个文档门（anchor 105/105、structure、ref、doc-code-map 50/50）。未重跑 cargo test 与 QEMU，宿主计数用 `#[test]` 静态计数核对。

VERDICT: IMPROVED（相对 R22；运行质量 NOT PASS，收口需先补 5 项轨迹/裁决；文档仍 NOT CONVERGED，G8 新增 110 / 唯一实质约 89 / 面板 P0 根 2 + RB P0 回归 1 均已修）；严格一致率 22/30=73.3%（最终态 27/30=90.0%）

---

## 第二次审计（2026-11-09，提交 `a6d6c0e95` / BASELINE `6c97ae794`）

> 只读复核 5 项收口；不改原九问正文。原「运行质量 NOT PASS」的结论在本节按新证据更新。
> 复核方式：`git show a6d6c0e95` / `6c97ae794`、`git status --porcelain`、`sha256sum`、`anchor-resolve.sh --check`、台账/loop/metrics 的 `awk` 计数、`cargo check -p minix-boot --lib`、`/tmp/r23c-*.log`。未重跑 QEMU（日志已由本轮收口产生）。

### 1. 五项收口逐条

| # | 收口要求 | 复核证据 | 判定 |
|---|---|---|---|
| 1 | Q2 残项已提交 | `git show a6d6c0e95 -- rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 把 doc:2515 的 `const _: () = assert!(... == NR_BOOT_MODULES)` 句改为「数组长度仍是 `[&str; NR_BOOT_MODULES]`」；`git status --porcelain` 不再有该文档的已跟踪改动；`sha256sum` = `0c92afc4…`、3433 行，与 `BASELINE.txt` 的 `r23_fixed_doc_sha256` / `r23_fixed_doc_lines` 一致 | PASS |
| 2 | RB23 PASS 写入 loop §5/§7、metrics、ledger | `loop.md:1219` 写「RB23 首轮 NOT PASS…补修后第二次回读 PASS」；`loop.md:1227` 写「RB23 补修后 PASS」；`metrics.md:71` 写「补修后第二次回读 PASS」；`defect-ledger.md` 新增「R23 独立回读（RB23）与补修」段，逐条列 B1/B2/Q1–Q4 | PASS（有三处 P3 陈旧措辞，见第 2 节） |
| 3 | DESIGN-06/12 改 ① 本轮实施 | 台账 `DESIGN-06` / `DESIGN-12` 已从 ② 改为 ①；`follow-ups.md:113/116` 标「✅ 已修（R23 补修）」；代码：`os/libs/minix-boot/src/handoff_v1.rs:335` 新增 `pub mod envelope_offsets`（22 个 `offset_of!`），`handoff_v1_read.rs:36` / `handoff_v1_write.rs:70` 改为 `pub use …::envelope_offsets::*`，两份裸数字已删；`os/kernel/src/lib.rs:1189` 新增 `fn land_slice<T: Copy>`，五段调用在 1221/1233/1272/1291/1294，`param` 清零与 `modules` 名字池特例保留；`cargo check -p minix-boot --lib` 退出 0；静态 `#[test]` 计数 minix-boot 196、boot-shim 135、kernel 870（862+8）| PASS（形式条件：两条纯重构没有新增 `#[test]`，见第 3 节） |
| 4 | DESIGN-11 进 OPEN-DEBT；10/11/13 进 `DECISIONS-PENDING.md` | `OPEN-DEBT.md:17` 新增 `F-R23-03`（负责人 boot owner、截止 R24 2026-11-16）；`DECISIONS-PENDING.md` 末新增「R23 新增待裁决（2026-11-09）」表，D-R23-01（DESIGN-10）、D-R23-02（DESIGN-11）、D-R23-03（DESIGN-13）各有事实/选项/默认建议 | PASS |
| 5 | anchor 105/105、P1 49、metrics「登记 3」三项、引文块头 | `tools/anchor-resolve.sh --check` = `anchors=105 resolved=105 zero-def=0 multi-def=0`；台账严重度 `P0 3 / P1 49 / P2 53 / P3 3 / — 2`；处置 `①61 / ②3 / ③6 / ④40`；`metrics.md:68` 改为「登记 3（bootface 合并、kern_ident 删除、pt_alloc release 语义转 R24；wire 偏移与 landing helper 本轮已实施）」，清单恰好 3 项；`loop.md:1217` 改为 anchor 105/105；`loop.md:1200` 改为 P1 49（但 `loop.md:1205` 仍留旧 ①59/②5，见第 2 节残留）；`handoff_v1.rs` 引文块头在 doc:2522 改为「略去 `minix_types::` 路径限定符；`minix_boot::InputObjectId` 保留以标出权威类型」 | PASS（`loop.md:1205` 旧数残留） |

### 2. 剩余阻断与计数

- **P0 open = 0**。台账 3 条 P0（FACT-A-01、FACT-B-01、FACT-B-02）全部 ①；FACT-A-01/B-01 的引文与 `os/boot-shim/src/lib.rs:101-132` 同形，FACT-B-02 的 cfg 陈述在 doc:1486 已限定为「固件选择 7 处 + `qemu-test-kernels` 8 处」；RB 找到的 B1（§4.2 旧 `decode_descriptors`）已在 doc:2283 与源码 1126 同形。
- **P1 open = 0**。49 条 P1 全部 ①；RB 抽 36 条 ①（含大部分 P1），本 meta 原抽 30 条又覆盖一批；RB 未抽到的 19 条 P1 ①（FACT-A-02、FACT-B-04/06/07/08/12、READER-B-02/03/05/17、READER-C-02/03/05/06/07/08/11/13/14）在本次复核中逐条看了改动：注册失败消息、aarch64/riscv 表行、`phys_bytes offset`、arena 求和、`arch_boot arg/reslen` 路标、`add_memmap` 空洞不豁免、取整因果、三道/四道防线、NUL 后限定、20 号槽改「最后一个槽（下标 19）」、两种 `KernelMapError` 文案，均已落地；未发现新开 P0/P1。
- **新增 P0/P1 = 0**；收口提交只做 DESIGN-06/12 重构与轨迹校正，未引入新的文档-代码失配（`cargo check -p minix-boot --lib` 0，anchor 105/105）。
- **非阻断残留（P2/P3）**：
  1. `loop.md:1201` 仍写「设计侧 16 条，5 条转 R24」；实际是 3 条（DESIGN-10/11/13）。`loop.md:1205` 仍写「① 59 / ② 5」，与同文件 §6 的 61/3 冲突（§6 已改，§3 未改）。
  2. `loop.md:1227` / `metrics.md:71` 仍带第一轮 meta 的「运行质量 NOT PASS」；第二次审计改判「条件 PASS」后应同步改掉，否则 run 轨迹与 meta 终稿不一致。
  3. `loop.md:1218` 写 QEMU 日志在 `tmp/review-eval/.../logs/round23-*`；该目录只有 `round22-*`，实际日志是 `/tmp/r23c-x86.log`、`/tmp/r23c-a64.log`、`/tmp/r23c-rv.log`（分别含 `TEST_RESULT: PASS` / `TEST_RESULT: PASS` / `RESULT: PASS … 6/6`）。证据存在，路径不实，且 `/tmp` 不随仓库归档。
  4. DESIGN-06/12 是纯重构，没有新增 `#[test]`；§4d 字面要求「新增或修改至少一个 `#[test]`」。现有测试覆盖了行为（`test_store_kernel_info_lands_firmware_slices` + 七条 `rejects_*_overflow`；`handoff_v1` 的 22 个 `offset_of!` 偏移常量与同文件的编译期布局门），但台账应写一句「重构豁免」或补一条 `land_slice` 同址/溢出单测。
  5. `metrics.md:70-71` 有两条 `回读:` 键，建议合并为一条带时间线的记录。
  6. 原抽样中 3 条处置争议（READER-A-18 的 ③、READER-B-12 的 ③、READER-C-17 的 ③）未因收口改变；它们是 R24 读者轴改进项，不是本轮 P0/P1。
  7. B-62/B-63/B-64 按 `BLIND-SPOTS.md` 的 R23 delta 仍是 OPEN（配额噪声、G8 同根计数、④ 复读缺失），属 R24 观察项。

### 3. 一致率更新

- 原 30 条抽样不变：**写入时点 22/30 = 73.3%；最终态 27/30 = 90.0%**（部分 2、不一致 1）。
- 5 项收口复核：**5/5 PASS**。
- 若把 5 项收口作为定向样本并入最终态：32/35 = 91.4%（定向样本，不与原随机抽样同分布，仅作参考）。
- RB 自抽 ① 36 条：真修 30/36 = 83.3%（历史读数；其 5 半修 + 1 处置错已在 `91a010975` 补修）。
- 结论：**最终态严格一致率 90.0%（原抽样）+ 收口 5/5**，达到 4.6 的 ≥90% 门槛；写入时点 73.3% 保留为「RB 前自评不可信」的历史读数，不再作为 R23 收口阻断。

### 4. 结论更新

- **「运行质量 NOT PASS」可以改为「条件 PASS」**。理由：5 项收口全部落地；P0/P1 open = 0/0；最终态严格一致率 90.0%；RB23 的 NOT PASS→补修→PASS 链真实；`BASELINE.txt` 已封存 `r23_fix_head=a6d6c0e95` / doc `0c92afc4…` / 3433 行。
- 条件（P2/P3，不阻断 R23 轮次收口，但应在下一次轨迹写入时清掉）：① 更正 `loop.md:1201/1205` 的 5 条/59-5 旧数；② 把 `loop.md:1227` 与 `metrics.md:71` 的第一轮 meta 结论改为「条件 PASS / 5 项已处理」；③ 把 QEMU 日志路径改成实际 `/tmp/r23c-*` 或归档进 `tmp/review-eval/.../logs/round23-*`；④ 在台账给 DESIGN-06/12 写「重构豁免」或补一条单测；⑤ 合并 `metrics.md` 的两条 `回读`。
- **R23 可以收口为轮次**（RB PASS + meta 条件 PASS + 5 项收口 + BASELINE 封存）；**文档仍 NOT CONVERGED**（G8 新增 110，唯一实质约 89；3 条 ② 转 R24 带日期/裁决条目；B-62/B-63/B-64 留 R24）。原 final VERDICT 中的「运行质量 NOT PASS」由本节更新为「条件 PASS」。
