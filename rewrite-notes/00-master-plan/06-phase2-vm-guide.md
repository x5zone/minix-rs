# Phase 2: VM 层实现指南

## 1. 阶段目标

实现 `vm_fork` 的真实版本，完成地址空间克隆与 Copy-on-Write 设置。

> **范围限定**: VM 功能逻辑（地址空间克隆、CoW）为真实实现，仅硬件访问（MMU、物理内存）使用 Mock。

## 2. 关键任务

### 任务 2.1: vm_fork 真实实现

**目标**: 实现 `vm_fork` 函数，完成地址空间克隆和 endpoint 生成

**接口定义**:
```rust
// os/servers/vm/src/fork.rs
pub fn vm_fork(
    parent_ep: Endpoint,
    child_slot: SlotIndex,
) -> Result<Endpoint, Error>;
```

**执行流程**:
1. 验证父进程 endpoint 有效
2. 生成新的 endpoint（基于 child_slot）
3. 克隆地址空间（真实实现，硬件使用 Mock）
4. 设置 Copy-on-Write 标志
5. 返回新的 endpoint

**错误处理**:
- `EINVAL`: 无效的父进程 endpoint
- `ENOMEM`: 内存不足

**参考文档**: [../02-stage-vm/draft/vm-fork-mock.md](../02-stage-vm/draft/vm-fork-mock.md)

---

### 任务 2.2: VM 进程表基础

**目标**: 建立 VM 进程表 `vmproc`，支持 fork 所需字段

**接口定义**:
```rust
// os/servers/vm/src/vmproc.rs
pub struct VmProc {
    pub endpoint: Endpoint,
    pub parent_slot: SlotIndex,
    pub flags: VmProcFlags,
    // 页表结构（真实实现，硬件操作使用 Mock）
    pub page_table: PageTable,
}

pub struct VmProcTable {
    slots: [Option<VmProc>; NR_PROCS],
}
```

**与 PM 的索引对齐**:
- VM 进程表索引与 PM 进程表索引保持一致
- `vmproc[i]` 对应 `mproc[i]`

---

### 任务 2.3: IPC 消息协议

**目标**: 定义 PM 与 VM 之间的 fork 消息格式

**PM → VM: VM_FORK 消息（Minix3 C 定义）**:
```c
// minix/include/minix/com.h
#define VM_FORK         (VM_RQ_BASE+1)
#  define VMF_ENDPOINT       m1_i1    /* 父进程 endpoint（输入） */
#  define VMF_SLOTNO         m1_i2    /* 子进程槽号（输入） */
#  define VMF_CHILD_ENDPOINT m1_i3    /* 子进程 endpoint（输出） */
```

**VM → Kernel: SYS_FORK 消息（Minix3 C 定义）**:
```c
// minix/include/minix/com.h
/* 输入字段 */
m_lsys_krn_sys_fork.endpt    /* 父进程 endpoint */
m_lsys_krn_sys_fork.slot     /* 子进程槽号 */
m_lsys_krn_sys_fork.flags    /* fork 标志 (PFF_VMINHIBIT=0x01) */

/* 输出字段 */
m_krn_lsys_sys_fork.endpt    /* 子进程的新 endpoint */
m_krn_lsys_sys_fork.msgaddr  /* 子进程的消息缓冲区虚拟地址 */

#define PFF_VMINHIBIT: u32 = 0x01  /* 抑制 VM 的 fork 处理 */
```

**Rust 消息定义**:
```rust
// os/libs/minix-types/src/messages/vm.rs
pub struct VmForkRequest {
    pub parent_endpoint: Endpoint,
    pub child_slot: SlotIndex,
}

pub struct VmForkResponse {
    pub child_endpoint: Endpoint,
    pub status: i32,
}

pub const VM_FORK: MessageType = MessageType(0x1001);

pub struct SysForkRequest {
    pub parent_endpoint: Endpoint,
    pub child_slot: i32,
    pub flags: u32,  // PFF_VMINHIBIT = 0x01
}

pub struct SysForkResponse {
    pub child_endpoint: Endpoint,
    pub msg_addr: VirBytes,
}
```

---

### 任务 2.4: Endpoint 生成

**目标**: 实现基于槽位的 endpoint 生成

**算法**:
```rust
fn generate_endpoint(slot: SlotIndex) -> Endpoint {
    // 真实实现：使用槽位号和 generation 字段生成 endpoint
    Endpoint::new(slot, generation)
}
```

## 3. 数据结构

### 3.1 核心类型

| 类型 | 定义 | 说明 |
|------|------|------|
| `Endpoint` | `#[repr(transparent)] pub struct Endpoint(u32);` | 进程端点 |
| `SlotIndex` | `#[repr(transparent)] pub struct SlotIndex(u16);` | 槽位索引 |
| `VmProcFlags` | `bitflags! { ... }` | VM 进程标志 |

### 3.2 标志位

```rust
pub struct VmProcFlags: u32 {
    const IN_USE = 0x00001;      // 槽位已使用
    const HAS_MEM = 0x00002;     // 拥有内存映射
    const COW = 0x00004;         // Copy-on-Write 设置
}
```

## 4. 文件组织

```
os/servers/vm/src/
├── main.rs           # 入口
├── lib.rs            # 库导出
├── vmproc.rs         # VmProc 结构体定义
├── table.rs          # VM 进程表管理
├── fork.rs           # vm_fork 实现
└── endpoint.rs       # Endpoint 生成
```

## 5. 验收标准

### 5.1 功能验收

- [ ] `vm_fork` 能正确克隆地址空间
- [ ] Copy-on-Write 标志正确设置
- [ ] VM 进程表与 PM 进程表索引对齐
- [ ] 错误处理路径正确（无效 endpoint 返回 EINVAL，内存不足返回 ENOMEM）
- [ ] IPC 消息格式定义完整

### 5.2 测试验收

- [ ] 单元测试：vm_fork 成功路径
- [ ] 单元测试：vm_fork 错误路径（无效 endpoint、内存不足）
- [ ] 单元测试：CoW 标志设置
- [ ] 集成测试：PM 调用 VM fork（端到端）

### 5.3 代码质量验收

- [ ] 所有函数有文档注释
- [ ] 通过 Clippy 检查
- [ ] 单元测试覆盖率 > 80%

## 6. 与 Minix3 对照

| Minix3 (C) | Rust 重构 | 说明 |
|-----------|----------|------|
| `vm_fork()` | `vm_fork()` | 接口保持一致 |
| `struct vmproc` | `VmProc` | 字段对应，类型安全化 |
| `endpoint` 生成算法 | `generate_endpoint()` | 算法保持一致 |
| 真实内存复制 | 真实内存复制 | 硬件操作使用 Mock |

## 7. 参考文档

- **C 源码分析**: [../02-stage-vm/draft/vm-fork-mock.md](../02-stage-vm/draft/vm-fork-mock.md)
- **架构分析**: [02-architecture-analysis.md](./02-architecture-analysis.md)
- **PM 层指南**: [06-phase1-pm-guide.md](./06-phase1-pm-guide.md)
