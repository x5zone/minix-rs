//! Startup: fresh-install sequence and signal behavior.
//!
//! C correspondence: `sef_cb_init_fresh` and `sef_cb_signal_handler` in
//! `minix3/minix/fs/mfs/main.c:47-78`. The entry point and the startup
//! handshake belong to the service-runtime stage; this module owns the four
//! ordered steps that turn an empty process into a ready server, plus the
//! pure decision behind the termination signal.

use minix_types::{EINVAL, Errno};

use minix_fs::cache::{BlockCache, BlockSource, SecondLevelCache};

use crate::second_level::MfsSecondLevel;

/// Default pool size in blocks.
///
/// C: `DEFAULT_NR_BUFS` (`minix3/minix/fs/mfs/Makefile:11`, value one
/// thousand twenty-four). A disk-backed server starts here; the heuristic
/// from document 04 resizes later as usage figures arrive.
pub const DEFAULT_POOL_BUFFERS: usize = 1024;

/// Boot configuration: the two knobs the fresh-install sequence needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootConfig {
    /// Pool size in blocks (defaults to [`DEFAULT_POOL_BUFFERS`]).
    pub pool_buffers: usize,
    /// Whether the virtual-memory second level may be used later.
    pub use_vmcache: bool,
}

impl BootConfig {
    /// Standard boot configuration.
    pub const fn standard() -> Self {
        Self {
            pool_buffers: DEFAULT_POOL_BUFFERS,
            use_vmcache: true,
        }
    }
}

/// A ready server core: buffer pool plus the cache-use flag.
///
/// The inode table zeroing and inode-cache init from the C sequence belong
/// to the inode stage (document 09); this core carries what the startup
/// stage owns: the pool and the flag. Wiring grows as later documents land.
#[derive(Debug)]
pub struct ServerCore<S: BlockSource> {
    cache: BlockCache<S, MfsSecondLevel>,
    use_vmcache: bool,
}

impl<S: BlockSource> ServerCore<S> {
    /// Shared access to the buffer pool.
    pub fn cache(&self) -> &BlockCache<S, MfsSecondLevel> {
        &self.cache
    }

    /// Exclusive access to the buffer pool.
    pub fn cache_mut(&mut self) -> &mut BlockCache<S, MfsSecondLevel> {
        &mut self.cache
    }

    /// Whether virtual-memory caching may be used.
    pub const fn uses_vmcache(&self) -> bool {
        self.use_vmcache
    }

    /// Whether the second level is actually in play: the flag above, and a
    /// block size that fills whole pages (the pool's own decision,
    /// `cache.c:1236-1239`).
    pub fn vmcache_active(&self) -> bool {
        self.cache.second_level().is_enabled()
    }
}

/// Run the fresh-install sequence: enable the cache-use flag, then build
/// the buffer pool.
///
/// C: `sef_cb_init_fresh` (`main.c:47-65`) minus the inode steps, which the
/// inode stage owns. Order matters: the flag first (later steps consult
/// it), the pool second. A pool below the cache minimum is refused.
///
/// The second level follows the flag: with `use_vmcache` set, blocks come
/// from the memory server and are offered to its page cache; the pool
/// switches the level off by itself when the block size is not a whole page
/// (cache.c:1236-1239).
pub fn prepare<S: BlockSource>(source: S, config: BootConfig) -> Result<ServerCore<S>, Errno> {
    if config.pool_buffers < minix_fs::cache::MIN_POOL_SIZE {
        return Err(Errno::from_i32(EINVAL));
    }
    let second_level = if config.use_vmcache {
        MfsSecondLevel::vm(crate::second_level::production_wire())
    } else {
        MfsSecondLevel::off()
    };
    let cache = BlockCache::with_pool(source, second_level, config.pool_buffers)?;
    Ok(ServerCore {
        cache,
        use_vmcache: config.use_vmcache,
    })
}

/// Shutdown decision for an incoming signal.
///
/// C: `sef_cb_signal_handler` (`main.c:70-78`): anything but termination is
/// ignored; termination syncs the file system first and then asks the
/// framework to stop. The sync itself runs through the driver's own hook
/// (`FsDriver::synchronized`), so this answers only the decision and the task
/// loop performs the flush
/// (`minix_fs::task::Incoming::SyncThenCancelled`).
pub const fn decide_on_signal(signal: i32) -> minix_fs_rt::transport::SignalAction {
    if signal == minix_types::SIGNAL_TERMINATE {
        minix_fs_rt::transport::SignalAction::SyncThenTerminate
    } else {
        minix_fs_rt::transport::SignalAction::Ignore
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_fs::bio::RamDisk;

    #[test]
    fn test_prepare_builds_pool_and_flag() {
        let disk = RamDisk::new(16, 512).unwrap();
        let core = prepare(disk, BootConfig::standard()).unwrap();
        assert_eq!(core.cache().pool_size(), DEFAULT_POOL_BUFFERS);
        assert!(core.uses_vmcache());
        // A custom config is honored.
        let disk = RamDisk::new(16, 512).unwrap();
        let config = BootConfig {
            pool_buffers: 64,
            use_vmcache: false,
        };
        let core = prepare(disk, config).unwrap();
        assert_eq!(core.cache().pool_size(), 64);
        assert!(!core.uses_vmcache());
    }

    #[test]
    fn test_prepare_refuses_tiny_pool() {
        let disk = RamDisk::new(16, 512).unwrap();
        let config = BootConfig {
            pool_buffers: minix_fs::cache::MIN_POOL_SIZE - 1,
            use_vmcache: true,
        };
        assert_eq!(prepare(disk, config).unwrap_err().to_i32(), EINVAL);
    }

    #[test]
    fn test_signal_decision() {
        use minix_fs_rt::transport::SignalAction;
        assert_eq!(
            decide_on_signal(minix_types::SIGNAL_TERMINATE),
            SignalAction::SyncThenTerminate
        );
        // A kernel wake-up number is not a process signal: mfs keeps serving.
        assert_eq!(decide_on_signal(2), SignalAction::Ignore);
        assert_eq!(decide_on_signal(0), SignalAction::Ignore);
        assert_eq!(
            decide_on_signal(minix_types::SIGNAL_KERNEL_MEMORY),
            SignalAction::Ignore
        );
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(DEFAULT_POOL_BUFFERS, 1024);
        assert_eq!(minix_types::SIGNAL_TERMINATE, 15);
    }
}
