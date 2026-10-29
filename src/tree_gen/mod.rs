mod axis;
mod compatibility;
#[cfg(test)]
mod compatibility_capture;
mod foliage;
pub(crate) mod gpu_pose;
mod growth;
pub(crate) mod kick;
pub mod pose;
pub mod skin;
mod species;
#[cfg(test)]
mod species_tests;
mod tree;
mod variation;
pub use axis::AxisSegment;
pub(crate) use axis::MAX_AXIS_SEGMENTS;
pub use species::TreeSpecies;
pub use tree::{LeafPlacement, Tree, TreeDesc};
pub use variation::{TreeAppearance, TreeVariation};
