# 01-boot-shim-bootstrap Round 20 meta-review（冷启动）

- 评审对象：`runs/01-stage-kernel/01-boot-shim-bootstrap/` 的 R20 段（`loop.md:1001-1056`、`defect-ledger.md:1400-1438`、`metrics.md` 的 R20 块）、`follow-ups.md`、`OPEN-DEBT.md`、`BASELINE.txt`；七份面板原始输出 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round20/`；独立回读 `panel/round20-rb.md`；上一轮 `meta/01-boot-shim-bootstrap-r19.md`；`prompt/review-rules/review-evals.md`、`review-panel.md`；`BLIND-SPOTS.md`、`WORKFLOW-VERSIONS.md`、`DECISIONS.md`；当前文档与关联源码。
- 基线核对（`git log`、`git show`、`wc -l`、`sha256sum` 重放）：
  - 面板读取基线 `03ed48505c1e605a1a070e818397704ae4283128`（2026-11-09 06:11:19），文档 3387 行、sha256 `56cefc7d…`，与 `loop.md §0`、`BASELINE.txt` 的 `r20_panel_base_*` 逐项吻合。
  - 最终修复 `e4e26862a39db03faf98ca585983e882f3953160`（06:48:21），文档 3390 行、sha256 `0314e194a7d174cc4be47af181aba73fe87ed4e87f0082ed177f92ac6fe349c0`；`BASELINE.txt` 的 `r20_fix_head`、`r20_fixed_doc_sha256`、`r20_fixed_doc_lines` 与提交及工作树逐项相同。冻结提交 `85dd6c74b`（06:48:41）。
  - 当前 `HEAD=85dd6c74b`，`git status --porcelain` 只有未跟踪的 `prompt/resources/`。
  - 可观测时间线：面板落盘 06:14–06:30（约 16 分钟），第一版修复 06:36，RB20 06:42，最终补修 06:48；文件活动跨度约 37 分钟，与 `metrics.md` 的“约 40 分钟”量级相符（R19 的“4.5 小时”问题未复发）。
- 未采信：`loop.md` / 台账自评只作线索，每条结论回源到面板原文、文档、C 源、Rust 源或实际命令。`expected/`、`traps/`、其他篇目输出未读。
- 局限声明：**同 agent 验证 + 已重放命令**。本报告是冷启动子 agent，但与主 agent 同模型；抽样不是随机，刻意覆盖“④ 范围行内未列 ID”“RB20 声称修复项”“③/④ 分档”“删除探针”四类可疑点。

## 结论速览

**`IMPROVED`，运行质量 `NOT PASS`。**

进步在检测层与设计层：R20 用六条 REFUTE 与七条 D2 抓到 R19 的五处修复残留与两处同段漏检（最典型的是 R19 把参数分歧写成“五条”，实际八条）；D2 的七条里只有一条是 D1 方案否决（R19 是十二条里十条），跨角色重复降到一对；`checked_add` 与单区间负控是真实加固；三组删除探针全部转红；宿主测试 189/135/4/868/287 逐项重放命中；`BASELINE.txt` 指向 `e4e26862` 且文档 sha/行数一致。

不通过的原因集中在记账层，而不是面板层：

1. **④ 的“48 条本轮修”不成立。** `READER-A-01..20`、`READER-B-01..20`、`READER-C-01..08` 三条范围行把 48 个 ID 压成三行；逐 ID 对照后，`READER-A-07`、`READER-B-17` 在处置文字里被标成“（③）”，`READER-A-10`、`READER-B-08`、`READER-B-14`、`READER-B-18` 既没有修复文字也没有 ③ 理由。按台账自己的文字复算，至少 6 条 ④ 没有修；`loop.md §7` 的“G1–G4 修复后为 0”因此为假。
2. **RB20 第七项声称已修，实际没改。** `§5.2.1` 的“整条截断”应改“整条丢弃”，`e4e26862` 的提交信息与台账都写了这一项，但文档 3071 行仍是“整条截断”，`git show e4e26862` 的文档 diff 没有这一段。这是“未修记已修”，比半修更重。
3. **分档计数与台账文字相差 2。** 台账把 `READER-A-07`、`READER-B-17` 明写为 ③，却仍按 ④ 汇总；`metrics.md` 的 ③1/④48 只有在忽略这两处括注时才成立。

以下第一至九节是逐问结论，抽样表与半修表在末尾。

## 一、Detection — PASS

### 1.1 本轮抓到的、R19 漏检或修复不完整的实例

| R19 的哪次修复 | R20 抓到的残留 | 证据 |
|---|---|---|
| R19 把参数分歧从“三条”改成“五条” | REFUTE-01：实际至少八条（分隔符集合、非 ASCII/不可打印、空键三条被漏） | R19 `loop.md §4` 写“参数五条分歧”；当前文档 1650 行“共八条”，`os/libs/minix-boot/src/boot_params.rs:8` 写 “eight observable divergences”；`is_separator` 含 tab/CR/LF（`:62-64`），`is_printable_ascii` 与 `eq > 0` 门（`:163-171`） |
| R19 在同段改过 §4.6（Step 1 函数归属） | REFUTE-05：同段的“预算是 1024 页”仍是单腿普遍化，R19 的编辑没有覆盖这一句 | 当前文档 2776 行已改“UEFI/U-Boot 腿 1024 页（两个直核面 256 页）”；`os/boot-shim/src/lib.rs:250` 与 `os/kernel-image/src/bootface.rs:135` 分别 1024/256 |
| R19 C-03 在同段改过 §5.1 M6 的 TODO-05 归属 | REFUTE-06：M6 的宿主证据仍被说成验证生产 Step 1，实际只验证合成窗口下的策略函数 | 当前文档 2975 行“在合成窗口与 1 GiB 块配置下验证策略函数自洽（不跑生产 Step 1）”；`os/kernel/src/lib.rs:5462-5498` |
| R19 B-04 把 §3.4.1 改成“三条 v1 发送腿 + 两个构造入口” | B-01：§4.4 标题与 §3.10 仍写“单一 builder” | 当前文档 2497 行标题已改“两个构造入口”；R19 只改了 §3.4.1，§4.4 标题当时未同步 |
| R19 D2-12 在 §4.4/§5.1 补了接收侧模块—模块复核 | B-02：§4.3 仍写模块重叠“不再单独查” | 当前文档 2376 行补“接收侧 `KernelInfo::validate` 另按半开区间复核一次”；`os/libs/minix-boot/src/kernel_info.rs:335-370` |
| R19 C-04 把 §2.4.1.3 改成“后两种缺陷形态” | B-05：同段“页对齐退化请求什么都没切”与“任何切除请求至少让涉及的那一页出局”仍直接矛盾 | 当前文档 682 行已拆开两种情形；R19 修了缺陷形态枚举，没有消掉这句冲突 |
| R19 C-03/C-13 收敛了 M6 的 TODO-05 说法 | B-15：§5.4 仍写 M3/M6/M17“当前都写不出可观测断言”，而 M3 有 fail-closed 负控、M6 有宿主断言 | 当前文档 3143 行已改成三个子项；R19 的 M6 改动使这句变成过宽概括 |

结论：R20 的冷读确实抓到了 R19 修复没有传播到的地方，而且这些点 R19 meta 与 RB19 都没抓到（RB19 抓的是 FACT-06 的 `protect.c` 行号与三条 P2/P3，不覆盖这一组）。Detection 判 PASS。

### 1.2 本轮仍未接住的类别

- **范围行内静默漏项（B-54）现在是实锤。** 6 条 ④ 没有修却按 48 条 ④ 汇总，见第八问。R19 meta 已把这一条作为“下一轮动作 2”提出，R20 没有执行：台账仍无 `origin`/`dup-of`，范围行仍是 `READER-A-01..20` 三行。这不是“工作流看不见新类别”，而是“看见了但没有落装置”。
- **运行期缺陷仍为 0 条新增。** 12 条 ① 里没有 UB、竞态、内存破坏或可达 panic 的新证据；`checked_add` 是防未来伪造载荷的加固，不是已发生的故障。B-49 仍 OPEN。
- **“声称已修但 diff 没有”仍无探测器。** §5.2.1 的整条截断是 RB20 已经点名的 P3，主 agent 在提交信息与台账里写“已改”，实际没改；轨迹门只查“有处置行、复验非空”，不比对 diff。

## 二、Precision — PASS（附两条口径问题）

- **D2 七条里只有一条是 D1 方案否决。** `DESIGN-D2-06`（单实现 `BootShim`）记 ③，理由是“与 F-R17-01 同一裁决点、未新增证据”。其余六条（D2-01/02/03/04/05/07）都指向现仓代码或文档：物理地址身份、其余裸加回绕、端点/长度接口面、测试路径假 `'static`、测试快照非配对、入口注释身份歧义。R19 的 12 条 D2 里有 10 条是 D1 否决，R20 降到 1/7，这是本轮精度上最明显的进步。
- **跨角色重复只剩一对。** `READER-B-11`（§4.6 一处说 riscv 用 `kernel_identity_mmio_regions()`，另一处说它“不是 Step 1 的窗口清单”）与 `READER-C-04`（§4.6 补 `identity_device_windows` 包装关系）指向同一句；C-04 的修复（2774 行的括注）同时消掉了 B-11 的事实冲突。按 ID 去重应合并为一条。`READER-A-05`（§4.1 列名把 U-Boot shim 放进“直核面”）与 `READER-B-10`（§3.4 通道形态漏 U-Boot shim）部分重叠，但一个讲列名、一个讲枚举，可分开保留。重复率约 1–2/64（1.6%–3.1%），远低于 R19 的 8/80。
- **两条 ③ 被算进 ④。** `READER-A-07`（§3.4.1 的 `memmap` 长单元格未拆）与 `READER-B-17`（§4.9 表后接 `alloc_pagetable` 反事实）在台账处置文字里都带“（③）”，但所在行处置类写 ④，`metrics.md` 也按 ④ 计。若尊重台账自己的括注，分档应为 ③3 / ④46，而不是 ③1 / ④48。这是计数口径问题，不是面板乱报。
- **误报率本身很低。** 64 条里我能找到的“证据自带错误”是 RB20 的 `§5.2.1` 项：RB20 指出旧串存在，这是对的；主 agent 声称改了但没改，这是修复层的问题，不是 REFUTE 的误报。D2-06 记 ③ 而不是 ②，分类正确。

结论：Precision 判 PASS，附带“两条 ③ 混进 ④”的 CONCERN。

## 三、Design quality — PASS（附“D1 应停跑”结论）

### 3.1 `checked_add` 与单区间负控是真实加固

- `bb6b9fc07` 把六处逻辑点改成 `checked_add`：`vm_handoff::from_memmap`（`os/kernel/src/vm_handoff.rs:275`）、`FreeRangeSet::cut`（`:329`）、`kernel_info` 的表内有序判定（`:292`）、跨表归并（`:314/:319`）、startup 覆盖判定（`:466`）、`memmap_source::dtb_covers_conventional`（`:1471/:1478`）。`e4e26862` 又给 `memmap`/`reserved_regions` 的逐条跨度补了两处（`:377/:388`）。
- `checked_add(...).expect(...)` 在溢出时 panic，boot 路径本来就是 fail-closed，没有恢复路径；这比裸加回绕后让“有序/相交”判定退化为恒真更安全。`validate` 段现在没有裸 `+`（`grep -n 'base\.0 +\|kern_lo +\|start + size'` 无命中）。
- 单区间负控 `test_validate_rejects_memmap_span_overflow` 真实存在并通过：`base = u64::MAX - 1`、`len = 4`，`validate()` 在 `memmap[0] span overflows` 处 panic。实测 `cargo test -p minix-boot --lib -- test_validate_rejects_memmap_span_overflow test_validate_rejects_kernel_span_overflow test_validate_rejects_module_span_overflow` → 3 passed。
- 两条残留，都是 CONCERN 不是 FAIL：
  1. 新加的 13 个调用点里只有 `memmap` 一条有负控；`reserved_regions`、`vm_handoff` 的 `from_memmap`/`cut`、`memmap_source`、startup 覆盖都没有溢出负控。RB20 的 P3 只被部分关闭。
  2. `cut` 在 `checked_add` 之后还有 `hi = end.div_ceil(PAGE_SIZE) * PAGE_SIZE`；`end` 接近 `u64::MAX` 时这次乘法会回绕，但紧接着的 `assert!(hi > lo)` 会 panic，仍是 fail-closed，不是静默放行。若以后有人删掉这条断言，回绕就会复活。

### 3.2 D1 的边际收益已到零，应停跑全量提案

- 事实先校正：**“D1 连续四轮被 D2 判 NOT PASS”不准确。** `panel/round20/design-d2.md` 全文没有 “NOT PASS”，VERDICT 是“接受 D1-01/03、拆分 02/05、拒绝 04、三条需用户裁决”；R17 的 D2 也没有 “NOT PASS”；只有 R18、R19 两份 D2 明写 NOT PASS。
- 但“D1 的方案落地为零”成立：R18、R19、R20 三轮里，被接受并落码的代码项全部来自 D2 对现仓的审计（D2-10/11/12、R20 的 D2-02/05/07），没有一条来自 D1 的决策。R20 的 D1 已缩到 58 行、5 条决策，D2 仍只接受其中两条、拒绝一条、两条拆分。
- 按 `review-evals.md §6B` 第 5 条，这不触发“设计能力未成立”（D2-01/02/04 是结构性判断，不是命名/注释），但继续每轮跑全量 D1 是纯成本。结论：**停跑“每轮全量 D1 提案”，D1 改为架构变更时触发；每轮保留 D2 的现仓审计。** 省下的角色预算给“④ 范围行逐 ID 回读”。

结论：Design quality 判 PASS，附 D1 停跑建议与负控覆盖不足的 CONCERN。

## 四、Independence — CONCERN

- **RB20 有效，且不是橡皮图章。** 它读 `bb6b9fc07`（3390 行、sha `45dd8aa1…`），抓到两条 P2（`LSR` 释义错误、参数“八条”未同步 §4.9 与源码注释）与五条 P3；其中 `LSR` 是 R20 第一版修复自己写进去的，说明回读层能抓修复自伤。RB20 还指出新加的 `checked_add` 点缺负控，这一点被最终补修部分采纳。
- **但 RB20 的 ④ 抽样避开了本轮真正漏掉的 ID。** 它抽了 18 条 ④（A-01/05/09/13/17、B-01/05/11/15/20、C-01..08），没有抽 `READER-A-07/A-10` 与 `READER-B-08/B-14/B-17/B-18`；这六条正是“范围行里有 ID、处置文字里没有”的那一组。RB20 的“16/18 成立”只对它自己的样本成立，不能给 ④ 桶背书。
- **面板冻结面：目标文档没有漂移。** fact、reader-a/b/c、refute、design-d2 六份报告头都写 `03ed48505` / 3387 行 / `56cefc7d…`；`fact.md:8-11` 记录会话期间只有 `loop.md` 被外部进程改动（06:11:54，+10 行），目标文档 `git diff` 为空、sha 未变。这比 R19 的“文档在面板期间被改”好。
- **仍未消除的两点：** `design-d1.md` 没有 sha/HEAD 头（6/7，与 R19 相同）；`loop.md §1` 只写“面板期间工作树只有主 agent 改动”，没有写出“loop.md 在面板期间被改”，与 fact 的取证不一致。B-53 仍 OPEN。
- 同 agent 的共享盲区仍在：面板与 RB 都没有发现 6 条 ④ 漏项，最后靠逐 ID 对账才发现。

结论：Independence 判 CONCERN。RB20 有效，但它的抽样面没有覆盖范围行的空洞。

## 五、Scope discipline — CONCERN

- **F-R20-01..03 不阻塞本轮。** 三条都是 `DESIGN-D2-01/03/04`，登记为 `adjacent（R21）`，`adjacent 阻塞 = 0`；按 `review-evals.md §4.2`，相邻项不阻塞本篇收敛。这一点做对了。
- **但它们的登记位置不对。** 三条都跨 `minix-boot`/`minix-kernel`/`boot-shim` 的接口面（F-R20-03 还要回归 16 个测试内核），属于跨 crate 改造；`review-evals.md §4.0` 要求“确需跨 crate 协调的，在 `OPEN-DEBT.md` 立独立项目（带负责人与截止轮次）”，而它们现在只在 `follow-ups.md` 的 `adjacent` 表里。这不是阻塞问题，是欠账源分类问题。
- **F-R17-01..04 的截止日再次过期。** R19 把四条顺延到 R20；R20 收口时四条仍是原状态：`follow-ups.md:95-98` 写“截止 R20”，`OPEN-DEBT.md:9-10` 仍列 `F-R17-02`/`F-R17-04` 截止 R20，既没有关闭、没有重排、也没有标逾期。这与 R19 meta 点名的 B-47 同形，且现在是第二轮过期。
- **冻结决策没有被重提。** `re-litigation = 0`；`DESIGN-D2-06` 明确写“与 F-R17-01 同一裁决点、未新增证据”，记 ③ 而不是 ②，处理正确。D-013..D-019 没有被重开。

结论：Scope discipline 判 CONCERN。没有扩大责任边界，但跨 crate 项的欠账源与过期截止让范围账比实际干净。

## 六、Teaching quality — CONCERN

抽样 14 条 ④（A 6、B 6、C 2），逐条对照文档原文与缺陷成立条件。**14 条都是真实理解/一致性障碍，纯文风噪声 0 条。** 问题在“发现得准”与“修得完”之间：

| ID | 障碍 | 当前位置 | 判定 |
|---|---|---|---|
| READER-A-01 | 一段里叠框号、依赖序、D 编号、执行序四套坐标 | 74 行已加读序对照 | 真实，已修 |
| READER-A-04 | §2.4.1.1 表“有更早空洞…不断开”过强 | 652 行改“通常不断开；若空洞地址更高仍会断开” | 真实，已修 |
| READER-A-05 | §4.1 第三列“直核面”里放了跨镜像的 U-Boot shim | 2170 行列名改“非 UEFI 路径” | 真实，已修 |
| READER-A-07 | `memmap` 单元格四层括号，主语被定义切断 | 1592 行未拆 | 真实，**未修**（台账记 ③ 却算 ④） |
| READER-A-10 | §2.4.1.2 与 §2.4.1.3 标题都含“alloc_lowest 的参数错误”，且“本节只讲消费者”与标题不符 | 659/681 行两标题仍在 | 真实，**未修** |
| READER-A-13 | DTB/SBI/OVMF/AAVMF/MTRR/LSR 首现未展开 | 38 行补表；LSR 第一版写错，最终改为串口 LSR | 真实，已修（经 RB20 纠错） |
| READER-A-19 | “17–40 号条目”用 1-based，§2.4 通篇 0-based | 705 行标 1-based/0-based | 真实，已修 |
| READER-B-05 | §2.4.1.3 页对齐退化请求“什么都没切”与“任何切除请求至少让涉及页出局”矛盾 | 682 行拆开两种情形 | 真实，已修 |
| READER-B-08 | `pg_mapkernel` 叫“建表最后一步”，五步表里是第 3 步 | 82 行仍写“建表最后一步” | 真实，**未修** |
| READER-B-14 | §5.1 说“状态唯一权威、5.3 不另立状态”，§5.3 仍有“现状与回退判据”第二套状态 | 2990 与 3128 行未变 | 真实，**未修** |
| READER-B-17 | §4.9 表后接 `alloc_pagetable` 静态池反事实，属 §3.8 | 2956 行仍在 | 真实，**未修**（台账记 ③ 却算 ④） |
| READER-B-18 | §3.4.1 与 §4.4 各写一遍“各腿如何产出 memmap/reserved” | 1596 与 2534 行两处仍在 | 真实，**未修** |
| READER-B-20 | 跨镜像协议段写“因此 `x0` 只需传 arena 基址”，x86 用 `rbx`、riscv U-Boot 用 `a0` | 1697 行改三腿寄存器 | 真实，已修 |
| READER-C-01 | §2.6.4 把重建活动根写成“等价/无害”，与 §3.7“清零活动根会页错误”相反 | 1123 行补“靠 4MB TLB 覆盖撑窗口，这是时序脆弱，不是无害” | 真实，已修 |

- **真实障碍 14/14，纯文风噪声 0/14（0%）**，远低于 20% 的过拟合警报线。读者角色确实在发现理解障碍，而不是报措辞偏好。
- **修复完成度 8/14（57%）。** 未修的 6 条都是结构/组织级问题（长单元格、重复标题、步骤命名、双状态表、章节边界、重复叙述），修起来比补一句定义贵；它们被范围行“平均”掉了。
- 教学侧的结论：面板质量 PASS，记账与收口 CONCERN。

## 七、Cost — CONCERN

| 指标 | R17 | R18 | R19 | R20 |
|---|---:|---:|---:|---:|
| 角色报告 | 7 | 7 | 7 | 7 |
| 原始发现 | 87 | 98 | 80 | 64 |
| ① 本轮修 | 26 | 21 | 13 | 12 |
| ④ 本轮修（声称 / 实核） | 48 / — | 56 / — | 53 / — | **48 / ≤42** |
| ③ 非缺陷/否决 | 10 | 21 | 14 | 1（台账文字另含 2 条 ③） |
| 独立回读 | 无 | RB18 | RB19 | RB20 |
| 可观测时间线 | 约 2.5 h | 86 min | 33 min | 约 37 min |
| `metrics.md` 自述耗时 | 约 2.5 h | 约 4 h | 约 4.5 h | 约 40 min |

- 发现总数连续第三轮下降（98 → 80 → 64），可观测耗时稳定在 35–40 分钟；R19 的“自述 4.5 小时 vs 可观测 33 分钟”在本轮消失，读数可信度改善。
- 单位产出：12 条 ① 里约 7 条是 R19 修复残留，真正的新外部代码/文档缺陷约 5 条；64 条里 48 条是读者项，其中 6 条没修。按“实际闭环的 ①+④”算，约 54 条 / 37 分钟，比 R19 的 66 条 / 33 分钟略差。
- token 连续第四轮“未采集”；`loop.md §1` 仍无并发数与各角色起止时间。仪表缺口与 R18/R19 相同。
- D1 轻量版省下的篇幅没有换成产出：58 行、5 条决策、0 条落码。预算应转给 D2 与 RB 的 ④ 全量回读。

结论：Cost 判 CONCERN。总量在降，但收口质量没有同步提高，D1 仍是净成本。

## 八、Trace integrity — FAIL

### 8.1 删除探针（副本在 `/tmp/meta-r20-probe/`，仓库只读）

命令：`python3 tools/eval-trace-check.py /tmp/meta-r20-probe/<p> --panel-dir /home/xzhao/github/minix-rs/tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round20`

| 探针 | 操作 | 读数 | 期望 | 判定 |
|---|---|---|---|---|
| 基线 | 原样 | `PASS 8 / WARN 1 / FAIL 0`；`panel: 64 条`；`metrics: 64`；`ledger: 1266 条处置` | PASS | PASS |
| A | 删 `defect-ledger.md` 的整段 `## R20 面板轮`（1400 行至文件末） | `FAIL coverage: 面板发现没有处置行`，列出 64 个 ID；exit 1 | FAIL | FAIL 成立 |
| B | 删 `loop.md` 的整段 `## R20 面板轮`（1001 行至文件末） | `FAIL loop-sections: loop.md 缺少 R20 面板轮小节`；exit 1 | FAIL | FAIL 成立 |
| C | 把 R20 指标改成 `发现总数: 999` / `①1 ②0 ③1 ④1 ⑤0` | `FAIL metrics: 发现总数 999 与五项处置之和 3 不一致`；exit 1 | FAIL | FAIL 成立 |

三组探针全部转红，B-51 的 `CLOSED` 在本轮有实测支撑。这一点比 R19 初评时好。

### 8.2 计数与逐 ID 复算

- 面板解析：FACT 3（FACT-01 主条 + FACT-02/03 次级）、READER-A 20、READER-B 20、READER-C 8、REFUTE 6、DESIGN-D2 7 = **64**。
- 台账按行展开：① = FACT-01/02/03（3）+ REFUTE-01..06（6）+ DESIGN-D2-02/05/07（3）= **12**；② = DESIGN-D2-01/03/04 = **3**；③ = DESIGN-D2-06 = **1**；④ = READER-A/B/C 三行 = **48**；⑤ = **0**。12+3+1+48 = 64，`metrics.md` 自洽。
- **但逐 ID 读处置文字，④ 不可复算：**
  - `READER-A-07`：A 行写“§3.4.1 memmap 长单元格未拆（③）”。既然是 ③，就不该计入 ④；当前文档 1592 行的长单元格仍在。
  - `READER-B-17`：B 行写“§4.9/§3.4 重复与反事实分离（③）”。同理；当前文档 2956 行的 `alloc_pagetable` 反事实仍在。
  - `READER-A-10`：A 行没有对应文字；当前文档 659/681 行两个标题仍重复，缺陷成立。
  - `READER-B-08`：B 行没有对应文字；当前文档 82 行仍写“建表最后一步 `pg_mapkernel()`”。
  - `READER-B-14`：B 行没有对应文字；当前文档 2990 行仍说“5.3 不另立状态口径”，3128 行仍有“现状与回退判据”列。
  - `READER-B-18`：B 行没有对应文字；当前文档 1596 与 2534 行仍各写一遍产出来源。
- 结论：按台账自己的括注，分档至少应为 ③3 / ④46；再扣掉 4 条无处置文字的 ④，实际“本轮修”的 ④ 至多 **42**。`loop.md §7` 的“G1–G4 修复后为 0”不成立，`metrics.md` 的 ④48 是“赋值数”不是“完成数”。

### 8.3 RB20 第七项：声称已修，实际未改

- RB20 的七项是：`LSR` 释义、参数八条同步、§7.2 补 `pm/misc.c`、§4.6 限 1024/256、§3.4.3 三腿寄存器、§5.2.1 “整条截断”改“整条丢弃”、新增单区间溢出负控。
- 前五项与第七项都在最终树成立：38 行 LSR 已改串口 LSR（0x3FD）；2940 行“八条”与 `boot_params.rs:8` “eight”；3336 行有 `pm/misc.c`；2776 行“UEFI/U-Boot 腿 1024（直核面 256）”；1697 行“x86_64 `rbx`、aarch64 `x0`、riscv64 U-Boot shim `a0`”；`test_validate_rejects_memmap_span_overflow` 通过。
- **第六项没有改。** 文档 3071 行仍写“重复键首条胜出、整条截断”；`git log -S'整条截断'` 只命中 `b860dd2cc`，`e4e26862` 的文档 diff 没有这一行（该段是 diff 的上下文，不是新增或删除）。`defect-ledger.md:1436` 写“① 改‘整条丢弃’”，`loop.md §5` 写“§5.2.1 改整条丢弃”，`e4e26862` 的提交信息也写“§5.2.1 改‘整条丢弃’”。三处声称与一次实际缺失。
- 这属于 B-50“半修记整修”的升级形态：不是“改了一部分”，而是“没改却写已改”。按 `review-evals.md §八` 的处置行要求，这条应记 ⑤ 或重新打开 ①，不能留在“已补修”里。

### 8.4 其余读数

- `BASELINE.txt` 的 R20 四项与提交/工作树逐项吻合（`e4e26862` / `0314e194` / 3390 / `03ed48505` / `56cefc7d` / 3387）。PASS。
- 宿主测试全部重放命中：`minix-boot` 189、`minix-elf` 23、`boot-shim --lib` 135、`ebs_boundary_audit` 4、`minix-kernel --features mock` 868（860 passed + 8 ignored）、`minix-arch --features "x86_64 runtime-window"` 287。逐文件 `#[test]` 计数：boot-shim 22+11+12+44+17+15+13+1=135；minix-boot 26+8+8+73+2+10+21+29+2+5+5=189。PASS。
- 六道文档门在最终树上全绿（anchor 108/108、code-map 46/46、ref 通过、承载门通过、structure 0 问题、style 全量 error 0）。但 `fact.md:309-321` 记录 `doc-style-lint.sh --diff HEAD` 在工作树上 exit=1，唯一命中是 `loop.md:1001` 的 SL-4（评审台账标题里的日期行，不是受审文档）。`loop.md §5` 写“文档六门全绿”在“只算文档”的口径下成立，在“本轮变更全绿”的口径下不成立；这是 P3 措辞问题，记在这里以免下一轮把非零退出当门故障。
- QEMU 三门在 `loop.md §4/§5` 写 PASS，但 RB20 明确“本机只读回读未跑”，本报告也没有重放。按 L1 证据规则，这一项应标“未验证”，不能算 G6 的独立证据。

结论：Trace integrity 判 FAIL。判据不是“探针没红”（三组都红了），而是“④ 的 48 条里有 6 条没修却记已修”与“RB20 第七项没改却写已改”。

## 九、本轮最值得、但工作流仍看不见的一件事

**台账的处置单位是“行”，不是“发现 ID”；行内既不要求逐 ID 的文本锚点，也不要求 hunk 回指。** 于是两类错误同时不可见：

1. **静默漏项。** `READER-A-01..20`、`READER-B-01..20`、`READER-C-01..08` 三行覆盖 48 个 ID，但处置文字只列了 42 条左右；A-10、B-08、B-14、B-18 没有任何文字，A-07、B-17 写“（③）”却按 ④ 计。三组删除探针只校验“面板 ID 在台账有行”，不校验“每个 ID 在行内有处置文字或 ③ 理由”，所以全绿。
2. **假修复。** `§5.2.1` 的“整条截断”在 RB20 里是 P3，主 agent 在台账、loop、提交信息三处写“已改”，但 diff 里没有。现有门只查“① 行复验非空”，不查“声称的改动是否有对应 hunk”。

R19 meta 的“下一轮动作 2”（范围行展开为逐 ID 行、台账加 `origin`/`dup-of`）与“动作 4”（修复 diff 生成 changed-sentences 清单，作为 RB/FACT 的固定输入）都没有执行；`BLIND-SPOTS.md` 也没有收 R19 meta 提议的 B-54/B-55。R20 用 6 条漏项和 1 条假修复证明：这不是“可能的风险”，是已经发生的账实不符。

可执行的仪器只有一条：**每条发现 ID 必须绑定一个可重放的落点**——文本句子（行号或原句片段）、代码 hunk、或 ③ 的反证句；`eval-trace-check.py` 对范围行展开后逐 ID 检查，缺一项即 FAIL。`review-evals.md §八` 第 13/14 项已经对 ① 提了 hunk 回指，把它扩到 ④ 与 RB 补修行即可，不需要新装置。

## 抽样一致性

- 抽样 28 条：12 条 ① 全覆盖，14 条 ④（A 6 / B 6 / C 2），2 条 ②/③（D2-01、D2-06）；占 64 条的 43.8%，④ 抽样占 48 条的 29.2%，均高于 20%。
- 判据：不看“旧串是否消失”，看“缺陷成立条件是否消失”或“声称的修复是否有对应文本/hunk”。

| # | ID | 角色 | 处置 | 复判 | 判据/证据 |
|---|---|---|---|---|---|
| 1 | FACT-01 | fact | ① | PASS | 3336 行有“只列主干文件”声明并列 14 组小文件；`pm/misc.c` 已在枚举；正文 422 行的引用有去处 |
| 2 | FACT-02 | fact | ① | PASS | 1999 行块头“英文 SAFETY 注释有节略，原文见源码”；对照 `os/arch/src/arch/pt_alloc.rs:105-107` |
| 3 | FACT-03 | fact | ① | PASS | 1472 行块头“一处英文注释译为中文”；对照 `os/boot-shim/src/uefi_helpers.rs:1154` |
| 4 | REFUTE-01 | refute | ① | PASS | 1650 行“共八条”逐条列；`boot_params.rs:8` 写 eight；8 条测试逐条对应 |
| 5 | REFUTE-02 | refute | ① | PASS | 437 行补“后缀可致伪记录”；C 侧 `pre_init.c:47-59` 的删除扫描支持 |
| 6 | REFUTE-03 | refute | ① | PASS | 1608 行 `user_end` 移出“上浮为常量”；1624 行 release/version 改“构建身份” |
| 7 | REFUTE-04 | refute | ① | PASS | 2777 行补 `kern_size % 1GiB == 0`；`os/kernel/src/boot_policy.rs:90-95` |
| 8 | REFUTE-05 | refute | ① | PASS | 2776 行“UEFI/U-Boot 腿 1024（直核面 256）”；`bootface.rs:135`、`bootface_a64.rs:99` |
| 9 | REFUTE-06 | refute | ① | PASS | 2975 行 M6“合成窗口/1 GiB 块配置下验证策略函数，不跑生产 Step 1”；`lib.rs:5462-5498` |
| 10 | DESIGN-D2-02 | design-d2 | ① | PASS | `vm_handoff.rs:275/329`、`kernel_info.rs:292/314/319/466`、`memmap_source.rs:1471/1478` 均 `checked_add`；`validate` 段无裸加 |
| 11 | DESIGN-D2-05 | design-d2 | ① | PASS | `uefi_helpers.rs:1129-1138` 注释改“best-effort、不与 build_memmaps 同 MapKey”；与 §3.1 一致 |
| 12 | DESIGN-D2-07 | design-d2 | ① | PASS | `main.rs:32-36` 改“serialize BootHandoffV1（envelope + partitions）”；与 §4.1 一致 |
| 13 | READER-A-01 | reader-a | ④ | PASS | 74 行四坐标 + 读序对照表 |
| 14 | READER-A-04 | reader-a | ④ | PASS | 652 行“通常不断开；若空洞 h 的地址反而高于槽 m，仍会断开” |
| 15 | READER-A-05 | reader-a | ④ | PASS | 2170 行列名“非 UEFI 路径（自引导/直核面 + U-Boot shim；按‘谁建根’归类）” |
| 16 | READER-A-07 | reader-a | ④ | **FAIL** | 1592 行长单元格未拆；台账记“（③）”，未修却计入 ④ |
| 17 | READER-A-10 | reader-a | ④ | **FAIL** | 659/681 行两个标题仍都含“alloc_lowest 的参数错误”；2.4.1.2 的“本节只讲消费者”仍与标题不符 |
| 18 | READER-A-13 | reader-a | ④ | PASS | 38 行缩写表；LSR 第一版写错，最终改为串口 LSR（0x3FD），与 3170 行及 `lib.rs:341-343` 一致 |
| 19 | READER-A-19 | reader-a | ④ | PASS | 705 行“17–40 号条目（按 C 的 1-based 编号；对应 0-based 下标 17..=39）” |
| 20 | READER-B-05 | reader-b | ④ | PASS | 682 行拆开“页对齐退化请求两个取整都不生效”与“退化且未对齐时漏切” |
| 21 | READER-B-08 | reader-b | ④ | **FAIL** | 82 行仍写“建表最后一步 `pg_mapkernel()`”；§2.6.1 五步表里它是第 3 步 |
| 22 | READER-B-14 | reader-b | ④ | **FAIL** | 2990 行“5.3 不另立状态口径”，3128 行仍有“现状与回退判据”第二套状态 |
| 23 | READER-B-17 | reader-b | ④ | **FAIL** | 2956 行 `alloc_pagetable` 静态池反事实仍在 §4.9 表后；台账记“（③）”却计入 ④ |
| 24 | READER-B-18 | reader-b | ④ | **FAIL** | 1596 行与 2534 行仍各写一遍各腿 memmap/reserved 产出 |
| 25 | READER-B-20 | reader-b | ④ | PASS | 1697 行“x86_64 `rbx`、aarch64 `x0`、riscv64 U-Boot shim `a0`” |
| 26 | READER-C-01 | reader-c | ④ | PASS | 1123 行补“清表与回填之间只执行已被 4MB TLB 项覆盖的代码/栈……这是时序脆弱，不是无害” |
| 27 | DESIGN-D2-01 | design-d2 | ② | PASS | 物理地址身份问题真实（`boot_shim.rs:26-36`、`lib.rs:419-423`）；R21 有档期。登记位置问题见第五问 |
| 28 | DESIGN-D2-06 | design-d2 | ③ | PASS | 单实现 trait 事实成立（1 impl / 15 引用 / 0 dyn），与 F-R17-01 同一裁决点、未新增证据，记 ③ 正确 |

- 严格一致：**22/28 = 78.6%**（6 条 FAIL，全部是“④ 未修却记已修/已算”）；实质一致（把 6 条降为 CONCERN 而不算 FAIL）仍是 22/28。
- 按 `review-evals.md §5.4/§9` 的“低于 90% 写 NOT PASS”，本轮判 NOT PASS。
- 抽样偏向：12 条 ① 全查；④ 刻意选了 A-07、A-10、B-08、B-14、B-17、B-18 六个“范围行里没有处置文字或带 ③ 括注”的 ID，这是本轮最可疑的记账点。

## 半修／整修清单

| 项 | 声称 | 实际 | 判定 |
|---|---|---|---|
| `§5.2.1` “整条截断” | 台账、loop、提交信息三处写“改‘整条丢弃’” | 3071 行仍“整条截断”；`e4e26862` 文档 diff 无这一段 | **未修记已修**（最重） |
| `READER-A-07` 长单元格 | A 行写“未拆（③）” | 1592 行未拆；仍按 ④ 计 | 分类错 + 未修 |
| `READER-B-17` §4.9 反事实 | B 行写“重复与反事实分离（③）” | 2956 行仍在；仍按 ④ 计 | 分类错 + 未修 |
| `READER-A-10` 标题重复 | A 行无对应文字 | 659/681 行两标题仍重复；2.4.1.2 的“本节只讲消费者”仍与标题不符 | **未修，静默漏项** |
| `READER-B-08` 步骤命名 | B 行无对应文字 | 82 行仍写“建表最后一步 `pg_mapkernel()`” | **未修，静默漏项** |
| `READER-B-14` 双状态表 | B 行无对应文字 | 2990 行仍“5.3 不另立状态口径”，3128 行仍“现状与回退判据” | **未修，静默漏项** |
| `READER-B-18` 重复叙述 | B 行无对应文字 | 1596 与 2534 行仍各写一遍 | **未修，静默漏项** |
| `DESIGN-D2-02` 负控 | 行写“minix-boot 189”，只新增 `memmap` 单区间负控 | 13 个 `checked_add` 调用点，只有 `memmap` 一条负控；`reserved`、`from_memmap`、`cut`、`memmap_source`、startup 都没有 | **半修（加固真、可观测性不全）** |
| RB20 `§4.6` 1024 页 | RB20 判 P3 | 2776 行已改 1024/256 | 整修 |
| RB20 `§7.2` `pm/misc.c` | RB20 判 P3 | 3336 行已补 | 整修 |
| RB20 `§3.4.3` `x0` | RB20 判 P3 | 1697 行已改三腿寄存器 | 整修 |
| RB20 新增 `checked_add` 缺负控 | RB20 判 P3 | 新增 `test_validate_rejects_memmap_span_overflow`，只覆盖 `memmap` | 半修 |
| QEMU 三门 | `loop §4/§5` 写 PASS | RB20 未重放，本报告未重放；L1 证据缺失 | **未验证** |
| `F-R17-01..04` 截止 R20 | R19 已顺延到 R20 | R20 收口仍是原状态；`OPEN-DEBT.md` 仍写截止 R20 | **截止日第二轮过期** |
| `DECISIONS.md` 判决账 | R19 meta 点名 B-44 | 仍停在 D-023（R18）；没有 RB19/RB20 的 NOT PASS 行，也没有 D1 不采纳行 | **未落账** |
| `BLIND-SPOTS.md` B-54/B-55 | R19 meta 提议新增 | 账本仍停在 B-53；范围行漏项与分档不可复算没有编号 | **未落盘** |
| `loop.md §2` “D2 判 NOT PASS” | 本轮 §2 写“D2 判 NOT PASS” | `design-d2.md` 全文没有 “NOT PASS”；实际是接受 2、拆分 2、拒绝 1、3 条待裁决 | 措辞过宽（P3） |
| `loop.md §7` “G1–G4 = 0” | 写“G1–G4 修复后为 0” | G4 至少 6 条 ④ 未修 | **状态不实** |
| `BASELINE.txt` | R20 四项 | `e4e26862` / `0314e194` / 3390 / `03ed48505` / `56cefc7d` / 3387 逐项吻合 | 整修 |
| 宿主测试 | 189 / 135 / 4 / 868 / 287 | 全部实跑命中；逐文件计数闭合 | 整修 |

## 给下一轮的具体动作（≤5 条）

1. **把三条 ④ 范围行展开为逐 ID 行，并给每个 ID 一个落点。** `READER-A-07/A-10`、`READER-B-08/B-14/B-17/B-18` 现在要么写“（③）”却按 ④ 计，要么完全没有处置文字；每条必须给“文本原句/行号”“代码 hunk”或“③ 的反证句”。`metrics.md` 同时报“原始 64 / 去重 63 / 实际修 ①12+④42 / 未修 6”四个数。这是 R19 meta 动作 2 的未执行项，本轮 6 条漏项是它的直接后果。
2. **修 `§5.2.1` 的“整条截断”并做一次“声称 vs diff”对账。** 把 3071 行改“整条丢弃”；把 RB20 七项逐项用 `git show e4e26862` 对一遍，`loop.md §5` 改写成“六项已改、一项未改（§5.2.1）”，不要再写“七项已补修”。下一轮 RB 必须按 diff 判，不按台账文字判。
3. **停跑全量 D1，保留 D2。** R18–R20 三轮 D1 落码 0 条，D2 每轮都产出真实代码项；D1 改为架构变更时触发（或每三篇一次），省下的角色预算给“④ 全量回读 + 修复 diff 回读”。同时修正“连续四轮 NOT PASS”的说法：R20 的 D2 没有 NOT PASS。
4. **把 F-R20-01..03 从 `follow-ups.md` 的 adjacent 改登记到 `OPEN-DEBT.md`。** 三条都跨 `minix-boot`/`minix-kernel`/`boot-shim` 与 16 个测试内核，按 `review-evals.md §4.0` 必须有负责人与截止轮次（R21）；同时把 `F-R17-01..04` 的过期截止改到 R21 或明确标“逾期”，不要让 OPEN-DEBT 继续停在 R20。
5. **补两处判决与盲区。** `DECISIONS.md` 加 RB20 的 `reviewer_verdict: NOT PASS` 行与 D1 不采纳行（八字段，`user_decision` 与 `reviewer_verdict` 分列）；`BLIND-SPOTS.md` 落 B-54（范围行把多 ID 压成单 token，允许静默漏项）与 B-55（分档指标不可由台账逐 ID 复算），并把 B-53 的 design-d1 缺 sha/HEAD 补上。

## BLIND-SPOT-DELTA

| 盲区 | 状态 | R20 证据 |
|---|---|---|
| B-51 轨迹门轮次作用域 | **CLOSED（实测支撑）** | 三组探针全 FAIL：删 R20 台账、删 R20 loop 段、篡改 R20 指标；exit 均为 1 |
| B-53 面板读取状态无机械锚 | **PARTIAL** | 6/7 报告头有 sha/HEAD；目标文档稳定在 `56cefc7d`；design-d1 无头；`loop.md` 在面板期间被改（fact 记录） |
| B-54 范围行把多 ID 压成单 token | **新增 OPEN（R19 meta 已提议，未落盘）** | 6 条 ④ 无处置文字/带 ③ 括注却计入 ④：A-07/A-10/B-08/B-14/B-17/B-18 |
| B-55 分档指标不可由台账复算 | **新增 OPEN（R19 meta 已提议，未落盘）** | 台账文字 ③3/④46，`metrics.md` ③1/④48；④ 实际修 ≤42 |
| B-50 半修记整修 | **REGRESSED（新形态）** | `§5.2.1` 没改却写“已改”；RB20 抓到旧串，主 agent 在台账/loop/提交信息三处声称修复 |
| B-52 轮次来源缺失（`origin`/`dup-of`） | **仍 OPEN** | 台账仍无 `origin`/`dup-of`；B-11/C-04 重复未合并 |
| B-47 欠账源自身不收敛 | **仍 OPEN（第二轮）** | `F-R17-01..04` 截止 R20 再次过期；`OPEN-DEBT.md` 仍列 F-R17-02/04 截止 R20 |
| B-44 判决不落账 | **仍 OPEN** | `DECISIONS.md` 停在 D-023；无 RB19/RB20 判决行 |
| B-49 严重度地板为零 | **仍 OPEN** | 12 条 ① 无运行期缺陷；`checked_add` 是加固不是已发生故障 |
| B-34 代码改动后文档引文未回读 | **DETECTED（继续）** | REFUTE-01（五→八）、REFUTE-05（§4.6 1024）、B-02（§4.3 模块复核） |
| B-03 跨节矛盾 | **DETECTED（继续）** | B-01 单一 builder vs 两个入口、B-03 来源不进交接、B-05 取整矛盾、B-11 §4.6 窗口归属 |
| B-09 门绿但事错 | **仍 OPEN** | `doc-style-lint --diff` 在 `loop.md` exit=1；QEMU 三门未独立重放 |
| B-10 重复已裁决问题 | **OK** | `re-litigation 0`；D2-06 明确同一裁决点、未新增证据 |
| B-11 范围蔓延 | **PARTIAL** | `adjacent 阻塞 0`；但 F-R20-01..03 是跨 crate，登记在 follow-ups 而非 OPEN-DEBT |
| B-01/B-02 设计死代码/翻译残留 | **DETECTED（守住）** | D2-01（物理地址身份）、D2-02（其余裸加）、D2-04（测试路径假 `'static`） |
| B-35 合并读者角色 | **OK** | A/B/C 齐全，20/20/8 |
| B-46 表结构/跨文档链接门 | **CLOSED** | `doc-structure-check.py` 0 问题 |
| B-05 假反事实/前提过强 | **本轮无新证据** | 没有新的反事实前提错误；C-14 的旧项未重开 |
| B-13 硬件机制泄漏 | **本轮无新证据** | D2 的抽象边界维度只到物理地址类型，没有新的 CR3/PTE 泄漏 |
| B-06/B-07/B-12/B-14/B-15/B-16/B-17/B-18/B-19..B-30/B-32/B-33/B-36..B-39/B-41/B-42/B-45/B-48 | **本轮无新证据，未重估** | 不在 R20 射程内 |

**新增候选盲区（建议下一轮 L6.5 落盘）：**

- **B-54 范围行把多 ID 压成单 token，允许静默漏项。** 本轮实锤 6 条：A-07、A-10、B-08、B-14、B-17、B-18。期望探测器：范围行必须展开为逐 ID 行；每个 ID 要么有文本锚点/代码 hunk，要么有 ③ 的反证句；`eval-trace-check.py` 拒绝未展开且未逐 ID 给落点的范围行。
- **B-55 分档指标不可由台账逐 ID 复算。** 本轮 ③/④ 差 2（A-07、B-17 的 ③ 括注），④ 实际修 ≤42 而 metrics 写 48。期望探测器：`metrics.md` 由台账逐 ID 生成，轨迹门比对逐类归属，不只比对“总数 = 五项之和”。
- **B-56 声称修复无 diff 回指。** §5.2.1 是“没改却写已改”。期望探测器：每条 ①/RB 补修行必须给“文件:行号 + 变更前串 → 变更后串”，RB 与 meta 用 `git show` 对 hunk，缺 hunk 即 FAIL。

## 总评与局限

- 本轮是检测层与精度层的进步：R19 的五处修复残留与两处同段漏检被抓到，D2 的 D1 否决从 10/12 降到 1/7，跨角色重复从 8 条降到 1 条，三组删除探针全红，宿主测试与文档 sha/行数全部命中。`IMPROVED` 成立。
- 不通过的是记账层：④ 的 48 条里至少 6 条没修却记已修；RB20 第七项没改却写已改；分档 ③/④ 与台账自己的括注差 2；“G1–G4 = 0”因此不成立。`NOT PASS` 也成立。
- 这一轮最值得记的不是“又发现了 64 条”，而是“范围行 + 无 hunk 回指”这个老洞在 R19 meta 已经点名、R20 仍未堵，结果同时生产了 6 条漏项和 1 条假修复。下一轮如果只修 §5.2.1 而不展开范围行，同样的漏项会以新 ID 再来一次。
- 本报告是**同 agent 验证 + 已重放命令**；六门中我重跑了 anchor/code-map/ref/density/structure/style 全量与五套宿主测试，没有重跑 QEMU；QEMU 的“三门 PASS”按主 agent 自述处理，不构成独立确认。探针副本在 `/tmp/meta-r20-probe/`，未修改仓库其他文件。

VERDICT: NOT PASS
