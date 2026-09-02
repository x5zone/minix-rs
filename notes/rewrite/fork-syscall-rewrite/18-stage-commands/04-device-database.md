# 04-设备节点与系统数据库

> **状态**: pending（最小骨架，待改写）
> **定位**: 设备节点与系统数据库
> **源码**: `minix3/minix/commands/MAKEDEV/、sbin/mknod/、usr.sbin/{dev_mkdb,mtree,services_mkdb}/、usr.bin/getent/、etc/{group,shells,hosts,services,protocols,motd,utmp,nsswitch.conf}`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - MAKEDEV：静态设备节点生成脚本语义（/dev 全节点面）；mknod：字符/块节点创建
- - dev_mkdb/services_mkdb：设备/服务数据库构建面；getent：数据库查询命令
- - mtree：目录层次规范与校验
- - 系统数据库文件面：group/shells/hosts/services/protocols/motd/utmp/nsswitch.conf 格式与消费方
- - [ARCH] A-14：devman 动态 + MAKEDEV 兜底

## 边界

- - **前置依赖**: 00、11-stage-devman
- - **不覆盖（移交）**: devman server 实现（11-stage-devman）

