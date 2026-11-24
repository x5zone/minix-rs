## 2. C 源码分析

### 2.0 本章导读

#### 问题入口

C 侧没有“交接协议”这个类型，却要用另一套形状完成同一件事：在引导器退出前把可用内存图、内核镜像位置与交接结构变成内核能读的记录。C 的坏法换一种形式：内核按引导器给的地址去读内存图，读到的却是自己地址空间里一段无关内容——不报错，只有错的数据。

C 的做法一句话概括：先用 `get_parameters` 把引导器的结构体整份拷进自己的 `kinfo`，再用 `add_memmap`/`cut_memmap` 把外来区间裁成内存账，并在每条记录切除前用半开区间检查替模块与内核的字节把关。命令行子链只有一条：`boot.cfg` → boot2 → `head.S` → `pre_init` → `get_parameters` → `mb_set_param`；完整调用链（含 `add_memmap`/`cut_memmap`/`overlaps`）见 §2.8。每一段只回答两个问题：C 在这里保证了什么，边界在哪。

#### 前置概念

第 1 章的三类数据与 I1–I4。本章会用到 `struct kinfo`、`add_memmap`/`cut_memmap` 等 C 名字，读到即学，不要求提前掌握。本章的判断只有一句：C 的交接面是结构体整份拷贝、三类载荷落盘加一条区间检查；后面章节的类型与协议，都是把这句判断写进类型系统。

三类数据在 C 侧各有一个落点：

- 可用内存图 → `kinfo.memmap[]`。
- 镜像位置 → `_kern_phys_base`/`_kern_size`（链接布局）；`kinfo.bootstrap_*` 只是其中的 unpaged 段。
- 交接结构 → `kinfo.mbi`。

地址空间的建立不在这条链上，属于 [02 篇（镜像篇）](02-higher-half-kernel.md)。

本章路线：交接契约与入口 §2.1–§2.2；参数落表 §2.3；内存账与取址 §2.4；模块重叠与三层检查 §2.5–§2.6；ARM 对照 §2.7；归纳与 D1–D5 证据归属 §2.8；章末失败对照、最小示例、小结与术语。

### 2.1 引导器契约与启动信息

引导器与内核之间靠三样东西对接：内核镜像里的识别头、跳转时的寄存器约定、一份由引导器填好的结构体。识别头嵌在内核二进制里，按 Multiboot 规范由引导器扫镜像前 8KB 找到它才认这个内核（[minix3/minix/kernel/arch/i386/head.S:multiboot_magic](minix3/minix/kernel/arch/i386/head.S)）。

- 协议只要求识别头落在镜像开头一小段里，因为这个头必须在引导器还不知道镜像布局的时候就能被找到；`.balign 8` 是比协议本身更严的对齐选择。
- magic、flags、checksum 三个字段满足 `magic + flags + checksum == 0` 这一条加法恒等式——它不是校验和算法，是一次加法恒等式；三个字相加为零就是“这个头部完整”的证明。
- flags 只置了两位：`WANT_MEMORY`（请把基本内存大小也告诉我）与 `MODS_ALIGNED`（模块按页对齐）。**没有置视频位**，所以同一段里的 EGA/80/25 四个视频字段按结构体注释应当被引导器忽略。`WANT_MEMORY` 让粗档（细表之外的备用来源，定义见 §2.4.1）有数可用；`MODS_ALIGNED` 让模块按页对齐，重叠检查（§2.5）的页粒度假设才有依据。
- 紧跟的五个 `.long 0` 是 `header_addr`、`load_addr`、`load_end_addr`、`bss_end_addr`、`entry_addr`；五个字段同受 `HAS_ADDR` 一位管，而 flags 没置它，所以五个都不被读。这五个 0 是一个选择：内核不在识别头里指定地址；**装入位置仍由 ELF 头承载的链接布局决定**。
- 本仓这条路径上跑的是 MINIX 自带的 boot2，它按 ELF 头装载，不读识别头；识别头是给 GRUB 一类合规引导器准备的。

跳转时两个寄存器分工：`%eax` 放 info 魔术值，`%ebx` 放 `multiboot_info` 地址；两个魔术值只差最高半字节（`0x1` 与 `0x2`），方向相反：

| 魔术值 | 值 | 方向 | 谁读它 |
|---|---|---|---|
| `MULTIBOOT_HEADER_MAGIC` | `0x1BADB002` | 内核 → 引导器 | Multiboot 合规引导器扫镜像前 8KB（boot2 不读） |
| `MULTIBOOT_INFO_MAGIC` | `0x2BADB002` | 引导器 → 内核 | `pre_init()` 的第一条断言 |

配置文件里的每条指令各把一份信息送到这个交接面上。解释 `menu=`/`load_mods`/`multiboot` 这套指令语言的是 MINIX 自带的第二级引导（[minix3/sys/arch/i386/stand/boot/boot2.c:command_load_mods](minix3/sys/arch/i386/stand/boot/boot2.c) 是其中一条的实现）：

```
menu=Start MINIX 3:load_mods /boot/minix_default/mod*;multiboot /boot/minix_default/kernel rootdevname=$rootdevname $args
```

一条 `menu=` 行里，分号前是 `load_mods`，分号后是 `multiboot`；顺序就是契约本身。两条指令加一个参数各送什么：

| 指令 | 送到哪里 | 内核哪一段读它 |
|---|---|---|
| `multiboot <内核路径> [命令行]` | 镜像被读进物理内存后跳到入口；内核路径与参数拼成一串成为 `mbi->mi_cmdline`（`mbi` 是引导器结构体副本，§2.3 详述；[minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot](minix3/sys/arch/i386/stand/lib/exec.c)） | 命令行由 `get_parameters()` 解析；镜像地址不从引导器读，来自链接期常量 |
| `load_mods <模块通配路径>` | 只把每个匹配文件的**路径**挂进引导器自己的模块链表，不读内容、不分配地址；真正的读取与地址登记发生在 `multiboot` 执行时的 `module_init` | `get_parameters()` 把整个数组拷进 `kinfo.module_list[]` |
| `rootdevname=<设备名>` | 命令行里的一个 `key=value` 词，随整串进 `mi_cmdline` | `get_parameters()` 逐词交给 `mb_set_param()`，落进 `kinfo.param_buf` |

两条指令加一个参数有一处不对称：只有 `rootdevname=` 的内容是内核能直接看见的，另两条的产物是一个地址或一段内存，内容要看引导器填的结构体。`kinfo` 是 `pre_init` 填、`kmain` 读的全局记录；`param_buf`（NUL 分隔的 `key=value` 串，格式见本节末）与 `module_list` 是它的两个字段（§2.3、术语）。这里说的“模块”是内核随后要逐个启动的那些服务进程的镜像（名单与顺序见第 4 章）。

`rootdevname` 是根文件系统的设备名，写时不带 `/dev/` 前缀（值形如 `c0d0p0`）：装机脚本 `update_bootcfg.sh` 把 `$ROOT` 过一道 `sed 's/\/dev\///'` 剥掉前缀，再以 `rootdevname=` 写进 `boot.cfg.default`；该值在 setup 期间会变，所以配置里只写变量 `$rootdevname`。它之所以走命令行，是因为 Multiboot 结构体里没有对应字段；它只能作为一个通用 `key=value` 搭乘。

内核自己不解释这个参数——`grep -rn rootdevname minix3/minix/kernel/` 无命中——它只是把它搬进 `param_buf`；需要“根设备是谁”的是用户态的两个读者：RAMDS 的启动脚本与 mib 服务。这条分工就是微内核分层的直接体现：**对纯用户态参数，内核只当搬运工，语义留给用户态**。两个有专用字段的键（`cttyline`/`cttybaud`）与有内核读点的 `board=` 是例外，见 §2.3。

模块数有硬上限。`MULTIBOOT_MAX_MODS` 是 20（[`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)），`kinfo` 的 `module_list[]` 按这个数开数组（[minix3/minix/include/minix/param.h:struct kinfo](minix3/minix/include/minix/param.h)），内核用 `assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS)` 把它当硬约束：实际模块数最多 19；内核以第 M+1 个模块身份追加到第一个空槽（M=12 时下标 12）；到 20 不是截断，是当场停住。

**`load_mods` 必须写在 `multiboot` 前面。**

- `load_mods` 只登记文件名；告诉内核“有模块”的标志位由 `multiboot` 在装载内核时置上：`exec_multiboot` 按链表当时的内容建表，再置 `MULTIBOOT_INFO_HAS_MODS`、填 `mi_mods_count`/`mi_mods_addr`。链表被看见的时刻就是 `multiboot` 执行的时刻。
- 反序时表照样建、标志照样置（模块开关是配置级全局，默认开），所以查标志的 `assert(mi_flags & MULTIBOOT_INFO_HAS_MODS)` 会通过，真正停住内核的是 `assert(mbi->mi_mods_count > 0)`。
- 若模块表分配失败，`module_init` 提前返回、标志未置，停在前一条断言。**诊断入口按分支选**：`HAS_MODS` 未置说明模块表分配失败，查 `module_init`；`HAS_MODS` 已置而条数为 0，才查 `load_mods` 与 `multiboot` 的先后。

停机不是静默挂死：内核链接的是系统服务库的 assert（[`libsys/assert.c`](minix3/minix/lib/libsys/assert.c) 的 `__assert13`，打印后 panic），不是用户态那份（[`gen/assert.c`](minix3/lib/libc/gen/assert.c)，打印后 `abort()`）。

结论：引导器契约的实质是“三样东西、一处交接面”；内核不猜引导器的内存，凡是它要用的，都必须在 C 侧有一处按值收下的代码。

### 2.2 `head.S` 入口：`multiboot_init` 与 `load_stack_start → pre_init`

boot2 跳到 `MINIX` 标签后，一条 `jmp multiboot_init` 先跳过识别头：`MINIX` 是入口符号，识别头必须在镜像开头 8KB 内可被扫描，而 `multiboot_init` 的代码不必被扫到。[minix3/minix/kernel/arch/i386/head.S:multiboot_init](minix3/minix/kernel/arch/i386/head.S) 只做三件事：设栈、清帧指针与标志、按 C 调用约定把两个寄存器值变成两个参数。

```asm
// 节选，minix3/minix/kernel/arch/i386/head.S:multiboot_init。
multiboot_init:
	mov	$load_stack_start, %esp	/* make usable stack */
	mov	$0, %ebp
	push	$0	/* set flags to known good state */
	popf
	push	$0
	push	%ebx		/* info struct */
	push	%eax		/* magic */
	call	_C_LABEL(pre_init)
```

（节选，止于 `call pre_init`；行序与原文一致。）

- **设栈**：`load_stack` 是文件末尾 `.data` 段的 4096 字节静态空间，`load_stack_start` 是它的高端地址——取高端是因为栈向低地址生长。这是“能进 C”的全部原因：没有栈，`push`、`call` 与任何 C 函数都无从谈起。
- **对齐是构建保证**：`load_stack_start` 落在 4096 边界不是链接顺序的巧合。`head.o` 固定在 unpaged 对象列表首位（[Makefile.inc](minix3/minix/kernel/arch/i386/Makefile.inc)），其 `.data` 又是 `.unpaged_data ALIGN(4096)` 的第一个输入节（[`kernel.lds`](minix3/minix/kernel/arch/i386/kernel.lds)）；ARM32 入口用的是同一个 `load_stack_start`，同样落在 `.data` 末尾（[`earm/head.S`](minix3/minix/kernel/arch/earm/head.S)）。
- **清帧指针与标志**：`mov $0, %ebp` 终止栈回溯链（panic 打印调用栈与 GDB 远程调试都靠 `%ebp` 链逐帧走）；`push $0`/`popf` 把 EFLAGS 装成全零，净效果是栈深不变，关中断、清嵌套任务位与方向标志；`DF=0` 让随后的 `memcpy` 按地址递增方向走。
- **传参**：i386 的 C 调用约定下第一个参数落在最低地址，所以第二个参数（`multiboot_info` 地址）先压、第一个参数（魔术值）后压。`_C_LABEL(pre_init)` 是汇编宏，ELF 目标上直接展开为 C 符号名，非 ELF 目标上加 `_` 前缀；本仓是 ELF，可读作 `call pre_init`。

入口只能用汇编：设栈之前不能 `push`，而任何 C 函数的第一条指令就可能用栈；EFLAGS 与 `%ebp` 也只有在汇编里才能按需清零。`load_stack` 放在 `.data` 而不是 `.bss`，差别在这段字节的确定性由谁负责：`.data` 的 4096 个零字节是镜像文件自带的，装载时按普通内容拷进内存；`.bss` 没有这些字节，清零要靠装载器（[`libsa/loadfile_elf32.c`](minix3/sys/lib/libsa/loadfile_elf32.c) 在调用方给了 `LOAD_BSS` 位时才清）。这条选择省下的是对外部组件的依赖，不是一处崩溃；代价是内核镜像文件里多出这 4KB 零字节。

**栈对齐不满足 SysV。** 从 4096 对齐的栈顶算：`push $0; popf` 净零，第二个 `$0` 得 top−4，两个参数到 top−12，`call` 压返回地址后入口 `%esp ≡ 0 (mod 16)`。紧随的第二个 `$0` 不是第五个参数或保留槽，它只是一个用来挪动栈顶的空位；它把入口从 ≡4 变成 ≡0，却仍不满足 ABI。SysV 要求入口 `%esp + 4` 是 16 的倍数（即 `≡ 12`），达到它需要 20 字节，这段布局只压了 16 字节——比标准 ABI 少一次 push。这是风险提示而非设计选择：按当前 GCC 的默认假设，`pre_init` 很可能没有生成依赖栈对齐的 SSE 访问，换编译器或优化档可能改变；Rust 侧的对齐处理见第 3 章。

**两条 `push` 对调会怎样**：`pre_init(magic, ebx)` 收到 `magic = ebx`（`multiboot_info` 的物理地址，一个看起来完全合法的数字），`ebx` 收到 `0x2BADB002`。第一条断言当场失败，但这条 assert 只打印失败的表达式本身，不打印值——**不核对两条 `push` 的次序，容易往“数据结构坏了”的方向查**。

结论：入口的三件事（设栈、清状态、传参）都由汇编承担，因为它们都发生在“能调用 C”之前；`.data` 的选择省下的是对外部装载器的依赖，栈对齐则是原作留下的 ABI 缺口。

### 2.3 `get_parameters`：把别人的数据变成自己的记录

[minix3/minix/kernel/arch/i386/pre_init.c:get_parameters](minix3/minix/kernel/arch/i386/pre_init.c) 的第一件事是**整份拷贝**：不长期持有引导器给的指针，而是把整个结构体复制进自己的全局 `kinfo`。它的两个实参是 `ebx` 与 `pre_init` 传进来的 `&kinfo`。

```c
// 节选，minix3/minix/kernel/arch/i386/pre_init.c:get_parameters。
multiboot_info_t *mbi = &cbi->mbi;
memcpy((void *) mbi, (void *) ebx, sizeof(*mbi));
```

这里有个同名陷阱：`cbi` 是 `&kinfo` 的形参名，`mbi` 是局部指针，`kinfo` 的第一个字段也叫 `mbi`，声明行就是把前者指向后者。看到 `memcpy` 里光秃秃一个 `mbi` 就以为它是全局变量，是这一段最常见的误读。拷贝之后 `ebx` 指向的那份结构体不再被引用。

整份拷贝只是第一步；按值交付还要把结构体里的三类指针载荷各自落成内核自己的存储：

- `mbi` 里的 `mi_cmdline`、`mmap_addr`、`mods_addr` 仍是指向引导器内存的地址，只拷结构体仍然在解引用发送侧内存。
- 命令行拷进 `param_buf`；模块表按 `mi_mods_count` 拷进 `kinfo.module_list[]`；内存图逐条 `add_memmap` 成内核自己的条目。
- 这三类载荷都落完，I1（按值交付）才成立；只做第一步，命令行与模块表随时可能被引导器或后续装载覆盖。

同一段里五个字段分两类来源：

- `mem_high_phys = 0` 是“待累计”的初值，稍后由 `add_memmap` 推到最高可用地址。
- `user_sp`、`vir_kern_start`、`bootstrap_start`、`bootstrap_len` 没有一个来自引导器，全部取自链接期常量 `_kern_vir_base`、`_kern_unpaged_start`、`_kern_unpaged_end`。`vir_bytes` 类型上的 `&符号` 取的是符号的地址值——值在链接期就定了，运行时只是读一个常数。
- 道理是：**内核被装在哪儿、哪一段是分页前用的，在链接那一刻就定下了，内核不必问任何人**。这正是镜像位置不从内存图取值、反而要用自己的区间把内存图裁短的根据。要把“谁放”和“放哪”分开：按 ELF 头把镜像放到链接指定的物理地址的是引导器；地址值本身来自双方共读的链接结果，所以内核读自己的常量即可，不必再问。第 1 章把镜像位置列为三类数据之一；C 侧不通过交接结构传它，而由链接常量重建——这不是 I1 的例外，而是可重建的数据不必跨边界复制。
- `user_sp` 先赋 `_kern_vir_base` 只是占位，同一函数后段会被 `USR_STACKTOP` 覆盖。

`bootstrap_len` 是两个符号地址相减，得到的那一小段叫 unpaged：链接在**低地址**、虚拟地址等于物理地址，建页表与开分页必须由它完成。分页开启之后这段代码仍在低地址上执行一小段，靠的是恒等映射；`pre_init` 返回后 `head.S` 末段（换栈与 `call kmain`）也属于同一低段，而 `kmain` 等代码不在 unpaged 对象名单里，链接后落在高半。

命令行解析只在 `MULTIBOOT_INFO_HAS_CMDLINE` 置位时进行；没置位时 `param_buf` 里只有后面两条固定键。解析器是一个手写的逐词扫描状态机，不是 `strtok`：跳过空格、收集 `var` 到 `=`、再收集 `value` 到空格；`if (*p++ != '=') continue;` 跳过非 `key=value` 形式的词，`p++` 已推过一个字符，所以不会死循环。命令行缓冲按 `MULTIBOOT_PARAM_BUF_SIZE`（1024）开。命令行里还可能有 `bootopts=` 等其它词，解析器不关心键名，一律按 `key=value` 进 `param_buf`。

每个键值对都过一遍 `mb_set_param`；随后**无条件**再写 `arch=` 与 `board=`（值来自编译期板卡身份，i386 上固定查 `BOARD_ID_INTEL`）。这两行保证的是“`param_buf` 里至少各有一条内核写入的记录”，不是“内核无条件知道自己是什么板”：`arch=` 在内核侧只有写点、没有读点，但用户态 RAMDS 的 `rc` 会经 `sysenv arch` 读它；`board=` 有读点（[minix3/minix/kernel/main.c:get_value](minix3/minix/kernel/main.c)），当引导器传入同名键且该键不是第一条时，读取返回第一条匹配，外部输入可以遮蔽内核写入的值。

[minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param](minix3/minix/kernel/arch/i386/pre_init.c) 同时做三件事：

- **识别专用字段**：头两行 `strcmp` 是仅有的两个专用键，`SERVARNAME`/`SERBAUDVARNAME`（`"cttyline"`/`"cttybaud"`，[`minix/const.h`](minix3/minix/include/minix/const.h)），命中时同时设 `do_serial_debug`/`serial_debug_baud`——从哪个串口做内核调试是内核启动早期自己要做的事，不能等用户态去读参数。
- **维护“键不重复”**：命中已有同名键时，把该条字符串末尾之后的内容整段左移，覆盖掉旧条目。维护键唯一的目的只有一个：读参数的人返回第一条匹配，重复键会让先写的那条胜出。
- **维护可追加格式**：找末尾的循环走到“当前字节与下一字节都是 NUL”的位置，再把写入点挪到终止 NUL 之后；空缓冲时从头开始写。

“已存在先删”那段有一个真实的 off-by-one：

```c
// 示意，minix3/minix/kernel/arch/i386/pre_init.c:mb_set_param。
while (*p) {
	if (strncmp(p, name, namelen) == 0 && p[namelen] == '=') { /* 删除 */ }
	while (*p++) ;	/* 越过本条 NUL */
	p++;		/* 多走一步：扫描起点偏一字节 */
}
```

`while (*p++)` 越过本条记录的 NUL 时，后缀自增已让 `p` 落在下一条记录的第一个字节上，紧接着的独立 `p++` 又多走一步——从第二条记录起，匹配检查从记录的第二个字符开始，同名键再也匹配不上。于是“键不重复”只在待删键恰好是缓冲区第一条记录时可靠成立：

| 情形 | 扫描行为 | 后果 |
|---|---|---|
| 待删键是首条记录 | 起点正确，命中 | 删除生效，键不重复成立 |
| 待删键在第二条以后 | 起点偏一字节，前缀匹配落空 | 旧记录保留，同名键重复（`A=1\0B=2\0` 再写 `B=3` 得两条 `B`） |
| 中段命中（空键，或 `xboard=` 这类含 `board=` 子串的非首条） | 只按 `name=` 前缀匹配，不要求落在记录起点 | 从记录中部开始删，后续字节前移或被截断，留下伪记录 |
| 外部 `board=` 是首条 | 删除首条，后续记录前移；内核值追加在末尾 | 内核值胜出，仍正常 |

空键（`namelen == 0`）把同一缺陷推到极端：`strncmp(p, "", 0)` 恒真，判定只剩 `p[0] == '='`；若第二条记录形如 `B=2`，扫描落在它的 `=` 上，删除从记录中部开始、后续记录被截断。这是 C 源码的真实行为，本篇照实记录，不替它圆。

这段删除循环的存在意义可以用 `board=` 说清：

| 输入 | `param_buf` 里的记录 | `get_value("board")` 返回 |
|---|---|---|
| 无外部键 | 只有内核写的 `board=intel` | 内核值 |
| 外部 `board=bar` 非首条 | 两条都保留（删除扫描从第二条起匹配落空） | 第一条，外部值遮蔽内核值 |
| 外部 `board=bar` 是首条 | 循环删掉首条，再补回内核值 | 内核值 |
| 去掉删除循环，外部 `board=bar` 是首条 | 两条都保留 | 第一条，外部值遮蔽内核值 |

所以循环只在外部键恰好是首条时守住板卡身份；非首条时命令行仍能遮蔽内核值——这是上一张表第二行的直接后果。

`param_buf` 不是一条字符串，而是**一串以 NUL 分隔的 `key=value`，整串以一个额外 NUL 收尾**。容量检查用 `p + namelen + valuelen + 3 > bufend` 提前拒绝并返回 `-1`，但三处调用点全部忽略返回值（命令行解析一处、固定键 `arch`/`board` 各一处）：装不下是静默丢弃，不是启动失败。

`rootdevname` 的完整落链是：`boot.cfg` → 引导器拼进 multiboot 命令行 → `mbi->mi_cmdline` → `get_parameters` 逐词拆出 name/value → `mb_set_param` 写进 `kinfo.param_buf` → PM 启动期先用 `GET_MONPARAMS` 把 `param_buf` 整块取成缓存（[`do_getinfo.c`](minix3/minix/kernel/system/do_getinfo.c) 的拷出点）→ 用户态 `sysenv` 的 `PMGETPARAM` 从这份缓存回答。它不经过 `mb_set_param`，就只是一串留在 `mi_cmdline` 里、之后没人再读的字符。

两个读者各取所需：

- RAMDS 的 `rc` 把值拼成 `/dev/xxx` 交给驱动，没设时打印 “rootdevname not set”。
- mib 服务把它作为 `kern.root_device` 节点暴露。

结论：`param_buf` 才是内核自己的参数表，写入它、维护格式的唯一入口是 `mb_set_param`；这条入口的“键不重复”只在首条记录上可靠。

### 2.4 内存账：条目来源、追加/切除与取址

#### 2.4.1 条目来源：细表与粗档

引导器给的是两档信息里的一档，`get_parameters` 先看细的那档，取不到才退回两个数。

- **细档**（`MULTIBOOT_INFO_HAS_MMAP`，bit 6）：逐条遍历 E820 表，只把 `MULTIBOOT_MEMORY_AVAILABLE`（值为 1）的条目交给 `add_memmap`；保留区、ACPI 数据区在这一步就被丢掉，而不是留着让下游判断。类型值与 E820 里“可用”同号，这不是一次翻译，是一次沿用。
- **遍历步长**是 `mm_size + sizeof(mm_size)`：`mm_size` 描述的是除它自己以外那些字段的长度（[`include/multiboot.h`](minix3/sys/arch/i386/include/multiboot.h)；本仓发那次查询的代码在 [`lib/biosmemx.S`](minix3/sys/arch/i386/stand/lib/biosmemx.S)）。这个“长度字段不算自己”的约定也是从 E820 的返回缓冲区继承来的。
- **粗档**（`MULTIBOOT_INFO_HAS_MEMORY`，bit 0）：`else` 分支把两个以 KB 计的数硬造成 `[0, mem_lower*1024)` 与 `[0x100000, 0x100000 + mem_upper*1024)` 两段连续区间。这里藏着一个假设：`mem_upper` 的语义是“从 1MB 往上**连续**可用多少”，而 `add_memmap` 收到的是一个长度。`else` 分支先 `assert(HAS_MEMORY)`：两档都没有时当场停住，而不是建一份空账。
- 细档可能有很多条（BIOS 报告的内存空洞），粗档固定两条；这是“清单形态取决于档位”的具体差别。

**引导器只给粗档那一头会静默变坏**：1MB 之上被记成一整段连续可用区间，若保留区（显卡镜像区、ACPI 数据区）落在这段范围内，[minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page](minix3/minix/kernel/arch/i386/pg_utils.c) 从最高条目尾部向下发放、迟早会消耗到那里，把页表页放进本该保留的区域。症状不是“分配失败”，而是写入静默丢失或机器随机挂死，报错位置离真正的原因（引导器只置了 bit 0）很远。这也是 §1.1 把可用内存写成“物理内存的清单”而不是“引导器给的内存数”的原因：清单的形态取决于引导器给哪一档，而“排除占用区”这一步两档都躲不掉。

结论：两档来源给出的清单形态不同，内核只能保证“至少有一档”，不能保证“细档一定存在”；可用性过滤放在入账这一步，下游才不必各自重判。

#### 2.4.2 追加与切除

可用物理内存用一个定长数组记账：`kinfo.memmap[MAXMEMMAP]`，`MAXMEMMAP` 为 40（[`minix/param.h`](minix3/minix/include/minix/param.h)）。用定长数组而不是链表，是因为引导期没有可用的分配器；每一项是一段可用区间（`mm_start` 基址 + `mm_length` 长度两个字段），`mmap_size` 是当前遍历边界（切除不回缩，因此可能高于实际最高占用下标；中间可有零长空洞）。[minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap](minix3/minix/kernel/arch/i386/pg_utils.c) 与 [minix3/minix/kernel/arch/i386/pg_utils.c:cut_memmap](minix3/minix/kernel/arch/i386/pg_utils.c) 是仅有的两个操作。

```c
// 示意，minix3/minix/kernel/arch/i386/pg_utils.c:add_memmap。
if (addr > LIMIT) return;
if (addr + len > LIMIT) len -= addr + len - LIMIT;
assert(cbi->mmap_size < MAXMEMMAP); if (len == 0) return;
```

- `LIMIT` 把账截在 4 GiB 以下，0xFFFFF000 保证低 12 位为零；源码注释给的理由是“the rest of minix currently can't deal with any bigger”。`add_memmap` 写进去的条目类型恒为 `MULTIBOOT_MEMORY_AVAILABLE`——这份数组里不存在别的类型。
- 两条判断的次序有实际意义：先判 `addr` 把整条在界外的描述符整条丢掉，再判跨界并按需要截短长度。
- 两条 `assert`（`mmap_size` 未满、`kernel_may_alloc` 为真）与一条提前返回的次序有意义：长度为零的区间在通过**容量断言之后**被静默丢弃。
- **满槽断言是粗检，空洞不豁免它**：`mmap_size` 是遍历边界，即使下标 5 有空位，只要最高下标是 39，`add_memmap` 也会在找空位之前停住——`panic("no available memmap slot")` 只在 assert 被 `-DNDEBUG` 编掉时可达。
- **找空位**：从下标 0 往后，第一个 `mm_length == 0` 的格子就是空位（没有单独的“已占用”标志位）。这个等价由入口的 `len == 0` 拒绝兜住；但 `0 < len < 4096` 的退化调用经 `rounddown` 后变成 0 并被写成一个零长度条目，会把 `mmap_size` 推高一格，只能靠“下次从下标 0 找空位时当空格复用”兜住。所以“零长度区间根本不会被写进来”这句话过强。
- `if(m >= cbi->mmap_size) cbi->mmap_size = m+1;`：新条目可能落在比当前边界更低的下标上，所以 `mmap_size` 取 `m+1` 而不能简单自增。它代表**遍历边界**；消费者必须自己跳过零长项——`pg_alloc_page` 从最高下标往回扫时，第一件事就是 `if(!mmap->mm_length) continue;`。一次切除会清空一条并把余量补到更早的空槽，`mmap_size` 不回缩；所以“当前条目数”与 `mmap_size` 不是一回事。`cut_memmap` 的循环也只走到 `mmap_size`，空洞与零长条目都在这段范围内，靠消费者跳过。
- **取整**：`roundup(addr)/rounddown(len)` 是**起点向大、长度向小**，只保证条目自身页对齐，不保证落在原描述符内。起点上取整会让终点最多外移一页：`addr = 0x1001, len = 0x2000` 得到 `[0x2000, 0x4000)`，原终点是 `0x3001`。已核对过的调用点起点都页对齐，因此这条缺口不可达；长度不保证页对齐，`mi_mem_lower*1024` 与模块文件长度都会被截掉尾页。
- `mem_high_phys` 只增不减，是所有条目的最高地址上界，`pg_identity` 依赖它。它记的是历史最大值，不是“当前最高条目的终点”：切除不回缩它，所以它是恒等映射的上界而不是当前可用上界。

两个分支还有一处不对称：`if` 分支显式清 `mmap_size = 0`，`else` 分支不写——它靠 `kinfo` 落在 `.bss` 的零初值与 `add_memmap` 的下标补救起来；这条零初值前提由两条装载路径兑现（[`loadfile_machdep.h`](minix3/sys/arch/x86/include/loadfile_machdep.h) 的 `LOAD_KERNEL` 含 `LOAD_BSS` 位，[`loadfile.h`](minix3/sys/lib/libsa/loadfile.h) 定义该位）。

`kernel_may_alloc` 是全局开关，分的是“这份账归谁”：[minix3/minix/kernel/main.c:kmain](minix3/minix/kernel/main.c) 开头写 1（“Kernel may use bits of main memory before VM is started”），[minix3/minix/kernel/main.c:bsp_finish_booting](minix3/minix/kernel/main.c) 在 `switch_to_user()` 前写 0。跨过这条线再改账，撞上的就是 `assert(kernel_may_alloc)`——改账动作当场失败，而不是悄悄改写一份已经交给 VM 的账。这个开关有两个同名定义：`pre_init.c` 的 unpaged 副本只在引导期有效，`kmain`/`bsp_finish_booting` 与高半的 `add_memmap`/`cut_memmap` 读写的是 `table.c` 通过 `_TABLE` 生成的 paged 副本。`cut_memmap` 入口也有一条同样的断言，补回余量时又经 `add_memmap` 再撞一次。

切割的方向相反：`start` 向下、`end` 向上取整，因为这里要“不把用过的页留在账上”，宁可多切一页。两处取整改变“切/不切”这个二值结果的情形只有一个：**退化请求**（`start == end` 且未落在页边界）——带取整时它被扩成整页出局，不带取整时夹取后 `substart >= subend`，一次都不切，被切对象照样留在空闲清单里。`substart/subend` 的两次修正把切除区间夹进当前条目，`if (substart >= subend) continue;` 让“完全在外”与“恰好相切”走同一个分支。命中时先把自己清零，再用 `add_memmap` 把左余量与右余量补回去；补回按“最早空槽”落位，不是末尾追加。一次切除因此可能把一条记录拆成两段，长度守恒：切 `[0x400123, 0x401F00)` 于 `[0x100000, 0x1000000)`，取整后切 `[0x400000, 0x402000)`，得到 `[0x100000, 0x400000)` 与 `[0x402000, 0x1000000)` 两段。

结论：走到这里，机器上第一次有了一份可用物理内存清单；区间值来自引导器表，但记录本身不保存声明者与依据（I2 的缺口见本章末）。粗档可能把保留区记成可用，`mem_high_phys` 只是给出了上界。

#### 2.4.3 从记录里取址：两个消费者与移交截断

`pg_utils.c` 另有两个从记录里取地址的函数，方向相反。[minix3/minix/kernel/arch/i386/pg_utils.c:alloc_lowest](minix3/minix/kernel/arch/i386/pg_utils.c) 从最低处扫，唯一调用点是 SMP 路径的 [minix3/minix/kernel/arch/i386/arch_smp.c:copy_trampoline](minix3/minix/kernel/arch/i386/arch_smp.c)：把 16 位启动代码拷到 1MB 以下给辅助处理器执行。跳板必须在 1MB 以下，是因为辅助处理器从实模式起步，16 位代码只能寻址低端内存。

它取段的姿势是 `alloc_lowest(&kinfo, tramp_size)`，而函数体里调用的是 `cut_memmap(cbi, lowest, len)`——`cut_memmap` 的第三个参数是**结束地址**，这里传的是**长度**。这是一次真实的参数语义错位：“实际切掉”的区间是 `[lowest, len)`，“想要切掉”的是 `[lowest, lowest+len)`。

| `lowest` 与 `len` 的关系 | 实际切掉 | 后果 |
|---|---|---|
| `lowest == 0` | `[0, len)`，与想要的一致 | 行为正确，但真实调用序下不可达 |
| `0 < lowest < len` | `[lowest, len)`，比分配量少 `lowest` 字节 | 尾部仍挂在空闲清单上 |
| `len ≤ lowest` | 夹取后一次都不切 | 整块被占用的内存从未从账上除去 |

真实调用序落在第三行，取决于四个前提：

1. `kmain` 先调 `cstart`（内含 `arch_init`），`smp_init` 晚于它，而 `alloc_lowest` 只在 SMP 路径被调用。
2. `arch_init` 末尾切掉含 0 的低段，`add_memmap` 又把起点向上对齐到页，所以记录里的最低基址至少是 `0x1000`。
3. `roundup(tramp_size)` 为 4096（当前跳板约 2212 B），不大于最低基址。
4. `alloc_lowest` 没有第二个调用点。

四条同时成立时，`len ≤ lowest`，**切除一次都不发生，跳板页仍挂在空闲账上**——这就是“第三行常态”。任何一条前提变化（改 `CONFIG_MAX_CPUS`、搬动 `arch_init` 的切除、新增调用点），“常态是第三行”都要重推。

第三行常态只说明“切除不发生”，不说明“不会当场被踩”：后者还要两条不在四前提内的条件——`pg_alloc_page` 从最高下标取页，跳板所在的低条目在常态下不是最高条目；跳板的生命周期在 `smp_init` 完成、VM 启动之前就结束（VM 的分配器是首次适配）。所以四前提是“落第三行”的必要条件，不是“不会暴露”的充分条件。

真正的后果在别处：返回的地址被拷进 16 位启动代码，而这段物理内存在账上仍是空闲；一旦更高下标的条目被耗尽，下一次 `pg_alloc_page` 就可能把它当空闲页发出去，辅助处理器启动到一半时代码被改写成页表内容——症状是某个 AP 起不来或执行到随机地址。**看到“分配”，要去确认它是否真的把那块内存从可用清单中除去了，而不是只看它返回了一个地址。**

`copy_trampoline` 紧接着只校验 `trampoline_base + tramp_size < (1 << 20)`：它查的是“地址足够低”，不是“这段内存已经从账上切掉”。校验通不过能发现分配得不低，却检查不出“分配了但账没改”。

另一个取址函数 [minix3/minix/kernel/arch/i386/pg_utils.c:pg_alloc_page](minix3/minix/kernel/arch/i386/pg_utils.c) 从**下标最高**的条目取最后一页：先减长度、再返回“基址+新长度”，并把用量记进 `kernel_allocated_bytes_dynamic`。取页前有三条断言，其中“长度是页的整数倍、基址页对齐”两条是整页对齐这一不变量在“发页”这一步的校验点。`alloc_lowest` 自己则先把 `len` 向上取整到页，断言 `len > 0`、找到的 `lowest != EMPTY`（`0xffffffff`）——没有一条够长时就断言失败，而不是返回 0——也把这一段的长度记进 `kernel_allocated_bytes_dynamic`。按**下标**取不等于按**地址**取：C 不维护“下标序 = 地址序”，进账按引导器报告顺序写入，切除的左右余量又都回填到最早空槽，存在至少一种切割形态使两者不一致。账耗尽时它走 `panic("can't find free memory")`。

两个方向各有其消费者：

- `pg_alloc_page` 服务装载启动镜像那条链上的两个消费点：`pre_init` 之后的 4KB 细粒度映射与 `PG_ALLOCATEME` 路径；`kernel_may_alloc` 在 `switch_to_user()` 前写 0，此后再调会先撞断言。VM 接手后的页分配由 VM 自己的分配器完成，不经过内核 `pg_alloc_page`。
- `alloc_lowest` 只服务跳板。

两者读的是同一份账，所以这份账既是建表的上界依据，也是后续所有物理页分配的来源。

**移交 VM 时的 16 条截断**：VM 只读前 `MIN(MAXMEMMAP, NR_MEMS) = 16` 条（`NR_MEMS = 16`，[`minix/const.h`](minix3/minix/include/minix/const.h)），下标 16–39 的 24 条被静默忽略，`vm/utility.c` 的注释原文是 “Any memory chunk in excess of NR_MEMS is silently ignored”。被忽略的 24 段不会以任何形式进入 VM 的位图，等于对 VM 不存在。截断发生在 VM 读取内核交接信息时，内核侧的 40 条内存账身不受影响。账在 40 条容量里建、在 16 条口径里被消费；接收方与 VM 的消费口径见 §4.5、§4.6。

结论：取址有两套规则、两个方向，而 `alloc_lowest` 的切除参数错位在当前调用序下不发作——这是“核对过的布局”，不是“代码正确”。

### 2.5 `overlaps` 与 `kern_mod`：内核把自己算成第 M+1 个模块

内存账建好后，要切掉模块占用的区间与内核自己占用的区间；每条记录切除之前先检查它与其余记录是否重叠——[minix3/minix/kernel/arch/i386/pre_init.c:overlaps](minix3/minix/kernel/arch/i386/pre_init.c) 就是源码注释里那道 sanity check。检查与切除按记录交错，没有全局预检。

正常布局里 boot2 顺序摆放模块、以页对齐推进 `image_end`（[minix3/sys/arch/i386/stand/lib/exec.c:exec_multiboot](minix3/sys/arch/i386/stand/lib/exec.c)）：前一条长度恰为页的整数倍时两者正好**相切**，否则中间留一段对齐补白。所以正常情况下谁也不会盖住谁；但相切恰恰是边界处理最容易写错的相邻关系。真出现重叠（换个引导器、或模块与内核被放在同一处）时，若没有这道检查，症状是两份镜像互相覆盖而无人报错。

```c
// 代码引用，minix3/minix/kernel/arch/i386/pre_init.c:overlaps。
#define INRANGE(mod, v) ((v) >= mod->mod_start && (v) < mod->mod_end)
#define OVERLAP(mod1, mod2) (INRANGE(mod1, mod2->mod_start) || \
		INRANGE(mod1, mod2->mod_end-1))
```

- 区间语义是半开区间 `[start, end)`：`mod_end` 是最后一个字节之后的地址。依据在引导器一侧——boot2 填的是起点加长度；minix3 的 `multiboot.h` 对 `mod_end` 一句注释都没有，**这个语义只能从填它的那行读出来**。
- `OVERLAP` 判两个点：对方区间的起点与**最后字节**。两个方向各查一次：比较对象被包在对方里时，它的最后字节落在对方内（`mod_end - 1`）；比较对象包住对方时，对方的起点落在它内。第二项写成 `mod_end - 1`，就是把半开区间翻译回最后一个有效字节。
- `overlaps` 对每条记录各调用一次，所以一对重叠会被两个方向各检查一次，返回值 0/1 由调用点变成 panic；`cmp_mod` 是外层当前记录的下标，`n` 是 `mods_with_kernel`。一对记录 O(n²) 在这个规模上（最多 20 条）完全够用，没必要做全局预检。

内核自己不带模块描述符，但要参与核查就只能自报地址范围：[minix3/minix/kernel/arch/i386/pre_init.c:get_parameters](minix3/minix/kernel/arch/i386/pre_init.c) 把它塞进模块数组的第一个空槽——`k = mbi->mi_mods_count`（`M` 是引导器报告的模块数，`k` 就是 `kern_mod`）、`assert(k < MULTIBOOT_MAX_MODS)`、`module_list[k] = [kernbase, kernbase + kernsize)`、`mods_with_kernel = k + 1`。

`module_list[k]` 的写入发生在模块表整份拷贝之后，`k` 正好是第一个空槽（拷贝只填了 0..M−1）；那条 assert 确认这个槽存在，也是“内核还要占一个槽”的形式化。`kernbase`/`kernsize` 不来自引导器，是链接脚本导出的常量（`_kern_phys_base = 0x00400000`、`_kern_vir_base = 0xF0400000`，`_kern_size` 由链接布局算出）。

如果内核走特例分支，重叠检查与切除就要各写两遍；塞进同一个数组后，一段循环同时覆盖模块与内核——这正是“内核把自己算成第 M+1 个模块”的动机。同一个循环里既检查又切除：`overlaps` 报重叠则 `panic("overlapping boot modules/kernel")`，否则用 `[mod_start, mod_end)` 从账上切除。交错也意味着 panic 可能发生在已经切掉几条之后；boot 反正停住，内存账不再有人消费。

**去掉 `-1` 会误报相切**：A 占 `[0x100000, 0x200000)`、B 占 `[0x200000, 0x300000)`，两者不重叠；不带 `-1` 时 `INRANGE(B, 0x200000)` 为真，合法布局直接 panic。误报只需一个方向成立就够——外层以 A 为比较对象的那一次就已经停住，反方向根本轮不到执行。

**把 `overlaps` 整个去掉，内存账不会坏**：同一段内存被两个模块声明时，第二次 `cut_memmap` 落在已被排除的区间上，是空操作。坏的是内容：两份镜像在重叠地址上互相覆盖，内核一声不响。所以 `overlaps` 守的是**模块字节不被踩**，不是内存账不被记错。

#### 2.5.1 三处改账动作与 12 个模块

| 名称 | 对象 | 谁做 | 动作 |
|---|---|---|---|
| 切占用 | 内核与全部模块的字节 | `cut_memmap`（§2.5） | 从可用记录里切掉 `[mod_start, mod_end)` |
| 还 VM 镜像 | 只有 VM 模块的字节 | [minix3/minix/kernel/arch/i386/protect.c:arch_boot_proc](minix3/minix/kernel/arch/i386/protect.c) | 字节搬进目标地址空间后 `add_memmap` 加回，并把 `mod_start`/`mod_end` 置 0 当“已回收”标记 |
| 还引导段 | 内核自己的 unpaged 段 | `kmain` 后段 | `add_memmap(&kinfo, bootstrap_start, bootstrap_len)`；此时 `kernel_may_alloc` 仍为 1，`kinfo` 已在 `kmain` 开头拷到链接脚本的 `.usermapped` 段副本 |

三条动作的对象各不相同：切占用处的是模块字节连同内核镜像自己那一块；还 VM 镜像处的是 VM 模块的字节；还引导段处的是 unpaged 段（边界由 `_kern_unpaged_start/end` 给出）。先后上，切占用最早，还 VM 镜像发生在内核装载启动进程时，还引导段在 `kmain` 后段。

`add_memmap` 的第三参数是**长度**（`mod->mod_end - mod->mod_start`），与 `cut_memmap` 的结束地址不同名不同义——两个被调函数同在 `pg_utils.c`，调用点分布在 `pre_init.c`（主调用）、`protect.c` 与 `pg_utils.c`，只能逐个核对签名。

- “还 VM 镜像”的加回发生在字节复制进目标地址空间之后；置零在两个字段上一起做，此后遍历 `module_list[]` 的代码会看到一条“空模块”，这是“已回收”在数据结构里的表示，而不是另设一个布尔位。`kinfo.vm_allocated_bytes` 紧跟着记录 VM 的用量。
- 其余模块在 C 的内存账上不归还：12 个引导模块里，只有 VM 的字节被加回内存账（`NR_BOOT_MODULES = INIT_PROC_NR + 1 = 12`，[`minix/com.h`](minix3/minix/include/minix/com.h)；[minix3/minix/kernel/main.c:kmain](minix3/minix/kernel/main.c) 里有 `if (NR_BOOT_MODULES != kinfo.mbi.mi_mods_count) panic(...)` 的计数核对），另外 11 个的字节区间被 `cut_memmap` 切走后没有 `add_memmap` 加回。
- 但这不等于永久少掉这段内存：VM 接手后先按模块长度把总量补进自己的空闲位图，`exec_bootproc` 装完再 `free_mem` 把 blob 放回位图。代价只发生在 VM 接手之前，且是可用总量的最后几页，不是启动失败的原因。
- 它真正的坏处是安静——不报错、不留痕；要对账也难，因为内存账缺的模块字节两边都不记，残差里还混着低段切除、数据段末端（EOD）取整损失与跳板那一页，不能单归因给模块。启动期唯一会走到 `pg_alloc_page` 的是装载启动镜像那条链（[minix3/minix/kernel/arch/i386/protect.c:libexec_pg_alloc](minix3/minix/kernel/arch/i386/protect.c) 传 `PG_ALLOCATEME`），真到耗尽那一刻 panic 会打在装载途中，那只是报信的地方，不是出错的地方。

“还引导段”要等到 `kmain` 后段，因为在此之前 `pre_init` 的低段代码与 `kinfo` 还在使用；`kinfo` 拷到 `.usermapped` 副本、活跃的页目录与静态页表池属于高半 paged 副本之后，这段低地址内存才可以归还。

结论：内核与模块占用区只被一段代码处理，模块与内核之间没有第二套逻辑；C 的内存账对非 VM 模块“只切不还”，回收责任移交 VM。

### 2.6 模块装载的检查与失败模式

模块装载的检查分三层，失败现场不同，诊断入口也不同：

| 层 | C 判据 | 失败现场 | 诊断入口 |
|---|---|---|---|
| 次序 | `load_mods` 必须先于 `multiboot` | `assert(mi_mods_count > 0)`，不是查 `HAS_MODS` 的那条 | `HAS_MODS` 已置且条数为 0 时查次序；未置先查 `module_init` |
| 条数 | `MULTIBOOT_MAX_MODS = 20`，内核自己还要占一个槽 | `assert(k < MULTIBOOT_MAX_MODS)` 或装载器越界 | 模块数上限与内核占槽 |
| 区间 | `overlaps` 用半开区间检查，相切放行 | `panic("overlapping boot modules/kernel")` | 重叠检查的 `-1` 与相切判据 |

条数上限 20 是数组容量，实际引导模块数是 12（`NR_BOOT_MODULES`）；两者不是一回事——12 是名单长度，20 是能装下的最大槽数。

三层问的不是同一件事：

- 次序问“模块表建的时候链表里有没有东西”。
- 条数问“内核之外还有几个槽可用”。
- 区间问“两段字节有没有互相覆盖”。

混层排查会把引导器配置错误当成内核缺陷，或者反过来。“参数缓冲装不下”不在这三层里，因为它是静默丢弃，没有失败现场。

结论：三层检查各自停在不同位置，排障先分层再定位；Rust 侧的对应处理见第 3、4 章。

### 2.7 ARM32 对照：自己造一份 multiboot

同一段引导准备在 x86 与 ARM 两侧各有一份入口装配，但主体逻辑共用同一串函数名。Minix3 的 C 树只有 `i386` 与 `earm` 两侧；Rust 侧的第三架构见第 3 章。相同的地方：

- 入口标签同名 `MINIX`，入口之后都跳到 `multiboot_init`。
- 临时栈的放置与写法一致：`load_stack` 在文件末尾 `.data`，取高端符号。
- 清帧指针、随后换高半栈并调 `kmain` 的次序一致。

不同的地方：

- 指令集本身。
- 参数传递：x86 用栈传魔术值与 `multiboot_info` 地址；ARM 用寄存器传 `argc`/`argv`，没有 `magic` 也没有 `ebx`。
- `pre_init` 返回的 `kinfo` 指针：x86 经 `%eax` 压栈交给 `kmain`，ARM 留在 `r0` 里一路带到 `kmain`。

真正体现“引导协议不属于内核”的地方在 `pre_init` 内部：ARM 的 `get_parameters` 第一件事不是读引导器结构体，而是**造一个**——[minix3/minix/kernel/arch/earm/pre_init.c:setup_mbi](minix3/minix/kernel/arch/earm/pre_init.c) 先 `memset`，再自己置 `MODS | MEM_MAP | CMDLINE` 三位（x86 上这三位是引导器置的），模块表是编译期常量 `MB_MODS_BASE + i * MB_MODS_ALIGN`，内存表只有一条硬编码的连续区间，命令行直接取自引导器寄存器传入的 `bootargs`（注释写作 “morph the bootargs into multiboot”）。

ARM 自造 `mbi` 的意义在于：`get_parameters`、`overlaps`、`cut_memmap`、`mb_set_param` 这一整串主体逻辑不需要第二份实现——差异被挡在入口装配层。此后 [minix3/minix/kernel/arch/earm/pre_init.c:get_parameters](minix3/minix/kernel/arch/earm/pre_init.c) 的其余部分与 x86 同构；注意 ARM 侧头文件用的是另一套字段名，同构是调用序与语义上的，不是拼写上的。

[minix3/minix/kernel/arch/earm/pre_init.c:overlaps](minix3/minix/kernel/arch/earm/pre_init.c) 有一处与 x86 不同：`INRANGE` 的上界写成 `<= ...->mod_end`，且上界取的是外层循环变量 `thismod`（宏在展开点捕获它，不是宏形参），`OVERLAP` 第二项直接取 `mod_end`、没有 `- 1`——合起来是闭区间判定，正是 §2.5 论证为错误的那一版。`INRANGE` 捕获外层局部变量是宏的典型陷阱：读这段代码必须把宏放回它在循环里的展开位置，不能按普通函数理解。

它在 ARM 上无害：模块槽距是 8 MiB、每个合成模块的 `mod_end` 又各留了一页（`MB_MODS_ALIGN - ARM_PAGE_SIZE`），模块之间不会相切。内核项是例外：它作为第 M+1 个模块追加时没有减一页，与 0 号模块的 30 MiB 间隔（`MB_MODS_BASE = 0x82000000` 减 `_kern_phys_base = 0x80200000`）是“当前镜像远小于 30 MiB”的构建事实（`[观测]`），镜像恰好长到 `0x1E00000` 字节时内核项与 0 号模块会相切并被误报。内存表只有一条，所以 16 条截断在 ARM 上不构成风险；x86 才可能用满 40 条。

撇开硬件差别，引导准备里与硬件无关的部分是四步：

| 步 | 语义 | x86-32 落点 | ARM32 落点 | 谁定的 |
|---|---|---|---|---|
| 1 | 接收引导参数 | `head.S` 压栈 + `pre_init(magic, ebx)` | `head.S` 只 `bl pre_init`，再由 `setup_mbi` 自造结构 | 引导协议（各架构不同） |
| 2 | 建立地址映射 | `pg_clear` + `pg_identity` + `pg_mapkernel` | 同名函数（页粒度不同） | 分页硬件（语义相同） |
| 3 | 开启分页/MMU | `pg_load` + `vm_enable_paging` | 同名函数（MMU 寄存器不同） | MMU 寄存器 |
| 4 | 把启动信息交下去 | `return &kinfo` → `kmain(&kinfo)` | `r0` 仍持有指针 → `kmain` | 调用约定 |

结论：步骤 2、3 在各架构调用同名函数，因为那串操作的语义与“用哪条指令、页表项多少位”无关；只有第 1 步的入口与参数来源必须按架构另写，因为“参数从哪来”由引导器决定。

### 2.8 本章归纳：C 交付物的形状

x86-32 主链一句话：`boot.cfg` → boot2 → `head.S` 的 `multiboot_init` → `pre_init(magic, ebx)` → `get_parameters`（整份拷贝 `mbi`、命令行进 `mb_set_param`、模块表、内存图进 `add_memmap`）→ 对 M+1 条记录 `overlaps`/`cut_memmap` → `pg_*` 建表与开分页（见 02 篇（镜像篇））→ `return &kinfo` → `kmain`。`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging` 五步属 02 篇（镜像篇）的建表节，本章只保留调用位置与 D3/D5 的落点。

这三类载荷在交付面落成 `kinfo` 里的五组记录（最后一组含建表结果 `freepde_start`，它不是交接数据）：

- `kinfo.mbi`：引导器结构体的副本；其中的指针只在 `get_parameters` 内用于拷贝模块表与命令行，拷贝完成后不再解引用。
- `kinfo.param_buf`：命令行与固定键（`arch`/`board`）的落地表，NUL 分隔。
- `kinfo.memmap[]`/`mmap_size`/`mem_high_phys`：可用物理内存清单与它的上界。
- `kinfo.module_list[]`/`mods_with_kernel`/`kern_mod`：模块表与“内核当模块”的那一项。
- `kinfo.bootstrap_start`/`bootstrap_len` 与 `freepde_start`：镜像的 unpaged 段与建表返回的第一个空闲槽位号；`freepde_start` 不是交接数据，是建表结果。

交付物里没有单独的“可用内存总量”字段；总量要由 `memmap[]` 前 `mmap_size` 条累加。本章只描述 C 的记录形状；类型化与协议留给第 3 章。

启动期的五条次序约束 D1–D5（编号是约束名，不是执行序；D1/D2/D4 的证据在本章，D3/D5 在 02 篇（镜像篇）的建表节）：

- **D1. 内存清单先于建表**。清单在 `get_parameters()` 里建好，随后在 `pre_init` 里切完；两者都先于 `pg_*`（[minix3/minix/kernel/arch/i386/pre_init.c:pre_init](minix3/minix/kernel/arch/i386/pre_init.c) 里的排列顺序）。`pg_identity()` 用 `mem_high_phys` 决定哪些条目标成非缓存，且函数开头就有 `assert(cbi->mem_high_phys)`——反排时该值还是 `.bss` 的零初值，断言当场失败。这条 assert 只覆盖 `add_memmap` 这一半；`cut_memmap` 先于任何清单消费者，由调用序保证。`pg_mapkernel()` 的初始大页映射不依赖记录（它用链接期常量与已清零的静态页目录）；从记录里动态取页的是 `pre_init()` 之后的 4KB 细粒度路径，它同样要求记录已就位。
- **D2. 内核位置先于清单裁剪**。清单要排除的区间里包含内核镜像自己；镜像的物理区间来自链接期常量 `_kern_phys_base`/`_kern_size`，不来自引导器的数据结构（§2.3、§2.5）。
- **D3. 页表先于开启分页**。建表五步与 `enable` 的次序在 02 篇（镜像篇）的建表节；本章只保留 `pg_*` 的调用位置。
- **D4. 结构先于 `kmain`**。`pre_init` 的返回值是 `&kinfo`（[minix3/minix/kernel/arch/i386/pre_init.c:pre_init](minix3/minix/kernel/arch/i386/pre_init.c) 的 `return &kinfo;`），`head.S` 在 `call kmain` 前把它压栈作为第一个参数（同一条 `push %eax` 在引文里是给 `pre_init` 传参用的，同名不同处）。
- **D5. 恒等映射先于内核映射**。覆盖次序的推演在 02 篇（镜像篇）的建表节；本章只保留约束名与调用位置。

换栈那一步的落点：`head.S` 在 `call kmain` 之前把栈换到 `k_initial_stktop`，它落在高半一侧；判据与证据表在 02 篇（镜像篇）的 §2.5。本章只需要这个事实：`pre_init` 返回后换上的栈与紧接着要跳去的高半在同一侧，所以换栈与跳转能成对。

断言不证明布局一定合法，它把“不合法”从后置的沉默故障提前成现场的立即停止——断言改不了事实，只改故障出现的位置。对回第 1 章的 I1–I4：I1（按值交付）由整份拷贝 `mbi`、命令行、模块表与内存图复制承载；I2（来源）在 C 侧的 memmap 记录上没有承载者（镜像位置的来源是链接布局）——`add_memmap` 只抄过滤后的区间数值，不记声明者与依据，这正是第 3 章要补的来源记录缺口；I3（所有权）在 C 侧分两步：引导器一次填好 `mbi` 后不再改（发送侧发布）；内核把 `mbi` 整份拷贝进 `kinfo` 后独占该副本，后续只由内核自己写；I4（校验）由 `overlaps` 与各条断言承载，`cut_memmap` 是切除动作而不是校验器；I4 的版本项在 C 侧也没有对应物——Multiboot info 没有版本字段，Rust 侧的协议版本校验是新增面；来源项按 I2 复核，而 C 侧 memmap 记录无来源，这一支为空；镜像落在某个可用区间内这一项也没有独立检查——`cut_memmap` 对界外区间是空操作，不会报错。

---

#### 失败对照

`boot.cfg` 里把 `load_mods` 写在 `multiboot` 之后，内核会停在 `get_parameters` 的 `assert(mbi->mi_mods_count > 0)`——不是查 `HAS_MODS` 的那条：标志位由 `multiboot` 置上，表也照建，只是条数为 0。正确写法是把两条指令放在同一条 `menu=` 行里、用分号隔开，`load_mods` 在前。模块表分配失败是另一条路：`module_init` 提前返回，`HAS_MODS` 未置位，内核停在查标志的断言，到不了条数断言。两种现场输出（是否设 `cttyline=`）见第 6 章。这条断言只打印表达式，不打印条数值，所以现场只能看到“`mi_mods_count > 0` 失败”，不能直接看到 0。同类的“边界写错”在 §2.5 还有一例：`OVERLAP` 少写 `-1`，相切的合法布局会被判成重叠并 panic。

#### 最小示例

```bash
$ grep -rn rootdevname minix3/minix/kernel/
$ grep -n 'assert(magic == MULTIBOOT_INFO_MAGIC)' minix3/minix/kernel/arch/i386/pre_init.c
$ grep -n 'add_memmap\|cut_memmap\|mb_set_param' minix3/minix/kernel/arch/i386/pre_init.c
$ grep -n 'MULTIBOOT_MAX_MODS' minix3/sys/arch/i386/include/multiboot.h
```

第一条无输出，正是“内核只当搬运工”的判据；第二条命中一行；第三条给出建账、切账与写参数表的三个入口；第四条给出数组容量 20 的定义处。

#### 小结

- 引导器契约由识别头、寄存器约定与 `mbi` 三样东西对接。
- `get_parameters` 整份拷贝 `mbi`，再把命令行、模块表、内存图变成 `kinfo` 的记录。
- `add_memmap` 建清单，`overlaps`/`cut_memmap` 逐条切除内核与模块占用区。
- D1–D5 是约束名，不是执行序；证据在本章与 02 篇（镜像篇）的建表节。
- ARM32 复用了同名入口与同一串函数名，差异在参数来源与 MMU 寄存器。

#### 术语

- **`mbi`**：引导器填好的 `multiboot_info` 结构体；`get_parameters` 把它整份拷进 `kinfo.mbi`。
- **`kinfo`**：`struct kinfo`（`kinfo_t`）的全局实例，`pre_init` 填好、`kmain` 读取的记录。
- **内存账**：C 侧 `kinfo.memmap[]` 里的可用物理内存清单（细档已过滤，粗档可能含保留区；`mmap_size` 是遍历边界，`mem_high_phys` 是上界），是第 1 章内存图在 C 的落点；不是另一个数据结构。
- **`param_buf`**：内核自己的启动参数表，NUL 分隔的 `key=value` 串，唯一写入口是 `mb_set_param`。
- **粗档 / 细档**：内存图的两种来源——`mem_lower`/`mem_upper` 两段连续区间，或 `mmap` 条目表。
- **`overlaps`**：检查内核与模块、模块与模块区间是否相交的函数；相切必须放行。
- **`kern_mod`**：内核在 `module_list[]` 里的下标；内核以第 M+1 个模块的身份参与重叠检查与切除。
- **unpaged 段**：链接在低地址、虚拟地址等于物理地址、分页开启前执行、分页开启后只靠恒等映射再执行一小段、之后可归还的那段代码与数据。
- **[观测]**：真机或源码观察，不是结构证明。
- **第三行常态**：四前提成立时，`alloc_lowest` 的调用序落在 `len ≤ lowest` 那一行，切除不发生。
