# Panic-in-Drop 防御：KProcess / KPriv 的 slot ownership invariant

> **状态**：已实施（2026-09-02）。`KProcess` 与 `KPriv` 均已加防御性 `Drop`，测试夹具体系（`os/kernel/src/test_helpers.rs`）同步落地。
> **设计来源**：2026-08-31 与 GPT 的两轮设计讨论（完整记录见 [`AI-chats/comments.md`](../../../AI-chats/comments.md)）
> **关联设计**：与 `BklProtected` sealed trait + `BklSection` witness 同一系列——kernel object 生命周期类型化保护

---

## 1. 设计原则（三分类法）

把 kernel 对象的生命周期分为三类，每类对应不同的 Drop 语义：

| 类别 | Rust 生命周期 vs OS 生命周期 | Drop 策略 | 本仓库典型 |
|---|---|---|---|
| **A. 普通值** | 等价 | 正常 Rust Drop（隐式释放即正确） | `CpuMask`、`ProcNr`、`Endpoint`、`Quantum`、`TimeStats` |
| **B. Kernel state object** | **不等价**——slot 持续存在，字段修改 ≠ 对象销毁 | **`Drop + panic`**（意外 drop 视为 kernel invariant violation） | `KProcess`（`proc.rs`）、`KPriv`（`kpriv.rs`）；`AddressSpace`、`IPC filter`、`Scheduler entity` 为同类未来候选 |
| **C. Rust-only 资源 owner** | 等价——Drop 本身就是正确的 OS 释放 | 正常 Drop（释放即正确） | `BklGuard`（drop 释放 BKL）、未来的 `SpinLockGuard`、`temp alloc guard` |

**核心断言**：

> 所有具有 OS 生命周期语义的 kernel object（B 类），不应该默认拥有 Rust 的 RAII destruction semantics。Rust 所有权 ≠ OS 资源所有权，slot 的生命周期必须由显式 OS protocol 管理（`init`/`reap`/`clear`/`recycle`），Rust `Drop` 只作为"绝不应该发生"的报警器。

三分类法的判断标准是**"drop 这个值在 OS 语义下意味着什么"**：释放资源（A/C）还是销毁一个持续存在的实体（B）。判断归属不是看字段多少，而是看该对象是否被**表/链/引用**结构登记——被登记的对象其生命周期由登记关系决定，不由 Rust 值的存活决定。

---

## 2. 为什么是 KProcess / KPriv

按 GPT 第一轮意见，B 类对象优先给**生命周期极其敏感、存在跨对象反向引用/链表/表项关系**的对象：

```
KProcess
 ├── caller_q_head / caller_q_tail / send_q_link  ← IPC sender queue（跨 slot 反向引用）
 ├── p_rts_flags / RTS_SENDING / RTS_RECEIVING    ← 调度链表 / IPC 链表
 ├── priv_id: Option<PrivId>                       ← 跨表引用 PrivTable
 └── 被 AlarmTimerNode 链（侵入链节点）反向引用    ← timer chain

KPriv
 └── PrivRuntime.s_alarm_timer                     ← 闹钟侵入链节点
```

**危险场景**：

```rust
proctab[i] = KProcess::new(...);  // 旧 slot 整体覆盖
// → 旧 KProcess 的 caller_q_head 静默消失
// → 其他 slot 的 send_q_link 仍指向该 slot
// → 链永久断裂 → 系统数据彻底损坏 → 无法恢复
```

C 语言中"覆盖活槽"是合法的（内存操作）；Rust 中同一次覆盖会把旧值**隐式 drop**。若 drop 静默执行，链断裂就无声发生。防御性 `Drop` 把这条路径变成 fail-fast：覆盖活槽时旧值 drop → `panic!`，错误在开发期暴露而非运行期变成悬链。

**B 类对象的 Drop + panic（已实施形态）**——注意 panic 只检查"槽是否占用"，不执行任何释放：

```rust
// proc.rs — KProcess
impl Drop for KProcess {
    fn drop(&mut self) {
        if self.slot_is_occupied() {
            panic!(
                "BUG: occupied KProcess slot #{} dropped without explicit destruction",
                self.p_nr.0
            );
        }
    }
}

// kpriv.rs — KPriv（对称设计）
impl Drop for KPriv {
    fn drop(&mut self) {
        if self.slot_is_occupied() {
            panic!(
                "BUG: occupied KPriv slot (s_id={}) dropped without explicit destruction; \
                 clear the slot binding first (s_proc_nr = None)",
                self.identity.s_id
            );
        }
    }
}
```

占用判定分别取自两个槽的权威空闲标记：

- `KProcess`：`SLOT_FREE` 位（`proc.rs:1350` 初始化，C: `RTS_SLOT_FREE`，isemptyp 判空宏 proc.h:273-274）。位**置位** = 空槽，清除 = 占用。
- `KPriv`：`identity.s_proc_nr: Option<ProcNr>`（`kpriv.rs`，C: `priv[priv_id].s_proc_nr != NONE`）。`None` = 未绑定，`Some` = 已绑定。

判定逻辑提取为 `slot_is_occupied()` 方法，供 `Drop` 体使用，也让判定本身可被单元测试钉死（见 §5 测试策略——`panic = "abort"` 下无法用 `#[should_panic]` 捕获 abort，只能测判定谓词）。

`Drop + panic` **同时阻止 `Copy`**——Rust 规则"实现 `Drop` 的类型不能实现 `Copy"——KProcess 不再是"一坨可以随便复制的值"，而是"有生命周期语义的实体"。这是 `Drop + panic` **比单纯注释禁止 Drop 更有价值**的原因：类型系统从文法上拒绝 `let p2 = p1;` 这种把活进程当普通值复制的写法。

---

## 3. 不要做的事

### 3.1 不要把 RTS 状态做成完整 typestate

GPT 第二轮意见明确反对：

> KProcess 的状态空间不是简单生命周期，而是**多维状态向量**：
> - Lifecycle：UNUSED / USED / ZOMBIE
> - RTS：SENDING / RECEIVING / NO_QUANTUM / ...
> - Scheduler：ready / not ready / scheduler assigned
> - IPC：caller queue / sendto / getfrom
> - Privilege / Signal / CPU / Timer
>
> 强行 typestate 化会变成 `BlockedProc<Receiving>` / `BlockedProc<Sending>` / `RunnableProc<NoQuantum>` / ...——类型系统追着 Minix 的位图状态跑，**喧宾夺主**。

`Drop + panic` 与 typestate 的边界：前者只关心**一个二元问题**（槽占用与否），是一维判定；后者要建模整个多维状态向量。Drop 判定不随 RTS 状态增长，所以不会演化成 typestate。

### 3.2 不要给所有 kernel object 一刀切

`Quantum` / `CpuMask` / `TimeStats` 是普通值（类别 A），不应加 Drop + panic——会引入无意义的编译期约束。判定标准见 §1 末段（登记关系）。

### 3.3 不要用 `Box::leak` / `&'static` 绕过测试 fixture

> 如果为了让 `let p = KProcess::new_zeroed();` 不触发防御性 Drop，而把测试改成 `Box::leak(Box::new(p))`，
> 你实际上是在**绕过 invariant**。
>
> 测试应该反过来**证明**：
> - 正常的 KProcess 生命周期路径**不会** Drop（pass）
> - 错误地让 KProcess 离开 owner → Drop → panic（**专门测试**）

实施中采用了与 `Box::leak` 互补的**显式豁免夹具**（`os/kernel/src/test_helpers.rs`）：单元测试构造的"槽形状 scratch 值"（表、数组、单进程）用带 `ManuallyDrop` 的 wrapper 类型持有，scope 结束时**不触发**槽位 `Drop`。豁免写进类型名（`TestProcTable` / `TestProcArray` / `TestKProc`），不藏在 `Box::leak` 里。三层夹具与它们的对象一一对应：

| 测试对象 | 豁免夹具 | 构造函数 |
|---|---|---|
| `ProcessTable`（局部表） | `TestProcTable` | `test_proc_table()` |
| `PrivTable`（局部表） | `TestPrivTable` | `test_priv_table()` |
| `[KProcess; N]`（IPC 引擎测试的迷你表） | `TestProcArray<N>` | `scratch_procs()` |
| 单个 `KProcess`（独立 scratch 值） | `TestKProc` | `scratch_kproc()` |

豁免原则的边界：**只豁免"测试把槽塑造成占用形态但从不执行 OS 生命周期"的场景**。测试要走真实生命周期（构造表 → 占用槽 → 显式清理）时直接使用原始类型。integration test（无法访问 crate 私有的 `test_helpers`）用 `ManuallyDrop::new(ProcessTable::new())` 显式标注同一豁免。

必须强调的例外：**`ProcessTable::new()` 的 IDLE 槽从构造起就是占用**（常驻内核任务的 production 语义，`proc_table.rs:new`），所以"真实"局部表 drop 必然触发报警——这是设计使然：**表只在 `PROC_TABLE` 静态中永生**，任何脱离静态的临时表都必须显式豁免。

---

## 4. 与现有 BklSection 的关系

现有 `BklSection<'a>` 是**并发层 typestate**（"BKL 持有证明"）；`Drop + panic` 是**生命周期层 typestate**（"slot ownership 不变量"）。**两层正交、可叠加**：

```rust
pub fn dispatch_clear(bkl_section: &BklSection<'_>, nr: ProcNr) {
    let active: ActiveProc = proc_table_with(bkl_section)
        .try_get_active(nr)
        .expect("process not active");
    let _empty: EmptyProc = active.reap();
    // 编译期 witness：BKL 持有 + 状态转换合法
    // 运行期防御：意外 Drop 触发 panic
}
```

区别一句话：**BklSection 管"并发合法性"，防御性 Drop 管"生命周期合法性"**。前者是用类型证明"锁在我手上"，后者是用运行时报警捕获"值消失得不对"。

---

## 5. 实施清单（2026-09-02 已完成）

按 fix-guard 原则，动手前逐项核实，全部通过后实施：

| 检查项 | 核实结果 | 结论 |
|---|---|---|
| **kernel 两个 profile 均为 `panic = "abort"`** | workspace `Cargo.toml [profile.dev]/[profile.release]` + kernel `Cargo.toml` `[profile.dev]`/`[profile.release]` 均为 `panic = "abort"` | abort 保证 panic-in-drop 不会 unwind 展开（panic-in-panic 危险），也不会出现"析构时再 panic → abort"的二次崩溃路径 |
| **生产路径无隐式 drop 活槽** | `fork_from` child 写入的是空槽（`syscall_process.rs:199` `*slot = child`，旧槽 SLOT_FREE）；`swap_slots`（`proc_table.rs:126-141`）用 `mem::swap` 位搬运不触 Drop；`PROC_TABLE`/`PRIV_TABLE` 是 `static`，永不 drop | 生产侧零改动，防御纯增 |
| **B 类对象完整清单** | 首批实施 `KProcess` + `KPriv`（两个都触发过真实 panic 场景审查）；`AddressSpace`/`IPC filter`/`Scheduler entity` 留作未来同类候选，不在本次范围 | 范围收敛 |
| ****`mem::swap` 不 drop 验证** | `swap_slots`（`proc_table.rs:126-141`）内部 L133-139 用 `mem::swap` 交换两个槽——位搬运不触发 `Drop`，防御性 `Drop` 不干预交换本身 | swap 路径无 drop 干扰 |
| **fixture 影响面** | 测试中局部表构造 198 处（`ProcessTable::new()`）+ 173 处（`PrivTable::new()`），局部占用进程/数组数百处 | 全部走三类夹具豁免，`cargo test` 全绿（kernel 615 单测 + 2 集成，含 4 个新增 Drop 行为测试） |

**测试策略（drop panic 的硬边界）**：

- 测试构建（`cargo test`，Cargo `test` profile）实际是 **unwind**——workspace 的 `panic = "abort"` 只约束 `dev`/`release`，Cargo 的 `test` profile 默认 `panic = "unwind"`（既有 `#[should_panic]` 测试因此能跑）。但 `Drop` 体内的 panic 有两条**不可捕获**路径：
  1. **abort build**（dev/release）：panic 直接终止进程，无 `should_panic` 可言。
  2. **unwind build**（test）：若 drop 发生在 unwind 清理阶段（测试中途 assert 失败、或前一个 panic 正在展开），析构中出现第二个 panic 触发 Rust 的 "panic in a destructor during cleanup" 规则 → **强制 abort**（本次实施中 ipc 测试的 abort 即此路径）。普通 drop panic 即使被 `catch_unwind` 捕获（报红），也被多占用槽的清理解耦成随机 abort。
- 因此"drop 占用槽 → panic"路径**不能用 `#[should_panic]` 断言**，改为两个稳定层次：
  1. **判定谓词测试**：`slot_is_occupied` 与 `SLOT_FREE` 位 / `s_proc_nr` 绑定的一致性（`proc.rs::tests::test_slot_is_occupied_agrees_with_slot_free_bit`、`kpriv.rs::tests::test_slot_is_occupied_agrees_with_binding`）。
  2. **正常路径测试**：空槽 drop 不 panic（`test_drop_empty_slot_is_normal`、`test_drop_unassigned_slot_is_normal`）。
- panic 路径本身由"任何未豁免的占用槽 drop 即 panic/abort"在 CI 中承担——测试套件本身就是报警器：一旦有人写了覆盖活槽的代码，测试进程立刻崩溃，错误定位到具体测试。

---

## 6. 触发条件与实施决策（2026-09-02）

原设计定义了五个触发条件，本次实施的理由如下（命中第 3、4 条）：

1. ~~实际"slot 被覆盖"导致的 bug 出现~~ —— 未发生，此前是保守等待的理由
2. ~~启动 RTS 状态机大重构~~ —— 未发生
3. **Kernel `panic = "abort"` 验证完成** —— 已确认（§5 第一行），解锁 Drop + panic 的安全性前提
4. **fixture 迁移成本评估完成** —— 影响面量化后确认可控（§5），决策为"严格 Drop + panic + 专职夹具"
5. ~~PM/VFS 端的 typestate 引入~~ —— 未发生

**实施决策记录**：

- **严格模式**：占用即 `panic!`，不做"测试特判"（如 `cfg(test)` 分支）——特判属于 §3.3 禁止的绕过，会让报警器失效。
- **夹具落地为统一 helper**：所有测试局部表的构造点统一替换为 `test_proc_table()` / `test_priv_table()`；IPC 测试的迷你表数组统一 `scratch_procs()`；独立 scratch 进程统一 `scratch_kproc()`。豁免是类型名可见的，而非散落各处的 `mem::forget`。
- **integration test 无法访问 crate 私有夹具**：`boot_integration.rs` 用 `ManuallyDrop::new(ProcessTable::new())` 显式标注豁免（表只读，不涉及生命周期）。

---

## 7. 与 VM typestate 的对比

| 维度 | VM (vmproc) | Kernel (KProcess) |
|---|---|---|
| 跨 slot 反向引用 | ❌ 无（地址空间独立） | ✅ 有（caller_q / 闹钟 / priv_id） |
| 适合完整 typestate？ | ✅ 适合（Empty → Active → Exiting 封闭状态机） | ❌ 不适合（多维状态向量） |
| 当前防御模式 | `debug_assert!` + 防御性 `clear()`（draft/01-vmproc-struct.md:847） | **`Drop + panic`**（kernel invariant 违反不可恢复） |
| Drop 触发后果 | release 静默清理 + 计数 | panic + abort（kernel 终止） |

**两者防御策略不同是合理设计，不是设计不一致**——kernel 不可恢复，VM 服务可重启。判据是服务是否可以被独立重启：能重启的服务（VM/PM…）犯错后静默收敛是可接受的；不可重启的 kernel 内部状态犯错必须立即暴露。

---

## 8. 状态与后续

- **当前**：已实施（2026-09-02），涉及文件：
  - `os/kernel/src/proc.rs` —— `KProcess::slot_is_occupied` + `impl Drop`
  - `os/kernel/src/kpriv.rs` —— `KPriv::slot_is_occupied` + `impl Drop`
  - `os/kernel/src/test_helpers.rs` —— 三层豁免夹具 + 构造函数
  - 16 个测试源文件的局部表/局部进程构造点统一迁移到夹具
  - `os/kernel/tests/boot_integration.rs` —— `ManuallyDrop` 显式豁免
  - `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/06-proc-init-boot-proc.md` §3.1.1 —— 资源所有权模型同步
- **验证**：`cargo test`（os/ workspace）全绿；kernel 615 单测 + 2 集成，含新增的 4 个 Drop 行为测试。
- **后续候选**（同类 B 类对象，未定时间表）：`AddressSpace`、IPC filter pool、Scheduler entity 可复用同一模式（判定如何影响还需要独立设计，不默认套用）。
- **设计文档**：本文件作为"为什么这样做 + 完整设计来源"的权威记录，独立持久化。
- **设计来源备份**：GPT 两轮意见的完整原文位于 `AI-chats/comments.md`（该文件可能因后续 GPT 对话而被清理；本文件已沉淀核心论点，不依赖外部来源）。