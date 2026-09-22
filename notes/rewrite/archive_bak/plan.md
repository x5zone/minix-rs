# Minix3 mproc 重写计划

> **目标**: 严格依据 Minix3 源码，验证设计方案，生成 Rust 重写代码  
> **约束**: no_std 环境，仅支持 64 位系统，兼容 Minix3 微内核架构

---

## 任务概览

| # | 任务 | 预计工作量 | 优先级 |
|---|------|-----------|--------|
| 1 | 源码验证：检查 fork-rewrite-01.md 正确性 | 高 | P0 |
| 2 | 文档生成：mp_flags 状态转换详细说明 | 中 | P0 |
| 3 | 代码实现：mproc.h Rust 重写 | 高 | P1 |
| 4 | 文档编写：代码 README 说明 | 中 | P2 |

---

## 任务一：源码验证

### 1.1 验证目标

严格检查 `fork-rewrite-01.md` 中方案二的设计是否正确，确保：

1. **生命周期状态转换**是否符合 Minix3 源码
2. **阻塞状态组合**是否覆盖所有合法组合
3. **父进程等待状态**语义是否正确
4. **监护关系**模型是否完整
5. **权限模型**映射是否准确

### 1.2 验证方法

逐个检查 Minix3 源码中的关键文件：

| 文件 | 验证内容 |
|------|---------|
| `minix/servers/pm/mproc.h` | 字段定义、flag 定义 |
| `minix/servers/pm/forkexit.c` | fork/exit 状态转换 |
| `minix/servers/pm/signal.c` | 信号处理、PROC_STOPPED |
| `minix/servers/pm/main.c` | VFS 回复处理 |
| `minix/servers/pm/event.c` | EVENT_CALL 处理 |
| `minix/servers/pm/trace.c` | 追踪相关状态 |
| `minix/servers/pm/utility.c` | VFS_CALL 设置 |

### 1.3 验证清单

- [ ] `Lifecycle` enum 是否覆盖所有生命周期状态
- [ ] `BlockState` 是否正确表达 `PROC_STOPPED` + `VFS_CALL` 组合
- [ ] `WaitState` 是否正确放在父进程
- [ ] `Guardianship` 是否正确处理 tracer != parent 情况
- [ ] `Privilege` 是否正确映射 `PRIV_PROC`
- [ ] `SignalState` 是否包含所有信号相关字段

---

## 任务二：状态转换文档

### 2.1 文档目标

生成 `mp-flags-analysis.md`，详细记录：

1. **所有 mp_flags 的定义和用途**
2. **状态组合约束**（哪些可以组合，哪些互斥）
3. **状态转换路径**（从哪个状态到哪个状态）
4. **源码引用**（每个转换对应的代码位置）

### 2.2 文档结构

```markdown
# mp_flags 状态分析

## 一、Flag 定义
- 每个 flag 的值、名称、用途

## 二、Flag 分类
- 生命周期 flags
- 阻塞状态 flags
- 权限 flags
- 追踪 flags
- 其他 flags

## 三、状态组合约束
- 互斥组合
- 必须组合
- 可选组合

## 四、状态转换路径
- fork 路径
- exit 路径
- wait 路径
- trace 路径

## 五、源码引用
- 每个转换的代码位置
```

### 2.3 输出位置

`notes/rewrite/fork-syscall-rewrite/mp-flags-analysis.md`

---

## 任务三：代码实现

### 3.1 设计方案

结合方案二和方案三：

1. **保留方案二的 enum 设计**（类型安全）
2. **采用方案三的分层结构**（字段聚合）

### 3.2 代码结构

```
os/libs/minix-types/
├── Cargo.toml
└── src/
    ├── lib.rs              # 模块导出
    ├── process/
    │   ├── mod.rs          # Process 模块
    │   ├── lifecycle.rs    # Lifecycle enum
    │   ├── block.rs        # BlockState struct
    │   ├── wait.rs         # WaitState struct
    │   ├── guardianship.rs # Guardianship enum
    │   ├── privilege.rs    # Privilege enum
    │   ├── signal.rs       # SignalState struct
    │   └── process.rs      # Process struct
    ├── types/
    │   ├── mod.rs          # 基础类型模块
    │   ├── pid.rs          # Pid, Endpoint 等类型
    │   ├── id.rs           # Uid, Gid, IdSet
    │   └── clock.rs        # Clock, Timer
    └── ipc/
        ├── mod.rs          # IPC 模块
        └── message.rs      # Message 类型
```

### 3.3 实现步骤

1. **创建 `minix-types` crate**
   - 在 `os/libs/` 下创建新目录
   - 配置 `Cargo.toml`（no_std 支持）

2. **实现基础类型**
   - `Pid`, `Endpoint`, `ProcIndex`
   - `Uid`, `Gid`, `IdSet<T>`
   - `Clock`, `VirBytes`

3. **实现生命周期模块**
   - `Lifecycle` enum
   - 状态转换方法

4. **实现阻塞状态模块**
   - `BlockState` struct
   - `IpcBlockReason` enum

5. **实现其他模块**
   - `WaitState`, `Guardianship`, `Privilege`, `SignalState`

6. **实现 Process struct**
   - 聚合所有模块
   - 提供高层 API

---

## 任务四：README 编写

### 4.1 README 目标

为生成的 Rust 代码提供详尽说明：

1. **设计理念**（为什么这样设计）
2. **与 C 代码的对应关系**
3. **使用示例**
4. **测试方法**

### 4.2 README 结构

```markdown
# minix-types

## 一、概述
- 模块用途
- 设计理念

## 二、类型映射表
- C → Rust 类型对应

## 三、核心类型
- Process
- Lifecycle
- BlockState
- ...

## 四、使用示例
- 创建进程
- 状态转换
- ...

## 五、测试
- 单元测试
- 集成测试
```

### 4.3 输出位置

`os/libs/minix-types/README.md`

---

## 执行顺序

```
任务一（源码验证）
    ↓
任务二（状态文档）──→ 发现问题 ──→ 修正 fork-rewrite-01.md
    ↓
任务三（代码实现）
    ↓
任务四（README）
```

---

## 验收标准

### 任务一验收

- [ ] 所有 flag 映射正确
- [ ] 所有状态组合覆盖
- [ ] 所有源码引用准确

### 任务二验收

- [ ] 文档完整覆盖所有 flags
- [ ] 状态转换图清晰
- [ ] 源码引用完整

### 任务三验收

- [ ] 代码编译通过
- [ ] 类型安全（无 unsafe）
- [ ] no_std 兼容

### 任务四验收

- [ ] README 完整
- [ ] 示例代码可运行
- [ ] 测试覆盖核心逻辑

---

## 风险与应对

| 风险 | 影响 | 应对措施 |
|------|------|---------|
| 发现方案二设计错误 | 高 | 及时修正，重新验证 |
| 源码理解偏差 | 中 | 多文件交叉验证 |
| 类型布局问题 | 中 | 使用 `#[repr(C)]` |
| 内存开销过大 | 低 | 评估后优化 |

---

## 时间估算

| 任务 | 预计时间 |
|------|---------|
| 任务一 | 2-3 小时 |
| 任务二 | 1-2 小时 |
| 任务三 | 3-4 小时 |
| 任务四 | 1 小时 |
| **总计** | **7-10 小时** |

---

## 开始执行

确认计划后，将按照以下顺序执行：

1. ✅ 开始任务一：源码验证
2. ⏳ 开始任务二：状态文档
3. ⏳ 开始任务三：代码实现
4. ⏳ 开始任务四：README
