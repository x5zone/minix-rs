# 01-todo.md — 01-boot-shim-bootstrap 待办登记

> 来源：第 3 步 R1 正确性 review 的延后项，与第 4 步 R2 卓越性 review 的预登记项。
> 状态词按 `rewrite-notes/coordination/TODO-LEDGER-INDEX.md` §3 六值词表。

## 一、设计快照落后于代码（P0-design-deviation → design Refactor v2 已产出）

> **状态：已处理（检查点 B1 用户裁决"先补 design 快照 v2"，2026-10-08）**。已生成 `.design/01-design.v2.md`、`01-outline.v2.md`、`01-outline-review.v2.md`，把三处 v1 与代码的偏离对齐（以 C 外部行为为准）。以下保留为 v2 已登记的**开放裁决项（OQ）**，需用户拍板。

`.design/01-design.v1.md`（2026-07-16，DESIGN-INDEX 曾标 ✅ PASS）早于代码演进，以下三处快照与 `os/` 现状不符，已在 v2 更正：

- **OQ-01-C `param_buf` 去留**：v2 已按现状写"保留、当前传 `&[]`"。仍需裁决：何时落地 UEFI 启动选项解析并填充，还是删除该字段。
- **`reserved_regions` 未入设计**：v2 已补录（§3.1.1，NK4-A fix27）。
- **交接形态**：v2 已新增方案 A `BootHandoff`（§3.7/§4.4）及其根因。

## 二、测试真实缺口（P1-test-missing）

R1 已纠正文档 §5 的两处描述错误（虚构的 BootShim mock 单测、被误判缺失的端到端门——后者实由 `os/qemu-tests/test-kernels/kernel/bootstrap/` 三架构引导内核覆盖）。基线实测：`cargo test -p minix-boot` 17/17、`-p boot-shim` 14/14 全绿（详见 `.review/.../test-audit.md`）。以下四处缺口：

- **`free_upper_idx` 生产者无定论**：boot-shim 现传 `None`（`kernel_info.rs:70` 注释），由内核用前校验。"根级第一个空闲索引最终由谁计算"既无测试也无明确设计归属。→ 补生产者测试或在 design 明确"由内核侧计算、shim 不填"。
- **caller-saved 寄存器交接陷阱无自动拦截**：`os/kernel/src/lib.rs:arch_boot` 注释记录了"引用跨 `noreturn` 内联汇编经 caller-saved 寄存器（x86-64 RDI / aarch64 x0）交接被覆写"的真实事故，现靠"入口即拷进 `.bss` 全局"的约定守。→ 需要一个能主动捕获该形态的检查（静态 lint 或运行期断言），非本文档职责，登记待建。
- **T-2 方案 A `BootHandoff` 无宿主单元断言**（第 5 步 test-audit 新增）：blob 写入与 `bump_next` 游标推进不重发，现仅靠 QEMU 真机端到端覆盖 → 建议补 host 构造测试。
- **T-3 opensbi feature 门控未进宿主默认覆盖**（第 5 步 test-audit 新增）：`test_build_kernel_info_uses_riscv64_user_sp` 等需 `opensbi` feature 才编译 → 建议 CI 增补 `cargo test -p boot-shim --features opensbi`，或文档标注覆盖依赖真机门。

## 三、R2 卓越性处置结果（检查点 B2 已确认，2026-10-08）

R2（`style-fix` + `code-excellence`）已完成，产物 `.review/qoder/01-stage-kernel/01-boot-shim-bootstrap/excellence.md`。结果：

- **B2-1 `bootstrap_start/len` 哨兵（OQ-01-A）**：用户裁决 **保留现状、不删**；已作为 `adjudicated`（临时）索引行移交 `coordination/TODO-LEDGER-OPEN.md` §1，终态待独立 ELF 自引导（TODO-02-3）对 unpaged 段的裁决。
- **B2-2 BootPrepareResult/BootHandoff 不合并**：判定为语义分层不同（trait 输出 vs 跨镜像 ABI），非冷/非冗余，无需改。
- **B2-3 param_buf newtype 化**：收益边际，不做。
- **B2-4 design v2 outline 节序**：已将 `01-outline.v2.md` Ch3/Ch4 小节序对齐交付文档（仅改中间产物）。
- **B2-5 文档 Ch1–Ch5 文风/章节/深度**：审计通过，无改动。

R2 未发现新正确性 bug（无退回 full-review），未产生任何交付正文/代码行为改动。
