# 00-fs-overview: FS 子系统整体概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/fs/`（8 server）+ `minix3/minix/lib/libfsdriver/`、`libminixfs/`、`libvtreefs/`、`libsffs/`、`minix3/minix/include/minix/vfsif.h`
> **Rust 模块**: `os/fs/*`（8 crate）、`os/libs/minix-fs`
> **draft 素材**: `draft/README.md`（素材）

## 核心点

- FS 子系统是什么：8 个 FS server（pfs/mfs/procfs/ptyfs/ext2/isofs/vbfs/hgfs）+ 4 个共享框架库（libfsdriver/libminixfs/libvtreefs/libsffs），不是单个 server（plan §1.1）
- 语义主线图：框架（01~05）→ pfs（06，boot 第一个挂载 + 最小 server 样例）→ mfs（07~17，参考实现）→ 虚拟树变体（18~20）→ 磁盘/宿主变体（21~24）
- boot 挂载因果链：VFS `do_init_root`（servers/vfs/main.c:491）→ `mount_pfs()` → `mount_fs(DEV_IMGRD, "/", MFS)`；mfs/pfs 是 boot image 成员（kernel/table.c:62-63）
- 文档导航：6 阶段 26 篇，新编号交叉引用规则（plan §2）
- 设计原则：位置可回答性 / 禁止前向引用 / 每篇一个语义单元 / 变体 diff 原则（plan §3.6）/ ARCH 三处一致标注
- ARCH 全景：13 项设计期候选（plan §4），写文档时逐项确认

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~24、99）
