use crate::{
    AccelStruct, Buffer, BufferState, CommandBuffer, DescriptorAccess, DescriptorPool,
    DescriptorResource, DescriptorSetLayout, Device, MemoryAccess, PipelineStage, ResourceState,
    Texture, TextureLayout,
};
use anyhow::Result;
use ash::vk;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[allow(dead_code)]
#[derive(Clone)]
enum DescriptorResourceOwner {
    Buffer(Buffer),
    Texture(Texture),
    AccelStruct(AccelStruct),
}

impl DescriptorResourceOwner {
    fn identity(&self) -> DescriptorResourceIdentity {
        match self {
            Self::Buffer(buffer) => DescriptorResourceIdentity::Buffer(buffer.as_raw()),
            Self::Texture(texture) => DescriptorResourceIdentity::Texture {
                image_view: texture.get_image_view().as_raw(),
                sampler: texture.get_sampler().as_raw(),
            },
            Self::AccelStruct(accel_struct) => {
                DescriptorResourceIdentity::AccelerationStructure(accel_struct.as_raw())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DescriptorResourceIdentity {
    Buffer(vk::Buffer),
    Texture {
        image_view: vk::ImageView,
        sampler: vk::Sampler,
    },
    AccelerationStructure(vk::AccelerationStructureKHR),
}

impl DescriptorResourceIdentity {
    fn from_resource(resource: DescriptorResource<'_>) -> Self {
        match resource {
            DescriptorResource::Buffer(buffer) => Self::Buffer(buffer.as_raw()),
            DescriptorResource::Texture(texture) => Self::Texture {
                image_view: texture.get_image_view().as_raw(),
                sampler: texture.get_sampler().as_raw(),
            },
            DescriptorResource::AccelerationStructure(accel_struct) => {
                Self::AccelerationStructure(accel_struct.as_raw())
            }
        }
    }
}

fn descriptor_resource_requires_write(
    current: Option<DescriptorResourceIdentity>,
    requested: DescriptorResourceIdentity,
) -> bool {
    current != Some(requested)
}

#[derive(Clone)]
struct DescriptorBindingOwner {
    descriptor_type: vk::DescriptorType,
    owner: DescriptorResourceOwner,
    texture_layout: Option<TextureLayout>,
    shader_use: Option<ReflectedShaderUse>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReflectedShaderUse {
    stage_flags: vk::ShaderStageFlags,
    access: DescriptorAccess,
}

impl ReflectedShaderUse {
    fn stage(self) -> PipelineStage {
        let supported = vk::ShaderStageFlags::VERTEX
            | vk::ShaderStageFlags::FRAGMENT
            | vk::ShaderStageFlags::COMPUTE;
        let unsupported = self.stage_flags & !supported;
        assert!(
            unsupported.is_empty(),
            "unsupported reflected descriptor shader stages: {unsupported:?}",
        );

        let mut stage = PipelineStage::empty();
        if self.stage_flags.contains(vk::ShaderStageFlags::VERTEX) {
            stage |= PipelineStage::VERTEX_SHADER;
        }
        if self.stage_flags.contains(vk::ShaderStageFlags::FRAGMENT) {
            stage |= PipelineStage::FRAGMENT_SHADER;
        }
        if self.stage_flags.contains(vk::ShaderStageFlags::COMPUTE) {
            stage |= PipelineStage::COMPUTE_SHADER;
        }
        assert!(
            stage != PipelineStage::empty(),
            "reflected descriptor has no shader stage",
        );
        stage
    }

    fn memory_access(self) -> MemoryAccess {
        match (self.access.readable(), self.access.writable()) {
            (true, false) => MemoryAccess::SHADER_READ,
            (false, true) => MemoryAccess::SHADER_WRITE,
            (true, true) => MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
            (false, false) => unreachable!("a reflected descriptor must have an access mode"),
        }
    }

    fn buffer_state(self) -> BufferState {
        BufferState::new(self.stage(), self.memory_access())
    }

    fn image_state(self, layout: TextureLayout) -> ResourceState {
        ResourceState::new(layout, self.stage(), self.memory_access())
    }
}

struct DescriptorSetInner {
    device: Device,
    descriptor_set: vk::DescriptorSet,
    pool: Option<DescriptorPool>,
    owners: Mutex<HashMap<(u32, u32), DescriptorBindingOwner>>,
}

#[derive(Clone)]
pub struct DescriptorSet(Arc<DescriptorSetInner>);

impl DescriptorSet {
    pub fn new(device: Device, descriptor_set: vk::DescriptorSet) -> Self {
        Self(Arc::new(DescriptorSetInner {
            device,
            descriptor_set,
            pool: None,
            owners: Mutex::new(HashMap::new()),
        }))
    }

    pub(crate) fn from_pool(
        device: Device,
        pool: DescriptorPool,
        descriptor_set: vk::DescriptorSet,
    ) -> Self {
        Self(Arc::new(DescriptorSetInner {
            device,
            descriptor_set,
            pool: Some(pool),
            owners: Mutex::new(HashMap::new()),
        }))
    }

    pub fn as_raw(&self) -> vk::DescriptorSet {
        self.0.descriptor_set
    }

    pub(crate) fn perform_writes(&self, writes: &mut [WriteDescriptorSet]) {
        if writes.is_empty() {
            return;
        }
        let raw_writes: Vec<_> = writes.iter_mut().map(|w| w.make_raw(self)).collect();
        unsafe { self.0.device.update_descriptor_sets(&raw_writes, &[]) }
        let mut owners = self.0.owners.lock().unwrap();
        for write in writes {
            if let Some(owner) = write.owner() {
                owners.insert((write.binding(), write.array_element()), owner);
            }
        }
    }

    /// Allocate a new descriptor set containing the same written resources as this set.
    pub fn fork(&self, pool: &DescriptorPool, layout: &DescriptorSetLayout) -> Result<Self> {
        let fork = pool.allocate_set(layout)?;
        let mut writes = self
            .0
            .owners
            .lock()
            .unwrap()
            .iter()
            .map(|(&(binding, array_element), owner)| {
                let mut write = match &owner.owner {
                    DescriptorResourceOwner::Buffer(buffer) => {
                        WriteDescriptorSet::new_buffer_write_for_type(
                            binding,
                            owner.descriptor_type,
                            buffer,
                        )
                    }
                    DescriptorResourceOwner::Texture(texture) => {
                        WriteDescriptorSet::new_texture_write(
                            binding,
                            owner.descriptor_type,
                            texture,
                            owner
                                .texture_layout
                                .unwrap_or(TextureLayout::SHADER_READ_ONLY),
                        )
                    }
                    DescriptorResourceOwner::AccelStruct(accel_struct) => {
                        WriteDescriptorSet::new_acceleration_structure_write(binding, accel_struct)
                    }
                };
                write.array_element = array_element;
                write.shader_use = owner.shader_use;
                write
            })
            .collect::<Vec<_>>();
        fork.perform_writes(&mut writes);
        Ok(fork)
    }

    /// Declare reflected shader uses for the resources owned by this descriptor generation.
    ///
    /// The descriptor set is the source of truth for both the Vulkan binding and its resource
    /// owner. Keeping the declaration here avoids a second pipeline-local resource inventory that
    /// could describe a different generation than the bound set.
    pub(crate) fn record_resource_uses(&self, cmdbuf: &CommandBuffer) {
        let owners = self.0.owners.lock().unwrap();
        for owner in owners.values() {
            let Some(shader_use) = owner.shader_use else {
                continue;
            };
            match &owner.owner {
                DescriptorResourceOwner::Buffer(buffer) => {
                    cmdbuf.record_buffer_state(buffer, shader_use.buffer_state());
                }
                DescriptorResourceOwner::Texture(texture) => {
                    if !descriptor_type_accesses_image(owner.descriptor_type) {
                        continue;
                    }
                    let image = texture.get_image();
                    cmdbuf.record_image_state(
                        image,
                        0,
                        image.get_desc().array_len,
                        shader_use.image_state(
                            owner
                                .texture_layout
                                .unwrap_or(TextureLayout::SHADER_READ_ONLY),
                        ),
                    );
                }
                DescriptorResourceOwner::AccelStruct(_) => {
                    // Acceleration-structure build/storage synchronization remains owned by the
                    // RTX module; the descriptor only owns the shader-side handle.
                }
            }
        }
    }

    pub(crate) fn resources_match<'a>(
        &self,
        resources: impl IntoIterator<Item = (u32, DescriptorResource<'a>)>,
    ) -> bool {
        let owners = self.0.owners.lock().unwrap();
        resources.into_iter().all(|(binding, resource)| {
            !descriptor_resource_requires_write(
                owners
                    .get(&(binding, 0))
                    .map(|owner| owner.owner.identity()),
                DescriptorResourceIdentity::from_resource(resource),
            )
        })
    }

    pub(crate) fn image_owner_count(&self) -> usize {
        self.0
            .owners
            .lock()
            .unwrap()
            .values()
            .filter(|owner| matches!(&owner.owner, DescriptorResourceOwner::Texture(_)))
            .count()
    }

    pub(crate) fn has_bindings(&self, binding_numbers: &[u32]) -> bool {
        let owners = self.0.owners.lock().unwrap();
        binding_numbers
            .iter()
            .all(|binding| owners.contains_key(&(*binding, 0)))
    }
}

fn descriptor_type_accesses_image(descriptor_type: vk::DescriptorType) -> bool {
    matches!(
        descriptor_type,
        vk::DescriptorType::STORAGE_IMAGE
            | vk::DescriptorType::COMBINED_IMAGE_SAMPLER
            | vk::DescriptorType::SAMPLED_IMAGE
            | vk::DescriptorType::INPUT_ATTACHMENT
    )
}

impl Drop for DescriptorSetInner {
    fn drop(&mut self) {
        if let Some(pool) = &self.pool {
            unsafe {
                self.device
                    .free_descriptor_sets(**pool, std::slice::from_ref(&self.descriptor_set))
                    .expect("failed to free descriptor set");
            }
        }
    }
}

pub(crate) struct WriteDescriptorSet<'a> {
    binding: u32,
    descriptor_type: vk::DescriptorType,
    array_element: u32,

    image_infos: Option<Vec<vk::DescriptorImageInfo>>,
    texture: Option<Texture>,
    texture_layout: Option<TextureLayout>,
    buffer: Option<Buffer>,
    buffer_infos: Option<Vec<vk::DescriptorBufferInfo>>,
    accel_struct_infos: Option<Vec<vk::WriteDescriptorSetAccelerationStructureKHR<'a>>>,
    accel_struct: Option<AccelStruct>,
    shader_use: Option<ReflectedShaderUse>,

    _accel_handles: Option<Vec<vk::AccelerationStructureKHR>>,
}

impl<'a> WriteDescriptorSet<'a> {
    pub(crate) fn new_texture_write(
        binding: u32,
        descriptor_type: vk::DescriptorType,
        texture: &Texture,
        image_layout: TextureLayout,
    ) -> Self {
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(image_layout.as_raw())
            .image_view(texture.get_image_view().as_raw())
            .sampler(texture.get_sampler().as_raw());

        Self {
            binding,
            descriptor_type,
            array_element: 0,
            image_infos: Some(vec![image_info]),
            texture: Some(texture.clone()),
            texture_layout: Some(image_layout),
            buffer: None,
            buffer_infos: None,
            accel_struct_infos: None,
            accel_struct: None,
            shader_use: None,
            _accel_handles: None,
        }
    }

    pub(crate) fn new_buffer_write_for_type(
        binding: u32,
        descriptor_type: vk::DescriptorType,
        buffer: &Buffer,
    ) -> Self {
        let buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(buffer.as_raw())
            .offset(0)
            .range(buffer.get_size_bytes());

        Self {
            binding,
            descriptor_type,
            array_element: 0,
            image_infos: None,
            texture: None,
            texture_layout: None,
            buffer: Some(buffer.clone()),
            buffer_infos: Some(vec![buffer_info]),
            accel_struct_infos: None,
            accel_struct: None,
            shader_use: None,
            _accel_handles: None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn new_acceleration_structure_write(binding: u32, tlas: &AccelStruct) -> Self {
        let handles = vec![tlas.as_raw()];
        let as_info = vk::WriteDescriptorSetAccelerationStructureKHR {
            acceleration_structure_count: handles.len() as u32,
            p_acceleration_structures: handles.as_ptr(),
            ..Default::default()
        };

        Self {
            binding,
            descriptor_type: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            array_element: 0,
            image_infos: None,
            texture: None,
            texture_layout: None,
            buffer: None,
            buffer_infos: None,
            accel_struct_infos: Some(vec![as_info]),
            accel_struct: Some(tlas.clone()),
            shader_use: None,
            _accel_handles: Some(handles),
        }
    }

    pub(crate) fn with_shader_use(
        mut self,
        stage_flags: vk::ShaderStageFlags,
        access: DescriptorAccess,
    ) -> Self {
        self.shader_use = Some(ReflectedShaderUse {
            stage_flags,
            access,
        });
        self
    }

    pub(crate) fn binding(&self) -> u32 {
        self.binding
    }

    pub(crate) fn array_element(&self) -> u32 {
        self.array_element
    }

    pub(crate) fn descriptor_type(&self) -> vk::DescriptorType {
        self.descriptor_type
    }

    fn owner(&self) -> Option<DescriptorBindingOwner> {
        let owner = self
            .texture
            .as_ref()
            .map(|texture| DescriptorResourceOwner::Texture(texture.clone()))
            .or_else(|| {
                self.buffer
                    .as_ref()
                    .map(|buffer| DescriptorResourceOwner::Buffer(buffer.clone()))
            })
            .or_else(|| {
                self.accel_struct
                    .clone()
                    .map(DescriptorResourceOwner::AccelStruct)
            })?;
        Some(DescriptorBindingOwner {
            descriptor_type: self.descriptor_type,
            owner,
            texture_layout: self.texture_layout,
            shader_use: self.shader_use,
        })
    }

    pub(crate) fn make_raw(
        &mut self,
        descriptor_set: &DescriptorSet,
    ) -> vk::WriteDescriptorSet<'_> {
        assert!(
            self.image_infos.is_some()
                ^ self.buffer_infos.is_some()
                ^ self.accel_struct_infos.is_some(),
            "A WriteDescriptorSet must contain exactly one of: image_infos, buffer_infos, or accel_struct_infos"
        );

        let mut write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set.as_raw())
            .dst_binding(self.binding)
            .dst_array_element(self.array_element)
            .descriptor_type(self.descriptor_type);

        if let Some(image_info) = &self.image_infos {
            write = write
                .image_info(image_info)
                .descriptor_count(image_info.len() as u32);
        }
        if let Some(buffer_info) = &self.buffer_infos {
            write = write
                .buffer_info(buffer_info)
                .descriptor_count(buffer_info.len() as u32);
        }

        if let Some(accel_infos) = self.accel_struct_infos.as_mut() {
            // compute count before taking a mutable reference for push_next
            let count = accel_infos.len() as u32;
            let accel_info_ptr = &mut accel_infos[0];
            write = write.push_next(accel_info_ptr).descriptor_count(count);
        }

        write
    }
}

#[cfg(test)]
mod tests {
    use super::{
        descriptor_resource_requires_write, descriptor_type_accesses_image,
        DescriptorResourceIdentity, ReflectedShaderUse,
    };
    use crate::{DescriptorAccess, MemoryAccess, PipelineStage, TextureLayout};
    use ash::vk;
    use ash::vk::Handle;

    #[test]
    fn reflected_shader_use_preserves_exact_stage_and_access() {
        let read = ReflectedShaderUse {
            stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            access: DescriptorAccess::ReadOnly,
        };
        assert_eq!(
            read.buffer_state(),
            crate::BufferState::new(
                PipelineStage::VERTEX_SHADER | PipelineStage::FRAGMENT_SHADER,
                MemoryAccess::SHADER_READ,
            )
        );
        assert_eq!(
            read.image_state(TextureLayout::GENERAL),
            crate::ResourceState::new(
                TextureLayout::GENERAL,
                PipelineStage::VERTEX_SHADER | PipelineStage::FRAGMENT_SHADER,
                MemoryAccess::SHADER_READ,
            )
        );
    }

    #[test]
    fn only_image_descriptors_plan_image_state() {
        assert!(descriptor_type_accesses_image(
            vk::DescriptorType::STORAGE_IMAGE
        ));
        assert!(descriptor_type_accesses_image(
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER
        ));
        assert!(!descriptor_type_accesses_image(
            vk::DescriptorType::STORAGE_BUFFER
        ));
    }

    #[test]
    fn identical_descriptor_resource_identities_skip_writes() {
        let identities = [
            DescriptorResourceIdentity::Buffer(vk::Buffer::from_raw(11)),
            DescriptorResourceIdentity::Texture {
                image_view: vk::ImageView::from_raw(12),
                sampler: vk::Sampler::from_raw(13),
            },
            DescriptorResourceIdentity::AccelerationStructure(
                vk::AccelerationStructureKHR::from_raw(14),
            ),
        ];

        for identity in identities {
            assert!(!descriptor_resource_requires_write(
                Some(identity),
                identity
            ));
        }
    }

    #[test]
    fn changed_or_missing_descriptor_resources_force_writes() {
        let buffer = DescriptorResourceIdentity::Buffer(vk::Buffer::from_raw(21));
        assert!(descriptor_resource_requires_write(None, buffer));
        assert!(descriptor_resource_requires_write(
            Some(buffer),
            DescriptorResourceIdentity::Buffer(vk::Buffer::from_raw(22)),
        ));
        assert!(descriptor_resource_requires_write(
            Some(buffer),
            DescriptorResourceIdentity::AccelerationStructure(
                vk::AccelerationStructureKHR::from_raw(21),
            ),
        ));

        let texture = DescriptorResourceIdentity::Texture {
            image_view: vk::ImageView::from_raw(23),
            sampler: vk::Sampler::from_raw(24),
        };
        assert!(descriptor_resource_requires_write(
            Some(texture),
            DescriptorResourceIdentity::Texture {
                image_view: vk::ImageView::from_raw(25),
                sampler: vk::Sampler::from_raw(24),
            },
        ));
        assert!(descriptor_resource_requires_write(
            Some(texture),
            DescriptorResourceIdentity::Texture {
                image_view: vk::ImageView::from_raw(23),
                sampler: vk::Sampler::from_raw(26),
            },
        ));

        let acceleration_structure = DescriptorResourceIdentity::AccelerationStructure(
            vk::AccelerationStructureKHR::from_raw(27),
        );
        assert!(descriptor_resource_requires_write(
            Some(acceleration_structure),
            DescriptorResourceIdentity::AccelerationStructure(
                vk::AccelerationStructureKHR::from_raw(28),
            ),
        ));
    }
}
