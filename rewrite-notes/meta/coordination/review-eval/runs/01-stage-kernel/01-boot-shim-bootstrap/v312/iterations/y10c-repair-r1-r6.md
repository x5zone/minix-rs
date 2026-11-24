# y10c — 修复 R1–R6

DATE: 2026-11-24
BEFORE: `c53a80075cbb8a74c4889af9b3dcd5309993065ec58d3676a463e8b3fbd6536c`（2019 行）
AFTER: `5023b4543f55a1f37ed2105b4b70d617ebdcd50ce463484e6f1f9c3477abd5d2`（2020 行）

## 范围

只做六视角汇总的 R1–R6；R7（入口计数 4/3/5）与 R8（02 篇反向引用 01 §3.7）未动。

## 修复

- R1：Ch1 §1.1/§1.5 的镜像位置来源改为「链接时的布局结果」，按值交付范围收为内存图与交接结构，补「可重建的数据不必跨边界复制」。
- R2：Ch1 术语「内存账」删 `kinfo.memmap[]`；Ch2 删「首次出现」，术语表补「内存账」。
- R3：Ch5/Ch6 前置概念改为「第 3 章的五条生产腿」。
- R4：Ch4 章表删「换栈」；Ch2 的 `k_initial_stktop` 指针改 02 篇 §2.5。
- R5：正文「下一篇（镜像篇）」全部改「02 篇（镜像篇）」，首用处加 `02-higher-half-kernel.md` 链接。
- R6：02 篇指针簇按目标节校正（§2.2/§2.4/§2.5/§3.6/§4.5）；`alloc_pagetable`、借槽链改指 C 源。

## 同步

- `summaries/ch1-summary.md`、`ch2-summary.md` 更新；ch1 摘要头指向 `ch1-8ec90a1b.md`。
- 修复后章切片快照：Ch1 `8ec90a1b`、Ch2 `5adbbe1f`、Ch3 `848bfdf1`、Ch5 `4a4054b3`、Ch6 `90b309c3`；Ch4 未变（`a1b899d7`）。
- `CHAPTER-FREEZE.md` 加 `REPAIR-STALE` 注记：以下冻结记录只作历史证据。

## 门

- `doc-style-lint.sh`：error 级 0。
- `doc-publication-check.py`：无新增违规。
- `chapter-contract-check.py`：PASS。
- `tutorial-rubric-check.py`：6 章，0 章缺项。
- `doc-citation-check.py`：failed=0，warned=4。
- `doc-ref-check.sh`：内部引用全解析。
- `doc-code-density-check.sh`：承载门阻断项通过。
- `summary-anchor-check.py`：六份摘要全 PASS。
- 独立复核 `REPAIR-VERIFY.md`：R1–R6 全部 CLOSED，VERDICT PASS（R6-NOTE 非阻断）。

## 未做

- 受影响章的章级六视角、摘要 FAITHFUL 门、`chapter-freeze` 事件未重跑。
- 修复后最终整篇六视角未重跑；六份 final 报告仍是修复前证据。
- R7、R8 未修。
