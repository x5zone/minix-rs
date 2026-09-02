# 21-包管理与构建工具

> **状态**: pending（最小骨架，待改写）
> **定位**: 包管理与构建工具
> **源码**: `minix3/minix/commands/{pkgin_all,pkgin_cd,pkgin_sets,gcov-pull}/、usr.sbin/{postinstall,installboot}/、usr.bin/{make,mkdep,nbperf,genassym}/、etc/mk.conf`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 包管理命令契约表（10 命令）：pkgin_all/pkgin_cd/pkgin_sets（Minix 包集安装）、postinstall
- - 引导安装：installboot；源码工具：gcov-pull
- - 构建辅助：make/mkdep/nbperf/genassym + mk.conf 配置面
- - 构建链边界声明（00-master-plan 之外，仅系统内命令面）

## 边界

- - **前置依赖**: 02（服务管理面）
- - **不覆盖（移交）**: 构建工具链（gcc/binutils 宿主面，00-master-plan 外）

