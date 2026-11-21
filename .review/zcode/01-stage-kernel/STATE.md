# Review State: 01-stage-kernel (zcode)

> **重写基线（2026-11-21）**：01/02 两篇已按“01=交接载荷、02=内核镜像”重写。
> 旧 v3 CONVERGED 作废；旧轮次（D01 R36、D02 R4）只作历史记录，不跨重写比较。
> 新基线：01 = 2673 行（sha256 `665230ebb46af3d1…`），02 = 1840 行（sha256 `b2ab482fe7b4961b…`）。
> Phase 7 已跑 D01 R37 / D02 R5（full）：两篇均 `ROUND-CLOSED`，未闭环 0、certify 新准入 0。
> 未宣告 CONVERGED：full 轮无声明权；且 G11 code-publication、G13 tutorial-rubric、`D01/D02-EXTERNAL-D107` 外部哨兵仍未清零。
> 旧 OPEN-DEBT 仍全 CLOSED；重写不替旧债销账，也不新增代码项。

- **Tool**: zcode
- **Stage**: 01-stage-kernel
- **Phase**: R1 正确性 + R2 卓越性 + 修复轮 + 第二轮回归 完成
- **Last completed phase**: R2 回归（2026-11-02）
- **Open P0 issues**: 0
- **Open P1 issues**: 4
- **Open P2 issues**: 12
- **Convergence status**: CONVERGED_WITH_WARNING_BACKLOG（P0=0；残余 P1=4、P2 backlog 见报告 R2-4）
- **Next action**: 清 R2-4 列的 4 条 P1；P2 backlog 交 code-excellence
- **Blocker Gates**: 0✅ A✅ B✅ C✅ D✅ D-6✅ E✅ G⏳ H✅

## Resume Point（NEW 2026-07-16，跨 session 续审必填）
- **Next Session Resume Point**: R3 → R4 → R5（VERIFY-CHECK / 01-review.md / 复盘）
- **Last Session Status**: R1/R2 完成；14 P0 / 32 P1 / 69 P2 已登记 scan.md
- **必读文件清单**: scan.md + structure.md + SYMBOLS.md + excellence.md + test-audit.md + design v3

## Phase Completion Log
| Phase | Date | Passes | P0 found | P1 found | P2 found |
|-------|------|--------|----------|----------|----------|
| R0 预检 | 2026-11-02 | 1 | 0 | 0 | 0 |
| R1 正确性 | 2026-11-02 | 1 | 14 | 32 | 69 |
| R2 卓越性 | 2026-11-02 | 1 | 0 | 0 | 0（Layer 2 PARTIAL，不新增计数） |
| 修复轮 | 2026-11-02 | 1 | 0 | 32 fixed | 57 fixed |
| R2 回归 | 2026-11-02 | 1 | 0 | 4 new（均修） | 9 new（部分修） |

## Per-Doc Status
| Doc | Last scan | Agent | P0 | P1 | P2 | VERIFY? |
|-----|-----------|-------|----|----|----|---------|
| 01-boot-shim-bootstrap | 2026-11-02 | zcode | 0 | 4 | 12 | ✅ |
| 02-higher-half-kernel | 2026-11-02 | zcode | 0（R1 11） | 0（R1 47） | 4（R1 13） | ✅（R1 14/14；R2 14/14） |
| 03-kmain-cstart | 2026-11-02 | zcode | 0 | 0 | 0 | ✅（机械 PASS；A4 87.5% CONCERN 预修复；A5 回归待回） |
| 04-platform-discovery | 2026-11-03 | zcode | 0 | 0 | 0 | ✅（R2：43/43 关闭；同 agent 8/8；test-smp-aps 基线失败单列） |
| 05-clock-interrupt-init | 2026-11-03 | zcode | 0（R1 11） | 0（R1 13） | 0（R1 8；+1 closed） | ✅（R2：12/12 抽样；Gate 全 PASS；QEMU 三架构 PASS） |

## 02-higher-half-kernel（2026-11-02，独立计数，不并入上方 01 状态）
- **Status**: **CONVERGED_WITH_WARNING_BACKLOG**（P0=0；P1=0；P2 4 accepted/backlog；QEMU 未跑）
- **Blocker Gates**: 0✅ A✅ B✅ C✅ D✅ D-6✅ E✅ G✅ H✅
- **P0**: 0 open（11/11 fixed）
- **P1**: 0 open（47/47 落地；P1-test-02 的生产入口断言残项转 R02-T04）
- **产物**: `rewrite-notes/01-stage-kernel/02-review.md`；`.review/zcode/01-stage-kernel/02-higher-half-kernel/`（scan/SYMBOLS/structure/excellence/test-audit/VERIFY-CHECK/VERIFY-CHECK-R2/trace）；`.design/02-outline.v4.md`、`02-outline-review.v4.md`、`02-design.v3.md`
- **Next**: R02-T01（段归属断言）、R02-T03（riscv64 PC 判据）、R02-T04（生产 kmain 入口断言）；QEMU 三架构回归待跑

## Session Status
| # | Session | 范围 | Status | Started | Completed | 产出 |
|---|---------|------|--------|---------|-----------|------|
| 1 | 01-boot-shim-bootstrap | doc + code，deep | ⏳ IN_PROGRESS | 2026-11-02 | — | scan/SYMBOLS/structure/excellence/test-audit/design v3 |
| 2 | 02-higher-half-kernel | doc + code，deep | ✅ CONVERGED_WITH_WARNING_BACKLOG | 2026-11-02 | 2026-11-02 | 02-review/scan/SYMBOLS/structure/excellence/test-audit/VERIFY-CHECK/VERIFY-CHECK-R2/design v4+v3；修复轮 + 回归 |
| 3 | 03-kmain-cstart | doc + code，deep | ✅ CONVERGED_WITH_WARNING（P0=0） | 2026-11-02 | 2026-11-02 | 03-review/03-review-trajectory/scan/SYMBOLS/structure/VERIFY-CHECK/design v2/outline v4+review v4；修复轮 + QEMU 三架构 + test-smp-aps |
| 4 | 04-platform-discovery | doc + code，deep；修复 + R2 回归 | ✅ CONVERGED_WITH_WARNING_BACKLOG（P0=0） | 2026-11-02 | 2026-11-03 | 04-review/04-review-trajectory/scan/SYMBOLS/structure/VERIFY-CHECK/design v2/outline v4+review v4；QEMU 三架构 test-smp-topo |
| 5 | 05-clock-interrupt-init | doc + code，deep；修复 + R2 回归 | ✅ CONVERGED_WITH_WARNING_BACKLOG（P0=0） | 2026-11-02 | 2026-11-03 | 05-review/05-review-trajectory/scan/SYMBOLS/structure/excellence/test-audit/VERIFY-CHECK/VERIFY-CHECK-R2/design v3/outline v4+review v4；QEMU 三架构 test-timer-irq |

## 03-kmain-cstart（2026-11-02，独立计数，不并入上方 01 状态）

- **Status**: **CONVERGED_WITH_WARNING**（P0=0；P1/P2 无 open doc；Gate A 工具分母、A5 待回）
- **Blocker Gates**: 0✅ A⚠️ PARTIAL（人工射程分档无漏写） B✅ C✅ D✅ D-6✅ E✅（QEMU 三架构 + test-smp-aps） G✅（A5 回归二轮复核 PASS） H✅（以 v2）
- **P0**: 0 open（6/6 CLOSED；AP TSS.sp0 由 `test-smp-aps` 真机 PASS 证实）
- **P1/P2**: 0 open doc（P1：24 CLOSED、1 WITHDRAWN 规则层、1 TOOL-LIMIT；P2：14 CLOSED、1 CORRECTED、1 WITHDRAWN 风格偏好）
- **产物**: `rewrite-notes/01-stage-kernel/03-review.md`（含第二轮回归 R2-1..R2-8）、`03-review-trajectory.md`；`.review/zcode/01-stage-kernel/03-kmain-cstart/{scan,SYMBOLS,structure}.md`；`.review/zcode/01-stage-kernel/VERIFY-CHECK.md`；`.design/03-design.v2.md`、`03-outline.v4.md`、`03-outline-review.v4.md`
- **Next**: 提交本轮修改（03 文档 + 关联代码 + test-protection + review 产物）
- **注意**：`.review/zcode/01-stage-kernel/VERIFY-CHECK.md` 的 A4 独立验证为 87.5% CONCERN（预修复）；主会话同 agent 95% 只作暂测；修复后的回归门禁与 QEMU 读数见 `03-review.md` R2-4。

## 05-clock-interrupt-init（2026-11-03，独立计数，不并入上方 01 状态）

- **Status**: **CONVERGED_WITH_WARNING_BACKLOG**（P0=0；P1=0；P2=0 open；R05-T01/T05/T06 backlog）
- **Blocker Gates**: 0✅ A✅ B✅ C✅ D✅ D-6✅ E✅ G✅（R2 12/12=100%） H✅
- **P0**: 0 open（11/11 fixed；PIC 从片 0x70、aarch64 空链 claim 回执、文档 7 类事实/引文缺口）
- **P1/P2**: 0 open（13/13、8/8 fixed；P2-design-01 由 v3 快照关闭）
- **产物**: `rewrite-notes/01-stage-kernel/05-review.md`（§17 R2）、`05-review-trajectory.md`；`.review/zcode/01-stage-kernel/05-clock-interrupt-init/{scan,SYMBOLS,structure,excellence,test-audit,VERIFY-CHECK,VERIFY-CHECK-R2}.md`；`.design/05-design.v3.md`、`05-outline.v4.md`、`05-outline-review.v4.md`
- **Next**: 清 R05-T01（位级寄存器真值）、R05-T05（用户态 tick）、R05-T06（风暴注入）；跨 agent 复核可选
- **注意**: QEMU x86_64/aarch64/riscv64 `test-timer-irq` 均 PASS；位级寄存器/用户态 tick/风暴注入仍未证，不得写成已验证。

## Convergence Checklist
- [x] §2.1 概念准确性 — DONE（P0-fact-01/03/05 等）
- [x] §2.2 C引用验证 — DONE（锚点 98/98；路径 40/41）
- [x] §2.3 数据结构覆盖 — DONE（kinfo 26 字段逐项）
- [x] §2.8 源码覆盖完整性 — DONE（SYMBOLS 192 语义域）
- [x] §2.9 设计决策质量 — DONE（outline-review D1–D9）
- [x] §2.10 章节链路 — DONE（structure.md §8/§12）
- [x] Code §1-14 — DONE（gate-evidence-code）
- [x] 跨文档联动 — DONE（§7 路径与 02–05 回指）
- [x] Claims-Evidence — DONE（P0 全部主会话重放）
- [x] 独立验证 (Gate G) — DONE（95.2% CONCERN；R2 回归门禁全绿）

## 受审正文绑定
- `sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 修复前：`fda90e0045ed4ba1e7675e833ef99b46c1b770cdebcb5cd6dc52961e05b5f776`
- 修复后：`ee4e9fc6a48e889fdbc62c185581fe517c7783c51792f2ec068afd8470dc7b5d`


## 04-platform-discovery（2026-11-02，独立计数，不并入上方 01 状态）

- **Status**: **CONVERGED_WITH_WARNING_BACKLOG**（P0=0；43/43 closed；test-smp-aps 基线失败单列）
- **Blocker Gates**: 0✅ A⚠️ PARTIAL（84/213=39.4%，人工射程分档无漏写；`--check-filled` exit=0） B✅ C⚠️ PARTIAL（3 条升级 P0） D✅（带 P1） D-6✅（带 P1/P2） E✅（宿主 18+65+857；QEMU topo 三架构 + timer-irq/protection/ipi-riscv64/aps-aarch64 PASS；test-smp-aps 基线失败单列） G✅（机械 PASS；同 agent 8/8=100%） H✅（以 v2/v4）
- **P0**: 0 open（6/6 CLOSED；P0-fact-lapicConsumed 以 check-and-reject 关闭，动态窗口 R04-T09）
- **P1/P2**: 0 open / 0 open（18/18、19/19 CLOSED；详见 `VERIFY-CHECK-R2.md`）
- **产物**: `rewrite-notes/01-stage-kernel/04-review.md`、`04-review-trajectory.md`；`.review/zcode/01-stage-kernel/04-platform-discovery/{scan,SYMBOLS,structure,VERIFY-CHECK,VERIFY-CHECK-R2}.md`；`.design/04-design.v2.md`、`04-outline.v4.md`、`04-outline-review.v4.md`；`tmp/review04/{A1-codereview,A2-student,A3-structure}.md`、`qemu-topo.log`
- **Next**: R04-T09/T10/T11 设计缺口与 R04-T01..T08 测试缺口作为 backlog；`test-smp-aps` x86 基线失败另开修复轮
- **注意**: Gate A 的 39.4% 是语义域分母过宽（跨篇并集），不是本篇漏写率；Gate G 为同 agent 验证，不宣称异构独立。
