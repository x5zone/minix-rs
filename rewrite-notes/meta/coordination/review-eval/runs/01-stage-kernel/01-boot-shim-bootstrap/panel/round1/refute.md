# D01 P-REFUTE 反证报告（round1）

范围：只读核对 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 中可证伪主张，对照 `minix3/` 与 `os/` 源码。未读取作者结论、其他角色报告、历史评审或 expected。

判定口径：CONFIRM = 反证尝试失败、主张成立；REFUTE = 找到反例或源码与主张相反；UNCLEAR = 证据不足以判定。

## CLAIM 01

- 主张位置：§2.1「约定：`load_mods` 必须写在 `multiboot` 前面」与 §2.6 第一段。
- 原话摘要：反序时模块表仍会建、`MULTIBOOT_INFO_HAS_MODS` 仍会置上，`assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS)` 顺利通过，内核停在 `get_parameters()` 的 `assert(mbi->mi_mods_count > 0)`。
- 反证路径/命令：
  - `grep -n 'assert\|mods_count\|HAS_MODS' minix3/minix/kernel/arch/i386/pre_init.c`
  - `read minix3/sys/arch/i386/stand/lib/exec.c` 742-761 行
  - `read minix3/sys/arch/i386/stand/lib/exec.c` 500-575 行（`module_init`）
- 证据：`pre_init.c:167-169` 依次为 `assert(mbi->mi_flags & MULTIBOOT_INFO_HAS_MODS); assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS); assert(mbi->mi_mods_count > 0);`。`exec.c:742-761`：`if (boot_modules_enabled) { module_init(file); if (btinfo_modulelist) { ... mbi->mi_flags |= MULTIBOOT_INFO_HAS_MODS; mbi->mi_mods_count = btinfo_modulelist->num; ... } }`。`module_init` 在 542-575 行无条件 `len = sizeof(struct btinfo_modulelist)`、`btinfo_modulelist = alloc(len)`、`memset(...,0,len)`、`btinfo_modulelist->num = 0`，即零模块时链表对象仍非空。反序时 `num == 0`，`HAS_MODS` 仍置位，故前一条断言通过，后一条 `> 0` 失败。
- 结论：CONFIRM。
- 反证失败说明：唯一能推翻的路径是“空链表时 `btinfo_modulelist` 为 NULL”，但 `module_init` 的分配长度含结构头（至少 `sizeof(struct btinfo_modulelist)`），不依赖模块数；只有分配失败才会返回 NULL，而那是另一条失败路径。

## CLAIM 02

- 主张位置：§2.2 第 4、5 条与段末「SysV 栈对齐」。
- 原话摘要：`load_stack_start` 按 4096 对齐（由 unpaged 对象列表首位与 `.unpaged_data ALIGN(4096)` 保证），入口处 `%esp = top-16 ≡ 0 (mod 16)`，不满足 SysV 的 `%esp ≡ 12 (mod 16)`；`call` 前 `%esp = top-12 ≡ 4`。
- 反证路径/命令：
  - `sed -n '55,130p' minix3/minix/kernel/arch/i386/head.S`
  - `sed -n '1,110p' minix3/minix/kernel/arch/i386/kernel.lds`
  - `sed -n '40,95p' minix3/minix/kernel/arch/i386/Makefile.inc`
- 证据：`head.S` 的 `multiboot_init` 为 `mov $load_stack_start,%esp; mov $0,%ebp; push $0; popf; push $0; push %ebx; push %eax; call pre_init`；文件末尾 `.data` / `load_stack:` / `.space 4096` / `load_stack_start:`。`kernel.lds` 有 `.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }`。`Makefile.inc` 的 `.for unpaged_obj in head.o pre_init.o ...` 把 `head.o` 放在首位，且 `OBJS.kernel+= ${UNPAGED_OBJS}` 保序进入链接。栈深：`push $0; popf` 净零，随后 `push $0`（top-4）、`push %ebx`（top-8）、`push %eax`（top-12）、`call` 压返回地址（top-16）。top 为 4096 的倍数 ⇒ 入口 `%esp ≡ 0 (mod 16)`，`call` 前 `≡ 4`。
- 结论：CONFIRM。
- 反证失败说明：要推翻需证明 head.o 的 `.data` 不在 `.unpaged_data` 的输入首位，或 `load_stack` 之前还有非零填充。源码显示 `.for` 列表首位是 `head.o`，`head.S` 的 `.data` 第一项就是 `load_stack`；链接脚本按输入顺序收节。注意该结论依赖链接器保序（ld 的通常行为），主张本身已把它表述为“可逐环验证的构建保证”，未超出证据。

## CLAIM 03

- 主张位置：§2.3「已存在先删」两段与 §3.2.2.4 其一。
- 原话摘要：删除扫描 `while (*p++) ;` 后再多走一步 `p++`，从第二条记录起匹配从记录的第二个字符开始，同名键（位于记录起点）再也匹配不上；“键不重复”只在待删键是缓冲区第一条记录时可靠成立；先写 `A=1`、`B=2`、再写 `B=3` 会留下两条 `B`。
- 反证路径/命令：
  - `sed -n '1,115p' minix3/minix/kernel/arch/i386/pre_init.c`
  - 用 Python 逐字节复刻 C 的扫描（bufend=1024、NUL 分隔、双 NUL 终止）跑序列 `A=1 → B=2 → B=3 → A=3`，以及 `A=1 → board=bar → board=intel`、`board=bar → board=intel`
- 证据：`pre_init.c:52-57` 的删除循环为 `while (*p) { if (strncmp(p,name,namelen)==0 && p[namelen]=='=') {...break;} while (*p++) ; p++; }`。逐字节复刻输出：
  - `A1 B2 -> ['A=1','B=2']`
  - `then B=3 -> ['A=1','B=2','B=3']`
  - `then A=3 -> ['B=2','B=3','A=3']`
  - `A1 board=bar board=intel -> ['A=1','board=bar','board=intel']`
  - `board=bar then board=intel -> ['board=intel']`
  与主张逐项一致：非首条重复键留下两条；首条重复键被真正删除；`board=bar` 在首条时被内核值取代，非首条时保留双记录。
- 结论：CONFIRM。
- 反证失败说明：需要找到一种输入让非首条同名键在原位被删。扫描游标在每条记录末尾固定多走一格，除非键名恰好等于该记录第二字符起的后缀（那是文档 §2.3 后文单独列出的“同一 off-by-one 的第二种表现”，不是本句的反例）。

## CLAIM 04

- 主张位置：§2.1 末段、§2.3 命令行解析解码、§3.2.2.4 其二/其三。
- 原话摘要：`grep -rn rootdevname minix3/minix/kernel/` 无命中，内核不解释 `rootdevname`；`arch=` 在内核侧只有写点、没有读点；`board=` 有读点（`get_value`/`env_get`），读取返回第一条匹配，外部同名键可遮蔽内核写入值（仅当它不是 `param_buf` 首条）。
- 反证路径/命令：
  - `grep -rn 'rootdevname' minix3/minix/kernel/`（输出为空）
  - `grep -rn 'ARCHVARNAME' minix3/minix/kernel/`（只有 `pre_init.c:149` 写点与 `earm/pre_init.c:274` 写点）
  - `sed -n '476,505p' minix3/minix/kernel/main.c`（`get_value`）
  - `grep -n 'BOARDVARNAME' minix3/minix/kernel/main.c`
- 证据：`minix3/minix/kernel/` 下 `rootdevname` 零命中。`ARCHVARNAME` 只出现在两处 `mb_set_param` 写入。`main.c:133` 的 `machine.board_id = get_board_id_by_name(env_get(BOARDVARNAME));` 是 `board=` 的内核读者。`get_value` 从头扫描，命中键名后要求下一字符为 `=`，随即 `return(envp + 1)`——第一条匹配胜出；未命中则 `while (*envp++ != 0);` 跳到下一条记录。结合 CLAIM 03 的删除行为：外部 `board=` 为首条时被内核写入删除，非首条时保留并先被 `get_value` 返回。
- 结论：CONFIRM。
- 反证失败说明：要推翻需在内核目录找到 `rootdevname` 或 `ARCHVARNAME` 的读点，或证明 `get_value` 不是首匹配。源码均不支持。

## CLAIM 05

- 主张位置：§2.4.2.2、§2.4.2.3。
- 原话摘要：`alloc_lowest` 把长度 `len` 当结束地址传给 `cut_memmap(cbi, lowest, len)`；唯一调用点在 `copy_trampoline`；在四个前提下真实调用序落在“第三行”（`len ≤ lowest`，切除一次都不发生）；`assert(trampoline_base + tramp_size < (1 << 20))` 只查地址低，不查账是否已改。
- 反证路径/命令：
  - `grep -rn 'alloc_lowest' minix3/minix/kernel/arch/i386/`
  - `sed -n '60,105p' minix3/minix/kernel/arch/i386/pg_utils.c`
  - `sed -n '/copy_trampoline/,/^}/p' minix3/minix/kernel/arch/i386/arch_smp.c | head -60`
  - `grep -n 'cut_memmap' minix3/minix/kernel/arch/i386/arch_system.c`
  - `grep -n 'smp_init\|cstart\|add_memmap' minix3/minix/kernel/main.c`
  - `grep -rn 'BIOS_MEM_BEGIN\|BIOS_MEM_END\|BASE_MEM_TOP\|UPPER_MEM_END' minix3/minix/include/arch/i386/include/memory.h`
  - `grep -rn 'define CONFIG_MAX_CPUS\|define TSS_INDEX' minix3/`
- 证据：`pg_utils.c:65-83` 的 `alloc_lowest` 结尾正是 `cut_memmap(cbi, lowest, len);`，而 `cut_memmap` 第三形参是 `end`（函数头 `void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)`）。检索只命中定义、原型与 `arch_smp.c:76` 的调用。`main.c` 中 `cstart()` 在 147 行、`smp_init()` 在 311 行；`arch_system.c:277-278` 在 `arch_init` 末尾切 `[BIOS_MEM_BEGIN,BIOS_MEM_END]`=`[0,0x4FF]` 与 `[BASE_MEM_TOP,UPPER_MEM_END]`=`[0x90000,0xFFFFF]`；`cut_memmap` 起点向下、终点向上取整，前者实际切 `[0,0x1000)`，且 `add_memmap` 起点向上对齐，故最低条目基址 ≥ `0x1000`。`CONFIG_MAX_CPUS=1`、`TSS_INDEX(cpu)=6+cpu` ⇒ `GDT_SIZE=7`，`IDT_SIZE*DESC_SIZE=256*8=2048`，跳板总量小于 4096，`roundup` 为 `0x1000`，故 `len ≤ lowest` 成立。`copy_trampoline` 的断言只比较 `trampoline_base + tramp_size < (1<<20)`，不检查 `memmap`。
- 结论：CONFIRM（“第三行常态”在四个前提下成立；文档也已声明它不是结构保证）。
- 反证失败说明：要推翻需找到第二个 `alloc_lowest` 调用点、让最低条目基址 < `0x1000`，或让跳板 `roundup(tramp_size) > 0x1000`。当前树三者都不成立。注意 `roundup(tramp_size)==0x1000` 与 `lowest==0x1000` 时是“等号落在第三行”，切除仍是空操作，结论不变。

## CLAIM 06

- 主张位置：§2.4.2.4、§3.2.1 `memmap` 行。
- 原话摘要：C 的 VM 接手这份账时只读前 `MIN(MAXMEMMAP, NR_MEMS) = 16` 条，17–40 号条目被静默忽略，`vm/utility.c` 注释原文 “Any memory chunk in excess of NR_MEMS is silently ignored”；Rust 不继承这条截断。
- 反证路径/命令：
  - `grep -n 'NR_MEMS\|MAXMEMMAP' minix3/minix/include/minix/const.h minix3/minix/include/minix/param.h minix3/minix/servers/vm/utility.c`
  - `grep -n 'silently ignored\|MIN(' minix3/minix/servers/vm/utility.c`
  - 对照 `os/kernel/src/globals.rs` 的 `MEMMAP_REGION_STORE_LEN` 与 `os/kernel/src/vm_handoff.rs` 的 `VM_BOOT_HANDOFF_MAX_REGIONS`（见 CLAIM 10）
- 证据：`const.h:98 #define NR_MEMS 16`；`param.h:13 #define MAXMEMMAP 40`；`utility.c:55-59` 为 `memset(mem_chunks,0,NR_MEMS*sizeof(*mem_chunks)); /* XXX Any memory chunk in excess of NR_MEMS is silently ignored. */ for(i = 0; i < MIN(MAXMEMMAP, NR_MEMS); i++)`。1-based 第 17–40 条对应 0-based 下标 16..39，循环上界 16 不含，确为静默忽略。
- 结论：CONFIRM。
- 反证失败说明：需证明 VM 从别的通道读取 16 条之后的条目，或 `NR_MEMS` 不是 16。当前树没有这样的第二通道。

## CLAIM 07

- 主张位置：§2.4.2「其余模块在“内核账”上不归还」表后分析、§3.4.2 扣除表说明。
- 原话摘要：除 VM 模块外，其余模块的字节区间在 C 里从头到尾都没有被 `add_memmap` 加回过；唯一的回收点是 `arch_boot_proc` 对 VM blob 的 `add_memmap` + 置零。
- 反证路径/命令：
  - `grep -n 'add_memmap' -r minix3/minix/kernel/ | grep -v '\.h:'`
- 证据：i386 树内 `add_memmap` 的调用点只有：`protect.c:450`（VM blob 回收）、`main.c:301`（`add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)` 归还 unpaged 段）、`pg_utils.c:59/61`（`cut_memmap` 内部补回左右余量）、`pre_init.c:182/186/187`（初始建账）。没有任何针对非 VM 模块的加回。`protect.c:447-452` 的注释与代码为 “Free VM blob that was just copied into existence.” + `add_memmap(&kinfo, mod->mod_start, mod->mod_end-mod->mod_start); mod->mod_end = mod->mod_start = 0;`。
- 结论：CONFIRM。
- 反证失败说明：需找到对普通模块 `mod_start` 的第二次 `add_memmap`，或 VM 侧在引导期就把这些区间加回内核账。`add_memmap` 的符号只在上述位置出现；`cut_memmap` 的补回只是保留未切部分，不是归还已切模块区。

## CLAIM 08

- 主张位置：§2.7 ARM32 对照。
- 原话摘要：ARM 的 `INRANGE` 上界写成 `<= thismod->mod_end`，`OVERLAP` 第二项直接取 `mod2->mod_end`、没有 `-1`；模块槽距 8 MiB 且每槽 `mod_end` 各留一页，所以模块—模块不会相切；内核项没有减一页，靠 0 号模块基址与内核物理基址之间 30 MiB 的构建事实避免误报。
- 反证路径/命令：
  - `grep -n 'INRANGE\|OVERLAP\|MB_MODS_ALIGN\|MB_MODS_BASE\|MB_MMAP' minix3/minix/kernel/arch/earm/pre_init.c`
  - `sed -n '1,10p' minix3/minix/kernel/arch/earm/kernel.lds`
  - `grep -n 'load_stack\|\.data' minix3/minix/kernel/arch/earm/head.S`
- 证据：`earm/pre_init.c:167-169` 为 `#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) <= thismod->mod_end)`、`#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || INRANGE(mod1, mod2->mod_end))`；182-184 行 `MB_MODS_BASE 0x82000000`、`MB_MODS_ALIGN 0x00800000`；200-201 行 `mod_end = mod_start + MB_MODS_ALIGN - ARM_PAGE_SIZE`。`earm/kernel.lds` 的 `_kern_phys_base = 0x80200000`，差值 `0x82000000 - 0x80200000 = 0x1E00000`（30 MiB）。`earm/head.S` 的 `load_stack` 在文件末尾 `.data` 段，与 x86 侧同形。
- 结论：CONFIRM。
- 反证失败说明：要推翻需找到 ARM 侧 `OVERLAP` 里存在 `-1`、模块槽距不是 8 MiB，或内核物理基址不是 `0x80200000`。源码均不支持。主张把“闭区间判定无害”限定为“当前镜像尺寸下”，没有把它写成结构保证。

## CLAIM 09

- 主张位置：§2.5 `overlaps` 与 `kern_mod`。
- 原话摘要：`OVERLAP` 判对方起点与最后字节；去掉第二项的 `-1` 会把首尾相接的模块误报为重叠；包含关系只在一个方向命中（被包含者作 `cmp`、包含者作 `thismod`），靠外层对每条记录各调一次补上；相切必须放行。
- 反证路径/命令：
  - `sed -n '1,115p' minix3/minix/kernel/arch/i386/pre_init.c`
  - 对宏做逐项代入（A=[0x100000,0x200000), B=[0x200000,0x300000)）
- 证据：`pre_init.c:73-90` 的 `INRANGE`/`OVERLAP` 与文档引文一致。正确宏下 A、B 相切：`OVERLAP(B,A)` 的两项分别为 `INRANGE(B,0x100000)=false`、`INRANGE(B,0x1FFFFF)=false`，不 panic；去掉 `-1` 后第二项变成 `INRANGE(B,0x200000)=true`，在 `cmp_mod=0`、内层 `m=1` 时即 panic。包含例 A=[0x100000,0x300000)、B=[0x200000,0x210000)：`cmp=B, thismod=A` 时 `INRANGE(A,0x200000)=true` 命中；`cmp=A, thismod=B` 时两项均 false。与主张一致。
- 结论：CONFIRM。
- 反证失败说明：需要构造一个“真重叠但两个方向都不命中”的例子。半开区间的两种相交形态（部分交叠、包含）在上述宏下至少有一个方向命中，找不到反例。

## CLAIM 10

- 主张位置：§3.2.1 字段表、§3.2.3 arena 注释、§4.5「落地区与交接容量」表。
- 原话摘要：`KernelInfo` 共 10 个字段；六个落地区容量为 256/128/12/4/1024/296；`WIRE_SOURCES_CAP=296`（每记录 72 B，分区 21312 B）；`ARENA_CAP=32768`、`WIRE_SOURCES_END=29184`、余量 3584；wire 信封版本 10；`VmBootHandoff.free_regions` 64 条、`deducted` 47 条（3 + 32 + 12），交接前容量断言 fail-closed。
- 反证路径/命令：
  - `sed -n '/pub struct KernelInfo/,/^}/p' os/libs/minix-boot/src/kernel_info.rs`
  - `grep -n 'STORE_LEN\|BOOT_MODULE_NAME_LEN' os/kernel/src/globals.rs`
  - `grep -n 'WIRE_SOURCES_CAP\|WIRE_SOURCE_RECORD_SIZE\|WIRE_SOURCES_END\|ARENA_CAP\|BOOT_HANDOFF_V1_VERSION' os/libs/minix-boot/src/handoff_v1.rs`
  - `grep -n 'VM_BOOT_HANDOFF_MAX_REGIONS\|VM_BOOT_HANDOFF_MAX_DEDUCTED' os/libs/minix-types/src/types/boot.rs`
  - `grep -n 'MAX_BOOT_REGIONS' os/arch/src/arch/frame.rs`
- 证据：`KernelInfo` 字段为 `memmap`、`reserved_regions`、`source_records`、`kern_virt_base`、`kern_phys_base`、`kern_size`、`boot_modules`、`platform_sources`、`param_buf`、`authority`，共 10 个。`globals.rs`：`RESERVED_REGION_STORE_LEN=256`、`MEMMAP_REGION_STORE_LEN=128`、`BOOT_MODULE_NAME_LEN=16`、`PLATFORM_SOURCE_STORE_LEN=4`、`PARAM_BUF_STORE_LEN=1024`、`SOURCE_RECORD_STORE_LEN=WIRE_SOURCES_CAP`。`handoff_v1.rs`：`BOOT_HANDOFF_V1_VERSION=10`、`WIRE_SOURCES_CAP=296`、`WIRE_SOURCE_RECORD_SIZE=72`、`ARENA_CAP=8*4096`、编译期/测试断言 `WIRE_SOURCES_END==29184` 与 `ARENA_CAP-WIRE_SOURCES_END==3584`。`boot.rs`：`VM_BOOT_HANDOFF_MAX_REGIONS=64`、`VM_BOOT_HANDOFF_MAX_DEDUCTED=47`；`frame.rs`：`MAX_BOOT_REGIONS=32`，且 `boot.rs:235-236` 的编译期断言为 `MAX_DEDUCTED >= 3 + MAX_BOOT_REGIONS + NR_BOOT_MODULES`（3+32+12=47）。
- 结论：CONFIRM。
- 反证失败说明：所有数字与源码常量逐项相等；`KernelInfo` 字段计数也没有第 11 个字段。

## CLAIM 11

- 主张位置：§5.0 最小示例末句、§5.2 第一段。
- 原话摘要：`cargo test -p minix-boot --lib` 的读数是 **187/0**；`-p boot-shim --lib` 136/0、`-p minix-kernel --features mock --lib` 866/0/8、`-p minix-elf --lib` 27/0。
- 反证路径/命令：
  - `cd os && cargo test -p minix-boot --lib 2>&1 | tail -1`
  - `cd os && cargo test -p boot-shim --lib 2>&1 | tail -1`
  - `cd os && cargo test -p minix-kernel --features mock --lib 2>&1 | tail -1`
  - `cd os && cargo test -p minix-elf --lib 2>&1 | tail -1`
- 证据（2026-11-22 实测，rustc/cargo 1.94.1）：
  - `minix-boot`：`test result: ok. 188 passed; 0 failed; 0 ignored; ...`（文档写 187/0）
  - `boot-shim`：`test result: ok. 136 passed; 0 failed; 0 ignored; ...`（与文档一致）
  - `minix-kernel --features mock`：`test result: ok. 866 passed; 0 failed; 8 ignored; ...`（与文档一致）
  - `minix-elf`：`test result: ok. 27 passed; 0 failed; 0 ignored; ...`（与文档一致）
- 结论：REFUTE（数值主张错误：187 → 实际 188；其余三个计数成立）。
- 反证失败说明：这不是“反证失败”，而是反证成功。可能的原因是文档写定后 `minix-boot` 新增了一条测试；但文档把该数字写成“§5.2 记录的读数”，在当前树上已不可复现。修复方向是改成 188/0，或删除易漂移的绝对计数改为“全绿 + 计数以当日运行为准”。

## CLAIM 12

- 主张位置：§3.1.1 接线状态表、§3.6 计数换算表、§5.4 真机门链。
- 原话摘要：riscv64 UEFI shim 未接线，原因是 Rust 编译器没有 riscv64 的 UEFI 目标；`test-riscv64-uboot-shim.sh` 的六个串口路标全部出现；`test-kernel-image-riscv64-bootface.sh` 是手工门、不在 `run_all.sh` 中；aarch64 直核面不在五条真机门链里。
- 反证路径/命令：
  - `rustc --print target-list | grep -i uefi`
  - `grep -n 'uboot leg enter\|table built\|kernel + 12 modules loaded\|entering kernel image v1\|resuming high-half\|entering scheduler' os/qemu-tests/test-riscv64-uboot-shim.sh`
  - `grep -n 'bootface\|uboot-shim\|test-riscv64-uboot' os/qemu-tests/run_all.sh`
  - `grep -n 'kernel loaded (entry staged)\|12 boot modules loaded\|memmap source\|DTB preflight' os/qemu-tests/test-shim-bootmarks-aarch64.sh`
- 证据：`rustc --print target-list` 的 UEFI 目标只有 `aarch64-unknown-uefi`、`i686-unknown-uefi`、`x86_64-unknown-uefi`，无 riscv64。`test-riscv64-uboot-shim.sh` 的 206-209 行依次 grep `uboot leg enter (d73)`、`table built (MNXBOOT1`、`kernel + 12 modules loaded (d73)`、`[raw] entering kernel image v1`，44-45 行定义 `KERNEL_MARKER`（`resuming high-half boot (handoff v1 arena, riscv64)`）与 `SCHED_MARKER`（`entering scheduler`），合计 6 个。`run_all.sh` 只登记 `test-riscv64-uboot-shim` 与 `test-riscv64-uboot`，没有 `bootface`。aarch64 脚本的判据是两条固定标记加 `memmap source`/`DTB preflight failed` 变体，不是 `test-smp-aps-aarch64` 的 SMP 判据。
- 结论：CONFIRM（结构层面；未运行 QEMU，不核验“当前是否全过”）。
- 反证失败说明：要推翻需证明存在 riscv64 UEFI 目标、脚本里少于 6 个路标，或 `run_all.sh` 已登记 bootface 门。当前树不支持。

## CLAIM 13

- 主张位置：§3.7 第二段、§4.4.1 第一段、§4.5 名字池行。
- 原话摘要：直核面由 `arch_boot_finish` 先 `store_kernel_info`、再进 `arch_boot_impl` 的 `boot_validate_and_prepare`；v1 接收面在 `resume_additive` 里先 `boot_validate_and_prepare`、后 `store_kernel_info`，两条腿的 store 与 boot_validate 次序相反；v1 接收路径写名字池时 `name_len` 取到 16、不做 `min(15)`；`store_kernel_info` 有一次性守卫，第二次调用 panic。
- 反证路径/命令：
  - `sed -n '/fn arch_boot_finish/,/^}/p' os/kernel/src/lib.rs`
  - `sed -n '740,760p' os/kernel/src/lib.rs`、`sed -n '299,330p' os/kernel/src/lib.rs`
  - `sed -n '693,710p' os/kernel/src/lib.rs`
  - `sed -n '1562,1575p' os/kernel/src/lib.rs`
- 证据：`arch_boot_finish` 顺序为 `validate_before_tables()` → `check_boot_postconditions()` → `store_kernel_info()` → `register_panic_diagnostic()` → `arch_boot_impl()`（后者开头调 `boot_validate_and_prepare`）。`arch_boot_resume_v1` 在 749-751 行 `validate_requiring_sources()` → `check_boot_postconditions()` → `resume_boot_pt_alloc()`，随后进入 `resume_additive`；`resume_additive` 在 307 行调 `boot_validate_and_prepare`，324 行才 `store_kernel_info`。v1 名字解码：`name_len = wire.name.iter().position(|&b| b==0).unwrap_or(wire.name.len())`，`wire.name` 是 `[u8;16]`，无 NUL 时取 16，`copy_from_slice(&wire.name[..16])`，确实没有 `min(15)`。`store_kernel_info` 开头断言 `(*KERNEL_INFO.get()).is_none()`，第二次调用 panic。
- 结论：CONFIRM。
- 反证失败说明：要推翻需在 v1 接收路径找到 `validate_before_tables` 调用或 store 早于 boot_validate 的调用点。`grep -n validate_before_tables` 仅命中 `arch_boot_finish`（223 行）；v1 路径没有它。

## CLAIM 14

- 主张位置：§3.2.1 删除行与字段计数、§3.2.2 GET_KINFO 收窄、§3.2.2.1 的 22 个字段表。
- 原话摘要：C 的 `kinfo_t` 有 22 个字段在 Rust 侧退场；`KernelInfo` 现有 10 个字段；`KinfoStruct` 是 48 字节，五个字段有 Rust userland 读者（`user_sp`/`nr_procs`/`nr_tasks`/`release`/`version`），`freepde_start` 恒填 0、`vir_kern_start` 有取值来源但仓内零读者。
- 反证路径/命令：
  - `sed -n '1,80p' minix3/minix/include/minix/param.h`
  - `sed -n '/pub struct KinfoStruct/,/^}/p' os/libs/minix-types/src/types/kinfo.rs`
  - `grep -rn --exclude-dir=target --include='*.rs' 'KinfoStruct' os/`
  - `grep -rn --exclude-dir=target --include='*.rs' 'vir_kern_start' os/`
  - `grep -n 'GET_KINFO' -A2 minix3/minix/kernel/system/do_getinfo.c`
- 证据：C `struct kinfo` 共 26 个成员；Rust 保留/改名的四个是 `memmap`、`module_list`→`boot_modules`、`param_buf`、`vir_kern_start`→`kern_virt_base`，其余 22 个与文档删除表逐项对应（`mbi`、`mem_high_phys`、`mods_with_kernel`、`kern_mod`、`kmessages`、`do_serial_debug`、`serial_debug_baud`、`minix_panicing`、`user_end`、`boot_procs[]`、`nr_procs`、`nr_tasks`、`release[]`、`version[]`、`vm_allocated_bytes`、`kernel_allocated_bytes`、`kernel_allocated_bytes_dynamic`、`mmap_size`、`freepde_start`、`bootstrap_start`、`bootstrap_len`、`user_sp`）。`KinfoStruct` 有 8 个成员（含 `_padding`），编译期断言 `size_of::<KinfoStruct>() == 48`，偏移 0/8/16/24/28/32/38 与文档一致。Rust 读者：`os/servers/rs/src/trap_api.rs` 读 `user_sp`；`os/servers/is/src/dump_kernel.rs:572-575` 读 `nr_procs`/`nr_tasks`/`release`/`version`。`vir_kern_start` 在非 test 代码中只由 `os/kernel/src/misc.rs:723` 取出写进 `KinfoStruct`，没有下游读者。C 侧 `do_getinfo.c:66-67` 为 `case GET_KINFO: length = sizeof(struct kinfo);`。
- 结论：CONFIRM。
- 反证失败说明：要推翻需找到第 23 个退场字段、`KinfoStruct` 不是 48 字节，或五个字段中某个没有读者。源码不支持；`vir_kern_start` 的唯一非 test 出现是写入端，符合“有取值来源但零读者”的表述。

## CLAIM 15

- 主张位置：§5.5「未闭环项交接」首句。
- 原话摘要：“当前没有未闭环的测试缺口。”
- 反证路径/命令：
  - `cd os && cargo test -p minix-kernel --features mock --lib`（末行给出 `8 ignored`）
  - 对 related-code.txt 全量检索 `TODO|FIXME|deferred|not yet`
  - `grep -n 'remains deferred\|deferred' os/kernel/src/lib.rs os/kernel/src/misc.rs`
- 证据：`minix-kernel --features mock` 实测 `866 passed; 0 failed; 8 ignored`；8 条 ignored 测试本身不等于“测试缺口”，但文档没有逐一说明它们为何可忽略。相关代码里存在多处 `deferred` 标注（`os/kernel/src/lib.rs:3923` 的 “Hardware half … remains deferred — see todo.md”、`:3513`、`:4469`，`os/kernel/src/misc.rs:15` 的 “D5: TRACE deferred”），它们多是功能债而不是测试债；未找到明确写着“本项无测试、待补”的条目。`os/boot-shim/src/ebs_map.rs:1893` 的 `TODO-07` 是测试函数上的编号注释，不是缺口声明。
- 结论：UNCLEAR。
- 反证失败说明：这是全称否定（“没有”），仅靠 grep 不能证明；同时 8 条 ignored 与多处 deferred 说明该句至少需要限定范围（例如“§5.1–§5.4 列出的四项已闭环”）。没有发现可直接反驳的测试缺口文件或注释，因此不判 REFUTE。

## VERDICT

- CLAIM 01–10：CONFIRM（10 条）。
- CLAIM 11：REFUTE（`minix-boot` 实测 188 passed / 0 failed，文档两处写 187/0；其余三个包计数与文档一致）。
- CLAIM 12–14：CONFIRM（3 条）。
- CLAIM 15：UNCLEAR（全称否定“没有未闭环测试缺口”证据不足；8 条 ignored 与多处 deferred 标注要求限定范围，但未找到直接反例）。
- 合计：13 CONFIRM / 1 REFUTE / 1 UNCLEAR。
- 最重一条：CLAIM 11 的 187 vs 188 是可复现的硬性数值错误，但它只影响 §5.0/§5.2 的证据读数，不影响交接语义结论。C 侧与 Rust 侧的核心主张（load_mods 次序、栈对齐、`mb_set_param` off-by-one、`alloc_lowest` 参数误用、VM 16 条截断、模块不归还、容量常量、门链次序）在本次核对中全部成立。
- 局限：未运行 QEMU 真机门（CLAIM 12 只核验脚本内容与路标定义，不核验当前是否六路标全过）；未做 C 侧反汇编（CLAIM 02 的栈对齐结论依赖链接器输入保序，文档已按“构建保证”限定）；未核对 `minix-boot` 新增测试的来历（只看当前树）。

