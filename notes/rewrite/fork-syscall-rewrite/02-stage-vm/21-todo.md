# 21-todo: Review 修复记录

## 审查文件
`21-vm-exit.md`

## 审查结果

### 发现 P0 Bug：handle_vm_willexit 未设置 EXITING 标志

#### Bug 描述

原始 `handle_vm_willexit()` 函数只是返回 Ok(())，没有设置 EXITING 标志：

```rust
pub(crate) fn handle_vm_willexit(
    table: &VmProcTable,
    endpoint: Endpoint,
) -> Result<(), VmExitError> {
    let slot = table.vm_isokendpt(endpoint)
        .map_err(|_| VmExitError::ProcessNotFound)?;

    let _active = table.get_active(slot)
        .ok_or(VmExitError::ProcessNotFound)?;

    Ok(()) // BUG: 未调用 mark_exiting()
}
```

这导致 `handle_vm_exit()` 中的检查总是失败：
```rust
if active.flags().contains(VmFlags::EXITING) {
    return Err(VmExitError::AlreadyExiting);
}
```

#### 修复内容

1. **handle_vm_willexit**：
   - 调用 `active.mark_exiting()` 设置 EXITING 标志
   - 返回 ExitingProc（之后立即 drop，但标志已设置）

2. **handle_vm_exit**：
   - 添加页表 unmap 逻辑（遍历所有区域，取消所有页表映射）
   - 调用 `exiting.reap()` 执行资源清理

### 修复文件
- `src/exit.rs` - handle_vm_willexit() 和 handle_vm_exit() 函数

### 验证
- `cargo check` 通过
