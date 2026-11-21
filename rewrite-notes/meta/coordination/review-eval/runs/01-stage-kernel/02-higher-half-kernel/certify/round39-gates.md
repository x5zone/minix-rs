# D02 确认轮机械门证据（2026-11-22）

裁决依据：D-114

## 文档门

- `python3 tools/doc-publication-check.py <doc>` → `无新增违规`
- `python3 tools/tutorial-rubric-check.py <doc>` → `检查 6 章，0 章缺项`
- `python3 tools/doc-structure-check.py <doc>` → `表结构与内部 .md 链接均可解析`
- `python3 tools/doc-citation-check.py <doc>` → `blocks=47 checked=27 failed=0 skipped=15 warned=5`
- `bash tools/doc-ref-check.sh <doc>` → `内部章节引用均可解析`
- `bash tools/doc-style-lint.sh <doc>` → `error级=0`
- `bash tools/doc-code-density-check.sh <doc>` → `承载门阻断项通过（4 个可判章均有码/命令）`
- `bash tools/anchor-resolve.sh --check <doc>` → `anchors=66 resolved=66 zero-def=0 multi-def=0`
- `bash tools/doc-code-map.sh <doc> --check` → `30 个文件；存在性 30/30 通过`

## 代码门

- `python3 tools/code-publication-check.py <30 个 related-code>` → `命中 0；无新增违规`
- 30 文件 `grep -nP '//.*[\x{4e00}-\x{9fff}]'` → 0 行
- `cargo test -p minix-arch --lib` → 266/0
- `cargo test -p minix-elf --lib` → 27/0
- `cargo test -p boot-shim --lib` → 136/0
- `cargo test -p minix-kernel --features mock --lib` → 866/0/8
- `cargo check -p minix-rs -p minix-vfs` → 通过

## 真机/布局门

- `test-higher-half`（x86_64）→ `### TEST_RESULT: PASS test-higher-half ###`
- `test-higher-half-aarch64` → `### TEST_RESULT: PASS test-higher-half ###`
- `test-higher-half-riscv64` → `### TEST_RESULT: PASS test-higher-half ###`
- `bash os/kernel-image/check-layout.sh all` → `CHECK-LAYOUT=PASS（all）`

## 账本与跨模型

- `python3 tools/todo-ledger.py check --doc 02-higher-half-kernel --fail-on-open` → `open=0 blocked=0`
- 跨模型复核：`opencode/hy4-flash-free` 5 条断言；C1/C3/C4 CONFIRM，C2/C5 REFUTE 仅针对断言包措辞；结论“必须改代码/文档：无”。见 `crosscheck/round39-free.md`。

## 单元账本

（`review-units.py` 冻结后补记 check/audit 输出。）
