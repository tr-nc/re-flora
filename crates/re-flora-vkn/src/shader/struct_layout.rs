#![allow(dead_code)]

use spirv_reflect::types::ReflectDescriptorType;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum GeneralMemberType {
    Plain,
    Struct,
    Array,
}

#[derive(Debug, Clone)]
pub struct BufferLayout {
    pub root_member: StructMemberLayout,
    pub descriptor_type: ReflectDescriptorType,
}

impl BufferLayout {
    pub fn get_size_bytes(&self) -> u64 {
        self.root_member.get_size_bytes()
    }

    pub fn get_member(&self, name: &str) -> Option<&MemberLayout> {
        self.root_member.get_member(name)
    }
}

#[derive(Debug, Clone)]
pub enum MemberLayout {
    Plain(PlainMemberLayout),
    Struct(StructMemberLayout),
}

impl MemberLayout {
    pub fn get_member(&self, name: &str) -> Result<&MemberLayout, String> {
        match self {
            MemberLayout::Plain(_) => Err(format!("Member {} is not a struct", name)),
            MemberLayout::Struct(struct_member) => struct_member
                .get_member(name)
                .ok_or_else(|| format!("Member {} not found", name)),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum PlainMemberType {
    Int,
    UInt,
    Int64,
    UInt64,
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
    Mat3x4,
    Array, // TODO: maybe remove this later
}

#[derive(Debug, Clone)]
pub struct PlainMemberLayout {
    pub name: String,
    pub ty: PlainMemberType,
    pub offset: u64,
    pub size: u64,
    pub padded_size: u64,
}

#[derive(Debug, Clone)]
pub struct StructMemberLayout {
    pub name: String,
    pub ty: String,
    pub name_member_table: HashMap<String, MemberLayout>,
}

impl StructMemberLayout {
    fn get_last_offset_with_size_info(&self, last_offset: &mut u64, size: &mut u64) {
        for member in self.name_member_table.values() {
            match member {
                MemberLayout::Plain(plain_member) => {
                    let this_offset = plain_member.offset;
                    let this_size = plain_member.padded_size;
                    if *last_offset <= this_offset {
                        *last_offset = this_offset;
                        *size = *last_offset + this_size;
                    }
                }
                MemberLayout::Struct(struct_member) => {
                    struct_member.get_last_offset_with_size_info(last_offset, size);
                }
            }
        }
    }

    pub fn get_size_bytes(&self) -> u64 {
        let mut last_offset = 0;
        let mut size = 0;
        self.get_last_offset_with_size_info(&mut last_offset, &mut size);
        size
    }

    pub fn get_member(&self, name: &str) -> Option<&MemberLayout> {
        self.name_member_table.get(name)
    }
}
