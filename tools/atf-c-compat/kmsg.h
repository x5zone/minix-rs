/* kmsg.h — NK4-C 目标③ 兼容层的 IPC Message 手写镜像（单点定义）。
 *
 * 布局对位 minix-types ipc/message.rs：m_source i32@0、m_type i32@4、
 * m_u 载荷 @8（56B = 7 个 8 字节槽）。sys-bridge.c（trap 腿）与
 * posix-stubs.c（VFS 族）共用本头——CodeReview 续-277b P2-2：两处手抄
 * struct 今天逐字节一致，但 C 链接层不做跨 TU 类型检查，一侧改槽序
 * 即静默错位；收敛到单点头使两侧永远同源。
 *
 * 仅 ③ 构建/上机用；不参与生产镜像。 */
#ifndef MINIX_RS_ATF_KMSG_H
#define MINIX_RS_ATF_KMSG_H

struct kmsg {
    int m_source;
    int m_type;
    long long slots[7]; /* m_u：消息字节 8 起，与 minix-types Message 同形 */
};

#endif /* MINIX_RS_ATF_KMSG_H */
