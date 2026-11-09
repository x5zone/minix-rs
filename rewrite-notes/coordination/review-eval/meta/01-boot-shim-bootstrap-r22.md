# D01 R22 meta-review（独立冷启动，逐条回源）

> 只读审计；唯一写入本文件。验证方式：**同 agent 验证 + 已重放命令**（单 session，无跨工具复核）。
> 基线：`BASELINE.txt` 的 `r22_fix_head=e68190a4`；文档 3394 行、sha256 `1cf3351b…`；HEAD `ed8db4f67`。
> 面板基线：`f53ad998` / 文档 3392 行 / sha256 `c531add4…`；面板七份在 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round22/`。

## 0. 基线与读数核对

| 项 | 要求 | 实测 | 判定 |
|---|---|---|---|
| BASELINE 指向 | `e68190a4` | `git show -s e68190a4` = `e68190a4fd35…`，2026-11-09 12:05:54 | PASS |
| 文档 sha/行数 | `1cf3351b…` / 3394 | `sha256sum` = `1cf3351bee7f37d0bc7566795288bd9e245abec1d03f658e2b531329832ddca8`；`wc -l` = 3394 | PASS |
| 工作树一致性 | 文档与审计测试无未提交改动 | `git status --porcelain` 两项均为空 | PASS |
| QEMU 日志 | `logs/round22-*.log` | 三份：`round22-test-shim-bootmarks-aarch64.sh.log`、`round22-test-shim-bootmarks-x86.sh.log`、`round22-test-riscv64-uboot-shim.sh.log`；均含 `TEST_RESULT: PASS` / `RESULT: PASS` | PASS |
| 宿主计数 | 190/135/5/868/287 | `cargo test -p minix-boot` 190 passed；`-p boot-shim --features test-all --lib` 135 passed；`--test ebs_boundary_audit` 5 passed；`-p minix-kernel --features mock --lib` 868 = 860 passed + 8 ignored；`-p minix-arch --features "x86_64 runtime-window" --lib` 287 passed | PASS |
| 六门 | anchor/map/ref/structure/style/density 绿 | anchor `105/105`；map `46/46`；ref/structure/style/density 均 exit 0 | PASS（但 loop §5 写 anchor `107/107`，见问 8） |

**总评：IMPROVED / PASS（六次复核后）。** 初次审计时台账把 50 条 ④ 全记“本轮修”、18 条原句未动，指标分档不可复算、篡改分档探针仍 PASS；经六次补修，50 条 ④ 已全修，R21/R22 两套计数与 67 分档可复算，真实 run-dir、A–E 探针与 ④ 证据闸全 PASS。Detection/Design/Scope/Cost 仍 CONCERN，D1 停跑与 B-52 origin 机制仍是后续观察项，但不属本轮阻断。

> **五次复核后（`de2a982f8`）：** R15 三行已改 ③，真实 run-dir PASS 8/0/0；A-05、A-17、A-20、B-19 已全修。B-10 的探针读数已移入 §5.2.3，但 §3.4.4 的“下面这枚探针/见探针读数后的三层结论”失去指向，§3.4.4 没有 §5.2.3 指针（唯一指针在 §5.2.3 里自指），仍算半修并形成新的悬空引用。另：R22 A 行仍写“A-05 按部分处理”，与 de2a 已修相反；B 行没有写 B-10 半修，用户所称“A/B 行已按最终状态改写”并未发生。Teaching 仍 FAIL、Trace 记 CONCERN，总体仍判 **NOT PASS**。详见文末《五次复核后记》。

---

## 1. Detection：CONCERN

**P-FACT 拆分补上了 R21 的 MISSING 空档，但不是全文事实普查；R21 的一条点名漏检仍未被接住。**

- R21 是 `P-FACT MISSING`；R22 拆成 P-FACT-A（机械）与 P-FACT-B（引文/合同），交付 `FACT-A-01..03` + `FACT-B-01..02` = 5 条，其中 4 条 ①、1 条 ③。覆盖了 R21 的机械面空档：868 命令缺 `--lib`（`FACT-A-01`）、`新 impl BootShim` 通则与现仓单实现冲突（`FACT-A-02`）、四个 `path:Type::method` 锚点逃逸（`FACT-A-03`）、§4.2 引文块漏记 arena 闸/扩容分支/dumpdtb（`FACT-B-01`）、块头虚构“省略号”（`FACT-B-02`）。这比 R21 用 REFUTE 代替 fact 是实质进步。
- 但 fact 面仍窄：`FACT-A` 自述 brief 里的 287 在文档不存在、判 N/A；`FACT-B` 只审引文与合同，不审全文断言。R21 meta §1 点名的 `os/libs/minix-boot/src/kernel_info.rs:245-246`（“Hand-built test kernels … use `Self::validate`”）与文档 §3.4.1“手工构造的测试内核走 `validate_before_tables` 或直接 `arch_boot_impl`”相反，**R22 的 FACT-A/FACT-B/D2 都没有再报，R22 也没有修**（`kernel_info.rs` 当前仍是旧注释）。这是连续两轮漏检。
- R22 面板接住的 R21 漏检/半修：`FACT-A-02` 对应 R21 D2-01 只改代码注释、文档 L1489“测试形态成立”未改；`REFUTE-01` 对应 R21 REFUTE-01 核心成立但未落码；`REFUTE-02` 的 `board=` 全称反例与 `REFUTE-03` 的 `nr_tasks` 载体错配是新事实发现，不是 R21 已点名项。
- R22 修复自伤（由 RB22 接住，e68190a4 补修）：`e93b99fe` 的 §3.4.1 把 `EBS` 写成 `EAB`；`REFUTE-01` 新增第 5 条审计测试但 §5.2.1 逐文件表仍写 4；`e68190a4` 又把 §7.5 `overlaps` 行的两个符号链接删掉，anchor 从 107 降到 105，loop §5 却仍写 107。RB22 是这轮 detection 的有效补位。
- 结论：FACT 拆分补上了“没有 fact 角色”的空档，但补的是改动面，不是 R21 meta 已点名的源码注释与全文事实；判 CONCERN。

## 2. Precision：FAIL → CONCERN → PASS（四次复核后仍 PASS）

**台账第一张表能复算到 ①9/②3/③1/④50；loop §3 不能；③ 与 ② 的分档本身有错；R21 的历史指标被 repair commit 改成了 R22 的数。**

- 按 R22 台账第一张表逐 ID 展开：① = `FACT-A-01`、`FACT-A-02`、`FACT-B-01`、`FACT-B-02`、`REFUTE-01`、`REFUTE-02`、`REFUTE-03`、`DESIGN-D2-01`、`DESIGN-D2-05` = 9；② = `DESIGN-D2-02/03/04` = 3；③ = `FACT-A-03` = 1；④ = `READER-A-01..20` + `READER-B-01..20` + `READER-C-01..10` = 50。合计 63。`metrics.md` R22 与 loop §6 均为 9/3/1/50，**这一组自洽**。
- `loop.md` R22 §3 仍写“① 14；④ 43；② 3；③ 3”（第 1127 行），与同文件 §6（第 1145 行）和 `metrics.md` 冲突。①14 是 R21 旧口径的拷贝，不是 R22 的逐 ID 聚合。
- ③=1 的 `FACT-A-03` 不是误报：四个锚点确实逃逸，符号存在但 `anchor-resolve` 文法不认；应归 ②（R23 工具候选）或 ⑤，不能记 ③。若改正，R22 应为 ①9 / ②4 / ③0 / ④50（或把工具项单列 ⑤）。
- ②=3 是 `DESIGN-D2-02/03/04` 再次报告 `F-R20-01/03/04`——这三项在 R20/R21 已登记为跨 crate 欠账；按 `review-evals.md` §4.3 属“命中已有处置且无新证据”，应记 `dup-of` 或 `re-litigation`，不是 3 条新 ②。`metrics.md` 的 `re-litigation: 0` 因此不成立。
- 台账第二张表把 `CLAIM-01..07` 重列一遍（`CLAIM-01..03` 是 `REFUTE-01..03` 的别名，`CLAIM-04..07` 是“反证未成立”的主张），又重列 `DESIGN-D2-03/04/05`。按唯一 ID 计为 67；`metrics.md` 只算 63；`eval-trace-check.py` 报“面板报告解析到 67 条发现”但判 coverage PASS。按 §8 第 10 条，P-REFUTE 主张应进独立的“## P-REFUTE 附加”表，不进缺陷计数；当前第二张表没有标题、没有 `dup-of`，是重复记账。
- 历史指标被改写：`e68190a4` 把 `metrics.md` 的 R21 块从 `①14/②3/③3/④43` 改成 `①9/②3/③1/④50`，并把 `loop.md` R21 §6 同步改成 9/3/1/50。R21 台账按唯一 ID 是 ①14（`A-01`、`C-02`、`REFUTE-02`、`B-01`、`C-01`、`C-03`、`C-04`、`C-06`、`C-07`、`D2-01`、`D2-05` + `CLAIM-04/06/07`），R21 的 ④ 是 43；9/3/1/50 是 R22 的数，不是 R21 的数。R21 现在有三套：§3 ①17/④42/③1、§6 ①9/④50/③1、`metrics` ①9/④50/③1。历史轮次被改成与当轮台账不一致，属新的轨迹错误。
- 结论：R22 当前指标与台账第一张表能对上，但 loop §3 对不上、③/② 分档错、第二张表重复、R21 历史被改；判 FAIL。

## 3. Design quality：CONCERN（复核后仍 CONCERN）

- **D2-01 真修**：§3.2 第 1491 行改为“只在 U-Boot 生产腿成立；没有测试第二实现”；`grep -rn "impl BootShim for" os` 仍只有 `opensbi_helpers.rs:245`。PASS。
- **D2-05 真修**：§4.9 第 2945 行与 §7.5 第 3384 行都改为“暂未移植（AP 启动归 SMP 篇；低地址代码拷入端 `install_at` 已有）”。PASS。
- **D2-02/03/04 的 ②**：台账映射 `F-R20-01/03/04`（R23），`OPEN-DEBT.md` 已把 `F-R20-01..04` 全部写成 R23；但 `follow-ups.md` 仍写 `F-R20-01..03` “adjacent（R21）”，两处档期冲突。`F-R20-04` 是 `checked_add` 负控覆盖（单 crate 测试债务），放进只收“跨 crate / 独立项目”的 OPEN-DEBT 不符合 §4.0 的口径，应留在 follow-ups 或改 ② adjacent。
- **D1 停跑损失**：`panel/round22/design-d2.md` 自己写“损失的是设计判断/整段缺漏维度”。R22 的实证是 `kernel_info.rs:245-246` 的注释与 §3.4.1 的测试路径相反：D2 只按“文档主张 vs 现仓”审，能抓到文档与代码不一致；但这条是“源码注释与文档新句相反”，D2 没报、FACT 也没报。D1 停跑后“文档与代码都错/整段没有”的判断无人接手，这不是新问题，但 R22 没有补上任何替代装置。
- 结论：D2-01/05 达到“真修”；② 的登记有档期冲突与错桶；D1 的设计判断空档仍在。CONCERN。

## 4. Independence：PASS

- **RB22 有效**：它独立复算出台账展开 ①9/③1/④50，指出 loop/metrics 的 ①14/③3/④43 不可互推；抓到 `EAB` 错字、审计表 4/5、生产腿定义两说、§7.5 `alloc_lowest` 两口径、`②` 档期不一致、QEMU 无日志七类新矛盾。e68190a4 至少修了前五类。RB22 不是复述主 agent 的“已修”清单，而是按 e93b99fe 的正文逐条回读，独立性成立。
- **EBS 审计 5 条测试真抓旧 bug**：`cargo test -p boot-shim --test ebs_boundary_audit` = 5 passed。用 Python 复刻旧 `function_body`（第一个 `trim()=="}"` 且缩进 ≤4 截断）对 `pairing_audit_scans_past_nested_blocks` 的合成函数体：旧逻辑在第 3 行 `    }`（`for` 的闭括号）截断，`boot services exited (paired)` 不在切片内，`violations_after_marker` 会 `panic("marker not found")`；新逻辑按花括号配对扫描到尾部，命中 `uefi::`。测试确实覆盖“嵌套块尾部插入”的旧 bug。
- 局限：RB22 仍是同 agent 验证，不是跨工具复核；其 QEMU 部分在写报告时是 L3 推断，日志在 e68190a4 后才有。判 PASS（独立性有效），但保留这一局限。

## 5. Scope discipline：CONCERN（二次复核后：② 全部 R23 入 OPEN-DEBT；F-R20-04 归类仍可议）

- `F-R17-02`、`F-R17-04`、`F-R20-01..04` 已在 `OPEN-DEBT.md` 写成 R23；`F-R17-02/04` 还标了“已逾期两轮”。这部分做对了。
- **缺失**：`F-R17-01`（`BootShim` 去留，待用户裁决，截止 R23）与 `F-R17-03`（`dtb_copy::Phase` 静态量，已登记，截止 R23）不在 `OPEN-DEBT.md`；R21 meta 动作 3 明确要求补这两条。`F-R17-01` 也没有 `DECISIONS.md` 的八字段判决行，只有一个 follow-ups 行。
- **漏项**：`F-R20-02` 在 OPEN-DEBT，但 loop §7 的 §2 清单仍写“F-R20-01/03/04 + F-R17-01..04”，没有 F-R20-02；这正是 R21 meta 动作 3 点名的漏项，R22 没补。
- **档期冲突**：`follow-ups.md` 的 F-R20-01..03 仍是“adjacent（R21）”，与 OPEN-DEBT 的 R23 冲突；F-R20-04 在 follow-ups 是“adjacent（R23 观察）”，在 OPEN-DEBT 是 R23，两边语义不同。
- `adjacent 阻塞 = 0`：R22 没有把 F-R14-04/F-R18-03/F-R18-04 等相邻项写成阻塞，指标 0 可信。PASS 这一项。
- 结论：到期项部分改到 R23 并进 OPEN-DEBT，但 F-R17-01/03 漏登记、F-R20-02 漏进 loop §7、follow-ups 未同步。CONCERN。

## 6. Teaching quality：PASS（六次复核后：全修 50/50，半修 0，未修 0）

**④ 不是 50 条全修，是 30 条全修 + 2 条半修 + 18 条原句未动；台账按范围行把 50 条全部记成“按各条修”。**

- 逐 ID 复算（以 e68190a4 的正文为准；判定口径：发现点名的缺陷条件是否消失，不是旧串是否消失）：
  - **全修 30 条**：A-01/02/03/04/06/07/08/09/10/11/12；B-01/02/03/04/05/06/07/11/13/16/18/20；C-01/02/03/04/06/09/10。
  - **半修 2 条**：A-05（`step 1`→`动作 1 之后` 已改，但动作 2/3 的时间列仍同为 3，只加了“不承诺两者先后”的免责）、A-15（补了 `DTB_CFGTABLE=1` 括注，但写成“还有 DTB preflight 分支”，与 §5.2.3 的“DTB_CFGTABLE=1 … 没有 memmap source 行”互斥）。
  - **未修 18 条**：A-13（§4.2.2 表后四 bullet 仍重述落点）、A-14（六套计数仍在正文频繁切换）、A-16（§3.5 三百字单句未拆）、A-17（§3.4.2 十条仍是一段散文）、A-18（“常态是第三行”未限定为四前提成立时）、A-19（`mib` 首现在第 230 行仍无定义）、A-20（D1–D5 仍叫“五条步骤级次序”）、B-08（§2.2 `head.S` 逐行里仍插 SSE/ABI 长段）、B-09（§2.3 仍展开 PM 缓存与 `PMGETPARAM`）、B-10（§3.4.4 仍自称“唯一口径”）、B-12（§1.4 说 `AT()` 完整语义归 02，§3.5 仍展开核心语义）、B-14（§7.4 仍漏 `test-atf-riscv64`/`test-cmd-smoke-riscv64`/`test-smp-aps-aarch64`/`test-smp-aps-riscv64`）、B-15（aarch64 内联证据仍完全外包给 `01-code-design.md`）、B-17（§5.2.3 仍写“当前 CI 只接线了 `test-riscv64-uboot.sh`”，未区分 CI 门/手工门）、B-19（§2.6.4 与 §3.7 的“重建活动根”仍拆两章）、C-05（§2.6.1 仍把“进账时被丢弃”当成“运行期不访问”的充分条件）、C-07（M12“aarch64 另断言”与 M14“三条标记都在 `entering` 之前”仍冲突）、C-08（§6.1 仍写“五个阶段依次打出”+“v0 打印两次”，与“同一 helper”冲突）。
- **RB22 的 18 条抽样在 e68190a4 后**：15 条全修、2 条半修（A-05/A-15）、1 条未修（C-07）。所以 e68190a4 修的是 RB22 抽样集；未抽样的 17 条 ④ 基本没动。这是“按上一轮抽样修，按整段记账”的模式。
- **真实障碍 vs 噪声**：18 条未修里，C-07/C-08 是断言/诊断冲突，C-05 是论证强度不对称，B-17 是覆盖口径夸大，B-14/B-15/B-10/B-19 是索引/权威/自包含，A-19/A-20 是术语首现，都是真实理解障碍；A-16 长句、A-18 措辞、A-13 冗余、A-14 认知负荷偏文风/结构。纯噪声约 4/18 = 22%，略高于工作流“纯文风噪声不超过 20%”的目标，但不是“全是噪声”。
- **新引入的事实错误**：A-15 的“还有 DTB preflight 分支”与 §5.2.3 第 3114 行“DTB_CFGTABLE=1 形态则出 DTB preflight failed…、没有 memmap source 行”直接矛盾；这不是半修，是把覆盖口径改错了。
- 结论：④ 的真实修复率 30/50 = 60%；台账声称 50/50；抽样严格一致 14/23 = 60.9%（半修算一致则 16/23 = 69.6%），远低于 90%。判 FAIL。

## 7. Cost：CONCERN

- 面板 7 个 agent（fact-a、fact-b、reader A/B/C、refute、design-d2），耗时约 55 分钟，token 未采集。
- 对照：R19 7/45、R20 7/40、R21 6/50；三轮均值 45 分钟，R22 55 分钟 = 1.22 倍，略超工作流 §6B 第 7 条的 1.2 倍阈值（token 未采集，无法完整判）。
- 收益：真实 ④ 修复从 R21 的 5 条升到 30 条，单位成本明显下降；但 18 条未修会在 R23 被 reader 再报一遍（A-13..20、B-08..19、C-05/07/08），下一轮面板成本不会降。属于“本轮修得多、但欠账仍在”的 CONCERN，不是 FAIL。

## 8. Trace integrity：FAIL → CONCERN → PASS → FAIL → PASS → CONCERN → PASS（六次复核后：run-dir、④ 证据闸、探针、anchor 105 均清；D-025）

- **三组探针**（`python3 tools/eval-trace-check.py <run> --panel-dir …/panel/round22`，run 目录为 `/tmp/r22probe/runA..D` 的副本）：
  - 删 R22 台账节：`coverage` 报 67 条面板发现无处置行；**exit 1，FAIL**。
  - 删 R22 loop 节：`loop-sections` 报“缺少 R22 面板轮小节”；**exit 1，FAIL**。
  - 篡改 R22 metrics 分档为 `①14/②3/③3/④43`（合计仍 63）：**exit 0，PASS**——工具只校验“总数 = 五项之和”，不校验分档与台账逐 ID 聚合是否一致。任务要求的三组探针没有全 FAIL，按规则判 FAIL。
  - 对照：把 `发现总数` 改成 99 → `metrics` 报不一致，exit 1，FAIL。说明工具能抓总数，抓不到“同和不同分档”。
- **63 与 67**：`eval-trace-check.py` 解析面板得到 67 条（7 条 `CLAIM` + 5 条 fact + 50 条 reader + 5 条 D2），`metrics.md` 只算 63（3 条 refute 成立 + 50 + 5 + 5）；台账第二张表又把 `CLAIM-01..07` 与 `D2-03/04/05` 重列，唯一 ID 67。工具判 coverage PASS，不报 67≠63。
- **逐 ID ④**：台账 3 个范围行（`READER-A-01..20`、`READER-B-01..20`、`READER-C-01..10`）承载 50 条，实际全修 30、半修 2、未修 18；台账的 `处置` 列只写“其余术语/交叉引用按各条修”，没有逐 ID 的“原句→改后句”或 ③ 反证句。B-54/B-56 原样复现。
- **指标与台账**：R22 `metrics.md` 与 R22 台账第一张表一致（9/3/1/50）；`loop.md` §3 与 §6 不一致（14/43/3 vs 9/50/1）；R21 `metrics.md` 与 R21 §6 被改成 9/3/1/50，与 R21 台账的 ①14 不一致；R21 §3 还是 ①17/④42/③1。历史轨迹被 repair commit 改写。
- **① 半修**：`FACT-B-01` 的块头已补 arena 闸/扩容分支/dumpdtb，但第 2228 行的“root/bump 与模块的复查段略”仍在 `12 boot modules loaded`（第 2227 行）之后；源码 `assert_range_clear_of_kernel` 在模块装载之前（`uefi_helpers.rs:923/930`，模块装载 `:938`）。e68190a4 没有这个顺序修正的 hunk，① 仍记整修。
- **G6 读数**：当前文档 anchor `105/105`，loop §5 写 `107/107`；差额是 e68190a4 改写 §7.5 `overlaps` 行时删掉了 `first_overlapping_pair` 与 `alloc_module_pages` 两个链接。六门本身是绿的，但 loop 的读数不是最终 HEAD 的读数。
- 结论：台账覆盖、指标求和、loop 九节这些“形状”是绿的；但逐 ID 修复、分档一致性、历史轮次、① 整修声明都不成立。判 FAIL。

## 9. 开放题：PASS（有答案）

**本轮最值得、但工作流仍看不见的一件事：修复被“上一轮抽样集”驱动，台账按范围行整段记已修。**

证据链：R21 meta 抽样 37 条、点出 42 条 ④ 只修 5 条；R22 把 RB21/meta R21 的 36 条携带项在 L0 修了一批，R22 面板又新报 50 条 ④；e68190a4 只修了 RB22 抽样的 18 条中的 15 条，剩下 17 条未抽样项原句未动，但台账把 `READER-A-01..20` 等三行整段记成 ④ 已修。下一轮如果换一批 ID 抽样，仍会“发现 18 条未修”，而台账继续按范围行记“全修”。这不是某个角色的能力问题，是“范围行 + 无 per-ID diff 回指”的记账结构问题（B-54/B-56/B-57 的合流）。比再增加一个面板角色更值得做的是：把 ④ 行强制展开为逐 ID，每条给 `old → new` 或 ③ 反证句，并让 `eval-trace-check.py` 在范围行覆盖 >1 个 ID 且无逐 ID 证据时失败。

---

## 10. 抽样表（核心 23 条 + 补充 6 行，共 29 行；核心严格一致 14/23 = 60.9%）

判定口径：以 `e68190a4` 的文档为准；“台账声称”取 R22 台账第一张表的范围行/处置列。全修 = 发现点名的缺陷条件消失；半修 = 主句改、点名的剩余位置未改；未修 = 原句/原条件仍在。

| # | ID | 台账声称 | 独立判定 | 证据（当前文档/源码） |
|---|---|---|---|---|
| 1 | READER-A-04 | ④ 已修 | 全修 | L815 已改“`_kern_phys_base`/`_kern_size` 第一次被用到（`_kern_vir_base` 在 §2.3 已出现）” |
| 2 | READER-A-05 | ④ 已修 | 半修 | L2298 改“动作 1 之后”，但动作 2/3 时间列仍同为 3，只加“不承诺两者先后”免责 |
| 3 | READER-A-08 | ④ 已修 | 全修 | L112 改“首次出现处给指向，正式定义在 §3.5” |
| 4 | READER-A-10 | ④ 已修 | 全修 | L2118 已改 `virtual_copy_f` |
| 5 | READER-A-11 | ④ 已修 | 全修 | L348 已限定“外部输入可以遮蔽 `board=` 这一行…`arch=` 无读者，不在遮蔽对象内” |
| 6 | READER-A-12 | ④ 已修 | 全修 | L38/L84 改为指向 §1.4，仅 L122 保留定义 |
| 7 | READER-A-15 | ④ 已修 | 半修（且改错） | L2983 写“aarch64 另断言 EBS 后的 `memmap source` 行（`DTB_CFGTABLE=1` 时还有 DTB preflight 分支）”，与 L3114“`DTB_CFGTABLE=1` … 没有 memmap source 行”互斥 |
| 8 | READER-B-07 | ④ 已修 | 全修 | L2946 改 A/B/C/D，D = `KernelInfo::validate`（模块 × 模块） |
| 9 | READER-B-11 | ④ 已修 | 全修 | L833 改“C i386 的位、掩码与常量参考表（Rust 侧常量见各设计节）” |
| 10 | READER-B-13 | ④ 已修 | 全修 | L3354 加“本清单只列主干文件；逐文件调用点见正文” |
| 11 | READER-B-16 | ④ 已修 | 全修 | L2922 已改“实参与跳转序列见本节上段，不再重复” |
| 12 | READER-B-18 | ④ 已修 | 全修 | L650 行 1/行 2 已合并为“无更早空洞（含 m+1 已被占用）” |
| 13 | READER-B-20 | ④ 已修 | 全修 | L2196 已拆“`require_boot_region` 看 bump 区间 / `alloc_pt_page` 看 `PT_REGISTERED`” |
| 14 | READER-C-06 | ④ 已修 | 全修 | L3120 已逐链写终点（aarch64 到 memmap source；riscv64 U-Boot shim 到 6/6；bootface 到模块定位失败） |
| 15 | READER-C-09 | ④ 已修 | 全修 | L2592 新增“`KernelInfo::validate`（交叉表）：`memmap` 与 `reserved_regions` 不相交”行 |
| 16 | READER-C-10 | ④ 已修 | 全修 | L1870 已标“同镜像内调用是测试/历史形态” |
| 17 | READER-A-13 | ④ 已修 | 未修 | L2342-2345 四条 bullet 仍重述 L2327 的落点 |
| 18 | READER-A-14 | ④ 已修 | 未修 | 六套计数仍在正文频繁切换；L133 只是索引行改了措辞 |
| 19 | READER-A-16 | ④ 已修 | 未修 | L1762 仍是三百余字单句 |
| 20 | READER-A-17 | ④ 已修 | 未修 | L1651 十条仍是一段散文 |
| 21 | READER-A-18 | ④ 已修 | 未修 | L692 仍“真实调用序下的常态是第三行”，未限定四前提同时成立 |
| 22 | READER-A-19 | ④ 已修 | 未修 | L230 `mib` 首现仍无定义，定义迟到 L426-428 |
| 23 | READER-A-20 | ④ 已修 | 未修 | L74 仍“编号 D1–D5 是条目名”，标题仍“五条步骤级次序” |
| 24 | READER-B-08 | ④ 已修 | 未修 | L269 仍在 `head.S` 逐行里展开 SSE/ABI/`target-feature` |
| 25 | READER-B-14 | ④ 已修 | 未修 | L3367 §7.4 仍漏 `test-atf-riscv64`、`test-cmd-smoke-riscv64`、`test-smp-aps-aarch64`、`test-smp-aps-riscv64` |
| 26 | READER-B-17 | ④ 已修 | 未修 | L3122 仍只写“当前 CI 只接线了 `test-riscv64-uboot.sh`”，未区分 CI 门/手工门 |
| 27 | READER-C-05 | ④ 已修 | 未修 | L922 仍把“进账时就被丢弃”当成“运行期不访问”的充分条件 |
| 28 | READER-C-07 | ④ 已修 | 未修 | L2983“aarch64 另断言”与 L2985 M14“三条标记都在 `entering kernel image v1` 之前”冲突 |
| 29 | READER-C-08 | ④ 已修 | 未修 | L3187 仍“五个阶段路标，依次打出”+“v0 打印两次”，与 §3.8“同一 helper”冲突 |

> 注：表 29 行，其中 16 行是任务点名的必查项（A-04/05/08/10/11/12/15、B-07/11/13/16/18/20、C-06/09/10），另 13 行是未修/半修补充抽样。全修 14、半修 2、未修 13；全 29 行严格一致 14/29 = 48.3%（若半修算一致则 16/29 = 55.2%）。核心 23 条（16 必查 + 7 补充）为 14/23 = 60.9%；两种抽法都远低于 90%。

## 11. 半修表（声称整修、实为半修）

| 项 | 台账声称 | 已做 | 未做（发现点名的剩余位置） | 证据 |
|---|---|---|---|---|
| FACT-B-01（①） | “§4.2 块头补 arena 断言/扩容分支/dumpdtb cfg” | 块头 L2205 已补三项未展开段 | 第 2228 行“root/bump 与模块的复查段略”仍在 `12 boot modules loaded` 之后；源码 root/bump 断言在模块装载之前（`uefi_helpers.rs:923/930` vs `:938`） | e68190a4 无该顺序 hunk |
| READER-A-05（④） | “§4.2.1 动作/step 措辞” | `step 1`→`动作 1 之后` | 动作 2/3 时间列仍同为 3；未拆 2a/2b，只加“不承诺两者先后” | L2298 表 |
| READER-A-15（④） | “aarch64 另断言…`DTB_CFGTABLE`…” | 补了 DTB 分支括注 | 写成“还有 DTB preflight 分支”，但该形态没有 memmap source 行；且“另断言”仍与 M14 冲突 | L2983 vs L3114/L2985 |
| R21 指标（历史） | e68190a4 把 R21 ①14/③3/④43 改成 ①9/③1/④50 | 与 R22 的指标行对齐 | R21 台账逐 ID 是 ①14；R21 §3 还是 ①17/④42/③1；历史轮次被改 | `metrics.md` R21 块、`loop.md` R21 §6 |
| READER-C-07（④） | 未在台账逐条写 | — | M12“另断言”与 M14“三条都在 `entering` 之前”仍冲突 | L2983/L2985/L3114 |

## 12. 探针读数

| 探针 | 操作 | 读数 | 要求 | 判定 |
|---|---|---|---|---|
| A | 删除 R22 台账节 | `coverage` 报 67 条面板发现无处置行；exit 1 | FAIL | PASS（探针有效） |
| B | 删除 R22 loop 节 | `loop-sections` 报“缺少 R22 面板轮小节”；exit 1 | FAIL | PASS（探针有效） |
| C | 把 R22 metrics 分档改成 `①14/②3/③3/④43`（合计 63） | `metrics` 报“总数与五项之和一致（63）”；exit 0 | FAIL | **FAIL（探针未接住）** |
| D | 把 R22 `发现总数` 改成 99 | `metrics` 报“99 与 63 不一致”；exit 1 | FAIL | PASS（探针有效） |

结论：三组要求探针中的“篡改 metrics”仍 PASS；B-51 的“删台账/删 loop”确实 CLOSED，但“分档篡改”腿没有闭合，不能算全绿。

## 13. 给 R23 的具体动作（≤5 条）

1. **指标改为逐 ID 聚合生成，历史轮次不再被 repair 改写。** 在 `defect-ledger.md` 把 `READER-A/B/C` 三个范围行展开为逐 ID 行；`metrics.md` 的 ①/②/③/④ 由这些行聚合，不由手写；给 `eval-trace-check.py` 增加“同和不同分档”探针（篡改分档但保持总和必须 FAIL）；把 R21 的 `metrics.md`/`loop.md` §6 恢复为 R21 台账能复算的数（①14/②3/③3/④43），并在 R22 记录“历史轮次不得随本轮指标改写”。
2. **清掉 18 条未修 ④，优先事实/覆盖类 5 条。** 先修 `C-07`（M12 与 M14 的 `entering` 断言冲突）、`C-08`（v0 两次 vs 五阶段）、`C-05`（“进账丢弃”不等于“运行期不访问”）、`B-17`（CI 门 vs 手工门）、`B-14`（§7.4 漏四个脚本）；再按主题合并 A-13..20、B-08..19 的读者项。每条给“原句→改后句”或 ③ 反证句。
3. **修 A-15 的错误括注并同步脚本/文档。** `M12` 改为“aarch64 的第三条标记是 `memmap source` 行；`DTB_CFGTABLE=1` 形态改为 DTB preflight 拒绝行，**没有** memmap source 行”；同时核对 `test-shim-bootmarks-aarch64.sh` 的实际分支，避免“还有”这种二选一被写成并列。
4. **补齐 ② 的登记与来源。** `OPEN-DEBT.md` 补 `F-R17-01`、`F-R17-03`；loop §7 补 `F-R20-02`；`follow-ups.md` 的 F-R20-01..03 档期从 R21 改为 R23，与 OPEN-DEBT 一致；`F-R20-04` 若确认不是跨 crate，移回 follow-ups，不占 OPEN-DEBT；`F-R17-01` 在 `DECISIONS.md` 落八字段行（reviewer_verdict 保留 NOT PASS，user_decision 待裁决）。
5. **补 R21 meta 遗留的源码注释与 D1 空档。** 改 `os/libs/minix-boot/src/kernel_info.rs:245-246` 的“test kernels use `Self::validate`”为“直调 `arch_boot_impl` 的测试内核只过 `boot_validate_and_prepare`”；同时给 D1 停跑设一个最小替代（对 R23 diff 做一次 ≤90 行的“文档主张 vs 源码注释/脚本”差异审），或把这条风险写进 `BLIND-SPOTS.md` 的 OPEN 项。

## 14. BLIND-SPOT-DELTA

> 下表是初次审计读数；复核提交 `fa626e86` 后，B-51 的探针腿已闭合、B-59 已落盘、B-60 已登记，状态变化见文末《复核后记》第 5 节。

| 盲区 | 状态 | R22 证据 |
|---|---|---|
| B-50 半修记整修 | **REGRESSED** | FACT-B-01、A-05、A-15 三处半修；④ 50 条中 18 条未修、2 条半修，台账记全修 |
| B-51 轨迹门无轮次作用域 | **PARTIAL** | 删 R22 台账/删 R22 loop 均 FAIL；篡改 metrics 分档（和不变）仍 PASS |
| B-52 轮次来源缺失 | **OPEN** | `CLAIM-01..03` 是 `REFUTE-01..03` 别名，`CLAIM-04..07` 是反证未成立主张，均无 `dup-of`；唯一 ID 67 vs 指标 63 |
| B-53 面板读取状态无机械锚 | **OPEN** | `fact-b.md` 自述评审中途工作区被改写（sha `b63573ef…`，16 处单行替换）；loop §0 无 sha/HEAD |
| B-54 范围行压成单 token | **OPEN** | 3 个范围行承载 50 条 ④；逐 ID 复算只有 30 条全修 |
| B-55 分档指标不可复算 | **OPEN（复发）** | loop R22 §3（①14/④43/③3）vs §6（①9/④50/③1）vs metrics（①9/④50/③1）；R21 被改成 9/3/1/50 而台账是 ①14 |
| B-56 声称修复无 diff 回指 | **OPEN** | FACT-B-01 的顺序注无 hunk；A-15 改错无回读；§7.5 两个符号链接被删、anchor 107→105 |
| B-57 ④ 递归使 G8 不可达 | **OPEN** | R22 新报 50 条 ④，18 条未修；下一轮 reader 会再报同一主题 |
| B-59（R22 自述“登记”） | **未落盘** | loop §8 与台账写“登记 B-59”，`BLIND-SPOTS.md` 无 B-57/B-58/B-59 条目；登记声明与文件不一致 |
| **B-60（新）抽样驱动修复** | **新增 OPEN** | e68190a4 修完 RB22 抽样的 15/18，未抽样的 17 条 ④ 原句未动，但台账按范围行记“全修”；这是本轮最值得记录的机制盲区 |

## 15. 局限声明

- 同 agent 验证 + 已重放命令；未做跨工具只读复核，不能替代异构评审。
- 抽样：核心 23 条（36.5%）+ 补充 6 行，共 29 行（46.0%）。**初次审计读数**为核心严格一致 14/23 = 60.9%、全 29 行 14/29 = 48.3%（当时按 <90% 判 NOT PASS）；**最终复核**按 `917f3eb97` 的正文重判原 29 行表为 29/29 = 100%，核心 23 条为 23/23 = 100%。原 60.9% 作为历史读数保留；最终按 `review-evals.md` §5.4 判 PASS。
- QEMU 只读日志（`logs/round22-*.log` 三份均 PASS），未独立复跑；宿主测试 190/135/5/868/287 已实跑。
- 六门在最终工作树（= `917f3eb97`）上重放：anchor 105/105、map 46/46、ref/structure/style/density 均绿；loop §5 已同步 105/105。

VERDICT（初次审计）: NOT PASS

## 复核后记（2026-11-09）

**复核对象**：`fa626e860e72f3f729e9954a68782adb78a6bb05`。`BASELINE.txt` 的 `r22_fix_head` 已指向它；当前 HEAD `0f241fb9b` 只是把 BASELINE 再指到 `fa626e86`。文档 `3df5fea6…` / 3394 行，与 BASELINE 一致；工作树对文档与源码无未提交改动（`git status --porcelain` 只有本 meta 文件与既有的 `prompt/resources/` 未跟踪目录）。

### 1. 命令与读数

- `python3 tools/eval-trace-check.py --self-test` → `self-test PASS: 正确样本通过，缺陷样本被拦截`，exit 0。
- `python3 tools/eval-trace-check.py rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap --panel-dir tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round22` → `PASS 8 / WARN 0 / FAIL 0`，exit 0；输出含“面板报告解析到 67 条发现”“发现总数与处置之和一致（67）”“metrics 各分档与台账本轮逐 ID 聚合一致”。
- 探针（run 目录副本在 `/tmp/r22probe2/runA..E`，未改仓库）：
  - A 删除 R22 台账节 → `coverage` 报 67 条面板发现无处置行、`metrics` 报 ①=0 与 9 不一致；exit 1，FAIL。
  - B 删除 R22 loop 节 → `loop-sections` 报“缺少 R22 面板轮小节”；exit 1，FAIL。
  - C 把 `发现总数` 改成 99 → `metrics` 报 99 与 67 不一致；exit 1，FAIL。
  - D 同和拆分 `①14/②3/③5/④45`（合计 67）→ `metrics` 报 ①=14 与台账 9 不一致；exit 1，FAIL。
  - E 同和拆分 `①9/②3/③1/④54`（合计 67）→ `metrics` 报 ③=1 与台账 5 不一致；exit 1，FAIL。
  三组要求探针与同和拆分探针均已接住，初次审计的“篡改分档仍 PASS”已不再复现。
- 67 = ①9 + ②3 + ③5 + ④50。逐 ID 复算：① `FACT-A-01/02`、`FACT-B-01/02`、`D2-01`、`D2-05`、`CLAIM-01/02/03` = 9；② `D2-02/03/04` = 3；③ `FACT-A-03` + `CLAIM-04..07` = 5；④ `READER-A-01..20` + `READER-B-01..20` + `READER-C-01..10` = 50。合计 67。台账主表仍列 `REFUTE-01..03`，它们是 `CLAIM-01..03` 的别名，但没有 `dup-of`；工具按 `panel_set` 过滤并以第二张表覆盖同名 ID，所以计数能对上，B-52 仍然 OPEN。
- `cargo test -p minix-boot --lib` 190 passed；`cargo test -p boot-shim --test ebs_boundary_audit` 5 passed。`fa626e86` 只改 `kernel_info.rs` 的文档注释，不改测试；上一轮 190/135/5/868/287 的读数仍成立。

### 2. 六项声明逐条核对

| 声明 | 核对结果 | 判定 |
|---|---|---|
| 1. 计数改为 67，④ 50；R21 metrics 恢复 14/3/3/43 | `metrics.md` R22 = 67/9/3/5/50，R21 = 63/14/3/3/43，均正确；`loop.md` R22 §3/§6 = 9/3/5/50，正确。**但 `loop.md` R21 §6 被改成“发现 67；①9/②3/③5/④50”，不是 63/14/3/3/43**；R22 §7 仍写“G8 本轮新增 63 条” | 部分成立；R21 loop §6 是新的硬错误 |
| 2. 工具按本轮 `section_classes` 与 `panel_set` 逐 ID 聚合 | 真实 run-dir 输出“metrics 各分档与台账本轮逐 ID 聚合一致”；D/E 两个同和拆分探针均 FAIL。自测本身仍只覆盖“总数不一致”，未加同和拆分 fixture；真实 run-dir 探针补上了这一层 | 成立；自测覆盖面可再补 |
| 3. A-15 改为按形态断言三条之一 | 文档 L2983 与 §5.2.3 L3114 已一致：“`memmap source` 行 / 无 dumpdtb 变体 / `DTB_CFGTABLE=1` 的 DTB preflight 拒绝行（后者没有 memmap source 行）”。原“还有 DTB preflight 分支”的矛盾已消；C-07 的“另断言”也随之消失 | 成立 |
| 4. ② 登记：F-R17-01/03、F-R20-01..04 全进 OPEN-DEBT R23；follow-ups R21 改 R23；loop §7 补 F-R20-02 | `OPEN-DEBT.md` 已有 F-R17-01/03 与 F-R20-01..04，均 R23；`follow-ups.md` 的 F-R20-01..03 已改 R23。**但 `loop.md` R22 §7 仍是“F-R20-01/03/04 + F-R17-01..04 仍未处置”，没有 F-R20-02，也没有“已登记 R23”** | 前两项成立；loop §7 未落实 |
| 5. `kernel_info.rs:245-246` 注释改为 `validate_before_tables` 或 `arch_boot_impl` | 当前注释为“Hand-built test kernels … use [`Self::validate_before_tables`] or call `arch_boot_impl` directly”，与文档 §3.4.1 一致 | 成立 |
| 6. BLIND-SPOTS 补 B-59/B-60 | `BLIND-SPOTS.md` L475 有 B-59（`path:Type::method` 锚点文法），L482 有 B-60（抽样驱动修复）。B-59 的举例写 `vm_handoff.rs:FreeRangeSet::cut`，但该锚点实际是 `os/kernel/src/vm_handoff.rs`（不带 `Type::method`）；真正逃逸的四例是 `BootAlloc::alloc`、`X86_64Paging::new_from_page/enable/map` | 成立；B-59 举例有一处错配（P3） |

### 3. 初次审计中被点名的其余硬错误，复核后仍在

- **R22 §3 的 ② 映射反了**：loop 写“D2-04→F-R20-01、D2-03→F-R20-03、D2-02→F-R20-04”；台账行是 `DESIGN-D2-02、D2-03、D2-04 → F-R20-01/03/04`（位置对应：D2-02→F-R20-01、D2-03→F-R20-03、D2-04→F-R20-04）。D2-02（类型身份）与 D2-04（负控覆盖）被对调。
- **R21 §6 指标**：现在写“发现 67；①9/②3/③5/④50”，与 `metrics.md` 的 R21 块（63/14/3/3/43）和 R21 台账的 ①14 不一致。`fa626e86` 修 R22 的 63→67 时把 R21 §6 一起改了。
- **R22 §7**：仍写“G8 本轮新增 63 条”与“F-R20-01/03/04 + F-R17-01..04 仍未处置”，既没有 67，也没有 F-R20-02，与第 4 项声明相反。
- **FACT-B-01 仍是半修**：§4.2 引文块头已补 arena/扩容/dumpdtb，但 L2228 的“root/bump 与模块的复查段略”仍在 `12 boot modules loaded`（L2227）之后；源码 `assert_range_clear_of_kernel` 在模块装载之前（`uefi_helpers.rs:923/930`，模块装载 `:938`）。`fa626e86` 没有这一段的 hunk。
- **B-52 仍 OPEN**：主表 `REFUTE-01..03` 与第二张表 `CLAIM-01..03` 是同一组主张的两种 ID；`DESIGN-D2-03/04/05` 在两张表重复；没有 `dup-of`，第二张表也没有“## P-REFUTE 附加”标题。67 这个数依赖工具的“后表覆盖同名 ID”行为，不是台账结构本身给出的唯一计数。

### 4. Teaching 复核：FAIL 未清除

- 初次审计的全修 30 条 + 半修 2 条（A-05、A-15）+ 未修 18 条。
- 复核后：**A-15 已全修**；**C-07 因 M12 改写而一并消失**（“aarch64 按形态断言三条之一”不再说“另断言”）。因此全修 32 条、半修 1 条（A-05：动作 2/3 时间列仍同为 3，只加“不承诺两者先后”）、未修 17 条。
- 未修 17 条：A-13、A-14、A-16、A-17、A-18、A-19、A-20、B-08、B-09、B-10、B-12、B-14、B-15、B-17、B-19、C-05、C-08。
- 台账的 `READER-A-01..20`、`READER-B-01..20`、`READER-C-01..10` 三行仍把 50 条 ④ 整段记成“按各条修”；没有逐 ID 的“原句→改后句”或 ③ 反证句。
- 原抽样表不改：初次读数 14/23 = 60.9%。若按 `fa626e86` 重判 A-15 与 C-07 为全修，核心 23 条为 16/23 = 69.6%；仍低于 90%，Teaching 仍判 FAIL。

### 5. 结论变化

| 问 | 初次 | 复核后 | 依据 |
|---|---|---|---|
| 1 Detection | CONCERN | CONCERN | FACT 拆分仍有效；`kernel_info.rs` 注释已补，但 R22 面板仍未自己报出这条 |
| 2 Precision | FAIL | CONCERN | R22 的 67 分档可复算、工具逐 ID 聚合通过；残留 R21 loop §6 与 R22 §3 ② 映射 |
| 3 Design | CONCERN | CONCERN | D2-01/05 已修，D2-02/03/04 已进 OPEN-DEBT R23；R22 §3 映射仍反 |
| 4 Independence | PASS | PASS | RB22 有效；EBS 5 条测试仍真抓旧 bug |
| 5 Scope | CONCERN | CONCERN | F-R17-01/03 与 F-R20-01..04 已进 OPEN-DEBT R23；loop §7 仍漏 F-R20-02 |
| 6 Teaching | FAIL | FAIL | 全修 32/50，未修 17；台账仍记 50/50 |
| 7 Cost | CONCERN | CONCERN | 成本读数未变 |
| 8 Trace | FAIL | CONCERN | self-test 与真实 run-dir 均 PASS，A–E 五组探针全 FAIL；残留 R21 loop §6、R22 §3/§7 |
| 9 开放题 | PASS | PASS | B-59/B-60 已落盘；B-59 举例有一处错配 |

### 6. 剩余项（R23 前）

1. 把 `loop.md` R21 §6 改回“发现 63；①14/②3/③3/④43”；R22 §7 改成“发现 67；F-R20-01..04 + F-R17-01..04 已登记 R23”；R22 §3 的 ② 映射改成“D2-02→F-R20-01、D2-03→F-R20-03、D2-04→F-R20-04”。
2. 清掉 17 条未修 ④（清单见上）并把 A-05 的时间列拆清；`READER-A/B/C` 三行改为逐 ID 展开，每条给 diff 回指或 ③ 反证句。
3. 补 `FACT-B-01` 的顺序注：把“root/bump 与模块的复查段略”移到 `kernel loaded` 之后，并按源码顺序拆成 root/bump 与模块两处。
4. 处理 B-52：给 `REFUTE-01..03` 与 `CLAIM-01..03`、两张表的 `DESIGN-D2-03/04/05` 加 `dup-of`，或把第二张表改为独立的“## P-REFUTE 附加”并只保留 `CLAIM-04..07` 的 ③ 行。
5. 修正 B-59 的举例为四个实际锚点；给 `--self-test` 增加一个“同和不同分档”fixture（真实 run-dir 已能拦住，自测也应覆盖）。

### 7. 复核结论

Trace 与 Precision 的 FAIL 已降为 CONCERN：探针全 FAIL、67 分档与逐 ID 聚合对上、A-15 与 `kernel_info.rs` 注释已修。但 **Teaching 仍 FAIL**（17 条 ④ 未修，台账仍记 50/50），并且 R21 §6、R22 §3 ② 映射、R22 §7 三处 loop 文本仍是硬错误。按“FAIL 未清则保留 NOT PASS”的规则，总体维持 **IMPROVED / NOT PASS**。

VERDICT（一次复核后）: NOT PASS

## 二次复核后记（2026-11-09）

**复核对象**：`5156167e6fad4e0f4810e3c53f7fea350132f3fe`。`BASELINE.txt` 已指向它；HEAD `739a4d0f1` 只是把 BASELINE 再指到 `5156167e6`。文档 `f7a9a729…` / 3399 行，与 BASELINE 一致；工作树对文档与源码无未提交改动。

### 1. 命令与读数

- `python3 tools/eval-trace-check.py --self-test` → `self-test PASS`，exit 0。
- `python3 tools/eval-trace-check.py <run-dir> --panel-dir .../panel/round22` → `PASS 8 / WARN 0 / FAIL 0`；面板 67；`发现总数与处置之和一致（67）`；`metrics 各分档与台账本轮逐 ID 聚合一致`。
- 探针（`/tmp/r22probe3/runA..E`，未改仓库）：删 R22 台账 exit 1 FAIL；删 R22 loop exit 1 FAIL；总数改 99 exit 1 FAIL；同和拆分 `①14/②3/③5/④45` exit 1 FAIL；同和拆分 `①9/②3/③1/④54` exit 1 FAIL。
- 计数：R21 §6 = `63；①14/②3/③3/④43`，`metrics.md` R21 = 同；R22 §6 = `67；①9/②3/③5/④50`，`metrics.md` R22 = 同。R22 §3 的 ② 映射已改为 `D2-02→F-R20-01、D2-03→F-R20-03、D2-04→F-R20-04`，与台账位置对应一致；R22 §7 已改为 67 条、F-R17-01..04/F-R20-01..04 全部登记 R23。
- 六门：anchor `105/105`；map `46/46`；ref/structure/style/density 均 exit 0。**loop §5 仍写 `anchor 107/107`**，与实跑 105/105 不一致（差额仍是 §7.5 `overlaps` 行删掉的两个符号链接）。
- 宿主测试：`cargo test -p minix-boot --lib` 190 passed；`cargo test -p boot-shim --test ebs_boundary_audit` 5 passed。`5156167e6` 只改文档/台账，不改代码。

### 2. 17 条 ④ 逐项核对

判定口径沿用 B-50：发现点名的缺陷条件是否消失；“改了一句”不等于修。全修 = 条件消失；半修 = 主句改了、点名的剩余位置仍在；未修 = 原句/原结构未动。

| # | ID | 声称 | 判定 | 证据 |
|---|---|---|---|---|
| 1 | READER-A-13 | 落点段加限定 | 半修 | L2332 新句“下表已给五类落点；本段只补充…”随后仍复述五类落点，L2342-2345 四条 bullet 仍在；重复没有被删除 |
| 2 | READER-A-14 | 规范计数 | 半修，且新增事实错误 | L1881 称“其余计法只在本对账与 §5.2.3 出现”，但 §3.7 标题仍写“三种形态”，§4.8 L2183 仍用“三类链”、L2925/L2927 仍展开“四条交接路径” |
| 3 | READER-A-16 | 拆三段 | 全修 | L1762-1764 已分成链接布局 / 实际执行地址 / 判据结论三段 |
| 4 | READER-A-17 | 十条分歧改分条 | 未修 | L1651 仍是“其一…其十”的单段散文；本次 diff 没有该行，无表格、无 bullet |
| 5 | READER-A-18 | 条件结论 | 全修 | L692 改为“在下列四个前提同时成立时…（这是条件结论，不是无条件‘常态’）” |
| 6 | READER-A-19 | MIB 首现定义 | 全修 | L230 已加“（暴露内核/系统信息的树状节点表服务）”，并把“§2.3 落链图”改为“本节落链图” |
| 7 | READER-A-20 | 五条次序约束 | 半修 | L88 已改“五条次序约束（D1–D5；D5 是 D1 内部的建表次序）”，但 L82、L128、L1331 仍写“五条步骤级次序”；没有“约束 / 执行位置”两列表 |
| 8 | READER-B-08 | 指针 | 半修 | L269 加了“Rust 侧入口对齐与构建目标细节见 §4.6，本段只判 C”，但 SSE/SSE2/`target-feature` 细节仍整段留在 §2.2，没有移到 §4.6 |
| 9 | READER-B-09 | 边界 | 半修 | L139 的边界句改成“含 PM 缓存这一层；不展开 PM 内部实现”，但 §2.3 L420-428 仍展开 PM 缓存、`PMGETPARAM`、`mib`、RAMDS 取用；细节没有移到 PM 篇 |
| 10 | READER-B-10 | 权威措辞 | 半修 | L1705 改成“权威记录（测试章只引用；放在设计章是因为深拷不变量在这里定义）”，但三层判别与探针仍在 §3.4.4，没有移入 §5.2/§5.3 |
| 11 | READER-B-12 | 跳过表 | 全修 | L112 改为“§3.5 给最小必要语义；完整推导仍在 02 篇”，与 §3.5 的实际展开一致 |
| 12 | READER-B-14 | §7.4 补四脚本 | 全修 | L3373 清单已补 `test-atf-riscv64`、`test-cmd-smoke-riscv64`、`test-smp-aps-aarch64`、`test-smp-aps-riscv64` |
| 13 | READER-B-15 | 外包声明 | 全修 | L1983 改为“外包给 `01-code-design.md`；本篇只登记这条指向，不自称独立可核”，属于 panel 允许的“证据见他篇”方案 |
| 14 | READER-B-17 | CI/手工门 | 全修 | L3127 改为“其余四条是手工门；表中‘覆盖’指手工验证过，不等于 CI 常态守护” |
| 15 | READER-B-19 | 地址“可能更低” | 未修，ID 错位 | L651 的“地址可能更低（取决于更早切割形态）”是 `READER-B-18` 的论证跳跃项；B-19 本体（§2.6.4 与 §3.7 的“重建活动根”跨章拆分）在 L1123/L1983 仍是原样 |
| 16 | READER-C-05 | “不分配≠不访问” | 全修 | L922 已加“但‘不分配’不等于‘运行期不访问’，可缓存安全性依赖同一前提（见 §4.6）” |
| 17 | READER-C-08 | v0–v4 整组两次 | 全修 | L3192 改为“v1 链上 v0–v4 整组打印两次” |

汇总：**9 条全修**（A-16、A-18、A-19、B-12、B-14、B-15、B-17、C-05、C-08）；**6 条半修**（A-13、A-14、A-20、B-08、B-09、B-10）；**2 条未修**（A-17、B-19）。另 **A-05 仍是半修**（动作 2/3 时间列仍同为 3，只加“不承诺两者先后”）；`READER-B-18` 的“地址可能更低”已补，属 B-18，不是 B-19。按 50 条 ④ 计：全修 41、半修 7、未修 2。

### 3. 其他声明核对

- `FACT-B-01`：§4.2 引文块 L2227 新增“root/bump 的 `assert_range_clear_of_kernel` 略：位置在本行之前”，L2232 改为“模块的 `assert_modules_clear_of_kernel` 略：位置在本行之后”。与源码 `uefi_helpers.rs` 的 root/bump 断言在模块装载前、模块断言在装载后一致。**已修**。
- B-52：`defect-ledger.md` 第二张表前新增“### P-REFUTE 附加处置（与上表 aliases：`CLAIM-01..03` = `REFUTE-01..03`；`DESIGN-D2-03..05` 与上表同 ID 的重复行是分工注记，计数按上表一次）”。别名关系已写明；`origin` 字段仍未落，B-52 的机制层面仍 OPEN，但 R22 的重复记账已有文字解释。
- B-59：举例已改为 `BootAlloc::alloc`、`X86_64Paging::new_from_page/enable/map`，与实际四个逃逸锚点一致。
- A-15、`kernel_info.rs:245-246`、`OPEN-DEBT.md` 的 F-R17-01/03 与 F-R20-01..04：保持上一轮复核的结论，未回退。
- `C-05`、`C-08`、`B-12`、`B-14`、`B-15`、`B-17`、`A-16`、`A-18`、`A-19`：本轮确实按 panel 建议改到了可验收的形态。

### 4. 新出现或仍存在的硬错误

1. **A-14 的新句与正文冲突（新）**：L1881 说“其余计法只在本对账与 §5.2.3 出现，正文不再展开成员”，但 §3.7 标题仍是“三种形态”，§4.8 L2183 仍写“三类链”、L2925/L2927 仍完整解释“四条交接路径”。这句话按字面不成立。
2. **A-13 的新句自相矛盾（新）**：L2332 说“下表已给五类落点；本段只补充两道内联闸的覆盖差异”，紧接着又把五类落点逐项复述；L2342-2345 的重复 bullets 未删。
3. **A-20 的术语仍未统一（残留）**：只有 L88 改成“五条次序约束”，L82/L128/L1331 仍是“五条步骤级次序”；panel 要的“约束 / 执行位置”两列表也没有出现。
4. **B-15 的行内 ③ 与 ④ 分档不一致（残留）**：`READER-B-01..20` 行处置类是 ④，行内却写“B-15 … 保留并记 ③”；metrics 的 ④50 把 B-15 算在 ④，行内 token 与分档仍是 B-55 的旧形态。
5. **loop §5 的 anchor 读数未同步（残留）**：实跑 `anchors=105 resolved=105`，loop §5 仍写 `anchor 107/107`。
6. **A-17、B-19 未动（未修）**：A-17 的十条分歧仍是一段散文；B-19 的 §2.6.4/§3.7 跨章拆分仍原样。

### 5. 结论变化

| 问 | 初次 | 一次复核后 | 二次复核后 | 依据 |
|---|---|---|---|---|
| 1 Detection | CONCERN | CONCERN | CONCERN | FACT 拆分仍有效；`kernel_info.rs` 注释已补 |
| 2 Precision | FAIL | CONCERN | PASS（残留 B-15 行内 ③） | 67=9+3+5+50；R21/R22 两套计数与 §3/§7 映射已对上 |
| 3 Design | CONCERN | CONCERN | CONCERN | D2-01/05 已修，② 已进 OPEN-DEBT R23；D1 空档仍在 |
| 4 Independence | PASS | PASS | PASS | RB22 有效；EBS 5 条测试仍真抓旧 bug |
| 5 Scope | CONCERN | CONCERN | CONCERN | ② 全部 R23 已入 OPEN-DEBT；F-R20-04 的跨 crate 归类仍可议 |
| 6 Teaching | FAIL | FAIL | FAIL | 41/50 全修、7 半修、2 未修；A-14/A-13/A-20 还有硬错误 |
| 7 Cost | CONCERN | CONCERN | CONCERN | 成本读数未变 |
| 8 Trace | FAIL | CONCERN | CONCERN | self-test 与 run-dir PASS、A–E 探针全 FAIL；loop §5 anchor 107/105 未同步 |
| 9 开放题 | PASS | PASS | PASS | B-59/B-60 已落盘；B-59 举例已修正 |

总体：**IMPROVED / NOT PASS**。Trace 与 Precision 的 FAIL 已清到 PASS/CONCERN，但 Teaching 仍 FAIL，且 A-14/A-13 的新句与正文冲突、A-17/B-19 未修，所以不能改判 PASS。

### 6. 剩余项（R23 前）

1. A-17 把十条分歧改成两列表格或独立 bullet；B-19 把 §2.6.4 与 §3.7 的“重建活动根”合成一处对照或明确分工；A-05 把动作 2/3 的时间列拆开。
2. A-13 删掉 L2342-2345 的重复落点 bullets（或把 L2332 改成真正的“只讲两道闸”）；A-14 收回“其余计法只在 §3.7/§5.2.3”的过强句，或在 §4.8/§3.7 标题同步改口径；A-20 把 L82/L128/L1331 统一为“五条次序约束”，并补“约束 / 执行位置”两列表。
3. B-08 把 §2.2 的 SSE/ABI/`target-feature` 细节移到 §4.6；B-09 把 §2.3 的 PM 缓存/`PMGETPARAM` 细节移到 PM 篇或明确改边界；B-10 把 §3.4.4 的探针与三层判别移到 §5.2/§5.3。
4. `defect-ledger.md` 的 READER-A/B 行不能写“逐 ID 已处理/均按 panel 建议改正文”：A-17、B-19 未动，A-13/A-14/A-20/B-08/B-09/B-10 只是半修；B-15 行内“记 ③”与 ④ 分档统一。
5. loop §5 的 anchor 改为 105/105（或补回两个 §7.5 符号链接）；B-52 的 `origin` 字段、B-59 的锚点文法仍是 OPEN 机制项。

### 7. 二次复核结论

本轮把 R21/R22 两套计数、R22 §3 ② 映射、R22 §7、FACT-B-01 注记、B-52 标题、B-59 举例都修到了可验收状态，Trace/Precision 的 FAIL 已清。未清的是 Teaching：17 条目标里 9 条全修、6 条半修、2 条未修，另有 A-14/A-13 的新句与正文冲突。按“Teaching 未清且存在新硬错误则保留 NOT PASS”的规则，总体维持 **IMPROVED / NOT PASS**。

VERDICT（二次复核后）: NOT PASS

## 三次复核后记（2026-11-09）

**复核对象**：`ebcc02384ec68404d82af7fa8b17c50e83a4f609`。`BASELINE.txt` 已指向它；HEAD `a78e48fc8` 只是把 BASELINE 再指到 `ebcc02384`。文档 `df8f6755…` / 3407 行，与 BASELINE 一致；工作树对文档与源码无未提交改动。

### 1. 命令与读数

- `python3 tools/eval-trace-check.py --self-test` → PASS，exit 0。
- `python3 tools/eval-trace-check.py <run-dir> --panel-dir .../panel/round22` → `PASS 8 / WARN 0 / FAIL 0`；面板 67；`发现总数与处置之和一致（67）`；`metrics 各分档与台账本轮逐 ID 聚合一致`。
- 探针（`/tmp/r22probe4/runA..E`）：删 R22 台账 exit 1 FAIL；删 R22 loop exit 1 FAIL；总数改 99 exit 1 FAIL；同和拆分 `①14/②3/③5/④45` exit 1 FAIL；同和拆分 `①9/②3/③1/④54` exit 1 FAIL。
- 六门：anchor `105/105`（loop §5 已同步改 105/105）、map `46/46`、ref/structure/style/density 均 exit 0。
- 宿主：`cargo test -p minix-boot --lib` 190 passed；`cargo test -p boot-shim --test ebs_boundary_audit` 5 passed。`ebcc02384` 只改文档/台账，不改代码。
- 复核期间工作树出现 `DECISIONS-PENDING.md` / `DECISIONS-PENDING-R18.md` 的未提交改动（用户侧 §15 三条硬规则，含“轮转闸：①③④ 全部修完且附证据才可派下一轮面板”“② 拖欠须用户批准”）；不是本报告写入。按这三条，当前 5 条 ④ 半修仍属未修，不能进 R23 面板。

### 2. 七项 Teaching 逐条核对

| # | ID | 声称 | 判定 | 证据 |
|---|---|---|---|---|
| 1 | READER-A-17 | 十条分歧每条一行 | 半修 | L1649 的“其二”到“其十”已改独立 bullet；但“其一”仍在引导段内，不是独立 bullet；“每条一行”未完全成立 |
| 2 | READER-A-13 | 删除重复落点 bullet | 全修 | L2339 改为“下表已给五类落点；这里只补充两道内联闸的覆盖差异，不再复述落点”；原 L2342-2345 三条重复 bullet 已删 |
| 3 | READER-A-14 | 删过强句，改主计数口径 | 全修 | L1888 改为“正文以‘五条生产腿’为主计数；其他计法在需要时引用本对账，不改变正文的规范口径”；不再声称“只在两处出现” |
| 4 | READER-A-20 | 全部统一为“五条次序约束” | 半修 | L128、L159 已改；但 L82 仍引“§2.8 的‘步骤级共五条’”，L1329 仍写“**步骤级**次序共五条”；“不再有步骤级混用”不成立 |
| 5 | READER-B-08 | SSE/目标细节移到 §4.6 | 全修 | §2.2 L269 只留 C 判定与“见 §4.6”指针；§4.6 L2837 新增“入口栈对齐与 SSE（承接 §2.2）”整段 |
| 6 | READER-B-09 | PM 缓存细节压成指针 | 全修 | §2.3 L420 改为“PM 缓存细节见 PM 篇；起点 `pm/main.c:169`”，保留读者链，不再展开 `pm/misc.c:352-386` |
| 7 | READER-B-19 | §3.7 指向 §2.6.4 的 TLB 窗口 | 半修 | 新增的交叉引用在 L1614 的 `mem_high_phys` 字段行（§3.4.2），不在 §3.7；§2.6.4 L1121 与 §3.7 L1983 仍各自完整解释“重建活动根”，拆分没有合并 |

**其他核对**：B-15 行内已改“保留两节（分层需要，仍按 ④ 计入，不另记 ③）”，与 ④ 分档一致；loop §5 anchor 已改 105/105，与实跑一致。

### 3. 新出现的硬错误

**§4.2.1 动作表的时间序自相矛盾（新）**：本轮把动作 3–9 的时间列从 3–9 改成 4–10（动作 1 时间 2、动作 2 时间 1/3），但动作 2 的同一句仍写“时间列的 3 与动作 3 同段，不承诺两者先后”；动作 3 的时间列已经是 4。按表读，“时间 3”属于动作 2 的拷贝/模块部分，不属于动作 3。应改为“与动作 2 的拷贝/模块部分同段”，或把动作 3 的时间列改回 3。该句正是 A-05 的残留位置，本轮把 A-05 从“部分”改成了“自相矛盾”。

### 4. 50 条 ④ 的当前状态

- **全修 45 条**；**半修 5 条**：A-05（时间列与动作 3 的矛盾）、A-17（“其一”未独立）、A-20（L82 引文与 L1329 仍写“步骤级”）、B-10（“权威记录”只是措辞，三层判别与探针仍在 §3.4.4，未移 §5.2/§5.3）、B-19（交叉引用在 §3.4.2，不在 §3.7；两节拆分仍在）；**未修 0 条**。
- 台账的 `READER-A-01..20`、`READER-B-01..20` 仍写“逐 ID 已处理/均按 panel 建议改正文”；按上面的 5 条半修，这句话仍然过强。B-15 的 ③/④ 冲突已消，但 B-10 的半修没有被台账写出来。

### 5. 抽样一致性

- 原抽样表不改；原核心 23 条读数仍是 14/23 = 60.9%（历史读数保留）。
- 按 `ebcc02384` 重判核心 23 条：A-15、C-07、A-13、B-08 等已全修，A-05 仍半修，得 22/23 = 95.7%。
- 按报告里原 29 行抽样表重判：A-05、A-17、A-20 三条半修，其余 26 条全修，得 26/29 = 89.7%，**仍低于 90%**。所以即使不看新出的动作表错误，按报告自己的 29 行样本，严格一致率也没有达到 PASS 线。

### 6. 结论变化

| 问 | 初次 | 一次复核后 | 二次复核后 | 三次复核后 | 依据 |
|---|---|---|---|---|---|
| 1 Detection | CONCERN | CONCERN | CONCERN | CONCERN | FACT 拆分仍有效 |
| 2 Precision | FAIL | CONCERN | PASS（残留 B-15） | PASS | B-15 已改回 ④；67 分档与 R21/R22 两套计数均对上 |
| 3 Design | CONCERN | CONCERN | CONCERN | CONCERN | D2-01/05 已修，② 已进 OPEN-DEBT R23；D1 空档仍在 |
| 4 Independence | PASS | PASS | PASS | PASS | RB22 有效；EBS 5 条测试仍真抓旧 bug |
| 5 Scope | CONCERN | CONCERN | CONCERN | CONCERN | ② 全部 R23 已入 OPEN-DEBT；F-R20-04 归类仍可议 |
| 6 Teaching | FAIL | FAIL | FAIL | FAIL | 45/50 全修、5 半修、0 未修；动作表新增自相矛盾 |
| 7 Cost | CONCERN | CONCERN | CONCERN | CONCERN | 成本读数未变 |
| 8 Trace | FAIL | CONCERN | CONCERN | PASS | self-test、run-dir、A–E 探针、anchor 105 均清 |
| 9 开放题 | PASS | PASS | PASS | PASS | B-59/B-60 已落盘 |

总体：**IMPROVED / NOT PASS**。Trace/Precision 的 FAIL 已清到 PASS；Teaching 仍有 5 条半修（其中 A-05 因动作表时间序改动而变成自相矛盾），且 29 行样本严格一致率 89.7% < 90%，所以不能改判 PASS。

### 7. 剩余项（R23 前）

1. 修 A-05 的动作表：把“时间列的 3 与动作 3 同段”改成“与动作 2 的拷贝/模块部分同段”，或恢复动作 3 的时间列 3；顺手把 A-05 的时间列拆成唯一顺序。
2. A-20：L82 的“§2.8 的‘步骤级共五条’”与 L1329 的“步骤级次序共五条”统一成“五条次序约束（D1–D5）”。
3. A-17：把“其一”从引导段移成独立 bullet，真正达到十条逐条可扫。
4. B-10：把 §3.4.4 的探针与三层判别移入 §5.2/§5.3，或明确写“设计章保留探针的理由”并让 §5 不再自称覆盖判别；B-19：把交叉引用放到 §3.7 或 §2.6.4 本身，或合并两节对“重建活动根”的重复解释。
5. 台账 `READER-A/B` 行按 5 条半修改写，不要再用“逐 ID 已处理/均按 panel 建议改正文”覆盖这 5 条。

### 8. 三次复核结论

本轮把 A-13、A-14、B-08、B-09 修到可验收，B-15 注记与 anchor 105 也清；Trace/Precision 的 FAIL 已清。未清的是 Teaching：45 全修、5 半修、0 未修，且 §4.2.1 动作表出现新的时间序自相矛盾；29 行样本严格一致率 26/29 = 89.7%，低于 90%。按“Teaching 未清且存在新硬错误则保留 NOT PASS”的规则，总体维持 **IMPROVED / NOT PASS**。

VERDICT（三次复核后）: NOT PASS

## 四次复核后记（2026-11-09）

**复核对象**：`777f21ce1968036a355f043c99b9902cdbeb4cdb`。`BASELINE.txt` 已指向它；HEAD `ecc2c8aef` 只是把 BASELINE 再指到 `777f21ce1`。文档 `32a6b3ee…` / 3411 行，与 BASELINE 一致；工作树对文档与源码无未提交改动。复核期间另有一笔 `2803a1907`（用户裁决“轮转与修复纪律三条硬规则 + ④ 证据机械闸”），它把 `eval-trace-check.py` 对 ④ 行的复验列也改成必填；这是本轮 Trace 读数的关键变量。

### 1. 命令与读数

- `python3 tools/eval-trace-check.py --self-test` → PASS，exit 0。
- `python3 tools/eval-trace-check.py <run-dir> --panel-dir .../panel/round22` → **FAIL 3，exit 1**：
  - `READER-A-10` 处置类为 ④，但复验列为空或占位（轮转闸：可修项必须有已修证据）
  - `READER-A-12` 同上
  - `READER-A-27` 同上
  这三行是 R15 的历史行（`defect-ledger.md` L997/L999/L1049），处置列写“④ 不改：……”，复验列为“—”。新的 ④ 证据闸是全局的，所以 R22 run-dir 在未改这三行前不会 PASS。`2803a1907` 的提交信息自己写了“D01 真实轨迹即被点名 3 处 ④ 证据缺失 → FAIL（预期信号，主对话须补证据或真修后才能过闸）”，本轮没有补。
- 探针（`/tmp/r22probe5/runA..E`）：A–E 均 exit 1 FAIL；但基线本身已被上面 3 行拦住，探针读数只证明“删台账/删 loop/改总数/同和拆分”仍能被接住，不能把基线洗绿。
- 六门：anchor `105/105`、map `46/46`、ref/structure/style/density 均 exit 0；anchor 与 loop §5 一致。
- 宿主：`cargo test -p minix-boot --lib` 190 passed；`cargo test -p boot-shim --test ebs_boundary_audit` 5 passed。`777f21ce1` 只改文档，不改代码。

### 2. 五条 Teaching 逐条核对

| # | ID | 声称 | 判定 | 证据 |
|---|---|---|---|---|
| 1 | READER-A-05 | 动作 2 时间列与动作 3 一致 | 全修 | L2312 已改“时间列的 3 是动作 2 的拷贝/模块段；动作 3 在时间 4”；动作 3 时间列确为 4，矛盾消失 |
| 2 | READER-A-17 | “其一”改独立 bullet | 全修 | L1651 的“其一”已以 `- ` 开头；其十到其十均为独立 bullet，十条逐条可扫 |
| 3 | READER-A-20 | 全文不再有“步骤级”旧称 | 半修 | L82 引文已改“五条次序约束（D1–D5）”；但 **L1329 仍写“四样之间的**步骤级**次序共五条”**；“全文不再有”不成立 |
| 4 | READER-B-19 | §3.7 加 §2.6.4 交叉引用 | 半修 | L1988 的 §3.7 段已加“C 侧同一‘重建活动根’动作为何能靠 TLB 窗口撑住，见 §2.6.4”；但 §2.6.4 L1121 与 §3.7 仍各自完整解释同一动作，panel 要的“合并/只留一处对照”没有发生 |
| 5 | READER-B-10 | 三层判别移 §5.2.3，§3.4.4 只留指针 | 半修，且新增自相矛盾 | §3.4.4 的 L1713 仍保留“深拷探针读数”整段读数，L1714 才是指针；迁到 §5.2.3 L3127 的句子还写“测试章只引用；放在设计章是因为深拷不变量在这里定义”——它现在就在测试章，且测试章不是“只引用”；应改为“设计章只引用；证据归测试章”或删掉括注 |

**其他**：A-13、A-14、B-08、B-09 上一轮已全修；B-15 仍按 ④；anchor 105/105 已清。

### 3. 50 条 ④ 的当前状态

- **全修 47 条**；**半修 3 条**：A-20（L1329 仍“步骤级”）、B-10（探针读数仍在 §3.4.4；迁入 §5.2.3 的句子仍写“放在设计章/测试章只引用”）、B-19（只加交叉引用，未合并两节）；**未修 0 条**。
- 若把 B-19 的交叉引用视为 panel 允许的替代方案，则 48 全修、2 半修；按“合并两节或反过来”的原始判据，仍记半修。
- 台账 `READER-A-01..20`、`READER-B-01..20` 仍写“逐 ID 已处理/均按 panel 建议改正文”；A-20、B-10、B-19 三条半修没有被写出来。

### 4. 抽样一致性

- 原抽样表不改；原核心 23 条读数仍是 14/23 = 60.9%（历史读数保留）。
- 按 `777f21ce1` 重判原 29 行抽样表：A-05、A-17 已全修，只剩 A-20 半修，得 28/29 = 96.6%，**已超过 90% 的抽样线**。
- 但抽样线不是唯一出口：新的 ④ 证据闸在真实 run-dir 上 FAIL，Teaching 也还有 3 条半修，所以仍不能判 PASS。

### 5. 结论变化

| 问 | 初次 | 一次复核后 | 二次复核后 | 三次复核后 | 四次复核后 | 依据 |
|---|---|---|---|---|---|---|
| 1 Detection | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | FACT 拆分仍有效 |
| 2 Precision | FAIL | CONCERN | PASS | PASS | PASS | 67 分档与 R21/R22 两套计数均对上 |
| 3 Design | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | D2-01/05 已修；D1 空档仍在 |
| 4 Independence | PASS | PASS | PASS | PASS | PASS | RB22 有效；EBS 5 条测试仍真抓旧 bug |
| 5 Scope | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | ② 全部 R23 已入 OPEN-DEBT；F-R20-04 归类仍可议 |
| 6 Teaching | FAIL | FAIL | FAIL | FAIL | FAIL | 47/50 全修、3 半修、0 未修；B-10 新增自相矛盾 |
| 7 Cost | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | 成本读数未变 |
| 8 Trace | FAIL | CONCERN | CONCERN | PASS | **FAIL** | 真实 run-dir 被 ④ 证据闸拦住（R15 的 A-10/A-12/A-27） |
| 9 开放题 | PASS | PASS | PASS | PASS | PASS | B-59/B-60 已落盘 |

总体：**IMPROVED / NOT PASS**。Precision 与抽样率已经过关，但 Trace 因新的 ④ 证据闸重新变红，Teaching 仍有 3 条半修，且 B-10 的迁移引入“放在设计章/测试章只引用”的新自相矛盾。

### 6. 剩余项（R23 前）

1. **先让真实 run-dir 变绿**：把 R15 的 `READER-A-10/A-12/A-27` 从 ④ 改为 ③（它们本来就是“不改”的裁决，应有反证句），或给 ④ 行补可重放证据（旧串 grep 归零/文档五门）。不补这三行，新的轮转闸会一直红。
2. A-20：L1329 的“步骤级次序共五条”改成“五条次序约束（D1–D5）”。
3. B-10：把 L1713 的“深拷探针读数”整段也移到 §5.2.3；把 L3127 的“测试章只引用；放在设计章”改成“设计章只引用；证据归测试章”或删掉括注。
4. B-19：要么把 §2.6.4 的 C 侧解释压成 §3.7 的对照框（或反向合并），要么在 DECISIONS 里明确“交叉引用即本条的验收形态”，否则仍按半修。
5. 台账 `READER-A/B` 行按 A-20/B-10/B-19 三条半修改写，不要再用“逐 ID 已处理/均按 panel 建议改正文”覆盖。

### 7. 四次复核结论

本轮把 A-05、A-17 修到可验收，A-20/B-10/B-19 仍是半修；更关键的是 `2803a1907` 新增的 ④ 证据闸把真实 run-dir 判 FAIL（3 处 R15 历史行复验列为“—”）。Trace 与 Teaching 都未清，按“FAIL 未清则保留 NOT PASS”的规则，总体维持 **IMPROVED / NOT PASS**。抽样率 28/29 = 96.6% 已过线，但不能替代机械闸与 Teaching 的完整验收。

VERDICT（四次复核后）: NOT PASS

## 五次复核后记（2026-11-09）

**复核对象**：`de2a982f8b99d8d8efcff09bdf910820d0e56379`。`BASELINE.txt` 已指向它；HEAD `76924e99b` 只是把 BASELINE 再指到 `de2a982f8`。文档 `dceab2ac…` / 3413 行，与 BASELINE 一致；工作树对文档与源码无未提交改动。

### 1. 命令与读数

- `python3 tools/eval-trace-check.py --self-test` → PASS，exit 0。
- `python3 tools/eval-trace-check.py <run-dir> --panel-dir .../panel/round22` → **PASS 8 / WARN 0 / FAIL 0**，exit 0；`发现总数与处置之和一致（67）`；`metrics 各分档与台账本轮逐 ID 聚合一致`。④ 证据闸不再点名 R15 三行。
- 探针（`/tmp/r22probe6/runA..E`）：删 R22 台账 FAIL、删 R22 loop FAIL、总数改 99 FAIL、同和拆分 `①14/④45` FAIL、`③1/④54` FAIL；exit 1 均符合预期。
- 六门：anchor `105/105`、map `46/46`、ref/structure/style/density 均 exit 0。
- 宿主：`cargo test -p minix-boot --lib` 190 passed；`cargo test -p boot-shim --test ebs_boundary_audit` 5 passed。`de2a982f8` 只改文档/台账，不改代码。

### 2. 五项逐条核对

| # | ID | 声称 | 判定 | 证据 |
|---|---|---|---|---|
| 1 | R15 `READER-A-10/A-12/A-27` | ④ 改 ③，复验列写反证句 | 机械闸通过；语义仍需留意 | L997/L999/L1049 已改 ③，复验列分别为“保留一行计数史，无需改动”“索引已消歧，无需改动”“保留一行 commit 史，无需改动”。反证句是“为什么保留”的理由，不是源码/测试/文档原文式的误报反证；作为“不改裁决”更像 ② 或 ④，建议后续在 DECISIONS 明确 |
| 2 | READER-A-20 | L1329 改“五条次序约束（D1–D5）”，全文无“步骤级” | 全修（带 P3 冗余） | `grep -n '步骤级'` 零命中；但 L1329 写成“四样之间的**五条次序约束（D1–D5）**共五条”，出现“五条…共五条”的重复，建议改为“四样之间的次序约束共五条（D1–D5）” |
| 3 | READER-B-10 | §3.4.4 探针读数移 §5.2.3，§3.4.4 只留指针 | 半修，新增悬空引用 | 探针读数确已移到 §5.2.3 L3126；但 §3.4.4 L1711 仍写“本篇用**下面这枚探针**做了初步排除（……见**探针读数后的三层结论**）”，而 §3.4.4 下面已没有探针；§3.4.4 也没有 §5.2.3 指针。唯一指针是 §5.2.3 L3127“（三层判别与探针读数的权威记录在 §5.2.3。）”，它在 §5.2.3 内自指。应把这句话移到 §3.4.4 末尾，或把 L1711 改成“见 §5.2.3” |
| 4 | READER-B-19 | §3.7 加“两节分工，不合并” | 全修 | L1987 已写“两节分工：§2.6.4 讲 C 侧的 TLB 窗口，本节讲 Rust 侧为何禁止重建活动根；不合并”。这是对 panel“章节边界模糊”的明确分工声明，可验收 |
| 5 | 台账 R22 A/B 行 | 已按最终状态改写为逐 ID 说明 | 未落实 | de2a 的 ledger diff 只改了 R15 三行（L997/L999/L1049），R22 的 A 行 L1493 仍写“A-05 时间列仍与动作 3 同段（……按部分处理）”，与 de2a 已修相反；B 行 L1494 仍写“B-01..B-20 均按 panel 建议改正文”，没有写 B-10 半修。用户所称“A/B 行已改写”没有发生 |

**其他**：A-05（动作 2 时间列与动作 3 时间 4 一致）、A-17（“其一”独立 bullet）、A-13、A-14、B-08、B-09、C-05、C-08 均已全修；B-15 仍按 ④；anchor 105/105 已清。

### 3. 50 条 ④ 的当前状态

- **全修 49 条**；**半修 1 条**：B-10（探针读数已移 §5.2.3，但 §3.4.4 的“下面这枚探针/见探针读数后的三层结论”失去指向，且没有 §5.2.3 指针；§5.2.3 的指针自指）；**未修 0 条**。
- 台账 R22 A 行仍把 A-05 写成“部分处理”，是陈旧记录；B 行没有写 B-10 的半修。机械闸只校验“复验列非空”，所以这两处文本滞后不会被工具抓住。

### 4. 抽样一致性

- 原抽样表不改；原核心 23 条读数仍是 14/23 = 60.9%（历史读数保留）。
- 按 `de2a982f8` 重判原 29 行抽样表：A-05、A-17、A-20 均已全修，得 29/29 = 100%。
- 但 B-10 不在 29 行样本里，它仍是半修——这正是 B-60“抽样驱动修复”的形态：样本 100%，样本外的 B-10 还差一个指针。

### 5. 结论变化

| 问 | 初次 | 一次复核后 | 二次复核后 | 三次复核后 | 四次复核后 | 五次复核后 | 依据 |
|---|---|---|---|---|---|---|---|
| 1 Detection | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | FACT 拆分仍有效 |
| 2 Precision | FAIL | CONCERN | PASS | PASS | PASS | PASS | 67 分档与 R21/R22 两套计数均对上 |
| 3 Design | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | D2-01/05 已修；D1 空档仍在 |
| 4 Independence | PASS | PASS | PASS | PASS | PASS | PASS | RB22 有效；EBS 5 条测试仍真抓旧 bug |
| 5 Scope | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | ② 全部 R23 已入 OPEN-DEBT；F-R20-04 归类仍可议 |
| 6 Teaching | FAIL | FAIL | FAIL | FAIL | FAIL | **FAIL** | 49/50 全修、1 半修（B-10 指针）；A-20 有 P3 冗余 |
| 7 Cost | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | 成本读数未变 |
| 8 Trace | FAIL | CONCERN | CONCERN | PASS | FAIL | **CONCERN** | 机械闸 PASS；R22 A/B 行文本仍滞后（A-05 写成部分、B-10 未写半修） |
| 9 开放题 | PASS | PASS | PASS | PASS | PASS | PASS | B-59/B-60 已落盘 |

总体：**IMPROVED / NOT PASS**。Precision 与抽样率 100% 已过关，Trace 机械闸也 PASS；但 B-10 的指针迁移留下“下面这枚探针”悬空引用，Teaching 仍 FAIL；台账 R22 A/B 行仍与文档现状相反，Trace 记 CONCERN。

### 6. 剩余项（R23 前）

1. **B-10 指针（唯一实质性阻断）**：把 §5.2.3 L3127 的“（三层判别与探针读数的权威记录在 §5.2.3。）”移到 §3.4.4 L1711 末尾；或把 L1711 的“下面这枚探针/见探针读数后的三层结论”改成“见 §5.2.3”。同时删掉 §5.2.3 里的自指句。
2. **台账 R22 A/B 行**：A 行把 A-05 从“部分处理”改成“已修（动作 2 时间 3 属拷贝/模块段，动作 3 在时间 4）”；B 行补 B-10 的“探针读数已移 §5.2.3，§3.4.4 指针待补”。用户所称“已按最终状态改写”应落到这两行。
3. A-20 的 P3 冗余：L1329“五条次序约束（D1–D5）共五条”改为“四样之间的次序约束共五条（D1–D5）”。
4. R15 三行的 ③ 归类：若它们是“不改裁决”，更贴近 ②（需用户批准并落账）或 ④ 加证据；当前“反证句”只是保留理由，不是真正的误报反证。可在 DECISIONS 里补一句裁决，避免下次又被 ④ 证据闸问住。

### 7. 五次复核结论

本轮把 R15 三行、A-20、B-19 修到可验收，真实 run-dir 与探针均 PASS。剩下的唯一实质问题是 B-10 的指针：探针读数移走了，§3.4.4 还留着“下面这枚探针”，而 §3.4.4 没有 §5.2.3 指针；§5.2.3 的指针又在自指。加上台账 A/B 行文本滞后，Teaching 仍 FAIL、Trace 记 CONCERN，总体维持 **IMPROVED / NOT PASS**。把 B-10 一句话和 A/B 两行台账补上，即可进入 PASS 判定。

VERDICT（五次复核后）: NOT PASS

## 六次复核后记（2026-11-09）

**复核对象**：`917f3eb97fe9baa2386cae7d6caffc6e1c119a1e`。`BASELINE.txt` 已指向它；HEAD `aee20423d` 只是把 BASELINE 再指到 `917f3eb97`。文档 `1540d5ad…` / 3412 行，与 BASELINE 一致；工作树对文档与源码无未提交改动。

### 1. 命令与读数

- `python3 tools/eval-trace-check.py --self-test` → PASS，exit 0。
- `python3 tools/eval-trace-check.py <run-dir> --panel-dir .../panel/round22` → **PASS 8 / WARN 0 / FAIL 0**，exit 0；67 条；`发现总数与处置之和一致（67）`；`metrics 各分档与台账本轮逐 ID 聚合一致`；④ 证据闸不再报任何行。
- 探针（`/tmp/r22probe7/runA..E`）：删 R22 台账 FAIL、删 R22 loop FAIL、总数改 99 FAIL、同和拆分 `①14/④45` FAIL、`③1/④54` FAIL；exit 1 均符合预期。
- 六门：anchor `105/105`、map `46/46`、ref/structure/style/density 均 exit 0。
- 宿主：`cargo test -p minix-boot --lib` 190 passed；`cargo test -p boot-shim --test ebs_boundary_audit` 5 passed。`917f3eb97` 只改文档/台账/DECISIONS，不改代码。

### 2. 本轮六项核对

| # | 项 | 判定 | 证据 |
|---|---|---|---|
| 1 | B-10 指针归位 | 全修 | §3.4.4 L1711 改为“本篇用本仓探针做了初步排除（读数与三层判别见 §5.2.3）”；§5.2.3 的自指句已删；§5.2.3 L3128 写“设计章只引用；证据归本节” |
| 2 | §5.2.3 自指删除 | 全修 | `grep '三层判别与探针读数的权威记录在 §5.2.3'` 零命中；§5.2.3 不再有自指句 |
| 3 | 台账 R22 A 行 | 全修 | L1493 已写“A-05 已改动作 2 时间列：时间 3 属拷贝/模块段，动作 3 在时间 4” |
| 4 | 台账 R22 B 行 | 全修 | L1494 已写“B-10 探针读数已移 §5.2.3，§3.4.4 留指针”；B-15 仍按 ④、不另记 ③ |
| 5 | A-20 去冗余 | 全修 | L1329 改为“四样之间的次序约束共五条（D1–D5）”，不再有“五条…共五条”；`grep '步骤级'` 零命中 |
| 6 | D-025 | 已落账 | `DECISIONS.md` 新增 D-025：R15 三行确认为“不改裁决”，改 ③ 并在复验列写保留理由；④ 证据闸对 ③ 不要求代码/测试反证；实现状态“已实施”，reopen 条件“新证据显示该三项应修” |

### 3. 50 条 ④ 与抽样

- 50 条 ④ 当前：**全修 50、半修 0、未修 0**。
- 原抽样表不改；原核心 23 条读数仍是 14/23 = 60.9%（历史读数保留）。最终按 `917f3eb97` 重判原 29 行抽样表为 **29/29 = 100%**，核心 23 条为 **23/23 = 100%**。
- 局限仍适用：同 agent 验证 + 已重放命令；QEMU 只读日志未独立复跑；宿主 190/135/5/868/287 为实跑读数。原 60.9% 与这些局限都保留在报告正文与 §15。

### 4. 结论变化

| 问 | 初次 | 一次复核后 | 二次复核后 | 三次复核后 | 四次复核后 | 五次复核后 | 六次复核后 | 依据 |
|---|---|---|---|---|---|---|---|---|
| 1 Detection | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | FACT 拆分仍有效；非阻断观察 |
| 2 Precision | FAIL | CONCERN | PASS | PASS | PASS | PASS | PASS | 67 分档与 R21/R22 两套计数可复算 |
| 3 Design | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | D2-01/05 已修；D1 停跑空档仍在 |
| 4 Independence | PASS | PASS | PASS | PASS | PASS | PASS | PASS | RB22 有效；EBS 5 条测试真抓旧 bug |
| 5 Scope | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | ② 全部 R23；F-R20-04 归类仍可议 |
| 6 Teaching | FAIL | FAIL | FAIL | FAIL | FAIL | FAIL | **PASS** | 50/50 全修；B-10 指针归位；A-20 去冗余 |
| 7 Cost | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | CONCERN | 成本读数未变 |
| 8 Trace | FAIL | CONCERN | CONCERN | PASS | FAIL | CONCERN | **PASS** | run-dir、④ 证据闸、A–E 探针、anchor 105 全绿；D-025 落账 |
| 9 开放题 | PASS | PASS | PASS | PASS | PASS | PASS | PASS | B-59/B-60 已落盘 |

总体：**IMPROVED / PASS**。本轮把 B-10 指针、A-20 冗余、台账 A/B 两行、D-025 都补到可验收；Teaching/Trace/Precision 的 FAIL 均已清，未发现新的硬错误。Detection、Design、Scope、Cost 仍是 CONCERN（D1 停跑、B-52 的 `origin` 机制、B-59 的锚点文法、F-R20-04 的跨 crate 归类、成本未采集），列为后续观察项，不阻断本轮 PASS。

### 5. 六次复核结论

R22 的阻断项已闭合：50 条 ④ 全修，真实 run-dir 与探针全 PASS，67=①9+②3+③5+④50 可复算，D-025 把 R15 三行的 ③ 定性落成用户裁决。按“Teaching/Trace/Precision FAIL 已清且无新硬错误则 PASS”的规则，改判 **PASS**；原抽样 60.9% 作为历史读数保留，最终 29/29 = 100%。

VERDICT: PASS
