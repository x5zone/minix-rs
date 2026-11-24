# y10b — 最终整篇六视角（map-reduce 跑通）

DATE: 2026-11-24
DOC: 01-boot-shim-bootstrap.md
DOC_SHA256: c53a80075cbb8a74c4889af9b3dcd5309993065ec58d3676a463e8b3fbd6536c

## 方法

全篇 2019 行单次读取仍会停滞。改用 map-reduce：

1. 7 个 300 行分块抽取器各写 `final/chunks/chunk-1.md` … `chunk-7.md`，只记跨章接缝证据并带绝对行号；
2. 6 个视角各自读全部 7 份分块 + 6 份章节摘要 + 章契约 §五；
3. 每条准备写入报告的候选用 ≤80 行窗口回读主文档复核；不整篇读、不读源码、不重审章内正文。

## 结果

- 6/6 独立报告实际产出，全部 `SCOPE: WHOLE-DOC SEAM-ONLY`、`HARD-REJECTIONS: 0`；
- 0/6 PASS：六份均 `VERDICT: FAIL`（无 BLOCKER，有未解决 MAJOR）。

| 视角 | VERDICT | HARD | MAJOR | MINOR |
|---|---|---|---|---|
| beginner-me | FAIL | 0 | 4 | 6 |
| g18 | FAIL | 0 | 4 | 9 |
| editor | FAIL | 0 | 5 | 8 |
| navigator | FAIL | 0 | 6 | 10 |
| reference | FAIL | 0 | 3 | 4 |
| instructional | FAIL | 0 | 3 | 7 |

## MAJOR 根因

1. 镜像位置来源/交付义务：Ch1 L45/L106–107 vs Ch2 L205/L280–281/L543。
2. 「内存账」术语归属错章：Ch1 L131 用 `kinfo.memmap[]`；Ch2 L166 称首次出现、术语表未收。
3. 五条生产腿前置知识：Ch5 L1626、Ch6 L1729 写「第 1 章」，定义在 Ch3 L603。
4. 换栈归属：L13 判给 Ch4，L4/L84/L548 判给 02 篇，Ch2 L284 已叙述。
5. 「02 篇/镜像篇/下一篇」称法与迟定义：L4 首用、L603 才定义；Ch2 五处「下一篇」与摘要 L54 相抵。
6. 02 篇指针簇语义落空：L743/L937/L939/L945/L1081/L2008 的 4.3 节、L1073 的 Step 4、
   L548/L884 的 §2.4、L910/L2009 的 `alloc_pagetable`。
7. 入口计数 4/3/5 未对账：Ch3 L1043/L1057 vs Ch4 L1181。
8. 02 篇反向引用 01 §3.7，但计数表在 §3.6。

## 摘要缺陷

- `ch4-summary.md:26`：A/B/C 防线职责与原文 L1351–1354 不符。
- `ch3-summary.md:23`：把直核面的 `RamAuthority::DTB` 写成 `InputObjectId::DtbBootInput`。

## 边界

- 本轮没有改正文、没有重开冻结章；修复 MAJOR 会触碰 Ch1/Ch2/Ch4/Ch5/Ch6 与 02 篇指针，需按章重开流程执行。
- D01 meta-review 与 D02 继续挂起。

## 证据

- `v312/final/MECHANICAL-AUDIT.md`
- `v312/final/beginner-me.md`、`g18.md`、`editor.md`、`navigator.md`、`reference.md`、`instructional.md`
- `v312/final/chunks/chunk-1.md` … `chunk-7.md`
