# 轮次身份与裁决依据纠正（2026-11-22）

## 事实

- `R39` 是 **D01 的第 39 个本地轮次**，不是 stage 级批次号。
- 各篇本地轮次链在本轮之前分别是：
  - D01：R1–R39（R38 为 V3.10 full，R39 为确认轮）。
  - D02：R1–R5（R5 为 V3.4 full，`ROUND-CLOSED`，未宣告 CONVERGED）。
  - D03：R1–R3（R1 为 V3.3 full；R2、R3 为 V3.4 确认轮）。
  - D04：R1–R3（R1 为 V3.3 full；R2、R3 为 V3.4 确认轮）。
  - D05：V1、R1–R2（R1 为 V3.4 修改轮；R2 为 V3.4 确认轮）。

## 错误

1. **轮次冒名**：D02–D05 的 `certify/round39-*.md`、`loop.md`、`metrics.md`、`defect-ledger.md`、`FREEZE.md` 中的 `R39`，是把 D01 的轮次号套到了其它篇。它们既不是 D01 的 R39，也不是各篇真实发生的轮次。
2. **裁决越界**：D02–D05 的 `round39-gates.md` 写“裁决依据：D-114”。D-114 的原文范围是 D01 直接关联的 48 个 `.rs` 文件（文件级整改），并明确“未涉及的其他 stage 代码不在 D01 范围”。它不能为 D02–D05 的出版清理、单元冻结或确认轮背书。
3. **覆盖声明失真**：D02 的“未抽样单元由 R5 full survey 覆盖”只对 D02 R5（V3.4）成立；D03 的 R3、D04 的 R3、D05 的 R2 都是**确认轮**，不是 full survey。D03–D05 在本 edition 没有 full survey，不能声称未抽样单元已被 full survey 覆盖。

## 后果

- D02–D05 的 `R39 CONVERGED` 宣告**作废**，状态应为 `CONVERGENCE-CANDIDATE`，直至补做当前 edition 的 full survey 与逐单元证据。
- D01 的 R39 不受影响：D01 的 R38 是 V3.10 full，D-114 也确实是 D01 范围内的作者裁决。
- 原文件不删除、不改写内容，只加 `INVALIDATED` 标记，保留错误轨迹作为证据。
- 各篇下一个合法本地轮次（尚未发生，仅占位）：D02 `R6`、D03 `R4`、D04 `R4`、D05 `R3`。

## 纠正动作

1. 在 D02–D05 的 R39 相关文件与章节加 `INVALIDATED` 横幅。
2. 把 `round39-gates.md` 的“裁决依据：D-114”改为“无；D-114 仅适用于 D01”。
3. 状态改 `CONVERGENCE-CANDIDATE`；补当前 edition full survey 前不得再宣告 CONVERGED。
4. 轮次号、裁决依据、基线 edition 三项写入后续 workflow 门：轮次必须按篇本地编号；裁决必须写明适用范围；baseline 必须绑定当前 edition。
