# 05-shell 家族与环境

> **状态**: pending（最小骨架，待改写）
> **定位**: shell 家族与环境
> **源码**: `minix3/bin/sh/（ash）、bin/ksh/、bin/csh/、bin/{hostname,domainname}/、usr.bin/{env,printenv,getopt,uname,machine,pagesize}/、minix/commands/sysenv/、etc/{profile,shrc,csh.cshrc,csh.login,csh.logout,hostname.file}`
> **Rust 模块**: `os/commands/bin/sh`
> **draft 素材**: 无（新建）

## 核心点

- - sh（ash）主线：解析器/内建/重定向/管道/作业控制语义；ksh/csh 差异面
- - shell 启动文件：profile/shrc/csh.* 的执行时序（登录/交互/非交互）
- - 环境命令面：env/printenv/sysenv/getopt；hostname/domainname/uname/machine/pagesize
- - [ARCH] A-1 关联：静态链接对 shell 内建（printf/test/echo 等）的约束
- - [ARCH] A-6 关联：rc 脚本面依赖 shell 就绪

## 边界

- - **前置依赖**: 03（登录后入口）、13（termios）
- - **不覆盖（移交）**: 命令工具本体（06~24）、termios 细节（13）

