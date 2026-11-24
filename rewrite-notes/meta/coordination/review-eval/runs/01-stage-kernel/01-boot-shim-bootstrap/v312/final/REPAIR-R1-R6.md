# D01 接缝修复 R1–R6

DATE: 2026-11-24
BEFORE: `c53a80075cbb8a74c4889af9b3dcd5309993065ec58d3676a463e8b3fbd6536c`（2019 行）
AFTER: `b5ceb02cc8834e640711340d8e88808213e7d104e982084a5411a43fc55c5cff`（2020 行）

## 范围

只修六视角汇总的 R1–R6；R7（入口计数 4/3/5）与 R8（02 篇反向引用 01 §3.7）未动。

## R1 镜像位置来源与交付义务

- Ch1 `§1.1`：改为「镜像位置由链接时的布局结果确定：引导器负责把镜像放到链接指定的地址，内核从同一份布局读出位置，不必从交接结构复制」；
  按值交付的范围收为「内存图与交接结构」，并补一句「可重建的数据不必跨边界复制」。
- Ch1 `§1.5` 示例的 I2/I3：来源从「引导器按加载记录」改为「链接时的布局结果」。
- 与 Ch2 `§2.3` 的「不从引导器读、由链接常量重建」对齐，不再互相否定。
- 交接结构的定义同步收口：§1.1 表格、§1.1 正文、§1.4、§1.5 的 I1 前提、本章小结与术语表，全部从「承载前两类」改为「承载内存图与其他必要启动参数」；镜像位置不再被定义为交接结构的载荷。

## R2「内存账」归属错章

- Ch1 术语：改为「本章对可用物理内存清单的称呼，对应 §1.1 的内存图；第 2 章会看到它在 C 侧的落点」，删除 `kinfo.memmap[]`。
- Ch2 `§2.0`：「本章会首次出现 `struct kinfo`」改为「本章会用到」，不再与 Ch1 术语条冲突。
- Ch2 术语表补「内存账」条：C 侧 `kinfo.memmap[]` 里过滤后的可用物理内存清单。

## R3 五条生产腿前置知识

- Ch5 `§5.0`、Ch6 `§6.0`：前置概念从「第 1 章的 I1–I4 与五条生产腿」改为「第 1 章的 I1–I4 与第 3 章的五条生产腿」。

## R4 换栈归属

- 章表：Ch4 行删去「换栈」，只保留「装载、建根、映射、跳转」；换栈的判据与证据仍指向 02 篇。
- Ch2 `§2.8` 的 `k_initial_stktop` 指针同时改指 02 篇 §2.5（原 §2.4）。

## R5「02 篇/镜像篇/下一篇」

- 正文全部「下一篇（镜像篇）」改为「02 篇（镜像篇）」；首用处（开头引文与 Ch1 §1.3）加 `02-higher-half-kernel.md` 链接。
- Ch3 §3.1 的「02 篇」定义补文件名。

## R6 02 篇指针簇

- `mem_high_phys` 行：C 侧两份 `pagedir` 改指 02 篇 §2.2（两份 `pg_utils.o`）与 §2.4（`pagedir`）。
- `k_initial_stktop`：02 篇 §2.4 改为 §2.5（02 §2.5 是「`k_initial_stktop` 落在哪一段」）。
- `alloc_pagetable`：02 篇 §2.4 不覆盖该符号，改指 `minix3/minix/kernel/arch/i386/pg_utils.c:alloc_pagetable`；
  `pg_alloc_page` 的 02 指针删除，保留本篇 §2.4.2。
- 借槽链（`freepde_start`/`createpde`/`virtual_copy_f`）：02 篇 §4.3 不覆盖，改指 `minix3/minix/kernel/arch/i386/memory.c`。
- additive Step 4：02 篇 §4.3 明说 DM 不展开，改为「02 篇 §3.6/§4.5 只确认它在跳转前完成；完整机制归独立内存管理主题」。
- `freepde_start` 索引行：02 篇 §4.3 改为 §2.4（该节有 `kinfo.freepde_start = pg_mapkernel()`）。

## 同步产物

- `v312/summaries/ch1-summary.md`：镜像位置来源、按值交付范围、交接结构载荷、`02 篇` 称法；切片头更新为 `ch1-8ec90a1b.md`。
- `v312/summaries/ch2-summary.md`：`下一篇` 改为 `02 篇（镜像篇）`。
- `v312/chapter-slices/` 修复后快照：Ch1 `8ec90a1b`、Ch2 `5adbbe1f`、Ch3 `848bfdf1`、Ch5 `4a4054b3`、Ch6 `90b309c3`；Ch4 未变（`a1b899d7`）。

## 机械门读数

- `doc-style-lint.sh`：error 级 0。
- `doc-publication-check.py`：无新增违规。
- `chapter-contract-check.py`：PASS（Ch2/Ch4/Ch6 代码行数、Ch4 设计词为 WARN）。
- `tutorial-rubric-check.py`：6 章，0 章缺项。
- `doc-citation-check.py`：failed=0，warned=4。
- `doc-ref-check.sh`：内部章节引用均可解析。
- `doc-code-density-check.sh`：承载门阻断项通过。
- `summary-anchor-check.py`：六份摘要全 PASS。

## 仍未做

- 受影响章的章级六视角、摘要 FAITHFUL 门、`chapter-freeze` 事件均未重跑；`CHAPTER-FREEZE.md` 仍是修复前记录，已加 stale 注记。
- 最终整篇六视角未重跑；六份 final 报告仍对应 `c53a8007`，只能作修复前证据。
- `REPAIR-VERIFY.md` 第一轮判 R1 NOT-CLOSED（交接结构仍承载前两类）；上述定义收口后已请同一复核员重判。
- R7、R8 未修。
