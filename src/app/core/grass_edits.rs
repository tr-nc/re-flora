//! One-slot admission/backpressure for unlimited ordinary-grass painting.
//! Input accepts a request; frame-tail submission snapshots the current world; polling
//! publishes a complete generation. Explicit edits/save drain this slot to preserve order.
use super::*;
use crate::builder::{GrassEditCompletion, GrassEditTransaction, GrassPaintRequest};

#[derive(Default)]
pub(super) struct GrassEdits {
    state: State,
    pub last: Option<GrassEditMetrics>,
}

#[derive(Clone, Copy, serde::Serialize)]
pub(super) struct GrassEditMetrics {
    pub submit_ms: f64,
    pub finish_ms: f64,
    pub accepted_to_publish_ms: f64,
    pub retries: u8,
}

impl GrassEdits {
    pub fn is_idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }
}

#[derive(Clone, Copy, Default)]
struct CpuCost {
    submit_ms: f64,
    finish_ms: f64,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Queued {
        request: GrassPaintRequest,
        accepted: Instant,
        retries: u8,
        cost: CpuCost,
    },
    Submitted {
        transaction: GrassEditTransaction,
        accepted: Instant,
        retries: u8,
        cost: CpuCost,
    },
}

impl App {
    pub(super) fn queue_grass_edit(&mut self, request: GrassPaintRequest) -> Result<()> {
        // Backpressure is lossless: never replace an accepted stroke endpoint.
        self.flush_grass_edits()?;
        if !self.surface_builder.supports_grass_transaction(&request) {
            return self.apply_grass_request_sync(request);
        }
        self.grass_edits.last = None;
        self.grass_edits.state = State::Queued {
            request,
            accepted: Instant::now(),
            retries: 0,
            cost: CpuCost::default(),
        };
        Ok(())
    }

    fn apply_grass_request_sync(&mut self, request: GrassPaintRequest) -> Result<()> {
        world_ops::mesh_regenerate_flora_for_brush_edit(
            &mut self.surface_builder,
            VOXEL_DIM_PER_CHUNK,
            request.voxel_bound,
            world_ops::FloraBrushEdit {
                start: request.start,
                end: request.end,
                radius: request.radius,
                tick: request.tick,
                spawn_time_ms: request.spawn_time_ms,
            },
            request.selection,
            request.dab_serial,
            request.brush,
        )
    }

    pub(super) fn submit_queued_grass_edit(&mut self) -> Result<()> {
        if !matches!(self.grass_edits.state, State::Queued { .. }) {
            return Ok(());
        }
        let State::Queued {
            request,
            accepted,
            retries,
            mut cost,
        } = std::mem::take(&mut self.grass_edits.state)
        else {
            unreachable!()
        };
        let start = Instant::now();
        let transaction = self
            .surface_builder
            .submit_grass_transaction(request, self.visible_terrain_revision)?;
        cost.submit_ms += start.elapsed().as_secs_f64() * 1000.;
        self.grass_edits.state = State::Submitted {
            transaction,
            accepted,
            retries,
            cost,
        };
        Ok(())
    }

    pub(super) fn poll_grass_edits(&mut self) -> Result<()> {
        let State::Submitted { transaction, .. } = &self.grass_edits.state else {
            return Ok(());
        };
        if !transaction.is_ready()? {
            return Ok(());
        }
        let State::Submitted {
            transaction,
            accepted,
            retries,
            mut cost,
        } = std::mem::take(&mut self.grass_edits.state)
        else {
            unreachable!()
        };
        let start = Instant::now();
        let result = self
            .surface_builder
            .finish_grass_transaction(transaction, self.visible_terrain_revision)?;
        cost.finish_ms += start.elapsed().as_secs_f64() * 1000.;
        match result {
            GrassEditCompletion::Stale(request) if retries < 2 => {
                log::debug!("[GRASS_EDIT] stale retry={}", retries + 1);
                self.grass_edits.state = State::Queued {
                    request,
                    accepted,
                    retries: retries + 1,
                    cost,
                };
            }
            GrassEditCompletion::Stale(request) => {
                // Bounded progress under continuous terrain/growth mutation. Same
                // admission backend, current state, no partial stale publication.
                log::info!("[GRASS_EDIT] synchronous conflict fallback retries={retries}");
                let fallback = Instant::now();
                self.apply_grass_request_sync(request)?;
                cost.finish_ms += fallback.elapsed().as_secs_f64() * 1000.;
                self.grass_edits.last = Some(GrassEditMetrics {
                    submit_ms: cost.submit_ms,
                    finish_ms: cost.finish_ms,
                    accepted_to_publish_ms: accepted.elapsed().as_secs_f64() * 1000.,
                    retries,
                });
            }
            GrassEditCompletion::Published {
                retired,
                changed_chunks,
                latency_ms,
            } => {
                self.frame_manager.retirement_sink().retire(retired);
                self.grass_edits.last = Some(GrassEditMetrics {
                    submit_ms: cost.submit_ms,
                    finish_ms: cost.finish_ms,
                    accepted_to_publish_ms: accepted.elapsed().as_secs_f64() * 1000.,
                    retries,
                });
                log::debug!("[GRASS_EDIT] published chunks={changed_chunks} retries={retries} submit_ms={:.3} finish_ms={:.3} submitted_to_publish_ms={latency_ms:.3} accepted_to_publish_ms={:.3}",
                    cost.submit_ms, cost.finish_ms, accepted.elapsed().as_secs_f64() * 1000.);
            }
        }
        Ok(())
    }

    pub(super) fn flush_grass_edits(&mut self) -> Result<()> {
        loop {
            match &mut self.grass_edits.state {
                State::Idle => return Ok(()),
                State::Queued { .. } => self.submit_queued_grass_edit()?,
                State::Submitted {
                    transaction, cost, ..
                } => {
                    let start = Instant::now();
                    transaction.wait()?;
                    cost.finish_ms += start.elapsed().as_secs_f64() * 1000.;
                    self.poll_grass_edits()?;
                }
            }
        }
    }
}
