# 跨模型复核 · round38

目标：D01 R38 的六条断言（断言包 `tmp/d01-r38/crosscheck/brief.md`）。
模型：`opencode/hy4-flash-free`；`--agent plan` 只读；原始输出 `tmp/d01-r38/crosscheck/out-hy4.log`。
范围：只核对 brief 里的六条，不是对 D01 的第二次全篇评审。

## FINDING XMODEL-C4（P2）

- 位置：§4.8「交接后的 Direct Map（概念入口）」。
- 严重度：P2。
- 问题：§4.8 的启动链写成 `store_kernel_info → establish_boot_dm → jump_to_kmain`，省略了 `arch_boot_impl` 内的 `build_bootstrap_root_and_enable`。读者无法得知“当前活动的 bootstrap 根”是从哪来的。
- 影响：概念入口把根构建步骤隐去，读者可能误以为 `establish_boot_dm` 负责建根，或以为它直接跟在 `store_kernel_info` 之后。
- 证据：`os/kernel/src/lib.rs:1035-1047` 先 `build_bootstrap_root_and_enable`、再 `establish_boot_dm`；`os/kernel/src/dm_coverage.rs:179-202` 的 `establish_boot_dm` 只接收并复用 `root`，不创建或替换根页表。跨模型原始输出见 `tmp/d01-r38/crosscheck/out-hy4.log` 的 C4 行。
- 最小修复：§4.8 的箭头链补 `arch_boot_impl` 与 `build_bootstrap_root_and_enable`，说明根先建立并启用，再由 `establish_boot_dm` 追加映射。
- 处置：已修；anchor 89/89、citation 21/0、structure/ref/publication/tutorial 全绿。
- 置信度：高。
- 是否需源码核实：已核实。
- 若我错了：若 D01 的 §4.4/§4.5 已在同一篇内完整给出根构建顺序，且 §4.8 明确声明只是“调用位置”而非“完整顺序”，则可降为 SUGGESTION；当前 §4.8 的箭头链没有任何“省略中间步骤”的声明。
- 对应判据：文档时序断言与源码调用链一致；概念入口必须让读者能回答“当前活动的根从哪来”。
