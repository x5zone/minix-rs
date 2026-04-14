//! 内存区域管理模块
//!
//! 提供虚拟区域 (vir_region) 和物理区域 (phys_region) 的管理。
//! 对应 Minix3: `region.h`, `phys_region.h`

pub mod vir_region;
pub mod phys_region;
pub mod avl;

pub use vir_region::{VirRegion, VrFlags};
pub use phys_region::{PhysRegion, PhysBlock};
pub use avl::RegionAvl;
