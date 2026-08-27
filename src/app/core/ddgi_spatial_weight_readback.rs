use crate::ddgi::{
    DdgiFieldIdentity, DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT,
    DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE,
    DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_RECEIVER, DDGI_SPATIAL_WEIGHT_READBACK_PIXELS,
    DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT, DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT,
};
use crate::tracer::Tracer;
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{Buffer, BufferUsage, CommandBuffer, MemoryLocation, VulkanContext};
use std::io::Write;
use std::path::Path;

const READBACK_WIDTH: u32 = 1440;
const READBACK_HEIGHT: u32 = 810;
const FLOAT4_BYTE_COUNT: usize = std::mem::size_of::<[f32; 4]>();
const AGGREGATE_OFFSET: usize =
    1 + DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReadbackState {
    #[default]
    WaitingForTerminalField,
    Armed,
    Recording,
    Complete,
}

pub(super) struct DdgiSpatialWeightReadbackRuntime {
    path: Option<String>,
    state: ReadbackState,
}

pub(super) struct PendingDdgiSpatialWeightReadback {
    path: String,
    field: DdgiFieldIdentity,
    buffer: Buffer,
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

impl DdgiSpatialWeightReadbackRuntime {
    pub(super) fn new(path: Option<String>) -> Self {
        Self {
            path,
            state: ReadbackState::WaitingForTerminalField,
        }
    }

    fn should_record(&mut self, terminal_field_ready: bool) -> bool {
        if self.path.is_none()
            || matches!(
                self.state,
                ReadbackState::Recording | ReadbackState::Complete
            )
        {
            return false;
        }
        if !terminal_field_ready {
            self.state = ReadbackState::WaitingForTerminalField;
            return false;
        }
        match self.state {
            ReadbackState::WaitingForTerminalField => {
                self.state = ReadbackState::Armed;
                log::info!(
                    "[DDGI_SPATIAL_WEIGHT_READBACK] armed; waiting one frame for shading_info ready"
                );
                false
            }
            ReadbackState::Armed => true,
            ReadbackState::Recording | ReadbackState::Complete => false,
        }
    }

    pub(super) fn record_if_ready(
        &mut self,
        tracer: &Tracer,
        vulkan_ctx: &VulkanContext,
        cmdbuf: &CommandBuffer,
    ) -> Result<Option<PendingDdgiSpatialWeightReadback>> {
        let active = tracer.ddgi_runtime_status().active();
        let terminal_field = active
            .complete_field
            .filter(|field| field.field().state() == crate::ddgi::DdgiFieldState::Converged);
        if !self.should_record(terminal_field.is_some()) {
            return Ok(None);
        }

        let path = self
            .path
            .clone()
            .context("DDGI spatial-weight readback path disappeared after arming")?;
        let output_path = Path::new(&path);
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                anyhow::bail!("parent directory does not exist: {}", parent.display());
            }
        }

        let extent = tracer.environment_irradiance_capture_extent();
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
        let field = terminal_field
            .context("cannot capture DDGI spatial weights before a terminal field is complete")?;
        ensure!(
            active.published_field == Some(field),
            "terminal DDGI field is not the published active field: terminal={field:?} published={:?}",
            active.published_field,
        );

        let allocator = tracer
            .get_screen_output_tex()
            .get_image()
            .get_allocator()
            .clone();
        let buffer = Buffer::new_sized(
            vulkan_ctx.device().clone(),
            allocator,
            BufferUsage::transfer_dst(),
            MemoryLocation::GpuToCpu,
            DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT as u64,
        );
        let readback = PendingDdgiSpatialWeightReadback {
            path,
            field,
            buffer,
        };
        tracer.record_ddgi_spatial_weight_readback(cmdbuf, &readback.buffer);
        self.state = ReadbackState::Recording;
        log::info!(
            "[DDGI_SPATIAL_WEIGHT_READBACK] recording path={}",
            readback.path,
        );
        Ok(Some(readback))
    }

    pub(super) fn complete(&mut self, readback: PendingDdgiSpatialWeightReadback) -> Result<()> {
        debug_assert_eq!(self.state, ReadbackState::Recording);
        self.state = ReadbackState::Complete;
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
            "field=serial:{} geometry_revision:{} radiance_revision:{} spacing_voxels:{} state:{:?} update_epoch:{}",
            field_key.serial(),
            field_key.geometry_revision(),
            field_key.radiance_revision(),
            field_key.spacing_voxels(),
            field_key.state(),
            field_key.update_epoch(),
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

        for (receiver_index, pixel) in DDGI_SPATIAL_WEIGHT_READBACK_PIXELS
            .iter()
            .copied()
            .enumerate()
            .take(DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT)
        {
            let receiver_base = receiver_index * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_RECEIVER;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_field_arms_before_recording() {
        let mut runtime = DdgiSpatialWeightReadbackRuntime::new(Some("readback.txt".to_owned()));

        assert!(!runtime.should_record(false));
        assert_eq!(runtime.state, ReadbackState::WaitingForTerminalField);
        assert!(!runtime.should_record(true));
        assert_eq!(runtime.state, ReadbackState::Armed);
        assert!(runtime.should_record(true));
    }

    #[test]
    fn losing_the_terminal_field_requires_a_fresh_arm_frame() {
        let mut runtime = DdgiSpatialWeightReadbackRuntime::new(Some("readback.txt".to_owned()));

        assert!(!runtime.should_record(true));
        assert!(!runtime.should_record(false));
        assert_eq!(runtime.state, ReadbackState::WaitingForTerminalField);
        assert!(!runtime.should_record(true));
        assert_eq!(runtime.state, ReadbackState::Armed);
    }

    #[test]
    fn disabled_or_in_flight_runtime_cannot_record_again() {
        let mut disabled = DdgiSpatialWeightReadbackRuntime::new(None);
        assert!(!disabled.should_record(true));

        let mut runtime = DdgiSpatialWeightReadbackRuntime::new(Some("readback.txt".to_owned()));
        runtime.state = ReadbackState::Recording;
        assert!(!runtime.should_record(true));
        runtime.state = ReadbackState::Complete;
        assert!(!runtime.should_record(true));
    }
}
