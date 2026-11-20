# 独立评审请求：页表页归还的共享子树与所有权表达

> 本文是为了交给一个**没有任何本仓库上下文**的外部评审者而写的自包含文档。
> **版本说明**：这是**第十版（概念设计冻结终版）**。
>
> 各版演进（变更对照见 §7）：
>
> | 版 | 本版做了什么 | 评审裁决 |
> |---|---|---|
> | 一 | 提出「跨域归还」为问题 B；提出问题 C（共享子树）| 问题 B 论证缺少"注册状态作用域"前提 |
> | 二 | 核清跨映像静态状态，**撤回问题 B** | 认可 C 与 D；撤回它自己第一轮对 Acquire 的判断 |
> | 三 | 补齐三组证据；**纠正我的错判**（bootstrap 根页不来自 `pt_alloc`）| 认可证据；指出四处表述过头 |
> | 四 | 降级"全部跨域归还"为合同缺陷；`destroy()` 无钩子分支重述为危险能力；维度 C 改名 | 认可修订；指出 `OwnedPaging` trait 拆分**不能隔离** |
> | 五 | 承认 trait 拆分无效，转向外层 handle 类型 | 认可方向乙；要求补**两个生命周期合同** |
> | 六 | 补齐那两项生命周期合同 + 收敛 API 三层层分 | 接受分层；要求证明**物理页持续保留**、补**四项合同细节** |
> | 七 | 查清保留机制并提出隐患；确认四个活动根调用点全部会写页表；纳入四项合同 | 冻结概念设计；要求更正时间线错判、给范围分类接口、拆凭证语义 |
> | 八 | 更正三处事实；把风险范围缩到一处；补 `classify_range` / 双凭证 / `'static` 限制；`retire` 取代 `reclaim`；给 A–H 分期 | 设计冻结通过；要求补四处实施基线、同步六处文档不一致 |
> | 九 | 补四处实施基线；同步六处旧 API 残留；拆 Q3 为三种变化；细化 TX 保留区并点出容量数据缺失；给六项测试验收清单 | 设计冻结通过；要求补三处实施接口 |
> | **十（本版，终版）** | **补三处实施接口**（`PrivateSupervisor` 终结路径 + `enable` 归位 / 内部回收测试入口 / `#[must_use]` 效力收紧）；同步六处文档残留；**冻结范围改为"签名允许实现中调整"** | **概念设计冻结通过；代码实施现在开始；生产安全验收有条件通过** |

---

## 1. 背景

Minix3 是微内核操作系统。本项目（`minix-rs`）用 Rust 重写其内核模块，目标三架构（x86-64 / aarch64 / riscv64），`#![no_std]`。

两个执行模型：

- **内核**：SMP + 大内核锁（BKL，自旋锁）
- **用户态服务器**（VM、PM、VFS 等）：单线程事件循环

**VM 服务器**负责管理所有进程页表（Minix3 传统设计：页表由用户态 VM 建，不由内核建）。**VM 是一个独立的用户态 ELF 进程**，不是链进内核的模块。

### 1.1 关键事实：VM 是独立进程，静态变量各自一份

这一点是本次修订的核心。核实结果：

| 事实 | 证据 |
|---|---|
| VM 服务器是独立可执行体 | `os/servers/vm/Cargo.toml` 的 `[[bin]] name = "minix-vm"` |
| VM 是引导模块之一 | `os/libs/minix-boot/src/kernel_info.rs:667` 的 `MODULE_NAMES = ["ds","rs","pm","sched","vfs","memory","tty","mib","**vm**","pfs","mfs","init"]` |
| `minix_arch` 被两个独立映像分别链接 | `os/servers/vm/Cargo.toml:25` 与 `os/kernel/Cargo.toml:13` 各自 `minix-arch = { path = ... }` |

**结论：`minix_arch::pt_alloc` 的 `static PT_ALLOC` / `static PT_FREE` 在"内核映像"与"VM 映像"里各有一份，互不共享。**

内核启动时 `register(boot_pt_alloc)` 只改**内核映像里**的那份；VM 启动时 `register_free(vm_pt_free)` 只改**VM 映像里**的那份。

**因此第一版提出的「内核 bump 页被 VM allocator 归还」这一跨域路径不成立**——内核映像里的 `PT_FREE` 在整个内核生命周期内始终是 `uninit_free`（因为 `os/kernel/src` 与 `os/kernel-image/src` 里 `register_free` 零命中）。

---

## 2. 机制：注册式页表页分配器

### 2.1 为什么需要"注册式分配器"

多级页表映射新区域时可能需要新建中间层页表页。谁提供这些页，随阶段变化：

| 阶段 | 页表页来源 | 地址关系 |
|---|---|---|
| 内核引导期 | 引导期 bump 池（一次性递增，**从不回收**）| VA = PA（恒等映射）|
| VM 运行期 | VM 的物理分配器（`BitmapAllocator`，**可分配可回收**）| VA = 直接映射基址 + PA |

为了让 `Paging` 实现不需要知道自己在哪个阶段，项目用**函数指针注册**（每个映像各自一份静态状态）：

```rust
// os/arch/src/arch/pt_alloc.rs（简化）
type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;
static PT_ALLOC: PtAllocSlot = ...;
static PT_REGISTERED: AtomicBool = false;

pub fn register(alloc_fn: PtAllocFn) { /* 一次性注册 */ }
pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    assert!(is_registered(), "called before register");   // ← fail-closed
    let f = unsafe { read(PT_ALLOC) };
    f()
}
```

### 2.2 归还侧（后加的，与分配侧不对称）

```rust
type PtFreeFn = fn(PhysBytes);
static PT_FREE: PtAllocSlotUnary = UnsafeCell::new(uninit_free);
static PT_FREE_REGISTERED: AtomicBool = false;

fn uninit_free(_phys: PhysBytes) {}          // ← 空操作

pub fn register_free(free_fn: PtFreeFn) { /* 一次性注册 */ }

pub fn free_pt_page(phys: PhysBytes) {
    let _ = PT_FREE_REGISTERED.load(Ordering::Acquire);   // ← 返回值被丢弃
    let f = unsafe { read(PT_FREE) };
    f(phys)                                              // ← 未注册时调 uninit_free
}
```

**不对称**：分配侧未注册 panic，归还侧未注册静默空操作。

### 2.3 谁注册（按映像分别看）

| 映像 | `register`（分配） | `register_free`（归还） |
|---|---|---|
| **内核映像**（`os/kernel/src/lib.rs:896`）| `boot_pt_alloc`（bump 池）| **从不注册** |
| **VM 映像**（`os/servers/vm/src/vm_server.rs:383/391`）| `vm_pt_alloc`（物理分配器）| `vm_pt_free` |
| QEMU 测试内核（20+ 个） | `boot_pt_alloc` | **从不注册** |

核实命令与读数：

```
$ grep -rn "register_free" os/kernel/src os/kernel-image/src --include="*.rs" | wc -l
0
$ grep -rln "register_free" os/qemu-tests --include="*.rs" | wc -l
0
```

**所以"归还侧静默空操作"在内核映像与全部测试内核里是常态**——它们的 `PT_FREE` 永远是 `uninit_free`。

### 2.4 C 侧证据：引导期页表池本来就是"只进不出"

`minix3/minix/kernel/arch/i386/pg_utils.c`：

```c
u32_t *alloc_pagetable(phys_bytes *ph)
{
	u32_t *ret;
#define PG_PAGETABLES 6
	static u32_t pagetables[PG_PAGETABLES][1024]  __aligned(4096);
	static int pt_inuse = 0;
	if(pt_inuse >= PG_PAGETABLES) panic("no more pagetables");
	ret = pagetables[pt_inuse++];
	*ph = vir2phys(ret);
	return ret;
}
```

**只递增 `pt_inuse`，无任何归还路径，6 页用尽即 panic。**

C 的 VM 侧（`minix3/minix/servers/vm/pagetable.c:1427`）则可回收，且只释放**自己分配的**中间表页：

```c
void pt_free(pt_t *pt)
{
	int i;
	for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++)
		if(pt->pt_pt[i])
			vm_freepages((vir_bytes) pt->pt_pt[i], 1);
	return;
}
```

C 的内核空间映射用**大页**，因此不产生中间表页（`pt_mapkernel`）：

```c
while(mapped < kern_size) {
#if defined(__i386__)
	pt->pt_dir[kern_pde] = addr | ARCH_VM_PDE_PRESENT |
		ARCH_VM_BIGPAGE | ARCH_VM_PTE_RW | global_bit;
#endif
	kern_pde++;
	mapped += ARCH_BIG_PAGE_SIZE;
	addr += ARCH_BIG_PAGE_SIZE;
}
```

**每条 PDE 直接是叶条目（4 MiB），零中间表页。**

---

## 3. 问题清单（修订后）

### 3.1 已撤回：问题 B（跨域归还）

第一版论证：内核注册 `boot_pt_alloc`、VM 注册 `vm_pt_free`，于是 UNIX 进程内 `PT_ALLOC` 是 bump、`PT_FREE` 是 VM 分配器。

**外部评审指出这缺一个前提：两次注册是否作用于同一份静态状态。** 核实结果见 §1.1——**不作用于同一份**。VM 是独立映像，独立的静态变量。

**撤回依据（三条独立证据）**：
1. VM 有独立 `[[bin]]`（`minix-vm`）
2. VM 是 `MODULE_NAMES` 里的第 9 个引导模块
3. 内核侧 `register_free` 零命中 → 内核映像的 `PT_FREE` 恒为 `uninit_free`

**保留的相关观察**：虽然跨映像不成立，但**同一映像内**"当前注册的 free 函数 ≠ 该物理页的分配来源"这一点仍然成立（见 §3.3 的根页所有权问题）。

### 3.2 成立：问题 C（继承的共享子树被递归归还）

**这是本轮确认的真实缺陷。**

`load_process_elf`（`os/arch/src/arch/boot.rs:800-828`）建新进程页表的流程：

```rust
// ① 根页来自 VmBootAllocator（不是 pt_alloc）
let root_frame = vm_alloc.alloc_page()?;
let root = PhysBytes(root_frame.start().0);

// ② 包装并 zero-fill
let mut paging = P::new_from_page(root);

// ③ 继承 bootstrap 根的上半场（内核空间）
paging.inherit_supervisor_half(bootstrap_root);

// ④ 装 ELF 到下半场
load_elf_into(module, kernel_info, &mut paging, vm_alloc, access)?;
```

**第 ③ 步的实现是"复制 PTE 值"**（x86-64，`os/arch/src/x86_64/paging.rs:650`）：

```rust
fn inherit_supervisor_half(&mut self, source_root: PhysBytes) {
    // PML4 entries 256..512 = the higher half: kernel text/data, stacks and
    // the DM windows. Raw entry copy — the entries may point at shared
    // PDPT/PD/PT pages, which is sound because user mappings never
    // touch the upper half (see trait doc).
    for i in 256usize..512 {
        let slot = (i as u64) * 8;
        let entry = read_pte_dm(source_root.0 + slot, PteChannel::KernelDm);
        write_pte_dm(self.root_paddr + slot, entry, 0, PteChannel::KernelDm);
    }
}
```

**注释自己承认 "the entries may point at shared PDPT/PD/PT pages"。**

于是两个根之间形成**共享子树**：

```
bootstrap root
  └── upper-half entry → 共享 PDPT / PD / PT（来自内核 bump 池）

新进程 root
  └── 复制的 upper-half entry → 同一批 PDPT / PD / PT
```

**而回收侧 `free_child_tables` 不识别共享**（`os/arch/src/x86_64/paging.rs:1108`）：

```rust
unsafe fn free_child_tables(&self, table_paddr: u64, level: u8) {
    if level >= 3 { return; }
    let base = channel_to_ptr(table_paddr, self.channel);
    for i in 0..512usize {                       // ← 全 512 条，含上半场
        let pte = unsafe { core::ptr::read_volatile(base.add(i)) };
        if pte & PRESENT != 0 && pte & PS == 0 {
            let child = pte & ADDR_MASK;
            if level + 1 < 3 {
                unsafe { self.free_child_tables(child, level + 1) };
            }
            crate::pt_alloc::free_pt_page(PhysBytes(child));   // ← 归还！
        }
    }
}
```

**无跳过、无"这是继承的"标记。**

若对这样的表调 `destroy()`（生产调用点 2 处，见 §4 表）：

```rust
unsafe fn destroy(&mut self) {
    if !crate::pt_alloc::is_free_registered() {
        // 未注册：只零根
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        return;
    }
    unsafe { self.free_child_tables(self.root_paddr, 0) };   // ← 递归全树
    unsafe { core::ptr::write_bytes(ptr, 0, 512) };
    crate::pt_alloc::free_pt_page(PhysBytes(self.root_paddr));
}
```

**三架构同形**（注意目录名是 `arm64` 不是 `aarch64`）：

| 架构 | 文件 | `destroy` | `free_child_tables` | `inherit_supervisor_half` |
|---|---|---|---|---|
| x86-64 | `os/arch/src/x86_64/paging.rs` | `:762` | `:1108` | `:650` |
| aarch64 | `os/arch/src/arm64/paging.rs` | `:738` | `:243` | `:595` |
| riscv64 | `os/arch/src/riscv64/paging.rs` | `:778` | `:604` | `:694` |

三份 `inherit_supervisor_half` 都只复制 256..512 这一段（x86-64 是 PML4 条目，arm64 是 L0 条目，riscv64 是 Sv39 的 L2 条目）。

**aarch64 有一处措辞差异值得记下**：它的注释明确说本 crate 把 `TTBR0_EL1` 与 `TTBR1_EL1` 钉在**同一个根页**，所以"两张表"的划分发生在同一个 512 项 L0 内部（`arm64/paging.rs:595` 起）。x86-64 与 riscv64 的注释只说"上半场"，没有这层说明。**语义上三者一致，解释方式不同**——如果未来要统一文档措辞，这一处是最容易被误认为"三架构有实质差异"的地方。

#### 严重度

**结构缺陷：P1 级危险。** 一旦在支持递归回收的上下文里销毁这种共享页表，会破坏仍在使用的页表。

**当前触发：无。** 核实：
- 生产 `.destroy()` 调用点只有 2 处（`vmproc_handle.rs:419`、`vmproc.rs:186`），对象都是 `Paging::new()` 建的自分配根；第 3 处 `x86_64/paging.rs:1769` 在测试内
- `load_process_elf` 只被 `os/qemu-tests/.../test-sysboot/src/main.rs:254` 调用，且该测试内核拿到 `ProcessLoad` 后**不调 destroy**

**分级措辞（采纳第二版评审）**：

> **失效后果达到 P1；当前触发条件不成立，所以修复优先级按 P2 管理。**

这避免把"目前没有调用点"误读成"设计是安全的"。

**措辞订正（采纳第二版评审）**：上面写「对这样的表调 `destroy()`」后接三个调用点，容易让读者以为那三个调用点里**已经有**继承型页表。准确表述是：

> **若未来对 `new_from_page() + inherit_supervisor_half()` 构造的 handle 调用 `destroy()`，当前实现将进入该错误回收路径。现有三个调用点均不是这种对象。**

这与 §4 的"当前无触发"一致。

### 3.3 成立：根页所有权同样未表达

外部评审补充的一点，我核实后同意：

`destroy()` 末尾**无条件**执行：

```rust
crate::pt_alloc::free_pt_page(PhysBytes(self.root_paddr));
```

但 `new_from_page()` 的根页有四种来源（§4bis.1b），**没有一种**是 `pt_alloc::alloc_pt_page()`。

所以：**`new_from_page()` 收到的根页，`destroy()` 却假定它属于当前 free 钩子的域。**

即使修好 §3.2 的共享子树问题，"根页归谁"仍未回答。

**这是比 §3.2 更基础的一层**：不只是"共享子树漏了一个跳过条件"，而是 `Paging` API 对根页与子树的所有权都表达不清。

#### 3.3.1 措辞订正（采纳第三版评审）：这是合同缺陷，但不是正在发生的错误释放

第三版评审指出我上一版的表述过头了。它说得对，我接受。上一版写「对所有四种真实根页来源**全部**都是跨域调用」，这句话读起来像是"现在已经在错放"。

准确的表述要过两道门：

| 门 | 条件 | 当前状态 |
|---|---|---|
| 门 1 | 该 handle 所在映像注册了 `PT_FREE` | 内核映像、QEMU 测试内核都**没有注册**；只有 VM 映像注册了 `vm_pt_free` |
| 门 2 | 该 handle 被调 `destroy()` 且进入递归回收分支 | 生产调用点 2 处，都在 VM 映像、都由 `Paging::new()` 建 |

于是当前真实分布是：

| 场景 | `destroy()` 实际行为 | 是否调用 free 回调 |
|---|---|---|
| 内核映像的外部根 handle | 清零根页后返回 | **否** |
| QEMU 测试内核的外部根 handle | 清零根页后返回 | **否** |
| VM 映像的 `Paging::new()` 根 | 递归释放子树 + 归还根 | 是，且根确实来自 `pt_alloc` |

**没有证据表明任何外部根页已被交给 `vm_pt_free` 或任何错误回调。** 建议的准确结论是：

> 若外部根页对应的 handle 在**注册了 `PT_FREE` 的映像**里进入递归销毁分支，当前实现会把该根页交给 `pt_alloc::free_pt_page()`，即使根页并非由该分配域分配。这是**已证实的所有权合同缺陷**，但当前生产调用点尚未触发该路径。

#### 3.3.2 风险链是两个条件，不是一个

上一版我写"只差一次 `register_free` 调用"就是"最薄的一层壳"。第三版评审指出这也不准确——还缺第二项：

```
内核侧注册 PT_FREE
        +
对包装外部 bootstrap 根的 handle 调用 destroy()
        ↓
递归回收外部根及其子树（清零 + 把根页交给错误回调）
```

**注册钩子本身不触发释放。** 所以这条链的"薄"要靠两件事同时发生，而不是一件。

#### 3.3.3 但"没有 free 钩子"绝不等于 `destroy()` 安全

这是第三版评审最有力的补充，我核实后确认，而且它**比原稿写的问题更直接**。

`destroy()` 在无 free 钩子时不是空操作：

```rust
if !crate::pt_alloc::is_free_registered() {
    let ptr = channel_to_ptr(self.root_paddr, self.channel);
    unsafe { core::ptr::write_bytes(ptr, 0, 512) };      // ← 清零根页
    return;
}
```

（`x86_64/paging.rs:776`、`arm64/paging.rs:751`、`riscv64/paging.rs:790` 三处同形。）

**清零一个借用来的活动根，等于把正在使用的页表当场抹掉。** 这不需要任何 free 钩子、不需要任何分配器，只要一次 `destroy()` 调用就够了。

所以结论要升级一档：

> **`destroy()` 不该出现在借用型 handle 的能力集合里。** 这不是"调用时检查 `is_free_registered()`"能解决的——检查只能防止归还物理页，防不住清零。

而且注释自己的措辞就暴露了问题。无钩子分支写的是 "zero the root PML4 **to prevent use-after-free if the physical page is reused**"——这个理由的前提是"我有权处置这一页"（我要防止别人复用后仍被旧 PTE 引用）。对借用根页的 handle 来说，这个前提根本不成立。

### 3.4 已撤回：`free_pt_page` 的 Acquire 读取（原判"无效"不成立）

```rust
pub fn free_pt_page(phys: PhysBytes) {
    let _ = PT_FREE_REGISTERED.load(Ordering::Acquire);   // ← 返回值丢弃
    let f = unsafe { read(PT_FREE) };
    f(phys)
}
```

**第二版评审的纠正（我接受）**：Atomic load 的 Acquire 语义**不依赖返回值是否被使用**。若 `register_free` 先写指针、再以 Release 发布 `PT_FREE_REGISTERED = true`，那么 `free_pt_page` 的 Acquire load 只要**读到** `true`，同步关系就已建立——即使把布尔值丢弃。

所以「`_ = load(Acquire)` 等于没有发布关系」这个说法**不成立**。

**仍值得考虑的是**：若注册与释放可能并发，读方读到 `false` 却仍去读函数指针，会在"已写指针、未发布标志"窗口读到未发布的值。但：

- VM 是**单线程事件循环**，`VmServer::new()` 完成注册后才进入分配/销毁路径；
- 内核映像与全部测试内核**从不注册** `PT_FREE`；
- 且 `destroy()` 自己已先 `is_free_registered()` 检查，再调 `free_child_tables`。

**结论：不是已确认的并发缺陷。** 是否改成显式分支是"防御性写法"的取舍，不构成本轮修复项。

**保留一个观察**：`alloc_pt_page` 前面有 `assert!(is_registered())`，`free_pt_page` 没有；两侧的"前置条件检查"强度仍不对称（见 §3.5 原始议题）。

### 3.5 观察（非缺陷）：Rust 逐页映射 vs C 大页映射

| | 内核空间映射粒度 | 中间表页 |
|---|---|---|
| C `pt_mapkernel` | **大页**（1 PDE = 4 MiB 叶）| **零** |
| Rust `map_kernel` | **4 KiB 逐页** | **每进程一套** |

Rust 版本（`os/arch/src/arch/paging.rs:646-690`）逐页 `map()`，`map()` 在缺失中间层时 `alloc_pt_page()`。

**外部评审的判断（我同意）**：逐页映射可能是 W^X 的有意改进（text 需 RX、data 需 RW，大页无法分权限），**不应仅因偏离 C 就否定**。但成本需实测（每 root 的内核上半场页表页数）。

**这也解释了为什么不能简单"跳过上半场"**：

| 路径 | 内核上半场 | 正确回收 |
|---|---|---|
| `Paging::new()` + `map_kernel`（VM 进程，`vmproc_handle.rs:364`）| **自己分配** | 应释放自己的 |
| `new_from_page` + `inherit_supervisor_half`（`load_process_elf`）| **共享 bootstrap** | **不能释放** |

**两条路径并存，`destroy()` 不知道自己是哪一种。** 所以 C 的 `pt_free`（只遍历用户空间目录项）**不能直接照搬**——C 用大页不产生内核侧中间表页，Rust 逐页映射后所有权关系已不同。

---

## 4. 当前是否会触发（核实汇总）

| 触发条件 | 是否满足 | 证据 |
|---|---|---|
| 对包装 bootstrap 根的 handle 调 `destroy()` | **否** | 生产 `destroy()` 调用点 2 处：`vmproc_handle.rs:419`、`vmproc.rs:186`，对象都是 `Paging::new()` 建的自分配根（另有 1 处 `x86_64/paging.rs:1769`，在 `mod tests` 内）|
| `load_process_elf` 建的表被 destroy | **否** | 唯一生产调用者 `test-sysboot/src/main.rs:254`，不调 destroy |
| 内核映像里 `PT_FREE` 被注册 | **否** | `grep -rn register_free os/kernel/src os/kernel-image/src --include=*.rs` → **0 行** |
| 测试内核里 `PT_FREE` 被注册 | **否** | `os/qemu-tests` `register_free` 零命中 |
| 测试内核里调 `destroy()` | **否** | `os/qemu-tests` 生产代码 `.destroy()` 零命中 |
| 内核侧 `assert!(is_registered())` 会被绕过 | **否** | `register` 已在 `lib.rs:974` 执行，早于任何 `map()` |
| **对借用活动根的 handle 调 `destroy()`** | **否** | 生产 `from_active_root` 3 处、`adopt_active_root` 2 处**全都不调 destroy**（§4bis.1）|
| **内核侧未来注册 free 钩子** | **当前否** | 需同时满足两个条件（§3.3.2）：注册钩子 **且** 对外部根 handle 调 `destroy()` |
| **生产代码绕过 `destroy()` 直接调 `free_pt_page()`** | **否** | 全仓只有两个入口，都在 `is_free_registered()` 门后（§4bis.4）|

**结论：所有触发链当前都不成立。** 缺陷是结构性的（合同缺失），不是现行故障。

**两处措辞订正（采纳第三版评审）**：

1. 上一版写"内核侧只差一次 `register_free`"是"最薄的一层壳"。不准确——还要有代码去 `destroy()` 一个外部根 handle。风险链是**两个条件**，不是一个（§3.3.2）。
2. 上一版写"对四种根页来源全部是跨域调用"。也不准确——它需要 handle 位于**注册了 free 钩子的映像**里才成立。内核映像与测试内核都不注册，所以那些外部根 handle 目前只会走"清零"分支（§3.3.1）。

**但清零分支本身是另一个更直接的隐患**：它不需要 free 钩子，只需要一次 `destroy()` 调用就能抹掉一个活动页表（§3.3.3）。

---

## 4bis. 五组补充证据（第二版与第三版评审要求）

### 4bis.1 组一：四个构造函数的全部调用点与责任

| 构造函数 | 生产调用点 | 根页来源 | 调用者是否认为所有权转移 |
|---|---|---|---|
| `new()` | `os/servers/vm/src/vmproc/vmproc_handle.rs:364`（`init_page_table`）、`fork.rs` / `cow_exec_pf.rs` / `region/mod.rs` 的多处测试 | `alloc_pt_page()`（本域）| **是**（`destroy()` 释放它）|
| `new_from_page()` | **2 处生产**：`os/kernel/src/lib.rs:1292`（`build_bootstrap_root_and_enable`，被 `:525` 与 `:1017` 两个引导路径调用）、`os/arch/src/arch/boot.rs:818`（`load_process_elf`）；其余为测试 | **见 4bis.1b（四种来源，均非本域）** | **否**（见 4bis.1b）|
| `from_active_root()` | `os/kernel/src/kerninfo.rs:143`、`os/kernel/src/lib.rs:1707`、`:2902` | **活动根**（内核自举根）| **否**（trait 文档明写"除非你拥有它，否则不要 destroy"）|
| `adopt_active_root()` | `os/servers/vm/src/pagetable/vm_self_map.rs:102`；`test-proc-init` 测试内核 | **活动根**（VM 自举根 = 内核建的 bootstrap 根）| **否**（同上）|

#### 4bis.1b `new_from_page()` 的根页其实有**四种**来源，全都不是 `pt_alloc` 域

这是本次核实里**最出乎我意料的一格**。我原本以为内核侧 bootstrap 根页来自 `pt_alloc::alloc_pt_page()`，逐行追下去发现不是。

| 引导形态 | 根页分配代码 | 分配器 | 属于 `pt_alloc` 域？ |
|---|---|---|---|
| UEFI 腿（x86-64 / aarch64）| `os/boot-shim/src/uefi_helpers.rs:602` `alloc_root_page()`，被 `:991` 调用 | `boot::allocate_pages(AllocateType::MaxAddress(...), MemoryType::LOADER_DATA, 1)` —— **UEFI 固件分配服务** | **否** |
| 直连核 `bootface` 腿（riscv64）| `os/kernel-image/src/bootface.rs:264` `bump_alloc_pages(1)` | boot-shim 的 bump 游标（`os/boot-shim/src/opensbi_helpers.rs:630`，转发到 `bump_alloc`）——**同一物理池，但走的是 `bump_alloc_pages` 入口，不是 `pt_alloc::alloc_pt_page`** | **否**（同一池，不同入口）|
| 直连核 `bootface_a64` 腿（aarch64）| `os/kernel-image/src/bootface_a64.rs:243` `bump_alloc_pages(1)`（本文件私有，见 `:115`）| **这个文件自己的 `static mut BUMP_PTR`**（`:111`，起点 `DRAM_BASE + 0x0400_0000`，`:92`），**不是** boot-shim 那个池 | **否**（第三个、完全独立的池）|
| `load_process_elf`（QEMU 测试内核）| `os/arch/src/arch/boot.rs:810` `vm_alloc.alloc_page()` | `VmBootAllocator`（`os/arch/src/arch/frame.rs:639`）| **否** |

**精度要求**（采纳第四版评审 §1.1）：这里的"四种来源"要分清**分配入口**与**分配器实例**两件事，不能笼统说成"四份互不共享的状态"。

| 引导形态 | 分配入口 | 分配器实例 | 与 `pt_alloc` 区间的关系 |
|---|---|---|---|
| UEFI 腿 | `alloc_root_page()` | UEFI 固件分配服务 | 完全无关 |
| riscv64 `bootface` | `bump_alloc_pages(1)` | **同一个 boot-shim bump 游标** | **同一实例、不同区间** |
| aarch64 `bootface_a64` | 该文件私有的 `bump_alloc_pages(1)` | **另一个 `BUMP_PTR`** | 完全无关 |
| `load_process_elf` | `VmBootAllocator::alloc_page()` | `VmBootAllocator` | 完全无关 |

**riscv64 那一行是"同一分配器、不同区间"，不是"不同分配器"。** 它的根页与中间表页出自同一个游标、同一片物理池，只是根页落在 `[pt_base, pt_end)` 之前一格。这个区别很重要：它意味着 riscv64 侧根页**理论上可以**通过同一个游标归还，只是当前没有接口。

其余三行才是真正的分配器互不相通。

**第二、三行有个共同的陷阱**，容易被误读成"就在本域里"：两个 `bootface` 腿都先 `bump_alloc_pages(1)` 拿根页，再 `bump_alloc_pages(PT_BUMP_PAGES=256)` 拿中间表区间，然后把**后者**通过 `init_boot_pt_alloc(pt_base, pt_end)` 注册给 `boot_alloc`（`bootface.rs:280`、`bootface_a64.rs:257`）。根页取自同一个池、同一个游标，但**不在注册给 `pt_alloc` 的那个区间内**——它是紧挨在 `[pt_base, pt_end)` 之前的那一格。

**riscv64 侧甚至有一条 `assert_eq!` 把这个顺序钉死了**（`opensbi_helpers.rs:355-364`）：

```rust
assert_eq!(root_page.0, bump_pool_base(),
    "boot-shim: root page is not the bump pool's first page — halting");
assert_eq!(bump_base, root_page.0 + 4096,
    "boot-shim: page-table subrange does not start after the root page — halting");
```

即：根页**必须是**池的第一页，中间表区间**必须**紧跟其后。设计上明确承认"根页在池里但不在 pt_alloc 区间里"。

所以四种引导形态**各自**有一套根页来源，而 `pt_alloc` 的分配钩子只管中间表页。

**这使问题 D 的结论比"所有权未表达"更硬，但要按第四版评审 §1.1 的精度写**：

> 四种真实根页来源**均不属于 `pt_alloc` 的释放域**。因此，若包装这些根页的 handle 进入**已注册 free 钩子**的映像的递归销毁分支，当前 `destroy()` 会把根页交给一个不负责该根页的释放回调。这证明了**根页释放合同错误**，但**不代表这四种根页当前已经被错误释放**——触发它还需要"映像注册了 free 钩子"与"该 handle 被 `destroy()`"两个条件同时成立（§3.3.1）。

**并且内核映像根本没有 free 钩子**：`os/kernel/src` 与 `os/kernel-image/src` 只有 `pt_alloc::register(boot_alloc::boot_pt_alloc)`（`os/kernel/src/lib.rs:974`），`register_free` 零命中。所以内核映像里 `is_free_registered()` 恒为 `false`，`destroy()` 恒走"只零根、不递归"的分支。

**注意：把这一支叫作"降级"是我上一版的措辞失误。** 它不是安全降级，而是另一个独立的破坏动作（§3.3.3）。**同一个 `destroy()` 动词，在 VM 映像里意味着"归还全树+根"，在内核映像里意味着"清零根页"——语义随映像变化，而调用点看不出自己在哪一支。**

### 4bis.2 组二：`load_process_elf` 的完整资源生命周期

```rust
// os/arch/src/arch/boot.rs:800-828
pub fn load_process_elf<P: Paging, A: PhysAccess>(
    module: &BootModule,
    kernel_info: &KernelInfo,
    bootstrap_root: PhysBytes,
    vm_alloc: &mut VmBootAllocator,
    access: &A,
) -> Result<ProcessLoad, VmLoadError> {
    let root_frame = vm_alloc.alloc_page().map_err(|_| VmLoadError::OutOfMemory)?;
    let root = PhysBytes(root_frame.start().0);
    let mut paging = P::new_from_page(root);
    paging.inherit_supervisor_half(bootstrap_root);
    let result = load_elf_into(module, kernel_info, &mut paging, vm_alloc, access)?;
    Ok(ProcessLoad { root, ..., allocated_bytes: result.allocated_bytes + P::PAGE_SIZE })
}
```

**`VmBootAllocator` 的分配与回收合同**（`os/arch/src/arch/frame.rs:597-660`）：

```rust
pub struct VmBootAllocator<const N: usize = MAX_BOOT_REGIONS> {
    regions: VmBootRegions<N>,
    idx: usize,
    cursor: PhysBytes,      // 从 region.end 单调递减
    used_frames: u64,
}

impl<const N: usize> VmBootAllocator<N> {
    pub fn alloc_page(&mut self) -> Result<PhysFrame, VmBootAllocError> { /* 递减 cursor */ }
    pub const fn used_frames(&self) -> u64 { ... }
    // ← 没有任何 free / dealloc 方法
}
```

**关键事实：`VmBootAllocator` 是纯 bump 分配器，没有归还 API。**

| 问题 | 答案 |
|---|---|
| `ProcessLoad` 的接收者是谁 | 只有 `os/qemu-tests/.../test-sysboot/src/main.rs:254`（QEMU 测试内核）|
| 失败路径（`load_elf_into` 返回 `?`）时已分配的根页怎么办 | **无人回收**——`VmBootAllocator` 无 free，且 `Paging` / `PageTable` 均无 `Drop` 实现（`grep -rn "impl.*Drop for" os/arch/src` → **零命中**；全仓的 `impl Drop` 都在别的模块，如 `os/kernel/src/proc.rs:1139` 的 `KProcess`）|
| 成功返回 `ProcessLoad` 后，调用者负责什么 | 文档未定义；`ProcessLoad` 只带 `root: PhysBytes` 与标量，不带任何"如何释放"的信息 |
| 进程正常退出时谁销毁根页与中间表页 | `os/servers/vm/src/vmproc/vmproc_handle.rs:419` 与 `vmproc.rs:186` 的 `destroy()`——但那是 `Paging::new()` 建的自分配根，**不是** `load_process_elf` 建的 |

**结论：`load_process_elf` 的资源生命周期在失败路径与成功路径都不闭合。**

- 失败时：根页 + 部分中间表页泄漏（在 `VmBootAllocator` 的单次启动生命周期内无害，因为它是 bump 池，但语义上未定义）
- 成功时：`ProcessLoad` 的接收者拿到的 `root` 是一个**无法通过任何现有 API 正确释放**的物理页

### 4bis.3 组三：`register_free` / `is_free_registered` 完整实现

```rust
pub fn register_free(free_fn: PtFreeFn) {
    let claimed = PT_FREE_CLAIMED.swap(true, Ordering::AcqRel);
    assert!(!claimed, "pt_alloc::register_free called twice");
    unsafe { core::ptr::write(PT_FREE.0.get(), free_fn); }     // ① 写指针
    PT_FREE_REGISTERED.store(true, Ordering::Release);          // ② 发布
}

pub fn is_free_registered() -> bool { PT_FREE_REGISTERED.load(Ordering::Relaxed) }   // ← Relaxed

pub fn free_pt_page(phys: PhysBytes) {
    let _ = PT_FREE_REGISTERED.load(Ordering::Acquire);         // ③ 同步点
    let free_fn = unsafe { core::ptr::read(PT_FREE.0.get()) };  // ④ 读指针
    free_fn(phys)
}
```

**发布顺序是标准的 write-then-Release-publish**，`free_pt_page` 的 Acquire 与之配对（只要读到 `true`）。

**但 `is_free_registered()` 用的是 `Relaxed`**——这与 `alloc_pt_page` 里 `let _ = PT_REGISTERED.load(Acquire)` 的严格程度不一致。当前无实际后果（因为唯一消费者 `destroy()` 之后紧接着会调 `free_pt_page`，后者自带 Acquire），但**这是一处口径不齐**，评审可评估是否值得统一。

**并发注册是否可能**：`PT_FREE_CLAIMED.swap` 已串行化写者；读者侧若在"①写指针、②未发布"窗口读，会读到旧的 `uninit_free`（安全，只是不归还）。**没有 UB**。

---

### 4bis.4 组四：`free_pt_page()` 的全仓调用点（第三版评审第一问）

第三版评审要求确认"是否有代码绕过 `destroy()` 的注册检查直接调 `free_pt_page()`"。核实结果：

| 位置 | 形态 | 是否绕过 `is_free_registered()` 门 |
|---|---|---|
| `os/arch/src/x86_64/paging.rs:789` | `destroy()` 内，根页归还 | **否**（前面已过 `:776` 的门）|
| `os/arch/src/x86_64/paging.rs:1124` | `free_child_tables()` 内，子表归还 | **否**（`free_child_tables` 只在 `:786` 被调，也在门内）|
| `os/arch/src/arm64/paging.rs:759` / `:257` | 同上 | **否** |
| `os/arch/src/riscv64/paging.rs:802` / `:618` | 同上 | **否** |
| `os/arch/tests/pt_alloc_write_once.rs:68` / `:77` | 测试，显式注册后调用 | 不适用（测试自有注册）|

**结论：生产代码里 `free_pt_page()` 只有两个入口——`destroy()` 的根页归还与 `free_child_tables()` 的子表归还，两者都在 `is_free_registered()` 门之后。**

这**完全关闭**了第三版评审 §6.2 提出的"绕过检查的并发读写"这一问：不存在第三个入口，所以 `PT_FREE` 的非原子读永远发生在注册完成之后（内核/测试内核根本不注册，VM 在 `VmServer::new()` 里注册完才进事件循环）。

**但 §6.2 的技术纠正我仍要记下并接受**：如果真出现跨 CPU 并发注册与回收，`UnsafeCell` 里的非原子读**读到旧值也不等于没有数据竞争**。我上一版写"读到 `uninit_free` 所以安全"是错的。正确说法是：**当前不存在这个并发场景**（因为只有两个入口且都在门后、且注册早于任何回收），而不是"即使并发也安全"。

**附带一条第三版评审建议、我认同的处理**：`free_pt_page()` 自身在读到"未注册"时应当直接返回，而不是无条件读 `PT_FREE`。这不是当前缺陷，但它是让这个函数**独立于调用点**也安全的唯一办法——现在的安全性依赖"所有调用者都先检查过门"，是调用者的责任，不是函数自身的不变量。

### 4bis.5 组五：`pt_alloc` 在测试内核中的实际实现（第三版评审第二问）

第三版评审要求确认 `boot_pt_alloc` 的游标/容量、是否支持 checkpoint/rewind、加载期间是否有别的 `alloc_pt_page()` 调用、以及 `VmBootAllocator` 与该池是否完全独立。

#### 实现与状态

`os/kernel/src/boot_alloc.rs` 的 `BootAlloc` 有三个字段（都是 `AtomicU64`）：

```rust
pub struct BootAlloc {
    base: AtomicU64,
    next: AtomicU64,     // ← 游标，只增
    end: AtomicU64,
}
```

**容量**由调用者在 `init_boot_pt_alloc(base, end)` 时给定。两个 `bootface` 腿给 `PT_BUMP_PAGES = 256` 页（1 MiB，`bootface.rs:134`）；UEFI 腿由 `alloc_bump_region(bump_pages)` 决定。

#### checkpoint/rewind：**已经存在，但只用于跨映像交接**

这是我核实里的**唯一一处好消息**，也是第三版评审没有预料到的：

```rust
/// Resume a bump allocator whose region was already partly consumed by a
/// previous stage (the cross-image handoff).
pub fn resume(&self, base: u64, next: u64, end: u64) { /* 校验后存三个值 */ }
```

公开入口是 `boot_alloc::resume_boot_pt_alloc(base, next, end)`（`boot_alloc.rs:210`），生产调用点在 `os/kernel/src/lib.rs:766`——**跨映像 v1 交接**用它恢复 boot-shim 留下的游标。

它带一致性校验（`BumpRegion::is_consistent()`：`start < end && start <= next <= end`），所以"回退到一个非法游标"会被 `assert!` 挡住，不会静默写坏状态。

**所以 §4ter.4 里我建议的"回滚游标"不需要新 API**——`resume` 就是。唯一缺的是**读取当前游标**的公开入口：`boot_alloc_next()`（`boot_alloc.rs:199`）已经有了。两者合起来就是完整的 checkpoint/rewind：

```rust
let checkpoint = boot_alloc::boot_alloc_next();      // 已有
// ...失败的加载...
boot_alloc::resume_boot_pt_alloc(base, checkpoint, end);   // 已有
```

**但第三版评审 §5.2 的纠正我完全接受**：`BootAlloc` 的状态虽然只有三个值，而 `VmBootAllocator` 的状态是 `{ regions, idx, cursor, used_frames }` 四项。回滚 `VmBootAllocator` 必须恢复 `idx` 和 `used_frames`，不能只回退 `cursor`——因为一次分配可能跨 region 边界，`idx` 会跟着变。**`VmBootAllocator` 没有 `resume`，也没有读 `idx` 的入口。**

#### 加载期间是否有别的 `alloc_pt_page()` 调用

这是"能否安全回滚"的**决定性前提**，核实结果如下。

`test-sysboot` 的初始化顺序（`os/qemu-tests/.../test-sysboot/src/main.rs`）：

| 行 | 动作 |
|---|---|
| `:180` | `boot_alloc::init_boot_pt_alloc(bump_base, bump_end)` |
| `:181` | `pt_alloc::register(boot_alloc::boot_pt_alloc)` |
| `:182` | `arch_boot_impl` —— **这里会大量消耗 `pt_alloc`**（映射内核上半场）|
| `:248` | `VmBootAllocator::new(regions)` |
| `:254` | `load_process_elf(...)` |

**关键：`pt_alloc` 的注册发生在 `:181`，`arch_boot_impl` 在 `:182` 就已经开始用它。** 到 `:254` 调 `load_process_elf` 时，`BootAlloc` 的游标已经前进了"引导期所有页表页"那么多。

所以：**在 `load_process_elf` 内部，`paging.map()` 调 `alloc_pt_page()` 落到的就是同一个 `BootAlloc`**——它确实与 `VmBootAllocator` 完全独立（两个不同的 `static` / 不同的 `&mut`），但**它不是独占的**：`BootAlloc` 是进程级 `static BOOT_ALLOC`，任何持有 `pt_alloc` 钩子的代码都能推进它。

在 `test-sysboot` 这个具体上下文里，`load_process_elf` 的调用处恰好在 BKL 保护段内、且此前没有别的并发 `map()`（见下文"结论"），但**这是调用点的性质，不是分配器的保证**。

#### `VmBootAllocator` 与 `BootAlloc` 是否完全独立

**是，完全独立。** 证据：

| | `BootAlloc` | `VmBootAllocator` |
|---|---|---|
| 存储 | `static BOOT_ALLOC: BootAlloc`（`boot_alloc.rs`）| 调用者栈上的 `&mut VmBootAllocator<..>` |
| 游标方向 | **递增** `next`（`alloc_pages` 里 `next += need`）| **递减** `cursor`（`frame.rs:643` `cursor - PAGE_SIZE`）|
| 区域数 | 单一 `[base, end)` | 多个 `VmBootRegions<N>`，可跨 region |
| 初始化 | `init_boot_pt_alloc` / `resume_boot_pt_alloc` | `VmBootAllocator::new(regions)` |
| 谁注册给 `pt_alloc` | 是（`boot_pt_alloc`）| **否**（`VmBootAllocator` 从不注册）|

两者唯一的交汇点是 `load_elf_into` 同时持有 `&mut Paging`（走 `pt_alloc` → `BootAlloc`）和 `&mut VmBootAllocator`（走 `alloc_page`）。

#### 结论：回滚在什么前提下安全

第三版评审 §5.3 列的四个前提我逐一核对：

| 前提 | `VmBootAllocator` | `BootAlloc` |
|---|---|---|
| 整个加载独占借用 | **是**（`&mut`）| **否**（`static`，谁都能调）|
| 无其他分配交错 | 由调用点保证 | 由调用点保证，**不是分配器保证** |
| 分配的页未逃逸到活动根/全局 | 是（`load_process_elf` 失败时 `ProcessLoad` 未返回）| 是（失败的中间表页无人引用）|
| 可把状态恢复到检查点 | **否**（没有 `resume`，没有读 `idx` 的入口）| **是**（`resume` + `boot_alloc_next` 已有，带一致性校验）|

**这是个反直觉的结果**：独占借用的那一个反而**没有** checkpoint 能力，非独占的那个反而**有**。

所以第三版评审 §5.4 的三选一里，我倾向的是：

> **接受启动期失败后的页表中间页消耗，明确限制重试；不为理论上的释放引入通用 allocator 事务。**

理由：`BootAlloc` 有 `resume` 但不是事务语义（别人也能推进它），`VmBootAllocator` 根本没有 rewind。为这两者造一套真事务，要先给 `VmBootAllocator` 加 `checkpoint()`/`rollback()`、给 `BootAlloc` 加锁或独占借用的包装——而当前**没有任何生产重试需求**（`load_process_elf` 的唯一调用者是 QEMU 测试内核，失败即 `panic!`）。

**定性（采纳第三版评审）**：

> 失败路径的资源管理没有明确合同，值得写成文档并加断言；但在单次启动、单调 bump 的场景下，**未回收不自动等于运行期内存泄漏**，因此不升级为当前 P1 缺陷。

---

## 4ter. 所有权合同草案（第三版主体，第四至第六版按评审修订）

第二版评审建议"以所有权模型为主体"。我把 **三个独立维度** 拆开表达，而不是我第一版那个 `enum RootKind { Owned, Adopted, Inherited }`。

### 4ter.1 三个维度，且第三个要分开名字

| 维度 | 要回答的问题 | 取值 |
|---|---|---|
| **A. 根页所有权** | 这个 handle 有没有义务把根页还回去？还回哪个分配器？| `OwnedByPtAlloc` / `CallerManaged` / `Borrowed` |
| **B. 子树所有权** | 根页以下的中间表页，哪些是自分配的、哪些是共享借用的？| `AllOwn` / `SomeShared(upper_half)` / `BorrowedUnknown` |
| **C. 分配来源与释放策略** | 被这个 handle 拥有的页面，应由谁释放？| **每个可释放资源都能确定其来源与责任**（合同要求，非实现形式）|

**维度 C 的名字从"分配域"改成"分配来源与释放策略"**（采纳第三版评审 §3.2）。原来的名字会让人误以为它是一个全局单值，而实际有两个不同的东西被我塞在一个词里：

| | 是什么 | 谁决定 |
|---|---|---|
| **默认分配器绑定** | 后续 `alloc_pt_page()` 落到哪个函数 | 当前映像的 `PT_ALLOC` 全局静态 |
| **物理页的分配来源** | 某一个具体页面当初由谁掏出 | 那一页自己的 provenance |

维度 C 讲的是**后者**。所以不能因为"默认 `pt_alloc` 是全局配置"就推断"整个 `Paging` 对象只有一个分配来源"——`new_from_page()` 的根页与它随后 `map()` 出来的中间表页，就来自**两个不同的来源**（§4bis.1b + §4bis.5）。

**但"每页记录 provenance"是过度设计**（采纳第四版评审 §5 末段）。我上一版把合同要求写成了实现形式要求。按当前已知路径，正确的回收行为完全可以从三个已有的量推导出来：

```
根页责任（A：OwnedByPtAlloc / CallerManaged / Borrowed）
  + 私有/共享子树状态（B：AllOwn / SomeShared / BorrowedUnknown）
  + 当前映像的 pt_alloc 释放策略
```

**不需要给每一张中间页保存 provenance 标签。** 只有将来允许同一棵树里的私有页表页由多个独立分配域提供时，才需要更细粒度。

所以维度 C 的正确表述是**合同要求**：

> 每个可释放资源必须能够确定其**分配来源**和**释放责任**。

而不是"每一页都要存一个标签"。

**A 的取值用 `CallerManaged` 而不是 `OwnedByCaller`**（采纳第三版评审 §4.3）：如果调用者仍然拥有根页，那么对 `Paging` 来说它实际是"外部管理"，而不是"`Paging` 自己拥有但委托给调用者"。这两个说法在出问题时的追问方向不同——前者问"`Paging` 有没有越权处置"，后者问"调用者有没有履约"。

**为什么三个维度不能合并**（采纳第三版评审 §3.2 的收尾）：

| 机制 | 解决什么 | 不能替代谁 |
|---|---|---|
| `PtDomain { alloc, free }` | 内部页表页的分配器与释放器配对 | 不能回答 A（根页归属）|
| `RootOwnership` | 根页归属 | 不能回答 B（共享子树）|
| `SubtreeOwnership` | 共享子树 | 不能回答 A |

三者可以同时出现在一个对象上，例如：

```
root:          UEFI AllocatePages      → A = CallerManaged
lower subtree: pt_alloc                → B = AllOwn, C = 本域
upper subtree: bootstrap root 共享     → B = SomeShared(upper_half)
```

**这正是第一版 `enum RootKind` 表达不出来的组合**：`Inherited` 一个值同时暗示了 A（根页来自别处）和 B（子树共享），但它俩可以独立组合。

### 4ter.2 建议的构造合同

| 构造函数 | A. 根页 | B. 子树 | `destroy(self)` 应做什么 | 是否该有 `destroy` |
|---|---|---|---|---|
| `new()` | `OwnedByPtAlloc` | `AllOwn` | 递归释放全部 + 归还根页 | **是** |
| `new_from_page(p)` | **`CallerManaged`**（四种来源都不是 `pt_alloc`）| `AllOwn`（调用者之后可能 `inherit`）| **不归还根页**；只回收自己建的中间表页 | **是** |
| `from_active_root(p)` | `Borrowed` | **`BorrowedUnknown`** | —— | **否** |
| `adopt_active_root(p)` | `Borrowed` | **`BorrowedUnknown`** | —— | **否** |

**`from_active_root` / `adopt_active_root` 的 B 从 `AllOwn` 改成 `BorrowedUnknown`**（采纳第三版评审 §3.3）。上一版写"`AllOwn`（但无人能证明）"是自相矛盾的：既然无人能证明，就不能标成"全部自有"。这两个构造函数包装的是**已经存在**的活动根，对整棵树都没有回收授权。

只有两种树有资格标 `AllOwn`：

1. `new()` 建的空树——从零开始，每一页都是自己掏的；
2. `new_from_page()` 之后由本 handle 逐步 `map()` 出来的私有子树（根页仍是 `CallerManaged`）。

**`new_from_page()` 的"可写"与"可释放"必须分开**（采纳第三版评审 §3.4）。它 zero-fill 根页，所以调用者必须授权它**独占修改**；但**允许修改不等于转移释放责任**。这两件事在上一版被混在一起了。

**第四版评审 §4.1 指出 `CallerManaged` 这个词还漏了第三件事**，我接受。这里其实是**三个独立责任**：

| 责任 | 负责人 |
|---|---|
| 保证根页物理内存持续有效（不释放、不复用）| **调用者** |
| handle 存活期间不让其他主体同时修改根页 | **调用者授权，handle 独占修改** |
| 以后决定如何释放根页 | **调用者** |

只写 `CallerManaged` 会让两个方向都误读：

- 调用者以为已经转移了所有权 → 期望 `Paging` 去释放（但 `Paging` 不会）
- `Paging` 以为调用者仍在管理 → 但它已经在 zero-fill 和后续 `map()` 里一直写这一页

**建议的表达方式（两个层次）**：

**首选手法：显式访问凭证**（第四版评审 §4.2）

```rust
struct CallerManagedRoot<'a> {
    phys: PhysBytes,
    // 调用者把根页的独占修改权借给当前 handle 的这个生命周期
    _exclusive: PhantomData<&'a mut ()>,
}

fn new_from_page(root: CallerManagedRoot<'_>) -> CallerManagedPageTable<'_>;
```

凭证表达的是**访问权与生命周期**，不是 allocator provenance——它不需要知道这一页来自 UEFI 还是 bump 池。

**低成本退路：狭窄的 `unsafe` 构造边界**（第四版评审 §4.3）

引导期的裸物理页很难表达 Rust 借用关系（`root_page` 是 `PhysBytes`，没有对应的所有者对象）。这种情况下允许：

```rust
unsafe fn new_from_caller_managed_page(root: PhysBytes) -> Self;
```

`SAFETY` 合同必须写四条（评审给的清单，我原样采纳）：

1. 根页已分配，且在 handle 生命周期内不会被释放或复用
2. 调用者授权该 handle 独占修改根页
3. 初始化时根页不被其他活动页表或任何 CPU 使用
4. handle 结束前，调用者不得另行修改、释放或复用该页

**明确不做的**（评审 §4.3 末句）：**不为此发明全局物理页所有权登记表。** 引导期根页的所有者信息只存在于调用点，硬要登记成全局表会引入比问题本身更大的复杂度。

| 语义 | 含义 |
|---|---|
| `CallerManaged` | 调用者管生命周期与释放；`Paging` 有独占修改权，但**绝不**把根页交给 `pt_alloc::free_pt_page()` |
| `OwnedByPtAlloc` | 根页所有权明确转移给 `Paging`，且必须携带能正确释放它的凭证 |

四种真实来源（§4bis.1b）**都没有**展示与 `pt_alloc` 配套的单页释放能力，所以 `new_from_page()` 应定义为前者。

**具体改法建议（6 步，按依赖顺序；第 1 步已按评审调整）**：

1. **拆分拥有型与借用型 handle，从类型上消除借用活动根调用 `destroy()` 的可能**。

   **第四版评审 §2.1 指出我上一版给的方案是错的，我核实后完全接受。** 上一版写"`trait Paging` 去掉 `destroy`、`trait OwnedPaging` 加上 `destroy`"——这**根本不能隔离**。

   原因：Rust 的 trait 实现关联于**类型**，不是某一次构造出来的对象。核实当前代码：

   ```rust
   // os/arch/src/x86_64/paging.rs:282
   pub struct X86_64Paging {
       root_paddr: u64,
       channel: PteChannel,     // ← 只有这两个字段
   }
   // :580
   impl Paging for X86_64Paging { ... }
   ```

   四个构造函数（`:213`/`:235`/`:260`/`:287`）**全都返回 `Self`，也就是同一个 `X86_64Paging`**。所以只要

   ```rust
   impl OwnedPaging for X86_64Paging {}
   ```

   那么 `X86_64Paging::from_active_root(root)` 得到的对象**照样能调用 `destroy()`**。编译器不知道它是哪个构造函数产生的。**类型系统里根本没有"这个实例是怎么来的"这个信息。**

   更关键的约束：`X86_64Paging` 只有 `root_paddr` 与 `channel` 两个字段，**没有任何字段能表达所有权状态**。所以问题不只是"trait 拆得不够"，而是**当前那个具体类型本身承载不了所有权信息**。

   #### 可行的两个方向

   **方向甲：给具体类型加一个所有权状态字段，`destroy` 改为带状态检查**

   最小改动形态是在 `X86_64Paging` 加一个字段（三架构各加一份）：

   ```rust
   pub struct X86_64Paging {
       root_paddr: u64,
       channel: PteChannel,
       ownership: PagingOwnership,     // 新增
   }

   enum PagingOwnership {
       OwnedByPtAlloc,                 // new()
       CallerManaged,                  // new_from_page()
       Borrowed,                       // from_active_root() / adopt_active_root()
   }
   ```

   `destroy(self)` 在 `Borrowed` 状态下 `panic!`（或干脆不提供入口）。**优点**：改动面小，四个构造函数原地改。**缺点**：这仍然是**运行期检查**，不是编译期隔离——`destroy()` 依然在类型上可见，只是会在运行期拒绝。评审者要的"类型上真的成为三种对象"没做到。

   **方向乙：外层 handle 类型（第四版评审 §2.2 推荐）**

   ```rust
   trait PagingOps {                  // 纯操作，无 destroy
       fn map(&mut self, ...) -> ...;
       fn unmap(&mut self, ...) -> ...;
       fn query(&self, ...) -> Option<...>;
       // … walk / remap / update_flags / split_huge / flush_tlb
   }

   struct OwnedPageTable<P>         { inner: P }   // destroy(self)
   struct CallerManagedPageTable<P> { inner: P }   // into_permanent_root(self)
   struct BorrowedPageTable<P>       { inner: P }   // 无终结操作
   ```

   构造入口按 handle 类型分开：

   ```rust
   impl<P: PagingOps> OwnedPageTable<P>         { fn new() -> Result<Self, _>; }
   impl<P: PagingOps> CallerManagedPageTable<P> { fn new_from_page(r: CallerManagedRoot<'_>) -> Self; }
   impl<P: PagingOps> BorrowedPageTable<P>      { fn from_active_root(r: PhysBytes) -> Self; }
                                               { fn adopt_active_root(r: PhysBytes) -> Self; }
   ```

   **优点**：编译期隔离，`BorrowedPageTable` 上**不存在** `destroy` 这个名字。**缺点**：`CurrentPaging` 是架构关联类型（`os/arch/src/lib.rs:149` `pub type CurrentPaging = <CurrentBootArch as BootArch>::Paging;`），要给它套 handle 就要改 `load_elf_into`/`load_vm_elf`/`load_process_elf` 的泛型签名（现在是 `P: Paging`），以及 VM 侧 `PageTable` 别名（`os/servers/vm/src/pagetable/mod.rs:32`，且 `cfg(test)` 下换成 `sim::SimPaging`——**两个类型都要满足 handle 的 bound**）。

   #### 我的判断

   **方向乙**符合评审者的目标（"三种不同资源责任在 Rust 类型上确实成为三种不同的对象"），也是唯一能实现编译期隔离的路径。

   但它有一个必须先回答的问题：**`BorrowedPageTable` 没有 `destroy`，那它怎么"结束"？** 答案是不需要结束——它本来就不拥有任何需要归还的东西，`Drop` 什么都不做。这反而简化了 `from_active_root` 的 4 处生产调用点（`kerninfo.rs:143`、`lib.rs:1707`、`lib.rs:2928`、`vm_self_map.rs:102`），它们全都不调 destroy，改成 `BorrowedPageTable` 后一行代码都不用改逻辑。

   **`CallerManagedPageTable` 的终结动词不能叫 `destroy`**（采纳第四版评审 §2.2）：

   ```rust
   unsafe fn into_permanent_root(self) -> PhysBytes;   // 受 BorrowedSupervisor<'static> 限制
   ```

   它回收**自己 `map()` 过程中建的中间表页**，然后把根页的访问凭证还给调用者。名字里不出现"destroy"是刻意的:它没有销毁外部根页，只回收自己拥有的部分。

   #### 与 `CurrentPaging` 的关系

   两条方向都需要回答同一件事：`CurrentPaging` 现在是 `X86_64Paging` / `AArch64Paging` / `Riscv64Paging` / `SimPaging` 四个具体类型之一，`BootArch::Paging` 的 bound 是 `HugePages`（`os/arch/src/arch/paging_ext.rs:75` `pub trait HugePages: Paging`）。

   如果走方向乙，`Paging` 这个 trait 名要么保留为"纯操作"（把 `destroy` 摘出去），要么改名为 `PagingOps` 并给 `HugePages: PagingOps`。**前者改动面小得多**：只需把 `destroy` 从 `Paging` 移到新的 `OwnedPaging` trait 上，同时**把四个构造函数从 `Paging` 上摘走**（它们是"创建 handle"的入口，不属于"操作"能力）。

#### 两个必须一起改的泛型调用点

核实后确认，`new_from_page` 有两处**在泛型上下文里**被调用，它们决定了方案不能只改具体类型：

| 位置 | 形态 | 改法 |
|---|---|---|
| `os/kernel/src/lib.rs:1292` | `let mut paging = A::Paging::new_from_page(root_page);`（`A: BootArch`）| 需要 `A::Paging` 能满足 `CallerManagedPageTable` 的构造入口。要么给 `BootArch` 加一个 handle 关联类型，要么这里直接换成 `CallerManagedPageTable::<A::Paging>::new_from_page(...)` |
| `os/arch/src/arch/boot.rs:820` | `let mut paging = P::new_from_page(root);`（`P: Paging`）| 同上；`P` 的 bound 要从 `Paging` 收窄/换型 |

另外 `os/kernel/src/lib.rs:991` 的文档注释写着 "`P: HugePages` implies `P: Paging`, so we get `new_from_page`, `enable`, `map`"——**这句话本身就暴露了当前设计的问题**：它把"创建 handle 的能力"和"操作页表的能力"当成同一套 trait 提供的。改完之后这句注释必须重写。

2. **明确四个构造函数的 root 合同**。尤其 `new_from_page()`：根页不由默认 `pt_alloc` 自动接管释放责任（`CallerManaged`）。
3. **明确子树所有权**。`inherit_supervisor_half()` 不能只复制 PTE，还必须更新 handle 的所有权状态；回收只遍历私有子树。**不能一律跳过上半场**（见 4ter.3）。
4. **`destroy(&mut self)` → `destroy(self)`**。消费型签名降低重复销毁与销毁后继续使用的风险，但**不是完整的安全证明**（第三版评审 §4.2）：它不能证明没有别的 handle 引用同一棵树、没有 CPU 仍在走这棵树、TLB 与页表遍历者已失效、共享子树已从所有权图里排除。这些必须继续写在 `unsafe` 合同里。
5. **增加三架构回收测试**。第三版评审特别强调：**不能只数"释放了几个页"，还要验具体物理页的归属**——测试要能区分根页、私有中间页、共享中间页，确认没有误释放。
6. **最后处理失败路径与性能**。不在所有权修复里扩展成通用 allocator 事务系统（理由见 §4bis.5 结论）。

**为什么第 1 步仍排最前，但理由要改**：不是为了"零成本"，而是因为它是**唯一一个能同时关掉两条错误链**的改动——既关 §3.2 的共享子树误回收，又关 §3.3.3 的"清零活动根"。其余五步都是在收紧合同，而第 1 步是把一整类对象从危险能力里摘出去。

### 4ter.3 不能靠"所有时候都跳过上半场"来修

这是我看完两套并存策略后确认的**反例**，也是我认为问题 C 不能照搬 C 侧 `pt_free` 的原因。

| 路径 | 内核上半场页表页 | 谁分配 | 正确回收 |
|---|---|---|---|
| `Paging::new()` + `map_kernel`（`vmproc_handle.rs:364`→`:386`）| **自己分配**（`map()` 缺中间层时调 `alloc_pt_page`）| 本域 | **必须释放** |
| `new_from_page()` + `inherit_supervisor_half`（`boot.rs:818`→`:822`）| **共享 bootstrap**（PTE 值复制）| 别人 | **不能释放** |

**同一个 `free_child_tables` 面对两种对象。** 如果按"上半场是内核空间、内核空间不归 VM 管"去一律跳过 256..512，那么第一条路径的中间表页会全部泄漏——而那条路径是 VM 的**每进程常规路径**（`init_page_table` 每建一个进程页表就走一次）。

反之，如果一律不跳过，第二条路径就会打穿共享子树。

**所以修法必须是"handle 记住自己是哪一种"，不是"循环里改个边界"。** 这也解释了为什么问题 D（根页所有权）与问题 C（共享子树）在实践中总是同时出现：它们都是"handle 不知道自己是什么"的症状。

#### 4ter.3.1 共享状态用什么表示：语义枚举，不是 `bool`

第四版评审 §5 给了明确结论，我采纳：**按当前实现，一个二值语义枚举就够，不需要记录"共享到第几层"。**

理由（三条都来自评审，我核实后认同）：

- 三个架构都只在**根页的顶层条目**里复制固定范围：256..512；
- 共享关系只由一次 `inherit_supervisor_half()` 建立；
- 被共享的是整个顶层条目所指的子树。

所以释放时只需**在根页遍历时跳过这一组被继承的顶层条目**，不需要递归到更深层再判断共享状态。

```rust
enum SupervisorSubtree<'source> {
    Private,
    Borrowed(SharedPageTableSource<'source>),   // ← 带源树的稳定性凭证
}
```

**用语义枚举而不是 `bool`**，避免以后出现"`true`/`false` 与架构概念之间的隐式约定"。

**`Borrowed` 变体要带凭证，不只是一个标记**（采纳第五版评审 §4.3）。理由：复制 PTE 之后，目标页表不只依赖"一个抽象的共享状态"，还**依赖源页表及其子树继续有效**。如果只存一个 `Borrowed` 标记，源 owner 先结束生命周期时没有任何东西能阻止它——保存 `SharedPageTableSource<'source>` 至少把这份依赖连到源树的稳定性凭证上。

对当前永久存活的 bootstrap root，`'source` 最终是 `'static`，不给调用点增加实质负担。

如果将来根页所有者与共享源的生命周期无法自然合并，就**用两个生命周期参数**，不要为"只有一个参数"而牺牲真实关系。

**三个必须遵守的实现条件**：

1. `inherit_supervisor_half()` **只能在约定的构造状态执行**——建议禁止重复继承（当前代码没有禁止，重复调用会静默覆盖同一批条目）。
2. **继承发生时目标根的上半场必须为空**（采纳第五版评审 §6）。这是新加的一条，理由如下。

   若目标根原本已有本 handle 私自建立的上半场子树，而 `inherit_supervisor_half()` 直接覆盖对应的顶层 PTE，就会：

   | 步 | 后果 |
   |---|---|
   | ① 原本有私有上半场子树 | —— |
   | ② 继承覆盖顶层 PTE | 原有子树的**根入口**消失 |
   | ③ handle 把整个上半场标成 `Borrowed` | —— |
   | ④ 释放时按 `Borrowed` 跳过整个上半场 | **原有私有子树再也无法回收** |

   造成泄漏，甚至留下仍有其他引用的悬空结构。

   **当前已知路径满足这个条件，但靠的是构造路径而非方法自身**：`new_from_page()` 会清零根页，所以 `load_process_elf`（`boot.rs:818`→`:822`）的上半场必然为空。这是 `new_from_page` 提供的保证，不是 `inherit_supervisor_half` 的保证。所以要么在 `unsafe` 合同里写明，要么在实现里检查目标上半场没有 Present 条目。

3. `free_child_tables()` 必须在**根层遍历时直接跳过**共享顶层条目，**不能先递归进入、再只跳过最终的 `free_pt_page()`**。后者会在一次遍历里进入共享子树、把里层的私有页判断错。

   对应的根层释放逻辑（第五版评审 §6 末）：

   | 状态 | 遍历范围 |
   |---|---|
   | `SupervisorSubtree::Private` | 根页全部顶层条目 |
   | `SupervisorSubtree::Borrowed(_)` | 根层只遍历私有区域，**直接跳过**共享顶层条目 |

**什么时候需要更细的表示**：只有当日后允许"任意范围共享"或"多个来源的部分子树"时，才需要按根条目维护所有权掩码。当前不需要。

### 4ter.4 `load_process_elf` 的失败路径（第四版按评审重写）

**上一版这一节漏了一半。** 第三版评审 §5 指出：我只讨论了回滚 `VmBootAllocator`，忽略了 `paging.map()` 内部的中间表页走的是**另一套分配器**。核实后确认这条批评完全成立。

`load_elf_into` 用 `?` 传播错误（`boot.rs:825`），且：

- `Paging` / `PageTable` 在 `os/arch/src` 里**没有任何 `impl Drop`**（`grep -rn "impl.*Drop for" os/arch/src` 零命中）
- `VmBootAllocator` 无 `free`

所以任一步失败，**两套分配器**都已消耗，且都无回收：

| 分配来源 | 失败时已消耗什么 | 有回滚能力吗 |
|---|---|---|
| `VmBootAllocator`（`frame.rs:639`）| 进程根页、ELF 镜像页、栈页 | **没有**（无 `free`、无 `checkpoint`，`idx` 无读取入口）|
| `pt_alloc` → 当前注册的钩子 | 映射过程中新建的中间表页（`x86_64/paging.rs:949`、`:989` 等）| **看钩子**：测试内核注册 `boot_pt_alloc`，而它有 `resume`（§4bis.5）|

**两个反直觉的结论**：

1. **独占借用的 `VmBootAllocator` 反而没有 rewind 能力**，因为它的状态是 `{ regions, idx, cursor, used_frames }` 四项，一次分配跨 region 时 `idx` 会变；只回退 `cursor` 不够（第三版评审 §5.2）。
2. **非独占的 `BootAlloc` 反而有 checkpoint 能力**（`boot_alloc_next()` 读 + `resume_boot_pt_alloc()` 写，带一致性校验），但它不是事务——别的代码也能推进它。

**因此我采纳第三版评审 §5.4 的建议，不给 `VmBootAllocator` 加通用 `release_page()`。** 三个理由：

- 它本来就是单调、一次性的启动分配器；没有逐页释放能力不必然是缺陷
- 真要做事务，得先给 `VmBootAllocator` 加 `checkpoint()`/`rollback()`（含 `idx` 与 `used_frames`），再给 `BootAlloc` 加独占保证——而**当前没有任何生产重试需求**（唯一调用者是 QEMU 测试内核，失败即 `panic!`）
- 为理论上的释放引入一套通用 allocator 事务系统，成本远超收益

**第四版评审 §3.3 对"加断言"这条给了更严谨的替代，我采纳并改写。**

单独一条 `debug_assert` **并不能禁止调用者重试**——断言只能证明"我以为你不会重试"，不能阻止。真正有效的合同要落在**调用点**：

> `load_process_elf()` 返回错误后，调用者**必须终止本次启动流程**，不得继续使用已经消耗资源的启动分配器进行重试。

最好由唯一调用点（`test-sysboot/src/main.rs:254-260`）直接体现——错误转成启动失败，而不是返回给一个可能重试的上层策略。当前该调用点已经写成 `.unwrap_or_else(|e| panic!("test-sysboot: TX ELF load failed: {e:?}"))`（`:260`），**事实上符合这个合同**，只是没有写下来。

**改为文档化，不依赖断言**：

- 在 `load_process_elf` 的文档里写明"失败即放弃整个启动池，不支持重试"
- 明确记录"`pt_alloc` 侧消耗的中间表页在失败后同样不回滚"
- 不新增 `debug_assert`（它做不到评审要求的约束力）

#### 关于 BKL 能否作为独占证明（第四版评审 §3.1）

评审 §3.1 给了一个我可以走但不建议走的路：**如果 `load_process_elf` 全程持有 BKL，且所有能调 `alloc_pt_page()` 的路径都必须在同一把 BKL 下**，就能证明期间无人推进游标。

但评审同时点出两件不能混的事：

- `load_process_elf()` 的**调用者**持有 BKL；
- `BootAlloc` **自身**保证所有分配受 BKL 保护。

**前者不自动推出后者。** 核实后确认：`BootAlloc` 用的是 `AtomicU64`，`alloc()` 内部是 `load → 计算 → store` 三步，**完全不要求调用者持锁**（`boot_alloc.rs` 模块文档自己写着 "Treat the allocator as boot-only and single-threaded; a future SMP path needs a CAS loop or a per-CPU allocator"）。

评审的结论我照录：

> BKL 可以作为当前调用环境的证据，但不必为了这次评审把它提升为事务系统的基础设施。

所以：**不为了证明独占性开启大规模锁审查，也不把 `BootAlloc` 改成锁保护。** 当前测试场景的独占性来自"单执行流 + 调用点顺序"，这是环境事实，写进文档而不是写成类型约束。

**定性（采纳第三版与第四版评审）**：

> 失败路径的资源管理**没有明确合同**，值得写成文档；但在单次启动、单调 bump 的场景下，**未回收不自动等于运行期内存泄漏**，因此不升级为当前 P1 缺陷。**不回滚的决策正确**，且与 `BootAlloc` 已具备 checkpoint 接口**不矛盾**——有能力不等于该用。

---

## 4quater. 生命周期合同与收敛后的 API 形态（第六版新增，第五版评审要求）

第五版评审在冻结概念设计前要求补齐两件事。这一节是答卷。

### 4quater.0 收敛到的 API 形态（第五版评审 §2、§8）

第五版评审给了完整的结构建议。我采纳，并记录它对我上一版方案的四处具体修正。

#### 三层层分

```rust
// 第 1 层：只有操作，没有构造、没有销毁
trait Paging {
    fn map(&mut self, ...) -> Result<(), PageTableError>;
    fn unmap(&mut self, ...) -> Result<PhysBytes, PageTableError>;
    fn query(&self, ...) -> Option<(PhysBytes, PageFlags)>;
    // remap / update_flags / walk / split_huge / flush_tlb / map_range
}

// 第 2 层：架构后端，承载根页操作。实际应为 pub(crate) 或 sealed，
// 防止调用方绕过 wrapper 直接造出没有所有权约束的裸后端
trait PagingBackend: Paging {
    fn create_owned_root() -> Result<Self, PageTableError>;
    unsafe fn initialize_external_root(root: PhysBytes) -> Self;
    unsafe fn view_active_root(root: PhysBytes, channel: PteChannel) -> Self;
    unsafe fn inherit_supervisor_half(&mut self, source: PhysBytes);
    unsafe fn destroy_private_subtrees(&mut self, supervisor: SupervisorOwnership);
}

// 第 3 层：三个 handle，能力各不相同
pub struct OwnedPageTable<P>                { inner: P }
pub struct CallerManagedPageTable<'root, S, P> {
    inner: P,
    root: CallerManagedRoot<'root>,
    supervisor: S,                          // PrivateSupervisor | BorrowedSupervisor<'source>
}
pub struct BorrowedPageTable<'a, P>         { inner: P, root: BorrowedRootMut<'a> }
```

#### 四处对我上一版的修正

| 项 | 我上一版 | 第五版评审 | 采纳 |
|---|---|---|---|
| 后端是否也分层 | 只有 handle 与 `Paging` | **后端单独一层 `PagingBackend`**，保留三架构实现不动 | 是 |
| handle 是否实现 `Paging` | 隐含 | **三个 handle 都实现 `Paging`**（delegate 到 `inner`），这样 `map_kernel`/`load_elf_into` 等泛型函数不用改约束 | **部分采纳**：只实现 `Paging` 不够，`HugePages` 等扩展必须一并代理（§4quinquies.1h）|
| `CurrentPaging` | 想把它换成 handle | **保留为后端类型**，另加三个别名 `CurrentOwnedPageTable` / `CurrentCallerManagedPageTable<'a>` / `CurrentBorrowedPageTable<'a>` | 是 |
| `adopt_active_root` 命名 | 沿用 | `adopt` 让读者误以为发生了所有权转移，建议改为 `from_active_root_vm_dm()` 或显式 `PteChannel` 参数 | **是**：并入 `from_active_root(root, channel: PteChannel)`，`adopt_active_root` 名字退役（§4quater.0）|

#### 方法归属表（第五版评审 §2.3，我照录）

| 方法 | 归属 |
|---|---|
| `map` / `unmap` / `query` / `walk` | `Paging` |
| `new()` | `OwnedPageTable<P>` |
| `new_from_page()` | `CallerManagedPageTable<'root, PrivateSupervisor, P>` |
| `from_active_root(root, channel: PteChannel)` | `BorrowedPageTable<'a, P>`（原 `adopt_active_root` 并入此签名）|
| `inherit_supervisor_half()` | `CallerManagedPageTable<'root, PrivateSupervisor, P>` → `…, BorrowedSupervisor<'source>, P` |
| `destroy(self)` | `OwnedPageTable<P>` |
| `into_permanent_root(self)` | `CallerManagedPageTable<'root, BorrowedSupervisor<'static>, P>` |
| ~~`retire_owned_subtrees(self)`~~ | **未来预留，不属于当前公开 API**（§4quinquies.1f）|

**`inherit_supervisor_half()` 从通用 `Paging` 上摘走这一条我要单独记**：评审说得对——它不是普通映射操作，而是"建立共享子树关系，并改变当前 handle 的回收责任"。如果它留在所有 `Paging` 对象都能调的通用方法上，wrapper 就无法可靠维护自己的所有权状态。

底层架构仍需提供复制顶层 PTE 的机制（`PagingBackend::inherit_supervisor_half`），但那个机制**不应暴露成所有 handle 都能随意调用的公共接口**。

#### 未来 `retire_owned_subtrees` 需要说明"无 free 钩子"时的行为（第五版评审 §3.2；当前不实现，见 §4quinquies.1f）

这是我上一版完全没写的一处。要区分两件事：

| 动作 | 含义 |
|---|---|
| ① 清除私有子树的 PTE | 当前根不再引用它们 |
| ② 把这些物理页真正还给 allocator | 它们可以再次分配 |

**①不等于②。** 内核映像不注册 `PT_FREE`，所以 `pt_alloc::free_pt_page()` 在内核映像里是空操作（`uninit_free`）。

评审给的两种合同选择，我采纳后者：

> 若允许在 bump allocator 上结束 handle、但物理页只能永久消耗，那么必须明确这是 **abandon/retire**，不能让名称暗示物理页已归还。

**所以未来 `retire_owned_subtrees` 的文档必须写清**：它保证①，②取决于当前映像是否注册了 free 钩子。**不能让它在 VM 映像里真归还、在内核映像里静默不归还，而没有文档区分。** 这条约束现在就写下来，是为了避免将来实现时重新踩一遍。

当前没有调用需求，所以只需写进 `unsafe` 合同，**不为此设计新的 allocator trait**。

### 4quater.1 `ProcessLoad` 的所有权交接（第五版评审 §5、§9）

**评审的问题**：`ProcessLoad` 的成功结果究竟是"需要长期保有、以后可能销毁的页表对象"，还是"只在 QEMU 测试启动期间使用、整个启动池永久保留"？

**核实答案：两者都是，而且这正是问题所在。**

#### 事实链

`load_process_elf` 全仓只有 **1 个调用点**：`os/qemu-tests/test-kernels/kernel/bootstrap/test-sysboot/src/main.rs:254`（QEMU 测试内核）。生产内核**不走这条路径**（生产 VM 用 `load_vm_elf` 映射进 bootstrap 根，见 `os/kernel/src/lib.rs:2865`/`:2935`）。

但这个唯一调用点拿到 `tx` 之后做了三件事：

| 行 | 动作 |
|---|---|
| `:287` | `tx.root` 作为目标 PML4 物理地址，往里补抄内核槽位 |
| `:326` | `tx.pc` / `tx.sp` / `tx.ps_strings` 用来构建 RS 槽的 CPU 上下文 |
| `:332` | **`rs.p_seg.phys_root = tx.root`** —— 写进进程段描述符 |

**第 332 行是关键。** 从此这个根页不再只是"加载结果"，而是 **RS 进程的活地址空间**，被调度器的地址空间切换与 `syscall_copy` 系列（全仓 `p_seg.phys_root` 共 **82 处**读取）持续使用。

#### 所以"裸 `PhysBytes`"丢掉的到底是什么

按第五版 API，`CallerManagedPageTable<P>` 携带的信息有四项：

1. 根页是 `CallerManaged`（**不是** `pt_alloc` 的）
2. 私有子树是这个 handle 建的
3. 上半场是共享借用的
4. 将来回收时该跳过哪些子树

`ProcessLoad { root: PhysBytes, ... }` **四项全丢**。而 `p_seg.phys_root` 已经是 `PhysBytes`（`os/kernel/src/proc.rs:881`），所以这个丢失是**贯穿到底的**——从 handle 到进程表都没有任何地方记得"这个根不是 `pt_alloc` 发的"。

#### 我的判断

评审说"这一项比继续争论 `reclaim_owned_subtrees` 的命名更值得优先处理"——我同意，并且我认为核实结果让它的优先级**更高**，理由有二：

1. 这个根**真的会长期存活**（RS 进程活多久它就活多久），不是说"理论上有回收需求"；
2. 但**没有任何东西会去销毁它**：`os/qemu-tests` 的生产代码 `.destroy()` 零命中，`destroy()` 的 2 处生产调用都在 VM 映像且都在 `Paging::new()` 的根上。

**所以当前这是一个"只创建、不销毁"的对象。** 泄漏不存在（因为不销毁），所有权语义丢失也不产生故障（因为没人来收）——**但这是因为没有回收方，不是因为合同正确**。

#### 建议的两条路

**路一（如果 `ProcessLoad` 要泛型化并持 handle）**

```rust
pub struct ProcessLoad<P> {
    paging: CallerManagedPageTable<'static, P>,
    pc: VirBytes,
    sp: VirBytes,
    ps_strings: VirBytes,
    allocated_bytes: usize,
}

impl<P> ProcessLoad<P> {
    fn root_phys(&self) -> PhysBytes { self.paging.root_phys() }   // 需要裸地址时
}
```

`'static` 仅作示意，实际应绑定 bootstrap root 的所有者生命周期。

**代价**：`ProcessLoad` 从无泛型变泛型，`load_process_elf<P, A>` 的返回类型跟着变，`test-sysboot` 要改写 `tx.root`/`tx.pc` 的访问方式（`tx.root` 变 `tx.root_phys()`）。**而 `rs.p_seg.phys_root = tx.root` 这一行无论如何都要拿裸地址**——所以泛型化并不消除裸地址，只是让"谁有权给裸地址"变清楚。

**路二（如果保持裸 `PhysBytes`）**

必须明确写出评审给的那句话：

> 该路径将页表结构的生命周期交给整个启动池，不支持后续通过 `Paging::destroy()` 回收。

**我的倾向：路二，加上明确的文档**。理由：

- 唯一调用者是测试内核，它的根页进的是启动池（`VmBootAllocator`），本来就没有 free 方法（§4bis.2）
- 泛型化 `ProcessLoad` 会让一个只有 1 个调用者的 API 变复杂
- 真正需要 handle 语义的是**生产路径**（VM 建进程页表），而那条路径现在走 `Paging::new()`，根页来自 `pt_alloc`，所有权是清楚的

**但无论选哪条，都必须把这句话写进 `ProcessLoad` 的文档**——当前文档只说 "The caller records it in the process's segment descriptor"（`os/arch/src/arch/boot.rs:745-748`），没说过"不能回收"。

### 4quater.2 `BorrowedRoot` 说的"生命周期"必须是真的（第五版评审 §4.1）

**评审的技术要求，我完全接受**：

> 只给 `PhysBytes` 加一个 `PhantomData` **不能**建立借用关系。因为 `phys` 是一个整数地址，它与任何实际的 Rust 借用都没有联系。安全构造函数让编译器凭空推断 `'a`，并没有真正证明根页存活。

我上一版之所以想问"要不要现在就带 `'a`"，正是因为我隐约知道这个问题——**但当时的方案确实只是给 `PhysBytes` 套了个 `PhantomData`，那是假的生命周期**。

#### 当前有没有可以真实借出的所有者对象

核实答案：**没有。** 三个 `from_active_root` 生产调用点拿到的 root 来源：

| 调用点 | root 从哪来 | 有 Rust 所有者对象吗 |
|---|---|---|
| `os/kernel/src/kerninfo.rs:143` | `crate::current_root_phys()`（读 CpuLocal 里的 `PhysBytes`）| **没有** |
| `os/kernel/src/lib.rs:1707` | 同上的 `root` | **没有** |
| `os/kernel/src/lib.rs:2928` | `root_phys` 参数透传 | **没有** |
| `os/servers/vm/src/pagetable/vm_self_map.rs:102` | `VmSelfPageTable::adopt(root_paddr)` 的入参 | **没有** |

**四处全是裸 `PhysBytes`。** 内核的活动根登记在 `CpuLocal::root_phys`（`os/kernel/src/lib.rs:4152` 附近），那是一个 per-CPU 字段，不是可以借出的 Rust 对象。

#### 因此只能走评审的方式 B，而且要拆成两个凭证（第九版评审 §4.3）

单纯一个 `BorrowedRoot<'a>` + `PhantomData<&'a ()>` 有两个问题：

1. `PhantomData` **不建立真实借用关系**（第八版评审 §4.1 已指出）；
2. 它把"能修改活动根"与"保证共享源存活"两种不同权限混成一个凭证（第九版评审 §4.3）。

所以拆成两个：

```rust
/// 不拥有根页，但获得受同步合同约束的**修改能力**。
pub struct BorrowedRootMut<'a> {
    phys: PhysBytes,
    _exclusive: PhantomData<&'a mut ()>,
}

impl BorrowedRootMut<'_> {
    /// # Safety
    /// Caller must guarantee:
    /// 1. the root page and **every page-table page reachable through
    ///    the entries used by this handle** stay allocated and valid for `'a`;
    /// 2. the caller is entitled to modify this root;
    /// 3. other modifiers are coordinated through the same mechanism;
    /// 4. page-table changes, address-space activation, CPU participation
    ///    and TLB invalidation satisfy the architecture contract;
    /// 5. there is no uncoordinated concurrent modification.
    unsafe fn from_raw(root: PhysBytes) -> Self { ... }
}

/// 不拥有源树，只保留"源树有效且结构稳定"的**保活合同**。
pub struct SharedPageTableSource<'source> {
    phys: PhysBytes,
    _source: PhantomData<&'source ()>,
}

impl SharedPageTableSource<'_> {
    /// # Safety
    /// Same item 1 as `BorrowedRootMut` (the whole reachable tree),
    /// plus: the source root will not replace or release any shared
    /// top-level entry's subtree while inheritors exist (cf. §4quinquies.1c).
    unsafe fn from_raw(root: PhysBytes) -> Self { ... }
}
```

**`BorrowedPageTable` 用前者，`SupervisorSubtree::Borrowed` / `BorrowedSupervisor<'source>` 用后者。** 两者都走狭窄的 `unsafe from_raw`，都不需要全局 owner 表。

**这不是"消除不了 `unsafe` 的妥协"，而是当前资源形态的准确表达**：引导期根页的所有者信息只存在于调用点，没有 Rust 对象可借。

评审说的"不要为了消除所有 `unsafe`，硬造一个不反映真实资源所有权的 Rust owner 对象"——我照这一条办。

#### 一个真实的约束：`'a` / `'source` 现在只能填 `'static`

四个调用点借的都是**内核的永久活动根**（bootstrap 根 / VM 自举根），它们的实际存活期是整个内核运行期。所以在这些调用点上 `'a = 'source = 'static`。

**这意味着引入 `'a` 之后，这些调用点在类型上立刻就"诚实"了**——不是"我们猜它够久"，而是"我们声明它是 `'static` 并写了 `unsafe` 理由"。

### 4quater.3 第五版评审的其余裁决（记录，不再展开）

| 事项 | 评审裁决 | 我的处理 |
|---|---|---|
| `reclaim_owned_subtrees(self) -> CallerManagedRoot` | **值得**（第五版时的裁决）| 已被第八版降级为暂不实现，改名 `retire_owned_subtrees`（§4quinquies.1f）|
| `BorrowedPageTable` 现在就带 `'a` | **应该** | §4quater.2 已纳入 |
| 仅 `PhantomData` 能否证明物理页生命周期 | **不能** | §4quater.2 已改为 `unsafe fn from_raw` |
| 共享状态是否按层记录 | 不需要；但 `Borrowed` 最好保存源根凭证 | §4ter.3.1 已改为 `SupervisorSubtree::Borrowed(SharedPageTableSource<'source>)` |
| `inherit_supervisor_half` 前置检查 | **需要**：目标上半场必须为空，只允许一次继承 | §4ter.3.1 第 1 条已写，见下一节补充 |
| `ProcessLoad` 是否只存 `PhysBytes` | 若表需存活并可能销毁，**不应丢弃 handle** | §4quater.1 给了两条路与倾向 |
| 方向甲可作最终设计吗 | 不建议，只可作短期过渡 | §4ter.2 已标注 |
| 失败回滚、逐页映射重开吗 | 不需要 | 不动 |

---

## 4quinquies. 证据、合同与实施分期（第六至第八版评审逐轮增补）

第六版评审要两组材料，并提出四项需要纳入正式合同的细节。这一节是答卷。

### 4quinquies.1 组一：`VmBootAllocator` 的页如何进入物理内存保留合同

**这条时间线上有两处需要更正。** 第八版评审指出两处，核实后确认都成立，并且真实结构比先前设想的清楚。

#### 先纠正两处错判

**更正一**：先前写"再调一次 `load_process_elf`，第二个 TX 从同一个 free list 的剩余部分取页 → 也可能重叠"。

**这条不成立。** 若继续使用同一个 `VmBootAllocator` 实例，它的 `cursor` 已经前移，正常从剩余区间分配，不会重复得到第一次的页。正确的表述（采纳第八版评审 §1.1）：

> 若重新从同一份未更新的 `vm_pmm_free_regions()` 快照构造**新的**分配器，或真实 VM PMM 消费该快照，TX 已消耗的页面仍会被视为可用，存在重复分配风险。继续使用原来的 `VmBootAllocator` 实例则由其递减游标避免重复分配。

**风险来自"多个分配器消费同一份陈旧快照"，不是单个 bump allocator 的行为。**

**更正二**：先前把 `classify()` 看到的 allocator 与 `:254` 那个 `VmBootAllocator` 当成同一个实例，于是时间线不可能成立（`classify` 在 `:203` 内执行，而 `VmBootAllocator::new` 在 `:242`）。

**第八版评审 §1.2 指出得对。实际是两个不同的实例**，核实如下：

| 实例 | 创建位置 | 使用者 | 消耗是否进入 handoff 快照 |
|---|---|---|---|
| **VM 专用的 `vm_alloc`**（在 `init_proc_and_boot` 内）| `os/kernel/src/lib.rs:2922`（生产分支）| `load_vm_elf` 装 VM 镜像 | **是**。`classify()`（`:3043` 调 `build_vm_handoff`）读它的 `current_region_idx()`/`cursor()`，把消耗区间做成后缀 cut 掉 |
| **TX 专用的 `vm_alloc`**（在 `test-sysboot` 内）| `test-sysboot/src/main.rs:248` | `load_process_elf` 装 RS 镜像 | **否**。它在 `:242` 从已发布的快照建起来，之后消耗的帧从未被 cut |

**所以时间线是通的**：`classify` 用的是 `init_proc_and_boot` 内那个局部 `vm_alloc`（`lib.rs:2922`），它跟 `test-sysboot` 的 `:248` 那个是**两个独立对象**，各自消费同一批 region 的不同区间。

#### 但更重要的一点：`pt_alloc` 的中间表页从不在风险范围里

先前把 `BootAlloc` 的中间表页也列为"需要保留的页"。核实后发现**它们根本不在同一份名单里**：

`test-sysboot` 的 bump 区来自 **UEFI 分配服务**（`:129` `uefi_helpers::alloc_bump_region(512)` → `boot::allocate_pages(MaxAddress, LOADER_DATA)`）。

这里要精确一点，因为两种引导形态的保护来源不同：

| 引导形态 | bump 区在 memmap 里吗 | 靠什么排除出 free list |
|---|---|---|
| 真实 UEFI 腿 | **否**（`LOADER_DATA` 在 UEFI 的 conventional 列表里本来就不是 Available）| 两道：memmap 排除 + 整块 deduct |
| **`test-sysboot`** | **是**——它的 `MEMMAP` 是硬编码的两段 `[0, 0x200_0000) + [0x200_0000, 0x800_0000)`（`:103-106`），覆盖整个 128 MiB，不反映 UEFI 分配类型 | **只剩整块 deduct 这一道** |

而整块 deduct 是无条件执行的（`vm_handoff.rs:693-695`）：

```rust
if let Some((base, end)) = boot_bump {
    f(base, end.saturating_sub(base));      // ← 整块，不是已消耗前缀
}
```

`boot_alloc_region()`（`boot_alloc.rs:184`）返回 `BOOT_ALLOC.region()`，即 `(base, end)` **全区间**。

**结论：`pt_alloc` 的中间表页（以及整个 512 页 bump 区）被整块排除在 free list 之外，不管用了多少。** 这些页从来没有"可能被重新分配"的风险。

这也顺带说明了一件事：**`test-sysboot` 的保护完全依赖这一条整块 deduct。** 如果哪天有人把 `boot_alloc_region()` 改成分段粒度（只 deduct 已消耗前缀），那么在硬编码 MEMMAP 下 bump 区的未用部分就会重新变成"可用"，而 `BootAlloc` 随时可能把它们发出去。

#### 于是真正的风险范围缩小到一处

| 资源 | 来源 | 是否在 VM 的 free list 里 |
|---|---|---|
| TX 的根页、ELF 数据页、栈页 | `test-sysboot:248` 的 `VmBootAllocator` | **是**（从 `:242` 快照取，**未被 cut**）|
| TX 的私有中间表页 | `BootAlloc`（经 `pt_alloc`）| **否**（整块 bump 区已 deduct）|
| TX 的共享上半场 | 复制 bootstrap 根 PTE | 不适用（没有新页）|
| UEFI 分配的 bump 区本身 | UEFI `allocate_pages` | 否（不在 memmap）|

**唯一有风险的资源是"TX 从 `VmBootAllocator` 拿到的那些页"，即根页 + 镜像页 + 栈页。**

而且这块风险的成因现在非常清楚：`:242` 读的快照是 `:203` 发布的，`:248` 之后 TX 从该快照的区间里扣走了 `N` 个页，**这份扣减没有写回任何地方**。

#### 风险链（修正后的版本）

```
init_proc_and_boot (:203)
    └── 内部 vm_alloc（lib.rs:2922）消耗 VM 镜像的帧
    └── classify() 把这些消耗 cut 掉
    └── build_vm_handoff() 发布 VM_PMM_FREE          ← write-once

vm_pmm_free_regions() (:242)  →  一份不可变的快照

VmBootAllocator::new(快照) (:248)
    └── load_process_elf (:254) 从这份快照扣走根页+镜像页+栈页
    └── 这些页从未被 cut 出任何 free list
```

**后果**：若真实 VM PMM 用这份快照建 bitmap（`os/servers/vm/src/boot.rs:327-329`、`:351-354`），它会把 TX 已占的页判为可用 → 分配给别的进程 → 零页覆盖正在运行的 RS 镜像。

**当前为何没触发**：`test-sysboot` 的 VM 槽跑的是桩镜像 `sysboot-rx`（`os/qemu-tests/test-kernels/user/sysboot-rx/src/main.rs`，首行自述 "the receive half of the multi-process boot carrier... parks in `receive(ANY)`"），它不跑 `VmServer::new()`，所以不读 `handoff.free_regions`。

#### 定性（采纳第八版评审 §6.1）

| 维度 | 判断 |
|---|---|
| 当前是否触发 | **否** |
| 失效后果 | TX 页与 VM PMM 分配重叠，可能覆盖正在运行的镜像 |
| 是否只是所有权 API 问题 | **不是**，是物理内存分配快照的一致性问题 |
| 是否需要现在修复 | **真实 VM 消费该 free list 之前必须修复** |
| 是否阻止当前 handle API 工作 | **不阻止** |

**记录为独立的启动期内存分配一致性缺陷，不与页表所有权问题 C/D 合并。**

#### 修复方向（第八版评审 §6.2，不现在选定）

必须保证的是：

> `F_VM = F_handoff − T_post-handoff`，其中 `T_post-handoff` 是 handoff 发布之后由 TX 分配器消耗的全部帧。

三个可选方向：

1. 在发布 VM handoff **之前**完成 TX 加载；
2. 在 VM PMM 消费**之前**，把 TX 的全部已用物理区间从 free list 扣除；
3. 把 TX 使用的启动页分到一个**本来就不属于** VM PMM free list 的专用保留区间。

**关键要求**（评审原文）：不能让同一份旧快照被多个独立 allocator 当作当前可用集合。

### 4quinquies.1b 范围分类接口：共享上半场禁改的落地前提（第八版评审 §3）

第八版评审确认了我上一版提的遗留问题并给了正式方案。采纳。

#### 为什么后端必须提供这个接口

架构间的顶层划分不同（x86-64 PML4 条目、aarch64 L0 条目、riscv64 Sv39 L2 条目）。**通用层不能硬编码 256..512**，否则就把架构机制泄漏回共享层——这正是项目第一原则禁止的（"抽象语义，不描述机制"）。

所以判定必须由后端回答，通用层只表达语义：

```rust
enum PagingRangeClass {
    Private,
    Borrowed,
    CrossesBoundary,
}

trait PagingBackend: Paging {
    fn classify_range(range: VirtRange) -> Result<PagingRangeClass, PageTableError>;
}
```

各架构实现负责算顶层索引、判断范围是否跨界。**通用 handle 不需要知道 PML4 / L0 / Sv39。**

#### 所有修改入口必须统一走这条检查

`CallerManagedPageTable` 在 `PrivateSupervisor` 状态时可以修改整个根；转成 `BorrowedSupervisor<'source>` 之后，**所有**普通修改操作必须先过：

```rust
fn ensure_private_range(&self, range: VirtRange) -> Result<(), PageTableError> {
    match P::classify_range(range)? {
        PagingRangeClass::Private => Ok(()),
        PagingRangeClass::Borrowed | PagingRangeClass::CrossesBoundary
            => Err(PageTableError::PermissionDenied),
    }
}
```

**不能只覆盖 `map()`。** 至少要审查这些入口（第八版评审 §3.2 的清单，照录）：

| 操作 | 要求 |
|---|---|
| `map` / `map_range` | 检查目标范围 |
| `unmap` / `unmap_range` | 检查整个解除映射范围 |
| `remap` | 检查**源**与**目标**两个范围 |
| `update_flags` | 检查目标地址范围 |
| `grant_user_walk` | 检查目标范围 |
| `split_huge` | 检查被拆分的地址 |
| 其他会改页表结构或 PTE 的方法 | 一律纳入检查 |

**批量操作的额外要求**（评审明确点出，这条最容易被漏）：`unmap_range` / `map_range` 必须在**修改任何 PTE 之前**检查整个范围。不能先改私有区域、遍历到边界才发现跨入借用区域。

#### handle 是策略边界，backend 是机制边界

第六版说"三个 handle 都实现 `Paging` 并 delegate 给 inner"。方向对，但**具体含义必须修正**（第八版评审 §3.3）：

```
调用方
  ↓
CallerManagedPageTable::map()
  ↓
检查当前 Supervisor 状态 + 目标范围
  ↓
PagingBackend::map()
```

**不是**无条件的 `inner.map()`。**如果 wrapper 只负责转发，那些所有权与访问权合同就仍然只是文档。**

### 4quinquies.1c 源页表的修改合同：拆成三种变化（第八版评审 Q3）

先前只写了一句"源树在继承者存活期间必须稳定"。第八版评审指出这句话**过宽**——它会把真正需要保证的事情说得太大，也可能漏掉危险情况。核实后采纳它的三分拆解：

#### 三种变化，三种处理

| 变化 | 机制 | 处理 |
|---|---|---|
| **① 修改共享子表中的叶 PTE 或权限** | 目标根复制的是源根**顶层 PTE 的值**，两个根随后指向同一批下级页表页。源根通过这些共享页表页改叶 PTE，目标根也会看到 | **可以有意图地做，不应一概禁止。** 但必须满足共同同步合同 |
| **② 替换源根自己的顶层入口** | 目标根有自己的根页，继承时只复制了顶层 PTE 值。源根后来替换那个 PTE，**目标根不会自动收到** | **目标根继续引用旧子树**——即使旧子树暂未释放，也是**语义陈旧**；若被释放或复用，变成**悬空** |
| **③ 释放或复用任何仍可从目标根继承入口到达的页表页** | —— | **必须禁止**，除非已解除所有继承关系，或有能同步更新所有继承者的机制 |

#### 采纳的精确合同（评审原文）

> 继承者存活期间，源根不得使继承者持有的顶层入口变成悬空引用。共享子树内部允许在统一同步合同下修改；但不得释放或复用仍可能由继承者到达的页表页。若源根替换自己的顶层入口，必须保证旧子树在继承者不再引用前仍保持有效；若要求继承者看到新映射，则必须通过共享子树内部更新或明确的继承者同步机制实现。

#### 关于 TLB：锁不能替代失效合同

评审特别强调一条，我照录：

> 只拿着源根的 `&mut`，或者只持有 BKL，**并不能自动证明所有使用者的旧翻译都已失效**。

修改共享子表要协调的**不只是修改源根的 CPU**，还包括所有可能使用这些共享子树的地址空间与 CPU，以及各架构要求的 TLB 失效。**BKL 存在不等于 TLB 合同成立。**

#### 当前可以不建登记表，但成立条件要落实到具体路径

不引入全局共享子树登记表或通用引用计数（评审不要求）。但"当前不需要引用计数"的依据必须是这四条，**不能泛化成"所有未来调用者都能靠源根自律"**：

| # | 依据 | 当前状态 |
|---|---|---|
| 1 | 共享源限定为**永久存活**的 bootstrap 根 | 有据 |
| 2 | 共享子树页来自**永久保留**的引导期分配区，不会被 allocator 释放或重新分配 | **较有依据**（§4quinquies.1 已证 bump 区整块 deduct）|
| 3 | 源根允许的后续修改**经核实不会让继承者持有失效入口** | **未闭合** |
| 4 | 对共享页表内容的修改遵守覆盖所有使用者的**同步和 TLB 合同** | **未闭合** |

#### 第 3、4 项需要核实什么（评审列的清单，照录）

`harden_kernel_text`（`lib.rs:1707`）与 heap-window eviction（`lib.rs:2928`）**不能只记录它们调用过 `split_huge` / `update_flags` / `unmap`**，还要确认：

- 写入的是**共享子树里的某个下级页表页**，还是**源根自己的顶层入口**？
- 是否会**清除一个仍被继承者根页直接引用的入口**？
- 释放或复用的物理页**是否可能仍在继承者的可达集合内**？
- TLB 失效**覆盖哪些地址空间和 CPU**？

**结论（评审原文）**：

> 不建登记表，但 Q3 必须作为共享 handle 接入生产路径前的**验证门**。如果核实发现这些路径可能替换或释放继承者仍依赖的结构，不能靠补一句 SAFETY 文档解决；届时要么约束这些操作，要么引入明确的解除继承/同步机制。

### 4quinquies.1d 两种凭证必须分开（第八版评审 §4.3）

我上一版把 `BorrowedRoot` 定义成"独占修改权 + 不拥有释放责任"，然后**同时用于两个场合**。第八版评审指出这是概念冲突，我接受：

| 用处 | 需要什么权限 |
|---|---|
| `BorrowedPageTable` 包装活动根并修改其 PTE | **修改授权** + 调用者负责同步其他 CPU 与页表使用者 |
| `SupervisorSubtree::Borrowed` 保留源根凭证 | 只需**源树存活与结构稳定**的保证；**不应因此获得修改源根的权限** |

所以拆成两个类型：

```rust
struct BorrowedRootMut<'a> {
    // 不拥有根页，但获得受同步合同约束的修改能力
}

struct SharedPageTableSource<'source> {
    // 不拥有源树，只保留"源树有效且结构稳定"的合同
}
```

两者都走狭窄的 `unsafe from_raw`，都不需要全局 owner 表。`BorrowedPageTable` 用前者，`SupervisorSubtree::Borrowed` 用后者。

**这不是增加所有权状态机，而是避免"能修改活动根"与"保证共享源存活"被同一个凭证混为一谈。**

### 4quinquies.1e `into_permanent_root` 必须限制为 `'static` 源（第八版评审 §4.1、§4.2）

这是我上一版的一个**类型漏洞**：

```rust
unsafe fn into_permanent_root(self) -> PhysBytes;   // ← 丢掉了 'source
```

消费 handle 后 `BorrowedSupervisor<'source>` 被丢弃，只返回裸物理地址。**但目标根的上半场仍含指向源子树的 PTE**，所以目标根的有效期仍依赖源页表树。若 `'source` 不是 `'static`，类型就不再表达这个物理依赖。

采纳评审给的限制：**只允许永久源**。

```rust
impl<'root, P> CallerManagedPageTable<'root, BorrowedSupervisor<'static>, P> {
    unsafe fn into_permanent_root(self) -> PhysBytes;
}
```

这条限制表达了一个有价值的约束：**只有源页表及其可达子树确实永久有效时，才能把仍引用它们的目标根转成不带生命周期的裸地址。**

将来若需支持非 `'static` 源，则应返回携带 `'source` 的根凭证，或让调用者通过另一条明确的 `unsafe` 转换承担永久保留责任。

**当前 `test-sysboot` 的 bootstrap 场景不受影响**（bootstrap 根是 `'static`）。

### 4quinquies.1f `reclaim_owned_subtrees` 降级为暂不承诺（第八版评审 §2）

第八版评审不接受"只写文档差异表"作为最终 API 合同，但也不建议为一个无生产调用者的方法引入回收策略抽象。它给的两步方案我采纳：

**第一步（当前实施）**：`reclaim_owned_subtrees` **暂不公开、暂不实现**。只实现实际需要的三个：

- `OwnedPageTable::destroy(self)`
- `CallerManagedPageTable::into_permanent_root(self)`（`'static` 源限制）
- `BorrowedPageTable` 的非拥有操作能力（含范围检查）

**第二步（将来有真实调用点再做）**：显式报告物理页处置结果，并采用 `retire` 而非 `reclaim` 命名：

```rust
enum RetireDisposition {
    ReturnedToAllocator,
    PermanentlyRetired,
}

struct RetireResult<'root> {
    root: CallerManagedRoot<'root>,
    disposition: RetireDisposition,
}

unsafe fn retire_owned_subtrees(self) -> RetireResult<'root>;
```

**评选用 `retire` 而非 `reclaim` 的理由我认同**：

| | 含义 |
|---|---|
| `retire` | 从当前页表树解除资源使用关系 |
| `ReturnedToAllocator` | 物理页确实重新进入分配器 |
| `PermanentlyRetired` | 不再被页表引用，但没有重新进入分配器 |

**这比一个叫 `reclaim` 的方法再在文档里解释"有时真回收、有时只是不用了"更诚实。**

评审同时提醒：返回枚举也**不能**凭空证明 free 回调真执行了正确释放，`PT_FREE` 的注册合同仍须保证回调与分配来源配对。

**本轮不实现这个枚举，也不新增 allocator trait。**

### 4quinquies.1g `BorrowedRootMut::from_raw` 的 SAFETY 合同（第八版评审 §5）

第八版评审指出一个我前几版隐含假设的错误：

> `&mut BorrowedPageTable` 只能约束这个 Rust handle 的独占访问，**不能自动约束同一个物理根的其他 handle、其他 CPU 或硬件页表遍历器**。

例如两个 handle 都能从同一个 `root` 构造并各自拿到 `&mut self`，它们可能同时修改同一张页表。

所以 SAFETY 合同不能只写生命周期与物理页有效性，必须包括四条（照录）：

1. 调用者**有权修改**该根；
2. 其他修改者已经通过**同一同步机制**协调；
3. 页表修改与地址空间激活、CPU 参与集合、TLB 失效满足架构合同；
4. **不会出现未经协调的并发修改**。

按本项目的执行模型，实施时需分别核对：

| 场景 | 同步依据 |
|---|---|
| 内核侧四个调用点 | 相应路径是否始终受 BKL 或其它同步机制保护 |
| VM 自映射路径 | 单线程事件循环是否覆盖所有操作及其生命周期 |

**不为此再做一次全仓锁审计**（评审明确不要求），但**四个实际调用点的同步依据必须在实施记录中留下**。

### 4quinquies.1h 扩展 trait 必须一并代理（第八版评审 §「Paging 以外的公开修改能力」）

第八版评审指出我 v8 有一处断言不成立：**"三个 handle 都实现 `Paging`，所以 `map_kernel`/`load_elf_into` 的泛型约束不用改"**。

核实后确认这句话是错的，而且有一个直接反例：

| 事实 | 位置 |
|---|---|
| `map_kernel<P: Paging>` 的约束确实只要求 `Paging` | `os/arch/src/arch/paging.rs:630` |
| **但 `build_bootstrap_root_and_enable` 里的 `paging.map_huge(...)` 用的是 `A::Paging`** | `os/kernel/src/lib.rs:1340`（`match paging.map_huge(VirBytes(addr), PhysBytes(addr), PageSize::MIB_2, piece_flags)`）|
| `BootArch::Paging` 的 bound 是 **`HugePages`**，不是 `Paging` | `os/arch/src/arch/boot_arch.rs:22` `type Paging: HugePages;` |
| `HugePages: Paging` 且另有 `map_huge` / `supports_huge_page` 等方法 | `os/arch/src/arch/paging_ext.rs:75` |

所以若 `new_from_page` 返回 `CallerManagedPageTable<P>` 而该 handle **只** 实现了 `Paging`，`lib.rs:1340` 这一行**编译期就失败**。

**结论**：handle 必须代理**所有**扩展 trait，不只是 `Paging`。至少 `HugePages`（`map_huge` / `supports_huge_page` / 三个关联常量），以及 `os/arch/src/arch/paging.rs` 里 `map_kernel` 之外任何被 `A::Paging` 调用的能力。

#### 采纳的 API 合同（第八版评审原文）

> 所有公开的页表结构或 PTE 修改入口，无论来自 `Paging`、`HugePages` 还是其他扩展 trait，都必须经过 handle 的授权检查。内部 backend 不得通过公开接口绕过该检查。

**这带来两个具体要求**：

1. **`HugePages` 的代理不能只是转发**——`map_huge` 会改页表结构，同样要过 `ensure_private_range`。
2. 要么给每个 handle 实现相应扩展 trait并委托，要么调整具体 helper 的泛型约束。**不能把"handle 实现了 `Paging`"等同于"现有所有泛型调用都不用改"。**

**这不推翻三层 API**，只是把 wrapper 必须覆盖的能力范围定义完整。

### 4quinquies.1i 根层跳过共享条目也归 `PagingBackend`（第八版评审 §「根层跳过」）

v8 只解决了"通用层不能硬编码 256..512"这件事的一半。另一半是：

**释放私有子树时，由谁识别并跳过共享的顶层入口？**

通用 handle 只知道自己是 `PrivateSupervisor` 还是 `BorrowedSupervisor<'source>`，**它不应该知道 x86-64 PML4 / aarch64 L0 / Sv39 L2 的顶层索引**。

所以不能让通用层自己遍历根页再按 256..512 跳过。应由后端提供内部机制：

```rust
unsafe fn destroy_private_subtrees(
    &mut self,
    supervisor: SupervisorOwnership,     // Private | Borrowed
);
```

合同（照录评审）：

- **`PagingBackend` 根据架构决定哪些根层条目属于继承范围**；
- `Borrowed` 时从根层**直接跳过**共享条目；
- 通用 handle 只传递语义状态；
- 后端**不对共享条目指向的子树递归，也不释放它们**。

**`classify_range` 不能代替这个接口**：它判断的是**虚拟地址范围**，而递归回收要判断的是**物理页表根层入口**。二者相关，但不是同一个问题。

### 4quinquies.1j `OwnedPageTable::destroy` 也有物理处置差异（第八版评审 §1）

第八版评审指出：`retire` 被延后了，但 `destroy(self)` **仍然存在同类问题**。

| 所在映像 | `PT_FREE` | 当前 `destroy()` 的效果 |
|---|---|---|
| VM 映像 | 已注册 | 遍历并释放私有子表，归还根页 |
| 内核映像 | 未注册 | 清零根页后返回，**不递归归还子表** |

对 `OwnedPageTable` 而言这**不再造成"把外部根页交给错误 allocator"**——它的根页和中间页本来就来自 `pt_alloc`。**但物理处置结果仍然不同。**

采纳评审的处理：

**保留 `destroy(self)`，不增加 `DestroyOutcome`，但必须写清**：

> `destroy` 保证页表对象不再由该 handle 管理；物理页是否重新进入 allocator，**取决于当前映像提供的分配/释放合同**。没有释放能力时，页表页**永久消耗**。

**外加一个 Rust API 问题**（评审提出，我认同）：`OwnedPageTable` 如果没有 `Drop` 自动释放，直接 drop handle 就会遗失显式销毁机会；而销毁还要求 CPU 停止使用、TLB 同步等条件，**不能简单在 `Drop` 里无条件释放**。所以：

| 决定 | 理由 |
|---|---|
| `destroy(self)` 保持**显式**操作，保留 `unsafe` 条件 | 销毁的前提（CPU 停用、TLB）无法自动判定 |
| 普通 `Drop` **不执行物理回收** | 同上 |
| 用 `#[must_use]` + 文档明确"丢弃未销毁的 owned handle 会遗留页表资源" | 防止把普通 drop 当成 destroy |

### 4quinquies.1k `load_process_elf` 必须真正执行显式交接（第八版评审 §4）

v8 引入了 `into_permanent_root` 这个动词，但**现有流程片段仍然直接返回裸地址**。若只补文档不改代码，就会出现：类型系统里有了 `SharedPageTableSource` 与 `into_permanent_root`，生产代码却绕开这两个边界、直接用裸地址构造和返回。

采纳评审要求，实施时改成：

```rust
// 伪代码：实际类型与错误处理按最终 API 调整
let root = unsafe { paging.into_permanent_root() };

Ok(ProcessLoad {
    root,
    // ...
})
```

**并且 `load_process_elf` 接收的共享源不应再是裸 `PhysBytes`**，而应由调用者在明确的 `unsafe` 边界创建 `SharedPageTableSource<'static>` 再传入继承操作。

### 4quinquies.1l `into_permanent_root` 的 SAFETY 必须分两条（第八版评审 Q5）

`unsafe fn into_permanent_root(self) -> PhysBytes` **不只丢弃 `'source`，也丢弃 `CallerManagedRoot<'root>`**。所以 SAFETY 要分别说明：

| 条 | 内容 | 当前靠什么 |
|---|---|---|
| ① | 源根及其可达共享子树确实永久有效 | `BorrowedSupervisor<'static>` 的 bound |
| ② | **目标根页及其私有子树由调用者永久保留**，不得在目标进程地址空间仍可能使用时释放或复用 | **调用点承担，不是类型保证** |

**第二条不能被第一条自动覆盖。** 对当前 `test-sysboot` 路径，这个责任应与 TX 专用保留区方案（§4quinquies.1m）一起落实；在真实 VM PMM 可能消费同一批页之前，**不能只凭 `VmBootAllocator` 没有 `free()` 就宣称永久保留成立**。

### 4quinquies.1m TX 专用保留区：方案与实际容量核查（第八版评审 Q4）

第八版评审赞成"专用保留区"方向，并给了一个关键的落地约束：

> 要避免把"专用保留区"实现成另一个名字不同、实际仍从 `vm_pmm_free_regions()` 构造的 `VmBootAllocator`。那样只是把问题搬了位置。

建议形态（评审原文）：

```
Boot reserved region
├── PT allocator range       → 注册给 pt_alloc / BootAlloc
└── TX image range           → 专供 load_process_elf
```

外层完整保留区间仍被 handoff 的 memmap 分类逻辑排除。于是：

- `pt_alloc` 只在自己的子区间分配中间表页；
- TX allocator 只在自己的子区间分配根页、ELF 页、栈页；
- **VM PMM 的 free list 根本不包含这两个子区间**；
- 不需要向 PMM 解释 TX 后来消耗了哪些页，也不需要多个 allocator 同步维护一份可变快照。

**一个不能想当然的实现细节**（评审明确点出）：`boot_alloc_region()` 当前返回整个 bump 区间。**不能让 `BootAlloc` 的游标仍能越过 PT 子区间边界进入 TX 保留区。** 最好让 `BootAlloc` 初始化时直接拿到自己的子区间，而 handoff reservation 仍记录整个外层保留区间。

#### 实际容量核查（第八版评审要求，我按现有数据算）

| 资源 | 当前容量 | 依据 |
|---|---|---|
| `test-sysboot` 的 bump 区 | **512 页**（2 MiB）| `:129` `uefi_helpers::alloc_bump_region(512)` |
| `PT_BUMP_PAGES`（两个 `bootface` 腿） | 256 页（1 MiB） | `bootface.rs:134`、`bootface_a64.rs:100` |
| TX 的 ELF + 栈实际占用 | `ProcessLoad::allocated_bytes >= 256 KiB`（测试断言）| `os/arch/src/arch/boot.rs:1189` `assert!(a.allocated_bytes >= 256 * 1024 + 0x1000 + 0x1000)` |

**512 页 = 2 MiB 的 bump 区要同时装 PT allocator（引导期全部页表页）+ TX 镜像（≥256 KiB + 根页 + 栈）+ VM handoff frame。**

这里**不能下结论说不够**，因为 `test-sysboot` 只在 `arch_boot_impl` 之后才消耗 `pt_alloc`，而引导期上半场映射的页表页数量我没实测。**但这正是评审说的"当前材料没有容量数据，因此还不能宣称方案三已经具备直接落地条件"**——需要实测。列为实施前置任务（§5 Q4）。

### 4quinquies.2 组二：四个活动根调用点实际调了哪些 `Paging` 方法

第六版评审要区分"只查询 / 修改 PTE / 映射 / 是否涉及 TLB 同步"。逐个核实：

| 调用点 | 实际调用的方法 | 是否修改页表 |
|---|---|---|
| `os/kernel/src/kerninfo.rs:143` | `query(...)`；随后 `map(...)` | **是**（`map` 装 kerninfo 页）|
| `os/kernel/src/lib.rs:1707`（`harden_kernel_text`）| `split_huge(...)`；`update_flags(...)` | **是**（拆大页 + 改权限）|
| `os/kernel/src/lib.rs:2928`（heap-window eviction）| `query(...)`；`split_huge(...)`；`grant_user_walk(...)`；`unmap(...)` | **是**（拆页 + 解映射）|
| `os/servers/vm/src/pagetable/vm_self_map.rs:102` | `root_paddr()`；`map(...)`；`unmap(...)`；`query(...)`；`unmap_range(...)` | **是**（VM 自映射增删）|

**四个全部会修改页表。没有一个只查询。**

#### 因此 `Borrowed` 绝不能是只读

第六版评审 §3.3 的结论成立，而且比它推测的更确定：

> `BorrowedPageTable` 的 `Borrowed` 表达的是"不拥有释放责任"，不代表"只读访问"。

既然四个调用点全都要写，这个 handle 必须是**非拥有但可修改**的形态。它需要的 `unsafe` 合同包括（评审列的三条，我照录）：

1. 没有未经协调的其他修改者；
2. 相关 CPU 与页表使用者遵守同步协议；
3. 修改后的 TLB 失效按架构要求完成。

**并且 `PhantomData<&'a ()>` 那种共享借用标记表达不了独占修改权**——所以 `BorrowedRoot` 的凭证语义必须是"独占修改权 + 不拥有释放责任"，不是"共享只读借用"。

### 4quinquies.1n `PrivateSupervisor` 也需要终结路径，且 `enable` 要归位（第十版评审 §2.1）

这是第十轮评审认为第九版**最值得再补一笔**的地方，核实后同意：它指出了 A 阶段会立刻卡住的缺口。

#### 缺口

`new_from_page()` 有**两个**生产调用路径：

| 路径 | 位置 | 继承上半场？ | 第九版给的状态 |
|---|---|---|---|
| `build_bootstrap_root_and_enable`（建内核 bootstrap 根）| `os/kernel/src/lib.rs:1292` | **否** | `CallerManagedPageTable<'root, PrivateSupervisor, P>` |
| `load_process_elf`（建 TX 根）| `os/arch/src/arch/boot.rs:820` | **是**（`:824`）| `CallerManagedPageTable<'root, BorrowedSupervisor<'static>, P>` |

第九版只为**第二条**定义了 `into_permanent_root`。**第一条没有出口**，但 bootstrap 根同样要长期保留、且最终成为活动根（`set_current_root_phys(root_page)` 在 `lib.rs:343` / `:1486`）。

#### 补齐：两条路径都提供受约束的实现

```rust
impl<'root, P> CallerManagedPageTable<'root, PrivateSupervisor, P> {
    /// SAFETY: 调用者永久保留该根页及其私有子树。
    unsafe fn into_permanent_root(self) -> PhysBytes;
}

impl<'root, P> CallerManagedPageTable<'root, BorrowedSupervisor<'static>, P> {
    /// SAFETY: 上述条件 + 共享源及其可达子树永久有效（由 'static bound 表达）。
    unsafe fn into_permanent_root(self) -> PhysBytes;
}
```

**两者 SAFETY 不同，不能合并成一个无约束的泛型实现。** 评审核可"用经过约束的 sealed trait 合并两种实现"——实现时按哪种方便走哪种，但**两种状态都必须有出口**。

#### `enable` 的归属必须进 A 阶段清单

第九版补了 `HugePages` 代理，但**没明确 `enable` 在新 API 里归谁**。核实实际使用：

| 事实 | 位置 |
|---|---|
| `unsafe fn enable(&self) -> PhysBytes` 是 `Paging` 的方法 | `os/arch/src/arch/paging.rs:339` |
| `build_bootstrap_root_and_enable` 的函数体确实调用它 | `os/kernel/src/lib.rs:1285` 起（函数名即 `…_and_enable`）|

评审判断照录：

> `enable` 属于**地址空间激活能力**，应通过 handle 或明确的架构激活接口进行，**不能因为它不是 PTE 修改就遗漏**。激活后的根页和页表子树仍由调用者永久保留。

所以 A 阶段的方法清单要包含三类，不只是"PTE 修改"：

| 类别 | 方法 | 是否过范围检查 |
|---|---|---|
| 页表修改 | `map` / `unmap` / `remap` / `update_flags` / `map_huge` / `split_huge` / `grant_user_walk` | **是** |
| 查询 | `query` / `root_paddr` / `walk` | 否 |
| **地址空间激活** | **`enable`** | 否（不是范围操作），但**必须经 handle 或明确激活接口**；激活后根页与子树责任仍在调用者 |

### 4quinquies.1o 共享子树回收测试走 backend 内部入口（第十版评审 §2.2）

第九版有一处实施逻辑不一致：决定当前不公开 `retire_owned_subtrees`，但六项测试里又要求验证"继承后的目标根销毁私有子树时，共享子表物理页不得进入释放回调"。

第十轮评审的解法采纳：这项测试**仍然做**，但明确它是 **backend 内部单元测试**，不需要恢复公开 API。

```rust
// 测试内直接调 backend 的内部机制
destroy_private_subtrees(&mut backend, SupervisorOwnership::Borrowed);
```

测试形态：用受控的模拟页分配器记录每个物理页的来源，然后直接检查 backend 是否跳过了共享顶层入口。

**这样同时做到三件事**（评审原文）：

- 实现并验证三架构的根层跳过机制；
- 不为尚无生产调用点的 `retire` 增加公开 API；
- 将来真正实现 `retire` 时**复用已经验证的机制**。

一句话：**机制现在实现和测试，面向调用者的终结 API 暂缓。**

### 4quinquies.1p `#[must_use]` 不是销毁保证（第十版评审 §2.3）

第九版的措辞需要收紧。评审指出：`#[must_use]` 能提醒调用者不要直接忽略创建结果，**却不能保证一个已绑定变量、使用过、随后离开作用域的 handle 一定调用了 `destroy()`**。

采纳评审给的措辞：

> `#[must_use]` 用于提醒调用者显式处理页表资源；**它不构成销毁保证**。普通 `Drop` 不执行物理回收，未调用 `destroy()` 而丢弃 handle **会留下资源消耗**。

并明确一条：**不要为了让 `Drop` 强制调用销毁而引入一套难以证明安全的自动清理机制。**

### 4quinquies.3 四项纳入正式合同的细节

#### （1）`PagingBackend` 不能成为绕过 handle 的第二套公共构造 API（评审 §1）

采纳。两条具体约束：

- `PagingBackend` **`pub(crate)` 或 sealed**，`initialize_external_root` 等入口不对外。
- **`CallerManagedRoot.phys` 必须与后端 `root_paddr` 一致，且只能由唯一私有构造入口建立。**

第二条是我上一版没写的一个真实风险：`CallerManagedPageTable` 里 `root: CallerManagedRoot<'a>` 与 `inner: P` 都可能保存根页物理地址。若两者能各自独立构造，就会不一致。约束是：**两个字段只能由一个私有构造函数一次写入，之后不可独立替换。**

#### （2）`BorrowedRootMut` / `SharedPageTableSource` 的 SAFETY 都要覆盖整棵可达页表树（评审 §3.1；第九版拆成两个凭证）

采纳，SAFETY 合同原文照录：

```rust
/// # Safety
/// The caller guarantees that the root page and every page-table
/// page reachable through the entries used by this handle remain
/// allocated and valid for `'a`.
```

**`every page-table page reachable` 这个措辞是关键**——我上一版只写了"根页有效"，那不够：根页活着但下面的某个 PDPT/PD/PT 被释放或复用时，遍历照样不安全。

对 `SharedPageTableSource<'source>` 还要额外保证（评审 §3.1）：

- 被复制的根条目及其后续子表在借用期内不能被释放或复用；
- 源树结构若发生修改，必须遵守共享页表的同步合同；
- 若某个源子树被解除映射或替换，**目标根的对应条目也必须得到一致处理**。

#### （3）两个生命周期参数：`'root` 与 `'source`（评审 §4）

采纳评审的推荐。**但理由要按它自己的纠正写**：不是因为"两个资源来自不同 owner"，而是因为**终结时根页凭证不该被缩短**。

```rust
unsafe fn retire_owned_subtrees(self) -> RetireResult<'root>;      // 未来预留
```

返回的凭证只依赖 `'root`。若强行共用一个 `'a = min('root, 'source)`，那么"共享源已销毁"这个事件会**无必要地**让仍然有效的根页凭证失活。

**类型状态也采纳**（评审 §4.2）：

```rust
struct PrivateSupervisor;
struct BorrowedSupervisor<'source> { source: SharedPageTableSource<'source> }

struct CallerManagedPageTable<'root, S, P> {
    inner: P,
    root: CallerManagedRoot<'root>,
    supervisor: S,
}
```

构造返回 `CallerManagedPageTable<'root, PrivateSupervisor, P>`；`inherit_supervisor_half()` 消费 `self` 返回 `<'root, BorrowedSupervisor<'source>, P>`。

**副产品：重复继承变成类型错误。** 但评审提醒的一点我也记下：类型状态保证不了底层物理页没有残留 Present 条目，所以"目标上半场必须为空"的检查仍然要做。

**不做更多层级**（评审 §4.2 末）：只这两种状态，不引入更多类型状态机。

#### （4）`ProcessLoad` 的出口要有显式动词（评审 §2.3）

采纳。当前是从"局部变量离开作用域"隐式丢弃，改成可审计的边界：

```rust
impl<'r, 's, P> CallerManagedPageTable<'r, 's, P> {
    /// 结束 handle 管理，把整棵页表树永久保留。
    /// 不释放根页或私有子树。调用者承担永久保留责任。
    unsafe fn into_permanent_root(self) -> PhysBytes;
}
```

三个终结动词的语义区分（评审列的，我照录并补第四行）：

| 动词 | 归属 | 释放根页 | 回收私有子树 | 保留整棵树 |
|---|---|---|---|---|
| `destroy(self)` | `OwnedPageTable` | **是** | **是** | 否 |
| ~~`reclaim_owned_subtrees(self)`~~ | **未来 `retire_owned_subtrees`，当前不实现** | 否 | 是 | 否 |
| `into_permanent_root(self)` | `CallerManagedPageTable` | 否 | 否 | **是** |
| （无终结动作）| `BorrowedPageTable` | 否 | 否 | 否（借来的，本来就不处置）|

**建议名字可取 `into_permanent_root`，但"permanent"的成立性依赖 §4quinquies.1 的保留合同**——所以这个名字本身也在提醒调用者：用它可以，但要保证那些页真的永久保留。

### 4quinquies.4 未来 `retire_owned_subtrees` 返回前必须清除共享条目（评审 §6；当前不实现）

采纳。这是我上一版漏掉的一步，而且它是逻辑闭合的必要条件。

若不清除：

| 步 | 状态 |
|---|---|
| ① handle 结束 | —— |
| ② `SharedPageTableSource<'source>` 凭证丢弃 | 共享依赖的**类型层**保证消失 |
| ③ 返回的根页仍保留指向共享子树的 PTE | **物理层依赖仍在** |
| ④ 根页 owner 拿到一个仍依赖已失效共享源的根 | **依赖没有真正结束** |

**正确的顺序**（评审 §6 给的，我照录）：

```
确保目标根不再被任何 CPU 使用
        ↓
执行必要的 TLB 失效及同步
        ↓
清除目标根的借用上半场条目
        ↓
递归回收目标根私有子树（跳过借用上半场，不进入共享子树）
        ↓
清空目标根页
        ↓
返回 CallerManagedRoot<'root>
```

两条注意（评审明确点出）：

1. `free_child_tables()` 必须跳过借用上半场，**不能进入共享子树**。
2. **清除借用条目不等于释放源页表页**——源页表结构仍由其原始 owner 管理。

闭合后的语义：返回的凭证代表一个**仍然存在、但已清空**的外部根页。

#### 物理页是否归还要显式区分（评审 §6.1）

采纳"必须显式"这一要求，但对具体形式我给一个更轻的取舍。

评审建议两种形式：`enum RetireOutcome { ReturnedToAllocator, PermanentlyRetired }` 返回值，或"要求调用时具备回收能力"。

**我当时的判断是不引入新枚举，只在文档写差异表。第八版评审否掉了这个做法**，改为"暂不公开该 API"（§4quinquies.1f）。理由成立：既然没有生产调用者，最干净的处理是**先不承诺语义**，而不是承诺一个语义模糊的公开方法。

**但必须做的**：把差异写进 API 文档的合同表（评审 §6.1 那张表，我照录）：

| 动作 | 内核 bump 映像 | VM 可回收映像 |
|---|---|---|
| 从目标根清除私有子树入口 | 是 | 是 |
| 清除共享上半场入口 | 是 | 是 |
| 递归清理私有子树页 | 是 | 是 |
| **将物理页放回 allocator** | **否** | **是** |

**并明确写一句**：未来的 `retire_owned_subtrees` 在两种映像下**物理回收结果不同**，调用者必须按自己所在映像理解那个动词的含义。不要依赖 `PT_FREE` 是否注册来静默决定语义。

### 4quinquies.5 共享上半场也要约束后续映射（评审 §5）

采纳，这是本轮最有实际价值的新增约束。

目前的合同只说了"回收时跳过共享条目"。但还有一条链没堵：

```
① CallerManagedPageTable 从 bootstrap root 继承上半场
② 它的 Paging 实现把操作委托给 inner
③ 调用者通过 map()/unmap() 修改上半场的虚拟地址
④ 这个修改实际进入共享子树
```

**这会破坏源页表及其他继承者正在使用的结构。**

原来的注释 "user mappings never touch the upper half" 证明**设计意图**是这样，但**泛型 `Paging::map()` 接受任意 `VirBytes`，不会自动维护这个边界**。

采纳评审的建议（不为每种操作发明 handle，而在 handle 上定地址范围合同）：

- `map` / `unmap` / `remap` / `update_flags` 等普通操作**不得修改借用上半场**；
- 若确需修改共享内核映射，必须走明确授权的全局 owner 路径；
- **由架构后端识别对应的顶层范围，通用层只表达"不得修改借用的共享子树"这一语义。**

**测试要求**（评审给的，照录）：

> 对 `SupervisorSubtree::Borrowed` 的 handle，尝试修改继承范围内的地址，应被拒绝，且源根和其他继承者的 PTE 不发生变化。

这样共享状态才真正参与**整个操作合同**，而不是只在 `destroy()` 时发挥作用。

### 4quinquies.6 第六版评审的其余裁决（记录）

| 事项 | 裁决 | 落点 |
|---|---|---|
| 三层 API | 接受 | §4quater.0 |
| `PagingBackend` 内部约束 | **需要**：pub(crate)/sealed + 凭证与后端根页一致 | §4quinquies.3（1）|
| `ProcessLoad` 路二 | **有条件接受**：需证明物理页持续保留 | §4quinquies.1 |
| `BorrowedRoot` 生命周期 | 覆盖**整棵可达页表树**，不只是根页 | §4quinquies.3（2）|
| 两个生命周期 | **推荐** `'root` / `'source` 分开 | §4quinquies.3（3）|
| `Borrowed` 是否只读 | **不是**；需确认实际调用点 | §4quinquies.2（四个全写）|
| 共享范围的后续映射 | **必须禁止**普通操作修改借用子树 | §4quinquies.5 |
| `reclaim` 返回前清共享条目 | **建议清除**，再返回凭证 | §4quinquies.4 |
| 无 free 钩子的终结语义 | 明确 `retire` 与真回收的差异 | §4quinquies.4 |
| 失败回滚 / 逐页映射 / 池化 | 不扩展本轮 | 不动 |

---

## 5. 当前开放事项（已按第九版评审收敛）

第八版的 Q1–Q6 已全部由第九版评审回答。**这里只留仍未闭合的实质风险与待核实项**，不再放已裁决的设计问题。

### 已裁决（记录结论，不再是开放问题）

| 原问题 | 第九版裁决 | 落点 |
|---|---|---|
| Q1 `RetireDisposition` 三态？ | **二值足够**，不加 `PartiallyReturned`。"部分归还"不是当前模型的正常结果；若未来一棵子树混合多个分配来源，那需要重新设计来源表达，不是加个枚举值 | §4quinquies.1f |
| Q1 附带：`destroy` 也有处置差异 | **成立**，见下"已落实" | §4quinquies.1j |
| Q2 `classify_range` 关联函数还是方法？ | **关联函数**。它分类的是架构地址布局中的位置，不读当前根页顶层 PTE，不需要 Direct Map | §4quinquies.1b |
| Q2 `CrossesBoundary` 要不要？ | **保留**。不能把"调用者通常不跨界"当 API 安全条件 | §4quinquies.1b |
| Q5 非 `'static` 源的返回类型？ | 需要**同时携带 `'root` 与 `'source`** 的不可拆散凭证；当前 `'static` 限制合理，不要提前泛型化 `ProcessLoad` | 见下"待实现" |
| Q6 D/E 能否合并？ | **分开开发、合并验收**。锁解决并发协调，TLB 失效解决旧翻译——两者不能互相替代 | 见下"D/E 验收门" |

### 仍未闭合的两项实质风险

| # | 风险 | 关闭条件 |
|---|---|---|
| **R1（= Q3）** | 源根后续修改是否会让继承者持有**陈旧或悬空入口** | `harden_kernel_text` 与 heap-window eviction 的四个问题必须核实（§4quinquies.1c）。**这是共享 handle 接入生产路径前的验证门** |
| **R2（= Q4）** | TX 启动分配是否会与**未来 VM PMM 消费的 free list** 重叠 | 专用保留区方案落地 + **容量实测**（§4quinquies.1m 已算出现有 512 页 bump 区要同时装 PT allocator 与 TX 镜像，容量数据缺失）|

**评审对此的定性**：这两项都不要求推迟整个页表 handle 重构，应作为**独立实施门**——R1 决定共享页表方案能否安全用于当前 bootstrap 路径；R2 决定 `ProcessLoad` 永久保留方案能否安全用于真实 VM 拓扑。

### D/E 验收门（采纳第九版评审 Q6）

**D 与 E 分开开发，作为两个独立工作项；但只在两者都完成后验收 borrowed handle 的生产迁移。**

| 阶段 | 验证对象 | 核心问题 | 证据类型 |
|---|---|---|---|
| **D** | handle 对地址范围的操作授权 | 能否阻止修改借用上半场？ | 架构分类 + wrapper 检查 + 测试 |
| **E** | 对活动根与共享页表的并发、TLB 使用合同 | 被授权的修改能否**安全执行**？ | 四个调用点各自的同步依据 |

**E 的验收记录必须对四个调用点分别回答三问**（评审原文）：

1. 哪个锁、单线程约束或启动顺序防止未协调的并发修改？
2. 修改时可能有哪些 CPU 正在使用该根，或使用共享子树的其他根？
3. 修改后由谁执行 TLB 失效，失效覆盖哪些 CPU、地址空间和翻译缓存范围？

**评审的警告我照录**：

> 锁解决的是并发修改协调，TLB 失效解决的是旧翻译仍可能被使用；两者不能相互替代。BKL 存在，也不能单独证明 TLB 合同成立。

**D 完成而 E 未过时的中间状态**：代码已具备范围检查，却仍允许对一个缺乏全局同步证明的活动根进行修改。**这种状态不能标记为"生产安全改造完成"。**

### 待实现（未来预留，不在当前 API 面）

| 项 | 形态 | 何时做 |
|---|---|---|
| `retire_owned_subtrees(self) -> RetireResult<'root>` | `RetireDisposition::{ReturnedToAllocator, PermanentlyRetired}` | 出现真实 `CallerManagedPageTable` 私有子树的回收需求时 |
| 非 `'static` 源的终结凭证 | `RetainedPageTableRoot<'root, 'source>`：根凭证与共享源凭证**作为不可拆散的安全结果**返回 | 出现非 `'static` 源时 |
| TX 专用保留区 | `Boot reserved region` 拆成 `PT allocator range` + `TX image range` | R2 关闭时，**真实 VM 接入前** |

**一个必须记住的边界**（评审指出）：`RetireDisposition` 只能**报告已约定的释放策略，不能证明回调实现正确**。`PtFreeFn` 返回 `()`，调用者无法从返回值验证 allocator 是否真的接受了每一页。`ReturnedToAllocator` 成立的前提仍是注册时保证 `PT_FREE` 与该映像的 `PT_ALLOC` 来源配对。

### 已落实（第九版新增的四处合同）

| 项 | 内容 | 位置 |
|---|---|---|
| 扩展 trait 代理 | `HugePages` 等也必须经 wrapper 检查；`BootArch::Paging: HugePages` 而 `lib.rs:1340` 直接调 `map_huge` —— 只实现 `Paging` 会编译失败 | §4quinquies.1h |
| 根层跳过的归属 | 由 `PagingBackend::destroy_private_subtrees(&mut self, supervisor)` 决定；通用层不硬编码 256..512 | §4quinquies.1i |
| `destroy` 的处置差异 | 保留 `destroy(self)` 不加返回值，但文档写清"无释放能力时页表页永久消耗"；`Drop` 不回收，用 `#[must_use]` 防误丢 | §4quinquies.1j |
| `load_process_elf` 真交接 | 实施时必须真的改写成 `into_permanent_root()` + `SharedPageTableSource<'static>` 入参，不能只补文档 | §4quinquies.1k |

### 三架构测试验收清单（第九版评审给，六项）

不能只检查"总共释放了几页"。至少覆盖：

| # | 行为 |
|---|---|
| 1 | **私有上半场**：`OwnedPageTable` 建立的上半场私有子表，销毁时应被释放 |
| 2 | **共享上半场**：继承后的目标根销毁私有子树时，**共享子表物理页不得进入释放回调** |
| 3 | **根页归属**：`CallerManaged` 根页不能进入 `pt_free`；`Borrowed` 根页不能被清零或释放 |
| 4 | **范围拒绝**：继承后尝试 `map`/`unmap`/`update_flags`/`split_huge` 修改借用范围，**必须在写入任何 PTE 前失败** |
| 5 | **批量操作原子性**：跨边界的 `map_range`/`unmap_range`/`remap` 被拒绝后，**私有区域也不能留下部分修改** |
| 6 | **源共享行为**：共享子表内部允许的修改会被继承者看到；同时验证源根替换入口或解除映射时**不会释放仍被继承者引用的页** |

**第 6 项尤其重要**（评审原文）：前五项证明目标 handle 不会越权修改共享区域，**但还不能证明源页表不会把目标根弄成悬空或陈旧状态**。

**第 6 项的测试还要再补一格**（第十版评审）：当源根顶层入口被替换时，目标根可能继续指向旧子树——这**可能是暂时安全但语义陈旧**的状态。所以测试不能只断言"旧子树没有释放"，还要按最终选定的合同明确：**这种陈旧映射是允许的，还是必须在替换时同步更新/拒绝替换。**

### R2 落地时必须记录的指标（第十版评审）

专用保留区是首选方案，但当前只有最低使用量，**没有 PT allocator 的实际高水位**，所以不能判断现有 512 页（2 MiB）是否够。实施时须同时采集：

| 指标 | 需要得到的数 |
|---|---|
| PT allocator | `arch_boot_impl` 全程实际分配页数**及高水位** |
| TX allocator | 根页、ELF、栈的**实际总页数** |
| 保留区 | PT 与 TX 两个子区间的**大小、边界及对齐** |
| VM handoff | 外层完整保留区间是否**仍被整体排除** |

**一个不能搞混的点**（评审原文）：外层 reservation 的范围与 `BootAlloc` 自己可分配的子区间是**两个概念**——不能因为拆分 allocator 区间而缩小 handoff 的保留范围。

---

## 6. 附：关键源码

### 6.1 `pt_alloc.rs` 关键片段

```rust
fn uninit_free(_phys: PhysBytes) {}

pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    assert!(is_registered(), "pt_alloc::alloc_pt_page called before register");
    let _ = PT_REGISTERED.load(Ordering::Acquire);
    let alloc_fn = unsafe { core::ptr::read(PT_ALLOC.0.get()) };
    alloc_fn()
}

pub fn free_pt_page(phys: PhysBytes) {
    let _ = PT_FREE_REGISTERED.load(Ordering::Acquire);
    let free_fn = unsafe { core::ptr::read(PT_FREE.0.get()) };
    free_fn(phys)
}
```

### 6.2 `Paging` trait 的四个构造函数（`os/arch/src/arch/paging.rs`；**迁移前现状**）

四个构造函数（`os/arch/src/arch/paging.rs` 的行号）：

| 构造函数 | 行 | trait 文档的关键句 |
|---|---|---|
| `new()` | `:213` | 根页由实现内部通过 `pt_alloc` 分配 |
| `new_from_page(root_page)` | `:235` | "Used during boot when no global allocator exists"；"The implementation zero-fills it" |
| `from_active_root(root_phys)` | `:260` | "must not outlive the page table it wraps (i.e., don't call `destroy()` on it unless you own the table)" |
| `adopt_active_root(root_phys)` | `:287` | 同 `from_active_root`，但 PTE 通道走 VM Direct Map |

```rust
fn new() -> Result<Self, PageTableError> where Self: Sized;
fn new_from_page(root_page: PhysBytes) -> Self;
fn from_active_root(root_phys: PhysBytes) -> Self;
fn adopt_active_root(root_phys: PhysBytes) -> Self;

fn inherit_supervisor_half(&mut self, source_root: PhysBytes);
unsafe fn destroy(&mut self);
```

**注意 trait 文档自己已经承认了本次评审的核心问题**：`from_active_root` 那句 "don't call `destroy()` on it unless you own the table" 是**用注释表达所有权**。注释无法被编译器检查，也无法被 `destroy()` 内部区分——这正是 §3.3 与 §4ter.2 要解决的。

### 6.3 `Paging::new()` 的生产调用（VM 侧；**迁移前现状**）

`os/servers/vm/src/vmproc/vmproc_handle.rs:364`：

```rust
let mut pt = <PageTable as Paging>::new()?;
map_kernel(&mut pt, &layout)?;      // ← 逐页映射内核空间，自分配中间表
```

`PageTable` = `minix_arch::CurrentPaging`（`os/servers/vm/src/pagetable/mod.rs:32`）。

### 6.4 四套引导期根页来源（原稿写"两套"）

| 来源 | 谁在跑 | 分配入口 | 供什么 | 是否被回收 |
|---|---|---|---|---|
| UEFI 分配服务 | boot-shim（x86-64 / aarch64 UEFI 腿）| `uefi_helpers.rs:602` `alloc_root_page()` → `boot::allocate_pages(...)` | **bootstrap 根页** | 否 |
| boot-shim bump 游标 | kernel-image 自举（riscv64 直连核腿）| `opensbi_helpers.rs:630` `bump_alloc_pages(1)` | **bootstrap 根页** | 否 |
| `bootface_a64` 私有 bump | kernel-image 自举（aarch64 直连核腿）| `bootface_a64.rs:115` 本文件私有 `BUMP_PTR`（`:111`）| **bootstrap 根页** | 否 |
| 内核 `BootAlloc` | 内核 `arch_boot_impl` 之后 | `boot_alloc::boot_pt_alloc`（经 `pt_alloc::register`，`lib.rs:974`）| **中间表页** | 否（无 free 钩子）|
| VM `VmBootAllocator` | QEMU 测试内核 `load_process_elf` | `frame.rs:639` `alloc_page()` | **进程根页 + 镜像页** | 否（无 free 方法）|

**这张表是本版最重要的更正**：根页与中间表页走**不同的分配器**，而且四种引导形态各一套根页来源（其中两个甚至用了完全不相干的池）。原稿的"两套"把根页与中间表页混成一列了。

---

## 7. 版本变更对照

### 7.1 第一版 → 第二版

| 项 | 第一版 | 第二版 |
|---|---|---|
| 问题 B（跨域归还）| 判 P1 | **撤回**（跨映像静态状态不共享，`register_free` 内核侧零命中）|
| 问题 C（共享子树）| 判 P1 | **保留**，当前无触发 → 后果 P1 / 优先级 P2 |
| 问题 D（根页所有权）| 未提出 | **新增**（外部评审补充，已核实）|
| `free_pt_page` 的 Acquire | 判"无效" | 第二版评审**自行撤回**；我也接受（§3.4）|
| 逐页 vs 大页 | 提为"偏离" | 改判为**设计差异，非缺陷**（W^X 可能是理由）|
| 修法 | `enum RootKind` 三值 | 改为**两个独立维度**（根页 / 子树）|

### 7.2 第二版 → 第三版

| 项 | 第二版 | 第三版 |
|---|---|---|
| §4bis 三组证据 | 无 | **新增**：构造函数调用点表、`load_process_elf` 生命周期、`register_free` 完整实现 |
| 根页来源 | 只知"两处 `new_from_page`，来源不同" | **纠正并加强**：四种来源、四份互不共享的状态，**全都不是 `pt_alloc` 域** |
| 所有权模型 | 两维度 | **三维度**（新增"分配来源与释放策略"）|
| 修法 | 只有方向 | **6 步实施顺序**（§4ter.2）|
| "跳过上半场"的诱惑 | 只说"C 不能照搬" | **给出反例表**（§4ter.3），两套策略并存 |
| `VmBootAllocator` | 未提 | **新增**：纯 bump，无 free（§4bis.2）|
| 失败路径 | 未提 | **新增**（§4ter.4）|
| `is_free_registered` 的 `Relaxed` | 未提 | **新增**（§4bis.3）|
| 内核映像的 destroy 语义 | 未提 | **新增**：无 free 钩子 → 恒走"只零根"分支 |

### 7.3 第三版 → 第四版

| 项 | 第三版 | 第四版 |
|---|---|---|
| "四种来源全部跨域归还" | 写成当前事实 | **降级为合同缺陷**：需两道门（映像注册了 free 钩子 + handle 被 `destroy()`），当前无一成立（§3.3.1）|
| 风险链 | "只差一次 `register_free`" | **两个条件**：注册钩子 + 销毁外部根 handle（§3.3.2）|
| `destroy()` 的无钩子分支 | 描述为"降级为只零根" | **重述为危险能力**：清零活动根不需要 free 钩子（§3.3.3）|
| `free_pt_page` 调用点 | 未完整列出 | **新增 §4bis.4**：全仓只有两个入口，都在门后；接受"读到旧值不等于无数据竞争"的纠正 |
| `pt_alloc` 实现 | 未提 | **新增 §4bis.5**：`BootAlloc` 已有 `resume`（checkpoint 能力已具备但非事务）；`VmBootAllocator` 反而无 rewind |
| 维度 C 命名 | "分配域" | **改为"分配来源与释放策略"**，并区分"默认分配器绑定"与"物理页 provenance" |
| `from_active_root` 的 B | `AllOwn`（但无人能证明）| **`BorrowedUnknown`**（消除表格自相矛盾）|
| `new_from_page` 的 A | "未知" | **`CallerManaged`**；并把"可写"与"可释放"分开 |
| 第 1 步成本 | "零成本删除" | **API 拆分**（`Paging` / `OwnedPaging`），不是删方法；理由改为"同时关掉两条错误链" |
| 失败路径 | 只讨论 `VmBootAllocator` | **两套分配器**：`pt_alloc` 的中间表页同样不回滚；采纳"不加通用事务系统" |
| 测试要求 | 未提 | **新增**：验物理页归属，不只数释放页数 |

**第四版仍是给同一批评审者的**：Q1、Q2 已获裁决（成立，保留），本轮只需要复核 §7.3 里那几处改写是否准确，以及 §5 的三个新问题。

### 7.4 第四版 → 第五版

| 项 | 第四版 | 第五版 |
|---|---|---|
| §4bis.1b 的结论 | 仍写"四种来源全部是跨域调用"（与 §3.3.1 冲突）| **已同步**为合同缺陷 + 两道门（§4bis.1b 末）|
| §4bis.1b 的"四份不共享状态" | 措辞笼统 | **改为区分分配入口与分配器实例**：riscv64 是"同一实例、不同区间"（§4bis.1b）|
| `destroy()` 的降级分支 | 描述为"降级" | 改为**独立的破坏动作**，不是安全降级（§4bis.1b 末、§3.3.3）|
| **修法第 1 步（核心）** | `trait Paging` / `trait OwnedPaging` 拆分 | **承认该方案无效**：trait 实现关联于类型，`X86_64Paging` 只有两个字段、四个构造函数都返回 `Self`；改为**方向甲（加所有权字段，运行期检查）/ 方向乙（外层 handle 类型，编译期隔离）**两案对比 + 泛型调用点约束（§4ter.2 第 1 步）|
| `CallerManaged` | 只说"可写但不可释放" | **三个独立责任**（内存有效 / 独占修改 / 最终释放）；`CallerManagedRoot<'a>` 凭证 或 狭窄 `unsafe` 边界；明确**不建全局页归属表**（§4ter.2）|
| 维度 C 的"每页 provenance" | 写成实现形式要求 | 改为**合同要求**"每个可释放资源能确定来源与责任"，不需要 per-page 标签（§4ter.1）|
| 共享状态表示 | 我提问"布尔够不够" | **评审给出结论**：二值语义枚举够用；两个实现条件（禁止重复继承、根层直接跳过）已记录（§4ter.3.1）|
| 失败路径的"加断言" | `debug_assert` | **断言禁止不了重试**，改为调用点合同（错误即终止启动）+ 不依赖断言（§4ter.4）|
| BKL 作为独占证据 | 未分析 | **新增**：`BootAlloc` 用 `AtomicU64`，不要求调用者持锁；BKL 是环境事实不是类型保证；不开锁审查（§4ter.4）|
| Q4 池化 vs 共享 | 混在一起 | **分开**：池化解决成本，共享解决重复持有；共享可能是更直接的方向；当前只统计数量（§5 Q4）|
| Q5 三问 | 仍是开放问题 | **已由第四版评审全部回答**，改为结论记录 + 两个新问题（§5 Q5）|

**第五版只保留第四版评审要求的三个修订**，其余均已收敛：

1. §4bis.1b 旧措辞同步 —— 已完成
2. `OwnedPaging` 改为真正不同的 handle 类型或所有权类型参数 —— 已完成（两案对比 + 泛型约束）
3. `new_from_page()` 的独占修改授权 + 终结操作只回收私有子树 —— 已完成

**下一步的性质变了**：从"概念合同"转向**具体 API 设计**。评审的收尾要求我照录：

> 重点不是再发明一个更复杂的 `RootKind`，而是让三种不同资源责任在 Rust 类型上确实成为三种不同的对象。

### 7.5 第五版 → 第六版

| 项 | 第五版 | 第六版 |
|---|---|---|
| **`ProcessLoad` 的所有权交接** | 未提 | **新增 §4quater.1**：唯一调用点把根页写进 `rs.p_seg.phys_root`，成为活进程地址空间（82 处读取）；无任何销毁方；两条路 + 倾向路二（保持裸地址 + 明确文档）|
| **`BorrowedRoot` 的生命周期来源** | 只是给 `PhysBytes` 套 `PhantomData`（假的）| **新增 §4quater.2**：承认假生命周期不成立；核实四个调用点全无 Rust 所有者对象；改为 `unsafe fn from_raw` 狭窄边界；`'a` 现填 `'static` |
| **API 三层层分** | 只有 handle 与 `Paging` | **新增 §4quater.0**：`Paging`（操作）/ `PagingBackend`（后端，pub(crate)/sealed）/ 三个 handle |
| handle 是否实现 `Paging` | 隐含 | **明确**：三个 handle 都 delegate 实现 `Paging`，`map_kernel`/`load_elf_into` 的泛型约束不用改 |
| `CurrentPaging` | 想换成 handle | **保留为后端类型**，另加三个别名 |
| `adopt_active_root` 命名 | 沿用 | **改名**：`adopt` 暗示所有权转移，改为显式 `PteChannel` 参数 |
| 方法归属 | 已在第 1 步讨论 | **成表**（§4quater.0）：`inherit_supervisor_half` 从通用 `Paging` 摘到 `CallerManagedPageTable` |
| `reclaim_owned_subtrees` 无 free 钩子 | 未提 | **新增**：区分"清 PTE"与"真归还"；内核映像下是 abandon/retire，必须写进 `unsafe` 合同 |
| `SupervisorSubtree` 的 `Borrowed` | 只有标记 | **带凭证**——现为 `Borrowed(SharedPageTableSource<'source>)`（第九版拆凭证后）|
| `inherit_supervisor_half` 前置条件 | 两条（禁止重复、根层跳过）| **三条**：新增"目标上半场必须为空"及其四步后果表 |
| `ProcessLoad` 的两条路 | 无 | **路一泛型化 / 路二裸地址 + 文档**；倾向路二，理由是可核实的（唯一调用者、无 free 方法、生产路径不走这条）|

**第六版的状态**：评审说"第五版可以冻结概念设计，进入 API 方案落地"。本版补齐了它要求的两项生命周期合同，**概念设计可以冻结**。

**第七版补上了第六版留下的两处**：

1. **`ProcessLoad` 选路一还是路二** —— 第七版查清了保留机制，并发现一个**可命名的隐患**：`test-sysboot` 之所以没出事，是因为 VM 槽跑的是桩镜像 `sysboot-rx`；若换成真 VM 服务，`tx` 消耗的帧从未被 `cut` 出 free list，会直接重叠覆盖（§4quinquies.1）。路二可以冻结，但必须带上这条边界条件。
2. **两个生命周期参数** —— 采纳评审的推荐，但理由按它的纠正写：不是因为两个资源来自不同 owner，而是因为**终结时根页凭证不该被缩短到 `'source`**（§4quinquies.3（3））。

**第八版仍请复核的两处**（都是第七版新提出、评审尚未表态的）：

1. **`reclaim_owned_subtrees` 的返回值形式** —— 评审建议 `enum RetireOutcome` 或"要求具备回收能力"。我倾向第三种：**只在文档里写差异表，不引入新枚举**，理由是该函数当前无生产调用者。这个取舍是否可接受？
2. **"不得修改借用上半场"的检查位置** —— 评审说由架构后端识别顶层范围、通用层表达语义。但架构后端目前只暴露 `root_paddr` 与逐层操作，**没有"这一段是否属于共享条目"的查询能力**。是不是需要给后端加一个范围判定接口？这不是所有权问题，但它是这条约束能否落地的前提。

### 7.6 第六版 → 第七版

| 项 | 第六版 | 第七版 |
|---|---|---|
| **物理页保留证据** | 只说"bump allocator 不提供 free" | **新增 §4quinquies.1**：完整追查 `init_proc_and_boot:203` 发布 → `:242` 读取 → `:254` 消耗的链路；发现 `used_frames` 只在发布瞬间被读一次；发现消费方不存在（`sysboot-rx` 是桩）|
| **可命名的隐患** | 未提 | **新增**：若 VM 槽换真服务或再调一次 `load_process_elf`，`tx` 消耗的帧从未被 `cut` 出 free list → 直接重叠（`test-sysboot:238-241` 自己的注释描述的正是这个机制）|
| **四个活动根调用点的操作** | 未区分 | **新增 §4quinquies.2**：全部会写（`map`/`split_huge`/`update_flags`/`grant_user_walk`/`unmap`/`unmap_range`），**没有一个只查询** |
| `Borrowed` 是否只读 | 未定 | **确定：不是只读**；`BorrowedRoot` 凭证语义 = 独占修改权 + 不拥有释放责任 |
| `PagingBackend` 封装 | 只说 pub(crate)/sealed | **补两条**：`CallerManagedRoot.phys` 必须由唯一私有构造与后端 `root_paddr` 一致，之后不可独立替换 |
| `BorrowedRoot` SAFETY | 只写"根页有效" | **改为"根页及所有可达页表页"**；并补共享子树的同步与一致处理要求 |
| 两个生命周期 | 我提问"要不要" | **采纳 `'root`/`'source` 分开**，理由改为"终结时凭证不该被缩短"；类型状态 `PrivateSupervisor` / `BorrowedSupervisor<'source>` 一并采纳 |
| `ProcessLoad` 出口 | 隐式丢弃 | **新增显式动词 `into_permanent_root(self) -> PhysBytes`**；四个终结动词的语义区分表 |
| `reclaim_owned_subtrees` 清共享条目 | 未提 | **新增**：返回前必须清除借用上半场条目（六步顺序），否则共享依赖没有真正结束 |
| 物理回收差异 | 只说"取决于 free 钩子" | **加合同表**：四行动作 × 内核/VM 两种映像；明确"不要依赖 `PT_FREE` 是否注册来静默决定语义" |
| **共享上半场禁改** | 未提 | **新增 §4quinquies.5**：普通 `map`/`unmap`/`remap`/`update_flags` 不得修改借用上半场；并加测试要求 |

**第七版的状态**：评审说"概念设计基本可以冻结"。本版补齐了它要求的物理保留证据与四项合同细节，**概念设计可以冻结，进入 API 落地**。

**但第七版自己查出一个应当独立立项的隐患**：`test-sysboot` 的保留成立性依赖"VM 槽是桩"。若该槽换真实 VM 服务，`load_process_elf` 消耗的帧从未 `cut` 出 `vm_pmm_free_regions()` 那份列表，会与 VM PMM 重叠、零页覆盖 RX/TX 镜像。**这是与所有权无关的独立缺陷**，见 §4quinquies.1 末的边界条件。

### 7.7 第七版 → 第八版

| 项 | 第七版 | 第八版 |
|---|---|---|
| **同一 allocator 重复加载** | 判"也可能重叠" | **更正为不成立**：同一实例的递减游标避免重复；风险只在"多个 allocator 消费同一陈旧快照"（§4quinquies.1）|
| **classify 与 TX 的 allocator 身份** | 混为一个实例，时间线不成立 | **更正为两个独立对象**：VM 的 `vm_alloc` 在 `lib.rs:2922`（`classify` 读它）、TX 的在 `test-sysboot:248`（不进入快照）（§4quinquies.1）|
| **`pt_alloc` 中间表页是否在风险范围** | 列为"需要保留的页" | **更正为不在名单里**：`for_each_live_bootstrap` 把 `boot_alloc_region()` 的**整块**区间 deduct（`vm_handoff.rs:693-695`）；且该 bump 区来自 UEFI `allocate_pages`，不在 memmap |
| **风险范围** | 含糊（含三套分配器）| **缩小到一处**：只有 TX 从 `VmBootAllocator` 拿的根页/镜像页/栈页 |
| **`classify_range` 接口** | 我提问"要不要加" | **确认需要**，并给形态 `PagingRangeClass::{Private,Borrowed,CrossesBoundary}`（§4quinquies.1b）|
| **修改入口覆盖** | 只说 `map`/`unmap`/`remap`/`update_flags` | **扩展成七行清单**，含 `grant_user_walk`、`split_huge`、批量操作"改前先查全范围"（§4quinquies.1b）|
| **handle 委托语义** | "delegate 给 inner" | **修正为"handle 是策略边界、backend 是机制边界"**，不是无条件转发（§4quinquies.1b）|
| **源页表修改合同** | 未提 | **新增 §4quinquies.1c**：源树在继承者存活期间不得替换/释放共享子树；成立性**尚未核实**（列 §5 Q3）|
| **凭证语义** | `BorrowedRoot` 一个类型兼两用 | **拆成** `BorrowedRootMut<'a>`（改）与 `SharedPageTableSource<'source>`（保活）（§4quinquies.1d）|
| **`into_permanent_root`** | `-> PhysBytes` 无条件 | **限制为 `BorrowedSupervisor<'static>`**，否则丢掉 `'source` 而类型不再表达物理依赖（§4quinquies.1e）|
| **`reclaim_owned_subtrees`** | 我倾向文档差异表 | **降级为暂不公开/暂不实现**；将来用 `retire` + `RetireDisposition`（§4quinquies.1f）|
| **`BorrowedRootMut::from_raw`** | 只写生命周期与页有效 | **加四条 SAFETY**：修改权、同一同步机制、架构失效合同、无未协调并发（§4quinquies.1g）|
| **§5 问答区** | 仍留 Q1–Q5（多数已裁决）| **整段替换**为六个真正开放的事项（Q1–Q6）|

### 7.8 冻结状态与实施分期（采纳第八版评审 §8）

**冻结范围（第十版评审的最终措辞，本项目以此为准）**：

> **冻结所有权模型、共享子树语义、三层 API 架构和安全合同；具体 Rust 方法签名、泛型边界与调用点迁移，允许在实现中根据编译器和测试结果调整。**

**这是设计冻结，不是要求实现者逐字照抄文档里的每一个伪代码签名。**

项目内的展开表述：

> 页表 handle 的资源责任、共享子树语义、三层 API 边界及生命周期方向已经冻结。具体实现必须遵守共享范围禁改、共享源稳定、根页与共享源生命周期分离等合同。`ProcessLoad` 的永久保留方案**仅适用于当前 `test-sysboot` 拓扑**；真实 VM 消费 free-list 前必须修复 post-handoff 分配未扣除的问题。

**三项不再退回争论的决定**（第十版评审点名）：

1. **不再考虑用一个 `RootKind` 枚举代替三种 handle**——三种资源责任已有足够明确的类型边界。
2. **不再考虑把所有上半场一律跳过**——私有与继承上半场并存，必须由状态表达、由架构后端识别根层入口。
3. **不再把 borrowed 当成只读**——四个现有活动根调用点全部会修改页表；真正要表达的是"不拥有释放责任 + 受同步合同约束的修改授权"。

| 阶段 | 工作 | 裁决 | 说明 |
|---|---|---|---|
| **A：API 分层** | `Paging` / 内部 `PagingBackend` / 三种 handle | **可开始** | 建立内部 backend 与三类 handle；**补齐 `HugePages` 等扩展能力的代理关系**（§4quinquies.1h）|
| **B：所有权隔离** | 移除通用构造与 `destroy` 能力 | **可开始** | 构造函数按资源责任归属 |
| **C：共享子树** | 类型状态、目标上半场为空检查、根层跳过 | **可开始** | **同时核实共享源约束（R1）**；API 与测试可先做，但 `SharedPageTableSource` 用于真实 bootstrap 路径前必须过 R1 |
| **D：访问权限** | 全部公开修改入口统一范围检查 | **可开始** | 批量操作必须**先整体检查**再改 |
| **E：借用同步** | 四个活动根调用点的同步、CPU 参与集合与 TLB 合同 | **生产迁移门** | **必须有实际证据**；D+E 合并验收 |
| **F：永久保留** | `into_permanent_root` 的 `'static` 限制、目标根永久保留责任 | **有条件实施** | 与 TX 保留区设计一致；不能只凭"`VmBootAllocator` 没有 free()"宣称成立 |
| **G：VM handoff 重叠** | 修复陈旧 free-list 快照风险 | **真实 VM 接入前阻塞** | 专用 TX 保留区是首选候选；**需容量实测** |
| **F'：bootstrap 根交接** | `PrivateSupervisor` 的 `into_permanent_root` + `enable` 归位 | **可开始（前置）** | 两个 `new_from_page` 调用点都要有生命周期出口（§4quinquies.1n）|
| **H：物理回收结果** | 暂缓 `retire_owned_subtrees` 的公开 API | **暂缓** | 未来有真实调用点再定义二值结果 |

**第十版评审建议的执行顺序**（减少返工，我照录）：

| 顺序 | 工作 | 验收标准 |
|---|---|---|
| A | 三层 API 与 handle 骨架 | 三类 handle 分离；backend 对外封装正确 |
| B | 构造、根页凭证、终结能力 | borrowed 不可 destroy；caller-managed 根不进入 `pt_free` |
| C | 共享状态与 backend 根层遍历 | 三架构实现根层跳过；**内部测试**验证物理页归属 |
| D | 修改范围检查 | `Paging`、`HugePages` 及其他公开修改能力全部受检查 |
| E | 活动根与共享树同步 | 四个调用点逐一给出同步、CPU 参与集合及 TLB 证据 |
| F | bootstrap／TX 显式生命周期交接 | `PrivateSupervisor` 与 `BorrowedSupervisor<'static>` **两条路径都闭合** |
| G | TX 专用保留区 | 容量实测，区间不重叠，VM PMM 无法分配 TX 页 |
| H | 未来 retire API | 有真实需求再实现 |

**两项跨阶段的依赖**（评审明确点出）：

- **C 的 API 与测试可以先做，但 `SharedPageTableSource` 用于真实 bootstrap 路径前必须通过 R1（源结构稳定性验证）。**
- **D 的范围检查可以先做，但不能在 E 尚未通过时把 borrowed handle 的生产迁移标记为完成。**

**实施原则**（评审原文，作为本轮的收束）：

> 不要等到所有非核心问题都解决才开始代码工作，但也不要在共享范围检查和页表同步合同尚未落实时，把类型拆分本身当成修复完成。

**建议的起手顺序**（评审给的，我采纳）：

1. 先完成 `PagingBackend` 的范围分类接口与三种 handle 的骨架；
2. 再迁移四个活动根调用点；
3. 同时增加三架构的共享子树回收测试；
4. **VM handoff 的重叠问题作为独立修复项推进**——不拖住页表 API 改造，也不被这次所有权重构掩盖。

### 7.9 第八版 → 第九版

| 项 | 第八版 | 第九版 |
|---|---|---|
| **扩展 trait 代理** | 断言"handle 实现 `Paging` 则泛型调用不用改" | **该断言不成立**，有直接反例：`BootArch::Paging: HugePages`（`boot_arch.rs:22`）而 `lib.rs:1340` 直接调 `paging.map_huge(...)`。只实现 `Paging` 会**编译失败**。`HugePages` 等必须一并代理且同样过范围检查（§4quinquies.1h）|
| **根层跳过的归属** | 只说后端要跳过共享条目 | **补齐接口** `PagingBackend::destroy_private_subtrees(&mut self, SupervisorOwnership)`；并说明 `classify_range` 不能替代它（一个判虚拟范围、一个判物理根层入口）（§4quinquies.1i）|
| **`OwnedPageTable::destroy` 的处置语义** | 未提 | **新增 §4quinquies.1j**：保留无返回值的 `destroy`，但文档必须写"无释放能力时页表页永久消耗"；`Drop` 不回收 + `#[must_use]` 防误丢 |
| **`load_process_elf` 的实际交接** | 只提 `into_permanent_root` 这个动词 | **新增 §4quinquies.1k**：必须真的把返回路径改成 `into_permanent_root()`，且共享源改由调用者建 `SharedPageTableSource<'static>` 传入——不能只补文档 |
| **`into_permanent_root` SAFETY** | 只写源必须永久 | **拆两条**：①源永久有效（`'static` bound 表达）②**目标根及私有子树由调用者永久保留**（调用点责任，类型不保证）（§4quinquies.1l）|
| **TX 保留区方案** | 只列为三选一 | **首选候选**，形态细化为 `Boot reserved region` 拆 `PT allocator range` + `TX image range`；并点明不能让它变成"换个名字仍从 `vm_pmm_free_regions()` 构造"；**容量数据缺失，不能宣称可落地**（§4quinquies.1m）|
| **源页表修改合同** | 一句"源树必须稳定" | **拆成三种变化**（改叶 PTE 可有意为之 / 替换顶层入口致陈旧 / 释放复用必须禁止）；补 TLB 与锁不能互替；R1 四条依据里**第 3、4 条未闭合**（§4quinquies.1c）|
| **`RetireDisposition` 三态** | 我留作开放问题 | **二值足够**，不加 `PartiallyReturned`（§4quinquies.1f）|
| **`classify_range` 该是方法？** | 我提问 | **关联函数**：它分类架构地址布局，不读页表状态（§4quinquies.1b）|
| **D/E 验收关系** | 我提问能否合并 | **分开开发、合并验收**；E 必须对四个调用点各答三问；"锁≠TLB 合同"（§5 D/E 验收门）|
| **文档内部一致性** | 多处旧 API 形态并存 | **已同步**：`reclaim_owned_subtrees` 全标为未来预留；`SupervisorSubtree::Borrowed` 改 `SharedPageTableSource<'source>`；handle 定义改 `'root` + 状态类型；SAFETY 拆成两个凭证；§6.2/§6.3 标注"迁移前现状"（§4ter.3.1、§4quater.0、§4quinquies.3、§6）|
| **测试验收** | 只说"验物理页归属" | **扩展成六项清单**，新增第 5 项批量操作原子性、第 6 项源共享行为（§5 测试清单）|

**第九版的状态**：评审给了"设计冻结：通过"与"实施启动：通过，但分阶段验收"。

> A–D 可以开始。D 与 E 分别开发、合并验收。C 的共享源合同必须在真实 bootstrap 调用点启用前得到证据支持。

**评审的收尾判断我照录，作为本文档的终点**：

> 第八版现在最值得做的下一步不是第九轮概念评审，而是按 A–D 开始 API 骨架和三架构测试，同时把 Q3、Q4 的证据核实列成明确的实施任务。这样既不会把尚未证明的安全条件当成已完成，也不会让已经成熟的设计继续空转。

**因此本文档到此不再是"等评审"状态**：概念设计已冻结，两个未闭合风险（R1 源结构稳定性、R2 TX/VM free-list 重叠）转为独立的实施门任务。

### 7.10 第九版 → 第十版（终版）

第十轮评审的裁决是**"第九版就作为概念设计冻结版，不要再做第十轮方案讨论，转入代码实施"**。本版只做它要求的三项接口补齐与六处文档同步。

| 项 | 第九版 | 第十版 |
|---|---|---|
| **`PrivateSupervisor` 的终结路径** | 只为 `BorrowedSupervisor<'static>` 定义了 `into_permanent_root` | **两条状态都补**（§4quinquies.1n）：bootstrap 根（`PrivateSupervisor`）此前**没有出口**，而它恰恰是最先要用的那条 |
| **`enable` 的归属** | 补了 `HugePages` 代理，没管 `enable` | **纳入 A 阶段清单**（§4quinquies.1n）：地址空间激活不是 PTE 修改，但**不能因此遗漏**；激活后根页与子树责任仍在调用者 |
| **共享子树回收测试入口** | 六项测试要求验证，但 `retire` 不公开 → 逻辑不一致 | **明确为 backend 内部单元测试** `destroy_private_subtrees(&mut backend, SupervisorOwnership::Borrowed)`；机制现在测，面向调用者的 API 暂缓（§4quinquies.1o）|
| **`#[must_use]` 的效力** | 写成"防止误丢" | **收紧**：它只提醒、**不构成销毁保证**；普通 `Drop` 不回收；不为此造自动清理机制（§4quinquies.1p）|
| **冻结范围的措辞** | 逐项列举 | **改为"冻结所有权模型/语义/架构/安全合同，签名与泛型允许实现中调整"**（§7.8）|
| §4ter.3.1 的 `SupervisorSubtree<'a>` | 用 `'a` 却在内部写 `'source` | **改为 `SupervisorSubtree<'source>`** |
| §4quater.2 的 `BorrowedRoot<'a>` | 仍是单一旧类型 | **拆成 `BorrowedRootMut<'a>` / `SharedPageTableSource<'source>`**，各自 SAFETY 分开写 |
| §4quater.0 的 backend 伪代码 | `reclaim_private_subtrees` | **改 `destroy_private_subtrees(&mut self, SupervisorOwnership)`** |
| §4quater.0 方法表 | 仍有 `adopt_active_root` | **并入 `from_active_root(root, channel: PteChannel)`**；`new_from_page` / `inherit_supervisor_half` 的签名字段补全 |
| §4ter.2 的 `CallerManagedPageTable` + `destroy` | 旧方案 | **标注为历史方案**，最终 API 见 §4quater.0 / §4quinquies.1n |
| `R1` 第 6 项测试 | 只说"验源共享行为" | **补一格**：源根替换入口时"陈旧映射"是允许还是必须拒绝/同步，测试须按选定合同断言 |
| `R2` 指标 | 只说"需容量实测" | **给四行指标表**（PT 高水位 / TX 实际页数 / 子区间边界对齐 / 外层 reservation 是否仍整体排除），并点明"外层 reservation ≠ `BootAlloc` 可分配子区间" |

**终版裁决**（第十版评审，照录）：

> **概念设计：冻结通过。代码实施：现在开始。生产安全验收：有条件通过**（R1、D/E、R2 分别设门）。不能因为类型拆分编译通过，就宣称页表所有权问题全部修复。

**第一批提交的建议内容**（评审原文）：

> 先完成 backend、三种 handle、两类借用凭证，以及三架构的物理页归属测试。同步补齐 bootstrap 根的 `enable` 和 `PrivateSupervisor` 永久交接路径。之后根据编译错误迁移 `BootArch::Paging`、`HugePages` 和现有调用点。

**本文档至此结束设计评审阶段。** R1 与 R2 保留为独立验证任务——不拿它们阻止基础 API 工作，也不允许基础 API 工作掩盖它们。
