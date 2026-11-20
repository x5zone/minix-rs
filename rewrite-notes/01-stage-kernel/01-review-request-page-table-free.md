# 独立评审请求：页表页归还的共享子树与所有权表达

> 本文是为了交给一个**没有任何本仓库上下文**的外部评审者而写的自包含文档。
> **版本说明**：这是**第十二版（终版）**。
> **状态**：设计已收束。第十版曾宣布冻结、第十一版收回冻结做减法审查、本版采纳减法审查的裁决结果——**十二个命名项全部废止，只保留一个状态字段 + 一组入口断言**。
> **现行 API 只有 §4ter。** §7 的 7.1–7.10 是历史记录，其中的类型名已全部废止。
> **不变的资产**：§1–§3 的取证、§4bis 的五组补充证据、§4ter.7 的 R1/R2 两个独立验证门。
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
> | 十 | 补三处实施接口；同步六处文档残留；冻结范围改为"签名允许实现中调整" | 概念设计冻结通过 |
> | 十一 | 新增 §0「设计原则」（四条判据 + 十一调用点 / 十二命名的比例 + 四处过度设计自查）；收回第十版的冻结 | **四处自查全部确认为过度设计** |
> | **十二（本版，终版）** | **按裁决压缩全篇**：§4ter 重写为「一状态 + 五断言」最小方案；§4ter.6 列十二个已废止命名；§5 改为实施清单与验收门；2678 → 1722 行 | 待评审 |

---

## 0. 设计原则（评审本设计时必须先适用的判据）

> 这一节是**刹车**，不是修辞。前十轮评审每一轮都要求"更强的合同、更多的证据、更细的类型区分"，而每一轮我都照办——结果是本文档长到 2532 行，去覆盖合计**七个操作、十一个生产调用点**的改动。没有本仓库上下文的评审者，结构上必然倾向于要求显式化。这一节要求评审者反向用力。

### 0.1 四条判据

**判据一：减法优先。** 任何方案必须回答：**有没有用更少类型、更少 trait、更少状态表达同一语义的方案？** 若有，为什么不用？只写"现有方案够好"不算回答。

**判据二：新增类型的准入三问。** 每提出一个 newtype / token / 守卫字段 / 值对象，依次回答：

| 问 | 答案决定什么 |
|---|---|
| ① **故障由谁制造？** | 外部攻击者 → `Result` + 完整校验；**可信链（本仓其他模块 → 内核）→ 优先 `assert!` / `debug_assert!` fail-fast，零类型成本**；同一调用点用错 → 才考虑类型 |
| ② **编译器 / 现有类型已经拦住了吗？** | 拦不住真故障的新增字段应当删掉，而不是命名 |
| ③ **发生率 × 后果代价 vs 理解成本？** | 理解成本按新增字段数、API 表面倍数、调用点改动数计；发生率按生产调用点数计 |

**判据三：可判边界。** 只有**同时满足**两条才够格判缺陷级：**构造点 ≥ 2**；**错用是静默的**（编译器和测试都看不见）。不满足这两条，最多算改进建议，不能升格为设计缺陷。

**判据四：教学概念账。** 每新增一个抽象，要报：引入几个新概念、讲解段落增多少、与既有叙事有没有冲突。**能让读者少记一个概念的同保障方案优先。**

### 0.2 一句话的验收标准

好的设计应当**简洁、优美、可理解、可读、教学性强**。判据是：一个读过 Minix3 对应 C 源码的工程师，能否在不读本文档的情况下从代码本身理解"这一页归谁、谁能改、什么时候还"。

如果需要读者先记住十二个类型的名字才能判断"这段代码会不会误释放"，**那这个设计就算把缺陷修好了，也仍然是不合格的设计。**

### 0.3 本设计的自查（第十一版提出，外部评审已裁决——结果见 §0.5）

下列数字均为实测，不是估计：**七个操作、合计十一个生产调用点**。

| 操作 | 生产调用点 | 位置 |
|---|---|---|
| `destroy()` | **2** | `vmproc_handle.rs:419`、`vmproc.rs:186`（都在 VM，都在 `Paging::new()` 的根上）|
| `from_active_root()` | **3** | `kerninfo.rs:143`、`lib.rs:1707`、`lib.rs:2928` |
| `adopt_active_root()` | **1** | `vm_self_map.rs:102` |
| `inherit_supervisor_half()` | **1** | `boot.rs:824` |
| `enable()` | **1** | `lib.rs:1474` |
| `new_from_page()` | **2** | `lib.rs:1292`、`boot.rs:820` |
| `Paging::new()` | **1** | `vmproc_handle.rs:364` |

而本文档建议的**新命名项共十二个**（另有三个延后）：

`PagingBackend`、`OwnedPageTable`、`CallerManagedPageTable`、`BorrowedPageTable`、`PrivateSupervisor`、`BorrowedSupervisor`、`BorrowedRootMut`、`SharedPageTableSource`、`CallerManagedRoot`、`PagingRangeClass`、`SupervisorSubtree`、`SupervisorOwnership`；延后三个：`RetireDisposition`、`RetireResult`、`RetainedPageTableRoot`。

**为十一个调用点新增十二个命名项，这个比例本身就是需要辩护的。** 下面按过度设计风险从高到低列出四处自查结果。

#### 自查一（风险最高）：`BorrowedRootMut<'a>` 与 `SharedPageTableSource<'source>`

两者都是**零业务字段 + `PhantomData` 生命期标记**。文档 §4quater.2 自己承认：

> 只给 `PhysBytes` 加一个 `PhantomData` **不能**建立借用关系。因为 `phys` 是一个整数地址，它与任何实际的 Rust 借用都没有联系。

也就是说：**它们不提供任何编译期保证，作用是把 `unsafe` 合同换一个类型名来承载。**

按判据一，等价方案是**在调用点写 `unsafe` 注释并在函数 `# Safety` 文档里列条件**，表达力完全相同，新增类型为零。按判据四，两个新概念换零收益。

**我的判断：应删。** 请评审者裁决：除了"在签名上留下一个类型名"之外，这两个类型提供了什么是注释给不出的？

#### 自查二（风险高）：`PrivateSupervisor` / `BorrowedSupervisor<'source>`

要解决的问题是"禁止重复继承"。而 `inherit_supervisor_half()` **只有一个生产调用点**（`boot.rs:824`）。

按判据三，构造点 ≥2 不成立，**不够格判缺陷级**。按判据一，等价方案是：

```rust
struct X86_64Paging { root_paddr: u64, channel: PteChannel, inherited: bool }
// inherit 里一行：assert!(!self.inherited, "supervisor half already inherited");
```

代价是 1 bit + 1 行。而类型状态方案的代价是把反省型参数 `S` 穿过 `load_elf_into`、`map_kernel`、`load_vm_elf`、`load_process_elf`、`BootArch::Paging`、以及 VM 的 `PageTable` 别名（该别名在 `cfg(test)` 下还换成 `SimPaging`，两个类型都要满足同一 bound）。

**我的判断：应降级为 `bool` + `assert!`。** 请评审者裁决：为了在一个单调用点的操作上禁止重复调用，值得改六个泛型签名吗？

#### 自查三（风险中高）：`classify_range` + `PagingRangeClass`

要解决的问题是"不许改借用上半场"。`PagingRangeClass` 的三个取值是 `Private` / `Borrowed` / `CrossesBoundary`。

按判据一，等价方案是每个架构一个三行函数：

```rust
fn is_supervisor_va(v: VirBytes) -> bool;   // x86: v.0 >= 0xffff_8000_0000_0000
                                            // arm64: v.0 >= 0xffff_0000_0000_0000
                                            // riscv64: (v.0 >> 38) == 0b111... (canonical high)
```

然后 `map` / `unmap` / `remap` 等处各加一条断言：`assert!(!is_supervisor_va(v) || private_half_owned)`。

`CrossesBoundary` 这个变体可以完全省去：批量操作先检查 `is_supervisor_va(start)` 与 `is_supervisor_va(end - 1)` 两个端点即可。

**一个 trait 方法 + 一个三值枚举 + 三架构各一份实现 + 通用层的 `ensure_private_range` 包装**，对比**三个三行函数 + 若干断言**。

**我的判断：应降级为 `is_supervisor_va` 断言。** 请评审者裁决：枚举的第三态解决了什么"两个端点比较"解决不了的问题？

#### 自查四（风险低，但需要确认）：三种 handle 类型

`OwnedPageTable` / `CallerManagedPageTable` / `BorrowedPageTable` 是本文档的主干。它们要解决的是三条**真实**危险（见下 §0.4），这三条都有确凿证据。但按判据一仍要问：

**能否不新增三种 handle，只在现有 `Paging` 类型上加一个三值 `root_owner` 字段 + 三处断言？**

- 断言版：`assert!(self.root_owner == RootOwner::PtAlloc, "destroy on non-owned root")`、`assert!(!is_supervisor_va(v) || self.supervisor == Private)`，以及 `free_child_tables` 在 `Borrowed` 时跳过顶层共享条目。
- handle 版：编译期隔离，借用型实例上根本没有 `destroy` 这个名字。

区别是**运行期拒绝 vs 编译期不存在**。前三处自查（凭证、类型状态、范围分类）的收益接近零，但这一处的收益是真实的：`destroy()` 目前在 `Paging` trait 上，**任何**实例都能调。

**我的判断：这一处需要评审者裁决是"值得"还是"过度"。** 关键权衡是：三个 handle 会改变 `CurrentPaging` 的使用方式（现为架构关联类型 `os/arch/src/lib.rs:149`），进而改动 VM 的 `PageTable` 别名与至少六个泛型签名；而三字段 + 三断言只改 `os/arch`。

### 0.4 无论怎么减法都**不能砍**的部分

以下五条是本文档取证得出的**真实缺陷**，与 API 形态无关，任何减法版本都必须保留：

| # | 缺陷 | 证据强度 |
|---|---|---|
| 1 | `destroy()` 末尾无条件 `free_pt_page(root_paddr)`，而 `new_from_page` 的四种真实根页来源**全都不在** `pt_alloc` 释放域 | 确凿（§4bis.1b）|
| 2 | `destroy()` 无 free 钩子时**不是空操作，而是清零根页**——对借用活动根等于当场抹掉页表 | 确凿（§3.3.3，三架构同形）|
| 3 | `free_child_tables` 递归进入 `inherit_supervisor_half` 建立的共享子树 | 确凿（§3.2）|
| 4 | **R1**：源根可能在继承者存活期间替换或释放共享子树，现有材料未证明 | 未核实（§4quinquies.1c）|
| 5 | **R2**：TX 从 `VmBootAllocator` 消耗的帧从未 `cut` 出 VM 的 free-list 快照 | 确凿（§4quinquies.1）|

**第 4、5 条与所有权模型无关，尤其 R2 是纯粹的物理内存分配一致性问题，不能因为本次重构而搁置。**

### 0.5 四处自查的裁决结果

第十一版提出四处自查，外部评审逐项裁决，**四项全部确认为过度设计**：

| 自查项 | 裁决 | 采纳的处理 |
|---|---|---|
| `BorrowedRootMut` / `SharedPageTableSource` 双凭证 | **过度，应删** | 删除。零业务字段 + `PhantomData` 不建立真实借用关系；`PhantomData<&'a mut ()>` 也阻止不了另一个调用点从同一 `PhysBytes` 造第二个 handle |
| `PrivateSupervisor` / `BorrowedSupervisor` 类型状态 | **过度，应删** | 改为内部运行期状态 + 状态转换 |
| `classify_range` + `PagingRangeClass` 三值枚举 | **过度，应删** | 改为架构谓词 `is_supervisor_va` + 首尾两端点断言 |
| 三种 handle | **当前过度** | 保留单个具体 `Paging` 类型，`destroy` 入口断言 |
| `PagingBackend` 层 | **过度，应删** | 没有 handle wrapper 要保护，不必单独成层 |

**评审对"断言版会漏什么"的回答**：在状态初始化正确、所有写入口完整检查、断言在任何修改之前执行的前提下，**找不到一个当前调用点可触发、却会被静默放过的具体故障**。但要诚实区分两者差异：

| 情况 | 断言方案 | 编译期 handle |
|---|---|---|
| 外部根上调用 `destroy()` | 运行到入口 panic | 编译失败 |
| 重复继承 | 运行到入口 panic | 类型状态可禁止调用 |
| 对共享上半场 `map()` | 运行到范围检查 panic | **仍需要范围检查** |
| 多个 handle 指向同一物理根 | 不能自动识别 | **也不能自动识别** |
| 其他 CPU 仍用旧翻译 | 不能自动证明 | **也不能自动证明** |
| 源根释放继承者仍在用的子树 | 不能自动阻止 | **仅靠凭证的 `unsafe from_raw` 也不能证明** |

**结论**：编译期方案的真实优势是"让本地代码更难误用"，**不是自动解决物理页表生命周期、同步与 TLB 问题**。对当前主要是可信内部代码、只有 2 个生产销毁调用点且都确认为 `Paging::new()` 的项目，运行期断言是合理取舍。

**四项实施要求**（断言版成立的前提）：

1. 状态字段**私有**，只能由构造函数与 `inherit` 方法修改；
2. 每个构造函数都设置正确状态；
3. `destroy()` 用 `assert!`，且在**任何内存操作之前**执行；
4. 测试覆盖**所有**构造方式，不只是 `Paging::new()`。

**一条重要边界**：所有公开 PTE 修改入口必须经过检查。不能只在 `map()` 加断言却漏掉 `map_huge` / `split_huge` / `grant_user_walk`；直接写 PTE 的内部辅助函数也不能被外部调用绕开。**这才是断言方案的真实安全边界。**

**最终方案见 §4ter。**

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

## 4ter. 最终方案：一个状态字段 + 一组入口断言

前面 §4bis 的取证与 §3 的问题清单是资产，保留。但从第三版到第十版累积的 API 处方（三层结构、三种 handle、两类凭证、类型状态、范围分类枚举）**经减法审查后整体废止**。本节是唯一有效的 API 合同。

### 4ter.1 为什么不用类型系统

前十轮评审把"必须正确表达的资源责任"逐步扩张成了"必须由独立 Rust 类型表达的每一种概念"。这两件事不等价。

判据（见 §0.1）：真实危险只有三条（§0.4 的第 1、2、3 条），而它们对应的生产调用点合计 **11 个**；建议的类型方案需要 **12 个新命名项**。按 §0.1 判据三，其中三处的构造点不足 2（`inherit_supervisor_half` 只有 1 个、`enable` 只有 1 个、`adopt_active_root` 只有 1 个）。

更关键的一条：**类型方案最强的能力，是让某个变量在语法上没有 `destroy` 方法。它并不能证明别处没有对同一个物理页表做销毁、修改或激活。** 而后者才是真实风险所在（跨 CPU、TLB、另一个 handle 别名）。这些无论如何都要靠 `unsafe` 合同 + 实证。

所以采用：**一个具体 `Paging` 类型 + 一个私有状态字段 + 一组入口断言。**

### 4ter.2 状态机（三态，不是三维度）

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
enum PagingState {
    /// 根页及其页表树由本 PT_ALLOC 域拥有（Paging::new() 建）
    Owned,
    /// 根页由外部管理；本 handle 不得释放它
    External,
    /// 外部根 + 已继承的共享上半场
    Inherited,
}

pub struct X86_64Paging {
    root_paddr: u64,
    channel: PteChannel,
    state: PagingState,      // 私有；只有构造函数与 inherit 能改
}
```

aarch64、riscv64、`SimPaging` 同语义各一份。

**`External` 特意同时覆盖 `new_from_page()` 与 `from_active_root()`**: 两者初始化语义不同，但就"当前 handle 不拥有释放根页的权利"这一点，规则是同一条。`Inherited` 在此基础上多一个事实：上半场有共享子树。

#### 状态转移表

| 构造或操作 | 结果状态 | 允许的行为 |
|---|---|---|
| `Paging::new()` | `Owned` | 可销毁 |
| `new_from_page()` | `External` | 不可销毁；可建映射 |
| `from_active_root()` | `External` | 不可销毁；可在同步合同下修改 |
| `inherit_supervisor_half()` | `Inherited` | 不可销毁；**禁止修改继承上半场** |

### 4ter.3 三处入口断言

**`destroy()`**

```rust
unsafe fn destroy(self) {          // 由 &mut self 改消费型
    assert_eq!(
        self.state, PagingState::Owned,
        "destroy called on externally managed page table"
    );
    // 原有的 CPU 静止与 TLB 前提不变
    // 原有的私有树递归销毁不变
}
```

**这个断言必须在任何根页清零、递归遍历、物理页释放之前执行。** 位置错了就无效：

```rust
// 错误：先按 free 钩子分支，再断言 —— 清零已经发生
if !pt_alloc::is_free_registered() { zero_root(); return; }
assert_eq!(self.state, PagingState::Owned, ...);

// 正确：先断言
assert_eq!(self.state, PagingState::Owned, ...);
if !pt_alloc::is_free_registered() { /* 原有降级行为 */ }
```

**没有 `PT_FREE` 时物理页不归还**这一行为不变，写进文档即可，不增加返回值类型。

**`inherit_supervisor_half()`**

```rust
unsafe fn inherit_supervisor_half(&mut self, source_root: PhysBytes) {
    assert_eq!(self.state, PagingState::External, "already inherited");
    assert!(self.supervisor_half_is_empty(), "target half not empty");
    // SAFETY: 调用者保证源树有效与稳定（见 §4ter.5 的 R1）
    self.copy_supervisor_entries(source_root);
    self.state = PagingState::Inherited;      // 复制完成之后才转移
}
```

三条要求：只能从 `External` 进入（重复继承自然失败）；复制**之前**检查目标上半场为空；复制**之后**才改状态。

**修改入口的共享范围保护**

```rust
fn ensure_writable_range(&self, start: VirBytes, len: usize) {
    if self.state == PagingState::Inherited {
        assert_private_range::<Self>(start, len);
    }
}
```

```rust
fn assert_private_range<P: Paging>(start: VirBytes, len: usize) {
    if len == 0 { return; }
    let last = start.0.checked_add(len as u64 - 1)
        .expect("virtual range overflow");
    assert!(!P::is_supervisor_va(start));
    assert!(!P::is_supervisor_va(VirBytes(last)));
}
```

**首尾两端点检查足以覆盖当前边界**，不需要三值枚举：

| 区间 | 结果 |
|---|---|
| 全在私有区域 | 接受 |
| 全在借用上半场 | 拒绝 |
| 私有 → 借用（跨界）| 拒绝（末端点命中）|
| 借用 → 私有（跨界）| 拒绝（首端点命中）|

**必须覆盖全部写入口**，不能只加在 `map()`：

| 入口 | 检查 |
|---|---|
| `map` / `map_range` / `map_huge` | 目标范围（`map_huge` 按**实际映射大小**，不能只查起始 VA）|
| `unmap` / `unmap_range` | 整个解除范围 |
| `remap` | **源与目标都查**，两项都过才动手 |
| `update_flags` | 目标地址 |
| `grant_user_walk` | 目标范围 |
| `split_huge` | 被拆分地址 |

批量操作**必须在修改任何 PTE 之前**检查完整范围——不能先改私有区、遍历到边界才发现跨界。

`query` / `root_paddr` / `walk` 不需要；`enable` 不是范围操作，不走这个 guard，但保留其原有安全条件。

**`is_supervisor_va` 的语义**：按**当前页表继承的顶层地址范围**定义，不是查某个 PTE 的 User/Supervisor 权限位。两者不是同一个概念。每架构三行：

| 架构 | 判据 |
|---|---|
| x86-64 | `va.0 >= 0xffff_8000_0000_0000`（PML4 条目 ≥256）|
| aarch64 | `va.0 >= 0xffff_0000_0000_0000`（TTBR1 区，T1SZ=16）|
| riscv64 | Sv39 规范高半场（bit 38 起全 1）|

**为什么断言而不是 `Result`**：当前继承路径属于可信内部代码，越过已继承的共享边界是调用方违反内部不变量 → `assert!` 合适。**必须用 `assert!`，不能用发布构建会消失的 `debug_assert!`。** 将来若该接口接收未经信任的输入，应改返回错误。

### 4ter.4 问题 3 的答案：只改 `destroy()` 不够，但只需要这五条

| 只改什么 | 为什么不够 |
|---|---|
| 只改 `is_free_registered()` 判断 | 不能证明根页属于这个 allocator |
| 只改 `destroy()` 的行为 | 无 free 钩子时旧代码仍清零根页；且继承后的 `map()` 仍能改共享上半场 |

**最小充分修复就是上面那五条**：构造记状态 → `destroy` 只许 `Owned` → `inherit` 只许 `External` 并检查空半场 → `Inherited` 拒绝写共享范围 → 不公开任何能销毁 / 回收 `External` / `Inherited` 的接口。

### 4ter.5 三条真实危险如何被覆盖

| 危险 | 覆盖方式 |
|---|---|
| **① 外部根页被交给 `pt_free`** | `new_from_page()` 得到 `External`，`destroy()` 入口断言拒绝。UEFI / boot-shim / `bootface_a64` / `VmBootAllocator` 四种根页都不会进入 `free_pt_page(root_paddr)`。不需要记录每页 provenance |
| **② 无 free 钩子时清零借用活动根** | 同一入口断言阻止。**注意断言位置**（§4ter.3 的正误对照）|
| **③ 递归释放共享子树** | **用前置约束解决，不实现跳过机制**：`Owned` 由 `Paging::new()` 建、不继承；`inherit` 只许 `External` 进入；继承后成为 `Inherited`；`destroy` 只接受 `Owned`。所以 `free_child_tables()` 只会作用于自有私有树，**可以继续遍历全部顶层条目，不需要理解共享关系** |

**第 ③ 条的减法**：过去打算用"遍历时识别共享条目"解决，现在用更简单的前置约束解决——**共享页表不允许进入销毁路径**。比实现根层跳过机制简单，也符合当前没有回收需求的事实。将来真出现"外部根要释放自己建的私有中间表但保留共享子树"的调用点，再加专用回收路径。

### 4ter.6 明确不做的事

| 不做 | 理由 |
|---|---|
| `PagingBackend` 层 | 没有 handle wrapper 要保护，不必为它单独加一层 |
| 三种 handle（`OwnedPageTable` / `CallerManagedPageTable` / `BorrowedPageTable`）| 对 11 个调用点过重；最强能力只是"语法上没有 `destroy`"，证明不了别名/跨 CPU |
| 两类凭证（`BorrowedRootMut` / `SharedPageTableSource`）| 零业务字段 + `PhantomData`，文档自己承认不建立真实借用关系，纯把 `unsafe` 合同换类型名 |
| 类型状态（`PrivateSupervisor` / `BorrowedSupervisor`）| 单调用点；`bool` + `assert!` 等价 |
| `classify_range` + `PagingRangeClass` | 首尾两端点比较够用 |
| `CallerManagedRoot` / `into_permanent_root` | 调用点本来就有 `PhysBytes`，不加凭证 |
| `SupervisorSubtree` / `SupervisorOwnership` | 状态机已表达 |
| `RetireDisposition` / `RetireResult` / `RetainedPageTableRoot` / `retire_owned_subtrees` | 无实际调用需求，不提前概念化 |

**这些名字不是永久禁止，是"没有真实使用需要时不提前变成项目概念"。**

### 4ter.7 缺陷仍然要修，证据责任不因减法而缩小

**R1（源根修改是否破坏继承者）**——`Inherited` 只保护目标根不主动改共享范围，**不能**阻止源根替换顶层入口、释放旧子树、复用继承者可达的页表页、或改叶 PTE 后漏 TLB。**删除凭证类型不会让 R1 的安全保证减少**：凭证本身没有真实 owner，替代不了对源根修改路径的实际核实。R1 仍是真实 bootstrap 继承路径的验证门。

#### R1 的三种变化与处理


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


**R1 需要核实什么**（`harden_kernel_text` `lib.rs:1707`、heap-window eviction `lib.rs:2928` 不能只记录"调用过 `split_huge`/`update_flags`/`unmap`"）：

- 写入的是**共享子树里的下级页表页**，还是**源根自己的顶层入口**？
- 是否**清除一个仍被继承者根页直接引用的入口**？
- 释放或复用的物理页**是否仍在继承者可达集合内**？
- TLB 失效**覆盖哪些地址空间和 CPU**？

**关于 TLB**（锁不能替代）：协调对象不只是改源根的 CPU，还包括所有可能使用这些共享子树的地址空间与 CPU。**BKL 存在不等于 TLB 合同成立。**

**R2（TX 与 VM free-list 重叠）**——与所有权 API 无关，类型变复杂或变简单都不改变这个事实。真实 VM 消费 free-list 前必须关闭。


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


### 4ter.8 四个活动根调用点实际调了哪些方法


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


---

## 5. 实施清单、验收门与测试

设计已在 §4ter 收束为「一个状态字段 + 一组入口断言」。本节只列实施与验收。

### 5.1 实施顺序

| 阶段 | 工作 | 验收标准 |
|---|---|---|
| **A** | 三架构 `PagingState` 字段 + 三个构造函数设状态 | 状态字段私有；每个构造方式都设对 |
| **B** | `destroy(self)` 消费型 + 入口断言 | 外部根上 `destroy` 必然 panic；断言在**任何内存操作之前** |
| **C** | `inherit_supervisor_half` 状态转换 + 目标半场为空检查 | 重复继承 panic；覆盖私有半场被拒 |
| **D** | `is_supervisor_va` 三架构 + 全部写入口范围断言 | 六行清单全覆盖；批量操作改前先查 |
| **E** | 四个活动根调用点的同步与 TLB 依据 | 每点答三问（见 5.3）|
| **F** | R1 实证：源根修改路径核实 | 四个问题逐个有结论 |
| **G** | R2 修复：TX 专用保留区 + 容量实测 | 真实 VM 接入前必须关闭 |

**D 与 E 分开开发、合并验收**：D 完成而 E 未过时，代码已有范围检查却仍允许修改一个缺乏同步证明的活动根——**不能标记为"生产安全改造完成"**。

### 5.2 两条独立的证据责任（减法不削减它们）

| # | 门 | 内容 | 关闭条件 |
|---|---|---|---|
| **R1** | 源根结构稳定性 | `Inherited` 只保护目标根不主动改共享范围，**不能**阻止源根替换顶层入口 / 释放旧子树 / 复用继承者可达的页表页 / 改叶 PTE 后漏 TLB | `harden_kernel_text`（`lib.rs:1707`）与 heap-window eviction（`lib.rs:2928`）的四个问题逐个核实（§4ter.7）|
| **R2** | TX 与 VM free-list 重叠 | 与所有权 API 无关；类型变复杂或变简单都不改变这个事实 | TX 专用保留区落地 + 容量实测（§4ter.7）|

**删除凭证类型不会让 R1 的安全保证减少**：凭证本身没有真实 owner，替代不了对源根修改路径的实际核实。

### 5.3 E 门：四个调用点各答三问

| 调用点 | 位置 | 实际调用的写方法 |
|---|---|---|
| `init_kerninfo` | `kerninfo.rs:143` | `query` + **`map`** |
| `harden_kernel_text` | `lib.rs:1707` | **`split_huge`** + **`update_flags`** |
| heap-window eviction | `lib.rs:2928` | `query` + **`split_huge`** + **`grant_user_walk`** + **`unmap`** |
| VM 自映射 | `vm_self_map.rs:102` | **`map` / `unmap` / `unmap_range`** |

**四个全部会写页表，没有一个只查询。**

每个调用点必须回答：

1. 哪个锁、单线程约束或启动顺序**防止未协调的并发修改**？
2. 修改时可能有哪些 CPU 正在使用该根，或使用共享子树的其他根？
3. 修改后由谁执行 TLB 失效，覆盖哪些 CPU、地址空间、翻译缓存范围？

> 锁解决并发修改协调，TLB 失效解决旧翻译仍可能被使用；两者不能相互替代。**BKL 存在，也不能单独证明 TLB 合同成立。**

### 5.4 三架构测试清单（六项）

不能只检查"总共释放了几页"。

| # | 行为 |
|---|---|
| 1 | **私有上半场**：`Owned` 页表销毁时，私有子表应被释放 |
| 2 | **根页归属**：`External` / `Inherited` 根页**不得**进入 `free_pt_page`，也**不得**被清零 |
| 3 | **状态拒绝**：在 `External` / `Inherited` 上调用 `destroy` 必须 panic；重复 `inherit` 必须 panic |
| 4 | **范围拒绝**：`Inherited` 状态下 `map` / `unmap` / `update_flags` / `split_huge` / `map_huge` 修改共享范围，**必须在写入任何 PTE 前失败** |
| 5 | **批量原子性**：跨边界的 `map_range` / `unmap_range` / `remap` 被拒后，**私有区域也不能留下部分修改** |
| 6 | **源共享行为**：共享子表内部允许的修改会被继承者看到；源根替换入口或解映射时**不得释放仍被继承者引用的页** |

**第 3、4 项是断言版的命门**：它们直接验证"断言在任何修改之前执行"这条实施要求。

**第 6 项还要补一格**：源根顶层入口被替换时，目标根可能仍指向旧子树——这**可能是暂时安全但语义陈旧**的状态。测试不能只断言"旧子树没释放"，还要按最终选定的合同明确：**陈旧映射是允许的，还是必须在替换时同步更新 / 拒绝替换。**

### 5.5 已确认的行为，写进文档而非加类型

| 行为 | 处理 |
|---|---|
| 无 `PT_FREE` 时物理页不归还 | 写进 `destroy` 文档（"页表对象不再由 handle 管理，物理页不重新进入 allocator"），不加返回值类型 |
| `Drop` 不自动回收 | 保持显式 `destroy`；`#[must_use]` 只提醒，**不构成销毁保证** |
| `enable` 不是范围操作 | 不走范围 guard，保留原有安全条件 |
| `ProcessLoad` 保持裸 `PhysBytes` | 唯一调用者是 QEMU 测试内核；补一行"该路径不支持后续回收"的文档，不加泛型 |

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

> **⚠ 读这一节前必读**：§7.1–§7.10 是**历史记录**，其中出现的 `PagingBackend`、三种 handle、`BorrowedRootMut`、`SharedPageTableSource`、`PrivateSupervisor`、`BorrowedSupervisor`、`classify_range`、`into_permanent_root` 等名称**全部已废止**（见 §4ter.6 的不做清单）。保留这些历史是为了让评审者看到"为什么走到了最小方案"，**不是现行 API**。**现行 API 只有 §4ter。**


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

**冻结范围（第十版评审措辞；第十一版已收回冻结，见 §0 / §7.11）**：

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

### 7.11 第十版 → 第十一版

| 项 | 第十版 | 第十一版 |
|---|---|---|
| **状态** | 宣布概念设计冻结、代码实施开始 | **收回冻结**；API 形态重开裁决 |
| **文档规模** | 2532 行，未做减法自查 | **新增 §0「设计原则」**，把四条判据写在最前面 |
| **比例尺** | 未统计 | **实测：七个操作、十一个生产调用点 vs 十二个新命名项**（§0.3 表）|
| **过度设计自查** | 无 | **四处**（§0.3）：双凭证（应删）/ 类型状态（应降级为 `bool`）/ `classify_range`（应降级为三行函数断言）/ 三 handle（这一处需裁决）|
| **不可砍清单** | 分散在全文 | **§0.4 集中列五条**，并点明 R1/R2 与所有权模型无关 |
| **给评审者的三问** | 前几版问"设计对不对" | **§0.5 改问"哪里过度、断言版会漏什么、有没有更简方案"** |

**本版的核心变化是把判据从"设计是否完备"翻转成"设计是否值得它的成本"。** 前十轮评审的方向是单向加码，本版开始做反向压力测试。

**请评审者特别注意 §0.5 第 3 问**：是否存在只改 `destroy()` 一条（按来源决定是否 `free_pt_page` + `Borrowed` 时跳过共享条目）就覆盖了大部分真实危险的方案。如果存在，前面十轮的类型体系就应当整体降级。

### 7.12 第十一版 → 第十二版

| 项 | 第十一版 | 第十二版 |
|---|---|---|
| **文档规模** | 2678 行 | **1722 行**（压缩 36%）|
| **API 处方** | §4ter 至 §4quinquies 全部保留三层结构、三 handle、双凭证、类型状态、范围枚举 | **整段替换为单一最小方案**（§4ter：一个 `PagingState` 三态 + 五条入口断言）|
| **废止命名** | 建议保留或待裁 | **十二个全部废止**（§4ter.6 明确列名 + 理由）；三个延后项同样不提前概念化 |
| **`destroy` 签名** | `destroy(&mut self)` 依 handle 类型区分 | **统一 `destroy(self)` 消费型 + 状态断言** |
| **共享子树回收** | 要实现的根层跳过机制 | **用前置约束解决**：共享页表不允许进入销毁路径，`free_child_tables` 继续遍历全部条目 |
| **凭证 / 生命周期** | 两个 `PhantomData` 类型 + `into_permanent_root` | **全部删除**；调用点本来就有 `PhysBytes` |
| **§5 的性质** | 开放问答 | **实施清单 + 两条证据门 + 六项测试** |
| **R1 / R2** | 两个待核实项 | **不变，且明确"删凭证不减少 R1 的安全保证"** |

**评审的取向我照录作为本文档的收尾**：

> 一个优秀的 OS API，不应该让读者先学习一套所有权类型系统，才能看懂页表页的生命周期。读者看到 `Paging::new()` 就知道这是由页表分配器创建、由 `destroy()` 负责结束的页表；看到 `new_from_page()` 就知道根页来自外部、当前 handle 不会释放它；看到 `inherit_supervisor_half()` 就知道这个根从此有一段共享区域、普通映射操作不能改它；看到 `destroy()` 就知道它只接受自有页表。

> 这套叙事足够直观，而且与 Minix3 C 代码的阅读路径接近：**先看谁分配，再看谁修改，最后看谁释放。**

**实施起点**：§5.1 的 A–G 七阶段。A–D 可立即开始；E 与 D 合并验收；F（R1）与 G（R2）是独立门，不阻止 A–D，也不允许 A–D 掩盖它们。
