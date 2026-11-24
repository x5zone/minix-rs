# D01 全篇机械化审计与最终六视角

VERDICT: FAIL（修复前六视角 0/6 PASS；R1–R6 已修，待重跑）
DATE: 2026-11-24
DOC_SHA256: c53a80075cbb8a74c4889af9b3dcd5309993065ec58d3676a463e8b3fbd6536c（修复前）

> **REPAIR 2026-11-24**：R1–R6 已修，当前 doc sha256
> `db6586a2babe3563d24d043792bc0d1272e95621fe54ba5088fce1c5fbf2d909`（1987 行）。
> 本文件下方的六视角结论、MAJOR 汇总与报告均针对修复前 sha，只作历史证据；
> 受影响章需重跑章级六视角、摘要门与冻结，之后再重跑最终整篇六视角。
> 修复明细见 `REPAIR-R1-R6.md`。R7（入口计数）与 R8（02 反向引用）未修。

## 机械门（PASS）

- 六章冻结：Ch1 `10950802`、Ch2 `672fc625`、Ch3 `d709dcfb`、Ch4 `a1b899d7`、Ch5 `fe2f6165`、Ch6 `735a9c5d`；
  `chapter-freeze-check.py` 全篇 PASS。
- 六份摘要锚点：`summary-anchor-check.py` 全 PASS。
- `doc-publication-check.py`：无新增违规。
- `doc-style-lint.sh`：error 级 0。
- `chapter-contract-check.py`：PASS（Ch2/Ch4/Ch6 代码总行数、Ch4 设计词为 WARN）。
- `tutorial-rubric-check.py`：6 章，0 章缺项。
- `doc-citation-check.py`：failed=0。
- 跨章 `§N.M` 指针机械对账：标题存在性 0 条落空。语义指向错误另见六视角 MAJOR（02 篇 §4.3/§2.4 的内容不在所指数节）。

## 最终整篇六视角（已运行）

方法：七个 300 行分块抽取（`chunks/chunk-1.md` … `chunk-7.md`，只记接缝证据并带绝对行号），
六个视角各自读全部七份分块 + 六份章节摘要 + 章契约 §五，对准备写入的每条候选用 ≤80 行窗口回读主文档复核；
不整篇读、不读源码、不重审章内正文。报告头均为 `SCOPE: WHOLE-DOC SEAM-ONLY`。

| 视角 | VERDICT | HARD | MAJOR | MINOR | 报告 |
|---|---|---|---|---|---|
| beginner-me | FAIL | 0 | 4 | 6 | `beginner-me.md` |
| g18 | FAIL | 0 | 4 | 9 | `g18.md` |
| editor | FAIL | 0 | 5 | 8 | `editor.md` |
| navigator | FAIL | 0 | 6 | 10 | `navigator.md` |
| reference | FAIL | 0 | 3 | 4 | `reference.md` |
| instructional | FAIL | 0 | 3 | 7 | `instructional.md` |

六视角均确认：无 BLOCKER、无 HARD-REJECTION（都能继续读下去），但存在未解决的 MAJOR 接缝，故 0/6 PASS。

## MAJOR 根因汇总（跨报告合并）

- **R1 镜像位置来源与交付义务冲突**：Ch1 L45/L106–L107「由引导器按加载记录声明」「三者都必须按值交付」
  vs Ch2 L205/L280–L281/L543「不从引导器读、由链接常量重建」「可重建的数据不必跨边界复制」。
  五份报告独立命中（beginner/editor/g18/reference/instructional）。
- **R2「内存账」术语归属错章**：Ch1 L131 术语条用 `kinfo.memmap[]` 定义，违反 Ch1 L86 自定的抽象边界；
  Ch2 L166 又称「本章会首次出现 `struct kinfo`」，且 Ch2 术语表 L579–L587 未收该词。六份报告全部命中。
- **R3 五条生产腿前置知识指错章**：Ch5 L1626、Ch6 L1729「第 1 章的 I1–I4 与五条生产腿」；
  Ch1 正文无定义，定义为 Ch3 L603。六份报告全部命中。
- **R4 换栈归属冲突**：L13 章表把「换栈」列入 Ch4，L4/L84/L548 判给 02 篇，Ch2 L284 已叙述 `head.S` 末段换栈；
  Ch4 正文无「换栈」一词。五份报告命中（beginner/editor/g18/reference/instructional）。
- **R5「02 篇/镜像篇/下一篇」称法与迟定义**：L4 首用「02 篇」，L603 才定义；Ch2 L174/L528/L540/L548
  的「下一篇（镜像篇）」在分章文档里紧邻 Ch3，且与 Ch2 摘要 L54「下一章是设计契约」相抵。
- **R6 02 篇指针簇语义落空**（navigator）：L743/L937/L939/L945/L1081/L2008 的「02 篇 4.3 节」不覆盖借槽链；
  L1073 的 Step 4 DM 在 02 篇 §4.5/§4.6 而非 §4.3；L548/L884 的 `k_initial_stktop` 判据在 02 篇 §2.5 而非 §2.4；
  L910/L2009 的 `alloc_pagetable` 在 02 篇 §2.4 无对应内容。
- **R7 入口计数 4/3/5 未对账**：Ch3 L1043「四个容易混的入口」、L1057「三条对外入口」 vs Ch4 L1181「五个入口」，
  Ch4 无五人名单；instructional 判 MAJOR，reference/editor 判 MINOR。
- **R8 02 篇反向引用节号错**（navigator）：02 篇 1720/1722「以 01 篇 3.7 节为唯一权威」，
  01 的计数换算表在 §3.6；§3.7 是所有权与校验门链。

## 摘要缺陷（摘要门需重跑）

- `ch4-summary.md:26`：A/B/C 防线职责与原文 L1351–L1354 不符（A 是文件字节区间×目标 LMA，B 是目标 LMA 两两不交，
  C 才是模块×内核跨度）。navigator 判 MINOR，但属摘要忠实性缺陷。
- `ch3-summary.md:23`：把直核面的 `RamAuthority::DTB` 写成 `U-Boot/直核腿声明 InputObjectId::DtbBootInput`。

## 结论

- 硬阻断（整篇六视角无法运行）已解除：6/6 独立报告实际产出，全部 `HARD-REJECTIONS: 0`。
- 读者验收未通过：8 个 MAJOR 根因未清，0/6 PASS。
- 因存在 MAJOR，D01 不能宣布读者验收收敛；未启动 `WORKFLOW-META-REVIEW-D01.md`，D02 未启动。
- 修复这些根因会改动已冻结的 Ch1/Ch2/Ch4/Ch5/Ch6 与 02 篇指针，需按章重开流程执行；
  本轮只完成“让最终六视角可运行并产出真实结论”。
