# 总规划说明

## 文档主线变更

本目录（`fork-syscall-rewrite/`）的文档主线已调整：

- **旧主线**：以 `fork` 系统调用为主线，按 fork 在各服务中的执行路径分阶段（PM→VM→Kernel→VFS→SCHED）。
- **新主线**：以**服务的启动执行顺序**为主线。`fork` 系统调用降为次主线，在相关服务章节内部展开。

### 变更原因

旧主线以 fork 为线索，实践发现：fork 路径会牵涉大量尚未实现的服务逻辑，导致文档顺序被迫频繁跳跃、补述未实现内容，打乱阅读流。改用服务启动执行顺序为主线后，文档流与系统实际启动因果链一致，fork 作为次主线在 PM/VFS 等章节内自然展开即可。

## 新目录结构（按启动执行顺序）

| 编号 | 目录 | 服务 | 启动顺序说明 |
|------|------|------|-------------|
| 00 | `00-master-plan/` | — | 顶层规划（本目录） |
| 01 | `01-stage-kernel/` | Kernel | boot 期最先运行，含 boot-shim、kmain、proc init、IPC、syscall 等 |
| 02 | `02-stage-vm/` | VM | 执行顺序上最先运行的用户服务（ptproc，`main.c:196` 立即可调度 + `:265-267` VMINHIBIT），为后续服务建立页表 |
| 03 | `03-stage-rs/` | RS | root system process（`proc.h:279` = RS_PROC_NR），VM 之后第二个运行，负责加载并启动其余用户服务 |
| 04 | `04-stage-pm/` | PM | 由 RS 加载，进程管理；fork 次主线核心 |
| 05 | `05-stage-vfs/` | VFS | 由 RS 加载，虚拟文件系统；fork 次主线涉及 fd 复制 |
| 06 | `06-stage-sched/` | SCHED | 由 RS 加载，调度参数继承 |
| 07 | `07-stage-ds/` | DS | Data Store，系统服务注册与查询（boot_image 登记顺序第一，`table.c:52`） |
| 08 | `08-stage-is/` | IS | Information Server（不在 boot_image，RS 运行时加载） |
| 09 | `09-stage-init/` | INIT | 用户态 init，启动登录/用户进程（boot_image 最后一项，`table.c:64`） |
| 10 | `10-stage-mib/` | MIB | 系统信息库（**boot_image 直接登记**，`table.c:60`；2026-08-14 补建） |
| 11 | `11-stage-devman/` | DEVMAN | 设备管理（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 12 | `12-stage-input/` | INPUT | 输入服务器（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 13 | `13-stage-ipc/` | IPC | 用户态 IPC 服务（不在 boot_image，RS 运行时加载；2026-08-14 补建） |
| 14 | `14-stage-integration/` | — | 跨服务集成、状态机、端到端测试（原 10，2026-08-14 后移） |
| 15 | `15-redesign/` | — | 设计重构记录（原 11，2026-08-14 后移） |

> **boot 两层语义**：登记顺序（`kernel/table.c:44-64` boot_image 数组）= 模块槽位顺序（ds→rs→pm→sched→vfs→memory→tty→mib→vm→pfs→mfs→init）；执行顺序（`kernel/main.c:196` 仅 kernel 任务 + RS + VM 立即可调度；`:265-267` 非 VM 挂 RTS_VMINHIBIT）= kernel 任务 → VM → RS → 其余。目录编号采用执行语义 + 阅读理解顺序。

### 启动顺序因果链

```
Kernel (boot)
    │  boot 期加载 VM ELF（ptproc）；VM 立即可调度，其余挂 RTS_VMINHIBIT
    ▼
VM (ptproc)
    │  为 PM/VFS/RS 等创建页表（main.c:265-267 解除抑制）
    ▼
RS (root sysproc)
    │  运行时加载并启动其余用户服务
    ├─► PM / SCHED / VFS / DS / MIB   （boot_image 直接登记，VM 解除后即可运行）
    ├─► IS / DEVMAN / INPUT / IPC     （RS 运行时加载）
    └─► INIT                          （boot_image 最后一项）
```

> 依据：`01-stage-kernel/09-vm-boot-protocol.md` 第 27 行——"VM 为 PM/VFS/RS 等创建页表"；
> `01-stage-kernel/06-proc-init-boot-proc.md`——boot 循环中仅 VM（`VM_PROC_NR`）执行 `arch_boot_proc` 加载 ELF，其余进程跳过，由 RS 运行时加载。

## 旧→新目录映射

| 旧目录 | 新目录 |
|--------|--------|
| `01-stage-pm/` | `04-stage-pm/` |
| `02-stage-vm/` | `02-stage-vm/`（不变） |
| `03-stage-kernel/` | `01-stage-kernel/` |
| `04-stage-vfs/` | `05-stage-vfs/` |
| `05-stage-sched/` | `06-stage-sched/` |
| `06-stage-integration/` | `14-stage-integration/`（2026-08-14 后移） |
| `07-redesign/` | `15-redesign/`（2026-08-14 后移） |
| —（新建） | `03-stage-rs/` |
| —（新建） | `07-stage-ds/` |
| —（新建） | `08-stage-is/` |
| —（新建） | `09-stage-init/` |
| —（新建） | `10-stage-mib/`（2026-08-14 补建，boot_image 直接登记） |
| —（新建） | `11-stage-devman/`（2026-08-14 补建，RS 加载） |
| —（新建） | `12-stage-input/`（2026-08-14 补建，RS 加载） |
| —（新建） | `13-stage-ipc/`（2026-08-14 补建，RS 加载） |

## 本目录现有文档状态（defer）

> **注意**：本目录（`00-master-plan/`）下 `01-*.md` ~ `15-*.md` 等文档均基于**旧主线**（fork 为主线）编写，内容已过时：
> - 阶段编号与命名基于旧的 fork 分阶段方案（如 `phase1-pm`、`phase2-vm`、`phase3-kernel`）；
> - 路线图、里程碑、依赖关系图均以 fork 执行路径组织，与新主线（服务启动顺序）不一致。
>
> 这些文档**暂不修改**（defer）。新主线规划以上文本节为准。待新主线文档稳定后，再决定是增量更新还是整体重写本目录文档。

## 内部引用说明

目录重命名后，文档间的相对路径引用（如 `../03-stage-kernel/...`）会失效。这些引用暂不修复，后续通过 `grep` 批量定位并修正。修正时可参照上表"旧→新目录映射"。

## 顶层 README.md 状态（defer）

> **注意**：上级目录 [`fork-syscall-rewrite/README.md`](../README.md) 同样基于旧主线编写，列出的目录结构（`01-stage-pm`、`03-stage-kernel` 等）、阶段映射、项目进度表均已过时，**暂不修改**（defer）。新主线目录结构以本文档"新目录结构"章节为准。
