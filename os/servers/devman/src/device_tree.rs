//! Device tree: server-side device objects and their hierarchy
//! (doc 04-device-tree).
//!
//! C: `device.c:16-39` (statics), `:187-207` (`devman_init_devices`),
//! `:283-308` (`_find_dev` / `devman_find_device`), `:45-70`
//! (`devman_generate_path`).
//!
//! The tree owns [`Device`](crate::structs::Device) objects; the VTreeFS
//! inode tree (02) owns the filesystem bindings. Each device links to its
//! framework inode via [`Device::binding`](crate::structs::Device).
//! Event files/queues are 06 business and live outside this tree.

use alloc::string::String;
use minix_types::Errno;

use crate::structs::{Device, DeviceId};
use crate::vtreefs::{Ino, InodeStat, InodeTree};

/// C: `default_dir_stat` (device.c:18-24) — `S_IFDIR|0444, 0,0,0,NO_DEV`.
pub fn default_dir_stat() -> InodeStat {
    InodeStat {
        mode: crate::hooks::S_IFDIR | crate::hooks::S_IRALL,
        uid: 0,
        gid: 0,
        size: 0,
        dev: crate::hooks::NO_DEV,
    }
}

/// C: `default_file_stat` (device.c:25-31) — `S_IFREG|0444`, size
/// **`0x1000`** (not 0 — static-info files report one page; 06 relies on it).
pub fn default_file_stat() -> InodeStat {
    InodeStat {
        mode: crate::vtreefs::S_IFREG | crate::hooks::S_IRALL,
        uid: 0,
        gid: 0,
        size: 0x1000,
        dev: crate::hooks::NO_DEV,
    }
}

/// The server-side device hierarchy.
///
/// C: `root_dev` + `children`/`siblings` TAILQs + `next_device_id`
/// (device.c:16/35) as one owned value. Indexed by [`DeviceId`];
/// slot `i` holds id `i` (ids are dense: allocated in order, never reused
/// — C never reuses either, `next_device_id` only grows).
/// Deleted ids become tombstones (`None`): slots never compact, so live
/// cookies/ids keep resolving (added for 08; 04 scan notes it — the 04
/// "dense, never reused" invariant is preserved: tombstoned slots keep
/// their ids, they just hold no device).
pub struct DeviceTree {
    devices: alloc::vec::Vec<Option<Device>>,
    next_id: u32,
}

impl DeviceTree {
    /// C: `devman_init_devices()` (device.c:187-207) — wire the event file
    /// (framework-level; `read_fn` binding is 06's), init `root_dev`
    /// (id 0, no BSS ambiguity — see [`Device::root`]), add the `devices`
    /// directory + `events` file to the framework tree, init the lists.
    /// `events_stat` should be [`default_file_stat`] (size 0x1000).
    pub fn new(
        framework: &mut InodeTree,
        events_stat: InodeStat,
    ) -> Result<Self, Errno> {
        let root_ino = framework.add(framework.root(), "devices", default_dir_stat(), 0)?;
        framework.add(framework.root(), "events", events_stat, 0)?;
        let mut root = Device::root();
        root.binding = Some(crate::structs::FileBinding { ino: root_ino, cookie: None });
        Ok(DeviceTree {
            devices: alloc::vec![Some(root)],
            // C: static next_device_id = 1 (device.c:16); root took 0.
            next_id: 1,
        })
    }

    pub fn root(&self) -> DeviceId {
        DeviceId::ROOT
    }

    pub fn get(&self, id: DeviceId) -> Option<&Device> {
        self.devices
            .get(id.0 as usize)
            .and_then(|s| s.as_ref())
            .filter(|d| d.id == id)
    }

    fn get_mut(&mut self, id: DeviceId) -> Option<&mut Device> {
        self.devices
            .get_mut(id.0 as usize)
            .and_then(|s| s.as_mut())
            .filter(|d| d.id == id)
    }

    /// 08: tombstone a deleted device (slot keeps its id; never compacts).
    pub(crate) fn remove(&mut self, id: DeviceId) -> Result<(), Errno> {
        match self.devices.get_mut(id.0 as usize) {
            Some(slot @ Some(_)) => {
                *slot = None;
                Ok(())
            }
            _ => Err(Errno::EINVAL),
        }
    }

    /// 08: unlink `id` from `parent`'s children (C `TAILQ_REMOVE`,
    /// device.c:505). Missing link → `EINVAL` (C would corrupt).
    pub(crate) fn unlink_child(&mut self, parent: DeviceId, id: DeviceId) -> Result<(), Errno> {
        let p = self.get_mut(parent).ok_or(Errno::EINVAL)?;
        let at = p.children.iter().position(|&c| c == id).ok_or(Errno::EINVAL)?;
        p.children.remove(at);
        Ok(())
    }

    /// 08: mutable refcount slot (C `dev->ref_count` direct access).
    pub(crate) fn devices_mut_for_refcount(&mut self, id: DeviceId) -> Option<&mut u32> {
        self.get_mut(id).map(|d| &mut d.refcount)
    }

    /// 08/09: mutable device for state transitions (C direct field write).
    pub(crate) fn get_mut_for_state(&mut self, id: DeviceId) -> Option<&mut Device> {
        self.get_mut(id)
    }

    /// C: `_find_dev` DFS pre-order (device.c:283-301) — self first, then
    /// children in order. Iterative (explicit stack) with identical visit
    /// order; depth is pool-bounded either way.
    pub fn find(&self, id: DeviceId) -> Option<DeviceId> {
        let mut stack = alloc::vec![DeviceId::ROOT];
        while let Some(cur) = stack.pop() {
            if cur == id {
                return Some(cur);
            }
            if let Some(dev) = self.get(cur) {
                // Push reversed so the first child pops first (pre-order).
                for &c in dev.children.iter().rev() {
                    stack.push(c);
                }
            }
        }
        None
    }

    /// C: `devman_find_device()` (device.c:305-308) — find from root.
    /// (Same as [`DeviceTree::find`]: the wrapper exists in C because the
    /// recursion needs a cursor; here it is one method.)
    pub fn find_device(&self, id: DeviceId) -> Option<DeviceId> {
        self.find(id)
    }

    /// Allocate the next device id.
    /// [ARCH:A-5] C increments unboundedly (`next_device_id++`, wraps
    /// undefined at `i32::MAX`); Rust reports `ENOMEM` at `u32::MAX`.
    pub fn alloc_id(&mut self) -> Result<DeviceId, Errno> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(Errno::ENOMEM)?;
        Ok(DeviceId(id))
    }

    /// Link a fully-formed device under `parent` (07 builds the object;
    /// the linking itself is tree mechanics, hence here).
    /// The device's `id`/`parent` are authoritative: mismatches are
    /// `EINVAL` (C would corrupt the list; Rust refuses).
    pub fn insert(&mut self, parent: DeviceId, dev: Device) -> Result<DeviceId, Errno> {
        let id = dev.id;
        if dev.parent != Some(parent) {
            return Err(Errno::EINVAL);
        }
        if (id.0 as usize) != self.devices.len() {
            // Dense-id invariant (see struct docs): 07 allocates via
            // alloc_id in order, so any gap is a caller bug.
            return Err(Errno::EINVAL);
        }
        if self.get(parent).is_none() {
            return Err(Errno::EINVAL);
        }
        self.devices.push(Some(dev));
        self.get_mut(parent)
            .ok_or(Errno::EINVAL)?
            .children
            .push(id);
        Ok(id)
    }

    /// C: `devman_generate_path()` (device.c:45-70), reproduced exactly:
    /// recurse parent-first; a `None` parent appends `"./"` (C's
    /// `dev == NULL` branch appends `"."` + `"/"`); then the framework
    /// node name + `"/"` (trailing slash preserved — event strings carry
    /// it, 06 parses around it). `budget` is C's `len` parameter:
    /// `len(buf) + len(name) + len("/") + 1 > budget` → `ENOMEM`
    /// ([ARCH:A-10] explicit cap). Callers pass `DEVMAN_STRING_LEN`
    /// (bare paths) or `DEVMAN_STRING_LEN - 11` (event lines reserve
    /// `" 0x%08x"`, device.c:92/119 — 06 passes 117).
    /// Unknown ids / missing bindings are `EINVAL` (C would dereference
    /// NULL; Rust refuses).
    pub fn generate_path(
        &self,
        framework: &InodeTree,
        id: DeviceId,
        budget: usize,
    ) -> Result<String, Errno> {
        let mut buf = String::new();
        self.generate_into(framework, id, budget, &mut buf)?;
        Ok(buf)
    }

    fn generate_into(
        &self,
        framework: &InodeTree,
        id: DeviceId,
        budget: usize,
        buf: &mut String,
    ) -> Result<(), Errno> {
        let dev = self.get(id).ok_or(Errno::EINVAL)?;
        match dev.parent {
            Some(p) => self.generate_into(framework, p, budget, buf)?,
            // C: dev == NULL → name ".", then sep "/" → "./".
            None => buf.push_str("./"),
        }
        let ino: Ino = dev.binding.ok_or(Errno::EINVAL)?.ino;
        let name = framework.name(ino).ok_or(Errno::EINVAL)?;
        if buf.len() + name.len() + 1 + 1 > budget {
            return Err(Errno::ENOMEM);
        }
        buf.push_str(name);
        buf.push('/');
        Ok(())
    }

    /// Test/support: live device count.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Test/support: find by name among root's children (04-level helper;
    /// 07 uses wire ids, not names).
    pub fn lookup_child(&self, parent: DeviceId, name: &str) -> Option<DeviceId> {
        let p = self.get(parent)?;
        p.children
            .iter()
            .find_map(|&c| self.get(c).filter(|d| d.name.as_deref() == Some(name)).map(|_| c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::{DeviceState, DEVMAN_STRING_LEN};
    use crate::vtreefs::InodeTree;

    /// Insert a named child dir bound to a fresh framework inode.
    fn insert_dir(
        tree: &mut DeviceTree,
        fw: &mut InodeTree,
        parent: DeviceId,
        name: &str,
    ) -> DeviceId {
        let id = DeviceId(tree.device_count() as u32);
        assert_eq!(tree.alloc_id().unwrap(), id);
        let pino = tree.get(parent).unwrap().binding.unwrap().ino;
        let ino = fw.add(pino, name, default_dir_stat(), 0).unwrap();
        let dev = Device {
            id,
            name: Some(String::from(name)),
            refcount: 0,
            state: DeviceState::Unbound,
            owner: None,
            binding: Some(crate::structs::FileBinding { ino, cookie: None }),
            parent: Some(parent),
            info: None,
            children: alloc::vec::Vec::new(),
            attrs: alloc::vec::Vec::new(),
        };
        tree.insert(parent, dev).unwrap()
    }

    #[test]
    fn init_builds_root_and_top_files() {
        // C: devman_init_devices (device.c:187-207).
        let mut fw = InodeTree::new(64, default_dir_stat()).unwrap();
        let tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        assert_eq!(tree.root(), DeviceId::ROOT);
        assert_eq!(tree.device_count(), 1);
        // Framework holds "devices" + "events" under the VFS root.
        assert!(fw.lookup(fw.root(), "devices").is_ok());
        assert!(fw.lookup(fw.root(), "events").is_ok());
        // Root device shape (BSS made explicit).
        let r = tree.get(DeviceId::ROOT).unwrap();
        assert_eq!(r.state, DeviceState::Unbound);
        assert!(r.binding.is_some());
    }

    #[test]
    fn default_stats_match_c() {
        // C: device.c:18-24 (dir) and :25-31 (file, size 0x1000).
        let d = default_dir_stat();
        assert_eq!(d.mode, crate::hooks::S_IFDIR | crate::hooks::S_IRALL);
        assert_eq!(d.size, 0);
        let f = default_file_stat();
        assert_eq!(f.mode, crate::vtreefs::S_IFREG | crate::hooks::S_IRALL);
        assert_eq!(f.size, 0x1000);
    }

    #[test]
    fn find_is_dfs_preorder() {
        // C: _find_dev self-first, children in order (device.c:283-301).
        let mut fw = InodeTree::new(64, default_dir_stat()).unwrap();
        let mut tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        let a = insert_dir(&mut tree, &mut fw, DeviceId::ROOT, "a");
        let b = insert_dir(&mut tree, &mut fw, DeviceId::ROOT, "b");
        let a1 = insert_dir(&mut tree, &mut fw, a, "a1");
        assert_eq!(tree.find(a1), Some(a1));
        assert_eq!(tree.find(b), Some(b));
        assert_eq!(tree.find(DeviceId(99)), None);
        assert_eq!(tree.find_device(a), Some(a));
        assert_eq!(tree.lookup_child(DeviceId::ROOT, "b"), Some(b));
    }

    #[test]
    fn generate_path_reproduces_c_strings() {
        // C: "./" base + names + trailing "/" (device.c:45-70).
        let mut fw = InodeTree::new(64, default_dir_stat()).unwrap();
        let mut tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        assert_eq!(
            tree.generate_path(&fw, DeviceId::ROOT, DEVMAN_STRING_LEN).unwrap(),
            "./devices/"
        );
        let usb = insert_dir(&mut tree, &mut fw, DeviceId::ROOT, "usb");
        assert_eq!(tree.generate_path(&fw, usb, DEVMAN_STRING_LEN).unwrap(), "./devices/usb/");
        let zero = insert_dir(&mut tree, &mut fw, usb, "0");
        assert_eq!(
            tree.generate_path(&fw, zero, DEVMAN_STRING_LEN).unwrap(),
            "./devices/usb/0/"
        );
    }

    #[test]
    fn generate_path_budget_is_enomem() {
        // C: strlen(buf)+strlen(name)+strlen(sep)+1 > len → ENOMEM.
        let mut fw = InodeTree::new(256, default_dir_stat()).unwrap();
        let mut tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        let mut parent = DeviceId::ROOT;
        // ~20 six-char levels overflow 128 (each adds 7 incl. "/").
        for i in 0..20 {
            let nm = alloc::format!("d{i:04}");
            parent = insert_dir(&mut tree, &mut fw, parent, nm.as_str());
        }
        assert_eq!(
            tree.generate_path(&fw, parent, DEVMAN_STRING_LEN),
            Err(Errno::ENOMEM)
        );
    }

    #[test]
    fn generate_path_event_budget_117() {
        // Callers reserve 11 bytes for " 0x%08x" (device.c:92/119):
        // a 120-char path fits 128 but not 117.
        let mut fw = InodeTree::new(256, default_dir_stat()).unwrap();
        let mut tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        // "./devices/" (11) + 18 × 6-char levels (108) + trailing = ~120.
        let mut parent = DeviceId::ROOT;
        for i in 0..18 {
            let nm = alloc::format!("e{i:04}");
            parent = insert_dir(&mut tree, &mut fw, parent, nm.as_str());
        }
        let full = tree.generate_path(&fw, parent, DEVMAN_STRING_LEN).unwrap();
        assert!(full.len() > DEVMAN_STRING_LEN - 11);
        assert_eq!(
            tree.generate_path(&fw, parent, DEVMAN_STRING_LEN - 11),
            Err(Errno::ENOMEM)
        );
    }

    #[test]
    fn alloc_id_sequence_and_mismatch_rejected() {
        let mut fw = InodeTree::new(64, default_dir_stat()).unwrap();
        let mut tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        assert_eq!(tree.alloc_id().unwrap(), DeviceId(1));
        assert_eq!(tree.alloc_id().unwrap(), DeviceId(2));
        // insert() enforces dense ids + matching parent link.
        let bad = Device {
            id: DeviceId(99),
            name: None,
            refcount: 0,
            state: DeviceState::Unbound,
            owner: None,
            binding: None,
            parent: Some(DeviceId::ROOT),
            info: None,
            children: alloc::vec::Vec::new(),
            attrs: alloc::vec::Vec::new(),
        };
        assert_eq!(tree.insert(DeviceId::ROOT, bad), Err(Errno::EINVAL));
    }
}
