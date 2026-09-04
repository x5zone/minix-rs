//! Keyboard and keypad event codes (USB HID Usage Table, keyboard page).
//!
//! C: `minix3/minix/include/minix/input.h:59-290` (the `INPUT_KEY_*` enumerators).
//! Every constant below is mechanically derived from that header: the numeric
//! value is part of the wire contract (drivers report these numbers, readers
//! interpret them), so each value is locked by `key_code_values_match_c`.
//! To regenerate, re-run `tools/gen-input-keycodes.py` and then `cargo test`.
//!
//! Names drop the `INPUT_KEY_` prefix and stay UPPER_SNAKE_CASE
//! (digit keys gain a `NUM_` prefix: `INPUT_KEY_1` becomes `KeyCode::NUM_1`);
//! the numeric value is unchanged. Reserved gaps in the C numbering (`0x00A5-0x00AF`,
//! `0x00DE-0x00DF`, `0x00E8-0xFFFF`) have no constant here either:
//! `KeyCode::is_defined` returns `false` for them, exactly matching the set
//! the C header names.

/// A keyboard-page event code as it appears on the wire (`code` field).
///
/// C passes these numbers through uninterpreted (`input.c` never validates
/// `code`); the meaning lives with the reader (terminal driver, window system).
/// The newtype keeps that transparency: any `u16` can travel, while the named
/// constants below give the defined subset a readable spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyCode(pub u16);

impl KeyCode {
    /// C: `INPUT_KEY_A = 0x0004` (`input.h`).
    pub const A: KeyCode = KeyCode(0x0004);
    /// C: `INPUT_KEY_B = 0x0005` (`input.h`).
    pub const B: KeyCode = KeyCode(0x0005);
    /// C: `INPUT_KEY_C = 0x0006` (`input.h`).
    pub const C: KeyCode = KeyCode(0x0006);
    /// C: `INPUT_KEY_D = 0x0007` (`input.h`).
    pub const D: KeyCode = KeyCode(0x0007);
    /// C: `INPUT_KEY_E = 0x0008` (`input.h`).
    pub const E: KeyCode = KeyCode(0x0008);
    /// C: `INPUT_KEY_F = 0x0009` (`input.h`).
    pub const F: KeyCode = KeyCode(0x0009);
    /// C: `INPUT_KEY_G = 0x000A` (`input.h`).
    pub const G: KeyCode = KeyCode(0x000A);
    /// C: `INPUT_KEY_H = 0x000B` (`input.h`).
    pub const H: KeyCode = KeyCode(0x000B);
    /// C: `INPUT_KEY_I = 0x000C` (`input.h`).
    pub const I: KeyCode = KeyCode(0x000C);
    /// C: `INPUT_KEY_J = 0x000D` (`input.h`).
    pub const J: KeyCode = KeyCode(0x000D);
    /// C: `INPUT_KEY_K = 0x000E` (`input.h`).
    pub const K: KeyCode = KeyCode(0x000E);
    /// C: `INPUT_KEY_L = 0x000F` (`input.h`).
    pub const L: KeyCode = KeyCode(0x000F);
    /// C: `INPUT_KEY_M = 0x0010` (`input.h`).
    pub const M: KeyCode = KeyCode(0x0010);
    /// C: `INPUT_KEY_N = 0x0011` (`input.h`).
    pub const N: KeyCode = KeyCode(0x0011);
    /// C: `INPUT_KEY_O = 0x0012` (`input.h`).
    pub const O: KeyCode = KeyCode(0x0012);
    /// C: `INPUT_KEY_P = 0x0013` (`input.h`).
    pub const P: KeyCode = KeyCode(0x0013);
    /// C: `INPUT_KEY_Q = 0x0014` (`input.h`).
    pub const Q: KeyCode = KeyCode(0x0014);
    /// C: `INPUT_KEY_R = 0x0015` (`input.h`).
    pub const R: KeyCode = KeyCode(0x0015);
    /// C: `INPUT_KEY_S = 0x0016` (`input.h`).
    pub const S: KeyCode = KeyCode(0x0016);
    /// C: `INPUT_KEY_T = 0x0017` (`input.h`).
    pub const T: KeyCode = KeyCode(0x0017);
    /// C: `INPUT_KEY_U = 0x0018` (`input.h`).
    pub const U: KeyCode = KeyCode(0x0018);
    /// C: `INPUT_KEY_V = 0x0019` (`input.h`).
    pub const V: KeyCode = KeyCode(0x0019);
    /// C: `INPUT_KEY_W = 0x001A` (`input.h`).
    pub const W: KeyCode = KeyCode(0x001A);
    /// C: `INPUT_KEY_X = 0x001B` (`input.h`).
    pub const X: KeyCode = KeyCode(0x001B);
    /// C: `INPUT_KEY_Y = 0x001C` (`input.h`).
    pub const Y: KeyCode = KeyCode(0x001C);
    /// C: `INPUT_KEY_Z = 0x001D` (`input.h`).
    pub const Z: KeyCode = KeyCode(0x001D);
    /// C: `INPUT_KEY_1 = 0x001E` (`input.h`).
    pub const NUM_1: KeyCode = KeyCode(0x001E);
    /// C: `INPUT_KEY_2 = 0x001F` (`input.h`).
    pub const NUM_2: KeyCode = KeyCode(0x001F);
    /// C: `INPUT_KEY_3 = 0x0020` (`input.h`).
    pub const NUM_3: KeyCode = KeyCode(0x0020);
    /// C: `INPUT_KEY_4 = 0x0021` (`input.h`).
    pub const NUM_4: KeyCode = KeyCode(0x0021);
    /// C: `INPUT_KEY_5 = 0x0022` (`input.h`).
    pub const NUM_5: KeyCode = KeyCode(0x0022);
    /// C: `INPUT_KEY_6 = 0x0023` (`input.h`).
    pub const NUM_6: KeyCode = KeyCode(0x0023);
    /// C: `INPUT_KEY_7 = 0x0024` (`input.h`).
    pub const NUM_7: KeyCode = KeyCode(0x0024);
    /// C: `INPUT_KEY_8 = 0x0025` (`input.h`).
    pub const NUM_8: KeyCode = KeyCode(0x0025);
    /// C: `INPUT_KEY_9 = 0x0026` (`input.h`).
    pub const NUM_9: KeyCode = KeyCode(0x0026);
    /// C: `INPUT_KEY_0 = 0x0027` (`input.h`).
    pub const NUM_0: KeyCode = KeyCode(0x0027);
    /// C: `INPUT_KEY_ENTER = 0x0028` (`input.h`).
    pub const ENTER: KeyCode = KeyCode(0x0028);
    /// C: `INPUT_KEY_ESCAPE = 0x0029` (`input.h`).
    pub const ESCAPE: KeyCode = KeyCode(0x0029);
    /// C: `INPUT_KEY_BACKSPACE = 0x002A` (`input.h`).
    pub const BACKSPACE: KeyCode = KeyCode(0x002A);
    /// C: `INPUT_KEY_TAB = 0x002B` (`input.h`).
    pub const TAB: KeyCode = KeyCode(0x002B);
    /// C: `INPUT_KEY_SPACEBAR = 0x002C` (`input.h`).
    pub const SPACEBAR: KeyCode = KeyCode(0x002C);
    /// C: `INPUT_KEY_DASH = 0x002D` (`input.h`).
    pub const DASH: KeyCode = KeyCode(0x002D);
    /// C: `INPUT_KEY_EQUAL = 0x002E` (`input.h`).
    pub const EQUAL: KeyCode = KeyCode(0x002E);
    /// C: `INPUT_KEY_OPEN_BRACKET = 0x002F` (`input.h`).
    pub const OPEN_BRACKET: KeyCode = KeyCode(0x002F);
    /// C: `INPUT_KEY_CLOSE_BRACKET = 0x0030` (`input.h`).
    pub const CLOSE_BRACKET: KeyCode = KeyCode(0x0030);
    /// C: `INPUT_KEY_BACKSLASH = 0x0031` (`input.h`).
    pub const BACKSLASH: KeyCode = KeyCode(0x0031);
    /// C: `INPUT_KEY_EUROPE_1 = 0x0032` (`input.h`).
    pub const EUROPE_1: KeyCode = KeyCode(0x0032);
    /// C: `INPUT_KEY_SEMICOLON = 0x0033` (`input.h`).
    pub const SEMICOLON: KeyCode = KeyCode(0x0033);
    /// C: `INPUT_KEY_APOSTROPH = 0x0034` (`input.h`).
    pub const APOSTROPH: KeyCode = KeyCode(0x0034);
    /// C: `INPUT_KEY_GRAVE_ACCENT = 0x0035` (`input.h`).
    pub const GRAVE_ACCENT: KeyCode = KeyCode(0x0035);
    /// C: `INPUT_KEY_COMMA = 0x0036` (`input.h`).
    pub const COMMA: KeyCode = KeyCode(0x0036);
    /// C: `INPUT_KEY_PERIOD = 0x0037` (`input.h`).
    pub const PERIOD: KeyCode = KeyCode(0x0037);
    /// C: `INPUT_KEY_SLASH = 0x0038` (`input.h`).
    pub const SLASH: KeyCode = KeyCode(0x0038);
    /// C: `INPUT_KEY_CAPS_LOCK = 0x0039` (`input.h`).
    pub const CAPS_LOCK: KeyCode = KeyCode(0x0039);
    /// C: `INPUT_KEY_F1 = 0x003A` (`input.h`).
    pub const F1: KeyCode = KeyCode(0x003A);
    /// C: `INPUT_KEY_F2 = 0x003B` (`input.h`).
    pub const F2: KeyCode = KeyCode(0x003B);
    /// C: `INPUT_KEY_F3 = 0x003C` (`input.h`).
    pub const F3: KeyCode = KeyCode(0x003C);
    /// C: `INPUT_KEY_F4 = 0x003D` (`input.h`).
    pub const F4: KeyCode = KeyCode(0x003D);
    /// C: `INPUT_KEY_F5 = 0x003E` (`input.h`).
    pub const F5: KeyCode = KeyCode(0x003E);
    /// C: `INPUT_KEY_F6 = 0x003F` (`input.h`).
    pub const F6: KeyCode = KeyCode(0x003F);
    /// C: `INPUT_KEY_F7 = 0x0040` (`input.h`).
    pub const F7: KeyCode = KeyCode(0x0040);
    /// C: `INPUT_KEY_F8 = 0x0041` (`input.h`).
    pub const F8: KeyCode = KeyCode(0x0041);
    /// C: `INPUT_KEY_F9 = 0x0042` (`input.h`).
    pub const F9: KeyCode = KeyCode(0x0042);
    /// C: `INPUT_KEY_F10 = 0x0043` (`input.h`).
    pub const F10: KeyCode = KeyCode(0x0043);
    /// C: `INPUT_KEY_F11 = 0x0044` (`input.h`).
    pub const F11: KeyCode = KeyCode(0x0044);
    /// C: `INPUT_KEY_F12 = 0x0045` (`input.h`).
    pub const F12: KeyCode = KeyCode(0x0045);
    /// C: `INPUT_KEY_PRINT_SCREEN = 0x0046` (`input.h`).
    pub const PRINT_SCREEN: KeyCode = KeyCode(0x0046);
    /// C: `INPUT_KEY_SCROLL_LOCK = 0x0047` (`input.h`).
    pub const SCROLL_LOCK: KeyCode = KeyCode(0x0047);
    /// C: `INPUT_KEY_PAUSE = 0x0048` (`input.h`).
    pub const PAUSE: KeyCode = KeyCode(0x0048);
    /// C: `INPUT_KEY_INSERT = 0x0049` (`input.h`).
    pub const INSERT: KeyCode = KeyCode(0x0049);
    /// C: `INPUT_KEY_HOME = 0x004A` (`input.h`).
    pub const HOME: KeyCode = KeyCode(0x004A);
    /// C: `INPUT_KEY_PAGE_UP = 0x004B` (`input.h`).
    pub const PAGE_UP: KeyCode = KeyCode(0x004B);
    /// C: `INPUT_KEY_DELETE = 0x004C` (`input.h`).
    pub const DELETE: KeyCode = KeyCode(0x004C);
    /// C: `INPUT_KEY_END = 0x004D` (`input.h`).
    pub const END: KeyCode = KeyCode(0x004D);
    /// C: `INPUT_KEY_PAGE_DOWN = 0x004E` (`input.h`).
    pub const PAGE_DOWN: KeyCode = KeyCode(0x004E);
    /// C: `INPUT_KEY_RIGHT_ARROW = 0x004F` (`input.h`).
    pub const RIGHT_ARROW: KeyCode = KeyCode(0x004F);
    /// C: `INPUT_KEY_LEFT_ARROW = 0x0050` (`input.h`).
    pub const LEFT_ARROW: KeyCode = KeyCode(0x0050);
    /// C: `INPUT_KEY_DOWN_ARROW = 0x0051` (`input.h`).
    pub const DOWN_ARROW: KeyCode = KeyCode(0x0051);
    /// C: `INPUT_KEY_UP_ARROW = 0x0052` (`input.h`).
    pub const UP_ARROW: KeyCode = KeyCode(0x0052);
    /// C: `INPUT_KEY_NUM_LOCK = 0x0053` (`input.h`).
    pub const NUM_LOCK: KeyCode = KeyCode(0x0053);
    /// C: `INPUT_KEY_KP_SLASH = 0x0054` (`input.h`).
    pub const KP_SLASH: KeyCode = KeyCode(0x0054);
    /// C: `INPUT_KEY_KP_STAR = 0x0055` (`input.h`).
    pub const KP_STAR: KeyCode = KeyCode(0x0055);
    /// C: `INPUT_KEY_KP_DASH = 0x0056` (`input.h`).
    pub const KP_DASH: KeyCode = KeyCode(0x0056);
    /// C: `INPUT_KEY_KP_PLUS = 0x0057` (`input.h`).
    pub const KP_PLUS: KeyCode = KeyCode(0x0057);
    /// C: `INPUT_KEY_KP_ENTER = 0x0058` (`input.h`).
    pub const KP_ENTER: KeyCode = KeyCode(0x0058);
    /// C: `INPUT_KEY_KP_1 = 0x0059` (`input.h`).
    pub const KP_1: KeyCode = KeyCode(0x0059);
    /// C: `INPUT_KEY_KP_2 = 0x005A` (`input.h`).
    pub const KP_2: KeyCode = KeyCode(0x005A);
    /// C: `INPUT_KEY_KP_3 = 0x005B` (`input.h`).
    pub const KP_3: KeyCode = KeyCode(0x005B);
    /// C: `INPUT_KEY_KP_4 = 0x005C` (`input.h`).
    pub const KP_4: KeyCode = KeyCode(0x005C);
    /// C: `INPUT_KEY_KP_5 = 0x005D` (`input.h`).
    pub const KP_5: KeyCode = KeyCode(0x005D);
    /// C: `INPUT_KEY_KP_6 = 0x005E` (`input.h`).
    pub const KP_6: KeyCode = KeyCode(0x005E);
    /// C: `INPUT_KEY_KP_7 = 0x005F` (`input.h`).
    pub const KP_7: KeyCode = KeyCode(0x005F);
    /// C: `INPUT_KEY_KP_8 = 0x0060` (`input.h`).
    pub const KP_8: KeyCode = KeyCode(0x0060);
    /// C: `INPUT_KEY_KP_9 = 0x0061` (`input.h`).
    pub const KP_9: KeyCode = KeyCode(0x0061);
    /// C: `INPUT_KEY_KP_0 = 0x0062` (`input.h`).
    pub const KP_0: KeyCode = KeyCode(0x0062);
    /// C: `INPUT_KEY_KP_PERIOD = 0x0063` (`input.h`).
    pub const KP_PERIOD: KeyCode = KeyCode(0x0063);
    /// C: `INPUT_KEY_EUROPE_2 = 0x0064` (`input.h`).
    pub const EUROPE_2: KeyCode = KeyCode(0x0064);
    /// C: `INPUT_KEY_APPLICATION = 0x0065` (`input.h`).
    pub const APPLICATION: KeyCode = KeyCode(0x0065);
    /// C: `INPUT_KEY_POWER = 0x0066` (`input.h`).
    pub const POWER: KeyCode = KeyCode(0x0066);
    /// C: `INPUT_KEY_KP_EQUAL = 0x0067` (`input.h`).
    pub const KP_EQUAL: KeyCode = KeyCode(0x0067);
    /// C: `INPUT_KEY_F13 = 0x0068` (`input.h`).
    pub const F13: KeyCode = KeyCode(0x0068);
    /// C: `INPUT_KEY_F14 = 0x0069` (`input.h`).
    pub const F14: KeyCode = KeyCode(0x0069);
    /// C: `INPUT_KEY_F15 = 0x006A` (`input.h`).
    pub const F15: KeyCode = KeyCode(0x006A);
    /// C: `INPUT_KEY_F16 = 0x006B` (`input.h`).
    pub const F16: KeyCode = KeyCode(0x006B);
    /// C: `INPUT_KEY_F17 = 0x006C` (`input.h`).
    pub const F17: KeyCode = KeyCode(0x006C);
    /// C: `INPUT_KEY_F18 = 0x006D` (`input.h`).
    pub const F18: KeyCode = KeyCode(0x006D);
    /// C: `INPUT_KEY_F19 = 0x006E` (`input.h`).
    pub const F19: KeyCode = KeyCode(0x006E);
    /// C: `INPUT_KEY_F20 = 0x006F` (`input.h`).
    pub const F20: KeyCode = KeyCode(0x006F);
    /// C: `INPUT_KEY_F21 = 0x0070` (`input.h`).
    pub const F21: KeyCode = KeyCode(0x0070);
    /// C: `INPUT_KEY_F22 = 0x0071` (`input.h`).
    pub const F22: KeyCode = KeyCode(0x0071);
    /// C: `INPUT_KEY_F23 = 0x0072` (`input.h`).
    pub const F23: KeyCode = KeyCode(0x0072);
    /// C: `INPUT_KEY_F24 = 0x0073` (`input.h`).
    pub const F24: KeyCode = KeyCode(0x0073);
    /// C: `INPUT_KEY_EXECUTE = 0x0074` (`input.h`).
    pub const EXECUTE: KeyCode = KeyCode(0x0074);
    /// C: `INPUT_KEY_HELP = 0x0075` (`input.h`).
    pub const HELP: KeyCode = KeyCode(0x0075);
    /// C: `INPUT_KEY_MENU = 0x0076` (`input.h`).
    pub const MENU: KeyCode = KeyCode(0x0076);
    /// C: `INPUT_KEY_SELECT = 0x0077` (`input.h`).
    pub const SELECT: KeyCode = KeyCode(0x0077);
    /// C: `INPUT_KEY_STOP = 0x0078` (`input.h`).
    pub const STOP: KeyCode = KeyCode(0x0078);
    /// C: `INPUT_KEY_AGAIN = 0x0079` (`input.h`).
    pub const AGAIN: KeyCode = KeyCode(0x0079);
    /// C: `INPUT_KEY_UNDO = 0x007A` (`input.h`).
    pub const UNDO: KeyCode = KeyCode(0x007A);
    /// C: `INPUT_KEY_CUT = 0x007B` (`input.h`).
    pub const CUT: KeyCode = KeyCode(0x007B);
    /// C: `INPUT_KEY_COPY = 0x007C` (`input.h`).
    pub const COPY: KeyCode = KeyCode(0x007C);
    /// C: `INPUT_KEY_PASTE = 0x007D` (`input.h`).
    pub const PASTE: KeyCode = KeyCode(0x007D);
    /// C: `INPUT_KEY_FIND = 0x007E` (`input.h`).
    pub const FIND: KeyCode = KeyCode(0x007E);
    /// C: `INPUT_KEY_MUTE = 0x007F` (`input.h`).
    pub const MUTE: KeyCode = KeyCode(0x007F);
    /// C: `INPUT_KEY_VOLUME_UP = 0x0080` (`input.h`).
    pub const VOLUME_UP: KeyCode = KeyCode(0x0080);
    /// C: `INPUT_KEY_VOLUME_DOWN = 0x0081` (`input.h`).
    pub const VOLUME_DOWN: KeyCode = KeyCode(0x0081);
    /// C: `INPUT_KEY_LOCKING_CAPS_LOCK = 0x0082` (`input.h`).
    pub const LOCKING_CAPS_LOCK: KeyCode = KeyCode(0x0082);
    /// C: `INPUT_KEY_LOCKING_NUM_LOCK = 0x0083` (`input.h`).
    pub const LOCKING_NUM_LOCK: KeyCode = KeyCode(0x0083);
    /// C: `INPUT_KEY_LOCKING_SCROLL_LOCK = 0x0084` (`input.h`).
    pub const LOCKING_SCROLL_LOCK: KeyCode = KeyCode(0x0084);
    /// C: `INPUT_KEY_KP_COMMA = 0x0085` (`input.h`).
    pub const KP_COMMA: KeyCode = KeyCode(0x0085);
    /// C: `INPUT_KEY_EQUAL_SIGN = 0x0086` (`input.h`).
    pub const EQUAL_SIGN: KeyCode = KeyCode(0x0086);
    /// C: `INPUT_KEY_I10L_1 = 0x0087` (`input.h`).
    pub const I10L_1: KeyCode = KeyCode(0x0087);
    /// C: `INPUT_KEY_I10L_2 = 0x0088` (`input.h`).
    pub const I10L_2: KeyCode = KeyCode(0x0088);
    /// C: `INPUT_KEY_I10L_3 = 0x0089` (`input.h`).
    pub const I10L_3: KeyCode = KeyCode(0x0089);
    /// C: `INPUT_KEY_I10L_4 = 0x008A` (`input.h`).
    pub const I10L_4: KeyCode = KeyCode(0x008A);
    /// C: `INPUT_KEY_I10L_5 = 0x008B` (`input.h`).
    pub const I10L_5: KeyCode = KeyCode(0x008B);
    /// C: `INPUT_KEY_I10L_6 = 0x008C` (`input.h`).
    pub const I10L_6: KeyCode = KeyCode(0x008C);
    /// C: `INPUT_KEY_I10L_7 = 0x008D` (`input.h`).
    pub const I10L_7: KeyCode = KeyCode(0x008D);
    /// C: `INPUT_KEY_I10L_8 = 0x008E` (`input.h`).
    pub const I10L_8: KeyCode = KeyCode(0x008E);
    /// C: `INPUT_KEY_I10L_9 = 0x008F` (`input.h`).
    pub const I10L_9: KeyCode = KeyCode(0x008F);
    /// C: `INPUT_KEY_LANG_1 = 0x0090` (`input.h`).
    pub const LANG_1: KeyCode = KeyCode(0x0090);
    /// C: `INPUT_KEY_LANG_2 = 0x0091` (`input.h`).
    pub const LANG_2: KeyCode = KeyCode(0x0091);
    /// C: `INPUT_KEY_LANG_3 = 0x0092` (`input.h`).
    pub const LANG_3: KeyCode = KeyCode(0x0092);
    /// C: `INPUT_KEY_LANG_4 = 0x0093` (`input.h`).
    pub const LANG_4: KeyCode = KeyCode(0x0093);
    /// C: `INPUT_KEY_LANG_5 = 0x0094` (`input.h`).
    pub const LANG_5: KeyCode = KeyCode(0x0094);
    /// C: `INPUT_KEY_LANG_6 = 0x0095` (`input.h`).
    pub const LANG_6: KeyCode = KeyCode(0x0095);
    /// C: `INPUT_KEY_LANG_7 = 0x0096` (`input.h`).
    pub const LANG_7: KeyCode = KeyCode(0x0096);
    /// C: `INPUT_KEY_LANG_8 = 0x0097` (`input.h`).
    pub const LANG_8: KeyCode = KeyCode(0x0097);
    /// C: `INPUT_KEY_LANG_9 = 0x0098` (`input.h`).
    pub const LANG_9: KeyCode = KeyCode(0x0098);
    /// C: `INPUT_KEY_ALT_ERASE = 0x0099` (`input.h`).
    pub const ALT_ERASE: KeyCode = KeyCode(0x0099);
    /// C: `INPUT_KEY_SYSREQ = 0x009A` (`input.h`).
    pub const SYSREQ: KeyCode = KeyCode(0x009A);
    /// C: `INPUT_KEY_CANCEL = 0x009B` (`input.h`).
    pub const CANCEL: KeyCode = KeyCode(0x009B);
    /// C: `INPUT_KEY_CLEAR = 0x009C` (`input.h`).
    pub const CLEAR: KeyCode = KeyCode(0x009C);
    /// C: `INPUT_KEY_PRIOR = 0x009D` (`input.h`).
    pub const PRIOR: KeyCode = KeyCode(0x009D);
    /// C: `INPUT_KEY_RETURN = 0x009E` (`input.h`).
    pub const RETURN: KeyCode = KeyCode(0x009E);
    /// C: `INPUT_KEY_SEPARATOR = 0x009F` (`input.h`).
    pub const SEPARATOR: KeyCode = KeyCode(0x009F);
    /// C: `INPUT_KEY_OUT = 0x00A0` (`input.h`).
    pub const OUT: KeyCode = KeyCode(0x00A0);
    /// C: `INPUT_KEY_OPER = 0x00A1` (`input.h`).
    pub const OPER: KeyCode = KeyCode(0x00A1);
    /// C: `INPUT_KEY_CLEAR_AGAIN = 0x00A2` (`input.h`).
    pub const CLEAR_AGAIN: KeyCode = KeyCode(0x00A2);
    /// C: `INPUT_KEY_CR_SEL = 0x00A3` (`input.h`).
    pub const CR_SEL: KeyCode = KeyCode(0x00A3);
    /// C: `INPUT_KEY_EX_SEL = 0x00A4` (`input.h`).
    pub const EX_SEL: KeyCode = KeyCode(0x00A4);
    /// C: `INPUT_KEY_KP_00 = 0x00B0` (`input.h`).
    pub const KP_00: KeyCode = KeyCode(0x00B0);
    /// C: `INPUT_KEY_KP_000 = 0x00B1` (`input.h`).
    pub const KP_000: KeyCode = KeyCode(0x00B1);
    /// C: `INPUT_KEY_THOUSANDS_SEP = 0x00B2` (`input.h`).
    pub const THOUSANDS_SEP: KeyCode = KeyCode(0x00B2);
    /// C: `INPUT_KEY_DECIMAL_SEP = 0x00B3` (`input.h`).
    pub const DECIMAL_SEP: KeyCode = KeyCode(0x00B3);
    /// C: `INPUT_KEY_CURRENCY_UNIT = 0x00B4` (`input.h`).
    pub const CURRENCY_UNIT: KeyCode = KeyCode(0x00B4);
    /// C: `INPUT_KEY_CURRENCY_SUBUNIT = 0x00B5` (`input.h`).
    pub const CURRENCY_SUBUNIT: KeyCode = KeyCode(0x00B5);
    /// C: `INPUT_KEY_KP_OPEN_PARENTHESIS = 0x00B6` (`input.h`).
    pub const KP_OPEN_PARENTHESIS: KeyCode = KeyCode(0x00B6);
    /// C: `INPUT_KEY_KP_CLOSE_PARENTHESIS = 0x00B7` (`input.h`).
    pub const KP_CLOSE_PARENTHESIS: KeyCode = KeyCode(0x00B7);
    /// C: `INPUT_KEY_KP_OPEN_BRACE = 0x00B8` (`input.h`).
    pub const KP_OPEN_BRACE: KeyCode = KeyCode(0x00B8);
    /// C: `INPUT_KEY_KP_CLOSE_BRACE = 0x00B9` (`input.h`).
    pub const KP_CLOSE_BRACE: KeyCode = KeyCode(0x00B9);
    /// C: `INPUT_KEY_KP_TAB = 0x00BA` (`input.h`).
    pub const KP_TAB: KeyCode = KeyCode(0x00BA);
    /// C: `INPUT_KEY_KP_BACKSPACE = 0x00BB` (`input.h`).
    pub const KP_BACKSPACE: KeyCode = KeyCode(0x00BB);
    /// C: `INPUT_KEY_KP_A = 0x00BC` (`input.h`).
    pub const KP_A: KeyCode = KeyCode(0x00BC);
    /// C: `INPUT_KEY_KP_B = 0x00BD` (`input.h`).
    pub const KP_B: KeyCode = KeyCode(0x00BD);
    /// C: `INPUT_KEY_KP_C = 0x00BE` (`input.h`).
    pub const KP_C: KeyCode = KeyCode(0x00BE);
    /// C: `INPUT_KEY_KP_D = 0x00BF` (`input.h`).
    pub const KP_D: KeyCode = KeyCode(0x00BF);
    /// C: `INPUT_KEY_KP_E = 0x00C0` (`input.h`).
    pub const KP_E: KeyCode = KeyCode(0x00C0);
    /// C: `INPUT_KEY_KP_F = 0x00C1` (`input.h`).
    pub const KP_F: KeyCode = KeyCode(0x00C1);
    /// C: `INPUT_KEY_KP_XOR = 0x00C2` (`input.h`).
    pub const KP_XOR: KeyCode = KeyCode(0x00C2);
    /// C: `INPUT_KEY_KP_CARET = 0x00C3` (`input.h`).
    pub const KP_CARET: KeyCode = KeyCode(0x00C3);
    /// C: `INPUT_KEY_KP_PERCENT = 0x00C4` (`input.h`).
    pub const KP_PERCENT: KeyCode = KeyCode(0x00C4);
    /// C: `INPUT_KEY_KP_SMALLER_THEN = 0x00C5` (`input.h`).
    pub const KP_SMALLER_THEN: KeyCode = KeyCode(0x00C5);
    /// C: `INPUT_KEY_KP_GREATER_THEN = 0x00C6` (`input.h`).
    pub const KP_GREATER_THEN: KeyCode = KeyCode(0x00C6);
    /// C: `INPUT_KEY_KP_AMP = 0x00C7` (`input.h`).
    pub const KP_AMP: KeyCode = KeyCode(0x00C7);
    /// C: `INPUT_KEY_KP_DOUBLE_AMP = 0x00C8` (`input.h`).
    pub const KP_DOUBLE_AMP: KeyCode = KeyCode(0x00C8);
    /// C: `INPUT_KEY_KP_PIPE = 0x00C9` (`input.h`).
    pub const KP_PIPE: KeyCode = KeyCode(0x00C9);
    /// C: `INPUT_KEY_KP_DOUBLE_PIPE = 0x00CA` (`input.h`).
    pub const KP_DOUBLE_PIPE: KeyCode = KeyCode(0x00CA);
    /// C: `INPUT_KEY_KP_COLON = 0x00CB` (`input.h`).
    pub const KP_COLON: KeyCode = KeyCode(0x00CB);
    /// C: `INPUT_KEY_KP_NUMBER = 0x00CC` (`input.h`).
    pub const KP_NUMBER: KeyCode = KeyCode(0x00CC);
    /// C: `INPUT_KEY_KP_SPACE = 0x00CD` (`input.h`).
    pub const KP_SPACE: KeyCode = KeyCode(0x00CD);
    /// C: `INPUT_KEY_KP_AT = 0x00CE` (`input.h`).
    pub const KP_AT: KeyCode = KeyCode(0x00CE);
    /// C: `INPUT_KEY_KP_EXCLAMATION_MARK = 0x00CF` (`input.h`).
    pub const KP_EXCLAMATION_MARK: KeyCode = KeyCode(0x00CF);
    /// C: `INPUT_KEY_KP_MEM_STORE = 0x00D0` (`input.h`).
    pub const KP_MEM_STORE: KeyCode = KeyCode(0x00D0);
    /// C: `INPUT_KEY_KP_MEM_RECALL = 0x00D1` (`input.h`).
    pub const KP_MEM_RECALL: KeyCode = KeyCode(0x00D1);
    /// C: `INPUT_KEY_KP_MEM_CLEAR = 0x00D2` (`input.h`).
    pub const KP_MEM_CLEAR: KeyCode = KeyCode(0x00D2);
    /// C: `INPUT_KEY_KP_MEM_ADD = 0x00D3` (`input.h`).
    pub const KP_MEM_ADD: KeyCode = KeyCode(0x00D3);
    /// C: `INPUT_KEY_KP_MEM_SUBTRACT = 0x00D4` (`input.h`).
    pub const KP_MEM_SUBTRACT: KeyCode = KeyCode(0x00D4);
    /// C: `INPUT_KEY_KP_MEM_MULTIPLY = 0x00D5` (`input.h`).
    pub const KP_MEM_MULTIPLY: KeyCode = KeyCode(0x00D5);
    /// C: `INPUT_KEY_KP_MEM_DIVIDE = 0x00D6` (`input.h`).
    pub const KP_MEM_DIVIDE: KeyCode = KeyCode(0x00D6);
    /// C: `INPUT_KEY_KP_PLUS_MINUS = 0x00D7` (`input.h`).
    pub const KP_PLUS_MINUS: KeyCode = KeyCode(0x00D7);
    /// C: `INPUT_KEY_KP_CLEAR = 0x00D8` (`input.h`).
    pub const KP_CLEAR: KeyCode = KeyCode(0x00D8);
    /// C: `INPUT_KEY_KP_CLEAR_ENTRY = 0x00D9` (`input.h`).
    pub const KP_CLEAR_ENTRY: KeyCode = KeyCode(0x00D9);
    /// C: `INPUT_KEY_KP_BIN = 0x00DA` (`input.h`).
    pub const KP_BIN: KeyCode = KeyCode(0x00DA);
    /// C: `INPUT_KEY_KP_OCT = 0x00DB` (`input.h`).
    pub const KP_OCT: KeyCode = KeyCode(0x00DB);
    /// C: `INPUT_KEY_KP_DEC = 0x00DC` (`input.h`).
    pub const KP_DEC: KeyCode = KeyCode(0x00DC);
    /// C: `INPUT_KEY_KP_HEX = 0x00DD` (`input.h`).
    pub const KP_HEX: KeyCode = KeyCode(0x00DD);
    /// C: `INPUT_KEY_LEFT_CTRL = 0x00E0` (`input.h`).
    pub const LEFT_CTRL: KeyCode = KeyCode(0x00E0);
    /// C: `INPUT_KEY_LEFT_SHIFT = 0x00E1` (`input.h`).
    pub const LEFT_SHIFT: KeyCode = KeyCode(0x00E1);
    /// C: `INPUT_KEY_LEFT_ALT = 0x00E2` (`input.h`).
    pub const LEFT_ALT: KeyCode = KeyCode(0x00E2);
    /// C: `INPUT_KEY_LEFT_GUI = 0x00E3` (`input.h`).
    pub const LEFT_GUI: KeyCode = KeyCode(0x00E3);
    /// C: `INPUT_KEY_RIGHT_CTRL = 0x00E4` (`input.h`).
    pub const RIGHT_CTRL: KeyCode = KeyCode(0x00E4);
    /// C: `INPUT_KEY_RIGHT_SHIFT = 0x00E5` (`input.h`).
    pub const RIGHT_SHIFT: KeyCode = KeyCode(0x00E5);
    /// C: `INPUT_KEY_RIGHT_ALT = 0x00E6` (`input.h`).
    pub const RIGHT_ALT: KeyCode = KeyCode(0x00E6);
    /// C: `INPUT_KEY_RIGHT_GUI = 0x00E7` (`input.h`).
    pub const RIGHT_GUI: KeyCode = KeyCode(0x00E7);

    /// Wraps a raw wire value (no validation; unknown values travel untouched).
    pub const fn from_u16(raw: u16) -> Self {
        Self(raw)
    }

    /// The raw wire value.
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Whether the C header names this value (reserved gaps return `false`).
    pub const fn is_defined(self) -> bool {
        matches!(
            self.0,
            0x0004
                | 0x0005
                | 0x0006
                | 0x0007
                | 0x0008
                | 0x0009
                | 0x000A
                | 0x000B
                | 0x000C
                | 0x000D
                | 0x000E
                | 0x000F
                | 0x0010
                | 0x0011
                | 0x0012
                | 0x0013
                | 0x0014
                | 0x0015
                | 0x0016
                | 0x0017
                | 0x0018
                | 0x0019
                | 0x001A
                | 0x001B
                | 0x001C
                | 0x001D
                | 0x001E
                | 0x001F
                | 0x0020
                | 0x0021
                | 0x0022
                | 0x0023
                | 0x0024
                | 0x0025
                | 0x0026
                | 0x0027
                | 0x0028
                | 0x0029
                | 0x002A
                | 0x002B
                | 0x002C
                | 0x002D
                | 0x002E
                | 0x002F
                | 0x0030
                | 0x0031
                | 0x0032
                | 0x0033
                | 0x0034
                | 0x0035
                | 0x0036
                | 0x0037
                | 0x0038
                | 0x0039
                | 0x003A
                | 0x003B
                | 0x003C
                | 0x003D
                | 0x003E
                | 0x003F
                | 0x0040
                | 0x0041
                | 0x0042
                | 0x0043
                | 0x0044
                | 0x0045
                | 0x0046
                | 0x0047
                | 0x0048
                | 0x0049
                | 0x004A
                | 0x004B
                | 0x004C
                | 0x004D
                | 0x004E
                | 0x004F
                | 0x0050
                | 0x0051
                | 0x0052
                | 0x0053
                | 0x0054
                | 0x0055
                | 0x0056
                | 0x0057
                | 0x0058
                | 0x0059
                | 0x005A
                | 0x005B
                | 0x005C
                | 0x005D
                | 0x005E
                | 0x005F
                | 0x0060
                | 0x0061
                | 0x0062
                | 0x0063
                | 0x0064
                | 0x0065
                | 0x0066
                | 0x0067
                | 0x0068
                | 0x0069
                | 0x006A
                | 0x006B
                | 0x006C
                | 0x006D
                | 0x006E
                | 0x006F
                | 0x0070
                | 0x0071
                | 0x0072
                | 0x0073
                | 0x0074
                | 0x0075
                | 0x0076
                | 0x0077
                | 0x0078
                | 0x0079
                | 0x007A
                | 0x007B
                | 0x007C
                | 0x007D
                | 0x007E
                | 0x007F
                | 0x0080
                | 0x0081
                | 0x0082
                | 0x0083
                | 0x0084
                | 0x0085
                | 0x0086
                | 0x0087
                | 0x0088
                | 0x0089
                | 0x008A
                | 0x008B
                | 0x008C
                | 0x008D
                | 0x008E
                | 0x008F
                | 0x0090
                | 0x0091
                | 0x0092
                | 0x0093
                | 0x0094
                | 0x0095
                | 0x0096
                | 0x0097
                | 0x0098
                | 0x0099
                | 0x009A
                | 0x009B
                | 0x009C
                | 0x009D
                | 0x009E
                | 0x009F
                | 0x00A0
                | 0x00A1
                | 0x00A2
                | 0x00A3
                | 0x00A4
                | 0x00B0
                | 0x00B1
                | 0x00B2
                | 0x00B3
                | 0x00B4
                | 0x00B5
                | 0x00B6
                | 0x00B7
                | 0x00B8
                | 0x00B9
                | 0x00BA
                | 0x00BB
                | 0x00BC
                | 0x00BD
                | 0x00BE
                | 0x00BF
                | 0x00C0
                | 0x00C1
                | 0x00C2
                | 0x00C3
                | 0x00C4
                | 0x00C5
                | 0x00C6
                | 0x00C7
                | 0x00C8
                | 0x00C9
                | 0x00CA
                | 0x00CB
                | 0x00CC
                | 0x00CD
                | 0x00CE
                | 0x00CF
                | 0x00D0
                | 0x00D1
                | 0x00D2
                | 0x00D3
                | 0x00D4
                | 0x00D5
                | 0x00D6
                | 0x00D7
                | 0x00D8
                | 0x00D9
                | 0x00DA
                | 0x00DB
                | 0x00DC
                | 0x00DD
                | 0x00E0
                | 0x00E1
                | 0x00E2
                | 0x00E3
                | 0x00E4
                | 0x00E5
                | 0x00E6
                | 0x00E7
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_code_values_match_c() {
        // Spot checks against `minix3/minix/include/minix/input.h` explicit values.
        assert_eq!(KeyCode::A.as_u16(), 0x0004);
        assert_eq!(KeyCode::Z.as_u16(), 0x001D);
        assert_eq!(KeyCode::NUM_1.as_u16(), 0x001E);
        assert_eq!(KeyCode::ENTER.as_u16(), 0x0028);
        assert_eq!(KeyCode::LEFT_CTRL.as_u16(), 0x00E0);
        assert_eq!(KeyCode::RIGHT_GUI.as_u16(), 0x00E7);
        assert_eq!(KeyCode::KP_00.as_u16(), 0x00B0);
        assert_eq!(KeyCode::KP_HEX.as_u16(), 0x00DD);
    }

    #[test]
    fn test_key_code_defined_set_matches_c_gaps() {
        // Every enumerator the header names is defined ...
        assert!(KeyCode(0x0004).is_defined());
        assert!(KeyCode(0x0005).is_defined());
        assert!(KeyCode(0x0006).is_defined());
        assert!(KeyCode(0x0007).is_defined());
        assert!(KeyCode(0x0008).is_defined());
        assert!(KeyCode(0x0009).is_defined());
        assert!(KeyCode(0x000A).is_defined());
        assert!(KeyCode(0x000B).is_defined());
        assert!(KeyCode(0x000C).is_defined());
        assert!(KeyCode(0x000D).is_defined());
        assert!(KeyCode(0x000E).is_defined());
        assert!(KeyCode(0x000F).is_defined());
        assert!(KeyCode(0x0010).is_defined());
        assert!(KeyCode(0x0011).is_defined());
        assert!(KeyCode(0x0012).is_defined());
        assert!(KeyCode(0x0013).is_defined());
        assert!(KeyCode(0x0014).is_defined());
        assert!(KeyCode(0x0015).is_defined());
        assert!(KeyCode(0x0016).is_defined());
        assert!(KeyCode(0x0017).is_defined());
        assert!(KeyCode(0x0018).is_defined());
        assert!(KeyCode(0x0019).is_defined());
        assert!(KeyCode(0x001A).is_defined());
        assert!(KeyCode(0x001B).is_defined());
        assert!(KeyCode(0x001C).is_defined());
        assert!(KeyCode(0x001D).is_defined());
        assert!(KeyCode(0x001E).is_defined());
        assert!(KeyCode(0x001F).is_defined());
        assert!(KeyCode(0x0020).is_defined());
        assert!(KeyCode(0x0021).is_defined());
        assert!(KeyCode(0x0022).is_defined());
        assert!(KeyCode(0x0023).is_defined());
        assert!(KeyCode(0x0024).is_defined());
        assert!(KeyCode(0x0025).is_defined());
        assert!(KeyCode(0x0026).is_defined());
        assert!(KeyCode(0x0027).is_defined());
        assert!(KeyCode(0x0028).is_defined());
        assert!(KeyCode(0x0029).is_defined());
        assert!(KeyCode(0x002A).is_defined());
        assert!(KeyCode(0x002B).is_defined());
        assert!(KeyCode(0x002C).is_defined());
        assert!(KeyCode(0x002D).is_defined());
        assert!(KeyCode(0x002E).is_defined());
        assert!(KeyCode(0x002F).is_defined());
        assert!(KeyCode(0x0030).is_defined());
        assert!(KeyCode(0x0031).is_defined());
        assert!(KeyCode(0x0032).is_defined());
        assert!(KeyCode(0x0033).is_defined());
        assert!(KeyCode(0x0034).is_defined());
        assert!(KeyCode(0x0035).is_defined());
        assert!(KeyCode(0x0036).is_defined());
        assert!(KeyCode(0x0037).is_defined());
        assert!(KeyCode(0x0038).is_defined());
        assert!(KeyCode(0x0039).is_defined());
        assert!(KeyCode(0x003A).is_defined());
        assert!(KeyCode(0x003B).is_defined());
        assert!(KeyCode(0x003C).is_defined());
        assert!(KeyCode(0x003D).is_defined());
        assert!(KeyCode(0x003E).is_defined());
        assert!(KeyCode(0x003F).is_defined());
        assert!(KeyCode(0x0040).is_defined());
        assert!(KeyCode(0x0041).is_defined());
        assert!(KeyCode(0x0042).is_defined());
        assert!(KeyCode(0x0043).is_defined());
        assert!(KeyCode(0x0044).is_defined());
        assert!(KeyCode(0x0045).is_defined());
        assert!(KeyCode(0x0046).is_defined());
        assert!(KeyCode(0x0047).is_defined());
        assert!(KeyCode(0x0048).is_defined());
        assert!(KeyCode(0x0049).is_defined());
        assert!(KeyCode(0x004A).is_defined());
        assert!(KeyCode(0x004B).is_defined());
        assert!(KeyCode(0x004C).is_defined());
        assert!(KeyCode(0x004D).is_defined());
        assert!(KeyCode(0x004E).is_defined());
        assert!(KeyCode(0x004F).is_defined());
        assert!(KeyCode(0x0050).is_defined());
        assert!(KeyCode(0x0051).is_defined());
        assert!(KeyCode(0x0052).is_defined());
        assert!(KeyCode(0x0053).is_defined());
        assert!(KeyCode(0x0054).is_defined());
        assert!(KeyCode(0x0055).is_defined());
        assert!(KeyCode(0x0056).is_defined());
        assert!(KeyCode(0x0057).is_defined());
        assert!(KeyCode(0x0058).is_defined());
        assert!(KeyCode(0x0059).is_defined());
        assert!(KeyCode(0x005A).is_defined());
        assert!(KeyCode(0x005B).is_defined());
        assert!(KeyCode(0x005C).is_defined());
        assert!(KeyCode(0x005D).is_defined());
        assert!(KeyCode(0x005E).is_defined());
        assert!(KeyCode(0x005F).is_defined());
        assert!(KeyCode(0x0060).is_defined());
        assert!(KeyCode(0x0061).is_defined());
        assert!(KeyCode(0x0062).is_defined());
        assert!(KeyCode(0x0063).is_defined());
        assert!(KeyCode(0x0064).is_defined());
        assert!(KeyCode(0x0065).is_defined());
        assert!(KeyCode(0x0066).is_defined());
        assert!(KeyCode(0x0067).is_defined());
        assert!(KeyCode(0x0068).is_defined());
        assert!(KeyCode(0x0069).is_defined());
        assert!(KeyCode(0x006A).is_defined());
        assert!(KeyCode(0x006B).is_defined());
        assert!(KeyCode(0x006C).is_defined());
        assert!(KeyCode(0x006D).is_defined());
        assert!(KeyCode(0x006E).is_defined());
        assert!(KeyCode(0x006F).is_defined());
        assert!(KeyCode(0x0070).is_defined());
        assert!(KeyCode(0x0071).is_defined());
        assert!(KeyCode(0x0072).is_defined());
        assert!(KeyCode(0x0073).is_defined());
        assert!(KeyCode(0x0074).is_defined());
        assert!(KeyCode(0x0075).is_defined());
        assert!(KeyCode(0x0076).is_defined());
        assert!(KeyCode(0x0077).is_defined());
        assert!(KeyCode(0x0078).is_defined());
        assert!(KeyCode(0x0079).is_defined());
        assert!(KeyCode(0x007A).is_defined());
        assert!(KeyCode(0x007B).is_defined());
        assert!(KeyCode(0x007C).is_defined());
        assert!(KeyCode(0x007D).is_defined());
        assert!(KeyCode(0x007E).is_defined());
        assert!(KeyCode(0x007F).is_defined());
        assert!(KeyCode(0x0080).is_defined());
        assert!(KeyCode(0x0081).is_defined());
        assert!(KeyCode(0x0082).is_defined());
        assert!(KeyCode(0x0083).is_defined());
        assert!(KeyCode(0x0084).is_defined());
        assert!(KeyCode(0x0085).is_defined());
        assert!(KeyCode(0x0086).is_defined());
        assert!(KeyCode(0x0087).is_defined());
        assert!(KeyCode(0x0088).is_defined());
        assert!(KeyCode(0x0089).is_defined());
        assert!(KeyCode(0x008A).is_defined());
        assert!(KeyCode(0x008B).is_defined());
        assert!(KeyCode(0x008C).is_defined());
        assert!(KeyCode(0x008D).is_defined());
        assert!(KeyCode(0x008E).is_defined());
        assert!(KeyCode(0x008F).is_defined());
        assert!(KeyCode(0x0090).is_defined());
        assert!(KeyCode(0x0091).is_defined());
        assert!(KeyCode(0x0092).is_defined());
        assert!(KeyCode(0x0093).is_defined());
        assert!(KeyCode(0x0094).is_defined());
        assert!(KeyCode(0x0095).is_defined());
        assert!(KeyCode(0x0096).is_defined());
        assert!(KeyCode(0x0097).is_defined());
        assert!(KeyCode(0x0098).is_defined());
        assert!(KeyCode(0x0099).is_defined());
        assert!(KeyCode(0x009A).is_defined());
        assert!(KeyCode(0x009B).is_defined());
        assert!(KeyCode(0x009C).is_defined());
        assert!(KeyCode(0x009D).is_defined());
        assert!(KeyCode(0x009E).is_defined());
        assert!(KeyCode(0x009F).is_defined());
        assert!(KeyCode(0x00A0).is_defined());
        assert!(KeyCode(0x00A1).is_defined());
        assert!(KeyCode(0x00A2).is_defined());
        assert!(KeyCode(0x00A3).is_defined());
        assert!(KeyCode(0x00A4).is_defined());
        assert!(KeyCode(0x00B0).is_defined());
        assert!(KeyCode(0x00B1).is_defined());
        assert!(KeyCode(0x00B2).is_defined());
        assert!(KeyCode(0x00B3).is_defined());
        assert!(KeyCode(0x00B4).is_defined());
        assert!(KeyCode(0x00B5).is_defined());
        assert!(KeyCode(0x00B6).is_defined());
        assert!(KeyCode(0x00B7).is_defined());
        assert!(KeyCode(0x00B8).is_defined());
        assert!(KeyCode(0x00B9).is_defined());
        assert!(KeyCode(0x00BA).is_defined());
        assert!(KeyCode(0x00BB).is_defined());
        assert!(KeyCode(0x00BC).is_defined());
        assert!(KeyCode(0x00BD).is_defined());
        assert!(KeyCode(0x00BE).is_defined());
        assert!(KeyCode(0x00BF).is_defined());
        assert!(KeyCode(0x00C0).is_defined());
        assert!(KeyCode(0x00C1).is_defined());
        assert!(KeyCode(0x00C2).is_defined());
        assert!(KeyCode(0x00C3).is_defined());
        assert!(KeyCode(0x00C4).is_defined());
        assert!(KeyCode(0x00C5).is_defined());
        assert!(KeyCode(0x00C6).is_defined());
        assert!(KeyCode(0x00C7).is_defined());
        assert!(KeyCode(0x00C8).is_defined());
        assert!(KeyCode(0x00C9).is_defined());
        assert!(KeyCode(0x00CA).is_defined());
        assert!(KeyCode(0x00CB).is_defined());
        assert!(KeyCode(0x00CC).is_defined());
        assert!(KeyCode(0x00CD).is_defined());
        assert!(KeyCode(0x00CE).is_defined());
        assert!(KeyCode(0x00CF).is_defined());
        assert!(KeyCode(0x00D0).is_defined());
        assert!(KeyCode(0x00D1).is_defined());
        assert!(KeyCode(0x00D2).is_defined());
        assert!(KeyCode(0x00D3).is_defined());
        assert!(KeyCode(0x00D4).is_defined());
        assert!(KeyCode(0x00D5).is_defined());
        assert!(KeyCode(0x00D6).is_defined());
        assert!(KeyCode(0x00D7).is_defined());
        assert!(KeyCode(0x00D8).is_defined());
        assert!(KeyCode(0x00D9).is_defined());
        assert!(KeyCode(0x00DA).is_defined());
        assert!(KeyCode(0x00DB).is_defined());
        assert!(KeyCode(0x00DC).is_defined());
        assert!(KeyCode(0x00DD).is_defined());
        assert!(KeyCode(0x00E0).is_defined());
        assert!(KeyCode(0x00E1).is_defined());
        assert!(KeyCode(0x00E2).is_defined());
        assert!(KeyCode(0x00E3).is_defined());
        assert!(KeyCode(0x00E4).is_defined());
        assert!(KeyCode(0x00E5).is_defined());
        assert!(KeyCode(0x00E6).is_defined());
        assert!(KeyCode(0x00E7).is_defined());
        // ... and the reserved gaps the header leaves out are not.
        assert!(!KeyCode(0x00A5).is_defined());
        assert!(!KeyCode(0x00AF).is_defined());
        assert!(!KeyCode(0x00DE).is_defined());
        assert!(!KeyCode(0x00DF).is_defined());
        assert!(!KeyCode(0x00E8).is_defined());
        assert!(!KeyCode(0xFFFF).is_defined());
    }
}
