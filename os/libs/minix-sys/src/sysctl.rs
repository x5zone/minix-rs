//! System control: the MIB-service transport face behind `sysctl(3)`, plus
//! the by-name resolution walk behind `sysctlbyname`.
//!
//! C correspondence splits in two:
//!
//! * `minix3/minix/lib/libc/sys/__sysctl.c` — the transport. The caller's
//!   name, old-value sink, and new-value source travel in one message
//!   (`m_lc_mib_sysctl`, `minix3/minix/include/minix/ipc.h:424-433`) with a
//!   blocking SENDREC to the MIB service (`MIB_PROC_NR`, `com.h:66`; the
//!   call `MIB_SYSCTL`, `com.h:1026`). The reply's single lane is the
//!   result length, and C copies it out *even when the call failed* — the
//!   undocumented NetBSD behavior `sysctl(8)` sizes its buffer from
//!   (`__sysctl.c:29-38`). This face keeps that: a server-replied failure
//!   still updates the caller's length, only a transport-level failure
//!   (no reply at all) leaves it untouched — C's comment itself calls the
//!   value in that case garbage.
//!
//! * `minix3/minix/lib/libc/gen/sysctlgetmibinfo.c` — the by-name walk.
//!   C keeps a user-side cache of the whole tree and refills it through
//!   `CTL_QUERY` enumerations; this face runs the same per-level query
//!   mechanism without the cache (one round trip per name component — the
//!   cache pays off across many lookups, while `init`'s chroot probe asks
//!   once). A token that parses as a number rides directly into the mib
//!   array (`strtoimax` with base 0, `sysctlgetmibinfo.c` — so `1.9` and
//!   `kern.securelevel` resolve alike), a name token enumerates the parent
//!   level and matches `sysctl_name`, and a name under a leaf is `ENOTDIR`
//!   exactly as C's node-type gate answers.
//!
//! Known platform gap (inherited, not introduced here): the request
//! overlay's three address lanes (`oldp`/`newp`/`namep`) are 32-bit. C
//! pins every payload at 56 bytes (`_ASSERT_MSG_SIZE`,
//! `minix/include/minix/ipcconst.h:17-19`), a bound the LP64 expansion of
//! `mess_lc_mib_sysctl` (80 bytes) cannot meet — C itself only ever built
//! the 32-bit form, where addresses are 32-bit too. minix-rs user images
//! link at 5 GiB (the bootstrap root keeps the first 4 GiB identity-mapped,
//! see the user link script), so a real caller's buffer address exceeds
//! the lane on the live wire; the hosted round trips this crate tests are
//! unaffected. Widening needs the inline-name window traded for lane width
//! and moves all three ends (overlay, service decode, this client).

use crate::Errno;
use crate::ipc::IpcTransport;
use crate::misc::{MIB_CALL_SYSCTL, MIB_ENDPOINT_NUMBER};
use minix_types::{
    CTL_MAXNAME, CTL_QUERY, CTLTYPE_NODE, Endpoint, MessLcMibSysctl, MessMibLcSysctl, Message,
    SYSCTL_NAMELEN, SYSCTL_TYPEMASK, SysctlNode,
};

/// Numeric mib path for the kernel security level.
///
/// C init composes `{CTL_KERN, KERN_SECURELVL}` at the call sites
/// (`minix3/sbin/init/init.c:554-556`, `:607-608`); the pair travels under
/// these names so callers never hard-code the numbers.
pub use minix_types::{CTL_KERN, KERN_SECURELVL};

/// MIB service endpoint, the C `MIB_PROC_NR`
/// (`minix3/minix/include/minix/com.h:66`).
pub const fn mib_endpoint() -> Endpoint {
    Endpoint(MIB_ENDPOINT_NUMBER)
}

/// One sysctl exchange frame — 96 bytes, the C header's own design goal
/// (the exchange face is 96 in both data models; the `SysctlNode` layout
/// test in minix-types pins it).
const NODE_BYTES: usize = size_of::<SysctlNode>();

/// Children a single enumeration window holds.
///
/// Sized past the largest static subtree in this tree (kern, 46 entries);
/// a busier level makes the service answer `ENOMEM`, which propagates.
/// C grows its learn buffer from the parent node's own child count
/// (`sysctlgetmibinfo.c`) — the window trades that growth for a fixed
/// frame.
const QUERY_WINDOW_NODES: usize = 64;

/// The old-value sink of a sysctl call — C's `oldp`/`oldlenp` pair.
pub struct SysctlOld<'a> {
    /// Where the node's value lands (C `oldp`).
    pub buffer: &'a mut [u8],
    /// C `*oldlenp`: the capacity goes in, the server's length comes out —
    /// also on a failed call (`__sysctl.c:29-38`, the buffer-sizing quirk).
    pub length: &'a mut usize,
}

/// One raw sysctl round trip — the seam the name walk climbs on.
///
/// Production wires it to [`sysctl_via`] over the transport; tests
/// substitute an in-memory tree. The method set is exactly what the C walk
/// issues (`__learn_tree` sends the same `CTL_QUERY` enumeration,
/// `sysctlgetmibinfo.c`).
pub(crate) trait RawSysctl {
    /// C: `sysctl(name, namelen, oldp, oldlenp, newp, newlen)`.
    fn call(
        &self,
        name: &[i32],
        old: Option<(&mut [u8], &mut usize)>,
        new: Option<&[u8]>,
    ) -> Result<(), Errno>;
}

/// Production seam: every walk step goes out over the MIB wire.
struct WireSysctl<'a, T: IpcTransport> {
    transport: &'a T,
}

impl<T: IpcTransport> RawSysctl for WireSysctl<'_, T> {
    fn call(
        &self,
        name: &[i32],
        old: Option<(&mut [u8], &mut usize)>,
        new: Option<&[u8]>,
    ) -> Result<(), Errno> {
        let slot = old.map(|(buffer, length)| SysctlOld { buffer, length });
        sysctl_via(self.transport, name, slot, new)
    }
}

/// Runs one sysctl exchange over an explicit transport.
///
/// The name travels inline while it fits the message window and by pointer
/// past that (`__sysctl.c:23-24`, the `CTL_SHORTNAME` split —
/// [`crate::misc::sysctl_name_fits_inline`]). A failed round trip is an
/// `Err` carrying the service's or the transport's errno; the sink's
/// length is refreshed whenever the service answered.
pub fn sysctl_via<T: IpcTransport>(
    transport: &T,
    name: &[i32],
    mut old: Option<SysctlOld<'_>>,
    new: Option<&[u8]>,
) -> Result<(), Errno> {
    let mut message = Message::zeroed();
    // The address lanes are 32-bit (see the module header's platform
    // note); the pointers leave the frame only for the synchronous round
    // trip — the C `oldp`/`newp` argument shape.
    let old_pointer = old
        .as_mut()
        .map_or(0, |slot| slot.buffer.as_mut_ptr() as u64 as u32);
    let old_capacity = old.as_ref().map_or(0, |slot| slot.buffer.len());
    let new_pointer = new.map_or(0, |bytes| bytes.as_ptr() as u64 as u32);
    {
        // SAFETY: the overlay is plain `repr(C)` data at the message
        // payload head; every field is written before the send.
        let wire: &mut MessLcMibSysctl = unsafe { &mut message.m_u.m_lc_mib_sysctl };
        wire.oldp = old_pointer;
        wire.oldlen = old_capacity as u32;
        wire.newp = new_pointer;
        wire.newlen = new.map_or(0, <[u8]>::len) as u32;
        wire.namelen = name.len() as u32;
        if crate::misc::sysctl_name_fits_inline(name.len()) {
            wire.name[..name.len()].copy_from_slice(name);
        } else {
            wire.namep = name.as_ptr() as u64 as u32;
        }
    }
    message.m_type = MIB_CALL_SYSCTL;
    // The round trip is spelled out rather than folded through
    // [`crate::syscall::perform_syscall`] because the two failure kinds
    // owe different answers here: a transport failure carries the
    // positive errno and means no reply exists (the sink stays
    // untouched), while a server-rejected call keeps its reply and its
    // length lane (`__sysctl.c:29-38`).
    if let Err(status) = transport.sendrec(mib_endpoint(), &mut message) {
        return Err(Errno::from_i32(status.0));
    }
    // SAFETY: the reply overlay reuses the same payload union lane; the
    // service wrote its single length field (`ipc.h:1548-1552`).
    let reply: &MessMibLcSysctl = unsafe { &message.m_u.m_mib_lc_sysctl };
    let replied_length = reply.oldlen as usize;
    if let Some(slot) = old.as_mut() {
        *slot.length = replied_length;
    }
    if message.m_type < 0 {
        Err(Errno::from_i32(-message.m_type))
    } else {
        Ok(())
    }
}

/// Resolves a dotted sysctl name to its mib components over an explicit
/// transport.
///
/// Separators: C picks the first `.` or `/` in the name and honors a
/// leading one (`sysctlgetmibinfo.c`); a trailing one ends the walk
/// unprocessed. `mib` must hold [`CTL_MAXNAME`] components — C's array is
/// caller-allocated to the same bound (`sys/sys/sysctl.h:75`), and a walk
/// needing more room is refused as `EINVAL` where C would overflow.
pub fn sysctlnametomib_via<T: IpcTransport>(
    transport: &T,
    name: &[u8],
    mib: &mut [i32],
) -> Result<usize, Errno> {
    nametomib_walk(&WireSysctl { transport }, name, mib)
}

/// Resolves the name, then runs the exchange — C `sysctlbyname`
/// (`minix3/minix/lib/libc/gen/sysctlbyname.c:48-54`), whose whole body is
/// this composition.
pub fn sysctlbyname_via<T: IpcTransport>(
    transport: &T,
    name: &[u8],
    old: Option<SysctlOld<'_>>,
    new: Option<&[u8]>,
) -> Result<(), Errno> {
    sysctlbyname_with(&WireSysctl { transport }, name, old, new)
}

/// The by-name composition over any seam (the tests climb this face).
pub(crate) fn sysctlbyname_with(
    call: &impl RawSysctl,
    name: &[u8],
    old: Option<SysctlOld<'_>>,
    new: Option<&[u8]>,
) -> Result<(), Errno> {
    let mut mib = [0i32; CTL_MAXNAME as usize];
    let length = nametomib_walk(call, name, &mut mib)?;
    call.call(
        &mib[..length],
        old.map(|slot| (slot.buffer, slot.length)),
        new,
    )
}

/// The per-component name walk (C `sysctlgetmibinfo_unlocked` without the
/// tree cache — one `CTL_QUERY` enumeration per name token).
pub(crate) fn nametomib_walk(
    call: &impl RawSysctl,
    gname: &[u8],
    mib: &mut [i32],
) -> Result<usize, Errno> {
    let separator = gname
        .iter()
        .copied()
        .find(|b| *b == b'.' || *b == b'/')
        .unwrap_or(b'.');
    // C: a leading separator is allowed and skipped once.
    let body = if gname.first() == Some(&separator) {
        &gname[1..]
    } else {
        gname
    };
    let room = mib.len().min(CTL_MAXNAME as usize);
    let mut filled = 0usize;
    // The walk starts at the root node; each matched name refreshes the
    // shape fact the next token is judged against.
    let mut parent_is_node = true;
    let mut cursor = 0usize;
    while cursor < body.len() {
        let end = body[cursor..]
            .iter()
            .position(|b| *b == separator)
            .map_or(body.len(), |offset| cursor + offset);
        let token = &body[cursor..end];
        cursor = if end < body.len() { end + 1 } else { end };

        if token.is_empty() {
            // Interior double separators have no name; C's `strtoimax`
            // reads the empty token as numeric zero and typically fails
            // one level later — refusing here answers the same question
            // without manufacturing a bogus component.
            return Err(Errno::ENOENT);
        }
        if filled >= room {
            // C's array is [CTL_MAXNAME] by caller contract; overflow is
            // refused instead of written past the end.
            return Err(Errno::EINVAL);
        }

        if let Some(number) = parse_numeric_token(token) {
            // C `just_numbers`: numeric components ride into the array
            // without a query, even past a leaf.
            mib[filled] = number;
            filled += 1;
            continue;
        }

        // C: a name token under a non-node is ENOTDIR.
        if !parent_is_node {
            return Err(Errno::ENOTDIR);
        }
        // C: tokens carry `SYSCTL_NAMELEN` bytes minus the terminator
        // (`token[SYSCTL_NAMELEN]`, the `l > sizeof(token)` gate).
        if token.len() >= SYSCTL_NAMELEN {
            return Err(Errno::ENAMETOOLONG);
        }

        match query_child(call, &mib[..filled], token)? {
            Some(child) => {
                parent_is_node = child.flags & SYSCTL_TYPEMASK == CTLTYPE_NODE;
                mib[filled] = child.number;
                filled += 1;
            }
            None => return Err(Errno::ENOENT),
        }
    }
    Ok(filled)
}

/// One matched child: its mib number plus the shape facts the walk needs.
struct MatchedChild {
    number: i32,
    flags: u32,
}

/// Enumerates one level and matches `token` against `sysctl_name`.
///
/// The query is C's `name[namelen] = CTL_QUERY` shape
/// (`sysctlgetmibinfo.c`): the path so far, the enumeration op appended,
/// the 96-byte nodes copied into the window by the service. Enumeration
/// failures propagate; `Ok(None)` is "no child carries this name".
fn query_child(
    call: &impl RawSysctl,
    path: &[i32],
    token: &[u8],
) -> Result<Option<MatchedChild>, Errno> {
    let mut query = [0i32; CTL_MAXNAME as usize];
    query[..path.len()].copy_from_slice(path);
    query[path.len()] = CTL_QUERY;
    let mut window = [0u8; NODE_BYTES * QUERY_WINDOW_NODES];
    let mut length = window.len();
    call.call(
        &query[..path.len() + 1],
        Some((&mut window, &mut length)),
        None,
    )?;
    let count = length / NODE_BYTES;
    for index in 0..count {
        let node = &window[index * NODE_BYTES..][..NODE_BYTES];
        if node_name(node) != token {
            continue;
        }
        // Field offsets pinned by the `SysctlNode` layout test in
        // minix-types: flags@0, number@4, name@8.
        let mut number_bytes = [0u8; 4];
        number_bytes.copy_from_slice(&node[4..8]);
        let mut flag_bytes = [0u8; 4];
        flag_bytes.copy_from_slice(&node[0..4]);
        return Ok(Some(MatchedChild {
            number: i32::from_le_bytes(number_bytes),
            flags: u32::from_le_bytes(flag_bytes),
        }));
    }
    Ok(None)
}

/// The NUL-padded name field of one exchange node.
fn node_name(node: &[u8]) -> &[u8] {
    let field = &node[8..8 + SYSCTL_NAMELEN];
    let length = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    &field[..length]
}

/// Reads a name token as C's `strtoimax(token, &t, 0)` would: optional
/// sign, `0x` hex, leading-`0` octal, else decimal — the whole token must
/// convert, and the value must land in `[INT_MIN, UINT_MAX]` before C's
/// int cast (`sysctlgetmibinfo.c` numeric block). `None` = "a name".
fn parse_numeric_token(token: &[u8]) -> Option<i32> {
    let (digits, negative) = match token.first() {
        Some(b'-') => (&token[1..], true),
        Some(b'+') => (&token[1..], false),
        Some(_) => (token, false),
        None => return None,
    };
    let (radix, digits) = if digits.len() >= 2 && digits[0] == b'0' && (digits[1] | 32) == b'x' {
        (16u32, &digits[2..])
    } else if digits.len() >= 2 && digits[0] == b'0' {
        (8u32, &digits[1..])
    } else {
        (10u32, digits)
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: u64 = 0;
    for digit in digits {
        let weight = (*digit as char).to_digit(radix)?;
        value = value
            .checked_mul(u64::from(radix))?
            .checked_add(u64::from(weight))?;
    }
    // C accepts [INT_MIN, UINT_MAX] (asymmetric — the int cast wraps above
    // INT_MAX), and rejects anything wider.
    if value > u64::from(u32::MAX) {
        return None;
    }
    let signed = if negative {
        -(value as i64)
    } else {
        value as i64
    };
    if signed < i64::from(i32::MIN) {
        return None;
    }
    Some(signed as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{CannedTransport, TrapStatus};
    use crate::syscall::cleared_message;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;
    use minix_types::{CTL_HW, ENOMEM, SYSCTL_VERSION};

    /// Packs one exchange node the way the service's serializer does:
    /// flags@0, number@4, NUL-padded name@8 (`serialize_node`, mib
    /// walker; the same shape the minix-types layout test pins).
    fn node_bytes(flags: u32, number: i32, name: &[u8]) -> [u8; NODE_BYTES] {
        let mut node = [0u8; NODE_BYTES];
        node[0..4].copy_from_slice(&flags.to_le_bytes());
        node[4..8].copy_from_slice(&number.to_le_bytes());
        node[8..8 + name.len()].copy_from_slice(name);
        node
    }

    fn node_flags() -> u32 {
        SYSCTL_VERSION | CTLTYPE_NODE
    }

    /// In-memory stand-in for the MIB service: `CTL_QUERY` at the tail
    /// lists the level's children, anything else reads the recorded value.
    struct FakeMib {
        levels: BTreeMap<Vec<i32>, Vec<[u8; NODE_BYTES]>>,
        values: BTreeMap<Vec<i32>, Vec<u8>>,
    }

    impl FakeMib {
        fn with_level(mut self, path: &[i32], children: &[(i32, &[u8])]) -> Self {
            let nodes = children
                .iter()
                .map(|(number, name)| node_bytes(node_flags(), *number, name))
                .collect();
            self.levels.insert(Vec::from(path), nodes);
            self
        }

        /// A level of leaf children (C `CTLTYPE_INT` values live at the
        /// bottom of each subtree) — the walk's ENOTDIR gate reads the
        /// type off these nodes.
        fn with_leaf_level(mut self, path: &[i32], children: &[(i32, &[u8])]) -> Self {
            let leaf_flags = SYSCTL_VERSION | minix_types::CTLTYPE_INT;
            let nodes = children
                .iter()
                .map(|(number, name)| node_bytes(leaf_flags, *number, name))
                .collect();
            self.levels.insert(Vec::from(path), nodes);
            self
        }

        fn with_value(mut self, path: &[i32], value: &[u8]) -> Self {
            self.values.insert(Vec::from(path), Vec::from(value));
            self
        }
    }

    impl RawSysctl for FakeMib {
        fn call(
            &self,
            name: &[i32],
            old: Option<(&mut [u8], &mut usize)>,
            _new: Option<&[u8]>,
        ) -> Result<(), Errno> {
            if name.last() == Some(&CTL_QUERY) {
                let children = self
                    .levels
                    .get(&name[..name.len() - 1])
                    .ok_or(Errno::ENOENT)?;
                let (buffer, length) = old.ok_or(Errno::EINVAL)?;
                let mut used = 0usize;
                for node in children {
                    if used + NODE_BYTES > buffer.len() {
                        return Err(Errno::ENOMEM);
                    }
                    buffer[used..used + NODE_BYTES].copy_from_slice(node);
                    used += NODE_BYTES;
                }
                *length = used;
                return Ok(());
            }
            let value = self.values.get(name).ok_or(Errno::ENOENT)?;
            if let Some((buffer, length)) = old {
                let copied = value.len().min(buffer.len());
                buffer[..copied].copy_from_slice(&value[..copied]);
                *length = value.len();
            }
            Ok(())
        }
    }

    /// `kern.securelevel` resolves through two enumerations; a numeric
    /// token skips its query entirely.
    #[test]
    fn test_nametomib_walks_names_and_numbers() {
        let tree = FakeMib {
            levels: BTreeMap::new(),
            values: BTreeMap::new(),
        }
        .with_level(&[], &[(CTL_KERN, b"kern"), (CTL_HW, b"hw")])
        .with_leaf_level(&[CTL_KERN], &[(KERN_SECURELVL, b"securelevel")]);
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"kern.securelevel", &mut mib).unwrap(),
            2
        );
        assert_eq!(&mib[..2], &[CTL_KERN, KERN_SECURELVL]);

        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(nametomib_walk(&tree, b"1.9", &mut mib).unwrap(), 2);
        assert_eq!(&mib[..2], &[1, 9]);

        // Mixed: the numeric head rides in, the name tail still queries.
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"1.securelevel", &mut mib).unwrap(),
            2
        );
        assert_eq!(&mib[..2], &[1, KERN_SECURELVL]);
    }

    /// Separator variants: C picks the first `.` or `/`, honors a leading
    /// one, and ends the walk before a trailing one.
    #[test]
    fn test_nametomib_separator_forms() {
        let tree = FakeMib {
            levels: BTreeMap::new(),
            values: BTreeMap::new(),
        }
        .with_level(&[], &[(CTL_KERN, b"kern")])
        .with_leaf_level(&[CTL_KERN], &[(KERN_SECURELVL, b"securelevel")]);
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"kern/securelevel", &mut mib).unwrap(),
            2
        );
        assert_eq!(&mib[..2], &[CTL_KERN, KERN_SECURELVL]);
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b".kern.securelevel", &mut mib).unwrap(),
            2
        );
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(nametomib_walk(&tree, b"kern.", &mut mib).unwrap(), 1);
        assert_eq!(mib[0], CTL_KERN);
    }

    /// The errno faces: missing name is ENOENT, a name under a leaf is
    /// ENOTDIR (while C's `just_numbers` still rides past a leaf), an
    /// oversized token is ENAMETOOLONG, a full array is EINVAL.
    #[test]
    fn test_nametomib_error_faces() {
        let tree = FakeMib {
            levels: BTreeMap::new(),
            values: BTreeMap::new(),
        }
        .with_level(&[], &[(CTL_KERN, b"kern")])
        .with_leaf_level(&[CTL_KERN], &[(KERN_SECURELVL, b"securelevel")]);
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"kern.nosuch", &mut mib).unwrap_err(),
            Errno::ENOENT
        );
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"kern.securelevel.deeper", &mut mib).unwrap_err(),
            Errno::ENOTDIR
        );
        // C `just_numbers`: numerics continue past a leaf.
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(nametomib_walk(&tree, b"1.9.2", &mut mib).unwrap(), 3);
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, &[b'a'; SYSCTL_NAMELEN], &mut mib).unwrap_err(),
            Errno::ENAMETOOLONG
        );
        // Twelve numerics fill the array; the thirteenth is refused.
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"1.2.3.4.5.6.7.8.9.10.11.12", &mut mib).unwrap(),
            12
        );
        assert_eq!(
            nametomib_walk(&tree, b"1.2.3.4.5.6.7.8.9.10.11.12.13", &mut mib).unwrap_err(),
            Errno::EINVAL
        );
        // An interior empty separator has no name (C's strtoimax would
        // read zero; refusing answers the same ENOENT-shaped question).
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"kern..x", &mut mib).unwrap_err(),
            Errno::ENOENT
        );
        // The empty name resolves to zero components (C's loop never runs).
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(nametomib_walk(&tree, b"", &mut mib).unwrap(), 0);
    }

    /// The composition: by-name read lands the value and the server
    /// length (`sysctlbyname.c:48-54`).
    #[test]
    fn test_sysctlbyname_composes_walk_and_read() {
        let tree = FakeMib {
            levels: BTreeMap::new(),
            values: BTreeMap::new(),
        }
        .with_level(&[], &[(CTL_KERN, b"kern")])
        .with_leaf_level(&[CTL_KERN], &[(KERN_SECURELVL, b"securelevel")])
        .with_value(&[CTL_KERN, KERN_SECURELVL], &3i32.to_ne_bytes());
        let mut buffer = [0u8; 4];
        let mut length = buffer.len();
        sysctlbyname_with(
            &tree,
            b"kern.securelevel",
            Some(SysctlOld {
                buffer: &mut buffer,
                length: &mut length,
            }),
            None,
        )
        .unwrap();
        assert_eq!(length, 4);
        assert_eq!(i32::from_ne_bytes(buffer), 3);
    }

    /// The wire face: call number and destination are the MIB pair, the
    /// inline name rides the overlay lanes, and the reply's length lands
    /// in the sink (`__sysctl.c:14-27`).
    #[test]
    fn test_sysctl_wire_carries_inline_name_and_reply_length() {
        let mut transport = CannedTransport::new();
        let mut reply = cleared_message();
        // SAFETY(test): 回程 overlay 单 lane（oldlen@0），服务端长度。
        unsafe { &mut reply.m_u.m_mib_lc_sysctl }.oldlen = 4;
        transport.reply_sendrec(Ok(reply));
        let mut buffer = [0u8; 4];
        let mut length = buffer.len();
        sysctl_via(
            &transport,
            &[CTL_KERN, KERN_SECURELVL],
            Some(SysctlOld {
                buffer: &mut buffer,
                length: &mut length,
            }),
            None,
        )
        .unwrap();
        assert_eq!(length, 4);
        let (destination, sent) = transport.sent.borrow()[0];
        assert_eq!(destination, mib_endpoint());
        assert_eq!(destination, Endpoint(MIB_ENDPOINT_NUMBER));
        assert_eq!(sent.m_type, MIB_CALL_SYSCTL);
        assert_eq!(sent.m_type, 0x600);
        // SAFETY(test): 读回请求 overlay 的字段序（ipc.h:424-433）。
        let wire = unsafe { &sent.m_u.m_lc_mib_sysctl };
        assert_ne!(wire.oldp, 0);
        assert_eq!(wire.oldlen, 4);
        assert_eq!(wire.newp, 0);
        assert_eq!(wire.newlen, 0);
        assert_eq!(wire.namelen, 2);
        assert_eq!(wire.namep, 0);
        assert_eq!(&wire.name[..2], &[CTL_KERN, KERN_SECURELVL]);
    }

    /// Write-only call: the new payload's address and length ride, the
    /// sink lanes stay zero, and no length is reported back.
    #[test]
    fn test_sysctl_wire_write_only_carries_new_payload() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(cleared_message()));
        let payload = [7u8, 0, 0, 0];
        sysctl_via(
            &transport,
            &[CTL_KERN, KERN_SECURELVL],
            None,
            Some(&payload),
        )
        .unwrap();
        // SAFETY(test): 同上，读 new 侧 lanes。
        let wire = unsafe { &transport.sent.borrow()[0].1.m_u.m_lc_mib_sysctl };
        assert_ne!(wire.newp, 0);
        assert_eq!(wire.newlen, 4);
        assert_eq!(wire.oldp, 0);
        assert_eq!(wire.oldlen, 0);
    }

    /// A name past the inline window travels by pointer only, and the
    /// inline lanes stay zero (`__sysctl.c:23-24` — the copy is gated on
    /// `CTL_SHORTNAME`).
    #[test]
    fn test_sysctl_wire_long_name_goes_by_pointer() {
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Ok(cleared_message()));
        let name = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert_eq!(name.len(), crate::misc::SYSCTL_SHORT_NAME_LENGTH + 1);
        sysctl_via(&transport, &name, None, None).unwrap();
        // SAFETY(test): 长名时 name lanes 恒零、namep 承载。
        let wire = unsafe { &transport.sent.borrow()[0].1.m_u.m_lc_mib_sysctl };
        assert_eq!(wire.namelen, 9);
        assert_ne!(wire.namep, 0);
        assert!(wire.name.iter().all(|lane| *lane == 0));
    }

    /// The buffer-sizing quirk: a server-replied failure still reports
    /// the service's length (C `__sysctl.c:29-38`), while a transport
    /// failure leaves the sink untouched (no reply exists to read).
    #[test]
    fn test_sysctl_wire_failure_length_contract() {
        // Server answered with ENOMEM and the needed size.
        let mut transport = CannedTransport::new();
        let mut reply = cleared_message();
        reply.m_type = -ENOMEM;
        // SAFETY(test): 回程长度 lane 携带"需要多大"。
        unsafe { &mut reply.m_u.m_mib_lc_sysctl }.oldlen = 88;
        transport.reply_sendrec(Ok(reply));
        let mut buffer = [0u8; 4];
        let mut length = buffer.len();
        let error = sysctl_via(
            &transport,
            &[CTL_KERN, KERN_SECURELVL],
            Some(SysctlOld {
                buffer: &mut buffer,
                length: &mut length,
            }),
            None,
        )
        .unwrap_err();
        assert_eq!(error, Errno::ENOMEM);
        assert_eq!(length, 88);

        // Transport-level failure: no reply, no length write.
        let mut transport = CannedTransport::new();
        transport.reply_sendrec(Err(TrapStatus(ENOMEM)));
        let mut buffer = [0u8; 4];
        let mut length = buffer.len();
        let error = sysctl_via(
            &transport,
            &[CTL_KERN, KERN_SECURELVL],
            Some(SysctlOld {
                buffer: &mut buffer,
                length: &mut length,
            }),
            None,
        )
        .unwrap_err();
        assert_eq!(error, Errno::ENOMEM);
        assert_eq!(length, 4);
    }

    /// The numeric-token reader mirrors `strtoimax(…, 0)`: base-0 shapes
    /// convert, trailing bytes and out-of-range values do not.
    #[test]
    fn test_parse_numeric_token_base_zero() {
        assert_eq!(parse_numeric_token(b"9"), Some(9));
        assert_eq!(parse_numeric_token(b"0x10"), Some(16));
        assert_eq!(parse_numeric_token(b"010"), Some(8));
        assert_eq!(parse_numeric_token(b"-1"), Some(-1));
        // The asymmetric C range: above INT_MAX still converts (the int
        // cast wraps), past UINT_MAX does not convert at all.
        assert_eq!(parse_numeric_token(b"4294967295"), Some(-1));
        assert_eq!(parse_numeric_token(b"4294967296"), None);
        assert_eq!(parse_numeric_token(b"-2147483648"), Some(i32::MIN));
        assert_eq!(parse_numeric_token(b"-2147483649"), None);
        // Trailing bytes mean "a name", not a partial number.
        assert_eq!(parse_numeric_token(b"12x"), None);
        assert_eq!(parse_numeric_token(b"0x"), None);
        assert_eq!(parse_numeric_token(b"kern"), None);
    }

    /// The window passes whole nodes only: a level bigger than the frame
    /// propagates the service's ENOMEM (`sysctlgetmibinfo.c` sizes from
    /// the node's child count; the fixed window answers the same errno).
    #[test]
    fn test_nametomib_oversized_level_is_enomem() {
        let mut many: Vec<(i32, &[u8])> = Vec::new();
        for index in 0..(QUERY_WINDOW_NODES as i32 + 1) {
            many.push((index, b"n"));
        }
        let tree = FakeMib {
            levels: BTreeMap::new(),
            values: BTreeMap::new(),
        }
        .with_level(&[], &many);
        let mut mib = [0i32; CTL_MAXNAME as usize];
        assert_eq!(
            nametomib_walk(&tree, b"n1", &mut mib).unwrap_err(),
            Errno::ENOMEM
        );
    }

    /// The exchange frame is 96 bytes and the walk's constant pair rides
    /// under the pinned values (`CTL_KERN 1`, `KERN_SECURELVL 9`).
    #[test]
    fn test_layout_and_constant_pins() {
        assert_eq!(NODE_BYTES, 96);
        assert_eq!(size_of::<SysctlNode>(), 96);
        assert_eq!(CTL_KERN, 1);
        assert_eq!(KERN_SECURELVL, 9);
        // The version rides the flag word's high bits, off the type mask.
        assert_eq!(SYSCTL_VERSION & SYSCTL_TYPEMASK, 0);
    }
}
