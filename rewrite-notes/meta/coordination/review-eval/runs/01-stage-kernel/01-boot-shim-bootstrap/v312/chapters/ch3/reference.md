DOC: 01-boot-shim-bootstrap.md
CHAPTER: 3. Rust 设计决策
HARD-REJECTIONS: 0
VERDICT: NEEDS-WORK

行号按 `ch3.md` 切片（586 行）。输入只含本章切片、ch2 摘要、章契约与 scope；C 侧只做符号/引文抽查。

R5/R6 五项复核：
1. `alloc_pagetable`：PASS。L322 "6 页静态池，用完 panic" 与 `minix3/minix/kernel/arch/i386/pg_utils.c:123-129` 一致（`PG_PAGETABLES 6`、`static u32_t pagetables[6][1024]`、`panic("no more pagetables")`）；L324 的 `PG_PAGETABLES = 6` 亦对。
2. 借槽链不再声称由 02 §4.3 展开：PASS。全文无 "02 篇 §4.3"；L349/L351 只给 `memory.c` 的 `createpde`/`virtual_copy_f`。
3. `freepde_start` 指 02 §2.4：FAIL。L349 只说 "C 的 `freepde_start` 是 C 侧借槽链的起点" 并给 `memory.c`；L171 的 `freepde_start` 行只指 §3.4。`freepde_start` 的赋值 `kinfo.freepde_start = pg_mapkernel()` 在 02 §2.4（`02-higher-half-kernel.md:321`），`memory_init` 只消费它（`memory.c:711-712`）；本节没有这个指针。另 L351 "这套机制的三个前提（完整陈述见 `memory.c` 的 `createpde`/`virtual_copy_f` 段）" 把本节的三条 Rust 侧判据误推给 C 源，C 源只描述机制。建议 L349 或 L171 补 "赋值/建表见 02 篇 §2.4"，L351 改为 "C 侧机制见 `memory.c`；三个前提是本节判据"。
4. Step 4 DM：PASS。L485 "02 篇 §3.6/§4.5 只确认它在跳转前完成" 与 02 §3.6/§4.5 的时序确认一致。
5. `k_initial_stktop`：PASS。L296 "其落点见 02 篇 §2.5"；02 §2.5 标题即 "`k_initial_stktop` 落在哪一段"。

其余抽查：`createpde`（memory.c:69）、`virtual_copy_f`（:592）、`pg_alloc_page`（pg_utils.c:138）、`pt_free`（servers/vm/pagetable.c:1427）、`k_initial_stktop`（i386/head.S:84、i386/mpx.S:634）均存在。非阻断小项：L55 "02 篇的 4.5 节 的实现现状" 多一空格；L72 "另外四节" 实列 §2.4.2/§4.5/§4.6 三个不同节。
