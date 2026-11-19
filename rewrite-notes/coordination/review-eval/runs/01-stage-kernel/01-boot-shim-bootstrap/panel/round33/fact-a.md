# P-FACT-A 报告 — 01-boot-shim-bootstrap.md 第 2 章 + §1 中引用 C 的断言

- **基线核对**：目标文档 sha256 = `14a27c145d070c480600d72d7276bef5f3d030e5248c4b758196651dca3a1d37`，与 R33 基线前缀 `14a27c145d070c48` 一致；行数 3565，一致。审查期间未修改任何仓库文件（含 `minix3/`）。
- **范围**：§2.1–§2.8 全部事实断言（代码引用逐块对读、行号逐一核对、行为推演逐条重放），以及 §1 中引用 C 的断言（§1.1 的 head.S/设栈论证、§1.2 的框表与 D1–D5 的 C 侧落点、§1.4 的五行栈顶表与命名陷阱）。
- **方法**：每条断言回到 `minix3/` 原始 C/汇编/链接脚本/构建文件取证；引用代码块做逐字符比对；行为性断言（删除扫描 off-by-one、get_value 首匹配、cut_memmap 余量落槽、第三行常态四前提、SysV 栈对齐算术、pg_identity 非缓存条件、ARM 闭区间相切条件等）按源码逐条重放推演。
- **总裁定**：第 2 章主体质量非常高——引文块（head.S、pre_init.c、pg_utils.c、memory.c、protect.c、earm 三文件、vm/pagetable.c、multiboot.h 注释）全部与源码逐字一致；行号引用（Makefile.inc 51/83、kernel.lds 18、head.S 93–96/86、exec.c 584/595/617/753/759-760、arch_smp.c:76、arch_system.c:246/277-278、main.c 128-129/142/147/301/311、protect.c 33/360-363/381/449-454、memory.c 35/189/707-717、do_vmctl.c:164、pg_utils.c:19/270-302、pm/main.c:169、vm/main.c 482-520、utility.c 注释）全部命中；§2.6.5 常量表（含 vm.h `I386_CR0_WP` 注释复制错误）全部核实。发现 3 处缺陷（P2×1、P3×2）+ 1 条 SUGGESTION，无 P0/P1。

---

## FINDING RD33-FACT-A-01

- **严重度**：P2
- **位置**：§2.3，两处。
  - 行 365：「（`arch=` 无读者，不在遮蔽对象内）」
  - 行 465：「循环还在时，当命令行里的 `board=` 不是 `param_buf` 的第一条记录（它前面还有别的键），同样的双记录与同样的改写后果仍会出现（`arch=` 也会留下双记录，但**仓内没有它的读者**）。」
- **源码证据**：`minix3/minix/drivers/storage/ramdisk/rc:9`——`if [ X`+`sysenv arch`+` = Xi386 ]`（该 rc 的整个 i386 驱动分支都以此判定为前提）；`minix3/minix/commands/sysenv/sysenv.c:59`（`svrctl(PMGETPARAM, &sysgetenv)`，`sysenv arch` 正是按 key 查询）；读取链终点是 `kinfo.param_buf` 里 `mb_set_param(cbi->param_buf, ARCHVARNAME, …)` 写下的记录（`minix3/minix/kernel/arch/i386/pre_init.c:149`，`ARCHVARNAME` = `"arch"`，`minix3/minix/include/minix/const.h:138`）。PM 侧缓存链：`minix3/minix/kernel/system/do_getinfo.c:143-145`（GET_MONPARAMS 整块拷 `kinfo.param_buf`）← `minix3/minix/servers/pm/main.c:169`。
- **为什么错**：文档自己在同一节为 `rootdevname` 完整交代了「读者全在用户态」的落链（sysenv → PM 缓存 → ramdisk rc），说明本篇的「读者」一词在落链语境下覆盖用户态；但轮到 `arch=` 时，行 465 用「**仓内**没有它的读者」做了 repo 全域断言，行 365 的「无读者」也未限定内核范围（行 457 的「`arch=` 只有写点、没有读点」紧跟在 `kernel/main.c` 的 `get_value`/`env_get` 语境里，是内核域断言，成立）。事实是仓内存在明确读者：ramdisk rc 用 `sysenv arch` 决定是否走整段 i386 驱动启动分支。 consequently，`arch=` 双记录 + 外部同名键遮蔽并非「无后果」：若命令行携带 `arch=<x>` 且它不是首条记录，`get_value`（内核）与 PM 缓存后的 `sysenv arch`（用户态，首匹配语义）都会先读到外部值——这正是本节为 `board=` 推演出的同一类后果，`arch=` 只是读者从 `kmain` 换成了 rc 脚本。
- **反证路径**：`rg -n "sysenv arch" minix3/minix/drivers/storage/ramdisk/rc` 命中第 9 行；`rg -n "PMGETPARAM" minix3/minix/commands/sysenv/sysenv.c` 命中第 59 行；对照文档 §2.3 落链图对 `rootdevname` 的同类交代（其读者判定就是 repo 域的）。若要维持原文，需把行 365/465 的限定词显式改为「内核内无读者」并补一句用户态 rc 有读点。
- **修复方向**：行 465 改为「（`arch=` 也会留下双记录；内核没有它的读者，但用户态 ramdisk `rc` 经 `sysenv arch` 读它——遮蔽后果落在 rc 的驱动选择分支，不在内核）」；行 365 括注同步收窄为内核域。

## FINDING RD33-FACT-A-02

- **严重度**：P3
- **位置**：§2.7 行 1246（earm/head.S 引文块内的省略注释）：「/* ……（**4 行标题注释省略，原文见 earm/head.S:20-27**）…… */」
- **源码证据**：`minix3/minix/kernel/arch/earm/head.S` 实际内容——第 21 行 `.section .startup`（引文已含）；第 22–24 行为三行横线标题注释（`/*==…==*/`、`/*	MINIX	*/`、`/*==…==*/`）；第 25 行 `.global MINIX`、第 26 行 `MINIX:`（均已被引文收录）、第 27 行 `/* this is the entry point for the MINIX kernel */`。
- **为什么错**：`.section .startup` 与 `.global MINIX` 之间实际省略的是 **3 行**（22–24），不是 4 行；且所引范围 `20-27` 含 4 个并未省略的行（21 `.section .startup`、25 `.global MINIX`、26 `MINIX:`、27 行注释也在引文中出现）。属于引文元信息（行数与出处区间）与源文件不符。
- **反证路径**：`sed -n '20,28p' minix3/minix/kernel/arch/earm/head.S` 逐行比对引文块。
- **修复方向**：改为「3 行标题注释省略，原文见 earm/head.S:22-24」。

## FINDING RD33-FACT-A-03

- **严重度**：P3
- **位置**：§2.4.2 行 782：「残差至少还混着三项：`arch_init` 的两条 BIOS 区 `cut_memmap`（**典型样本约 `0xF000` 字节**）、……」
- **源码证据**：`minix3/minix/kernel/arch/i386/arch_system.c:277-278`（`cut_memmap(&kinfo, BIOS_MEM_BEGIN, BIOS_MEM_END);` 与 `cut_memmap(&kinfo, BASE_MEM_TOP, UPPER_MEM_END);`）；`minix3/minix/include/arch/i386/include/memory.h:118-119`（`BIOS_MEM_BEGIN 0x00000`、`BIOS_MEM_END 0x004FF`）与 `:29/:33`（`BASE_MEM_TOP 0x090000`、`UPPER_MEM_END 0x0FFFFF`）；`minix3/minix/kernel/arch/i386/pg_utils.c:37-40`（cut_memmap 起点向下、终点向上取整）→ 两条切除取整后为 `[0, 0x1000)` 与 `[0x90000, 0x100000)`。粗档回退路径 `minix3/minix/kernel/arch/i386/pre_init.c:186`（`add_memmap(cbi, 0, mbi->mi_mem_lower*1024)`，GRUB/QEMU 典型 `mem_lower` = 639 → 低条目 `[0, 0x9FC00)`）。
- **为什么错**：按残差口径（E820 原始可用总量 − 最终 `memmap[]` 总量）重算两条 BIOS 切除从账上拿掉的量：`0x1000`（第一条，全在低条目内）+ 低条目被第二条切走的尾部 `0x9FC00 − 0x90000 = 0xFC00`，合计 **0x10C00**，不是「约 0xF000」。`0xF000` 只在两种情形下成立：低条目终点恰为 `0x9E000`（非典型），或只计第二条切除且低条目终点为 `0x9F000`（此时还漏了第一条的 0x1000）。该数字以「典型样本约」 hedge，但与典型布局（0x9FC00 低尾）不符，会让按此文档做残差对账的人系统性少扣约 0x1C00–0x2000 字节。
- **反证路径**：`sed -n '277,278p' minix3/minix/kernel/arch/i386/arch_system.c`；`grep -n "BIOS_MEM\|BASE_MEM_TOP\|UPPER_MEM_END" minix3/minix/include/arch/i386/include/memory.h`；按 `cut_memmap` 取整规则手算两条切除并对照 `mem_lower=639` 的典型样本；如需实证，可在 QEMU+GRUB 启动日志读 `mi_mem_lower`。
- **修复方向**：改为「（典型样本约 0x10C00 字节：`[0, 0x1000)` 全段 + 低条目 `[0x90000, 低尾)` 一段；随 `mi_mem_lower` 取值浮动）」，或直接给出两项分解式，不再给单一近似值。

## SUGGESTION S-01（非缺陷，措辞建议）

- **位置**：§2.7 行 1270：「……清帧指针一致（`%ebp` / `fp`），**调用 `pre_init` 与随后调用 `kmain` 的一致**。」
- **依据**：x86 侧 `head.S:87` 是 `call _C_LABEL(kmain)`（返回地址入栈，返回后落 `hang:`，`head.S:90-91`）；ARM 侧 `earm/head.S:45-46` 是 `ldr r2, =_C_LABEL(kmain)` + `bx r2`——尾跳（branch），不压返回地址、无返回语义。文档同段引文已如实展示 `bx`，但「调用……一致」的概括把 call 与尾跳混为同类；§2.7 末段「第 4 步的差别只在调用约定」也未覆盖这一点。
- **建议**：改为「随后进入 `kmain` 的方式不同：x86 是 `call`（返回后落 `hang:`），ARM 是 `bx` 尾跳（r0 携带 `kinfo`，不再返回）」。不构成事实错误，属表述精度。

---

## 附：本轮已核实无误的代表性断言抽样（供面板复核，非穷举）

- 引文逐字一致：`i386/head.S` 识别头块与 `multiboot_init`、`pre_init.c` 的 `get_parameters`/`mb_set_param`/`overlaps`/`pre_init`、`pg_utils.c` 的 `cut_memmap`/`add_memmap`/`alloc_lowest`/`pg_alloc_page`/`alloc_pagetable`/`pg_identity`/`pg_mapkernel`/`vm_enable_paging`/`pg_map` 入口、`memory.c` 的 `memory_init`/`createpde`/`lin_lin_copy` 断言行、`protect.c` 的 `arch_post_init`/`libexec_pg_alloc`/`arch_boot_proc` VM 段、`earm/head.S`/`setup_mbi`/ARM `overlaps` 宏、`vm/pagetable.c` BIGPAGE 跳过注释。
- 常量与值：`MULTIBOOT_HEADER_MAGIC 0x1BADB002`、`MULTIBOOT_INFO_MAGIC 0x2BADB002`、`HAS_VBE 0x4`、`HAS_ADDR 0x10000`、`HAS_MODS 0x8`、`HAS_MEMORY 0x1`、`HAS_MMAP 0x40`、`MULTIBOOT_MAX_MODS 20`、`MULTIBOOT_MEMORY_AVAILABLE 1`、`MAXMEMMAP 40`、`NR_MEMS 16`、`MULTIBOOT_PARAM_BUF_SIZE 1024`、`PG_PAGETABLES 6`、`PG_ALLOCATEME ((phys_bytes)-1)`、`GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS) = 6+1`（CONFIG_MAX_CPUS=1，`kernel/kernel.h:8`）、IDT 256×8=2048、§2.6.5 全表含 `I386_CR0_WP` 注释复制错误（`vm.h:47` 与 48 行注释逐字相同）。
- 行为重放成立：`mb_set_param` 删除扫描 off-by-one（第二条记录起匹配从第二字节开始、`xboard=2`→`xarch=i386` 伪记录推演、首条删除成立）；`get_value` 无 off-by-one 且首匹配；`add_memmap` 的 `len==0` 判定先于 `rounddown`（0<len<4096 写出零长度条目）；`cut_memmap` 余量落槽规则；`alloc_lowest` 第三参数是长度、四前提与「第三行常态」；`pg_identity` 非缓存条件方向与 0x2001000 示例；`pg_mapkernel` 起始下标 961、覆写推演；`vm_enable_paging` 清/置次序与 SMEP/SMAP 不触碰；boot2 空模块表仍建、标志仍置、`mi_mods_count > 0` 断言拦下；`assert` 走 libsys `__assert13` → panic（`kernel/Makefile` `LDADD+= -lsys`）→ `arch_shutdown` 屏幕刷诊断缓冲、等键复位、串口出口此时无声；内核无 `rootdevname` 读点；`objcopy --prefix-symbols` 与 mpx.S 手写前缀符号的命名陷阱链；跳板总量 2212 B（≈84 B 16 位码 + 24 B 数据槽 + 56 B GDT + 2048 B IDT）；ARM `_kern_phys_base 0x80200000` 与 `MB_MODS_BASE 0x82000000` 的 30MB 间隔、`0x1E00000` 相切条件、ARM 闭区间宏逐字核实。
