# 04-protection: 保护模式基础设施

> **分类**: Kernel 保护与中断
> **源码**: `minix3/minix/kernel/arch/i386/protect.c`(456行)
> **说明**: GDT/IDT/TSS 初始化、段选择符、boot_proc 二进制加载——用户态进程不能碰硬件的根基

---

## 1. 概述

### 1.1 概念定义与作用

x86 保护模式通过**段描述符**和**特权级**两层机制隔离内核与用户态。Minix3 内核在自举早期调用 `prot_init()` 构建三张核心表：

| 表 | 全称 | 作用 | 位置 |
|---|------|------|------|
| **GDT** | Global Descriptor Table | 存放内核/用户代码段、数据段、TSS、LDT 描述符 | `gdt[GDT_SIZE]`（protect.c:25） |
| **IDT** | Interrupt Descriptor Table | 存放异常/硬件中断/系统调用门描述符 | `idt[IDT_SIZE]`（protect.c:26） |
| **TSS** | Task State Segment | 保存内核栈指针，用于 Ring3→Ring0 切换时自动换栈 | `tss[CONFIG_MAX_CPUS]`（protect.c:27） |

**为什么需要保护模式？** 在 01-multiboot-bootstrap 阶段开启分页后，内核已拥有虚拟内存。但此时没有中断机制，没有特权隔离——任何代码都能执行 `cli`/`outb` 等特权指令。保护模式通过硬件强制实现：

1. **段级隔离**：用户态进程使用 CPL=3 的段选择符，无法访问 CPL=0 的内核段
2. **中断门控**：所有中断/异常通过 IDT 门描述符进入内核，门描述符的 DPL 决定是否允许用户态触发
3. **栈自动切换**：TSS 中记录的 `ss0:sp0` 保证用户态→内核态切换时硬件自动换到内核栈

### 1.2 与 Minix3 启动流程的对应关系

`protect.c` 中的函数在内核启动的三个阶段被调用：

| 阶段 | 调用点 | 函数 | 作用 |
|------|--------|------|------|
| **cstart()** | main.c:411 | `prot_init()` | 构建 GDT/IDT/TSS，加载段选择符，重建页表 |
| **main() 循环** | main.c:257 | `arch_boot_proc()` | 为每个 boot_image 进程（特别是 VM）加载 ELF 二进制 |
| **main() 末尾** | main.c:283 | `arch_post_init()` | 设置 VM 为当前页表进程，获取 VM 的 cr3 信息 |

**关键时序**：`prot_init()` 在 `cstart()` 中被调用（main.c:411），此时分页已开启但使用 01 阶段的恒等映射页表。`prot_init()` 内部会重建页表（`pg_clear`→`pg_identity`→`pg_mapkernel`→`pg_load`），使内核不再依赖自举阶段的预重定位数据。

### 1.3 关键状态与机制说明

#### 1.3.1 GDT 布局

Minix3 的 GDT 采用固定索引布局（archconst.h:14-21）：

| 索引 | 选择符 | 用途 | DPL |
|------|--------|------|-----|
| 0 | 0x00 | 空描述符（硬件要求） | — |
| 1 | 0x08 (`KERN_CS_SELECTOR`) | 内核代码段 | 0 |
| 2 | 0x10 (`KERN_DS_SELECTOR`) | 内核数据段 | 0 |
| 3 | 0x1B (`USER_CS_SELECTOR`) | 用户代码段 | 3 |
| 4 | 0x23 (`USER_DS_SELECTOR`) | 用户数据段 | 3 |
| 5 | 0x28 (`LDT_SELECTOR`) | 不可用的 LDT | 0 |
| 6+ | `TSS_SELECTOR(cpu)` | 每 CPU 一个 TSS | 0 |

**SYSENTER/SYSEXIT 约束**：Intel SYSENTER 指令要求 `CS` 选择符紧接 `SS` 选择符（偏移 +8），因此内核代码段（索引1）和数据段（索引2）必须相邻，用户代码段（索引3）和数据段（索引4）也必须相邻。`GDT_SIZE` = `TSS_INDEX(CONFIG_MAX_CPUS)` = 6 + CONFIG_MAX_CPUS。

#### 1.3.2 特权级

Minix3 仅使用两级特权（archconst.h:33-34）：

| 级别 | 常量 | 值 | 使用者 |
|------|------|----|--------|
| Ring 0 | `INTR_PRIVILEGE` | 0 | 内核 + 中断处理程序 |
| Ring 3 | `USER_PRIVILEGE` | 3 | 服务进程 + 用户进程 |

**注意**：Minix3 不使用 Ring 1/2。虽然 `INIT_TASK_PSW` 设置 IOPL=1（archconst.h:130），但这仅允许内核任务执行 I/O 指令，不代表它们运行在 Ring 1。

#### 1.3.3 系统调用入口机制

Minix3 支持三种用户态→内核态的系统调用入口，通过 CPU 特性检测选择（protect.c:323-324）：

| 机制 | CPU 特性标志 | IDT 向量 | 入口函数 |
|------|-------------|---------|---------|
| `int` 指令（原始） | 始终可用 | `KERN_CALL_VECTOR_ORIG`(32) | `kernel_call_entry_orig` |
| `int` 指令（user-mapped） | 始终可用 | `KERN_CALL_VECTOR_UM`(34) | `kernel_call_entry_um` |
| SYSENTER（Intel） | `MKF_I386_INTEL_SYSENTER` | MSR | `ipc_entry_sysenter` |
| SYSCALL（AMD） | `MKF_I386_AMD_SYSCALL` | MSR | `ipc_entry_syscall_cpuN` |

IPC 入口同理，使用向量 33（原始）和 35（user-mapped）。

#### 1.3.4 VM 二进制加载的特殊性

`arch_boot_proc()` 中，VM 进程（`VM_PROC_NR == 8`）的加载路径与其他 boot 进程完全不同：

- **VM**：在自举页表中通过 `libexec_load_elf()` 解析 ELF 并映射到自举页表，使用 `libexec_pg_alloc()` 分配页面
- **其他进程**：`arch_boot_proc()` 对 `p_nr < 0` 的内核任务直接返回；对 `p_nr >= 0` 的非 VM 进程，仅设置 `RTS_VMINHIBIT | RTS_BOOTINHIBIT` 等待 VM 为其创建页表

**原因**：VM 是第一个运行的用户态进程，它负责为其他所有进程管理内存。在 VM 运行之前，没有服务进程能处理内存分配请求，因此内核必须在自举页表中亲手加载 VM。

### 1.4 行为规则

1. **GDT/IDT 初始化顺序**：`prot_init()` 先清零 GDT/IDT → 设置描述符指针 → 初始化 TSS → 填充 GDT 条目 → 加载选择符 → 重建页表
2. **段描述符基地址/界限**：内核和用户段的基地址均为 0，界限均为 4GB（平坦内存模型），通过 `sdesc()` 统一设置
3. **IDT 门描述符特权**：硬件中断和 CPU 异常使用 `INTR_PRIVILEGE`(0)，仅 `breakpoint`(向量3)、`overflow`(向量4)、IPC/系统调用向量使用 `USER_PRIVILEGE`(3)
4. **TSS 初始化**：每 CPU 一个 TSS，`ss0:sp0` 指向该 CPU 的内核栈顶（预留 2 个 `reg_t` 用于存储进程指针和 CPU ID）
5. **`prot_init_done` 标志**：`prot_init()` 完成后置 1（protect.c:30），供其他代码判断保护模式是否已初始化

---

## 2. C 源码分析

### 2.1 相关定义

#### 2.1.1 GDT/IDT/TSS 尺寸与索引常量

定义于 `archconst.h`：

| 常量 | 值 | 含义 |
|------|----|------|
| `IDT_SIZE` | 256 | IDT 最大条目数（archconst.h:11） |
| `KERN_CS_INDEX` | 1 | 内核代码段在 GDT 中的索引 |
| `KERN_DS_INDEX` | 2 | 内核数据段在 GDT 中的索引 |
| `USER_CS_INDEX` | 3 | 用户代码段在 GDT 中的索引 |
| `USER_DS_INDEX` | 4 | 用户数据段在 GDT 中的索引 |
| `LDT_INDEX` | 5 | LDT 在 GDT 中的索引 |
| `TSS_INDEX_FIRST` | 6 | 第一个 TSS 在 GDT 中的索引 |
| `TSS_INDEX(cpu)` | `6 + cpu` | 每 CPU 的 TSS 索引 |
| `GDT_SIZE` | `6 + CONFIG_MAX_CPUS` | GDT 总条目数 |
| `DESC_SIZE` | 8 | 每个描述符占 8 字节 |
| `SEG_SELECTOR(i)` | `i * 8` | 将 GDT 索引转为段选择符 |

#### 2.1.2 段选择符

定义于 `archconst.h:24-29`：

| 选择符 | 值 | 构成 |
|--------|----|------|
| `KERN_CS_SELECTOR` | 0x08 | 索引1, TI=0, RPL=0 |
| `KERN_DS_SELECTOR` | 0x10 | 索引2, TI=0, RPL=0 |
| `USER_CS_SELECTOR` | 0x1B | 索引3, TI=0, RPL=3 |
| `USER_DS_SELECTOR` | 0x23 | 索引4, TI=0, RPL=3 |
| `LDT_SELECTOR` | 0x28 | 索引5, TI=0, RPL=0 |
| `TSS_SELECTOR(cpu)` | `(6+cpu)*8` | 索引6+cpu, TI=0, RPL=0 |

**注意**：用户态选择符的 RPL=3（`| USER_PRIVILEGE`），内核态选择符 RPL=0。

#### 2.1.3 描述符访问权限位

定义于 `archconst.h:55-80`：

| 位/常量 | 值 | 含义 |
|---------|----|------|
| `PRESENT` | 0x80 | 描述符有效 |
| `DPL_SHIFT` | 5 | DPL 在 access 字节中的位移 |
| `SEGMENT` | 0x10 | 段类型描述符（非门描述符） |
| `EXECUTABLE` | 0x08 | 可执行段 |
| `READABLE` | 0x02 | 可读（代码段） |
| `WRITEABLE` | 0x02 | 可写（数据段） |
| `ACCESSED` | 0x01 | 已访问（硬件自动设置） |
| `LDT` | 2 | LDT 描述符类型 |
| `INT_286_GATE` | 6 | 286 中断门类型 |
| `DESC_386_BIT` | 0x08 | 386 扩展位 |
| `INT_GATE_TYPE` | `6 \| 8` = 0x0E | 386 中断门类型 |
| `TSS_TYPE` | `1 \| 8` = 0x09 | 386 可用 TSS 类型 |

#### 2.1.4 粒度与界限常量

定义于 `archconst.h:97-112`：

| 常量 | 值 | 含义 |
|------|----|------|
| `BASE_MIDDLE_SHIFT` | 16 | base→base_middle 位移 |
| `BASE_HIGH_SHIFT` | 24 | base→base_high 位移 |
| `BYTE_GRAN_MAX` | 0xFFFFF | 字节粒度最大界限（1MB） |
| `GRANULARITY_SHIFT` | 16 | limit→granularity 位移 |
| `PAGE_GRAN_SHIFT` | 12 | 页粒度额外位移 |
| `OFFSET_HIGH_SHIFT` | 16 | 门偏移→offset_high 位移 |
| `GRANULAR` | 0x80 | 4K 页粒度标志 |
| `DEFAULT` | 0x40 | 32 位默认操作数/堆栈大小 |
| `BIG` | 0x40 | expand-down 段的 BIG 标志 |

#### 2.1.5 中断向量号

定义于 `interrupt.h` 和 `archconst.h`：

**CPU 异常向量**（interrupt.h:21-25, archconst.h:38-49, 82-86）：

| 向量号 | 常量 | 含义 | 门特权 |
|--------|------|------|--------|
| 0 | `DIVIDE_VECTOR` | 除法错误 | INTR |
| 1 | `DEBUG_VECTOR` | 单步/调试 | INTR |
| 2 | `NMI_VECTOR` | 不可屏蔽中断 | INTR |
| 3 | `BREAKPOINT_VECTOR` | 软件断点 | **USER** |
| 4 | `OVERFLOW_VECTOR` | INTO 溢出 | **USER** |
| 5 | `BOUNDS_VECTOR` | 越界检查 | INTR |
| 6 | `INVAL_OP_VECTOR` | 无效操作码 | INTR |
| 7 | `COPROC_NOT_VECTOR` | 协处理器不可用 | INTR |
| 8 | `DOUBLE_FAULT_VECTOR` | 双重故障 | INTR |
| 9 | `COPROC_SEG_VECTOR` | 协处理器段越界 | INTR |
| 10 | `INVAL_TSS_VECTOR` | 无效 TSS | INTR |
| 11 | `SEG_NOT_VECTOR` | 段不存在 | INTR |
| 12 | `STACK_FAULT_VECTOR` | 栈异常 | INTR |
| 13 | `PROTECTION_VECTOR` | 一般保护错误 | INTR |
| 14 | `PAGE_FAULT_VECTOR` | 页错误 | INTR |
| 16 | `COPROC_ERR_VECTOR` | 协处理器错误 | INTR |
| 17 | `ALIGNMENT_CHECK_VECTOR` | 对齐检查 | INTR |
| 18 | `MACHINE_CHECK_VECTOR` | 机器检查 | INTR |
| 19 | `SIMD_EXCEPTION_VECTOR` | SIMD 浮点异常 | INTR |

**系统调用/IPC 向量**（interrupt.h:28-31）：

| 向量号 | 常量 | 含义 | 门特权 |
|--------|------|------|--------|
| 32 | `KERN_CALL_VECTOR_ORIG` | 内核调用（原始 int 路径） | **USER** |
| 33 | `IPC_VECTOR_ORIG` | IPC 入口（原始 int 路径） | **USER** |
| 34 | `KERN_CALL_VECTOR_UM` | 内核调用（user-mapped 路径） | **USER** |
| 35 | `IPC_VECTOR_UM` | IPC 入口（user-mapped 路径） | **USER** |

**硬件中断向量**（interrupt.h:54）：

```c
#define VECTOR(irq)  (((irq) < 8 ? IRQ0_VECTOR : IRQ8_VECTOR) + ((irq) & 0x07))
```

IRQ0-7 映射到向量 0x50-0x57，IRQ8-15 映射到向量 0x70-0x77。

#### 2.1.6 CPU 特性标志

| 常量 | 值 | 含义 | 来源 |
|------|----|------|------|
| `_CPUF_I386_SYSENTER` | 16 | CPU 支持 SYSENTER | cpufeature.h:25 |
| `_CPUF_I386_SYSCALL` | 17 | CPU 支持 SYSCALL | cpufeature.h:26 |
| `MKF_I386_INTEL_SYSENTER` | `1 << 0` | 运行时特性标志 | const.h:167 |
| `MKF_I386_AMD_SYSCALL` | `1 << 1` | 运行时特性标志 | const.h:168 |

#### 2.1.7 MSR 地址

| 常量 | 值 | 用途 |
|------|----|------|
| `INTEL_MSR_SYSENTER_CS` | 0x174 | SYSENTER CS 段选择符 |
| `INTEL_MSR_SYSENTER_ESP` | 0x175 | SYSENTER 内核栈指针 |
| `INTEL_MSR_SYSENTER_EIP` | 0x176 | SYSENTER 入口点 |
| `AMD_MSR_EFER` | 0xC0000080 | 扩展特性使能寄存器 |
| `AMD_MSR_STAR` | 0xC0000081 | SYSCALL CS/SS 选择符 |
| `AMD_EFER_SCE` | `1 << 0` | EFER 中 SYSCALL 使能位 |

### 2.2 核心数据结构

#### 2.2.1 segdesc_s — 段描述符

定义于 `archtypes.h:10-18`，8 字节 packed 结构：

```c
struct segdesc_s {
  u16_t limit_low;      // 界限低 16 位
  u16_t base_low;       // 基地址低 16 位
  u8_t  base_middle;    // 基地址中 8 位
  u8_t  access;         // 访问权限字节 |P|DPL|1|X|E|R|A|
  u8_t  granularity;    // 粒度字节 |G|X|0|A|LIMIT_HIGH|
  u8_t  base_high;      // 基地址高 8 位
} __attribute__((packed));
```

**字段说明**：

| 字段 | 位宽 | 作用 |
|------|------|------|
| `limit_low` | 16 位 | 段界限低 16 位 |
| `base_low` | 16 位 | 基地址低 16 位 |
| `base_middle` | 8 位 | 基地址 16-23 位 |
| `access` | 8 位 | P(1)+DPL(2)+S(1)+Type(3)+A(1) |
| `granularity` | 8 位 | G(1)+D/B(1)+0+AVL(1)+Limit高4位 |
| `base_high` | 8 位 | 基地址 24-31 位 |

基地址共 32 位 = `base_low` + `base_middle << 16` + `base_high << 24`。界限共 20 位 = `limit_low` + `(granularity & 0x0F) << 16`，若 G=1 则实际界限 = 界限 << 12。

#### 2.2.2 gatedesc_s — 门描述符

定义于 `archtypes.h:19-26`，8 字节 packed 结构：

```c
struct gatedesc_s {
  u16_t offset_low;    // 处理程序偏移低 16 位
  u16_t selector;      // 目标代码段选择符
  u8_t  pad;           // 填充 |000|XXXXX|（中断/陷阱门）或 |XXXXXXXX|（任务门）
  u8_t  p_dpl_type;    // |P|DPL|0|TYPE|
  u16_t offset_high;   // 处理程序偏移高 16 位
} __attribute__((packed));
```

**字段说明**：

| 字段 | 作用 |
|------|------|
| `offset_low` + `offset_high` | 处理程序入口地址（32 位） |
| `selector` | 目标代码段选择符（始终为 `KERN_CS_SELECTOR`） |
| `p_dpl_type` | P(1)+DPL(2)+0+TYPE(5)，TYPE = `INT_GATE_TYPE`(0x0E) |

#### 2.2.3 desctableptr_s — 描述符表指针

定义于 `archtypes.h:27-30`，6 字节 packed 结构：

```c
struct desctableptr_s {
  u16_t limit;    // 表大小 - 1
  u32_t base;     // 表的线性地址
} __attribute__((packed));
```

用于 `lgdt`/`lidt` 指令加载 GDT/IDT 的基地址和界限。`init_segdesc()` 函数（protect.c:219-224）巧妙地将 GDT 条目重新解释为 `desctableptr_s` 来设置描述符指针。

#### 2.2.4 tss_s — 任务状态段

定义于 `arch_proto.h:167-203`，packed 结构：

```c
struct tss_s {
  reg_t backlink;    // 任务门返回链接
  reg_t sp0;         // Ring 0 栈指针（中断时自动加载）
  reg_t ss0;         // Ring 0 栈段选择符
  reg_t sp1;         // Ring 1 栈指针（未使用）
  reg_t ss1;         // Ring 1 栈段选择符（未使用）
  reg_t sp2;         // Ring 2 栈指针（未使用）
  reg_t ss2;         // Ring 2 栈段选择符（未使用）
  reg_t cr3;         // 页目录基址（任务切换时加载，Minix3 不使用硬件任务切换）
  reg_t ip;          // EIP（任务切换恢复点，Minix3 不使用）
  reg_t flags;       // EFLAGS
  reg_t ax, cx, dx, bx, sp, bp, si, di;  // 通用寄存器
  reg_t es, cs, ss, ds, fs, gs;           // 段寄存器
  reg_t ldt;         // LDT 选择符
  u16_t trap;        // 调试陷阱标志
  u16_t iobase;      // I/O 权限位图偏移
} __attribute__((packed));
```

**Minix3 实际使用的字段**：

| 字段 | 用途 |
|------|------|
| `sp0` | Ring 0 栈指针，中断/异常时硬件自动切换到此处 |
| `ss0` | Ring 0 栈段选择符，始终为 `KERN_DS_SELECTOR` |
| `iobase` | I/O 权限位图偏移，设为 `sizeof(tss_s)` 表示空位图（禁止所有 I/O） |

**Minix3 不使用的字段**：`backlink`、`sp1/ss1`、`sp2/ss2`、`cr3`、`ip`、通用寄存器、段寄存器、`ldt`、`trap`——这些仅在硬件任务切换时使用，Minix3 用软件切换代替。

#### 2.2.5 gate_table_s — 中断向量表条目

定义于 `arch_proto.h:217-220`：

```c
struct gate_table_s {
  void (*gate)(void);      // 处理程序入口
  unsigned char vec_nr;    // 中断向量号
  unsigned char privilege; // DPL
};
```

用于静态定义中断处理程序到 IDT 向量的映射。`gate_table_pic[]`（protect.c:107-124）映射 16 个 PIC 硬件中断，`gate_table_exceptions[]`（protect.c:127-153）映射 CPU 异常和系统调用入口。数组以 `{ NULL, 0, 0 }` 结尾作为哨兵。

#### 2.2.6 exec_info — ELF 加载上下文

定义于 `libexec.h:23-57`，`arch_boot_proc()` 使用此结构调用 `libexec_load_elf()` 加载 VM 的 ELF 二进制：

| 字段 | 用途 |
|------|------|
| `proc_e` | 目标进程 endpoint |
| `hdr` | ELF 头/完整映像的物理地址 |
| `hdr_len` / `filesize` | 文件大小 |
| `stack_high` / `stack_size` | 栈顶地址 / 栈大小（64KB） |
| `progname` | 进程名 |
| `copymem` | 复制回调 → `libexec_copy_memcpy` |
| `clearmem` | 清零回调 → `libexec_clear_memset` |
| `allocmem_*` | 分配回调 → `libexec_pg_alloc`（自举页表分配） |
| `pc` | ELF 入口点（由 libexec 填充） |

#### 2.2.7 boot_image — 启动映像条目

定义于 `type.h:148-156`：

```c
struct boot_image {
  int proc_nr;                    // 进程号
  char proc_name[PROC_NAME_LEN]; // 进程名
  endpoint_t endpoint;            // endpoint
  phys_bytes start_addr;          // 内存起始地址
  phys_bytes len;                 // 映像长度
};
```

内核启动时，`image[NR_BOOT_PROCS]` 全局数组（glo.h:79）存储所有 boot 进程的信息。`bootmod()` 函数（protect.c:274-289）通过 `proc_nr` 在 `image[]` 中查找对应的 multiboot 模块。

### 2.3 关键函数分析

#### 2.3.1 prot_init() — 保护模式初始化主函数

**位置**：protect.c:321-368

**调用点**：`cstart()` → `prot_init()`（main.c:411）

**执行流程**：

1. **CPU 特性检测**（L323-324）：通过 `_cpufeature()` 检测 SYSENTER/SYSCALL 支持，设置 `minix_feature_flags`
2. **清零 GDT/IDT**（L327-328）：`memset(gdt, 0, sizeof(gdt))` / `memset(idt, 0, sizeof(idt))`
3. **设置描述符表指针**（L330-333）：`gdt_desc.base/limit` 和 `idt_desc.base/limit`
4. **初始化 TSS**（L334）：`tss_init(0, &k_boot_stktop)` — BSP 使用 boot 栈顶
5. **填充 GDT 条目**（L336-340）：
   - LDT（索引5）：不可用的 LDT，类型 = `PRESENT | LDT`
   - 内核代码段（索引1）：DPL=0，可执行+可读
   - 内核数据段（索引2）：DPL=0，可写+已访问
   - 用户代码段（索引3）：DPL=3，可执行+可读
   - 用户数据段（索引4）：DPL=3，可写+已访问
6. **加载选择符**（L353）：`prot_load_selectors()` — 加载 GDTR/IDTR/LDTR/TR 和所有段寄存器
7. **重建页表**（L356-358）：`pg_clear()` → `pg_identity()` → `pg_mapkernel()` → `pg_load()` — 替换自举页表
8. **设置完成标志**（L370）：`prot_init_done = 1`

**关键细节**：所有段描述符的基地址为 0、界限为 4GB（平坦模型），通过 `sdesc()` 统一设置。`sdesc()` 将 size-1 转换为 limit，当 size > `BYTE_GRAN_MAX`(1MB) 时自动切换到页粒度。

#### 2.3.2 prot_load_selectors() — 加载段选择符

**位置**：protect.c:298-316

**调用点**：`prot_init()` 和 AP 启动代码（`mpx.S`）

**执行流程**：

1. `x86_lgdt(&gdt_desc)` — 加载 GDTR
2. `idt_init()` — 填充 IDT（PIC 中断 + CPU 异常）
3. `idt_reload()` — `x86_lidt(&idt_desc)` 加载 IDTR
4. `x86_lldt(LDT_SELECTOR)` — 加载 LDTR（不可用的 LDT）
5. `x86_ltr(TSS_SELECTOR(booting_cpu))` — 加载 TR（per-CPU TSS）
6. 加载所有段寄存器：CS=`KERN_CS_SELECTOR`，DS/ES/FS/GS/SS=`KERN_DS_SELECTOR`

**SMP 注意**：`booting_cpu` 全局变量（protect.c:296）标识当前正在启动的 CPU。BSP 启动时 `booting_cpu=0`，AP 启动时由 `mpx.S` 设置为对应 CPU 编号。

#### 2.3.3 tss_init() — TSS 初始化

**位置**：protect.c:154-198

**参数**：`cpu` — CPU 编号，`kernel_stack` — 内核栈顶地址

**返回值**：TSS 段选择符 `SEG_SELECTOR(TSS_INDEX(cpu))`

**执行流程**：

1. 在 GDT 中创建 TSS 描述符（L160-163）：基地址 = TSS 结构体物理地址，界限 = `sizeof(tss_s)`，类型 = `TSS_TYPE`(0x09)
2. 清零 TSS 结构体（L166）
3. 设置段寄存器（L167）：`ds=es=fs=gs=ss0=KERN_DS_SELECTOR`，`cs=KERN_CS_SELECTOR`
4. 设置 I/O 权限位图偏移（L170）：`iobase = sizeof(tss_s)` — 空位图，禁止所有 I/O 端口访问
5. 设置内核栈指针（L175）：`sp0 = kernel_stack - X86_STACK_TOP_RESERVED`，预留 2 个 `reg_t` 空间
6. 在栈顶存储 CPU ID（L179）：`*((reg_t *)(t->sp0 + 1 * sizeof(reg_t))) = cpu`
7. **SYSENTER 设置**（L182-186）：若 CPU 支持，写 MSR `SYSENTER_CS=KERN_CS_SELECTOR`、`SYSENTER_ESP=sp0`、`SYSENTER_EIP=ipc_entry_sysenter`
8. **SYSCALL 设置**（L189-205）：若 CPU 支持，使能 EFER.SCE，设置 STAR 寄存器（CS 选择符 + 入口函数地址），每个 CPU 有独立的入口函数

**X86_STACK_TOP_RESERVED**（archconst.h:157）：`2 * sizeof(reg_t)` = 8 字节，用于在内核栈顶存储当前进程指针和 CPU ID。

#### 2.3.4 sdesc() — 填充段描述符的基地址和界限

**位置**：protect.c:58-75

**参数**：`segdp` — 段描述符指针，`base` — 基地址，`size` — 段大小

**算法**：

1. 设置 `base_low`、`base_middle`（base >> 16）、`base_high`（base >> 24）
2. 将 size-1 转换为 limit（0 size 表示 4GB）
3. 若 limit > `BYTE_GRAN_MAX`(0xFFFFF)：
   - `limit_low = size >> PAGE_GRAN_SHIFT`(12)
   - `granularity = GRANULAR | (size >> (PAGE_GRAN_SHIFT + GRANULARITY_SHIFT))`
4. 否则：
   - `limit_low = size`
   - `granularity = size >> GRANULARITY_SHIFT`(16)
5. `granularity |= DEFAULT` — 设置 32 位默认操作数

**关键**：`sdesc()` 不设置 `access` 字节，由调用者根据段类型设置。

#### 2.3.5 init_codeseg() / init_param_dataseg() / init_dataseg()

**位置**：protect.c:78-104

| 函数 | 参数 | access 字节 |
|------|------|------------|
| `init_codeseg(index, privilege)` | GDT 索引, DPL | `PRESENT \| SEGMENT \| EXECUTABLE \| READABLE \| (DPL << 5)` |
| `init_param_dataseg(segdp, base, size, privilege)` | 描述符指针, 基地址, 大小, DPL | `PRESENT \| SEGMENT \| WRITEABLE \| ACCESSED \| (DPL << 5)` |
| `init_dataseg(index, privilege)` | GDT 索引, DPL | 同上，base=0, size=0xFFFFFFFF |

**注意**：数据段描述符包含 `ACCESSED` 位（0x01），代码段不包含。这是 x86 硬件惯例——数据段的 A 位由硬件在首次访问时自动设置，初始时置 1 可避免不必要的硬件写入。

#### 2.3.6 int_gate() / int_gate_idt() / idt_copy_vectors()

**位置**：protect.c:227-252

- **`int_gate(tab, vec_nr, offset, dpl_type)`**：在门描述符表 `tab` 的 `vec_nr` 位置构建中断门描述符。`selector = KERN_CS_SELECTOR`，`p_dpl_type = dpl_type`
- **`int_gate_idt(vec_nr, offset, dpl_type)`**：直接操作全局 `idt[]`
- **`idt_copy_vectors(first)`**：遍历 `gate_table_s[]` 数组，逐条调用 `int_gate()` 填充 IDT，遇到 `{ NULL, 0, 0 }` 哨兵停止

#### 2.3.7 idt_init() / idt_reload()

**位置**：protect.c:254-270

- **`idt_init()`**：先调用 `idt_copy_vectors_pic()`（16 个 PIC 中断），再调用 `idt_copy_vectors(gate_table_exceptions)`（CPU 异常 + 系统调用入口）
- **`idt_reload()`**：执行 `x86_lidt(&idt_desc)` 重新加载 IDTR

#### 2.3.8 arch_boot_proc() — 启动进程 ELF 加载

**位置**：protect.c:388-456

**调用点**：`main()` 中遍历 `image[]` 时对每个进程调用（main.c:257）

**执行流程**：

1. **内核任务跳过**（L393）：`rp->p_nr < 0` 直接返回
2. **查找 multiboot 模块**（L395）：`bootmod(rp->p_nr)` 在 `image[]` 中查找
3. **VM 特殊路径**（L398-453）：仅当 `rp->p_nr == VM_PROC_NR`(8) 时执行
   - 填充 `exec_info` 结构体（L400-424）：栈顶 = `kinfo.user_sp`，栈大小 = 64KB，ELF 头 = multiboot 模块的物理地址
   - 设置回调：`copymem = libexec_copy_memcpy`，`clearmem = libexec_clear_memset`，`allocmem_* = libexec_pg_alloc`
   - 调用 `libexec_load_elf(&execi)` 解析 ELF 并映射到自举页表（L427）
   - 在栈上构建 `ps_strings` 结构（L430-445）：设置 argv/envp 指针，预留 argc/argv/envp 三个字
   - 调用 `arch_proc_init()` 设置进程的入口点和栈指针（L447-449）
   - 释放 VM 的 multiboot 模块占用的内存映射（L452）：`add_memmap()` 标记该物理内存区域为可用
   - 记录分配给 VM 的字节数（L455）：`kinfo.vm_allocated_bytes = alloc_for_vm`

**非 VM 进程**：`arch_boot_proc()` 对非 VM 进程不做 ELF 加载。`main()` 中设置 `RTS_VMINHIBIT | RTS_BOOTINHIBIT`（main.c:260-261），等待 VM 为其创建页表。

#### 2.3.9 arch_post_init() — 架构后初始化

**位置**：protect.c:370-376

**调用点**：`main()` 末尾（main.c:283）

**执行流程**：

1. 获取 VM 进程的 `proc` 结构：`proc_addr(VM_PROC_NR)`
2. 设置 per-CPU 的 `ptproc` 指向 VM：`get_cpulocal_var(ptproc) = vm`
3. 获取 VM 的页目录物理地址和虚拟地址：`pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v)`

**作用**：将 VM 设为"当前页表进程"，后续地址空间切换以 VM 的页目录为基准。

#### 2.3.10 vir2phys() — 虚拟地址转物理地址

**位置**：protect.c:35-40

**算法**：利用链接脚本导出的 `_kern_vir_base` 和 `_kern_phys_base` 符号计算偏移量：

```c
offset = &_kern_vir_base - &_kern_phys_base;
phys = (phys_bytes)vir - offset;
```

**适用场景**：仅在 1:1 映射仍然存在时有效（protect.c:22 注释）。

#### 2.3.11 enable_iop() — 允许进程使用 I/O 指令

**位置**：protect.c:44-53

**算法**：设置 PSW 的 IOPL 位为 3（`pp->p_reg.psw |= 0x3000`），使 CPL=3 的用户进程也能执行 I/O 指令。

**使用场景**：极少数需要直接硬件访问的驱动程序。

#### 2.3.12 bootmod() — 查找 boot 模块

**位置**：protect.c:274-289

**算法**：在 `image[]` 数组中从 `NR_TASKS`(5) 开始搜索 `proc_nr == pnr` 的条目，返回对应的 `kinfo.module_list[p]`。`NR_TASKS` 之前的条目是内核任务，没有对应的 multiboot 模块。

#### 2.3.13 libexec_pg_alloc() — 自举页表内存分配

**位置**：protect.c:379-385

**算法**：调用 `pg_map(PG_ALLOCATEME, vaddr, vaddr+len, &kinfo)` 在自举页表中分配并映射页面，然后清零。累计分配量记录在 `alloc_for_vm` 中。

**PG_ALLOCATEME**（archconst.h:160）：`((phys_bytes)-1)`，是 `pg_map()` 的特殊参数，表示"自动分配物理页面"。

### 2.4 调用关系分析

#### 2.4.1 启动阶段调用链

```
cstart()                              [main.c:411]
  └─ prot_init()                      [protect.c:321]
       ├─ _cpufeature()               [cpufeature.S]
       ├─ tss_init(0, &k_boot_stktop) [protect.c:155]
       │    ├─ init_param_dataseg()   [protect.c:80]
       │    ├─ ia32_msr_write()       [klib.S]  (SYSENTER)
       │    └─ ia32_msr_read/write()  [klib.S]  (SYSCALL)
       ├─ init_param_dataseg()        [LDT]
       ├─ init_codeseg()              [KERN_CS]
       ├─ init_dataseg()              [KERN_DS, USER_DS]
       │    └─ init_param_dataseg()
       ├─ prot_load_selectors()       [protect.c:298]
       │    ├─ x86_lgdt()             [klib.S]
       │    ├─ idt_init()             [protect.c:254]
       │    │    ├─ idt_copy_vectors_pic()
       │    │    │    └─ idt_copy_vectors(gate_table_pic)
       │    │    │         └─ int_gate()
       │    │    └─ idt_copy_vectors(gate_table_exceptions)
       │    │         └─ int_gate()
       │    ├─ idt_reload() → x86_lidt()
       │    ├─ x86_lldt()
       │    ├─ x86_ltr()
       │    └─ x86_load_*()           [klib.S] (CS/DS/ES/FS/GS/SS)
       ├─ pg_clear()                  [pg_utils.c]
       ├─ pg_identity(&kinfo)         [pg_utils.c]
       ├─ pg_mapkernel()              [pg_utils.c]
       └─ pg_load()                   [pg_utils.c]

main()                                [main.c:257]
  └─ arch_boot_proc(ip, rp)           [protect.c:388]
       ├─ bootmod(rp->p_nr)           [protect.c:274]
       │    └─ 搜索 image[] 数组
       ├─ libexec_load_elf(&execi)    [exec_elf.c]  (仅 VM)
       │    ├─ execi.copymem → libexec_copy_memcpy()
       │    ├─ execi.clearmem → libexec_clear_memset()
       │    └─ execi.allocmem_* → libexec_pg_alloc()
       │         └─ pg_map(PG_ALLOCATEME, ...)
       ├─ arch_proc_init()            [memory.c:722]  (仅 VM)
       │    └─ arch_proc_reset()      [arch_system.c:146]
       └─ add_memmap()                [仅 VM，释放模块内存]

main()                                [main.c:283]
  └─ arch_post_init()                 [protect.c:370]
       └─ pg_info()                   [pg_utils.c]
```

#### 2.4.2 函数调用点汇总

| 函数 | 定义位置 | 调用者 |
|------|---------|--------|
| `prot_init()` | protect.c:321 | `cstart()`（main.c:411） |
| `prot_load_selectors()` | protect.c:298 | `prot_init()`、AP 启动代码（mpx.S） |
| `tss_init()` | protect.c:154 | `prot_init()`、AP 启动代码（arch_smp.c） |
| `sdesc()` | protect.c:58 | `init_param_dataseg()`、`init_codeseg()` |
| `init_param_dataseg()` | protect.c:80 | `tss_init()`、`prot_init()`（LDT） |
| `init_dataseg()` | protect.c:92 | `prot_init()`（KERN_DS、USER_DS） |
| `init_codeseg()` | protect.c:100 | `prot_init()`（KERN_CS、USER_CS） |
| `int_gate()` | protect.c:227 | `idt_copy_vectors()` |
| `int_gate_idt()` | protect.c:237 | 外部调用（APIC 初始化等） |
| `idt_copy_vectors()` | protect.c:245 | `idt_init()`、APIC 初始化 |
| `idt_copy_vectors_pic()` | protect.c:258 | `idt_init()` |
| `idt_init()` | protect.c:261 | `prot_load_selectors()` |
| `idt_reload()` | protect.c:266 | `prot_load_selectors()`、APIC 初始化 |
| `arch_boot_proc()` | protect.c:388 | `main()`（main.c:257） |
| `arch_post_init()` | protect.c:370 | `main()`（main.c:283） |
| `bootmod()` | protect.c:274 | `arch_boot_proc()` |
| `libexec_pg_alloc()` | protect.c:379 | `libexec_load_elf()` 通过回调调用 |
| `enable_iop()` | protect.c:44 | 外部调用（驱动程序） |
| `vir2phys()` | protect.c:35 | 外部调用（1:1 映射期间） |
| `init_segdesc()` | protect.c:219 | 外部调用（设置描述符指针） |

### 2.5 设计要点与特殊处理

#### 2.5.1 平坦内存模型

Minix3 使用平坦内存模型：所有段的基地址为 0，界限为 4GB。这意味着段选择符实际上不提供地址隔离——保护完全依赖分页。段描述符的主要作用是：

- 设置 DPL（决定哪些特权级可以访问）
- 区分代码段（可执行）和数据段（可写）
- 满足 x86 硬件对 CS/DS/SS 必须指向有效段描述符的要求

#### 2.5.2 GDT 条目复用为描述符指针

`init_segdesc()`（protect.c:219-224）将 GDT 条目重新解释为 `desctableptr_s`：

```c
struct desctableptr_s *dtp = (struct desctableptr_s *) &gdt[gdt_index];
dtp->limit = size - 1;
dtp->base = (phys_bytes) base;
```

这是可行的因为 `desctableptr_s`（6 字节：limit + base）恰好覆盖 `segdesc_s`（8 字节）的前 6 字节，而 GDT 条目 0 是空描述符（不被使用）。

#### 2.5.3 SYSCALL 的 per-CPU 入口函数

AMD SYSCALL 机制通过 STAR MSR 指定入口函数地址。由于每个 CPU 有独立的内核栈，入口函数必须知道当前 CPU 编号才能找到正确的栈。Minix3 的解决方案是为每个 CPU 编写独立的入口函数（`ipc_entry_syscall_cpu0` ~ `ipc_entry_syscall_cpu7`），通过 `set_star_cpu()` 宏（protect.c:197-205）在 `tss_init()` 中设置。`CONFIG_MAX_CPUS` 限制为 8（protect.c:206 assert）。

#### 2.5.4 VM 栈上的 ps_strings 布局

`arch_boot_proc()` 在 VM 的用户栈上构建 `ps_strings` 结构（protect.c:430-445），为 VM 的 C 运行时启动代码提供 argv/envp 信息：

```
高地址
  ┌──────────────────┐ ← stack_high
  │  ps_strings      │
  │  ps_argvstr ─────┼──┐
  │  ps_nargvstr = 0 │  │
  │  ps_envstr ──────┼──┼──┐
  │  ps_nenvstr = 0  │  │  │
  ├──────────────────┤  │  │
  │  envp (NULL)     │←─┼──┘
  │  argv (NULL)     │←─┘
  │  argc = 0        │
  └──────────────────┘ ← sp（实际栈指针）
低地址
```

VM 启动时没有命令行参数，因此 argc=0、argv=NULL、envp=NULL。

#### 2.5.5 prot_init_done 标志

`prot_init_done`（protect.c:30）在 `prot_init()` 末尾置 1。其他代码可检查此标志判断保护模式是否已初始化。例如，在 `prot_init()` 完成前不能依赖 GDT/IDT 中的描述符。

#### 2.5.6 video_mem 全局指针

`video_mem`（protect.c:23）初始化为 `MULTIBOOT_VIDEO_BUFFER`(0xB8000)，指向 VGA 文本缓冲区。注释说明这仅在 1:1 映射存在时有效（protect.c:22）。分页重建后，此指针通过 `arch_phys_map()` 重新映射到正确的虚拟地址。

#### 2.5.7 内核任务不加载 ELF

`arch_boot_proc()` 对 `p_nr < 0` 的内核任务直接返回（protect.c:393）。内核任务（如 CLOCK、SYSTEM）的代码已包含在内核映像中，不需要从 multiboot 模块加载。它们的入口点在 `image[]` 数组中通过 `start_addr` 指定。

---

## 3. Rust 设计决策

> 本章解释从 Ch1&2 的 C 源码到 Rust 设计的每一个关键选择。每个决策给出：为什么选这条路径、替代方案有哪些、为什么否决替代方案。

### 3.1 两 trait 拆分：ProtectionArch + TrapEntryArch

**C 源码依据**：§1.3 分析了保护模式的三层机制——段级隔离（GDT）、中断门控（IDT）、栈自动切换（TSS）。§2.3.1-2.3.3 分析了 `prot_init()`、`prot_load_selectors()`、`tss_init()` 的职责划分。

**决策**：将保护模式相关硬件操作拆分为两个 trait：

| trait | 职责 | x86-64 实现 | ARM64 实现 | RISC-V 实现 |
|-------|------|------------|-----------|-------------|
| `ProtectionArch` | 特权级 + 栈切换 + 描述符表 | GDT/TSS/段选择符 | SP_EL0/SP_EL1 | sscratch/sstatus |
| `TrapEntryArch` | 中断/异常/系统调用入口配置 | IDT + SYSCALL MSR | VBAR_EL1 异常向量 | stvec trap 向量 |

**为什么是两个而非三个**：

考虑过三 trait 拆分（`ProtectionArch` + `InterruptTableArch` + `SystemCallArch`），但否决了。原因：ARM64 和 RISC-V 的中断、异常、系统调用共用同一入口机制——ARM64 的 SVC 指令和 IRQ 中断都通过 VBAR_EL1 异常向量表进入，RISC-V 的 ecall 和中断都通过 stvec 进入。如果拆出独立的 `SystemCallArch`，ARM64/RISC-V 的实现将是空操作或冗余的重复注册，违反"trait 每个方法在不同架构上实现真的不同"的判定标准（review-code-skill §2.5）。

x86-64 的 SYSCALL 确实独立于 IDT（通过 MSR 配置），但这是 x86 的特殊情况。将 SYSCALL 配置归入 `TrapEntryArch`，让 ARM64/RISC-V 的 `configure_syscall()` 实现为空操作，比拆出第三个 trait 更合理——因为 SYSCALL 的语义是"配置系统调用入口"，属于"入口配置"范畴。

**为什么不是一个**：

`ProtectionArch` 和 `TrapEntryArch` 的职责不同：前者是"谁能访问什么"（访问控制），后者是"如何进入内核"（入口机制）。它们有不同的初始化时机——保护结构（GDT/TSS）必须在陷阱入口（IDT）之前加载，因为 IDT 门描述符引用的段选择符必须在 GDT 中有效。合并为一个 trait 会模糊这个时序依赖。

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| 三 trait（Protection + InterruptTable + SystemCall） | ARM64/RISC-V 的 SystemCall 实现为空操作，违反"方法在不同架构上实现真的不同"标准 |
| 单 trait（ProtectionArch 覆盖全部） | 模糊访问控制与入口机制的职责边界，且初始化时序依赖不清晰 |
| 不拆分，用 `#[cfg(target_arch)]` 条件编译 | 违反"硬件必须抽象为 trait"原则，OS 代码不应出现架构条件编译 |

### 3.2 PrivilegeLevel 作为 trait 关联类型

**C 源码依据**：§1.3.2 分析了 Minix3 的两级特权——`INTR_PRIVILEGE`(0) 和 `USER_PRIVILEGE`(3)。§2.1.3 分析了 DPL 在描述符访问权限字节中的编码。

**决策**：`ProtectionArch` trait 使用关联类型 `PrivilegeLevel`，同时提供公共枚举 `Privilege` 供 OS 代码使用。每个架构定义自己的 `PrivilegeLevel` 类型，并通过 trait 方法与 `Privilege` 枚举互转。

**推理过程**：

三种架构的特权级语义相同（都是两级：内核/用户），但表示方式不同：

| 架构 | 内核级 | 用户级 | 硬件编码 |
|------|--------|--------|---------|
| x86-64 | Ring 0 | Ring 3 | 段选择符 RPL 字段（2 位） |
| ARM64 | EL1 | EL0 | SPSR_EL1.M 字段（4 位） |
| RISC-V | S-mode | U-mode | sstatus.SPP 位（1 位） |

如果直接用公共枚举 `Privilege { Kernel, User }`，OS 代码无法获取架构特定的硬件编码值（如 x86 的 Ring 3 = 0x3）。如果用关联类型但不提供公共枚举，OS 代码无法在架构无关的上下文中比较特权级。

因此，采用"关联类型 + 公共枚举 + 转换方法"的三层设计：

```
OS 代码 ←→ Privilege 枚举 ←→ ProtectionArch::PrivilegeLevel ←→ 硬件编码
```

**为什么不用 bitflags**：特权级是互斥的（一个时刻只有一个级别），不是可组合的标志位。bitflags 的 `|` 操作对特权级无意义。

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| 公共枚举 `Privilege` 直接作为 trait 关联类型 | x86-64 实现需要 Ring 0/3 的数值编码（用于段选择符 RPL），纯枚举无法携带此信息 |
| bitflags `PrivilegeFlags` | 特权级互斥，不是可组合标志 |
| 硬编码 `u8` | 裸整数表达语义，Translate 味道（review-patterns-skill 模式15） |

### 3.3 OS 语义方法名，隐藏 x86 的 GDT/TSS

**C 源码依据**：§1.3.1 分析了 GDT 布局，§2.3.3 分析了 `tss_init()` 的实现细节。

**决策**：trait 方法使用 OS 语义命名，不暴露架构特定的硬件概念。x86-64 的 GDT/TSS 细节完全封装在实现内部。

**方法名映射**：

| OS 语义方法 | 语义 | x86-64 实现 | ARM64 实现 | RISC-V 实现 |
|------------|------|------------|-----------|-------------|
| `init()` | 初始化保护结构 | 清零 GDT，填充段描述符，创建 TSS | 配置 SP_EL0/SP_EL1 | 配置 sscratch |
| `set_kernel_stack()` | 设置特权级切换时的内核栈 | 更新 TSS.sp0 | 更新 SP_EL1 | 更新 sscratch |
| `load()` | 将保护结构加载到硬件 | lgdt, lldt, ltr, 重载段寄存器 | msr SP_EL1 | csrw sscratch |
| `init_ap()` | 初始化 AP 的保护结构 | 创建 per-CPU TSS/GDT 条目 | 配置 per-CPU SP_EL1 | 配置 per-CPU sscratch |

**为什么不用 x86 语义方法名**：如果 trait 方法叫 `setup_tss()`，ARM64/RISC-V 的实现者需要理解"TSS 是什么"才能决定如何实现——这是 x86 特有的概念，与 ARM64/RISC-V 的栈切换机制无关。OS 语义方法名 `set_kernel_stack()` 描述的是"设置内核栈指针"这个 OS 需求，各架构用自己的机制实现。

**GDT 段描述符去哪了**：x86-64 长模式下，代码段和数据段描述符退化为平坦模型（基地址 0，界限全地址空间），仅用于加载 CS/DS/SS/ES 等段寄存器。这些描述符的填充是 `init()` 实现的内部细节，不需要暴露为 trait 方法。TSS 描述符同理——它是 GDT 中的一个条目，由 `init()` 和 `set_kernel_stack()` 内部管理。

### 3.4 64 位架构演进

**C 源码依据**：§2.2.1-2.2.4 分析的 `segdesc_s`、`gatedesc_s`、`tss_s` 都是 32 位结构。§2.1.6-2.1.7 分析的 SYSENTER/SYSCALL MSR 也是 32 位视角。

**决策**：minix-rs 仅考虑 64 位现代硬件，32 位遗留机制全部删除。

**32 位 → 64 位变化清单**：

| 方面 | Minix3 (32 位) | minix-rs (64 位) | 影响 |
|------|---------------|-----------------|------|
| 段描述符 | 8 字节 `segdesc_s` | 64 位长模式下代码/数据段描述符仍 8 字节，但基地址/界限无意义 | GDT 简化，仅 TSS 描述符有实际作用 |
| 门描述符 | 8 字节 `gatedesc_s` | 16 字节（64 位中断门/陷阱门） | IDT 结构完全重写 |
| TSS | 104 字节 `tss_s` | 64 位 TSS（含 IST1-7） | TSS 结构重写，新增 IST 支持 |
| SYSENTER | 支持（MSR 0x174-0x176） | **删除**——64 位使用 SYSCALL/SYSRET | `tss_init()` 中的 SYSENTER MSR 写入全部删除 |
| 描述符表指针 | 6 字节 `desctableptr_s` | 10 字节（64 位 LGDT/LIDT 操作数） | `init_segdesc()` 技巧不再适用 |
| 段选择符 | 16 位，RPL=0/3 | 不变 | 选择符值不变（0x08/0x10/0x1B/0x23） |
| LDT | 不可用的 LDT 描述符 | **删除**——64 位不使用 LDT | GDT 中 LDT_INDEX 条目删除 |
| `prot_init_done` | 全局标志 | **删除**——Rust 用类型状态表达初始化完成 | 见 §3.5 |

**SYSENTER 删除的理由**：Intel 文档明确说明 SYSENTER/SYSEXIT 不支持 64 位模式（应使用 SYSCALL/SYSRET）。AMD 处理器在 64 位模式下 SYSENTER 行为未定义。Minix3 的 `_cpufeature()` 检测和 `minix_feature_flags` 条件分支在 64 位下可以简化为仅支持 SYSCALL。

**LDT 删除的理由**：Minix3 在 GDT 索引 5 放置一个"不可用的 LDT 描述符"（`PRESENT | LDT`，protect.c:336），仅用于加载 LDTR。64 位长模式下 LDT 无任何用途，删除后 GDT 布局更简洁。

### 3.5 类型状态替代 prot_init_done 全局标志

**C 源码依据**：§2.5.5 分析了 `prot_init_done` 标志——其他代码检查此标志判断保护模式是否已初始化。

**决策**：用 Rust 类型系统表达"保护模式是否已初始化"，删除 `prot_init_done` 全局标志。

**推理过程**：

Minix3 的 `prot_init_done` 是一个运行时检查——如果代码在 `prot_init()` 之前调用依赖保护模式的函数，行为未定义。Rust 可以在编译时阻止这种错误：

```rust
// 类型状态模式：初始化前后的类型不同
struct BeforeInit;
struct AfterInit<P: ProtectionArch> {
    protection: P,
    trap_entry: T,
}

// 编译时保证：只有 AfterInit 状态才能调用 load()、set_kernel_stack() 等
```

**为什么不用 `Option<ProtectionArch>`**：`Option` 允许运行时检查 `is_some()`，但无法阻止在 `None` 状态下调用方法。类型状态在编译时阻止非法调用，更符合"非法状态不可表达"原则。

**否决的替代方案**：

| 方案 | 否决原因 |
|------|---------|
| 保留 `prot_init_done: AtomicBool` | 运行时检查，编译器无法阻止未初始化时调用 |
| `Option<ProtectionArch>` | 运行时 unwrap，不如类型状态安全 |
| `OnceCell<ProtectionArch>` | 适合全局单例，但当前设计是 boot 流程传递所有权，不需要全局访问 |

### 3.6 boot_load_vm 为自由函数

**C 源码依据**：§2.3.6 分析了 `arch_boot_proc()`——为 VM 进程加载 ELF 二进制。§1.3.4 分析了 VM 加载的特殊性——在自举页表中亲手加载。

**决策**：`boot_load_vm()` 是 kernel crate 中的自由函数，通过泛型约束 `P: Paging` 调用页表操作。不纳入 `ProtectionArch` 或 `TrapEntryArch` trait。

**推理过程**：

`arch_boot_proc()` 的核心逻辑是 ELF 解析和页表映射——这些是架构无关的操作：

1. 从 boot module 读取 ELF 头
2. 调用 `libexec_load_elf()` 解析 ELF 段
3. 通过 `libexec_pg_alloc()` 在自举页表中分配和映射页面
4. 设置进程的入口点和栈

其中只有第 3 步涉及硬件——通过 `Paging::map()` 映射页面。ELF 解析、栈布局、`ps_strings` 设置都是纯软件逻辑，与架构无关。

将 `boot_load_vm` 纳入 `ProtectionArch` trait 会导致：
- ARM64/RISC-V 的实现与 x86-64 完全相同（都是 ELF 解析 + Paging::map），违反"方法在不同架构上实现真的不同"标准
- trait 承担了不属于"保护模式"的职责（ELF 加载是启动流程，不是硬件保护机制）

作为自由函数，`boot_load_vm()` 通过 `P: Paging` 泛型约束访问页表操作，天然架构无关：

```rust
fn boot_load_vm<P: Paging>(
    kernel_info: &KernelInfo,
    paging: &mut P,
    proc: &mut KProcess,
    boot_module: &BootModule,
) -> Result<VirBytes, BootError>
```

**与 Minix3 的对应**：

| Minix3 函数 | minix-rs 函数 | 变化 |
|------------|--------------|------|
| `arch_boot_proc()` | `boot_load_vm()` | 仅处理 VM，其他进程由 VM 管理 |
| `libexec_load_elf()` | ELF 解析逻辑内联 | 无需独立 libexec 模块 |
| `libexec_pg_alloc()` | `Paging::map()` | 通过 trait 调用，架构无关 |
| `arch_proc_init()` | 设置进程入口点/栈 | 进程结构初始化，非硬件操作 |

### 3.7 arch_post_init 消除

**C 源码依据**：§2.3.7 分析了 `arch_post_init()`——设置 `ptproc = vm` 并获取 VM 的 CR3 信息。

**决策**：`arch_post_init()` 在 64 位设计中不再需要，其功能由其他机制替代。

**推理过程**：

`arch_post_init()` 做两件事：

1. **`get_cpulocal_var(ptproc) = vm`**：设置"当前页目录所属进程"为 VM。这是 32 位临时 PDE 映射机制的核心——`createpde()` 操作 `ptproc` 的页目录。64 位 Direct Map 消除了临时 PDE 映射（02-page-table-kernel.md §3.1），因此 `ptproc` 不再存在。

2. **`pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v)`**：获取 VM 页目录的物理地址和虚拟地址。在 64 位设计中，VM 的 CR3 存储在进程的 `PageTableRef::cr3` 字段中，虚拟地址通过 `DirectMapArch::kernel_phys_to_virt()` 按需计算，无需在启动时特殊获取。

### 3.8 InterruptVector 公共类型

**C 源码依据**：§2.1.5 分析了中断向量号的定义——CPU 异常（0-19）、系统调用（32-35）、硬件中断（0x50-0x77）。

**决策**：在 arch crate 中定义 `InterruptVector` newtype，封装向量号。定义 CPU 异常向量为常量，而非枚举——因为向量号范围不连续，枚举会产生大量空变体。

**为什么是 newtype 而非裸 u8**：向量号有语义——它标识中断来源。裸 `u8` 无法区分"向量 14 表示页错误"和"向量 14 是某个硬件中断"。newtype 提供类型安全，同时保持与硬件编码的兼容性。

**为什么不用枚举**：中断向量号 0-255 中，只有约 30 个有定义含义，其余由硬件中断使用。枚举需要覆盖所有 256 个值，大部分是无意义的变体。常量 + newtype 更实用：

```rust
pub struct InterruptVector(pub u8);

// CPU 异常常量
pub const DIVIDE_ERROR: InterruptVector = InterruptVector(0);
pub const PAGE_FAULT: InterruptVector = InterruptVector(14);
pub const BREAKPOINT: InterruptVector = InterruptVector(3);
// ...
```

### 3.9 错误码对齐

**C 源码依据**：§2.3.6 分析了 `arch_boot_proc()` 中的 `panic("VM loading failed")`——VM 加载失败直接 panic，不返回错误码。

**决策**：保护模式初始化失败（GDT/IDT/TSS 设置错误）属于不可恢复的内核错误，直接 panic。`boot_load_vm()` 失败返回 `BootError` 枚举，错误码与 Minix3 的 errno 对齐。

**错误码映射**：

| 场景 | Minix3 行为 | Rust 行为 |
|------|-----------|----------|
| `prot_init()` 失败 | 不会失败（静态数组，不可能分配失败） | 同——GDT/IDT 是静态/栈分配，不会失败 |
| VM 加载失败 | `panic("VM loading failed")` | `Err(BootError::VmLoadFailed)` — 使用 `ENOEXEC`(8) |
| ELF 解析失败 | `libexec_load_elf()` 返回非 OK | `Err(BootError::ElfParse(ENOEXEC))` |
| 页面分配失败 | `libexec_pg_alloc()` 调 `pg_map()` | `Err(BootError::PageAlloc(ENOMEM))` |

### 3.10 BKL 保护下的单线程假设

**C 源码依据**：`prot_init()` 在 `cstart()` 中被调用，此时仅 BSP 运行，无并发问题。`arch_boot_proc()` 在 `main()` 循环中被调用，此时 AP 尚未启动。

**决策**：保护模式初始化代码假设单线程执行，不引入同步机制。`ProtectionArch` 和 `TrapEntryArch` 的实例不是 `Send`/`Sync`——它们只在 boot 阶段由 BSP 使用，之后通过 `init_ap()` 为每个 AP 独立创建。

**推理过程**：保护模式初始化发生在内核启动的最早阶段——`cstart()` 调用 `prot_init()`，此时 AP 尚未启动。AP 启动后，每个 AP 通过 `init_ap()` 独立初始化自己的保护结构（per-CPU TSS/GDT），不共享状态。因此不需要锁或原子操作。

---

## 4. 实现详解

> 每个结构的引导语解释核心思路，代码注释标注 C 源码对应。实现对应 Ch3 的设计决策。

### 4.1 ProtectionArch trait

> 设计决策：§3.1（两 trait 拆分）、§3.2（PrivilegeLevel 关联类型）、§3.3（OS 语义方法名）

`ProtectionArch` 抽象"谁能访问什么"——特权级定义、内核栈设置、保护结构加载。OS 代码通过此 trait 管理硬件保护机制，无需了解 GDT/TSS/SP_EL1/sscratch 等架构细节。

```rust
/// Architecture abstraction for hardware protection mechanisms.
///
/// Manages privilege levels, kernel stack setup for privilege transitions,
/// and loading protection structures into hardware.
///
/// # Architecture mapping
///
/// | Method          | x86-64                    | ARM64              | RISC-V          |
/// |-----------------|---------------------------|--------------------|-----------------|
/// | `init()`        | Clear GDT, fill segment   | Configure SP_EL0/  | Configure       |
/// |                 | descriptors, create TSS   | SP_EL1, set up     | sscratch, set   |
/// |                 |                           | exception regs     | up trap regs    |
/// | `set_kernel_    | Update TSS.sp0            | Update SP_EL1      | Update sscratch |
/// |  stack()`       |                           |                    |                 |
/// | `load()`        | lgdt, lldt, ltr, reload   | msr SP_EL1, ensure | csrw sscratch,  |
/// |                 | segment registers         | VBAR_EL1 set       | ensure stvec    |
/// | `init_ap()`     | Per-CPU TSS/GDT entry,    | Per-CPU SP_EL1     | Per-CPU         |
/// |                 | load selectors            |                    | sscratch        |
pub trait ProtectionArch: Sized {
    /// Architecture-specific privilege level representation.
    ///
    /// x86-64: Ring 0 (Kernel) / Ring 3 (User) — encoded in segment
    ///         selector RPL field and descriptor DPL field.
    /// ARM64:  EL1 (Kernel) / EL0 (User) — encoded in SPSR_EL1.M.
    /// RISC-V: S-mode (Kernel) / U-mode (User) — encoded in sstatus.SPP.
    type PrivilegeLevel: Copy + Eq + core::fmt::Debug;

    /// Kernel privilege level.
    /// x86-64: Ring 0; ARM64: EL1; RISC-V: S-mode.
    const KERNEL_PRIVILEGE: Self::PrivilegeLevel;

    /// User privilege level.
    /// x86-64: Ring 3; ARM64: EL0; RISC-V: U-mode.
    const USER_PRIVILEGE: Self::PrivilegeLevel;

    /// Convert architecture-specific privilege level to common enum.
    fn to_privilege(level: Self::PrivilegeLevel) -> Privilege;

    /// Convert common privilege enum to architecture-specific level.
    fn from_privilege(privilege: Privilege) -> Self::PrivilegeLevel;

    /// Initialize protection structures for the boot CPU (BSP).
    ///
    /// Called once during `cstart()`, before `TrapEntryArch::init()`.
    ///
    /// C: prot_init() — protect.c:321
    ///    (clears GDT/IDT, sets descriptor table pointers, fills GDT
    ///     entries, calls tss_init() for BSP)
    fn init(cpu_id: u32, kernel_stack_top: VirBytes) -> Self;

    /// Set the kernel stack pointer for privilege level transitions.
    ///
    /// When a transition from user mode to kernel mode occurs (interrupt,
    /// exception, or system call), the CPU automatically switches to this
    /// kernel stack.
    ///
    /// C: tss_init() sets tss.sp0 — protect.c:175
    fn set_kernel_stack(&mut self, cpu_id: u32, stack_top: VirBytes);

    /// Load protection structures into hardware registers.
    ///
    /// After this call, the CPU enforces the protection boundaries
    /// defined by the initialized structures.
    ///
    /// C: prot_load_selectors() — protect.c:298
    ///    (lgdt, lldt, ltr, reload CS/DS/ES/FS/GS/SS)
    fn load(&self);

    /// Initialize protection for an Application Processor (AP).
    ///
    /// Called once per AP during SMP bringup. Creates per-CPU protection
    /// structures (e.g., TSS entry in GDT) and loads them.
    ///
    /// C: tss_init(cpu, stack) + prot_load_selectors() — called from mpx.S
    fn init_ap(&self, cpu_id: u32, kernel_stack_top: VirBytes);
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 | minix-rs |
|------|--------|----------|
| 初始化保护 | `prot_init()` 全局函数修改全局 GDT/IDT | `ProtectionArch::init()` 返回拥有保护状态的结构体 |
| 设置内核栈 | `tss_init()` 修改全局 `tss[cpu]` | `set_kernel_stack()` 修改结构体内部状态 |
| 加载到硬件 | `prot_load_selectors()` 全局函数 | `load()` 方法 |
| 初始化 AP | `tss_init()` + `prot_load_selectors()` | `init_ap()` 方法 |
| 检查初始化完成 | `prot_init_done` 全局标志 | 类型状态（编译时保证） |

### 4.2 TrapEntryArch trait

> 设计决策：§3.1（两 trait 拆分——中断/异常/系统调用统一入口）

`TrapEntryArch` 抽象"如何进入内核"——中断/异常/系统调用的入口配置。x86-64 的 IDT 门描述符和 SYSCALL MSR 配置、ARM64 的异常向量表、RISC-V 的 trap 向量，都通过此 trait 统一管理。

```rust
/// Architecture abstraction for trap entry configuration.
///
/// Manages how the CPU enters the kernel in response to interrupts,
/// exceptions, and system calls. On x86-64, this includes the IDT
/// (Interrupt Descriptor Table) and SYSCALL MSR configuration.
/// On ARM64/RISC-V, the exception/trap vector serves all three purposes.
pub trait TrapEntryArch: Sized {
    /// Initialize the trap entry table with architecture-specific handlers.
    ///
    /// Fills the table with handler addresses for CPU exceptions,
    /// hardware interrupts, and system call vectors.
    ///
    /// C: idt_init() — protect.c:260
    ///    (fills IDT with gate_table_exceptions[] and gate_table_pic[])
    fn init() -> Self;

    /// Configure the system call entry mechanism.
    ///
    /// x86-64: Enable SYSCALL/SYSRET via MSR — sets STAR, LSTAR, SFMASK,
    ///         and enables EFER.SCE. Called after `init()` and before `load()`.
    /// ARM64:  No-op — SVC instruction uses the exception vector set by `init()`.
    /// RISC-V: No-op — ecall instruction uses the trap vector set by `init()`.
    ///
    /// C: tss_init() lines 189-205 — SYSCALL MSR setup
    fn configure_syscall(&mut self, entry_point: VirBytes);

    /// Load the trap entry table into hardware.
    ///
    /// After this call, the CPU will route interrupts, exceptions, and
    /// system calls through the configured entry points.
    ///
    /// C: idt_reload() — protect.c:268
    ///    (x86_lidt(&idt_desc))
    fn load(&self);

    /// Load the trap entry table on an AP.
    ///
    /// On x86-64, the IDT is shared across CPUs, so this just reloads
    /// the IDTR. On ARM64/RISC-V, each CPU has its own VBAR_EL1/stvec.
    fn load_ap(&self);
}
```

**为什么 `configure_syscall` 不是 `ProtectionArch` 的方法**：SYSCALL 的语义是"配置系统调用入口点"，属于"入口配置"范畴。虽然 SYSCALL MSR 配置在 C 源码中位于 `tss_init()` 内部（protect.c:189-205），但这是因为 C 代码按函数组织而非按职责组织。在 Rust 设计中，按职责划分更清晰。

**为什么 `init()` 不需要 `protection` 参数**：虽然 x86-64 的 IDT 门描述符引用段选择符（必须在 GDT 中有效），但 `ProtectionArch::load()` 必须在 `TrapEntryArch::init()` 之前调用（见 §3.1 的时序分析）。因此 `init()` 被调用时，GDT 已经加载，段选择符已经有效。

### 4.3 Privilege 枚举与 PrivilegeLevel 关联类型

> 设计决策：§3.2（PrivilegeLevel 作为 trait 关联类型）

公共枚举 `Privilege` 供 OS 代码在架构无关的上下文中使用。每个架构的 `PrivilegeLevel` 关联类型携带硬件编码值，通过 trait 方法与 `Privilege` 互转。

```rust
/// Common privilege level enum for architecture-agnostic OS code.
///
/// All supported architectures use exactly two privilege levels:
/// kernel (supervisor) and user. This enum provides a unified
/// representation that OS code can use without knowing the
/// architecture-specific encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privilege {
    /// Kernel / supervisor / hypervisor privilege level.
    /// x86-64: Ring 0; ARM64: EL1; RISC-V: S-mode.
    Kernel,
    /// User / unprivileged level.
    /// x86-64: Ring 3; ARM64: EL0; RISC-V: U-mode.
    User,
}
```

x86-64 的 `PrivilegeLevel` 实现：

```rust
/// x86-64 privilege level representation.
///
/// Encodes the x86 Ring level (0 or 3) used in segment selector
/// RPL fields and descriptor DPL fields. The numeric value matches
/// the hardware encoding: Ring 0 = 0b00, Ring 3 = 0b11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X86PrivilegeLevel(u8);

impl X86PrivilegeLevel {
    /// Ring 0 — kernel / interrupt handler privilege.
    /// C: INTR_PRIVILEGE — archconst.h:33
    pub const RING0: Self = Self(0);
    /// Ring 3 — user process privilege.
    /// C: USER_PRIVILEGE — archconst.h:34
    pub const RING3: Self = Self(3);
}

impl ProtectionArch for X86_64Protection {
    type PrivilegeLevel = X86PrivilegeLevel;

    const KERNEL_PRIVILEGE: X86PrivilegeLevel = X86PrivilegeLevel::RING0;
    const USER_PRIVILEGE: X86PrivilegeLevel = X86PrivilegeLevel::RING3;

    fn to_privilege(level: X86PrivilegeLevel) -> Privilege {
        match level {
            X86PrivilegeLevel::RING0 => Privilege::Kernel,
            X86PrivilegeLevel::RING3 => Privilege::User,
            _ => Privilege::User,
        }
    }

    fn from_privilege(privilege: Privilege) -> X86PrivilegeLevel {
        match privilege {
            Privilege::Kernel => X86PrivilegeLevel::RING0,
            Privilege::User => X86PrivilegeLevel::RING3,
        }
    }
    // ... other methods
}
```

**C 行为 vs Rust 行为对照**：

| 操作 | Minix3 | minix-rs |
|------|--------|----------|
| 内核特权级 | `INTR_PRIVILEGE = 0` | `X86PrivilegeLevel::RING0` |
| 用户特权级 | `USER_PRIVILEGE = 3` | `X86PrivilegeLevel::RING3` |
| 设置 DPL | `access |= (privilege << DPL_SHIFT)` | `from_privilege(Privilege::User).0 << DPL_SHIFT` |
| 架构无关比较 | 不存在（直接用整数 0/3） | `to_privilege(level) == Privilege::User` |

### 4.4 InterruptVector 类型

> 设计决策：§3.8（InterruptVector 公共类型）

```rust
/// Interrupt/exception vector number.
///
/// Wraps a u8 vector number with type safety. On x86-64, this
/// corresponds to an IDT vector index (0-255). On ARM64/RISC-V,
/// the meaning is architecture-specific but the type is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptVector(pub u8);

// CPU exception vectors — C: interrupt.h:21-25, archconst.h:38-49
pub const DIVIDE_ERROR: InterruptVector      = InterruptVector(0);
pub const DEBUG: InterruptVector             = InterruptVector(1);
pub const NMI: InterruptVector               = InterruptVector(2);
pub const BREAKPOINT: InterruptVector        = InterruptVector(3);
pub const OVERFLOW: InterruptVector          = InterruptVector(4);
pub const BOUNDS_CHECK: InterruptVector      = InterruptVector(5);
pub const INVALID_OPCODE: InterruptVector    = InterruptVector(6);
pub const DEVICE_NOT_AVAILABLE: InterruptVector = InterruptVector(7);
pub const DOUBLE_FAULT: InterruptVector      = InterruptVector(8);
pub const INVALID_TSS: InterruptVector       = InterruptVector(10);
pub const SEGMENT_NOT_PRESENT: InterruptVector = InterruptVector(11);
pub const STACK_FAULT: InterruptVector       = InterruptVector(12);
pub const GENERAL_PROTECTION: InterruptVector = InterruptVector(13);
pub const PAGE_FAULT: InterruptVector        = InterruptVector(14);

// System call / IPC vectors — C: interrupt.h:28-31
pub const KERN_CALL_VECTOR: InterruptVector  = InterruptVector(32);
pub const IPC_VECTOR: InterruptVector        = InterruptVector(33);
```

**64 位变化**：Minix3 的 `KERN_CALL_VECTOR_ORIG`(32) 和 `KERN_CALL_VECTOR_UM`(34) 是两套 int 指令入口。64 位模式下，系统调用通过 SYSCALL/SYSRET 进入，不再需要 `int 0x20` 路径。但保留向量号常量用于 IDT 填充（SYSCALL 故障时的回退路径）。

### 4.5 X86_64Protection — x86-64 保护结构实现

> 设计决策：§3.3（OS 语义方法名）、§3.4（64 位架构演进）

`X86_64Protection` 封装 x86-64 的 GDT 和 TSS 结构。所有 x86 特有的硬件细节（段描述符编码、TSS 字段布局）都隐藏在此结构内部，OS 代码只通过 `ProtectionArch` trait 方法访问。

```rust
/// x86-64 GDT entry count.
/// C: GDT_SIZE = 6 + CONFIG_MAX_CPUS — archconst.h:21
/// 64-bit: removed LDT_INDEX, so GDT_SIZE = 5 + MAX_CPUS.
///   Index 0: Null descriptor
///   Index 1: Kernel code segment (CS, Ring 0)
///   Index 2: Kernel data segment (DS/SS, Ring 0)
///   Index 3: User code segment (CS, Ring 3)
///   Index 4: User data segment (DS/SS, Ring 3)
///   Index 5+: Per-CPU TSS descriptors
const GDT_NULL_INDEX: usize      = 0;
const GDT_KERN_CS_INDEX: usize   = 1;
const GDT_KERN_DS_INDEX: usize   = 2;
const GDT_USER_CS_INDEX: usize   = 3;
const GDT_USER_DS_INDEX: usize   = 4;
const GDT_TSS_FIRST_INDEX: usize = 5;

/// x86-64 segment selectors.
/// C: archconst.h:24-29 — SEG_SELECTOR(i) = i * 8
/// In 64-bit long mode, CS/SS selectors are still loaded from GDT.
pub const KERN_CS_SELECTOR: u16 = (GDT_KERN_CS_INDEX * 8) as u16;   // 0x08
pub const KERN_DS_SELECTOR: u16 = (GDT_KERN_DS_INDEX * 8) as u16;   // 0x10
pub const USER_CS_SELECTOR: u16 = ((GDT_USER_CS_INDEX * 8) | 3) as u16; // 0x1B
pub const USER_DS_SELECTOR: u16 = ((GDT_USER_DS_INDEX * 8) | 3) as u16; // 0x23

/// x86-64 protection state — owns GDT and per-CPU TSS.
///
/// Internal details (GDT entry layout, TSS structure) are not
/// exposed to the OS layer. The OS interacts only through
/// `ProtectionArch` trait methods.
pub struct X86_64Protection {
    /// GDT entries. 64-bit long mode: code/data segments are flat
    /// (base=0, limit=full), only TSS descriptors have real meaning.
    gdt: [u64; GDT_ENTRIES],
    /// Per-CPU TSS structures. 64-bit TSS includes IST1-IST7.
    tss: [Tss64; MAX_CPUS],
    /// High 64-bit words of per-CPU TSS descriptors in GDT.
    /// Needed because TSS descriptor is 128 bits (two GDT slots),
    /// and the high word must be updated when sp0 changes.
    tss_desc_high: [u64; MAX_CPUS],
    /// Number of initialized CPUs (for TSS tracking).
    cpu_count: u32,
}

/// 64-bit Task State Segment.
///
/// C: tss_s — arch_proto.h:167-203 (32-bit version)
/// 64-bit TSS is different: no general-purpose registers, no segment
/// registers. Only sp0/ss0 (for privilege stack switch), IST1-IST7
/// (Interrupt Stack Table), and iobase are used.
///
/// Key fields used by Minix3:
/// - sp0: Ring 0 stack pointer (hardware auto-loads on Ring3→Ring0)
/// - ss0: Ring 0 stack segment selector (always KERN_DS_SELECTOR)
/// - ist1-ist7: Interrupt Stack Table entries (for NMI, #DF, etc.)
/// - iobase: I/O permission bitmap offset
#[repr(C, packed)]
struct Tss64 {
    _reserved0: u32,
    /// Ring 0 stack pointer — C: tss.sp0
    sp0: u64,
    /// Ring 0 stack segment selector — C: tss.ss0
    ss0: u16,
    _reserved1: u16,
    _reserved2: u32,
    sp1: u64,
    ss1: u16,
    _reserved3: u16,
    _reserved4: u32,
    sp2: u64,
    ss2: u16,
    _reserved5: u16,
    _reserved6: u32,
    _reserved7: u64,
    _reserved8: u64,
    _reserved9: u64,
    _reserved10: u64,
    ist: [u64; 7],     // IST1-IST7
    _reserved11: u64,
    _reserved12: u16,
    iobase: u16,       // I/O permission bitmap offset
}
```

**64 位 TSS vs 32 位 TSS 关键差异**：

| 方面 | 32 位 `tss_s` | 64 位 `Tss64` |
|------|-------------|-------------|
| 大小 | 104 字节 | 约 104 字节（布局不同） |
| 通用寄存器 | 有（ax/bx/cx/dx/si/di/bp/sp） | **无**——64 位不使用硬件任务切换 |
| 段寄存器 | 有（cs/ds/es/fs/gs/ss/ldt） | **无** |
| IST | **无** | 有（ist1-ist7，用于 NMI/#DF 等不可屏蔽异常） |
| cr3 | 有（任务切换时加载） | **无**——64 位不使用硬件任务切换 |
| sp0/ss0 | 有 | 有（语义不变） |
| iobase | 有 | 有（语义不变） |

### 4.6 X86_64TrapEntry — x86-64 陷阱入口实现

> 设计决策：§3.1（TrapEntryArch 统一中断/异常/系统调用入口）

```rust
/// x86-64 trap entry state — owns the IDT.
///
/// In 64-bit long mode, each IDT entry is 16 bytes (vs 8 bytes in
/// 32-bit mode). The IDT is shared across all CPUs.
pub struct X86_64TrapEntry {
    /// IDT entries. 64-bit gate descriptors are 16 bytes each.
    /// C: idt[IDT_SIZE] — protect.c:26, but 32-bit uses 8-byte gates.
    idt: [IdtEntry64; 256],
}

/// 64-bit IDT gate descriptor (16 bytes).
///
/// C: gatedesc_s — archtypes.h:19-26 (8-byte 32-bit version)
/// 64-bit version doubles the offset field to hold a 64-bit handler address.
#[repr(C, packed)]
struct IdtEntry64 {
    offset_low: u16,     // Handler offset bits 0-15
    selector: u16,       // Target code segment selector (KERN_CS_SELECTOR)
    ist: u8,             // IST offset (0 = don't use IST, 1-7 = use ISTn)
    p_dpl_type: u8,      // P(1) + DPL(2) + 0 + Type(5)
    offset_mid: u16,     // Handler offset bits 16-31
    offset_high: u32,    // Handler offset bits 32-63
    _reserved: u32,      // Must be zero
}
```

**64 位 IDT 门描述符 vs 32 位**：

| 字段 | 32 位 `gatedesc_s` | 64 位 `IdtEntry64` |
|------|-------------------|-------------------|
| 总大小 | 8 字节 | 16 字节 |
| 偏移量 | 32 位（offset_low + offset_high） | 64 位（offset_low + offset_mid + offset_high） |
| IST | **无** | 有（ist 字段，0=不用，1-7=使用 ISTn） |
| 保留字段 | **无** | 有（_reserved，必须为 0） |
| selector | 有 | 有（语义不变） |
| p_dpl_type | 有 | 有（语义不变） |

**SYSCALL MSR 配置**（`configure_syscall()` 实现）：

```rust
impl TrapEntryArch for X86_64TrapEntry {
    fn configure_syscall(&mut self, entry_point: VirBytes) {
        // C: tss_init() lines 189-205 — SYSCALL MSR setup
        //
        // 64-bit only: SYSENTER is not supported in 64-bit mode.
        // Only SYSCALL/SYSRET is used.

        // STAR[32:47] = SYSCALL CS (KERN_CS_SELECTOR)
        // STAR[48:63] = SYSRET CS (USER_CS_SELECTOR)
        // C: AMD_MSR_STAR — archconst.h:92
        let star = (KERN_CS_SELECTOR as u64) << 32
                 | (USER_CS_SELECTOR as u64) << 48;
        wrmsr(0xC0000081, star);

        // LSTAR = SYSCALL entry point
        // C: ipc_entry_syscall_cpuN — protect.c:197
        wrmsr(0xC0000082, entry_point.0);

        // SFMASK = flags to clear on SYSCALL
        wrmsr(0xC0000084, 0x200); // Clear IF (interrupt flag)

        // Enable SYSCALL in EFER
        // C: AMD_EFER_SCE — archconst.h:94
        let efer = rdmsr(0xC0000080);
        wrmsr(0xC0000080, efer | 1);
    }
}
```

### 4.7 boot_load_vm 自由函数

> 设计决策：§3.6（boot_load_vm 为自由函数）

`boot_load_vm()` 在自举页表中加载 VM 的 ELF 二进制。它是架构无关的——ELF 解析和栈布局对所有架构相同，只有页表映射通过 `Paging` trait 调用。

```rust
/// Load VM process ELF binary into bootstrap page tables.
///
/// C: arch_boot_proc() for VM_PROC_NR — protect.c:388-447
///
/// This is a free function (not a trait method) because the ELF loading
/// logic is architecture-independent. Only the underlying page table
/// operations (via `P: Paging`) differ across architectures.
///
/// # Arguments
///
/// * `paging` — Bootstrap page table to map VM's pages into
/// * `proc` — VM process structure to set entry point and stack
/// * `boot_module` — Boot module containing VM's ELF binary
///
/// # Returns
///
/// VM's entry point address on success, or a `BootError` on failure.
fn boot_load_vm<P: Paging>(
    paging: &mut P,
    proc: &mut KProcess,
    boot_module: &BootModule,
) -> Result<VirBytes, BootError> {
    // C: exec_info setup — protect.c:396-417
    let elf_data = boot_module.data;
    let entry_point = parse_elf_and_map::<P>(paging, elf_data)?;

    // C: stack setup — protect.c:419-436
    // Allocate 64KB stack, set up ps_strings at stack_high
    let stack_high = user_sp();
    let stack_top = stack_high - 64 * 1024;
    setup_stack(paging, stack_top, stack_high)?;

    // C: arch_proc_init() — sets process entry point and stack pointer
    proc.set_entry_point(entry_point);
    proc.set_stack_pointer(stack_top);

    Ok(entry_point)
}
```

**与 Minix3 的对应**：

| Minix3 步骤 | minix-rs 步骤 | 变化 |
|------------|--------------|------|
| `memset(&execi, 0, sizeof(execi))` | 函数参数直接传递 | 无需 exec_info 结构体 |
| `execi.copymem = libexec_copy_memcpy` | `Paging::map()` 内部处理 | 通过 trait 调用 |
| `execi.allocmem_* = libexec_pg_alloc` | `Paging::map()` + `Paging::new_empty()` | 通过 trait 调用 |
| `libexec_load_elf(&execi)` | `parse_elf_and_map::<P>()` | 架构无关的 ELF 解析 |
| `ps_strings` 设置 | `setup_stack()` | 逻辑相同 |
| `arch_proc_init(rp, ...)` | `proc.set_entry_point()` / `proc.set_stack_pointer()` | 进程结构方法调用 |

### 4.8 启动流程集成

> 设计决策：§3.5（类型状态替代 prot_init_done）、§3.7（arch_post_init 消除）

内核启动流程中，保护模式初始化的调用顺序：

```rust
/// Kernel boot flow — protection and trap entry initialization.
///
/// C: cstart() → prot_init() → prot_load_selectors() — main.c:411
///
/// The type system enforces correct initialization order:
/// 1. ProtectionArch::init() + load() must complete before
///    TrapEntryArch methods are called (GDT must be loaded
///    before IDT gate descriptors' segment selectors are valid).
/// 2. Both must complete before Paging::enable() (page table
///    rebuild requires valid segment selectors).
fn cstart<
    Prot: ProtectionArch,
    Trap: TrapEntryArch,
    Pg: Paging + HugePages,
>(
    kernel_info: &KernelInfo,
    root_page: PhysBytes,
) -> ! {
    // Phase 1: Protection — privilege levels, kernel stack, GDT/TSS
    // C: prot_init() — protect.c:321-368
    let mut protection = Prot::init(0, k_boot_stktop());
    protection.load();

    // Phase 2: Trap entry — IDT, SYSCALL MSR
    // C: prot_load_selectors() calls idt_init() + idt_reload()
    //    — protect.c:298-316
    let mut trap_entry = Trap::init();
    trap_entry.configure_syscall(syscall_entry_point());
    trap_entry.load();

    // Phase 3: Rebuild page tables
    // C: pg_clear() → pg_identity() → pg_mapkernel() → pg_load()
    //    — protect.c:356-358
    let mut paging = Pg::new_empty(root_page);
    // ... identity mapping, kernel mapping (see 01-multiboot-bootstrap §4.5)
    unsafe { paging.enable(); }

    // Phase 4: Load VM binary
    // C: arch_boot_proc() for VM — protect.c:388-447
    let vm_proc = get_vm_process();
    let vm_module = kernel_info.boot_modules[VM_MODULE_INDEX];
    boot_load_vm(&mut paging, vm_proc, vm_module)
        .expect("VM loading failed");

    // Phase 5: Enter main loop
    // C: main() — main.c:257
    kmain(kernel_info, protection, trap_entry, paging)
}
```

**类型状态保证**：

```
BeforeInit
    │
    ▼ ProtectionArch::init()
ProtectionArch exists (can call load(), set_kernel_stack())
    │
    ▼ ProtectionArch::load()
Protection loaded (GDT/TSS in hardware)
    │
    ▼ TrapEntryArch::init()
TrapEntryArch exists (can call configure_syscall(), load())
    │
    ▼ TrapEntryArch::load()
Trap entry loaded (IDT in hardware, SYSCALL configured)
    │
    ▼ Paging::enable()
Paging enabled (full protection + paging active)
```

编译器保证：在 `ProtectionArch::load()` 之前无法调用 `TrapEntryArch::init()`（因为 `TrapEntryArch` 实例还不存在），在 `TrapEntryArch::load()` 之前无法调用 `Paging::enable()`（因为调用顺序由函数体控制）。这比 Minix3 的 `prot_init_done` 运行时标志更安全。

---

## 5. 测试要点

### 5.1 ProtectionArch trait 测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| `init()` 创建有效保护结构 | GDT 条目正确填充，TSS.sp0 指向给定栈顶 | §3.3 |
| `set_kernel_stack()` 更新 TSS | 修改后 TSS.sp0 反映新栈顶 | §3.3 |
| `load()` 后段寄存器正确 | CS=0x08, DS=0x10, SS=0x10 | §3.3 |
| `to_privilege()` / `from_privilege()` 转换 | RING0↔Kernel, RING3↔User 双向正确 | §3.2 |
| `init_ap()` 创建 per-CPU TSS | AP 的 TSS 条目在 GDT 中正确，TR 加载正确 | §3.3 |

### 5.2 TrapEntryArch trait 测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| `init()` 填充 IDT | CPU 异常向量有正确处理程序和 DPL | §3.1 |
| `configure_syscall()` 设置 MSR | STAR/LSTAR/SFMASK/EFER 值正确 | §3.1 |
| `load()` 后 IDTR 正确 | IDT 基地址和界限与结构体一致 | §3.1 |
| 用户态向量 DPL=3 | breakpoint(3)/overflow(4)/syscall 向量可从 Ring 3 触发 | §1.3.3 |

### 5.3 boot_load_vm 测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| 有效 ELF 加载 | 入口点正确，页面映射正确 | §3.6 |
| 无效 ELF 头 | 返回 `BootError::ElfParse(ENOEXEC)` | §3.9 |
| 页面分配失败 | 返回 `BootError::PageAlloc(ENOMEM)` | §3.9 |
| 栈布局正确 | ps_strings 在 stack_high 下方，argc=0 | §2.5.4 |

### 5.4 类型状态测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| 编译时顺序保证 | `load()` 只能在 `init()` 之后调用 | §3.5 |
| 未初始化时调用方法 | 编译错误（类型不匹配） | §3.5 |

### 5.5 64 位演进测试

| 测试场景 | 验证内容 | 对应设计 |
|---------|---------|---------|
| GDT 无 LDT 条目 | 64 位 GDT 布局不包含 LDT_INDEX | §3.4 |
| TSS 为 64 位格式 | Tss64 大小和字段布局正确 | §3.4 |
| IDT 条目 16 字节 | IdtEntry64 大小 = 16 | §3.4 |
| SYSENTER 代码不存在 | 64 位实现中无 SYSENTER MSR 操作 | §3.4 |

---

## 6. 参见

| 文档 | 关联 |
|------|------|
| [01-multiboot-bootstrap.md](01-multiboot-bootstrap.md) | 启动阶段页表建立（Paging trait 的 boot 使用） |
| [02-page-table-kernel.md](02-page-table-kernel.md) | Direct Map 消除临时 PDE 映射（ptproc/arch_post_init 消除的依据） |
| [05-exception-interrupt.md](05-exception-interrupt.md) | 中断/异常处理（TrapEntryArch 的运行时使用） |
| [06-proc-struct.md](06-proc-struct.md) | 进程结构（KProcess 中的保护模式相关字段） |
| [11-privilege.md](11-privilege.md) | 特权级管理（Privilege 枚举的运行时使用） |
| [18-smp.md](18-smp.md) | SMP 启动（ProtectionArch::init_ap() 的调用场景） |
| [99-global-concepts.md](99-global-concepts.md) | 全局概念（Direct Map、Paging trait 定义） |
