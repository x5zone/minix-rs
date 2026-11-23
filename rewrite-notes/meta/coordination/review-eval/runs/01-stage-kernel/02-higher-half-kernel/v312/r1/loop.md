# D02 R1 loop（V3.12，full，ROUND-OPEN）

schema: V3.12 from R1
edition: V3.12
轮型: full
文档: rewrite-notes/01-stage-kernel/02-higher-half-kernel.md
基线 sha256: 95e32868e74b730663b9f436805546ecce8fa14e9ca1a314da4faea8cb25736a
修复后 sha256: a55c81c4d8c9fcce5da4c597606bf64537baa072477a8b203708d7a1fb8c99d9
行数: 2,120
单元: 41（6 章 + 35 节），0 冻结
run 目录: rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/02-higher-half-kernel/v312/r1
学习范围: `meta/review/scope/01-stage-kernel/02-higher-half-kernel.yaml`（me-agent 代确认，待作者追认）
历史: V3.11 账户已归档到 `state/.../editions/V3.11.yaml`；V3.12 从 R0 重建

### §0 基线

- 迁移：`new-edition V3.12 --unfreeze-all`，41 单元、round=0；R1 baseline 覆盖 41 单元。
- L0.9：TR-0 五章补齐；anchor 65/65；publication/rubric/citation/density 全绿。
- 输入 doc sha `95e32868…`；R1 修复 P0 后 `a55c81c4…`。

### §1 面板

- 6 角色 fresh：P-FACT `REV-20261123-150D52CE`、READER-A `C3636ED7`、READER-B `ABEE8E01`、
  READER-C `20EBA638`、DESIGN-D2 `2FF3D49A`、P-REFUTE `ECB380F6`；均 completed-pass。
- 面板 FINDING 53：P0 5 / P1 8 / P2 33 / P3 7；P-REFUTE 6 CLAIM（2 条 P1 级反证）。

### §2 发现

- P0：FACT-01（发送侧 `KernelInfo` 文本）、FACT-02（`[ARCH]` 三处一致）、
  DESIGN-01（`kmain` 指针别名）、DESIGN-02（路径图发送侧）、DESIGN-03（`KERNEL_ENTRY_ANCHOR` 签名）。
- P1：entry_point 文档、jump-tables 构建前提、J3/J4 归属、表行号错指、计数冲突等。
- 其余集中在术语首用、章节边界、指针错指、过程词/决策编号、代码节选与源码不符。

### §3 归因

- 主因：D02 写成于 DESIGN-02(b)/R3 之前，发送侧形状与 `kmain` 实现已变；
  V3.11 的机械 CONVERGED 未覆盖教学结构与跨节一致性。
- 另有两个真实代码问题：`kmain` 物理/高半别名断言（已修）、`[ARCH]` 代码侧缺标（已补）。

### §4 修复

- P0 全部 ①：
  - FACT-01/DESIGN-02/DESIGN-03：发送侧与测试载体统一 `BootReport<'_>`；
    `KERNEL_ENTRY_ANCHOR` 改三参签名。
  - FACT-02：`[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]` 补到
    `os/arch/src/lib.rs`、`os/kernel/src/lib.rs`、`02-design.v4.md`。
  - DESIGN-01：`kmain` 断言接受高半/物理两种别名（commit `7ff12bca4`），
    `test-smp-aps-aarch64` 越过 kmain 后停在已登记 A2 用户链异常；x86/riscv 门 PASS。
- P1/P2/P3 48 项仍 `OPEN`；本轮不写 `ROUND-CLOSED`。

### §5 复验

- 文档门：anchor 65/65、code-map 30/30、ref/style/pub/citation/density 全绿。
- QEMU：x86_64 UEFI、riscv64 U-Boot shim PASS；aarch64 direct 越过 kmain，后续 A2 遗留 panic。
- 宿主：minix-elf 27/0、boot-shim loader 32/136；`check-layout.sh all` PASS。

### §6 指标

见 `metrics.md`：53 FINDING、6 CLAIM、P0 5 全 ①、P1/P2/P3 48 OPEN。

### §7 状态

`ROUND-OPEN`。下一步：修 P1/P2/P3 48 项，独立复验后 R1 才能关账；不开 R2。

### §8 给 meta-review 的不确定点

- `kmain` 别名断言的 `arg±delta` 判据接受两种视图；是否需要改为跳转前归一化，留给代码复核。
- P-REFUTE 指出的 `-C jump-tables=no` 构建前提未写入正文，属 P1，待修。
