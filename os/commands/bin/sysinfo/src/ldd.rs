//! Static archive listing (the no-dynamic-linker `ldd`).
//!
//! Ground truth: `minix3/usr.bin/ldd/ldd.c`. The tool handles executable and
//! linkable format files opaquely (near line 93: thirty two and sixty four bit
//! variants both accepted). Architectural evolution: the system links
//! statically and ships no dynamic linker, so this tool lists static archive
//! members instead of shared object dependencies. The execution layer owns
//! file reading; this module owns the member table and the display decision.

use crate::SysinfoError;

/// One archive member (name plus size in bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveMember<'a> {
    /// Member name (`libc.a(printf.o)` style leaf, or plain object name).
    pub name: &'a str,
    /// Member size in bytes.
    pub size: u64,
}

/// Member table behind the listing.
pub trait ArchiveTable<'a> {
    /// Member at `index`, or `None` when past the end.
    fn member(&self, index: usize) -> Option<ArchiveMember<'a>>;
    /// Number of members.
    fn len(&self) -> usize;
    /// True when the archive holds no members.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Table backed by a borrowed slice.
pub struct SliceArchive<'a> {
    members: &'a [ArchiveMember<'a>],
}

impl<'a> SliceArchive<'a> {
    /// Build a table over borrowed members.
    pub fn new(members: &'a [ArchiveMember<'a>]) -> Self {
        SliceArchive { members }
    }
}

impl<'a> ArchiveTable<'a> for SliceArchive<'a> {
    fn member(&self, index: usize) -> Option<ArchiveMember<'a>> {
        self.members.get(index).copied()
    }

    fn len(&self) -> usize {
        self.members.len()
    }
}

/// Empty archive (nothing to list).
pub struct EmptyArchive;

impl<'a> ArchiveTable<'a> for EmptyArchive {
    fn member(&self, _index: usize) -> Option<ArchiveMember<'a>> {
        None
    }

    fn len(&self) -> usize {
        0
    }
}

/// Render one listing line (`name (size bytes)`) into `out`.
pub fn render_member_line(
    member: ArchiveMember<'_>,
    out: &mut [u8],
) -> Result<usize, SysinfoError> {
    let name = member.name.as_bytes();
    let mut size_digits = [0u8; 20];
    let mut digits = 0;
    let mut value = member.size;
    if value == 0 {
        size_digits[0] = b'0';
        digits = 1;
    } else {
        while value > 0 {
            size_digits[digits] = b'0' + (value % 10) as u8;
            value /= 10;
            digits += 1;
        }
    }
    let suffix = b" (size ";
    let tail = b" bytes)";
    let needed = name.len() + suffix.len() + digits + tail.len();
    if out.len() < needed || member.name.is_empty() {
        return Err(SysinfoError::InvalidArgument);
    }
    let mut written = 0;
    out[written..written + name.len()].copy_from_slice(name);
    written += name.len();
    out[written..written + suffix.len()].copy_from_slice(suffix);
    written += suffix.len();
    let mut index = digits;
    while index > 0 {
        index -= 1;
        out[written] = size_digits[index];
        written += 1;
    }
    out[written..written + tail.len()].copy_from_slice(tail);
    written += tail.len();
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_lists() {
        static MEMBERS: &[ArchiveMember<'static>] = &[
            ArchiveMember { name: "printf.o", size: 1200 },
            ArchiveMember { name: "malloc.o", size: 800 },
        ];
        let table = SliceArchive::new(MEMBERS);
        assert_eq!(table.len(), 2);
        assert_eq!(table.member(0).unwrap().name, "printf.o");
        assert!(table.member(2).is_none());
    }

    #[test]
    fn test_empty_lists_nothing() {
        let table = EmptyArchive;
        assert_eq!(table.len(), 0);
        assert!(table.member(0).is_none());
    }

    #[test]
    fn test_line_renders() {
        let member = ArchiveMember { name: "printf.o", size: 1200 };
        let mut out = [0u8; 64];
        let len = render_member_line(member, &mut out).unwrap();
        assert_eq!(&out[..len], b"printf.o (size 1200 bytes)");
    }

    #[test]
    fn test_empty_name_rejected() {
        let member = ArchiveMember { name: "", size: 10 };
        let mut out = [0u8; 64];
        assert_eq!(
            render_member_line(member, &mut out),
            Err(SysinfoError::InvalidArgument)
        );
    }

    #[test]
    fn test_small_buffer_rejected() {
        let member = ArchiveMember { name: "printf.o", size: 10 };
        let mut out = [0u8; 4];
        assert_eq!(
            render_member_line(member, &mut out),
            Err(SysinfoError::InvalidArgument)
        );
    }
}
