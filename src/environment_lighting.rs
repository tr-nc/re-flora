use glam::Vec3;

// Authored sky lighting is compiled into these shaders rather than supplied through a runtime
// uniform. Hash the authoritative sources so a capture or cached field can still name the exact
// sky model that produced it. Adding runtime sky controls later should replace this compilation-
// bound identity with their explicit snapshot values.
pub(crate) const DDGI_AUTHORED_SKY_MODEL_IDENTITY: u64 = authored_sky_model_identity();
const FNV1A64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

const fn authored_sky_model_identity() -> u64 {
    let mut hash = FNV1A64_OFFSET_BASIS;
    hash = hash_bytes(
        hash,
        include_bytes!("../shader/slang/sky_environment_data.slang"),
    );
    hash = hash_bytes(hash, include_bytes!("../shader/slang/skylight.slang"));
    hash = hash_bytes(
        hash,
        include_bytes!("../shader/slang/ddgi_global_sky_filter.slang"),
    );
    hash_bytes(
        hash,
        include_bytes!("../shader/slang/ddgi_probe_trace.slang"),
    )
}

const fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(FNV1A64_PRIME);
        index += 1;
    }
    hash
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiVoxelPaletteSnapshot {
    pub dirt_color: Vec3,
    pub sand_color: Vec3,
    pub cherry_wood_color: Vec3,
    pub oak_wood_color: Vec3,
    pub rock_color: Vec3,
    pub hash_color_variance: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DdgiRadianceSnapshot {
    pub sun_direction: Vec3,
    pub sun_color: Vec3,
    pub sun_luminance: f32,
    pub terrain_ray_origin_offset_world: f32,
    pub ddgi_receiver_visibility_bias_world: f32,
    pub voxel_palette: DdgiVoxelPaletteSnapshot,
}

impl DdgiRadianceSnapshot {
    fn identity(self) -> DdgiRadianceIdentity {
        self.identity_for_authored_sky(DDGI_AUTHORED_SKY_MODEL_IDENTITY)
    }

    fn identity_for_authored_sky(self, authored_sky_model_identity: u64) -> DdgiRadianceIdentity {
        DdgiRadianceIdentity {
            authored_sky_model_identity,
            sun_direction: self.sun_direction.to_array().map(f32::to_bits),
            sun_color: self.sun_color.to_array().map(f32::to_bits),
            sun_luminance: self.sun_luminance.to_bits(),
            terrain_ray_origin_offset_world: self.terrain_ray_origin_offset_world.to_bits(),
            ddgi_receiver_visibility_bias_world: self.ddgi_receiver_visibility_bias_world.to_bits(),
            dirt_color: self.voxel_palette.dirt_color.to_array().map(f32::to_bits),
            sand_color: self.voxel_palette.sand_color.to_array().map(f32::to_bits),
            cherry_wood_color: self
                .voxel_palette
                .cherry_wood_color
                .to_array()
                .map(f32::to_bits),
            oak_wood_color: self
                .voxel_palette
                .oak_wood_color
                .to_array()
                .map(f32::to_bits),
            rock_color: self.voxel_palette.rock_color.to_array().map(f32::to_bits),
            hash_color_variance: self.voxel_palette.hash_color_variance.to_bits(),
        }
    }
}

impl PartialEq for DdgiRadianceSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DdgiRadianceIdentity {
    authored_sky_model_identity: u64,
    sun_direction: [u32; 3],
    sun_color: [u32; 3],
    sun_luminance: u32,
    terrain_ray_origin_offset_world: u32,
    ddgi_receiver_visibility_bias_world: u32,
    dirt_color: [u32; 3],
    sand_color: [u32; 3],
    cherry_wood_color: [u32; 3],
    oak_wood_color: [u32; 3],
    rock_color: [u32; 3],
    hash_color_variance: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnvironmentLightingState {
    pub revision: u32,
    pub snapshot: DdgiRadianceSnapshot,
}

#[derive(Debug, Default)]
pub(crate) struct EnvironmentLightingCache {
    current_revision: u32,
    last_identity: Option<DdgiRadianceIdentity>,
}

impl EnvironmentLightingCache {
    pub fn update(&mut self, mut snapshot: DdgiRadianceSnapshot) -> EnvironmentLightingState {
        self.update_for_authored_sky(&mut snapshot, DDGI_AUTHORED_SKY_MODEL_IDENTITY)
    }

    fn update_for_authored_sky(
        &mut self,
        snapshot: &mut DdgiRadianceSnapshot,
        authored_sky_model_identity: u64,
    ) -> EnvironmentLightingState {
        snapshot.sun_direction = snapshot.sun_direction.normalize_or_zero();
        let identity = snapshot.identity_for_authored_sky(authored_sky_model_identity);
        if self.last_identity != Some(identity) {
            self.current_revision = self.current_revision.wrapping_add(1).max(1);
            self.last_identity = Some(identity);
        }
        EnvironmentLightingState {
            revision: self.current_revision,
            snapshot: *snapshot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gui_param<'a>(config: &'a toml::Value, id: &str) -> &'a toml::Value {
        config["section"]
            .as_array()
            .expect("GUI config must contain sections")
            .iter()
            .flat_map(|section| section["param"].as_array().into_iter().flatten())
            .find(|param| param["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("missing GUI parameter {id}"))
    }

    fn snapshot() -> DdgiRadianceSnapshot {
        DdgiRadianceSnapshot {
            sun_direction: Vec3::Y,
            sun_color: Vec3::new(1.0, 0.9, 0.8),
            sun_luminance: 2.0,
            terrain_ray_origin_offset_world: 0.005,
            ddgi_receiver_visibility_bias_world: 0.001,
            voxel_palette: DdgiVoxelPaletteSnapshot {
                dirt_color: Vec3::new(0.1, 0.2, 0.3),
                sand_color: Vec3::new(0.4, 0.5, 0.6),
                cherry_wood_color: Vec3::new(0.7, 0.2, 0.1),
                oak_wood_color: Vec3::new(0.2, 0.3, 0.1),
                rock_color: Vec3::splat(0.4),
                hash_color_variance: 0.5,
            },
        }
    }

    fn sample_linear_probe_field(position_in_probe_cells: f64) -> f64 {
        let base = position_in_probe_cells.floor();
        let fraction = position_in_probe_cells - base;
        base * (1.0 - fraction) + (base + 1.0) * fraction
    }

    fn canonical_terrain_voxel_center_in_probe_cells(
        position_in_probe_cells: f64,
        terrain_voxels_per_probe: f64,
    ) -> f64 {
        ((position_in_probe_cells * terrain_voxels_per_probe).floor() + 0.5)
            / terrain_voxels_per_probe
    }

    #[test]
    fn continuous_terrain_position_basis_does_not_quantize_a_linear_probe_field() {
        let epsilon = 1.0e-6;
        let left = 1.0 - epsilon;
        let right = 1.0 + epsilon;
        let exact_delta = sample_linear_probe_field(right) - sample_linear_probe_field(left);

        assert!((exact_delta - 2.0 * epsilon).abs() < 1.0e-12);

        let terrain_voxels_per_probe = 32.0;
        let canonical_left =
            canonical_terrain_voxel_center_in_probe_cells(left, terrain_voxels_per_probe);
        let canonical_right =
            canonical_terrain_voxel_center_in_probe_cells(right, terrain_voxels_per_probe);
        let quantized_delta =
            sample_linear_probe_field(canonical_right) - sample_linear_probe_field(canonical_left);

        assert!((quantized_delta - 1.0 / terrain_voxels_per_probe).abs() < 1.0e-12);
        assert!(quantized_delta > exact_delta * 10_000.0);
    }

    #[test]
    fn cache_revision_is_stable_for_an_identical_radiance_snapshot() {
        let mut cache = EnvironmentLightingCache::default();
        let first = cache.update(snapshot());
        let unchanged = cache.update(snapshot());

        assert_eq!(first.revision, 1);
        assert_eq!(unchanged.revision, first.revision);
        assert_eq!(unchanged.snapshot, first.snapshot);
    }

    #[test]
    fn cache_revision_covers_every_transport_radiance_input() {
        let mut variants = Vec::new();
        let mut value = snapshot();
        value.sun_direction = Vec3::Z;
        variants.push(value);
        value = snapshot();
        value.sun_color.x += 0.1;
        variants.push(value);
        value = snapshot();
        value.sun_luminance += 0.1;
        variants.push(value);
        value = snapshot();
        value.terrain_ray_origin_offset_world += 0.001;
        variants.push(value);
        value = snapshot();
        value.ddgi_receiver_visibility_bias_world += 0.001;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.dirt_color.x += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.sand_color.y += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.cherry_wood_color.z += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.oak_wood_color.x += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.rock_color.y += 0.1;
        variants.push(value);
        value = snapshot();
        value.voxel_palette.hash_color_variance += 0.1;
        variants.push(value);

        for changed in variants {
            let mut cache = EnvironmentLightingCache::default();
            let first = cache.update(snapshot());
            let changed = cache.update(changed);
            assert_eq!(changed.revision, first.revision + 1);
        }
    }

    #[test]
    fn cache_revision_covers_the_compiled_authored_sky_model() {
        assert_ne!(DDGI_AUTHORED_SKY_MODEL_IDENTITY, 0);
        assert_eq!(
            snapshot().identity().authored_sky_model_identity,
            DDGI_AUTHORED_SKY_MODEL_IDENTITY,
        );

        let mut cache = EnvironmentLightingCache::default();
        let mut value = snapshot();
        let first = cache.update_for_authored_sky(&mut value, DDGI_AUTHORED_SKY_MODEL_IDENTITY);
        let changed = cache
            .update_for_authored_sky(&mut value, DDGI_AUTHORED_SKY_MODEL_IDENTITY.wrapping_add(1));

        assert_eq!(changed.revision, first.revision + 1);
        assert_eq!(changed.snapshot, first.snapshot);
    }

    #[test]
    fn cache_identity_uses_the_normalized_sun_direction() {
        let mut cache = EnvironmentLightingCache::default();
        let first = cache.update(snapshot());
        let mut scaled = snapshot();
        scaled.sun_direction *= 10.0;
        let unchanged = cache.update(scaled);

        assert_eq!(unchanged.revision, first.revision);
        assert_eq!(unchanged.snapshot.sun_direction, Vec3::Y);
    }

    #[test]
    fn terrain_and_raster_consumers_share_the_ddgi_sampler_contract() {
        let shared = include_str!("../shader/slang/environment_lighting.slang");
        let terrain = include_str!("../shader/slang/tracer.slang");
        let raster = include_str!("../shader/slang/flora_shadow.slang");
        let pipeline_builder = include_str!("tracer/pipeline_builder.rs");
        let tracer_host = include_str!("tracer/mod.rs");

        assert!(shared.contains("import ddgi_query;"));
        assert!(shared.contains("return sampleDdgiDiffuseEnvironment("));
        assert!(!shared.contains("SH"));
        assert!(!shared.contains("environment_probe_coefficients"));
        assert!(!shared.contains("environment_lighting_backend"));
        assert!(terrain.contains("consumerResult = sampleDdgiTerrainSmoothEnvironment("));
        assert!(terrain.contains("environmentIrradiance = consumerResult.irradiance"));
        assert!(terrain.contains("environmentCaptureIrradiance = consumerResult.irradiance"));
        assert!(terrain.contains("color = environmentIrradiance * albedo"));
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

        let consumer_update = tracer_host
            .split_once("fn stage_ddgi_consumer_descriptors")
            .expect("DDGI consumer promotion seam must exist")
            .1
            .split_once("fn publish_ddgi_consumer_descriptors")
            .expect("staged consumer descriptors must have an explicit publication seam")
            .0;
        assert!(consumer_update.contains("compute_pipelines"));
        assert!(consumer_update.contains("tracer_ppl"));
        assert!(consumer_update.contains("graphics_pipelines"));
        assert!(consumer_update.contains("flora_ppl"));
        assert!(consumer_update.contains("ddgi_probe_metadata"));
        assert!(consumer_update.contains("ddgi_irradiance_atlas"));
        assert!(consumer_update.contains("ddgi_visibility_atlas"));

        let promotion = tracer_host
            .split_once("fn promote_ready_ddgi_staging")
            .expect("DDGI promotion must exist")
            .1;
        let descriptor_rebind = promotion
            .find("publish_ddgi_consumer_descriptors")
            .expect("promotion must rebind every shared consumer");
        let ownership_swap = promotion
            .find("promote_staging(build_token)")
            .expect("promotion must swap the same builder token");
        assert!(descriptor_rebind < ownership_swap);
        assert!(promotion.contains("[DDGI][CONSUMERS]"));
        assert!(promotion.contains("consumer_set=terrain_compute,flora_raster"));
        assert!(tracer_host.contains("[DDGI][FLORA_CONSUMER] draw_recorded"));
        assert!(tracer_host.contains("recorded_flora_instance_count > 0"));
    }

    #[test]
    fn path_tracing_reference_is_terrain_only_and_bypasses_ddgi() {
        let shared = include_str!("../shader/slang/environment_lighting.slang");
        let terrain = include_str!("../shader/slang/tracer.slang");
        let raster = include_str!("../shader/slang/flora_shadow.slang");
        let path_tracing_branch = terrain
            .split_once("if (gui_input.path_tracing_reference != 0u")
            .expect("terrain shader must expose the path-tracing GUI switch")
            .1
            .split_once("// The moment-visibility receiver remains fixed")
            .expect("path-tracing branch must remain ahead of the DDGI query")
            .0;

        assert!(!shared.contains("path_tracing_reference"));
        assert!(!terrain.contains("raster_flora_ddgi_lighting"));
        assert!(shared.contains("return sampleDdgiDiffuseEnvironment("));
        assert!(path_tracing_branch.contains("pathTraceTerrainReference("));
        assert!(path_tracing_branch.contains("return;"));
        assert!(!path_tracing_branch.contains("sampleDdgiDiffuseEnvironment"));
        assert!(raster.contains("applyStylizedVoxelLighting(U_GuiInput gui"));
        assert!(raster.contains("sampleDiffuseEnvironment(\n        gui, shading"));
        for consumer in [
            include_str!("../shader/slang/flora_vertex.slang"),
            include_str!("../shader/slang/dynamic_fruit.vert.slang"),
            include_str!("../shader/slang/sprinkler.vert.slang"),
            include_str!("../shader/slang/particle_lod_textured.vert.slang"),
        ] {
            assert!(consumer.contains("gui_input, sun_info, shading_info"));
        }
    }

    #[test]
    fn raster_flora_lighting_switch_preserves_legacy_and_ddgi_paths() {
        let config: toml::Value = toml::from_str(include_str!("../config/gui.toml"))
            .expect("GUI config must be valid TOML");
        let switch = gui_param(&config, "raster_flora_ddgi_lighting");
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let lighting = include_str!("../shader/slang/flora_shadow.slang");
        let shared = include_str!("../shader/slang/flora_vertex.slang");
        let flora_cache = include_str!("../shader/slang/flora_lighting_cache.comp.slang");
        let flora = include_str!("../shader/slang/flora.vert.slang");
        let flora_lod = include_str!("../shader/slang/flora_lod.vert.slang");
        let tree_leaf_cache = include_str!("../shader/slang/tree_leaf_lighting_cache.comp.slang");
        let leaves = include_str!("../shader/slang/leaves.vert.slang");
        let leaves_lod = include_str!("../shader/slang/leaves_lod.vert.slang");

        assert_eq!(switch["kind"].as_str(), Some("bool"));
        assert_eq!(switch["data"]["value"].as_bool(), Some(true));
        assert!(lighting.contains("float3(24.0 / 255.0)"));
        assert!(lighting.contains("sunLight * shadowWeight + LEGACY_RASTER_FLORA_AMBIENT_LIGHT"));
        assert!(shared.contains("applyLegacyRasterFloraLighting("));
        let runtime_query = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("DDGI must expose the runtime consumer query")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("runtime consumer query must remain isolated from terrain smoothing")
            .0;
        assert!(runtime_query.contains("getDdgiMomentProbeContribution("));
        assert!(runtime_query.contains("contribution.moment_visibility"));
        assert!(!runtime_query.contains("getDdgiMomentExactProbeContribution("));
        let flora_environment = shared
            .split_once("public float3 sampleFloraEnvironment(")
            .expect("raster flora must have a shared environment query")
            .1
            .split_once("public float3 shadeFloraVertexWithEnvironment(")
            .expect("flora environment query must remain isolated from shading")
            .0;
        assert!(flora_environment.contains("sampleDiffuseEnvironment("));
        assert_eq!(flora_cache.matches("sampleFloraEnvironment(").count(), 1);
        for shader in [flora, flora_lod] {
            let lighting_branch = shader
                .split_once("if (rasterFloraUsesDdgiLighting())")
                .expect("raster flora shader must branch before cache access")
                .1;
            assert!(lighting_branch.contains("flora_lighting_cache.irradiance["));
            assert!(lighting_branch.contains("shadeLegacyRasterFloraVertex("));
        }
        assert_eq!(
            tree_leaf_cache.matches("sampleFloraEnvironment(").count(),
            1
        );
        assert!(
            tree_leaf_cache.contains("floraLightingCacheIndex(floraPc, localInstanceIndex, 0u)")
        );
        assert!(!tree_leaf_cache.contains("vertexOffset"));
        for shader in [leaves, leaves_lod] {
            let lighting_branch = shader
                .split_once("if (rasterFloraUsesDdgiLighting())")
                .expect("tree-leaf shader must branch before cache access")
                .1;
            assert!(lighting_branch.contains("flora_lighting_cache.irradiance["));
            assert!(lighting_branch.contains("shadeTreeLeafVertexWithEnvironment("));
            assert!(lighting_branch.contains("shadeLegacyTreeLeafVertex("));
            assert!(!shader.contains("sampleFloraEnvironment("));
        }
        let tree_leaf_finish = shared
            .split_once("float3 finishTreeLeafShading(")
            .expect("tree-leaf view-dependent finishing helper must exist")
            .1;
        assert!(tree_leaf_finish.contains("backlightVisibility"));
        assert!(tree_leaf_finish.contains("applyTerrainEditPreviewTint("));
    }

    #[test]
    fn path_tracing_controls_and_transport_are_validated_semantically() {
        let terrain = include_str!("../shader/slang/tracer.slang");
        let skylight = include_str!("../shader/slang/skylight.slang");
        let ddgi_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        let ddgi_sky = include_str!("../shader/slang/ddgi_global_sky_filter.slang");
        let config: toml::Value = toml::from_str(include_str!("../config/gui.toml"))
            .expect("GUI config must be valid TOML");
        let reference = gui_param(&config, "path_tracing_reference");
        let ambient = gui_param(&config, "path_tracing_ambient_light");
        let max_bounces = gui_param(&config, "path_tracing_max_bounces");
        let ray_origin_offset = gui_param(&config, "terrain_ray_origin_offset_world");
        let receiver_visibility_bias = gui_param(&config, "ddgi_receiver_visibility_bias_world");

        assert_eq!(reference["kind"].as_str(), Some("bool"));
        assert_eq!(ambient["kind"].as_str(), Some("color"));
        assert_eq!(max_bounces["kind"].as_str(), Some("uint"));
        assert_eq!(ray_origin_offset["kind"].as_str(), Some("float"));
        assert_eq!(ray_origin_offset["data"]["min"].as_integer(), Some(0));
        assert_eq!(ray_origin_offset["data"]["max"].as_float(), Some(0.02));
        assert_eq!(receiver_visibility_bias["kind"].as_str(), Some("float"));
        assert_eq!(
            receiver_visibility_bias["data"]["min"].as_integer(),
            Some(0)
        );
        assert_eq!(
            receiver_visibility_bias["data"]["max"].as_float(),
            Some(0.02)
        );
        assert_eq!(
            receiver_visibility_bias["data"]["value"].as_float(),
            Some(1.0 / 256.0),
            "the default visibility receiver bias must remain one terrain voxel"
        );
        for dependent in [ambient, max_bounces] {
            assert_eq!(
                dependent["enabled_if"]["param"].as_str(),
                Some("path_tracing_reference")
            );
            assert_eq!(dependent["enabled_if"]["equals"].as_bool(), Some(true));
        }

        let transport = terrain
            .split_once("float3 pathTracingDirectIrradiance(")
            .expect("path tracer must evaluate direct sun independently")
            .1
            .split_once("float depthFromWorldPosition(")
            .expect("path-tracing transport must remain a bounded terrain helper")
            .0;
        assert!(terrain.contains("import skylight;"));
        assert!(transport.contains("getAuthoredSkyRadiance("));
        assert!(transport.contains("sampleDiffuseBounce("));
        assert!(transport.contains("sampleSunDisk("));
        assert!(transport.contains("generalSceneMarching(shadowRay"));
        assert!(transport.contains("generalSceneMarching(indirectRay"));
        assert!(transport.contains("gui_input.path_tracing_max_bounces"));
        assert!(!transport.contains("sampleDdgi"));
        assert!(!transport.contains("directSunShadowTransmittance"));
        assert!(!transport.contains("shadow_map"));
        assert!(!transport.contains("leaf_shadow"));
        assert!(!transport.contains("cloud_shadow"));

        assert!(skylight.contains("public float3 getAuthoredSkyRadiance("));
        assert!(ddgi_trace.contains("getAuthoredSkyRadiance("));
        assert!(ddgi_sky.contains("getAuthoredSkyRadiance("));
    }

    #[test]
    fn ddgi_visibility_policy_uses_adjustable_world_bias_and_rejects_distant_hits() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let filter = include_str!("../shader/slang/ddgi_visibility_filter.slang");

        assert!(query.contains("max(0.0, query.visibility_bias_world)"));
        assert!(!query.contains("visibility_bias_world * 0.125"));
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
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
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

    #[test]
    fn consumer_and_transport_adapters_share_probe_core_but_keep_distinct_visibility() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let consumer_implementation = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("runtime consumer implementation must exist")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain smoothing must follow the runtime consumer")
            .0;
        let consumer = query
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironment(")
            .expect("consumer adapter must exist")
            .1
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport implementation must follow the consumer adapter")
            .0;
        let transport = query
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport implementation must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport adapter must follow its implementation")
            .0;

        assert_eq!(
            consumer_implementation
                .matches("for (uint z = 0u; z < 2u; ++z)")
                .count(),
            1
        );
        assert_eq!(
            transport.matches("for (uint z = 0u; z < 2u; ++z)").count(),
            1
        );
        assert!(consumer.contains("sampleDdgiDiffuseEnvironmentFromAtlas("));
        assert!(consumer.contains("ddgi_irradiance_atlas"));
        assert!(transport.contains("getDdgiMomentExactProbeContributionFromAtlases("));

        let trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        assert!(!trace.contains("ConstantBuffer<U_SunInfo>"));
        assert!(!trace.contains("ConstantBuffer<U_ShadingInfo>"));
        assert!(trace.contains("[[vk::binding(29, 0)]]"));
        assert!(trace.contains("[[vk::binding(30, 0)]]"));
        assert!(trace.contains("[[vk::binding(31, 0)]]"));
        assert!(trace.contains("[[vk::binding(32, 0)]]"));
    }

    #[test]
    fn transport_multiplies_exact_and_moment_while_runtime_consumers_use_moment_only() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let runtime = query
            .split_once("DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("runtime DDGI sampler must exist")
            .1
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain smoothing must follow runtime sampler")
            .0;
        let transport = query
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport DDGI sampler must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport adapter must follow transport sampler")
            .0;
        assert!(query.contains("import ddgi_voxel_visibility;"));
        assert!(query.contains("ddgiVoxelSegmentVisibility("));
        assert!(query.contains("worldPosition + normal * biasWorld"));
        assert!(query.contains("float3 hardVisibilityWorldPosition"));
        assert!(!query.contains("surfaceOutward"));
        let probe_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        assert!(!probe_trace.contains("surfaceOutward"));
        assert!(probe_trace.contains(
            "terrainVoxelSurfacePositionAlongNormal(\n        result.center_position, normal)"
        ));
        assert!(probe_trace.contains(
            "terrainRayOriginAlongNormal(\n        result.center_position, normal,\n        ddgi_radiance_sun.terrain_ray_origin_offset_world)"
        ));
        assert!(probe_trace.contains("ddgiHardVisibilityPosition"));
        assert!(!probe_trace.contains("ddgi_transport_query_info, result.position"));
        assert!(runtime.contains("result, contribution, contribution.moment_visibility"));
        assert!(!runtime.contains("contribution.hard_visibility"));
        assert!(transport.contains("contribution.moment_visibility *"));
        assert!(transport.contains("contribution.hard_visibility"));

        let tracer = include_str!("../shader/slang/tracer.slang");
        assert!(!tracer.contains("result.position, result.normal, -ray.direction"));
        assert!(tracer.contains(
            "terrainVoxelSurfacePositionAlongNormal(\n        result.center_position, result.normal)"
        ));
        assert!(tracer.contains("terrainDdgiHardVisibilityOrigin("));
        assert!(tracer.contains("gui_input.terrain_ray_origin_offset_world"));
        assert!(tracer.contains(
            "surfacePosition +\n            normalDirection * gui_input.terrain_ray_origin_offset_world"
        ));
        assert!(tracer.contains(
            "sampleDdgiTerrainSmoothEnvironment(\n        shading_info, ddgiReceiverPosition, result.position,\n        result.normal)"
        ));
        assert!(!tracer.contains("register(t39"));
        assert!(!tracer.contains("register(t40"));
        assert!(!tracer.contains("register(t42"));
        let terrain_smooth = query
            .split_once("DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(")
            .expect("terrain smooth Moment query must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTerrainSmoothEnvironment(")
            .expect("terrain smooth adapter must follow its implementation")
            .0;
        assert!(terrain_smooth.contains("getDdgiMomentSpatialWeightProbeContributionAt("));
        assert!(terrain_smooth.contains("DDGI_SPATIAL_WEIGHT_NOMINAL_HARD"));
        assert!(!terrain_smooth.contains("hardVisibilityWorldPosition"));
        assert!(query.contains("getDdgiMomentSpatialWeightProbeContributionAt("));
        assert!(query.contains("surfaceSideWeight = sqrt(max(0.0, surfaceAlignment));"));
        assert!(query.contains("float3 biasedWorldPosition = worldPosition + normal * biasWorld;"));
        assert!(query.contains(
            "ddgiVoxelSegmentVisibility(\n        hardVisibilityWorldPosition, contribution.actual_position"
        ));
        assert!(
            query.contains("float3 surfaceToProbe = actualPosition - positionWeightWorldPosition;")
        );
        let exact_reference = query
            .split_once("public DdgiQueryResult sampleDdgiExactTerrainReference(")
            .expect("exact voxel reference must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiUnoccludedTerrainReference(")
            .expect("exact reference must remain isolated")
            .0;
        assert!(exact_reference.contains("contribution.hard_visibility"));
    }

    #[test]
    fn irradiance_filter_forbids_relative_rgb_history_resets() {
        let filter = include_str!("../shader/slang/ddgi_irradiance_filter.slang");
        let history_block = filter
            .split_once("if (pc.has_history != 0u)")
            .expect("irradiance history block must exist")
            .1
            .split_once("storeIrradiance(atlasCoordinate, current);")
            .expect("irradiance history block must precede the atlas store")
            .0;

        assert!(history_block.contains("if (localRecoveryProbe)"));
        assert!(history_block.contains("historyRetention = recoveryEpoch / (recoveryEpoch + 1.0);"));
        assert!(!history_block.contains("historyRetention = 0.0;"));
        assert!(!history_block.contains("relativeChange"));
        assert!(!history_block.contains("relativeDarkening"));
        assert!(!history_block.contains("DDGI_IRRADIANCE_CHANGE_THRESHOLD"));
        assert!(!history_block.contains("DDGI_IRRADIANCE_MIN_DARKENING_STEP"));
    }

    #[test]
    fn runtime_consumers_are_moment_only_while_transport_and_reference_keep_exact_visibility() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let types = include_str!("../shader/slang/tracer_types.slang");

        assert!(types.contains("public uint ddgi_terrain_hard_origin;"));
        assert!(query.contains("getDdgiMomentSpatialWeightProbeContributionAt("));
        assert!(query.contains("getDdgiMomentExactSpatialWeightProbeContributionAt("));
        let transport = query
            .split_once("DdgiQueryResult sampleDdgiTransportEnvironmentFromAtlas(")
            .expect("transport must retain an explicit moment-plus-exact query")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport implementation must remain behind its adapter")
            .0;
        assert!(transport.contains("getDdgiMomentExactProbeContributionFromAtlases("));
        assert!(transport.contains("contribution.hard_visibility"));
    }

    #[test]
    fn unoccluded_irradiance_debug_isolated_from_final_visibility_path() {
        let tracer = include_str!("../shader/slang/tracer.slang");
        let query = include_str!("../shader/slang/ddgi_query.slang");

        assert!(tracer.contains("DDGI_DEBUG_UNOCCLUDED_IRRADIANCE = 12u"));
        assert!(tracer.contains("DDGI_DEBUG_EQUAL_WEIGHT_IRRADIANCE = 13u"));
        assert!(tracer.contains("DDGI_DEBUG_RAW_CAGE_IRRADIANCE = 14u"));
        assert!(tracer.contains("sampleDdgiUnoccludedTerrainReference("));
        assert!(tracer.contains("sampleDdgiEqualWeightTerrainReference("));
        assert!(tracer.contains("sampleDdgiRawCageIrradiance("));
        assert!(query.contains("getDdgiUnoccludedProbeContribution("));
        assert!(query.contains("accumulateDdgiEqualWeightContribution("));
        assert!(query.contains("sampleDdgiRawCageIrradiance("));
        assert!(query.contains("accumulateDdgiContribution(result, contribution, 1.0);"));
        assert!(!query.contains("public DdgiProbeContribution getDdgiProbeContribution("));
        assert!(!tracer.contains("accumulateDdgiContribution("));
        assert!(query.contains("public void writeDdgiSpatialWeightDiagnostics("));
        assert!(tracer.contains("writeDdgiSpatialWeightDiagnostics("));
        assert!(tracer.contains("if (view == DDGI_DEBUG_EXACT_IRRADIANCE)"));
    }

    #[test]
    fn direct_terrain_shadow_uses_exact_surface_hit_and_keeps_ddgi_voxel_receiver() {
        let tracer = include_str!("../shader/slang/tracer.slang");
        let ray_origin = include_str!("../shader/slang/terrain_ray_origin.slang");

        assert!(ray_origin.contains("public float3 terrainRayOriginFromSurface("));
        assert!(ray_origin.contains(
            "return surfacePosition +\n        normalDirection * max(0.0, offsetWorld);"
        ));
        assert!(tracer.contains(
            "shadowRay.origin = terrainShadowReceiverPositionFromSurface(\n        surfacePosition, normal);"
        ));
        assert!(tracer
            .contains("directLight = directLighting(albedo, result.normal, result.position);"));
        assert!(tracer.contains(
            "terrainVoxelSurfacePositionAlongNormal(\n        result.center_position, result.normal)"
        ));
        assert!(tracer.contains(
            "sampleDdgiTerrainSmoothEnvironment(\n        shading_info, ddgiReceiverPosition, result.position,\n        result.normal)"
        ));
    }

    #[test]
    fn terrain_ray_origin_offset_is_shared_by_every_exact_terrain_ray_stage() {
        let shared = include_str!("../shader/slang/terrain_ray_origin.slang");
        let tracer = include_str!("../shader/slang/tracer.slang");
        let exact_sun = include_str!("../shader/slang/ddgi_exact_sun_visibility.slang");
        let probe_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        let moisture = include_str!("../shader/slang/terrain_moisture_dry.slang");

        assert!(shared.contains("public float3 terrainRayOriginAlongNormal("));
        assert!(shared.contains("public float3 terrainRayOriginFromSurface("));
        assert!(tracer.contains("import terrain_ray_origin;"));
        assert!(tracer.contains("gui_input.terrain_ray_origin_offset_world"));
        assert!(exact_sun.contains("originOffsetWorld"));
        assert!(exact_sun.contains("terrainRayOriginAlongNormal("));
        assert!(probe_trace.contains("import terrain_ray_origin;"));
        assert!(probe_trace.contains("ddgi_radiance_sun.terrain_ray_origin_offset_world"));
        assert!(probe_trace.contains("ddgiHardVisibilityPosition"));
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let transport = query
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport query adapter must exist")
            .1
            .split_once("public uint ddgiNearestNominalProbeIndex")
            .expect("transport query adapter must remain isolated")
            .0;
        assert!(transport.contains("float3 hardVisibilityWorldPosition"));
        assert!(transport.contains("hardVisibilityWorldPosition);"));
        assert!(!transport.contains("visibility_bias_world *"));
        assert!(moisture.contains("import terrain_ray_origin;"));
        assert!(moisture.contains("gui_input.terrain_ray_origin_offset_world"));
    }
}
