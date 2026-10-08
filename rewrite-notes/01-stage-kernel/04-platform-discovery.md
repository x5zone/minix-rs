# 04 · 平台发现：内核从哪儿知道自己跑在哪块板子上

> **C 真值源**：`minix3/minix/kernel/arch/i386/acpi.c`、`minix3/minix/kernel/arch/i386/pre_init.c`、`minix3/minix/kernel/glo.h`
> **Rust 实现**：`os/libs/minix-platform/src/{global.rs,kind.rs,acpi.rs,device_tree.rs,qemu_virt.rs,desc.rs}`、`os/libs/minix-boot/src/platform.rs`、`os/libs/minix-types/src/types/cell.rs`、`os/kernel/src/lib.rs`

05 篇（时钟与中断初始化）要配中断控制器、要定定时器，两件事都要先知道一堆数字：控制器寄存器在哪、有几个核、串口是哪个地址。这些数字不在代码里，也不该在代码里。这一篇处理"内核怎么拿到它们"，以及拿到之后**为什么必须一次形成、之后只读**。

---

## 1. 概述

### 1.1 内核怎么知道自己跑在哪

有三类信息要被问出来：**设备寄存器的地址**（中断控制器、定时器、串口）、**拓扑**（几个核、怎么给核发中断）、**关机方式**。它们的来源只有两类：

- **固件给的表**。统一可扩展固件接口机型上是一张张按签名标识的系统描述表，其中多源描述表列出中断控制器的地址与核；设备树机型上是一棵节点树，地址与兼容性串都在里面。
- **编译期写死的假设**。板级表、常量布局。它在受控环境里总是对的，在真机上可能整体就是错的。

这两类都存在于本项目的代码里。先接受这个事实，后面的设计（谁优先、失败怎么办）才有落点。

### 1.2 这份信息有几个读者

一旦拿到，读它的就有：中断控制器初始化、定时器编程、串口驱动、关机路径、多核启动时每个核的编号来源。它们在时间上分散、在 CPU 上也可能分散。

于是有一个约束不必论证：**这些值形成之后就不能再变**。否则先拿到引用的读者与后拿到的读者看到的不是同一个世界，而且这种不一致通常表现为"某些时候能跑"。

### 1.3 机制图

```
引导侧递来一份来源清单（每一项 = 一个类型标签 + 一个物理地址）
  │
  ▼
按清单顺序逐个试解析 ──成功──▶ 得到一份平台描述
  │                                   │
  └────全部失败                        │
        │                              ▼
        │                     冻结一次（此后只读）
        ▼                              │
   按构建档位决定：                            │
   开发构建 → 用一份假设布局             ▼
   发布构建 → 中止启动            所有 CPU、所有驱动只读同一份
```

图上有两个节点是本篇的主题：**"先成者胜"的试解析**（§3.3）与**"冻结一次"的生命周期**（§3.4）。第三个分支——兜底还是中止——容易就被当成工程细节，实际是产品决定（§3.5）。

### 1.4 范围与可以跳过的内容

- **拿到地址之后怎么配控制器、怎么编程定时器**：属 05 篇。本篇只到"描述可用"。
- **辅助处理器何时唤醒、编号怎么分配**：属多核篇。本篇只用它的一条前提：冻结必须早于第一个辅助处理器被唤醒（§4.1）。
- **ACPI 表的完整规范细节、设备树节点的全部属性**：读代码时按段落引用，不必通读规范；本篇只讲被用到的部分。

---

## 2. C 源码分析

### 2.1 板卡身份是常量

`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters` 里有两行：

```c
        /* let higher levels know what we are booting on */
        mb_set_param(cbi->param_buf, ARCHVARNAME, (char *)get_board_arch_name(BOARD_ID_INTEL), cbi);
	mb_set_param(cbi->param_buf, BOARDVARNAME,(char *)get_board_name(BOARD_ID_INTEL) , cbi);
```

参数键值对是留给上层服务的（内核自己不看）。实参才是重点：`BOARD_ID_INTEL`。x86 这条路径上，**板子是什么是写死的**，名字也是从一张编译期表里查出来的。

全局还有一份 `extern struct machine machine;`，`minix3/minix/kernel/glo.h` 给它的注释是 "machine info for services"——机器信息与板级身份同样是"填好给人读"的数据，而不是"解析出来"的结果。

### 2.2 固件的表：`acpi.c` 到底做了什么

`minix3/minix/kernel/arch/i386/acpi.c` 的函数清单就界定了它的能力范围（**签名示意**，按定义顺序列名）：

```
acpi_check_csum          对一张表求字节和，验证是否为零
acpi_check_signature     比较表签名
acpi_phys2vir            把物理地址转成当前可用地址
acpi_phys_copy           从物理地址抄一段
acpi_read_sdt_at         在给定物理地址读一张系统描述表
acpi_get_table_base      按签名找表的基址
acpi_get_table_length    按签名找表的长度
acpi_madt_get_typed_item 在多源描述表里按类型挑第 idx 个条目
acpi_rsdp_test / get_acpi_rsdp   找到并确认表根
acpi_init_poweroff       为关机找出所需信息
acpi_init                整体入口
```

四个动作概括完这条链：**找根、验校验和、按签名取表、按类型挑条目**。没有命名空间、没有字节码执行，所以别把它当一个完整 ACPI 实现读——它服务于"取几个地址"。

### 2.3 校验和，以及"这件事必须在某个时刻之前做完"

`acpi_check_csum`（函数体全文）：

```c
static int acpi_check_csum(struct acpi_sdt_header * tb, size_t size)
{
	u8_t total = 0;
	int i;
	for (i = 0; i < size; i++)
		total += ((unsigned char *)tb)[i];
	return total == 0 ? 0 : -1;
}
```

表由固件写在内存里、由内核读。中间可能被搬动、被截断、版本对不上。校验和的做法朴素到可以口算：整张表的字节和应为零。任何一项不符就不认这张表。**这是"来自别人的数据先验证再用"的最小版本**，与 01 篇那个魔数断言是同一类前提检查。

同一文件里还有两处读法直接相关（各节选）：

```c
static u32_t acpi_phys2vir(u32_t p)
{
	if(!vm_running) {
		DEBUGEXTRA(("acpi: returning 0x%lx as vir addr\n", p));
		return p;
	}
	panic("acpi: can't get virtual address of arbitrary physical address");
}
```

**它读的是任意物理地址。** 能这么做的唯一条件是：此刻"虚拟地址数值等于物理地址"对内核关心的范围成立，也就是 01、02 篇铺下的恒等段还在。函数用 `vm_running` 把这个前提写成了一句断言：一旦虚拟内存服务接管了地址空间，读任意物理地址这件事就不可能了，于是它选择立刻中止，而不是返回一个看起来能用的地址。

这条约束回答了 03 篇末尾那次页表重建留下的疑问：为什么内核在把内存管理权交出去之前，还要特意保留恒等段——**有一类固件表只能在这个窗口里读**。

### 2.4 按类型挑条目：变长遍历

`acpi_madt_get_typed_item`（函数体全文）：

```c
static void * acpi_madt_get_typed_item(struct acpi_madt_hdr * hdr,
					unsigned char type,
					unsigned idx)
{
	u8_t * t, * end;
	int i;

	t = (u8_t *) hdr + sizeof(struct acpi_madt_hdr);
	end = (u8_t *) hdr + hdr->hdr.length;

	i = 0;
	while(t < end) {
		if (type == ((struct acpi_madt_item_hdr *) t)->type) {
			if (i == idx)
				return t;
			else
				i++;
		}
		t += ((struct acpi_madt_item_hdr *) t)->length;
	}

	return NULL;
}
```

三点读法：

**遍历的边界是表头里的长度字段**，推进步长是**每个条目自己声明的长度**。于是这张表可以混放不同类型的条目、条目大小还不一致——这正是"变长条目遍历"，与 01 篇 2.3 那份内存映射的推进式（`本条目地址 + 本条目大小 + 那个大小字段的宽度`）是同一类形状，只是这里步长写在条目自己身上。

**先按类型筛，再数第几个。** 调用方用它取两类条目：本地高级可编程中断控制器与 I/O 可编程中断控制器（`acpi.c` 里传的是 `ACPI_MADT_TYPE_LAPIC` 与 `ACPI_MADT_TYPE_IOAPIC`）。同一类型可以出现多次（多核、多个控制器），所以要带一个序号。

**没找到返回 `NULL`。** 由调用方决定怎么处理——这也是为什么这份代码必须"一次读全、之后交给别人用"，而不是每个使用者自己翻表：翻表逻辑（含边界与步长）只有一份。

### 2.5 归纳：C 侧的形状

把 §2.1–§2.4 放在一起，C 的"认识硬件"是这样：

- 板卡身份与机器信息：编译期常量 + 一个供服务读取的全局。
- 设备地址：一次 ACPI 表读取，动作有限（找根、验和、取表、挑条目），并且只能在虚拟内存服务接管之前的窗口里做。
- **产物没有统一形状**：控制器地址进了高级可编程中断控制器的私有结构、关机信息进了 ACPI 自己的关机路径、板名进了参数缓冲。谁要用，谁自己去拿；也没有"现在还没解析出来"这个状态被显式表达。

第 3 章要处理的就是这两件 C 没有的东西：**统一的读接口**与**显式的形成时机**。

---

## 3. Rust 设计决策

### 3.1 三类来源，一份读接口

**C 里"表"与"常量"是两条路，产物形状不同**（§2.5）。重写要让消费方不关心描述从哪来，于是有了 `os/libs/minix-boot/src/platform.rs:trait PlatformDesc`——它规定"一块平台要能回答哪些问题"（中断控制器描述、定时器描述、控制台描述、拓扑、来源标记），再配三个子接口 `trait InterruptControllerDesc`、`trait TimerDesc`、`trait ConsoleDesc`（同文件）。

三个实现各自对应一条来源：`os/libs/minix-platform/src/acpi.rs:struct AcpiDesc`（从表解析）、`os/libs/minix-platform/src/device_tree.rs:struct DeviceTreeDesc`（从设备树解析）、`os/libs/minix-platform/src/arch/x86_64.rs:struct QemuVirtDesc`（按假设构造，另有 `arch/aarch64.rs` 一份）。**统一的是读接口，不是构造方式**——本章其余决策都建立在这条区分上。

兜底那份不是三架构共用一张表：它按架构各有一份（布局常量本就不同），`os/libs/minix-platform/src/qemu_virt.rs` 只做按架构再导出。也就是说，**连"用假设值兑现一份描述"这件事，假设内容也是架构相关的**。

一条文件内的说明记一下：`os/libs/minix-platform/src/desc.rs` 现在只做再导出，它的文件头说明"早期版本用枚举把三种来源并列，后来换成接口"。这个转变与 §3.3 那条策略直接相关。

### 3.2 递标签加地址，不递解析器

**C 的表读取直接发生在内核里，用物理地址硬读**（§2.3）。64 位重写里"能读固件内存"的一方是引导垫片，而解析器要运行在内核侧，于是"有哪些来源"必须跨镜像传递。

规则在 01 篇 §3.1 立过：跨边界只能是值。落到这里就是两个小类型（`os/libs/minix-boot/src/platform.rs:struct PlatformDescKind` 是一个包着 `u32` 的新类型；`struct PlatformDescSource` 是"一个标签 + 一个物理地址"），并且标签的取值是常量：`os/libs/minix-boot/src/platform.rs:const DTB`（值 1）与 `const RSDP`（值 2）。

分工因此变成三块，各自独立：

```
引导垫片        往清单里放 (标签, 地址)         —— 它知道固件给了什么
minix-boot      定义标签与来源结构                —— 两侧共同的词汇
minix-platform  fn parse_by_kind 按标签分发解析器  —— 它知道怎么解析
```

带来的实际好处写在 `os/libs/minix-platform/src/kind.rs` 的文件头：新增一种固件表格式，只需三步——在 `minix_boot::platform` 加一个标签常量、在 `parse_by_kind` 加一条分发分支、新建一个解析模块；而 `KernelInfo`、引导垫片、`init_from_kinfo` 三处**一行都不用改**。

对比 C 的做法（要加一类信息就往参数缓冲或全局里塞一笔，再让各个读者自己去读），差别不在"能不能扩展"，而在**扩展点收敛到一处**。

### 3.3 先成者胜，解析失败就往下试

**C 只有一个来源可试**（它直接找 ACPI 表根；ARM 端口则是编译期定死的板级布局）。重写手上同时可能有两三项（比如 ARM64 既给了设备树又给了接口表），于是要定一条策略。`os/libs/minix-platform/src/global.rs:fn init_from_kinfo`（节选）：

```rust
    let mut parsed: Option<PlatformDescEnum> = None;
    for source in kinfo.platform_sources {
        match unsafe { parse_by_kind(*source) } {
            Ok(desc) => {
                parsed = Some(desc);
                break;
            }
            Err(_e) => {
                // Continue to next source on parse failure.
            }
        }
    }

    let desc = match parsed {
        Some(d) => d,
        None => qemu_fallback_or_panic("no platform source parsed successfully"),
    };
```

三条读法：

**顺序由引导侧决定**——清单本来就由它排（01 篇 §3.6 说过 ARM64 上"设备树优先、接口表回落"这个偏好写在哪里）。解析器不问"我该不该赢"，这是策略与机制的分界。

**失败是往下试，不是致命错误**。一块机器同时给两种描述、其中一种残缺是常见情况；把任何一种解析失败都当致命，等于让最不可靠的一方决定能不能开机。

**这里为什么不是枚举匹配**：3.1 提到早期版本用枚举把三种来源并列。那种形状下，消费方要么按类型分支、要么把整份描述复制出来，而新增来源会波及所有分支点。换成"先成者胜 + 只读接口"之后，来源种类只在分发那一个 `match` 里出现，其余代码只认接口。

### 3.4 形成一次、之后只读，而且这件事要能被检查

**C 侧靠约定**：全局填一次、之后大家读（§2.1 的 `machine`），机制上没有任何东西阻止有人中途再写一次。

重写要让这个约定变成类型系统管得住的东西。承载它的是 `os/libs/minix-types/src/types/cell.rs` 里的 `Frozen<T>`：一个格子，允许**一次**写入并带释放语义的发布，之后所有读者以获取语义看到同一个不再变化的值，交出的还是共享引用。用它的原因是三条否决：

| 选项 | 为什么不用 |
|---|---|
| `static mut` | 每次访问都要 `unsafe`，而且不额外提供任何保证——它不阻止第二次写 |
| 加锁 | 之后根本没有写者，锁把只读串行化了，是纯开销；语义也讲错：锁保护的是"会变的东西" |
| `OnceLock` | 需要标准库与分配器，引导期拿不到 |

还有一条**曾经用错过**的形状，它是这类代码最隐蔽的失败方式。这个格子早期是一个"单线程环境的可变格子 + 手写 `Sync"`，安全论证写的是"永远不会并发访问"。这个前提在辅助处理器被唤醒之前成立，唤醒之后自动为假——**而类型系统不会提醒你，因为 `Sync` 是你手写的**。文件里的说法是：它承诺的比多核实际给的少。换成 `Frozen` 之后，"写一次、读多次"是类型的性质，不再是一句需要读者自行核对的注释。

实现本身只有两样东西（`os/libs/minix-types/src/types/cell.rs` 里的 `Frozen<T>`，字段定义逐字）：

```rust
pub struct Frozen<T> {
    value: core::cell::UnsafeCell<core::mem::MaybeUninit<T>>,
    frozen: core::sync::atomic::AtomicBool,
}
```

一块可能被写的区域，加一个"是否已冻结"的原子布尔：写的那一次用释放语义发布，之后的读者用获取语义看见它，取到的就是共享不变量。**"只读"不靠约定，靠这个布尔与两次屏障。**

一个配套细节同样不是可省修饰：`os/libs/minix-platform/src/global.rs:struct PlatformContext` 的字段全是整型与枚举，**不含裸指针**（需要地址时在方法内部由整数现推）。含裸指针的类型不会被自动判定为可跨线程共享，那就又要回到"手写 `Sync`"。字段形状与并发保证之间是有因果的。

### 3.5 全部失败时怎么办，是产品决定

**C 不面对这个问题**：它的来源就是编译期那块板子的假设，"没找到"这个状态不存在（§2.1、§2.5）。重写要真跑在未知硬件上，就必须显式回答。

`os/libs/minix-platform/src/global.rs:fn qemu_fallback_or_panic` 的答案按构建档位分叉：开发构建下打一条警告、返回一份按假设构造的描述（`QemuVirtDesc::default()`）；发布构建下直接中止，中止消息直说"不是开发构建，没有这个兜底"。

两条各保护一类人。开发构建保护的是"我在一台没有正确表格的机器/模拟器上做开发，不该因为表格缺失就什么都做不了"；发布构建保护的是"真机上绝不能带着假设值跑"——带着假设值跑最坏的结果不是起不来，而是**读写到别的设备**，这类故障难查得多。

代价也要说清：同一份二进制在两种构建下行为不同，因此"能起来"不等于"描述是真的"。为此读接口上保留了 `source()`，任何一层都可以问一句"我现在拿到的是解析结果还是兜底"。

### 3.6 三架构对照

| 环节 | x86-64 | ARM64 | RISC-V |
|---|---|---|---|
| 来源 | 高级配置与电源接口表根指针 | 设备树优先、接口表回落 | 只有设备树 |
| 解析模块 | `os/libs/minix-platform/src/acpi.rs` | 两个都用（`acpi.rs`、`device_tree.rs`） | `os/libs/minix-platform/src/device_tree.rs` |
| 兜底 | 同一份按假设构造的描述 | 同 | 同 |
| 上游消费方式 | 调 `minix_platform::platform_desc()` 取只读引用 | 同 | 同 |

差异只出现在"从哪问、谁解析"，一旦形成描述，下游三行都一样。**这就是 3.1"统一的是读接口"的具体收益**：新增一种板型不改上层，新增一类来源只改 3.2 那三处。

---

## 4. 实现详解

### 4.1 冻结与读取（→3.4）

写的一侧在 `os/libs/minix-platform/src/global.rs`：`fn init_from_kinfo` 选出描述后调用 `fn init`，`init` 把值放进 `static PLATFORM` 并冻结；读的一侧是 `os/libs/minix-platform/src/global.rs:fn platform_desc`，它返回一个 `'static` 的只读引用，且在冻结之前被调用会直接中止——**"还没形成"不是返回空值，而是立刻暴露调用时机错了**。内核侧的用法就是调这个函数（`os/kernel/src/lib.rs` 里 `let topo = minix_platform::platform_desc().cpu_topology();` 这类）。

调用时机由内核主流程保证。`os/kernel/src/lib.rs` 里这一步在 Phase A.5：

```rust
        minix_platform::init_from_kinfo(kernel_info);
```

它必须早于唤醒第一个辅助处理器——多核启动在主流程更靠后。这条顺序写在 `global.rs` 的安全不变量说明里：冻结对所有读者都发生在前。它是代码顺序维持的，不是测试维持的（测试章末尾的"盲区"段把这条顺序列为无测试看守的弱点）。

### 4.2 一次解析：设备树（→3.1、3.3）

`os/libs/minix-platform/src/device_tree.rs` 里 `impl DeviceTreeDesc::from_bytes` 吃一段字节、返回 `Result`——失败是一种正常结果，所以 §3.3 那个循环才可能"继续试下一个来源"。

产出是查询能力而不是字段堆。举个例子：`impl DeviceTreeDesc::psci_conduit` 返回"如果这台机器给了电源状态协调接口调用通道，是哪一种"——一个可选值。消费方要发核间中断时问这一句就够，不需要知道它来自哪个节点、属性叫什么名字。这就是 §3.1"统一读接口"在实现里的样子：**结构体里不必有对应字段，接口上有一个能问的问题。**

### 4.3 一次解析：高级配置与电源接口表（→3.1）

`os/libs/minix-platform/src/acpi.rs:struct AcpiDesc` 与它的构造入口做的是 §2.2 那四个动词的同一件事，只是把结果结构化：表根确认与校验和验证在解析阶段完成，之后消费方拿不到"半张表"这种状态。对照 §2.4，C 里"挑第 idx 个某类条目"这套遍历是每次使用时现翻；重写把它归进构造期——**遍历一次、之后按值回答**。

顺带回答一个自然的问题：为什么 §2.3 那句 `panic`（读不了物理地址就中止）在重写里不是一行？因为重写读表的是引导垫片，它有恒等段可用（01 篇 §4.2），而内核侧只收到已解析的值。**"能不能读任意物理地址"这个时机约束，被边界划分消化掉了**，不再是内核代码里的一句断言。

### 4.4 兜底与真机差异（→3.5）

`fn qemu_fallback_or_panic` 的两条分支前面已经看过。实现上还有一点：兜底出来的描述与真解析出来的**同型**（同一套接口），所以任何一层忘记问 `source()`，编译不会提醒你。这条就是 3.5 那个代价的具体形状，也是为什么 §6 的诊断第一条就是"先问来源"。

---

## 5. 测试要点

数量由作者现场跑过（命令可复算）：

```bash
cd os && rg -c '#\[test\]' libs/minix-platform/src/*.rs
```

读数：`acpi.rs` 5、`device_tree.rs` 4、`kind.rs` 1、`global.rs` 1（合计 11）。

| 约束 | 守它的东西 | 强度 |
|---|---|---|
| 描述形成一次、之后只读（3.4） | 类型（`Frozen` 的写一次语义 + 字段无裸指针） | 编译期，最强 |
| 先成者胜、失败继续试（3.3） | 解析器各自返回 `Result`；分发处的单测 | 宿主可测 |
| 设备树字段读错要报错而不是给默认值（4.2） | 解析用例（含负例） | 宿主可测 |
| 兜底只在开发构建（3.5） | 编译期条件 | 不可由宿主测试同时验证两条分支 |
| 冻结早于辅助处理器唤醒（4.1） | 主流程代码顺序 | **只靠顺序**，见本章末的"盲区"段 |

只能真机判定的：这台机器的表根是否可解析、是否走了兜底分支（发布构建会中止，开发构建要靠 `source()` 去看）、多核拓扑数值与真实硬件是否一致。

**盲区**（如实列出）：兜底分支是编译期二选一，一次构建只跑一条；顺序契约（§4.1）没有任何断言或测试守着，重构 `kmain` 时把它挪到唤醒之后就没人报警——这是本篇披露的一处真实弱点。**另一处**：描述形成之后没有"再校验一次"的机会，所以任何解析期漏掉的字段，之后都只能靠上层自己检查。

---

## 6. 诊断

```bash
# 一：这台机器给了哪些来源、清单顺序是什么
rg -n "platform_sources|PlatformDescSource|DTB|RSDP" \
    os/libs/minix-boot/src/platform.rs os/boot-shim/src/*.rs

# 二：解析与兜底的分支在哪
rg -n "fn parse_by_kind|qemu_fallback_or_panic|debug_assertions|QemuVirtDesc" \
    os/libs/minix-platform/src/*.rs

# 三：冻结发生在哪、谁在读
rg -n "fn init_from_kinfo|fn init\(|PLATFORM|fn platform_desc" \
    os/libs/minix-platform/src/global.rs os/kernel/src/lib.rs
```

三类症状与读法：

**发布构建在启动早期就中止，消息里带着"没有兜底"字样。** 就是 §3.5 那条分支生效：所有来源都没解析成功。查命令一——清单是不是空的（引导侧没找到任何表），再查命令二的分发（表在但解析失败，多半是格式假设不成立）。

**开发构建能起来，之后某个驱动配不下去。** 先问来源：如果 `source()` 报兜底，那么控制器基址与串口地址都是假设值，症状与真机无关，与"你在用假数据"有关。这是这一类最容易被误当成 bug 的情况。

**某个 CPU 读到的描述与启动处理器不同。** 按 §4.1 查顺序：它一定意味着有人在被冻结之前读了（会中止），或者绕开 `Frozen` 自己复制了一份描述并各自更新。只读接口挡不住"我另存一份"，这类代码要单独审查。

顺带一条与 03 篇相连的判法：如果症状是"读某张固件表时中止"，那说明读表发生得太晚——虚拟内存服务已经接管地址空间（§2.3）。这条约束在重写里靠边界划分消化（§4.3），所以真出现时先查是不是有人在引导后期自己去读物理地址。

---

## 7. 参见

**C 真值源**

- `minix3/minix/kernel/arch/i386/acpi.c` —— 表根、校验和、取表、挑条目、关机信息（§2.2–§2.4）
- `minix3/minix/kernel/arch/i386/pre_init.c` —— 板级身份写死的那两行（§2.1）
- `minix3/minix/kernel/glo.h` —— `machine` 的声明与用途注释（§2.1）

**Rust 实现**

- `os/libs/minix-boot/src/platform.rs` —— `trait PlatformDesc` 与三个子接口；`struct PlatformDescKind`、`struct PlatformDescSource`、两个标签常量（§3.1、§3.2）
- `os/libs/minix-platform/src/global.rs` —— `fn init_from_kinfo`、`fn init`、`fn platform_desc`、`static PLATFORM`、`fn qemu_fallback_or_panic`、`struct PlatformContext`（§3.3–§3.5、§4.1）
- `os/libs/minix-platform/src/kind.rs` —— 按标签分发与"新增来源三步"（§3.2）
- `os/libs/minix-platform/src/acpi.rs`、`device_tree.rs`、`qemu_virt.rs`、`desc.rs` —— 三个实现与再导出层（§3.1、§4.2、§4.3）
- `os/libs/minix-types/src/types/cell.rs` —— `struct Frozen`（§3.4）
- `os/kernel/src/lib.rs` —— Phase A.5 的调用点与 `fn platform_desc` 的再导出（§4.1）

**相关主题篇**（尚未生成的只给主题名）

- 01 篇（控制权交接）：`reserved_regions` 与"只传值"规则、引导侧为何能读任意物理地址
- 03 篇（三张表）：§2.3 那个读表窗口为何在保护结构建立之后仍然存在
- 05 篇（时钟与中断初始化）：本篇产出的描述如何被消费（控制器基址、拓扑、定时器参数）
- 多核启动篇：唤醒辅助处理器的时机，以及本篇 §4.1 那条顺序契约的另一半
- 电源与关机篇：`acpi_init_poweroff` 与 `psci_conduit` 各自回答的问题

**外部规范来源**（引用处均已就地标明为规范层面事实）

- ACPI 规范的系统描述表结构与多源描述表条目类型（表头签名、校验和定义、条目类型编号）
- 设备树规范与电源状态协调接口固件调用规范（节点与属性命名、调用通道种类）
