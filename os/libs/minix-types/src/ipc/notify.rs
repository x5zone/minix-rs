//! Notification type definitions.
//!
//! Corresponds to Minix3's notification types used in IPC.

use crate::ipc::message::MESSAGE_PAYLOAD_SIZE;

/// Base of the notification band in `m_type` space.
///
/// C: `NOTIFY_MESSAGE` — `minix3/minix/include/minix/com.h:90`. The band
/// `[NOTIFY_MESSAGE, NOTIFY_MESSAGE + 0x100)` sits just above the
/// call-number space; C's `is_notify(a)` (`(unsigned)((a) -
/// NOTIFY_MESSAGE) < 0x100`, com.h:93) tests membership with an unsigned
/// wrap — consumers must keep that unsignedness (a signed compare reads
/// every call number below the band as a notification).
pub const NOTIFY_MESSAGE: i32 = 0x1000;

/// Notification type.
///
/// Identifies the kind of asynchronous notification sent to a process.
/// Corresponds to Minix3's notify message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NotifyType {
    /// Hardware interrupt.
    HardInt = 1,
    /// Clock tick.
    ClockTick = 2,
    /// System event.
    SysEvent = 3,
}

/// Number of 32-bit words in a C `sigset_t`.
///
/// C: `__uint32_t __bits[4]` — `sigtypes.h:61`.
const SIGSET_WORDS: usize = 4;

/// Notification payload's signal bitmap — the C `sigset_t` shape.
///
/// C: `sigset_t` — `minix3/sys/sys/sigtypes.h:60-61`
/// (`__uint32_t __bits[4]`, 16 bytes). Bit `signo - 1` is signal `signo`,
/// spread over words exactly like C's `__sigword`/`__sigmask`
/// (`sigtypes.h:67-68`): the kernel signal family (71..=74) therefore lives
/// in `bits[2]`, which is why this field is 128 bits wide and not 64 — the
/// 64-bit form could never carry a kernel signal at all.
pub type SigSetBits = [u32; SIGSET_WORDS];

/// Reports whether a signal is a member of the bitmap.
///
/// C: `sigismember(&set, signo)` — `signal.h:108` forwarding to
/// `__sigismember` (`sigtypes.h:71`, word `(signo-1)>>5`, bit `(signo-1)&31`).
///
/// Unlike C, an out-of-range number answers `false` instead of indexing past
/// the array: the C macros are unchecked because every caller passes a
/// signal it has already range-tested, and a notification is untrusted input
/// here (a kernel we do not run could fill any byte).
pub const fn sigset_contains(set: SigSetBits, signo: u32) -> bool {
    if signo == 0 {
        return false;
    }
    let index = (signo - 1) as usize;
    let word = index / 32;
    if word >= SIGSET_WORDS {
        return false;
    }
    set[word] & (1u32 << (index % 32)) != 0
}

/// Builds the bitmap from a 64-bit set — the kernel producer's bridge.
///
/// The kernel still keeps pending signals in a 64-bit `SigSet` (its own
/// documented limitation), so a `signo` above 64 — the whole kernel-signal
/// family 71..=74 — cannot be raised yet: it lands in `bits[0..1]` only.
/// Widening that type to 128 bits is the tracked cross-layer change; this
/// shape already has room for it, so only the producer has to move.
pub const fn sigset_from_u64(low: u64) -> SigSetBits {
    [low as u32, (low >> 32) as u32, 0, 0]
}

/// Reads back the low 64 bits of a [`SigSetBits`] — the bridge for hooks
/// that still take a `u64` bitmap (see `minix-fs-rt`'s `SignalDecision`).
/// Kernel signals 71..=74 live above this window; walk them with
/// [`sigset_contains`] instead of truncating.
pub const fn sigset_to_u64(set: SigSetBits) -> u64 {
    (set[0] as u64) | ((set[1] as u64) << 32)
}

/// Notification message payload.
///
/// C: `mess_notify` — `minix3/minix/include/minix/ipc.h:1714-1719`
///
/// Filled by `BuildNotifyMessage` (proc.c:98-114) when a notification
/// is delivered synchronously (dst was in RECEIVE). The `m_type` field
/// of the enclosing `Message` is set to `NOTIFY_MESSAGE`.
///
/// # Field semantics by source
///
/// | Source | `timestamp` | `interrupts` | `sigset` |
/// |--------|-------------|--------------|----------|
/// | HARDWARE | ✅ get_monotonic() | ✅ `s_int_pending` (then cleared) | zero |
/// | SYSTEM | ✅ get_monotonic() | zero | ✅ `s_sig_pending` (then cleared) |
/// | Process | ✅ get_monotonic() | zero | zero |
///
/// # Layout
///
/// Total size = 56 bytes (= `MESSAGE_PAYLOAD_SIZE`), matching the C
/// `mess_notify` union member field for field: `timestamp` @0, `interrupts`
/// @8, `sigset` @16 (16 bytes, [`SigSetBits`]), padding @32
/// (`minix3/minix/include/minix/ipc.h:1714-1719`).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MessNotify {
    /// Monotonic timestamp at notification time.
    /// C: `m_notify.timestamp = get_monotonic()`
    pub timestamp: u64,
    /// Pending hardware interrupt bitmap.
    /// Valid only when source == HARDWARE; copied from `priv(dst)->s_int_pending` then cleared.
    /// C: `m_notify.interrupts = priv(dst_ptr)->s_int_pending`
    pub interrupts: u64,
    /// Pending signal bitmap.
    /// Valid only when source == SYSTEM; copied from `priv(dst)->s_sig_pending` then cleared.
    /// C: `m_notify.sigset` (`sigset_t`, 16 bytes — `sigtypes.h:60-61`)
    pub sigset: SigSetBits,
    /// Padding to fill `MESSAGE_PAYLOAD_SIZE` (56 bytes total).
    /// C: `uint8_t padding[24]` — `ipc.h:1718`.
    _padding: [u8; 24],
}

impl MessNotify {
    /// Create a zeroed notification payload.
    pub const fn zeroed() -> Self {
        Self {
            timestamp: 0,
            interrupts: 0,
            sigset: [0; SIGSET_WORDS],
            _padding: [0u8; 24],
        }
    }

    /// Create a notification payload with the given field values.
    ///
    /// Used by `build_notify_message` to construct a `MessNotify` from
    /// the source-specific fields (timestamp / interrupts / sigset).
    /// `_padding` is zeroed and kept private — callers cannot set it.
    pub const fn new(timestamp: u64, interrupts: u64, sigset: SigSetBits) -> Self {
        Self {
            timestamp,
            interrupts,
            sigset,
            _padding: [0u8; 24],
        }
    }
}

impl Default for MessNotify {
    fn default() -> Self {
        Self::zeroed()
    }
}

// Compile-time size assertion: MessNotify must fit in the message payload.
const _: () = assert!(
    core::mem::size_of::<MessNotify>() <= MESSAGE_PAYLOAD_SIZE,
    "MessNotify exceeds MESSAGE_PAYLOAD_SIZE"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KERNEL_SIGNAL_FIRST, KERNEL_SIGNAL_LAST};
    use core::mem::{offset_of, size_of};

    /// C 绝对值 pin:通知带基址(com.h:90)。is_notify 的无符号回绕判定
    /// 依赖此值,漂移即把整段调用号误判为通知。
    #[test]
    fn test_notify_message_band_base_matches_c() {
        assert_eq!(NOTIFY_MESSAGE, 0x1000); // com.h:90
    }

    /// 布局见证:`mess_notify` 逐字节对位 ipc.h:1714-1719 —— 总长 56,
    /// `sigset` 落在偏移 16 并占满 16 字节(C `sigset_t`),剩余 24 字节是
    /// C 自己的 padding。字段形状若漂回 8 字节,内核信号族(71..=74,
    /// 位于 `bits[2]`)就没有落脚点。
    #[test]
    fn test_mess_notify_layout_matches_c() {
        assert_eq!(size_of::<MessNotify>(), MESSAGE_PAYLOAD_SIZE);
        assert_eq!(size_of::<MessNotify>(), 56);
        assert_eq!(offset_of!(MessNotify, timestamp), 0); // ipc.h:1715
        assert_eq!(offset_of!(MessNotify, interrupts), 8); // ipc.h:1716
        assert_eq!(offset_of!(MessNotify, sigset), 16); // ipc.h:1717
        assert_eq!(size_of::<SigSetBits>(), 16); // sigtypes.h:60-61
        assert_eq!(offset_of!(MessNotify, _padding), 32); // ipc.h:1718
    }

    /// 位基与 C `__sigmask/__sigword` 同形:bit(signo-1),按 32 位分字。
    /// 内核信号族的四个字(71..=74)全落在 `bits[2]`,即 u64 形态装不下的
    /// 那一段;越界的号码一律答 `false`,不索引数组之外。
    #[test]
    fn test_sigset_contains_matches_c_bit_numbering() {
        let mut set = [0u32; SIGSET_WORDS];
        set[0] = 1 << 0; // signo 1
        set[0] = set[0] | (1 << 31); // signo 32
        set[1] = 1 << 31; // signo 64
        set[2] = (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9); // 71..=74
        assert!(sigset_contains(set, 1));
        assert!(sigset_contains(set, 32));
        assert!(sigset_contains(set, 64));
        for signo in KERNEL_SIGNAL_FIRST as u32..=KERNEL_SIGNAL_LAST as u32 {
            assert!(
                sigset_contains(set, signo),
                "kernel signal {signo} must be visible"
            );
        }
        assert!(!sigset_contains(set, 2));
        assert!(!sigset_contains(set, 65)); // bits[2] 的 bit 0 未置
        assert!(!sigset_contains(set, 70)); // SIGSNDELAY 不在此测试集
        assert!(!sigset_contains(set, 75)); // 族外:bits[2] bit 10 未置
        assert!(!sigset_contains(set, 0)); // signo 0 不是信号
        assert!(!sigset_contains([u32::MAX; SIGSET_WORDS], 129)); // 越界不 panic
    }

    /// 与内核 64 位 `SigSet` 的桥:低 64 位往返无损,高 64 位在生产者拓宽
    /// 之前恒零。
    #[test]
    fn test_sigset_u64_bridge_round_trips_low_half() {
        let low = 0x0000_0001_8000_0001u64; // 置位的 bit: 0 / 31 / 32
        let set = sigset_from_u64(low);
        assert_eq!(sigset_to_u64(set), low);
        assert_eq!(set[2], 0); // u64 表达不了 bits[2..]
        assert!(sigset_contains(set, 1)); // bit 0
        assert!(sigset_contains(set, 32)); // bit 31
        assert!(sigset_contains(set, 33)); // bit 32 → bits[1] bit 0
        assert!(!sigset_contains(set, 71)); // 内核信号需内核先拓宽 SigSet
    }
}
