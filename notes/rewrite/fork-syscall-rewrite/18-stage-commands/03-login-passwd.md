# 03-登录链路与口令数据库

> **状态**: pending（最小骨架，待改写）
> **定位**: 登录链路与口令数据库
> **源码**: `minix3/libexec/getty/（3 .c）、usr.bin/login/（4 .c）、usr.bin/{passwd,chpass,su,newgrp,id,pwhash}/、usr.sbin/{pwd_mkdb,vipw,user}/、sbin/nologin/、etc/{gettytab,ttys,master.passwd,passwd.conf,skel}`
> **Rust 模块**: `新 crate（待建）`
> **draft 素材**: 无（新建）

## 核心点

- - getty：libexec/getty（init.c/main.c/subr.c）——ttys 每行派生、终端初始化、exec login
- - login：usr.bin/login 流程——口令验证 → 会话建立 → exec shell；gettytab 终端参数面
- - 口令数据库面：passwd/master.passwd 格式 + pwd_mkdb（Berkeley DB 面）+ vipw + user + chpass + pwhash
- - 权限命令：su/newgrp/nologin/id 的 setuid 语义（[ARCH] A-12，依赖 04-stage-pm）
- - 会话初始化：profile/skel 面

## 边界

- - **前置依赖**: 02、13（termios）、04-stage-pm（认证/权限）
- - **不覆盖（移交）**: 口令存储实现（PM）、shell 启动文件细节（05）

