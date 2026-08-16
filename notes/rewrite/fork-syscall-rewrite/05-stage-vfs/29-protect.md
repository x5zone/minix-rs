# 29-protect: 权限检查

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 13 — 目录/链接/权限
> **源码**: `protect.c` 全文件、`utility.c:128-141`（in_group）
> **Rust 模块**: （未实现）protect 模块
> **draft 素材**: 无（新建）

## 核心点

- do_chmod/do_chown：模式与属主修改
- do_umask：掩码设置（fp_umask）
- do_access：访问测试（R_OK/W_OK/X_OK）
- forbidden/in_group：权限判定核心（uid/gid/补组）
- R_BIT/W_BIT/X_BIT 位语义

## 边界

- 凭证字段设置不覆盖（10 pm_setuid 族）
- 凭证结构定义不覆盖（02）
