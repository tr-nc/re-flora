use crate::Device;
use anyhow::Result;
use ash::vk;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DescriptorAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl DescriptorAccess {
    pub(crate) fn readable(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite)
    }

    pub(crate) fn writable(self) -> bool {
        matches!(self, Self::WriteOnly | Self::ReadWrite)
    }

    fn merged_with(self, other: Self) -> Self {
        match (
            self.readable() || other.readable(),
            self.writable() || other.writable(),
        ) {
            (true, false) => Self::ReadOnly,
            (false, true) => Self::WriteOnly,
            (true, true) => Self::ReadWrite,
            (false, false) => unreachable!("a descriptor must be readable, writable, or both"),
        }
    }
}

#[derive(Debug)]
struct DescriptorSetLayoutInner {
    device: Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
    bindings: HashMap<u32, DescriptorSetLayoutBinding>,
}

impl Drop for DescriptorSetLayoutInner {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

#[derive(Clone, Debug)]
pub struct DescriptorSetLayout(Arc<DescriptorSetLayoutInner>);

impl std::ops::Deref for DescriptorSetLayout {
    type Target = vk::DescriptorSetLayout;
    fn deref(&self) -> &Self::Target {
        &self.0.descriptor_set_layout
    }
}

impl DescriptorSetLayout {
    /// Use the builder pattern to create a new DescriptorSetLayout
    fn new(device: &Device, bindings: &HashMap<u32, DescriptorSetLayoutBinding>) -> Result<Self> {
        let raw_bindings = bindings.iter().map(|b| b.1.as_raw()).collect::<Vec<_>>();
        let descriptor_set_create_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&raw_bindings);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&descriptor_set_create_info, None)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?
        };

        Ok(Self(Arc::new(DescriptorSetLayoutInner {
            device: device.clone(),
            descriptor_set_layout,
            bindings: bindings.clone(),
        })))
    }

    pub fn as_raw(&self) -> vk::DescriptorSetLayout {
        self.0.descriptor_set_layout
    }

    pub fn get_bindings(&self) -> Vec<DescriptorSetLayoutBinding> {
        self.0.bindings.values().cloned().collect::<Vec<_>>()
    }

    pub fn merge(&self, other: &DescriptorSetLayout) -> Result<Self> {
        if self.0.device != other.0.device {
            return Err(anyhow::anyhow!(
                "Cannot merge DescriptorSetLayouts from different devices"
            ));
        }

        let merged_bindings = merge_descriptor_bindings(&self.0.bindings, &other.0.bindings)?;

        Self::new(&self.0.device, &merged_bindings)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DescriptorSetLayoutBinding {
    pub no: u32,
    pub name: String,
    pub descriptor_type: vk::DescriptorType,
    pub descriptor_count: u32,
    pub stage_flags: vk::ShaderStageFlags,
    pub(crate) access: DescriptorAccess,
}

impl DescriptorSetLayoutBinding {
    fn as_raw(&self) -> vk::DescriptorSetLayoutBinding<'_> {
        vk::DescriptorSetLayoutBinding::default()
            .binding(self.no)
            .descriptor_type(self.descriptor_type)
            .descriptor_count(self.descriptor_count)
            .stage_flags(self.stage_flags)
    }

    pub(crate) fn merged_with(&self, other: &Self) -> Result<Self> {
        anyhow::ensure!(
            self.no == other.no
                && self.name == other.name
                && self.descriptor_type == other.descriptor_type
                && self.descriptor_count == other.descriptor_count,
            "incompatible reflected descriptor declarations at binding {}: {:?} != {:?}",
            self.no,
            self,
            other,
        );
        let mut merged = self.clone();
        merged.stage_flags |= other.stage_flags;
        merged.access = self.access.merged_with(other.access);
        Ok(merged)
    }
}

pub(crate) fn merge_descriptor_bindings(
    left: &HashMap<u32, DescriptorSetLayoutBinding>,
    right: &HashMap<u32, DescriptorSetLayoutBinding>,
) -> Result<HashMap<u32, DescriptorSetLayoutBinding>> {
    let mut merged = left.clone();
    for (&binding_no, right_binding) in right {
        match merged.get(&binding_no) {
            Some(left_binding) => {
                merged.insert(binding_no, left_binding.merged_with(right_binding)?);
            }
            None => {
                merged.insert(binding_no, right_binding.clone());
            }
        }
    }
    Ok(merged)
}

pub struct DescriptorSetLayoutBuilder {
    bindings: HashMap<u32, DescriptorSetLayoutBinding>,
}

impl DescriptorSetLayoutBuilder {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn set_bindings(
        &mut self,
        bindings: HashMap<u32, DescriptorSetLayoutBinding>,
    ) -> &mut Self {
        self.bindings = bindings;
        self
    }

    pub fn build(self, device: &Device) -> Result<DescriptorSetLayout> {
        DescriptorSetLayout::new(device, &self.bindings.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{DescriptorAccess, DescriptorSetLayoutBinding};
    use ash::vk;

    #[test]
    fn merging_stage_declarations_preserves_union_of_access() {
        let vertex_read = DescriptorSetLayoutBinding {
            no: 3,
            name: "shared".to_owned(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::VERTEX,
            access: DescriptorAccess::ReadOnly,
        };
        let fragment_write = DescriptorSetLayoutBinding {
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
            access: DescriptorAccess::WriteOnly,
            ..vertex_read.clone()
        };

        let merged = vertex_read.merged_with(&fragment_write).unwrap();
        assert_eq!(
            merged.stage_flags,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT
        );
        assert_eq!(merged.access, DescriptorAccess::ReadWrite);
    }
}
