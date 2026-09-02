# 02-服务管理与调度

> **状态**: pending（最小骨架，待改写）
> **定位**: 服务管理与调度
> **源码**: `minix3/usr.sbin/service/、minix/commands/{svrctl,minix-service,cron,crontab,at,atnormalize,update}/、etc/crontab、etc/rs.lwip、etc/rs.single`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - service/minix-service：RS 客户端命令面（SERVICE_UP/DOWN/STATUS 协议语义）；svrctl：低层 RS 控制接口
- - 调度面：cron/crontab（etc/crontab 格式）、at/atnormalize（一次性调度）、update（sync 守护）
- - rs.lwip/rs.single：RS 启动配置面（与本 stage 命令交互）
- - 常驻服务启动面：与 19（inetd/syslogd 守护实现）的调用关系

## 边界

- - **前置依赖**: 01（rc 调用面）、03-stage-rs（RS 协议）
- - **不覆盖（移交）**: RS server 端实现（03-stage-rs）、守护进程实现（19）

