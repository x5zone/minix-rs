# DOC-ITER-LEDGER — 文档迭代工作流状态台账（入库的最小可恢复记录）

> 规格：`prompt/DOC-ITER-WORKFLOW.md`（本台账是 §五·5.1 定义的最小记录；完整轮次包与过程
> 产物在 `tmp/doc-iter/`，不入库）。
> 恢复现场的方法：读本表定位目标文档、基线/结果 commit、workflow 版本与包路径，再取包文件。

## 轮次账

| 篇 | 轮 | 基线 commit | 结果 commit | workflow | 审阅状态 | 轮次包（tmp/） |
|---|---|---|---|---|---|---|
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 01 | `7468a9e39` | `227028e23` | v0.1 | 已审（GPT 反馈已回收，见 Round 02 §1.x） | `tmp/doc-iter/01-boot-shim-bootstrap/round-01.md` |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 02 | `227028e23` | `5d0f99e69` | v0.2 | 已审（反馈回收进 Round 03） | `tmp/doc-iter/01-boot-shim-bootstrap/round-02.md` |
| `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` | 03 | `5d0f99e69` | `f7a50edc5` | v0.3 | 待送审 | `tmp/doc-iter/01-boot-shim-bootstrap/round-03.md` |

## 规则变更裁决记录

| 日期 | 版本 | 变更 | 来源 | 状态 |
|---|---|---|---|---|
| 2026-10-13 | v0.1 | 建立（见 v0.1 变更记录） | 用户裁决 + GPT 对话 | 已生效 |
| 2026-10-13 | v0.2 | S1 证据盘点 / S4 盲测判分三分 / E7 三级化 + C1 澄清 / §八 归因记录 / §九 三道理门 / F5 / 包与台账 | Round 01 实证 + 外部评审（用户裁决） | 已生效 |
| 2026-10-14 | v0.3 | S1.5 关键因果主张 / S1.6 运行阶段边界 / S2.1-6 职责漂移+机械对账 / V5 分阶段冷读 / §八 四类归因 / S5 发布扫描+台账重放 | Round 02 实证 + 外部评审（用户裁决"继续"） | 已生效 |
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
- **切片读坏的机制归因未定案**：`os/kernel/src/globals.rs`（`.bss` 落地区注释）与 `os/kernel/src/lib.rs`
  的相关注释把"切片源页读成零"解释为"内核自己的页表不重放这些堆页的 1:1 映射"；但真机读数
  "15 条 conventional 在建立 Direct Map 覆盖时（切表之后）还完好、到 classify 才全零"与该解释相抵，
  更像"页被当空闲复用后清零"。01 篇文档已改为按读数陈述、不选边；**源码注释待取证定案后统一**
  （Round 03 V3-lite discovery）。
- `os/boot-shim/src/main.rs` 的预算注释称 1024 页池"costs every boot 4 MiB of VM-selectable memory"，
  而 `os/kernel/src/vm_handoff.rs:160-168` 明写 UEFI 路径下 bump 区不在 conventional 列表、切割是
  no-op；两处注释冲突，待统一（Round 03 V3-lite discovery）。
