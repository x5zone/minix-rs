//! Startup: fresh-install sequence and signal behavior.
//!
//! C correspondence: `sef_cb_init_fresh` and `sef_cb_signal_handler` in
//! `minix3/minix/fs/mfs/main.c:47-78`. The entry point and the startup
//! handshake belong to the service-runtime stage; this module owns the four
//! ordered steps that turn an empty process into a ready server, plus the
//! pure decision behind the termination signal.

use minix_types::{EINVAL, Errno};

use minix_fs::cache::{BlockCache, BlockSource, NoSecondLevel};

/// Default pool size in blocks.
///
/// C: `DEFAULT_NR_BUFS` (`minix3/minix/fs/mfs/Makefile:11`, value one
/// thousand twenty-four). A disk-backed server starts here; the heuristic
/// from document 04 resizes later as usage figures arrive.
pub const DEFAULT_POOL_BUFFERS: usize = 1024;

/// Termination signal number (POSIX `SIGTERM`, fifteen). The handler only
/// honors this one; anything else is ignored (`main.c:73`).
pub const SIGNAL_TERMINATE: i32 = 15;

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
    cache: BlockCache<S>,
    use_vmcache: bool,
}

impl<S: BlockSource> ServerCore<S> {
    /// Shared access to the buffer pool.
    pub fn cache(&self) -> &BlockCache<S> {
        &self.cache
    }

    /// Exclusive access to the buffer pool.
    pub fn cache_mut(&mut self) -> &mut BlockCache<S> {
        &mut self.cache
    }

    /// Whether virtual-memory caching may be used.
    pub const fn uses_vmcache(&self) -> bool {
        self.use_vmcache
    }
}

/// Run the fresh-install sequence: enable the cache-use flag, then build
/// the buffer pool.
///
/// C: `sef_cb_init_fresh` (`main.c:47-65`) minus the inode steps, which the
/// inode stage owns. Order matters: the flag first (later steps consult
/// it), the pool second. A pool below the cache minimum is refused.
pub fn prepare<S: BlockSource>(source: S, config: BootConfig) -> Result<ServerCore<S>, Errno> {
    if config.pool_buffers < minix_fs::cache::MIN_POOL_SIZE {
        return Err(Errno::from_i32(EINVAL));
    }
    let cache = BlockCache::with_pool(source, NoSecondLevel, config.pool_buffers)?;
    Ok(ServerCore {
        cache,
        use_vmcache: config.use_vmcache,
    })
}

/// Shutdown decision for an incoming signal.
///
/// C: `sef_cb_signal_handler` (`main.c:70-78`): anything but termination is
/// ignored; termination syncs the file system first and then asks the
/// framework to stop. The sync itself runs through the injected callback
/// (owned by the maintenance stage); this type only records the decision so
/// the event loop stays testable without signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownDecision {
    /// Ignore the signal.
    Ignore,
    /// Sync, then terminate.
    SyncThenTerminate,
}

/// Decide what a signal means, without touching any state.
pub const fn decide_on_signal(signal: i32) -> ShutdownDecision {
    if signal == SIGNAL_TERMINATE {
        ShutdownDecision::SyncThenTerminate
    } else {
        ShutdownDecision::Ignore
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
        assert_eq!(
            decide_on_signal(SIGNAL_TERMINATE),
            ShutdownDecision::SyncThenTerminate
        );
        assert_eq!(decide_on_signal(2), ShutdownDecision::Ignore);
        assert_eq!(decide_on_signal(0), ShutdownDecision::Ignore);
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(DEFAULT_POOL_BUFFERS, 1024);
        assert_eq!(SIGNAL_TERMINATE, 15);
    }
}
