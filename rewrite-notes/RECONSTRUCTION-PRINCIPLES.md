# Rust 重构指导原则

> 本文档定义 Minix3 Rust 重构的核心方法论，适用于所有纵向切片。

---

## 重构的四个层次

| 层 | 名字 | 说明 | 当前位置 |
|---|---|---|---|
| L1 | **Translation**（翻译） | 直接 1:1 翻译 C 代码 | ❌ 不做 |
| L2 | **Semantic Preservation**（语义保持） | 外部语义不变，内部表达优化 | ✅ **当前阶段** |
| L3 | **Model Extraction**（模型提取） | 提取隐式状态机，显式化模型 | 🔜 **下一步** |
| L4 | **Redesign**（重新设计） | 改变系统架构/机制 | 🚀 未来 |

---

## 核心术语定义

| 术语 | 英文 | 定义 |
|---|---|---|
| **代码迁移** | Translate | 直接 1:1 翻译 C 代码到 Rust，保持代码结构、命名、逻辑完全一致。仅做语法转换，不做语义改变。 |
| **语义重建** | Rewrite | 保持外部可观察行为不变，内部使用 Rust 类型系统重新表达。将隐式编码显式化（如 flag → enum、int → newtype）。 |
| **系统重构** | Redesign | 改变系统架构、机制或协议。修改 IPC 协议、合并服务、改变调度策略等都属于此范畴。 |

> 💡 Translate 是"翻译"，Rewrite 是"重写"，Redesign 是"重设计"

---

## 关键原则

> 👉 **外部语义不变（observable behavior）**
> 👉 **内部表达可以改变（internal model）**

> ❗ **把"隐式编码"变成"显式协议"

---

## 判断标准

每次重构时问自己：
> 👉 "这个改动，是在改变系统行为，还是只是让模型更清晰？"

| 类型 | 允许吗 |
|---|---|
| flag → enum | ✅ 语义保持 |
| int → newtype | ✅ 语义保持 |
| struct 拆分 | ✅ 语义保持 |
| IPC 改协议 | ❌ 这是 redesign |
| PM 合入内核 | ❌ 这是 redesign |

---

## 应用示例

```c
// Minix C 代码
ZOMBIE + WAITING + TOLD_PARENT
```

可以变成：

```rust
// Rust
Lifecycle::Zombie { reaped: bool }
```

👉 ✔ 行为一致  
👉 ✔ 表达更好

---

## 硬件抽象原则

> 👉 **我们不描述硬件，我们只抽象机制**
> 👉 **OS需要硬件提供什么机制，抽象出trait，然后硬件实现这些trait**

### 核心规则

1. **当前硬件相关代码仅允许 Mock**
   - 暂不实现任何真实硬件代码
   - Mock 硬件必须实现对应的 trait

2. **禁止硬件特定命名**
   - ❌ 不允许：`InitialTss`、`load_cr3`、`invlpg` 等硬件特定函数名
   - ✅ 允许：`activate_page_table`、`flush_tlb` 等机制抽象名称

3. **trait 定义规则：分散定义，集中实现**
   - 各功能模块定义自己的 trait（如 `Paging`、`Interrupt`、`Timer`）
   - 所有 trait 在 `os/arch` crate 中统一实现
   - 当前 `arch` crate 只有 Mock 实现

4. **文档中的硬件讲解**
   - ✅ 允许：硬件机制讲解出现在 `.md` 文档中
   - ❌ 禁止：代码中直接操作硬件

### 架构支持规划

| 架构 | 状态 | 说明 |
|------|------|------|
| Mock | ✅ 当前 | 用户态测试，软件模拟 |
| x86-64 | 🔜 待实现 | Intel/AMD 64位架构 |
| ARM64 | 🔜 待实现 | ARM 64位架构 |
| RISC-V 64 | 🔜 待实现 | RISC-V 64位架构 |

### 代码示例

```rust
// ❌ 错误：直接操作硬件
fn load_cr3(addr: u64) {
    unsafe { asm!("mov cr3, {}", in(reg) addr); }
}

// ✅ 正确：抽象机制
trait Paging {
    fn switch(&self);  // 激活页表
    fn flush_tlb(&self);  // 刷新地址转换缓存
}

// Mock 实现
struct MockPaging { ... }
impl Paging for MockPaging { ... }

// 未来 x86-64 实现
// struct X86_64Paging { ... }
// impl Paging for X86_64Paging { ... }
```

### 目录结构

```
os/
├── arch/                    # 硬件抽象层 crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs           # 导出 trait 和当前实现
│       ├── paging.rs        # 分页机制 trait
│       ├── interrupt.rs     # 中断机制 trait（待添加）
│       └── timer.rs         # 定时器机制 trait（待添加）
│
├── servers/vm/              # VM 服务（使用 arch crate）
│   └── src/
│       └── vmproc/
│           └── vmproc.rs    # 使用 PageTableRef，不直接操作硬件
│
└── kernel/                  # 内核（使用 arch crate）
    └── ...
```
