DOC: 01-boot-shim-bootstrap.md
CHAPTER: 3. Rust 设计决策
HARD-REJECTIONS: 0
VERDICT: NEEDS-WORK

输入：ch3 切片（586 行）、章契约 §一/§三/§五、`summaries/ch2-summary.md`、`SCOPE.md`；只判导航，不判事实。

**可达结论**
- 节序：§3.0–§3.7 共 8 节，与 SCOPE「第 3 章 3.0–3.7，1+8」一致；无缺号、无跳号。
- 章内 §3.x 引用 12 个去重值全部命中现有标题（`grep -o '§3\.[0-9.]*' | comm -23 标题集` 空集），无死指针。
- 跨章 §4.x/§5.x 目标标题全部存在：4.1/4.2/4.2.1/4.2.2/4.3/4.4/4.4.1/4.5/4.6/4.8、5.1/5.3/5.4。
- 02 篇指路逐条与目标标题吻合（`grep -n '^#{2,5} ' 02-higher-half-kernel.md`）：§2.2 两段式=两份 `pg_utils.o`、§2.4 建表五步=`pagedir`、§2.5 标题即 `k_initial_stktop` 落点、§3.6 自举根与映射建立、§4.5 完整路径（4.5.1 直核自引导可承载 VMA=LMA 测试内核）。
- Ch2 指向物实测存在：§2.5.1「还 VM 镜像」「还引导段」两行在 ch2.md:302/303；§2.5 含 `_kern_size`（ch2.md:289）。
- C 锚点 href 全部解析（`tools/anchor-resolve.sh --check`）：`pg_utils.c:alloc_pagetable`@123、`memory.c:createpde`@69、`memory.c:virtual_copy_f`@592。

**问题**
1. [门禁] 357 行 ``[`memory.c:createpde`](minix3/minix/kernel/arch/i386/memory.c:createpde)``：链接文字被 `tools/anchor-resolve.sh` 当裸锚点，同名文件多定义 → 全篇 `anchors=94 resolved=93 zero-def=0 multi-def=1`、exit 1（doc:943）。href 可解析；把链接文字改成 `createpde` 或写全路径即可。
2. [交接] 章末未把读者交给第 4 章：`第 4 章` 全章只在 19 行（§3.1 开头）；小结 572–579 与术语 580–586 收尾无指路句。Ch1 有同类收尾（ch1.md:110），本章可补一句。
3. [轻微] 55 行「02 篇的 4.5 节 的实现现状」多一“的”，指路短语被拆断。
4. [轻微] 95 行「见 §3.2 的字段去向表」实际在 §3.2.2.1；193 行「见 §3.2 的…三条 bullet」实际在 §3.2.1；与 85 行的精确写法不一（可达，粒度偏粗）。
5. [轻微] 23/29/291/319 行裸写 `2.1`/`2.2`/`2.3`/`2.4`，与全章 `§2.x` 体例不一（仍可解析）。
