# 16-镜像与介质工具

> **状态**: pending（最小骨架，待改写）
> **定位**: 镜像与介质工具
> **源码**: `minix3/minix/commands/{writeisofs,isoread,dosread,vol,eject,cdprobe,ramdisk,loadramdisk,rawspeed}/、usr.sbin/vnconfig/、bin/dd/`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 镜像命令契约表（11 命令）：writeisofs/isoread（ISO）、dosread（FAT 读取）、vol/eject/cdprobe（介质控制）
- - ramdisk/loadramdisk：内存盘镜像加载
- - vnconfig：vnode 磁盘配置（磁盘镜像挂载面）
- - dd：块级复制语义（bs/count/skip/seek、转换面）
- - [ARCH] A-9：64 位块偏移

## 边界

- - **前置依赖**: 14
- - **不覆盖（移交）**: 驱动实现（16-stage-drivers）

