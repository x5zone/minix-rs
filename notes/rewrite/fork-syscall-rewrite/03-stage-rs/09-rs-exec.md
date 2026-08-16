# 09-rs-exec: 服务二进制加载与执行

> **分类**: 阶段 4 — 服务创建与配置（从槽位到运行进程的第二步：二进制）
> **源码**: `minix3/minix/servers/rs/exec.c`（165 行，`srv_execve`—21、`do_exec`—62、`exec_restart`—121、`read_seg`—143）、`minix3/minix/servers/rs/manager.c:1354-1455`（`share_exec`—1357、`read_exec`—1372、`free_exec`—1424）、`minix3/minix/servers/rs/manager.c:531-650`（`create_service` 的 exec 调用点）、`minix3/minix/servers/rs/manager.c:1629-1661`（`edit_slot` 的 `RSS_COPY`/`RSS_REUSE` 分支）、`minix3/minix/lib/libexec/exec_elf.c`（`libexec_load_elf`）、`minix3/minix/lib/libc/sys/stack_utils.c`（`minix_stack_params`/`minix_stack_fill`）
> **Rust 模块**: `os/servers/rs/src/exec.rs`（`validate_image`/`share_exec`/`has_shared_exec`/`free_exec`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md`（`RSS_COPY`/`RSS_REUSE` 输入、`r_argv` 来源）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（`SF_USE_COPY`/`SF_NEED_COPY` 标志、`ServiceSlot.exec` 字段）
> **说明**: 服务的二进制映像由 RS 亲自加载：它要么从命令路径 `stat`/`open`/`read` 进内存（`read_exec`），要么复用另一槽的共享副本（`share_exec`），随后 `srv_execve` 把映像 exec 进已 fork 的子进程（10 的创建路径调用）。本文档建模内存副本的**生命周期与共享语义**（ARCH A-5），并固化 `srv_execve` 全链的外部契约（ARCH A-8，接线在 19）。

---

## 1. 概念：谁把服务的二进制放进地址空间

### 1.0 章节引言

`08-rs-slot-config.md` 之后，槽位里有了命令、IPC 列表、调度参数——但还没有**可执行的映像**。在 Minix3 里，服务二进制映像的加载不是 VFS/PM 的事，而是 RS 自己的事：`create_service`（10）fork 出子进程后，由 RS 读取映像、构造栈帧、通过 PM 完成 exec。本文档回答的问题是：**服务映像从"槽位声明"到"子进程地址空间"的完整路径是什么，内存副本在 RS 表内如何共享与释放**。

> **本章不讲什么**（机制一律移交）:
> - `rs_start` 的校验与槽位落地（`08-rs-slot-config.md`）——本文档只消费 `RSS_COPY`/`RSS_REUSE` 两个输入位与 `r_argv[0]`
> - fork/priv/sched 的创建编排（`10-rs-service-create.md`）——本文档只陈述 `create_service` 对 exec 面的调用点（manager.c:625-650）
> - `PM_EXEC_RESTART` 的消息布局与 `sys_datacopy` 的内核机制（`19-rs-external-interfaces.md`）——本文档只固化语义契约
> - ELF 段解析器内部（`os/libs/minix-elf` crate，ARCH A-8）

### 1.1 为什么 RS 亲自加载（WHY）

服务是系统进程，不是普通用户进程：它的 exec 必须与"RS 表里的槽位"严格同步（pid、endpoint、priv 结构、调度器都在 `create_service` 里按顺序就位）。如果交给 PM 走普通 `execve` 路径，RS 就失去了对"映像来自哪里"（内存副本 vs 命令路径）的控制。所以 RS 用**自定义 exec 链**：`srv_execve`（exec.c:21）在 RS 进程内构建栈帧，`do_exec`（exec.c:62）调用 libexec 加载器把段拷入子进程地址空间，最后 `exec_restart`（exec.c:121）通知 PM 完成 exec 收尾。

### 1.2 两条映像路径（WHAT）

```
08 槽位落地
  ├─ RSS_COPY（manager.c:1628-1661）
  │    ├─ RSS_REUSE + 已存在同名 SF_USE_COPY 槽 → share_exec（共享指针）
  │    └─ 否则 → read_exec（stat/open/read 全量读入）
  │    → rpub->sys_flags |= SF_USE_COPY（映像常驻，跨 restart 复用）
  └─ 无 RSS_COPY → 命令路径：create_service 时按需 read_exec + exec 后 free_exec
       （manager.c:625-650：非 SF_USE_COPY 时读入、exec、立即释放）
```

- **常驻副本路径**（`SF_USE_COPY`）：映像保存在 RS 堆里，供 restarts/LU 复用（`restart_service` 的 clone 路径靠 `share_exec` 分享，manager.c:1834-1835）。`SF_NEED_COPY`（rs.h:193）表示"创建时必须有内存副本"，没有则 `EPERM`（manager.c:555-560）。
- **命令路径**（无 `SF_USE_COPY`）：每次创建时 `read_exec` 读入 → `srv_execve` exec → `free_exec` 释放（manager.c:642-644）。`SF_NEED_COPY` 未设时若无命令（`r_cmd == ""`）→ `EPERM`（manager.c:563-568）。

### 1.3 共享与释放的引用计数语义

C 用裸指针共享（`rp_dst->r_exec = rp_src->r_exec`，manager.c:1366），`free_exec` 靠**全表扫描**判断"还有没有别的槽指向同一块内存"（manager.c:1433-1440）。这是"引用计数"的原始形态：谁最后释放谁 free。Rust 用 `Arc<[u8]>` 把这份手动记账交给类型系统（ARCH A-5），但**扫描语义保留**——因为 `Arc::strong_count` 会数进槽外的克隆（测试持有、future 代码持有），而 C 的语义只数表内槽位。

---

## 2. C 源码分析

### 2.1 `read_exec`：从文件系统读入映像（manager.c:1372-1419）

```c
r= stat(e_name, &sb);            /* 1384：映像来自 r_argv[0] */
if (r != 0) return -errno;       /* 1386：stat 失败 → -errno */
if (sb.st_size < sizeof(Elf_Ehdr))
    return ENOEXEC;              /* 1388-1389：比 ELF 头还小 → 不可执行 */
fd= open(e_name, O_RDONLY);      /* 1391 */
if (fd == -1) return -errno;     /* 1392-1393 */
rp->r_exec_len= sb.st_size;      /* 1395 */
rp->r_exec= malloc(rp->r_exec_len);  /* 1396 */
if (rp->r_exec == NULL) { close(fd); return ENOMEM; }  /* 1397-1403 */
r= read(fd, rp->r_exec, rp->r_exec_len);  /* 1405 */
e= errno; close(fd);             /* 1406-1407 */
if (r == rp->r_exec_len) return OK;  /* 1408-1409 */
free_exec(rp);                   /* 1413：短读/失败 → 释放已分配映像 */
if (r >= 0) return EIO;          /* 1416-1417：短读（r>=0 且 != len）→ EIO */
else return -e;                  /* 1418-1419：read 错误 → -errno */
```

错误面：`-errno`（stat/open/read 失败）、`ENOEXEC`（映像比 `Elf_Ehdr` 小）、`ENOMEM`（分配失败）、`EIO`（短读）。注意 `read` 返回 0（EOF）也走 `EIO` 分支——只有"一次读满 `r_exec_len`"才成功，没有循环读（映像文件由 RS 假设可一次读完）。

### 2.2 `share_exec`/`free_exec`：共享与释放（manager.c:1357-1367, 1424-1455）

```c
void share_exec(rp_dst, rp_src) {        /* 1357 */
    rp_dst->r_exec_len = rp_src->r_exec_len;  /* 1365 */
    rp_dst->r_exec = rp_src->r_exec;          /* 1366：指针共享 */
}
void free_exec(rp) {                      /* 1424 */
    /* 扫描全表找共享者（1433-1440）：RS_IN_USE && other != rp && r_exec 同址 */
    for (slot_nr = 0; slot_nr < NR_SYS_PROCS; slot_nr++) {
        other_rp = &rproc[slot_nr];
        if (other_rp->r_flags & RS_IN_USE && other_rp != rp
            && other_rp->r_exec == rp->r_exec) { has_shared_exec = TRUE; break; }
    }
    if(!has_shared_exec) free(rp->r_exec);  /* 1443-1446：最后一个持有者 */
    rp->r_exec = NULL;                      /* 1453 */
    rp->r_exec_len = 0;                     /* 1454 */
}
```

关键点：`free_exec` 在**置空之前**扫描；共享判断只看 `RS_IN_USE` 槽（排除了刚 free 的槽）。Rust 的 `has_shared_exec` 用 `Arc::ptr_eq` 复刻同样的扫描（ARCH A-5）。

> **R18（2026-08-16）**：`free_exec` 的第三个生产调用点——`free_slot` 的 `SF_USE_COPY` 分支
> （manager.c:2100-2102）——已落进表原语（02 §3.6）：槽释放时先取 `sys_flags` 再
> `crate::exec::free_exec(table, id)`，丢弃该槽的 exec `Arc`（非最后一个持有者则保留给共享者）。
> 此前生产路径对 `free_exec` 零调用，被 free 的行会滞留映像直到槽被重用；现在与 C"free 即释放"
> 语义对齐。命令路径（manager.c:642-644）仍是 09 execve 接线的调用点。

### 2.3 `edit_slot` 的 `RSS_COPY`/`RSS_REUSE` 分支（manager.c:1629-1661）

```c
if ((rs_start->rss_flags & RSS_COPY) && !(rpub->sys_flags & SF_USE_COPY)) {
    int exst_cpy = 0;
    if(rs_start->rss_flags & RSS_REUSE) {          /* 1635 */
        for(i = 0; i < NR_SYS_PROCS; i++) {         /* 1638-1648 */
            rp2 = &rproc[i];
            if(strcmp(rpub->proc_name, rpub2->proc_name) == 0 &&
               (rpub2->sys_flags & SF_USE_COPY)) { exst_cpy = 1; break; }
        }
    }
    if(!exst_cpy) s = read_exec(rp);                /* 1653 */
    else          share_exec(rp, rp2);              /* 1655 */
    if (s != OK) return s;
    rpub->sys_flags |= SF_USE_COPY;                 /* 1660 */
}
```

`RSS_REUSE` 的语义：**同名**（`proc_name` 相等）且已 `SF_USE_COPY` 的槽 → 共享其映像，不重复读盘。这正是 `RSS_COPY` + `RSS_REUSE` 同时设置时 `create_service` 能免读盘的原因。

### 2.4 `srv_execve`：构建栈帧（exec.c:21-59）

```c
int srv_execve(int proc_e, char *exec, size_t exec_len, char *progname,
               char **argv, char **envp)
{
    minix_stack_params(argv[0], argv, envp, &frame_size, &overflow, &argc, &envc);
    if (overflow) { errno = E2BIG; return -1; }      /* 38-41 */
    if ((frame = (char *) sbrk(frame_size)) == (char *) -1) {  /* 44 */
        errno = E2BIG; return -1; }
    minix_stack_fill(argv[0], argc, argv, envc, envp, frame_size, frame, &vsp, &psp);
    r = do_exec(proc_e, exec, exec_len, progname, frame, frame_size,
                vsp + ((char *)psp - frame));        /* 52-53：ps_str 偏移 */
    (void) sbrk(-frame_size);                        /* 56：失败也回滚栈帧 */
    return r;
}
```

`minix_stack_params`/`minix_stack_fill`（lib/libc/sys/stack_utils.c:76,119）负责把 argv/envp 排布成新进程的初始栈（含 `ps_strings` 结构）。`ps_str` 是 `ps_strings` 在栈帧内的偏移（exec.c:53），后续 `exec_restart` 把它传给 PM。

### 2.5 `do_exec`：装配 exec_info 并加载（exec.c:62-116）

```c
memset(&execi, 0, sizeof(execi));
execi.stack_high = minix_get_user_sp();     /* 72：进程栈顶 */
execi.stack_size = DEFAULT_STACK_LIMIT;     /* 73：sys_config.h，4MB */
execi.proc_e = proc_e; execi.hdr = exec;
execi.filesize = execi.hdr_len = exec_len;  /* 76 */
strncpy(execi.progname, progname, PROC_NAME_LEN-1);  /* 77-78：type.h:145=16 */
execi.frame_len = frame_len;
execi.copymem = read_seg;                   /* 82：段拷贝回调 */
execi.clearproc = libexec_clearproc_vm_procctl;       /* 83 */
execi.clearmem = libexec_clear_sys_memset;            /* 84 */
execi.allocmem_prealloc_cleared = libexec_alloc_mmap_prealloc_cleared;  /* 85 */
execi.allocmem_prealloc_junk = libexec_alloc_mmap_prealloc_junk;        /* 86 */
execi.allocmem_ondemand = libexec_alloc_mmap_ondemand;                  /* 87 */
for(i = 0; exec_loaders[i].load_object != NULL; i++) {  /* 89-93 */
    r = (*exec_loaders[i].load_object)(&execi);         /* 唯一 loader: libexec_load_elf */
    if (r == OK) break;
}
if (r != OK) { printf("RS: do_exec: loading error %d\n", r); return r; }  /* 96-99 */
if((r = libexec_pm_newexec(execi.proc_e, &execi)) != OK) return r;        /* 102 */
vsp = execi.stack_high - frame_len;          /* 106：栈帧落点 */
r = sys_datacopy(SELF, (vir_bytes) frame, proc_e, (vir_bytes) vsp,
                 (phys_bytes) frame_len);    /* 107-108：帧拷入子进程 */
if (r != OK) { exec_restart(proc_e, r, execi.pc, ps_str); return r; }  /* 109-113 */
return exec_restart(proc_e, OK, execi.pc, ps_str);  /* 115 */
```

`libexec_load_elf`（exec_elf.c:127）做 ELF 头校验（`elf_unpack`）、拒绝带解释器（动态链接）的映像（`elf_has_interpreter` → `ENOEXEC`）、round 栈、逐段调 `copymem`（= `read_seg`）把 `PT_LOAD` 段拷入。

### 2.6 `exec_restart`/`read_seg`（exec.c:121-165）

```c
static int exec_restart(int proc_e, int result, vir_bytes pc, vir_bytes ps_str) {
    m.m_type = PM_EXEC_RESTART;
    m.m_rs_pm_exec_restart.endpt = proc_e;
    m.m_rs_pm_exec_restart.result = result;
    m.m_rs_pm_exec_restart.pc = pc;
    m.m_rs_pm_exec_restart.ps_str = ps_str;
    r = ipc_sendrec(PM_PROC_NR, &m);       /* 133 */
    if (r != OK) return r;
    return m.m_type;                        /* 137：PM 的回复即结果 */
}
static int read_seg(struct exec_info *execi, off_t off,
                    vir_bytes seg_addr, size_t seg_bytes) {
    if (off+seg_bytes > execi->hdr_len) return ENOEXEC;  /* 158：越界 → ENOEXEC */
    if((r= sys_datacopy(SELF, ((vir_bytes)execi->hdr)+off,
        execi->proc_e, seg_addr, seg_bytes)) != OK) { ... }
    return r;
}
```

`read_seg` 是加载器与 RS 映像缓冲之间的桥：`libexec_load_elf` 请求"把文件偏移 `off` 起 `seg_bytes` 字节拷到 `seg_addr`"，`read_seg` 先做边界检查再 `sys_datacopy`。

---

## 3. Rust 设计决策

### 3.1 `Arc<[u8]>` 副本语义（D1，ARCH A-5）

```rust
// ServiceSlot 内：
pub exec: Option<Arc<[u8]>>,   // C: char *r_exec + size_t r_exec_len → 单值
```

- C 的 `r_exec` 指针 + `r_exec_len` 两个字段合并为 `Arc<[u8]>`（长度内建，消除了"指针与长度不同步"的一类 C bug，如 free_exec 只清指针不清长度）。
- `share_exec(dst, src)` = `Arc::clone`：两个槽强引用同一缓冲，与 C 的指针共享（manager.c:1366）等价。
- **与 C 的差异**：C 的 `free()` 是即时释放；Rust 的 `drop` 延迟到最后一个 `Arc` 离开作用域。行为契约（"最后一个持有者释放"）不变，但"释放时点"从确定性变为引用计数驱动——这正是 ARC 设计点（ARCH A-5 已标注）。

### 3.2 `free_exec` 的扫描语义（D2）

```rust
pub fn has_shared_exec(rp: &ServiceSlot, table: &RProcTable) -> bool {
    // C: manager.c:1433-1440 —— 全表扫描，排除自身，RS_IN_USE 限定
    table.iter_in_use().any(|(_, other)| {
        !core::ptr::eq(other, rp) && other.exec.as_ref().is_some_and(|o| Arc::ptr_eq(o, img))
    })
}
```

- **保留 O(N) 扫描**而非用 `Arc::strong_count > 1`：`strong_count` 会数进表外克隆（例如测试句柄、19 接线后的临时持有），与 C"只数表内槽位"的语义不同。文档化这一取舍（§4 不变量 3）。
- `free_exec` 的释放由 `Arc` 的 **last-holder-free** 完成，与 C 的"扫描后 `free`"（manager.c:1443-1446）等价；`has_shared_exec` 的扫描作为显式 API 保留（本模块导出 + 测试直接断言），等价 C 的 manager.c:1433-1440。

### 3.3 `validate_image` 纯化（D3）

`read_exec` 的 `st_size < sizeof(Elf_Ehdr) → ENOEXEC`（manager.c:1388-1389）与 `libexec_load_elf` 的魔数/class/data 校验（exec_elf.c:127-141）被纯化为 `validate_image`：

```rust
pub fn validate_image(image: &[u8]) -> Result<(), i32> {
    if image.len() < 64 { return Err(ENOEXEC); }        // Elf64_Ehdr = 64B
    if &image[0..4] == b"\x7fELF" && image[4] == 2 && image[5] == 1 {
        return Ok(());                                  // ELFCLASS64 + ELFDATA2LSB
    }
    Err(ENOEXEC)
}
```

边界：这只是**最小门**（头大小 + 魔数 + 64 位小端），完整解析（`e_phnum` 循环、段重叠、解释器拒绝）属于 `minix-elf` crate 的 `parse_ehdr`/`segment_iter`（ARCH A-8），在 19 接线后由 `do_exec` 的加载链调用。

### 3.4 `srv_execve`/`do_exec`/`exec_restart`/`read_seg` 的 DEFERRED 契约（D4）

这四者依赖内核面（`sbrk`、`sys_datacopy`、`ipc_sendrec(PM)`、`minix_get_user_sp`、libexec 回调族），接线在 `19-rs-external-interfaces.md`。本文档固化语义契约，供 19 与 10 的 `create_service` 实现引用：

| C 函数 | Rust 契约 | 依赖 |
|--------|-----------|------|
| `srv_execve`（exec.c:21） | `srv_execve(proc_e, exec, progname, argv, envp) -> Result<i32,i32>`：栈帧构建（minix_stack_params/fill 语义）+ `do_exec`；overflow/`sbrk` 失败 → `E2BIG`；栈帧事后回滚 | minix-sys：stack builder、sbrk |
| `do_exec`（exec.c:62） | `do_exec(proc_e, exec, progname, frame, frame_len, ps_str)`：exec_info 装配（`stack_high = minix_get_user_sp()`、`stack_size = DEFAULT_STACK_LIMIT`）+ loader 循环 + `libexec_pm_newexec` + 帧 `sys_datacopy` + `exec_restart` | minix-sys：datacopy、PM；minix-elf：loader |
| `exec_restart`（exec.c:121） | `exec_restart(proc_e, result, pc, ps_str)`：`PM_EXEC_RESTART` 消息（endpt/result/pc/ps_str）+ `ipc_sendrec(PM_PROC_NR)`，回复即结果 | minix-types：`MessRsPmExecRestart`；minix-sys：sendrec |
| `read_seg`（exec.c:143） | `read_seg(hdr, hdr_len, off, seg_addr, seg_bytes)`：`off+seg_bytes > hdr_len → ENOEXEC`（exec.c:158）+ `sys_datacopy` | minix-sys：datacopy |

**不变量**：`read_seg` 的越界检查是"加载器信任映像偏移"的最后防线——Rust 侧必须保留（这是把"信任边界"从 ELF 解析器移到 RS 的明确点）。

---

## 4. 实现详解（exec.rs）

模块结构（已实现）：

```
exec.rs
├─ validate_image（D3：ELF 最小门校验）
├─ share_exec（D1：Arc::clone）
├─ has_shared_exec（D2：iter_in_use 扫描 + Arc::ptr_eq）
├─ free_exec（D1/D2：先扫后清）
└─ #[cfg(test)] 5 个测试（§5）
```

关键不变量：

1. **最后持有者释放**：`free_exec` 置 `exec = None` 由 `Arc` 的 last-holder-free 完成，等价 C 的"扫描后 `free`"（manager.c:1443-1446）；`has_shared_exec`（`iter_in_use` 扫描 + `Arc::ptr_eq`，等价 manager.c:1433-1440）作为显式 API 保留并被测试断言。
2. **共享只发生一次**：`share_exec` 不改 `sys_flags`——`SF_USE_COPY` 的置位在 `edit_slot`（08 范围，manager.c:1660）与 `clone_slot`（manager.c:1834-1835 由 10 调用 `share_exec`）处完成，exec.rs 只管映像指针。
3. **长度内建**：无 `r_exec_len` 字段；`Arc<[u8]>` 的长度即 C 的 `r_exec_len`，杜绝长度/指针失配。
4. **最小门与完整解析分层**：`validate_image` 只做 `read_exec` 等价检查（manager.c:1388-1389 + 魔数）；`parse_ehdr`/`segment_iter` 在 minix-elf，19 接线。
5. **DEFERRED 面 fail-closed**：`srv_execve`/`do_exec` 依赖的 kernel 面在 19 前不编译为 stub 路径（同 `KernelApi` 模式，01 §3.6）——本模块只交付纯语义。

---

## 5. 测试要点

`cargo test -p minix-rs --lib`（exec 相关 5 个）：

| 测试 | 覆盖 |
|------|------|
| `test_validate_image_rejects_tiny` | 4 字节 → ENOEXEC（manager.c:1388 最小门） |
| `test_validate_image_accepts_minimum_ehdr` | 64 字节 + 魔数 + class/data → OK |
| `test_share_exec_clones` | `Arc::clone` 后两槽共享同一缓冲（`Arc::ptr_eq`） |
| `test_free_exec_exclusive` | 单持有者 → 释放（exec = None） |
| `test_free_exec_shared_keeps_other` | 共享者存在 → 本槽断开、他槽保留（manager.c:1433-1440 扫描） |

**Gate D 证据**：`rg "fn (validate_image|share_exec|has_shared_exec|free_exec)" os/servers/rs/src/exec.rs` —— 4 个函数全部存在；无 `todo!`/`unimplemented!`（DEFERRED 面以文档契约 + `KernelApi` 模式表达，非 stub 占位）。

---

## 6. 过渡：从"二进制"到"进程"

09 交付的是**映像**——槽位拥有了可 exec 的字节序列与共享/释放规则。下一篇 `10-rs-service-create.md` 把这些映像接进 `create_service` 的编排：`srv_fork`（A-1）→ priv/sched 就位 → `read_exec`/`share_exec` → `srv_execve`（manager.c:531-650）→ 发布（11）。在启动时序中，09 位于"服务创建机制"子链（08→09→10→11）的第二步；运行时路径上，`RS_UP`/`RS_EDIT` 的 `RSS_COPY` 分支（08）在创建时经 09 的读入/共享完成映像准备。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/08-rs-slot-config.md` — `RSS_COPY`/`RSS_REUSE` 输入（§2.4）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md` — `create_service` 消费 exec 面（§2.5 调用点）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` — `SF_USE_COPY`/`SF_NEED_COPY` 标志（§2.4）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` — sys_datacopy / PM_EXEC_RESTART / minix_stack_* 契约
- `os/libs/minix-elf/src/lib.rs` — `parse_ehdr`/`segment_iter`/`entry_point`（A-8 完整解析）
- `minix3/minix/servers/rs/exec.c`、`manager.c:1354-1455` — ground truth
- `minix3/minix/lib/libexec/exec_elf.c` — `libexec_load_elf` 校验链
- `minix3/minix/lib/libc/sys/stack_utils.c` — `minix_stack_params`/`minix_stack_fill`
