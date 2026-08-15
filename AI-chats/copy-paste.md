现在把**完整材料**看完后，我会修正我上一轮的判断：

> **是，V3 的“设计决策”已经收敛；但 V3 文档本身还存在一个非常明确的内部矛盾。**
>
> 这个矛盾不是要重新选方案，而是 **“最终方案已经是 A，却 checklist 里还残留着 B 的落地形态”**。
>
> 把这个清掉之后，我认为就可以真正 freeze，进入代码实现。

而且完整看完以后，我反而更确定：**你现在的最终方案是对的。**

---

# 一、先给最终结论：现在不要再改架构方向

完整材料把整个推理链补齐后，现在的事实非常清楚：

### 已确认

```text
ArchBoot
    ↓
是错误的 abstraction boundary
```

原因不是简单的“方法太多”，而是：

> `boot` 是时序，不是 capability。

你现在的元原则：

> **trait 成员由 architecture variance 决定，不由调用时序决定**

我认为可以正式冻结。

---

### `enable/disable_timer_irq`

已经不是未来假设，而是**当前代码就存在真实三架构差异**：

* x86 → LAPIC LVT
* ARM → CNTP
* RISC-V → `sie.STIE`

所以：

```rust
pub trait TimerIrqGate {
    fn enable_timer_irq();
    fn disable_timer_irq();
}
```

**必须保留。**

这一点 V3 已经有代码事实支撑，不再是设计推测。

---

### `register_timer_handler`

这里现在也终于真正收敛：

```text
当前：
    mock 占位
    无真实 reader
    无真实 dispatch
    kernel 已 deferred

未来：
    可能存在架构差异
    但 abstraction ownership 尚未确定
```

因此：

> **现在删除，而不是现在为未来建 trait。**

这个判断我现在明确支持。

尤其完整材料里已经证明真实 dispatch 未来走：

```text
IrqManager::register_hook
```

而不是原来的 `ArchBoot::register_timer_handler`。

所以现在：

```text
register_timer_handler
```

**既不是 free function，也不是 `TimerHandlerRegistry`，就是删除。**

未来真有需求，再从真实调用路径重新建模。

这比之前的“下沉 free function”更干净。

---

# 二、真正剩下的唯一设计问题：`CurrentTimerIrqGate`

这是完整材料让我现在可以**非常确定**的一点。

V3 已经事实核验出：

> 项目现有真实模式不是 `Arch` associated types，而是 **per-arch ZST + `lib.rs` 的 `Current*` cfg alias**。

而 V3 自己也已经决定：

```rust
pub struct X86_64TimerIrqGate;
impl TimerIrqGate for X86_64TimerIrqGate { ... }
```

所以最终调用：

```rust
CurrentTimerIrqGate
```

其实**是正确的**。

我上一轮建议删掉 `CurrentTimerIrqGate`，是基于你之前给我的“`Arch` associated type 已存在”这个前提。

现在完整事实告诉我们：

> **这个前提是错的。**

所以我撤回上一轮对这一点的批评。

---

# 三、因此最终调用点应该就是：

```rust
<CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq();
```

而不是：

```rust
<CurrentArch as TimerIrqGate>::enable_timer_irq();
```

也不是：

```rust
<CurrentArch::TimerIrqGate as TimerIrqGate>::enable_timer_irq();
```

这是现在最符合项目既有架构风格的。

因为项目实际已经有：

```text
CurrentClockArch
CurrentArchInit
...
```

V2 的事实核验已经确认这一点。

所以：

```text
per-arch ZST
      ↓
trait capability
      ↓
CurrentXxx cfg alias
      ↓
kernel UFCS
```

就是你们现在的 architecture dispatch idiom。

这也完全符合用户最开始的要求：

> kernel 不知道 x86/ARM/RISC-V；cfg 只存在于 `Current*` 选择点。

---

# 四、但现在 V3 有一个“文档内部矛盾”，必须修

你现在 V3 一方面明确写：

> `CurrentTimerIrqGate` cfg alias 三分支。

另一方面元原则又说：

> 不要 cfg。

严格来说这两个东西**不矛盾**，但文档措辞很容易让人误解。

应该明确区分：

```text
❌ kernel-level architecture selection

#[cfg(target_arch = "...")]
enable_x86();
```

和：

```text
✅ arch crate 的 Current* type selection

#[cfg(target_arch = "...")]
pub type CurrentTimerIrqGate = ...
```

你们其实已经有这个 pattern。

所以我建议在 V3 里把：

> “不要 cfg”

改成：

> **“不在 capability 使用方进行 target-specific 条件分支；target-specific `cfg` 仅允许存在于 `Current*` 选择 alias 等架构边界处。”**

这样就完全没有歧义了。

---

# 五、还有一个非常重要的地方：你 V3 的“元原则 4”现在写得过头了

现在是：

> **真硬件实现必须包含使 IRQ 端到端可达的全部寄存器写入——aarch64 enable_timer_irq 必须同时写 CNTP + GIC（ICC_IGRPEN1_EL1）。** 

这里我建议**必须改**。

因为你同一份 V3 前面又说：

> Step 0.5b 要先核验 `SmpArch::init_bsp` / `init_ap`，确认 GIC 是否已经 enable。

那么逻辑上：

```text
如果 GIC 已由 BSP init enable
    ↓
TimerIrqGate 不需要写 ICC_IGRPEN1_EL1

如果 GIC 没 enable
    ↓
才需要决定谁负责 enable
```

所以不能在事实核验之前把：

```text
TimerIrqGate::enable_timer_irq
    必须写 ICC_IGRPEN1_EL1
```

写死成架构原则。

---

## 应该改成：

> **真硬件 timer IRQ 路径必须满足端到端可达；具体寄存器由哪个初始化阶段/能力负责，必须与实际初始化时序一致。**

然后：

```text
Step 0.5b
    ↓
事实核验
    ↓
┌───────────────┬────────────────┐
│ GIC 已 enable │ GIC 未 enable  │
│               │                │
│ 修注释        │ 决定责任归属   │
│               │ + 必要硬件修复 │
└───────────────┴────────────────┘
```

这样你就不会在“事实核验”之前偷偷把设计结论塞进原则。

---

# 六、同理，Step 0.5b 的结论也不要预设“必须加 GIC 到 TimerIrqGate”

你现在：

> 若 GIC 未 enable → `enable_timer_irq` 新增 `ICC_IGRPEN1_EL1`。

我会再弱化一层。

正确的问题其实不是：

> “谁来写 `ICC_IGRPEN1_EL1`？”

而是：

> **“BSP timer IRQ 开启之前，GIC 的 delivery path 是否已经成立？”**

如果没成立，再问：

> **“哪个已有 initialization capability 才拥有 GIC global enable 的职责？”**

很可能答案不是 `TimerIrqGate`。

这和你前面自己发现的 x86：

```text
SVR Enable
vs
LVT Timer Mask
```

是完全同一个设计原则。

不要把：

```text
“这个寄存器最终必须被写”
```

等价成：

```text
“这个 trait 必须负责写这个寄存器”
```

这正是这次 `ArchBoot` review 应该得到的高级经验。

---

# 七、x86 Follow-up 也应该稍微调整措辞

你现在写：

> 当前合并实现合理，因为 `boot_init_timer` 集中处理 boot 早期。

我觉得这句话不够稳。

因为你现在已经准备删除：

```text
boot_init_timer
```

所以不能拿一个即将被删除的函数作为“职责合理性”的依据。

建议改成：

> 当前实现暂时保留合并语义，不在本次重构中拆分 LAPIC 全局 enable 与 timer LVT unmask；其职责边界作为 follow-up，由后续 LAPIC initialization 时序设计统一处理。

这样不会把临时实现合理化。

---

# 八、`register_timer_handler` 的 TODO 放在 `timer_irq_gate.rs`：我仍然建议拿掉

完整材料里现在：

```rust
// TODO(FutureHWBinding): 原 ArchBoot::register_timer_handler 已删除...
// 未来决定去 ClockArch / TimerHandlerDispatch / free fn
```

放在：

```text
arch/timer_irq_gate.rs
```

顶部。

我仍然认为这是**不理想但不阻塞**的。

因为未来它：

```text
ClockArch
TimerHandlerDispatch
free fn
```

都有可能。

那么：

```text
timer_irq_gate.rs
```

不是它的稳定归属。

### 更好的位置：

**设计 doc。**

代码里只需要：

```rust
//! Timer IRQ hardware gate.
//!
//! Architecture-specific enable/disable of timer IRQ delivery.
```

而：

```text
“原 register 为什么删”
“未来可能去哪里”
```

留在：

```text
05-clock-interrupt-init.md
```

即可。

如果你特别希望代码里留下 breadcrumb，我建议只留一句：

```rust
// Timer handler registration is deferred to the real interrupt-dispatch path.
// See 05-clock-interrupt-init.md §4.7.1.
```

不要在代码里列三个未来候选。

这样生产代码更干净。

---

# 九、Mock 这一轮也终于收敛了：不要建 `MockTimerIrqGate`

V3 §2.2 目前写：

> `test_mock_enable_disable_timer_irq` 移到 `timer_irq_gate.rs::tests`，测试 `MockTimerIrqGate` 的 enable/disable。

但你自己的 V3 测试原则又说：

> 不为不存在的契约写测试。

我建议：

> **直接删掉这个测试。**

因为它的实际内容大概率就是：

```text
调用 enable
→ bool = true

调用 disable
→ bool = false
```

它并没有测试：

```text
TimerIrqGate contract
```

而只是测试 mock。

更重要的是，V3 已经明确：

> 真硬件行为由 QEMU 验证。

所以最终：

```text
MockArchBoot
    ❌

MockTimerIrqGate
    ❌

compile coverage
    ✅

QEMU hardware behavior
    ✅
```

我认为这是最一致的。

---

# 十、这里反而是 V3 最漂亮的一处：测试策略已经可以极简

最终我会建议：

### Arch crate

```text
test_current_timer_irq_gate_compiles
test_x86_64_timer_irq_gate_compiles
test_aarch64_timer_irq_gate_compiles
test_riscv64_timer_irq_gate_compiles
```

保留。

### 删除

```text
test_mock_register_timer_handler
test_mock_enable_disable_timer_irq
test_boot_init_timer_runs_both
test_boot_error_variants
test_register_timer_handler_idempotent_*
```

全部删。

### Integration

```text
QEMU:
timer setup
  ↓
timer IRQ
  ↓
interrupt dispatch
  ↓
handler
  ↓
clock tick
```

这就非常干净。

---

# 十一、`BootError` / `TimerHandlerFn` 全删，我现在完全同意

这个在 V3 已经处理得很好：

```text
ArchBoot
BootError
TimerHandlerFn
boot_init_timer
dummy_timer_handler
MOCK_*
```

作为一个旧设计簇一起清理。

而不是：

```text
ArchBoot 删除
↓
为了 compatibility
保留 TimerHandlerFn
保留 BootError
保留 register free fn
```

这样会留下半死不活的 API。

V3 的 13 项删除清单非常好。

---

# 十二、V3 现在还有一个“事实核验”和“实施”之间的边界问题

Step 0.5 / 0.5b 现在是：

```text
Step 0.5
LAPIC

Step 0.5b
GIC
```

我赞成，而且应该保留为 blocking gate。

但是建议明确：

> **这两个步骤不是设计的一部分，而是 implementation precondition。**

也就是说：

```text
Architecture decision
    ↓
已经 freeze
    ↓
Implementation precondition
    ↓
LAPIC/GIC factual verification
    ↓
implementation details
```

这样未来读文档的人不会误以为：

> “TimerIrqGate 设计还没决定。”

实际上：

> **TimerIrqGate 已经决定；只是硬件实现细节还要根据现有初始化时序确定。**

---

# 十三、因此我现在会把最终 architecture 写成这样

这才是我看完整材料之后最认可的版本：

```text
                         Current* aliases
                              │
               ┌──────────────┼──────────────┐
               │              │              │
               ▼              ▼              ▼
         CurrentClockArch  CurrentArchInit  CurrentTimerIrqGate
               │                              │
               ▼                              ▼
          ClockArch                       TimerIrqGate
                                              │
                              ┌───────────────┼───────────────┐
                              ▼               ▼               ▼
                            x86              ARM             RISC-V
                           LAPIC             CNTP             sie
```

注意：

```text
register_timer_handler
```

**不在这个图里。**

因为现在它已经不是一个存在的 capability。

未来如果真的需要：

```text
real timer dispatch binding
```

再重新画它。

这其实是非常健康的架构演进方式。

---

# 十四、我会把最终代码模型定为

```rust
// arch/timer_irq_gate.rs

pub trait TimerIrqGate: Sized {
    fn enable_timer_irq();
    fn disable_timer_irq();
}
```

```rust
// arch/x86_64/timer_irq_gate.rs

pub struct X86_64TimerIrqGate;

impl TimerIrqGate for X86_64TimerIrqGate {
    fn enable_timer_irq() {
        // actual LAPIC programming
    }

    fn disable_timer_irq() {
        // actual LAPIC programming
    }
}
```

ARM / RISC-V 同理。

然后：

```rust
// arch/lib.rs

#[cfg(target_arch = "x86_64")]
pub type CurrentTimerIrqGate = x86_64::timer_irq_gate::X86_64TimerIrqGate;

#[cfg(target_arch = "aarch64")]
pub type CurrentTimerIrqGate = arm64::timer_irq_gate::AArch64TimerIrqGate;

#[cfg(target_arch = "riscv64")]
pub type CurrentTimerIrqGate = riscv64::timer_irq_gate::Riscv64TimerIrqGate;
```

kernel：

```rust
<CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq();
```

而当前：

```text
register_timer_handler
boot_init_timer
dummy_timer_handler
```

全部不存在。

**这就是我现在认为可以 freeze 的设计。**

---

# 十五、最后给你一个真正的“收敛检查表”

我现在不会再让你继续讨论方案，而是看下面这些是否全部满足。

| 项目                                       | 状态                  |
| ---------------------------------------- | ------------------- |
| `ArchBoot` 删除                            | ✅                   |
| `register_timer_handler` 当前删除            | ✅                   |
| 不创建 `TimerHandlerRegistry`               | ✅                   |
| `TimerIrqGate` 独立 trait                  | ✅                   |
| 只有 enable/disable                        | ✅                   |
| 三架构真实硬件实现                                | ✅                   |
| per-arch ZST                             | ✅                   |
| `CurrentTimerIrqGate` alias              | ✅                   |
| kernel 不写 target cfg                     | ✅                   |
| 不建立 `Arch` 大聚合                           | ✅                   |
| 不建立 `Arch::TimerIrqGate` associated type | ✅                   |
| 删除 MockArchBoot                          | ✅                   |
| 不让生产代码写 MOCK state                       | ✅                   |
| `BootError` / `TimerHandlerFn` 清理        | ✅                   |
| QEMU 作为真实硬件验证                            | ✅                   |
| 行为变更显式记录                                 | ✅                   |
| LAPIC 时序前置核验                             | ✅                   |
| GIC 时序前置核验                               | ✅                   |
| **GIC 必须由 TimerIrqGate 写死**              | ❌ 应修改为“端到端可达，责任待核验” |
| `MockTimerIrqGate`                       | ⚠️ 建议删除             |
| `register` TODO 放 timer_irq_gate.rs      | ⚠️ 建议移到 doc         |

---

# 最终 Verdict

### **架构决策：已收敛。**

我会给：

> **READY FOR IMPLEMENTATION**

而不是：

> “再讨论一下方案。”

但在真正改代码前，我要求你做 **4 个最后修改**：

### P0：改元原则 4

把：

> `TimerIrqGate` 必须写 CNTP + `ICC_IGRPEN1_EL1`

改成：

> **timer IRQ 必须端到端可达；具体 GIC enable 的职责和寄存器写入位置由 Step 0.5b 的实际初始化时序决定。**

### P1：测试直接删 `MockTimerIrqGate`

不要为了平移旧测试而制造一个没有价值的新 mock。

### P1：`register` future TODO 从 `timer_irq_gate.rs` 移到 doc

代码里最多留一个指向设计文档的 breadcrumb。

### P1：明确 `cfg` 的边界

不是：

> “禁止 cfg”。

而是：

> **禁止 architecture selection 泄漏到 capability 使用方；`cfg` 只用于 `Current*` alias 等 architecture boundary。**

---

除此之外，**我认为这次真的已经收敛了。**

尤其完整看完之后，我现在反而不会再建议你改成 `ClockArch`，也不会再建议 free function，更不会再建议 `TimerHandlerRegistry`。

因为现在的事实链已经足够强：

```text
ArchBoot
   ↓
boot phase ≠ capability
   ↓
删除

register_timer_handler
   ↓
当前无真实调用链 / 仅 mock / dispatch 已 deferred
   ↓
删除，未来重建

enable/disable
   ↓
当前就存在真实 arch variance
   ↓
TimerIrqGate

CurrentTimerIrqGate
   ↓
项目实际已有 Current* alias 模式
   ↓
保留

kernel
   ↓
<CurrentTimerIrqGate as TimerIrqGate>::enable_timer_irq()
```

这条链现在是**自洽、由代码事实支撑，而且没有为了未来过度设计**的。

所以这次我会明确说：

> **可以冻结架构决策，进入实现。剩下的问题已经不是“设计选哪条路”，而是 LAPIC/GIC 时序核验和代码清理是否正确。** 
