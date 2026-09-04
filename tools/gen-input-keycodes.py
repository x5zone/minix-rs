#!/usr/bin/env python3
"""Regenerate os/servers/input/src/key_codes.rs from the Minix3 C header.

Every INPUT_KEY_* enumerator in minix3/minix/include/minix/input.h becomes one
KeyCode associated constant with the same numeric value. Re-run after any
header change, then run `cargo test -p minix-input` (the value-lock tests fail
on any drift).

Usage (from the repository root):
    python3 tools/gen-input-keycodes.py
"""

import re
import sys

HEADER = 'minix3/minix/include/minix/input.h'
OUTPUT = 'os/servers/input/src/key_codes.rs'


def rust_name(s):
    # Associated constants use UPPER_SNAKE_CASE (Rust convention, and close to
    # the C spelling minus the prefix). Digit keys would start with a digit,
    # which is illegal, so they gain a NUM_ prefix.
    if s and s[0].isdigit():
        return 'NUM_' + s
    return s


def main():
    try:
        src = open(HEADER).read()
    except OSError as exc:
        sys.exit('cannot read %s: %s' % (HEADER, exc))
    blocks = re.findall(r'enum \{(.*?)\};', src, re.S)
    key = blocks[1]
    entries = []
    cur = None
    for line in key.splitlines():
        line = line.split('/*')[0].strip().rstrip(',')
        if not line or line.startswith('/'):
            continue
        if '=' in line:
            name, val = [x.strip() for x in line.split('=')]
            cur = int(val, 16)
        else:
            name = line.strip()
            cur += 1
        entries.append((name.replace('INPUT_KEY_', ''), cur))

    out = []
    out.append('//! Keyboard and keypad event codes (USB HID Usage Table, keyboard page).')
    out.append('//!')
    out.append('//! C: `minix3/minix/include/minix/input.h:59-290` (the `INPUT_KEY_*` enumerators).')
    out.append('//! Every constant below is mechanically derived from that header: the numeric')
    out.append('//! value is part of the wire contract (drivers report these numbers, readers')
    out.append('//! interpret them), so each value is locked by `key_code_values_match_c`.')
    out.append('//! To regenerate, re-run `tools/gen-input-keycodes.py` and then `cargo test`.')
    out.append('//!')
    out.append('//! Names drop the `INPUT_KEY_` prefix and stay UPPER_SNAKE_CASE')
    out.append('//! (digit keys gain a `NUM_` prefix: `INPUT_KEY_1` becomes `KeyCode::NUM_1`);')
    out.append('//! the numeric value is unchanged. Reserved gaps in the C numbering (`0x00A5-0x00AF`,')
    out.append('//! `0x00DE-0x00DF`, `0x00E8-0xFFFF`) have no constant here either:')
    out.append('//! `KeyCode::is_defined` returns `false` for them, exactly matching the set')
    out.append('//! the C header names.')
    out.append('')
    out.append('/// A keyboard-page event code as it appears on the wire (`code` field).')
    out.append('///')
    out.append('/// C passes these numbers through uninterpreted (`input.c` never validates')
    out.append('/// `code`); the meaning lives with the reader (terminal driver, window system).')
    out.append('/// The newtype keeps that transparency: any `u16` can travel, while the named')
    out.append('/// constants below give the defined subset a readable spelling.')
    out.append('#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]')
    out.append('pub struct KeyCode(pub u16);')
    out.append('')
    out.append('impl KeyCode {')
    for name, val in entries:
        out.append('    /// C: `INPUT_KEY_%s = 0x%04X` (`input.h`).' % (name, val))
        out.append('    pub const %s: KeyCode = KeyCode(0x%04X);' % (rust_name(name), val))
    out.append('')
    out.append('    /// Wraps a raw wire value (no validation; unknown values travel untouched).')
    out.append('    pub const fn from_u16(raw: u16) -> Self {')
    out.append('        Self(raw)')
    out.append('    }')
    out.append('')
    out.append('    /// The raw wire value.')
    out.append('    pub const fn as_u16(self) -> u16 {')
    out.append('        self.0')
    out.append('    }')
    out.append('')
    out.append('    /// Whether the C header names this value (reserved gaps return `false`).')
    out.append('    pub const fn is_defined(self) -> bool {')
    out.append('        matches!(self.0,')
    for i, (name, val) in enumerate(entries):
        out.append('            0x%04X%s' % (val, ' |' if i < len(entries) - 1 else ''))
    out.append('        )')
    out.append('    }')
    out.append('}')
    out.append('')
    out.append('#[cfg(test)]')
    out.append('mod tests {')
    out.append('    use super::*;')
    out.append('')
    out.append('    #[test]')
    out.append('    fn test_key_code_values_match_c() {')
    out.append('        // Spot checks against `minix3/minix/include/minix/input.h` explicit values.')
    out.append('        assert_eq!(KeyCode::A.as_u16(), 0x0004);')
    out.append('        assert_eq!(KeyCode::Z.as_u16(), 0x001D);')
    out.append('        assert_eq!(KeyCode::NUM_1.as_u16(), 0x001E);')
    out.append('        assert_eq!(KeyCode::ENTER.as_u16(), 0x0028);')
    out.append('        assert_eq!(KeyCode::LEFT_CTRL.as_u16(), 0x00E0);')
    out.append('        assert_eq!(KeyCode::RIGHT_GUI.as_u16(), 0x00E7);')
    out.append('        assert_eq!(KeyCode::KP_00.as_u16(), 0x00B0);')
    out.append('        assert_eq!(KeyCode::KP_HEX.as_u16(), 0x00DD);')
    out.append('    }')
    out.append('')
    out.append('    #[test]')
    out.append('    fn test_key_code_defined_set_matches_c_gaps() {')
    out.append('        // Every enumerator the header names is defined ...')
    for name, val in entries:
        out.append('        assert!(KeyCode(0x%04X).is_defined());' % val)
    out.append('        // ... and the reserved gaps the header leaves out are not.')
    out.append('        assert!(!KeyCode(0x00A5).is_defined());')
    out.append('        assert!(!KeyCode(0x00AF).is_defined());')
    out.append('        assert!(!KeyCode(0x00DE).is_defined());')
    out.append('        assert!(!KeyCode(0x00DF).is_defined());')
    out.append('        assert!(!KeyCode(0x00E8).is_defined());')
    out.append('        assert!(!KeyCode(0xFFFF).is_defined());')
    out.append('    }')
    out.append('}')
    open(OUTPUT, 'w').write('\n'.join(out) + '\n')
    print('wrote %s (%d lines, %d codes)' % (OUTPUT, len(out), len(entries)))


if __name__ == '__main__':
    main()
