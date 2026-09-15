//! Kernel-call number family — the single Rust authority for C's
//! `KERNEL_CALL` vector (`minix3/minix/include/minix/com.h:204-267`).
//!
//! Every `SYS_*` consumer (minix-sys wrappers, server transport mirrors,
//! kernel dispatch linkage) imports from here instead of hand-copying
//! `0x600 + n`. The numbers decide which entry of the kernel's call
//! vector fires, so a hand-copy drift is a wrong-kernel-call bug —
//! `E-MINTYPES-SYS` registered exactly that pattern in sched's local
//! mirror. Redox/Linux analogy: one uapi header, every consumer imports.
//!
//! Coverage: the full C family including gaps (call numbers 11/12, 20,
//! 29/30, 37/38, 41/42, 47-49 are unallocated in C and stay undefined
//! here — a consumer needing them is a ground-truth problem, not a
//! constant away). `SYS_BASIC_CALLS` moved here from `ipc/sysinfo.rs`
//! (it is a member selection of this family, not a sysinfo vocabulary).

/// Base for kernel calls to SYSTEM. C: `KERNEL_CALL` — com.h:205.
pub const KERNEL_CALL: i32 = 0x600;

/// C: `SYS_FORK (KERNEL_CALL + 0)` — com.h:208.
pub const SYS_FORK: i32 = KERNEL_CALL;

/// C: `SYS_EXEC (KERNEL_CALL + 1)` — com.h:209.
pub const SYS_EXEC: i32 = KERNEL_CALL + 1;

/// C: `SYS_CLEAR (KERNEL_CALL + 2)` — com.h:210.
pub const SYS_CLEAR: i32 = KERNEL_CALL + 2;

/// C: `SYS_SCHEDULE (KERNEL_CALL + 3)` — com.h:211.
pub const SYS_SCHEDULE: i32 = KERNEL_CALL + 3;

/// C: `SYS_PRIVCTL (KERNEL_CALL + 4)` — com.h:212.
pub const SYS_PRIVCTL: i32 = KERNEL_CALL + 4;

/// C: `SYS_TRACE (KERNEL_CALL + 5)` — com.h:213.
pub const SYS_TRACE: i32 = KERNEL_CALL + 5;

/// C: `SYS_KILL (KERNEL_CALL + 6)` — com.h:214.
pub const SYS_KILL: i32 = KERNEL_CALL + 6;

/// C: `SYS_GETKSIG (KERNEL_CALL + 7)` — com.h:216.
pub const SYS_GETKSIG: i32 = KERNEL_CALL + 7;

/// C: `SYS_ENDKSIG (KERNEL_CALL + 8)` — com.h:217.
pub const SYS_ENDKSIG: i32 = KERNEL_CALL + 8;

/// C: `SYS_SIGSEND (KERNEL_CALL + 9)` — com.h:218.
pub const SYS_SIGSEND: i32 = KERNEL_CALL + 9;

/// C: `SYS_SIGRETURN (KERNEL_CALL + 10)` — com.h:219.
pub const SYS_SIGRETURN: i32 = KERNEL_CALL + 10;

/// C: `SYS_MEMSET (KERNEL_CALL + 13)` — com.h:221.
pub const SYS_MEMSET: i32 = KERNEL_CALL + 13;

/// C: `SYS_UMAP (KERNEL_CALL + 14)` — com.h:223.
pub const SYS_UMAP: i32 = KERNEL_CALL + 14;

/// C: `SYS_VIRCOPY (KERNEL_CALL + 15)` — com.h:224.
pub const SYS_VIRCOPY: i32 = KERNEL_CALL + 15;

/// C: `SYS_PHYSCOPY (KERNEL_CALL + 16)` — com.h:225.
pub const SYS_PHYSCOPY: i32 = KERNEL_CALL + 16;

/// C: `SYS_UMAP_REMOTE (KERNEL_CALL + 17)` — com.h:226.
pub const SYS_UMAP_REMOTE: i32 = KERNEL_CALL + 17;

/// C: `SYS_VUMAP (KERNEL_CALL + 18)` — com.h:227.
pub const SYS_VUMAP: i32 = KERNEL_CALL + 18;

/// C: `SYS_IRQCTL (KERNEL_CALL + 19)` — com.h:229.
pub const SYS_IRQCTL: i32 = KERNEL_CALL + 19;

/// C: `SYS_DEVIO (KERNEL_CALL + 21)` — com.h:231.
pub const SYS_DEVIO: i32 = KERNEL_CALL + 21;

/// C: `SYS_SDEVIO (KERNEL_CALL + 22)` — com.h:232.
pub const SYS_SDEVIO: i32 = KERNEL_CALL + 22;

/// C: `SYS_VDEVIO (KERNEL_CALL + 23)` — com.h:233.
pub const SYS_VDEVIO: i32 = KERNEL_CALL + 23;

/// C: `SYS_SETALARM (KERNEL_CALL + 24)` — com.h:234.
pub const SYS_SETALARM: i32 = KERNEL_CALL + 24;

/// C: `SYS_TIMES (KERNEL_CALL + 25)` — com.h:235.
pub const SYS_TIMES: i32 = KERNEL_CALL + 25;

/// C: `SYS_GETINFO (KERNEL_CALL + 26)` — com.h:236.
pub const SYS_GETINFO: i32 = KERNEL_CALL + 26;

/// C: `SYS_ABORT (KERNEL_CALL + 27)` — com.h:237.
pub const SYS_ABORT: i32 = KERNEL_CALL + 27;

/// C: `SYS_IOPENABLE (KERNEL_CALL + 28)` — com.h:238.
pub const SYS_IOPENABLE: i32 = KERNEL_CALL + 28;

/// C: `SYS_SAFECOPYFROM (KERNEL_CALL + 31)` — com.h:239.
pub const SYS_SAFECOPYFROM: i32 = KERNEL_CALL + 31;

/// C: `SYS_SAFECOPYTO (KERNEL_CALL + 32)` — com.h:240.
pub const SYS_SAFECOPYTO: i32 = KERNEL_CALL + 32;

/// C: `SYS_VSAFECOPY (KERNEL_CALL + 33)` — com.h:241.
pub const SYS_VSAFECOPY: i32 = KERNEL_CALL + 33;

/// C: `SYS_SETGRANT (KERNEL_CALL + 34)` — com.h:242.
pub const SYS_SETGRANT: i32 = KERNEL_CALL + 34;

/// C: `SYS_READBIOS (KERNEL_CALL + 35)` — com.h:243.
pub const SYS_READBIOS: i32 = KERNEL_CALL + 35;

/// C: `SYS_SPROF (KERNEL_CALL + 36)` — com.h:245.
pub const SYS_SPROF: i32 = KERNEL_CALL + 36;

/// C: `SYS_STIME (KERNEL_CALL + 39)` — com.h:247.
pub const SYS_STIME: i32 = KERNEL_CALL + 39;

/// C: `SYS_SETTIME (KERNEL_CALL + 40)` — com.h:248.
pub const SYS_SETTIME: i32 = KERNEL_CALL + 40;

/// C: `SYS_VMCTL (KERNEL_CALL + 43)` — com.h:250.
pub const SYS_VMCTL: i32 = KERNEL_CALL + 43;

/// C: `SYS_DIAGCTL (KERNEL_CALL + 44)` — com.h:252.
pub const SYS_DIAGCTL: i32 = KERNEL_CALL + 44;

/// C: `SYS_VTIMER (KERNEL_CALL + 45)` — com.h:254.
pub const SYS_VTIMER: i32 = KERNEL_CALL + 45;

/// C: `SYS_RUNCTL (KERNEL_CALL + 46)` — com.h:255.
pub const SYS_RUNCTL: i32 = KERNEL_CALL + 46;

/// C: `SYS_GETMCONTEXT (KERNEL_CALL + 50)` — com.h:256.
pub const SYS_GETMCONTEXT: i32 = KERNEL_CALL + 50;

/// C: `SYS_SETMCONTEXT (KERNEL_CALL + 51)` — com.h:257.
pub const SYS_SETMCONTEXT: i32 = KERNEL_CALL + 51;

/// C: `SYS_UPDATE (KERNEL_CALL + 52)` — com.h:258.
pub const SYS_UPDATE: i32 = KERNEL_CALL + 52;

/// C: `SYS_EXIT (KERNEL_CALL + 53)` — com.h:259.
pub const SYS_EXIT: i32 = KERNEL_CALL + 53;

/// C: `SYS_SCHEDCTL (KERNEL_CALL + 54)` — com.h:262.
pub const SYS_SCHEDCTL: i32 = KERNEL_CALL + 54;

/// C: `SYS_STATECTL (KERNEL_CALL + 55)` — com.h:263.
pub const SYS_STATECTL: i32 = KERNEL_CALL + 55;

/// C: `SYS_SAFEMEMSET (KERNEL_CALL + 56)` — com.h:265.
pub const SYS_SAFEMEMSET: i32 = KERNEL_CALL + 56;

/// C: `SYS_PADCONF (KERNEL_CALL + 57)` — com.h:267.
pub const SYS_PADCONF: i32 = KERNEL_CALL + 57;

/// Total kernel-call vector size. C: `NR_SYS_CALLS` — com.h:269.
pub const NR_SYS_CALLS: i32 = 58;

/// The basic kernel-call set every system service gets on request.
/// C: `SYS_BASIC_CALLS` — com.h:275-278 (NULL_C terminator added by the
/// consumer, mirroring `int basic_kc[] = {SYS_BASIC_CALLS, NULL_C}`,
/// manager.c:1470).
pub const SYS_BASIC_CALLS: [i32; 11] = [
    SYS_EXIT,
    SYS_SAFECOPYFROM,
    SYS_SAFECOPYTO,
    SYS_VSAFECOPY,
    SYS_GETINFO,
    SYS_TIMES,
    SYS_SETALARM,
    SYS_SETGRANT,
    SYS_DIAGCTL,
    SYS_STATECTL,
    SYS_SAFEMEMSET,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// C 绝对值 pin:抽钉族内全部非连续边界 + 每个连续段的首尾,防手抄
    /// 漂移(E-MINTYPES-SYS 登记的事故模式)。断言值为 com.h:208-269 的
    /// 字面偏移,不是本模块常量的自证。
    #[test]
    fn test_kernel_call_numbers_match_c_com_h() {
        assert_eq!(KERNEL_CALL, 0x600); // com.h:205
        assert_eq!(SYS_FORK, 0x600); // com.h:208
        assert_eq!(SYS_EXEC, 0x601); // com.h:209
        assert_eq!(SYS_CLEAR, 0x602); // com.h:210
        assert_eq!(SYS_SCHEDULE, 0x603); // com.h:211
        assert_eq!(SYS_PRIVCTL, 0x604); // com.h:212
        assert_eq!(SYS_TRACE, 0x605); // com.h:213
        assert_eq!(SYS_KILL, 0x606); // com.h:214
        assert_eq!(SYS_GETKSIG, 0x607); // com.h:216
        assert_eq!(SYS_ENDKSIG, 0x608); // com.h:217
        assert_eq!(SYS_SIGSEND, 0x609); // com.h:218
        assert_eq!(SYS_SIGRETURN, 0x60a); // com.h:219
        assert_eq!(SYS_MEMSET, 0x60d); // com.h:221
        assert_eq!(SYS_UMAP, 0x60e); // com.h:223
        assert_eq!(SYS_VIRCOPY, 0x60f); // com.h:224
        assert_eq!(SYS_PHYSCOPY, 0x610); // com.h:225
        assert_eq!(SYS_UMAP_REMOTE, 0x611); // com.h:226
        assert_eq!(SYS_VUMAP, 0x612); // com.h:227
        assert_eq!(SYS_IRQCTL, 0x613); // com.h:229
        assert_eq!(SYS_DEVIO, 0x615); // com.h:231
        assert_eq!(SYS_SDEVIO, 0x616); // com.h:232
        assert_eq!(SYS_VDEVIO, 0x617); // com.h:233
        assert_eq!(SYS_SETALARM, 0x618); // com.h:234
        assert_eq!(SYS_TIMES, 0x619); // com.h:235
        assert_eq!(SYS_GETINFO, 0x61a); // com.h:236
        assert_eq!(SYS_ABORT, 0x61b); // com.h:237
        assert_eq!(SYS_IOPENABLE, 0x61c); // com.h:238
        assert_eq!(SYS_SAFECOPYFROM, 0x61f); // com.h:239
        assert_eq!(SYS_SAFECOPYTO, 0x620); // com.h:240
        assert_eq!(SYS_VSAFECOPY, 0x621); // com.h:241
        assert_eq!(SYS_SETGRANT, 0x622); // com.h:242
        assert_eq!(SYS_READBIOS, 0x623); // com.h:243
        assert_eq!(SYS_SPROF, 0x624); // com.h:245
        assert_eq!(SYS_STIME, 0x627); // com.h:247
        assert_eq!(SYS_SETTIME, 0x628); // com.h:248
        assert_eq!(SYS_VMCTL, 0x62b); // com.h:250
        assert_eq!(SYS_DIAGCTL, 0x62c); // com.h:252
        assert_eq!(SYS_VTIMER, 0x62d); // com.h:254
        assert_eq!(SYS_RUNCTL, 0x62e); // com.h:255
        assert_eq!(SYS_GETMCONTEXT, 0x632); // com.h:256
        assert_eq!(SYS_SETMCONTEXT, 0x633); // com.h:257
        assert_eq!(SYS_UPDATE, 0x634); // com.h:258
        assert_eq!(SYS_EXIT, 0x635); // com.h:259
        assert_eq!(SYS_SCHEDCTL, 0x636); // com.h:262
        assert_eq!(SYS_STATECTL, 0x637); // com.h:263
        assert_eq!(SYS_SAFEMEMSET, 0x638); // com.h:265
        assert_eq!(SYS_PADCONF, 0x639); // com.h:267
        assert_eq!(NR_SYS_CALLS, 58); // com.h:269
    }

    /// 偏移合法性:全族落在 `KERNEL_CALL..KERNEL_CALL + NR_SYS_CALLS` 内,
    /// 且族内无重复号(同一号两个名字 = 分派歧义)。
    #[test]
    fn test_kernel_call_family_in_range_and_unique() {
        let family = [
            SYS_FORK, SYS_EXEC, SYS_CLEAR, SYS_SCHEDULE, SYS_PRIVCTL, SYS_TRACE, SYS_KILL,
            SYS_GETKSIG, SYS_ENDKSIG, SYS_SIGSEND, SYS_SIGRETURN, SYS_MEMSET, SYS_UMAP,
            SYS_VIRCOPY, SYS_PHYSCOPY, SYS_UMAP_REMOTE, SYS_VUMAP, SYS_IRQCTL, SYS_DEVIO,
            SYS_SDEVIO, SYS_VDEVIO, SYS_SETALARM, SYS_TIMES, SYS_GETINFO, SYS_ABORT,
            SYS_IOPENABLE, SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY, SYS_SETGRANT,
            SYS_READBIOS, SYS_SPROF, SYS_STIME, SYS_SETTIME, SYS_VMCTL, SYS_DIAGCTL,
            SYS_VTIMER, SYS_RUNCTL, SYS_GETMCONTEXT, SYS_SETMCONTEXT, SYS_UPDATE, SYS_EXIT,
            SYS_SCHEDCTL, SYS_STATECTL, SYS_SAFEMEMSET, SYS_PADCONF,
        ];
        for v in family {
            assert!(
                v >= KERNEL_CALL && v < KERNEL_CALL + NR_SYS_CALLS,
                "{v:#x} 越出内核调用向量"
            );
        }
        let mut sorted = family;
        sorted.sort_unstable();
        for w in sorted.windows(2) {
            assert_ne!(w[0], w[1], "族内重复调用号 {:#x}", w[0]);
        }
    }

    /// `SYS_BASIC_CALLS` 与 C com.h:275-278 的字面顺序一致(RS 的
    /// edit_slot 路径按此顺序下发,顺序即契约)。
    #[test]
    fn test_basic_calls_order_matches_c() {
        assert_eq!(SYS_BASIC_CALLS[0], SYS_EXIT);
        assert_eq!(SYS_BASIC_CALLS[1], SYS_SAFECOPYFROM);
        assert_eq!(SYS_BASIC_CALLS[2], SYS_SAFECOPYTO);
        assert_eq!(SYS_BASIC_CALLS[3], SYS_VSAFECOPY);
        assert_eq!(SYS_BASIC_CALLS[4], SYS_GETINFO);
        assert_eq!(SYS_BASIC_CALLS[5], SYS_TIMES);
        assert_eq!(SYS_BASIC_CALLS[6], SYS_SETALARM);
        assert_eq!(SYS_BASIC_CALLS[7], SYS_SETGRANT);
        assert_eq!(SYS_BASIC_CALLS[8], SYS_DIAGCTL);
        assert_eq!(SYS_BASIC_CALLS[9], SYS_STATECTL);
        assert_eq!(SYS_BASIC_CALLS[10], SYS_SAFEMEMSET);
    }
}
