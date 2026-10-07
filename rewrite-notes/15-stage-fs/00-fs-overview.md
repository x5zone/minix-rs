# 00-fs-overview: FS 子系统整体概览

> **重写**: 2026-09-20（edge3 卡N/S41）
> **状态**: 正文
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/fs/`（8 server）+ `minix3/minix/lib/libfsdriver/`、`libminixfs/`、`libvtreefs/`、`libsffs/`、`minix3/minix/include/minix/vfsif.h`
> **Rust 模块**: `os/fs/*`（8 server crate + `fs/fs-rt` 运行时）、`os/libs/minix-fs`
> **draft 素材**: `draft/README.md`（素材，已并入各篇）

## 1 概念：FS 子系统是什么，为什么它是这个形状

### 1.1 不是"一个文件服务器"，是一族服务器加一套框架

Minix3 的文件系统不是一个程序，而是**8 个文件系统服务器**（pfs/mfs/procfs/ptyfs/ext2/isofs/vbfs/hgfs）共用**4 个共享框架库**（libfsdriver/libminixfs/libvtreefs/libsffs）组成的一族进程。每个 server 回答一种介质的语义：mfs 管磁盘的原生文件系统、pfs 管管道、procfs/ptyfs 管虚拟树、ext2/isofs 管外来格式、vbfs/hgfs 管宿主与访客。共享框架库把"收请求、验参数、回结果"的骨架抽出来（libfsdriver 的 `fsdriver_task`），server 只填语义回调。

minix-rs 保留这个形状：8 个 server crate（`os/fs/`）+ 框架库 `minix-fs`（libfsdriver/libminixfs 的对应物）+ `minix-vtreefs`（虚拟树框架）+ 卡 D 新增的运行时 `minix-fs-rt`（SEF 启动握手、RS_INIT 出生面、生产传输——`os/fs/fs-rt`）。分层的收益是语义测试与协议测试分离：框架的 31 个分发位测一遍，8 个 server 各测各的语义。

### 1.2 语义主线：从框架到样例再到变体

26 篇文档按这条线走（编号即阅读序）：

```
框架（01~05）：协议常量 → 数据通道 → 块 I/O → 缓存 → 驱动接口
   ↓
pfs（06）：boot 第一个挂载的 server，最小完整样例
   ↓
mfs（07~17）：参考实现——挂载/目录/inode/读写/链接/元数据/维护
   ↓
虚拟树变体（18~20）：procfs/ptyfs 共用 vtreefs 框架
   ↓
磁盘/宿主变体（21~24）：ext2/isofs/vbfs/hgfs 与 mfs 的 diff
```

### 1.3 boot 挂载因果链：谁第一个被挂，为什么

根文件系统的挂载是 VFS 发起的：`do_init_root`（`servers/vfs/main.c:491`）先挂 PFS 再挂根——`mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, ...)`（`main.c:516`）。mfs 与 pfs 是 boot 镜像成员：`kernel/table.c:62-63` 里 `PFS_PROC_NR` 与 `MFS_PROC_NR` 相邻登记。这条因果链解释了 06 号篇为什么选 pfs 当最小样例（boot 链第一个真正挂载的文件系统），也解释了 mfs 为什么必须是参考实现（根文件系统跑在它上面）。

### 1.4 设计原则（全阶段文档共守）

- **位置可回答性**：任何机制问题都有一个确定的篇章可以回答，不靠全库搜索
- **禁止前向引用**：阅读路径只向后依赖，机制细节在所属篇章，调用点只留锚
- **每篇一个语义单元**：一篇讲透一个机制，不做"杂物章"
- **变体 diff 原则**：变体 server（ext2/isofs/vbfs/hgfs）只写与 mfs 的差异，不重抄共同语义
- **ARCH 三处一致**：架构级改写必须同时标注在文档正文、design 快照、代码注释三处

## 2 C 源码分析与 Rust 对位

| C 侧 | Rust 侧 | 篇章 |
|---|---|---|
| `libfsdriver`（fsdriver_task + 31 适配器） | `minix-fs`（task/driver/call）+ `fs-rt`（生产传输/出生面） | 01~05 |
| `fs/pfs` | `minix-fs-pfs` | 06 |
| `fs/mfs`（17 个 .c） | `minix-fs-mfs` | 07~17 |
| `libvtreefs` + `fs/procfs`/`fs/ptyfs` | `minix-vtreefs` + 两个 crate | 18~20 |
| `fs/ext2`/`isofs`/`vbfs`/`hgfs`、`libsffs` | 四个 crate + `minix-sffs` | 21~24 |

## 3 文档导航：6 阶段 26 篇

01~05 框架 / 06 pfs / 07~17 mfs / 18~20 虚拟树 / 21~24 磁盘与宿主变体 / 99 全局概念与测试资产总表。阅读依赖只向后：07 以后的 mfs 篇都依赖 01~05 的框架定义；变体篇（21~24）只讲 diff。

## 4 边界

- **前置依赖**: 05-stage-vfs（VFS 是协议的另一端，`vfsif.h` 的三十三章节归那边）
- **不覆盖（移交）**: VFS 侧的请求发起与调度（`../05-stage-vfs/`）；块驱动的接缝（edge 登记 E-FSBDEV）；二级缓存的零拷贝通道（E-FSVMCACHE，已由 edge3 S38 交付）；fsck/mkfs 命令（E-FSCMDS，卡 G 认领）

## 7 参见

- 测试资产总表（全 crate 单点计数）：99 篇 §2
- 四条 edge 锚点的当前状态：`../../edge3.md` S29/S30/S32/S38 行
