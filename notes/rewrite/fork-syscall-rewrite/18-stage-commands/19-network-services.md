# 19-网络服务与守护

> **状态**: pending（最小骨架，待改写）
> **定位**: 网络服务与守护
> **源码**: `minix3/usr.sbin/{inetd,syslogd}/、libexec/{ftpd,telnetd,rshd,fingerd,httpd}/、usr.bin/{telnet,ftp,rsh,whois,mail}/、bin/{rcmd,rcp}/、minix/commands/{fetch,zmodem,lp,lpd,mail}/、etc/{inetd.conf,syslog.conf,inet.conf}`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - 守护进程命令契约表：inetd（服务分发守护）/syslogd（系统日志）
- - libexec 服务：ftpd/telnetd/rshd/fingerd/httpd 的启动与协议面
- - 客户端命令：telnet/ftp/rsh/rcp/rcmd/whois/fetch/zmodem
- - 邮件/打印面：mail/lp/lpd + 相关配置文件（inetd.conf/syslog.conf/inet.conf）
- - 协议实现归属决策（17-stage-net 或库层）

## 边界

- - **前置依赖**: 18
- - **不覆盖（移交）**: 各守护进程协议实现（17-stage-net 或库层）

