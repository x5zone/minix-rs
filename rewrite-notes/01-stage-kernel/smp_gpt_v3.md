这版我觉得已经**非常接近真正可以冻结**了，但这次我发现了一个比上一轮任何问题都更值得停下来确认的点：

> **你这次为了解决 mailbox handshake，把 C 的“早期启动握手”和“AP 已经 online”合并成了同一个 `online_mask`。这实际上又把两个原本刻意分开的状态合并了。**

所以我现在的结论是：

**大框架不用动，但 S-3/S-5 这里还有一个 P0 级语义问题，建议让 IDE 查清楚再冻结。**

---

# 1. 目前最重要的问题：`online_mask` 被用错地方了

你现在的设计是：

```text
S-3d:
AP early entry
    ↓
online_mask.fetch_or(AP bit)
    ↓
park
```

而 S-5 又规定：

```text
BSP 等 online bit
    ↓
认为 AP 已消费 bootstrap
    ↓
置 CpuFlags.READY
```

文档同时又明确描述你的 CPU 状态：

```text
DISCOVERED
  ↓
BOOTING
  ↓
READY       ← BSP 根据 early handshake 代置
  ↓
ONLINE      ← AP 完成 init_ap 后自报
  ↓
scheduler
```



这三件东西其实已经**互相矛盾**了。

因为你自己在同一份文档里还写着：

> C 的 `CPU_IS_READY` 是 BSP 在 AP 启动握手后代置，而 `ap_cpus_booted` 是 AP 在 `ap_boot_finished()` 时自报。

也就是说 C 的真实时间轴是：

```text
BSP:
    mailbox
      ↓
    INIT/SIPI
      ↓
    AP 消费 mailbox
      ↓
    handshake ack
      ↓
    BSP 设置 CPU_IS_READY
```

然后更晚：

```text
AP:
    init_ap()
      ↓
    ap_boot_finished()
      ↓
    ap_cpus_booted++
      ↓
    scheduler
```

而你的 Rust 现在变成：

```text
AP:
    early entry
      ↓
    online_mask |= bit
      ↓
    park / init_ap
```

这就意味着：

### `online_mask` 不再表示 ONLINE

而是：

> “AP 至少已经成功进入 early entry，并消费了 bootstrap”。

这是一个完全不同的语义。

---

## 这不是纯术语问题，而是会真的造成错误

例如你现在 S-3d：

```text
AP1:
    online_mask |= bit1
    park

BSP:
    看见 bit1
    → READY
```

但 AP1 根本还没有：

```text
GDT
TSS
GS_BASE
timer
init_ap
```

甚至还没进入 Rust 正常 AP 环境。

于是：

```text
READY
```

在你的状态机里实际上变成了：

```text
early-entry alive
```

而不是：

```text
arch init complete
```

更严重的是 S-5 后：

```text
wait_for_aps()
```

如果它依据这个 `online_mask` 判断完成，那么可以出现：

```text
所有 AP 都已经置 online bit
但所有 AP 都还 parked / 没 init_ap
BSP 却认为 SMP bring-up 完成
```

**这是实际的 bring-up 逻辑 bug。**

---

# 2. 我建议不要再强行“一个变量解决两种握手”

上一轮我自己也建议过把计数器换成 bitmask，但现在看得更清楚了：

**应该是两个 bitmask，而不是一个。**

例如：

```text
boot_ack_mask
    = AP 已经消费 bootstrap / 到达 early entry

online_mask
    = AP 完成 init_ap / 正式进入 online 状态
```

时间轴：

```text
                 AP
                  │
              early entry
                  │
        boot_ack_mask |= bit
                  │
                  │
              init_ap
                  │
         online_mask |= bit
                  │
             scheduler
```

这样就非常干净：

```text
boot_ack_mask
    ↓
解决单 mailbox 串行复用 + 5 秒 timeout

CpuFlags.READY
    ↓
BSP 确认该 AP 完成启动阶段

online_mask
    ↓
AP 真正完成 init_ap，可以参与正常 SMP
```

而且你马上会发现一个好处：

**S-3d 还是可以很好测。**

```text
L2:
    boot_ack_mask & AP_BIT != 0
```

而不是污染正式 `online_mask`。

S-4：

```text
init_ap()
    ↓
online_mask |= bit
```

S-5：

```text
online_mask == expected_mask
```

这样状态机就真正和名字一致了。

---

# 3. 这甚至比“忠实 C”更合理

C 本身就已经有两个不同信号：

```text
ap_cpu_ready
ap_cpus_booted
```

你现在为了 Rust 类型安全把：

```text
ap_cpu_ready
+
ap_cpus_booted
```

合并成一个 `online_mask`。

这其实是反过来破坏了你已经识别出来的 C 语义结构。

所以我建议：

```text
ApBootstrap {
    ...
    boot_ack_mask_pa,
    online_mask_pa,
}
```

或者甚至：

```text
ApBootstrap {
    ...
    boot_ack_pa,
    ...
}
```

具体是不是 mask，可以让 IDE 根据现有 `CpuFlags` / `SmpState` 结构决定。

**核心不是“两变量必须是 mask”，而是“两种生命周期必须有独立状态”。**

---

# 4. 第二个很明显的问题：文档里 `ap_early_entry` 的签名还没完全同步

前面还写：

```rust
unsafe fn ap_early_entry(ctx: &ApBootstrap) -> !
```



后面已经正式决定：

```text
unsafe fn ap_early_entry(bootstrap_pa: usize)
```

并且 PA → direct map → `&ApBootstrap`。



所以这里现在是一个非常明确的**文档内部矛盾**。

而且不是只有那一行，§3.2 前面的概念说明也还在说：

> “三架构汇合点是同一个 Rust 函数 `ap_early_entry(ctx: &ApBootstrap)`”

建议直接改成最终模型：

```rust
unsafe fn ap_early_entry(bootstrap_pa: usize) -> !
```

然后明确：

```text
assembly boundary:
    PA

Rust boundary:
    PA
        ↓
    direct-map VA
        ↓
    &ApBootstrap
```

这样文档不会再让 reviewer 以为 assembly → Rust 已经完成了 pointer conversion。

---

# 5. `ApBootstrap` 里的 PA/VA 语义也有一处旧文字没完全清干净

现在结构体：

```text
page_table_root_pa
kernel_stack_top_va
rust_entry_va
online_mask_pa
```



这是好的。

但后面还写：

> `ApEntry / Bootstrap / PageTableRoot / KernelStackTop` 在 MMU 开启以前一律是物理地址。

这里 `KernelStackTop` 已经不是 PA 了。

应该改成类似：

```text
MMU-off only:
    bootstrap_pa
    page_table_root_pa
    boot-time stack PA

MMU-on only:
    kernel_stack_top_va
    rust_entry_va
```

否则下一轮 IDE 又会把这个当成 P0 地址空间问题来追。

---

# 6. 我现在反而最担心的是 `.ap_early_entry` / `.ap_early_entry_data` 两个 section 的关系

你已经把它们拆成：

```text
.ap_early_entry
.ap_early_entry_data
```

代码和数据分离是合理的。

但是现在有一个具体实现问题还没写：

> **early-entry code 如何在完全 relocation-free 的情况下找到 `.ap_early_entry_data`？**

如果：

```asm
lea rax, ap_early_entry_data
```

这种方式跨 section 引用，很容易又产生 relocation。

如果：

```asm
runtime_pc + delta
```

那就必须保证：

```text
code/data 的 link-time relative layout
=
runtime copied layout
```

如果 data 是跟 code 一起整体搬运，则又涉及：

```text
section ordering
alignment
padding
copy range
```

所以这个我非常建议让 IDE 查。

---

# 7. `readelf -r == 0` 其实还不够

你这次已经加了：

```text
readelf -r
objdump -d
objcopy
拷贝执行验证
```

这是对的。

而且我刚核了当前 Rust 官方文档：`global_asm!` 确实支持 `const` operand；`const` 表达式要求是整数常量表达式，所以你用 `offset_of!` 生成字段 offset 的方向在语言层面是成立的。([Rust 文档][1])

但：

```text
relocation == 0
```

并不等于：

```text
position-independent
```

真正关键的仍然是：

> **把 image 从 link address 复制到另一个完全不同的物理地址，然后实际执行。**

你现在已经写进 spike 验收了，所以没问题。

只是建议把这个原则直接写成：

```text
ELF relocation-free ≠ PIC proof
```

最终证据是：

```text
same machine code
different runtime base
execution succeeds
```

这样会更严谨。

---

# 8. linker 保留机制也值得让 IDE 查一下

你说：

> 拷贝边界由链接器段符号界定。

但我没看到你明确说：

```text
KEEP(*(.ap_early_entry))
KEEP(*(.ap_early_entry_data))
```

或者等价机制。

如果 section 不是被普通代码符号引用，而只是通过 linker symbols / runtime address 使用，linker GC 有没有可能把它丢掉？

这应该让 IDE 根据**当前实际 linker script**回答，而不是假定。

---

# 9. S-4 timer 这一块现在基本舒服了

这里已经修得很好：

```text
x86:
    8254 PIT = system-wide
    no per-AP init

ARM/RISC-V:
    per-CPU comparator

IRQ 到达需要：
    source
    controller
    CPU interrupt enable
    trap entry
```



而且你还明确把：

```text
IDT load()
```

放到 S-8。

这个逻辑现在是闭合的。

---

# 10. S-7 的安全论证现在也足够了，但最好改成“按架构描述”

目前文档拿：

> “无 IDT 在位”

作为三重证据的一部分。

对于 x86 没问题。

但 AArch64 没有 IDT，RISC-V 也没有 IDT。

所以这里最好把：

```text
无 IDT
```

改成：

```text
AP 尚未开启任何可达的异常/IRQ 路径
```

然后分别列：

```text
x86:
    lidt 未执行 + IF disabled

ARM:
    DAIF masked / GIC interrupt path disabled

RISC-V:
    sie/sstatus.SIE disabled
```

这样三架构才是同一个抽象下的证明。

---

# 11. 另外一个值得让 IDE 查的问题：AP 的 `GS_BASE` 和 `swapgs` 顺序

你已经很好地发现：

```text
IA32_GS_BASE
IA32_KERNEL_GS_BASE
```

都需要处理。

但是我建议让 IDE 再查一个非常具体的问题：

> 当前项目的 syscall/trap entry 到底是谁负责第一次 `swapgs`，以及 `IA32_KERNEL_GS_BASE` 在 AP 第一次进入异常前是否必须已经有效？

因为你现在设计里：

```text
S-4:
    写两个 GS_BASE

S-8:
    syscall/trap stub
```

理论上这是正确依赖，但最好有源码级证明：

```text
AP init_ap 完成
    ↓
任何可能触发 swapgs 的路径
```

中间没有遗漏。

---

# 我现在建议你让 IDE 下一版只回答这 7 个问题

这次不用再来一大堆问题了。真正剩下的就是这些：

### 最后一轮源码核实问题

1. **boot handshake 与 online 状态是否应当分离？【P0】**

   当前设计让 `online_mask` 在 `ap_early_entry` 阶段置位，用它同时承担：

   * x86 单 mailbox 的“AP 已消费 bootstrap”握手；
   * CPU ONLINE 状态；
   * S-3d AP-alive 测试；
   * S-5 `online_mask == expected_mask` 的最终上线判断。

   但 C 的实际语义是两个阶段：
   `ap_cpu_ready` = AP 已消费 bootstrap / early startup handshake；
   `ap_cpus_booted` = AP 完成 `init_ap` 后正式报到。

   请逐行核实 C 与现有 Rust 调用顺序，并回答是否应该保留两个独立状态，例如：
   `boot_ack_mask`（early entry 握手）与 `online_mask`（init_ap 完成后置位）。

   特别确认：如果当前方案维持一个 `online_mask`，是否可能出现“AP 已置 online bit、但尚未执行 init_ap，BSP 已认为该 CPU READY/ONLINE”的语义错误？

2. **`ap_early_entry` 最终 ABI 是否完全确定？**

   当前正文前半仍出现：
   `unsafe fn ap_early_entry(&ApBootstrap) -> !`

   后文已经决定：
   `unsafe fn ap_early_entry(bootstrap_pa: usize) -> !`

   请确认最终选择，并沿代码路径说明：
   `bootstrap_pa → direct-map VA → &ApBootstrap`
   的实际发生位置。

   同时确认 assembly → Rust 边界是否应该永远传 PA，而不传 Rust reference。

3. **`.ap_early_entry` 与 `.ap_early_entry_data` 如何在 relocation-free 条件下互相定位？**

   请检查实际计划中的 assembly：

   * code 如何获得 data/mailbox 的运行期地址？
   * 是否存在跨 section symbol reference？
   * `readelf -r == 0` 是否足以证明这一点？
   * 如果使用 runtime-relative 计算，请给出具体实现方式及其不变量。

   最终需要保证整个 image 被复制到任意合法低地址后仍能正确找到 mailbox。

4. **linker 是否保证 early-entry image 不被 GC 丢弃？**

   请检查当前实际 linker script / link arguments：

   * `.ap_early_entry` 是否被 `KEEP()` 或等价机制保留？
   * `.ap_early_entry_data` 是否同样保留？
   * start/end symbols 是否覆盖预期内容与 padding？
   * runtime copy size / alignment 如何得到？

   不要依据设计意图回答，以实际 linker 配置为准。

5. **MMU 切换点的 PA/VA 不变量最终版是什么？**

   当前 `ApBootstrap` 已包含：
   `page_table_root_pa`、`kernel_stack_top_va`、`rust_entry_va`、`online_mask_pa`。

   请分别给出 x86/aarch64/riscv64 的：

   * MMU-off 可直接使用的物理地址；
   * MMU-on 后才可使用的虚拟地址；
   * 当前 SP 在 MMU enable 前后分别是什么地址；
   * 什么时候切换到 `kernel_stack_top_va`；
   * `bootstrap_pa` 在 MMU on 后如何通过恒等/direct-map 继续访问。

   同时确认文档不再把 `kernel_stack_top_va` 写成 MMU-off PA。

6. **S-7 的“trap 不可达”证明是否三架构都成立？**

   当前文档以“无 IDT 在位”作为证据之一。
   请分别检查 x86_64 / aarch64 / riscv64，在 S-7 AP scheduler 阶段：

   * interrupt mask 是否仍然关闭；
   * timer 是否可能已经 enabled；
   * external/software interrupt 是否可能到达；
   * vector/base 是否已经安装；
   * 是否可能进入 user mode。

   最终把证明写成“三架构都没有可达的异常/IRQ 路径”，不要使用 x86-only 的“无 IDT”作为统一表述。

7. **AP 第一次执行 syscall/trap 前的 GS_BASE 不变量**

   请沿当前真实代码路径检查：

   * `IA32_GS_BASE` 与 `IA32_KERNEL_GS_BASE` 在 AP 哪一步写入；
   * 第一次 `swapgs` 在哪条入口路径发生；
   * 第一次 syscall / exception / IRQ 到达前是否一定已经完成 GS 状态初始化；
   * 是否存在 BSP 已配置、AP 未配置的 per-CPU MSR / TSS / stack 状态。

   输出最终的 BSP-only / per-CPU 清单即可。

---

# 最后的判断

这一次我反而不会再建议你继续扩展文档。

**现在真正需要处理的是一个语义 bug，而不是设计不足：**

```text
               C
        ┌─────────────────┐
        │ bootstrap ack   │
        │      ↓          │
        │ CPU_IS_READY    │
        │      ↓          │
        │ init_ap         │
        │      ↓          │
        │ ap_cpus_booted  │
        └─────────────────┘

               Rust
        目前却变成：

        online_mask
             ↑
        early entry
             ↑
        init_ap
```

我建议把这条线重新拆开。

**这是我现在唯一会阻止“冻结”的问题。**

除此之外，`global_asm!` 方案我已经没有原则性异议了；当前 Rust 官方文档确认 `global_asm!` 是模块级汇编，并且允许 `const` operand，你们用 `offset_of!` 注入 ABI offset 的方案在语言机制上是成立的。([Rust 文档][1])

等 IDE 回答上面 7 个问题，尤其是 **#1**。如果 #1 确认需要拆 `boot_ack_mask / online_mask`，我认为改完之后这份 SMP 设计就真的到了可以冻结的程度。

[1]: https://doc.rust-lang.org/reference/inline-assembly.html?highlight=inlin&utm_source=chatgpt.com "Inline assembly - The Rust Reference"
