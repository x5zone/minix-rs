# D01 CONVERGED（V3.1，R30 收口；已被 R31 推翻）

> **R31 追加轮找到了 53 条（含 1 条 P0、3 条 P1）**，R30 的 `CONVERGED` 不成立。
> 当前状态见 `loop.md` R31：`NOT CONVERGED`。本文件保留为 R30 时点的记录。

> 收敛口径：按用户授权的代理裁决 A——“所有已登记问题都有处置且 open=0”，
> 不是“证明零未知缺陷”。

## 一、轮次

- R28（V3.1 第 1 轮，结构轮）：R27 backlog 37 个发现 ID / 32 个根因关闭。
- R29（V3.1 第 2 轮，完整 survey）：55 条全部关闭（① 10 / ④ 35 / ⑥ 9 / FP 1 / ② 0 / ⑤ 0）。
- R30（V3.1 第 3 轮，完整 survey）：57 条全部关闭（① 10 / ④ 39 / ⑥ 8 / FP 0 / ② 0 / ⑤ 0）。
- R27 台账已回填 70 行（64 FINDING + 6 CLAIM）。
- R25/R26/R28/R29/R30 的 `eval-trace-check.py --round` 均 PASS；R25 有 3 个历史 WARN。

## 二、最终证据

- 文档六门：citation 69 块 / 29 检查 / 0 失败；anchor 105/105；ref/structure/style/density 全绿。
- 宿主：`cargo test -p minix-kernel --features mock` 866 passed / 8 ignored；
  `cargo test -p boot-shim --features test-all --lib` 136 passed；
  `cargo test -p minix-boot` 196 passed。
- 构建：`cargo check -p kernel-image` 的 riscv64/aarch64/x86_64 三目标通过。
- QEMU：5 PASS（riscv64 bootface、aarch64 SMP AP、riscv64 U-Boot shim、x86 UEFI、aarch64 UEFI）
  + 1 环境 SKIP（riscv64 U-Boot 直载链在宿主 U-Boot 2025.10 上装载故障，内核未执行；
  脚本按自身契约 exit 2，不写成 PASS）。

## 三、规则改动

V3/V3.1 的轮次交易、根因身份、准入四判据、B 类结构重写、读者三问、edition 纪律、
最终 HEAD 机械再对账、第 3 轮结构轮，均已落地并在 R25–R30 实际运行。

## 四、限制

- 不声称“零未知缺陷”；CONVERGED 的含义是“本 edition 的完整 survey + certify 下没有 open 项”。
- R30 的 57 条由主 agent 修复、同 agent 冷读；独立第二读者未做。
- QEMU 直载链的宿主 U-Boot 兼容缺口保留；若用户要求 6/6 全绿，需换 U-Boot/CI 环境。
- 后续新证据或 V3.2 可重开；重开不是本次失败的反悔。
