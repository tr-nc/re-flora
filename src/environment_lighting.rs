use glam::Vec3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EnvironmentLightingState {
    pub revision: u32,
}

#[derive(Debug, Default)]
pub(crate) struct EnvironmentLightingCache {
    current_revision: u32,
    last_sun_direction_bits: Option<[u32; 3]>,
}

impl EnvironmentLightingCache {
    pub fn update(&mut self, sun_direction: Vec3) -> EnvironmentLightingState {
        let sun_direction = sun_direction.normalize_or_zero();
        let direction_bits = [
            sun_direction.x.to_bits(),
            sun_direction.y.to_bits(),
            sun_direction.z.to_bits(),
        ];
        if self.last_sun_direction_bits != Some(direction_bits) {
            self.current_revision = self.current_revision.wrapping_add(1).max(1);
            self.last_sun_direction_bits = Some(direction_bits);
        }
        EnvironmentLightingState {
            revision: self.current_revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_revision_changes_only_with_environment_direction() {
        let mut cache = EnvironmentLightingCache::default();
        let first = cache.update(Vec3::Y);
        let unchanged = cache.update(Vec3::Y);
        let changed = cache.update(Vec3::Z);

        assert_eq!(first.revision, 1);
        assert_eq!(unchanged.revision, first.revision);
        assert_eq!(changed.revision, first.revision + 1);
    }

    #[test]
    fn terrain_and_raster_consumers_share_the_ddgi_sampler_contract() {
        let shared = include_str!("../shader/slang/environment_lighting.slang");
        let terrain = include_str!("../shader/slang/tracer.slang");
        let raster = include_str!("../shader/slang/flora_shadow.slang");
        let pipeline_builder = include_str!("tracer/pipeline_builder.rs");

        assert!(shared.contains("import ddgi_query;"));
        assert!(shared.contains("return sampleDdgiDiffuseEnvironment("));
        assert!(!shared.contains("SH"));
        assert!(!shared.contains("environment_probe_coefficients"));
        assert!(!shared.contains("environment_lighting_backend"));
        assert!(terrain.contains("environmentIrradiance = sampleDiffuseEnvironment("));
        assert!(raster.contains("sampleDiffuseEnvironment("));
        assert!(raster.contains("shading, voxelCenter, shadingNormal"));
        for consumer in [
            include_str!("../shader/slang/flora.vert.slang"),
            include_str!("../shader/slang/flora_lod.vert.slang"),
            include_str!("../shader/slang/leaves.vert.slang"),
            include_str!("../shader/slang/leaves_lod.vert.slang"),
        ] {
            assert!(consumer.contains("import flora_vertex;"));
        }
        for consumer in [
            include_str!("../shader/slang/dynamic_fruit.vert.slang"),
            include_str!("../shader/slang/sprinkler.vert.slang"),
            include_str!("../shader/slang/particle_lod_textured.vert.slang"),
        ] {
            assert!(consumer.contains("import flora_shadow;"));
            assert!(consumer.contains("applyStylizedVoxelLighting("));
        }
        assert!(pipeline_builder.contains("environment_lighting_resources"));
        assert!(!pipeline_builder.contains("environment_probes"));
    }

    #[test]
    fn ddgi_visibility_policy_keeps_bias_in_voxel_units_and_rejects_distant_hits() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let filter = include_str!("../shader/slang/ddgi_visibility_filter.slang");

        assert!(query.contains("lighting.environment_probe_visibility_bias_world * 0.125"));
        assert!(!query.contains("0.25 / max(gridScale"));
        assert!(filter.contains("hitDistance > supportDistance"));
        assert!(filter.contains("signedDistance >= pc.far_distance_world * 0.999"));
        assert!(filter.contains("if (!skyMiss) continue;"));
        assert!(filter.contains("hitDistance = supportDistance;"));
    }

    #[test]
    fn terrain_invalidation_fails_closed_before_the_global_sky_fallback() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let sampler = query
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironment(")
            .expect("shared DDGI sampler must exist")
            .1;
        let invalidation = sampler
            .find("ddgiQueryIsTerrainInvalidated")
            .expect("shared sampler must reject invalidated terrain");
        let global_sky = sampler
            .find("ddgiQueryUsesGlobalSky")
            .expect("shared sampler must retain the outside-volume sky fallback");

        assert!(invalidation < global_sky);
        assert!(sampler[invalidation..global_sky].contains("return result;"));
    }
}
