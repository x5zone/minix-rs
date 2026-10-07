# write.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vfs/write.c`
> 
> **行数**: 25 行
> 
> **核心内容**: `do_write` 函数，write 系统调用的 VFS 入口

---

## 文件概述

`write.c` 是 VFS 中**最简短的文件之一**（仅 25 行），实现了 `write()` 系统调用的入口函数。它的核心设计思想是**代码复用**：write 和 read 共享同一个底层实现 `do_read_write_peek()`，仅通过操作方向标志（`WRITING` vs `READING`）区分。

---

## 逐行讲解

### 第 1-6 行：文件头注释

```c
/* This file is the counterpart of "read.c".  It contains the code for writing
 * insofar as this is not contained in read_write().
 *
 * The entry points into this file are
 *   do_write:     call read_write to perform the WRITE system call
 */
```

**注释翻译**：
- `This file is the counterpart of "read.c"` → 此文件是 "read.c" 的对应文件
- `It contains the code for writing insofar as this is not contained in read_write()` → 它包含写入的代码，只要这些代码不包含在 read_write() 中
- `do_write: call read_write to perform the WRITE system call` → do_write：调用 read_write 执行 WRITE 系统调用

**设计思路**：
作者明确指出 write.c 是 read.c 的"对应文件"，暗示两者高度对称。这种设计减少了代码重复，但也意味着修改 read/write 逻辑时需要同时考虑两者的影响。

---

### 第 8-11 行：头文件包含

```c
#include "fs.h"
#include "file.h"
#include <minix/callnr.h>
```

**是什么**：包含 VFS 主头文件、filp 结构定义和系统调用号。

**为什么**：
- `fs.h`：所有 VFS 基础定义
- `file.h`：`struct filp` 定义（文件描述符到 vnode 的映射）
- `minix/callnr.h`：系统调用号定义

---

### 第 12-25 行：do_write 函数

```c
/*===========================================================================*
 *				do_write				     *
 *===========================================================================*/
int do_write(void)
{
/* Perform the write(fd, buffer, nbytes) system call. */

  /* See the comment in do_read(). */
  if (job_m_in.m_lc_vfs_readwrite.cum_io != 0)
	return(EINVAL);

  return(do_read_write_peek(WRITING, job_m_in.m_lc_vfs_readwrite.fd,
	job_m_in.m_lc_vfs_readwrite.buf, job_m_in.m_lc_vfs_readwrite.len));
}
```

**注释翻译**：
- `Perform the write(fd, buffer, nbytes) system call` → 执行 write(fd, buffer, nbytes) 系统调用
- `See the comment in do_read()` → 参见 do_read() 中的注释

**是什么**：write 系统调用的 VFS 入口函数。

**逐行讲解**：

**第 17 行**：函数注释，说明此函数实现 `write(fd, buffer, nbytes)` 系统调用。这是 POSIX 标准接口。

**第 20-21 行**：`cum_io` 检查
```c
if (job_m_in.m_lc_vfs_readwrite.cum_io != 0)
    return(EINVAL);
```
- **`cum_io`**：累积 I/O 字节数，用于恢复被阻塞的读写操作
- **为什么检查**：正常 write 调用时 `cum_io` 应为 0。非零值表示这是被恢复的 pipe 操作，不应通过 `do_write` 入口进入（应通过 `do_pending_pipe`）
- **`EINVAL`**：参数无效错误

**第 23-24 行**：调用共享实现
```c
return(do_read_write_peek(WRITING, job_m_in.m_lc_vfs_readwrite.fd,
    job_m_in.m_lc_vfs_readwrite.buf, job_m_in.m_lc_vfs_readwrite.len));
```
- **`WRITING`**：操作方向标志，与 `READING` 对应
- **`fd`**：文件描述符
- **`buf`**：用户态缓冲区地址（虚拟地址）
- **`len`**：要写入的字节数
- **`do_read_write_peek`**：read/write/peek 的共享实现，在 `read_write.c` 中定义

**为什么共享实现**：
read 和 write 的核心逻辑几乎相同：
1. 根据 fd 查找 filp
2. 检查 filp 的打开模式（O_RDONLY/O_WRONLY）
3. 获取 vnode 锁
4. 根据 vnode 的 `v_fs_e` 向 FS 进程发送 IPC 请求
5. 等待回复
6. 返回结果

唯一的区别是操作方向（读 vs 写）和打开模式检查。通过参数化方向标志，消除了大量重复代码。

**应用场景**：
```c
// 用户态调用
write(fd, buffer, 100);

// VFS 处理流程
// 1. main.c 的 do_work() 通过 call_vec 调用 do_write()
// 2. do_write() 调用 do_read_write_peek(WRITING, ...)
// 3. do_read_write_peek() 向 FS 进程发送写请求
// 4. FS 进程实际写入数据
// 5. 回复 VFS，VFS 回复用户进程
```

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 VFS write | Linux VFS write |
|------|------------------|-----------------|
| 位置 | 用户态 VFS 服务器 | 内核态 `sys_write` |
| 实现 | 委托给 FS 进程（IPC） | 通过 `vfs_write` → `file->f_op->write_iter` |
| 代码复用 | 与 read 共享 `do_read_write_peek` | 与 read 共享 `vfs_iter_read/write` |
| 数据拷贝 | 通过 grant 机制安全拷贝 | 通过 `copy_from_user` |
| 性能 | IPC 开销大 | 直接函数调用，开销小 |

### Rust 重构建议

```rust
// Minix3 C 代码
// int do_write(void) {
//     return(do_read_write_peek(WRITING, fd, buf, len));
// }

// Rust 改进：使用枚举区分操作方向
enum IoDirection {
    Reading,
    Writing,
}

fn do_write(fd: i32, buf: &[u8]) -> Result<usize> {
    do_read_write_peek(IoDirection::Writing, fd, buf)
}

fn do_read(fd: i32, buf: &mut [u8]) -> Result<usize> {
    do_read_write_peek(IoDirection::Reading, fd, buf)
}

// 或者使用泛型进一步抽象
trait IoOperation {
    fn direction() -> IoDirection;
    fn execute(fs: &mut FileSystem, vnode: &VNode, buf: &[u8]) -> Result<usize>;
}

struct WriteOp;
impl IoOperation for WriteOp {
    fn direction() -> IoDirection { IoDirection::Writing }
    fn execute(fs: &mut FileSystem, vnode: &VNode, buf: &[u8]) -> Result<usize> {
        // 写入逻辑
    }
}
```

---

## 总结

`write.c` 虽然只有 25 行，但体现了重要的设计原则：

1. **代码复用**：与 read 共享 `do_read_write_peek` 实现
2. **方向参数化**：通过 `WRITING`/`READING` 标志区分操作
3. **安全检查**：`cum_io != 0` 防止错误的调用路径
4. **微内核架构**：实际写入操作由 FS 进程完成，VFS 只做路由

这种设计在微内核中非常典型：VFS 作为"路由器"，将请求转发给正确的 FS 进程，自身不处理具体的文件数据。
