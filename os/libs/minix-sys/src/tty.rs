//! TTY 控制面客户端:`TTY_FKEY_CONTROL`(`_taskcall` 同步,无 grant)。
//!
//! C: libsys `fkey_ctl`(`minix3/minix/lib/libsys/fkey_ctl.c:11-28`)。
//! 消费者是 IS 的 fkey 契约面(`08-stage-is/02-is-fkey-contract.md`);
//! `_taskcall(TTY, ...)` 的形状与 rs 的 `sendrec` 任务调用写法同源。

use minix_types::{Endpoint, Message, TTY_FKEY_CONTROL};

/// `TIOCSCTTY` — claim the terminal as controlling tty
/// (`sys/sys/ttycom.h:136`, `_IO('t', 97)`): MINIX `ioccom` packs it as
/// `IOC_VOID (0x20000000) | ('t' << 8) | 97`
/// (`sys/sys/ioccom.h:72/65`). VFS intercepts this request at the char
/// device layer to authorize the claim
/// (`minix3/minix/servers/vfs/cdev.c:296-303`); the tty driver answers it
/// (`minix3/minix/drivers/tty/tty/tty.c:701`). C `login_tty` issues it
/// with a NULL argument (`minix3/lib/libutil/login_tty.c:56`).
pub const TIOCSCTTY: u64 = 0x2000_7461;

/// C: `fkey_ctl(req, *fkeys, *sfkeys)` — fkey_ctl.c:11-28。
///
/// `_taskcall(TTY, TTY_FKEY_CONTROL)`:请求臂
/// `m_lsys_tty_fkey_ctl {request, fkeys, sfkeys}`(ipc.h:1447-1454),
/// TTY 就地覆写回复臂 `m_tty_lsys_fkey_ctl {fkeys, sfkeys}`
/// (ipc.h:1925-1931)。返回 `(status, leftover_fkeys, leftover_sfkeys)`:
/// `status` 是 taskcall 的返回值(C 的 `r = _taskcall(...)`),leftover
/// 是 TTY 未消费的位(C 回写,fkey_ctl.c:26-27)。传输失败 →
/// `(EIO, 入参原样)`。
pub fn fkey_ctl_via(
    ipc: &impl crate::ipc::IpcTransport,
    request: i32,
    fkeys: u32,
    sfkeys: u32,
) -> (i32, u32, u32) {
    let mut msg = Message {
        m_type: TTY_FKEY_CONTROL,
        ..Default::default()
    };
    {
        // SAFETY: m_lsys_tty_fkey_ctl 是 TTY_FKEY_CONTROL 的文档化请求
        // 载荷(request@0/fkeys@4/sfkeys@8,C ipc.h:1447-1454)。
        let m = unsafe { &mut msg.m_u.m_lsys_tty_fkey_ctl };
        m.request = request;
        m.fkeys = fkeys as i32;
        m.sfkeys = sfkeys as i32;
    }
    match ipc.sendrec(Endpoint::TTY, &mut msg) {
        Ok(()) => {
            // SAFETY: TTY 以 m_tty_lsys_fkey_ctl 就地覆写回复臂
            // (fkey_ctl.c:26-27 的 *fkeys/*sfkeys 回写)。
            let r = unsafe { &msg.m_u.m_tty_lsys_fkey_ctl };
            (msg.m_type, r.fkeys as u32, r.sfkeys as u32)
        }
        Err(_) => (minix_types::EIO, fkeys, sfkeys),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::CannedTransport;
    use minix_types::{FKEY_MAP, OK};

    #[test]
    fn test_fkey_ctl_via_roundtrip() {
        // 请求:TTY_FKEY_CONTROL + m_lsys_tty_fkey_ctl{request,fkeys,sfkeys};
        // 回复:m_type=OK 且 m_tty_lsys_fkey_ctl 带未消费位。
        let mut ipc = CannedTransport::new();
        let mut reply = Message {
            m_type: OK,
            ..Default::default()
        };
        {
            // SAFETY: 测试构造——按回复臂域序写。
            let r = unsafe { &mut reply.m_u.m_tty_lsys_fkey_ctl };
            r.fkeys = 0x2;
            r.sfkeys = 0x1;
        }
        ipc.sendrec_replies.push(Ok(reply));

        let (status, fk, sfk) = fkey_ctl_via(&ipc, FKEY_MAP, 0x3, 0x1);
        assert_eq!(status, OK);
        assert_eq!((fk, sfk), (0x2, 0x1));
        // 发出请求的形状:dst=TTY,m_type 与三字段。
        let sent = ipc.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, Endpoint::TTY);
        assert_eq!(sent[0].1.m_type, TTY_FKEY_CONTROL);
        // SAFETY: 断言侧按请求臂域序读。
        let req = unsafe { &sent[0].1.m_u.m_lsys_tty_fkey_ctl };
        assert_eq!((req.request, req.fkeys, req.sfkeys), (FKEY_MAP, 0x3, 0x1));
    }

    #[test]
    fn test_fkey_ctl_via_transport_failure_is_eio() {
        // 传输失败 → (EIO, 入参原样)(C 的 r 传播面)。
        let mut ipc = CannedTransport::new();
        ipc.sendrec_replies.push(Err(crate::ipc::TrapStatus(-5)));
        let (status, fk, sfk) = fkey_ctl_via(&ipc, FKEY_MAP, 0x3, 0x1);
        assert_eq!(status, minix_types::EIO);
        assert_eq!((fk, sfk), (0x3, 0x1));
    }
}
