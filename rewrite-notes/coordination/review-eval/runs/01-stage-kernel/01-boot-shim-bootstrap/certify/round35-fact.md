# R35 certify-fact 独立回读回执

- 结论：PASS
- 核验条数：21
- CLOSED：21；STILL-BROKEN：0
- 时间：2026-11-19T21:32:41+08:00

> 所有命令均在仓库根 `/home/xzhao/github/minix-rs` 执行；`doc` 指 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`。只读核对，未改任何被核对文件。

## 1. §1 首段 boot2 侧 CR0.PG 零写点

- 判定：CLOSED
- 证据：
  - `awk 'NR>=150 && NR<=152 {printf "%d\t%s\n", NR, $0}' minix3/sys/arch/i386/stand/lib/realprot.S` → 150 `movl %cr0, %eax`、151 `or $CR0_PE, %ax`、152 `movl %eax, %cr0`，只置 PE。
  - `grep -rnw 'PG\|CR0_PG\|I386_CR0_PG' minix3/sys/arch/i386/stand/ | wc -l` → `0`。
  - `grep -rn 'mov.*cr0' minix3/sys/arch/i386/stand/` 的其余写点（`realprot.S:230-232`、`crt/dos/start_dos.S:213-215/268-270`）也只清/置 PE。
  - 与 `doc:9` 的「只置 `CR0.PE`……对 `CR0_PG` 零写点」一致。

## 2. §1.1 Multiboot 引文版本与取证口径

- 判定：CLOSED
- 证据：
  - `sed -n '32p' doc` → 含「Multiboot 规范（0.6.96；本仓无规范副本，按通行版本引）」，措辞明说无仓内副本，不再声称仓内有证据。
  - `find . -iname '*multiboot*' -not -path './os/target/*' -not -path './.git/*'` → 只有 `minix3/.../multiboot.h`、`minix3/.../multiboot.S` 等结构定义/汇编，无规范文档副本。
  - `grep -rn 'Machine state' --include='*' . | grep -v '^./minix3/' | grep -v '^./rewrite-notes/coordination'` → 仅本条引文自身，无第二处「仓内规范」来源。

## 3. §2.3 `unpaged` 段落的行号与内容

- 判定：CLOSED
- 证据：
  - `awk 'NR==237' minix3/minix/kernel/arch/i386/pre_init.c` → `return &kinfo;`；同文件 `sed -n '2p'` → `#define UNPAGED 1`。
  - `awk 'NR==51' minix3/minix/kernel/arch/i386/Makefile.inc` → `.for unpaged_obj in head.o pre_init.o direct_tty_utils.o \`，两个对象都在名单首行。
  - `awk 'NR>=84 && NR<=87 {printf "%d\t%s\n", NR, $0}' minix3/minix/kernel/arch/i386/head.S` → 84 `mov $k_initial_stktop, %esp`、85 `push $0`、86 `push %eax`、87 `call _C_LABEL(kmain)`。
  - `awk 'NR==17' minix3/minix/kernel/arch/i386/kernel.lds` → `.unpaged_text : { unpaged_*.o(.text) }`。
  - `awk 'NR>=128 && NR<=129 {printf "%d\t%s\n", NR, $0}' minix3/minix/kernel/main.c` → `memcpy(&kinfo, local_cbi, sizeof(kinfo));` 与 `memcpy(&kmess, kinfo.kmess, sizeof(kmess));`（paged 全局 `kinfo` 由 `minix3/minix/kernel/usermapped_data.c:7` 放进 `.usermapped`）。
  - 非阻断措辞观察：文中「这三条指令」实列四条（mov + push + push + call）；行号与内容均无误，仅计数措辞可再校。

## 4. §2.4 `kernel_may_alloc` 的两个同名定义

- 判定：CLOSED
- 证据：
  - `awk 'NR>=9 && NR<=12' minix3/minix/kernel/glo.h` → `#ifdef _TABLE` / `#undef EXTERN` / `#define EXTERN` / `#endif`；`awk 'NR==76'` → `EXTERN int kernel_may_alloc;`。
  - `awk 'NR>=30 && NR<=32 {printf "%d\t%s\n", NR, $0}' minix3/minix/kernel/table.c` → 30 `#define _TABLE`、32 `#include "kernel/kernel.h"`。
  - `awk 'NR==42' minix3/minix/kernel/kernel.h` → `#include "kernel/glo.h"`。
  - `awk 'NR==33' minix3/minix/kernel/arch/i386/pre_init.c` → `int kernel_may_alloc = 1;`（unpaged 副本；`pre_init.o` 只在 Makefile.inc:51 的 unpaged 名单里链接）。
  - 与 `doc:550` 的「`glo.h:76` / `glo.h:9-12` / `table.c:30-32` / `pre_init.c:33`」逐条一致。

## 5. §2.5 内核与模块重叠核查的输入

- 判定：CLOSED
- 证据：
  - `awk 'NR>=102 && NR<=103' minix3/minix/kernel/arch/i386/pre_init.c` → `kernbase = (phys_bytes) &_kern_phys_base` / `kernsize = (phys_bytes) &_kern_size`。
  - `awk 'NR>=112 && NR<=113'` → `cbi->user_sp = ... &_kern_vir_base;` / `cbi->vir_kern_start = ... &_kern_vir_base;`。
  - `awk 'NR>=162 && NR<=163'` → `kinfo.kernel_allocated_bytes = (phys_bytes) &_kern_size;` / `-= cbi->bootstrap_len;`。
  - `grep -n 'kernbase\|kernsize' minix3/minix/kernel/arch/i386/pre_init.c` → 仅 102/103 与 196/197；196-197 正是 `module_list[k].mod_start/mod_end` 的登记，随后 201-214 做 `overlaps` 与 `cut_memmap`。故「第一次作为内核与模块重叠核查的输入」成立。

## 6. §2.7 30MB 间隔的常量与算术

- 判定：CLOSED
- 证据：
  - `awk 'NR==182' minix3/minix/kernel/arch/earm/pre_init.c` → `#define MB_MODS_BASE  0x82000000`。
  - `awk 'NR==4' minix3/minix/kernel/arch/earm/kernel.lds` → `_kern_phys_base = 0x80200000;`。
  - `awk 'BEGIN{printf "%#x = %d MiB\n", 0x82000000-0x80200000, (0x82000000-0x80200000)/1048576}'` → `0x1e00000 = 30 MiB`，与 `doc:1330` 的 `0x1E00000` 一致。

## 7. §2.7 `earm/head.S:22-24` 是否为恰 3 行注释

- 判定：CLOSED
- 证据：`awk 'NR>=21 && NR<=26 {printf "%d\t%s\n", NR, $0}' minix3/minix/kernel/arch/earm/head.S` → 21 `.section .startup`；22 `/***...***/`；23 `/* MINIX */`；24 `/***...***/`；25 `.global MINIX`（非注释）；26 `MINIX:`。恰为 3 行注释，第 25 行不是注释。

## 8. §2.4.1.3 前提二的无锚点读数已删

- 判定：CLOSED
- 证据：
  - `grep -c 'E820 样本\|样本最低条目\|本仓核对过的 E820' doc` → `0`。
  - `sed -n '725p' doc` → 「……所以记录里的最低基址至少是 `0x1000`。论证只需要『非零且页对齐』这一条，不依赖任何具体读数……」——改为由 `cut_memmap` 与 `add_memmap` 对齐规则推出的结论。

## 9. §3.4.1 第 3 层 Special 的显式映射与五步判定

- 判定：CLOSED
- 证据：
  - `sed -n '1648p' doc` → 「现行实现把 Special 展开成两档落点：MMIO 类走 `DevicePath`，其余按属性与类型表走 `Side`……因此实现侧的完整落点是五类——可分配 / `ReservedMapped` / `Excluded` / `DevicePath` / `Side`，判定是五步（不是三条规则）」。
  - `awk 'NR>=2438 && NR<=2446' doc` 的 §4.2.2 表 → Step 1、Step 2、内联闸 `checked_shape`、Step 3、内联闸 `checked_len`、Step 4、Step 5，共五步加两道闸，与 §3.4.1 的「五步」交叉引用一致。

## 10. §3.4.1 `memmap` 行含 aarch64 直核腿

- 判定：CLOSED
- 证据：`sed -n '1633p' doc` → 「内核镜像在**两个直核自引导腿（riscv64 `bootface`、aarch64 `bootface_a64`）**与 U-Boot shim 腿的表内」。两个直核腿都被点名。

## 11. §4.1 256 页预算含 DM 窗口且与常量注释一致

- 判定：CLOSED
- 证据：
  - `sed -n '2295p' doc` → 「覆盖范围按 `bootface.rs` 的常量注释是『Sv39 root + identity low half + high half + two DM windows』——**DM 窗口也算在这 256 页里**」。
  - `awk 'NR>=127 && NR<=134 {printf "%d\t%s\n", NR, $0}' os/kernel-image/src/bootface.rs` → 注释「intermediate tables for Sv39 root + identity low half + high half + two DM windows measured below this」，且 `const PT_BUMP_PAGES: usize = 256;`。
  - `awk 'NR>=97 && NR<=100' os/kernel-image/src/bootface_a64.rs` → 另一条直核腿同样是 1 MiB / 256 页。

## 12. `set_startup_len` 为 `#[cfg(test)]`、生产提交口是 `push_startup`

- 判定：CLOSED（内容成立；该句实际在 §3.4.5，不在任务所写 §4.1）
- 证据：
  - `sed -n '1768p' doc` → 「startup 分区的生产提交口是 `push_startup`……而 `set_startup_len` 是 `#[cfg(test)]` 的测试专用入口，生产路径不可调用」。
  - `grep -c 'set_startup_len' doc` → `1`（只此一处）；`grep -c 'push_startup' doc` → `2`（§3.4.5 与 §4.3 的开关叙述）。
  - `awk 'NR>=292 && NR<=305 {printf "%d\t%s\n", NR, $0}' os/libs/minix-boot/src/boot_storage.rs` → 293 `#[cfg(test)]`、294 `pub(crate) fn set_startup_len`、305 `pub fn push_startup`。

## 13. §4.3 U-Boot loader 的生产者截断与过期英文注释

- 判定：CLOSED
- 证据：
  - `awk 'NR>=2608 && NR<=2613' doc` → 「超槽容量时生产者已截断到槽边界并打印 truncating」以及编注「源码同处的英文注释说 "The table stores the slot capacity, not the file length" 已与实现不符……该注释待同步」。
  - `awk 'NR>=362 && NR<=370' os/boot-shim/src/uboot_main.rs` → `slot = 2 MiB`、`used > slot` 时打印 `kernel ELF exceeds the 2 MiB staging slot — truncating`、随后 `used.min(slot)` 写入长度；`awk 'NR>=388 && NR<=397'` 对模块做同样处理（1 MiB 槽、`truncating`、`min(slot)`），循环继续。
  - `awk 'NR==206' os/boot-shim/src/opensbi_helpers.rs` → 英文注释原文 `// The table stores the slot capacity, not the file length (the shim`，确为编注所指的过期句。

## 14. §4.6 `MockPaging::map_huge` 与真实实现的 `AlreadyMapped` 分叉

- 判定：CLOSED
- 证据：
  - `sed -n '2933p' doc` → 「`MockPaging::map_huge` 走 `map_range`，在**任意已映射**的虚拟页上返回 `AlreadyMapped`，而三架构真实实现只在槽位已是 1 GiB 叶时返回」。
  - `awk 'NR>=922 && NR<=953' os/arch/src/arch/paging.rs` → mock 的 `map_huge` 末尾 `self.map_range(vaddr, paddr, pages, flags)`；`awk 'NR>=480 && NR<=490'` 的 `map_range` 预检 `if self.query(v).is_some() { return Err(PageTableError::AlreadyMapped); }`，对任意已映射页生效。
  - 真实实现：`os/arch/src/x86_64/paging.rs:1212-1216` 仅当 `e3 PRESENT && PS`（1 GiB 叶）返回 `AlreadyMapped`；`os/arch/src/riscv64/paging.rs:1174-1181` 仅当 `pte_is_leaf(e2)`（1 GiB 叶）；`os/arch/src/arm64/paging.rs:1184-1188` 仅当 `VALID && !TABLE`（1 GiB block）。分叉描述与源码一致。

## 15. §5.1 M6 只统计跑到 Step 1 之后的门链

- 判定：CLOSED
- 证据：
  - `sed -n '3151p' doc` → 「真机侧只有 x86 UEFI 门（到 `entering scheduler`）、riscv64 U-Boot shim 门（6/6 路标）与 aarch64 SMP AP 门跑到 Step 1 之后——aarch64 UEFI 门止于 EBS 后的 `memmap source`（在 `enter_kernel_image_v1` 之前），riscv 直核 `bootface` 门是故意的模块定位失败路径（在 `arch_boot` 之前），二者都不覆盖 Step 1」。
  - 脚本读数：`os/qemu-tests/test-shim-bootmarks-aarch64.sh:41-50` 的 MARK1/2/3 只到 `memmap source`；`os/qemu-tests/test-riscv64-uboot-shim.sh:205-208` 六个标记（含 `entering scheduler`）；`os/qemu-tests/test-shim-bootmarks-x86.sh:34` MARK5=`entering scheduler`；`os/qemu-tests/test-smp-aps-aarch64.sh:39-40` 判 `ap-arrived`/`ap-irq-ready`。
  - `grep -n 'module placement failed\|arch_boot' os/kernel-image/src/bootface.rs` → `place_modules` 失败点在 `:296`，`arch_boot` 调用在 `:442`，故障路径确在 `arch_boot` 之前。
  - M6 列的六个宿主测试名逐一存在：`grep -rl 'fn boot_identity_windows_are_2mib_and_mark_device_pieces' os/` 等六条均命中 `os/kernel/src/lib.rs`。

## 16. §5.1 M14 把 riscv64 U-Boot 直载测试链移出覆盖

- 判定：CLOSED
- 证据：
  - `sed -n '3159p' doc` → 「riscv64 U-Boot 直载测试链不在覆盖列——它的测试内核直构 `KernelInfo`、绕过 `prepare_boot`，只证明 U-Boot 的 `fatload`/`bootelf` 交棒透明，不经过 v1 握手」。
  - `sed -n '1,20p' os/qemu-tests/test-kernels/kernel/bootstrap/hello-boot-riscv64/src/main.rs` → 头部注释「We therefore construct `KernelInfo` directly with hardcoded values, bypassing the production boot-prep entry.」。
  - `grep -n 'hello-boot' os/qemu-tests/test-riscv64-uboot.sh` → 25 行默认内核为 `hello-boot-riscv64`，确为该测试内核。

## 17. §5.2.3 CI 接线句区分门链与命令冒烟链

- 判定：CLOSED
- 证据：
  - `sed -n '3301p' doc` → 「当前 CI 只接线了这五条门链里的 `test-riscv64-uboot.sh`，其余四条是手工门（另有独立的一套 `test-cmd-smoke*.sh` 命令冒烟链也在 CI……）」。
  - `grep -n 'run: bash' .github/workflows/qemu-tests.yml` → 只有 `qemu-tests/run_all.sh`（测试内核矩阵）与 `os/qemu-tests/test-riscv64-uboot.sh`；`grep -rn 'test-shim-bootmarks\|test-riscv64-uboot-shim\|test-kernel-image-riscv64-bootface' .github/workflows/` 无命中。
  - `grep -n 'test-cmd-smoke' .github/workflows/cmd-smoke.yml` → 55-57 三行，分别为 `test-cmd-smoke.sh`、`-aarch64.sh`、`-riscv64.sh`。

## 18. aarch64 跨距句补上 `aarch64.ld` 上界 ASSERT

- 判定：CLOSED（内容成立；该句实际在 §3.4.1:1653，§5.2.3 无此句）
- 证据：
  - `sed -n '1653p' doc` → 「aarch64 直核面目前只有 L0/L3 的间接基址对账与 `aarch64.ld` 的一条上界 `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000)`，跨距没有与 riscv64 L9 等价的等值对账」。
  - `awk 'NR==116' os/kernel-image/aarch64.ld` → `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000,`，与引文一致。
  - 备注：§5.2.3（`awk 'NR>=3285 && NR<=3305' doc | grep -n 'aarch64.ld\|ASSERT\|跨距'`）无此句；§4.4:2677 仍写「aarch64 跨距暂无等价断言」，该说法仍成立（上界不是等值断言），不构成矛盾。

## 19. §3.4.2 累计退场台账与「字段删除」裁决分开

- 判定：CLOSED
- 证据：
  - `sed -n '1657p' doc` → 「**先分清两个计数**：这 21 个是 C `kinfo_t` → Rust `KernelInfo` 的累计退场台账……而 §3.5/§3.9 与 §3.4.1 说的「字段删除」是一次具体裁决——从 `KernelInfo` 里删掉 `free_upper_idx`、`bootstrap_start`、`bootstrap_len` 三个，使字段数由 16 变成 13、`authority` 加入后为 14」。
  - `sed -n '1624p' doc | grep -o '`[a-zA-Z_][a-zA-Z0-9_]*\(\[\]\)\?`' | sort -u | wc -l` → `21`，与累计台账条数一致（行内重复出现的 `mmap_size`/`freepde_start` 是去向说明，不增名）。

## 20. §3.4 `KinfoStruct` 的 48 字节宽度加法

- 判定：CLOSED
- 证据：
  - `sed -n '1685p' doc` → 「布局：`freepde_start`/`user_sp`/`vir_kern_start` 三个 `i64` = 24 B，`nr_procs`/`nr_tasks` 两个 `i32` = 8 B，`release`/`version` 两个 `[u8; 6]` = 12 B，再加 4 B 对齐 padding，合计 48 B」。
  - `awk 'NR>=25 && NR<=44 {printf "%d\t%s\n", NR, $0}' os/libs/minix-types/src/types/kinfo.rs` → 三个 `i64`、两个 `i32`、两个 `[u8; 6]`、`pub _padding: [u8; 4]`；`awk 'NR==54'` → `assert_eq!(core::mem::size_of::<KinfoStruct>(), 48);`。24+8+12+4=48。

## 21. 引文头「未展开段」列表补 `entry_point` 断言

- 判定：CLOSED（内容成立；该引文头实际在 §4.2:2306，不在任务所写 §4.3）
- 证据：
  - `sed -n '2306p' doc` → 「未展开段：……入口与首段 VMA 的 `assert_eq!(layout.entry_point, layout.kern_virt_base.0, …)`」，其后紧跟 `fn prepare_and_enter_kernel_image` 引文。
  - `awk 'NR>=976 && NR<=978 {printf "%d\t%s\n", NR, $0}' os/boot-shim/src/uefi_helpers.rs` → `assert_eq!(` / `layout.entry_point, layout.kern_virt_base.0,` / `"boot-shim: ELF entry does not equal the first PT_LOAD VMA"`。
  - 备注：§4.3 的引文头（`sed -n '2507p' doc`）没有「未展开段」列表；全篇该列表只在 §4.2:2306 一处。

结论：回读验证通过，21 条逐条落证。
