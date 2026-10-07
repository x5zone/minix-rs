# 架构相关（i386 特定）

> **机制关注点**: 无。这些是纯硬件相关代码，仅作为参考。
> 
> **学习建议**: 现代实现应使用 trait 抽象，避免直接依赖 i386 特性。

---

## C.1 启动与入口

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 1 | `kernel/arch/i386/head.S` | ⏳ 待读 | 内核启动入口（汇编） | - |
| 2 | `kernel/arch/i386/mpx.S` | ✅ 已读 | 中断/异常/IPC 入口、上下文恢复、SMP 启动 | [📖](../../tmp/ipc/tmp_mpx.S.md) |
| 3 | `kernel/arch/i386/klib.S` | ⏳ 待读 | 内核库函数（汇编） | - |
| 4 | `kernel/arch/i386/pre_init.c` | ⏳ 待读 | 早期初始化 | - |
| 5 | `kernel/arch/i386/usermapped_glo_ipc.S` | ⏳ 待读 | 用户映射全局 IPC 数据 | - |

---

## C.2 内存与保护

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 6 | `kernel/arch/i386/memory.c` | ⏳ 待读 | 内存管理 | - |
| 7 | `kernel/arch/i386/protect.c` | ⏳ 待读 | 保护模式设置 | - |
| 8 | `kernel/arch/i386/pg_utils.c` | ⏳ 待读 | 页表工具 | - |

---

## C.3 I/O 端口

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 9 | `kernel/arch/i386/io_inb.S` | ⏳ 待读 | I/O 输入字节 | - |
| 10 | `kernel/arch/i386/io_inw.S` | ⏳ 待读 | I/O 输入字 | - |
| 11 | `kernel/arch/i386/io_inl.S` | ⏳ 待读 | I/O 输入长字 | - |
| 12 | `kernel/arch/i386/io_outb.S` | ⏳ 待读 | I/O 输出字节 | - |
| 13 | `kernel/arch/i386/io_outw.S` | ⏳ 待读 | I/O 输出字 | - |
| 14 | `kernel/arch/i386/io_outl.S` | ⏳ 待读 | I/O 输出长字 | - |

---

## C.4 SMP 支持

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 15 | `kernel/arch/i386/arch_smp.c` | ⏳ 待读 | SMP 架构支持 | - |
| 16 | `kernel/arch/i386/trampoline.S` | ⏳ 待读 | SMP 启动跳板 | - |
| 17 | `kernel/smp.c` | ⏳ 待读 | SMP 核心实现 | - |
| 18 | `kernel/smp.h` | ⏳ 待读 | SMP 头文件 | - |

---

## C.5 其他

| 序号 | 文件 | 状态 | 说明 | 讲解笔记 |
|------|------|------|------|----------|
| 19 | `kernel/arch/i386/acpi.c` | ⏳ 待读 | ACPI 支持 | - |
| 20 | `kernel/arch/i386/acpi.h` | ⏳ 待读 | ACPI 头文件 | - |
| 21 | `kernel/arch/i386/debugreg.S` | ⏳ 待读 | 调试寄存器 | - |
| 22 | `kernel/arch/i386/debugreg.h` | ⏳ 待读 | 调试寄存器头文件 | - |
| 23 | `kernel/arch/i386/breakpoints.c` | ⏳ 待读 | 断点支持 | - |
| 24 | `kernel/arch/i386/arch_system.c` | ⏳ 待读 | 架构相关系统调用 | - |
| 25 | `kernel/arch/i386/arch_do_vmctl.c` | ⏳ 待读 | 架构相关 VM 控制 | - |
| 26 | `kernel/arch/i386/arch_watchdog.c` | ⏳ 待读 | 看门狗 | - |
| 27 | `kernel/arch/i386/arch_reset.c` | ⏳ 待读 | 系统重启 | - |
| 28 | `kernel/arch/i386/do_readbios.c` | ⏳ 待读 | 读取 BIOS | - |
| 29 | `kernel/arch/i386/do_iopenable.c` | ⏳ 待读 | I/O 权限 | - |
| 30 | `kernel/arch/i386/do_sdevio.c` | ⏳ 待读 | 安全设备 I/O | - |
| 31 | `kernel/arch/i386/oxpcie.c` | ⏳ 待读 | OxPCIe 支持 | - |
| 32 | `kernel/arch/i386/oxpcie.h` | ⏳ 待读 | OxPCIe 头文件 | - |
| 33 | `kernel/arch/i386/direct_tty_utils.c` | ⏳ 待读 | 直接 TTY 工具 | - |
| 34 | `kernel/arch/i386/usermapped_data_arch.c` | ⏳ 待读 | 用户映射数据架构相关 | - |
| 35 | `kernel/arch/i386/sconst.h` | ⏳ 待读 | 架构常量 | - |
| 36 | `kernel/arch/i386/glo.h` | ⏳ 待读 | 架构全局变量 | - |
| 37 | `kernel/arch/i386/serial.h` | ⏳ 待读 | 串口头文件 | - |

---

## 核心概念总结

### i386 特定机制
- **保护模式**：GDT、LDT、TSS
- **分页机制**：两级页表（32 位）
- **I/O 端口**：in/out 指令
- **中断处理**：IDT、8259/APIC

### 现代架构抽象
- **Rust trait**：抽象硬件接口
- **设备树**：描述硬件拓扑
- **ACPI**：电源管理和设备发现
- **UEFI**：现代引导接口

---

## 进度统计

| 分类 | 已读 | 待读 | 覆盖率 |
|------|------|------|--------|
| 启动与入口 | 1 | 4 | 20% |
| 内存与保护 | 0 | 3 | 0% |
| I/O 端口 | 0 | 6 | 0% |
| SMP 支持 | 0 | 4 | 0% |
| 其他 | 0 | 19 | 0% |
| **总计** | **1** | **36** | **2.7%** |
