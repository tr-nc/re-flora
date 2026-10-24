mod axis;
pub(crate) mod gpu_pose;
pub(crate) mod kick;
pub mod pose;
pub mod skin;
mod species;
#[cfg(test)]
mod species_tests;
mod tree;
pub use axis::AxisSegment;
pub(crate) use axis::MAX_AXIS_SEGMENTS;
pub use species::TreeSpecies;
pub use tree::{LeafPlacement, Tree, TreeDesc};
