//! Rock Ridge names and links: long names, identities, and symbolic
//! targets over the system-use tail (`susp.c`, `susp_rock_ridge.c`).
//!
//! Plain ISO9660 names are uppercase with versions (`FILE.TXT;1`) and
//! directories have no owner: fine for interchange, poor for a Unix
//! personality. Rock Ridge appends typed entries to each record's tail:
//! `NM` carries the real name, `PX` the mode and ownership, `SL` the
//! link target as components, `CL` the relocated directory block, `RE`
//! the relocation flag. The `norock` mount option skips all of it, and
//! the server then shows the raw interchange names.
//!
//! Each entry opens with a two-byte signature, a length byte, and a
//! version byte; parsers must never read past the length byte, and
//! unknown signatures are skipped, not refused (forward compatibility
//! with newer extensions).

/// Longest Rock Ridge name (`ISO9660_RRIP_MAX_FILE_ID_LEN`, 256 bytes).
pub const MAX_NAME_BYTES: usize = 256;
/// Name entry (`NM`).
pub const TAG_NAME: &[u8; 2] = b"NM";
/// Mode entry (`PX`).
pub const TAG_MODE: &[u8; 2] = b"PX";
/// Link entry (`SL`).
pub const TAG_LINK: &[u8; 2] = b"SL";
/// Child-link entry (`CL`): the directory really lives elsewhere.
pub const TAG_CHILD: &[u8; 2] = b"CL";
/// Relocation entry (`RE`): this directory moved up for interchange.
pub const TAG_RELOCATED: &[u8; 2] = b"RE";
/// Link-component flags: plain component (`susp_rock_ridge.c:71-90`).
pub const LINK_PLAIN: u8 = 0x0;
/// Link-component flags: current directory (`susp_rock_ridge.c:91-101`).
pub const LINK_CURRENT: u8 = 0x2;
/// Link-component flags: parent directory (`susp_rock_ridge.c:102-112`).
pub const LINK_PARENT: u8 = 0x4;
/// Link-component flags: root directory (`susp_rock_ridge.c:113-120`).
pub const LINK_ROOT: u8 = 0x8;

/// One parsed system-use entry: signature plus payload slice bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemEntry {
    /// Two-byte signature.
    pub tag: [u8; 2],
    /// Payload offset inside the tail.
    pub payload: usize,
    /// Payload length in bytes.
    pub length: usize,
}

/// Split a system-use tail into entries. Short headers stop the walk
/// (padding, not corruption); a length below the four-byte header or
/// past the tail refuses the whole tail.
pub fn split_entries(tail: &[u8]) -> Result<alloc::vec::Vec<SystemEntry>, crate::RockError> {
    use crate::RockError;
    let mut entries = alloc::vec::Vec::new();
    let mut rest = tail;
    let mut base = 0usize;
    while rest.len() >= 4 {
        let length = rest[2] as usize;
        if length < 4 || length > rest.len() {
            return Err(RockError::Invalid);
        }
        entries.push(SystemEntry {
            tag: [rest[0], rest[1]],
            payload: base + 4,
            length: length - 4,
        });
        rest = &rest[length..];
        base += length;
    }
    Ok(entries)
}

/// Assemble a symbolic-link target from `SL` components
/// (`parse_susp_rock_ridge_sl`, `susp_rock_ridge.c:47-130`): plain
/// components join with slashes, current contributes a dot, parent two
/// dots, root a slash. Every append checks the bound first; overflow
/// stops the assembly with what fits so far, never a torn write. An
/// empty assembly stays empty (the placeholder hook reports it as-is).
pub fn assemble_link(components: &[(u8, &[u8])]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::new();
    for (flags, text) in components {
        match flags & 0xF {
            LINK_CURRENT => {
                if out.len() + 1 >= MAX_NAME_BYTES {
                    return out;
                }
                if !out.is_empty() && out != b"/" {
                    out.push(b'/');
                }
                out.push(b'.');
            }
            LINK_PARENT => {
                if out.len() + 2 >= MAX_NAME_BYTES {
                    return out;
                }
                if !out.is_empty() && out != b"/" {
                    out.push(b'/');
                }
                out.extend_from_slice(b"..");
            }
            LINK_ROOT => {
                if out.len() + 1 >= MAX_NAME_BYTES {
                    return out;
                }
                out.push(b'/');
            }
            _ => {
                if out.len() + text.len() + 1 >= MAX_NAME_BYTES {
                    return out;
                }
                if !out.is_empty() && out != b"/" {
                    out.push(b'/');
                }
                out.extend_from_slice(text);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_entries_walk() {
        // Two entries: NM with two payload bytes, PX with none, then
        // two padding bytes that end the walk.
        let tail = [b'N', b'M', 6, 1, b'h', b'i', b'P', b'X', 4, 1, 0, 0];
        let entries = split_entries(&tail).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].tag, *TAG_NAME);
        assert_eq!(entries[0].length, 2);
        assert_eq!(entries[1].tag, *TAG_MODE);
        // Short length refuses.
        assert_eq!(split_entries(&[b'N', b'M', 3, 1]).unwrap_err(), crate::RockError::Invalid);
    }

    #[test]
    fn test_assemble_link_components() {
        assert_eq!(assemble_link(&[]), alloc::vec::Vec::new());
        assert_eq!(
            assemble_link(&[(LINK_PLAIN, b"usr"), (LINK_PLAIN, b"bin")]),
            b"usr/bin".to_vec()
        );
        assert_eq!(assemble_link(&[(LINK_PARENT, b"")]), b"..".to_vec());
        assert_eq!(assemble_link(&[(LINK_CURRENT, b"")]), b".".to_vec());
        assert_eq!(assemble_link(&[(LINK_ROOT, b"")]), b"/".to_vec());
        // Mixed walk with separators exactly once between parts.
        assert_eq!(
            assemble_link(&[(LINK_PLAIN, b"a"), (LINK_PARENT, b""), (LINK_PLAIN, b"b")]),
            b"a/../b".to_vec()
        );
    }
}
