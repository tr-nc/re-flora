//! Opt-in real-game drop trace. No alternate physics or pose filtering.
use super::*;
use re_flora_physics::FixedStepResult;
use std::fs::File;
use std::io::{BufWriter, Write};

pub(super) struct FruitGroundTrace {
    output: BufWriter<File>,
    frame: u64,
    simulated: f32,
    step: Option<(f32, FixedStepResult)>,
}

impl FruitGroundTrace {
    pub(super) fn from_env() -> Option<Self> {
        let path = std::env::var_os("RE_FLORA_FRUIT_GROUND_TRACE")?;
        let output = File::create(&path).expect("create fruit ground diagnostic trace");
        log::info!("[FRUIT_GROUND_TRACE] path={path:?} auto_drop_frame=90");
        Some(Self {
            output: BufWriter::new(output),
            frame: 0,
            simulated: 0.0,
            step: None,
        })
    }

    pub(super) fn prepare(
        &mut self,
        physics: &mut TerrainPhysics,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        self.frame += 1;
        match self.frame {
            60 => physics.set_fruit_cycle(0.8, tracer)?,
            90 => physics.set_fruit_cycle(1.0, tracer)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn stepped(&mut self, frame_dt: f32, step: FixedStepResult) {
        self.simulated += step.simulated_seconds;
        self.step = Some((frame_dt, step));
    }

    pub(super) fn record(
        &mut self,
        physics: &TerrainPhysics,
        instances: &[DynamicFruitRenderInstance],
    ) -> anyhow::Result<()> {
        let Some((frame_dt, step)) = self.step.take() else {
            return Ok(());
        };
        for (index, (tree_id, fruit)) in physics
            .fruits_by_tree
            .iter()
            .flat_map(|(tree, fruits)| fruits.values().map(move |fruit| (*tree, fruit)))
            .filter(|(_, fruit)| fruit.body.is_some())
            .take(8)
            .enumerate()
        {
            let body = fruit.body.unwrap();
            let state = physics.collision_world.dynamic_body_state(body).unwrap();
            let contacts = physics
                .collision_world
                .dynamic_body_diagnostics(body)
                .unwrap();
            let row = serde_json::json!({
                "frame": self.frame, "simulated": self.simulated,
                "frame_dt": frame_dt, "fixed_dt": physics.collision_world.fixed_step_seconds(),
                "steps": step.steps, "dropped": step.dropped_seconds, "alpha": step.interpolation_alpha,
                "tree": tree_id, "fruit": fruit.spec.id, "body": body.get(),
                "position": state.position.to_array(), "rotation": state.rotation.to_array(),
                "linvel": state.linear_velocity.to_array(), "angvel": state.angular_velocity.to_array(),
                "sleeping": state.sleeping, "sleep_timer": contacts.time_since_can_sleep,
                "pairs": contacts.contact_pairs, "manifolds": contacts.manifolds,
                "solver_contacts": contacts.solver_contacts, "distances": contacts.contact_distances,
                "normals": contacts.contact_normals.iter().map(|v| v.to_array()).collect::<Vec<_>>(),
                "impulse": contacts.impulse,
                "force": contacts.user_force.to_array(), "torque": contacts.user_torque.to_array(),
                "render_position": instances[index].position.to_array(),
                "render_rotation": instances[index].rotation.to_array(),
                "dirty_bricks": physics.dirty_terrain_bricks.len(),
            });
            serde_json::to_writer(&mut self.output, &row)?;
            writeln!(self.output)?;
        }
        self.output.flush()?;
        Ok(())
    }
}
