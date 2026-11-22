# D03 FREEZE —— 03-kmain-cstart（历史 CONVERGED；当前 CONVERGENCE-CANDIDATE；R39 宣告作废）

- 冻结日期：2026-11-12
- 工作流 edition：V3.3
- 确认轮：R2（`轮型: confirmation`）
- 冻结提交：`88ac92c60`（R2 修复与轨迹提交）
- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 文档行数：R2 冻结提交（88ac92c60）时点 1393；R3 面板基线（c2ba58596）时点 1396；R3 修复后 1396
- 文档 sha256：R2 冻结时点 `da85d6cf…`；R3 面板基线 `e54cad6907da01f4bd455e4b63712bda4896a1c92fe89ce8d4d04e227362344d`；R3 修复后 `64659b6c…`（最终工作树实测）
- 关联代码：`tools/doc-code-map.sh --check` 21/21 通过

## 收敛证据

- `eval-trace-check.py --round R2`：PASS（11 PASS / 1 WARN / 0 FAIL）。WARN 是 `D03-design-batch` 的已批准延期（D-053）。
- R2 台账：48 条（survey 43 + certify 5），处置 ① 41 / ② 2 / SUGG 5；`未闭环 = 0`；`certify 新准入 = 0`（certify 的 5 条已修，最终同 agent grep 复核）。
- 100% 复验：每个 ① 行有 grep / path:line / 冷读回执；无抽样。
- 文档六门：citation 42 块全跳过（`/* */` 标注、工具只认 `//`，已登记 D03-citation-tool-batch）；anchor 75/75；structure/ref 通过；style-lint error=0；doc-code-map 21/21。
- 宿主：本轮无代码行为改动；R1 已核对 `os/arch` 373/54、`os/arch/tests` 87/21、9 文件子集 55。QEMU 未跑。

## 轮次链

- V1 旧 run 保留为历史段，不追溯改账。
- R1（V3.3 full）：49 条收口，6 P0（memcpy 语义、kernel_may_alloc 承重、KERNEL_MAY_ALLOC 已删、Step 1 旧引文、pg_utils 双副本）；ROUND-CLOSED，提交 `7a506169b`。
- R2（V3.3 confirmation）：43 条 survey + 5 条 certify；3 条 P0（bootstrap 字段已删、GsArea.user_rsp、Rust 无 IOAPIC 64 门）与 12 条读者 P1 全修；CONVERGED。

## 未解决事项（不阻断，已登记）

- `D03-design-batch`（D-053）：D2-10 陈旧注释、D2-11 TSS 占位/riscv64 断言强度、D2-16 就地初始化 + 删 refresh_descriptors（ARCH 提案）、R2 F03/F04 过期注释与非 0 BSP TSS 槽 0 写错。
- `D03-citation-tool-batch`：citation 工具只认 `//`，本文 42 个 `/* */` 块全跳过；下一 workflow 边界修工具或改标注。
- 设计 P3 5 条与读者 SUGGESTION 7 条未入台账。

## 限制

- `CONVERGED` 是确认轮口径下的“无 open 项 + 无未销账债务”，不是零未知缺陷保证。
- 同 agent 复核：面板与 certify 使用同一模型；已重放全部 grep/门命令，跨模型独立复核未做。
- 设计代码项与 citation 工具缺口延期到各自批次；实施后需重新固定基线。

## 重开条件

- 出现新的 P0/P1 事实或行为缺陷证据；
- `D03-design-batch` 或 citation 工具修完导致基线改变；
- 用户明确要求重开。

## 回归 review（2026-11-13）

- 回归审查判 FAIL：RD3-R2-25 的“已修”不实（正文残留 2×`本轮未跑`、2×`本篇不引节号`、1×`正说明这一点`、2×`D01-F08`），另有 REG-D03-01 P2（GDB `finish` 停点写成 `bsp_finish_booting`，实为 `kmain`）。
- 两条均已在正文修正；`anchor 75/75`、`doc-code-map 21/21`、`doc-ref` 通过，`--round R2` 仍 PASS（11/1/0）。

## 回归 review（2026-11-17，恢复独立读者后重做）

用户裁决 D-055 恢复两条承重墙（读者 A/B/C 独立、certify 必须独立冷读）后，对 D03 重做回归：

- **独立三角色**共报 18 条（READER-A 8、B 5、C 5，含 P1 1），远超合并版当时的 14 条；其中 §4.7 出口状态自相矛盾（说 `intr.init()` 已编程、又说下一件事是控制器初始化）、§3.6 未定义 Phase 名、§2.5 中间态论证等全部是合并角色漏掉的。
- **独立冷读 certify 三轮**：第 1 轮 7 条新准入、第 2 轮 3 条、第 3 轮 1 条。按 `review-evals.md` §4.0b 硬规则 3 记 `BLOCKED` 并写 `STUCK-REPORT.md`。
- 对两个复发根因做了**结构性重写**（§2.5 第 1 步拆两段、§3.6 Phase 表改七行并把 E 归 `13-syscall-dispatch.md`），随后一次独立闭集冷读判 `结构性重写验证 = PASS`。
- 最终文档 sha256 `7f3fe6a0…`；anchor 75/75、doc-code-map 21/21、structure/ref/style 全绿。

**结论：回归通过，D03 维持 CONVERGED。** 依据是独立冷读对重写段的 PASS，不是主 agent 自检。

## 完整 review（2026-11-18，D-055 恢复独立读者后）

用户要求对 D03 做完整 review。五角色（FACT、DESIGN-D2、READER A/B/C 独立）全量 survey：

- 面板编号发现 22 条（含 FACT-04 面板自注撤回）：FACT 3(+1 撤回)、READER-A 1、READER-B 4、CERT 4、DESIGN-D2 9（其中 5 条自标 SUGGESTION 级）；「33 条」为含未编号 SUGG 段的聚合口径（reader-c 0 条 FINDING）。r6 补账按面板 ID 列明（见 defect-ledger FULL-2026-11-18 节）。
- 6 条 P2 文档修复已提交（`ebcf673dc`）；certify 抓到 4 条更细粒度 P2 也已修（`c2ba58596`）。
- 1 条 arm64 代码注释残留（UNDEFINED 口径）登记 D03-design-batch。
- 独立冷读 certify 最终 6/6 CLOSED（第 4 条 P2 残项修正后）。
- 机械门：anchor 75/75、doc-code-map 21/21、structure/ref/style 全绿。
- 最终文档 sha256 `e54cad69…`。

**结论：完整 review 通过，D03 维持 CONVERGED。** 残余为 4 条 certify 微调（已修）与 5 条设计 P3 + 2 条 ARCH 提案（D-053 延期）。

## R3 确认轮（V3.4；D-066 授权的重开确认，append-only 追加）

- 上一节记录的是 V3.3 口径下的 R2 确认；按 D-066（用户指令 `prompt/EXECUTE-D01-D06-CONVERGENCE.md`）在 V3.5 制品链下重开一次，轮次号 R3、edition V3.4、`schema: V3.4 from R3`。
- **R3 确认轮**：六角色全量 survey（fact 2、design-d2 15、reader-a 3、reader-b 0、reader-c 2 条 FINDING；SUGGESTION 46；refute 6 条 CLAIM）。无 P0、无 P1；8 条 P2。fact 角色实测 25 块引文逐字一致、约 90 项常量现算一致——R2 后的文档事实质量已经很高。
- **修复**：8 处文档修改点（`tss_init` 参数位次、M1 的 user_accessible 口径、E1 指针、测试计数口径、§4.2 省略标注、门类型两个写点、lgdt→lidt 窗口的递送轴限定、反事实「先炸」排序限定、两处引文精度）。
- **certify**：`certify/round3-certify.md`（本轮新派发的冷启动子 agent）。首轮 8 条 6 CLOSED / 2 STILL-BROKEN；按证据改正 3 处后复核 **8/0 PASS**。回执抓到主 agent 两处实质问题（374/95 口径串位、§4.2 省略方向写反）。
- **面板后处理（VARIANCE）**：design-d2 的 `### FINDING` 升为 `##` 并删裸标题、refute 索引重排为 `## CLAIM 11..16`——正文未改动；派发按平台并发 3+3 两波（L1.1 降档协议）。
- **机械门**：`--round R3` PASS；anchor 75/75；citation `checked=32 failed=0`；structure/ref/style(error=0)/density 通过；doc-code-map 21/21；`panel-index --check` 6 文件哈希一致；`--round R1/R2` 仍 PASS（历史面板已入库以支持按轮作用域）。
- **未闭环 = 0**。`OPEN-DEBT` 的 `R3-design-batch`（5 条设计 P2 + 10 条设计 P3）有 D-066 判决行与 2026-11-20 日期。
- **ARCH 提案 2 条**（RD03-A1/A2）待用户亲裁，不阻断。
- 限制同前：`CONVERGED` = 确认轮口径下无 open 项 + 无未销账债务，不是零未知缺陷保证。

## citation 漂移修复（2026-11-21，用户指示的一行修复，append-only 追加）

- **漂移**：`doc-citation-check.py` 报 `blocks=42 checked=31 failed=1`——块 §4.2 节选（文档 985-1002）行 `let prot = CurrentProtection::init(0, kernel_info.kern_stack_top);` 缺调用括号，源码实为 `kernel_info.kern_stack_top()`（`os/kernel/src/lib.rs:2368`）。
- **根因**：**I-31（`52110580f`，2026-11-20 16:06）** 把 `KernelInfo` 的 `kern_stack_top` 从字段改为方法调用，落地在 **R3 确认轮的引文门之后**（R3 时点读数为 `checked=32 failed=0`，该读数当时为真）；文档引文未随 I-31 同步。另：原 `D03-citation-tool-batch`（工具只认 `//` 标注、42 个 `/* */` 块全跳过）已由 V3.5 的 `154e90392` 修复，本条正是工具修好后才暴露的**文档侧漂移**。
- **修复**：文档 `:989` 一行补 `()`。修后 `doc-citation-check.py`：**`blocks=42 checked=32 failed=0 skipped=10`**。
- **门复跑（修复后工作树）**：`anchor-resolve --check` 75/75（zero-def=0 / multi-def=0）；`doc-code-map --check` 21/21；`doc-ref-check --check` 通过；`doc-code-density-check --check` 承载门阻断项通过；`doc-style-lint --diff` error=0。
- **新文档 sha256**：`1e6037125cf95a79ef940a94be6356243392ae1d8135c697595d0b1cfcb1a22f`（1396 行）。R3 修复后记录值 `64659b6c…` 自此被本行取代。
- **性质**：事实同步修复（Doc Code Sync），非轮次重开、不改任何实质结论；`CONVERGED` 状态维持。重开条件不变（见上文「重开条件」）。

## R39 确认轮（V3.10，append-only；2026-11-22）

> **INVALIDATED（2026-11-22）**：本文件中的 R39 是 D01 的轮次，不是本篇轮次；D-114 仅适用于 D01。原“CONVERGED”宣告作废，状态为 CONVERGENCE-CANDIDATE。纠正记录见 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/ROUND-IDENTITY-CORRECTION.md`。

- 轮结果：`INVALIDATED`（原写 `CONVERGED`）。
- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`，sha256 `5b142de24f009880fffc78818af6913be8150af31374632e80d57187fd6354f7`，1688 行。
- 单元账本：44/44 frozen、reopened=0、stale=0、T3 PASS；`audit` 全部单元 open=0。
- 确认轮：9 个冻结单元抽样；13 条发现（4 P1 + 6 P2 + 3 P3）全部修复，`DELTA: PASS`；修复 2/2 批次，未开新修改轮。
- 文档七门：publication/tutorial 0 新增；structure/ref/style/density 通过；citation `32 checked/0 failed`；anchor 75/75；doc-code-map 22/22。
- 代码门：22 个 related-code code-publication 0 命中、CJK 0；minix-arch 266/0、minix-kernel mock 866/0/8、minix-elf 27/0。
- 真机：test-protection x86_64/aarch64/riscv64 三腿 PASS。
- 账本：D03 TODO 匹配 0 行，无未闭环项。
