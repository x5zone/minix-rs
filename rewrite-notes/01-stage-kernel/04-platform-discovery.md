# 04 · 内核怎么认识自己脚下的硬件：平台描述符抽象

> **C 真值源**: `minix3/minix/kernel/arch/i386/pre_init.c`（`get_board_name`/`BOARD_ID_INTEL`）、`minix3/minix/kernel/arch/earm`（板级 BSP 常量）
> **Rust 实现**: `os/libs/minix-boot/src/platform.rs:trait PlatformDesc`、`os/libs/minix-platform/src/{kind.rs:parse_by_kind,device_tree.rs:DeviceTreeDesc,acpi.rs:AcpiDesc,qemu_virt.rs:QemuVirtDesc,global.rs}`

## 第 1 章　概念：内核不能"知道自己跑在哪块板子上"靠写死

### 1.1 问题：硬编码地址撑不起多平台

中断控制器的基址、有几个核、时钟频率、串口在哪个地址——内核要干活就必须知道这些。Minix3 的做法朴素：每个板级支持包（BSP）把这些写成 `#define` 常量，x86 一套、每块 ARM 板各一套。换一块板、进一个新的 QEMU 机型，就得改这些常量、重新编译。这在"只跑固定几块板"的年代够用，但和 minix-rs 的目标（同一份内核在 x86-64 / aarch64 / riscv64、不同固件下都能起）直接冲突。本篇讲的抽象，就是把"硬件长什么样"从**编译期写死**变成**运行期从固件描述里读出来**。

### 1.2 换个视角：这是 CPU 三问里"我站在哪"的延伸

01-boot-shim-bootstrap.md说 boot-shim 要回答"我站在哪里、哪里可用"。那是**内存**层面。本篇是**外设与拓扑**层面：有几个 CPU/hart？中断控制器是 LAPIC+IOAPIC、GICv3 还是 PLIC？它们的状态寄存器在哪？时钟多快？——这些不该编进内核逻辑，而该在启动时"发现"。

### 1.3 三个架构，硬件描述的来源不同

| | x86-64 | aarch64 | riscv64 |
|---|---|---|---|
| 描述载体 | ACPI（RSDP → XSDT → MADT） | DTB（扁平设备树）或 ACPI 共存 | DTB |
| 谁给内核 | 固件表留在物理内存某地址 | 固件把 DTB 地址传给内核 | OpenSBI 传 DTB 指针 |

> **先释义几个名词（后文不再解释）**：**DTB**＝Device Tree Blob，一棵描述外设节点的树，开一个内存地址放它；**ACPI** ＝一整套带签名的小表，入口指针叫 **RSDP**，层层指下去到 **MADT**（中断拓扑表）；**OpenSBI** ＝ riscv 上跑在 M-mode 的固件，负责把内核提到 S-mode 并把 DTB 地址递过来。三者的共性是：都只在物理内存某处留一段"硬件说明书"，内核自己去找、去读。
>
> 三行“谁给内核”的实质区别：x86 是内核**主动扫**内存找 RSDP；ARM/RISC-V 是固件**把地址直接递**到内核手里（一个寄存器/参数）。方向不同，但都回到同一个动作：“拿着这个入口去读描述”。

01-boot-shim-bootstrap.md 里 `KernelInfo.platform_sources` 装的就是这些**物理地址 + 不透明分类标签**（DTB=1、RSDP=2）。本篇接手：内核拿这些句柄，把硬件描述**解析**成一份统一的结构。注意分工——**01 只管"句柄怎么跨镜像安全传进来"，本篇管"句柄背后的表怎么被读成 OS 语义"**，这是"发现"，不是"编程"（把寄存器写进驱动是后续各篇的事）。

```
发现(discovery) 与 编程(programming) 的边界：
  发现: 读 DTB/ACPI → 得到 "有 N 个核、GIC 基址 = X、时钟 = Y"   ← 本篇
  编程: 拿这些值去初始化 GICC、写 timer 比较寄存器              ← 05/中断与驱动各篇
本篇的产物是一份只读的 PlatformDesc；它是上面那条边界里"发现"的输出。
```

---

## 第 2 章　C 源码：Minix3 怎么把板子写死

### 2.1 x86：靠一个 board id

`pre_init` 阶段，C 版给内核贴一个"板子身份"字符串，全凭一个编译期常量 `BOARD_ID_INTEL`（`minix3/minix/kernel/arch/i386/pre_init.c:get_parameters`，节选）：

```c
/* let higher levels know what we are booting on */
mb_set_param(cbi->param_buf, ARCHVARNAME,
             (char *)get_board_arch_name(BOARD_ID_INTEL), cbi);
mb_set_param(cbi->param_buf, BOARDVARNAME,
             (char *)get_board_name(BOARD_ID_INTEL), cbi);
```

`BOARD_ID_INTEL` 是写死的：x86 假设自己就在"intel 板"上。ARM 侧（`arch/earm`）更直接——每个 BSP（如 omap）在头文件里把串口、中断控制器、时钟基址全列成 `#define`。

> **局限（这才是抽象存在的理由）**：内核逻辑里一旦散布"串口在 0x…、GIC 在 0x…"这类字面量，换一块板就得改代码、重编译，而且这些常量会被无数子系统各自抄一份。C 版能忍，是因为它本来就一次只针对有限的板。minix-rs 要"一处描述、多处只读"，就必须把这些**从代码里挪进数据**。

---

## 第 3 章　Rust 设计：把"硬件长什么样"抽成一个 trait

### 3.1 为什么是 trait 不是 struct

不同固件给的表结构天差地别（ACPI 是一堆签名表、DTB 是一棵树、QEMU virt 是硬编码兜底），但它们**对内核暴露的语义相同**：给我中断控制器描述、给我定时器频率、给我 CPU 拓扑。语义相同、来源各异，正是 trait 的用武之地：

```rust
pub trait PlatformDesc: Send + Sync + fmt::Debug {
    fn interrupt_controller(&self) -> &dyn InterruptControllerDesc;
    fn timer(&self) -> &dyn TimerDesc;
    fn cpu_topology(&self) -> CpuTopology;
    fn source(&self) -> PlatformSource;   // 从哪来（DTB / ACPI / QemuVirt），供诊断
    // …early_console / arch_misc / sswi_setip_base / psci_conduit…
}
```

（`os/libs/minix-boot/src/platform.rs:trait PlatformDesc`）三个实现：`AcpiDesc`（`os/libs/minix-platform/src/acpi.rs:struct AcpiDesc`）、`DeviceTreeDesc`（`os/libs/minix-platform/src/device_tree.rs:struct DeviceTreeDesc`）、`QemuVirtDesc`（硬编码兜底，`qemu_virt.rs:QemuVirtDesc`）。OS 层只拿 `&dyn PlatformDesc`，看不见背后是 ACPI 还是 DTB——与 01/02/03 一贯的"抽象语义、把形状关进实现"同源。

### 3.2 子描述符：trait + `Any` 向下转型

中断控制器既有共性语义（`nr_irqs()`），又有架构特有字段（x86 的 lapic_base/ioapic_base、ARM 的 gicd_base/gicr_base、RISC-V 的 plic context）。全塞进 trait 会让接口被某一家污染；全砍掉又让 arch 层拿不到具体基址。折中：通用方法走 trait，架构特有字段用 `Any` 向下转型在 arch 层取。

具体到代码，"同一个语义、不同的形状"是这样分家的（字段名取自 `os/libs/minix-boot/src/platform.rs` 与各 arch 实现）：

```rust
// OS 层只看得见的共性接口：
pub trait InterruptControllerDesc: Any + Send + Sync { fn nr_irqs(&self) -> u32; /* + as_any */ }

// 三家各自的“形状”——同名方法、字段完全不同：
//   x86 : ApicDesc  { nr, lapic_base, ioapic_base }
//   arm : Gicv3Desc { nr, gicd_base,  gicr_base  }
//   rv  : PlicDesc  { nr, plic_base,  context    }

// arch 层要拿“自己家特有”的基址时才 downcast；OS 层从不 match 品牌：
let ic = desc.interrupt_controller();                 // &dyn InterruptControllerDesc
let n  = ic.nr_irqs();                                // ← 三架构都走这一句，语义相同
let g  = ic.as_any().downcast_ref::<Gicv3Desc>();     // 只有 aarch64 驱动会转成功
if let Some(g) = g { let base = g.gicr_base; /* 形状只在 arch 内部流动 */ }
```

这样 OS 层代码里没有一处 `match 品牌`："语义"是 `nr_irqs()`，"形状"是 `gicr_base`，各归各位——downcast 在错的架构上返回 `None`，不会把别家的字段泄进通用路径。

### 3.3 分发：opaque tag 查表，不跨边界传函数指针

01-boot-shim-bootstrap.md §3.4 立的规矩在此落地：内核不 import 各解析器再 if-else 判类型，而是拿 `PlatformDescSource` 里的 `kind`（u32）走一张分发表在**同一 crate 内**解析（`os/libs/minix-platform/src/kind.rs:fn parse_by_kind`）。

> **为什么非得这样（就地讲清，不外推）：** 三条路里，①把解析器做成跨镜像传进来的函数指针——boot-shim 内存一回收就悬垂（正是 01 §3.4 那个事故）；②内核直接 `if kind == DTB { 调 DTB 解析器 }`——就得 import 每一家的具体类型，把"世上有几种平台描述"焊死进内核代码，加一种就改内核；③只传不透明 `u32` tag + 在 minix-platform 内部一张表分发——内核只认数字，加一家描述只改那张表、不碰内核码。**注意 `parse_by_kind` 是 `unsafe fn`**：它要按物理地址去读固件留下的表，安全前提（地址有效、identity 映射仍在）由 boot 期调用方保证。

### 3.4 全局存储：为什么是 `Frozen` 不是 `Mutex`/`static mut`

解析出的描述符是启动期一次算好、之后**只读**的。它存在内核 BSS 的一个 `static`：

```rust
static PLATFORM: Frozen<PlatformContext> = Frozen::new();
```

（`os/libs/minix-platform/src/global.rs`）取舍：

| 方案 | 是否选用 | 为什么 |
|---|---|---|
| `Frozen`（boot 期单线程写一次、之后只读、手动 impl `Sync`） | ✅ | 读侧无锁、无需原子；写冻结发生在 BKL 持有、单线程的启动里程碑 |
| `Mutex`/`OnceLock` | ❌ | 为一个"写完不再改"的数据背运行时同步开销，语义也不符 |
| `static mut` | ❌ | 每次访问都要 `unsafe`，并不比 `Frozen` 更安全 |

这条正好呼应 KERNEL 面约束（SMP+BKL：共享数据要么 `Arc`+锁/原子、要么有明确的初始化后只读理由）——`PlatformDesc` 属于后者，`Frozen` 把"为什么可以只读"写进了类型与注释。

---

## 第 4 章　实现 + 实地导览：T2.5 这一步在整条启动链的哪里

前几章是设计判断；这一章给一条**打开当前代码顺着读**的路径。平台发现发生在启动里程碑 **T2.5**（`kmain` 里、进程表初始化前后），链路如下（锚点均 `os/kernel/src/lib.rs` 与 `os/libs/minix-platform/src/`）：

```
kmain (Phase)                         ← 03 篇的 kmain 编排骨架
  └─ 取 kernel_info.platform_sources  （01-boot-shim-bootstrap.md §3.4 交进来的 [PlatformDescSource]）
       └─ parse_by_kind(source)        (kind.rs，unsafe：按 phys_addr 读固件表)
            ├─ DTB  ─► DeviceTreeDesc   (device_tree.rs)
            ├─ RSDP ─► AcpiDesc         (acpi.rs)
            └─ 解析全失败 / 无句柄 ─► QemuVirtDesc 兜底 (qemu_virt.rs)
       └─ freeze(PlatformContext{ desc })   →  写入 global.rs 的 `static PLATFORM: Frozen<…>`
            ↓ 之后所有子系统的只读入口
  PlatformContext::current()  ──► 时钟/中断/SMP 初始化各篇拿 nr_cpus()、gicc 基址、frequency() 等
```

读代码时就按这条序点下去。两个最关键的真实代码行（非流程图）：分发入口 `os/libs/minix-platform/src/kind.rs:fn parse_by_kind` 按 `kind` match 出三种描述符之一；冻结落盘在 `os/libs/minix-platform/src/global.rs` 里那一行——

```rust
// global.rs: 把解析好的描述符写进只读全局（BKL 持有、单线程的启动里程碑）
PLATFORM.freeze(PlatformContext { desc });
```

`parse_by_kind` 是**按 kind 分发**的那一个 match；`QemuVirtDesc` 是"什么描述都没给"时的**兜底分支**（dev 构造，release 下无句柄才 panic）；`PLATFORM` 被上面那行 `freeze` 之后，`current()` 在 SMP 下被多 CPU 只读——所以冻结点必须在 BKL 持有时、在任何次级核起来之前。**"发现是只读的、编程在别处"这条 §1.3 的边界，在这条链上就是 `freeze` 与 `current()` 两个点**：把这条链（从 `parse_by_kind` 到上面那行 `freeze`）在仓库里点一遍，“为什么需要 `Frozen`”就不再是结论而是你在代码里看到的位置。

---

## 第 5 章　测试要点

- 三种 `PlatformDesc` 实现各可单测：喂一段最小 DTB 字节/ACPI 表构造 `DeviceTreeDesc`/`AcpiDesc`，断 `nr_irqs()`/`frequency()`/`cpu_topology().nr_cpus`；`QemuVirtDesc` 断兜底值（如预置核数）。
- `parse_by_kind` 对未知 kind 返回 `UnknownKind`（错误码语义，不能 panic 于畸形表）。
- 端到端：三架构引导内核起来后 `PlatformContext::current()` 有合法描述（`os/qemu-tests/.../test-proc-init` 一类会用到拓扑）。**测试数以现算为准**（`rg -c '#\[test\]' os/libs/minix-platform/src/`），不抄表。

> 认知减负：`global.rs` 里 `Frozen`/`AssumeSync` 的内存模型细节（为什么手动 `Sync` 在只读前提下 sound）属 unsafe-abstraction 专题，本篇用到结论即可，深究留给 `27-kernel-utility.md`。

---

## 第 6 章　诊断：硬件"没认出来"会怎样

最典型的故障形态是：**DTB/ACPI 给了但解析失败**，或**干脆没句柄**。

- 没句柄 → 走 `QemuVirtDesc` 兜底（dev 构造下静默兜底、release 下 panic）。若你在真 ARM 板上却看到 QemuVirt 的值，说明 `platform_sources` 是空的——回到 01-boot-shim-bootstrap.md §3.4 查 boot-shim 是否把 DTB/RSDP 句柄塞进了 `KernelInfo`。
- 有句柄但解析 panic → 大概率 `parse_by_kind` 按 `phys_addr` 读表时越界或表畸形；它是 `unsafe fn`，安全前提（identity 映射覆盖该地址、boot 期）没满足就会炸。

可复制的定位命令：

```bash
# 看 kind 分发都支持哪些平台描述符 + 兜底路径
rg -n "match|DTB|RSDP|QemuVirt|UnknownKind|DtbParse|AcpiParse" os/libs/minix-platform/src/kind.rs

# 确认 PLATFORM 冻结点与读入口（发现=只读，编程在别处）
rg -n "freeze|current\(\)|static PLATFORM|Frozen" os/libs/minix-platform/src/global.rs
```

预期：正常启动日志里应看到选中的 `PlatformSource`（DeviceTree/Acpi/QemuVirt）；若打了 QemuVirt 而你在真实 DTB 板上，就是句柄没传到——沿 §4 那条链倒回 `KernelInfo.platform_sources`。
