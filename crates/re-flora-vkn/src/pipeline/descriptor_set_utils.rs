use crate::{
    DescriptorPool, DescriptorSetLayoutBinding, PipelineLayout, ResourceContainer,
};
use anyhow::Result;
use std::{collections::HashMap, sync::Mutex};

use super::{DescriptorBindingPlan, DescriptorResource, DescriptorSetGeneration};

pub fn allocate_descriptor_sets(
    descriptor_pool: &DescriptorPool,
    pipeline_layout: &PipelineLayout,
    descriptor_sets_bindings: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
) -> Result<DescriptorSetGeneration> {
    let mut descriptor_sets = DescriptorSetGeneration::empty();
    let mut sorted_sets: Vec<_> = descriptor_sets_bindings.iter().collect();
    sorted_sets.sort_by_key(|(set_no, _)| *set_no);

    // allocate descriptor sets from the pool
    for (set_no, _) in sorted_sets {
        let layout = pipeline_layout
            .get_descriptor_set_layouts()
            .get(set_no)
            .ok_or_else(|| anyhow::anyhow!("missing descriptor set layout {set_no}"))?;
        let descriptor_set = descriptor_pool.allocate_set(layout)?;
        descriptor_sets.insert(*set_no, descriptor_set);
    }
    Ok(descriptor_sets)
}

/// Fully initializes a descriptor generation from resource containers.
///
/// Unlike the legacy automatic path, every reflected binding must resolve.  Pipelines with
/// resources whose lifetime starts after construction must use their semantic initialization
/// methods instead of relying on an implicit name prefix.
pub fn initialize_descriptor_sets(
    resource_containers: &[&dyn ResourceContainer],
    descriptor_binding_plan: &DescriptorBindingPlan,
    descriptor_sets_storage: &Mutex<DescriptorSetGeneration>,
) -> Result<()> {
    let descriptor_sets = descriptor_sets_storage.lock().unwrap();
    initialize_descriptor_sets_on_sets(resource_containers, descriptor_binding_plan, &descriptor_sets)
}

/// Initializes one reflected descriptor set during a two-phase pipeline construction.
///
/// This is useful when a pipeline's other sets are supplied by a later frame/draw lifetime. The
/// selected set is still complete: every binding in that set must resolve.
pub fn initialize_descriptor_set(
    binding_name: &str,
    resource_containers: &[&dyn ResourceContainer],
    descriptor_binding_plan: &DescriptorBindingPlan,
    descriptor_sets_storage: &Mutex<DescriptorSetGeneration>,
) -> Result<()> {
    let set_no = descriptor_binding_plan.binding(binding_name)?.set_no();
    let descriptor_sets = descriptor_sets_storage.lock().unwrap();
    initialize_descriptor_set_on_set(
        set_no,
        resource_containers,
        descriptor_binding_plan,
        &descriptor_sets,
    )
}

fn initialize_descriptor_sets_on_sets(
    resource_containers: &[&dyn ResourceContainer],
    descriptor_binding_plan: &DescriptorBindingPlan,
    descriptor_sets: &DescriptorSetGeneration,
) -> Result<()> {
    for set_no in descriptor_binding_plan.set_numbers() {
        initialize_descriptor_set_on_set(
            *set_no,
            resource_containers,
            descriptor_binding_plan,
            descriptor_sets,
        )?;
    }
    Ok(())
}

fn initialize_descriptor_set_on_set(
    set_no: u32,
    resource_containers: &[&dyn ResourceContainer],
    descriptor_binding_plan: &DescriptorBindingPlan,
    descriptor_sets: &DescriptorSetGeneration,
) -> Result<()> {
    let descriptor_set = descriptor_sets
        .get(&set_no)
        .ok_or_else(|| anyhow::anyhow!("missing allocated descriptor set {set_no}"))?;

    let binding_names = descriptor_binding_plan
        .bindings_for_set(set_no)?
        .iter()
        .map(|binding| binding.name().to_owned())
        .collect::<Vec<_>>();
    for binding_name in binding_names {
        let mut buffers = Vec::new();
        let mut textures = Vec::new();
        for container in resource_containers {
            if let Some(buffer) = container.get_buffer(&binding_name) {
                buffers.push(buffer);
            }
            if let Some(texture) = container.get_texture(&binding_name) {
                textures.push(texture);
            }
        }

        anyhow::ensure!(
            buffers.len() + textures.len() == 1,
            "descriptor resource '{}' must have exactly one provider during initialization of {} (found {} buffers and {} textures)",
            binding_name,
            descriptor_binding_plan.pipeline_name(),
            buffers.len(),
            textures.len(),
        );

        let resource = if let Some(buffer) = buffers.first() {
            DescriptorResource::Buffer(buffer)
        } else {
            DescriptorResource::Texture(
                textures
                    .first()
                    .expect("descriptor provider count was validated above"),
            )
        };
        let mut write = descriptor_binding_plan.make_write(&binding_name, resource)?;
        descriptor_set.perform_writes(std::slice::from_mut(&mut write));
    }
    Ok(())
}
