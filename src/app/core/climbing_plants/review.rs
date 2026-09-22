//! Deterministic real-terrain pruning / waiting / repair scenario for hidden release runs.
use super::{
    wall_edit, Fixture, Plant, Terrain, WorldEditTransaction, VOXEL_TYPE_EMPTY,
    VOXEL_TYPE_LIMESTONE,
};
use anyhow::{ensure, Result};
use glam::UVec3;

#[derive(Default)]
enum Phase {
    #[default]
    Grow,
    Cut,
    Wait,
    Repair,
    RootCut,
    RootWait,
    RootRepair,
    Done,
}
#[derive(Default)]
pub(super) struct Review {
    phase: Phase,
    fixture: Option<Fixture>,
    before: Option<Plant>,
    stump: Option<Plant>,
    hole: Option<(UVec3, UVec3)>,
    pub ticks: u32,
}
impl Review {
    pub fn for_fixture(fixture: Option<Fixture>) -> Self {
        Self {
            fixture,
            ..Default::default()
        }
    }
    pub fn growing(&self) -> bool {
        !matches!(self.phase, Phase::Done)
    }
    pub fn phase(&self) -> &'static str {
        if let Some(fixture) = self.fixture {
            return fixture.name();
        }
        match self.phase {
            Phase::Grow => "growth",
            Phase::Cut => "cut",
            Phase::Wait => "waiting",
            Phase::Repair => "repair",
            Phase::RootCut => "root_cut",
            Phase::RootWait => "root_waiting",
            Phase::RootRepair => "root_repair",
            Phase::Done => "done",
        }
    }
    pub fn advance(
        &mut self,
        plant: &Plant,
        terrain: &impl Terrain,
    ) -> Result<Option<WorldEditTransaction>> {
        if let Some(fixture) = self.fixture {
            if matches!(self.phase, Phase::Done) {
                return Ok(None);
            }
            self.ticks += 1;
            if self.ticks >= 180 {
                let height = plant
                    .anchors
                    .iter()
                    .map(|a| a.position.y)
                    .fold(0.0f32, f32::max);
                let collision_clear = plant.nodes.iter().all(|n| {
                    n.parent.is_none_or(|p| {
                        crate::climbing_plants::clear_segment(
                            terrain,
                            plant.nodes[p].position,
                            n.position,
                            plant.radius,
                        ) == Some(true)
                    })
                });
                ensure!(height >= 262.0 && collision_clear && plant.nodes.iter().all(|n| n.position.is_finite()),
                    "climbing {} fixture failed: attached_height={height} collision_clear={collision_clear} nodes={}", fixture.name(), plant.nodes.len());
                ensure!(
                    plant
                        .anchors
                        .iter()
                        .all(|a| terrain.voxel(a.cell) == Some(a.material)),
                    "attachment not backed by authoritative terrain"
                );
                log::info!("[CLIMBING][REVIEW] fixture={} verified=true collision_clear=true attached_height={height:.3} nodes={} anchors={}", fixture.name(), plant.nodes.len(), plant.anchors.len());
                self.phase = Phase::Done;
            }
            return Ok(None);
        }
        match self.phase {
            Phase::Grow if plant.nodes.len() >= 100 => {
                // Cut below the first fork: all downstream branches must disappear,
                // despite their still-valid upper attachments. Other-branch isolation
                // is covered independently in the core tests.
                let c = plant.anchors[1].cell.as_uvec3();
                let min = c - UVec3::new(4, 4, 3);
                let max = c + UVec3::new(5, 5, 1);
                self.before = Some(plant.clone());
                self.hole = Some((min, max));
                self.phase = Phase::Cut;
                log::info!("[CLIMBING][REVIEW] remove wall under lower attachment bounds={min:?}..{max:?} nodes={}", plant.nodes.len());
                return Ok(Some(wall_edit(min, max, VOXEL_TYPE_EMPTY)?));
            }
            Phase::Cut if plant.nodes.len() < self.before.as_ref().unwrap().nodes.len() => {
                let before = self.before.as_ref().unwrap();
                check_survivors(before, plant)?;
                ensure!(
                    plant.tips.len() == 1 && plant.regrowth_nodes().count() == 1,
                    "cut must leave one frontier bud"
                );
                ensure!(
                    before.anchors.iter().skip(2).all(|anchor| {
                        !plant
                            .nodes
                            .iter()
                            .any(|node| node.id == before.nodes[anchor.node].id)
                    }),
                    "upper attached branches were retained"
                );
                log::info!("[CLIMBING][REVIEW] pruned=true upper_attached_removed=true stable_survivors=true before={} after={}", before.nodes.len(), plant.nodes.len());
                self.stump = Some(plant.clone());
                self.ticks = 0;
                self.phase = Phase::Wait;
            }
            Phase::Wait | Phase::RootWait => {
                ensure!(
                    Some(plant) == self.stump.as_ref(),
                    "vine grew across the missing wall or lost its waiting state"
                );
                self.ticks += 1;
                if self.ticks >= 30 {
                    let root = matches!(self.phase, Phase::RootWait);
                    log::info!(
                        "[CLIMBING][REVIEW] waiting=true root={root} frames={} nodes={}",
                        self.ticks,
                        plant.nodes.len()
                    );
                    self.phase = if root {
                        Phase::RootRepair
                    } else {
                        Phase::Repair
                    };
                    let (min, max) = if root {
                        (UVec3::new(224, 192, 300), UVec3::new(288, 300, 306))
                    } else {
                        self.hole.unwrap()
                    };
                    return Ok(Some(wall_edit(min, max, VOXEL_TYPE_LIMESTONE)?));
                }
            }
            Phase::Repair if plant.nodes.len() >= self.stump.as_ref().unwrap().nodes.len() + 20 => {
                let stump = self.stump.as_ref().unwrap();
                ensure!(
                    plant.nodes.starts_with(&stump.nodes),
                    "repair changed the retained stem"
                );
                let old_max = self
                    .before
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .map(|n| n.id)
                    .max()
                    .unwrap();
                ensure!(
                    plant
                        .nodes
                        .iter()
                        .skip(stump.nodes.len())
                        .all(|n| n.id > old_max),
                    "regrowth reused deleted IDs"
                );
                ensure!(
                    plant.nodes[stump.nodes.len()].parent == Some(stump.tips[0].node),
                    "growth did not start at the cut"
                );
                log::info!(
                    "[CLIMBING][REVIEW] regrown=true from_cut=true fresh_ids=true nodes={}",
                    plant.nodes.len()
                );
                self.before = Some(plant.clone());
                self.phase = Phase::RootCut;
                return Ok(Some(wall_edit(
                    UVec3::new(224, 192, 300),
                    UVec3::new(288, 300, 306),
                    VOXEL_TYPE_EMPTY,
                )?));
            }
            Phase::RootCut if plant.nodes.len() == 1 => {
                ensure!(
                    plant.root_connected() && !plant.anchors[0].attached,
                    "missing root support must retain only a latent seed"
                );
                check_survivors(self.before.as_ref().unwrap(), plant)?;
                self.stump = Some(plant.clone());
                self.ticks = 0;
                self.phase = Phase::RootWait;
            }
            Phase::RootRepair if plant.nodes.len() >= 24 => {
                ensure!(
                    plant.nodes.starts_with(&self.stump.as_ref().unwrap().nodes),
                    "root repair moved the root"
                );
                log::info!("[CLIMBING][REVIEW] verified prune=true wait=true regrow=true root_recovery=true finite={} nodes={}", plant.nodes.iter().all(|n| n.position.is_finite()), plant.nodes.len());
                self.phase = Phase::Done;
            }
            _ => {}
        }
        Ok(None)
    }
}
fn check_survivors(before: &Plant, after: &Plant) -> Result<()> {
    for node in &after.nodes {
        let original = before
            .nodes
            .iter()
            .find(|n| n.id == node.id)
            .ok_or_else(|| anyhow::anyhow!("pruning generated a new stem"))?;
        ensure!(
            node.position == original.position
                && node.rest_length == original.rest_length
                && node.parent.map(|p| after.nodes[p].id)
                    == original.parent.map(|p| before.nodes[p].id),
            "pruning changed an unrelated stem"
        );
    }
    Ok(())
}
