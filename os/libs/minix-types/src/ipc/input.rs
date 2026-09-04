//! Input-server wire protocol: message numbers, payload helpers, device nodes.
//!
//! C: `minix3/minix/include/minix/com.h:872-893` (numbers) +
//! `minix3/minix/include/minix/ipc.h:232-259,993-1001,2434-2436,2517`
//! (payloads) + `minix3/minix/include/minix/dmap.h:78` (major number) +
//! `minix3/minix/commands/MAKEDEV/MAKEDEV.sh:330-343` (device nodes) +
//! `minix3/minix/include/sys/kbdio.h` (light bits) +
//! `minix3/sys/sys/ttycom.h:174` (light control number).
//!
//! The payload shapes live with the other `Mess*` types in
//! [`crate::MessInputLinputdriverInputConf`] /
//! [`crate::MessInputLinputdriverSetleds`] / [`crate::MessInputTtyEvent`] /
//! [`crate::MessLinputdriverInputEvent`] (`message.rs`); this module owns
//! the numbers, the node table, the light control encoding, and the
//! construct/decode helpers.
//!
//! `[ARCH: A-9]`: the input protocol has no replies (com.h:886 says so
//! outright). Every message below is fire-and-forget; [`needs_no_reply`]
//! names the set so dispatchers never wait for an answer that cannot come.
//!
//! Authority (§2.4g): the numbers below are the single definition in
//! minix-rs. The input server (`os/servers/input`) and its future drivers
//! import from here; do not redefine per-crate. (`TTY_RQ_BASE` itself lives
//! in [`crate::TTY_RQ_BASE`] — owned by `tty.rs`, referenced here.)

use crate::ipc::message::{
    MessInputLinputdriverInputConf, MessInputLinputdriverSetleds, MessInputTtyEvent,
    MessLinputdriverInputEvent,
};
use crate::{Endpoint, Message, MessageUnion, TTY_RQ_BASE};

// ── Message numbers ──

/// Input server is up (server → TTY). C: `TTY_INPUT_UP (TTY_RQ_BASE + 2)` —
/// com.h:879. The base itself is owned by `tty.rs`
/// ([`crate::TTY_RQ_BASE`]); only the offset arithmetic lives here.
pub const TTY_INPUT_UP: i32 = TTY_RQ_BASE + 2;

/// Relayed input event (server → TTY). C: `TTY_INPUT_EVENT
/// (TTY_RQ_BASE + 3)` — com.h:880.
pub const TTY_INPUT_EVENT: i32 = TTY_RQ_BASE + 3;

/// Input request base: TTY → server, or server → driver.
/// C: `INPUT_RQ_BASE 0x1500` — com.h:888.
pub const INPUT_RQ_BASE: i32 = 0x1500;

/// Input report base: driver → server. C: `INPUT_RS_BASE 0x1580` —
/// com.h:889.
pub const INPUT_RS_BASE: i32 = 0x1580;

/// Configure driver (server → driver). C: `INPUT_CONF (INPUT_RQ_BASE + 0)` —
/// com.h:891. The `+ 0` repeats the C spelling on purpose: it marks this
/// number as the first of the request sequence (see `INPUT_SETLEDS`).
#[allow(clippy::identity_op)]
pub const INPUT_CONF: i32 = INPUT_RQ_BASE + 0;

/// Set keyboard lights (server → driver). C: `INPUT_SETLEDS
/// (INPUT_RQ_BASE + 1)` — com.h:892.
pub const INPUT_SETLEDS: i32 = INPUT_RQ_BASE + 1;

/// Send input event (driver → server). C: `INPUT_EVENT (INPUT_RS_BASE + 0)`
/// — com.h:894. The `+ 0` repeats the C spelling (first of the report
/// sequence), see `INPUT_CONF`.
#[allow(clippy::identity_op)]
pub const INPUT_EVENT: i32 = INPUT_RS_BASE + 0;

/// Whether a message number belongs to the fire-and-forget input protocol.
///
/// C: "The input protocol has no real replies. All messages are one-way."
/// (com.h:886). Dispatchers use this to decide that no reply path exists —
/// waiting for one would block forever.
pub const fn needs_no_reply(message_type: i32) -> bool {
    matches!(
        message_type,
        TTY_INPUT_UP | TTY_INPUT_EVENT | INPUT_CONF | INPUT_SETLEDS | INPUT_EVENT
    )
}

// ── Device identity ──

/// Major number of all input device nodes. C: `INPUT_MAJOR 64` — dmap.h:78.
pub const INPUT_MAJOR: i32 = 64;

/// One `/dev` node: name, major, minor.
///
/// C: `MAKEDEV.sh:330-343` (`makedev kbdmux c 64 0`, `makedev kbd0 c 64 1`,
/// …, `makedev mousemux c 64 64`, `makedev mouse0 c 64 65`, …). The minor
/// numbers repeat the server's minor scheme (document 03) by construction:
/// both spellings describe the same ten devices, and
/// `nodes_match_minor_scheme` locks them together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceNode {
    /// Node name under `/dev` (without the directory).
    pub name: &'static str,
    /// Major number (always [`INPUT_MAJOR`]).
    pub major: i32,
    /// Minor number (the server's minor for this device).
    pub minor: i32,
}

/// The ten `/dev` nodes, in server slot order (multiplexer, keyboards,
/// multiplexer, mice). C: `MAKEDEV.sh:330-343`.
pub const INPUT_NODES: [DeviceNode; 10] = [
    DeviceNode {
        name: "kbdmux",
        major: INPUT_MAJOR,
        minor: 0,
    },
    DeviceNode {
        name: "kbd0",
        major: INPUT_MAJOR,
        minor: 1,
    },
    DeviceNode {
        name: "kbd1",
        major: INPUT_MAJOR,
        minor: 2,
    },
    DeviceNode {
        name: "kbd2",
        major: INPUT_MAJOR,
        minor: 3,
    },
    DeviceNode {
        name: "kbd3",
        major: INPUT_MAJOR,
        minor: 4,
    },
    DeviceNode {
        name: "mousemux",
        major: INPUT_MAJOR,
        minor: 64,
    },
    DeviceNode {
        name: "mouse0",
        major: INPUT_MAJOR,
        minor: 65,
    },
    DeviceNode {
        name: "mouse1",
        major: INPUT_MAJOR,
        minor: 66,
    },
    DeviceNode {
        name: "mouse2",
        major: INPUT_MAJOR,
        minor: 67,
    },
    DeviceNode {
        name: "mouse3",
        major: INPUT_MAJOR,
        minor: 68,
    },
];

// ── Device kinds and the unassigned marker ──

/// Driver device-type bit: keyboard. C: `INPUT_DEV_KBD 0x01` — input.h:9
/// (`_SYSTEM` only).
///
/// Authority (§2.4g): canonical shared definition. The input server crate
/// (`os/servers/input/src/event.rs`) restates the same value for
/// crate-local use; change this copy first, then sync that one.
pub const INPUT_DEV_KBD: u16 = 0x01;

/// Driver device-type bit: mouse. C: `INPUT_DEV_MOUSE 0x02` — input.h:10.
///
/// Same authority note as [`INPUT_DEV_KBD`].
pub const INPUT_DEV_MOUSE: u16 = 0x02;

/// No device assigned yet. C: `INVALID_INPUT_ID (-1)` — input.h:13.
///
/// Same authority note as [`INPUT_DEV_KBD`]: the server crate restates this
/// value locally; this copy leads.
pub const INVALID_INPUT_ID: i32 = -1;

/// Data-store key prefix for input-driver announcements.
///
/// C: `driver_prefix = "drv.inp."` (`inputdriver.c:23`, client side) matched
/// against the subscription `drv\.inp\..*` and filtered with
/// `strncmp(key, "drv.inp.", len)` (`input.c:562-579`, server side). Both
/// sides spell the same dots; this constant is the shared spelling.
/// (The sibling `"drv.chr."` belongs to the character framework, document
/// 02, and lives outside this protocol.)
pub const DRIVER_KEY_PREFIX: &str = "drv.inp.";

/// Whether a data-store key names an input driver, and if so, which.
///
/// Mirrors the server filter (`input.c:578-582`): the key must start with
/// [`DRIVER_KEY_PREFIX`]; the remainder is the driver's own label. Returns
/// the label part, unvalidated (the server verifies it against the sender
/// before trusting it — document 11).
pub fn driver_label(key: &str) -> Option<&str> {
    key.strip_prefix(DRIVER_KEY_PREFIX)
}

// ── Light control encoding (ioctl in, mask out) ──

/// Keyboard light bits as the caller passes them (`kio_leds_t.kl_bits`).
/// C: `KBD_LEDS_NUM/CAPS/SCROLL 0x1/0x2/0x4` — kbdio.h:22-24.
pub const KBD_LEDS_NUM: u32 = 0x1;
/// C: `KBD_LEDS_CAPS 0x2` — kbdio.h:23.
pub const KBD_LEDS_CAPS: u32 = 0x2;
/// C: `KBD_LEDS_SCROLL 0x4` — kbdio.h:24.
pub const KBD_LEDS_SCROLL: u32 = 0x4;

/// Caller-side light state. C: `struct kio_leds { unsigned kl_bits; }` —
/// kbdio.h:15-18 (`unsigned` is 32 bits on Minix3, so the struct is 4 bytes;
/// `led_bits_match_c` pins the size).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KioLeds {
    /// Light bits (`KBD_LEDS_*`).
    pub kl_bits: u32,
}

/// Set keyboard lights control number.
///
/// C: `KIOCSLEDS _IOW('k', 2, struct kio_leds)` — ttycom.h:174, with `_IOW`
/// from ioccom.h:87 (`IOC_IN | (sizeof << 16) | (group << 8) | num`). The
/// `const` expression below repeats that construction instead of hardcoding
/// the result, so a future change to `KioLeds` reshapes the number
/// automatically — and `led_control_number_match_c` still pins today's
/// value (`0x80046B02`).
pub const KIOCSLEDS: u32 = 0x8000_0000
    | ((core::mem::size_of::<KioLeds>() as u32 & 0xFFF) << 16)
    | ((b'k' as u32) << 8)
    | 2;

// ── Payload construct/decode helpers ──

/// Builds an `INPUT_CONF` driver-configuration message (server → driver).
///
/// C: input.c:516-522 fills `m_input_linputdriver_input_conf` with the two
/// owned slots (each possibly `INVALID_INPUT_ID`) and `_SYSTEM`-reserved
/// slots fixed to `INVALID_INPUT_ID`. The reserved lanes are set here, not
/// by callers: today they have exactly one legal value.
pub fn conf_msg(kbd_id: i32, mouse_id: i32) -> Message {
    let payload = MessInputLinputdriverInputConf {
        kbd_id,
        mouse_id,
        rsvd1_id: INVALID_INPUT_ID,
        rsvd2_id: INVALID_INPUT_ID,
        _padding: [0; 40],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: INPUT_CONF,
        m_u: MessageUnion {
            m_input_linputdriver_input_conf: payload,
        },
    }
}

/// Decodes an `INPUT_CONF` payload into (keyboard slot, mouse slot).
///
/// Returns `None` for other message types. Reserved lanes are validated:
/// a configuration naming a reserved device is corrupt (`None`), because
/// the server never sends one (see [`conf_msg`]).
pub fn decode_conf(msg: &Message) -> Option<(i32, i32)> {
    if msg.m_type != INPUT_CONF {
        return None;
    }
    let payload = unsafe { msg.m_u.m_input_linputdriver_input_conf };
    if payload.rsvd1_id != INVALID_INPUT_ID || payload.rsvd2_id != INVALID_INPUT_ID {
        return None;
    }
    Some((payload.kbd_id, payload.mouse_id))
}

/// Builds an `INPUT_SETLEDS` message (server → driver).
///
/// C: input.c:212-215 zeroes the message, sets the type and the mask.
pub fn setleds_msg(led_mask: u32) -> Message {
    let payload = MessInputLinputdriverSetleds {
        led_mask,
        _padding: [0; 52],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: INPUT_SETLEDS,
        m_u: MessageUnion {
            m_input_linputdriver_setleds: payload,
        },
    }
}

/// Decodes an `INPUT_SETLEDS` payload into the light mask.
pub fn decode_setleds(msg: &Message) -> Option<u32> {
    if msg.m_type != INPUT_SETLEDS {
        return None;
    }
    let payload = unsafe { msg.m_u.m_input_linputdriver_setleds };
    Some(payload.led_mask)
}

/// Builds an `INPUT_EVENT` driver report (driver → server).
///
/// C: the driver fills `m_linputdriver_input_event` (inputdriver.c:52-62);
/// `id` is the driver's assigned table slot (document 11), not a minor.
pub fn input_event_msg(id: i32, page: i32, code: i32, value: i32, flags: i32) -> Message {
    let payload = MessLinputdriverInputEvent {
        id,
        page,
        code,
        value,
        flags,
        _padding: [0; 36],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: INPUT_EVENT,
        m_u: MessageUnion {
            m_linputdriver_input_event: payload,
        },
    }
}

/// Decodes an `INPUT_EVENT` payload into (slot, page, code, value, flags).
pub fn decode_input_event(msg: &Message) -> Option<(i32, i32, i32, i32, i32)> {
    if msg.m_type != INPUT_EVENT {
        return None;
    }
    let payload = unsafe { msg.m_u.m_linputdriver_input_event };
    Some((
        payload.id,
        payload.page,
        payload.code,
        payload.value,
        payload.flags,
    ))
}

/// Builds a `TTY_INPUT_EVENT` relay (server → TTY).
///
/// C: input.c:409-420 copies the incoming report lane-for-lane into
/// `m_input_tty_event` and sends it on.
pub fn tty_event_msg(id: i32, page: i32, code: i32, value: i32, flags: i32) -> Message {
    let payload = MessInputTtyEvent {
        id,
        page,
        code,
        value,
        flags,
        _padding: [0; 36],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: TTY_INPUT_EVENT,
        m_u: MessageUnion {
            m_input_tty_event: payload,
        },
    }
}

/// Decodes a `TTY_INPUT_EVENT` payload into (slot, page, code, value, flags).
pub fn decode_tty_event(msg: &Message) -> Option<(i32, i32, i32, i32, i32)> {
    if msg.m_type != TTY_INPUT_EVENT {
        return None;
    }
    let payload = unsafe { msg.m_u.m_input_tty_event };
    Some((
        payload.id,
        payload.page,
        payload.code,
        payload.value,
        payload.flags,
    ))
}

/// Builds a `TTY_INPUT_UP` announcement (server → TTY).
///
/// C: input.c:672-677 zeroes the message and sets only the type: the
/// announcement carries no payload, presence is the message.
pub fn tty_up_msg() -> Message {
    Message {
        m_source: Endpoint::NONE,
        m_type: TTY_INPUT_UP,
        m_u: MessageUnion {
            raw: [0u8; crate::ipc::message::MESSAGE_PAYLOAD_SIZE],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_numbers_match_c() {
        // C: com.h:872-894 (bases and offsets).
        assert_eq!(TTY_RQ_BASE, 0x1300);
        assert_eq!(TTY_INPUT_UP, 0x1302);
        assert_eq!(TTY_INPUT_EVENT, 0x1303);
        assert_eq!(INPUT_RQ_BASE, 0x1500);
        assert_eq!(INPUT_RS_BASE, 0x1580);
        assert_eq!(INPUT_CONF, 0x1500);
        assert_eq!(INPUT_SETLEDS, 0x1501);
        assert_eq!(INPUT_EVENT, 0x1580);
        assert_eq!(INPUT_MAJOR, 64);
    }

    #[test]
    fn test_one_way_set_matches_c() {
        // C: com.h:886 — all five protocol messages need no reply.
        for number in [
            TTY_INPUT_UP,
            TTY_INPUT_EVENT,
            INPUT_CONF,
            INPUT_SETLEDS,
            INPUT_EVENT,
        ] {
            assert!(
                needs_no_reply(number),
                "message {number:#X} must be one-way"
            );
        }
        // Neighbouring numbers from other protocols are not ours.
        assert!(!needs_no_reply(0x0402)); // CDEV_READ answers
        assert!(!needs_no_reply(INPUT_RQ_BASE + 2)); // unassigned
        assert!(!needs_no_reply(INPUT_RS_BASE + 1)); // unassigned
    }

    #[test]
    fn test_nodes_match_makedev() {
        // C: MAKEDEV.sh:330-343 — ten nodes, major 64, minors per scheme.
        assert_eq!(INPUT_NODES.len(), 10);
        let names: [&str; 10] = [
            "kbdmux", "kbd0", "kbd1", "kbd2", "kbd3", "mousemux", "mouse0", "mouse1", "mouse2",
            "mouse3",
        ];
        let minors: [i32; 10] = [0, 1, 2, 3, 4, 64, 65, 66, 67, 68];
        for (slot, node) in INPUT_NODES.iter().enumerate() {
            assert_eq!(node.major, INPUT_MAJOR);
            assert_eq!(node.name, names[slot]);
            assert_eq!(node.minor, minors[slot]);
        }
    }

    #[test]
    fn test_device_kinds_match_c() {
        // C: input.h:9-13 (_SYSTEM segment).
        assert_eq!(INPUT_DEV_KBD, 0x01);
        assert_eq!(INPUT_DEV_MOUSE, 0x02);
        assert_eq!(INPUT_DEV_KBD | INPUT_DEV_MOUSE, 0x03);
        assert_eq!(INVALID_INPUT_ID, -1);
    }

    #[test]
    fn test_led_bits_match_c() {
        // C: kbdio.h:15-24.
        assert_eq!(core::mem::size_of::<KioLeds>(), 4);
        assert_eq!(KBD_LEDS_NUM, 0x1);
        assert_eq!(KBD_LEDS_CAPS, 0x2);
        assert_eq!(KBD_LEDS_SCROLL, 0x4);
    }

    #[test]
    fn test_led_control_number_match_c() {
        // C: _IOW('k', 2, struct kio_leds) = IOC_IN | (4 << 16) | ('k' << 8) | 2.
        assert_eq!(KIOCSLEDS, 0x8004_6B02);
    }

    #[test]
    fn test_conf_roundtrip_with_reserved_lanes() {
        let msg = conf_msg(1, -1);
        assert_eq!(msg.m_type, INPUT_CONF);
        assert_eq!(decode_conf(&msg), Some((1, -1)));
        // Corrupt reserved lanes are rejected (the server never sends them):
        // rebuilt by hand with a legal-looking but forbidden zero lane.
        let bad = Message {
            m_source: Endpoint::NONE,
            m_type: INPUT_CONF,
            m_u: MessageUnion {
                m_input_linputdriver_input_conf: MessInputLinputdriverInputConf {
                    kbd_id: 1,
                    mouse_id: -1,
                    rsvd1_id: 0,
                    rsvd2_id: -1,
                    _padding: [0; 40],
                },
            },
        };
        assert_eq!(decode_conf(&bad), None);
        // Other types do not decode as configuration.
        assert_eq!(decode_conf(&setleds_msg(0)), None);
    }

    #[test]
    fn test_setleds_roundtrip() {
        let msg = setleds_msg(0x6);
        assert_eq!(msg.m_type, INPUT_SETLEDS);
        assert_eq!(decode_setleds(&msg), Some(0x6));
        assert_eq!(decode_setleds(&conf_msg(1, 2)), None);
    }

    #[test]
    fn test_event_reports_roundtrip() {
        let msg = input_event_msg(2, 0x0007, 0x0004, 1, 0);
        assert_eq!(msg.m_type, INPUT_EVENT);
        assert_eq!(decode_input_event(&msg), Some((2, 0x0007, 0x0004, 1, 0)));
        assert_eq!(
            decode_input_event(&tty_event_msg(2, 0x0007, 0x0004, 1, 0)),
            None
        );
        let relay = tty_event_msg(2, 0x0007, 0x0004, 1, 0);
        assert_eq!(relay.m_type, TTY_INPUT_EVENT);
        assert_eq!(decode_tty_event(&relay), Some((2, 0x0007, 0x0004, 1, 0)));
        assert_eq!(decode_tty_event(&msg), None);
    }

    #[test]
    fn test_tty_up_carries_no_payload() {
        let msg = tty_up_msg();
        assert_eq!(msg.m_type, TTY_INPUT_UP);
    }

    #[test]
    fn test_driver_key_prefix_matches_both_sides() {
        // C: client publishes "drv.inp.<label>" (inputdriver.c:23,32);
        // server filters strncmp(key, "drv.inp.", len) (input.c:562-579).
        assert_eq!(DRIVER_KEY_PREFIX, "drv.inp.");
        assert_eq!(driver_label("drv.inp.kbd0"), Some("kbd0"));
        assert_eq!(driver_label("drv.inp."), Some(""));
        assert_eq!(driver_label("drv.chr.kbd0"), None);
        assert_eq!(driver_label("drv.inpx.kbd0"), None);
        assert_eq!(driver_label("other"), None);
    }
}
