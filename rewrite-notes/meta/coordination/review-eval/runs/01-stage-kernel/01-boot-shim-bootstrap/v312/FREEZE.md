# D01 FREEZE 记录（V3.12）

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 状态：`CONVERGED`（R4 确认轮，2026-11-23）
- R4 证据：`v312/r4/`（G18 `HARD-REJECTIONS: 0 / READY`、48/48 confirm-review、R4 trajectory PASS、`gate T3: PASS`）
- R3 证据：`v312/r3/`（53 项面板/复验发现全部 ①、独立 doc/code 复验 PASS、R3 trajectory PASS）

## 冻结时刻

| 项 | 值 |
|---|---|
| 文档 sha256 | `0b943d0c9540676a9b3d8cb237f73276ff575f9d475f1b878cf492dd0475c405`（规则一/二追溯 + 规则三/四/五回归后） |
| 行数 | 3,107 |
| 代码基线 | `7ff12bca4`（`kmain` 物理/高半别名修复） |
| 单元 | 48/48 冻结（state 记录） |
| 学习范围 | `meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`，`confirmed_by=me-agent-deputy`（待作者追认） |

## 门读数（规则一/二追溯后）

- anchor 87/87、code-map 49/49、ref OK、style error 0、publication 无新增违规、
  tutorial-rubric 6 章 0 缺项、citation failed=0、density 通过、structure 通过。

## 六视角可读性守门（V3.12+）

| 视角 | 报告 | 结论 |
|---|---|---|
| beginner-me | `cold-read/y04-g18.md`（R4） | `HARD-REJECTIONS: 0 / READY` |
| G18 READER-C | 同上 | `VERDICT: READY` |
| 编辑（取证腔/节奏） | 待补 | 待补 |
| 导航者 | 待补 | 待补 |
| 效率读者 | 待补 | 待补 |
| 教学设计师 | 待补 | 待补 |

## 回归节（post-CONVERGED）

- 规则三/四/五内容修复：`REGRESSION.md`；净减约 1.1k 实义字符；五组复述压到权威段；
  15 处标签化；21 个长段拆分；八门全绿。
- `kmain` 指针别名修复：commit `7ff12bca4`；x86 UEFI 与 riscv64 U-Boot shim 门 PASS；
  aarch64 direct-core 越过 `kmain`，随后停在已登记 A2 用户链异常。
