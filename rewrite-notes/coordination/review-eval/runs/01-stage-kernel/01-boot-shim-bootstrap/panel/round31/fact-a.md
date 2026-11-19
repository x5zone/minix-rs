# P-FACT-A · R31 · `01-boot-shim-bootstrap.md` · 第 2 章 + §1 的 C 断言

- 目标文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（3554 行）
- 基线：sha256 `1601d667b0791defc0d751aeab89436451f5ee9d1ace736e0ff0d9746bfb050d`（与任务给定前缀 `1601d667b0791def` 一致）
- 只读范围：§1 中引用 C 的断言、§2 全章（含 §2.1–§2.8 与三个下游小节）
- 对照源码：`/home/xzhao/github/minix-rs/minix3/`（未修改）
- 判据：源码函数体、链接脚本、构建规则、调用序优先；源码注释只作线索

发现 8 条（P0×1 / P1×1 / P2×3 / P3×3），另附 3 条建议。

---

## FINDING FACT-A-01

**位置**：§2.6.4 第 1 段（文档 1159 行）

原文摘录：

> 这里只讲 C 的时序事实（Rust 侧后果在 §3.7 展开）：C 确实在分页已开时清活动根，它能撑过窗口靠的是"清表与回填之间只执行已被 4MB TLB 项覆盖的代码/栈，且 `pg_load` 在回填之后才重载 CR3"——这是时序脆弱，不是"无害"。**窗口里执行的对象不止一类**：`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load` 在 unpaged 低段；调用这些函数的 `prot_init`/`cstart` 代码在高半 `.text`；`k_initial_stktop` 栈在高半 `.bss`（判据在 `02-higher-half-kernel.md` §2.4；§1.4/§2.8 只引用结论）。所以"窗口能撑过"实际要求低段与高半两套 4MB TLB 项同时存在……

同段还有一句相关断言：

> `pre_init` 建完这张 `pagedir[]` 之后，`kmain → cstart → prot_init` 会用同样四步……把它**整体重建一次**……重建后的内容与 `pre_init` 那张等价

**源码证据**：

1. `pg_utils.c` 在构建里被编译成**两份**，不是一份：
   - `minix3/minix/kernel/arch/i386/Makefile.inc:51-54` 的 `.for unpaged_obj in head.o pre_init.o … pg_utils.o klib.o utility.o …` 把 `pg_utils.o` 列入 unpaged 名单；
   - `:56-71` 用 `objcopy --prefix-symbols=__k_unpaged_` 生成 `unpaged_pg_utils.o`（注释 `:48-50` 明说“must live in their own unique namespace”）；
   - `:75-76` 同时又把 `pg_utils.c` 放进 `SRCS += …`（生成未加前缀的 paged `pg_utils.o`）；
   - `:83` `OBJS.kernel+= ${UNPAGED_OBJS}`，两份对象都进入链接。
2. 两份各有自己的静态 `pagedir`：`minix3/minix/kernel/arch/i386/pg_utils.c:19` `static u32_t pagedir[1024] __aligned(4096);`。unpaged 副本落在 `.unpaged_bss`（`kernel.lds:19`，低地址），paged 副本落在高半 `.bss`（`kernel.lds:32`，`AT(ADDR(.bss) - _kern_offset)`）。
3. `objcopy --prefix-symbols` 会重命名符号表里的**全部**符号（含未定义引用），这正是 `kernel.lds:8-10` 要定义 `__k_unpaged__kern_offset` / `__k_unpaged__kern_vir_base` / `__k_unpaged__kern_phys_base` 这三个别名、以及 `proto.h:50` 要 `#define kmain __k_unpaged_kmain` 的原因。因此：
   - `pre_init.c`（`UNPAGED` 定义见 `pre_init.c:2`）里的 `pg_clear/pg_identity/pg_mapkernel/pg_load` 引用被改名为 `__k_unpaged_pg_*`，调到的是 **unpaged 副本**，`pg_load` 把低段那份 `pagedir` 的物理地址写进 CR3；
   - `protect.c` 的 `prot_init`（paged）里的同名调用是未加前缀的引用，只能解析到 **paged 副本**；证据是 paged `pg_utils.o` 的 `vir2phys` 引用必须解析到同属 paged 的 `protect.c:33-35`（该版本要减去 `_kern_vir_base - _kern_phys_base`），unpaged `pg_utils.o` 的 `vir2phys` 则解析到 `pre_init.c:24` 的恒等版本。
4. 关键调用点：`pre_init.c:230-234`（unpaged 四步）与 `protect.c:360-363`（paged 四步）。`protect.c:355-359` 的注释 “Set up a new post-relocate bootstrap pagetable … we no longer rely on pre-relocated data.” 与两份副本的事实一致。

**为什么错**：

- `prot_init` 的 `pg_clear()` 清的是 **paged `pg_utils.o` 在 高半 `.bss` 里的 `pagedir`**，而此刻 CR3 里的活动根是 **unpaged `pg_utils.o` 在低段 `.unpaged_bss` 里的 `pagedir`**。所以“C 确实在分页已开时清活动根”不成立：清的不是活动根，活动根也不在窗口里被清。
- 因为新根是在高半 `.bss` 里离线建好、最后由 `pg_load` 一次性换上，`prot_init` 这一段不存在“清表与回填之间靠旧 4MB TLB 项撑住”的窗口，“时序脆弱”这一结论失去事实基础（`prot_init` 里唯一的 `pg_load` 在最后，`protect.c:363`）。
- “`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load` 在 unpaged 低段”对 `prot_init` 的调用路径也不成立：这四个调用落在 paged `pg_utils.o` 的高半 `.text` 副本里；unpaged 低段副本只被 `pre_init` 使用。
- “`pre_init` 建完这张 `pagedir[]`……整体重建一次”把两个同名静态对象当成同一个；正确的说法是“`prot_init` 在 paged 副本里重建一张内容等价的新表，并把它装上 CR3”。

**严重度**：P0（P0-fact）。这是 §2.6.4 的核心机制断言，且被 §3.7 用来论证“Rust 侧禁止没有回填窗口地重建活动根”。

**反证路径**：

```bash
grep -n "pg_utils.c" minix3/minix/kernel/arch/i386/Makefile.inc
# 75: SRCS+= ... pg_utils.c
grep -n "prefix-symbols\|unpaged_obj in" minix3/minix/kernel/arch/i386/Makefile.inc
# 51: .for unpaged_obj in head.o pre_init.o ... pg_utils.o ...
# 67: ${OBJCOPY} --prefix-symbols=__k_unpaged_ ...
grep -n "unpaged_\*\.o\|^	\.bss\|^	\.text" minix3/minix/kernel/arch/i386/kernel.lds
grep -n "static u32_t pagedir" minix3/minix/kernel/arch/i386/pg_utils.c
grep -n "phys_bytes vir2phys" minix3/minix/kernel/arch/i386/protect.c minix3/minix/kernel/arch/i386/pre_init.c
```

若手上有 C 内核构建产物，可再核对符号表：`nm kernel | grep -E '(^| )(_?_k_unpaged_)?pagedir$'` 应同时出现 `pagedir` 与 `__k_unpaged_pagedir`。

---

## FINDING FACT-A-02

**位置**：§2.4.2 表「还引导段」行（文档 750 行）

原文摘录：

> C 把整段归还；但 `kinfo`（`pre_init.c` 的 `.bss`）、`pagedir[]` 与静态页表池在启动后仍有读者，归还的安全性依赖后续分配时序（VM 在这些读者完成前不重用该段），不是结构保证；与模块无关

（同一说法在 §3.5 文档 1806 行重复：“但段内的 `kinfo`、`pagedir[]` 与静态页表池在启动后仍有读者”。）

**源码证据**：

- 启动后真正被读的 `kinfo`/`kmessages` 是 `.usermapped` 里的副本：`minix3/minix/kernel/usermapped_data.c:7` `struct kinfo kinfo __section(".usermapped");`、`:9` `struct kmessages kmessages __section(".usermapped");`；`pre_init.c:20-21` 的 `kinfo`/`kmessages` 是 **unpaged 引导副本**（`objcopy` 后是 `__k_unpaged_kinfo` / `__k_unpaged_kmessages`）。
- `kmain` 一进门就把引导副本整体拷走：`minix3/minix/kernel/main.c:128-129` `memcpy(&kinfo, local_cbi, sizeof(kinfo)); memcpy(&kmess, kinfo.kmess, sizeof(kmess));`。归还发生在 `main.c:301`，晚于这次拷贝。
- 启动后活动的 `pagedir` / 静态页表池是 **paged `pg_utils.o`** 在 高半 `.bss` 的副本（见 FACT-A-01 的证据；`protect.c:370-378` 的 `pg_info` 交给 VM 的也是 paged 副本）。
- 归还的区间只覆盖 `kernel.lds:15-20` 的 `.unpaged_text/.unpaged_data/.unpaged_bss`，也就是 **unpaged 副本**所在的那一段，不含 paged `.bss`（`kernel.lds:32`）。
- paged 代码没有回到 unpaged 副本的调用路径：唯一把 paged 名字映射到 `__k_unpaged_` 名字的宏是 `proto.h:50` 的 `kmain`（= paged `main.c` 里的函数）；`pagedir`/`pg_*`/`kinfo` 都没有这种别名。

**为什么错**：

- 被归还的那一段里，`kinfo` 在 `kmain` 开头就已被拷进 `.usermapped`，`pagedir` 与静态页表池的活跃副本在 paged `.bss`（不在该段内），unpaged `pg_utils.o` 的静态池从未被调用（`pg_map` 唯一调用点是 paged `protect.c:381` 的 `libexec_pg_alloc`）。
- 因此“启动后仍有读者”对这三个对象都不成立；“归还的安全性依赖后续分配时序（VM 在这些读者完成前不重用该段）”也就没有对应的事实。
- 这不是说“归还一定绝对安全”没法再论证；缺陷在于文档给出的**理由与对象**都不对：它把 unpaged 副本当成了活跃对象，而活跃对象在另一段。

**严重度**：P1。

**反证路径**：

```bash
grep -n "struct kinfo kinfo\|struct kmessages kmessages" minix3/minix/kernel/usermapped_data.c minix3/minix/kernel/arch/i386/pre_init.c
grep -n "memcpy(&kinfo\|memcpy(&kmess\|add_memmap(&kinfo, kinfo.bootstrap" minix3/minix/kernel/main.c
grep -n "unpaged_text\|unpaged_data\|unpaged_bss\|^	\.bss" minix3/minix/kernel/arch/i386/kernel.lds
grep -rn "pg_map(" minix3/minix/kernel/arch/i386/   # 只有 protect.c:381 一个调用
```

---

## FINDING FACT-A-03

**位置**：§2.6.2「页表页从哪来」第 2 条（文档 1072 行）

原文摘录：

> 页表页的物理地址由 `vir2phys(ret)` 得到，而 `pre_init.c` 里 `vir2phys` 的定义是 `return (phys_bytes) addr;`——引导期恒等映射成立，所以"虚拟地址即物理地址"这句话在这个函数里是可执行的。

**源码证据**：

- `pg_utils.c:123-134` (`alloc_pagetable`) 在 `:132` 调 `vir2phys(ret)`。
- `pre_init.c:24` 是恒等版 `vir2phys`（unpaged 符号 `__k_unpaged_vir2phys`），但 `pg_map` 的唯一调用链是 **paged** 的：`main.c:257` `arch_boot_proc` → `protect.c:381` `pg_map(PG_ALLOCATEME, …)` → `pg_utils.c:298` `alloc_pagetable(&ph)` → `pg_utils.c:132 vir2phys`。
- 这条 paged 链上的 `vir2phys` 解析到 `protect.c:33-35`：
  ```c
  u32_t offset = (vir_bytes) &_kern_vir_base - (vir_bytes) &_kern_phys_base;
  return (phys_bytes)vir - offset;
  ```
  也就是**要减 `_kern_offset`**，不是恒等。
- unpaged `pg_utils.o` 的 `pg_map`/`alloc_pagetable` 没有任何调用者（`pre_init` 不调 `pg_map`），恒等版 `vir2phys` 在运行时路径上是死代码。

**为什么错**：

- 对 `alloc_pagetable` 实际被调用的那条链，“虚拟地址即物理地址”**不成立**；paged 副本的 `pagetables[][]` 在高半 `.bss`，必须减去 `_kern_offset` 才能得到物理地址，否则会把高半虚拟地址当物理地址写进页表项/交给后续使用者。
- 文档把 unpaged 副本（只有恒等版 `vir2phys` 才能工作）的解释套到了唯一的活路径上。

**严重度**：P2。

**反证路径**：

```bash
grep -rn "pg_map(" minix3/minix/kernel/arch/i386/          # 只有 protect.c:381
grep -n "vir2phys" minix3/minix/kernel/arch/i386/protect.c # 33-35 减 offset
grep -n "vir2phys" minix3/minix/kernel/arch/i386/pre_init.c # 24 恒等
grep -n "alloc_pagetable" minix3/minix/kernel/arch/i386/pg_utils.c
```

---

## FINDING FACT-A-04

**位置**：§2.3 解码 `unpaged` 的段落（文档 363 行）

原文摘录：

> `unpaged` 指的是链接在**低地址**、虚拟地址等于物理地址的那一小段引导期代码与数据——建页表与开分页这几步必须由它完成（否则刚开启分页的下一条取指，要去查一张还没建好的页表）。它不是"未分页的段"（那是 CPU 状态）；**这段代码在分页开启后不再执行**；C 仍在 `kmain` 后段把整段……归还给空闲清单

**源码证据**：

- `head.S:68-87` 的 `multiboot_init` 位于 `.text`，被 `objcopy` 成 `unpaged_head.o`、收进 `.unpaged_text`（`kernel.lds:17`，低地址）。
- `pre_init` 在 `pre_init.c:234` 调 `vm_enable_paging()` 之后，还要执行 `pre_init.c:237` 的 `return &kinfo;`；随后 `head.S:84-87` 继续执行 `mov $k_initial_stktop, %esp` / `push $0` / `push %eax` / `call kmain`。这些指令都在 `.unpaged_text`，都在分页开启之后、且都在低地址（靠恒等映射取指）。

**为什么错**：

- “这段代码在分页开启后不再执行”至少对 `pre_init` 的返回路径与 `head.S:84-87` 是假的。它们正是“设栈 + 跳高半”这一步，属于本篇 §2.2/§2.8 自己描述的对象。
- 文档下一节 §2.6.4 也在讨论“分页已开时低段代码执行”的窗口（不论该段结论是否成立，那里明确承认低段代码会在分页开启后运行），两处互相矛盾。

**严重度**：P2。

**反证路径**：

```bash
sed -n '68,92p' minix3/minix/kernel/arch/i386/head.S
sed -n '217,238p' minix3/minix/kernel/arch/i386/pre_init.c
grep -n "unpaged_text" minix3/minix/kernel/arch/i386/kernel.lds
grep -n "vm_enable_paging\|return &kinfo\|mov.*k_initial_stktop" minix3/minix/kernel/arch/i386/pre_init.c minix3/minix/kernel/arch/i386/head.S
```

---

## FINDING FACT-A-05

**位置**：§1 首段（文档 9 行）

原文摘录：

> 地址翻译这一步两条路不一样——UEFI 上的固件是开着分页进来的，而 BIOS 上的引导器（boot2 或 GRUB）在本仓的路径上**关着分页**就把控制权交出来（Multiboot 规范不强制这一状态；C 的 `vm_enable_paging` 仍先清 `PG`，是防御性写法）

**源码/文档证据**：

- 文档自己在 §1.1 表第 3 行（27 行）写的是：“C 假定分页未开（x86-32；**boot2 实际 `CR0.PG` 状态未证明**）”；§1.4 的边界声明（154 行）再次说 boot2 的交棒契约“不在本篇取证范围”。
- 引导器侧交接代码 `minix3/sys/arch/i386/stand/lib/multiboot.S:94-124`（`ENTRY(multiboot)` 到 `lret`）只重设栈、装段寄存器、压入口地址并 `lret`，**不写 CR0/CR3**；因此被引的这条路径不能证明“关着分页”。GRUB 一侧由 Multiboot 规范约束，boot2 一侧没有取证。

**为什么错**：

- 这是把 GRUB（Multiboot 规范约束）的形态写成了“boot2 或 GRUB”两条路的普遍结论；文档别处明确标了 boot2 未证明。首段是读者第一眼看到的口径，与 §1.1/§1.4 的边界声明不一致。

**严重度**：P2。

**反证路径**：

```bash
sed -n '94,124p' minix3/sys/arch/i386/stand/lib/multiboot.S   # 无 CR0/CR3 访问
sed -n '25,28p;150,156p' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
# 若需坐实 boot2：需审计 sys/arch/i386/stand/boot/ 全链的 CR0 写点；本篇未做
```

---

## FINDING FACT-A-06

**位置**：§2.5「三处具体事实」第 1 条（文档 852 行）

原文摘录：

> 这里 `_kern_phys_base`/`_kern_size` **第一次被用到**（`_kern_vir_base` 在 §2.3 已出现）：**内核要参与"谁占了哪段内存"的核查，就只能自己报出自己的地址范围**……

**源码证据**：

- 同一个函数 `get_parameters` 里，`_kern_size` 在更早的 `pre_init.c:162-163` 就被用来算 `kinfo.kernel_allocated_bytes`：
  ```c
  kinfo.kernel_allocated_bytes = (phys_bytes) &_kern_size;
  kinfo.kernel_allocated_bytes -= cbi->bootstrap_len;
  ```
- `_kern_phys_base` 在 `pre_init.c:102` 就被取址赋给 `kernbase`（`_kern_vir_base` 同段赋给另外两个字段），虽然“作为区间参与切除”确实到 196-197 行才发生。
- 文档自己在 §2.4.2（782 行）也写“`kernel_allocated_bytes` 是内核镜像自己那一块（`_kern_size` 减去 `bootstrap_len`）”，等于承认了更早的读点。

**为什么错**：

- “`_kern_size` 第一次被用到”与 `pre_init.c:162` 直接冲突；`_kern_phys_base` 的“第一次”（取址）也在 102 行。若作者想说的是“第一次作为内核/模块重叠核查的输入”，应改成这一限定，而不是“第一次被用到”。

**严重度**：P3。

**反证路径**：

```bash
sed -n '94,120p'  minix3/minix/kernel/arch/i386/pre_init.c   # kernbase 初始化
sed -n '159,166p' minix3/minix/kernel/arch/i386/pre_init.c   # kernel_allocated_bytes
sed -n '194,199p' minix3/minix/kernel/arch/i386/pre_init.c   # 内核当模块
```

---

## FINDING FACT-A-07

**位置**：§2.3「同一个 off-by-one 的第二种表现」条目（文档 464 行）

原文摘录：

> 例如 `param_buf` 里先有 `xboard=2`，内核再写 `board=` 时，中段 `board=2` 会被删掉，后一条 `arch=i386` 的前半截被补进原位，留下伪记录 `xarch=i386`。

**源码证据**：`pre_init.c:47-59` 的删除扫描：

```c
while (*p) {
    if (strncmp(p, name, namelen) == 0 && p[namelen] == '=') { … break; }
    while (*p++);
    p++;
}
```

- 扫描从 `p = bigbuf` 开始。若 `xboard=2` 是 `param_buf` 的**第一条**记录，`strncmp("xboard=2…", "board", 5) != 0`，不命中；随后 `while (*p++)` 越过本条 NUL 使 `p` 落在下一条记录首字节，再 `p++` 落在下一条记录第二字节——下一条是 `arch=i386` 时比较 `"rch=i386"` 也不命中。
- 因此“只有 xboard=2 一条 + 内核写 arch 和 board”这个最小复现**不会**发生删除，也不会产生 `xarch=i386`；缓冲区结果是 `xboard=2\0arch=i386\0board=intel\0\0`，`get_value("board")` 仍返回 `intel`。
- 该示例成立需要额外前提：`xboard=2` **不是第一条记录**（前面还有别的键）。此时扫描第二条记录时 `p` 落在其第二字节 `'b'`，`"board=2…"` 前缀命中，才会删掉中段并把后面的 `arch=i386` 前移成 `xarch=i386`。

**为什么错**：

- 示例漏写了“`xboard=2` 必须是第二条或更后的记录”这一前提；按字面最小化复现（`param_buf` 只有 `xboard=2`）得到相反结果。上文 419 行已解释“从第二条记录起”才会错位，但本示例没有把这层前提带下来，读者照抄会复现不出来。

**严重度**：P3。

**反证路径**：

```bash
sed -n '47,59p' minix3/minix/kernel/arch/i386/pre_init.c
sed -n '61,67p' minix3/minix/kernel/main.c   # get_value 从头扫描、首个匹配
# 最小复现：param_buf 仅 "xboard=2"，再写 board=intel → 无删除，无 xarch
```

---

## FINDING FACT-A-08

**位置**：§2.4「`kernel_may_alloc`」条目（文档 550 行）

原文摘录：

> `kernel_may_alloc` 是全局开关（**定义在 `i386/pre_init.c`，初值 1**；声明在 `kernel/glo.h`）。它分的不是"引导期 / 非引导期"这种时段，而是**这份账归谁**：`kernel/main.c` 的 `kmain` 开头显式写一次 1……而 `kernel/main.c` 的 `bsp_finish_booting`……把它写成 0……跨过这条线再想改账，撞上的就是上面那几条 `assert(kernel_may_alloc)`

**源码证据**：

- `pre_init.c:33` 确实有 `int kernel_may_alloc = 1;`（unpaged，符号为 `__k_unpaged_kernel_may_alloc`）。
- 但 `kernel/glo.h:76` 是 `EXTERN int kernel_may_alloc;`，而 `table.c:30-32` 先 `#define _TABLE` 再 `#include "kernel/kernel.h"`（`kernel.h:42` 又包含 `glo.h`），于是 `EXTERN` 展开为空，`table.c` **另外定义**了一个 `int kernel_may_alloc;`（paged，高半 `.bss`，初值 0）。
- `kmain` 的 `kernel_may_alloc = 1`（`main.c:142`）与 `bsp_finish_booting` 的 `= 0`（`main.c:105`）都在 paged `main.c` 里，写的是 `table.c` 的那一份；paged `add_memmap`/`cut_memmap` 的 `assert(kernel_may_alloc)` 读的也是这一份。unpaged `pg_utils.o` 的 `add_memmap`/`cut_memmap` 读的是 `pre_init.c` 的那一份，而它始终保持 1。

**为什么错**：

- “一个全局、由 `kmain`/`bsp_finish_booting` 按时段切换”的模型与符号事实不符：存在两个同名定义，切的是 paged 那一份；`pre_init.c` 那一份在 `kmain` 之后不再被赋 0（也没有调用者再检查它）。
- 结论“过了那条线再改账会撞 assert”对 paged 调用链仍然成立，所以这不是行为层错误，而是**定义位置与对象指认错误**（回查 `grep` 时会在两个文件同时命中，按文档会误判）。

**严重度**：P3。

**反证路径**：

```bash
grep -n "kernel_may_alloc" minix3/minix/kernel/glo.h minix3/minix/kernel/arch/i386/pre_init.c minix3/minix/kernel/table.c minix3/minix/kernel/main.c
sed -n '29,34p' minix3/minix/kernel/table.c      # #define _TABLE + include kernel.h
grep -n "EXTERN" minix3/minix/kernel/glo.h | head
```

---

## SUGGESTION FACT-A-09

**位置**：§2.4.1.4（文档 740 行）

原文：`vm/utility.c` 的注释原文是 "Any memory chunk in excess of NR_MEMS is silently ignored"

实际 `minix3/minix/servers/vm/utility.c:58` 是：

```c
/* XXX Any memory chunk in excess of NR_MEMS is silently ignored. */
```

既然写作“注释原文”，建议补上 `XXX ` 前缀与句末句号，或改成“大意是”。不改变结论。

---

## SUGGESTION FACT-A-10

**位置**：§2.6.2（文档 1090 行）

原文：依据是 `pg_utils.c:270-302` 的 `static int mapped_pde = -1; static u32_t *pt;`

实际 `minix3/minix/kernel/arch/i386/pg_utils.c:271` 是 `static u32_t *pt = NULL;`。建议补上 `= NULL`，否则与“依据是……原文”的口径不符。

---

## SUGGESTION FACT-A-11

**位置**：§2.4.1.3（文档 712 行）

原文：“取整真正改变的只有一种情形：**退化请求**（`start == end`）且首尾地址未落在页边界上……其余情形……取不取整**结果相同**”

在“最终哪些页离开空闲清单”这个意义上，该结论成立（我按 `add_memmap` 的 `roundup(addr)/rounddown(len)` 逐例复算过左/右余量）。但若把数组记账也算“结果”，不取整时补回的余量可能小于一页，`add_memmap` 会写出零长度条目并可能推高 `mmap_size`（`pg_utils.c:98-111`），极端情况下会撞后续 `assert(cbi->mmap_size < MAXMEMMAP)`。建议把“结果相同”限定为“释放出的页集合相同”，避免读者把记账副作用也算进去。

---

## 已核对且未发现事实错误的断言（覆盖说明）

以下项目逐条对照源码核对，未发现问题（按文档节序）：

- §2.1：Multiboot 头字段顺序与三个核心字段（`head.S:43-66`、`multiboot.h:40-44`）；`WANT_MEMORY=2`/`MODS_ALIGNED=1`/`HAS_VBE=4`/`HAS_ADDR=0x10000`（`multiboot.h:41-44`）；五个 `.long 0` 的语义（`multiboot.h:53-68`）；`MULTIBOOT_HEADER_MAGIC=0x1BADB002`、`MULTIBOOT_INFO_MAGIC=0x2BADB002`（`:40,86`）；`HAS_MMAP=0x40`/`HAS_MEMORY=0x01`/`HAS_MODS=0x08`（`:87-93`）；`MULTIBOOT_MAX_MODS=20`（`:244`）；`boot.cfg.default:4` 的 menu 行；`boot2.c:146,151,153` 的命令表；`exec_multiboot` 758-760 行、`module_init` 无条件建表（`exec.c:742-762`、`:500-620`）；`load_mods` 只登记路径（`boot2.c:453-458`、`exec.c:163-200`）；`libsys/assert.c:12-22` panic 与 `lib/libc/gen/assert.c:50-60` abort；`rootdevname` 的 `update_bootcfg.sh:96,99`、sysenv `sysenv.c:59`、PM `main.c:169`、`do_getinfo.c:143-147`、ramdisk `rc:56-60`、`mib/kern.c:96-115,392`。
- §2.2：`multiboot_init` 指令序（`head.S:68-77`）；`load_stack` 在 `.data` 末尾 4096 字节（`head.S:93-96`）；对象列表首位、`OBJS.kernel += ${UNPAGED_OBJS}`、`kernel.lds:18` 三环（`Makefile.inc:51-83`）；`load_stack_start` 4096 对齐与 `%esp ≡ 0` 的算术；`push $0/popf` 净零；两条 push 对调后的断言路径；`LOAD_KERNEL = LOAD_ALL & ~LOAD_TEXTA` 与 `LOAD_BSS`（`loadfile_machdep.h:35`、`loadfile.h:47-53`、`exec.c:300`、`loadfile_elf32.c:360-368`）；`k_initial_stktop` 的 `__k_unpaged_` 命名陷阱（`mpx.S:634`、`head.S:84`）。
- §2.3：`get_parameters` 引文（`pre_init.c:94-150`）；`kmess` 宏（`glo.h:35`）；`vir2phys`（`pre_init.c:24`）；`mb_set_param` 全文与三处忽略返回值的调用（`pre_init.c:35-75,144,149,150`）；`param_buf` 双 NUL 收尾；`get_value` 首个匹配（`main.c:481-500`）；`arch=` 只有写点（`grep ARCHVARNAME` 仅 pre_init.c）。
- §2.4：`memmap` 两档来源（`pre_init.c:173-188`）；E820 查询（`biosmemx.S:116-117`）；`mm_size+4` 步长（`multiboot.h:208-210`）；`add_memmap`/`cut_memmap` 全文（`pg_utils.c:32-121`）；`MAXMEMMAP=40`（`param.h:13`）；`LIMIT` 两次判断；`roundup/rounddown` 方向；零长度条目的两类来源；`cut_memmap` 手算（含 0x300000/0xBFE000/0x2000 校验）；`alloc_lowest` 唯一调用点（`arch_smp.c:76`）；`arch_init` 两条 `cut_memmap`（`arch_system.c:277-278`，`memory.h:29,33,118-119`）；`kmain` 调 `cstart`/`system_init`/`add_memmap` 的时序（`main.c:147,295,301`）；`kernel_may_alloc` 写 0 的位置（`main.c:105-107`）；`arch_boot_proc` 的 VM 回收（`protect.c:445-455`）；VM 的 `NR_MEMS=16` 截断（`const.h:98`、`vm/utility.c:58-59`）；VM 侧 `mem_add_total_pages`/`free_mem`（`vm/main.c:482-519`）。
- §2.5：`overlaps` 全文（`pre_init.c:77-92`）；`mmo_end` 语义（`exec.c:753`）；boot2 的 `image_end` 推进（`exec.c:584,595,617`）；内核当第 M+1 个模块（`pre_init.c:194-214`）；去掉 `-1` 的相切误报推演（A=[0x100000,0x200000)、B=[0x200000,0x300000)）；`overlaps` 去掉后账本不变的推演。
- §2.6：`pre_init` 八条语句（`pre_init.c:217-238`）；`pg_identity`/`pg_mapkernel`/`vm_enable_paging` 全文与位值（`pg_utils.c:162-245`）；`_kern_vir_base=0xF0400000`/`_kern_phys_base=0x00400000`（`kernel.lds:4-5`）；961 下标与 `0xF0000000` 偏移；`alloc_pagetable` 静态池 6 页（`pg_utils.c:123-134`）；`pg_map` 入口断言与 `mapped_pde < pde` 换表（`pg_utils.c:270-302`）；`PG_ALLOCATEME=((phys_bytes)-1)`（`archconst.h:151`）；`memory_init` 与 `createpde` 全文（`memory.c:69-129,707-718`）；`freepdes` 0/1 号槽（`memory.c:189-190,556`）；`arch_post_init`（`protect.c:370-378`）；`pt_init` 跳过 BIGPAGE（`vm/pagetable.c:1282-1286`）；`protect.c:360-363` 四步与注释；VM 的 `VMCTL_CLEARMAPCACHE`→`mem_clear_mapcache`（`memory.c:35`、`do_vmctl.c:164`）；`vm.h` 全部位/掩码/CR0/CR4 值与 `I386_CR0_WP` 注释复制错误（`vm.h:11-63`）。
- §2.7：`earm/head.S:21-55` 入口与 `load_stack`；`setup_mbi` 全文（`earm/pre_init.c:190-216`）；`overlaps` 闭区间宏（`:162-178`）；`MB_MODS_ALIGN=0x800000`、`MB_MODS_BASE=0x82000000`（`:181-185`）；`pre_init(argc, argv)`（`:374`）；`dcache_clean` 在开 MMU 前（`:404`）；ARM `pg_load` 写 TTBR0、`vm_enable_paging` 写 TTBCR/DACR/SCTLR（`earm/pg_utils.c:205-253`）；`earm/kernel.lds:4` `_kern_phys_base=0x80200000`，与 0 号模块间隔 `0x1E00000`（30 MiB）及“恰好 0x1E00000 时相切被误报”的推演。

**备注**：FACT-A-02 的同一说法也出现在 §3.5（文档 1806 行）；该处不在本次 §1/§2 范围内，但属同一缺陷，修复时应一并处理。
