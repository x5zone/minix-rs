# 第 2 章摘要：C 源码分析

## 本章回答什么

C 侧没有“交接协议”这个类型，却要把三类数据变成内核自己的记录：可用内存图落到 `kinfo.memmap[]`；镜像位置落到 `kinfo.bootstrap_*` 与链接期常量，不从 `mbi` 读；交接结构落到 `kinfo.mbi`。本章只回答两件事：C 在每一步保证了什么，边界在哪里失效。地址空间如何建立不在本章。

## 引导器契约与入口（§2.1–§2.2，第 25–105 行）

- `load_mods` 只把匹配文件的路径挂进引导器链表，不读内容、不分配地址；真正的读取与地址登记发生在 `multiboot` 执行时。
- 对接靠三样东西：识别头、两个魔术值、引导器填好的 `mbi`。`0x1BADB002` 是内核 → 引导器的识别头 magic，`0x2BADB002` 是引导器 → 内核的 info magic，后者是 `pre_init()` 的第一条断言。
- 识别头只在镜像前 8KB 内可被扫描；`magic + flags + checksum == 0` 是加法恒等式，不是校验算法。`flags` 只置 `WANT_MEMORY` 与 `MODS_ALIGNED`；五个地址字段全 0 且 `HAS_ADDR` 未置，装载位置交给引导器。本仓实际走 boot2（按 ELF 装载，不读识别头）。
- `boot.cfg` 的 `menu=` 行里 `load_mods` 必须先于 `multiboot`：反序时 `HAS_MODS` 已置、条数为 0，内核停在 `assert(mi_mods_count > 0)`。模块表分配失败是另一路：`module_init` 提前返回、`HAS_MODS` 未置，停在查标志的断言。诊断入口按分支选：`HAS_MODS` 未置查模块表分配失败，已置而条数为 0 才查两条指令的先后。
- 模块数硬上限 `MULTIBOOT_MAX_MODS = 20`；内核占第 M+1 个槽，引导器最多给 19 个模块。实际引导模块数 `NR_BOOT_MODULES = 12`，`kmain` 有 `NR_BOOT_MODULES != mi_mods_count` 的计数 panic。
- `head.S` 的 `multiboot_init` 设栈、清 `%ebp`/EFLAGS、按 cdecl 传参：两个 `$0`（一个配 `popf` 清标志，一个作栈顶空位），先压 `%ebx` 再压 `%eax`。入口 `%esp ≡ 0 (mod 16)`，不满足 SysV 的 `≡ 12`；`load_stack` 放 `.data` 省掉对装载器清 `.bss` 的依赖。
- 两条 `push` 对调的失败现场：`pre_init` 收到 `magic = ebx`，第一条断言失败，但 assert 只打印表达式、不打印值，容易误往“数据结构坏了”方向查。

## 参数落表（§2.3，第 106–186 行）

- `get_parameters` 先把整个 `mbi` 拷进 `kinfo.mbi`；按值交付要三层都完成：结构体、命令行、模块表/内存图。只做第一层，I1 不成立。
- `user_sp`/`vir_kern_start`/`bootstrap_start`/`bootstrap_len` 不来自引导器，来自链接期常量。镜像位置要分清“谁放”和“放哪”：按 ELF 头把镜像放到链接指定物理地址的是引导器；地址值来自双方共读的链接结果，内核读自己的常量即可。C 不通过交接结构传镜像位置，而由链接常量重建；可重建的数据不必跨边界复制。
- 命令行只在 `MULTIBOOT_INFO_HAS_CMDLINE` 置位时解析；没置位时 `param_buf` 里只有后面两条固定键；`param_buf` 是 NUL 分隔的 `key=value`，整串再以一个 NUL 收尾，容量 1024。容量检查失败返回 `-1`，三处调用点全部忽略返回值：装不下是静默丢弃，不在这三层装载检查里。
- `mb_set_param` 识别 `cttyline`/`cttybaud`、维护键不重复、维护可追加格式。删除扫描有真实 off-by-one：`while (*p++)` 后再 `p++`，从第二条记录起匹配从第二个字符开始，同名键匹配不上。因此“键不重复”只在待删键是首条时可靠；外部 `board=` 非首条时删不掉，`get_value` 返回第一条，命令行遮蔽内核值；首条才被删、内核值胜出。
- 空键把缺陷推到极端：`strncmp(p, "", 0)` 恒真，判定只剩 `p[0] == '='`；若第二条记录形如 `B=2`，删除从 `=` 处开始并截断后续记录。
- 随后无条件写 `arch=`/`board=`，保证 `param_buf` 至少各有一条内核写入的记录；`arch=` 在内核侧只有写点、由用户态 RAMDS 经 `sysenv arch` 读；`board=` 有读点，但外部同名键非首条时会遮蔽内核值（首条则被删除循环删掉）。
- `rootdevname` 的读者是 RAMDS 的 `rc`（拼 `/dev/xxx`）与 mib 服务（`kern.root_device`）。它走完整落链：`boot.cfg` → 引导器命令行 → `mi_cmdline` → `param_buf` → PM 缓存 → 用户态 `sysenv`。值写时不带 `/dev/` 前缀（形如 `c0d0p0`），由 `update_bootcfg.sh` 的 `sed` 剥掉，配置里只写 `$rootdevname`；内核只当搬运工。

## 内存账与取址（§2.4，第 187–269 行）

- 两档：细档（`HAS_MMAP` bit 6）逐条遍历 E820 表，只收 `AVAILABLE = 1`，步长 `mm_size + sizeof(mm_size)`；粗档（`HAS_MEMORY` bit 0）把 `mem_lower`/`mem_upper` 硬造成两段连续区间。两档都没有时 `else` 先 `assert(HAS_MEMORY)`，当场停住，不建空账。
- 粗档风险有条件：只有“引导器只给粗档”且保留区（显卡/ACPI）落在 1MB 以上连续段时，`pg_alloc_page` 才会消耗到那里；症状是静默丢写或随机挂死。
- `add_memmap`：`LIMIT` 截在 4GiB 以下（0xFFFFF000）；起点向上、长度向下取整；`mmap_size` 是遍历边界、切除不回缩；满槽断言先于找空位，空洞不豁免；`0 < len < 4096` 经 `rounddown` 成 0 仍会被写成零长条目并推高 `mmap_size`。
- `cut_memmap`：`start` 向下、`end` 向上取整；退化请求被扩成整页出局；`substart/subend` 夹取，相切 `continue`；左右余量回填最早空槽，长度守恒。
- `kernel_may_alloc` 在 `kmain` 开头写 1、`switch_to_user()` 前写 0；`pre_init.c` 的 unpaged 副本只在引导期有效，`kmain`/高半改账读写的是 `table.c` 经 `_TABLE` 生成的 paged 副本。
- `mem_high_phys` 只增不减，是恒等映射上界而不是当前可用上界；`pg_identity` 开头 `assert(cbi->mem_high_phys)` 只覆盖 `add_memmap` 一半，`cut_memmap` 由调用序保证；`pg_mapkernel` 的初始大页映射不依赖记录。
- `alloc_lowest` 把长度传给 `cut_memmap` 的结束地址，是真实的参数语义错位。四前提（`cstart` 先于 `smp_init`、低段已切、`roundup(tramp_size)=4096` 不大于最低基址、无第二调用点）成立时落在“一次都不切”的第三行常态；`copy_trampoline` 只查地址低于 1MB，不查账已切。“不切”只说明账没改，不说明跳板页不会被后续分配踩到。
- `pg_alloc_page` 从最高下标取页，`alloc_lowest` 从最低处扫；按索引取不等于按地址取。VM 只读前 `MIN(MAXMEMMAP, NR_MEMS) = 16` 条，16–39 条被静默忽略；C 的 40 条账在 VM 的 16 条口径里消费。

## 重叠检查与模块（§2.5–§2.6，第 270–339 行）

- `overlaps` 用半开区间 `[start, end)`，`OVERLAP` 查对方起点与最后字节；`mod_end - 1` 是把半开区间翻译回最后有效字节。少写 `-1` 会把相切的合法布局误判为重叠并 panic。
- 内核没有模块描述符，但把自己塞进第 M+1 个槽（`kern_mod`），用同一段循环做重叠检查与切除；`overlaps` 守的是模块字节不被踩，不是内存账不被记错。
- 三处改账动作对象不同、先后固定：切内核与全部模块最早，还 VM 镜像在装载启动进程时，还内核 unpaged 段在 `kmain` 后段；非 VM 模块在 C 的内核账上只切不还，回收责任移交 VM。
- 装载检查分三层：次序（`load_mods` 先于 `multiboot`）、条数（20 槽，实际 `NR_BOOT_MODULES = 12` 个引导模块）、区间（`overlaps`）。`param_buf` 装不下不在这三层里，因为它是静默丢弃、没有失败现场。

## ARM32 对照与归纳（§2.7–§2.8，第 340–400 行）

- ARM32 入口同样叫 `MINIX`/`multiboot_init`，但参数走 `argc`/`argv`，没有 `magic`/`ebx`；`kinfo` 经 `r0` 返回。`setup_mbi` 先 `memset` 再自造 `mbi`，模块表是编译期常量，内存表只有一条硬编码区间，命令行取自 `bootargs`；ARM 头文件字段名不同，同构是调用序与语义上的，不是拼写上的。
- ARM 的 `overlaps` 用闭区间（`<=`、无 `-1`）且宏捕获外层 `thismod`；槽距 8 MiB 通常不相切。例外是内核项作为第 M+1 个模块追加时未减一页，与 0 号模块的 30 MiB 间隔是 `[观测]` 构建事实；镜像长到 `0x1E00000` 字节时会相切并被误报。
- §2.7 的四步结论是：只有第 1 步“接收引导参数”随架构不同，建立映射、开启分页、把启动信息交下去三步共用同一语义。x86-32 主链：`boot.cfg` → boot2 → `head.S` → `pre_init` → `get_parameters` → `add_memmap`/`overlaps`/`cut_memmap` → 建表与开分页（02 篇（镜像篇））→ `return &kinfo` → `kmain`。交付物是 `kinfo` 里的五组记录：`mbi`、`param_buf`、`memmap`/`mmap_size`/`mem_high_phys`、`module_list`/`mods_with_kernel`/`kern_mod`、`bootstrap_start`/`bootstrap_len`/`freepde_start`。没有单独的“可用内存总量”字段，总量由 `memmap[]` 前 `mmap_size` 条累加；`freepde_start` 不是交接数据，是建表返回的第一个空闲槽位号。
- D1–D5 是约束名而不是执行序：D1/D2/D4 的证据在本章，D3/D5 的建表与开分页证据在 02 篇的建表节。
- 对回 I1–I4：I1 由整份拷贝 `mbi`、命令行、模块表、内存图承载；I2 在 C 侧没有承载者——`memmap` 条目只有基址、长度、固定 `AVAILABLE` 标签，不记声明者与依据；I3 分两步：引导器一次填好 `mbi`（发送侧发布），内核整份拷贝后独占 `kinfo`，此后不再改；I4 由 `overlaps` 与断言承载，`cut_memmap` 是切除动作不是校验器；I4 的版本项也没有对应物（Multiboot info 无版本字段）；“镜像落在可用区间内”也没有独立检查（`cut_memmap` 对界外区间是空操作）。
- 本章不证明：C 的记录不含来源 provenance；地址空间如何建立、页表如何开分页，归 02 篇。

下一章把这套 C 记录改写成设计契约：`KernelInfo` 按值交接、两张表加来源记录、门链先于消费；本章的 `kinfo` 字段形状与 I1–I4 结论是它的输入。
