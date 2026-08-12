# 20-syscall-device-outline.md — 文档结构契约

> **文档**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/20-syscall-device.md`
> **C 源码**: `minix3/minix/kernel/system/do_irqctl.c` (174 行), `do_devio.c` (107 行), `do_vdevio.c` (165 行); `minix3/minix/kernel/arch/i386/do_sdevio.c` (162 行), `do_iopenable.c` (34 行), `do_readbios.c` (37 行)
> **Rust 实现**: `os/kernel/src/syscall_device.rs` (1464 行), `os/kernel/src/irq_manager.rs`, `os/kernel/src/syscall.rs`
> **创建**: 2026-08-01
> **依据**: `20-syscall-device-glm-structure.md`（知识点全集 + 诊断）
> **方法**: C 源码 → OS 理论 → Rust 对照（非反向）

---

## 一、章节骨架与主语

### Ch1 主语：硬件 + 安全（"内核如何让用户态驱动安全访问硬件？"）

核心问题：**内核如何让用户态驱动安全访问硬件？**

设备 I/O 系统调用回答两个正交问题：(1) 驱动如何收到硬件中断？(2) 驱动如何读写 I/O 端口？内核的回答是"钩子 + 权限"——用 IRQ 钩子把中断翻译成通知，用 `CHECK_IO_PORT`/`CHECK_IRQ` 权限表把端口/向量限制在驱动被授权的范围内，x86-only 特性（IOPL/BIOS）用 trait + BadCall 在非 x86 上退化。

| 节 | 标题 | 灵魂本质（一句话） | 概念组 |
|----|------|-------------------|--------|
| §1.1 | IRQ 控制：中断如何变成通知 | "IRQ 钩子是驱动-中断解耦机制——驱动注册钩子，内核把硬件中断翻译成 HARDWARE 通知唤醒驱动" | A |
| §1.2 | 端口 I/O：DEVIO/VDEVIO/SDEVIO 的语义分层 | "三种端口 I/O 是单次/批量/跨进程批量三个抽象层，共享 type/direction 解码与 CHECK_IO_PORT 权限" | B, C, D |
| §1.3 | x86-only 特性：IOPENABLE/READBIOS 的架构范围 | "IOPL 提权与 BIOS 读取是 x86 历史包袱，内核只承担'概念'，编码下沉到 arch 层" | E |
| §1.4 | 架构抽象：PortIo trait + BadCall 替代 #[cfg] | "x86 端口 I/O 与 ARM/RISC-V MMIO 共享 trait 接口，非 x86 调用返回 BadCall 而非分散条件编译" | F |

### Ch2 主语：C 源码符号（file:line 锚定）

每节以 C 符号为单元，附 file:line，说明语义与调用关系。覆盖全部 6 个 do_* 函数 + generic_handler。

### Ch3 主语：设计决策（hypothesis-driven）

采用"如果 X 设计会有 Y 问题所以用 Z"格式，禁止"旧版/最初/后来/我们改成"迭代叙事。

### Ch4 主语：Rust 实现（真实代码，非 stub）

贴 syscall_device.rs / irq_manager.rs 真实代码片段，标注 file:line。DEFERRED 函数诚实标注 + 理由。

### Ch5 主语：测试函数（可 grep 验证）

列出实际 `fn test_*` 函数名，每个测试对应一个被测行为。

---

## 二、详细大纲

### Ch1. 概念建构（concept-driven）

#### §1.1 IRQ 控制：中断如何变成通知

**灵魂本质**: IRQ 钩子是驱动-中断解耦机制——驱动注册钩子，内核把硬件中断翻译成 HARDWARE 通知唤醒驱动。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 中断上下文不可阻塞、不可执行用户态代码；但驱动逻辑必须在用户态运行。需要一个"翻译层"把硬件中断转为可被驱动 RECEIVE 的消息。
- **WHAT**: IRQ 钩子（hook）是 (endpoint, notify_id, policy) 三元组。驱动通过 `SYS_IRQCTL` 注册钩子；硬件中断触发时，内核 `generic_handler` 设置 `s_int_pending` 位 + 发 `mini_notify(HARDWARE, endpoint)`，驱动下次 RECEIVE 即拿到中断事件。
- **HOW**: C 用 `do_irqctl()` (do_irqctl.c:23-138) 的 4 子请求管理钩子生命周期：SETPOLICY 注册、RMPOLICY 删除、ENABLE/DISABLE 动态控线。`generic_handler()` (do_irqctl.c:143-172) 在中断上下文执行"随机数→位图→通知"三步。

**4 个子请求的语义**:

| 子请求 | 语义 | C 常量 |
|--------|------|--------|
| SETPOLICY | 注册钩子（权限→查重→空闲槽→安装→返回 1-based hook_id） | `IRQ_SETPOLICY` |
| RMPOLICY | 删除钩子（验主→rm→清槽） | `IRQ_RMPOLICY` |
| ENABLE | 启用中断线（验主→enable_irq） | `IRQ_ENABLE` |
| DISABLE | 禁用中断线（验主→disable_irq） | `IRQ_DISABLE` |

**generic_handler 的三步副作用**:
1. `get_randomness(&krandom, hook->irq)` — 采集随机数（/dev/random）
2. `priv(proc)->s_int_pending |= (1 << hook->notify_id)` — 位图记账
3. `mini_notify(HARDWARE, hook->proc_nr_e)` — 唤醒驱动
4. 返回 `policy & IRQ_REENABLE` 控制是否重启用中断线

**关键约束**:
1. notify_id ≤ 31（`s_int_pending` 是 u32 位图）
2. 只有钩子 owner 能 RMPOLICY/ENABLE/DISABLE（do_irqctl.c:46,126）
3. 进程退出必须摘钩（不变式；违例 panic — do_irqctl.c:160-161）

#### §1.2 端口 I/O：DEVIO/VDEVIO/SDEVIO 的语义分层

**灵魂本质**: 三种端口 I/O 是单次/批量/跨进程批量三个抽象层，共享 type/direction 解码与 CHECK_IO_PORT 权限。

**三层抽象**:

| 系统调用 | 抽象层 | 语义 | C 函数 |
|---------|--------|------|--------|
| SYS_DEVIO | 单次 | 一次读/写一个端口 | `do_devio()` |
| SYS_VDEVIO | 批量 | 一次处理 N 个 (port,value) 对（buffer 在调用者空间） | `do_vdevio()` |
| SYS_SDEVIO | 跨进程批量 | 一次处理 N 个元素（buffer 在另一进程空间，可走 grant） | `do_sdevio()` |

**共享的解码 + 权限**:
- request 解码：`_DIO_TYPEMASK`(0x0F0) 提取 type（byte/word/long），`_DIO_DIRMASK`(0x00F) 提取 dir（input/output）
- CHECK_IO_PORT：扫描 `s_io_tab[]`，要求 `port >= base && port+size-1 <= limit`
- 对齐检查：`port & (size-1)` 必须为 0（word/long 自然对齐）

**VDEVIO 的批量语义**:
1. `data_copy` 从用户拷入 (port,value) 向量到内核 `vdevio_buf`
2. 批量 CHECK_IO_PORT（逐元素扫 `s_io_tab`）
3. 批量 in/out（byte 无对齐；word/long 内联对齐检查，违例 panic）
4. input 模式 `data_copy` 拷回结果

**SDEVIO 的跨进程语义**:
1. SELF/endpoint 验证 + 拒绝 kernel 目标
2. `_DIO_SAFE` 区分 safe（verify_grant 映射 grant→物理地址）与 unsafe（要求 target==caller）
3. `switch_address_space(destproc)` 切到目标空间
4. 仅支持 byte/word（long 不支持）；`phys_insb/outsb/insw/outsw` 批量原语
5. `switch_address_space(caller)` 切回

#### §1.3 x86-only 特性：IOPENABLE/READBIOS 的架构范围

**灵魂本质**: IOPL 提权与 BIOS 读取是 x86 历史包袱，内核只承担"概念"，编码下沉到 arch 层。

**IOPENABLE**:
- 语义：给用户态进程 IOPL=3 权限，允许其执行 `in/out` 指令访问**所有**端口（绕过 CHECK_IO_PORT）
- C 实现：`do_iopenable()` (do_iopenable.c:19-29) → `enable_iop()` 设 `p_reg.psw |= 0x3000`
- x86 范围：IOPL 是 x86 RFLAGS 第 12-13 位；ARM/RISC-V 无对应概念
- 内核层抽象：只知"enable user I/O"概念，arch 层决定编码（x86: IOPL=3；其他: no-op）

**READBIOS**:
- 语义：从 BIOS 内存区拷贝数据到用户 buffer（用户态不可直接读物理 BIOS 区）
- C 实现：`do_readbios()` (do_readbios.c:15-37) → `virtual_copy_vmcheck`（src=NONE 物理地址）
- BIOS 内存范围两段：`0x0..=0x4FF`（IVT+BIOS data）和 `0x90000..=0xFFFFF`（upper memory）
- USERRANGE 检查：首尾都要在任一段范围内

**架构范围标注**: IOPENABLE/READBIOS/DEVIO/VDEVIO/SDEVIO 都是 x86-only（I/O 端口是 x86 概念）。其他架构用 MMIO，不需要这些调用。

#### §1.4 架构抽象：PortIo trait + BadCall 替代 #[cfg]

**灵魂本质**: x86 端口 I/O 与 ARM/RISC-V MMIO 共享 trait 接口，非 x86 调用返回 BadCall 而非分散条件编译。

**跨架构差异表**:

| CPU 问题 | x86_64 | aarch64 | riscv64 | Rust 抽象 |
|---------|--------|---------|---------|-----------|
| 端口读 | `inb/inw/inl` 指令 | MMIO load | MMIO load | `PortIo::read_byte/word/long` |
| 端口写 | `outb/outw/outl` 指令 | MMIO store | MMIO store | `PortIo::write_byte/word/long` |
| 中断控制器 | PIC/APIC | GIC | PLIC | `InterruptController` trait |
| 用户态 I/O 提权 | RFLAGS.IOPL=3 | no-op | no-op | `CpuContextArch::enable_user_io` |
| BIOS 数据 | 物理内存 0x0-0xFFFFF | 无 | 无 | (x86-only，BadCall on others) |

**设计原则**: 内核代码（syscall_device.rs）只依赖 trait 方法，不出现 `#[cfg(target_arch)]` 行为选择。各架构在 arch 层提供 trait 实现。x86-only 调用在非 x86 上由 dispatch 层 `dispatch_arch_*` 返回 `KcallResult::BadCall`（D9 全局决策）。

---

### Ch2. C 源码分析（file:line 锚定）

#### §2.1 do_irqctl.c — IRQ 控制

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_irqctl()` | do_irqctl.c:23-138 | switch(request): SETPOLICY(57-120)/RMPOLICY(122-132)/ENABLE(42-52)/DISABLE(42-52) |
| SETPOLICY 权限检查 | do_irqctl.c:62-82 | `CHECK_IRQ` + 扫描 `s_irq_tab[]` |
| SETPOLICY notify_id 上限 | do_irqctl.c:88 | `> CHAR_BIT*sizeof(irq_id_t)-1` → EINVAL |
| SETPOLICY 钩子池查找 | do_irqctl.c:90-108 | 先覆盖同 (ep, nid)，再找空闲；满→ENOSPC |
| SETPOLICY 安装 | do_irqctl.c:110-120 | `put_irq_handler(hook, vec, generic_handler)`；返回 `hook_id+1` |
| ENABLE/DISABLE 校验 | do_irqctl.c:44-46 | hook_id 范围 + `proc_nr_e != NONE` + owner 检查 |
| `generic_handler()` | do_irqctl.c:143-172 | get_randomness(154)→isokendpt(160)→s_int_pending(167)→mini_notify(170)→return policy&IRQ_REENABLE(171) |

#### §2.2 do_devio.c — 单次端口 I/O

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_devio()` | do_devio.c:19-106 | 解析 type/dir(27-36)→CHECK_IO_PORT(38-59)→对齐(61-67)→in/out(69-103) |
| type 解码 | do_devio.c:30-36 | `_DIO_BYTE=1, _DIO_WORD=2, _DIO_LONG=4`；default size=4 |
| CHECK_IO_PORT | do_devio.c:44-58 | 扫 `s_io_tab[]`：`port>=base && port+size-1<=limit` |
| 对齐检查 | do_devio.c:62-67 | `port & (size-1)` → EPERM |
| input 写回 | do_devio.c:70-86 | 结果写 `m_krn_lsys_sys_devio.value` |
| 无 priv → goto doit | do_devio.c:39-43,61 | 内核进程跳过检查 |

#### §2.3 do_vdevio.c — 批量端口 I/O

| 符号 | 位置 | 说明 |
|------|------|------|
| `vdevio_buf` 静态缓冲区 | do_vdevio.c:17-20 | `char[1024]`；pvb/pvw/pvl 三种 pair cast 复用 |
| `do_vdevio()` | do_vdevio.c:25-164 | 解析(44-64)→size 校验(65)→拷入(67-70)→批量权限(72-100)→批量 I/O(102-149)→拷回(151-156) |
| vec_size 校验 | do_vdevio.c:49,65 | `<=0`→EINVAL；`bytes>buf`→E2BIG |
| 批量 CHECK_IO_PORT | do_vdevio.c:72-100 | 逐元素扫 `s_io_tab`；失败→EPERM |
| word/long 对齐 panic | do_vdevio.c:116,125,135,145,159-161 | `port&1`/`port&3` → `panic("unaligned port")` |
| 拷回结果 | do_vdevio.c:151-156 | input 模式 `data_copy` 回用户 |

#### §2.4 do_sdevio.c — 跨进程批量端口 I/O

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_sdevio()` | do_sdevio.c:24-161 | endpoint 验证(56-61)→safe/unsafe 分支(67-93)→switch_space(96)→size(98-104)→权限(106-124)→对齐(126-131)→phys_*(133-150)→switch_back(159) |
| SELF/isokendpt/iskerneln | do_sdevio.c:56-61 | SELF→caller；kernel 拒绝 |
| `_DIO_SAFE` 分支 | do_sdevio.c:40,68-82 | safe: `verify_grant` 映射 grant→物理地址 |
| unsafe 分支 | do_sdevio.c:83-93 | 要求 `proc_nr == _ENDPOINT_P(caller)`；否则 EPERM |
| `switch_address_space` | do_sdevio.c:96,159 | 切到目标空间执行 phys_*；完成切回 |
| 仅 byte/word | do_sdevio.c:100-104,138-148 | long → EINVAL；`phys_insl` 不存在 |
| `phys_insb/outsb/insw/outsw` | do_sdevio.c:134-150 | 按 count 重复的批量 I/O 原语 |

#### §2.5 do_iopenable.c — IOPL 提权

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_iopenable()` | do_iopenable.c:19-33 | SELF→okendpt(24-25)→isokendpt(26-27)→enable_iop(28)→OK(29) |
| `enable_iop()` | (arch) | `pp->p_reg.psw |= 0x3000` 设 IOPL=3 |
| `#if ENABLE_USERPRIV` | do_iopenable.c:23,31 | 编译期门控；关闭时返回 EPERM |

#### §2.6 do_readbios.c — BIOS 读取

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_readbios()` | do_readbios.c:15-37 | src=NONE/buf(21-24)→limit(26)→USERRANGE(28-34)→virtual_copy_vmcheck(36) |
| BIOS 内存范围 | do_readbios.c:32-33 | `BIOS_MEM_BEGIN..END`(0x0-0x4FF) + `BASE_MEM_TOP..UPPER_MEM_END`(0x90000-0xFFFFF) |
| USERRANGE 宏 | do_readbios.c:28-30 | `VINRANGE(src,a,b) && VINRANGE(limit,a,b)`；首尾都要在范围内 |
| `virtual_copy_vmcheck` | do_readbios.c:36 | src 是物理地址（NONE endpoint），dst 是 caller buffer |

#### §2.7 调用关系图

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

### Ch3. 设计决策（hypothesis-driven）

#### D1. IRQ 子请求表达：整数 switch vs enum

**假设性推理**:
- 如果用整数 switch（C 方式）：request 是 i32，每个 case 是魔数 0/1/2/3；新增子请求需改多处 match；编译器不检查穷尽性；调用者传 99 不会编译期发现。
- 如果用 `IrqctlRequest` enum + `TryFrom<i32>`：类型安全，`match` 编译期检查穷尽性；`TryFrom` 集中处理非法值返回 EINVAL；`#[repr(i32)]` 保证 ABI 兼容。
- 所以用 `IrqctlRequest` enum：类型安全 + 穷尽性检查 + 集中校验。

**实现**: `IrqctlRequest { SetPolicy=0, RmPolicy=1, Enable=2, Disable=3 }` + `impl TryFrom<i32>` (syscall_device.rs:36-61)。

#### D2. I/O 端口操作：直接 in/out vs trait PortIo

**假设性推理**:
- 如果直接 `inb/outb`（C 方式）：x86 内联汇编散落在内核代码；ARM/RISC-V 需 `#[cfg(target_arch)]` 分支；每加一架构改多处；测试无法 mock（真实 I/O 端口访问）。
- 如果用 `trait PortIo`：内核代码依赖 trait 方法；x86 提供 `X86_64PortIo`（`in/out` 指令），ARM/RISC-V 提供 MMIO impl；测试用 `MockPortIo`；泛型 `<PI: PortIo>` 静态分发零虚拟开销。
- 所以用 `trait PortIo`：多架构支持 + 可测试。

**实现**: `pub use minix_plat::PortIo` (syscall_device.rs:161) + `X86_64PortIo`/`MockPortIo` impl。

#### D3. IRQ hook 池：全局数组 vs IrqManager<IC> 泛型

**假设性推理**:
- 如果用全局 `irq_hooks[]` 数组（C 方式）：无封装，任意代码可改；无法 mock 中断控制器；测试需真实硬件。
- 如果用 `IrqManager<IC: InterruptController>` 泛型：钩子池+链表封装；IC 注入可 mock；`IrqManager<X86Apic>`/`IrqManager<MockIc>` 可测。
- 所以用 `IrqManager<IC>` 泛型：封装 + 可测试。

**实现**: `IrqManager<IC>` (irq_manager.rs) — **类型层完整**；dispatch 层 BadCall（见 D6）。

> **dispatch 层 DEFERRED 状态（诚实标注）**：
> - 类型层：`IrqManager<IC>` + `dispatch_irqctl<IC>` (syscall_device.rs:174-310) 4 子请求完整实现
> - dispatch 层：`syscall.rs:533-597 dispatch_irqctl` 返回 `BadCall`，因为 `KernelState` 未持有 `IrqManager<ArchIc>` 单态（dispatch 函数签名只有 `caller, msg`）
> - 根因：`IrqManager<IC>` 是泛型，需 `KernelState` 持有具体单态后才能从 dispatch 传入
> - 完整 DEFERRED 理由与依赖见 design.md §3.1

#### D4. generic_handler：函数指针+全局 vs IrqHookContext+IrqNotify trait

**假设性推理**:
- 如果用 C 方式（`static int generic_handler(irq_hook_t* hook)` + 全局 `mini_notify`）：函数指针不可 mock；`mini_notify` 全局访问导致 unsafe 散布；测试需真实 IPC 引擎。
- 如果用 `IrqHookContext<'a>` + `IrqNotify` trait：钩子信息打包成 context；通知抽象为 trait（`KernelNotifier` 生产 / `MockNotifier` 测试）；handler 签名 `fn(&mut IrqHookContext) -> IrqAction` 类型安全。
- 所以用 `IrqHookContext` + `IrqNotify` trait：可测试 + 安全。

**实现**: `IrqHookContext<'a>` (irq_manager.rs:71-85) + `trait IrqNotify` (irq_manager.rs:52-58) + `KernelNotifier` (irq_manager.rs:119-183)。

#### D5. VDEVIO 缓冲区：静态可变 vs 栈分配

**假设性推理**:
- 如果用 `static mut vdevio_buf`（C 方式）：no_std 下静态可变需 `unsafe`；SMP 下需锁保护（C 用 `lock()`）；多 CPU 并发 VDEVIO 会争用。
- 如果用栈分配 `[u8; VDEVIO_BUF_SIZE]`：每个调用栈独立，无竞争；`unsafe` 仅在 cast 时；BKL 已串行化无需额外锁。
- 所以用栈分配：no_std 友好 + SMP 安全。

**实现**: D5 决策；当前 dispatch_vdevio DEFERRED（需 data_copy_vmcheck），缓冲区待实现时落地。

#### D6. x86-only 调用：#[cfg(target_arch)] vs trait + BadCall

**假设性推理**:
- 如果用 `#[cfg(target_arch)]` 在内核代码中行为选择：每处 I/O 调用都需 cfg 分支；架构耦合渗入内核；新增架构需改内核代码；违反"硬件抽象为 trait"原则。
- 如果用 trait + BadCall：内核 dispatch 层有 `dispatch_arch_*` 函数（cfg 门控只在此处），x86 转发到 `syscall_device::dispatch_*`，非 x86 返回 `BadCall`；内核主体架构无关。
- 所以用 trait + BadCall：架构耦合集中在 arch 层，内核主体纯净。这是 D9 全局决策在设备 I/O 的应用。

**实现**: `#[cfg(target_arch="x86_64")] dispatch_arch_devio/sdevio/vdevio/iopenable/readbios` (syscall.rs:1179-1226) 转发；非 x86 返回 `BadCall`。

#### D7. I/O 类型/方向：裸位掩码 vs IoSize/IoDirection enum

**假设性推理**:
- 如果用裸 `i32` 位掩码（C 方式）：`request & 0x0F0` 是魔数；type/dir 混在 int 里易写错；无类型安全。
- 如果用 `IoSize`/`IoDirection` enum + `from_request_mask`：类型安全；解码集中；`match` 穷尽性检查。
- 所以用 enum：类型安全 + 集中解码。

**实现**: `IoSize { Byte=1, Word=2, Long=4 }` + `IoDirection { Input, Output }` (syscall_device.rs:71-115)。

---

### Ch4. 实现详解（真实代码）

#### §4.1 dispatch_irqctl（类型层完整，dispatch 层 BadCall）

贴 `syscall_device.rs:174-310` 的 `dispatch_irqctl<IC>` 完整 4 子请求分支代码，标注 C 对应 do_irqctl.c 行号。

**诚实标注**:
- 类型层（syscall_device.rs:174-310）：4 子请求完整，调用 `IrqManager` 方法
- dispatch 层（syscall.rs:533-597）：仅 step1-3 校验（request/vector/sys_proc），hook 链操作返回 `BadCall`
- 根因：`KernelState` 未持有 `IrqManager<ArchIc>`，dispatch 签名无 `&mut IrqManager` 参数

#### §4.2 dispatch_devio（完整）

贴 `syscall_device.rs:325-402` 代码：request 解码→CHECK_IO_PORT→对齐→`PortIo::in/out`→写回结果。

**与 C 的差异**:
- unknown type：C default size=4（do_devio.c:35），Rust 返回 EINVAL（合理收紧）
- PortIo trait 替代直接 `inb/outb`

#### §4.3 dispatch_vdevio（部分 DEFERRED）

贴 `syscall_device.rs:418-466` 代码：参数验证完整，批量 I/O 返回 ENOSYS。

**DEFERRED 项**:
- `data_copy_vmcheck` 拷入/拷出（依赖跨空间拷贝子系统）
- 批量 CHECK_IO_PORT + 批量 in/out（依赖已拷入的数组）
- 返回 ENOSYS（非 OK）避免 silent 语义漂移

#### §4.4 dispatch_sdevio（部分 DEFERRED）

贴 `syscall_device.rs:557-665` 代码：参数验证 + endpoint + CHECK_IO_PORT + 对齐完整，批量 I/O 返回 ENOSYS。

**DEFERRED 项**:
- `verify_grant`（safe 变体 grant→物理地址映射）
- `switch_address_space` + `phys_insb/outsb/insw/outsw`（跨空间批量 I/O 原语）
- 返回 ENOSYS 避免语义漂移

#### §4.5 dispatch_iopenable（完整，trap frame DEFERRED）

贴 `syscall_device.rs:483-528` 代码：SELF 解析 + endpoint 验证 + iskerneln + `enable_user_io()`。

**DEFERRED 项**:
- 运行中进程的 trap frame 同步更新（需 scheduler/arch trap frame 访问）

#### §4.6 dispatch_readbios（部分 DEFERRED）

贴 `syscall_device.rs:692-731` 代码：size==0 guard + checked_add + BIOS 范围检查，拷贝返回 ENOSYS。

**DEFERRED 项**:
- `virtual_copy_vmcheck`（src=NONE 物理地址，dst=caller buffer）
- 返回 ENOSYS 避免语义漂移

#### §4.7 PortIo trait + BadCall 架构抽象

贴 `syscall_device.rs:150-161` PortIo re-export + `syscall.rs:1179-1226` dispatch_arch_* cfg 门控。

**架构差异表**（同 §1.4）。

#### §4.8 DEFERRED 汇总表

| 函数 | C 位置 | 状态 | DEFERRED 理由 |
|------|--------|------|--------------|
| dispatch_irqctl (dispatch 层) | do_irqctl.c:23-138 | BadCall | `KernelState` 未持有 `IrqManager<ArchIc>` |
| dispatch_vdevio 批量 I/O | do_vdevio.c:67-156 | ENOSYS | 需 `data_copy_vmcheck` |
| dispatch_sdevio 批量 I/O | do_sdevio.c:68-150 | ENOSYS | 需 `verify_grant` + `switch_address_space` + `phys_*` |
| dispatch_readbios 拷贝 | do_readbios.c:36 | ENOSYS | 需 `virtual_copy_vmcheck` |
| dispatch_iopenable trap frame | (Rust 独有) | 部分 | 需 scheduler/arch trap frame 访问 |
| generic_handler get_randomness | do_irqctl.c:154 | 缺失 | 需 krandom 子系统 |

---

### Ch5. 测试（可 grep 函数名）

#### §5.1 现有测试（已实现，30+ 个）

| 测试函数 | 验证行为 | 对应 C 符号 |
|---------|---------|------------|
| `test_irqctl_request_try_from` | IrqctlRequest TryFrom 全分支 + 非法值 | `do_irqctl` request 解码 |
| `test_io_size_from_mask` | IoSize 从 mask 解码 | `_DIO_TYPEMASK` |
| `test_io_direction_from_mask` | IoDirection 从 mask 解码 | `_DIO_DIRMASK` |
| `test_dio_combined_request` | 组合 request 的 type+dir 解码 | `_DIO_INPUT|_DIO_BYTE` |
| `test_irq_reenable_flag` | IRQ_REENABLE=0x01 | `IRQ_REENABLE` |
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
| `test_iopenable_kernel_process_returns_eperm` | kernel 目标 → EPERM | do_iopenable.c:61 (sdevio 同理) |
| `test_iopenable_iopl_bits_only_affects_bits_12_13` | IOPL 仅影响 12-13 位 | `psw |= 0x3000` |
| `test_sdevio_invalid_endpoint_returns_einval` | 无效 endpoint → EINVAL | do_sdevio.c:59-60 |
| `test_sdevio_kernel_target_returns_eperm` | kernel 目标 → EPERM | do_sdevio.c:61 |
| `test_sdevio_unsafe_target_not_caller_returns_eperm` | unsafe 非 caller → EPERM | do_sdevio.c:84-89 |
| `test_sdevio_long_type_returns_einval` | long 不支持 → EINVAL | do_sdevio.c:138-148 |
| `test_sdevio_unaligned_port_returns_eperm` | 未对齐 → EPERM | do_sdevio.c:126-131 |
| `test_sdevio_check_io_port_denied` | 端口不在范围 EPERM | do_sdevio.c:116-123 |
| `test_sdevio_valid_returns_enosys` | 有效输入 → ENOSYS（DEFERRED） | (Rust DEFERRED 标注) |
| `test_sdevio_invalid_direction_returns_einval` | 非法方向 → EINVAL | do_sdevio.c:151-153 |
| `test_readbios_zero_size_returns_einval` | size=0 → EINVAL | (Rust anti-overflow) |
| `test_readbios_outside_bios_range_returns_eperm` | 超范围 → EPERM | do_readbios.c:32-34 |
| `test_readbios_in_bios_mem_range_returns_enosys` | 低段范围 → ENOSYS | (DEFERRED) |
| `test_readbios_in_upper_mem_range_returns_enosys` | 高段范围 → ENOSYS | (DEFERRED) |
| `test_readbios_straddling_ranges_returns_eperm` | 跨段 → EPERM | do_readbios.c:32-34 |
| `test_readbios_overflow_returns_einval` | addr+size 溢出 → EINVAL | (Rust anti-overflow) |

#### §5.2 待补充测试（DEFERRED 函数实现后）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_dispatch_irqctl_setpolicy_full` | SETPOLICY 完整路径 | IrqManager 接入 KernelState |
| `test_dispatch_irqctl_rmpolicy_owner_check` | RMPOLICY owner 校验 | 同上 |
| `test_dispatch_vdevio_batch_io` | 批量 I/O 端到端 | `data_copy_vmcheck` |
| `test_dispatch_sdevio_safe_grant` | safe 变体 verify_grant | `verify_grant` |
| `test_dispatch_sdevio_phys_batch` | phys_insb/outsb 批量 | `switch_address_space` + `phys_*` |
| `test_dispatch_readbios_copy` | BIOS 拷贝端到端 | `virtual_copy_vmcheck` |

---

### Ch6. 参见

- [14-exception-interrupt.md](../14-exception-interrupt.md) — IRQ hook 注册/分发 + IrqManager 设计
- [22-privilege.md](../22-privilege.md) — CHECK_IO_PORT / CHECK_IRQ 权限表
- [13-syscall-dispatch.md](../13-syscall-dispatch.md) — syscall dispatch 架构 + D9 BadCall 决策
- [18-syscall-copy.md](../18-syscall-copy.md) — `data_copy_vmcheck` / `virtual_copy_vmcheck` / `verify_grant`（VDEVIO/SDEVIO/READBIOS DEFERRED 依赖）
- [16-smp.md](../16-smp.md) — BKL 串行化（dispatch 入口持锁）

---

## 三、知识点覆盖矩阵

| 概念组 | Ch1 | Ch2 | Ch3 | Ch4 | Ch5 |
|--------|-----|-----|-----|-----|-----|
| A. IRQ 控制 | §1.1 | §2.1 | D1, D3, D4 | §4.1 | test_irqctl_*, test_check_irq_* |
| B. DEVIO | §1.2 | §2.2 | D2, D7 | §4.2 | test_devio_*, test_io_* |
| C. VDEVIO | §1.2 | §2.3 | D5 | §4.3 (DEFERRED) | (待补) |
| D. SDEVIO | §1.2 | §2.4 | D2 | §4.4 (DEFERRED) | test_sdevio_* |
| E. IOPENABLE/READBIOS | §1.3 | §2.5, §2.6 | — | §4.5, §4.6 | test_iopenable_*, test_readbios_* |
| F. 架构抽象 | §1.4 | §2.7 | D2, D6 | §4.7 | (arch 层测试) |
| G. redox 对比 | — | — | — | design.md 附录 B | — |

---

## 四、断裂修复表

| 断裂点 | 修复方案 |
|--------|---------|
| "⚠️ DEFERRED @ dispatch" 表格行 | Ch3 D3 hypothesis-driven 重写；状态移至 Ch4 §4.1 + §4.8 |
| "DEFERRED 状态说明（2026-06-14 更新）"整节 | **整节删除**；信息分散到 Ch4 §4.x DEFERRED 标注 + design.md §3 |
| P0-07/P1-05/P1-09 内部 review ID | 删除；用语义依赖描述（"依赖 KernelState 重构"等） |
| dispatch_irqctl BadCall 未诚实标注 | Ch3 D3 + Ch4 §4.1 明确"类型层完整，dispatch 层 BadCall" |
| dispatch_sdevio/readbios DEFERRED 未列 | Ch4 §4.4/§4.6 + §4.8 汇总表 |
| 测试 bullet 不可 grep | Ch5 §5.1 列出 33 个实际 `fn test_*` 函数名 |
| Ch2 行号粗略 | Ch2 每符号带 file:line 精确锚点 |
| 缺 SDEVIO/IOPENABLE/READBIOS C 分析 | Ch2 §2.4/§2.5/§2.6 补全 |
| 缺 generic_handler C 分析 | Ch2 §2.1 补 do_irqctl.c:143-172 |
| Ch3 非 hypothesis-driven | Ch3 D1-D7 重写为"如果 X 会有 Y 问题所以用 Z" |
| 缺 redox 对比 | design.md 附录 B 补 IrqScheme + Pio |
| Ch1 §1.3 "架构相关性"过浅 | Ch1 §1.4 新增"架构抽象"节 |

---

## 五、自检

- [x] Ch1 主语是硬件/安全，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch2 每个符号带 file:line
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] Ch3 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED + 理由
- [x] Ch5 测试函数可 grep 验证（`fn test_*`）
- [x] 知识点覆盖矩阵完整（A-G 七组）
- [x] 断裂修复表完整（12 处断裂 + 修复方案）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无迭代叙事日期（2026-XX-XX）
- [x] 跨架构统一抽象（PortIo trait + BadCall）
- [x] anti-translate 体现（IrqctlRequest enum / PortIo trait / IrqManager<IC> / IrqNotify trait / IoSize enum）
- [x] VMCTL 归属判定（不适用，属内存系统调用）
