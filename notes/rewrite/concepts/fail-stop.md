# Fail-Stop 语义与 Panic 安全
* TODO 待全面 review
## 1. Fail-Stop 语义

Minix3 的核心服务（包括 VM）采用 **Fail-Stop** 语义：

```
Fail-Stop = 检测到错误 → 立即停止 → 不执行任何副作用
```

**核心原则**: 宁可让系统停止，也不要让系统处于不确定状态。

## 2. Minix3 panic 实现

**源码路径**: `minix3/minix/lib/libsys/panic.c`

```c
void panic(const char *fmt, ...)
{
    /* 某些可怕的事情发生了。panic 在检测到内部不一致时被触发，
     * 例如编程错误或定义的常量的非法值。
     */
    printf("%s(%d): panic: ", name, me);
    vprintf(fmt, args);        // 打印 panic 信息
    util_stacktrace();         // 打印堆栈跟踪
    panic_hook();              // 调试钩子
    
    _exit(1);                  // 直接退出！
    abort();                   // 备用方案
    for(;;) { }                // 最后手段：死循环
}
```

**关键特点**：
- **没有清理操作**！直接 `_exit(1)`
- **没有资源释放**！不调用任何 cleanup 函数
- **立即终止**，不执行任何后续代码

## 3. Rust panic 的风险

| 特性 | Minix3 panic | Rust panic |
|------|-------------|------------|
| **触发后行为** | 打印信息 → `_exit(1)` | 栈展开 → 调用 `Drop` → 终止 |
| **资源清理** | ❌ 不清理 | ✅ 自动 Drop |
| **副作用风险** | ✅ 无（直接退出） | ⚠️ Drop 可能出错 |

**问题**：如果对象处于不一致状态，Rust 的 `Drop` 可能：
- 释放错误的资源
- 写入损坏的数据
- 导致更严重的系统状态污染

**Minix3 的设计哲学**：一旦检测到内部不一致，系统状态可能已经损坏，**执行任何清理操作都可能加剧损坏**，所以直接 `_exit(1)`。

## 4. Minix3 核心服务约束

**VM 是核心服务（SF_CORE_SRV）**：

```c
// rs/const.h
#define SRV_SF   (SF_CORE_SRV)  // 系统服务
#define VM_SF    (SRVR_SF)      // VM 被标记为核心服务
```

**一旦 VM 崩溃，RS 会直接退出**：

```c
// rs/manager.c#L1120-1122
if ((rp->r_pub->sys_flags & SF_CORE_SRV) && !shutting_down) {
    printf("core system service died: %s\n", srv_to_string(rp));
    _exit(1);  // RS 直接退出，系统崩溃
}
```

**只有非核心服务才能被重启**：RS 的"自修复"能力只适用于驱动程序等非核心服务。

## 5. Rust 实现策略

### 5.1 避免污染系统状态

即使 VM 最终必须崩溃，也要**干净地死掉**：

```rust
/// 错误处理策略
pub enum VmError {
    /// 内存分配失败（可能是硬件问题）
    OutOfMemory,
    /// 页表操作失败
    PageTableError,
    /// 进程不存在或已退出
    ProcessNotFound,
    /// 权限不足
    PermissionDenied,
}

impl VmError {
    /// 判断是否应该 panic
    pub fn is_fatal(&self) -> bool {
        match self {
            // 致命错误：VM 状态已损坏
            VmError::OutOfMemory => true,
            // 非致命错误：可以继续运行
            VmError::ProcessNotFound => false,
            VmError::PermissionDenied => false,
            _ => false,
        }
    }
    
    /// 记录错误并决定行为
    pub fn handle(&self) {
        log::error!("VM error: {:?}", self);
        if self.is_fatal() {
            // 干净地死掉：不执行任何副作用
            unsafe { libc::_exit(1) };
        }
    }
}
```

### 5.2 panic 时避免污染系统状态

```rust
/// 安全地处理可能 panic 的操作
pub fn safe_operation<F, T>(op: F) -> Result<T, VmError>
where
    F: FnOnce() -> Result<T, VmError>,
{
    // 捕获任何 panic，防止破坏 VM 内部状态
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(op))
        .map_err(|_| VmError::InternalError)?
}

/// 危险的清理操作（只在确认安全时调用）
pub unsafe fn force_cleanup() {
    // 直接调用 _exit，不执行任何 Drop
    libc::_exit(1);
}
```

### 5.3 MaybeUninit 的 panic 安全价值

`MaybeUninit` 防止未初始化内存的非法 Drop：

```rust
// 普通数组会 panic
let arr: [VmProc; 256] = unsafe { uninitialized() };  // 未定义行为！

// MaybeUninit 明确告诉编译器：这是未初始化的
let arr: [MaybeUninit<VmProc>; 256] = unsafe { uninitialized() };
// panic 时不会尝试 Drop 不存在的对象
```

**价值总结**:

| 风险 | 没有 MaybeUninit | 有 MaybeUninit |
|------|------------------|---------------|
| panic 时 Drop | 释放不存在的物理页 | 安全：未初始化内存不 Drop |
| 破坏页表 | 可能 | 避免 |
| 发送错误 IPC | 可能 | 避免 |
| 调试信息 | 不可靠 | 可靠 |

## 6. 总结

- **Minix3 panic = 真正的 Fail-Stop**：立即退出，不执行任何清理
- **Rust panic ≠ Fail-Stop**：会执行 Drop，可能产生副作用
- **核心服务必须 Fail-Stop**：VM 崩溃会导致整个系统崩溃
- **Rust 实现需要特别注意**：使用 `_exit(1)` 而不是依赖 Drop，使用 `MaybeUninit` 避免未初始化内存的非法 Drop
