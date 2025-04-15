use spirv_reflect::types::ReflectDescriptorType;
use std::collections::HashMap;

/// Represents the layout of a uniform buffer or push constant block.
#[derive(Debug, Clone)]
pub struct StructLayout {
    pub type_name: String,
    pub total_size: u32,

    /// The names of the members are guaranteed to be unique within a buffer layout.
    pub members: HashMap<String, StructMember>,

    pub descriptor_type: ReflectDescriptorType,
}

impl StructLayout {
    /// Retrieve a member by name.
    pub fn get_member(&self, name: &str) -> Option<&StructMember> {
        self.members.get(name)
    }

    pub fn get_size(&self) -> u32 {
        self.total_size
    }
}

/// Represents a single member (field) within a uniform buffer or push constant block.
#[derive(Debug, Clone)]
pub struct StructMember {
    #[allow(dead_code)]
    pub name: String,
    pub type_name: String,
    pub offset: u32,
    #[allow(dead_code)]
    pub size: u32,
    pub padded_size: u32,
}
