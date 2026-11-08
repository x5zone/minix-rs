# 01-boot-shim-bootstrap Round 19 meta-review（冷启动）

- 评审对象：`runs/01-stage-kernel/01-boot-shim-bootstrap/` 的 R19 段（`loop.md:943-999`、`defect-ledger.md:1349-1398`、`metrics.md` 的 R19 块）、`follow-ups.md`、`OPEN-DEBT.md`、`BASELINE.txt`；七份面板原始输出 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round19/`；独立回读 `panel/round19-rb.md`；上一轮 `meta/01-boot-shim-bootstrap-r18.md`；`prompt/review-rules/review-evals.md`、`review-panel.md`；`BLIND-SPOTS.md`、`WORKFLOW-VERSIONS.md`、`DECISIONS.md`。
- 时间线（`git log --format='%h %ci %s'`、`stat`、`git show` 重放）：
  - 面板读取基线 `87ceaaad6`（2026-11-09 05:12:06），文档 3386 行、sha256 `755b762e0a…`，与 `BASELINE.txt` 的 `r19_panel_base_*` 逐项吻合。
  - 七份面板输出落盘 05:15–05:29（design-d1 05:15、reader-a 05:17、design-d2 05:21、reader-b 05:24、reader-c 05:25、fact 05:27、refute 05:29）。
  - 第一版修复 `23a001e40`（05:33:54），文档 3387 行、sha256 `417b3ba9…`；RB19 读的就是这一版。
  - RB19 补修 `a140bb6a0`（05:44:45），文档 3387 行、sha256 `6c13d722…`；`BASELINE.txt` 的 `r19_fix_head`、`r19_fixed_doc_sha256`、`r19_fixed_doc_lines` 与 `git show a140bb6a0:` 及工作树逐项相同；冻结提交 `14e516d64`（05:44:59）。
  - 当前 `HEAD=14e516d64`，`git status --porcelain` 只有未跟踪的 `prompt/resources/`。
- 未采信：主 agent 的 `loop.md` / 台账自评只作线索，每条结论回源到面板原文、文档、C 源或实际命令。`expected/`、`traps/`、其他篇目输出未读。
- 局限声明：**同 agent 验证 + 已重放命令**。本报告是冷启动子 agent，但与主 agent 同模型；抽样不是随机，刻意覆盖“① 带残留”“④ 群组行”“③ 分类语义”“轨迹门探针”四类可疑点。

## 结论速览

**`IMPROVED`，运行质量 `PASS`（二次复核后）。**

进步在产出层：R19 用四条独立冷读抓到 R18 修复批自己写进去的槽号算术错误（`961+k-1` → 应为 `961+k`），抓到 R18 接收侧复核的 release 回绕（D2-11）并落了 `checked_add` 与两条负控；面板误报率从 R18 的约 10.5% 降到约 1%；`BASELINE.txt` 与最新 fix head 逐项吻合；六门、`minix-boot` 188、`boot-shim --lib` 135、`ebs_boundary_audit` 4 均可重放，逐文件计数闭合。

以下第一至九节是**初次评审（`a140bb6a0` 时点）的原始读数**，保留不改。`7dbd07f6` 与 `11200ab55` 后的两次复核读数与结论变化见末尾“复核后记（2026-11-09）”与“二次复核后记（2026-11-09）”。

初评点名的三条，加上 `7dbd07f6` 引入的第四条，现已全部关闭：

1. **删除探针只有 2/3 转红**（初评）→ 已修：删 R19 loop 段现报 `FAIL loop-sections`；四组探针（删 R18 台账、删 R19 台账、删 R19 loop、篡改 R19 指标）全部 FAIL。
2. **指标分档不可复算**（初评）→ 已修：R19 台账与 `metrics.md` 都给出 ① 13 / ② 0 / ③ 14 / ④ 53 / ⑤ 0 = 80；R18 台账与 `metrics.md` 都给出 ① 21 / ② 0 / ③ 21 / ④ 56 / ⑤ 0 = 98。
3. **`READER-A-09` 未修**（初评）→ 已修：§6.1 复算命令补了 `os/kernel/src/lib.rs`、`os/kernel-image/src/main.rs`、`os/kernel-image/src/bootface*.rs`，并如实标注“串前缀约 10/16 组”。
4. **R18 台账 READER-B 范围被误改**（`7dbd07f6` 引入）→ 已修：`11200ab55` 改为 `READER-B-01..14、READER-B-16..20`（排除 B-15、保留 B-12），R18 段展开为 98 个唯一 ID。

剩余是 CONCERN 与 P3，不构成 FAIL：Precision 的分母仍含 10 条 D1 方案否决与若干跨角色重复（B-52 仍 OPEN）；Independence 的面板期间工作树漂移未消除（B-53 仍 OPEN）；Scope 的 `23a001e40` 提交信息仍写“R19 全修”；Cost 的 token 仍“未采集”；§3.4.1 图例列 `R14/R16/R17/R18` 而未列 `R19`（有“等”字，属可选 P3）。

## 一、Detection — PASS

### 1.1 本轮抓到的、R18 漏检的实例

| 历史盲区 | R19 实例 | 证据 |
|---|---|---|
| 取槽/下标算术（新类别） | FACT-01、READER-A-01、READER-B-01、READER-C-01 四源同靶：§2.6.3 正常路径写作 `p_cr3_v[961+k-1]` | 该句在 `7a36a9253` 不存在，由 R18 修复批 `cba3558de` 新写入；`pg_mapkernel` 返回首个空闲槽（`minix3/minix/kernel/arch/i386/pg_utils.c:186-202`），`memory_init` 把原值写进 `freepdes[0]`（`memory.c:707-718`），故正常值是 `961+k`，`961+k-1` 是本文 1115 行反事实里的错误值 |
| B-34 代码改动后文档引文未回读（复发） | FACT-02..06、REFUTE-01/02/05/06 | FACT-02 路标前缀；REFUTE-01 “七个都消费”改“五个有读者 + 两个布局位”（`os/rs/src/trap_api.rs:226`、`os/is/src/dump_kernel.rs:572-575`）；REFUTE-02 “恰好三条”改五条；REFUTE-05 `kernel_identity_mmio_regions` 不是 Step 1 清单；REFUTE-06 `init` 调用者枚举漏两个直核面（`bootface.rs:272`、`bootface_a64.rs:256`） |
| B-50 修复句自身成为缺陷源（复发） | 三条 ① 直接由 R18 修复批产生：槽号句、D2-11（R18 新增复核用裸 `+`）、D2-12（R18 加复核但门表未同步） | `git show cba3558de` 与 `87ceaaad6` 对读；`kernel_info.rs:327-348` 的 `kend/me/pe` 现均 `checked_add`，负控 `:827/:835` |
| B-09 门绿但事错（仍 OPEN） | RB19 抓到 `protect.c` 区间被改成 358–361；正确是 360–363 | `nl -ba minix3/minix/kernel/arch/i386/protect.c \| sed -n '345,366p'`：358 是 `*/`、359 空行、360–363 是四步 |

### 1.2 四源同靶说明什么

A-01 / B-01 / C-01 / FACT-01 同一处错误被四个材料隔离的角色分别抓到，说明两件事：

1. **冷读冗余对“本地算术 + 同文档自相矛盾”有效。** 该句与文档 1115/1117/1119 行直接冲突，读者角色只读正文即可判定，不需要源码；FACT 用 `pg_utils.c`/`memory.c` 独立确认。四源不是复述，是四个独立证据链。
2. **工作流的真正缺口不是“类别不够”，而是“上一轮新写的句子没有进下一轮的强制回读面”。** 该句是 R18 修复批新增的，R18 修复后只有 RB18 读过（未抓），R18 面板在修复前读过（看不到），R18 meta 的抽样也没覆盖。R19 的四源之所以能抓到，是因为他们读的是全篇冷读，不是修复 diff。把“修复新增/改写的句子清单”作为下一轮 RB 与 FACT 的固定输入，比再加一个读者角色更省。

### 1.3 仍然漏的类别

- **运行期缺陷仍为 0 条新增。** D2-11 是 release 回绕导致的校验门退化为恒真，属真实代码加固，但不是已发生的运行期故障；本轮没有 UB、竞态、内存破坏或可达 panic 的新证据。
- **“④ 群组行静默漏项”没被任何门抓到。** `READER-A-01..20` 一行承载 20 个 ID，19 条处置里少 A-09，轨迹门与六门都不检查“范围行内的每个 ID 是否有对应处置文字”。这是本轮最具体的漏检，见第六问。
- **指标分档漂移没有探测器。** 轨迹门只校验“总数 = 五项之和”，不校验 ③/④ 分档与台账逐 ID 归属一致。

## 二、Precision — CONCERN

- **12 条 D2 里 10 条是 D1 方案否决，不是本仓文档/代码缺陷。** D2-01..10 的每条“位置”都指向 `design-d1.md` 的决策或 `MINIMAL-DESIGN`，不指向 `01-boot-shim-bootstrap.md` 的任何一句。把它们计入“发现 80”使可修面高估 12.5%。这与 R18 的 12/98（12.2%）同构，说明口径没有改。
- **至少 8 个 ID 是跨角色重复，没有 `dup-of` 合并。** 四源同靶 `FACT-01/A-01/B-01/C-01`（−3）；六套计数 `A-02/B-03/C-06`（−2）；三条 v1 发送腿 `B-04/C-07`（−1）；§1.4 边界名单 `B-10/B-14`（−1）；M6/TODO-05 `B-12/C-03`（−1）。按 ID 去重后约 72 条；再去掉 10 条 D1 方案，本仓唯一可修面约 62 条。`metrics.md` 仍只报原始 80，没有去重数（B-52 仍 OPEN）。
- **误报率本身很低。** 80 条里我能找到的“证据自带错误”只有一处：面板 FACT-06 把正确区间写成 358–361（实际 360–363），主 agent 照抄后由 RB19 抓到。约 1/80 = 1.3%。REFUTE-03/04/07 是“反证失败、主张成立”，记 ③ 正确，不是误报。这比 R18 的约 10.5% 明显改善。
- **③ 桶语义仍混装。** 13（台账 14）条 ③ 里同时有：D1 方案否决 10、反证失败 3、以及 `READER-B-12` 一条“已由 C-03 的修复解决却记不改”。三种东西混在一个数里，精度指标不能直读。

结论：Precision 判 CONCERN。问题是分母与分类语义，不是面板乱报。

## 三、Design quality — PASS（附“D1 应降频”结论）

### 3.1 D2-11 是真实加固

- `os/libs/minix-boot/src/kernel_info.rs:327-350`：`kend = kbase.checked_add(kern_size)`、`me = mb.checked_add(len)`、`pe = pb.checked_add(prev.len)`，三处都 `.expect(...)`；负控 `test_validate_rejects_kernel_span_overflow`（`:827`）与 `test_validate_rejects_module_span_overflow`（`:835`）真实构造 `u64::MAX` / `u64::MAX-1` 溢出，`cargo test -p minix-boot` 188 通过。
- 攻击面成立：`os/Cargo.toml` 的 `[profile.release]` 未开 `overflow-checks`，裸 `+` 回绕后 `kend < kbase`，`me <= kbase || mb >= kend` 退化为恒真，正是该门宣称要挡的“伪造载荷把模块放到内核身上”。`cargo test -p minix-boot span_overflow` 两条均通过。
- RB19 指出的第三处 `pe = pb + prev.len` 也已在 `a140bb6a0` 改成 `checked_add`；D2-11 的字面判据（该段无裸 `+`）现在成立。

### 3.2 D1 连续两轮被 D2 判 NOT PASS，应降频而不是全停

- D1 的 5 条决策在 R18、R19 两轮里落到本仓的接受项为 0；两轮唯一被接受的代码改动（R18 D2-10、R19 D2-11/12）都出自 D2 对现仓的审计，不是 D1 的方案。
- 但 D1 不是“命名/注释级意见”：D2-01/02/05 指出的现仓锚点错、`startup_objects` 重复、低窗口上界不可计算，都是实质设计判断。按 `review-evals.md` §6B 第 5 条，这不触发“设计能力未成立”。
- 结论：**不要停跑对抗，但要停跑“每轮全量 D1 提案”。** 建议 D1 改为每两轮一次，或在架构变更落地时触发；每轮保留 D2 的“现仓设计审计”半边。理由是本篇架构已冻结（D-013..D-019），D1 的提案边际收益接近零，而 D2 的审计每轮都产出真实代码项。这是下一轮的动作 3。

## 四、Independence — CONCERN

- **面板读取面：6/7 份报告头写了基线 sha/行数。** fact/reader-a/b/c/refute/design-d2 都写 `87ceaaad6`、3386 行、`755b762e…`；design-d1 没写。比 R18（0/7）改善，B-53 只做了一半。
- **但面板期间工作树被改了，违反 C-10 “面板期间不改文档”。** 面板自己留下了证据：`fact.md:29-49` 记录会话末 `git status` 多出文档（5 hunk/11 行）、`kernel_info.rs`（32 行）、`loop.md`（11 行），文档 sha 从 `755b762e…` 变成 `f17d944b…`；`refute.md:4` 记录另一个中间态 `74a13229…`（3387 行）并说 `kernel_info.rs` 已加两条溢出负控。也就是说 R19 面板至少看到了三个文档状态。`loop.md §1` 只写“面板前后 `git status` 只有主 agent 的改动”，没有记录“面板期间”的写入，读者会以为冻结面没动。
- **影响可核性，但不影响面板结论。** FACT 明确以基线提交为准并标注了漂移；REFUTE 逐句核对了工作树新句。因此“面板读的是冻结面”在证据层成立，“面板期间冻结面稳定”不成立。
- **RB19 独立有效，但不是强独立。** 它读 `23a001e40`（不是最终 `a140bb6a0`），抓到 `protect.c` 区间这一条真阻断和三条 P2/P3，说明不是橡皮图章；但同 agent。它自己也有一处错观察：`round19-rb.md` 的 FACT-02 行把源码位置写成 `os/kernel-image/src/main.rs:387`，实际全仓 `grep -rn 'DM ok, jumping to kmain' os/ --include=*.rs` 只命中 `os/kernel/src/lib.rs:396`；台账 RB19 段把它记 ③“保留原证据”是正确的，RB 报告与台账在这里不一致。

结论：Independence 判 CONCERN。冷读独立性在发现层很强（四源同靶 + REFUTE 四条独立反证），扣分在“冻结面在面板期间被改”与同 agent。

## 五、Scope discipline — CONCERN

- **相邻项没有阻塞。** `adjacent 阻塞 0`；`F-R14-04` 指 02 篇立项轮，`F-R18-03/04` 是 R19 观察；D1 的方案问题记 ③ 而不是 ②，没有把设计稿分歧升级成本篇阻塞。
- **② 债的 R19 截止日集体过期，没有重排。** `OPEN-DEBT.md` 的 `F-R17-02`、`F-R17-04` 写“截止 R19”；`follow-ups.md` 的 `F-R17-01`（待用户裁决，截止 R19）、`F-R17-03`（“剩余 Phase 栈化作为 R19 重构”）同样写截止 R19。R19 收口时四条都还在原状态，没有被关闭、重排或标逾期。`loop.md §7` 的 G5 只写“无 ② 阻塞”，回避了“日期已过”。
- **D1 方案问题没有被当成本文档阻塞，这点做对了。** D2-01..10 记 ③，D2-11/12 落码，边界清楚。
- **“R19 全修”再次过宽。** `23a001e40` 的提交信息是“R19 全修”，但该提交的 FACT-06 区间是错的（RB19 判 NOT PASS 才发现），且 READER-A-09 从未修；`a140bb6a0` 只补了 RB19 的六项，A-09 仍不在其中。最终树不是“全修”。这与 R18 的“全修”同名问题复发。

结论：Scope discipline 判 CONCERN。没有扩大责任边界，但 ② 截止日与“全修”标签让范围账比实际干净。

## 六、Teaching quality — PASS

抽样 24 条 ④（A 8、B 8、C 8），逐条回源。**真实理解/一致性障碍 23 条，纯文风噪声 0–1 条（约 0–4%）**，远低于 R18 的 22%，也低于 20% 的过拟合警报线。

真实障碍的代表：

- A-01 / B-01 / C-01 槽号自相矛盾（见第一问）。
- A-02 / B-03 / C-06：§1.4 索引说“六套计法”只列五套；三源同靶说明入口索引确实会误导。
- A-03：§2.8 说“五条步骤级次序”，正文只给三条条目；修复后明写“下面三条给论证、另两条只给指向”。
- A-04：§1.1 阶段表把 C/x86-32 的“无栈无堆无中断”写成整篇前提；修复后表内限定 `C/x86-32 路径` 并加 Rust UEFI 状态指向。
- B-04 / C-07：§3.4.1 “两条 v1 发送腿”与全文“三条 v1 发送腿”冲突；修复后改“三条 v1 发送腿（两条 UEFI 链 + U-Boot shim）+ 两个构造入口”。
- C-04：`alloc_lowest` 把 `lowest == 0`（表格第一行写“行为正确”）并进“前两种缺陷形态”；修复后改“后两种缺陷形态（`0 < lowest < len` 与 `len ≤ lowest`）”。
- C-08：§3.10 把 `unpaged` 回收锚点写成 §2.6；修复后改 §2.4.2。
- C-13：`checked_shape` 的覆盖范围写“所有非 `RefuseToBoot` 描述符”；修复后改“所有通过 Step 1 的描述符”，与 Step 1 报错即终止一致。

唯一的边界样本：C-14“内核当参数搬运工，参数语义留给用户态”对 `rootdevname` 成立、对 `cttyline`/`board=` 不成立。这是结论过宽（精度问题），不是读者理解障碍，但也不是纯文风噪声。

修复忠实度：抽样的 24 条 ④ 里 23 条能在当前文档找到对应改动；**A-09 是唯一一条改了台账、没改文本的**（见第七问半修清单）。A-05 的图例补了 R14/R16/R17/R18，但 §5.2.1 新写了“R19 的两条跨度溢出负控”，图例没跟上——属 P3 残留，不是原缺陷未修。

结论：Teaching quality 判 PASS。

## 七、Cost — CONCERN

| 指标 | R17 | R18 | R19 |
|---|---:|---:|---:|
| 角色报告 | 7 | 7 | 7 |
| 原始发现 | 87 | 98 | 80 |
| ① 本轮修 | 26 | 21 | 13 |
| ④ 本轮修 | 48 | 56 | 54 |
| ③ 非缺陷/否决 | 10 | 21 | 13（台账 14） |
| 独立回读 | 无 | RB18 | RB19 |
| 可观测时间线 | 约 2.5 h（自述） | 03:46–05:12 ≈ 86 min | 05:12–05:45 ≈ 33 min |
| `metrics.md` 自述耗时 | 约 2.5 h | 约 4 h | **约 4.5 h** |

- **自述耗时与可观测时间线相差一个数量级。** R19 的面板文件落盘 05:15–05:29、修复提交 05:33、RB19 05:42、冻结 05:44，文件活动跨度约 33 分钟；`metrics.md` 写“约 4.5 小时（面板 + 修复 + 三门）”。要么这个数是累计会话时间，要么是未支持的估计。R18 是 86 分钟，也不支持“4 小时”。这是 Cost 读数自身的完整性问题。
- **单位成本上升。** R19 每条 ① 修复摊到约 2.5 分钟面板时间（13 条 / 33 分钟），R18 约 4.1 分钟（21 条 / 86 分钟）；但 R19 的 ① 里有 3 条是上一轮修复自伤，真正的新外部缺陷约 10 条。按“新外部 ①”计，每条约 3.3 分钟，比 R18 的 4.1 分钟略好，但绝对产出从 21 降到 10。
- **收益递减信号明确。** 本轮 80 条、13 ①、0 条运行期缺陷、2 条 D1 方案否决占 12.5%、8 个重复 ID、G8 仍是“新增 80 远大于 0”。按 R18 meta 自设的判据（同基线 80+ 条且无新运行期缺陷 → 递减），本轮踩线；因 D2-11 是真实代码加固，留 CONCERN 不判 FAIL，但下一轮同量级即应判 FAIL。
- token 连续第三轮“未采集”；`loop.md §1` 仍无并发数；这些仪表缺口与 R18 相同。

## 八、Trace integrity — FAIL

> 初评读数（`a140bb6a0`）。`7dbd07f6` 后 R19 三组探针 FAIL、R19 指标与台账可复算；`11200ab55` 后四组探针（含删 R18 台账）全部 FAIL，R18/R19 两个 panel-dir 均 `PASS 8 / WARN 1 / FAIL 0`，本项判 PASS。见末尾两节后记。

### 8.1 删除探针（副本在 `/tmp/meta-r19-probe/`，仓库只读）

命令：`python3 tools/eval-trace-check.py /tmp/meta-r19-probe/<p> --panel-dir /home/xzhao/github/minix-rs/tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round19`

| 探针 | 操作 | 读数 | 期望 | 判定 |
|---|---|---|---|---|
| 基线 | 原样 | `PASS 8 / WARN 1 / FAIL 0`；`panel: 80 条`；`metrics: 80`；`ledger: 1197` | PASS | PASS |
| A | 删 `defect-ledger.md` 的整段 `## R19 面板轮`（1349 行至文件末） | `FAIL coverage: 面板发现没有处置行: CLAIM-07`；`ledger 1112` | FAIL | FAIL 成立 |
| B | 删 `loop.md` 的整段 `## R19 面板轮`（943 行至文件末） | `PASS 8 / WARN 1 / FAIL 0`；`ledger 1197` | FAIL | **未变红** |
| C | 把 R19 指标改成 `发现总数: 999` / `①1 ②0 ③1 ④1 ⑤0` | `FAIL metrics: 发现总数 999 与五项处置之和 3 不一致` | FAIL | FAIL 成立 |

- **探针 B 的根因**：`tools/eval-trace-check.py:117-131` 的 `check_loop_sections(round_tag="R19")` 先找 `^## .*R19.*$` 的“面板轮”标题；找不到时 `headings` 为空，代码不报错，直接 `text` 保留全文，于是 R18 及更早轮次的 §1–§8 让检查通过。删除整段本轮 loop 是最彻底的篡改，却是唯一仍然 PASS 的探针。
- **探针 A 的通过带有偶然性**：`coverage` 的轮次作用域是 `scoped = {… if "r19" in row["section"]}`，但随后 `coverage_ledger = scoped or ledger_set`——`scoped` 为空时退回全文件台账。只有 `CLAIM-07` 这个前几轮从未用过的 ID 让门失败；若 80 个 ID 都有历史同名行，删整张 R19 台账仍会 PASS。这是与探针 B 同源的“空集回退”洞。
- **`BLIND-SPOTS.md` 的 B-51 标 `CLOSED` 不成立**：其期望探测器明写“删本轮台账/loop 段/篡改本轮指标必须 FAIL”，实测 loop 段删除仍 PASS。B-51 应改回 OPEN/PARTIAL。

### 8.2 指标与台账对账

- 总数自洽：`16 + 20 + 20 + 15 + 7 + 12` 面板展开为 80；`metrics` 的 80 = ①13 + ②0 + ③13 + ④54 + ⑤0。
- **分档不自洽。** 把 R19 台账行按 ID 展开：① = FACT-01、FACT-02、FACT-03..06（4）、READER-B-01、REFUTE CLAIM 01/02/05/06（4）、DESIGN-D2-11、DESIGN-D2-12 = **13**；③ = `READER-B-12`（1）+ REFUTE CLAIM 03/04/07（3）+ DESIGN-D2-01..10（10）= **14**；④ = READER-A-01..20（20）+ READER-B 除 B-01/B-12 外的 18 条 + READER-C-01..15（15）= **53**。13+14+53 = 80。`metrics.md` 的 ③13/④54 把 `READER-B-12` 同时算进 ③（台账行）与 ④（“READER-B 19”），一处两算。`loop.md §3` 的 ③ 列举只有 REFUTE 3 + D2 10 = 13，漏掉 B-12，与台账直接矛盾。
- `loop.md §7` 写“RB19/meta R19 待跑”，但 `loop.md §5` 已写“RB19 判 NOT PASS 并已补修”。同一份文件里 RB19 既“已判”又“待跑”，是收口时没有同步 §7。
- `loop.md §8` 写“本轮 80 条里 13 条 ③ 是 D1 否决”；实际 D1 否决是 10 条（D2-01..10），另 3 条是反证失败。

### 8.3 六门与测试读数

| 项 | R19 §5 声称 | RB19 读数 | 本次重放 |
|---|---|---|---|
| anchor | 108/108 | 108/108 | 未重跑，沿用 RB19 与 R18 meta 的一致读数 |
| doc-code-map | 46/46 | 46/46 | 未重跑 |
| style / ref / density / structure | 通过 | 通过 | 未重跑 |
| `minix-boot` | 188 | 188 | `cargo test -p minix-boot` → **188 passed** |
| `boot-shim --lib` | 135 | 135 | `cargo test -p boot-shim --features test-all --lib` → **135 passed** |
| `ebs_boundary_audit` | 4 | 4 | `cargo test -p boot-shim --features test-all --test ebs_boundary_audit` → **4 passed** |
| `minix-kernel --features mock` | 860 + 8 ignored（868） | 未跑 | 未执行；`grep -rh '#\[test\]' os/kernel/src/*.rs \| wc -l` = **868**，真实 `#[ignore =` 属性 = **8**（另有 3 处文档注释提及 `#[ignore]`，不计），与 860+8 吻合 |
| `minix-arch` | 287 | 未跑 | 未执行 |
| QEMU 三门 | PASS | 未跑 | 未重放，按主 agent 自述 |

- §5.2.1 的逐文件表可复算：boot-shim `22+11+12+44+17+15+13+1 = 135`；minix-boot `25+8+8+73+2+10+21+29+2+5+5 = 188`；`minix-elf` 23；`ebs_boundary_audit` 4；`boot_storage.rs` 5。全部与 `grep -c '#[test]'` 相同，无漂移。
- 归因更正：`minix-kernel` 的 868 在文档里标的是“R18 实测”，R19 只是沿用；`loop.md §4` 把它列在 R19 的测试清单里，措辞上像本轮重跑，实际没有 R19 的运行证据。

### 8.4 冻结面与 BASELINE

- `BASELINE.txt` 的 R19 三项与 `a140bb6a0` / 工作树逐项吻合（sha `6c13d722…`、3387 行、fix head `a140bb6a0`），R18 meta 点名的“BASELINE 未更新”已消。R19 面板基线 `87ceaaad6` / `755b762e…` / 3386 也吻合。
- 残留：设计 D1 报告没有 sha/HEAD；面板期间工作树漂移（§4）；C-10 的“面板与修复分离”在 R19 被违反。

结论：Trace integrity 判 FAIL。判据不是“有 WARN”，而是“删掉整段本轮 loop 仍 PASS”与“分档数不可复算”。

## 九、本轮最值得、但工作流仍看不见的一件事

**工作流看不见“上一轮修复新增/改写的句子”这一层身份：它既没有把修复 diff 作为下一轮冷读的强制输入，也没有 `origin` 字段区分“本轮新发现 / 上轮残留 / 上轮修复自伤”。**

R19 的三个硬数据：

1. 13 条 ① 里有 3 条（槽号句、D2-11、D2-12）是 R18 修复批自己写进去的；槽号句被四个角色抓到，D2-11/12 由 D2 抓到，但没有一条是“因为知道它是上轮新写的”而定向检查出来的。
2. `READER-A-09` 被记 ④ 已修，但文本没改。范围行 `READER-A-01..20` 把 20 个 ID 压成一行，没有任何门检查“每个 ID 是否都有对应的文本改动或 ③ 理由”。
3. 8 个跨角色重复与 10 条 D1 方案否决仍然计入“新发现 80”，G8 的“连续两轮无新 in-scope 发现”因此永远达不到。

可执行的仪器（下一轮动作 1/2）：让每轮修复提交生成一份“changed sentences + 锚点”清单，作为下一轮 RB 与 FACT 的固定输入；台账加 `origin` 与 `dup-of`，`metrics.md` 同时报原始/去重/上轮残留三个数。

## 抽样一致性

- 抽样 39 条：13 条 ① 全覆盖，24 条 ④（A 8 / B 8 / C 8），2 条 ③；占 80 条的 48.8%，④ 抽样占 54 条的 44.4%，均高于 20%。每条都重放了 `grep`/`sed`/`nl -ba`/`cargo test`/`git show`。
- 判据：不看“旧串是否消失”，看“缺陷成立条件是否消失”或“③ 的反证是否成立”。

| # | ID | 角色 | 处置 | 复判 | 判据/证据 |
|---|---|---|---|---|---|
| 1 | FACT-01 | fact | ① | PASS | 文档 1078 行现为 `p_cr3_v[961+k]`；C 侧 `pg_utils.c:186-202`、`memory.c:707-718` 支持 |
| 2 | FACT-02 | fact | ① | PASS | 文档 3186 行带 `kernel-image:`；`grep -rn 'DM ok, jumping to kmain' os/ --include=*.rs` 只命中 `os/kernel/src/lib.rs:396`，台账证据 `lib.rs:396` 正确 |
| 3 | FACT-03 | fact | ① | PASS | §4.4 引文头（2502 行）声明略去 `minix_boot::`/`minix_types::` 限定符 |
| 4 | FACT-04 | fact | ① | PASS | §3.3 1521 行注 `bitflags` 从略，旧“完整”措辞已去 |
| 5 | FACT-05 | fact | ① | PASS | §4.2 2202 行标两处未展开段 |
| 6 | FACT-06 | fact | ① | PASS（经 RB19 补修） | 文档 1123 行 `protect.c:360-363`；`nl -ba` 360–363 正是四步 |
| 7 | READER-B-01 | reader-b | ① | PASS | 同 FACT-01 |
| 8 | REFUTE-01 | refute | ① | PASS | §3.4.2 1635 行“五个有读者 + 两个 C 布局位”；`rs/src/trap_api.rs:226`、`is/src/dump_kernel.rs:572-575` 佐证 |
| 9 | REFUTE-02 | refute | ① | PASS（附 P3） | 五条分歧与实现同源；但“`drops_tail_records_whole_at_capacity` 锁住前者”过强，见半修清单 |
| 10 | REFUTE-05 | refute | ① | PASS | §4.6 2782 行“不是 Step 1 窗口清单；x86 分支为空”；`os/kernel/src/lib.rs:907-928` 佐证 |
| 11 | REFUTE-06 | refute | ① | PASS | §4.7 2853 行“两个直核面在自己的 bootface 初始化并注册”；`bootface.rs:272`、`bootface_a64.rs:256` |
| 12 | DESIGN-D2-11 | design-d2 | ① | PASS | `kernel_info.rs:329-350` 三处 `checked_add`；`:827/:835` 两条负控；188 通过 |
| 13 | DESIGN-D2-12 | design-d2 | ① | PASS | §4.4 门链第 4 项与表 2590 行“`checked_add` + 信任边界第二道复核”；表体 12 行 |
| 14 | READER-A-01 | reader-a | ④ | PASS | 1078 行 `961+k` |
| 15 | READER-A-02 | reader-a | ④ | PASS | §1.4 132 行补“架构×链五行 5”，六套齐 |
| 16 | READER-A-03 | reader-a | ④ | PASS | §2.8 1331 行“下面三条给论证、另两条只给指向” |
| 17 | READER-A-04 | reader-a | ④ | PASS | §1.1 表“引导准备（C/x86-32 路径…Rust UEFI…见 §3.1）” |
| 18 | READER-A-08 | reader-a | ④ | PASS | §7.5 3373 行“没有同名对应物…只角色相似” |
| 19 | READER-A-09 | reader-a | ④ | **FAIL** | §6.1 3194 行复算命令仍只扫两个 boot-shim 文件；表含 `kernel:` 与 `kernel-image` 行；台账 A 行 19 条处置无 A-09，也未记 ③ |
| 20 | READER-A-19 | reader-a | ④ | PASS | §4.2.1 2305 行 startup 写入记为 6b |
| 21 | READER-A-20 | reader-a | ④ | PASS | §3.8 2065 行“第 1 行是初始化、不注册；第 2 行两次；第 3 行一次” |
| 22 | READER-B-02 | reader-b | ④ | PASS | §1.2 74 行“D5 是建表动作内部”，执行序含 `（D5）` |
| 23 | READER-B-04 | reader-b | ④ | PASS | §3.4.1 1601 行“三条 v1 发送腿 + 两个构造入口” |
| 24 | READER-B-06 | reader-b | ④ | PASS | §3.5 1762 行“见本节下文的过度回收风险表” |
| 25 | READER-B-07 | reader-b | ④ | PASS | §5.1 M2 2969 行写取整/零长丢弃/跨 `LIMIT` 截断，与 §2.4 一致 |
| 26 | READER-B-10 | reader-b | ④ | PASS | §1.4 138 行补 §5.2.3 `entering scheduler` 边界 |
| 27 | READER-B-13 | reader-b | ④ | PASS | §4.9 2938 行“`build_kernel_info`（仅直核面）”，ledger 声称的文本存在 |
| 28 | READER-B-17 | reader-b | ④ | PASS | §2.4.1.4 705 行只留指针，128/64 已删 |
| 29 | READER-B-19 | reader-b | ④ | PASS | §7.5 3366 行“不保证穷举” |
| 30 | READER-C-02 | reader-c | ④ | PASS | §4.1 2192 行“构造 `BootReport`…发送侧不构造 `KernelInfo`” |
| 31 | READER-C-03 | reader-c | ④ | PASS | §5.1 M6 改“TODO-05 只登记 1 GiB 首选档；全量走查当前未登记” |
| 32 | READER-C-04 | reader-c | ④ | PASS | §2.4.1.3 696 行改“后两种缺陷形态（`0 < lowest < len` 与 `len ≤ lowest`）” |
| 33 | READER-C-08 | reader-c | ④ | PASS | §3.10 2147 行“（§2.3/§2.4.2）” |
| 34 | READER-C-10 | reader-c | ④ | PASS | §3.3 1532 行“用上面那条 `sed` 就能一次验证” |
| 35 | READER-C-12 | reader-c | ④ | PASS | §1.4 138 行把 GET_KINFO 载荷指向 §3.4.2 |
| 36 | READER-C-13 | reader-c | ④ | PASS | §4.2.2 2325 行“所有通过 Step 1 的描述符” |
| 37 | READER-C-15 | reader-c | ④ | PASS | §4.4 2577 行“十二行；第 4 项含两变体，门列五个标签” |
| 38 | READER-B-12 | reader-b | ③ | **FAIL** | 台账记 ③“M6 已改…不再复用”，但 M6 的改动正是本轮对 C-03 的修复；B-12 的缺陷条件已消失，应记 ④，且它是 C-03 的重复（`dup-of`）；同时被汇总重复计进 ④ |
| 39 | REFUTE-03/04/07 | refute | ③ | PASS | 三条主张经 C 源、链接脚本、wire 注释核对成立，保留不改正确 |

- 严格一致：**37/39 = 94.9%**（FAIL 2、无 CONCERN）；实质一致 37/39 = 94.9%。按 `review-evals.md` §5.4/§9 的“低于 90% 写 NOT PASS”，抽样本身过线；整体 NOT PASS 由第八问的轨迹门 FAIL 与 A-09/B-12 的处置错决定。
- 抽样偏向：13 条 ① 全查；④ 里刻意选了 A-09（群组行漏项）、B-12（③ 分类）、B-13（RB19 点名的半修）、C-15（计数口径）四个最可疑点。

## 半修／整修清单

| 项 | 声称 | 实际 | 判定 |
|---|---|---|---|
| FACT-06 `protect.c` 区间 | R19 面板给 358–361，主 agent 照抄进 `23a001e40` | 实际 360–363；RB19 抓到，`a140bb6a0` 改成 360–363 | 半修→整修（最终成立）；面板证据本身错误 |
| READER-B-13 §4.9 文本 | 台账称“§4.9 行加仅直核” | `23a001e40` 时无此文本；RB19 抓到；`a140bb6a0` 在 2938 行补“（仅直核面）” | 半修→整修 |
| REFUTE-02 测试断言 | 文档称 `drops_tail_records_whole_at_capacity`“锁住”后续记录被拒 | 测试补了 `!out.windows(4).any(BBBB/CCCC)`，但 32 字节缓冲下 `ARCH=x86_64`(13)+A(13)=26，B(13) 已放不下，C 即使尝试也放不下；该断言无法区分“丢弃其后全部”与“逐条继续尝试”。REFUTE 的附记已指出这一点 | **半修（文案强度高于测试强度）**；建议加一条“B 放不下、C 能放下”的用例，或把“锁住”改成“覆盖该分支” |
| READER-A-09 §6.1 复算命令 | 台账按 ④ 汇总，未单列 | 命令未改，表仍含内核侧/接收侧行；台账 A 行无 A-09，也无 ③ | **未修记已修**（比半修更重） |
| READER-B-12 TODO 一号两用 | 台账 ③“不改” | M6 已按 C-03 改成“TODO-05 只登记 1 GiB”，缺陷条件消失；且与 C-03 重复 | **分类错 + 重复计数**：应记 ④ 并在 C-03 上 `dup-of` |
| READER-A-05 图例 | 台账“图例补 R14–R18/A+D/裁决 A” | 图例确实补了；但 §5.2.1 3050 行新写“R19 的两条跨度溢出负控”，图例没有 R19 | 原缺陷已修；P3 残留（建议图例改 `R<nn>` 通配） |
| D2-11 第三条裸 `+` | RB19 指 `kernel_info.rs:347` `pe = pb + prev.len` | `a140bb6a0` 已同改 `checked_add` | 已修 |
| `BASELINE.txt` | R18 meta 说未跟到最新 | 现指向 `a140bb6a0` / `6c13d722…` / 3387，与提交及工作树一致 | 已修 |
| `BLIND-SPOTS.md` B-51 | 标 `CLOSED`（“删本轮台账/loop 段/篡改指标必须 FAIL”） | 删台账 FAIL、改指标 FAIL、**删 loop 段 PASS** | **CLOSED 不成立，应改 OPEN/PARTIAL** |
| F-R17-01..04 截止 R19 | `loop.md §0` 说“按 follow-ups 档期” | 四条截止 R19 全部过期，未关闭、未重排、未标逾期 | **截止日失守** |
| `loop.md §7` | 写“RB19/meta R19 待跑” | §5 已写 RB19 判 NOT PASS 并补修 | 收口未同步 |
| `metrics.md` 耗时 | R19 约 4.5 小时 | 可观测文件活动 05:12–05:45 ≈ 33 分钟；R18 为 86 分钟 | **读数不可复现** |

## 给下一轮的具体动作（≤5 条）

1. **补轨迹门的“空集不许回退”规则（最高优先）。** `check_loop_sections` 在 `round_tag` 存在但找不到本轮标题时，应直接 FAIL，而不是退回全文；`coverage` 的 `scoped or ledger_set` 在 `scoped` 为空时必须 FAIL。用本轮三个探针 + 一个“删最后一行 loop 段”的变体做自测，四个都必须红。同时把 `BLIND-SPOTS.md` B-51 从 `CLOSED` 改回 OPEN，直到探针 B 转红。
2. **台账加 `origin`/`dup-of`，并把范围行改为逐 ID 行。** 至少标出 `FACT-01/A-01/B-01/C-01`、`A-02/B-03/C-06`、`B-04/C-07`、`B-10/B-14`、`B-12/C-03` 五组重复；`READER-A-01..20` 等范围行展开为 20 行，每行给文本锚点或 ③ 理由。`metrics.md` 同时报“原始 80 / 去重 72 / D1 方案 10 / 唯一可修 62”。
3. **D1 降频，D2 保留每轮。** 本篇架构已冻结，D1 的 5 条决策两轮接受 0 条；改为每两轮一次或架构变更时触发，每轮只跑 D2 的现仓设计审计。省下的一个角色预算给“修复 diff 定向回读”。
4. **把“上一轮修复新增/改写的句子”变成 RB 与 FACT 的固定输入。** 每轮修复提交生成 `changed sentences + 锚点` 清单，RB 逐句对源码与文档内其他断言；本轮槽号句、D2-11、D2-12 三条都会在这张清单上。`READER-A-09` 这类“记了没改”也能由“每个 ④ ID 必须有一个文本 hunk 或 ③ 理由”的机械检查挡住。
5. **补两条小口子。** (a) §6.1 的复算命令扩到 `os/kernel/src/lib.rs` 与 `os/kernel-image/src/main.rs`，或把表拆成 shim/内核两张各配命令（A-09）；(b) `drops_tail_records_whole_at_capacity` 增加“B 放不下、C 能放下”的用例，使“丢弃其后全部”与“逐条继续尝试”可区分（REFUTE-02 的文案—测试强度）。

## BLIND-SPOT-DELTA

| 盲区 | 状态 | R19 证据 |
|---|---|---|
| B-03 跨节矛盾 | **DETECTED（继续）** | 槽号（1078 vs 1115）、§3.4.1 “两条 v1 腿” vs 全文“三条”、§3.10 §2.6 vs §2.4.2、§4.4 两处“覆盖” |
| B-08/B-14 计数与读数漂移 | **DETECTED（继续）** | §1.4 五列说六套（A-02/B-03/C-06）；§4.4 十行/四门/五个标签（C-15） |
| B-12 引文块被改写 | **DETECTED（弱化）** | FACT-03/04/05 只剩引文头口径，字段/顺序逐字正确 |
| B-17/B-26 非 FINDING 面板逃出处置 | **PARTIAL** | 轨迹门已能解析 `- **FACT-nn**` 与 `## CLAIM nn`（面板 80 条全解析）；但范围行仍把 20 个 ID 压成 1 个 token，A-09 静默漏项 |
| B-31 台账无轮次作用域 | **PARTIAL（改善）** | 删 R19 台账已 FAIL；但 `scoped or ledger_set` 的空集回退仍在，失败靠 `CLAIM-07` 这个新 ID |
| B-34 代码改动后文档引文未回读 | **DETECTED／复发** | FACT-01/02..06、REFUTE-01/02/05/06；其中槽号句是 R18 修复自伤 |
| B-40 指标三层不同数 | **仍 OPEN（新实例）** | 总数 80 自洽；分档 ③13/④54 vs 台账 ③14/④53；R19 耗时 4.5h vs 可观测 33min |
| B-44 判决不落账 | **仍 OPEN** | `DECISIONS.md` 没有 RB19 的 NOT PASS 行，也没有 R19 的 D1 不采纳行（R18 meta 动作 3 未执行） |
| B-47 欠账源自身不收敛 | **PARTIAL（退步）** | `F-R17-01..04` 截止 R19 全部过期未处理；`OPEN-DEBT.md` 仍列 F-R17-02/04 |
| B-49 严重度地板为零 | **PARTIAL** | 仍无运行期缺陷；D2-11 是 release 校验回绕的真实代码加固 |
| B-50 半修记整修 | **DETECTED（RB19 接住 1 条）；新 FAIL 1 条** | FACT-06、B-13 由 RB19 接住；**A-09 未修记已修**，比半修更重 |
| B-51 轨迹门轮次作用域 | **REGRESSED 于账面** | `BLIND-SPOTS.md` 标 CLOSED，实测删本轮 loop 段仍 PASS |
| B-52 无 `origin`/`dup-of` | **仍 OPEN** | R19 台账仍无 origin；至少 8 个重复 ID 与 10 条 D1 方案未分账 |
| B-53 面板读取状态无机械锚 | **PARTIAL** | 6/7 报告头有 sha/行数；面板期间工作树被改（`f17d944b`、`74a13229`），无脚本比对 |
| B-01/B-02 设计死代码/翻译残留 | **DETECTED（守住）** | D2-01/02/03/07/08/09 的实质判断；D1 的方案无新增死抽象落地 |
| B-05 假反事实/前提过强 | **DETECTED（1 条）** | C-14 “参数语义留给用户态” 对 `cttyline`/`board=` 不成立 |
| B-09 门绿但事错 | **仍 OPEN（2 条）** | §6.1 复算命令不可复现全部行（A-09）；`drops_tail…` 的“锁住”断言不区分行为 |
| B-10 重复已裁决问题 | **OK** | `re-litigation 0`；D-013..D-019 未被重提 |
| B-11 范围蔓延 | **OK** | `adjacent 阻塞 0`；F-R14-04、F-R18-03/04 按 adjacent 登记 |
| B-35 合并读者角色 | **OK** | A/B/C 齐全，20/20/15 |
| B-46 表结构/跨文档链接门 | **CLOSED** | 本轮六门含 `doc-structure-check.py`，0 问题 |
| B-48 撤诉式修复 | **未触发（仍 OPEN）** | 本轮没有把断言改成“未复核” |
| B-06/B-07/B-13/B-15/B-16/B-18/B-19..B-30/B-32/B-33/B-36..B-39/B-41/B-42/B-45 | **本轮无新证据，未重估** | 不在 R19 射程内 |

**新增候选盲区（建议下一轮 L6.5 落盘，最多两条）：**

- **B-54 范围行把多 ID 压成单 token，允许静默漏项。** 案例：`READER-A-01..20` 一行 19 条处置，A-09 既没修也没记 ③，轨迹门与六门都不报。期望探测器：范围行必须展开为逐 ID 行；每个 ID 要么有文本锚点，要么有 ③ 理由；`eval-trace-check.py` 拒绝未展开的范围行。
- **B-55 分档指标不可由台账复算。** 案例：总数 80 自洽，③/④ 分档与台账差 1（B-12 一处两算），R19 耗时 4.5h 与可观测 33min 差一个数量级。期望探测器：`metrics.md` 由台账逐 ID 生成（总数、五类、去重数、耗时由 `git log`/`stat` 取），轨迹门比对逐类归属，不只比对总数。

## 总评与局限

- 本轮是产出层的小幅进步（四条冷读抓到上轮修复自伤、D2-11 真实加固、误报率降到约 1%、BASELINE 正确），但装置层的两个老洞没有关上（删 loop 段仍 PASS、范围行静默漏项），并新增两处账面错（B-12 分类、耗时读数）。因此 `IMPROVED` 与 `NOT PASS` 同时成立。
- 本报告是**同 agent 验证 + 已重放命令**；六门中我只重放了 anchor 之外的宿主测试与计数，没有重跑 QEMU、`minix-kernel --features mock` 与 `minix-arch`；这三项按主 agent 自述处理，不构成对它们的独立确认。
- 本报告未读 `expected/`、`traps/` 与其他篇目的面板输出；未修改仓库任何文件，探针全部在 `/tmp/meta-r19-probe/`。

## 复核后记（2026-11-09）

- 复核对象：`7dbd07f64422d786530f7129d682ca5d351ac6e1`（2026-11-09 05:56:09，`fix(01): R19 meta 补修——轨迹门缺轮即 FAIL/A-09 复算/B 计数/容量测试`），基线记录提交 `bb788156d`（05:56:14）。当前文档 3387 行、sha256 `f9ddbc562f81862b1033165e07a0f1c5346d06c9da97da406796cb60f8ae1275`；`git show 7dbd07f64:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 的 sha/行数与工作树逐项相同。`BASELINE.txt` 的 `r19_fix_head=7dbd07f64422d786530f7129d682ca5d351ac6e1`、`r19_fixed_doc_sha256=f9ddbc56…`、`r19_fixed_doc_lines=3387` 与提交及工作树一致。
- 局限不变：**同 agent 验证 + 已重放命令**。原抽样表与 37/39 = 94.9% 的读数按要求保留，不按新时点重算；下文单独给当前树的重判。

### 命令与读数

| 项 | 命令 | 读数 | 结论 |
|---|---|---|---|
| 轨迹门自测 | `python3 tools/eval-trace-check.py --self-test` | `self-test PASS: 正确样本通过，缺陷样本被拦截` | PASS |
| 真实 run-dir | `python3 tools/eval-trace-check.py <run-dir> --panel-dir .../panel/round19` | `PASS 8 / WARN 1 / FAIL 0`；`panel: 80 条`；`metrics: 80`；`ledger: 1196 条处置` | PASS |
| 探针 A：删 R19 台账表（1349→EOF） | 同上，副本 `/tmp/meta-r19-probe2/p1` | `FAIL coverage: 面板发现没有处置行: CLAIM-07`；`ledger 1111` | FAIL（符合预期） |
| 探针 B：删 R19 loop 段（943→EOF） | 同上，副本 `/tmp/meta-r19-probe2/p2` | `FAIL loop-sections: loop.md 缺少 R19 面板轮小节` | FAIL（符合预期） |
| 探针 C：R19 指标改 `999` / `1/0/1/1/0` | 同上，副本 `/tmp/meta-r19-probe2/p3` | `FAIL metrics: 发现总数 999 与五项处置之和 3 不一致` | FAIL（符合预期） |
| R19 计数复算 | 按 ID 展开 `defect-ledger.md:1349-1385` | ① 13 / ② 0 / ③ 14 / ④ 53 / ⑤ 0 = 80；`metrics.md` 的 R19 块逐项相同 | PASS |
| `minix-boot` | `cargo test -p minix-boot` | `188 passed; 0 failed` | 与 §5.2.1 一致 |
| 新容量测试 | `cargo test -p minix-boot drops_tail_records_whole_at_capacity` | `1 passed` | PASS（见下） |

### 三项点名补修的复核

1. **轨迹门缺轮即 FAIL：成立。** `tools/eval-trace-check.py` 的 `check_loop_sections` 现在在 `round_tag` 存在但找不到本轮标题时直接 `report.fail("loop-sections", "loop.md 缺少 R19 面板轮小节")`；探针 B 复现 FAIL。删台账表与改指标仍是 FAIL。
2. **计数：R19 已可复算。** 台账 `READER-B` 段现为逐 ID 行（B-01 ①、B-02..B-11 ④、B-12 ③、B-13..B-20 ④），③ = B-12 + REFUTE 03/04/07 + D2-01..10 = 14，④ = A 20 + B 18 + C 15 = 53，① = 13，总数 80；`metrics.md` 同步为 ①13/②0/③14/④53/⑤0。R19 的“总数 = 五项之和”与“分档 = 台账逐 ID 归属”这两层都成立。
3. **A-09：文件面已覆盖，pattern 面有 P3 残余。** §6.1 的复算命令现在是两条：boot-shim 一组，另加 `grep -n 'boot_stage!\|### minix-rs kernel-image' os/kernel/src/lib.rs os/kernel-image/src/main.rs os/kernel-image/src/bootface*.rs`。按表内 16 组里程碑串逐条比对，命中 10 组；`param_buf bytes=`（`uefi_helpers.rs:956`，宏下一行）、`memmap source:`（`:786/:794`）、`kernel: arch_boot entered`（`lib.rs:285`，`Console::write_str`）、`kernel: v0 enter`/`v4 …`（`vmark!`）、`kernel-image: DM ok, jumping to kmain`（`lib.rs:395-396`，作为 `resume_additive` 实参）、直核面 `param_buf bytes=… first=…`（`bootface.rs:314`/`bootface_a64.rs:294`，`say()`）仍不在输出里。原因是 pattern 按“发射宏”匹配，而这几处走别的宏或裸字符串。原缺陷的“只扫 boot-shim 两个文件”已消失，故 A-09 本身判 PASS；注释“覆盖表内全部来源”过强属 P3，建议把 pattern 改成字符串前缀 `boot-shim: \|kernel: \|kernel-image: `。
4. **REFUTE-02 测试：现在是可区分的。** `os/libs/minix-boot/src/boot_params.rs:276-288` 改用 24 字节缓冲与 `"A=1 BBBBBBBBBBBBBBBBBBBB=2 C=1"`：生成的 `ARCH=x86_64`（13 字节）+ `A=1`（4 字节）= 17，22 字符的大记录放不下 → `dropped`；`C=1` 若被继续尝试只占 4 字节（17+4=21 ≤ 24）会落进缓冲，故 `assert!(!out.windows(3).any(|w| w == b"C=1"))` 能区分“丢弃其后全部”与“逐条继续尝试”。测试 1 passed，套件 188 passed。
5. **F-R17-01..04：截止已顺延 R20。** `follow-ups.md:95-98` 与 `OPEN-DEBT.md:9-10` 均写 R20；`loop.md §7` 补了“R19 的‘全修’只指 in-scope 面板项，不含这四项 ②”。

### 新发现：R18 台账的 READER-B 范围被改错（阻断）

- `git show 14e516d64:` 的 R18 段是 `READER-B-01..20`；`7dbd07f6` 改成 `READER-B-01..11、READER-B-13..20`，排掉的是 `READER-B-12`。但 R18 的 ③ 是 `READER-B-15`（`defect-ledger.md:1324`，单复数）。R19 的 ③ 才是 B-12（`:1369`）。这条改动把 R19 的排除项误用到 R18 段。
- 后果：R18 面板的 `READER-B-12`（§4.3 开篇 vs §5.1 M11，`panel/round18/reader-b.md:198`）在 R18 段没有处置行；`READER-B-15` 既在 ④ 范围内、又由单行 ③ 精化覆盖。把 R18 段按范围展开，唯一 ID 是 97 条（①21 + ③21 + ④55），与 R18 的 `metrics.md` 98 差 1。轨迹门的 `ledger: 1196 条处置` 也比补修前的 1197 少 1，少的正是 B-12。该行的处置文字仍包含 B-12 的修复（“§4.3 开篇改‘没有编译期检查’”），但 ID 范围把它排除了——同一行自相矛盾。
- 轨迹门的 `round18` 复核没有报这条：`coverage` 的 `scoped` 用 `"r18" in section.lower()`，而 R19 的标题是“R19 面板轮（2026-11-09，R18 收口后…）”，子串命中把 R19 行也算进 R18 作用域，R19 的 B-12 行顶替了 R18 缺失的行。这是工具的子串匹配缺陷，不是这条台账错的理由。
- 最小修复：把 R18 段的范围改成 `READER-B-01..14、READER-B-16..20`（排除 B-15，保留 B-12），或恢复 `READER-B-01..20` 并保留 B-15 的单行 ③ 精化。R19 段不动。

### 其余 P3 残留（不单独阻断，但应在下一轮清零）

- `loop.md §3` 仍写 `④ 54 条：… READER-B 19`、`③ 13 条`；与 `metrics.md` 的 ③14/④53 相反。`§8` 仍写“13 条 ③ 是 D1 否决”（实际 D1 否决 10 条 + 反证失败 3 条 + B-12 1 条）。`§7` 仍写“RB19/meta R19 待跑”，与 §5 的“RB19 判 NOT PASS、meta R19 判 IMPROVED/NOT PASS”矛盾。
- R19 台账 `FACT-03..06` 行的处置文字仍写“§2.6.4 行号改 358–361”；最终文档（`1123` 行）与 RB19 子表都是 `360-363`。
- `metrics.md` 的 R19 “回读”行仍写“RB19 与 meta R19 待提交后执行”。
- `coverage` 的 `scoped or ledger_set` 空集回退仍在：探针 A 之所以转红，是因为 `CLAIM-07` 这个 ID 前几轮从未出现；若 80 个 ID 都有历史同名行，删整张 R19 台账仍会 PASS。
- 轮次标签子串匹配（`R18` 命中 `R19 … R18 收口后`）让 `--panel-dir round18` 的作用域被 R19 行污染。

### 结论变化

- **初次评审的三条不通过理由，前两条与第三条主体均已修**：删 R19 loop 段现 FAIL、三组探针全 FAIL；R19 的 ③14/④53 与台账逐 ID 可复算；A-09 的文件覆盖已补；REFUTE-02 的测试现在可区分两种行为；F-R17 截止顺延 R20；BASELINE 指向 `7dbd07f6`。
- **R19 自身的 Trace integrity 与计数转 PASS。** 但复核仍判 **NOT PASS**，唯一阻断是 `7dbd07f6` 把 R19 的“排除 B-12”误用到 R18 段，造成 R18 的 `READER-B-12` 失去处置行、R18 台账 97 ≠ metrics 98。这是同一台账文件里的新账实不符，按 `review-evals.md` §八第 1、2 项不能放过。
- 原抽样表的两个 FAIL（A-09、B-12）在当前树上按“缺陷成立条件是否消失”重判：A-09 的文件面已消失（P3 pattern 残余）、B-12 的缺陷条件已消失且 R19 已单独记 ③；若按当前树重算，严格一致会从 37/39 变为 39/39。但按合同保留初次评审的 94.9% 读数，不改原表。
- 修掉 R18 范围这一行（并把上面 P3 清掉）后，下一轮复核可以判 PASS；当前不判。

## 二次复核后记（2026-11-09）

- 复核对象：`11200ab55f3bb9ea38710c484bfacb39d0e1ec81`（2026-11-09 06:06:49，`fix(tools): 轨迹门按节保留 ID 集合；修 R18 B 范围与 R19 计数/P3`），基线记录提交 `38c9176d4`（06:06:54）。当前文档 3387 行、sha256 `e6de33a09250244a9fe162726bdadc939affb1bc97f0bc5643c96abf279d3259`；`git show 11200ab55:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 的 sha/行数与工作树逐项相同。`BASELINE.txt` 的 `r19_fix_head=11200ab55f3bb9ea38710c484bfacb39d0e1ec81`、`r19_fixed_doc_sha256=e6de33a0…`、`r19_fixed_doc_lines=3387` 与提交及工作树一致。
- 局限不变：**同 agent 验证 + 已重放命令**。原抽样表与 37/39 = 94.9% 的初次读数按要求保留；下文单独给当前树的重判。

### 命令与读数

| 项 | 命令 | 读数 | 结论 |
|---|---|---|---|
| 轨迹门自测 | `python3 tools/eval-trace-check.py --self-test` | `self-test PASS: 正确样本通过，缺陷样本被拦截` | PASS |
| round19 run-dir | `python3 tools/eval-trace-check.py <run-dir> --panel-dir .../panel/round19` | `PASS 8 / WARN 1 / FAIL 0`；`panel: 80`；`metrics: 80`；`ledger: 1196` | PASS |
| round18 run-dir | 同上，`--panel-dir .../panel/round18` | `PASS 8 / WARN 1 / FAIL 0`；`panel: 98`；`metrics: 98`；`ledger: 1196` | PASS |
| 探针 A：删 R18 台账表（1296–1348） | 副本 `/tmp/meta-r19-probe3/p18ledger`，`--panel-dir round18` | `FAIL coverage: 面板发现没有处置行`（98 个 ID 全列）；`ledger 1092` | FAIL（符合预期） |
| 探针 B：删 R19 台账表（1349→EOF） | 副本 `p19ledger`，`--panel-dir round19` | `FAIL coverage: 面板发现没有处置行`（80 个 ID 全列）；`ledger 1111` | FAIL（符合预期） |
| 探针 C：删 R19 loop 段（943→EOF） | 副本 `p19loop`，`--panel-dir round19` | `FAIL loop-sections: loop.md 缺少 R19 面板轮小节` | FAIL（符合预期） |
| 探针 D：R19 指标改 `999` / `1/0/1/1/0` | 副本 `pmetrics`，`--panel-dir round19` | `FAIL metrics: 发现总数 999 与五项处置之和 3 不一致` | FAIL（符合预期） |
| R18 计数复算 | 按 ID 展开 `defect-ledger.md:1296-1348` | ① 21 / ② 0 / ③ 21 / ④ 56 / ⑤ 0 = **98**；B-12 为 ④、B-15 为 ③ | 与 R18 metrics 逐项一致 |
| R19 计数复算 | 按 ID 展开 `defect-ledger.md:1349-1385` | ① 13 / ② 0 / ③ 14 / ④ 53 / ⑤ 0 = **80**；B-12 单独 ③ | 与 R19 metrics 逐项一致 |
| `minix-boot` | `cargo test -p minix-boot` | `188 passed; 0 failed` | 与 §5.2.1 一致 |

### 上轮阻断与 P3 的复核

1. **R18 范围已修对。** `defect-ledger.md:1323` 现为 `READER-B-01..14、READER-B-16..20`，展开 19 个 ID，含 B-12、排除 B-15；`:1324` 的 B-15 单行 ③ 作为精化覆盖。R18 段唯一 ID 98，与 metrics 98 一致；轨迹门 `--panel-dir round18` 覆盖与孤儿两项都 PASS。
2. **轨迹门按节集合已落地。** `parse_ledger` 现在返回 `section_ids`（`tools/eval-trace-check.py:205/312/315`）；coverage 用 `coverage_ledger = scoped`（`:523`，不再 `or ledger_set`）；`scoped` 的节匹配改为 `section.lower().startswith(round_tag.lower())`（`:518`），R18 不再被“R19 … R18 收口后”的标题污染。orphan 也改用 `coverage_ledger`（`:545`）。四组探针全部 FAIL，B-51 的 CLOSED 现在有实测支撑。
3. **loop 计数与状态已同步。** `loop.md §3` 现为 ④ 53（READER-B 18）、③ 14（B-12 + REFUTE 3 + D2 10）；`§7` 写明 RB19/meta 的 NOT PASS 与二次补修；`§8` 写“14 条 ③（10 条 D1 否决 + 3 条反证失败 + 1 条重复/已消失）”。
4. **R19 台账 FACT-03..06 行已改 360–363**（`defect-ledger.md:1357`），与文档 `1123` 行及 RB19 子表一致。
5. **A-09 注释已改实。** §6.1 复算命令现写“按文件覆盖表内全部来源；串前缀扫描约命中 10/16 组，其余走别的宏或裸字符串，见各行位置列”。原缺陷的文件面消失，注释也不再夸大。
6. **metrics 回读行已改实况。** R19 的“回读”行现写 RB19 判 NOT PASS、补修后 meta 判 IMPROVED/NOT PASS、二次补修后探针 FAIL 与 R18/R19 计数闭合。

### 当前树的重判与剩余项

- 初次抽样表的两个 FAIL 在当前树上按“缺陷成立条件是否消失”重判：**A-09 PASS**（文件面已覆盖，pattern 面 P3 已如实标注）、**B-12 PASS**（R19 单独 ③，R18 已恢复处置行）。按当前树重算，严格一致为 39/39；按合同保留初次的 37/39 = 94.9%，不改原表。
- **Trace integrity、计数、半修清单均已清**：四组探针 FAIL、两个 panel-dir PASS、R18 98 与 R19 80 各自与 metrics 逐项一致；原半修清单里的 FACT-06、READER-B-13、REFUTE-02、A-09、B-12、BASELINE 均已闭环。
- 剩余 CONCERN（不构成 FAIL）：Precision 的 80 仍含 10 条 D1 方案否决与若干跨角色重复，B-52（`origin`/`dup-of`）仍 OPEN；Independence 的面板期间工作树漂移未消除，B-53 仍 OPEN；Scope 的 `23a001e40` 提交信息仍写“R19 全修”；Cost 的 token 仍“未采集”。
- 剩余 P3（可选，不影响判定）：§3.4.1 图例列 `R14/R16/R17/R18` 而未列 `R19`（有“等”字，R19 按类别可读）；`metrics.md` 的 R19 回读行写“三组探针”，本轮实际做了四组。

### 结论变化

- 初次的 `NOT PASS` 由 Trace integrity + A-09/B-12 处置错驱动；这两类在 `7dbd07f6`/`11200ab55` 后均已消失。
- **本轮最终结论：`IMPROVED` / `PASS`。** 保留初次的 94.9% 抽样读数、`IMPROVED` 评价与“同 agent 验证 + 已重放命令”的局限声明；当前树的抽样一致率为 39/39（100%），但按合同只在正文说明，不改原表。
- 未改仓库其他文件；探针副本在 `/tmp/meta-r19-probe3/`。

VERDICT: PASS
