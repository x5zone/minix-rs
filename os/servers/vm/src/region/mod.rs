//! Memory region management module.

pub(crate) mod vir_region;
pub(crate) mod phys_region;
pub(crate) mod avl;

pub(crate) use vir_region::{VirRegion, VrFlags};
pub(crate) use phys_region::{PhysRegion, PhysBlock};
pub(crate) use avl::RegionAvl;
