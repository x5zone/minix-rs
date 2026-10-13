# DOC-ITER-LEDGER — 文档迭代工作流状态台账（入库的最小可恢复记录）

> 规格：`prompt/DOC-ITER-WORKFLOW.md`（本台账是 §五·5.1 定义的最小记录；完整轮次包与过程
> 产物在 `tmp/doc-iter/`，不入库）。
> 恢复现场的方法：读本表定位目标文档、基线/结果 commit、workflow 版本与包路径，再取包文件。

## 轮次账

| 篇 | 轮 | 基线 commit | 结果 commit | workflow | 审阅状态 | 轮次包（tmp/） |
|---|---|---|---|---|---|---|
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 01 | `7468a9e39` | `227028e23` | v0.1 | 已审（GPT 反馈已回收，见 Round 02 §1.x） | `tmp/doc-iter/01-boot-shim-bootstrap/round-01.md` |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 02 | `227028e23` | `5d0f99e69` | v0.2 | 待送审 | `tmp/doc-iter/01-boot-shim-bootstrap/round-02.md` |

## 规则变更裁决记录

| 日期 | 版本 | 变更 | 来源 | 状态 |
|---|---|---|---|---|
| 2026-10-13 | v0.1 | 建立（见 v0.1 变更记录） | 用户裁决 + GPT 对话 | 已生效 |
| 2026-10-13 | v0.2 | S1 证据盘点 / S4 盲测判分三分 / E7 三级化 + C1 澄清 / §八 归因记录 / §九 三道理门 / F5 / 包与台账 | Round 01 实证 + 外部评审（用户裁决） | 已生效 |
| — | — | 候选规则 ③（行号引用检查）、④（对账表集合守恒） | Round 01 | 登记待工具排期 |
| — | — | 入口 A 完整重做回流 | v0.2 裁决 | 待入口 B 在 2~3 篇不同形态文档上稳定 |

## 跨范围待办（不属于文档迭代的提交，登记防丢）

- `os/kernel/src/lib.rs` Step 1 注释的 `sstatus.SUM` 说法与 RISC-V 规范相反（规范：
  `Irrespective of SUM, the supervisor may not execute code on pages with U=1`；SUM 只管 load/store）。
  01 篇文档已按规范陈述并在 §4.6 加注；**源码注释本身待另开一笔修正**（Round 01 discovery）。
- `os/boot-shim/src/uefi_helpers.rs` 的两条注释互相冲突：D-64②（快照顺序）说模块占页经
  `allocate_pages`、kernel 段也在"全部 LOADER_DATA 分配"之列；F8 更正（真机 d1a）说 kernel 段与
  boot 模块段"直写 conventional RAM（不经 UEFI 分配）"。代码事实：kernel 段直写（`load_segments_into_phys_memory`），
  模块段经 `alloc_module_pages`（`allocate_pages(LOADER_DATA)`）。文档侧已按代码事实修正（Round 02），
  **源码注释的措辞待另开一笔统一**。
- `os/boot-shim/src/uefi_helpers.rs:238-239` 的注释引用 "01-boot-shim-bootstrap.md §3.5.1"，
  文档无该小节（内容在 §3.5）；源码指针待改（Round 02 V3-lite discovery）。
