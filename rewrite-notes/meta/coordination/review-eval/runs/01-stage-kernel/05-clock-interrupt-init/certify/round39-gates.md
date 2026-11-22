# D05 确认轮机械门证据（2026-11-22）

> **INVALIDATED（2026-11-22）**：本文件中的 R39 是 D01 的轮次，不是本篇轮次；D-114 仅适用于 D01。原“CONVERGED”宣告作废，状态为 CONVERGENCE-CANDIDATE。纠正记录见 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/ROUND-IDENTITY-CORRECTION.md`。

裁决依据：无（原写 D-114 无效；D-114 仅适用于 D01 的 48 个关联文件）

## 文档门

- `python3 tools/doc-publication-check.py <doc>` → `无新增违规`
- `python3 tools/tutorial-rubric-check.py <doc>` → `检查 6 章，0 章缺项`
- `python3 tools/doc-structure-check.py <doc>` → `表结构与内部 .md 链接均可解析`
- `python3 tools/doc-citation-check.py <doc>` → `blocks=68 checked=43 failed=0 skipped=20 warned=5`
- `bash tools/doc-ref-check.sh <doc>` → `内部章节引用均可解析`
- `bash tools/doc-style-lint.sh <doc>` → `error级=0`
- `bash tools/doc-code-density-check.sh <doc>` → `承载门阻断项通过（4 个可判章均有码/命令）`
- `bash tools/anchor-resolve.sh --check <doc>` → `anchors=103 resolved=103 zero-def=0 multi-def=0`
- `bash tools/doc-code-map.sh <doc> --check` → `33 个文件；存在性 33/33 通过`

## 代码门

- `python3 tools/code-publication-check.py <33 个 related-code>` → `命中 0；无新增违规`
- 33 文件 `grep -nP '//.*[\x{4e00}-\x{9fff}]'` → 0 行
- `cargo test -p minix-arch --lib` → 266/0
- `cargo test -p minix-kernel --features mock --lib` → 866/0/8

## 真机门

- `test-timer-irq`（x86_64）→ `### TEST_RESULT: PASS test-timer-irq ###`
- `test-timer-irq-aarch64` → `### TEST_RESULT: PASS test-timer-irq-aarch64 ###`
- `test-timer-irq-riscv64` → 连续 4 次 `### TEST_RESULT: PASS test-timer-irq-riscv64 ###`（修复 UART loopback 采样竞态后）

## 账本

- `python3 tools/todo-ledger.py check --doc 05-clock-interrupt-init --fail-on-open` → 匹配 0 行，exit 0

## 单元账本

- `review-units.py sync <doc>` → 47 个单元（6 章 + 41 节）
- `review-units.py check <doc>` → `units=47 frozen=47 chapters=6`，exit 0
- `review-units.py audit --doc <doc>` → `audit: OK（全部单元 open=0）`
- `review-units.py gate <doc> --pass --evidence <本文件>` → `gate T3: PASS`
- `review-units.py sample <doc> --seed <HEAD>` → 抽样 10 个单元：6、5.3、4.2、3.0、3.6、4.8、6.2、2.6、2.5、4.7

## 测试载体修复

- `test-timer-irq-riscv64`：`kern_virt_base/kern_phys_base/kern_size` 改为 Sv39 高半 + DRAM 4 MiB 覆盖；timer 载体不加载 VM ELF（stub 无 PT_LOAD），故不调 `init_proc_and_boot`。
- `uart_irq::enable_uart_rx_selftest`：THR 写后轮询 LSR.DR，再清 LOOP，消除 RISC-V 首次源 1/3 抖动；连续 4 次 PASS。
