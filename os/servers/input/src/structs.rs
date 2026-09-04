//! Device structures and the two numberings of input devices.
//!
//! C: `minix3/minix/servers/input/input.h` (45 lines) plus `input_map` and
//! `input_revmap` (`minix3/minix/servers/input/input.c:44-80`).
//!
//! The single hardest idea in this module: every input device has *two*
//! numbers. The **minor number** is what user processes see in `/dev` (0 for
//! the keyboard multiplexer, 1-4 for individual keyboards, 64 for the mouse
//! multiplexer, 65-68 for individual mice) — sparse, with gaps left for
//! future keyboards. The **table index** is where the server keeps the device
//! in its ten-slot array (0-9, dense). C converts with two small functions;
//! this module keeps the conversion but makes each numbering its own type, so
//! passing a minor where an index is expected does not compile.
//!
//! Corresponding document: `03-input-device-structs.md`.

use crate::error::InputError;
use crate::event::InputEvent;
use minix_types::{DS_MAX_KEYLEN, Endpoint};

/// How many events one device buffer holds.
///
/// C: `EVENTBUF_SIZE 32` (`input.h:7`). Thirty-two key or motion events is
/// enough to ride out scheduling delays (a fast typist produces a handful of
/// events per scheduling quantum); when the buffer does fill, the oldest
/// event is overwritten, never the newest (document 09).
pub const EVENT_BUFFER_SIZE: usize = 32;

/// How many device slots the server owns.
///
/// C: `INPUT_DEV_MAX (1 + KBD_MINORS + 1 + MOUSE_MINORS)` = 10 (`input.h:26`):
/// one keyboard multiplexer, four keyboards, one mouse multiplexer, four
/// mice. The count is fixed at compile time — no allocation, no resizing.
pub const DEVICE_COUNT: usize = 10;

// ── Minor numbers: the numbers user processes see ──

/// Minor number of the keyboard multiplexer (`KBDMUX_MINOR = 0`).
pub const KEYBOARD_MULTIPLEXER_MINOR: i32 = 0;
/// Minor number of the first individual keyboard (`KBD0_MINOR = 1`).
pub const KEYBOARD_FIRST_MINOR: i32 = 1;
/// How many individual keyboard minors exist (`KBD_MINORS = 4`).
pub const KEYBOARD_MINOR_COUNT: i32 = 4;
/// Minor number of the mouse multiplexer (`MOUSEMUX_MINOR = 64`).
pub const MOUSE_MULTIPLEXER_MINOR: i32 = 64;
/// Minor number of the first individual mouse (`MOUSE0_MINOR = 65`).
pub const MOUSE_FIRST_MINOR: i32 = 65;
/// How many individual mouse minors exist (`MOUSE_MINORS = 4`).
pub const MOUSE_MINOR_COUNT: i32 = 4;

// ── Table indices: the slots inside the server ──

/// Index of the keyboard multiplexer slot (`KBDMUX_DEV = 0`).
pub const KEYBOARD_MULTIPLEXER_INDEX: usize = 0;
/// Index of the first keyboard slot (`FIRST_KBD_DEV = 1`).
pub const FIRST_KEYBOARD_INDEX: usize = 1;
/// Index of the last keyboard slot (`LAST_KBD_DEV = 4`).
pub const LAST_KEYBOARD_INDEX: usize = 4;
/// Index of the mouse multiplexer slot (`MOUSEMUX_DEV = 5`).
pub const MOUSE_MULTIPLEXER_INDEX: usize = 5;
/// Index of the first mouse slot (`FIRST_MOUSE_DEV = 6`).
pub const FIRST_MOUSE_INDEX: usize = 6;
/// Index of the last mouse slot (`LAST_MOUSE_DEV = 9`).
pub const LAST_MOUSE_INDEX: usize = 9;

/// A device minor number: the identity user processes use.
///
/// C: `devminor_t` (an `int`). The newtype exists so a minor can never be
/// confused with a table index: both are small integers, and mixing them up
/// reads the wrong device's buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Minor(pub i32);

impl Minor {
    /// Whether this is one of the two multiplexer minors (0 or 64).
    pub const fn is_multiplexer(self) -> bool {
        self.0 == KEYBOARD_MULTIPLEXER_MINOR || self.0 == MOUSE_MULTIPLEXER_MINOR
    }

    /// Whether this is the keyboard-multiplexer minor (0).
    ///
    /// Needed by the light broadcast (`input.c:224` matches only
    /// `KBDMUX_MINOR`): the mouse multiplexer (64) broadcasts nothing,
    /// since the light loop ranges over keyboard slots only.
    pub const fn is_keyboard_multiplexer(self) -> bool {
        self.0 == KEYBOARD_MULTIPLEXER_MINOR
    }
}

/// A device-table index: the slot inside the server's ten-element array.
///
/// Valid values are `0..DEVICE_COUNT`. Construction is unchecked; use
/// [`DeviceIndex::new`] when the value comes from outside the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceIndex(pub usize);

impl DeviceIndex {
    /// Validates a raw index from outside the server (an event message
    /// carries one — document 09).
    ///
    /// C: `input_event` drops events with `id < 0 || id >= INPUT_DEV_MAX`
    /// (`input.c:384-385`). Rust returns the failure instead of dropping
    /// silently at this layer; the caller decides to drop.
    pub const fn new(raw: usize) -> Result<Self, InputError> {
        if raw < DEVICE_COUNT {
            Ok(DeviceIndex(raw))
        } else {
            Err(InputError::InvalidDeviceIndex)
        }
    }
}

/// Converts a minor number to the owning table slot.
///
/// C: `input_map` (`input.c:44-62`). The keyboard minors 1-4 land on slots
/// 1-4, the mouse minors 65-68 on slots 6-9; anything else (including the
/// gaps 5-63 and 69+) is no device. Unknown minors are `None` — the caller
/// (document 06) answers `ENXIO`.
pub const fn map_minor_to_index(minor: Minor) -> Option<DeviceIndex> {
    let number = minor.0;
    if number == KEYBOARD_MULTIPLEXER_MINOR {
        Some(DeviceIndex(KEYBOARD_MULTIPLEXER_INDEX))
    } else if number >= KEYBOARD_FIRST_MINOR && number < KEYBOARD_FIRST_MINOR + KEYBOARD_MINOR_COUNT
    {
        Some(DeviceIndex(
            FIRST_KEYBOARD_INDEX + (number - KEYBOARD_FIRST_MINOR) as usize,
        ))
    } else if number == MOUSE_MULTIPLEXER_MINOR {
        Some(DeviceIndex(MOUSE_MULTIPLEXER_INDEX))
    } else if number >= MOUSE_FIRST_MINOR && number < MOUSE_FIRST_MINOR + MOUSE_MINOR_COUNT {
        Some(DeviceIndex(
            FIRST_MOUSE_INDEX + (number - MOUSE_FIRST_MINOR) as usize,
        ))
    } else {
        None
    }
}

/// Converts a table slot back to its minor number.
///
/// C: `input_revmap` (`input.c:67-80`), which calls `panic` on an invalid
/// index. That panic documents a C assumption — "the server only ever maps
/// indices it owns" — by crashing when it breaks. Rust states the assumption
/// in the type instead (architecture evolution A-7): invalid indices are
/// `None`, and the only caller that needs totality (`InputTable::fresh`)
/// indexes a compile-time table that cannot be wrong (see below).
pub const fn minor_of_index(index: DeviceIndex) -> Option<Minor> {
    let slot = index.0;
    if slot == KEYBOARD_MULTIPLEXER_INDEX {
        Some(Minor(KEYBOARD_MULTIPLEXER_MINOR))
    } else if slot >= FIRST_KEYBOARD_INDEX && slot <= LAST_KEYBOARD_INDEX {
        Some(Minor(
            KEYBOARD_FIRST_MINOR + (slot - FIRST_KEYBOARD_INDEX) as i32,
        ))
    } else if slot == MOUSE_MULTIPLEXER_INDEX {
        Some(Minor(MOUSE_MULTIPLEXER_MINOR))
    } else if slot >= FIRST_MOUSE_INDEX && slot <= LAST_MOUSE_INDEX {
        Some(Minor(MOUSE_FIRST_MINOR + (slot - FIRST_MOUSE_INDEX) as i32))
    } else {
        None
    }
}

/// The minor of each table slot, in slot order.
///
/// The same mapping as [`minor_of_index`], written as data so table setup is
/// total by construction: slot `i` owns `MINOR_OF_SLOT[i]`, no lookup that
/// could fail. `minor_table_matches_map_functions` locks the two spellings
/// together, so they cannot drift apart.
const MINOR_OF_SLOT: [i32; DEVICE_COUNT] = [0, 1, 2, 3, 4, 64, 65, 66, 67, 68];

/// One input device: everything the server remembers about it.
///
/// C: `struct input_dev` (`input.h:29-43`). Thirteen fields, each with a
/// distinct role:
///
/// - Identity: `minor` (which `/dev` node), `owner` (which driver feeds it,
///   or nobody), `label` (that driver's registry name).
/// - Event queue: `events` (ring of 32), `tail` (where the next event lands),
///   `count` (how many are stored).
/// - Reader state: `opened` (someone holds the device), `suspended` (a read
///   is parked waiting for events), `caller`/`grant`/`request_id` (how to
///   answer that parked read), `selector` (who asked to be told when events
///   arrive).
/// - Lights: `leds` (last known indicator mask, survives driver restarts —
///   document 10).
#[derive(Debug, Clone, Copy)]
pub struct InputDevice {
    /// The `/dev` minor number of this slot (fixed for the slot's lifetime).
    pub minor: Minor,
    /// The driver currently feeding this device, or [`Endpoint::NONE`].
    pub owner: Endpoint,
    /// The owning driver's registry name, NUL-terminated (`DS_MAX_KEYLEN`
    /// bytes including the terminator, as in C).
    pub label: [u8; DS_MAX_KEYLEN],
    /// Ring buffer of pending events (element format: [`InputEvent`]).
    pub events: [InputEvent; EVENT_BUFFER_SIZE],
    /// Ring position where the next event will be stored.
    pub tail: u32,
    /// How many events are currently stored (`0..=EVENT_BUFFER_SIZE`).
    pub count: u32,
    /// Whether a process currently holds the device open.
    pub opened: bool,
    /// Whether a read is parked on this device waiting for events.
    pub suspended: bool,
    /// Who issued the parked read.
    pub caller: Endpoint,
    /// Memory grant into which the parked read copies events.
    pub grant: i32,
    /// Request identifier the parked read's answer must echo.
    pub request_id: u32,
    /// Who asked (via select) to be told when events arrive, or
    /// [`Endpoint::NONE`].
    pub selector: Endpoint,
    /// Last known indicator-light mask (kept across driver restarts).
    pub leds: u32,
}

impl InputDevice {
    /// An untouched slot: no owner, empty queue, closed, lights off.
    ///
    /// The minor is filled in by [`InputTable::fresh`]; every other field
    /// starts at its resting value, mirroring the `input_init` loop
    /// (`input.c:653-662`).
    const fn empty() -> Self {
        Self {
            minor: Minor(-1),
            owner: Endpoint::NONE,
            label: [0; DS_MAX_KEYLEN],
            events: [InputEvent::zero(); EVENT_BUFFER_SIZE],
            tail: 0,
            count: 0,
            opened: false,
            suspended: false,
            caller: Endpoint::NONE,
            grant: 0,
            request_id: 0,
            selector: Endpoint::NONE,
            leds: 0,
        }
    }

    /// Whether this device currently accepts readers.
    ///
    /// C: `input_dev_active` (`input.c:24-26`): a device is active when a
    /// driver owns it — *or* when it is one of the two multiplexers, which
    /// are always active so readers can wait on "any keyboard" before any
    /// keyboard driver has announced itself.
    pub fn is_active(self) -> bool {
        self.owner != Endpoint::NONE || self.minor.is_multiplexer()
    }

    /// Whether the queue holds no events (`input_dev_buf_empty`).
    pub const fn is_buffer_empty(self) -> bool {
        self.count == 0
    }

    /// Whether the queue holds a full buffer (`input_dev_buf_full`).
    pub const fn is_buffer_full(self) -> bool {
        self.count as usize == EVENT_BUFFER_SIZE
    }

    /// Whether a select waiter is recorded.
    pub const fn has_selector(self) -> bool {
        self.selector.0 != Endpoint::NONE.0
    }

    /// Stores a driver registry name, truncating to fit with a terminator.
    ///
    /// C keeps the label in a fixed `char[DS_MAX_KEYLEN]` array; overlong
    /// names cannot grow the array, so they are cut short. The terminator is
    /// always written: a full array with no terminator would read past its
    /// end as a C string.
    pub fn set_label(&mut self, name: &[u8]) {
        let room = DS_MAX_KEYLEN - 1;
        let take = if name.len() < room { name.len() } else { room };
        self.label[0..take].copy_from_slice(&name[0..take]);
        self.label[take] = 0;
        for byte in self.label[take + 1..].iter_mut() {
            *byte = 0;
        }
    }

    /// The stored name, up to (not including) the first NUL byte.
    pub fn label_bytes(&self) -> &[u8] {
        let end = self
            .label
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(DS_MAX_KEYLEN);
        &self.label[0..end]
    }
}

/// All ten device slots, owned by the server event loop.
///
/// Created once at startup by [`InputTable::fresh`], then mutated in place
/// for the server's whole lifetime (single-threaded event loop: one message
/// at a time, no locking needed — same execution model as the data-store and
/// device-manager servers).
#[derive(Debug, Clone, Copy)]
pub struct InputTable {
    /// The ten slots in index order (multiplexer, keyboards, multiplexer,
    /// mice).
    pub devices: [InputDevice; DEVICE_COUNT],
}

impl InputTable {
    /// Builds the freshly booted table.
    ///
    /// C: the `input_init` loop (`input.c:653-662`): each slot gets its minor
    /// back-filled, no owner, an empty queue, closed, unparked, no selector,
    /// lights off. Total by construction — the minors come from
    /// `MINOR_OF_SLOT`, so there is no lookup to fail.
    pub fn fresh() -> Self {
        let mut devices = [InputDevice::empty(); DEVICE_COUNT];
        let mut slot = 0;
        while slot < DEVICE_COUNT {
            devices[slot].minor = Minor(MINOR_OF_SLOT[slot]);
            slot += 1;
        }
        Self { devices }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minor_constants_match_c() {
        // C: `input.h:9-15`.
        assert_eq!(KEYBOARD_MULTIPLEXER_MINOR, 0);
        assert_eq!(KEYBOARD_FIRST_MINOR, 1);
        assert_eq!(KEYBOARD_MINOR_COUNT, 4);
        assert_eq!(MOUSE_MULTIPLEXER_MINOR, 64);
        assert_eq!(MOUSE_FIRST_MINOR, 65);
        assert_eq!(MOUSE_MINOR_COUNT, 4);
        // C: `input.h:18-26` — ten slots total.
        assert_eq!(DEVICE_COUNT, 10);
        assert_eq!(LAST_MOUSE_INDEX, 9);
        // Multiplexer predicates (used by the document 10 broadcast).
        assert!(Minor(0).is_multiplexer());
        assert!(Minor(64).is_multiplexer());
        assert!(!Minor(1).is_multiplexer());
        assert!(Minor(0).is_keyboard_multiplexer());
        assert!(!Minor(64).is_keyboard_multiplexer());
        assert!(!Minor(1).is_keyboard_multiplexer());
    }

    #[test]
    fn test_map_minor_to_index_matches_c_branches() {
        // C: `input_map` (`input.c:44-62`) — each branch in order.
        assert_eq!(
            map_minor_to_index(Minor(0)),
            Some(DeviceIndex(0)) // KBDMUX_MINOR branch
        );
        for minor in 1..=4 {
            assert_eq!(
                map_minor_to_index(Minor(minor)),
                Some(DeviceIndex(minor as usize)) // KBD0 branch
            );
        }
        assert_eq!(
            map_minor_to_index(Minor(64)),
            Some(DeviceIndex(5)) // MOUSEMUX_MINOR branch
        );
        for minor in 65..=68 {
            assert_eq!(
                map_minor_to_index(Minor(minor)),
                Some(DeviceIndex((minor - 65) as usize + 6)) // MOUSE0 branch
            );
        }
        // The gaps C leaves for future devices, plus negatives and the tail.
        for minor in [5i32, 7, 63, 69, 100, -1, i32::MAX] {
            assert_eq!(map_minor_to_index(Minor(minor)), None);
        }
    }

    #[test]
    fn test_minor_of_index_round_trips_map() {
        // Every minor the map accepts converts back to itself ...
        for minor in [0, 1, 2, 3, 4, 64, 65, 66, 67, 68] {
            let index = map_minor_to_index(Minor(minor)).unwrap();
            assert_eq!(minor_of_index(index), Some(Minor(minor)));
        }
        // ... and indices outside the table fail instead of panicking (A-7:
        // C `panic`s at `input.c:79`; Rust returns `None`).
        assert_eq!(minor_of_index(DeviceIndex(10)), None);
        assert_eq!(minor_of_index(DeviceIndex(usize::MAX)), None);
        assert_eq!(DeviceIndex::new(10), Err(InputError::InvalidDeviceIndex));
    }

    #[test]
    fn test_minor_table_matches_map_functions() {
        // `MINOR_OF_SLOT` and the two branch functions must say the same
        // thing; this test is the lock between the two spellings.
        for (slot, minor) in MINOR_OF_SLOT.iter().enumerate() {
            assert_eq!(map_minor_to_index(Minor(*minor)), Some(DeviceIndex(slot)));
            assert_eq!(minor_of_index(DeviceIndex(slot)), Some(Minor(*minor)));
        }
    }

    #[test]
    fn test_fresh_table_matches_input_init_loop() {
        // C: `input_init` (`input.c:653-662`) — minor back-filled, owner NONE,
        // empty queue, closed, unparked, no selector, lights off.
        let table = InputTable::fresh();
        for (slot, device) in table.devices.iter().enumerate() {
            assert_eq!(device.minor, Minor(MINOR_OF_SLOT[slot]));
            assert_eq!(device.owner, Endpoint::NONE);
            assert_eq!(device.tail, 0);
            assert_eq!(device.count, 0);
            assert!(device.is_buffer_empty());
            assert!(!device.is_buffer_full());
            assert!(!device.opened);
            assert!(!device.suspended);
            assert!(!device.has_selector());
            assert_eq!(device.leds, 0);
        }
        // The two multiplexers are active with no driver; plain slots are not.
        assert!(table.devices[KEYBOARD_MULTIPLEXER_INDEX].is_active());
        assert!(table.devices[MOUSE_MULTIPLEXER_INDEX].is_active());
        assert!(!table.devices[FIRST_KEYBOARD_INDEX].is_active());
        assert!(!table.devices[FIRST_MOUSE_INDEX].is_active());
    }

    #[test]
    fn test_label_truncates_with_terminator() {
        let mut device = InputDevice::empty();
        device.set_label(b"kbd0");
        assert_eq!(device.label_bytes(), b"kbd0");
        let long = [b'x'; DS_MAX_KEYLEN + 20];
        device.set_label(&long);
        assert_eq!(device.label_bytes().len(), DS_MAX_KEYLEN - 1);
        assert!(device.label.iter().all(|byte| *byte == b'x' || *byte == 0));
        assert_eq!(device.label[DS_MAX_KEYLEN - 1], 0);
    }
}
