# 23-isofs: ISOFS：ISO9660 只读文件系统

> **状态**: pending（最小骨架，待改写）
> **定位**: 磁盘变体：光盘只读 FS（后置）
> **源码**: `minix3/minix/fs/isofs/`（12 个 .c）
> **Rust 模块**: `os/fs/isofs`
> **draft 素材**: 无（新建）

## 核心点

- isofs 定位：ISO9660 只读 CD/DVD FS（libminixfs 变体）
- mount/super：PVD（Primary Volume Descriptor）解析、块大小（**子页块 FIXME**）
- inode.c：目录记录（directory record）→ inode 模型
- path/read：只读路径查找与数据读取（无写面）
- **Rock Ridge**：susp.c（System Use Sharing Protocol）+ susp_rock_ridge.c（RR 属性：长名/符号链接/UID）
- norock 选项（optset）、link 空实现、stadir
- fdr 回调表：只读子集（无 create/write/link 等）

## 边界

- **前置依赖**: 04~10
- **不覆盖（移交）**: 光盘驱动（16-stage-drivers）、mfs 对照语义（07~17）
