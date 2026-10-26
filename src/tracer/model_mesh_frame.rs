//! Native triangles or immutable pre-baked pixel cells, selected per object.
//! Geometry, cached surfaces and published poses retain acquired-frame-slot ownership.
use super::{
    butterfly_mesh::{ButterflyMeshRenderer, ButterflyMeshSettings},
    dynamic_fruit_resources::DynamicFruitRendererResources,
    model_geometry::{self, Source},
    model_pixel_views,
    model_surface_cache::{
        Buffers as CacheBuffers, Options as CacheOptions, SurfaceCache, MAX_RESOLUTION, MODEL_KINDS,
    },
    pipeline_builder::{ComputePipelines, GraphicsPipelines},
    resources::TracerResources,
    PushConstantFlora,
};
use crate::{flora::models, particles::ParticleSnapshot};
use anyhow::Result;
use glam::{Mat4, Vec3};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline,
    DescriptorResource, Device, Extent3D, GraphicsPipeline, MemoryLocation,
    PreparedDrawDescriptors, PushConstantInfo, VulkanContext,
};
use std::sync::Arc;

// ceil(colors * ceil(curve_segments / colors)) is at most colors + 11
// for the 12-segment main path; branches need only four subdivisions.
fn flower_stem_capacity(maximum_world_height: f32, cell_height_voxels: f32) -> u32 {
    ((maximum_world_height.max(0.) * 256. / cell_height_voxels.clamp(0.1, 4.)).ceil() as u32).max(1)
        + 11
}

#[cfg(test)]
mod stem_tests {
    use super::flower_stem_capacity;

    #[test]
    fn full_growth_allocation_covers_material_and_curve_subdivisions() {
        for max_height_voxels in [1., 4., 16., 64., 128.] {
            for cell in [0.1, 0.5, 1., 4.] {
                let capacity = flower_stem_capacity(max_height_voxels / 256., cell);
                for growth in [0., 0.01, 0.5, 1.] {
                    for (fraction, minimum_segments) in [(1., 12u32), (0.34, 4)] {
                        let colors =
                            ((max_height_voxels * fraction * growth / cell).ceil() as u32).max(1);
                        let subdivisions = minimum_segments.div_ceil(colors);
                        assert!(colors * subdivisions <= capacity);
                        assert!(colors * subdivisions >= minimum_segments);
                    }
                }
            }
        }
    }
}

pub(super) struct MeshPass<'a> {
    pub display: &'a GraphicsPipeline,
}

/// Paired flower-head and stem pipelines, including GPU stem preparation.
pub(super) struct FlowerPass<'a> {
    pub display: &'a GraphicsPipeline,
    pub stem: &'a GraphicsPipeline,
    pub preparation: &'a ComputePipeline,
}

struct Geometry {
    shape: models::Shape,
    source: Arc<Source>,
    empty_cache: Arc<CacheBuffers>,
    triangles: Arc<Buffer>,
    ranges: Arc<Buffer>,
    parts: Arc<Buffer>,
    sprinkler_parts: Arc<Buffer>,
    palette: Arc<Buffer>,
    indices: Arc<Buffer>,
}
impl Geometry {
    fn new(device: &Device, allocator: &Allocator, shape: models::Shape) -> Result<Self> {
        let source = model_geometry::source(shape);
        let upload = |bytes: &[u8], usage| -> Result<Arc<Buffer>> {
            let buffer = Arc::new(Buffer::try_new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(usage),
                MemoryLocation::CpuToGpu,
                bytes.len().max(16) as u64,
            )?);
            buffer.fill_range_with_raw_u8(0, bytes)?;
            Ok(buffer)
        };
        let indices: Vec<u32> = (0..source
            .ranges
            .iter()
            .map(|r| r[1] * 3)
            .max()
            .unwrap()
            .max(MAX_RESOLUTION * MAX_RESOLUTION * 6))
            .collect();
        let empty_cache = CacheBuffers::empty(device, allocator, source.ranges.len())?;
        Ok(Self {
            shape,
            triangles: upload(
                bytemuck::cast_slice(&source.triangles),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?,
            ranges: upload(
                bytemuck::cast_slice(&source.ranges),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?,
            parts: upload(
                bytemuck::cast_slice(&source.flower_parts),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?,
            sprinkler_parts: upload(
                bytemuck::cast_slice(&source.sprinkler_parts),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?,
            palette: upload(
                bytemuck::cast_slice(&source.palette),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?,
            indices: upload(
                bytemuck::cast_slice(&indices),
                vk::BufferUsageFlags::INDEX_BUFFER,
            )?,
            source: Arc::new(source),
            empty_cache,
        })
    }
    fn bindings(&self) -> [(&'static str, DescriptorResource<'_>); 2] {
        [
            (
                "model_mesh_triangles",
                DescriptorResource::Buffer(&self.triangles),
            ),
            (
                "model_mesh_ranges",
                DescriptorResource::Buffer(&self.ranges),
            ),
        ]
    }
}

pub(super) struct PreparedModelMeshes {
    pipeline: GraphicsPipeline,
    descriptors: PreparedDrawDescriptors,
    count: u32,
    vertices: u32,
    push: Option<PushConstantInfo>,
    geometry: Arc<Geometry>,
}
impl PreparedModelMeshes {
    pub fn record(&self, cmdbuf: &CommandBuffer, _: u32) {
        cmdbuf.bind_index_buffer_u32(&self.geometry.indices);
        self.pipeline.record_indexed_with_prepared_descriptors(
            cmdbuf,
            &self.descriptors,
            self.vertices,
            self.count,
            0,
            0,
            0,
            self.push.as_ref(),
        );
    }
}

pub(super) struct PreparedFlowerModels {
    heads: PreparedModelMeshes,
    stems: PreparedDrawDescriptors,
    stem_pipeline: GraphicsPipeline,
    count: u32,
    bands: Arc<Buffer>,
}
impl PreparedFlowerModels {
    pub fn record(&self, cmdbuf: &CommandBuffer, resources: &TracerResources) {
        cmdbuf.bind_index_buffer_u32(&resources.flower_models.stem_band_indices);
        cmdbuf.bind_vertex_buffers(0, &[&resources.flower_models.stem_band_vertices]);
        let _keep_bands_alive = &self.bands;
        self.stem_pipeline.record_indexed_with_prepared_descriptors(
            cmdbuf,
            &self.stems,
            36,
            self.count,
            0,
            0,
            0,
            None,
        );
        self.heads.record(cmdbuf, 0);
    }
}

#[derive(Default)]
struct Frame {
    geometry: Option<Arc<Geometry>>,
    particles: Option<Arc<Buffer>>,
    cache: [Option<Arc<CacheBuffers>>; MODEL_KINDS],
    tool_pose: Option<Arc<Buffer>>,
    soil_bag: Option<crate::soil_bag::Instance>,
    stem_bands: Vec<(u64, Arc<Buffer>)>,
    stem_draws: usize,
}
pub(super) struct ModelMeshFrame {
    device: Device,
    allocator: Allocator,
    active: Option<Arc<Geometry>>,
    frames: Vec<Frame>,
    slot: usize,
    particles: ButterflyMeshRenderer,
    view_azimuths: Option<Arc<Buffer>>,
    view_count: Option<u32>,
    review_draws: u8,
    cache: [SurfaceCache; MODEL_KINDS],
    cache_options: CacheOptions,
    geometry_generation: u64,
    logged_cache_modes: [Option<bool>; MODEL_KINDS],
    soil_bag: Option<crate::soil_bag::Instance>,
    soil_bag_shadow: crate::soil_bag::ShadowState,
    reviewed_bag_shadows: [bool; 2],
}
impl ModelMeshFrame {
    pub fn new(context: &VulkanContext, allocator: Allocator) -> Self {
        Self {
            device: context.device().clone(),
            allocator,
            active: None,
            frames: Vec::new(),
            slot: 0,
            particles: ButterflyMeshRenderer::default(),
            view_azimuths: None,
            view_count: None,
            review_draws: 0,
            cache: std::array::from_fn(|_| SurfaceCache::default()),
            cache_options: CacheOptions::default(),
            geometry_generation: 0,
            logged_cache_modes: [None; MODEL_KINDS],
            soil_bag: None,
            soil_bag_shadow: Default::default(),
            reviewed_bag_shadows: [false; 2],
        }
    }
    pub fn set_cache_options(&mut self, options: CacheOptions) {
        self.cache_options = options.normalized();
    }
    fn prepare_surfaces(&mut self) -> Result<()> {
        let geometry = self.active.as_ref().unwrap();
        for kind in 0..MODEL_KINDS {
            let cache = if self.cache_options.enabled[kind] {
                self.cache[kind].prepare(
                    &self.device,
                    &self.allocator,
                    if kind == 2 {
                        self.geometry_generation
                    } else {
                        0
                    },
                    geometry.source.clone(),
                    self.cache_options.for_kind(kind),
                    &geometry.empty_cache,
                )?
            } else {
                geometry.empty_cache.clone()
            };
            let cached = cache.cached(kind);
            if self.logged_cache_modes[kind] != Some(cached) {
                log::info!(
                    "[MODEL_SURFACE_MODE] object={} mode={} views={} resolution={}",
                    [
                        "apples",
                        "butterflies",
                        "flower_heads",
                        "soil_bag",
                        "sprinklers"
                    ][kind],
                    if cached { "cached" } else { "native" },
                    self.cache_options.views,
                    self.cache_options.resolutions[kind]
                );
                self.logged_cache_modes[kind] = Some(cached);
                self.review_draws &= !(match kind {
                    0 => 4 | 16,
                    1 => 1,
                    2 => 8,
                    3 => 32 | 64,
                    _ => 128,
                });
            }
            self.frames[self.slot].cache[kind] = Some(cache);
        }
        Ok(())
    }
    fn draw_vertices(&self, kind: usize, native: u32) -> u32 {
        self.frames[self.slot].cache[kind]
            .as_ref()
            .unwrap()
            .vertices(kind, native)
    }
    pub fn set_view_count(&mut self, requested: u32) {
        let count = model_pixel_views::runtime_count(requested);
        if self.view_count != Some(count) {
            self.view_count = Some(count);
            self.review_draws = 0;
            log::info!("[MODEL_VIEW_QUANTIZATION] count={count} bank=actual_n_fibonacci selection=nearest_dot roll=continuous pivot=per_object depth=hardware simulation=unchanged");
        }
    }
    fn review_draw(&mut self, bit: u8, object: &str, count: u32) {
        if count > 0
            && self.review_draws & bit == 0
            && (std::env::var_os("RE_FLORA_MODEL_VIEW_REVIEW").is_some()
                || ((bit == 32 || bit == 64)
                    && std::env::var_os("RE_FLORA_SOIL_BAG_REVIEW").is_some())
                || (bit == 128 && std::env::var_os("RE_FLORA_SPRINKLER_REVIEW").is_some()))
        {
            self.review_draws |= bit;
            let kind = if bit == 128 {
                4
            } else if bit == 32 || bit == 64 {
                3
            } else if bit == 1 {
                1
            } else if bit == 8 {
                2
            } else {
                0
            };
            let cached = self.frames[self.slot].cache[kind]
                .as_ref()
                .unwrap()
                .cached(kind);
            log::info!("[MODEL_VIEW_DRAW] object={object} instances={count} shader={} bank_binding=19 count={:?}", if cached { "cached_surface_cells" } else { "native_triangle" }, self.view_count);
        }
    }
    pub fn begin_frame(
        &mut self,
        slot: usize,
        compute: &ComputePipelines,
        graphics: &GraphicsPipelines,
    ) {
        self.frames
            .resize_with((slot + 1).max(self.frames.len()), Frame::default);
        self.slot = slot;
        // Called only after this frame's fence. Other slots retain their source
        // buffers if a live shape change creates a new geometry generation.
        self.frames[slot].geometry = None;
        self.frames[slot].cache = Default::default();
        self.frames[slot].soil_bag = None;
        self.frames[slot].stem_draws = 0;
        compute
            .flower_stem_bands_ppl
            .begin_transient_descriptor_frame(slot);
        graphics
            .flower_stem_experiment_ppl
            .begin_transient_descriptor_frame(slot);
    }
    pub fn startup_cache_progress(&self) -> f32 {
        if (0..MODEL_KINDS).all(|kind| self.cache[kind].ready(self.cache_options.for_kind(kind))) {
            1.0
        } else {
            0.0
        }
    }
    pub fn warmup_cache(
        &mut self,
        slot: usize,
        _: &CommandBuffer,
        options: CacheOptions,
        flowers: models::Settings,
    ) -> Result<bool> {
        self.slot = slot;
        self.set_cache_options(options);
        self.set_view_count(options.views);
        self.ensure_geometry(flowers.shape)?;
        self.prepare_surfaces()?;
        Ok(self.startup_cache_progress() == 1.0)
    }
    fn ensure_geometry(&mut self, shape: models::Shape) -> Result<()> {
        if self.view_azimuths.is_none() {
            let bank = model_pixel_views::azimuths();
            let buffer = Arc::new(Buffer::try_new_sized(
                self.device.clone(),
                self.allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                std::mem::size_of_val(bank.as_slice()) as u64,
            )?);
            buffer.fill_range_with_raw_u8(0, bytemuck::cast_slice(&bank))?;
            self.view_azimuths = Some(buffer);
        }
        if self.active.as_ref().is_none_or(|g| g.shape != shape) {
            self.geometry_generation = self
                .geometry_generation
                .checked_add(1)
                .expect("model geometry generation overflow");
            self.active = Some(Arc::new(Geometry::new(
                &self.device,
                &self.allocator,
                shape,
            )?));
            log::info!("[MODEL_MESHES] geometry=immutable_triangles cached_surfaces=independent_generations poses=published");
        }
        self.frames
            .resize_with((self.slot + 1).max(self.frames.len()), Frame::default);
        self.frames[self.slot].geometry = self.active.clone();
        Ok(())
    }
    pub fn prepare_cache(&mut self, _: &CommandBuffer, flowers: models::Settings) -> Result<()> {
        self.ensure_geometry(flowers.shape)?;
        self.prepare_surfaces()
    }
    pub fn flower_culling_padding(&self, scale: f32, overshoot: f32) -> (Vec3, Vec3) {
        self.active
            .as_ref()
            .unwrap()
            .source
            .flower_culling_padding(scale, overshoot)
    }
    pub fn prepare_particle_models(
        &mut self,
        snapshots: &[ParticleSnapshot],
        settings: ButterflyMeshSettings,
        camera: Vec3,
    ) -> Result<()> {
        self.particles
            .prepare_frame_models(snapshots, settings, camera)
    }
    pub(super) fn flower_hosts(
        &self,
    ) -> std::collections::HashSet<crate::particles::flower_visits::FlowerId> {
        self.particles.flower_hosts()
    }
    pub(super) fn flower_parts_buffer(&self) -> Arc<Buffer> {
        self.active
            .as_ref()
            .expect("prepared flower geometry")
            .parts
            .clone()
    }
    pub(super) fn bind_flower_hosts(
        &mut self,
        indices: &std::collections::HashMap<crate::particles::flower_visits::FlowerId, u32>,
    ) {
        self.particles.bind_flower_hosts(indices);
    }
    pub fn particle_count(&self) -> u32 {
        self.particles.count()
    }
    pub fn particles(
        &mut self,
        cmd: &CommandBuffer,
        pass: MeshPass<'_>,
        _: Mat4,
        _: Mat4,
        contacts: &Buffer,
    ) -> Result<PreparedModelMeshes> {
        if std::env::var_os("RE_FLORA_MODEL_VIEW_REVIEW").is_some() {
            self.review_draw(1, "mesh_butterflies", self.particles.count());
        }
        let frame = &mut self.frames[self.slot];
        self.particles.publish_mesh_frame(|bytes| {
            let buffer = Arc::new(Buffer::try_new_sized(
                self.device.clone(),
                self.allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                bytes.len().max(16) as u64,
            )?);
            buffer.fill_range_with_raw_u8(0, bytes)?;
            frame.particles = Some(buffer);
            Ok(())
        })?;
        let particle_buffer = frame.particles.as_ref().unwrap().clone();
        let g = self.active.as_ref().unwrap().clone();
        let mut resources = g.bindings().to_vec();
        resources.push((
            "butterfly_mesh_instances",
            DescriptorResource::Buffer(&particle_buffer),
        ));
        resources.push(("flower_visit_output", DescriptorResource::Buffer(contacts)));
        let vertices = g.source.ranges[..model_geometry::FLOWER_SOURCE_BASE as usize]
            .iter()
            .map(|r| r[1] * 3)
            .max()
            .unwrap();
        self.prepare(
            cmd,
            pass,
            self.particles.count(),
            self.draw_vertices(1, vertices),
            resources,
            None,
        )
    }
    /// Shared native-model/stone seam. Call after loading-time warmup (or
    /// ensure_geometry). The immutable bank is never replaced on count/resize
    /// changes and remains owned by this frame module until renderer shutdown.
    pub(super) fn view_bank_binding(&self) -> (&'static str, DescriptorResource<'_>) {
        (
            "model_view_azimuths",
            DescriptorResource::Buffer(
                self.view_azimuths
                    .as_ref()
                    .expect("model view bank initialized by warmup"),
            ),
        )
    }

    fn prepare<'a>(
        &'a self,
        cmd: &CommandBuffer,
        pass: MeshPass<'_>,
        count: u32,
        vertices: u32,
        mut resources: Vec<(&str, DescriptorResource<'a>)>,
        push: Option<PushConstantInfo>,
    ) -> Result<PreparedModelMeshes> {
        for (kind, (samples, entries)) in [
            ("model_cached_apple_surfaces", "model_cached_apple_entries"),
            (
                "model_cached_butterfly_surfaces",
                "model_cached_butterfly_entries",
            ),
            (
                "model_cached_flower_surfaces",
                "model_cached_flower_entries",
            ),
            ("model_cached_tool_surfaces", "model_cached_tool_entries"),
            (
                "model_cached_sprinkler_surfaces",
                "model_cached_sprinkler_entries",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let cache = self.frames[self.slot].cache[kind].as_ref().unwrap();
            resources.push((samples, DescriptorResource::Buffer(&cache.samples)));
            resources.push((entries, DescriptorResource::Buffer(&cache.entries)));
        }
        self.prepare_geometry(cmd, pass, count, vertices, resources, push)
    }
    fn prepare_geometry<'a>(
        &'a self,
        cmd: &CommandBuffer,
        pass: MeshPass<'_>,
        count: u32,
        vertices: u32,
        mut resources: Vec<(&str, DescriptorResource<'a>)>,
        push: Option<PushConstantInfo>,
    ) -> Result<PreparedModelMeshes> {
        resources.push(self.view_bank_binding());
        cmd.use_buffer(&self.active.as_ref().unwrap().indices, BufferUse::IndexRead);
        Ok(PreparedModelMeshes {
            pipeline: pass.display.clone(),
            descriptors: pass.display.prepare_draw_descriptors(cmd, &resources)?,
            count,
            vertices,
            push,
            geometry: self.active.as_ref().unwrap().clone(),
        })
    }
    pub fn attached_apples(
        &mut self,
        cmd: &CommandBuffer,
        pass: MeshPass<'_>,
        count: u32,
        pose: &[(&str, DescriptorResource<'_>)],
        push: PushConstantFlora,
    ) -> Result<PreparedModelMeshes> {
        self.review_draw(4, "attached_apples", count);
        let g = self.active.as_ref().unwrap();
        let mut resources = g.bindings().to_vec();
        resources.extend_from_slice(pose);
        self.prepare(
            cmd,
            pass,
            count,
            self.draw_vertices(
                0,
                g.source.ranges[model_geometry::APPLE_SOURCE as usize][1] * 3,
            ),
            resources,
            Some(PushConstantInfo {
                shader_stage: vk::ShaderStageFlags::VERTEX,
                push_constants: bytemuck::bytes_of(&push).to_vec(),
            }),
        )
    }
    pub fn flowers(
        &mut self,
        cmd: &CommandBuffer,
        pass: FlowerPass<'_>,
        settings: models::StemExperiment,
        count: u32,
        mut push: crate::generated::gpu_structs::PushConstantFlowerPixel,
        pose: &[(&str, DescriptorResource<'_>)],
    ) -> Result<PreparedFlowerModels> {
        let FlowerPass {
            display,
            stem: stem_pipeline,
            preparation,
        } = pass;
        let pass = MeshPass { display };
        self.review_draw(8, "flower_heads", count);
        let g = self.active.as_ref().unwrap();
        let mut resources = g.bindings().to_vec();
        resources.extend_from_slice(pose);
        resources.push(("flower_parts", DescriptorResource::Buffer(&g.parts)));
        resources.push((
            "model_cache_palette",
            DescriptorResource::Buffer(&g.palette),
        ));
        let parts = &g.source.flower_parts
            [(push.species - crate::flora::MODEL_FLOWER_FIRST_SPECIES) as usize * 4..][..4];
        // A rest-length material band can contain several curve subdivisions.
        // Allocate for full growth; wind and spawn translation do not add bands.
        let maximum_height = parts[0].maximum_stem_length() * push.world_scale;
        let capacity = flower_stem_capacity(maximum_height, settings.cell_height_voxels);
        let branches = if settings.branches { 3 } else { 1 };
        let band_count = count
            .checked_mul(capacity)
            .and_then(|n| n.checked_mul(branches))
            .ok_or_else(|| anyhow::anyhow!("flower stem band count overflow"))?;
        let bytes = u64::from(band_count) * 112; // seven float4s in StemMeshBand
        let frame = &mut self.frames[self.slot];
        let index = frame.stem_draws;
        if index == frame.stem_bands.len() || frame.stem_bands[index].0 < bytes {
            let buffer = Arc::new(Buffer::try_new_sized(
                self.device.clone(),
                self.allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::GpuOnly,
                bytes.max(16),
            )?);
            log::info!("[FLOWER_STEM_MESH] plants={count} branches={branches} capacity={capacity} bytes={bytes} pose=gpu_prepared colors=flat depth=raster");
            if index == frame.stem_bands.len() {
                frame.stem_bands.push((bytes, buffer));
            } else {
                frame.stem_bands[index] = (bytes, buffer);
            }
        }
        frame.stem_draws += 1;
        let bands = frame.stem_bands[index].1.clone();
        let mut stem_resources = pose.to_vec();
        stem_resources.push(("flower_parts", DescriptorResource::Buffer(&g.parts)));
        stem_resources.push(("flower_stem_bands", DescriptorResource::Buffer(&bands)));
        // These assembly fields are unused by flowerPlantPose: compute dispatch
        // uses prepare_object for plant count and padding for per-branch stride.
        push.prepare_object = count;
        push.padding = capacity;
        preparation.record_with_descriptors(
            cmd,
            &stem_resources,
            Extent3D::new(count, 1, 1),
            Some(bytemuck::bytes_of(&push)),
        )?;
        let heads = parts[0].range[3];
        let vertices = parts[1..=heads as usize]
            .iter()
            .map(|p| p.range[1] * 3)
            .max()
            .unwrap();
        let constant = PushConstantInfo {
            shader_stage: vk::ShaderStageFlags::VERTEX,
            push_constants: bytemuck::bytes_of(&push).to_vec(),
        };
        Ok(PreparedFlowerModels {
            stems: stem_pipeline.prepare_draw_descriptors(
                cmd,
                &[("flower_stem_bands", DescriptorResource::Buffer(&bands))],
            )?,
            stem_pipeline: stem_pipeline.clone(),
            count: band_count,
            bands,
            heads: self.prepare(
                cmd,
                pass,
                count * heads,
                self.draw_vertices(2, vertices),
                resources,
                Some(constant),
            )?,
        })
    }
    pub fn set_soil_bag(&mut self, instance: Option<crate::soil_bag::Instance>) {
        self.soil_bag = instance;
    }
    pub fn has_soil_bag(&self) -> bool {
        self.soil_bag.is_some()
    }
    pub fn take_soil_bag_shadow_changed(&mut self, camera: Vec3) -> bool {
        self.soil_bag_shadow
            .observe(self.soil_bag, camera, self.view_count.unwrap())
    }
    /// Publish once after the acquired-slot fence, before *both* world passes.
    /// Do not rewrite the instance after preparing its shadow descriptors.
    pub fn publish_soil_bag(&mut self, cmd: &CommandBuffer) -> Result<()> {
        let frame = &mut self.frames[self.slot];
        frame.soil_bag = self.soil_bag;
        let Some(pose) = frame.soil_bag else {
            return Ok(());
        };
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct WorldModelInstance {
            position: [f32; 4],
            axes: [[f32; 4]; 3],
            metadata: [u32; 4],
            mouth: [f32; 4],
        }
        let g = self.active.as_ref().unwrap();
        let state = usize::from(pose.pouring);
        let source = g.source.soil_bag_sources[state];
        let asset = crate::soil_bag::asset();
        let data = WorldModelInstance {
            position: pose.position.extend(pose.scale).to_array(),
            axes: [Vec3::X, Vec3::Y, Vec3::Z].map(|v| (pose.rotation * v).extend(0.).to_array()),
            metadata: [
                source,
                g.source.soil_bag_palette,
                asset.texture_size[0],
                asset.texture_size[1],
            ],
            mouth: Vec3::from(asset.states[state].mouth).extend(0.).to_array(),
        };
        if frame.tool_pose.is_none() {
            frame.tool_pose = Some(Arc::new(Buffer::try_new_sized(
                self.device.clone(),
                self.allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                std::mem::size_of::<WorldModelInstance>() as u64,
            )?));
        }
        let buffer = frame.tool_pose.as_ref().unwrap();
        buffer.fill_range_with_raw_u8(0, bytemuck::bytes_of(&data))?;
        cmd.use_buffer(buffer, BufferUse::HostWrite);
        Ok(())
    }
    pub fn soil_bag(
        &mut self,
        cmd: &CommandBuffer,
        pass: MeshPass<'_>,
        shadow: bool,
    ) -> Result<Option<PreparedModelMeshes>> {
        let Some(pose) = self.frames[self.slot].soil_bag else {
            return Ok(None);
        };
        let state = usize::from(pose.pouring);
        if !shadow {
            self.review_draw(
                if pose.pouring { 64 } else { 32 },
                if pose.pouring {
                    "soil_bag_pour"
                } else {
                    "soil_bag_hover"
                },
                1,
            );
        } else if !self.reviewed_bag_shadows[state]
            && std::env::var_os("RE_FLORA_SOIL_BAG_REVIEW").is_some()
        {
            self.reviewed_bag_shadows[state] = true;
            log::info!("[WORLD_MODEL_DRAW] object=soil_bag pose={} instances=1 pass=sun_shadow geometry=published_triangles shared_frame_instance=true", if pose.pouring {"pour"} else {"hover"});
        }
        let frame = &self.frames[self.slot];
        let g = self.active.as_ref().unwrap();
        let source = g.source.soil_bag_sources[state];
        let mut resources = g.bindings().to_vec();
        resources.push((
            "world_model_instances",
            DescriptorResource::Buffer(frame.tool_pose.as_ref().unwrap()),
        ));
        if !shadow {
            resources.push(("tool_model_colors", DescriptorResource::Buffer(&g.palette)));
        }
        let native_vertices = g.source.ranges[source as usize][1] * 3;
        Ok(Some(if shadow {
            self.prepare_geometry(cmd, pass, 1, native_vertices, resources, None)?
        } else {
            self.prepare(
                cmd,
                pass,
                1,
                self.draw_vertices(3, native_vertices),
                resources,
                None,
            )?
        }))
    }

    pub fn sprinkler_cache_status(&self) -> Option<(bool, u32)> {
        let cache = self.frames.get(self.slot)?.cache[4].as_ref()?;
        Some((cache.cached(4), self.cache_options.resolutions[4]))
    }

    pub fn sprinklers(
        &mut self,
        cmd: &CommandBuffer,
        pass: MeshPass<'_>,
        instances: &super::SprinklerRendererResources,
    ) -> Result<PreparedModelMeshes> {
        self.review_draw(128, "sprinklers", instances.instance_count);
        let g = self.active.as_ref().unwrap();
        let mut resources = g.bindings().to_vec();
        resources.push((
            "sprinkler_instances",
            DescriptorResource::Buffer(instances.instances()),
        ));
        resources.push((
            "sprinkler_parts",
            DescriptorResource::Buffer(&g.sprinkler_parts),
        ));
        resources.push((
            "sprinkler_model_colors",
            DescriptorResource::Buffer(&g.palette),
        ));
        let native = g
            .source
            .sprinkler_parts
            .iter()
            .map(|part| part.range[1] * 3)
            .max()
            .unwrap();
        self.prepare(
            cmd,
            pass,
            instances.instance_count,
            self.draw_vertices(4, native),
            resources,
            None,
        )
    }

    pub fn fallen_apples(
        &mut self,
        cmd: &CommandBuffer,
        pass: MeshPass<'_>,
        fruit: &DynamicFruitRendererResources,
        _: u32,
    ) -> Result<PreparedModelMeshes> {
        self.review_draw(16, "dynamic_apples", fruit.instance_count);
        let g = self.active.as_ref().unwrap();
        let mut resources = g.bindings().to_vec();
        resources.push((
            "dynamic_fruit_pixel_instances",
            DescriptorResource::Buffer(&fruit.instances),
        ));
        self.prepare(
            cmd,
            pass,
            fruit.instance_count,
            self.draw_vertices(
                0,
                g.source.ranges[model_geometry::APPLE_SOURCE as usize][1] * 3,
            ),
            resources,
            None,
        )
    }
}
