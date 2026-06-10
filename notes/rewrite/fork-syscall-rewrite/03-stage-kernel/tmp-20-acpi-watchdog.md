# 20-acpi-watchdog: ACPI 电源管理与看门狗

> **分类**: Kernel 多核与补全
> **源码**: `minix3/minix/kernel/arch/i386/acpi.c`(410行), `watchdog.c`(passive), `arch_watchdog.c`(235行)
> **说明**: ACPI 表解析、电源管理、硬件看门狗定时器

---

## 1. 概述

### 1.1 概念定义/作用

**ACPI（Advanced Configuration and Power Interface）** 是 Minix3 内核用于发现硬件拓扑和实现电源管理的接口。ACPI 提供了两项关键功能：

1. **硬件拓扑发现**：通过 RSDP → RSDT → MADT 表链发现 CPU、中断控制器、I/O APIC 等硬件资源，SMP 初始化依赖此信息
2. **电源管理**：通过 FADT/DSDT 表实现系统关机（S5 睡眠状态），`do_abort()` 调用 ACPI 关机

**硬件看门狗（NMI Watchdog）** 是内核死锁检测的最后防线。看门狗使用 CPU 性能计数器（Performance Counter）作为定时器，当内核长时间未更新看门狗计数时触发 NMI（不可屏蔽中断），强制进入调试或重启流程。

### 1.2 与 Minix3 的对应关系

| 功能 | 函数 | 源文件 |
|------|------|--------|
| ACPI 初始化 | `acpi_init()` | arch/i386/acpi.c |
| RSDP 定位 | `acpi_locate()` | arch/i386/acpi.c |
| RSDT 解析 | `acpi_init_rsdt()` | arch/i386/acpi.c |
| MADT 解析 | `acpi_init_madt()` | arch/i386/acpi.c |
| FADT 解析 | `acpi_init_fadt()` | arch/i386/acpi.c |
| 系统关机 | `acpi_poweroff()` | arch/i386/acpi.c |
| 看门狗初始化 | `arch_watchdog_init()` | arch/i386/arch_watchdog.c |
| 看门狗重置 | `watchdog_reinit()` | arch/i386/arch_watchdog.c |
| 看门狗定时器更新 | `watchdog_timer_queue_handler()` | watchdog.c |
| 看门狗 NMI 处理 | `watchdog_nmi_handler()` | watchdog.c |

### 1.3 关键状态/机制说明

**ACPI 表链**：ACPI 信息通过层次化的表结构组织：

```
RSDP (Root System Description Pointer)
  └─ RSDT (Root System Description Table)
       ├─ MADT (Multiple APIC Description Table) → CPU 拓扑、APIC 信息
       ├─ FADT (Fixed ACPI Description Table) → 电源管理寄存器
       │    └─ DSDT (Differentiated System Description Table) → AML 字节码
       └─ 其他 SSDT 表
```

RSDP 是入口点，存储在 BIOS 区域（0xE0000-0xFFFFF 或 UEFI 系统表）中。RSDT 包含指向所有其他表的物理地址数组。

**ACPI 关机流程**：`acpi_poweroff()` 通过向 PM1a_CNT / PM1b_CNT 寄存器写入 SLP_TYP + SLP_EN 实现关机。SLP_TYP 值从 DSDT 表的 S5 包中解析——S5 是 ACPI 定义的"软关机"状态。

**NMI 看门狗**：使用 CPU 性能计数器的溢出中断作为 NMI 触发源。Intel 使用架构性能计数器（`MSR_PERFMON_CRT0/SEL0`），AMD 使用特定系列的性能计数器。计数器设为 CPU 频率的一半（约 0.5-1 秒溢出），正常情况下时钟中断定期重置计数器。若内核死锁导致时钟中断停止，计数器溢出触发 NMI。

### 1.4 行为规则

1. **ACPI 仅在 VM 运行前使用**：`acpi_phys_copy()` 在 VM 运行后 panic，因为物理地址映射不再直接可用
2. **RSDP 搜索范围**：BIOS 区域 0xE0000-0xFFFFF 和扩展 BIOS 区域 0x9FC00-0xA0000
3. **ACPI 表校验**：每个表的校验和必须为 0（所有字节之和 mod 256 == 0）
4. **看门狗需要 APIC**：NMI 看门狗依赖 Local APIC 的 LVT PC 寄存器，无 APIC 则无法使用
5. **看门狗仅支持 Intel/AMD**：Intel 需要架构性能计数器支持，AMD 仅支持 family 6/15/16/17
6. **看门狗计数器 31 位**：Intel 性能计数器仅最低 31 位可写，CPU 频率超过 2^31 Hz 时需多次除 2

## 2. C 源码分析

### 2.1 相关定义（常量、配置等）

#### 2.1.1 ACPI 表签名

| 签名 | 含义 |
|------|------|
| `RSDP` | Root System Description Pointer |
| `RSDT` | Root System Description Table |
| `MADT` | Multiple APIC Description Table |
| `FADT` | Fixed ACPI Description Table |
| `DSDT` | Differentiated System Description Table |

#### 2.1.2 ACPI 电源管理常量

| 常量 | 值 | 含义 |
|------|-----|------|
| `SLP_EN_CODE` | `1 << 13` | 睡眠使能位 |
| `AMI_SLP_TYPA_SHIFT` | 10 | SLP_TYPa 在 S5 包中的位移 |
| `AMI_SLP_TYPB_SHIFT` | 10 | SLP_TYPb 在 S5 包中的位移 |

#### 2.1.3 看门狗相关 MSR

| 常量 | 含义 |
|------|------|
| `INTEL_MSR_PERFMON_CRT0` | Intel 性能计数器 0 |
| `INTEL_MSR_PERFMON_SEL0` | Intel 性能计数器选择寄存器 0 |
| `INTEL_MSR_PERFMON_SEL0_ENABLE` | 计数器使能位 |

#### 2.1.4 看门狗相关 APIC 寄存器

| 常量 | 含义 |
|------|------|
| `LAPIC_LVTPCR` | Local APIC LVT 性能计数器寄存器 |
| `APIC_ICR_DM_NMI` | 中断投递模式：NMI |
| `APIC_ICR_INT_MASK` | 中断屏蔽位 |

### 2.2 核心数据结构

#### 2.2.1 struct acpi_rsdp（RSDP 结构）

| 字段 | 类型 | 含义 |
|------|------|------|
| `signature[8]` | `char` | 签名 "RSD PTR " |
| `checksum` | `u8_t` | 校验和 |
| `oem_id[6]` | `char` | OEM 标识 |
| `revision` | `u8_t` | ACPI 版本 |
| `rsdt_address` | `u32_t` | RSDT 物理地址 |
| `length` | `u32_t` | RSDP 长度（ACPI 2.0+） |
| `xsdt_address` | `u64_t` | XSDT 物理地址（ACPI 2.0+） |

#### 2.2.2 struct acpi_sdt_header（SDT 表头）

| 字段 | 类型 | 含义 |
|------|------|------|
| `signature[4]` | `char` | 表签名 |
| `length` | `u32_t` | 表长度 |
| `revision` | `u8_t` | 修订版本 |
| `checksum` | `u8_t` | 校验和 |
| `oem_id[6]` | `char` | OEM 标识 |
| `oem_table_id[8]` | `char` | OEM 表标识 |
| `oem_revision` | `u32_t` | OEM 修订版本 |
| `creator_id` | `u32_t` | 创建者标识 |
| `creator_revision` | `u32_t` | 创建者修订版本 |

#### 2.2.3 struct arch_watchdog（看门狗结构）

| 字段 | 类型 | 含义 |
|------|------|------|
| `init` | `void (*)(unsigned cpu)` | 初始化函数 |
| `reinit` | `void (*)(unsigned cpu)` | 重置函数 |
| `resetval` | `u64_t` | 计数器重置值 |
| `watchdog_resetval` | `u64_t` | 原始重置值（备份） |

### 2.3 关键函数分析

#### 2.3.1 acpi_init()——ACPI 初始化

`minix3/minix/kernel/arch/i386/acpi.c`

```c
int acpi_init(int (*read_func)(phys_bytes, void *, size_t))
```

**功能**：初始化 ACPI 子系统，解析所有 ACPI 表。

**行为**：
1. 保存 `read_func`（VM 运行前为直接内存读取，VM 运行后不可用）
2. 定位 RSDP：`acpi_locate()` 在 BIOS 区域搜索 "RSD PTR " 签名
3. 验证 RSDP 校验和
4. 读取 RSDT：`acpi_init_rsdt()` 解析 RSDT 中的表地址数组
5. 遍历 RSDT 中的每个表地址：
   - `MADT`：`acpi_init_madt()` 解析 CPU 和 APIC 信息
   - `FADT`：`acpi_init_fadt()` 解析电源管理寄存器
   - 其他表：记录签名和长度

#### 2.3.2 acpi_init_madt()——MADT 解析

```c
static int acpi_init_madt(struct acpi_madt *madt)
```

**功能**：解析 MADT 表，提取 CPU 和 APIC 信息。

**行为**：
1. 验证 MADT 签名和校验和
2. 记录 Local APIC 物理地址
3. 遍历 MADT 中的中断控制器结构：
   - **Processor Local APIC**：记录 CPU ID、APIC ID、是否可用
   - **I/O APIC**：记录 I/O APIC 地址和中断基
   - **Interrupt Source Override**：记录 IRQ 覆盖映射
4. 更新 `ncpus`（可用 CPU 数量）

#### 2.3.3 acpi_poweroff()——ACPI 关机

```c
void acpi_poweroff(void)
```

**功能**：通过 ACPI S5 状态实现系统关机。

**行为**：
1. 向 PM1a_CNT 写入 `slp_typa | SLP_EN_CODE`
2. 向 PM1b_CNT 写入 `slp_typb | SLP_EN_CODE`
3. 若写入失败或硬件不支持，函数返回（关机失败）

**SLP_TYP 值来源**：从 DSDT 表的 S5 包中解析。S5 包是 AML（ACPI Machine Language）字节码，Minix3 使用简化的 AML 解析器提取 SLP_TYPa 和 SLP_TYPb 值。

#### 2.3.4 arch_watchdog_init()——看门狗初始化

`minix3/minix/kernel/arch/i386/arch_watchdog.c:53-99`

```c
int arch_watchdog_init(void)
```

**功能**：初始化 NMI 看门狗。

**行为**：
1. 检查 Local APIC 是否可用
2. 根据 CPU 厂商选择看门狗实现：
   - **Intel**：检查架构性能计数器可用性（CPUID.0AH:EBX[0]），使用 `intel_arch_watchdog`
   - **AMD**：检查 CPU family（6/15/16/17），使用 `amd_watchdog`
   - 其他：返回 -1
3. 设置 Local APIC LVT PC 寄存器为 NMI 模式（先屏蔽）
4. 调用 `watchdog->init(cpuid)` 初始化性能计数器

#### 2.3.5 intel_arch_watchdog_init()——Intel 看门狗初始化

`minix3/minix/kernel/arch/i386/arch_watchdog.c:17-45`

```c
static void intel_arch_watchdog_init(const unsigned cpu)
```

**功能**：初始化 Intel 架构性能计数器作为看门狗。

**行为**：
1. 清零性能计数器 0：`ia32_msr_write(INTEL_MSR_PERFMON_CRT0, 0, 0)`
2. 配置计数器选择寄存器：INT + OS + USR + Core Cycles 事件（0x3C）
3. 计算重置值：CPU 频率 / 2（约 0.5-1 秒溢出），限制在 31 位以内
4. 写入计数器初始值
5. 使能计数器
6. 设置 Local APIC LVT PC 为 NMI 模式

### 2.4 调用关系/调用点分析

#### 2.4.1 ACPI 初始化路径

```
kmain()
  └─ cstart()
       └─ acpi_init(acpi_phys_copy)
            ├─ acpi_locate() → 搜索 RSDP
            ├─ acpi_init_rsdt() → 解析 RSDT
            ├─ acpi_init_madt() → CPU/APIC 信息
            │    └─ 更新 ncpus, cpu_info[], ioapic_info[]
            └─ acpi_init_fadt() → 电源管理寄存器
                 └─ 解析 DSDT 中的 S5 包
```

#### 2.4.2 系统关机路径

```
do_abort() / panic()
  └─ minix_shutdown()
       └─ arch_shutdown()
            └─ acpi_poweroff()
                 ├─ outw(PM1a_CNT, slp_typa | SLP_EN)
                 └─ outw(PM1b_CNT, slp_typb | SLP_EN)
```

#### 2.4.3 看门狗运行路径

```
时钟中断 → timer_int_handler()
  └─ watchdog_local_timer_ticks++

看门狗定时器到期 → NMI 中断
  └─ watchdog_nmi_handler()
       ├─ 检查 watchdog_local_timer_ticks 是否增长
       ├─ [未增长?] → 内核死锁
       │    ├─ 打印诊断信息
       │    └─ 可能重启
       └─ [已增长?] → 重置计数器
            └─ watchdog->reinit(cpuid)
```

### 2.5 设计要点/特殊处理

#### 2.5.1 ACPI 仅在 VM 运行前可用

ACPI 表位于物理内存中，VM 运行后物理地址映射不再直接可用。`acpi_phys_copy()` 在 VM 运行后调用会 panic。这意味着所有 ACPI 初始化必须在 `kmain()` 的早期阶段完成，VM 运行后不再访问 ACPI 表。

#### 2.5.2 简化的 AML 解析

Minix3 实现了一个极简的 AML 解析器，仅提取 S5 包中的 SLP_TYP 值。完整的 AML 解释器过于复杂（ACPI 规范定义了完整的 AML 虚拟机），Minix3 选择仅实现关机所需的最小功能。

#### 2.5.3 看门狗的 31 位限制

Intel 性能计数器仅最低 31 位可写。对于高频 CPU（>2^31 Hz ≈ 2.1 GHz），计数器值需要多次除 2 才能放入 31 位。这导致看门狗超时时间可能长于 1 秒，但仍在可接受范围内。

#### 2.5.4 看门狗与 BKL 的交互

看门狗 NMI 是不可屏蔽中断，即使 CPU 在自旋等待 BKL 时也能触发。这使得看门狗能检测 BKL 死锁——若某个 CPU 持有 BKL 过久（超过看门狗超时），NMI 中断处理程序可以输出诊断信息。

#### 2.5.5 MADT 对 SMP 的必要性

SMP 初始化完全依赖 MADT 表提供的信息：CPU 数量、APIC ID、I/O APIC 地址。若 MADT 不存在或解析失败，`ncpus` 保持为 1，系统以单 CPU 模式运行。这是 Minix3 的容错设计——ACPI 不可用时回退到单 CPU。

#### 2.5.6 看门狗的被动模式

`watchdog.c` 中的看门狗定时器使用 `minix_timer_t` 实现，定期调用 `watchdog_timer_queue_handler()` 更新 `watchdog_local_timer_ticks`。若时钟中断停止（内核死锁），此变量不再增长，NMI 处理程序检测到后触发诊断或重启。
