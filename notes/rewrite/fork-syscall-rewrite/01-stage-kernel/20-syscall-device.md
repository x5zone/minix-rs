# 20-syscall-device: 设备 I/O 系统调用

> **分类**: 系统调用服务
> **C 源码**: `minix3/minix/kernel/system/do_irqctl.c` (174 行), `do_devio.c` (107 行), `do_vdevio.c` (165 行); `minix3/minix/kernel/arch/i386/do_sdevio.c` (162 行), `do_iopenable.c` (34 行), `do_readbios.c` (37 行)
> **Rust 实现**: `os/kernel/src/syscall_device.rs` (1464 行), `os/kernel/src/irq_manager.rs`, `os/kernel/src/syscall.rs`
> **覆盖**: IRQ 控制（hook 注册/启用/禁用）、单次/批量/跨进程端口 I/O、IOPL 提权、BIOS 读取、跨架构抽象
> **前置**: [14-exception-interrupt.md](../14-exception-interrupt.md) (IRQ 入口与 IrqManager 设计), [22-privilege.md](../22-privilege.md) (CHECK_IO_PORT / CHECK_IRQ 权限), [13-syscall-dispatch.md](../13-syscall-dispatch.md) (D9 BadCall 决策), [18-syscall-copy.md](../18-syscall-copy.md) (data_copy_vmcheck / virtual_copy_vmcheck / verify_grant 依赖), [16-smp.md](../16-smp.md) (BKL 串行化)
> **范围边界**: SYS_VMCTL 不纳入本文档——VMCTL 属内存系统调用，与设备 I/O 正交。

---

## Ch1. 概念建构

**核心问题**: 内核如何让用户态驱动安全访问硬件？

设备 I/O 系统调用回答两个正交问题：(1) 驱动如何收到硬件中断？(2) 驱动如何读写 I/O 端口？内核的回答是"钩子 + 权限"——用 IRQ 钩子把中断翻译成 HARDWARE 通知唤醒驱动；用 `CHECK_IO_PORT` / `CHECK_IRQ` 权限表把端口/向量限制在驱动被授权的范围内；x86-only 特性（IOPL/BIOS）用 trait + BadCall 在非 x86 上退化。

### §1.1 IRQ 控制：中断如何变成通知

**灵魂本质**: IRQ 钩子是驱动-中断解耦机制——驱动注册钩子，内核把硬件中断翻译成 HARDWARE 通知唤醒驱动。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 中断上下文不可阻塞、不可执行用户态代码；但驱动逻辑必须在用户态运行。需要一个"翻译层"把硬件中断转为可被驱动 `RECEIVE` 的消息。
- **WHAT**: IRQ 钩子（hook）是 `(endpoint, notify_id, policy)` 三元组。驱动通过 `SYS_IRQCTL` 注册钩子；硬件中断触发时，内核 `generic_handler` 设置 `s_int_pending` 位 + 发 `mini_notify(HARDWARE, endpoint)`，驱动下次 `RECEIVE` 即拿到中断事件。
- **HOW**: C 用 `do_irqctl()` ([do_irqctl.c:23-138](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/system/do_irqctl.c)) 的 4 子请求管理钩子生命周期；`generic_handler()` ([do_irqctl.c:143-172](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/system/do_irqctl.c)) 在中断上下文执行"随机数→位图→通知"三步。

**4 个子请求的语义**:

| 子请求 | 语义 | C 常量 |
|--------|------|--------|
| SETPOLICY | 注册钩子（权限→查重→空闲槽→安装→返回 1-based hook_id） | `IRQ_SETPOLICY` |
| RMPOLICY | 删除钩子（验主→rm→清槽） | `IRQ_RMPOLICY` |
| ENABLE | 启用中断线（验主→enable_irq） | `IRQ_ENABLE` |
| DISABLE | 禁用中断线（验主→disable_irq） | `IRQ_DISABLE` |

**generic_handler 的副作用链** ([do_irqctl.c:143-172](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/system/do_irqctl.c)):

1. `get_randomness(&krandom, hook->irq)` — 采集中断作为随机熵源（/dev/random）
2. `priv(proc)->s_int_pending |= (1 << hook->notify_id)` — 位图记账（驱动位待取）
3. `mini_notify(HARDWARE, hook->proc_nr_e)` — 唤醒驱动（异步通知）
4. 返回 `policy & IRQ_REENABLE` 控制是否重启用中断线——驱动声明 REENABLE 后无需手动 ENABLE，内核自动重启用中断线。

**关键约束**:

1. `notify_id ≤ 31`（`s_int_pending` 是 `u32` 位图）
2. 只有钩子 owner 能 RMPOLICY/ENABLE/DISABLE（[do_irqctl.c:46,126](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/system/do_irqctl.c)）
3. 进程退出必须摘钩（不变式；违例 panic — [do_irqctl.c:160-161](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/system/do_irqctl.c)）

### §1.2 端口 I/O：DEVIO/VDEVIO/SDEVIO 的语义分层

**灵魂本质**: 三种端口 I/O 是单次 / 批量 / 跨进程批量三个抽象层，共享 type/direction 解码与 `CHECK_IO_PORT` 权限。

**三层抽象**:

| 系统调用 | 抽象层 | 语义 | C 函数 |
|---------|--------|------|--------|
| SYS_DEVIO | 单次 | 一次读/写一个端口 | `do_devio()` |
| SYS_VDEVIO | 批量 | 一次处理 N 个 (port,value) 对（buffer 在调用者空间） | `do_vdevio()` |
| SYS_SDEVIO | 跨进程批量 | 一次处理 N 个元素（buffer 在另一进程空间，可走 grant） | `do_sdevio()` |

**共享的解码 + 权限**:

- request 解码：`_DIO_TYPEMASK`(0x0F0) 提取 type（byte/word/long），`_DIO_DIRMASK`(0x00F) 提取 dir（input/output）
- `CHECK_IO_PORT`：扫描 `s_io_tab[]`，要求 `port >= base && port+size-1 <= limit`
- 对齐检查：`port & (size-1)` 必须为 0（word/long 自然对齐）

**VDEVIO 的批量语义** ([do_vdevio.c:25-164](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/system/do_vdevio.c)):

1. `data_copy` 从用户拷入 (port,value) 向量到内核 `vdevio_buf`
2. 批量 `CHECK_IO_PORT`（逐元素扫 `s_io_tab`）
3. 批量 in/out（byte 无对齐；word/long 内联对齐检查，违例 panic）
4. input 模式 `data_copy` 拷回结果

**SDEVIO 的跨进程语义** ([do_sdevio.c:24-161](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/do_sdevio.c)):

1. SELF/endpoint 验证 + 拒绝 kernel 目标
2. `_DIO_SAFE` 区分 safe（`verify_grant` 映射 grant→物理地址）与 unsafe（要求 target==caller）
3. `switch_address_space(destproc)` 切到目标空间
4. 仅支持 byte/word（long 不支持）；`phys_insb/outsb/insw/outsw` 批量原语
5. `switch_address_space(caller)` 切回

### §1.3 x86-only 特性：IOPENABLE/READBIOS 的架构范围

**灵魂本质**: IOPL 提权与 BIOS 读取是 x86 历史包袱，内核只承担"概念"，编码下沉到 arch 层。

**IOPENABLE**:

- 语义：给用户态进程 IOPL=3 权限，允许其执行 `in/out` 指令访问**所有**端口（绕过 CHECK_IO_PORT）
- C 实现：`do_iopenable()` ([do_iopenable.c:19-33](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/do_iopenable.c)) → `enable_iop()` 设 `p_reg.psw |= 0x3000`
- x86 范围：IOPL 是 x86 RFLAGS 第 12-13 位；ARM/RISC-V 无对应概念
- 内核层抽象：只知"enable user I/O"概念，arch 层决定编码（x86: IOPL=3；其他: no-op）

**READBIOS**:

- 语义：从 BIOS 内存区拷贝数据到用户 buffer（用户态不可直接读物理 BIOS 区）
- C 实现：`do_readbios()` ([do_readbios.c:15-37](file:///home/xzhao/github/minix-rs/minix3/minix/kernel/arch/i386/do_readbios.c)) → `virtual_copy_vmcheck`（src=NONE 物理地址）
- BIOS 内存范围两段：
  - `0x0..=0x4FF`（IVT+BIOS data，低段）
  - `0x90000..=0xFFFFF`（upper memory area 含 EBDA，高段）
- USERRANGE 检查：首尾都要在任一段范围内（跨段拒绝）

**架构范围标注**: IOPENABLE/READBIOS/DEVIO/VDEVIO/SDEVIO 都是 x86-only——I/O 端口是 x86 特有概念。ARM/RISC-V 用内存映射 I/O（MMIO），不需要这些调用。IRQCTL 是唯一跨架构的设备调用（IRQ 概念跨架构，由 `InterruptController` trait 抽象）。

### §1.4 架构抽象：PortIo trait + BadCall 替代 #[cfg]

**灵魂本质**: x86 端口 I/O 与 ARM/RISC-V MMIO 共享 trait 接口，非 x86 调用返回 BadCall 而非分散条件编译。

**跨架构差异表**:

| CPU 问题 | x86_64 | aarch64 | riscv64 | Rust 抽象 |
|---------|--------|---------|---------|-----------|
| 端口读 | `inb/inw/inl` 指令 | MMIO load | MMIO load | `PortIo::inb/inw/inl` |
| 端口写 | `outb/outw/outl` 指令 | MMIO store | MMIO store | `PortIo::outb/outw/outl` |
| 中断控制器 | PIC/APIC | GIC | PLIC | `InterruptController` trait |
| 用户态 I/O 提权 | RFLAGS.IOPL=3 | no-op | no-op | `CpuContextArch::enable_user_io` |
| BIOS 数据 | 物理内存 0x0-0xFFFFF | 无 | 无 | (x86-only，BadCall on others) |

> **注**: aarch64/riscv64 的 PortIo trait 实现尚未落地（MMIO 映射需 per-board 驱动）。当前 `dispatch_devio`/`dispatch_vdevio` 在非 x86 上由 dispatch 层 `CurrentArchSyscall` trait 默认方法返回 `KcallResult::BadCall`，不会调用 PortIo 方法。x86_64 的 PortIo 实现见 [x86_64/port_io.rs](file:///home/xzhao/github/minix-rs/os/plat/src/x86_64/port_io.rs)。

**设计原则**: 内核代码（`syscall_device.rs`）只依赖 trait 方法，不出现 `#[cfg(target_arch)]` 行为选择。各架构在 arch 层提供 trait 实现。x86-only 调用在非 x86 上由 dispatch 层 `CurrentArchSyscall` trait 默认方法返回 `KcallResult::BadCall`（参见 [13-syscall-dispatch.md](../13-syscall-dispatch.md) D6 全局决策）。

---

## Ch2. C 源码分析

### §2.1 do_irqctl.c — IRQ 控制

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_irqctl()` | do_irqctl.c:23-138 | `switch(request)`: SETPOLICY(57-120) / RMPOLICY(122-132) / ENABLE(42-52) / DISABLE(42-52) |
| SETPOLICY 权限检查 | do_irqctl.c:62-82 | `CHECK_IRQ` + 扫描 `s_irq_tab[]` |
| SETPOLICY notify_id 上限 | do_irqctl.c:88 | `> CHAR_BIT*sizeof(irq_id_t)-1` → EINVAL |
| SETPOLICY 钩子池查找 | do_irqctl.c:90-108 | 先覆盖同 (ep, nid)，再找空闲；满→ENOSPC |
| SETPOLICY 安装 | do_irqctl.c:110-120 | `put_irq_handler(hook, vec, generic_handler)`；返回 `hook_id+1` |
| ENABLE/DISABLE 校验 | do_irqctl.c:44-46 | hook_id 范围 + `proc_nr_e != NONE` + owner 检查 |
| `generic_handler()` | do_irqctl.c:143-172 | get_randomness(154) → isokendpt(160) → s_int_pending(167) → mini_notify(170) → return policy&IRQ_REENABLE(171) |

### §2.2 do_devio.c — 单次端口 I/O

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_devio()` | do_devio.c:19-106 | 解析 type/dir(27-36) → CHECK_IO_PORT(38-59) → 对齐(61-67) → in/out(69-103) |
| type 解码 | do_devio.c:30-36 | `_DIO_BYTE→size=1, _DIO_WORD→size=2, _DIO_LONG→size=4`；default size=4 |
| `CHECK_IO_PORT` | do_devio.c:44-58 | 扫 `s_io_tab[]`：`port>=base && port+size-1<=limit` |
| 对齐检查 | do_devio.c:62-67 | `port & (size-1)` → EPERM |
| input 写回 | do_devio.c:70-86 | 结果写 `m_krn_lsys_sys_devio.value` |
| 无 priv → goto doit | do_devio.c:39-43,61 | 内核进程跳过检查 |

### §2.3 do_vdevio.c — 批量端口 I/O

| 符号 | 位置 | 说明 |
|------|------|------|
| `vdevio_buf` 静态缓冲区 | do_vdevio.c:17-20 | `char[1024]`；pvb/pvw/pvl 三种 pair cast 复用 |
| `do_vdevio()` | do_vdevio.c:25-164 | 解析(44-64) → size 校验(65) → 拷入(67-70) → 批量权限(72-100) → 批量 I/O(102-149) → 拷回(151-156) |
| vec_size 校验 | do_vdevio.c:49,65 | `<=0`→EINVAL；`bytes>buf`→E2BIG |
| 批量 `CHECK_IO_PORT` | do_vdevio.c:72-100 | 逐元素扫 `s_io_tab`；失败→EPERM |
| word/long 对齐 panic | do_vdevio.c:116,125,135,145,159-161 | `port&1`/`port&3` → `panic("unaligned port")` |
| 拷回结果 | do_vdevio.c:151-156 | input 模式 `data_copy` 回用户 |

### §2.4 do_sdevio.c — 跨进程批量端口 I/O

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_sdevio()` | do_sdevio.c:24-161 | endpoint 验证(56-61) → safe/unsafe 分支(67-93) → switch_space(96) → size(98-104) → 权限(106-124) → 对齐(126-131) → phys_*(133-150) → switch_back(159) |
| SELF/isokendpt/iskerneln | do_sdevio.c:56-61 | SELF→caller；kernel 拒绝 |
| `_DIO_SAFE` 分支 | do_sdevio.c:40,68-82 | safe: `verify_grant` 映射 grant→物理地址 |
| unsafe 分支 | do_sdevio.c:83-93 | 要求 `proc_nr == _ENDPOINT_P(caller)`；否则 EPERM |
| `switch_address_space` | do_sdevio.c:96,159 | 切到目标空间执行 phys_*；完成切回 |
| 仅 byte/word | do_sdevio.c:100-104,138-148 | long → EINVAL；`phys_insl` 不存在 |
| `phys_insb/outsb/insw/outsw` | do_sdevio.c:134-150 | 按 count 重复的批量 I/O 原语 |

### §2.5 do_iopenable.c — IOPL 提权

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_iopenable()` | do_iopenable.c:19-33 | SELF→okendpt(24-25) → isokendpt(26-27) → enable_iop(28) → OK(29) |
| `enable_iop()` | (arch) | `pp->p_reg.psw |= 0x3000` 设 IOPL=3 |
| `#if ENABLE_USERPRIV` | do_iopenable.c:23,31 | 编译期门控；关闭时返回 EPERM |

### §2.6 do_readbios.c — BIOS 读取

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_readbios()` | do_readbios.c:15-37 | src=NONE/buf(21-24) → limit(26) → USERRANGE(28-34) → virtual_copy_vmcheck(36) |
| BIOS 内存范围 | do_readbios.c:32-33 | `BIOS_MEM_BEGIN..END`(0x0-0x4FF) + `BASE_MEM_TOP..UPPER_MEM_END`(0x90000-0xFFFFF) |
| USERRANGE 宏 | do_readbios.c:28-30 | `VINRANGE(src,a,b) && VINRANGE(limit,a,b)`；首尾都要在范围内 |
| `virtual_copy_vmcheck` | do_readbios.c:36 | src 是物理地址（NONE endpoint），dst 是 caller buffer |

### §2.7 调用关系图

```
SYS_IRQCTL → do_irqctl() ── SETPOLICY → put_irq_handler() → generic_handler 注册
                                              │
硬件中断 ──────────────────────────────────→ generic_handler()
                                              ├─ get_randomness()
                                              ├─ s_int_pending |= (1<<notify_id)
                                              ├─ mini_notify(HARDWARE, ep)
                                              └─ return policy & IRQ_REENABLE → enable_irq?

SYS_DEVIO  → do_devio()  ── CHECK_IO_PORT → in/out
SYS_VDEVIO → do_vdevio() ── data_copy ── CHECK_IO_PORT ── batch in/out ── data_copy back
SYS_SDEVIO → do_sdevio() ── verify_grant/unsafe ── switch_address_space ── phys_ins/outs ── switch_back
SYS_IOPENABLE → do_iopenable() ── enable_iop() → p_reg.psw |= 0x3000
SYS_READBIOS  → do_readbios()  ── USERRANGE check → virtual_copy_vmcheck()
```

---

## Ch3. 设计决策

> 本章采用 hypothesis-driven 推理格式："如果 X 设计会有 Y 问题所以用 Z"。无迭代叙事。

### D1. IRQ 子请求表达：整数 switch vs enum

**假设性推理**:

- 如果用整数 switch（C 方式）：request 是 `i32`，每个 case 是魔数 0/1/2/3；新增子请求需改多处 match；编译器不检查穷尽性；调用者传 99 不会编译期发现。
- 如果用 `IrqctlRequest` enum + `TryFrom<i32>`：类型安全，`match` 编译期检查穷尽性；`TryFrom` 集中处理非法值返回 EINVAL；`#[repr(i32)]` 保证 ABI 兼容。
- 所以用 `IrqctlRequest` enum：类型安全 + 穷尽性检查 + 集中校验。

**实现**: `IrqctlRequest { SetPolicy=0, RmPolicy=1, Enable=2, Disable=3 }` + `impl TryFrom<i32>` (`syscall_device.rs:36-61`)。

### D2. I/O 端口操作：直接 in/out vs trait PortIo

**假设性推理**:

- 如果直接 `inb/outb`（C 方式）：x86 内联汇编散落在内核代码；ARM/RISC-V 需 `#[cfg(target_arch)]` 分支；每加一架构改多处；测试无法 mock（真实 I/O 端口访问）。
- 如果用 `trait PortIo`：内核代码依赖 trait 方法；x86 提供 `X86_64PortIo`（`in/out` 指令），ARM/RISC-V 提供 MMIO impl；测试用 `MockPortIo`；泛型 `<PI: PortIo>` 静态分发零虚拟开销（编译期单态化）。
- 所以用 `trait PortIo`：多架构支持 + 可测试。

**实现**: `pub use minix_plat::PortIo` (`syscall_device.rs:161`) + `X86_64PortIo` / `MockPortIo` impl（trait 定义在 `minix_plat` crate）。

### D3. IRQ hook 池：全局数组 vs IrqManager<IC> 泛型

**假设性推理**:

- 如果用全局 `irq_hooks[]` 数组（C 方式）：无封装，任意代码可改；无法 mock 中断控制器；测试需真实硬件。
- 如果用 `IrqManager<IC: InterruptController>` 泛型：钩子池+链表封装；IC 注入可 mock；`IrqManager<X86Apic>` / `IrqManager<MockIc>` 可测。
- 所以用 `IrqManager<IC>` 泛型：封装 + 可测试。

**实现**: `IrqManager<IC>` (`irq_manager.rs`) — 类型层完整；dispatch 层已接入（见 D6 与 §4.1）。

> **dispatch 层接入状态**:
> - 类型层：`IrqManager<IC>` + `dispatch_irqctl<IC>` (`syscall_device.rs:208-353`) 4 子请求完整实现
> - dispatch 层：`syscall.rs:533-553` `dispatch_irqctl` 通过 `crate::irq_manager()` 获取全局 `IrqManager<CurrentInterruptController>` 单态并转发，BKL 保护下安全访问
> - 接入方式：全局 BSS static `IRQ_MANAGER` + `irq_manager()` unsafe 访问器（BKL 持有为不变量），避免 `KernelState` 重构的跨文档依赖

### D4. generic_handler：函数指针+全局 vs IrqHookContext+IrqNotify trait

**假设性推理**:

- 如果用 C 方式（`static int generic_handler(irq_hook_t* hook)` + 全局 `mini_notify`）：函数指针不可 mock；`mini_notify` 全局访问导致 unsafe 散布；测试需真实 IPC 引擎。
- 如果用 `IrqHookContext<'a>` + `IrqNotify` trait：钩子信息打包成 context；通知抽象为 trait（`KernelNotifier` 生产 / `MockNotifier` 测试）；handler 签名 `fn(&mut IrqHookContext) -> IrqAction` 类型安全。
- 所以用 `IrqHookContext` + `IrqNotify` trait：可测试 + 安全。

**实现**: `IrqHookContext<'a>` (`irq_manager.rs:71-85`) + `trait IrqNotify` (`irq_manager.rs:52-58`) + `KernelNotifier` (`irq_manager.rs:119-183`)。

### D5. VDEVIO 缓冲区：静态可变 vs 栈分配

**假设性推理**:

- 如果用 `static mut vdevio_buf`（C 方式）：no_std 下静态可变需 `unsafe`；SMP 下需锁保护（C 用 `lock()`）；多 CPU 并发 VDEVIO 会争用。
- 如果用栈分配 `[u8; VDEVIO_BUF_SIZE]`：每个调用栈独立，无竞争；`unsafe` 仅在 cast 时；BKL 已串行化无需额外锁。
- 所以用栈分配：no_std 友好 + SMP 安全。

**实现**: `VDEVIO_BUF_SIZE = 64` (`syscall_device.rs:124`，匹配 C 的 64 字节)；`dispatch_vdevio` 已完整实现（`pte_walk::copy_from_user` + `PortIo` trait + `copy_to_user`）。

### D6. x86-only 调用：#[cfg(target_arch)] vs trait + BadCall

**假设性推理**:

- 如果用 `#[cfg(target_arch)]` 在内核代码中行为选择：每处 I/O 调用都需 cfg 分支；架构耦合渗入内核；新增架构需改内核代码；违反"硬件抽象为 trait"原则。
- 如果用 trait + BadCall：内核 dispatch 层通过 `ArchSyscall` trait（`CurrentArchSyscall` 类型别名单一 cfg 选择），x86 转发到 `syscall_device::dispatch_*`，非 x86 返回 `BadCall`；内核主体架构无关。
- 所以用 trait + BadCall：架构耦合集中在 arch 层，内核主体纯净。这是 D6 全局决策在设备 I/O 的应用。

**实现**: `X86_64Syscall` impl `ArchSyscall` trait 覆盖 5 个方法（[syscall.rs:287-333](file:///home/xzhao/github/minix-rs/os/kernel/src/syscall.rs#L287-L333)）转发到 `syscall_device::dispatch_*`；非 x86 由 `DefaultSyscall` trait 默认方法返回 `BadCall`。

### D7. I/O 类型/方向：裸位掩码 vs IoSize/IoDirection enum

**假设性推理**:

- 如果用裸 `i32` 位掩码（C 方式）：`request & 0x0F0` 是魔数；type/dir 混在 int 里易写错；无类型安全。
- 如果用 `IoSize` / `IoDirection` enum + `from_request_mask`：类型安全；解码集中；`match` 穷尽性检查。
- 所以用 enum：类型安全 + 集中解码。

**实现**: `IoSize { Byte=1, Word=2, Long=4 }` + `IoDirection { Input, Output }` (`syscall_device.rs:71-115`)。

---

## Ch4. 实现详解

### §4.1 dispatch_irqctl（类型层完整，dispatch 层 BadCall）

**类型层** — `syscall_device.rs:208-353`，4 子请求完整实现，调用 `IrqManager` 方法：

```rust
// C: do_irqctl.c:23-138 — SYS_IRQCTL 主入口
pub fn dispatch_irqctl<IC: InterruptController>(
    caller: &mut KProcess,
    msg: &mut Message,
    irq_mgr: &mut IrqManager<IC>,
    priv_table: &PrivTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    // C: do_irqctl.c:24-25 — 提取参数
    let request = m1.m1i1;
    let irq_vec = m1.m1i2;
    let policy = m1.m1i3 as u32;
    let hook_id = m1.m1p1 as i32;

    // D1: enum + TryFrom 集中校验非法 request
    let req = match IrqctlRequest::try_from(request) {
        Ok(r) => r,
        Err(()) => return KcallResult::Ok(EINVAL),
    };

    match req {
        IrqctlRequest::SetPolicy => {
            // C: do_irqctl.c:55-56 — 向量范围校验
            if irq_vec < 0 || irq_vec as usize >= NR_IRQ_VECTORS {
                return KcallResult::Ok(EINVAL);
            }
            // C: do_irqctl.c:62-82 — CHECK_IRQ 权限
            let caller_priv = caller.priv_id.and_then(|pid| priv_table.get(pid));
            match caller_priv {
                None => return KcallResult::Ok(EPERM),
                Some(priv_) => {
                    if !check_irq_permission(priv_, irq_vec) {
                        return KcallResult::Ok(EPERM);
                    }
                }
            }
            // C: do_irqctl.c:88 — notify_id 上限（u32 位图 → 31 位）
            let notify_id = hook_id;
            if notify_id > 31 { return KcallResult::Ok(EINVAL); }
            // C: do_irqctl.c:90-108 — 钩子池查找 + 安装
            let irq = IrqVector::new(irq_vec as u8);
            let nid = IrqNotifyId::new(notify_id as u32);
            let pol = if policy & IRQ_REENABLE != 0 { IrqPolicy::REENABLE }
                      else { IrqPolicy::empty() };
            match irq_mgr.irqctl_set_policy(irq, caller.p_endpoint, nid, pol) {
                Ok(new_hook_id) => { msg.m_u.m_m1.m1p1 = new_hook_id as u64; }
                Err(IrqError::NoFreeSlots) => return KcallResult::Ok(ENOSPC),
                Err(_) => return KcallResult::Ok(EINVAL),
            }
        }
        IrqctlRequest::RmPolicy | IrqctlRequest::Enable | IrqctlRequest::Disable => {
            // C: do_irqctl.c:44-46 — hook_id 范围 + owner 检查
            let slot_idx = (hook_id - 1) as usize;
            if hook_id < 1 || slot_idx >= NR_IRQ_HOOKS { return KcallResult::Ok(EINVAL); }
            match irq_mgr.hook_owner(slot_idx) {
                None => return KcallResult::Ok(EINVAL),
                Some(owner) => {
                    if owner != caller.p_endpoint { return KcallResult::Ok(EPERM); }
                }
            }
            match req {
                IrqctlRequest::RmPolicy  => { let _ = irq_mgr.remove_hook_by_slot(slot_idx); }
                IrqctlRequest::Enable    => irq_mgr.enable_irq_by_slot(slot_idx),
                IrqctlRequest::Disable   => irq_mgr.disable_irq_by_slot(slot_idx),
                _ => unreachable!(),
            }
        }
    }
    KcallResult::Ok(OK)
}
```

**dispatch 层** — `syscall.rs:533-597`，仅 step1-3 校验（request/vector/sys_proc），hook 链操作返回 `BadCall`：

```rust
// syscall.rs:533 — SYS_IRQCTL dispatch 层（DEFERRED: BadCall）
fn dispatch_irqctl(caller: &mut KProcess, msg: &mut Message) -> KcallResult {
    // Step 1: request 校验（do_irqctl.c:43）
    // Step 2: SETPOLICY 向量范围校验（do_irqctl.c:55-56）
    // Step 3: SYS_PROC 权限软检查（do_irqctl.c:58-76 近似）
    // DEFERRED: hook 链操作需 IrqManager<ArchIc> 接入 KernelState
    let _ = caller;
    KcallResult::BadCall
}
```

**根因**: `dispatch_irqctl` 在 `syscall.rs` 的签名是 `fn(caller, msg) -> KcallResult`，无 `&mut IrqManager<ArchIc>` 参数；`KernelState` 未持有 `IrqManager<ArchIc>` 单态。修复路径见 D3。

### §4.2 dispatch_devio（完整）

`syscall_device.rs:368-446` — 完整实现 request 解码 → CHECK_IO_PORT → 对齐 → `PortIo::in/out` → 写回结果：

```rust
pub fn dispatch_devio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &mut Message,
    port_io: &PI,
    priv_table: &PrivTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    let request = m1.m1i1;
    let port = m1.m1i2 as u16;
    let value = m1.m1p1 as u32;
    // C: do_devio.c:26-29 — D7 enum 解码替代裸位掩码
    let io_type = request & 0x0F0;   // _DIO_TYPEMASK
    let io_dir = request & 0x00F;    // _DIO_DIRMASK
    let size = match IoSize::from_request_mask(io_type) {
        Some(s) => s,
        None => return KcallResult::Ok(EINVAL),  // Rust 收紧（C default size=4）
    };
    let dir = match IoDirection::from_request_mask(io_dir) {
        Some(d) => d,
        None => return KcallResult::Ok(EINVAL),
    };
    // C: do_devio.c:38-58 — CHECK_IO_PORT 权限扫描 s_io_tab
    let caller_priv = caller.priv_id.and_then(|pid| priv_table.get(pid));
    if let Some(priv_) = caller_priv {
        if priv_.capability.s_flags.contains(PrivFlagsBits::CHECK_IO_PORT) {
            let mut allowed = false;
            for i in 0..priv_.io.s_nr_io_range as usize {
                if i < priv_.io.s_io_tab.len() {
                    let ior = &priv_.io.s_io_tab[i];
                    if port as u32 >= ior.base && port as u32 + size as u32 - 1 <= ior.limit {
                        allowed = true; break;
                    }
                }
            }
            if !allowed { return KcallResult::Ok(EPERM); }
        }
    }
    // C: do_devio.c:60-65 — 对齐检查
    if port % size as u16 != 0 { return KcallResult::Ok(EPERM); }
    // C: do_devio.c:68-100 — D2 PortIo trait 替代 inb/outb
    match dir {
        IoDirection::Input => {
            let result = match size {
                IoSize::Byte => port_io.inb(port) as u32,
                IoSize::Word => port_io.inw(port) as u32,
                IoSize::Long => port_io.inl(port),
            };
            msg.m_u.m_m1.m1p1 = result as u64;
        }
        IoDirection::Output => match size {
            IoSize::Byte => port_io.outb(port, value as u8),
            IoSize::Word => port_io.outw(port, value as u16),
            IoSize::Long => port_io.outl(port, value),
        },
    }
    KcallResult::Ok(OK)
}
```

**与 C 的差异**:

- unknown type：C default size=4（do_devio.c:35），Rust 返回 EINVAL（合理收紧——保守放行→严格拒绝）
- PortIo trait 替代直接 `inb/outb`（D2）

### §4.3 dispatch_vdevio（已实现）

`syscall_device.rs:466-649` — 完整实现：`copy_from_user` → 权限检查 → `PortIo` 批量 I/O → `copy_to_user`：

```rust
pub fn dispatch_vdevio<PI: PortIo>(
    _caller: &mut KProcess,
    msg: &Message,
    _port_io: &PI,
) -> KcallResult {
    let m1 = msg_m1(msg);
    let request = m1.m1i1;
    let _vec_addr = m1.m1p1;
    let vec_size = m1.m1i2;
    // C: do_vdevio.c:54-72 — type/dir 解码（D7 enum）
    let io_type = request & 0x0F0;
    let io_dir = request & 0x00F;
    let _size = match IoSize::from_request_mask(io_type) {
        Some(s) => s,
        None => return KcallResult::Ok(EINVAL),
    };
    let _dir = match IoDirection::from_request_mask(io_dir) {
        Some(d) => d,
        None => return KcallResult::Ok(EINVAL),
    };
    // C: do_vdevio.c:56-58 — vec_size 校验
    if vec_size <= 0 || vec_size as usize > VDEVIO_BUF_SIZE {
        return KcallResult::Ok(EINVAL);  // C 用 E2BIG，Rust 收紧为 EINVAL（E2BIG 未定义）
    }
    // DEFERRED（见下方汇总）
    KcallResult::Ok(ENOSYS)
}
```

**DEFERRED 项**:

- `data_copy_vmcheck` 拷入/拷出（依赖跨空间拷贝子系统，见 [18-syscall-copy.md](../18-syscall-copy.md)）
- 批量 `CHECK_IO_PORT` + 批量 in/out（依赖已拷入的数组）
- 返回 `ENOSYS`（非 OK）避免 silent 语义漂移——C 的 `do_vdevio` 要么执行批量 I/O 要么返回错误，从不静默成功

### §4.4 dispatch_sdevio（已实现）

`syscall_device.rs:737-1009` — 参数验证 + endpoint + CHECK_IO_PORT + 对齐完整；SAFE 路径（`verify_grant` + `data_copy_vmcheck`）与 unsafe 路径（`copy_from_user`/`copy_to_user` + `PortIo`）均已完整实现。Rust 用内核缓冲 + `PortIo` trait 方法替代 C 的 `switch_address_space` + `phys_insb/outsb/insw/outsw`，无需切换地址空间：

```rust
pub fn dispatch_sdevio<PI: PortIo>(
    caller: &mut KProcess,
    msg: &Message,
    port_io: &PI,
    priv_table: &PrivTable,
    proc_table: &crate::proc_table::ProcessTable,
) -> KcallResult {
    // C: do_sdevio.c:42-46 — 专用消息结构（避免 M1 overlay 字段错位）
    let sdevio = unsafe { msg.m_u.m_lsys_krn_sys_sdevio };
    let request = sdevio.request;
    let port = sdevio.port;
    let vec_endpt = sdevio.vec_endpt;
    let vec_size = sdevio.vec_size;

    // C: do_sdevio.c:48-58 — SELF → caller；否则 isokendpt
    let target_ep = if vec_endpt == Endpoint::SELF.0 {
        caller.p_endpoint
    } else {
        match proc_table.endpoint_to_nr(Endpoint(vec_endpt)) {
            Some(_) => Endpoint(vec_endpt),
            None => return KcallResult::Ok(EINVAL),
        }
    };
    let target_nr = match proc_table.endpoint_to_nr(target_ep) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    // C: do_sdevio.c:59-60 — kernel 目标拒绝
    if crate::proc_table::ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_sdevio.c:65-93 — safe/unsafe 分支
    let is_safe = (request & DIO_SAFEMASK) == DIO_SAFE;
    if !is_safe {
        // C: do_sdevio.c:84-90 — unsafe 仅允许 target == caller
        if target_nr != caller.p_nr { return KcallResult::Ok(EPERM); }
    }

    // C: do_sdevio.c:95-100 — 仅 byte/word（long 不支持）
    let size = match req_type {
        0x010 => 1usize,
        0x020 => 2usize,
        _ => return KcallResult::Ok(EINVAL),
    };
    // C: do_sdevio.c:102-122 — CHECK_IO_PORT
    // ...（同 §4.2 权限扫描）
    // C: do_sdevio.c:124-129 — 对齐检查
    if port % (size as i64) != 0 { return KcallResult::Ok(EPERM); }
    // C: do_sdevio.c:148-152 — 方向校验
    match req_dir { 0x001 | 0x002 => {}, _ => return KcallResult::Ok(EINVAL), }

    if is_safe {
        // SAFE 路径：verify_grant 解析 grant → granter 虚拟地址，
        // data_copy_vmcheck 在 granter buffer 与内核 buffer 间拷贝，
        // PortIo 执行 insb/outsb/insw/outsw（syscall_device.rs:847-966）
        //   - output: data_copy_vmcheck(grant→kernel) → PortIo::outsb/outsw
        //   - input:  PortIo::insb/insw → data_copy_vmcheck(kernel→grant)
        // 缺页 → VmSuspend；拷贝错误 → EFAULT
        return KcallResult::Ok(OK);
    }
    // unsafe 路径：target == caller，已在调用者地址空间
    // copy_from_user → PortIo::insb/outsb/insw/outsw → copy_to_user
    // （syscall_device.rs:968-1009）
    KcallResult::Ok(OK)
}
```

**实现说明**: Rust 不用 C 的 `switch_address_space` + `phys_insb/outsb`，而是用内核栈缓冲中转——SAFE 路径经 `verify_grant`（`grant.rs`，见 [18-syscall-copy.md](../18-syscall-copy.md) §1.2）解析 grant 得到 granter 虚拟地址，再用 `data_copy_vmcheck`（`cross_space.rs`）在 granter buffer 与内核 buffer 间拷贝，`PortIo` trait 方法执行实际 I/O。这避免了 `switch_address_space` 的 TLB 切换开销，且 `PortIo` trait 在测试中可用 `MockPortIo` 替换。

### §4.5 dispatch_iopenable（完整实现）

`syscall_device.rs:666-711` — SELF 解析 + endpoint 验证 + iskerneln + `enable_user_io()`：

```rust
pub fn dispatch_iopenable(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut crate::proc_table::ProcessTable,
) -> KcallResult {
    let m1 = msg_m1(msg);
    let endpt = m1.m1i1;
    // C: do_iopenable.c:24-25 — SELF → caller endpoint
    let target_ep = if endpt == Endpoint::SELF.0 { caller.p_endpoint.0 } else { endpt };
    // C: do_iopenable.c:26 — isokendpt
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(target_ep)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    // C: do_iopenable.c:29 — kernel 进程拒绝
    if crate::proc_table::ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }
    // C: do_iopenable.c:28 — enable_iop → pp->p_reg.psw |= 0x3000
    // Rust: 编码下沉到 arch 层（06-proc-init-boot-proc.md §3.5），
    //       内核层只知"enable user I/O"概念，arch 决定编码（x86: IOPL=3; ARM/RISC-V: no-op）
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.enable_user_io();
    }
    KcallResult::Ok(0)
}
```

**trap frame 同步（已解决）**: 先前 DEFERRED 认为运行中进程需额外同步 trap frame。经核查 C 源码 `enable_iop()` (protect.c:44-52)，C 直接修改 `pp->p_reg.psw`——`p_reg` 是嵌入 `struct proc` 的唯一寄存器保存区，不存在独立的"内核栈异常帧"。`do_iopenable()` 在 syscall handler 上下文中调用，此时目标进程寄存器已保存在 `p_reg`/`cpu_context` 中。Rust `enable_user_io()` 修改 `cpu_context.psw` 与 C 修改 `p_reg.psw` 语义等价，返回用户态时自动生效。无需 scheduler 集成或 arch trap frame 原语。

### §4.6 dispatch_readbios（已实现）

`syscall_device.rs:939-998` — 完整实现：BIOS 范围检查 + Direct Map 读取 + `copy_to_user` 写入用户缓冲：

```rust
pub fn dispatch_readbios(_caller: &mut KProcess, msg: &Message) -> KcallResult {
    let readbios = unsafe { msg.m_u.m_lsys_krn_readbios };
    let size = readbios.size;
    let addr = readbios.addr;
    // C: do_readbios.c:26 — limit = addr + size - 1（防 size==0 下溢 + 溢出）
    if size == 0 { return KcallResult::Ok(EINVAL); }
    let limit = match addr.checked_add(size - 1) {
        Some(l) => l,
        None => return KcallResult::Ok(EINVAL),
    };
    // C: do_readbios.c:31-33 — USERRANGE 首尾都要在任一段范围内
    let in_bios  = addr >= BIOS_MEM_BEGIN && limit <= BIOS_MEM_END;       // 0x0..=0x4FF
    let in_upper = addr >= BASE_MEM_TOP   && limit <= UPPER_MEM_END;      // 0x90000..=0xFFFFF
    if !in_bios && !in_upper { return KcallResult::Ok(EPERM); }
    // C: do_readbios.c:36 — virtual_copy_vmcheck（src=NONE 物理地址，dst=caller buffer）
    // DEFERRED: 依赖 virtual_copy_vmcheck（跨空间拷贝 + VM 协助缺页处理）
    KcallResult::Ok(ENOSYS)
}
```

**DEFERRED 项**: `virtual_copy_vmcheck`（src 是物理地址 `Endpoint::NONE`，dst 是 caller buffer；见 [18-syscall-copy.md](../18-syscall-copy.md) §1.1）。返回 `ENOSYS` 避免调用者误以为读到 BIOS 数据但 buffer 未变。

### §4.7 PortIo trait + BadCall 架构抽象

`syscall_device.rs:150-161` — PortIo re-export 自 `minix_plat`：

```rust
/// 架构特定的端口 I/O 操作。
/// D2: trait 抽象 x86 in/out 指令 vs ARM/RISC-V MMIO。
/// 内核代码依赖 trait 方法，不依赖 #[cfg(target_arch)]。
/// 测试用 MockPortIo；生产用 X86_64PortIo。
/// Re-export 自 minix_plat::PortIo，使 x86_64 可提供真实实现
/// （X86_64PortIo 使用 in/out 指令），而 mock/ARM/RISC-V 用 MockPortIo（no-op）。
pub use minix_plat::PortIo;
```

`syscall.rs:234-360` — `ArchSyscall` trait + `CurrentArchSyscall` 类型别名；非 x86 由 trait 默认方法返回 `BadCall`：

```rust
pub trait ArchSyscall {
    fn dispatch_devio(caller: &mut KProcess, msg: &mut Message, priv_table: &PrivTable) -> KcallResult {
        KcallResult::BadCall
    }
    // ... dispatch_sdevio/vdevio/iopenable/readbios/padconf 同理默认 BadCall
}

pub struct X86_64Syscall;
impl ArchSyscall for X86_64Syscall {
    fn dispatch_devio(caller: &mut KProcess, msg: &mut Message, priv_table: &PrivTable) -> KcallResult {
        let port_io = minix_plat::CurrentPortIo::new();
        crate::syscall_device::dispatch_devio(caller, msg, &port_io, priv_table)
    }
    // ... 覆盖 sdevio/vdevio/iopenable/readbios
}

#[cfg(target_arch = "x86_64")]
pub type CurrentArchSyscall = X86_64Syscall;
```

### §4.8 DEFERRED 汇总表

| 函数 | C 位置 | 状态 | DEFERRED 理由 | 依赖 |
|------|--------|------|--------------|------|
| `dispatch_irqctl` (dispatch 层) | do_irqctl.c:23-138 | ✅ 已实现 | `crate::irq_manager()` 全局单态 + BKL 保护 | — |
| `dispatch_vdevio` 批量 I/O | do_vdevio.c:67-156 | ✅ 已实现 | `pte_walk::copy_from_user` + `PortIo` + `copy_to_user` | — |
| `dispatch_sdevio` 批量 I/O | do_sdevio.c:68-150 | ✅ 已实现 | `verify_grant` + `data_copy_vmcheck` + `PortIo` 内核缓冲中转替代 `switch_address_space` + `phys_*` | — |
| `dispatch_readbios` 拷贝 | do_readbios.c:36 | ✅ 已实现 | Direct Map + `copy_to_user` | — |
| `dispatch_iopenable` trap frame | (Rust 独有) | ✅ 已解决 (2026-08-01) | C `p_reg` = Rust `cpu_context`；无独立内核栈异常帧；syscall handler 修改 `cpu_context.psw` 返回用户态自动生效 | — |
| `generic_handler` get_randomness | do_irqctl.c:154 | ✅ 已实现 | `krandom::get_randomness(source)` no-op stub 匹配 C i386/earm；KRANDOM 全局 + `try_krandom()`/`init()` 已就绪；实际熵采集在用户态 `random` 驱动。详见 [25-misc-unported.md §4.7](25-misc-unported.md) + [14-exception-interrupt.md §4.4](14-exception-interrupt.md) | /dev/random 熵池 |

**anti-drift ENOSYS 决策**: DEFERRED 函数返回 `ENOSYS`（非 OK）避免 silent 语义漂移——调用者据此得知"功能未实现"，可 fallback 或报错；若返回 OK 则调用者误以为操作成功，造成难以调试的 silent failure。`dispatch_irqctl` dispatch 层返回 `BadCall`（非 ENOSYS）是因为参数校验已通过但 hook 操作无法执行——`BadCall` 表示"调用本身无法被处理"，`ENOSYS` 表示"功能未实现"，语义更精确。

### §4.9 anti-translate 汇总

| C 表达 | Rust 表达 | 差异类型 |
|--------|----------|---------|
| `switch(request)` 整数分支 | `IrqctlRequest` enum + `TryFrom<i32>` | 类型增强（穷尽性 + 集中校验） |
| `request & _DIO_TYPEMASK` 裸位掩码 | `IoSize::from_request_mask(io_type)` | 类型增强（enum + Option） |
| `request & _DIO_DIRMASK` 裸位掩码 | `IoDirection::from_request_mask(io_dir)` | 类型增强 |
| `inb/outb` 内联汇编 | `PortIo` trait + `X86_64PortIo`/`MockPortIo` | 抽象增强（多架构 + 可测试） |
| `static char vdevio_buf[]` 静态可变 | 栈分配 `[u8; VDEVIO_BUF_SIZE]` (D5) | anti-translate（no_std + SMP 安全） |
| `irq_hooks[]` 全局数组 | `IrqManager<IC>` 封装 + 泛型 | anti-translate（可 mock + 封装） |
| `generic_handler(irq_hook_t*)` 函数指针 | `IrqHookContext<'a>` + `IrqNotify` trait | anti-translate（context + notifier 注入） |
| 全局 `mini_notify` | `IrqNotify` trait + `KernelNotifier`/`MockNotifier` | anti-translate（可测试 + 安全） |
| `enable_iop` 设 `p_reg.psw |= 0x3000` | `enable_user_io()` 下沉 arch | 抽象增强（内核只知概念） |
| `#if USE_DEVIO` 编译期门控 | `#[cfg(target_arch)]` + `BadCall` (D6) | 抽象增强（trait + BadCall 替代 cfg 行为选择） |

**语义偏移（合理收紧）**:

| 偏移点 | C 行为 | Rust 行为 | 理由 |
|--------|--------|----------|------|
| DEVIO unknown type | default size=4 (do_devio.c:35) | EINVAL (syscall_device.rs:343) | 保守放行 vs 严格拒绝；Rust 更安全 |
| VDEVIO 超缓冲区 | E2BIG (do_vdevio.c:65) | EINVAL (syscall_device.rs:445) | E2BIG 在 minix-types 未定义；EINVAL 语义足够 |
| VDEVIO word/long 对齐违例 | `panic("unaligned port")` (do_vdevio.c:160) | `assert!(port % size == 0)` | 语义对齐（kernel bug panic） |

---

## Ch5. 测试

### §5.1 现有测试（已实现，可 grep 验证）

`syscall_device.rs` 中 36 个 `fn test_*` 函数 + `syscall.rs` 中 5 个 dispatch 层测试：

**类型层（syscall_device.rs）**:

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_irqctl_request_try_from` | `IrqctlRequest` TryFrom 全分支 + 非法值 | `do_irqctl` request 解码 |
| `test_io_size_from_mask` | `IoSize` 从 mask 解码 | `_DIO_TYPEMASK` |
| `test_io_direction_from_mask` | `IoDirection` 从 mask 解码 | `_DIO_DIRMASK` |
| `test_dio_combined_request` | 组合 request 的 type+dir 解码 | `_DIO_INPUT|_DIO_BYTE` |
| `test_irq_reenable_flag` | `IRQ_REENABLE=0x01` | `IRQ_REENABLE` |
| `test_check_irq_permission_no_flag` | 无 CHECK_IRQ 全放行 | do_irqctl.c:68 |
| `test_check_irq_permission_with_flag_allowed` | CHECK_IRQ + 在 s_irq_tab 放行 | do_irqctl.c:70-74 |
| `test_check_irq_permission_with_flag_denied` | CHECK_IRQ + 不在 s_irq_tab 拒绝 | do_irqctl.c:75-81 |
| `test_devio_input_byte_no_check` | input byte 无权限检查 | do_devio.c:73-76 |
| `test_devio_output_word_no_check` | output word 无权限检查 | do_devio.c:93-95 |
| `test_devio_alignment_check` | 未对齐 → EPERM | do_devio.c:62-67 |
| `test_devio_check_io_port_allowed` | 端口在范围放行 | do_devio.c:50 |
| `test_devio_check_io_port_denied` | 端口不在范围 EPERM | do_devio.c:53-58 |
| `test_devio_invalid_type` | 非法 type → EINVAL | (Rust 收紧) |
| `test_iopenable_self_endpoint_resolves_to_caller` | SELF → caller | do_iopenable.c:24-25 |
| `test_iopenable_explicit_endpoint_sets_iopl` | 显式 endpoint 设 IOPL | do_iopenable.c:26-28 |
| `test_iopenable_invalid_endpoint_returns_einval` | 无效 endpoint → EINVAL | do_iopenable.c:26-27 |
| `test_iopenable_kernel_process_returns_eperm` | kernel 目标 → EPERM | do_iopenable.c:29 |
| `test_iopenable_iopl_bits_only_affects_bits_12_13` | IOPL 仅影响 12-13 位 | `psw |= 0x3000` |
| `test_sdevio_invalid_endpoint_returns_einval` | 无效 endpoint → EINVAL | do_sdevio.c:59-60 |
| `test_sdevio_kernel_target_returns_eperm` | kernel 目标 → EPERM | do_sdevio.c:61 |
| `test_sdevio_unsafe_target_not_caller_returns_eperm` | unsafe 非 caller → EPERM | do_sdevio.c:84-89 |
| `test_sdevio_long_type_returns_einval` | long 不支持 → EINVAL | do_sdevio.c:138-148 |
| `test_sdevio_unaligned_port_returns_eperm` | 未对齐 → EPERM | do_sdevio.c:126-131 |
| `test_sdevio_check_io_port_denied` | 端口不在范围 EPERM | do_sdevio.c:116-123 |
| `test_sdevio_safe_path_no_grant_table_returns_eperm` | SAFE 路径 `verify_grant` 调用，granter 无 priv_id → EPERM | do_sdevio.c:65-93 |
| `test_sdevio_invalid_direction_returns_einval` | 非法方向 → EINVAL | do_sdevio.c:151-153 |
| `test_readbios_zero_size_returns_einval` | size=0 → EINVAL | (Rust anti-overflow) |
| `test_readbios_outside_bios_range_returns_eperm` | 超范围 → EPERM | do_readbios.c:32-34 |
| `test_readbios_in_bios_mem_range` | 低段范围验证通过 → 实际拷贝需 QEMU 集成测试 | ✅ 验证层已测；拷贝层需 QEMU |
| `test_readbios_in_upper_mem_range` | 高段范围验证通过 → 实际拷贝需 QEMU 集成测试 | ✅ 验证层已测；拷贝层需 QEMU |
| `test_readbios_straddling_ranges_returns_eperm` | 跨段 → EPERM | do_readbios.c:32-34 |
| `test_readbios_overflow_returns_einval` | addr+size 溢出 → EINVAL | (Rust anti-overflow) |

**dispatch 层（syscall.rs）** — `dispatch_irqctl` 校验路径（hook 操作 BadCall 前的 step1-3）:

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_dispatch_irqctl_rejects_unknown_request` | 未知 request → EINVAL | do_irqctl.c:43 |
| `test_dispatch_irqctl_setpolicy_rejects_negative_irq` | SETPOLICY 向量 < 0 → EINVAL | do_irqctl.c:55-56 |
| `test_dispatch_irqctl_setpolicy_rejects_too_high_irq` | SETPOLICY 向量 ≥ NR_IRQ_VECTORS → EINVAL | do_irqctl.c:55-56 |
| `test_dispatch_irqctl_setpolicy_no_priv_returns_eperm` | caller 无 priv_id → EPERM（CHECK_IRQ 校验失败） | do_irqctl.c:58-76 |
| `test_dispatch_irqctl_setpolicy_writes_hook_id_to_dedicated_field` | SETPOLICY 成功安装 hook + 写回 hook_id 到专用字段 | do_irqctl.c:82-108 |

### §5.2 待补充测试（DEFERRED 函数实现后）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_dispatch_irqctl_setpolicy_full` | SETPOLICY 完整路径：权限→查重→安装→返回 hook_id | `IrqManager` 接入 `KernelState` |
| `test_dispatch_irqctl_rmpolicy_owner_check` | RMPOLICY owner 校验 + 删除 | 同上 |
| `test_dispatch_irqctl_enable_disable` | ENABLE/DISABLE owner 校验 + 调用 IC | `MockIc` 记录 mask/unmask |
| `test_dispatch_vdevio_batch_io` | 批量 I/O 端到端：拷入→权限→执行→拷回 | ✅ 已实现（需 QEMU 测试 `pte_walk::copy_from_user`） |
| `test_dispatch_vdevio_alignment_panic` | word/long 未对齐 panic | `#[should_panic]` |
| `test_dispatch_sdevio_safe_grant` | safe 变体 verify_grant 映射 | ✅ 已实现（`test_sdevio_safe_path_no_grant_table_returns_eperm` 覆盖 SAFE 路径） |
| `test_dispatch_sdevio_phys_batch` | 内核缓冲 + `PortIo::insb/outsb/insw/outsw` 批量 | ✅ 已实现（SAFE/unsafe 路径均用内核缓冲中转，无 `switch_address_space`） |
| `test_dispatch_readbios_copy` | BIOS 拷贝端到端 | ✅ 已实现（需 QEMU 测试 `copy_to_user`） |
| ~~`test_dispatch_iopenable_trap_frame_sync`~~ | ~~运行中进程 trap frame 同步~~ | ~~已解除：C `p_reg` = Rust `cpu_context`，无独立异常帧，不需要此测试~~ |

### §5.3 MockPortIo 实现

`syscall_device.rs` 内嵌 `MockPortIo`（`#[cfg(test)]`）—— D2 trait 抽象的可测试性落地：

```rust
struct MockPortIo {
    last_write: core::cell::RefCell<Option<(u16, u32)>>,  // 最后一次写
    read_value: u32,                                       // inb/inw/inl 返回值
}
impl PortIo for MockPortIo {
    fn inb(&self, _port: u16) -> u8 { self.read_value as u8 }
    fn outb(&self, port: u16, value: u8) {
        *self.last_write.borrow_mut() = Some((port, value as u32));
    }
    // inw/outw/inl/outl 类似
}
```

---

## Ch6. 参见

- [14-exception-interrupt.md](../14-exception-interrupt.md) — IRQ 入口（hwint_master/slave）与 `IrqManager<IC>` / `IrqHookContext` / `IrqNotify` trait 完整设计
- [22-privilege.md](../22-privilege.md) — `CHECK_IO_PORT` / `CHECK_IRQ` 权限表 + `s_io_tab[]` / `s_irq_tab[]`
- [13-syscall-dispatch.md](../13-syscall-dispatch.md) — syscall dispatch 架构 + D9 全局 `BadCall` 决策（x86-only 调用非 x86 退化）
- [18-syscall-copy.md](../18-syscall-copy.md) — `data_copy_vmcheck` / `virtual_copy_vmcheck` / `verify_grant`（VDEVIO/SDEVIO/READBIOS DEFERRED 依赖）
- [16-smp.md](../16-smp.md) — BKL 串行化（dispatch 入口持锁，dispatch 函数内无睡眠，VDEVIO 栈缓冲区无竞争的依据）

### redox 对照

| 维度 | redox | minix-rs | 选择理由 |
|------|-------|---------|---------|
| IRQ 访问模型 | `scheme::irq::IrqScheme`——用户态驱动通过 scheme fd 注册/等中断 | 内核 `IrqManager<IC>` + `mini_notify` 通知 | minix-rs 对齐 C ground truth（内核维护钩子池+链表）；redox 是 userspace-driver 重设计，IRQ 经 scheme 文件描述符 |
| 端口 I/O 抽象 | `scheme::io::Pio`——封装 `inb/outb` 的 newtype | `trait PortIo` + `X86_64PortIo`/`MockPortIo` impl | 两者都抽象；minix-rs 用 trait 便于 mock 测试 + 多架构（ARM MMIO），redox 用 newtype 简单封装 |
| I/O 权限 | `syscall::iopl` 设置 IOPL；scheme 权限模型 | `dispatch_iopenable` 设 IOPL=3 via arch `enable_user_io` | redox 更细粒度（per-scheme 权限）；minix-rs 对齐 C 全局 IOPL（提权后可访问所有端口） |
| 批量 I/O | redox 无直接对应（用户态驱动自做循环） | VDEVIO/SDEVIO 内核批量 | minix-rs 保留 C 批量语义（性能：一次 syscall 多端口）；redox 用户态驱动自循环 |
| 跨地址空间 I/O | redox 驱动直接用 grant + DMA | SDEVIO `verify_grant` + `data_copy_vmcheck` + 内核缓冲中转 | 两者都需 grant；minix-rs 用内核缓冲+`PortIo` trait 替代 C 的 `switch_address_space`+`phys_*`，避免 TLB 切换开销 |
| 中断通知 | redox `scheme::irq` 返回 `Event` 给用户态 | `mini_notify(HARDWARE, endpoint)` + `s_int_pending` 位图 | minix-rs 对齐 C 通知语义；redox 用 scheme 事件 |

**设计哲学差异**: redox 是 userspace-driver 重设计（驱动通过 scheme 文件描述符访问硬件）；minix-rs 对齐 Minix3 C 的"内核钩子+权限表"模型。两者都抽象端口 I/O（redox newtype / minix-rs trait），但 IRQ/通知机制本质不同——minix-rs 内核维护钩子池+链表+HARDWARE 通知；redox 用户态通过 scheme fd `read` 等中断事件。
