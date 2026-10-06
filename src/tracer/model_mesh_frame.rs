//! Ordinary hardware-raster model draws. No model-local sampling, tiles,
//! per-view surface baking, relighting passes or sampled-depth reconstruction.
//! Geometry and published simulation poses retain acquired-frame-slot ownership.
use super::{
    butterfly_mesh::{ButterflyMeshRenderer, ButterflyMeshSettings, LeafModelSettings},
    dynamic_fruit_resources::DynamicFruitRendererResources,
    model_geometry::{self, Source},
    model_pixel_views,
    pipeline_builder::{ComputePipelines, GraphicsPipelines},
    resources::TracerResources,
    PushConstantFlora,
};
use crate::{flora::models, particles::ParticleSnapshot};
use anyhow::Result;
use glam::{Mat4, Vec3};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, DescriptorResource, Device,
    GraphicsPipeline, MemoryLocation, PreparedDrawDescriptors, PushConstantInfo, VulkanContext,
};
use std::sync::Arc;

pub(super) struct MeshPass<'a> {
    pub display: &'a GraphicsPipeline,
}

struct Geometry {
    shape: models::Shape,
    source: Source,
    triangles: Arc<Buffer>,
    ranges: Arc<Buffer>,
    parts: Arc<Buffer>,
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
        let indices: Vec<u32> =
            (0..source.ranges.iter().map(|r| r[1] * 3).max().unwrap()).collect();
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
            palette: upload(
                bytemuck::cast_slice(&source.palette),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?,
            indices: upload(
                bytemuck::cast_slice(&indices),
                vk::BufferUsageFlags::INDEX_BUFFER,
            )?,
            source,
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
    push: PushConstantInfo,
}
impl PreparedFlowerModels {
    pub fn record(&self, cmdbuf: &CommandBuffer, resources: &TracerResources) {
        cmdbuf.bind_index_buffer_u32(&resources.flower_models.flower_stem_indices);
        cmdbuf.bind_vertex_buffers(0, &[&resources.flower_models.flower_stem_vertices]);
        self.stem_pipeline.record_indexed_with_prepared_descriptors(
            cmdbuf,
            &self.stems,
            6,
            self.count,
            0,
            0,
            0,
            Some(&self.push),
        );
        self.heads.record(cmdbuf, 0);
    }
}

#[derive(Default)]
struct Frame {
    geometry: Option<Arc<Geometry>>,
    particles: Option<Arc<Buffer>>,
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
        }
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
            && std::env::var_os("RE_FLORA_MODEL_VIEW_REVIEW").is_some()
        {
            self.review_draws |= bit;
            log::info!("[MODEL_VIEW_DRAW] object={object} instances={count} shader=native_triangle bank_binding=19 count={:?}", self.view_count);
        }
    }
    pub fn begin_frame(&mut self, slot: usize, _: &ComputePipelines, _: &GraphicsPipelines) {
        self.frames
            .resize_with((slot + 1).max(self.frames.len()), Frame::default);
        self.slot = slot;
        // Called only after this frame's fence. Other slots retain their source
        // buffers if a live shape change creates a new geometry generation.
        self.frames[slot].geometry = None;
    }
    pub fn startup_cache_progress(&self) -> f32 {
        1.0
    }
    pub fn warmup_cache(
        &mut self,
        slot: usize,
        _: &CommandBuffer,
        _: u32,
        _: u32,
        _: [u32; 2],
        flowers: models::Settings,
    ) -> Result<bool> {
        self.slot = slot;
        self.ensure_geometry(flowers.shape)?;
        Ok(true)
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
            self.active = Some(Arc::new(Geometry::new(
                &self.device,
                &self.allocator,
                shape,
            )?));
            log::info!("[MODEL_MESHES] geometry=native raster=triangles per_model_pixels=false");
        }
        self.frames
            .resize_with((self.slot + 1).max(self.frames.len()), Frame::default);
        self.frames[self.slot].geometry = self.active.clone();
        Ok(())
    }
    pub fn prepare_cache(
        &mut self,
        _: &CommandBuffer,
        _: u32,
        _: u32,
        flowers: models::Settings,
    ) -> Result<()> {
        self.ensure_geometry(flowers.shape)
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
        leaves: LeafModelSettings,
        camera: Vec3,
    ) -> Result<()> {
        self.particles
            .prepare_frame_models(snapshots, settings, leaves, camera)
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
    ) -> Result<PreparedModelMeshes> {
        if std::env::var_os("RE_FLORA_MODEL_VIEW_REVIEW").is_some() {
            let (butterflies, leaves) = self.particles.model_counts();
            self.review_draw(1, "mesh_butterflies", butterflies);
            self.review_draw(2, "mesh_leaves", leaves);
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
        let vertices = g.source.ranges[..model_geometry::FLOWER_SOURCE_BASE as usize]
            .iter()
            .map(|r| r[1] * 3)
            .max()
            .unwrap();
        self.prepare(cmd, pass, self.particles.count(), vertices, resources, None)
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
            g.source.ranges[64][1] * 3,
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
        pass: MeshPass<'_>,
        stem_pipeline: &GraphicsPipeline,
        count: u32,
        push: crate::generated::gpu_structs::PushConstantFlowerPixel,
        pose: &[(&str, DescriptorResource<'_>)],
    ) -> Result<PreparedFlowerModels> {
        self.review_draw(8, "flower_heads", count);
        let g = self.active.as_ref().unwrap();
        let mut resources = g.bindings().to_vec();
        resources.extend_from_slice(pose);
        resources.push(("flower_parts", DescriptorResource::Buffer(&g.parts)));
        resources.push((
            "model_cache_palette",
            DescriptorResource::Buffer(&g.palette),
        ));
        let mut stem_resources = pose.to_vec();
        stem_resources.push(("flower_parts", DescriptorResource::Buffer(&g.parts)));
        let parts = &g.source.flower_parts
            [(push.species - crate::flora::MODEL_FLOWER_FIRST_SPECIES) as usize * 4..][..4];
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
            stems: stem_pipeline.prepare_draw_descriptors(cmd, &stem_resources)?,
            stem_pipeline: stem_pipeline.clone(),
            count,
            push: PushConstantInfo {
                shader_stage: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                push_constants: constant.push_constants.clone(),
            },
            heads: self.prepare(
                cmd,
                pass,
                count * heads,
                vertices,
                resources,
                Some(constant),
            )?,
        })
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
            g.source.ranges[64][1] * 3,
            resources,
            None,
        )
    }
}
