DOC 02-higher-half-kernel.md

# D02 反证报告（v312 / round1）

反证对象：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（2121 行）。
读取范围：该文档；`v312/SCOPE.md`；`DECISIONS.md` 的 D-047/D-062/D-110/D-111/D-112/D-116/D-117/D-119 行；`related-code.txt` 的 30 个 Rust 文件（按 §7.2/§7.3 锚点选读）；文档 §7.2 列出的 Minix3 C 源；文档锚点指向的 `.ld`、`check-layout.sh`、`run_qemu.sh` 等。**未读** `panel/round1/` 下其他面板角色的文件（design-d2.md / reader-a/b/c.md），也未读旧轮面板目录。
仓库改动：本次反证没有写任何 tracked 文件；唯一写入的文件是本报告，另在 `/tmp` 留了两份只读复核的串口/QEMU 日志（`/tmp/refute-riscv64-direct.log`、`/tmp/refute-thh-riscv.log`）。**评审期间工作树里出现了并行改动**：`os/kernel/src/lib.rs` 的 `kmain` 断言从 `ptr::eq` 改成“高半别名或物理别名”的三选一（另含 `events.csv`/`V3.11.yaml` 的评审状态改动），来自并行工作流，不是本报告所为。本报告的代码结论以**评审开始时的工作树/工件**为基线；该并行改动恰好落在 CLAIM 05 指出的失败路径上，我在 CLAIM 05 里单独标注它的含义。

结论摘要：6 条核心主张中 3 条反证失败（成立），3 条成立反证——其中 1 条 P0、2 条 P1。P0 落在**直核交接的地址语义**：`jump_to_kmain` 在 riscv64 直核腿把 `.bss` 副本的**低物理视图地址**原样交给 `kmain`，而生产 `kmain` 的 `ptr::eq(kernel_info, &KERNEL_INFO)` 在高半视图里与高 VMA 地址比较；文档既承认前者（直核面传物理视图引用），又把后者写成两条生产路径合流的保证，二者不能同时成立。

---

## CLAIM 01

- **主张**：C 侧建表次序是 J3 的原型——`pg_clear → pg_identity（把 1024 个 4MB 项全部铺成恒等）→ pg_mapkernel（从页目录下标 961 起写内核高半项）→ pg_load → vm_enable_paging`，且 `pg_identity` 与 `pg_mapkernel` 的先后不可交换（§1.2 J3 行、§2.4 的五步表与反序推演、§2.6 归纳表）。

- **反证尝试**：
  1. 读 `kernel.lds` 的三个常量并复算下标：`printf '%d\n' $((0xF0400000 / 0x400000))` → `961`；`printf '0x%x\n' $((0xF0400000 - 0x00400000))` → `0xf0000000`。与 §2.4/§6.2 的读数一致。
  2. 逐行核对 `pg_identity`：循环上界 `I386_VM_DIR_ENTRIES`=1024，步进 `I386_BIG_PAGE_SIZE`=4MB，条件 `(mem_high_phys & 0xFFC00000) <= (phys & 0xFFC00000)` 额外加 `PWT|PCD`；`I386_VM_ADDR_MASK_4MB` 确为 `0xFFC00000`，即向下取整。
  3. 逐行核对 `pg_mapkernel`：两条 4MB 对齐断言；`pde = kern_vir_start / I386_BIG_PAGE_SIZE`；循环条件 `mapped < kern_kernlen`，步进 4MB；返回 `pde`。`kern_kernlen` 没有对齐断言，末块向上取整——与 §3.5 的说法一致。
  4. 逐行核对 `vm_enable_paging`：`#ifdef PAE` 的能力检查、先清 `PG` 与 `PGE|PSE`、再置 `PSE`、再 `PG`、再 `WP`、最后按 `pgeok` 置 `PGE`；文档摘录把 PAE 段标为省略，正确。
  5. 反序推演复核：`pg_mapkernel` 先写 `pagedir[961] = kern_phys|flags`，`pg_identity` 后写 `pagedir[961] = 961*4MB|flags = 0xF0400000|flags`；两者相差 `_kern_offset`，高半基址被解释成物理 0xF0400000。文档的推演与代码一致。
  6. 找“第三条写点”：`grep -rn 'pagedir\[' minix3/minix/kernel/arch/i386/*.c` 只有 `pg_utils.c:182`（identity）、`:195`（mapkernel）、`:299`（`pg_map` 的 4KB 路径，开启分页之后才走）。没有能在 enable 前改写高半条目的第三条路径。
  7. `assert(cbi->mem_high_phys)` 的证明力：`mem_high_phys` 只在 `add_memmap`（`pg_utils.c:113-114`）里抬高，`pre_init.c:111` 先清零；文档说它只能证明“至少一次 add_memmap 产生非零上界”，不能证明清单已切完——成立。

- **结果**：**反证失败（主张成立）**。

- **证据**：
  - `minix3/minix/kernel/arch/i386/kernel.lds:4-6,14-22,29-35`。
  - `minix3/minix/kernel/arch/i386/pg_utils.c:162-184`（pg_identity）、`:186-201`（pg_mapkernel）、`:204-245`（vm_enable_paging）、`:247-256`（pg_load/pg_clear）。
  - `minix3/minix/kernel/arch/i386/pre_init.c:217-238`（八条语句的 pre_init）。
  - `minix3/minix/include/arch/i386/include/vm.h:34-38,47-58`（1024/4MB/CR0/CR4 位值与那处 `I386_CR0_WP` 注释错误）。
  - 复算输出：`961`、`0xf0000000`。

- **若我错了**：如果 `pagedir[961]` 还有我在 `pg_utils.c` 之外没找到的写入点（例如汇编或另一编译单元直接写这个静态数组），或 `vm_enable_paging` 在置 `PSE` 之前先置 `PG`，则先后不可交换的推演不成立。我只在 `pg_utils.c` 里找到三处写点，且 `pg_map` 的写点在 `enable` 之后；若作者知道第四处，请给出文件与行号。

---

## CLAIM 02

- **主张**：C 侧 `k_initial_stktop` 落在**高半** `.bss`，不是 `.unpaged_bss`；`__k_unpaged_` 这个字样在定义处是手写的、在引用处是 `objcopy` 加的，**不是**“它进了 unpaged 段”的证据（§2.2、§2.5、§2.6；§5.2 的 L10 与 §5.4 的 M2/M4 判据同源）。

- **反证尝试**：
  1. 试图证明 `mpx.o` 被收进低侧：读 `i386/Makefile.inc` 的 `.for unpaged_obj in ...` 名单（9 个显式对象 + `MINLIB_OBJS_UNPAGED` 3 + `MINC_OBJS_UNPAGED` 13 + `SYS_OBJS_UNPAGED` 2 = 27）；名单里没有 `mpx.o`。`mpx.S` 只出现在 `SRCS+= mpx.S arch_clock.c ...` 这一行。复算命令与输出与 §2.2 一致（13/3/2）。
  2. 试图证明 `kernel.lds` 的 `.unpaged_bss` 会收走 `mpx.o` 的 `.bss`：`.unpaged_bss ALIGN(4096) : { unpaged_*.o(.bss COMMON) }`；通配匹配的是**输入文件名**，`mpx.o` 没有 `unpaged_` 前缀；通用 `.bss` 在 `. += _kern_offset` 之后，收 `*(.bss* COMMON)`。落点唯一。
  3. 试图证明 `mpx.S` 的栈不在 `.bss`：`mpx.S:628-641` 是 `.data`、`.bss`、`k_initial_stack:`、`.space K_STACK_SIZE`、`LABEL(__k_unpaged_k_initial_stktop)`、`k_boot_stack`、`LABEL(k_boot_stktop)`。确实在 `.bss`。
  4. 试图证明前缀机制不闭合：`asm.h:64-65` 在 `__ELF__` 下 `_C_LABEL(x)` 原样返回；`:98-99` `_LABEL(x)` 是 `.globl x; x:`；`:177` `LABEL(y)` = `_LABEL(_C_LABEL(y))`。`head.o` 被 `objcopy --prefix-symbols=__k_unpaged_` 改名后，`head.S` 的 `k_initial_stktop` 引用变成 `__k_unpaged_k_initial_stktop`，与 `mpx.S` 里手写的字面量同名。
  5. 试图证明 `kmain` 的符号名两侧不闭合：`pre_init.c:2` 有 `#define UNPAGED 1`；`kernel/proto.h:49-52` 在 `#ifndef UNPAGED` 下 `#define kmain __k_unpaged_kmain`；`main.c:115` 定义 `kmain`，因此定义名是 `__k_unpaged_kmain`，与 `head.o` 里被 objcopy 改过的 `call` 引用同名。
  6. 试图用 ARM 侧反例推翻“同一判据”：`earm/Makefile.inc:8,12,17-21,61-63` 的 `MINLIB_OBJS_UNPAGED` 只有 `get_bp.o`，另多 `divmodsi4.o divsi3.o udivsi3.o umodsi3.o` 与 `${BSP_OBJS_UNPAGED}`，`mpx.o` 仍不在名单；`earm/kernel.lds:17-22` 的 `.unpaged_data` 也只收 `unpaged_*.o`；`earm/mpx.S:265-272` 在 `.data/.bss/.data` 切换后把 `k_initial_stack` 放进 `.data`，`mpx.o` 不在名单 ⇒ 落通用 `.data`（高半）。与 §2.6 一致。
  7. 读 `check-layout.sh` 的 L10：它断言名单含 `head.o/pre_init.o/pg_utils.o/klib.o`、不含 `mpx.o`，`.lds` 有三个 `unpaged_*` 段，`mpx.S` 有 `LABEL(__k_unpaged_k_initial_stktop)`，`head.S` 有 `mov $k_initial_stktop, %esp`。与 §5.2 的 L10 行一致。

- **结果**：**反证失败（主张成立）**。这条是本篇证据链最完整的一节；我逐项找不到反例。

- **证据**：
  - `minix3/minix/kernel/arch/i386/Makefile.inc:8,12,17-32,51-71,75`。
  - `minix3/minix/kernel/arch/i386/kernel.lds:14-22,29-35`。
  - `minix3/minix/kernel/arch/i386/mpx.S:628-649`。
  - `minix3/sys/arch/i386/include/asm.h:64-65,98-99,177`。
  - `minix3/minix/kernel/proto.h:49-52`；`minix3/minix/kernel/main.c:115`；`minix3/minix/kernel/arch/i386/pre_init.c:2`。
  - `minix3/minix/kernel/arch/earm/Makefile.inc:8,12,17-21,61-63`；`earm/kernel.lds:17-22`；`earm/mpx.S:265-272`。
  - 复算：`awk ... | grep -oE '[A-Za-z0-9_]+\.o' | wc -l` → 13；`MINLIB_OBJS_UNPAGED` → 3；`SYS_OBJS_UNPAGED` → 2。

- **若我错了**：如果链接器的 `unpaged_*.o(...)` 匹配的是**输出名**而不是输入文件名，或 `mpx.o` 还被另一条我没看到的规则做过 `objcopy --prefix-symbols`，或 `mpx.S` 的 `.bss` 被某个 COMDAT/`COMMON` 规则拉进低侧，则结论反转。我检查了 `Makefile.inc` 的全部 `.for` 块与 `kernel.lds` 的全部收段语句，没有找到这样的规则。

---

## CLAIM 03

- **主张**：Rust 侧 `kern_stack_top` 是派生值（`kern_virt_base + kern_size`），其成立的三个前提——末段覆盖镜像尾、镜像尾 16 字节对齐（由 2MiB 收口顺带满足）、`kernel_boot_stack_*` 是 `.bss` 里 64KiB 的入口栈且其顶与 `__bss_end` 同值（§3.3、§3.5、§5.2 L7）——在三个生产镜像上都成立。

- **反证尝试**：
  1. 试图证明栈顶可能落在收口填充之外：读三份 `.ld`，`.bss` 括号内的顺序是 `__bss_start` → 各 BSS 节 → `. = ALIGN(16)` → `kernel_boot_stack_bottom` → `+= 0x10000` → `kernel_boot_stack_top` → `__bss_end` → `. = ALIGN(0x200000)` → `__image_end`。末段 `memsz` 包含收口填充；装载方按 `[filesz,memsz)` 清零（boot-shim）或入口汇编清到 `__bss_end`（自引导），文档的“填充只作栈用、不需预清零”成立。
  2. 试图证明 `kernel_boot_stack_top != __bss_end`：`nm` 三份工件：
     - x86_64：`kernel_boot_stack_top = __bss_end = 0xffff800000553020`，`__image_end = 0xffff800000600000`；
     - aarch64：`0xffff8000002c6d10`，`__image_end = 0xffff800000400000`；
     - riscv64：`0xffffffc0002b5360`，`__image_end = 0xffffffc000400000`。
     三份都相等；填充量分别约 0xACFE0 / 0x1392F0 / 0x14ACA0，都在 `0..2MiB` 内。
  3. 试图证明 16 字节对齐不成立：`kernel_info.rs:253-259` 的访问器是 `checked_add`；生产镜像的首段 VMA 与 `kern_size` 都是 2MiB 的倍数（`readelf -lW`：三架构末段 `VMA+memsz - 首段 VMA` 都是 `0x400000`），所以 `kern_stack_top` 16 字节对齐；`boot_validate_and_prepare` 的 `kern_stack_top.0.is_multiple_of(16)` 断言（`lib.rs:916-920`）与三份 `jump_to_kmain` 的本地断言一致。
  4. 试图证明末段不是 `.bss`：`readelf -lW` 三份工件都是 4 个 `LOAD`；第 4 段是 RW、memsz 含 `.bss`、末地址正好 `__image_end`。`Section to Segment mapping` 也把 `.bss` 放在第 4 段。
  5. 试图证明 L7 的“64KiB + 末段”检查与文档描述不符：脚本 `check-layout.sh:257-271` 取 `kernel_boot_stack_bottom/top`，要求 `top - bottom == 0x10000` 且 `bottom >= 末段 VMA`、`top <= 末段 VMA+memsz`。与文档的 L7 行一致；但文档 L7 行括号里的“**不检查自顶向下方向**”是错的：`top - bottom = 0x10000` 已经隐含 `top > bottom`（见 CLAIM 06 的 P2 附注）。
  6. 试图证明 `kern_size` 用了 VMA 跨距而映射用了 LMA 跨距：`loader.rs:268-284` 用 `min(paddr)`/`max(paddr+memsz)`；当前三份脚本的 L4 平移一致，所以与 VMA 跨距相等；§5.2 L5 行已经把这个等式的前提与“未来插 VMA-only 填充会断”写清了。

- **结果**：**反证失败（主张成立）**。

- **证据**：
  - `os/kernel-image/{x86_64,aarch64,riscv64}.ld` 的 `.bss` 尾部与两条 `ASSERT`。
  - `os/libs/minix-boot/src/kernel_info.rs:242-259`。
  - `os/kernel/src/lib.rs:872-920`（`boot_validate_and_prepare` 的派生与 16 字节断言）。
  - `os/kernel/src/arch/{x86_64,aarch64,riscv64}/higher_half.rs` 的本地断言。
  - `os/kernel-image/check-layout.sh:257-271`（L7）。
  - 工件读数：`readelf -lW`（4 个 LOAD、跨度 0x400000、首段 VMA/LMA）；`nm`（栈顶与 `__bss_end` 同址、`__image_end` 2MiB 对齐）。

- **若我错了**：如果某个架构的最后一个 `LOAD` 不是 `.bss`（例如出现 `.lbss` 之外的孤儿段排在收口之后），或 `kern_virt_base + kern_size` 不是 16 的倍数，或 `.ld` 的收口从 `.bss` 括号里移到外面，则前提不成立。当前三份工件与三份脚本都不支持这些反例；但“收口被移到括号外”这种改法正是 §3.5 的失败对照，文档已经写了。

---

## CLAIM 04

- **主张**：§3.1 的字段对应表与 §4.1 的失败推演把“入口由谁决定”归给 `entry_point`：`ENTRY(_start)` → `entry_point`（`KernelLoadResult`）→“装载方决定从哪里进入镜像”；并且“把 `KEEP(*(.text.boot))` 从 `.text` 首位挪走后，装载方（或固件）从 `ENTRY` 得到入口后**仍然能跳**”。（§3.1 表 `ENTRY(_start)` 行；§4.1 “把 `KEEP(*(.text.boot))` 从 `.text` 首位挪走”段。）

- **反证尝试**：
  1. 查 v1 发送侧的实际跳转目标：`os/kernel/src/lib.rs:447-554` 的 `prepare_kernel_image_v1` 在第 533 行是 `let entry = VirBytes(core::hint::black_box(report.kern_virt_base.0));`——跳的是 `kern_virt_base`（首段 `min(p_vaddr)`），**不是** `entry_point`。
  2. 查装载入口的注释与断言：`os/boot-shim/src/loader.rs:553-575` 的 `load_kernel_with_loader_gated` 明确写 “The jump target is `kern_virt_base` (the lowest PT_LOAD VMA), not `entry_point`; cross-check the ELF header here so a link-script or section-order change fails loudly”，随后 `assert_eq!(image.entry_point, image.kern_virt_base.0, "boot-shim: ELF entry does not equal the first PT_LOAD VMA")`。UEFI 腿（`uefi_helpers.rs:1031-1058` 的 `assert_eq!`）和 U-Boot shim 腿（`opensbi_helpers.rs:359` 调用 `load_kernel_with_loader_gated`）都走这条闸。
  3. 查文档是否提到这条闸：`grep -n 'entry does not equal\|load_kernel_with_loader_gated\|kern_virt_base.*entry'` 在本篇 §3.1/§3.2/§4.1/§4.2 都没有命中；§4.2 的“装载前的校验分成值对象的两步”只列 `compute_kernel_layout` 与 `KernelImage::validate`，没有列这条入口等式断言与 U-Boot 腿的 `layout_gate`。
  4. 反推 §4.1 的失败路径：把 `KEEP(*(.text.boot))` 挪走、`ENTRY(_start)` 不变时，`e_entry` 移到新位置，`min(p_vaddr)` 仍是首段 VMA；`compare` 的 L6a 会红、L6b 仍绿（文档这两句是对的），但生产装载器在 `load_kernel_with_loader_gated` 的 `assert_eq!` 处**直接 panic，根本不会跳到镜像**。所以“装载方（或固件）从 `ENTRY` 得到入口后仍然能跳”对三条 v1 腿是错的；对直核腿，OpenSBI 跳的是最低 `p_paddr`（`bootface.rs` 头部），也不是 `e_entry`。
  5. 找“入口确实是 entry_point”的腿：x86_64/aarch64 的 UEFI 测试载体是 PE，不是这条链；三架构生产链没有一条用 `e_entry` 决定跳转。唯一用到 `e_entry` 的地方是 `compute_kernel_layout` 把它放进 `KernelLoadResult` 供 `check-layout.sh` 的 L6 与上面的 assert 使用。

- **结果**：**阻断（P1）**。当前行为是 fail-closed（assert 拒载），所以不是运行期 bug；但文档把“入口地址的所有权”写反了，并且漏掉了生产装载链上唯一一条“入口 = 首段 VMA”的运行时闸。§4.2 的“装载前校验两步”因此不完整。

- **证据**：
  - 文档原文：§3.1 表（`ENTRY(_start)` 行）与 §4.1 的“装载方（或固件）从 `ENTRY` 得到入口后仍然能跳”。
  - `os/kernel/src/lib.rs:447-554`（`prepare_kernel_image_v1`；第 533 行 `entry = report.kern_virt_base.0`）。
  - `os/boot-shim/src/loader.rs:553-575`（注释 + `assert_eq!`）。
  - `os/boot-shim/src/uefi_helpers.rs:1031-1058`（UEFI 腿的同一 assert）。
  - `os/boot-shim/src/opensbi_helpers.rs:359`（U-Boot shim 走 `load_kernel_with_loader_gated`）。
  - `os/kernel-image/src/bootface.rs:43-47`（OpenSBI 跳最低 `p_paddr`，不是 `e_entry`）。

- **若我错了**：如果 v1 还有第二条我没读到的跳转路径确实使用 `entry_point`（例如某个 `fw-*` 变体），或者 `load_kernel_with_loader_gated` 只用于测试、不在三条 v1 生产腿上，则“装载方用 `kern_virt_base`”这一反例不成立。我按 `load_kernel_with_loader_gated`/`compute_kernel_layout` 的调用点查了 UEFI 与 OpenSBI 两条 shim 族，二者都在生产路径上；若作者知道反例，请给出调用点。若作者的本意只是“`entry_point` 是 ELF 声明的入口，装载方必须让它等于首段 VMA”，那么表格的“读者关心的语义”一列与 §4.1 的那句仍然需要改写。

---

## CLAIM 05

- **主张**：两条生产路径在 `kmain` 处合流；两条路径在跳转前都把 `KernelInfo` 落进 `.bss` 副本，跳转时消费的是副本地址；生产 `kmain` 用 `ptr::eq` 断言形参与 `KERNEL_INFO` 同址，字段读取走全局副本；并且“生产 `kmain` 只比较指针、不解引用”，所以直核面在分页开启前传物理视图引用是可接受的（§3.7 的 Safety 三条前置条件与前置条件表第 2 行；§4.5“两条生产路径在 `kmain` 处合流”段；§4.5 状态表“本篇主线的终点在这里”）。

- **反证尝试**：
  1. 查直核腿跳转时 `kinfo` 的数值：riscv64 `jump_to_kmain` 的 asm 用 `in("a0") kinfo`，块内只用 `mv sp`/`li s0`/`fence.i`/`auipc`/`la`/`bltu`/`add`/`jalr`，**从不改写 a0**（`os/kernel/src/arch/riscv64/higher_half.rs` 的 asm 与前后的注释）。`jump_to_kmain` 的 `delta` 只加到 `t1`（`kmain` 地址），不碰 `a0`。
  2. 实测 a0：用仓库现有的 `test-higher-half-riscv64` 载体（同一 `arch_boot → arch_boot_finish → Riscv64HigherHalf` 链），QEMU `-s -S` + gdb-multiarch 在 `jump_to_kmain` 入口断下：
     ```
     AT_JUMP_ENTRY: a0=0x8028ce08 pc=0x8020414a
     ```
     a0 是 `.bss` 副本的**低物理视图地址**（载体 delta = `kern_virt_base − DRAM_BASE` = `0xffffffbf80000000`，所以高别名 = `0xffffffc00028ce08`；生产镜像的 `kern_phys_base` 是 `0x80200000`，delta 是 `0xffffffbf7fe00000`），PC 在低半。随后 asm 加 delta 跳到高半 `kmain`。
  3. 查生产 `kmain` 的地址比较：`os/kernel/src/lib.rs:1839-1845` 是
     ```rust
     let stored: &KernelInfo = crate::kernel_info().expect(...);
     assert!(core::ptr::eq(kernel_info, stored), "... same .bss copy");
     let kernel_info: &KernelInfo = stored;
     ```
     `kernel_info()` 读 `globals::KERNEL_INFO`；在高半视图里这个全局的地址是 `KERNEL_INFO` 的高 VMA（本轮 riscv64 生产工件 `readelf -sW`：`0xffffffc0000c99b8`），而实参是低别名（`0xffffffc0000c99b8 - delta = 0x802c99b8`）。两者数值不同。
  4. 查生产 `kmain` 的机器码：`kmain` 的 `auipc a1, 0xc2; addi a1, 0x4ae` 得到 `&KERNEL_INFO` 的运行时地址（本轮工件 `kmain` = `0xffffffc0000074f8`，`KERNEL_INFO` = `0xffffffc0000c99b8`），随后 `bne a0, a1, panic_fmt`（`0xffffffc00000751c`；`llvm-objdump` 读数）。高半视图下 a1 = 高 VMA；a0 是低别名；`bne` 走 panic 路径。
  5. 查是否存在把 a0 归一化到高半的补偿：`grep` `delta`/`kern_virt_base`/`kern_phys_base` 在 `arch_boot_finish`、`store_boot_report`、`land_and_store`、`jump_to_kmain` 里；唯一的 delta 加法在 `higher_half.rs` 的 `t1` 上。没有对 `a0` 的 `+delta`/`−delta`。
  6. 查“post-assert 的直核全 boot 证据”：最新全 boot 串口日志 `os/target/riscv64gc-unknown-none-elf/boot-full-serial.jfull1.log` 的 mtime 是 2026-11-21 05:29；`ptr::eq` 随提交 `ea18db20e`（2026-11-23 20:01，替换原先的临时打印）落地。仓库里唯一一条时间上接近的 riscv64 `kmain Phase A enter` 日志是 `tmp/review-loop/01-stage/evidence/y03-strict-kmain-riscv64.log`（2026-11-23 12:12），但它是 `test-riscv64-uboot-shim` 的 **v1 接收腿**（`kernel-image: resuming high-half boot`，高半进入），不是直核腿；无论 12:12 的工作树是否已含断言，它都不能证伪直核腿的结论。我用当前 `release/kernel` 复跑直核全 boot 时，在更早的 FDT 预扫处就因跳转表故障停住（见 CLAIM 06），所以没能在本轮把断言跑到。直核全 boot 到达 `kmain` 的唯一形状证据因此是“共享 jump 入口的 a0 实测 + 生产 kmain 的机器码”，两者组合已经足够。
  7. 文档自身的矛盾：§3.7 的 Safety ② 明确写“直核面在分页开启前传物理视图引用”，同一行的结论却写“生产 `kmain` 只比较指针、不解引用”——比较指针本身正是跨视图失败的动作；§4.5 又把 `ptr::eq` 同址写成合流的保证。两处不能同时为真。
  8. 文档的“第一条语句即遮蔽”也不准确：生产 `kmain` 的第一条语句是 `kernel_info()` 读全局，第二条是 `ptr::eq` 断言，第三条才遮蔽；如果断言失败，`boot_stage!("kmain Phase A enter")` 永远不会打印。
  9. 评审期间工作树的并行改动：`os/kernel/src/lib.rs` 的 `kmain` 现在（工作树，不是提交）是
     ```rust
     let stored_addr = stored as *const KernelInfo as usize;
     let arg_addr = kernel_info as *const KernelInfo as usize;
     let delta = stored.kern_virt_base.0 as usize
         .wrapping_sub(stored.kern_phys_base.0 as usize);
     assert!(arg_addr == stored_addr
         || arg_addr.wrapping_add(delta) == stored_addr
         || arg_addr.wrapping_sub(delta) == stored_addr, "...");
     ```
     注释写“The direct-core legs call this function from the physical view, so the argument can be the physical alias of the same object while the global is the high-half alias”。这不是本报告所为；它恰好是对 CLAIM 05 失败路径的修复。含义：(a) 它**证实**了评审基线的 `ptr::eq` 在直核腿会失败（否则不需要改成别名感知）；(b) 若以该工作树为准，P0 的“必然 panic”降为“已修”，但文档 §3.7/§4.5 仍写 `ptr::eq` 同址与“只比较指针、不解引用”，**Doc Code Sync 失败**，文档必须改写为“接受同一 `.bss` 副本的高半别名或物理别名”；(c) 该修法本身还需要审：它接受的第三个分支 `arg_addr - delta == stored_addr` 与第二个分支对称，但没有任何证据表明 `arg_addr ± delta` 对应的物理页就是 `stored` 的物理页（它只验证数值差，不验证别名关系）；如果 `kern_virt_base`/`kern_phys_base` 被写坏，或 `.bss` 的物理别名不在线性映射内，这个断言会接受一个错误指针。这个修法不在本次文档快照里，我只把它作为“我的反证成立”的旁证记录。

- **结果**：**阻断（P0）**。这是“引导交接的地址语义/所有权/失败路径”交叉点上的一条硬反证：直核腿的 `kmain` 形参是 `.bss` 副本的物理视图别名，生产 `kmain` 却拿它跟高半视图的 `&KERNEL_INFO` 做数值比较，按文档自己承认的视图模型必然不等；文档没有讨论这条失败路径，反而把合流当成结论。**口径**：以评审基线（提交版 `ptr::eq` + 我实测的工件）为准是 P0；评审期间工作树出现的别名感知断言是对这条路径的修复，若采纳，文档侧仍有 Doc Code Sync 的 P1（§3.7/§4.5 仍写 `ptr::eq`/“只比较指针”），且该修复的 `arg_addr ± delta` 判据本身需要审（见反证尝试 9）。

- **证据**：
  - 文档原文：§3.7 Safety 三条前置条件与前置条件表第 2 行；§4.5 的 `ptr::eq`/合流段；§4.5 状态表。
  - `os/kernel/src/lib.rs:1839-1845`（生产 kmain 的 `ptr::eq`）。
  - `os/kernel/src/arch/riscv64/higher_half.rs`（`jump_to_kmain` 的 asm，a0 不被改写）。
  - `os/kernel/src/lib.rs:215-248`（`arch_boot_finish` 把 `store_boot_report` 的返回值直接交给 `jump_to_kmain`）。
  - gdb 读数：`a0=0x8028ce08 pc=0x8020414a`（载体 `test-higher-half-riscv64`，`jump_to_kmain` 入口）。
  - 生产工件读数：`KERNEL_INFO` 高 VMA `0xffffffc0000c99b8`（本轮 `readelf -sW`），低别名 `0x802c99b8`（减 delta）；`kmain` = `0xffffffc0000074f8`，`bne a0, a1` 在 `0xffffffc00000751c`（`llvm-objdump`）。工件地址会随重编移动，判据是“PC 相对算出的高 VMA 与低别名数值不等”这一关系，不是具体数字。
  - 复跑命令：`qemu-system-riscv64 -machine virt -smp 1 -m 256M -display none -serial null -bios default -kernel os/target/riscv64gc-unknown-none-elf/release/test-higher-half-riscv64 -s -S`，gdb `break *0x8020414a; continue; printf "a0=%#lx pc=%#lx\n", $a0, $pc`。
  - 反证时间线：`boot-full-serial.jfull1.log` = 2026-11-21 05:29；`ea18db20e` = 2026-11-23（`git log -S 'KernelInfo argument and KERNEL_INFO global'`）。

- **若我错了**：以下三者任一成立我就错了——(a) 有一条 **2026-11-23 之后**的 riscv64 直核全 boot 串口日志，打印了 `kernel: kmain Phase A enter` 且没有该 assert 的 panic；(b) `jump_to_kmain` 在跳转前对 `a0` 做了视图归一化（`+delta` 或 `−delta`），或 `store_boot_report` 在直核腿返回的是高别名而不是低别名；(c) 生产 `kmain` 的 `ptr::eq` 两端在运行时被编译成同一个数值（例如编译器把形参常量折叠成高 VMA）。我检查了源码与机器码，没有发现 (b)/(c)；gdb 的 a0 实测排除了“形参是高别名”。若作者能给出 (a) 的日志，这条降为 P2 的文档说明（“直核别名在 kmain 入口被接受”需写明机制）。

---

## CLAIM 06

- **主张**：直核自引导镜像的早期代码在物理视图下“靠 PC 相对寻址自洽（`la` 算出的值与当前 PC 同侧），只有最后跳 `kmain` 需要按 direct leg 加视图差值 `delta`”（§4.5 自引导路径末段）。另外，概述/登记表/测试章对“进入高半”的复述应一致（§1.1 四时刻表的“进入”行、§3.1 的五行视图登记表、§4.5.1 的“谁建根”表、§5.2 的 L 表）。

- **反证尝试**：
  1. 查“早期代码全是 PC 相对”的反例：仓库自己的 `os/.cargo/config.toml:47-53` 写：
     > Early boot executes before every block of the image is reachable through the active translation; a compiler-generated jump table in `.rodata` can make an indirect branch land on an unmapped address (**observed as a RISC-V instruction access fault inside the FDT pre-scan**). Disable jump tables for this target …
     > `[target.riscv64gc-unknown-none-elf] rustflags = ["-C", "jump-tables=no"]`
     同一文件 `:55-60` 对 aarch64 也加了 `jump-tables=no`，理由是“the direct-boot face calls the FDT pre-scan before every image page is reachable”。`os/xtask/src/image.rs:1015-1028` 又把 `-C jump-tables=no` 显式注入 `RUSTFLAGS`，注释写“An explicit RUSTFLAGS overrides the target config, so the flag has to be here”。也就是说：早期代码的“PC 相对自洽”不是 `la` 的语言性质，而是一条**构建期开关**维持的性质；没有它，编译器生成的跳转表会把 `.rodata` 里的绝对表项当目标，跳到未映射的高地址。
  2. 查文档是否提到这个开关：`grep -n 'jump-tables\|跳转表' 02-higher-half-kernel.md 01-boot-shim-bootstrap.md 03-kmain-cstart.md` 全部无命中。§4.5 的那句话是绝对表述（“只有最后跳 `kmain` 需要 delta”），没有给“在 `jump-tables=no` 的构建下”这一前提。
  3. 复现该失败形状：用当前 `os/target/riscv64gc-unknown-none-elf/release/kernel`（含本次评审的代码）与仓库现有的 `qemu.dtb`/`table.bin`/`mod_*.bin` 直核点火，`-d int` 记录：
     ```
     riscv_cpu_do_interrupt: hart:0, async:0, cause:0000000000000001,
       epc:0xffffffc000033dda, tval:0xffffffc000033dda, desc=fault_fetch
     ```
     `0xffffffc000033dda` 落在 `minix_boot::fdt_layout::validate_structure`（`readelf -sW`：`0xffffffc000033d2a` + 606 字节）内部；`llvm-objdump` 在 `0xffffffc000033dc0`–`0xffffffc000033dd8` 显示 `slli a1,a1,3; ld a2,0x18(sp); add a1,a1,a2; ld a1,0(a1); jr a1`——一张**表驱动的间接跳转**，表项是绝对链接地址。CPU 仍在低物理视图（`Domain0 Next Address: 0x80200000`），所以 `jr` 跳到高 VMA 立刻取指失败；串口停在 self-boot banner。
     **口径**：这个工件是否带 `-C jump-tables=no` 我无法从 ELF 直接证明；如果它是某次未注入该 flag 的构建，故障是构建配置问题，不是当前源码的必然行为。但两点不受影响：(i) 文档的“只有最后跳需要 delta”缺少这条构建期前提；(ii) 仓库自己的 config 注释证明同一形状在 FDT 预扫里**真实发生过**，而且 aarch64 直核面同样依赖它。
  4. 跨节复述：§1.1 四时刻表的“进入”行写“PC 的落点按形态分（riscv64 高半，x86_64/aarch64 **测试载体**低半）”。§3.1 的五行登记表第 4 行写 aarch64 **direct-core（生产镜像）**“低地址进入、物理视图”；§4.5.1 的“谁建根”表也把 aarch64 直核归为独立镜像 `_start`（低进）。§1.1 的这句只列了“riscv64 高半 / x86_64、aarch64 测试载体低半”两档，漏掉了 aarch64 直核生产腿的“低进、高落”，读者会把它读成二值划分。
  5. 同一类小问题：§5.2 的 L7 行括号写“（不检查自顶向下方向）”，但 `check-layout.sh:257-271` 的 `span_want = top - bottom == 0x10000` 已经隐含 `top > bottom`；真正没检查的是“入口汇编把 SP 设成 `kernel_boot_stack_top`”这一运行期事实，不是方向本身。
  6. 另一处注释与代码的张力：§4.5 状态表把 `init_protection` 的文档注释 “must be the very first thing called in kmain” 解释成“指安装动作的次序，不是 `kmain` 的第一条语句”；但 `lib.rs:1830-1917` 的实际顺序是先 `validate`、`check_boot_postconditions`、`Console::init`、`init_from_kinfo`，再 `init_protection`。§4.3 对 Step 1 的旧注释采用了“源码注释与实现不一致时以实现为准”的处理；§4.5 这里却改写了注释语义，两节的处理口径不一致。状态表“跳转完成时保护结构未建立”这个结论本身是对的，问题在依据的写法。

- **结果**：**阻断（P1）**（早期视图自洽缺少 `jump-tables=no` 这一构建期前提，且仓库已有实测故障记录）；跨节复述的 §1.1 进入行、L7 括号、`init_protection` 注释解释属 **P2** 观察。

- **证据**：
  - 文档原文：§4.5 自引导路径末段；§1.1 四时刻表“进入”行；§5.2 L7 行；§4.5 状态表的 `init_protection` 行。
  - `os/.cargo/config.toml:47-60`；`os/xtask/src/image.rs:1015-1028`。
  - QEMU `-d int` 读数：`fault_fetch epc=tval=0xffffffc000033dda`；`readelf -sW` 的 `validate_structure` 符号范围；`llvm-objdump` 的 `jr a1` 表驱动跳转。
  - `os/kernel-image/check-layout.sh:257-271`（L7）。
  - `os/kernel/src/lib.rs:1830-1917`（kmain 的实际调用序）与 `init_protection` 的文档注释。

- **若我错了**：如果所有 kernel-image 产物的构建都被**强制**注入 `-C jump-tables=no`（target config 在无 `RUSTFLAGS` 时生效，xtask 在有 `RUSTFLAGS` 时补注入），并且作者认为“构建契约”不需要写进 §4.5 的地址视图叙述，那么 (1) 降为 P2（文档没有点名一条已由构建系统保证的开关）；如果当前工件的跳转表来自 `jump-tables=no` 管不到的机制（例如函数指针表而不是 switch 表），则我的“跳转表”归因需要修正，但“早期代码并非无条件 PC 相对自洽”这一反例仍然成立。跨节复述的 (4)/(5)/(6) 若作者认为属于“概述允许举例不全/注释可以意译”，可降为观察；但 (4) 与 §3.1 的权威登记表直接冲突，(5) 是可执行的断言事实，二者至少应改字。

---

## 附：本轮反证失败的条目与已核对但未写入主张的读数

- 反证失败（主张成立）的正面证据已并入 CLAIM 01–03；这里只列未单独立项、但已核对的读数，便于作者复核：
  - `check-layout.sh` 的 L0–L10 与文档 §5.2 的表逐项一致；L9 的 `bootface.rs`/`bootface_a64.rs` 选择、L10 的 x86_64/all 输出条件均一致；L4 的平移量 `0xffff800000000000 / 0xffff7fffbfe00000 / 0xffffffbf7fe00000` 与三份工件一致；`grep -c ': AT('` 三份各 4、`grep -c 'AT('` x86_64 4 / 另两份 5，与 §6.2 一致；VPN[2] 复算 `0x100` / `0x0` / `0x0` 与 §4.1 一致；`MINC_OBJS_UNPAGED` 13、`MINLIB` 3、`SYS` 2 与 §2.2 一致；`minix-elf` 27 个测试、`loader.rs` 32 个 `#[test]`、五个 crate 依赖与 §3.2/§5.1 一致；`test-higher-half-riscv64` 的 `kmain` 符号 `0x80200144` 与 §6.2 的断点值一致，实跑 PASS 的入口读数为 `SP=0xffffffc000400000`、`PC=0xffffffc000200146`、`FP=0`（PC 比 `nm` 值多 2，是 `kmain_verify` 在 `auipc` 处取 PC 的偏移，不影响 §6.2 的 GDB 断点口径）。
  - `os/kernel/src/lib.rs` 的 Step 1 源码注释仍写“同尺寸重写返回 `Ok`；`AlreadyMapped` 只在 1 GiB 叶时出现”，与三份 `map_huge` 实现不符；文档 §4.3 已显式声明“以实现为准”。我复核了三份 `map_huge`：x86_64/riscv64/arm64 的 2MiB 分支在目标叶已 present 时都返回 `AlreadyMapped`，且 1GiB 分支先拒绝任何已存在的 PDPT 条目。文档的这条自认是准确的。
  - `new_from_page` 的 `write_bytes(ptr, 0, 512)` 是 512 个 `u64`（4KiB），文档 §4.3 的括号写法无误；`from_active_root` 不清零，`resume_additive` 不重建根，文档 §3.6/§3.8 的分工成立。
  - 直核/跨镜像的 `arch_boot_finish` 与 `resume_additive` 调用序、`BootWindows` 的 180 容量、`BOOT_WINDOW_FLOOR`、1024/256 页预算、x86 低 2MiB 窗口、`(0, 2 MiB)` 追加、shim 三窗口（image/stack/dumpdtb）、U-Boot shim 的 `[SHIM_LINK_BASE, UBOOT_SHIM_STACK_TOP)` 均与源码一致。
  - `kernel_identity_mmio_regions`（riscv64 = PLIC + UART，x86/aarch64 空）与 `device_window_regions`（x86 LAPIC/IO-APIC bootstrap=true；aarch64 空）与文档 §4.3 的对照一致；`device_window_flags` 的 `DEVICE` 编码与 §4.3 的“反向不可恢复”说明一致。
  - 文档 §4.3 的窗口清单里“直核腿的调用者栈是固件栈或固件给出的栈，不在内核 LMA 窗口内”这一句对两个**自举**面并不精确：`main.rs` 的 aarch64/riscv64 `_start` 在调用 `arch_boot` 之前已经把 SP 设成 `kernel_boot_stack_top`（镜像自己的 `.bss` 内），所以直核自举面的“调用者栈”其实在 kernel LMA 窗口内；这句话真正成立的是 x86_64/aarch64 的 inline 测试载体（UEFI 固件栈）。窗口本身加得对，只是理由对自举面不成立——这是一条观察（P2），没有单独立项。
