use spirv_reflect::types::ReflectDescriptorType;
use std::collections::HashMap;

// /// Represents the layout of a uniform buffer or push constant block.
// #[derive(Debug, Clone)]
// pub struct StructLayout {
//     pub type_name: String,
//     pub total_size: u32,

//     /// The names of the members are guaranteed to be unique within a buffer layout.
//     pub members: HashMap<String, StructMember>,
//     pub descriptor_type: ReflectDescriptorType,
// }

// impl StructLayout {
//     /// Retrieve a member by name.
//     pub fn get_member(&self, name: &str) -> Option<&StructMember> {
//         self.members.get(name)
//     }

//     pub fn get_size(&self) -> u32 {
//         self.total_size
//     }
// }

#[derive(Debug, Clone, PartialEq)]
pub enum GeneralMemberType {
    Plain,
    Struct,
    Array,
}

pub enum Member {
    Plain(PlainMember),
    Struct(StructMember),
}

pub struct BufferLayout {
    pub ty: String, // typically in the form of U_<uniform_name> or B_<storage_buffer_name>
    pub members: HashMap<String, Member>, // member name - the member
    pub descriptor_type: ReflectDescriptorType,
}

impl BufferLayout {
    pub fn get_size(&self) -> u32 {
        let mut size = 0;
        for member in self.members.values() {
            match member {
                Member::Plain(plain_member) => size += plain_member.padded_size,
                Member::Struct(struct_member) => size += struct_member.get_size(),
            }
        }
        size
    }
}

pub enum PlainMemberType {
    Int,
    UInt,
    Float,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    Mat2,
    Mat3,
    Mat4,
}

pub struct PlainMember {
    pub ty: PlainMemberType,
    pub offset: u32,
    pub size: u32,
    pub padded_size: u32,
    pub data: Vec<u8>, // raw data of the member
}

pub struct StructMember {
    pub ty: String,                       // name of the struct's type
    pub members: HashMap<String, Member>, // member name - the member
}

impl StructMember {
    pub fn get_size(&self) -> u32 {
        let mut size = 0;
        for member in self.members.values() {
            match member {
                Member::Plain(plain_member) => size += plain_member.padded_size,
                Member::Struct(struct_member) => size += struct_member.get_size(),
            }
        }
        size
    }
}
