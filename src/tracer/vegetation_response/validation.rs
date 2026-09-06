//! Opt-in real-GPU acceptance using the production compute shader and live wind
//! settings. It never mutates the game's wind buffers or response state.
use super::*;
use crate::{
    generated::gpu_structs::{GuiInput, WindSources},
    resource::Resource,
};
use re_flora_vkn::{
    execute_one_time_command, BufferUse, DescriptorPool, DescriptorUpdate, ResourceContainer,
    ResourceLookup, ShaderModule, VulkanContext,
};

struct WindInputs {
    wind_field_info: Resource<Buffer>,
    replay_parameters: Resource<Buffer>,
    wind_sources: Resource<Buffer>,
}

impl ResourceContainer for WindInputs {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_> {
        // Reuse the production shader interface, but bind only independently
        // allocated replay buffers, never the game's live GUI uniform.
        match name {
            "wind_field_info" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.wind_field_info))
            }
            "gui_input" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.replay_parameters))
            }
            "wind_sources" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.wind_sources))
            }
            _ => ResourceLookup::Missing,
        }
    }
}

struct Harness<'a> {
    context: &'a VulkanContext,
    pipeline: ComputePipeline,
    wind: WindInputs,
    buffers: FrameBuffers,
    readback: Buffer,
    controls: [f32; 4],
    _pool: DescriptorPool,
}

impl<'a> Harness<'a> {
    fn new(
        context: &'a VulkanContext,
        allocator: Allocator,
        count: usize,
        source_bytes: usize,
    ) -> Result<Self> {
        let device = context.device();
        let shader = ShaderModule::from_precompiled(
            device,
            "shader/foliage/vegetation_response.comp",
            "main",
        )
        .map_err(anyhow::Error::msg)?;
        let pool = DescriptorPool::new(device)?;
        let wind =
            WindInputs {
                wind_field_info: Resource::new(Buffer::new_uniform::<
                    crate::wind_field::WindFieldFrame,
                >(device.clone(), allocator.clone())),
                replay_parameters: Resource::new(Buffer::new_uniform::<GuiInput>(
                    device.clone(),
                    allocator.clone(),
                )),
                wind_sources: Resource::new(Buffer::new_sized(
                    device.clone(),
                    allocator.clone(),
                    BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                    MemoryLocation::CpuToGpu,
                    source_bytes.max(32) as u64,
                )),
            };
        wind.wind_field_info
            .fill_uniform(&crate::wind_field::WindFieldFrame::default())?;
        let pipeline = ComputePipeline::new_uninitialized(device, &shader, &pool);
        pipeline.initialize_descriptors(DescriptorUpdate::SetContaining {
            anchor: "gui_input",
            providers: &[&wind],
        })?;
        let buffers = FrameBuffers::new(device.clone(), allocator.clone(), count);
        let readback = Buffer::new_sized(
            device.clone(),
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            count as u64 * STATE_BYTES,
        );
        Ok(Self {
            context,
            pipeline,
            wind,
            buffers,
            readback,
            controls: [1., 1., 1., 0.],
            _pool: pool,
        })
    }

    fn step(
        &mut self,
        inputs: &[ResponseInput],
        start: f32,
        end: f32,
        tick: f32,
    ) -> Result<Vec<[f32; 20]>> {
        self.buffers
            .inputs
            .fill_range_with_raw_u8(0, bytemuck::cast_slice(inputs))?;
        let previous = self.buffers.output_index;
        let output = previous ^ 1;
        let step = ResponseStep {
            start_time: start,
            end_time: end,
            tick_seconds: tick,
            count: inputs.len() as u32,
            controls: self.controls,
        };
        self.pipeline.begin_transient_descriptor_frame(0);
        execute_one_time_command(
            self.context.device(),
            self.context.command_pool(),
            &self.context.get_general_queue(),
            |cmd| -> Result<()> {
                self.pipeline.record_with_descriptors(
                    cmd,
                    &[
                        (
                            "response_inputs",
                            DescriptorResource::Buffer(&self.buffers.inputs),
                        ),
                        (
                            "response_previous",
                            DescriptorResource::Buffer(&self.buffers.outputs[previous]),
                        ),
                        (
                            "response_output",
                            DescriptorResource::Buffer(&self.buffers.outputs[output]),
                        ),
                    ],
                    Extent3D::new(step.count, 1, 1),
                    Some(bytemuck::bytes_of(&step)),
                )?;
                self.buffers.outputs[output].record_copy_to_buffer(
                    cmd,
                    &self.readback,
                    inputs.len() as u64 * STATE_BYTES,
                    0,
                    0,
                );
                cmd.use_buffer(&self.readback, BufferUse::HostRead);
                Ok(())
            },
        )?;
        self.buffers.output_index = output;
        let bytes = self
            .readback
            .read_back_range(0, inputs.len() as u64 * STATE_BYTES)?;
        let states = bytemuck::try_cast_slice::<u8, [f32; 20]>(&bytes)
            .map_err(|err| anyhow::anyhow!("response readback ABI: {err}"))?
            .to_vec();
        anyhow::ensure!(
            states.iter().flatten().all(|value| value.is_finite()),
            "nonfinite GPU response"
        );
        Ok(states)
    }
}

pub(in crate::tracer) fn validate_gpu(
    context: &VulkanContext,
    allocator: Allocator,
    resources: &crate::tracer::TracerResources,
) -> Result<()> {
    let source_bytes = resources.wind.wind_sources.read_back()?;
    let live_gui_bytes = resources.uniforms.gui_input.read_back()?;
    let live_gui: GuiInput = bytemuck::pod_read_unaligned(&live_gui_bytes);
    let mut harness = Harness::new(context, allocator, 2113, source_bytes.len())?;
    let mut gui = GuiInput::zeroed();
    gui.wind_source_count = 1;
    gui.wind_directional_bias_fraction = 1.;
    harness.wind.replay_parameters.fill_uniform(&gui)?;
    let mut source = WindSources {
        params: [0., 0., 1., 0.],
        noise: [1., 1., 2., 0.5],
    };
    harness
        .wind
        .wind_sources
        .fill_range_with_raw_u8(0, bytemuck::bytes_of(&source))?;
    // Same forcing and the same spatial point deliberately remove wind-field
    // variation: individual leaf mechanics must not collapse into one spray pose.
    let mut leaves: Vec<_> = [11, 23, 47, 83]
        .into_iter()
        .map(|seed| ResponseInput {
            root: [1., 1., 1., 0.],
            identity: [NO_PREVIOUS, 5, seed, 1],
        })
        .collect();
    let mut leaf_difference = 0.0_f32;
    for frame in 0..30 {
        let states = harness.step(&leaves, frame as f32 / 60., (frame + 1) as f32 / 60., 0.025)?;
        for state in &states[1..] {
            leaf_difference = leaf_difference.max((states[0][0] - state[0]).abs());
        }
        for (index, leaf) in leaves.iter_mut().enumerate() {
            leaf.identity[0] = index as u32;
        }
    }
    anyhow::ensure!(
        leaf_difference > 0.02,
        "individual leaves collapse into one mechanical response: difference={leaf_difference}"
    );
    log::info!("[VEGETATION_RESPONSE][INDIVIDUAL_LEAVES] same_force_max_difference={leaf_difference:.6} independent_mechanics=passed");
    let mut inputs: Vec<_> = [0, 2, 3, 4, 5, 6]
        .into_iter()
        .map(|species| ResponseInput {
            root: [1., 1., 1., 0.],
            identity: [NO_PREVIOUS, species, 0, 0],
        })
        .collect();
    let mut minimum_after_stop = 0.0_f32;
    let mut late_peak = 0.0_f32;
    let mut grass_flower_difference = 0.0_f32;
    let mut last = vec![];
    let mut previous_held = None;
    let mut response_samples = Vec::new();
    for frame in 0..360 {
        if frame == 120 {
            source.params[2] = 0.;
            harness
                .wind
                .wind_sources
                .fill_range_with_raw_u8(0, bytemuck::bytes_of(&source))?;
        }
        last = harness.step(&inputs, frame as f32 / 60., (frame + 1) as f32 / 60., 0.05)?;
        response_samples.push(last.clone());
        for (index, input) in inputs.iter_mut().enumerate() {
            input.identity[0] = index as u32;
        }
        grass_flower_difference = grass_flower_difference.max((last[0][0] - last[1][0]).abs());
        if frame >= 120 {
            minimum_after_stop = minimum_after_stop.min(last[0][0]);
        }
        if frame >= 300 {
            late_peak = late_peak.max(last[0][0].abs());
        }
        if frame == 0 {
            previous_held = Some(last[0][4..].to_vec());
        }
        if frame == 1 {
            anyhow::ensure!(
                previous_held.as_ref().unwrap() == &last[0][4..],
                "pose changed between publication ticks"
            );
        }
    }
    // Production-state measurements, not a verdict on the deliberately discrete art style.
    for (index, species) in [0, 2, 3, 4, 5, 6].into_iter().enumerate() {
        let reach = response_samples[..120]
            .iter()
            .position(|s| s[index][0] >= 0.9)
            .map(|frame| (frame + 1) as f32 / 60.)
            .unwrap_or(f32::NAN);
        let changes = response_samples
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0][index][4] != pair[1][index][4])
            .map(|(frame, _)| frame + 1)
            .collect::<Vec<_>>();
        let gaps = changes.windows(2).map(|p| p[1] - p[0]).collect::<Vec<_>>();
        anyhow::ensure!(
            gaps.iter().all(|&gap| (11..=13).contains(&gap)),
            "publication cadence missed its 5 Hz contract: species={species} gaps={gaps:?}"
        );
        log::info!("[VEGETATION_RESPONSE][RHYTHM] species={species} force_step_t90_seconds={reach:.4} held_bucket_hz=5 min_hold_frames={} max_hold_frames={} sample_fps=60 subjective_acceptance=required",
            gaps.iter().min().unwrap(), gaps.iter().max().unwrap());
    }
    anyhow::ensure!(
        minimum_after_stop < -0.05 && late_peak < 0.02,
        "missing underdamped stop/decay: minimum={minimum_after_stop} late_peak={late_peak}"
    );
    anyhow::ensure!(
        grass_flower_difference > 0.1,
        "grass and flower responses are identical"
    );
    // Identity remap and birth are read from real previous GPU state, with no time advance.
    let old = last.clone();
    inputs[0].identity[0] = 1;
    inputs[1].identity[0] = NO_PREVIOUS;
    last = harness.step(&inputs, 6., 6., 0.05)?;
    anyhow::ensure!(
        last[0] == old[1] && last[1].iter().all(|&v| v == 0.),
        "GPU remap or new lifetime inherited velocity"
    );
    log::info!("[VEGETATION_RESPONSE][GPU_VALIDATION] stop_min={minimum_after_stop:.6} late_peak={late_peak:.6} grass_flower_difference={grass_flower_difference:.6} held_pose=passed lifetime_remap=passed");

    let mut trajectories = Vec::new();
    for tick in [0.1, 0.05, 0.025] {
        for input in &mut inputs {
            input.identity[0] = NO_PREVIOUS;
        }
        let mut trajectory = Vec::new();
        for frame in 0..90 {
            source.params[0] = if (frame / 9) % 2 == 0 { 0. } else { 180. };
            source.params[2] = if frame < 54 { 1. } else { 0. };
            harness
                .wind
                .wind_sources
                .fill_range_with_raw_u8(0, bytemuck::bytes_of(&source))?;
            last = harness.step(&inputs, frame as f32 / 60., (frame + 1) as f32 / 60., tick)?;
            trajectory.push(last[0][0]);
            for (index, input) in inputs.iter_mut().enumerate() {
                input.identity[0] = index as u32;
            }
        }
        anyhow::ensure!(
            trajectory
                .windows(2)
                .all(|pair| (pair[1] - pair[0]).abs() < 0.2),
            "rapid reversal teleported a pose"
        );
        trajectories.push(trajectory);
    }
    let cadence_error = trajectories[1..]
        .iter()
        .flat_map(|trajectory| {
            trajectory
                .iter()
                .zip(&trajectories[0])
                .map(|(a, b)| (a - b).abs())
        })
        .fold(0.0_f32, f32::max);
    anyhow::ensure!(
        cadence_error < 0.0001,
        "presentation cadence altered dynamics: {cadence_error}"
    );
    log::info!("[VEGETATION_RESPONSE][GPU_VALIDATION] rapid_reversal=passed display_hz=2.5,5,10 trajectory_max_delta={cadence_error:.8}");

    // All classes execute the production solver. Live changes at zero dt must
    // preserve state; acceptance below tests effects separately, one control at a time.
    let mut control_results = Vec::new();
    for controls in [
        [1., 1., 1., 0.],
        [1.5, 1., 1., 0.],
        [1., 2., 1., 0.],
        [1., 1., 2., 0.],
    ] {
        harness.controls = controls;
        for input in &mut inputs {
            input.identity[0] = NO_PREVIOUS;
        }
        source.params = [0., 0., 1., 0.];
        harness
            .wind
            .wind_sources
            .fill_range_with_raw_u8(0, bytemuck::bytes_of(&source))?;
        let mut t90 = vec![f32::NAN; inputs.len()];
        let mut overshoot = vec![0.0_f32; inputs.len()];
        for frame in 0..180 {
            last = harness.step(&inputs, frame as f32 / 60., (frame + 1) as f32 / 60., 0.05)?;
            for index in 0..inputs.len() {
                inputs[index].identity[0] = index as u32;
                if t90[index].is_nan() && last[index][0] >= 0.9 * controls[2] {
                    t90[index] = (frame + 1) as f32 / 60.;
                }
                overshoot[index] = overshoot[index].max(last[index][0] / controls[2] - 1.);
            }
        }
        let old = last.clone();
        harness.controls = [3., 0.25, 0., 0.];
        let changed = harness.step(&inputs, 3., 3., 0.025)?;
        anyhow::ensure!(
            old == changed,
            "live controls or cadence change reset state"
        );
        log::info!("[VEGETATION_RESPONSE][CONTROLS] multipliers={controls:?} species=0,2,3,4,5,6 t90_seconds={t90:?} overshoot={overshoot:?} zero_dt_continuity=passed");
        control_results.push((t90, overshoot, last.clone()));
    }
    for index in 0..inputs.len() {
        anyhow::ensure!(
            control_results[1].0[index] < control_results[0].0[index],
            "speed control ineffective for class {index}"
        );
        anyhow::ensure!(
            control_results[2].1[index] < control_results[0].1[index],
            "damping control ineffective for class {index}"
        );
        anyhow::ensure!(
            (control_results[3].2[index][0] - 2. * control_results[0].2[index][0]).abs() < 0.0001,
            "gain control ineffective for class {index}"
        );
    }
    harness.controls = [1., 1., 1., 0.];

    // Compare a 16-voxel production grid against exact midpoint root responses
    // under the *current* wind source settings, over the actual 512-voxel world.
    harness.wind.replay_parameters.fill_uniform(&live_gui)?;
    harness
        .wind
        .wind_sources
        .fill_range_with_raw_u8(0, &source_bytes)?;
    let mut grid_inputs =
        VegetationResponse::new(UAabb3::new(glam::UVec3::ZERO, glam::UVec3::splat(2))).grid_inputs;
    grid_inputs.truncate(1089);
    for z in 0..32 {
        for x in 0..32 {
            grid_inputs.push(ResponseInput {
                root: [(x as f32 + 0.5) / 16., 0., (z as f32 + 0.5) / 16., 0.],
                identity: [NO_PREVIOUS, 0, 0, 0],
            });
        }
    }
    for frame in 0..120 {
        last = harness.step(
            &grid_inputs,
            frame as f32 / 60.,
            (frame + 1) as f32 / 60.,
            0.05,
        )?;
        for (index, input) in grid_inputs.iter_mut().enumerate() {
            input.identity[0] = index as u32;
        }
    }
    let mut squared_error = 0.;
    let mut squared_reference = 0.;
    let mut maximum_error = 0.0_f32;
    for z in 0..32 {
        for x in 0..32 {
            let exact = &last[1089 + x + z * 32];
            for axis in 0..2 {
                let interpolated = [
                    x + z * 33,
                    x + 1 + z * 33,
                    x + (z + 1) * 33,
                    x + 1 + (z + 1) * 33,
                ]
                .into_iter()
                .map(|i| last[i][axis])
                .sum::<f32>()
                    * 0.25;
                squared_error += (interpolated - exact[axis]).powi(2);
                squared_reference += exact[axis].powi(2);
                maximum_error = maximum_error.max((interpolated - exact[axis]).abs());
            }
        }
    }
    log::info!("[VEGETATION_RESPONSE][GPU_GRID] spacing_voxels=16 roots=1024 time=2s rms_voxels={:.6} normalized_rms={:.6} max_axis_error_voxels={:.6} sources={}",
        (squared_error / 2048.).sqrt(), (squared_error / squared_reference.max(1e-12)).sqrt(), maximum_error, live_gui.wind_source_count);
    if std::env::var_os("RE_FLORA_WIND_PROTOTYPE_SMOKE").is_some() {
        // Production GPU solver, private replay inputs: test a traveling packet,
        // a radial front, and expiry without changing the user's live field.
        let mut field = crate::wind_field::WindField::default();
        field.mode = 1;
        field.strength = 0.;
        field.detail_strength = 0.;
        harness
            .wind
            .replay_parameters
            .fill_uniform(&GuiInput::zeroed())?;
        field.advance(0.);
        field.release(glam::Vec3::new(1., 0.5, 1.), glam::Vec2::X, false);
        field.advance(1.);
        let front = 1. + field.manual_gust.speed / 256.;
        let probe = |x, z| ResponseInput {
            root: [x, 0., z, 0.],
            identity: [NO_PREVIOUS, 0, 0, 0],
        };
        let probes = [
            probe(front, 1.),
            probe(1., front),
            probe(0., 0.),
            probe(1., 1.),
        ];
        harness.wind.wind_field_info.fill_uniform(&field.frame())?;
        let directional = harness.step(&probes, 0., 0.2, 0.025)?;
        anyhow::ensure!(
            directional[0][0] > 0.1
                && directional[0][1].abs() < 1e-6
                && directional[1][0].abs() < 1e-6
                && directional[2][0].abs() < 1e-6,
            "directional gust escaped its support or lost direction"
        );
        let extent = field.gusts[0].half_extents() / 256.;
        let band_probes = [
            probe(front, 1.),
            probe(front, 1. + extent.y * 0.25),
            probe(front, 1. + extent.y * 0.75),
            probe(front, 1. - extent.y * 0.75),
            probe(front, 1. + extent.y * 1.01),
            probe(front + extent.x * 1.01, 1.),
            probe(front + extent.x * 0.75, 1. + extent.y * 0.75),
        ];
        let band = harness.step(&band_probes, 0., 0.2, 0.025)?;
        anyhow::ensure!(
            (extent.y / extent.x - 4.).abs() < 1e-6
                && band[0][0] > band[1][0]
                && band[1][0] > band[2][0]
                && band[2][0] > 0.
                && (band[2][0] - band[3][0]).abs() < 1e-5
                && band[4][0].abs() < 1e-6
                && band[5][0].abs() < 1e-6
                && band[6][0] > 0.,
            "wind band lost its rectangular support or symmetric center-to-edge falloff"
        );
        log::info!("[WIND_PROTOTYPE][GPU] band_aspect=4:1 center_to_edge_falloff=passed symmetric_sides=passed rectangular_support=passed");
        field.gusts[0].settings.softness = 0.2;
        harness.wind.wind_field_info.fill_uniform(&field.frame())?;
        let sharper = harness.step(&band_probes, 0., 0.2, 0.025)?;
        anyhow::ensure!(
            sharper[2][0] > band[2][0] && sharper[4][0].abs() < 1e-6,
            "edge softness did not affect force independently of the bounds"
        );
        field.gusts[0].settings.softness = 1.;
        field.gusts[0].settings.width *= 2.;
        harness.wind.wind_field_info.fill_uniform(&field.frame())?;
        let wider = harness.step(&band_probes, 0., 0.2, 0.025)?;
        anyhow::ensure!(
            wider[4][0] > 0. && wider[5][0].abs() < 1e-6,
            "width adjustment changed depth or failed to expand sideways"
        );
        field.gusts[0].settings.width *= 0.5;
        log::info!("[WIND_PROTOTYPE][GPU] adjustable_softness=passed independent_width=passed");
        for mode in 0..=2 {
            field.mode = mode;
            harness.wind.wind_field_info.fill_uniform(&field.frame())?;
            let manual = harness.step(&probes, 0., 0.2, 0.025)?;
            anyhow::ensure!(
                (manual[0][0] - directional[0][0]).abs() < 1e-6,
                "background mode {mode} changed or disabled manual wind"
            );
        }
        log::info!("[WIND_PROTOTYPE][GPU] manual_wind_modes=A,B,C identical_force=passed");
        field.mode = 1;
        field.advance(6.);
        field.release(glam::Vec3::new(1., 0.5, 1.), glam::Vec2::ZERO, true);
        field.advance(7.);
        harness.wind.wind_field_info.fill_uniform(&field.frame())?;
        let radial = harness.step(&probes, 0., 0.2, 0.025)?;
        anyhow::ensure!(
            radial[0][0] > 0.1
                && radial[1][1] > 0.1
                && radial[3][0].abs() < 1e-6
                && radial[3][1].abs() < 1e-6,
            "radial gust failed outward directions or center singularity"
        );
        field.advance(12.);
        harness.wind.wind_field_info.fill_uniform(&field.frame())?;
        let expired = harness.step(&probes, 0., 0.2, 0.025)?;
        anyhow::ensure!(
            expired
                .iter()
                .all(|state| state.iter().all(|value| value.abs() < 1e-6)),
            "expired gust still applies force"
        );
        log::info!("[WIND_PROTOTYPE][GPU] directional_support=passed radial_directions=passed finite_center=passed expired_force=zero");
    }
    Ok(())
}
