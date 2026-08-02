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
        assert!(terrain.contains("consumerResult = sampleDdgiDiffuseEnvironment("));
        assert!(terrain.contains("environmentIrradiance = consumerResult.irradiance"));
        assert!(terrain.contains("environmentCaptureIrradiance = captureResult.irradiance"));
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
            .split_once("fn update_ddgi_consumer_descriptors")
            .expect("DDGI consumer promotion seam must exist")
            .1
            .split_once("fn promote_ready_ddgi_staging")
            .expect("consumer update must remain adjacent to promotion")
            .0;
        assert!(consumer_update.contains("compute_pipelines.tracer_ppl"));
        assert!(consumer_update.contains("graphics_pipelines.flora_ppl"));
        assert!(consumer_update.contains("DDGI_PROBE_METADATA_BINDING"));
        assert!(consumer_update.contains("DDGI_IRRADIANCE_ATLAS_BINDING"));
        assert!(consumer_update.contains("DDGI_VISIBILITY_ATLAS_BINDING"));

        let promotion = tracer_host
            .split_once("fn promote_ready_ddgi_staging")
            .expect("DDGI promotion must exist")
            .1;
        let descriptor_rebind = promotion
            .find("update_ddgi_consumer_descriptors")
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
            .split_once("// DDGI is part of the flat voxel material contract")
            .expect("path-tracing branch must remain ahead of the DDGI query")
            .0;

        assert!(!shared.contains("path_tracing_reference"));
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

        assert_eq!(reference["kind"].as_str(), Some("bool"));
        assert_eq!(ambient["kind"].as_str(), Some("color"));
        assert_eq!(max_bounces["kind"].as_str(), Some("uint"));
        assert_eq!(ray_origin_offset["kind"].as_str(), Some("float"));
        assert_eq!(ray_origin_offset["data"]["value"].as_float(), Some(0.005));
        assert_eq!(ray_origin_offset["data"]["min"].as_integer(), Some(0));
        assert_eq!(ray_origin_offset["data"]["max"].as_float(), Some(0.02));
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
    fn ddgi_visibility_policy_keeps_bias_in_voxel_units_and_rejects_distant_hits() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let filter = include_str!("../shader/slang/ddgi_visibility_filter.slang");

        assert!(query.contains("query.visibility_bias_world * 0.125"));
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
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
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
    fn consumer_and_transport_adapters_share_one_eight_probe_query() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let shared = query
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("shared atlas-parametric DDGI query must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironment(")
            .expect("consumer adapter must follow the shared query")
            .0;
        let consumer = query
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironment(")
            .expect("consumer adapter must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport adapter must follow the consumer adapter")
            .0;
        let transport = query
            .split_once("public DdgiQueryResult sampleDdgiTransportSource(")
            .expect("transport adapter must exist")
            .1
            .split_once("public uint ddgiNearestNominalProbeIndex")
            .expect("debug helpers must follow both adapters")
            .0;

        assert_eq!(shared.matches("for (uint z = 0u; z < 2u; ++z)").count(), 1);
        assert!(consumer.contains("sampleDdgiDiffuseEnvironmentFromAtlas("));
        assert!(consumer.contains("ddgi_irradiance_atlas"));
        assert!(transport.contains("sampleDdgiDiffuseEnvironmentFromAtlas("));
        assert!(transport.contains("sourceIrradianceAtlas"));

        let trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        assert!(!trace.contains("ConstantBuffer<U_SunInfo>"));
        assert!(!trace.contains("ConstantBuffer<U_ShadingInfo>"));
        assert!(trace.contains("[[vk::binding(29, 0)]]"));
        assert!(trace.contains("[[vk::binding(30, 0)]]"));
        assert!(trace.contains("[[vk::binding(31, 0)]]"));
        assert!(trace.contains("[[vk::binding(32, 0)]]"));
    }

    #[test]
    fn shared_query_multiplies_exact_voxel_and_moment_visibility_without_camera_direction() {
        let query = include_str!("../shader/slang/ddgi_query.slang");
        let shared = query
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironmentFromAtlas(")
            .expect("shared DDGI sampler must exist")
            .1
            .split_once("public DdgiQueryResult sampleDdgiDiffuseEnvironment(")
            .expect("consumer adapter must follow shared sampler")
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
        assert!(!probe_trace.contains("ddgi_transport_query_info, result.position"));
        assert!(shared.contains("contribution.hard_visibility * contribution.moment_visibility"));

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
            "shading_info, ddgiReceiverPosition, result.normal,\n        ddgiHardVisibilityOrigin"
        ));
        let exact_reference = tracer
            .split_once("DdgiQueryResult sampleDdgiExactTerrainReference(")
            .expect("exact voxel reference must exist")
            .1
            .split_once("float3 ddgiTerrainDebugValue(")
            .expect("exact reference must remain isolated")
            .0;
        assert!(exact_reference.contains("contribution.hard_visibility"));
    }

    #[test]
    fn terrain_ray_origin_offset_is_shared_by_every_exact_terrain_ray_stage() {
        let shared = include_str!("../shader/slang/terrain_ray_origin.slang");
        let tracer = include_str!("../shader/slang/tracer.slang");
        let exact_sun = include_str!("../shader/slang/ddgi_exact_sun_visibility.slang");
        let probe_trace = include_str!("../shader/slang/ddgi_probe_trace.slang");
        let moisture = include_str!("../shader/slang/terrain_moisture_dry.slang");

        assert!(shared.contains("public float3 terrainRayOriginAlongNormal("));
        assert!(tracer.contains("import terrain_ray_origin;"));
        assert!(tracer.contains("gui_input.terrain_ray_origin_offset_world"));
        assert!(exact_sun.contains("originOffsetWorld"));
        assert!(exact_sun.contains("terrainRayOriginAlongNormal("));
        assert!(probe_trace.contains("import terrain_ray_origin;"));
        assert!(probe_trace.contains("ddgi_radiance_sun.terrain_ray_origin_offset_world"));
        assert!(moisture.contains("import terrain_ray_origin;"));
        assert!(moisture.contains("manual_gui_input.terrain_ray_origin_offset_world"));
    }
}
