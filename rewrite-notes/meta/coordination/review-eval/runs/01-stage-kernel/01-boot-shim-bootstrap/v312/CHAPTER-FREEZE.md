# CHAPTER-FREEZE — D01 `01-boot-shim-bootstrap`

DOC: 01-boot-shim-bootstrap.md

VERDICT: FROZEN

## Ch1 概述

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 章切片：`tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch1-final-10950802.md`（`tools/extract-chapter-slice.py` 产出，纯章正文）
- sha256：`10950802a1ca1ec4a424c14012aeb34bf43e1a2e762b1224cf7ac2f5ca7e07cc`
- 行数：133
- outline：`rewrite-notes/meta/design/01-stage-kernel/01-outline.v7.md`
- 摘要：`v312/summaries/ch1-summary.md`（74 行，9 条锚点）
- 摘要验证：`v312/summaries/ch1-summary-verification.md` — `VERDICT: FAITHFUL`（2026-11-24）
- 事件：`chapter-review` REV-20261124-B40002E6 / `summary-faithful` REV-20261124-7F878FAC / `chapter-freeze` REV-20261124-0FF649F8
- 冻结日期：2026-11-24

### 六视角

| 视角 | 报告 | 结论 |
|---|---|---|
| beginner-me | `v312/chapters/ch1/beginner-me.md` | READY / HARD 0（c56c） |
| G18 | `v312/chapters/ch1/g18.md` | READY / HARD 0 / P1 无（10950802 终审） |
| editor | `v312/chapters/ch1/editor.md` | READABLE（c56c） |
| navigator | `v312/chapters/ch1/navigator.md` | READABLE（c56c；首读全文 133 行的边界说明，报告证据限允许行） |
| reference | `v312/chapters/ch1/reference.md` | READABLE（c56c） |
| instructional | `v312/chapters/ch1/instructional.md` | READABLE（c56c） |

### 附加

- Reader-B：`v312/chapters/ch1/reader-b.md` — PASS（c56c；outline v7 七节点跨节顺序一致；三处非阻断子序分叉）。
- 理论事实核对：`v312/chapters/ch1/theory-fact.md` — PASS（c56c；六项硬点全 PASS）。
- 版本差：除 G18 外的报告绑定 `c56c946c`。`10950802` 相对 `c56c` 只改 §1.2 布局句、§1.3 I3 措辞、
  §1.5 I4 示例数字与 §1.6 自检句，均为 G18 P1 修复；G18 终审已在这份最终切片上判 READY/HARD 0。
  其余视角的 READABLE/READY 结论与结构、导航、术语、教学骨架相关，不因这些局部措辞变化而改变；
  按“已冻结章继承”处理，不重跑整章。
- 非阻断打磨：导航者 N1（L7/L63 路线漏 §1.3 接缝）、编辑 P1 组、初学者软卡点、参考/教学 P1/P2；
  均为章内可修，不影响 6/6 结论；最终整篇六视角会再核对接缝与摘要↔原文一致。

### 允许下游引用

- 三类数据（可用内存图、镜像位置、交接结构）及其用途。
- I1–I4 概念表述与四项检查；物理区间值 vs 物理指针的区别。
- 发送侧/接收方、声明者/转交者/所有者、交接时序、声明凭据。
- 承重接缝：本篇保证按值/可追溯/有唯一所有者/先校验；镜像篇保证进入高半后仍可访问。

### 禁止修改

- Ch1 正文在本轮后视为只读；下游发现矛盾只定向重开相关单元并重跑对应视角，不整章重读。
- 不得把 Ch1 的概念改写为项目类型名、协议名、字段表或代码块。
- D01 评审门 `CONVERGED` 不因本章 post-CONVERGED 回归而重置；`review-units check` 的 stale 读数是预期。

## Ch4 实现详解

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 章切片：`v312/chapter-slices/ch4-a1b899d7.md`（`tools/extract-chapter-slice.py` 产出，纯章正文）
- sha256：`a1b899d7446c81b58ff75a07e51fa8cd989fdcc5fd3032f7a37564dcf5d6000f`
- 行数：440
- outline：`rewrite-notes/meta/design/01-stage-kernel/01-outline.v7.md`
- 摘要：`v312/summaries/ch4-summary.md`（52 行，锚点机械校验 PASS）
- 摘要验证：`v312/summaries/ch4-summary-verification.md` — `VERDICT: FAITHFUL`（2026-11-24）
- 事件：`chapter-review` REV-20261124-8914CFB2 / `summary-faithful` REV-20261124-0F31CBC8 / `chapter-freeze` REV-20261124-CA3722C3
- 冻结日期：2026-11-24

### 六视角

| 视角 | 报告 | 结论 |
|---|---|---|
| beginner-me | `v312/chapters/ch4/beginner-me.md` | READY / HARD 0（7cbb → 6581 delta 为 fact 修正） |
| G18 | `v312/chapters/ch4/g18-final.md` | READY / HARD 0（6581 终审） |
| editor | `v312/chapters/ch4/editor.md` | READABLE / HARD 0（667ad） |
| navigator | `v312/chapters/ch4/navigator.md` | READABLE / HARD 0（f3f86） |
| reference | `v312/chapters/ch4/reference.md` | READABLE / HARD 0（f3f86） |
| instructional | `v312/chapters/ch4/instructional.md` | READABLE / HARD 0（f3f86） |

### 附加

- Reader-B：`v312/chapters/ch4/reader-b.md` — PASS / HARD 0（f3f86）。
- 代码事实：`v312/chapters/ch4/fact.md` — PASS（6581f2；errors=0 notes=6，均为非阻断措辞/边界）。
- 版本差：六视角报告绑定 9f8016/f3f86/667ad/7cbb；`6581f2` 相对这些版本只改 fact 两处（`tty`/`mib` 互换机制、最小示例预期 16/6 行）与“入口不共享函数、取件共用 `FileLoader`”，不改变可读性结论。G18 终审已直接读 `6581f2`。

### 允许下游引用

- 两条产出入径、九动作/十拍、六套编号、六落地区与门链逐腿执行序。
- `BootReport<'_>` 九字段视图、`KernelInfoView` 深拷、`store_boot_report`/`store_kernel_info` 的落地区。
- `BootAlloc::init`/`resume`、`pt_alloc::register`、arena 生命周期与 `FreeRangeSet` 扣除。
- 失败现场索引：`read_required`、EBS 配对、`boot_validate_and_prepare`/`resume_additive`、建根 halt、`kmain` strict+postconditions。

### 禁止修改

- Ch4 正文在本轮后视为只读；下游发现矛盾只定向重开相关单元并重跑对应视角。
- 不得在 Ch4 重论证第 3 章的设计取舍或重讲第 2 章的 C 行为。
- D01 评审门 `CONVERGED` 不因本章 post-CONVERGED 回归而重置；`review-units check` 的 stale 读数是预期。

## Ch2 C 源码分析

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 章切片：`v312/chapter-slices/ch2-672fc625.md`（`tools/extract-chapter-slice.py` 产出，纯章正文）
- sha256：`672fc625b4170d98bf2d8fe7de62d6680056d6ec7229b3bf13230793f9050428`
- 行数：434
- outline：`rewrite-notes/meta/design/01-stage-kernel/01-outline.v7.md`
- 摘要：`v312/summaries/ch2-summary.md`（54 行，锚点机械校验 PASS）
- 摘要验证：`v312/summaries/ch2-summary-verification.md` — `VERDICT: FAITHFUL`（2026-11-24）
- 事件：`chapter-review` REV-20261124-9C6A872C / `summary-faithful` REV-20261124-506770D0 / `chapter-freeze` REV-20261124-F9EE8846
- 冻结日期：2026-11-24

### 六视角

| 视角 | 报告 | 结论 |
|---|---|---|
| beginner-me | `v312/chapters/ch2/beginner-me.md` | READY / HARD 0（f784；d08ba8→7de182 为 seam/事实修正） |
| G18 | `v312/chapters/ch2/g18-final.md` | READY / HARD 0（47830a 终审；后续为 seam/事实修正） |
| editor | `v312/chapters/ch2/editor.md` | READABLE / HARD 0（397a8c） |
| navigator | `v312/chapters/ch2/navigator.md` | READABLE / HARD 0（397a8c） |
| reference | `v312/chapters/ch2/reference.md` | READABLE / HARD 0（397a8c） |
| instructional | `v312/chapters/ch2/instructional.md` | READABLE / HARD 0（397a8c；TR-2 已修） |

### 附加

- Reader-B：`v312/chapters/ch2/reader-b.md` — PASS / HARD 0（397a8c）。
- 代码事实：`v312/chapters/ch2/fact.md` 的 2 个实质错误已修（`pg_alloc_page` 范围、`board` 删除表/首条提升），最终确认见 `fact-final.md`。
- 版本差：六视角报告绑定 397a8c/f784/47830a；最终 `7de1821f` 相对这些版本的变化集中在：board 删除表语义、I2 无承载者、I3 发送侧/内核两步、I4 版本项无对应物、D1–D5 自足（删除假 §1.4 指针）、三类数据/接收方术语统一。G18 终审已直接读 47830a；后续差异为接缝修正，不改变“可跟读”结论。

### 允许下游引用

- C 的行为链：`boot.cfg` → boot2 → `head.S` → `pre_init` → `get_parameters` → `add_memmap`/`overlaps`/`cut_memmap` → 建表（下一篇）→ `kmain`。
- `kinfo` 五组记录、`param_buf` 格式与删除扫描 off-by-one、两档内存图、`alloc_lowest` 参数错位、`overlaps` 半开区间、三层装载检查。
- D1–D5 约束名与 D1/D2/D4 的本章证据；I1–I4 的 C 侧落点与 I2/版本项的缺口。

### 禁止修改

- Ch2 正文在本轮后视为只读；下游发现矛盾只定向重开相关单元并重跑对应视角。
- 不得把 C 行为改写成 Rust 设计，也不得重讲 Ch1 的概念。
- D01 评审门 `CONVERGED` 不因本章 post-CONVERGED 回归而重置；`review-units check` 的 stale 读数是预期。
