//! Opt-in native regression through Winit pointer routing, picking and redraws.
//! Synthetic window events are not a claim of OS-mouse or audible acceptance.
use super::*;
use glam::Vec2;
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
};

pub(super) struct PointerReview {
    frame: u32,
    case: usize,
    start: Vec3,
    turning: bool,
    recovering: bool,
    profiling: bool,
    recovery_anchor: Option<Vec3>,
    fixed_at_rejection: Option<usize>,
    recovery_forward: Vec3,
    recovery_exit: Option<crate::app::path_band::CourseSection>,
    join_verified: bool,
    origin: Option<Vec3>,
    began: Option<Instant>,
    prefix_witness: Option<(UVec3, UVec3, u32)>,
    blocked_witness: Option<(usize, usize, u32)>,
    original: (f32, u32, f32),
}

const CASES: [Vec2; 2] = [Vec2::new(1.25, 1.65), Vec2::new(1.3, 0.45)];
// Wider paving needs enough straight lead to publish before a tight turn.
const TURN_LEAD: f32 = 0.5;

impl PointerReview {
    pub(super) fn record_published(&mut self, row: PathSample) -> Result<()> {
        if self.recovering
            && self
                .blocked_witness
                .is_some_and(|w| row.index as usize == w.1)
        {
            ensure!(
                Some(row.entry) == self.recovery_exit,
                "actual first continued course did not share the original accepted exit"
            );
            self.join_verified = true;
        }
        Ok(())
    }
}

impl App {
    fn prepare_pavement_pointer_case(&mut self, case: usize) -> Result<Vec3> {
        self.contree_builder.flush_cpu_chunk_cache_jobs();
        let mut at = CASES[case];
        // Isolate long gestures from one another; ordinary drags retain their
        // original natural-soil coordinates.
        if matches!(
            std::env::var("RE_FLORA_PAVEMENT_REVIEW").as_deref(),
            Ok("turn" | "recover" | "perf")
        ) {
            at = Vec2::new(0.35, 1.2 + case as f32 * 0.45);
        }
        let start = self
            .query_terrain_ray_cpu(Vec3::new(at.x, 2.5, at.y), Vec3::NEG_Y)
            .filter(|h| is_ground(h.voxel_type))
            .context("pointer fixture requires soil")?
            .position;
        let s = &mut self.debug_settings.adjustables;
        s.pavement_piece_length.value = 0.05;
        s.pavement_kind.value = case as u32;
        s.pavement_width.value = 0.12;
        // Orbit mode keeps the pointer unlocked even for a hidden window.
        self.camera_control = super::super::camera_control::CameraControlRuntime::default();
        self.camera_control.set_orbit_focus(start);
        let (camera, target) = if std::env::var("RE_FLORA_PAVEMENT_REVIEW").as_deref() == Ok("perf")
        {
            let target = start + Vec3::X * 0.65;
            (target + Vec3::new(0., 1.2, 1.4), target)
        } else {
            (start + Vec3::new(0., 0.6, 0.8), start)
        };
        ensure!(
            self.tracer.set_camera_pose_looking_at(camera, target),
            "pointer fixture camera"
        );
        self.sync_cursor_with_panels();
        self.reset_camera_movement_input();
        Ok(start)
    }

    // Isolate the gesture lifecycle from legitimate unbuildable voxel-picked
    // kinks on steep soil. This opt-in test garden uses the real soil voxelizer,
    // atlas and visible publication; ordinary pointer tests retain natural ground.
    fn seed_recovery_test_ground(&mut self) -> Result<()> {
        use crate::app::path_band::CellSolid;
        let footprint = [
            Vec2::new(0.02, 0.3),
            Vec2::new(1.98, 0.3),
            Vec2::new(1.98, 1.98),
            Vec2::new(0.02, 1.98),
        ];
        let cell = PathCell {
            center: Vec3::new(1., 0.48, 1.14),
            forward: Vec3::X,
            size: Vec3::new(1.68, 0.42, 1.96),
            seed: 0,
            footprint,
        };
        let solid = CellSolid::new(footprint, [0.48; 4], 0., 0.42, 1.);
        let grounded = grounded_cell(
            cell,
            Vec3::ZERO,
            self.world_chunk_dim.as_vec3(),
            solid.triangles(),
        )?
        .context("recovery fixture garden bounds")?;
        let bound = self.plain_builder.voxelize_model(
            &grounded.triangles,
            grounded.base,
            VOXEL_TYPE_DIRT,
        )?;
        self.publish_visible_terrain(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                bound,
            )])?
            .context("recovery test garden publication")?,
        )?;
        self.observe_player_terrain_publication_for_connectivity(bound);
        log::info!("[PAVEMENT_RECOVERY_REVIEW] garden=flat fixture_only=true ordinary_pointer_garden=unchanged");
        Ok(())
    }

    pub(super) fn begin_pavement_pointer_review(&mut self) -> Result<()> {
        let s = &self.debug_settings.adjustables;
        let original = (
            s.pavement_piece_length.value,
            s.pavement_kind.value,
            s.pavement_width.value,
        );
        self.select_item_panel_slot(super::super::ui_style::PAVEMENT_SLOT_INDEX);
        self.frame_timing_panel_visible = false;
        if matches!(
            std::env::var("RE_FLORA_PAVEMENT_REVIEW").as_deref(),
            Ok("turn" | "recover" | "perf")
        ) {
            self.seed_recovery_test_ground()?;
        }
        let start = self.prepare_pavement_pointer_case(0)?;
        self.pavement.pointer_review = Some(PointerReview {
            frame: 0,
            case: 0,
            start,
            turning: matches!(
                std::env::var("RE_FLORA_PAVEMENT_REVIEW").as_deref(),
                Ok("turn" | "recover")
            ),
            recovering: std::env::var("RE_FLORA_PAVEMENT_REVIEW").as_deref() == Ok("recover"),
            profiling: std::env::var("RE_FLORA_PAVEMENT_REVIEW").as_deref() == Ok("perf"),
            recovery_anchor: None,
            fixed_at_rejection: None,
            recovery_forward: Vec3::X,
            recovery_exit: None,
            join_verified: false,
            origin: None,
            began: None,
            prefix_witness: None,
            blocked_witness: None,
            original,
        });
        Ok(())
    }

    pub(in crate::app::core) fn advance_pavement_pointer_review(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<()> {
        let Some(mut review) = self.pavement.pointer_review.take() else {
            return Ok(());
        };
        // Let the real overlay complete two frames before pressing in the world.
        if review.frame >= 2 {
            let step = review.frame - 2;
            let last_step = if review.profiling {
                130
            } else if review.recovering {
                149
            } else if review.turning {
                75
            } else {
                40
            };
            let mut point = if review.recovering && step > 75 {
                if step <= 100 {
                    // Resume beyond the original input head; approaching it from
                    // behind would intentionally retract this editable stroke.
                    review.recovery_anchor.context("valid anchor")?
                        + review.recovery_forward * (0.5 + (step - 76) as f32 * 0.004)
                } else {
                    let angle = (step - 100) as f32 / 49. * std::f32::consts::FRAC_PI_2;
                    let left = Vec3::new(-review.recovery_forward.z, 0., review.recovery_forward.x);
                    review
                        .recovery_anchor
                        .context("recovery needs an accepted endpoint")?
                        + review.recovery_forward * (0.6 + 0.18 * angle.sin())
                        + left * (0.18 * (1. - angle.cos()))
                }
            } else if review.turning && step > 30 {
                // Start outside the approved backdrag capture corridor. A small
                // initial reverse is now a stroke edit, not an invalid target.
                let diagonal = 0.2;
                let extra = step.saturating_sub(50) as f32 * 0.01;
                // Keep both rejection fixtures genuinely ~160 degrees behind
                // the last fixed end. Sideways motion previously made the recover
                // fixture gentler; those targets are now correctly buildable.
                review.start
                    + Vec3::new(
                        TURN_LEAD - (diagonal + extra) * 0.94,
                        0.,
                        (diagonal + extra) * 0.342,
                    )
            } else {
                let distance = if review.turning {
                    step.min(30) as f32 / 30. * TURN_LEAD
                } else {
                    step.min(last_step) as f32 * 0.01
                };
                review.start + Vec3::X * distance
            };
            if (review.recovering && step > 75)
                || (!review.recovering && review.turning && step > 30)
            {
                // Specify an actual ground-path target, not a horizontal plane
                // under rising soil that an oblique ray would hit far off-route.
                point = self
                    .pavement
                    .active
                    .as_ref()
                    .context("recovery active")?
                    .ground
                    .query_terrain_ray_cpu(Vec3::new(point.x, 2.5, point.z), Vec3::NEG_Y)
                    .context("recovery target ground")?
                    .position;
            }
            if review.recovering && step > 75 {
                let anchor = review.recovery_anchor.context("direct candidate anchor")?;
                ensure!(
                    glam::Vec2::new(point.x - anchor.x, point.z - anchor.z).length() >= 0.18,
                    "direct-candidate fixture accidentally visited the old accepted endpoint"
                );
            }
            let matrix = self.tracer.camera_view_projection();
            let extent = self.window_state.window_extent();
            let clip = matrix * point.extend(1.);
            ensure!(clip.w > 0., "pointer fixture is behind the camera");
            let ndc = clip.truncate() / clip.w;
            let cursor = Vec2::new(
                (ndc.x * 0.5 + 0.5) * extent.width as f32,
                (ndc.y * 0.5 + 0.5) * extent.height as f32,
            );
            let id = self.window_state.window().id();
            self.on_window_event(
                event_loop,
                id,
                WindowEvent::CursorMoved {
                    device_id: DeviceId::dummy(),
                    position: PhysicalPosition::new(cursor.x as f64, cursor.y as f64),
                },
            );
            if step == 0 {
                self.on_window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                    },
                );
                // Live settings affect only the next gesture.
                self.debug_settings.adjustables.pavement_width.value = 0.24;
            }
            if review.turning && step == 60 {
                // A redundant down while still held must not begin at a new
                // location (or adopt the changed next-gesture width).
                self.on_window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                    },
                );
            }
            let active = self.pavement.active.as_ref().with_context(|| {
                format!(
                    "drag preview disappeared at case={} step={step} cursor={cursor:?}",
                    review.case
                )
            })?;
            ensure!(
                !self.pavement.pending_preview().is_empty()
                    || (review.recovering && self.pavement.intent_preview(&[]).is_some()),
                "drag tail preview is empty at case={} step={step}",
                review.case
            );
            if let Some(began) = review.began {
                ensure!(
                    active.started == began,
                    "held recovery replaced the active gesture"
                );
            } else {
                review.began = Some(active.started);
            }
            if let Some(origin) = review.origin {
                ensure!(
                    active.intent.origin() == origin,
                    "held path changed its original press point"
                );
            } else {
                review.origin = Some(active.intent.origin());
            }

            ensure!(
                active.stroke.config.width == 0.12,
                "live width changed the frozen pavement"
            );
            if !review.turning {
                // The adaptable tail can be provisionally red on a voxel riser
                // (or while a corner lacks lookahead), then resolve as input
                // extends. It must not latch a permanent stable-course failure.
                // A rejected current target is replaceable, not a held-drag latch.
                if step == last_step {
                    ensure!(
                        active.continuity.blocked().is_none(),
                        "ordinary drag did not find a valid endpoint at case={}",
                        review.case
                    );
                }
            } else if active.continuity.blocked().is_some() {
                if review.fixed_at_rejection.is_none() {
                    review.fixed_at_rejection = Some(active.intent.floor());
                }
                ensure!(
                    Some(active.intent.floor()) == review.fixed_at_rejection,
                    "rejected targets were saved as fixed endpoints"
                );
                if review.recovery_anchor.is_none() {
                    review.recovery_anchor = active.continuity.accepted().map(|r| r.exit.center);
                    if let Some(row) = active.continuity.accepted() {
                        review.recovery_exit = Some(row.exit);
                        review.recovery_forward =
                            Vec3::new(-row.exit.across.z, 0., row.exit.across.x);
                    }
                }
                ensure!(
                    self.pavement.pending_preview().iter().all(|p| !p.valid),
                    "blocked drag became a green restart"
                );
                ensure!(
                    self.player_tools.terrain_edit_loop_sound_muted,
                    "blocked drag kept Dig audible"
                );
                if review.blocked_witness.is_none()
                    || step == 75
                    || (step == last_step && !review.recovering)
                {
                    // Covers both the accepted prefix and the attempted later
                    // route, over the full world height. No renderer-only proof.
                    let width = active.recipe.path_settings().width;
                    let mut lo = ((review.start - Vec3::new(width, 0., width)) * 256.)
                        .floor()
                        .max(Vec3::ZERO)
                        .as_uvec3();
                    let mut hi = ((review.start + Vec3::new(0.3 + width, 0., 0.35 + width)) * 256.)
                        .ceil()
                        .as_uvec3()
                        .min(self.world_chunk_dim * super::super::VOXEL_DIM_PER_CHUNK);
                    lo.y = 0;
                    hi.y = self.world_chunk_dim.y * super::super::VOXEL_DIM_PER_CHUNK.y;
                    let bytes = self.plain_builder.read_chunk_atlas_region(lo, hi - lo)?;
                    let witness = (
                        active.pieces_placed,
                        active.surfaces_cleared,
                        crc32fast::hash(&bytes),
                    );
                    if let Some(first) = review.blocked_witness {
                        ensure!(
                            first == witness,
                            "held invalid drag wrote terrain after blocking"
                        );
                    } else {
                        review.blocked_witness = Some(witness);
                        // New proposal rejection occurs before later courses bake.
                        // Witness the interior of the actual first baked course,
                        // not a future station correctly written on continuation.
                        let p = review.start + Vec3::X * 0.01;
                        let mut lo = (p * 256.)
                            .floor()
                            .as_uvec3()
                            .saturating_sub(UVec3::new(2, 0, 2));
                        lo.y = 0;
                        let dim = UVec3::new(
                            5,
                            self.world_chunk_dim.y * super::super::VOXEL_DIM_PER_CHUNK.y,
                            5,
                        );
                        let bytes = self.plain_builder.read_chunk_atlas_region(lo, dim)?;
                        review.prefix_witness = Some((lo, dim, crc32fast::hash(&bytes)));
                    }
                }
            }

            if step == last_step {
                if review.recovering {
                    ensure!(
                        review.join_verified,
                        "first continued join was not verified: expected={} blocked={:?} cursor={:?} anchor={:?} fixed={} recovery_anchor={:?}",
                        active.continuity.expected(), active.continuity.blocked().map(|b|b.reason), active.intent.cursor(), active.intent.anchor(), active.intent.floor(), review.recovery_anchor
                    );
                    let (lo, dim, crc) =
                        review.prefix_witness.context("accepted prefix witness")?;
                    ensure!(
                        crc32fast::hash(&self.plain_builder.read_chunk_atlas_region(lo, dim)?)
                            == crc,
                        "candidate retry changed the earlier baked prefix"
                    );
                    ensure!(
                        active.continuity.expected() as usize == active.surfaces_cleared,
                        "recovery skipped course IDs"
                    );
                    ensure!(
                        active.continuity.blocked().is_none(),
                        "a valid direct target left the held path blocked without retracing"
                    );
                    ensure!(
                        active.surfaces_cleared
                            >= review.blocked_witness.context("blocked witness")?.1 + 2,
                        "same held path did not continue from a direct valid target"
                    );
                }
                if review.turning {
                    ensure!(
                        review.blocked_witness.is_some(),
                        "tight turn must block, not silently split"
                    );
                    if !review.recovering {
                        ensure!(
                            active
                                .continuity
                                .blocked()
                                .is_some_and(|b| b.reason == BlockReason::Turn),
                            "fixture did not exercise geometric turn limit"
                        );
                    }
                    ensure!(
                        active
                            .continuity
                            .accepted()
                            .is_some_and(|r| r.index + 1 == active.continuity.expected())
                            && active.continuity.expected() as usize == active.surfaces_cleared,
                        "paving course checkpoint disagrees with actual publication"
                    );
                }
                // Count only published courses, never the adaptable preview.
                let minimum = if review.turning && !review.recovering {
                    3
                } else {
                    2
                };
                ensure!(
                    active.surfaces_cleared >= minimum,
                    "drag only placed a standing column: accepted {} path cells",
                    active.surfaces_cleared
                );
                let rows = active.stroke.pending_rows();
                let along = rows
                    .iter()
                    .flat_map(|r| active.recipe.pieces(*r))
                    .chain(active.coverage.published_pieces().iter().copied())
                    .flat_map(|p| p.footprint)
                    .map(|p| p.x)
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), x| {
                        (a.min(x), b.max(x))
                    });
                if !review.turning {
                    ensure!(
                        along.1 - along.0 >= 0.3,
                        "pavement did not extend along ground"
                    );
                }
                ensure!(
                    rows.iter().all(|r| r.forward.y.abs() < 1e-6),
                    "path points upward instead of along ground"
                );
                let name = active.recipe.name();
                self.on_window_event(
                    event_loop,
                    id,
                    WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Released,
                        button: MouseButton::Left,
                    },
                );
                ensure!(self.pavement.active.is_none(), "release retained a gesture");
                if review.profiling {
                    let offset = ((review.start - Vec3::splat(0.1)) * 256.)
                        .floor()
                        .as_uvec3();
                    let mut bytes = self
                        .plain_builder
                        .read_chunk_atlas_region(offset, UVec3::new(384, 52, 52))?;
                    for byte in &mut bytes {
                        *byte &= crate::builder::VOXEL_TYPE_MASK;
                    }
                    log::info!("[PAVEMENT_PERF_REVIEW] kind={name} steps=130 material_crc={} release=muted", crc32fast::hash(&bytes));
                }
                if review.recovering {
                    let (lo, dim, crc) = review.prefix_witness.context("release prefix witness")?;
                    ensure!(
                        crc32fast::hash(&self.plain_builder.read_chunk_atlas_region(lo, dim)?)
                            == crc,
                        "release changed the old baked prefix"
                    );
                }
                ensure!(
                    self.player_tools.terrain_edit_loop_sound_muted,
                    "release retained the shared Dig loop"
                );
                if review.recovering {
                    log::info!("[PAVEMENT_RECOVERY_REVIEW] kind={name} intent_origin=retained invalid_tail=red paused_atlas=exact retry=valid_target no_retreat=true same_gesture=true contiguous_courses=true baked_prefix=exact frozen_width=true release=muted");
                } else if review.turning {
                    log::info!("[PAVEMENT_TURN_REVIEW] kind={name} red_preview=true contiguous_prefix=true terrain_after_block=unchanged held_no_restart=true duplicate_down=ignored release=muted");
                } else {
                    log::info!("[PAVEMENT_POINTER_REVIEW] kind={name} drag=verified window_events=true preview_retained=true along_ground=true frozen_width=true release=muted");
                }
                review.case += 1;
                if review.case == CASES.len() {
                    if review.recovering {
                        self.review_guarded_construction_candidates()?;
                    }
                    let s = &mut self.debug_settings.adjustables;
                    s.pavement_piece_length.value = review.original.0;
                    s.pavement_kind.value = review.original.1;
                    s.pavement_width.value = review.original.2;
                    if review.recovering {
                        log::info!("[PAVEMENT_RECOVERY_REVIEW] complete=true cases=2 settings_restored=true");
                    } else if review.turning {
                        log::info!("[PAVEMENT_TURN_REVIEW] complete=true cases=2 single_path_per_drag=true");
                    } else if review.profiling {
                        log::info!("[PAVEMENT_PERF_REVIEW] complete=true cases=2 steps=130 settings_restored=true");
                    } else {
                        log::info!("[PAVEMENT_POINTER_REVIEW] complete=true cases=2 settings_restored=true");
                    }
                    return Ok(());
                }
                review.start = self.prepare_pavement_pointer_case(review.case)?;
                review.blocked_witness = None;
                review.recovery_anchor = None;
                review.fixed_at_rejection = None;
                review.recovery_exit = None;
                review.join_verified = false;
                review.origin = None;
                review.began = None;
                review.prefix_witness = None;
                review.frame = 0;
            }
        }
        review.frame += 1;
        self.pavement.pointer_review = Some(review);
        Ok(())
    }
}
