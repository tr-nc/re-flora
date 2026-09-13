//! Opt-in real-GPU acceptance using the production compute shader and live wind
//! settings. It never mutates the game's wind buffers or response state.
use super::*;
use crate::resource::Resource;
use re_flora_vkn::{
    execute_one_time_command, BufferUse, DescriptorPool, DescriptorUpdate, ResourceContainer,
    ResourceLookup, ShaderModule, VulkanContext,
};

struct WindInputs {
    wind_field_info: Resource<Buffer>,
}

impl ResourceContainer for WindInputs {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_> {
        // Reuse the production shader interface, but bind only independently
        // allocated replay buffers, never the game's live GUI uniform.
        match name {
            "wind_field_info" => {
                ResourceLookup::Unique(DescriptorResource::Buffer(&self.wind_field_info))
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
    fn new(context: &'a VulkanContext, allocator: Allocator, count: usize) -> Result<Self> {
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
            };
        wind.wind_field_info
            .fill_uniform(&crate::wind_field::WindFieldFrame::default())?;
        let pipeline = ComputePipeline::new_uninitialized(device, &shader, &pool);
        pipeline.initialize_descriptors(DescriptorUpdate::SetContaining {
            anchor: "wind_field_info",
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
    ) -> Result<Vec<[f32; STATE_FLOATS]>> {
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
        let states = bytemuck::try_cast_slice::<u8, [f32; STATE_FLOATS]>(&bytes)
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
    let live_bytes = resources.wind.wind_field_info.read_back()?;
    let live_field: crate::wind_field::WindFieldFrame = bytemuck::pod_read_unaligned(&live_bytes);
    let mut harness = Harness::new(context, allocator, 2113)?;
    let mut source = crate::wind_field::WindFieldFrame::uniform(glam::Vec2::X);
    harness.wind.wind_field_info.fill_uniform(&source)?;
    // Same forcing and the same spatial point deliberately remove wind-field
    // variation: individual leaf mechanics must not collapse into one spray pose.
    let mut leaves: Vec<_> = [11, 23, 47, 83]
        .into_iter()
        .map(|seed| ResponseInput {
            root: [1., 1., 1., 0.],
            identity: [
                NO_PREVIOUS,
                species::TREE_LEAF_RENDER_SPECIES_INDEX,
                seed,
                1,
            ],
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
    // The production GPU state owns local angle and all held publication buckets.
    harness.controls[3] = 1.;
    for leaf in &mut leaves {
        leaf.identity[0] = NO_PREVIOUS;
    }
    source = crate::wind_field::WindFieldFrame::uniform(glam::Vec2::X * 0.3);
    harness.wind.wind_field_info.fill_uniform(&source)?;
    let mut angle_peak = 0_f32;
    let mut angle_difference = 0_f32;
    let mut quiet_angle = 0_f32;
    for frame in 0..480 {
        if frame == 240 {
            source.cells.fill([0.; 4]);
            harness.wind.wind_field_info.fill_uniform(&source)?;
        }
        let states = harness.step(&leaves, frame as f32 / 60., (frame + 1) as f32 / 60., 0.05)?;
        angle_peak = angle_peak.max(states[0][20].abs());
        angle_difference = angle_difference.max((states[0][20] - states[1][20]).abs());
        quiet_angle = states[0][20].abs();
        anyhow::ensure!(
            states
                .iter()
                .all(|s| s[24..28].iter().all(|a| a.abs() <= 1.)),
            "unbounded leaf publication"
        );
        for (index, leaf) in leaves.iter_mut().enumerate() {
            leaf.identity[0] = index as u32;
        }
    }
    anyhow::ensure!(
        angle_peak > 0.03 && angle_peak < 1. && angle_difference > 0.02 && quiet_angle < 0.001,
        "leaf torsion invalid peak={angle_peak} independent={angle_difference} quiet={quiet_angle}"
    );
    log::info!("[LEAF_FLUTTER][GPU] peak_angle={angle_peak:.5} independent_difference={angle_difference:.5} quiet_angle={quiet_angle:.8} held_bounds=passed");
    harness.controls[3] = 0.;
    source = crate::wind_field::WindFieldFrame::uniform(glam::Vec2::X);
    harness.wind.wind_field_info.fill_uniform(&source)?;
    let validation_species = [
        species::TALL_GRASS_SPECIES_INDEX,
        species::LAVENDER_SPECIES_INDEX,
        species::EMBER_BLOOM_SPECIES_INDEX,
        species::TREE_LEAF_RENDER_SPECIES_INDEX,
        species::APPLE_RENDER_SPECIES_INDEX,
    ];
    let mut inputs: Vec<_> = validation_species
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
            source.cells.fill([0.; 4]);
            harness.wind.wind_field_info.fill_uniform(&source)?;
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
            previous_held = Some(last[0][4..20].to_vec());
        }
        if frame == 1 {
            anyhow::ensure!(
                previous_held.as_ref().unwrap() == &last[0][4..20],
                "pose changed between publication ticks"
            );
        }
    }
    // Production-state measurements, not a verdict on the deliberately discrete art style.
    for (index, species) in validation_species.into_iter().enumerate() {
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
            let force = if frame >= 54 {
                0.
            } else if (frame / 9) % 2 == 0 {
                1.
            } else {
                -1.
            };
            source = crate::wind_field::WindFieldFrame::uniform(glam::Vec2::X * force);
            harness.wind.wind_field_info.fill_uniform(&source)?;
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
        source = crate::wind_field::WindFieldFrame::uniform(glam::Vec2::X);
        harness.wind.wind_field_info.fill_uniform(&source)?;
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
        log::info!("[VEGETATION_RESPONSE][CONTROLS] multipliers={controls:?} species={validation_species:?} t90_seconds={t90:?} overshoot={overshoot:?} zero_dt_continuity=passed");
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
    harness.wind.wind_field_info.fill_uniform(&live_field)?;
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
    log::info!("[VEGETATION_RESPONSE][GPU_GRID] spacing_voxels=16 roots=1024 time=2s rms_voxels={:.6} normalized_rms={:.6} max_axis_error_voxels={:.6} shared_field=true",
        (squared_error / 2048.).sqrt(), (squared_error / squared_reference.max(1e-12)).sqrt(), maximum_error);
    if std::env::var_os("RE_FLORA_WIND_PROTOTYPE_SMOKE").is_some() {
        // Validate the transported field through the actual plant solver. Compare
        // it with uniform fields at each CPU-sampled probe, avoiding assumptions
        // about the plant solver's nonlinear response curve.
        let mut field = crate::wind_field::WindField::default();
        field.strength = 0.;
        field.detail_strength = 0.;
        field.advance(0.);
        field.release(glam::Vec3::new(1., 0.5, 1.), glam::Vec2::X);
        field.advance(1.);
        let probe = |x, z| ResponseInput {
            root: [x, 0., z, 0.],
            identity: [NO_PREVIOUS, 0, 0, 0],
        };
        let probes = [
            probe(1.2, 1.),
            probe(1.2, 1.3),
            probe(0., 0.),
            probe(1., 1.),
        ];
        harness.wind.wind_field_info.fill_uniform(&field.frame())?;
        let actual = harness.step(&probes, 0., 0.2, 0.025)?;
        anyhow::ensure!(
            actual[0][0] > 0.01 && actual[2][0].abs() < 1e-6,
            "local input failed to enter the field or changed a distant location"
        );
        for (index, probe) in probes.iter().enumerate() {
            let expected = field.sample(glam::Vec2::new(probe.root[0], probe.root[2]) * 256.);
            let mut uniform = field.frame();
            uniform
                .cells
                .fill([expected.x, expected.y, expected.x, expected.y]);
            harness.wind.wind_field_info.fill_uniform(&uniform)?;
            let reference = harness.step(&[*probe], 0., 0.2, 0.025)?;
            anyhow::ensure!(
                actual[index]
                    .iter()
                    .zip(reference[0])
                    .all(|(a, b)| (a - b).abs() < 1e-4),
                "CPU/GPU transported wind sampling mismatch at probe {index}"
            );
        }
        log::info!("[WIND_PROTOTYPE][GPU] shared_field=passed cpu_gpu_sampling=passed local_support=passed");
    }
    Ok(())
}
