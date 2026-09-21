# Minix-RS 项目结构文档

> **版本**: 1.0  
> **日期**: 2026-04-06  
> **状态**: 已确定，可开始实施

---

## 1. 总体架构

```
minix-rs/                          # 仓库根目录
├── minix3/                        # 原始 Minix3 C 代码（参考/对比）
│   └── minix/
│       ├── kernel/
│       ├── servers/
│       └── commands/
│
├── os/                            # Rust 重写的 Minix3 系统（核心）
│   ├── Cargo.toml                 # 根 Workspace（使用 glob）
│   ├── Makefile                   # 控制不同 target 的构建
│   ├── README.md
│   │
│   ├── .cargo/
│   │   └── config.toml            # 不写死 target，由 Makefile 控制
│   │
│   ├── kernel/                    # minix-kernel（no_std）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── ...
│   │
│   ├── servers/                   # 系统服务器
│   │   ├── pm/                    # minix-pm
│   │   ├── vfs/                   # minix-vfs
│   │   ├── vm/                    # minix-vm
│   │   └── rs/                    # minix-rs-server
│   │
│   ├── drivers/                   # 设备驱动
│   │
│   ├── libs/                      # 共享库
│   │   ├── minix-ipc/            # IPC 协议（核心）
│   │   ├── minix-sys/            # 系统调用封装
│   │   └── minix-rt/             # 用户态运行时
│   │
│   ├── userspace/                 # 用户态程序
│   │   ├── shell/                 # minix-shell
│   │   ├── bin/                   # 基础命令
│   │   │   ├── ls/
│   │   │   ├── cat/
│   │   │   └── ...
│   │   └── games/                 # 小游戏
│   │       ├── tetris/
│   │       └── snake/
│   │
│   └── tools/                     # 开发工具
│       ├── mkfs/                  # minix-mkfs
│       ├── boot/
│       └── qemu-runner/
│
├── docs/                          # 文档
└── notes/                         # 笔记（已存在）
    ├── study/
    ├── rewrite/
    └── redesign/
```

---

## 2. 关键设计决策

### 2.1 Workspace 结构

**文件**: `os/Cargo.toml`

```toml
[workspace]
members = [
    "kernel",
    "servers/*",
    "libs/*",
    "userspace/shell",
    "userspace/bin/*",
    "userspace/games/*",
    "tools/mkfs",
]
resolver = "2"
```

**设计要点**:
- 使用 glob 模式简化配置
- 不写 `[profile]` 和 `[build]`，由 Makefile 控制
- 单一 workspace，避免嵌套

### 2.2 Target 控制

**文件**: `os/.cargo/config.toml`

```toml
# 空的或只放通用配置
# 不写死 target，由 Makefile 控制
```

**文件**: `os/Makefile`

```makefile
KERNEL_TARGET := x86_64-minix.json
USER_TARGET := x86_64-unknown-linux-musl

.PHONY: all kernel servers userspace tools image run clean

all: kernel servers userspace tools image

kernel:
	@cargo build -p minix-kernel --target $(KERNEL_TARGET)

servers:
	@for server in pm vfs vm rs; do \
		cargo build -p minix-$$server --target $(KERNEL_TARGET); \
	done

userspace:
	@cargo build -p minix-shell --target $(USER_TARGET)
	@cargo build -p minix-ls --target $(USER_TARGET)
	# ...

tools:
	@cargo build -p minix-mkfs

image: kernel servers userspace
	@./tools/mkfs/target/debug/minix-mkfs \
		--kernel kernel/target/$(KERNEL_TARGET)/debug/minix-kernel \
		--servers servers/*/target/$(KERNEL_TARGET)/debug/* \
		--userspace userspace/*/target/$(USER_TARGET)/debug/* \
		--output minix-rs.img

run: image
	@./tools/qemu-runner/run.sh minix-rs.img

clean:
	@cargo clean
	@rm -f minix-rs.img
```

### 2.3 Crate 命名规范

| 目录 | Crate 名称 | 说明 |
|------|-----------|------|
| `os/kernel/` | `minix-kernel` | 内核（no_std） |
| `os/servers/pm/` | `minix-pm` | 进程管理器 |
| `os/servers/vfs/` | `minix-vfs` | 虚拟文件系统 |
| `os/servers/vm/` | `minix-vm` | 虚拟内存服务器 |
| `os/servers/rs/` | `minix-rs-server` | 重启动服务器 |
| `os/libs/minix-ipc/` | `minix-ipc` | IPC 协议核心 |
| `os/libs/minix-sys/` | `minix-sys` | 系统调用封装 |
| `os/libs/minix-rt/` | `minix-rt` | 用户态运行时 |
| `os/userspace/shell/` | `minix-shell` | Shell |
| `os/userspace/bin/ls/` | `minix-ls` | ls 命令 |
| `os/userspace/bin/cat/` | `minix-cat` | cat 命令 |
| `os/userspace/games/tetris/` | `minix-tetris` | 俄罗斯方块 |
| `os/tools/mkfs/` | `minix-mkfs` | 镜像生成工具 |

### 2.4 分层架构

```
┌─────────────────────────────────────────┐
│           Userspace (std)               │
│  shell, bin/ls, bin/cat, games/tetris   │
├─────────────────────────────────────────┤
│           Servers (no_std/特殊ABI)      │
│     pm, vfs, vm, rs                     │
├─────────────────────────────────────────┤
│           Kernel (no_std)               │
│     ipc, proc, sched, vm, hal           │
├─────────────────────────────────────────┤
│           Hardware                      │
└─────────────────────────────────────────┘
```

**设计原则**:
- 内核: 机制（mechanism）
- 服务器: 策略（policy）
- 用户态: 应用（application）

---

## 3. 关键问题与解决方案

### 3.1 问题 1: 单一 Workspace 的编译模型冲突

**问题**: Kernel（no_std）、Servers（特殊 ABI）、Userspace（std）混在同一个 workspace 中

**解决方案**:
- 不写死 target 在 `.cargo/config.toml`
- 使用 Makefile 控制不同 crate 使用不同 target
- 例如: `cargo build -p minix-kernel --target x86_64-minix.json`

### 3.2 问题 2: Workspace 膨胀

**问题**: Userspace 命令和游戏会越来越多

**解决方案**:
- 使用 glob 模式: `"userspace/bin/*"`
- 未来扩展时自动包含，无需修改 `Cargo.toml`

### 3.3 问题 3: 命名混淆

**问题**: 原命名 `minix-libc` 容易与标准 C 库混淆

**解决方案**:
- 改名为 `minix-rt`（runtime）
- 明确表示这是用户态运行时，不是 C 标准库

---

## 4. 未来扩展

### 4.1 接口层（预留）

未来添加 `libs/interfaces/` 目录，定义服务器间接口:

```
libs/
  minix-ipc/          # IPC 协议（Message, Endpoint）
  minix-sys/          # 系统调用封装
  minix-rt/           # 用户态运行时
  interfaces/         # 服务器间接口（未来添加）
    ├── process.rs    # PM 接口定义
    ├── vfs.rs        # VFS 接口定义
    └── vm.rs         # VM 接口定义
```

**目的**: 避免服务器间直接依赖，防止循环依赖

### 4.2 多架构支持

未来在 `kernel/src/arch/` 下添加:
- `x86_64/` - x86_64 架构
- `aarch64/` - ARM64 架构（未来）

---

## 5. 实施步骤

### 阶段 1: 基础设施（1-2 周）
1. 创建目录结构
2. 初始化 Cargo workspace
3. 配置 Makefile
4. 设置 QEMU 运行环境

### 阶段 2: 核心库（2-3 周）
1. 设计 `minix-ipc` crate（最关键）
   - Message 结构
   - Endpoint 类型
   - Send/Receive/Notify 抽象
2. 实现 `minix-sys` crate
3. 实现 `minix-rt` crate

### 阶段 3: 内核（4-6 周）
1. 基础类型定义（Pid, Endpoint, PhysAddr, VirtAddr）
2. 进程管理（Process, ProcTable, 状态机）
3. IPC 实现（mini_send, mini_receive, mini_notify）
4. 调度器（运行队列，上下文切换）

### 阶段 4: 服务器（4-6 周）
1. PM（进程管理器）
2. VM（虚拟内存）
3. VFS（虚拟文件系统）
4. RS（重启动服务器）

### 阶段 5: 用户态（2-3 周）
1. Shell
2. 基础命令（ls, cat, echo, ...）
3. 小游戏（tetris, snake, ...）

---

## 6. 设计原则

### 6.1 语义冻结
- 重写期间保留所有 Minix3 行为
- 标记设计缺陷，暂不修复
- 先建立行为等价性，再改进设计

### 6.2 类型安全
- 利用 Rust 类型系统消除 C 语言隐式假设
- 使用类型状态模式替代标志位
- 显式错误处理（Result）

### 6.3 分层清晰
- kernel: 机制
- servers: 策略
- userspace: 应用

---

## 7. 参考资料

- [Rust for Linux](https://rust-for-linux.com/)
- [Writing an OS in Rust](https://os.phil-opp.com/)
- [seL4 Formal Verification](https://sel4.systems/)
- `notes/rewrite/rewrite.md` - 重构设计思路
- `notes/rewrite/rewrite-strategy.md` - 重写策略（语义冻结）

---

## 8. 总结

**当前状态**: 结构设计完成，达到"可以开干"的工程级别

**核心优势**:
1. 分层清晰（kernel / servers / userspace）
2. 库抽取合理（minix-ipc, minix-sys, minix-rt）
3. 工具独立（mkfs, qemu-runner）
4. 可扩展性强（glob 模式）

**下一步**: 设计 `minix-ipc` crate（消息传递模型）

---

**注意**: 此结构已确定，不要再反复重构目录。接下来应该:
1. 先实现 `minix-ipc`
2. 再让 PM 跑起来
3. 验证 fork/exec
4. 回头优化架构
