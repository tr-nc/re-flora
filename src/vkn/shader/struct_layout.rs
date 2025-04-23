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
    pub fn get_size(&self) -> u32 {
        self.root_member.get_size()
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

#[derive(Debug, Clone)]
pub enum PlainMemberTypeWithData {
    Int(i32),
    UInt(u32),
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    Mat2([[f32; 2]; 2]),
    Mat3([[f32; 3]; 3]),
    Mat4([[f32; 4]; 4]),
}

impl PlainMemberTypeWithData {
    pub fn has_type(&self, ty: &PlainMemberType) -> bool {
        match (self, ty) {
            (PlainMemberTypeWithData::Int(_), PlainMemberType::Int) => true,
            (PlainMemberTypeWithData::UInt(_), PlainMemberType::UInt) => true,
            (PlainMemberTypeWithData::Float(_), PlainMemberType::Float) => true,
            (PlainMemberTypeWithData::Vec2(_), PlainMemberType::Vec2) => true,
            (PlainMemberTypeWithData::Vec3(_), PlainMemberType::Vec3) => true,
            (PlainMemberTypeWithData::Vec4(_), PlainMemberType::Vec4) => true,
            (PlainMemberTypeWithData::IVec2(_), PlainMemberType::IVec2) => true,
            (PlainMemberTypeWithData::IVec3(_), PlainMemberType::IVec3) => true,
            (PlainMemberTypeWithData::IVec4(_), PlainMemberType::IVec4) => true,
            (PlainMemberTypeWithData::UVec2(_), PlainMemberType::UVec2) => true,
            (PlainMemberTypeWithData::UVec3(_), PlainMemberType::UVec3) => true,
            (PlainMemberTypeWithData::UVec4(_), PlainMemberType::UVec4) => true,
            (PlainMemberTypeWithData::Mat2(_), PlainMemberType::Mat2) => true,
            (PlainMemberTypeWithData::Mat3(_), PlainMemberType::Mat3) => true,
            (PlainMemberTypeWithData::Mat4(_), PlainMemberType::Mat4) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlainMemberLayout {
    pub name: String,
    pub ty: PlainMemberType,
    pub offset: u32,
    pub size: u32,
    pub padded_size: u32,
}

#[derive(Debug, Clone)]
pub struct StructMemberLayout {
    pub name: String,
    pub ty: String,
    pub name_member_table: HashMap<String, MemberLayout>,
}

impl StructMemberLayout {
    pub fn get_size(&self) -> u32 {
        let mut size = 0;
        for member in self.name_member_table.values() {
            match member {
                MemberLayout::Plain(plain_member) => size += plain_member.padded_size,
                MemberLayout::Struct(struct_member) => size += struct_member.get_size(),
            }
        }
        size
    }

    pub fn get_member(&self, name: &str) -> Option<&MemberLayout> {
        self.name_member_table.get(name)
    }
}
