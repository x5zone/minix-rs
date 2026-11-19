# D02 R3 certify 独立回读回执

- 结论：PASS
- 核验条数：19
- CLOSED：19；STILL-BROKEN：0

## 1. §4.4 状态表「程序计数器」行
- 判定：CLOSED
- 证据：`02-higher-half-kernel.md:1075` 现为「aarch64 direct-core：高半 VMA（`ldr` 取绝对值跳）」；旧表述「低地址进入（`ldr` 取高半绝对值跳）」已不在该行。§4.3 的按架构表（`:1036`）也把 aarch64 direct-core 的「跳转完成时 PC」列成高半 VMA。

## 2. §3.1 测试载体分类表述
- 判定：CLOSED
- 证据：`02-higher-half-kernel.md:365` 现为「测试载体分两类，不是一个视图：x86_64/aarch64 测试载体走 inline `arch_boot`、是低地址视图；riscv64 测试载体虽然也直接调 `arch_boot`，但它链接在物理地址上、`direct leg` 加 `delta`，跳转完成时 PC 落在高半 VMA（下表第 5 行）」。该表第 5 行（`:373`）正是 riscv64 测试载体行。

## 3. §3.2「Rust 侧没有这个外部角色」
- 判定：CLOSED
- 证据：`02-higher-half-kernel.md:407` 已改为「Rust 侧不存在 C 那种「内核代码之外、由另一个程序按 `p_paddr` 摆放镜像」的角色——Rust 的 boot-shim 与内核镜像分属两个可执行体，装载发生在 shim 内、由 shim 自己按 `p_paddr` 摆放（§3.1 的表把 shim 记为装载方），所以这些字段由 shim 自己读」。旧句「Rust 侧没有这个外部角色，字段要自己读」已删除；与 §3.1「boot-shim 是装载方」不再正面冲突。首句「不存在 C 那种」仍略拗口，但紧随其后的分句已明确承认 shim 的装载角色。

## 4. §3.4 四条前置条件
- 判定：CLOSED
- 证据：`:565` 现为「下表按顺序对上这四条（第 1、2、3、5 行），第 4 行记的是**返回类型**（`-> !`）——它与四条并列，但**不是前置条件**」。表 `:569-573` 的行序与此一致：第 1 行分页、第 2 行 `kinfo`、第 3 行 `stack_top`、第 4 行「不返回 / 类型 `-> !`」、第 5 行「只调用一次」；先前「最后两行」的误述已消失，第 4 行被明确排除在前置条件之外。

## 5. §1.1 两套地址的限定
- 判定：CLOSED
- 证据：`02-higher-half-kernel.md:22` 现为「同一份镜像因此有两套地址，但**只对带 `AT()` 的高半段成立**……低侧段（`.unpaged_*`，`head.o`/`pre_init.o`/`pg_utils.o` 等）不走 `AT()`，它们的符号值与字节都在 0x00400000 一段（判据见 §2.1 的脚本摘录）」。

## 6. §2.1 `ld` 摘录与 citation-check
- 判定：CLOSED
- 证据：摘录 `:92-94` 已有三行定义：`_kern_phys_base = 0x00400000;`、`_kern_vir_base =  0xF0400000;`、`_kern_offset = (_kern_vir_base - _kern_phys_base);`。运行 `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md --json` 得 `blocks=32 checked=25 failed=0 skipped=4 warned=3`，其中 `location: "90-110"` 为 `PASS`、`kind: "节选"`、`checked_lines: 14`，不是 skip。

## 7. §5.4 M5 行（题称 §5.1）
- 判定：CLOSED
- 证据：`02-higher-half-kernel.md:1162` 的 M5 行覆盖状态已写「**部分覆盖**」，并写明「「arch link.ld 无链接读者」只有一次性的 `grep` 证据（§3.1 自述锁步测试「不证明这些脚本被链接」），没有测试钉住」。

## 8. §6.2 第五组 `$rsp` 按架构的高半范围
- 判定：CLOSED
- 证据：`:1242` 的第五条预期输出现为「x86_64 `0xffff8000…`、aarch64 `0xffff8000…`、riscv64 `0xffffffc0…`」，aarch64 已与 `:1233` 对齐。全局检索 `rg -n '0xffff7fff' rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 只剩 `:1216` 与 `:1233` 两处，均为 L4 平移量 `0xffff7fffbfe00000` 的注解，不再作为栈地址出现。`os/kernel-image/aarch64.ld:38` 的 `KERNEL_VIRT_BASE = 0xFFFF800000000000` 与测试载体 `kern_stack_top: 0xFFFF_8000_0020_0000` 均落在 `0xffff8000…`。

## 9. §6.2 第五组 `$rip` 的 riscv64 delta 说明
- 判定：CLOSED
- 证据：`:1242` 已写明「**riscv64 测试载体不是 `nm` 值本身**：它链接在物理地址上，`$rip` = `nm` 值 + `delta`（`delta = kern_virt_base − kern_phys_base`），例如加载基址 `0x80200000` 时 `nm` 给出 `0x80200160`，而 `$rip` 是 `0xffffffc000200160`」。复算 `python3 -I -c "print(hex(0x80200160 + (0xFFFFFFC000000000 - 0x80000000)))"` 输出 `0xffffffc000200160`，与例子一致（该载体的 `kern_phys_base` 是 `DRAM_BASE = 0x8000_0000`，见 `test-higher-half-riscv64/src/main.rs:63,128`）。命令块注释 `:1232` 现为「riscv64 测试载体是高半 VMA 但等于 `nm` 值加 `delta`，见下一条汇总与 §4.3 的第三种形态」，旧的「`$rip` 与 `nm` 符号值一致」矛盾已消除。

## 10. §4.1 基址反事实
- 判定：CLOSED
- 证据：`:758` 现为「Step 1 固定用 2 MiB，不产生 1 GiB 叶」，与 `os/kernel/src/lib.rs:1234,1238` 的「every write is a fixed 2 MiB leaf」「Step 1 never emits 1 GiB leaves」一致；同句末尾已改为「x86_64 与 riscv64 的 `map_huge` 都会以 `AlreadyMapped` 拒绝 1 GiB 叶降级（见 `os/arch/src/x86_64/paging.rs` 与 `os/arch/src/riscv64/paging.rs`）」。源码核对：`os/arch/src/x86_64/paging.rs:1212-1215` 与 `os/arch/src/riscv64/paging.rs:1174-1180` 都在上层已是 1 GiB 叶、本次写 2 MiB 时返回 `PageTableError::AlreadyMapped`，两架构一致（`riscv64/paging.rs:1176-1180` 原文：`if pte_is_leaf(e2) { ... return Err(PageTableError::AlreadyMapped); }`；`x86_64/paging.rs:1214-1215` 原文：`if e3 & ... != 0 { return Err(PageTableError::AlreadyMapped); }`）。

## 11. §3.4「再算一次对齐这笔账」段末
- 判定：CLOSED
- 证据：`:592` 现为「`push 0` 之后 `and` 不再有作用，`call` 前 RSP 是 8 mod 16；`call` 压返回地址后入口 RSP 变成 0 mod 16，而 System V 要求入口 `(RSP+8) % 16 == 0` 即 RSP ≡ 8 mod 16，正好违反」。按 `os/kernel/src/arch/x86_64/higher_half.rs:889-894` 的指令序列复算（原文序列：`mov rsp, {stktop}` / `and rsp, -16` / `xor rbp, rbp` / `push 0` / `sub rsp, 8` / `call {kmain}`）：`stack_top ≡ 0 (mod 16)`，`push 0` 后 RSP = stack_top − 8（8 mod 16）；只去掉 `sub rsp, 8` 时 `call` 前仍是 8 mod 16，`call` 压返回地址后入口 RSP = stack_top − 16（0 mod 16），`(RSP + 8) % 16 = 8`，与改正后的算式一致。

## 12. §3.5 panic 消息
- 判定：CLOSED
- 证据：`:615` 现为「`arch_boot` 把该错误 panic 成两层包装的消息：外层 `arch_boot: page-size policy: {e}`（`os/kernel/src/lib.rs`），内层 `Display` 是 `kernel mapping policy: {e}`（`os/kernel/src/boot_policy.rs`），完整形如 `arch_boot: page-size policy: kernel mapping policy: no kernel page size satisfies the layout`」。源码核对：`os/kernel/src/lib.rs:946-947` 的 panic 正是 `panic!("arch_boot: page-size policy: {e}")`，`os/kernel/src/boot_policy.rs:187` 的 `Self::Kernel(e)` 正是 `write!(f, "kernel mapping policy: {e}")`，`os/kernel/src/boot_policy.rs:51` 的 `NoKernelPageSize` 文案正是 `no kernel page size satisfies the layout`；外层、内层与完整顺序均与文中一致。

## 13. §4.3 源码注释的表述
- 判定：CLOSED
- 证据：`:997` 的旧句「源码注释把这条记录为『内核从未迁到它的高半 VMA』」已删除，现为「direct leg 的代码仍在 `kern_virt_base` 之下执行，只有跳转目标被抬到高半——即「PC 仍留在低地址」，而不是「内核从未迁到高半 VMA」」。`os/kernel/src/arch/riscv64/higher_half.rs:42-48` 原文是「the kernel enabled its own root and still executes below `kern_virt_base` (identity window), so `la` resolved to kmain's LOW runtime address... delta ... must be added, landing PC at kmain's true high-half VMA」；新表述与其「代码在低半、目标抬到高半」的主干一致。`PC 仍留在低地址` 一句若不区分「跳转前代码 PC」与「跳转后目标 PC」会有歧义，但已不再是旧句的「从未迁到」。

## 14. §3.3 测试模块的导入表述
- 判定：CLOSED
- 证据：`:522` 现为「`uefi_helpers.rs` 与 `opensbi_helpers.rs` 的测试模块都从 crate 根导入 `build_kernel_info`（各自另有 `use super::*` 与其他依赖导入）」，不再声称「只」。源码核对：`os/boot-shim/src/uefi_helpers.rs:1302-1303` 有 `use super::*;` 与 `use crate::build_kernel_info;`；`os/boot-shim/src/opensbi_helpers.rs:669-670` 同样两条。

## 15. §3.5 引文前的脚本注释说明
- 判定：CLOSED
- 证据：`:626` 已在引文前写明「**注意脚本注释与现行实现不符**：选映射粒度的是 `plan_kernel_mapping`（`os/kernel/src/boot_policy.rs`），`boot_validate_and_prepare` 只采集能力事实——下面引文保留原文，按此口径读」。源码 `os/kernel/src/boot_policy.rs:206-213` 的 `plan_boot_pages` 正是先 `plan_kernel_mapping`，再 `plan_dm_policy`。

## 16. §6.2 第四组异常追踪
- 判定：CLOSED
- 证据：`:1220` 现为「关键行：qemu_int.log 里的 check_exception 行给向量（出错类型）；故障地址在紧随其后的异常明细行（x86 看 CR2，arm/riscv 看对应字段）」，已把向量与故障地址分到两行。

## 17. §6.2 第五组寄存器名说明
- 判定：CLOSED
- 证据：`:1223` 现为「下面的 `$rip`/`$rsp` 是 x86_64 寄存器名，aarch64/riscv64 用 `$pc`/`$sp`」。

## 18. §3.1 复算句的两段区分
- 判定：CLOSED
- 证据：`:359` 现为「在**构建输入**上无命中；把检索面扩到全仓的 `*.rs`/`*.toml`/`*.sh`/Makefile 后仍有 6 处命中，全部是注释引用（5 处脚本常量注释 + `os/kernel/tests/link_ld_lockstep.rs` 的一处测试头注释），无构建输入常量」。复算 `git grep -n 'src/arch/.*link\.ld' -- '*.rs' '*.toml' '*.sh' 'Makefile' | wc -l` 输出 `6`；逐行为 `os/arch/src/arch/direct_map.rs:244,296,350` 与 `os/kernel/src/lib.rs:6220,6251` 五处常量注释，加 `os/kernel/tests/link_ld_lockstep.rs:3` 一处测试头注释，与文中口径一致。

## 19. §1.1 第 34 行的待判引用
- 判定：CLOSED
- 证据：`:34` 已写明「本节后文（§2.3 末）用到「写进 `%esp` 的是 0xF04… 一侧的值」时，那是**待 §2.4 判据确认后**的引用，不是本节已证的结论」。§2.3 末 `:236` 也把 `k_initial_stktop` 是否同侧指向 §2.4，前后一致。

结论：回读验证通过，19 条逐条落证。
