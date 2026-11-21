# D05 回归审查（R2 确认轮后）

- 审计日期：2026-11-20
- 审计性质：FREEZE 后独立回归审计（FREEZE.md 记录 vs 最终工作树逐项实测）
- **结论：FAIL（记录级不一致 2 处，见「不一致清单」；除该 2 处转写滑误外，FREEZE 的全部实质冻结声明——文档同一性、六门实质判定、轨迹门、metrics 与台账聚合、certify 回执、未闭环与债务、代码实施——均逐项复核一致）**

## 基线提交号

- 审计时点 HEAD：`efc765b9074a92e2bf9d13a9bb1b0abc2c7ed400`（即冻结提交 `fix(05): R1 完整修改轮 + R2 确认轮——D05 从零收敛（CONVERGED）`，2026-11-20 06:27:15 +0800）
- loop.md R1 §0 记录的面板启动基线：`cd76b73d1`（实测存在：`fix(04): R3 回归审计的 2 项记录层不一致`，2026-11-20）
- 工作树状态：`git status --short` 仅 3 个与 D05 无关的未跟踪文件（`prompt/review-loop-r26-r29-report.md`、`rewrite-notes/01-stage-kernel/01-design-learn.md`、`rewrite-notes/coordination/review-eval/meta/01-boot-shim-bootstrap-r26.md`）；文档与整个 run 目录无未提交改动

## FREEZE 记录复核表

| FREEZE 声明 | 实测命令 | 实测读数 | 判定 |
|---|---|---|---|
| 文档 sha256 `c86d3ffaa29307cb4c6aef28169c02dab8f79e182fd446276da12c99b001669e` | `sha256sum rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` | `c86d3ffaa29307cb4c6aef28169c02dab8f79e182fd446276da12c99b001669e` | 一致 |
| 文档行数 2209 | `wc -l <doc>` | 2209 | 一致 |
| 关联代码 doc-code-map 33/33 | `tools/doc-code-map.sh <doc> --check` | `（共 33 个文件；存在性 33/33 通过）` exit 0 | 一致 |
| R1 certify 三轮至 15/15 CLOSED | `certify/round1-certify.md` L3-L5 | 结论 PASS、核验 15、CLOSED 15、需整改 0 | 一致 |
| R2 certify 8/8 首轮即 PASS | `certify/round2-certify.md` L3-L5 | 结论 PASS、核验 8、CLOSED 8、需整改 0 | 一致 |
| `cargo check -p minix-arch` 绿 | 实测重跑 | `Finished dev profile … in 0.25s`，exit 0（11 条 warning，dead_code 类） | 一致 |
| kernel lib 测试 866/0 | 实测 `cargo test -p minix-kernel --features mock --lib` | `866 passed; 0 failed; 8 ignored`，exit 0 | 一致（FREEZE 写 866/0，实测另有 8 ignored，与 loop.md R1 §5「866/0/8」口径一致） |
| OPEN-DEBT 无未销账行 | `test -f OPEN-DEBT.md` | 文件不存在（无行即无未销账行，与 loop.md R1 §7「无 OPEN-DEBT 行」一致） | 一致 |
| D-075 授权 | `grep -n "D-075" rewrite-notes/coordination/review-eval/DECISIONS.md` | L156 一整行，代裁决内容与 FREEZE 描述吻合，状态「已实施」 | 一致 |

## 不一致清单（FAIL 依据，逐条）

1. **anchor 计数：记录 106/106，最终工作树实测 105/105。**
   - 记录处：FREEZE.md L27「anchor 106/106」；loop.md L156（R1 §4）、L233（R2 §4）、metrics.md L50（R1 块）、L72（R2 块）均写 106/106。
   - 实测命令与输出：`tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md` → `anchors=105 resolved=105 zero-def=0 multi-def=0`，exit 0；`--extract` 逐行清点同为 105。
   - 排除工作树漂移：文档 sha256 与 FREEZE 逐字节一致、文档自冻结提交后无改动（`git log -1 -- <doc>` = efc765b90，`git status` 干净）；门工具无版本差（`tools/anchor-resolve.sh` 最后一次改动 0c1d1e1ce（2026-10-11）早于冻结提交，`git merge-base --is-ancestor` 验证为祖先）。
   - 判定：106 在最终工作树上不可复现，属冻结记录的转写滑误（推测：R2 尾段编辑退役了含锚点的句后未重读门计数，六门行沿用 R1 数值）。门的实质判定不受影响（全部锚点解析、zero-def=0 multi-def=0、exit 0），但记录值与可复现值不一致即记 FAIL。
2. **loop.md R1 §6（L168）处置分布与它声称「一致」的 metrics.md 不一致。**
   - 实测：loop.md L168 写「处置 ① 15 / ⑥ 6 / ④ 7 / FP 5 / SUGG 37」并称「与 `metrics.md` 的 `## R1 完整修改轮` 块一致」；metrics.md L38 实为「① 15 / ② 0 / ④ 7 / ⑤ 0 / ⑥ 4 / FP 5 / SUGG 39」。
   - 判定：⑥ 6 + SUGG 37 与 ⑥ 4 + SUGG 39 合计同为 43、总数同为 70，属转写滑误而非口径分歧；台账逐 ID 聚合（见下节）支持 metrics.md 的 ⑥ 4 / SUGG 39 口径，loop.md L168 的分档数是错的。

## 轨迹门与制品链

- `python3 -I tools/eval-trace-check.py <run-dir> --round R1` → 10 PASS / 0 WARN / 0 FAIL，结论 PASS（panel 70 条、台账 222 条处置、coverage 双向通过、metrics 发现总数与处置之和一致（70）、metrics 各分档与台账本轮逐 ID 聚合一致、未闭环=0、edition V3.4 试验窗口 1/3）。
- `python3 -I tools/eval-trace-check.py <run-dir> --round R2` → 10 PASS / 0 WARN / 0 FAIL，结论 PASS（同上，总数 69）。
- `python3 -I tools/panel-index.py <run-dir>/panel/round1 --check` → `panel-index: 6 个文件哈希与计数一致`，exit 0；round2 同。
- INDEX 计数与 metrics 同源核验：round1 INDEX FINDING 6+11+4+2+2+0=25、SUGGESTION 0+3+8+14+14+0=39、CLAIM 6，合计 70；round2 INDEX FINDING 9+5+3+3+1+0=21、SUGGESTION 0+0+10+15+14+0=42、CLAIM 6，合计 69——均与 metrics.md 两条轮块「与 panel/roundN/INDEX.md 同源」声明一致。

## 六门读数（最终工作树实测）

| 门 | 命令 | 读数 | FREEZE/loop 记录 | 判定 |
|---|---|---|---|---|
| anchor | `tools/anchor-resolve.sh --check <doc>` | `anchors=105 resolved=105 zero-def=0 multi-def=0`，exit 0 | 106/106 | 实质通过，计数不一致（见不一致 1） |
| citation | `python3 -I tools/doc-citation-check.py <doc>` | `blocks=62 checked=43 failed=0 skipped=14 warned=5`，exit 0 | checked=43 failed=0 | 一致 |
| structure | `python3 -I tools/doc-structure-check.py <doc>` | `1 个文件表结构与内部 .md 链接均可解析`，exit 0 | 通过 | 一致 |
| ref | `AWK=gawk tools/doc-ref-check.sh --check <doc>` | `内部章节引用均可解析`，exit 0 | 通过 | 一致 |
| style | `tools/doc-style-lint.sh <doc>` | `命中汇总：\| error级=0`，exit 0 | style(error=0) | 一致 |
| density | `tools/doc-code-density-check.sh <doc>` | `承载门阻断项通过（4 个可判章均有码/命令）`，exit 0 | 通过 | 一致 |
| doc-code-map | `tools/doc-code-map.sh <doc> --check` | `共 33 个文件；存在性 33/33 通过`，exit 0 | 33/33 | 一致 |

## metrics 与台账逐 ID 聚合（独立脚本，非仅依赖 eval-trace-check）

- 命令：Python 解析 `defect-ledger.md` 两张轮表，按「发现 ID」列以「、」拆分逐 ID 计数、按行尾处置类分档。
- R1 实测：`roots=29 total=70 disp={'FP': 5, 'SUGG': 39, '①': 15, '④': 7, '⑥': 4}`，轮内无重复 ID——与 metrics.md R1 块（①15/④7/⑥4/FP5/SUGG39、总数 70、根因 29）逐项一致，与任务书期望值一致。
- R2 实测：`roots=28 total=69 disp={'FP': 3, 'SUGG': 43, '①': 17, '④': 6}`（⑥=0），轮内无重复 ID——与 metrics.md R2 块（①17/④6/FP3/SUGG43、总数 69、根因 28）逐项一致。
- 跨轮 ID 重叠：无（R1 `RD05-`/`CLAIM-11..16` 与 R2 `RD05R2-`/`CLAIM-21..26` 两套命名空间隔离）。
- certify 新准入：metrics R1、R2 块均记 0，与两条 certify 回执的闭集复核陈述一致；未闭环：R1、R2 均记 0，eval-trace-check 两轮「未闭环 = 0」PASS。

## 未闭环与债务

- 未闭环：0（R1、R2 两轮台账聚合与 metrics 声明一致）。
- OPEN-DEBT.md：不存在于 run 目录，无未销账行——与 FREEZE「OPEN-DEBT 无未销账行」一致。
- `follow-ups.md`（2026-11-18）为旧 V1 轮历史债务（design-debt 18 / teaching-debt 47 / test-debt 1），按 L0.8 不继承、保留供追溯，不在 V3.4 R1/R2 口径内——与 loop.md V1 历史段一致，不计入本轮未闭环。
- D-075：存在于 `rewrite-notes/coordination/review-eval/DECISIONS.md` L156，内容（开 R1 完整修改轮、6 条设计项当轮 ⑥ 实施、RD05-A1 待亲裁）与 FREEZE/loop.md 引用一致，状态「已实施」。

## 一致性抽查（台账 6 行，重放命令或核对摘录）

| 台账行 | 处置类 | 重放/核对 | 结果 |
|---|---|---|---|
| R1 RD-R1-06（DESIGN-D2-01/02/04，⑥ [DEL]/[MODERN]/[DEL]） | ⑥ | `grep -rn "single MMIO block" os/arch/src/ --include=*.rs` → 0 命中（旧 CLINT 单块注释已删）；`sed -n '6,8p' os/arch/src/riscv64/timer_irq_gate.rs` → 新注释「mtimecmp, programmed through the SBI TIME extension…（S-mode cannot write the M-mode comparator directly）」在文 | 符合 |
| R1 RD-R1-10（READER-A-03 P1，位置三处矛盾） | ④ | 文档 L129 现文：「`boot_init_timer` 在 `bsp_finish_booting` 的 Step 6（Phase F，在进程表建立之后……源注释明言这一位置对应 C 的 `boot_cpu_init_timer`，见 §4.7）」；与 `sed -n '3722,3725p' os/kernel/src/lib.rs`（Step 6 头注释、`called from bsp_finish_booting (main.c:73)`）三方一致 | 符合 |
| R1 RD-R1-26（CLAIM-11/13/14，FP 反证失败） | FP | C 源真值核对：`minix3/minix/kernel/arch/i386/i8259.c` L28-57 `intr_init` 十次 `outb`（ICW1-4 主从各 5 次、末态 `~(1 << CASCADE_IRQ)` 与 `~0`）、L66-70 `i8259_disable` 写 0xFF×2——主张「成立」的源码证据在，反证失败判定成立 | 符合 |
| R2 RD-R2-01（FACT-01 P1，arch DEFAULT_HZ 注释） | ① | `sed -n '40,41p' os/arch/src/arch/clock.rs` → `**Authoritative definition**: this arch-level constant is the single / source of truth; the kernel crate re-exports it`；重放 certify 条目 1：文档 L756-779 与源码 L23-46 `diff` 为空（QUOTE-IDENTICAL） | 符合 |
| R2 RD-R2-20（RB-S15，SUGG） | SUGG | 登记核对：`suggestions.md` R2 段 L15 以区间形式「reader-b.md 的 SUGGESTION（RD05R2-RB-S01..S15）\| 15 \| 登记不实施（读者 P3）」——S15 经区间覆盖（精确字符串 `RD05R2-RB-S15` 不单独出现，登记为簇级区间；本审计仅对 suggestions.md 做了计数/行摘录核验） | 符合 |
| R2 RD-R2-28（CLAIM-23/24/25，FP 反证失败） | FP | 与 RD-R1-26 同源核对：i8259.c `intr_init` 的十次写与全屏蔽末态在 C 源（「intr_init 十次写与全屏蔽」证据行成立，主张反证失败） | 符合 |

## 代码实施复核（FREEZE §R1 四项 ⑥）

- `grep -rn disable_timer_irq os/ --include=*.rs` → 0 命中（exit 1）——trait 与三架构实现删除干净。
- `grep -n "pub use minix_arch::arch::clock::DEFAULT_HZ" os/kernel/src/clock.rs` → `710:pub use minix_arch::arch::clock::DEFAULT_HZ;`——re-export 在位。
- `grep -c debug_assert os/arch/src/arm64/clock.rs` → 1；`grep -c debug_assert os/arch/src/riscv64/clock.rs` → 1（arm64 L44、riscv64 L194，均为重挂回退分支的 `debug_assert!`）。
- 补充：`pub const DEFAULT_HZ` 仅在 `os/arch/src/arch/clock.rs:46`，kernel 侧无独立副本（单一权威定义成立）。
- 宿主复测：`cargo check -p minix-arch` Finished（exit 0）；`cargo test -p minix-kernel --features mock --lib` 866 passed / 0 failed / 8 ignored（exit 0）。

## 局限声明

- 同 agent 验证：本审计与被审的冻结记录产自同一模型体系，未经跨工具只读复核；全部关键读数附了可重放命令与输出以降低该局限。
- QEMU 真机回归未跑（沿用冻结口径：本轮 ⑥ 改动不含三架构真机门面，debug_assert 在 release 下不生成代码）。
- anchor 106 的历史真值不可重建：R1 修复后（2207 行）与 R2 修复后（2209 行）两个中间文档态未单独入库（两轮修复同入 efc765b90），故无法判定「R1 时点确为 106、R2 编辑后变 105」还是「记录即错」；仅能确证「最终工作树 = 105，记录 = 106」。
- `suggestions.md`、`follow-ups.md`、`related-code.txt` 不在本审计的允许读取清单内，仅以 `grep -c` / `grep -n` 计数与单行摘录核验登记存在性，未整读内容。
- token 成本未采集（与 FREEZE 记录一致）；本报告为唯一产出文件，未修改任何被审文件。

## R2 回归审计的补修记录（主 agent，append-only）

上面是独立审计者的原始结论（FAIL，2 条记录层不一致）。按 §4h 已全部处置：

| # | 审计发现 | 处置 | 复验 |
|---|---|---|---|
| 1 | FREEZE/loop/metrics 五处记 anchor 106/106，实测 105/105 | 五处统一改为 105/105（R1 时点真值因中间态未入库不可重建，以最终树实测为准） | `anchor-resolve.sh --check` → 105/105 |
| 2 | loop R1 §6 写 ⑥6/SUGG37 与 metrics 的 ⑥4/SUGG39 不一致 | loop 改 ⑥4/SUGG39（台账聚合口径） | 审计者的独立聚合读数即实测 |

补修后实测：`--round R1/R2` 两轮 PASS；anchor 105/105、citation 43/0、panel-index 两轮哈希一致。
