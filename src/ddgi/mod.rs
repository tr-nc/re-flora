//! Dynamic diffuse global illumination domain types.
//!
//! Atlas addressing and octahedral wrapping live here so callers can work in probe-space without
//! depending on the physical GPU texture layout.

mod atlas;
#[cfg_attr(not(test), allow(dead_code))]
mod octahedral;
mod resources;

pub use atlas::{
    DdgiAtlasLayout, DdgiVolumeGrid, DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE,
    DDGI_PROBE_BATCH_SIZE, DDGI_RAYS_PER_PROBE, DDGI_RELOCATION_WORKGROUP_SIZE,
    DDGI_VISIBILITY_INTERIOR_SIDE,
};
pub use resources::DdgiVolume;
