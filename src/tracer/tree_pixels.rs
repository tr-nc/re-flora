//! Display-only adapter for the resident posed wood scene. The current camera
//! drives every sample; there is no shape/view bank and no second tree state.
//! Tiles share the common fence-slot allocator and coverage rules with models,
//! but use the live tree BVH and per-vertex irradiance instead of rigid caching.
use super::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeDisplaySettings {
    pub pixelized: bool,
    pub pixel_size: u32,
}
impl Default for TreeDisplaySettings {
    fn default() -> Self {
        Self {
            pixelized: false,
            pixel_size: 4,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PixelGrid {
    extent: [u32; 2],
    size: u32,
    width: u32,
    height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelBand {
    first: u32,
    rows: u32,
}
impl PixelGrid {
    fn new(extent: [u32; 2], size: u32) -> Result<Self> {
        anyhow::ensure!(
            extent.iter().all(|v| *v > 0),
            "tree pixels require a nonzero scene extent"
        );
        let size = size.clamp(1, 16);
        let width = extent[0].div_ceil(size);
        anyhow::ensure!(
            width as usize <= model_pixel_tiles::BATCH_TEXELS,
            "tree pixel row exceeds portable buffer range"
        );
        Ok(Self {
            extent,
            size,
            width,
            height: extent[1].div_ceil(size),
        })
    }
    fn bands(self) -> Vec<PixelBand> {
        let max_rows = (model_pixel_tiles::BATCH_TEXELS / self.width as usize) as u32;
        (0..self.height)
            .step_by(max_rows as usize)
            .map(|first| PixelBand {
                first,
                rows: max_rows.min(self.height - first),
            })
            .collect()
    }
    fn push(self, band: PixelBand) -> [u32; 8] {
        [
            self.extent[0],
            self.extent[1],
            self.size,
            0,
            band.first,
            band.rows,
            0,
            0,
        ]
    }
}

#[derive(Default)]
pub(super) struct TreePixels {
    pub settings: TreeDisplaySettings,
    pub frames: u64,
    // Keep only the latest publication for explicit smoke readback. Normal
    // rendering does not synchronize or read any tree pixel data on the CPU.
    last: Vec<(Arc<Buffer>, PixelGrid, PixelBand)>,
}
pub(super) struct PreparedTreePixels {
    grid: PixelGrid,
    bands: Vec<(PixelBand, PreparedDrawDescriptors)>,
}
impl PreparedTreePixels {
    pub fn draw(&self, pipeline: &GraphicsPipeline, cmd: &CommandBuffer, viewport: Viewport) {
        for (band, descriptors) in &self.bands {
            let y = band.first * self.grid.size;
            let end = ((band.first + band.rows) * self.grid.size).min(self.grid.extent[1]);
            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: y as i32 },
                extent: vk::Extent2D {
                    width: self.grid.extent[0],
                    height: end - y,
                },
            };
            pipeline.record_viewport_scissor(cmd, viewport, scissor);
            pipeline.record_indexed_with_prepared_descriptors(
                cmd,
                descriptors,
                6,
                1,
                0,
                0,
                0,
                Some(&PushConstantInfo {
                    shader_stage: vk::ShaderStageFlags::FRAGMENT,
                    push_constants: bytemuck::bytes_of(&self.grid.push(*band)).to_vec(),
                }),
            );
        }
    }
}
impl Tracer {
    pub(super) fn prepare_tree_pixels(
        &mut self,
        cmd: &CommandBuffer,
        frame: usize,
    ) -> Result<Option<PreparedTreePixels>> {
        self.tree_pixels.last.clear();
        if !self.tree_pixels.settings.pixelized
            || !self.raster_trees.enabled
            || self.raster_trees.index_count == 0
        {
            return Ok(None);
        }
        let extent = self
            .resources
            .extent_dependent_resources
            .gfx_output_tex
            .get_image()
            .get_desc()
            .extent;
        let grid = PixelGrid::new(
            [extent.width, extent.height],
            self.tree_pixels.settings.pixel_size,
        )?;
        let compute = &self.pipeline_topology.compute().tree_pixel_ppl;
        let display = &self.pipeline_topology.graphics().tree_pixel_ppl;
        compute.begin_transient_descriptor_frame(frame);
        display.begin_transient_descriptor_frame(frame);
        let mut prepared = PreparedTreePixels {
            grid,
            bands: Vec::new(),
        };
        for (index, band) in grid.bands().into_iter().enumerate() {
            let tiles = self.model_pixel_tiles.get_stream(
                frame,
                ("tree.pixels", 0, index as u32),
                grid.width as usize * band.rows as usize,
                self.vulkan_ctx.device().clone(),
                self.allocator.clone(),
            )?;
            let bindings = [("tree_pixel_tiles", DescriptorResource::Buffer(&tiles))];
            compute.record_with_descriptors(
                cmd,
                &bindings,
                Extent3D::new(grid.width, band.rows, 1),
                Some(bytemuck::bytes_of(&grid.push(band))),
            )?;
            prepared
                .bands
                .push((band, display.prepare_draw_descriptors(cmd, &bindings)?));
            self.tree_pixels.last.push((tiles, grid, band));
        }
        self.tree_pixels.frames += 1;
        Ok(Some(prepared))
    }

    pub fn tree_pixel_frames(&self) -> u64 {
        self.tree_pixels.frames
    }

    /// Explicit GPU fixture, not a production frame-loop download.
    pub fn validate_tree_pixels(&self) -> Result<()> {
        anyhow::ensure!(
            !self.tree_pixels.last.is_empty(),
            "no published pixelized tree frame"
        );
        let mut opaque = 0usize;
        let mut empty = 0usize;
        for (source, grid, band) in &self.tree_pixels.last {
            let bytes = grid.width as u64 * band.rows as u64 * 16;
            let readback = Buffer::new_sized(
                self.vulkan_ctx.device().clone(),
                self.allocator.clone(),
                re_flora_vkn::BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                re_flora_vkn::MemoryLocation::GpuToCpu,
                bytes,
            );
            execute_one_time_gpu_job(
                self.vulkan_ctx.device(),
                self.vulkan_ctx.command_pool(),
                &self.vulkan_ctx.get_general_queue(),
                |cmd| {
                    source.record_copy_to_buffer(cmd, &readback, bytes, 0, 0);
                    cmd.use_buffer(&readback, BufferUse::HostRead);
                },
            );
            for value in readback.read_back()?.chunks_exact(16) {
                let value: [f32; 4] = bytemuck::pod_read_unaligned(value);
                anyhow::ensure!(
                    value.iter().all(|v| v.is_finite() && *v >= 0.) && value[3] <= 1.,
                    "invalid tree pixel color/depth: {value:?}"
                );
                if value[3] < 1. {
                    opaque += 1;
                } else {
                    empty += 1;
                }
            }
        }
        anyhow::ensure!(
            opaque > 0 && empty > 0,
            "tree pixel fixture requires wood and background"
        );
        log::info!("[TREE][PIXELS] validated camera=current opaque={opaque} empty={empty} bands={} frames={}",
            self.tree_pixels.last.len(),self.tree_pixels.frames);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_current_camera_grids_batch_without_losing_rows() {
        for extent in [[1, 1], [513, 287], [5120, 2880], [16384, 16384]] {
            for size in [1, 3, 4, 16] {
                let grid = PixelGrid::new(extent, size).unwrap();
                let bands = grid.bands();
                assert_eq!(bands.iter().map(|b| b.rows).sum::<u32>(), grid.height);
                assert!(bands.iter().all(|b| b.rows > 0
                    && b.rows as usize * grid.width as usize <= model_pixel_tiles::BATCH_TEXELS));
                assert!(bands
                    .windows(2)
                    .all(|b| b[1].first == b[0].first + b[0].rows));
                assert!(grid.width * size >= extent[0] && (grid.width - 1) * size < extent[0]);
                assert!(grid.height * size >= extent[1] && (grid.height - 1) * size < extent[1]);
            }
        }
        assert!(PixelGrid::new([0, 10], 4).is_err());
    }
    #[test]
    fn rectangular_adapter_preserves_integer_screen_pixel_centers() {
        let grid = PixelGrid::new([513, 287], 4).unwrap();
        let span = 2.0 * (grid.width * grid.size) as f64;
        for (x, y) in [(0, 0), (37, 19), (grid.width - 1, grid.height - 1)] {
            let ndc = [
                -1. + (x as f64 + 0.5) / grid.width as f64 * span / 513.,
                -1. + (y as f64 + 0.5) / grid.width as f64 * span / 287.,
            ];
            assert!(((ndc[0] + 1.) * 0.5 * 513. - (x as f64 + 0.5) * 4.).abs() < 1e-10);
            assert!(((ndc[1] + 1.) * 0.5 * 287. - (y as f64 + 0.5) * 4.).abs() < 1e-10);
        }
    }
}
