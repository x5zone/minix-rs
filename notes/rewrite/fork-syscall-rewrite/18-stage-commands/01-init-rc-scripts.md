# 01-init 与启动脚本链

> **状态**: pending（最小骨架，待改写）
> **定位**: init 与启动脚本链
> **源码**: `minix3/sbin/init/、rcorder/、shutdown/、reboot/、minix/commands/setup/、etc/rc*、etc/rc.d/（32 脚本）、etc/defaults/、etc/boot.cfg.default`
> **Rust 模块**: `os/commands/sbin/init`
> **draft 素材**: 无（新建）

## 核心点

- - init 启动链：sbin/init 读 boot.cfg 面 → 执行 rc 脚本 → 派生 getty/服务；PID 1 语义（孤儿收养/reap）
- - rc 体系：rc.conf 变量面 + rc.subr 函数面 + rc 主脚本；rc.d/ 32 脚本（mountcritlocal/fsck/sysctl/network/ttys/LOGIN/DAEMON…）
- - rcorder：rc.d 脚本依赖排序语义（KEYWORD/PROVIDE/REQUIRE）
- - shutdown/reboot：信号链与 halt 语义；setup：首启配置
- - [ARCH] A-6：rc 配置面保留 shell 脚本 vs 编译期静态 rc（重大决策）

## 边界

- - **前置依赖**: 00、kernel 启动完成、14-stage-runtime（exec/exit）
- - **不覆盖（移交）**: 服务管理协议（02）、getty 启动细节（03）、FS 挂载命令面（14）

