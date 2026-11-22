# D01 CONVERGED（V3.10，R39 确认轮，2026-11-22）

> 收敛口径：`review-evals.md` §6A 四态模型——确认轮在最终工作树上 `certify 新准入 = 0`，
> G1–G14 全过，`OPEN-DEBT`/TODO 账本无未销账行。CONVERGED 表示“本 edition 的完整 survey
> 与确认轮下没有 open 项”，不声称“零未知缺陷”。

## 一、轮次

- R38（V3.10 新 edition，full）：29 条面板/设计发现；DESIGN-01/03 驳回，DESIGN-02/05/06/10 当场实施；DESIGN-07/08、R14-04、R37-05 转作者裁决。
- R38 作者裁决批次（D-110..D-114）：DESIGN-07/08 保留现有表示并补责任注释；R14-04 补 §4.8 Direct Map 概念入口；R37-05 免费模型 6/6 复核；I-25 只修 D01 关联 48 个 `.rs`；DESIGN-04/09 批准实施。
- R39 确认轮：I-25（门 bug + 48 文件 CP 清零 + 注释英文化）、DESIGN-04/09、I-43/rt-birth-sp 全部关闭；49/49 单元冻结、T3 PASS；冷启动抽样 10 单元，D1–D7 修复后 `DELTA: PASS`。

## 二、最终证据

- 文档七门：publication 0 新增；tutorial 6 章 0 缺项；structure/ref 通过；citation `blocks=54 checked=21 failed=0`；style `error=0`；density 承载门通过；anchor 96/96。
- 关联代码：`doc-code-map --check` 48/48；`code-publication-check` 48 文件 0 命中；48 文件注释 CJK 0 行。
- 宿主：minix-arch 266/0、minix-boot 188/0、minix-types 342/0、boot-shim 136/0、minix-vm 548/0、minix-kernel mock 866/0/8。
- QEMU：x86/aarch64 bootmarks PASS、riscv64 U-Boot shim PASS（6/6）、rt-birth-riscv64 PASS（五个用户态标记全命中）。
- A2 判读：自由集 `[0x80000000,0x80200000)`、`[0x80400000,0x81000000)`、`[0x81001000,0x82200000)`、`[0x82301000,0x8fe00000)`；BSP 栈 `0x803a2108..0x803b2108`；U-ecall 栈 `0x803ff000..0x80400000`；交集为空。
- 账本：D01 `todo-ledger check --fail-on-open` 为 0；`review-units audit` 全部单元 open=0。

## 三、轮的边界

- x86/aarch64 `rt-birth` 用户链在 2026-11-22 仍失败；x86 已在 `c9f4e544c` 独立 worktree 复现同一日志，aarch64 由 `FREEZE.md:71` 记录为既有 A2 遗留。两者不在 D01 §5.4 的五条启动门内，登记为 stage 级 `D01-RT-BIRTH-USERCHAIN`，不挡 D01。
- 全仓代码出版物债（48 文件之外）登记为 `COORD-CODE-PUBCLEAN`，按 stage 顺序推进，不反向阻断 D01。
- 确认轮修复 2/2 批次；未开新修改轮。确认轮发现见 `certify/round39-confirmation.md` 与 `defect-ledger.md` 的 `RD-R39-001..009`。

---

## 历史记录：R30 CONVERGED（已被 R31 推翻，仅存档）

> **R31 追加轮找到了 53 条（含 1 条 P0、3 条 P1）**，R30 的 `CONVERGED` 不成立。
> 本段保留为 R30 时点的记录，不是当前状态。

> 收敛口径：按用户授权的代理裁决 A——“所有已登记问题都有处置且 open=0”，
> 不是“证明零未知缺陷”。

### 一、轮次

- R28（V3.1 第 1 轮，结构轮）：R27 backlog 37 个发现 ID / 32 个根因关闭。
- R29（V3.1 第 2 轮，完整 survey）：55 条全部关闭（① 10 / ④ 35 / ⑥ 9 / FP 1 / ② 0 / ⑤ 0）。
- R30（V3.1 第 3 轮，完整 survey）：57 条全部关闭（① 10 / ④ 39 / ⑥ 8 / FP 0 / ② 0 / ⑤ 0）。
- R27 台账已回填 70 行（64 FINDING + 6 CLAIM）。
- R25/R26/R28/R29/R30 的 `eval-trace-check.py --round` 均 PASS；R25 有 3 个历史 WARN。

### 二、最终证据

- 文档六门：citation 69 块 / 29 检查 / 0 失败；anchor 105/105；ref/structure/style/density 全绿。
- 宿主：`cargo test -p minix-kernel --features mock` 866 passed / 8 ignored；
  `cargo test -p boot-shim --features test-all --lib` 136 passed；
  `cargo test -p minix-boot` 196 passed。
- 构建：`cargo check -p kernel-image` 的 riscv64/aarch64/x86_64 三目标通过。
- QEMU：5 PASS（riscv64 bootface、aarch64 SMP AP、riscv64 U-Boot shim、x86 UEFI、aarch64 UEFI）
  + 1 环境 SKIP（riscv64 U-Boot 直载链在宿主 U-Boot 2025.10 上装载故障，内核未执行；
  脚本按自身契约 exit 2，不写成 PASS）。

### 三、规则改动

V3/V3.1 的轮次交易、根因身份、准入四判据、B 类结构重写、读者三问、edition 纪律、
最终 HEAD 机械再对账、第 3 轮结构轮，均已落地并在 R25–R30 实际运行。

### 四、限制

- 不声称“零未知缺陷”；CONVERGED 的含义是“本 edition 的完整 survey + certify 下没有 open 项”。
- R30 的 57 条由主 agent 修复、同 agent 冷读；独立第二读者未做。
- QEMU 直载链的宿主 U-Boot 兼容缺口保留；若用户要求 6/6 全绿，需换 U-Boot/CI 环境。
- 后续新证据或 V3.2 可重开；重开不是本次失败的反悔。
