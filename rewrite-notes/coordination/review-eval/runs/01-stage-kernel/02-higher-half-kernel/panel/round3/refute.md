# 反证报告：02-higher-half-kernel.md

反证范围：只读文档一遍；源码核对集中在 `minix3/minix/kernel/arch/{i386,earm}/` 与 `os/{kernel-image,kernel,boot-shim,libs,arch,qemu-tests}`。每条主张按「主张 / 反证尝试 / 结果 / 证据 / 若我错了」记录。反证失败（主张成立）也是合法结果。

---

## CLAIM RD3-CLAIM-01 — 双地址链接的必要性，以及「谁读 p_paddr」决定脚本形态

**主张**：同一镜像必须有两套地址；C 侧必须逐段写 `AT(ADDR(sec) - _kern_offset)`，写成常量 `AT(_kern_phys_base)` 会让五个高半段在物理上互相覆盖，删掉 `AT()` 则装载器会向高半 VMA 写段体；Rust 侧按「有没有读者按 p_paddr 摆放」分成三种脚本形态（`os/kernel-image/*.ld` 带 `AT()`、`os/kernel/src/arch/*/link.ld` 无链接读者、测试载体单地址脚本）。

**反证尝试**：

1. 找「VMA 恰好等于 0xF0400000 的段」，若存在，则 `AT(ADDR(sec) - _kern_offset)` 与 `AT(_kern_phys_base)` 在该段上等价，文档的「没有这样的段」会被削弱。实际：`.unpaged_text/.unpaged_data/.unpaged_bss` 都非空，`. += _kern_offset` 之后位置计数器已大于 0xF0400000；`.usermapped_glo` 与 `.usermapped` 各有供料（`usermapped_glo_ipc.S` 的 `.section .usermapped_glo`；`usermapped_data.c` 8 个与 `usermapped_data_arch.c` 3 个 `__section(".usermapped")` 对象），所以不存在 VMA 恰为 0xF0400000 的段。
2. 找「常量 AT 只重叠一部分」的可能：五个带 AT 的段 LMA 全为 0x00400000，且低侧 `.unpaged_*` 也占 0x00400000 起的一段，重叠范围只会更大，不会更小。
3. 实测现行 GNU ld 对 LMA 重叠的行为。用最小 i386 脚本（`.text` 在 0x00400000、`.data` 用 `AT(0x00400000)`）复现，ld 报 `section .data LMA [00400000,00400003] overlaps section .text LMA [00400000,00400000]` 并退出非零。文档「现行 GNU ld 对 LMA 重叠报错并拒绝产出 ELF」成立。
4. 找「生产路径不读 p_paddr」的分支。`load_segments_into_phys_memory` 直接用 `seg.paddr` 作目的地址；`compute_kernel_layout` 的 `kern_phys_base/kern_size` 来自 `paddr`；UEFI 与 OpenSBI 两条腿都经 `load_kernel_with_loader(_gated)` 调它。没有按 vaddr 摆放内核镜像的生产分支。
5. 找「arch link.ld 被链接」的证据。全仓 `*.rs/*.toml/*.sh/Makefile` 检索 `arch/.*link.ld` 只命中注释与 `link_ld_lockstep.rs` 的常量锁步测试，无 `-T` 使用；三份 arch 脚本常量与生产脚本一致。
6. 用已构建工件核对：三架构 ELF 都是 `Type: EXEC`，首段 LMA 分别 0x200000 / 0x40200000 / 0x80200000，所有 LOAD 行的 `vaddr − paddr` 各自一致；`check-layout.sh` 的 L3c/L4 全 PASS。

**结果**：反证失败（主张成立）。

**证据**：`minix3/minix/kernel/arch/i386/kernel.lds:4-6,17-22,25-34`；`minix3/minix/kernel/arch/i386/usermapped_glo_ipc.S:10`；`minix3/minix/kernel/usermapped_data.c:4`；`minix3/minix/kernel/arch/i386/usermapped_data_arch.c:4,14,24`；`os/boot-shim/src/loader.rs:175-215,245-345,374-408`；`os/kernel-image/x86_64.ld:26-35`；`os/kernel-image/aarch64.ld:38-45`；`os/kernel-image/riscv64.ld:41-48`；`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld` 的 `KERN_*_BASE`；`os/kernel/tests/link_ld_lockstep.rs:43-72`；`os/kernel-image/check-layout.sh:181-210`；工件 `os/target/{x86_64-unknown-none,aarch64-unknown-none,riscv64gc-unknown-none-elf}/release/kernel`（`readelf -lW`）。

**若我错了**：如果某个生产 leg 按 vaddr 而不是 paddr 摆放内核段，或某个链接脚本里存在 VMA 恰为基址的段，则「常量 AT 必然重叠」「有低 LMA 才有读者」这两条会被削弱。实际两条腿都走物理地址拷贝，且低侧段非空；未找到反例。

---

## CLAIM RD3-CLAIM-02 — 段体按 p_paddr 摆放，BSS 按 memsz 清零

**主张**：内核镜像的段体由装载方按 ELF 的 `p_paddr` 写到物理内存，并按 `[filesz, memsz)` 清零；宿主测试只能证明 buffer 孪生的拷贝/清零语义，物理写入留给真机链。

**反证尝试**：

1. 找「内核镜像按 vaddr 摆放」的分支：`load_segments_into_phys_memory` 用 `seg.paddr`；`load_elf_into`/`load_vm_elf` 才用 `vaddr` 逐页映射用户 ELF。两条去向与文档的字段表一致。
2. 找「BSS 不清零」的分支：物理版只在 `memsz > filesz` 时 `write_bytes`；缓冲区版同样；入口汇编三架构都清 `__bss_start..__bss_end`。文档「两个环节各自成立、叠起来才是三架构一致的保证」成立。
3. 找「重叠检测被绕过」的分支：`first_overlapping_pair` 用半开区间判交叠、相邻不算；源/目标重叠是 panic 而非返回值；文档对这对不对称行为的描述准确。
4. 找「物理版自身会拒绝 filesz > memsz」：没有；它只做重叠检测与拷贝。文档明确写了 D02-F05（调用者必须先跑 `compute_kernel_layout`），并把生产顺序归给 `load_kernel_with_loader`，与代码一致。
5. 核对宿主测试是否确如文档所说逐字节验证 0xAB/0xCC 与哨兵：`test_bss_region_is_zeroed_after_load` 正是如此（文件字节 0xAB、BSS 预填 0xCC、`memsz` 后一字节保持 0xCC）。

**结果**：反证失败（主张成立）。

**证据**：`os/boot-shim/src/loader.rs:217-345,374-408,422-471,720-810`；`os/kernel-image/src/main.rs:137-315`（三架构 `global_asm!` 的 `__bss_start/__bss_end` 清零）；`os/arch/src/arch/boot.rs:370-399`（用户 ELF 的 vaddr 映射）。

**若我错了**：如果某个生产 leg 直接调用 `pub unsafe fn load_segments_into_phys_memory` 而没有先跑 `compute_kernel_layout`，则 D02-F05 的边界会被突破；三条腿都经 `load_kernel_with_loader(_gated)`，未找到这样的直接调用。

---

## CLAIM RD3-CLAIM-03 — `k_initial_stktop` 落在高侧，`__k_unpaged_` 前缀不是判据

**主张**：`mpx.o` 不在 `objcopy --prefix-symbols` 的改名名单里，它的 `.bss` 被链接脚本的通用 `.bss` 收走，而通用 `.bss` 在 `. += _kern_offset` 之后，所以 `k_initial_stktop` 的符号值是 0xF04xxxxx 一侧；定义处手写的 `__k_unpaged_` 前缀与 `head.o` 被 objcopy 加的前缀只是名字对上，不代表它进了 unpaged 段。

**反证尝试**：

1. 找第二处定义或另一条改名路径。全树只有 `mpx.S:631-634` 的 `k_initial_stack`/`LABEL(__k_unpaged_k_initial_stktop)`；`head.S:84` 是唯一引用。
2. 检查 `mpx.o` 是否可能被 `.unpaged_bss` 的输入文件通配收走。`.unpaged_bss` 的规则是 `unpaged_*.o(.bss COMMON)`，通配对象是输入文件名；`mpx.S` 只出现在 `SRCS+= mpx.S arch_clock.c …`，输出名是 `mpx.o`，不含 `unpaged_` 前缀。
3. 检查 `. += _kern_offset` 与 `.bss` 的先后。`kernel.lds:22` 在 `.usermapped*`/`.text`/`.data`/`.bss` 之前，`.bss` 规则在 `:32`，次序与文档一致。
4. 检查宏展开是否可能改出别的名字。`asm.h` 在 `__ELF__` 下 `_C_LABEL(x)` 原样返回 `x`，`_LABEL(x)` 展开为 `.globl x; x:`，`LABEL(y)` 展开为 `_LABEL(_C_LABEL(y))`，三跳后定义名就是字面量 `__k_unpaged_k_initial_stktop`；引用侧由 `objcopy` 给 `head.o` 的未定义符号加同一前缀。
5. 反向检查文档自述的「假设落在 .unpaged_bss」后果链：`bootstrap_start/bootstrap_len` 覆盖 `.unpaged_*`，`kmain` 在 `system_init()` 之后 `add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)`，且 `pg_mapkernel` 只映射高半范围。文档的条件后果成立。

**结果**：反证失败（主张成立）。

**证据**：`minix3/minix/kernel/arch/i386/Makefile.inc:8,14,19-23,51-54,58-68,75`；`minix3/minix/kernel/arch/i386/mpx.S:628-634`；`minix3/minix/kernel/arch/i386/kernel.lds:17-22,32-34`；`minix3/minix/kernel/arch/i386/head.S:84-87`；`minix3/sys/arch/i386/include/asm.h:64-66,98-99,177`；`minix3/minix/kernel/main.c:298-301`；`minix3/minix/kernel/arch/i386/protect.c:323,338`；`minix3/minix/kernel/arch/i386/pg_utils.c:186-202`。

**若我错了**：如果 `objcopy --prefix-symbols` 会改段名（它不改），或如果链接脚本的 `unpaged_*.o` 通配匹配的是对象内部的符号名而非文件名（不是），结论会反转。两者都不成立。

---

## CLAIM RD3-CLAIM-04 — `kern_stack_top` 的算式、2MiB 收口与页粒度策略

**主张**：`kern_size = max(paddr + memsz) − min(paddr)`；`kern_stack_top = kern_virt_base + kern_size`；链接脚本在 `.bss` 末尾的 `. = ALIGN(0x200000)` 把收口填充算进末段 `memsz`，同时决定跨距、栈顶与映射循环上界；页粒度选择是 1GiB（首选）→ 2MiB（回退）→ panic，当前三架构落 2MiB、跨距 4MiB。

**反证尝试**：

1. 找「平移不一致/末段不是 .bss」导致 `kern_virt_base + kern_size` 不等于虚拟镜像尾的情况。三架构链接脚本都把 `.bss` 放在最后；已构建工件的 L4 全 PASS、末段就是 `.bss`；x86 的 `.lrodata.*` 孤儿段落在收口之前，不影响末段端点。
2. 找「1GiB 档会被选中」的可能。三个物理基址 0x200000 / 0x40200000 / 0x80200000 都不是 1GiB 对齐；x86 的 `kern_virt_base` 是 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，也不是 1GiB 对齐；`kern_size` 4MiB 不是 1GiB 整数倍。`layout_satisfies` 要求三量同时整除，故 1GiB 档必不入选。
3. 检查 2MiB 档的三量整除：x86 `0xFFFF800000200000` / `0x200000` / `0x400000`，aarch64 `0xFFFF800000000000` / `0x40200000` / `0x400000`，riscv `0xFFFFFFC000000000` / `0x80200000` / `0x400000`，均被 2MiB 整除；工件 L5/L9 全 PASS。
4. 检查 panic 是否发生在任何页表写入之前。`arch_boot_impl` 先 `boot_validate_and_prepare`，再 `plan_boot_pages`（`arch_boot: page-size policy: {e}`），之后才 Step 1 建根；`plan_kernel_mapping` 是纯函数。文档「分页还没开就停」成立。
5. 检查 `kern_stack_top` 的 16 字节断言与收口的关系。`arch_boot` 的 `kern_stack_top must be 16-byte aligned` 断言存在；2MiB 收口必然满足 16 字节。工件实测：x86 `kern_virt_base + 0x400000 = 0xFFFF800000600000`，末段 `0x339000 + 0x2c7000 = 0x600000`；aarch64/riscv 同样对齐。
6. 检查「收口缺失 ⇒ L5/L9 红」的复现路径是否可能不红。riscv 去掉 ALIGN 后末段端点降到 `__bss_end`（低于 0x400000），L5 与 L9 都会失败；文档的复现命令与判据成立。

**结果**：反证失败（主张成立）。

**证据**：`os/boot-shim/src/loader.rs:175-215`；`os/boot-shim/src/lib.rs:101-131`；`os/kernel-image/x86_64.ld:74-98`；`os/kernel-image/aarch64.ld:73-98,116`；`os/kernel-image/riscv64.ld:80-105,124`；`os/kernel/src/lib.rs:873-882,925-947`；`os/kernel/src/boot_policy.rs:98-140`；`os/arch/src/x86_64/paging.rs:1154-1156`、`os/arch/src/arm64/paging.rs:1130-1132`、`os/arch/src/riscv64/paging.rs:1125-1127`；`os/kernel-image/check-layout.sh:205-210,248-260`；工件 `os/target/*/release/kernel`（`readelf -lW`：跨距均 0x400000）。

**若我错了**：如果 `kern_size` 改成按虚拟侧端点计算，或 `kern_stack_top` 不再由装载方算，这条链会断；当前实现按物理侧端点算，且 L4 保证平移一致时两口径等价。文档把这一点写成有前提的成立，而不是无条件成立，未找到反例。

---

## CLAIM RD3-CLAIM-05 — 三架构成对跳转的寻址方式、psABI 对齐与屏障

**主张**：x86_64 的 `call` 携带相对位移，要求调用点的执行地址等于链接地址；aarch64 的 `ldr` 取链接期绝对符号值，literal 读取虽 PC 相对但随代码一起摆放在同一视图，故不要求调用点=链接地址；riscv64 的 `la` 是 PC 相对，direct leg 需用 `auipc/bltu` 判断 PC 是否低于 `kern_virt_base` 并补 `delta`；x86_64 还需 `and rsp,-16`、`push 0`、`sub rsp,8` 使 `kmain` 入口 RSP 满足 System V 的 8 mod 16；三架构都清帧指针。

**反证尝试**：

1. 逐条比对三份 asm 与文档摘录：x86_64 六条、aarch64 十条、riscv64 十二条，顺序与寄存器绑定一致；`in("rdi")/in("x0")/in("a0")` 与文档的 ABI 表一致。
2. 找「生产 kmain 是 naked，psABI 垫片非必需」的可能。`kmain` 只在 `#[cfg(feature = "qemu_test")]` 下是 `naked_asm!`；非该 feature 是普通 Rust 函数。`higher_half.rs` 的注释也明确写了这一点。
3. 核对 `sp_aligned` 判定：x86_64 `sp & 0xF == 8`，aarch64/riscv64 `sp & 0xF == 0`，与文档的 psABI 表一致。
4. 找 aarch64 `isb` 是唯一同步点的可能。`enable()` 写 TTBR 后执行 `dsb sy; isb`，写 SCTLR 后再 `isb`；三条 aarch64 路径都经 `enable()`，`jump_to_kmain` 的 `isb` 是防御性保留。文档「其二」成立。
5. 核对 riscv64 `delta` 的 `checked_sub`/`expect`、`auipc t3, 0`、`bltu t3, t4, 3f`、`add t1, t1, t2` 与两条 `jalr`：与源码一致。源码注释里的加数确有一个多写的 `f`（`0xffffffbf7ffe0000`，实际 `delta = 0xffffffbf7fe00000`），文档指出这一点并复算出注释给出的故障地址，复算正确。
6. 核对 x86_64 的对齐算术：`stack_top` 16 对齐时 `and rsp,-16` 空操作；`push 0` 后 −8、`sub rsp,8` 后 −16、`call` 后 −24，入口 RSP ≡ 8 (mod 16)。

**结果**：反证失败（主张成立）。

**证据**：`os/kernel/src/arch/x86_64/higher_half.rs:44-57`；`os/kernel/src/arch/aarch64/higher_half.rs:40-52`；`os/kernel/src/arch/riscv64/higher_half.rs:91-114`（条件 delta 的注释在 `:45-75`）；`os/arch/src/arm64/paging.rs:635-712`；`os/kernel/src/boot/higher_half.rs:33-48`；`os/kernel/src/lib.rs:2069-2099,2144-2155`。

**若我错了**：如果生产 `kmain` 启用帧指针或改成 naked 入口，`sub rsp,8`/`xor rbp` 的必要性论证会变；当前非 qemu_test 的 `kmain` 是普通函数，仓库未启用 `-C force-frame-pointers`。如果 aarch64 的 literal 池不随代码摆放（它随同一镜像的 `.text` 摆放），`ldr` 的绝对性论证会变；当前链接脚本把 literal 放在同一镜像内。

---

## CLAIM RD3-CLAIM-06 — 跳转完成时 PC 的落点与测试载体的视图

**主张**：跳转完成后 PC 落在高半 VMA，例外是 x86_64/aarch64 的内联测试载体（低地址）；riscv64 测试载体低装高进、PC 落在高半；PC 不参与 PASS/FAIL，`pc_at_high` 只在失败分支打印 `INFO`。

**反证尝试**：

1. 读 riscv64 测试载体自己的注释：`test-higher-half-riscv64/src/main.rs:116-117` 写 "PC remains at a low address because QEMU -kernel loads the test binary at 0x8020_0000"。这与文档「riscv64 载体 PC 在高半」相反。继续核对实现：`higher_half.rs` 的 `auipc t3, 0` 读当前 PC，`bltu t3, t4, 3f` 在 PC < `kern_virt_base` 时走 `add t1, t1, t2` 加 `delta`；载体的 `KernelInfo` 取 `kern_virt_base=0xFFFFFFC000000000`、`kern_phys_base=0x80000000`，`la` 取到低地址 `0x8020…`，加 `delta` 后为 `0xFFFFFFC00020…`。因此该注释是修正前/stale；文档 §5.2 已写明「修正前是 0x8020…」并引用 `higher_half.rs` 的实机读数 `0xffffffc0…`。这一路反证失败。
2. 核对 kmain_verify 的判定：`sp_ok && sp_aligned && fp_zero` 决定 PASS；`pc_at_high` 只在失败分支打印 `INFO: PC not at high address (expected in test kernel)`，不参与判定。文档正确。
3. 核对文档自身是否自洽。发现两处与主张/表格相矛盾的表述：
   - 文档行 362 写「测试载体走 inline `arch_boot`，是低地址执行视图」，与紧邻的表行 370（riscv64 测试载体 → 物理视图 → 高半 VMA）、行 372（只有 x86_64/aarch64 测试载体是低地址）以及 §4.3「riscv64 测试载体不是这种」矛盾。该句把三种载体一并概括成低地址视图，对 riscv64 不成立。
   - 文档行 1072（§4.4 的「跳转完成时的值」状态表，程序计数器一行）写「aarch64 direct-core：低地址进入（`ldr` 取高半绝对值跳）」，与行 371、行 1033 的「跳转完成时 PC = 高半 VMA（`ldr` 取绝对值）」以及 aarch64 asm 的 `ldr x1, ={kmain}` + `br x1` 矛盾。`br` 之后 PC 就是 `kmain` 的高半 VMA；「低地址进入」描述的是进入 `bootface_a64` 的起点，不是跳转完成时的 PC。
4. 检查 aarch64 direct-core 是否有跳回低地址的第二次操作：`jump_to_kmain` 只有 `ldr x1, ={kmain}` 与 `br x1`，没有。行 1072 的表述无实现依据。

**结果**：核心主张成立（实现与 `kmain_verify` 支持「riscv64 测试载体 PC 在高半、PC 不参与判定」）；但文档有两处概括性表述与自己的表格/代码矛盾，属非阻断（P2）。建议把行 362 改为「x86_64/aarch64 测试载体是低地址执行视图；riscv64 测试载体低装高进」，把行 1072 的 aarch64 direct-core 改为「高半 VMA（`ldr` 取绝对值）」。

**证据**：`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/src/main.rs:115-117`（stale 注释）；`os/kernel/src/arch/riscv64/higher_half.rs:45-75,91-114`；`os/kernel/src/arch/aarch64/higher_half.rs:48-49`；`os/kernel/src/lib.rs:2144-2175`；测试载体 `test-higher-half/src/main.rs:42-46`、`test-higher-half-aarch64/src/main.rs:41-45`；文档 `rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:362,370-372,1033,1072`。

**若我错了**：如果 riscv64 测试载体的 `arch_boot` 走的是不带 `delta` 的路径（它直接调 `arch_boot`，走 `arch_boot_impl` 的 direct leg，带 `delta`），或如果 `kern_virt_base` 被改成与物理侧同量级（载体取 Sv39 canonical high），则 PC 可能留低；当前代码与载体取值都不满足这两个条件。如果 aarch64 direct-core 在 `br` 前后还有一次低地址跳转（没有），行 1072 才可能成立。

---

- 本次反证：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5）。

---

（主 agent 追加的机器可读索引，**未改动上文正文**。本文件原用 `## CLAIM RD3-CLAIM-<NN>` 编号，轨迹门的 `CLAIM <数字>` 正则识别不到；下面按门要求的形状补 6 行索引，判定与证据见上文各条。）

## CLAIM 11
- 结果：反证失败（对应上文 RD3-CLAIM-01，主张成立）

## CLAIM 12
- 结果：反证失败（对应上文 RD3-CLAIM-02，主张成立）

## CLAIM 13
- 结果：反证失败（对应上文 RD3-CLAIM-03，主张成立）

## CLAIM 14
- 结果：反证失败（对应上文 RD3-CLAIM-04，主张成立）

## CLAIM 15
- 结果：反证失败（对应上文 RD3-CLAIM-05，主张成立）

## CLAIM 16
- 结果：非阻断（对应上文 RD3-CLAIM-06：§4.4 PC 行与 §3.1 测试载体口径的冲突，已按 reader 侧同根条目修复）
