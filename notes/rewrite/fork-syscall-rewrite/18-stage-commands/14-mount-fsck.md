# 14-挂载与文件系统检查

> **状态**: pending（最小骨架，待改写）
> **定位**: 挂载与文件系统检查
> **源码**: `minix3/sbin/{mount,fsck,fsck_ext2fs}/、minix/commands/{mount,umount,fsck.mfs}/、etc/newfstab.sh`
> **Rust 模块**: `os/commands/sbin/fsck、os/commands/sbin/mkfs（挂载面）`
> **draft 素材**: 无（新建）

## 核心点

- - mount/umount 命令面：挂载选项（rw/ro/dev/type）、fstab 生成面（newfstab.sh）
- - fsck 族：fsck/fsck.mfs/fsck_ext2fs 的检查-修复语义（阶段 pass/fail）
- - [ARCH] A-9：64 位文件系统大小面
- - 与 15-stage-fs 服务端挂载回调的调用关系（命令面 vs FS 内部语义）

## 边界

- - **前置依赖**: 06、15-stage-fs（FS 服务端语义）
- - **不覆盖（移交）**: FS server 实现（15-stage-fs）、块驱动（16-stage-drivers）

