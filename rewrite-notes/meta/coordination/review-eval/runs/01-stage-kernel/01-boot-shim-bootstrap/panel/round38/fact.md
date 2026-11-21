# P-FACT 独立事实审查 · round38（增量草稿，查证进行中）

目标文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2941 行）
审查口径：只读；机械门读数 + C/Rust 源码逐项对账。

## 机械门读数（本轮已跑）

- `tools/anchor-resolve.sh --check <doc>`：`anchors=89 resolved=89 zero-def=0 multi-def=0`。
- `tools/doc-code-map.sh <doc> --check`：`共 48 个文件；存在性 48/48 通过`。
- `python3 tools/doc-structure-check.py <doc>`：表结构与内部链接均可解析。
- `tools/doc-ref-check.sh --check <doc>`：内部章节引用均可解析。
- `tools/doc-style-lint.sh --diff`：diff 范围 0 个文件，增量门通过。
- `python3 tools/doc-citation-check.py <doc>`：`blocks=54 checked=21 failed=0 skipped=29 warned=4`（warn 块：1393-1435、1439-1460、1930-2030、2225-2253）。

## FINDING FACT-01（P1）

- 位置：文档第 492 行，§2.4 `add_memmap` 代码引用块（标注「代码引用，minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap」）。
- 严重度：P1（引文失真；不影响语义，但违反“代码引用逐字或显式节选”的门）。
- 问题：引文把源码注释 `Truncate available memory at 4GB` 写成 `Truncate available memory at 4 GiB`，块内未标“节选/改写”。
- 影响：读者按引文回查源码会 grep 不到；后续若有人以引文为准复制注释，会与 C 源不一致。
- 证据：E1 工具+直接对读。`grep -n 'Truncate available memory' minix3/minix/kernel/arch/i386/pg_utils.c` → `90:        /* Truncate available memory at 4GB as the rest of minix`；文档同句 → 第 492 行 `4 GiB`。
- 最小修复：把块内 `4 GiB` 改回 `4GB`（其余行不动）。
- 置信度：高。
- 是否需源码核实：已核实（见命令）。
- 若我错了：若项目约定允许把注释里的 `4GB` 规范化为 `4 GiB` 且该块已声明“非逐字”，则降为 SUGGESTION；但块头只写“代码引用”，没有节选/改写标注，判 P1 成立。
- 对应判据：文献引用纪律（代码引用块逐字/节选标注）；doc-citation-check 未覆盖此行（该工具 failed=0 不能证明逐字）。

## ZERO-FINDINGS（草稿说明，最终版替换）

其余检查项（路径/常量/签名/测试名/测试计数/覆盖率/反事实前提）尚未全部完成，最终报告将补齐各条“扫了什么、发现数、N=0 时不覆盖什么”。
