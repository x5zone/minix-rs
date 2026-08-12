# Fork 集成测试

> **Mock 策略**: 
> - **硬件 Mock**: 寄存器、FPU、磁盘 I/O（保留）
> - **功能逻辑**: VM/VFS 功能、IPC 通信（真实实现，不 mock）
> - **测试隔离**: 通过测试框架提供的 mock 硬件接口隔离测试环境

## 1. 测试场景

| 场景 | 描述 | 涉及函数 |
|------|------|---------|
| 基本fork | 父进程 fork，子进程运行 | `do_fork` |
| fork后exit | 子进程退出，父进程 wait | `do_fork` + `do_exit` + `do_wait4` |
| 多子进程 | 父进程 fork 多次 | `do_fork` × N |
| 进程表满 | NR_PROCS 个进程后 fork 失败 | `do_fork` |
| PID耗尽 | NR_PIDS 个 PID 后循环复用 | `get_free_pid` |
| 僵尸回收 | 子进程退出但父进程未 wait | `do_exit` + `cleanup` |
| INIT收养 | 父进程退出，子进程被 INIT 收养 | `exit_proc` |
| 追踪fork | 被追踪的进程 fork | `do_fork` + ptrace |
| srv_fork | RS 创建服务进程 | `do_srv_fork` |
| 进程组信号 | 会话领导者退出，SIGHUP 发送 | `exit_proc` |

## 2. 测试用例设计

### 2.1 基本 fork 测试

```rust
#[test]
fn test_basic_fork() {
    let mut ctx = setup_pm_context();
    let parent_pid = ctx.current_proc().pid;
    
    // 执行 fork
    let child_pid = do_fork(&mut ctx).unwrap();
    
    // 验证子进程创建成功
    assert_ne!(child_pid, parent_pid);
    assert!(ctx.table.find_by_pid(child_pid).is_some());
    
    // 验证父子关系
    let child = ctx.table.find_by_pid(child_pid).unwrap();
    assert_eq!(child.state.parent(), Some(parent_pid));
}
```

### 2.2 fork + exit + wait 完整流程

```rust
#[test]
fn test_fork_exit_wait() {
    let mut ctx = setup_pm_context();
    
    // fork
    let child_pid = do_fork(&mut ctx).unwrap();
    
    // 模拟子进程 exit
    switch_to_proc(&mut ctx, child_pid);
    do_exit(&mut ctx, 42).unwrap();
    
    // 父进程 wait
    switch_to_proc(&mut ctx, parent_pid);
    let result = do_wait4(&mut ctx, PidArg::Any, WaitOptions::empty(), None);
    
    assert_eq!(result.unwrap(), child_pid);
    // 验证子进程已被清理
    assert!(ctx.table.find_by_pid(child_pid).is_none());
}
```

### 2.3 进程表满测试

```rust
#[test]
fn test_proc_table_full() {
    let mut ctx = setup_pm_context();
    
    // 填满进程表
    for _ in 0..NR_PROCS {
        do_fork(&mut ctx).unwrap();
    }
    
    // 再次 fork 应该失败
    let result = do_fork(&mut ctx);
    assert_eq!(result.unwrap_err(), Error::EAGAIN);
}
```

### 2.4 僵尸进程测试

```rust
#[test]
fn test_zombie_process() {
    let mut ctx = setup_pm_context();
    let child_pid = do_fork(&mut ctx).unwrap();
    
    // 子进程 exit，父进程不 wait
    switch_to_proc(&mut ctx, child_pid);
    do_exit(&mut ctx, 0).unwrap();
    
    // 验证子进程变成僵尸
    let child = ctx.table.find_by_pid(child_pid).unwrap();
    assert!(child.is_zombie());
    
    // 父进程 wait 后清理
    switch_to_proc(&mut ctx, parent_pid);
    do_wait4(&mut ctx, PidArg::Specific(child_pid), WaitOptions::empty(), None).unwrap();
    
    assert!(ctx.table.find_by_pid(child_pid).is_none());
}
```

### 2.5 INIT 收养测试

```rust
#[test]
fn test_init_adoption() {
    let mut ctx = setup_pm_context();
    
    // 创建子进程
    let child_pid = do_fork(&mut ctx).unwrap();
    
    // 父进程 exit
    do_exit(&mut ctx, 0).unwrap();
    
    // 验证子进程被 INIT 收养
    let child = ctx.table.find_by_pid(child_pid).unwrap();
    assert_eq!(child.state.parent(), Some(INIT_PID));
}

### 2.6 CoW 写时复制测试

```rust
#[test]
fn test_cow_write_triggers_copy() {
    let mut ctx = setup_pm_context_with_vm();
    let child_pid = do_fork(&mut ctx).unwrap();
    
    // 父子进程共享物理页，refcount = 2
    let parent_phys = ctx.vm.get_phys_page(parent_addr);
    let child_phys = ctx.vm.get_phys_page(child_addr);
    assert_eq!(parent_phys, child_phys);
    
    // 子进程写入，触发 CoW
    switch_to_proc(&mut ctx, child_pid);
    write_to_addr(&mut ctx, child_addr, 0x42);
    
    // 验证页面已复制，refcount = 1
    let child_phys_after = ctx.vm.get_phys_page(child_addr);
    assert_ne!(parent_phys, child_phys_after);
    assert_eq!(ctx.vm.get_refcount(parent_phys), 1);
    assert_eq!(ctx.vm.get_refcount(child_phys_after), 1);
}
```

### 2.7 filp 引用计数测试

```rust
#[test]
fn test_fork_close_does_not_close_file() {
    let mut ctx = setup_pm_context_with_vfs();
    let fd = open_file(&mut ctx, "/tmp/test.txt");
    let filp_count_before = ctx.vfs.get_filp_count(fd);
    
    // fork 后 filp_count++
    let child_pid = do_fork(&mut ctx).unwrap();
    let filp_count_after_fork = ctx.vfs.get_filp_count(fd);
    assert_eq!(filp_count_after_fork, filp_count_before + 1);
    
    // 子进程 close，文件不关闭
    switch_to_proc(&mut ctx, child_pid);
    close_fd(&mut ctx, fd).unwrap();
    let filp_count_after_close = ctx.vfs.get_filp_count(fd);
    assert_eq!(filp_count_after_close, filp_count_before);
    
    // 文件仍然可读
    assert!(ctx.vfs.is_file_open(fd));
}
```

### 2.8 vnode 引用计数测试

```rust
#[test]
fn test_fork_close_does_not_release_vnode() {
    let mut ctx = setup_pm_context_with_vfs();
    let fd = open_file(&mut ctx, "/tmp/test.txt");
    let vnode = ctx.vfs.get_vnode(fd);
    let vref_count_before = ctx.vfs.get_vref_count(vnode);
    
    // fork 后 v_ref_count++
    let child_pid = do_fork(&mut ctx).unwrap();
    let vref_count_after_fork = ctx.vfs.get_vref_count(vnode);
    assert_eq!(vref_count_after_fork, vref_count_before + 1);
    
    // 子进程 close，vnode 不释放
    switch_to_proc(&mut ctx, child_pid);
    close_fd(&mut ctx, fd).unwrap();
    let vref_count_after_close = ctx.vfs.get_vref_count(vnode);
    assert_eq!(vref_count_after_close, vref_count_before);
    
    // vnode 仍然存在
    assert!(ctx.vfs.vnode_exists(vnode));
}
```

### 2.9 子进程 fork 返回 0 测试

```rust
#[test]
fn test_ret_reg_zero() {
    let mut ctx = setup_pm_context_with_kernel();
    let child_pid = do_fork(&mut ctx).unwrap();
    
    // 切换到子进程上下文
    switch_to_proc(&mut ctx, child_pid);
    
    // 验证子进程的返回寄存器为 0
    let ret_reg = ctx.kernel.get_ret_reg(child_endpoint);
    assert_eq!(ret_reg, 0);
}
```

### 2.10 共享内存不触发 CoW 测试

```rust
#[test]
fn test_shared_memory_not_cow() {
    let mut ctx = setup_pm_context_with_vm();
    
    // 创建共享内存段
    let shared_addr = ctx.vm.mmap_shared(0, 4096, Prot::ReadWrite);
    
    let child_pid = do_fork(&mut ctx).unwrap();
    
    // 子进程写入共享内存
    switch_to_proc(&mut ctx, child_pid);
    write_to_addr(&mut ctx, shared_addr, 0x42);
    
    // 验证共享内存不触发 CoW，父子仍共享同一物理页
    let parent_phys = ctx.vm.get_phys_page(shared_addr);
    let child_phys = ctx.vm.get_phys_page(shared_addr);
    assert_eq!(parent_phys, child_phys);
}
```

### 2.11 特权进程降级测试

```rust
#[test]
fn test_privileged_process_demotion() {
    let mut ctx = setup_pm_context();
    
    // 创建特权进程
    let priv_proc = create_privileged_proc(&mut ctx);
    switch_to_proc(&mut ctx, priv_proc);
    assert!(ctx.current_proc().is_privileged());
    
    // fork 后子进程降级
    let child_pid = do_fork(&mut ctx).unwrap();
    let child = ctx.table.find_by_pid(child_pid).unwrap();
    assert!(!child.is_privileged());
    assert_eq!(child.scheduler, Endpoint::RS);
}
```

### 2.12 endpoint generation 递增测试

```rust
#[test]
fn test_endpoint_generation_increment() {
    let mut ctx = setup_pm_context_with_kernel();
    let slot = 5;
    
    // 第一次使用 slot
    let child1 = do_fork_to_slot(&mut ctx, slot).unwrap();
    let ep1 = ctx.table.find_by_pid(child1).unwrap().endpoint;
    let gen1 = ctx.kernel.get_generation(ep1);
    
    // 子进程退出，槽位释放
    switch_to_proc(&mut ctx, child1);
    do_exit(&mut ctx, 0).unwrap();
    
    // 再次使用同一 slot
    let child2 = do_fork_to_slot(&mut ctx, slot).unwrap();
    let ep2 = ctx.table.find_by_pid(child2).unwrap().endpoint;
    let gen2 = ctx.kernel.get_generation(ep2);
    
    // 验证 generation 递增
    assert_eq!(gen2, gen1 + 1);
}
```

### 2.13 PID 冲突检测测试

```rust
#[test]
fn test_pid_allocation_no_conflict() {
    let mut ctx = setup_pm_context();
    let mut pids = std::collections::HashSet::new();
    
    // 创建多个子进程，验证 PID 无冲突
    for _ in 0..100 {
        let child_pid = do_fork(&mut ctx).unwrap();
        assert!(
            pids.insert(child_pid),
            "PID conflict detected: {}",
            child_pid
        );
    }
}
```

## 3. 必须通过的集成测试清单

```rust
// 全链路测试
test_full_fork_flow()                  // 完整fork流程测试
test_cow_write_triggers_copy()         // CoW写触发页面复制
test_fork_close_does_not_close_file()  // fork后close不关闭文件
test_endpoint_generation_increment()   // endpoint generation递增
test_ret_reg_zero()                    // 子进程返回值为0
test_shared_memory_not_cow()           // 共享内存不触发CoW
test_privileged_process_demotion()     // 特权进程子进程降级
test_four_table_endpoint_consistency() // 四表endpoint一致性
test_pid_pm_vfs_consistency()          // PID在PM和VFS中一致性
test_fork_table_full()                 // 进程表满返回EAGAIN
test_pid_allocation_no_conflict()      // PID分配无冲突
```

## 4. 验证检查清单

| # | 逻辑点 | 状态 |
|---|--------|------|
| M-02 | VM→Kernel SYS_FORK 消息协议 | ⬜ |
| M-05 | VM pt_bind 清除 RTS_VMINHIBIT | ⬜ |
| T-01 | 四份进程表 endpoint 一致性 | ⬜ |
| T-02 | PID 在 PM 和 VFS 中一致性 | ⬜ |
| T-03 | 子进程初始不可运行 | ⬜ |
| T-04 | CoW 写触发页面复制 | ⬜ |
| T-05 | CoW refcount 从2降为1 | ⬜ |
| T-06 | filp 引用计数验证 | ⬜ |
| T-07 | vnode 引用计数验证 | ⬜ |
| T-08 | ret_reg=0 子进程返回0 | ⬜ |
| T-09 | endpoint generation 递增 | ⬜ |
| T-10 | 共享内存段不触发 CoW | ⬜ |
| T-11 | 特权进程子进程降级 | ⬜ |
| T-12 | 进程表满返回 EAGAIN | ⬜ |
| T-13 | PID 冲突检测 | ⬜ |

## 5. 验证目标

- [ ] 所有测试场景通过
- [ ] 无内存泄漏
- [ ] 无死锁
- [ ] 进程表状态一致性
