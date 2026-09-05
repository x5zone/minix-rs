//! Scancodes: press/release split, escape states, key mapping.
//!
//! C correspondence: `kbd_process` with `kbd_state` (`pckbd.c:328-368`),
//! the release bit `SCAN_RELEASE 0x80`, the escape prefixes `SCAN_EXT0
//! 0xE0` and `SCAN_EXT1 0xE1`, the pause path through `SCAN_CTRL 0x1D`
//! and `SCAN_NUMLOCK 0x45` (`pckbd.h:31-35`), and the normal plus escaped
//! tables in `table.c` (page plus code per scancode).

/// Release bit: set means key release, clear means key press.
///
/// C: `SCAN_RELEASE 0x80` (`pckbd.h:31`).
pub const RELEASE_BIT: u8 = 0x80;

/// Escape prefix for extended keys.
///
/// C: `SCAN_EXT0 0xE0` (`pckbd.h:34`).
pub const EXTEND_0: u8 = 0xE0;
/// Escape prefix starting the pause sequence.
///
/// C: `SCAN_EXT1 0xE1` (`pckbd.h:35`).
pub const EXTEND_1: u8 = 0xE1;
/// Control key index inside the pause sequence.
///
/// C: `SCAN_CTRL 0x1D` (`pckbd.h:32`).
pub const CONTROL_INDEX: u8 = 0x1D;
/// Num-lock index completing the pause sequence.
///
/// C: `SCAN_NUMLOCK 0x45` (`pckbd.h:33`).
pub const NUMLOCK_INDEX: u8 = 0x45;

/// Press or release of one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Key went down.
    Down,
    /// Key came up.
    Up,
}

/// Mapped key: usage page plus usage code.
///
/// C: the `page` plus `code` pair from `scanmap_normal` and
/// `scanmap_escaped` (`table.c`). A zero page means "no event" (padding
/// entries in the C tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyCode {
    /// Usage page (zero means no event).
    pub page: u16,
    /// Usage code within the page.
    pub code: u16,
}

impl KeyCode {
    /// True when this entry produces an event.
    pub const fn is_event(self) -> bool {
        self.page != 0
    }
}

/// Keyboard usage page (matches the input server vocabulary).
///
/// C: `INPUT_PAGE_KEY 0x0007` (`input.h:36`).
pub const PAGE_KEY: u16 = 0x0007;

/// Pause key code on the keyboard page.
pub const KEY_PAUSE: u16 = 0x0048;

/// Escape key on the keyboard page (spot-check value from the table).
pub const KEY_ESCAPE: u16 = 0x0029;

/// Enter key on the keyboard page (spot-check value from the table).
pub const KEY_ENTER: u16 = 0x0028;

/// Outcome of feeding one scancode byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedOutcome {
    /// Prefix consumed, waiting for the next byte.
    NeedMore,
    /// One key event produced.
    Event(KeyCode, Press),
    /// Byte consumed with no event (padding entry).
    Swallowed,
}

/// Scancode state machine: normal, escaped, pause-armed, pause-confirm.
///
/// C: `kbd_state` (`pckbd.c:26`) with states zero through three
/// (`kbd_process`, `pckbd.c:335-367`). Prefix bytes never produce events
/// by themselves; the pause key resolves only after its full six-byte
/// prelude (extend-one, control press and release are swallowed).
pub struct ScancodeState {
    state: u8,
}

impl ScancodeState {
    /// Fresh machine (normal state).
    pub const fn new() -> ScancodeState {
        ScancodeState { state: 0 }
    }

    /// Feed one scancode byte, looking keys up in `map`.
    pub fn feed<M: KeyMap>(&mut self, map: &M, byte: u8) -> FeedOutcome {
        let press = if byte & RELEASE_BIT == 0 {
            Press::Down
        } else {
            Press::Up
        };
        let index = byte & !RELEASE_BIT;
        match self.state {
            1 => {
                self.state = 0;
                let key = map.escaped(index);
                emit(key, press)
            }
            2 => {
                self.state = if index == CONTROL_INDEX { 3 } else { 0 };
                FeedOutcome::NeedMore
            }
            3 => {
                self.state = 0;
                if index == NUMLOCK_INDEX {
                    return FeedOutcome::Event(
                        KeyCode {
                            page: PAGE_KEY,
                            code: KEY_PAUSE,
                        },
                        press,
                    );
                }
                FeedOutcome::Swallowed
            }
            _ => match byte {
                EXTEND_0 => {
                    self.state = 1;
                    FeedOutcome::NeedMore
                }
                EXTEND_1 => {
                    self.state = 2;
                    FeedOutcome::NeedMore
                }
                _ => {
                    let key = map.normal(index);
                    emit(key, press)
                }
            },
        }
    }
}

impl Default for ScancodeState {
    fn default() -> Self {
        ScancodeState::new()
    }
}

fn emit(key: KeyCode, press: Press) -> FeedOutcome {
    if key.is_event() {
        FeedOutcome::Event(key, press)
    } else {
        FeedOutcome::Swallowed
    }
}

/// Key lookup for both tables.
pub trait KeyMap {
    /// Normal (unescaped) table entry.
    fn normal(&self, index: u8) -> KeyCode;
    /// Escaped table entry (after the extend-zero prefix).
    fn escaped(&self, index: u8) -> KeyCode;
}

/// Reference map: a handful of real entries from `table.c`, padding
/// elsewhere. Enough to prove the machine; the full table lives with the
/// service data (it is data, not policy).
#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceMap;

impl KeyMap for ReferenceMap {
    fn normal(&self, index: u8) -> KeyCode {
        match index {
            0x01 => KeyCode {
                page: PAGE_KEY,
                code: KEY_ESCAPE,
            },
            0x1C => KeyCode {
                page: PAGE_KEY,
                code: KEY_ENTER,
            },
            0x39 => KeyCode {
                page: PAGE_KEY,
                code: 0x002C,
            },
            _ => KeyCode { page: 0, code: 0 },
        }
    }

    fn escaped(&self, index: u8) -> KeyCode {
        match index {
            0x1C => KeyCode {
                page: PAGE_KEY,
                code: 0x0058,
            },
            _ => KeyCode { page: 0, code: 0 },
        }
    }
}

/// Empty map: every entry is padding (proves the swallow path).
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyMap;

impl KeyMap for EmptyMap {
    fn normal(&self, _index: u8) -> KeyCode {
        KeyCode { page: 0, code: 0 }
    }

    fn escaped(&self, _index: u8) -> KeyCode {
        KeyCode { page: 0, code: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_press_and_release_split_on_high_bit() {
        let map = ReferenceMap;
        let mut state = ScancodeState::new();
        assert_eq!(
            state.feed(&map, 0x1C),
            FeedOutcome::Event(
                KeyCode {
                    page: PAGE_KEY,
                    code: KEY_ENTER
                },
                Press::Down
            )
        );
        assert_eq!(
            state.feed(&map, 0x1C | RELEASE_BIT),
            FeedOutcome::Event(
                KeyCode {
                    page: PAGE_KEY,
                    code: KEY_ENTER
                },
                Press::Up
            )
        );
    }

    #[test]
    fn test_escaped_prefix_routes_to_escaped_table() {
        let map = ReferenceMap;
        let mut state = ScancodeState::new();
        assert_eq!(state.feed(&map, EXTEND_0), FeedOutcome::NeedMore);
        assert!(matches!(
            state.feed(&map, 0x1C),
            FeedOutcome::Event(_, Press::Down)
        ));
    }

    #[test]
    fn test_pause_sequence_needs_full_prelude() {
        let map = ReferenceMap;
        let mut state = ScancodeState::new();
        assert_eq!(state.feed(&map, EXTEND_1), FeedOutcome::NeedMore);
        assert_eq!(state.feed(&map, CONTROL_INDEX), FeedOutcome::NeedMore);
        assert_eq!(
            state.feed(&map, NUMLOCK_INDEX),
            FeedOutcome::Event(
                KeyCode {
                    page: PAGE_KEY,
                    code: KEY_PAUSE
                },
                Press::Down
            )
        );
    }

    #[test]
    fn test_broken_pause_prelude_resets() {
        let map = ReferenceMap;
        let mut state = ScancodeState::new();
        state.feed(&map, EXTEND_1);
        state.feed(&map, 0x10);
        assert_eq!(
            state.feed(&map, 0x1C),
            FeedOutcome::Event(
                KeyCode {
                    page: PAGE_KEY,
                    code: KEY_ENTER
                },
                Press::Down
            )
        );
    }

    #[test]
    fn test_padding_entries_are_swallowed() {
        let map = EmptyMap;
        let mut state = ScancodeState::new();
        assert_eq!(state.feed(&map, 0x1C), FeedOutcome::Swallowed);
    }

    #[test]
    fn test_constants_match_c_headers() {
        assert_eq!(RELEASE_BIT, 0x80);
        assert_eq!(EXTEND_0, 0xE0);
        assert_eq!(PAGE_KEY, 0x0007);
    }
}
