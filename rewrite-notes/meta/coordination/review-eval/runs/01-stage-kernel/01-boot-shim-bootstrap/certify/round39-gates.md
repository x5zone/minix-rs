# D01 确认轮机械门证据（2026-11-22）

裁决依据：D-114

单元冻结基线：R38 全篇 survey（`panel/round38/` 的 fact、reader-a/b/c、refute、design-d2）覆盖 §1–§6，29 条发现全部闭环（`defect-ledger.md` 的 `RD-R38-*`）。本轮只修复 6 个单元的锚点指向（2.7、3.2、3.4、3.5、4.7、6.1），未改正文语义。

## 文档门

命令与输出（工作树 HEAD `2489d793f` 之后的确认轮基线）：

- `python3 tools/doc-publication-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → `无新增违规`
- `python3 tools/tutorial-rubric-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → `检查 6 章，0 章缺项`
- `python3 tools/doc-structure-check.py <doc>` → `1 个文件表结构与内部 .md 链接均可解析`
- `python3 tools/doc-citation-check.py <doc>` → `blocks=54 checked=21 failed=0 skipped=29 warned=4`
- `bash tools/doc-ref-check.sh <doc>` → `内部章节引用均可解析`
- `bash tools/doc-style-lint.sh <doc>` → `error级=0`
- `bash tools/doc-code-density-check.sh <doc>` → `承载门阻断项通过（4 个可判章均有码/命令）`
- `bash tools/anchor-resolve.sh --check <doc>` → `anchors=89 resolved=89 zero-def=0 multi-def=0`
- `bash tools/doc-code-map.sh <doc> --check` → `共 48 个文件；存在性 48/48 通过`

## 代码门

- `python3 tools/code-publication-check.py --self-test` → `self-test PASS`
- `python3 tools/code-publication-check.py <48 个 related-code>` → `命中 0；无新增违规`
- 48 文件 `grep -nP '//.*[\x{4e00}-\x{9fff}]'` → 0 行
- `cargo test -p minix-arch` → 266 passed / 0 failed（含 `pt_alloc_write_once` 2/0、`pt_alloc_concurrent` 1/0）
- `cargo test -p minix-boot --lib` → 188 passed / 0 failed
- `cargo test -p minix-types --lib` → 342 passed / 0 failed
- `cargo test -p boot-shim` → 136 passed / 0 failed；`ebs_boundary_audit` 6/0
- `cargo test -p minix-vm --lib` → 548 passed / 0 failed
- `cargo test -p minix-kernel --features mock --lib` → 866 passed / 0 failed / 8 ignored

## 真机门

- `bash os/qemu-tests/test-shim-bootmarks-x86.sh` → PASS（`entering scheduler` 标记）
- `bash os/qemu-tests/test-shim-bootmarks-aarch64.sh` → PASS（`entering scheduler` 标记）
- `bash os/qemu-tests/test-riscv64-uboot-shim.sh` → `RESULT: PASS（6/6 markers, entering scheduler）`
- `bash os/qemu-tests/test-rt-birth-riscv64.sh` → `TEST_RESULT: PASS`；A2 自由集与 BSP/U-ecall 栈区间交集为空
- `bash os/qemu-tests/test-rt-birth.sh` / `test-rt-birth-aarch64.sh` → 既有用户链失败；在 `c9f4e544c` 独立 worktree 复跑 x86 得到同一日志，aarch64 由 `FREEZE.md:71` 记录为既有 A2 遗留。两条均不在 D01 §5.4 的五条启动门内，登记为 stage 级 `D01-RT-BIRTH-USERCHAIN`。

## 账本

- `python3 tools/todo-ledger.py check --fail-on-open --doc 01-boot-shim-bootstrap` → `open=0 blocked=0`，exit 0

## 单元账本

- `python3 tools/review-units.py sync <doc>` → 6 处 hash 更新（锚点修正后）
- `python3 tools/review-units.py check <doc>` → `units=49 frozen=49 chapters=6`，exit 0
- `python3 tools/review-units.py audit --doc <doc>` → `audit: OK（全部单元 open=0）`
- `python3 tools/review-units.py gate <doc> --pass --evidence <本文件>` → `gate T3: PASS`
- `python3 tools/review-units.py sample <doc> --seed <HEAD>` → 抽样 10 个单元：4.8、3.5、3.7、6.0、2.5、2.1、1.0、3.6、6.2、5.3
