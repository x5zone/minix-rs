# 16-do-fork-copy - do_fork 进程结构复制

> 本文档分析 `minix3/minix/kernel/system/do_fork.c` 第 51-90 行，讲解 do_fork 函数的进程结构复制部分。

---

## 1. 概述

进程结构复制是 fork 系统调用的核心步骤。在 Minix3 中，每个进程在内核中都有一个 `proc` 结构体，记录了进程的全部运行时状态：寄存器、调度信息、IPC 状态、端点标识等。fork 的语义要求子进程是父进程的副本，因此最直接的方式就是将父进程的 `proc` 结构体整体复制到子进程的槽位中。

然而，简单的整体复制并不足够——子进程需要拥有自己独立的身份（不同的端点、进程号）和独立的硬件状态（FPU 缓冲区、页表）。因此，复制操作分为三个阶段：

1. **复制前保存**：保存子进程槽位中即将被覆盖的关键信息（FPU 缓冲区指针、端点代数），这些信息在整体复制后会丢失，但子进程仍需使用。
2. **整体复制**：通过 `*rpc = *rpp` 将父进程的整个 `proc` 结构体赋值给子进程，子进程此时继承了父进程的所有字段。
3. **字段修正**：恢复子进程的 FPU 缓冲区指针、递增端点代数、设置新的进程号和端点，使子进程获得独立的身份。

这种先复制、后修正的策略，既保证了复制的完整性（所有字段都被复制），又确保了子进程的独立性（关键字段被正确覆盖）。

### 1.1 进程结构复制

Minix3 的 fork 采用**整体赋值 + 字段修正**的机制。具体流程如下：

```c
// do_fork.c 第 56-72 行
save_fpu(rpp);                                    // 1. 保存父进程 FPU 状态
gen = _ENDPOINT_G(rpc->p_endpoint);               // 2. 保存子进程槽位的端点代数
old_fpu_save_area_p = rpc->p_seg.fpu_state;       // 3. 保存子进程的 FPU 缓冲区指针
*rpc = *rpp;                                      // 4. 整体复制 proc 结构体
rpc->p_seg.fpu_state = old_fpu_save_area_p;       // 5. 恢复子进程的 FPU 缓冲区指针
if(proc_used_fpu(rpp))                            // 6. 如果父进程使用过 FPU
    memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);  // 复制 FPU 状态内容
if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;    // 7. 递增端点代数
rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;     // 8. 恢复子进程号
rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);      // 9. 生成新端点
```

核心思路是：先用 C 语言的结构体赋值 `*rpc = *rpp` 完成一次性整体复制，再逐个修正子进程需要独立拥有的字段。这种方式简单高效，但需要仔细识别哪些字段需要修正——遗漏任何一个都可能导致两个进程共享了不该共享的状态。

### 1.2 FPU 状态处理

FPU（浮点运算单元）状态的处理是进程结构复制中最微妙的环节。问题在于 `proc` 结构体中的 `p_seg.fpu_state` 字段是一个**指针**，指向该进程专属的 FPU 状态缓冲区，而非内联数据。

在 i386 架构中，每个用户进程在 `fpu_state[NR_PROCS][FPU_XFP_SIZE]` 静态数组中拥有一个独立的 FPU 缓冲区（见 `arch_system.c` 第 144 行）。这个缓冲区在 `arch_proc_reset()` 中被分配给进程（第 168 行：`pr->p_seg.fpu_state = v`）。

当执行 `*rpc = *rpp` 整体复制时，子进程的 `fpu_state` 指针会被覆盖为父进程的指针——这意味着两个进程将共享同一个 FPU 缓冲区，这是严重的错误。因此，Minix3 采取了三步保护措施：

1. **复制前保存**子进程的 FPU 缓冲区指针（`old_fpu_save_area_p = rpc->p_seg.fpu_state`）
2. **复制后恢复**子进程的指针（`rpc->p_seg.fpu_state = old_fpu_save_area_p`）
3. 如果父进程使用过 FPU，则**复制 FPU 状态内容**到子进程的缓冲区（`memcpy`）

此外，在复制前还必须调用 `save_fpu(rpp)` 确保父进程的 FPU 状态已从硬件寄存器保存到内存缓冲区，否则复制到的可能是过时的数据。

---

## 2. C 源码分析

本节逐行分析 `do_fork.c` 第 56-72 行的进程结构复制代码，涵盖 FPU 状态保存、结构体整体赋值、FPU 指针恢复、FPU 内容复制、端点代数递增、进程号恢复和端点生成等关键操作。每个操作都涉及对 `proc` 结构体中特定字段的精确处理，任何遗漏都可能导致进程状态不一致。

### 2.1 FPU 状态保存

FPU 状态保存是进程结构复制的第一步，对应源码第 56-57 行：

```c
save_fpu(rpp);
```

这条语句确保父进程的 FPU 状态已从硬件寄存器写入内存中的 FPU 缓冲区。在 i386 架构中，FPU 状态可能仍驻留在硬件寄存器中（如果父进程是当前 FPU 的拥有者），尚未写回内存。如果不先保存，后续的 `*rpc = *rpp` 复制到的将是缓冲区中的过时数据，而非硬件中的最新状态。

#### 2.1.1 save_fpu 调用

`save_fpu(rpp)` 的作用是将父进程的 FPU 上下文从硬件寄存器保存到内存缓冲区。其实现逻辑（见 `arch/i386/arch_system.c` 第 111-139 行）如下：

1. **SMP 场景**：如果父进程运行在其他 CPU 上，需要先停止该进程并强制保存其上下文（`smp_schedule_stop_proc_save_ctx`），然后再恢复其运行状态。
2. **单核场景**：检查父进程是否是当前 CPU 的 FPU 拥有者（`fpu_owner == pr`），如果是，则调用 `save_local_fpu` 执行 `fxsave` 或 `fnsave` 指令将 FPU 寄存器内容写入 `pr->p_seg.fpu_state` 指向的缓冲区。

关键点：`save_fpu` 只在父进程确实是当前 FPU 拥有者时才执行实际保存操作。如果父进程不是 FPU 拥有者，说明其 FPU 状态已经在之前被保存过了，缓冲区中的数据是最新的，无需再次保存。

#### 2.1.2 保存时机

在复制前保存 FPU 状态是必要的，原因在于 FPU 硬件采用延迟保存（lazy save）策略：只有当另一个进程尝试使用 FPU 时，内核才会触发保存当前 FPU 拥有者的状态。这意味着，在调用 `save_fpu(rpp)` 之前，父进程的 FPU 状态可能仍驻留在硬件寄存器中，而内存缓冲区 `rpp->p_seg.fpu_state` 中的数据是过时的。

如果不先调用 `save_fpu`，直接执行 `*rpc = *rpp`，复制到的 FPU 缓冲区内容就不是父进程当前的 FPU 状态。后续 `memcpy` 将这个过时数据复制到子进程的 FPU 缓冲区，子进程恢复执行时将得到错误的浮点运算结果。

因此，`save_fpu(rpp)` 必须在 `*rpc = *rpp` 之前调用，确保父进程的 FPU 状态已经从硬件寄存器写回内存，使后续的复制操作能获取到完整且最新的状态。

### 2.2 进程结构复制

进程结构复制的核心是 C 语言的结构体赋值操作，对应源码第 59-72 行。这段代码可以分为三个逻辑组：

**第一组：复制前保存**（第 59-62 行）
```c
gen = _ENDPOINT_G(rpc->p_endpoint);           // 保存子进程槽位的端点代数
old_fpu_save_area_p = rpc->p_seg.fpu_state;   // 保存子进程的 FPU 缓冲区指针
```
在整体赋值之前，必须保存子进程槽位中即将被覆盖的两个关键信息。端点代数用于后续生成新的端点；FPU 缓冲区指针则是子进程专属的内存区域，不能被父进程的指针覆盖。

**第二组：整体复制**（第 63 行）
```c
*rpc = *rpp;   // 整体复制 proc 结构体
```
这是 C 语言的结构体赋值语义：将 `rpp` 指向的整个 `proc` 结构体按字节复制到 `rpc` 指向的位置。复制后，`rpc` 的所有字段与 `rpp` 完全相同。

**第三组：字段修正**（第 64-72 行）
```c
rpc->p_seg.fpu_state = old_fpu_save_area_p;   // 恢复子进程的 FPU 缓冲区指针
if(proc_used_fpu(rpp))                         // 条件复制 FPU 状态内容
    memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1; // 递增端点代数
rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;  // 恢复子进程号
rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);   // 生成新端点
```
修正子进程需要独立拥有的字段，确保两个进程不会共享不该共享的状态。

#### 2.2.1 结构体赋值

`*rpc = *rpp` 是 C 语言中结构体指针解引用后的赋值操作，其语义是：将 `rpp` 指向的 `struct proc` 实例的所有字节，按成员顺序复制到 `rpc` 指向的 `struct proc` 实例中。

在 C 语言标准中，结构体赋值执行的是**浅拷贝**（shallow copy）：
- 对于标量字段（如 `int`、`long`），直接复制值
- 对于内嵌结构体，递归执行成员级复制
- 对于指针字段（如 `char *fpu_state`），复制指针值本身，而非指针指向的内容

这意味着复制后，`rpc` 和 `rpp` 的所有标量和内嵌结构体字段完全相同，但指针字段指向同一块内存。这就是为什么 FPU 缓冲区指针需要在复制后立即修正——否则两个进程将共享同一个 FPU 缓冲区。

##### 2.2.1.1 整体复制

整体复制 `*rpc = *rpp` 的效果是：子进程的 `proc` 结构体中的每一个字节都被父进程的对应字节覆盖。复制完成后，`rpc` 指向的结构体与 `rpp` 指向的结构体在内容上完全一致——包括进程号 `p_nr`、端点 `p_endpoint`、运行时标志 `p_rts_flags`、调度优先级、IPC 状态、寄存器保存区等所有字段。

这种整体复制的方式有一个重要优势：**不需要逐一列举需要复制的字段**。`proc` 结构体包含数十个字段，如果逐字段复制，不仅代码冗长，而且容易遗漏。整体赋值由编译器生成高效的内存复制代码（通常优化为 `memcpy`），既简洁又高效。

但整体复制也带来了一个必须处理的问题：子进程的某些字段必须与父进程不同（如端点、进程号），某些指针字段必须指向子进程自己的内存（如 FPU 缓冲区）。这些字段需要在复制后逐一修正。

##### 2.2.1.2 字段继承

整体复制后，子进程继承了父进程的所有字段值，包括：

| 字段类别 | 继承的字段 | 后续是否修正 |
|---------|-----------|------------|
| 进程标识 | `p_nr`, `p_endpoint` | ✅ 必须修正 |
| 调度信息 | `p_priority`, `p_cpu_time_left` | ✅ 后续章节修正 |
| 运行时标志 | `p_rts_flags` | ✅ 后续章节修正 |
| 杂项标志 | `p_misc_flags` | ✅ 后续章节修正 |
| IPC 状态 | `p_getfrom_e`, `p_sendto_e`, `p_sendmsg` 等 | 部分修正 |
| 寄存器 | `p_reg`（含 `retreg`） | ✅ retreg 修正为 0 |
| FPU 状态 | `p_seg.fpu_state`（指针） | ✅ 必须修正 |
| 信号 | `p_pending` | ✅ 后续章节清空 |
| 进程名 | `p_name` | ✅ 后续章节追加后缀 |
| 时间统计 | `p_user_time`, `p_sys_time` | ✅ 后续章节清零 |

继承的字段中，有些需要保持与父进程相同（如调度优先级），有些必须修正为子进程独立的值（如端点、进程号），还有些需要清零或重置（如时间统计）。后续章节（17-19）将详细分析这些修正操作。

#### 2.2.2 复制前的保存

在执行 `*rpc = *rpp` 之前，代码保存了两个关键信息：端点代数和 FPU 缓冲区指针。这两个信息在子进程的 `proc` 结构体中已经存在（由 `arch_proc_reset` 初始化），但整体复制会将它们覆盖为父进程的值。由于子进程需要保留自己的 FPU 缓冲区和使用新的端点代数，必须在覆盖前保存。

##### 2.2.2.1 gen 变量

`gen = _ENDPOINT_G(rpc->p_endpoint)` 从子进程槽位当前的端点中提取代数（generation）。注意这里读取的是 `rpc`（子进程）的端点，而非 `rpp`（父进程）的端点。

为什么需要读取子进程的端点代数？因为子进程槽位在 fork 之前已经被初始化过（由 `arch_proc_reset` 完成），其端点中包含了一个有效的代数值。fork 后，子进程需要一个新的端点，其代数应基于当前槽位的代数递增，而非从 0 开始。这样保证了同一槽位每次被重用时，端点的代数都会递增，使得旧的端点引用自动失效。

`_ENDPOINT_G` 宏的定义为 `(((e)+MAX_NR_TASKS) >> 15)`，通过偏移加右移提取端点的高位部分作为代数。

##### 2.2.2.2 端点代数

端点代数（generation）是 Minix3 endpoint 机制的核心组成部分。每个端点由两部分编码：

```
endpoint = (generation << 15) + slot
```

- **slot**（低 15 位）：进程在进程表中的槽位号，范围 `[-MAX_NR_TASKS, MAX_NR_PROCS)`
- **generation**（高位）：槽位的重用计数器，每次槽位被新进程占用时递增

代数的作用是**防止 ABA 问题**：当一个进程退出后，其槽位可能被新进程重用。如果端点只包含 slot，旧进程的通信方仍持有旧端点，消息会错误地发送到新进程。代数递增后，旧端点自动失效，内核检测到代数不匹配时返回 `EDEADEPT` 错误。

代数的最大值为 `_ENDPOINT_MAX_GENERATION = INT_MAX / 32768 - 1 = 65534`，超过后回绕为 1（不使用 0，因为 generation=0 时 endpoint 等于 slot，用于硬编码的内核任务端点）。

#### 2.2.3 i386 特定处理

i386 架构下的 FPU 处理是条件编译的，对应源码第 60-68 行：

```c
#if defined(__i386__)
  old_fpu_save_area_p = rpc->p_seg.fpu_state;
#endif
  *rpc = *rpp;
#if defined(__i386__)
  rpc->p_seg.fpu_state = old_fpu_save_area_p;
  if(proc_used_fpu(rpp))
        memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
#endif
```

这段代码仅在 i386 架构下编译。在 ARM 架构（`__arm__`）下，FPU 状态的处理方式不同，不需要这种"保存-恢复-复制"的模式。这是因为不同架构的 FPU 状态管理机制不同：

- **i386**：FPU 状态存储在进程专属的静态缓冲区中，`p_seg.fpu_state` 是指向该缓冲区的指针，整体复制会覆盖这个指针
- **ARM**：FPU 状态可能以不同方式管理，不需要相同的指针保护

这种架构相关的条件编译在 Minix3 中很常见，反映了不同硬件平台对同一机制的不同实现方式。

##### 2.2.3.1 old_fpu_save_area_p

`old_fpu_save_area_p` 是一个局部变量，用于在整体复制前保存子进程槽位的 FPU 缓冲区指针。其声明为：

```c
#if defined(__i386__)
  char *old_fpu_save_area_p;
#endif
```

这个变量的作用是**临时保存子进程专属的 FPU 缓冲区地址**。在 i386 架构中，每个用户进程在静态数组 `fpu_state[NR_PROCS][FPU_XFP_SIZE]` 中有一个固定的 FPU 缓冲区，`p_seg.fpu_state` 指向该缓冲区。子进程的缓冲区在 `arch_proc_reset` 中被分配。

当执行 `*rpc = *rpp` 时，`rpc->p_seg.fpu_state` 会被覆盖为 `rpp->p_seg.fpu_state`（即父进程的缓冲区指针）。如果不保存子进程的原始指针，复制后子进程将丢失自己的缓冲区引用，导致内存泄漏和状态混乱。

##### 2.2.3.2 FPU 状态指针恢复

这条语句在整体复制后立即执行，将子进程的 `fpu_state` 指针恢复为其自己的缓冲区地址。

整体复制 `*rpc = *rpp` 后，`rpc->p_seg.fpu_state` 被覆盖为父进程的缓冲区指针 `rpp->p_seg.fpu_state`。此时两个进程的 `fpu_state` 指向同一块内存，这是错误的——每个进程必须拥有独立的 FPU 缓冲区。

恢复操作将 `rpc->p_seg.fpu_state` 重新指向子进程自己的缓冲区（之前保存在 `old_fpu_save_area_p` 中）。恢复后，子进程的 `fpu_state` 指针正确，但缓冲区内容仍然是初始值（全零），需要通过后续的 `memcpy` 从父进程复制 FPU 状态内容。

### 2.3 FPU 状态复制

FPU 状态复制对应源码第 66-68 行：

```c
if(proc_used_fpu(rpp))
    memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
```

这段代码在恢复子进程的 FPU 缓冲区指针之后执行，条件性地将父进程的 FPU 状态内容复制到子进程的缓冲区中。条件判断 `proc_used_fpu(rpp)` 检查父进程是否曾经使用过 FPU——如果父进程从未执行过浮点运算，其 FPU 缓冲区中没有任何有意义的数据，无需复制。

#### 2.3.1 proc_used_fpu 检查

`proc_used_fpu(rpp)` 是一个宏，定义在 `proc.h` 第 175 行：

```c
#define proc_used_fpu(p) ((p)->p_misc_flags & (MF_FPU_INITIALIZED))
```

它检查父进程的 `p_misc_flags` 中是否设置了 `MF_FPU_INITIALIZED` 标志。这个标志在进程首次使用 FPU 时由内核设置（见 `restore_fpu` 函数：`pr->p_misc_flags |= MF_FPU_INITIALIZED`）。

如果父进程从未使用过 FPU，`MF_FPU_INITIALIZED` 未设置，`proc_used_fpu` 返回假，跳过 `memcpy`。这是合理的优化：未使用 FPU 的进程，其缓冲区内容无意义，子进程也不需要继承 FPU 状态。子进程首次使用 FPU 时，内核会通过 `fninit` 指令初始化 FPU 硬件并设置 `MF_FPU_INITIALIZED` 标志。

#### 2.3.2 memcpy 复制

这条 `memcpy` 将父进程 FPU 缓冲区的全部内容复制到子进程的 FPU 缓冲区中。

- **源地址**：`rpp->p_seg.fpu_state`，父进程的 FPU 缓冲区（已在 `save_fpu` 中确保包含最新状态）
- **目标地址**：`rpc->p_seg.fpu_state`，子进程的 FPU 缓冲区（已恢复为子进程自己的缓冲区指针）
- **大小**：`FPU_XFP_SIZE`，FPU 扩展状态的完整大小

复制后，子进程的 FPU 缓冲区包含了与父进程完全相同的浮点运算状态，包括寄存器值、控制字、状态字等。当子进程恢复执行时，FPU 状态将被正确恢复，浮点运算可以继续进行，就像 fork 从未发生过一样。

#### 2.3.3 FPU 状态大小

`FPU_XFP_SIZE` 是 i386 架构下 FPU 扩展状态（Extended Floating Point）的完整大小。它定义了保存 FPU/SSE 状态所需的缓冲区字节数，包括：

- **x87 FPU 寄存器**：8 个 80 位浮点寄存器、控制字、状态字、标签字等
- **SSE 寄存器**：8 个 128 位 XMM 寄存器（如果支持 SSE）
- **MXCSR**：SSE 控制/状态寄存器

`FPU_XFP_SIZE` 的值取决于具体的 FPU 类型：支持 SSE 时使用 `fxsave` 格式（512 字节），仅 x87 时使用 `fnsave` 格式（108 字节）。Minix3 在运行时根据 CPU 特性选择保存格式，但缓冲区统一按最大尺寸分配。

在 ARM 架构下，FPU 状态的大小和格式不同，因此这段代码被 `#if defined(__i386__)` 条件编译保护。

### 2.4 端点代数递增

端点代数递增对应源码第 69-70 行：

```c
if(++gen >= _ENDPOINT_MAX_GENERATION)
    gen = 1;
```

这条语句将代数加 1，并在超过最大值时回绕为 1。代数递增保证了同一槽位每次被新进程使用时，端点值都会改变，使得旧端点引用自动失效。

`_ENDPOINT_MAX_GENERATION` 定义为 `INT_MAX / 32768 - 1 = 65534`，这是代数的上限，确保生成的端点值不会超过 `INT_MAX`。

#### 2.4.1 代数递增

`if(++gen >= _ENDPOINT_MAX_GENERATION)` 检查递增后的代数是否超过了最大允许值。

`_ENDPOINT_MAX_GENERATION = INT_MAX / _ENDPOINT_GENERATION_SIZE - 1 = 2147483647 / 32768 - 1 = 65534`。

为什么代数不能超过 65534？因为端点的编码公式为 `(gen << 15) + slot`，当 `gen = 65535` 且 `slot` 为正值时：

```
(65535 << 15) + slot = 2147516416 + slot
```

这个值超过了 `INT_MAX = 2147483647`，导致有符号整数溢出，产生未定义行为。因此代数必须在 65534 处截断并回绕。

#### 2.4.2 代数回绕

当代数超过 `_ENDPOINT_MAX_GENERATION` 时，回绕为 1 而非 0。为什么不回绕为 0？

因为 `generation = 0` 时，`endpoint = (0 << 15) + slot = slot`，端点值等于槽位号。这个特性被 Minix3 用于硬编码的内核任务端点：

```c
#define KERNEL  (-1)   // endpoint = slot = -1, generation = 0
#define SYSTEM  (-2)   // endpoint = slot = -2, generation = 0
```

如果用户进程的端点代数为 0，其端点值将与内核任务的端点格式相同，可能导致混淆。因此代数从 1 开始，回绕时也回到 1，避免与 generation=0 的硬编码端点冲突。

注意：回绕为 1 存在理论上的 ABA 风险——如果同一槽位经历了 65534 次重用后回到代数 1，恰好有非常旧的端点引用可能再次匹配。但在实践中，65534 次重用几乎不可能在系统运行期间发生。

#### 2.4.3 代数作用

端点代数是 Minix3 微内核容错机制的基石，其作用体现在三个方面：

1. **身份唯一性**：同一槽位的不同进程实例拥有不同的代数，端点 `(gen, slot)` 唯一标识一个进程实例。即使槽位被重用，新旧进程的端点也不同。

2. **自动过期**：当进程退出后，其槽位的代数递增。持有旧端点的通信方发送消息时，内核检测到代数不匹配，返回 `EDEADEPT` 错误，通信方知道对端已失效，可以重新查询。

3. **服务重启安全**：在微内核中，系统服务（如磁盘驱动）可能崩溃并被重启。代数机制确保重启后的服务拥有新端点，旧客户端不会错误地向新服务发送格式不匹配的消息。

关于端点机制的完整分析，参见 [20-endpoint](20-endpoint.md) 和 [endpoint 概念](../../concepts/endpoint.md)。

### 2.5 进程号恢复

进程号恢复对应源码第 71 行：

```c
rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;
```

这条语句将子进程的 `p_nr` 恢复为系统调用参数中指定的槽位号。整体复制 `*rpc = *rpp` 后，`rpc->p_nr` 被覆盖为父进程的进程号，但子进程必须使用自己的槽位号。

#### 2.5.1 p_nr 恢复

`rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot` 将子进程的进程号设置为调用者（PM）指定的槽位号。

`m_ptr->m_lsys_krn_sys_fork.slot` 是 `SYS_FORK` 系统调用消息中的字段，由 PM 在调用内核之前设置。PM 负责在进程表中为子进程分配一个空闲槽位，并将该槽位号通过消息传递给内核。

这个值必须与 `rpc` 指针指向的进程表位置一致——`rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot)`，即 `rpc` 本身就是通过这个槽位号获取的。恢复 `p_nr` 只是为了修正整体复制造成的覆盖，确保子进程的 `p_nr` 字段与其在进程表中的位置一致。

#### 2.5.2 为什么需要恢复

整体复制 `*rpc = *rpp` 后，`rpc->p_nr` 被覆盖为父进程的进程号 `rpp->p_nr`。但子进程必须拥有自己的进程号，原因有二：

1. **进程号是槽位索引**：`p_nr` 是进程在进程表中的位置标识，内核通过 `proc_addr(p_nr)` 定位进程结构体。如果子进程的 `p_nr` 与父进程相同，将导致进程查找混乱。

2. **端点生成依赖进程号**：新端点的计算公式为 `_ENDPOINT(gen, rpc->p_nr)`，如果 `p_nr` 不正确，生成的端点也将错误。

源码注释明确指出：`this was obliterated by copy`——进程号被复制操作"抹除"了，必须恢复。

### 2.6 端点生成

端点生成对应源码第 72 行：

```c
rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);
```

这条语句为子进程生成新的端点标识符，由递增后的代数和子进程的进程号组合而成。这是 fork 中最关键的字段修正操作——子进程必须拥有与父进程不同的端点，否则 IPC 通信将无法区分两个进程。

#### 2.6.1 _ENDPOINT 宏

`_ENDPOINT(gen, rpc->p_nr)` 宏将代数和进程号组合为一个 32 位端点值：

```c
#define _ENDPOINT(g, p) ((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
```

即 `endpoint = (gen << 15) + p_nr`。代数占据高位，进程号占据低位。这种编码方式使得：
- 当 `gen = 0` 时，`endpoint = p_nr`（用于硬编码的内核任务端点）
- 当 `gen > 0` 时，端点值唯一标识一个进程实例
- 可以通过 `_ENDPOINT_G` 和 `_ENDPOINT_P` 宏反向提取代数和进程号

#### 2.6.2 新端点赋值

这条语句将子进程的端点设置为新计算的端点值。整体复制后，`rpc->p_endpoint` 被覆盖为父进程的端点，这是错误的——两个进程不能共享同一个端点。

新端点由两部分组成：
1. **代数 `gen`**：基于子进程槽位的旧代数递增得到，保证了与旧端点的区分
2. **进程号 `rpc->p_nr`**：子进程自己的槽位号（已在前一步恢复）

新端点的代数比子进程槽位之前的代数大 1（或回绕为 1），这意味着所有持有旧端点（代数较小）的引用都将自动失效。这是 Minix3 端点协议的核心安全保证。

#### 2.6.3 端点唯一性

端点唯一性由以下机制保证：

1. **槽位唯一**：每个进程在进程表中占据唯一的槽位，`p_nr` 不会重复
2. **代数递增**：同一槽位每次被重用时，代数递增，新端点与旧端点不同
3. **代数回绕安全**：代数回绕为 1 而非 0，避免与硬编码端点冲突
4. **运行时验证**：内核通过 `isokendpt` / `okendpt` 宏验证端点的有效性，检查消息中的端点是否与进程表中存储的端点匹配

在 fork 场景中，子进程获得的新端点 `(new_gen, child_slot)` 与父进程的端点 `(parent_gen, parent_slot)` 在两个维度上都不同（除非极端巧合），因此保证了唯一性。

更严格地说，端点唯一性是**概率性保证**：在代数回绕前，唯一性是绝对的；回绕后存在理论上的 ABA 风险，但在实践中可忽略。关于端点唯一性的完整讨论，参见 [20-endpoint](20-endpoint.md)。

---

## 3. 复制流程图

本节用流程图展示进程结构复制的完整步骤和各字段的处理方式。

### 3.1 复制步骤

```
进程结构复制流程
│
├── 1. save_fpu(rpp)
│       └── 确保父进程 FPU 状态已保存到内存缓冲区
│
├── 2. gen = _ENDPOINT_G(rpc->p_endpoint)
│       └── 保存子进程槽位的端点代数
│
├── 3. [i386] old_fpu_save_area_p = rpc->p_seg.fpu_state
│       └── 保存子进程的 FPU 缓冲区指针
│
├── 4. *rpc = *rpp
│       └── 整体复制 proc 结构体（浅拷贝）
│
├── 5. [i386] rpc->p_seg.fpu_state = old_fpu_save_area_p
│       └── 恢复子进程的 FPU 缓冲区指针
│
├── 6. [i386] if(proc_used_fpu(rpp))
│       └── memcpy FPU 状态内容到子进程缓冲区
│
├── 7. if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1
│       └── 递增端点代数（回绕保护）
│
├── 8. rpc->p_nr = slot
│       └── 恢复子进程号
│
└── 9. rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr)
        └── 生成新端点
```

### 3.2 字段处理

```
字段处理分类
│
├── 直接继承（保持父进程值）
│   ├── p_priority       调度优先级
│   ├── p_sendmsg        发送消息缓冲区
│   ├── p_delivermsg     投递消息缓冲区
│   └── p_reg            寄存器保存区（除 retreg）
│
├── 必须修正（子进程独立值）
│   ├── p_nr             ← 消息中的 slot
│   ├── p_endpoint       ← _ENDPOINT(gen, p_nr)
│   ├── p_seg.fpu_state  ← 恢复子进程自己的缓冲区指针
│   └── p_reg.retreg     ← 0（后续章节处理）
│
├── 条件复制
│   └── FPU 状态内容     ← 仅当 proc_used_fpu(rpp) 时 memcpy
│
└── 后续章节修正
    ├── p_rts_flags      ← RTS_NO_QUANTUM（18-do-fork-init）
    ├── p_misc_flags     ← 清除定时器标志（17-do-fork-endpoint）
    ├── p_user_time      ← 清零（17-do-fork-endpoint）
    ├── p_name           ← 追加 "*F"（17-do-fork-endpoint）
    └── p_pending        ← 清空（19-do-fork-priv）
```

---

## 4. Rust 设计决策

在 Rust 中实现进程结构复制，需要解决 C 语言中不存在的几个问题：

1. **所有权与借用**：C 的 `*rpc = *rpp` 直接覆盖目标结构体，Rust 中需要考虑所有权转移和借用规则
2. **浅拷贝风险**：C 的结构体赋值是浅拷贝，Rust 的 `Clone` trait 可以实现深拷贝，但需要区分哪些字段应该深拷贝
3. **类型安全**：Rust 的类型系统可以在编译期防止 C 中常见的指针混淆错误
4. **FPU 状态抽象**：不同架构的 FPU 处理方式不同，需要通过 trait 抽象硬件差异

### 4.1 结构体复制

Rust 中结构体复制有三种方式：

**1. `Clone` trait（深拷贝）**
```rust
let child = parent.clone();
```
需要为 `KProcess` 实现 `Clone`，但 `KProcess` 包含 `AtomicU32` 等不可 `Clone` 的字段，需要特殊处理。

**2. 逐字段复制**
```rust
child.p_nr = parent.p_nr;
child.p_endpoint = parent.p_endpoint;
// ...
```
显式列出每个字段，不会遗漏，但代码冗长。

**3. 结构体更新语法**
```rust
let child = KProcess {
    p_nr: new_nr,
    p_endpoint: new_endpoint,
    ..parent
};
```
Rust 的结构体更新语法类似 C 的整体赋值，但只适用于值类型，不适用于包含 `Atomic` 字段的结构体。

由于 `KProcess` 包含原子类型字段，最合适的方式是**逐字段复制**，对原子字段使用 `.load()` / `.store()` 方法。这虽然冗长，但能精确控制每个字段的复制行为，避免浅拷贝风险。

### 4.2 Clone trait

为 `KProcess` 实现 `Clone` trait 需要特殊处理原子类型字段。Rust 的 `AtomicU32` 等类型不实现 `Clone`（因为原子操作的所有权语义不明确），因此需要手动实现：

```rust
impl Clone for KProcess {
    fn clone(&self) -> Self {
        Self {
            p_nr: self.p_nr,
            p_endpoint: self.p_endpoint,
            p_rts_flags: RtsFlags::new(self.p_rts_flags.load()),
            p_misc_flags: MiscFlags::new(self.p_misc_flags.load()),
            p_sched: SchedFields {
                priority: AtomicI8::new(self.p_sched.priority.load()),
                quantum: Quantum::new(self.p_sched.quantum.size_ms.load()),
                cpu: AtomicU32::new(self.p_sched.cpu.load()),
            },
            // ... 其他字段
        }
    }
}
```

但直接 `Clone` 会复制所有字段，包括 `p_nr` 和 `p_endpoint`，然后还需要修正。更好的方式是提供一个专用的 `fork_from` 方法，在复制的同时完成字段修正，避免"先复制、后修正"的两步操作。

### 4.3 FPU 状态抽象

FPU 状态的抽象需要遵循硬件抽象原则（参见 [RECONSTRUCTION-PRINCIPLES](../../RECONSTRUCTION-PRINCIPLES.md)）：**我们不描述硬件，我们只抽象机制**。

在 Minix3 的 C 代码中，FPU 状态通过 `char *fpu_state` 指针和条件编译处理架构差异。在 Rust 重构中，应通过 trait 抽象 FPU 机制：

```rust
/// FPU 上下文管理 trait
///
/// 各架构需要实现此 trait，提供 FPU 状态的保存、恢复和复制操作。
pub trait FpuContext {
    /// FPU 状态缓冲区类型
    type StateBuffer;

    /// 保存当前 FPU 状态到缓冲区
    fn save(&self, buf: &mut Self::StateBuffer);

    /// 从缓冲区恢复 FPU 状态
    fn restore(&self, buf: &Self::StateBuffer);

    /// 复制 FPU 状态（用于 fork）
    fn copy_state(src: &Self::StateBuffer, dst: &mut Self::StateBuffer);

    /// 检查进程是否使用过 FPU
    fn is_used(flags: u32) -> bool;
}
```

Mock 实现使用简单的字节数组：

```rust
pub struct MockFpuContext;

impl FpuContext for MockFpuContext {
    type StateBuffer = [u8; 512]; // 模拟 FPU 状态大小
    // ...
}
```

这样，`KProcess` 中的 FPU 状态字段不再是 `char *` 指针，而是泛型关联类型，由架构实现决定具体形式。fork 时的 FPU 复制操作通过 trait 方法完成，无需条件编译。

---

## 5. 实现

本节给出进程结构复制和端点生成的 Rust 实现，包括 `fork_from` 方法、端点代数递增逻辑和单元测试。

### 5.1 进程复制方法

```rust
impl KProcess {
    /// 从父进程复制创建子进程（fork）
    ///
    /// 对应 Minix3 do_fork.c 中的 *rpc = *rpp 及后续字段修正。
    /// 不复制 p_nr 和 p_endpoint，由调用者通过参数指定。
    ///
    /// # 参数
    /// - `parent`: 父进程的引用
    /// - `child_nr`: 子进程的进程号（槽位号）
    /// - `child_endpoint`: 子进程的新端点
    pub fn fork_from(parent: &KProcess, child_nr: ProcNr, child_endpoint: Endpoint) -> Self {
        Self {
            p_nr: child_nr,
            p_endpoint: child_endpoint,
            p_rts_flags: RtsFlags::new(parent.p_rts_flags.load()),
            p_misc_flags: MiscFlags::new(parent.p_misc_flags.load()),
            p_sched: SchedFields {
                priority: AtomicI8::new(parent.p_sched.priority.load()),
                quantum: Quantum::new(parent.p_sched.quantum.size_ms.load()),
                cpu: AtomicU32::new(parent.p_sched.cpu.load()),
            },
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            p_getfrom_e: parent.p_getfrom_e,
            p_sendto_e: parent.p_sendto_e,
            p_pending: SigSet::empty(),
            p_name: parent.p_name,
            p_sendmsg: parent.p_sendmsg.clone(),
            p_delivermsg: parent.p_delivermsg.clone(),
            p_delivermsg_vir: parent.p_delivermsg_vir,
        }
    }
}
```

注意：
- `p_nr` 和 `p_endpoint` 直接使用参数值，不从父进程复制
- `p_accounting`、`p_time`、`p_cycles` 初始化为新值（非复制），对应 Minix3 的清零操作
- `p_pending` 初始化为空，对应 Minix3 的 `sigemptyset`
- `p_nextready`、`p_caller_q`、`p_q_link` 初始化为 None，子进程不在任何队列中

### 5.2 端点生成方法

```rust
impl Endpoint {
    /// 为 fork 生成子进程的新端点
    ///
    /// 对应 Minix3 do_fork.c 中的代数递增和端点生成逻辑：
    /// ```c
    /// gen = _ENDPOINT_G(rpc->p_endpoint);
    /// if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;
    /// rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);
    /// ```
    ///
    /// # 参数
    /// - `current_endpoint`: 子进程槽位当前的端点（用于提取旧代数）
    /// - `child_slot`: 子进程的进程号
    ///
    /// # 返回值
    /// 新端点，代数基于旧代数递增
    pub fn fork_new_endpoint(current_endpoint: Endpoint, child_slot: ProcNr) -> Endpoint {
        const ENDPOINT_MAX_GENERATION: i32 = i32::MAX / ENDPOINT_GENERATION_SIZE - 1;
        let mut gen = current_endpoint.generation();
        gen += 1;
        if gen >= ENDPOINT_MAX_GENERATION {
            gen = 1;
        }
        Endpoint::from_generation_slot(gen, child_slot)
    }
}
```

这个方法将 Minix3 中的三步操作（提取代数、递增、构造端点）封装为一个纯函数，逻辑清晰且易于测试。

### 5.3 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fork_new_endpoint_increment() {
        // 代数递增测试
        let current = Endpoint::from_generation_slot(5, 10);
        let new = Endpoint::fork_new_endpoint(current, 10);
        assert_eq!(new.generation(), 6);
        assert_eq!(new.slot(), 10);
    }

    #[test]
    fn test_fork_new_endpoint_wraparound() {
        // 代数回绕测试
        let max_gen = i32::MAX / ENDPOINT_GENERATION_SIZE - 1;
        let current = Endpoint::from_generation_slot(max_gen, 10);
        let new = Endpoint::fork_new_endpoint(current, 10);
        assert_eq!(new.generation(), 1);
        assert_eq!(new.slot(), 10);
    }

    #[test]
    fn test_fork_new_endpoint_different_slot() {
        // 不同槽位测试
        let current = Endpoint::from_generation_slot(3, 5);
        let new = Endpoint::fork_new_endpoint(current, 20);
        assert_eq!(new.generation(), 4);
        assert_eq!(new.slot(), 20);
    }

    #[test]
    fn test_fork_from_basic() {
        // 基本复制测试
        let parent = KProcess::new(5, Endpoint::from_generation_slot(3, 5));
        parent.p_rts_flags.clear(rts::SLOT_FREE);
        parent.set_priority(priority::USER_Q);

        let child_endpoint = Endpoint::fork_new_endpoint(
            Endpoint::from_generation_slot(0, 10), 10
        );
        let child = KProcess::fork_from(&parent, 10, child_endpoint);

        // 子进程使用自己的进程号和端点
        assert_eq!(child.p_nr, 10);
        assert_eq!(child.p_endpoint, child_endpoint);

        // 子进程继承父进程的调度优先级
        assert_eq!(child.get_priority(), parent.get_priority());

        // 子进程的时间统计从零开始
        assert_eq!(child.p_time.user_time.load(Ordering::Relaxed), 0);
        assert_eq!(child.p_time.sys_time.load(Ordering::Relaxed), 0);

        // 子进程的信号集为空
        assert!(child.p_pending.is_empty());
    }

    #[test]
    fn test_fork_from_accounting_reset() {
        // 会计信息重置测试
        let parent = KProcess::new(5, Endpoint(5));
        parent.p_accounting.record_ipc_sync();
        parent.p_accounting.record_ipc_sync();

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        // 子进程的会计信息从零开始
        assert_eq!(child.p_accounting.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_fork_from_independent_queues() {
        // 队列独立性测试
        let parent = KProcess::new(5, Endpoint(5));
        parent.p_nextready = Some(3);
        parent.p_caller_q = Some(7);

        let child = KProcess::fork_from(&parent, 10, Endpoint::from_generation_slot(1, 10));

        // 子进程不在任何队列中
        assert_eq!(child.p_nextready, None);
        assert_eq!(child.p_caller_q, None);
        assert_eq!(child.p_q_link, None);
    }
}
```

---

## 6. 参见

- [15-do-fork-validate](15-do-fork-validate.md) - 参数验证
- [17-do-fork-endpoint](17-do-fork-endpoint.md) - 端点生成
- [20-endpoint](20-endpoint.md) - 端点机制
