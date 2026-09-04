//! Input event wire format and event codes.
//!
//! C: `minix3/minix/include/minix/input.h` (333 lines). Every input driver in
//! the system (keyboard, mouse, and any future driver) converts what its
//! hardware reports into this one format before sending it to the input
//! server; every reader (the terminal driver today, a window system tomorrow)
//! interprets the same format. This module owns the format definition, so a
//! driver and a reader can never disagree about what a byte means.
//!
//! The keyboard-page code table (215 enumerators) lives in [`key_codes`]:
//! it is mechanically derived from the C header, one constant per
//! enumerator, with the values locked by test.
//!
//! Corresponding document: `04-input-event-format.md`.

/// Driver device-type bit: keyboard (`INPUT_DEV_KBD = 0x01`, `input.h:9`).
///
/// C: visible only under `_SYSTEM` — ordinary programs never see it. Drivers
/// announce their kind with these bits (a combined keyboard-plus-pointer
/// announces both; document 14), and the server matches announcements
/// against them (document 11). Defined here because the header that names
/// them is this module's ground truth; *used* in documents 11/12/14.
///
/// Authority (§2.4g): `minix_types::INPUT_DEV_KBD` is the shared canonical
/// copy (document 05 owns the protocol numbers); this copy serves
/// crate-local use. Change that copy first, then sync this one.
pub const DEVICE_TYPE_KEYBOARD: u16 = 0x01;

/// Driver device-type bit: mouse (`INPUT_DEV_MOUSE = 0x02`, `input.h:10`).
///
/// See [`DEVICE_TYPE_KEYBOARD`] for the contract (canonical shared copy:
/// `minix_types::INPUT_DEV_MOUSE`).
pub const DEVICE_TYPE_MOUSE: u16 = 0x02;

/// "No device assigned yet" (`INVALID_INPUT_ID = -1`, `input.h:13`).
///
/// Negative so it can never collide with a real table index (all
/// non-negative). The server hands this to drivers whose connect attempt
/// found no slot, and drivers store it as "I have no device" (document 11).
///
/// Authority (§2.4g): canonical shared copy `minix_types::INVALID_INPUT_ID`;
/// this copy serves crate-local use (see above).
pub const INVALID_INPUT_ID: i32 = -1;

/// One input event as read from an input device.
///
/// C: `struct input_event` (`input.h:25-32`). Fixed 20-byte layout on the
/// wire: two 16-bit discriminators, one 32-bit value, two 16-bit routing
/// fields, two reserved 32-bit words. `repr(C)` keeps the field order and
/// padding identical to C; `event_layout_matches_c` locks the total size.
///
/// The `reserved` words are always zero on send (the server writes zero, see
/// document 09) and ignored on receive. They hold space for a future
/// timestamp (`input.h:31`); readers must not assign them meaning today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct InputEvent {
    /// Which family the event belongs to ([`EventPage`]).
    pub page: u16,
    /// Page-specific code (for example a [`KeyCode`] when `page` is keyboard).
    pub code: u16,
    /// Event value: press/release for keys and buttons, position or delta for
    /// pointer motion (see [`PressState`]).
    pub value: i32,
    /// Absolute or relative interpretation of `value` (see [`ValueMode`]).
    pub flags: u16,
    /// Which physical device produced the event (filled in by the server).
    pub source_device: u16,
    /// Reserved for a future timestamp; always zero.
    pub reserved: [u32; 2],
}

impl InputEvent {
    /// A zero event: all discriminators unset, value released, reserved zero.
    ///
    /// C: the server zeroes messages with `memset` before filling them in;
    /// starting from zero means "no information yet" in both languages.
    pub const fn zero() -> Self {
        Self {
            page: 0,
            code: 0,
            value: 0,
            flags: 0,
            source_device: 0,
            reserved: [0, 0],
        }
    }

    /// Whether both reserved words are zero, as the contract requires.
    pub const fn reserved_is_zero(self) -> bool {
        self.reserved[0] == 0 && self.reserved[1] == 0
    }
}

impl Default for InputEvent {
    fn default() -> Self {
        Self::zero()
    }
}

/// Event families: the top-level discriminator of every event.
///
/// C: `INPUT_PAGE_*` (`input.h:35-39`). The values mirror the USB HID usage
/// page numbers the input server borrows its vocabulary from (general
/// desktop `0x01`, keyboard `0x07`, LED `0x08`, button `0x09`, consumer
/// `0x0C`); the gaps between them are HID pages the input server does not
/// use, not an oversight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EventPage {
    /// Pointer motion and system controls (`INPUT_PAGE_GD = 0x0001`).
    GeneralDesktop = 0x0001,
    /// Key presses and releases (`INPUT_PAGE_KEY = 0x0007`).
    Keyboard = 0x0007,
    /// Indicator lights (`INPUT_PAGE_LED = 0x0008`).
    Led = 0x0008,
    /// Pointer buttons (`INPUT_PAGE_BUTTON = 0x0009`).
    Button = 0x0009,
    /// Media and application keys (`INPUT_PAGE_CONS = 0x000C`).
    Consumer = 0x000C,
}

impl EventPage {
    /// Interprets a wire value; unknown pages are `None` (the server drops
    /// such events rather than guessing — document 09).
    pub const fn from_u16(raw: u16) -> Option<Self> {
        match raw {
            0x0001 => Some(EventPage::GeneralDesktop),
            0x0007 => Some(EventPage::Keyboard),
            0x0008 => Some(EventPage::Led),
            0x0009 => Some(EventPage::Button),
            0x000C => Some(EventPage::Consumer),
            _ => None,
        }
    }

    /// The wire value.
    pub const fn as_u16(self) -> u16 {
        self as u16
    }
}

/// Key and button values: pressed or released.
///
/// C: `INPUT_PRESS = 1`, `INPUT_RELEASE = 0` (`input.h:42-43`, "not
/// exhaustive" — pointer motion carries positions and deltas instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum PressState {
    /// The key or button was released (`INPUT_RELEASE = 0`).
    Released = 0,
    /// The key or button was pressed (`INPUT_PRESS = 1`).
    Pressed = 1,
}

impl PressState {
    /// Interprets a wire value; motion values are not press states.
    pub const fn from_i32(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(PressState::Released),
            1 => Some(PressState::Pressed),
            _ => None,
        }
    }
}

/// Whether a `value` is absolute or relative.
///
/// C: `INPUT_FLAG_ABS = 0x00` (the default), `INPUT_FLAG_REL = 0x04`
/// (`input.h:46-47`). A touchscreen reports where the finger is (absolute);
/// a mouse reports how far it moved (relative). The flag bit `0x04` is the
/// HID "relative" bit, kept verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueMode {
    /// Absolute position (default when flags are zero).
    Absolute,
    /// Relative displacement since the previous event.
    Relative,
}

impl ValueMode {
    /// Interprets the wire flags; only the relative bit is examined, exactly
    /// as C tests it (`flags & INPUT_FLAG_REL`).
    pub const fn from_flags(flags: u16) -> Self {
        if flags & 0x04 != 0 {
            ValueMode::Relative
        } else {
            ValueMode::Absolute
        }
    }
}

/// General-desktop-page codes.
///
/// C: `INPUT_GD_*` (`input.h:50-57`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum GeneralDesktopCode {
    /// Horizontal pointer axis (`0x0030`).
    X = 0x0030,
    /// Vertical pointer axis (`0x0031`, implicit next value in C).
    Y = 0x0031,
    /// System power down (`0x0081`).
    SystemPowerDown = 0x0081,
    /// System sleep (`0x0082`).
    SystemSleep = 0x0082,
    /// System wake up (`0x0083`).
    SystemWakeUp = 0x0083,
}

impl GeneralDesktopCode {
    /// Interprets a wire code on the general-desktop page.
    pub const fn from_u16(raw: u16) -> Option<Self> {
        match raw {
            0x0030 => Some(GeneralDesktopCode::X),
            0x0031 => Some(GeneralDesktopCode::Y),
            0x0081 => Some(GeneralDesktopCode::SystemPowerDown),
            0x0082 => Some(GeneralDesktopCode::SystemSleep),
            0x0083 => Some(GeneralDesktopCode::SystemWakeUp),
            _ => None,
        }
    }
}

/// LED-page codes: which indicator a value addresses.
///
/// C: `INPUT_LED_*` (`input.h:292-296`). Note these are LED *identities* on
/// the event page; the on/off bit mask used by the set-lights request is a
/// separate encoding owned by document 10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum LedCode {
    /// Number-lock indicator (`0x0001`).
    NumLock = 0x0001,
    /// Caps-lock indicator (`0x0002`).
    CapsLock = 0x0002,
    /// Scroll-lock indicator (`0x0003`).
    ScrollLock = 0x0003,
}

impl LedCode {
    /// Interprets a wire code on the LED page.
    pub const fn from_u16(raw: u16) -> Option<Self> {
        match raw {
            0x0001 => Some(LedCode::NumLock),
            0x0002 => Some(LedCode::CapsLock),
            0x0003 => Some(LedCode::ScrollLock),
            _ => None,
        }
    }
}

/// Button-page codes.
///
/// C: `INPUT_BUTTON_1 = 0x0001` (`input.h:298-300`) — the only button the
/// header names. Further buttons travel as larger codes on the same page;
/// they are valid wire values without a named constant, the same way
/// undefined key codes travel inside [`KeyCode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ButtonCode {
    /// Primary button (`0x0001`).
    Button1 = 0x0001,
}

impl ButtonCode {
    /// Interprets a wire code on the button page.
    pub const fn from_u16(raw: u16) -> Option<Self> {
        match raw {
            0x0001 => Some(ButtonCode::Button1),
            _ => None,
        }
    }
}

/// Consumer-page codes: media and application keys.
///
/// C: `INPUT_CONS_*` (`input.h:302-331`). Explicit values are repeated here
/// even where C relies on auto-increment, so each number is reviewable in
/// isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ConsumerCode {
    /// Scan next track (`0x00B5`).
    ScanNextTrack = 0x00B5,
    /// Scan previous track (`0x00B6`).
    ScanPreviousTrack = 0x00B6,
    /// Stop playback (`0x00B7`).
    Stop = 0x00B7,
    /// Play or pause (`0x00CD`).
    PlayPause = 0x00CD,
    /// Mute (`0x00E2`).
    Mute = 0x00E2,
    /// Volume up (`0x00E9`).
    VolumeUp = 0x00E9,
    /// Volume down (`0x00EA`).
    VolumeDown = 0x00EA,
    /// Launch media selector (`0x0183`).
    MediaSelect = 0x0183,
    /// Launch mail reader (`0x018A`).
    MailReader = 0x018A,
    /// Launch calculator (`0x0192`).
    Calculator = 0x0192,
    /// Launch local browser (`0x0194`).
    LocalBrowser = 0x0194,
    /// Search (`0x0221`).
    Search = 0x0221,
    /// Go to (`0x0222`).
    GoTo = 0x0222,
    /// Home (`0x0223`).
    Home = 0x0223,
    /// Back (`0x0224`).
    Back = 0x0224,
    /// Forward (`0x0225`).
    Forward = 0x0225,
    /// Stop browsing (`0x0226`).
    AcquisitionStop = 0x0226,
    /// Refresh (`0x0227`).
    Refresh = 0x0227,
    /// Bookmarks (`0x022A`).
    Bookmarks = 0x022A,
}

impl ConsumerCode {
    /// Interprets a wire code on the consumer page.
    pub const fn from_u16(raw: u16) -> Option<Self> {
        match raw {
            0x00B5 => Some(ConsumerCode::ScanNextTrack),
            0x00B6 => Some(ConsumerCode::ScanPreviousTrack),
            0x00B7 => Some(ConsumerCode::Stop),
            0x00CD => Some(ConsumerCode::PlayPause),
            0x00E2 => Some(ConsumerCode::Mute),
            0x00E9 => Some(ConsumerCode::VolumeUp),
            0x00EA => Some(ConsumerCode::VolumeDown),
            0x0183 => Some(ConsumerCode::MediaSelect),
            0x018A => Some(ConsumerCode::MailReader),
            0x0192 => Some(ConsumerCode::Calculator),
            0x0194 => Some(ConsumerCode::LocalBrowser),
            0x0221 => Some(ConsumerCode::Search),
            0x0222 => Some(ConsumerCode::GoTo),
            0x0223 => Some(ConsumerCode::Home),
            0x0224 => Some(ConsumerCode::Back),
            0x0225 => Some(ConsumerCode::Forward),
            0x0226 => Some(ConsumerCode::AcquisitionStop),
            0x0227 => Some(ConsumerCode::Refresh),
            0x022A => Some(ConsumerCode::Bookmarks),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_codes::KeyCode;
    use core::mem::size_of;

    #[test]
    fn test_event_layout_matches_c() {
        // C: 2 + 2 + 4 + 2 + 2 + 8 = 20 bytes, no padding on any alignment
        // that matters (`uint16_t`/`int32_t`/`uint32_t` natural alignment).
        assert_eq!(size_of::<InputEvent>(), 20);
        let zero = InputEvent::zero();
        assert!(zero.reserved_is_zero());
        assert_eq!(zero, InputEvent::default());
    }

    #[test]
    fn test_event_page_values_match_c() {
        // C: `input.h:35-39` (USB HID usage page numbers).
        assert_eq!(EventPage::GeneralDesktop.as_u16(), 0x0001);
        assert_eq!(EventPage::Keyboard.as_u16(), 0x0007);
        assert_eq!(EventPage::Led.as_u16(), 0x0008);
        assert_eq!(EventPage::Button.as_u16(), 0x0009);
        assert_eq!(EventPage::Consumer.as_u16(), 0x000C);
        // HID pages the input server does not use stay unknown.
        assert_eq!(EventPage::from_u16(0x0002), None);
        assert_eq!(EventPage::from_u16(0xFFFF), None);
    }

    #[test]
    fn test_press_state_and_value_mode_match_c() {
        // C: `input.h:42-43` and `input.h:46-47`.
        assert_eq!(PressState::from_i32(0), Some(PressState::Released));
        assert_eq!(PressState::from_i32(1), Some(PressState::Pressed));
        assert_eq!(PressState::from_i32(5), None); // a motion value, not a press
        assert_eq!(ValueMode::from_flags(0x00), ValueMode::Absolute);
        assert_eq!(ValueMode::from_flags(0x04), ValueMode::Relative);
    }

    #[test]
    fn test_small_page_tables_match_c() {
        assert_eq!(
            GeneralDesktopCode::from_u16(0x0030),
            Some(GeneralDesktopCode::X)
        );
        assert_eq!(
            GeneralDesktopCode::from_u16(0x0083),
            Some(GeneralDesktopCode::SystemWakeUp)
        );
        assert_eq!(GeneralDesktopCode::from_u16(0x0040), None);
        assert_eq!(LedCode::from_u16(0x0002), Some(LedCode::CapsLock));
        assert_eq!(ButtonCode::from_u16(0x0001), Some(ButtonCode::Button1));
        assert_eq!(ButtonCode::from_u16(0x0002), None);
        assert_eq!(
            ConsumerCode::from_u16(0x00CD),
            Some(ConsumerCode::PlayPause)
        );
        assert_eq!(
            ConsumerCode::from_u16(0x022A),
            Some(ConsumerCode::Bookmarks)
        );
        assert_eq!(ConsumerCode::from_u16(0x00B8), None); // gap between Stop and PlayPause
    }

    #[test]
    fn test_system_segment_constants_match_c() {
        // C: `input.h:6-15` (`_SYSTEM` only). The two type bits compose;
        // the invalid id is negative so it never collides with an index.
        assert_eq!(DEVICE_TYPE_KEYBOARD, 0x01);
        assert_eq!(DEVICE_TYPE_MOUSE, 0x02);
        assert_eq!(DEVICE_TYPE_KEYBOARD | DEVICE_TYPE_MOUSE, 0x03);
        assert_eq!(INVALID_INPUT_ID, -1);
    }

    #[test]
    fn test_wire_event_round_trip() {
        // A key press as the server stores it: page, code, value, flags, and
        // the source device filled in; reserved stays zero.
        let event = InputEvent {
            page: EventPage::Keyboard.as_u16(),
            code: KeyCode::A.as_u16(),
            value: PressState::Pressed as i32,
            flags: 0,
            source_device: 1,
            reserved: [0, 0],
        };
        assert_eq!(EventPage::from_u16(event.page), Some(EventPage::Keyboard));
        assert_eq!(KeyCode::from_u16(event.code), KeyCode::A);
        assert_eq!(PressState::from_i32(event.value), Some(PressState::Pressed));
        assert_eq!(ValueMode::from_flags(event.flags), ValueMode::Absolute);
        assert!(event.reserved_is_zero());
    }
}
