use super::{
    descriptor_binding_plan::{DescriptorBindingPlan, DescriptorRuntimeIdentity},
    transient_descriptor_sets::TransientDescriptorSets,
    DescriptorResource, DescriptorUpdate, PreparedDescriptorGeneration,
};
use crate::{
    CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, FrameRetirement,
    PipelineLayout, ResourceContainer,
};
use anyhow::{Context, Result};
use ash::vk;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Owns the reflected descriptor lifecycle shared by compute and graphics pipelines.
///
/// Numeric Vulkan locations, active-generation residency, provider resolution,
/// completeness checks, transient allocation, and bind-run construction stay behind this module.
/// Pipeline modules remain the concrete adapters that encode compute or graphics bind commands.
pub(super) struct ReflectedDescriptorRuntime {
    descriptor_pool: DescriptorPool,
    pipeline_layout: PipelineLayout,
    state: Mutex<DescriptorRuntimeState>,
    transient: Mutex<TransientDescriptorSets>,
    plan: DescriptorBindingPlan,
    identity: Arc<DescriptorRuntimeIdentity>,
}

struct DescriptorRuntimeState {
    active: PreparedDescriptorGeneration,
    creation_open: bool,
}

pub(super) struct PreparedTransientDescriptorSet {
    identity: Arc<DescriptorRuntimeIdentity>,
    set_no: u32,
    descriptor_set: DescriptorSet,
}

impl PreparedTransientDescriptorSet {
    pub(super) fn set_no(&self) -> u32 {
        self.set_no
    }

    pub(super) fn as_raw(&self) -> vk::DescriptorSet {
        self.descriptor_set.as_raw()
    }

    pub(super) fn belongs_to(&self, identity: &Arc<DescriptorRuntimeIdentity>) -> bool {
        Arc::ptr_eq(&self.identity, identity)
    }
}

impl ReflectedDescriptorRuntime {
    pub(super) fn from_reflection(
        pipeline_name: impl Into<String>,
        descriptor_pool: &DescriptorPool,
        pipeline_layout: &PipelineLayout,
        reflected: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    ) -> Result<Self> {
        let plan = DescriptorBindingPlan::from_reflection(pipeline_name, reflected)?;
        let identity = Arc::new(DescriptorRuntimeIdentity);
        let active = allocate_descriptor_sets(
            descriptor_pool,
            pipeline_layout,
            reflected,
            identity.clone(),
        )?;

        Ok(Self {
            descriptor_pool: descriptor_pool.clone(),
            pipeline_layout: pipeline_layout.clone(),
            state: Mutex::new(DescriptorRuntimeState {
                active,
                creation_open: true,
            }),
            transient: Mutex::new(TransientDescriptorSets::default()),
            plan,
            identity,
        })
    }

    /// Completes descriptor initialization while the pipeline is still under construction.
    /// The first prepare, publish, or recording operation seals this creation-only interface.
    pub(super) fn initialize(&self, update: DescriptorUpdate<'_>) -> Result<()> {
        let state = self.state.lock().unwrap();
        anyhow::ensure!(
            state.creation_open,
            "creation-time descriptor initialization is closed for {}",
            self.plan.pipeline_name(),
        );
        let touched_set_nos = self.apply_update(&state.active, update)?;
        self.plan
            .assert_generation_complete_for_sets(&state.active, &touched_set_nos);
        Ok(())
    }

    /// Forks the active generation, applies one complete semantic update, and validates it.
    pub(super) fn prepare(
        &self,
        update: DescriptorUpdate<'_>,
    ) -> Result<PreparedDescriptorGeneration> {
        let (pending, mut required_set_nos) = {
            let mut state = self.state.lock().unwrap();
            state.creation_open = false;
            (
                self.fork_generation(&state.active)?,
                self.complete_set_numbers(&state.active),
            )
        };

        required_set_nos.extend(self.apply_update(&pending, update)?);
        required_set_nos.sort_unstable();
        required_set_nos.dedup();
        self.plan
            .assert_generation_complete_for_sets(&pending, &required_set_nos);
        Ok(pending)
    }

    pub(super) fn publish(
        &self,
        name: &'static str,
        generation: u64,
        update: DescriptorUpdate<'_>,
    ) -> Result<FrameRetirement> {
        let pending = self.prepare(update)?;
        Ok(self.publish_prepared(name, generation, pending))
    }

    pub(super) fn publish_prepared(
        &self,
        name: &'static str,
        generation: u64,
        pending: PreparedDescriptorGeneration,
    ) -> FrameRetirement {
        assert!(
            pending.belongs_to(&self.identity),
            "prepared descriptor generation belongs to another pipeline; target={}",
            self.plan.pipeline_name(),
        );
        let mut state = self.state.lock().unwrap();
        state.creation_open = false;
        let old = std::mem::replace(&mut state.active, pending);
        FrameRetirement::new(name, generation, old)
    }

    pub(super) fn begin_transient_frame(&self, frame_slot: usize) {
        self.transient.lock().unwrap().begin_frame(frame_slot);
    }

    pub(super) fn prepare_transient_set(
        &self,
        cmdbuf: &CommandBuffer,
        descriptors: &[(&str, DescriptorResource<'_>)],
    ) -> Result<PreparedTransientDescriptorSet> {
        self.seal_creation();
        let first_name = descriptors
            .first()
            .ok_or_else(|| {
                anyhow::anyhow!("transient descriptor set requires at least one resource")
            })?
            .0;
        let set_no = self.plan.binding(first_name)?.set_no();
        let reflected_bindings = self.plan.bindings_for_set(set_no)?;
        anyhow::ensure!(
            descriptors.len() == reflected_bindings.len(),
            "transient descriptor set {} for {} requires exactly {} reflected resources, got {}",
            set_no,
            self.plan.pipeline_name(),
            reflected_bindings.len(),
            descriptors.len(),
        );
        for reflected in reflected_bindings {
            let count = descriptors
                .iter()
                .filter(|(name, _)| *name == reflected.name())
                .count();
            anyhow::ensure!(
                count == 1,
                "transient descriptor set {} for {} requires exactly one resource named '{}', got {}",
                set_no,
                self.plan.pipeline_name(),
                reflected.name(),
                count,
            );
        }

        let layout = self
            .pipeline_layout
            .get_descriptor_set_layouts()
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("descriptor set {set_no} is not reflected"))?;
        let descriptor_set = self.transient.lock().unwrap().next_descriptor_set(
            set_no,
            &self.descriptor_pool,
            layout,
            self.plan.pipeline_name(),
        )?;
        let mut writes = Vec::with_capacity(descriptors.len());
        for (name, resource) in descriptors {
            let binding = self.plan.binding(name)?;
            anyhow::ensure!(
                binding.set_no() == set_no,
                "transient descriptor resources must use one descriptor set; '{}' is in set {} but '{}' is in set {}",
                first_name,
                set_no,
                name,
                binding.set_no(),
            );
            writes.push(self.plan.make_write(name, *resource)?);
        }
        descriptor_set.perform_writes(&mut writes);
        descriptor_set.record_resource_uses(cmdbuf);
        Ok(PreparedTransientDescriptorSet {
            identity: self.identity.clone(),
            set_no,
            descriptor_set,
        })
    }

    pub(super) fn bind_prepared_transient(
        &self,
        prepared: &PreparedTransientDescriptorSet,
        bind: impl FnOnce(u32, vk::DescriptorSet),
    ) {
        assert!(
            prepared.belongs_to(&self.identity),
            "prepared transient descriptor set belongs to another pipeline; target={}",
            self.plan.pipeline_name(),
        );
        bind(prepared.set_no(), prepared.as_raw());
    }

    pub(super) fn allocate_standalone_descriptor(
        &self,
        name: &str,
        resource: DescriptorResource<'_>,
    ) -> Result<DescriptorSet> {
        self.seal_creation();
        let binding = self.plan.binding(name)?;
        let reflected_bindings = self.plan.bindings_for_set(binding.set_no())?;
        anyhow::ensure!(
            reflected_bindings.len() == 1,
            "standalone transient descriptor '{}' requires its reflected set {} to contain exactly one binding",
            name,
            binding.set_no(),
        );
        let layout = self
            .pipeline_layout
            .get_descriptor_set_layouts()
            .get(&binding.set_no())
            .ok_or_else(|| {
                anyhow::anyhow!("descriptor set {} is not reflected", binding.set_no())
            })?;
        let descriptor_set = self.descriptor_pool.allocate_set(layout)?;
        let mut write = self.plan.make_write(name, resource)?;
        descriptor_set.perform_writes(std::slice::from_mut(&mut write));
        Ok(descriptor_set)
    }

    pub(super) fn bind_standalone_descriptor(
        &self,
        name: &str,
        descriptor_set: &DescriptorSet,
        bind: impl FnOnce(u32, vk::DescriptorSet),
    ) -> Result<()> {
        self.seal_creation();
        let set_no = self.plan.binding(name)?.set_no();
        bind(set_no, descriptor_set.as_raw());
        Ok(())
    }

    pub(super) fn record_active_resource_uses(&self, cmdbuf: &CommandBuffer) {
        let mut state = self.state.lock().unwrap();
        state.creation_open = false;
        for descriptor_set in state.active.values() {
            descriptor_set.record_resource_uses(cmdbuf);
        }
    }

    pub(super) fn record_standalone_resource_uses(
        &self,
        cmdbuf: &CommandBuffer,
        descriptor_set: &DescriptorSet,
    ) {
        self.seal_creation();
        descriptor_set.record_resource_uses(cmdbuf);
    }

    pub(super) fn tracked_texture_binding_count(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .active
            .values()
            .map(DescriptorSet::image_owner_count)
            .sum()
    }

    pub(super) fn bind_active(
        &self,
        excluded_set_no: Option<u32>,
        bind_run: impl FnMut(u32, &[vk::DescriptorSet]),
    ) {
        let mut state = self.state.lock().unwrap();
        state.creation_open = false;
        self.plan
            .assert_generation_complete_except(&state.active, excluded_set_no);

        let descriptor_sets = state
            .active
            .keys()
            .copied()
            .filter(|set_no| Some(*set_no) != excluded_set_no)
            .map(|set_no| {
                (
                    set_no,
                    state
                        .active
                        .get(&set_no)
                        .expect("descriptor set location was collected from the generation")
                        .as_raw(),
                )
            })
            .collect::<Vec<_>>();
        for_each_contiguous_bind_run(descriptor_sets, bind_run);
    }

    fn write_set_from_resources(
        &self,
        set_no: u32,
        resource_containers: &[&dyn ResourceContainer],
        descriptor_sets: &PreparedDescriptorGeneration,
    ) -> Result<()> {
        let binding_names = self
            .plan
            .bindings_for_set(set_no)?
            .iter()
            .map(|binding| binding.name().to_owned())
            .collect::<Vec<_>>();
        for binding_name in binding_names {
            let resource = self
                .plan
                .resolve_resource(&binding_name, resource_containers)
                .with_context(|| {
                    format!(
                        "failed to resolve descriptor resource during initialization of {}",
                        self.plan.pipeline_name()
                    )
                })?;
            self.write_descriptor(descriptor_sets, &binding_name, resource)?;
        }
        Ok(())
    }

    fn apply_update(
        &self,
        descriptor_sets: &PreparedDescriptorGeneration,
        update: DescriptorUpdate<'_>,
    ) -> Result<Vec<u32>> {
        let mut touched_set_nos = Vec::new();
        match update {
            DescriptorUpdate::All(resource_containers) => {
                for set_no in self.plan.set_numbers() {
                    self.write_set_from_resources(*set_no, resource_containers, descriptor_sets)?;
                    touched_set_nos.push(*set_no);
                }
            }
            DescriptorUpdate::SetContaining { anchor, providers } => {
                let set_no = self.plan.binding(anchor)?.set_no();
                self.write_set_from_resources(set_no, providers, descriptor_sets)?;
                touched_set_nos.push(set_no);
            }
            DescriptorUpdate::Named(writes) => {
                for write in writes {
                    let set_no = self.plan.binding(write.name)?.set_no();
                    self.write_descriptor(descriptor_sets, write.name, write.resource)?;
                    touched_set_nos.push(set_no);
                }
            }
        }
        touched_set_nos.sort_unstable();
        touched_set_nos.dedup();
        Ok(touched_set_nos)
    }

    fn write_descriptor(
        &self,
        descriptor_sets: &PreparedDescriptorGeneration,
        name: &str,
        resource: DescriptorResource<'_>,
    ) -> Result<()> {
        let set_no = self.plan.binding(name)?.set_no();
        let mut write = self.plan.make_write(name, resource)?;
        self.plan.validate_write(set_no, &write)?;
        descriptor_sets
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("missing allocated descriptor set {set_no}"))?
            .perform_writes(std::slice::from_mut(&mut write));
        Ok(())
    }

    fn fork_generation(
        &self,
        active: &PreparedDescriptorGeneration,
    ) -> Result<PreparedDescriptorGeneration> {
        let mut pending = PreparedDescriptorGeneration::empty(self.identity.clone());
        for set_no in self.plan.set_numbers() {
            let set = active.get(set_no).ok_or_else(|| {
                anyhow::anyhow!(
                    "descriptor generation for {} is missing reflected set {}",
                    self.plan.pipeline_name(),
                    set_no
                )
            })?;
            let layout = self
                .pipeline_layout
                .get_descriptor_set_layouts()
                .get(set_no)
                .ok_or_else(|| {
                    anyhow::anyhow!("descriptor set layout {set_no} is not reflected")
                })?;
            pending.insert(*set_no, set.fork(&self.descriptor_pool, layout)?);
        }
        Ok(pending)
    }

    fn complete_set_numbers(&self, generation: &PreparedDescriptorGeneration) -> Vec<u32> {
        self.plan
            .set_numbers()
            .iter()
            .copied()
            .filter(|set_no| {
                let binding_numbers = self
                    .plan
                    .bindings_for_set(*set_no)
                    .expect("descriptor plan set list must be internally consistent")
                    .iter()
                    .map(|binding| binding.binding_no())
                    .collect::<Vec<_>>();
                generation
                    .get(set_no)
                    .is_some_and(|set| set.has_bindings(&binding_numbers))
            })
            .collect()
    }

    fn seal_creation(&self) {
        self.state.lock().unwrap().creation_open = false;
    }
}

fn for_each_contiguous_bind_run(
    mut descriptor_sets: Vec<(u32, vk::DescriptorSet)>,
    mut bind_run: impl FnMut(u32, &[vk::DescriptorSet]),
) {
    descriptor_sets.sort_unstable_by_key(|(set_no, _)| *set_no);
    let mut run = Vec::new();
    let mut run_start = None;
    for (set_no, descriptor_set) in descriptor_sets {
        if let Some(start) = run_start {
            if set_no != start + run.len() as u32 {
                bind_run(start, &run);
                run.clear();
                run_start = Some(set_no);
            }
        } else {
            run_start = Some(set_no);
        }
        run.push(descriptor_set);
    }
    if let Some(start) = run_start {
        bind_run(start, &run);
    }
}

fn allocate_descriptor_sets(
    descriptor_pool: &DescriptorPool,
    pipeline_layout: &PipelineLayout,
    reflected: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    identity: Arc<DescriptorRuntimeIdentity>,
) -> Result<PreparedDescriptorGeneration> {
    let mut descriptor_sets = PreparedDescriptorGeneration::empty(identity);
    let mut sorted_sets = reflected.keys().copied().collect::<Vec<_>>();
    sorted_sets.sort_unstable();
    for set_no in sorted_sets {
        let layout = pipeline_layout
            .get_descriptor_set_layouts()
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("missing descriptor set layout {set_no}"))?;
        descriptor_sets.insert(set_no, descriptor_pool.allocate_set(layout)?);
    }
    Ok(descriptor_sets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;

    #[test]
    fn sparse_descriptor_sets_form_minimal_contiguous_bind_runs() {
        let descriptor_sets = vec![
            (3, vk::DescriptorSet::from_raw(103)),
            (1, vk::DescriptorSet::from_raw(101)),
            (6, vk::DescriptorSet::from_raw(106)),
            (0, vk::DescriptorSet::from_raw(100)),
        ];
        let mut runs = Vec::new();
        for_each_contiguous_bind_run(descriptor_sets, |first_set, sets| {
            runs.push((
                first_set,
                sets.iter().map(|set| set.as_raw()).collect::<Vec<_>>(),
            ));
        });

        assert_eq!(
            runs,
            vec![(0, vec![100, 101]), (3, vec![103]), (6, vec![106])]
        );
    }
}
