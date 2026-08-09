use crate::{
    DescriptorResource, DescriptorSetLayoutBinding, ResourceContainer, ResourceLookup,
    TextureLayout, WriteDescriptorSet,
};
use anyhow::{anyhow, ensure, Result};
use ash::vk;
use std::{collections::HashMap, sync::Arc};

#[derive(Debug)]
pub(crate) struct DescriptorRuntimeIdentity;

/// An opaque, prepared descriptor generation owned by a pipeline.
///
/// The numeric set locations stay inside VKN. Application code may move a prepared generation
/// across a transaction boundary, but cannot construct or mutate its numeric bindings directly.
pub struct PreparedDescriptorGeneration {
    identity: Arc<DescriptorRuntimeIdentity>,
    sets: HashMap<u32, crate::DescriptorSet>,
}

impl PreparedDescriptorGeneration {
    pub(crate) fn empty(identity: Arc<DescriptorRuntimeIdentity>) -> Self {
        Self {
            identity,
            sets: HashMap::new(),
        }
    }

    pub(crate) fn belongs_to(&self, identity: &Arc<DescriptorRuntimeIdentity>) -> bool {
        Arc::ptr_eq(&self.identity, identity)
    }

    pub(crate) fn insert(&mut self, set_no: u32, descriptor_set: crate::DescriptorSet) {
        self.sets.insert(set_no, descriptor_set);
    }

    pub(crate) fn get(&self, set_no: &u32) -> Option<&crate::DescriptorSet> {
        self.sets.get(set_no)
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &u32> {
        self.sets.keys()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &crate::DescriptorSet> {
        self.sets.values()
    }
}

/// One semantic resource write in a descriptor update.
#[derive(Clone, Copy)]
pub struct DescriptorWrite<'a> {
    pub name: &'a str,
    pub resource: DescriptorResource<'a>,
}

/// A complete semantic descriptor update. Numeric set and binding locations remain private.
#[derive(Clone, Copy)]
pub enum DescriptorUpdate<'a> {
    All(&'a [&'a dyn ResourceContainer]),
    SetContaining {
        anchor: &'a str,
        providers: &'a [&'a dyn ResourceContainer],
    },
    Named(&'a [DescriptorWrite<'a>]),
}

/// One descriptor binding as declared by shader reflection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DescriptorBinding {
    name: String,
    set_no: u32,
    binding_no: u32,
    descriptor_type: vk::DescriptorType,
    descriptor_count: u32,
}

impl DescriptorBinding {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn set_no(&self) -> u32 {
        self.set_no
    }

    pub(crate) fn binding_no(&self) -> u32 {
        self.binding_no
    }
}

/// The semantic descriptor interface for one compute or graphics pipeline.
///
/// Reflection is converted once at pipeline construction time.  Application code
/// addresses resources by their stable shader name; this plan owns the numeric
/// Vulkan location and validates writes before they reach `vkUpdateDescriptorSets`.
#[derive(Clone, Debug)]
pub(crate) struct DescriptorBindingPlan {
    pipeline_name: String,
    bindings_by_name: HashMap<String, DescriptorBinding>,
    bindings_by_location: HashMap<(u32, u32), DescriptorBinding>,
    bindings_by_set: HashMap<u32, Vec<DescriptorBinding>>,
    set_numbers: Vec<u32>,
}

impl DescriptorBindingPlan {
    pub(crate) fn from_reflection(
        pipeline_name: impl Into<String>,
        reflected: &HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>>,
    ) -> Result<Self> {
        let pipeline_name = pipeline_name.into();
        let mut bindings_by_name = HashMap::<String, DescriptorBinding>::new();
        let mut bindings_by_location = HashMap::<(u32, u32), DescriptorBinding>::new();
        let mut bindings_by_set = HashMap::<u32, Vec<DescriptorBinding>>::new();

        let mut set_numbers = reflected.keys().copied().collect::<Vec<_>>();
        set_numbers.sort_unstable();

        for set_no in &set_numbers {
            let bindings = reflected.get(set_no).ok_or_else(|| {
                anyhow!(
                    "descriptor plan for {pipeline_name} lost reflected descriptor set {set_no}"
                )
            })?;
            let mut binding_numbers = bindings.keys().copied().collect::<Vec<_>>();
            binding_numbers.sort_unstable();

            let set_bindings = bindings_by_set.entry(*set_no).or_default();
            for binding_no in binding_numbers {
                let binding = bindings.get(&binding_no).ok_or_else(|| {
                    anyhow!(
                        "descriptor plan for {pipeline_name} lost reflected binding {set_no}:{binding_no}"
                    )
                })?;
                ensure!(
                    binding.no == binding_no,
                    "descriptor plan for {pipeline_name} has inconsistent reflected binding key {}:{} (binding metadata says {})",
                    set_no,
                    binding_no,
                    binding.no,
                );
                ensure!(
                    !binding.name.is_empty(),
                    "descriptor plan for {pipeline_name} has an unnamed reflected binding at {}:{}",
                    set_no,
                    binding_no,
                );
                ensure!(
                    binding.descriptor_count == 1,
                    "descriptor plan for {pipeline_name} does not support descriptor arrays: {} at {}:{} has count {}",
                    binding.name,
                    set_no,
                    binding_no,
                    binding.descriptor_count,
                );

                let semantic_binding = DescriptorBinding {
                    name: binding.name.clone(),
                    set_no: *set_no,
                    binding_no,
                    descriptor_type: binding.descriptor_type,
                    descriptor_count: binding.descriptor_count,
                };

                if let Some(previous) = bindings_by_name.get(&semantic_binding.name) {
                    ensure!(
                        previous.set_no == semantic_binding.set_no
                            && previous.binding_no == semantic_binding.binding_no,
                        "descriptor resource name '{}' is declared at both {}:{} and {}:{} in {}",
                        semantic_binding.name,
                        previous.set_no,
                        previous.binding_no,
                        semantic_binding.set_no,
                        semantic_binding.binding_no,
                        pipeline_name,
                    );
                    ensure!(
                        previous.descriptor_type == semantic_binding.descriptor_type
                            && previous.descriptor_count == semantic_binding.descriptor_count,
                        "descriptor resource '{}' has incompatible declarations in {}",
                        semantic_binding.name,
                        pipeline_name,
                    );
                }

                if let Some(previous) = bindings_by_location
                    .get(&(semantic_binding.set_no, semantic_binding.binding_no))
                {
                    ensure!(
                        previous.name == semantic_binding.name
                            && previous.descriptor_type == semantic_binding.descriptor_type
                            && previous.descriptor_count == semantic_binding.descriptor_count,
                        "descriptor location {}:{} has incompatible declarations in {}",
                        semantic_binding.set_no,
                        semantic_binding.binding_no,
                        pipeline_name,
                    );
                }

                bindings_by_name
                    .entry(semantic_binding.name.clone())
                    .or_insert_with(|| semantic_binding.clone());
                bindings_by_location
                    .entry((semantic_binding.set_no, semantic_binding.binding_no))
                    .or_insert_with(|| semantic_binding.clone());
                set_bindings.push(semantic_binding);
            }
        }

        Ok(Self {
            pipeline_name,
            bindings_by_name,
            bindings_by_location,
            bindings_by_set,
            set_numbers,
        })
    }

    pub(crate) fn pipeline_name(&self) -> &str {
        &self.pipeline_name
    }

    pub(crate) fn binding(&self, name: &str) -> Result<&DescriptorBinding> {
        self.bindings_by_name.get(name).ok_or_else(|| {
            anyhow!(
                "descriptor resource '{}' is not reflected by {}",
                name,
                self.pipeline_name
            )
        })
    }

    pub(crate) fn binding_at(&self, set_no: u32, binding_no: u32) -> Result<&DescriptorBinding> {
        self.bindings_by_location
            .get(&(set_no, binding_no))
            .ok_or_else(|| {
                anyhow!(
                    "descriptor location {}:{} is not reflected by {}",
                    set_no,
                    binding_no,
                    self.pipeline_name
                )
            })
    }

    pub(crate) fn bindings_for_set(&self, set_no: u32) -> Result<&[DescriptorBinding]> {
        self.bindings_by_set
            .get(&set_no)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                anyhow!(
                    "descriptor set {} is not reflected by {}",
                    set_no,
                    self.pipeline_name
                )
            })
    }

    pub(crate) fn set_numbers(&self) -> &[u32] {
        &self.set_numbers
    }

    pub(crate) fn assert_generation_complete_except(
        &self,
        generation: &PreparedDescriptorGeneration,
        excluded_set_no: Option<u32>,
    ) {
        let set_nos = self
            .set_numbers
            .iter()
            .copied()
            .filter(|set_no| Some(*set_no) != excluded_set_no)
            .collect::<Vec<_>>();
        self.assert_generation_complete_for_sets(generation, &set_nos);
    }

    pub(crate) fn assert_generation_complete_for_sets(
        &self,
        generation: &PreparedDescriptorGeneration,
        set_nos: &[u32],
    ) {
        for set_no in set_nos {
            let set = generation.get(set_no).unwrap_or_else(|| {
                panic!(
                    "descriptor generation for {} is missing reflected set {set_no}",
                    self.pipeline_name
                )
            });
            let binding_numbers = self
                .bindings_for_set(*set_no)
                .expect("descriptor plan set list must be internally consistent")
                .iter()
                .map(|binding| binding.binding_no)
                .collect::<Vec<_>>();
            assert!(
                set.has_bindings(&binding_numbers),
                "descriptor generation for {} has incomplete reflected set {set_no}; initialize every binding before recording",
                self.pipeline_name,
            );
        }
    }

    pub(crate) fn validate_write(&self, set_no: u32, write: &WriteDescriptorSet<'_>) -> Result<()> {
        let binding = self.binding_at(set_no, write.binding())?;
        ensure!(
            write.array_element() < binding.descriptor_count,
            "descriptor write for '{}' in {} uses array element {}, but reflected count is {}",
            binding.name,
            self.pipeline_name,
            write.array_element(),
            binding.descriptor_count,
        );
        ensure!(
            write.descriptor_type() == binding.descriptor_type,
            "descriptor write for '{}' in {} uses {:?}, reflection requires {:?}",
            binding.name,
            self.pipeline_name,
            write.descriptor_type(),
            binding.descriptor_type,
        );
        Ok(())
    }

    pub(crate) fn make_write<'a>(
        &self,
        name: &str,
        resource: DescriptorResource<'a>,
    ) -> Result<WriteDescriptorSet<'a>> {
        let binding = self.binding(name)?;
        match resource {
            DescriptorResource::Buffer(buffer) => {
                let usage = buffer.get_usage().as_raw();
                ensure_buffer_usage(name, binding.descriptor_type, usage)?;
                Ok(WriteDescriptorSet::new_buffer_write_for_type(
                    binding.binding_no,
                    binding.descriptor_type,
                    buffer,
                ))
            }
            DescriptorResource::Texture(texture) => {
                ensure!(
                    matches!(
                        binding.descriptor_type,
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER
                            | vk::DescriptorType::SAMPLED_IMAGE
                            | vk::DescriptorType::STORAGE_IMAGE
                    ),
                    "descriptor resource '{}' in {} expects {:?}, but a texture was supplied",
                    name,
                    self.pipeline_name,
                    binding.descriptor_type,
                );
                Ok(WriteDescriptorSet::new_texture_write(
                    binding.binding_no,
                    binding.descriptor_type,
                    texture,
                    image_layout(binding.descriptor_type),
                ))
            }
            DescriptorResource::AccelerationStructure(accel_struct) => {
                ensure!(
                    binding.descriptor_type == vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                    "descriptor resource '{}' in {} expects {:?}, but an acceleration structure was supplied",
                    name,
                    self.pipeline_name,
                    binding.descriptor_type,
                );
                Ok(WriteDescriptorSet::new_acceleration_structure_write(
                    binding.binding_no,
                    accel_struct,
                ))
            }
        }
    }

    pub(crate) fn resolve_resource<'a>(
        &self,
        name: &str,
        containers: &[&'a dyn ResourceContainer],
    ) -> Result<DescriptorResource<'a>> {
        let lookup = containers.iter().fold(ResourceLookup::Missing, |lookup, container| {
            lookup.merge(container.resolve_resource(name))
        });
        match lookup {
            ResourceLookup::Unique(resource) => Ok(resource),
            ResourceLookup::Missing => Err(anyhow!(
                "descriptor resource '{}' must have exactly one provider for {} (found 0)",
                name,
                self.pipeline_name,
            )),
            ResourceLookup::Ambiguous { providers } => Err(anyhow!(
                "descriptor resource '{}' must have exactly one provider for {} (found {})",
                name,
                self.pipeline_name,
                providers,
            )),
        }
    }
}

pub(crate) fn image_layout(descriptor_type: vk::DescriptorType) -> TextureLayout {
    match descriptor_type {
        vk::DescriptorType::COMBINED_IMAGE_SAMPLER | vk::DescriptorType::SAMPLED_IMAGE => {
            TextureLayout::SHADER_READ_ONLY
        }
        _ => TextureLayout::GENERAL,
    }
}

pub(crate) fn ensure_buffer_usage(
    name: &str,
    descriptor_type: vk::DescriptorType,
    usage: vk::BufferUsageFlags,
) -> Result<()> {
    let required = match descriptor_type {
        vk::DescriptorType::UNIFORM_BUFFER | vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC => {
            vk::BufferUsageFlags::UNIFORM_BUFFER
        }
        vk::DescriptorType::STORAGE_BUFFER | vk::DescriptorType::STORAGE_BUFFER_DYNAMIC => {
            vk::BufferUsageFlags::STORAGE_BUFFER
        }
        vk::DescriptorType::UNIFORM_TEXEL_BUFFER => vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER,
        vk::DescriptorType::STORAGE_TEXEL_BUFFER => vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER,
        _ => {
            return Err(anyhow!(
                "descriptor resource '{}' expects image or acceleration-structure type {:?}, but a buffer was supplied",
                name,
                descriptor_type,
            ));
        }
    };
    ensure!(
        usage.contains(required),
        "buffer for descriptor resource '{}' lacks required {:?} usage for {:?} descriptor",
        name,
        required,
        descriptor_type,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn binding(
        no: u32,
        name: &str,
        descriptor_type: vk::DescriptorType,
    ) -> DescriptorSetLayoutBinding {
        DescriptorSetLayoutBinding {
            no,
            name: name.to_owned(),
            descriptor_type,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::COMPUTE,
        }
    }

    #[test]
    fn plans_binding_zero_and_binding_one_without_dense_set_assumptions() {
        let mut reflected = HashMap::new();
        reflected.insert(
            1,
            HashMap::from([
                (
                    0,
                    binding(0, "instances", vk::DescriptorType::STORAGE_BUFFER),
                ),
                (1, binding(1, "growth", vk::DescriptorType::STORAGE_BUFFER)),
            ]),
        );

        let plan = DescriptorBindingPlan::from_reflection("test", &reflected).unwrap();
        assert_eq!(plan.set_numbers(), &[1]);
        assert_eq!(plan.binding("instances").unwrap().binding_no(), 0);
        assert_eq!(plan.binding("growth").unwrap().binding_no(), 1);

        let mut sparse = HashMap::new();
        sparse.insert(
            3,
            HashMap::from([(7, binding(7, "sparse", vk::DescriptorType::STORAGE_BUFFER))]),
        );
        let sparse_plan = DescriptorBindingPlan::from_reflection("sparse", &sparse).unwrap();
        assert_eq!(sparse_plan.set_numbers(), &[3]);
        assert_eq!(sparse_plan.binding("sparse").unwrap().set_no(), 3);
    }

    #[test]
    fn rejects_duplicate_names_at_different_locations() {
        let mut reflected = HashMap::new();
        reflected.insert(
            0,
            HashMap::from([(0, binding(0, "same", vk::DescriptorType::STORAGE_BUFFER))]),
        );
        reflected.insert(
            2,
            HashMap::from([(4, binding(4, "same", vk::DescriptorType::STORAGE_BUFFER))]),
        );

        let error = DescriptorBindingPlan::from_reflection("duplicate", &reflected)
            .unwrap_err()
            .to_string();
        assert!(error.contains("same"));
        assert!(error.contains("0:0"));
        assert!(error.contains("2:4"));
    }

    #[test]
    fn rejects_descriptor_arrays_at_the_semantic_boundary() {
        let mut reflected = HashMap::new();
        reflected.insert(
            0,
            HashMap::from([(
                0,
                DescriptorSetLayoutBinding {
                    descriptor_count: 2,
                    ..binding(0, "array", vk::DescriptorType::STORAGE_BUFFER)
                },
            )]),
        );

        let error = DescriptorBindingPlan::from_reflection("array", &reflected)
            .unwrap_err()
            .to_string();
        assert!(error.contains("descriptor arrays"));
        assert!(error.contains("array"));
    }

    #[test]
    fn prepared_generation_identity_is_pipeline_local() {
        let owner = Arc::new(DescriptorRuntimeIdentity);
        let other = Arc::new(DescriptorRuntimeIdentity);
        let generation = PreparedDescriptorGeneration::empty(owner.clone());

        assert!(generation.belongs_to(&owner));
        assert!(!generation.belongs_to(&other));
    }

    #[test]
    fn chooses_image_layout_from_reflection_type() {
        assert_eq!(
            image_layout(vk::DescriptorType::SAMPLED_IMAGE),
            TextureLayout::SHADER_READ_ONLY
        );
        assert_eq!(
            image_layout(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            TextureLayout::SHADER_READ_ONLY
        );
        assert_eq!(
            image_layout(vk::DescriptorType::STORAGE_IMAGE),
            TextureLayout::GENERAL
        );
    }
}
