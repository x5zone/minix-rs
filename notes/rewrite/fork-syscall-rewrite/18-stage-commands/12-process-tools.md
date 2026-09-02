# 12-进程与用户会话

> **状态**: pending（最小骨架，待改写）
> **定位**: 进程与用户会话
> **源码**: `minix3/bin/{date,kill,ps,sleep}/、usr.bin/{finger,from,ipcrm,ipcs,last,leave,lock,logger,logname,mesg,nice,nohup,renice,shlock,time,tty,users,w,wall,who,write}/、minix/usr.bin/{ministat,mtop,toproto}/、etc/utmp`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 进程命令契约表（28 命令）：ps/kill/nice/renice/nohup/time/sleep/date
- - 用户会话：finger/who/w/last/users/wall/write/mesg/tty/logname + utmp 数据库读写面
- - 进程间通信面：ipcs/ipcrm（共享内存/信号量/消息队列状态）
- - Minix 监控：ministat/mtop/toproto；锁：lock/shlock；日志：logger
- - 邮件提示面：from

## 边界

- - **前置依赖**: 05
- - **不覆盖（移交）**: 终端控制（13）、网络会话（19）

