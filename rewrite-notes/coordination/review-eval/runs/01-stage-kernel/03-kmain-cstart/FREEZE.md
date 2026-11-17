# D03 FREEZE —— 03-kmain-cstart（CONVERGED）

- 冻结日期：2026-11-12
- 工作流 edition：V3.3
- 确认轮：R2（`轮型: confirmation`）
- 冻结提交：`b00c44292`（R2 修复与轨迹提交）
- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 文档行数：1396
- 文档 sha256：`7f3fe6a0d918eff04a13eabfac737d0ee2b6089b03dc2a97e102a8a4ec2702ef`
- 关联代码：`tools/doc-code-map.sh --check` 21/21 通过

## 收敛证据

- `eval-trace-check.py --round R2`：PASS（11 PASS / 1 WARN / 0 FAIL）。WARN 是 `D03-design-batch` 的已批准延期（D-053）。
- R2 台账：48 条（survey 43 + certify 5），处置 ① 41 / ② 2 / SUGG 5；`未闭环 = 0`；`certify 新准入 = 0`（certify 的 5 条已修，最终同 agent grep 复核）。
- 100% 复验：每个 ① 行有 grep / path:line / 冷读回执；无抽样。
- 文档六门：citation 42 块全跳过（`/* */` 标注、工具只认 `//`，已登记 D03-citation-tool-batch）；anchor 75/75；structure/ref 通过；style-lint error=0；doc-code-map 21/21。
- 宿主：本轮无代码行为改动；R1 已核对 `os/arch` 373/54、`os/arch/tests` 87/21、9 文件子集 55。QEMU 未跑。

## 轮次链

- V1 旧 run 保留为历史段，不追溯改账。
- R1（V3.3 full）：49 条收口，6 P0（memcpy 语义、kernel_may_alloc 承重、KERNEL_MAY_ALLOC 已删、Step 1 旧引文、pg_utils 双副本）；ROUND-CLOSED，提交 `1b1ea902f`。
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
