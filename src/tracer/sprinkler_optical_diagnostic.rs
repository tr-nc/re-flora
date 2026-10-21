//! Consumer-only ablation of the existing fused optical shader. No simulation or
//! preparation flags change. Only the hidden/muted garden benchmark enables it.
use crate::generated::gpu_structs::PushConstantSprinklerMist;
use anyhow::{bail, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SprinklerOpticalDiagnostic {
    Shared,
    Mist,
    Droplets,
    Both,
}

impl SprinklerOpticalDiagnostic {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "shared" => Ok(Self::Shared),
            "mist" => Ok(Self::Mist),
            "droplets" => Ok(Self::Droplets),
            "both" => Ok(Self::Both),
            _ => bail!("RE_FLORA_SPRINKLER_OPTICAL_DIAGNOSTIC expects shared|mist|droplets|both; use with the loaded, muted RE_FLORA_GARDEN_PERF fixture"),
        }
    }

    /// Apply only at the optical consumer, never at prepare or tile publication.
    /// `shared` intentionally still dispatches the empty optical pass: its time
    /// is a diagnostic floor, not the cost of normally disabling both effects.
    pub(crate) fn apply(self, push: &mut PushConstantSprinklerMist) {
        if matches!(self, Self::Shared | Self::Droplets) {
            push.density = 0.0;
        }
        if matches!(self, Self::Shared | Self::Mist) {
            push.particles_enabled = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    #[test]
    fn optical_diagnostic_accepts_only_explicit_channel_combinations() {
        for (name, expected) in [
            ("shared", SprinklerOpticalDiagnostic::Shared),
            ("mist", SprinklerOpticalDiagnostic::Mist),
            ("droplets", SprinklerOpticalDiagnostic::Droplets),
            ("both", SprinklerOpticalDiagnostic::Both),
        ] {
            assert_eq!(SprinklerOpticalDiagnostic::parse(name).unwrap(), expected);
        }
        for value in ["", "off", "all", "particles", "Both", "mist,droplets"] {
            assert!(SprinklerOpticalDiagnostic::parse(value).is_err());
        }
    }

    #[test]
    fn optical_ablation_only_changes_consumer_density_and_droplet_visibility() {
        let original = PushConstantSprinklerMist {
            count: 16,
            density: 6.0,
            shadows_enabled: 1,
            glass_enabled: 1,
            flow_enabled: 1,
            lobes_per_instance: 256,
            droplets_per_instance: 128,
            particles_enabled: 1,
            tile_size: 16,
            width: 820,
            height: 512,
            ..Zeroable::zeroed()
        };
        for (mode, density, droplets) in [
            (SprinklerOpticalDiagnostic::Shared, 0.0, 0),
            (SprinklerOpticalDiagnostic::Mist, 6.0, 0),
            (SprinklerOpticalDiagnostic::Droplets, 0.0, 1),
            (SprinklerOpticalDiagnostic::Both, 6.0, 1),
        ] {
            let mut push = original;
            mode.apply(&mut push);
            assert_eq!(push.density, density);
            assert_eq!(push.particles_enabled, droplets);
            push.density = original.density;
            push.particles_enabled = original.particles_enabled;
            assert_eq!(bytemuck::bytes_of(&push), bytemuck::bytes_of(&original));
        }
    }
}
