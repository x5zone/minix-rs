# 06-mib-copy-io: 拷贝与长度原语

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 拷贝原语（全部 handler 的前置）
> **源码**: `main.c:64-258`、`tree.c:361-421`
> **Rust 模块**: `io/copy.rs`、`io/relay.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_oldp`/`mib_newp` 不透明体（endpt/addr/len）：仅 handler 通过原语访问
- 读侧：`mib_inrange`（off<len 测试）/`mib_getoldlen`/`mib_copyout`（部分拷贝，`sys_datacopy(SELF→caller)`，返回请求长度）
- 写侧：`mib_getnewlen`/`mib_copyin`（长度必须精确匹配，否则 EINVAL）/`mib_copyin_aux`（按前次拷入的地址二次拷贝）
- `mib_copyin_str`（tree.c）：字符串长度计算/拷贝 + NUL 边界检查，scratch 整页缓冲
- relay：`mib_relay_oldp/newp` → `cpf_grant_magic`（CPF_WRITE/READ），`GRANT_INVALID` → EINVAL（远程调用专用）
- `mib_setoldlen`：错误路径携带 oldlen（`call_reslen`）
- A-12：`sys_datacopy` vs grant 两种传输模型

## 边界

- **前置依赖**: 02
- **不覆盖（移交）**: 消息解码（01）、远程 relay 调用面（12）
