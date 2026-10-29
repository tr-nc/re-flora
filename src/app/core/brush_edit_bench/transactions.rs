//! Opt-in real GPU acceptance for transaction ownership, ordering and conflicts.
use super::*;
use crate::builder::{GrassEditCompletion, GrassEditTransaction, GrassPaintRequest};

fn publish(app: &mut App, transaction: GrassEditTransaction) -> Result<()> {
    transaction.wait()?;
    match app
        .surface_builder
        .finish_grass_transaction(transaction, app.visible_terrain_revision)?
    {
        GrassEditCompletion::Published { retired, .. } => {
            app.frame_manager.retirement_sink().retire(retired)
        }
        GrassEditCompletion::Stale(_) => anyhow::bail!("unexpected stale acceptance transaction"),
    }
    Ok(())
}
fn reject(app: &mut App, transaction: GrassEditTransaction) -> Result<()> {
    let before = roots(app)?;
    transaction.wait()?;
    anyhow::ensure!(
        matches!(
            app.surface_builder
                .finish_grass_transaction(transaction, app.visible_terrain_revision)?,
            GrassEditCompletion::Stale(_)
        ),
        "conflicting transaction published"
    );
    anyhow::ensure!(before == roots(app)?, "stale batch partially published");
    Ok(())
}

pub(super) fn verify(app: &mut App, replay: &Replay) -> Result<()> {
    anyhow::ensure!(
        replay.seed_step == 2,
        "transaction acceptance requires dense fixture"
    );
    let edit = TerrainBrushEdit {
        start: Vec3::new(192., 127.5, 96.) / 256.,
        end: Vec3::new(320., 127.5, 160.) / 256.,
        radius: 14. / 256.,
    };
    let request = GrassPaintRequest {
        voxel_bound: UAabb3::new(UVec3::new(178, 113, 82), UVec3::new(334, 142, 174)),
        start: edit.start,
        end: edit.end,
        radius: edit.radius,
        tick: 0,
        spawn_time_ms: 10000,
        selection: species::FloraPaintSelection::GrassMix,
        dab_serial: 7,
        brush: species::GRASS_MIX_PAINT_BRUSH_SETTINGS,
    };
    let index = app
        .surface_builder
        .resources
        .instances
        .chunk_flora_instances
        .iter()
        .position(|(_, r)| r.chunk_id == UVec3::ZERO)
        .unwrap();

    // Even completed GPU output cannot become visible before the aggregate publish.
    replay.reset(app)?;
    let before = roots(app)?;
    let tx = app
        .surface_builder
        .submit_grass_transaction(request, app.visible_terrain_revision)?;
    tx.wait()?;
    anyhow::ensure!(
        before == roots(app)?,
        "unpublished output touched the live generation"
    );
    publish(app, tx)?;
    anyhow::ensure!(
        roots(app)?.len() > before.len(),
        "acceptance brush added no roots"
    );

    // Merely recording a write does not mutate content or poison the source epoch.
    {
        let buffer = &app
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances[index]
            .1
            .resource
            .instances_buf;
        let version = buffer.write_revision();
        let command = re_flora_vkn::CommandBuffer::new(
            app.vulkan_ctx.device(),
            app.vulkan_ctx.command_pool(),
        );
        command.begin(true);
        command.use_buffer(buffer, re_flora_vkn::BufferUse::ComputeWrite);
        command.end();
        drop(command);
        anyhow::ensure!(
            version == buffer.write_revision(),
            "abandoned recording advanced content version"
        );
    }

    // Host writes and submitted GPU writes both invalidate the input snapshot.
    for gpu in [false, true] {
        replay.reset(app)?;
        let tx = app
            .surface_builder
            .submit_grass_transaction(request, app.visible_terrain_revision)?;
        if gpu {
            app.surface_builder
                .update_flora_growth_for_chunk(UVec3::ZERO, 0)?;
        } else {
            let r = &app
                .surface_builder
                .resources
                .instances
                .chunk_flora_instances[index]
                .1;
            let bytes = r.resource.instances_buf.read_back_range(0, 8)?;
            r.resource.instances_buf.fill_range_with_raw_u8(0, &bytes)?;
        }
        reject(app, tx)?;
    }

    // A real material edit advances the world's existing publication revision.
    replay.reset(app)?;
    let tx = app
        .surface_builder
        .submit_grass_transaction(request, app.visible_terrain_revision)?;
    app.apply_surface_terrain_removal(edit, None, Some(65536), None)?;
    reject(app, tx)?;

    // Readbacks, current hydration and unrelated species must not be rolled back.
    replay.reset(app)?;
    let flower_species = species::MODEL_FLOWER_FIRST_SPECIES as usize;
    let saved_flower = {
        let r = &app
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances[index]
            .1;
        let bytes = r.resource.instances_buf.read_back_range(
            u64::from(FloraInstanceResources::species_offset(flower_species)) * 8,
            u64::from(r.species_len(flower_species)) * 8,
        )?;
        bytes
            .chunks_exact(8)
            .map(bytemuck::pod_read_unaligned::<Instance>)
            .collect::<Vec<_>>()
    };
    let flower = Instance {
        packed_local_pos: 42 | (127 << 8) | (43 << 16) | (255 << 24),
        spawn_start_ms: u32::MAX,
    };
    app.surface_builder
        .resources
        .instances
        .chunk_flora_instances[index]
        .1
        .write_species_instances(flower_species, &[flower])?;
    let tx = app
        .surface_builder
        .submit_grass_transaction(request, app.visible_terrain_revision)?;
    let root = {
        let r = &mut app
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances[index]
            .1;
        r.hydration[0][0].1 = 0.37;
        r.upload_hydration(0)?;
        let root = r.hydration[0][0].0;
        r.root_water_reserve.insert(root, 0.123);
        root
    };
    publish(app, tx)?;
    {
        let r = &app
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances[index]
            .1;
        anyhow::ensure!(
            r.hydration[0]
                .iter()
                .find(|v| v.0 == root)
                .unwrap()
                .1
                .to_bits()
                == 0.37f32.to_bits(),
            "hydration rolled back"
        );
        anyhow::ensure!(r.root_water_reserve[&root] == 0.123, "water reserve lost");
        anyhow::ensure!(r.species_len(flower_species) == 1, "authored count lost");
        let bytes = r.resource.instances_buf.read_back_range(
            u64::from(FloraInstanceResources::species_offset(flower_species)) * 8,
            8,
        )?;
        anyhow::ensure!(
            bytes == bytemuck::bytes_of(&flower),
            "authored payload lost"
        );
    }
    app.surface_builder
        .resources
        .instances
        .chunk_flora_instances[index]
        .1
        .write_species_instances(flower_species, &saved_flower)?;

    // Capacity failure is private too: no cross-species overwrite or partial publish.
    replay.reset(app)?;
    {
        let r = &mut app
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances[index]
            .1;
        let full: Vec<_> = (0..crate::builder::MAX_FLORA_INSTANCES_PER_SPECIES)
            .map(|i| Instance {
                packed_local_pos: (i % 256) | (127 << 8) | ((i / 256) << 16) | (255 << 24),
                spawn_start_ms: u32::MAX,
            })
            .collect();
        r.write_species_instances(0, &full)?;
        r.write_species_instances(1, &[])?;
    }
    let mut capacity = request;
    capacity.start = Vec3::new(128., 127.5, 176.) / 256.;
    capacity.end = capacity.start;
    capacity.voxel_bound = UAabb3::new(UVec3::new(114, 113, 162), UVec3::new(142, 142, 190));
    let before_capacity = roots(app)?;
    let tx = app
        .surface_builder
        .submit_grass_transaction(capacity, app.visible_terrain_revision)?;
    tx.wait()?;
    anyhow::ensure!(
        app.surface_builder
            .finish_grass_transaction(tx, app.visible_terrain_revision)
            .is_err(),
        "capacity overflow was not rejected"
    );
    anyhow::ensure!(
        before_capacity == roots(app)?,
        "failed transaction partially published"
    );

    // Cancellation keeps old state and drains residency even when called immediately.
    replay.reset(app)?;
    let before = roots(app)?;
    drop(
        app.surface_builder
            .submit_grass_transaction(request, app.visible_terrain_revision)?,
    );
    anyhow::ensure!(before == roots(app)?, "cancelled paint was published");

    // One queued plus one incoming operation uses lossless drain, not replacement.
    app.queue_surface_grass_at(edit, 7, 10000)?;
    app.submit_queued_grass_edit()?;
    app.queue_surface_grass_at(edit, 8, 10000)?;
    let first = roots(app)?;
    anyhow::ensure!(
        first.len() > before.len(),
        "backpressure dropped accepted edit"
    );
    app.player_tools.planting.flora_index = 1; // changing tool cannot change captured selection
    app.flush_grass_edits()?;
    anyhow::ensure!(
        first == roots(app)?,
        "repeat/tool switch changed the accepted request"
    );
    app.player_tools.planting.flora_index = 0;

    // Repeated conflicts have bounded progress; retry costs are not hidden.
    replay.reset(app)?;
    app.queue_surface_grass_at(edit, 7, 10000)?;
    for _ in 0..3 {
        app.submit_queued_grass_edit()?;
        let r = &app
            .surface_builder
            .resources
            .instances
            .chunk_flora_instances[index]
            .1;
        let bytes = r.resource.instances_buf.read_back_range(0, 8)?;
        r.resource.instances_buf.fill_range_with_raw_u8(0, &bytes)?;
        app.vulkan_ctx.device().wait_idle(); // acceptance barrier, not the production poll
        app.poll_grass_edits()?;
    }
    anyhow::ensure!(
        app.grass_edits.is_idle() && app.grass_edits.last.unwrap().retries == 2,
        "conflict retry was unbounded or dropped the request"
    );
    anyhow::ensure!(
        roots(app)?.len() > before.len(),
        "conflict fallback omitted paint"
    );

    // Save uses the actual quiescence boundary, including a not-yet-submitted edit.
    replay.reset(app)?;
    app.queue_surface_grass_at(edit, 7, 10000)?;
    let directory = tempfile::tempdir()?;
    app.perform_startup_terrain_save(&directory.path().join("pending-grass.flora"))?;
    anyhow::ensure!(
        app.grass_edits.is_idle(),
        "save did not drain accepted paint"
    );
    anyhow::ensure!(
        roots(app)?.len() > before.len(),
        "save omitted queued grass"
    );
    replay.reset(app)?;
    log::info!("[GRASS_TRANSACTION_REVIEW] atomic=true abandoned_recording=true host_conflict=true gpu_conflict=true terrain_conflict=true hydration=true authored=true capacity=true cancel=true backpressure=true tool_switch=true bounded_retry=true save=true");
    Ok(())
}
