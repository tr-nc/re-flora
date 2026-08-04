use crate::{
    DescriptorPool, DescriptorSetLayoutBinding, PipelineLayout, ResourceContainer,
    WriteDescriptorSet,
};
use anyhow::Result;
use std::{collections::HashMap, sync::Mutex};

use super::{ensure_buffer_usage, image_layout, DescriptorSetGeneration};

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
    descriptor_sets_bindings: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    descriptor_sets_storage: &Mutex<DescriptorSetGeneration>,
) -> Result<()> {
    let descriptor_sets = descriptor_sets_storage.lock().unwrap();
    initialize_descriptor_sets_on_sets(
        resource_containers,
        descriptor_sets_bindings,
        &descriptor_sets,
    )
}

/// Initializes one reflected descriptor set during a two-phase pipeline construction.
///
/// This is useful when a pipeline's other sets are supplied by a later frame/draw lifetime. The
/// selected set is still complete: every binding in that set must resolve.
pub fn initialize_descriptor_set(
    set_no: u32,
    resource_containers: &[&dyn ResourceContainer],
    descriptor_sets_bindings: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    descriptor_sets_storage: &Mutex<DescriptorSetGeneration>,
) -> Result<()> {
    let bindings = descriptor_sets_bindings
        .get(&set_no)
        .ok_or_else(|| anyhow::anyhow!("descriptor set {set_no} is not reflected"))?;
    let selected = HashMap::from([(set_no, bindings.clone())]);
    let descriptor_sets = descriptor_sets_storage.lock().unwrap();
    initialize_descriptor_sets_on_sets(
        resource_containers,
        &selected,
        &descriptor_sets,
    )
}

fn initialize_descriptor_sets_on_sets(
    resource_containers: &[&dyn ResourceContainer],
    descriptor_sets_bindings: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    descriptor_sets: &DescriptorSetGeneration,
) -> Result<()> {
    let mut sorted_sets: Vec<_> = descriptor_sets_bindings.iter().collect();
    sorted_sets.sort_by_key(|(set_no, _)| *set_no);

    for (set_no, bindings) in sorted_sets {
        let descriptor_set = descriptor_sets
            .get(set_no)
            .ok_or_else(|| anyhow::anyhow!("missing allocated descriptor set {set_no}"))?;

        for (_binding_idx, binding) in bindings {
            // find the exact resource for this binding across all resource containers
            let mut found_buffer_containers = Vec::new();
            let mut found_texture_containers = Vec::new();

            for (i, container) in resource_containers.iter().enumerate() {
                if container.get_buffer(&binding.name).is_some() {
                    found_buffer_containers.push(i);
                }
                if container.get_texture(&binding.name).is_some() {
                    found_texture_containers.push(i);
                }
            }

            // ensure that only one resource container has that resource
            let total_found = found_buffer_containers.len() + found_texture_containers.len();
            if total_found == 0 {
                return Err(anyhow::anyhow!("Resource not found: {}", binding.name));
            } else if total_found > 1 {
                return Err(anyhow::anyhow!(
                    "Resource '{}' found in multiple containers: {} buffer containers, {} texture containers",
                    binding.name,
                    found_buffer_containers.len(),
                    found_texture_containers.len()
                ));
            }

            // each resource may be Buffer or Texture, but not both
            if !found_buffer_containers.is_empty() && !found_texture_containers.is_empty() {
                return Err(anyhow::anyhow!(
                    "Resource '{}' found as both Buffer and Texture",
                    binding.name
                ));
            }

            // write the descriptor set based on the found resource
            if let Some(container_idx) = found_buffer_containers.first() {
                let resource = resource_containers[*container_idx]
                    .get_buffer(&binding.name)
                    .unwrap();
                ensure_buffer_usage(
                    &binding.name,
                    binding.descriptor_type,
                    resource.get_usage().as_raw(),
                )?;
                descriptor_set.perform_writes(&mut [WriteDescriptorSet::new_buffer_write_for_type(
                    binding.no,
                    binding.descriptor_type,
                    resource,
                )]);
            } else if let Some(container_idx) = found_texture_containers.first() {
                let resource = resource_containers[*container_idx]
                    .get_texture(&binding.name)
                    .unwrap();
                descriptor_set.perform_writes(&mut [WriteDescriptorSet::new_texture_write(
                    binding.no,
                    binding.descriptor_type,
                    resource,
                    image_layout(binding.descriptor_type),
                )]);
            }
        }
    }
    Ok(())
}
