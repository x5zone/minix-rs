# D04 确认轮机械门证据（2026-11-22）

> **INVALIDATED（2026-11-22）**：本文件中的 R39 是 D01 的轮次，不是本篇轮次；D-114 仅适用于 D01。原“CONVERGED”宣告作废，状态为 CONVERGENCE-CANDIDATE。纠正记录见 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/ROUND-IDENTITY-CORRECTION.md`。

裁决依据：无（原写 D-114 无效；D-114 仅适用于 D01 的 48 个关联文件）

## 文档门

- `python3 tools/doc-publication-check.py <doc>` → `无新增违规`
- `python3 tools/tutorial-rubric-check.py <doc>` → `检查 6 章，0 章缺项`
- `python3 tools/doc-structure-check.py <doc>` → `表结构与内部 .md 链接均可解析`
- `python3 tools/doc-citation-check.py <doc>` → `blocks=93 checked=68 failed=0 skipped=18 warned=7`
- `bash tools/doc-ref-check.sh <doc>` → `内部章节引用均可解析`
- `bash tools/doc-style-lint.sh <doc>` → `error级=0`
- `bash tools/doc-code-density-check.sh <doc>` → `承载门阻断项通过（4 个可判章均有码/命令）`
- `bash tools/anchor-resolve.sh --check <doc>` → `anchors=76 resolved=76 zero-def=0 multi-def=0`
- `bash tools/doc-code-map.sh <doc> --check` → `53 个文件；存在性 53/53 通过`

## 代码门

- `python3 tools/code-publication-check.py <53 个 related-code>` → `命中 0；无新增违规`
- 53 文件 `grep -nP '//.*[\x{4e00}-\x{9fff}]'` → 0 行
- `cargo test -p minix-arch --lib` → 266/0
- `cargo test -p minix-platform --lib` → 19/0
- `cargo test -p minix-kernel --features mock --lib` → 866/0/8
- `cargo check -p minix-plat` → 通过

## 真机门

- `test-smp-topo`（x86_64）→ `### TEST_RESULT: PASS test-smp-topo ###`
- `test-smp-topo-aarch64` → `### TEST_RESULT: PASS test-smp-topo-aarch64 ###`
- `test-smp-topo-riscv64` → `### TEST_RESULT: PASS test-smp-topo-riscv64 ###`

## 账本

- `python3 tools/todo-ledger.py check --doc 04-platform-discovery --fail-on-open` → 匹配 0 行，exit 0

## 单元账本

- `review-units.py sync <doc>` → 54 个单元（6 章 + 48 节）
- `review-units.py check <doc>` → `units=54 frozen=54 chapters=6`，exit 0
- `review-units.py audit --doc <doc>` → `audit: OK（全部单元 open=0）`
- `review-units.py gate <doc> --pass --evidence <本文件>` → `gate T3: PASS`
- `review-units.py sample <doc> --seed <HEAD>` → 抽样 11 个单元：4.0、4.1、4.11、3.8、5.1、3.9、1.5、5、5.2、4.5、2.0
