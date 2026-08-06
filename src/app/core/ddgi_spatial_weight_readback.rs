use super::App;
use crate::ddgi::{
    DdgiFieldIdentity, DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT,
    DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE,
    DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_RECEIVER, DDGI_SPATIAL_WEIGHT_READBACK_PIXELS,
    DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT, DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT,
};
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, MemoryLocation};
use std::io::Write;
use std::path::Path;

const READBACK_WIDTH: u32 = 1440;
const READBACK_HEIGHT: u32 = 810;
const FLOAT4_BYTE_COUNT: usize = std::mem::size_of::<[f32; 4]>();
const AGGREGATE_OFFSET: usize =
    1 + DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE;

pub(super) struct DdgiSpatialWeightReadback {
    path: String,
    field: DdgiFieldIdentity,
    buffer: Buffer,
}

impl DdgiSpatialWeightReadback {
    pub(super) fn path(&self) -> &str {
        &self.path
    }
}

fn read_float4(raw: &[u8], float4_index: usize) -> [f32; 4] {
    let offset = float4_index * FLOAT4_BYTE_COUNT;
    let bytes = &raw[offset..offset + FLOAT4_BYTE_COUNT];
    [
        f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        f32::from_le_bytes(bytes[12..16].try_into().unwrap()),
    ]
}

fn write_float4(writer: &mut impl Write, label: &str, value: [f32; 4]) -> Result<()> {
    writeln!(
        writer,
        "{label}={:.9},{:.9},{:.9},{:.9}",
        value[0], value[1], value[2], value[3]
    )?;
    Ok(())
}

impl App {
    pub(super) fn prepare_ddgi_spatial_weight_readback(
        &self,
        path: String,
    ) -> Result<DdgiSpatialWeightReadback> {
        let output_path = Path::new(&path);
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                anyhow::bail!("parent directory does not exist: {}", parent.display());
            }
        }

        let extent = self.tracer.environment_irradiance_capture_extent();
        ensure!(
            extent.width == READBACK_WIDTH && extent.height == READBACK_HEIGHT,
            "DDGI spatial-weight readback requires {}x{} rendering extent, got {}x{}",
            READBACK_WIDTH,
            READBACK_HEIGHT,
            extent.width,
            extent.height,
        );
        ensure!(
            DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT
                == DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT
                    * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_RECEIVER
                    * FLOAT4_BYTE_COUNT,
            "DDGI spatial-weight readback host layout is inconsistent"
        );
        let active = self.tracer.ddgi_runtime_status().active();
        let field = active
            .complete_field
            .filter(|field| {
                matches!(
                    field.field().stage(),
                    crate::ddgi::DdgiFieldStage::Converged
                        | crate::ddgi::DdgiFieldStage::NonConverged
                )
            })
            .context("cannot capture DDGI spatial weights before a terminal field is complete")?;
        ensure!(
            active.published_field == Some(field),
            "terminal DDGI field is not the published active field: terminal={field:?} published={:?}",
            active.published_field,
        );

        let allocator = self
            .tracer
            .get_screen_output_tex()
            .get_image()
            .get_allocator()
            .clone();
        let buffer = Buffer::new_sized(
            self.vulkan_ctx.device().clone(),
            allocator,
            BufferUsage::transfer_dst(),
            MemoryLocation::GpuToCpu,
            DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT as u64,
        );
        Ok(DdgiSpatialWeightReadback {
            path,
            field,
            buffer,
        })
    }

    pub(super) fn record_ddgi_spatial_weight_readback(
        &self,
        cmdbuf: &CommandBuffer,
        readback: &DdgiSpatialWeightReadback,
    ) {
        self.tracer
            .record_ddgi_spatial_weight_readback(cmdbuf, &readback.buffer);
    }

    pub(super) fn write_ddgi_spatial_weight_readback(
        readback: DdgiSpatialWeightReadback,
    ) -> Result<()> {
        let raw = readback.buffer.read_back()?;
        ensure!(
            raw.len() == DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT,
            "DDGI spatial-weight readback byte count mismatch: got {}, expected {}",
            raw.len(),
            DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT,
        );

        let mut file = std::fs::File::create(&readback.path)
            .with_context(|| format!("create {}", readback.path))?;
        writeln!(file, "# DDGI_SPATIAL_WEIGHT_READBACK v1")?;
        writeln!(file, "render_extent=1440x810")?;
        writeln!(file, "screen_extent=2880x1620")?;
        writeln!(file, "tracer_scale=0.5")?;
        let field_key = readback.field.field();
        writeln!(
            file,
            "field=serial:{} geometry_revision:{} radiance_revision:{} spacing_voxels:{} stage:{:?} iteration:{}",
            field_key.serial(),
            field_key.geometry_revision(),
            field_key.radiance_revision(),
            field_key.spacing_voxels(),
            field_key.stage(),
            field_key.iteration(),
        )?;
        writeln!(
            file,
            "receiver_count={DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT}"
        )?;
        writeln!(
            file,
            "probe_count={DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT}"
        )?;
        writeln!(
            file,
            "probe_float4_stride={DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE}"
        )?;
        writeln!(
            file,
            "layout=receiver;probe_meta;actual_position_and_position_weight;nominal_position_and_surface_side_weight;irradiance_and_moment_visibility;visibility_and_support;candidate_exact_weights_current_nominal_wrap_nominal_wrap;exact_result;consumer_result;exact_summary;consumer_summary"
        )?;

        for receiver_index in 0..DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT {
            let receiver_base = receiver_index * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_RECEIVER;
            let pixel = DDGI_SPATIAL_WEIGHT_READBACK_PIXELS[receiver_index];
            writeln!(
                file,
                "receiver={receiver_index} tracer_pixel={},{} screen_reference_pixel={},{}",
                pixel[0],
                pixel[1],
                pixel[0] * 2,
                pixel[1] * 2,
            )?;
            write_float4(
                &mut file,
                "receiver_position_and_query_status_bit0_hit_bit1_ready_bit2_global_sky",
                read_float4(&raw, receiver_base),
            )?;

            for probe_index in 0..DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT {
                let probe_base = receiver_base
                    + 1
                    + probe_index * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE;
                let meta = read_float4(&raw, probe_base);
                let actual = read_float4(&raw, probe_base + 1);
                let nominal = read_float4(&raw, probe_base + 2);
                let irradiance = read_float4(&raw, probe_base + 3);
                let visibility = read_float4(&raw, probe_base + 4);
                let weights = read_float4(&raw, probe_base + 5);
                writeln!(
                    file,
                    "probe={receiver_index}:{probe_index} index={:.0} state={:.0} trustworthy={:.0} rejection_flags={:.0}",
                    meta[0],
                    meta[1],
                    meta[2],
                    meta[3],
                )?;
                write_float4(&mut file, "  actual_position_and_position_weight", actual)?;
                write_float4(
                    &mut file,
                    "  nominal_position_and_surface_side_weight",
                    nominal,
                )?;
                write_float4(&mut file, "  irradiance_and_moment_visibility", irradiance)?;
                write_float4(&mut file, "  visibility_and_support", visibility)?;
                write_float4(
                    &mut file,
                    "  candidate_exact_weights_current_nominal_wrap_nominal_wrap",
                    weights,
                )?;
            }

            write_float4(
                &mut file,
                "exact_result_irradiance_and_weight",
                read_float4(&raw, receiver_base + AGGREGATE_OFFSET),
            )?;
            write_float4(
                &mut file,
                "consumer_result_irradiance_and_weight",
                read_float4(&raw, receiver_base + AGGREGATE_OFFSET + 1),
            )?;
            write_float4(
                &mut file,
                "exact_summary_base_mean_visibility_count_dominant",
                read_float4(&raw, receiver_base + AGGREGATE_OFFSET + 2),
            )?;
            write_float4(
                &mut file,
                "consumer_summary_base_mean_visibility_count_dominant",
                read_float4(&raw, receiver_base + AGGREGATE_OFFSET + 3),
            )?;
        }
        file.flush()?;
        log::info!(
            "[DDGI_SPATIAL_WEIGHT_READBACK] saved {} bytes to {}",
            raw.len(),
            readback.path
        );
        Ok(())
    }
}
