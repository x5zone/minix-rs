# Minix-RS 项目完整规划

> **目标**: 完整复刻 Minix3 到 Rust  
> **策略**: 纵向切片（按功能而非模块）  
> **性质**: OS 研究项目（架构优先，测试驱动）  
> **构建系统**: xtask (Rust)  
> **创建日期**: 2026-04-06  
> **状态**: 规划中

---

## 一、项目概述

### 1.1 核心目标

基于 Rust 语言完整复刻 Minix3 操作系统，**作为研究项目**深入理解微内核设计：
- 面向 64 位现代硬件
- 利用类型系统消除 C 语言中的隐式假设
- 通过所有权模型保证内存安全
- **架构优先**：先理解设计，再考虑启动
- **测试驱动**：通过单元测试验证，硬件代码可 mock

### 1.2 设计原则

**语义冻结原则**: 重写期间保留所有 Minix3 行为，先建立行为等价性，再改进设计。

```
阶段 1: Rewrite（语义等价）→ 当前阶段
阶段 2: Redesign（语义改进）→ 未来阶段
```

### 1.3 技术选型

| 组件 | 选择 | 理由 |
|------|------|------|
| 构建系统 | **xtask** | Rust 原生，类型安全，可调试，跨平台 |
| 架构 | x86_64 | 现代硬件，文档丰富 |
| 目标 | 64位 Minix | 自定义 target |
| 验证方式 | **单元测试** | 架构研究优先，不着急启动 |
| 硬件抽象 | **Mock** | 缺失的硬件代码用 mock 实现 |

### 1.4 项目性质

**这不是一个优先"让系统跑起来"的项目，而是一个"理解微内核设计"的研究项目。**

- ✅ 关注：模块边界、IPC 设计、类型安全
- ✅ 验证：单元测试、集成测试
- ❌ 不着急：QEMU 启动、磁盘镜像、驱动实现
- ❌ 不着急：看到 "Hello World" 输出

---

## 二、纵向切片重写策略

### 2.1 为什么不按模块重写

按模块重写（先写完整 PM，再写完整 VFS）会失败：

1. **概念爆炸**：每个模块包含太多概念（进程语义、信号、fork/exec/exit/wait、与内核/VM/VFS 的交互）
2. **无限上下文追踪**：一行代码背后有一大串上下文，上下文又牵出其他模块
3. **无法闭环**：在所有模块完成之前，永远得不到一个"可运行的系统"

> **如果试图搞清楚每行代码的上下文，那意味着一定搞不清楚。**

### 2.2 纵向切片策略

**按功能（因果链）重写，而不是按模块：**

```
切片 1: fork 切片      ← 穿过 kernel/pm/vm 的最简 fork 路径
切片 2: exec 切片      ← 穿过 kernel/pm/vfs/vm 的最简 exec 路径
切片 3: exit 切片      ← 穿过 kernel/pm/vm 的最简 exit 路径
切片 4: signal 切片    ← 穿过 kernel/pm 的最简 signal 路径
切片 5: mmap 切片      ← 穿过 kernel/pm/vm 的最简 mmap 路径
...
```

**每个切片都是一条穿过所有模块的完整闭环路径。**

### 2.3 核心原则：切片闭环，模块不完整

> **每个切片必须闭环（可通过测试验证），但每个模块不必完整。**

实现 fork 切片时：

```rust
// pm/fork.rs - 只实现 fork 相关的入口
pub fn sys_fork() -> Result<Pid, Error> {
    // 最简 fork 逻辑
}

// kernel/process.rs - 只实现 fork 需要的支持
pub fn copy_process(proc: &Process) -> Process {
    // 最简进程复制
}

// vm/cow.rs - 只实现写时复制或最简地址空间复制
pub fn copy_address_space(as: &AddressSpace) -> AddressSpace {
    // 最简 VM 支持（甚至可以 mock）
}

// vfs/ - 暂不触碰，或留空接口
```

### 2.4 示例：exec 切片

```
用户 exec("hello")
       │
       ▼
┌─────────────────────────────────────────────────────────┐
│  kernel/                                                │
│    ├─ 最简调度器（mock）                                │
│    ├─ 最简地址空间（mock）                              │
│    └─ IPC（即使是假的也行）                             │
│                                                         │
│  pm/                                                    │
│    └─ 只实现 exec handler                               │
│       （不实现 signal、fork、wait）                     │
│                                                         │
│  vfs/                                                   │
│    ├─ open("/bin/hello") - mock 文件系统               │
│    └─ read ELF - 最简 ELF 解析                         │
│                                                         │
│  vm/                                                    │
│    └─ 映射 ELF 段 - 最简映射                           │
│                                                         │
│  ipc/                                                   │
│    └─ 消息传递 - 最简实现                              │
└─────────────────────────────────────────────────────────┘
```

**你实际要写的代码：**

```
pm/exec.rs      ← PM 的 exec 处理
vfs/elf.rs      ← VFS 的 ELF 加载
vm/map.rs       ← VM 的段映射
kernel/sched.rs ← 内核的最简调度器（mock）
```

### 2.5 这种方法的优势

1. **防止概念爆炸**：每个切片的范围有界，只需要理解一条因果链
2. **可验证的结果**：每个切片完成后，都有可工作的东西（通过测试验证）
3. **强制依赖分析**：自然学会区分核心依赖、可延后依赖、历史包袱

---

## 三、项目结构

### 3.1 整体架构（完整复刻目标）

**纵向切片是切入方式，不是最终目标。若干切片完成后，完整复刻也就近在眼前。**

```
minix-rs/
├── minix3/                    # 原始 C 代码（完整参考，不修改）
│
├── os/                        # Rust 实现（核心）- 完整复刻目标
│   ├── Cargo.toml            # 根 workspace
│   ├── xtask/                # 构建系统（Rust）
│   │   └── src/
│   │       └── main.rs       # xtask 入口
│   │
│   ├── kernel/               # minix-kernel（微内核）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs        # 库入口
│   │       ├── main.rs       # 内核入口（最终启动用）
│   │       ├── arch/         # 架构抽象（x86_64）
│   │       ├── boot/         # 启动代码
│   │       ├── clock/        # 时钟管理
│   │       ├── debug/        # 调试支持
│   │       ├── hal/          # 硬件抽象层
│   │       ├── include/      # 内核头文件
│   │       ├── ipc/          # IPC机制
│   │       ├── proc/         # 进程管理
│   │       ├── sched/        # 调度器
│   │       ├── system/       # 系统调用处理
│   │       └── vm/           # 虚拟内存（内核部分）
│   │
│   ├── servers/              # 系统服务器（9个核心）
│   │   ├── pm/              # minix-pm（进程管理）
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── main.rs
│   │   │       ├── fork.rs   # fork 切片
│   │   │       ├── exec.rs   # exec 切片
│   │   │       ├── exit.rs   # exit 切片
│   │   │       ├── signal.rs # signal 切片
│   │   │       ├── wait.rs   # wait 切片
│   │   │       └── ...       # 其他功能
│   │   ├── vfs/             # minix-vfs（文件系统）
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── main.rs
│   │   │       ├── elf.rs    # exec 切片
│   │   │       ├── open.rs   # open 切片
│   │   │       ├── read.rs   # read 切片
│   │   │       ├── write.rs  # write 切片
│   │   │       └── ...       # 其他功能
│   │   ├── vm/              # minix-vm（虚拟内存）
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── main.rs
│   │   │       ├── map.rs    # exec/mmap 切片
│   │   │       ├── cow.rs    # fork 切片
│   │   │       ├── unmap.rs  # exit 切片
│   │   │       └── ...       # 其他功能
│   │   ├── rs/              # minix-rs-server（重启动服务器）
│   │   ├── ds/              # minix-ds（数据存储）
│   │   ├── ipc/             # minix-ipc-server（IPC服务）
│   │   ├── sched/           # minix-sched（调度策略）
│   │   ├── is/              # minix-is（信息服务器）
│   │   ├── devman/          # minix-devman（设备管理）
│   │   ├── input/           # minix-input（输入服务器）
│   │   └── mib/             # minix-mib（管理信息库）
│   │
│   ├── drivers/              # 设备驱动（用户态）
│   │   ├── block/           # 块设备驱动
│   │   ├── char/            # 字符设备驱动
│   │   └── net/             # 网络驱动
│   │
│   ├── fs/                  # 文件系统实现
│   │   ├── mfs/             # Minix File System
│   │   └── ext2/            # ext2 支持
│   │
│   ├── net/                 # 网络协议栈
│   │   └── lwip/            # Lightweight IP
│   │
│   ├── libs/                 # 共享库
│   │   ├── minix-ipc/       # IPC协议（核心）
│   │   ├── minix-sys/       # 系统调用封装
│   │   ├── minix-rt/        # 用户态运行时
│   │   └── minix-mock/      # 硬件 mock 库（开发期用）
│   │
│   ├── commands/             # 用户态命令
│   │   ├── bin/             # 基础命令
│   │   │   ├── cat/         # minix-cat
│   │   │   ├── cp/          # minix-cp
│   │   │   ├── echo/        # minix-echo
│   │   │   ├── ls/          # minix-ls
│   │   │   ├── mv/          # minix-mv
│   │   │   ├── rm/          # minix-rm
│   │   │   └── sh/          # minix-sh（shell）
│   │   ├── sbin/            # 系统命令
│   │   │   ├── init/        # minix-init
│   │   │   ├── fsck/        # minix-fsck
│   │   │   ├── mkfs/        # minix-mkfs
│   │   │   └── reboot/      # minix-reboot
│   │   └── games/           # 游戏
│   │       ├── tetris/      # minix-tetris
│   │       ├── snake/       # minix-snake
│   │       └── rogue/       # minix-rogue
│   │
│   └── tests/                # 集成测试
│       ├── fork_test.rs      # fork 切片测试
│       ├── exec_test.rs      # exec 切片测试
│       └── ...
│
├── docs/                      # 文档
└── notes/                     # 笔记（已存在）
    ├── study/
    ├── rewrite/
    └── redesign/
```

### 3.2 纵向切片实施路径

**切片是切入方式，逐步填满完整结构：**

```
阶段 1: fork 切片
         ├─ kernel/src/proc/fork.rs
         ├─ servers/pm/src/fork.rs
         └─ servers/vm/src/cow.rs

阶段 2: exec 切片
         ├─ kernel/src/proc/exec.rs
         ├─ servers/pm/src/exec.rs
         ├─ servers/vfs/src/elf.rs
         └─ servers/vm/src/map.rs

阶段 3: exit 切片
         ├─ kernel/src/proc/exit.rs
         ├─ servers/pm/src/exit.rs
         ├─ servers/pm/src/wait.rs
         └─ servers/vm/src/unmap.rs

...（更多切片）

阶段 N: 完整复刻
         └─ 所有文件都已实现，完整目录结构填满
```

### 3.3 Crate 命名规范

| 目录 | Crate 名称 | 说明 |
|------|-----------|------|
| `os/kernel/` | `minix-kernel` | 微内核（no_std，大量 mock） |
| `os/servers/pm/` | `minix-pm` | 进程管理器 |
| `os/servers/vfs/` | `minix-vfs` | 虚拟文件系统 |
| `os/servers/vm/` | `minix-vm` | 虚拟内存服务器 |
| `os/libs/minix-ipc/` | `minix-ipc` | IPC协议库 |
| `os/libs/minix-mock/` | `minix-mock` | 硬件 mock 库 |
| `os/xtask/` | `xtask` | 构建系统 |

### 3.4 Mock 策略（开发期使用）

**缺失的硬件代码用 mock 实现：**

```rust
// libs/minix-mock/src/lib.rs

/// Mock 内存管理单元
pub struct MockMMU;

impl MockMMU {
    pub fn new() -> Self { Self }
    
    /// Mock 页表分配
    pub fn alloc_page_table(&self) -> PageTable {
        // 返回一个 mock 页表，不真正操作硬件
        PageTable::mock()
    }
    
    /// Mock 地址映射
    pub fn map(&self, vaddr: VirtAddr, paddr: PhysAddr, flags: Flags) {
        // 记录映射关系，不真正写入硬件
        log::debug!("mock map: {:?} -> {:?}", vaddr, paddr);
    }
}

/// Mock 中断控制器
pub struct MockPIC;

impl MockPIC {
    pub fn enable_irq(&self, irq: u8) {
        log::debug!("mock enable_irq: {}", irq);
    }
}
```

---

## 四、xtask 构建系统

### 4.1 功能设计

```rust
// os/xtask/src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Minix-RS build system")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 编译项目
    Build {
        #[arg(short, long)]
        release: bool,
    },
    /// 运行测试（核心功能）
    Test {
        /// 指定切片测试（fork/exec/exit/...）
        #[arg(short, long)]
        slice: Option<String>,
    },
    /// 检查代码
    Check,
    /// 生成文档
    Doc,
    /// 运行特定切片演示
    Demo {
        slice: String,
    },
}

fn main() {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Build { release } => build(release),
        Commands::Test { slice } => run_tests(slice),
        Commands::Check => check(),
        Commands::Doc => doc(),
        Commands::Demo { slice } => run_demo(slice),
    }
}
```

### 4.2 使用方式

```bash
# 进入 os 目录
cd minix-rs/os

# 编译
cargo run -p xtask build

# 运行所有测试
cargo run -p xtask test

# 运行特定切片测试
cargo run -p xtask test --slice fork
cargo run -p xtask test --slice exec

# 运行切片演示
cargo run -p xtask demo fork
cargo run -p xtask demo exec

# 检查代码
cargo run -p xtask check

# 生成文档
cargo run -p xtask doc
```

---

## 五、实施路线图（纵向切片）

### 切片 1: fork 切片（第 1-2 周）

**目标**: 实现穿过 kernel/pm/vm 的最简 fork 路径

**涉及模块**:
```
kernel/src/proc.rs   - 最简进程创建
pm/src/fork.rs      - fork 系统调用处理
vm/src/cow.rs       - 最简地址空间复制（或 mock）
```

**测试验证**:
```rust
#[test]
fn test_fork_basic() {
    let parent = Process::new(1);
    let child = sys_fork(&parent).unwrap();
    
    assert_eq!(child.parent, parent.pid);
    assert_eq!(child.state, ProcState::Runnable);
    // 地址空间已复制（或标记为 COW）
}
```

**交付物**:
- `cargo run -p xtask test --slice fork` 通过

### 切片 2: exec 切片（第 3-4 周）

**目标**: 实现穿过 kernel/pm/vfs/vm 的最简 exec 路径

**涉及模块**:
```
kernel/src/proc.rs   - 进程状态切换
pm/src/exec.rs      - exec 系统调用处理
vfs/src/elf.rs      - 最简 ELF 解析
vm/src/map.rs       - 最简段映射
```

**测试验证**:
```rust
#[test]
fn test_exec_basic() {
    let mut proc = Process::new(1);
    let elf_data = include_bytes!("test/hello.elf");
    
    sys_exec(&mut proc, "/bin/hello", elf_data).unwrap();
    
    assert_eq!(proc.name, "hello");
    // 地址空间已加载 ELF
}
```

**交付物**:
- `cargo run -p xtask test --slice exec` 通过

### 切片 3: exit 切片（第 5-6 周）

**目标**: 实现穿过 kernel/pm/vm 的最简 exit 路径

**涉及模块**:
```
kernel/src/proc.rs   - 进程终止
pm/src/exit.rs      - exit 系统调用处理
pm/src/wait.rs      - wait 系统调用处理
vm/src/unmap.rs     - 地址空间释放
```

**测试验证**:
```rust
#[test]
fn test_exit_basic() {
    let parent = Process::new(1);
    let child = sys_fork(&parent).unwrap();
    
    sys_exit(&child, 0).unwrap();
    
    assert_eq!(child.state, ProcState::Zombie);
    // 父进程可以 wait
    let status = sys_wait(&parent, child.pid).unwrap();
    assert_eq!(status, 0);
}
```

**交付物**:
- `cargo run -p xtask test --slice exit` 通过

### 切片 4: signal 切片（第 7-8 周）

**目标**: 实现穿过 kernel/pm 的最简 signal 路径

**涉及模块**:
```
kernel/src/signal.rs - 信号传递机制
pm/src/signal.rs     - signal 系统调用处理
pm/src/sigaction.rs - 信号处理注册
```

**测试验证**:
```rust
#[test]
fn test_signal_basic() {
    let mut proc = Process::new(1);
    
    sys_sigaction(&mut proc, SIGTERM, handler).unwrap();
    sys_kill(&mut proc, SIGTERM).unwrap();
    
    assert!(proc.pending_signals.contains(SIGTERM));
}
```

**交付物**:
- `cargo run -p xtask test --slice signal` 通过

### 切片 5: mmap 切片（第 9-10 周）

**目标**: 实现穿过 kernel/pm/vm 的最简 mmap 路径

**涉及模块**:
```
kernel/src/vm.rs     - 内存管理接口
pm/src/mmap.rs      - mmap 系统调用处理
vm/src/map.rs       - 地址空间映射
```

**测试验证**:
```rust
#[test]
fn test_mmap_basic() {
    let mut proc = Process::new(1);
    
    let addr = sys_mmap(&mut proc, 0, 4096, PROT_READ|PROT_WRITE, MAP_ANON).unwrap();
    
    assert!(addr.is_aligned(4096));
    // 可以读写映射的内存
}
```

**交付物**:
- `cargo run -p xtask test --slice mmap` 通过

### 后续切片

- **切片 6**: IPC 切片（消息传递）
- **切片 7**: VFS 切片（文件操作）
- **切片 8**: 调度切片
- **切片 9**: 权限切片
- ...

---

## 六、关键设计决策

### 6.1 语义冻结

重写期间保留所有 Minix3 行为：
- 标记设计缺陷，暂不修复
- 先建立行为等价性，再改进设计

```rust
/// DESIGN NOTE:
/// 此字段承担双重职责：
/// 1. Unix 父子关系（用于 wait()/SIGCHLD）
/// 2. 系统进程的启动初始化链接
///
/// 这两种语义并不等价。
/// 详见 notes/redesign/process-model.md 中的分离方案。
parent: Pid,

/// HACK (继承自 Minix3):
/// 此调用作为 PM 和 VFS 之间的同步屏障。
/// 它与 setuid 在语义上无关。
fn setuid_barrier() { ... }
```

### 6.2 类型安全

利用 Rust 类型系统消除 C 语言隐式假设：

```rust
// C: 隐式状态约定
if (p->p_rts_flags == 0) { /* 可运行 */ }

// Rust: 类型状态模式
enum ProcState {
    Runnable,
    Sending { target: ProcId },
    Receiving { from: ProcId },
    // ...
}
```

### 6.3 分层架构

```
┌─────────────────────────────────────────┐
│           Tests（验证切片）              │
├─────────────────────────────────────────┤
│           Servers（按切片实现）          │
│     pm/fork.rs, pm/exec.rs, ...         │
├─────────────────────────────────────────┤
│           Kernel（按切片实现）           │
│     proc/fork.rs, proc/exec.rs, ...     │
├─────────────────────────────────────────┤
│           Mock（硬件抽象）               │
│     MockMMU, MockPIC, MockTimer         │
└─────────────────────────────────────────┘
```

---

## 七、参考资料

- [Rust for Linux](https://rust-for-linux.com/)
- [Writing an OS in Rust](https://os.phil-opp.com/)
- [seL4 Formal Verification](https://sel4.systems/)
- `notes/rewrite/rewrite.md` - 重构设计思路
- `notes/rewrite/rewrite-strategy.md` - 重写策略（语义冻结）
- `notes/rewrite/vertical-slice-strategy.md` - 纵向切片策略

---

## 八、总结

**当前状态**: 规划完成，准备开始实施

**核心策略**:
1. **纵向切片**：按功能（fork/exec/exit）而非模块重写，逐步填满完整目录结构
2. **完整复刻**：若干切片完成后，最终目标是完整复刻 Minix3
3. **架构优先**：先理解设计，再考虑启动
4. **测试驱动**：通过单元测试验证，硬件代码可 mock（开发期）
5. **研究性质**：OS 研究项目，不是启动项目

**实施路径**:
```
阶段 1: fork 切片（2周）
         └─ 实现 kernel/pm/vm 的 fork 相关代码

阶段 2: exec 切片（2周）
         └─ 实现 kernel/pm/vfs/vm 的 exec 相关代码

阶段 3: exit 切片（2周）
         └─ 实现 kernel/pm/vm 的 exit/wait 相关代码

...（更多切片）

阶段 N: 完整复刻
         └─ 所有目录填满，所有功能实现
         └─ 可以启动运行
```

**下一步行动**:
1. 创建完整目录结构（按 3.1 节）
2. 初始化 Cargo workspace
3. 实现 xtask 构建系统
4. 编写 minix-mock 库
5. 开始 **fork 切片**

**关键成功因素**:
- 严格遵守纵向切片策略
- 每个切片必须闭环（可通过测试）
- 模块不必完整，但边界必须清晰
- 保持与 Minix3 的行为等价性
- **最终目标是完整复刻，不是只做切片**

---

**准备开始实施？** 🚀
