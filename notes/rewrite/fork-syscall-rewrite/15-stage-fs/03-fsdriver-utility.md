# 03-fsdriver-utility: fsdriver 辅助：copy/名字/目录项/查找

> **状态**: pending（最小骨架，待改写）
> **定位**: 框架层：数据复制、getname、getdents 组装、挂载点查找语义
> **源码**: `minix3/minix/lib/libfsdriver/utility.c`、`dentry.c`、`lookup.c`
> **Rust 模块**: `minix-fs`（copy/dentry/lookup 辅助）
> **draft 素材**: 无（新建）

## 核心点

- `fsdriver_copyin`/`copyout`/`zero`：grant（endpt≠SELF）vs 本地指针（endpt==SELF）双路径，`struct fsdriver_data` 抽象
- `fsdriver_getname`：grant → 名字缓冲，`not_empty` 参数、ENAMETOOLONG/长度语义
- `fsdriver_dentry_init`/`add`/`finish`：getdents 目录项序列化（ino/name/type），`struct fsdriver_dentry` 状态机
- `fsdriver_lookup`（lookup.c）：单组件查找辅助——`access_as_dir` ucred 搜索权限检查（ROOT_UID/owner/group/supplemental）、PATH_RET_SYMLINK/PATH_GET_UCRED 标志、挂载点 EENTERMOUNT/ELEAVEMOUNT、ESYMLINK
- `vfs_ucred_t` 凭据结构与 PATH_GET_UCRED grant 约定

## 边界

- **前置依赖**: 01/02
- **不覆盖（移交）**: 各 FS 的 fs_lookup 实现（11/19/20/22/23）
