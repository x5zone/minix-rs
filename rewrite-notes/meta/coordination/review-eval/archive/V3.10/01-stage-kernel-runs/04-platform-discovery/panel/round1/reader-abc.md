VARIANCE: A+B+C 合并一个 agent

# D04 R1 P-READER（A 初学者路径 + B 结构编辑 + C 因果一致性）

- 基线核对：`rewrite-notes/01-stage-kernel/04-platform-discovery.md` 为 2688 行，sha256 前缀 `a8cb91b1`，与任务给定一致。
- 只读边界：本报告只依据该文档与 `reader-profile.md`；未读源码、其他文档、diff、历史评审、其他面板输出、`expected/`。凡需要源码才能判定的条目，在「是否需源码核实」一栏标「是」。
- 严重度口径：高 = 会让读者得出错误结论或无法继续；中 = 需要回读或交叉翻页才能继续；低 = 措辞、精度或可复现性瑕疵。
- 类型口径：`FINDING` = 卡住阅读或文档内部自相矛盾；`SUGGESTION` = 措辞或编排偏好。
- 总览：A 7 FINDING + 2 SUGGESTION；B 5 FINDING + 4 SUGGESTION；C 12 FINDING + 3 SUGGESTION（共 33 条）。最高优先级：C-01（反事实结论相反）、C-02（53/52 计数矛盾）、C-06（两条 BSP 回退路径不一致）、A-04 与 A-05（关键代码节选缺行）。

# A 初学者路径

**路径记录**：按读者画像从「熟悉 OS 概念与 C、Rust 只读到 trait/no_std 静态全局/unsafe 大意」出发，顺序走 §1 → §7。§1 的四组问题表在第一屏就把「定时器频率」与「CPU 主频」混在一格（A-01）；§1.2 的「ARM 这一侧」与上一行的 `aarch64` 冲突（A-02）；§1.4 的例外注记一口气抛出五个未定义标识符与一个未解释计数（A-03）。§2 的 C 码段整体可跟，但 §2.1 与 §2.3 的两段关键节选缺行，读者必须靠散文补全（A-04、A-05）；§2.7 的默认开关段需要回读三遍（A-06）。§3 的 `PlatformSource` 与 `PlatformDescSource` 只差一个词（A-07）。§4–§6 没有新的卡点，§7 的两处措辞问题记为 A-08、A-09。结论：能读完，但 A-01 至 A-07 每一条都造成一次回读。

**教学目标覆盖**（对照 `reader-profile.md` 的六项）：

| 目标 | 覆盖 | 卡点 |
|---|---|---|
| ① 四组问题与消费者（§1.1） | 有表、有消费者 | A-01 把定时器频率与 CPU 主频混在一行 |
| ② 三种固件答法与 C 两套做法（§1.2、§2.9） | 两节都完整 | A-02 的「ARM」歧义 |
| ③ 启动窗口两条硬边界与「只搬地址」（§1.3、§1.5） | 边界清楚 | A-03 的例外注记冲淡主线 |
| ④ Rust 八项设计决策（§3） | §3.1–§3.8 完整 | B-01 的「八项/七项」口径 |
| ⑤ 实现链（§4） | §4.1–§4.11 完整 | A-04、A-05 的节选缺行 |
| ⑥ 测试与诊断（§5、§6） | 两章完整 | C-02、C-03 的计数与栈帧 |

## FINDING

## FINDING RD4-READER-A-01（P2）

- **位置+摘录**：§1.1（L23-L28）表 1 第 2 行：「| 计数器在哪、频率是多少 | 配置时钟中断频率、读取当前计数 | `minix3/minix/kernel/arch/i386/arch_clock.c:TIMER_FREQ`（8254 的输入时钟，常量）；CPU 主频由 `minix3/minix/kernel/arch/earm/arch_system.c:cpu_identify` 写进 `cpu_info[cpu].freq` |」
- **严重度**：中
- **问题**：一行里塞了两个不同问题——定时器的输入时钟频率（`TIMER_FREQ`，`1193182` Hz）与 CPU 主频（`cpu_identify` 写 `660`）；且「计数器在哪」在这行没有答案，PIT 的端口要到 §3.8 才出现。
- **影响**：读者在全书第一张表里建立不起「四组问题」的边界；后文 §2.10 又把「定时器频率」与「CPU 主频」拆成两行，前后分类不一致。
- **证据（只引文档）**：§2.10（L1004、L1007）把「定时器频率」与「CPU 主频」列为两行；§2.9（L885）`cpu_info[cpu].freq = 660; /* 660 Mhz hardcoded */`；§3.8（L1445-L1446）才给出 PIT 端口 `0x40`/`0x43`。
- **最小修复**：把该行拆成「定时器输入频率」与「CPU 主频」两行，或在行内注明「CPU 主频另见 §2.9，不属定时器问题」；「计数器在哪」指向 §3.8。
- **置信度**：高
- **是否需源码核实**：否（文档内部表与表之间已经不一致）
- **若我错了**：若 §1.1 的「频率」有意统称「时钟相关频率」，则只需把该行改名为「时钟相关频率」；但「计数器在哪」仍然缺答案。
- **命中清单**：§1.1 表 1；§2.9；§2.10 表；§3.8 判据表。

## FINDING RD4-READER-A-02（P2）

- **位置+摘录**：§1.2（L54）：「x86 这一侧有一整个 ACPI 解析器（`minix3/minix/kernel/arch/i386/acpi.c`，410 行；复核见 §6.2 #7），ARM 这一侧连解析器都没有——它的中断控制器（OMAP INTCPS）基址、串口基址、定时器源时钟全部来自 BSP 头文件与板号分支（§2.9）。」
- **严重度**：中
- **问题**：上一张表（L50-L52）刚把 `aarch64` 单列一行；此处的「ARM 这一侧」实际指 `earm`（32 位 ARM 的板级实现），但读者会把它读成 `aarch64`。§1.6（L177）才说「earm 与 Rust 的 aarch64 是两回事」。
- **影响**：读者会把「aarch64 没有解析器」带进 §1.6 与 §4.5，与后文「aarch64 有 DTB 与 ACPI 两条解析链」直接冲突；这是全篇 C/Rust 对照的第一处错位。
- **证据（只引文档）**：§1.6（L165）「本篇的 C 侧对照物只有 Minix3 内核的两个架构实现：i386 与 earm」；§1.6（L177）「earm 与 Rust 的 aarch64 是两回事：前者出现在本篇 C 章……后者的发现链在 C 树里没有任何先例」；§4.5（L1820-L1853）给出 aarch64 的 `from_fdt`。
- **最小修复**：在 L54 首次出现时写成「ARM 这一侧（`earm`，32 位 ARM）」，或把 §1.6 的对照表前移为 §1.2 的脚注。
- **置信度**：高
- **是否需源码核实**：否（文档内两名并用是事实）
- **若我错了**：若本仓约定「ARM」在 C 语境中专指 `earm`、在 Rust 语境中专指 `aarch64`，那么问题只是没有在首次出现处声明这条约定；仍建议补一句。
- **命中清单**：§1.2 三种固件表；§1.6 覆盖边界；§4.5。

## FINDING RD4-READER-A-03（P2）

- **位置+摘录**：§1.4（L98-L106）步骤 ② 的例外注记：「例外注记：boot-shim（OpenSBI/U-Boot 腿）与 bootface（直核腿）为装配 `KernelInfo.memmap` 各自解 DTB 的 `/memory` 子集——经 minix-boot 的 `DtbMemoryMap` 单源共享……aarch64 UEFI 腿另有一处更重的例外：`find_platform_sources` 之后 `dtb_copy::preflight` 读 DTB 头、`authorize_and_copy` 整块复制、`cross_validate_config_dtb` 解析整棵树并把 `/memory` 与 UEFI 内存图交叉比对（5 条 panic 停机路径）……」
- **严重度**：中
- **问题**：一段括号里出现 `bootface`、`DtbMemoryMap`、`dtb_copy::preflight`、`authorize_and_copy`、`cross_validate_config_dtb` 五个未定义标识符，外加一个未解释的计数「5 条 panic 停机路径」；读者在「只搬地址」的概念链上突然被拽进 §4.2 与 01 篇的实现细节。
- **影响**：§1.4 是读者建立「五段链」的地方；这段例外把「只搬地址」的主结论冲淡，读者会误以为引导阶段本来就要解析整棵树。
- **证据（只引文档）**：本节紧接着的 L108-L123 又把五段链完整画了一遍；这些标识符在本篇其余部分没有再解释（`DtbMemoryMap` 全文仅此一处）。§1.3（L78-L85）刚刚把「取地址 / 解析」切成两行。
- **最小修复**：在 §1.4 只保留一句「引导阶段另有两处例外：memmap 装配所需的 `/memory` 子集，以及 aarch64 UEFI 腿的 DTB 预校验；详见 §4.2 与 01 篇」；把这些标识符与「5 条 panic 停机路径」的计数移到 §4.2 或脚注。
- **置信度**：高
- **是否需源码核实**：否（问题在编排，不在事实）
- **若我错了**：若本节目标读者被假定已读 01 篇并认识这些标识符，那么这是「跨篇引用」而非「未定义」；但读者画像只保证「启动信息已由 boot-shim 交接」，不保证记得这些名字。
- **命中清单**：§1.3 边界；§1.4 五段链；§4.2。

## FINDING RD4-READER-A-04（P2）

- **位置+摘录**：§2.1（L325-L342）。代码节选（L326-L340）标题为「`apic_single_cpu_init` —— 节选（函数开头的三段）」，正文只显示 `cpu_feature_apic_on_chip()`、`lapic_addr = LOCAL_APIC_DEF_ADDR`、`lapic_enable(0)` 三段，以 `...` 收尾；紧接着 L340-L342 断言：「节选里这三段都在 `acpi_init()` **之前**执行，而 `detect_ioapics()` 排在它之后——两段次序写在函数体里。」
- **严重度**：中
- **问题**：`acpi_init()` 与 `detect_ioapics()` 两行都不在节选里。读者看到的是「函数开头三段」，却被要求接受「两段次序写在函数体里」这一关键事实；同一节前面 L302 还说「`apic_single_cpu_init` 的第一步就是常量赋值」，而节选的第一行明明是 `cpu_feature_apic_on_chip()`。
- **影响**：§2.1 的核心结论（「发现与第一个消费者相邻」以及「顺序不依赖 `arch_init`」）全靠这两行未展示的代码支撑；读者要么跳过论证，要么怀疑节选是残缺的。
- **证据（只引文档）**：同一节 L323 已经用散文补过一次「`acpi_init()` 调用之后紧接着的下一句才是 `if (!detect_ioapics())`」；L342 说「两段次序写在函数体里」，但 L326-L339 的节选没有函数体后半段。§2.7（L793）又单独引用了 `apic.c:1141` 的 `acpi_init();`，说明作者手里有这一行。
- **最小修复**：把节选扩到包含 `acpi_init();` 与 `if (!detect_ioapics()) { ... }` 两行（或至少加一行注释「省略号处：`acpi_init();` 然后 `if (!detect_ioapics())`」）；把 L302 的「第一步就是常量赋值」改为「第一段是 APIC 存在性检查，随后才是常量赋值」。
- **置信度**：高
- **是否需源码核实**：是（确认 `acpi_init` 与 `detect_ioapics` 在函数体中的确切位置；文档散文已给出，但节选未给）
- **若我错了**：若节选的 `...` 按全篇惯例代表「与论点无关的省略」，且读者被假定会去读 C 源，那么这只是一处引用完整性问题；但本篇的教学目标不要求读者读 C 源。
- **命中清单**：§2.1 调用序；§2.7 调用点；代码节选完整性。

## FINDING RD4-READER-A-05（P2）

- **位置+摘录**：§2.3（L486-L526）`acpi_read_sdt_at` 节选。前五行是：`struct acpi_sdt_header hdr;` → `if (tb == NULL) { ... return hdr.length; }` → `...`（L503）→ `if (acpi_check_signature(tb->signature, name))`（L504）→ `if (size < tb->length)`（L509）→ `if (read_func(addr, tb, size))`（L514）。
- **严重度**：中
- **问题**：`tb->signature` 与 `tb->length` 在 `read_func(addr, tb, size)` 之前被读取，节选却把「先只读表头到 `tb`」的那一步藏在 `...` 里。按显示顺序读，这段代码在使用未初始化的 `tb`。
- **影响**：读者会认为三道闸的顺序有 bug（先签名/容量、后读入），或认为 `tb` 由调用方预填；两种理解都会让「签名/容量/校验和三道闸」这一段的解释失效。
- **证据（只引文档）**：节选标题 L486 说「只保留签名/容量/校验和三道闸」，L503 的 `...` 没有说明省略了什么；L514 的 `read_func(addr, tb, size)` 是第一次出现「把数据读进 `tb`」的动作。§2.3 后文 L528 又用 `tb->length` 解释容量闸，默认读者已经接受 `tb` 已被填好。
- **最小修复**：在 `...` 处补两行（例如 `if (read_func(addr, tb, sizeof(struct acpi_sdt_header))) ...` 与随后的 `if (acpi_check_signature(tb->signature, name))`），或改成一句「省略号处先只读 36 字节表头到 `tb`，以下三道闸都基于这份表头」。
- **置信度**：高
- **是否需源码核实**：是（确认省略号处的确切语句；文档自身不足以还原）
- **若我错了**：若 `tb` 在调用点已经被填过表头（例如 `acpi_init` 传入的 `rsdt` 是静态且先被 `read_func` 填过），那么节选仍缺这层上下文；问题从「代码错」变成「上下文缺失」，仍需补一句。
- **命中清单**：§2.3 三道闸；代码节选完整性；`acpi_phys_copy` / `read_func` 语义。

## FINDING RD4-READER-A-06（P2）

- **位置+摘录**：§2.7（L796）：「`main.c` 的 `cstart` 会读启动参数 `no_apic`，**未给出时把 `config_no_apic` 置为 1**（`if(value) config_no_apic = atoi(value); else config_no_apic = 1;`），于是 `arch_init` 里的 `if (config_no_apic) {…} else if (!apic_single_cpu_init())` 走前一个分支、`apic_single_cpu_init` 里那次不执行；SMP 构建的 `cstart` 先按 `config_no_apic` 置 `config_no_smp`，再用 `no_smp` 覆盖（缺省 0）……默认 `no_apic` 缺省为 1，`kmain` 走 `smp_single_cpu_fallback`。」
- **严重度**：中
- **问题**：一句话里出现四个近名开关（`no_apic` 启动参数、`config_no_apic` 变量、`config_no_smp` 变量、`no_smp` 启动参数），且「默认 `no_apic` 缺省为 1」把「启动参数未给出」与「变量被置 1」混成一句；读者无法一眼还原「默认构建到底 APIC 开不开、SMP 开不开、哪一次 `acpi_init` 会跑」。
- **影响**：§2.7 的全部结论（三个调用点、默认只跑一次、第二次不幂等）都以这条默认状态为前提；读错默认值，后面整节的「第二次」讨论就失去对象。
- **证据（只引文档）**：同节 L798-L801 的表把「第一次」与「第二次（还需启动参数 `no_apic=0`）」并列，但表头没有说明默认值是「参数未给出」还是「变量为 1」；L803 说「第二次不是空转」，L805 说「它的失败路径不幂等」，都以「第二次会执行」为例，而默认配置下它不执行。
- **最小修复**：在 L796 前加一张三行小表：「启动参数 → `config_no_apic` → `config_no_smp` → 实际分支」；把「默认 `no_apic` 缺省为 1」改成「`no_apic` 未给出 ⇒ `config_no_apic = 1` ⇒ `arch_init` 跳过 `apic_single_cpu_init`」。
- **置信度**：高（问题在表述；具体默认值需源码确认）
- **是否需源码核实**：是（`no_apic`/`no_smp` 的读取点与 `config_no_smp` 的赋值需要对照 C 源确认）
- **若我错了**：若这些变量名在 C 源里就是同名同义、且 `no_apic` 确实是一个「缺省为 1」的全局，那么问题只是没有把「参数」与「变量」分开写；仍建议拆句。
- **命中清单**：§2.7 调用点；§2.10 归纳；§4.1 C/Rust 时序表。

## FINDING RD4-READER-A-07（P2）

- **位置+摘录**：§3.2（L1088）trait 签名：「`fn source(&self) -> PlatformSource;`」；§3.2（L1096）：「`PlatformSource` 是 `DeviceTree` / `Acpi` / `QemuVirt` 三个变体的诊断标签」；§3.1（L1047-L1052）：「`pub platform_sources: &'static [PlatformDescSource]`」；§3.6（L1359）：「切片元素是 `PlatformDescSource`……一个 kind 标签加一个物理地址」。
- **严重度**：中
- **问题**：`PlatformSource`（格式诊断标签，三个变体）与 `PlatformDescSource`（kind + 物理地址的交接项）只差一个 `Desc`，两者都被中文写作「来源」。§7 的 Rust 落点表（L2650）只列了 `PlatformDescSource`，没有 `PlatformSource`。
- **影响**：读者在 §3.1 先遇到 `platform_sources`（复数、交接切片），在 §3.2 遇到 `PlatformSource`（单数、格式标签），在 §4.7 遇到 `source()`（返回格式标签），很容易把三者当成同一层；`init_from_kinfo` 的「按来源顺序尝试」与「`source()` 诊断」因此混淆。
- **证据（只引文档）**：§3.6（L1359）明确 `PlatformDescSource` 是「kind 标签加一个物理地址」；§3.2（L1096）明确 `PlatformSource` 是「三个变体的诊断标签」；§4.7（L2081）说 `source()` 是「类型擦除后的手工 RTTI」。三处定义不同，命名只差一个词。
- **最小修复**：在 §3.2 第一次出现 `PlatformSource` 处加一张两行对照：「`PlatformDescSource` = 交接项（kind + PA）；`PlatformSource` = 解析结果的格式标签」；在 §7 的 Rust 落点表补 `PlatformSource`。
- **置信度**：高
- **是否需源码核实**：否（命名与用途在文档内已给出）
- **若我错了**：若本仓有意用 `PlatformSource` 作为 `PlatformDescSource` 的「解析后投影」，那么这是合理的分层，只需在首次出现处写明「投影」关系；目前没有这句话。
- **命中清单**：§3.1 handoff 字段；§3.2 trait；§3.6 不透明标签；§4.7 `source()`；§7 落点表。

## SUGGESTION

## FINDING RD4-READER-A-08（P3）

- **位置+摘录**：§1.1（L40）：「LAPIC 那一格是空的：MADT 的表头里明明有一个 `local_apic_address` 字段，C 从头到尾没读过它（§2.5 给出全部检索证据）。」
- **严重度**：低
- **问题**：上一张表（L35）的 LAPIC 行并不是空格子，而是写着「常量 `LOCAL_APIC_DEF_ADDR`」与「**否**」。读者会去找一个空白单元格，找不到。
- **影响**：只是措辞，但它是全篇「半发现」论点的第一句，容易让读者以为表格排版出错。
- **证据（只引文档）**：L32-L38 的表把 LAPIC 的来源写成常量、是否来自固件描述写成「**否**」；§1.1 L40 自己紧接着说「这一组自己也只发现了半格」。
- **最小修复**：改成「LAPIC 那一行答的是『否』：MADT 表头里明明有 `local_apic_address`……」，或直接说「LAPIC 基址没有走固件发现」。
- **置信度**：高
- **是否需源码核实**：否
- **若我错了**：若「那一格」指的是「是否来自固件描述」这一列，那么「空」是想表达「没有『是』」，但表格里写的是「否」；仍建议改成「否」。
- **命中清单**：§1.1 表 2；§2.5 LAPIC 半发现。

## FINDING RD4-READER-A-09（P3）

- **位置+摘录**：§2.8（L837-L862）：「`struct machine machine __section(".usermapped");`……这段由 VM 以用户权限映射进进程地址空间——它是『C 内核数据直接暴露给用户态』的通用机制；ACPI 驱动走的是上面的系统调用拷贝路径。」
- **严重度**：低
- **问题**：文本同时给出两条用户态读取路径（`.usermapped` 直接映射、`SYS_GETMACHINE` 拷贝），但没有说哪条路径实际被谁使用。读者刚在 L860 看到 ACPI 驱动用 `sys_getmachine`，会问「那 `.usermapped` 映射给谁用」。
- **影响**：§2.8 的论点是「发现结果同时服务内核内与用户态消费者」；两条路径并存但只解释一条，读者无法判断 `.usermapped` 是活机制还是历史遗留。
- **证据（只引文档）**：L841 的 `__section(".usermapped")`；L860 的「驱动文件自己定义 `struct machine machine;`……用 `sys_getmachine(&machine)` 拿回一份拷贝」；L862 的「ACPI 驱动走的是上面的系统调用拷贝路径」。
- **最小修复**：加一句「`.usermapped` 的映射由 X 使用（或当前无活消费者，仅为 C 的通用机制保留）；ACPI 驱动只走 `SYS_GETMACHINE`」。
- **置信度**：中（文档内确实没有第三个消费者）
- **是否需源码核实**：是（确认 `.usermapped` 是否还有别的用户态读者）
- **若我错了**：若 `.usermapped` 的消费者在本篇范围外（例如 PM），那么这是「范围外不展开」，补一个指向即可。
- **命中清单**：§2.8 `machine` 发布；§4.10 `GET_MACHINE` 对账。

# B 结构编辑

**路径记录**：按文档自身骨架（§1 概述 → §2 C 分析 → §3 设计 → §4 实现 → §5 测试 → §6 诊断 → §7 参见）逐节检查段落归属、前后承诺与重复。§1–§2 的「先给结论、再给反事实」模式整体成立；问题集中在三处：§2.2 提前使用 §2.3 的变量、§3.9 追加后没有回头修 §3 开头与头部的「八项」、§6.2 的证据块不能由所给命令产生。另有四处重复/引用精度问题记为 SUGGESTION。

## FINDING

## FINDING RD4-READER-B-01（P2）

- **位置+摘录**：头部（L10）教学目标：「④Rust 八项设计决策（§3）」；§3 开头（L1027）：「本章按这个次序给出八项设计决策」；§3.9（L1475）：「这七项与 §3.1–§3.8 合起来才是完整的设计面；§3 不再宣称『八项』覆盖全部决定。」
- **严重度**：中
- **问题**：§3.9 明确否认「八项」是完整设计面，但头部教学目标与 §3 开头仍写「八项设计决策」，且没有说「八项核心 + 七项实现层」。三处对同一数字给出不同承诺。
- **影响**：读者按教学目标验收 §3 时会以为 §3.9 是附录；按 §3.9 读又会以为开头漏了七项。§3.9 的七项里有若干（地址三语义、`check_gic_madt` 决策表）是后文的关键论证，不是可选补充。
- **证据（只引文档）**：L10 的「八项」；L1027 的「按这个次序给出八项」；L1475 的「不再宣称八项覆盖全部决定」；L1463 的「§3.1–§3.8 是跨架构的核心决策；还有七项决定只在某一层成立」。
- **最小修复**：把 L10 与 L1027 改为「八项核心决策（§3.1–§3.8）＋七项实现层决策索引（§3.9）」；在 §3 开头加一张「决策 → 对应问题（§2.10 的三问）」映射表。
- **置信度**：高
- **是否需源码核实**：否
- **若我错了**：若「八项」被定义为「§3.1–§3.8 的八项」，而 §3.9 只是索引、不算设计决策，那么 L1475 的「不再宣称」就说过头了；无论哪种，三处需要统一措辞。
- **命中清单**：头部教学目标；§3 导语；§3.9 索引；§2.10 三问。

## FINDING RD4-READER-B-02（P2）

- **位置+摘录**：§2.2（L405）：「这里有一个容易读错的地方：`sdt_count` 的算式里 `s` 是 `int`、`sizeof` 是无符号类型，`-1` 在减法前被提升成 32 位无符号数，`(-1 - 36) / 4` 得到的是**正数 1073741814**……循环会进入，越界在 `i=35` 处开始……」；§2.3（L530-L534）又用两段重讲同一件事：「第一层不是写穿而是**越界读**……第二层才是写越界，路径在别处：`acpi_init` 的循环里由表头长度算出 `sdt_count = (s - 36)/4`……被污染的是 `sdt_trans[]`……」
- **严重度**：中
- **问题**：§2.2 还没定义 `sdt_count`、`sdt_trans`、`acpi_read_sdt_at` 的返回值，就先做了一遍它们的反事实推演；到 §2.3 定义完这些名字后又把同一推演用「两层」框架重讲一遍。两处的强调点不同（§2.2 说「越界读写 + 表清单被垃圾填充」，§2.3 说「第一层越界读、第二层越界写」）。
- **影响**：读者在 §2.2 被 `sdt_count` 打断，在 §2.3 又读到同一段论证，容易以为两处结论不一致；两段合计约 20 行，是 §2 里最长的反事实。
- **证据（只引文档）**：§2.3（L442）才出现 `sdt_count = (s - sizeof(struct acpi_sdt_header)) / sizeof(u32_t);`；§2.3（L471-L479）才定义 `rsdt` / `sdt_trans` / `sdt_count` 三个静态；§2.2 L405 却已经使用 `sdt_count` 与 `sdt_trans[]`。
- **最小修复**：§2.2 只保留「缺签名闸会让 `rsdt_addr` 取到任意值，`acpi_read_sdt_at` 返回 -1」这一层，把整数提升与越界推演整段移到 §2.3 的 `sdt_count` 定义之后；§2.3 用「两层」框架统一讲，§2.2 加一句「后果见 §2.3」。
- **置信度**：高
- **是否需源码核实**：否
- **若我错了**：若 §2.2 的提前推演是为了让读者在读到 `sdt_count` 时已有警觉，那么这是有意的「预告」；但它使用了未定义名字，预告的代价大于收益。
- **命中清单**：§2.2 签名闸反事实；§2.3 `sdt_count` / `sdt_trans`；重复段落。

## FINDING RD4-READER-B-03（P2）

- **位置+摘录**：§2.1（L323-L342）。先设问：「这里有一个容易读反的地方：`apic_single_cpu_init()` **自己会先调一次 `acpi_init()`**……所以『把 `arch_init` 里的两步对调』并不能制造出『字段还是 0』的状态——`acpi_init` 由被调函数内部发起，仍排在 `detect_ioapics` 之前。真正的依赖是：`detect_ioapics` 读 `machine.acpi_rsdp`，而这个字段必须有人先填。被对调顺序影响的只是『谁去填它』。」随后 L342 又总结：「所以这条链不依赖 `arch_init` 的调用次序。」
- **严重度**：中
- **问题**：段落开头用「把这两步对调会坏在哪」建立预期，随后用五行论证这个对调其实不会坏；结论「只影响谁去填」与「不依赖调用次序」是同一件事的两种说法，但读者要等到段尾才知道「不会坏」。
- **影响**：这是 §2.1 的核心论证段；自问自答的结构让「会坏在哪」的预期落空，读者可能把「被对调顺序影响的只是谁去填它」误读成「还是有一点影响」，从而带走一个模糊结论。
- **证据（只引文档）**：L323 的设问；L323 中段的自我否定；L342 的总结；同节 L300 的「准确的边界是：`init_clock` 与 8259 路径不依赖发现结果；而真正消费 ACPI 结果的 APIC 初始化在发现之后、同一个函数体内」——真正的结论其实在 L300 已经给出。
- **最小修复**：把 L300 的结论提到段首，改成「次序不影响正确性，因为 `apic_single_cpu_init` 自己会调 `acpi_init`；真正的依赖是 `machine.acpi_rsdp` 必须先被填。下面用一段代码确认这条内部次序。」然后只保留 L342 的总结，删掉「把这两步对调会坏在哪」的设问。
- **置信度**：高
- **是否需源码核实**：否
- **若我错了**：若作者的目的正是先复现一个常见误读、再纠正它，那么结构是有意的；但当前写法把误读放在前面且篇幅更长，纠正只有一句，重心反了。
- **命中清单**：§2.1 调用序论证；§2.7 调用点；教学节奏。

## FINDING RD4-READER-B-04（P2）

- **位置+摘录**：§6.2（L2593-L2600）：
  ```
  $ rg -c 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | sort -t: -k2 -rn | head -3
  os/kernel/src/dm_coverage.rs:10
  os/arch/src/arch/boot.rs:9
  os/kernel/src/lib.rs:6
  ...（其余见 §5.3 分布表）
  os/libs/minix-boot/src/handoff_v1_write.rs:1
  os/kernel/src/kerninfo.rs:1
  ...
  ```
- **严重度**：中
- **问题**：`head -3` 只能输出三行，代码块却列了五行输出（前三行之外还有两行 `:1`）。这两行不可能来自这条命令。若它们是从完整输出里摘的示例，命令应写成 `head -5` 或分成两条命令；现在的写法让「可复制命令」不可复制。
- **影响**：§6.2 的标题是「可复制命令与预期输出」，证据强度依赖命令与输出一一对应；这一处让读者无法判断是命令写错、输出过期，还是 `rg -c` 的排序口径不同。
- **证据（只引文档）**：L2593 的命令以 `head -3` 结尾；L2594-L2596 是前三名；L2598-L2599 是第四、第五行；L2597 的「...（其余见 §5.3 分布表）」夹在中间，说明后两行是「其余」的示例而不是 `head -3` 的输出。§6.2 开头（L2576）称这一节为「可复制命令与预期输出」。
- **最小修复**：把 `head -3` 改成 `head -5`（并同步 §5.3 的表），或删掉 L2598-L2599 两行、只保留「其余见 §5.3」；若确实想展示「1 条」的示例，另起一条命令并标注「示例，不是排序结果」。
- **置信度**：高（shell 语义确定）
- **是否需源码核实**：否
- **若我错了**：若 L2598-L2599 是人工补充的注释而非命令输出，那么应加 `#` 前缀或「例如：」；当前无标记，按代码块惯例会被读成输出。
- **命中清单**：§6.2 #4；§5.3 分布表；证据可复现性。

## FINDING RD4-READER-B-05（P2）

- **位置+摘录**：§5.3（L2507）：「21 个极简测试内核（`os/qemu-tests/test-kernels/kernel/bootstrap/` 下，如 `hello-boot` / `test-paging-enable` / `test-protection` 各架构变体；全量即 §6.2 #4 输出里的测试内核条目）」。
- **严重度**：中
- **问题**：§6.2 #4 的输出只显示前三名与两个 `:1` 文件，没有任何测试内核条目；「全量即 §6.2 #4 输出」这个指向落空。读者无法从文档里枚举这 21 个内核，也无法核对「21」这个数字。
- **影响**：§5.3 的分布表合计 52、命令输出 53（见 C-02），本来就有一个未对上的条目；「21 个测试内核」又是最大的一格。指向落空使这两个数字都无法自查。
- **证据（只引文档）**：L2497-L2507 的分布表最后一行写「21 个极简测试内核……各 1」；L2593-L2600 的 §6.2 #4 输出没有出现 `test-kernels` 路径；L2509 说「上述 52 处」。
- **最小修复**：补一条命令，例如 `rg -l 'platform_sources:\s*&\[\]' os/qemu-tests/test-kernels | wc -l` 与 `... | sed 's#.*/bootstrap/##'` 的清单；或把「全量即 §6.2 #4 输出」改成「清单见 `os/qemu-tests/test-kernels/kernel/bootstrap/` 下各 `src/main.rs`」。
- **置信度**：高
- **是否需源码核实**：是（确认 21 个内核的实际清单与计数）
- **若我错了**：若 §6.2 #4 的完整输出在别处（例如日志）且文档只摘了前五行，那么至少应把「全量」指向那份日志；当前指向的是本节代码块。
- **命中清单**：§5.3 分布表；§6.2 #4；测试内核清单。

## SUGGESTION

## FINDING RD4-READER-B-06（P3）

- **位置+摘录**：§1.3（L64-L74）与 §2.4（L613-L623）两次逐字引用同一个 `acpi_phys2vir` 函数，两处代码块内容相同（`if(!vm_running) { DEBUGEXTRA(...); return p; } panic(...)`）。
- **严重度**：低
- **问题**：同一个十行函数在相隔约 550 行处完整出现两次，第二次没有新增注释或不同视角。
- **影响**：增加篇幅；读者读到 §2.4 会以为这是另一个函数或另一个版本，先比对再确认是同一个。
- **证据（只引文档）**：§1.3 L65-L74 与 §2.4 L614-L623 的代码逐字相同；§2.4 L625 只说「这条约束与 Rust 侧……是同一件事的两侧表达」，没有解释为何重引。
- **最小修复**：§2.4 只引 `if(!vm_running)` 三行，或写「同 §1.3 的 `acpi_phys2vir`，此处只看 `vm_running` 分支」并给出交叉引用。
- **置信度**：高
- **是否需源码核实**：否
- **若我错了**：若两处引用分别服务于「启动窗口」与「MADT 消费」两个论点，重引可以让读者不必翻页；那么可以在第二次引用上方加一句「重引是为了对照 MADT 消费点」，成本更低。
- **命中清单**：§1.3 启动窗口；§2.4 MADT 消费；重复引用。

## FINDING RD4-READER-B-07（P3）

- **位置+摘录**：§7（L2639）：「`minix3/minix/kernel/arch/i386/acpi.c` — `acpi_init` / `get_acpi_rsdp` / `acpi_read_sdt_at` / `acpi_madt_get_typed_item` / `acpi_get_ioapic_next` / `acpi_get_lapic_next` / `acpi_get_ioapics`（`apic.c`）」。
- **严重度**：低
- **问题**：`acpi_get_ioapics` 被列在 `acpi.c` 的条目下，只用括号注明「（`apic.c`）」。§2.6（L741）的出处是 `minix3/minix/kernel/arch/i386/apic.c:acpi_get_ioapics`。
- **影响**：读者按 §7 去 `acpi.c` 找 `acpi_get_ioapics` 会找不到；括号里的更正容易被扫读忽略。
- **证据（只引文档）**：L2639 的条目；§2.6 L741 的「`/* minix3/minix/kernel/arch/i386/apic.c:acpi_get_ioapics —— 逐字引用 */`」；同页 L2642 另有 `apic.c` 条目。
- **最小修复**：把 `acpi_get_ioapics` 移到 L2642 的 `apic.c` 条目下，或把它单列一行并标注「在 `apic.c`」。
- **置信度**：高
- **是否需源码核实**：否（文档内 §2.6 已给出出处）
- **若我错了**：若 `acpi_get_ioapics` 在 `acpi.c` 与 `apic.c` 各有一份，那么 §2.6 与 §7 需要同时说明两份的关系；目前只有一处出处。
- **命中清单**：§7 C 源码清单；§2.6 IOAPIC。

## FINDING RD4-READER-B-08（P3）

- **位置+摘录**：§3.1（L1052）引用 `KernelInfo` 的文档注释：「Empty slice means boot-shim did not provide any — kernel falls back to `QemuVirtDesc` (dev) or panics (release). See 04-platform-discovery.md §4.6.」
- **严重度**：低
- **问题**：引文让读者去看「§4.6」，但本稿 §4.6 是「QemuVirtDesc：三份 per-arch 常量表」，兜底规则在 §3.5（L1267-L1310）与 §4.7（L2030-L2060）。引文里的节号与当前结构不对应。
- **影响**：读者跳转到 §4.6 只看到常量表，找不到「dev 兜底 / release panic」的判定；若他不再回翻，就会以为兜底规则没有正文。
- **证据（只引文档）**：L1052 的引文；§4.6 的标题（L1922）；§3.5 的标题（L1267）与 §4.7 的 `init_from_kinfo`（L2032-L2059）。
- **最小修复**：在引文后加一句「引文中的 §4.6 是源码注释写作时的编号；本稿的兜底规则见 §3.5、§4.7」，或同步更新源码注释（若允许改源码）。
- **置信度**：中高
- **是否需源码核实**：是（确认源码注释里的 §4.6 是否指本文旧版）
- **若我错了**：若源码注释里的「04-platform-discovery.md §4.6」指的是另一份文档的 §4.6，那么只需在引文旁注明「另一份文档」；当前读起来就是本稿。
- **命中清单**：§3.1 KernelInfo 引文；§3.5 兜底；§4.6 QemuVirtDesc；§4.7 `init_from_kinfo`。

## FINDING RD4-READER-B-09（P3）

- **位置+摘录**：§4.5（L1888-L1897）的「字段的来源分两类」表：`ic`、`timer`、`console`、`cpu_topology`、`sswi_setip_base`、`psci_conduit` 六组字段共八行，没有 `PlicDesc.context`。直到 §5.1（L2392）才说：「它验证 PLIC 基址、`PlicDesc.context` 的硬编码值 1（这是『断言现状』，不是从 DTB 发现出来的值）」。
- **严重度**：低
- **问题**：§3.3（L1172）的 `PlicDesc` 有三个字段 `plic_base` / `nr_irqs` / `context`；§4.5 的来源表只解释了前两个，`context` 的来源（硬编码 1）第一次出现在 §5.1 的测试断言里。
- **影响**：读者在 §4.5 会以为 `context` 也是从 DTB 解析出来的；到 §5.1 才知道它是常量。§4.6 的 `QemuVirtDesc` 里 `context: 1` 也没有解释。
- **证据（只引文档）**：§3.3（L1172）的字段表；§4.5（L1888-L1897）的来源表；§4.6（L1953-L1957）的 `context: 1`；§5.1（L2392）的「硬编码值 1」。
- **最小修复**：在 §4.5 的来源表补一行「`context` | 硬编码 1（S-mode hart 0 的 PLIC context） | 不从 DTB 读；见 §5.1 的『断言现状』」。
- **置信度**：高
- **是否需源码核实**：是（确认 `parse_plic` 是否真的硬编码 1，以及是否有其他取值路径）
- **若我错了**：若 `context` 在某个 DTB 属性里有来源，那么 §5.1 的「硬编码」说法需要改；无论哪种，§4.5 的来源表都漏了一个字段。
- **命中清单**：§3.3 `PlicDesc`；§4.5 字段来源表；§5.1 测试断言。

# C 因果一致性

**路径记录**：逐条核对文档里每一个「会坏在哪」「若……会……」的反事实，以及每一个计数、交叉引用与前后承诺。整体上这套「反事实 + 证据」的写法是有效的：§2.3 的两层越界、§2.5 的 LAPIC 半发现、§3.5 的兜底代价、§4.4 的 GICv2 拒绝、§4.8 的切根后 MMIO 失效都自洽。问题集中在两处硬冲突（C-01 的反事实结论与所引代码相反；C-02 的 53/52 计数）与若干处「同一件事两种说法」（C-03 至 C-12）。

## FINDING

## FINDING RD4-READER-C-01（P2）

- **位置+摘录**：§2.4（L646）：「去掉 `acpi_get_lapic_next` 里那句 `if (ret->flags & 1) break;`（只留 `idx++`）会坏在哪：`acpi_get_lapic_next` 在遍历完所有记录后返回 `NULL`，`discover_cpus` 的 `while` 立刻结束，`ncpus` 停在已计入的启用项数；如果第一条记录就是未启用项，结果是 0 颗 CPU，而不是『把所有记录都计进 `ncpus`』。」
- **严重度**：高
- **问题**：按 L587-L601 的代码，「只留 `idx++`」之后，`for (;;)` 会在**第一次调用内**依次取 LAPIC 记录 0、1、2……直到 `acpi_madt_get_typed_item` 返回 `NULL`，然后 `break` 出循环、返回 `NULL`。`discover_cpus` 的 `while` 第一次判断就拿到 `NULL`，循环体一次都不执行，所以 `ncpus` **恒为 0**。不存在「停在已计入的启用项数」的状态（之前没有任何一次成功计入），也不存在「第一条记录是未启用项才 0、否则全计入」的分叉。
- **影响**：这条反事实是 §2.4 对 `flags & 1` 过滤器的唯一论证。读者若按文档的改法做实验，会得到「无论如何都是 0 颗 CPU」，与文中「少了 CPU / 一颗都没有」两种症状的描述对不上；更严重的是，它会让人误以为过滤器的语义是「少算」，而实际是「让游标停在第一个启用项」。
- **证据（只引文档）**：L575 `static unsigned idx = 0;`；L587-L601 的 `for (;;) { ret = ...; if (!ret) break; idx++; if (ret->flags & 1) break; }`；L635 `while (ncpus < CONFIG_MAX_CPUS && (cpu = acpi_get_lapic_next()))`。把 `if (ret->flags & 1) break;` 删掉后，循环条件只剩 `if (!ret) break;`，第一次调用就会耗尽所有匹配记录。
- **最小修复**：改写为「第一次调用就会耗尽所有 LAPIC 记录并返回 `NULL`，`discover_cpus` 的 `ncpus` 恒为 0；`flags & 1` 的作用不是『少算』，而是让游标停在第一个启用项、把后续记录留给下一次调用」。若想保留「如果第一条是未启用项」的讨论，需要构造一个不同的改法（例如把 `break` 改成 `continue` 并保留 `return ret` 的时机）。
- **置信度**：高（所引代码足以判定）
- **是否需源码核实**：否（文档内代码完整；若要确认 `static idx` 的跨调用行为，L575-L601 已给出）
- **若我错了**：若 `acpi_get_lapic_next` 在每次调用后把 `idx` 重置（代码里没有），或 `discover_cpus` 在调用前已计入若干项（代码里没有），那么「停在已计入的启用项数」才可能成立；当前两处都没有。
- **命中清单**：§2.4 `acpi_get_lapic_next`；§2.4 `discover_cpus`；反事实论证。

## FINDING RD4-READER-C-02（P2）

- **位置+摘录**：§5.3（L2491-L2493）命令输出 `53`；§5.3 分布表（L2497-L2507）逐行相加为 52（10+9+6+1+1+1+1+1+21）；§5.3（L2509）正文写「上述 **52** 处的被测单元」；§6.2 #4（L2592）再次输出 `53`。
- **严重度**：高
- **问题**：同一节里「53」与「52」并存，分布表自身合计 52，正文也说 52，只有命令输出说 53。至少有一个 `platform_sources: &[]` 位置没有被分布表计入，或者命令的计数口径与表格不同。
- **影响**：§5.3 的核心论点是「52 处空切片都是正常夹具，不是覆盖漏洞」；如果实际是 53 处，就有一处未分类的 `&[]`，判据「被测单元是否依赖平台发现」可能漏掉一个。这个数字还被 §5.4 的 M1/M2 覆盖判断引用，错误会向下游传播。
- **证据（只引文档）**：L2492 的 `53`；L2497-L2507 的九行分布（各行合计 52）；L2509 的「上述 52 处」；L2592 的 `53`；L2491 的命令 `rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' | wc -l`。
- **最小修复**：重跑命令并二选一：若真为 53，补上遗漏的那一行（并核对是不是某个 `src/main.rs` 被算了两次）；若真为 52，把两处输出改成 52 并说明命令为何多算一条（例如注释或字符串里出现了一次）。同时把 L2509 的「52」与分布表锁定为同一口径。
- **置信度**：高（算术确定）
- **是否需源码核实**：是（需要实际重跑 `rg` 才能知道 53 与 52 哪个是真相）
- **若我错了**：若命令的 53 里有一条是 `platform_sources: &[]` 出现在非夹具上下文（例如文档注释），而分布表有意只统计夹具，那么正文应写「命令 53 条，其中 1 条为注释/非夹具，夹具 52 处」；当前没有这层说明。
- **命中清单**：§5.3 分布表；§6.2 #4；计数口径；§5.4 覆盖表。

## FINDING RD4-READER-C-03（P2）

- **位置+摘录**：§4.1（L1548）：「两次排错的症状是同一句 panic——运行时断言文本是 `"Frozen::get before freeze"`……且调用栈相同；区别只是原本要验证的时钟/中断消费者更晚才被到达。」§6.1（L2562）：「栈里先出现 `init_protection`（Phase B 的第一个 `platform_desc()` 读取点），再出现 `init_clock_and_interrupts` 或 `boot_init_timer` ⇒ 发现排在它们之后；栈里没有 `init_from_kinfo` ⇒ 根本没调（§4.1）。」
- **严重度**：中
- **问题**：如果 `init_protection` 是第一个读 `platform_desc()` 的点，它会在自己的栈帧里 panic；调用栈到此为止，不可能再出现更晚的 `init_clock_and_interrupts` 或 `boot_init_timer`。§4.1 的「更晚才被到达」也不成立——它们根本不会被到达。
- **影响**：§6.1 是诊断章的行 2，读者会按「栈里有没有 `init_clock_and_interrupts`」来判断；实际症状栈里永远没有这一帧。按文中判据，读者可能误判「不是发现顺序问题」。
- **证据（只引文档）**：§4.1（L1505-L1508）的表把 `init_protection` 列为 Phase B 的第一个动作；§4.1（L1548）自己说 `init_protection` 已经无条件读 `platform_desc()`；§4.9（L2218-L2223）的 `init_protection` 节选确实先调用 `platform_desc().cpu_topology()`；§6.1（L2562）却要求栈里出现更晚的消费者。
- **最小修复**：把 §6.1 行 2 的「预期输出关键行」改为「栈顶是 `Frozen::get`，其下是 `platform_desc`、`init_protection`、`kmain`；栈里没有 `init_from_kinfo` ⇒ 发现没跑或跑在 `init_protection` 之后」。§4.1 的「更晚才被到达」改为「时钟/中断消费者永远到不了，所以症状只会停在 `init_protection`」。
- **置信度**：高
- **是否需源码核实**：否（文档内 `init_protection` 先读的结论已给出）
- **若我错了**：若 `init_protection` 在某些架构上不读 `platform_desc()`，那么该架构的栈里可能出现更晚的消费者；但 §4.1 说它是「x86-64 与 aarch64/riscv64 共有的读取点」，三架构都先读。
- **命中清单**：§4.1 Phase A.5；§4.9 `init_protection`；§6.1 诊断表行 2。

## FINDING RD4-READER-C-04（P2）

- **位置+摘录**：§1.4（L127-L133）的五段链表：「| ② | 物理地址 | 交接记录里的一个地址字段 | **引导器（loader / boot-shim）** |」；表头写的是「谁做的（C 侧）」。§2.2（L346-L374）却显示 C 的 `get_acpi_rsdp` 由内核自己读 `0x40E`、扫描 EBDA 与 `0xE0000..0x100000`，并把命中地址写进 `machine.acpi_rsdp`。
- **严重度**：中
- **问题**：五段链是 §1.4 的概念骨干，但 ② 的「谁做的（C 侧）」把 C 的 RSDP 定位归给引导器；C 的实际做法是内核扫描，引导器只交 `kinfo`。§3.1（L1031）也明说「C 的发现全在内核内：`acpi_init` 是 kernel 的函数……引导器只负责把 `kinfo` 填好」。
- **影响**：读者会把「RSDP 由 loader 发现」带进 §2.2，读到内核两次扫描时以为那是 Rust 侧的新做法；C 与 Rust 的职责切分（§3.1 的核心论点）因此被提前混淆。
- **证据（只引文档）**：§2.2 的 `get_acpi_rsdp` 全文（L350-L374）；§3.1（L1031）「C 的发现全在内核内……引导器只负责把 `kinfo` 填好」；§1.4 L129-L133 的表头「谁做的（C 侧）」与 ② 行「引导器（loader / boot-shim）」。
- **最小修复**：把 ② 行拆成两列或两行：「C：内核自己扫描（§2.2）；Rust：boot-shim 从配置表/`a1` 取地址（§4.2）」。若只想描述 Rust 链，把表头「（C 侧）」改成「（Rust 侧）」，并另给一张 C 的五段表。
- **置信度**：高
- **是否需源码核实**：否（§2.2 与 §3.1 已给出 C 侧事实）
- **若我错了**：若 C 的 loader 也会在 `kinfo` 里放一个 ACPI 指针、`get_acpi_rsdp` 只是兜底扫描，那么 ② 的「引导器」对 C 也部分成立；但 §2.2 的代码没有读 `kinfo`，§3.1 也说「全在内核内」。
- **命中清单**：§1.4 五段链；§2.2 `get_acpi_rsdp`；§3.1 解析职责切分。

## FINDING RD4-READER-C-05（P2）

- **位置+摘录**：§2.6（L783）：「`ApicDesc` 只表示**首个** IOAPIC……`nr_irqs` 是 IDT/IRQ 向量容量（常量 64），IOAPIC 引脚数由 `IOAPIC_NUM_PINS = 24` 常量给出。」§3.2（L1143）trait 注释：「/// Number of IRQ sources supported.」§3.3（L1178）表：「| 『有多少根 IRQ 线』 | trait 方法 `nr_irqs()`，经 vtable |」。
- **严重度**：中
- **问题**：同一个 `nr_irqs` 在三处被定义成三种东西：x86 的向量表容量 64、trait 的「支持的 IRQ 源数量」、§3.3 的「有多少根 IRQ 线」。x86 的真实引脚数是 24（`IOAPIC_NUM_PINS`），既不是 64 也不是 `nr_irqs`。
- **影响**：消费方按 §3.3 的「线数」语义使用 `nr_irqs` 时，会在 x86 上拿到 64 而不是 24；按 §2.6 的「向量容量」理解时，又会在 riscv64 上把它当成 PLIC 的 `ndev`（§4.5）。这是跨架构接口最容易埋错的地方。
- **证据（只引文档）**：§2.6 L783 的「`nr_irqs` 是 IDT/IRQ 向量容量（常量 64）」；§3.2 L1143 的「Number of IRQ sources supported」；§3.3 L1178 的「有多少根 IRQ 线」；§4.5 L1890 的 riscv64「线数取 `riscv,ndev` 属性」；§4.8 L2142 的 `(gicv3.nr_irqs as usize).min(NR_IRQ_VECTORS)`。
- **最小修复**：把 `nr_irqs` 统一解释为「IRQ 向量空间容量（消费方据此钳制）」，并在 §3.3 的表里把「有多少根 IRQ 线」改成「IRQ 向量空间有多大」；另起一行说明 x86 的 IOAPIC 引脚数是 `IOAPIC_NUM_PINS = 24`，不由 `nr_irqs` 表达（R04-T10 的多 IOAPIC 模型）。
- **置信度**：高
- **是否需源码核实**：否（三处定义都在文档内）
- **若我错了**：若本仓约定 `nr_irqs` 就是「容量」而 §3.3 的「线」只是口语，那么这是措辞问题；但 §3.2 的 trait 注释「Number of IRQ sources supported」把「源」与「容量」混在一起，仍建议改。
- **命中清单**：§2.6 IOAPIC；§3.2 `InterruptControllerDesc`；§3.3 分工表；§4.5 riscv PLIC；§4.8 钳制。

## FINDING RD4-READER-C-06（P2）

- **位置+摘录**：§4.9（L2195-L2198）`init_smp_state`：
  ```
  let bsp_logical = (0..topo.nr_cpus as usize)
      .find(|&i| topo.cpus[i].hw_id == bsp_hw)
      .unwrap_or(topo.bsp_id as usize) as u32;
  ```
  §4.9（L2220-L2222）`init_protection`：
  ```
  let bsp_logical = (0..topo.nr_cpus as usize)
      .find(|&i| topo.cpus[i].hw_id == bsp_hw)
      .unwrap_or(0) as u32;
  ```
- **严重度**：中
- **问题**：两处用同一个 `elected_bsp_hw_id` 做同一次选举，却在「找不到」时回退到不同的值：`init_smp_state` 回到固件的 `bsp_id` 提示，`init_protection` 回到逻辑 0。文档没有解释这个差异。若选举失败（`bsp_hw` 不在 `topo.cpus[0..nr_cpus]` 里，或 `nr_cpus == 0`），两条路径会把 BSP 逻辑号定成不同的 CPU。
- **影响**：§4.9 自己警告「若直接用槽位 0，在一台把非引导 CPU 记录排在前面的机器上，`gs:0x10` 会被编程成**另一颗 CPU**的逻辑号；`current_cpu_id()` 随后返回错误身份，per-CPU 数据……全部落到错槽位」（L2274）。两处不同的回退正好可能制造这个症状；而且 `init_protection` 的注释说「The BSP's logical id comes from the arch election, not from the firmware's first-record `bsp_id`」，但它的回退 `0` 也是固件式的默认。
- **证据（只引文档）**：两段代码的 `unwrap_or` 参数不同；§4.9 L2274 的故障描述；§4.10（L2306）说 `bsp_id`「经 SmpState 中转」，但 `init_protection` 是直接重算，不经过 `SmpState`。
- **最小修复**：二选一并写清理由：(a) 两处统一用 `topo.bsp_id` 或统一用 `0`；(b) 若 `init_protection` 必须在 `SmpState` 之前运行、只能用 0，则加一句「此处回退 0 是 Phase B 的限制；Phase D 的 `init_smp_state` 会用 `bsp_id` 重新对齐，且选举失败在 Phase B 已被 X 检查排除」。同时补一条「`elected_bsp_hw_id` 保证返回集合内成员」的不变量，或删掉回退分支。
- **置信度**：中高
- **是否需源码核实**：是（确认两段节选与源码一致，以及 `elected_bsp_hw_id` 是否可能失败）
- **若我错了**：若 `elected_bsp_hw_id` 的实现保证永远返回 `topo.cpus` 中的成员（例如它自己就带 fallback），那么两个 `unwrap_or` 都是死分支，问题降为「死代码/注释不一致」；仍需说明。
- **命中清单**：§4.9 `init_smp_state`；§4.9 `init_protection`；§4.10 `bsp_id`；BSP 选举。

## FINDING RD4-READER-C-07（P2）

- **位置+摘录**：头部（L13）：「本篇的 Rust 实现相对 Minix3 C 属 Rewrite（保持外部行为）与 Refactor（不改语义的重构）；**riscv64 的整套平台发现是 Architectural Evolution**（Minix3 没有 RISC-V 版本），涉及处标注 `[ARCH: K-2]`。」§1.6（L172-L175）：「aarch64 的设备树路径（`parse_gic`、GICv3、PSCI 通道） | **无**——C 内核没有 aarch64 版本 | **未覆盖（无 C 源）**」；「aarch64 的 ACPI 路径（GICC/GICD/GICR 记录、`check_gic_madt`） | **无**——C 的 ACPI 解析器是 i386 专属…… | **未覆盖（无 C 源）**」；§1.6（L177）：「earm 与 Rust 的 aarch64 是两回事……后者的发现链在 C 树里没有任何先例。」
- **严重度**：中
- **问题**：按文档自己的标准（「Minix3 没有该架构版本」⇒ Architectural Evolution），aarch64 的整条发现链同样没有 C 先例，却没有被归入 ARCH；头部只把 riscv64 列为 ARCH。aarch64 是 Rewrite（保持外部行为）还是 ARCH（新行为）在文档里没有答案。
- **影响**：文档头部用三档术语（Rewrite/Refactor/ARCH）给全篇定调；aarch64 的归类缺口使这个框架在本文内不闭合。读者无法判断 aarch64 的 DTB/ACPI 路径该按哪一档审阅；后续 §5.4 的「无 C 源分支」覆盖依据也建立在这个分类上。
- **证据（只引文档）**：L13 的「riscv64 的整套平台发现是 Architectural Evolution」；L172-L175 的 aarch64 两行「无——C 内核没有 aarch64 版本」；L177 的「后者的发现链在 C 树里没有任何先例」。
- **最小修复**：二选一：(a) 把 aarch64 的发现链也标为 Architectural Evolution（给出相应 `[ARCH: ...]` 标签），并在头部说明它为什么与 riscv64 分开；(b) 若 aarch64 被归入某条既有的 Rewrite 契约（例如「重表达 earm 的板级语义」），在头部写出这条契约，并解释为何「无 C 先例」仍算 Rewrite。
- **置信度**：高
- **是否需源码核实**：否（分类矛盾在文档内）
- **若我错了**：若项目另有一份架构决策把 aarch64 预先定义为「Rewrite 目标架构」，而 riscv64 是「ARCH 新增」，那么头部应引用那份决策；当前只给了 riscv64 的理由。
- **命中清单**：头部三档术语；§1.6 覆盖边界；§5.4 覆盖依据。

## FINDING RD4-READER-C-08（P2）

- **位置+摘录**：头部（L13）：「riscv64 的整套平台发现是 Architectural Evolution……**涉及处标注 `[ARCH: K-2]`**。」全文检索：`[ARCH: K-2]` 只出现在 L13（承诺本身）与 L174（§1.6 表里括号提到「头部术语档的 `[ARCH: K-2]`」）；riscv64 的具体章节（§3.2 的 SSWI 能力、§4.5 的 PLIC/CLINT/ACLINT、§4.8 的 `TIMER_IRQ`、§4.11 的 riscv 列）都没有这个标记。相反，L2161 出现了一个头部未提及的 `[ARCH: mmio-device]`。
- **严重度**：中
- **问题**：头部承诺「涉及处标注」，实际只有两处元提及；一个未在头部声明的 `[ARCH: mmio-device]` 突然出现。读者无法按标签定位 riscv64 的 ARCH 内容，也无法从头部解码 `mmio-device`。
- **影响**：头部承诺「涉及处标注」；如果这条承诺要可审计，至少要在 riscv64 的实质段落出现标签。当前只在头部与覆盖表各出现一次，审计时无法确认哪些段落属于 ARCH。`[ARCH: mmio-device]` 与 `[ARCH: K-2]` 是否同一套编号、是否都按「涉及处标注」处理，也没有说明。
- **证据（只引文档）**：L13 的承诺；L174 的括号提及；L2161 的 `[ARCH: mmio-device]`；§4.5（L1820-L1921）的 riscv64 解析无标签；§4.8（L2163-L2178）的 `TIMER_IRQ` 无标签；§4.11（L2329-L2337）的 riscv 列无标签。
- **最小修复**：在 riscv64 的第一处实质内容（§3.2 的 `sswi_setip_base` 或 §4.5 的 `parse_plic`）加 `[ARCH: K-2]`，并在 §4.11 的 riscv 列加一次总标；在头部的术语说明里补一句「`[ARCH: mmio-device]` 是另一条 ARCH 编号，解码见 README」。
- **置信度**：高（检索结果确定）
- **是否需源码核实**：否
- **若我错了**：若「涉及处」被定义为「首次定义处」而不是「每次出现处」，那么 L174 可以算首次定义；但它只是覆盖表里的一格，不是 riscv64 发现的定义段。仍建议把标签放到实质段落。
- **命中清单**：头部三档术语；§3.2/§4.5/§4.8/§4.11 的 riscv64 内容；`[ARCH: mmio-device]`。

## FINDING RD4-READER-C-09（P2）

- **位置+摘录**：§4.4（L1711）：「失败分别报 `BadRsdpChecksum` / `BadRootSignature` / `RootTableMalformed` / `BadChecksum` / `BadMadtSignature` / `MadtTooLong`（`os/libs/minix-platform/src/acpi.rs:enum AcpiParseError`）。」同节 L1726 的代码又返回 `AcpiParseError::MadtTooShort`；L1792-L1800 的 `check_gic_madt` 又返回 `GicdNotFound` / `GicVersionUnsupported` / `GicrNotFound`。
- **严重度**：低
- **问题**：L1711 用「失败分别报」列出六个变体，读者会把它当成 `AcpiParseError` 的完整清单；但同一节的代码随后展示了至少四个不在清单里的变体（`MadtTooShort`、`GicdNotFound`、`GicVersionUnsupported`、`GicrNotFound`）。
- **影响**：读者按清单写匹配或写测试时会漏掉四个错误；§4.4 的「失败分别报」也因此显得与本节代码不一致。
- **证据（只引文档）**：L1711 的六项清单；L1726 的 `return Err(AcpiParseError::MadtTooShort);`；L1793-L1799 的三个 GIC 错误。
- **最小修复**：把 L1711 改成「根表/MADT 的闸分别报 [六项]；另有 `MadtTooShort` 与 GIC 决策表的 `GicdNotFound` / `GicVersionUnsupported` / `GicrNotFound`，见下文」，或列出完整变体表。
- **置信度**：高
- **是否需源码核实**：是（确认 `AcpiParseError` 的完整变体集合）
- **若我错了**：若 L1711 的「失败」被限定为「根表/MADT 的四道闸」，那么清单不必包含 GIC 决策表的错误；但 `MadtTooShort` 仍属于 MADT 闸，却被漏掉。
- **命中清单**：§4.4 ACPI 校验；`AcpiParseError`；§4.4 GIC 决策表。

## FINDING RD4-READER-C-10（P2）

- **位置+摘录**：§3.8（L1457）：「把 ISA 字面量也搬进描述符会坏在哪：若给 `PitDesc` 加一个 `port_base: u16`，**三个来源的每一个**都要填这个字段，而三个值必然相同（`0x40`/`0x43`）。」
- **严重度**：低
- **问题**：`PitDesc` 是 x86-64 的品牌 struct（§3.3 L1170），而 `PlatformDescEnum::DeviceTree` 变体被 `#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]` 门控（§3.2 L1101-L1103）。在 x86-64 上只有 `Acpi` 与 `QemuVirt` 两个来源能构造 `PitDesc`，不存在「三个来源的每一个」。
- **影响**：反事实的代价被夸大；熟悉 `#[cfg]` 门控的读者会停下来核对枚举的变体数，发现「三个来源」与 §3.2 的 cfg 表不一致。
- **证据（只引文档）**：§3.2 L1101-L1109 的三个变体与各自的 `#[cfg]`；§3.3 L1170 的 x86-64 `PitDesc`；§3.8 L1457 的「三个来源的每一个」。
- **最小修复**：改成「x86-64 上能构造 `PitDesc` 的两个来源（ACPI 与 QemuVirt）都要填这个字段」，或泛指「每个能构造 `PitDesc` 的来源」。
- **置信度**：高
- **是否需源码核实**：否（cfg 门控在文档内）
- **若我错了**：若 `PitDesc` 也在 aarch64/riscv64 的 `QemuVirtDesc` 里作为占位字段出现，那么「三个来源」可能成立；§3.3 的字段表把 `PitDesc` 只列在 x86-64 行，未见其他架构使用。
- **命中清单**：§3.2 `PlatformDescEnum` cfg；§3.3 品牌 struct 表；§3.8 常量迁移判据。

## FINDING RD4-READER-C-11（P2）

- **位置+摘录**：§4.10（L2310）`board_id` 行：「| `board_id` | `kmain`（`get_board_id_by_name(env_get(BOARDVARNAME))`） | 内核内各 `BOARD_IS_*` 分支 | **淘汰**：Rust 侧无板卡标识；板级差异由描述符字段承载…… | `os/libs/minix-platform/src/arch/mod.rs` |」；同节 L2285-L2298 的 `MachineStruct` 仍有 `board_id: u32`，L2256 说「六个字段里填四个……`padding` 与 `board_id` 用 `..Default::default()`（全零）」。
- **严重度**：低
- **问题**：`board_id` 被标为「淘汰」，但 ABI 结构里仍保留该字段并填 0；同表的 `padding` 明确写「保留位置、淘汰语义」，`board_id` 没有这层限定。读者会以为 `MachineStruct` 里已经删掉 `board_id`。
- **影响**：`GET_MACHINE` 的 ABI 兼容性是本节的核心；「淘汰」与「保留位置」是两种不同的兼容策略，读者无法判断 `board_id` 属于哪一种。
- **证据（只引文档）**：L2285-L2298 的 `MachineStruct` 含 `board_id: u32`；L2256 的「六个字段里填四个」；L2307 的 `padding` 行「保留位置、淘汰语义」；L2310 的 `board_id` 行只写「淘汰」。
- **最小修复**：把 `board_id` 的处置改成「**淘汰语义、保留 ABI 位置**（恒 0，同 `padding`）」，或明确写「从结构里删除（若 ABI 允许）」。
- **置信度**：高
- **是否需源码核实**：否（文档内代码与表已不一致）
- **若我错了**：若「淘汰」在本节的定义就是「语义淘汰、位置保留」，那么 `padding` 行是多余的解释；但两行用了不同措辞，仍应统一。
- **命中清单**：§4.10 `MachineStruct`；`GET_MACHINE` ABI；`padding`/`board_id` 处置。

## FINDING RD4-READER-C-12（P2）

- **位置+摘录**：§4.8（L2100）：「x86-64 的 `acpi_tables`（`os/arch/src/x86_64/arch_init.rs:struct X86_64ArchInit` 收下，字段标 `#[allow(dead_code)]`，**当前无消费者**）」；§4.10（L2309）`acpi_rsdp` 行：「`ArchMiscDesc.acpi_tables` 在 `os/libs/minix-platform/src/acpi.rs:fn parse` 里填 `Some(rsdp_phys)`；**`getinfo_machine` 把它填进 `MachineStruct.acpi_rsdp`**」。
- **严重度**：低
- **问题**：两处都说 `acpi_tables`，但指的可能是不同对象：§4.8 是 `X86_64ArchInit` 的实例字段（无消费者），§4.10 是 `ArchMiscDesc` 的描述符字段（被 `getinfo_machine` 读）。文档没有区分这两层，读者会直接看到「无消费者」与「被 getinfo_machine 消费」的矛盾。
- **影响**：`ArchInit` 的 `acpi_tables` 是否真的无用，关系到「架构初始化是否需要 ACPI 表」这一设计判断；如果读者按 §4.8 认为它无用，就会忽略 §4.10 的发布路径。
- **证据（只引文档）**：L2100 的「当前无消费者」；L2309 的「`getinfo_machine` 把它填进 `MachineStruct.acpi_rsdp`」；§4.9 L2237-L2240 的 `getinfo_machine` 代码确实读 `platform_desc().arch_misc().acpi_tables`。
- **最小修复**：在 §4.8 的括号里补一句「这里的『无消费者』指 `X86_64ArchInit` 的实例字段；`ArchMiscDesc.acpi_tables` 由 §4.10 的 `getinfo_machine` 消费」。
- **置信度**：高
- **是否需源码核实**：否（文档内两条路径已给出）
- **若我错了**：若 `X86_64ArchInit` 的字段与 `ArchMiscDesc.acpi_tables` 是同一个值、同一个消费者，那么 §4.8 的「无消费者」就是错的；无论哪种，两处需要统一。
- **命中清单**：§4.8 `ArchInit`；§4.9 `getinfo_machine`；§4.10 `acpi_rsdp` 发布。

## SUGGESTION

## FINDING RD4-READER-C-13（P3）

- **位置+摘录**：§1.4（L122）步骤 ⑤ 的注记：「早期控制台用 COM1 常量、**本地定时器用 `lapic_addr` 常量**，都不在此列，见 §1.5、§2.5。」§2.5（L727）：「C 的活路径里，`lapic_addr` 的**物理基址**唯一来源是 `LOCAL_APIC_DEF_ADDR`；进入分页之后该变量改存 VM 映射返回的虚拟地址（`memory.c:954` 的 `lapic_addr = lapic_addr_vaddr`……）。」
- **严重度**：低
- **问题**：`lapic_addr` 是一个全局变量，不是常量；它的值在分页后从物理基址改成 VM 映射返回的虚拟地址。把它叫作「`lapic_addr` 常量」与 §2.5 的精确区分（唯一来源是常量 `LOCAL_APIC_DEF_ADDR`，变量后来改存 VA）冲突。
- **影响**：读者会把「本地定时器用常量」理解成「编译期固定」，从而忽略 §2.5 的 VA 重映射；对排障（§2.5 说「要看映射请求而不是变量现值」）不利。
- **证据（只引文档）**：L122 的「`lapic_addr` 常量」；L727 的「`lapic_addr` 的物理基址唯一来源是 `LOCAL_APIC_DEF_ADDR`……进入分页之后该变量改存 VM 映射返回的虚拟地址」。
- **最小修复**：改成「本地定时器用 `LOCAL_APIC_DEF_ADDR` 常量（经 `lapic_addr` 变量，分页后改存 VA）」。
- **置信度**：高
- **是否需源码核实**：否
- **若我错了**：若 `lapic_addr` 是一个 `#define` 而 §2.5 的赋值是另一变量，那么 §2.5 需要改名；但 §2.5 明确称它为「变量」。
- **命中清单**：§1.4 步骤 ⑤；§2.5 LAPIC 基址；§2.1 消费者表。

## FINDING RD4-READER-C-14（P3）

- **位置+摘录**：§4.8（L2115）：「……避免被 `mmio_translate` 的越界 **const-assert** 抢先 panic 成泛化的『address outside the mapped MMIO window』。」§3.5（L1310）：「aarch64 还会先过一次 `mmio_translate`，`os/libs/minix-platform/src/arch/aarch64.rs:fn mmio_translate` 的 **const 断言**会因地址落在 `0x0800_0000..0x0A00_0000` 窗口内而通过。」
- **严重度**：低
- **问题**：`mmio_translate` 的输入是描述符里的运行时地址；`const` 断言只能在编译期对常量求值，不能对运行时值「抢先 panic」。这里的检查要么是运行期 `assert!`，要么是 `const fn` 里对常量参数的编译期检查 + 运行期检查。两种机制被混写成「const-assert / const 断言」。
- **影响**：熟悉 Rust 的读者会认为这个检查在编译期完成，从而低估运行时开销或误判「描述符地址为 0」的失败路径；§4.8 的「三个零基址断言排在 `mmio_translate` 之前」也因此显得像在躲避一个不存在的编译期错误。
- **证据（只引文档）**：L2115 的「const-assert 抢先 panic」；L1310 的「const 断言会因地址落在……窗口内而通过」；L2157 的「`mmio_translate` 的断言信息是『mmio_translate: address outside the mapped MMIO window』」。
- **最小修复**：确认实现后统一措辞：若是运行期检查，写「运行时断言」；若同时有编译期常量检查与运行期检查，分开写「编译期常量断言（针对固定窗口常量）＋运行时地址检查」。
- **置信度**：中
- **是否需源码核实**：是（确认 `mmio_translate` 是 `const fn` 还是普通 `fn`，以及断言是 `const { assert! }` 还是 `assert!`）
- **若我错了**：若 `mmio_translate` 只被常量调用（例如窗口常量在编译期展开），那么「const 断言」是对的；但 §3.5 说的是用 QemuVirtDesc 的字段（运行期结构体）调用它，输入不是编译期常量。
- **命中清单**：§3.5 兜底代价；§4.8 aarch64 `new()`；地址三语义。

## FINDING RD4-READER-C-15（P3）

- **位置+摘录**：§1.4（L122）：「SMP 构建下 `apic_single_cpu_init` **并入 `smp_init` 路径**。」§4.1（L1520）重复：「APIC 初始化（`apic_single_cpu_init`，非 SMP 构建；SMP 构建下并入 `smp_init` 路径，§2.1）。」§2.7（L800-L801）的表：「非 SMP | …… | `apic.c` 的 `apic_single_cpu_init` 内，由它自己发起（守卫是 `#if defined(USE_APIC) && !defined(CONFIG_SMP)`）」；「SMP | …… | `arch_smp.c` 的 `smp_init` 内，由它自己发起」。
- **严重度**：低
- **问题**：按 §2.7 的守卫，SMP 构建根本不编译/不调用 `apic_single_cpu_init`；`smp_init` 是「自己发起」一次 `acpi_init`，不是「把 `apic_single_cpu_init` 并入」。「并入路径」这个说法会让读者去找 `apic_single_cpu_init` 在 SMP 下的调用点。
- **影响**：§2.1 的「发现与第一个消费者相邻」在 SMP 下不成立（消费者换成 `smp_init`），但读者可能以为只是同一函数换了个调用者；§2.7 的表已经给出正确答案，两处措辞不一致。
- **证据（只引文档）**：L122 与 L1520 的「并入 `smp_init` 路径」；L800-L801 的守卫与「由它自己发起」；§2.1（L231）的 `arch_init` 节选里 `apic_single_cpu_init` 的守卫是 `#if defined(USE_APIC) && !defined(CONFIG_SMP)`。
- **最小修复**：改成「SMP 构建不调用 `apic_single_cpu_init`；`smp_init` 自己发起一次 `acpi_init`，消费点相应换成 `smp_init`」。
- **置信度**：高
- **是否需源码核实**：否（文档内守卫已给出）
- **若我错了**：若 `smp_init` 内部真的调用 `apic_single_cpu_init`（而 `apic.c` 的守卫只包住 `arch_init` 里的那一次调用），那么「并入」是对的；但 §2.7 说 `arch_smp.c` 的 `acpi_init`「由它自己发起」，不是调用 `apic_single_cpu_init`。
- **命中清单**：§1.4 步骤 ⑤；§2.1 `arch_init`；§2.7 调用点表；§4.1 C/Rust 时序表。

# 收尾：已核对为自洽的怀疑（未成立，不计 FINDING）

以下是我在 C 段逐条验证过、但**没有**发现文档内部矛盾的候选，列出来供协调者判断是否需要升格为别的角色（FACT / DESIGN）：

1. §2.3 的容量算术：`36 + 35 × 4 = 176`，`(-1 - 36)/4 = 1073741814`（32 位无符号），`i = 35` 首次越界——三处算术自洽。
2. §4.4 的 `check_gic_madt` 八行决策表与函数体逐行对应（GICD 优先、版本 1/2 拒绝、GICR 缺失拒绝、版本 0 放行）。
3. §4.4 的 RSDP/XSDT/MADT 长度：RSDP rev2 36 字节、MADT 头 44 字节、GICC 80 字节、GICR 16 字节——与文档给出的偏移一致。
4. §4.6 的 CLINT 算术：`0x0200_0000 + 0xBFF8 = 0x0200_BFF8`，`+ 0x4000 = 0x0200_4000`；`mtimecmp_stride = 8` 与 `QemuVirtDesc::default` 的循环一致。
5. §3.2 的 25 条计数：8 个转发方法中 7 个三分支（21）＋ `psci_conduit` 一臂（1）＋ `Debug` 三条（3）＝ 25；不含 `_ => None` 则 26，与 §6.2 #1 的口径说明一致。
6. §5.1 的 26 条测试：9（acpi）＋4（device_tree）＋4（x86_64）＋4（riscv64）＋3（aarch64）＋1（kind）＋1（global）＝ 26。
7. §5.4 的 M1–M11 与 §5.3 的三条缺口、R04-T01–T11 的对应关系（M1→T05、M2→T08、M3→T01、M4→T02、M6→T03、M7→T04、M8→T06、M9→T07）自洽。
8. §6.1 的六行诊断表按判据归四类（1&6、2、3、4&5）与表后文字一致。
9. §2.7 的三个 `acpi_init` 调用点行号（`arch_system.c:264`、`arch_smp.c:315`、`apic.c:1141`）与 §6.2 #5 的命令输出一致。
10. §4.2 的两组 DTB GUID（`b1b621d5-…` / `b1b621d2-…`）在 §4.2 表与 §5.2 实测段逐字一致。

# 给协调者的三行摘要

1. **必须先处理的因果错误**：C-01（§2.4 删 `flags` 检查的反事实结论与所引代码相反：`ncpus` 恒 0，不是「停在已计入项」）与 C-02（§5.3/§6.2 的 53 与分布表/正文的 52 矛盾）。
2. **必须先补的证据缺口**：A-04（§2.1 节选没有 `acpi_init`/`detect_ioapics` 两行）、A-05（§2.3 节选没有表头预读，`tb` 看似未初始化）、B-04（§6.2 #4 的 `head -3` 输出五行）、B-05（「全量即 §6.2 #4」落空）。
3. **最值得设计轴复核的一条**：C-06（`init_smp_state` 与 `init_protection` 对同一次 BSP 选举用了不同的 `unwrap_or` 回退；若选举失败，两条路径会把 BSP 逻辑号定成不同 CPU，正好触发 §4.9 自己警告的 `gs:0x10` 错槽位）。

