# D03 follow-ups —— 本轮转出项与去处

| ID | 来源 | 事项 | 去处 |
|---|---|---|---|
| D03-F01 | DESIGN-01 | NMI 配 IST1 改变 C 的嵌套 NMI 语义；代码注释写作 "C gate_table_exceptions parity"，但 C 的 32 位 TSS 无 IST 字段，且无 `[ARCH]` 标注 | 用户裁决：保留 IST1 并标 `[ARCH]`，或改回 C 的嵌套行为 |
| D03-F02 | DESIGN-02 | kernel `MAX_CPUS=32` 与 x86 保护结构 8 个 TSS/GS/AP 栈不一致；9 核机器启动即 panic | code batch：文件 `os/arch/src/x86_64/protection.rs:83`、`os/arch/src/x86_64/trap_stub.rs:661`、`os/kernel/src/smp.rs:1585`；验收 = 编译期关联断言或 9-CPU QEMU 启动不 panic |
| D03-F03 | DESIGN-03 | 门的 DPL 有 `init` 硬编码与 `user_accessible()` 两个真相源，安装后只后者生效 | code batch：单一来源 + 安装后表断言 |
| D03-F04 | DESIGN-04 | `load()` 是安全 `&self` 可重入方法，x86 第二次 `ltr` 会撞 busy TSS 描述符 #GP | 用户裁决：标 `unsafe`/一次性令牌，或加 `debug_assert` + 文档 |
| D03-F05 | DESIGN-05 | 门的 IST 字段与 TSS `ist[]` 槽的对应只对 2/8 恢复，其余向量可能失配 | code batch：统一 IST 分配表 |
| D03-F06 | DESIGN-06 | `kernel_stack_top == 0` 同时表示“无”和“合法零值” | code batch：`Option<VirBytes>` |
| D03-F07 | DESIGN-07/08/15 | `cpu_count` 只写不读；`boot_cpu_id` 与 BSP 选择重叠；多 CPU 共用同一内核栈可表示 | code batch：删只写字段，加 per-CPU 栈断言 |
| D03-F08 | DESIGN-09 | IST 栈是 `u8` 数组，类型对齐 1，交给硬件的 RSP 不保证 16 对齐 | code batch：`#[repr(align(16))]` 包装 |
| D03-F09 | DESIGN-12 | `is_trap` 门类型有 `init` 与 `set_handler` 两个来源 | code batch：单一来源 |
| D03-F10 | DESIGN-13/14/16/17/18 | `Privilege`/`PrivilegeLevel` 无生产读者、`load_ap` 三实现同体、冗余 `unsafe impl`、`program_entry_base` doc 不一致、HigherHalf 统一调用点理由 | code-excellence 批次 |
| D03-F11 | DESIGN-19 | `lgdt→ltr`、GS 搬迁、`load` 次序无宿主/真机断言；x86 `test-protection` 跳过 stub 安装与全局搬迁 | test batch：在 `os/arch/src/x86_64/protection.rs` 加宿主顺序断言，或让 `test-protection` 调 `install_trap_stubs` 并读 TR/GS；验收 = 顺序反排时测试变红 |
| D03-F12a | READER-A 10 条 | A-01..A-08、A-11、A-12：syscall 入口节号错归属、lidt/lgdt 窗口、AP ltr 槽、bss_test、lldt、STAR 槽算术 | 闭环轮；优先 A-06（节号错归属）与 A-07（窗口） |
| D03-F12b | READER-B 15 条 | B-02..B-12、B-14..B-17：§3.2 标题因果、§2.8 归纳引入新论证、riscv64 越界展开、LDT/页表越界、重复叙述 | 教学债批次；优先 B-04/B-05/B-06 |
| D03-F12c | READER-C 15 条 | C-01、C-04..C-17：`prot_init_done` 降级、控制器状态、Phase A 映射、§1.2 每 CPU 槽、GS 区、负向缺口、TSS 双调用 | 闭环轮；优先 C-01（已装载 vs 已初始化）与 C-08（Phase A 映射） |
| D03-F13 | REFUTE CLAIM 01/05 | LDT 描述符注释过期、DS 访问字节少 ACCESSED 位；`x86_syscall_entry` 写 `gs:[0x8]` 而 §4.5 说 GsArea 入口只读 | code/doc batch：改注释与 §4.5 |
| D03-F14 | REFUTE CLAIM 05/06 | APIC 模式门数 90/92 与 41 的对照、定时器/调度向量号与 C 相反、aarch64 载体注释仍称写 SP_EL1、test-protection 自述矛盾 | 闭环轮逐条核对 |
| D03-F15 | 复验 | `cargo test -p minix-arch` 测试模块编译失败（沿用 D01-F01） | test-audit / code-excellence 批次 |
| D03-F16 | FACT-12 | 文档 §5.1 的 375 个测试是静态属性计数；`cargo test -p minix-arch --no-run` 当前有 45 个错误（`KernelInfo` 缺 `memmap_sources`/`startup_objects`），375 不能算“能跑” | 与 D03-F15 合并，先修初始化器与 import，再用 `--list` 重算 |
