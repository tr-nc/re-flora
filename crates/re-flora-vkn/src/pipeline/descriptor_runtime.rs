use super::{
    transient_descriptor_sets::TransientDescriptorSets, DescriptorBindingPlan,
    DescriptorGenerationDraft, DescriptorResource, DescriptorRuntimeIdentity,
    DescriptorSetGeneration,
};
use crate::{
    CommandBuffer, DescriptorPool, DescriptorSet, DescriptorSetLayoutBinding, FrameRetirement,
    PipelineLayout, ResourceContainer,
};
use anyhow::{Context, Result};
use ash::vk;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// Owns the reflected descriptor lifecycle shared by compute and graphics pipelines.
///
/// Numeric Vulkan locations, active-generation residency, draft leasing, provider resolution,
/// completeness checks, transient allocation, and bind-run construction stay behind this module.
/// Pipeline modules remain the concrete adapters that encode compute or graphics bind commands.
pub(super) struct ReflectedDescriptorRuntime {
    descriptor_pool: DescriptorPool,
    pipeline_layout: PipelineLayout,
    active: Mutex<DescriptorSetGeneration>,
    transient: Mutex<TransientDescriptorSets>,
    plan: DescriptorBindingPlan,
    identity: Arc<DescriptorRuntimeIdentity>,
    draft_active: Arc<AtomicBool>,
}

pub(super) struct PreparedTransientDescriptorSet {
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
            active: Mutex::new(active),
            transient: Mutex::new(TransientDescriptorSets::default()),
            plan,
            identity,
            draft_active: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn initialize_resources(
        &self,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        let active = self.active.lock().unwrap();
        for set_no in self.plan.set_numbers() {
            self.write_set_from_resources(*set_no, resource_containers, &active)?;
        }
        Ok(())
    }

    pub(super) fn initialize_set_resources(
        &self,
        binding_name: &str,
        resource_containers: &[&dyn ResourceContainer],
    ) -> Result<()> {
        let set_no = self.plan.binding(binding_name)?.set_no();
        let active = self.active.lock().unwrap();
        self.write_set_from_resources(set_no, resource_containers, &active)
    }

    pub(super) fn initialize_descriptor(
        &self,
        name: &str,
        resource: DescriptorResource<'_>,
    ) -> Result<()> {
        let mut write = self.plan.make_write(name, resource)?;
        let set_no = self.plan.binding(name)?.set_no();
        self.active
            .lock()
            .unwrap()
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("descriptor set {set_no} is not reflected"))?
            .perform_writes(std::slice::from_mut(&mut write));
        Ok(())
    }

    pub(super) fn begin_draft(&self) -> Result<DescriptorGenerationDraft> {
        self.draft_active
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| {
                anyhow::anyhow!(
                    "descriptor draft already exists for {}",
                    self.plan.pipeline_name()
                )
            })?;

        let active = self.active.lock().unwrap();
        let required_set_nos = self
            .plan
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
                active
                    .get(set_no)
                    .is_some_and(|set| set.has_bindings(&binding_numbers))
            })
            .collect::<Vec<_>>();

        let result = (|| {
            let mut pending = DescriptorSetGeneration::empty(self.identity.clone());
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
            Ok(DescriptorGenerationDraft::new(
                pending,
                self.plan.clone(),
                self.draft_active.clone(),
                required_set_nos,
            ))
        })();
        if result.is_err() {
            self.draft_active.store(false, Ordering::Release);
        }
        result
    }

    pub(super) fn publish_draft(
        &self,
        name: &'static str,
        generation: u64,
        draft: DescriptorGenerationDraft,
    ) -> FrameRetirement {
        self.publish_generation(name, generation, draft.into_generation())
    }

    pub(super) fn publish_generation(
        &self,
        name: &'static str,
        generation: u64,
        pending: DescriptorSetGeneration,
    ) -> FrameRetirement {
        assert!(
            pending.belongs_to(&self.identity),
            "prepared descriptor generation belongs to another pipeline; target={}",
            self.plan.pipeline_name(),
        );
        let old = std::mem::replace(&mut *self.active.lock().unwrap(), pending);
        FrameRetirement::new(name, generation, old)
    }

    pub(super) fn begin_transient_frame(&self, frame_slot: usize) {
        self.transient.lock().unwrap().begin_frame(frame_slot);
    }

    pub(super) fn prepare_transient_set(
        &self,
        descriptors: &[(&str, DescriptorResource<'_>)],
    ) -> Result<PreparedTransientDescriptorSet> {
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
        Ok(PreparedTransientDescriptorSet {
            set_no,
            descriptor_set,
        })
    }

    pub(super) fn allocate_standalone_descriptor(
        &self,
        name: &str,
        resource: DescriptorResource<'_>,
    ) -> Result<DescriptorSet> {
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
        let set_no = self.plan.binding(name)?.set_no();
        bind(set_no, descriptor_set.as_raw());
        Ok(())
    }

    pub(super) fn record_texture_transitions(&self, cmdbuf: &CommandBuffer) {
        for descriptor_set in self.active.lock().unwrap().values() {
            descriptor_set.record_image_uses(cmdbuf);
        }
    }

    pub(super) fn tracked_texture_binding_count(&self) -> usize {
        self.active
            .lock()
            .unwrap()
            .values()
            .map(DescriptorSet::image_owner_count)
            .sum()
    }

    pub(super) fn bind_active(
        &self,
        excluded_set_no: Option<u32>,
        bind_run: impl FnMut(u32, &[vk::DescriptorSet]),
    ) {
        let active = self.active.lock().unwrap();
        self.plan
            .assert_generation_complete_except(&active, excluded_set_no);

        let descriptor_sets = active
            .keys()
            .copied()
            .filter(|set_no| Some(*set_no) != excluded_set_no)
            .map(|set_no| {
                (
                    set_no,
                    active
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
        descriptor_sets: &DescriptorSetGeneration,
    ) -> Result<()> {
        let descriptor_set = descriptor_sets
            .get(&set_no)
            .ok_or_else(|| anyhow::anyhow!("missing allocated descriptor set {set_no}"))?;
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
            let mut write = self.plan.make_write(&binding_name, resource)?;
            descriptor_set.perform_writes(std::slice::from_mut(&mut write));
        }
        Ok(())
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
) -> Result<DescriptorSetGeneration> {
    let mut descriptor_sets = DescriptorSetGeneration::empty(identity);
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
