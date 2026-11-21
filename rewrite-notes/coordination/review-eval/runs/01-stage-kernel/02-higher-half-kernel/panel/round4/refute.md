# D02 R4 · 独立 P-REFUTE

对象：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
方法：只从文档、设计合同、源码/工件取证；每个反例落到源码位置、命令输出或文档原句。
结论先行：文档的三条主干机制主张（双地址布局、`k_initial_stktop` 高半落点、成对跳转契约）经反证未倒；倒掉的是 **ELF 解析器的错误处理/测试清单**与 **v1 交接载荷分区**两处事实层主张，其中前者与文档 §4.2 自相矛盾，后者与 wire v10 的权威定义矛盾。

---

## CLAIM 01

- 主张：C 侧 `kernel.lds` 是双地址布局：五个带 `AT()` 的输出段满足 `LMA = VMA − _kern_offset`，低侧 `.unpaged_text/.unpaged_data/.unpaged_bss` 不带 `AT()`、`VMA = LMA`；Rust 生产脚本 `os/kernel-image/*.ld` 保持“所有 `PT_LOAD` 的 `vaddr − paddr` 同一平移量”这条不变量，且 `os/kernel/src/arch/*/link.ld` 无 `AT()`、无生产链接读者。文档 §1.1、§2.1、§3.1、§4.1 的主干。

- 反证尝试：
  1. 找“同一镜像里某段平移量不同”的反例。三份生产工件的 `readelf -lW` 全部逐行相减：x86_64 恒为 `0xffff800000000000`，aarch64 恒为 `0xffff7fffbfe00000`，riscv64 恒为 `0xffffffbf7fe00000`；`check-layout.sh` 的 L4 也逐段判等并 PASS。没有找到反例。
  2. 找“`AT(ADDR(sec) − _kern_offset)` 实际等于 `AT(_kern_phys_base)`”的反例。两者相等要求该段 VMA 恰为 `0xF0400000`。`i386/kernel.lds` 在低侧三段之后才执行 `. += _kern_offset`（第 22 行），随后还有两个非空的 `.usermapped*` 输出段（第 24–28 行），所以 `.text` 的 VMA 严格大于 `0xF0400000`，`AT(ADDR(.text) − _kern_offset)` 严格大于 `0x00400000`。文档 §2.1 的逐段结论成立。
  3. 找“`os/kernel/src/arch/*/link.ld` 被某个构建输入消费”的反例。`grep -rn 'src/arch/.*link\.ld' --include='*.rs' --include='*.toml' --include='*.sh' --include='Makefile*' --exclude-dir=target os` 共 6 行命中，全部是注释（`os/kernel/tests/link_ld_lockstep.rs:3`、`os/arch/src/arch/direct_map.rs:244/296/350`、`os/kernel/src/lib.rs:6329/6360`），没有链接参数或构建常量。文档 §3.1 的“无生产链接读者”成立。
  4. 找“生产脚本没有 `AT()`”的反例。`grep -c ': AT(' os/kernel-image/*.ld` 实测 `aarch64.ld:4`、`x86_64.ld:4`、`riscv64.ld:4`；arch 三份均为 0。与文档 §6.2 的口径一致。
  5. 核对“树内 boot monitor 不读 `p_paddr`”的射程声明。`minix3/sys/lib/libsa/loadfile_elf32.c:326,336` 用 `LOADADDR(phdr[i].p_vaddr)` 与 `READ(fd, phdr[i].p_vaddr, ...)`；`minix3/sys/arch/x86/include/loadfile_machdep.h:38-44` 的 `_STANDALONE` 分支把地址截到低 27 位。文档 §1.2 把 `p_paddr` 射程限定为外部 multiboot 装载器，成立。

- 结果：反证失败（主张成立）。未发现相反执行路径、失败路径或架构分支能推翻“逐段平移一致 + 有无读者决定 `AT()` 形态”这条主干。
- 证据：
  - `minix3/minix/kernel/arch/i386/kernel.lds:4-6,14-35`（三常量、低侧三段、`. += _kern_offset`、五个 `AT()`）。
  - `os/kernel-image/x86_64.ld:26-33,58,66,74`；`aarch64.ld:38-45,57`；`riscv64.ld:41-48,61,71,80`。
  - `os/kernel-image/check-layout.sh:193-203`（L4 逐段判等）。
  - 实跑：`SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all`，三架构 `[L4] PASS`，且 `CHECK-LAYOUT=PASS（all）`。
  - 工件：`readelf -lW os/target/{x86_64-unknown-none,aarch64-unknown-none,riscv64gc-unknown-none-elf}/release/kernel` 的 `LOAD` 行（x86_64 5 行，aarch64/riscv64 各 4 行；首段 LMA 分别为 `0x200000`、`0x40200000`、`0x80200000`）。
  - `grep -rn 'src/arch/.*link\.ld' ... --exclude-dir=target os` 的 6 处注释命中（命令与输出见上）。
- 若我错了：能推翻它的证据是：某份生产链接脚本出现逐段不同的 `vaddr − paddr`（L4 会红），或某个树外/树内构建步骤把 `os/kernel/src/arch/*/link.ld` 当作链接输入（现有 grep 面覆盖 `*.rs/*.toml/*.sh/Makefile*`，但若存在未被这些模式覆盖的构建入口，则“无生产读者”不成立），或 C 侧某个 `AT()` 表达式的减数不是同一个 `_kern_offset`。这三种情况我都没有找到。

---

## CLAIM 02

- 主张：i386 的 `k_initial_stktop` 落在高半一侧。判据不是符号名里的 `__k_unpaged_` 字样，而是：`mpx.o` 不在 `objcopy --prefix-symbols` 的改名名单里，且它的 `.bss` 只可能被 `. += _kern_offset` 之后的通用 `.bss` 收走。文档 §2.2、§2.4 的主干（§2.5 复述）。

- 反证尝试：
  1. 找第二个定义/别名。`grep -rn 'k_initial_stktop' minix3/minix/kernel/arch/i386` 只命中 `head.S:84` 的引用与 `mpx.S:634` 的 `LABEL(__k_unpaged_k_initial_stktop)` 定义；没有第二个定义。
  2. 找“`mpx.o` 被改名”的路径。`minix3/minix/kernel/arch/i386/Makefile.inc:51-54` 的 `.for unpaged_obj in ...` 名单里没有 `mpx.o`；`mpx.S` 只在第 75 行的普通 `SRCS+=` 清单里。名单里不存在 `unpaged_mpx.o` 规则；`mpx.S` 的产物就是 `mpx.o`。
  3. 找“`mpx.o` 的 `.bss` 被低侧段收走”的路径。`kernel.lds:19` 的 `.unpaged_bss` 模式是 `unpaged_*.o(.bss COMMON)`，按输入文件名匹配；`mpx.o` 不匹配。通用 `.bss` 在 `kernel.lds:32`，位于 `kernel.lds:22` 的 `. += _kern_offset` 之后。
  4. 找“符号名是判据”的反例。定义处手写 `__k_unpaged_k_initial_stktop`（`mpx.S:634`），引用处 `head.S:84` 的 `k_initial_stktop` 被 `objcopy` 改名后与之同名；`LABEL(y)` 在 `minix3/sys/arch/i386/include/asm.h:177` 展开为 `_LABEL(_C_LABEL(y))`，`_C_LABEL` 在 `__ELF__` 下恒等（`:64-65`），`_LABEL(x)` 为 `.globl x; x:`（`:98-99`）。名字两半都闭合，但名字本身不决定段归属；文档的“名字不是判据”成立。
  5. 顺带核对 ARM 对照。`minix3/minix/kernel/arch/earm/mpx.S:265-272` 的 `k_initial_stack` 在 `.data`，`earm/Makefile.inc:61-73` 的改名名单同样没有 `mpx.o`，`earm/kernel.lds:17,33` 的 `.data` 带 `AT()`。因此 ARM 侧按同一判据也应落高半。文档 §2.5 把它写成“改名名单与 `.data` 收段证据待补”，是**文档覆盖度上的保守**，不是反例；现有 C 源已经能把这条证据补上。

- 结果：反证失败（主张成立）。i386 的落点结论有完整依据链；ARM 侧结论也可由现有源码复现，文档的“待补”是未取证的空白而非错误主张。附一条非阻断观察：§2.5 的“ARM 待补”可在不新增源码的前提下用 `earm/mpx.S:265-272` + `earm/Makefile.inc:61-73` + `earm/kernel.lds:17,33` 直接闭合。
- 证据：
  - `minix3/minix/kernel/arch/i386/Makefile.inc:51-54,66-70,75`。
  - `minix3/minix/kernel/arch/i386/kernel.lds:17-22,32-35`。
  - `minix3/minix/kernel/arch/i386/mpx.S:628-649`（`.data` 魔数、`.bss` 的 `k_initial_stack`/`k_boot_stack`/`k_stacks_*`）。
  - `minix3/minix/kernel/arch/i386/head.S:84` 与 `minix3/sys/arch/i386/include/asm.h:64-65,98-99,177`。
  - `minix3/minix/kernel/arch/earm/mpx.S:265-272`；`earm/Makefile.inc:61-73`；`earm/kernel.lds:17,33`。
- 若我错了：能推翻它的证据是：BSD make 的隐式规则把 `mpx.S` 也生成了一个匹配 `unpaged_*.o` 的输入名；或链接器把 `mpx.o` 的 `.bss` 作为孤儿节放进 `.unpaged_bss`；或 `mpx.o` 被另一个未检索到的改名规则处理。这三种情况在 `Makefile.inc` 与 `kernel.lds` 里都没有路径。

---

## CLAIM 03

- 主张：换栈与跳转必须成对，Rust 用 `HigherHalf` trait 把四条前置条件与 `-> !` 收进单一契约；三份实现分别用 x86_64 `call` 相对位移、aarch64 `ldr =kmain` + `br` 绝对值、riscv64 `la` + `auipc/bltu` 条件 `delta`；x86_64 的 `sub rsp, 8` 是 psABI 必需垫片，aarch64 的 `isb` 是防御性屏障，riscv64 的 `delta` 只在 direct leg 加。文档 §2.3、§3.4、§4.3 的主干。

- 反证尝试：
  1. 找“x86_64 相对 `call` 在不满足调用点=链接地址的形态下仍被使用”的反例。生产 x86_64 只有 v1 高半视图（`jump_kernel_image_v1` 跳到 `report.kern_virt_base`，`enter_kernel_image_v1` 的 `entry` 取自 `report.kern_virt_base`）；x86_64 测试载体自己就是低地址链接的 PE，`call` 目标与调用点同侧；`rust_image_main` 的 x86_64 分支在 `arena_pa == 0` 时直接 `halt`，没有低执行/高链接的内联路径。没有找到反例。
  2. 找“去掉 `sub rsp, 8` 仍满足 System V”的反例。按指令序列：`mov rsp, stack_top`（16 对齐）→ `and rsp,-16`（无操作）→ `push 0`（RSP ≡ 8）→ 若去掉 `sub`，`call` 前 RSP ≡ 8，`call` 压 8 字节后入口 RSP ≡ 0；System V 要求入口 `(RSP+8) % 16 == 0`，即 RSP ≡ 8。反例成立：只去掉 `sub rsp, 8` 确实会违约。文档 §3.4 的这一条成立。
  3. 找“生产 `kmain` 是裸函数、psABI 不适用”的反例。`os/kernel/src/lib.rs:1806-1807` 的 `kmain` 在 `cfg(all(not(mock), not(qemu_test)))` 下是普通 `pub fn`；裸函数在 `:2206-2208` 的 `cfg(feature = "qemu_test")`。反汇编 `os/target/x86_64-unknown-none/release/kernel` 的 `kmain`（`ffff80000020be00`）首部是 `push r15/r14/r12/rbx` 与栈探测，没有 `push rbp`。文档 §4.3 的“生产 kmain 是普通 Rust 函数、序言不保存 rbp”成立。
  4. 找“aarch64 `isb` 是正确性前提”的反例。`os/arch/src/arm64/paging.rs:720-735` 的 `enable()` 在 TTBR 写入后执行 `dsb sy; isb`，在 SCTLR 写入后执行 `isb`；DM 的 PTE 写入路径 `write_pte_dm`（`:312-325`）在 KernelDm 通道上执行 `dsb ishst; tlbi vae1is; dsb ish; isb`。`establish_boot_dm` 之后到 `br` 之间不再写 TTBR/SCTLR。因此删掉 `jump_to_kmain` 的 `isb` 不构成“按旧映射取指”的失败路径。文档 §4.3 其二成立。
  5. 找“riscv64 `delta` 条件判断会误判”的反例。`auipc t3, 0` 取当前 PC；`bltu t3, t4` 与 `kvirt = kern_virt_base` 无符号比较。direct leg 的 PC 在 `0x8020…`，小于 `0xffffffc0…`；resume leg 的 PC 已在高半，不小于。`la t1, {kmain}` 在 `-mcmodel=medany` 下是 `auipc+addi`；direct leg 得低物理地址，加 `delta` 后正好是 `kern_virt_base + (PA − kern_phys_base)`；resume leg 得高 VMA，不再加。源码注释的故障算式也已复算：`0xffffffc00000477a + 0xffffffbf7fe00000 = 0xffffff7f7fe0477a`（64 位截断），与注释的故障地址一致；注释里多一个 `f` 的加数 `0xffffffbf7ffe0000` 则得 `0xffffff7f7ffe477a`，文档 §4.3 对该笔误的判定成立。
  6. 找“trait 的两个生产调用点没有传 `kinfo.kern_stack_top()`”的反例。`os/kernel/src/lib.rs:232` 与 `:370` 是仅有的两处 `H::jump_to_kmain` 调用，均传 `kinfo.kern_stack_top()`。文档 §3.4 的“可表示冗余值”成立。

- 结果：反证失败（主张成立）。三份 asm 的指令序列、对齐账、屏障归属与 `delta` 条件性都与源码一致；没有找到能推翻“成对跳转 + trait 契约”的反例。
- 证据：
  - `os/kernel/src/boot/higher_half.rs:31-48`（四条 Safety 与 `-> !`）。
  - `os/kernel/src/arch/x86_64/higher_half.rs:44-55`；`aarch64/higher_half.rs:40-54`；`riscv64/higher_half.rs:92-120`。
  - `os/kernel/src/lib.rs:1806-1807`（生产 `kmain`）、`:2206-2239`（`qemu_test` 裸函数）、`:2283-2294`（`sp_ok`/`sp_aligned`/`fp_zero`）。
  - `os/arch/src/arm64/paging.rs:720-735`、`:312-325`（TTBR/SCTLR 与 PTE 写入屏障）。
  - 反汇编：`objdump -d --start-address=0xffff80000020be00 --stop-address=0xffff80000020be60 os/target/x86_64-unknown-none/release/kernel`，首部无 `push %rbp`。
- 若我错了：能推翻它的证据是：生产 `kmain` 被改成裸函数（psABI 论证失效）；x86_64 出现低地址执行高半链接镜像的新腿（相对 `call` 会落到低侧）；aarch64 `enable()`/`write_pte_dm` 的屏障被移除（`isb` 变成正确性前提）；riscv64 的 `la` 改成绝对取址或 `bltu` 比较改成有符号（`delta` 条件判断失效）。这些在当前源码里都不存在。

---

## CLAIM 04

- 主张：`kmain` 的栈顶由消费点按 `kern_virt_base + kern_size` 现算（`KernelInfo::kern_stack_top()`），`kern_size` 来自 `compute_kernel_layout` 的 `max(paddr+memsz) − min(paddr)`；入口 `_start` 另用 `.bss` 里的 64 KiB `kernel_boot_stack_*`，进入 `kmain` 前切到镜像尾栈；该值成立依赖“末段覆盖镜像尾”与“16 字节对齐”两个前提，后者由链接脚本的 2MiB 收口顺带满足。文档 §3.3、§4.1、§5.2 的主干。

- 反证尝试：
  1. 找“栈顶不是镜像尾”的生产路径。x86_64/aarch64 UEFI 与 riscv64 U-Boot 的 v1 路径都从 ELF 现算 `kern_size`；riscv64 自引导用 `KERN_SIZE = 0x40_0000`，L9 用 `readelf` 实测跨距对账；三份当前工件（x86_64/aarch64/riscv64）实测跨距都是 `0x400000`，与常量一致。当前工件上没有反例。
  2. 找“16 字节对齐断言不存在或不被满足”的反例。`os/kernel/src/lib.rs:956-960` 有 `arch_boot: kern_stack_top must be 16-byte aligned`；`KernelInfo::kern_stack_top()` 在 `os/libs/minix-boot/src/kernel_info.rs:200-207` 用 `checked_add`。三份生产脚本的 `.bss` 末尾都有 `. = ALIGN(0x200000)`（`x86_64.ld:98`、`aarch64.ld` 末尾、`riscv64.ld:105`），2MiB 对齐蕴含 16 字节对齐。没有找到违反路径。
  3. 找“两个栈被混为一谈”的反例。`kernel_boot_stack_bottom/top` 在 `.bss` 内预留 64 KiB（`x86_64.ld:86-91`），`kern_stack_top` 是收口后的镜像尾；`os/kernel-image/src/main.rs:154-166` 的 `_start` 用 `lea rsp, [rip + kernel_boot_stack_top]`，`arch_boot_finish`/`resume_additive` 在跳转前传 `kinfo.kern_stack_top()`。文档 §3.3 的“两个栈分工”成立。
  4. 找“收口填充不在 `.bss` memsz 内”的反例。`ALIGN(0x200000)` 写在 `.bss` 的括号里（`x86_64.ld:74-99`），`readelf -lW` 的末段 `memsz` 到 2MiB 边界；L5/L7 实跑 PASS。没有找到反例。
  5. 找“测试载体的 `KernelInfo` 与文档 §5.2 的复述一致”的反例。**这一条找到了**：文档 §5.2 说“各载体同形、取值按架构；以 x86_64 载体为例：`kern_virt_base: VirBytes(0xFFFF_8000_0000_0000)`、`kern_size: 0x200_000`、`kern_size: 0x200_000`”。实际 `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:51-62` 是 `kern_phys_base: PhysBytes(image_base)`、`kern_size: image_end - image_base`（`image_base`/`image_end` 在 `:35-37` 由 UEFI 镜像窗口现算）；aarch64 载体 `test-higher-half-aarch64/src/main.rs:49-53` 同形；riscv64 载体 `test-higher-half-riscv64/src/main.rs:118-129` 又是 `kern_virt_base = 0xFFFF_FFC0_0000_0000`、`kern_phys_base = DRAM_BASE`、`kern_size = 0x400_000`。文档的 `0x200_000` 字面量与“各载体同形”都不成立，且同一行把 `kern_size` 写了两遍。
  6. 找“aarch64 direct-core 的常量跨距等于实际镜像跨距”的反例。`bootface_a64.rs:87` 的 `KERN_SIZE = 0x40_0000`，`aarch64.ld` 末尾只有 `ASSERT(__bss_end - KERN_VIRT_BASE <= 0x400000, ...)`，是上界断言；L9 只对 riscv64 执行。若未来镜像缩到 2MiB，`KERN_SIZE` 仍是 4MiB，`kern_stack_top` 将不是“镜像尾”。当前工件实测跨距 4MiB，所以当前构建没有触发；这是“前提一”在 aarch64 侧不被等值检查保证的逻辑缺口，不是当前工件上的失败。

- 结果：反证失败（主干公式与当前工件成立）；附两条非阻断观察：①§5.2 的 x86_64 载体 `KernelInfo` 字面量/“各载体同形”复述与源码不符（P2 事实错误，公式本身仍成立）；②aarch64 direct-core 的 `KERN_SIZE` 只有上界断言，文档“末段覆盖镜像尾”的前提一在该腿不是被等值检查保证的（P2 逻辑缺口，当前工件因跨距恰为 4MiB 未触发）。
- 证据：
  - `os/libs/minix-boot/src/kernel_info.rs:186-207`（v10 删除字段、访问器与 `checked_add`）。
  - `os/boot-shim/src/loader.rs:175-216`（`compute_kernel_layout` 的 `kern_end_phys − kern_phys_base`）。
  - `os/kernel/src/lib.rs:914-960`（`boot_validate_and_prepare` 的能力事实与 16 字节断言）。
  - `os/kernel-image/x86_64.ld:74-99`；`riscv64.ld:80-105`；`aarch64.ld` 末尾 `ASSERT`。
  - `os/kernel-image/src/main.rs:154-166`（`_start` 的 64 KiB 引导栈）。
  - `os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half/src/main.rs:35-37,51-62`；`test-higher-half-aarch64/src/main.rs:49-53`；`test-higher-half-riscv64/src/main.rs:118-129`。
  - 实跑：`SKIP_BUILD=1 bash os/kernel-image/check-layout.sh all` 的 L5/L7/L9 全 PASS；`readelf -lW` 三架构末段 `memsz` 都收口到 2MiB 边界。
- 若我错了：能推翻它的证据是：某条生产腿的 `kern_size` 与实际映射范围不一致（例如 `KERN_SIZE` 小于实际跨距且没有被链接期断言挡住），或 `kern_virt_base`/`kern_size` 不是 2MiB 对齐导致 `kern_stack_top` 16 字节断言失败，或 `_start` 与 `kmain` 使用了同一个栈符号（两个栈没有分工）。当前源码与工件都没有这三种情况；aarch64 的常量上界是唯一需要继续用等值检查补强的地方。

---

## CLAIM 05

- 主张：ELF 解析器是自建的零分配迭代器；`SegmentIter::next()` 对越界/畸形段采取“跳过”而不是报错（文档把段数据越界列为第三类 `continue`，并称其为已知设计缺口 D2-05）；`ElfError` 共九个变体，`InvalidSegment` 覆盖 `filesz > memsz`、`paddr + memsz` 溢出、`kern_end − kern_base` 下溢、物理重叠四种原因；宿主测试为 `minix-elf` 23 个、`loader.rs` 22 个（总共 125 个 `boot-shim` 测试）。文档 §3.2、§5.1、§5.4 M6/M7 的主干。

- 反证尝试：
  1. 找“段数据越界仍被静默跳过”的反例。**找到相反执行路径**：`os/libs/minix-elf/src/lib.rs:393-446` 的 `next()` 现在对 `checked_mul`/`checked_add` 溢出返回 `Some(Err(ElfError::PhdrOutOfBounds))`（`:395-404`），对 `parse_phdr` 失败返回 `Some(Err(e))`（`:408-411`），对 `p_offset + p_filesz` 溢出或越过镜像尾返回 `Some(Err(ElfError::SegmentOutOfBounds))`（`:419-425`），对 `p_filesz > p_memsz` 返回 `Some(Err(ElfError::FileszExceedsMemsz))`（`:431-433`）。唯一保留的 `continue` 是 `p_type != PT_LOAD` 的正常过滤（`:413-415`）。`SegmentIter` 的文档注释也明说“A malformed program header surfaces as `Err`, never as a silent skip (I-48 / D-084)”（`:321-326`）。
  2. 找“`ElfError` 仍是九个变体”的反例。**找到**：`lib.rs:157-198` 当前有 12 个变体，新增了 `SegmentOutOfBounds`（`:180`）、`FileszExceedsMemsz`（`:186`）、`TargetBufferTooSmall { needed_end, buf_len }`（`:194-197`）。文档 §3.2 的九变体表与“`filesz > memsz` 返回 `InvalidSegment`”的归属都已被源码推翻：`filesz > memsz` 现在返回 `FileszExceedsMemsz`，不是 `InvalidSegment`。同一段的“四个 `InvalidSegment` 判断点由 `loader.rs` 产生”也过时：当前 `loader.rs` 只有三个 `InvalidSegment` 产生点（`:196` 的 `paddr + memsz` 溢出、`:208` 的 `kern_end_phys − kern_phys_base` 下溢、`:353` 的段间重叠），第四种 `filesz > memsz` 已移入迭代器并改报 `FileszExceedsMemsz`。
  3. 找“文档 §3.2 与 §4.2 不冲突”的反例。**两节直接冲突**：文档 §3.2 第 499 行写“第三类是越界检查，它的动作也是跳过——一个段声明的文件数据超出了镜像长度时，迭代器不报错、不截断，直接不产出这个段……已知设计缺口（设计批 D2-05）”；而 §4.2 第 817-818 行引用的源码注释写“I-48 / D-084: a malformed program header is an `Err` item, not a silent skip; `filesz > memsz` is rejected inside the iterator”。同一文档对同一函数给出相反的失败语义，且 §4.2 与当前源码一致、§3.2 与当前源码相反。
  4. 找“宿主测试清单与源码对不上”的反例。**找到**：`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` 实测 27（文档 §5.1 写 23）；`grep -c '#\[test\]' os/boot-shim/src/loader.rs` 实测 31（文档 §5.1 写 22）。这两条用的就是文档自己给的口径，可直接推翻。文档 §5.1 表里的 `test_segment_iter_skips_segment_data_out_of_bounds` 已不存在；实际是 `test_segment_iter_reports_segment_data_out_of_bounds`（`lib.rs:1005`），并新增了 `test_segment_iter_truncated_mid_segment_is_err`（`:679`）、`test_segment_iter_filesz_exceeds_memsz_is_err`（`:721`）、`test_segment_iter_non_load_filter_stays_silent`（`:752`）、`test_segment_iter_rejects_filesz_greater_than_memsz`（`:1029`）。§5.4 M6/M7 的 23/22 计数同样过时。至于“默认特性下共 125 个测试”，`grep -rh '#\[test\]' os/boot-shim/src/*.rs | wc -l` 的源文件总数是 146，扣除默认关闭的 `opensbi_helpers`（11 个）后约 135；我没有运行 `cargo test`（会写 `target/`，超出本角色只读约束），所以只把 loader.rs 的 31 与 minix-elf 的 27 作为直接反证，125 这个总数不单独断言。
  5. 公平核对文档里仍然成立的部分。文档 §3.2 第 457 行说“`SegmentIter::new` 与 `parse_phdr` 的表范围算式是未检查加法，畸形输入会 panic 而不是 `Err`（D2-01）”；这一条**仍成立**：`lib.rs:357-358` 的 `e_phoff as usize + (e_phnum as usize) * (e_phentsize as usize)` 与 `:271` 的 `offset + entsize` 都是未检查加法。但这条成立不能救第 1–4 条的相反执行路径；它只说明 `next()` 的逐项检查与 `new()`/`parse_phdr` 的入口算术是两种不同的失败面。
  6. 找“设计合同支持文档 §3.2”的反例。设计合同 `02-design.v3.md:69-70` 与 `02-outline.v4.md:101` 确实仍写“9 变体 / 越界跳过”，但它们是 2026-11-02 的旧快照；当前源码在 I-48/D-084 之后已经 fail-closed，且文档 §4.2 自己已经引用了 I-48/D-084。把旧设计快照当“文档 §3.2 正确”的依据，会与文档 §4.2 和当前源码同时冲突。
  7. 找“模块装载不经解析”的反例。**找到一处措辞过强**：文档 §3.2 说 boot-shim 装载内核镜像时用解析器，而“模块是按原字节整块拷贝的，不经解析——`loader.rs` 的 `fn load_boot_modules_into` 里没有段迭代”。实际 `loader.rs:485-518` 的 `load_boot_modules_into` 在拷贝前调用 `elf_file_end(data)`（`:517`），后者在 `:227-235` 调用 `minix_elf::segment_iter(data)` 并遍历 `PT_LOAD` 只为把文件修剪到最后一个段的文件尾（D-082/I-36）。它不是“按段拷贝”，但确实是段迭代/解析；“不经解析、没有段迭代”不成立。这一条只是 P2 措辞问题，不改变“模块字节整块拷贝”的主干。

- 结果：阻断（P1；事实错误且与 §4.2 自相矛盾）。§3.2 的“越界静默跳过 / D2-05 设计缺口”与九变体错误分类必须按当前源码重写；§5.1/§5.4 的测试计数与测试名必须重算。主干“自建零分配迭代器、按 `e_phentsize` 步进、非 `PT_LOAD` 过滤”仍然成立，但错误处理这一面已被源码反证。另附 P2 措辞问题：§3.2 的“模块装载不经解析、没有段迭代”与 `load_boot_modules_into` → `elf_file_end` → `segment_iter` 的实际调用链不符。
- 证据：
  - `os/libs/minix-elf/src/lib.rs:157-198`（12 个 `ElfError` 变体）。
  - `os/libs/minix-elf/src/lib.rs:321-326,376-446`（`Err` 语义与 `next()` 的 checked 分支）。
  - `os/libs/minix-elf/src/lib.rs:547-1150`（27 个 `fn test_`；新增/改名的迭代器测试）。
  - `os/boot-shim/src/loader.rs:184-191`（`seg_result?` 与 I-48/D-084 注释）、`:345-354`（`collect::<Result<Vec<_>, _>>()?`）。
  - 复算命令：`grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` → 27；`grep -c '#\[test\]' os/boot-shim/src/loader.rs` → 31；`grep -rh '#\[test\]' os/boot-shim/src/*.rs | wc -l` → 146（源文件总数，未按 feature 过滤；默认关闭的 `opensbi_helpers` 占 11 个）。
- 若我错了：能推翻它的证据是：当前 `SegmentIter::next()` 对越界段仍走 `continue`（源码不是这样），或 `ElfError` 仍只有九个变体（源码有 12 个），或 `minix-elf` 的 `fn test_` 计数恰好是 23、`loader.rs` 的 `#[test]` 计数恰好是 22（同口径实测 27/31）。若文档被明确限定为“I-48 之前的代码快照”，则 §3.2 可以成立；但文档在同一篇 §4.2 引用了 I-48/D-084 并声称当前实现，因此这个限定不成立。

---

## CLAIM 06

- 主张：v1 生产交接（UEFI x86_64/aarch64、riscv64 U-Boot）的 arena 有七块载荷，分别是 `memmap/reserved/modules/platform/param/sources/startup`；接收侧把这七块落进本镜像 `.bss`。文档 §3.1（第 365 行）与 §4.4 路径 A（第 1018 行）。

- 反证尝试：
  1. 查 wire 的权威定义。`os/libs/minix-boot/src/handoff_v1.rs:289-296` 的版本注释写明 v10（D-062）把三个派生标量（`user_sp`/`kern_stack_top`/`syscall_entry`）和 **startup-object 分区**一起删除；“the startup arena partition (20 × 72 = 1440 B) is retired”。`BootHandoffV1` 的实际字段（`:307-333`）只有六个 payload 长度描述符：`memmap_byte_len`、`reserved_regions_byte_len`、`boot_modules_byte_len`、`platform_sources_byte_len`、`param_buf_byte_len`、`sources_byte_len`；没有 `startup_byte_len`。两个发送侧也一致：`os/boot-shim/src/uefi_helpers.rs:1220-1222` 写“v10 (D-062) removed the startup-object partition”，`os/boot-shim/src/uboot_main.rs:516-517` 同文；`os/libs/minix-boot/src/boot_storage.rs:300` 写“v10 (D-062) removed the startup-object slice; no field remains here”。
  2. 查接收侧的解码函数。`os/libs/minix-boot/src/handoff_v1_read.rs` 只有 `decode_memmap`、`decode_reserved_regions`、`decode_modules`、`decode_platform_sources`、`decode_sources`、`decode_param_buf` 六个 payload 解码器；没有 `decode_startup`。`os/kernel/src/lib.rs:680-768` 的 `arch_boot_resume_v1` 也只落这六类载荷并构造 `KernelInfo`。
  3. 查“startup”是否以另一种身份存在。`handoff_v1.rs` 里确有 `KIND_STARTUP_OBJECT`/`ORIGIN_STARTUP_OBJECT`（`:112,121`），但它们是 **source-record 分区内部的 kind/origin 标签**，不是独立的 payload 分区；文档把 `sources` 与 `startup` 并列成两块，按 wire 定义不成立。
  4. 查文档其他处对 v10 的认知。文档 §3.3 第 522 行已经写“v10（D-062）之前这里曾是一个独立字段 `kern_stack_top`……字段与三个派生标量一起删除”。同一篇文档知道 v10 删了三个标量，却在 §3.1/§4.4 仍把同一个 v10 删掉的 startup 分区留在“七块载荷”里，属于跨节复述未同步。
  5. 公平核对“七”的可能来源。`os/kernel-image/src/main.rs:374` 的源码注释也写“decodes the seven payload partitions from the arena”；这条注释本身就是过时的。但 wire 的权威定义是 `handoff_v1.rs` 的 `BootHandoffV1`，当前字段与解码函数都是六个；文档不能把过时注释当作 wire 事实。

- 结果：阻断（P1；生产 wire 事实错误，且与文档 §3.3 对 v10 的认知不一致）。把 startup 从载荷清单里删掉、改成六块，并把“七块”两处同步；否则读者会按文档去找一个不存在的 `startup` 分区。
- 证据：
  - `os/libs/minix-boot/src/handoff_v1.rs:289-296`（v10 删除 startup 分区）、`:307-333`（六个 `*_byte_len` 字段）。
  - `os/libs/minix-boot/src/handoff_v1_read.rs:235-362`（六个 `decode_*` 函数，无 `decode_startup`）。
  - `os/boot-shim/src/uefi_helpers.rs:1220-1222`、`os/boot-shim/src/uboot_main.rs:516-517`、`os/libs/minix-boot/src/boot_storage.rs:300`（两个发送侧与存储层都明写 v10 删除 startup-object 分区/切片）。
  - `os/kernel/src/lib.rs:680-768`（`arch_boot_resume_v1` 的六类载荷落 `.bss`）。
  - `os/kernel-image/src/main.rs:374`（过时注释“seven payload partitions”；作为来源说明，不作为 wire 权威）。
  - 文档原句：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:365` 与 `:1018`。
- 若我错了：能推翻它的证据是：`BootHandoffV1` 里仍有一个 `startup_byte_len` 字段（没有），或 `handoff_v1_read.rs` 有 `decode_startup`（没有），或 `arch_boot_resume_v1` 仍读取一个独立的 startup 分区（没有）。若“startup”只是指 `sources` 分区里的 `KIND_STARTUP_OBJECT` 记录，那么文档不应把它与 `sources` 并列成两块，也不应称“七块载荷”；按当前 wire 定义，六块才是正确计数。

---

## 跨节复述冲突汇总

1. **ELF 失败语义**：§3.2（越界静默跳过、D2-05 设计缺口、九变体）↔ §4.2（I-48/D-084 的 `Err` 项、`filesz > memsz` 由迭代器拒绝、`collect::<Result<...>>()?`）。后者与当前源码一致，前者是旧实现的复述。
2. **v10 的删除范围**：§3.3（v10 删除三个派生标量与 `kern_stack_top` 字段）↔ §3.1/§4.4（v1 arena 仍是七块、仍含 startup 分区）。v10 同时删了 startup 分区（`handoff_v1.rs:289-296`），文档只更新了前一半。
3. **测试载体取值**：§5.2（x86_64 载体只给 `kern_virt_base` 与 `kern_size: 0x200_000` 两个字面量，并称“各载体同形”）↔ `test-higher-half/src/main.rs:35-37,51-62`（`kern_phys_base: image_base`、`kern_size: image_end - image_base`）与 `test-higher-half-riscv64/src/main.rs:118-129`（`DRAM_BASE`、`0x400_000`）。公式 `kern_stack_top = kern_virt_base + kern_size` 仍成立，但文档给出的字面量与“同形”结论不成立。
4. **测试计数**：§5.1/§5.4 的 23（minix-elf）与 22（loader）↔ 同口径实测 27 与 31；同一篇的 §5.1 表还保留了已改名的测试 `test_segment_iter_skips_segment_data_out_of_bounds`。文档的“默认特性下共 125 个测试”未用 cargo 复跑（避免写 `target/`），本报告不把它当直接反证。
5. **表行号指错**：§3.1 第 365 行说 riscv64 测试载体“跳转完成时 PC 落在高半 VMA（下表第 5 行）”，但 §3.1 的表（第 369-376 行）第 5 行是 x86_64/aarch64 测试载体、PC 为低地址；riscv64 测试载体是第 3 行。同一段的文字结论（riscv64 PC 高）与 §5.2 一致，只有行号指错；这是 P2 交叉引用错误，但会把读者引到相反结论的那一行。

---

## 完成声明

已完成对文档 6 条核心主张的反证：

- CLAIM 01（双地址布局 / `AT()` / L4）：反证失败，主张成立。
- CLAIM 02（`k_initial_stktop` 高半落点）：反证失败，主张成立；ARM 侧“待补”实为文档覆盖度保守，现有 C 源可补证。
- CLAIM 03（成对换栈跳转 / trait / 三架构 asm）：反证失败，主张成立。
- CLAIM 04（栈顶派生 / 两个栈 / 收口）：主干反证失败，主张成立；附 §5.2 载体字面量与 aarch64 常量上界两条 P2 观察。
- CLAIM 05（ELF 迭代器静默跳过 / 九变体 / 测试计数）：反证成功，阻断 P1；与 §4.2 自相矛盾，需按当前源码重写 §3.2、§5.1、§5.4 M6/M7。
- CLAIM 06（v1 arena 七块载荷含 startup）：反证成功，阻断 P1；v10 已删除 startup 分区，wire 只有六块，需同步 §3.1 与 §4.4。

本轮没有修改仓库任何文件；唯一写入是本文件。反证过程中未读取作者结论性解释、历史评审或其他面板角色输出。

